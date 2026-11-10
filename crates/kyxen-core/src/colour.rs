//! `#rrggbb` colours as stored in profile files.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Rgb;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Colour(pub Rgb);

#[derive(Debug, thiserror::Error)]
#[error("invalid colour {0:?} (expected #rrggbb)")]
pub struct ColourError(String);

impl FromStr for Colour {
    type Err = ColourError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.trim().trim_start_matches('#');
        let bad = || ColourError(s.to_owned());
        if hex.len() != 6 || !hex.is_ascii() {
            return Err(bad());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        Ok(Self(Rgb(byte(0)?, byte(2)?, byte(4)?)))
    }
}

impl fmt::Display for Colour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Rgb(r, g, b) = self.0;
        write!(f, "#{r:02x}{g:02x}{b:02x}")
    }
}

impl Serialize for Colour {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Colour {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints() {
        let c: Colour = "#00CC44".parse().unwrap();
        assert_eq!(c.0, Rgb(0x00, 0xcc, 0x44));
        assert_eq!(c.to_string(), "#00cc44");
        assert!("#12345".parse::<Colour>().is_err());
        assert!("#gg0000".parse::<Colour>().is_err());
    }
}
