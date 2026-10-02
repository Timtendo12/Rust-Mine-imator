//! Meshes of model shapes, straight or bent (`model_shape_generate_block`,
//! `model_shape_generate_plane`).
//!
//! A bent shape is cut into segments along the bend direction; each cut is
//! transformed by a share of the bend angle. With the "blocky" bend style
//! the joint is a single sharp fold whose sides are widened so that the limb
//! keeps its thickness; with the "realistic" style the fold is spread over
//! several segments.

use crate::model_file::{Bend, ModelShape, ShapeKind, Wave};
use mi_anim::math::Mat4;
use mi_anim::scene::BendPart;
use mi_anim::{Transition, Vec3};
use mi_mesh::MeshData;

/// How joints are shaped (`project_bend_style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BendStyle {
    Blocky,
    Realistic,
}

impl BendStyle {
    /// Unknown names count as blocky, the default.
    pub fn from_name(name: &str) -> Self {
        if name == "realistic" {
            BendStyle::Realistic
        } else {
            BendStyle::Blocky
        }
    }
}

type Uv = [f64; 2];

const X: usize = 0;
const Y: usize = 1;
const Z: usize = 2;

/// Everything about the bend of the part a shape belongs to that the
/// generators need.
struct BendSetup<'a> {
    bend: &'a Bend,
    /// Angles after limiting them to the part's range.
    angles: Vec3,
    sharp: bool,
    size: f64,
    segment_size: f64,
    /// Whether the bent half is the one at the start of the segment axis.
    inverted: bool,
    start: f64,
    end: f64,
}

/// `model_shape_get_bend`: the angles a share `weight` of the way through
/// the joint. Rotation around the bend direction itself is spread evenly,
/// the others eased.
fn bend_at(bend: Vec3, weight: f64) -> Vec3 {
    let eased = Transition::EaseInOutQuint.ease(weight);
    [bend[X] * eased, bend[Y] * eased, bend[Z] * weight]
}

/// `model_shape_get_bend_scale`: how much a cut inside a sharp fold is
/// widened, so that the folded limb keeps its thickness.
fn bend_scale(setup: &BendSetup, weight: f64, position: f64) -> Vec3 {
    if !(position > setup.start && position < setup.end) {
        return [0.0; 3];
    }
    let base = if weight <= 0.5 { weight * 2.0 } else { (1.0 - weight) * 2.0 };
    let axis = setup.bend.axis.iter().position(|&a| a).unwrap_or(X);
    let mut angle = setup.angles[axis].abs();
    if angle > 90.0 {
        angle -= (angle - 90.0) * 2.0;
    }
    let amount = (angle / 90.0).clamp(0.0, 1.0);
    let scale = Transition::EaseInCubic.ease(base * amount) / 2.5;
    let mut out = [scale; 3];
    out[axis] = 0.0;
    out
}

/// `model_part_get_bend_matrix` for a shape: bends around the joint, taking
/// the shape's own position and rotation into account.
fn shape_bend_matrix(shape: &ModelShape, bend: &Bend, angles: Vec3, scale: Vec3) -> Mat4 {
    let mut limited = [0.0; 3];
    for i in X..=Z {
        if angles[i] == 0.0 || !bend.axis[i] {
            continue;
        }
        limited[i] = angles[i].clamp(bend.direction_min[i], bend.direction_max[i]);
        if bend.invert[i] {
            limited[i] = -limited[i];
        }
    }
    let axis = match bend.part {
        BendPart::Right | BendPart::Left => X,
        BendPart::Front | BendPart::Back => Y,
        BendPart::Upper | BendPart::Lower => Z,
    };
    let mut pivot = [0.0; 3];
    pivot[axis] = bend.offset - shape.position[axis];
    let back = [-pivot[X], -pivot[Y], -pivot[Z]];
    Mat4::build(back, shape.rotation, [1.0; 3]).then(&Mat4::build(pivot, limited, scale))
}

fn plain_matrix(shape: &ModelShape) -> Mat4 {
    Mat4::build([0.0; 3], shape.rotation, [1.0; 3])
}

