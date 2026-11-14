// SPDX-License-Identifier: MIT
//! CAM through the JSON api (mitcad#1013) on the mock kernel: a 40 x 30
//! block `h` high (F2.b0) with a pocket profile on a second sketch (F3),
//! a router with a flat end mill and a drill, one setup with facing, a
//! pocket, an outside contour and a hole; values with units (inches),
//! toolpaths generated and marked out of date by a change of the model
//! (and up to date again by its undo), the G-code in millimetres and
//! inches, a pocket on a round face whose arcs stay arcs, refusals in
//! words, the design file's `cam` section read back (and read tolerantly)
//! and the machine library's files; a setup's eye kept in the design and
//! undone; inputs picked in each placement of a component placed twice
//! outlined where that placement puts them.

use serde_json::{Value, json};

use mitcad_model::cam::library::set_user_cam_library;
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::testing::temp::{remove_temp, unique_temp_dir};
use mitcad_model::{Document, EdgeName, FaceName};
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

/// The block: `h` = 10, a 40 x 30 rectangle on XY extruded `h` (F2.b0),
/// and a 12 x 10 rectangle at (10, 10) on a second sketch (F3).
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "h", "value": 10},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "h"}, "operation": "new_body"}},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [10, 10], "width": 12, "height": 10},
        ]),
    );
    doc
}

/// A router on grbl with a 6 mm flat end mill (1) and a 5 mm drill (2).
fn router() -> Value {
    json!({
        "id": "router",
        "machine": {
            "name": "Bench router",
            "controller": "grbl",
            "travel": {"min": [0, 0, -120], "max": [600, 400, 0]},
            "spindle": {"min_rpm": 8000, "max_rpm": 24000, "spin_up_seconds": 2,
                        "nose_diameter": 50, "nose_length": 80},
            "max_feed": 5000,
            "rapid_feed": 6000,
            "tool_changer": {"kind": "manual"}
        },
        "tools": {"tools": [
            {"number": 1, "name": "6 mm flat end mill", "shape": {"kind": "flat"}, "diameter": 6,
             "flute_length": 18, "length": 25,
             "cutting": {"spindle_rpm": 18000, "feed": 1200, "plunge_feed": 300}},
            {"number": 2, "shape": {"kind": "drill", "point_angle": 118}, "diameter": 5,
             "flute_length": 15, "length": 25,
             "cutting": {"spindle_rpm": 10000, "feed": 400, "plunge_feed": 200}}
        ]}
    })
}

/// The block with the router and one setup (the stock 1 mm above the
/// block, the work zero at its top's front left corner): facing, a pocket
/// on F3's profile 0.125 in deep, an outside contour of F1's profile and
/// a hole at (20, 15).
fn machined() -> Document<MockKernel> {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": router()}),
    );
    let added = command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"stock": {"kind": "around_bodies", "top": 1}}}),
    );
    assert_eq!(added["setup"], json!(1));
    for operation in [
        json!({"kind": {"type": "facing"}, "tool": 1}),
        json!({"kind": {"type": "pocket"}, "tool": 1,
               "inputs": [{"kind": "region", "sketch": "F3", "region": R}],
               "depths": {"bottom": "-0.125 in"}}),
        json!({"kind": {"type": "contour", "side": "outside"}, "tool": 1,
               "inputs": [{"kind": "region", "sketch": "F1", "region": R}]}),
        json!({"kind": {"type": "drill"}, "tool": 2,
               "inputs": [{"kind": "point", "at": {"point": [20, 15, 10]}}]}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    doc
}

fn operations(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "cam_operations", "setup": 1}))["operations"]
        .as_array()
        .unwrap()
        .clone()
}

