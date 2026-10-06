// SPDX-License-Identifier: MIT
//! Command scripts with expectations, for `mitcad-cli run` and its tests.
//!
//! A script is a JSON array of steps: commands (`{"cmd": ...}`, see
//! `commands.md`), expectations (`{"expect": {...}}`) and comments
//! (`{"comment": "..."}`). An expectation checks the document after the
//! steps before it:
//!
//! - `"bodies": n`: the number of bodies at the timeline marker;
//! - `"body": "Body1"` (a name or uid) with any of `"volume"`, `"area"`
//!   (relative `"tolerance"`, default 1e-6), `"faces"`, `"edges"` (counts),
//!   `"has_face"`, `"has_edge"` and `"no_edge"` (topological names), and
//!   `"min"`, `"max"` (bounding box corners, the same tolerance);
//! - `"feature": "Fillet1"` (a name or uid) with `"status"` (`ok`,
//!   `warning`, `error`, `suppressed`, `rolled_back`) and `"error_contains"`
//!   (the error, or the warnings of a feature that succeeded with them);
//! - `"parameter": "d3"` with `"value"`;
//! - `"total_volume"`: the volumes of all bodies added up (relative
//!   `"tolerance"`); with `"body"`, `"bbox": {"min": [x, y, z], "max": […]}`
//!   and `"center": [x, y, z]` (the centre of mass), both within `"tolerance"`
//!   in millimetres;
//! - `"query": {...}` with `"result"`: the query's answer contains the
//!   result (objects: the keys given; arrays: element by element; numbers
//!   within the relative `"tolerance"`, default 1e-6);
//! - `"sketch": "Sketch1"` (a name or uid) with `"regions"` (its profile
//!   region keys, in any order) and `"dof"` (its degrees of freedom);
//! - components (F6): `"occurrence"` (a path from the root like
//!   `Arm:1/Pin:2`, or an occurrence placed once) with `"body"` measures
//!   the body as that occurrence places it in the design (`volume`,
//!   `area`, `bbox`, `center`, `min`, `max`); `"instances": n` counts the
//!   visible bodies in the design (every occurrence's) and
//!   `"world_volume"` adds up their volumes.

use serde::Deserialize;
use serde_json::Value;

use super::{ApiError, read_json};
use crate::document::{Document, status_message, status_text};
use crate::ids::OccurrenceUid;
use crate::kernel::Kernel;
use crate::topo::{EdgeName, FaceName, RegionKey};

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expectation {
    bodies: Option<usize>,
    body: Option<String>,
    volume: Option<f64>,
    area: Option<f64>,
    tolerance: Option<f64>,
    faces: Option<usize>,
    edges: Option<usize>,
    has_face: Option<FaceName>,
    has_edge: Option<EdgeName>,
    no_edge: Option<EdgeName>,
    min: Option<[f64; 3]>,
    max: Option<[f64; 3]>,
    feature: Option<String>,
    status: Option<String>,
    error_contains: Option<String>,
    parameter: Option<String>,
    value: Option<f64>,
    // Transforms, patterns and primitives (F4).
    total_volume: Option<f64>,
    bbox: Option<BoxExpectation>,
    center: Option<[f64; 3]>,
    query: Option<Value>,
    result: Option<Value>,
    // The general sketch (S1).
    sketch: Option<String>,
    regions: Option<Vec<RegionKey>>,
    dof: Option<usize>,
    // Components and occurrences (F6).
    occurrence: Option<String>,
    instances: Option<usize>,
    world_volume: Option<f64>,
}

/// A body's bounding box.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoxExpectation {
    min: [f64; 3],
    max: [f64; 3],
}

fn close(actual: f64, expected: f64, tolerance: f64) -> bool {
    (actual - expected).abs() <= tolerance * expected.abs().max(1.0)
}

