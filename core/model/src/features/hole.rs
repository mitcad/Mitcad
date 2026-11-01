// SPDX-License-Identifier: MIT
//! Hole: drilled holes cut into the participant bodies: simple, counterbore,
//! countersink or counterdrill, straight or tapered, with a drill point or
//! a flat bottom, to a depth, through all or up to an object, at points on
//! a planar face or at sketch points, optionally tapped with a thread whose
//! size sets the bore (cosmetic, or modelled). A taper pipe thread (NPT,
//! ISO 7-1 `Rc`; mitcad#733) with `taper` is drilled at the hole's
//! diameter (the tap drill) and tapers the bore from the hole's start,
//! 1:16 on the diameter from the thread's minor diameter at its opening,
//! cosmetic or modelled; without `taper` (holes saved before) its bore is
//! the size's minor diameter, as it was.
//!
//! Mitcad's definitions (experiments K24-K28 test them on .f3d designs):
//! the depth runs to the shoulder and the drill point is extra; a
//! counterbore's depth and the hole's depth are both measured from the
//! start; the countersink angle is the full angle; a
//! tapped hole's bore is the thread's basic minor diameter D1; the holes go
//! against the face or sketch normal, into the material, and `flip` turns
//! them round.

use serde::{Deserialize, Serialize};

use super::extrude::{Operation, apply_operation, check_participants, participant_bodies};
use super::geom_ref::{GeomRef, Want};
use super::hole_sizes::ClearanceFit;
use super::place_at::PlaceAt;
use super::reference::{ExtentObject, normalized};
use super::thread::{
    CustomSize, ThreadSize, check_clearance, check_custom, check_starts, clearance_value,
    fitted_clearance, flank_from_fit, size_of, starts_value, thread_clearance, thread_part,
    thread_starts,
};
use super::thread_table::ThreadStandard;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    ToolUse, is_false,
};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::{
    Axis, Bound, Cylinder, HoleEnd, HoleShape, HoleSpec, Kernel, ThreadSpec, ThreadTaper,
};
use crate::parameters::ParamId;
use crate::sketch::geometry::Curve2;
use crate::topo::{EdgeName, FaceName};
use mitcad_model_base::internal::ids::curve_serde;
use mitcad_vec3::{cross, dot};

/// The default drill point, 118 degrees.
pub const DEFAULT_TIP_ANGLE: f64 = 118.0 * std::f64::consts::PI / 180.0;

/// A wall of a tapped hole whose modelled thread has its crest off the wall
/// (a radial clearance), as the wall was before the thread was cut into it
/// (mitcad#436). Such a thread leaves no piece of the wall, or only pieces
/// outside its part (mitcad#628). The hole makes the wall and threads it in
/// one step, so no body before the hole has it; the threads query measures
/// the thread on this.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadedWall {
    pub body: BodyUid,
    pub wall: FaceName,
    pub cylinder: Cylinder,
}

/// Where the holes are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HolePlacement<P = ParamId> {
    /// Points on a planar face, each put on its plane
    /// (`setPositionByPoint` in .f3d designs): one hole per point. On a
    /// cylindrical face each point is put on the cylinder and the hole goes
    /// along its radius (mitcad#719).
    Face {
        body: BodyUid,
        face: FaceName,
        points: Vec<[f64; 3]>,
    },
    /// One hole on a planar face, `offset1` from the line of `edge1` and
    /// `offset2` from the line of `edge2`, on the face's side of both
    /// (`setPositionByPlaneAndOffsets` in .f3d designs).
    FaceOffsets {
        body: BodyUid,
        face: FaceName,
        edge1: Box<EdgeName>,
        offset1: P,
        edge2: Box<EdgeName>,
        offset2: P,
    },
    /// Points of a sketch, against its normal
    /// (`setPositionBySketchPoints` in .f3d designs).
    SketchPoints {
        sketch: FeatureUid,
        points: Vec<SketchPoint>,
    },
    /// Points projected onto a plane (an origin plane, a construction
    /// plane or a planar face: `setPositionByPoint` on a plane in .f3d
    /// designs); the holes start on it, against its normal, in the
    /// participants that reach it.
    Plane {
        plane: GeomRef,
        points: Vec<[f64; 3]>,
    },
    /// One hole placed by picked geometry (mitcad#656, `place_at.rs`):
    /// along the axis into a shaft or hole, against the normal into a face.
    At { at: Box<PlaceAt<P>> },
    /// One hole on a cylindrical face (a shaft's side) whose axis point
    /// lies `offset` from `plane` (a planar face or a plane the axis
    /// crosses), measured along the plane's normal, on the face's side of
    /// the plane; on the side of the axis where `toward` lies; along the
    /// radius into the material (`setPositionByPlaneAndOffsets` on a
    /// cylindrical face in .f3d designs, mitcad#719). Across a plane square
    /// to the axis the offset is the distance along the axis.
    CylinderOffset {
        body: BodyUid,
        face: FaceName,
        plane: GeomRef,
        offset: P,
        toward: [f64; 3],
    },
}

/// A point of a sketch: coordinates in it, the centre of a circle, arc or
/// ellipse of it (`"c5"`), or a sketch point entity (`"p3"`, as
/// `setPositionBySketchPoints` in .f3d designs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SketchPoint {
    At([f64; 2]),
    Center(#[serde(with = "curve_serde")] EntityUid),
    Point(#[serde(with = "mitcad_sketch::internal::point_serde")] EntityUid),
}

/// The hole types; the countersink angle is the cone's full angle. A
/// counterdrill is a counterbore whose floor is a cone of the full `angle`
/// narrowing to the hole (FreeCAD's counterdrilled holes, mitcad#4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HoleKind<P = ParamId> {
    Simple,
    Counterbore { diameter: P, depth: P },
    Countersink { diameter: P, angle: P },
    Counterdrill { diameter: P, depth: P, angle: P },
}

