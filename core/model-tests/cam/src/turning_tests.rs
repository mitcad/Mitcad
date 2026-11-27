// SPDX-License-Identifier: MIT
//! Turning on a lathe through the JSON api (mitcad#1869) on the mock
//! kernel: a block 40 x 30 x 10 the mock cuts as a box, turned about the
//! design's Z through its middle (a part 40 mm across, 10 long), with a
//! lathe on the Fanuc turning dialect and its turret's tools. Facing,
//! roughing and finishing as G71 and G70, a thread as G76 at a constant
//! speed, parting off; the program read back; the turned stock simulated;
//! milling and turning kept apart in words; the stock as a bar of a given
//! diameter and length (mitcad#2550).

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

/// LinuxCNC on a lathe (mitcad#2425): the lathe's controller
/// `linuxcnc_turning` posts every operation longhand (diameter mode, feeds
/// per revolution, the spindle's limit with G96, the tool change with
/// G43, threads as G33 with K), the canned cycles the setup asks for
/// written as moves, read back as LinuxCNC reads it; the milling dialect
/// `linuxcnc` still does not post a lathe setup, and the turning one is
/// refused on a mill.
#[test]
fn a_lathe_on_linuxcnc_posts_longhand() {
    let mut doc = turned();
    let mut machine = lathe();
    machine["machine"]["controller"] = json!("linuxcnc_turning");
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": machine}),
    );
    let post = query(&doc, json!({"query": "cam_post", "setup": 1}));
    assert_eq!(post["dialect"], json!("linuxcnc_turning"));
    assert_eq!(post["label"], json!("LinuxCNC (turning)"));
    assert_eq!(post["extension"], json!("ngc"));
    assert_eq!(post["cycles"], json!([]));
    let text = post["text"].as_str().unwrap();
    assert!(text.starts_with("%\n(WARNING: "), "{text}");
    assert!(!text.contains("O2001"), "{text}");
    assert!(
        text.contains("G21 G18 G7 G90 G91.1 G40 G80 G95\nG64 P0.01\nG54\n"),
        "{text}"
    );
    assert!(
        text.contains("(TURNING CYCLES ARE WRITTEN AS MOVES FOR THIS CONTROL)\n"),
        "{text}"
    );
    assert!(text.contains("T1 M6 G43\nG96 D3000 S200 M3\n"), "{text}");
    assert!(text.contains("T3 M6 G43\nG97 S796 M3\n"), "{text}");
    assert!(text.contains("G33 Z") && text.contains(" K1.5\n"), "{text}");
    for code in ["G71", "G70", "G76", "G50", "G99", "G32"] {
        assert!(!text.contains(code), "{code} in {text}");
    }
    assert!(text.ends_with("M2\n%\n"), "{text}");

    // The milling dialect of the same control does not post for a lathe.
    let refused_post = doc
        .query(&json!({"query": "cam_post", "setup": 1, "dialect": "linuxcnc"}).to_string())
        .unwrap_err()
        .to_string();
    assert_eq!(
        refused_post,
        "MitCad does not write lathe G-code for that controller."
    );
    // Both turning dialects are listed for lathes.
    let dialects = query(&doc, json!({"query": "cam_dialects"}));
    let lathe_ids: Vec<&str> = dialects["dialects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["lathe"] == json!(true))
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(lathe_ids, ["fanuc_turning", "linuxcnc_turning"]);
    // A mill on the turning dialect says which kind it writes for.
    assert_eq!(
        refused(
            &mut doc,
            json!({"cmd": "cam_machine_put", "machine": {
                "id": "router",
                "machine": {"name": "Router", "controller": "linuxcnc_turning",
                            "travel": {"min": [0, 0, -100], "max": [500, 500, 0]},
                            "spindle": {"min_rpm": 8000, "max_rpm": 24000,
                                        "nose_diameter": 50, "nose_length": 80},
                            "max_feed": 5000, "rapid_feed": 6000},
                "tools": {"tools": []}}})
        ),
        "The machine Router is a milling machine; LinuxCNC (turning) writes for lathes."
    );
}

/// The first setup's work as `cam_setups` gives it: the frame, the stock
/// box the view draws (`stock_min`, `stock_max`) and the turned part.
fn first_work(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "cam_setups"}))["setups"][0]["work"].clone()
}

