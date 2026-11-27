// SPDX-License-Identifier: MIT
//! A turning tool's and a boring bar's insert and holder through the JSON
//! api (mitcad#2502, GitHub mitcad/Mitcad#705): ISO codes and typed angles
//! resolved by `cam_tool_put` and previewed by `cam_tool_shape` through the
//! same rules, the insert code's corner radius only without a typed nose
//! radius, the typed values kept beside the codes through Edit Tool, the
//! design file and the machine library, and tools without an insert as
//! before.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model::testing::temp::{remove_temp, unique_temp_dir};
use mitcad_model_api::DocumentApi;

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

fn query_refused(doc: &Document<MockKernel>, query: Value) -> String {
    match doc.query(&query.to_string()) {
        Ok(answer) => panic!("{query} was answered: {answer}"),
        Err(e) => e.to_string(),
    }
}

/// A lathe with one turning tool kept without an insert, as before
/// mitcad#2502.
fn lathe_doc() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": {
            "id": "lathe",
            "machine": {
                "name": "Lathe", "controller": "fanuc_turning",
                "travel": {"min": [-5, 0, -300], "max": [150, 0, 200]},
                "spindle": {"min_rpm": 50, "max_rpm": 3000, "nose_diameter": 0, "nose_length": 0},
                "max_feed": 5000, "rapid_feed": 20000,
                "lathe": {"turret_stations": 8, "chuck_diameter": 160, "chuck_length": 50}
            },
            "tools": {"tools": [
                {"number": 1, "name": "Turning", "shape": {"kind": "turning", "nose_radius": 0.4},
                 "diameter": 20, "flute_length": 3, "length": 30}
            ]}
        }}),
    );
    doc
}

fn shape_of(doc: &Document<MockKernel>, tool: Value) -> Value {
    query(doc, json!({"query": "cam_tool_shape", "tool": tool}))
}

fn put(doc: &mut Document<MockKernel>, tool: Value) -> Value {
    command(
        doc,
        json!({"cmd": "cam_tool_put", "machine": "lathe", "tool": tool}),
    )["tool"]
        .clone()
}

/// The machine's tool of a number as the design keeps it.
fn kept(doc: &Document<MockKernel>, number: u64) -> Value {
    let machines = query(doc, json!({"query": "cam_machines"}));
    machines["machines"][0]["tools"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["number"] == json!(number))
        .cloned()
        .unwrap_or_else(|| panic!("no tool {number}"))
}

/// The points of an outline's polygon, `[x, z]`.
fn points(outline: &Value, part: &str) -> Vec<(f64, f64)> {
    outline[part]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap()))
        .collect()
}

/// The tool Edit Tool sends back when it is opened on a kept tool and OK
/// is pressed without a change (`askTool`, app/cam/CamMachinesDialog.cpp):
/// the codes as kept, only the typed angles and hand in their fields (the
/// codes' values as placeholders), and the nose radius field empty when it
/// is the insert code's corner radius.
fn as_edited(kept: &Value) -> Value {
    let insert = &kept["insert"];
    let typed = &insert["typed"];
    let mut shape = json!({"kind": kept["shape"]["kind"]});
    let from_code = insert["insert"].is_string() && typed["nose_radius"].is_null();
    if !from_code {
        shape["nose_radius"] = json!(format!("{} mm", kept["shape"]["nose_radius"]));
    }
    let mut fields = json!({});
    for key in ["insert", "holder"] {
        if let Some(code) = insert[key].as_str() {
            fields[key] = json!(code);
        }
    }
    for key in ["tip_angle", "approach_angle"] {
        if let Some(angle) = typed[key].as_f64() {
            fields[key] = json!(angle);
        }
    }
    if let Some(hand) = typed["hand"].as_str() {
        fields["hand"] = json!(hand);
    }
    json!({
        "number": kept["number"], "name": kept["name"], "shape": shape,
        "diameter": format!("{} mm", kept["diameter"]),
        "flute_length": format!("{} mm", kept["flute_length"]),
        "length": format!("{} mm", kept["length"]),
        "flutes": kept["flutes"], "insert": fields
    })
}

