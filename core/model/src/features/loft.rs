// SPDX-License-Identifier: MIT
//! Loft: a solid through two or more sections (sketch regions, faces of
//! bodies, and points at the ends) in order, smooth or ruled, open or
//! closed, optionally along a centre line or through rails, with end
//! conditions at the first and the last section and the operations of an
//! extrusion. End conditions and rails need an open, smooth loft; with a
//! centre line, end conditions fail as unsupported. A surface loft is the
//! side faces only, a sheet body of its own.

use serde::{Deserialize, Serialize};

use super::extrude::ProfileRef;
use super::extrude::{
    Operation, apply_operation, check_participants, participant_bodies, profile_regions,
};
use super::geom_ref::{GeomRef, Want};
use super::path::{CurvePath, SketchCurves};
use super::surface_sweeps;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput,
    References, ToolUse, UNSUPPORTED, is_false,
};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::profile::ProfileRegion;
use crate::sweeps::{LoftEnd, LoftEndKind, LoftFace, LoftSection, LoftSpec, PathCurve};
use crate::topo::{FaceName, RegionKey};
// The face of a curved face's sketch (mitcad#2345).
use super::sketch::SketchPlane;
// Frames along a centre line (mitcad#543).
use super::sweep::SweepFrame;
use crate::sweeps::PathFrame;
// The parallel orientation along a centre line (mitcad#547).
use crate::sweeps::SweepOrientation;

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
    /// Curves of a sketch making one chain, open or closed (mitcad#455):
    /// only in a surface loft whose sections are all curves.
    Curves {
        sketch: FeatureUid,
        #[serde(with = "super::path::curves_serde")]
        curves: Vec<EntityUid>,
        /// Adds the curves joined to them end to end.
        #[serde(default, skip_serializing_if = "is_false")]
        chain: bool,
    },
}

impl LoftSectionDef {
    fn is_point(&self) -> bool {
        matches!(self, Self::Point { .. })
    }

    fn is_curves(&self) -> bool {
        matches!(self, Self::Curves { .. })
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Profile { .. } => "profile",
            Self::Face { .. } => "face",
            Self::Point { .. } => "point",
            Self::Curves { .. } => "curves",
        }
    }

    /// The curves of a curve section.
    fn sketch_curves(&self) -> Option<SketchCurves> {
        match self {
            Self::Curves {
                sketch,
                curves,
                chain,
            } => Some(SketchCurves {
                sketch: *sketch,
                curves: curves.clone(),
                chain: *chain,
                reversed: false,
            }),
            _ => None,
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
    /// At right angles to a face the section lies on (mitcad#2345): `face`,
    /// or without it the face the section lies on (a face section's own
    /// face, a sketch profile's or a planar sketch's plane, where it is
    /// Direction at angle 0, a curved face's sketch's face).
    Perpendicular {
        weight: P,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        face: Option<EndFace>,
    },
}

/// A face of a body or surface body that a Perpendicular end leaves at
/// right angles (mitcad#2345): `{"body": "F2.b0", "face": "F2:side(c1)"}`.
/// The section must lie on it (within 0.01 mm).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndFace {
    pub body: BodyUid,
    pub face: FaceName,
}

