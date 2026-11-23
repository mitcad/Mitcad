// SPDX-License-Identifier: MIT
//! A part placed on a plane or a face follows it (mitcad#2286, GitHub
//! mitcad/Mitcad#585): the joint keeps the picked point, the direction and
//! the edge alignment in the plane's own frame (`frame_override.relative`),
//! so that a profile on an offset construction plane moves when the
//! plane's distance is edited, one on a body's top face when the body's
//! height changes, and one on a plane that turns turns with it; a face that
//! widens in its own plane leaves the profile where it is (its point is
//! measured from the point of the face's plane nearest to the origin, not
//! from the face's middle). A design saved with the absolute override
//! (`frame_override.origin`) opens with every part in place, and its next
//! Placement edit writes the relative form.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ValueInput;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::features::{FeatureDef, JointDef};
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::parameters::ParamId;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_tests_support::test_library::*;

fn axis(t: &Transform) -> [f64; 3] {
    t.apply_vector([0.0, 0.0, 1.0])
}

fn x_of(t: &Transform) -> [f64; 3] {
    t.apply_vector([1.0, 0.0, 0.0])
}

fn plus(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Whether two placements are the same (origin and axes).
fn same(a: &Transform, b: &Transform) -> bool {
    close(a.translation, b.translation) && close(axis(a), axis(b)) && close(x_of(a), x_of(b))
}

fn uid_of(answer: &Value) -> String {
    answer["uid"]
        .as_str()
        .or_else(|| answer["feature"].as_str())
        .unwrap_or_else(|| panic!("no feature in {answer}"))
        .to_owned()
}

/// The joint that places a part.
fn joint_of(doc: &Document<MockKernel>, answer: &Value) -> FeatureUid {
    doc.part_info(component(answer))
        .and_then(|i| i.record.joint)
        .unwrap_or_else(|| panic!("no joint for {answer}"))
}

/// The joint's `b.frame_override` as the design keeps it.
fn kept_override(doc: &Document<MockKernel>, answer: &Value) -> Value {
    let joint = query(
        doc,
        json!({"query": "feature", "uid": joint_of(doc, answer).to_string()}),
    );
    joint["def"]["b"]["frame_override"].clone()
}

/// A construction plane added; its uid.
fn add_plane(doc: &mut Document<MockKernel>, definition: Value) -> String {
    let added = command(
        doc,
        json!({"cmd": "add_feature",
               "def": {"type": "construction_plane", "definition": definition}}),
    )
    .unwrap_or_else(|e| panic!("construction plane: {e}"));
    uid_of(&added)
}

fn edit_plane(doc: &mut Document<MockKernel>, plane: &str, definition: Value) {
    command(
        doc,
        json!({"cmd": "edit_feature", "uid": plane,
               "def": {"type": "construction_plane", "definition": definition}}),
    )
    .unwrap_or_else(|e| panic!("editing the plane: {e}"));
    all_ok(doc);
}

/// A box from (100, 200) on the XY plane, `length` along X, 60 along Y,
/// `height` high, with its faces on its bounds; its top face's plane
/// (normal +Z) as a kernel describes it. Returns the box feature and the
/// top face as `at.geometry` takes it.
fn block(doc: &mut Document<MockKernel>) -> (String, Value) {
    doc.kernel().box_faces.set(true);
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": block_def(40.0, 500.0)}),
    )
    .unwrap_or_else(|e| panic!("box: {e}"));
    let feature = uid_of(&added);
    let top = format!("{feature}:top");
    doc.kernel().surfaces.borrow_mut().insert(
        top.clone(),
        SurfaceGeometry::Plane {
            origin: [120.0, 230.0, 500.0],
            normal: [0.0, 0.0, 1.0],
        },
    );
    all_ok(doc);
    let face = json!({"body": format!("{feature}.b0"), "face": top});
    (feature, face)
}

fn block_def(length: f64, height: f64) -> Value {
    json!({"type": "box", "plane": {"type": "origin", "plane": "xy"}, "corner": [100, 200],
           "length": length, "width": 60, "height": height, "operation": "new_body"})
}

fn edit_block(doc: &mut Document<MockKernel>, feature: &str, length: f64, height: f64) {
    command(
        doc,
        json!({"cmd": "edit_feature", "uid": feature, "def": block_def(length, height)}),
    )
    .unwrap_or_else(|e| panic!("editing the box: {e}"));
    all_ok(doc);
}

