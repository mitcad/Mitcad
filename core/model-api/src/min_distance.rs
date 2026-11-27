// SPDX-License-Identifier: MIT
//! The `minimum_distance` query (mitcad#2519; commands.md, "Minimum
//! distance"): the smallest distance between two selections, each one or
//! more picks of bodies, faces, edges, vertices and components, of a kept
//! Minimum Distance analysis (by its `name`) or of a definition given as
//! fields, with the two closest points and, with a clearance, whether the
//! selections come closer than it.
//!
//! A component's pick (an `occurrence` alone) stands for every body placed
//! in that occurrence and below it. Every pair of a pick of the first
//! selection and one of the second is measured as the `measure` query
//! measures two selections (`AnalysisOps::pick_separation`: the kernel's
//! exact distance in the design's coordinates), the pairs nearest by their
//! placed boxes first: a pair whose boxes are farther apart than the
//! smallest distance found so far cannot be nearer, so it is not measured.
//! What a pair gave is kept by the picks' key (`AnalysisOps::picks_key`:
//! the picks as named, their bodies' shape versions and placements), so a
//! shown analysis asked again after a change elsewhere measures nothing.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::prelude::*;
use mitcad_model::analysis::Separation;
use mitcad_model::datum::Vec3;
use mitcad_model::features::OccurrencePath;
use mitcad_model::kernel::Kernel;
use mitcad_model::{
    AnalysisDef, Document, MeasurePick, MinimumDistanceAnalysis, ModelError, check_clearance,
};

/// Closer than this, mm, the selections touch.
const TOUCHING: f64 = 1e-6;

/// How many pairs' separations are kept at most.
const KEPT_PAIRS: usize = 1024;

/// The `minimum_distance` query: a kept analysis by its `name`, or a
/// definition's `first`, `second` and `clearance`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MinimumDistanceQuery {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    first: Vec<MeasurePick>,
    #[serde(default)]
    second: Vec<MeasurePick>,
    #[serde(default)]
    clearance: Option<f64>,
}

/// The separations of pairs of picks already measured, by the pair's key,
/// the most recently used last.
#[derive(Default)]
struct SeparationCache {
    entries: Mutex<VecDeque<(u128, Separation)>>,
}

impl SeparationCache {
    fn found(&self, key: u128) -> Option<Separation> {
        let mut entries = self.entries.lock().ok()?;
        let i = entries.iter().position(|(k, _)| *k == key)?;
        let entry = entries.remove(i)?;
        let found = entry.1;
        entries.push_back(entry);
        Some(found)
    }

    fn keep(&self, key: u128, separation: Separation) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|(k, _)| *k != key);
            entries.push_back((key, separation));
            while entries.len() > KEPT_PAIRS {
                entries.pop_front();
            }
        }
    }
}

/// A pick of a selection as measured: a body (with a face, edge or vertex
/// of it) placed by its occurrence, and which of the selection's picks it
/// comes from (a component's pick gives one per body).
struct Measured {
    pick: MeasurePick,
    from: usize,
    /// The body's box placed in the design, when the kernel gives one.
    bounds: Option<([f64; 3], [f64; 3])>,
}

/// The answer to the `minimum_distance` query: the distance and the
/// closest points, or `{"error"}` when a pick is not there.
pub(super) fn minimum_distance_json<K: Kernel>(
    doc: &Document<K>,
    query: &MinimumDistanceQuery,
) -> Result<Value, ApiError> {
    let def = match &query.name {
        Some(name) => {
            if !query.first.is_empty() || !query.second.is_empty() || query.clearance.is_some() {
                return Err(ApiError(
                    "give a kept analysis' name or the picks and the clearance, not both"
                        .to_owned(),
                ));
            }
            let analysis = doc
                .analyses()
                .iter()
                .find(|a| &a.name == name)
                .ok_or_else(|| ApiError(format!("there is no analysis {name}")))?;
            let AnalysisDef::MinimumDistance(def) = &analysis.def else {
                return Err(ApiError(format!(
                    "{name} is not a minimum distance analysis"
                )));
            };
            def.clone()
        }
        None => {
            if query.first.is_empty() || query.second.is_empty() {
                return Err(ApiError(
                    "the minimum distance needs a first and a second selection (first, second) \
                     or a kept analysis' name"
                        .to_owned(),
                ));
            }
            if let Some(clearance) = query.clearance {
                check_clearance(clearance).map_err(ApiError)?;
            }
            MinimumDistanceAnalysis {
                first: query.first.clone(),
                second: query.second.clone(),
                clearance: query.clearance,
            }
        }
    };
    if let Err(error) = doc.minimum_distance_check(&def) {
        return Ok(json!({"error": error}));
    }
    let (first, second) = match (measured(doc, &def.first), measured(doc, &def.second)) {
        (Ok(first), Ok(second)) => (first, second),
        (Err(error), _) | (_, Err(error)) => return Ok(json!({"error": error})),
    };
    // Every pair but a pick with itself, the nearest by their boxes first.
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (i, a) in first.iter().enumerate() {
        for (j, b) in second.iter().enumerate() {
            if a.pick != b.pick {
                pairs.push((box_gap(a.bounds, b.bounds), i, j));
            }
        }
    }
    if pairs.is_empty() {
        return Ok(json!({"error": "the two selections are the same"}));
    }
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
    let cache = doc.side_cache::<SeparationCache>();
    let mut best: Option<(Separation, usize, usize)> = None;
    let mut measured_pairs = 0usize;
    for (gap, i, j) in &pairs {
        if best
            .as_ref()
            .is_some_and(|(found, _, _)| *gap >= found.distance)
        {
            break;
        }
        if doc.monitor().is_some_and(|m| m.is_cancelled()) {
            return Err(ModelError::Cancelled.into());
        }
        let (a, b) = (&first[*i].pick, &second[*j].pick);
        // A pick's occurrence that does not place it: why, in words.
        let key = match doc.picks_key(a, Some(b)) {
            Ok(key) => key,
            Err(error) => return Ok(json!({"error": error.0})),
        };
        let separation = match cache.found(key) {
            Some(found) => found,
            None => match doc.pick_separation(a, b) {
                Ok(found) => {
                    cache.keep(key, found);
                    found
                }
                // A face, edge or vertex not there: why, in words.
                Err(error) => return Ok(json!({"error": error.0})),
            },
        };
        measured_pairs += 1;
        let distance = if separation.inside {
            0.0
        } else {
            separation.distance
        };
        if best
            .as_ref()
            .is_none_or(|(found, _, _)| distance < found.distance)
        {
            best = Some((
                Separation {
                    distance,
                    ..separation
                },
                *i,
                *j,
            ));
        }
    }
    let Some((found, i, j)) = best else {
        return Ok(json!({"error": "the two selections are the same"}));
    };
    let mut answer = json!({
        "distance": found.distance,
        "points": [found.on_a, found.on_b],
        "touching": found.distance <= TOUCHING,
        "inside": found.inside,
        "first": pick_json(doc, &first[i]),
        "second": pick_json(doc, &second[j]),
        "pairs": pairs.len(),
        "measured": measured_pairs,
    });
    if let Some(clearance) = def.clearance {
        answer["clearance"] = json!(clearance);
        answer["within_clearance"] = json!(found.distance < clearance);
    }
    Ok(answer)
}