impl<P> EndCondition<P> {
    // Written by hand: the condition maps by its kind under its prefix.
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
            Self::Perpendicular { weight, face } => EndCondition::Perpendicular {
                weight: f(&slot("weight"), weight)?,
                face: face.clone(),
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
            Self::Perpendicular { .. } => "perpendicular",
        }
    }

    /// A Perpendicular end's face, when it names one.
    fn face(&self) -> Option<&EndFace> {
        match self {
            Self::Perpendicular { face, .. } => face.as_ref(),
            _ => None,
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
    /// sketch profile or curves, tangent and smooth at a face, point
    /// conditions at a point, perpendicular at anything but a point
    /// (mitcad#2345); free anywhere.
    fn fits(&self, section: &LoftSectionDef) -> bool {
        match self {
            Self::Free => true,
            Self::Direction { .. } => matches!(
                section,
                LoftSectionDef::Profile { .. } | LoftSectionDef::Curves { .. }
            ),
            Self::Tangent { .. } | Self::Smooth { .. } => {
                matches!(section, LoftSectionDef::Face { .. })
            }
            Self::PointSharp | Self::PointTangent { .. } => section.is_point(),
            Self::Perpendicular { .. } => !section.is_point(),
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
            Self::Perpendicular { weight, .. } => {
                (LoftEndKind::Perpendicular, 0.0, weight_of(ctx, *weight)?)
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
    /// A surface through the sections instead of a solid: the side faces
    /// only, no caps (`isSolid` false in .f3d designs). A new body or
    /// component, never joined or cut.
    #[serde(default, skip_serializing_if = "is_false")]
    pub surface: bool,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// How the sections turn along the centre line instead of keeping
    /// their angle to it (mitcad#543), as a sweep's `frame`: the Frenet
    /// frame, towards an auxiliary curve, or about a constant binormal.
    /// Only with a centre line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<SweepFrame>,
    /// How the sections turn along the centre line (mitcad#547), as a
    /// sweep's: perpendicular (left out) keeps their angle to it, parallel
    /// keeps their direction, only moving them along it. Parallel only with
    /// a centre line and without a frame.
    #[serde(default, skip_serializing_if = "is_perpendicular")]
    pub orientation: SweepOrientation,
    /// Tangent Edges (mitcad#2162): each section's runs of tangent edges
    /// joined into one smooth edge (`merge`), or kept (`keep`, left out).
    #[serde(default, skip_serializing_if = "is_keep")]
    pub tangent_edges: TangentEdges,
}

fn is_perpendicular(orientation: &SweepOrientation) -> bool {
    *orientation == SweepOrientation::Perpendicular
}

/// What a loft does with a section's edges that meet tangentially (Tangent
/// Edges, mitcad#2162). `Merge` joins each run of them into one smooth edge
/// (within 1e-8 mm, by arc length), so that the side is one face between
/// such runs, tangent-continuous across their former joins and named after
/// the run's first edge; `Keep` keeps every edge its own face. Left out it
/// is `Keep`, as designs saved before the choice built; a new loft from the
/// panel or MCP takes `Merge`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TangentEdges {
    Merge,
    #[default]
    Keep,
}

fn is_keep(tangent_edges: &TangentEdges) -> bool {
    *tangent_edges == TangentEdges::Keep
}

/// How the curves of a curve section lie (mitcad#2345): in their sketch's
/// plane, on a curved face (a sketch on a cylindrical or conical face,
/// mitcad#1768) or in space (a 3D sketch's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CurvesLie {
    InPlane,
    OnFace,
    InSpace,
}

fn curves_lie(def: Option<&FeatureDef>) -> CurvesLie {
    match def {
        Some(FeatureDef::SpatialSketch(_)) => CurvesLie::InSpace,
        Some(FeatureDef::Sketch(sketch)) if sketch.plane.is_developed() => CurvesLie::OnFace,
        _ => CurvesLie::InPlane,
    }
}

/// The checks of an end condition that depend on its section (mitcad#2345):
/// a Perpendicular end's face must be there; at curves, a direction needs
/// them in a plane, and a Perpendicular end without a face needs them in a
/// plane or on a curved face's sketch (a 3D sketch's lie on no known face).
fn check_end_section(
    ctx: &CheckContext<'_>,
    condition: &EndCondition,
    section: &LoftSectionDef,
    which: &str,
) -> Result<(), String> {
    if let Some(face) = condition.face() {
        let at = |e: String| format!("the {which} condition's face: {e}");
        ctx.body(face.body).map_err(at)?;
        for feature in face.face.features() {
            ctx.feature(feature).map_err(|e| {
                at(crate::notice::about_place(
                    crate::notice::NoticePlace::face(face.face.to_string()),
                    &e.to_string(),
                ))
            })?;
        }
        return Ok(());
    }
    let LoftSectionDef::Curves { sketch, .. } = section else {
        return Ok(());
    };
    let lie = curves_lie(ctx.feature(*sketch).ok().map(|entry| &entry.def));
    match condition {
        EndCondition::Direction { .. } if lie != CurvesLie::InPlane => Err(format!(
            "the {which} condition direction needs curves that lie in a plane; give \
             perpendicular with the face they lie on instead"
        )),
        EndCondition::Perpendicular { .. } if lie == CurvesLie::InSpace => Err(format!(
            "the {which} condition perpendicular needs the face that the 3D sketch's curves \
             lie on: give it as the condition's face"
        )),
        _ => Ok(()),
    }
}

impl<P> LoftDef<P> {
    pub const TYPE: &'static str = "loft";
    pub const BASE_NAME: &'static str = "Loft";

    /// The faces the Perpendicular ends name (mitcad#2345).
    fn end_faces(&self) -> impl Iterator<Item = &EndFace> {
        self.start_condition
            .iter()
            .chain(&self.end_condition)
            .filter_map(EndCondition::<P>::face)
    }

    // Written by hand: the end conditions map with their prefixes.
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
            surface: self.surface,
            operation: self.operation,
            participants: self.participants.clone(),
            frame: self.frame.clone(),
            orientation: self.orientation,
            tangent_edges: self.tangent_edges,
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
                LoftSectionDef::Curves { sketch, .. } => {
                    references.features.insert(*sketch);
                }
            }
        }
        for path in self.centerline.iter().chain(&self.rails) {
            path.add_references(&mut references);
        }
        if let Some(SweepFrame::Auxiliary { curve, .. }) = &self.frame {
            curve.add_references(&mut references);
        }
        for body in &self.participants {
            references.body(*body);
        }
        // The faces of Perpendicular ends (mitcad#2345).
        for face in self.end_faces() {
            references.body(face.body);
            references.features.extend(face.face.features());
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_participants(ctx, self.operation, &self.participants)?;
        let count = self.sections.len();
        if count < 2 {
            return Err("a loft needs two or more sections".to_owned());
        }
        if self.surface && !matches!(self.operation, Operation::NewBody | Operation::NewComponent) {
            return Err("a surface loft is a new body or a new component".to_owned());
        }
        // Sections of curves (mitcad#455).
        if self.sections.iter().any(LoftSectionDef::is_curves) {
            if !self.surface {
                return Err("only a surface loft has sections of curves".to_owned());
            }
            if !self.sections.iter().all(LoftSectionDef::is_curves) {
                return Err("a loft of curve sections has only curve sections".to_owned());
            }
            // End conditions, since mitcad#2345; rails and a center line
            // not.
            if self.centerline.is_some() || !self.rails.is_empty() {
                return Err("a loft of curve sections has no center line or rails".to_owned());
            }
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
                    ctx.profile_sketch(*sketch).map_err(at)?;
                }
                LoftSectionDef::Face { body, face } => {
                    ctx.body(*body).map_err(at)?;
                    for feature in face.features() {
                        ctx.feature(feature).map_err(|e| {
                            at(crate::notice::about_place(
                                crate::notice::NoticePlace::face(face.to_string()),
                                &e.to_string(),
                            ))
                        })?;
                    }
                }
                LoftSectionDef::Point { point } => point.check(ctx, Want::Point).map_err(at)?,
                // Curves of a planar or a 3D sketch (mitcad#1550).
                LoftSectionDef::Curves { .. } => {
                    if let Some(curves) = section.sketch_curves() {
                        curves.check_any(ctx).map_err(at)?;
                    }
                }
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
                check_end_section(ctx, condition, section, which)?;
                imposed |= condition.imposes();
            }
        }
        if self.centerline.is_some() && !self.rails.is_empty() {
            return Err("a loft has a center line or rails, not both".to_owned());
        }
        if self.closed && (self.centerline.is_some() || !self.rails.is_empty()) {
            return Err("a closed loft has no center line or rails".to_owned());
        }
        if self.closed && imposed {
            return Err("a closed loft has no end conditions".to_owned());
        }
        if self.closed && !self.ruled && count < 3 {
            return Err("a smooth closed loft needs three or more sections".to_owned());
        }
        if self.ruled && (imposed || !self.rails.is_empty()) {
            return Err("a ruled loft has no end conditions or rails".to_owned());
        }
        // A 3D sketch's curves too, in their typed form (mitcad#1550).
        for path in self.centerline.iter().chain(&self.rails) {
            path.check(ctx)?;
        }
        // A frame along the centre line (mitcad#543).
        if let Some(frame) = &self.frame {
            if self.centerline.is_none() {
                return Err(
                    "a loft's frame turns its sections along a center line: it needs one"
                        .to_owned(),
                );
            }
            match frame {
                SweepFrame::Frenet => {}
                SweepFrame::Auxiliary { curve, .. } => {
                    curve
                        .check(ctx)
                        .map_err(|e| format!("auxiliary curve: {e}"))?;
                }
                SweepFrame::ConstantBinormal { binormal } => {
                    super::sweep::check_binormal(*binormal)?;
                }
            }
        }
        // The parallel orientation along the centre line (mitcad#547).
        if self.orientation == SweepOrientation::Parallel {
            if self.centerline.is_none() {
                return Err(
                    "a parallel loft moves its sections along a center line: it needs one"
                        .to_owned(),
                );
            }
            if self.frame.is_some() {
                return Err("a parallel loft along a center line takes no frame".to_owned());
            }
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
        if self.sections.iter().any(LoftSectionDef::is_curves) {
            return self.curve_loft(ctx);
        }
        let ends = self.ends(ctx)?;
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
                    // A section a sketch change split (mitcad#407) lofts its
                    // main piece, the one listed first in the warning.
                    if regions.len() > 1 {
                        let place = crate::notice::NoticePlace::region_of(
                            profile.sketch,
                            profile.region.to_string(),
                        );
                        ctx.warn_at(
                            at(format!(
                                "a loft section is one region; of the pieces of {region} the \
                                 first is used",
                                region = profile.region
                            )),
                            vec![place],
                        );
                    }
                    let region = regions.remove(0);
                    if region.loops.len() > 1 {
                        return Err(at(format!(
                            "{UNSUPPORTED}a loft section with holes ({region})",
                            region = region.key
                        )));
                    }
                    Found::Region(output.frame, region)
                }
                LoftSectionDef::Face { body, face } => {
                    let shape = ctx.surface_body(*body).map_err(at)?;
                    let face = ctx
                        .face_or(&shape, face, || {
                            crate::notice::placed(
                                "the body has no such face",
                                crate::notice::NoticePlace::face(face.to_string())
                                    .in_body(&body.to_string()),
                                format!("body {body} has no face {face}"),
                            )
                        })
                        .map_err(at)?;
                    Found::Face(shape, face)
                }
                LoftSectionDef::Point { point } => Found::Point(ctx.point(point).map_err(at)?),
                LoftSectionDef::Curves { .. } => {
                    return Err(at("a curve section in a loft of other sections".to_owned()));
                }
            });
        }
        let centerline: Option<Vec<PathCurve>> = match &self.centerline {
            Some(path) => Some(path.resolve(ctx).map_err(|e| format!("center line: {e}"))?),
            None => None,
        };
        let mut rails: Vec<Vec<PathCurve>> = Vec::new();
        for (i, path) in self.rails.iter().enumerate() {
            rails.push(
                path.resolve(ctx)
                    .map_err(|e| format!("rail {}: {e}", i + 1))?,
            );
        }
        // A frame along the centre line (mitcad#543).
        let auxiliary: Option<Vec<PathCurve>> = match &self.frame {
            Some(SweepFrame::Auxiliary { curve, .. }) => Some(
                curve
                    .resolve(ctx)
                    .map_err(|e| format!("auxiliary curve: {e}"))?,
            ),
            _ => None,
        };
        let centerline_frame = match &self.frame {
            None => None,
            Some(SweepFrame::Frenet) => Some(PathFrame::Frenet),
            Some(SweepFrame::Auxiliary { curvilinear, .. }) => Some(PathFrame::Auxiliary {
                curve: auxiliary.as_deref().unwrap_or(&[]),
                curvilinear: *curvilinear,
            }),
            Some(SweepFrame::ConstantBinormal { binormal }) => Some(PathFrame::ConstantBinormal(
                super::sweep::check_binormal(*binormal)?,
            )),
        };
        // The faces of Perpendicular ends (mitcad#2345).
        let faces = self.perpendicular_faces(ctx)?;
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
            solid: !self.surface,
            centerline: centerline.as_deref(),
            rails: rails.iter().map(Vec::as_slice).collect(),
            start: ends[0],
            end: ends[1],
            centerline_frame,
            centerline_parallel: self.orientation == SweepOrientation::Parallel,
            merge_tangent_edges: self.tangent_edges == TangentEdges::Merge,
            start_face: loft_face(&faces[0]),
            end_face: loft_face(&faces[1]),
        };
        let tool = ctx.kernel.loft(&spec).map_err(|e| e.to_string())?;
        if self.surface {
            // A sheet body: one body of the whole shell.
            let mut output = FeatureOutput {
                tool: Some(tool.clone()),
                ..FeatureOutput::default()
            };
            output.changes.push(BodyChange::Set(ctx.new_body(), tool));
            return Ok(output);
        }
        apply_operation(ctx, self.operation, participants, tool, "loft")
    }
}

