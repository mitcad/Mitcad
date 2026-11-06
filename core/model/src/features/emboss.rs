// SPDX-License-Identifier: MIT
//! Emboss (mitcad#458): profiles of a sketch, texts too, raised from a face
//! or sunk into it by a depth. On a planar face, or the sketch plane when
//! no face is named, the profiles are extruded from the face's plane out of
//! the material (raise, joined) or into it (sink, cut). On a cylindrical
//! face they are wrapped onto the cylinder: laid out by arc length around
//! it, not projected, and raised or sunk radially, so that a profile keeps
//! its widths on the face and its walls stand at right angles to it.
//!
//! Projection (mitcad#1091): on any face, the profiles are projected onto
//! it along the sketch normal or a `direction` and raised from it or sunk
//! into it along its normals, so the top follows the face (an offset of the
//! projected region). Spheres, tori and free-form faces always take it; a
//! planar, cylindrical or conical face takes it with `projection` or a
//! `direction`.
//!
//! Wrap offsets (mitcad#1259): profiles wrapped onto a cylinder or a cone
//! can be moved from where the sketch places them, `along_axis` (a length
//! along the face's axis), `around_angle` (about the axis) and
//! `rotation_angle` (turned on the face about their middle). Absent, they
//! are 0 and the wrap is the sketch's.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::extrude::{
    Operation, ProfileRef, apply_operation, check_participants, check_profiles, participant_bodies,
    profile_references, profile_regions, profile_set,
};
use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::reference::FaceRef;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse, is_false,
};
use crate::datum::rotate;
use crate::ids::BodyUid;
use crate::kernel::{
    Axis, Bound, EmbossProjectSpec, EmbossWrapSpec, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeStart,
    Kernel, Plane, SideEnd, Target,
};
use crate::parameters::ParamId;
use crate::profile::{ProfileRegion, SketchFrame};
use mitcad_vec3::{add, dot, scale, sub, unit};

/// Whether the profiles stand up from the face or go down into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbossMode {
    /// Out of the material, joined to the body.
    #[default]
    Raise,
    /// Into the material, cut from the body.
    Sink,
}

impl EmbossMode {
    fn is_raise(&self) -> bool {
        *self == Self::Raise
    }

