// SPDX-License-Identifier: MIT
//! Chamfer: bevels edges of one body in edge sets, with an equal distance,
//! two distances or a distance and an angle (the chamfer types).
//!
//! Two distances and distance-angle measure the first distance (and the
//! angle, from that face) on a reference face, stored as a face name
//! (the "left"/"right" sides of .f3d chamfers, resolved by the importer);
//! without one on the first face of each edge's name. `flip` takes the
//! other face. The corner type shapes the vertices where three or more
//! bevelled edges meet (see the kernel's `chamfer`).
//!
//! In files and commands a single set without faces or a reference face is
//! written with `edges`, `size` and `flip` next to `body`; the general form
//! lists `sets`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{Chamfer, ChamferSize, Kernel};
use crate::parameters::ParamId;
use crate::topo::{EdgeName, FaceName};

/// Distances in millimetres, the angle in radians.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChamferSizeDef<P = ParamId> {
    EqualDistance { distance: P },
    TwoDistances { distance1: P, distance2: P },
    DistanceAngle { distance: P, angle: P },
}

/// The corner type where three or more chamfered edges meet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChamferCorner {
    /// The geometry kernel's own patch.
    #[default]
    Chamfer,
    /// The bevels continued until they meet.
    Miter,
    /// A smooth patch tangent to the bevels and the faces.
    Blend,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChamferSetDef<P = ParamId> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<EdgeName>,
    /// Every edge of these faces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<FaceName>,
    pub size: ChamferSizeDef<P>,
    /// The face the first distance (and the angle) is measured on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_face: Option<FaceName>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub tangent_chain: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChamferDef<P = ParamId> {
    pub body: BodyUid,
    pub sets: Vec<ChamferSetDef<P>>,
    pub corner: ChamferCorner,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_chamfer_corner(value: &ChamferCorner) -> bool {
    *value == ChamferCorner::Chamfer
}

/// The file and command form: `edges`, `size` and `flip` for one plain set,
/// or `sets`.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
struct ChamferForm<P> {
    body: BodyUid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    edges: Option<Vec<EdgeName>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size: Option<ChamferSizeDef<P>>,
    #[serde(default, skip_serializing_if = "is_false")]
    flip: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sets: Option<Vec<ChamferSetDef<P>>>,
    #[serde(default, skip_serializing_if = "is_chamfer_corner")]
    corner: ChamferCorner,
}

#[derive(Serialize)]
struct ChamferFormRef<'a, P> {
    body: &'a BodyUid,
    #[serde(skip_serializing_if = "Option::is_none")]
    edges: Option<&'a [EdgeName]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<&'a ChamferSizeDef<P>>,
    #[serde(skip_serializing_if = "is_false")]
    flip: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    sets: Option<&'a [ChamferSetDef<P>]>,
    #[serde(skip_serializing_if = "is_chamfer_corner")]
    corner: ChamferCorner,
}

impl<P: Serialize> Serialize for ChamferDef<P> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let plain = match self.sets.as_slice() {
            [set] if set.faces.is_empty() && set.reference_face.is_none() && set.tangent_chain => {
                Some(set)
            }
            _ => None,
        };
        let form = ChamferFormRef {
            body: &self.body,
            edges: plain.map(|set| set.edges.as_slice()),
            size: plain.map(|set| &set.size),
            flip: plain.is_some_and(|set| set.flip),
            sets: plain.is_none().then_some(self.sets.as_slice()),
            corner: self.corner,
        };
        form.serialize(serializer)
    }
}

