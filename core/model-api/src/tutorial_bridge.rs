// SPDX-License-Identifier: MIT
//! The tutorials' bridge `query` (mitcad#2371, docs/tutorials.md "Reading
//! the design"): what a tutorial's page may read of the design it runs in,
//! so that a lesson checks the learner's geometry and not only that a
//! command finished. Tutorial pages come from any repository a user adds
//! as a source, so they read an allow-list of the model's read-only
//! queries, each answer cut to what a lesson checks:
//!
//! - `sketch`: the open sketch, or one by `name` (its name or uid): its
//!   plane, degrees of freedom, dimensions, curves and profiles;
//! - `feature`: one by `name`, else the last one before the timeline
//!   marker (of a `type` when given): its type, status, operation, extent
//!   and definition with the values in mm and radians;
//! - `bodies`: their count, how many are solids, and each one's kind and
//!   volume.
//!
//! Only the design of the tutorial's window: a `design` that names
//! another is refused, as are commands, other queries and fields the
//! query does not take, each in words. The application's bridge call
//! (`app/tutorials/MainWindowTutorials.cpp`) gives the open sketch and the
//! window's design.

use serde_json::{Map, Value, json};

use super::{ApiError, DocumentApi};
use mitcad_model::Document;
use mitcad_model::features::FeatureDef;
use mitcad_model::kernel::Kernel;
use mitcad_model::parameters::ParamId;

/// The queries a tutorial's page may ask, in the order the refusals list
/// them.
pub const TUTORIAL_QUERIES: [&str; 3] = ["sketch", "feature", "bodies"];

/// The answer to a tutorial page's `query` on `doc`, the design of the
/// tutorial's window named `design` (its file's name, as the bridge's
/// `state` gives it), while the sketch `open_sketch` (a uid, empty for
/// none) is open in it. A request the bridge does not allow is refused in
/// words.
pub fn tutorial_query<K: Kernel>(
    doc: &Document<K>,
    request: &str,
    open_sketch: &str,
    design: &str,
) -> Result<String, ApiError> {
    let request: Value = serde_json::from_str(request)
        .map_err(|error| ApiError(format!("The query is not JSON: {error}")))?;
    let Some(fields) = request.as_object() else {
        return Err(ApiError(
            "A query is an object, such as {\"query\": \"sketch\"}".to_owned(),
        ));
    };
    if let Some(command) = fields.get("cmd") {
        let command = command.as_str().unwrap_or("a command");
        return Err(ApiError(format!(
            "A tutorial page only reads the design: it cannot run {command}; the user makes every change"
        )));
    }
    let name = match fields.get("query") {
        Some(Value::String(name)) => name.as_str(),
        _ => {
            return Err(ApiError(format!(
                "A query needs \"query\", one of {}",
                listed()
            )));
        }
    };
    // Only the tutorial's own design.
    if let Some(given) = fields.get("design")
        && given.as_str() != Some(design)
    {
        let given = given
            .as_str()
            .map_or_else(|| given.to_string(), str::to_owned);
        return Err(ApiError(format!(
            "A tutorial page reads only the design of its own window ({}), not {given}",
            shown_design(design)
        )));
    }
    let takes: &[&str] = match name {
        "sketch" => &["name"],
        "feature" => &["name", "type"],
        "bodies" => &[],
        other => {
            return Err(ApiError(format!(
                "A tutorial page cannot read the query {other}; it reads {}",
                listed()
            )));
        }
    };
    if let Some(field) = fields
        .keys()
        .find(|key| !matches!(key.as_str(), "query" | "design") && !takes.contains(&key.as_str()))
    {
        let fields = if takes.is_empty() {
            "no fields".to_owned()
        } else {
            takes
                .iter()
                .map(|f| format!("\"{f}\""))
                .collect::<Vec<_>>()
                .join(" and ")
        };
        return Err(ApiError(format!(
            "The query {name} has no field \"{field}\"; it takes {fields}"
        )));
    }
    let answer = match name {
        "sketch" => sketch(doc, text_field(fields, "name")?, open_sketch)?,
        "feature" => feature(
            doc,
            text_field(fields, "name")?,
            text_field(fields, "type")?,
        )?,
        _ => bodies(doc)?,
    };
    Ok(answer.to_string())
}

