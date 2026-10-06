// SPDX-License-Identifier: MIT
//! Loft: a solid through two or more sections (sketch regions, faces of
//! bodies, and points at the ends) in order, smooth or ruled, open or
//! closed, optionally along a centre line or through rails, with end
//! conditions at the first and the last section and the operations of an
//! extrusion. End conditions and rails need an open, smooth loft; with a
//! centre line, end conditions fail as unsupported.

use serde::{Deserialize, Serialize};

use super::extrude::ProfileRef;
use super::extrude::{
    Operation, apply_operation, check_participants, participant_bodies, profile_regions,
};
use super::geom_ref::{GeomRef, Want};
use super::path::CurvePath;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse,
    UNSUPPORTED, is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::profile::ProfileRegion;
use crate::sweeps::{LoftEnd, LoftEndKind, LoftSection, LoftSpec, PathCurve};
use crate::topo::{FaceName, RegionKey};

/// A section of a loft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LoftSectionDef {
    /// A region of a sketch (without holes).
    Profile {
        sketch: FeatureUid,
        region: RegionKey,
    },
    /// A face of a body (its outer boundary).
    Face { body: BodyUid, face: FaceName },
    /// A point at the start or the end: the origin, a construction point, a
    /// vertex, a sketch point (`{"sketch": "F3", "point": "p5"}`) or a
    /// fixed point.
    Point { point: GeomRef },
}

impl LoftSectionDef {
    fn is_point(&self) -> bool {
        matches!(self, Self::Point { .. })
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Profile { .. } => "profile",
            Self::Face { .. } => "face",
            Self::Point { .. } => "point",
        }
    }
}

/// How the loft leaves its first or arrives at its last section (the end
/// conditions; see [`crate::LoftEnd`] for the geometry). Weights are
/// unitless, greater than 0 and at most 10; angles lie between -90 and 90
/// degrees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EndCondition<P = ParamId> {
    Free,
    /// At `angle` to the section's plane.
    Direction {
        angle: P,
        weight: P,
    },
    /// Tangent to the faces next to a face section.
    Tangent {
        weight: P,
    },
    /// Curvature continuous with the faces next to a face section.
    Smooth {
        weight: P,
    },
    /// A point section: a cone-like tip.
    PointSharp,
    /// A point section: a rounded tip.
    PointTangent {
        weight: P,
    },
}

impl<P> EndCondition<P> {
    fn map_params<Q, E>(
        &self,
        prefix: &str,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<EndCondition<Q>, E> {
        let slot = |field: &str| format!("{prefix}.{field}");
        Ok(match self {
            Self::Free => EndCondition::Free,
            Self::Direction { angle, weight } => EndCondition::Direction {
                angle: f(&slot("angle"), angle)?,
                weight: f(&slot("weight"), weight)?,
            },
            Self::Tangent { weight } => EndCondition::Tangent {
                weight: f(&slot("weight"), weight)?,
            },
            Self::Smooth { weight } => EndCondition::Smooth {
                weight: f(&slot("weight"), weight)?,
            },
            Self::PointSharp => EndCondition::PointSharp,
            Self::PointTangent { weight } => EndCondition::PointTangent {
                weight: f(&slot("weight"), weight)?,
            },
        })
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Direction { .. } => "direction",
            Self::Tangent { .. } => "tangent",
            Self::Smooth { .. } => "smooth",
            Self::PointSharp => "point_sharp",
            Self::PointTangent { .. } => "point_tangent",
        }
    }

    fn for_points(&self) -> bool {
        matches!(self, Self::PointSharp | Self::PointTangent { .. })
    }

    /// False for the conditions that impose nothing: free, sharp points.
    fn imposes(&self) -> bool {
        !matches!(self, Self::Free | Self::PointSharp)
    }

    /// Whether the condition can be given at the section: a direction at a
    /// sketch profile, tangent and smooth at a face, point conditions at a
    /// point; free anywhere.
    fn fits(&self, section: &LoftSectionDef) -> bool {
        match self {
            Self::Free => true,
            Self::Direction { .. } => matches!(section, LoftSectionDef::Profile { .. }),
            Self::Tangent { .. } | Self::Smooth { .. } => {
                matches!(section, LoftSectionDef::Face { .. })
            }
            Self::PointSharp | Self::PointTangent { .. } => section.is_point(),
        }
    }
}

