// SPDX-License-Identifier: MIT
//! Scripts from the timeline and scripts run into a design (mitcad#2245).
//!
//! `export_script` writes the steps that rebuild a design on an empty one
//! as a `run_script` file (`script.rs`): the document's length unit, the
//! user parameters with their expressions, each feature by `add_feature`
//! with its definition, its name and its uid (so that the topological names
//! of later features, the body uids and every reference to it hold as they
//! are) and what it remembers of its splits' pieces (so that they keep
//! their ids, mitcad#2268), the slots' parameters named as they are (`"d3 = 10 mm"`), the
//! components and occurrences by their commands, the bodies' names, the
//! timeline groups and the marker, and at the end expectations (the body
//! count, each body's volume and face count) so that a replay checks
//! itself. What a replay does not make again is listed in `omitted`, in
//! words; what keeps a design from being exported at all refuses it.
//!
//! `run_script` runs such steps (or a recorded macro) into the open design
//! as one undo step; the first step that fails stops it and nothing
//! changes.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::assembly::ROOT_NAME;
use mitcad_model::expr::{ParamKind, value_to_expression};
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{ParamId, Parameter, Transform};

named_enum! {
    /// Scripts: the timeline as a script, and a script run as one undo
    /// step.
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", deny_unknown_fields)]
    pub(crate) enum ScriptCommand {
        "export_script" => ExportScript {
            /// The file the script is written to; without it the reply
            /// holds the script.
            #[serde(default)]
            path: Option<String>,
            /// `marker`: the features before the timeline marker; a
            /// feature's uid or name: the features up to it and with it.
            /// Every feature when left out.
            #[serde(default)]
            upto: Option<String>,
        },
        "run_script" => RunScript {
            /// The steps, as a script file holds them: commands, comments
            /// and expectations.
            steps: Vec<Value>,
            /// The undo step's name.
            #[serde(default)]
            label: Option<String>,
        },
    }
}

/// Runs a script command.
pub(crate) fn run<K: Kernel>(
    doc: &mut Document<K>,
    command: ScriptCommand,
) -> Result<Value, ApiError> {
    match command {
        ScriptCommand::ExportScript { path, upto } => {
            let exported = export_script(doc, upto.as_deref())?;
            let mut reply = json!({"steps": exported.script.len(),
                                   "features": exported.features,
                                   "omitted": exported.omitted});
            match path {
                Some(path) => {
                    std::fs::write(&path, script_text(&exported.script))
                        .map_err(|e| ApiError(format!("cannot write {path}: {e}")))?;
                    reply["path"] = json!(path);
                }
                None => reply["script"] = Value::Array(exported.script),
            }
            Ok(reply)
        }
        ScriptCommand::RunScript { steps, label } => run_steps(doc, steps, label.as_deref()),
    }
}

/// A script as a file holds it: one step a line.
pub fn script_text(script: &[Value]) -> String {
    let lines: Vec<String> = script.iter().map(|step| format!("  {step}")).collect();
    format!("[\n{}\n]\n", lines.join(",\n"))
}

/// Runs the steps of a script into the document as one undo step named
/// `label` (`Run Script` when left out). The first step that fails stops
/// it: the document is as it was before, and the error names the step's
/// number (from 1) with its message.
fn run_steps<K: Kernel>(
    doc: &mut Document<K>,
    steps: Vec<Value>,
    label: Option<&str>,
) -> Result<Value, ApiError> {
    let label = label
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or("Run Script")
        .to_owned();
    let depth = doc.undo_depth();
    let token = doc.begin_command().map_err(|_| {
        ApiError("a command is open; finish or cancel it before running a script".to_owned())
    })?;
    let count = steps.len();
    let mut results = Vec::new();
    let mut kept = super::script::Kept::new();
    for (i, step) in steps.into_iter().enumerate() {
        let outcome = match super::script::run_step(doc, step, &mut kept) {
            Ok(_) if doc.undo_depth() < depth => Err(ApiError(
                "it undoes a change made before the script".to_owned(),
            )),
            other => other,
        };
        match outcome {
            Ok(Some(result)) => results.push(result),
            Ok(None) => {}
            Err(e) => {
                let restored = doc.restore_command(&token);
                let _ = doc.discard_command(&token);
                restored?;
                return Err(ApiError(format!("Step {}: {e}", i + 1)));
            }
        }
    }
    doc.discard_command(&token)?;
    doc.merge_undo(depth, &label);
    Ok(json!({"steps": count, "results": results}))
}

/// A design's timeline as a script.
#[derive(Debug, Clone, Default)]
pub struct Exported {
    /// The steps.
    pub script: Vec<Value>,
    /// The features it adds.
    pub features: usize,
    /// What a replay does not make again, one line per kind in words.
    pub omitted: Vec<String>,
    /// The features left out (imported bodies, placeholders and the
    /// features that use them): the replay's bodies then differ from the
    /// design's, and the script has no expectations.
    pub left_out: Vec<FeatureUid>,
}

