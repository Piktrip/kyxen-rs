//! Physical key geometry and LED zone ids.

mod g815;

use std::collections::HashMap;
use std::sync::OnceLock;

/// One physical key: name (as used in profile files), LED zone id and
/// position in key units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyDef {
    pub name: &'static str,
    pub led: Option<u8>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Physical key arrangement: ISO (tall Enter, `<>` next to left Shift) or
/// ANSI (wide Enter, `\` above it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Form {
    #[default]
    Iso,
    Ansi,
}

impl Form {
    /// The form from the `keyboard_form` setting (`"iso"` / `"ansi"`), else
    /// guessed from the text layout: US keyboards are ANSI.
    pub fn resolve(setting: Option<&str>, layout: &str) -> Self {
        match setting {
            Some("ansi") => Self::Ansi,
            Some("iso") => Self::Iso,
            _ if layout == "us" => Self::Ansi,
            _ => Self::Iso,
        }
    }
}

/// A keyboard's keys with lookup helpers.
#[derive(Debug)]
pub struct Keymap {
    pub keys: &'static [KeyDef],
    by_name: HashMap<&'static str, usize>,
    /// Key centres normalised to 0.0 ..= 1.0 (left→right, top→bottom).
    centres: Vec<(f32, f32)>,
}

impl Keymap {
    fn build(keys: &'static [KeyDef]) -> Self {
        let x0 = keys.iter().map(|k| k.x).fold(f32::INFINITY, f32::min);
        let y0 = keys.iter().map(|k| k.y).fold(f32::INFINITY, f32::min);
        let x1 = keys
            .iter()
            .map(|k| k.x + k.w)
            .fold(f32::NEG_INFINITY, f32::max);
        let y1 = keys
            .iter()
            .map(|k| k.y + k.h)
            .fold(f32::NEG_INFINITY, f32::max);
        let (sx, sy) = ((x1 - x0).max(1e-3), (y1 - y0).max(1e-3));
        Self {
            keys,
            by_name: keys.iter().enumerate().map(|(i, k)| (k.name, i)).collect(),
            centres: keys
                .iter()
                .map(|k| ((k.x + k.w / 2.0 - x0) / sx, (k.y + k.h / 2.0 - y0) / sy))
                .collect(),
        }
    }

    /// The G815 / G915 full-size layout, ISO form (DE, AT, UK …).
    pub fn g815() -> &'static Self {
        static MAP: OnceLock<Keymap> = OnceLock::new();
        MAP.get_or_init(|| Self::build(g815::KEYS))
    }

    /// The G815 / G915 in ANSI form (US): wide Enter, wide left Shift, `\`
    /// above Enter, no `<>` key.
    pub fn g815_ansi() -> &'static Self {
        static MAP: OnceLock<Keymap> = OnceLock::new();
        static KEYS: OnceLock<Vec<KeyDef>> = OnceLock::new();
        MAP.get_or_init(|| Self::build(KEYS.get_or_init(|| g815::ansi(g815::KEYS))))
    }

    /// The G815 / G915 layout for `form`.
    pub fn g815_for(form: Form) -> &'static Self {
        match form {
            Form::Iso => Self::g815(),
            Form::Ansi => Self::g815_ansi(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&KeyDef> {
        self.by_name.get(name).map(|&i| &self.keys[i])
    }

    /// Normalised centre of `name`, or the middle of the board if unknown.
    pub fn centre(&self, name: &str) -> (f32, f32) {
        self.by_name
            .get(name)
            .map_or((0.5, 0.5), |&i| self.centres[i])
    }

    /// Keys that have an LED.
    pub fn lit(&self) -> impl Iterator<Item = (&'static str, u8)> + '_ {
        self.keys.iter().filter_map(|k| Some((k.name, k.led?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_leds(map: &Keymap) {
        let mut leds: Vec<u8> = map.lit().map(|(_, l)| l).collect();
        leds.sort_unstable();
        let unique = leds.len();
        leds.dedup();
        assert_eq!(leds.len(), unique, "duplicate LED ids");
    }

    #[test]
    fn ansi_map_reshapes_enter_and_shift() {
        let iso = Keymap::g815();
        let ansi = Keymap::g815_ansi();
        unique_leds(ansi);
        assert!(ansi.get("HASH").is_none() && ansi.get("BACKSLASH").is_none());
        assert_eq!(ansi.keys.len(), iso.keys.len() - 1);
        let bs = ansi.get("ANSI_BACKSLASH").expect("backslash above Enter");
        let rb = ansi.get("R_BRACKET").unwrap();
        assert_eq!((bs.y, bs.x), (rb.y, rb.x + rb.w));
        let enter = ansi.get("RETURN").unwrap();
        let quote = ansi.get("APOSTROPHE").unwrap();
        assert_eq!((enter.y, enter.x), (quote.y, quote.x + quote.w));
        // rows still end flush with the right Shift
        let right = |k: &KeyDef| k.x + k.w;
        let rs = right(ansi.get("R_SHIFT").unwrap());
        assert!((right(bs) - rs).abs() < 1e-3 && (right(enter) - rs).abs() < 1e-3);
        assert_eq!(
            right(ansi.get("L_SHIFT").unwrap()),
            ansi.get("Z").unwrap().x
        );
        assert_eq!(Form::resolve(None, "us"), Form::Ansi);
        assert_eq!(Form::resolve(None, "de"), Form::Iso);
        assert_eq!(Form::resolve(Some("iso"), "us"), Form::Iso);
    }

    #[test]
    fn g815_map_is_consistent() {
        let map = Keymap::g815();
        unique_leds(map);
        assert_eq!(map.get("A").and_then(|k| k.led), Some(0x01));
        assert_eq!(map.get("G1").and_then(|k| k.led), Some(0xb4));
        assert!(map.lit().count() > 100);
        let (lx, _) = map.centre("ESC");
        let (rx, _) = map.centre("NUM_MINUS");
        assert!(lx < rx);
        assert!(
            map.keys
                .iter()
                .all(|k| (0.0..=1.0).contains(&map.centre(k.name).0))
        );
    }
}
