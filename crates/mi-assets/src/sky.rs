//! Geometry and textures of the sky: the cloud layer generated from the
//! clouds texture (`background_sky_update_clouds`) and the phases of the
//! moon (`res_load_pack_misc`).

use crate::Rgba;
use mi_mesh::{MeshData, Vertex};

/// How clouds are drawn (`background_sky_clouds_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloudMode {
    /// Blocks, shaded by side.
    Normal,
    /// A single flat layer.
    Flat,
    /// Blocks that fade out towards the top (Story Mode style).
    Faded,
}

impl CloudMode {
    /// Unknown names count as normal.
    pub fn from_name(name: &str) -> Self {
        match name {
            "flat" => CloudMode::Flat,
            "faded" => CloudMode::Faded,
            _ => CloudMode::Normal,
        }
    }
}

type Rgb = [f32; 3];

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

const BOTTOM: Rgb = rgb(174, 181, 193);
const TOP: Rgb = rgb(255, 255, 255);
const SIDES_LIGHT: Rgb = rgb(215, 222, 234);
const SIDES_DARK: Rgb = rgb(194, 201, 215);

/// Number of moon phases in `moon_phases.png`: four columns, two rows.
pub const MOON_PHASES: u32 = 8;

/// One phase of the moon cut out of the texture of all phases.
pub fn moon_phase(phases: &Rgba, phase: u32) -> Option<Rgba> {
    let (w, h) = ((phases.width / 4).max(1), (phases.height / 2).max(1));
    let phase = phase % MOON_PHASES;
    let (x0, y0) = ((phase % 4) * w, (phase / 4) * h);
    if x0 + w > phases.width || y0 + h > phases.height {
        return None;
    }
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in y0..y0 + h {
        let start = ((y * phases.width + x0) * 4) as usize;
        pixels.extend_from_slice(&phases.pixels[start..start + (w * 4) as usize]);
    }
    Some(Rgba { width: w, height: h, pixels })
}

/// One tile of the cloud layer, `size` units wide, from the clouds texture:
/// every opaque pixel is a block `thickness` high, of which the sides next
/// to transparent pixels are built (the texture wraps around), between one
/// bottom and one top face for the whole tile. Flat clouds are only those
/// two faces.
pub fn clouds_mesh(texture: &Rgba, size: f32, thickness: f32, mode: CloudMode) -> MeshData {
    let (width, height) = (texture.width as usize, texture.height as usize);
    let mut mesh = MeshData::default();
    if width == 0 || height == 0 {
        return mesh;
    }
    let (sides_dark, sides_light, top, bottom, top_alpha) = match mode {
        CloudMode::Faded => ([1.0; 3], [1.0; 3], [1.0; 3], [1.0; 3], 0.0),
        _ => (SIDES_DARK, SIDES_LIGHT, TOP, BOTTOM, 1.0),
    };
    let mut add = |position: [f32; 3], normal: [f32; 3], uv: [f32; 2], color: Rgb, alpha: f32| {
        mesh.vertices.push(Vertex { position, normal, uv, color: [color[0], color[1], color[2], alpha], custom: [0.0; 4] });
    };

    let mut thick = 0.0;
    if mode != CloudMode::Flat {
        thick = thickness;
        let solid = |x: usize, y: usize| texture.pixels[(y * width + x) * 4 + 3] == 255;
        let (pw, ph) = (1.0 / width as f32, 1.0 / height as f32);
        let (bw, bh) = (size / width as f32, size / height as f32);
        for xx in 0..width {
            for yy in 0..height {
                if !solid(xx, yy) {
                    continue;
                }
                let (vx, vy) = (xx as f32 * bw, yy as f32 * bh);
                let (tx, ty) = (xx as f32 * pw, yy as f32 * ph);
                // A side: two corners at the bottom, two at the top.
                let mut side = |a: [f32; 2], b: [f32; 2], normal: [f32; 3], color: Rgb| {
                    add([a[0], a[1], 0.0], normal, [tx, ty], color, 1.0);
                    add([a[0], a[1], thick], normal, [tx, ty + ph], color, top_alpha);
                    add([b[0], b[1], thick], normal, [tx + pw, ty + ph], color, top_alpha);
                    add([b[0], b[1], thick], normal, [tx + pw, ty + ph], color, top_alpha);
                    add([b[0], b[1], 0.0], normal, [tx + pw, ty], color, 1.0);
                    add([a[0], a[1], 0.0], normal, [tx, ty], color, 1.0);
                };
                if !solid((xx + 1) % width, yy) {
                    side([vx + bw, vy + bh], [vx + bw, vy], [1.0, 0.0, 0.0], sides_dark);
                }
                if !solid((xx + width - 1) % width, yy) {
                    side([vx, vy], [vx, vy + bh], [-1.0, 0.0, 0.0], sides_dark);
                }
                if !solid(xx, (yy + 1) % height) {
                    side([vx, vy + bh], [vx + bw, vy + bh], [0.0, 1.0, 0.0], sides_light);
                }
                if !solid(xx, (yy + height - 1) % height) {
                    side([vx + bw, vy], [vx, vy], [0.0, -1.0, 0.0], sides_light);
                }
            }
        }
    }

    // Flat clouds show the top colour from below.
    let under = if mode == CloudMode::Flat { top } else { bottom };
    let down = [0.0, 0.0, -1.0];
    add([0.0, 0.0, 0.0], down, [0.0, 0.0], under, 1.0);
    add([0.0, size, 0.0], down, [0.0, 1.0], under, 1.0);
    add([size, size, 0.0], down, [1.0, 1.0], under, 1.0);
    add([size, size, 0.0], down, [1.0, 1.0], under, 1.0);
    add([size, 0.0, 0.0], down, [1.0, 0.0], under, 1.0);
    add([0.0, 0.0, 0.0], down, [0.0, 0.0], under, 1.0);

    let up = [0.0, 0.0, 1.0];
    add([0.0, 0.0, thick], up, [0.0, 0.0], top, top_alpha);
    add([size, 0.0, thick], up, [1.0, 0.0], top, top_alpha);
    add([size, size, thick], up, [1.0, 1.0], top, top_alpha);
    add([size, size, thick], up, [1.0, 1.0], top, top_alpha);
    add([0.0, size, thick], up, [0.0, 1.0], top, top_alpha);
    add([0.0, 0.0, thick], up, [0.0, 0.0], top, top_alpha);
    mesh
}