impl<K: Kernel> Document<K> {
    /// Runs a script; fails at the first command or expectation that fails.
    /// Returns the results of the commands as a JSON array.
    pub fn run_script(&mut self, json: &str) -> Result<String, ApiError> {
        let Value::Array(steps) = read_json(json)? else {
            return Err(ApiError("a script is a JSON array of steps".to_owned()));
        };
        let mut results = Vec::new();
        for (i, step) in steps.into_iter().enumerate() {
            let fail = |e: ApiError| ApiError(format!("steps[{i}]: {e}"));
            if let Some(expect) = step.get("expect") {
                let expectation: Expectation = serde_json::from_value(expect.clone())
                    .map_err(|e| fail(ApiError(format!("invalid expectation: {e}"))))?;
                self.check(&expectation).map_err(fail)?;
            } else if step.get("comment").is_some()
                && step.as_object().is_some_and(|o| o.len() == 1)
            {
                continue;
            } else {
                results.push(self.run_command(step).map_err(fail)?);
            }
        }
        Ok(Value::Array(results).to_string())
    }

    fn check(&self, expect: &Expectation) -> Result<(), ApiError> {
        let fail = |message: String| Err(ApiError(format!("expected {message}")));
        let bodies = self.bodies();
        if let Some(count) = expect.bodies
            && bodies.len() != count
        {
            let names: Vec<_> = bodies.iter().map(|b| b.name.as_str()).collect();
            if names.is_empty() {
                return fail(format!("{count} bodies, got none"));
            }
            return fail(format!(
                "{count} bodies, got {}: {}",
                bodies.len(),
                names.join(", ")
            ));
        }
        if let Some(total) = expect.total_volume {
            let tolerance = expect.tolerance.unwrap_or(1e-6);
            let mut sum = 0.0;
            for body in &bodies {
                let mass = self
                    .kernel()
                    .mass_properties(body.shape)
                    .map_err(|e| ApiError(format!("{}: {e}", body.name)))?;
                sum += mass.volume;
            }
            if !close(sum, total, tolerance) {
                return fail(format!("a total volume of {total}, got {sum}"));
            }
        }
        self.check_instances(expect)?;
        if let Some(wanted) = &expect.body {
            let Some(body) = bodies
                .iter()
                .find(|b| &b.name == wanted || &b.uid.to_string() == wanted)
            else {
                return fail(format!("a body {wanted}"));
            };
            let kernel = self.kernel();
            let error = |e: crate::kernel::KernelError| ApiError(format!("{wanted}: {e}"));
            let tolerance = expect.tolerance.unwrap_or(1e-6);
            // With an occurrence, the body as that occurrence places it.
            let placed;
            let shape = match &expect.occurrence {
                Some(occurrence) => {
                    let path = self.instance_path(occurrence)?;
                    let component = self
                        .assembly()
                        .occurrence(*path.last().expect("not empty"))
                        .map(|o| o.component);
                    if component.is_none() || component != self.body_component(body.uid) {
                        return fail(format!(
                            "{wanted} to be a body of the component of {occurrence}"
                        ));
                    }
                    placed = kernel
                        .transform_shape(body.shape, &self.path_transform(&path), None)
                        .map_err(error)?;
                    &placed
                }
                None => body.shape,
            };
            let body = crate::document::BodyView {
                uid: body.uid,
                name: body.name.clone(),
                shape,
            };
            if expect.volume.is_some() || expect.area.is_some() {
                let mass = kernel.mass_properties(body.shape).map_err(error)?;
                if let Some(volume) = expect.volume.filter(|v| !close(mass.volume, *v, tolerance)) {
                    return fail(format!(
                        "{wanted} to have volume {volume}, got {}",
                        mass.volume
                    ));
                }
                if let Some(area) = expect.area.filter(|a| !close(mass.area, *a, tolerance)) {
                    return fail(format!("{wanted} to have area {area}, got {}", mass.area));
                }
            }
            if let Some(center) = expect.center {
                let mass = kernel.mass_properties(body.shape).map_err(error)?;
                if (0..3).any(|i| (mass.center[i] - center[i]).abs() > tolerance) {
                    return fail(format!(
                        "{wanted} to have its centre at {center:?}, got {:?}",
                        mass.center
                    ));
                }
            }
            if let Some(expected) = &expect.bbox {
                let actual = kernel.bounding_box(body.shape).map_err(error)?;
                let near =
                    |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() <= tolerance);
                if !actual.is_some_and(|b| near(b.min, expected.min) && near(b.max, expected.max)) {
                    return fail(format!(
                        "{wanted} to have the box {:?} .. {:?}, got {actual:?}",
                        expected.min, expected.max
                    ));
                }
            }
            if let Some(count) = expect.faces {
                let actual = kernel.faces(body.shape).map_err(error)?.len();
                if actual != count {
                    return fail(format!("{wanted} to have {count} faces, got {actual}"));
                }
            }
            if let Some(count) = expect.edges {
                let actual = kernel.edges(body.shape).map_err(error)?.len();
                if actual != count {
                    return fail(format!("{wanted} to have {count} edges, got {actual}"));
                }
            }
            if let Some(face) = &expect.has_face
                && kernel.count_faces(body.shape, face).map_err(error)? == 0
            {
                return fail(format!("{wanted} to have a face {face}"));
            }
            if let Some(edge) = &expect.has_edge
                && kernel.count_edges(body.shape, edge) == 0
            {
                return fail(format!("{wanted} to have an edge {edge}"));
            }
            if let Some(edge) = &expect.no_edge
                && kernel.count_edges(body.shape, edge) != 0
            {
                return fail(format!("{wanted} to have no edge {edge}"));
            }
            if expect.min.is_some() || expect.max.is_some() {
                let Some(bounds) = kernel.bounding_box(body.shape).map_err(error)? else {
                    return fail(format!("{wanted} to have a bounding box"));
                };
                for (corner, expected, actual) in [
                    ("min", expect.min, bounds.min),
                    ("max", expect.max, bounds.max),
                ] {
                    if let Some(expected) = expected
                        && (0..3).any(|i| !close(actual[i], expected[i], tolerance))
                    {
                        return fail(format!(
                            "{wanted} to have bounding box {corner} {expected:?}, got {actual:?}"
                        ));
                    }
                }
            }
        }
        if let Some(wanted) = &expect.feature {
            let Some(entry) = self
                .features()
                .find(|f| &f.name == wanted || &f.uid.to_string() == wanted)
            else {
                return fail(format!("a feature {wanted}"));
            };
            let status = self.status(entry.uid);
            let warnings = self.warnings(entry.uid);
            let text = status_text(status, warnings);
            let message = status_message(status, warnings).unwrap_or_default();
            if let Some(expected) = expect.status.as_ref().filter(|s| s.as_str() != text) {
                return fail(format!("{wanted} to be {expected}, got {text} {message}"));
            }
            if let Some(part) = &expect.error_contains
                && !message.contains(part.as_str())
            {
                return fail(format!(
                    "the error of {wanted} to contain '{part}', got '{message}'"
                ));
            }
        }
        if let Some(query) = &expect.query {
            let answer: Value = serde_json::from_str(&self.query(&query.to_string())?)
                .expect("queries answer JSON");
            if let Some(expected) = &expect.result {
                contains(&answer, expected, expect.tolerance.unwrap_or(1e-6), "")
                    .map_err(|e| ApiError(format!("expected {e} in the answer to {query}")))?;
            }
        }
        if let Some(name) = &expect.parameter {
            let parameters = self.parameters();
            let Some(id) = parameters.find(name) else {
                return fail(format!("a parameter {name}"));
            };
            let actual = parameters.value(id).unwrap_or(f64::NAN);
            if let Some(value) = expect.value.filter(|v| !close(actual, *v, 1e-12)) {
                return fail(format!("{name} = {value}, got {actual}"));
            }
        }
        if let Some(wanted) = &expect.sketch {
            self.check_sketch(wanted, expect)?;
        }
        Ok(())
    }

    /// The path from the root to an occurrence given by its path
    /// (`Arm:1/Pin:2`, `O1/O4`), or by its uid or name when its component
    /// is placed once.
    fn instance_path(&self, text: &str) -> Result<Vec<OccurrenceUid>, ApiError> {
        let a = self.assembly();
        if let Some(path) = a.find_path(text).filter(|p| !p.is_empty()) {
            return Ok(path);
        }
        let uid = self.occurrence_ref(text)?;
        let component = a.occurrence(uid).map(|o| o.component).expect("found");
        let mut paths: Vec<_> = a
            .paths_to(component)
            .into_iter()
            .filter(|p| p.last() == Some(&uid))
            .collect();
        match paths.len() {
            1 => Ok(paths.remove(0)),
            0 => Err(ApiError(format!("{text} is not placed in the design"))),
            _ => Err(ApiError(format!(
                "{text} is placed more than once; give its path from the root"
            ))),
        }
    }

    /// `instances` and `world_volume`: the visible bodies as placed.
    fn check_instances(&self, expect: &Expectation) -> Result<(), ApiError> {
        if expect.instances.is_none() && expect.world_volume.is_none() {
            return Ok(());
        }
        let shown: Vec<_> = self.instances().into_iter().filter(|i| i.visible).collect();
        if let Some(count) = expect.instances
            && shown.len() != count
        {
            let names: Vec<String> = shown
                .iter()
                .map(|i| format!("{}/{}", i.path_name, i.name))
                .collect();
            return Err(ApiError(format!(
                "expected {count} visible bodies in the design, got {}: {}",
                shown.len(),
                names.join(", ")
            )));
        }
        if let Some(total) = expect.world_volume {
            let tolerance = expect.tolerance.unwrap_or(1e-6);
            let mut sum = 0.0;
            for i in &shown {
                let shape = self.body_shape(i.body).expect("at the marker");
                // A rigid motion keeps the volume.
                sum += self
                    .kernel()
                    .mass_properties(shape)
                    .map_err(|e| ApiError(format!("{}: {e}", i.name)))?
                    .volume;
            }
            if !close(sum, total, tolerance) {
                return Err(ApiError(format!(
                    "expected a volume of {total} in the design, got {sum}"
                )));
            }
        }
        Ok(())
    }

    /// The profile regions and degrees of freedom of a sketch.
    fn check_sketch(&self, wanted: &str, expect: &Expectation) -> Result<(), ApiError> {
        let fail = |message: String| Err(ApiError(format!("expected {message}")));
        let Some(entry) = self
            .features()
            .find(|f| f.name == wanted || f.uid.to_string() == wanted)
        else {
            return fail(format!("a sketch {wanted}"));
        };
        if let Some(regions) = &expect.regions {
            let Some(output) = self.sketch_output(entry.uid) else {
                return fail(format!("{wanted} to be an evaluated sketch"));
            };
            let mut actual: Vec<String> =
                output.regions.iter().map(|r| r.key.to_string()).collect();
            let mut expected: Vec<String> = regions.iter().map(ToString::to_string).collect();
            actual.sort();
            expected.sort();
            if actual != expected {
                return fail(format!(
                    "{wanted} to have the regions {expected:?}, got {actual:?}"
                ));
            }
        }
        if let Some(dof) = expect.dof {
            let Some(output) = self.sketch_output(entry.uid) else {
                return fail(format!("{wanted} to be an evaluated sketch"));
            };
            let actual = output.status().dof;
            if actual != dof {
                return fail(format!(
                    "{wanted} to have {dof} degrees of freedom, got {actual}"
                ));
            }
        }
        Ok(())
    }
}

