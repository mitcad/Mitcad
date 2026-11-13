// SPDX-License-Identifier: MIT
//! The `parameter_range` query (mitcad#1730): the design evaluated at
//! values of one parameter (by default its min and its max) without
//! changing the document. Each value is set with `set_parameter` on a fork
//! of the document ([`Document::fork`]: the same definition and cached
//! results, a kernel of its own, no undo history), which recomputes from
//! the document's cached results; the answer gives per value the bodies'
//! volumes and boxes, the body count, the failed features and the new
//! warnings, compared with the current value. The document, its undo
//! history and its results are left as they were. One time limit covers
//! all values, as `model_render`'s covers its render. See `commands.md`,
//! "A parameter's range tried on a copy".

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiError, DocumentApi};
use mitcad_model::Document;
use mitcad_model::expr::{DEFAULT_DECIMALS, Limited, Parameter, format_value};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::monitor::{self, Cancelled, RecomputeMonitor};
use mitcad_model::notice::Notice;

/// The most values one query tries.
const MOST_VALUES: usize = 50;
/// The time limit when the query gives none, in seconds.
const DEFAULT_TIME_LIMIT_S: f64 = 30.0;
/// The shortest and the longest time limit, in seconds.
const TIME_LIMIT_RANGE_S: (f64, f64) = (1.0, 600.0);
/// A box corner that moves less than this (mm) has not changed: the
/// baseline tolerance of geometric comparisons.
const LENGTH_TOLERANCE: f64 = 1e-6;
/// A volume that differs by less than this share of it (at least 1 mm³
/// times it) has not changed.
const VOLUME_TOLERANCE: f64 = 1e-6;

/// The `parameter_range` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ParameterRangeQuery {
    /// The parameter by name.
    parameter: String,
    /// The values to try, in millimetres, radians or as plain numbers (as
    /// `set_parameter`'s `value`); its min and max when left out.
    #[serde(default)]
    values: Option<Vec<f64>>,
    /// Evenly spaced values from its min to its max, both included.
    #[serde(default)]
    steps: Option<usize>,
    /// The most seconds all values may take together.
    #[serde(default)]
    time_limit_s: Option<f64>,
}

/// Answers the query within its time limit.
pub(super) fn query<K: Kernel>(
    doc: &Document<K>,
    query: &ParameterRangeQuery,
) -> Result<Value, ApiError> {
    let limit = query.time_limit_s.unwrap_or(DEFAULT_TIME_LIMIT_S);
    let (shortest, longest) = TIME_LIMIT_RANGE_S;
    if !(shortest..=longest).contains(&limit) {
        return Err(ApiError(format!(
            "time_limit_s must be {shortest} to {longest} seconds, got {limit}"
        )));
    }
    let mut answer = evaluate(doc, query, monitor::now() + Duration::from_secs_f64(limit))?;
    answer["time_limit_s"] = json!(limit);
    Ok(answer)
}

/// Answers the query, trying no value after `deadline` (on the clock of
/// [`monitor::now`]).
fn evaluate<K: Kernel>(
    doc: &Document<K>,
    query: &ParameterRangeQuery,
    deadline: Instant,
) -> Result<Value, ApiError> {
    let name = query.parameter.as_str();
    let parameter = doc
        .parameters()
        .find(name)
        .and_then(|id| doc.parameters().get(id))
        .ok_or_else(|| ApiError(format!("unknown parameter {name}")))?;
    if parameter.is_text() {
        return Err(ApiError(format!(
            "{name} is a text parameter; parameter_range tries numbers"
        )));
    }
    let unit = parameter.unit();
    let values = values_of(parameter, query)?;
    let started = Instant::now();
    let current = Summary::of(doc);
    let mut current_json = current.json();
    current_json["value"] = json!(parameter.value());
    current_json["text"] = json!(format_value(parameter.value(), unit, DEFAULT_DECIMALS));

    let mut timed_out = false;
    let mut answers = Vec::with_capacity(values.len());
    for value in values {
        let mut answer = json!({
            "value": value,
            "text": format_value(value, unit, DEFAULT_DECIMALS),
        });
        if timed_out || monitor::now() >= deadline {
            timed_out = true;
            answer["timed_out"] = json!(true);
            answers.push(answer);
            continue;
        }
        let at = Instant::now();
        match at_value(doc, name, value, deadline)? {
            Tried::TimedOut => {
                timed_out = true;
                answer["timed_out"] = json!(true);
            }
            Tried::Refused(error) => {
                answer["error"] = json!(error);
            }
            Tried::Evaluated { summary, applied } => {
                let compared = summary.compared(&current);
                if let Value::Object(fields) = compared {
                    for (key, field) in fields {
                        answer[key] = field;
                    }
                }
                if let Some((applied, limited)) = applied {
                    answer["applied"] = json!(applied);
                    answer["applied_text"] = json!(format_value(applied, unit, DEFAULT_DECIMALS));
                    answer["limited"] = json!(limited.map(Limited::as_str));
                }
            }
        }
        answer["timing_ms"] = json!(at.elapsed().as_millis() as u64);
        answers.push(answer);
    }
    Ok(json!({
        "parameter": name,
        "current": current_json,
        "values": answers,
        "timed_out": timed_out,
        "timing_ms": started.elapsed().as_millis() as u64,
    }))
}