// Not derived: that would need P: Default.
#[allow(clippy::derivable_impls)]
impl<P> Default for HoleKind<P> {
    fn default() -> Self {
        Self::Simple
    }
}

impl<P> HoleKind<P> {
    pub fn is_simple(&self) -> bool {
        matches!(self, Self::Simple)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HoleExtent<P = ParamId> {
    /// The depth of the cylindrical part from the start; the drill point
    /// is beyond it.
    Distance { depth: P },
    /// Through every participant body, with a flat end.
    ThroughAll,
    /// The cylindrical part ends where each hole's axis first meets the
    /// object (moved by `offset`); the drill point is beyond it.
    ToObject {
        object: Box<ExtentObject>,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        offset: Option<P>,
    },
}

/// A tapped hole's thread (`setToTappedHole` in .f3d designs). It sets the bore to
/// the size's basic minor diameter; a modelled one is cut into the wall. A
/// taper pipe thread with `taper` keeps the hole's diameter (the tap drill)
/// and tapers the bore from the hole's start (mitcad#733).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoleThread<P = ParamId> {
    #[serde(default, skip_serializing_if = "is_iso")]
    pub standard: ThreadStandard,
    /// None for a custom thread (mitcad#893).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub designation: String,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub right_handed: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub modeled: bool,
    /// `length` from `offset` below the hole's start; the whole wall
    /// without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub length: Option<P>,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// A modelled thread's extra clearance, radial and perpendicular to
    /// each flank (as the thread feature's); 0 without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub radial_clearance: Option<P>,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub flank_clearance: Option<P>,
    /// A taper pipe thread (`npt`, `iso_taper`) that tapers the bore from
    /// the hole's start, the hole drilled at its `diameter`, the tap drill
    /// (mitcad#733). Without it (holes saved before) the bore is the size's
    /// minor diameter and nothing tapers.
    #[serde(default, skip_serializing_if = "is_false")]
    pub taper: bool,
    /// A custom thread's size (`standard` `custom`, mitcad#893): its major
    /// diameter and pitch, as the thread feature's; the bore is its minor
    /// diameter.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub custom: Option<CustomSize<P>>,
    /// The number of starts of a modelled thread (mitcad#893), as the
    /// thread feature's; 1 without (or the designation's).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub starts: Option<P>,
}

/// A clearance hole's screw size and fit (mitcad#903): `{"size": "M6",
/// "fit": "normal"}`. It names where the hole's values came from (the
/// `hole_sizes` table: ISO 273's clearance hole, the head's counterbore or
/// countersink) so that the Hole panel shows them again; the hole itself
/// is cut at its own `diameter` and `kind`, plain values as any hole's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoleClearance {
    pub size: String,
    #[serde(default)]
    pub fit: ClearanceFit,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_iso(standard: &ThreadStandard) -> bool {
    *standard == ThreadStandard::IsoMetric
}

impl<P> HoleThread<P> {
    /// Whether it is a taper pipe thread that tapers the bore (mitcad#733).
    pub fn tapers(&self) -> bool {
        self.taper && self.standard.is_tapered()
    }

    pub fn size(&self) -> ThreadSize {
        ThreadSize {
            standard: self.standard,
            designation: self.designation.clone(),
            class: self.class.clone(),
            right_handed: self.right_handed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoleDef<P = ParamId> {
    pub placement: HolePlacement<P>,
    /// The hole's diameter; a tapped hole's thread sets it instead (but a
    /// taper pipe thread's, mitcad#733: the tap drill).
    pub diameter: P,
    #[serde(
        default = "Default::default",
        skip_serializing_if = "HoleKind::is_simple"
    )]
    pub kind: HoleKind<P>,
    /// The drill point's full angle, 118 degrees when left out.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub tip_angle: Option<P>,
    /// A flat bottom instead of a drill point (Drill Point Flat).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flat: bool,
    pub extent: HoleExtent<P>,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub thread: Option<HoleThread<P>>,
    /// Turns the holes round, out of the face (`isDefaultDirection = false`
    /// in .f3d designs).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// The bodies to cut; empty for all bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// A tapered hole: the wall's angle to the axis, positive narrowing the
    /// hole as it goes deeper from its diameter at the start (FreeCAD's
    /// tapered holes: 90 degrees less their taper angle, mitcad#4).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub taper: Option<P>,
    /// A clearance hole's screw size and fit (mitcad#903); not with a
    /// `thread`. Holes saved before have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance: Option<HoleClearance>,
}

type Mapper<'a, P, Q, E> = dyn FnMut(&str, &P) -> Result<Q, E> + 'a;

fn map_option<P, Q, E>(
    slot: &str,
    value: &Option<P>,
    f: &mut Mapper<'_, P, Q, E>,
) -> Result<Option<Q>, E> {
    value.as_ref().map(|v| f(slot, v)).transpose()
}

impl<P> HoleDef<P> {
    pub const TYPE: &'static str = "hole";
    pub const BASE_NAME: &'static str = "Hole";

