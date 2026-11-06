// SPDX-License-Identifier: MIT
//! A BUILD part's options (mitcad#801): its cut length is one of its
//! derived part's `parameters`, which `set_component_parameters` binds to
//! the design's parameters like an inserted component's options.

use std::sync::Arc;

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::BodyUid;
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, ValueInput};
use mitcad_model_api::AnalysisApi;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

#[test]
fn a_build_cut_length_works_through_parameters() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    all_ok(&doc);
    let profile = component(&t);
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Span", "expression": "700 mm"}),
    )
    .unwrap();
    // Its length bound to the design's parameter.
    let changed = command(
        &mut doc,
        json!({"cmd": "set_component_parameters", "component": profile,
               "parameters": {"L": "Span"}}),
    )
    .unwrap();
    assert_eq!(changed["moves"], json!([]));
    assert_eq!(changed["parameters"]["L"], "Span");
    all_ok(&doc);
    let length = |doc: &mitcad_model::Document<mitcad_model::testing::MockKernel>| {
        let part = derived(doc, profile);
        let shape = doc.body_shape(BodyUid::new(part, 0)).unwrap();
        let bounds = shape.bounds.unwrap();
        bounds.max[2] - bounds.min[2]
    };
    assert!((length(&doc) - 700.0).abs() < 1e-6);
    assert!((doc.part_info(profile).unwrap().length.unwrap().1 - 700.0).abs() < 1e-6);
    // The design's parameter changes: the part's length and its frames
    // follow, and the parts list names it at its length.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Span", "expression": "650 mm"}),
    )
    .unwrap();
    all_ok(&doc);
    assert!((length(&doc) - 650.0).abs() < 1e-6);
    let frames = doc.part_info(profile).unwrap().mounts;
    assert!(close(frames["end_b"].translation, [0.0, 0.0, 650.0]));
    let rows = query(&doc, json!({"query": "parts_list"}))["rows"].clone();
    assert_eq!(rows[0]["designation"], "Profile 30x30 B8 L=650");
    assert_eq!(rows[0]["options"]["parameters"]["L"], "Span");
    // A BUILD part's row is build_edit's.
    let error = command(
        &mut doc,
        json!({"cmd": "set_component_parameters", "component": profile,
               "config": "x"}),
    )
    .unwrap_err();
    assert!(error.contains("is a BUILD part"), "{error}");
    // A number again: its own value.
    command(
        &mut doc,
        json!({"cmd": "set_component_parameters", "component": profile,
               "parameters": {"L": 480}}),
    )
    .unwrap();
    assert!((length(&doc) - 480.0).abs() < 1e-6);
}

/// An open-table profile whose actual section follows its text option,
/// numeric offset, dimensionless quantity and angle. No real model needed.
fn text_option_profile() -> Document<MockKernel> {
    let mut source = Document::new(MockKernel::default());
    for (name, expression) in [
        ("Grade", "'A'"),
        ("Offset", "8 mm"),
        ("Quantity", "2"),
        ("Twist", "0 deg"),
        ("L", "1000 mm"),
        ("W", "if(Grade == 'B'; 30 mm; 20 mm) + Offset"),
        ("H", "Quantity * 5 mm + Twist / 1 deg * 1 mm"),
    ] {
        let mut add = json!({"cmd": "add_parameter", "name": name, "expression": expression});
        if name == "Grade" {
            add["values"] = json!(["'A'", "'B'"]);
        } else if name == "Offset" {
            add["min"] = json!("0 mm");
            add["max"] = json!("10 mm");
            add["step"] = json!("1 mm");
        }
        command(&mut source, add).unwrap();
    }
    let sketch = source.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = source
        .add_rectangle(
            sketch,
            [0.0, 0.0],
            &ValueInput::Name("W".into()),
            &ValueInput::Name("H".into()),
        )
        .unwrap()
        .region;
    let solid = source
        .add_feature(
            &def(
                json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                        "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    fixed_frame(
        &mut source,
        "end_a",
        [0.0; 3],
        [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0],
    );
    source
        .add_feature(
            &def(
                json!({"type": "joint_origin", "geometry": {"body": format!("{solid}.b0"),
                        "face": format!("{solid}:end({region})")}}),
            ),
            Some("mount:end_b"),
        )
        .unwrap();
    command(
        &mut source,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["Grade", "Offset", "L"], "closed": false, "default": "Standard",
            "rows": [{"name": "Standard", "values": {"Grade": "'A'", "Offset": "8 mm", "L": "1000 mm"}}],
            "rules": [{"name": "Grade B offset", "when": "Grade == 'B'",
                       "then": [{"option": "Offset", "max": "5 mm"}]},
                      {"name": "Grade B length", "when": "Grade == 'B'",
                       "then": [{"option": "L", "max": "1000 mm"}]}]}}),
    )
    .unwrap();
    source.recompute();
    all_ok(&source);
    let mut library = TestLibrary::default();
    library.parts.insert(
        ("text-profile".into(), V1.into()),
        Part {
            text: source.to_json(),
            build: Some(profile_build("I", 5.0, 20.0, &[])),
            part_number: String::new(),
            name: "Text option profile".into(),
            designation: "Profile {Grade}".into(),
        },
    );
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(Arc::new(library)));
    doc
}

