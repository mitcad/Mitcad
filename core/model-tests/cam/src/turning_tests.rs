// SPDX-License-Identifier: MIT
//! Turning on a lathe through the JSON api (mitcad#1869) on the mock
//! kernel: a block 40 x 30 x 10 the mock cuts as a box, turned about the
//! design's Z through its middle (a part 40 mm across, 10 long), with a
//! lathe on the Fanuc turning dialect and its turret's tools. Facing,
//! roughing and finishing as G71 and G70, a thread as G76 at a constant
//! speed, parting off; the program read back; the turned stock simulated;
//! milling and turning kept apart in words.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model::testing::temp::{remove_temp, unique_temp_dir};
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

/// A two-axis lathe on a Fanuc control with four tools on its turret.
fn lathe() -> Value {
    json!({
        "id": "lathe",
        "machine": {
            "name": "Lathe",
            "controller": "fanuc_turning",
            "travel": {"min": [-5, 0, -300], "max": [150, 0, 200]},
            "spindle": {"min_rpm": 50, "max_rpm": 3000, "nose_diameter": 0, "nose_length": 0},
            "max_feed": 5000,
            "rapid_feed": 20000,
            "lathe": {"turret_stations": 8, "chuck_diameter": 160, "chuck_length": 50}
        },
        "tools": {"tools": [
            {"number": 1, "name": "Turning", "shape": {"kind": "turning", "nose_radius": 0.4},
             "diameter": 20, "flute_length": 3, "length": 30},
            {"number": 2, "name": "Parting", "shape": {"kind": "parting", "width": 3},
             "diameter": 20, "flute_length": 25, "length": 30},
            {"number": 3, "name": "Thread", "shape": {"kind": "threading", "angle": 60},
             "diameter": 16, "flute_length": 2, "length": 30},
            {"number": 4, "name": "Groove", "shape": {"kind": "grooving", "width": 2},
             "diameter": 16, "flute_length": 3, "length": 30}
        ]}
    })
}

