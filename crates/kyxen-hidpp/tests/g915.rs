//! Behaviour against a simulated G915 (feature indices, 255-byte sectors and
//! ROM sectors without CRC as measured on real hardware).

use kyxen_hidpp::error::{Error, ErrorCode};
use kyxen_hidpp::mock::MockKeyboard;
use kyxen_hidpp::onboard::{self, F13_F17, GKEY_BLOCK_OFFSET, Mode, OnboardProfiles};
use kyxen_hidpp::{FeatureId, HidppDevice};

const F1_F5: [u8; 5] = [0x3a, 0x3b, 0x3c, 0x3d, 0x3e];
const DIGITS: [u8; 5] = [0x1e, 0x1f, 0x20, 0x21, 0x22];

#[derive(Clone, Copy)]
enum Crc {
    Valid,
    Bad,
    /// G915 ROM sectors end in `ff ff`.
    None,
}

fn sector(gkeys: [u8; 5], crc: Crc) -> Vec<u8> {
    let mut d = vec![0u8; 255];
    d[0] = 0x42;
    for (i, k) in gkeys.iter().enumerate() {
        d[GKEY_BLOCK_OFFSET + i * 4..][..4].copy_from_slice(&[0x80, 0x02, 0x00, *k]);
    }
    onboard::apply_crc(&mut d);
    match crc {
        Crc::Valid => {}
        Crc::Bad => d[254] ^= 0xff,
        Crc::None => d[253..].copy_from_slice(&[0xff, 0xff]),
    }
    d
}

fn g915_with(sectors: &[(u16, Vec<u8>)]) -> HidppDevice<MockKeyboard> {
    let mut kb = MockKeyboard::g915();
    kb.sectors.extend(sectors.iter().cloned());
    HidppDevice::new(kb)
}

#[test]
fn resolves_g915_feature_indices() {
    let mut dev = g915_with(&[]);
    assert_eq!(
        dev.feature_index(FeatureId::PER_KEY_LIGHTING_V2).unwrap(),
        0x0a
    );
    assert_eq!(
        dev.feature_index(FeatureId::ONBOARD_PROFILES).unwrap(),
        0x14
    );
    assert_eq!(dev.feature_index(FeatureId::GKEY).unwrap(), 0x10);
    assert!(matches!(
        dev.feature_index(FeatureId(0x1234)),
        Err(Error::Unsupported(0x1234))
    ));
    let lookups = dev.transport_mut().sent.len();
    dev.feature_index(FeatureId::GKEY).unwrap();
    assert_eq!(
        dev.transport_mut().sent.len(),
        lookups,
        "second lookup must hit the cache"
    );
}

#[test]
fn reads_feature_table() {
    let mut dev = g915_with(&[]);
    let table = dev.feature_table().unwrap();
    assert!(table.contains(&(0x14, FeatureId::ONBOARD_PROFILES)));
}

#[test]
fn sector_size_comes_from_description() {
    let mut dev = g915_with(&[]);
    assert_eq!(OnboardProfiles::open(&mut dev).unwrap().sector_size, 255);
}

#[test]
fn reads_255_byte_sector_without_reading_past_the_end() {
    let data: Vec<u8> = (0..255).map(|i| i as u8).collect();
    let mut dev = g915_with(&[(0x0001, data.clone())]);
    let op = OnboardProfiles::open(&mut dev).unwrap();
    assert_eq!(op.read_sector(&mut dev, 0x0001).unwrap(), data);
    let last = dev.transport_mut().sent.last().unwrap().clone();
    assert_eq!(u16::from_be_bytes([last[6], last[7]]), 255 - 16);
}

#[test]
fn hidpp_errors_are_errors_not_data() {
    let mut dev = g915_with(&[]);
    let op = OnboardProfiles::open(&mut dev).unwrap();
    let err = op.read_sector(&mut dev, 0x0042).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Hidpp {
                code: ErrorCode::InvalidArgument,
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn notifications_during_a_request_are_kept() {
    let mut dev = g915_with(&[]);
    // G1 pressed while we look up a feature
    dev.transport_mut().push(vec![
        0x11, 0xff, 0x10, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ]);
    dev.feature_index(FeatureId::GKEY).unwrap();
    let n = dev
        .next_notification(std::time::Duration::ZERO)
        .unwrap()
        .expect("buffered notification");
    assert_eq!((n.feature, n.payload[0]), (0x10, 0x01));
}

#[test]
fn remap_patches_valid_user_sectors_in_place() {
    let mut user1 = sector(F1_F5, Crc::Valid);
    user1[1] = 0x99; // a setting made with G Hub
    onboard::apply_crc(&mut user1);
    let mut dev = g915_with(&[
        (0x0001, user1),
        (0x0002, sector(DIGITS, Crc::Bad)), // broken → rebuilt from ROM
        (0x0003, sector(DIGITS, Crc::Valid)),
        (0x0102, sector(DIGITS, Crc::None)),
    ]);
    let op = OnboardProfiles::open(&mut dev).unwrap();
    assert!(op.ensure_gkey_remap(&mut dev).unwrap());

    let writes = &dev.transport_mut().writes;
    assert_eq!(
        writes.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(writes[0].1[1], 0x99, "user setting kept");
    for (_, data) in writes {
        assert_eq!(data.len(), 255);
        assert_eq!(onboard::gkey_usages(data), F13_F17);
        assert!(onboard::crc_valid(data));
    }
}

#[test]
fn no_flash_write_when_any_source_is_unusable() {
    let mut dev = g915_with(&[
        (0x0001, sector(F1_F5, Crc::Valid)),
        (0x0002, sector(DIGITS, Crc::Valid)),
        (0x0003, sector(DIGITS, Crc::Bad)),
        (0x0103, sector(DIGITS, Crc::Bad)),
    ]);
    let op = OnboardProfiles::open(&mut dev).unwrap();
    assert!(matches!(
        op.ensure_gkey_remap(&mut dev),
        Err(Error::Refused(_))
    ));
    assert!(dev.transport_mut().writes.is_empty());
}

#[test]
fn already_remapped_is_left_alone() {
    let s = sector(F13_F17, Crc::Valid);
    let mut dev = g915_with(&[(1, s.clone()), (2, s.clone()), (3, s)]);
    let op = OnboardProfiles::open(&mut dev).unwrap();
    assert!(!op.ensure_gkey_remap(&mut dev).unwrap());
    assert!(dev.transport_mut().writes.is_empty());
}

#[test]
fn source_check_rejects_non_keyboard_bindings() {
    let mut s = sector(F1_F5, Crc::Valid);
    s[GKEY_BLOCK_OFFSET + 1] = 0x01; // mouse binding
    onboard::apply_crc(&mut s);
    assert!(onboard::check_source(1, &s, false).is_err());
    assert!(onboard::check_source(0x102, &sector(DIGITS, Crc::None), true).is_ok());
    assert!(onboard::check_source(2, &sector(DIGITS, Crc::None), false).is_err());
}

#[test]
fn restore_leaves_host_mode() {
    let mut dev = g915_with(&[]);
    let op = OnboardProfiles::open(&mut dev).unwrap();
    op.set_mode(&mut dev, Mode::Host).unwrap();
    assert_eq!(op.current_profile(&mut dev).unwrap(), 0);
    assert!(
        op.set_current_profile(&mut dev, 1).is_err(),
        "profile switch needs onboard mode"
    );

    op.restore(&mut dev, 2).unwrap();
    assert_eq!(op.mode(&mut dev).unwrap(), Mode::Onboard);
    assert_eq!(op.current_profile(&mut dev).unwrap(), 2);
}
