// SPDX-License-Identifier: MIT
//! Joint features (mitcad#55): timeline features that connect occurrences
//! and place them (see `crate::joints` for the kinematics and what
//! recompute does with them).
//!
//! - `joint`: origin `a` on geometry of one occurrence joined to origin
//!   `b` on another's, with a kind ([`JointKind`]: its free motions), an
//!   offset along and an angle about the joint's z axis, a flip, limits
//!   of the free motions and the values they are driven to (its
//!   position);
//! - `as_built_joint`: two occurrences joined where they are: the relative
//!   placement recorded when it was made, with an optional origin that
//!   sets the frame of the free motions;
//! - `joint_origin`: a frame on geometry of the feature's component (a
//!   construction feature; its datum is a plane with that frame), for
//!   joints to use;
//! - `rigid_group`: occurrences that move as one.
//!
//! Occurrence paths are relative to the feature's component (`O1/O4`; the
//! empty path is that component's own geometry). A joint's geometry is in
//! the component its path ends in.

use std::collections::BTreeMap;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::construction::datum_kind;
use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::pattern::none;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, is_false,
};
use crate::datum::{Datum, DatumPlane, Vec3};
use crate::ids::OccurrenceUid;
use crate::joints::{self, JointKind, Motion, SlideAxis};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::transform::Transform;

pub use mitcad_model_base::occurrence::OccurrencePath;

/// Parts of a joint origin's frame given instead of the resolved ones, in
/// the component's coordinates (mm, directions); `shift` then moves the
/// origin along the frame's own axes as they are after the override (mm),
/// so it follows geometry that moves (BUILD's corner on a profile's end,
/// mitcad#417).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_axis: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_axis: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shift: Option<Vec3>,
}

impl FrameOverride {
    fn check(&self) -> Result<(), String> {
        let numbers: Vec<f64> = [self.origin, self.z_axis, self.x_axis, self.shift]
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        if !numbers.iter().all(|v| v.is_finite()) {
            return Err("the frame override's numbers must be finite".to_owned());
        }
        if self.z_axis.is_some_and(|z| mitcad_vec3::unit(z).is_none()) {
            return Err("the frame override's z axis is zero".to_owned());
        }
        if self.x_axis.is_some_and(|x| mitcad_vec3::unit(x).is_none()) {
            return Err("the frame override's x axis is zero".to_owned());
        }
        Ok(())
    }
}

/// Where one side of a joint is: geometry of the component an occurrence
/// path ends in, and optionally parts of the frame given directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointOrigin {
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub occurrence: OccurrencePath,
    pub geometry: GeomRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_override: Option<FrameOverride>,
}

impl JointOrigin {
    pub(crate) fn add_references(&self, references: &mut References) {
        self.geometry.add_to(references);
    }

    pub(crate) fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_geometry(ctx, &self.geometry)?;
        if let Some(o) = &self.frame_override {
            o.check()?;
        }
        Ok(())
    }
}

/// Checks that a joint origin's geometry can give a frame (only
/// evaluation can tell whether a face is planar or an edge circular).
fn check_geometry(ctx: &CheckContext<'_>, geometry: &GeomRef) -> Result<(), String> {
    match geometry {
        GeomRef::Body(_) => Err(format!(
            "{geometry} gives no frame for a joint (a face, an edge, a point, a plane or an axis)"
        )),
        GeomRef::Datum(uid) => {
            let entry = ctx.feature(*uid)?;
            if datum_kind(&entry.def).is_some() {
                Ok(())
            } else {
                Err(format!(
                    "{} ({uid}) is not construction geometry",
                    entry.name
                ))
            }
        }
        GeomRef::Face { .. } => geometry.check(ctx, Want::Face),
        GeomRef::Edge { .. } => geometry.check(ctx, Want::EdgeOrFace),
        GeomRef::SketchCurves { sketch, curves } => {
            let (entry, def) = ctx.sketch(*sketch)?;
            match curves.as_slice() {
                [curve] if def.entity(*curve).is_some_and(|e| !e.is_point()) => Ok(()),
                [curve] => Err(format!(
                    "{} has no curve {}",
                    entry.name,
                    curve.curve_name()
                )),
                _ => Err(format!("a joint origin is one curve of {}", entry.name)),
            }
        }
        GeomRef::FixedPlane { .. } => geometry.check(ctx, Want::Plane),
        GeomRef::FixedAxis { .. } => geometry.check(ctx, Want::Axis),
        GeomRef::Origin(_) => Ok(()),
        GeomRef::Mount { .. } => geometry.check(ctx, Want::Plane),
        GeomRef::Vertex { .. } | GeomRef::SketchPoint { .. } | GeomRef::FixedPoint(_) => {
            geometry.check(ctx, Want::Point)
        }
    }
}

