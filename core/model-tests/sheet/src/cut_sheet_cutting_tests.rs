// SPDX-License-Identifier: MIT
//! Nesting sheet-metal Flat Patterns on a cut sheet and cutting them with
//! a plasma torch on grbl 1.1 (mitcad#2553), through the JSON api on the
//! mock kernel: two sheet-metal components (a sheet Face each, no bends),
//! one placed with a copy, nested with a spacing and a kerf; the cutting
//! paths moved half the kerf off the parts once; what keeps a layout from
//! being cut (overlaps, a part off the sheet, parts closer together than
//! the kerf) in words with the parts it concerns, the program refused
//! then; the torch's settings kept with the cut sheet and undone; the
//! paths drawn over the sheet. Holes and bend lines through OCCT:
//! `tools/cli/tests/cut_sheet_cutting.json`; the order, the lead-ins and
//! the G-code: `core/cam/src/sheet_cut.rs`; the program through grbl's own
//! code: `core/cam/tests/sim/sheet_cut_sim.rs`.

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::BodyUid;
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|text| serde_json::from_str(&text).unwrap_or(Value::Null))
        .map_err(|e| e.to_string())
}

/// A new component beside the root's others, active.
fn component(doc: &mut Document<MockKernel>, name: &str) {
    command(doc, json!({"cmd": "activate_component", "component": "C0"})).unwrap();
    command(doc, json!({"cmd": "create_component", "name": name})).unwrap();
}

/// A sheet Face of `w` × `h` (1 mm thick) in the active component.
fn sheet_face(doc: &mut Document<MockKernel>, w: f64, h: f64) -> BodyUid {
    let rules = json!({"type":"sheet_rules","thickness":1,"bend_radius":2,
        "allowance":{"type":"k_factor","k":0.4},"reference_side":"front",
        "bend_relief":{"type":"round","width":1,"depth":3},
        "corner_relief":{"type":"rectangular","width":1,"depth":3}});
    let rules = doc.add_feature(&def(rules), None).unwrap().uid;
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(w), &num(h))
        .unwrap()
        .region;
    let face =
        json!({"type":"sheet_face","rules":rules,"profiles":[{"sketch":sketch,"region":region}]});
    BodyUid::new(doc.add_feature(&def(face), None).unwrap().uid, 0)
}

/// A Bracket (40 × 20) and a Plate (30 × 10) of sheet metal; a cut sheet
/// 200 × 100 with a kerf of 1.5 holding the Bracket once and the Plate
/// twice, nested 6 mm apart: parts 1 (Bracket), 2 and 3 (Plate).
fn nested() -> (Document<MockKernel>, Value) {
    let mut doc = Document::new(MockKernel::default());
    component(&mut doc, "Bracket");
    let bracket = sheet_face(&mut doc, 40.0, 20.0);
    component(&mut doc, "Plate");
    let plate = sheet_face(&mut doc, 30.0, 10.0);
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    )
    .unwrap();
    let added = command(
        &mut doc,
        json!({"cmd": "add_cut_sheet", "flat": true, "width": 200, "height": 100,
               "kerf": 1.5, "nest": true, "gap": 6,
               "bodies": [{"body": bracket.to_string(), "count": 1},
                          {"body": plate.to_string(), "count": 2}]}),
    )
    .unwrap();
    assert_eq!(added["parts"], json!([1, 2, 3]), "{added}");
    assert_eq!(added["warnings"], json!([]), "{added}");
    (doc, added["drawing"].clone())
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

fn point(value: &Value) -> [f64; 2] {
    [number(&value[0]), number(&value[1])]
}

