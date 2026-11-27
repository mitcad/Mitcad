// SPDX-License-Identifier: MIT
//! Exploded views of assemblies (mitcad#2113, `mitcad_model::explode`; see
//! `commands.md`, "Exploded views"): the commands `add_explode_view`,
//! `edit_explode_view`, `remove_explode_view` and `auto_explode`, the
//! queries `explode_views` and `explode_view`, and the warning a command
//! gives when it deletes an occurrence an explode view moved.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::assembly::matrix4;
use mitcad_model::datum::Vec3;
use mitcad_model::explode::{ExplodeAxis, ExplodeStep, ExplodeView, value_text};
use mitcad_model::features::{GeomRef, OccurrencePath};
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, ExplodePart, ExplodeState};

/// A value in a command: a number (millimetres or degrees) or an
/// expression (`40 mm`, `2 * gap`, `90 deg`).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum ValueIn {
    Number(f64),
    Text(String),
}

impl ValueIn {
    fn text(&self, unit: &str) -> Result<String, ApiError> {
        match self {
            Self::Number(v) if v.is_finite() => Ok(value_text(*v, unit)),
            Self::Number(_) => Err(ApiError("a value is not finite".to_owned())),
            Self::Text(t) if t.trim().is_empty() => Err(ApiError("a value is empty".to_owned())),
            Self::Text(t) => Ok(t.trim().to_owned()),
        }
    }
}

/// A direction or an axis in a command: `[x, y, z]`, an axis of the view's
/// component (`"x"`, `"-y"`, ...), or `{"vector": [...]}` /
/// `{"occurrence": "Arm:1", "geometry": {...}}` with `"flip"`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum AxisIn {
    Vector(Vec3),
    Named(String),
    Object(Box<AxisObject>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AxisObject {
    #[serde(default)]
    vector: Option<Vec3>,
    /// A path from the view's component by uids or names.
    #[serde(default)]
    occurrence: String,
    #[serde(default)]
    geometry: Option<GeomRef>,
    #[serde(default)]
    flip: bool,
    /// With geometry (mitcad#2328): the way it should point, a vector in
    /// the view component's coordinates (the Explode Step panel's arrow);
    /// `flip` is set so that the geometry's direction points along it, not
    /// kept itself.
    #[serde(default)]
    toward: Option<Vec3>,
}

/// A step in a command; in `edit_explode_view`'s `set`, only what is given
/// changes (`"move": false` and `"turn": false` take a step's move or turn
/// away).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StepIn {
    #[serde(default)]
    name: Option<String>,
    /// Paths from the view's component by uids or names.
    #[serde(default)]
    occurrences: Option<Vec<String>>,
    /// Bodies of the view's component itself, by uid or name.
    #[serde(default)]
    bodies: Option<Vec<String>>,
    #[serde(default)]
    distance: Option<ValueIn>,
    #[serde(default)]
    direction: Option<AxisIn>,
    #[serde(default)]
    angle: Option<ValueIn>,
    #[serde(default)]
    axis: Option<AxisIn>,
    #[serde(default)]
    trails: Option<bool>,
    #[serde(default)]
    point: Option<Vec3>,
    #[serde(default)]
    seconds: Option<f64>,
    #[serde(default, rename = "move")]
    keep_move: Option<bool>,
    #[serde(default, rename = "turn")]
    keep_turn: Option<bool>,
}