/// The values a query asks for: its `values`, or `steps` values (two by
/// default) from the parameter's min to its max.
fn values_of(parameter: &Parameter, query: &ParameterRangeQuery) -> Result<Vec<f64>, ApiError> {
    let name = parameter.name();
    match (&query.values, query.steps) {
        (Some(_), Some(_)) => Err(ApiError(
            "parameter_range takes values or steps, not both".to_owned(),
        )),
        (Some(values), None) => {
            if values.is_empty() || values.len() > MOST_VALUES {
                return Err(ApiError(format!(
                    "values must have 1 to {MOST_VALUES} numbers, got {}",
                    values.len()
                )));
            }
            Ok(values.clone())
        }
        (None, steps) => {
            let steps = steps.unwrap_or(2);
            if !(2..=MOST_VALUES).contains(&steps) {
                return Err(ApiError(format!(
                    "steps must be 2 to {MOST_VALUES}, got {steps}"
                )));
            }
            let bounds = parameter.bounds();
            let (min, max) = match (bounds.min, bounds.max) {
                (Some(min), Some(max)) => (min, max),
                (None, None) => {
                    return Err(ApiError(format!(
                        "{name} has no min and no max; give values"
                    )));
                }
                (None, Some(_)) => {
                    return Err(ApiError(format!("{name} has no min; give values")));
                }
                (Some(_), None) => {
                    return Err(ApiError(format!("{name} has no max; give values")));
                }
            };
            let last = (steps - 1) as f64;
            Ok((0..steps)
                .map(|i| min + (max - min) * i as f64 / last)
                .collect())
        }
    }
}

/// What one value gave.
enum Tried {
    /// The time limit passed before its recompute ended.
    TimedOut,
    /// `set_parameter` refused the value (a strict limit, a parameter a
    /// feature measures).
    Refused(String),
    /// The design at the value; `applied` when the parameter's limits made
    /// another value of it.
    Evaluated {
        summary: Summary,
        applied: Option<(f64, Option<Limited>)>,
    },
}

/// Sets the parameter to `value` on a fork of `doc` and reads the design
/// there. A cancel of the document's own job rejects the query.
fn at_value<K: Kernel>(
    doc: &Document<K>,
    name: &str,
    value: f64,
    deadline: Instant,
) -> Result<Tried, ApiError> {
    let mut fork = doc.fork().ok_or_else(|| {
        ApiError("this design's geometry kernel cannot evaluate a copy of the design".to_owned())
    })?;
    // The document's job, while it runs: its cancel and memory guard stop
    // this one too.
    let parents: Vec<Arc<RecomputeMonitor>> = doc
        .monitor()
        .filter(|m| !m.is_cancelled())
        .cloned()
        .into_iter()
        .collect();
    let job = Arc::new(RecomputeMonitor::within(parents, Some(deadline)));
    fork.set_monitor(Some(Arc::clone(&job)));
    let command = json!({"cmd": "set_parameter", "name": name, "value": value});
    let result = fork.command(&command.to_string());
    if job.past_deadline() {
        return Ok(Tried::TimedOut);
    }
    if job.is_cancelled() {
        return Err(ApiError(
            job.cancel_reason()
                .unwrap_or_else(|| Cancelled::MESSAGE.to_owned()),
        ));
    }
    if let Err(error) = result {
        return Ok(Tried::Refused(error.0));
    }
    let applied = fork
        .parameters()
        .find(name)
        .and_then(|id| fork.parameters().get(id))
        .filter(|p| p.value() != value)
        .map(|p| (p.value(), p.limited()));
    Ok(Tried::Evaluated {
        summary: Summary::of(&fork),
        applied,
    })
}

