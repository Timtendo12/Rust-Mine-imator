//! Player skins (`res_load_player_skin`): skins in the layout from before
//! Minecraft 1.8, which have one arm and one leg, are completed with
//! mirrored copies for the other side.

use crate::{decode_square, Rgba};

/// Decodes a player skin and brings it into the 1.8 layout. The image is
/// padded to a square; if it was not square, or its lower half is empty,
/// the left arm and leg are made from the right ones.
pub fn player_skin(bytes: &[u8]) -> Option<Rgba> {
    let dimensions = image::load_from_memory(bytes).ok()?;
    let was_square = dimensions.width() == dimensions.height();
    let mut skin = decode_square(bytes)?;
    let size = skin.width;

    let alpha = |skin: &Rgba, x: u32, y: u32| skin.pixels[((y * size + x) * 4 + 3) as usize];
    let lower_half_used = was_square && (0..size).any(|x| (size / 2..size).any(|y| alpha(&skin, x, y) > 0));
    if lower_half_used {
        return Some(skin);
    }

    // Skins are 64 pixels wide, or a multiple of that.
    let scale = size / 64;
    if scale == 0 {
        return Some(skin);
    }
    let source = skin.clone();
    // Copies a part mirrored: `right` is the destination's right edge.
    let mut mirror = |right: u32, top: u32, from_x: u32, from_y: u32, w: u32, h: u32| {
        let (right, top, from_x, from_y, w, h) = (right * scale, top * scale, from_x * scale, from_y * scale, w * scale, h * scale);
        for y in 0..h {
            for x in 0..w {
                let from = (((from_y + y) * size + from_x + x) * 4) as usize;
                let to = (((top + y) * size + right - 1 - x) * 4) as usize;
                skin.pixels[to..to + 4].copy_from_slice(&source.pixels[from..from + 4]);
            }
        }
    };
    // Leg: outer side, front, inner side, back, top, bottom.
    mirror(28, 52, 0, 20, 4, 12);
    mirror(24, 52, 4, 20, 4, 12);
    mirror(20, 52, 8, 20, 4, 12);
    mirror(32, 52, 12, 20, 4, 12);
    mirror(24, 48, 4, 16, 4, 4);
    mirror(28, 48, 8, 16, 4, 4);
    // Arm
    mirror(44, 52, 40, 20, 4, 12);
    mirror(40, 52, 44, 20, 4, 12);
    mirror(36, 52, 48, 20, 4, 12);
    mirror(48, 52, 52, 20, 4, 12);
    mirror(40, 48, 44, 16, 4, 4);
    mirror(44, 48, 48, 16, 4, 4);
    Some(skin)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba(pixel(x, y)));
        let mut bytes = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        bytes
    }

    fn at(skin: &Rgba, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * skin.width + x) * 4) as usize;
        skin.pixels[i..i + 4].try_into().unwrap()
    }

    /// A pixel that says where it is.
    fn coded(x: u32, y: u32) -> [u8; 4] {
        [x as u8, y as u8, 7, 255]
    }

    #[test]
    fn old_skins_get_a_mirrored_arm_and_leg() {
        let skin = player_skin(&png(64, 32, coded)).unwrap();
        assert_eq!((skin.width, skin.height), (64, 64));
        // The upper half is unchanged.
        assert_eq!(at(&skin, 5, 21), coded(5, 21));
        // Leg front: source columns 4..8 land mirrored in 20..24.
        assert_eq!(at(&skin, 23, 52), coded(4, 20));
        assert_eq!(at(&skin, 20, 63), coded(7, 31));
        // The outer side (0..4) ends up on the other side of the front.
        assert_eq!(at(&skin, 27, 52), coded(0, 20));
        // Arm back: 52..56 to 44..48; arm top: 44..48 to 36..40.
        assert_eq!(at(&skin, 47, 52), coded(52, 20));
        assert_eq!(at(&skin, 39, 48), coded(44, 16));
        // Nothing else is put in the lower half.
        assert_eq!(at(&skin, 2, 40)[3], 0);
        assert_eq!(at(&skin, 60, 60)[3], 0);
    }

    #[test]
    fn square_skins_with_a_lower_half_are_kept() {
        let bytes = png(64, 64, coded);
        assert_eq!(player_skin(&bytes), decode_square(&bytes));
        // A square image whose lower half is empty is an old skin on a
        // bigger canvas.
        let padded = player_skin(&png(64, 64, |x, y| if y < 32 { coded(x, y) } else { [0; 4] })).unwrap();
        assert_eq!(at(&padded, 23, 52), coded(4, 20));
        // High resolution skins scale the layout.
        let big = player_skin(&png(128, 64, coded)).unwrap();
        assert_eq!(big.width, 128);
        assert_eq!(at(&big, 47, 104), coded(8, 40));
        assert!(player_skin(b"not an image").is_none());
    }
}
