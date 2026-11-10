//! G815 / G915 key geometry and LED zone ids.
//!
//! Generated from Kyxen's `keyboard_layout.py` and `lighting.py` (LED ids
//! confirmed by physical probing on the G815 and lighting tests on a G915).
//! Positions are in key units; x = 0, y = 0 is the top-left of the G-key column.

#![allow(clippy::unreadable_literal)]

use super::KeyDef;

#[rustfmt::skip]
pub static KEYS: &[KeyDef] = &[
    KeyDef { name: "G1", led: Some(0xb4), x: 0.0, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "G2", led: Some(0xb5), x: 0.0, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "G3", led: Some(0xb6), x: 0.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "G4", led: Some(0xb7), x: 0.0, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "G5", led: Some(0xb8), x: 0.0, y: 5.74, w: 1.0, h: 1.0 },
    KeyDef { name: "LOGO", led: Some(0xd2), x: 0.25, y: 0.0, w: 0.75, h: 0.6 },
    KeyDef { name: "M1", led: None, x: 3.0, y: 0.0, w: 1.0, h: 0.6 },
    KeyDef { name: "M2", led: None, x: 4.0, y: 0.0, w: 1.0, h: 0.6 },
    KeyDef { name: "M3", led: None, x: 5.0, y: 0.0, w: 1.0, h: 0.6 },
    KeyDef { name: "MR", led: None, x: 6.0, y: 0.0, w: 1.0, h: 0.6 },
    KeyDef { name: "GAME_MODE", led: None, x: 7.5, y: 0.0, w: 1.0, h: 0.6 },
    KeyDef { name: "ILLUMINATION", led: Some(0x99), x: 8.5, y: 0.0, w: 1.0, h: 0.6 },
    KeyDef { name: "MEDIA_PREV", led: Some(0x9e), x: 20.25, y: 0.0, w: 1.0, h: 0.6 },
    KeyDef { name: "PLAY_PAUSE", led: Some(0x9b), x: 21.25, y: 0.0, w: 1.0, h: 0.6 },
    KeyDef { name: "MEDIA_NEXT", led: Some(0x9d), x: 22.25, y: 0.0, w: 1.0, h: 0.6 },
    KeyDef { name: "MUTE", led: Some(0x9c), x: 23.25, y: 0.0, w: 1.0, h: 0.6 },
    KeyDef { name: "ESC", led: Some(0x26), x: 1.25, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F1", led: Some(0x37), x: 3.0, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F2", led: Some(0x38), x: 4.0, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F3", led: Some(0x39), x: 5.0, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F4", led: Some(0x3a), x: 6.0, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F5", led: Some(0x3b), x: 7.5, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F6", led: Some(0x3c), x: 8.5, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F7", led: Some(0x3d), x: 9.5, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F8", led: Some(0x3e), x: 10.5, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F9", led: Some(0x3f), x: 12.5, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F10", led: Some(0x40), x: 13.5, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F11", led: Some(0x41), x: 14.5, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "F12", led: Some(0x42), x: 15.5, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "PRINT_SCREEN", led: Some(0x43), x: 16.5, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "SCROLL_LOCK", led: Some(0x44), x: 17.5, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "PAUSE", led: Some(0x45), x: 18.5, y: 0.72, w: 1.0, h: 0.7 },
    KeyDef { name: "BACKTICK", led: Some(0x32), x: 1.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "1", led: Some(0x1b), x: 2.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "2", led: Some(0x1c), x: 3.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "3", led: Some(0x1d), x: 4.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "4", led: Some(0x1e), x: 5.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "5", led: Some(0x1f), x: 6.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "6", led: Some(0x20), x: 7.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "7", led: Some(0x21), x: 8.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "8", led: Some(0x22), x: 9.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "9", led: Some(0x23), x: 10.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "0", led: Some(0x24), x: 11.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "MINUS", led: Some(0x2a), x: 12.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "EQUALS", led: Some(0x2b), x: 13.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "BACKSPACE", led: Some(0x27), x: 14.25, y: 1.54, w: 2.25, h: 1.0 },
    KeyDef { name: "INSERT", led: Some(0x46), x: 16.5, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "HOME", led: Some(0x47), x: 17.5, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "PAGE_UP", led: Some(0x48), x: 18.5, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_LOCK", led: Some(0x50), x: 20.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_SLASH", led: Some(0x51), x: 21.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_STAR", led: Some(0x52), x: 22.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_MINUS", led: Some(0x53), x: 23.25, y: 1.54, w: 1.0, h: 1.0 },
    KeyDef { name: "TAB", led: Some(0x28), x: 1.25, y: 2.59, w: 1.5, h: 1.0 },
    KeyDef { name: "Q", led: Some(0x11), x: 2.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "W", led: Some(0x17), x: 3.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "E", led: Some(0x05), x: 4.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "R", led: Some(0x12), x: 5.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "T", led: Some(0x14), x: 6.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "Y", led: Some(0x19), x: 7.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "U", led: Some(0x15), x: 8.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "I", led: Some(0x09), x: 9.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "O", led: Some(0x0f), x: 10.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "P", led: Some(0x10), x: 11.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "L_BRACKET", led: Some(0x2c), x: 12.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "R_BRACKET", led: Some(0x2d), x: 13.75, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "RETURN", led: Some(0x25), x: 14.75, y: 2.59, w: 1.75, h: 1.0 },
    KeyDef { name: "DELETE", led: Some(0x49), x: 16.5, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "END", led: Some(0x4a), x: 17.5, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "PAGE_DOWN", led: Some(0x4b), x: 18.5, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_7", led: Some(0x5c), x: 20.25, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_8", led: Some(0x5d), x: 21.25, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_9", led: Some(0x5e), x: 22.25, y: 2.59, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_PLUS", led: Some(0x54), x: 23.25, y: 2.59, w: 1.0, h: 2.055556 },
    KeyDef { name: "CAPS_LOCK", led: Some(0x36), x: 1.25, y: 3.64, w: 1.75, h: 1.0 },
    KeyDef { name: "A", led: Some(0x01), x: 3.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "S", led: Some(0x13), x: 4.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "D", led: Some(0x04), x: 5.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "F", led: Some(0x06), x: 6.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "G", led: Some(0x07), x: 7.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "H", led: Some(0x08), x: 8.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "J", led: Some(0x0a), x: 9.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "K", led: Some(0x0b), x: 10.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "L", led: Some(0x0c), x: 11.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "SEMICOLON", led: Some(0x30), x: 12.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "APOSTROPHE", led: Some(0x31), x: 13.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "HASH", led: Some(0x2f), x: 14.0, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_4", led: Some(0x59), x: 20.25, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_5", led: Some(0x5a), x: 21.25, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_6", led: Some(0x5b), x: 22.25, y: 3.64, w: 1.0, h: 1.0 },
    KeyDef { name: "L_SHIFT", led: Some(0x69), x: 1.25, y: 4.69, w: 1.25, h: 1.0 },
    KeyDef { name: "BACKSLASH", led: Some(0x61), x: 2.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "Z", led: Some(0x1a), x: 3.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "X", led: Some(0x18), x: 4.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "C", led: Some(0x03), x: 5.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "V", led: Some(0x16), x: 6.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "B", led: Some(0x02), x: 7.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "N", led: Some(0x0e), x: 8.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "M", led: Some(0x0d), x: 9.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "COMMA", led: Some(0x33), x: 10.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "PERIOD", led: Some(0x34), x: 11.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "R_SLASH", led: Some(0x35), x: 12.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "R_SHIFT", led: Some(0x6d), x: 13.5, y: 4.69, w: 3.0, h: 1.0 },
    KeyDef { name: "UP", led: Some(0x4f), x: 17.5, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_1", led: Some(0x56), x: 20.25, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_2", led: Some(0x57), x: 21.25, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_3", led: Some(0x58), x: 22.25, y: 4.69, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_ENTER", led: Some(0x55), x: 23.25, y: 4.69, w: 1.0, h: 2.055556 },
    KeyDef { name: "L_CTRL", led: Some(0x68), x: 1.25, y: 5.74, w: 1.25, h: 1.0 },
    KeyDef { name: "L_SUPER", led: Some(0x6b), x: 2.5, y: 5.74, w: 1.25, h: 1.0 },
    KeyDef { name: "L_ALT", led: Some(0x6a), x: 3.75, y: 5.74, w: 1.25, h: 1.0 },
    KeyDef { name: "SPACE", led: Some(0x29), x: 5.0, y: 5.74, w: 6.25, h: 1.0 },
    KeyDef { name: "R_ALT", led: Some(0x6e), x: 11.25, y: 5.74, w: 1.25, h: 1.0 },
    KeyDef { name: "R_SUPER", led: Some(0x6f), x: 12.5, y: 5.74, w: 1.25, h: 1.0 },
    KeyDef { name: "CONTEXT", led: Some(0x62), x: 13.75, y: 5.74, w: 1.25, h: 1.0 },
    KeyDef { name: "R_CTRL", led: Some(0x6c), x: 15.0, y: 5.74, w: 1.5, h: 1.0 },
    KeyDef { name: "LEFT", led: Some(0x4d), x: 16.5, y: 5.74, w: 1.0, h: 1.0 },
    KeyDef { name: "DOWN", led: Some(0x4e), x: 17.5, y: 5.74, w: 1.0, h: 1.0 },
    KeyDef { name: "RIGHT", led: Some(0x4c), x: 18.5, y: 5.74, w: 1.0, h: 1.0 },
    KeyDef { name: "NUM_0", led: Some(0x5f), x: 20.25, y: 5.74, w: 2.0, h: 1.0 },
    KeyDef { name: "NUM_PERIOD", led: Some(0x60), x: 22.25, y: 5.74, w: 1.0, h: 1.0 },
];

/// ANSI variant of `iso`: drops the ISO-only `#` and `<>` keys, widens the
/// left Shift, moves Enter down a row and puts `\` above it. LED zones follow
/// the HID usage − 3 rule of all other keys (`\` = usage 0x31 → zone 0x2e).
pub fn ansi(iso: &[KeyDef]) -> Vec<KeyDef> {
    let mut keys = Vec::with_capacity(iso.len());
    for k in iso {
        match k.name {
            "HASH" | "BACKSLASH" => {}
            "L_SHIFT" => keys.push(KeyDef { w: 2.25, ..*k }),
            "RETURN" => {
                let quote = iso.iter().find(|q| q.name == "APOSTROPHE").unwrap_or(k);
                let right = k.x + k.w;
                let x = quote.x + quote.w;
                keys.push(KeyDef {
                    x,
                    y: quote.y,
                    w: right - x,
                    ..*k
                });
            }
            "R_BRACKET" => {
                keys.push(*k);
                let enter = iso.iter().find(|e| e.name == "RETURN").unwrap_or(k);
                keys.push(KeyDef {
                    name: "ANSI_BACKSLASH",
                    led: Some(0x2e),
                    x: k.x + k.w,
                    w: enter.x + enter.w - (k.x + k.w),
                    ..*k
                });
            }
            _ => keys.push(*k),
        }
    }
    keys
}