/// The first setup's lathe values replaced by `lathe`.
fn set_lathe(doc: &mut Document<MockKernel>, lathe: Value) {
    command(
        doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"lathe": lathe}}),
    );
}

/// The stock's radius, face and back of a lathe setup's work.
fn bar_of(work: &Value) -> (f64, f64, f64) {
    let turning = &work["turning"];
    assert!(turning.is_object(), "{work}");
    (
        turning["part"]["stock_radius"].as_f64().unwrap(),
        turning["part"]["stock_face"].as_f64().unwrap(),
        turning["stock_back"].as_f64().unwrap(),
    )
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// A lathe setup's bar of a given size (mitcad#2550): a Ø50 bar 30 mm
/// long from its face, 1 mm in front of the part's face, is 25 in radius
/// from Z 1 to Z -29, in the stock the view draws, in the turning and in
/// the simulated stock; each size left out keeps its margin; a bar too
/// thin or too short is refused in words, and so is a bore as wide as
/// the bar.
#[test]
fn a_lathe_setup_takes_a_bar_of_a_given_size() {
    let mut doc = turned();
    set_lathe(
        &mut doc,
        json!({"program_number": 2001, "stock_diameter": 50, "stock_length": 30}),
    );
    let work = first_work(&doc);
    let turning = &work["turning"];
    assert_eq!(turning["part"]["stock_radius"], json!(25.0), "{turning}");
    assert_eq!(turning["part"]["stock_face"], json!(1.0));
    assert_eq!(turning["stock_back"], json!(-29.0));
    // The stock the view draws is the same bar.
    assert_eq!(work["stock_min"], json!([-25.0, -25.0, -29.0]));
    assert_eq!(work["stock_max"], json!([25.0, 25.0, 1.0]));
    // The turning works on it: every operation computed, the stock left
    // behind the part the bar's 25 mm.
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    for op in generated["setups"][0]["operations"].as_array().unwrap() {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
    }
    let sim = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let stock = &sim["stock"];
    let zs: Vec<f64> = serde_json::from_value(stock["z"].clone()).unwrap();
    let outer: Vec<f64> = serde_json::from_value(stock["outer"].clone()).unwrap();
    let i = zs
        .iter()
        .position(|z| (z + 20.0).abs() < 0.026)
        .expect("a node at Z -20");
    assert!((outer[i] - 25.0).abs() < 1e-9, "{}", outer[i]);
    // The simulated stock reaches past the bar's back.
    assert!(zs.iter().copied().fold(f64::INFINITY, f64::min) < -29.0);

    // The diameter alone keeps the bottom margin (5 mm behind the part's
    // back at Z -10); the length alone the side margin (2 mm on 20).
    set_lathe(&mut doc, json!({"stock_diameter": 50}));
    assert_eq!(bar_of(&first_work(&doc)), (25.0, 1.0, -15.0));
    set_lathe(&mut doc, json!({"stock_length": 30}));
    assert_eq!(bar_of(&first_work(&doc)), (22.0, 1.0, -29.0));

    // A bar thinner than the part, or shorter than the part and the stock
    // in front of its face, is refused in words; a bar just the part's
    // size is taken.
    for (lathe, message) in [
        (
            json!({"stock_diameter": 30}),
            "The bar (30 mm) is thinner than the part (40 mm).",
        ),
        (
            json!({"stock_length": 5}),
            "The bar (5 mm) is shorter than the part and the stock in front of its face (11 mm).",
        ),
        // The bore and the bar's diameter meet in the stock's own check.
        (
            json!({"stock_diameter": 50, "stock_bore": 50}),
            "The stock's bore must be narrower than the stock.",
        ),
    ] {
        set_lathe(&mut doc, lathe.clone());
        assert_eq!(first_work(&doc)["error"], json!(message), "{lathe}");
    }
    set_lathe(&mut doc, json!({"stock_diameter": 40, "stock_length": 11}));
    assert_eq!(bar_of(&first_work(&doc)), (20.0, 1.0, -10.0));
    // A tube: the bar with its bore.
    set_lathe(&mut doc, json!({"stock_diameter": 50, "stock_bore": 20}));
    let work = first_work(&doc);
    assert_eq!(work["turning"]["part"]["stock_inner_radius"], json!(10.0));
    assert_eq!(bar_of(&work), (25.0, 1.0, -15.0));

    // Left out again: the margins as before.
    set_lathe(&mut doc, json!({"program_number": 2001}));
    assert_eq!(bar_of(&first_work(&doc)), (22.0, 1.0, -15.0));
}

/// The bar's sizes are the design's values (mitcad#2550): units and
/// parameters, kept through a part edit (the bar's back stays where the
/// length puts it while the margin's would follow the part), followed
/// by a renamed parameter, keeping it from being deleted, saved and
/// opened again as they were typed; a setup with margins only writes
/// neither.
#[test]
fn a_bar_s_sizes_take_units_and_parameters_and_are_saved() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "h", "value": 10},
            {"cmd": "add_parameter", "name": "bar_l", "value": 30},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "h"}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": lathe()}),
    );
    // Margins only: the file has no bar.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {
            "stock": {"kind": "around_bodies", "side": 2, "top": 1, "bottom": 5},
            "lathe": {"program_number": 2001}
        }}),
    );
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let lathe_file = &file["cam"]["setups"][0]["lathe"];
    assert!(lathe_file.get("stock_diameter").is_none(), "{lathe_file}");
    assert!(lathe_file.get("stock_length").is_none(), "{lathe_file}");
    assert_eq!(bar_of(&first_work(&doc)), (22.0, 1.0, -15.0));

    // A 2 inch bar, as long as the parameter.
    set_lathe(
        &mut doc,
        json!({"stock_diameter": "2 in", "stock_length": "bar_l"}),
    );
    let (radius, face, back) = bar_of(&first_work(&doc));
    assert!(near(radius, 25.4) && near(face, 1.0) && near(back, -29.0));

    // A longer part: the bar's back stays, the view's stock with it.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "h", "value": 12}),
    );
    let work = first_work(&doc);
    let (_, face, back) = bar_of(&work);
    assert!(near(face, 1.0) && near(back, -29.0), "{work}");
    assert!(near(work["stock_min"][2].as_f64().unwrap(), back), "{work}");
    assert!(near(work["stock_max"][2].as_f64().unwrap(), face), "{work}");
    // The parameter shortened below the part: refused in words.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "bar_l", "value": 12}),
    );
    assert_eq!(
        first_work(&doc)["error"],
        json!(
            "The bar (12 mm) is shorter than the part and the stock in front of its face (13 mm)."
        )
    );
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "bar_l", "value": 40}),
    );
    assert!(near(bar_of(&first_work(&doc)).2, -39.0));

    // The setup uses the parameter: it is not deleted, and follows its
    // new name.
    let message = refused(
        &mut doc,
        json!({"cmd": "delete_parameter", "name": "bar_l"}),
    );
    assert!(message.contains("the CAM setup Setup 1"), "{message}");
    command(
        &mut doc,
        json!({"cmd": "rename_parameter", "name": "bar_l", "new_name": "bar_length"}),
    );

    // Saved as typed and opened again with the same bar.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let lathe_file = &file["cam"]["setups"][0]["lathe"];
    assert_eq!(lathe_file["stock_diameter"], json!("2 in"), "{lathe_file}");
    assert_eq!(
        lathe_file["stock_length"],
        json!("bar_length"),
        "{lathe_file}"
    );
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(reopened.to_json(), saved);
    reopened.recompute();
    let (radius, face, back) = bar_of(&first_work(&reopened));
    assert!(near(radius, 25.4) && near(face, 1.0) && near(back, -39.0));
}

