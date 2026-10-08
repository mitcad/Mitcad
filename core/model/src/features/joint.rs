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
use std::fmt;
use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::construction::datum_kind;
use super::geom_ref::{GeomRef, Want};
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

/// Occurrences from a component down, `"O1/O4"` in files and commands;
/// empty (`""`) for the component itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OccurrencePath(pub Vec<OccurrenceUid>);

impl OccurrencePath {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for OccurrencePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(ToString::to_string).collect();
        f.write_str(&parts.join("/"))
    }
}

impl Serialize for OccurrencePath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for OccurrencePath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        text.split('/')
            .map(|part| part.trim().parse::<OccurrenceUid>())
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
            .map_err(|_| {
                serde::de::Error::custom(format!(
                    "invalid occurrence path '{text}': occurrence ids like \"O1/O4\""
                ))
            })
    }
}

/// Parts of a joint origin's frame given instead of the resolved ones, in
/// the component's coordinates (mm, directions).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_axis: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_axis: Option<Vec3>,
}

impl FrameOverride {
    fn check(&self) -> Result<(), String> {
        let numbers: Vec<f64> = [self.origin, self.z_axis, self.x_axis]
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        if !numbers.iter().all(|v| v.is_finite()) {
            return Err("the frame override's numbers must be finite".to_owned());
        }
        if self.z_axis.is_some_and(|z| crate::datum::unit(z).is_none()) {
            return Err("the frame override's z axis is zero".to_owned());
        }
        if self.x_axis.is_some_and(|x| crate::datum::unit(x).is_none()) {
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
    fn add_references(&self, references: &mut References) {
        self.geometry.add_to(references);
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
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

fn map_option<P, Q, E>(
    slot: &str,
    value: &Option<P>,
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Option<Q>, E> {
    value.as_ref().map(|v| f(slot, v)).transpose()
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
}

impl<P> JointDef<P> {
    pub const TYPE: &'static str = "joint";
    pub const BASE_NAME: &'static str = "Joint";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<JointDef<Q>, E> {
        Ok(JointDef {
            kind: self.kind,
            slide_axis: self.slide_axis,
            a: self.a.clone(),
            b: self.b.clone(),
            offset: map_option("offset", &self.offset, f)?,
            angle: map_option("angle", &self.angle, f)?,
            flip: self.flip,
            limits: map_limits(&self.limits, f)?,
            position: map_position(&self.position, f)?,
        })
    }
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

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<AsBuiltJointDef<Q>, E> {
        Ok(AsBuiltJointDef {
            kind: self.kind,
            slide_axis: self.slide_axis,
            a: self.a.clone(),
            b: self.b.clone(),
            origin: self.origin.clone(),
            relative: self.relative,
            limits: map_limits(&self.limits, f)?,
            position: map_position(&self.position, f)?,
        })
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
}

impl<P> JointOriginDef<P> {
    pub const TYPE: &'static str = "joint_origin";
    pub const BASE_NAME: &'static str = "JointOrigin";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<JointOriginDef<Q>, E> {
        Ok(JointOriginDef {
            geometry: self.geometry.clone(),
            frame_override: self.frame_override,
            offset: map_option("offset", &self.offset, f)?,
            angle: map_option("angle", &self.angle, f)?,
            flip: self.flip,
        })
    }
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

    pub fn map_params<Q, E>(
        &self,
        _f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<RigidGroupDef<Q>, E> {
        Ok(RigidGroupDef::new(self.occurrences.clone()))
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