/// CNMG 120408 in a PCLNR 2525 M12: previewed and kept with a 5 degree back
/// angle and the code's 0.8 mm nose radius; the preview keeps nothing.
#[test]
fn a_turning_tool_takes_its_insert_and_holder_codes() {
    let mut doc = lathe_doc();
    let tool = json!({"number": 2, "shape": {"kind": "turning"}, "diameter": "25 mm",
                      "insert": {"insert": "cnmg 120408", "holder": "PCLNR 2525 M12"}});
    let before = doc.to_json();
    let shape = shape_of(&doc, tool.clone());
    // Preview queries do not change the design or its tool library.
    assert_eq!(doc.to_json(), before);
    assert_eq!(shape["back_angle"], json!(5.0));
    assert_eq!(shape["insert"]["text"], json!("CNMG 120408"));
    assert_eq!(shape["insert"]["nose_radius"], json!(0.8));
    assert_eq!(shape["holder"]["approach_angle"], json!(95.0));
    assert_eq!(shape["holder"]["hand"], json!("right"));
    assert_eq!(shape["nose_radius_from_code"], json!(true));
    assert_eq!(shape["tool"]["shape"]["nose_radius"], json!(0.8));
    // A right-hand outside tool: above the axis, towards +z from the
    // controlled point, which the nose touches.
    let insert = points(&shape["outline"], "insert");
    assert!(insert.len() > 4);
    assert!(insert.iter().all(|p| p.0 >= -0.01 && p.1 >= -0.01));
    assert!(insert.iter().any(|p| p.0.abs() < 0.01));
    assert!(insert.iter().any(|p| p.1.abs() < 0.01));
    assert!(
        points(&shape["outline"], "holder")
            .iter()
            .all(|p| p.0 > 0.0)
    );

    let kept = put(&mut doc, tool);
    assert_eq!(kept, shape["tool"], "the preview is the tool as kept");
    assert_eq!(kept["shape"]["nose_radius"], json!(0.8));
    assert_eq!(kept["insert"]["insert"], json!("CNMG 120408"));
    assert_eq!(kept["insert"]["holder"], json!("PCLNR 2525 M12"));
    assert_eq!(kept["insert"]["tip_angle"], json!(80.0));
    assert_eq!(kept["insert"]["approach_angle"], json!(95.0));
    assert_eq!(kept["insert"]["hand"], json!("right"));
    // Nothing typed over the codes.
    assert!(kept["insert"].get("typed").is_none(), "{kept}");
}

/// The boring bar S25S-PCLNR 12 is drawn on the bore's side; a boring bar's
/// code on a turning tool, an insert of another shape than the holder's, a
/// hand not the holder's, an unusable code and an insert on a grooving tool
/// are refused in words, by the preview as by `cam_tool_put`.
#[test]
fn a_boring_bar_is_drawn_in_the_bore_and_mismatches_are_refused() {
    let mut doc = lathe_doc();
    let bar = json!({"number": 3, "shape": {"kind": "boring"}, "diameter": "25 mm",
                     "insert": {"insert": "CNMG 120408", "holder": "S25S-PCLNR 12"}});
    let shape = shape_of(&doc, bar.clone());
    assert_eq!(shape["holder"]["bar"], json!(true));
    assert_eq!(shape["back_angle"], json!(5.0));
    for part in ["insert", "holder"] {
        let drawn = points(&shape["outline"], part);
        assert!(
            drawn.iter().all(|p| p.0 <= 0.01),
            "the bar's {part} below the axis: {drawn:?}"
        );
    }
    assert_eq!(put(&mut doc, bar)["shape"]["nose_radius"], json!(0.8));

    let refusals = [
        (
            json!({"kind": "turning"}),
            json!({"insert": "DNMG 150608", "holder": "PCLNR"}),
            "The holder PCLNR takes C inserts (rhombic 80), not D (rhombic 55).",
        ),
        (
            json!({"kind": "turning"}),
            json!({"insert": "CNMG 120408", "holder": "S25S-PCLNR 12"}),
            "S25S-PCLNR 12 is a boring bar's code: a turning tool needs a holder's code, like PCLNR 2525 M12.",
        ),
        (
            json!({"kind": "turning"}),
            json!({"holder": "PCLNR 2525 M12", "hand": "left"}),
            "The holder PCLNR 2525 M12 is right hand, not left hand.",
        ),
        (
            json!({"kind": "turning"}),
            json!({"insert": "CNMG 120499"}),
            "99 is not a corner radius code (00, 02, 04, 08, 12 and so on).",
        ),
        (
            json!({"kind": "turning"}),
            json!({"insert": "RCMT 1204M0"}),
            "Insert shape R is not supported yet (S, T, C, D, E, M and V are).",
        ),
        (
            json!({"kind": "grooving", "width": "3 mm"}),
            json!({"insert": "CNMG 120408"}),
            "Only turning tools and boring bars take an insert and a holder.",
        ),
    ];
    for (kind, insert, words) in refusals {
        let tool = json!({"number": 4, "shape": kind, "diameter": "25 mm", "insert": insert});
        assert_eq!(
            query_refused(&doc, json!({"query": "cam_tool_shape", "tool": tool})),
            words
        );
        assert_eq!(
            refused(
                &mut doc,
                json!({"cmd": "cam_tool_put", "machine": "lathe", "tool": tool})
            ),
            words
        );
    }
    assert!(kept_numbers(&doc).iter().all(|n| *n != 4));
}