    // Written by hand: placement, kind, extent and thread map by their kind.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<HoleDef<Q>, E> {
        let placement = match &self.placement {
            HolePlacement::Face { body, face, points } => HolePlacement::Face {
                body: *body,
                face: face.clone(),
                points: points.clone(),
            },
            HolePlacement::FaceOffsets {
                body,
                face,
                edge1,
                offset1,
                edge2,
                offset2,
            } => HolePlacement::FaceOffsets {
                body: *body,
                face: face.clone(),
                edge1: edge1.clone(),
                offset1: f("placement.offset1", offset1)?,
                edge2: edge2.clone(),
                offset2: f("placement.offset2", offset2)?,
            },
            HolePlacement::SketchPoints { sketch, points } => HolePlacement::SketchPoints {
                sketch: *sketch,
                points: points.clone(),
            },
            HolePlacement::Plane { plane, points } => HolePlacement::Plane {
                plane: plane.clone(),
                points: points.clone(),
            },
            HolePlacement::At { at } => HolePlacement::At {
                at: Box::new(at.map_params("placement.at.", &mut *f)?),
            },
            HolePlacement::CylinderOffset {
                body,
                face,
                plane,
                offset,
                toward,
            } => HolePlacement::CylinderOffset {
                body: *body,
                face: face.clone(),
                plane: plane.clone(),
                offset: f("placement.offset", offset)?,
                toward: *toward,
            },
        };
        let diameter = f("diameter", &self.diameter)?;
        let kind = match &self.kind {
            HoleKind::Simple => HoleKind::Simple,
            HoleKind::Counterbore { diameter, depth } => HoleKind::Counterbore {
                diameter: f("kind.diameter", diameter)?,
                depth: f("kind.depth", depth)?,
            },
            HoleKind::Countersink { diameter, angle } => HoleKind::Countersink {
                diameter: f("kind.diameter", diameter)?,
                angle: f("kind.angle", angle)?,
            },
            HoleKind::Counterdrill {
                diameter,
                depth,
                angle,
            } => HoleKind::Counterdrill {
                diameter: f("kind.diameter", diameter)?,
                depth: f("kind.depth", depth)?,
                angle: f("kind.angle", angle)?,
            },
        };
        let tip_angle = map_option("tip_angle", &self.tip_angle, f)?;
        let extent = match &self.extent {
            HoleExtent::Distance { depth } => HoleExtent::Distance {
                depth: f("extent.depth", depth)?,
            },
            HoleExtent::ThroughAll => HoleExtent::ThroughAll,
            HoleExtent::ToObject { object, offset } => HoleExtent::ToObject {
                object: object.clone(),
                offset: map_option("extent.offset", offset, f)?,
            },
        };
        let thread = match &self.thread {
            None => None,
            Some(thread) => Some(HoleThread {
                standard: thread.standard,
                designation: thread.designation.clone(),
                class: thread.class.clone(),
                right_handed: thread.right_handed,
                modeled: thread.modeled,
                length: map_option("thread.length", &thread.length, f)?,
                offset: map_option("thread.offset", &thread.offset, f)?,
                radial_clearance: map_option(
                    "thread.radial_clearance",
                    &thread.radial_clearance,
                    f,
                )?,
                flank_clearance: map_option("thread.flank_clearance", &thread.flank_clearance, f)?,
                taper: thread.taper,
                custom: thread
                    .custom
                    .as_ref()
                    .map(|c| c.map_params("thread.custom.", &mut *f))
                    .transpose()?,
                starts: map_option("thread.starts", &thread.starts, f)?,
            }),
        };
        let taper = map_option("taper", &self.taper, f)?;
        Ok(HoleDef {
            placement,
            diameter,
            kind,
            tip_angle,
            flat: self.flat,
            extent,
            thread,
            flip: self.flip,
            participants: self.participants.clone(),
            taper,
            clearance: self.clearance.clone(),
        })
    }