/// `sketch, feature and bodies`.
fn listed() -> String {
    let (last, first) = TUTORIAL_QUERIES.split_last().expect("not empty");
    format!("{} and {last}", first.join(", "))
}

/// The design's name in a refusal; an untitled design has none.
fn shown_design(design: &str) -> &str {
    if design.is_empty() {
        "untitled"
    } else {
        design
    }
}

/// A field that is text when given.
fn text_field<'a>(fields: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, ApiError> {
    match fields.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.as_str())),
        Some(_) => Err(ApiError(format!("\"{key}\" is a text, a name or a uid"))),
    }
}

/// A model query's answer on `doc`.
fn ask<K: Kernel>(doc: &Document<K>, query: Value) -> Result<Value, ApiError> {
    let answer = doc.query(&query.to_string())?;
    serde_json::from_str(&answer).map_err(|error| ApiError(format!("unreadable answer: {error}")))
}

fn is_sketch(def: &FeatureDef) -> bool {
    matches!(def, FeatureDef::Sketch(_))
}

/// The sketch `name` (its name or uid), else the open one: its plane,
/// degrees of freedom, dimensions, curves and profiles.
fn sketch<K: Kernel>(
    doc: &Document<K>,
    name: Option<&str>,
    open_sketch: &str,
) -> Result<Value, ApiError> {
    let uid = match name {
        Some(name) => {
            let entry = doc
                .feature_by_text(name, Some(&is_sketch))
                .map_err(|error| ApiError(error.to_string()))?
                .ok_or_else(|| ApiError(format!("The design has no sketch {name}")))?;
            if !is_sketch(&entry.def) {
                return Err(ApiError(format!(
                    "{name} is a feature of the type {}, not a sketch",
                    entry.def.type_name()
                )));
            }
            entry.uid
        }
        None if open_sketch.is_empty() => {
            return Err(ApiError(
                "No sketch is open; name one: {\"query\": \"sketch\", \"name\": \"Sketch1\"}"
                    .to_owned(),
            ));
        }
        None => open_sketch
            .parse()
            .map_err(|_| ApiError(format!("The open sketch {open_sketch} is not a uid")))?,
    };
    let full = ask(doc, json!({"query": "sketch", "uid": uid}))?;
    let dof = full["dof"].as_u64().unwrap_or(0);
    let solved = full["solved"].as_bool().unwrap_or(false);
    let dimensions: Vec<Value> = full["dimensions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| {
            let driven = d["driven"].as_bool().unwrap_or(false);
            let mut value = json!({
                "id": d["id"],
                "type": d["type"],
                "value": d["measured"],
                "driven": driven,
            });
            // Its parameter's name: a driving one's own, a driven one's
            // reference.
            let parameter = if driven { &d["reference"] } else { &d["value"] };
            if parameter.is_string() {
                value["parameter"] = parameter.clone();
            }
            if d["active"] == json!(false) {
                value["active"] = json!(false);
            }
            value
        })
        .collect();
    // The curves that make profiles, by type; construction curves apart.
    let mut curves = Map::new();
    let mut construction = 0;
    for entity in full["entities"].as_array().into_iter().flatten() {
        let kind = entity["type"].as_str().unwrap_or_default();
        if kind == "point" || kind.is_empty() {
            continue;
        }
        if entity["construction"].as_bool().unwrap_or(false) || !entity["axis"].is_null() {
            construction += 1;
            continue;
        }
        let count = curves.entry(kind.to_owned()).or_insert(json!(0));
        *count = json!(count.as_u64().unwrap_or(0) + 1);
    }
    let profiles: Vec<Value> = full["regions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|region| {
            let mut value = json!({"area": region["area"], "centroid": region["centroid"]});
            if region.get("perimeter").is_some() {
                value["perimeter"] = region["perimeter"].clone();
            }
            value
        })
        .collect();
    let frame = &full["frame"];
    Ok(json!({
        "uid": full["uid"],
        "name": full["name"],
        "open": uid.to_string() == open_sketch,
        "plane": full["plane"],
        "origin": frame["origin"],
        "normal": frame["normal"],
        "solved": solved,
        "error": full["error"],
        "dof": dof,
        "fully_constrained": solved && dof == 0,
        "dimensions": dimensions,
        "curves": curves,
        "construction_curves": construction,
        "profiles": profiles,
    }))
}

