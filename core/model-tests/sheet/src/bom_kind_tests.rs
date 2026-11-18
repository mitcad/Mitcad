// SPDX-License-Identifier: MIT
//! A Bill of Materials' Kind, the Sheet Metal list and Flat Pattern parts
//! on a cut sheet (mitcad#1980), and the BOM Structure's default, override
//! and filter (mitcad#1985), on the mock kernel: sheet-metal components (a
//! sheet Face each, no bends), an imported body, a plain solid, a mixed
//! component and a subassembly. The BUILD kinds (Build profile, Library
//! part) and their BOM Structure are tested with BUILD's test library
//! (`build-frames/src/bom_kind_tests.rs`); the structured and parts-only
//! views and drawing parts lists in `drawing/src/bom_structure_tests.rs`.

use mitcad_model::features::{Brep, SketchPlane};
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{BomStructure, ComponentKind, Document};
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

/// A new component beside the root's others, active; its uid.
fn component(doc: &mut Document<MockKernel>, name: &str) -> ComponentUid {
    command(doc, json!({"cmd": "activate_component", "component": "C0"})).unwrap();
    let made = command(doc, json!({"cmd": "create_component", "name": name})).unwrap();
    made["component"].as_str().unwrap().parse().unwrap()
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

/// A 10 mm box in the active component.
fn solid(doc: &mut Document<MockKernel>) -> BodyUid {
    let added = doc
        .add_feature(
            &def(json!({"type": "box", "plane": "xy", "corner": [0, 0],
                        "length": 10, "width": 10, "height": 10, "operation": "new_body"})),
            None,
        )
        .unwrap();
    BodyUid::new(added.uid, 0)
}

/// A body without history, as an imported file gives it, in the active
/// component.
fn imported(doc: &mut Document<MockKernel>) -> BodyUid {
    let brep = serde_json::to_value(Brep::new(b"6 bolt".to_vec())).unwrap();
    let added = doc
        .add_feature(
            &def(
                json!({"type": "base", "bodies": [{"name": "Bolt", "brep": brep}],
                        "source": "bolt.step"}),
            ),
            None,
        )
        .unwrap();
    BodyUid::new(added.uid, 0)
}

/// The parts list's rows by component name.
fn rows(doc: &Document<MockKernel>, fields: Value) -> Vec<Value> {
    let mut asked = json!({"query": "parts_list"});
    for (key, value) in fields.as_object().unwrap() {
        asked[key] = value.clone();
    }
    query(doc, asked)["rows"].as_array().unwrap().clone()
}

fn row<'a>(rows: &'a [Value], name: &str) -> &'a Value {
    rows.iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("no row {name}: {rows:?}"))
}

/// A component's property as `property_sets` shows it, computed values too.
fn property(doc: &Document<MockKernel>, component: ComponentUid, set: &str, key: &str) -> Value {
    let levels = query(
        doc,
        json!({"query": "property_sets", "component": component, "computed": true}),
    );
    levels["levels"][0]["sets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["name"] == set)
        .flat_map(|s| s["properties"].as_array().unwrap().clone())
        .find(|p| p["key"] == key)
        .unwrap_or_else(|| panic!("no {set} / {key}: {levels}"))
}

/// The kinds' design: Bracket and Plate of sheet metal, Bolt imported,
/// Block a plain solid, Combo a solid and an imported body, Frame a
/// subassembly of a solid Leaf.
struct Design {
    doc: Document<MockKernel>,
    bracket: (ComponentUid, BodyUid),
    plate: (ComponentUid, BodyUid),
    bolt: ComponentUid,
    block: ComponentUid,
    combo: ComponentUid,
    frame: ComponentUid,
    leaf: ComponentUid,
}

fn design() -> Design {
    let mut doc = Document::new(MockKernel::default());
    let bracket = component(&mut doc, "Bracket");
    let bracket_body = sheet_face(&mut doc, 40.0, 20.0);
    let plate = component(&mut doc, "Plate");
    let plate_body = sheet_face(&mut doc, 30.0, 10.0);
    let bolt = component(&mut doc, "Bolt");
    imported(&mut doc);
    let block = component(&mut doc, "Block");
    solid(&mut doc);
    let combo = component(&mut doc, "Combo");
    solid(&mut doc);
    imported(&mut doc);
    let frame = component(&mut doc, "Frame");
    let made = command(&mut doc, json!({"cmd": "create_component", "name": "Leaf"})).unwrap();
    let leaf: ComponentUid = made["component"].as_str().unwrap().parse().unwrap();
    solid(&mut doc);
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    )
    .unwrap();
    Design {
        doc,
        bracket: (bracket, bracket_body),
        plate: (plate, plate_body),
        bolt,
        block,
        combo,
        frame,
        leaf,
    }
}

