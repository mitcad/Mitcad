// SPDX-License-Identifier: MIT
//! Commands and queries of joints (mitcad#55, see `commands.md`, "Joints").

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::ApiError;
use crate::assembly::matrix_rows;
use crate::datum::Vec3;
use crate::document::{Document, status_message, status_text};
use crate::features::{
    AsBuiltJointDef, FeatureDef, FrameOverride, GeomRef, JointDef, JointOrigin, JointPosition,
    Limits, OccurrencePath, RigidGroupDef, ValueInput,
};
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::{
    DofUnit, Dragged, JointKind, JointResult, Motion, SlideAxis, apply_override, find_path_from,
    frame_parts, limit_values, origin_frame, path_component,
};
use crate::kernel::Kernel;
use crate::transform::Transform;

/// A joint origin in a command: the occurrence by a path from the joint's
/// component (uids or names; empty: the component's own geometry).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OriginInput {
    #[serde(default)]
    occurrence: String,
    geometry: GeomRef,
    #[serde(default)]
    frame_override: Option<FrameOverride>,
}

/// The fields of `add_joint`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddJoint {
    kind: JointKind,
    #[serde(default)]
    slide_axis: SlideAxis,
    a: OriginInput,
    b: OriginInput,
    #[serde(default)]
    offset: Option<ValueInput>,
    #[serde(default)]
    angle: Option<ValueInput>,
    #[serde(default)]
    flip: bool,
    #[serde(default)]
    limits: Limits<ValueInput>,
    #[serde(default)]
    position: JointPosition<ValueInput>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    component: Option<String>,
}

/// The fields of `add_as_built_joint`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddAsBuiltJoint {
    kind: JointKind,
    #[serde(default)]
    slide_axis: SlideAxis,
    a: String,
    b: String,
    #[serde(default)]
    origin: Option<OriginInput>,
    #[serde(default)]
    limits: Limits<ValueInput>,
    #[serde(default)]
    position: JointPosition<ValueInput>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    component: Option<String>,
}

/// The variants are the commands' names (`add_joint`, ...).
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum JointCommand {
    AddJoint(Box<AddJoint>),
    AddAsBuiltJoint(Box<AddAsBuiltJoint>),
    AddRigidGroup {
        occurrences: Vec<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        component: Option<String>,
    },
    /// Drives free motions: a value each, null to take it away.
    DriveJoint {
        joint: String,
        values: BTreeMap<Motion, Option<ValueInput>>,
    },
    DragOccurrence {
        occurrence: String,
        #[serde(default)]
        point: Option<Vec3>,
        target: Vec3,
    },
}

/// The commands' names, to send them here.
pub(crate) const COMMANDS: [&str; 5] = [
    "add_joint",
    "add_as_built_joint",
    "add_rigid_group",
    "drive_joint",
    "drag_occurrence",
];

/// The `joint_dof` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JointDofQuery {
    occurrence: String,
}

/// The `joint_drag` query: where `drag_occurrence` would put the
/// occurrences.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JointDragQuery {
    occurrence: String,
    #[serde(default)]
    point: Option<Vec3>,
    target: Vec3,
}

/// The `joint_frame` query: the frame a joint origin gives at the marker,
/// for a panel to show where an origin snaps before the joint exists.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JointFrameQuery {
    /// The path from `component` (uids or names; empty: its own geometry).
    #[serde(default)]
    occurrence: String,
    geometry: GeomRef,
    #[serde(default)]
    frame_override: Option<FrameOverride>,
    /// The component the path starts in (the root when left out), whose
    /// coordinates the frame is in.
    #[serde(default)]
    component: Option<String>,
}

fn frame_json(frame: &Transform) -> Value {
    let [origin, x_axis, y_axis, z_axis] = frame_parts(frame);
    json!({"origin": origin, "x_axis": x_axis, "y_axis": y_axis, "z_axis": z_axis})
}

