//! Mesh data and the generators for the built-in shapes
//! (`vbuffer_add_triangle`, `vbuffer_create_*`, `background_ground_startup`).
//!
//! Meshes are plain triangle lists, like the vertex buffers of the original.
//! The cross product of a triangle's edges (first to second, first to third
//! corner) points along its normal. Because the world is left-handed, that
//! makes front faces clockwise on screen.

use bytemuck::{Pod, Zeroable};
use std::f32::consts::PI;

/// One vertex, matching the original vertex format (`vertex_format_startup`)
/// except for the tangent, which is not generated yet.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    /// Wind sway on XY, wind sway on Z, emissive, subsurface (`in_Wave`).
    pub custom: [f32; 4],
}

/// A triangle list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
}

impl MeshData {
    pub fn triangle_count(&self) -> usize {
        self.vertices.len() / 3
    }

    /// `vertex_add` with an explicit normal.
    fn vertex(&mut self, position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) {
        let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        let normal = if length > 0.0 { normal.map(|n| n / length) } else { normal };
        self.vertices.push(Vertex { position, normal, uv, color: [1.0; 4], custom: [0.0; 4] });
    }

    /// `vbuffer_add_triangle` in its coordinate form: a flat triangle whose
    /// normal follows from its corners. `invert` flips it.
    fn triangle(&mut self, p: [[f32; 3]; 3], uv: [[f32; 2]; 3], invert: bool) {
        let [p1, p2, p3] = p;
        let mut normal = [
            (p1[2] - p2[2]) * (p3[1] - p2[1]) - (p1[1] - p2[1]) * (p3[2] - p2[2]),
            (p1[0] - p2[0]) * (p3[2] - p2[2]) - (p1[2] - p2[2]) * (p3[0] - p2[0]),
            (p1[1] - p2[1]) * (p3[0] - p2[0]) - (p1[0] - p2[0]) * (p3[1] - p2[1]),
        ];
        if invert {
            normal = normal.map(|n| -n);
            self.vertex(p2, normal, uv[1]);
            self.vertex(p1, normal, uv[0]);
        } else {
            self.vertex(p1, normal, uv[0]);
            self.vertex(p2, normal, uv[1]);
        }
        self.vertex(p3, normal, uv[2]);
    }
}

/// Settings of a shape template that affect its mesh (`temp_update_shape`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeSettings {
    pub tex_mapped: bool,
    pub tex_hoffset: f32,
    pub tex_voffset: f32,
    pub tex_hrepeat: f32,
    pub tex_vrepeat: f32,
    pub tex_hmirror: bool,
    pub tex_vmirror: bool,
    pub closed: bool,
    pub invert: bool,
    /// Number of segments of round shapes.
    pub detail: u32,
}

impl Default for ShapeSettings {
    fn default() -> Self {
        Self {
            tex_mapped: false,
            tex_hoffset: 0.0,
            tex_voffset: 0.0,
            tex_hrepeat: 1.0,
            tex_vrepeat: 1.0,
            tex_hmirror: false,
            tex_vmirror: false,
            closed: true,
            invert: false,
            detail: 32,
        }
    }
}

/// The built-in shapes (`e_shape_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    Cube,
    Cone,
    Cylinder,
    Sphere,
    Surface,
}

/// Half the size of every shape: shapes are one block (16 units) wide.
pub const SHAPE_RADIUS: f32 = 8.0;

struct TexRect {
    tex1: [f32; 2],
    tex2: [f32; 2],
    hflip: f32,
    vflip: f32,
}

fn tex_rect(s: &ShapeSettings) -> TexRect {
    // `negate(x)` is -1 for true and 1 for false.
    let mut hflip = if s.tex_hmirror { -1.0 } else { 1.0 };
    let vflip = if s.tex_vmirror { -1.0 } else { 1.0 };
    if s.invert {
        hflip = -hflip;
    }
    let mut tex1 = [s.tex_hoffset, s.tex_voffset];
    let mut tex2 = [s.tex_hoffset + s.tex_hrepeat, s.tex_voffset + s.tex_vrepeat];
    if hflip < 0.0 {
        tex1[0] = 1.0 - tex1[0];
        tex2[0] = 1.0 - tex2[0];
    }
    if vflip < 0.0 {
        tex1[1] = 1.0 - tex1[1];
        tex2[1] = 1.0 - tex2[1];
    }
    TexRect { tex1, tex2, hflip, vflip }
}

