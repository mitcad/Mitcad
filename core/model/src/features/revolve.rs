// SPDX-License-Identifier: MIT
//! Revolve: profiles of a sketch (or a planar face of a body, mitcad#539)
//! turned about an axis into a tool body, with the operations of an
//! extrusion.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::extrude::{
    Operation, ProfileRef, apply_operation, check_participants, participant_bodies,
    profile_references, profile_regions,
};
use super::geom_ref::{GeomRef, Want};
use super::path::SketchCurves;
use super::reference::{ExtentObject, project_axis};
use super::surface_sweeps;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse, is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{Axis, FaceRevolveSpec, Kernel, RevolveSpec, Target};
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;
use crate::profile::{ProfileRegion, SketchFrame};

const FULL_TURN: f64 = std::f64::consts::TAU;

/// How far the revolution turns; angles in radians, positive by the right
/// hand rule about the axis direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RevolveExtent<P = ParamId> {
    /// One side; a negative angle turns the other way, 2 pi is a full turn.
    Angle { angle: P },
    /// A full turn.
    Full,
    /// `angle` to each side (a symmetric revolve measures per side).
    Symmetric { angle: P },
    /// `angle1` the positive way, `angle2` the other; a negative `angle2`
    /// ends side two past the profile (mitcad#955): the turn runs from
    /// `-angle2` to `angle1`, the profile turned there first.
    TwoSides { angle1: P, angle2: P },
    /// The positive way up to an object: its first face reached, or
    /// through a body (`through`, mitcad#539) to its last face, where the
    /// turn leaves it.
    ToObject { object: Box<ExtentObject> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevolveDef<P = ParamId> {
    /// Regions of one sketch, a set (see `extrude::selected_regions`); none
    /// for a surface revolution of curves only.
    #[serde(
        default,
        deserialize_with = "super::extrude::profile_set",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub profiles: Vec<ProfileRef>,
    /// A sketch line, an origin or construction axis, a straight edge or a
    /// fixed axis.
    pub axis: GeomRef,
    /// Projects the axis into the sketch plane (`isProjectAxis` in .f3d designs).
    #[serde(default, skip_serializing_if = "is_false")]
    pub project_axis: bool,
    pub extent: RevolveExtent<P>,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// The body of another component that the `to_object` object is or is
    /// a face of (mitcad#762, as an extrusion's `object_links`): read in its
    /// component and moved into the revolution's coordinates by the
    /// placements at its point of the timeline.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub object_links: BTreeMap<BodyUid, OccurrenceLink>,
    /// A surface revolution (mitcad#455, `surface_sweeps.rs`): walls
    /// without caps, a new surface body.
    #[serde(default, skip_serializing_if = "is_false")]
    pub surface: bool,
    /// Sketch curves a surface revolution turns beside or instead of the
    /// profiles: open chains or closed loops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curves: Option<SketchCurves>,
    /// A planar face of a body as the profile instead of sketch regions
    /// (mitcad#539, as an extrusion's `profile_face`): its bounded area,
    /// holes kept, turned about the axis, found by its topological name on
    /// every recompute. By angles only (one side, full, symmetric, two
    /// sides); no `profiles`, `surface`, `curves` or `project_axis`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_face: Option<super::reference::FaceRef>,
}

impl<P> RevolveDef<P> {
    pub const TYPE: &'static str = "revolve";
    pub const BASE_NAME: &'static str = "Revolve";

    // Written by hand: the extent maps by its kind (`extent.angle1`).
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<RevolveDef<Q>, E> {
        let extent = match &self.extent {
            RevolveExtent::Angle { angle } => RevolveExtent::Angle {
                angle: f("extent.angle", angle)?,
            },
            RevolveExtent::Full => RevolveExtent::Full,
            RevolveExtent::Symmetric { angle } => RevolveExtent::Symmetric {
                angle: f("extent.angle", angle)?,
            },
            RevolveExtent::TwoSides { angle1, angle2 } => RevolveExtent::TwoSides {
                angle1: f("extent.angle1", angle1)?,
                angle2: f("extent.angle2", angle2)?,
            },
            RevolveExtent::ToObject { object } => RevolveExtent::ToObject {
                object: object.clone(),
            },
        };
        Ok(RevolveDef {
            profiles: self.profiles.clone(),
            axis: self.axis.clone(),
            project_axis: self.project_axis,
            extent,
            operation: self.operation,
            participants: self.participants.clone(),
            object_links: self.object_links.clone(),
            surface: self.surface,
            curves: self.curves.clone(),
            profile_face: self.profile_face.clone(),
        })
    }

    /// The sketch of the profiles (or of the curves).
    pub fn sketch(&self) -> Option<FeatureUid> {
        self.profiles
            .first()
            .map(|p| p.sketch)
            .or_else(|| self.curves.as_ref().map(|c| c.sketch))
    }
}

impl FeatureInfo for RevolveDef {
    fn references(&self) -> References {
        let mut references = References::default();
        profile_references(&self.profiles, &mut references);
        if let Some(curves) = &self.curves {
            curves.add_references(&mut references);
        }
        self.axis.add_to(&mut references);
        if let RevolveExtent::ToObject { object } = &self.extent {
            object.add_references(&mut references);
        }
        for body in &self.participants {
            references.body(*body);
        }
        // A body face as the profile (mitcad#539): its body and the
        // features its name refers to.
        if let Some(face) = &self.profile_face {
            face.add_references(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        // Links only for the object's body (mitcad#762, as an extrusion's).
        let object_body = match &self.extent {
            RevolveExtent::ToObject { object } => object.body(),
            _ => None,
        };
        if let Some(body) = self.object_links.keys().find(|b| Some(**b) != object_body) {
            return Err(format!("body {body} is linked but not an object's"));
        }
        if let Some(face) = &self.profile_face {
            check_face_profile(self, ctx, face)?;
            check_participants(ctx, self.operation, &self.participants)?;
            return self.axis.check(ctx, Want::Line);
        }
        surface_sweeps::check_inputs(
            ctx,
            self.surface,
            self.operation,
            &self.profiles,
            self.curves.as_ref(),
            "revolution",
        )?;
        check_participants(ctx, self.operation, &self.participants)?;
        self.axis.check(ctx, Want::Line)?;
        if let RevolveExtent::ToObject { object } = &self.extent {
            object.check(ctx)?;
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_angles(&self.extent, value)
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

fn check_angles(extent: &RevolveExtent, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
    let total = match extent {
        RevolveExtent::Angle { angle } => {
            let a = value(*angle);
            if a == 0.0 || !a.is_finite() {
                return Err(format!("the revolve angle must not be zero, got {a}"));
            }
            a.abs()
        }
        RevolveExtent::Symmetric { angle } => {
            let a = value(*angle);
            if a.is_nan() || a <= 0.0 {
                return Err(format!(
                    "the revolve angle must be greater than zero, got {a}"
                ));
            }
            2.0 * a
        }
        RevolveExtent::TwoSides { angle1, angle2 } => {
            let (a, b) = (value(*angle1), value(*angle2));
            // A negative second angle ends side two past the profile
            // (mitcad#955): the turn runs from -b to a.
            if !(a >= 0.0 && b.is_finite() && a + b > 0.0) {
                return Err(format!(
                    "the first revolve angle must not be negative and the two must add up to more than zero, got {a} and {b}"
                ));
            }
            a + b
        }
        RevolveExtent::Full | RevolveExtent::ToObject { .. } => 0.0,
    };
    if total > FULL_TURN + 1e-9 {
        return Err(format!(
            "the revolve angles add up to more than a full turn ({total} rad)"
        ));
    }
    Ok(())
}

impl<K: Kernel> Evaluate<K> for RevolveDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        if let Some(face) = &self.profile_face {
            // A body face as the profile (mitcad#539).
            let participants = participant_bodies(ctx, self.operation, &self.participants)?;
            let tool = self.face_tool(ctx, face)?;
            return apply_operation(ctx, self.operation, participants, tool, "revolution");
        }
        let (sketch, regions) = match (&self.curves, self.profiles.is_empty()) {
            (Some(curves), true) => (ctx.sketch(curves.sketch)?, Vec::new()),
            _ => profile_regions(ctx, &self.profiles)?,
        };
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let line = ctx.axis(&self.axis)?;
        let mut axis = Axis {
            origin: line.origin,
            direction: line.direction,
        };
        if self.project_axis {
            axis = project_axis(axis, &sketch.frame)?;
        }
        let mut values = Vec::new();
        for id in self.params_of_extent() {
            values.push((id, ctx.param(id)?));
        }
        let value = |id: ParamId| {
            values
                .iter()
                .find(|(p, _)| *p == id)
                .map_or(f64::NAN, |(_, v)| *v)
        };
        check_angles(&self.extent, &value)?;
        let mut frame = sketch.frame;
        let (mut angle1, mut angle2, mut target) = (0.0, None, None);
        match &self.extent {
            RevolveExtent::Angle { angle } => {
                let a = value(*angle);
                if a < 0.0 {
                    axis.direction = axis.direction.map(|v| -v);
                }
                angle1 = a.abs().min(FULL_TURN);
            }
            RevolveExtent::Full => angle1 = FULL_TURN,
            RevolveExtent::Symmetric { angle } => {
                angle1 = value(*angle);
                angle2 = Some(angle1);
            }
            RevolveExtent::TwoSides {
                angle1: one,
                angle2: two,
            } => {
                angle1 = value(*one);
                angle2 = Some(value(*two));
                // A side of nothing leaves one side.
                if angle1 == 0.0 {
                    axis.direction = axis.direction.map(|v| -v);
                    angle1 = value(*two);
                    angle2 = None;
                } else if value(*two) == 0.0 {
                    angle2 = None;
                } else if value(*two) < 0.0 {
                    // Side two ending past the profile (mitcad#955): the
                    // profile turned to where the turn starts, then one
                    // side.
                    frame = turned_frame(frame, &axis, -value(*two))?;
                    angle1 += value(*two);
                    angle2 = None;
                }
            }
            RevolveExtent::ToObject { object } => {
                target = Some(object.resolve_linked(ctx, &self.object_links)?);
            }
        }
        if self.surface {
            return self.surface_output(ctx, frame, &regions, axis, angle1, angle2, target);
        }
        let spec = RevolveSpec {
            feature: ctx.uid,
            frame,
            regions: &regions,
            axis,
            angle1,
            angle2,
            target,
        };
        let tool = ctx.kernel.revolve(&spec).map_err(|e| e.to_string())?;
        apply_operation(ctx, self.operation, participants, tool, "revolution")
    }
}

impl RevolveDef {
    /// A surface revolution's bodies (mitcad#455): the profiles' walls (the
    /// solid revolution without its caps), then the curves'.
    #[allow(clippy::too_many_arguments)]
    fn surface_output<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        frame: SketchFrame,
        regions: &[ProfileRegion],
        axis: Axis,
        angle1: f64,
        angle2: Option<f64>,
        target: Option<Target<K::Shape>>,
    ) -> Result<FeatureOutput<K::Shape>, String> {
        let chains = match &self.curves {
            Some(_) if target.is_some() => {
                return Err(
                    "a surface revolution of curves turns by angles, not up to an object"
                        .to_owned(),
                );
            }
            Some(curves) => Some(surface_sweeps::chains(ctx, curves, &frame)?),
            None => None,
        };
        let mut spec = RevolveSpec {
            feature: ctx.uid,
            frame,
            regions,
            axis,
            angle1,
            angle2,
            target,
        };
        let mut sheets = Vec::new();
        if !regions.is_empty() {
            let tool = ctx.kernel.revolve(&spec).map_err(|e| e.to_string())?;
            sheets.push(surface_sweeps::walls(ctx, &tool)?);
        }
        if let Some(chains) = &chains {
            spec.regions = &[];
            sheets.push(
                ctx.kernel
                    .revolve_sheet(&spec, chains)
                    .map_err(|e| e.to_string())?,
            );
        }
        surface_sweeps::output(ctx, sheets)
    }
}

impl RevolveDef {
    fn params_of_extent(&self) -> Vec<ParamId> {
        match &self.extent {
            RevolveExtent::Angle { angle } | RevolveExtent::Symmetric { angle } => vec![*angle],
            RevolveExtent::TwoSides { angle1, angle2 } => vec![*angle1, *angle2],
            RevolveExtent::Full | RevolveExtent::ToObject { .. } => Vec::new(),
        }
    }
}

// A planar body face as the profile (mitcad#539).

/// How a revolution of a body face turns.
const FACE_EXTENTS: &str =
    "a revolution of a body face turns by angles (one side, full, symmetric or two sides)";

/// Checks a revolution whose profile is a body face: nothing a sketch
/// profile has besides it, angles only, the face's body and the features
/// its name refers to before the feature.
fn check_face_profile(
    def: &RevolveDef,
    ctx: &CheckContext<'_>,
    face: &super::reference::FaceRef,
) -> Result<(), String> {
    if !def.profiles.is_empty() {
        return Err("a revolution of a body face takes no sketch profiles".to_owned());
    }
    if def.surface || def.curves.is_some() {
        return Err("a revolution of a body face has no surface or curves".to_owned());
    }
    if def.project_axis {
        return Err("a revolution of a body face does not project its axis".to_owned());
    }
    if matches!(def.extent, RevolveExtent::ToObject { .. }) {
        return Err(FACE_EXTENTS.to_owned());
    }
    face.check(ctx)
}

impl RevolveDef {
    /// The tool of a revolution whose profile is a body face: the face as
    /// found on its body now (a face a sketch change renamed is found
    /// again, with a warning), turned by the kernel, its faces named after
    /// the feature. The angles as a revolution of sketch profiles takes
    /// them: a negative single angle turns the other way, a side of
    /// nothing leaves one side. A face that is gone, not planar or of a
    /// mesh body fails the feature; no other profile is taken instead.
    fn face_tool<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        face: &super::reference::FaceRef,
    ) -> Result<K::Shape, String> {
        let body = ctx.surface_body(face.body)?;
        let found = ctx.face_on(face.body, &body, &face.face)?;
        let line = ctx.axis(&self.axis)?;
        let mut axis = Axis {
            origin: line.origin,
            direction: line.direction,
        };
        let mut values = Vec::new();
        for id in self.params_of_extent() {
            values.push((id, ctx.param(id)?));
        }
        let value = |id: ParamId| {
            values
                .iter()
                .find(|(p, _)| *p == id)
                .map_or(f64::NAN, |(_, v)| *v)
        };
        check_angles(&self.extent, &value)?;
        let reverse = |axis: &mut Axis| axis.direction = axis.direction.map(|v| -v);
        let (angle1, angle2) = match &self.extent {
            RevolveExtent::Angle { angle } => {
                let a = value(*angle);
                if a < 0.0 {
                    reverse(&mut axis);
                }
                (a.abs().min(FULL_TURN), None)
            }
            RevolveExtent::Full => (FULL_TURN, None),
            RevolveExtent::Symmetric { angle } => (value(*angle), Some(value(*angle))),
            RevolveExtent::TwoSides { angle1, angle2 } => {
                let (a, b) = (value(*angle1), value(*angle2));
                if b < 0.0 {
                    return Err(
                        "a revolution of a body face turns from the face: the second angle must not be negative"
                            .to_owned(),
                    );
                }
                if a == 0.0 {
                    reverse(&mut axis);
                    (b, None)
                } else if b == 0.0 {
                    (a, None)
                } else {
                    (a, Some(b))
                }
            }
            RevolveExtent::ToObject { .. } => return Err(FACE_EXTENTS.to_owned()),
        };
        let spec = FaceRevolveSpec {
            feature: ctx.uid,
            body: &body,
            face: &found,
            key: &face.face,
            axis,
            angle1,
            angle2,
        };
        ctx.kernel
            .revolve_face(&spec)
            .map_err(super::face_refs::kernel_error)
    }
}

/// The sketch frame turned by `angle` about the axis (right-handed): a
/// turn that starts past the profile (two sides with a negative second
/// angle, mitcad#955) turns the profile there first.
fn turned_frame(frame: SketchFrame, axis: &Axis, angle: f64) -> Result<SketchFrame, String> {
    use crate::datum::{add, rotate, sub, unit};
    let d = unit(axis.direction).ok_or("the revolve axis has no direction")?;
    Ok(SketchFrame {
        origin: add(
            axis.origin,
            rotate(sub(frame.origin, axis.origin), d, angle),
        ),
        x_axis: rotate(frame.x_axis, d, angle),
        y_axis: rotate(frame.y_axis, d, angle),
    })
}
