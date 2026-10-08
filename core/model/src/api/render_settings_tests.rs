// SPDX-License-Identifier: MIT
//! Render settings (mitcad#47): the defaults, changes as undo steps that
//! recompute nothing, the project file and older files, the comparison of
//! versions.

use serde_json::{Value, json};

use crate::Document;
use crate::diff::diff_states;
use crate::testing::MockKernel;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: &str) -> Value {
    serde_json::from_str(&doc.query(&json!({"query": query}).to_string()).unwrap()).unwrap()
}

fn settings(doc: &Document<MockKernel>) -> Value {
    query(doc, "render_settings")
}

/// A 20 x 20 x 20 box (F1.b0).
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 20, "width": 20, "height": 20, "operation": "new_body"}}),
    );
    doc
}

#[test]
fn a_new_document_has_the_default_settings() {
    let doc = block();
    let s = settings(&doc);
    assert_eq!(
        s,
        json!({
            "environment": {"preset": "studio", "strength": 1.0, "rotation": 0.0,
                            "sun_elevation": std::f64::consts::PI / 4.0,
                            "sun_azimuth": 1.25 * std::f64::consts::PI},
            "background": {"mode": "view", "color": [1.0, 1.0, 1.0]},
            "ground": {"shadows": true, "reflections": false},
            "film": {"exposure": 0.0, "view_transform": "standard"},
            "output": {"width": 1920, "height": 1080, "aspect": "view", "samples": 128,
                       "time_limit": 0.0, "denoise": true, "transparent": false,
                       "format": "png", "quality": 90},
            "lights": []
        })
    );
}

#[test]
fn changes_are_undo_steps_that_recompute_nothing() {
    let mut doc = block();
    let computed = query(&doc, "document")["revision"].clone();
    let before = doc.state().clone();
    let result = command(
        &mut doc,
        json!({"cmd": "set_render_settings",
               "environment": {"preset": "outdoor", "sun_elevation": 0.3},
               "background": {"mode": "color", "color": [0.2, 0.3, 0.4]},
               "film": {"exposure": -1.5, "view_transform": "filmic"}}),
    );
    assert_eq!(result["changed"], true);
    assert_eq!(query(&doc, "document")["undo"], "Change Render Settings");
    assert_ne!(query(&doc, "document")["revision"], computed);
    let s = settings(&doc);
    assert_eq!(s["environment"]["preset"], "outdoor");
    assert_eq!(s["environment"]["sun_elevation"], 0.3);
    assert_eq!(s["environment"]["strength"], 1.0, "the others stay");
    assert_eq!(s["background"]["color"], json!([0.2, 0.3, 0.4]));
    assert_eq!(s["film"]["view_transform"], "filmic");
    assert_eq!(s["ground"]["shadows"], true);
    // Nothing was computed: the box is the same result.
    assert_eq!(doc.state().features, before.features);

    // The same values again: no undo step.
    let undo = query(&doc, "document")["undo_depth"].clone();
    let result = command(
        &mut doc,
        json!({"cmd": "set_render_settings", "film": {"exposure": -1.5}}),
    );
    assert_eq!(result["changed"], false);
    assert_eq!(query(&doc, "document")["undo_depth"], undo);

    // Ground: no shadows, a height; then back to the lowest body with null.
    command(
        &mut doc,
        json!({"cmd": "set_render_settings", "ground": {"shadows": false, "height": -12.5}}),
    );
    assert_eq!(
        settings(&doc)["ground"],
        json!({"shadows": false, "reflections": false, "height": -12.5})
    );
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        settings(&doc)["ground"],
        json!({"shadows": true, "reflections": false})
    );
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(settings(&doc)["ground"]["height"], -12.5);
    command(
        &mut doc,
        json!({"cmd": "set_render_settings", "ground": {"height": null}}),
    );
    assert!(settings(&doc)["ground"].get("height").is_none());

    // Reset: the defaults in one step, undone in one.
    let result = command(&mut doc, json!({"cmd": "reset_render_settings"}));
    assert_eq!(result["changed"], true);
    assert_eq!(query(&doc, "document")["undo"], "Reset Render Settings");
    assert_eq!(settings(&doc), settings(&block()));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(settings(&doc)["environment"]["preset"], "outdoor");
    let again = command(&mut doc, json!({"cmd": "reset_render_settings"}));
    assert_eq!(again["changed"], true);
    let none = command(&mut doc, json!({"cmd": "reset_render_settings"}));
    assert_eq!(none["changed"], false);
}

