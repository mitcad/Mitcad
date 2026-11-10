// SPDX-License-Identifier: MIT
//! Bound length (mitcad#375, `commands.md`, "BUILD: profile assemblies"):
//! a length measured along a ray, set into the feature's own reference
//! parameter, which a derived part after it takes as its length. The ray
//! starts at `from`'s frame (geometry of an occurrence, as a joint's
//! origin, taken into the feature's component through the placements at
//! this point of the timeline), shifted along that frame's axes by
//! `shift`, and runs along its `direction` to `to`: a planar face, or a
//! frame (the plane through its origin across its z axis). With `mode`
//! `face` the length is the distance to it, `centre` to the middle of the
//! part it belongs to, `through` to that part's far side; `add` is added.
//!
//! A ray that meets nothing, or a target that is gone, fails the feature,
//! and the parameter keeps its last value.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Resolver};
use super::joint::{JointOrigin, OccurrencePath};
use super::pattern::none;
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::datum::{Datum, SurfaceGeometry};
use crate::expr::ParamKind;
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::{ParamId, Parameters};
use crate::transform::Transform;

/// An axis of the start frame the start is shifted along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShiftAxis {
    X,
    Y,
    Z,
}

impl ShiftAxis {
    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}

/// An axis of the start frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamedAxis {
    #[serde(rename = "x")]
    X,
    #[serde(rename = "-x")]
    MinusX,
    #[serde(rename = "y")]
    Y,
    #[serde(rename = "-y")]
    MinusY,
    #[serde(rename = "z")]
    Z,
    #[serde(rename = "-z")]
    MinusZ,
}

/// The ray's direction in the start frame: one of its axes, or a vector
/// in its coordinates (a part placed at an angle to the frame).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RayDirection {
    Axis(NamedAxis),
    Vector([f64; 3]),
}

impl Default for RayDirection {
    fn default() -> Self {
        Self::Axis(NamedAxis::Z)
    }
}

impl RayDirection {
    pub fn vector(self) -> [f64; 3] {
        match self {
            Self::Axis(NamedAxis::X) => [1.0, 0.0, 0.0],
            Self::Axis(NamedAxis::MinusX) => [-1.0, 0.0, 0.0],
            Self::Axis(NamedAxis::Y) => [0.0, 1.0, 0.0],
            Self::Axis(NamedAxis::MinusY) => [0.0, -1.0, 0.0],
            Self::Axis(NamedAxis::Z) => [0.0, 0.0, 1.0],
            Self::Axis(NamedAxis::MinusZ) => [0.0, 0.0, -1.0],
            Self::Vector(v) => v,
        }
    }

    fn is_z(&self) -> bool {
        *self == Self::Axis(NamedAxis::Z)
    }
}

/// Where the length ends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundMode {
    /// At the face (or the frame's plane).
    #[default]
    Face,
    /// At the middle of the part the face belongs to.
    Centre,
    /// At that part's far side.
    Through,
}

impl BoundMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Face => "face",
            Self::Centre => "centre",
            Self::Through => "through",
        }
    }

    fn is_face(&self) -> bool {
        *self == Self::Face
    }
}

/// The ray's start: geometry of an occurrence and a shift along its
/// frame's axes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundFrom<P = ParamId> {
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub occurrence: OccurrencePath,
    pub geometry: GeomRef,
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub shift: BTreeMap<ShiftAxis, P>,
}

/// What the ray ends on: a planar face or a frame of an occurrence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundTo {
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub occurrence: OccurrencePath,
    pub geometry: GeomRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundLengthDef<P = ParamId> {
    /// The feature's own reference parameter, set to the length.
    pub parameter: P,
    pub from: BoundFrom<P>,
    #[serde(default, skip_serializing_if = "RayDirection::is_z")]
    pub direction: RayDirection,
    pub to: BoundTo,
    #[serde(default, skip_serializing_if = "BoundMode::is_face")]
    pub mode: BoundMode,
    /// Added to the length: an overlap or a gap (may be negative).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub add: Option<P>,
}

impl<P> BoundLengthDef<P> {
    pub const TYPE: &'static str = "bound_length";
    pub const BASE_NAME: &'static str = "BoundLength";