/// Builds the mesh of a shape template.
pub fn shape_mesh(shape: Shape, settings: &ShapeSettings) -> MeshData {
    let rect = tex_rect(settings);
    let detail = settings.detail.max(3);
    match shape {
        Shape::Surface => surface(SHAPE_RADIUS, &rect, settings.invert),
        Shape::Cube => cube(SHAPE_RADIUS, &rect, settings.invert, settings.tex_mapped),
        Shape::Cone => round(SHAPE_RADIUS, rect, detail, settings, true),
        Shape::Cylinder => round(SHAPE_RADIUS, rect, detail, settings, false),
        Shape::Sphere => sphere(SHAPE_RADIUS, &rect, detail, settings.invert),
    }
}

/// `vbuffer_create_surface`: an upright square facing +Y.
fn surface(r: f32, rect: &TexRect, invert: bool) -> MeshData {
    let (t1, t2) = (rect.tex1, rect.tex2);
    let mut mesh = MeshData::default();
    mesh.triangle([[-r, 0.0, r], [r, 0.0, r], [r, 0.0, -r]], [[t1[0], t1[1]], [t2[0], t1[1]], [t2[0], t2[1]]], invert);
    mesh.triangle([[-r, 0.0, -r], [-r, 0.0, r], [r, 0.0, -r]], [[t1[0], t2[1]], [t1[0], t1[1]], [t2[0], t2[1]]], invert);
    mesh
}

/// `vbuffer_create_cube`. With `mapped`, each face takes its own cell of a
/// 3×2 texture layout.
fn cube(r: f32, rect: &TexRect, invert: bool, mapped: bool) -> MeshData {
    let mut mesh = MeshData::default();
    let cell = [1.0 / 3.0, 1.0 / 2.0];
    let mut t1 = rect.tex1;
    let mut t2 = rect.tex2;

    // Texture cell of a face when mapped, with mirroring applied.
    let face_cell = |origin: [f32; 2]| -> ([f32; 2], [f32; 2]) {
        let mut a = origin;
        let mut b = [origin[0] + cell[0], origin[1] + cell[1]];
        if rect.hflip < 0.0 {
            std::mem::swap(&mut a[0], &mut b[0]);
        }
        if rect.vflip < 0.0 {
            std::mem::swap(&mut a[1], &mut b[1]);
        }
        (a, b)
    };

    // X+
    if mapped {
        (t1, t2) = face_cell([if invert { cell[0] } else { cell[0] * 2.0 }, 0.0]);
    }
    mesh.triangle([[r, r, -r], [r, r, r], [r, -r, r]], [[t1[0], t2[1]], [t1[0], t1[1]], [t2[0], t1[1]]], invert);
    mesh.triangle([[r, r, -r], [r, -r, r], [r, -r, -r]], [[t1[0], t2[1]], [t2[0], t1[1]], [t2[0], t2[1]]], invert);

    // X-
    if mapped {
        (t1, t2) = face_cell([if invert { cell[0] * 2.0 } else { cell[0] }, 0.0]);
    }
    mesh.triangle([[-r, r, r], [-r, r, -r], [-r, -r, r]], [[t2[0], t1[1]], [t2[0], t2[1]], [t1[0], t1[1]]], invert);
    mesh.triangle([[-r, -r, r], [-r, r, -r], [-r, -r, -r]], [[t1[0], t1[1]], [t2[0], t2[1]], [t1[0], t2[1]]], invert);

    // Y+
    if mapped {
        (t1, t2) = face_cell([0.0, 0.0]);
    }
    mesh.triangle([[-r, r, r], [r, r, r], [r, r, -r]], [[t1[0], t1[1]], [t2[0], t1[1]], [t2[0], t2[1]]], invert);
    mesh.triangle([[-r, r, -r], [-r, r, r], [r, r, -r]], [[t1[0], t2[1]], [t1[0], t1[1]], [t2[0], t2[1]]], invert);

    // Y-
    if mapped {
        (t1, t2) = face_cell([0.0, cell[1]]);
    }
    mesh.triangle([[r, -r, r], [-r, -r, r], [r, -r, -r]], [[t1[0], t1[1]], [t2[0], t1[1]], [t1[0], t2[1]]], invert);
    mesh.triangle([[-r, -r, r], [-r, -r, -r], [r, -r, -r]], [[t2[0], t1[1]], [t2[0], t2[1]], [t1[0], t2[1]]], invert);

    // Z+
    if mapped {
        (t1, t2) = face_cell(cell);
    }
    mesh.triangle([[-r, -r, r], [r, -r, r], [-r, r, r]], [[t1[0], t1[1]], [t2[0], t1[1]], [t1[0], t2[1]]], invert);
    mesh.triangle([[r, -r, r], [r, r, r], [-r, r, r]], [[t2[0], t1[1]], [t2[0], t2[1]], [t1[0], t2[1]]], invert);

    // Z-
    if mapped {
        (t1, t2) = face_cell([cell[0] * 2.0, cell[1]]);
    }
    mesh.triangle([[r, -r, -r], [-r, -r, -r], [-r, r, -r]], [[t2[0], t2[1]], [t1[0], t2[1]], [t1[0], t1[1]]], invert);
    mesh.triangle([[r, r, -r], [r, -r, -r], [-r, r, -r]], [[t2[0], t1[1]], [t2[0], t2[1]], [t1[0], t1[1]]], invert);

    mesh
}