/// A Perpendicular end's face as evaluation finds it (mitcad#2345): its
/// body's shape and its name now.
type FoundFace<S> = Option<(S, FaceName)>;

/// The kernel's form of a Perpendicular end's face (mitcad#2345).
fn loft_face<S>(found: &FoundFace<S>) -> Option<LoftFace<'_, S>> {
    found.as_ref().map(|(body, face)| LoftFace { body, face })
}

impl LoftDef {
    /// The kernel's ends: the conditions evaluated and checked, free (or
    /// sharp at a point) where none is given.
    fn ends<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<[LoftEnd; 2], String> {
        let mut ends = [LoftEnd::default(); 2];
        for (end, (condition, which)) in ends.iter_mut().zip([
            (&self.start_condition, "start"),
            (&self.end_condition, "end"),
        ]) {
            if let Some(condition) = condition {
                *end = condition.evaluate(ctx, which)?;
            }
        }
        Ok(ends)
    }

    /// The faces the Perpendicular ends leave at right angles (mitcad#2345),
    /// each with its body's shape and its name now: the condition's face,
    /// or at curves of a sketch on a curved face that face. `None` where
    /// the kernel takes the section's own face or plane.
    fn perpendicular_faces<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<[FoundFace<K::Shape>; 2], String> {
        let count = self.sections.len();
        let mut found: [FoundFace<K::Shape>; 2] = [None, None];
        for (slot, (condition, section, which)) in found.iter_mut().zip([
            (&self.start_condition, &self.sections[0], "start"),
            (&self.end_condition, &self.sections[count - 1], "end"),
        ]) {
            let Some(condition @ EndCondition::Perpendicular { .. }) = condition else {
                continue;
            };
            let at = |e: String| format!("the {which} condition's face: {e}");
            if let Some(face) = condition.face() {
                let shape = ctx.surface_body(face.body).map_err(at)?;
                let name = ctx
                    .face_or(&shape, &face.face, || {
                        crate::notice::placed(
                            "the body has no such face",
                            crate::notice::NoticePlace::face(face.face.to_string())
                                .in_body(&face.body.to_string()),
                            format!("body {} has no face {}", face.body, face.face),
                        )
                    })
                    .map_err(at)?;
                *slot = Some((shape, name));
            } else if let LoftSectionDef::Curves { sketch, .. } = section {
                // A sketch on a curved face (mitcad#1768): its curves lie
                // on that face.
                let developed = match ctx.env.feature(*sketch).map(|f| &f.def) {
                    Some(FeatureDef::Sketch(def)) => match &def.plane {
                        SketchPlane::Developed { face, body, .. } => {
                            Some(((**face).clone(), *body))
                        }
                        _ => None,
                    },
                    _ => None,
                };
                if let Some((face, body)) = developed {
                    let shapes = match body {
                        Some(body) => vec![(body, ctx.surface_body(body).map_err(at)?)],
                        None => ctx.surface_bodies(),
                    };
                    let (_, shape, renamed) =
                        super::sketch::face_body(ctx.kernel, &face, shapes).map_err(at)?;
                    *slot = Some((shape, renamed.unwrap_or(face)));
                }
            }
        }
        Ok(found)
    }