/// `edit_explode_view`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditCommand {
    name: String,
    #[serde(default)]
    new_name: Option<String>,
    /// Every step, in place of the view's.
    #[serde(default)]
    steps: Option<Vec<StepIn>>,
    /// A step added at `at` (an index from 0; the end when left out).
    #[serde(default)]
    add_step: Option<StepIn>,
    #[serde(default)]
    at: Option<usize>,
    /// A step changed: its index and what of it changes.
    #[serde(default)]
    step: Option<usize>,
    #[serde(default)]
    set: Option<StepIn>,
    #[serde(default)]
    remove_step: Option<usize>,
    /// A step moved to the index `to`.
    #[serde(default)]
    move_step: Option<usize>,
    #[serde(default)]
    to: Option<usize>,
    /// The trail lines of every step on or off.
    #[serde(default)]
    trails: Option<bool>,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum ExplodeCommand {
        "add_explode_view" => Add {
            /// The next free `Explode<n>` when left out.
            #[serde(default)]
            name: Option<String>,
            /// The assembly component (uid or name); the root when left
            /// out.
            #[serde(default)]
            component: Option<String>,
            #[serde(default)]
            steps: Vec<StepIn>,
        },
        "edit_explode_view" => Edit(Box<EditCommand>),
        "remove_explode_view" => Remove { name: String },
        "auto_explode" => Auto {
            /// The view whose steps it makes (a new one when there is none
            /// of this name, or when left out).
            #[serde(default)]
            name: Option<String>,
            /// A new view's component (uid or name); the root when left
            /// out.
            #[serde(default)]
            component: Option<String>,
            /// The occurrences it moves (paths from the component); its
            /// top-level ones when left out.
            #[serde(default)]
            occurrences: Option<Vec<String>>,
            /// Each part moves by its size times this (1 when left out).
            #[serde(default)]
            factor: Option<f64>,
            /// After the view's steps instead of in their place.
            #[serde(default)]
            append: bool,
        },
    }
}

/// The `explode_view` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExplodeQuery {
    name: String,
    /// Seconds into its animation; every step done when left out.
    #[serde(default)]
    time: Option<f64>,
    /// Also where the bodies are in the design (`placements`,
    /// `body_placements`) and the trail lines in its coordinates
    /// (`design_trails`), for the 3D view.
    #[serde(default)]
    placements: bool,
}

fn failed(error: mitcad_model::ModelError) -> ApiError {
    ApiError(error.to_string())
}

/// A component by uid or name; the root when none is given.
fn component_of<K: Kernel>(
    doc: &Document<K>,
    text: Option<&str>,
) -> Result<ComponentUid, ApiError> {
    match text.map(str::trim).filter(|t| !t.is_empty()) {
        None => Ok(ComponentUid::ROOT),
        Some(t) => doc.component_ref(t),
    }
}

/// A path from `component` by uids or names.
fn path_in<K: Kernel>(
    doc: &Document<K>,
    component: ComponentUid,
    text: &str,
) -> Result<OccurrencePath, ApiError> {
    doc.occurrence_path_from(component, text)
        .map(OccurrencePath)
        .map_err(ApiError)
}

/// A body of `component` itself by uid or name.
fn body_in<K: Kernel>(
    doc: &Document<K>,
    component: ComponentUid,
    text: &str,
) -> Result<BodyUid, ApiError> {
    let own = doc.component_bodies(component);
    let text = text.trim();
    if let Ok(uid) = text.parse::<BodyUid>()
        && own.contains(&uid)
    {
        return Ok(uid);
    }
    own.into_iter()
        .find(|b| doc.body_name(*b) == text)
        .ok_or_else(|| {
            ApiError(format!(
                "{text} is not a body of {}",
                doc.assembly().name(component)
            ))
        })
}

fn axis_of<K: Kernel>(
    doc: &Document<K>,
    component: ComponentUid,
    input: &AxisIn,
) -> Result<ExplodeAxis, ApiError> {
    Ok(match input {
        AxisIn::Vector(v) => ExplodeAxis::along(*v),
        AxisIn::Named(name) => {
            let (sign, axis) = match name.trim().strip_prefix('-') {
                Some(rest) => (-1.0, rest),
                None => (1.0, name.trim().trim_start_matches('+')),
            };
            let mut v = [0.0; 3];
            match axis.to_ascii_lowercase().as_str() {
                "x" => v[0] = sign,
                "y" => v[1] = sign,
                "z" => v[2] = sign,
                _ => {
                    return Err(ApiError(format!(
                        "{name} is no axis: give x, y, z (-x, ...), a vector or geometry"
                    )));
                }
            }
            ExplodeAxis::along(v)
        }
        AxisIn::Object(o) => {
            let mut axis = ExplodeAxis {
                vector: o.vector,
                occurrence: path_in(doc, component, &o.occurrence)?,
                geometry: o.geometry.clone(),
                flip: o.flip,
            };
            if let Some(toward) = o.toward {
                if axis.geometry.is_none() || axis.vector.is_some() {
                    return Err(ApiError(
                        "toward orients geometry: give it with geometry, not with a vector"
                            .to_owned(),
                    ));
                }
                let (_, along) = doc.explode_axis(component, &axis).map_err(ApiError)?;
                if mitcad_vec3::dot(along, toward) < 0.0 {
                    axis.flip = !axis.flip;
                }
            }
            axis
        }
    })
}

