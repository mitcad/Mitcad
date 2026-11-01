// SPDX-License-Identifier: MIT
//! Commands and queries of joints (mitcad#55, see `commands.md`, "Joints").

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::dispatch::named_enum;
use super::prelude::*;
use super::{ApiError, add_feature_notices};
use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::assembly::matrix_rows;
use mitcad_model::constraints::RowReference;
use mitcad_model::datum::Vec3;
use mitcad_model::features::{
    AsBuiltJointDef, ConstraintRow, ConstraintSetDef, ConstraintType, Coupling, FeatureDef,
    FrameOverride, GeomRef, JointDef, JointOrigin, JointPosition, Limits, LinkedMotion,
    MotionLinkDef, OccurrencePath, RigidGroupDef, ValueInput,
};
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::internal::document::status_text;
use mitcad_model::internal::joints::{frame_parts, limit_values, path_component};
use mitcad_model::joints::{DofUnit, Dragged, JointKind, JointResult, Motion, SlideAxis};
use mitcad_model::kernel::Kernel;
use mitcad_model::transform::Transform;

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

impl OriginInput {
    /// The reference as the model checks a constraint row's
    /// ([`Document::constraint_row`]).
    fn row_reference(&self) -> RowReference<'_> {
        RowReference {
            occurrence: &self.occurrence,
            geometry: &self.geometry,
            frame_override: self.frame_override,
        }
    }
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

named_enum! {
    /// The variants are the commands' names (`add_joint`, ...).
    #[allow(clippy::enum_variant_names)]
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum JointCommand {
        "add_joint" => AddJoint(Box<AddJoint>),
        "add_as_built_joint" => AddAsBuiltJoint(Box<AddAsBuiltJoint>),
        "add_rigid_group" => AddRigidGroup {
            occurrences: Vec<String>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            component: Option<String>,
        },
        /// Drives free motions: a value each, null to take it away.
        "drive_joint" => DriveJoint {
            joint: String,
            values: BTreeMap<Motion, Option<ValueInput>>,
        },
        "drag_occurrence" => DragOccurrence {
            occurrence: String,
            #[serde(default)]
            point: Option<Vec3>,
            target: Vec3,
        },
        /// A constraint set (mitcad#148): rows of `constraint_set`, their
        /// occurrences also by names.
        "add_constraint_set" => AddConstraintSet {
            rows: Vec<Value>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            component: Option<String>,
        },
        /// A motion link (mitcad#466): joints by uid or name, occurrences of
        /// picked geometry also by names.
        "add_motion_link" => AddMotionLink(Box<AddMotionLink>),
    }
}

/// A joint's motion in `add_motion_link`: the joint by uid or name.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MotionInput {
    joint: String,
    motion: Motion,
}

/// The fields of `add_motion_link`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddMotionLink {
    a: MotionInput,
    b: MotionInput,
    coupling: Value,
    #[serde(default)]
    reverse: bool,
    #[serde(default)]
    offset: Option<ValueInput>,
    /// Without an offset: keep the joints where they are (the default for
    /// every coupling but a cam).
    #[serde(default)]
    as_built: Option<bool>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    component: Option<String>,
}

/// The `motion_link_offset` query (mitcad#466): the offset that keeps a
/// link's joints where they are at the marker, for a panel to add the link
/// as the `add_motion_link` command would.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MotionLinkOffsetQuery {
    a: MotionInput,
    b: MotionInput,
    coupling: Value,
    #[serde(default)]
    reverse: bool,
    #[serde(default)]
    component: Option<String>,
}

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