    /// The operation the tool takes part in.
    pub fn operation(self) -> Operation {
        match self {
            Self::Raise => Operation::Join,
            Self::Sink => Operation::Cut,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct EmbossDef<P = ParamId> {
    /// Regions of one sketch, a set (as an extrusion's): the glyphs of its
    /// texts bound regions too.
    #[serde(deserialize_with = "profile_set")]
    pub profiles: Vec<ProfileRef>,
    /// The face the profiles are raised from or sunk into: a planar face,
    /// a cylindrical or conical face they are wrapped onto, or any other
    /// face they are projected onto. None: the sketch plane, raised along
    /// the sketch normal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<FaceRef>,
    pub depth: P,
    #[serde(default, skip_serializing_if = "EmbossMode::is_raise")]
    pub mode: EmbossMode,
    /// The bodies it joins or cuts; empty for the face's body, or all
    /// bodies without a face.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// The profiles projected onto the face (mitcad#1091) instead of
    /// wrapped onto a cylinder or cone or extruded from a plane. Faces of
    /// other kinds always take them so.
    #[serde(default, skip_serializing_if = "is_false")]
    pub projection: bool,
    /// The direction the profiles are projected along instead of the sketch
    /// normal: an axis, an edge or a sketch line, or a plane or planar face
    /// (its normal). Implies the projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<GeomRef>,
    /// A wrap's profiles moved along the face's axis (mitcad#1259), in its
    /// direction; absent: 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub along_axis: Option<P>,
    /// A wrap's profiles turned about the face's axis, right-handed about
    /// its direction (radians); absent: 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub around_angle: Option<P>,
    /// A wrap's profiles turned on the face about the middle of their
    /// bounds, counter-clockwise in the sketch (radians); absent: 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_angle: Option<P>,
}

impl<P> EmbossDef<P> {
    pub const TYPE: &'static str = "emboss";
    pub const BASE_NAME: &'static str = "Emboss";

    map_params! {
        EmbossDef {
            profiles: clone,
            face: clone,
            depth: param,
            mode: copy,
            participants: clone,
            projection: copy,
            direction: clone,
            along_axis: option,
            around_angle: option,
            rotation_angle: option,
        }
    }

    /// Whether the profiles are projected whatever the face (a planar,
    /// cylindrical or conical face otherwise takes them its own way).
    fn projects(&self) -> bool {
        self.projection || self.direction.is_some()
    }

    /// The bodies it works on when none are listed: the face's.
    fn default_participants(&self) -> Vec<BodyUid> {
        if self.participants.is_empty() {
            self.face.iter().map(|f| f.body).collect()
        } else {
            self.participants.clone()
        }
    }
}

impl FeatureInfo for EmbossDef {
    fn references(&self) -> References {
        let mut references = References::default();
        profile_references(&self.profiles, &mut references);
        if let Some(face) = &self.face {
            face.add_references(&mut references);
        }
        for body in &self.participants {
            references.body(*body);
        }
        if let Some(direction) = &self.direction {
            direction.add_to(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_profiles(ctx, &self.profiles)?;
        if let Some(face) = &self.face {
            face.check(ctx)?;
        } else if self.projects() {
            return Err(
                "a projected emboss needs the face to project the profiles onto".to_owned(),
            );
        }
        if let Some(direction) = &self.direction {
            direction
                .check(ctx, Want::Axis)
                .or_else(|_| direction.check(ctx, Want::Plane))
                .map_err(|e| format!("the projection direction: {e}"))?;
        }
        check_participants(ctx, self.mode.operation(), &self.participants)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let depth = value(self.depth);
        if !(depth > 0.0 && depth.is_finite()) {
            return Err(format!(
                "the emboss depth must be greater than zero, got {depth}"
            ));
        }
        for (slot, name) in [
            (self.along_axis, "along the axis"),
            (self.around_angle, "around the axis"),
            (self.rotation_angle, "rotation"),
        ] {
            if let Some(id) = slot
                && !value(id).is_finite()
            {
                return Err(format!("the wrap's offset {name} is not a number"));
            }
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        // A sink can cut a body into pieces.
        true
    }

    fn tool_use(&self) -> Option<ToolUse> {
        Some(ToolUse {
            operation: self.mode.operation(),
            participants: self.default_participants(),
        })
    }

    fn profiles(&self) -> &[ProfileRef] {
        &self.profiles
    }
}

impl<K: Kernel> Evaluate<K> for EmbossDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let depth = ctx.param(self.depth)?;
        let offsets = WrapOffsets {
            along: optional_param(ctx, self.along_axis)?,
            around: optional_param(ctx, self.around_angle)?,
            rotation: optional_param(ctx, self.rotation_angle)?,
        };
        if !(depth > 0.0 && depth.is_finite()) {
            return Err(format!(
                "the emboss depth must be greater than zero, got {depth}"
            ));
        }
        if !offsets.is_finite() {
            return Err("the wrap's offsets must be numbers".to_owned());
        }
        let (sketch, regions) = profile_regions(ctx, &self.profiles)?;
        let operation = self.mode.operation();
        let participants = participant_bodies(ctx, operation, &self.default_participants())?;
        let tool = self.tool(ctx, sketch.frame, &regions, depth, offsets)?;
        apply_operation(ctx, operation, participants, tool, "emboss")
    }
}

/// Sketch normals within this of a plane's normal count as parallel, and
/// within this of right angles to a cylinder's axis as along it.
const PARALLEL: f64 = 1e-9;
const ALONG_AXIS: f64 = 1e-6;

/// The value of an optional slot, 0 without one.
fn optional_param<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    slot: Option<ParamId>,
) -> Result<f64, String> {
    slot.map_or(Ok(0.0), |id| ctx.param(id))
}

/// The direction Along Axis counts along and Around turns about
/// (mitcad#1259): the face's axis as a unit vector, pointing towards +Z of
/// the component, else (an axis across Z) towards +Y, else +X. The face's
/// own axis may point either way (an extruded rod's cylinder has its axis
/// against the extrusion), so a positive Along Axis moves up a rod however
/// it was made.
pub(crate) fn counted(direction: [f64; 3]) -> [f64; 3] {
    let d = unit(direction).unwrap_or(direction);
    let flip = if d[2].abs() > 1e-9 {
        d[2] < 0.0
    } else if d[1].abs() > 1e-9 {
        d[1] < 0.0
    } else {
        d[0] < 0.0
    };
    if flip { d.map(|v| -v) } else { d }
}

/// Where wrapped profiles go beyond where the sketch places them
/// (mitcad#1259): `along` the face's axis (mm, in the [`counted`]
/// direction), `around` it (radians, right-handed about that direction) and
/// turned by `rotation` on the face about the middle of their bounds
/// (radians, counter-clockwise in the sketch). All 0: the sketch's wrap.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct WrapOffsets {
    pub along: f64,
    pub around: f64,
    pub rotation: f64,
}

impl WrapOffsets {
    fn is_zero(&self) -> bool {
        self.along == 0.0 && self.around == 0.0 && self.rotation == 0.0
    }

    fn is_finite(&self) -> bool {
        self.along.is_finite() && self.around.is_finite() && self.rotation.is_finite()
    }

    /// The sketch frame and regions that wrap where the offsets put them,
    /// about and along `axis`. The rotation turns the regions in the sketch
    /// (on the face's development, so the profiles keep their shape on the
    /// face); Along Axis and Around move the sketch's frame, as moving the
    /// sketch would: along the axis and about it, which on a cylinder moves
    /// the wrapped profiles rigidly and on a cone lays them against it at
    /// the new height (the frame's origin is where the development is laid
    /// against the cone). The inputs are not changed.
    pub(crate) fn placed<'r>(
        &self,
        frame: SketchFrame,
        regions: &'r [ProfileRegion],
        axis: &Axis,
    ) -> (SketchFrame, Cow<'r, [ProfileRegion]>) {
        if self.is_zero() {
            return (frame, Cow::Borrowed(regions));
        }
        let regions = if self.rotation == 0.0 {
            Cow::Borrowed(regions)
        } else {
            let centre = middle(regions);
            Cow::Owned(
                regions
                    .iter()
                    .map(|r| r.rotated(centre, self.rotation))
                    .collect(),
            )
        };
        let direction = counted(axis.direction);
        let turn = |v: [f64; 3]| rotate(v, direction, self.around);
        let from_axis = sub(frame.origin, axis.origin);
        let origin = add(
            add(axis.origin, turn(from_axis)),
            scale(direction, self.along),
        );
        let frame = SketchFrame {
            origin,
            x_axis: turn(frame.x_axis),
            y_axis: turn(frame.y_axis),
        };
        (frame, regions)
    }
}

/// The middle of the regions' bounds, in sketch coordinates.
fn middle(regions: &[ProfileRegion]) -> [f64; 2] {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for region in regions {
        let (low, high) = region.bounds();
        for i in 0..2 {
            min[i] = min[i].min(low[i]);
            max[i] = max[i].max(high[i]);
        }
    }
    if min[0] > max[0] || min[1] > max[1] {
        return [0.0, 0.0];
    }
    [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0]
}

/// The tool of profiles wrapped onto a cylindrical face by arc length, or
/// onto a conical one by its development (mitcad#473), raised out of the
/// material by `depth` or sunk into it; None for a face of another kind.
/// Emboss and Extrude's `wrap` build it alike; Emboss moves the profiles
/// by its `offsets` (mitcad#1259; Extrude passes none).
pub(crate) fn wrapped_tool<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    frame: SketchFrame,
    regions: &[ProfileRegion],
    face: &FaceRef,
    depth: f64,
    raise: bool,
    offsets: WrapOffsets,
) -> Result<Option<K::Shape>, String> {
    let normal = frame.normal();
    let shape = ctx.surface_body(face.body)?;
    let found = ctx.face_on(face.body, &shape, &face.face)?;
    if let Ok(cylinder) = ctx.kernel.face_cylinder(&shape, &found) {
        let axis = cylinder.axis;
        if dot(normal, axis.direction).abs() > ALONG_AXIS {
            return Err(format!(
                "to wrap onto face {}, the sketch plane must be parallel to the \
                 cylinder's axis",
                face.face
            ));
        }
        // Out of the material: away from the axis on an outer face,
        // towards it on a bore's wall.
        let outwards = if cylinder.internal { -1.0 } else { 1.0 };
        let along = if raise { outwards } else { -outwards };
        let to_radius = cylinder.radius + along * depth;
        if to_radius <= cylinder.radius * 1e-6 {
            return Err(format!(
                "the depth {depth} mm reaches the axis of face {} (radius {} mm)",
                face.face, cylinder.radius
            ));
        }
        let (frame, regions) = offsets.placed(frame, regions, &axis);
        let spec = EmbossWrapSpec {
            feature: ctx.uid,
            frame,
            regions: &regions,
            axis,
            radius: cylinder.radius,
            to_radius,
            half_angle: 0.0,
        };
        return ctx
            .kernel
            .wrapped_emboss(&spec)
            .map(Some)
            .map_err(|e| e.to_string());
    }
    if let Ok(cone) = ctx.kernel.face_cone(&shape, &found) {
        if dot(normal, cone.axis).abs() > ALONG_AXIS {
            return Err(format!(
                "to wrap onto face {}, the sketch plane must be parallel to the cone's axis",
                face.face
            ));
        }
        let axis = Axis {
            origin: cone.apex,
            direction: cone.axis,
        };
        let (frame, regions) = offsets.placed(frame, regions, &axis);
        // The cone's radius at the height of the sketch's origin.
        let from: [f64; 3] = std::array::from_fn(|i| frame.origin[i] - cone.apex[i]);
        let height = dot(from, cone.axis);
        let radius = height * cone.half_angle.tan();
        if radius.is_nan() || radius <= 0.0 {
            return Err(format!(
                "to wrap onto face {}, the sketch's origin must lie beside the cone, not \
                 beyond its apex",
                face.face
            ));
        }
        let outwards = if cone.internal { -1.0 } else { 1.0 };
        let offset = if raise { outwards } else { -outwards } * depth;
        if radius + offset * cone.half_angle.cos() <= radius * 1e-6 {
            return Err(format!(
                "the depth {depth} mm reaches the axis of face {}",
                face.face
            ));
        }
        let spec = EmbossWrapSpec {
            feature: ctx.uid,
            frame,
            regions: &regions,
            axis,
            radius,
            to_radius: radius + offset,
            half_angle: cone.half_angle,
        };
        return ctx
            .kernel
            .wrapped_emboss(&spec)
            .map(Some)
            .map_err(|e| e.to_string());
    }
    Ok(None)
}

