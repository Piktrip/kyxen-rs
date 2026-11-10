//! Character → physical key tables for typing text through uinput.
//!
//! uinput injects key *positions*; the desktop's XKB layout decides which
//! character each produces. Supported: `us`, `gb`, `de`, `at` (Austrian
//! German types identically to German), with and without dead keys.

use std::collections::HashMap;
use std::process::Command;

use evdev::KeyCode as K;

/// Modifier needed for a character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Mods {
    pub shift: bool,
    pub altgr: bool,
}

impl Mods {
    pub const NONE: Self = Self {
        shift: false,
        altgr: false,
    };
    pub const SHIFT: Self = Self {
        shift: true,
        altgr: false,
    };
    pub const ALTGR: Self = Self {
        shift: false,
        altgr: true,
    };

    pub fn is_none(self) -> bool {
        self == Self::NONE
    }
}

#[derive(Debug, Clone)]
pub struct TextLayout {
    pub name: String,
    chars: HashMap<char, (K, Mods)>,
    /// Characters typed with a dead key, followed by Space to emit them bare.
    dead: Vec<char>,
}

impl TextLayout {
    pub fn lookup(&self, c: char) -> Option<(K, Mods)> {
        self.chars.get(&c).copied()
    }

    pub fn is_dead(&self, c: char) -> bool {
        self.dead.contains(&c)
    }

    pub fn chars(&self) -> impl Iterator<Item = (char, K, Mods)> + '_ {
        self.chars.iter().map(|(&c, &(k, m))| (c, k, m))
    }

    /// Table for an XKB layout + variant; unknown layouts fall back to `us`.
    pub fn new(layout: &str, variant: &str) -> Self {
        let layout = layout.trim().to_ascii_lowercase();
        let dead_keys = !variant.to_ascii_lowercase().contains("nodeadkeys");
        match layout.as_str() {
            "de" | "at" => german(&layout, dead_keys),
            "gb" | "uk" => gb(),
            "us" => us(),
            other => {
                tracing::warn!(
                    layout = other,
                    "layout not supported for text macros, using us"
                );
                us()
            }
        }
    }
}

pub(crate) const LETTERS: [K; 26] = [
    K::KEY_A,
    K::KEY_B,
    K::KEY_C,
    K::KEY_D,
    K::KEY_E,
    K::KEY_F,
    K::KEY_G,
    K::KEY_H,
    K::KEY_I,
    K::KEY_J,
    K::KEY_K,
    K::KEY_L,
    K::KEY_M,
    K::KEY_N,
    K::KEY_O,
    K::KEY_P,
    K::KEY_Q,
    K::KEY_R,
    K::KEY_S,
    K::KEY_T,
    K::KEY_U,
    K::KEY_V,
    K::KEY_W,
    K::KEY_X,
    K::KEY_Y,
    K::KEY_Z,
];
pub(crate) const DIGITS: [K; 10] = [
    K::KEY_0,
    K::KEY_1,
    K::KEY_2,
    K::KEY_3,
    K::KEY_4,
    K::KEY_5,
    K::KEY_6,
    K::KEY_7,
    K::KEY_8,
    K::KEY_9,
];

fn base(swap_yz: bool) -> HashMap<char, (K, Mods)> {
    let mut m = HashMap::new();
    for (c, key) in ('a'..='z').zip(LETTERS) {
        let key = match (swap_yz, c) {
            (true, 'y') => K::KEY_Z,
            (true, 'z') => K::KEY_Y,
            _ => key,
        };
        m.insert(c, (key, Mods::NONE));
        m.insert(c.to_ascii_uppercase(), (key, Mods::SHIFT));
    }
    for (c, key) in ('0'..='9').zip(DIGITS) {
        m.insert(c, (key, Mods::NONE));
    }
    m.insert(' ', (K::KEY_SPACE, Mods::NONE));
    m.insert('\n', (K::KEY_ENTER, Mods::NONE));
    m.insert('\t', (K::KEY_TAB, Mods::NONE));
    m
}

fn add(m: &mut HashMap<char, (K, Mods)>, entries: &[(char, K, Mods)]) {
    m.extend(entries.iter().map(|&(c, k, mods)| (c, (k, mods))));
}

