//! Text objects: the Minecraft sprite font and the image and mesh a text
//! timeline is drawn with (`new_minecraft_font`, `SpriteFont`,
//! `render_generate_text`).

use crate::Rgba;
use mi_mesh::MeshData;

/// Characters of the font image, in the order of its glyphs.
const CHARACTERS: &str = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~¿ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒÓÔÕÖ×ØÙÚÛÜÝÞßàáâãäåæçèéêëìíîïðñòóôõö÷øùúûüýþÿỳ";

/// Size of a glyph cell in the font image.
const CELL: (u32, u32) = (9, 12);

/// Space between characters (`font_add_sprite_ext(..., 1)`).
const SEPARATION: i32 = 1;

/// Where a text is anchored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    /// Left or top.
    Start,
    #[default]
    Center,
    /// Right or bottom.
    End,
}

impl Align {
    /// From the names project files use for `TEXT_HALIGN` and
    /// `TEXT_VALIGN`.
    pub fn from_name(name: &str) -> Self {
        match name {
            "left" | "top" => Align::Start,
            "right" | "bottom" => Align::End,
            _ => Align::Center,
        }
    }
}

/// A font made of fixed-size glyph cells, proportionally spaced.
#[derive(Debug, Clone)]
pub struct SpriteFont {
    image: Rgba,
    /// Distance from one character to the next, per glyph.
    advance: Vec<i32>,
    /// Distance between lines: two less than the cell height.
    pub height: i32,
}

impl SpriteFont {
    /// The Minecraft font from its glyph strip (`Data/Fonts/minecraft.png`):
    /// one cell per character, left to right.
    pub fn minecraft(png: &[u8]) -> Option<Self> {
        let image = image::load_from_memory(png).ok()?.to_rgba8();
        let (width, height) = image.dimensions();
        let count = CHARACTERS.chars().count() as u32;
        if height != CELL.1 || width != CELL.0 * count {
            return None;
        }
        let image = Rgba { width, height, pixels: image.into_raw() };
        // A character is as wide as the columns it uses, at least one.
        let advance = (0..count)
            .map(|glyph| {
                let used: Vec<u32> = (0..CELL.0)
                    .filter(|&x| (0..CELL.1).any(|y| image.pixels[((y * width + glyph * CELL.0 + x) * 4 + 3) as usize] > 0))
                    .collect();
                let used_width = match (used.first(), used.last()) {
                    (Some(first), Some(last)) => (last + 1 - first) as i32,
                    _ => 0,
                };
                used_width.max(1) + SEPARATION
            })
            .collect();
        Some(Self { image, advance, height: CELL.1 as i32 - 2 })
    }

    fn glyph(&self, c: char) -> Option<usize> {
        CHARACTERS.chars().position(|g| g == c)
    }

    /// Width of a line of text; characters the font lacks take no room.
    pub fn line_width(&self, line: &str) -> i32 {
        line.chars().filter_map(|c| self.glyph(c)).map(|g| self.advance[g]).sum()
    }

    /// Width of the widest line.
    pub fn text_width(&self, text: &str) -> i32 {
        text.split('\n').map(|line| self.line_width(line)).max().unwrap_or(0)
    }

    pub fn text_height(&self, text: &str) -> i32 {
        if text.is_empty() {
            0
        } else {
            self.height * (1 + text.matches('\n').count() as i32)
        }
    }

    /// Draws a line with its top left corner at (`x`, `y`) of `target`.
    fn draw_line(&self, target: &mut Rgba, line: &str, x: i32, y: i32) {
        let mut pen = x;
        for c in line.chars() {
            let Some(glyph) = self.glyph(c) else { continue };
            for gy in 0..CELL.1 as i32 {
                for gx in 0..CELL.0 as i32 {
                    let (tx, ty) = (pen + gx, y + gy);
                    if tx < 0 || ty < 0 || tx >= target.width as i32 || ty >= target.height as i32 {
                        continue;
                    }
                    let source = ((gy as u32 * self.image.width + glyph as u32 * CELL.0 + gx as u32) * 4) as usize;
                    let alpha = self.image.pixels[source + 3];
                    if alpha == 0 {
                        continue;
                    }
                    let dest = ((ty as u32 * target.width + tx as u32) * 4) as usize;
                    // Text is drawn white; colour comes from the timeline.
                    target.pixels[dest..dest + 4].copy_from_slice(&[255, 255, 255, alpha]);
                }
            }
            pen += self.advance[glyph];
        }
    }
}

/// A text ready to be drawn: its image and where the image sits relative to
/// the timeline's position, in pixels (X to the right, Z up).
#[derive(Debug, Clone)]
pub struct TextImage {
    pub image: Rgba,
    pub x: f64,
    pub z: f64,
}

