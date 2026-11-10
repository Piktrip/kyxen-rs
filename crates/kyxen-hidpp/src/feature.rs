//! Feature ids and runtime index lookup.

use crate::device::{HidppDevice, Transport};
use crate::error::{Error, Result};

/// A HID++ 2.0 feature id. Constant across devices, unlike its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FeatureId(pub u16);

impl FeatureId {
    pub const ROOT: Self = Self(0x0000);
    pub const FEATURE_SET: Self = Self(0x0001);
    pub const DEVICE_NAME: Self = Self(0x0005);
    pub const GKEY: Self = Self(0x8010);
    pub const MKEYS: Self = Self(0x8020);
    pub const MR: Self = Self(0x8030);
    pub const RGB_EFFECTS: Self = Self(0x8071);
    pub const PER_KEY_LIGHTING_V2: Self = Self(0x8081);
    pub const ONBOARD_PROFILES: Self = Self(0x8100);

    pub fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            0x0000 => "ROOT",
            0x0001 => "FEATURE_SET",
            0x0003 => "FW_VERSION",
            0x0005 => "DEVICE_NAME",
            0x0020 => "RESET",
            0x1001 => "BATTERY_VOLTAGE",
            0x1814 => "CHANGE_HOST",
            0x1815 => "HOSTS_INFO",
            0x1b04 => "REPROG_CONTROLS_V4",
            0x1d4b => "WIRELESS_STATUS",
            0x4522 => "DISABLE_KEYS_BY_USAGE",
            0x4540 => "KEYBOARD_LAYOUT_2",
            0x8010 => "GKEY",
            0x8020 => "MKEYS",
            0x8030 => "MR",
            0x8040 => "BRIGHTNESS",
            0x8060 => "REPORT_RATE",
            0x8071 => "RGB_EFFECTS",
            0x8081 => "PER_KEY_LIGHTING_V2",
            0x8100 => "ONBOARD_PROFILES",
            _ => return None,
        })
    }
}

impl<T: Transport> HidppDevice<T> {
    /// Runtime index of `id` on this keyboard (ROOT `getFeature`), cached.
    pub fn feature_index(&mut self, id: FeatureId) -> Result<u8> {
        if let Some(&idx) = self.feature_cache().get(&id.0) {
            return Ok(idx);
        }
        let resp = self.request(0x00, 0, &id.0.to_be_bytes())?;
        let idx = resp[0];
        if idx == 0 && id != FeatureId::ROOT {
            return Err(Error::Unsupported(id.0));
        }
        self.feature_cache().insert(id.0, idx);
        Ok(idx)
    }

    /// Full feature table as `(index, feature id)` via FEATURE_SET (index 1).
    pub fn feature_table(&mut self) -> Result<Vec<(u8, FeatureId)>> {
        let count = self.request(0x01, 0, &[])?[0];
        (1..=count)
            .map(|idx| {
                let resp = self.request(0x01, 1, &[idx])?;
                Ok((idx, FeatureId(u16::from_be_bytes([resp[0], resp[1]]))))
            })
            .collect()
    }
}
