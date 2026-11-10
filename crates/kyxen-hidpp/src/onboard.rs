//! ONBOARD_PROFILES (0x8100): mode, active profile and raw sector access.
//!
//! Sectors: ROM `0x0101..=0x0103` (factory defaults, read-only; the G915's
//! carry no CRC) and user `0x0001..=0x0003` (one per M-key slot). The sector
//! size comes from `getDescription`: 256 bytes on the G815, 255 on the G915.
//! Reading past the end fails with `InvalidArgument`, so the last chunk is
//! read at `size - 16`. Each sector ends in a big-endian CRC-CCITT
//! (poly 0x1021, init 0xFFFF) over all preceding bytes.

use tracing::info;

use crate::device::{HidppDevice, Transport};
use crate::error::{Error, Result};
use crate::feature::FeatureId;
use crate::report::ReportKind;

const FN_DESCRIPTION: u8 = 0;
const FN_SET_MODE: u8 = 1;
const FN_GET_MODE: u8 = 2;
const FN_SET_CURRENT: u8 = 3;
const FN_GET_CURRENT: u8 = 4;
const FN_READ: u8 = 5;
const FN_WRITE_START: u8 = 6;
const FN_WRITE_DATA: u8 = 7;
const FN_WRITE_END: u8 = 8;

/// `(ROM sector, user sector)` per profile slot.
pub const PROFILE_SECTORS: [(u16, u16); 3] = [(0x0101, 0x0001), (0x0102, 0x0002), (0x0103, 0x0003)];
/// Offset of the first G-key binding in a profile sector.
pub const GKEY_BLOCK_OFFSET: usize = 0x20;
pub const GKEY_COUNT: usize = 5;
/// G1–G5 → F13–F17, which no physical key produces.
pub const F13_F17: [u8; GKEY_COUNT] = [0x68, 0x69, 0x6a, 0x6b, 0x6c];

/// Binding entry: `80 02 <modifiers> <HID usage>` is a plain keyboard key.
const BINDING_HID: u8 = 0x80;
const BINDING_KEYBOARD: u8 = 0x02;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The keyboard runs its stored profiles (lighting, bindings).
    Onboard,
    /// The host controls everything; nothing from flash is applied.
    Host,
    Other(u8),
}

impl From<u8> for Mode {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::Onboard,
            2 => Self::Host,
            other => Self::Other(other),
        }
    }
}

impl From<Mode> for u8 {
    fn from(m: Mode) -> Self {
        match m {
            Mode::Onboard => 1,
            Mode::Host => 2,
            Mode::Other(v) => v,
        }
    }
}

/// Handle to the ONBOARD_PROFILES feature of one keyboard.
#[derive(Debug, Clone, Copy)]
pub struct OnboardProfiles {
    pub index: u8,
    pub sector_size: usize,
}

impl OnboardProfiles {
    /// Resolve the feature and read its description.
    pub fn open<T: Transport>(dev: &mut HidppDevice<T>) -> Result<Self> {
        let index = dev.feature_index(FeatureId::ONBOARD_PROFILES)?;
        let desc = dev.request(index, FN_DESCRIPTION, &[])?;
        let sector_size = usize::from(u16::from_be_bytes([desc[7], desc[8]]));
        if !(32..=4096).contains(&sector_size) {
            return Err(Error::Protocol(format!(
                "implausible sector size {sector_size}"
            )));
        }
        Ok(Self { index, sector_size })
    }

    pub fn mode<T: Transport>(&self, dev: &mut HidppDevice<T>) -> Result<Mode> {
        Ok(dev.request(self.index, FN_GET_MODE, &[])?[0].into())
    }

    pub fn set_mode<T: Transport>(&self, dev: &mut HidppDevice<T>, mode: Mode) -> Result<()> {
        dev.request(self.index, FN_SET_MODE, &[mode.into()])
            .map(drop)
    }

    /// Active profile slot, 1-based (0 = none, e.g. in host mode).
    pub fn current_profile<T: Transport>(&self, dev: &mut HidppDevice<T>) -> Result<u8> {
        Ok(dev.request(self.index, FN_GET_CURRENT, &[])?[1])
    }

    /// Activate a profile slot (1-based). Requires onboard mode.
    pub fn set_current_profile<T: Transport>(
        &self,
        dev: &mut HidppDevice<T>,
        slot: u8,
    ) -> Result<()> {
        dev.request_with(ReportKind::Short, self.index, FN_SET_CURRENT, &[0x00, slot])
            .map(drop)
    }

    /// Put the keyboard back into onboard mode on `slot`, re-applying its
    /// stored lighting. Used when the daemon exits.
    pub fn restore<T: Transport>(&self, dev: &mut HidppDevice<T>, slot: u8) -> Result<()> {
        self.set_mode(dev, Mode::Onboard)?;
        self.set_current_profile(dev, slot)
    }