fn bend_setup<'a>(shape: &ModelShape, bend: Option<&'a Bend>, angles: Vec3, style: BendStyle) -> Option<BendSetup<'a>> {
    let bend = bend?;
    if !shape.bend_shape {
        return None;
    }
    // Limit to the part's range first (`model_part_fill_shape_vbuffer_map`).
    let mut limited = angles;
    for (i, angle) in limited.iter_mut().enumerate() {
        *angle = angle.clamp(bend.direction_min[i], bend.direction_max[i]);
    }
    if limited == [0.0; 3] {
        return None;
    }

    let single_axis = bend.axis.iter().filter(|&&a| a).count() == 1;
    let sharp = style == BendStyle::Blocky && bend.size.is_none() && single_axis;
    let size = bend.size.unwrap_or(if style == BendStyle::Realistic { 4.0 } else { 1.0 });
    Some(BendSetup {
        bend,
        angles: limited,
        sharp,
        size,
        segment_size: 0.0,
        inverted: matches!(bend.part, BendPart::Lower | BendPart::Back | BendPart::Left),
        start: 0.0,
        end: 0.0,
    })
}

impl BendSetup<'_> {
    /// Places the bent section along the segment axis of `shape`.
    fn place(&mut self, shape: &ModelShape, axis: usize) {
        let mut detail = if self.sharp { 2.0 } else { self.size.max(2.0) };
        if self.bend.size.is_some_and(|s| s >= 1.0) && shape.scale[axis] > 0.5 {
            detail /= shape.scale[axis];
        }
        self.segment_size = self.size / detail;
        let centre = self.bend.offset - (shape.position[axis] + shape.from[axis]);
        self.start = centre - self.size / 2.0;
        self.end = centre + self.size / 2.0;
    }

    /// Share of the bend angle that applies at `position` along the axis.
    fn weight(&self, position: f64, at_start: bool) -> f64 {
        let weight = if at_start {
            if self.start > 0.0 {
                0.0
            } else if self.end < 0.0 {
                1.0
            } else {
                1.0 - self.end / self.size
            }
        } else if position < self.start {
            0.0
        } else if position >= self.end {
            1.0
        } else {
            1.0 - (self.end - position) / self.size
        };
        if self.inverted {
            1.0 - weight
        } else {
            weight
        }
    }

    /// Length of the next segment starting at `position`.
    fn segment(&self, position: f64, total: f64, from: f64) -> f64 {
        if position >= self.end {
            total - position
        } else if position < self.start {
            (total - position).min(self.start)
        } else {
            let mut size = self.segment_size;
            if position == 0.0 {
                // Starting inside the bend: align the cuts with the joint.
                size -= (from - self.start) % self.segment_size;
            }
            (total - position).min(size)
        }
    }
}

fn custom(shape: &ModelShape, z: f64) -> [f32; 4] {
    let in_range = shape.wave_zmin.is_none_or(|min| z > min) && shape.wave_zmax.is_none_or(|max| z < max);
    let (xy, zz) = match shape.wave {
        Wave::None => (0.0, 0.0),
        _ if !in_range => (0.0, 0.0),
        Wave::All => (1.0, 1.0),
        Wave::ZOnly => (0.0, 1.0),
    };
    [xy, zz, shape.color.emissive as f32, 0.0]
}

struct Builder<'a> {
    mesh: MeshData,
    shape: &'a ModelShape,
}

impl Builder<'_> {
    fn triangle(&mut self, p: [Vec3; 3], uv: [Uv; 3], normals: Option<[Vec3; 3]>) {
        let f = |v: Vec3| v.map(|c| c as f32);
        self.mesh.triangle_with(
            p.map(f),
            uv.map(|t| t.map(|c| c as f32)),
            normals.map(|n| n.map(f)),
            self.shape.invert,
            p.map(|v| custom(self.shape, v[Z])),
        );
    }
}

/// Texture size of a box in texture units, optionally snapped to whole
/// pixels for Bedrock-style UVs.
fn tex_size(shape: &ModelShape) -> Vec3 {
    let mut size = [
        shape.to_noscale[X] - shape.from_noscale[X],
        shape.to_noscale[Y] - shape.from_noscale[Y],
        shape.to_noscale[Z] - shape.from_noscale[Z],
    ];
    if shape.floor_box_uvs {
        for value in &mut size {
            *value = if value.fract() != 0.0 { (*value + 0.000001).floor() } else { value.floor() };
        }
    }
    size
}

fn tex_origin(shape: &ModelShape) -> Uv {
    let uv = if shape.floor_box_uvs { [shape.uv[0].floor(), shape.uv[1].floor()] } else { shape.uv };
    [uv[0] / shape.texture_size[0], uv[1] / shape.texture_size[1]]
}

