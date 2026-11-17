// SPDX-License-Identifier: MIT
//! A Flex item let hang (mitcad#1713; `commands.md`, "Hanging cables")
//! through the JSON api with the mock kernel, whose chain is a stand-in
//! (`MockKernel::flex_drape_step`: free points fall 100 mm a second and
//! stop on the top of a box below them; `flex_drape_nodes`: every fourth
//! point and the ends). Refused without gravity; both ends and a node
//! clipped onto a face pinned by default and held, a node placed freely
//! let go; a pin let go and another node pinned; a point grabbed
//! and dragged, then let go; the length set; locked as the item's nodes in
//! one undo step, the middle ones on the plate's top face; put back; a new
//! direction of gravity settles it again, a cleared one lets it go, and any
//! other change of the design ends it; a body beyond its reach from its
//! pinned ends is no obstacle of its steps.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, value: Value) -> String {
    doc.command(&value.to_string()).unwrap_err().to_string()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

/// A plate 200 × 200 × 5 (F1, its top at z 5) and a hose 10 thick (bend
/// radius 30) 100 long at 50 above it (F2, nodes n1 and n2).
fn plate_and_hose() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box",
               "plane": {"origin": [0, 0, 0], "normal": [0, 0, 1]}, "corner": [-50, -100],
               "length": 200, "width": 200, "height": 5, "operation": "new_body"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex",
               "item": {"name": "Hose 10", "section": {"type": "round", "diameter": 10},
                        "bend_radius": 30},
               "nodes": [{"id": "n1", "at": [0, 0, 50]}, {"id": "n2", "at": [100, 0, 50]}]}}),
    );
    doc
}

