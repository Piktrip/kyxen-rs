//! In-memory keyboard for tests: a feature table, onboard-profile flash
//! with a configurable sector size, and a queue of incoming reports.

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::time::Duration;

use crate::device::Transport;
use crate::feature::FeatureId;
use crate::report::{LONG_ID, LONG_LEN, SHORT_ID};

/// Fake keyboard speaking enough HID++ for this crate's operations.
#[derive(Debug)]
pub struct MockKeyboard {
    /// `(index, feature id)`; index 0 is ROOT, 1 FEATURE_SET.
    pub features: Vec<(u8, FeatureId)>,
    pub sector_size: usize,
    pub sectors: BTreeMap<u16, Vec<u8>>,
    pub mode: u8,
    pub current_profile: u8,
    /// Reports waiting to be read (responses, notifications).
    pub inbox: VecDeque<Vec<u8>>,
    /// Every report the host sent.
    pub sent: Vec<Vec<u8>>,
    /// Sector writes as `(sector, data)`, committed on writeEnd.
    pub writes: Vec<(u16, Vec<u8>)>,
    write_buf: Option<(u16, Vec<u8>)>,
}

impl MockKeyboard {
    /// A G915 as measured on hardware: 255-byte sectors, GKEY at 0x10,
    /// MKEYS 0x11, PER_KEY_LIGHTING_V2 0x0a, ONBOARD_PROFILES 0x14.
    pub fn g915() -> Self {
        Self {
            features: vec![
                (0x00, FeatureId::ROOT),
                (0x01, FeatureId::FEATURE_SET),
                (0x03, FeatureId::DEVICE_NAME),
                (0x09, FeatureId::RGB_EFFECTS),
                (0x0a, FeatureId::PER_KEY_LIGHTING_V2),
                (0x10, FeatureId::GKEY),
                (0x11, FeatureId::MKEYS),
                (0x14, FeatureId::ONBOARD_PROFILES),
            ],
            sector_size: 255,
            sectors: BTreeMap::new(),
            mode: 1,
            current_profile: 1,
            inbox: VecDeque::new(),
            sent: Vec::new(),
            writes: Vec::new(),
            write_buf: None,
        }
    }

    fn index_of(&self, id: FeatureId) -> u8 {
        self.features
            .iter()
            .find(|(_, f)| *f == id)
            .map_or(0, |(i, _)| *i)
    }

    /// Queue a raw report as if the keyboard had sent it.
    pub fn push(&mut self, report: Vec<u8>) {
        self.inbox.push_back(report);
    }

    fn reply(&mut self, req: &[u8], payload: &[u8]) {
        let mut r = vec![0u8; LONG_LEN];
        r[..4].copy_from_slice(&[LONG_ID, req[1], req[2], req[3]]);
        r[4..4 + payload.len()].copy_from_slice(payload);
        self.inbox.push_back(r);
    }

    fn error(&mut self, req: &[u8], code: u8) {
        let mut r = vec![0u8; LONG_LEN];
        r[..6].copy_from_slice(&[LONG_ID, req[1], 0xff, req[2], req[3], code]);
        self.inbox.push_back(r);
    }

    fn handle(&mut self, req: &[u8]) {
        if !matches!(req[0], SHORT_ID | LONG_ID) {
            return;
        }
        let (feat, func, p) = (req[2], req[3] >> 4, &req[4..]);
        let p: Vec<u8> = p.to_vec();
        let id = self
            .features
            .iter()
            .find(|(i, _)| *i == feat)
            .map(|(_, f)| *f);
        match (id, func) {
            (Some(FeatureId::ROOT), 0) => {
                let wanted = FeatureId(u16::from_be_bytes([p[0], p[1]]));
                let idx = self.index_of(wanted);
                self.reply(req, &[idx]);
            }
            (Some(FeatureId::FEATURE_SET), 0) => {
                // real devices number features 1..=count without gaps
                let n = self.features.iter().map(|(i, _)| *i).max().unwrap_or(0);
                self.reply(req, &[n]);
            }
            (Some(FeatureId::FEATURE_SET), 1) => {
                let fid = self
                    .features
                    .iter()
                    .find(|(i, _)| *i == p[0])
                    .map_or(0, |(_, f)| f.0);
                self.reply(req, &fid.to_be_bytes());
            }
            (Some(FeatureId::ONBOARD_PROFILES), f) => self.onboard(req, f, &p),
            // lighting and other fire-and-forget reports get a plain ack
            (Some(_), _) => self.reply(req, &[]),
            (None, _) => self.error(req, 6),
        }
    }

    fn onboard(&mut self, req: &[u8], func: u8, p: &[u8]) {
        let size = self.sector_size;
        match func {
            0 => {
                let s = (size as u16).to_be_bytes();
                self.reply(req, &[1, 4, 1, 3, 3, 8, 16, s[0], s[1]]);
            }
            1 => {
                self.mode = p[0];
                self.reply(req, &[]);
            }
            2 => self.reply(req, &[self.mode]),
            3 => {
                if self.mode == 1 {
                    self.current_profile = p[1];
                    self.reply(req, &[]);
                } else {
                    self.error(req, 2);
                }
            }
            4 => {
                let cur = if self.mode == 1 {
                    self.current_profile
                } else {
                    0
                };
                self.reply(req, &[0, cur]);
            }
            5 => {
                let sector = u16::from_be_bytes([p[0], p[1]]);
                let offset = usize::from(u16::from_be_bytes([p[2], p[3]]));
                match self.sectors.get(&sector) {
                    Some(data) if offset + 16 <= size => {
                        let chunk = data[offset..offset + 16].to_vec();
                        self.reply(req, &chunk);
                    }
                    _ => self.error(req, 2),
                }
            }
            6 => {
                let sector = u16::from_be_bytes([p[0], p[1]]);
                self.write_buf = Some((sector, Vec::new()));
                self.reply(req, &[]);
            }
            7 => {
                if let Some((_, buf)) = self.write_buf.as_mut() {
                    buf.extend_from_slice(&p[..16]);
                }
                self.reply(req, &[]);
            }
            8 => {
                if let Some((sector, mut buf)) = self.write_buf.take() {
                    buf.truncate(size);
                    self.sectors.insert(sector, buf.clone());
                    self.writes.push((sector, buf));
                }
                self.reply(req, &[]);
            }
            _ => self.error(req, 7),
        }
    }
}

impl Transport for MockKeyboard {
    fn send(&mut self, report: &[u8]) -> io::Result<()> {
        self.sent.push(report.to_vec());
        self.handle(report);
        Ok(())
    }

    fn recv(&mut self, buf: &mut [u8], _timeout: Duration) -> io::Result<usize> {
        let Some(r) = self.inbox.pop_front() else {
            return Ok(0);
        };
        buf[..r.len()].copy_from_slice(&r);
        Ok(r.len())
    }
}