fn states(doc: &Document<MockKernel>) -> Vec<String> {
    operations(doc)
        .iter()
        .map(|o| o["state"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn setups_and_operations_are_added_edited_moved_deleted_and_undone() {
    let mut doc = machined();
    let setups = query(&doc, json!({"query": "cam_setups"}));
    let setup = &setups["setups"][0];
    assert_eq!(setup["name"], json!("Setup 1"));
    // The design's only machine is the setup's.
    assert_eq!(setup["machine"], json!("router"));
    assert_eq!(setup["effective_dialect"], json!("grbl"));
    // The stock: the block's box 1 mm higher, in work coordinates from
    // its top's front left corner.
    assert_eq!(setup["work"]["origin"], json!([0.0, 0.0, 11.0]));
    assert_eq!(setup["work"]["stock_min"], json!([0.0, 0.0, -11.0]));
    assert_eq!(setup["work"]["stock_max"], json!([40.0, 30.0, 0.0]));
    assert_eq!(setup["work"]["bodies_top"], json!(-1.0));
    let names: Vec<&str> = setup["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Facing 1", "Pocket 1", "Contour 1", "Drill 1"]);
    let ids: Vec<u64> = setup["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_u64().unwrap())
        .collect();
    assert_eq!(ids, [2, 3, 4, 5]);
    assert!(states(&doc).iter().all(|s| s == "not_generated"));
    // The pocket's bottom is kept as typed.
    assert_eq!(
        setup["operations"][1]["depths"]["bottom"],
        json!("-0.125 in")
    );

    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": 3,
               "changes": {"name": "Window", "stepover": 2.5}}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_operation_move", "operation": 5, "index": 0}),
    );
    let names: Vec<String> = operations(&doc)
        .iter()
        .map(|o| o["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["Drill 1", "Facing 1", "Window", "Contour 1"]);
    assert_eq!(operations(&doc)[2]["stepover"], json!(2.5));
    // A null clears an optional value.
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": 3, "changes": {"stepover": null}}),
    );
    assert!(operations(&doc)[2].get("stepover").is_none());
    command(
        &mut doc,
        json!({"cmd": "cam_operation_delete", "operation": 4}),
    );
    assert_eq!(operations(&doc).len(), 3);
    let undone = command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(undone["label"], json!("Delete CAM Operation"));
    assert_eq!(operations(&doc).len(), 4);

    // A second setup by name, moved first, then deleted.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"name": "Back side", "work_offset": 2}}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_move", "setup": "back side", "index": 0}),
    );
    let setups = query(&doc, json!({"query": "cam_setups"}));
    assert_eq!(setups["setups"][0]["name"], json!("Back side"));
    assert_eq!(setups["setups"][0]["work_offset"], json!(2));
    command(
        &mut doc,
        json!({"cmd": "cam_setup_delete", "setup": "Back side"}),
    );
    assert_eq!(
        query(&doc, json!({"query": "cam_setups"}))["setups"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn tools_take_inches_and_the_design_s_parameters() {
    let mut doc = machined();
    let put = command(
        &mut doc,
        json!({"cmd": "cam_tool_put", "machine": "router", "tool": {
            "number": 3, "shape": {"kind": "bull", "corner_radius": "0.02 in"},
            "diameter": "0.25 in", "flute_length": "h",
            "cutting": {"spindle_rpm": 16000, "feed": "40 in", "plunge_feed": 250}}}),
    );
    let tool = &put["tool"];
    assert_eq!(tool["number"], json!(3));
    assert!((tool["diameter"].as_f64().unwrap() - 6.35).abs() < 1e-9);
    assert!((tool["shape"]["corner_radius"].as_f64().unwrap() - 0.508).abs() < 1e-9);
    assert_eq!(tool["flute_length"], json!(10.0));
    // The stick-out: the flutes and 5 mm.
    assert_eq!(tool["length"], json!(15.0));
    assert!((tool["cutting"]["feed"].as_f64().unwrap() - 1016.0).abs() < 1e-9);
    assert_eq!(tool["name"], json!("6.35 mm bull nose end mill"));
    let machines = query(&doc, json!({"query": "cam_machines"}));
    let tools = machines["machines"][0]["tools"]["tools"]
        .as_array()
        .unwrap();
    assert_eq!(
        tools
            .iter()
            .map(|t| t["number"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(machines["machines"][0]["used_by"], json!([1]));
    command(
        &mut doc,
        json!({"cmd": "cam_tool_delete", "machine": "router", "number": 3}),
    );
    let machines = query(&doc, json!({"query": "cam_machines"}));
    assert_eq!(
        machines["machines"][0]["tools"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn toolpaths_are_generated_and_a_model_change_marks_them_out_of_date() {
    let mut doc = machined();
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let ops = generated["setups"][0]["operations"].as_array().unwrap();
    assert_eq!(ops.len(), 4);
    for op in ops {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
        assert!(op["cut_length"].as_f64().unwrap() > 0.0, "{op}");
        assert!(op["duration"].as_f64().unwrap() > 0.0, "{op}");
    }
    assert!(states(&doc).iter().all(|s| s == "up_to_date"));

    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let by_name = |name: &str| -> Value {
        paths["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["name"] == json!(name))
            .unwrap()
            .clone()
    };
    // The pocket goes 0.125 in (3.175 mm) below the stock's top, inside
    // its profile less the tool's radius.
    let pocket = by_name("Pocket 1");
    let bounds = &pocket["bounds"];
    assert!(
        (bounds[0][2].as_f64().unwrap() + 3.175).abs() < 1e-9,
        "{bounds}"
    );
    assert!(bounds[0][0].as_f64().unwrap() >= 13.0 - 1e-6, "{bounds}");
    assert!(bounds[1][0].as_f64().unwrap() <= 19.0 + 1e-6, "{bounds}");
    // The facing takes the stock down to the block's top.
    let facing = by_name("Facing 1");
    assert!((facing["bounds"][0][2].as_f64().unwrap() + 1.0).abs() < 1e-9);
    // The contour cuts through the stock; its corners are arcs.
    let contour = by_name("Contour 1");
    assert!((contour["bounds"][0][2].as_f64().unwrap() + 11.0).abs() < 1e-9);
    assert!(
        contour["toolpath"]["moves"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["kind"] == json!("arc"))
    );
    // The hole at (20, 15).
    let drill = by_name("Drill 1");
    assert_eq!(drill["tool"]["number"], json!(2));
    let moves = drill["toolpath"]["moves"].as_array().unwrap();
    assert!(moves.iter().any(|m| m["to"][0] == json!(20.0)
        && m["to"][1] == json!(15.0)
        && (m["to"][2].as_f64().unwrap() + 11.0).abs() < 1e-9));

    // A taller block: the stock and every toolpath change.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "h", "value": 12}),
    );
    assert!(
        states(&doc).iter().all(|s| s == "out_of_date"),
        "{:?}",
        states(&doc)
    );
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(
        states(&doc).iter().all(|s| s == "up_to_date"),
        "{:?}",
        states(&doc)
    );
    // So does an edit of the operation itself, but not its name.
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": 3, "changes": {"name": "Window"}}),
    );
    assert_eq!(states(&doc)[1], "up_to_date");
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": 3,
               "changes": {"depths": {"bottom": -2}}}),
    );
    assert_eq!(states(&doc)[1], "out_of_date");
    // A suppressed operation is neither computed nor written.
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": 2, "changes": {"suppressed": true}}),
    );
    assert_eq!(states(&doc)[0], "suppressed");
    let generated = command(&mut doc, json!({"cmd": "cam_generate"}));
    assert_eq!(
        generated["setups"][0]["operations"][0]["state"],
        json!("suppressed")
    );
    assert!(states(&doc)[1..].iter().all(|s| s == "up_to_date"));
}

#[test]
fn a_setup_is_written_in_millimetres_or_inches() {
    let mut doc = machined();
    let post = query(&doc, json!({"query": "cam_post", "setup": 1}));
    assert_eq!(post["dialect"], json!("grbl"));
    assert_eq!(post["extension"], json!("nc"));
    assert_eq!(post["units"], json!("millimetres"));
    let text = post["text"].as_str().unwrap();
    assert!(text.lines().any(|l| l.contains("G21")), "{text}");
    assert!(!text.lines().any(|l| l.contains("G20")), "{text}");
    assert!(
        text.lines()
            .any(|l| l.starts_with("G2 ") || l.starts_with("G3 ")),
        "{text}"
    );
    // Every operation by its name, the tools changed by hand on grbl.
    for name in ["(Facing 1)", "(Pocket 1)", "(Contour 1)", "(Drill 1)"] {
        assert!(text.contains(name), "{name}: {text}");
    }
    assert!(text.lines().any(|l| l.starts_with("M0")), "{text}");

    let inches = query(
        &doc,
        json!({"query": "cam_post", "setup": 1, "units": "inches"}),
    );
    assert_eq!(inches["units"], json!("inches"));
    let text = inches["text"].as_str().unwrap();
    assert!(text.lines().any(|l| l.contains("G20")), "{text}");
    assert!(!text.lines().any(|l| l.contains("G21")), "{text}");
    // The pocket's floor, 0.125 in down.
    assert!(text.contains("Z-0.125"), "{text}");

    // The setup's own units and another dialect.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1,
               "changes": {"post": {"units": "inches"}, "dialect": "linuxcnc"}}),
    );
    let post = query(&doc, json!({"query": "cam_post", "setup": 1}));
    assert_eq!(post["dialect"], json!("linuxcnc"));
    assert_eq!(post["extension"], json!("ngc"));
    assert!(post["text"].as_str().unwrap().contains("G20"));
    let dialects = query(&doc, json!({"query": "cam_dialects"}));
    assert!(dialects["dialects"].as_array().unwrap().len() >= 7);
}

#[test]
fn a_pocket_on_a_round_face_keeps_its_arcs() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": router()}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"stock": {"kind": "around_bodies", "top": 1}}}),
    );
    // A round floor 4 mm above the block's base: its boundary a circle.
    let face: FaceName = format!("F2:end({R})").parse().unwrap();
    doc.kernel().face_boundaries.borrow_mut().insert(
        face.to_string(),
        vec![(
            EdgeName::new(face.clone(), face.clone()),
            Curve3::Conic {
                center: [20.0, 15.0, 4.0],
                normal: [0.0, 0.0, 1.0],
                x_axis: [1.0, 0.0, 0.0],
                major: 8.0,
                minor: 8.0,
                start: 0.0,
                end: 2.0 * std::f64::consts::PI,
                closed: true,
            },
        )],
    );
    command(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
            "kind": {"type": "pocket"}, "tool": 1,
            "inputs": [{"kind": "face", "body": "F2.b0", "face": face.to_string()}]}}),
    );
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let pocket = &paths["operations"][0];
    assert!(pocket.get("error").is_none(), "{pocket}");
    // Down to the face: 4 above the base is 7 below the stock's top.
    assert!((pocket["bounds"][0][2].as_f64().unwrap() + 7.0).abs() < 1e-9);
    // Within the circle less the tool's radius.
    assert!(pocket["bounds"][1][0].as_f64().unwrap() <= 25.0 + 1e-6);
    assert!(
        pocket["toolpath"]["moves"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["kind"] == json!("arc"))
    );
}