#[test]
fn text_options_reach_build_plan_preview_shape_and_commit() {
    for (grade, width) in [("'A'", 24.0), ("'B'", 34.0), ("ParentGrade", 34.0)] {
        let mut doc = text_option_profile();
        command(
            &mut doc,
            json!({"cmd": "add_parameter", "name": "ParentGrade", "expression": "'B'"}),
        )
        .unwrap();
        command(
            &mut doc,
            json!({"cmd": "add_parameter", "name": "ParentOffset", "expression": "4 mm"}),
        )
        .unwrap();
        let mut selected = part("text-profile");
        let offset = if grade == "ParentGrade" {
            "ParentOffset"
        } else {
            "4 mm"
        };
        selected["parameters"] =
            json!({"Grade": grade, "Offset": offset, "Quantity": "3", "Twist": "90"});
        let mut fields = json!({"part": selected, "at": {"point": [0, 0, 0]}, "length": 600});
        let revision = doc.revision();
        let undo = doc.undo_depth();
        fields["query"] = json!("build_preview");
        let report = query(&doc, fields.clone());
        assert_eq!(report["problems"], json!([]), "{report}");
        assert_eq!(
            report["designation"],
            if grade == "'A'" {
                "Profile A"
            } else {
                "Profile B"
            }
        );
        let vector =
            |value: &Value| -> [f64; 3] { std::array::from_fn(|i| value[i].as_f64().unwrap()) };
        let min = vector(&report["box"]["min"]);
        let max = vector(&report["box"]["max"]);
        let size: [f64; 3] = std::array::from_fn(|i| max[i] - min[i]);
        // Bare 90 is degrees for the source's angle, not a length or radians;
        // Quantity stays dimensionless. Grade changes the real source section.
        assert!(close(size, [width, 105.0, 600.0]), "{grade}: {report}");
        fields.as_object_mut().unwrap().remove("query");
        fields["shape"] = json!("build_preview");
        let shown = doc.analysis_shape(&fields.to_string()).unwrap().unwrap();
        let bounds = shown.bounds.unwrap();
        assert!(close(
            std::array::from_fn(|i| bounds.max[i] - bounds.min[i]),
            size
        ));
        assert_eq!(doc.revision(), revision, "preview changed the document");
        assert_eq!(doc.undo_depth(), undo, "preview added an undo step");
        fields.as_object_mut().unwrap().remove("shape");
        let placed = place(&mut doc, fields);
        assert_eq!(
            placed["name"],
            if grade == "'A'" {
                "Profile A"
            } else {
                "Profile B"
            }
        );
        all_ok(&doc);
        let uid = derived(&doc, component(&placed));
        let committed = doc
            .body_shape(BodyUid::new(uid, 0))
            .unwrap()
            .bounds
            .unwrap();
        assert!(close(committed.min, bounds.min) && close(committed.max, bounds.max));
        assert_eq!(doc.undo_depth(), undo + 1, "placement must stay one edit");
        if grade == "ParentGrade" {
            command(
                &mut doc,
                json!({"cmd": "set_parameter", "name": "ParentGrade", "expression": "'A'"}),
            )
            .unwrap();
            all_ok(&doc);
            let bounds = doc
                .body_shape(BodyUid::new(uid, 0))
                .unwrap()
                .bounds
                .unwrap();
            assert!((bounds.max[0] - bounds.min[0] - 24.0).abs() < 1e-6);
        }
    }
}