/// Builds the mesh of a shape. `bend` is the bend of the part the shape
/// belongs to and `angles` the current bend angles; pass zero angles for the
/// resting mesh.
///
/// 3D planes need the texture to know their outline and are generated as
/// flat planes here.
pub fn shape_mesh(shape: &ModelShape, bend: Option<&Bend>, angles: Vec3, style: BendStyle) -> MeshData {
    let setup = bend_setup(shape, bend, angles, style);
    match shape.kind {
        ShapeKind::Block => block(shape, setup),
        ShapeKind::Plane => plane(shape, setup),
    }
}

fn block(shape: &ModelShape, mut setup: Option<BendSetup>) -> MeshData {
    let (x1, y1, z1) = (shape.from[X], shape.from[Y], shape.from[Z]);
    let (x2, y2, z2) = (shape.to[X], shape.to[Y], shape.to[Z]);
    let size = [x2 - x1, y2 - y1, z2 - z1];
    // Later segments are scaled up by a hair to avoid Z-fighting at folds.
    let scalef = 0.005;

    let segaxis = match setup.as_ref().map(|s| s.bend.part) {
        Some(BendPart::Left | BendPart::Right) => X,
        Some(BendPart::Back | BendPart::Front) => Y,
        _ => Z,
    };
    if let Some(setup) = setup.as_mut() {
        setup.place(shape, segaxis);
    }

    // Texture layout: the unfolded box. `fix` shrinks faces by a sliver to
    // keep neighbouring texels out.
    let ts = shape.texture_size;
    let raw = tex_size(shape);
    let texsize = [raw[X] / ts[0], raw[Y] / ts[1], raw[Z] / ts[1]];
    let fix = [(raw[X] - 1.0 / 256.0) / ts[0], (raw[Y] - 1.0 / 256.0) / ts[1], (raw[Z] - 1.0 / 256.0) / ts[1]];
    let texuv = tex_origin(shape);
    let add = |a: Uv, x: f64, y: f64| -> Uv { [a[0] + x, a[1] + y] };
    let quad = |origin: Uv, w: f64, h: f64| -> [Uv; 4] { [origin, add(origin, w, 0.0), add(origin, w, h), add(origin, 0.0, h)] };

    let mut east = quad(add(texuv, texsize[X], 0.0), fix[Y], fix[Z]);
    let mut west = quad(add(texuv, -texsize[Y], 0.0), fix[Y], fix[Z]);
    let mut south = quad(texuv, fix[X], fix[Z]);
    let mut north = quad(add(east[0], texsize[Y], 0.0), fix[X], fix[Z]);
    let mut up = quad(add(texuv, 0.0, -texsize[Y]), fix[X], fix[Y]);
    // Down is flipped vertically.
    let down4 = add(up[0], texsize[X], 0.0);
    let mut down = [add(down4, 0.0, fix[Y]), add(down4, fix[X], fix[Y]), add(down4, fix[X], 0.0), down4];

    let mirror = if shape.texture_mirror { -1.0 } else { 1.0 };
    if shape.texture_mirror {
        std::mem::swap(&mut east, &mut west);
        for face in [&mut east, &mut west, &mut south, &mut north, &mut up, &mut down] {
            face.swap(0, 1);
            face.swap(2, 3);
        }
    }

    // The cut at the start of the segment axis: four corners and the
    // normals of the four sides running along the axis.
    let corners = |offset: f64| -> [Vec3; 4] {
        match segaxis {
            X => [[x1 + offset, y1, z2], [x1 + offset, y2, z2], [x1 + offset, y2, z1], [x1 + offset, y1, z1]],
            Y => [[x2, y1 + offset, z2], [x1, y1 + offset, z2], [x1, y1 + offset, z1], [x2, y1 + offset, z1]],
            _ => [[x1, y2, z1 + offset], [x2, y2, z1 + offset], [x2, y1, z1 + offset], [x1, y1, z1 + offset]],
        }
    };
    let side_normals: [Vec3; 4] = match segaxis {
        X => [[0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]],
        Y => [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]],
        _ => [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0]],
    };
    // Running texture coordinates along the axis.
    let mut tex = match segaxis {
        X => [south[0][0], north[1][0], down[3][0]],
        Y => [east[1][0], west[0][0], up[0][1]],
        _ => [south[2][1], 0.0, 0.0],
    };
    let (tex_start, tex_end) = match segaxis {
        X => (west, east),
        Y => (north, south),
        _ => (down, up),
    };

    let matrix_at = |setup: &Option<BendSetup>, position: f64, at_start: bool| -> Mat4 {
        match setup {
            Some(setup) => {
                let weight = setup.weight(position, at_start);
                let mut angles = bend_at(setup.angles, weight);
                let mut scale = [1.0; 3];
                if setup.sharp {
                    let widen = bend_scale(setup, weight, position);
                    scale = [1.0 + widen[X], 1.0 + widen[Y], 1.0 + widen[Z]];
                    if !at_start {
                        angles = setup.angles.map(|a| a * weight);
                    }
                }
                if !at_start {
                    scale = scale.map(|s| s + weight * scalef);
                }
                shape_bend_matrix(shape, setup.bend, angles, scale)
            }
            None => plain_matrix(shape),
        }
    };
    let sharp = setup.as_ref().is_some_and(|s| s.sharp);
    let transform_normals = |mat: &Mat4| side_normals.map(|n| mi_anim::math::normalize(mat.transform_vector(n)));

    let mat = matrix_at(&setup, 0.0, true);
    let mut p = corners(0.0).map(|c| mat.transform_point(c));
    let mut n = transform_normals(&mat);

    let mut builder = Builder { mesh: MeshData::default(), shape };
    let mut segpos = 0.0;
    loop {
        // End face
        if segpos >= size[segaxis] - 0.0001 {
            if segaxis == Z {
                builder.triangle([p[3], p[2], p[1]], [tex_end[0], tex_end[1], tex_end[2]], None);
                builder.triangle([p[1], p[0], p[3]], [tex_end[2], tex_end[3], tex_end[0]], None);
            } else {
                builder.triangle([p[1], p[0], p[3]], [tex_end[0], tex_end[1], tex_end[2]], None);
                builder.triangle([p[3], p[2], p[1]], [tex_end[2], tex_end[3], tex_end[0]], None);
            }
            break;
        }

        // Start face
        if segpos == 0.0 {
            builder.triangle([p[0], p[1], p[2]], [tex_start[0], tex_start[1], tex_start[2]], None);
            builder.triangle([p[2], p[3], p[0]], [tex_start[2], tex_start[3], tex_start[0]], None);
        }

        let segsize = match &setup {
            Some(setup) => setup.segment(segpos, size[segaxis], shape.from[segaxis]),
            None => size[segaxis] - segpos,
        };
        segpos += segsize.max(0.005);

        let progress = segpos / size[segaxis];
        let ntex = match segaxis {
            X => {
                let offset = progress * fix[X] * mirror;
                [south[0][0] + offset, north[1][0] - offset, down[3][0] + offset]
            }
            Y => {
                let offset = progress * fix[Y];
                [east[1][0] - offset * mirror, west[0][0] + offset * mirror, up[0][1] + offset]
            }
            _ => [south[2][1] - progress * fix[Z], 0.0, 0.0],
        };

        let mat = matrix_at(&setup, segpos, false);
        let np = corners(segpos).map(|c| mat.transform_point(c));
        let nn = transform_normals(&mat);

        // Sides. `side` adds one quad: corners a, b on this cut and c, d on
        // the next, with the normals of both cuts unless the fold is sharp.
        let mut side = |tris: [([Vec3; 3], [Uv; 3], [Vec3; 3]); 2]| {
            for (points, uvs, normals) in tris {
                builder.triangle(points, uvs, (!sharp).then_some(normals));
            }
        };
        match segaxis {
            X => {
                // South
                let t = [[tex[0], south[0][1]], [ntex[0], south[0][1]], [ntex[0], south[2][1]], [tex[0], south[2][1]]];
                side([([p[1], np[1], np[2]], [t[0], t[1], t[2]], [n[0], nn[0], nn[0]]), ([np[2], p[2], p[1]], [t[2], t[3], t[0]], [nn[0], n[0], n[0]])]);
                // North
                let t = [[ntex[1], north[0][1]], [tex[1], north[0][1]], [tex[1], north[2][1]], [ntex[1], north[2][1]]];
                side([([np[0], p[0], p[3]], [t[0], t[1], t[2]], [nn[1], n[1], n[1]]), ([p[3], np[3], np[0]], [t[2], t[3], t[0]], [n[1], nn[1], nn[1]])]);
                // Up
                let t = [[tex[0], up[0][1]], [ntex[0], up[0][1]], [ntex[0], up[2][1]], [tex[0], up[2][1]]];
                side([([p[0], np[0], np[1]], [t[0], t[1], t[2]], [n[2], nn[2], nn[2]]), ([np[1], p[1], p[0]], [t[2], t[3], t[0]], [nn[2], n[2], n[2]])]);
                // Down
                let t = [[tex[2], down[0][1]], [ntex[2], down[0][1]], [ntex[2], down[2][1]], [tex[2], down[2][1]]];
                side([([p[2], np[2], np[3]], [t[0], t[1], t[2]], [n[3], nn[3], nn[3]]), ([np[3], p[3], p[2]], [t[2], t[3], t[0]], [nn[3], n[3], n[3]])]);
            }
            Y => {
                // East
                let t = [[ntex[0], east[0][1]], [tex[0], east[0][1]], [tex[0], east[2][1]], [ntex[0], east[2][1]]];
                side([([np[0], p[0], p[3]], [t[0], t[1], t[2]], [nn[0], n[0], n[0]]), ([p[3], np[3], np[0]], [t[2], t[3], t[0]], [n[0], nn[0], nn[0]])]);
                // West
                let t = [[tex[1], west[0][1]], [ntex[1], west[0][1]], [ntex[1], west[2][1]], [tex[1], west[2][1]]];
                side([([p[1], np[1], np[2]], [t[0], t[1], t[2]], [n[1], nn[1], nn[1]]), ([np[2], p[2], p[1]], [t[2], t[3], t[0]], [nn[1], n[1], n[1]])]);
                // Up
                let t = [[up[0][0], tex[2]], [up[1][0], tex[2]], [up[1][0], ntex[2]], [up[0][0], ntex[2]]];
                side([([p[1], p[0], np[0]], [t[0], t[1], t[2]], [n[2], n[2], nn[2]]), ([np[0], np[1], p[1]], [t[2], t[3], t[0]], [nn[2], nn[2], n[2]])]);
                // Down
                let t = [[down[0][0], ntex[2]], [down[1][0], ntex[2]], [down[1][0], tex[2]], [down[0][0], tex[2]]];
                side([([np[2], np[3], p[3]], [t[0], t[1], t[2]], [nn[3], nn[3], n[3]]), ([p[3], p[2], np[2]], [t[2], t[3], t[0]], [n[3], n[3], nn[3]])]);
            }
            _ => {
                let around = |face: &[Uv; 4]| [[face[0][0], ntex[0]], [face[1][0], ntex[0]], [face[1][0], tex[0]], [face[0][0], tex[0]]];
                // East
                let t = around(&east);
                side([([np[1], np[2], p[2]], [t[0], t[1], t[2]], [nn[0], nn[0], n[0]]), ([p[2], p[1], np[1]], [t[2], t[3], t[0]], [n[0], n[0], nn[0]])]);
                // West
                let t = around(&west);
                side([([np[3], np[0], p[0]], [t[0], t[1], t[2]], [nn[1], nn[1], n[1]]), ([p[0], p[3], np[3]], [t[2], t[3], t[0]], [n[1], n[1], nn[1]])]);
                // South
                let t = around(&south);
                side([([np[0], np[1], p[1]], [t[0], t[1], t[2]], [nn[2], nn[2], n[2]]), ([p[1], p[0], np[0]], [t[2], t[3], t[0]], [n[2], n[2], nn[2]])]);
                // North
                let t = around(&north);
                side([([np[2], np[3], p[3]], [t[0], t[1], t[2]], [nn[3], nn[3], n[3]]), ([p[3], p[2], np[2]], [t[2], t[3], t[0]], [n[3], n[3], nn[3]])]);
            }
        }

        tex = ntex;
        p = np;
        n = nn;
    }
    builder.mesh
}