#[test]
fn refusals_say_what_is_wrong_in_words() {
    let mut doc = machined();
    let cases = [
        (
            json!({"cmd": "cam_operation_add", "setup": 1,
                   "operation": {"kind": {"type": "pocket"}, "tool": 9}}),
            "The setup's machine has no tool 9.",
        ),
        (
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": {"tool": 1}}),
            "An operation needs its kind: Facing, Contour, Pocket, Adaptive Clearing, Rest Machining, Drill, 3D Roughing, Raster Finishing or Waterline Finishing.",
        ),
        (
            json!({"cmd": "cam_setup_add", "setup": {"machine": "mill"}}),
            "The design keeps no such machine: put the machine in first.",
        ),
        (
            json!({"cmd": "cam_machine_delete", "machine": "router"}),
            "A setup uses the machine: choose another machine for it first.",
        ),
        (
            json!({"cmd": "cam_operation_edit", "operation": 3,
                   "changes": {"tabs": {"count": 4, "width": 5, "height": 2}}}),
            "Holding tabs are for contours.",
        ),
        (
            json!({"cmd": "cam_operation_delete", "operation": 99}),
            "There is no such CAM operation.",
        ),
        (
            json!({"cmd": "cam_setup_add", "setup": {"bodies": ["F9.b0"]}}),
            "A body of the setup is not in the design at the timeline marker.",
        ),
    ];
    for (command, message) in cases {
        assert_eq!(refused(&mut doc, command.clone()), message, "{command}");
    }
    // An operation that cannot be computed says why, and the program is
    // not written without it.
    command(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
            "name": "Clear", "kind": {"type": "pocket"}, "tool": 1}}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let clear = &generated["setups"][0]["operations"][4];
    assert_eq!(clear["state"], json!("failed"));
    assert_eq!(
        clear["error"],
        json!("The operation has no region to clear: pick faces or sketch profiles.")
    );
    let error = doc
        .query(&json!({"query": "cam_post", "setup": 1}).to_string())
        .unwrap_err()
        .to_string();
    assert_eq!(
        error,
        "Clear: The operation has no region to clear: pick faces or sketch profiles."
    );
}

