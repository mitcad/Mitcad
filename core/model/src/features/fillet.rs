// SPDX-License-Identifier: MIT
//! Fillet: rounds edges of one body in edge sets. Each set has its edges
//! (named edges, and every edge of named faces), a size, the continuity
//! and the tangent chain option. A body may have any number
//! of fillets; each names its faces, so later features can refer to the
//! edges a fillet made.
//!
//! The geometry kernel always follows tangent chains. What it cannot build
//! (asymmetric and curvature continuous fillets that meet other rounded
//! edges at a corner, for instance) fails with an "unsupported:" message
//! (see [`super::face_refs`]).
//!
//! In files and commands a single plain set is written with the fields
//! `edges` and `radius` next to `body`; the general form lists `sets`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{FilletSet, FilletSize, Kernel};
use crate::parameters::ParamId;
use crate::topo::{EdgeName, FaceName, VertexName};

/// An edge set's continuity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Continuity {
    /// G1, circular cross-sections.
    #[default]
    Tangent,
    /// G2: cross-sections whose curvature continues the faces' (Mitcad's
    /// own shape, `commands.md`), shaped by the set's tangency weight.
    Curvature,
}

/// A radius along a variable fillet at a relative position (0 to 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MidRadius<P = ParamId> {
    pub position: f64,
    pub radius: P,
}

/// Millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub enum FilletSizeDef<P = ParamId> {
    Constant {
        radius: P,
    },
    /// The width across the rounding (chord length).
    ChordLength {
        length: P,
    },
    /// From `start` at `start_vertex` (an end of the edges' chain; the
    /// kernel's start of the chain when left out) to `end`, through the mid
    /// radii at their positions along the chain (0 at the start, 1 at the
    /// end), smoothly.
    Variable {
        start: P,
        end: P,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_vertex: Option<VertexName>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mid: Vec<MidRadius<P>>,
    },
    /// Two distances from the edge (in .f3d designs since 11/2025):
    /// `distance1` on the set's reference face (or on the first face of
    /// each edge's name), `distance2` on the other; `flip` swaps the faces.
    /// The cross-section is an elliptic arc (a conic) tangent to both faces.
    Asymmetric {
        distance1: P,
        distance2: P,
        #[serde(default, skip_serializing_if = "super::is_false")]
        flip: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct FilletSetDef<P = ParamId> {
    /// A name without `#k` rounds every edge between its faces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<EdgeName>,
    /// Every edge of these faces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<FaceName>,
    pub size: FilletSizeDef<P>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub tangent_chain: bool,
    #[serde(default, skip_serializing_if = "is_tangent")]
    pub continuity: Continuity,
    /// G2: how long the cross-section keeps to the faces, 0.1 to 2 (1 when
    /// left out); unitless.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tangency_weight: Option<P>,
    /// Asymmetric: the face `distance1` is measured on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_face: Option<FaceName>,
}

impl<P> FilletSetDef<P> {
    /// A set of edges with a size and the defaults: tangent chain, G1.
    pub fn new(edges: Vec<EdgeName>, size: FilletSizeDef<P>) -> Self {
        Self {
            edges,
            faces: Vec::new(),
            size,
            tangent_chain: true,
            continuity: Continuity::Tangent,
            tangency_weight: None,
            reference_face: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilletDef<P = ParamId> {
    pub body: BodyUid,
    pub sets: Vec<FilletSetDef<P>>,
    /// Rolling ball corners; false asks for setback corners.
    pub rolling_ball_corners: bool,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_tangent(value: &Continuity) -> bool {
    *value == Continuity::Tangent
}

/// The file and command form: `edges` and `radius` for one plain set, or
/// `sets`.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
struct FilletForm<P> {
    body: BodyUid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    edges: Option<Vec<EdgeName>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    radius: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sets: Option<Vec<FilletSetDef<P>>>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    rolling_ball_corners: bool,
}

/// The form with borrowed fields, for writing.
#[derive(Serialize)]
struct FilletFormRef<'a, P> {
    body: &'a BodyUid,
    #[serde(skip_serializing_if = "Option::is_none")]
    edges: Option<&'a [EdgeName]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    radius: Option<&'a P>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sets: Option<&'a [FilletSetDef<P>]>,
    #[serde(skip_serializing_if = "is_true")]
    rolling_ball_corners: bool,
}

impl<P: Serialize> Serialize for FilletDef<P> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let plain = match self.sets.as_slice() {
            [set]
                if set.faces.is_empty()
                    && set.tangent_chain
                    && is_tangent(&set.continuity)
                    && set.tangency_weight.is_none()
                    && set.reference_face.is_none() =>
            {
                match &set.size {
                    FilletSizeDef::Constant { radius } => Some((&set.edges, radius)),
                    _ => None,
                }
            }
            _ => None,
        };
        let form = FilletFormRef {
            body: &self.body,
            edges: plain.map(|(edges, _)| edges.as_slice()),
            radius: plain.map(|(_, radius)| radius),
            sets: plain.is_none().then_some(self.sets.as_slice()),
            rolling_ball_corners: self.rolling_ball_corners,
        };
        form.serialize(serializer)
    }
}

impl<'de, P: Deserialize<'de>> Deserialize<'de> for FilletDef<P> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let form = FilletForm::<P>::deserialize(deserializer)?;
        let sets = match (form.edges, form.radius, form.sets) {
            (Some(edges), Some(radius), None) => {
                vec![FilletSetDef::new(edges, FilletSizeDef::Constant { radius })]
            }
            (None, None, Some(sets)) => sets,
            _ => {
                return Err(serde::de::Error::custom(
                    "a fillet has either `edges` and `radius` or `sets`",
                ));
            }
        };
        Ok(Self {
            body: form.body,
            sets,
            rolling_ball_corners: form.rolling_ball_corners,
        })
    }
}