#[test]
fn a_profile_on_an_offset_plane_follows_the_plane_s_distance() {
    let mut doc = design();
    let plane = add_plane(
        &mut doc,
        json!({"type": "offset", "plane": "xy", "distance": 200}),
    );
    // Standing at a click far from the plane's origin, and lying along X.
    let standing = place(
        &mut doc,
        json!({"part": part("p30"),
               "at": {"geometry": plane.as_str(), "point": [-900.0, 1200.0, 200.0]},
               "length": 300}),
    );
    let lying = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"geometry": plane.as_str(), "point": [400.0, -300.0, 200.0],
                      "direction": [1.0, 0.0, 0.0]},
               "length": 300}),
    );
    all_ok(&doc);
    let stood = placed(&doc, occurrence(&standing));
    let lay = placed(&doc, occurrence(&lying));
    assert!(
        close(stood.translation, [-900.0, 1200.0, 200.0]),
        "{stood:?}"
    );
    assert!(close(axis(&lay), [1.0, 0.0, 0.0]), "{lay:?}");
    // Kept in the plane's frame: no absolute origin.
    let kept = kept_override(&doc, &standing);
    assert!(kept.get("origin").is_none(), "{kept}");
    assert!(
        close(numbers(&kept["relative"]["origin"]), [-900.0, 1200.0, 0.0]),
        "{kept}"
    );
    assert!(kept["relative"].get("nearest").is_none(), "{kept}");

    // The plane 350 above XY: both parts 150 higher, turned as they were.
    edit_plane(
        &mut doc,
        &plane,
        json!({"type": "offset", "plane": "xy", "distance": 350}),
    );
    let up = [0.0, 0.0, 150.0];
    let now = placed(&doc, occurrence(&standing));
    assert!(
        close(now.translation, plus(stood.translation, up)),
        "{now:?}"
    );
    assert!(close(axis(&now), axis(&stood)), "{now:?}");
    let now = placed(&doc, occurrence(&lying));
    assert!(close(now.translation, plus(lay.translation, up)), "{now:?}");
    assert!(close(axis(&now), axis(&lay)), "{now:?}");
    assert!(close(x_of(&now), x_of(&lay)), "{now:?}");
}

#[test]
fn a_profile_on_a_body_s_top_face_follows_its_height_and_stays_when_it_widens() {
    let mut doc = design();
    let (block_feature, top) = block(&mut doc);
    // At a click off the face's middle (120, 230).
    let point = [110.0, 215.0, 500.0];
    let t = place(
        &mut doc,
        json!({"part": part("p30"), "at": {"geometry": top, "point": point}, "length": 300}),
    );
    all_ok(&doc);
    let before = placed(&doc, occurrence(&t));
    assert!(close(before.translation, point), "{before:?}");
    assert!(close(axis(&before), [0.0, 0.0, 1.0]), "{before:?}");
    let kept = kept_override(&doc, &t);
    assert!(kept.get("origin").is_none(), "{kept}");
    assert_eq!(kept["relative"]["nearest"], true, "{kept}");

    // 700 high: the top face, and the profile, 200 higher.
    edit_block(&mut doc, &block_feature, 40.0, 700.0);
    let higher = placed(&doc, occurrence(&t));
    assert!(
        close(higher.translation, [110.0, 215.0, 700.0]),
        "{higher:?}"
    );
    assert!(close(axis(&higher), axis(&before)), "{higher:?}");
    assert!(close(x_of(&higher), x_of(&before)), "{higher:?}");

    // 100 along X instead of 40: the face's middle moves 30 along X, the
    // profile stays at its click.
    edit_block(&mut doc, &block_feature, 100.0, 700.0);
    let wider = placed(&doc, occurrence(&t));
    assert!(same(&wider, &higher), "{wider:?} {higher:?}");
}