/// `vbuffer_create_cylinder` and `vbuffer_create_cone`, which differ only in
/// the top.
fn round(r: f32, rect: TexRect, detail: u32, s: &ShapeSettings, cone: bool) -> MeshData {
    let mut mesh = MeshData::default();
    let invert = s.invert;
    let mut tex1 = [rect.tex1[0] + 0.25, rect.tex1[1]];
    let mut tex2 = [rect.tex2[0] + 0.25, rect.tex2[1]];

    for step in 0..detail {
        let ip = step as f32 / detail as f32;
        let i = (step + 1) as f32 / detail as f32;
        let mut texsize = [tex2[0] - tex1[0], tex2[1] - tex1[1]];
        let mut texmid = [tex1[0] + texsize[0] / 2.0, tex1[1] + texsize[1] / 2.0];

        let (a1, a2) = (ip * PI * 2.0, i * PI * 2.0);
        let (mut n1, mut n2) = ([a1.cos(), -a1.sin()], [a2.cos(), -a2.sin()]);
        let (p1, p2) = ([n1[0] * r, n1[1] * r], [n2[0] * r, n2[1] * r]);
        if invert {
            n1 = n1.map(|n| -n);
            n2 = n2.map(|n| -n);
        }

        if s.tex_mapped {
            texsize = [if cone { 0.5 } else { 1.0 / 3.0 } * rect.hflip, rect.vflip];
            texmid[1] = texsize[1] / 2.0;
        }

        if s.closed {
            // Bottom
            if s.tex_mapped {
                texmid[0] = if cone { 3.0 / 4.0 } else { 5.0 / 6.0 };
            }
            let half = [texsize[0] / 2.0, texsize[1] / 2.0];
            mesh.triangle(
                [[0.0, 0.0, -r], [p1[0], p1[1], -r], [p2[0], p2[1], -r]],
                [
                    texmid,
                    [texmid[0] + a1.cos() * half[0], texmid[1] + a1.sin() * half[1]],
                    [texmid[0] + a2.cos() * half[0], texmid[1] + a2.sin() * half[1]],
                ],
                invert,
            );
            // Top
            if !cone {
                if s.tex_mapped {
                    texmid[0] = 1.0 / 2.0;
                }
                mesh.triangle(
                    [[0.0, 0.0, r], [p2[0], p2[1], r], [p1[0], p1[1], r]],
                    [
                        texmid,
                        [texmid[0] + a2.cos() * half[0], texmid[1] - a2.sin() * half[1]],
                        [texmid[0] + a1.cos() * half[0], texmid[1] - a1.sin() * half[1]],
                    ],
                    invert,
                );
            }
        }

        // Sides
        if s.tex_mapped {
            tex1 = [0.0, 0.0];
            tex2 = [texsize[0].abs(), texsize[1].abs()];
            if rect.hflip < 0.0 {
                std::mem::swap(&mut tex1[0], &mut tex2[0]);
            }
            if rect.vflip < 0.0 {
                std::mem::swap(&mut tex1[1], &mut tex2[1]);
            }
        }
        let u1 = tex1[0] + texsize[0] * ip;
        let u2 = tex1[0] + texsize[0] * i;
        let (v_top, v_bottom) = (tex1[1], tex1[1] + texsize[1]);
        let low1 = ([p1[0], p1[1], -r], [n1[0], n1[1], 0.0], [u1, v_bottom]);
        let low2 = ([p2[0], p2[1], -r], [n2[0], n2[1], 0.0], [u2, v_bottom]);

        let corners: Vec<([f32; 3], [f32; 3], [f32; 2])> = if cone {
            // The original leaves the tip normal pointing up on inverted
            // cones, which lights their inside wrongly; it is flipped here.
            let tip = ([0.0, 0.0, r], [0.0, 0.0, if invert { -1.0 } else { 1.0 }], [u2, v_top]);
            if invert {
                vec![low2, tip, low1]
            } else {
                vec![tip, low2, low1]
            }
        } else {
            let high1 = ([p1[0], p1[1], r], [n1[0], n1[1], 0.0], [u1, v_top]);
            let high2 = ([p2[0], p2[1], r], [n2[0], n2[1], 0.0], [u2, v_top]);
            if invert {
                vec![high1, low1, high2, low2, high2, low1]
            } else {
                vec![low1, high1, high2, high2, low2, low1]
            }
        };
        for (position, normal, uv) in corners {
            mesh.vertex(position, normal, uv);
        }
    }
    mesh
}

