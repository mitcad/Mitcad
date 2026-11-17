// SPDX-License-Identifier: MIT
//! A Flex item let hang (mitcad#1713; `commands.md`, "Hanging cables"; the
//! model's side is `Document::flex_hang` and the rest): `flex.hang` lets an
//! item hang toward the design's direction of gravity, `flex.hang_step`
//! moves it one step (dragged by a point, or settling), `flex.hang_set`
//! pins and lets go its nodes or changes its length, `flex.hang_lock` keeps
//! where it rests as its nodes (one undo step) and `flex.hang_cancel` puts
//! it back. The `flex_hang` query gives the chain to draw. The commands are
//! variants of `flex::FlexCommand` (their namespace); their fields and
//! answers are here.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, HangState};

/// `flex.hang`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HangCommand {
    /// The Flex item.
    flex: FeatureUid,
    /// The ids of the nodes that stay where they are; when left out, both
    /// ends and every node attached to a face, an edge, a point or a port.
    #[serde(default)]
    pins: Option<Vec<String>>,
    /// How strongly the air holds it back, 0 to 1 (0.5 when left out).
    #[serde(default)]
    damping: Option<f64>,
}

/// The pointer's hold of a hanging item: where it took the chain and where
/// it is now, in the item's component.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GrabInput {
    point: [f64; 3],
    target: [f64; 3],
}

/// `flex.hang_step`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HangStepCommand {
    /// The seconds to simulate (1/30 when left out, at most 10).
    #[serde(default)]
    seconds: Option<f64>,
    /// A point of the chain held by the pointer; left out, it is let go.
    #[serde(default)]
    grab: Option<GrabInput>,
}

/// `flex.hang_set`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HangSetCommand {
    /// The ids of the nodes that stay where they are (the others let go).
    #[serde(default)]
    pins: Option<Vec<String>>,
    /// The item's length.
    #[serde(default)]
    length: Option<f64>,
    /// How strongly the air holds it back, 0 to 1.
    #[serde(default)]
    damping: Option<f64>,
}

/// Whether a command is one of a hanging item's that change only the
/// document's runtime state, never the design (no undo step, no revision,
/// nothing recomputed): `flex.hang_step`, `flex.hang_set` and
/// `flex.hang_cancel`. `DocumentApi::command` runs them without the work of
/// a design change.
pub(super) fn is_runtime_command(value: &Value) -> bool {
    value
        .get("cmd")
        .and_then(Value::as_str)
        .is_some_and(|cmd| matches!(cmd, "flex.hang_step" | "flex.hang_set" | "flex.hang_cancel"))
}

pub(super) fn hang<K: Kernel>(
    doc: &mut Document<K>,
    command: HangCommand,
) -> Result<Value, ApiError> {
    let state = doc.flex_hang(command.flex, command.pins, command.damping)?;
    Ok(state_json(&state))
}

pub(super) fn step<K: Kernel>(
    doc: &mut Document<K>,
    command: HangStepCommand,
) -> Result<Value, ApiError> {
    let grab = command.grab.map(|g| (g.point, g.target));
    let state = doc.flex_hang_step(command.seconds, grab)?;
    Ok(state_json(&state))
}

pub(super) fn set<K: Kernel>(
    doc: &mut Document<K>,
    command: HangSetCommand,
) -> Result<Value, ApiError> {
    if command.pins.is_none() && command.length.is_none() && command.damping.is_none() {
        return Err(ApiError(
            "flex.hang_set takes pins, a length, a damping or more of them".to_owned(),
        ));
    }
    let state = doc.flex_hang_set(command.pins, command.length, command.damping)?;
    Ok(state_json(&state))
}

pub(super) fn cancel<K: Kernel>(doc: &mut Document<K>) -> Value {
    json!({"cancelled": doc.flex_hang_cancel()})
}

/// The hanging item as the commands and the `flex_hang` query give it: the
/// chain's points (`polyline`) and the section's `y` at each (`normals`),
/// nothing given twice (a step answers many times a second; the tangents
/// and the lengths along the chain follow from the points).
pub(super) fn state_json(state: &HangState) -> Value {
    let mut along = vec![0.0];
    for w in state.points.windows(2) {
        along.push(along.last().copied().unwrap_or(0.0) + mitcad_vec3::distance(w[0], w[1]));
    }
    let normals: Vec<[f64; 3]> = (0..state.points.len())
        .map(|i| state.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]))
        .collect();
    let nodes: Vec<Value> = state
        .nodes
        .iter()
        .map(|n| {
            json!({
                "id": n.id,
                "index": n.index,
                "point": state.points.get(n.index),
                "along": along.get(n.index),
                "pinned": n.pinned,
                "support": n.support,
                "moved": n.moved,
            })
        })
        .collect();
    json!({
        "flex": state.flex,
        "polyline": state.points,
        "normals": normals,
        "touching": state.touching,
        "nodes": nodes,
        "length": state.length,
        "segment": state.segment,
        "grabbed": state.grabbed.map(|i| along.get(i).copied().unwrap_or(0.0)),
        "at_rest": state.at_rest,
        "time": state.time,
        "taut": state.taut,
        "damping": state.damping,
    })
}

/// The `flex_hang` query: the hanging item, null when none hangs.
pub(super) fn query_json<K: Kernel>(doc: &Document<K>) -> Value {
    doc.flex_hang_state()
        .map_or(Value::Null, |state| state_json(&state))
}