/// One thing that makes a component, an occurrence or both, in the order a
/// replay makes them.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Making {
    /// A feature: it may make components (a new-component operation,
    /// Component from Bodies).
    Feature(usize),
    /// `create_component`: the component and its first occurrence.
    Create(ComponentUid, OccurrenceUid),
    /// `copy_occurrence`: another occurrence of a component placed in the
    /// same component as its first one.
    Copy(OccurrenceUid, OccurrenceUid),
}

/// The script that rebuilds the design on an empty one: every feature, or
/// with `upto` the features before the marker (`marker`) or up to a
/// feature (its uid or name) and with it.
pub fn export_script<K: Kernel>(
    doc: &Document<K>,
    upto: Option<&str>,
) -> Result<Exported, ApiError> {
    let state = doc.state();
    if let Some(refusal) = state.script_refusal() {
        return Err(ApiError(format!(
            "cannot export the timeline as a script: {refusal}"
        )));
    }
    let features: Vec<_> = doc.features().collect();
    let end = match upto.map(str::trim) {
        None => features.len(),
        Some("marker") => doc.marker(),
        Some(wanted) => {
            let by_uid = wanted
                .parse::<FeatureUid>()
                .ok()
                .and_then(|uid| features.iter().position(|f| f.uid == uid));
            let position = match by_uid {
                Some(position) => position,
                None => {
                    let named: Vec<usize> = (0..features.len())
                        .filter(|i| features[*i].name == wanted)
                        .collect();
                    match named.as_slice() {
                        [one] => *one,
                        [] => {
                            return Err(ApiError(format!(
                                "upto: no feature {wanted}; give \"marker\" or a feature's uid \
                                 or name"
                            )));
                        }
                        _ => {
                            return Err(ApiError(format!(
                                "upto: several features are named {wanted}; give its uid"
                            )));
                        }
                    }
                }
            };
            position + 1
        }
    };
    let marker = doc.marker().min(end);

    // Imported bodies and placeholders have their source only in the
    // design file: they are left out, with the features that use them or
    // live in a component they make.
    let mut omitted = Vec::new();
    let mut left: BTreeSet<FeatureUid> = BTreeSet::new();
    for f in &features[..end] {
        if matches!(f.def, FeatureDef::Base(_) | FeatureDef::Placeholder(_)) {
            left.insert(f.uid);
            omitted.push(format!(
                "{}: imported bodies, whose source is only in the design file",
                f.name
            ));
        }
    }
    let assembly = doc.assembly();
    loop {
        let before = left.len();
        let mut grown = left.clone();
        grown.extend(state.dependents(&left));
        for f in &features[..end] {
            let made_by_left = assembly
                .component(f.component)
                .and_then(|c| c.created_by)
                .is_some_and(|by| grown.contains(&by));
            if made_by_left {
                grown.insert(f.uid);
            }
        }
        left = grown;
        if left.len() == before {
            break;
        }
    }
    for f in &features[..end] {
        if left.contains(&f.uid)
            && !matches!(f.def, FeatureDef::Base(_) | FeatureDef::Placeholder(_))
        {
            omitted.push(format!(
                "{}: it uses imported bodies the script leaves out",
                f.name
            ));
        }
    }
    let exported: Vec<usize> = (0..end)
        .filter(|i| !left.contains(&features[*i].uid))
        .collect();

    let makings = component_makings(doc, &exported)?;

    let params = doc.parameters();
    let mut script = Vec::new();
    script.push(json!({"comment": format!(
        "The timeline of a design as a script, made by MitCad's export_script: {} features. \
         Run it on an empty design (mitcad-cli run, or Run Script in the application).",
        exported.len()
    )}));
    let length = params.context().default_length_unit;
    if length.symbol() != "mm" {
        script.push(json!({"cmd": "set_units", "length": length.symbol()}));
    }
    if assembly.root_name != ROOT_NAME {
        script.push(json!({"cmd": "rename_component", "component": "C0",
                           "name": assembly.root_name}));
    }

    // The parameters no feature owns are added as soon as what they use is
    // there; a feature's own ones come with it.
    let mut defined: HashSet<ParamId> = HashSet::new();
    let mut pending: Vec<&Parameter> = params
        .iter()
        .filter(|p| params.owner(p.id()).is_none())
        .collect();
    let owned_by_left: HashSet<ParamId> = params
        .iter()
        .filter(|p| params.owner(p.id()).is_some_and(|o| left.contains(&o)))
        .map(Parameter::id)
        .collect();
    // Where the replay has placed each occurrence so far: an occurrence
    // whose parent is made later (it was moved there by
    // `reparent_occurrence`) goes into the root first and is moved into its
    // parent as soon as that is there. `create_component` and
    // `copy_occurrence` place into the active component.
    let mut placing = Placing {
        active: ComponentUid::ROOT,
        made: BTreeSet::from([ComponentUid::ROOT]),
        placed: BTreeMap::new(),
        waiting: Vec::new(),
    };
    for making in &makings {
        match *making {
            Making::Feature(i) => {
                add_ready_parameters(doc, &mut pending, &mut defined, &mut script);
                let f = features[i];
                export_feature(doc, f, &mut defined, &owned_by_left, &mut script)?;
                // The components it made (placed in its component): their
                // names and placements.
                for (component, occurrence) in made_components(doc, f.uid) {
                    let name = assembly.name(component);
                    script.push(json!({"cmd": "rename_component",
                                       "component": component.to_string(), "name": name}));
                    placing.made.insert(component);
                    placing.placed.insert(occurrence, f.component);
                    let Some(o) = assembly.occurrence(occurrence) else {
                        continue;
                    };
                    if o.parent != f.component {
                        placing.waiting.push(occurrence);
                    } else if o.transform != Transform::IDENTITY {
                        script.push(json!({"cmd": "set_occurrence_transform",
                                           "occurrence": placing.path(doc, occurrence),
                                           "transform": rows(&o.transform)}));
                    }
                }
                // Its bodies' names. The replay names a feature's new
                // bodies in the order its evaluation gives them, which
                // need not be the design's (a split's pieces keep their
                // ids by what the feature remembers, but not their order,
                // mitcad#2268): with more than one, each first takes a
                // name no body has, so that none is renamed to a name
                // another of them still holds.
                let named: Vec<(String, &String)> = state
                    .body_names
                    .iter()
                    .filter(|(body, _)| body.feature == f.uid)
                    .map(|(body, name)| (body.to_string(), name))
                    .collect();
                if named.len() > 1 {
                    for (body, name) in &named {
                        script.push(json!({"cmd": "rename_body", "uid": body,
                                           "name": passing_name(state, body, name)}));
                    }
                }
                for (body, name) in &named {
                    script.push(json!({"cmd": "rename_body", "uid": body, "name": name}));
                }
            }
            Making::Create(component, occurrence) => {
                let o = assembly.occurrence(occurrence).expect("listed");
                let into = placing.place_into(o.parent, &mut script);
                let mut step = json!({"cmd": "create_component",
                                      "name": assembly.name(component), "activate": false});
                if into == o.parent && o.transform != Transform::IDENTITY {
                    step["transform"] = rows(&o.transform);
                }
                script.push(step);
                placing.made.insert(component);
                placing.placed.insert(occurrence, into);
                if into != o.parent {
                    placing.waiting.push(occurrence);
                }
            }
            Making::Copy(occurrence, of) => {
                let o = assembly.occurrence(occurrence).expect("listed");
                let into = placing.place_into(o.parent, &mut script);
                script.push(json!({"cmd": "copy_occurrence",
                                   "occurrence": placing.path(doc, of),
                                   "transform": rows(&o.transform)}));
                placing.placed.insert(occurrence, into);
                if into != o.parent {
                    placing.waiting.push(occurrence);
                }
            }
        }
        placing.move_waiting(doc, &mut script);
    }
    let active = placing.active;
    add_ready_parameters(doc, &mut pending, &mut defined, &mut script);
    // What is left uses parameters of features the script does not add.
    for p in pending {
        omitted.push(format!(
            "the parameter {}: it uses parameters of features the script does not add",
            p.name()
        ));
    }

    let kept: HashSet<FeatureUid> = exported.iter().map(|i| features[*i].uid).collect();
    for group in &state.groups {
        if group.features.iter().all(|f| kept.contains(f)) {
            script.push(json!({"cmd": "group_features", "features": group.features,
                               "name": group.name}));
        } else {
            omitted.push(format!(
                "the timeline group {}: its features are not all in the script",
                group.name
            ));
        }
    }
    for (uid, visible) in &state.feature_visibility {
        if kept.contains(uid) {
            script.push(json!({"cmd": "set_feature_visible", "uid": uid, "visible": visible}));
        }
    }
    for (uid, size) in &state.datum_display_sizes {
        if kept.contains(uid) {
            script.push(json!({"cmd": "set_datum_display_size", "uid": uid, "size": size}));
        }
    }
    for o in &assembly.occurrences {
        if !occurrence_kept(doc, o.uid, &left) {
            continue;
        }
        if !o.visible {
            script.push(json!({"cmd": "set_occurrence_visible",
                               "occurrence": occurrence_path(doc, o.uid), "visible": false}));
        }
        if o.grounded {
            script.push(json!({"cmd": "ground_occurrence",
                               "occurrence": occurrence_path(doc, o.uid)}));
        }
    }
    // Design policies last (mitcad#220): the general defaults allow
    // everything the script does before, and the design held its content
    // under these.
    let general = mitcad_model::policy::Policy::GENERAL;
    let mut policies = vec![(ComponentUid::ROOT, assembly.root_policy)];
    policies.extend(assembly.components.iter().map(|c| (c.uid, c.policy)));
    for (component, policy) in policies {
        if policy != general && placing.made.contains(&component) {
            script.push(
                json!({"cmd": "set_policy", "component": component.to_string(),
                               "policy": policy}),
            );
        }
    }
    if assembly.active != active {
        script.push(json!({"cmd": "activate_component",
                           "component": assembly.active.to_string()}));
    }
    // The marker, counted among the features the script adds.
    let position = exported.iter().filter(|i| **i < marker).count();
    if position != exported.len() {
        script.push(json!({"cmd": "set_marker", "position": position}));
    }

    if left.is_empty() {
        expectations(doc, marker, &mut script);
    } else {
        script.push(json!({"comment":
            "No expectations: the script leaves out imported bodies, so its bodies differ \
             from the design's."}));
    }
    omitted.extend(state.script_omissions());
    // The script says itself what it does not make again.
    if !omitted.is_empty() {
        script.insert(
            1,
            json!({"comment": format!("Not made again by this script: {}.", omitted.join("; "))}),
        );
    }
    Ok(Exported {
        script,
        features: exported.len(),
        omitted,
        left_out: left.into_iter().collect(),
    })
}