fn motions_json(motions: &[Motion]) -> Value {
    json!(motions.iter().map(|m| m.as_str()).collect::<Vec<_>>())
}

/// Values per motion, null for none.
fn values_json(values: Option<&BTreeMap<Motion, f64>>) -> Value {
    match values {
        Some(values) => Value::Object(
            values
                .iter()
                .map(|(m, v)| (m.as_str().to_owned(), json!(v)))
                .collect(),
        ),
        None => Value::Null,
    }
}

impl<K: Kernel> Document<K> {
    /// An occurrence path from `component`: uids (`O1/O4`), names
    /// (`Arm:1/Pin:2`), or empty for the component's own geometry.
    fn path_from(
        &self,
        component: ComponentUid,
        text: &str,
    ) -> Result<Vec<OccurrenceUid>, ApiError> {
        let a = self.assembly();
        find_path_from(a, component, text)
            .ok_or_else(|| ApiError(format!("no occurrence {text} in {}", a.name(component))))
    }

    fn origin_from(
        &self,
        component: ComponentUid,
        input: OriginInput,
    ) -> Result<JointOrigin, ApiError> {
        Ok(JointOrigin {
            occurrence: OccurrencePath(self.path_from(component, &input.occurrence)?),
            geometry: input.geometry,
            frame_override: input.frame_override,
        })
    }

    pub(crate) fn run_joint_command(&mut self, command: JointCommand) -> Result<Value, ApiError> {
        let component = |doc: &Self, c: &Option<String>| -> Result<ComponentUid, ApiError> {
            match c {
                Some(c) => doc.component_ref(c),
                None => Ok(doc.active_component()),
            }
        };
        let added = match command {
            JointCommand::AddJoint(input) => {
                let AddJoint {
                    kind,
                    slide_axis,
                    a,
                    b,
                    offset,
                    angle,
                    flip,
                    limits,
                    position,
                    name,
                    component: c,
                } = *input;
                let c = component(self, &c)?;
                let def = JointDef {
                    kind,
                    slide_axis,
                    a: self
                        .origin_from(c, a)
                        .map_err(|e| ApiError(format!("a: {e}")))?,
                    b: self
                        .origin_from(c, b)
                        .map_err(|e| ApiError(format!("b: {e}")))?,
                    offset,
                    angle,
                    flip,
                    limits,
                    position,
                };
                self.add_feature_to(&FeatureDef::Joint(def), name.as_deref(), Some(c))?
            }
            JointCommand::AddAsBuiltJoint(input) => {
                let AddAsBuiltJoint {
                    kind,
                    slide_axis,
                    a,
                    b,
                    origin,
                    limits,
                    position,
                    name,
                    component: c,
                } = *input;
                let c = component(self, &c)?;
                let def = AsBuiltJointDef {
                    kind,
                    slide_axis,
                    a: OccurrencePath(self.path_from(c, &a)?),
                    b: OccurrencePath(self.path_from(c, &b)?),
                    origin: origin
                        .map(|o| self.origin_from(c, o))
                        .transpose()
                        .map_err(|e| ApiError(format!("origin: {e}")))?,
                    relative: None,
                    limits,
                    position,
                };
                self.add_as_built_joint(&def, name.as_deref(), Some(c))?
            }
            JointCommand::AddRigidGroup {
                occurrences,
                name,
                component: c,
            } => {
                let c = component(self, &c)?;
                let mut members = Vec::with_capacity(occurrences.len());
                for text in &occurrences {
                    match self.path_from(c, text)?.as_slice() {
                        [o] => members.push(*o),
                        _ => {
                            return Err(ApiError(format!(
                                "{text}: a rigid group's occurrences are placed in {}",
                                self.assembly().name(c)
                            )));
                        }
                    }
                }
                let def = FeatureDef::<ValueInput>::RigidGroup(RigidGroupDef::new(members));
                self.add_feature_to(&def, name.as_deref(), Some(c))?
            }
            JointCommand::DriveJoint { joint, values } => {
                let uid = self.joint_ref(&joint)?;
                let created = self.drive_joint(uid, &values)?;
                let mut result = json!({"uid": uid, "parameters": created});
                if let Some(joint) = self.joint_result(uid) {
                    result["state"] = json!(joint.state.as_str());
                    result["values"] = values_json(joint.values.as_ref());
                }
                return Ok(result);
            }
            JointCommand::DragOccurrence {
                occurrence,
                point,
                target,
            } => {
                let uid = self.occurrence_ref(&occurrence)?;
                let dragged = self.drag_occurrence(uid, point, target)?;
                return Ok(self.dragged_json(&dragged));
            }
        };
        let mut result = json!({"uid": added.uid, "name": added.name,
                                "parameters": added.parameters});
        if let Some(joint) = self.joint_result(added.uid) {
            result["state"] = json!(joint.state.as_str());
            result["solved"] = json!(joint.values.is_some());
        }
        Ok(result)
    }

