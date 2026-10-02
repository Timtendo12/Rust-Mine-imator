//! Item meshes: a texture as a flat card or extruded pixel by pixel
//! (`render_generate_item`, `vbuffer_add_pixels`).

use crate::Rgba;
use mi_mesh::MeshData;

/// Width of an item in world units (`item_size`).
pub const ITEM_SIZE: f64 = 16.0;

/// Texture coordinates end this fraction of a pixel early, which the
/// original does against bleeding from neighbouring pixels.
const PIXEL_FIX: f64 = 1.0 / 256.0;

/// The mesh of an item drawn from the whole of `image`. Flat items are a
/// two-sided card at Y = 0; 3D items are one unit thick with sides wherever
/// an opaque pixel borders a transparent one.
pub fn item_mesh(image: &Rgba, is_3d: bool) -> MeshData {
    let mut mesh = MeshData::default();
    let (w, h) = (image.width as f64, image.height as f64);
    if w < 1.0 || h < 1.0 {
        return mesh;
    }
    // Size of the texture in pixels and in texture coordinates, a little
    // short of the full image.
    let tex_size = [1.0 - PIXEL_FIX / w, 1.0 - PIXEL_FIX / h];
    let size_px = [w - PIXEL_FIX, h - PIXEL_FIX];

    // The longer side is `ITEM_SIZE`.
    let mut size = [ITEM_SIZE, if is_3d { 1.0 } else { 0.0 }, ITEM_SIZE];
    if size_px[0] > size_px[1] {
        size[2] *= size_px[1] / size_px[0];
    } else if size_px[1] > size_px[0] {
        size[0] *= size_px[0] / size_px[1];
    }

    let p = |x: f64, y: f64, z: f64| [x as f32, y as f32, z as f32];
    let t = |u: f64, v: f64| [u as f32, v as f32];
    // Front
    let front = [p(0.0, size[1], size[2]), p(size[0], size[1], size[2]), p(size[0], size[1], 0.0), p(0.0, size[1], 0.0)];
    let uv = [t(0.0, 0.0), t(tex_size[0], 0.0), t(tex_size[0], tex_size[1]), t(0.0, tex_size[1])];
    mesh.triangle([front[0], front[1], front[2]], [uv[0], uv[1], uv[2]], false);
    mesh.triangle([front[2], front[3], front[0]], [uv[2], uv[3], uv[0]], false);
    // Back
    let back = [p(size[0], 0.0, size[2]), p(0.0, 0.0, size[2]), p(0.0, 0.0, 0.0), p(size[0], 0.0, 0.0)];
    let uv = [t(tex_size[0], 0.0), t(0.0, 0.0), t(0.0, tex_size[1]), t(tex_size[0], tex_size[1])];
    mesh.triangle([back[0], back[1], back[2]], [uv[0], uv[1], uv[2]], false);
    mesh.triangle([back[2], back[3], back[0]], [uv[2], uv[3], uv[0]], false);

    if is_3d {
        let scale = [size[0] / size_px[0], 1.0, size[2] / size_px[1]];
        let pixel = [tex_size[0] / size_px[0], tex_size[1] / size_px[1]];
        let solid = |x: i64, y: i64| {
            x >= 0
                && y >= 0
                && (x as u32) < image.width
                && (y as u32) < image.height
                && image.pixels[((y as u32 * image.width + x as u32) * 4 + 3) as usize] == 255
        };
        add_pixels(&mut mesh, image.width, image.height, &solid, size[2], size_px, pixel, scale);
    }
    mesh
}