/// A name for `body` (its uid) on its way to `name` that no body of the
/// design has (`Body2 (F3.b0)`), so that renaming a feature's bodies one by
/// one never meets a name another of them holds (mitcad#2268).
fn passing_name(state: &mitcad_model::DocState, body: &str, name: &str) -> String {
    let mut passing = format!("{name} ({body})");
    while state.body_names.values().any(|n| *n == passing) {
        passing.push('\'');
    }
    passing
}

/// The user parameters whose expressions use only parameters already
/// there, added in turn until none is left that could be.
fn add_ready_parameters<K: Kernel>(
    doc: &Document<K>,
    pending: &mut Vec<&Parameter>,
    defined: &mut HashSet<ParamId>,
    script: &mut Vec<Value>,
) {
    loop {
        let Some(at) = pending
            .iter()
            .position(|p| p.dependencies().iter().all(|d| defined.contains(d)))
        else {
            return;
        };
        let p = pending.remove(at);
        defined.insert(p.id());
        script.push(add_parameter_step(doc, p));
    }
}

/// `add_parameter` of a parameter no feature owns, with its unit, comment,
/// group and limits, and its star.
fn add_parameter_step<K: Kernel>(doc: &Document<K>, p: &Parameter) -> Value {
    let mut step = json!({"cmd": "add_parameter", "name": p.name(),
                          "expression": p.expression()});
    // A unitless parameter's unit is empty: its expression says it.
    let unit = p.unit().to_string();
    if !p.is_text() && !unit.is_empty() {
        step["unit"] = json!(unit);
    }
    if !p.comment().is_empty() {
        step["comment"] = json!(p.comment());
    }
    if let Some(group) = doc.parameter_groups().group_of(p.id()) {
        step["group"] = json!(group);
    }
    add_limits(&mut step, p);
    step
}