impl<P> FilletDef<P> {
    pub const TYPE: &'static str = "fillet";
    pub const BASE_NAME: &'static str = "Fillet";

    /// One set of edges with a constant radius.
    pub fn constant(body: BodyUid, edges: Vec<EdgeName>, radius: P) -> Self {
        Self {
            body,
            sets: vec![FilletSetDef::new(edges, FilletSizeDef::Constant { radius })],
            rolling_ball_corners: true,
        }
    }

    /// Slots: `radius` (`chord_length`, `start_radius`, `end_radius`,
    /// `mid[k].radius`, `distance1`, `distance2`, `tangency_weight`) for the
    /// first set, `sets[i].radius`, ... for the others.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<FilletDef<Q>, E> {
        let mut sets = Vec::with_capacity(self.sets.len());
        for (i, set) in self.sets.iter().enumerate() {
            let prefix = if i == 0 {
                String::new()
            } else {
                format!("sets[{i}].")
            };
            let mut slot = |name: &str, value: &P| f(&format!("{prefix}{name}"), value);
            let size = match &set.size {
                FilletSizeDef::Constant { radius } => FilletSizeDef::Constant {
                    radius: slot("radius", radius)?,
                },
                FilletSizeDef::ChordLength { length } => FilletSizeDef::ChordLength {
                    length: slot("chord_length", length)?,
                },
                FilletSizeDef::Variable {
                    start,
                    end,
                    start_vertex,
                    mid,
                } => FilletSizeDef::Variable {
                    start: slot("start_radius", start)?,
                    end: slot("end_radius", end)?,
                    start_vertex: start_vertex.clone(),
                    mid: mid
                        .iter()
                        .enumerate()
                        .map(|(k, m)| {
                            Ok(MidRadius {
                                position: m.position,
                                radius: slot(&format!("mid[{k}].radius"), &m.radius)?,
                            })
                        })
                        .collect::<Result<_, E>>()?,
                },
                FilletSizeDef::Asymmetric {
                    distance1,
                    distance2,
                    flip,
                } => FilletSizeDef::Asymmetric {
                    distance1: slot("distance1", distance1)?,
                    distance2: slot("distance2", distance2)?,
                    flip: *flip,
                },
            };
            let tangency_weight = match &set.tangency_weight {
                Some(weight) => Some(slot("tangency_weight", weight)?),
                None => None,
            };
            sets.push(FilletSetDef {
                edges: set.edges.clone(),
                faces: set.faces.clone(),
                size,
                tangent_chain: set.tangent_chain,
                continuity: set.continuity,
                tangency_weight,
                reference_face: set.reference_face.clone(),
            });
        }
        Ok(FilletDef {
            body: self.body,
            sets,
            rolling_ball_corners: self.rolling_ball_corners,
        })
    }
}

