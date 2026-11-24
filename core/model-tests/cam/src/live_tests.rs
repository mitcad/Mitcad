// SPDX-License-Identifier: MIT
//! Live tools on a lathe with a C axis through the JSON api (mitcad#2067)
//! on the mock kernel: the block 40 x 30 x 10 turned about the design's Z
//! through its middle (turning_tests.rs), on a lathe whose turret also
//! holds live tools: a hole and a pocket on the face, a cross hole on the
//! diameter. The program with the C axis read back; the live tools' cuts
//! simulated; what does not fit refused in words.

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

/// A lathe with a C axis and live tools: a turning tool, an axial drill
/// and end mill, a radial drill.
fn mill_turn(live: bool) -> Value {
    json!({
        "id": "mill-turn",
        "machine": {
            "name": "Mill-turn",
            "controller": "fanuc_turning",
            "travel": {"min": [-5, 0, -300], "max": [150, 0, 200]},
            "spindle": {"min_rpm": 50, "max_rpm": 3000, "nose_diameter": 0, "nose_length": 0},
            "max_feed": 5000,
            "rapid_feed": 20000,
            "lathe": {"turret_stations": 12, "chuck_diameter": 160, "chuck_length": 50,
                      "axes": {"c": live, "live_tools": live}}
        },
        "tools": {"tools": [
            {"number": 1, "name": "Turning", "shape": {"kind": "turning", "nose_radius": 0.4},
             "diameter": 20, "flute_length": 3, "length": 30}
        ]}
    })
}

fn tool(number: u32, shape: &str, diameter: f64, mount: &str) -> Value {
    json!({"cmd": "cam_tool_put", "machine": "mill-turn", "tool": {
        "number": number, "shape": {"kind": shape}, "diameter": diameter, "mount": mount,
        "cutting": {"spindle_rpm": 2000, "feed": 200, "plunge_feed": 100}}})
}

