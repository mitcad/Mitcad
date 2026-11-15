// SPDX-License-Identifier: MIT
//! Members along an edge (mitcad#1814; `commands.md`, "BUILD: timber
//! construction", *Members along an edge*): `build_place` with `along`
//! lays a member along a straight edge on a face beside it, its side flush
//! with the edge, its underside on the face, moved by four offsets. The
//! model's `edge_frame` keeps the edge, the face and the offsets; the
//! member's length is bound from its `start` frame to its `end` frame's
//! plane and its joint is on `start`, so that the member follows the body
//! the edge is on.

use std::collections::BTreeMap;
use std::f64::consts::FRAC_PI_2;

use serde::{Deserialize, Serialize};

use crate::geometry::inverse;
use crate::place::{
    Plan, RayPlan, add_bound, add_feature, make_part, move_before, refuse, value_in,
};
use crate::prelude::*;
use crate::timber::Lie;
use crate::{BuildItem, PlaceInput, Placed};
use mitcad_model::expr::Unit;
use mitcad_model::features::bound_length::{BoundMode, BoundTo, RayDirection};
use mitcad_model::features::derived_part::EndCut;
use mitcad_model::features::edge_frame::{
    END, EdgeFrameDef, EdgeOffsets, EdgeRun, START, edge_run,
};
use mitcad_model::features::{
    FeatureDef, FrameOverride, GeomRef, JointDef, JointOrigin, OccurrencePath, ValueInput,
};
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::joints::{JointKind, SlideAxis};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::transform::Transform;
use mitcad_model::{DocState, Document, ModelError};

/// Where a member along an edge goes (`build_place`'s `along`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlongEdge {
    /// A path from the BUILD host to the component the edge and the face
    /// are in (uids or names); empty for its own geometry.
    #[serde(default)]
    pub occurrence: String,
    /// A straight edge.
    pub edge: GeomRef,
    /// The planar face beside it that the member lies on: it decides which
    /// side of the edge the member is on.
    pub face: GeomRef,
    /// Across the face, away from the edge: 0 flush, negative out over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inset: Option<ValueInput>,
    /// Along the edge at its start: positive shortens, negative runs past.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<ValueInput>,
    /// Along the edge at its end, likewise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<ValueInput>,
    /// Off the face along its normal: 0 rests on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lift: Option<ValueInput>,
}

/// The joint's turn about the edge and the shift of the `start` frame
/// along its axes (across the face, up from it) that put a member's side
/// flush with the edge and its underside on the face, lying as `lie` says
/// (as members by two points lie): its section's smaller dimension up
/// when flat, the larger one on edge. The section is the member's box in
/// its own coordinates (its width along X, thickness along Y).
pub(crate) fn lie_on_edge(section: Option<&BoundingBox>, lie: Lie) -> (f64, [f64; 3]) {
    let Some(b) = section else {
        return (0.0, [0.0; 3]);
    };
    let (w, t) = (b.max[0] - b.min[0], b.max[1] - b.min[1]);
    let y_up = match lie {
        Lie::Flat => w >= t,
        Lie::Edge => w < t,
    };
    if y_up {
        // X across the face, Y up.
        (0.0, [-b.min[0], -b.min[1], 0.0])
    } else {
        // Turned a quarter about the edge: X up, Y across the face the
        // other way.
        (FRAC_PI_2, [b.max[1], -b.min[0], 0.0])
    }
}