impl FeatureInfo for FilletDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        for set in &self.sets {
            references.edges(&set.edges);
            add_faces(&mut references, &set.faces);
            add_faces(&mut references, &set.reference_face);
            if let FilletSizeDef::Variable {
                start_vertex: Some(vertex),
                ..
            } = &set.size
            {
                for face in vertex.faces() {
                    references.features.extend(face.features());
                }
            }
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        if self.sets.is_empty() {
            return Err("no edges selected".to_owned());
        }
        for (i, set) in self.sets.iter().enumerate() {
            let at = |e: String| {
                if self.sets.len() > 1 {
                    format!("sets[{i}]: {e}")
                } else {
                    e
                }
            };
            if set.edges.is_empty() && set.faces.is_empty() {
                return Err(at("no edges selected".to_owned()));
            }
            if !set.edges.is_empty() {
                ctx.edges(&set.edges).map_err(at)?;
            }
            if !set.faces.is_empty() {
                check_faces(ctx, &set.faces, "faces").map_err(at)?;
            }
            for edge in &set.edges {
                if self.sets[..i].iter().any(|s| s.edges.contains(edge)) {
                    return Err(at(format!("edge {edge} is in two edge sets")));
                }
            }
            if let FilletSizeDef::Variable { mid, .. } = &set.size {
                if let Some(m) = mid.iter().find(|m| !(m.position > 0.0 && m.position < 1.0)) {
                    return Err(at(format!(
                        "a mid radius position must be between 0 and 1, got {}",
                        m.position
                    )));
                }
                if let Some(pair) = mid.windows(2).find(|p| p[1].position <= p[0].position) {
                    return Err(at(format!(
                        "the mid radius positions must increase, got {} after {}",
                        pair[1].position, pair[0].position
                    )));
                }
            }
            if let Some(face) = &set.reference_face {
                if !matches!(set.size, FilletSizeDef::Asymmetric { .. }) {
                    return Err(at(
                        "a reference face is only for asymmetric fillets".to_owned()
                    ));
                }
                check_faces(ctx, std::slice::from_ref(face), "reference face").map_err(at)?;
            }
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let positive = |id: &ParamId, what: &str| {
            let v = value(*id);
            if v > 0.0 {
                Ok(())
            } else {
                Err(format!("fillet {what} must be greater than zero, got {v}"))
            }
        };
        for set in &self.sets {
            match &set.size {
                FilletSizeDef::Constant { radius } => positive(radius, "radius")?,
                FilletSizeDef::ChordLength { length } => positive(length, "chord length")?,
                FilletSizeDef::Variable {
                    start, end, mid, ..
                } => {
                    positive(start, "start radius")?;
                    positive(end, "end radius")?;
                    for m in mid {
                        positive(&m.radius, "mid radius")?;
                    }
                }
                FilletSizeDef::Asymmetric {
                    distance1,
                    distance2,
                    ..
                } => {
                    positive(distance1, "distance")?;
                    positive(distance2, "distance")?;
                }
            }
            if let Some(weight) = set.tangency_weight {
                check_weight(value(weight))?;
            }
        }
        Ok(())
    }
}

fn check_weight(weight: f64) -> Result<(), String> {
    if (0.1..=2.0).contains(&weight) {
        Ok(())
    } else {
        Err(format!(
            "the fillet's tangency weight must be between 0.1 and 2, got {weight}"
        ))
    }
}

/// The sizes of a set, in millimetres.
enum Size {
    Constant(f64),
    ChordLength(f64),
    /// Start, end and the mid radii (position, radius).
    Variable(f64, f64, Vec<(f64, f64)>),
    Asymmetric(f64, f64),
}