#[test]
fn constant_build_options_cascade_but_bound_options_refuse_the_same_rule() {
    let mut doc = text_option_profile();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "ParentOffset", "expression": "8 mm"}),
    )
    .unwrap();
    let mut selected = part("text-profile");
    selected["parameters"] =
        json!({"Grade": "'B'", "Offset": "8 mm", "Quantity": "3", "Twist": "90"});
    let mut fields = json!({"part": selected, "at": {"point": [0, 0, 0]}, "length": 600});
    let original = doc.to_json();
    let revision = doc.revision();
    let undo = doc.undo_depth();
    fields["query"] = json!("build_preview");
    let report = query(&doc, fields.clone());
    assert_eq!(report["problems"], json!([]), "{report}");
    assert_eq!(
        report["designation"], "Profile B",
        "text naming must follow the configured source"
    );
    assert!(
        report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("Grade B offset")),
        "{report}"
    );
    let span = |value: &Value, axis: usize| {
        value["max"][axis].as_f64().unwrap() - value["min"][axis].as_f64().unwrap()
    };
    assert!(
        (span(&report["box"], 0) - 35.0).abs() < 1e-6,
        "constant Offset 8 must cascade to 5: {report}"
    );
    fields.as_object_mut().unwrap().remove("query");
    fields["shape"] = json!("build_preview");
    let preview = doc
        .analysis_shape(&fields.to_string())
        .unwrap()
        .unwrap()
        .bounds
        .unwrap();
    assert!(close(
        std::array::from_fn(|i| preview.max[i] - preview.min[i]),
        [35.0, 105.0, 600.0]
    ));
    assert_eq!(
        doc.to_json(),
        original,
        "a repaired preview must remain read-only"
    );
    assert_eq!(doc.revision(), revision);
    assert_eq!(doc.undo_depth(), undo);

    // The same numeric Offset supplied by a parent parameter is pinned.
    // Every entry point refuses it, even after the constant preview succeeded.
    let mut bound = fields.clone();
    bound["part"]["parameters"]["Offset"] = json!("ParentOffset");
    bound.as_object_mut().unwrap().remove("shape");
    bound["query"] = json!("build_preview");
    let report = query(&doc, bound.clone());
    assert!(
        report["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p.as_str().unwrap().contains("Grade B offset")),
        "{report}"
    );
    bound.as_object_mut().unwrap().remove("query");
    bound["shape"] = json!("build_preview");
    let error = doc
        .analysis_shape(&bound.to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("Grade B offset"), "{error}");
    bound.as_object_mut().unwrap().remove("shape");
    bound["cmd"] = json!("build_place");
    let error = command(&mut doc, bound).unwrap_err();
    assert!(error.contains("Grade B offset"), "{error}");
    assert_eq!(
        doc.to_json(),
        original,
        "refusal must leave the design unchanged"
    );
    assert_eq!(doc.undo_depth(), undo);

    fields.as_object_mut().unwrap().remove("shape");
    let placed = place(&mut doc, fields);
    all_ok(&doc);
    assert_eq!(placed["name"], "Profile B");
    let uid = derived(&doc, component(&placed));
    let mitcad_model::features::FeatureDef::DerivedPart(definition) =
        &doc.feature(uid).unwrap().def
    else {
        panic!("derived part");
    };
    assert!(
        (doc.parameters()
            .value(definition.parameters["Offset"])
            .unwrap()
            - 5.0)
            .abs()
            < 1e-6,
        "the repaired constant must be kept by the component"
    );
    let committed = doc
        .body_shape(BodyUid::new(uid, 0))
        .unwrap()
        .bounds
        .unwrap();
    assert!(close(committed.min, preview.min) && close(committed.max, preview.max));
    assert_eq!(doc.undo_depth(), undo + 1);
}