    /// The number of holes, when it does not depend on evaluation.
    pub fn count(&self) -> usize {
        match &self.placement {
            HolePlacement::Face { points, .. } => points.len(),
            HolePlacement::FaceOffsets { .. } => 1,
            HolePlacement::SketchPoints { points, .. } => points.len(),
            HolePlacement::Plane { points, .. } => points.len(),
            HolePlacement::At { .. } | HolePlacement::CylinderOffset { .. } => 1,
        }
    }
}

/// A hole's taper: less than a right angle either way.
fn check_taper(taper: f64) -> Result<(), String> {
    if taper.is_finite() && taper.abs() < std::f64::consts::FRAC_PI_2 {
        Ok(())
    } else {
        Err(format!(
            "the hole's taper must be less than 90 degrees either way, got {taper} rad"
        ))
    }
}

/// The name of hole `index`'s part made by `feature`: `F5:hole0.wall`.
pub fn hole_face(feature: FeatureUid, index: usize, part: &str) -> FaceName {
    FaceName::new(feature, &format!("hole{index}.{part}"), None)
}

impl FeatureInfo for HoleDef {
    fn references(&self) -> References {
        let mut references = References::default();
        match &self.placement {
            HolePlacement::Face { body, face, .. } => {
                references.body(*body);
                references.features.extend(face.features());
            }
            HolePlacement::FaceOffsets {
                body,
                face,
                edge1,
                edge2,
                ..
            } => {
                references.body(*body);
                references.features.extend(face.features());
                references.edges([&**edge1, &**edge2]);
            }
            HolePlacement::SketchPoints { sketch, .. } => {
                references.features.insert(*sketch);
            }
            HolePlacement::Plane { plane, .. } => plane.add_to(&mut references),
            HolePlacement::At { at } => at.add_to(&mut references),
            HolePlacement::CylinderOffset {
                body, face, plane, ..
            } => {
                references.body(*body);
                references.features.extend(face.features());
                plane.add_to(&mut references);
            }
        }
        if let HoleExtent::ToObject { object, .. } = &self.extent {
            object.add_references(&mut references);
        }
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match &self.placement {
            HolePlacement::Face { body, face, points } => {
                ctx.body(*body)?;
                for feature in face.features() {
                    ctx.feature(feature)
                        .map_err(|e| format!("face {face}: {e}"))?;
                }
                if points.is_empty() {
                    return Err("the hole has no position".to_owned());
                }
                if points.iter().flatten().any(|v| !v.is_finite()) {
                    return Err("the hole positions must be finite points".to_owned());
                }
            }
            HolePlacement::FaceOffsets {
                body,
                face,
                edge1,
                edge2,
                ..
            } => {
                ctx.body(*body)?;
                for feature in face.features() {
                    ctx.feature(feature)
                        .map_err(|e| format!("face {face}: {e}"))?;
                }
                ctx.edges(&[(**edge1).clone(), (**edge2).clone()])?;
            }
            HolePlacement::Plane { plane, points } => {
                plane.check(ctx, Want::Plane)?;
                if points.is_empty() {
                    return Err("the hole has no position".to_owned());
                }
                if points.iter().flatten().any(|v| !v.is_finite()) {
                    return Err("the hole positions must be finite points".to_owned());
                }
            }
            HolePlacement::At { at } => at.check(ctx)?,
            HolePlacement::CylinderOffset {
                body,
                face,
                plane,
                toward,
                ..
            } => {
                ctx.body(*body)?;
                for feature in face.features() {
                    ctx.feature(feature)
                        .map_err(|e| format!("face {face}: {e}"))?;
                }
                plane.check(ctx, Want::Plane)?;
                if !toward.iter().all(|v| v.is_finite()) {
                    return Err("the hole's side must be a finite point".to_owned());
                }
            }
            HolePlacement::SketchPoints { sketch, points } => {
                ctx.sketch(*sketch)?;
                if points.is_empty() {
                    return Err("the hole has no position".to_owned());
                }
                for point in points {
                    if let SketchPoint::At(p) = point
                        && !p.iter().all(|v| v.is_finite())
                    {
                        return Err("the hole positions must be finite points".to_owned());
                    }
                }
            }
        }
        if let HoleExtent::ToObject { object, .. } = &self.extent {
            object.check(ctx)?;
        }
        if let Some(thread) = &self.thread {
            if thread.offset.is_some() && thread.length.is_none() {
                return Err("a thread over the whole wall has no offset".to_owned());
            }
            if thread.taper && !thread.standard.is_tapered() {
                return Err(
                    "only a taper pipe thread (NPT, ISO 7-1) is tapered: leave out `taper`"
                        .to_owned(),
                );
            }
            if thread.modeled {
                super::thread::check_modeled(thread.standard, thread.taper)?;
            } else if thread.radial_clearance.is_some() || thread.flank_clearance.is_some() {
                return Err("only a modeled thread has extra clearance".to_owned());
            }
            check_custom(&thread.size(), thread.custom.is_some())?;
            if thread.custom.is_none() {
                thread.size().check(Some(true))?;
            }
        }
        // A clearance hole's size is one of the table's (mitcad#903).
        if let Some(clearance) = &self.clearance {
            if self.thread.is_some() {
                return Err(
                    "a clearance hole has no thread: leave out `clearance` or `thread`".to_owned(),
                );
            }
            super::hole_sizes::lookup(&clearance.size)?;
        }
        check_participants(ctx, Operation::Cut, &self.participants)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let positive = |what: &str, id: ParamId| {
            let v = value(id);
            if v > 0.0 {
                Ok(v)
            } else {
                Err(format!("the {what} must be greater than zero, got {v}"))
            }
        };
        let angle = |what: &str, id: ParamId| {
            let v = value(id);
            if v > 0.0 && v < std::f64::consts::PI {
                Ok(())
            } else {
                Err(format!(
                    "the {what} must be between 0 and 180 degrees, got {v} rad"
                ))
            }
        };
        // (A tapering taper pipe thread keeps the hole's diameter,
        // mitcad#733.)
        let diameter = if self.thread.as_ref().is_some_and(|t| !t.tapers()) {
            None
        } else {
            Some(positive("hole diameter", self.diameter)?)
        };
        match &self.kind {
            HoleKind::Simple => {}
            HoleKind::Counterbore { diameter: d, depth } => {
                let d = positive("counterbore diameter", *d)?;
                positive("counterbore depth", *depth)?;
                if diameter.is_some_and(|hole| d <= hole) {
                    return Err("the counterbore must be wider than the hole".to_owned());
                }
            }
            HoleKind::Countersink {
                diameter: d,
                angle: a,
            } => {
                let d = positive("countersink diameter", *d)?;
                angle("countersink angle", *a)?;
                if diameter.is_some_and(|hole| d <= hole) {
                    return Err("the countersink must be wider than the hole".to_owned());
                }
            }
            HoleKind::Counterdrill {
                diameter: d,
                depth,
                angle: a,
            } => {
                let d = positive("counterdrill diameter", *d)?;
                positive("counterdrill depth", *depth)?;
                angle("counterdrill angle", *a)?;
                if diameter.is_some_and(|hole| d <= hole) {
                    return Err("the counterdrill must be wider than the hole".to_owned());
                }
            }
        }
        if let Some(tip) = self.tip_angle {
            angle("drill point angle", tip)?;
        }
        if let Some(id) = self.taper {
            check_taper(value(id))?;
            if self.thread.is_some() {
                return Err("a tapered hole has no thread".to_owned());
            }
        }
        if let HoleExtent::Distance { depth } = &self.extent {
            positive("hole depth", *depth)?;
        }
        if let HolePlacement::FaceOffsets {
            offset1, offset2, ..
        } = &self.placement
        {
            for offset in [offset1, offset2] {
                if value(*offset) < 0.0 {
                    return Err(format!(
                        "the hole's edge offsets must not be negative, got {}",
                        value(*offset)
                    ));
                }
            }
        }
        if let HolePlacement::CylinderOffset { offset, .. } = &self.placement
            && value(*offset) < 0.0
        {
            return Err(format!(
                "the hole's offset must not be negative, got {}",
                value(*offset)
            ));
        }
        if let Some(thread) = &self.thread {
            clearance_value("radial clearance", thread.radial_clearance.map(value))?;
            clearance_value("flank clearance", thread.flank_clearance.map(value))?;
            if let Some(custom) = &thread.custom {
                super::thread_table::ThreadData::custom(value(custom.major), value(custom.pitch))?;
            }
            if let Some(starts) = thread.starts {
                let starts = starts_value(value(starts))?;
                if let Ok(data) = thread.size().data() {
                    check_starts(starts, &data)?;
                }
            }
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        // Pieces of bodies the holes split.
        true
    }

    fn tool_use(&self) -> Option<ToolUse> {
        // A pattern repeats only the tool, which has no modelled thread
        // and no taper (mitcad#733).
        if self
            .thread
            .as_ref()
            .is_some_and(|t| t.modeled || t.tapers())
        {
            return None;
        }
        Some(ToolUse {
            operation: Operation::Cut,
            participants: self.participants.clone(),
        })
    }
}

impl HoleDef {
    /// Holes at points of a cylindrical face: each point moved across the
    /// axis onto the cylinder, the hole along the radius into the material
    /// (towards the axis on a shaft, away from it on a bore's wall), the
    /// other way with `flip`.
    fn on_cylinder(&self, cylinder: &Cylinder, points: &[[f64; 3]]) -> Result<Vec<Axis>, String> {
        if points.is_empty() {
            return Err("the hole has no position".to_owned());
        }
        let axis = &cylinder.axis;
        points
            .iter()
            .map(|p| {
                let from: [f64; 3] = std::array::from_fn(|i| p[i] - axis.origin[i]);
                let along = dot(from, axis.direction);
                let out = normalized(std::array::from_fn(|i| from[i] - along * axis.direction[i]))
                    .ok_or("a hole's point lies on the axis of its cylindrical face")?;
                let origin = std::array::from_fn(|i| {
                    axis.origin[i] + along * axis.direction[i] + cylinder.radius * out[i]
                });
                let inward = if cylinder.internal { 1.0 } else { -1.0 };
                let sense = if self.flip { -inward } else { inward };
                Ok(Axis {
                    origin,
                    // (`+ 0.0`: no negative zeros.)
                    direction: out.map(|v| sense * v + 0.0),
                })
            })
            .collect()
    }

