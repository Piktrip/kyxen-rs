//! G-key and M-key notifications.
//!
//! With GKEY (0x8010) software control enabled, G-keys stop sending key
//! codes and arrive as `GKEY` notifications; M-keys arrive as `MKEYS`
//! (0x8020) notifications. Both carry a bitmask of held keys in the first
//! payload bytes (bit 0 = G1/M1); an all-zero mask means "released".

use crate::report::Notification;

/// Held-key bitmask of a GKEY notification (event 0) from feature `index`.
pub fn gkey_mask(n: &Notification, index: u8) -> Option<u16> {
    (n.feature == index && n.event == 0 && n.payload.len() >= 2)
        .then(|| u16::from_le_bytes([n.payload[0], n.payload[1]]))
}

/// Held-key bitmask of an MKEYS notification (event 0) from feature `index`.
pub fn mkey_mask(n: &Notification, index: u8) -> Option<u8> {
    (n.feature == index && n.event == 0 && !n.payload.is_empty()).then(|| n.payload[0])
}

/// Turns successive bitmasks into press / release edges.
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyTracker {
    held: u16,
}

/// A key edge; `key` is 1-based (1 = G1/M1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    Pressed(u8),
    Released(u8),
}

impl KeyTracker {
    pub fn update(&mut self, mask: u16) -> Vec<KeyEvent> {
        let (prev, now) = (self.held, mask);
        self.held = mask;
        (0..16u8)
            .filter_map(|bit| {
                let b = 1u16 << bit;
                match (prev & b != 0, now & b != 0) {
                    (false, true) => Some(KeyEvent::Pressed(bit + 1)),
                    (true, false) => Some(KeyEvent::Released(bit + 1)),
                    _ => None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Message;

    fn note(hex: &str) -> Notification {
        let raw: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let Some(Message::Notification(n)) = Message::parse(&raw) else {
            panic!("not a notification")
        };
        n
    }

    #[test]
    fn replays_captured_g915_gkey_presses() {
        // Captured from a G915 with GKEY at index 0x10.
        let packets = [
            "11ff100001000000000000000000000000000000",
            "11ff100000000000000000000000000000000000",
            "11ff100010000000000000000000000000000000",
            "11ff100000000000000000000000000000000000",
        ];
        let mut tracker = KeyTracker::default();
        let events: Vec<_> = packets
            .iter()
            .flat_map(|p| tracker.update(gkey_mask(&note(p), 0x10).unwrap()))
            .collect();
        assert_eq!(
            events,
            [
                KeyEvent::Pressed(1),
                KeyEvent::Released(1),
                KeyEvent::Pressed(5),
                KeyEvent::Released(5)
            ]
        );
    }

    #[test]
    fn chords_produce_one_edge_per_key() {
        let mut t = KeyTracker::default();
        assert_eq!(t.update(0b01), [KeyEvent::Pressed(1)]);
        assert_eq!(t.update(0b11), [KeyEvent::Pressed(2)]);
        assert_eq!(t.update(0b10), [KeyEvent::Released(1)]);
    }

    #[test]
    fn other_features_are_not_gkeys() {
        assert_eq!(
            gkey_mask(&note("11ff110002000000000000000000000000000000"), 0x10),
            None
        );
        assert_eq!(
            mkey_mask(&note("11ff110002000000000000000000000000000000"), 0x11),
            Some(0x02)
        );
    }
}