impl<'de, P: Deserialize<'de>> Deserialize<'de> for ChamferDef<P> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let form = ChamferForm::<P>::deserialize(deserializer)?;
        let sets = match (form.edges, form.size, form.sets) {
            (Some(edges), Some(size), None) => vec![ChamferSetDef {
                edges,
                faces: Vec::new(),
                size,
                reference_face: None,
                flip: form.flip,
                tangent_chain: true,
            }],
            (None, None, Some(sets)) if !form.flip => sets,
            _ => {
                return Err(serde::de::Error::custom(
                    "a chamfer has either `edges`, `size` and `flip` or `sets`",
                ));
            }
        };
        Ok(Self {
            body: form.body,
            sets,
            corner: form.corner,
        })
    }
}

impl<P> ChamferDef<P> {
    pub const TYPE: &'static str = "chamfer";
    pub const BASE_NAME: &'static str = "Chamfer";

    /// Slots: `size.distance` (...) for the first set, `sets[i].size.distance`
    /// for the others.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<ChamferDef<Q>, E> {
        let mut sets = Vec::with_capacity(self.sets.len());
        for (i, set) in self.sets.iter().enumerate() {
            let prefix = if i == 0 {
                "size".to_owned()
            } else {
                format!("sets[{i}].size")
            };
            let mut slot = |name: &str, value: &P| f(&format!("{prefix}.{name}"), value);
            let size = match &set.size {
                ChamferSizeDef::EqualDistance { distance } => ChamferSizeDef::EqualDistance {
                    distance: slot("distance", distance)?,
                },
                ChamferSizeDef::TwoDistances {
                    distance1,
                    distance2,
                } => ChamferSizeDef::TwoDistances {
                    distance1: slot("distance1", distance1)?,
                    distance2: slot("distance2", distance2)?,
                },
                ChamferSizeDef::DistanceAngle { distance, angle } => {
                    ChamferSizeDef::DistanceAngle {
                        distance: slot("distance", distance)?,
                        angle: slot("angle", angle)?,
                    }
                }
            };
            sets.push(ChamferSetDef {
                edges: set.edges.clone(),
                faces: set.faces.clone(),
                size,
                reference_face: set.reference_face.clone(),
                flip: set.flip,
                tangent_chain: set.tangent_chain,
            });
        }
        Ok(ChamferDef {
            body: self.body,
            sets,
            corner: self.corner,
        })
    }
}

fn check_angle(angle: f64) -> Result<(), String> {
    if angle > 0.0 && angle < std::f64::consts::FRAC_PI_2 {
        Ok(())
    } else {
        Err(format!(
            "the chamfer angle must be between 0 and 90 degrees, got {angle} rad"
        ))
    }
}

impl FeatureInfo for ChamferDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        for set in &self.sets {
            references.edges(&set.edges);
            add_faces(&mut references, &set.faces);
            add_faces(&mut references, &set.reference_face);
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
            if let Some(face) = &set.reference_face {
                check_faces(ctx, std::slice::from_ref(face), "reference face").map_err(at)?;
            }
            for edge in &set.edges {
                if self.sets[..i].iter().any(|s| s.edges.contains(edge)) {
                    return Err(at(format!("edge {edge} is in two edge sets")));
                }
            }
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let positive = |id: ParamId| {
            let v = value(id);
            if v > 0.0 {
                Ok(())
            } else {
                Err(format!(
                    "chamfer distance must be greater than zero, got {v}"
                ))
            }
        };
        for set in &self.sets {
            match &set.size {
                ChamferSizeDef::EqualDistance { distance } => positive(*distance)?,
                ChamferSizeDef::TwoDistances {
                    distance1,
                    distance2,
                } => positive(*distance1).and(positive(*distance2))?,
                ChamferSizeDef::DistanceAngle { distance, angle } => {
                    positive(*distance).and(check_angle(value(*angle)))?
                }
            }
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for ChamferDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let sizes = self.sizes(ctx)?;
        let shape = ctx.body(self.body)?;
        for set in &self.sets {
            ctx.require_edges(self.body, &shape, &set.edges)?;
            require_faces(ctx, self.body, &shape, &set.faces)?;
            require_faces(ctx, self.body, &shape, set.reference_face.as_slice())?;
        }
        let uid = ctx.uid;
        let bevelled = self.apply(ctx, uid, &shape, sizes)?;
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, bevelled)],
            ..FeatureOutput::default()
        })
    }
}