#[rustfmt::skip]
fn us_punctuation(m: &mut HashMap<char, (K, Mods)>) {
    use Mods as M;
    add(m, &[
        ('-', K::KEY_MINUS, M::NONE), ('_', K::KEY_MINUS, M::SHIFT),
        ('=', K::KEY_EQUAL, M::NONE), ('+', K::KEY_EQUAL, M::SHIFT),
        ('[', K::KEY_LEFTBRACE, M::NONE), ('{', K::KEY_LEFTBRACE, M::SHIFT),
        (']', K::KEY_RIGHTBRACE, M::NONE), ('}', K::KEY_RIGHTBRACE, M::SHIFT),
        (';', K::KEY_SEMICOLON, M::NONE), (':', K::KEY_SEMICOLON, M::SHIFT),
        (',', K::KEY_COMMA, M::NONE), ('<', K::KEY_COMMA, M::SHIFT),
        ('.', K::KEY_DOT, M::NONE), ('>', K::KEY_DOT, M::SHIFT),
        ('/', K::KEY_SLASH, M::NONE), ('?', K::KEY_SLASH, M::SHIFT),
        ('!', K::KEY_1, M::SHIFT), ('$', K::KEY_4, M::SHIFT), ('%', K::KEY_5, M::SHIFT),
        ('^', K::KEY_6, M::SHIFT), ('&', K::KEY_7, M::SHIFT), ('*', K::KEY_8, M::SHIFT),
        ('(', K::KEY_9, M::SHIFT), (')', K::KEY_0, M::SHIFT),
    ]);
}

#[rustfmt::skip]
fn us() -> TextLayout {
    use Mods as M;
    let mut m = base(false);
    us_punctuation(&mut m);
    add(&mut m, &[
        ('@', K::KEY_2, M::SHIFT), ('#', K::KEY_3, M::SHIFT),
        ('\'', K::KEY_APOSTROPHE, M::NONE), ('"', K::KEY_APOSTROPHE, M::SHIFT),
        ('`', K::KEY_GRAVE, M::NONE), ('~', K::KEY_GRAVE, M::SHIFT),
        ('\\', K::KEY_BACKSLASH, M::NONE), ('|', K::KEY_BACKSLASH, M::SHIFT),
    ]);
    TextLayout { name: "us".into(), chars: m, dead: Vec::new() }
}

#[rustfmt::skip]
fn gb() -> TextLayout {
    use Mods as M;
    let mut m = base(false);
    us_punctuation(&mut m);
    add(&mut m, &[
        ('"', K::KEY_2, M::SHIFT), ('£', K::KEY_3, M::SHIFT), ('€', K::KEY_4, M::ALTGR),
        ('\'', K::KEY_APOSTROPHE, M::NONE), ('@', K::KEY_APOSTROPHE, M::SHIFT),
        ('#', K::KEY_BACKSLASH, M::NONE), ('~', K::KEY_BACKSLASH, M::SHIFT),
        ('`', K::KEY_GRAVE, M::NONE), ('¬', K::KEY_GRAVE, M::SHIFT),
        ('\\', K::KEY_102ND, M::NONE), ('|', K::KEY_102ND, M::SHIFT),
    ]);
    TextLayout { name: "gb".into(), chars: m, dead: Vec::new() }
}

