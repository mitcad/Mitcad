// SPDX-License-Identifier: MIT
//! What the sheet-metal command UI asks of the model (mitcad#210), on the
//! mock kernel: a Face with rules of its own, picks as semantic references,
//! the browser's sheet bodies, the flat display choice and the DXF export.
//! Real bends are covered by the CLI fixture `sheet_flat_dxf`.

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::BodyUid;
use mitcad_model::test_support::num;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::{AnalysisApi, DocumentApi};
use serde_json::{Value, json};

fn rules() -> Value {
    json!({"thickness":"2 mm","bend_radius":"1 mm","allowance":{"type":"k_factor","k":0.44},
           "reference_side":"front","bend_relief":{"type":"round","width":1,"depth":3},
           "corner_relief":{"type":"rectangular","width":1,"depth":3}})
}

/// A 40 x 20 rectangle in sketch F1 and the command that makes a Face of
/// it with rules of its own.
fn sketch() -> (Document<MockKernel>, Value) {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(40.0), &num(20.0))
        .unwrap()
        .region;
    let command = json!({"cmd":"add_feature","def":{"type":"sheet_face","rules":rules(),
        "profiles":[{"sketch":sketch,"region":region}]}});
    (doc, command)
}

fn command(doc: &mut Document<MockKernel>, command: &Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|r| serde_json::from_str(&r).unwrap())
        .map_err(|e| e.to_string())
}

fn query(doc: &Document<MockKernel>, query: Value) -> Result<Value, String> {
    doc.query(&query.to_string())
        .map(|r| serde_json::from_str(&r).unwrap())
        .map_err(|e| e.to_string())
}

fn face_with_rules() -> (Document<MockKernel>, BodyUid) {
    let (mut doc, add) = sketch();
    let added = command(&mut doc, &add).unwrap();
    assert_eq!(added["uid"], "F3", "{added}");
    assert_eq!(added["rules"], "F2");
    (doc, "F3.b0".parse().unwrap())
}

#[test]
fn a_face_with_its_own_rules_is_one_undo_step() {
    let (mut doc, add) = sketch();
    // The preview evaluates both features on a copy and reports the Face.
    let preview: Value = serde_json::from_str(&doc.preview(&add.to_string()).unwrap()).unwrap();
    assert_eq!(preview["status"], "ok", "{preview}");
    assert_eq!(preview["uid"], "F3");
    assert_eq!(doc.features().count(), 1, "the preview adds nothing");
    let before = doc.undo_depth();
    let added = command(&mut doc, &add).unwrap();
    assert_eq!(added["name"], "SheetFace1");
    assert_eq!(doc.undo_depth(), before + 1);
    assert_eq!(doc.undo_label(), Some("Add SheetFace1"));
    let types: Vec<String> = doc
        .features()
        .map(|f| f.def.type_name().to_owned())
        .collect();
    assert_eq!(types, ["sketch", "sheet_rules", "sheet_face"]);
    assert!(
        doc.status("F3".parse().unwrap())
            .is_some_and(FeatureStatus::is_ok)
    );
    // The rules' thickness is the Face's own parameter.
    let rules = query(&doc, json!({"query":"sheet_rules","feature":"F2"})).unwrap();
    assert_eq!(rules["rules"]["thickness"], 2.0, "{rules}");
    doc.undo();
    assert_eq!(doc.features().count(), 1, "Undo takes both back");
    // A Face with an existing rules feature is unchanged; an inline
    // definition of another type is refused.
    let mut wrong = add.clone();
    wrong["def"]["rules"]["type"] = json!("extrude");
    assert!(command(&mut doc, &wrong).is_err());
    assert_eq!(doc.features().count(), 1);
}

#[test]
fn picks_of_a_sheet_resolve_to_domains_boundaries_and_seams() {
    let (doc, body) = face_with_rules();
    let reference = |extra: Value| {
        let mut q = json!({"query":"sheet_reference","body":body});
        q.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        query(&doc, q)
    };
    let front = reference(json!({"face":"F3:sheet_front(0)"})).unwrap();
    assert_eq!(
        front["domain"],
        json!({"kind":"panel","id":{"feature":"F3","slot":0}})
    );
    assert!(front.get("boundary").is_none());
    let wall = "F3:sheet_side0(F1:sheet_curve(c2[c1,c3]))";
    let side = reference(json!({ "face": wall })).unwrap();
    assert_eq!(
        side["boundary"],
        json!({"panel":{"feature":"F3","slot":0},
               "curve":{"type":"sketch","sketch":"F1","segment":"c2[c1,c3]"}}),
        "{side}"
    );
    assert_eq!(side["straight"], true);
    // The edge of the front face along that wall names the same boundary.
    let edge = reference(json!({ "edge": format!("E{{F3:sheet_front(0)|{wall}}}") })).unwrap();
    assert_eq!(edge["boundary"], side["boundary"]);
    // The vertical edge of two walls is the corner between them, in the
    // contour's direction whichever order the name has.
    let corner = reference(json!({ "edge": format!(
        "E{{F3:sheet_side0(F1:sheet_curve(c3[c2,c4]))|{wall}}}") }))
    .unwrap();
    let c = &corner["corner"];
    assert_eq!(c["panel"], json!({"feature":"F3","slot":0}), "{corner}");
    let curves = [
        c["incoming"]["segment"].clone(),
        c["outgoing"]["segment"].clone(),
    ];
    assert!(
        curves == [json!("c2[c1,c3]"), json!("c3[c2,c4]")]
            || curves == [json!("c3[c2,c4]"), json!("c2[c1,c3]")],
        "{corner}"
    );
    // Faces of other bodies or kinds are refused.
    assert!(reference(json!({"face":"F3:sheet_front(5)"})).is_err());
    assert!(reference(json!({"face":"F3:end(r{c1})"})).is_err());
    assert!(
        query(
            &doc,
            json!({"query":"sheet_reference","body":"F1.b0","face":"F3:sheet_front(0)"})
        )
        .is_err()
    );
}