/// A body as the query compares it.
struct BodyState {
    uid: BodyUid,
    name: String,
    /// None when the geometry kernel cannot measure it.
    volume: Option<f64>,
    bbox: Option<BoundingBox>,
}

/// A feature's failure or warning.
struct Note {
    feature: FeatureUid,
    name: String,
    text: String,
}

/// The design at the timeline marker as the query compares it.
struct Summary {
    bodies: Vec<BodyState>,
    failed: Vec<Note>,
    warnings: Vec<Note>,
}

impl Summary {
    fn of<K: Kernel>(doc: &Document<K>) -> Self {
        let kernel = doc.kernel();
        let bodies = doc
            .bodies()
            .into_iter()
            .map(|body| BodyState {
                uid: body.uid,
                volume: kernel.mass_properties(body.shape).ok().map(|m| m.volume),
                bbox: kernel.bounding_box(body.shape).ok().flatten(),
                name: body.name,
            })
            .collect();
        let mut failed = Vec::new();
        let mut warnings = Vec::new();
        for item in doc.timeline() {
            let note = |text: &str| Note {
                feature: item.entry.uid,
                name: item.entry.name.clone(),
                text: text.to_owned(),
            };
            if let Some(error) = item.error() {
                failed.push(note(error));
            }
            warnings.extend(item.warnings.iter().map(|w| note(w.as_str())));
        }
        Self {
            bodies,
            failed,
            warnings,
        }
    }

    /// The current design: `body_count`, `bodies`, `failed_features` and
    /// `feature_warnings`.
    fn json(&self) -> Value {
        json!({
            "body_count": self.bodies.len(),
            "bodies": self.bodies.iter().map(body_json).collect::<Vec<_>>(),
            "failed_features": self.failed.iter().map(failure_json).collect::<Vec<_>>(),
            "feature_warnings": self.warnings.iter().map(warning_json).collect::<Vec<_>>(),
        })
    }

    /// The design at a value against the current one (`now`): each body
    /// with its `volume_change` and `bbox_change` (or `new`), the bodies
    /// gone, the body count and its change, every failed feature (`new`
    /// when it does not fail now), the warnings it does not have now, and
    /// `changed` when any of them differs.
    fn compared(&self, now: &Summary) -> Value {
        let mut changed = false;
        let mut bodies = Vec::with_capacity(self.bodies.len());
        for body in &self.bodies {
            let mut value = body_json(body);
            let differs = match now.bodies.iter().find(|b| b.uid == body.uid) {
                None => {
                    value["new"] = json!(true);
                    true
                }
                Some(before) => {
                    let mut differs = false;
                    match (body.volume, before.volume) {
                        (Some(at), Some(was)) => {
                            value["volume_change"] = json!(at - was);
                            differs |= (at - was).abs() > VOLUME_TOLERANCE * was.abs().max(1.0);
                        }
                        (at, was) => differs |= at.is_some() != was.is_some(),
                    }
                    match (body.bbox, before.bbox) {
                        (Some(at), Some(was)) => {
                            let min = difference(at.min, was.min);
                            let max = difference(at.max, was.max);
                            value["bbox_change"] = json!({"min": min, "max": max});
                            differs |= min
                                .iter()
                                .chain(max.iter())
                                .any(|d| d.abs() > LENGTH_TOLERANCE);
                        }
                        (at, was) => differs |= at.is_some() != was.is_some(),
                    }
                    differs
                }
            };
            value["changed"] = json!(differs);
            changed |= differs;
            bodies.push(value);
        }
        let removed: Vec<Value> = now
            .bodies
            .iter()
            .filter(|b| !self.bodies.iter().any(|at| at.uid == b.uid))
            .map(|b| json!({"uid": b.uid, "name": b.name}))
            .collect();
        changed |= !removed.is_empty();
        let failed: Vec<Value> = self
            .failed
            .iter()
            .map(|f| {
                let mut value = failure_json(f);
                let new = !now.failed.iter().any(|n| n.feature == f.feature);
                value["new"] = json!(new);
                value
            })
            .collect();
        changed |=
            self.failed.len() != now.failed.len() || failed.iter().any(|f| f["new"] == json!(true));
        let new_warnings: Vec<Value> = self
            .warnings
            .iter()
            .filter(|w| {
                !now.warnings
                    .iter()
                    .any(|n| n.feature == w.feature && n.text == w.text)
            })
            .map(warning_json)
            .collect();
        changed |= !new_warnings.is_empty();
        json!({
            "changed": changed,
            "body_count": self.bodies.len(),
            "body_count_change": self.bodies.len() as i64 - now.bodies.len() as i64,
            "bodies": bodies,
            "removed_bodies": removed,
            "failed_features": failed,
            "new_warnings": new_warnings,
        })
    }
}

