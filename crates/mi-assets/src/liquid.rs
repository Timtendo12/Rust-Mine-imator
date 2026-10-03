//! Water and lava meshes, shaped by the levels of neighbouring liquid
//! (`block_generate_liquid`).

use crate::blocks::Dir;
use mi_mesh::MeshData;
use std::collections::HashMap;

const BLOCK: f64 = 16.0;

/// What the liquid generator needs to know about the grid.
pub(crate) struct LiquidSurroundings<'a> {
    /// The liquid's level (0..16) at a position, if the same liquid is
    /// there.
    pub level_at: &'a dyn Fn([i64; 3]) -> Option<i64>,
    pub waterlogged_at: &'a dyn Fn([i64; 3]) -> bool,
    /// Whether the side `dir` of the model at a position is a full opaque
    /// face.
    pub solid_face: &'a dyn Fn([i64; 3], Dir) -> bool,
    /// Whether liquids wave (`liquid_animation`), which changes which sides
    /// are drawn.
    pub animation: bool,
}

/// The height of the surface for a level.
fn level_z(level: i64) -> f64 {
    if level / 8 > 0 {
        BLOCK
    } else {
        14.0 - (level as f64 / 7.0) * 13.5
    }
}

fn add(p: [i64; 3], d: [i64; 3]) -> [i64; 3] {
    [p[0] + d[0], p[1] + d[1], p[2] + d[2]]
}

