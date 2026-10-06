// SPDX-License-Identifier: MIT
//! Hole: drilled holes cut into the participant bodies: simple, counterbore,
//! countersink or counterdrill, straight or tapered, with a drill point or
//! a flat bottom, to a depth, through all or up to an object, at points on
//! a planar face or at sketch points, optionally tapped with a thread whose
//! size sets the bore (cosmetic, or modelled).
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
use super::geom_ref::GeomRef;
use super::reference::{ExtentObject, normalized};
use super::thread::{ThreadSize, thread_part};
use super::thread_table::ThreadStandard;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    ToolUse, is_false,
};
use crate::ids::{BodyUid, EntityUid, FeatureUid, curve_serde};
use crate::kernel::{Axis, Bound, HoleEnd, HoleShape, HoleSpec, Kernel, ThreadSpec};
use crate::parameters::ParamId;
use crate::profile::{cross, dot};
use crate::sketch::geometry::Curve2;
use crate::topo::{EdgeName, FaceName};

/// The default drill point, 118 degrees.
pub const DEFAULT_TIP_ANGLE: f64 = 118.0 * std::f64::consts::PI / 180.0;

/// Where the holes are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HolePlacement<P = ParamId> {
    /// Points on a planar face, each put on its plane
    /// (`setPositionByPoint` in .f3d designs): one hole per point.
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
}

/// A point of a sketch: coordinates in it, the centre of a circle, arc or
/// ellipse of it (`"c5"`), or a sketch point entity (`"p3"`, as
/// `setPositionBySketchPoints` in .f3d designs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SketchPoint {
    At([f64; 2]),
    Center(#[serde(with = "curve_serde")] EntityUid),
    Point(#[serde(with = "crate::sketch::point_serde")] EntityUid),
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
/// the size's basic minor diameter; a modelled one is cut into the wall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoleThread<P = ParamId> {
    #[serde(default, skip_serializing_if = "is_iso")]
    pub standard: ThreadStandard,
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
    /// The hole's diameter; a tapped hole's thread sets it instead.
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
        })
    }

    /// The number of holes, when it does not depend on evaluation.
    pub fn count(&self) -> usize {
        match &self.placement {
            HolePlacement::Face { points, .. } => points.len(),
            HolePlacement::FaceOffsets { .. } => 1,
            HolePlacement::SketchPoints { points, .. } => points.len(),
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
            if thread.modeled {
                thread.standard.check_modeled()?;
            }
            thread.size().check(Some(true))?;
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
        let diameter = if self.thread.is_some() {
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
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        // Pieces of bodies the holes split.
        true
    }

    fn tool_use(&self) -> Option<ToolUse> {
        // A pattern repeats only the tool, which has no modelled thread.
        if self.thread.as_ref().is_some_and(|t| t.modeled) {
            return None;
        }
        Some(ToolUse {
            operation: Operation::Cut,
            participants: self.participants.clone(),
        })
    }
}

impl HoleDef {
    /// The holes' start points and their direction into the material.
    fn positions<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<Vec<Axis>, String> {
        let (points, normal) = match &self.placement {
            HolePlacement::Face { body, face, points } => {
                let shape = ctx.body(*body)?;
                let plane = ctx
                    .kernel
                    .face_plane(&shape, face)
                    .map_err(|e| e.to_string())?;
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
                let shape = ctx.body(*body)?;
                let plane = ctx
                    .kernel
                    .face_plane(&shape, face)
                    .map_err(|e| e.to_string())?;
                let mut lines = Vec::new();
                for edge in [edge1, edge2] {
                    if ctx.kernel.count_edges(&shape, edge) == 0 {
                        return Err(ctx.missing_edge(*body, edge));
                    }
                    let line = ctx.axis(&GeomRef::Edge {
                        body: *body,
                        edge: (**edge).clone(),
                    })?;
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
        let thread = match &self.thread {
            Some(thread) => Some((thread, thread.size().check(Some(true))?)),
            None => None,
        };
        let diameter = match &thread {
            Some((_, data)) => data.minor,
            None => ctx.positive(self.diameter)?,
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
        let mut output = apply_operation(ctx, Operation::Cut, participants, tool, "hole")?;
        let Some((thread, data)) = thread.filter(|(t, _)| t.modeled) else {
            return Ok(output);
        };
        // Cut the thread into the walls each body has: from the holes' start,
        // which is the low end of the walls' axes when they point inwards.
        let walls: Vec<FaceName> = (0..positions.len())
            .map(|i| hole_face(ctx.uid, i, "wall"))
            .collect();
        let mut part = thread_part(ctx, thread.length, thread.offset, false)?;
        for change in &mut output.changes {
            let BodyChange::Set(_, shape) = change else {
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
            if let Some((_, _, high)) = &mut part {
                let cylinder = ctx
                    .kernel
                    .face_cylinder(shape, first)
                    .map_err(|e| e.to_string())?;
                *high = dot(cylinder.axis.direction, positions[0].direction) < 0.0;
            }
            let spec = ThreadSpec {
                feature: ctx.uid,
                faces: &faces,
                pitch: data.pitch,
                depth: data.depth,
                right_handed: thread.right_handed,
                part,
            };
            *shape = ctx
                .kernel
                .modeled_thread(shape, &spec)
                .map_err(|e| e.to_string())?;
        }
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
}
