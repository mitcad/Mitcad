// SPDX-License-Identifier: MIT
//! Recompute All (mitcad#1982): `{"cmd": "recompute", "all": true}`
//! evaluates every feature again, without the cached results, makes no
//! undo step when nothing it measured changed, and reports a feature that
//! fails as every command does.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const EDGE: &str = "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

/// A 60 x 40 block (F1, F2) with a fillet (F3).
fn filleted_block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": REGION}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
               "edges": [EDGE], "radius": 5}}),
    );
    doc
}

#[test]
fn recompute_all_evaluates_every_feature_again() {
    let mut doc = filleted_block();
    // A plain recompute takes every result from the cache.
    let plain = command(&mut doc, json!({"cmd": "recompute"}));
    assert_eq!(plain["recomputed"], 0, "{plain}");
    assert!(plain.get("changed").is_none(), "{plain}");
    let depth = doc.undo_depth();
    let label = doc.undo_label().map(str::to_owned);

    let all = command(&mut doc, json!({"cmd": "recompute", "all": true}));
    assert_eq!(all["recomputed"], 3, "{all}");
    assert!(all["error"].is_null(), "{all}");
    // Nothing it measured changed: no undo step.
    assert_eq!(all["changed"], false, "{all}");
    assert_eq!(doc.undo_depth(), depth);
    assert_eq!(doc.undo_label().map(str::to_owned), label);

    // Its results are the cache's now: a plain recompute evaluates nothing.
    let after = command(&mut doc, json!({"cmd": "recompute"}));
    assert_eq!(after["recomputed"], 0, "{after}");
}

#[test]
fn recompute_all_reports_a_feature_the_cache_hid() {
    let mut doc = filleted_block();
    // The kernel fails fillets from now on; the cached fillet stays good
    // through a plain recompute.
    doc.kernel().fail.borrow_mut().insert("fillet");
    let plain = command(&mut doc, json!({"cmd": "recompute"}));
    assert!(plain["error"].is_null(), "{plain}");

    // Evaluated again, it fails, and the result says so.
    let all = command(&mut doc, json!({"cmd": "recompute", "all": true}));
    assert!(
        all["error"]
            .as_str()
            .is_some_and(|e| e.contains("fillet failed")),
        "{all}"
    );
    assert!(all["error_notice"].is_object(), "{all}");
    assert_eq!(all["failed_features"][0]["uid"], "F3", "{all}");
}