/// A parameter's limits on an `add_parameter` or `set_parameter` step.
fn add_limits(step: &mut Value, p: &Parameter) {
    let limits = p.limits();
    if limits.is_none() {
        return;
    }
    if let Some(min) = &limits.min {
        step["min"] = json!(min);
    }
    if let Some(max) = &limits.max {
        step["max"] = json!(max);
    }
    if let Some(value) = &limits.step {
        step["step"] = json!(value);
    }
    if limits.strict {
        step["strict"] = json!(true);
    }
    if let Some(values) = &limits.values {
        step["values"] = json!(values);
    }
}

/// The expression of a feature's own parameter as its slot takes it: a bare
/// number with the parameter's unit, so that it means the same in any
/// document unit.
fn slot_expression(p: &Parameter) -> String {
    let expression = p.expression().trim();
    if !p.is_text() && expression.parse::<f64>().is_ok() {
        value_to_expression(p.value(), p.unit())
    } else {
        expression.to_owned()
    }
}

/// The steps of one feature: a comment with its name, the reference
/// parameters of its driven dimensions, `add_feature` with its uid, its
/// suppression and its own parameters' stars and limits.
fn export_feature<K: Kernel>(
    doc: &Document<K>,
    f: &mitcad_model::FeatureEntry,
    defined: &mut HashSet<ParamId>,
    owned_by_left: &HashSet<ParamId>,
    script: &mut Vec<Value>,
) -> Result<(), ApiError> {
    let params = doc.parameters();
    let mut references = Vec::new();
    let mut own = Vec::new();
    let sketch = matches!(f.def, FeatureDef::Sketch(_) | FeatureDef::SpatialSketch(_));
    let def = f
        .def
        .map_params(&mut |_slot, id: &ParamId| -> Result<String, ApiError> {
            let Some(p) = params.get(*id) else {
                return Err(ApiError(format!(
                    "{}: a value's parameter does not exist",
                    f.name
                )));
            };
            if owned_by_left.contains(id) {
                return Err(ApiError(format!(
                    "cannot export the timeline as a script: {} uses {}, a parameter of a \
                     feature the script leaves out",
                    f.name,
                    p.name()
                )));
            }
            if defined.contains(id) || params.owner(*id) != Some(f.uid) {
                return Ok(p.name().to_owned());
            }
            defined.insert(*id);
            own.push(*id);
            if p.kind() == ParamKind::Reference {
                if sketch {
                    // A driven dimension's reference: a parameter of that
                    // name the sketch takes as its own (mitcad#250).
                    references.push(*id);
                    return Ok(p.name().to_owned());
                }
                // A measured parameter of another feature's own (a flex
                // item's length): made with its name and its last value.
                return Ok(format!(
                    "{} = {}",
                    p.name(),
                    value_to_expression(p.value(), p.unit())
                ));
            }
            Ok(format!("{} = {}", p.name(), slot_expression(p)))
        })?;
    let mut comment = f.name.clone();
    if f.suppressed {
        comment.push_str(" (suppressed)");
    }
    script.push(json!({"comment": comment}));
    for id in references {
        let p = params.get(id).expect("listed");
        script.push(json!({"cmd": "add_parameter", "name": p.name(),
                           "expression": value_to_expression(p.value(), p.unit())}));
    }
    let def = serde_json::to_value(&def)
        .map_err(|e| ApiError(format!("{}: its definition: {e}", f.name)))?;
    // Its component always: the script activates components to make
    // others in them, and a feature of the root added while another is
    // active would go there.
    let mut step = json!({"cmd": "add_feature", "def": def, "name": f.name, "uid": f.uid,
                          "component": f.component.to_string()});
    // What it remembers of its splits' pieces (mitcad#408): a split edited
    // after it was made keeps its pieces' ids, which the replay's first
    // evaluation would number in geometric order (mitcad#2268).
    if let Some(groups) = doc.state().split_pieces_of(f.uid) {
        step["pieces"] = serde_json::to_value(groups)
            .map_err(|e| ApiError(format!("{}: its pieces: {e}", f.name)))?;
    }
    script.push(step);
    if f.suppressed {
        script.push(json!({"cmd": "suppress_feature", "uid": f.uid}));
    }
    for id in own {
        let p = params.get(id).expect("listed");
        let mut step = json!({"cmd": "set_parameter", "name": p.name()});
        let mut any = false;
        if doc.is_favorite_parameter(p.name()) {
            step["favorite"] = json!(true);
            any = true;
        }
        if !p.limits().is_none() && p.kind() != ParamKind::Reference {
            add_limits(&mut step, p);
            any = true;
        }
        if any {
            script.push(step);
        }
    }
    Ok(())
}