/// The limits of one free motion: values (expressions allowed). Rest is
/// the value the motion is held at unless the joint's position gives
/// another; without either the motion is free.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limit<P> {
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub min: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub max: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub rest: Option<P>,
}

pub type Limits<P> = BTreeMap<Motion, Limit<P>>;

/// The values free motions are driven to (mm, radians; expressions
/// allowed), each within its limits.
pub type JointPosition<P> = BTreeMap<Motion, P>;

/// Slot names of a position: `position.tz.position`, a rotation's
/// `position.rz.position_angle`.
fn map_position<P, Q, E>(
    position: &JointPosition<P>,
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<JointPosition<Q>, E> {
    let mut out = BTreeMap::new();
    for (motion, value) in position {
        let suffix = if motion.is_rotation() { "_angle" } else { "" };
        out.insert(
            *motion,
            f(&format!("position.{motion}.position{suffix}"), value)?,
        );
    }
    Ok(out)
}

/// Slot names of limits: `limits.tx.min`; a rotation's end in `angle`
/// (`limits.rz.min_angle`), so they take angles.
fn map_limits<P, Q, E>(
    limits: &Limits<P>,
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Limits<Q>, E> {
    let mut out = BTreeMap::new();
    for (motion, limit) in limits {
        let suffix = if motion.is_rotation() { "_angle" } else { "" };
        let mut slot = |name: &str, value: &Option<P>| -> Result<Option<Q>, E> {
            match value {
                Some(v) => Ok(Some(f(&format!("limits.{motion}.{name}{suffix}"), v)?)),
                None => Ok(None),
            }
        };
        out.insert(
            *motion,
            Limit {
                min: slot("min", &limit.min)?,
                max: slot("max", &limit.max)?,
                rest: slot("rest", &limit.rest)?,
            },
        );
    }
    Ok(out)
}

/// Limits and positions only for the kind's free motions; a slider's axis
/// only on sliders.
fn check_kind<P>(
    kind: JointKind,
    slide: SlideAxis,
    limits: &Limits<P>,
    position: &JointPosition<P>,
) -> Result<(), String> {
    if kind != JointKind::Slider && !slide.is_z() {
        return Err("slide_axis is for slider joints".to_owned());
    }
    let motions = kind.motions(slide);
    for motion in limits.keys().chain(position.keys()) {
        if !motions.contains(motion) {
            return Err(format!(
                "a {kind} joint has no free motion {motion} (it has {})",
                if motions.is_empty() {
                    "none".to_owned()
                } else {
                    motions
                        .iter()
                        .map(|m| m.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ));
        }
    }
    Ok(())
}

/// Each motion's limits in order: min ≤ rest ≤ max, and min ≤ position ≤
/// max.
fn check_limits(
    limits: &Limits<ParamId>,
    position: &JointPosition<ParamId>,
    value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
) -> Result<(), String> {
    for (motion, limit) in limits {
        let mut get = |id: &Option<ParamId>| id.map(&mut *value).transpose();
        let (min, max, rest) = (get(&limit.min)?, get(&limit.max)?, get(&limit.rest)?);
        let at = get(&position.get(motion).copied())?;
        if let (Some(min), Some(max)) = (min, max)
            && min > max
        {
            return Err(format!(
                "limits of {motion}: the minimum is greater than the maximum"
            ));
        }
        let outside = |v: f64| min.is_some_and(|m| v < m) || max.is_some_and(|m| v > m);
        if rest.is_some_and(outside) {
            return Err(format!(
                "limits of {motion}: the rest value is outside the limits"
            ));
        }
        if at.is_some_and(outside) {
            return Err(format!("the position of {motion} is outside its limits"));
        }
    }
    for id in position.values() {
        value(*id)?;
    }
    Ok(())
}

/// Reads a joint's values through the evaluation, so the cache keys on
/// them, and checks the limits.
fn read_values<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    params: &[ParamId],
    limits: &Limits<ParamId>,
    position: &JointPosition<ParamId>,
) -> Result<(), String> {
    for id in params {
        ctx.param(*id)?;
    }
    check_limits(limits, position, &mut |id| ctx.param(id))
}

// joint

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointDef<P = ParamId> {
    pub kind: JointKind,
    /// A slider's axis in the joint's frame (z unless given).
    #[serde(default, skip_serializing_if = "SlideAxis::is_z")]
    pub slide_axis: SlideAxis,
    pub a: JointOrigin,
    pub b: JointOrigin,
    /// Along the joint's z axis, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// About the joint's z axis, radians.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
    /// Turns origin `a` over (half a turn about its x axis).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub limits: Limits<P>,
    /// Values the free motions are driven to (`drive_joint`).
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub position: JointPosition<P>,
    /// Where the free motions are when the joint places its moving side
    /// (mm, radians; numbers, mitcad#1745): the Joint command's new joint
    /// at 0, so that its origins meet at the offset and angle; a drag's
    /// values, so that the drag is kept. A free motion without one starts
    /// at the value nearest to where the side is. Unused for a motion
    /// that is driven and for a joint that closes a loop.
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub start: BTreeMap<Motion, f64>,
}

impl<P> JointDef<P> {
    pub const TYPE: &'static str = "joint";
    pub const BASE_NAME: &'static str = "Joint";

    map_params! {
        JointDef {
            kind: copy,
            slide_axis: copy,
            a: clone,
            b: clone,
            offset: option,
            angle: option,
            flip: copy,
            limits: with(map_limits),
            position: with(map_position),
            start: clone,
        }
    }
}

/// A joint's start values: free motions of its kind, finite numbers.
fn check_start(
    kind: JointKind,
    slide: SlideAxis,
    start: &BTreeMap<Motion, f64>,
) -> Result<(), String> {
    let motions = kind.motions(slide);
    for (motion, value) in start {
        if !motions.contains(motion) {
            return Err(format!("start: a {kind} joint has no free motion {motion}"));
        }
        if !value.is_finite() {
            return Err(format!("start: the value of {motion} must be finite"));
        }
    }
    Ok(())
}

impl FeatureInfo for JointDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.a.add_references(&mut references);
        self.b.add_references(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_kind(self.kind, self.slide_axis, &self.limits, &self.position)?;
        check_start(self.kind, self.slide_axis, &self.start)?;
        self.a.check(ctx).map_err(|e| format!("a: {e}"))?;
        self.b.check(ctx).map_err(|e| format!("b: {e}"))?;
        if self.a.occurrence.is_empty() && self.b.occurrence.is_empty() {
            return Err(
                "a joint connects occurrences: a and b are both the component's own geometry"
                    .to_owned(),
            );
        }
        if self.a.occurrence == self.b.occurrence {
            return Err(format!(
                "a and b are on the same occurrence ({})",
                self.a.occurrence
            ));
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_limits(&self.limits, &self.position, &mut |id| Ok(value(id)))
    }
}

impl<K: Kernel> Evaluate<K> for JointDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let params: Vec<ParamId> = self.offset.iter().chain(&self.angle).copied().collect();
        read_values(ctx, &params, &self.limits, &self.position)?;
        Ok(FeatureOutput::default())
    }
}

// as_built_joint

/// The relative placement as rows (see `occurrence::rows`), optional.
mod relative_rows {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::assembly::{from_rows, matrix_rows};
    use crate::transform::Transform;

    pub fn serialize<S: Serializer>(t: &Option<Transform>, s: S) -> Result<S::Ok, S::Error> {
        t.as_ref().map(matrix_rows).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Transform>, D::Error> {
        Option::<Vec<Vec<f64>>>::deserialize(d)?
            .map(|rows| from_rows(&rows).map_err(serde::de::Error::custom))
            .transpose()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsBuiltJointDef<P = ParamId> {
    pub kind: JointKind,
    #[serde(default, skip_serializing_if = "SlideAxis::is_z")]
    pub slide_axis: SlideAxis,
    pub a: OccurrencePath,
    pub b: OccurrencePath,
    /// The frame of the free motions; b's coordinates without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<JointOrigin>,
    /// a's placement in b's coordinates when the joint was made (rows of a
    /// rotation and a translation); without it the joint takes them as
    /// they are where it comes in the timeline.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "relative_rows"
    )]
    pub relative: Option<Transform>,
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub limits: Limits<P>,
    /// Values the free motions are driven to (`drive_joint`).
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub position: JointPosition<P>,
}

impl<P> AsBuiltJointDef<P> {
    pub const TYPE: &'static str = "as_built_joint";
    pub const BASE_NAME: &'static str = "AsBuiltJoint";

    map_params! {
        AsBuiltJointDef {
            kind: copy,
            slide_axis: copy,
            a: clone,
            b: clone,
            origin: clone,
            relative: copy,
            limits: with(map_limits),
            position: with(map_position),
        }
    }
}

impl FeatureInfo for AsBuiltJointDef {
    fn references(&self) -> References {
        let mut references = References::default();
        if let Some(origin) = &self.origin {
            origin.add_references(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_kind(self.kind, self.slide_axis, &self.limits, &self.position)?;
        if self.a.is_empty() && self.b.is_empty() {
            return Err("an as-built joint connects occurrences: a and b are empty".to_owned());
        }
        if self.a == self.b {
            return Err(format!("a and b are the same occurrence ({})", self.a));
        }
        if let Some(origin) = &self.origin {
            origin.check(ctx).map_err(|e| format!("origin: {e}"))?;
        }
        if let Some(t) = &self.relative
            && !(t.is_finite() && crate::assembly::is_rigid(t))
        {
            return Err("the relative placement must be a rotation and a translation".to_owned());
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_limits(&self.limits, &self.position, &mut |id| Ok(value(id)))
    }
}

impl<K: Kernel> Evaluate<K> for AsBuiltJointDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        read_values(ctx, &[], &self.limits, &self.position)?;
        Ok(FeatureOutput::default())
    }
}

// joint_origin

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointOriginDef<P = ParamId> {
    pub geometry: GeomRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_override: Option<FrameOverride>,
    /// Along the frame's z axis, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// About the frame's z axis, radians.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
    /// Turns the frame over (half a turn about its x axis).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// The frame is a mount frame of the part (mitcad#525): where it
    /// mounts on another part, or where others mount on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount: Option<MountSpec>,
}

impl<P> JointOriginDef<P> {
    pub const TYPE: &'static str = "joint_origin";
    pub const BASE_NAME: &'static str = "JointOrigin";

    map_params! {
        JointOriginDef {
            geometry: clone,
            frame_override: copy,
            offset: option,
            angle: option,
            flip: copy,
            mount: clone,
        }
    }
}

// Mount frames (mitcad#525).

/// The prefix of a mount frame's joint origin named after it, as BUILD's
/// parts name theirs (`mount:slot_x+_1`, mitcad#375).
pub const MOUNT_PREFIX: &str = "mount:";

/// What a mount frame is for (`docs/libraries.md`, *Mount frames*).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountKind {
    /// The part lies on a planar face: z out of the part, towards the face.
    #[default]
    Face,
    /// The part's hole goes onto a shaft or a pin: on its axis, z along it.
    Hole,
    /// The part (a screw, a pin) goes into a hole: on its axis at the face
    /// that bears on the hole's rim, z along the shank towards its tip.
    Shaft,
    /// Other parts stand on its cells (a baseplate): `pitch` and `cells`,
    /// the cells along +x and +y from the origin, z out of the part.
    Grid,
    /// The part stands on a grid's cells (a bin): `footprint` in cells, the
    /// origin in the middle of the footprint, z out of the part.
    Foot,
}

impl MountKind {
    pub const ALL: [Self; 5] = [Self::Face, Self::Hole, Self::Shaft, Self::Grid, Self::Foot];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Face => "face",
            Self::Hole => "hole",
            Self::Shaft => "shaft",
            Self::Grid => "grid",
            Self::Foot => "foot",
        }
    }

    /// The joint kinds a frame of this kind may ask for: how the part may
    /// move once placed.
    pub fn joints(self) -> &'static [JointKind] {
        match self {
            Self::Face => &[JointKind::Rigid, JointKind::Planar, JointKind::Revolute],
            Self::Hole | Self::Shaft => &[
                JointKind::Rigid,
                JointKind::Revolute,
                JointKind::Cylindrical,
                JointKind::Slider,
            ],
            Self::Grid | Self::Foot => &[JointKind::Rigid],
        }
    }
}