fn difference(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    // + 0.0 turns -0.0 into 0.0 in text.
    std::array::from_fn(|i| a[i] - b[i] + 0.0)
}

fn body_json(body: &BodyState) -> Value {
    let mut value = json!({"uid": body.uid, "name": body.name});
    if let Some(volume) = body.volume {
        value["volume"] = json!(volume);
    }
    if let Some(bbox) = body.bbox {
        value["bbox"] = json!({"min": bbox.min, "max": bbox.max});
    }
    value
}

fn failure_json(note: &Note) -> Value {
    json!({"feature": note.feature, "name": note.name, "error": note.text})
}

/// A feature's warning as its own text (`warning`, for agents) and as a
/// person reads it (`message`, as the timeline's notice says it).
fn warning_json(note: &Note) -> Value {
    json!({
        "feature": note.feature,
        "name": note.name,
        "warning": note.text,
        "message": Notice::warning(note.text.clone()).message,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{ParameterRangeQuery, evaluate};
    use crate::DocumentApi;
    use mitcad_model::Document;
    use mitcad_model::monitor;
    use mitcad_model::testing::MockKernel;

    fn range_query(value: Value) -> ParameterRangeQuery {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn no_value_is_tried_after_the_time_limit() {
        let mut doc = Document::new(MockKernel::default());
        doc.command(
            r#"{"cmd": "add_parameter", "name": "Width", "expression": "40 mm",
                "min": 30, "max": 50}"#,
        )
        .unwrap();
        let query = range_query(json!({"parameter": "Width", "steps": 3}));
        // A deadline that has passed already.
        let answer = evaluate(&doc, &query, monitor::now()).unwrap();
        assert_eq!(answer["timed_out"], json!(true), "{answer}");
        let values = answer["values"].as_array().unwrap();
        assert_eq!(values.len(), 3);
        for value in values {
            assert_eq!(value["timed_out"], json!(true), "{value}");
            assert!(value.get("bodies").is_none(), "{value}");
        }
        assert_eq!(answer["current"]["value"], json!(40.0));
    }

    #[test]
    fn the_values_are_the_steps_from_min_to_max() {
        let mut doc = Document::new(MockKernel::default());
        doc.command(
            r#"{"cmd": "add_parameter", "name": "Width", "expression": "40 mm",
                "min": 30, "max": 50}"#,
        )
        .unwrap();
        let answer: Value = serde_json::from_str(
            &doc.query(r#"{"query": "parameter_range", "parameter": "Width", "steps": 5}"#)
                .unwrap(),
        )
        .unwrap();
        let values: Vec<f64> = answer["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["value"].as_f64().unwrap())
            .collect();
        assert_eq!(values, [30.0, 35.0, 40.0, 45.0, 50.0]);
        assert_eq!(answer["values"][0]["text"], json!("30 mm"));
        assert_eq!(answer["timed_out"], json!(false));
        assert_eq!(answer["time_limit_s"], json!(30.0));
    }
}