impl ChamferDef {
    /// Bevels the sets' edges of `shape`, the new faces named after
    /// `feature` (the chamfer's copies on a pattern's copies, mitcad#105,
    /// are named after the pattern).
    pub(crate) fn bevel<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        feature: FeatureUid,
        shape: &K::Shape,
    ) -> Result<K::Shape, String> {
        let sizes = self.sizes(ctx)?;
        self.apply(ctx, feature, shape, sizes)
    }

    fn sizes<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<Vec<ChamferSize>, String> {
        let mut sizes = Vec::with_capacity(self.sets.len());
        for set in &self.sets {
            sizes.push(match &set.size {
                ChamferSizeDef::EqualDistance { distance } => ChamferSize::EqualDistance {
                    distance: ctx.positive(*distance)?,
                },
                ChamferSizeDef::TwoDistances {
                    distance1,
                    distance2,
                } => ChamferSize::TwoDistances {
                    distance1: ctx.positive(*distance1)?,
                    distance2: ctx.positive(*distance2)?,
                },
                ChamferSizeDef::DistanceAngle { distance, angle } => {
                    let distance = ctx.positive(*distance)?;
                    let angle_value = ctx.param(*angle)?;
                    check_angle(angle_value)
                        .map_err(|e| format!("{}: {e}", ctx.param_name(*angle)))?;
                    ChamferSize::DistanceAngle {
                        distance,
                        angle: angle_value,
                    }
                }
            });
        }
        Ok(sizes)
    }

    fn apply<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        feature: FeatureUid,
        shape: &K::Shape,
        sizes: Vec<ChamferSize>,
    ) -> Result<K::Shape, String> {
        let sets: Vec<Chamfer<'_>> = self
            .sets
            .iter()
            .zip(sizes)
            .map(|(set, size)| Chamfer {
                edges: &set.edges,
                faces: &set.faces,
                size,
                reference: set.reference_face.as_ref(),
                flip: set.flip,
                tangent_chain: set.tangent_chain,
            })
            .collect();
        let bevelled = ctx
            .kernel
            .chamfer(feature, shape, &sets, self.corner)
            .map_err(kernel_error)?;
        ctx.warn_notes(&bevelled);
        Ok(bevelled)
    }
}

#[cfg(test)]
mod tests {
    use crate::features::{FeatureDef, ValueInput};

    fn parse(json: &str) -> FeatureDef<ValueInput> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_short_form_stays_and_sets_round_trip() {
        let short = r#"{"type":"chamfer","body":"F2.b0","edges":["E{F2:a|F2:b}"],"size":{"type":"two_distances","distance1":1.0,"distance2":3.0},"flip":true}"#;
        assert_eq!(serde_json::to_string(&parse(short)).unwrap(), short);
        let full = parse(
            r#"{"type":"chamfer","body":"F2.b0","sets":[
                {"faces":["F2:a"],"size":{"type":"equal_distance","distance":2.0}},
                {"edges":["E{F2:a|F2:b}"],"size":{"type":"distance_angle","distance":5.0,"angle":0.5},
                 "reference_face":"F2:b","tangent_chain":false}],"corner":"miter"}"#,
        );
        assert_eq!(parse(&serde_json::to_string(&full).unwrap()), full);
        let mut slots = Vec::new();
        full.map_params(&mut |slot, _| {
            slots.push(slot.to_owned());
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(
            slots,
            [
                "size.distance",
                "sets[1].size.distance",
                "sets[1].size.angle"
            ]
        );
        let error = serde_json::from_str::<FeatureDef<ValueInput>>(
            r#"{"type":"chamfer","body":"F2.b0","sets":[],"flip":true}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("either"), "{error}");
    }
}