/// The block on the mill-turn lathe: faced, then a hole 6 deep at 10 mm
/// from the axis and a pocket 1 deep on the face, a cross hole down to a
/// 30 mm diameter on the diameter.
fn milled() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": mill_turn(true)}),
    );
    command(&mut doc, tool(5, "drill", 5.0, "axial"));
    command(&mut doc, tool(6, "flat", 4.0, "axial"));
    command(&mut doc, tool(7, "drill", 3.0, "radial"));
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {
            "stock": {"kind": "around_bodies", "side": 2, "top": 1, "bottom": 5},
            "post": {"canned_cycles": true}
        }}),
    );
    for operation in [
        json!({"kind": {"type": "turn_face"}, "tool": 1}),
        json!({"kind": {"type": "drill"}, "tool": 5, "live": "face",
               "inputs": [{"kind": "point", "at": {"point": [30, 15, 10]}}],
               "depths": {"bottom": -6}}),
        json!({"kind": {"type": "pocket"}, "tool": 6,
               "inputs": [{"kind": "region", "sketch": "F1", "region": R}],
               "depths": {"top": 0, "bottom": -1}}),
        json!({"kind": {"type": "drill"}, "tool": 7, "live": "diameter",
               "inputs": [{"kind": "point", "at": {"point": [40, 15, 5]}}],
               "depths": {"bottom": 30}}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    doc
}

#[test]
fn a_mill_turn_setup_drills_and_mills_with_the_c_axis() {
    let mut doc = milled();
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    for op in generated["setups"][0]["operations"].as_array().unwrap() {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
    }

    let post = query(&doc, json!({"query": "cam_post", "setup": 1}));
    let text = post["text"].as_str().unwrap();
    // Turning, then the C axis on with feeds per minute for the live
    // tools, off again at the end.
    assert!(text.contains("T0101\nG50 S3000\nG96 S200 M3\n"), "{text}");
    assert!(
        text.contains("T0505\nM5\nM154\nG28 H0.\nG98\nM133 P2000\n"),
        "{text}"
    );
    // The hole at 10 mm from the axis as a face drilling cycle, 6 deep.
    assert!(text.contains("G83 X20. C0. Z-6. "), "{text}");
    // The pocket in polar interpolation.
    assert!(text.contains("T0606\n"), "{text}");
    assert!(
        text.contains("G17\nG12.1\n") && text.contains("G13.1\nG18\n"),
        "{text}"
    );
    // The cross hole at C0 and 5 mm behind the face, down to 30 mm.
    assert!(text.contains("G87 Z-5. C0. X30. "), "{text}");
    assert!(text.contains("M135\nM155\nG99\n"), "{text}");
    let kinds: Vec<&str> = post["cycles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c[0].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["G83", "G87"]);

    // The live tools' paths in the work coordinates, off the section's
    // plane.
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let ops = paths["operations"].as_array().unwrap();
    assert_eq!(ops[2]["live"]["side"], json!({"kind": "face"}));
    assert!(
        ops[2]["toolpath"]["moves"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["to"][1].as_f64().unwrap().abs() > 1.0),
        "{}",
        ops[2]
    );
    assert_eq!(ops[3]["live"]["side"]["kind"], json!("diameter"));
    assert_eq!(ops[3]["tool"]["diameter"], json!(3.0));

    // The live tools' cuts: the face's map and the diameter's, no rapid
    // into the stock.
    let sim = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let live = sim["live"].as_array().unwrap();
    assert_eq!(live.len(), 2, "{}", sim["live"]);
    assert_eq!(live[0]["side"], json!({"kind": "face"}));
    assert_eq!(live[0]["operations"], json!([3, 4]));
    assert!(live[0]["removed"].as_f64().unwrap() > 0.0);
    assert_eq!(live[1]["side"]["kind"], json!("diameter"));
    for side in live {
        assert!(side["rapid_cuts"].as_array().unwrap().is_empty(), "{side}");
    }
}

#[test]
fn live_tools_keep_to_their_places_in_words() {
    let mut doc = milled();
    let cases = [
        (
            json!({"cmd": "cam_operation_add", "setup": 1,
                   "operation": {"kind": {"type": "pocket"}, "tool": 7,
                                 "inputs": [{"kind": "region", "sketch": "F1", "region": R}]}}),
            "Tool 7 sits in a radial holder: it cuts the diameter, not the face.",
        ),
        (
            json!({"cmd": "cam_operation_add", "setup": 1,
                   "operation": {"kind": {"type": "facing"}, "tool": 7, "live": "diameter"}}),
            "A live tool faces the part's face: on the diameter, contour or pocket the faces round the spindle axis.",
        ),
        (
            json!({"cmd": "cam_operation_add", "setup": 1,
                   "operation": {"kind": {"type": "raster_finish"}, "tool": 6}}),
            "On a lathe, live tools face, contour, pocket, clear adaptively and drill.",
        ),
        (
            json!({"cmd": "cam_operation_add", "setup": 1,
                   "operation": {"kind": {"type": "turn_face"}, "tool": 6}}),
            "Tool 6 is a live tool: turning needs a lathe tool.",
        ),
        (
            json!({"cmd": "cam_operation_add", "setup": 1,
                   "operation": {"kind": {"type": "turn_face"}, "tool": 1, "live": "face"}}),
            "A turning operation turns: it has no live tool's place.",
        ),
        (
            tool(120, "flat", 6.0, "axial"),
            "Tool 120 is a live tool: its number is its turret station, 1 to 99.",
        ),
    ];
    for (cmd, message) in cases {
        assert_eq!(refused(&mut doc, cmd), message);
    }
    // A lathe without the C axis takes no milling tools.
    let mut plain = Document::new(MockKernel::default());
    command(
        &mut plain,
        json!({"cmd": "cam_machine_put", "machine": mill_turn(false)}),
    );
    assert_eq!(
        refused(&mut plain, tool(5, "drill", 5.0, "axial")),
        "The lathe Mill-turn takes lathe tools only."
    );
}
