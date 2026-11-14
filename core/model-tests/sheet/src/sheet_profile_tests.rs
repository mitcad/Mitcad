// SPDX-License-Identifier: MIT
//! A Flange's profile sketch (mitcad#1648, GitHub mitcad/Mitcad#237),
//! independent of OCCT: `sheet_flange_profile` makes the sketch on the wall
//! from the flange's edge, length and width (a fixed plane and a sketch
//! before the flange, one undo step), and moves an existing profile's plane
//! back onto the wall; the flange reads the profile before any geometry and
//! refuses one across the bend line, or one given with offsets, in words.
//! The profile's plane follows the wall (mitcad#1776): after the Face is
//! narrowed or the flange's angle changed, and in a design from an
//! earlier version once it is opened.
//! The mock kernel trims no sheet domains, so the walls a profile makes,
//! their volumes and their Flat Pattern are the CLI test's
//! (`tools/cli/tests/sheet_flange_profile.json`).

use mitcad_model::features::{SketchPlane, ValueInput};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, SketchFrame};
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

/// The sheet of the flat pattern tests: t = 1, r = 2, K = 0.4, a 40 x 20
/// Face on XY; its sketch's uid too.
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
    (doc, sketch, BodyUid::new(uid, 0))
}

/// A flange of 90 degrees and 15 mm on the Face's edge x = 40 (20 mm long).
fn flange(body: BodyUid, sketch: FeatureUid) -> Value {
    json!({"type":"sheet_flange","body":body,
        "boundary":{"panel":{"feature":body.feature,"slot":0},
            "curve":{"type":"sketch","sketch":sketch,"segment":"c2[c1,c3]"}},
        "angle":std::f64::consts::FRAC_PI_2,"length":{"mode":"tangent","value":15},
        "position":"inside"})
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&text).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn uid(value: &Value) -> FeatureUid {
    serde_json::from_value(value.clone()).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6
}

/// Whether a text carries an internal feature id (`F26`), which text a
/// person sees never does (mitcad#1327).
fn has_feature_id(text: &str) -> bool {
    text.as_bytes()
        .windows(2)
        .any(|w| w[0] == b'F' && w[1].is_ascii_digit())
}

fn failure(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match doc.status(uid) {
        Some(FeatureStatus::Failed(message)) => message.to_string(),
        other => panic!("{uid} did not fail: {other:?}"),
    }
}

/// The timeline position of a feature.
fn position(doc: &Document<MockKernel>, uid: FeatureUid) -> usize {
    doc.features().position(|f| f.uid == uid).unwrap()
}

