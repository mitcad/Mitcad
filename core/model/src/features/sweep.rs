// SPDX-License-Identifier: MIT
//! Sweep: profiles of a sketch swept along a path into a tool body that
//! becomes new bodies or joins, cuts or intersects bodies: perpendicular
//! or parallel orientation, twist and taper, a guide rail that turns and
//! scales the profile, guide surfaces whose normal turns it (mitcad#157),
//! partial extents and a fixed binormal (mitcad#397), an end scale and a
//! second guide rail (mitcad#464), rail points where the rails cross the
//! plane square to the path (mitcad#469).

use serde::{Deserialize, Serialize};

use super::extrude::{
    Operation, ProfileRef, apply_operation, check_participants, participant_bodies,
    profile_references, profile_regions,
};
use super::mapping::map_params;
use super::path::{self, CurvePath, SketchCurves};
use super::surface_sweeps;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse, is_false,
};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::sweeps::{
    PathCurve, ProfileScaling, RailPoint, SweepGuide, SweepOrientation, SweepSpec,
};
// Sweeps along guide surfaces (mitcad#157).
use super::face_refs::kernel_error;
use super::reference::FaceRef;
use crate::sweeps::GuideSurface;
use crate::topo::FaceName;
// Paths and guide surfaces on other components' bodies (mitcad#161).
use super::geom_ref::PathRef;
use crate::links::OccurrenceLink;
use std::collections::BTreeMap;
// Frenet and auxiliary frames (mitcad#539).
use crate::sweeps::PathFrame;
// Body faces as profiles (mitcad#543).
use crate::profile::SketchFrame;
use crate::sweeps::ProfileFace;
// Surface sweeps of fixed curves (mitcad#548).
use super::geom_ref::FixedCurve;

/// How much of a path a sweep or a pipe covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathExtent<P = ParamId> {
    /// All of it, both ways from the profile (once round a closed path).
    Full,
    /// `fraction` of the path's length from the profile towards its end
    /// and `fraction2` of the length from the profile towards its start
    /// (`distanceOne` and `distanceTwo` in .f3d designs). Left out, `fraction2` is the
    /// whole part before the profile, or none on a closed path.
    Partial {
        fraction: P,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        fraction2: Option<P>,
    },
}

impl<P> PathExtent<P> {
    pub fn full() -> Self {
        Self::Full
    }

    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full)
    }

    // Written by hand: an enum, its slots by kind (`extent.fraction`).
    pub(crate) fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<PathExtent<Q>, E> {
        Ok(match self {
            Self::Full => PathExtent::Full,
            Self::Partial {
                fraction,
                fraction2,
            } => PathExtent::Partial {
                fraction: f("extent.fraction", fraction)?,
                fraction2: fraction2
                    .as_ref()
                    .map(|v| f("extent.fraction2", v))
                    .transpose()?,
            },
        })
    }
}

impl PathExtent {
    /// The fractions of the path on each side of the profile.
    pub(crate) fn fractions<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        closed: bool,
    ) -> Result<(f64, f64), String> {
        let (one, two) = match self {
            Self::Full => (1.0, if closed { 0.0 } else { 1.0 }),
            Self::Partial {
                fraction,
                fraction2,
            } => (
                ctx.param(*fraction)?,
                match fraction2 {
                    Some(id) => ctx.param(*id)?,
                    None if closed => 0.0,
                    None => 1.0,
                },
            ),
        };
        check_fractions(one, two, closed)?;
        Ok((one, two))
    }

    pub(crate) fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if let Self::Partial {
            fraction,
            fraction2,
        } = self
        {
            for id in std::iter::once(fraction).chain(fraction2) {
                check_fraction(value(*id))?;
            }
        }
        Ok(())
    }
}

fn check_fraction(value: f64) -> Result<(), String> {
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "a fraction of the path must be between 0 and 1, got {value}"
        ))
    }
}

