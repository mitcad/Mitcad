// SPDX-License-Identifier: MIT
//! A profile tilted on a side (mitcad#444; `commands.md`, "BUILD: profile
//! assemblies", *Tilt*): a brace leaning along its host's length, in the
//! plane of the host's axis and the side's normal, positive towards the
//! host's end B. Its foot is cut by the tilt's angle about the axis across
//! the slot so that it lies flat on the side, at its long point (the
//! derived part's rule, mitcad#443); the foot's long point is where the
//! joint's slide puts it on the slot. With a bound length its far end is
//! cut flush with the plane the length ends on, and the length is measured
//! to that end's long point.
//!
//! The tilt is the joint's frame on the part (joint side a) turned and
//! moved to the foot's long point: the slot's frame and the slide stay as
//! they are, so the part still slides along the slot.

use crate::LengthInput;
use crate::geometry::{V3, X, Y, Z, dot, inverse, parts, scale, sub, unit};
use crate::parts::PartInfo;
use crate::place::{refuse, value_in};
use crate::prelude::*;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::expr::Unit;
use mitcad_model::features::bound_length::BoundTo;
use mitcad_model::features::derived_part::{EndCut, Tilt};
use mitcad_model::features::{FeatureDef, FrameOverride, GeomRef, OccurrencePath, ValueInput};
use mitcad_model::internal::features::{end_plane, evaluate_source};
use mitcad_model::joints::JointKind;
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::transform::Transform;
use mitcad_model::{Document, ModelError};

/// The steepest tilt, degrees either way.
pub(crate) const MOST_DEGREES: f64 = 80.0;

/// Refuses a tilt beyond [`MOST_DEGREES`].
pub(crate) fn check_tilt(tilt: f64) -> Result<(), String> {
    if !tilt.is_finite() || tilt.abs() > MOST_DEGREES.to_radians() + 1e-9 {
        return Err(format!(
            "the tilt is {} degrees; it is from -{MOST_DEGREES} up to {MOST_DEGREES}",
            mitcad_model::internal::build::round(tilt.to_degrees())
        ));
    }
    Ok(())
}

/// Whether a tilt is one (not 0).
pub(crate) fn is_tilted(tilt: Option<f64>) -> bool {
    tilt.is_some_and(|t| t.abs() > 1e-12)
}

/// A part tilted on a slot, worked out from its section's box and its
/// end frame that stands on the slot.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TiltPlan {
    /// The end frame on the slot turned and moved to the foot's long point
    /// (the override of the joint's side a).
    pub own: FrameOverride,
    /// The foot is end B (a flipped part).
    pub foot_b: bool,
    /// The foot's cut: its angle and tilt.
    pub foot: (f64, Tilt),
    /// The foot's reach along the slot from its long point: the least and
    /// the most.
    pub reach: (f64, f64),
    /// From the part's coordinates into the slot frame's, the foot's long
    /// point at the slot's origin.
    pub to_slot: Transform,
}

/// Plans a tilt `tilt` (radians, positive towards the slot's +x, the
/// host's end B) of a part whose section's box is `section`, standing on
/// the slot with its end frame `own` (end B when `foot_b`) turned by
/// `rotate` about its axis; `length` is the part's length now. None when
/// the slot's x is not across the part's section (no tilt can be cut).
pub(crate) fn tilt_plan(
    section: &BoundingBox,
    own: &Transform,
    rotate: f64,
    foot_b: bool,
    length: f64,
    tilt: f64,
) -> Option<TiltPlan> {
    let align = mitcad_model::internal::joints::alignment(rotate, 0.0, true);
    // Square: from the part into the slot's frame at the slide.
    let square = align.after(&inverse(own));
    let along = inverse(&square).apply_vector(X);
    let angle = tilt.abs();
    let lean = Tilt::towards(scale(along, tilt.signum()))?;
    let (long_point, _) = end_plane(Some(section), foot_b, length, angle, lean)?;
    let at = square.apply_point(long_point);
    let turn = Transform::rotation([0.0; 3], Y, tilt)?;
    let to_slot = turn
        .after(&Transform::translation([-at[0], 0.0, -at[2]]))
        .after(&square);
    // The end frame that gives that placement through the joint: the
    // part is at `frame_b · align · own⁻¹`.
    let turned = inverse(&to_slot).after(&align);
    let [origin, x, _, z] = parts(&turned);
    let axes = mitcad_model::internal::joints::frame(own.translation, x, z);
    let shift = inverse(&axes).apply_point(origin);
    let k = if lean.lean()[0] != 0.0 { 0 } else { 1 };
    let foot = (section.max[k] - section.min[k]) / angle.cos();
    Some(TiltPlan {
        own: FrameOverride {
            origin: None,
            z_axis: Some(z),
            x_axis: Some(x),
            shift: Some(shift),
            relative: None,
        },
        foot_b,
        foot: (angle, lean),
        reach: if tilt > 0.0 {
            (0.0, foot)
        } else {
            (-foot, 0.0)
        },
        to_slot,
    })
}

