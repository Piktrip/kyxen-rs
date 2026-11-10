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

    /// The G815 / G915 full-size layout.
    pub fn g815() -> &'static Self {
        static MAP: OnceLock<Keymap> = OnceLock::new();
        MAP.get_or_init(|| Self::build(g815::KEYS))
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

    #[test]
    fn g815_map_is_consistent() {
        let map = Keymap::g815();
        assert_eq!(map.get("A").and_then(|k| k.led), Some(0x01));
        assert_eq!(map.get("G1").and_then(|k| k.led), Some(0xb4));
        assert!(map.lit().count() > 100);
        let mut leds: Vec<u8> = map.lit().map(|(_, l)| l).collect();
        leds.sort_unstable();
        let unique = leds.len();
        leds.dedup();
        assert_eq!(leds.len(), unique, "duplicate LED ids");
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
