// SPDX-License-Identifier: MIT
//! The length unit of the DXF exports (mitcad#1408) on the mock kernel: an
//! inch design's Flat Pattern, drawing, face and sketch are written in
//! inches (`$INSUNITS` 1, `$MEASUREMENT` 0), a millimetre design's in
//! millimetres as before, `units` writes another unit, and the DXF reader
//! gives the files back at their size.

use serde_json::{Value, json};

use mitcad_dxf::{Drawing, Geometry, Units};
use mitcad_model::Document;
use mitcad_model::expr::LengthUnit;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const INCH: f64 = 25.4;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// The width and height of what is on `layer`.
fn size(drawing: &Drawing, layer: &str) -> (f64, f64) {
    let (min, max) = drawing
        .on_layers(&[layer.to_owned()])
        .bounds()
        .unwrap_or_else(|| panic!("nothing on {layer}"));
    (max.x - min.x, max.y - min.y)
}

/// The `$INSUNITS` and `$MEASUREMENT` header values of a written file.
fn header(path: &str) -> (i64, i64) {
    let text = std::fs::read_to_string(path).unwrap();
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let value = |name: &str| {
        let at = lines
            .iter()
            .position(|l| *l == name)
            .unwrap_or_else(|| panic!("no {name} in {path}"));
        lines[at + 2].parse::<i64>().unwrap()
    };
    (value("$INSUNITS"), value("$MEASUREMENT"))
}

/// A design in `unit` with a 5.5 x 6 in sheet-metal Face (F3.b0, 2 mm
/// thick) drawn in sketch F1.
fn sheet_face(unit: LengthUnit) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.set_units(unit).unwrap();
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 5.5 * INCH, "height": 6.0 * INCH}),
    );
    let rules = json!({"thickness": "2 mm", "bend_radius": "1 mm",
        "allowance": {"type": "k_factor", "k": 0.44}, "reference_side": "front",
        "bend_relief": {"type": "round", "width": 1, "depth": 3},
        "corner_relief": {"type": "rectangular", "width": 1, "depth": 3}});
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "sheet_face", "rules": rules,
               "profiles": [{"sketch": "F1", "region": R}]}}),
    );
    assert_eq!(added["uid"], "F3", "{added}");
    doc
}

/// A design in `unit` with a 5.5 x 6 x 1 in block (F2.b0).
fn block(unit: LengthUnit) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.set_units(unit).unwrap();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
             "width": 5.5 * INCH, "height": 6.0 * INCH},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": INCH}, "operation": "new_body"}},
        ]),
    );
    doc
}