#[test]
fn the_cam_section_is_saved_and_read_back() {
    let plain = block();
    assert!(!plain.to_json().contains("\"cam\""));

    let mut doc = machined();
    command(&mut doc, json!({"cmd": "cam_generate"}));
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["cam"]["version"], json!(1));
    assert_eq!(file["cam"]["machines"][0]["id"], json!("router"));
    assert_eq!(
        file["cam"]["setups"][0]["operations"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(file["cam"]["setups"][0]["operations"][0]["generated"].is_string());
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(reopened.to_json(), saved);
    // A document read from a file builds its bodies before it is used.
    reopened.recompute();
    assert_eq!(
        query(&reopened, json!({"query": "cam_setups"})),
        query(&doc, json!({"query": "cam_setups"}))
    );
    // Generated before the save, up to date after it.
    assert!(states(&reopened).iter().all(|s| s == "up_to_date"));

    // Fields a newer MitCad adds inside the section are ignored.
    let mut newer = file.clone();
    newer["cam"]["setups"][0]["fixtures"] = json!([{"name": "vise"}]);
    newer["cam"]["setups"][0]["operations"][1]["engagement"] = json!(0.3);
    newer["cam"]["machines"][0]["probe"] = json!({"kind": "touch"});
    let read = Document::from_json(&newer.to_string(), MockKernel::default()).unwrap();
    assert_eq!(read.to_json(), saved);

    // Two setups with one id are refused.
    let mut twice = file.clone();
    twice["cam"]["setups"][0]["operations"][1]["id"] = json!(2);
    assert!(Document::from_json(&twice.to_string(), MockKernel::default()).is_err());
}

#[test]
fn machines_go_to_and_come_from_the_libraries() {
    let user = unique_temp_dir("cam-user-library");
    let project = unique_temp_dir("cam-project");
    set_user_cam_library(Some(user.clone()));
    let mut doc = machined();
    let saved = command(
        &mut doc,
        json!({"cmd": "cam_machine_to_library", "machine": "router", "library": "user"}),
    );
    assert_eq!(saved["version"], json!(1));
    let path = std::path::PathBuf::from(saved["path"].as_str().unwrap());
    assert!(path.starts_with(&user));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("\"format\": \"mitcad-cam-machine\""),
        "{text}"
    );
    // Saved again: the next version; into the project under another id.
    let again = command(
        &mut doc,
        json!({"cmd": "cam_machine_to_library", "machine": "router", "library": "user"}),
    );
    assert_eq!(again["version"], json!(2));
    command(
        &mut doc,
        json!({"cmd": "cam_machine_to_library", "machine": "router", "library": "project",
               "project": project.display().to_string(), "as_id": "shop-router"}),
    );
    let library = query(
        &doc,
        json!({"query": "cam_library", "project": project.display().to_string()}),
    );
    let found: Vec<(String, String, u64)> = library["machines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["library"].as_str().unwrap().to_owned(),
                m["id"].as_str().unwrap().to_owned(),
                m["version"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("user".to_owned(), "router".to_owned(), 2),
            ("project".to_owned(), "shop-router".to_owned(), 1)
        ]
    );
    assert_eq!(library["machines"][1]["in_design"], json!("router"));

    // Another design takes the project's machine with its tools.
    let mut other = block();
    let copied = command(
        &mut other,
        json!({"cmd": "cam_machine_from_library", "library": "project", "id": "shop-router",
               "project": project.display().to_string()}),
    );
    assert_eq!(copied["machine"], json!("shop-router"));
    let machines = query(&other, json!({"query": "cam_machines"}));
    let machine = &machines["machines"][0];
    assert_eq!(machine["machine"]["name"], json!("Bench router"));
    assert_eq!(machine["tools"]["tools"].as_array().unwrap().len(), 2);
    assert_eq!(
        machine["origin"],
        json!({"library": "project", "id": "shop-router", "version": 1})
    );
    set_user_cam_library(None);
    remove_temp(&user);
    remove_temp(&project);
}

