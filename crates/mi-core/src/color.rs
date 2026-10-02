//! 8-bit RGB colours.
//!
//! The original stores colours as GameMaker integers (`b << 16 | g << 8 | r`)
//! and writes them to files as `#RRGGBB`. A handful of animatable values are
//! stored as that raw integer instead (see [`Color::from_gm`]).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    /// GameMaker's `c_gray`.
    pub const GRAY: Color = Color::rgb(128, 128, 128);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parses `#RRGGBB` / `RRGGBB` the way `hex_to_color` does: the `#` is
    /// optional, case is ignored, short strings are padded with zeroes and
    /// characters that are not hex digits count as zero.
    pub fn from_hex(s: &str) -> Self {
        let mut digits = [0u8; 6];
        for (slot, c) in digits.iter_mut().zip(s.trim_start_matches('#').chars()) {
            *slot = c.to_digit(16).unwrap_or(0) as u8;
        }
        Self::rgb(
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
        )
    }

    /// Formats as `#RRGGBB` (upper case), matching `json_save_var_color`.
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// Converts from a GameMaker colour integer (`0xBBGGRR`).
    pub fn from_gm(v: u32) -> Self {
        Self::rgb((v & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, ((v >> 16) & 0xFF) as u8)
    }

    /// Converts to a GameMaker colour integer (`0xBBGGRR`).
    pub fn to_gm(self) -> u32 {
        (self.b as u32) << 16 | (self.g as u32) << 8 | self.r as u32
    }

    /// Linear per-channel blend, equivalent to GameMaker's `merge_color`.
    pub fn merge(self, other: Color, amount: f64) -> Color {
        let mix = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * amount) as u8;
        Color::rgb(mix(self.r, other.r), mix(self.g, other.g), mix(self.b, other.b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let c = Color::from_hex("#78A7FF");
        assert_eq!(c, Color::rgb(120, 167, 255));
        assert_eq!(c.to_hex(), "#78A7FF");
        assert_eq!(Color::from_hex("b2353b"), Color::rgb(0xB2, 0x35, 0x3B));
    }

    #[test]
    fn hex_is_lenient() {
        assert_eq!(Color::from_hex("#FF"), Color::rgb(255, 0, 0));
        assert_eq!(Color::from_hex(""), Color::BLACK);
        assert_eq!(Color::from_hex("#GG00FF"), Color::rgb(0, 0, 255));
    }

    #[test]
    fn gm_integer_is_bgr() {
        let c = Color::rgb(1, 2, 3);
        assert_eq!(c.to_gm(), 0x030201);
        assert_eq!(Color::from_gm(0x030201), c);
    }

    #[test]
    fn merge_endpoints() {
        let a = Color::rgb(0, 100, 200);
        let b = Color::rgb(200, 100, 0);
        assert_eq!(a.merge(b, 0.0), a);
        assert_eq!(a.merge(b, 1.0), b);
        assert_eq!(a.merge(b, 0.5), Color::rgb(100, 100, 100));
    }
}