    /// A surface loft through sections of curves (mitcad#455): each
    /// section's curves one chain, lofted by the kernel's `loft_sheet`;
    /// with end conditions (mitcad#2345) by its `loft`, the chains as
    /// sections, a planar sketch's with its plane.
    fn curve_loft<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<FeatureOutput<K::Shape>, String> {
        let mut sections = Vec::new();
        let mut normals = Vec::new();
        for (i, section) in self.sections.iter().enumerate() {
            let at = |e: String| format!("section {}: {e}", i + 1);
            let Some(curves) = section.sketch_curves() else {
                return Err(at(
                    "a loft of curve sections has only curve sections".to_owned()
                ));
            };
            // A 3D sketch's curves where it has them (mitcad#1550).
            let (frame, pieces) = curves.curves(ctx).map_err(at)?;
            let mut chains = super::path::wall_chains(pieces, &frame).map_err(at)?;
            if chains.len() != 1 {
                return Err(at(format!(
                    "the curves make {} chains; a section is one chain",
                    chains.len()
                )));
            }
            sections.push(chains.remove(0));
            let lie = curves_lie(ctx.env.feature(curves.sketch).map(|f| &f.def));
            normals.push((lie == CurvesLie::InPlane).then(|| frame.normal()));
        }
        let conditioned = [&self.start_condition, &self.end_condition]
            .into_iter()
            .flatten()
            .any(|condition| condition.imposes());
        if !conditioned {
            let sheet = ctx
                .kernel
                .loft_sheet(
                    ctx.uid,
                    &sections,
                    self.ruled,
                    self.closed,
                    self.tangent_edges == TangentEdges::Merge,
                )
                .map_err(|e| e.to_string())?;
            return surface_sweeps::output(ctx, vec![sheet]);
        }
        let ends = self.ends(ctx)?;
        let faces = self.perpendicular_faces(ctx)?;
        let spec = LoftSpec {
            feature: ctx.uid,
            sections: sections
                .iter()
                .zip(&normals)
                .map(|(chain, normal)| LoftSection::Chain {
                    curves: chain,
                    normal: *normal,
                })
                .collect(),
            ruled: self.ruled,
            closed: self.closed,
            solid: false,
            centerline: None,
            rails: Vec::new(),
            start: ends[0],
            end: ends[1],
            centerline_frame: None,
            centerline_parallel: false,
            merge_tangent_edges: self.tangent_edges == TangentEdges::Merge,
            start_face: loft_face(&faces[0]),
            end_face: loft_face(&faces[1]),
        };
        let sheet = ctx.kernel.loft(&spec).map_err(|e| e.to_string())?;
        surface_sweeps::output(ctx, vec![sheet])
    }
}