    // Written by hand: the shift's slots are named by axis (`from.shift.x`).
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<BoundLengthDef<Q>, E> {
        let mut shift = BTreeMap::new();
        for (axis, value) in &self.from.shift {
            let key = match axis {
                ShiftAxis::X => "x",
                ShiftAxis::Y => "y",
                ShiftAxis::Z => "z",
            };
            shift.insert(*axis, f(&format!("from.shift.{key}"), value)?);
        }
        Ok(BoundLengthDef {
            parameter: f("parameter", &self.parameter)?,
            from: BoundFrom {
                occurrence: self.from.occurrence.clone(),
                geometry: self.from.geometry.clone(),
                shift,
            },
            direction: self.direction,
            to: self.to.clone(),
            mode: self.mode,
            add: match &self.add {
                Some(add) => Some(f("add", add)?),
                None => None,
            },
        })
    }
}

impl BoundLengthDef {
    /// The start and the target as joint origins, for the checks of
    /// joints.
    pub(crate) fn origins(&self) -> [JointOrigin; 2] {
        [
            JointOrigin {
                occurrence: self.from.occurrence.clone(),
                geometry: self.from.geometry.clone(),
                frame_override: None,
            },
            JointOrigin {
                occurrence: self.to.occurrence.clone(),
                geometry: self.to.geometry.clone(),
                frame_override: None,
            },
        ]
    }
}

/// A bound length's parameter is its own reference: the number a new one
/// is given makes a dimension of the feature, which becomes the reference.
pub(crate) fn settle_parameter(
    params: &mut Parameters,
    id: ParamId,
    owner: FeatureUid,
) -> Result<(), String> {
    if params.owner(id) == Some(owner) {
        params.set_kind(id, ParamKind::Reference);
        Ok(())
    } else {
        Err(format!(
            "parameter: a bound length measures a parameter of its own; give its first value \
             as a number, not “{}”",
            params.name(id)
        ))
    }
}

impl FeatureInfo for BoundLengthDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.from.geometry.add_to(&mut references);
        self.to.geometry.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        let [from, to] = self.origins();
        from.check(ctx).map_err(|e| format!("from: {e}"))?;
        match &self.to.geometry {
            GeomRef::Face { .. } | GeomRef::Mount { .. } => {}
            other => {
                return Err(format!(
                    "to: {}",
                    other.error(" is no planar face or frame")
                ));
            }
        }
        to.check(ctx).map_err(|e| format!("to: {e}"))?;
        let v = self.direction.vector();
        if !v.iter().all(|c| c.is_finite()) || crate::build::unit(v).is_none() {
            return Err("the direction is zero".to_owned());
        }
        Ok(())
    }
}

/// Resolves geometry of another component through an evaluation: its
/// bodies at this point of the timeline.
struct InComponent<'c, 'a, K: Kernel> {
    ctx: &'c mut EvalContext<'a, K>,
    component: ComponentUid,
}

impl<K: Kernel> Resolver for InComponent<'_, '_, K> {
    type Kernel = K;

    fn kernel(&self) -> &K {
        self.ctx.kernel
    }

    fn name(&self, uid: FeatureUid) -> String {
        self.ctx.feature_name(uid)
    }

    fn datum(&mut self, uid: FeatureUid) -> Result<Datum, String> {
        self.ctx.datum(uid)
    }

    fn shape(&mut self, body: BodyUid) -> Result<K::Shape, String> {
        self.ctx.body_in(self.component, body)
    }

    fn sketch(&mut self, uid: FeatureUid) -> Result<Arc<super::SketchOutput>, String> {
        self.ctx.sketch(uid)
    }

    fn mount(&mut self, feature: FeatureUid, name: &str) -> Result<Transform, String> {
        self.ctx.mount(feature, name)
    }
}

/// The component an occurrence path from the feature's component ends in,
/// and the transform from its coordinates into the feature's.
fn path_placement<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    path: &OccurrencePath,
) -> Result<(ComponentUid, Transform), String> {
    let component = ctx.component();
    let at = crate::joints::path_component(ctx.env.assembly, component, &path.0)?;
    let mut placement = Transform::IDENTITY;
    for o in &path.0 {
        placement = placement.after(&ctx.placement(*o));
    }
    Ok((at, placement))
}

