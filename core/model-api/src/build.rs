// SPDX-License-Identifier: MIT
//! Commands and queries of the BUILD tab (mitcad#375; `commands.md`,
//! "BUILD: profile assemblies"): placing, editing, connecting, panels and
//! removing, the preview of each, the parts, the junctions and the cut
//! list. Frames in answers are in the design's coordinates; lengths are
//! millimetres and angles radians.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::expr::Unit;
use mitcad_model::features::bound_length::BoundMode;
use mitcad_model::features::derived_part::{EndCut, Ends, PartCut};
use mitcad_model::features::{FeatureDef, GeomRef, OccurrencePath, ValueInput};
use mitcad_model::ids::{ComponentUid, OccurrenceUid};
use mitcad_model::internal::build::value_in;
use mitcad_model::internal::features::{Evaluated, evaluate_source};
use mitcad_model::kernel::Kernel;
use mitcad_model::parameters::ParamId;
use mitcad_model::part_options::PartValue;
use mitcad_model::transform::Transform;
use mitcad_model_build::internal::{LengthPlan, Plan, SourceUse, Spot};
use mitcad_model_build::prelude::*;
use mitcad_model_build::{
    BuildRole, ConnectInput, CutListOptions, EditInput, MitreCut, PanelInput, PartInput,
    PlaceInput, Placed,
};

/// What a placing's ray meets: the occurrence path, the face and the
/// length to it.
type RayHit = (Vec<OccurrenceUid>, GeomRef, f64);

/// A preview's source evaluations by their values (mitcad#487): one
/// request's parts share one source.
type Evaluations<S> = Vec<(Vec<(String, PartValue)>, Evaluated<S>)>;

/// What a preview cuts and moves of a source's bodies: `touch` the end
/// that touches the target (end B when flipped), kept where the plan put
/// it when the length differs from the plan's; `ends` the end cuts at the
/// length; `rip` a box taken away, in the part's coordinates (a set's last
/// board, mitcad#478).
#[derive(Default)]
struct PreviewCuts<'a> {
    touch: Option<(&'a str, [f64; 3])>,
    ends: Option<(&'a Ends<ValueInput>, f64)>,
    rip: Option<&'a PartCut<ValueInput>>,
    bound_length: bool,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    // The variants are named after the commands (`build_place`, ...), read from `cmd`.
    #[allow(clippy::enum_variant_names)]
    pub(crate) enum BuildCommand {
        "build_place" => BuildPlace(Box<PlaceInput>),
        "build_edit" => BuildEdit(Box<EditInput>),
        "build_connect" => BuildConnect(Box<ConnectInput>),
        "build_panel" => BuildPanel(Box<PanelInput>),
        "build_remove" => BuildRemove {
            component: String,
            #[serde(default)]
            set: bool,
        },
        // Timber frames (mitcad#479).
        "build_frame" => BuildFrame(Box<mitcad_model_build::FrameInput>),
        // Fasteners swapped in place (mitcad#497).
        "build_swap_fasteners" => BuildSwapFasteners(Box<mitcad_model_build::SwapInput>),
        // Connectors placed by the first rule placed again (mitcad#1115).
        "build_upgrade" => BuildUpgrade {},
    }
}

/// What a timber frame's command made (mitcad#479).
fn framed_json(framed: &mitcad_model_build::internal::Framed) -> Value {
    json!({
        "component": framed.lead,
        "frame": framed.lead,
        // Its own component in the host (mitcad#522).
        "frame_component": framed.component,
        "name": framed.name,
        "kind": framed.kind,
        "members": framed.members.iter()
            .map(|(c, key)| json!({"component": c, "member": key}))
            .collect::<Vec<_>>(),
        "count": framed.members.len(),
        "fasteners": framed.fasteners,
        "relaid": framed.relaid,
        "warnings": framed.warnings,
        "junction": framed.junction.as_ref().map(|j| j.to_json()),
        // A wall's plates made of pieces (mitcad#536).
        "splices": framed.splices,
    })
}

/// The `build_junctions` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JunctionsQuery {
    #[serde(default)]
    components: Option<Vec<String>>,
}

/// The `build_cut_list` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CutListQuery {
    #[serde(default)]
    kerf: Option<f64>,
}

/// A frame as answers give it.
fn frame_json(t: &Transform) -> Value {
    let [origin, x_axis, y_axis, z_axis] = mitcad_model::internal::build::parts(t);
    json!({"origin": origin, "x_axis": x_axis, "y_axis": y_axis, "z_axis": z_axis})
}

fn path_text(path: &[OccurrenceUid]) -> String {
    OccurrencePath(path.to_vec()).to_string()
}

fn ends_json(ends: &Ends, angle: &dyn Fn(ParamId) -> Option<f64>) -> Value {
    let cut = |c: &Option<EndCut>| match c {
        Some(c) => json!({"angle": angle(c.angle), "tilt": c.tilt.unwrap_or_default().as_str()}),
        None => Value::Null,
    };
    json!({"a": cut(&ends.a), "b": cut(&ends.b)})
}

fn placed_json(placed: &Placed) -> Value {
    let mut value = json!({
        "component": placed.component,
        "occurrence": placed.occurrence,
        "name": placed.name,
        "features": placed.features,
        "joint": placed.joint,
        "bound": placed.bound,
        "adds": placed.adds,
        "warnings": placed.warnings,
        "place": placed.place,
    });
    if !placed.set.is_empty() {
        value["set"] = json!(placed.set);
        value["count"] = json!(placed.set.len());
    }
    if let Some((fits, reason)) = &placed.fits {
        value["fits"] = json!(fits);
        value["reason"] = if reason.is_empty() {
            Value::Null
        } else {
            json!(reason)
        };
    }
    value
}