/// Checks that `actual` contains `expected`; the error names the first
/// difference by its path (`bodies[0].volume`).
fn contains(actual: &Value, expected: &Value, tolerance: f64, path: &str) -> Result<(), String> {
    let at = |key: &dyn std::fmt::Display| {
        if path.is_empty() {
            key.to_string()
        } else {
            format!("{path}.{key}")
        }
    };
    let here = if path.is_empty() { "the answer" } else { path };
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => {
            let (a, e) = (
                a.as_f64().unwrap_or(f64::NAN),
                e.as_f64().unwrap_or(f64::NAN),
            );
            if close(a, e, tolerance) {
                Ok(())
            } else {
                Err(format!("{here} = {e}, got {a}"))
            }
        }
        (Value::Object(a), Value::Object(e)) => {
            for (key, value) in e {
                let Some(found) = a.get(key) else {
                    return Err(format!("{} (missing)", at(key)));
                };
                contains(found, value, tolerance, &at(key))?;
            }
            Ok(())
        }
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                return Err(format!("{here} with {} items, got {}", e.len(), a.len()));
            }
            for (i, (found, value)) in a.iter().zip(e).enumerate() {
                contains(found, value, tolerance, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        (a, e) if a == e => Ok(()),
        (a, e) => Err(format!("{here} = {e}, got {a}")),
    }
}