/// Where a replay of the script has placed the occurrences so far
/// (`export_script`'s components and occurrences).
struct Placing {
    /// The active component, where `create_component` and
    /// `copy_occurrence` place.
    active: ComponentUid,
    /// The components made so far, the root among them.
    made: BTreeSet<ComponentUid>,
    /// The component each occurrence made so far is placed in.
    placed: BTreeMap<OccurrenceUid, ComponentUid>,
    /// The occurrences placed in the root because their parent was not
    /// made yet, to be moved into it.
    waiting: Vec<OccurrenceUid>,
}

impl Placing {
    /// The component a new occurrence whose parent is `parent` goes into
    /// (its parent when made, else the root), activated.
    fn place_into(&mut self, parent: ComponentUid, script: &mut Vec<Value>) -> ComponentUid {
        let into = if self.made.contains(&parent) {
            parent
        } else {
            ComponentUid::ROOT
        };
        if into != self.active {
            script.push(json!({"cmd": "activate_component", "component": into.to_string()}));
            self.active = into;
        }
        into
    }

    /// An occurrence's path as the replay has it now: through the first
    /// occurrence of each component above it, where it is placed so far.
    fn path<K: Kernel>(&self, doc: &Document<K>, occurrence: OccurrenceUid) -> String {
        let mut path = vec![occurrence.to_string()];
        let mut parent = self.placed.get(&occurrence).copied();
        let mut guard = 0;
        while let Some(component) = parent.filter(|c| !c.is_root()) {
            guard += 1;
            let Some(above) = first_occurrence(doc, component).filter(|_| guard < 64) else {
                break;
            };
            path.push(above.to_string());
            parent = self.placed.get(&above).copied();
        }
        path.reverse();
        path.join("/")
    }