/// A part's box on the sheet, from the `cut_sheet` query.
fn part_box(sheet: &Value, id: u64) -> ([f64; 2], [f64; 2]) {
    let part = sheet["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap_or_else(|| panic!("no part {id}: {sheet}"));
    (point(&part["box"][0]), point(&part["box"][1]))
}

fn cutting(doc: &Document<MockKernel>, drawing: &Value) -> Value {
    query(
        doc,
        json!({"query": "cut_sheet_cutting", "drawing": drawing}),
    )
}

fn messages(cutting: &Value) -> Vec<(String, Value)> {
    cutting["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["message"].as_str().unwrap().to_owned(),
                p["parts"].clone(),
            )
        })
        .collect()
}

#[test]
fn nested_flat_patterns_are_cut_with_the_kerf_applied_once() {
    let (mut doc, drawing) = nested();
    let sheet = query(&doc, json!({"query": "cut_sheet", "drawing": drawing}));
    // Nested in a row from the lower left corner, 6 mm apart and from the
    // edges, the tallest first.
    for (id, lo, hi) in [
        (1, [6.0, 6.0], [46.0, 26.0]),
        (2, [52.0, 6.0], [82.0, 16.0]),
        (3, [88.0, 6.0], [118.0, 16.0]),
    ] {
        let (got_lo, got_hi) = part_box(&sheet, id);
        for (got, want) in got_lo.iter().chain(&got_hi).zip(lo.iter().chain(&hi)) {
            assert!((got - want).abs() < 1e-6, "part {id}: {sheet}");
        }
    }

    let c = cutting(&doc, &drawing);
    assert_eq!(c["ready"], true, "{c}");
    assert_eq!(c["problems"], json!([]), "{c}");
    assert_eq!(c["process"], "plasma");
    assert_eq!(c["controller"], "grbl 1.1");
    assert_eq!(c["kerf"], 1.5);
    assert_eq!(c["pierces"], 3);
    assert_eq!(c["holes"], 0);
    assert_eq!(c["left_out"], json!({"bend_lines": 0, "marks": 0}));
    let cuts = c["cuts"].as_array().unwrap();
    // The nearest part to the work zero first, then along the row.
    let order: Vec<u64> = cuts
        .iter()
        .map(|cut| cut["part"].as_u64().unwrap())
        .collect();
    assert_eq!(order, vec![1, 2, 3], "{c}");
    for cut in cuts {
        assert_eq!(cut["outer"], true, "{cut}");
        // The beam's path half the kerf outside the part, once: its box
        // the part's grown by 0.75 on every side.
        let (lo, hi) = part_box(&sheet, cut["part"].as_u64().unwrap());
        let path: Vec<[f64; 2]> = cut["path"].as_array().unwrap().iter().map(point).collect();
        let min_x = path.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let max_x = path.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
        let min_y = path.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let max_y = path.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
        for (got, want) in [
            (min_x, lo[0] - 0.75),
            (min_y, lo[1] - 0.75),
            (max_x, hi[0] + 0.75),
            (max_y, hi[1] + 0.75),
        ] {
            assert!((got - want).abs() < 1e-6, "{got} != {want}: {cut}");
        }
        // Closed, from its start round to its start.
        assert_eq!(path.first(), path.last());
        assert_eq!(point(&cut["start"]), path[0]);
        // The pierce point outside the part, at most the lead-in away.
        let pierce = point(&cut["pierce"]);
        let inside =
            pierce[0] > lo[0] && pierce[0] < hi[0] && pierce[1] > lo[1] && pierce[1] < hi[1];
        assert!(!inside, "{cut}");
        assert!(number(&cut["lead_in"]) <= 4.0 + 1e-9, "{cut}");
    }
    // The DXF of the same sheet takes the kerf as well: unchanged by this.
    let dir = mitcad_model::testing::unique_temp_dir("cut-sheet-cutting");
    let dxf = dir.join("sheet.dxf").to_string_lossy().into_owned();
    let written = command(
        &mut doc,
        json!({"cmd": "export_cut_sheet", "drawing": drawing, "path": dxf}),
    )
    .unwrap();
    assert_eq!(written["outer"], 3, "{written}");

    // The program: the torch fired and put out once per loop, the pierce
    // delay after each firing, the notes and the units.
    let path = dir.join("sheet.nc").to_string_lossy().into_owned();
    let revision = doc.revision();
    let posted = command(
        &mut doc,
        json!({"cmd": "post_cut_sheet", "drawing": drawing, "path": path}),
    )
    .unwrap();
    assert_eq!(doc.revision(), revision, "posting changes nothing");
    assert_eq!(posted["path"], json!(path));
    assert_eq!(posted["pierces"], 3);
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(posted["lines"], text.lines().count());
    assert_eq!(text.matches("M3 S1000").count(), 3, "{text}");
    assert_eq!(text.matches("G4 P0.5").count(), 3, "{text}");
    assert!(text.contains("G21"), "{text}");
    assert!(text.contains("Kerf 1.5 mm"), "{text}");
    assert!(text.contains("MitCad CAM is highly experimental"), "{text}");
    assert_eq!(text.lines().last(), Some("M30"));
    // Every cut lies on the sheet.
    for line in text.lines().filter(|l| l.starts_with("G1 ")) {
        for word in line.split_whitespace() {
            let (axis, value) = word.split_at(1);
            if axis == "X" || axis == "Y" {
                let v: f64 = value.parse().unwrap();
                let limit = if axis == "X" { 200.0 } else { 100.0 };
                assert!((0.0..=limit).contains(&v), "{line}");
            }
        }
    }
    // Inches asked for this program only.
    let inches = dir.join("sheet-in.nc").to_string_lossy().into_owned();
    command(
        &mut doc,
        json!({"cmd": "post_cut_sheet", "drawing": drawing, "path": inches,
               "cutting": {"units": "inches"}}),
    )
    .unwrap();
    assert!(std::fs::read_to_string(&inches).unwrap().contains("G20"));
}