#[test]
fn kind_says_each_components_form_and_origin() {
    let d = design();
    let doc = &d.doc;
    for (component, kind) in [
        (d.bracket.0, ComponentKind::SheetMetal),
        (d.plate.0, ComponentKind::SheetMetal),
        (d.bolt, ComponentKind::Imported),
        (d.block, ComponentKind::Solid),
        (d.combo, ComponentKind::Mixed),
        (d.frame, ComponentKind::Assembly),
        (d.leaf, ComponentKind::Solid),
    ] {
        assert_eq!(doc.component_kind(component, None), kind, "{component}");
        // The computed property, read-only, and the parts list's row.
        let shown = property(doc, component, "Computed", "Kind");
        assert_eq!(shown["text"], kind.as_str(), "{shown}");
        assert_eq!(shown["read_only"], true);
        assert_eq!(shown["source"], "computed");
    }
    assert!(doc.is_sheet_metal_body(d.bracket.1));
    let listed = rows(doc, json!({"properties": ["Kind"]}));
    assert_eq!(row(&listed, "Bracket")["kind"], "Sheet metal");
    // Combo's two bodies are a row each, each of its own kind; its
    // property column says what the component is.
    let mut combo: Vec<(&str, &str)> = listed
        .iter()
        .filter(|r| r["component"] == json!(d.combo))
        .map(|r| {
            (
                r["kind"].as_str().unwrap(),
                r["properties"]["Kind"].as_str().unwrap(),
            )
        })
        .collect();
    combo.sort();
    assert_eq!(combo, [("Imported", "Mixed"), ("Solid", "Mixed")]);
    assert_eq!(row(&listed, "Frame")["properties"]["Kind"], "Assembly");
    assert_eq!(row(&listed, "Bolt")["properties"]["Kind"], "Imported");
    // Never written to the design file.
    let text = doc.to_json();
    assert!(!text.contains("\"Kind\""), "{text}");
    assert!(!text.contains("BOM Structure"), "{text}");
    // A property search finds it by its value.
    let found = query(
        doc,
        json!({"query": "property_search", "text": "Kind = Sheet metal"}),
    );
    let names: Vec<&str> = found["matches"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["level"] == "component")
        .filter_map(|m| m["name"].as_str())
        .collect();
    assert_eq!(names, ["Bracket", "Plate"], "{found}");
}