    pub fn read_sector<T: Transport>(
        &self,
        dev: &mut HidppDevice<T>,
        sector: u16,
    ) -> Result<Vec<u8>> {
        let size = self.sector_size;
        let mut data = vec![0u8; size];
        for start in (0..size).step_by(16) {
            let offset = start.min(size - 16);
            let mut params = [0u8; 4];
            params[..2].copy_from_slice(&sector.to_be_bytes());
            params[2..].copy_from_slice(&(offset as u16).to_be_bytes());
            let resp = dev.request(self.index, FN_READ, &params)?;
            data[offset..offset + 16].copy_from_slice(&resp[..16]);
        }
        Ok(data)
    }

    pub fn write_sector<T: Transport>(
        &self,
        dev: &mut HidppDevice<T>,
        sector: u16,
        data: &[u8],
    ) -> Result<()> {
        let size = self.sector_size;
        if data.len() != size {
            return Err(Error::Refused(format!(
                "sector data is {} bytes, expected {size}",
                data.len()
            )));
        }
        let mut params = [0u8; 6];
        params[..2].copy_from_slice(&sector.to_be_bytes());
        params[4..].copy_from_slice(&(size as u16).to_be_bytes());
        dev.request(self.index, FN_WRITE_START, &params)?;
        for chunk in data.chunks(16) {
            dev.request(self.index, FN_WRITE_DATA, chunk)?;
        }
        dev.request_with(ReportKind::Short, self.index, FN_WRITE_END, &[])
            .map(drop)
    }

    /// Bind G1–G5 to F13–F17 in all three user profiles (G815 strategy).
    ///
    /// Valid user sectors are patched in place so settings stored by other
    /// tools survive; a broken user sector is rebuilt from its ROM sector.
    /// Every source is read and validated before anything is written.
    /// Returns `true` if flash was written.
    pub fn ensure_gkey_remap<T: Transport>(&self, dev: &mut HidppDevice<T>) -> Result<bool> {
        let user: Vec<Vec<u8>> = PROFILE_SECTORS
            .iter()
            .map(|&(_, u)| self.read_sector(dev, u))
            .collect::<Result<_>>()?;
        if user
            .iter()
            .all(|s| crc_valid(s) && gkey_usages(s) == F13_F17)
        {
            info!("G-key remap already present on all profiles");
            return Ok(false);
        }

        let mut plan = Vec::with_capacity(PROFILE_SECTORS.len());
        for (&(rom_addr, user_addr), data) in PROFILE_SECTORS.iter().zip(user) {
            let source = if crc_valid(&data) {
                check_source(user_addr, &data, false)?;
                data
            } else {
                let rom = self.read_sector(dev, rom_addr)?;
                check_source(rom_addr, &rom, true)?;
                rom
            };
            plan.push((user_addr, source));
        }

        info!(
            sector_size = self.sector_size,
            "remapping G1–G5 → F13–F17 in flash"
        );
        for (addr, mut data) in plan {
            patch_gkeys(&mut data);
            apply_crc(&mut data);
            self.write_sector(dev, addr, &data)?;
        }
        self.set_current_profile(dev, 1)?;
        Ok(true)
    }
}

/// CRC-CCITT (poly 0x1021, init 0xFFFF).
pub fn crc_ccitt(data: &[u8]) -> u16 {
    data.iter().fold(0xffff_u16, |mut crc, &b| {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x1021
            };
        }
        crc
    })
}

fn stored_crc(data: &[u8]) -> u16 {
    u16::from_be_bytes([data[data.len() - 2], data[data.len() - 1]])
}

pub fn crc_valid(data: &[u8]) -> bool {
    data.len() > 2 && crc_ccitt(&data[..data.len() - 2]) == stored_crc(data)
}

pub fn apply_crc(data: &mut [u8]) {
    let n = data.len();
    let crc = crc_ccitt(&data[..n - 2]);
    data[n - 2..].copy_from_slice(&crc.to_be_bytes());
}

pub fn gkey_usages(data: &[u8]) -> [u8; GKEY_COUNT] {
    std::array::from_fn(|i| data[GKEY_BLOCK_OFFSET + i * 4 + 3])
}

pub fn patch_gkeys(data: &mut [u8]) {
    for (i, usage) in F13_F17.iter().enumerate() {
        let base = GKEY_BLOCK_OFFSET + i * 4;
        data[base + 2] = 0x00;
        data[base + 3] = *usage;
    }
}

/// Refuse to patch a sector whose layout differs from what `patch_gkeys`
/// assumes: user sectors need a valid CRC (ROM may have none, i.e. `ff ff`),
/// and every G-key entry must be a keyboard binding `80 02 xx yy`.
pub fn check_source(sector: u16, data: &[u8], is_rom: bool) -> Result<()> {
    let rom_without_crc = is_rom && stored_crc(data) == 0xffff;
    if !crc_valid(data) && !rom_without_crc {
        return Err(Error::Refused(format!(
            "sector 0x{sector:04x} has an invalid CRC"
        )));
    }
    for g in 0..GKEY_COUNT {
        let entry = &data[GKEY_BLOCK_OFFSET + g * 4..GKEY_BLOCK_OFFSET + g * 4 + 4];
        if entry[0] != BINDING_HID || entry[1] != BINDING_KEYBOARD {
            return Err(Error::Refused(format!(
                "sector 0x{sector:04x}: G{} entry {entry:02x?} is not a keyboard binding",
                g + 1
            )));
        }
    }
    Ok(())
}
