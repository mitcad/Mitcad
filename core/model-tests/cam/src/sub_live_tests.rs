// SPDX-License-Identifier: MIT
//! Live tools on the sub-spindle through the JSON api (mitcad#2375) on the
//! mock kernel: the block 40 x 30 x 10 turned about the design's Z through
//! its middle (live_tests.rs, axes_tests.rs) parted off into the
//! sub-spindle, its back faced there, a hole drilled on the back and a flat
//! milled with the Y axis while the sub-spindle holds it. The program with
//! the sub-spindle's C axis, the toolpaths where the part was, the live
//! tools' maps and the stock in 3D, the lathe check of the sub-spindle's
//! travel, and the sub-spindle's machine data kept with the machine.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The region of a sketch's first rectangle.
const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

pub(super) fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

pub(super) fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

/// A lathe with a C axis, live tools, a Y axis and a sub-spindle whose
/// jaws' face travels as `travel` gives (machine Z), its own C axis codes
/// changed: a turning tool, a parting blade, an axial 3 mm drill and a
/// radial 6 mm end mill.
pub(super) fn lathe(travel: [f64; 2]) -> Value {
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
                      "axes": {"c": true, "live_tools": true, "y": true, "sub_spindle": true},
                      "mill_turn": {"sub_c_axis_on": ["M264", "G28 H0."], "sub_c_axis_off": ["M265"]},
                      "sub_spindle": {"travel": travel, "chuck_diameter": 120}}
        },
        "tools": {"tools": [
            {"number": 1, "name": "Turning", "shape": {"kind": "turning", "nose_radius": 0.4},
             "diameter": 20, "flute_length": 3, "length": 30},
            {"number": 2, "name": "Parting", "shape": {"kind": "parting", "width": 3},
             "diameter": 20, "flute_length": 25, "length": 30},
            {"number": 5, "name": "Drill", "shape": {"kind": "drill"}, "diameter": 3,
             "flute_length": 15, "length": 30, "mount": "axial",
             "cutting": {"spindle_rpm": 2000, "feed": 150, "plunge_feed": 100}},
            {"number": 8, "name": "Flat mill", "shape": {"kind": "flat"}, "diameter": 6,
             "flute_length": 18, "length": 30, "mount": "radial",
             "cutting": {"spindle_rpm": 2500, "feed": 400, "plunge_feed": 150}}
        ]}
    })
}