/// A step from a command, over `base` (a new step: the default one).
fn step_of<K: Kernel>(
    doc: &Document<K>,
    component: ComponentUid,
    input: &StepIn,
    base: Option<ExplodeStep>,
) -> Result<ExplodeStep, ApiError> {
    let mut step = base.unwrap_or(ExplodeStep {
        name: String::new(),
        occurrences: Vec::new(),
        bodies: Vec::new(),
        distance: None,
        direction: None,
        angle: None,
        axis: None,
        trails: true,
        point: None,
        seconds: None,
    });
    if let Some(name) = &input.name {
        step.name = name.trim().to_owned();
    }
    if let Some(list) = &input.occurrences {
        step.occurrences = list
            .iter()
            .map(|t| path_in(doc, component, t))
            .collect::<Result<_, _>>()?;
    }
    if let Some(list) = &input.bodies {
        step.bodies = list
            .iter()
            .map(|t| body_in(doc, component, t))
            .collect::<Result<_, _>>()?;
    }
    if input.keep_move == Some(false) {
        step.distance = None;
        step.direction = None;
    }
    if input.keep_turn == Some(false) {
        step.angle = None;
        step.axis = None;
    }
    if let Some(d) = &input.distance {
        step.distance = Some(d.text("mm")?);
    }
    if let Some(d) = &input.direction {
        step.direction = Some(axis_of(doc, component, d)?);
    }
    if let Some(a) = &input.angle {
        step.angle = Some(a.text("deg")?);
    }
    if let Some(a) = &input.axis {
        step.axis = Some(axis_of(doc, component, a)?);
    }
    if let Some(on) = input.trails {
        step.trails = on;
    }
    if let Some(p) = input.point {
        step.point = Some(p);
    }
    if let Some(s) = input.seconds {
        step.seconds = Some(s);
    }
    Ok(step)
}

fn steps_of<K: Kernel>(
    doc: &Document<K>,
    component: ComponentUid,
    inputs: &[StepIn],
) -> Result<Vec<ExplodeStep>, ApiError> {
    inputs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            step_of(doc, component, s, None)
                .map_err(|e| ApiError(format!("Step {}: {}", i + 1, e.0)))
        })
        .collect()
}

/// An index of the steps the view `name` has now, or why it is none.
fn step_index(name: &str, steps: &[ExplodeStep], index: usize) -> Result<usize, ApiError> {
    if index < steps.len() {
        Ok(index)
    } else {
        Err(ApiError(format!(
            "{} has no step {} (it has {})",
            name,
            index + 1,
            steps.len()
        )))
    }
}