#[test]
fn wrong_settings_are_refused_and_change_nothing() {
    let mut doc = block();
    let before = settings(&doc);
    for (given, message) in [
        (
            json!({"environment": {"preset": "underwater"}}),
            "unknown variant `underwater`",
        ),
        (json!({"environment": {"strength": 500}}), "strength"),
        (
            json!({"film": {"exposure": "bright"}}),
            "film: invalid type",
        ),
        (
            json!({"ground": {"depth": 3}}),
            "ground: unknown field `depth`",
        ),
        (
            json!({"ground": {"depth": null}}),
            "ground: unknown field `depth`",
        ),
        (json!({"lights": {}}), "unknown field `lights`"),
        (json!({"background": "white"}), "invalid type"),
    ] {
        let mut cmd = given.clone();
        cmd["cmd"] = json!("set_render_settings");
        let error = refused(&mut doc, cmd);
        assert!(error.contains(message), "{given}: {error}");
    }
    assert_eq!(settings(&doc), before);
}

#[test]
fn settings_are_saved_and_read_back_and_older_files_open() {
    let mut doc = block();
    let plain = doc.to_json();
    assert!(!plain.contains("\"render\""), "the defaults: left out");
    let before = doc.state().clone();
    command(
        &mut doc,
        json!({"cmd": "set_render_settings",
               "environment": {"preset": "image", "image": "environments/hall.hdr",
                               "rotation": 1.5, "strength": 0.8},
               "background": {"mode": "environment"},
               "ground": {"reflections": true, "height": 4},
               "film": {"exposure": 0.5, "view_transform": "neutral"}}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["render"]["environment"]["image"],
        "environments/hall.hdr"
    );
    assert_eq!(file["render"]["ground"]["height"], 4.0);
    assert_eq!(file["render"]["film"]["view_transform"], "neutral");

    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    assert_eq!(settings(&loaded), settings(&doc));

    // An older file (without render settings) opens with the defaults.
    let old = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert_eq!(settings(&old), settings(&block()));

    // Fields and sections left out of a file are the defaults.
    let mut file: Value = serde_json::from_str(&plain).unwrap();
    file["render"] = json!({"film": {"exposure": 2}});
    let short = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    assert_eq!(settings(&short)["film"]["exposure"], 2.0);
    assert_eq!(settings(&short)["film"]["view_transform"], "standard");
    assert_eq!(settings(&short)["environment"]["preset"], "studio");

    // Broken settings are refused with where they are.
    for (broken, expected) in [
        (json!({"film": {"exposure": 20}}), "render: film.exposure"),
        (
            json!({"film": {"gamma": 2}}),
            "render: unknown field `gamma`",
        ),
        (json!({"environment": {"preset": 3}}), "render: "),
    ] {
        file["render"] = broken;
        let Err(error) = Document::from_json(&file.to_string(), MockKernel::default()) else {
            panic!("{} was read", file["render"]);
        };
        assert!(error.to_string().contains(expected), "{error}");
    }

    // The comparison of versions lists the changed fields.
    let diff = diff_states(&before, doc.state());
    let texts: Vec<&str> = diff.document.iter().map(|c| c.text.as_str()).collect();
    assert!(
        texts.contains(&"render.environment.preset studio -> image"),
        "{texts:?}"
    );
    assert!(
        texts.contains(&"render.ground.height (none) -> 4"),
        "{texts:?}"
    );
    assert!(!diff.identical);
    assert!(diff_states(doc.state(), doc.state()).identical);
}

#[test]
fn the_output_section_is_saved_compared_and_undone() {
    let mut doc = block();
    let before = doc.state().clone();
    command(
        &mut doc,
        json!({"cmd": "set_render_settings",
               "output": {"width": 800, "height": 600, "aspect": "fixed", "format": "exr",
                          "samples": 32, "transparent": true}}),
    );
    assert_eq!(query(&doc, "document")["undo"], "Change Render Settings");
    let s = settings(&doc);
    assert_eq!(s["output"]["format"], "exr");
    assert_eq!(s["output"]["denoise"], true, "the others stay");
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["render"]["output"]["width"], 800);
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(settings(&loaded), settings(&doc));
    let diff = diff_states(&before, doc.state());
    let texts: Vec<&str> = diff.document.iter().map(|c| c.text.as_str()).collect();
    assert!(
        texts.contains(&"render.output.format png -> exr"),
        "{texts:?}"
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "set_render_settings", "output": {"samples": 0}}),
    );
    assert!(error.contains("output.samples"), "{error}");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(settings(&doc), settings(&block()));
}

// Lights of the user's own (mitcad#54).