#[test]
fn a_profile_on_a_plane_that_turns_turns_with_it() {
    let mut doc = design();
    let plane = add_plane(
        &mut doc,
        json!({"type": "fixed", "origin": [0, 0, 100], "x_axis": [1, 0, 0], "y_axis": [0, 1, 0]}),
    );
    let standing = place(
        &mut doc,
        json!({"part": part("p30"),
               "at": {"geometry": plane.as_str(), "point": [50.0, 20.0, 100.0]},
               "length": 300}),
    );
    // Lying along the plane's y.
    let lying = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"geometry": plane.as_str(), "point": [-50.0, 40.0, 100.0],
                      "direction": [0.0, 1.0, 0.0]},
               "length": 300}),
    );
    all_ok(&doc);
    let stood = placed(&doc, occurrence(&standing));
    let lay = placed(&doc, occurrence(&lying));
    assert!(close(stood.translation, [50.0, 20.0, 100.0]), "{stood:?}");
    assert!(close(axis(&stood), [0.0, 0.0, 1.0]), "{stood:?}");
    assert!(close(axis(&lay), [0.0, 1.0, 0.0]), "{lay:?}");

    // Turned a quarter about X through its origin: its y up, its normal
    // -Y. Every point and direction turns with it.
    edit_plane(
        &mut doc,
        &plane,
        json!({"type": "fixed", "origin": [0, 0, 100], "x_axis": [1, 0, 0], "y_axis": [0, 0, 1]}),
    );
    let point = |p: [f64; 3]| [p[0], -(p[2] - 100.0), 100.0 + p[1]];
    let vector = |v: [f64; 3]| [v[0], -v[2], v[1]];
    for (answer, before) in [(&standing, &stood), (&lying, &lay)] {
        let now = placed(&doc, occurrence(answer));
        assert!(
            close(now.translation, point(before.translation)),
            "{now:?} {before:?}"
        );
        assert!(
            close(axis(&now), vector(axis(before))),
            "{now:?} {before:?}"
        );
        assert!(
            close(x_of(&now), vector(x_of(before))),
            "{now:?} {before:?}"
        );
    }
    let now = placed(&doc, occurrence(&standing));
    assert!(close(now.translation, [50.0, 0.0, 120.0]), "{now:?}");
    assert!(close(axis(&now), [0.0, -1.0, 0.0]), "{now:?}");
    assert!(close(
        axis(&placed(&doc, occurrence(&lying))),
        [0.0, 0.0, 1.0]
    ));
}

/// A joint's definition in command form.
fn joint_def(doc: &Document<MockKernel>, joint: FeatureUid) -> JointDef<ValueInput> {
    let params = doc.parameters();
    let def = doc
        .feature(joint)
        .unwrap()
        .def
        .map_params(&mut |_, id: &ParamId| Ok::<_, ()>(ValueInput::Name(params.name(*id))))
        .unwrap();
    match def {
        FeatureDef::Joint(j) => j,
        _ => panic!("{joint} is no joint"),
    }
}

/// The joint's override written as designs before mitcad#2286 kept it:
/// origin and axes in the component's coordinates.
fn make_absolute(doc: &mut Document<MockKernel>, answer: &Value) {
    let joint = joint_of(doc, answer);
    let mut def = joint_def(doc, joint);
    let o = def.b.frame_override.expect("an override");
    let local = mitcad_model::internal::joints::origin_frame(&mut doc.resolver(), &def.b.geometry)
        .expect("the plane's frame");
    let absolute = mitcad_model::internal::joints::absolute_override(&local, &o);
    assert!(absolute.relative.is_none() && absolute.origin.is_some());
    def.b.frame_override = Some(absolute);
    doc.apply(|state| {
        state.edit_feature(joint, &FeatureDef::Joint(def))?;
        Ok(("Old override".to_owned(), ()))
    })
    .unwrap();
    all_ok(doc);
}

