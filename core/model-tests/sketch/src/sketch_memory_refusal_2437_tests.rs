// SPDX-License-Identifier: MIT
//! Sketch Dimension and the memory guard of model commands (mitcad#2437):
//! a driving radius on each arc of a free two-arc profile, as the
//! application sends it, refused before it started at 13.9 GiB of 16 GiB
//! (the reported refusal; the memory was the view's, not the command's).
//! The refused dimension is not added: the sketch, the document and its
//! undo steps stay as they were, and the error says that the memory was in
//! use before the command and how to free it, not to join bodies. With
//! enough memory both arcs take their radius.

use std::sync::Arc;

use serde_json::{Value, json};

use mitcad_model::monitor::memory_guard::{self, GuardedWork, MemoryReading};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, RecomputeMonitor};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn made(result: &Value) -> String {
    result["made"][0].as_str().unwrap().to_owned()
}

/// A field of an entity of the sketch (an arc's `start`).
fn field(s: &Value, id: &str, key: &str) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))[key]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A new document with Sketch1 (F1) and a free two-arc profile: a lens of
/// two three-point arcs between (5, -4) and (5, 4), through (10, 0) and
/// (0, 0), without dimensions.
fn two_arcs() -> (Document<MockKernel>, [String; 2]) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let right = made(&command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": [5, -4], "through": [10, 0], "end": [5, 4]}),
    ));
    let s = sketch(&d);
    let (bottom, top) = (field(&s, &right, "start"), field(&s, &right, "end"));
    let left = made(&command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": top, "through": [0, 0], "end": bottom}),
    ));
    (d, [right, left])
}

/// The memory as the guard reads it on a machine of 16 GiB: `share` of it
/// in use.
fn memory(share: f64) -> impl FnMut() -> MemoryReading + Send + 'static {
    move || MemoryReading {
        share,
        text: format!("{:.1} GiB of 16.0 GiB (physical memory)", share * 16.0),
        failed_allocations: 0,
    }
}

/// What a refused command must leave as it was.
#[derive(Debug, PartialEq)]
struct State {
    json: String,
    revision: u64,
    undo_depth: usize,
    sketch: Value,
}

fn state(doc: &Document<MockKernel>) -> State {
    State {
        json: doc.to_json(),
        revision: doc.revision(),
        undo_depth: doc.undo_depth(),
        sketch: sketch(doc),
    }
}

/// `sketch.add_dimension` of a driving radius on `arc` under the memory
/// guard, with the memory at `share`, as the application's bridge runs it.
fn radius(d: &mut Document<MockKernel>, arc: &str, share: f64) -> Result<String, String> {
    let monitor = Arc::new(RecomputeMonitor::new());
    d.set_monitor(Some(Arc::clone(&monitor)));
    let cmd = json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                     "dimension": {"type": "radius", "curve": arc}, "value": 5,
                     "text": [12, 6]});
    let result = memory_guard::guarded_command(
        &monitor,
        GuardedWork::Sketch,
        memory(share),
        |reason| reason,
        || d.command(&cmd.to_string()).map_err(|e| e.to_string()),
    );
    d.set_monitor(None);
    result
}

#[test]
fn a_radius_refused_for_memory_leaves_the_sketch_as_it_was() {
    let (mut d, arcs) = two_arcs();
    let before = state(&d);
    for arc in &arcs {
        let error = radius(&mut d, arc, 0.87).expect_err("refused for memory");
        assert!(
            error.starts_with(
                "the operation ran out of memory at 13.9 GiB of 16.0 GiB (physical memory) \
                 before it started and was not run; the document is as it was before the \
                 command"
            ),
            "{error}"
        );
        assert!(
            error.contains("save your work and restart MitCad, then try again"),
            "{error}"
        );
        assert!(!error.contains("join the bodies"), "{error}");
        assert_eq!(
            state(&d),
            before,
            "the refused radius on {arc} changed the design"
        );
    }
    assert!(
        sketch(&d)["dimensions"]
            .as_array()
            .is_none_or(|dimensions| dimensions.is_empty()),
        "{}",
        sketch(&d)
    );

    // With enough memory, each arc takes its driving radius.
    for arc in &arcs {
        let result = radius(&mut d, arc, 0.3).unwrap_or_else(|e| panic!("radius on {arc}: {e}"));
        let result: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            result["dimensions"].as_array().map(Vec::len),
            Some(1),
            "{result}"
        );
    }
    let s = sketch(&d);
    let dimensions = s["dimensions"].as_array().expect("dimensions");
    assert_eq!(dimensions.len(), 2, "{s}");
    for dimension in dimensions {
        assert!(
            (dimension["measured"].as_f64().unwrap() - 5.0).abs() <= 1e-6,
            "{dimension}"
        );
    }
}