impl EmbossDef {
    /// The tool body: the profiles extruded from a plane, wrapped onto a
    /// cylinder and grown radially, or projected onto the face and grown
    /// along its normals; faces named after the feature.
    fn tool<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        frame: SketchFrame,
        regions: &[ProfileRegion],
        depth: f64,
        offsets: WrapOffsets,
    ) -> Result<K::Shape, String> {
        let normal = frame.normal();
        let raise = self.mode == EmbossMode::Raise;
        // Along Axis, Around and Rotation move wrapped profiles only.
        let unwrapped = || {
            if offsets.is_zero() {
                Ok(())
            } else {
                Err(
                    "Along Axis, Around and Rotation move only profiles wrapped onto a \
                     cylindrical or conical face"
                        .to_owned(),
                )
            }
        };
        let Some(face) = &self.face else {
            unwrapped()?;
            // From the sketch plane, raised along its normal.
            let direction = if raise { normal } else { normal.map(|v| -v) };
            return self.extrude(
                ctx,
                frame,
                regions,
                direction,
                ExtrudeStart::Offset(0.0),
                depth,
            );
        };
        if self.projects() {
            unwrapped()?;
            return self.projected(ctx, frame, regions, face, depth, raise);
        }
        if let Some(tool) = wrapped_tool(ctx, frame, regions, face, depth, raise, offsets)? {
            return Ok(tool);
        }
        unwrapped()?;
        let shape = ctx.surface_body(face.body)?;
        let found = ctx.face_on(face.body, &shape, &face.face)?;
        let Ok(plane) = ctx.kernel.face_plane(&shape, &found) else {
            // A sphere, a torus, a free-form face: projected (mitcad#1091).
            return self.projected(ctx, frame, regions, face, depth, raise);
        };
        // Along the sketch normal, out of the material or into it.
        let facing = dot(normal, plane.normal);
        if facing.abs() < ALONG_AXIS {
            return Err(format!(
                "the sketch plane stands at right angles to face {}",
                face.face
            ));
        }
        let out = if facing > 0.0 {
            normal
        } else {
            normal.map(|v| -v)
        };
        let direction = if raise { out } else { out.map(|v| -v) };
        let start = if facing.abs() > 1.0 - PARALLEL {
            let offset: [f64; 3] = std::array::from_fn(|i| plane.origin[i] - frame.origin[i]);
            ExtrudeStart::Offset(dot(offset, normal) + 0.0)
        } else {
            // The profiles projected along the sketch normal onto the
            // face's plane.
            ExtrudeStart::Object(Bound {
                target: Target::Plane(Plane {
                    origin: plane.origin,
                    normal: plane.normal,
                }),
                offset: 0.0,
            })
        };
        self.extrude(ctx, frame, regions, direction, start, depth)
    }

    /// The tool of the profiles projected onto the face along the sketch
    /// normal or the direction and grown along the face's normals
    /// (mitcad#1091).
    fn projected<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        frame: SketchFrame,
        regions: &[ProfileRegion],
        face: &FaceRef,
        depth: f64,
        raise: bool,
    ) -> Result<K::Shape, String> {
        let direction = match &self.direction {
            Some(reference) => direction_of(ctx, reference)?,
            None => frame.normal(),
        };
        if dot(direction, frame.normal()).abs() < ALONG_AXIS {
            return Err(
                "the projection direction lies in the sketch plane: it must cross it".to_owned(),
            );
        }
        let shape = ctx.surface_body(face.body)?;
        let found = ctx.face_on(face.body, &shape, &face.face)?;
        let spec = EmbossProjectSpec {
            feature: ctx.uid,
            frame,
            regions,
            body: &shape,
            face: &found,
            direction,
            depth: if raise { depth } else { -depth },
        };
        ctx.kernel
            .projected_emboss(&spec)
            .map_err(|e| format!("to project onto face {}: {e}", face.face))
    }

    fn extrude<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        frame: SketchFrame,
        regions: &[ProfileRegion],
        direction: [f64; 3],
        start: ExtrudeStart<K::Shape>,
        depth: f64,
    ) -> Result<K::Shape, String> {
        let spec = ExtrudeFeatureSpec {
            feature: ctx.uid,
            frame,
            regions,
            direction,
            start,
            side1: ExtrudeSide {
                end: SideEnd::Distance(depth),
                taper: 0.0,
                thin: None,
            },
            side2: None,
        };
        ctx.kernel.extrude_feature(&spec).map_err(|e| e.to_string())
    }
}