fn check_fractions(one: f64, two: f64, closed: bool) -> Result<(), String> {
    check_fraction(one)?;
    check_fraction(two)?;
    if one + two <= 0.0 {
        return Err("the sweep covers none of the path".to_owned());
    }
    if closed && one + two > 1.0 + 1e-9 {
        return Err(format!(
            "on a closed path the fractions add up to at most 1, got {}",
            one + two
        ));
    }
    Ok(())
}

/// Where a sweep's profile lies on its path (mitcad#795): the preview
/// reports it as `sweep_path`, and the Sweep panel shows Distance 2 (the
/// part before the profile) only for a profile past the start of an open
/// path.
///
/// A pipe's preview reports it too (mitcad#1448), its section at the
/// path's start, with `corners`: how many sharp corners the path has
/// (where its curves meet at an angle), so that the Pipe panel shows its
/// Corners choice only for such a path. A sweep leaves it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SweepPlace {
    pub closed: bool,
    pub profile_at: ProfileAt,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub corners: usize,
}

fn is_zero(count: &usize) -> bool {
    *count == 0
}

/// The place of the profile along the path as it is swept (after `flip`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileAt {
    Start,
    End,
    /// Between the ends, or anywhere on a closed path.
    Inside,
}

/// Where the profile of plane `frame` whose middle is `center` lies on
/// `path`: at an end of an open path when that end lies in the profile's
/// plane (the end nearer to `center` when both do), otherwise inside. The
/// geometry starts the sweep where the path crosses the profile's plane;
/// this tells the ends from the rest without the kernel.
pub(crate) fn profile_place(
    path: &[PathCurve],
    frame: &SketchFrame,
    center: [f64; 3],
) -> SweepPlace {
    let closed = path::is_closed(path);
    let ends = match (path.first(), path.last()) {
        (Some(first), Some(last)) => path::curve_ends(&first.curve)
            .zip(path::curve_ends(&last.curve))
            .map(|((start, _), (_, end))| (start, end)),
        _ => None,
    };
    let normal = mitcad_vec3::unit(frame.normal());
    let (Some((start, end)), Some(normal), false) = (ends, normal, closed) else {
        return SweepPlace {
            closed,
            profile_at: ProfileAt::Inside,
            corners: 0,
        };
    };
    let tolerance = mitcad_model_base::curves::tolerance(path);
    let on_plane = |p: [f64; 3]| {
        mitcad_vec3::dot(mitcad_vec3::sub(p, frame.origin), normal).abs() <= tolerance
    };
    let profile_at = match (on_plane(start), on_plane(end)) {
        (true, true)
            if mitcad_vec3::distance(end, center) < mitcad_vec3::distance(start, center) =>
        {
            ProfileAt::End
        }
        (true, _) => ProfileAt::Start,
        (false, true) => ProfileAt::End,
        (false, false) => ProfileAt::Inside,
    };
    SweepPlace {
        closed,
        profile_at,
        corners: 0,
    }
}