/// How far along the chain its point `index` is.
fn along(state: &Value, index: usize) -> f64 {
    let points: Vec<[f64; 3]> = serde_json::from_value(state["polyline"].clone()).unwrap();
    points[..=index]
        .windows(2)
        .map(|w| {
            (0..3)
                .map(|i| (w[1][i] - w[0][i]).powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .sum()
}

fn lowest(state: &Value) -> f64 {
    state["polyline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p[2].as_f64().unwrap())
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn a_flex_item_hangs_onto_a_plate_from_its_ends() {
    let mut doc = plate_and_hose();
    // Without a direction of gravity nothing hangs.
    let error = refused(&mut doc, json!({"cmd": "flex.hang", "flex": "F2"}));
    assert!(error.contains("direction of gravity"), "{error}");
    command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, 0, -1]}),
    );
    let hung = command(&mut doc, json!({"cmd": "flex.hang", "flex": "F2"}));
    // Its length kept, in segments of the hose's thickness, both ends
    // pinned by default.
    assert_eq!(hung["length"], json!(100.0), "{hung}");
    assert_eq!(hung["segment"], json!(10.0), "{hung}");
    assert_eq!(hung["polyline"].as_array().map(Vec::len), Some(11));
    assert_eq!(hung["nodes"][0]["pinned"], json!(true), "{hung}");
    assert_eq!(hung["nodes"][1]["pinned"], json!(true), "{hung}");
    assert_eq!(hung["nodes"][1]["index"], json!(10), "{hung}");
    assert_eq!(hung["at_rest"], json!(false));
    assert_eq!(
        query(&doc, json!({"query": "gravity"}))["hanging"],
        json!("F2")
    );
    assert_eq!(query(&doc, json!({"query": "flex_hang"}))["flex"], "F2");
    // It falls: the middle onto the plate (its top and the hose's radius),
    // the ends where they were.
    let step = command(&mut doc, json!({"cmd": "flex.hang_step", "seconds": 2}));
    assert!((lowest(&step) - 10.0).abs() < 1e-9, "{step}");
    assert_eq!(step["polyline"][0], json!([0.0, 0.0, 50.0]));
    assert_eq!(step["polyline"][10], json!([100.0, 0.0, 50.0]));
    assert_eq!(step["touching"][5], json!(true), "{step}");
    let specs = doc.kernel().specs.borrow().join("\n");
    assert!(
        specs.contains("flex_drape_step points 11 pinned [0,10] grab None obstacles 1"),
        "{specs}"
    );
    // Still: it rests, and a step then moves nothing.
    let step = command(&mut doc, json!({"cmd": "flex.hang_step", "seconds": 1}));
    assert_eq!(step["at_rest"], json!(true), "{step}");
    let calls = doc.kernel().specs.borrow().len();
    let still = command(&mut doc, json!({"cmd": "flex.hang_step"}));
    assert_eq!(still["time"], json!(0.0));
    assert_eq!(doc.kernel().specs.borrow().len(), calls);

    // Locked: the ends as they were, the points between on the plate's top
    // face, one undo step.
    let locked = command(&mut doc, json!({"cmd": "flex.hang_lock"}));
    assert_eq!(locked["nodes"], json!(["n1", "n3", "n4", "n2"]), "{locked}");
    assert_eq!(locked["deviation"], json!(0.0));
    assert_eq!(query(&doc, json!({"query": "flex_hang"})), Value::Null);
    let item = query(&doc, json!({"query": "flex", "uid": "F2"}));
    assert_eq!(item["nodes"][0]["support"], "free", "{item}");
    assert_eq!(item["nodes"][1]["support"], "face", "{item}");
    assert_eq!(item["nodes"][1]["ref"]["body"], "F1.b0", "{item}");
    assert_eq!(item["nodes"][1]["ref"]["face"], "F1:top", "{item}");
    assert_eq!(item["nodes"][3]["point"], json!([100.0, 0.0, 50.0]));
    let specs = doc.kernel().specs.borrow().join("\n");
    assert!(
        specs.contains("flex_drape_nodes points 11 keep [0, 10] ports false false tolerance 2.5"),
        "{specs}"
    );
    doc.undo().unwrap();
    let item = query(&doc, json!({"query": "flex", "uid": "F2"}));
    assert_eq!(item["nodes"].as_array().map(Vec::len), Some(2), "{item}");
}

#[test]
fn pins_are_let_go_and_set_and_a_point_is_dragged() {
    let mut doc = plate_and_hose();
    command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, 0, -1]}),
    );
    command(
        &mut doc,
        json!({"cmd": "flex.hang", "flex": "F2", "pins": ["n2"]}),
    );
    // One end free: it falls too.
    let step = command(&mut doc, json!({"cmd": "flex.hang_step", "seconds": 2}));
    assert!((step["polyline"][0][2].as_f64().unwrap() - 10.0).abs() < 1e-9);
    assert_eq!(step["nodes"][0]["pinned"], json!(false));
    // Pins set again: none of an unknown node.
    let error = refused(&mut doc, json!({"cmd": "flex.hang_set", "pins": ["n7"]}));
    assert!(error.contains("no such node"), "{error}");
    let set = command(
        &mut doc,
        json!({"cmd": "flex.hang_set", "pins": ["n1", "n2"]}),
    );
    assert_eq!(set["nodes"][0]["pinned"], json!(true));
    // Grabbed near its middle and pulled up: that point follows; let go,
    // it is no longer held.
    let dragged = command(
        &mut doc,
        json!({"cmd": "flex.hang_step", "grab": {"point": [51, 0, 10], "target": [50, 0, 80]}}),
    );
    assert_eq!(
        dragged["polyline"][5],
        json!([50.0, 0.0, 80.0]),
        "{dragged}"
    );
    assert!((dragged["grabbed"].as_f64().unwrap() - along(&dragged, 5)).abs() < 1e-9);
    // The step's answer gives each point and its section's y once, and
    // nothing of a design change (it ran none of its checks).
    assert_eq!(
        dragged["normals"].as_array().map(Vec::len),
        dragged["polyline"].as_array().map(Vec::len)
    );
    assert!(dragged.get("frames").is_none() && dragged.get("recomputed").is_none());
    let again = command(
        &mut doc,
        json!({"cmd": "flex.hang_step", "grab": {"point": [0, 0, 0], "target": [50, 0, 90]}}),
    );
    assert_eq!(
        again["polyline"][5],
        json!([50.0, 0.0, 90.0]),
        "the same point"
    );
    let released = command(&mut doc, json!({"cmd": "flex.hang_step"}));
    assert_eq!(released["grabbed"], Value::Null);
    // Its length set: longer, in more segments.
    let longer = command(&mut doc, json!({"cmd": "flex.hang_set", "length": 150}));
    assert_eq!(longer["length"], json!(150.0));
    assert_eq!(longer["polyline"].as_array().map(Vec::len), Some(16));
    assert_eq!(longer["nodes"][1]["index"], json!(15));
    let error = refused(&mut doc, json!({"cmd": "flex.hang_set", "length": 0}));
    assert!(error.contains("greater than zero"), "{error}");
    // Put back: nothing of the design changed.
    assert_eq!(
        command(&mut doc, json!({"cmd": "flex.hang_cancel"}))["cancelled"],
        json!(true)
    );
    assert_eq!(
        command(&mut doc, json!({"cmd": "flex.hang_cancel"}))["cancelled"],
        json!(false)
    );
    let error = refused(&mut doc, json!({"cmd": "flex.hang_step"}));
    assert!(error.contains("no flex item hangs"), "{error}");
}