/// What a preview request is: a connector's, a panel's or a placing's.
enum PreviewInput {
    Place(Box<PlaceInput>),
    Panel(Box<PanelInput>),
    Connect(Box<ConnectInput>),
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait BuildCrateOps<K: Kernel> {
    fn run_build_command(&mut self, command: BuildCommand) -> Result<Value, ApiError>;

    /// The `build_preview` query.
    fn build_preview_json(&self, fields: Map<String, Value>) -> Result<Value, ApiError>;

    /// What a ray from `start` along `along` (design coordinates) meets
    /// first: a frame of another BUILD part facing it, within that part's
    /// box; with the path, the frame and the length the mode gives.
    fn ray_target(
        &self,
        start: mitcad_model::internal::build::V3,
        along: mitcad_model::internal::build::V3,
        except: Option<ComponentUid>,
        mode: BoundMode,
    ) -> Option<RayHit>;

    /// The preview's shapes (`analysis_shape` with `build_preview`): the
    /// new part's bodies in the design's coordinates; a spacing set's every
    /// member, the last one ripped when the set rips it (mitcad#487).
    fn build_preview_shape(&self, fields: Map<String, Value>)
    -> Result<Option<K::Shape>, ApiError>;

    /// The `build_host` query (mitcad#394): the component BUILD places
    /// parts in, its name, its path from the root (uids, "" for the root)
    /// and the active component; paths and points of the BUILD commands
    /// are from and in it.
    fn host_json(&self) -> Value;

    /// The `build_parts` query.
    fn build_parts_json(&self) -> Value;

    /// The `build_junctions` query.
    fn build_junctions_json(&self, query: &JunctionsQuery) -> Result<Value, ApiError>;

    /// The `build_cut_list` query.
    fn build_cut_list_json(&self, query: &CutListQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> BuildCrateOps<K> for Document<K> {
    fn run_build_command(&mut self, command: BuildCommand) -> Result<Value, ApiError> {
        Ok(match command {
            BuildCommand::BuildPlace(input) => {
                let placed = self.build_place(&input)?;
                let mut value = placed_json(&placed);
                // A set fastened to its hosts (mitcad#497): the junctions
                // given fasteners.
                if input.spacing.as_ref().is_some_and(|s| s.fastener.is_some())
                    && let Some(lead) = placed.set.first()
                {
                    value["fasteners"] = json!(self.set_fasteners(*lead).len());
                }
                value
            }
            // A timber frame laid out again (mitcad#479).
            BuildCommand::BuildEdit(input) if input.frame.is_some() => {
                let component = self.component_by_ref(&input.component)?;
                let frame = input.frame.clone().unwrap_or_default();
                let framed = self.edit_frame(component, &frame, input.config.as_ref())?;
                let mut answer = framed_json(&framed);
                answer["changed"] = json!(framed.changed);
                answer
            }
            BuildCommand::BuildFrame(input) => framed_json(&self.build_frame(&input)?),
            BuildCommand::BuildEdit(input) => {
                let changed = self.build_edit(&input)?;
                let mut answer = json!({"changed": changed});
                // A spacing set's members after it was laid out again.
                if input.spacing.is_some()
                    && let Ok(c) = self.component_by_ref(&input.component)
                {
                    let lead = self.part_info(c).and_then(|i| i.record.set).unwrap_or(c);
                    let members = self.set_members(lead);
                    answer["set"] = json!(members);
                    answer["count"] = json!(members.len());
                    answer["fasteners"] = json!(self.set_fasteners(lead).len());
                }
                answer
            }
            BuildCommand::BuildConnect(input) => {
                let placed = self.build_connect(&input)?;
                let mut value = placed_json(&placed);
                if input.joint.is_some() {
                    value["cuts"] = json!(self.joinery_json(&placed.features, &input.junction));
                }
                value
            }
            BuildCommand::BuildPanel(input) => placed_json(&self.build_panel(&input)?),
            BuildCommand::BuildRemove { component, set } => {
                let before = self.state().clone();
                let removed = self.build_remove_with(&component, set)?;
                let mut reply = json!({"removed": {
                    "components": removed.components,
                    "features": removed.features,
                    "detached": removed.detached,
                }});
                // The cut-scope exclusions it dropped (mitcad#1236).
                let dropped = self.state().dropped_exclusions(&before);
                if !dropped.is_empty() {
                    reply["notices"] = mitcad_model::notice::to_json(&dropped);
                }
                reply
            }
            BuildCommand::BuildSwapFasteners(input) => {
                let swapped = self.build_swap_fasteners(&input)?;
                json!({
                    "swapped": swapped.components,
                    "count": swapped.components.len(),
                    "designation": swapped.designation,
                    "sizes": swapped.sizes.iter().map(|(c, config, size)| json!({
                        "component": c,
                        "config": config,
                        "diameter": size.map(|s| s.0),
                        "length": size.map(|s| s.1),
                    })).collect::<Vec<_>>(),
                    "warnings": swapped.warnings,
                })
            }
            BuildCommand::BuildUpgrade {} => {
                let placed = self.build_upgrade()?;
                json!({"upgraded": placed, "count": placed.len()})
            }
        })
    }

    fn build_preview_json(&self, fields: Map<String, Value>) -> Result<Value, ApiError> {
        let problems = |e: mitcad_model::ModelError| json!({"problems": [e.to_string()]});
        // A timber frame's members (mitcad#479).
        if let Some(frame) = fields.get("frame") {
            let input: mitcad_model_build::FrameInput = super::parse(frame.clone(), "frame")?;
            let component = match fields.get("component").and_then(Value::as_str) {
                Some(c) => Some(self.component_by_ref(c)?),
                None => None,
            };
            return Ok(self
                .preview_frame(&input, component)
                .unwrap_or_else(problems));
        }
        Ok(match self.preview_input(fields)? {
            PreviewInput::Place(input) => match self.preview_place(&input) {
                Ok(plan) => {
                    let mut value = match self.place_preview_json(&input, &plan) {
                        Ok(value) => value,
                        Err(error) => return Ok(json!({"problems": [error.to_string()]})),
                    };
                    // A spacing set (mitcad#478): its members' frames.
                    if let Some(spacing) = &input.spacing {
                        match self.plan_set(&input, spacing) {
                            Ok(set) => {
                                let active = self.host_placement();
                                let frames: Vec<Value> = set
                                    .plans
                                    .iter()
                                    .map(|p| {
                                        let world = active.after(&p.placement);
                                        let end_a = p
                                            .mounts
                                            .get("end_a")
                                            .map_or([0.0; 3], |t| t.translation);
                                        frame_json(&mitcad_model::internal::build::frame(
                                            world.apply_point(end_a),
                                            world.apply_vector(mitcad_model::internal::build::X),
                                            world.apply_vector(mitcad_model::internal::build::Z),
                                        ))
                                    })
                                    .collect();
                                value["spacing"] = json!({
                                    "count": frames.len(),
                                    "frames": frames,
                                    "rip": set.rip.is_some(),
                                });
                            }
                            Err(e) => value = problems(e),
                        }
                    }
                    value
                }
                Err(e) => problems(e),
            },
            PreviewInput::Panel(input) => match self.plan_panel(&input) {
                Ok(plan) => {
                    let world = self.host_placement().after(&plan.placement);
                    json!({
                        "frame": frame_json(&world),
                        "place": "panel",
                        "fits": plan.fits,
                        "reason": if plan.reason.is_empty() { Value::Null } else { json!(plan.reason) },
                        "width": finite(plan.width),
                        "height": finite(plan.height),
                        "bound": plan.bounds.iter().map(|b| b.parameter.clone()).collect::<Vec<_>>(),
                        "designation": plan.designation,
                        "warnings": plan.warnings,
                        "body": "see analysis_shape",
                        "problems": [],
                    })
                }
                Err(e) => problems(e),
            },
            // Joinery and timber connections (mitcad#478).
            PreviewInput::Connect(input) if input.joint.is_some() => {
                let joint = input.joint.clone().unwrap_or_default();
                match self.plan_joinery(&input.junction, &joint) {
                    Ok((cuts, warnings)) => json!({
                        "place": "junction",
                        "junction": input.junction,
                        "cuts": cuts.iter().map(|c| json!({
                            "component": c.component,
                            "kind": c.cut.kind,
                            "size": c.cut.size.iter()
                                .map(|v| value_in(self, v, Unit::MM).ok())
                                .collect::<Vec<_>>(),
                        })).collect::<Vec<_>>(),
                        "warnings": warnings,
                        "problems": [],
                    }),
                    Err(e) => problems(e),
                }
            }
            PreviewInput::Connect(input) if self.is_wood_connect(&input) => {
                match self.plan_wood(&input) {
                    Ok(plan) => {
                        let active = self.host_placement();
                        let mut parts: Vec<String> =
                            plan.pieces.iter().map(|p| p.name.clone()).collect();
                        parts.extend(plan.on_lead.iter().map(|p| p.name.clone()));
                        json!({
                            "frame": plan.placement.map(|t| frame_json(&active.after(&t))),
                            "place": "junction",
                            "junction": plan.junction,
                            "fasteners": plan.frames.iter().map(|(name, t)| {
                                json!({"frame": name, "at": frame_json(&active.after(t))})
                            }).collect::<Vec<_>>(),
                            "fastener_parts": parts,
                            "size": plan.size.as_ref().map(|s| json!({
                                "config": s.config, "diameter": s.diameter, "length": s.length})),
                            "warnings": plan.warnings,
                            "body": "see analysis_shape",
                            "problems": [],
                        })
                    }
                    Err(e) => problems(e),
                }
            }
            PreviewInput::Connect(input) => match self.plan_connect(&input) {
                Ok(plan) => {
                    let active = self.host_placement();
                    let machining: Vec<Value> = plan
                        .holes
                        .iter()
                        .map(|h| {
                            json!({"component": h.component,
                                   "at": h.def["placement"]["plane"]["mount"],
                                   "point": h.def["placement"]["points"][0],
                                   "diameter": h.def["diameter"],
                                   "depth": h.def["extent"]["depth"],
                                   "thread": h.def["thread"]["designation"]})
                        })
                        .collect();
                    json!({
                        "frame": frame_json(&active.after(&plan.placement)),
                        "place": "junction",
                        "junction": plan.junction.id,
                        "fasteners": plan.fastener_frames.iter().map(|(name, t)| {
                            json!({"frame": name, "at": frame_json(&active.after(t))})
                        }).collect::<Vec<_>>(),
                        "fastener_parts": plan.fasteners.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
                        "machining": machining,
                        "warnings": plan.warnings,
                        "body": "see analysis_shape",
                        "problems": [],
                    })
                }
                Err(e) => problems(e),
            },
        })
    }

    fn ray_target(
        &self,
        start: mitcad_model::internal::build::V3,
        along: mitcad_model::internal::build::V3,
        except: Option<ComponentUid>,
        mode: BoundMode,
    ) -> Option<RayHit> {
        use mitcad_model::internal::build::{
            box_along, dot, in_box, moved_box, ray_plane, unit, z_axis,
        };
        let along = unit(along)?;
        let active = self.host_placement();
        let mut best: Option<(f64, Vec<OccurrenceUid>, GeomRef, f64)> = None;
        for info in self.build_parts_info() {
            if Some(info.component) == except {
                continue;
            }
            let (Some(path), Some(part), Some(bounds)) = (
                self.placed_in_host(info.component),
                info.part,
                self.part_bounds(info.component),
            ) else {
                continue;
            };
            let world = active.after(&self.path_transform(&path));
            let world_box = moved_box(&bounds, &world);
            for (name, local) in &info.mounts {
                let kind = info.frame_kind(name);
                if kind != "slot" && kind != "end" && !info.is_side(name) {
                    continue;
                }
                let frame = world.after(local);
                let n = z_axis(&frame);
                if dot(n, along) > -1e-6 {
                    continue;
                }
                let Some(t) = ray_plane(start, along, frame.translation, n) else {
                    continue;
                };
                if t <= 1e-6 {
                    continue;
                }
                let point = mitcad_model::internal::build::add(
                    start,
                    mitcad_model::internal::build::scale(along, t),
                );
                if !in_box(point, &world_box, 1e-3) {
                    continue;
                }
                if best.as_ref().is_none_or(|(d, ..)| t < *d) {
                    let far = box_along(&bounds, &world, start, along).1;
                    let value = match mode {
                        BoundMode::Face | BoundMode::ToFace => t,
                        BoundMode::Centre => (t + far) / 2.0,
                        BoundMode::Through => far,
                    };
                    best = Some((
                        t,
                        path.clone(),
                        GeomRef::Mount {
                            feature: part,
                            name: name.clone(),
                        },
                        value,
                    ));
                }
            }
        }
        best.map(|(_, path, geometry, value)| (path, geometry, value))
    }

    fn build_preview_shape(
        &self,
        fields: Map<String, Value>,
    ) -> Result<Option<K::Shape>, ApiError> {
        // The source evaluated once per set of values: a set's members
        // share them but for their lengths.
        let mut evaluations = Vec::new();
        let mut shapes = match self.preview_input(fields)? {
            PreviewInput::Place(input) => match &input.spacing {
                Some(spacing) => {
                    let set = self.plan_set(&input, spacing)?;
                    let last = set.plans.len().saturating_sub(1);
                    let mut shapes = Vec::new();
                    for (i, plan) in set.plans.iter().enumerate() {
                        let rip = if i == last { set.rip.as_ref() } else { None };
                        shapes.extend(self.placing_preview_bodies(
                            &input,
                            plan,
                            rip,
                            &mut evaluations,
                        )?);
                    }
                    shapes
                }
                None => {
                    let plan = self.preview_place(&input)?;
                    self.placing_preview_bodies(&input, &plan, None, &mut evaluations)?
                }
            },
            PreviewInput::Panel(input) => {
                let plan = self.plan_panel(&input)?;
                self.preview_bodies(
                    &plan.panel.source,
                    &plan.panel.values,
                    plan.placement,
                    PreviewCuts::default(),
                    &mut evaluations,
                )?
            }
            PreviewInput::Connect(input) => {
                if input.joint.is_some() {
                    return Ok(None);
                }
                if self.is_wood_connect(&input) {
                    let plan = self.plan_wood(&input)?;
                    match (plan.lead, plan.placement) {
                        (Some(lead), Some(placement)) => self.preview_bodies(
                            &lead.source,
                            &BTreeMap::new(),
                            placement,
                            PreviewCuts::default(),
                            &mut evaluations,
                        )?,
                        _ => return Ok(None),
                    }
                } else {
                    let plan = self.plan_connect(&input)?;
                    self.preview_bodies(
                        &plan.connector.source,
                        &BTreeMap::new(),
                        plan.placement,
                        PreviewCuts::default(),
                        &mut evaluations,
                    )?
                }
            }
        };
        Ok(match shapes.len() {
            0 => None,
            1 => shapes.pop(),
            _ => Some(
                self.kernel()
                    .compound(&shapes)
                    .map_err(|e| ApiError(e.to_string()))?,
            ),
        })
    }

    fn host_json(&self) -> Value {
        let a = self.assembly();
        let host = self.build_host();
        let path = if host.is_root() {
            String::new()
        } else {
            a.paths_to(host)
                .first()
                .map(|p| path_text(p))
                .unwrap_or_default()
        };
        json!({
            "component": host,
            "name": a.name(host),
            "path": path,
            "active": a.active,
        })
    }

    fn build_parts_json(&self) -> Value {
        let params = self.parameters();
        let angle = |id: ParamId| params.value(id);
        let parts: Vec<Value> = self
            .build_parts_info()
            .into_iter()
            .map(|info| {
                let values = info
                    .def
                    .as_ref()
                    .map(|d| mitcad_model::internal::build::part_values(d, params))
                    .unwrap_or_default();
                let config = info.config();
                let designation = info
                    .source
                    .as_ref()
                    .map(|s| mitcad_model::internal::build::designation(s, config.as_deref(), &values));
                let part_number = info
                    .source
                    .as_ref()
                    .map(|s| mitcad_model::internal::build::part_number(s, config.as_deref()));
                let bound = info.record.bound.iter().find_map(|uid| {
                    let entry = self.feature(*uid)?;
                    let FeatureDef::BoundLength(b) = &entry.def else {
                        return None;
                    };
                    Some(json!({
                        "feature": uid,
                        "parameter": params.name(b.parameter),
                        "to": {"occurrence": b.to.occurrence.to_string(), "geometry": b.to.geometry},
                        "mode": b.mode.as_str(),
                        // The offset of a length to a face (mitcad#1630),
                        // else the overlap or gap.
                        "add": b.add.and_then(|id| params.value(id)).unwrap_or(0.0),
                    }))
                });
                let (flip, rotate) = match info.record.joint.and_then(|j| self.feature(j)) {
                    Some(entry) => match &entry.def {
                        FeatureDef::Joint(j) => (
                            matches!(&j.a.geometry, GeomRef::Mount { name, .. } if name == "end_b"),
                            j.angle.and_then(|id| params.value(id)).unwrap_or(0.0),
                        ),
                        _ => (false, 0.0),
                    },
                    None => (false, 0.0),
                };
                let machining: Vec<Value> = self
                    .part_machining(&info)
                    .into_iter()
                    .filter(|(_, m)| m.connector.is_none())
                    .map(|(_, m)| {
                        json!({
                            "end": if m.at == "end_b" { "b" } else { "a" },
                            "hole": {"diameter": m.diameter, "depth": m.depth, "thread": m.thread},
                        })
                    })
                    .collect();
                let mut part = json!({
                    "component": info.component,
                    "name": info.name,
                    "role": info.record.role.as_str(),
                    "source": info.record.source,
                    "part": info.part,
                    "joint": info.record.joint,
                    "designation": designation,
                    "supplier": info.source.as_ref().map(|s| s.supplier.clone()),
                    "part_number": part_number,
                    "config": config,
                    "length": info.length.map(|(_, l)| l),
                    "ends": info.ends().map(|e| ends_json(e, &angle)),
                    "bound": bound,
                    "flip": flip,
                    "rotate": rotate,
                    "tilt": info.record.tilt.unwrap_or(0.0),
                    "added_by": info.record.added_by,
                    "adds": info.record.adds,
                    "junction": info.record.junction,
                    "from": info.record.from.map(|f| f.as_str()),
                    "machining": machining,
                    "seating": self.wood_seating_json(&info),
                    // Timber construction (mitcad#478).
                    "kind": info.is_timber().then_some("timber"),
                    "grade": info.record.grade.clone().or_else(|| {
                        info.meta.material.as_ref().map(|m| m.grade.clone()).filter(|g| !g.is_empty())
                    }),
                    "set": info.record.set,
                    "spacing": info.record.spacing.as_ref().map(|s| json!({
                        "definition": s["spacing"], "count": s["count"]})),
                    "cuts": info.def.as_ref().map(|d| d.cuts.iter().map(|c| json!({
                        "kind": c.kind, "with": c.with, "junction": c.junction,
                        "size": c.size.map(|id| params.value(id)),
                    })).collect::<Vec<_>>()).unwrap_or_default(),
                    // A profiled board's (mitcad#477).
                    "board": info.meta.board.as_ref().map(|b| json!({
                        "profile": b.profile, "cover": info.meta.cover_width(),
                        "joined": b.is_joined(),
                    })),
                    // Timber frames (mitcad#479).
                    "frame": info.record.frame,
                    // The frame's own component (mitcad#522).
                    "frame_component": info.record.frame.and_then(|l| self.frame_component(l)),
                    "member": info.record.member,
                    "framing": info.record.framing.as_ref().map(|f| json!({
                        "definition": f["frame"], "count": f["count"]})),
                });
                // Centred between these profiles (mitcad#1401).
                part["gap"] = json!(info.record.gap);
                part
            })
            .collect();
        json!({"parts": parts})
    }

    fn build_junctions_json(&self, query: &JunctionsQuery) -> Result<Value, ApiError> {
        let only = match &query.components {
            Some(list) => Some(
                list.iter()
                    .map(|c| self.component_ref(c))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            None => None,
        };
        let active = self.host_placement();
        let side = |s: &mitcad_model_build::JunctionSide| {
            // The part's own axes at end A.
            let world = active.after(&self.path_transform(&s.occurrence));
            let end_a = self
                .part_info(s.component)
                .and_then(|i| i.mount("end_a").copied())
                .map_or([0.0; 3], |t| t.translation);
            let own = mitcad_model::internal::build::frame(
                world.apply_point(end_a),
                world.apply_vector(mitcad_model::internal::build::X),
                world.apply_vector(mitcad_model::internal::build::Z),
            );
            json!({
                "component": s.component,
                "occurrence": path_text(&s.occurrence),
                "at": s.at,
                "frame": frame_json(&own),
                "at_frame": frame_json(&s.frame),
            })
        };
        let junctions: Vec<Value> = self
            .build_junctions(only.as_deref())
            .iter()
            .map(|j| {
                let mut value = json!({
                    "id": j.id,
                    "kind": j.kind,
                    "a": side(&j.a),
                    "b": side(&j.b),
                    "angle": j.angle,
                    "systems": j.systems.iter().map(|s| s.as_ref().map(|s| s.label())).collect::<Vec<_>>(),
                    "space": {"x": j.space[0], "y": j.space[1], "z": j.space[2]},
                    "connectors": j.connectors,
                });
                // Two timber members (mitcad#478): how fasteners go.
                let timber = |c| self.part_info(c).is_some_and(|i| i.is_timber());
                if timber(j.a.component)
                    && timber(j.b.component)
                    && let Some(f) = self.fastening(j)
                {
                    value["wood"] = mitcad_model_build::internal::wood_json(&f, j);
                }
                value
            })
            .collect();
        Ok(json!({"junctions": junctions}))
    }

    fn build_cut_list_json(&self, query: &CutListQuery) -> Result<Value, ApiError> {
        let kerf = query.kerf.unwrap_or(CutListOptions::default().kerf);
        if !(kerf.is_finite() && kerf >= 0.0) {
            return Err(ApiError(format!(
                "the kerf must be zero or more, got {kerf}"
            )));
        }
        let list = self.build_cut_list(CutListOptions { kerf });
        let end = |e: &Option<(f64, mitcad_model::features::derived_part::Tilt)>| match e {
            Some((angle, tilt)) => json!({"angle": angle, "tilt": tilt.as_str()}),
            None => Value::Null,
        };
        let has_timber = list.timber.is_some();
        let profiled = list.timber.is_some_and(|t| t.area.is_some());
        let rows: Vec<Value> = list
            .rows
            .iter()
            .map(|r| {
                let mut row = json!({
                    "source": r.source,
                    "designation": r.designation,
                    "supplier": r.supplier,
                    "part_number": r.part_number,
                    "config": r.config,
                    "length": r.length,
                    "ends": {"a": end(&r.ends[0]), "b": end(&r.ends[1])},
                    "machining": r.machining.iter().map(|m| match &m.kind {
                        // Joinery (mitcad#478).
                        Some(kind) => json!({
                            "at": null, "kind": kind, "size": m.size, "with": m.with,
                            "depth": m.depth, "diameter": null, "thread": null,
                            "connector": null,
                        }),
                        None => json!({
                            "at": m.at, "diameter": m.diameter, "depth": m.depth,
                            "thread": m.thread, "connector": m.connector,
                        }),
                    }).collect::<Vec<_>>(),
                    "features": r.features,
                    "quantity": r.quantity,
                    "mass": r.mass,
                    "components": r.components,
                    // A timber frame's members by frame (mitcad#522).
                    "frame": r.frame,
                });
                // A section of a typed size (mitcad#1328): its options.
                if let Some(size) = &r.size {
                    row["size"] = json!(size);
                }
                // The timber columns, only in a design with timber.
                if has_timber {
                    let t = r.timber.as_ref();
                    row["kind"] = json!(t.map(|_| "timber"));
                    row["material"] = json!(t.map(|t| json!({
                        "species": t.material.species, "finish": t.material.finish,
                        "grade": t.grade, "density": t.material.density.as_ref()
                            .and_then(|d| d.millimetres()),
                    })));
                    row["nominal"] = json!(t.and_then(|t| t.nominal).map(|n| json!({
                        "width": n[0], "height": n[1]})));
                    row["section"] = json!(t.map(|t| json!({
                        "width": t.section[0], "height": t.section[1]})));
                    row["availability"] = json!(t.and_then(|t| t.availability.clone()));
                    row["running_metres"] = json!(t.map(|t| t.running_metres));
                    row["volume"] = json!(t.map(|t| t.volume));
                }
                // Profiled boards' columns (mitcad#477), only in a design
                // with profiled boards.
                if profiled {
                    let b = r.timber.as_ref().and_then(|t| t.board.as_ref());
                    row["profile"] = json!(b.map(|b| b.profile.clone()));
                    row["cover"] = json!(b.map(|b| b.cover));
                    row["area"] = json!(b.map(|b| b.area));
                }
                row
            })
            .collect();
        let stock: Vec<Value> = list
            .stock
            .iter()
            .map(|s| {
                let mut entry = json!({
                    "source": s.source,
                    "designation": s.designation,
                    "stock_length": s.stock_length,
                    "pieces": s.pieces,
                    "bars": s.bars,
                    "needed": s.needed,
                    "offcut": s.offcut,
                    "too_long": s.too_long,
                });
                if s.config.is_some() {
                    entry["config"] = json!(s.config);
                }
                if !s.stock_lengths.is_empty() {
                    entry["stock_lengths"] = json!(s.stock_lengths);
                }
                entry
            })
            .collect();
        let other: Vec<Value> = self.parts_list_json()["rows"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| {
                row["component"]
                    .as_str()
                    .and_then(|c| c.parse::<ComponentUid>().ok())
                    .is_none_or(|c| !list.cut.contains(&c))
            })
            .collect();
        let mut answer = json!({"rows": rows, "stock": stock, "kerf": kerf, "other": other});
        if let Some(t) = &list.timber {
            answer["timber"] = json!(true);
            answer["totals"] = json!({
                "running_metres": t.running_metres, "volume": t.volume, "mass": t.mass});
            if let Some(area) = t.area {
                answer["profiled"] = json!(true);
                answer["totals"]["area"] = json!(area);
            }
        }
        Ok(answer)
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait BuildLocal<K: Kernel> {
    /// The joinery cuts of a junction in derived parts (mitcad#478):
    /// `{component, kind, size}`.
    fn joinery_json(&self, parts: &[mitcad_model::ids::FeatureUid], junction: &str) -> Vec<Value>;

    /// Reads a preview request (`build_preview`, `analysis_shape`):
    /// build_place's, build_panel's or build_connect's fields.
    fn preview_input(&self, fields: Map<String, Value>) -> Result<PreviewInput, ApiError>;

    /// What a placing's ray meets and the new part's length: a length
    /// given is the value; else what the ray gives, else the source's
    /// default. The preview's shape is cut to the same length.
    fn place_preview_length(
        &self,
        input: &PlaceInput,
        plan: &Plan,
    ) -> Result<(Option<RayHit>, Option<f64>), ApiError>;

    /// A placing's preview.
    fn place_preview_json(&self, input: &PlaceInput, plan: &Plan) -> Result<Value, ApiError>;

    /// A placing's bodies in the design's coordinates, as the preview shows
    /// them: a bound length (or none given) is the ray's, as the query
    /// says, not the source's default; `rip` a set's last member's rip.
    fn placing_preview_bodies(
        &self,
        input: &PlaceInput,
        plan: &Plan,
        rip: Option<&PartCut<ValueInput>>,
        evaluations: &mut Evaluations<K::Shape>,
    ) -> Result<Vec<K::Shape>, ApiError>;

    /// A source's bodies at `values`, placed by `placement` in the BUILD
    /// host, with `cuts`; the source evaluated once per values in
    /// `evaluations`.
    fn preview_bodies(
        &self,
        source: &SourceUse,
        values: &BTreeMap<String, ValueInput>,
        placement: Transform,
        cuts: PreviewCuts<'_>,
        evaluations: &mut Evaluations<K::Shape>,
    ) -> Result<Vec<K::Shape>, ApiError>;

    /// Physical head-top distance from a wood fastener's unchanged entry
    /// face. Positive is sunk into the member; negative stands proud.
    fn wood_seating_json(&self, info: &mitcad_model::internal::build::PartInfo) -> Option<Value>;
}

impl<K: Kernel> BuildLocal<K> for Document<K> {
    fn joinery_json(&self, parts: &[mitcad_model::ids::FeatureUid], junction: &str) -> Vec<Value> {
        let params = self.parameters();
        let mut out = Vec::new();
        for part in parts {
            let Some(entry) = self.feature(*part) else {
                continue;
            };
            let FeatureDef::DerivedPart(d) = &entry.def else {
                continue;
            };
            for c in d
                .cuts
                .iter()
                .filter(|c| c.junction.as_deref() == Some(junction))
            {
                out.push(json!({
                    "component": entry.component,
                    "kind": c.kind,
                    "size": c.size.map(|id| params.value(id)),
                }));
            }
        }
        out
    }

    fn preview_input(&self, mut fields: Map<String, Value>) -> Result<PreviewInput, ApiError> {
        fields.remove("query");
        fields.remove("shape");
        let value = Value::Object(fields);
        if value.get("junction").is_some()
            || value.get("end").is_some()
            || value.get("joint").is_some()
        {
            return super::parse(value, "build_preview")
                .map(|input| PreviewInput::Connect(Box::new(input)));
        }
        let part: PartInput = super::parse(value["part"].clone(), "part")?;
        let panel = self
            .source_use(&part)
            .is_ok_and(|s| s.meta.role == BuildRole::Panel);
        if panel {
            super::parse(value, "build_preview").map(|input| PreviewInput::Panel(Box::new(input)))
        } else {
            super::parse(value, "build_preview").map(|input| PreviewInput::Place(Box::new(input)))
        }
    }

    fn place_preview_length(
        &self,
        input: &PlaceInput,
        plan: &Plan,
    ) -> Result<(Option<RayHit>, Option<f64>), ApiError> {
        let world = self.host_placement().after(&plan.placement);
        let flip = input.flip;
        let touch = plan
            .mounts
            .get(if flip { "end_b" } else { "end_a" })
            .copied();
        let start = touch.map_or(world.translation, |t| world.apply_point(t.translation));
        let along = world.apply_vector(if flip {
            [0.0, 0.0, -1.0]
        } else {
            mitcad_model::internal::build::Z
        });
        let mode = match &plan.length {
            LengthPlan::Bound { mode, .. } => *mode,
            _ => BoundMode::Face,
        };
        let hit = self.ray_target(
            start,
            along,
            plan.target.as_ref().map(|t| t.info.component),
            mode,
        );
        // A member by two points without a length is as long as their
        // distance, whatever the ray meets (mitcad#478); the ray still
        // names the target an auto length would bind to.
        let given = matches!(
            input.length,
            Some(mitcad_model_build::LengthInput::Value(_))
        ) || (input.to.is_some() && input.length.is_none());
        let mut length_value = match (&plan.length, &hit) {
            (LengthPlan::None, _) => None,
            (LengthPlan::Bound { first, .. }, _) => Some(*first),
            (LengthPlan::Value(_, v), _) if given => Some(*v),
            (_, Some(hit)) => Some(hit.2),
            (LengthPlan::Value(_, v), None) => Some(*v),
        };
        if let (Some(meta), Some(length)) = (&plan.source.meta.length, length_value) {
            let mut values = plan.values.clone();
            values.insert(meta.parameter.clone(), ValueInput::Number(length));
            let mut chosen = mitcad_model_build::internal::source_option_values(
                self,
                &plan.source.def,
                &values,
            )?;
            if matches!(plan.length, LengthPlan::Bound { .. }) {
                chosen.pinned.insert(meta.parameter.clone());
            }
            let configured = mitcad_model_build::internal::source_state_with_options(
                &plan.source.def,
                plan.source.config.as_deref(),
                &chosen,
            )
            .map_err(ApiError)?;
            let parameter = configured
                .state
                .parameters
                .find(&meta.parameter)
                .ok_or_else(|| ApiError("the length parameter is missing".into()))?;
            length_value = Some(
                configured
                    .state
                    .parameters
                    .value(parameter)
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| ApiError("the configured length is not finite".into()))?,
            );
        }
        Ok((hit, length_value))
    }

    fn place_preview_json(&self, input: &PlaceInput, plan: &Plan) -> Result<Value, ApiError> {
        let active = self.host_placement();
        let world = active.after(&plan.placement);
        let end_a = plan.mounts.get("end_a").map_or([0.0; 3], |t| t.translation);
        let frame = mitcad_model::internal::build::frame(
            world.apply_point(end_a),
            world.apply_vector(mitcad_model::internal::build::X),
            world.apply_vector(mitcad_model::internal::build::Z),
        );
        let params = |c: &Option<EndCut<ValueInput>>| match c {
            Some(c) => json!({
                "angle": value_in(self, &c.angle, Unit::DEG).ok(),
                "tilt": c.tilt.unwrap_or_default().as_str(),
            }),
            None => Value::Null,
        };
        let mut ends = json!({"a": params(&plan.own_ends.a), "b": params(&plan.own_ends.b)});
        if let Some((part, b, cut)) = &plan.target_ends {
            ends["target"] = json!({
                "part": part,
                "end": if *b { "b" } else { "a" },
                "angle": value_in(self, &cut.angle, Unit::DEG).ok(),
                "tilt": cut.tilt.unwrap_or_default().as_str(),
            });
        }
        let bound_to = match &plan.length {
            LengthPlan::Bound { to, .. } => Some((**to).clone()),
            _ => None,
        };
        let (hit, length_value) = self.place_preview_length(input, plan)?;
        let mode = match &plan.length {
            LengthPlan::Bound { mode, .. } => *mode,
            _ => BoundMode::Face,
        };
        let given = matches!(
            input.length,
            Some(mitcad_model_build::LengthInput::Value(_))
        );
        let default = match &plan.length {
            LengthPlan::Value(_, v) if !given => Some(*v),
            LengthPlan::Bound { source_default, .. } => Some(*source_default),
            _ => None,
        };
        let target = match (bound_to, &hit) {
            (Some(to), _) => {
                json!({"occurrence": to.occurrence.to_string(), "geometry": to.geometry})
            }
            (None, Some((path, geometry, _))) => {
                json!({"occurrence": path_text(path), "geometry": geometry})
            }
            (None, None) => Value::Null,
        };
        // Snaps along the slot of a side placement (mitcad#394): where the
        // new part's near face may go, and the point on the slot where the
        // part then stands (its middle along the slot).
        let mut snaps = Vec::new();
        let mut snapped = Value::Null;
        if plan.place == "side"
            && let (Some(target), Spot::Joint(joint)) = (&plan.target, &plan.spot)
            && let GeomRef::Mount { name, .. } = &joint.b.geometry
            && let Some(slot) = target.info.mount(name)
            && let Some(side) = &joint.side
        {
            let to_world = active.after(&target.placement).after(slot);
            let middle = (side.near + side.far) / 2.0;
            let at = |offset: f64| to_world.apply_point([offset - side.near + middle, 0.0, 0.0]);
            for (kind, offset) in side.snaps() {
                snaps.push(json!({"kind": kind, "offset": offset, "point": at(offset)}));
            }
            // The reference the placement is bound to and the offset from
            // it, as `at.from` and `at.offset` take them (mitcad#417).
            snapped = json!({"kind": side.snapped, "from": side.from.as_str(),
                             "offset": side.from_offset, "point": at(side.offset),
                             "width": side.far - side.near,
                             "long_point": side.long_point,
                             "foot": active.apply_point(joint.frame_b.translation),
                             "normal": active.apply_vector(mitcad_model::internal::build::z_axis(&to_world)),
                             "along": active.apply_vector(mitcad_model::internal::build::x_axis(&to_world))});
            // The free stretch it stands in (mitcad#1401): its ends from
            // end A and the profiles around it, for a change of From.
            snapped["gap"] = match &side.gap {
                Some(g) => json!({"start": g.start, "end": g.end,
                                  "a": g.a.as_ref().map(|s| s.component),
                                  "b": g.b.as_ref().map(|s| s.component)}),
                None => Value::Null,
            };
        }
        // On an end (mitcad#417): straight or turned, the turns the panel
        // offers, a corner's offset and the mitre it gets.
        let mut end = Value::Null;
        if let Spot::Joint(joint) = &plan.spot
            && let Some(e) = &joint.end
        {
            let host = |v: mitcad_model::internal::build::V3| active.apply_vector(v);
            let mitre = match &e.mitre {
                Some(m) => {
                    let cut = match m.cut {
                        MitreCut::New => "new",
                        MitreCut::Existing => "existing",
                        MitreCut::Shared => "shared",
                        MitreCut::None => "none",
                    };
                    let own = plan.own_ends.a.as_ref().or(plan.own_ends.b.as_ref());
                    let angle = match m.cut {
                        MitreCut::Existing => plan.target_ends.as_ref().map(|(_, _, c)| &c.angle),
                        _ => own.map(|c| &c.angle),
                    };
                    json!({"cut": cut,
                           "angle": angle.and_then(|a| value_in(self, a, Unit::DEG).ok())})
                }
                None => json!({"cut": "none", "angle": null}),
            };
            end = json!({
                "zone": e.zone,
                "direction": host(e.direction),
                "turns": e.turns.map(host),
                "offset": e.offset,
                "mitre": mitre,
            });
        }
        // A corner the bound length closes (mitcad#485).
        let closing = match &plan.closing {
            Some(c) => json!({
                "component": c.component,
                "end": if c.end_b { "b" } else { "a" },
                "mitre": if c.cut.is_some() { "shared" } else { "none" },
                "angle": c.cut.as_ref().and_then(|cut| value_in(self, &cut.angle, Unit::DEG).ok()),
            }),
            None => Value::Null,
        };
        // A member by two points (mitcad#509): what each point is on, the
        // ends cut back to a member, and whether it lies on a plane.
        let points = match &plan.between {
            Some(b) => {
                let point = |i: usize| match &b.on[i] {
                    Some((component, name)) => {
                        json!({"on": component, "name": name, "cut": b.cut[i]})
                    }
                    None => json!({"on": null, "name": null, "cut": b.cut[i]}),
                };
                json!({"start": point(0), "end": point(1),
                       "lies": if b.plane { "plane" } else { "free" }})
            }
            None => Value::Null,
        };
        Ok(json!({
            "frame": frame_json(&frame),
            "place": plan.place,
            "closing": closing,
            "points": points,
            "length": {
                "value": length_value,
                "default": default,
                "target": target,
                "mode": mode.as_str(),
            },
            "ends": ends,
            "tilt": plan.tilt,
            "snaps": snaps,
            "side": snapped,
            "end": end,
            "host": self.host_json(),
            "designation": plan.designation,
            "box": plan.bounds.as_ref().map(|b| json!({"min": b.min, "max": b.max})),
            "warnings": plan.warnings,
            "body": "see analysis_shape",
            "problems": [],
        }))
    }

    fn placing_preview_bodies(
        &self,
        input: &PlaceInput,
        plan: &Plan,
        rip: Option<&PartCut<ValueInput>>,
        evaluations: &mut Evaluations<K::Shape>,
    ) -> Result<Vec<K::Shape>, ApiError> {
        let mut values = plan.values.clone();
        let mut cuts = PreviewCuts {
            rip,
            bound_length: matches!(plan.length, LengthPlan::Bound { .. }),
            ..PreviewCuts::default()
        };
        if let (Some(meta), (_, Some(length))) = (
            &plan.source.meta.length,
            self.place_preview_length(input, plan)?,
        ) {
            values.insert(meta.parameter.clone(), ValueInput::Number(length));
            let end = if input.flip { "end_b" } else { "end_a" };
            cuts.touch = plan.mounts.get(end).map(|t| (end, t.translation));
            // Its end cuts as the part will have them (mitres, a tilt's
            // foot, mitcad#443 and #444).
            cuts.ends = Some((&plan.own_ends, length));
        }
        self.preview_bodies(&plan.source, &values, plan.placement, cuts, evaluations)
    }

    fn preview_bodies(
        &self,
        source: &SourceUse,
        values: &BTreeMap<String, ValueInput>,
        mut placement: Transform,
        cuts: PreviewCuts<'_>,
        evaluations: &mut Evaluations<K::Shape>,
    ) -> Result<Vec<K::Shape>, ApiError> {
        let PreviewCuts {
            touch,
            ends,
            rip,
            bound_length,
        } = cuts;
        let mut chosen =
            mitcad_model_build::internal::source_option_values(self, &source.def, values)?;
        if bound_length && let Some(meta) = &source.meta.length {
            chosen.pinned.insert(meta.parameter.clone());
        }
        let configured = mitcad_model_build::internal::source_state_with_options(
            &source.def,
            source.config.as_deref(),
            &chosen,
        )
        .map_err(ApiError)?;
        let normalized = chosen.normalized(&configured).map_err(ApiError)?;
        let known = evaluations
            .iter()
            .position(|(options, _)| *options == normalized);
        let index = match known {
            Some(i) => i,
            None => {
                let evaluated =
                    evaluate_source(self.kernel(), &configured.state).map_err(ApiError)?;
                evaluations.push((normalized, evaluated));
                evaluations.len() - 1
            }
        };
        let evaluated = &evaluations[index].1;
        if let Some((end, was)) = touch
            && let Some(now) = evaluated.mounts.get(end)
        {
            let shift = mitcad_vec3::sub(was, now.translation);
            placement = placement.after(&Transform::translation(shift));
        }
        let world = self.host_placement().after(&placement);
        let kernel = self.kernel();
        let failed = |e: mitcad_model::kernel::KernelError| ApiError(e.to_string());
        let section = mitcad_model_build::internal::bounds_of(kernel, &evaluated.bodies);
        // A set's last board ripped (mitcad#478): its box as the derived
        // part cuts it.
        let rip_tool = match rip {
            Some(cut) => {
                let mm = |v: &[ValueInput; 3]| -> Result<[f64; 3], ApiError> {
                    Ok([
                        value_in(self, &v[0], Unit::MM)?,
                        value_in(self, &v[1], Unit::MM)?,
                        value_in(self, &v[2], Unit::MM)?,
                    ])
                };
                let size = mm(&cut.size)?;
                let spec = mitcad_model::transform::PrimitiveSpec {
                    feature: mitcad_model::ids::FeatureUid(0),
                    frame: mitcad_model::profile::SketchFrame {
                        origin: mm(&cut.origin)?,
                        x_axis: cut.x,
                        y_axis: cut.y,
                    },
                    shape: mitcad_model::transform::PrimitiveShape::Box {
                        length: size[0],
                        width: size[1],
                        height: size[2],
                    },
                };
                Some(kernel.primitive(&spec).map_err(failed)?)
            }
            None => None,
        };
        let mut shapes = Vec::new();
        for body in &evaluated.bodies {
            let mut body = body.clone();
            if let Some((cuts, length)) = &ends {
                for b in [false, true] {
                    let Some(cut) = cuts.end(b) else { continue };
                    let angle = value_in(self, &cut.angle, Unit::DEG)?;
                    if angle.abs() <= 1e-12 {
                        continue;
                    }
                    let Some(plane) = mitcad_model::internal::features::end_plane(
                        section.as_ref(),
                        b,
                        *length,
                        angle,
                        cut.tilt.unwrap_or_default(),
                    ) else {
                        continue;
                    };
                    let tool = mitcad_model::internal::features::end_tool(
                        kernel,
                        mitcad_model::ids::FeatureUid(0),
                        b,
                        *length,
                        plane,
                    )
                    .map_err(failed)?;
                    let output = kernel
                        .boolean(mitcad_model::kernel::BooleanOp::Cut, &[&body], &tool)
                        .map_err(failed)?;
                    if let Some(piece) = output.pieces.into_iter().find(|p| p.sources.contains(&0))
                    {
                        body = piece.shape;
                    }
                }
            }
            if let Some(tool) = &rip_tool {
                let output = kernel
                    .boolean(mitcad_model::kernel::BooleanOp::Cut, &[&body], tool)
                    .map_err(failed)?;
                if let Some(piece) = output.pieces.into_iter().find(|p| p.sources.contains(&0)) {
                    body = piece.shape;
                }
            }
            shapes.push(
                kernel
                    .transform_shape(&body, &world, None)
                    .map_err(failed)?,
            );
        }
        Ok(shapes)
    }

    fn wood_seating_json(&self, info: &mitcad_model::internal::build::PartInfo) -> Option<Value> {
        info.meta.wood.as_ref()?;
        info.record.junction.as_ref()?;
        let joint = info.record.joint?;
        let entry = self.entry_point(joint)?;
        let member = self.member(info.component)?;
        let axis = info
            .mounts
            .iter()
            .find(|(name, _)| {
                *name == "axis" || info.meta.frame(name).is_some_and(|f| f.kind == "shaft")
            })?
            .1;
        let local_z = mitcad_model::internal::build::z_axis(axis);
        // A box gives an exact axial extent only for an axis-aligned
        // source. Oblique sources can seat by a named frame, but need a
        // directional kernel extent query for this physical measurement.
        if !mitcad_model_build::internal::shaft_axis_is_aligned(local_z) {
            return None;
        }
        let z = member.placement.apply_vector(local_z);
        let (distance, tip) = member
            .bounds
            .corners()
            .map(|p| {
                mitcad_model::internal::build::dot(
                    mitcad_model::internal::build::sub(member.placement.apply_point(p), entry),
                    z,
                )
            })
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            });
        // The junction's members its shaft passes through (mitcad#521).
        let junction = info.record.junction.as_deref().unwrap_or_default();
        let (met, of) = self.shaft_members(junction, entry, z, distance, tip);
        Some(json!({"head_top_distance": distance, "tip_distance": tip,
                    "members": met, "of": of}))
    }
}

/// A number for JSON, null when it is not finite.
fn finite(v: f64) -> Value {
    if v.is_finite() { json!(v) } else { Value::Null }
}
