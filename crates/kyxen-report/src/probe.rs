//! Read-only HID++ queries.
//!
//! Every request here is a documented getter. Nothing is written to the
//! keyboard: no setters, no flash access, no mode changes. [`REQUESTS`] lists
//! them for the report.

use std::time::Duration;

use kyxen_hidpp::report::ReportKind;
use kyxen_hidpp::{Error, ErrorCode, FeatureId, HidppDevice, Transport};

/// Software id for our requests; the daemon uses another one, so a running
/// `kyxend` ignores our answers.
pub const SW_ID: u8 = 0x0d;

/// Every request this program may send, for the report.
pub const REQUESTS: &[&str] = &[
    "ROOT (0x0000) getFeature, getProtocolVersion (ping)",
    "FEATURE_SET (0x0001) getCount, getFeatureID",
    "DEVICE_NAME (0x0005) getDeviceNameCount, getDeviceName, getDeviceType",
    "FW_VERSION (0x0003) getDeviceInfo, getFwInfo",
    "KEYBOARD_LAYOUT_2 (0x4540) getKeyboardLayout",
    "GKEY (0x8010) getCount",
    "MKEYS (0x8020) getCount",
    "ONBOARD_PROFILES (0x8100) getDescription, getMode, getCurrentProfile",
];

const FW_VERSION: FeatureId = FeatureId(0x0003);
const KEYBOARD_LAYOUT_2: FeatureId = FeatureId(0x4540);

/// One feature table entry.
#[derive(Debug, Clone)]
pub struct Feature {
    pub index: u8,
    pub id: u16,
    pub version: u8,
    pub flags: u8,
}

/// One firmware entity (application, bootloader, hardware, …).
#[derive(Debug, Clone)]
pub struct Firmware {
    pub kind: u8,
    pub prefix: String,
    pub number: u8,
    pub revision: u8,
    pub build: u16,
}

/// What the onboard-profiles feature says about itself.
#[derive(Debug, Clone)]
pub struct Onboard {
    pub description: Vec<u8>,
    pub mode: Option<u8>,
    pub current_profile: Option<u8>,
}

/// Everything learned from one HID++ device index.
#[derive(Debug, Clone, Default)]
pub struct HidppInfo {
    pub device_index: u8,
    /// `(major, minor)`; `None` for a HID++ 1.0 device (a receiver).
    pub protocol: Option<(u8, u8)>,
    pub name: Option<String>,
    pub device_type: Option<u8>,
    pub model_ids: Option<Vec<u8>>,
    pub firmware: Vec<Firmware>,
    pub features: Vec<Feature>,
    pub keyboard_layout: Option<u8>,
    pub gkeys: Option<u8>,
    pub mkeys: Option<u8>,
    pub onboard: Option<Onboard>,
    /// Queries that failed, as `what: error`.
    pub errors: Vec<String>,
}

/// Result of pinging one device index.
#[derive(Debug)]
pub enum Ping {
    /// HID++ 2.0 or later.
    Hidpp20(u8, u8),
    /// Answers with a HID++ 1.0 error: a receiver (or an old device).
    Hidpp10,
    /// Nothing there (timeout, or "unknown device" from a receiver).
    Absent,
}

pub fn ping<T: Transport>(dev: &mut HidppDevice<T>) -> Ping {
    match dev.request_with(ReportKind::Short, 0x00, 1, &[0, 0, 0x5a]) {
        Ok(r) => Ping::Hidpp20(r[0], r[1]),
        // HID++ 1.0 codes 0x08 (unknown device) / 0x09 (resource error) from a
        // receiver: no device in that slot. They map onto these 2.0 names.
        Err(Error::Hidpp {
            code: ErrorCode::Busy | ErrorCode::Unsupported,
            ..
        }) => Ping::Absent,
        Err(Error::Hidpp { .. }) => Ping::Hidpp10,
        Err(_) => Ping::Absent,
    }
}