#[test]
fn repaired_cut_length_agrees_in_plan_report_preview_and_committed_geometry() {
    let mut doc = text_option_profile();
    let target = place(
        &mut doc,
        json!({"part": part("text-profile"), "at": {"point": [0, 0, 1200]}, "length": 100}),
    );
    let mut selected = part("text-profile");
    selected["parameters"] = json!({"Grade": "'B'", "Offset": "4 mm"});
    let mut fields = json!({"part": selected, "at": {"point": [0, 0, 0]}, "length": 1200});
    let original = doc.to_json();
    let undo = doc.undo_depth();
    let input = serde_json::from_value(fields.clone()).unwrap();
    let plan = doc.preview_place(&input).unwrap();
    let mitcad_model_build::internal::LengthPlan::Value(_, length) = plan.length else {
        panic!("plain length");
    };
    assert!((length - 1000.0).abs() < 1e-6);
    // The automatic ray suggestion is also repaired before its value is
    // reported, rather than showing 1200 while drawing 1000.
    let mut automatic = fields.clone();
    automatic.as_object_mut().unwrap().remove("length");
    automatic["query"] = json!("build_preview");
    let suggested = query(&doc, automatic.clone());
    assert_eq!(suggested["problems"], json!([]), "{suggested}");
    assert_eq!(
        suggested["length"]["target"]["occurrence"], target["occurrence"],
        "the ray must meet the actual target"
    );
    assert_eq!(suggested["length"]["value"], 1000.0, "{suggested}");
    automatic.as_object_mut().unwrap().remove("query");
    automatic["shape"] = json!("build_preview");
    let shape = doc
        .analysis_shape(&automatic.to_string())
        .unwrap()
        .unwrap()
        .bounds
        .unwrap();
    assert!((shape.max[2] - shape.min[2] - 1000.0).abs() < 1e-6);
    fields["query"] = json!("build_preview");
    let report = query(&doc, fields.clone());
    assert_eq!(report["problems"], json!([]), "{report}");
    assert_eq!(report["length"]["value"], 1000.0, "{report}");
    assert_eq!(report["box"]["max"][2], 1000.0, "{report}");
    fields.as_object_mut().unwrap().remove("query");
    fields["shape"] = json!("build_preview");
    let preview = doc
        .analysis_shape(&fields.to_string())
        .unwrap()
        .unwrap()
        .bounds
        .unwrap();
    assert!((preview.max[2] - preview.min[2] - 1000.0).abs() < 1e-6);
    assert_eq!(doc.to_json(), original);
    assert_eq!(doc.undo_depth(), undo);
    fields.as_object_mut().unwrap().remove("shape");
    let placed = place(&mut doc, fields);
    all_ok(&doc);
    let uid = derived(&doc, component(&placed));
    let committed = doc
        .body_shape(BodyUid::new(uid, 0))
        .unwrap()
        .bounds
        .unwrap();
    assert!(close(committed.min, preview.min) && close(committed.max, preview.max));
    assert!((doc.part_info(component(&placed)).unwrap().length.unwrap().1 - 1000.0).abs() < 1e-6);
}