/// Where the tiles of the cloud layer are (`background_sky_update`): a grid
/// around the camera that reaches as far as the fog, drifting along -Y.
/// `drift` is how far the clouds have moved, in units.
pub fn cloud_positions(camera: [f32; 2], size: f32, fog_distance: f32, drift: f32) -> Vec<[f32; 2]> {
    if size <= 0.0 {
        return Vec::new();
    }
    // GameMaker's `mod` and `div` truncate towards zero.
    let offset = drift % size;
    let xo = (camera[0] / size).trunc() * size;
    let yo = (camera[1] / size).trunc() * size - offset;
    let reach = ((fog_distance / size).ceil() + 1.0) * size;
    let mut positions = Vec::new();
    let mut x = -reach;
    while x < reach {
        let mut y = -reach;
        while y < reach {
            positions.push([x + xo, y + yo]);
            y += size;
        }
        x += size;
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2×2 texture with one opaque pixel at (0, 0) and one half transparent.
    fn texture() -> Rgba {
        let mut pixels = vec![0; 16];
        pixels[0..4].copy_from_slice(&[255; 4]);
        pixels[4..8].copy_from_slice(&[255, 255, 255, 128]);
        Rgba { width: 2, height: 2, pixels }
    }

    #[test]
    fn one_block_has_four_sides_between_the_tile_faces() {
        let mesh = clouds_mesh(&texture(), 32.0, 4.0, CloudMode::Normal);
        // Four sides of the single opaque pixel, plus bottom and top.
        assert_eq!(mesh.triangle_count(), 4 * 2 + 4);
        let xs: Vec<f32> = mesh.vertices[..24].iter().map(|v| v.position[0]).collect();
        assert!(xs.iter().all(|&x| (0.0..=16.0).contains(&x)));
        // Sides facing along X are darker than those along Y.
        assert_eq!(mesh.vertices[0].color, [SIDES_DARK[0], SIDES_DARK[1], SIDES_DARK[2], 1.0]);
        assert_eq!(mesh.vertices[12].color[..3], SIDES_LIGHT);
        // The tile faces span the whole tile.
        let bottom = &mesh.vertices[24..30];
        assert!(bottom.iter().all(|v| v.position[2] == 0.0 && v.color[..3] == BOTTOM));
        let top = &mesh.vertices[30..];
        assert!(top.iter().all(|v| v.position[2] == 4.0 && v.color == [1.0; 4]));
        assert!(top.iter().any(|v| v.position[0] == 32.0 && v.position[1] == 32.0));
    }

    #[test]
    fn flat_and_faded_clouds() {
        let flat = clouds_mesh(&texture(), 32.0, 4.0, CloudMode::Flat);
        assert_eq!(flat.triangle_count(), 4);
        assert!(flat.vertices.iter().all(|v| v.position[2] == 0.0 && v.color == [1.0; 4]));

        let faded = clouds_mesh(&texture(), 32.0, 4.0, CloudMode::Faded);
        assert_eq!(faded.triangle_count(), 12);
        // White, opaque at the bottom and transparent at the top.
        assert!(faded.vertices.iter().all(|v| v.color[..3] == [1.0; 3] && v.color[3] == if v.position[2] == 0.0 { 1.0 } else { 0.0 }));
        assert_eq!(CloudMode::from_name("faded"), CloudMode::Faded);
        assert_eq!(CloudMode::from_name("anything"), CloudMode::Normal);
    }

    #[test]
    fn tiles_surround_the_camera_and_drift() {
        let tiles = cloud_positions([100.0, -5000.0], 1000.0, 2500.0, 0.0);
        // Four tiles each way, from the camera's tile.
        assert_eq!(tiles.len(), 8 * 8);
        assert_eq!(tiles[0], [-4000.0, -9000.0]);
        assert_eq!(*tiles.last().unwrap(), [3000.0, -2000.0]);
        // The drift wraps around at the tile size.
        let moved = cloud_positions([100.0, -5000.0], 1000.0, 2500.0, 1250.0);
        assert_eq!(moved[0], [-4000.0, -9250.0]);
        assert!(cloud_positions([0.0, 0.0], 0.0, 100.0, 0.0).is_empty());
    }

    #[test]
    fn moon_phases_are_cut_from_a_four_by_two_sheet() {
        // 8×4 sheet: the red channel holds the phase.
        let mut pixels = Vec::new();
        for y in 0..4u32 {
            for x in 0..8u32 {
                pixels.extend_from_slice(&[((y / 2) * 4 + x / 2) as u8, x as u8, y as u8, 255]);
            }
        }
        let sheet = Rgba { width: 8, height: 4, pixels };
        for phase in 0..8 {
            let moon = moon_phase(&sheet, phase).unwrap();
            assert_eq!((moon.width, moon.height), (2, 2));
            assert!(moon.pixels.chunks(4).all(|p| p[0] == phase as u8));
        }
        assert_eq!(moon_phase(&sheet, 9), moon_phase(&sheet, 1));
    }
}