/// The block on the lathe: the stock 2 mm over its radius, 1 mm to face
/// and 5 behind it; cycles on.
fn turned() -> Document<MockKernel> {
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
        json!({"cmd": "cam_machine_put", "machine": lathe()}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {
            "stock": {"kind": "around_bodies", "side": 2, "top": 1, "bottom": 5},
            "post": {"canned_cycles": true},
            "lathe": {"program_number": 2001}
        }}),
    );
    for operation in [
        json!({"kind": {"type": "turn_face"}, "tool": 1}),
        json!({"kind": {"type": "turn_rough", "side": "outside"}, "tool": 1, "stock_to_leave": 0.2}),
        json!({"kind": {"type": "turn_finish", "side": "outside"}, "tool": 1}),
        json!({"kind": {"type": "turn_thread", "pitch": 1.5, "diameter": 40, "z_start": 0, "z_end": -8},
               "tool": 3}),
        json!({"kind": {"type": "turn_part_off"}, "tool": 2, "turning": {"peck": 3}}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    doc
}

#[test]
fn a_lathe_setup_turns_posts_checks_and_simulates() {
    let mut doc = turned();
    let setups = query(&doc, json!({"query": "cam_setups"}));
    let setup = &setups["setups"][0];
    assert_eq!(setup["turning"], json!(true));
    let names: Vec<&str> = setup["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "Turn Facing 1",
            "Turn Roughing 1",
            "Turn Finishing 1",
            "Threading 1",
            "Parting Off 1"
        ]
    );
    // The part: 20 radius from its face at Z 0 to Z -10; the stock 22.
    let turning = &setup["work"]["turning"];
    assert_eq!(turning["part"]["stock_radius"], json!(22.0));
    assert_eq!(turning["part"]["stock_face"], json!(1.0));
    assert_eq!(turning["stock_back"], json!(-15.0));

    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let ops = generated["setups"][0]["operations"].as_array().unwrap();
    for op in ops {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
    }

    let post = query(&doc, json!({"query": "cam_post", "setup": 1}));
    assert_eq!(post["dialect"], json!("fanuc_turning"));
    let text = post["text"].as_str().unwrap();
    assert!(text.starts_with("%\nO2001 (SETUP 1)\n"), "{text}");
    assert!(text.contains("G21 G18 G40 G99\nG54\n"), "{text}");
    assert!(text.contains("T0101\nG50 S3000\nG96 S200 M3\n"), "{text}");
    assert!(text.contains("G71 U2. R0.5\n"), "{text}");
    assert!(text.contains(" U0.4 W0.1 F0.25\n"), "{text}");
    assert!(text.contains("G70 P"), "{text}");
    // The thread at a constant speed: 100 m/min on 40 mm.
    assert!(text.contains("T0303\nG97 S796 M3\n"), "{text}");
    assert!(text.contains("G76 P010060 "), "{text}");
    let kinds: Vec<&str> = post["cycles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c[0].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["G71", "G70", "G76"]);

    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    assert_eq!(paths["turning"], json!(true));
    for op in paths["operations"].as_array().unwrap() {
        assert!(
            !op["toolpath"]["moves"].as_array().unwrap().is_empty(),
            "{op}"
        );
        // In the section's plane.
        for m in op["toolpath"]["moves"].as_array().unwrap() {
            assert_eq!(m["to"][1], json!(0.0), "{m}");
        }
    }

    let sim = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    assert_eq!(sim["turning"], json!(true));
    assert!(sim["rapid_cuts"].as_array().unwrap().is_empty(), "{sim}");
    assert!(sim["gouges"].as_array().unwrap().is_empty(), "{sim}");
    assert!(sim["left"].as_f64().unwrap() < 0.01, "{}", sim["left"]);
    // The section the view revolves: per node along Z its two radii; the
    // part turned to 20 mm, nothing left where it was parted off, the
    // stock's 22 mm behind the blade.
    let stock = &sim["stock"];
    let zs: Vec<f64> = serde_json::from_value(stock["z"].clone()).unwrap();
    let inner: Vec<f64> = serde_json::from_value(stock["inner"].clone()).unwrap();
    let outer: Vec<f64> = serde_json::from_value(stock["outer"].clone()).unwrap();
    assert_eq!(zs.len(), inner.len());
    assert_eq!(zs.len(), outer.len());
    let at = |z: f64| {
        let i = zs
            .iter()
            .position(|v| (v - z).abs() < 0.026)
            .unwrap_or_else(|| panic!("no node at {z}"));
        (inner[i], outer[i])
    };
    let (i, o) = at(-5.0);
    assert!(i == 0.0 && (o - 20.0).abs() < 0.01, "{i} {o}");
    let (i, o) = at(-11.5);
    assert!(o <= i + 1e-9, "{i} {o}");
    let (_, o) = at(-20.0);
    assert!((o - 22.0).abs() < 1e-9, "{o}");
}

#[test]
fn turning_and_milling_keep_apart_in_words() {
    let mut doc = turned();
    let cases = [
        (
            json!({"cmd": "cam_operation_add", "setup": 1,
                   "operation": {"kind": {"type": "pocket"}, "tool": 1}}),
            "The setup's machine is a lathe: give it turning operations.",
        ),
        (
            json!({"cmd": "cam_tool_put", "machine": "lathe",
                   "tool": {"number": 5, "shape": {"kind": "flat"}, "diameter": 6}}),
            "The lathe Lathe takes lathe tools only.",
        ),
        (
            json!({"cmd": "cam_tool_put", "machine": "lathe",
                   "tool": {"number": 120, "shape": {"kind": "turning"}, "diameter": 20}}),
            "Tool 120 is a lathe tool: its number is its turret station, 1 to 99.",
        ),
        (
            json!({"cmd": "cam_operation_add", "setup": 1,
                   "operation": {"kind": {"type": "turn_face"}, "tool": 1,
                                 "inputs": [{"kind": "stock"}]}}),
            "A turning operation turns the setup's bodies; grooving and threading also take the face they cut.",
        ),
    ];
    for (command, message) in cases {
        assert_eq!(refused(&mut doc, command.clone()), message, "{command}");
    }
    // A groove where the part has none says so.
    command(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
            "kind": {"type": "turn_groove", "z_from": -3, "z_to": -6}, "tool": 4}}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let groove = &generated["setups"][0]["operations"][5];
    assert_eq!(groove["state"], json!("failed"));
    assert_eq!(groove["error"], json!("The part has no groove there."));
    // Turning on a mill is refused.
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": {
            "id": "router",
            "machine": {"name": "Router", "controller": "grbl",
                        "travel": {"min": [0, 0, -100], "max": [500, 500, 0]},
                        "spindle": {"min_rpm": 8000, "max_rpm": 24000,
                                    "nose_diameter": 50, "nose_length": 80},
                        "max_feed": 5000, "rapid_feed": 6000},
            "tools": {"tools": [{"number": 1, "shape": {"kind": "flat"}, "diameter": 6,
                                 "flute_length": 18, "length": 25}]}}}),
    );
    // Setups and operations share their ids: the new setup's is returned.
    let mill = command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"machine": "router"}}),
    )["setup"]
        .clone();
    assert_eq!(
        refused(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": mill,
                   "operation": {"kind": {"type": "turn_face"}, "tool": 1}})
        ),
        "Turning operations need a lathe as the setup's machine."
    );
    let dialects = query(&doc, json!({"query": "cam_dialects"}));
    assert!(
        dialects["dialects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["id"] == json!("fanuc_turning") && d["lathe"] == json!(true))
    );
}