/// Lays out `text` (`render_generate_text`). Returns `None` for empty text.
pub fn text_image(font: &SpriteFont, text: &str, halign: Align, valign: Align) -> Option<TextImage> {
    if text.is_empty() {
        return None;
    }
    // A trailing line break still makes a line.
    let mut text = text.to_owned();
    if text.ends_with('\n') {
        text.push(' ');
    }
    let width = font.text_width(&text) + 3;
    let height = font.text_height(&text) + 1;
    let mut image = Rgba { width: width as u32, height: height as u32, pixels: vec![0; (width * height * 4) as usize] };

    let anchor = match halign {
        Align::Start => 0,
        Align::Center => (width + 1) / 2,
        Align::End => width,
    };
    // `draw_text_ext` at y = -1, and the font's own offset of -1.
    let mut y = -2;
    for line in text.split('\n') {
        let line_width = font.line_width(line);
        let x = match halign {
            Align::Start => anchor,
            Align::Center => anchor - line_width / 2,
            Align::End => anchor - line_width,
        };
        font.draw_line(&mut image, line, x, y);
        y += font.height;
    }

    let (w, h) = (width as f64, height as f64);
    let x = match halign {
        Align::Start => 0.0,
        Align::Center => -w / 2.0 - 1.0,
        Align::End => -w,
    };
    let z = match valign {
        Align::Start => -h,
        Align::Center => -h / 2.0 - 1.0,
        Align::End => 0.0,
    };
    Some(TextImage { image, x, z })
}

/// The two-sided plane a text image is drawn on (`render_generate_text_buffer`
/// without the 3D pixels).
pub fn text_mesh(text: &TextImage) -> MeshData {
    let (x1, z1) = (text.x as f32, text.z as f32);
    let (x2, z2) = (x1 + text.image.width as f32, z1 + text.image.height as f32);
    let p = [[x1, 0.0, z2], [x2, 0.0, z2], [x2, 0.0, z1], [x1, 0.0, z1]];
    let t = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut mesh = MeshData::default();
    // Front
    mesh.triangle([p[0], p[1], p[2]], [t[0], t[1], t[2]], false);
    mesh.triangle([p[2], p[3], p[0]], [t[2], t[3], t[0]], false);
    // Back
    mesh.triangle([p[1], p[0], p[2]], [t[1], t[0], t[2]], false);
    mesh.triangle([p[3], p[2], p[0]], [t[3], t[2], t[0]], false);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> SpriteFont {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/Data/Fonts/minecraft.png");
        SpriteFont::minecraft(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn characters_are_as_wide_as_what_they_draw() {
        let font = font();
        assert_eq!(font.height, 10);
        // Width used plus one pixel between characters.
        assert_eq!(font.line_width("A"), 6);
        assert_eq!(font.line_width("i"), 2);
        assert_eq!(font.line_width("l"), 3);
        // The empty space glyph counts as one pixel wide, as in the original.
        assert_eq!(font.line_width(" "), 2);
        assert_eq!(font.line_width("Ail"), 11);
        // Characters the font does not have are skipped.
        assert_eq!(font.line_width("A☃"), 6);
        assert_eq!(font.text_width("A\nAA"), 12);
        assert_eq!(font.text_height("A\nAA"), 20);
        assert_eq!(font.text_height(""), 0);
    }

    #[test]
    fn text_images_are_padded_and_anchored() {
        let font = font();
        assert!(text_image(&font, "", Align::Center, Align::Center).is_none());
        let text = text_image(&font, "Hi", Align::Center, Align::Center).unwrap();
        // Width + 3 and height + 1.
        assert_eq!((text.image.width, text.image.height), (font.line_width("Hi") as u32 + 3, 11));
        assert_eq!((text.x, text.z), (-(text.image.width as f64) / 2.0 - 1.0, -11.0 / 2.0 - 1.0));
        assert!(text.image.pixels.chunks_exact(4).any(|p| p[3] > 0), "something is drawn");
        assert!(text.image.pixels.chunks_exact(4).all(|p| p[3] == 0 || p[..3] == [255, 255, 255]));

        let left = text_image(&font, "Hi", Align::Start, Align::End).unwrap();
        assert_eq!((left.x, left.z), (0.0, 0.0));
        let right = text_image(&font, "Hi", Align::End, Align::Start).unwrap();
        assert_eq!((right.x, right.z), (-(right.image.width as f64), -11.0));

        // Two lines, and a trailing break makes a third.
        assert_eq!(text_image(&font, "a\nb", Align::Center, Align::Center).unwrap().image.height, 21);
        assert_eq!(text_image(&font, "a\nb\n", Align::Center, Align::Center).unwrap().image.height, 31);

        let mesh = text_mesh(&text);
        assert_eq!(mesh.triangle_count(), 4);
        assert!(mesh.vertices.iter().all(|v| v.position[1] == 0.0));
    }
}
