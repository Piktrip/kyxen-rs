//! Translate macros into key events (no I/O).

use evdev::KeyCode as K;

use crate::layout::{DIGITS, LETTERS, Mods, TextLayout};

/// `(key, pressed)` — `true` = press, `false` = release.
pub type KeyStroke = (K, bool);

/// Key events that type `text` on `layout`, plus characters it cannot type.
/// Dead-key characters (`^` on QWERTZ) are followed by Space.
pub fn text_strokes(text: &str, layout: &TextLayout) -> (Vec<KeyStroke>, Vec<char>) {
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    for c in text.chars() {
        let Some((key, mods)) = layout.lookup(c) else {
            skipped.push(c);
            continue;
        };
        tap(&mut out, key, mods);
        if layout.is_dead(c) {
            tap(&mut out, K::KEY_SPACE, Mods::NONE);
        }
    }
    (out, skipped)
}

fn tap(out: &mut Vec<KeyStroke>, key: K, mods: Mods) {
    let held: Vec<K> = [
        (mods.shift, K::KEY_LEFTSHIFT),
        (mods.altgr, K::KEY_RIGHTALT),
    ]
    .into_iter()
    .filter_map(|(on, k)| on.then_some(k))
    .collect();
    out.extend(held.iter().map(|&k| (k, true)));
    out.extend([(key, true), (key, false)]);
    out.extend(held.iter().rev().map(|&k| (k, false)));
}

/// Press all `keys` in order (`true`) or release them in reverse (`false`).
pub fn chord(keys: &[K], press: bool) -> Vec<KeyStroke> {
    if press {
        keys.iter().map(|&k| (k, true)).collect()
    } else {
        keys.iter().rev().map(|&k| (k, false)).collect()
    }
}

/// Resolve combo key names to key codes. Single characters follow the text
/// layout when they need no modifier, so `ctrl` + `z` is Undo on QWERTZ too.
/// Returns the codes and the names that could not be resolved.
pub fn resolve_keys(names: &[String], layout: &TextLayout) -> (Vec<K>, Vec<String>) {
    let mut codes = Vec::new();
    let mut unknown = Vec::new();
    for name in names {
        let n = name.trim().to_lowercase();
        let mut chars = n.chars();
        let single = chars.next().filter(|_| chars.next().is_none());
        let by_layout = single
            .and_then(|c| layout.lookup(c))
            .filter(|(_, m)| m.is_none())
            .map(|(k, _)| k);
        match by_layout.or_else(|| named_key(&n)) {
            Some(k) => codes.push(k),
            None => unknown.push(name.clone()),
        }
    }
    (codes, unknown)
}

#[rustfmt::skip]
fn named_key(n: &str) -> Option<K> {
    Some(match n {
        "ctrl" | "lctrl" | "control" => K::KEY_LEFTCTRL,
        "rctrl" => K::KEY_RIGHTCTRL,
        "alt" | "lalt" => K::KEY_LEFTALT,
        "ralt" | "altgr" => K::KEY_RIGHTALT,
        "shift" | "lshift" => K::KEY_LEFTSHIFT,
        "rshift" => K::KEY_RIGHTSHIFT,
        "super" | "win" | "meta" | "cmd" => K::KEY_LEFTMETA,
        "tab" => K::KEY_TAB,
        "enter" | "return" => K::KEY_ENTER,
        "esc" | "escape" => K::KEY_ESC,
        "space" => K::KEY_SPACE,
        "backspace" => K::KEY_BACKSPACE,
        "delete" | "del" => K::KEY_DELETE,
        "insert" | "ins" => K::KEY_INSERT,
        "capslock" | "caps_lock" => K::KEY_CAPSLOCK,
        "menu" => K::KEY_COMPOSE,
        "printscreen" => K::KEY_SYSRQ,
        "pause" => K::KEY_PAUSE,
        "home" => K::KEY_HOME,
        "end" => K::KEY_END,
        "pageup" | "pgup" => K::KEY_PAGEUP,
        "pagedown" | "pgdn" => K::KEY_PAGEDOWN,
        "up" => K::KEY_UP,
        "down" => K::KEY_DOWN,
        "left" => K::KEY_LEFT,
        "right" => K::KEY_RIGHT,
        // US key positions for symbols that need a modifier on the layout
        "-" => K::KEY_MINUS, "=" => K::KEY_EQUAL, "[" => K::KEY_LEFTBRACE, "]" => K::KEY_RIGHTBRACE,
        "\\" => K::KEY_BACKSLASH, ";" => K::KEY_SEMICOLON, "'" => K::KEY_APOSTROPHE,
        "," => K::KEY_COMMA, "." => K::KEY_DOT, "/" => K::KEY_SLASH, "`" => K::KEY_GRAVE,
        _ => return function_key(n).or_else(|| ascii_key(n)),
    })
}