    /// The hole on a cylindrical face at the point of its axis `offset`
    /// from a plane through `origin` with `normal` (the distance along the
    /// normal, on the side of the face's middle), on the side of the axis
    /// where `toward` lies ([`HolePlacement::CylinderOffset`]).
    fn at_offset(
        &self,
        cylinder: &Cylinder,
        origin: [f64; 3],
        normal: [f64; 3],
        offset: f64,
        toward: [f64; 3],
    ) -> Result<Vec<Axis>, String> {
        let axis = &cylinder.axis;
        let across = dot(axis.direction, normal);
        if across.abs() < 1e-9 {
            return Err("the hole's plane does not cross the axis of its face".to_owned());
        }
        // Where the axis meets the plane, and the way to the face's middle.
        let t = dot(std::array::from_fn(|i| origin[i] - axis.origin[i]), normal) / across;
        let middle = cylinder.length / 2.0 - t;
        let along = t + middle.signum() * offset / across.abs();
        let centre: [f64; 3] = std::array::from_fn(|i| axis.origin[i] + along * axis.direction[i]);
        let from: [f64; 3] = std::array::from_fn(|i| toward[i] - centre[i]);
        let out = normalized(std::array::from_fn(|i| {
            from[i] - dot(from, axis.direction) * axis.direction[i]
        }))
        .ok_or("the hole's side point lies on the axis of its face")?;
        let point: [f64; 3] = std::array::from_fn(|i| centre[i] + cylinder.radius * out[i]);
        self.on_cylinder(cylinder, &[point])
    }

