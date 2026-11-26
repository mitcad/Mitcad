// SPDX-License-Identifier: MIT
//! Guided tutorials over live MCP (mitcad#2012, docs/mcp.md "Guided
//! tutorials"): `ui_guide` shows the user a guide step in the application,
//! as a tutorial page's `window.mitcad.startStep` does, and the resource
//! `mitcad://ui-guide` is the step's state with its last event (the step
//! done, left, a command refused, input held back), which a subscribed
//! client hears about instead of polling.

use serde_json::{Value, json};

/// The live resource of the guide step: whether one is shown, the step,
/// its sequence number and its last event.
pub const UI_GUIDE_URI: &str = "mitcad://ui-guide";

/// `resources/list`'s row of the guide resource.
pub(crate) fn resource() -> Value {
    json!({
        "uri": UI_GUIDE_URI,
        "name": "Guide step",
        "description": "The guide step ui_guide shows the user: active, holding (the input still held after a step with hold is done), the step, its sequence number (one per step started) and last_event ({\"event\": \"step_done\" with what was done, \"step_left\" with the reason, \"refused\" with the command, \"held\" with the input held back, \"mismatch\" with the sketch input a step held back, or \"demonstration_ended\" with the reason}). Subscribe to hear when the user completes or leaves it.",
        "mimeType": "application/json"
    })
}

/// The state before the application gave one: no step shown yet.
pub(crate) fn idle() -> Value {
    json!({"active": false, "holding": false, "sequence": 0})
}

/// `resources/read`'s result for the guide step.
pub(crate) fn contents(state: &Value) -> Value {
    json!({"contents": [{
        "uri": UI_GUIDE_URI,
        "mimeType": "application/json",
        "text": state.to_string()
    }]})
}
