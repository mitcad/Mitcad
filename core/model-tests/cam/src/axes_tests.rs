// SPDX-License-Identifier: MIT
//! The mill-turn's Y axis, B head, sub-spindle, 3D stock and sketches
//! wrapped onto the diameter through the JSON api (mitcad#2319) on the
//! mock kernel: the block 40 x 30 x 10 turned about the design's Z through
//! its middle (live_tests.rs) on a lathe with a Y axis, a B head and a
//! sub-spindle: a flat faced with the Y axis, a hole drilled on a plane
//! tilted by the B head, a sketch on the XZ plane wrapped onto the
//! diameter, the part parted off into the sub-spindle and its back faced
//! there. The program read back, the stock cut in 3D, what does not fit
//! refused in words.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The region of a sketch's first rectangle.
const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was taken: {result}"),
        Err(e) => e.to_string(),
    }
}

/// A lathe with a C axis and live tools, and as asked a Y axis, a B head
/// and a sub-spindle: a turning tool and a parting blade.
fn lathe(y: bool, b: bool, sub: bool) -> Value {
    json!({
        "id": "mill-turn",
        "machine": {
            "name": "Mill-turn",
            "controller": "fanuc_turning",
            "travel": {"min": [-60, -40, -300], "max": [150, 40, 200]},
            "spindle": {"min_rpm": 50, "max_rpm": 3000, "nose_diameter": 0, "nose_length": 0},
            "max_feed": 5000,
            "rapid_feed": 20000,
            "lathe": {"turret_stations": 12, "chuck_diameter": 160, "chuck_length": 50,
                      "axes": {"c": true, "live_tools": true, "y": y, "b": b, "sub_spindle": sub}}
        },
        "tools": {"tools": [
            {"number": 1, "name": "Turning", "shape": {"kind": "turning", "nose_radius": 0.4},
             "diameter": 20, "flute_length": 3, "length": 30},
            {"number": 2, "name": "Parting", "shape": {"kind": "parting", "width": 3},
             "diameter": 20, "flute_length": 25, "length": 30}
        ]}
    })
}

fn tool(number: u32, shape: &str, diameter: f64, mount: &str) -> Value {
    json!({"cmd": "cam_tool_put", "machine": "mill-turn", "tool": {
        "number": number, "shape": {"kind": shape}, "diameter": diameter, "mount": mount,
        "cutting": {"spindle_rpm": 2000, "feed": 200, "plunge_feed": 100}}})
}