#[rustfmt::skip]
fn german(name: &str, dead_keys: bool) -> TextLayout {
    use Mods as M;
    let mut m = base(true);
    add(&mut m, &[
        ('^', K::KEY_GRAVE, M::NONE), ('°', K::KEY_GRAVE, M::SHIFT),
        ('!', K::KEY_1, M::SHIFT), ('¹', K::KEY_1, M::ALTGR),
        ('"', K::KEY_2, M::SHIFT), ('²', K::KEY_2, M::ALTGR),
        ('§', K::KEY_3, M::SHIFT), ('³', K::KEY_3, M::ALTGR),
        ('$', K::KEY_4, M::SHIFT), ('%', K::KEY_5, M::SHIFT), ('&', K::KEY_6, M::SHIFT),
        ('/', K::KEY_7, M::SHIFT), ('{', K::KEY_7, M::ALTGR),
        ('(', K::KEY_8, M::SHIFT), ('[', K::KEY_8, M::ALTGR),
        (')', K::KEY_9, M::SHIFT), (']', K::KEY_9, M::ALTGR),
        ('=', K::KEY_0, M::SHIFT), ('}', K::KEY_0, M::ALTGR),
        ('ß', K::KEY_MINUS, M::NONE), ('?', K::KEY_MINUS, M::SHIFT), ('\\', K::KEY_MINUS, M::ALTGR),
        ('´', K::KEY_EQUAL, M::NONE), ('`', K::KEY_EQUAL, M::SHIFT),
        ('@', K::KEY_Q, M::ALTGR), ('€', K::KEY_E, M::ALTGR), ('µ', K::KEY_M, M::ALTGR),
        ('ü', K::KEY_LEFTBRACE, M::NONE), ('Ü', K::KEY_LEFTBRACE, M::SHIFT),
        ('+', K::KEY_RIGHTBRACE, M::NONE), ('*', K::KEY_RIGHTBRACE, M::SHIFT), ('~', K::KEY_RIGHTBRACE, M::ALTGR),
        ('ö', K::KEY_SEMICOLON, M::NONE), ('Ö', K::KEY_SEMICOLON, M::SHIFT),
        ('ä', K::KEY_APOSTROPHE, M::NONE), ('Ä', K::KEY_APOSTROPHE, M::SHIFT),
        ('#', K::KEY_BACKSLASH, M::NONE), ('\'', K::KEY_BACKSLASH, M::SHIFT),
        ('<', K::KEY_102ND, M::NONE), ('>', K::KEY_102ND, M::SHIFT), ('|', K::KEY_102ND, M::ALTGR),
        (',', K::KEY_COMMA, M::NONE), (';', K::KEY_COMMA, M::SHIFT),
        ('.', K::KEY_DOT, M::NONE), (':', K::KEY_DOT, M::SHIFT),
        ('-', K::KEY_SLASH, M::NONE), ('_', K::KEY_SLASH, M::SHIFT),
    ]);
    let dead = if dead_keys { vec!['^', '´', '`'] } else { Vec::new() };
    TextLayout { name: name.into(), chars: m, dead }
}

/// `(layout, variant)` from `localectl status` output (first of comma lists).
pub fn parse_localectl(output: &str) -> (String, String) {
    let field = |key: &str| {
        output
            .lines()
            .filter_map(|l| l.split_once(':'))
            .find(|(k, _)| k.trim() == key)
            .map(|(_, v)| v.split(',').next().unwrap_or_default().trim().to_owned())
            .unwrap_or_default()
    };
    (field("X11 Layout"), field("X11 Variant"))
}

/// Detect `(layout, variant)`: explicit override, else `localectl`, else `us`.
pub fn detect(override_layout: Option<&str>, override_variant: Option<&str>) -> (String, String) {
    if let Some(l) = override_layout.filter(|l| !l.is_empty()) {
        return (
            l.to_owned(),
            override_variant.unwrap_or_default().to_owned(),
        );
    }
    Command::new("localectl")
        .arg("status")
        .output()
        .ok()
        .map(|o| parse_localectl(&String::from_utf8_lossy(&o.stdout)))
        .filter(|(l, _)| !l.is_empty())
        .unwrap_or_else(|| ("us".into(), String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn austrian_is_qwertz_with_altgr() {
        let at = TextLayout::new("at", "");
        assert_eq!(at.lookup('z'), Some((K::KEY_Y, Mods::NONE)));
        assert_eq!(at.lookup('y'), Some((K::KEY_Z, Mods::NONE)));
        assert_eq!(at.lookup('@'), Some((K::KEY_Q, Mods::ALTGR)));
        assert_eq!(at.lookup('ä'), Some((K::KEY_APOSTROPHE, Mods::NONE)));
        assert!(at.is_dead('^'));
        assert!(!TextLayout::new("at", "nodeadkeys").is_dead('^'));
    }

    #[test]
    fn unknown_layout_is_us() {
        assert_eq!(TextLayout::new("xx", "").name, "us");
    }

    #[test]
    fn parses_localectl() {
        let out = "   System Locale: LANG=de_AT.UTF-8\n      X11 Layout: at,us\n     X11 Variant: nodeadkeys,\n";
        assert_eq!(parse_localectl(out), ("at".into(), "nodeadkeys".into()));
        assert_eq!(detect(Some("de"), None), ("de".into(), String::new()));
    }
}