pub(crate) fn run<K: Kernel>(
    doc: &mut Document<K>,
    command: ExplodeCommand,
) -> Result<Value, ApiError> {
    match command {
        ExplodeCommand::Add {
            name,
            component,
            steps,
        } => {
            let component = component_of(doc, component.as_deref())?;
            let mut view = ExplodeView::new(name.unwrap_or_default(), component);
            view.steps = steps_of(doc, component, &steps)?;
            let name = doc.add_explode_view(view).map_err(failed)?;
            Ok(json!({"name": name}))
        }
        ExplodeCommand::Edit(edit) => {
            let EditCommand {
                name,
                new_name,
                steps,
                add_step,
                at,
                step,
                set,
                remove_step,
                move_step,
                to,
                trails,
            } = *edit;
            let view = doc.explode_view(&name).map_err(failed)?.clone();
            let component = view.component;
            // One change of the steps a command (mitcad#2527): an index of
            // one would else name a step another change moved.
            let changes = [
                steps.is_some(),
                step.is_some(),
                add_step.is_some(),
                remove_step.is_some(),
                move_step.is_some(),
            ];
            if changes.iter().filter(|given| **given).count() > 1 {
                return Err(ApiError(
                    "give one of steps, step, add_step, remove_step and move_step: one change \
                     of the steps a command"
                        .to_owned(),
                ));
            }
            let mut list = match &steps {
                Some(inputs) => Some(steps_of(doc, component, inputs)?),
                None => None,
            };
            let mut edited = |f: &mut dyn FnMut(&mut Vec<ExplodeStep>) -> Result<(), ApiError>| {
                let current = list.get_or_insert_with(|| view.steps.clone());
                f(current)
            };
            if let Some(i) = step {
                let input = set
                    .as_ref()
                    .ok_or_else(|| ApiError("give set: what of the step changes".to_owned()))?;
                edited(&mut |steps| {
                    let i = step_index(&view.name, steps, i)?;
                    let base = steps.get(i).cloned();
                    steps[i] = step_of(doc, component, input, base)?;
                    Ok(())
                })?;
            } else if set.is_some() {
                return Err(ApiError(
                    "give step: the index of the step set changes".to_owned(),
                ));
            }
            if let Some(input) = &add_step {
                let new = step_of(doc, component, input, None)?;
                edited(&mut |steps| {
                    let at = at.unwrap_or(steps.len()).min(steps.len());
                    steps.insert(at, new.clone());
                    Ok(())
                })?;
            }
            if let Some(i) = remove_step {
                edited(&mut |steps| {
                    let i = step_index(&view.name, steps, i)?;
                    steps.remove(i);
                    Ok(())
                })?;
            }
            if let Some(i) = move_step {
                let to = to.ok_or_else(|| ApiError("give to: where the step goes".to_owned()))?;
                edited(&mut |steps| {
                    let i = step_index(&view.name, steps, i)?;
                    let moved = steps.remove(i);
                    steps.insert(to.min(steps.len()), moved);
                    Ok(())
                })?;
            }
            if let Some(on) = trails {
                edited(&mut |steps| {
                    for s in steps.iter_mut() {
                        s.trails = on;
                    }
                    Ok(())
                })?;
            }
            doc.edit_explode_view(&name, new_name.as_deref(), list)
                .map_err(failed)?;
            let name = new_name
                .map(|n| n.trim().to_owned())
                .filter(|n| !n.is_empty())
                .unwrap_or(view.name);
            Ok(json!({"name": name}))
        }
        ExplodeCommand::Remove { name } => {
            doc.delete_explode_view(&name).map_err(failed)?;
            Ok(json!({}))
        }
        ExplodeCommand::Auto {
            name,
            component,
            occurrences,
            factor,
            append,
        } => {
            let existing = name
                .as_deref()
                .and_then(|n| doc.explode_view(n).ok())
                .map(|v| v.component);
            let component = match existing {
                Some(c) => c,
                None => component_of(doc, component.as_deref())?,
            };
            let occurrences = occurrences
                .map(|list| {
                    list.iter()
                        .map(|t| path_in(doc, component, t))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let name = doc
                .auto_explode(name.as_deref(), component, occurrences, factor, append)
                .map_err(failed)?;
            let steps = doc.explode_view(&name).map_err(failed)?.steps.len();
            Ok(json!({"name": name, "steps": steps}))
        }
    }
}

/// A direction or axis as the file keeps it, with the occurrence's name
/// and, for geometry, the unit direction it gives now in the view
/// component's coordinates (`resolved`, `flip` applied; mitcad#2328: Edit
/// Step shows its arrow so); left out when it cannot be found.
fn axis_json<K: Kernel>(doc: &Document<K>, component: ComponentUid, axis: &ExplodeAxis) -> Value {
    let mut value = serde_json::to_value(axis).unwrap_or(Value::Null);
    if !axis.occurrence.is_empty() {
        value["occurrence_name"] = json!(doc.assembly().path_name(&axis.occurrence.0));
    }
    if axis.geometry.is_some()
        && let Ok((_, along)) = doc.explode_axis(component, axis)
    {
        value["resolved"] = json!(along);
    }
    value
}

/// The `explode_views` query: every view with its steps as kept, the
/// occurrences and bodies with their names.
pub(crate) fn views_json<K: Kernel>(doc: &Document<K>) -> Value {
    let a = doc.assembly();
    Value::Array(
        doc.explode_views()
            .iter()
            .map(|view| {
                let steps: Vec<Value> = view
                    .steps
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let mut step = Map::new();
                        step.insert("index".to_owned(), json!(i));
                        step.insert("name".to_owned(), json!(s.shown_name(i)));
                        step.insert("own_name".to_owned(), json!(s.name));
                        step.insert(
                            "occurrences".to_owned(),
                            json!(
                                s.occurrences
                                    .iter()
                                    .map(|p| json!({"path": p, "name": a.path_name(&p.0)}))
                                    .collect::<Vec<_>>()
                            ),
                        );
                        step.insert(
                            "bodies".to_owned(),
                            json!(
                                s.bodies
                                    .iter()
                                    .map(|b| json!({"body": b, "name": doc.body_name(*b)}))
                                    .collect::<Vec<_>>()
                            ),
                        );
                        if let Some(d) = &s.distance {
                            step.insert("distance".to_owned(), json!(d));
                        }
                        if let Some(d) = &s.direction {
                            step.insert("direction".to_owned(), axis_json(doc, view.component, d));
                        }
                        if let Some(t) = &s.angle {
                            step.insert("angle".to_owned(), json!(t));
                        }
                        if let Some(t) = &s.axis {
                            step.insert("axis".to_owned(), axis_json(doc, view.component, t));
                        }
                        step.insert("trails".to_owned(), json!(s.trails));
                        if let Some(p) = s.point {
                            step.insert("point".to_owned(), json!(p));
                        }
                        step.insert("seconds".to_owned(), json!(s.seconds()));
                        Value::Object(step)
                    })
                    .collect();
                json!({
                    "name": view.name,
                    "component": view.component,
                    "component_name": a.name(view.component),
                    "seconds": view.seconds(),
                    "steps": steps,
                })
            })
            .collect(),
    )
}