#[test]
fn rest_machining_follows_the_bigger_tool_and_drilling_writes_canned_cycles() {
    let mut doc = block();
    let mut machine = router();
    machine["tools"]["tools"]
        .as_array_mut()
        .unwrap()
        .push(json!(
            {"number": 3, "name": "2 mm flat end mill", "shape": {"kind": "flat"}, "diameter": 2,
             "flute_length": 6, "length": 12,
             "cutting": {"spindle_rpm": 20000, "feed": 600, "plunge_feed": 150}}
        ));
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": machine}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"stock": {"kind": "around_bodies", "top": 1}}}),
    );
    let region = json!([{"kind": "region", "sketch": "F3", "region": R}]);
    for operation in [
        json!({"kind": {"type": "pocket"}, "tool": 1, "inputs": region,
               "depths": {"bottom": -3, "step_down": 3}}),
        json!({"kind": {"type": "rest", "from_tool": 1}, "tool": 3, "inputs": region,
               "depths": {"bottom": -3, "step_down": 3}}),
        json!({"kind": {"type": "drill", "peck": true, "chip_break": true}, "tool": 2,
               "inputs": [{"kind": "point", "at": {"point": [20, 15, 10]}}],
               "depths": {"step_down": 2}}),
        json!({"kind": {"type": "rest"}, "tool": 3, "inputs": region,
               "depths": {"bottom": -3, "step_down": 3}}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let ops = generated["setups"][0]["operations"].as_array().unwrap();
    for op in &ops[..3] {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
    }
    // The rest cuts the four corners the 6 mm tool rounded.
    assert!(ops[1]["cut_length"].as_f64().unwrap() > 0.0, "{}", ops[1]);
    // Without the bigger tool it says what is missing.
    assert_eq!(ops[3]["state"], json!("failed"));
    assert_eq!(
        ops[3]["error"],
        json!("Choose the bigger tool the rest machining follows.")
    );
    command(
        &mut doc,
        json!({"cmd": "cam_operation_delete", "operation": ops[3]["id"]}),
    );
    // The holes are recorded as one chip-breaking cycle.
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let rest_path = paths["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == json!("Rest 1"))
        .unwrap()["toolpath"]
        .clone();
    let plunges = rest_path["moves"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["kind"] == json!("line") && m["feed"] == json!(150.0))
        .count();
    assert_eq!(plunges, 4, "one stretch per corner: {rest_path}");
    let drill = paths["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == json!("Drill 1"))
        .unwrap()
        .clone();
    assert_eq!(drill["toolpath"]["cycles"][0]["kind"], json!("chip_break"));
    // Written as moves until the setup asks for canned cycles.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"dialect": "linuxcnc"}}),
    );
    let text = query(&doc, json!({"query": "cam_post", "setup": 1}))["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!text.contains("G73"), "{text}");
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1,
               "changes": {"post": {"canned_cycles": true}}}),
    );
    let text = query(&doc, json!({"query": "cam_post", "setup": 1}))["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(text.lines().any(|l| l.starts_with("G98 G73 ")), "{text}");
    assert!(text.contains("\nG80\n"), "{text}");
}

/// The label of the next undo.
fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn visible(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "cam_setups"}))["setups"][0]["visible"].clone()
}

