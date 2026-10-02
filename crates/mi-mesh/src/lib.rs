//! Mesh data shared by everything that produces or draws geometry.
//!
//! Meshes are plain triangle lists, like the vertex buffers of the original.
//! The cross product of a triangle's edges (first to second, second to third
//! corner) points along its normal. Because the world is left-handed, that
//! makes front faces clockwise on screen.

mod shapes;

pub use shapes::{ground_mesh, shape_mesh, Shape, ShapeSettings, SHAPE_RADIUS};

use bytemuck::{Pod, Zeroable};

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

fn normalized(n: [f32; 3]) -> [f32; 3] {
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if length > 0.0 {
        n.map(|c| c / length)
    } else {
        n
    }
}

impl MeshData {
    pub fn triangle_count(&self) -> usize {
        self.vertices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Adds a vertex with an explicit normal (`vertex_add`). The normal is
    /// normalised.
    pub fn vertex(&mut self, position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) {
        self.vertex_custom(position, normal, uv, [0.0; 4]);
    }

    /// Like [`MeshData::vertex`] with the wind and material values of the
    /// vertex.
    pub fn vertex_custom(&mut self, position: [f32; 3], normal: [f32; 3], uv: [f32; 2], custom: [f32; 4]) {
        self.vertices.push(Vertex { position, normal: normalized(normal), uv, color: [1.0; 4], custom });
    }

    /// Adds a flat triangle whose normal follows from its corners
    /// (`vbuffer_add_triangle`). `invert` flips it.
    pub fn triangle(&mut self, p: [[f32; 3]; 3], uv: [[f32; 2]; 3], invert: bool) {
        self.triangle_with(p, uv, None, invert, [[0.0; 4]; 3]);
    }

    /// Adds a triangle with optional per-corner normals; without them the
    /// face normal is used. `invert` swaps the first two corners and flips
    /// the normals. `custom` holds the wind and material values per corner.
    pub fn triangle_with(
        &mut self,
        p: [[f32; 3]; 3],
        uv: [[f32; 2]; 3],
        normals: Option<[[f32; 3]; 3]>,
        invert: bool,
        custom: [[f32; 4]; 3],
    ) {
        let [p1, p2, p3] = p;
        let face = [
            (p1[2] - p2[2]) * (p3[1] - p2[1]) - (p1[1] - p2[1]) * (p3[2] - p2[2]),
            (p1[0] - p2[0]) * (p3[2] - p2[2]) - (p1[2] - p2[2]) * (p3[0] - p2[0]),
            (p1[1] - p2[1]) * (p3[0] - p2[0]) - (p1[0] - p2[0]) * (p3[1] - p2[1]),
        ];
        let mut n = normals.unwrap_or([face; 3]);
        let order = if invert {
            n = n.map(|v| v.map(|c| -c));
            [1, 0, 2]
        } else {
            [0, 1, 2]
        };
        for i in order {
            self.vertex_custom(p[i], n[i], uv[i], custom[i]);
        }
    }
}
