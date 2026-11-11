// SPDX-License-Identifier: MIT
//! Flat Pattern items and Unfold/Refold refusals, independent of OCCT
//! (mitcad#208). Developed geometry of real bends is covered by the CLI
//! sheet fixtures; graph development by `sheet/folded/develop_tests.rs`.

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

fn fixture() -> (Document<MockKernel>, FeatureUid, BodyUid) {
    let mut doc = Document::new(MockKernel::default());
    let rules = json!({"type":"sheet_rules","thickness":1,"bend_radius":2,
        "allowance":{"type":"k_factor","k":0.4},"reference_side":"front",
        "bend_relief":{"type":"round","width":1,"depth":3},
        "corner_relief":{"type":"rectangular","width":1,"depth":3}});
    let rules = doc.add_feature(&def(rules), None).unwrap().uid;
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(40.0), &num(20.0))
        .unwrap()
        .region;
    let face =
        json!({"type":"sheet_face","rules":rules,"profiles":[{"sketch":sketch,"region":region}]});
    let uid = doc.add_feature(&def(face), None).unwrap().uid;
    (doc, rules, BodyUid::new(uid, 0))
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn flat(doc: &Document<MockKernel>, body: BodyUid) -> Value {
    query(doc, json!({"query":"flat_pattern","body":body}))
}

