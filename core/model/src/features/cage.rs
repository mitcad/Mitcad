// SPDX-License-Identifier: MIT
//! The control cage of an imported Form body (mitcad#1365).
//!
//! MitCad has no subdivision or T-spline surfaces of its own: no feature,
//! kernel call or editor works on a control cage (the `.dwg` import's
//! subdivision meshes come in as mesh bodies of their control mesh). A
//! Form body of an `.f3d` file comes in as its stored faces (a base
//! feature's body); the cage those faces were made from is kept with the
//! body as data ([`super::BaseBody::cage`]), saved and read with the
//! design, so that it is not lost and a later editor can start from it.
//! It does not take part in the body's evaluation: the body is its B-rep
//! data alone.
//!
//! Lengths are millimetres in the coordinates of the body's component.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::is_false;

fn is_one(x: &f64) -> bool {
    *x == 1.0
}

fn one() -> f64 {
    1.0
}

fn is_smooth(tag: &CageVertexTag) -> bool {
    *tag == CageVertexTag::Smooth
}

/// A control cage: control vertices (each with its own control point),
/// faces as vertex rings, edges with their knot intervals and creases, and
/// the secondary control points of vertices that have more than one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlCage {
    /// Where it comes from (the `.f3d` file's cage entry,
    /// `TSpline.<uuid>.tsm`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The surface degree (3).
    pub degree: u32,
    pub vertices: Vec<CageVertex>,
    /// Each face's vertices (indices into `vertices`) around it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<CageEdge>,
    /// Secondary control points.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grips: Vec<CageGrip>,
    /// The source's settings as written (`end-conditions`, `star-smoothness`,
    /// …), by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, String>,
    /// The source's tolerances (its `tol` and `geom-tol`), mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometric_tolerance: Option<f64>,
    /// The source's records that have no field here, as written (symmetry,
    /// selections, the secondary points' layouts; lengths in them in the
    /// source's units).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native: Vec<String>,
}

/// A control vertex with its own control point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CageVertex {
    /// mm.
    pub position: [f64; 3],
    /// The control point's weight, as the source writes it.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub weight: f64,
    #[serde(default, skip_serializing_if = "is_smooth")]
    pub tag: CageVertexTag,
}

/// An edge between two control vertices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CageEdge {
    /// Indices into the cage's `vertices`.
    pub vertices: [u32; 2],
    /// The knot interval (nominally 1).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub knot: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub crease: bool,
    /// On the cage's open boundary.
    #[serde(default, skip_serializing_if = "is_false")]
    pub boundary: bool,
}

/// A secondary control point of a vertex.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CageGrip {
    /// Index into the cage's `vertices`.
    pub vertex: u32,
    /// mm.
    pub position: [f64; 3],
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub weight: f64,
}

/// A vertex's subdivision tag, by the creased edges at it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CageVertexTag {
    /// No creased edge.
    #[default]
    Smooth,
    /// One creased edge.
    Dart,
    /// Two.
    Crease,
    /// Three or more.
    Corner,
}

impl ControlCage {
    /// Refuses a cage whose indices name no vertex or whose numbers are
    /// not finite.
    pub fn check(&self) -> Result<(), String> {
        let n = self.vertices.len();
        let vertex = |i: u32, what: &str| {
            if (i as usize) < n {
                Ok(())
            } else {
                Err(format!("{what} names vertex {i}, the cage has {n}"))
            }
        };
        let finite = |p: &[f64; 3], w: f64| p.iter().all(|c| c.is_finite()) && w.is_finite();
        if let Some(i) = self
            .vertices
            .iter()
            .position(|v| !finite(&v.position, v.weight))
        {
            return Err(format!("vertex {i} is not a finite point"));
        }
        for (i, face) in self.faces.iter().enumerate() {
            if face.len() < 3 {
                return Err(format!("face {i} has fewer than three vertices"));
            }
            for v in face {
                vertex(*v, &format!("face {i}"))?;
            }
        }
        for (i, edge) in self.edges.iter().enumerate() {
            for v in edge.vertices {
                vertex(v, &format!("edge {i}"))?;
            }
            if !(edge.knot.is_finite() && edge.knot > 0.0) {
                return Err(format!("edge {i}'s knot interval is not positive"));
            }
        }
        for (i, grip) in self.grips.iter().enumerate() {
            vertex(grip.vertex, &format!("grip {i}"))?;
            if !finite(&grip.position, grip.weight) {
                return Err(format!("grip {i} is not a finite point"));
            }
        }
        if [self.tolerance, self.geometric_tolerance]
            .iter()
            .flatten()
            .any(|t| !t.is_finite())
        {
            return Err("a tolerance is not finite".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::BaseDef;
    use serde_json::json;

    fn triangle() -> serde_json::Value {
        json!({
            "source": "TSpline.a.tsm",
            "degree": 3,
            "vertices": [
                {"position": [0.0, 0.0, 0.0], "tag": "dart"},
                {"position": [20.0, 0.0, 0.0]},
                {"position": [0.0, 30.0, 0.0], "weight": 0.5}
            ],
            "faces": [[1, 2, 0]],
            "edges": [
                {"vertices": [0, 1], "crease": true, "boundary": true},
                {"vertices": [1, 2], "knot": 2.0, "boundary": true},
                {"vertices": [2, 0], "boundary": true}
            ],
            "grips": [{"vertex": 2, "position": [1.0, 2.0, 3.0]}],
            "settings": {"end-conditions": "SUBD_CREASES"},
            "tolerance": 0.01,
            "native": ["105sym 0"]
        })
    }

    #[test]
    fn a_base_body_keeps_its_cage_through_its_json() {
        let def = json!({
            "bodies": [{"name": "Form1", "cage": triangle(),
                        "brep": {"format": "occt", "compression": "none", "size": 1, "data": "eA=="}}]
        });
        let base: BaseDef = serde_json::from_value(def).unwrap();
        let cage = base.bodies[0].cage.as_deref().expect("the cage");
        cage.check().unwrap();
        assert_eq!(cage.vertices[0].tag, CageVertexTag::Dart);
        assert_eq!(cage.vertices[1].weight, 1.0);
        assert_eq!(cage.edges[1].knot, 2.0);
        // Written back the same, defaults left out.
        let again = serde_json::to_value(&base).unwrap();
        assert_eq!(again["bodies"][0]["cage"], triangle());
        let back: BaseDef = serde_json::from_value(again).unwrap();
        assert_eq!(back, base);
        // A body without a cage writes no key.
        let plain = json!({"bodies": [{"brep": {"format": "occt", "compression": "none",
                                                 "size": 1, "data": "eA=="}}]});
        let plain: BaseDef = serde_json::from_value(plain).unwrap();
        assert!(
            serde_json::to_value(&plain).unwrap()["bodies"][0]
                .get("cage")
                .is_none()
        );
    }

    #[test]
    fn a_cage_that_names_no_vertex_is_refused() {
        let mut cage: ControlCage = serde_json::from_value(triangle()).unwrap();
        cage.faces[0][1] = 7;
        assert!(cage.check().unwrap_err().contains("face 0 names vertex 7"));
        let mut cage: ControlCage = serde_json::from_value(triangle()).unwrap();
        cage.edges[0].knot = 0.0;
        assert!(cage.check().is_err());
        let mut cage: ControlCage = serde_json::from_value(triangle()).unwrap();
        cage.grips[0].vertex = 3;
        assert!(cage.check().is_err());
        let mut unknown = triangle();
        unknown["colour"] = json!(1);
        assert!(serde_json::from_value::<ControlCage>(unknown).is_err());
    }
}