#[test]
fn an_inch_design_s_flat_pattern_is_written_in_inches() {
    let mut doc = sheet_face(LengthUnit::Inch);
    let dir = mitcad_model::testing::unique_temp_dir("dxf-units-flat");
    let path = dir.join("flat.dxf").to_string_lossy().into_owned();
    let exported = command(
        &mut doc,
        json!({"cmd": "export_flat_pattern_dxf", "body": "F3.b0", "path": path}),
    );
    assert_eq!(exported["units"], "in", "{exported}");
    assert_eq!(exported["unit"], "in");
    assert_eq!(header(&path), (1, 0));
    let back = mitcad_dxf::read_file(&path).unwrap();
    assert_eq!(back.units, Units::Inches);
    let (w, h) = size(&back, "CUT_OUTER");
    assert!(near(w, 5.5) && near(h, 6.0), "{w} x {h}");
    // Read back through the unit the file names: the millimetres it has.
    let mm = back.to_millimeters(Units::Unitless);
    let (w, h) = size(&mm, "CUT_OUTER");
    assert!(near(w, 5.5 * INCH) && near(h, 6.0 * INCH), "{w} x {h}");

    // Millimetres when asked for, from the same inch design; `unit` is
    // the field's older name.
    for field in ["units", "unit"] {
        let exported = command(
            &mut doc,
            json!({"cmd": "export_flat_pattern_dxf", "body": "F3.b0", "path": path, field: "mm"}),
        );
        assert_eq!(exported["units"], "mm", "{exported}");
        assert_eq!(header(&path), (4, 1));
        let back = mitcad_dxf::read_file(&path).unwrap();
        assert_eq!(back.units, Units::Millimeters);
        let (w, h) = size(&back, "CUT_OUTER");
        assert!(near(w, 139.7) && near(h, 152.4), "{w} x {h}");
    }
    // A unit the design does not know is refused.
    let error = doc
        .command(
            &json!({"cmd": "export_flat_pattern_dxf", "body": "F3.b0", "path": path,
                    "units": "furlong"})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("furlong"), "{error}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_millimetre_design_s_flat_pattern_stays_in_millimetres() {
    let mut doc = sheet_face(LengthUnit::Millimetre);
    let dir = mitcad_model::testing::unique_temp_dir("dxf-units-flat-mm");
    let path = dir.join("flat.dxf").to_string_lossy().into_owned();
    let exported = command(
        &mut doc,
        json!({"cmd": "export_flat_pattern_dxf", "body": "F3.b0", "path": path}),
    );
    assert_eq!(exported["units"], "mm", "{exported}");
    assert_eq!(header(&path), (4, 1));
    let back = mitcad_dxf::read_file(&path).unwrap();
    assert_eq!(back.units, Units::Millimeters);
    let (w, h) = size(&back, "CUT_OUTER");
    assert!(near(w, 139.7) && near(h, 152.4), "{w} x {h}");
    // Inches when asked for.
    command(
        &mut doc,
        json!({"cmd": "export_flat_pattern_dxf", "body": "F3.b0", "path": path, "units": "in"}),
    );
    assert_eq!(header(&path), (1, 0));
    let (w, h) = size(&mitcad_dxf::read_file(&path).unwrap(), "CUT_OUTER");
    assert!(near(w, 5.5) && near(h, 6.0), "{w} x {h}");
    std::fs::remove_dir_all(&dir).ok();
}

/// The largest text height of a drawing.
fn text_height(drawing: &Drawing) -> f64 {
    drawing
        .entities
        .iter()
        .filter_map(|e| match &e.geometry {
            Geometry::Text(t) => Some(t.height),
            _ => None,
        })
        .fold(0.0, f64::max)
}

#[test]
fn an_inch_design_s_drawing_is_a_sheet_in_inches_of_paper() {
    let mut doc = block(LengthUnit::Inch);
    let dir = mitcad_model::testing::unique_temp_dir("dxf-units-drawing");
    let path = dir.join("block.dxf").to_string_lossy().into_owned();
    // A 1:1 top view: it measures the model in inches.
    let result = command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": path, "sheet": "A3", "scale": "1:1",
               "views": ["top"]}),
    );
    assert_eq!(result["format"], "dxf", "{result}");
    assert_eq!(result["units"], "in");
    assert_eq!(result["file_units"], "in");
    assert_eq!(header(&path), (1, 0));
    let inches = mitcad_dxf::read_file(&path).unwrap();
    assert_eq!(inches.units, Units::Inches);
    let (w, h) = size(&inches, "VISIBLE");
    assert!(near(w, 5.5) && near(h, 6.0), "{w} x {h}");

    // The same sheet in millimetres when asked for: the view and the
    // texts 25.4 times as large.
    let mm_path = dir.join("block-mm.dxf").to_string_lossy().into_owned();
    let result = command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": mm_path, "sheet": "A3", "scale": "1:1",
               "views": ["top"], "units": "mm"}),
    );
    assert_eq!(result["file_units"], "mm", "{result}");
    assert_eq!(header(&mm_path), (4, 1));
    let millimetres = mitcad_dxf::read_file(&mm_path).unwrap();
    let (w, h) = size(&millimetres, "VISIBLE");
    assert!(near(w, 139.7) && near(h, 152.4), "{w} x {h}");
    let (inch_text, mm_text) = (text_height(&inches), text_height(&millimetres));
    assert!(mm_text > 1.0, "{mm_text}");
    assert!(near(inch_text * INCH, mm_text), "{inch_text} {mm_text}");
    // The frame too: the whole A3 sheet in inches of paper.
    let (fw, fh) = size(&inches, "FRAME");
    let (mw, mh) = size(&millimetres, "FRAME");
    assert!(near(fw * INCH, mw) && near(fh * INCH, mh), "{fw} {mw}");
    assert!(mw <= 420.0 + 1e-6 && mh <= 297.0 + 1e-6, "{mw} x {mh}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_millimetre_design_s_drawing_stays_in_millimetres() {
    let mut doc = block(LengthUnit::Millimetre);
    let dir = mitcad_model::testing::unique_temp_dir("dxf-units-drawing-mm");
    let path = dir.join("block.dxf").to_string_lossy().into_owned();
    let result = command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": path, "sheet": "A3", "scale": "1:1",
               "views": ["top"]}),
    );
    assert_eq!(result["file_units"], "mm", "{result}");
    assert_eq!(header(&path), (4, 1));
    let back = mitcad_dxf::read_file(&path).unwrap();
    assert_eq!(back.units, Units::Millimeters);
    let (w, h) = size(&back, "VISIBLE");
    assert!(near(w, 139.7) && near(h, 152.4), "{w} x {h}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_kept_drawing_is_written_in_its_sheet_s_unit_or_the_one_asked_for() {
    let mut doc = block(LengthUnit::Inch);
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing", "layout": {"sheet": "A3", "scale": "1:1",
               "views": ["top"]}}),
    );
    let uid = added["drawing"].as_str().unwrap().to_owned();
    let dir = mitcad_model::testing::unique_temp_dir("dxf-units-kept");
    let path = dir.join("kept.dxf").to_string_lossy().into_owned();
    let result = command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": path, "drawing": uid}),
    );
    assert_eq!(result["file_units"], "in", "{result}");
    let (w, h) = size(&mitcad_dxf::read_file(&path).unwrap(), "VISIBLE");
    assert!(near(w, 5.5) && near(h, 6.0), "{w} x {h}");
    // `units` writes the file in another unit; the drawing keeps its own.
    let result = command(
        &mut doc,
        json!({"cmd": "export_drawing", "path": path, "drawing": uid, "units": "mm"}),
    );
    assert_eq!(result["file_units"], "mm", "{result}");
    assert_eq!(result["units"], "in");
    assert_eq!(header(&path), (4, 1));
    let (w, h) = size(&mitcad_dxf::read_file(&path).unwrap(), "VISIBLE");
    assert!(near(w, 139.7) && near(h, 152.4), "{w} x {h}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_inch_design_s_face_is_written_in_inches_or_the_unit_asked_for() {
    let mut doc = Document::new(MockKernel::default());
    doc.set_units(LengthUnit::Inch).unwrap();
    doc.kernel().box_faces.set(true);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 5.5 * INCH, "width": 6.0 * INCH, "height": 3, "operation": "new_body"}}),
    );
    let dir = mitcad_model::testing::unique_temp_dir("dxf-units-face");
    let path = dir.join("plate.dxf").to_string_lossy().into_owned();
    let written = command(
        &mut doc,
        json!({"cmd": "export_face_dxf", "body": "F1.b0", "face": "F1:top", "path": path}),
    );
    assert_eq!(written["units"], "in", "{written}");
    assert_eq!(header(&path), (1, 0));
    let (w, h) = size(&mitcad_dxf::read_file(&path).unwrap(), "CUT_OUTER");
    assert!(near(w, 5.5) && near(h, 6.0), "{w} x {h}");
    let written = command(
        &mut doc,
        json!({"cmd": "export_face_dxf", "body": "F1.b0", "face": "F1:top", "path": path,
               "units": "mm"}),
    );
    assert_eq!(written["units"], "mm", "{written}");
    assert_eq!(header(&path), (4, 1));
    let (w, h) = size(&mitcad_dxf::read_file(&path).unwrap(), "CUT_OUTER");
    assert!(near(w, 139.7) && near(h, 152.4), "{w} x {h}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_inch_design_s_sketch_comes_back_at_its_size_through_the_dxf_reader() {
    let mut doc = Document::new(MockKernel::default());
    doc.set_units(LengthUnit::Inch).unwrap();
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": INCH}),
    );
    let dir = mitcad_model::testing::unique_temp_dir("dxf-units-sketch");
    let path = dir.join("circle.dxf").to_string_lossy().into_owned();
    let exported = command(
        &mut doc,
        json!({"cmd": "export_sketch", "sketch": "F1", "path": path}),
    );
    assert_eq!(exported["units"], "in", "{exported}");
    assert_eq!(header(&path), (1, 0));
    let back = mitcad_dxf::read_file(&path).unwrap();
    assert!(back.entities.iter().any(|e| matches!(e.geometry,
        Geometry::Circle { radius, .. } if near(radius, 0.5))));
    // What the Insert DXF dialog reads of it.
    let info = query(&doc, json!({"query": "dxf_info", "path": path}));
    assert_eq!(info["unit"], "in", "{info}");
    assert_eq!(info["unit_mm"], INCH);

    // Inserted into a millimetre design: the circle keeps its size.
    let radius = |doc: &Document<MockKernel>| {
        query(doc, json!({"query": "sketch", "uid": "F1"}))["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["type"] == "circle")
            .map(|e| e["geometry"]["radius"].as_f64().unwrap())
    };
    let mut metric = Document::new(MockKernel::default());
    command(&mut metric, json!({"cmd": "sketch.create"}));
    command(
        &mut metric,
        json!({"cmd": "sketch.import_dxf", "sketch": "F1", "path": path}),
    );
    assert!(radius(&metric).is_some_and(|r| near(r, INCH / 2.0)));

    // Centimetres when asked for, read back at the same size.
    let exported = command(
        &mut doc,
        json!({"cmd": "export_sketch", "sketch": "F1", "path": path, "units": "cm"}),
    );
    assert_eq!(exported["units"], "cm", "{exported}");
    assert_eq!(header(&path), (5, 1));
    let mut metric = Document::new(MockKernel::default());
    command(&mut metric, json!({"cmd": "sketch.create"}));
    command(
        &mut metric,
        json!({"cmd": "sketch.import_dxf", "sketch": "F1", "path": path}),
    );
    assert!(radius(&metric).is_some_and(|r| near(r, INCH / 2.0)));
    std::fs::remove_dir_all(&dir).ok();
}