/// `vbuffer_create_sphere`
fn sphere(r: f32, rect: &TexRect, detail: u32, invert: bool) -> MeshData {
    let mut mesh = MeshData::default();
    let tex1 = [rect.tex1[0] + 0.25, rect.tex1[1]];
    let tex2 = [rect.tex2[0] + 0.25, rect.tex2[1]];
    let texsize = [tex2[0] - tex1[0], tex2[1] - tex1[1]];
    let texmid_y = tex1[1] + texsize[1] / 2.0;
    let sign = if invert { -1.0 } else { 1.0 };
    let rings = detail - 2;

    for step in 0..detail {
        let ip = step as f32 / detail as f32;
        let i = (step + 1) as f32 / detail as f32;
        for ring in 0..rings {
            let jp = ring as f32 / rings as f32;
            let j = (ring + 1) as f32 / rings as f32;

            let normal = |around: f32, down: f32| -> [f32; 3] {
                let (a, d) = (around * PI * 2.0, down * PI);
                [a.sin() * d.sin(), -a.cos() * d.sin(), -d.cos()]
            };
            let corner = |around: f32, down: f32| -> ([f32; 3], [f32; 3], [f32; 2]) {
                let n = normal(around, down);
                (n.map(|c| c * r), n.map(|c| c * sign), [tex2[0] - around * texsize[0], texmid_y - n[2] * (texsize[1] / 2.0)])
            };
            let (c1, c2, c3, c4) = (corner(ip, jp), corner(ip, j), corner(i, jp), corner(i, j));

            let mut push = |corners: [([f32; 3], [f32; 3], [f32; 2]); 3]| {
                for (position, normal, uv) in corners {
                    mesh.vertex(position, normal, uv);
                }
            };
            // The first ring at each pole is a fan of single triangles.
            if ring > 0 {
                push(if invert { [c3, c1, c4] } else { [c1, c3, c4] });
            }
            if ring + 1 < rings {
                push(if invert { [c4, c1, c2] } else { [c1, c4, c2] });
            }
        }
    }
    mesh
}