/// The feature `name` (its name or uid), else the last one before the
/// timeline marker, of the type `kind` when given: its type, status,
/// operation, extent and definition with values.
fn feature<K: Kernel>(
    doc: &Document<K>,
    name: Option<&str>,
    kind: Option<&str>,
) -> Result<Value, ApiError> {
    let of_kind = |def: &FeatureDef| kind.is_none_or(|kind| def.type_name() == kind);
    let kind_filter: Option<&dyn Fn(&FeatureDef) -> bool> = kind.is_some().then_some(&of_kind);
    let entry = match name {
        Some(name) => {
            let entry = doc
                .feature_by_text(name, kind_filter)
                .map_err(|error| ApiError(error.to_string()))?
                .ok_or_else(|| ApiError(format!("The design has no feature {name}")))?;
            if !of_kind(&entry.def) {
                return Err(ApiError(format!(
                    "{name} is a feature of the type {}, not {}",
                    entry.def.type_name(),
                    kind.unwrap_or_default()
                )));
            }
            entry
        }
        None => doc
            .features()
            .take(doc.marker())
            .skip(doc.in_place_leading())
            .filter(|entry| of_kind(&entry.def))
            .last()
            .ok_or_else(|| {
                ApiError(match kind {
                    Some(kind) => {
                        format!("The design has no feature of the type {kind} before the marker")
                    }
                    None => "The design has no feature before the marker".to_owned(),
                })
            })?,
    };
    let uid = entry.uid;
    let full = ask(doc, json!({"query": "feature", "uid": uid}))?;
    // The definition with the parameters' values (mm, radians) in place of
    // their names.
    let params = doc.parameters();
    let def = entry
        .def
        .map_params(&mut |_: &str, id: &ParamId| Ok::<_, ()>(params.value(*id).unwrap_or(f64::NAN)))
        .ok()
        .and_then(|valued| serde_json::to_value(valued).ok())
        .unwrap_or(Value::Null);
    let mut value = json!({
        "uid": uid,
        "name": entry.name,
        "type": entry.def.type_name(),
        "suppressed": entry.suppressed,
        "status": full["status"],
        "error": full["error"],
        "rolled_back": doc.features().position(|f| f.uid == uid).is_some_and(|i| i >= doc.marker()),
    });
    for key in ["operation", "extent"] {
        if let Some(field) = def.get(key) {
            value[key] = field.clone();
        }
    }
    value["def"] = def;
    Ok(value)
}

/// The bodies at the timeline marker: their count, the solids among them,
/// and each one's name, kind and volume.
fn bodies<K: Kernel>(doc: &Document<K>) -> Result<Value, ApiError> {
    let full = ask(doc, json!({"query": "bodies", "properties": true}))?;
    let bodies: Vec<Value> = full
        .as_array()
        .into_iter()
        .flatten()
        .map(|body| {
            let kind = body["kind"].as_str().unwrap_or_default();
            let mut value = json!({
                "uid": body["uid"],
                "name": body["name"],
                "kind": kind,
                "solid": kind == "solid",
                "volume": body["volume"],
                "visible": body["visible"],
            });
            if let Some(component) = body.get("component") {
                value["component"] = component.clone();
            }
            value
        })
        .collect();
    let solids = bodies.iter().filter(|b| b["solid"] == json!(true)).count();
    Ok(json!({"count": bodies.len(), "solids": solids, "bodies": bodies}))
}
