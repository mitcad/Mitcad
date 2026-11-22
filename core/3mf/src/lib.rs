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
//! names and colours (`basematerials`); with [`Model::objects`], as several
//! build items, each an object of its own placements of the parts
//! (mitcad#856: the pieces a slicer arranges on its plate one by one).
//! [`read`] reads any core model back into parts with their placements
//! (and the build's objects when it has several) and checks what the core
//! specification requires of it (tests use it on what [`write`] wrote).
//! It follows the Production extension's components into other model parts
//! of the package, as slicers' projects keep their meshes (mitcad#2156), and
//! names objects without a name as the project's settings file does.
//! [`read_for_import`] reads a file to import (mitcad#2156): meshes that
//! are not closed come in with a warning rather than a refusal.
//!
//! A slicer's settings (mitcad#640) go with the model: more metadata of
//! the model and of each part's mesh object, and more files of the
//! package ([`Model::files`]); [`read_package`] reads them back without
//! the geometry.

mod package;
mod read;
mod write;

pub use package::{Package, PackageObject, read_package};
pub use read::{read, read_for_import};
pub use write::write;

use std::fmt;

/// The 3MF core namespace (2015/02).
pub const CORE_NAMESPACE: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
/// The content type of a model part.
pub const MODEL_CONTENT_TYPE: &str = "application/vnd.ms-package.3dmanufacturing-3dmodel+xml";
/// The relationship type of the package's root model.
pub const MODEL_RELATIONSHIP: &str = "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";
/// The Production extension's namespace (2015/06): components and build
/// items that refer to objects of other model parts by `p:path`
/// (mitcad#2156).
pub const PRODUCTION_NAMESPACE: &str =
    "http://schemas.microsoft.com/3dmanufacturing/production/2015/06";

/// What [`read_for_import`] read (mitcad#2156): the model, the base
/// material's name of each part's colour, and what it took that [`read`]
/// refuses, in words.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportRead {
    pub model: Model,
    /// By the index of the part in the model's `parts`: the name of the
    /// base material its colour comes from, when it has one.
    pub materials: Vec<Option<String>>,
    /// Each part whose mesh is not closed, faces inwards or had triangles
    /// left out: its name and what is wrong.
    pub warnings: Vec<String>,
}

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
    /// The mesh object's own metadata (a `metadatagroup`: a slicer's
    /// per-object settings, `cura:layer_height`), by name; the names'
    /// prefixes need their namespaces in [`Model::namespaces`].
    pub metadata: Vec<(String, String)>,
}

/// What a 3MF file holds: one object (named `name`) made of parts, or the
/// [`Model::objects`] of their placements.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    pub name: String,
    pub parts: Vec<Part>,
    /// The writing application, for the `Application` metadata.
    pub application: String,
    /// More metadata of the model (`BambuStudio:3mfVersion`), after the
    /// application and the title.
    pub metadata: Vec<(String, String)>,
    /// The namespaces of prefixed metadata names: (prefix, URI).
    pub namespaces: Vec<(String, String)>,
    /// More files of the package by their paths
    /// (`Metadata/project_settings.config`): a slicer's settings.
    pub files: Vec<(String, Vec<u8>)>,
    /// The unit of the meshes' coordinates and the placements' moves
    /// (mitcad#734): millimetres by default. [`read`] gives millimetres.
    pub unit: Unit,
    /// The build's objects (mitcad#856), each a build item of its own;
    /// empty for one object, named `name`, of every placement of every
    /// part. [`read`] gives them when the build has more than one item.
    pub objects: Vec<BuildObject>,
}

/// An object of the build (mitcad#856): its name and the placements of
/// the parts it is made of.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BuildObject {
    pub name: String,
    /// Its components: (index of the part in [`Model::parts`], index of
    /// the placement in that part's `placements`).
    pub components: Vec<(usize, usize)>,
}

/// A 3MF model's length unit (the `unit` attribute of `<model>`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Unit {
    Micron,
    #[default]
    Millimeter,
    Centimeter,
    Inch,
    Foot,
    Meter,
}

impl Unit {
    /// The decimals [`write`] gives a coordinate in this unit: a
    /// nanometre or finer (6 in millimetres, 8 in inches), never fewer
    /// than 6, so a placement's turn keeps its precision too.
    pub fn decimals(self) -> usize {
        match self {
            Self::Micron | Self::Millimeter => 6,
            Self::Centimeter => 7,
            Self::Inch => 8,
            Self::Foot | Self::Meter => 9,
        }
    }

    /// The attribute's value: `millimeter`, `inch`, ...
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Micron => "micron",
            Self::Millimeter => "millimeter",
            Self::Centimeter => "centimeter",
            Self::Inch => "inch",
            Self::Foot => "foot",
            Self::Meter => "meter",
        }
    }
}

/// The object id [`write`] gives the mesh object of part `index` (the
/// material group is 1, the parts' objects follow from 2): slicers'
/// per-part settings refer to it.
pub fn part_object_id(index: usize) -> u32 {
    index as u32 + 2
}

/// The object id [`write`] gives the object of a model's `parts` parts,
/// its one build item.
pub fn assembly_object_id(parts: usize) -> u32 {
    parts as u32 + 2
}

/// The object id [`write`] gives build object `index` of a model's
/// [`Model::objects`] (mitcad#856): after the parts' mesh objects, in
/// order; the first is [`assembly_object_id`].
pub fn build_object_id(parts: usize, index: usize) -> u32 {
    assembly_object_id(parts) + index as u32
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