fn function_key(n: &str) -> Option<K> {
    let num: u16 = n.strip_prefix('f')?.parse().ok()?;
    // KEY_F1..F10 = 59..68, F11/F12 = 87/88, F13..F24 = 183..194
    let code = match num {
        1..=10 => 58 + num,
        11 | 12 => 76 + num,
        13..=24 => 170 + num,
        _ => return None,
    };
    Some(K(code))
}

fn ascii_key(n: &str) -> Option<K> {
    let mut chars = n.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    match c {
        'a'..='z' => Some(LETTERS[usize::from(c as u8 - b'a')]),
        '0'..='9' => Some(DIGITS[usize::from(c as u8 - b'0')]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn altgr_and_shift_wrap_the_key() {
        let (ev, skipped) = text_strokes("@A", &TextLayout::new("at", ""));
        assert!(skipped.is_empty());
        assert_eq!(
            ev,
            [
                (K::KEY_RIGHTALT, true),
                (K::KEY_Q, true),
                (K::KEY_Q, false),
                (K::KEY_RIGHTALT, false),
                (K::KEY_LEFTSHIFT, true),
                (K::KEY_A, true),
                (K::KEY_A, false),
                (K::KEY_LEFTSHIFT, false),
            ]
        );
    }

    #[test]
    fn dead_key_gets_space() {
        let (ev, _) = text_strokes("^", &TextLayout::new("de", ""));
        assert_eq!(
            ev,
            [
                (K::KEY_GRAVE, true),
                (K::KEY_GRAVE, false),
                (K::KEY_SPACE, true),
                (K::KEY_SPACE, false)
            ]
        );
        let (ev, _) = text_strokes("^", &TextLayout::new("de", "nodeadkeys"));
        assert_eq!(ev.len(), 2);
    }

    #[test]
    fn reports_untypeable_chars() {
        assert_eq!(text_strokes("a☃b", &TextLayout::new("us", "")).1, ['☃']);
    }

    #[test]
    fn combo_letters_follow_layout() {
        let at = TextLayout::new("at", "");
        let names = ["ctrl", "z"].map(String::from);
        assert_eq!(resolve_keys(&names, &at).0, [K::KEY_LEFTCTRL, K::KEY_Y]);
        // '[' needs AltGr on QWERTZ → US position
        assert_eq!(resolve_keys(&["[".into()], &at).0, [K::KEY_LEFTBRACE]);
        assert_eq!(
            resolve_keys(&["f13".into(), "F5".into()], &at).0,
            [K::KEY_F13, K::KEY_F5]
        );
        assert_eq!(resolve_keys(&["nope".into()], &at).1, ["nope"]);
    }

    #[test]
    fn chord_releases_in_reverse() {
        let keys = [K::KEY_LEFTCTRL, K::KEY_C];
        assert_eq!(
            chord(&keys, false),
            [(K::KEY_C, false), (K::KEY_LEFTCTRL, false)]
        );
    }
}