    /// Moves each waiting occurrence whose parent is made now into it
    /// (`reparent_occurrence`, which keeps it where it is in the design),
    /// then gives it its own placement in that parent.
    fn move_waiting<K: Kernel>(&mut self, doc: &Document<K>, script: &mut Vec<Value>) {
        let assembly = doc.assembly();
        loop {
            let Some(at) = self.waiting.iter().position(|o| {
                assembly
                    .occurrence(*o)
                    .is_some_and(|o| self.made.contains(&o.parent))
            }) else {
                return;
            };
            let uid = self.waiting.remove(at);
            let o = assembly.occurrence(uid).expect("waiting");
            let parent = if o.parent.is_root() {
                String::new()
            } else {
                first_occurrence(doc, o.parent).map_or_else(String::new, |p| self.path(doc, p))
            };
            script.push(json!({"cmd": "reparent_occurrence",
                               "occurrence": self.path(doc, uid), "parent": parent}));
            self.placed.insert(uid, o.parent);
            script.push(json!({"cmd": "set_occurrence_transform",
                               "occurrence": self.path(doc, uid),
                               "transform": rows(&o.transform)}));
        }
    }
}

/// The components a feature made, with their first occurrences, by uid.
fn made_components<K: Kernel>(
    doc: &Document<K>,
    feature: FeatureUid,
) -> Vec<(ComponentUid, OccurrenceUid)> {
    let assembly = doc.assembly();
    let mut made: Vec<(ComponentUid, OccurrenceUid)> = assembly
        .components
        .iter()
        .filter(|c| c.created_by == Some(feature))
        .filter_map(|c| first_occurrence(doc, c.uid).map(|o| (c.uid, o)))
        .collect();
    made.sort_by_key(|(c, _)| c.0);
    made
}

/// The occurrence of a component made first.
fn first_occurrence<K: Kernel>(
    doc: &Document<K>,
    component: ComponentUid,
) -> Option<OccurrenceUid> {
    doc.assembly()
        .occurrences
        .iter()
        .filter(|o| o.component == component)
        .map(|o| o.uid)
        .min_by_key(|o| o.0)
}

/// An occurrence as commands name it: the path of uids from the root,
/// through the first occurrence of each component above it.
fn occurrence_path<K: Kernel>(doc: &Document<K>, occurrence: OccurrenceUid) -> String {
    let assembly = doc.assembly();
    let mut path = vec![occurrence.to_string()];
    let mut parent = assembly.occurrence(occurrence).map(|o| o.parent);
    let mut guard = 0;
    while let Some(component) = parent.filter(|c| !c.is_root()) {
        guard += 1;
        let Some(above) = first_occurrence(doc, component).filter(|_| guard < 64) else {
            break;
        };
        path.push(above.to_string());
        parent = assembly.occurrence(above).map(|o| o.parent);
    }
    path.reverse();
    path.join("/")
}

/// Whether the script makes an occurrence: its component is not made by a
/// feature it leaves out.
fn occurrence_kept<K: Kernel>(
    doc: &Document<K>,
    occurrence: OccurrenceUid,
    left: &BTreeSet<FeatureUid>,
) -> bool {
    let assembly = doc.assembly();
    assembly
        .occurrence(occurrence)
        .and_then(|o| assembly.component(o.component))
        .is_none_or(|c| c.created_by.is_none_or(|by| !left.contains(&by)))
}

/// A transform as the commands take it: three rows of four numbers.
fn rows(t: &Transform) -> Value {
    json!(
        (0..3)
            .map(|r| vec![
                t.linear[r][0],
                t.linear[r][1],
                t.linear[r][2],
                t.translation[r]
            ])
            .collect::<Vec<_>>()
    )
}

