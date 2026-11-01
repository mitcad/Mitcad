// SPDX-License-Identifier: MIT
//! Metric defaults (P11, `docs/architecture.md`): wherever a unit
//! or a measurement system is chosen, the default is metric; inches are
//! there to choose but never the default, and a file's own units are kept.
//! The application's dialogs are checked by `tools/ui-units-test.sh`.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ImportBody;
use mitcad_model::analysis::{DEFAULT_MATERIAL, material};
use mitcad_model::exchange::{ExportFormat, ExportOptions, LengthUnit, ReadOptions};
use mitcad_model::expr::{AngleUnit, EvalContext, LengthUnit as ExprLength};
use mitcad_model::features::ThreadStandard;
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// A document with a 60 x 40 x 20 block.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let rectangle = command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    );
    let region = rectangle["region"].clone();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    doc
}

#[test]
fn documents_and_expressions_are_millimetres_and_degrees() {
    let mut doc = Document::new(MockKernel::default());
    assert_eq!(
        query(&doc, json!({"query": "document"}))["units"],
        json!({"length": "mm", "angle": "deg"})
    );
    // Bare numbers are millimetres and degrees.
    let length = query(&doc, json!({"query": "evaluate", "expression": "20"}));
    assert_eq!(length["value"], 20.0);
    assert_eq!(length["text"], "20 mm");
    let angle = query(
        &doc,
        json!({"query": "evaluate", "expression": "90", "kind": "angle"}),
    );
    assert!((angle["value"].as_f64().unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    // A user parameter without a unit takes millimetres.
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "wall", "value": 3}),
    );
    let params = query(&doc, json!({"query": "parameters"}));
    assert_eq!(params[0]["unit"], "mm", "{params}");
    assert_eq!(params[0]["expression"], "3 mm", "{params}");
    // The evaluation context of the expression library.
    let context = EvalContext::default();
    assert_eq!(context.default_length_unit, ExprLength::Millimetre);
    assert_eq!(context.default_angle_unit, AngleUnit::Degree);

    // A design file without units is in millimetres; one with inches keeps
    // them (a file's own units are kept).
    let saved = doc.to_json();
    let mut file: Value = serde_json::from_str(&saved).unwrap();
    file.as_object_mut().unwrap().remove("units");
    let opened = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    assert_eq!(
        query(&opened, json!({"query": "document"}))["units"]["length"],
        "mm"
    );
    command(&mut doc, json!({"cmd": "set_units", "length": "in"}));
    let opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(
        query(&opened, json!({"query": "document"}))["units"]["length"],
        "in"
    );
}