/// Query a HID++ 2.0 device at its current device index.
pub fn query<T: Transport>(
    dev: &mut HidppDevice<T>,
    device_index: u8,
    protocol: (u8, u8),
) -> HidppInfo {
    let mut info = HidppInfo {
        device_index,
        protocol: Some(protocol),
        ..HidppInfo::default()
    };
    let note = |what: &str, e: &Error| info_err(what, e);
    let mut errors = Vec::new();

    match feature_table(dev) {
        Ok(f) => info.features = f,
        Err(e) => errors.push(note("feature table", &e)),
    }
    let has = |id: FeatureId| info.features.iter().any(|f| f.id == id.0);

    if has(FeatureId::DEVICE_NAME) {
        match device_name(dev) {
            Ok((name, kind)) => {
                info.name = Some(name);
                info.device_type = kind;
            }
            Err(e) => errors.push(note("device name", &e)),
        }
    }
    if has(FW_VERSION) {
        match firmware(dev) {
            Ok((models, fw)) => {
                info.model_ids = Some(models);
                info.firmware = fw;
            }
            Err(e) => errors.push(note("firmware", &e)),
        }
    }
    if has(KEYBOARD_LAYOUT_2) {
        match get_byte(dev, KEYBOARD_LAYOUT_2, 0, 0) {
            Ok(v) => info.keyboard_layout = Some(v),
            Err(e) => errors.push(note("keyboard layout", &e)),
        }
    }
    if has(FeatureId::GKEY) {
        match get_byte(dev, FeatureId::GKEY, 0, 0) {
            Ok(v) => info.gkeys = Some(v),
            Err(e) => errors.push(note("G-key count", &e)),
        }
    }
    if has(FeatureId::MKEYS) {
        match get_byte(dev, FeatureId::MKEYS, 0, 0) {
            Ok(v) => info.mkeys = Some(v),
            Err(e) => errors.push(note("M-key count", &e)),
        }
    }
    if has(FeatureId::ONBOARD_PROFILES) {
        match onboard(dev) {
            Ok(o) => info.onboard = Some(o),
            Err(e) => errors.push(note("onboard profiles", &e)),
        }
    }
    info.errors = errors;
    info
}

fn info_err(what: &str, e: &Error) -> String {
    format!("{what}: {e}")
}

fn feature_table<T: Transport>(dev: &mut HidppDevice<T>) -> Result<Vec<Feature>, Error> {
    let fs = dev.feature_index(FeatureId::FEATURE_SET)?;
    let count = dev.request(fs, 0, &[])?[0];
    let mut out = vec![Feature {
        index: 0,
        id: 0,
        version: 0,
        flags: 0,
    }];
    for index in 1..=count {
        let r = dev.request(fs, 1, &[index])?;
        out.push(Feature {
            index,
            id: u16::from_be_bytes([r[0], r[1]]),
            flags: r[2],
            version: r[3],
        });
    }
    Ok(out)
}

fn device_name<T: Transport>(dev: &mut HidppDevice<T>) -> Result<(String, Option<u8>), Error> {
    let idx = dev.feature_index(FeatureId::DEVICE_NAME)?;
    let len = usize::from(dev.request(idx, 0, &[])?[0]);
    let mut name = Vec::with_capacity(len);
    while name.len() < len {
        let chunk = dev.request(idx, 1, &[name.len() as u8])?;
        let take = (len - name.len()).min(chunk.len());
        if take == 0 {
            break;
        }
        name.extend_from_slice(&chunk[..take]);
    }
    let kind = dev.request(idx, 2, &[]).ok().map(|r| r[0]);
    let name = String::from_utf8_lossy(&name)
        .trim_end_matches('\0')
        .to_owned();
    Ok((name, kind))
}

/// Model ids and firmware entities. The unit id (bytes 1–4 of
/// getDeviceInfo) identifies the individual device and is left out.
fn firmware<T: Transport>(dev: &mut HidppDevice<T>) -> Result<(Vec<u8>, Vec<Firmware>), Error> {
    let idx = dev.feature_index(FW_VERSION)?;
    let info = dev.request(idx, 0, &[])?;
    let entities = info[0];
    let models = info.get(7..13).map(<[u8]>::to_vec).unwrap_or_default();
    let mut fw = Vec::new();
    for e in 0..entities {
        let r = dev.request(idx, 1, &[e])?;
        fw.push(Firmware {
            kind: r[0],
            prefix: String::from_utf8_lossy(&r[1..4])
                .trim_matches(|c: char| c == '\0' || c.is_whitespace())
                .to_owned(),
            number: r[4],
            revision: r[5],
            build: u16::from_be_bytes([r[6], r[7]]),
        });
    }
    Ok((models, fw))
}

