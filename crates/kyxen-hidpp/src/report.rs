//! HID++ report framing.
//!
//! ```text
//! short (7 bytes):  10 <dev> <feature idx> <func<<4 | sw id> <3 params>
//! long (20 bytes):  11 <dev> <feature idx> <func<<4 | sw id> <16 params>
//! 2.0 error:        11 <dev> ff <feature idx> <func|sw> <code> …
//! 1.0 error:        10 <dev> 8f <sub id> <address> <code> …
//! ```
//! Notifications use software id 0; our requests use [`SW_ID`], which lets
//! responses and notifications be told apart.

pub const SHORT_ID: u8 = 0x10;
pub const LONG_ID: u8 = 0x11;
pub const SHORT_LEN: usize = 7;
pub const LONG_LEN: usize = 20;
/// Device index of a keyboard connected by cable.
pub const WIRED_DEVICE: u8 = 0xff;
/// Software id stamped on every request we send.
pub const SW_ID: u8 = 0x0e;

const ERROR_20: u8 = 0xff;
const ERROR_10: u8 = 0x8f;

/// Which report type a request is sent with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    Short,
    Long,
}

/// Build a request report for `feature` (runtime index) and `function`.
pub fn request(kind: ReportKind, feature: u8, function: u8, params: &[u8]) -> Vec<u8> {
    let (id, len) = match kind {
        ReportKind::Short => (SHORT_ID, SHORT_LEN),
        ReportKind::Long => (LONG_ID, LONG_LEN),
    };
    debug_assert!(params.len() <= len - 4, "too many parameters for report");
    let mut pkt = vec![0u8; len];
    pkt[..4].copy_from_slice(&[id, WIRED_DEVICE, feature, (function << 4) | SW_ID]);
    let n = params.len().min(len - 4);
    pkt[4..4 + n].copy_from_slice(&params[..n]);
    pkt
}

/// Build a long request.
pub fn long(feature: u8, function: u8, params: &[u8]) -> Vec<u8> {
    request(ReportKind::Long, feature, function, params)
}

/// A notification sent by the keyboard on its own (software id 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub feature: u8,
    pub event: u8,
    pub payload: Vec<u8>,
}

/// A parsed HID++ report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Response {
        feature: u8,
        function: u8,
        sw_id: u8,
        payload: Vec<u8>,
    },
    Error {
        feature: u8,
        function: u8,
        code: u8,
    },
    Notification(Notification),
}

impl Message {
    /// Parse a raw report. Returns `None` for non-HID++ reports (keyboard,
    /// consumer-control, … reports that share the hidraw node).
    pub fn parse(buf: &[u8]) -> Option<Self> {
        let [id, _dev, b2, b3, rest @ ..] = buf else {
            return None;
        };
        if !matches!(*id, SHORT_ID | LONG_ID) {
            return None;
        }
        if (*id == LONG_ID && *b2 == ERROR_20) || (*id == SHORT_ID && *b2 == ERROR_10) {
            let [func_sw, code, ..] = rest else {
                return None;
            };
            return Some(Self::Error {
                feature: *b3,
                function: func_sw >> 4,
                code: *code,
            });
        }
        let function = b3 >> 4;
        let sw_id = b3 & 0x0f;
        let payload = rest.to_vec();
        Some(if sw_id == 0 {
            Self::Notification(Notification {
                feature: *b2,
                event: function,
                payload,
            })
        } else {
            Self::Response {
                feature: *b2,
                function,
                sw_id,
                payload,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_long_request() {
        let pkt = long(0x14, 5, &[0x00, 0x01, 0x00, 0xef]);
        assert_eq!(pkt.len(), 20);
        assert_eq!(&pkt[..8], &[0x11, 0xff, 0x14, 0x5e, 0x00, 0x01, 0x00, 0xef]);
    }

    #[test]
    fn builds_short_request() {
        let pkt = request(ReportKind::Short, 0x14, 3, &[0x00, 0x01]);
        assert_eq!(pkt, vec![0x10, 0xff, 0x14, 0x3e, 0x00, 0x01, 0x00]);
    }

    #[test]
    fn parses_hidpp20_error() {
        // Captured from a G915: readData past the end of a 255-byte sector.
        let raw = [
            0x11, 0xff, 0xff, 0x14, 0x50, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(
            Message::parse(&raw),
            Some(Message::Error {
                feature: 0x14,
                function: 5,
                code: 2
            })
        );
    }

    #[test]
    fn parses_gkey_notification() {
        // G915: G1 pressed (GKEY feature at index 0x10)
        let raw = [
            0x11, 0xff, 0x10, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let Some(Message::Notification(n)) = Message::parse(&raw) else {
            panic!("not a notification")
        };
        assert_eq!((n.feature, n.event, n.payload[0]), (0x10, 0, 0x01));
    }

    #[test]
    fn ignores_non_hidpp_reports() {
        assert_eq!(Message::parse(&[0x01, 0, 0x04, 0, 0, 0, 0, 0]), None);
        assert_eq!(Message::parse(&[0x11]), None);
    }
}