/// The middle of the profile regions' bounds, in model space.
fn regions_center(frame: &SketchFrame, regions: &[crate::profile::ProfileRegion]) -> [f64; 3] {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for region in regions {
        let (lo, hi) = region.bounds();
        min = [min[0].min(lo[0]), min[1].min(lo[1])];
        max = [max[0].max(hi[0]), max[1].max(hi[1])];
    }
    if !(min[0].is_finite() && max[0].is_finite() && min[1].is_finite() && max[1].is_finite()) {
        return frame.origin;
    }
    frame.point([(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0])
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweepDef<P = ParamId> {
    /// Regions of one sketch, a set (see `extrude::selected_regions`); none
    /// for a surface sweep of curves only.
    #[serde(
        default,
        deserialize_with = "super::extrude::profile_set",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub profiles: Vec<ProfileRef>,
    pub path: CurvePath,
    #[serde(default, skip_serializing_if = "is_perpendicular")]
    pub orientation: SweepOrientation,
    /// The profile's turn about the path over the swept length.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub twist_angle: Option<P>,
    /// Widens (positive) or narrows the profile along the path.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub taper_angle: Option<P>,
    /// A curve that turns the profile towards it and sizes it; twist and
    /// taper are then ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_rail: Option<CurvePath>,
    /// Faces the path lies on (mitcad#157): the profile turns with their
    /// normal; not with a guide rail, twist and taper are ignored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guide_surfaces: Vec<FaceRef>,
    #[serde(default, skip_serializing_if = "is_scale")]
    pub profile_scaling: ProfileScaling,
    #[serde(
        default = "PathExtent::full",
        skip_serializing_if = "PathExtent::is_full"
    )]
    pub extent: PathExtent<P>,
    /// Runs the path the other way (`isDirectionFlipped` in .f3d designs).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// Bodies of other components that the path's edges and the guide
    /// surfaces are on (mitcad#161), each with where it is seen (as a
    /// combine's `tool_links`): read in its component and moved into the
    /// sweep's coordinates by the placements at its point of the timeline.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub body_links: BTreeMap<BodyUid, OccurrenceLink>,
    /// A fixed binormal direction (mitcad#397): the profile keeps its
    /// angles to it along the path (a level profile along a sloped path
    /// stays level with the binormal up). Perpendicular orientation only,
    /// without a guide rail, guide surfaces, twist or taper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binormal: Option<[f64; 3]>,
    /// The profile's size at the ends of the swept part (mitcad#464): from
    /// 1 at the profile in proportion to the length run, about the path;
    /// unitless, 0.01 to 100. Not with a guide rail or guide surfaces.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub end_scale: Option<P>,
    /// A second guide rail beside `guide_rail` (mitcad#464): the profile
    /// moves with the first rail, turns with the line across to the second
    /// and is sized by its length (`profile_scaling`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_rail2: Option<CurvePath>,
    /// A surface sweep (mitcad#455, `surface_sweeps.rs`): walls without
    /// caps, a new surface body.
    #[serde(default, skip_serializing_if = "is_false")]
    pub surface: bool,
    /// Sketch curves a surface sweep sweeps beside or instead of the
    /// profiles: open chains or closed loops; not with twist, taper, guide
    /// rails or guide surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curves: Option<SketchCurves>,
    /// Where the rails' points are (mitcad#469): `plane`, where a rail
    /// crosses the plane square to the path, or `fraction` (left out: the
    /// designs saved before the option keep it), at the same fraction of
    /// the rail's length. Only with a guide rail.
    #[serde(default, skip_serializing_if = "is_fraction")]
    pub rail_point: RailPoint,
    /// How the profile turns along the path instead of keeping its angle to
    /// it (mitcad#539): the path's Frenet frame, or towards a curve beside
    /// the path. Left out, the default. Perpendicular orientation only,
    /// without twist, taper, end scale, guide rails, guide surfaces, a fixed
    /// binormal or curves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<SweepFrame>,
    /// A planar face of a body as the profile instead of sketch regions
    /// (mitcad#543, as an extrusion's `profile_face`): its bounded area,
    /// holes kept, swept along the path, found by its topological name on
    /// every recompute. Perpendicular or parallel, with a frame or a fixed
    /// binormal; no `profiles`, `curves`, `surface`, twist, taper, end
    /// scale, guide rails or guide surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_face: Option<FaceRef>,
    /// Fixed curves a surface sweep sweeps instead of a sketch's
    /// (mitcad#548): curves in model space that lie in no plane (an
    /// imported object's edges no sketch can hold), as a path's
    /// `fixed_curves`. Only a surface sweep, without profiles, sketch
    /// curves, a body face, twist, taper, guide rails, guide surfaces or a
    /// frame; it starts where the path meets the plane through the curves'
    /// first point square to the path's start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixed_curves: Vec<FixedCurve>,
}

