// SPDX-License-Identifier: MIT
//! The control cages of Form bodies (mitcad#1365): `_f3d.cages`.
//!
//! A Form body's faces come in from the B-rep blob like any other body's;
//! the cage they were made from is a `TSplines.BlobParts/*.tsm` entry of
//! the document, a line-oriented `TS0200` program
//! (`mitcad_f3d_design::cage` reads it). A [`FormCage`] holds that cage as
//! decoded, in the file's units (cm), with what joins it to the design's
//! records. These are decoder data: unlike the records of the dump's
//! sections, every key is written by the decoder, so they are plain
//! structs (absent keys read as their defaults).

use serde::{Deserialize, Serialize};

use crate::Vec3;

fn is_zero(n: &u64) -> bool {
    *n == 0
}

fn is_one(x: &f64) -> bool {
    *x == 1.0
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_smooth(tag: &VertexTag) -> bool {
    *tag == VertexTag::Smooth
}

/// A Form body's control cage as the file stores it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormCage {
    /// The archive entry (`<folder>/TSplines.BlobParts/TSpline.<uuid>.tsm`).
    pub entry: String,
    /// Its file name, `TSpline.<uuid>.tsm`: the design's serializer
    /// record names the cage by it.
    pub name: String,
    /// The design object that names the entry (the cage's serializer), when
    /// one does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serializer: Option<u64>,
    /// The object the serializer refers to right after the name (the
    /// cage's surface record).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface: Option<u64>,
    /// The timeline item the serializer is reached from (its object id and
    /// timeline index): the nearest item that refers to the serializer or
    /// the surface through the design's references.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_index: Option<i64>,
    /// The surface degree (`degree`, 3).
    pub degree: u32,
    /// The program's declarations and version records by keyword
    /// (`cap-type`, `star-smoothness`, `units`, `end-conditions`,
    /// `star-knot-rule`, `ver`, `behavior-version`, `compat-version`), the
    /// operands as written.
    pub settings: Vec<(String, String)>,
    /// The tolerances (`tol`, `geom-tol`), cm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometric_tolerance: Option<f64>,
    /// The control vertices, the populated `v` records in order.
    pub vertices: Vec<CageVertex>,
    /// The faces, the populated `f` records in order, each its ring of
    /// vertices (indices into `vertices`) in its half-edges' order.
    pub faces: Vec<Vec<u32>>,
    /// The edges, the populated `e` records in order.
    pub edges: Vec<CageEdge>,
    /// The secondary grips (grips that are not a vertex's own point).
    pub grips: Vec<CageGrip>,
    /// The records kept as written that have no field above: symmetry
    /// blocks (`105…`), editor selections (`100edges`, `100verts`,
    /// `50000grip`), the secondary grips' layouts (`0m cg`), the edge-knot
    /// mirror (`106ek`) and anything unknown or unreadable; each with its
    /// record number (line, 0 the `#TS0200` line).
    pub native: Vec<(u32, String)>,
    /// The native slots of the populated elements (each `vertices[i]`'s
    /// `v` record number among the `v` records, deleted ones counted, and
    /// likewise the faces and edges), so a writer can give the program back.
    pub vertex_slots: Vec<u32>,
    pub face_slots: Vec<u32>,
    pub edge_slots: Vec<u32>,
    /// What did not read or does not hold together (the cage is kept with
    /// what did).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// The number of half-edge records (`l`), deleted ones counted.
    #[serde(skip_serializing_if = "is_zero")]
    pub half_edges: u64,
}

/// A control vertex: its own grip's point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CageVertex {
    /// The point as written (cm, the component's coordinates).
    pub position: Vec3,
    /// The grip's weight as written (1 unless the point is rational).
    #[serde(skip_serializing_if = "is_one")]
    pub weight: f64,
    /// Smooth, dart, crease or corner, by the creased edges at it.
    #[serde(skip_serializing_if = "is_smooth")]
    pub tag: VertexTag,
    /// The `v` record's direction word (`NORTH`, `EAST`, `SOUTH`, `WEST`):
    /// where its grip layout starts.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub direction: String,
}