    /// A joint by uid or name.
    fn joint_ref(&self, text: &str) -> Result<FeatureUid, ApiError> {
        let found = self
            .features()
            .find(|f| f.uid.to_string() == text.trim() || f.name == text.trim())
            .ok_or_else(|| ApiError(format!("no joint {text}")))?;
        match found.def {
            FeatureDef::Joint(_) | FeatureDef::AsBuiltJoint(_) => Ok(found.uid),
            _ => Err(ApiError(format!("{} is not a joint", found.name))),
        }
    }

    fn dragged_json(&self, dragged: &Dragged) -> Value {
        let a = self.assembly();
        let placements: Vec<Value> = dragged
            .placements
            .iter()
            .map(|(o, t)| {
                json!({"occurrence": o, "name": a.occurrence_name(*o),
                       "transform": matrix_rows(t)})
            })
            .collect();
        let values: Map<String, Value> = dragged
            .values
            .iter()
            .map(|(uid, v)| (uid.to_string(), values_json(Some(v))))
            .collect();
        json!({"placements": placements, "values": values})
    }

    /// The `joint_drag` query.
    pub(crate) fn joint_drag_json(&self, query: &JointDragQuery) -> Result<Value, ApiError> {
        let uid = self.occurrence_ref(&query.occurrence)?;
        let dragged = self.joint_drag(uid, query.point, query.target)?;
        Ok(self.dragged_json(&dragged))
    }

    /// The `joint_frame` query: an origin's frame as a joint would resolve
    /// it, at the marker, placed by the path's occurrences into the
    /// coordinates of the component the path starts in.
    pub(crate) fn joint_frame_json(&self, query: &JointFrameQuery) -> Result<Value, ApiError> {
        let a = self.assembly();
        let from = match &query.component {
            Some(c) => self.component_ref(c)?,
            None => ComponentUid::ROOT,
        };
        let path = self.path_from(from, &query.occurrence)?;
        let at = path_component(a, from, &path).map_err(ApiError)?;
        // The geometry must be the component's the path ends in.
        let owner = match &query.geometry {
            GeomRef::Face { body, .. }
            | GeomRef::Edge { body, .. }
            | GeomRef::Vertex { body, .. }
            | GeomRef::Body(body) => Some(
                self.body_component(*body)
                    .ok_or_else(|| ApiError(format!("body {body} does not exist at the marker")))?,
            ),
            GeomRef::Datum(uid)
            | GeomRef::SketchCurves { sketch: uid, .. }
            | GeomRef::SketchPoint { sketch: uid, .. } => Some(
                self.feature(*uid)
                    .ok_or_else(|| ApiError(format!("feature {uid} does not exist")))?
                    .component,
            ),
            _ => None,
        };
        if let Some(owner) = owner
            && owner != at
        {
            return Err(ApiError(format!(
                "{} is in {}, not in {}",
                query.geometry,
                a.name(owner),
                a.name(at)
            )));
        }
        let mut local = origin_frame(&mut self.resolver(), &query.geometry).map_err(ApiError)?;
        if let Some(o) = &query.frame_override {
            local = apply_override(&local, o).map_err(ApiError)?;
        }
        let mut value = frame_json(&self.path_transform(&path).after(&local));
        value["component"] = json!(at);
        value["local"] = frame_json(&local);
        Ok(value)
    }