impl<K: Kernel> Evaluate<K> for FilletDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (sizes, weights) = self.values(ctx)?;
        let shape = ctx.body(self.body)?;
        for set in &self.sets {
            ctx.require_edges(self.body, &shape, &set.edges)?;
            require_faces(ctx, self.body, &shape, &set.faces)?;
            require_faces(ctx, self.body, &shape, set.reference_face.as_slice())?;
        }
        let uid = ctx.uid;
        let rounded = self.apply(ctx, uid, &shape, &sizes, &weights)?;
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, rounded)],
            ..FeatureOutput::default()
        })
    }
}

impl FilletDef {
    /// Rounds the sets' edges of `shape`, the new faces named after
    /// `feature` (the fillet's copies on a pattern's copies, mitcad#105,
    /// are named after the pattern).
    pub(crate) fn round<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        feature: FeatureUid,
        shape: &K::Shape,
    ) -> Result<K::Shape, String> {
        let (sizes, weights) = self.values(ctx)?;
        self.apply(ctx, feature, shape, &sizes, &weights)
    }

    /// The sets' sizes and tangency weights.
    fn values<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<(Vec<Size>, Vec<f64>), String> {
        let mut sizes = Vec::with_capacity(self.sets.len());
        let mut weights = Vec::with_capacity(self.sets.len());
        for set in &self.sets {
            sizes.push(match &set.size {
                FilletSizeDef::Constant { radius } => Size::Constant(ctx.positive(*radius)?),
                FilletSizeDef::ChordLength { length } => Size::ChordLength(ctx.positive(*length)?),
                FilletSizeDef::Variable {
                    start, end, mid, ..
                } => {
                    let mut radii = Vec::with_capacity(mid.len());
                    for m in mid {
                        radii.push((m.position, ctx.positive(m.radius)?));
                    }
                    Size::Variable(ctx.positive(*start)?, ctx.positive(*end)?, radii)
                }
                FilletSizeDef::Asymmetric {
                    distance1,
                    distance2,
                    ..
                } => Size::Asymmetric(ctx.positive(*distance1)?, ctx.positive(*distance2)?),
            });
            weights.push(match set.tangency_weight {
                Some(id) => {
                    let weight = ctx.param(id)?;
                    check_weight(weight).map_err(|e| format!("{}: {e}", ctx.param_name(id)))?;
                    weight
                }
                None => 1.0,
            });
        }
        Ok((sizes, weights))
    }

    fn apply<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        feature: FeatureUid,
        shape: &K::Shape,
        sizes: &[Size],
        weights: &[f64],
    ) -> Result<K::Shape, String> {
        let sets: Vec<FilletSet<'_>> = self
            .sets
            .iter()
            .zip(sizes.iter().zip(weights))
            .map(|(set, (size, weight))| FilletSet {
                edges: &set.edges,
                faces: &set.faces,
                size: match (size, &set.size) {
                    (Size::Constant(radius), _) => FilletSize::Constant { radius: *radius },
                    (Size::ChordLength(length), _) => FilletSize::ChordLength { length: *length },
                    (Size::Variable(start, end, mid), def) => FilletSize::Variable {
                        start: *start,
                        end: *end,
                        start_vertex: match def {
                            FilletSizeDef::Variable { start_vertex, .. } => start_vertex.as_ref(),
                            _ => None,
                        },
                        mid,
                    },
                    (Size::Asymmetric(distance1, distance2), def) => FilletSize::Asymmetric {
                        distance1: *distance1,
                        distance2: *distance2,
                        reference: set.reference_face.as_ref(),
                        flip: matches!(def, FilletSizeDef::Asymmetric { flip: true, .. }),
                    },
                },
                tangent_chain: set.tangent_chain,
                curvature: set.continuity == Continuity::Curvature,
                weight: *weight,
            })
            .collect();
        let rounded = ctx
            .kernel
            .fillet(feature, shape, &sets, self.rolling_ball_corners)
            .map_err(kernel_error)?;
        ctx.warn_notes(&rounded);
        Ok(rounded)
    }
}