/// The far end of a tilted part with a bound length: its cut flush with
/// the plane the length ends on (None: square to it), and the ray its
/// length is measured along, in the slot frame's coordinates with the
/// foot's long point at the origin: from that end's long side where the
/// foot's end plane is, along the part's axis.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FarEnd {
    pub cut: Option<(f64, Tilt)>,
    pub start: V3,
    pub along: V3,
}

/// The far end of `plan`'s part flush with a plane of the normal `normal`
/// (slot coordinates, facing the part).
pub(crate) fn far_end(
    plan: &TiltPlan,
    section: &BoundingBox,
    length: f64,
    normal: V3,
) -> Result<FarEnd, String> {
    let far_b = !plan.foot_b;
    let axis = if far_b { Z } else { scale(Z, -1.0) };
    let from_slot = inverse(&plan.to_slot);
    let foot_z = if plan.foot_b { length } else { 0.0 };
    let along = plan.to_slot.apply_vector(axis);
    let square = FarEnd {
        cut: None,
        start: plan.to_slot.apply_point([0.0, 0.0, foot_z]),
        along,
    };
    let Some(out) = unit(from_slot.apply_vector(scale(normal, -1.0))) else {
        return Ok(square);
    };
    let cos = dot(out, axis);
    if cos <= 1e-9 {
        return Err("the plane the length ends on runs along the part".to_owned());
    }
    let angle = cos.min(1.0).acos();
    if angle < 1e-9 {
        return Ok(square);
    }
    if angle >= 89f64.to_radians() {
        return Err(format!(
            "the far end would be cut by {} degrees; the most is 89",
            mitcad_model::internal::build::round(angle.to_degrees())
        ));
    }
    let lean = sub(out, scale(axis, cos));
    let Some(tilt) = Tilt::towards(lean) else {
        return Ok(square);
    };
    let (point, _) = end_plane(Some(section), far_b, length, angle, tilt)
        .ok_or("the far end's cut has no normal")?;
    Ok(FarEnd {
        cut: Some((angle, tilt)),
        start: plan.to_slot.apply_point([point[0], point[1], foot_z]),
        along,
    })
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait BoundNormalCrateOps<K: Kernel> {
    /// The normal of the plane a bound length ends on, in the BUILD host's
    /// coordinates: a frame's z, or a planar face's normal.
    fn bound_normal(&self, to: &BoundTo) -> Option<V3>;
}

impl<K: Kernel> BoundNormalCrateOps<K> for Document<K> {
    fn bound_normal(&self, to: &BoundTo) -> Option<V3> {
        let path = &to.occurrence.0;
        let placement = self.path_transform(path);
        match &to.geometry {
            GeomRef::Mount { feature, name } => {
                let component = mitcad_model::internal::joints::path_component(
                    self.assembly(),
                    self.build_host(),
                    path,
                )
                .ok()?;
                let info = self.part_info(component)?;
                if info.part != Some(*feature) {
                    return None;
                }
                Some(crate::geometry::z_axis(&placement.after(info.mount(name)?)))
            }
            GeomRef::Face { body, face } => {
                let shape = self.body_shape(*body)?;
                match self.kernel().face_geometry(shape, face).ok()? {
                    SurfaceGeometry::Plane { normal, .. } => Some(placement.apply_vector(normal)),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// What `build_edit` sets for a part's tilt (mitcad#444): the override of
/// its joint's side a, its end cuts (end A, end B), and its bound length's
/// ray (start and direction in the slot's coordinates from the slide).
#[derive(Debug, Clone)]
pub(crate) struct Retilt {
    pub tilt: f64,
    pub own: Option<FrameOverride>,
    pub ends: [Option<EndCut<ValueInput>>; 2],
    pub ray: (V3, V3),
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait RetiltCrateOps<K: Kernel> {
    /// Plans a part's tilt `tilt` for `build_edit`, with its turn, flip and
    /// length as the edit leaves them (`rotate`, `flip`, `length`; None:
    /// as they are).
    fn plan_retilt(
        &self,
        info: &PartInfo,
        tilt: f64,
        rotate: Option<f64>,
        flip: Option<bool>,
        length: Option<&LengthInput>,
    ) -> Result<Retilt, ModelError>;
}

impl<K: Kernel> RetiltCrateOps<K> for Document<K> {
    fn plan_retilt(
        &self,
        info: &PartInfo,
        tilt: f64,
        rotate: Option<f64>,
        flip: Option<bool>,
        length: Option<&LengthInput>,
    ) -> Result<Retilt, ModelError> {
        let not_on_side = || refuse(format!("{} is not on a side: it cannot tilt", info.name));
        let joint = info.record.joint.ok_or_else(not_on_side)?;
        let Some(FeatureDef::Joint(j)) = self.feature(joint).map(|e| &e.def) else {
            return Err(not_on_side());
        };
        let (GeomRef::Mount { name: own_name, .. }, GeomRef::Mount { name: slot, .. }) =
            (&j.a.geometry, &j.b.geometry)
        else {
            return Err(not_on_side());
        };
        let target = self
            .target(&j.b.occurrence.0)
            // On a slot, or centred on a side between its slots
            // (mitcad#2002).
            .filter(|t| {
                j.kind == JointKind::Slider
                    && matches!(t.info.frame_kind(slot).as_str(), "slot" | "side")
            })
            .ok_or_else(not_on_side)?;
        if own_name != "end_a" && own_name != "end_b" {
            return Err(not_on_side());
        }
        let foot_b = flip.unwrap_or(own_name == "end_b");
        let square = Retilt {
            tilt: 0.0,
            own: None,
            ends: [None, None],
            ray: ([0.0; 3], Z),
        };
        if tilt.abs() <= 1e-12 {
            return Ok(square);
        }
        // The part's section and frames at the length it gets.
        let (Some(source), Some(def), Some(meta)) =
            (&info.source, &info.def, info.meta.length.as_ref())
        else {
            return Err(refuse(format!(
                "{} is not cut to length: it cannot tilt",
                info.name
            )));
        };
        let mut values = crate::parts::part_values(def, self.parameters());
        let mut length_now = info.length.map_or(f64::NAN, |(_, l)| l);
        if let Some(LengthInput::Value(value)) = length {
            length_now = value_in(self, value, Unit::MM)?;
        }
        values.insert(meta.parameter.clone(), length_now);
        let values: Vec<(String, f64)> = values.into_iter().collect();
        let label = source.label();
        let state = crate::parts::source_state(source, def.config.as_deref(), &values)
            .map_err(|e| refuse(format!("{label}: {e}")))?;
        let evaluated =
            evaluate_source(self.kernel(), &state).map_err(|e| refuse(format!("{label}: {e}")))?;
        let section = crate::place::bounds_of(self.kernel(), &evaluated.bodies)
            .ok_or_else(|| refuse(format!("{label} has no box to tilt")))?;
        let foot_name = if foot_b { "end_b" } else { "end_a" };
        let own = evaluated
            .mounts
            .get(foot_name)
            .copied()
            .ok_or_else(|| refuse(format!("{label} has no frame {foot_name}")))?;
        let rotate = rotate.unwrap_or_else(|| {
            j.angle
                .and_then(|id| self.parameters().value(id))
                .unwrap_or(0.0)
        });
        let plan = tilt_plan(&section, &own, rotate, foot_b, length_now, tilt)
            .ok_or_else(|| refuse(format!("{} cannot tilt along the slot", info.name)))?;
        // The far end: flush with the plane a bound length ends on.
        let to = match length {
            Some(LengthInput::Bound { bind, .. }) => Some(BoundTo {
                occurrence: OccurrencePath(self.path_from_host(&bind.occurrence)?),
                geometry: bind.geometry.clone(),
            }),
            Some(LengthInput::Value(_) | LengthInput::Match { .. }) => None,
            None => info
                .record
                .bound
                .iter()
                .find_map(|b| match &self.feature(*b)?.def {
                    FeatureDef::BoundLength(d) => Some(d.to.clone()),
                    _ => None,
                }),
        };
        let slot_frame = target
            .frame(slot)
            .ok_or_else(|| refuse(format!("{} has no frame {slot}", target.info.name)))?;
        let cut = |(angle, tilt): (f64, Tilt)| {
            Some(EndCut {
                angle: ValueInput::Number(angle),
                tilt: Some(tilt),
            })
        };
        let mut ends = [None, None];
        ends[usize::from(foot_b)] = cut(plan.foot);
        let mut ray = (
            [0.0; 3],
            plan.to_slot
                .apply_vector(if foot_b { scale(Z, -1.0) } else { Z }),
        );
        if let Some(to) = &to {
            let normal = self
                .bound_normal(to)
                .map(|n| inverse(&slot_frame).apply_vector(n))
                .ok_or_else(|| refuse("the length's target is no plane"))?;
            let end = far_end(&plan, &section, length_now, normal)
                .map_err(|e| refuse(format!("{}: {e}", info.name)))?;
            if let Some(far) = end.cut {
                ends[usize::from(!foot_b)] = cut(far);
            }
            ray = (end.start, end.along);
        }
        Ok(Retilt {
            tilt,
            own: Some(plan.own),
            ends,
            ray,
        })
    }
}