#[test]
fn a_design_with_the_absolute_override_opens_in_place_and_an_edit_makes_it_follow() {
    let mut doc = design();
    let plane = add_plane(
        &mut doc,
        json!({"type": "offset", "plane": "xy", "distance": 200}),
    );
    let (_, top) = block(&mut doc);
    let on_plane = place(
        &mut doc,
        json!({"part": part("p30"),
               "at": {"geometry": plane.as_str(), "point": [-900.0, 1200.0, 200.0],
                      "direction": [0.0, 1.0, 0.0]},
               "length": 300, "rotate": "30 deg"}),
    );
    let on_face = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": {"geometry": top, "point": [110.0, 215.0, 500.0]},
               "length": 300}),
    );
    all_ok(&doc);
    let placed_now =
        |doc: &Document<MockKernel>| [&on_plane, &on_face].map(|a| placed(doc, occurrence(a)));
    let before = placed_now(&doc);
    for answer in [&on_plane, &on_face] {
        make_absolute(&mut doc, answer);
    }
    let absolute = placed_now(&doc);
    for (a, b) in absolute.iter().zip(&before) {
        assert!(same(a, b), "{a:?} {b:?}");
    }

    // Saved and opened: every part where it was, the override as it was
    // written (no conversion while the file is read).
    let text = doc.to_json();
    let kernel = doc.kernel().fork().expect("the mock kernel forks");
    let mut opened = Document::from_json(&text, kernel).unwrap();
    opened.recompute();
    all_ok(&opened);
    for (a, b) in placed_now(&opened).iter().zip(&before) {
        assert!(same(a, b), "{a:?} {b:?}");
    }
    let kept = kept_override(&opened, &on_plane);
    assert!(
        kept.get("relative").is_none() && kept.get("origin").is_some(),
        "{kept}"
    );
    let shown = query(&opened, json!({"query": "build_parts"}));
    let entry = shown["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["component"] == on_plane["component"])
        .cloned()
        .unwrap();
    assert!((entry["offset"].as_f64().unwrap()).abs() < 1e-9, "{entry}");
    assert!(
        (entry["rotate"].as_f64().unwrap() - 30f64.to_radians()).abs() < 1e-9,
        "{entry}"
    );

    // A Placement edit (Offset 0 again) writes the relative form, the part
    // staying where it is; from then on it follows the plane.
    command(
        &mut opened,
        json!({"cmd": "build_edit", "component": on_plane["component"], "at": {"offset": 0}}),
    )
    .unwrap();
    all_ok(&opened);
    let kept = kept_override(&opened, &on_plane);
    assert!(
        kept.get("origin").is_none() && kept.get("relative").is_some(),
        "{kept}"
    );
    let edited = placed(&opened, occurrence(&on_plane));
    assert!(same(&edited, &before[0]), "{edited:?} {:?}", before[0]);
    edit_plane(
        &mut opened,
        &plane,
        json!({"type": "offset", "plane": "xy", "distance": 260}),
    );
    let moved = placed(&opened, occurrence(&on_plane));
    assert!(
        close(
            moved.translation,
            plus(before[0].translation, [0.0, 0.0, 60.0])
        ),
        "{moved:?}"
    );
    assert!(close(axis(&moved), axis(&before[0])), "{moved:?}");
    assert!(close(x_of(&moved), x_of(&before[0])), "{moved:?}");

    // Rotate on the face's part writes it too, at the same place.
    command(
        &mut opened,
        json!({"cmd": "build_edit", "component": on_face["component"], "rotate": "90 deg"}),
    )
    .unwrap();
    all_ok(&opened);
    let kept = kept_override(&opened, &on_face);
    assert!(kept.get("origin").is_none(), "{kept}");
    assert_eq!(kept["relative"]["nearest"], true, "{kept}");
    let turned = placed(&opened, occurrence(&on_face));
    assert!(
        close(turned.translation, before[1].translation),
        "{turned:?}"
    );
    assert!(close(axis(&turned), axis(&before[1])), "{turned:?}");
}

/// `build_preview` shows the frame the joint's relative override gives
/// (`joint_frame` reads it), and `ground_offset` reads it.
#[test]
fn the_preview_and_the_offset_read_the_relative_form() {
    let mut doc = design();
    let plane = add_plane(
        &mut doc,
        json!({"type": "offset", "plane": "xy", "distance": 200}),
    );
    let fields = json!({"part": part("p30"),
                        "at": {"geometry": plane.as_str(), "point": [10.0, 20.0, 200.0],
                               "direction": [1.0, 0.0, 0.0], "offset": 15},
                        "length": 300});
    let mut probe = fields.clone();
    probe["query"] = json!("build_preview");
    let shown = query(&doc, probe);
    assert_eq!(shown["problems"], json!([]), "{shown}");
    let t = place(&mut doc, fields);
    all_ok(&doc);
    let at = placed(&doc, occurrence(&t));
    assert!(close(axis(&at), [1.0, 0.0, 0.0]), "{at:?}");
    let kept = kept_override(&doc, &t);
    assert!(
        kept.get("shift").is_some() && kept.get("relative").is_some(),
        "{kept}"
    );
    assert!(kept.get("origin").is_none(), "{kept}");
    // The preview's frame is the one the kept override gives: lying along
    // X, 15 above the plane and as far up as it rests on it.
    let joint = query(
        &doc,
        json!({"query": "feature", "uid": joint_of(&doc, &t).to_string()}),
    );
    let frame = query(
        &doc,
        json!({"query": "joint_frame", "geometry": joint["def"]["b"]["geometry"],
               "frame_override": kept}),
    );
    for key in ["origin", "x_axis", "z_axis"] {
        assert!(
            close(numbers(&frame[key]), numbers(&shown["frame"][key])),
            "{key}: {frame} {shown}"
        );
    }
    assert!(close(numbers(&frame["z_axis"]), [1.0, 0.0, 0.0]), "{frame}");
    assert!(numbers(&frame["origin"])[2] > 215.0 - 1e-6, "{frame}");
    let offset = mitcad_model_build::ground_offset(&doc, component(&t)).unwrap();
    assert!((offset - 15.0).abs() < 1e-9, "{offset}");
}

fn numbers(v: &Value) -> [f64; 3] {
    [0, 1, 2].map(|i| v[i].as_f64().unwrap_or_else(|| panic!("no number in {v}")))
}