#[test]
fn a_setup_s_eye_is_kept_in_the_design_and_undone() {
    let mut doc = machined();
    command(&mut doc, json!({"cmd": "cam_generate"}));
    // Shown by default, and the file does not say so.
    assert_eq!(visible(&doc), json!(true));
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(file["cam"]["setups"][0].get("visible").is_none());

    // Hidden: one undo step named as a body's eye, no toolpath out of
    // date.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"visible": false}}),
    );
    assert_eq!(visible(&doc), json!(false));
    assert_eq!(undo_label(&doc), json!("Hide Setup 1"));
    assert!(states(&doc).iter().all(|s| s == "up_to_date"));
    // Hidden again: nothing to undo.
    let depth = query(&doc, json!({"query": "document"}))["undo_depth"].clone();
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"visible": false}}),
    );
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo_depth"],
        depth
    );

    // Saved and opened again: still hidden, still up to date.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["cam"]["setups"][0]["visible"], json!(false));
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(reopened.to_json(), saved);
    // A document read from a file builds its bodies before it is used.
    reopened.recompute();
    assert_eq!(visible(&reopened), json!(false));
    assert!(states(&reopened).iter().all(|s| s == "up_to_date"));

    // Undone and redone.
    let undone = command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(undone["label"], json!("Hide Setup 1"));
    assert_eq!(visible(&doc), json!(true));
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(visible(&doc), json!(false));

    // Shown by its eye; with another change it is an edit.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"visible": true}}),
    );
    assert_eq!(undo_label(&doc), json!("Show Setup 1"));
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1,
               "changes": {"name": "Front", "visible": false}}),
    );
    assert_eq!(undo_label(&doc), json!("Edit CAM Setup"));
    assert_eq!(visible(&doc), json!(false));
}

/// A Plate component (its sketch F1, its body F2.b0: 20 x 20, 10 high)
/// placed twice: at (5, 5, 50) and at (100, 0, 50); nothing in the root.
/// The top face's boundary (F2:end) is four lines at its height, in the
/// component's coordinates. Returns the two occurrences.
fn plates() -> (Document<MockKernel>, String, String) {
    let mut doc = Document::new(MockKernel::default());
    let created = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plate",
               "transform": {"translation": [5, 5, 50]}}),
    );
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    let copied = command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "Plate:1",
               "transform": {"translation": [100, 0, 50]}}),
    );
    let face: FaceName = format!("F2:end({R})").parse().unwrap();
    let corners = [
        [0.0, 0.0, 10.0],
        [20.0, 0.0, 10.0],
        [20.0, 20.0, 10.0],
        [0.0, 20.0, 10.0],
    ];
    let lines: Vec<(EdgeName, Curve3)> = (0..4)
        .map(|i| {
            (
                EdgeName::new(face.clone(), face.clone()),
                Curve3::Line {
                    start: corners[i],
                    end: corners[(i + 1) % 4],
                },
            )
        })
        .collect();
    doc.kernel()
        .face_boundaries
        .borrow_mut()
        .insert(face.to_string(), lines);
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": router()}),
    );
    let first = created["occurrence"].as_str().unwrap().to_owned();
    let second = copied["occurrence"].as_str().unwrap().to_owned();
    (doc, first, second)
}

fn near(value: &Value, expected: f64, what: &str) {
    let got = value.as_f64().unwrap_or(f64::NAN);
    assert!(
        (got - expected).abs() <= 1e-6,
        "{what}: {value}, not {expected}"
    );
}

/// An operation's toolpath bounds lie within `[lo, hi]` in X and Y and go
/// down to `bottom`.
fn within(op: &Value, lo: [f64; 2], hi: [f64; 2], bottom: f64) {
    assert!(op.get("error").is_none(), "{op}");
    let bounds = &op["bounds"];
    for k in 0..2 {
        let min = bounds[0][k].as_f64().unwrap();
        let max = bounds[1][k].as_f64().unwrap();
        assert!(min >= lo[k] - 1e-6 && max <= hi[k] + 1e-6, "{k}: {bounds}");
    }
    near(&bounds[0][2], bottom, "bottom");
}