/// A sweep's frame along its path (mitcad#539; [`PathFrame`] in the
/// kernel's terms).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SweepFrame {
    /// The path's Frenet frame: the profile turns with the path's
    /// curvature.
    Frenet,
    /// Towards a curve beside the path (an auxiliary spine): the profile
    /// turns about the path as the direction from the path to the curve
    /// does, the curve's point at the same fraction of its length as the
    /// path's (`curvilinear`) or where it crosses the plane square to the
    /// path. The curve runs the way the path runs (both reversed by
    /// `flip`).
    Auxiliary {
        curve: CurvePath,
        #[serde(default, skip_serializing_if = "is_false")]
        curvilinear: bool,
    },
    /// About a constant binormal (any length but zero): the profile turns
    /// only about this direction, its frame's normal `binormal × tangent`
    /// (the pipe shell's constant binormal; FreeCAD's Binormal mode).
    ConstantBinormal { binormal: [f64; 3] },
}

fn is_fraction(point: &RailPoint) -> bool {
    *point == RailPoint::Fraction
}

fn is_perpendicular(orientation: &SweepOrientation) -> bool {
    *orientation == SweepOrientation::Perpendicular
}

fn is_scale(scaling: &ProfileScaling) -> bool {
    *scaling == ProfileScaling::Scale
}

impl<P> SweepDef<P> {
    pub const TYPE: &'static str = "sweep";
    pub const BASE_NAME: &'static str = "Sweep";

    map_params! {
        SweepDef {
            profiles: clone,
            path: clone,
            orientation: copy,
            twist_angle: option,
            taper_angle: option,
            guide_rail: clone,
            guide_surfaces: clone,
            profile_scaling: copy,
            extent: map,
            flip: copy,
            operation: copy,
            participants: clone,
            body_links: clone,
            binormal: copy,
            end_scale: option,
            guide_rail2: clone,
            surface: copy,
            curves: clone,
            rail_point: copy,
            frame: clone,
            profile_face: clone,
            fixed_curves: clone,
        }
    }
}

impl SweepDef {
    /// The bodies a link may name: the path's and the guide surfaces'.
    fn linkable(&self, body: BodyUid) -> bool {
        matches!(&self.path, CurvePath::Path(PathRef::Edges { body: b, .. }) if *b == body)
            || self.guide_surfaces.iter().any(|f| f.body == body)
    }

    /// A body the path or a guide surface is on: of the sweep's component,
    /// or of another one moved into its coordinates by its link.
    fn input_body<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        uid: BodyUid,
    ) -> Result<K::Shape, String> {
        // A mesh body has no edges or surfaces to sweep along (mitcad#399).
        let Some(link) = self.body_links.get(&uid) else {
            return ctx.surface_body(uid);
        };
        let (component, transform) = ctx.linked(link).map_err(|e| format!("body {uid}: {e}"))?;
        let shape = ctx.body_in(component, uid)?;
        ctx.require_surfaces(uid, &shape)?;
        ctx.kernel
            .transform_shape(&shape, &transform, None)
            .map_err(|e| format!("body {uid}: {e}"))
    }
}