#[cfg(test)]
mod tests {
    use crate::features::{FeatureDef, ValueInput};

    fn parse(json: &str) -> Result<FeatureDef<ValueInput>, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    #[test]
    fn one_plain_set_keeps_the_short_form() {
        let short = r#"{"type":"fillet","body":"F2.b0","edges":["E{F2:a|F2:b}"],"radius":2.0}"#;
        let def = parse(short).unwrap();
        assert_eq!(serde_json::to_string(&def).unwrap(), short);
        let FeatureDef::Fillet(fillet) = &def else {
            panic!("a fillet");
        };
        assert_eq!(fillet.sets.len(), 1);
        assert!(fillet.sets[0].tangent_chain && fillet.rolling_ball_corners);

        // The same set written out in full reads the same.
        let full = parse(
            r#"{"type":"fillet","body":"F2.b0","sets":[{"edges":["E{F2:a|F2:b}"],
                "size":{"type":"constant","radius":2.0}}]}"#,
        )
        .unwrap();
        assert_eq!(full, def);
    }

    #[test]
    fn sets_with_options_round_trip_and_name_their_slots() {
        let json = r#"{"type":"fillet","body":"F2.b0","sets":[
            {"edges":["E{F2:a|F2:b}"],"size":{"type":"constant","radius":5.0},"tangent_chain":false},
            {"faces":["F2:c"],"size":{"type":"chord_length","length":"d7"}},
            {"edges":["E{F2:a|F2:d}"],"size":{"type":"variable","start":2.0,"end":4.0,
             "start_vertex":"V{F2:a|F2:b|F2:d}","mid":[{"position":0.25,"radius":3.0},
             {"position":0.5,"radius":"d8"}]},"continuity":"curvature","tangency_weight":1.5},
            {"edges":["E{F2:b|F2:d}"],"size":{"type":"asymmetric","distance1":2.0,"distance2":3.0,
             "flip":true},"reference_face":"F2:b"}],
            "rolling_ball_corners":false}"#;
        let def = parse(json).unwrap();
        let again = parse(&serde_json::to_string(&def).unwrap()).unwrap();
        assert_eq!(again, def);
        let mut slots = Vec::new();
        def.map_params(&mut |slot, _| {
            slots.push(slot.to_owned());
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(
            slots,
            [
                "radius",
                "sets[1].chord_length",
                "sets[2].start_radius",
                "sets[2].end_radius",
                "sets[2].mid[0].radius",
                "sets[2].mid[1].radius",
                "sets[2].tangency_weight",
                "sets[3].distance1",
                "sets[3].distance2"
            ]
        );
        // The weight is unitless, the mid radii lengths.
        use crate::parameters::slot_unit;
        assert!(slot_unit("sets[2].tangency_weight").is_unitless());
        assert!(!slot_unit("sets[2].mid[0].radius").is_unitless());
        // A plain set with a weight or a reference face keeps the general form.
        let weighted = parse(
            r#"{"type":"fillet","body":"F2.b0","sets":[{"edges":["E{F2:a|F2:b}"],
                "size":{"type":"constant","radius":2.0},"tangency_weight":1.0}]}"#,
        )
        .unwrap();
        assert!(
            serde_json::to_string(&weighted)
                .unwrap()
                .contains("\"sets\"")
        );
    }

    #[test]
    fn malformed_fillets_are_rejected() {
        for (json, expected) in [
            (
                r#"{"type":"fillet","body":"F2.b0","edges":["E{F2:a|F2:b}"]}"#,
                "either `edges` and `radius` or `sets`",
            ),
            (
                r#"{"type":"fillet","body":"F2.b0","edges":[],"radius":1,"sets":[]}"#,
                "either `edges` and `radius` or `sets`",
            ),
            (
                r#"{"type":"fillet","body":"F2.b0","sets":[{"edges":[],"size":{"type":"round"}}]}"#,
                "unknown variant `round`",
            ),
        ] {
            let error = parse(json).unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
    }
}