fn rigid() -> JointKind {
    JointKind::Rigid
}

fn is_rigid(kind: &JointKind) -> bool {
    *kind == JointKind::Rigid
}

/// A joint origin's mount frame: its name, its kind, the joint a placement
/// on it makes and, for grids and feet, their cells.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountSpec {
    /// Letters, digits, `_`, `-`, `+` and `.` (as BUILD's frames).
    pub name: String,
    #[serde(default)]
    pub kind: MountKind,
    /// How the placed part may move: `rigid` (fixed), `planar` (free on the
    /// face), `revolute` (turns), ...
    #[serde(default = "rigid", skip_serializing_if = "is_rigid")]
    pub joint: JointKind,
    /// A grid's (or a foot's) cell size along the frame's x and y, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<[f64; 2]>,
    /// A grid's number of cells along x and y.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cells: Option<[u32; 2]>,
    /// A foot's size in cells along its x and y.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<[u32; 2]>,
}

/// Whether `name` can name a mount frame: 1 to 64 of letters, digits, `_`,
/// `-`, `+` and `.`.
pub fn valid_mount_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b'.'))
}

impl MountSpec {
    /// A frame of `kind` named `name`, fixed once placed.
    pub fn new(name: &str, kind: MountKind) -> Self {
        Self {
            name: name.to_owned(),
            kind,
            joint: JointKind::Rigid,
            pitch: None,
            cells: None,
            footprint: None,
        }
    }

