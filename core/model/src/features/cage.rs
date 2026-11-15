// SPDX-License-Identifier: MIT
//! The control cage of an imported Form body (mitcad#1365).
//!
//! A Form body of an `.f3d` file comes in as its stored faces (a base
//! feature's body); the cage those faces were made from (a T-spline cage)
//! is kept with the body as data ([`super::BaseBody::cage`]), saved and
//! read with the design, so that it is not lost. It does not take part in
//! the body's evaluation: the body is its B-rep data alone.
//!
//! Where the kept cage maps onto the cage of MitCad's own Form feature
//! (mitcad#1757, [`ControlCage::form_cage`]: degree 3, every control point
//! its vertex's own with a weight of 1, every knot interval the same), it
//! can be opened as an editable Form (`form.create` with the primitive
//! `kept_cage`); the Form's surface is the Catmull-Clark limit surface of
//! that cage, which is not the stored faces exactly (near extraordinary
//! vertices the two subdivide differently), so the imported body stays as
//! it is beside it.
//!
//! Lengths are millimetres in the coordinates of the body's component.

use std::collections::{BTreeMap, BTreeSet};

use mitcad_subdiv::cage::edge_key;
use mitcad_subdiv::orient::orient;
use mitcad_subdiv::{Cage, CageFace, Crease};

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

    /// The cage as a Form's control cage (mitcad#1757), where it maps onto
    /// one: a surface of degree 3 whose control points are its vertices'
    /// own, each with a weight of 1, and whose edges all have the same
    /// knot interval (a uniform cage: on its regular quads the same bicubic
    /// surface as a Form's; see the module).
    /// Its vertices and faces get the ids 1, 2, … in their order (vertices
    /// on no face are left out), its faces are turned to run
    /// counter-clockwise seen from outside, its creased edges between two
    /// faces are creases, sharp for good, and its corner vertices corners.
    /// Err says in words why it does not map (the body then stays as the
    /// file stored it).
    pub fn form_cage(&self) -> Result<Cage, String> {
        if self.check().is_err() {
            return Err("the kept cage is damaged".to_owned());
        }
        if self.degree != 3 {
            return Err(format!(
                "the kept cage makes a surface of degree {}; a Form's is of degree 3",
                self.degree
            ));
        }
        if self.vertices.iter().any(|v| v.weight != 1.0) {
            return Err(
                "a control point of the kept cage has a weight other than 1 \
                        (a rational surface), which a Form's cage does not have"
                    .to_owned(),
            );
        }
        if !self.grips.is_empty() {
            return Err(
                "the kept cage has control points beside its vertices' own, \
                        which a Form's cage does not have"
                    .to_owned(),
            );
        }
        if let Some(first) = self.edges.first().map(|e| e.knot)
            && self
                .edges
                .iter()
                .any(|e| (e.knot - first).abs() > 1e-9 * first.abs())
        {
            return Err(
                "the edges of the kept cage have knot intervals that are not all \
                        the same, which a Form's cage does not have"
                    .to_owned(),
            );
        }
        if self.faces.is_empty() {
            return Err("the kept cage has no faces".to_owned());
        }
        let points: Vec<[f64; 3]> = self.vertices.iter().map(|v| v.position).collect();
        let mut faces: Vec<Vec<usize>> = self
            .faces
            .iter()
            .map(|f| f.iter().map(|&v| v as usize).collect())
            .collect();
        for face in &faces {
            if face.iter().collect::<BTreeSet<_>>().len() != face.len() {
                return Err("a face of the kept cage passes through one vertex twice".to_owned());
            }
        }
        orient(&points, &mut faces).map_err(|e| format!("the kept cage: {e}"))?;
        let used: BTreeSet<usize> = faces.iter().flatten().copied().collect();
        // Ids from 1 in the vertices' order.
        let mut ids = vec![0u32; points.len()];
        for (n, &v) in used.iter().enumerate() {
            ids[v] = n as u32 + 1;
        }
        let mut sides: BTreeMap<[u32; 2], usize> = BTreeMap::new();
        for face in &faces {
            for k in 0..face.len() {
                let edge = edge_key(ids[face[k]], ids[face[(k + 1) % face.len()]]);
                *sides.entry(edge).or_insert(0) += 1;
            }
        }
        let mut creases: Vec<Crease> = Vec::new();
        for e in self.edges.iter().filter(|e| e.crease) {
            let [a, b] = e.vertices.map(|v| ids[v as usize]);
            let edge = edge_key(a, b);
            // A boundary is sharp already.
            if a != 0
                && b != 0
                && sides.get(&edge) == Some(&2)
                && !creases.iter().any(|c| c.edge == edge)
            {
                creases.push(Crease {
                    edge,
                    sharpness: None,
                });
            }
        }
        creases.sort_by_key(|c| c.edge);
        let mut corners: Vec<u32> = self
            .vertices
            .iter()
            .enumerate()
            .filter(|(v, vertex)| vertex.tag == CageVertexTag::Corner && ids[*v] != 0)
            .map(|(v, _)| ids[v])
            .collect();
        corners.sort_unstable();
        let cage = Cage {
            vertices: used
                .iter()
                .map(|&v| mitcad_subdiv::CageVertex {
                    id: ids[v],
                    at: points[v],
                })
                .collect(),
            faces: faces
                .iter()
                .zip(1u32..)
                .map(|(face, id)| CageFace {
                    id,
                    vertices: face.iter().map(|&v| ids[v]).collect(),
                })
                .collect(),
            creases,
            corners,
            symmetry: None,
            next_vertex: 0,
            next_face: 0,
        };
        cage.check()
            .map_err(|e| format!("the kept cage makes no Form cage: {e}"))?;
        Ok(cage)
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

    /// A uniform cube cage of 20 mm as a file keeps it: one face against
    /// its neighbours, every face pointing in otherwise, the edge from the
    /// vertex at (20, 0, 0) to the one at (20, 20, 0) creased, the vertex
    /// at (20, 20, 20) a corner and one vertex on no face.
    fn cube() -> ControlCage {
        let p = [
            [0.0, 0.0, 0.0],
            [20.0, 0.0, 0.0],
            [20.0, 20.0, 0.0],
            [0.0, 20.0, 0.0],
            [0.0, 0.0, 20.0],
            [20.0, 0.0, 20.0],
            [20.0, 20.0, 20.0],
            [0.0, 20.0, 20.0],
            [50.0, 50.0, 50.0],
        ];
        let out: [[u32; 4]; 6] = [
            [0, 3, 2, 1],
            [4, 5, 6, 7],
            [0, 1, 5, 4],
            [1, 2, 6, 5],
            [2, 3, 7, 6],
            [3, 0, 4, 7],
        ];
        // Each face pointing in, but the fourth, which points out.
        let faces: Vec<Vec<u32>> = out
            .iter()
            .enumerate()
            .map(|(k, f)| {
                let mut f = f.to_vec();
                if k != 3 {
                    f.reverse();
                }
                f
            })
            .collect();
        let mut edges: Vec<CageEdge> = Vec::new();
        for f in &out {
            for k in 0..4 {
                let (a, b) = (f[k], f[(k + 1) % 4]);
                if !edges
                    .iter()
                    .any(|e| e.vertices == [a, b] || e.vertices == [b, a])
                {
                    edges.push(CageEdge {
                        vertices: [a, b],
                        knot: 1.0,
                        crease: [a, b] == [2, 1] || [a, b] == [1, 2],
                        boundary: false,
                    });
                }
            }
        }
        ControlCage {
            source: Some("TSpline.c.tsm".into()),
            degree: 3,
            vertices: p
                .iter()
                .enumerate()
                .map(|(k, &position)| CageVertex {
                    position,
                    weight: 1.0,
                    tag: if k == 6 {
                        CageVertexTag::Corner
                    } else {
                        CageVertexTag::Smooth
                    },
                })
                .collect(),
            faces,
            edges,
            grips: Vec::new(),
            settings: BTreeMap::new(),
            tolerance: None,
            geometric_tolerance: None,
            native: Vec::new(),
        }
    }

    #[test]
    fn a_uniform_kept_cage_opens_as_a_form_cage() {
        let form = cube().form_cage().expect("a Form cage");
        form.check().unwrap();
        assert!(form.full().closed());
        let ids: Vec<u32> = form.vertices.iter().map(|v| v.id).collect();
        assert_eq!(ids, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(form.position(7), Some([20.0, 20.0, 20.0]));
        assert_eq!(form.faces.len(), 6);
        // Every face points out of the cube.
        for face in &form.faces {
            let (normal, center) = form.face_normal(face);
            let out = mitcad_vec3::sub(center, [10.0, 10.0, 10.0]);
            assert!(mitcad_vec3::dot(normal, out) > 0.0, "{face:?}");
        }
        assert_eq!(form.crease(2, 3), Some(f64::INFINITY));
        assert_eq!(form.creases.len(), 1);
        assert_eq!(form.corners, [7]);
    }

    #[test]
    fn a_kept_cage_that_is_no_form_cage_says_why() {
        let mut rational = cube();
        rational.vertices[2].weight = 0.5;
        assert!(rational.form_cage().unwrap_err().contains("weight"));
        let mut grips = cube();
        grips.grips.push(CageGrip {
            vertex: 0,
            position: [1.0, 1.0, 1.0],
            weight: 1.0,
        });
        assert!(
            grips
                .form_cage()
                .unwrap_err()
                .contains("beside its vertices")
        );
        let mut knots = cube();
        knots.edges[4].knot = 2.0;
        assert!(knots.form_cage().unwrap_err().contains("knot intervals"));
        let mut degree = cube();
        degree.degree = 5;
        assert!(degree.form_cage().unwrap_err().contains("degree"));
        // The words say nothing of ids or points.
        for e in [rational, grips, knots].map(|c| c.form_cage().unwrap_err()) {
            assert!(
                !e.chars()
                    .any(|c| c.is_ascii_digit() && c != '1' && c != '3'),
                "{e}"
            );
        }
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