/// Measures a bound length against its exact target. Shared by committed
/// evaluation and read-only BUILD planning; all coordinates are in the
/// caller's component, with target bodies supplied in their own coordinates.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn measure_to<R: Resolver>(
    resolver: &mut R,
    start: [f64; 3],
    direction: [f64; 3],
    geometry: &GeomRef,
    placement: Transform,
    available_bodies: &[BodyUid],
    mode: BoundMode,
    add: f64,
) -> Result<f64, String> {
    use crate::build::{box_along, ray_plane, unit};
    let (point, normal, bodies): (_, _, Vec<BodyUid>) = match geometry {
        GeomRef::Mount { feature, name } => {
            let frame = placement.after(
                &resolver
                    .mount(*feature, name)
                    .map_err(|e| format!("to: {e}"))?,
            );
            let bodies = available_bodies
                .iter()
                .copied()
                .filter(|uid| uid.feature == *feature)
                .collect();
            (frame.translation, crate::build::z_axis(&frame), bodies)
        }
        GeomRef::Face { body, face } => {
            let shape = resolver.shape(*body).map_err(|e| format!("to: {e}"))?;
            match resolver.kernel().face_geometry(&shape, face) {
                Ok(SurfaceGeometry::Plane { origin, normal }) => (
                    placement.apply_point(origin),
                    placement.apply_vector(normal),
                    vec![*body],
                ),
                Ok(other) => {
                    return Err(crate::notice::placed(
                        "to: the face is not planar",
                        crate::notice::NoticePlace::face(face.to_string())
                            .in_body(&body.to_string()),
                        format!("to: face {face} is not planar ({})", other.kind()),
                    ));
                }
                Err(e) => {
                    return Err(format!(
                        "to: {}",
                        crate::notice::about_place(
                            crate::notice::NoticePlace::face(face.to_string())
                                .in_body(&body.to_string()),
                            &e.to_string(),
                        )
                    ));
                }
            }
        }
        other => {
            return Err(format!(
                "to: {}",
                other.error(" is no planar face or frame")
            ));
        }
    };
    let normal = unit(normal).ok_or("to: the plane has no normal")?;
    let distance = ray_plane(start, direction, point, normal)
        .ok_or_else(|| format!("the ray runs along {} and meets nothing", geometry))?;
    if distance < -1e-9 {
        return Err(format!(
            "the ray meets nothing: {} is behind its start",
            geometry
        ));
    }
    let length = match mode {
        BoundMode::Face => distance,
        BoundMode::Centre | BoundMode::Through => {
            let mut far = f64::NEG_INFINITY;
            for body in &bodies {
                let shape = resolver.shape(*body).map_err(|e| format!("to: {e}"))?;
                if let Ok(Some(bounds)) = resolver.kernel().bounding_box(&shape) {
                    far = far.max(box_along(&bounds, &placement, start, direction).1);
                }
            }
            if !far.is_finite() || far < distance {
                return Err(format!(
                    "to: the far side of the part of {} is not known",
                    geometry
                ));
            }
            if mode == BoundMode::Through {
                far
            } else {
                (distance + far) / 2.0
            }
        }
    };
    let value = length + add;
    if value.is_nan() || value <= 0.0 {
        return Err(format!(
            "the length would be {} mm: the target is at the start",
            crate::build::round(value)
        ));
    }
    Ok(value)
}

impl<K: Kernel> Evaluate<K> for BoundLengthDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        use crate::build::unit;
        // The start frame in the feature's component.
        let (at, placement) = path_placement(ctx, &self.from.occurrence)?;
        let local = {
            let mut resolver = InComponent {
                ctx: &mut *ctx,
                component: at,
            };
            crate::joints::origin_frame(&mut resolver, &self.from.geometry)
                .map_err(|e| format!("from: {e}"))?
        };
        let start_frame = placement.after(&local);
        let mut shift = [0.0; 3];
        for (axis, id) in &self.from.shift {
            shift[axis.index()] = ctx.param(*id)?;
        }
        let start = start_frame.apply_point(shift);
        let direction = unit(start_frame.apply_vector(self.direction.vector()))
            .ok_or("the direction is zero")?;
        // The plane the ray ends on, and the bodies of the part it is of.
        let (at, placement) = path_placement(ctx, &self.to.occurrence)?;
        let bodies = if matches!(self.to.geometry, GeomRef::Mount { .. }) {
            ctx.bodies_in(at)
                .into_iter()
                .map(|(uid, _)| uid)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let add = match self.add {
            Some(id) => ctx.param(id)?,
            None => 0.0,
        };
        let mut resolver = InComponent {
            ctx: &mut *ctx,
            component: at,
        };
        let value = measure_to(
            &mut resolver,
            start,
            direction,
            &self.to.geometry,
            placement,
            &bodies,
            self.mode,
            add,
        )?;
        Ok(FeatureOutput {
            measured: vec![(self.parameter, value)],
            ..FeatureOutput::default()
        })
    }
}