/// Where a member along an edge goes in the BUILD host: its end A frame
/// on the edge frame's `start` (in the host's coordinates) as its joint
/// puts it.
pub(crate) fn along_placement(
    start: &Transform,
    mounts: &BTreeMap<String, Transform>,
    section: Option<&BoundingBox>,
    lie: Lie,
) -> Transform {
    let (angle, shift) = lie_on_edge(section, lie);
    let own = mounts.get("end_a").copied().unwrap_or(Transform::IDENTITY);
    start
        .after(&Transform::translation(shift))
        .after(&mitcad_model::internal::joints::alignment(angle, 0.0, true))
        .after(&inverse(&own))
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait AlongCrateOps<K: Kernel> {
    /// The edge frame `along` gives, now: the path to the edge's component
    /// and the frames in the BUILD host's coordinates.
    fn plan_along(&self, along: &AlongEdge) -> Result<(Vec<OccurrenceUid>, EdgeRun), ModelError>;

    /// Places a member along an edge (`build_place` with `along`).
    fn build_place_along(
        &mut self,
        input: &PlaceInput,
        along: &AlongEdge,
    ) -> Result<Placed, ModelError>;
}

impl<K: Kernel> AlongCrateOps<K> for Document<K> {
    fn plan_along(&self, along: &AlongEdge) -> Result<(Vec<OccurrenceUid>, EdgeRun), ModelError> {
        let path = self.path_from_host(&along.occurrence)?;
        let value = |v: &Option<ValueInput>| match v {
            Some(v) => value_in(self, v, Unit::MM),
            None => Ok(0.0),
        };
        let offsets = EdgeOffsets {
            inset: value(&along.inset)?,
            start: value(&along.start)?,
            end: value(&along.end)?,
            lift: value(&along.lift)?,
        };
        let local =
            edge_run(&mut self.resolver(), &along.edge, &along.face, &offsets).map_err(refuse)?;
        let placement = self.path_transform(&path);
        Ok((
            path,
            EdgeRun {
                start: placement.after(&local.start),
                end: placement.after(&local.end),
                ..local
            },
        ))
    }

    fn build_place_along(
        &mut self,
        input: &PlaceInput,
        along: &AlongEdge,
    ) -> Result<Placed, ModelError> {
        let plan = self.plan_place_between(input, None)?;
        let (path, run) = self.plan_along(along)?;
        let active = self.build_host();
        self.state().editable(active)?;
        let label = format!(
            "Place {}",
            if plan.designation.is_empty() {
                plan.source.def.label()
            } else {
                plan.designation.clone()
            }
        );
        let lie = input.lie.unwrap_or_default();
        self.apply(|state| {
            let placed = make_along(state, &plan, along, &path, run.length, lie, active)?;
            Ok((label, placed))
        })
    }
}

/// Refuses the fields a member along an edge does not take: the edge and
/// its offsets give its place and its length.
pub(crate) fn check_along(input: &PlaceInput) -> Result<(), ModelError> {
    let refused = if input.to.is_some() {
        Some("to")
    } else if input.length.is_some() {
        Some("length")
    } else if input.spacing.is_some() {
        Some("spacing")
    } else if input.mitre.is_some() {
        Some("mitre")
    } else if input.tilt.is_some() {
        Some("tilt")
    } else if input.at.as_ref().is_some_and(|a| a.geometry.is_some()) {
        Some("at")
    } else {
        None
    };
    match refused {
        Some(field) => Err(refuse(format!(
            "a member along an edge takes its place and length from the edge and its offsets: \
             leave out `{field}`"
        ))),
        None => Ok(()),
    }
}

/// Makes a member along an edge as `plan` says (the part where it goes),
/// then its edge frame before its derived part, its length bound from the
/// frame `start` to the plane of `end`, and its rigid joint on `start`;
/// the edge frame goes into the record's `adds`, so that it goes with the
/// member.
pub(crate) fn make_along(
    state: &mut DocState,
    plan: &Plan,
    along: &AlongEdge,
    path: &[OccurrenceUid],
    length: f64,
    lie: Lie,
    active: ComponentUid,
) -> Result<Placed, ModelError> {
    let mut placed = make_part(state, plan, active)?;
    let (Some(component), Some(occurrence)) = (placed.component, placed.occurrence) else {
        return Err(refuse("the member was not made"));
    };
    let part: FeatureUid = state
        .assembly
        .component(component)
        .and_then(|c| c.build.as_ref())
        .and_then(|b| b.part)
        .ok_or_else(|| refuse("the member has no derived part"))?;
    let length_name = plan
        .source
        .meta
        .length
        .as_ref()
        .map(|l| l.parameter.clone())
        .ok_or_else(|| refuse("the part is not cut to length"))?;
    // The edge frame, before the derived part.
    let def = EdgeFrameDef {
        occurrence: OccurrencePath(path.to_vec()),
        edge: along.edge.clone(),
        face: along.face.clone(),
        inset: along.inset.clone(),
        start: along.start.clone(),
        end: along.end.clone(),
        lift: along.lift.clone(),
    };
    let frames = add_feature(state, FeatureDef::EdgeFrame(def), active)?;
    move_before(state, frames, part);
    let mount = |name: &str| GeomRef::Mount {
        feature: frames,
        name: name.to_owned(),
    };
    // Its length from `start` to the plane of `end`.
    let ray = RayPlan {
        from: JointOrigin {
            occurrence: OccurrencePath::default(),
            geometry: mount(START),
            frame_override: None,
        },
        shift: [0.0; 3],
        shift_by_joint: false,
        slide_extra: 0.0,
        direction: RayDirection::default(),
        start: plan.placement.translation,
        along: plan.placement.apply_vector([0.0, 0.0, 1.0]),
    };
    let to = BoundTo {
        occurrence: OccurrencePath::default(),
        geometry: mount(END),
    };
    let bound = add_bound(
        state,
        active,
        part,
        &length_name,
        &ray,
        None,
        &to,
        BoundMode::Face,
        None,
        length,
    )?;
    // Held by its joint, no longer grounded.
    if let Some(o) = state.assembly.occurrence_mut(occurrence) {
        o.grounded = false;
    }
    let (angle, shift) = lie_on_edge(plan.bounds.as_ref(), lie);
    let joint_def = JointDef {
        kind: JointKind::Rigid,
        slide_axis: SlideAxis::Z,
        a: JointOrigin {
            occurrence: OccurrencePath(vec![occurrence]),
            geometry: GeomRef::Mount {
                feature: part,
                name: "end_a".to_owned(),
            },
            frame_override: None,
        },
        b: JointOrigin {
            occurrence: OccurrencePath::default(),
            geometry: mount(START),
            frame_override: Some(FrameOverride {
                shift: Some(shift),
                ..FrameOverride::default()
            }),
        },
        offset: None,
        angle: (angle.abs() > 1e-12).then_some(ValueInput::Number(angle)),
        flip: true,
        limits: BTreeMap::new(),
        position: BTreeMap::new(),
        start: BTreeMap::new(),
    };
    let joint = add_feature(state, FeatureDef::Joint(joint_def), active)?;
    if let Some(record) = state
        .assembly
        .component_mut(component)
        .and_then(|c| c.build.as_mut())
    {
        record.joint = Some(joint);
        record.bound = vec![bound];
        record.adds.push(BuildItem::Feature(frames));
    }
    placed.features.insert(0, bound);
    placed.features.insert(0, frames);
    placed.features.push(joint);
    placed.joint = Some(joint);
    placed.bound = vec![bound];
    placed.adds = vec![BuildItem::Feature(frames)];
    Ok(placed)
}

// Editing (mitcad#1814, point 7): a placed member's edge, face, offsets,
// lie and size.

/// What `build_edit`'s `along` changes of a member along an edge: each
/// field left out keeps what it has.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlongEdit {
    /// A path from the BUILD host to the edge's and the face's component.
    #[serde(default)]
    pub occurrence: Option<String>,
    #[serde(default)]
    pub edge: Option<GeomRef>,
    #[serde(default)]
    pub face: Option<GeomRef>,
    #[serde(default)]
    pub inset: Option<ValueInput>,
    #[serde(default)]
    pub start: Option<ValueInput>,
    #[serde(default)]
    pub end: Option<ValueInput>,
    #[serde(default)]
    pub lift: Option<ValueInput>,
}