fn part_json<K: Kernel>(doc: &Document<K>, part: &ExplodePart) -> Value {
    match part {
        ExplodePart::Occurrence(path) => json!({
            "occurrence": OccurrencePath(path.clone()),
            "name": doc.assembly().path_name(path),
        }),
        ExplodePart::Body(b) => json!({"body": b, "name": doc.body_name(*b)}),
    }
}

/// The `explode_view` query: the view worked out at a time (every step
/// done when none is given): each step's move and turn, the offset of
/// each part it moves (in the view component's coordinates), each part's
/// whole offset, the trail lines and the warnings; with `placements`
/// where the bodies are in the design.
pub(crate) fn view_json<K: Kernel>(
    doc: &Document<K>,
    query: &ExplodeQuery,
) -> Result<Value, ApiError> {
    let explode = doc.explode_state(&query.name, query.time).map_err(failed)?;
    let mut value = state_json(doc, &explode);
    if query.placements {
        let placed = doc.explode_placements(&explode);
        let path_text = |path: &[mitcad_model::ids::OccurrenceUid]| {
            path.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("/")
        };
        value["placements"] = json!(
            placed
                .occurrences
                .iter()
                .map(|(path, t)| json!({"path": path_text(path), "transform": matrix4(t)}))
                .collect::<Vec<_>>()
        );
        value["body_placements"] = json!(
            placed
                .bodies
                .iter()
                .map(|(path, body, t)| {
                    json!({"path": path_text(path), "body": body, "transform": matrix4(t)})
                })
                .collect::<Vec<_>>()
        );
        value["design_trails"] = json!(
            placed
                .trails
                .iter()
                .map(|(a, b)| json!([a, b]))
                .collect::<Vec<_>>()
        );
    }
    Ok(value)
}