#[test]
fn inputs_picked_in_each_placement_are_outlined_where_it_is() {
    let (mut doc, first, second) = plates();
    let face = format!("F2:end({R})");
    // Every body as placed: both plates, the stock 1 mm above them, the
    // work zero at its top's front left corner.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"stock": {"kind": "around_bodies", "top": 1}}}),
    );
    let work = query(&doc, json!({"query": "cam_setups"}))["setups"][0]["work"].clone();
    near(&work["origin"][0], 5.0, "origin x");
    near(&work["origin"][1], 0.0, "origin y");
    near(&work["origin"][2], 61.0, "origin z");
    near(&work["stock_max"][0], 115.0, "stock x");
    near(&work["stock_max"][1], 25.0, "stock y");
    near(&work["stock_min"][2], -11.0, "stock bottom");

    // The same face picked in each placement, and the sketch's profile
    // in the second.
    for (name, input) in [
        (
            "Left",
            json!({"kind": "face", "body": "F2.b0", "face": face, "occurrence": first}),
        ),
        (
            "Right",
            json!({"kind": "face", "body": "F2.b0", "face": face, "occurrence": second}),
        ),
        (
            "Profile",
            json!({"kind": "region", "sketch": "F1", "region": R, "occurrence": second}),
        ),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
                "name": name, "kind": {"type": "pocket"}, "tool": 1, "inputs": [input]}}),
        );
    }
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let ops = paths["operations"].as_array().unwrap();
    // In work coordinates, less the 6 mm tool's radius: the first plate
    // spans x 0 to 20 and y 5 to 25, the second x 95 to 115 and y 0 to
    // 20; the top face 1 below the stock's top, the profile at the
    // plates' base.
    within(&ops[0], [3.0, 8.0], [17.0, 22.0], -1.0);
    within(&ops[1], [98.0, 3.0], [112.0, 17.0], -1.0);
    within(&ops[2], [98.0, 3.0], [112.0, 17.0], -11.0);

    // Kept in the file with their occurrences, and read back.
    command(&mut doc, json!({"cmd": "cam_generate"}));
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let inputs = &file["cam"]["setups"][0]["operations"];
    assert_eq!(inputs[0]["inputs"][0]["occurrence"], json!(first));
    assert_eq!(inputs[1]["inputs"][0]["occurrence"], json!(second));
    // Read into a kernel that knows the top face's boundary too, and its
    // bodies built.
    let kernel = MockKernel::default();
    *kernel.face_boundaries.borrow_mut() = doc.kernel().face_boundaries.borrow().clone();
    let mut reopened = Document::from_json(&saved, kernel).unwrap();
    assert_eq!(reopened.to_json(), saved);
    reopened.recompute();
    assert_eq!(
        query(&reopened, json!({"query": "cam_toolpaths", "setup": 1}))["operations"][1]["bounds"],
        ops[1]["bounds"]
    );

    // Moving the second placement moves its toolpaths: out of date.
    command(
        &mut doc,
        json!({"cmd": "ground_occurrence", "occurrence": second, "grounded": false}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": second,
               "transform": {"translation": [100, 40, 50]}}),
    );
    assert_eq!(states(&doc)[1], "out_of_date");

    // A setup of the second plate alone, by its occurrence.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"name": "Second",
               "bodies": [{"body": "F2.b0", "occurrence": second}]}}),
    );
    let setups = query(&doc, json!({"query": "cam_setups"}));
    let work = &setups["setups"][1]["work"];
    near(&work["origin"][0], 100.0, "second origin x");
    near(&work["origin"][1], 40.0, "second origin y");
    near(&work["origin"][2], 60.0, "second origin z");
    assert_eq!(
        setups["setups"][1]["bodies"],
        json!([{"body": "F2.b0", "occurrence": second}])
    );

    // A pick in a component placed twice needs its placement.
    let cases = [
        (
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
                "kind": {"type": "pocket"}, "tool": 1,
                "inputs": [{"kind": "face", "body": "F2.b0", "face": face}]}}),
            "A CAM input is in a component placed more than once: pick it in one of its placements.",
        ),
        (
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
                "kind": {"type": "pocket"}, "tool": 1,
                "inputs": [{"kind": "face", "body": "F2.b0", "face": face, "occurrence": "O99"}]}}),
            "A placement a CAM input was picked in is gone.",
        ),
        (
            json!({"cmd": "cam_setup_add", "setup": {"bodies": ["F2.b0"]}}),
            "A CAM input is in a component placed more than once: pick it in one of its placements.",
        ),
    ];
    for (command, message) in cases {
        assert_eq!(refused(&mut doc, command.clone()), message, "{command}");
    }
}

/// The end points of a toolpath's cutting moves (lines and arcs).
fn cut_ends(op: &Value) -> Vec<[f64; 3]> {
    op["toolpath"]["moves"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["kind"] != json!("rapid"))
        .map(|m| {
            let to = &m["to"];
            [0, 1, 2].map(|k| to[k].as_f64().unwrap())
        })
        .collect()
}

/// Points along the cutting moves (ten to a move, its end included),
/// from where the move before ended.
fn cut_points(op: &Value) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    let mut at: Option<[f64; 3]> = None;
    for m in op["toolpath"]["moves"].as_array().unwrap() {
        let to = [0, 1, 2].map(|k| m["to"][k].as_f64().unwrap());
        if let Some(from) = at
            && m["kind"] != json!("rapid")
        {
            for i in 1..=10 {
                let t = f64::from(i) / 10.0;
                out.push([0, 1, 2].map(|k| from[k] + (to[k] - from[k]) * t));
            }
        }
        at = Some(to);
    }
    out
}