/// The unit vector of a projection's direction: an axis's (an edge, a
/// sketch line, a face of revolution), else a plane's normal (a planar
/// face, an origin or construction plane).
fn direction_of<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    reference: &GeomRef,
) -> Result<[f64; 3], String> {
    let along = match ctx.axis(reference) {
        Ok(axis) => axis.direction,
        Err(not_axis) => match ctx.plane(reference) {
            Ok(plane) => plane.normal(),
            Err(_) => return Err(format!("the projection direction: {not_axis}")),
        },
    };
    unit(along).ok_or_else(|| "the projection direction has no length".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{ProfileLoop, ProfileSegment, SegmentGeometry};

    /// A rectangle from `min` to `max` in sketch coordinates.
    fn rectangle(min: [f64; 2], max: [f64; 2]) -> ProfileRegion {
        let corners = [min, [max[0], min[1]], max, [min[0], max[1]]];
        let keys = ["c1[c4,c2]", "c2[c1,c3]", "c3[c2,c4]", "c4[c3,c1]"];
        ProfileRegion {
            key: "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"
                .parse()
                .unwrap(),
            loops: vec![ProfileLoop {
                segments: (0..4)
                    .map(|i| ProfileSegment {
                        key: keys[i].parse().unwrap(),
                        geometry: SegmentGeometry::Line {
                            start: corners[i],
                            end: corners[(i + 1) % 4],
                        },
                    })
                    .collect(),
            }],
        }
    }

    fn near(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    /// The XZ plane (sketch y along -Z, normal +Y) through the Z axis, as a
    /// sketch beside a rod along Z.
    const XZ: SketchFrame = SketchFrame {
        origin: [0.0, 0.0, 0.0],
        x_axis: [1.0, 0.0, 0.0],
        y_axis: [0.0, 0.0, -1.0],
    };
    const Z: Axis = Axis {
        origin: [0.0, 0.0, 0.0],
        direction: [0.0, 0.0, 1.0],
    };

    /// mitcad#1259: no offsets leave the sketch's frame and regions as
    /// they are (borrowed, not copied).
    #[test]
    fn zero_offsets_are_the_sketchs_wrap() {
        let regions = [rectangle([-2.0, -10.0], [2.0, -2.0])];
        let (frame, placed) = WrapOffsets::default().placed(XZ, &regions, &Z);
        assert_eq!(frame, XZ);
        assert!(matches!(placed, Cow::Borrowed(_)));
        assert_eq!(&*placed, &regions[..]);
    }

    /// A 4 x 8 rectangle from z = 2 to 10 on the +Y side of the Z axis,
    /// moved 10 along the axis, a quarter turn around it and a quarter turn
    /// on the face: 8 across and 4 along about its middle, which is at z =
    /// 16 on the -X side (the sketch normal there), where it wraps.
    #[test]
    fn offsets_move_the_frame_and_turn_the_regions() {
        let quarter = std::f64::consts::FRAC_PI_2;
        let regions = [rectangle([-2.0, -10.0], [2.0, -2.0])];
        let offsets = WrapOffsets {
            along: 10.0,
            around: quarter,
            rotation: quarter,
        };
        let (frame, placed) = offsets.placed(XZ, &regions, &Z);
        assert!(near(frame.origin, [0.0, 0.0, 10.0]), "{frame:?}");
        assert!(near(frame.x_axis, [0.0, 1.0, 0.0]), "{frame:?}");
        assert!(near(frame.y_axis, [0.0, 0.0, -1.0]), "{frame:?}");
        assert!(near(frame.normal(), [-1.0, 0.0, 0.0]), "{frame:?}");
        let (min, max) = placed[0].bounds();
        assert!(
            near([min[0], min[1], 0.0], [-4.0, -8.0, 0.0])
                && near([max[0], max[1], 0.0], [4.0, -4.0, 0.0]),
            "{min:?} {max:?}"
        );
        assert_eq!(placed[0].key, regions[0].key);
        // The middle on the axis at z = 16, the sketch turned to face -X.
        assert!(near(frame.point([0.0, -6.0]), [0.0, 0.0, 16.0]));
        // The input regions are not changed.
        assert_eq!(regions[0], rectangle([-2.0, -10.0], [2.0, -2.0]));
    }

    /// A sketch plane beside the axis (y = 5) turned half a turn about it
    /// lies at y = -5, facing the other way. Along Axis counts towards +Z
    /// whichever way the face's axis points (an extruded rod's points down):
    /// the frame moves up along a reversed axis too, and Around turns
    /// right-handed about +Z.
    #[test]
    fn around_turns_a_plane_beside_the_axis_and_along_counts_up() {
        let beside = SketchFrame {
            origin: [3.0, 5.0, 1.0],
            ..XZ
        };
        let regions = [rectangle([0.0, 0.0], [1.0, 1.0])];
        let half = WrapOffsets {
            around: std::f64::consts::PI,
            ..WrapOffsets::default()
        };
        let (frame, _) = half.placed(beside, &regions, &Z);
        assert!(near(frame.origin, [-3.0, -5.0, 1.0]), "{frame:?}");
        assert!(near(frame.normal(), [0.0, -1.0, 0.0]), "{frame:?}");
        let down = Axis {
            origin: [0.0, 0.0, 7.0],
            direction: [0.0, 0.0, -2.0],
        };
        let along = WrapOffsets {
            along: 4.0,
            ..WrapOffsets::default()
        };
        let (frame, placed) = along.placed(beside, &regions, &down);
        assert!(near(frame.origin, [3.0, 5.0, 5.0]), "{frame:?}");
        assert!(matches!(placed, Cow::Borrowed(_)));
        let quarter = WrapOffsets {
            around: std::f64::consts::FRAC_PI_2,
            ..WrapOffsets::default()
        };
        let (frame, _) = quarter.placed(beside, &regions, &down);
        assert!(near(frame.origin, [-5.0, 3.0, 1.0]), "{frame:?}");
        // An axis across Z counts towards +Y, else +X.
        assert!(near(counted([0.0, -3.0, 0.0]), [0.0, 1.0, 0.0]));
        assert!(near(counted([-1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]));
        assert!(near(counted([0.0, 0.0, -1.0]), [0.0, 0.0, 1.0]));
    }
}