/// The block on the lathe, the setup handing over to the sub-spindle, the
/// main spindle's facing and part-off, then on the sub-spindle the back
/// faced, a hole at the back's (25, 15) 3 mm deep and the block's right
/// side faced 1 mm deep with the Y axis.
pub(super) fn block() -> Document<MockKernel> {
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
        json!({"cmd": "cam_machine_put", "machine": lathe([-400.0, 300.0])}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {
            "stock": {"kind": "around_bodies", "side": 2, "top": 1, "bottom": 5},
            "lathe": {"hand_over": {"withdraw": "40 mm"}}
        }}),
    );
    for operation in [
        json!({"kind": {"type": "turn_face"}, "tool": 1}),
        json!({"kind": {"type": "turn_part_off"}, "tool": 2}),
        json!({"kind": {"type": "turn_face"}, "tool": 1, "sub_spindle": true}),
        json!({"kind": {"type": "drill"}, "tool": 5, "live": "face", "sub_spindle": true,
               "inputs": [{"kind": "point", "at": {"point": [25, 15, 0]}}],
               "depths": {"bottom": "-3 mm"}}),
        json!({"kind": {"type": "facing"}, "tool": 8, "live": "plane", "sub_spindle": true,
               "plane": {"c": 0, "b": 90}, "depths": {"bottom": "19 mm"}}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    doc
}

#[test]
fn live_tools_cut_the_back_on_the_sub_spindle() {
    let mut doc = block();
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    for op in generated["setups"][0]["operations"].as_array().unwrap() {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
    }

    // The program: after the hand-over and the back's facing the
    // sub-spindle stops, its own C axis goes on, the hole is drilled and
    // the flat milled in its work offset, its C axis off at the end.
    let posted = query(&doc, json!({"query": "cam_post", "setup": 1}));
    let text = posted["text"].as_str().unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let at = |line: &str| {
        lines
            .iter()
            .position(|l| *l == line)
            .unwrap_or_else(|| panic!("no {line}:\n{text}"))
    };
    assert!(at("G55") < at("(TURN FACING 2)"));
    assert!(at("(TURN FACING 2)") < at("(DRILL 1)"));
    assert!(at("(DRILL 1)") < at("M264"), "{text}");
    assert!(at("M205") < at("M264"), "{text}");
    assert!(at("M264") < at("(FACING 1)"));
    assert!(at("(FACING 1)") < at("G19"));
    assert!(at("G19") < at("M265"));
    assert!(
        !lines.contains(&"M154"),
        "the main spindle's C axis:\n{text}"
    );
    assert!(lines.contains(&"M114"), "the sub-spindle's clamp:\n{text}");

    // The toolpaths: on the part where it was, its back at work Z -10.
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let ops = paths["operations"].as_array().unwrap();
    let drill = ops
        .iter()
        .find(|o| o["name"] == json!("Drill 1"))
        .expect("the drilling");
    assert_eq!(drill["sub_spindle"], json!(true), "{drill}");
    let lowest = drill["bounds"][0][2].as_f64().unwrap();
    let highest = drill["bounds"][1][2].as_f64().unwrap();
    // Down to 3 mm into the back (work Z -7, the drill's point a little
    // deeper), up from beyond it.
    assert!((-7.01..=-5.5).contains(&highest), "{drill}");
    assert!(lowest < -10.0, "{drill}");
    assert_eq!(paths["findings"], json!([]), "{paths}");

    // The simulation: the sub-spindle's live tools on maps of their own,
    // the stock cut in 3D.
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let live = simulated["live"].as_array().unwrap();
    let sides: Vec<(String, bool)> = live
        .iter()
        .map(|s| {
            (
                s["side"]["kind"].as_str().unwrap().to_owned(),
                s["sub_spindle"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        sides,
        [("face".to_owned(), true), ("plane".to_owned(), true)]
    );
    for s in live {
        assert!(s["removed"].as_f64().unwrap() > 0.0, "{s}");
    }
    let solid = &simulated["solid"];
    assert!(solid["milled"].as_f64().unwrap() > 0.0, "{solid}");
}

#[test]
fn the_lathe_check_knows_the_sub_spindle_s_travel() {
    let mut doc = block();
    // The work zero 200 mm from the machine's zero: the sub-spindle grips
    // at work Z -5, machine Z -205.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"placement": [0, 0, -200]}}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let findings = generated["setups"][0]["findings"].as_array().unwrap();
    assert!(
        !findings.iter().any(|f| f["message"]
            .as_str()
            .unwrap()
            .contains("sub-spindle goes beyond")),
        "{generated}"
    );
    // A sub-spindle that comes no nearer than machine Z -150.
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": lathe([-150.0, 300.0])}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let findings = generated["setups"][0]["findings"].as_array().unwrap();
    assert!(
        findings.iter().any(|f| f["message"]
            == json!(
                "The hand-over before Parting Off 1: the sub-spindle goes beyond its travel."
            )),
        "{generated}"
    );
    // The machine keeps the sub-spindle's data and codes.
    let machines = query(&doc, json!({"query": "cam_machines"}));
    let lathe = machines["machines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == json!("mill-turn"))
        .expect("the lathe")["machine"]["lathe"]
        .clone();
    assert_eq!(
        lathe["sub_spindle"],
        json!({"travel": [-150.0, 300.0], "chuck_diameter": 120.0}),
        "{lathe}"
    );
    assert_eq!(
        lathe["mill_turn"]["sub_c_axis_on"],
        json!(["M264", "G28 H0."])
    );
}

#[test]
fn the_sub_spindle_s_data_is_refused_in_words() {
    let mut doc = block();
    let mut machine = lathe([300.0, -400.0]);
    let refused = doc
        .command(&json!({"cmd": "cam_machine_put", "machine": machine.clone()}).to_string())
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("The sub-spindle's travel must run from its lowest Z up to its highest."),
        "{refused}"
    );
    machine["machine"]["lathe"]["mill_turn"]["sub_c_axis_off"] = json!([]);
    machine["machine"]["lathe"]["sub_spindle"] = json!({});
    let refused = doc
        .command(&json!({"cmd": "cam_machine_put", "machine": machine}).to_string())
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("The sub-spindle's C axis needs one to eight blocks that switch it off."),
        "{refused}"
    );
}