/// The `constraint_row` query (mitcad#148): a row's references before the
/// row is added.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConstraintRowQuery {
    a: OriginInput,
    b: OriginInput,
    #[serde(default)]
    a2: Option<OriginInput>,
    #[serde(default)]
    b2: Option<OriginInput>,
    /// A type to check the references against.
    #[serde(default, rename = "type")]
    kind: Option<ConstraintType>,
    /// The component the paths start in (the active one when left out).
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

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait JointsCrateOps<K: Kernel> {
    fn run_joint_command(&mut self, command: JointCommand) -> Result<Value, ApiError>;

    /// The `motion_link_offset` query.
    fn motion_link_offset_json(&self, query: MotionLinkOffsetQuery) -> Result<Value, ApiError>;

    /// The `joint_drag` query.
    fn joint_drag_json(&self, query: &JointDragQuery) -> Result<Value, ApiError>;

    /// The `joint_frame` query: an origin's frame as a joint would resolve
    /// it, at the marker, placed by the path's occurrences into the
    /// coordinates of the component the path starts in.
    fn joint_frame_json(&self, query: &JointFrameQuery) -> Result<Value, ApiError>;

    /// The `constraint_row` query: the kinds of a row's references, the
    /// types they fit (the likeliest first), what they measure where they
    /// are (an align's `offset`, the `angle` between their z axes, an
    /// insert's `twist`: the turn a locked insert records), and for a
    /// `type` whether they fit it.
    fn constraint_row_json(&self, query: &ConstraintRowQuery) -> Result<Value, ApiError>;

    /// The `joints` query: every joint and as-built joint in timeline
    /// order, the rigid groups in effect, and the degrees of freedom of
    /// each component with joints.
    fn joints_json(&self) -> Value;

    /// The `joint_dof` query: an occurrence's free motions in its parent
    /// component.
    fn joint_dof_json(&self, query: &JointDofQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> JointsCrateOps<K> for Document<K> {
    fn run_joint_command(&mut self, command: JointCommand) -> Result<Value, ApiError> {
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
                let driven = self.drive_joint(uid, &values)?;
                let mut result = json!({"uid": uid, "parameters": driven.parameters});
                if let Some(joint) = self.joint_result(uid) {
                    result["state"] = json!(joint.state.as_str());
                    result["values"] = values_json(joint.values.as_ref());
                }
                // Stopped at a contact of a contact set (mitcad#454).
                if let Some((hit, fraction)) = &driven.contact {
                    result["contact"] = self.contact_json(hit, *fraction);
                    // What a panel says before it keeps it.
                    result["warnings"] = json!([result["contact"]["message"]]);
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
            JointCommand::AddConstraintSet {
                rows,
                name,
                component: c,
            } => {
                let c = component(self, &c)?;
                let mut parsed = Vec::with_capacity(rows.len());
                for (i, mut row) in rows.into_iter().enumerate() {
                    let at = |e: String| ApiError(format!("rows[{i}]: {e}"));
                    // Occurrences by names become uid paths.
                    for key in ["a", "a2", "b", "b2"] {
                        let Some(text) = row
                            .get(key)
                            .and_then(|o| o.get("occurrence"))
                            .and_then(Value::as_str)
                        else {
                            continue;
                        };
                        let path = self
                            .path_from(c, text)
                            .map_err(|e| at(format!("{key}: {e}")))?;
                        row[key]["occurrence"] = json!(OccurrencePath(path).to_string());
                    }
                    parsed.push(
                        serde_json::from_value::<ConstraintRow<ValueInput>>(row)
                            .map_err(|e| at(e.to_string()))?,
                    );
                }
                let def = FeatureDef::ConstraintSet(ConstraintSetDef { rows: parsed });
                let added = self.add_feature_to(&def, name.as_deref(), Some(c))?;
                let mut result = json!({"uid": added.uid, "name": added.name,
                                        "parameters": added.parameters});
                if let Some(set) = self.constraint_result(added.uid) {
                    result["state"] = json!(set.state.as_str());
                    result["solved"] = json!(set.rows.iter().all(|r| r.holds));
                }
                return Ok(result);
            }
            JointCommand::AddMotionLink(input) => {
                let AddMotionLink {
                    a,
                    b,
                    mut coupling,
                    reverse,
                    offset,
                    as_built,
                    name,
                    component: c,
                } = *input;
                let c = component(self, &c)?;
                let def = self.link_def(c, &a, &b, &mut coupling, reverse, offset)?;
                let added = self.add_motion_link(&def, as_built, name.as_deref(), Some(c))?;
                let mut result = json!({"uid": added.uid, "name": added.name,
                                        "parameters": added.parameters});
                if let Some(link) = self.link_result(added.uid) {
                    result["state"] = json!(link.state.as_str());
                    result["solved"] = json!(link.holds);
                }
                return Ok(result);
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

    fn motion_link_offset_json(&self, query: MotionLinkOffsetQuery) -> Result<Value, ApiError> {
        let MotionLinkOffsetQuery {
            a,
            b,
            mut coupling,
            reverse,
            component,
        } = query;
        let c = match &component {
            Some(c) => self.component_ref(c)?,
            None => self.active_component(),
        };
        let def = self.link_def(c, &a, &b, &mut coupling, reverse, None)?;
        let offset = self.link_offset_as_built(&def, c)?;
        Ok(json!({"offset": offset}))
    }

    fn joint_drag_json(&self, query: &JointDragQuery) -> Result<Value, ApiError> {
        let uid = self.occurrence_ref(&query.occurrence)?;
        let dragged = self.joint_drag(uid, query.point, query.target)?;
        Ok(self.dragged_json(&dragged))
    }

    fn joint_frame_json(&self, query: &JointFrameQuery) -> Result<Value, ApiError> {
        let from = match &query.component {
            Some(c) => self.component_ref(c)?,
            None => ComponentUid::ROOT,
        };
        let frame = self
            .joint_frame(
                from,
                &query.occurrence,
                &query.geometry,
                query.frame_override.as_ref(),
            )
            .map_err(ApiError)?;
        let mut value = frame_json(&frame.frame);
        value["component"] = json!(frame.component);
        value["local"] = frame_json(&frame.local);
        Ok(value)
    }

    fn constraint_row_json(&self, query: &ConstraintRowQuery) -> Result<Value, ApiError> {
        let component = match &query.component {
            Some(c) => self.component_ref(c)?,
            None => self.active_component(),
        };
        let a2 = query.a2.as_ref().map(OriginInput::row_reference);
        let b2 = query.b2.as_ref().map(OriginInput::row_reference);
        let row = self
            .constraint_row(
                component,
                &query.a.row_reference(),
                &query.b.row_reference(),
                a2.as_ref(),
                b2.as_ref(),
                query.kind,
            )
            .map_err(ApiError)?;
        let types: Vec<&str> = row
            .types
            .iter()
            .copied()
            .map(ConstraintType::as_str)
            .collect();
        let mut value = json!({
            "kinds": {"a": row.a.kind.as_str(), "b": row.b.kind.as_str()},
            "types": types,
            "type": types.first(),
            "measures": row.measures,
        });
        if let Some(fits) = row.fits {
            value["fits"] = json!(fits.is_ok());
            if let Err(e) = fits {
                value["error"] = json!(e);
            }
        }
        Ok(value)
    }

    fn joints_json(&self) -> Value {
        let a = self.assembly();
        let mut joints = Vec::new();
        let mut constraints = Vec::new();
        let mut links = Vec::new();
        let mut components: Vec<ComponentUid> = Vec::new();
        for entry in self.features() {
            if !matches!(
                entry.def,
                FeatureDef::Joint(_)
                    | FeatureDef::AsBuiltJoint(_)
                    | FeatureDef::RigidGroup(_)
                    | FeatureDef::ConstraintSet(_)
                    | FeatureDef::MotionLink(_)
            ) {
                continue;
            }
            if !components.contains(&entry.component) {
                components.push(entry.component);
            }
            if matches!(entry.def, FeatureDef::MotionLink(_)) {
                links.push(self.link_json(entry.uid));
            } else if matches!(entry.def, FeatureDef::ConstraintSet(_)) {
                constraints.push(self.constraint_json(entry.uid));
            } else if !matches!(entry.def, FeatureDef::RigidGroup(_)) {
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
                let warnings = self.warnings(f.uid);
                let mut value = json!({
                    "uid": f.uid,
                    "name": f.name,
                    "component": f.component,
                    "occurrences": g.occurrences,
                    "names": g.occurrences.iter().map(|o| a.occurrence_name(*o)).collect::<Vec<_>>(),
                    "active": active,
                    "status": status_text(status, warnings),
                    "error": status.and_then(FeatureStatus::error),
                });
                add_feature_notices(&mut value, status, warnings, None, f.uid);
                value
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
        json!({"joints": joints, "rigid_groups": groups, "constraints": constraints,
               "links": links, "dof": dof})
    }

    fn joint_dof_json(&self, query: &JointDofQuery) -> Result<Value, ApiError> {
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
            "mobile": unit.mobile,
            "motions": motions,
            "joints": joints,
        }))
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait JointsLocal<K: Kernel> {
    /// An occurrence path from `component`: uids (`O1/O4`), names
    /// (`Arm:1/Pin:2`), or empty for the component's own geometry.
    fn path_from(
        &self,
        component: ComponentUid,
        text: &str,
    ) -> Result<Vec<OccurrenceUid>, ApiError>;

    fn origin_from(
        &self,
        component: ComponentUid,
        input: OriginInput,
    ) -> Result<JointOrigin, ApiError>;

    /// Joint origins in a command's JSON (objects with `geometry` and an
    /// `occurrence` path by uids or names from `component`): their paths
    /// as uids.
    fn paths_to_uids(&self, component: ComponentUid, value: &mut Value) -> Result<(), ApiError>;

    /// A motion link's definition from a command's fields: joints by uid or
    /// name, occurrences of picked geometry by names too.
    fn link_def(
        &self,
        component: ComponentUid,
        a: &MotionInput,
        b: &MotionInput,
        coupling: &mut Value,
        reverse: bool,
        offset: Option<ValueInput>,
    ) -> Result<MotionLinkDef<ValueInput>, ApiError>;

    /// A motion link's state for the `joints` query (mitcad#466).
    fn link_json(&self, uid: FeatureUid) -> Value;

    /// A joint by uid or name.
    fn joint_ref(&self, text: &str) -> Result<FeatureUid, ApiError>;

    fn dragged_json(&self, dragged: &Dragged) -> Value;

    fn joint_json(&self, uid: FeatureUid, result: Option<&JointResult>) -> Value;

    /// One side of a joint or constraint row in the `joints` query
    /// (mitcad#523): `occurrence` (the path's names) and `path` (uids) from
    /// the feature's component, `geometry` (the reference as the
    /// definition stores it), `on` (what it is: [`Self::geometry_on`]),
    /// `frame_override` when it has one, and `component`, the one the path
    /// ends in (left out when the path leads nowhere).
    fn origin_side_json(&self, component: ComponentUid, origin: &JointOrigin) -> Value;

    /// What a side's reference is, for the application's joint highlight:
    /// `face`, `edge`, `vertex`, `body`, `sketch_curve`, `sketch_point`,
    /// `origin` (an origin plane, axis or the origin), `construction` (a
    /// construction plane, axis or point), `joint_origin`, `fixed` (a
    /// fixed plane, axis or point, in no geometry) or `mount` (a derived
    /// part's mount frame).
    fn geometry_on(&self, geometry: &GeomRef) -> &'static str;

    fn constraint_json(&self, uid: FeatureUid) -> Value;

    fn unit_json(&self, unit: &DofUnit) -> Value;
}

impl<K: Kernel> JointsLocal<K> for Document<K> {
    fn path_from(
        &self,
        component: ComponentUid,
        text: &str,
    ) -> Result<Vec<OccurrenceUid>, ApiError> {
        self.occurrence_path_from(component, text).map_err(ApiError)
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

    fn paths_to_uids(&self, component: ComponentUid, value: &mut Value) -> Result<(), ApiError> {
        match value {
            Value::Object(map) => {
                if map.contains_key("geometry")
                    && let Some(Value::String(text)) = map.get("occurrence")
                {
                    let path = self.path_from(component, text)?;
                    map.insert(
                        "occurrence".to_owned(),
                        json!(OccurrencePath(path).to_string()),
                    );
                }
                for v in map.values_mut() {
                    self.paths_to_uids(component, v)?;
                }
            }
            Value::Array(items) => {
                for v in items {
                    self.paths_to_uids(component, v)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn link_def(
        &self,
        component: ComponentUid,
        a: &MotionInput,
        b: &MotionInput,
        coupling: &mut Value,
        reverse: bool,
        offset: Option<ValueInput>,
    ) -> Result<MotionLinkDef<ValueInput>, ApiError> {
        self.paths_to_uids(component, coupling)?;
        let coupling = serde_json::from_value::<Coupling<ValueInput>>(coupling.clone())
            .map_err(|e| ApiError(format!("coupling: {e}")))?;
        Ok(MotionLinkDef {
            a: LinkedMotion {
                joint: self.joint_ref(&a.joint)?,
                motion: a.motion,
            },
            b: LinkedMotion {
                joint: self.joint_ref(&b.joint)?,
                motion: b.motion,
            },
            coupling,
            reverse,
            offset,
        })
    }

    fn link_json(&self, uid: FeatureUid) -> Value {
        let entry = self.feature(uid).expect("a feature of the document");
        let FeatureDef::MotionLink(link) = &entry.def else {
            unreachable!("motion links only")
        };
        let status = self.status(uid);
        let warnings = self.warnings(uid);
        let name = |f: FeatureUid| {
            self.feature(f)
                .map_or_else(|| f.to_string(), |e| e.name.clone())
        };
        let side = |m: &LinkedMotion| json!({"joint": m.joint, "name": name(m.joint), "motion": m.motion.as_str()});
        let mut value = json!({
            "uid": uid,
            "name": entry.name,
            "type": entry.def.type_name(),
            "component": entry.component,
            "coupling": link.coupling.type_name(),
            "a": side(&link.a),
            "b": side(&link.b),
            "reverse": link.reverse,
            "status": status_text(status, warnings),
            "error": status.and_then(FeatureStatus::error),
        });
        add_feature_notices(&mut value, status, warnings, None, uid);
        match self.link_result(uid) {
            Some(r) => {
                value["state"] = json!(r.state.as_str());
                value["solved"] = json!(r.holds);
                if let Some(ratio) = r.ratio {
                    value["ratio"] = json!(ratio);
                }
                value["offset"] = json!(r.offset);
                if let Some((va, vb)) = r.values {
                    value["values"] = json!({"a": va, "b": vb});
                }
                if let mitcad_model::joints::JointState::Placed(moved) = &r.state {
                    value["moved"] = json!(moved);
                }
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

    fn joint_ref(&self, text: &str) -> Result<FeatureUid, ApiError> {
        let is_joint =
            |def: &FeatureDef| matches!(def, FeatureDef::Joint(_) | FeatureDef::AsBuiltJoint(_));
        // The joint of a name it shares with other features (mitcad#1107).
        let found = self
            .feature_by_text(text, Some(&is_joint))?
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
        let mut value = json!({"placements": placements, "values": values});
        // Stopped at a contact of a contact set (mitcad#454).
        if let Some(hit) = &dragged.contact {
            value["contact"] = self.contact_json(hit, 1.0);
            if let Some(contact) = value["contact"].as_object_mut() {
                contact.remove("fraction");
            }
        }
        value
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
        // What each side is on (mitcad#523): a joint's origins with their
        // geometry, an as-built joint's occurrences and its motion origin.
        let (a_side, b_side, origin_side) = match &entry.def {
            FeatureDef::Joint(j) => (
                self.origin_side_json(entry.component, &j.a),
                self.origin_side_json(entry.component, &j.b),
                None,
            ),
            FeatureDef::AsBuiltJoint(j) => {
                let occurrence_side = |path: &OccurrencePath| {
                    let mut value = side(path);
                    value["on"] = json!("occurrence");
                    if let Ok(end) = path_component(a, entry.component, &path.0) {
                        value["component"] = json!(end);
                    }
                    value
                };
                (
                    occurrence_side(a_path),
                    occurrence_side(b_path),
                    j.origin
                        .as_ref()
                        .map(|o| self.origin_side_json(entry.component, o)),
                )
            }
            _ => unreachable!("joints only"),
        };
        let mut value = json!({
            "uid": uid,
            "name": entry.name,
            "type": entry.def.type_name(),
            "kind": kind.as_str(),
            "component": entry.component,
            "a": a_side,
            "b": b_side,
            "motions": motions_json(&kind.motions(slide)),
            "status": status_text(status, warnings),
            "error": status.and_then(FeatureStatus::error),
        });
        add_feature_notices(&mut value, status, warnings, None, uid);
        if let Some(origin) = origin_side {
            value["origin"] = origin;
        }
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
                if let mitcad_model::joints::JointState::Placed(moved) = &r.state {
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

    fn origin_side_json(&self, component: ComponentUid, origin: &JointOrigin) -> Value {
        let a = self.assembly();
        let mut value = json!({
            "occurrence": a.path_name(&origin.occurrence.0),
            "path": origin.occurrence.0,
            "geometry": origin.geometry,
            "on": self.geometry_on(&origin.geometry),
        });
        if let Some(o) = &origin.frame_override {
            value["frame_override"] = json!(o);
        }
        if let Ok(end) = path_component(a, component, &origin.occurrence.0) {
            value["component"] = json!(end);
        }
        value
    }

    fn geometry_on(&self, geometry: &GeomRef) -> &'static str {
        match geometry {
            GeomRef::Origin(_) => "origin",
            GeomRef::Datum(uid) => match self.feature(*uid).map(|f| &f.def) {
                Some(FeatureDef::JointOrigin(_)) => "joint_origin",
                _ => "construction",
            },
            GeomRef::Face { .. } => "face",
            GeomRef::Edge { .. } => "edge",
            GeomRef::Vertex { .. } => "vertex",
            GeomRef::Body(_) => "body",
            GeomRef::SketchCurves { .. } => "sketch_curve",
            GeomRef::SketchPoint { .. } => "sketch_point",
            GeomRef::FixedPlane { .. } | GeomRef::FixedAxis { .. } | GeomRef::FixedPoint(_) => {
                "fixed"
            }
            GeomRef::Mount { .. } => "mount",
        }
    }

    fn constraint_json(&self, uid: FeatureUid) -> Value {
        let entry = self.feature(uid).expect("a feature of the document");
        let FeatureDef::ConstraintSet(set) = &entry.def else {
            unreachable!("constraint sets only")
        };
        let status = self.status(uid);
        let warnings = self.warnings(uid);
        let result = self.constraint_result(uid);
        let rows: Vec<Value> = set
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let mut value = json!({
                    "row": i + 1,
                    "type": row.kind.as_str(),
                    "a": self.origin_side_json(entry.component, &row.a),
                    "b": self.origin_side_json(entry.component, &row.b),
                });
                for (key, second) in [("a2", &row.a2), ("b2", &row.b2)] {
                    if let Some(o) = second {
                        value[key] = self.origin_side_json(entry.component, o);
                    }
                }
                if let Some(r) = result.and_then(|r| r.rows.get(i)) {
                    value["holds"] = json!(r.holds);
                    if let Some(v) = r.value {
                        value["value"] = json!(v);
                    }
                }
                value
            })
            .collect();
        let mut value = json!({
            "uid": uid,
            "name": entry.name,
            "type": entry.def.type_name(),
            "component": entry.component,
            "rows": rows,
            "status": status_text(status, warnings),
            "error": status.and_then(FeatureStatus::error),
        });
        add_feature_notices(&mut value, status, warnings, None, uid);
        match result {
            Some(r) => {
                value["state"] = json!(r.state.as_str());
                if let mitcad_model::joints::JointState::Placed(moved) = &r.state {
                    value["moved"] = json!(moved);
                }
                value["solved"] = json!(r.rows.iter().all(|row| row.holds));
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
            "mobile": unit.mobile,
            "joints": unit.joints,
        })
    }
}