/// Side faces of the opaque pixels of an image (`vbuffer_add_pixels`):
/// between Y = 0 and Y = 1, from Z = `height` downwards, in pixel units
/// multiplied by `scale`.
#[allow(clippy::too_many_arguments)]
fn add_pixels(
    mesh: &mut MeshData,
    width: u32,
    height_px: u32,
    solid: &dyn Fn(i64, i64) -> bool,
    height: f64,
    tex_size: [f64; 2],
    tex_pixel: [f64; 2],
    scale: [f64; 3],
) {
    // The last pixel of each row and column is cut short like the texture.
    let end_x = tex_size[0].fract();
    let end_y = tex_size[1].fract();
    let last_x = tex_size[0].ceil() as i64 - 1;
    let last_y = tex_size[1].ceil() as i64 - 1;

    let mut px = 0.0;
    for x in 0..width as i64 {
        let pxs = if x == width as i64 - 1 && end_x > 0.0 { end_x } else { 1.0 };
        let mut pz = height / scale[2];
        for y in 0..height_px as i64 {
            let pzs = if y == height_px as i64 - 1 && end_y > 0.0 { end_y } else { 1.0 };
            if !solid(x, y) {
                pz -= pzs;
                continue;
            }
            let west = x == 0 || !solid(x - 1, y);
            let east = x == last_x || !solid(x + 1, y);
            let above = y == 0 || !solid(x, y - 1);
            let below = y == last_y || !solid(x, y + 1);

            let psize = 1.0 - PIXEL_FIX;
            let (u, v) = (x as f64, y as f64);
            let uv = [[u, v], [u + psize, v], [u + psize, v + psize], [u, v + psize]]
                .map(|c| [(c[0] * tex_pixel[0]) as f32, (c[1] * tex_pixel[1]) as f32]);
            let mut quad = |corners: [[f64; 3]; 4]| {
                let c = corners.map(|c| [(c[0] * scale[0]) as f32, (c[1] * scale[1]) as f32, (c[2] * scale[2]) as f32]);
                mesh.triangle([c[0], c[1], c[2]], [uv[0], uv[1], uv[2]], false);
                mesh.triangle([c[2], c[3], c[0]], [uv[2], uv[3], uv[0]], false);
            };
            if east {
                quad([[px + pxs, 1.0, pz - pzs], [px + pxs, 1.0, pz], [px + pxs, 0.0, pz], [px + pxs, 0.0, pz - pzs]]);
            }
            if west {
                quad([[px, 0.0, pz - pzs], [px, 0.0, pz], [px, 1.0, pz], [px, 1.0, pz - pzs]]);
            }
            if above {
                quad([[px, 1.0, pz], [px, 0.0, pz], [px + pxs, 0.0, pz], [px + pxs, 1.0, pz]]);
            }
            if below {
                quad([[px, 0.0, pz - pzs], [px, 1.0, pz - pzs], [px + pxs, 1.0, pz - pzs], [px + pxs, 0.0, pz - pzs]]);
            }
            pz -= pzs;
        }
        px += pxs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An image whose listed pixels are opaque.
    fn image(width: u32, height: u32, opaque: &[(u32, u32)]) -> Rgba {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for &(x, y) in opaque {
            let i = ((y * width + x) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&[255; 4]);
        }
        Rgba { width, height, pixels }
    }

    fn bounds(mesh: &MeshData) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for v in &mesh.vertices {
            for i in 0..3 {
                min[i] = min[i].min(v.position[i]);
                max[i] = max[i].max(v.position[i]);
            }
        }
        (min, max)
    }

    #[test]
    fn flat_items_are_a_two_sided_card() {
        let mesh = item_mesh(&image(16, 16, &[(0, 0)]), false);
        assert_eq!(mesh.triangle_count(), 4);
        let (min, max) = bounds(&mesh);
        assert_eq!((min, max), ([0.0, 0.0, 0.0], [16.0, 0.0, 16.0]));
    }

    #[test]
    fn items_keep_their_proportions() {
        // Twice as wide as tall: the height shrinks.
        let (_, max) = bounds(&item_mesh(&image(32, 16, &[]), false));
        assert!((max[0] - 16.0).abs() < 1e-3 && (max[2] - 8.0).abs() < 0.01, "{max:?}");
        let (_, max) = bounds(&item_mesh(&image(16, 32, &[]), false));
        assert!((max[2] - 16.0).abs() < 1e-3 && (max[0] - 8.0).abs() < 0.01, "{max:?}");
    }

    #[test]
    fn opaque_pixels_get_sides_where_they_border_transparency() {
        // One pixel: four sides.
        let one = item_mesh(&image(16, 16, &[(5, 5)]), true);
        assert_eq!(one.triangle_count(), 4 + 4 * 2);
        let (min, max) = bounds(&one);
        assert_eq!((min[1], max[1]), (0.0, 1.0));
        // Two neighbours share a side: six sides.
        let two = item_mesh(&image(16, 16, &[(5, 5), (6, 5)]), true);
        assert_eq!(two.triangle_count(), 4 + 6 * 2);
        // The pixel at (5, 5) is one unit wide and the top row is at the top.
        let sides: Vec<[f32; 3]> = one.vertices[12..].iter().map(|v| v.position).collect();
        let xs: Vec<f32> = sides.iter().map(|p| p[0]).collect();
        let zs: Vec<f32> = sides.iter().map(|p| p[2]).collect();
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(xs.iter().all(|&x| near(x, 5.0) || near(x, 6.0)), "{xs:?}");
        assert!(zs.iter().all(|&z| near(z, 11.0) || near(z, 10.0)), "{zs:?}");
        // Partly transparent pixels are not extruded.
        let mut soft = image(16, 16, &[(5, 5)]);
        soft.pixels[(5 * 16 + 5) * 4 + 3] = 128;
        assert_eq!(item_mesh(&soft, true).triangle_count(), 4);
    }
}