/// Adds the liquid at `p` (in blocks) to `out`. `name` is `water` or
/// `lava`; a waterlogged block holds still water whatever its level.
pub(crate) fn liquid_mesh(
    name: &str,
    p: [i64; 3],
    level: i64,
    waterlogged: bool,
    emissive: f64,
    s: &LiquidSurroundings,
    out: &mut HashMap<String, MeshData>,
) {
    let same = |d: [i64; 3]| (s.level_at)(add(p, d)).is_some();
    let logged = |d: [i64; 3]| (s.waterlogged_at)(add(p, d));
    let solid = |dir: Dir| (s.solid_face)(add(p, dir.step()), dir.opposite());

    let steps = Dir::ALL.map(Dir::step);
    let matches = steps.map(|d| same(d) || logged(d));
    let mut solids = Dir::ALL.map(solid);
    let [e, w, so, n, up, down] = [0, 1, 2, 3, 4, 5];

    // Sides next to liquid above or below are drawn when that liquid has
    // no solid block beside it, so waves leave no gaps.
    if s.animation {
        for (vertical, dz) in [(up, 1), (down, -1)] {
            if !matches[vertical] {
                continue;
            }
            for side in [e, w, so, n] {
                let d = steps[side];
                let diagonal = add(p, [d[0], d[1], dz]);
                if solids[side] && (s.level_at)(diagonal).is_none() {
                    solids[side] = (s.solid_face)(diagonal, Dir::ALL[side].opposite());
                }
            }
        }
    }

    // Surrounded: nothing to draw.
    let covered = |i: usize| matches[i] || solids[i];
    if covered(e) && covered(w) && covered(so) && covered(n) && matches[up] && covered(down) {
        return;
    }

    let level = if waterlogged { 0 } else { level };
    let (mut corner_z, min_z, average_z, top_flow, top_angle);
    if level / 8 > 0 || matches[up] {
        corner_z = [BLOCK; 4];
        min_z = BLOCK;
        average_z = BLOCK;
        top_flow = false;
        top_angle = 0.0;
    } else {
        let mut side_level = [level; 4];
        // Corners: (-x, -y), (+x, -y), (+x, +y), (-x, +y).
        let mut corner_level = [level; 4];
        if !waterlogged {
            for side in [e, w, so, n] {
                let d = steps[side];
                if logged(d) {
                    continue;
                }
                if (s.level_at)(add(p, [d[0], d[1], 1])).is_some() {
                    side_level[side] = 8;
                } else if let Some(other) = (s.level_at)(add(p, d)) {
                    side_level[side] = other;
                }
            }
            let corners = [[-1, -1], [1, -1], [1, 1], [-1, 1]];
            for (i, c) in corners.iter().enumerate() {
                let d = [c[0], c[1], 0];
                if logged(d) {
                    continue;
                }
                if (s.level_at)(add(p, [c[0], c[1], 1])).is_some() {
                    corner_level[i] = 8;
                } else if let Some(other) = (s.level_at)(add(p, d)) {
                    corner_level[i] = other;
                }
            }
        }

        // Flow towards lower neighbours.
        let (mut flow_xp, mut flow_xn, mut flow_yp, mut flow_yn) = (0, 0, 0, 0);
        let lower = |l: i64| l % 8 < level;
        let higher = |l: i64| l % 8 > level;
        if lower(side_level[e]) {
            flow_xn += 1;
        } else if higher(side_level[e]) {
            flow_xp += 1;
        }
        if lower(side_level[w]) {
            flow_xp += 1;
        } else if higher(side_level[w]) {
            flow_xn += 1;
        }
        if lower(side_level[so]) {
            flow_yn += 1;
        } else if higher(side_level[so]) {
            flow_yp += 1;
        }
        if lower(side_level[n]) {
            flow_yp += 1;
        } else if higher(side_level[n]) {
            flow_yn += 1;
        }

        let my_z = 14.0 - (level as f64 / 7.0) * 13.5;
        let side_z = side_level.map(level_z);
        corner_z = corner_level.map(level_z);
        corner_z[0] = corner_z[0].max(side_z[w]).max(side_z[n]).max(my_z);
        corner_z[1] = corner_z[1].max(side_z[e]).max(side_z[n]).max(my_z);
        corner_z[2] = corner_z[2].max(side_z[e]).max(side_z[so]).max(my_z);
        corner_z[3] = corner_z[3].max(side_z[w]).max(side_z[so]).max(my_z);
        average_z = corner_z.iter().sum::<f64>() / 4.0;
        min_z = corner_z.iter().copied().fold(f64::INFINITY, f64::min);

        let (xn, xp, yn, yp) = (flow_xn > 0, flow_xp > 0, flow_yn > 0, flow_yp > 0);
        let (fxn, fxp, fyn, fyp) = (flow_xn as f64, flow_xp as f64, flow_yn as f64, flow_yp as f64);
        let mut flow = true;
        let angle = if (!xn && !xp && !yn && !yp) || (xn && xp && yn && yp) || (xn && xp && !yn && !yp) || (!xn && !xp && yn && yp) {
            flow = false;
            0.0
        } else if xn && xp && yp {
            0.0
        } else if xn && xp && yn {
            180.0
        } else if xp && yn && yp {
            90.0
        } else if xn && yn && yp {
            270.0
        } else if xn && yn {
            180.0 + 45.0 + 10.0 * (fxn - 1.0) - 10.0 * (fyn - 1.0)
        } else if xp && yn {
            180.0 - 45.0 + 10.0 * (fyn - 1.0) - 10.0 * (fxp - 1.0)
        } else if xn && yp {
            270.0 + 45.0 + 10.0 * (fyp - 1.0) - 10.0 * (fxn - 1.0)
        } else if xp && yp {
            45.0 + 10.0 * (fxp - 1.0) - 10.0 * (fyp - 1.0)
        } else if yp {
            0.0
        } else if xp {
            90.0
        } else if yn {
            180.0
        } else {
            270.0
        };
        top_flow = flow;
        top_angle = angle;
    }

    // Texture coordinates, in pixels.
    let side_uv = [[0.0, BLOCK - min_z], [BLOCK, BLOCK - min_z], [BLOCK, BLOCK], [0.0, BLOCK]];
    let corner_left = corner_z.map(|z| [0.0, BLOCK - z]);
    let corner_right = corner_z.map(|z| [BLOCK, BLOCK - z]);
    let top_uv = if top_angle != 0.0 {
        let q = (top_angle.rem_euclid(90.0) / 90.0) * BLOCK;
        let mut uv = [[q, 0.0], [BLOCK, q], [BLOCK - q, BLOCK], [0.0, BLOCK - q]];
        for _ in 0..(top_angle / 90.0) as usize {
            uv.rotate_left(1);
        }
        uv
    } else {
        [[0.0, 0.0], [BLOCK, 0.0], [BLOCK, BLOCK], [0.0, BLOCK]]
    };
    let top_mid = [BLOCK / 2.0, BLOCK / 2.0];

    let flow_texture = format!("block/{name}_flow");
    let top_texture = if top_flow { flow_texture.clone() } else { format!("block/{name}_still") };

    let (mut x1, mut y1) = (p[0] as f64 * BLOCK, p[1] as f64 * BLOCK);
    let (mut x2, mut y2) = (x1 + BLOCK, y1 + BLOCK);
    let z1 = p[2] as f64 * BLOCK;
    let z2 = z1 + min_z;
    let mid = [x1 + BLOCK / 2.0, y1 + BLOCK / 2.0, z1 + average_z];

    // Waterlogged sides move in a little against flickering.
    if waterlogged {
        let indent = 0.05;
        if !matches[e] {
            x2 -= indent;
        }
        if !matches[w] {
            x1 += indent;
        }
        if !matches[so] {
            y2 -= indent;
        }
        if !matches[n] {
            y1 += indent;
        }
    }
    let cz = corner_z.map(|z| z + z1);

    // Liquids bob up and down; their bottom stays put unless there is
    // more of the liquid below (`block_generate_liquid`).
    let wave_from = if !s.animation {
        f64::INFINITY
    } else if matches[down] {
        f64::NEG_INFINITY
    } else {
        z1
    };
    let mut em = Emitter { out, emissive: emissive as f32, wave_from };
    let flow = flow_texture.as_str();

    if !matches[e] && !solids[e] {
        em.face(flow, [[x2, y2, z2], [x2, y1, z2], [x2, y1, z1], [x2, y2, z1]], side_uv);
        if cz[1] > cz[2] {
            em.triangle(flow, [[x2, y1, z2], [x2, y2, z2], [x2, y1, cz[1]]], [side_uv[1], side_uv[0], corner_right[1]]);
        } else {
            em.triangle(flow, [[x2, y1, z2], [x2, y2, z2], [x2, y2, cz[2]]], [side_uv[1], side_uv[0], corner_left[2]]);
        }
    }
    if !matches[w] && !solids[w] {
        em.face(flow, [[x1, y1, z2], [x1, y2, z2], [x1, y2, z1], [x1, y1, z1]], side_uv);
        if cz[3] > cz[0] {
            em.triangle(flow, [[x1, y2, z2], [x1, y1, z2], [x1, y2, cz[3]]], [side_uv[1], side_uv[0], corner_right[3]]);
        } else {
            em.triangle(flow, [[x1, y2, z2], [x1, y1, z2], [x1, y1, cz[0]]], [side_uv[1], side_uv[0], corner_left[0]]);
        }
    }
    if !matches[so] && !solids[so] {
        em.face(flow, [[x1, y2, z2], [x2, y2, z2], [x2, y2, z1], [x1, y2, z1]], side_uv);
        if cz[2] > cz[3] {
            em.triangle(flow, [[x2, y2, z2], [x1, y2, z2], [x2, y2, cz[2]]], [side_uv[1], side_uv[0], corner_right[2]]);
        } else {
            em.triangle(flow, [[x2, y2, z2], [x1, y2, z2], [x1, y2, cz[3]]], [side_uv[1], side_uv[0], corner_left[3]]);
        }
    }
    if !matches[n] && !solids[n] {
        em.face(flow, [[x2, y1, z2], [x1, y1, z2], [x1, y1, z1], [x2, y1, z1]], side_uv);
        if cz[0] > cz[1] {
            em.triangle(flow, [[x1, y1, z2], [x2, y1, z2], [x1, y1, cz[0]]], [side_uv[1], side_uv[0], corner_right[0]]);
        } else {
            em.triangle(flow, [[x1, y1, z2], [x2, y1, z2], [x2, y1, cz[1]]], [side_uv[1], side_uv[0], corner_left[1]]);
        }
    }
    let top = top_texture.as_str();
    if !matches[up] {
        let c = [[x1, y1, cz[0]], [x2, y1, cz[1]], [x2, y2, cz[2]], [x1, y2, cz[3]]];
        for i in 0..4 {
            let j = (i + 1) % 4;
            em.triangle(top, [mid, c[i], c[j]], [top_mid, top_uv[i], top_uv[j]]);
        }
    }
    if !matches[down] && !solids[down] {
        em.face(top, [[x1, y2, z1], [x2, y2, z1], [x2, y1, z1], [x1, y1, z1]], [top_uv[3], top_uv[2], top_uv[1], top_uv[0]]);
    }
}

/// Adds triangles to the mesh of their texture.
struct Emitter<'a> {
    out: &'a mut HashMap<String, MeshData>,
    emissive: f32,
    /// Vertices above this height wave.
    wave_from: f64,
}

impl Emitter<'_> {
    fn triangle(&mut self, texture: &str, ps: [[f64; 3]; 3], uvs: [[f64; 2]; 3]) {
        let uv = |c: [f64; 2]| [(c[0] / BLOCK) as f32, (c[1] / BLOCK) as f32];
        let mesh = self.out.entry(texture.to_owned()).or_default();
        let custom = ps.map(|c| [0.0, (c[2] > self.wave_from) as u8 as f32, self.emissive, 0.0]);
        mesh.triangle_with(ps.map(|c| c.map(|v| v as f32)), uvs.map(uv), None, false, custom);
    }

    fn face(&mut self, texture: &str, ps: [[f64; 3]; 4], uvs: [[f64; 2]; 4]) {
        self.triangle(texture, [ps[0], ps[1], ps[2]], [uvs[0], uvs[1], uvs[2]]);
        self.triangle(texture, [ps[2], ps[3], ps[0]], [uvs[2], uvs[3], uvs[0]]);
    }
}