/// The 3-axis operations (phase 3) on the block, which the mock kernel
/// meshes as its box (40 x 30 x 10, in the stock's work coordinates from
/// (5, 5, -11) to (45, 35, -1)): 3D roughing clears the millimetre above
/// it down to the stock to leave and round it to the stock's bottom,
/// raster finishing runs over its top, waterline round its sides at the
/// tool's radius; their kinds and angles are kept as given (values with
/// units), and a change of the block marks them out of date.
#[test]
fn three_axis_operations_machine_the_bodies_surfaces() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": router()}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_tool_put", "machine": "router", "tool": {
            "number": 3, "shape": {"kind": "ball"}, "diameter": 6,
            "cutting": {"spindle_rpm": 18000, "feed": 1500, "plunge_feed": 400}}}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"stock": {"kind": "around_bodies", "side": 5, "top": 1}}}),
    );
    for operation in [
        json!({"kind": {"type": "roughing_3d"}, "tool": 1, "stock_to_leave": "0.5 mm"}),
        json!({"kind": {"type": "raster_finish", "angle": "90 deg"}, "tool": 3,
               "stepover": 2, "inputs": [{"kind": "body", "body": "F2.b0"}]}),
        json!({"kind": {"type": "waterline_finish", "slope": 30}, "tool": 1}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    let listed = operations(&doc);
    assert_eq!(listed[0]["name"], json!("3D Roughing 1"));
    assert_eq!(listed[0]["kind"], json!({"type": "roughing_3d"}));
    assert_eq!(
        listed[1]["kind"],
        json!({"type": "raster_finish", "angle": "90 deg"})
    );
    assert_eq!(
        listed[1]["inputs"],
        json!([{"kind": "body", "body": "F2.b0"}])
    );
    assert_eq!(
        listed[2]["kind"],
        json!({"type": "waterline_finish", "slope": 30.0})
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    for op in generated["setups"][0]["operations"].as_array().unwrap() {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
    }
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let ops = paths["operations"].as_array().unwrap();
    // Roughing: over the block at the stock to leave, round it to the
    // stock's bottom.
    let rough = cut_ends(&ops[0]);
    let over = rough
        .iter()
        .filter(|p| p[0] > 10.0 && p[0] < 40.0 && p[1] > 10.0 && p[1] < 30.0)
        .map(|p| p[2])
        .fold(f64::INFINITY, f64::min);
    assert!(
        (over + 0.5).abs() < 1e-9,
        "the roughing over the block at {over}"
    );
    let lowest = rough.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    assert!((lowest + 11.0).abs() < 1e-9, "{lowest}");
    // Raster: on the block's top wherever the ball is wholly over it
    // (along the passes: a pass over the top is one move from edge to
    // edge).
    let raster = cut_points(&ops[1]);
    let on_top: Vec<&[f64; 3]> = raster
        .iter()
        .filter(|p| p[0] > 8.0 && p[0] < 42.0 && p[1] > 8.0 && p[1] < 32.0)
        .collect();
    assert!(on_top.len() > 10);
    assert!(on_top.iter().all(|p| (p[2] + 1.0).abs() < 1e-9));
    // Waterline: the flat end mill's center 3 mm off the block's sides
    // (its walls are vertical: steeper than 30 degrees).
    let waterline = cut_ends(&ops[2]);
    let sides: Vec<&[f64; 3]> = waterline.iter().filter(|p| p[2] < -1.5).collect();
    assert!(sides.len() > 20);
    for p in sides {
        let dx = (5.0 - p[0]).max(p[0] - 45.0).max(0.0);
        let dy = (5.0 - p[1]).max(p[1] - 35.0).max(0.0);
        assert!((dx.hypot(dy) - 3.0).abs() < 1e-5, "{p:?}");
    }
    // A taller block: every toolpath out of date.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "h", "value": 12}),
    );
    assert!(
        states(&doc).iter().all(|s| s == "out_of_date"),
        "{:?}",
        states(&doc)
    );
    // Bodies are inputs of the 3D operations only, and these take no
    // points.
    let cases = [
        (
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
                "kind": {"type": "pocket"}, "tool": 1,
                "inputs": [{"kind": "body", "body": "F2.b0"}]}}),
            "Only the 3D operations take bodies as inputs.",
        ),
        (
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
                "kind": {"type": "raster_finish"}, "tool": 3,
                "inputs": [{"kind": "point", "at": {"point": [20, 15, 10]}}]}}),
            "A 3D operation takes bodies, and faces or sketch profiles as its boundary.",
        ),
        (
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
                "kind": {"type": "waterline_finish"}, "tool": 1,
                "entry": {"kind": "ramp", "angle": 3}}}),
            "Ramp and helix entries are for contours and the clearing operations.",
        ),
    ];
    for (command, message) in cases {
        assert_eq!(refused(&mut doc, command.clone()), message, "{command}");
    }
}