#[test]
fn a_layout_that_cannot_be_cut_says_why_and_is_not_posted() {
    let (mut doc, drawing) = nested();
    let dir = mitcad_model::testing::unique_temp_dir("cut-sheet-cutting-refused");
    let path = dir.join("refused.nc").to_string_lossy().into_owned();

    // A Plate moved onto the Bracket.
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet_part", "drawing": drawing, "part": 2, "place": [26, 16]}),
    )
    .unwrap();
    let c = cutting(&doc, &drawing);
    assert_eq!(c["ready"], false, "{c}");
    assert_eq!(
        messages(&c),
        vec![(
            "Two components overlap: move them apart.".to_owned(),
            json!([1, 2])
        )],
        "{c}"
    );
    let error = command(
        &mut doc,
        json!({"cmd": "post_cut_sheet", "drawing": drawing, "path": path}),
    )
    .unwrap_err();
    assert!(error.contains("cannot be cut yet"), "{error}");
    assert!(error.contains("overlap"), "{error}");
    assert!(!std::path::Path::new(&path).exists(), "nothing written");

    // 1 mm beside the Bracket: no overlap, but closer than the kerf.
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet_part", "drawing": drawing, "part": 2, "place": [62, 16]}),
    )
    .unwrap();
    let c = cutting(&doc, &drawing);
    assert_eq!(
        messages(&c),
        vec![(
            "Two components lie closer together than the kerf: leave more room between them."
                .to_owned(),
            json!([1, 2])
        )],
        "{c}"
    );
    // Without a kerf they can be cut.
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet", "drawing": drawing, "kerf": 0}),
    )
    .unwrap();
    assert_eq!(cutting(&doc, &drawing)["ready"], true);

    // A Plate off the sheet.
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet_part", "drawing": drawing, "part": 3, "place": [500, 50]}),
    )
    .unwrap();
    let c = cutting(&doc, &drawing);
    assert_eq!(c["problems"][0]["parts"], json!([3]), "{c}");
    assert!(
        c["problems"][0]["message"]
            .as_str()
            .unwrap()
            .starts_with("A component reaches past the edge of the sheet"),
        "{c}"
    );
    assert!(
        command(
            &mut doc,
            json!({"cmd": "post_cut_sheet", "drawing": drawing, "path": path}),
        )
        .is_err()
    );

    // Nested again: everything can be cut and the program is written.
    command(
        &mut doc,
        json!({"cmd": "nest_cut_sheet", "drawing": drawing, "gap": 6}),
    )
    .unwrap();
    assert_eq!(cutting(&doc, &drawing)["ready"], true);
    command(
        &mut doc,
        json!({"cmd": "post_cut_sheet", "drawing": drawing, "path": path}),
    )
    .unwrap();
    assert!(std::path::Path::new(&path).exists());
}