/// The ground: a grid of large quads just below zero, with the texture
/// repeating once per block (`background_ground_startup`).
pub fn ground_mesh(render_distance: f32) -> MeshData {
    let total = (render_distance / 256.0).floor() * 256.0;
    let size = total / 16.0;
    let repeat = size / 16.0;
    let mut mesh = MeshData::default();
    if size <= 0.0 {
        return mesh;
    }
    let up = [0.0, 0.0, 1.0];
    let z = -0.01;
    for xi in -16..16 {
        for yi in -16..16 {
            let (x, y) = (xi as f32 * size, yi as f32 * size);
            mesh.vertex([x, y, z], up, [0.0, 0.0]);
            mesh.vertex([x + size, y, z], up, [repeat, 0.0]);
            mesh.vertex([x, y + size, z], up, [0.0, repeat]);
            mesh.vertex([x + size, y, z], up, [repeat, 0.0]);
            mesh.vertex([x + size, y + size, z], up, [repeat, repeat]);
            mesh.vertex([x, y + size, z], up, [0.0, repeat]);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    }

    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    /// The edge cross product of every triangle must agree with its stored
    /// normal, and for closed shapes that normal must point away from the centre
    /// (or towards it when inverted).
    fn check(mesh: &MeshData, outward: f32, name: &str) {
        assert_eq!(mesh.vertices.len() % 3, 0, "{name}");
        for (index, tri) in mesh.vertices.chunks(3).enumerate() {
            let face = cross(sub(tri[1].position, tri[0].position), sub(tri[2].position, tri[0].position));
            let area = dot(face, face).sqrt();
            if area < 1e-4 {
                continue;
            }
            for v in tri {
                assert!(dot(face, v.normal) > 0.0, "{name}: triangle {index} winds against its normal");
                assert!((dot(v.normal, v.normal) - 1.0).abs() < 1e-3, "{name}: normal not unit length");
            }
            let centre = [0, 1, 2].map(|k| (tri[0].position[k] + tri[1].position[k] + tri[2].position[k]) / 3.0);
            assert!(dot(face, centre) * outward > 0.0, "{name}: triangle {index} faces the wrong way");
        }
    }

    #[test]
    fn shapes_face_outwards() {
        let settings = ShapeSettings::default();
        for shape in [Shape::Cube, Shape::Cone, Shape::Cylinder, Shape::Sphere] {
            check(&shape_mesh(shape, &settings), 1.0, &format!("{shape:?}"));
        }
    }

    #[test]
    fn inverted_shapes_face_inwards() {
        let settings = ShapeSettings { invert: true, ..Default::default() };
        for shape in [Shape::Cube, Shape::Cone, Shape::Cylinder, Shape::Sphere] {
            check(&shape_mesh(shape, &settings), -1.0, &format!("inverted {shape:?}"));
        }
    }

    #[test]
    fn triangle_counts() {
        let s = ShapeSettings { detail: 8, ..Default::default() };
        assert_eq!(shape_mesh(Shape::Cube, &s).triangle_count(), 12);
        assert_eq!(shape_mesh(Shape::Surface, &s).triangle_count(), 2);
        // Per segment: two side triangles plus a bottom and a top one.
        assert_eq!(shape_mesh(Shape::Cylinder, &s).triangle_count(), 8 * 4);
        assert_eq!(shape_mesh(Shape::Cone, &s).triangle_count(), 8 * 2);
        let open = ShapeSettings { closed: false, ..s };
        assert_eq!(shape_mesh(Shape::Cylinder, &open).triangle_count(), 8 * 2);
        assert_eq!(shape_mesh(Shape::Cone, &open).triangle_count(), 8);
        // 8 segments × 6 rings, the pole rings having one triangle each.
        assert_eq!(shape_mesh(Shape::Sphere, &s).triangle_count(), 8 * (6 * 2 - 2));
    }

    #[test]
    fn shapes_fit_in_a_block() {
        for shape in [Shape::Cube, Shape::Cone, Shape::Cylinder, Shape::Sphere, Shape::Surface] {
            let mesh = shape_mesh(shape, &ShapeSettings::default());
            for v in &mesh.vertices {
                assert!(v.position.iter().all(|c| c.abs() <= SHAPE_RADIUS + 1e-4), "{shape:?}");
            }
        }
    }

    #[test]
    fn surface_faces_positive_y() {
        let mesh = shape_mesh(Shape::Surface, &ShapeSettings::default());
        assert!(mesh.vertices.iter().all(|v| v.normal == [0.0, 1.0, 0.0]));
        let uvs: Vec<[f32; 2]> = mesh.vertices.iter().map(|v| v.uv).collect();
        assert!(uvs.contains(&[0.0, 0.0]) && uvs.contains(&[1.0, 1.0]));
    }

    #[test]
    fn mapped_cube_uses_a_three_by_two_layout() {
        let mesh = shape_mesh(Shape::Cube, &ShapeSettings { tex_mapped: true, ..Default::default() });
        for v in &mesh.vertices {
            let on_grid = |value: f32, cells: f32| ((value * cells).round() - value * cells).abs() < 1e-4;
            assert!(on_grid(v.uv[0], 3.0) && on_grid(v.uv[1], 2.0), "{:?}", v.uv);
        }
    }

    #[test]
    fn texture_repeat_and_offset() {
        let s = ShapeSettings { tex_hrepeat: 4.0, tex_voffset: 0.5, ..Default::default() };
        let mesh = shape_mesh(Shape::Surface, &s);
        let max_u = mesh.vertices.iter().map(|v| v.uv[0]).fold(f32::MIN, f32::max);
        let min_v = mesh.vertices.iter().map(|v| v.uv[1]).fold(f32::MAX, f32::min);
        assert_eq!(max_u, 4.0);
        assert_eq!(min_v, 0.5);
    }

    #[test]
    fn ground_covers_the_render_distance() {
        let mesh = ground_mesh(30000.0);
        assert_eq!(mesh.triangle_count(), 32 * 32 * 2);
        let extent = mesh.vertices.iter().map(|v| v.position[0]).fold(f32::MIN, f32::max);
        assert_eq!(extent, 29952.0);
        check_ground(&mesh);
        assert_eq!(ground_mesh(100.0).triangle_count(), 0);
    }

    fn check_ground(mesh: &MeshData) {
        for tri in mesh.vertices.chunks(3) {
            let face = cross(sub(tri[1].position, tri[0].position), sub(tri[2].position, tri[0].position));
            assert!(face[2] > 0.0, "ground triangles face up");
        }
    }
}