#[test]
fn import_and_export_default_to_millimetres() {
    // STEP and IGES are written in millimetres unless asked otherwise.
    assert_eq!(
        ExportOptions::new(ExportFormat::Step).unit,
        LengthUnit::Millimeter
    );
    assert_eq!(LengthUnit::default(), LengthUnit::Millimeter);
    let mut doc = block();
    command(&mut doc, json!({"cmd": "export", "path": "part.step"}));
    command(&mut doc, json!({"cmd": "export", "path": "part.igs"}));
    command(
        &mut doc,
        json!({"cmd": "export", "path": "inch.step", "unit": "in"}),
    );
    assert_eq!(
        *doc.kernel().write_units.borrow(),
        [
            LengthUnit::Millimeter,
            LengthUnit::Millimeter,
            LengthUnit::Inch
        ]
    );

    // A mesh file without units is read in millimetres.
    assert_eq!(ReadOptions::default().unit_mm, 1.0);
    doc.kernel().files.borrow_mut().insert(
        "scan.stl".to_owned(),
        vec![ImportBody {
            name: None,
            color: None,
            shape: MockShape::imported("mesh", 4),
        }],
    );
    command(&mut doc, json!({"cmd": "import_file", "path": "scan.stl"}));
    assert_eq!(*doc.kernel().read_units.borrow(), [1.0]);

    // A DXF drawing that names no unit is in millimetres; its own unit
    // (here inches) is kept.
    let dir = mitcad_model::testing::unique_temp_dir("metric");
    let drawing = |units: i32| {
        format!(
            "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n{units}\n0\nENDSEC\n\
             0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n8\n0\n10\n0\n20\n0\n40\n1\n0\nENDSEC\n0\nEOF\n"
        )
    };
    let radius = |doc: &Document<MockKernel>, sketch: &str| {
        query(doc, json!({"query": "sketch", "uid": sketch}))["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["type"] == "circle")
            .map(|e| e["geometry"]["radius"].as_f64().unwrap())
    };
    for (units, expected) in [(0, 1.0), (1, 25.4), (4, 1.0)] {
        let path = dir.join(format!("circle{units}.dxf"));
        std::fs::write(&path, drawing(units)).unwrap();
        let mut doc = Document::new(MockKernel::default());
        command(&mut doc, json!({"cmd": "sketch.create"}));
        command(
            &mut doc,
            json!({"cmd": "sketch.import_dxf", "sketch": "F1", "path": path.to_str().unwrap()}),
        );
        assert_eq!(radius(&doc, "F1"), Some(expected), "$INSUNITS {units}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn thread_tables_list_iso_metric_first() {
    let doc = Document::new(MockKernel::default());
    let table = query(&doc, json!({"query": "thread_sizes"}));
    let standards = table["standards"].as_array().unwrap();
    assert_eq!(standards.len(), 9);
    assert_eq!(standards[0]["standard"], "iso_metric");
    assert_eq!(standards[0]["title"], "ISO Metric profile");
    assert_eq!(standards[0]["default"], true);
    assert_eq!(standards[1]["standard"], "unified");
    assert_eq!(standards[1]["default"], false);
    // Whitworth and NPT (mitcad#4) after them.
    assert_eq!(standards[2]["standard"], "whitworth");
    assert_eq!(standards[3]["standard"], "npt");
    // Tyre valve threads (mitcad#59) last.
    assert_eq!(standards[4]["standard"], "tyre_valve");
    // The forming screw threads of designs last.
    assert_eq!(standards[5]["standard"], "forming_screw");
    // The trapezoidal threads of lead screws (mitcad#449) after them.
    assert_eq!(standards[6]["standard"], "trapezoidal");
    assert_eq!(standards[6]["default_class_external"], "7e");
    // ISO 7-1's taper pipe threads (mitcad#733) after them.
    assert_eq!(standards[7]["standard"], "iso_taper");
    // A custom thread (mitcad#893) last: no sizes, no classes.
    assert_eq!(standards[8]["standard"], "custom");
    assert_eq!(standards[8]["title"], "Custom");
    assert_eq!(standards[8]["custom"], true);
    assert_eq!(standards[8]["sizes"], json!([]));
    assert_eq!(standards[8]["classes_external"], json!([]));
    assert!(standards[..8].iter().all(|s| s.get("custom").is_none()));
    // A taper pipe thread's sizes give their tap drills (mitcad#733).
    let quarter = standards[3]["sizes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["size"] == "1/4")
        .unwrap();
    let drill = quarter["tap_drill"].as_f64().unwrap();
    assert!((drill - 10.7206).abs() < 1e-3, "{quarter}");
    assert!(standards[0]["sizes"][0].get("tap_drill").is_none());
    assert_eq!(standards[0]["default_class_internal"], "6H");
    assert_eq!(standards[0]["default_class_external"], "6g");
    let m10 = standards[0]["sizes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["size"] == "10")
        .unwrap();
    assert_eq!(m10["designations"][0], "M10x1.5");
    assert_eq!(m10["major_diameter"], 10.0);
    // Asked for one standard.
    let unified = query(
        &doc,
        json!({"query": "thread_sizes", "standard": "unified"}),
    );
    assert_eq!(unified["standards"][0]["standard"], "unified");
    assert_eq!(unified["standards"].as_array().unwrap().len(), 1);
    let custom = query(&doc, json!({"query": "thread_sizes", "standard": "custom"}));
    assert_eq!(custom["standards"].as_array().unwrap().len(), 1);
    assert_eq!(custom["standards"][0]["standard"], "custom");
    // A custom size's default end chamfer: its minor diameter less P/10.
    let chamfer = query(
        &doc,
        json!({"query": "thread_chamfer", "standard": "custom", "major": 12, "pitch": 3}),
    );
    let minor = 12.0 - 1.25 * 3f64.sqrt() / 2.0 * 3.0;
    assert!(
        (chamfer["diameter"].as_f64().unwrap() - (minor - 0.3)).abs() < 1e-9,
        "{chamfer}"
    );

    // A thread or a tapped hole without a standard is ISO metric.
    assert_eq!(ThreadStandard::default(), ThreadStandard::IsoMetric);
    let size: mitcad_model::features::ThreadSize =
        serde_json::from_value(json!({"designation": "M6"})).unwrap();
    assert_eq!(size.standard, ThreadStandard::IsoMetric);
    assert_eq!(size.data().unwrap().pitch, 1.0);
}

#[test]
fn materials_are_in_grams_per_cubic_centimetre() {
    // Bodies without a material are steel, 7.85 g/cm³ (masses are kg and
    // moments of inertia kg mm², `mitcad_model::analysis`).
    let steel = material(DEFAULT_MATERIAL).unwrap();
    assert_eq!(steel.density, 7.85);
    assert_eq!(material("aluminum").map(|m| m.density), Some(2.70));
}

#[test]
fn a_new_design_can_start_in_other_units() {
    // mitcad#1036: the application's default units for new designs. A new
    // design is in millimetres; one made in inches starts in them, with no
    // undo step and not modified, as an initial state, not a change.
    let metric = Document::new_design(MockKernel::default());
    assert_eq!(
        query(&metric, json!({"query": "document"}))["units"]["length"],
        "mm"
    );
    let mut doc = Document::new_design_in(MockKernel::default(), ExprLength::Inch);
    assert_eq!(
        query(&doc, json!({"query": "document"}))["units"]["length"],
        "in"
    );
    assert_eq!(doc.undo_label(), None);
    assert_eq!(doc.undo_depth(), 0);
    assert!(!doc.is_modified());
    // The same arrangement as any new design: Component1 active.
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let mut metric_file: Value = serde_json::from_str(&metric.to_json()).unwrap();
    assert_eq!(file["units"]["length"], "in");
    file.as_object_mut().unwrap().remove("units");
    metric_file.as_object_mut().unwrap().remove("units");
    assert_eq!(file, metric_file);
    // Bare numbers are inches: a new extrude's distance of 1 is 25.4 mm.
    let length = query(&doc, json!({"query": "evaluate", "expression": "1"}));
    assert_eq!(length["value"], 25.4);
    assert_eq!(length["text"], "1 in");
    // The file opens in inches; the first change is the first undo step.
    let opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(
        query(&opened, json!({"query": "document"}))["units"]["length"],
        "in"
    );
    command(&mut doc, json!({"cmd": "set_units", "length": "mm"}));
    assert_eq!(doc.undo_label(), Some("Change Units to mm"));
    assert!(doc.is_modified());
    // The unit's names as set_units reads them.
    assert_eq!(
        mitcad_model::file::parse_length_unit("ft"),
        Ok(ExprLength::Foot)
    );
    assert!(mitcad_model::file::parse_length_unit("deg").is_err());
    assert!(mitcad_model::file::parse_length_unit("furlong").is_err());
}