#[test]
fn a_node_attached_to_something_stays_pinned_and_a_free_one_is_let_go() {
    // A hose from n1 to n4 with two nodes between: n2 placed freely in
    // space, n3 clipped onto the plate's top face. Let hang without pins,
    // the ends and the clipped node stay where they are (owner 15.11.2026:
    // attached nodes act as clips); only the free node falls.
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box",
               "plane": {"origin": [0, 0, 0], "normal": [0, 0, 1]}, "corner": [-50, -100],
               "length": 200, "width": 200, "height": 5, "operation": "new_body"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex",
               "item": {"name": "Hose 10", "section": {"type": "round", "diameter": 10},
                        "bend_radius": 30},
               "nodes": [{"id": "n1", "at": [0, 0, 50]}, {"id": "n2", "at": [40, 0, 50]},
                         {"id": "n3", "at": [80, 0, 5],
                          "on": {"body": "F1.b0", "face": "F1:top"}},
                         {"id": "n4", "at": [120, 0, 50]}]}}),
    );
    command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, 0, -1]}),
    );
    let hung = command(&mut doc, json!({"cmd": "flex.hang", "flex": "F2"}));
    let node = |id: &str| {
        hung["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .cloned()
            .unwrap()
    };
    assert_eq!(node("n1")["pinned"], json!(true), "{hung}");
    assert_eq!(node("n2")["pinned"], json!(false), "{hung}");
    assert_eq!(node("n2")["support"], "free", "{hung}");
    assert_eq!(node("n3")["pinned"], json!(true), "{hung}");
    assert_eq!(node("n3")["support"], "face", "{hung}");
    assert_eq!(node("n4")["pinned"], json!(true), "{hung}");
    let index = |id: &str| node(id)["index"].as_u64().unwrap() as usize;
    let (free, clipped, last) = (index("n2"), index("n3"), index("n4"));
    assert!(0 < free && free < clipped && clipped < last, "{hung}");
    // The kernel holds the ends and the clipped node's point.
    command(&mut doc, json!({"cmd": "flex.hang_step", "seconds": 2}));
    let specs = doc.kernel().specs.borrow().join("\n");
    assert!(
        specs.contains(&format!(
            "flex_drape_step points {} pinned [0,{clipped},{last}] grab None",
            last + 1
        )),
        "{specs}"
    );
    // The clipped node stays on the plate where it was; the free one falls
    // onto it (its top and the hose's radius).
    let step = command(&mut doc, json!({"cmd": "flex.hang_step", "seconds": 1}));
    assert_eq!(
        step["polyline"][clipped], hung["polyline"][clipped],
        "{step}"
    );
    assert_eq!(step["polyline"][clipped], json!([80.0, 0.0, 5.0]), "{step}");
    assert_eq!(step["polyline"][0], json!([0.0, 0.0, 50.0]));
    assert_eq!(step["polyline"][last], json!([120.0, 0.0, 50.0]));
    assert_eq!(hung["polyline"][free][2], json!(50.0), "{hung}");
    assert!(
        (step["polyline"][free][2].as_f64().unwrap() - 10.0).abs() < 1e-9,
        "{step}"
    );
    // Pins given replace the default: the clipped node is let go too.
    let ends = command(
        &mut doc,
        json!({"cmd": "flex.hang", "flex": "F2", "pins": ["n1", "n4"]}),
    );
    let pinned: Vec<bool> = ends["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["pinned"].as_bool().unwrap())
        .collect();
    assert_eq!(pinned, [true, false, false, true], "{ends}");
}

#[test]
fn a_new_direction_of_gravity_settles_it_again_and_other_changes_end_it() {
    let mut doc = plate_and_hose();
    command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, 0, -1]}),
    );
    command(&mut doc, json!({"cmd": "flex.hang", "flex": "F2"}));
    command(&mut doc, json!({"cmd": "flex.hang_step", "seconds": 2}));
    let rest = command(&mut doc, json!({"cmd": "flex.hang_step", "seconds": 1}));
    assert_eq!(rest["at_rest"], json!(true), "{rest}");
    // A new direction while it hangs (the Let It Hang panel's Direction
    // From): it goes on hanging, awake, and falls the new way from where it
    // rested, its ends held; the direction is an undo step of its own.
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, -1, 0]}),
    );
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(
        query(&doc, json!({"query": "gravity"}))["hanging"],
        json!("F2")
    );
    let hanging = query(&doc, json!({"query": "flex_hang"}));
    assert_eq!(hanging["at_rest"], json!(false), "{hanging}");
    let step = command(&mut doc, json!({"cmd": "flex.hang_step", "seconds": 0.5}));
    assert_eq!(step["polyline"][5], json!([50.0, -50.0, 10.0]), "{step}");
    assert_eq!(step["polyline"][0], json!([0.0, 0.0, 50.0]));
    // Cleared, no direction is left: it is let go.
    command(&mut doc, json!({"cmd": "gravity.clear"}));
    assert_eq!(query(&doc, json!({"query": "flex_hang"})), Value::Null);
    assert_eq!(
        query(&doc, json!({"query": "gravity"}))["hanging"],
        Value::Null
    );
    // Any other change of the design ends it (an undo here).
    command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, 0, -1]}),
    );
    command(&mut doc, json!({"cmd": "flex.hang", "flex": "F2"}));
    doc.undo().unwrap();
    let error = refused(&mut doc, json!({"cmd": "flex.hang_step"}));
    assert!(error.contains("the design changed"), "{error}");
    assert_eq!(query(&doc, json!({"query": "flex_hang"})), Value::Null);
    // Only a Flex item hangs (the undo above took the direction back too, so
    // it is given again: without one the refusal asks for it first).
    command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, 0, -1]}),
    );
    let error = refused(&mut doc, json!({"cmd": "flex.hang", "flex": "F1"}));
    assert!(error.contains("not a flex item"), "{error}");
}