    /// What is wrong with it, by itself.
    pub fn check(&self) -> Result<(), String> {
        if !valid_mount_name(&self.name) {
            return Err(format!(
                "“{}” is no mount frame name (1 to 64 of letters, digits, _ - + .)",
                self.name
            ));
        }
        let kind = self.kind.as_str();
        if !self.kind.joints().contains(&self.joint) {
            let allowed: Vec<&str> = self.kind.joints().iter().map(|j| j.as_str()).collect();
            return Err(format!(
                "a {kind} frame's joint is {}, not {}",
                allowed.join(", "),
                self.joint
            ));
        }
        if let Some(pitch) = self.pitch {
            if !matches!(self.kind, MountKind::Grid | MountKind::Foot) {
                return Err(format!(
                    "a {kind} frame has no pitch (a grid's or a foot's)"
                ));
            }
            if !pitch.iter().all(|p| p.is_finite() && *p > 0.0) {
                return Err("the pitch is two lengths above 0".to_owned());
            }
        }
        match (self.kind, self.cells) {
            (MountKind::Grid, Some(cells)) if cells.contains(&0) => {
                return Err("a grid has at least one cell each way".to_owned());
            }
            (MountKind::Grid, None) => return Err("a grid gives its cells".to_owned()),
            (MountKind::Grid, Some(_)) | (_, None) => {}
            (_, Some(_)) => return Err(format!("a {kind} frame has no cells (a grid's)")),
        }
        if self.kind == MountKind::Grid && self.pitch.is_none() {
            return Err("a grid gives its pitch".to_owned());
        }
        match (self.kind, self.footprint) {
            (MountKind::Foot, Some(footprint)) if footprint.contains(&0) => {
                Err("a foot covers at least one cell each way".to_owned())
            }
            (MountKind::Foot, None) => Err("a foot gives its footprint in cells".to_owned()),
            (MountKind::Foot, Some(_)) | (_, None) => Ok(()),
            (_, Some(_)) => Err(format!("a {kind} frame has no footprint (a foot's)")),
        }
    }
}