/// The bar's sizes count with the stock around the bodies only
/// (mitcad#2550): a stock body keeps its own size with them set, and
/// switching back to the margins brings the bar back; a milling setup's
/// stock does not read them.
#[test]
fn a_bar_s_sizes_leave_a_stock_body_and_a_mill_alone() {
    let mut doc = turned();
    // A 60 x 60 x 20 block about the part's axis as the stock body (F4.b0).
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-10, -15], "width": 60, "height": 60},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {
            "bodies": ["F2.b0"], "stock": {"kind": "body", "body": "F4.b0"}}}),
    );
    let body = bar_of(&first_work(&doc));
    set_lathe(&mut doc, json!({"stock_diameter": 80, "stock_length": 100}));
    assert_eq!(bar_of(&first_work(&doc)), body);
    // Back to the margins: the bar counts again.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {
            "stock": {"kind": "around_bodies", "side": 2, "top": 1, "bottom": 5}}}),
    );
    assert_eq!(bar_of(&first_work(&doc)), (40.0, 1.0, -99.0));

    // A milling setup with the same values: its stock box as without.
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
    let mill = command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"machine": "router", "bodies": ["F2.b0"],
               "stock": {"kind": "around_bodies", "side": 2, "top": 1, "bottom": 5}}}),
    )["setup"]
        .clone();
    let mill_work = |doc: &Document<MockKernel>| {
        let setups = query(doc, json!({"query": "cam_setups"}));
        let work = setups["setups"][1]["work"].clone();
        (work["stock_min"].clone(), work["stock_max"].clone())
    };
    let before = mill_work(&doc);
    assert!(before.0.is_array(), "{before:?}");
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": mill, "changes": {
            "lathe": {"stock_diameter": 80, "stock_length": 100}}}),
    );
    assert_eq!(mill_work(&doc), before);
}