/// Whether a text carries an internal feature id (`F26`), which text a
/// person sees never does (mitcad#1327).
fn has_feature_id(text: &str) -> bool {
    text.as_bytes()
        .windows(2)
        .any(|w| w[0] == b'F' && w[1].is_ascii_digit())
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<String, String> {
    doc.command(&command.to_string()).map_err(|e| e.to_string())
}

#[test]
fn flat_pattern_is_a_saved_undoable_item_and_never_a_body() {
    let (mut doc, _, body) = fixture();
    assert_eq!(flat(&doc, body)["status"], "unavailable");
    command(&mut doc, json!({"cmd":"set_flat_pattern","body":body})).unwrap();
    let valid = flat(&doc, body);
    assert_eq!(valid["status"], "valid", "{valid}");
    assert_eq!(
        valid["stationary"],
        json!({"feature":body.feature,"slot":0})
    );
    assert_eq!(valid["settings"]["reference_side"], "front");
    assert_eq!(valid["area"], 800.0);
    assert_eq!(valid["bounds"], json!({"min":[0.0,0.0],"max":[40.0,20.0]}));
    assert_eq!(valid["bends"], json!([]));
    assert_eq!(valid["regions"].as_array().unwrap().len(), 1);
    // A derived item: no extra body, and the same request reuses its result.
    assert_eq!(doc.bodies().len(), 1);
    let calls = doc.kernel().count("sheet_shape");
    assert_eq!(flat(&doc, body), valid);
    assert_eq!(doc.kernel().count("sheet_shape"), calls);
    assert!(doc.flat_pattern_shape(body).is_some());
    // Setting the same settings again is no undo step.
    let label = doc.undo_label().map(str::to_owned);
    command(&mut doc, json!({"cmd":"set_flat_pattern","body":body})).unwrap();
    assert_eq!(doc.undo_label().map(str::to_owned), label);
    doc.undo();
    assert_eq!(flat(&doc, body)["status"], "unavailable");
    doc.redo();
    assert_eq!(flat(&doc, body), valid);

    // The back view mirrors the drawing and puts its frame on the back.
    command(
        &mut doc,
        json!({"cmd":"set_flat_pattern","body":body,"reference_side":"back"}),
    )
    .unwrap();
    let back = flat(&doc, body);
    assert_eq!(back["bounds"], json!({"min":[-40.0,0.0],"max":[0.0,20.0]}));
    assert_eq!(back["frame"]["origin"], json!([0.0, 0.0, 1.0]));
    assert_eq!(back["frame"]["x_axis"], json!([-1.0, 0.0, 0.0]));
    assert_eq!(back["stationary"], valid["stationary"], "the panel is kept");

    // Saved and reopened with the design; evaluated again after recompute.
    let text = doc.to_json();
    assert!(text.contains("\"flat_patterns\""));
    let mut opened = Document::from_json(&text, MockKernel::default()).unwrap();
    opened.recompute();
    assert_eq!(flat(&opened, body)["bounds"], back["bounds"]);
    assert_eq!(flat(&opened, body)["settings"], back["settings"]);

    command(&mut doc, json!({"cmd":"remove_flat_pattern","body":body})).unwrap();
    assert_eq!(flat(&doc, body)["status"], "unavailable");
    assert!(command(&mut doc, json!({"cmd":"remove_flat_pattern","body":body})).is_err());
    doc.undo();
    assert_eq!(flat(&doc, body)["status"], "valid");
}

#[test]
fn flat_pattern_follows_later_edits_and_refuses_stale_or_blocked_sheets() {
    let (mut doc, _, body) = fixture();
    command(&mut doc, json!({"cmd":"set_flat_pattern","body":body})).unwrap();
    // A rigid move after the item was created: the drawing stays in the
    // fixed panel's material frame; only its placement follows.
    doc.add_feature(&def(json!({"type":"move","bodies":[body],"transform":{"type":"translate_xyz","x":4,"y":5,"z":6}})), None).unwrap();
    let moved = flat(&doc, body);
    assert_eq!(moved["bounds"], json!({"min":[0.0,0.0],"max":[40.0,20.0]}));
    assert_eq!(moved["frame"]["origin"], json!([4.0, 5.0, 6.0]));
    // The two-way correspondence follows the moved body.
    let at = query(
        &doc,
        json!({"query":"flat_point","body":body,"flat":[10.0,5.0]}),
    );
    assert_eq!(at["model"], json!([14.0, 10.0, 6.0]));
    let back = query(
        &doc,
        json!({"query":"flat_point","body":body,"model":[14.0,10.0,6.5]}),
    );
    assert_eq!(back["flat"], json!([10.0, 5.0]));
    assert_eq!(back["depth"], 0.5);
    assert!(
        doc.query(&json!({"query":"flat_point","body":body,"flat":[50.0,5.0]}).to_string())
            .is_err()
    );
    // An ordinary scale invalidates the sheet: no usable pattern. Its
    // reason names the feature in words, as a sheet feature's refusal
    // does, without ids (mitcad#1636); the ids are the `causes`.
    let scale = doc.add_feature(&def(json!({"type":"scale","bodies":[body],"point":{"type":"origin"},"scale":{"type":"uniform","factor":2}})), None).unwrap().uid;
    let invalid = flat(&doc, body);
    assert_eq!(invalid["status"], "invalid");
    assert!(invalid.get("regions").is_none(), "no export payload");
    let said = "this body is no longer sheet metal: Scale1 changed it in a way its sheet metal \
                cannot hold (the feature has no sheet-metal form); to cut across bends, use \
                Sheet Cut instead";
    assert_eq!(invalid["diagnostics"], json!([said]), "{invalid}");
    assert!(!has_feature_id(said));
    assert_eq!(invalid["causes"][0]["feature"], json!(scale), "{invalid}");
    // The browser's Flat Pattern row reads the same reason.
    let rows = query(&doc, json!({"query":"sheet_bodies"}));
    assert_eq!(
        rows[0]["flat_pattern"]["diagnostics"],
        json!([said]),
        "{rows}"
    );
    // A renamed feature is named by its new name.
    doc.rename_feature(scale, "Widen").unwrap();
    let renamed = flat(&doc, body)["diagnostics"][0]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        renamed.starts_with("this body is no longer sheet metal: Widen changed it"),
        "{renamed}"
    );
    doc.undo();
    doc.undo();
    assert_eq!(flat(&doc, body)["status"], "valid");
    // A later failure blocks the retained body and its pattern, naming the
    // feature that failed.
    doc.kernel().fail.borrow_mut().insert("transform");
    doc.add_feature(&def(json!({"type":"move","bodies":[body],"transform":{"type":"translate_xyz","x":1,"y":0,"z":0}})), None).unwrap();
    let blocked = flat(&doc, body);
    assert_eq!(blocked["status"], "blocked");
    let said = blocked["diagnostics"][0].as_str().unwrap();
    assert!(
        said.starts_with("this body's sheet metal is blocked: Move")
            && said.ends_with(" failed on it"),
        "{blocked}"
    );
    assert!(!has_feature_id(said), "{said}");
    doc.kernel().fail.borrow_mut().clear();
    doc.undo();
    assert_eq!(flat(&doc, body)["status"], "valid");
    // Rolling back before the Face leaves no sheet at the marker.
    doc.set_marker(2).unwrap();
    assert_eq!(flat(&doc, body)["status"], "unavailable");
}