#[test]
fn flange_profile_sketch_is_made_on_the_wall_and_read_by_the_flange() {
    let (mut doc, base, body) = fixture();
    let plain = flange(body, base);
    // The profile: a fixed plane on the wall and a sketch on it, at the
    // marker, in one undo step.
    let depth = query(&doc, json!({"query":"document"}))["undo_depth"].clone();
    let made = command(&mut doc, json!({"cmd":"sheet_flange_profile","def":plain}));
    assert_eq!(made["added"], 2, "{made}");
    let (sketch, plane) = (uid(&made["sketch"]), uid(&made["plane"]));
    assert!(position(&doc, plane) < position(&doc, sketch));
    assert!(
        doc.undo_label().is_some_and(|l| l.contains("profile")),
        "{:?}",
        doc.undo_label()
    );
    assert_eq!(
        query(&doc, json!({"query":"document"}))["undo_depth"],
        json!(depth.as_i64().unwrap() + 1)
    );
    // The sketch on the wall: its origin at the edge's middle, x along the
    // edge, y up the wall (normal to the Face), and one rectangle 20 along
    // the bend line and 15 up the wall.
    let output = doc.sketch_output(sketch).unwrap();
    let frame = output.frame;
    assert!(close(frame.origin[1], 10.0), "{frame:?}");
    assert!(close(frame.x_axis[1].abs(), 1.0), "{frame:?}");
    assert!(close(frame.y_axis[2].abs(), 1.0), "{frame:?}");
    assert_eq!(output.regions.len(), 1);
    let (min, max) = output.regions[0].bounds();
    assert!(close(min[0], -10.0) && close(min[1], 0.0), "{min:?}");
    assert!(close(max[0], 10.0) && close(max[1], 15.0), "{max:?}");

    // The sketch edited: its side at x = 10 slanted 10 degrees inward,
    // constrained by its angle to the side on the bend line (100 degrees
    // from it), its length kept at 15.
    let shown = query(&doc, json!({"query":"sketch","uid":sketch}));
    let vertical = shown["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "vertical" && c["line"] == "c2")
        .unwrap_or_else(|| panic!("no vertical c2 in {shown}"))["id"]
        .clone();
    command(
        &mut doc,
        json!({"cmd":"sketch.remove","sketch":sketch,"items":[vertical]}),
    );
    command(
        &mut doc,
        json!({"cmd":"sketch.add_dimension","sketch":sketch,
            "dimension":{"type":"angle","a":"c1","b":"c2","reverse_a":false,"reverse_b":false},
            "value":100f64.to_radians()}),
    );
    let (min, max) = doc.sketch_output(sketch).unwrap().regions[0].bounds();
    assert!(close(max[1], 15.0 * 100f64.to_radians().sin()), "{max:?}");
    assert!(close(max[0] - min[0], 20.0), "{min:?} {max:?}");

    // A sketch whose outline crosses the bend line, on the same plane.
    let crossing = uid(&command(&mut doc, json!({"cmd":"sketch.create","plane":plane}))["uid"]);
    command(
        &mut doc,
        json!({"cmd":"sketch.add_rectangle","sketch":crossing,"corner":[-5,-2],"width":10,"height":8}),
    );

    // The flange names the profile; the sketch is in its references, so
    // the timeline keeps it before the flange.
    let mut profiled = plain.clone();
    profiled["profile"] = json!(sketch);
    let flange_uid = doc.add_feature(&def(profiled.clone()), None).unwrap().uid;
    let feature = query(&doc, json!({"query":"feature","uid":flange_uid}));
    assert_eq!(feature["def"]["profile"], json!(sketch), "{feature}");
    assert!(position(&doc, sketch) < position(&doc, flange_uid));
    // (The mock kernel trims no sheet domains: the profile was read, the
    // trim of the panel then fails.)
    let message = failure(&doc, flange_uid);
    assert!(!message.contains("profile"), "{message}");

    // A profile across the bend line is refused in words, without ids.
    let mut across = plain.clone();
    across["profile"] = json!(crossing);
    doc.edit_feature(flange_uid, &def(across)).unwrap();
    let message = failure(&doc, flange_uid);
    assert!(message.contains("crosses the bend line"), "{message}");
    assert!(!has_feature_id(&message), "{message}");
    doc.undo();

    // A profile with a width or offsets is refused: the profile draws them.
    let mut both = profiled.clone();
    both["offsets"] = json!({"start":1,"end":1});
    doc.edit_feature(flange_uid, &def(both)).unwrap();
    let message = failure(&doc, flange_uid);
    assert!(message.contains("takes no offsets"), "{message}");
    doc.undo();

    // Saved and opened again: the flange still names its profile.
    let opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    let feature = query(&opened, json!({"query":"feature","uid":flange_uid}));
    assert_eq!(feature["def"]["profile"], json!(sketch), "{feature}");
}

#[test]
fn flange_profile_left_by_another_angle_is_moved_back_onto_the_wall() {
    let (mut doc, base, body) = fixture();
    let plain = flange(body, base);
    let made = command(&mut doc, json!({"cmd":"sheet_flange_profile","def":plain}));
    let sketch = uid(&made["sketch"]);
    let before = doc.sketch_output(sketch).unwrap().frame;

    // The flange at another angle, with the profile: the command moves the
    // profile's plane onto that wall and adds nothing.
    let mut profiled = plain.clone();
    profiled["profile"] = json!(sketch);
    profiled["angle"] = json!(100f64.to_radians());
    let moved = command(
        &mut doc,
        json!({"cmd":"sheet_flange_profile","def":profiled}),
    );
    assert_eq!(moved["added"], 0, "{moved}");
    assert_eq!(moved["sketch"], json!(sketch));
    assert!(
        doc.undo_label()
            .is_some_and(|l| l.contains("onto the flange's wall")),
        "{:?}",
        doc.undo_label()
    );
    let after = doc.sketch_output(sketch).unwrap().frame;
    assert!(
        close(after.y_axis[2].abs(), 100f64.to_radians().sin()),
        "{before:?} {after:?}"
    );
    assert!(close(after.origin[1], before.origin[1]), "{after:?}");
    // The sketch keeps its outline: only its plane moved.
    let (_, max) = doc.sketch_output(sketch).unwrap().regions[0].bounds();
    assert!(close(max[1], 15.0), "{max:?}");

    // Run again where it is already: nothing moves, no undo step.
    let label = doc.undo_label().map(str::to_owned);
    let depth = query(&doc, json!({"query":"document"}))["undo_depth"].clone();
    let again = command(
        &mut doc,
        json!({"cmd":"sheet_flange_profile","def":profiled}),
    );
    assert_eq!(again["added"], 0);
    assert_eq!(doc.undo_label().map(str::to_owned), label);
    assert_eq!(
        query(&doc, json!({"query":"document"}))["undo_depth"],
        depth
    );

    // A profile sketch that is not on a fixed plane of its own cannot be
    // moved: refused in words.
    let loose = uid(&command(&mut doc, json!({"cmd":"sketch.create"}))["uid"]);
    let mut on_xy = plain.clone();
    on_xy["profile"] = json!(loose);
    let error = doc
        .command(&json!({"cmd":"sheet_flange_profile","def":on_xy}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("fixed plane of its own"), "{error}");
}

// The profile sketch's plane follows the wall (mitcad#1776, GitHub
// mitcad/Mitcad#310): a `flange_wall` plane, worked out again at every
// recompute from the sheet and the flange that names the sketch.

/// The fixture with the Face's width along x the parameter `w` (40 mm), so
/// that narrowing it moves the flange's edge x = 40.
fn fixture_with_width() -> (Document<MockKernel>, FeatureUid, BodyUid) {
    let mut doc = Document::new(MockKernel::default());
    doc.add_parameter("w", 40.0, "").unwrap();
    let rules = json!({"type":"sheet_rules","thickness":1,"bend_radius":2,
        "allowance":{"type":"k_factor","k":0.4},"reference_side":"front",
        "bend_relief":{"type":"round","width":1,"depth":3},
        "corner_relief":{"type":"rectangular","width":1,"depth":3}});
    let rules = doc.add_feature(&def(rules), None).unwrap().uid;
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(
            sketch,
            [0.0, 0.0],
            &ValueInput::Name("w".to_owned()),
            &num(20.0),
        )
        .unwrap()
        .region;
    let face =
        json!({"type":"sheet_face","rules":rules,"profiles":[{"sketch":sketch,"region":region}]});
    let uid = doc.add_feature(&def(face), None).unwrap().uid;
    (doc, sketch, BodyUid::new(uid, 0))
}

fn frame_of(doc: &Document<MockKernel>, sketch: FeatureUid) -> SketchFrame {
    match doc.sketch_output(sketch) {
        Some(output) => output.frame,
        None => panic!(
            "{sketch} has no output: {:?}",
            doc.features()
                .map(|f| (f.uid, doc.status(f.uid)))
                .collect::<Vec<_>>()
        ),
    }
}

/// Whether two frames are the same within 1e-6 mm (the axes within 1e-6).
fn same_frame(a: &SketchFrame, b: &SketchFrame) -> bool {
    let near = |p: [f64; 3], q: [f64; 3]| p.iter().zip(q).all(|(u, v)| close(*u, v));
    near(a.origin, b.origin) && near(a.x_axis, b.x_axis) && near(a.y_axis, b.y_axis)
}

/// The wall's frame where `plain` (the flange's definition without a
/// profile) puts it at the flange `at`, worked out independently of the
/// profile's plane: a new profile made with the marker just before that
/// flange, as the Flange panel makes one, read, and undone with the
/// marker's move.
fn wall_frame(doc: &mut Document<MockKernel>, at: FeatureUid, plain: &Value) -> SketchFrame {
    let end = doc.features().count();
    doc.set_marker(position(doc, at)).unwrap();
    let made = command(doc, json!({"cmd":"sheet_flange_profile","def":plain}));
    assert_eq!(made["added"], 2, "{made}");
    let frame = frame_of(doc, uid(&made["sketch"]));
    doc.undo();
    doc.undo();
    assert_eq!(doc.features().count(), end);
    frame
}

/// The flange gives no warning that its profile left the wall. (The mock
/// kernel trims no sheet domains, so a flange with a profile fails after
/// it read the profile; the warning's condition, the frames within 1e-6
/// mm, is checked by the callers, and the CLI test sees the warning go.)
fn no_moved_warning(doc: &Document<MockKernel>, flange: FeatureUid) {
    let warnings = doc.warnings(flange);
    assert!(
        !warnings.iter().any(|w| w.contains("no longer lies")),
        "{warnings:?}"
    );
    if let Some(FeatureStatus::Failed(message)) = doc.status(flange) {
        let message = message.to_string();
        assert!(!message.contains("no longer lies"), "{message}");
    }
}

#[test]
fn flange_profile_plane_follows_the_wall_when_the_edge_or_the_angle_changes() {
    let (mut doc, base, body) = fixture_with_width();
    let plain = flange(body, base);
    let made = command(&mut doc, json!({"cmd":"sheet_flange_profile","def":plain}));
    let (sketch, plane) = (uid(&made["sketch"]), uid(&made["plane"]));
    let feature = query(&doc, json!({"query":"feature","uid":plane}));
    assert_eq!(
        feature["def"]["definition"]["type"], "flange_wall",
        "{feature}"
    );
    let mut profiled = plain.clone();
    profiled["profile"] = json!(sketch);
    let flange_uid = doc.add_feature(&def(profiled.clone()), None).unwrap().uid;
    let before = frame_of(&doc, sketch);
    let wall = wall_frame(&mut doc, flange_uid, &plain);
    assert!(same_frame(&before, &wall), "{before:?} {wall:?}");
    let outline = doc.sketch_output(sketch).unwrap().regions[0].bounds();

    // The Face narrowed by 10 mm: its edge moves to x = 30, and the
    // sketch's plane with it, its axes kept.
    doc.set_parameter("w", 30.0).unwrap();
    let narrowed = frame_of(&doc, sketch);
    let mut moved = before;
    moved.origin[0] -= 10.0;
    assert!(same_frame(&narrowed, &moved), "{before:?} {narrowed:?}");
    let wall = wall_frame(&mut doc, flange_uid, &plain);
    assert!(same_frame(&narrowed, &wall), "{narrowed:?} {wall:?}");
    // The curves stay where they were in the wall's frame.
    assert_eq!(
        doc.sketch_output(sketch).unwrap().regions[0].bounds(),
        outline
    );
    no_moved_warning(&doc, flange_uid);

    // The flange at 100 degrees: the plane turns with the wall.
    let mut steeper = profiled.clone();
    steeper["angle"] = json!(100f64.to_radians());
    doc.edit_feature(flange_uid, &def(steeper)).unwrap();
    let turned = frame_of(&doc, sketch);
    assert!(
        close(turned.y_axis[2].abs(), 100f64.to_radians().sin()),
        "{turned:?}"
    );
    let mut plain_steeper = plain.clone();
    plain_steeper["angle"] = json!(100f64.to_radians());
    let wall = wall_frame(&mut doc, flange_uid, &plain_steeper);
    assert!(same_frame(&turned, &wall), "{turned:?} {wall:?}");
    no_moved_warning(&doc, flange_uid);

    // Undone, the plane is back on the 90-degree wall.
    doc.undo();
    assert!(same_frame(&frame_of(&doc, sketch), &narrowed));

    // Saved and opened again: still on the wall.
    let mut opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    opened.recompute();
    let feature = query(&opened, json!({"query":"feature","uid":plane}));
    assert_eq!(
        feature["def"]["definition"]["type"], "flange_wall",
        "{feature}"
    );
    assert!(same_frame(&frame_of(&opened, sketch), &narrowed));
}

#[test]
fn flange_profile_on_an_earlier_fixed_plane_follows_the_wall_once_opened() {
    let (mut doc, base, body) = fixture_with_width();
    let plain = flange(body, base);
    // An earlier version's profile: a fixed plane where the wall was, the
    // sketch on it and the flange naming the sketch.
    let made = command(&mut doc, json!({"cmd":"sheet_flange_profile","def":plain}));
    let (sketch, plane) = (uid(&made["sketch"]), uid(&made["plane"]));
    let wall = frame_of(&doc, sketch);
    let fixed = json!({"type":"construction_plane","definition":{"type":"fixed",
        "origin":wall.origin,"x_axis":wall.x_axis,"y_axis":wall.y_axis}});
    doc.edit_feature(plane, &def(fixed)).unwrap();
    let mut profiled = plain.clone();
    profiled["profile"] = json!(sketch);
    let flange_uid = doc.add_feature(&def(profiled.clone()), None).unwrap().uid;
    let outline = doc.sketch_output(sketch).unwrap().regions[0].bounds();
    // Narrowed: the fixed plane stays where the wall was.
    doc.set_parameter("w", 30.0).unwrap();
    assert!(same_frame(&frame_of(&doc, sketch), &wall));
    let earlier = doc.to_json();

    // Opened: the plane is a flange-wall plane, the sketch on the wall
    // where the narrowed edge puts it, its curves where they were in the
    // wall's frame.
    let mut opened = Document::from_json(&earlier, MockKernel::default()).unwrap();
    opened.recompute();
    let feature = query(&opened, json!({"query":"feature","uid":plane}));
    assert_eq!(
        feature["def"]["definition"]["type"], "flange_wall",
        "{feature}"
    );
    let mut moved = wall;
    moved.origin[0] -= 10.0;
    let now = frame_of(&opened, sketch);
    assert!(same_frame(&now, &moved), "{wall:?} {now:?}");
    let expected = wall_frame(&mut opened, flange_uid, &plain);
    assert!(same_frame(&now, &expected), "{now:?} {expected:?}");
    assert_eq!(
        opened.sketch_output(sketch).unwrap().regions[0].bounds(),
        outline
    );
    no_moved_warning(&opened, flange_uid);

    // A fixed plane another sketch also lies on is the user's: left fixed
    // when the design is opened, and the profile stays where it is.
    let shared = uid(&command(&mut doc, json!({"cmd":"sketch.create","plane":plane}))["uid"]);
    assert!(position(&doc, shared) > position(&doc, flange_uid));
    let mut opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    opened.recompute();
    let feature = query(&opened, json!({"query":"feature","uid":plane}));
    assert_eq!(feature["def"]["definition"]["type"], "fixed", "{feature}");
    assert!(same_frame(&frame_of(&opened, sketch), &wall));

    // Edit Profile (the command with the marker before the flange) makes it
    // a flange-wall plane, in one undo step, and the sketch is on the wall.
    let end = opened.features().count();
    opened.set_marker(position(&opened, flange_uid)).unwrap();
    let again = command(
        &mut opened,
        json!({"cmd":"sheet_flange_profile","def":profiled}),
    );
    assert_eq!(again["added"], 0, "{again}");
    assert_eq!(again["plane"], json!(plane), "{again}");
    opened.set_marker(end).unwrap();
    let feature = query(&opened, json!({"query":"feature","uid":plane}));
    assert_eq!(
        feature["def"]["definition"]["type"], "flange_wall",
        "{feature}"
    );
    assert!(same_frame(&frame_of(&opened, sketch), &moved));
}