#[test]
fn exact_geometry_bound_length_is_pinned_in_plan_preview_and_placement() {
    let mut doc = text_option_profile();
    let target = place(
        &mut doc,
        json!({"part": part("text-profile"), "at": {"point": [0, 0, 1200]}, "length": 100}),
    );
    let target_part = derived(&doc, component(&target));
    let mut selected = part("text-profile");
    selected["parameters"] = json!({"Grade": "'B'", "Offset": "4 mm"});
    let binding = json!({"bind": {"occurrence": target["occurrence"], "geometry": {"mount": format!("{target_part}:mount(end_a)")}}});
    let original = doc.to_json();
    let undo = doc.undo_depth();
    for mode in ["face", "centre", "through"] {
        let mut length = binding.clone();
        length["mode"] = json!(mode);
        let mut fields = json!({"part": selected, "at": {"point": [0, 0, 0]}, "length": length});
        let input = serde_json::from_value(fields.clone()).unwrap();
        let error = doc.preview_place(&input).unwrap_err().to_string();
        assert!(error.contains("Grade B length"), "{mode}: {error}");
        fields["query"] = json!("build_preview");
        let report = query(&doc, fields.clone());
        assert!(
            report["problems"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p.as_str().unwrap().contains("Grade B length")),
            "{mode}: {report}"
        );
        fields.as_object_mut().unwrap().remove("query");
        fields["shape"] = json!("build_preview");
        let error = doc
            .analysis_shape(&fields.to_string())
            .unwrap_err()
            .to_string();
        assert!(error.contains("Grade B length"), "{mode}: {error}");
        fields.as_object_mut().unwrap().remove("shape");
        fields["cmd"] = json!("build_place");
        let error = command(&mut doc, fields).unwrap_err();
        assert!(error.contains("Grade B length"), "{mode}: {error}");
        assert_eq!(doc.to_json(), original);
        assert_eq!(doc.undo_depth(), undo);
    }
    // An explicit gap is part of the same exact measurement: 1200 - 300
    // is valid, and its reported/reference length must be 900, not the ray's 1200.
    let mut length = binding;
    length["add"] = json!("-300 mm");
    let mut fields = json!({"part": selected, "at": {"point": [0, 0, 0]}, "length": length});
    let input = serde_json::from_value(fields.clone()).unwrap();
    let plan = doc.preview_place(&input).unwrap();
    let mitcad_model_build::internal::LengthPlan::Bound {
        first,
        source_default,
        ..
    } = plan.length
    else {
        panic!("geometry-bound length");
    };
    assert!((first - 900.0).abs() < 1e-6);
    assert!((source_default - 1000.0).abs() < 1e-6);
    fields["query"] = json!("build_preview");
    let report = query(&doc, fields.clone());
    assert_eq!(report["problems"], json!([]), "{report}");
    assert_eq!(report["length"]["value"], 900.0, "{report}");
    // The stock/default remains distinct from the measured preview body.
    assert_eq!(report["length"]["default"], 1000.0, "{report}");
    assert_eq!(report["box"]["max"][2], 900.0, "{report}");
    fields.as_object_mut().unwrap().remove("query");
    fields["shape"] = json!("build_preview");
    let preview = doc
        .analysis_shape(&fields.to_string())
        .unwrap()
        .unwrap()
        .bounds
        .unwrap();
    assert!((preview.max[2] - preview.min[2] - 900.0).abs() < 1e-6);
    fields.as_object_mut().unwrap().remove("shape");
    let placed = place(&mut doc, fields);
    all_ok(&doc);
    let uid = derived(&doc, component(&placed));
    let committed = doc
        .body_shape(BodyUid::new(uid, 0))
        .unwrap()
        .bounds
        .unwrap();
    assert!(close(committed.min, preview.min) && close(committed.max, preview.max));
    assert!((doc.part_info(component(&placed)).unwrap().length.unwrap().1 - 900.0).abs() < 1e-6);
}

#[test]
fn a_part_s_row_sets_the_suppression_its_source_binds() {
    // The source's second body is suppressed by a condition of its rows'
    // parameter (mitcad#873): placed in its default row the part is 40
    // wide, in the other row 80, and moved back to the default row 40.
    let mut doc = design();
    let narrow = first(&mut doc, "bound-rows", [0.0; 3], 500.0);
    let mut wide_part = part("bound-rows");
    wide_part["config"] = json!("wide");
    let wide = place(
        &mut doc,
        json!({"part": wide_part, "at": {"point": [300, 0, 0]}, "length": 500}),
    );
    all_ok(&doc);
    let width = |doc: &mitcad_model::Document<mitcad_model::testing::MockKernel>,
                 answer: &serde_json::Value| {
        let bounds = doc.part_bounds(component(answer)).unwrap();
        bounds.max[0] - bounds.min[0]
    };
    assert!((width(&doc, &narrow) - 40.0).abs() < 1e-6);
    assert!((width(&doc, &wide) - 80.0).abs() < 1e-6);
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": component(&wide), "config": "narrow"}),
    )
    .unwrap();
    all_ok(&doc);
    assert!((width(&doc, &wide) - 40.0).abs() < 1e-6);
}