#[test]
fn a_hanging_item_takes_a_damping() {
    let mut doc = plate_and_hose();
    command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, 0, -1]}),
    );
    // The default, as the kernel gets it.
    let hung = command(&mut doc, json!({"cmd": "flex.hang", "flex": "F2"}));
    assert_eq!(hung["damping"], json!(0.5), "{hung}");
    command(&mut doc, json!({"cmd": "flex.hang_step"}));
    let specs = doc.kernel().specs.borrow().join("\n");
    assert!(specs.contains(" damping 0.5"), "{specs}");
    // Given when it is let hang, and changed while it hangs.
    let hung = command(
        &mut doc,
        json!({"cmd": "flex.hang", "flex": "F2", "damping": 0.8}),
    );
    assert_eq!(hung["damping"], json!(0.8), "{hung}");
    let set = command(&mut doc, json!({"cmd": "flex.hang_set", "damping": 0.2}));
    assert_eq!(set["damping"], json!(0.2), "{set}");
    command(&mut doc, json!({"cmd": "flex.hang_step"}));
    let specs = doc.kernel().specs.borrow().join("\n");
    assert!(specs.contains(" damping 0.2"), "{specs}");
    // From 0 to 1 only.
    let error = refused(&mut doc, json!({"cmd": "flex.hang_set", "damping": 1.5}));
    assert!(error.contains("from 0 to 1"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.hang", "flex": "F2", "damping": -0.1}),
    );
    assert!(error.contains("from 0 to 1"), "{error}");
    let error = refused(&mut doc, json!({"cmd": "flex.hang_set"}));
    assert!(error.contains("a damping"), "{error}");
}