fn kept_numbers(doc: &Document<MockKernel>) -> Vec<u64> {
    let machines = query(doc, json!({"query": "cam_machines"}));
    machines["machines"][0]["tools"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["number"].as_u64().unwrap())
        .collect()
}

/// A left-hand holder lies towards -z, a neutral one over the nose; angles
/// typed without codes work and are kept as typed.
#[test]
fn hands_are_drawn_their_way_and_angles_work_without_codes() {
    let mut doc = lathe_doc();
    let left = shape_of(
        &doc,
        json!({"number": 2, "shape": {"kind": "turning"}, "diameter": "25 mm",
               "insert": {"insert": "CNMG 120408", "holder": "PCLNL 2525 M12"}}),
    );
    assert_eq!(left["tool"]["insert"]["hand"], json!("left"));
    let insert = points(&left["outline"], "insert");
    assert!(insert.iter().all(|p| p.1 <= 0.01 && p.0 >= -0.01));
    assert!(
        points(&left["outline"], "holder")
            .iter()
            .all(|p| p.1 <= 0.01)
    );

    let neutral = shape_of(
        &doc,
        json!({"number": 2, "shape": {"kind": "turning"}, "diameter": "25 mm",
               "insert": {"insert": "VNMG 160404", "holder": "SVVBN 2525 M16"}}),
    );
    assert_eq!(neutral["tool"]["insert"]["hand"], json!("neutral"));
    assert_eq!(neutral["back_angle"], json!(72.5));
    let holder = points(&neutral["outline"], "holder");
    assert!(holder.iter().any(|p| p.1 < 0.0) && holder.iter().any(|p| p.1 > 0.0));

    // No codes: the angles alone, the nose radius as typed.
    let kept = put(
        &mut doc,
        json!({"number": 2, "shape": {"kind": "turning", "nose_radius": "0.4 mm"},
               "diameter": "25 mm",
               "insert": {"tip_angle": 55, "approach_angle": 93, "hand": "left"}}),
    );
    assert_eq!(kept["shape"]["nose_radius"], json!(0.4));
    assert_eq!(kept["insert"]["tip_angle"], json!(55.0));
    assert_eq!(kept["insert"]["approach_angle"], json!(93.0));
    assert_eq!(kept["insert"]["hand"], json!("left"));
    assert!(kept["insert"].get("insert").is_none());
    assert!(kept["insert"].get("holder").is_none());
    assert_eq!(kept["insert"]["typed"]["approach_angle"], json!(93.0));
    let again = shape_of(&doc, as_edited(&kept));
    assert_eq!(again["back_angle"], json!(32.0));
    assert_eq!(again["tool"], kept);
}

/// A typed nose radius stays when the insert code changes; without one the
/// code's corner radius follows the code.
#[test]
fn a_typed_nose_radius_survives_a_new_insert_code() {
    let mut doc = lathe_doc();
    let tool = |radius: Option<&str>, code: &str| {
        let mut shape = json!({"kind": "turning"});
        if let Some(radius) = radius {
            shape["nose_radius"] = json!(radius);
        }
        json!({"number": 2, "shape": shape, "diameter": "25 mm",
               "insert": {"insert": code, "holder": "PCLNR 2525 M12"}})
    };
    let kept = put(&mut doc, tool(Some("0.4 mm"), "CNMG 120408"));
    assert_eq!(kept["shape"]["nose_radius"], json!(0.4));
    assert_eq!(kept["insert"]["typed"]["nose_radius"], json!(0.4));
    // Edit Tool with the code changed: the typed radius is sent again.
    let mut edited = as_edited(&kept);
    assert_eq!(edited["shape"]["nose_radius"], json!("0.4 mm"));
    edited["insert"]["insert"] = json!("CNMG 120412");
    let kept = put(&mut doc, edited);
    assert_eq!(kept["insert"]["insert"], json!("CNMG 120412"));
    assert_eq!(kept["shape"]["nose_radius"], json!(0.4));

    // The code's radius follows the code.
    let kept = put(&mut doc, tool(None, "CNMG 120408"));
    assert_eq!(kept["shape"]["nose_radius"], json!(0.8));
    let mut edited = as_edited(&kept);
    assert!(edited["shape"].get("nose_radius").is_none());
    edited["insert"]["insert"] = json!("CNMG 120412");
    let shape = shape_of(&doc, edited.clone());
    assert_eq!(shape["nose_radius_from_code"], json!(true));
    assert_eq!(put(&mut doc, edited)["shape"]["nose_radius"], json!(1.2));
}