    fn joint_json(&self, uid: FeatureUid, result: Option<&JointResult>) -> Value {
        let a = self.assembly();
        let entry = self.feature(uid).expect("a feature of the document");
        let status = self.status(uid);
        let warnings = self.warnings(uid);
        let (kind, slide, limits, position, a_path, b_path) = match &entry.def {
            FeatureDef::Joint(j) => (
                j.kind,
                j.slide_axis,
                &j.limits,
                &j.position,
                &j.a.occurrence,
                &j.b.occurrence,
            ),
            FeatureDef::AsBuiltJoint(j) => {
                (j.kind, j.slide_axis, &j.limits, &j.position, &j.a, &j.b)
            }
            _ => unreachable!("joints only"),
        };
        let side =
            |path: &OccurrencePath| json!({"occurrence": a.path_name(&path.0), "path": path.0});
        let mut value = json!({
            "uid": uid,
            "name": entry.name,
            "type": entry.def.type_name(),
            "kind": kind.as_str(),
            "component": entry.component,
            "a": side(a_path),
            "b": side(b_path),
            "motions": motions_json(&kind.motions(slide)),
            "status": status_text(status, warnings),
            "error": status_message(status, warnings),
        });
        if kind == JointKind::Slider {
            value["slide_axis"] = json!(slide);
        }
        let mut limit_map = Map::new();
        for (m, [min, max, rest]) in limit_values(self.parameters(), limits) {
            let mut l = Map::new();
            for (key, v) in [("min", min), ("max", max), ("rest", rest)] {
                if let Some(v) = v {
                    l.insert(key.to_owned(), json!(v));
                }
            }
            limit_map.insert(m.as_str().to_owned(), Value::Object(l));
        }
        value["limits"] = Value::Object(limit_map);
        let params = self.parameters();
        let driven: BTreeMap<Motion, f64> = position
            .iter()
            .filter_map(|(m, id)| Some((*m, params.value(*id)?)))
            .collect();
        value["position"] = values_json(Some(&driven));
        match result {
            Some(r) => {
                value["state"] = json!(r.state.as_str());
                // Holds at the marker (a later feature may move a side).
                value["solved"] = json!(r.values.is_some());
                if r.values.is_none() {
                    value["message"] = json!(
                        "the joint does not hold at the marker: a later feature moved a side"
                    );
                }
                if let crate::joints::JointState::Placed(moved) = &r.state {
                    value["moved"] = json!(moved);
                }
                value["values"] = values_json(r.values.as_ref());
                value["within_limits"] = json!(r.beyond_limits.is_empty());
                if !r.beyond_limits.is_empty() {
                    value["beyond_limits"] = motions_json(&r.beyond_limits);
                }
                value["frames"] = json!({"a": frame_json(&r.frame_a), "b": frame_json(&r.frame_b)});
            }
            None => {
                let state = match status_text(status, warnings) {
                    "error" => "failed",
                    other => other,
                };
                value["state"] = json!(state);
                value["solved"] = json!(false);
            }
        }
        value
    }

    fn unit_json(&self, unit: &DofUnit) -> Value {
        let a = self.assembly();
        json!({
            "occurrences": unit.occurrences,
            "names": unit.occurrences.iter().map(|o| a.occurrence_name(*o)).collect::<Vec<_>>(),
            "grounded": unit.grounded,
            "dof": unit.dof,
            "joints": unit.joints,
        })
    }