/// An edge between two control vertices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CageEdge {
    /// From the mate's vertex to the root half-edge's vertex (indices into
    /// `vertices`).
    pub vertices: [u32; 2],
    /// The knot interval (`e` record's scalar, nominally 1).
    #[serde(skip_serializing_if = "is_one")]
    pub knot: f64,
    /// Marked as a crease (`ec`).
    #[serde(skip_serializing_if = "is_false")]
    pub crease: bool,
    /// On the cage's boundary (one of its half-edges has no face).
    #[serde(skip_serializing_if = "is_false")]
    pub boundary: bool,
}

/// A secondary grip of a vertex: a control point of its own beside the
/// vertex's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CageGrip {
    /// The vertex it belongs to (index into `vertices`).
    pub vertex: u32,
    /// cm, as written.
    pub position: Vec3,
    #[serde(skip_serializing_if = "is_one")]
    pub weight: f64,
}

/// A vertex's subdivision tag, by the number of creased edges at it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VertexTag {
    /// No creased edge.
    #[default]
    Smooth,
    /// One.
    Dart,
    /// Two.
    Crease,
    /// Three or more.
    Corner,
}

impl Default for CageVertex {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            weight: 1.0,
            tag: VertexTag::Smooth,
            direction: String::new(),
        }
    }
}

impl Default for CageEdge {
    fn default() -> Self {
        Self {
            vertices: [0, 0],
            knot: 1.0,
            crease: false,
            boundary: false,
        }
    }
}

impl Default for CageGrip {
    fn default() -> Self {
        Self {
            vertex: 0,
            position: [0.0; 3],
            weight: 1.0,
        }
    }
}

impl VertexTag {
    /// The tag of a vertex with `creases` creased edges.
    pub fn of(creases: usize) -> VertexTag {
        match creases {
            0 => VertexTag::Smooth,
            1 => VertexTag::Dart,
            2 => VertexTag::Crease,
            _ => VertexTag::Corner,
        }
    }
}

impl FormCage {
    /// The box of every grip (the vertices' and the secondary ones), cm;
    /// None without grips. The surface lies inside its control points'
    /// convex hull, so inside this box.
    pub fn grip_box(&self) -> Option<(Vec3, Vec3)> {
        let mut points = self
            .vertices
            .iter()
            .map(|v| v.position)
            .chain(self.grips.iter().map(|g| g.position))
            .filter(|p| p.iter().all(|c| c.is_finite()));
        let first = points.next()?;
        Some(points.fold((first, first), |(lo, hi), p| {
            (
                std::array::from_fn(|i| lo[i].min(p[i])),
                std::array::from_fn(|i| hi[i].max(p[i])),
            )
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cage_survives_its_json() {
        let cage = FormCage {
            entry: "A/TSplines.BlobParts/TSpline.x.tsm".into(),
            name: "TSpline.x.tsm".into(),
            item_index: Some(3),
            degree: 3,
            vertices: vec![
                CageVertex {
                    position: [0.0, 0.0, 0.0],
                    weight: 1.0,
                    tag: VertexTag::Dart,
                    direction: "NORTH".into(),
                },
                CageVertex {
                    position: [1.0, 0.0, 0.0],
                    weight: 0.5,
                    ..CageVertex::default()
                },
            ],
            edges: vec![CageEdge {
                vertices: [0, 1],
                knot: 1.0,
                crease: true,
                boundary: true,
            }],
            ..FormCage::default()
        };
        let json = serde_json::to_value(&cage).unwrap();
        // Defaults are left out: a weight of 1, a smooth vertex.
        assert!(json["vertices"][0].get("weight").is_none());
        assert!(json["vertices"][1].get("tag").is_none());
        assert_eq!(json["vertices"][0]["tag"], "dart");
        let back: FormCage = serde_json::from_value(json).unwrap();
        assert_eq!(back, cage);
        assert_eq!(cage.grip_box(), Some(([0.0; 3], [1.0, 0.0, 0.0])));
    }
}