#[test]
fn a_lock_whose_item_cannot_be_made_is_refused_and_it_still_hangs() {
    // Keep Shape left only a small part of an item (mitcad#1713): a lock
    // whose item is not made whole along the nodes it would keep is refused
    // in words, the design as it was and the item still hanging, to be
    // moved and kept again.
    let mut doc = plate_and_hose();
    command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, 0, -1]}),
    );
    command(&mut doc, json!({"cmd": "flex.hang", "flex": "F2"}));
    command(&mut doc, json!({"cmd": "flex.hang_step", "seconds": 2}));
    doc.kernel().fail.borrow_mut().insert("flex_solid");
    let error = refused(&mut doc, json!({"cmd": "flex.hang_lock"}));
    assert!(error.contains("could not be kept"), "{error}");
    assert_eq!(query(&doc, json!({"query": "flex_hang"}))["flex"], "F2");
    let item = query(&doc, json!({"query": "flex", "uid": "F2"}));
    assert_eq!(item["nodes"].as_array().map(Vec::len), Some(2), "{item}");
    // Made again: kept. (The refused lock's evaluation stays cached, and the
    // same nodes would fail the same way with a real kernel; the mock's
    // failure gone, its cached results go too.)
    doc.kernel().fail.borrow_mut().remove("flex_solid");
    doc.clear_memory_cache();
    let locked = command(&mut doc, json!({"cmd": "flex.hang_lock"}));
    assert_eq!(locked["nodes"], json!(["n1", "n3", "n4", "n2"]), "{locked}");
    assert_eq!(query(&doc, json!({"query": "flex_hang"})), Value::Null);
}

#[test]
fn a_step_gives_the_kernel_only_the_bodies_the_item_can_reach() {
    // A block far beyond the hose's reach from its pinned ends (its length
    // and its section): the steps leave it out while an end is pinned, and
    // give it again once nothing holds the hose. Its runtime commands make
    // no undo step.
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box",
               "plane": {"origin": [0, 0, 0], "normal": [0, 0, 1]}, "corner": [-50, -100],
               "length": 200, "width": 200, "height": 5, "operation": "new_body"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box",
               "plane": {"origin": [0, 0, 0], "normal": [0, 0, 1]}, "corner": [1000, 1000],
               "length": 50, "width": 50, "height": 50, "operation": "new_body"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex",
               "item": {"name": "Hose 10", "section": {"type": "round", "diameter": 10},
                        "bend_radius": 30},
               "nodes": [{"id": "n1", "at": [0, 0, 50]}, {"id": "n2", "at": [100, 0, 50]}]}}),
    );
    command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, 0, -1]}),
    );
    command(&mut doc, json!({"cmd": "flex.hang", "flex": "F3"}));
    let depth = doc.undo_depth();
    command(&mut doc, json!({"cmd": "flex.hang_step", "seconds": 0.1}));
    let specs = doc.kernel().specs.borrow().join("\n");
    assert!(
        specs.contains("flex_drape_step points 11 pinned [0,10] grab None obstacles 1"),
        "{specs}"
    );
    command(&mut doc, json!({"cmd": "flex.hang_set", "pins": []}));
    command(&mut doc, json!({"cmd": "flex.hang_step", "seconds": 0.1}));
    let specs = doc.kernel().specs.borrow().join("\n");
    assert!(
        specs.contains("flex_drape_step points 11 pinned [] grab None obstacles 2"),
        "{specs}"
    );
    assert_eq!(doc.undo_depth(), depth);
}