#[test]
fn bom_structure_is_worked_out_and_the_users_choice_overrides_it() {
    let mut d = design();
    // None of these is a library part: each is Normal.
    for component in [d.bracket.0, d.bolt, d.block, d.combo, d.frame] {
        assert_eq!(d.doc.bom_structure(component, None), BomStructure::Normal);
        let shown = property(&d.doc, component, "Bill of Materials", "BOM Structure");
        assert_eq!(shown["text"], "Normal", "{shown}");
        assert_eq!(shown["read_only"], false);
        assert_eq!(
            shown["choices"],
            json!(["Normal", "Purchased", "Phantom", "Reference", "Inseparable"])
        );
    }
    // The user buys the block: a stored value over the worked-out one, in
    // its own spelling, in one undo step.
    let set = command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": d.block, "set": "bill of materials",
               "key": "bom structure", "value": "purchased"}),
    )
    .unwrap();
    assert_eq!(set["value"], "Purchased", "{set}");
    assert_eq!(d.doc.bom_structure(d.block, None), BomStructure::Purchased);
    assert_eq!(
        d.doc.bom_structure_default(d.block, None),
        BomStructure::Normal
    );
    let shown = property(&d.doc, d.block, "Bill of Materials", "BOM Structure");
    assert_eq!(shown["text"], "Purchased");
    assert_eq!(shown["edited"], true);
    assert_eq!(shown["source_value"], "Normal");
    assert_eq!(shown["source_of_value"], "computed");
    let listed = rows(&d.doc, json!({"properties": ["BOM Structure"]}));
    assert_eq!(row(&listed, "Block")["bom_structure"], "Purchased");
    assert_eq!(
        row(&listed, "Block")["properties"]["BOM Structure"],
        "Purchased"
    );
    assert_eq!(row(&listed, "Bolt")["bom_structure"], "Normal");
    // Kept in the design file, as the user's.
    let text = d.doc.to_json();
    assert!(text.contains("BOM Structure"), "{text}");
    // None of the five is refused, naming them.
    let error = command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": d.bolt, "set": "Bill of Materials",
               "key": "BOM Structure", "value": "Maybe"}),
    )
    .unwrap_err();
    assert!(error.contains("Normal, Purchased, Phantom"), "{error}");
    // The filters of the Bill of Materials.
    let bought = rows(&d.doc, json!({"bom_structure": ["Purchased"]}));
    assert_eq!(bought.len(), 1, "{bought:?}");
    assert_eq!(bought[0]["name"], "Block");
    let sheets = rows(&d.doc, json!({"kind": ["sheet metal"]}));
    let names: Vec<&str> = sheets.iter().filter_map(|r| r["name"].as_str()).collect();
    assert_eq!(names, ["Bracket", "Plate"]);
    let normal_solids = rows(
        &d.doc,
        json!({"kind": ["Solid", "Mixed"], "bom_structure": ["normal"]}),
    );
    let components: Vec<&Value> = normal_solids.iter().map(|r| &r["component"]).collect();
    assert_eq!(
        components,
        [&json!(d.combo), &json!(d.leaf)],
        "{normal_solids:?}"
    );
    let error = d
        .doc
        .query(&json!({"query": "parts_list", "kind": ["Gadget"]}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("there is no kind Gadget"), "{error}");
    let error = d
        .doc
        .query(&json!({"query": "parts_list", "bom_structure": ["Ghost"]}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("there is no BOM Structure Ghost"), "{error}");
    // Removed, the worked-out value shows again; undo brings it back.
    command(
        &mut d.doc,
        json!({"cmd": "remove_property", "owner": d.block, "set": "Bill of Materials",
               "key": "BOM Structure"}),
    )
    .unwrap();
    assert_eq!(d.doc.bom_structure(d.block, None), BomStructure::Normal);
    d.doc.undo();
    assert_eq!(d.doc.bom_structure(d.block, None), BomStructure::Purchased);
}

#[test]
fn the_sheet_metal_list_gives_each_sheet_parts_blank() {
    let mut d = design();
    // The plate placed twice.
    let plate_occurrence = d
        .doc
        .assembly()
        .occurrences_of(d.plate.0)
        .next()
        .unwrap()
        .uid;
    command(
        &mut d.doc,
        json!({"cmd": "copy_occurrence", "occurrence": plate_occurrence}),
    )
    .unwrap();
    let list = query(&d.doc, json!({"query": "sheet_metal_list"}));
    let rows = list["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{list}");
    let bracket = rows
        .iter()
        .find(|r| r["component_name"] == "Bracket")
        .unwrap();
    assert_eq!(bracket["body"], json!(d.bracket.1));
    assert_eq!(bracket["quantity"], 1);
    assert_eq!(bracket["thickness"], 1.0);
    assert_eq!(bracket["material"], "No material");
    assert_eq!(
        (bracket["width"].as_f64(), bracket["height"].as_f64()),
        (Some(40.0), Some(20.0))
    );
    assert_eq!(bracket["area"], 800.0);
    assert_eq!(bracket["bends"], 0);
    assert!(bracket.get("problem").is_none(), "{bracket}");
    let plate = rows
        .iter()
        .find(|r| r["component_name"] == "Plate")
        .unwrap();
    assert_eq!(plate["quantity"], 2);
    assert_eq!(
        (plate["width"].as_f64(), plate["height"].as_f64()),
        (Some(30.0), Some(10.0))
    );
    assert_eq!(plate["area"], 300.0);
    // The solids are not listed; nothing was added to the design.
    let revision = d.doc.revision();
    query(&d.doc, json!({"query": "sheet_metal_list"}));
    assert_eq!(d.doc.revision(), revision);
    assert!(d.doc.flat_pattern_defs().is_empty());
}

/// The lengths of a DXF file's lines on a layer.
fn line_lengths(dxf: &mitcad_dxf::Drawing, layer: &str) -> Vec<f64> {
    dxf.entities
        .iter()
        .filter(|e| e.layer == layer)
        .filter_map(|e| match &e.geometry {
            mitcad_dxf::Geometry::Line { start, end } => {
                Some((end.x - start.x).hypot(end.y - start.y))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_cut_sheet_takes_flat_patterns_and_writes_their_developed_outlines() {
    let mut d = design();
    // The plate's Flat Pattern seen from its back: its own side is kept.
    command(
        &mut d.doc,
        json!({"cmd": "set_flat_pattern", "body": d.plate.1, "reference_side": "back"}),
    )
    .unwrap();
    let added = command(
        &mut d.doc,
        json!({"cmd": "add_cut_sheet", "flat": true,
               "bodies": [{"body": d.bracket.1.to_string(), "count": 1},
                          {"body": d.plate.1.to_string(), "count": 2}]}),
    )
    .unwrap();
    assert_eq!(added["parts"], json!([1, 2, 3]), "{added}");
    assert_eq!(added["warnings"], json!([]), "{added}");
    let sheet = query(
        &d.doc,
        json!({"query": "cut_sheet", "drawing": added["drawing"]}),
    );
    let parts = sheet["parts"].as_array().unwrap();
    for (part, size) in parts.iter().zip([[40.0, 20.0], [30.0, 10.0], [30.0, 10.0]]) {
        assert_eq!(part["flat"], true, "{part}");
        assert_eq!(part["outer"], 1, "{part}");
        assert_eq!(part["inner"], 0, "{part}");
        assert_eq!(part["bends"], 0, "{part}");
        let got = [
            part["size"][0].as_f64().unwrap(),
            part["size"][1].as_f64().unwrap(),
        ];
        assert!(
            (got[0] - size[0]).abs() < 1e-6 && (got[1] - size[1]).abs() < 1e-6,
            "{part}"
        );
        // The exterior runs counter-clockwise, its pieces end to end.
        let pieces = part["loops"][0]["pieces"].as_array().unwrap();
        let mut area = 0.0;
        for piece in pieces {
            let line = &piece["line"];
            let (a, b) = (&line[0], &line[1]);
            area += a[0].as_f64().unwrap() * b[1].as_f64().unwrap()
                - b[0].as_f64().unwrap() * a[1].as_f64().unwrap();
        }
        assert!((area / 2.0 - size[0] * size[1]).abs() < 1e-6, "{part}");
    }
    assert_eq!(sheet["counts"].as_array().unwrap().len(), 2, "{sheet}");
    // A Flat Pattern part takes no face.
    let error = command(
        &mut d.doc,
        json!({"cmd": "edit_cut_sheet_part", "drawing": added["drawing"], "part": 1,
               "face": "F3:top"}),
    )
    .unwrap_err();
    assert!(error.contains("Flat Pattern"), "{error}");

    // The DXF: the developed outlines at 1:1 as cut edges.
    let dir = mitcad_model::testing::unique_temp_dir("cut-sheet-flat");
    let path = dir.join("blanks.dxf").to_string_lossy().into_owned();
    let written = command(
        &mut d.doc,
        json!({"cmd": "export_cut_sheet", "drawing": added["drawing"], "path": path}),
    )
    .unwrap();
    assert_eq!(written["parts"], 3, "{written}");
    assert_eq!(written["outer"], 3, "{written}");
    assert_eq!(written["inner"], 0, "{written}");
    assert_eq!(written["bends"], 0, "{written}");
    let back = mitcad_dxf::read_file(&path).unwrap();
    let lengths = line_lengths(&back, "CUT_OUTER");
    let perimeter: f64 = lengths.iter().sum();
    assert!(
        (perimeter - (120.0 + 2.0 * 80.0)).abs() < 1e-6,
        "{lengths:?}"
    );
    assert!(back.layer("BEND_UP").is_none(), "no bends, no bend layers");
    // The SVG likewise: one closed path per blank.
    let svg = dir.join("blanks.svg").to_string_lossy().into_owned();
    command(
        &mut d.doc,
        json!({"cmd": "export_cut_sheet", "drawing": added["drawing"], "path": svg}),
    )
    .unwrap();
    let text = std::fs::read_to_string(&svg).unwrap();
    assert_eq!(text.matches("<path").count(), 3, "{text}");

    // A solid's Flat Pattern cannot be developed: a warning, as a missing
    // part gives, in words.
    let block_body = d
        .doc
        .bodies()
        .into_iter()
        .find(|b| d.doc.body_component(b.uid) == Some(d.block))
        .unwrap()
        .uid;
    let more = command(
        &mut d.doc,
        json!({"cmd": "add_cut_sheet", "flat": true, "bodies": [{"body": block_body.to_string()}]}),
    );
    // Adding needs its outline: refused, saying why.
    let error = more.unwrap_err();
    assert!(error.contains("not a sheet-metal body"), "{error}");
    // Parts whose body is gone stay on the sheet, each with a warning.
    let flat_sheet = added["drawing"].clone();
    let plate_sheet_face = d.plate.1.feature;
    d.doc.set_suppressed(plate_sheet_face, true).unwrap();
    let after = query(&d.doc, json!({"query": "cut_sheet", "drawing": flat_sheet}));
    let warnings = after["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 2, "{after}");
    assert!(
        warnings[0].as_str().unwrap().starts_with("part 2 "),
        "{after}"
    );
}