/// The features, the components and the occurrences in the order a replay
/// makes them, so that each gets the uid it has in the design: the
/// features in timeline order (their uids are given), the components and
/// occurrences not made by a feature by their uids, each before the first
/// feature that makes a later one or lives in it. Refused, in words, where
/// the replay could not give them their uids (one was deleted or they were
/// made in another order), or where a component's source is elsewhere.
fn component_makings<K: Kernel>(
    doc: &Document<K>,
    exported: &[usize],
) -> Result<Vec<Making>, ApiError> {
    let assembly = doc.assembly();
    let refuse = |why: String| ApiError(format!("cannot export the timeline as a script: {why}"));
    let features: Vec<_> = doc.features().collect();
    let kept: HashSet<FeatureUid> = exported.iter().map(|i| features[*i].uid).collect();
    // What makes each occurrence other than features.
    let mut others: BTreeMap<u32, Making> = BTreeMap::new();
    for c in &assembly.components {
        let name = &c.name;
        if c.link.is_some() {
            return Err(refuse(format!("{name} is linked from another design file")));
        }
        if c.library.is_some() {
            return Err(refuse(format!("{name} comes from a component library")));
        }
        if c.build.is_some() {
            return Err(refuse(format!("{name} was made by BUILD")));
        }
        let mut placed: Vec<_> = assembly
            .occurrences
            .iter()
            .filter(|o| o.component == c.uid)
            .collect();
        placed.sort_by_key(|o| o.uid.0);
        let Some(first) = placed.first() else {
            return Err(refuse(format!("{name} is placed nowhere")));
        };
        if placed.iter().any(|o| o.pattern.is_some()) {
            return Err(refuse(format!(
                "{name} is placed by a Pattern at Points, whose occurrences a script cannot \
                 make again"
            )));
        }
        match c.created_by {
            Some(by) if !kept.contains(&by) => continue,
            Some(_) => {}
            None => {
                others.insert(first.uid.0, Making::Create(c.uid, first.uid));
            }
        }
        // Further occurrences are copies of the first, made in their own
        // parents (`copy_occurrence` places into the active component).
        for o in &placed[1..] {
            others.insert(o.uid.0, Making::Copy(o.uid, first.uid));
        }
    }
    // Each feature after what comes before the components it makes and
    // its own component.
    let mut order = Vec::new();
    for &i in exported {
        let f = features[i];
        let made = made_components(doc, f.uid);
        let bound = made.first().map(|(_, o)| o.0);
        let own = (!f.component.is_root())
            .then(|| first_occurrence(doc, f.component))
            .flatten()
            .map(|o| o.0);
        let take: Vec<u32> = others
            .keys()
            .copied()
            .filter(|uid| bound.is_some_and(|b| *uid < b) || own.is_some_and(|own| *uid <= own))
            .collect();
        for uid in take {
            order.push(others.remove(&uid).expect("listed"));
        }
        order.push(Making::Feature(i));
    }
    order.extend(others.into_values());
    // The uids the replay gives, counted as the model counts them.
    let (mut next_component, mut next_occurrence) = (1u32, 1u32);
    let mut expect = |component: Option<ComponentUid>, occurrence: OccurrenceUid| {
        let ok = component.is_none_or(|c| c.0 == next_component) && occurrence.0 == next_occurrence;
        if component.is_some() {
            next_component += 1;
        }
        next_occurrence += 1;
        ok
    };
    for making in &order {
        let fits = match *making {
            Making::Feature(i) => made_components(doc, features[i].uid)
                .into_iter()
                .all(|(c, o)| expect(Some(c), o)),
            Making::Create(c, o) => expect(Some(c), o),
            Making::Copy(o, _) => expect(None, o),
        };
        if !fits {
            return Err(refuse(
                "its components and occurrences cannot be made again with the ids they have \
                 (some were deleted, or made in another order than the timeline keeps)"
                    .to_owned(),
            ));
        }
    }
    Ok(order)
}

/// The expectations a replay checks itself with: the number of bodies at
/// the marker and each body's volume and face count. With the marker
/// before the document's own (`upto` an earlier feature), the bodies of a
/// copy of the document with its marker there.
fn expectations<K: Kernel>(doc: &Document<K>, marker: usize, script: &mut Vec<Value>) {
    if marker != doc.marker() {
        let Some(mut fork) = doc.fork() else {
            script.push(
                json!({"comment": "No expectations: the design could not be computed \
                                            at the script's marker."}),
            );
            return;
        };
        if fork.set_marker(marker).is_err() {
            script.push(
                json!({"comment": "No expectations: the design could not be computed \
                                            at the script's marker."}),
            );
            return;
        }
        body_expectations(&fork, script);
    } else {
        body_expectations(doc, script);
    }
}

fn body_expectations<K: Kernel>(doc: &Document<K>, script: &mut Vec<Value>) {
    let bodies = doc.bodies();
    script.push(json!({"expect": {"bodies": bodies.len()}}));
    let kernel = doc.kernel();
    for body in &bodies {
        let mut expect = json!({"body": body.uid.to_string()});
        if let Ok(mass) = kernel.mass_properties(body.shape) {
            expect["volume"] = json!(mass.volume);
        }
        if let Ok(faces) = kernel.faces(body.shape) {
            expect["faces"] = json!(faces.len());
        }
        script.push(json!({"expect": expect}));
    }
}

/// What a round trip found ([`round_trip`]).
#[derive(Debug, Clone)]
pub struct RoundTrip {
    /// Why the design was not replayed (the script leaves features out),
    /// None when it was.
    pub skipped: Option<String>,
    /// What was compared, in words.
    pub report: String,
}