impl FeatureInfo for SweepDef {
    fn references(&self) -> References {
        let mut references = References::default();
        profile_references(&self.profiles, &mut references);
        if let Some(curves) = &self.curves {
            curves.add_references(&mut references);
        }
        self.path.add_references(&mut references);
        if let Some(rail) = &self.guide_rail {
            rail.add_references(&mut references);
        }
        if let Some(rail) = &self.guide_rail2 {
            rail.add_references(&mut references);
        }
        for face in &self.guide_surfaces {
            face.add_references(&mut references);
        }
        if let Some(SweepFrame::Auxiliary { curve, .. }) = &self.frame {
            curve.add_references(&mut references);
        }
        // A body face as the profile (mitcad#543).
        if let Some(face) = &self.profile_face {
            face.add_references(&mut references);
        }
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if !self.fixed_curves.is_empty() {
            // Fixed curves (mitcad#548).
            if !self.surface
                || !self.profiles.is_empty()
                || self.curves.is_some()
                || self.profile_face.is_some()
            {
                return Err(
                    "fixed curves are a surface sweep's alone: no profiles, sketch curves or body \
                     face"
                        .to_owned(),
                );
            }
            if !self.operation.makes_bodies() {
                return Err("a surface sweep is a new body or a new component".to_owned());
            }
            if self.guide_rail.is_some()
                || !self.guide_surfaces.is_empty()
                || self.twist_angle.is_some()
                || self.taper_angle.is_some()
                || self.frame.is_some()
            {
                return Err(
                    "a surface sweep of fixed curves takes no twist, taper, guide rail, guide \
                     surfaces or frame"
                        .to_owned(),
                );
            }
            for (i, curve) in self.fixed_curves.iter().enumerate() {
                curve
                    .check()
                    .map_err(|e| format!("fixed curve {}: {e}", i + 1))?;
            }
        } else if let Some(face) = &self.profile_face {
            // A body face as the profile (mitcad#543).
            if !self.profiles.is_empty() || self.curves.is_some() || self.surface {
                return Err(
                    "a sweep of a body face takes no sketch profiles, curves or surface".to_owned(),
                );
            }
            if self.guide_rail.is_some()
                || self.guide_rail2.is_some()
                || !self.guide_surfaces.is_empty()
            {
                return Err(
                    "a sweep of a body face is not taken with guide rails or guide surfaces"
                        .to_owned(),
                );
            }
            face.check(ctx)?;
        } else {
            surface_sweeps::check_inputs(
                ctx,
                self.surface,
                self.operation,
                &self.profiles,
                self.curves.as_ref(),
                "sweep",
            )?;
        }
        if self.curves.is_some()
            && (self.guide_rail.is_some()
                || !self.guide_surfaces.is_empty()
                || self.twist_angle.is_some()
                || self.taper_angle.is_some())
        {
            return Err(
                "a surface sweep of curves takes no twist, taper, guide rail or guide surfaces"
                    .to_owned(),
            );
        }
        check_participants(ctx, self.operation, &self.participants)?;
        self.path.check(ctx).map_err(|e| format!("path: {e}"))?;
        if let Some(rail) = &self.guide_rail {
            rail.check(ctx).map_err(|e| format!("guide rail: {e}"))?;
        }
        // A second rail and an end scale (mitcad#464).
        if let Some(rail) = &self.guide_rail2 {
            if self.guide_rail.is_none() {
                return Err("a second guide rail needs a first one".to_owned());
            }
            rail.check(ctx)
                .map_err(|e| format!("second guide rail: {e}"))?;
        }
        // The rail point rule (mitcad#469).
        if self.rail_point != RailPoint::Fraction && self.guide_rail.is_none() {
            return Err("a rail point is only taken with a guide rail".to_owned());
        }
        if self.end_scale.is_some()
            && (self.guide_rail.is_some() || !self.guide_surfaces.is_empty())
        {
            return Err(
                "an end scale is not taken with a guide rail or guide surfaces: they size the profile"
                    .to_owned(),
            );
        }
        if !self.guide_surfaces.is_empty() {
            if self.guide_rail.is_some() {
                return Err("a sweep takes a guide rail or guide surfaces, not both".to_owned());
            }
            if self.orientation == SweepOrientation::Parallel {
                return Err("guide surfaces need the perpendicular orientation".to_owned());
            }
            for (i, face) in self.guide_surfaces.iter().enumerate() {
                if self.guide_surfaces[..i].contains(face) {
                    return Err(crate::notice::placed(
                        "a guide surface is selected twice",
                        crate::notice::NoticePlace::face(face.face.to_string()),
                        format!("guide surface {} is selected twice", face.face),
                    ));
                }
                face.check(ctx).map_err(|e| format!("guide surface: {e}"))?;
            }
        }
        if let Some(binormal) = self.binormal {
            check_binormal(binormal)?;
            if self.orientation == SweepOrientation::Parallel {
                return Err("a fixed binormal needs the perpendicular orientation".to_owned());
            }
            if self.guide_rail.is_some() || !self.guide_surfaces.is_empty() {
                return Err(
                    "a fixed binormal is not taken with a guide rail or guide surfaces".to_owned(),
                );
            }
        }
        // A Frenet or auxiliary frame (mitcad#539).
        if let Some(frame) = &self.frame {
            if self.orientation == SweepOrientation::Parallel {
                return Err("a sweep's frame needs the perpendicular orientation".to_owned());
            }
            if self.guide_rail.is_some()
                || !self.guide_surfaces.is_empty()
                || self.binormal.is_some()
                || self.curves.is_some()
            {
                return Err(
                    "a sweep's frame is not taken with a guide rail, guide surfaces, a fixed binormal or curves"
                        .to_owned(),
                );
            }
            if let SweepFrame::Auxiliary { curve, .. } = frame {
                curve
                    .check(ctx)
                    .map_err(|e| format!("auxiliary curve: {e}"))?;
            }
            if let SweepFrame::ConstantBinormal { binormal } = frame {
                check_binormal(*binormal)?;
            }
        }
        if let Some(body) = self.body_links.keys().find(|b| !self.linkable(**b)) {
            return Err(format!(
                "body {body} is linked but neither the path's nor a guide surface's"
            ));
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        self.extent.check_values(value)?;
        // A body face's sweep (mitcad#543) neither twists nor scales.
        if self.profile_face.is_some()
            && (self
                .twist_angle
                .into_iter()
                .chain(self.taper_angle)
                .any(|v| value(v) != 0.0)
                || self.end_scale.is_some_and(|v| value(v) != 1.0))
        {
            return Err(
                "a sweep of a body face is not taken with a twist, a taper or an end scale"
                    .to_owned(),
            );
        }
        if let Some(taper) = self.taper_angle {
            check_taper(value(taper))?;
        }
        if let Some(twist) = self.twist_angle
            && !value(twist).is_finite()
        {
            return Err("the twist angle must be a number".to_owned());
        }
        if self.binormal.is_some()
            && self
                .twist_angle
                .into_iter()
                .chain(self.taper_angle)
                .any(|v| value(v) != 0.0)
        {
            return Err("a fixed binormal is not taken with a twist or a taper".to_owned());
        }
        if let Some(scale) = self.end_scale {
            check_end_scale(value(scale))?;
            if self.binormal.is_some() && value(scale) != 1.0 {
                return Err("a fixed binormal is not taken with an end scale".to_owned());
            }
        }
        // A Frenet or auxiliary frame decides the profile's turn alone
        // (mitcad#539).
        if self.frame.is_some()
            && (self
                .twist_angle
                .into_iter()
                .chain(self.taper_angle)
                .any(|v| value(v) != 0.0)
                || self.end_scale.is_some_and(|v| value(v) != 1.0))
        {
            return Err(
                "a sweep's frame is not taken with a twist, a taper or an end scale".to_owned(),
            );
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }

    fn profiles(&self) -> &[ProfileRef] {
        &self.profiles
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

/// A fixed binormal (mitcad#397): finite, of some length; its unit vector.
pub(crate) fn check_binormal(binormal: [f64; 3]) -> Result<[f64; 3], String> {
    let length = binormal.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !length.is_finite() || length < 1e-12 {
        return Err(format!(
            "the binormal must be a direction of some length, got {binormal:?}"
        ));
    }
    Ok(binormal.map(|v| v / length))
}

/// A sweep's end scale (mitcad#464): 0.01 to 100.
fn check_end_scale(scale: f64) -> Result<(), String> {
    if (0.01..=100.0).contains(&scale) {
        Ok(())
    } else {
        Err(format!(
            "the sweep's end scale must be between 0.01 and 100, got {scale}"
        ))
    }
}

fn check_taper(taper: f64) -> Result<(), String> {
    if taper.is_finite() && taper.abs() < std::f64::consts::FRAC_PI_2 {
        Ok(())
    } else {
        Err(format!(
            "the taper angle must be less than 90 degrees either way, got {taper} rad"
        ))
    }
}

impl<K: Kernel> Evaluate<K> for SweepDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        // A body face as the profile (mitcad#543): found on its body now;
        // the kernel takes the face's plane for the profile's.
        let face_profile: Option<(K::Shape, FaceName)> = match &self.profile_face {
            Some(face) => {
                let body = ctx.surface_body(face.body)?;
                let found = ctx.face_on(face.body, &body, &face.face)?;
                Some((body, found))
            }
            None => None,
        };
        let fixed = !self.fixed_curves.is_empty();
        let (mut frame, regions) = if face_profile.is_some() || fixed {
            (SketchFrame::XY, Vec::new())
        } else {
            let (sketch, regions) = match (&self.curves, self.profiles.is_empty()) {
                (Some(curves), true) => (ctx.sketch(curves.sketch)?, Vec::new()),
                _ => profile_regions(ctx, &self.profiles)?,
            };
            (sketch.frame, regions)
        };
        // A surface sweep's curves (mitcad#455).
        let mut chains = match (&self.curves, self.surface) {
            (Some(curves), true) => Some(surface_sweeps::chains(ctx, curves, &frame)?),
            _ => None,
        };
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let mut path = match &self.path {
            CurvePath::Path(PathRef::Edges { body, edges })
                if self.body_links.contains_key(body) =>
            {
                let shape = self.input_body(ctx, *body)?;
                path::resolve_edges_on(ctx, *body, &shape, edges)
            }
            path => path.resolve(ctx),
        }
        .map_err(|e| format!("path: {e}"))?;
        if self.flip {
            path = path::reversed_path(path);
        }
        // Where the profile lies on the path, for the panel (mitcad#795).
        if face_profile.is_none() && !fixed && !regions.is_empty() {
            let center = regions_center(&frame, &regions);
            ctx.sweep_place = Some(profile_place(&path, &frame, center));
        }
        // Fixed curves (mitcad#548): their chains, from the plane square to
        // the path at its start through their first point.
        if fixed {
            let pieces: Vec<PathCurve> = self
                .fixed_curves
                .iter()
                .enumerate()
                .map(|(i, c)| PathCurve {
                    name: format!("fixed{}", i + 1),
                    curve: c.curve(),
                })
                .collect();
            frame = path::plane_at_start(&path, &pieces)?;
            chains = Some(path::wall_chains(pieces, &frame)?);
        }
        let (extent1, extent2) = self.extent.fractions(ctx, path::is_closed(&path))?;
        let rail: Option<Vec<PathCurve>> = match &self.guide_rail {
            Some(rail) => Some(rail.resolve(ctx).map_err(|e| format!("guide rail: {e}"))?),
            None => None,
        };
        let rail2: Option<Vec<PathCurve>> = match &self.guide_rail2 {
            Some(rail) => Some(
                rail.resolve(ctx)
                    .map_err(|e| format!("second guide rail: {e}"))?,
            ),
            None => None,
        };
        // An auxiliary curve (mitcad#539), run the way the path runs.
        let auxiliary: Option<Vec<PathCurve>> = match &self.frame {
            Some(SweepFrame::Auxiliary { curve, .. }) => {
                let curve = curve
                    .resolve(ctx)
                    .map_err(|e| format!("auxiliary curve: {e}"))?;
                Some(if self.flip {
                    path::reversed_path(curve)
                } else {
                    curve
                })
            }
            _ => None,
        };
        // The guide surfaces' bodies, each face checked.
        let mut surfaces: Vec<(K::Shape, FaceName)> = Vec::new();
        for face in &self.guide_surfaces {
            let shape = self.input_body(ctx, face.body)?;
            let found = ctx.face_on(face.body, &shape, &face.face)?;
            surfaces.push((shape, found));
        }
        // A guide rail or guide surfaces override twist and taper.
        let (mut twist, mut taper, mut end_scale) = (0.0, 0.0, 1.0);
        if rail.is_none() && surfaces.is_empty() {
            if let Some(id) = self.twist_angle {
                twist = ctx.param(id)?;
            }
            if let Some(id) = self.taper_angle {
                taper = ctx.param(id)?;
                check_taper(taper)?;
            }
            if let Some(id) = self.end_scale {
                end_scale = ctx.param(id)?;
                check_end_scale(end_scale)?;
            }
        }
        let spec = SweepSpec {
            feature: ctx.uid,
            frame,
            regions: &regions,
            path: &path,
            extent1,
            extent2,
            orientation: self.orientation,
            twist,
            taper,
            guide: rail.as_deref().map(|rail| SweepGuide {
                rail,
                scaling: self.profile_scaling,
                rail2: rail2.as_deref(),
                point: self.rail_point,
            }),
            binormal: self.binormal.map(check_binormal).transpose()?,
            end_scale,
            path_frame: match &self.frame {
                None => None,
                Some(SweepFrame::Frenet) => Some(PathFrame::Frenet),
                Some(SweepFrame::Auxiliary { curvilinear, .. }) => Some(PathFrame::Auxiliary {
                    curve: auxiliary.as_deref().unwrap_or(&[]),
                    curvilinear: *curvilinear,
                }),
                Some(SweepFrame::ConstantBinormal { binormal }) => {
                    Some(PathFrame::ConstantBinormal(check_binormal(*binormal)?))
                }
            },
        };
        let solid = |ctx: &mut EvalContext<'_, K>, spec: &SweepSpec<'_>| {
            if let (Some((body, face)), Some(stored)) = (&face_profile, &self.profile_face) {
                let profile = ProfileFace {
                    body,
                    face,
                    key: &stored.face,
                };
                ctx.kernel.face_sweep(spec, &profile).map_err(kernel_error)
            } else if surfaces.is_empty() {
                let tool = ctx.kernel.sweep(spec).map_err(|e| e.to_string())?;
                // (mitcad#1697) A profile that passes through itself along
                // the path is built in pieces fused into one solid: a note.
                if ctx
                    .kernel
                    .infos(&tool)
                    .iter()
                    .any(|info| info == crate::sweeps::FOLDS_RESOLVED)
                {
                    ctx.inform(crate::sweeps::FOLDS_RESOLVED);
                }
                Ok(tool)
            } else {
                let guide: Vec<GuideSurface<'_, K::Shape>> = surfaces
                    .iter()
                    .map(|(body, face)| GuideSurface { body, face })
                    .collect();
                ctx.kernel.surface_sweep(spec, &guide).map_err(kernel_error)
            }
        };
        if self.surface {
            // The profiles' walls (the solid sweep without its caps), then
            // the curves' (mitcad#455).
            let mut sheets = Vec::new();
            if !regions.is_empty() {
                let tool = solid(ctx, &spec)?;
                sheets.push(surface_sweeps::walls(ctx, &tool)?);
            }
            if let Some(chains) = &chains {
                let spec = SweepSpec {
                    regions: &[],
                    ..spec
                };
                sheets.push(
                    ctx.kernel
                        .sweep_sheet(&spec, chains)
                        .map_err(|e| e.to_string())?,
                );
            }
            return surface_sweeps::output(ctx, sheets);
        }
        let tool = solid(ctx, &spec)?;
        apply_operation(ctx, self.operation, participants, tool, "sweep")
    }
}