/// The mount frame a joint origin is: its own `mount`, else, when it is
/// named `mount:<name>` (BUILD's parts), a face frame of that name.
pub fn mount_of<P>(feature_name: &str, def: &JointOriginDef<P>) -> Option<MountSpec> {
    if let Some(spec) = &def.mount {
        return Some(spec.clone());
    }
    let name = feature_name.strip_prefix(MOUNT_PREFIX)?;
    valid_mount_name(name).then(|| MountSpec::new(name, MountKind::Face))
}

impl FeatureInfo for JointOriginDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.geometry.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_geometry(ctx, &self.geometry)?;
        if let Some(o) = &self.frame_override {
            o.check()?;
        }
        if let Some(mount) = &self.mount {
            mount.check().map_err(|e| format!("mount: {e}"))?;
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for JointOriginDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let mut frame = joints::origin_frame(ctx, &self.geometry)?;
        if let Some(o) = &self.frame_override {
            frame = joints::apply_override(&frame, o)?;
        }
        let angle = match self.angle {
            Some(id) => ctx.param(id)?,
            None => 0.0,
        };
        let offset = match self.offset {
            Some(id) => ctx.param(id)?,
            None => 0.0,
        };
        let frame = frame.after(&joints::alignment(angle, offset, self.flip));
        let [origin, x_axis, y_axis, _] = joints::frame_parts(&frame);
        let datum = Datum::Plane(DatumPlane {
            origin,
            x_axis,
            y_axis,
        });
        Ok(FeatureOutput {
            datum: Some(super::construction::finite(datum)?),
            ..FeatureOutput::default()
        })
    }
}

// rigid_group

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigidGroupDef<P = ParamId> {
    /// Occurrences placed in the feature's component.
    pub occurrences: Vec<OccurrenceUid>,
    #[serde(skip)]
    pub marker: PhantomData<P>,
}

impl<P> RigidGroupDef<P> {
    pub const TYPE: &'static str = "rigid_group";
    pub const BASE_NAME: &'static str = "RigidGroup";

    pub fn new(occurrences: Vec<OccurrenceUid>) -> Self {
        Self {
            occurrences,
            marker: PhantomData,
        }
    }

    map_params! {
        RigidGroupDef {
            occurrences: clone,
            marker: phantom,
        }
    }
}

impl FeatureInfo for RigidGroupDef {
    fn references(&self) -> References {
        References::default()
    }

    fn check(&self, _ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.occurrences.len() < 2 {
            return Err("a rigid group has at least two occurrences".to_owned());
        }
        for (i, o) in self.occurrences.iter().enumerate() {
            if self.occurrences[..i].contains(o) {
                return Err(format!("occurrence {o} is listed more than once"));
            }
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for RigidGroupDef {
    fn evaluate(&self, _ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        Ok(FeatureOutput::default())
    }
}