fn state_json<K: Kernel>(doc: &Document<K>, explode: &ExplodeState) -> Value {
    let steps: Vec<Value> = explode
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let offsets: Vec<Value> = s
                .parts
                .iter()
                .map(|part| {
                    let mut v = part_json(doc, part);
                    v["transform"] = json!(matrix4(&s.transform));
                    v
                })
                .collect();
            json!({
                "index": i,
                "name": s.name,
                "fraction": s.fraction,
                "seconds": s.seconds,
                "distance": s.distance,
                "direction": s.direction,
                "angle": s.angle,
                "angle_degrees": s.angle.to_degrees(),
                "axis": s.axis.map(|(point, direction)| json!({"point": point, "direction": direction})),
                "translation": s.transform.translation,
                "offsets": offsets,
            })
        })
        .collect();
    let parts: Vec<Value> = explode
        .part_offsets()
        .iter()
        .map(|(part, t)| {
            let mut v = part_json(doc, part);
            v["translation"] = json!(t.translation);
            v["transform"] = json!(matrix4(t));
            v
        })
        .collect();
    let trails: Vec<Value> = explode
        .trails
        .iter()
        .map(|t| {
            let mut v = part_json(doc, &t.part);
            v["step"] = json!(t.step);
            v["from"] = json!(t.from);
            v["to"] = json!(t.to);
            v
        })
        .collect();
    json!({
        "name": explode.name,
        "component": explode.component,
        "seconds": explode.seconds,
        "time": explode.time,
        "steps": steps,
        "parts": parts,
        "trails": trails,
        "warnings": explode.warnings,
    })
}

/// What the explode views moved before a command, to say in words what a
/// deletion took out of them (mitcad#2113), or a move of an occurrence out
/// of a view's component (mitcad#2328): per view, each occurrence a step
/// moved and its name.
pub(crate) struct ExplodeBefore {
    moved: Vec<(String, Vec<(OccurrencePath, String)>)>,
}

impl ExplodeBefore {
    /// Nothing for the explode views' own commands, which say what they do.
    pub(crate) fn of<K: Kernel>(doc: &Document<K>, cmd: Option<&str>) -> Self {
        let own = cmd.is_some_and(|c| ExplodeCommand::NAMES.contains(&c));
        if own || doc.explode_views().is_empty() {
            return Self { moved: Vec::new() };
        }
        let a = doc.assembly();
        let moved = doc
            .explode_views()
            .iter()
            .map(|v| {
                let mut paths: Vec<(OccurrencePath, String)> = Vec::new();
                for s in &v.steps {
                    for p in &s.occurrences {
                        if !paths.iter().any(|(q, _)| q == p) {
                            paths.push((p.clone(), a.path_name(&p.0)));
                        }
                    }
                }
                (v.name.clone(), paths)
            })
            .collect();
        Self { moved }
    }

    /// The command's result with a warning for each occurrence an explode
    /// view no longer moves, and each view that went with its component.
    pub(crate) fn warn<K: Kernel>(&self, doc: &Document<K>, result: &mut Value) {
        if self.moved.is_empty() {
            return;
        }
        let mut texts: Vec<Value> = Vec::new();
        for (name, paths) in &self.moved {
            let Some(view) = doc.explode_views().iter().find(|v| &v.name == name) else {
                texts.push(json!(format!(
                    "the explode view {name} was removed with its component"
                )));
                continue;
            };
            for (path, occurrence) in paths {
                // By the occurrence itself: a path that followed it to
                // another parent in the view's component (mitcad#2328)
                // still moves it.
                let Some(last) = path.0.last() else {
                    continue;
                };
                let kept = view
                    .steps
                    .iter()
                    .flat_map(|s| &s.occurrences)
                    .any(|p| p.0.last() == Some(last));
                if kept {
                    continue;
                }
                let a = doc.assembly();
                texts.push(json!(if a.occurrence(*last).is_some() {
                    // Moved out of the view's component.
                    format!(
                        "{occurrence} is no longer in {}: the explode view {name} no longer moves it",
                        a.name(view.component)
                    )
                } else {
                    format!("{occurrence} was removed: the explode view {name} no longer moves it")
                }));
            }
        }
        if texts.is_empty() {
            return;
        }
        let Some(object) = result.as_object_mut() else {
            return;
        };
        for key in ["warnings", "warning_messages"] {
            match object.get_mut(key) {
                None => {
                    object.insert(key.to_owned(), Value::Array(texts.clone()));
                }
                Some(Value::Array(list)) => list.extend(texts.iter().cloned()),
                Some(_) => {}
            }
        }
    }
}