    /// The holes' start points and their direction into the material.
    pub(crate) fn positions<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<Vec<Axis>, String> {
        let (points, normal) = match &self.placement {
            HolePlacement::Face { body, face, points } => {
                let shape = ctx.surface_body(*body)?;
                let face = super::renamed::face_for(ctx, &shape, face)?;
                let plane = match ctx.kernel.face_plane(&shape, &face) {
                    Ok(plane) => plane,
                    Err(not_planar) => {
                        // A cylindrical face (a shaft's side, mitcad#719):
                        // each point put on the cylinder, the hole across
                        // its axis.
                        let Ok(cylinder) = ctx.kernel.face_cylinder(&shape, &face) else {
                            return Err(not_planar.to_string());
                        };
                        return self.on_cylinder(&cylinder, points);
                    }
                };
                let on_plane = points
                    .iter()
                    .map(|p| {
                        let height = dot(
                            std::array::from_fn(|i| p[i] - plane.origin[i]),
                            plane.normal,
                        );
                        std::array::from_fn(|i| p[i] - height * plane.normal[i])
                    })
                    .collect();
                (on_plane, plane.normal)
            }
            HolePlacement::FaceOffsets {
                body,
                face,
                edge1,
                offset1,
                edge2,
                offset2,
            } => {
                let shape = ctx.surface_body(*body)?;
                let face = super::renamed::face_for(ctx, &shape, face)?;
                let plane = ctx
                    .kernel
                    .face_plane(&shape, &face)
                    .map_err(|e| e.to_string())?;
                let mut lines = Vec::new();
                for edge in [edge1, edge2] {
                    let edge = ctx.edge_on(*body, &shape, edge)?;
                    let line = ctx.axis(&GeomRef::Edge { body: *body, edge })?;
                    lines.push(Axis {
                        origin: line.origin,
                        direction: line.direction,
                    });
                }
                let offsets = [ctx.param(*offset1)?, ctx.param(*offset2)?];
                let point = offset_point(plane.origin, plane.normal, &lines, offsets)?;
                (vec![point], plane.normal)
            }
            HolePlacement::SketchPoints { sketch, points } => {
                let output = ctx.sketch(*sketch)?;
                let mut located = Vec::new();
                for point in points {
                    let uv = match point {
                        SketchPoint::At(uv) => *uv,
                        SketchPoint::Center(curve) => match output.solved.curves.get(curve) {
                            Some(
                                Curve2::Circle { center, .. }
                                | Curve2::Arc { center, .. }
                                | Curve2::Ellipse { center, .. }
                                | Curve2::EllipticalArc { center, .. },
                            ) => *center,
                            _ => {
                                return Err(format!(
                                    "{} has no circle, arc or ellipse {}",
                                    ctx.feature_name(*sketch),
                                    curve.curve_name()
                                ));
                            }
                        },
                        SketchPoint::Point(p) => *output.solved.points.get(p).ok_or_else(|| {
                            format!("{} has no point p{}", ctx.feature_name(*sketch), p.0)
                        })?,
                    };
                    located.push(output.frame.point(uv));
                }
                (located, output.frame.normal())
            }
            HolePlacement::Plane { plane, points } => {
                let frame = ctx.plane(plane)?.frame();
                let normal = frame.normal();
                let on_plane = points
                    .iter()
                    .map(|p| {
                        let height = dot(std::array::from_fn(|i| p[i] - frame.origin[i]), normal);
                        std::array::from_fn(|i| p[i] - height * normal[i])
                    })
                    .collect();
                (on_plane, normal)
            }
            HolePlacement::CylinderOffset {
                body,
                face,
                plane,
                offset,
                toward,
            } => {
                let shape = ctx.surface_body(*body)?;
                let face = super::renamed::face_for(ctx, &shape, face)?;
                let cylinder = ctx
                    .kernel
                    .face_cylinder(&shape, &face)
                    .map_err(|e| e.to_string())?;
                let frame = ctx.plane(plane)?.frame();
                let offset = ctx.param(*offset)?;
                return self.at_offset(&cylinder, frame.origin, frame.normal(), offset, *toward);
            }
            // Placed by picked geometry (mitcad#656): into a shaft or a hole
            // along its axis, into a face against its normal.
            HolePlacement::At { at } => {
                let placed = ctx.place_def(at)?;
                let n = placed.frame.normal();
                let normal = if placed.seat.is_some() {
                    n.map(|v| -v)
                } else {
                    n
                };
                (vec![placed.frame.origin], normal)
            }
        };
        if points.is_empty() {
            return Err("the hole has no position".to_owned());
        }
        let direction = if self.flip {
            normal
        } else {
            normal.map(|v| -v)
        };
        Ok(points
            .into_iter()
            .map(|origin| Axis { origin, direction })
            .collect())
    }
}

/// The point of a plane at given distances from the lines of two edges, on
/// the side of each line where the plane's origin (the face's middle) is.
fn offset_point(
    origin: [f64; 3],
    normal: [f64; 3],
    lines: &[Axis],
    offsets: [f64; 2],
) -> Result<[f64; 3], String> {
    // Rows of the system: across each line in the plane, and the normal.
    let mut rows = [[0.0; 3]; 3];
    let mut right = [0.0; 3];
    for (k, line) in lines.iter().enumerate() {
        let mut across = normalized(cross(normal, line.direction))
            .ok_or("an edge of the hole's position is at right angles to the face")?;
        let side = dot(std::array::from_fn(|i| origin[i] - line.origin[i]), across);
        if side < 0.0 {
            across = across.map(|v| -v);
        }
        rows[k] = across;
        right[k] = offsets[k] + dot(line.origin, across);
    }
    rows[2] = normal;
    right[2] = dot(origin, normal);
    let det = dot(rows[0], cross(rows[1], rows[2]));
    if det.abs() < 1e-9 {
        return Err("the edges of the hole's position are parallel".to_owned());
    }
    // Cramer's rule.
    let solve = |k: usize| {
        let mut m = rows;
        for r in 0..3 {
            m[r][k] = right[r];
        }
        dot(m[0], cross(m[1], m[2])) / det
    };
    Ok([solve(0), solve(1), solve(2)])
}

impl<K: Kernel> Evaluate<K> for HoleDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let positions = self.positions(ctx)?;
        let participants = participant_bodies(ctx, Operation::Cut, &self.participants)?;
        // The size: the table's, or a custom thread's (mitcad#893).
        let thread = match &self.thread {
            Some(thread) => {
                let data = size_of(ctx, &thread.size(), thread.custom.as_ref(), Some(true))?;
                Some((thread, data))
            }
            None => None,
        };
        // A tapering taper pipe thread's hole is drilled at its diameter,
        // the tap drill (mitcad#733); another's bore is the size's minor
        // diameter.
        let diameter = match &thread {
            Some((t, data)) if !t.tapers() => data.minor,
            _ => ctx.positive(self.diameter)?,
        };
        let shape = match &self.kind {
            HoleKind::Simple => HoleShape::Simple,
            HoleKind::Counterbore { diameter: d, depth } => HoleShape::Counterbore {
                diameter: ctx.positive(*d)?,
                depth: ctx.positive(*depth)?,
            },
            HoleKind::Countersink { diameter: d, angle } => HoleShape::Countersink {
                diameter: ctx.positive(*d)?,
                angle: ctx.positive(*angle)?,
            },
            HoleKind::Counterdrill {
                diameter: d,
                depth,
                angle,
            } => HoleShape::Counterdrill {
                diameter: ctx.positive(*d)?,
                depth: ctx.positive(*depth)?,
                angle: ctx.positive(*angle)?,
            },
        };
        let taper = match self.taper {
            Some(id) => {
                let taper = ctx.param(id)?;
                check_taper(taper)?;
                taper
            }
            None => 0.0,
        };
        let tip_angle = if self.flat {
            None
        } else {
            Some(match self.tip_angle {
                Some(id) => ctx.positive(id)?,
                None => DEFAULT_TIP_ANGLE,
            })
        };
        let end = match &self.extent {
            HoleExtent::Distance { depth } => HoleEnd::Distance(ctx.positive(*depth)?),
            HoleExtent::ThroughAll => HoleEnd::ThroughAll,
            HoleExtent::ToObject { object, offset } => {
                let offset = offset.map(|id| ctx.param(id)).transpose()?.unwrap_or(0.0);
                HoleEnd::Target(Bound {
                    target: object.resolve(ctx)?,
                    offset,
                })
            }
        };
        // A simple hole through a sheet cuts its material domain, so the
        // body stays a sheet (mitcad#211); its tool is still the hole's,
        // which patterns and mirrors of it repeat.
        let sheet_depth = match &end {
            _ if thread.is_some() || !matches!(shape, HoleShape::Simple) || taper != 0.0 => None,
            HoleEnd::Distance(d) => Some(Some(*d)),
            HoleEnd::ThroughAll => Some(None),
            HoleEnd::Target(_) => None,
        };
        let sheet_body = match participants.as_slice() {
            [(body, _)] => Some(*body),
            _ => None,
        };
        let spec = HoleSpec {
            feature: ctx.uid,
            positions: &positions,
            diameter,
            shape,
            tip_angle,
            end,
            bodies: participants.iter().map(|(_, s)| s.clone()).collect(),
            taper,
        };
        let tool = ctx.kernel.hole_tool(&spec).map_err(|e| e.to_string())?;
        if let (Some(depth), Some(body)) = (sheet_depth, sheet_body)
            && let Some(mut output) =
                crate::sheet::folded::cut::hole_cut(ctx, body, &positions, diameter, depth)?
        {
            output.tool = Some(tool);
            return Ok(output);
        }
        let mut output = apply_operation(ctx, Operation::Cut, participants, tool, "hole")?;
        // A modelled thread, or a cosmetic taper pipe thread's taper.
        let Some((thread, data)) = thread.filter(|(t, _)| t.modeled || t.tapers()) else {
            return Ok(output);
        };
        // A taper pipe thread's diameters where it starts, at the hole's
        // start (mitcad#733); a trapezoidal thread's with its crest
        // clearance (mitcad#893).
        let (major, minor, pitch_diameter) = (if thread.tapers() {
            data.taper_start(true)
        } else {
            None
        })
        .unwrap_or_else(|| data.profile(true));
        let flank_angle = data.flank_angle();
        let starts = thread_starts(ctx, thread.starts, &data)?;
        // Cut the thread into the walls each body has: from the holes' start,
        // which is the low end of the walls' axes when they point inwards.
        let walls: Vec<FaceName> = (0..positions.len())
            .map(|i| hole_face(ctx.uid, i, "wall"))
            .collect();
        let mut part = thread_part(ctx, thread.length, thread.offset, false)?;
        let clearance = thread_clearance(ctx, thread.radial_clearance, thread.flank_clearance)?;
        // The printed-fit flank clearance capped where the pitch is too
        // fine for it (mitcad#691).
        let clearance = fitted_clearance(
            data.pitch,
            (major, minor, pitch_diameter),
            true,
            clearance,
            flank_from_fit(ctx, thread.flank_clearance),
            flank_angle,
        );
        check_clearance(
            data.pitch,
            (major, minor, pitch_diameter),
            true,
            clearance,
            flank_angle,
        )?;
        let (radial_clearance, flank_clearance) = clearance;
        let mut threaded = Vec::new();
        for change in &mut output.changes {
            let BodyChange::Set(body, shape) = change else {
                continue;
            };
            let mut faces = Vec::new();
            for wall in &walls {
                if ctx
                    .kernel
                    .count_faces(shape, wall)
                    .map_err(|e| e.to_string())?
                    > 0
                {
                    faces.push(wall.clone());
                }
            }
            let Some(first) = faces.first() else {
                continue;
            };
            // The hole's start: the high end of the walls' axes where they
            // point out of the material.
            let start_high = if part.is_some() || thread.tapers() {
                let cylinder = ctx
                    .kernel
                    .face_cylinder(shape, first)
                    .map_err(|e| e.to_string())?;
                dot(cylinder.axis.direction, positions[0].direction) < 0.0
            } else {
                false
            };
            if let Some((_, _, high)) = &mut part {
                *high = start_high;
            }
            let spec = ThreadSpec {
                feature: ctx.uid,
                faces: &faces,
                pitch: data.pitch,
                major,
                minor,
                pitch_diameter,
                right_handed: thread.right_handed,
                angle: 0.0,
                face_angles: &[],
                part,
                radial_clearance,
                flank_clearance,
                groove: thread.modeled,
                chamfer: None,
                taper: data.taper.filter(|_| thread.tapers()).map(|t| ThreadTaper {
                    per_length: t.per_length,
                    high_end: start_high,
                }),
                starts,
                flank_angle,
            };
            let cut = ctx
                .kernel
                .modeled_thread(shape, &spec)
                .map_err(|e| e.to_string())?;
            // The walls the thread left no piece of, or only pieces outside
            // its part (the crest off the wall: what is left does not show
            // where the thread runs, mitcad#628), as they were.
            for wall in faces.iter().filter(|_| thread.modeled) {
                let Ok(cylinder) = ctx.kernel.face_cylinder(shape, wall) else {
                    continue;
                };
                let crest_off = (data.minor / 2.0 + radial_clearance - cylinder.radius).abs()
                    > super::thread::CREST_ON_FACE;
                if crest_off || ctx.kernel.count_faces(&cut, wall).is_ok_and(|n| n == 0) {
                    threaded.push(ThreadedWall {
                        body: *body,
                        wall: wall.clone(),
                        cylinder,
                    });
                }
            }
            *shape = cut;
        }
        output.threaded_walls = threaded;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn offsets_from_two_edges_place_a_point() {
        // The top of a 60 x 40 block: 10 from the front edge (y = 0), 15
        // from the left edge (x = 0).
        let front = Axis {
            origin: [0.0, 0.0, 20.0],
            direction: [1.0, 0.0, 0.0],
        };
        let left = Axis {
            origin: [0.0, 40.0, 20.0],
            direction: [0.0, -1.0, 0.0],
        };
        let point = offset_point(
            [30.0, 20.0, 20.0],
            [0.0, 0.0, 1.0],
            &[front, left],
            [10.0, 15.0],
        )
        .unwrap();
        assert!(close(point, [15.0, 10.0, 20.0]), "{point:?}");
        let error = offset_point(
            [30.0, 20.0, 20.0],
            [0.0, 0.0, 1.0],
            &[front, front],
            [1.0, 2.0],
        )
        .unwrap_err();
        assert!(error.contains("parallel"), "{error}");
    }