#[test]
fn sheet_bodies_list_rules_and_flat_patterns_with_the_display_choice() {
    let (mut doc, body) = face_with_rules();
    let bodies = query(&doc, json!({"query":"sheet_bodies"})).unwrap();
    assert_eq!(bodies[0]["body"], "F3.b0", "{bodies}");
    assert_eq!(bodies[0]["rules"], "F2");
    assert_eq!(bodies[0]["rules_name"], "SheetRules1");
    assert_eq!(bodies[0]["thickness"], 2.0);
    assert_eq!(bodies[0]["k_factor"], 0.44);
    assert_eq!(bodies[0]["panels"], 1);
    assert_eq!(bodies[0]["flat_pattern"], Value::Null);
    command(&mut doc, &json!({"cmd":"set_flat_pattern","body":body})).unwrap();
    let bodies = query(&doc, json!({"query":"sheet_bodies"})).unwrap();
    let flat = &bodies[0]["flat_pattern"];
    assert_eq!(flat["status"], "valid", "{bodies}");
    assert_eq!(flat["area"], 800.0);
    assert_eq!(flat["shown"], false);
    // Showing the flat pattern is a display choice: an undo step of its own
    // that evaluates nothing again.
    let calls = doc.kernel().count("sheet_shape");
    command(
        &mut doc,
        &json!({"cmd":"set_flat_pattern","body":body,"shown":true}),
    )
    .unwrap();
    assert_eq!(doc.undo_label(), Some("Show Flat Pattern"));
    let bodies = query(&doc, json!({"query":"sheet_bodies"})).unwrap();
    assert_eq!(bodies[0]["flat_pattern"]["shown"], true);
    assert_eq!(doc.kernel().count("sheet_shape"), calls);
    let shape = json!({"shape":"flat_pattern","body":body}).to_string();
    assert!(doc.analysis_shape(&shape).unwrap().is_some());
    // It is saved with the item.
    let text = doc.to_json();
    assert!(text.contains("\"shown\": true"), "{text}");
    doc.undo();
    assert_eq!(
        query(&doc, json!({"query":"flat_pattern","body":body})).unwrap()["settings"].get("shown"),
        None
    );
}

#[test]
fn the_flat_pattern_exports_to_dxf_with_explicit_layers_and_units() {
    let (mut doc, body) = face_with_rules();
    let dir = mitcad_model::testing::unique_temp_dir("flat-dxf");
    let path = dir.join("flat.dxf").to_string_lossy().into_owned();
    // Without a Flat Pattern item the export uses a new item's settings.
    let name = doc.body_name(body);
    let exported = command(
        &mut doc,
        &json!({"cmd":"export_flat_pattern_dxf","body":name,"path":path}),
    )
    .unwrap();
    assert_eq!(exported["entities"], 4, "{exported}");
    assert_eq!(exported["unit"], "mm");
    assert_eq!(exported["bends"], json!({"up":0,"down":0}));
    assert_eq!(
        exported["bounds"],
        json!({"min":[0.0,0.0],"max":[40.0,20.0]})
    );
    let drawing = mitcad_dxf::read_file(&path).unwrap();
    assert_eq!(drawing.units, mitcad_dxf::Units::Millimeters);
    assert!(drawing.entities.iter().all(|e| e.layer == "CUT_OUTER"));
    for layer in ["CUT_OUTER", "CUT_INNER", "BEND_UP", "BEND_DOWN"] {
        assert!(drawing.layer(layer).is_some(), "{layer}");
    }
    // Inches, viewed from the back: mirrored, in the file's unit.
    let exported = command(
        &mut doc,
        &json!({"cmd":"export_flat_pattern_dxf","body":body,"path":path,"unit":"in",
                "reference_side":"back","tangents":true}),
    )
    .unwrap();
    assert_eq!(exported["reference_side"], "back");
    let drawing = mitcad_dxf::read_file(&path).unwrap();
    assert_eq!(drawing.units, mitcad_dxf::Units::Inches);
    let (min, max) = drawing.bounds().unwrap();
    assert!(
        (min.x + 40.0 / 25.4).abs() < 1e-9 && max.x.abs() < 1e-9,
        "{min:?} {max:?}"
    );
    assert!(drawing.layer("BEND_TANGENT").is_some());
    // Only .dxf files, and no drawing of an invalid pattern.
    assert!(
        command(
            &mut doc,
            &json!({"cmd":"export_flat_pattern_dxf","body":body,"path":"flat.txt"})
        )
        .is_err()
    );
    let missing = command(
        &mut doc,
        &json!({"cmd":"export_flat_pattern_dxf","body":"F9.b0","path":path}),
    );
    assert!(missing.is_err());
    std::fs::remove_dir_all(&dir).ok();
}
