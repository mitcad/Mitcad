// SPDX-License-Identifier: MIT
//! 3MF files (3D Manufacturing Format, core specification 1.3): triangle
//! meshes in millimetres for slicers (mitcad#13).
//!
//! A 3MF file is an OPC package: a zip archive with `[Content_Types].xml`,
//! `_rels/.rels` and the model part `3D/3dmodel.model`, whose XML lists
//! resources (materials, mesh objects, objects made of components) and the
//! build (the objects to print). [`write`] writes a [`Model`] as one build
//! item: an object whose components place one mesh object per part, so
//! slicers load the parts as one object in their positions, with the parts'
//! names and colours (`basematerials`). [`read`] reads any core model back
//! into parts with their placements and checks what the core specification
//! requires of it (tests use it on what [`write`] wrote).

mod read;
mod write;

pub use read::read;
pub use write::write;

use std::fmt;

/// The 3MF core namespace (2015/02).
pub const CORE_NAMESPACE: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
/// The content type of a model part.
pub const MODEL_CONTENT_TYPE: &str = "application/vnd.ms-package.3dmanufacturing-3dmodel+xml";
/// The relationship type of the package's root model.
pub const MODEL_RELATIONSHIP: &str = "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";

/// A triangle mesh: vertices and triangles of vertex indices,
/// counter-clockwise seen from outside.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    /// The volume the triangles enclose (the divergence theorem: tetrahedra
    /// to the origin); positive when they face outwards.
    pub fn volume(&self) -> f64 {
        let mut six = 0.0;
        for t in &self.triangles {
            let [a, b, c] = t.map(|i| self.vertices[i as usize]);
            six += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]);
        }
        six / 6.0
    }

    /// The mesh mapped by a placement; a mirroring placement turns the
    /// triangles round, so they still face outwards.
    pub fn placed(&self, placement: &Placement) -> Mesh {
        let mirrored = placement.determinant() < 0.0;
        Mesh {
            vertices: self.vertices.iter().map(|p| placement.apply(*p)).collect(),
            triangles: self
                .triangles
                .iter()
                .map(|&[a, b, c]| if mirrored { [a, c, b] } else { [a, b, c] })
                .collect(),
        }
    }

    /// Why the mesh does not bound a solid as 3MF requires of objects of
    /// type `model`: an index out of range, a degenerate triangle, or an
    /// edge that is not shared by exactly two triangles running along it in
    /// opposite directions (an opening, a fold, a triangle turned round).
    pub fn check_closed(&self) -> Result<(), String> {
        use std::collections::HashMap;
        let count = self.vertices.len();
        // Directed edge (from, to) -> uses.
        let mut edges: HashMap<(u32, u32), u32> = HashMap::with_capacity(self.triangles.len() * 3);
        for (i, t) in self.triangles.iter().enumerate() {
            if t.iter().any(|&v| v as usize >= count) {
                return Err(format!(
                    "triangle {i} refers to a vertex beyond the {count} vertices"
                ));
            }
            if t[0] == t[1] || t[1] == t[2] || t[2] == t[0] {
                return Err(format!("triangle {i} uses a vertex twice"));
            }
            for k in 0..3 {
                *edges.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
            }
        }
        for (&(a, b), &uses) in &edges {
            if uses != 1 || edges.get(&(b, a)) != Some(&1) {
                return Err(format!(
                    "the edge between vertices {a} and {b} is not shared by exactly two \
                     triangles in opposite directions"
                ));
            }
        }
        Ok(())
    }
}

/// An affine placement, `p' = linear * p + translation` (row-major).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub linear: [[f64; 3]; 3],
    pub translation: [f64; 3],
}

impl Placement {
    pub const IDENTITY: Self = Self {
        linear: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0; 3],
    };

    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|r| {
            self.linear[r][0] * p[0]
                + self.linear[r][1] * p[1]
                + self.linear[r][2] * p[2]
                + self.translation[r]
        })
    }

    /// `self` after `inner`: `p -> self(inner(p))`.
    pub fn after(&self, inner: &Placement) -> Placement {
        Placement {
            linear: std::array::from_fn(|r| {
                std::array::from_fn(|c| {
                    (0..3).map(|k| self.linear[r][k] * inner.linear[k][c]).sum()
                })
            }),
            translation: self.apply(inner.translation),
        }
    }

    pub fn determinant(&self) -> f64 {
        let m = &self.linear;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    /// The 3MF form, `m00 m01 m02 m10 m11 m12 m20 m21 m22 m30 m31 m32`:
    /// 3MF maps row vectors (`p' = p * M`, the translation in the last
    /// row), so its matrix is the transpose of `linear`.
    pub fn to_3mf(&self) -> [f64; 12] {
        let m = &self.linear;
        let t = &self.translation;
        [
            m[0][0], m[1][0], m[2][0], m[0][1], m[1][1], m[2][1], m[0][2], m[1][2], m[2][2], t[0],
            t[1], t[2],
        ]
    }

    pub fn from_3mf(m: [f64; 12]) -> Placement {
        Placement {
            linear: [[m[0], m[3], m[6]], [m[1], m[4], m[7]], [m[2], m[5], m[8]]],
            translation: [m[9], m[10], m[11]],
        }
    }
}

/// A part of the printed object: one mesh object of the 3MF file, placed
/// once or more (a body of a component that is placed twice).
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub name: String,
    /// sRGB components in [0, 1].
    pub color: Option<[f64; 3]>,
    pub mesh: Mesh,
    pub placements: Vec<Placement>,
}

/// What a 3MF file holds: one object (named `name`) made of parts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    pub name: String,
    pub parts: Vec<Part>,
    /// The writing application, for the `Application` metadata.
    pub application: String,
}

/// A 3MF file could not be written or read, or breaks the core
/// specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests;