    #[test]
    fn holes_serialize_with_defaults_left_out() {
        let def: HoleDef<String> = serde_json::from_value(serde_json::json!({
            "placement": {"type": "sketch_points", "sketch": "F3", "points": [[10, 10], "c5"]},
            "diameter": "d4", "extent": {"type": "through_all"},
            "thread": {"designation": "M6x1", "class": "6H"}}))
        .unwrap();
        let HolePlacement::SketchPoints { points, .. } = &def.placement else {
            panic!("sketch points");
        };
        assert_eq!(
            points,
            &vec![
                SketchPoint::At([10.0, 10.0]),
                SketchPoint::Center(EntityUid(5))
            ]
        );
        assert_eq!(
            serde_json::to_value(&def).unwrap(),
            serde_json::json!({
                "placement": {"type": "sketch_points", "sketch": "F3", "points": [[10.0, 10.0], "c5"]},
                "diameter": "d4", "extent": {"type": "through_all"},
                "thread": {"designation": "M6x1", "class": "6H"}})
        );
        assert_eq!(
            hole_face(FeatureUid(5), 2, "wall").to_string(),
            "F5:hole2.wall"
        );
    }

    #[test]
    fn clearance_holes_keep_their_size_and_fit() {
        // mitcad#903: a hole saved before has no `clearance` and writes
        // none; a clearance hole writes its size and fit (Normal when left
        // out).
        let before = serde_json::json!({
            "placement": {"type": "sketch_points", "sketch": "F3", "points": [[10.0, 10.0], "c5"]},
            "diameter": "d4", "kind": {"type": "counterbore", "diameter": "d5", "depth": "d6"},
            "extent": {"type": "through_all"}});
        let def: HoleDef<String> = serde_json::from_value(before.clone()).unwrap();
        assert_eq!(def.clearance, None);
        assert_eq!(serde_json::to_value(&def).unwrap(), before);
        let mut clearance = before.clone();
        clearance["clearance"] = serde_json::json!({"size": "M6"});
        let def: HoleDef<String> = serde_json::from_value(clearance.clone()).unwrap();
        assert_eq!(
            def.clearance,
            Some(HoleClearance {
                size: "M6".to_owned(),
                fit: ClearanceFit::Normal
            })
        );
        clearance["clearance"] = serde_json::json!({"size": "M6", "fit": "normal"});
        assert_eq!(serde_json::to_value(&def).unwrap(), clearance);
        // Mapped with the other values.
        let mapped = def
            .map_params::<String, ()>(&mut |_, p| Ok(p.clone()))
            .unwrap();
        assert_eq!(mapped.clearance, def.clearance);
        let mut wrong = before;
        wrong["clearance"] = serde_json::json!({"size": "M6", "fit": "loose"});
        assert!(serde_json::from_value::<HoleDef<String>>(wrong).is_err());
    }
}