/// The block on a lathe with every axis, a sketch on the XZ plane (15 mm
/// from the spindle axis, the block's front), the setup handing over to
/// the sub-spindle.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
            {"cmd": "sketch.create", "plane": "xz"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [18, -6], "width": 4, "height": 2},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": lathe(true, true, true)}),
    );
    command(&mut doc, tool(8, "flat", 6.0, "radial"));
    command(&mut doc, tool(9, "drill", 4.0, "head"));
    command(&mut doc, tool(10, "flat", 1.0, "radial"));
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {
            "stock": {"kind": "around_bodies", "side": 2, "top": 1, "bottom": 5},
            "post": {"canned_cycles": true},
            "lathe": {"hand_over": {"withdraw": "40 mm", "rpm": 600}}
        }}),
    );
    for operation in [
        json!({"kind": {"type": "turn_face"}, "tool": 1}),
        // The block's right side faced 1 mm deep with the Y axis.
        json!({"kind": {"type": "facing"}, "tool": 8, "live": "plane",
               "plane": {"c": 0, "b": 90}, "depths": {"bottom": 19}}),
        // A hole at the block's top right edge, square to a plane tilted
        // 45 degrees.
        json!({"kind": {"type": "drill"}, "tool": 9, "live": "plane",
               "plane": {"c": 0, "b": 45},
               "inputs": [{"kind": "point", "at": {"point": [40, 15, 10]}}],
               "depths": {"bottom": 10}}),
        // The XZ sketch's rectangle wrapped onto the diameter, engraved
        // down to a 29 mm diameter.
        json!({"kind": {"type": "contour", "side": "on"}, "tool": 10, "live": "diameter",
               "inputs": [{"kind": "region", "sketch": "F3", "region": R}],
               "depths": {"bottom": 29}}),
        json!({"kind": {"type": "turn_part_off"}, "tool": 2}),
        json!({"kind": {"type": "turn_face"}, "tool": 1, "sub_spindle": true}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    doc
}

#[test]
fn a_mill_turn_mills_with_y_and_b_wraps_a_sketch_and_hands_over() {
    let mut doc = block();
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    for op in generated["setups"][0]["operations"].as_array().unwrap() {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
    }
    assert_eq!(generated["setups"][0]["findings"], json!([]), "{generated}");

    let posted = query(&doc, json!({"query": "cam_post", "setup": 1}));
    let text = posted["text"].as_str().unwrap();
    let lines: Vec<&str> = text.lines().collect();
    // The flat: the C axis clamped at zero, the YZ plane.
    for line in ["G0 C0.", "M14", "G19", "G18"] {
        assert!(lines.contains(&line), "no {line}:\n{text}");
    }
    // The tilted hole: the tilted working plane, the head turned to it.
    for line in ["G68.2 X0. Y0. Z0. I90. J45. K-90.", "G53.1", "M107", "G69"] {
        assert!(lines.contains(&line), "no {line}:\n{text}");
    }
    assert!(lines.iter().any(|l| l.starts_with("G83 X")), "{text}");
    // The wrapped sketch: cylindrical interpolation on the 15 mm cylinder
    // it wraps on.
    assert!(lines.contains(&"G7.1 C15."), "{text}");
    // The hand-over: synchronised at 600 rpm, gripped, parted off, pulled
    // 40 mm back, the back faced on the sub-spindle in G55.
    let at = |line: &str| {
        lines
            .iter()
            .position(|l| *l == line)
            .unwrap_or_else(|| panic!("no {line}:\n{text}"))
    };
    assert!(at("G97 S600 M3") < at("M213"));
    assert!(at("M213") < at("M78"));
    assert!(at("M78") < at("(PARTING OFF 1)"));
    assert!(at("(PARTING OFF 1)") < at("M212"));
    assert!(at("M212") < at("G55"));
    assert!(at("G55") < at("(TURN FACING 2)"));
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("G96 S") && l.ends_with(" M203")),
        "{text}"
    );
    assert!(!lines.contains(&"M69"), "the part was parted off:\n{text}");

    // The toolpaths: the sub-spindle's on the part where it was, its Z
    // turned round at the part's back (the block's bottom at work Z -10).
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let back = paths["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["sub_spindle"] == json!(true))
        .expect("the sub-spindle's facing");
    let lowest = back["bounds"][0][2].as_f64().unwrap();
    assert!(lowest < -9.0, "{back}");

    // The stock in 3D: a mesh, the material the live tools took, the
    // part not gouged beyond the cubes along its surface.
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let solid = &simulated["solid"];
    assert!(solid["milled"].as_f64().unwrap() > 0.0, "{solid}");
    assert!(solid["volume"].as_f64().unwrap() < solid["initial"].as_f64().unwrap());
    let triangles = solid["mesh"]["triangles"].as_array().unwrap();
    assert!(!triangles.is_empty() && triangles.len().is_multiple_of(3));
    assert!(
        solid["mesh"]["vertices"]
            .as_array()
            .unwrap()
            .len()
            .is_multiple_of(3),
        "{solid}"
    );
    // The live tools' maps: the flat's and the tilted plane's own.
    let sides: Vec<String> = simulated["live"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["side"]["kind"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(sides, ["plane", "plane", "diameter"]);
}

#[test]
fn what_the_lathe_lacks_is_refused_in_words() {
    let mut doc = block();
    // A lathe without a Y axis, a B head or a sub-spindle.
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": lathe(false, false, false)}),
    );
    command(&mut doc, tool(8, "flat", 6.0, "radial"));
    let plane = refused(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
            "kind": {"type": "facing"}, "tool": 8, "live": "plane",
            "plane": {"c": 0, "b": 90}, "depths": {"bottom": 19}}}),
    );
    assert!(
        plane.contains(
            "A live tool cuts on a plane with the Y axis, which the lathe does not have."
        ),
        "{plane}"
    );
    let sub = refused(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
            "kind": {"type": "turn_face"}, "tool": 1, "sub_spindle": true}}),
    );
    assert!(
        sub.contains("The lathe has no sub-spindle to turn the part's back."),
        "{sub}"
    );
    // Live tools cut on the main spindle only.
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": lathe(true, true, true)}),
    );
    command(&mut doc, tool(8, "flat", 6.0, "radial"));
    let live_sub = refused(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
            "kind": {"type": "facing"}, "tool": 8, "live": "plane", "sub_spindle": true,
            "plane": {"c": 0, "b": 90}}}),
    );
    assert!(
        live_sub.contains("Only turning operations turn the part's back on the sub-spindle"),
        "{live_sub}"
    );
    // A wrapped sketch needs its bottom.
    command(&mut doc, tool(10, "flat", 1.0, "radial"));
    let added = command(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
            "kind": {"type": "contour", "side": "on"}, "tool": 10, "live": "diameter",
            "inputs": [{"kind": "region", "sketch": "F3", "region": R}]}}),
    );
    let id = added["operation"].as_u64().unwrap();
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let op = paths["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"].as_u64() == Some(id))
        .unwrap();
    assert_eq!(
        op["error"],
        json!("Set the bottom diameter of what is wrapped onto the diameter."),
        "{op}"
    );
}