/// A member placed along an edge: its edge frame, its joint and how it
/// lies now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlongPart {
    pub frames: FeatureUid,
    pub joint: FeatureUid,
    pub lie: Lie,
}

/// How a member lies by its joint's turn about the edge and its section
/// (as [`lie_on_edge`] turns it): unturned, its section's X runs across
/// the face.
fn lie_of(angle: f64, section: Option<&BoundingBox>) -> Lie {
    let wide = section.is_none_or(|b| b.max[0] - b.min[0] >= b.max[1] - b.min[1]);
    if (angle.abs() < 1e-6) == wide {
        Lie::Flat
    } else {
        Lie::Edge
    }
}

/// The member along an edge a BUILD component is, when it is one: its
/// joint is on an edge frame's `start`.
pub fn along_of<K: Kernel>(doc: &Document<K>, component: ComponentUid) -> Option<AlongPart> {
    let info = doc.part_info(component)?;
    let joint = info.record.joint?;
    let FeatureDef::Joint(j) = &doc.feature(joint)?.def else {
        return None;
    };
    let GeomRef::Mount { feature, name } = &j.b.geometry else {
        return None;
    };
    if name != START || !matches!(doc.feature(*feature)?.def, FeatureDef::EdgeFrame(_)) {
        return None;
    }
    let angle = j
        .angle
        .and_then(|id| doc.parameters().value(id))
        .unwrap_or(0.0);
    Some(AlongPart {
        frames: *feature,
        joint,
        lie: lie_of(angle, doc.part_bounds(component).as_ref()),
    })
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait AlongEditCrateOps<K: Kernel> {
    /// `build_edit` of a member along an edge with `along`, `lie` or
    /// `config` (and with them `parameters` and `ends`): its edge frame,
    /// its row and values, and its joint's turn and shift for the lie and
    /// the section, in one undo step. None for another part or edit.
    fn edit_along(
        &mut self,
        component: ComponentUid,
        input: &crate::EditInput,
    ) -> Result<Option<Vec<String>>, ModelError>;
}

impl<K: Kernel> AlongEditCrateOps<K> for Document<K> {
    fn edit_along(
        &mut self,
        component: ComponentUid,
        input: &crate::EditInput,
    ) -> Result<Option<Vec<String>>, ModelError> {
        if input.along.is_none() && input.lie.is_none() && input.config.is_none() {
            return Ok(None);
        }
        let Some(along) = along_of(self, component) else {
            return Ok(None);
        };
        let other = [
            ("length", input.length.is_some()),
            ("rotate", input.rotate.is_some()),
            ("flip", input.flip.is_some()),
            ("at", input.at.is_some()),
            ("tilt", input.tilt.is_some()),
            ("machining", input.machining.is_some()),
            ("cuts", input.cuts.is_some()),
            ("to", input.to.is_some()),
            ("grade", input.grade.is_some()),
        ];
        if let Some((field, _)) = other.iter().find(|(_, given)| *given) {
            return Err(refuse(format!(
                "a member along an edge takes its place and length from the edge and its offsets: \
                 edit `{field}` apart from `along`, `lie` and `config`"
            )));
        }
        let info = self
            .part_info(component)
            .ok_or_else(|| refuse(format!("{} is no BUILD part", input.component)))?;
        let part = info
            .part
            .ok_or_else(|| refuse(format!("{} has no derived part", info.name)))?;
        let source = info
            .record
            .source
            .ok_or_else(|| refuse(format!("{} has no library source", info.name)))?;
        if let Some(config) = &input.config
            && let Some(def) = &info.source
            && def
                .design
                .state()
                .map_err(refuse)?
                .configurations
                .row(config)
                .is_none()
        {
            return Err(refuse(format!(
                "{} has no configuration {config}",
                def.label()
            )));
        }
        // The edge frame as it is, with the changes.
        let frames_entry = self
            .feature(along.frames)
            .ok_or(ModelError::NoSuchFeature(along.frames))?;
        let FeatureDef::EdgeFrame(mut frames) =
            crate::parts::as_input(&frames_entry.def, self.parameters())
        else {
            return Err(refuse("the member's edge frame is gone"));
        };
        let mut changed = Vec::new();
        if let Some(edit) = &input.along {
            if let Some(text) = &edit.occurrence {
                frames.occurrence = OccurrencePath(self.path_from_host(text)?);
            }
            if let Some(edge) = &edit.edge {
                frames.edge = edge.clone();
            }
            if let Some(face) = &edit.face {
                frames.face = face.clone();
            }
            for (slot, value) in [
                (&mut frames.inset, &edit.inset),
                (&mut frames.start, &edit.start),
                (&mut frames.end, &edit.end),
                (&mut frames.lift, &edit.lift),
            ] {
                if let Some(value) = value {
                    *slot = Some(value.clone());
                }
            }
            changed.push("along".to_owned());
        }
        let lie = input.lie.unwrap_or(along.lie);
        if input.lie.is_some() {
            changed.push("lie".to_owned());
        }
        let config = input.config.clone().or_else(|| info.config());
        if input.config.is_some() {
            changed.push("config".to_owned());
        }
        // The member as it would be placed so: the edge, the face and the
        // offsets checked, its section that of the row.
        let plan = self.plan_place(&PlaceInput {
            part: crate::PartInput {
                source: Some(source),
                config,
                ..crate::PartInput::default()
            },
            along: Some(AlongEdge {
                occurrence: frames.occurrence.to_string(),
                edge: frames.edge.clone(),
                face: frames.face.clone(),
                inset: frames.inset.clone(),
                start: frames.start.clone(),
                end: frames.end.clone(),
                lift: frames.lift.clone(),
            }),
            lie: Some(lie),
            ..PlaceInput::default()
        })?;
        let (angle, shift) = lie_on_edge(plan.bounds.as_ref(), lie);
        let joint_entry = self
            .feature(along.joint)
            .ok_or(ModelError::NoSuchFeature(along.joint))?;
        let FeatureDef::Joint(mut joint) =
            crate::parts::as_input(&joint_entry.def, self.parameters())
        else {
            return Err(refuse("the member's joint is gone"));
        };
        joint.angle = (angle.abs() > 1e-12).then_some(ValueInput::Number(angle));
        joint.b.frame_override = Some(FrameOverride {
            shift: Some(shift),
            ..FrameOverride::default()
        });
        let parameters = input.parameters.clone();
        let ends = input.ends.clone();
        if !parameters.is_empty() {
            changed.push("parameters".to_owned());
        }
        if ends.is_some() {
            changed.push("ends".to_owned());
        }
        let edit_frames = input.along.is_some();
        let new_config = input.config.clone();
        let label = format!("Edit {}", info.name);
        let joint_uid = along.joint;
        let frames_uid = along.frames;
        self.apply(|state| {
            if edit_frames {
                state.edit_feature(frames_uid, &FeatureDef::EdgeFrame(frames))?;
            }
            if new_config.is_some() || !parameters.is_empty() || ends.is_some() {
                crate::place::edit_part(state, part, |def| {
                    if let Some(row) = new_config {
                        def.config = Some(row);
                    }
                    for (name, value) in parameters {
                        def.parameters.insert(name, value);
                    }
                    if let Some(ends) = ends {
                        let cut = |c: crate::EndCutInput| EndCut {
                            angle: c.angle,
                            tilt: c.tilt,
                        };
                        if let Some(a) = ends.a {
                            def.ends.a = a.map(cut);
                        }
                        if let Some(b) = ends.b {
                            def.ends.b = b.map(cut);
                        }
                    }
                })?;
            }
            state.edit_feature(joint_uid, &FeatureDef::Joint(joint))?;
            Ok((label, changed))
        })
        .map(Some)
    }
}