/// A design whose lathe names a controller MitCad does not write for (made
/// in another build, mitcad#2531) opens with its setup and toolpaths: the
/// controller is kept as it was, the toolpaths generate, and only the post
/// refuses in words, as it does for a setup's dialect of another build. A
/// machine put in, or a dialect set, is still checked.
#[test]
fn a_controller_of_another_build_opens_and_only_the_post_refuses() {
    let mut doc = turned();
    command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    file["cam"]["machines"][0]["machine"]["controller"] = json!("another_build_turning");
    let saved = file.to_string();
    let mut reopened = Document::from_json(&saved, MockKernel::default())
        .unwrap_or_else(|e| panic!("the design does not open: {e}"));
    reopened.recompute();
    let setups = query(&reopened, json!({"query": "cam_setups"}));
    assert_eq!(
        setups["machines"][0]["controller"],
        json!("another_build_turning")
    );
    assert_eq!(setups["setups"][0]["turning"], json!(true));
    assert_eq!(
        setups["setups"][0]["effective_dialect"],
        json!("another_build_turning")
    );
    let generated = command(&mut reopened, json!({"cmd": "cam_generate", "setup": 1}));
    let ops = generated["setups"][0]["operations"].as_array().unwrap();
    assert_eq!(ops.len(), 5);
    for op in ops {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
    }
    let refused_post = reopened
        .query(&json!({"query": "cam_post", "setup": 1}).to_string())
        .unwrap_err()
        .to_string();
    assert_eq!(
        refused_post,
        "MitCad does not write lathe G-code for that controller."
    );
    // A dialect MitCad writes for, chosen for the post, writes.
    let post = query(
        &reopened,
        json!({"query": "cam_post", "setup": 1, "dialect": "fanuc_turning"}),
    );
    assert_eq!(post["dialect"], json!("fanuc_turning"));
    // Saved again, the controller stays as the file had it.
    let resaved: Value = serde_json::from_str(&reopened.to_json()).unwrap();
    assert_eq!(
        resaved["cam"]["machines"][0]["machine"]["controller"],
        json!("another_build_turning")
    );
    // Other edits of the setup are taken; a dialect MitCad does not write
    // for is refused when it is set, and the machine when it is put in.
    command(
        &mut reopened,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"work_offset": 2}}),
    );
    assert_eq!(
        refused(
            &mut reopened,
            json!({"cmd": "cam_setup_edit", "setup": 1,
                   "changes": {"dialect": "another_build_turning"}})
        ),
        "MitCad does not write G-code for that controller."
    );
    let mut machine = lathe();
    machine["machine"]["controller"] = json!("another_build_turning");
    assert_eq!(
        refused(
            &mut reopened,
            json!({"cmd": "cam_machine_put", "machine": machine})
        ),
        "The machine Lathe has a controller MitCad does not write for."
    );

    // A setup's dialect of another build opens too, and its post refuses.
    let mut dialect: Value = serde_json::from_str(&doc.to_json()).unwrap();
    dialect["cam"]["setups"][0]["dialect"] = json!("another_build_turning");
    let mut reopened = Document::from_json(&dialect.to_string(), MockKernel::default())
        .unwrap_or_else(|e| panic!("the design does not open: {e}"));
    reopened.recompute();
    let refused_post = reopened
        .query(&json!({"query": "cam_post", "setup": 1}).to_string())
        .unwrap_err()
        .to_string();
    assert_eq!(
        refused_post,
        "MitCad does not write lathe G-code for that controller."
    );
}