/// A controller of the other kind of machine is refused with the kind it
/// writes for (mitcad#2119): a turning dialect on a mill, a milling one on
/// a lathe; a controller MitCad does not know keeps its plain refusal.
#[test]
fn a_controller_of_the_other_kind_says_which_kind_it_is_for() {
    let mut doc = Document::new(MockKernel::default());
    let mill = |controller: &str| {
        json!({"cmd": "cam_machine_put", "machine": {
            "id": "router",
            "machine": {"name": "Router", "controller": controller,
                        "travel": {"min": [0, 0, -100], "max": [500, 500, 0]},
                        "spindle": {"min_rpm": 8000, "max_rpm": 24000,
                                    "nose_diameter": 50, "nose_length": 80},
                        "max_feed": 5000, "rapid_feed": 6000},
            "tools": {"tools": []}}})
    };
    let turning = |controller: &str| {
        let mut machine = lathe();
        machine["machine"]["controller"] = json!(controller);
        json!({"cmd": "cam_machine_put", "machine": machine})
    };
    assert_eq!(
        refused(&mut doc, mill("fanuc_turning")),
        "The machine Router is a milling machine; Fanuc (turning) writes for lathes."
    );
    assert_eq!(
        refused(&mut doc, turning("grbl")),
        "The lathe Lathe needs a lathe controller; grbl writes for milling machines."
    );
    assert_eq!(
        refused(&mut doc, mill("no_such_controller")),
        "The machine Router has a controller MitCad does not write for."
    );
    assert_eq!(
        refused(&mut doc, turning("no_such_controller")),
        "The machine Lathe has a controller MitCad does not write for."
    );
    // Each kind's own dialect is taken.
    command(&mut doc, mill("grbl"));
    command(&mut doc, turning("fanuc_turning"));
}

/// A lathe as Machines and Tools… makes it (mitcad#2230): New Machine's
/// lathe defaults, a turning tool and a parting blade typed with their
/// units, Edit Machine's turret and chuck; saved to the project's library
/// and imported into another design with its turret and tools.
#[test]
fn a_lathe_made_in_the_dialog_goes_to_the_library_and_back() {
    let project = unique_temp_dir("cam-lathe-project");
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": {"id": "lathe", "machine": {
            "name": "Lathe", "controller": "fanuc_turning",
            "travel": {"min": [-5, 0, -300], "max": [150, 0, 200]},
            "spindle": {"min_rpm": 50, "max_rpm": 3000, "spin_up_seconds": 2,
                        "nose_diameter": 0, "nose_length": 0},
            "max_feed": 5000, "rapid_feed": 20000,
            "lathe": {"turret_stations": 12, "chuck_diameter": 160, "chuck_length": 50}}}}),
    );
    let turning = command(
        &mut doc,
        json!({"cmd": "cam_tool_put", "machine": "lathe", "tool": {
            "number": 1, "shape": {"kind": "turning", "nose_radius": "0.4 mm"},
            "diameter": "20 mm", "flute_length": "3 mm", "length": "30 mm"}}),
    );
    assert_eq!(turning["tool"]["name"], json!("20 mm turning tool"));
    assert_eq!(turning["tool"]["shape"]["nose_radius"], json!(0.4));
    let parting = command(
        &mut doc,
        json!({"cmd": "cam_tool_put", "machine": "lathe", "tool": {
            "number": 2, "shape": {"kind": "parting", "width": "3 mm"}, "diameter": "20 mm"}}),
    );
    assert_eq!(parting["tool"]["name"], json!("20 mm parting tool"));
    assert_eq!(parting["tool"]["flute_length"], json!(25.0));
    // A milling tool is refused on the lathe, in words.
    assert_eq!(
        refused(
            &mut doc,
            json!({"cmd": "cam_tool_put", "machine": "lathe", "tool": {
                "number": 3, "shape": {"kind": "flat"}, "diameter": "6 mm"}})
        ),
        "The lathe Lathe takes lathe tools only."
    );
    // Edit Machine's turret and chuck.
    let mut machine = query(&doc, json!({"query": "cam_machines"}))["machines"][0].clone();
    machine.as_object_mut().unwrap().remove("used_by");
    machine["machine"]["lathe"]["turret_stations"] = json!(8);
    machine["machine"]["lathe"]["chuck_diameter"] = json!(200);
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": machine}),
    );

    let folder = project.display().to_string();
    let saved = command(
        &mut doc,
        json!({"cmd": "cam_machine_to_library", "machine": "lathe", "library": "project",
               "project": folder}),
    );
    assert_eq!(saved["version"], json!(1));
    let mut other = Document::new(MockKernel::default());
    command(
        &mut other,
        json!({"cmd": "cam_machine_from_library", "library": "project", "id": "lathe",
               "project": folder}),
    );
    let machines = query(&other, json!({"query": "cam_machines"}));
    let copied = &machines["machines"][0];
    assert_eq!(copied["machine"]["controller"], json!("fanuc_turning"));
    assert_eq!(copied["machine"]["lathe"]["turret_stations"], json!(8));
    assert_eq!(copied["machine"]["lathe"]["chuck_diameter"], json!(200.0));
    assert_eq!(copied["machine"]["lathe"]["chuck_length"], json!(50.0));
    let kinds: Vec<&str> = copied["tools"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["shape"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["turning", "parting"]);
    remove_temp(&project);
}