/// The numbers of a line or a circle (`[x0, y0, x1, y1]`, `[x, y, r]`).
fn numbers(geometry: &Geometry) -> Vec<f64> {
    match geometry {
        Geometry::Line { start, end } => vec![start.x, start.y, end.x, end.y],
        Geometry::Circle { center, radius } => vec![center.x, center.y, *radius],
        other => panic!("not a line or a circle: {other:?}"),
    }
}

#[test]
fn a_sketch_s_dxf_holds_the_same_entities_in_millimetres_and_in_inches() {
    // GitHub #187 (mitcad#1452): a narrow bar with a small hole, written in
    // millimetres and in inches. Both files hold the same four lines and
    // one circle on the layer 0 with a continuous line type, and nothing
    // else; only the numbers (divided by 25.4) and the header differ.
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [10, 0],
               "width": 0.875, "height": 12.125}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [10.4375, 11.125],
               "diameter": 0.2}),
    );
    let dir = mitcad_model::testing::unique_temp_dir("dxf-units-same-entities");
    let mm_path = dir.join("bar-mm.dxf").to_string_lossy().into_owned();
    let in_path = dir.join("bar-in.dxf").to_string_lossy().into_owned();
    let exported = command(
        &mut doc,
        json!({"cmd": "export_sketch", "sketch": "F1", "path": mm_path}),
    );
    assert_eq!(exported["units"], "mm", "{exported}");
    assert_eq!(exported["entities"], 5, "{exported}");
    let exported = command(
        &mut doc,
        json!({"cmd": "export_sketch", "sketch": "F1", "path": in_path, "units": "in"}),
    );
    assert_eq!(exported["units"], "in", "{exported}");
    assert_eq!(exported["entities"], 5, "{exported}");
    assert_eq!(header(&mm_path), (4, 1));
    assert_eq!(header(&in_path), (1, 0));

    let mm = mitcad_dxf::read_file(&mm_path).unwrap();
    let inch = mitcad_dxf::read_file(&in_path).unwrap();
    assert_eq!(mm.units, Units::Millimeters);
    assert_eq!(inch.units, Units::Inches);
    for drawing in [&mm, &inch] {
        let layers: Vec<(&str, &str)> = drawing
            .layers
            .iter()
            .map(|l| (l.name.as_str(), l.linetype.as_str()))
            .collect();
        assert_eq!(layers, [("0", "Continuous")]);
        assert_eq!(drawing.entities.len(), 5);
        assert!(drawing.entities.iter().all(|e| e.layer == "0"));
        let circles = drawing
            .entities
            .iter()
            .filter(|e| matches!(e.geometry, Geometry::Circle { .. }))
            .count();
        assert_eq!(circles, 1);
    }
    // Entity by entity, the inch file is the millimetre file divided by 25.4.
    for (a, b) in mm.entities.iter().zip(&inch.entities) {
        let (a, b) = (numbers(&a.geometry), numbers(&b.geometry));
        assert_eq!(a.len(), b.len(), "{a:?} / {b:?}");
        assert!(
            a.iter().zip(&b).all(|(a, b)| near(*a, *b * INCH)),
            "{a:?} / {b:?}"
        );
    }
    let mm_bar = size(&mm, "0");
    assert!(
        near(mm_bar.0, 0.875) && near(mm_bar.1, 12.125),
        "{mm_bar:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}