/// A selection's picks as measured: a component's pick as every body
/// placed in its occurrence and below, each with its placed box.
fn measured<K: Kernel>(doc: &Document<K>, picks: &[MeasurePick]) -> Result<Vec<Measured>, String> {
    let instances = doc.instances();
    let mut out = Vec::new();
    for (from, pick) in picks.iter().enumerate() {
        match (pick.body, &pick.occurrence) {
            (Some(_), _) => out.push(Measured {
                bounds: bounds(doc, pick),
                pick: pick.clone(),
                from,
            }),
            (None, Some(path)) => {
                let before = out.len();
                for instance in instances.iter().filter(|i| i.path.starts_with(&path.0)) {
                    let pick = MeasurePick {
                        body: Some(instance.body),
                        occurrence: (!instance.path.is_empty())
                            .then(|| OccurrencePath(instance.path.clone())),
                        ..MeasurePick::default()
                    };
                    out.push(Measured {
                        bounds: bounds(doc, &pick),
                        pick,
                        from,
                    });
                }
                if out.len() == before {
                    return Err(if path.is_empty() {
                        "the design has no bodies at the timeline marker".to_owned()
                    } else {
                        format!("component occurrence {path} has no bodies at the timeline marker")
                    });
                }
            }
            (None, None) => {
                return Err("a pick needs a body, or a component's occurrence".to_owned());
            }
        }
    }
    Ok(out)
}

/// The box of a pick's body placed in the design, when the kernel gives
/// one (a face, an edge or a vertex lies in its body's box).
fn bounds<K: Kernel>(doc: &Document<K>, pick: &MeasurePick) -> Option<([f64; 3], [f64; 3])> {
    let body = pick.body?;
    let local = doc.body_shape(body)?;
    let found = doc.kernel().enclosing_box(local).ok()??;
    let transform = doc
        .component_transform(
            doc.body_component(body),
            pick.occurrence.as_ref(),
            "the pick",
        )
        .ok()?;
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for corner in found.corners() {
        let p = transform.apply_point(corner);
        for k in 0..3 {
            low[k] = low[k].min(p[k]);
            high[k] = high[k].max(p[k]);
        }
    }
    Some((low, high))
}

/// How far apart two boxes are (0 when they meet, or without a box): no
/// two points in them are nearer.
fn box_gap(a: Option<([f64; 3], [f64; 3])>, b: Option<([f64; 3], [f64; 3])>) -> f64 {
    let (Some((a_low, a_high)), Some((b_low, b_high))) = (a, b) else {
        return 0.0;
    };
    let gap: Vec3 =
        std::array::from_fn(|k| (b_low[k] - a_high[k]).max(a_low[k] - b_high[k]).max(0.0));
    mitcad_vec3::norm(gap)
}

/// The pick a closest point lies on: which of the selection's picks it
/// comes from, its body and the body's name, the occurrence and the face,
/// edge or vertex.
fn pick_json<K: Kernel>(doc: &Document<K>, measured: &Measured) -> Value {
    let mut value = serde_json::to_value(&measured.pick).unwrap_or_else(|_| json!({}));
    value["pick"] = json!(measured.from);
    if let Some(body) = measured.pick.body {
        value["name"] = json!(doc.body_name(body));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::box_gap;

    #[test]
    fn boxes_apart_meeting_and_unknown() {
        let unit = Some(([0.0; 3], [1.0; 3]));
        let beside = Some(([6.0, 0.0, 0.0], [7.0, 1.0, 1.0]));
        assert!((box_gap(unit, beside) - 5.0).abs() < 1e-12);
        assert!((box_gap(beside, unit) - 5.0).abs() < 1e-12);
        let corner = Some(([4.0, 5.0, 1.0], [5.0, 6.0, 2.0]));
        assert!((box_gap(unit, corner) - 5.0).abs() < 1e-12);
        let touching = Some(([1.0, 0.5, 0.5], [2.0, 2.0, 2.0]));
        assert_eq!(box_gap(unit, touching), 0.0);
        assert_eq!(box_gap(unit, None), 0.0);
    }
}