#[test]
fn flat_pattern_settings_are_checked_against_the_sheet() {
    let (mut doc, rules, body) = fixture();
    let error = command(
        &mut doc,
        json!({"cmd":"set_flat_pattern","body":body,"stationary":{"feature":body.feature,"slot":4}}),
    )
    .unwrap_err();
    assert!(error.contains("not a panel"), "{error}");
    let error = command(
        &mut doc,
        json!({"cmd":"set_flat_pattern","body":BodyUid::new(rules, 0)}),
    )
    .unwrap_err();
    assert!(error.contains("not a sheet-metal body"), "{error}");
    let error = command(
        &mut doc,
        json!({"cmd":"set_flat_pattern","body":body,"x_axis":{"type":"edge"}}),
    )
    .unwrap_err();
    assert!(!error.is_empty());
}

#[test]
fn unfold_and_refold_refuse_unknown_bends_and_unrelated_operations() {
    let (mut doc, rules, body) = fixture();
    let face = body.feature;
    let panel = json!({"feature":face,"slot":0});
    let empty = json!({"type":"sheet_unfold","body":body,"stationary":panel,"bends":[]});
    let error = doc.add_feature(&def(empty), None).unwrap_err().to_string();
    assert!(error.contains("nonempty"), "{error}");
    let twice = json!({"type":"sheet_unfold","body":body,"stationary":panel,
        "bends":[{"feature":face,"slot":0},{"feature":face,"slot":0}]});
    let error = doc.add_feature(&def(twice), None).unwrap_err().to_string();
    assert!(error.contains("more than once"), "{error}");
    // A Face has no bends: the Unfold fails and blocks its intended sheet.
    let unknown = json!({"type":"sheet_unfold","body":body,"stationary":panel,
        "bends":[{"feature":face,"slot":0}]});
    let unfold = doc.add_feature(&def(unknown), None).unwrap().uid;
    let Some(FeatureStatus::Failed(message)) = doc.status(unfold) else {
        panic!("an Unfold of a missing bend fails");
    };
    assert!(message.contains("not a bend"), "{message}");
    assert_eq!(
        query(&doc, json!({"query":"sheet_state","body":body}))["status"],
        "blocked"
    );
    // Refold names an earlier Unfold of the same body.
    let error = doc
        .add_feature(
            &def(json!({"type":"sheet_refold","body":body,"unfold":rules})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("not a sheet Unfold"), "{error}");
    let refold = doc
        .add_feature(
            &def(json!({"type":"sheet_refold","body":body,"unfold":unfold})),
            None,
        )
        .unwrap()
        .uid;
    assert!(matches!(doc.status(refold), Some(FeatureStatus::Failed(_))));
    // Suppressing the failed Unfold: the Refold then has nothing to close.
    doc.set_suppressed(unfold, true).unwrap();
    let Some(FeatureStatus::Failed(message)) = doc.status(refold) else {
        panic!("nothing to refold");
    };
    assert!(message.contains("no open bends"), "{message}");
    let reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(
        reopened.feature(refold).unwrap().def,
        doc.feature(refold).unwrap().def
    );
}

#[test]
fn sheet_cut_refuses_sketches_off_the_fixed_panel_and_blocks_its_sheet() {
    let (mut doc, _, body) = fixture();
    let face = body.feature;
    let off = doc.add_sketch(SketchPlane::Xz).unwrap().uid;
    let region = doc
        .add_rectangle(off, [5.0, 0.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let cut = json!({"type":"sheet_cut","body":body,"stationary":{"feature":face,"slot":0},
        "profiles":[{"sketch":off,"region":region}]});
    let uid = doc.add_feature(&def(cut), None).unwrap().uid;
    let Some(FeatureStatus::Failed(message)) = doc.status(uid) else {
        panic!("a cut sketch across the sheet is refused");
    };
    assert!(message.contains("not on a surface"), "{message}");
    assert_eq!(
        query(&doc, json!({"query":"sheet_state","body":body}))["status"],
        "blocked"
    );
    doc.undo();
    let twice = json!({"type":"sheet_cut","body":body,"stationary":{"feature":face,"slot":0},
        "open_bends":[{"feature":face,"slot":0},{"feature":face,"slot":0}],
        "profiles":[{"sketch":off,"region":region}]});
    let error = doc.add_feature(&def(twice), None).unwrap_err().to_string();
    assert!(error.contains("more than once"), "{error}");
    let none = json!({"type":"sheet_cut","body":body,"stationary":{"feature":face,"slot":0},
        "profiles":[]});
    assert!(doc.add_feature(&def(none), None).is_err());
}

#[test]
fn corner_relief_names_two_distinct_bends_of_the_sheet() {
    // mitcad#209: the relief geometry is covered by the CLI fixtures; here
    // the definition, its refusals and their blocking of the sheet.
    let (mut doc, _, body) = fixture();
    let face = body.feature;
    let bend = |slot: u32| json!({"feature":face,"slot":slot});
    let one = json!({"type":"sheet_corner_relief","body":body,"bends":[bend(0)]});
    let error = doc.add_feature(&def(one), None).unwrap_err().to_string();
    assert!(error.contains("exactly two bends"), "{error}");
    let twice = json!({"type":"sheet_corner_relief","body":body,"bends":[bend(0),bend(0)]});
    let error = doc.add_feature(&def(twice), None).unwrap_err().to_string();
    assert!(error.contains("more than once"), "{error}");
    let relief = json!({"type":"sheet_corner_relief","body":body,"bends":[bend(0),bend(1)],
        "relief":{"type":"round","width":4,"depth":1}});
    let uid = doc.add_feature(&def(relief), None).unwrap().uid;
    let Some(FeatureStatus::Failed(message)) = doc.status(uid) else {
        panic!("a Face has no bends to relieve");
    };
    assert!(message.contains("not a bend"), "{message}");
    assert_eq!(
        query(&doc, json!({"query":"sheet_state","body":body}))["status"],
        "blocked"
    );
    // The size override is a parameter of the feature, saved with it.
    let reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(
        reopened.feature(uid).unwrap().def,
        doc.feature(uid).unwrap().def
    );
    assert!(doc.to_json().contains("sheet_corner_relief"));
}

#[test]
fn sheet_cut_takes_profiles_of_several_sketches() {
    let (mut doc, _, body) = fixture();
    let face = body.feature;
    let first = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let a = doc
        .add_rectangle(first, [2.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let second = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let b = doc
        .add_rectangle(second, [20.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let cut = json!({"type":"sheet_cut","body":body,"stationary":{"feature":face,"slot":0},
        "profiles":[{"sketch":first,"region":a},{"sketch":second,"region":b}]});
    let uid = doc.add_feature(&def(cut), None).unwrap().uid;
    // The mock kernel has no planar trim: the definition is accepted and the
    // evaluation reaches the kernel, which reports what it cannot do.
    let Some(FeatureStatus::Failed(message)) = doc.status(uid) else {
        panic!("the mock kernel cannot trim");
    };
    assert!(!message.contains("one sketch"), "{message}");
}

#[test]
fn paste_new_copies_the_flat_pattern_items_of_the_copied_bodies() {
    use mitcad_model::Transform;
    use mitcad_model::ids::ComponentUid;
    use mitcad_model::sheet::flat::FlatPatternDef;
    let mut doc = Document::new(MockKernel::default());
    let rules = json!({"type":"sheet_rules","thickness":1,"bend_radius":2,
        "allowance":{"type":"k_factor","k":0.4},"reference_side":"front",
        "bend_relief":{"type":"round","width":1,"depth":3},
        "corner_relief":{"type":"rectangular","width":1,"depth":3}});
    let rules = doc.add_feature(&def(rules), None).unwrap().uid;
    let (component, occurrence) = doc
        .create_component(Some("Bracket"), Transform::IDENTITY, true)
        .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(40.0), &num(20.0))
        .unwrap()
        .region;
    let face =
        json!({"type":"sheet_face","rules":rules,"profiles":[{"sketch":sketch,"region":region}]});
    let face = doc.add_feature(&def(face), None).unwrap().uid;
    let body = BodyUid::new(face, 0);
    command(
        &mut doc,
        json!({"cmd":"set_flat_pattern","body":body,"reference_side":"back","shown":true}),
    )
    .unwrap();
    let original = flat(&doc, body);
    assert_eq!(original["status"], "valid", "{original}");
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (copy, _) = doc
        .paste_new(occurrence, Some(Transform::IDENTITY))
        .unwrap();
    assert_ne!(copy, component);
    let copy_body = doc
        .bodies()
        .iter()
        .find(|b| doc.body_component(b.uid) == Some(copy))
        .unwrap()
        .uid;
    assert_ne!(copy_body.feature, face);
    // The copy's item: its own body and fixed panel, the same settings and
    // the same drawing.
    let item: &FlatPatternDef = &doc.flat_pattern_defs()[&copy_body];
    assert_eq!(item.stationary.feature, copy_body.feature);
    assert_eq!(item.stationary.slot, 0);
    assert_eq!(item, &{
        let mut expected = doc.flat_pattern_defs()[&body].clone();
        expected.stationary.feature = copy_body.feature;
        expected
    });
    let copied = flat(&doc, copy_body);
    assert_eq!(copied["status"], "valid", "{copied}");
    for field in ["area", "bounds"] {
        assert_eq!(copied[field], original[field], "{field}");
    }
    // The same contours; their curves are the copied sketch's.
    let curves = |flat: &Value| -> Vec<Value> {
        flat["regions"][0]["loops"][0]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["curve"].clone())
            .collect()
    };
    assert_eq!(curves(&copied), curves(&original));
    assert_ne!(
        copied["regions"][0]["loops"][0]["edges"][0]["piece"]["curve"]["sketch"],
        original["regions"][0]["loops"][0]["edges"][0]["piece"]["curve"]["sketch"]
    );
    assert_eq!(doc.flat_pattern_defs().len(), 2);
    // The items are independent, saved, and one undo step with the copy.
    command(&mut doc, json!({"cmd":"remove_flat_pattern","body":body})).unwrap();
    assert_eq!(flat(&doc, copy_body)["status"], "valid");
    let mut opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    opened.recompute();
    assert_eq!(opened.flat_pattern_defs(), doc.flat_pattern_defs());
    doc.undo();
    doc.undo();
    assert_eq!(doc.flat_pattern_defs().len(), 1);
    assert!(doc.flat_pattern_defs().contains_key(&body));
}

/// What the application shows of a model's error: the text before its
/// tail, as `userText` (app/framework/UserText.hpp) takes it, without the
/// model's cleaner of older texts.
fn shown(error: &str) -> &str {
    error.split("\n[placed: ").next().unwrap_or(error)
}

/// The Flat Pattern's errors a person reads name the body as the Browser
/// does, or say "this body", and never carry an id (mitcad#1639, the rule
/// of mitcad#1327); agents read the ids in the error's detail.
#[test]
fn flat_pattern_errors_name_the_body_in_words() {
    use mitcad_model::notice::{FeatureError, user_text_leaks};
    let (mut doc, rules, body) = fixture();
    doc.rename_body(body, "Bracket").unwrap();
    let plain = |error: &str| {
        let said = shown(error).to_owned();
        assert!(!has_feature_id(&said), "{said}");
        assert_eq!(user_text_leaks(&said), None, "{said}");
        said
    };
    // The Flat Pattern panel's refusals.
    let error = command(
        &mut doc,
        json!({"cmd":"set_flat_pattern","body":body,"stationary":{"feature":body.feature,"slot":4}}),
    )
    .unwrap_err();
    assert_eq!(
        plain(&error),
        "the chosen fixed panel is not a panel of Bracket"
    );
    assert!(
        FeatureError::from_text(&error)
            .agent_text()
            .contains(&format!("slot 4 is not a panel of sheet {body}")),
        "{error}"
    );
    let error = command(
        &mut doc,
        json!({"cmd":"set_flat_pattern","body":BodyUid::new(rules, 0)}),
    )
    .unwrap_err();
    assert_eq!(
        plain(&error),
        "this body is not a sheet-metal body at the timeline marker"
    );
    let error = command(&mut doc, json!({"cmd":"remove_flat_pattern","body":body})).unwrap_err();
    assert_eq!(plain(&error), "Bracket has no Flat Pattern");
    assert_eq!(
        flat(&doc, body)["diagnostics"],
        json!(["this body has no Flat Pattern"])
    );
    assert_eq!(
        flat(&doc, body)["details"],
        json!([format!("body {body} has no Flat Pattern")])
    );

    // The export of an invalid sheet's Flat Pattern: with the item's own
    // settings and with others (evaluated anew), the same words.
    command(&mut doc, json!({"cmd":"set_flat_pattern","body":body})).unwrap();
    let scale = doc.add_feature(&def(json!({"type":"scale","bodies":[body],"point":{"type":"origin"},"scale":{"type":"uniform","factor":2}})), None).unwrap().uid;
    let dir = mitcad_model::testing::unique_temp_dir("flat-names");
    let path = dir.join("flat.dxf").to_string_lossy().into_owned();
    let said = "the Flat Pattern of Bracket is invalid: this body is no longer sheet metal: \
                Scale1 changed it in a way its sheet metal cannot hold (the feature has no \
                sheet-metal form); to cut across bends, use Sheet Cut instead";
    for side in ["front", "back"] {
        let error = command(
            &mut doc,
            json!({"cmd":"export_flat_pattern_dxf","body":body,"path":path,"reference_side":side}),
        )
        .unwrap_err();
        assert_eq!(plain(&error), said, "{side}");
        // Agents read the body's and the feature's ids.
        let agents = FeatureError::from_text(&error).agent_text().to_owned();
        assert!(
            agents.starts_with(&format!("the Flat Pattern of {body} is invalid: ")),
            "{agents}"
        );
        assert!(agents.contains(&format!("[{scale}: ")), "{agents}");
    }
    assert!(!std::path::Path::new(&path).exists(), "nothing is written");
    // A body that is not there at the marker: "this body", never its id.
    let error = command(
        &mut doc,
        json!({"cmd":"export_flat_pattern_dxf","body":"F9.b0","path":path}),
    )
    .unwrap_err();
    assert_eq!(
        plain(&error),
        "this body is not a sheet-metal body at the timeline marker"
    );
    std::fs::remove_dir_all(&dir).ok();
}