/// Exports `original`'s timeline as a script, runs it on `replay` (an
/// empty document) and compares the two: the features (uids, names,
/// types, suppression, status), the parameters (names and values), the
/// bodies (uids, names, volumes within 1e-6 relative, face counts) and
/// every body's face names. Fails, in words, at the first difference.
pub fn round_trip<K: Kernel>(
    original: &Document<K>,
    replay: &mut Document<K>,
) -> Result<RoundTrip, ApiError> {
    let exported = export_script(original, None)?;
    if !exported.left_out.is_empty() {
        return Ok(RoundTrip {
            skipped: Some(format!(
                "the script leaves out {} features: {}",
                exported.left_out.len(),
                exported.omitted.join("; ")
            )),
            report: String::new(),
        });
    }
    replay
        .run_script(&script_text(&exported.script))
        .map_err(|e| ApiError(format!("the replay failed: {e}")))?;
    let differ = |what: String| Err(ApiError(format!("the replay differs: {what}")));
    // The features.
    let a: Vec<_> = original.timeline();
    let b: Vec<_> = replay.timeline();
    if a.len() != b.len() {
        return differ(format!("{} features, the design has {}", b.len(), a.len()));
    }
    for (x, y) in a.iter().zip(&b) {
        let (fx, fy) = (x.entry, y.entry);
        if fx.uid != fy.uid || fx.name != fy.name {
            return differ(format!(
                "{} {} where the design has {} {}",
                fy.uid, fy.name, fx.uid, fx.name
            ));
        }
        if fx.def.type_name() != fy.def.type_name() || fx.suppressed != fy.suppressed {
            return differ(format!("{} is another feature than the design's", fx.name));
        }
        if x.status_text() != y.status_text() {
            return differ(format!(
                "{} is {}, in the design {}",
                fx.name,
                y.status_text(),
                x.status_text()
            ));
        }
    }
    if original.marker() != replay.marker() {
        return differ(format!(
            "the marker is after {} features, in the design after {}",
            replay.marker(),
            original.marker()
        ));
    }
    // The parameters.
    let (pa, pb) = (original.parameters(), replay.parameters());
    for p in pa.iter() {
        let Some(q) = pb.find(p.name()).and_then(|id| pb.get(id)) else {
            return differ(format!("it has no parameter {}", p.name()));
        };
        let close = (p.value() - q.value()).abs() <= 1e-9 * p.value().abs().max(1.0);
        if !close || p.text() != q.text() {
            return differ(format!(
                "parameter {} is {}, in the design {}",
                p.name(),
                q.value(),
                p.value()
            ));
        }
    }
    if pa.len() != pb.len() {
        return differ(format!(
            "{} parameters, the design has {}",
            pb.len(),
            pa.len()
        ));
    }
    // The bodies and their faces' names.
    let (ba, bb) = (original.bodies(), replay.bodies());
    if ba.len() != bb.len() {
        return differ(format!("{} bodies, the design has {}", bb.len(), ba.len()));
    }
    let mut faces = 0;
    for x in &ba {
        let Some(y) = bb.iter().find(|y| y.uid == x.uid) else {
            return differ(format!("it has no body {}", x.uid));
        };
        if x.name != y.name {
            return differ(format!(
                "body {} is named {}, in the design {}",
                x.uid, y.name, x.name
            ));
        }
        if let (Ok(mx), Ok(my)) = (
            original.kernel().mass_properties(x.shape),
            replay.kernel().mass_properties(y.shape),
        ) && (mx.volume - my.volume).abs() > 1e-6 * mx.volume.abs().max(1.0)
        {
            return differ(format!(
                "{} has the volume {}, in the design {}",
                x.name, my.volume, mx.volume
            ));
        }
        let query = json!({"query": "faces", "body": x.uid.to_string()}).to_string();
        let names = |doc: &Document<K>| -> Result<Vec<String>, ApiError> {
            let answer: Value =
                serde_json::from_str(&doc.query(&query)?).map_err(|e| ApiError(e.to_string()))?;
            let mut names: Vec<String> = answer
                .as_array()
                .into_iter()
                .flatten()
                .map(|face| face["names"].to_string())
                .collect();
            names.sort();
            Ok(names)
        };
        let (na, nb) = (names(original)?, names(replay)?);
        if na != nb {
            let missing: Vec<&String> = na.iter().filter(|n| !nb.contains(n)).take(3).collect();
            let extra: Vec<&String> = nb.iter().filter(|n| !na.contains(n)).take(3).collect();
            return differ(format!(
                "the faces of {} have other names: the design's {missing:?} are not there, \
                 {extra:?} are new",
                x.name
            ));
        }
        faces += na.len();
    }
    Ok(RoundTrip {
        skipped: None,
        report: format!(
            "Round trip: {} features, {} parameters, {} bodies and {} faces the same",
            a.len(),
            pa.len(),
            ba.len(),
            faces
        ),
    })
}