impl EndCondition {
    /// The kernel's end: the values evaluated and checked.
    fn evaluate<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        which: &str,
    ) -> Result<LoftEnd, String> {
        let weight_of = |ctx: &mut EvalContext<'_, K>, id: ParamId| {
            let weight = ctx.positive(id)?;
            if weight > 10.0 {
                return Err(format!(
                    "the {which} condition's weight must be at most 10, got {weight}"
                ));
            }
            Ok(weight)
        };
        let (kind, angle, weight) = match self {
            Self::Free => (LoftEndKind::Free, 0.0, 1.0),
            Self::PointSharp => (LoftEndKind::PointSharp, 0.0, 1.0),
            Self::Direction { angle, weight } => {
                let angle = ctx.param(*angle)?;
                if angle.is_nan() || angle.abs() >= std::f64::consts::FRAC_PI_2 {
                    return Err(format!(
                        "the {which} condition's angle must lie between -90 and 90 degrees"
                    ));
                }
                (LoftEndKind::Direction, angle, weight_of(ctx, *weight)?)
            }
            Self::Tangent { weight } => (LoftEndKind::Tangent, 0.0, weight_of(ctx, *weight)?),
            Self::Smooth { weight } => (LoftEndKind::Smooth, 0.0, weight_of(ctx, *weight)?),
            Self::PointTangent { weight } => {
                (LoftEndKind::PointTangent, 0.0, weight_of(ctx, *weight)?)
            }
        };
        Ok(LoftEnd {
            kind,
            angle,
            weight,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoftDef<P = ParamId> {
    pub sections: Vec<LoftSectionDef>,
    /// At the first section (free, or sharp for a point, when left out).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub start_condition: Option<EndCondition<P>>,
    /// At the last section.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub end_condition: Option<EndCondition<P>>,
    /// Keeps the sections at right angles to it on the way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub centerline: Option<CurvePath>,
    /// Curves the loft's surface passes through, each meeting every section.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rails: Vec<CurvePath>,
    /// Runs from the last section back to the first.
    #[serde(default, skip_serializing_if = "is_false")]
    pub closed: bool,
    /// Straight between neighbouring sections.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ruled: bool,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
}

impl<P> LoftDef<P> {
    pub const TYPE: &'static str = "loft";
    pub const BASE_NAME: &'static str = "Loft";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<LoftDef<Q>, E> {
        Ok(LoftDef {
            sections: self.sections.clone(),
            start_condition: self
                .start_condition
                .as_ref()
                .map(|c| c.map_params("start_condition", f))
                .transpose()?,
            end_condition: self
                .end_condition
                .as_ref()
                .map(|c| c.map_params("end_condition", f))
                .transpose()?,
            centerline: self.centerline.clone(),
            rails: self.rails.clone(),
            closed: self.closed,
            ruled: self.ruled,
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }
}

impl FeatureInfo for LoftDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for section in &self.sections {
            match section {
                LoftSectionDef::Profile { sketch, .. } => {
                    references.features.insert(*sketch);
                }
                LoftSectionDef::Face { body, face } => {
                    references.body(*body);
                    references.features.extend(face.features());
                }
                LoftSectionDef::Point { point } => point.add_to(&mut references),
            }
        }
        for path in self.centerline.iter().chain(&self.rails) {
            path.add_references(&mut references);
        }
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_participants(ctx, self.operation, &self.participants)?;
        let count = self.sections.len();
        if count < 2 {
            return Err("a loft needs two or more sections".to_owned());
        }
        for (i, section) in self.sections.iter().enumerate() {
            let at = |e: String| format!("section {}: {e}", i + 1);
            if section.is_point() && i != 0 && i + 1 != count {
                return Err(at(
                    "a point can only be the first or the last section".to_owned()
                ));
            }
            if section.is_point() && self.closed {
                return Err(at("a closed loft has no point sections".to_owned()));
            }
            if self.sections[..i].contains(section) {
                return Err(at("it is listed more than once".to_owned()));
            }
            match section {
                LoftSectionDef::Profile { sketch, .. } => {
                    ctx.sketch(*sketch).map_err(at)?;
                }
                LoftSectionDef::Face { body, face } => {
                    ctx.body(*body).map_err(at)?;
                    for feature in face.features() {
                        ctx.feature(feature)
                            .map_err(|e| at(format!("face {face}: {e}")))?;
                    }
                }
                LoftSectionDef::Point { point } => point.check(ctx, Want::Point).map_err(at)?,
            }
        }
        if self.sections.iter().all(LoftSectionDef::is_point) {
            return Err("a loft needs a section that is not a point".to_owned());
        }
        let mut imposed = false;
        for (condition, section, which) in [
            (&self.start_condition, &self.sections[0], "start"),
            (&self.end_condition, &self.sections[count - 1], "end"),
        ] {
            if let Some(condition) = condition {
                if !condition.fits(section) {
                    let kind = if !section.is_point() && condition.for_points() {
                        "curve"
                    } else {
                        section.kind()
                    };
                    return Err(format!(
                        "the {which} condition {} does not fit a {kind} section",
                        condition.name(),
                    ));
                }
                imposed |= condition.imposes();
            }
        }
        if self.centerline.is_some() && !self.rails.is_empty() {
            return Err("a loft has a centre line or rails, not both".to_owned());
        }
        if self.closed && (self.centerline.is_some() || !self.rails.is_empty()) {
            return Err("a closed loft has no centre line or rails".to_owned());
        }
        if self.closed && imposed {
            return Err("a closed loft has no end conditions".to_owned());
        }
        if self.ruled && (imposed || !self.rails.is_empty()) {
            return Err("a ruled loft has no end conditions or rails".to_owned());
        }
        for path in self.centerline.iter().chain(&self.rails) {
            path.check(ctx)?;
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }

    fn new_component(&self) -> bool {
        self.operation == Operation::NewComponent
    }

    fn tool_use(&self) -> Option<ToolUse> {
        Some(ToolUse {
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }
}

/// A section's inputs as evaluation finds them.
enum Found<S> {
    Region(crate::profile::SketchFrame, ProfileRegion),
    Face(S, FaceName),
    Point([f64; 3]),
}

impl<K: Kernel> Evaluate<K> for LoftDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let mut ends = [LoftEnd::default(); 2];
        for (end, (condition, which)) in ends.iter_mut().zip([
            (&self.start_condition, "start"),
            (&self.end_condition, "end"),
        ]) {
            if let Some(condition) = condition {
                *end = condition.evaluate(ctx, which)?;
            }
        }
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let mut found: Vec<Found<K::Shape>> = Vec::new();
        for (i, section) in self.sections.iter().enumerate() {
            let at = |e: String| format!("section {}: {e}", i + 1);
            found.push(match section {
                LoftSectionDef::Profile { sketch, region } => {
                    let profile = ProfileRef {
                        sketch: *sketch,
                        region: region.clone(),
                    };
                    let (output, mut regions) =
                        profile_regions(ctx, std::slice::from_ref(&profile)).map_err(at)?;
                    let region = regions.remove(0);
                    if region.loops.len() > 1 {
                        return Err(at(format!(
                            "{UNSUPPORTED}a loft section with holes ({})",
                            region.key
                        )));
                    }
                    Found::Region(output.frame, region)
                }
                LoftSectionDef::Face { body, face } => {
                    let shape = ctx.body(*body).map_err(at)?;
                    if ctx
                        .kernel
                        .count_faces(&shape, face)
                        .map_err(|e| at(e.to_string()))?
                        == 0
                    {
                        return Err(at(format!("body {body} has no face {face}")));
                    }
                    Found::Face(shape, face.clone())
                }
                LoftSectionDef::Point { point } => Found::Point(ctx.point(point).map_err(at)?),
            });
        }
        let centerline: Option<Vec<PathCurve>> = match &self.centerline {
            Some(path) => Some(path.resolve(ctx).map_err(|e| format!("centre line: {e}"))?),
            None => None,
        };
        let mut rails: Vec<Vec<PathCurve>> = Vec::new();
        for (i, path) in self.rails.iter().enumerate() {
            rails.push(
                path.resolve(ctx)
                    .map_err(|e| format!("rail {}: {e}", i + 1))?,
            );
        }
        let sections = found
            .iter()
            .map(|f| match f {
                Found::Region(frame, region) => LoftSection::Region {
                    frame: *frame,
                    region,
                },
                Found::Face(body, face) => LoftSection::Face { body, face },
                Found::Point(p) => LoftSection::Point(*p),
            })
            .collect();
        let spec = LoftSpec {
            feature: ctx.uid,
            sections,
            ruled: self.ruled,
            closed: self.closed,
            centerline: centerline.as_deref(),
            rails: rails.iter().map(Vec::as_slice).collect(),
            start: ends[0],
            end: ends[1],
        };
        let tool = ctx.kernel.loft(&spec).map_err(|e| e.to_string())?;
        apply_operation(ctx, self.operation, participants, tool, "loft")
    }
}