/// Angles typed alongside the codes are kept as typed through Edit Tool,
/// the design's file and the machine library, and leaving them out goes
/// back to the codes' values.
#[test]
fn typed_angles_survive_reopening_saving_and_the_library() {
    let project = unique_temp_dir("cam-insert-project");
    let mut doc = lathe_doc();
    let kept = put(
        &mut doc,
        json!({"number": 2, "shape": {"kind": "turning"}, "diameter": "25 mm",
               "insert": {"insert": "CNMG 120408", "holder": "PCLNR 2525 M12",
                          "approach_angle": 93, "tip_angle": 78}}),
    );
    assert_eq!(kept["insert"]["approach_angle"], json!(93.0));
    assert_eq!(kept["insert"]["tip_angle"], json!(78.0));
    assert_eq!(
        kept["insert"]["typed"],
        json!({"tip_angle": 78.0, "approach_angle": 93.0})
    );
    assert_eq!(kept["shape"]["nose_radius"], json!(0.8));

    // Saved and opened again: the same tool; Edit Tool and OK keep it.
    let saved = doc.to_json();
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(kept_tool(&reopened), kept);
    let again = put(&mut reopened, as_edited(&kept));
    assert_eq!(again, kept);
    let file: Value = serde_json::from_str(&reopened.to_json()).unwrap();
    assert_eq!(file["cam"]["machines"][0]["tools"]["tools"][1], kept);

    // To the project's library and into another design.
    let folder = project.display().to_string();
    command(
        &mut reopened,
        json!({"cmd": "cam_machine_to_library", "machine": "lathe", "library": "project",
               "project": folder}),
    );
    let mut other = Document::new(MockKernel::default());
    command(
        &mut other,
        json!({"cmd": "cam_machine_from_library", "library": "project", "id": "lathe",
               "project": folder}),
    );
    assert_eq!(kept_tool(&other), kept);
    assert_eq!(put(&mut other, as_edited(&kept)), kept);
    remove_temp(&project);

    // The typed angles left out: the codes' values again.
    let mut defaults = as_edited(&kept);
    defaults["insert"] = json!({"insert": "CNMG 120408", "holder": "PCLNR 2525 M12"});
    let back = put(&mut other, defaults);
    assert_eq!(back["insert"]["approach_angle"], json!(95.0));
    assert_eq!(back["insert"]["tip_angle"], json!(80.0));
    assert!(back["insert"].get("typed").is_none());
}

fn kept_tool(doc: &Document<MockKernel>) -> Value {
    kept(doc, 2)
}

/// Tools and designs without an insert read, show and are kept as before:
/// no `insert` in the tool, and an empty one is none.
#[test]
fn tools_without_an_insert_are_as_before() {
    let mut doc = lathe_doc();
    let old = kept(&doc, 1);
    assert!(old.get("insert").is_none(), "{old}");
    assert_eq!(old["shape"], json!({"kind": "turning", "nose_radius": 0.4}));
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let in_file = &file["cam"]["machines"][0]["tools"]["tools"][0];
    assert_eq!(in_file["number"], json!(1));
    assert!(in_file.get("insert").is_none(), "{in_file}");
    let reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(reopened.to_json(), saved);

    let plain = put(
        &mut doc,
        json!({"number": 2, "shape": {"kind": "turning", "nose_radius": "0.4 mm"},
               "diameter": "20 mm", "insert": {"insert": " ", "holder": ""}}),
    );
    assert!(plain.get("insert").is_none(), "{plain}");
    let shape = shape_of(
        &doc,
        json!({"number": 2, "shape": {"kind": "boring"}, "diameter": "20 mm"}),
    );
    assert!(shape.get("outline").is_none());
    assert_eq!(shape["tool"]["shape"]["nose_radius"], json!(0.0));
}