#[test]
fn lights_are_added_changed_and_deleted_as_undo_steps() {
    let mut doc = block();
    let computed = doc.state().features.clone();
    let added = command(
        &mut doc,
        json!({"cmd": "add_render_light", "type": "spot", "position": [100, -100, 150],
               "direction": [-1, 1, -1.5], "color": [1, 0.9, 0.8]}),
    );
    assert_eq!(added["id"], "light1");
    assert_eq!(query(&doc, "document")["undo"], "Add Light Light1");
    let light = &settings(&doc)["lights"][0];
    assert_eq!(light["type"], "spot");
    assert_eq!(light["name"], "Light1");
    assert_eq!(light["space"], "world");
    assert_eq!(light["power"], 5.0, "the kind's own power");
    assert_eq!(light["enabled"], true);
    // The direction is kept as a unit vector.
    let d: Vec<f64> = light["direction"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert!((d.iter().map(|c| c * c).sum::<f64>() - 1.0).abs() < 1e-12);
    assert!((d[2] / d[0] - 1.5).abs() < 1e-12);
    assert_eq!(doc.state().features, computed, "nothing is computed");

    // A sun gets a sun's power; ids and names count on.
    let sun = command(
        &mut doc,
        json!({"cmd": "add_render_light", "type": "sun", "direction": [0, 0, -1]}),
    );
    assert_eq!(sun["id"], "light2");
    assert_eq!(settings(&doc)["lights"][1]["power"], 2.0);
    assert_eq!(settings(&doc)["lights"][1]["name"], "Light2");

    // Changes: only the given fields; the same values are no undo step.
    let result = command(
        &mut doc,
        json!({"cmd": "edit_render_light", "id": "light1", "power": 25, "space": "camera",
               "name": "Key"}),
    );
    assert_eq!(result["changed"], true);
    assert_eq!(query(&doc, "document")["undo"], "Change Light Key");
    let light = &settings(&doc)["lights"][0];
    assert_eq!(light["power"], 25.0);
    assert_eq!(light["space"], "camera");
    assert_eq!(light["type"], "spot", "the others stay");
    let depth = query(&doc, "document")["undo_depth"].clone();
    let same = command(
        &mut doc,
        json!({"cmd": "edit_render_light", "id": "light1", "power": 25}),
    );
    assert_eq!(same["changed"], false);
    assert_eq!(query(&doc, "document")["undo_depth"], depth);
    // Null: the field's default.
    command(
        &mut doc,
        json!({"cmd": "edit_render_light", "id": "light1", "space": null, "color": null}),
    );
    assert_eq!(settings(&doc)["lights"][0]["space"], "world");
    assert_eq!(settings(&doc)["lights"][0]["color"], json!([1.0, 1.0, 1.0]));

    // Delete, undo, redo.
    command(
        &mut doc,
        json!({"cmd": "delete_render_light", "id": "light1"}),
    );
    assert_eq!(query(&doc, "document")["undo"], "Delete Light Key");
    assert_eq!(settings(&doc)["lights"].as_array().unwrap().len(), 1);
    assert_eq!(settings(&doc)["lights"][0]["id"], "light2");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(settings(&doc)["lights"][0]["name"], "Key");
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(settings(&doc)["lights"][0]["id"], "light2");
    // A new light takes the first free id.
    let again = command(&mut doc, json!({"cmd": "add_render_light"}));
    assert_eq!(again["id"], "light1");
    assert_eq!(settings(&doc)["lights"][1]["type"], "point");

    // Reset removes the lights in one step.
    command(&mut doc, json!({"cmd": "reset_render_settings"}));
    assert_eq!(settings(&doc)["lights"], json!([]));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(settings(&doc)["lights"].as_array().unwrap().len(), 2);
}

#[test]
fn wrong_lights_are_refused_and_change_nothing() {
    let mut doc = block();
    command(&mut doc, json!({"cmd": "add_render_light"}));
    let before = settings(&doc);
    for (given, message) in [
        (
            json!({"cmd": "add_render_light", "type": "laser"}),
            "unknown variant `laser`",
        ),
        (
            json!({"cmd": "add_render_light", "id": "light1"}),
            "a light 'light1' exists",
        ),
        (
            json!({"cmd": "add_render_light", "id": "a b"}),
            "no light id",
        ),
        (json!({"cmd": "add_render_light", "power": -1}), "power"),
        (
            json!({"cmd": "add_render_light", "direction": [0, 0, 0]}),
            "direction",
        ),
        (
            json!({"cmd": "add_render_light", "color": [2, 0, 0]}),
            "colour components",
        ),
        (
            json!({"cmd": "add_render_light", "position": [1e9, 0, 0]}),
            "position",
        ),
        (
            json!({"cmd": "add_render_light", "spot_angle": 0}),
            "spot_angle",
        ),
        (
            json!({"cmd": "add_render_light", "spot_blend": 2}),
            "spot_blend",
        ),
        (json!({"cmd": "add_render_light", "angle": 2}), "angle"),
        (json!({"cmd": "add_render_light", "size": -2}), "size"),
        (
            json!({"cmd": "add_render_light", "shape": "star"}),
            "unknown variant `star`",
        ),
        (
            json!({"cmd": "add_render_light", "space": "screen"}),
            "unknown variant `screen`",
        ),
        (
            json!({"cmd": "add_render_light", "brightness": 1}),
            "unknown field `brightness`",
        ),
        (
            json!({"cmd": "add_render_light", "name": " "}),
            "name is empty",
        ),
        (
            json!({"cmd": "edit_render_light", "id": "light9", "power": 1}),
            "no light 'light9'",
        ),
        (
            json!({"cmd": "edit_render_light", "power": 1}),
            "needs the light's id",
        ),
        (
            json!({"cmd": "edit_render_light", "id": "light1", "power": "bright"}),
            "invalid type",
        ),
        (
            json!({"cmd": "delete_render_light", "id": "light9"}),
            "no light 'light9'",
        ),
        (
            json!({"cmd": "set_render_settings", "lights": []}),
            "unknown field `lights`",
        ),
    ] {
        let error = refused(&mut doc, given.clone());
        assert!(error.contains(message), "{given}: {error}");
    }
    assert_eq!(settings(&doc), before);
}

#[test]
fn lights_are_saved_read_back_and_compared() {
    let mut doc = block();
    let plain = doc.to_json();
    let before = doc.state().clone();
    command(
        &mut doc,
        json!({"cmd": "add_render_light", "type": "area", "shape": "disc", "size": 80,
               "position": [0, -200, 120], "direction": [0, 1, -0.5], "power": 40}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_render_light", "type": "point", "space": "camera",
               "position": [-100, 100, 0], "color": [1, 0.5, 0.25]}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["render"]["lights"][0]["shape"], "disc");
    assert_eq!(file["render"]["lights"][1]["space"], "camera");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    assert_eq!(settings(&loaded), settings(&doc));

    // Files without lights (mitcad#47, #48) open without lights.
    let old = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert_eq!(settings(&old)["lights"], json!([]));
    let mut short: Value = serde_json::from_str(&plain).unwrap();
    short["render"] = json!({"ground": {"reflections": true}});
    let short = Document::from_json(&short.to_string(), MockKernel::default()).unwrap();
    assert_eq!(settings(&short)["lights"], json!([]));
    // A light's fields left out of a file are its defaults.
    let mut partial: Value = serde_json::from_str(&plain).unwrap();
    partial["render"] = json!({"lights": [{"id": "key", "name": "Key", "type": "sun"}]});
    let partial = Document::from_json(&partial.to_string(), MockKernel::default()).unwrap();
    assert_eq!(settings(&partial)["lights"][0]["enabled"], true);
    assert_eq!(
        settings(&partial)["lights"][0]["direction"],
        json!([0.0, 0.0, -1.0])
    );
    // Broken lights are refused.
    for (broken, expected) in [
        (
            json!({"lights": [{"id": "a", "name": "A", "power": -3}]}),
            "render: lights.a.power",
        ),
        (
            json!({"lights": [{"id": "a", "name": "A"}, {"id": "a", "name": "B"}]}),
            "two lights 'a'",
        ),
        (
            json!({"lights": [{"id": "a", "name": "A", "glow": 1}]}),
            "unknown field `glow`",
        ),
    ] {
        let mut file: Value = serde_json::from_str(&plain).unwrap();
        file["render"] = broken;
        let Err(error) = Document::from_json(&file.to_string(), MockKernel::default()) else {
            panic!("{} was read", file["render"]);
        };
        assert!(error.to_string().contains(expected), "{error}");
    }

    // The comparison of versions: lights added, removed and changed.
    let diff = diff_states(&before, doc.state());
    let texts: Vec<&str> = diff.document.iter().map(|c| c.text.as_str()).collect();
    assert!(
        texts.contains(&"render.lights.light1 (none) -> Light1 (area)"),
        "{texts:?}"
    );
    let changed = doc.state().clone();
    command(
        &mut doc,
        json!({"cmd": "edit_render_light", "id": "light1", "power": 60}),
    );
    command(
        &mut doc,
        json!({"cmd": "delete_render_light", "id": "light2"}),
    );
    let diff = diff_states(&changed, doc.state());
    let texts: Vec<&str> = diff.document.iter().map(|c| c.text.as_str()).collect();
    assert!(
        texts.contains(&"render.lights.light1.power 40 -> 60"),
        "{texts:?}"
    );
    assert!(
        texts.contains(&"render.lights.light2 Light2 (point) -> (none)"),
        "{texts:?}"
    );
}