    /// The `joints` query: every joint and as-built joint in timeline
    /// order, the rigid groups in effect, and the degrees of freedom of
    /// each component with joints.
    pub(crate) fn joints_json(&self) -> Value {
        let a = self.assembly();
        let mut joints = Vec::new();
        let mut components: Vec<ComponentUid> = Vec::new();
        for entry in self.features() {
            if !matches!(
                entry.def,
                FeatureDef::Joint(_) | FeatureDef::AsBuiltJoint(_) | FeatureDef::RigidGroup(_)
            ) {
                continue;
            }
            if !components.contains(&entry.component) {
                components.push(entry.component);
            }
            if !matches!(entry.def, FeatureDef::RigidGroup(_)) {
                joints.push(self.joint_json(entry.uid, self.joint_result(entry.uid)));
            }
        }
        let groups: Vec<Value> = self
            .features()
            .filter(|f| matches!(f.def, FeatureDef::RigidGroup(_)))
            .map(|f| {
                let FeatureDef::RigidGroup(g) = &f.def else {
                    unreachable!()
                };
                let active = self.rigid_groups().iter().any(|(uid, _, _)| *uid == f.uid);
                let status = self.status(f.uid);
                json!({
                    "uid": f.uid,
                    "name": f.name,
                    "component": f.component,
                    "occurrences": g.occurrences,
                    "names": g.occurrences.iter().map(|o| a.occurrence_name(*o)).collect::<Vec<_>>(),
                    "active": active,
                    "status": status_text(status, self.warnings(f.uid)),
                    "error": status_message(status, self.warnings(f.uid)),
                })
            })
            .collect();
        let dof: Vec<Value> = components
            .iter()
            .map(|c| {
                let report = self.joint_dof(*c);
                json!({
                    "component": c,
                    "total": report.total,
                    "overconstrained": report.overconstrained,
                    "redundant": report.redundant,
                    "conflicting": report.conflicting,
                    "units": report.units.iter().map(|u| self.unit_json(u)).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({"joints": joints, "rigid_groups": groups, "dof": dof})
    }

    /// The `joint_dof` query: an occurrence's free motions in its parent
    /// component.
    pub(crate) fn joint_dof_json(&self, query: &JointDofQuery) -> Result<Value, ApiError> {
        let uid = self.occurrence_ref(&query.occurrence)?;
        let a = self.assembly();
        let parent = a.occurrence(uid).expect("found").parent;
        let report = self.joint_dof(parent);
        let unit = report
            .units
            .iter()
            .find(|u| u.occurrences.contains(&uid))
            .expect("every top-level occurrence has a unit");
        let joints: Vec<Value> = unit
            .joints
            .iter()
            .filter_map(|j| {
                let result = self.joint_result(*j)?;
                let mine = |path: &[OccurrenceUid]| {
                    path.first().is_some_and(|o| unit.occurrences.contains(o))
                };
                let other = if mine(&result.a) {
                    &result.b
                } else {
                    &result.a
                };
                let name = self.feature(*j).map(|f| f.name.clone());
                Some(json!({
                    "uid": j,
                    "name": name,
                    "kind": result.kind.as_str(),
                    "other": if other.is_empty() { a.name(parent) } else { a.path_name(other) },
                    "motions": motions_json(&result.kind.motions(result.slide)),
                }))
            })
            .collect();
        let motions = if unit.grounded {
            json!([])
        } else {
            match unit.joints.as_slice() {
                [] => motions_json(&Motion::ALL),
                [one] => match self.joint_result(*one) {
                    Some(r) => motions_json(&r.kind.motions(r.slide)),
                    None => Value::Null,
                },
                _ => Value::Null,
            }
        };
        Ok(json!({
            "occurrence": uid,
            "name": a.occurrence_name(uid),
            "component": parent,
            "grounded": unit.grounded,
            "group": unit.occurrences,
            "dof": unit.dof,
            "motions": motions,
            "joints": joints,
        }))
    }
}