/// A profile of a typed size (mitcad#1328, the steel tubes of any size): an
/// open table whose option W sets the section's width, its parameter A
/// the section's area, which the build block names as `area_parameter`.
fn typed_size_profile() -> Document<MockKernel> {
    let mut source = Document::new(MockKernel::default());
    for (name, expression) in [("W", "20 mm"), ("L", "1000 mm"), ("A", "W * 10 mm")] {
        let mut add = json!({"cmd": "add_parameter", "name": name, "expression": expression});
        if name == "W" {
            add["min"] = json!("10 mm");
            add["max"] = json!("100 mm");
        }
        command(&mut source, add).unwrap();
    }
    let sketch = source.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = source
        .add_rectangle(
            sketch,
            [0.0, 0.0],
            &ValueInput::Name("W".into()),
            &ValueInput::Number(10.0),
        )
        .unwrap()
        .region;
    let solid = source
        .add_feature(
            &def(
                json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                        "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    fixed_frame(
        &mut source,
        "end_a",
        [0.0; 3],
        [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0],
    );
    source
        .add_feature(
            &def(
                json!({"type": "joint_origin", "geometry": {"body": format!("{solid}.b0"),
                        "face": format!("{solid}:end({region})")}}),
            ),
            Some("mount:end_b"),
        )
        .unwrap();
    command(
        &mut source,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["W"], "closed": false, "default": "20",
            "rows": [{"name": "20", "values": {"W": "20 mm"}}]}}),
    )
    .unwrap();
    source.recompute();
    all_ok(&source);
    // The default row's mass per metre is 0.9 kg (profile_build's), which
    // the area at 7850 kg/m3 replaces.
    let mut build = profile_build("I", 5.0, 20.0, &[]);
    build["material"] = json!({"density": 7850});
    build["area_parameter"] = json!("A");
    let mut library = TestLibrary::default();
    library.parts.insert(
        ("typed-profile".into(), V1.into()),
        Part {
            text: source.to_json(),
            build: Some(build),
            part_number: String::new(),
            name: "Typed bar".into(),
            designation: "Bar {W}x10 L={L}".into(),
        },
    );
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(Arc::new(library)));
    doc
}

#[test]
fn a_typed_size_s_cut_list_mass_and_stock_follow_its_options() {
    let mut doc = typed_size_profile();
    place(
        &mut doc,
        json!({"part": part("typed-profile"), "at": {"point": [0, 0, 0]}, "length": 500}),
    );
    let mut typed = part("typed-profile");
    typed["parameters"] = json!({"W": "30 mm"});
    place(
        &mut doc,
        json!({"part": typed, "at": {"point": [300, 0, 0]}, "length": 500}),
    );
    all_ok(&doc);
    let list = query(&doc, json!({"query": "build_cut_list"}));
    let rows = list["rows"].as_array().unwrap();
    // Two rows of the same source, row and length: each its own size.
    assert_eq!(rows.len(), 2, "{list}");
    let row = |designation: &str| -> &Value {
        rows.iter()
            .find(|r| r["designation"] == designation)
            .unwrap_or_else(|| panic!("no row {designation}: {list}"))
    };
    // 20 x 10 mm and 30 x 10 mm at 7850 kg/m3, 500 mm long.
    let mass = |designation: &str| -> f64 { row(designation)["mass"].as_f64().unwrap() };
    assert!((mass("Bar 20x10 L=500") - 0.785).abs() < 1e-9, "{list}");
    assert!((mass("Bar 30x10 L=500") - 1.1775).abs() < 1e-9, "{list}");
    assert_eq!(row("Bar 30x10 L=500")["size"], "W 30");
    assert_eq!(row("Bar 30x10 L=500")["quantity"], 1);
    // Each size is cut from its own stock.
    let stock = list["stock"].as_array().unwrap();
    assert_eq!(stock.len(), 2, "{list}");
    assert!(
        stock
            .iter()
            .any(|s| s["designation"] == "Typed bar (W 30)" && s["needed"] == 1),
        "{list}"
    );
}