#[test]
fn the_torch_settings_are_kept_with_the_cut_sheet_and_the_paths_drawn_over_it() {
    let (mut doc, drawing) = nested();
    let shown = |doc: &Document<MockKernel>| {
        query(
            doc,
            json!({"query": "drawing", "drawing": drawing, "geometry": false}),
        )["cutting"]
            .clone()
    };
    assert_eq!(shown(&doc)["feed"], 2000.0);
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet", "drawing": drawing, "cutting": {"feed": 3000, "lead_in": 2}}),
    )
    .unwrap();
    let settings = shown(&doc);
    assert_eq!(settings["feed"], 3000.0);
    assert_eq!(settings["lead_in"], 2.0);
    // The others keep their values.
    assert_eq!(settings["pierce_height"], 3.8);
    // One field more changes that field only.
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet", "drawing": drawing, "cutting": {"pierce_delay": 1.2}}),
    )
    .unwrap();
    let settings = shown(&doc);
    assert_eq!(settings["feed"], 3000.0);
    assert_eq!(settings["pierce_delay"], 1.2);
    let c = cutting(&doc, &drawing);
    for cut in c["cuts"].as_array().unwrap() {
        assert!(number(&cut["lead_in"]) <= 2.0 + 1e-9, "{cut}");
    }
    // Settings that cannot cut are refused in words.
    let error = command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet", "drawing": drawing, "cutting": {"pierce_height": 0.5}}),
    )
    .unwrap_err();
    assert!(
        error.contains("The pierce height must not be below the cut height."),
        "{error}"
    );
    assert!(
        command(
            &mut doc,
            json!({"cmd": "edit_cut_sheet", "drawing": drawing, "cutting": {"speed": 1}}),
        )
        .is_err()
    );
    // Undone with the cut sheet.
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    assert_eq!(shown(&doc)["pierce_delay"], 0.5);
    assert_eq!(shown(&doc)["feed"], 3000.0);
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    assert_eq!(shown(&doc)["feed"], 2000.0);

    // Drawn over the sheet when asked: the paths, the moves across and
    // the pierce points in one group, with the layout's state.
    let drawn = query(
        &doc,
        json!({"query": "drawing", "drawing": drawing, "cutting_paths": true}),
    );
    let groups = drawn["sheets"][0]["groups"].as_array().unwrap();
    let paths: Vec<&Value> = groups.iter().filter(|g| g["class"] == "cut-path").collect();
    assert_eq!(paths.len(), 1, "{drawn}");
    let items = paths[0]["items"].as_array().unwrap();
    assert_eq!(
        items.iter().filter(|i| i.get("polyline").is_some()).count(),
        3
    );
    assert!(items.iter().any(|i| i["pen"] == "hidden"));
    assert_eq!(drawn["cutting_paths"]["ready"], true, "{drawn}");
    // Not asked: no paths.
    let plain = query(&doc, json!({"query": "drawing", "drawing": drawing}));
    assert!(
        plain["sheets"][0]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["class"] != "cut-path")
    );
    assert!(plain.get("cutting_paths").is_none());
}