fn get_byte<T: Transport>(
    dev: &mut HidppDevice<T>,
    id: FeatureId,
    function: u8,
    byte: usize,
) -> Result<u8, Error> {
    let idx = dev.feature_index(id)?;
    Ok(dev.request(idx, function, &[])?[byte])
}

fn onboard<T: Transport>(dev: &mut HidppDevice<T>) -> Result<Onboard, Error> {
    let idx = dev.feature_index(FeatureId::ONBOARD_PROFILES)?;
    let description = dev.request(idx, 0, &[])?;
    Ok(Onboard {
        description,
        mode: dev.request(idx, 2, &[]).ok().map(|r| r[0]),
        current_profile: dev.request(idx, 4, &[]).ok().map(|r| r[1]),
    })
}

/// Probe one hidraw node: the device at `0xff`, and if that is a receiver,
/// the devices paired to it (indices 1–6).
pub fn probe_node<T: Transport>(mut make: impl FnMut() -> std::io::Result<T>) -> Vec<HidppInfo> {
    let mut out = Vec::new();
    let Ok(t) = make() else { return out };
    let mut dev = HidppDevice::new(t)
        .with_sw_id(SW_ID)
        .with_timeout(Duration::from_millis(800));
    match ping(&mut dev) {
        Ping::Hidpp20(major, minor) => out.push(query(&mut dev, 0xff, (major, minor))),
        Ping::Hidpp10 => {
            out.push(HidppInfo {
                device_index: 0xff,
                ..HidppInfo::default()
            });
            drop(dev);
            for index in 1..=6u8 {
                let Ok(t) = make() else { break };
                let mut child = HidppDevice::new(t)
                    .with_device(index)
                    .with_sw_id(SW_ID)
                    .with_timeout(Duration::from_millis(500));
                if let Ping::Hidpp20(major, minor) = ping(&mut child) {
                    child = child.with_timeout(Duration::from_secs(2));
                    out.push(query(&mut child, index, (major, minor)));
                }
            }
        }
        Ping::Absent => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use kyxen_hidpp::mock::MockKeyboard;

    use super::*;

    #[test]
    fn reads_the_mock_g915() {
        let infos = probe_node(|| Ok(MockKeyboard::g915()));
        assert_eq!(infos.len(), 1);
        let i = &infos[0];
        assert_eq!(i.device_index, 0xff);
        assert!(i.features.iter().any(|f| f.id == 0x8100));
        let o = i.onboard.as_ref().expect("onboard profiles");
        assert_eq!(
            u16::from_be_bytes([o.description[7], o.description[8]]),
            255
        );
    }

    #[test]
    fn sends_only_getters() {
        let mut dev = HidppDevice::new(MockKeyboard::g915()).with_sw_id(SW_ID);
        let _ = query(&mut dev, 0xff, (4, 2));
        let sent = &dev.transport_mut().sent;
        assert!(!sent.is_empty());
        for r in sent {
            let (feature, function) = (r[2], r[3] >> 4);
            assert_eq!(r[3] & 0x0f, SW_ID);
            let allowed: &[u8] = match feature {
                // ROOT getFeature + ping, FEATURE_SET getCount + getFeatureID
                0x00 | 0x01 => &[0, 1],
                0x03 => &[0, 1, 2],  // DEVICE_NAME getters
                0x10 | 0x11 => &[0], // GKEY / MKEYS getCount
                0x14 => &[0, 2, 4],  // ONBOARD getDescription, getMode, getCurrentProfile
                other => panic!("request to feature index {other:#04x}"),
            };
            assert!(
                allowed.contains(&function),
                "feature {feature:#04x} function {function}"
            );
        }
        assert!(dev.transport_mut().writes.is_empty());
    }
}