fn plane(shape: &ModelShape, mut setup: Option<BendSetup>) -> MeshData {
    let (x1, y1, z1) = (shape.from[X], shape.from[Y], shape.from[Z]);
    let (x2, z2) = (shape.to[X], shape.to[Z]);
    let size = [x2 - x1, 0.0, z2 - z1];

    // Planes are cut along X for sideways bends and along Z otherwise.
    let segaxis = match setup.as_ref().map(|s| s.bend.part) {
        Some(BendPart::Left | BendPart::Right) => X,
        _ => Z,
    };
    if let Some(setup) = setup.as_mut() {
        setup.place(shape, segaxis);
    }

    let ts = shape.texture_size;
    let raw = tex_size(shape);
    let texsize = [raw[X] / ts[0], raw[Y] / ts[1], raw[Z] / ts[1]];
    let origin = tex_origin(shape);
    let mut tex = [origin, [origin[0] + texsize[X], origin[1]], [origin[0] + texsize[X], origin[1] + texsize[Z]], [origin[0], origin[1] + texsize[Z]]];
    let mirror = if shape.texture_mirror { -1.0 } else { 1.0 };
    if shape.texture_mirror {
        tex.swap(0, 1);
        tex.swap(2, 3);
    }

    let edge = |offset: f64| -> [Vec3; 2] {
        if segaxis == X {
            [[x1 + offset, y1, z2], [x1 + offset, y1, z1]]
        } else {
            [[x1, y1, z1 + offset], [x2, y1, z1 + offset]]
        }
    };
    let matrix_at = |setup: &Option<BendSetup>, position: f64, at_start: bool| -> Mat4 {
        match setup {
            Some(setup) => {
                let weight = setup.weight(position, at_start);
                let angles = bend_at(setup.angles, weight);
                // The original computes the widening of sharp folds here but
                // stores it in the wrong variable for all cuts after the
                // first, so it is lost; it is applied to every cut here.
                let widen = if setup.sharp { bend_scale(setup, weight, position) } else { [0.0; 3] };
                shape_bend_matrix(shape, setup.bend, angles, [1.0 + widen[X], 1.0 + widen[Y], 1.0 + widen[Z]])
            }
            None => plain_matrix(shape),
        }
    };
    let sharp = setup.as_ref().is_some_and(|s| s.sharp);
    let front_back = |mat: &Mat4| -> [Vec3; 2] {
        [
            mi_anim::math::normalize(mat.transform_vector([0.0, 1.0, 0.0])),
            mi_anim::math::normalize(mat.transform_vector([0.0, -1.0, 0.0])),
        ]
    };

    let mat = matrix_at(&setup, 0.0, true);
    let mut p = edge(0.0).map(|c| mat.transform_point(c));
    let mut n = front_back(&mat);
    let mut texp = if segaxis == X { tex[0][0] } else { tex[2][1] };

    let mut builder = Builder { mesh: MeshData::default(), shape };
    let mut segpos = 0.0;
    while segpos < size[segaxis] {
        let segsize = match &setup {
            Some(setup) => setup.segment(segpos, size[segaxis], shape.from[segaxis]),
            None => size[segaxis] - segpos,
        };
        segpos += segsize.max(0.005);

        let progress = segpos / size[segaxis];
        let ntexp = if segaxis == X { tex[0][0] + progress * texsize[X] * mirror } else { tex[2][1] - progress * texsize[Z] };

        let mat = matrix_at(&setup, segpos, false);
        let np = edge(segpos).map(|c| mat.transform_point(c));
        let nn = front_back(&mat);
        let normals = |list: [Vec3; 3]| (!sharp).then_some(list);

        if segaxis == X {
            let t = [[texp, tex[0][1]], [ntexp, tex[0][1]], [ntexp, tex[2][1]], [texp, tex[2][1]]];
            if !shape.hide_front {
                builder.triangle([p[0], np[0], np[1]], [t[0], t[1], t[2]], normals([n[0], nn[0], nn[0]]));
                builder.triangle([np[1], p[1], p[0]], [t[2], t[3], t[0]], normals([nn[0], n[0], n[0]]));
            }
            if !shape.hide_back {
                builder.triangle([np[0], p[0], np[1]], [t[1], t[0], t[2]], normals([nn[1], n[1], nn[1]]));
                builder.triangle([p[1], np[1], p[0]], [t[3], t[2], t[0]], normals([n[1], nn[1], n[1]]));
            }
        } else {
            let t = [[tex[0][0], ntexp], [tex[1][0], ntexp], [tex[1][0], texp], [tex[0][0], texp]];
            if !shape.hide_front {
                builder.triangle([np[0], np[1], p[1]], [t[0], t[1], t[2]], normals([nn[0], nn[0], n[0]]));
                builder.triangle([p[1], p[0], np[0]], [t[2], t[3], t[0]], normals([n[0], n[0], nn[0]]));
            }
            if !shape.hide_back {
                builder.triangle([np[1], np[0], p[1]], [t[1], t[0], t[2]], normals([nn[1], nn[1], n[1]]));
                builder.triangle([p[0], p[1], np[0]], [t[3], t[2], t[0]], normals([n[1], n[1], nn[1]]));
            }
        }

        p = np;
        n = nn;
        texp = ntexp;
    }
    builder.mesh
}
