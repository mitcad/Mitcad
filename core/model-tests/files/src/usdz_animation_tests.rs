// SPDX-License-Identifier: MIT
//! Animated USDZ export (mitcad#1597) with the mock kernel: a two-component
//! slider mechanism over three poses, a component placed twice with a
//! nested occurrence (each placement moving on its own), the design's
//! coordinates, a root body, a motion study's frames, the refusals, the
//! document's pose and undo steps unchanged and a still export unchanged.
//! The written files are read back with `mitcad-usdz`'s reader: the
//! layer's time and the samples at the first, middle and last frames.
//!
//! The mock's blocks are 10 x 10 x 5 at their component's origin. In the
//! mechanism the plate (Plate:1) is grounded at the origin and the pin
//! (Pin:1) slides along x on the plate's top (Joint1, `tx`).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::assembly::matrix_rows;
use mitcad_model::exchange::{
    AnimationPoses, AnimationSpace, AnimationTarget, AnimationTrack, ExportFormat, ExportOptions,
    ExportPlacement, UsdzAnimation,
};
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::joints::Motion;
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;
use mitcad_usdz::sdf::Value as Usd;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string()).unwrap_err().to_string()
}

/// A 10 x 10 x 5 block in the active component.
fn block_in(doc: &mut Document<MockKernel>) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

fn face(doc: &Document<MockKernel>, body: BodyUid, role: &str) -> String {
    doc.body_shape(body)
        .unwrap()
        .faces
        .iter()
        .find(|f| f.role == role)
        .unwrap()
        .to_string()
}

/// A component with a block, placed in the active component; the active
/// component stays.
fn part(
    doc: &mut Document<MockKernel>,
    name: &str,
    at: Transform,
) -> (OccurrenceUid, BodyUid, ComponentUid) {
    let active = doc.assembly().active;
    let (component, occurrence) = doc.create_component(Some(name), at, true).unwrap();
    let body = block_in(doc);
    doc.activate_component(active).unwrap();
    (occurrence, body, component)
}

struct Mechanism {
    doc: Document<MockKernel>,
    pin: OccurrenceUid,
    slider: FeatureUid,
}

/// The plate grounded at the origin, the pin sliding along x on it.
fn mechanism() -> Mechanism {
    let mut doc = Document::new(MockKernel::default());
    let (plate, plate_body, _) = part(&mut doc, "Plate", Transform::IDENTITY);
    let (pin, pin_body, _) = part(&mut doc, "Pin", Transform::translation([100.0, 0.0, 0.0]));
    doc.set_occurrence_grounded(plate, true).unwrap();
    let top = json!({"occurrence": "Plate:1",
                     "geometry": {"body": plate_body, "face": face(&doc, plate_body, "end")}});
    let bottom = json!({"occurrence": "Pin:1",
                        "geometry": {"body": pin_body, "face": face(&doc, pin_body, "start")}});
    let slider = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "slider", "slide_axis": "x", "flip": true,
               "a": bottom, "b": top, "position": {"tx": 0}}),
    );
    let slider = slider["uid"].as_str().unwrap().parse().unwrap();
    Mechanism { doc, pin, slider }
}

/// A pose as `export`'s rows.
fn rows(t: &Transform) -> Value {
    json!(matrix_rows(t))
}

fn moved(t: &Transform, by: [f64; 3]) -> Transform {
    let mut out = *t;
    for (v, d) in out.translation.iter_mut().zip(by) {
        *v += d;
    }
    out
}

/// A turn about Z through a point.
fn turn_z(degrees: f64, at: [f64; 3]) -> Transform {
    Transform::rotation(at, [0.0, 0.0, 1.0], degrees.to_radians()).unwrap()
}

/// The transform of USD's row-vector matrix.
fn from_usd(m: &[[f64; 4]; 4]) -> Transform {
    Transform {
        linear: std::array::from_fn(|r| std::array::from_fn(|c| m[c][r])),
        translation: [m[3][0], m[3][1], m[3][2]],
    }
}

fn close(a: &Transform, b: &Transform) -> bool {
    (0..3).all(|r| (0..3).all(|c| (a.linear[r][c] - b.linear[r][c]).abs() < 1e-9))
        && (0..3).all(|k| (a.translation[k] - b.translation[k]).abs() < 1e-9)
}

fn layer(path: &str) -> mitcad_usdz::Layer {
    mitcad_usdz::read_package(&std::fs::read(path).unwrap())
        .unwrap()
        .layer
}

/// A prim's `xformOp:transform` samples (time, transform); None when it
/// has none.
fn samples(layer: &mitcad_usdz::Layer, prim: &str) -> Option<Vec<(f64, Transform)>> {
    let path = format!("{prim}.xformOp:transform");
    match layer.field(&path, "timeSamples") {
        None => None,
        Some(Usd::TimeSamples(samples)) => Some(
            samples
                .iter()
                .map(|(t, value)| match value {
                    Usd::Matrix4d(m) => (*t, from_usd(m)),
                    other => panic!("{path}: {other:?}"),
                })
                .collect(),
        ),
        Some(other) => panic!("{path}: {other:?}"),
    }
}

fn default_transform(layer: &mitcad_usdz::Layer, prim: &str) -> Transform {
    match layer.field(&format!("{prim}.xformOp:transform"), "default") {
        Some(Usd::Matrix4d(m)) => from_usd(m),
        other => panic!("{prim}: {other:?}"),
    }
}

fn temp_path(name: &str) -> String {
    mitcad_model::testing::unique_temp_dir("usdz-animation")
        .join(name)
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_slider_mechanism_moves_through_three_poses() {
    let mut m = mechanism();
    let at = m.doc.placement(m.pin).unwrap();
    let depth = m.doc.undo_depth();
    let label = m.doc.undo_label().map(str::to_owned);
    let path = temp_path("design.usdz");
    let poses: Vec<Transform> = [0.0, 5.0, 10.0]
        .iter()
        .map(|x| moved(&at, [*x, 0.0, 0.0]))
        .collect();
    let pose_rows: Vec<Value> = poses.iter().map(rows).collect();
    let result = command(
        &mut m.doc,
        json!({"cmd": "export", "path": path, "animation": {"fps": 24, "tracks": [
            {"occurrence": "Pin:1", "transforms": pose_rows}]}}),
    );
    assert_eq!(result["format"], "usdz");
    assert_eq!(
        result["animation"],
        json!({"fps": 24.0, "start": 0.0, "end": 2.0, "frames": 3, "moved": ["Pin:1"]})
    );
    let layer = layer(&path);
    // The declared rate and range.
    let root = layer.spec("/").unwrap();
    assert_eq!(root.field("startTimeCode"), Some(&Usd::Double(0.0)));
    assert_eq!(root.field("endTimeCode"), Some(&Usd::Double(2.0)));
    assert_eq!(root.field("timeCodesPerSecond"), Some(&Usd::Double(24.0)));
    assert_eq!(root.field("framesPerSecond"), Some(&Usd::Double(24.0)));
    // Three distinct poses of the pin, first, middle and last; the plate
    // does not move; the pin's default stays where the design has it.
    let pin = samples(&layer, "/design/Pin_1").unwrap();
    assert_eq!(
        pin.iter().map(|(t, _)| *t).collect::<Vec<_>>(),
        [0.0, 1.0, 2.0]
    );
    for ((_, got), want) in pin.iter().zip(&poses) {
        assert!(close(got, want), "{got:?} {want:?}");
    }
    assert!(!close(&pin[0].1, &pin[1].1) && !close(&pin[1].1, &pin[2].1));
    assert!(close(&default_transform(&layer, "/design/Pin_1"), &at));
    assert!(samples(&layer, "/design/Plate_1").is_none());
    // The meshes are written once each.
    let meshes = layer
        .specs
        .iter()
        .filter(|s| s.field("typeName") == Some(&Usd::Token("Mesh".to_owned())))
        .count();
    assert_eq!(meshes, 2);
    // The document did not move and has no new undo step.
    assert!(close(&m.doc.placement(m.pin).unwrap(), &at));
    assert_eq!(m.doc.joint_value(m.slider, Motion::Tx), 0.0);
    assert_eq!(m.doc.undo_depth(), depth);
    assert_eq!(m.doc.undo_label().map(str::to_owned), label);
}

#[test]
fn repeated_and_nested_occurrences_move_independently() {
    let mut doc = Document::new(MockKernel::default());
    // Arm:1 at the origin with a block and a nested Pin:1 5 mm up with a
    // block; Arm:2 a copy 100 mm along x; a block of the root.
    let (arm, arm_body, _) = {
        let (component, occurrence) = doc
            .create_component(Some("Arm"), Transform::IDENTITY, true)
            .unwrap();
        let body = block_in(&mut doc);
        part(&mut doc, "Pin", Transform::translation([0.0, 0.0, 5.0]));
        doc.activate_component(ComponentUid::ROOT).unwrap();
        (occurrence, body, component)
    };
    let root_body = block_in(&mut doc);
    let copy = doc
        .copy_occurrence(arm, Some(Transform::translation([100.0, 0.0, 0.0])))
        .unwrap();
    assert_eq!(doc.assembly().occurrence_name(copy), "Arm:2");
    let path = temp_path("design.usdz");

    // In the parents' coordinates: Arm:1's pin slides, Arm:2 turns, the
    // root's block rises; Arm:1 and Arm:2's pin stay in their parents.
    let slide: Vec<Value> = [0.0, 3.0, 6.0]
        .iter()
        .map(|x| rows(&Transform::translation([*x, 0.0, 5.0])))
        .collect();
    // Arm:2 turns about Z at its place.
    let turns: Vec<Transform> = [0.0, 45.0, 90.0]
        .iter()
        .map(|d| Transform::translation([100.0, 0.0, 0.0]).after(&turn_z(*d, [0.0; 3])))
        .collect();
    let turn_rows: Vec<Value> = turns.iter().map(rows).collect();
    let rise: Vec<Value> = [0.0, 1.0, 2.0]
        .iter()
        .map(|z| rows(&Transform::translation([0.0, 0.0, *z])))
        .collect();
    let root_name = mitcad_usdz::identifier(&doc.body_name(root_body));
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path, "animation": {"fps": 30, "start": 10, "tracks": [
            {"occurrence": "Arm:1/Pin:1", "transforms": slide},
            {"occurrence": "Arm:2", "transforms": turn_rows},
            {"body": root_body.to_string(), "transforms": rise}]}}),
    );
    assert_eq!(
        result["animation"]["moved"],
        json!(["Arm:1/Pin:1", "Arm:2", doc.body_name(root_body)])
    );
    assert_eq!(result["animation"]["start"], 10.0);
    assert_eq!(result["animation"]["end"], 12.0);
    let layer = layer(&path);
    let pin = samples(&layer, "/design/Arm_1/Pin_1").unwrap();
    assert_eq!(
        pin.iter().map(|(t, _)| *t).collect::<Vec<_>>(),
        [10.0, 11.0, 12.0]
    );
    let xs: Vec<f64> = pin.iter().map(|(_, t)| t.translation[0]).collect();
    assert_eq!(xs, [0.0, 3.0, 6.0]);
    // The other placement of the same nested occurrence, and Arm:1, do not
    // move in their parents.
    assert!(samples(&layer, "/design/Arm_2/Pin_1").is_none());
    assert!(samples(&layer, "/design/Arm_1").is_none());
    let arm2 = samples(&layer, "/design/Arm_2").unwrap();
    for (k, (_, got)) in arm2.iter().enumerate() {
        assert!(close(got, &turns[k]), "frame {k}: {got:?}");
    }
    // The root's block: its mesh moves in the root.
    let block = samples(&layer, &format!("/design/{root_name}")).unwrap();
    assert_eq!(block[2].1.translation, [0.0, 0.0, 2.0]);
    assert!(close(
        &default_transform(&layer, &format!("/design/{root_name}")),
        &Transform::IDENTITY
    ));

    // In the design's coordinates: Arm:2 turns, its pin stays where it is
    // in the design (100, 0, 5), so it turns back in Arm:2.
    let still_pin = rows(&Transform::translation([100.0, 0.0, 5.0]));
    command(
        &mut doc,
        json!({"cmd": "export", "path": path, "animation": {"fps": 30, "space": "design", "tracks": [
            {"occurrence": "Arm:2", "transforms": turn_rows},
            {"occurrence": "Arm:2/Pin:1", "transforms": [still_pin, still_pin, still_pin]}]}}),
    );
    let layer = self::layer(&path);
    let arm2 = samples(&layer, "/design/Arm_2").unwrap();
    let pin2 = samples(&layer, "/design/Arm_2/Pin_1").unwrap();
    for k in 0..3 {
        let world = arm2[k].1.after(&pin2[k].1);
        assert!(
            close(&world, &Transform::translation([100.0, 0.0, 5.0])),
            "frame {k}: {world:?}"
        );
    }
    assert!(close(&pin2[0].1, &Transform::translation([0.0, 0.0, 5.0])));
    assert!(
        !close(&pin2[2].1, &pin2[0].1),
        "the pin turns back in Arm:2"
    );

    // A body of the component placed twice is ambiguous; by its occurrence
    // it is not.
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": temp_path("ambiguous.usdz"), "animation": {"fps": 24,
               "tracks": [{"body": arm_body.to_string(), "transforms": [rows(&Transform::IDENTITY)]}]}}),
    );
    assert!(
        error.contains(&format!(
            "{} is written by 2 placements (Arm:1, Arm:2): animate one of them by its occurrence instead",
            doc.body_name(arm_body)
        )),
        "{error}"
    );
    // With only Arm:1's placements written, the body is one mesh.
    let path = temp_path("one.usdz");
    command(
        &mut doc,
        json!({"cmd": "export", "path": path, "occurrence": "Arm:1", "animation": {"fps": 24,
               "tracks": [{"body": arm_body.to_string(), "transforms": rise}]}}),
    );
    let arm_mesh = format!(
        "/one/Arm_1/{}",
        mitcad_usdz::identifier(&doc.body_name(arm_body))
    );
    let one = self::layer(&path);
    assert_eq!(
        samples(&one, &arm_mesh).unwrap()[1].1.translation,
        [0.0, 0.0, 1.0]
    );
}

#[test]
fn a_motion_study_gives_the_frames() {
    let mut m = mechanism();
    let at = m.doc.placement(m.pin).unwrap();
    command(
        &mut m.doc,
        json!({"cmd": "add_motion_study", "steps": 20, "keys": [
            {"joint": m.slider.to_string(), "motion": "tx", "points": [[0, 0], [20, 8]]}]}),
    );
    let depth = m.doc.undo_depth();
    let path = temp_path("design.usdz");
    let result = command(
        &mut m.doc,
        json!({"cmd": "export", "path": path,
               "animation": {"fps": 10, "study": "Study1", "from": 0, "to": 20, "frames": 5}}),
    );
    assert_eq!(
        result["animation"],
        json!({"fps": 10.0, "start": 0.0, "end": 4.0, "frames": 5, "moved": ["Pin:1"]})
    );
    let layer = layer(&path);
    let pin = samples(&layer, "/design/Pin_1").unwrap();
    assert_eq!(pin.len(), 5);
    // Steps 0, 5, 10, 15 and 20: tx 0, 2, 4, 6 and 8.
    for (k, (_, got)) in pin.iter().enumerate() {
        let want = moved(&at, [2.0 * k as f64, 0.0, 0.0]);
        assert!(close(got, &want), "frame {k}: {got:?} {want:?}");
    }
    assert!(samples(&layer, "/design/Plate_1").is_none());
    // The study's default frames: one per step, both ends.
    let result = command(
        &mut m.doc,
        json!({"cmd": "export", "path": path, "animation": {"fps": 24, "study": "Study1"}}),
    );
    assert_eq!(result["animation"]["frames"], 21);
    // The document did not move.
    assert_eq!(m.doc.joint_value(m.slider, Motion::Tx), 0.0);
    assert!(close(&m.doc.placement(m.pin).unwrap(), &at));
    assert_eq!(m.doc.undo_depth(), depth);
    assert_eq!(m.doc.undo_label(), Some("Add Study1"));

    // A study that moves nothing written is refused.
    command(
        &mut m.doc,
        json!({"cmd": "add_motion_study", "steps": 10, "keys": [
            {"joint": m.slider.to_string(), "motion": "tx", "points": [[0, 0], [10, 0]]}]}),
    );
    let error = refused(
        &mut m.doc,
        json!({"cmd": "export", "path": temp_path("none.usdz"),
               "animation": {"fps": 24, "study": "Study2"}}),
    );
    assert!(
        error.contains("Study2 moves none of the occurrences written"),
        "{error}"
    );
}

#[test]
fn a_still_export_is_unchanged() {
    let mut m = mechanism();
    let still = temp_path("still.usdz");
    let result = command(&mut m.doc, json!({"cmd": "export", "path": still}));
    assert!(result.get("animation").is_none());
    let before = std::fs::read(&still).unwrap();
    let layer = layer(&still);
    assert!(layer.spec("/").unwrap().field("startTimeCode").is_none());
    assert!(layer.specs.iter().all(|s| s.field("timeSamples").is_none()));
    // An animated export between two still ones changes nothing of them.
    let at = m.doc.placement(m.pin).unwrap();
    command(
        &mut m.doc,
        json!({"cmd": "export", "path": temp_path("animated.usdz"), "animation": {"fps": 24,
               "tracks": [{"occurrence": "Pin:1", "transforms": [rows(&at), rows(&moved(&at, [1.0, 0.0, 0.0]))]}]}}),
    );
    command(&mut m.doc, json!({"cmd": "export", "path": still}));
    assert_eq!(std::fs::read(&still).unwrap(), before);
}

#[test]
fn bad_animations_are_refused_and_write_nothing() {
    let mut m = mechanism();
    let at = m.doc.placement(m.pin).unwrap();
    let path = temp_path("refused.usdz");
    let pose = rows(&at);
    let track = |transforms: Value| json!([{"occurrence": "Pin:1", "transforms": transforms}]);
    let cases: Vec<(Value, &str)> = vec![
        (
            json!({"fps": 0, "tracks": track(json!([pose]))}),
            "fps must be a positive number of frames per second, not 0",
        ),
        (
            json!({"fps": -24, "tracks": track(json!([pose]))}),
            "fps must be a positive number",
        ),
        (
            json!({"fps": 24, "times": [0, 2, 1], "tracks": track(json!([pose, pose, pose]))}),
            "the frames' time codes must increase: 1 follows 2",
        ),
        (
            json!({"fps": 24, "times": [0, 1], "tracks": track(json!([pose, pose, pose]))}),
            "2 times for 3 frames",
        ),
        (
            json!({"fps": 24, "start": 1, "times": [0], "tracks": track(json!([pose]))}),
            "give start or times, not both",
        ),
        (
            json!({"fps": 24, "tracks": [{"occurrence": "Nope:1", "transforms": [pose]}]}),
            "there is no occurrence Nope:1",
        ),
        (
            json!({"fps": 24, "tracks": [{"body": "No body", "transforms": [pose]}]}),
            "no body at the timeline marker is named No body",
        ),
        (
            json!({"fps": 24, "tracks": [{"occurrence": "", "transforms": [pose]}]}),
            "the design itself does not move",
        ),
        (
            json!({"fps": 24, "tracks": [{"occurrence": "Pin:1", "body": "x", "transforms": [pose]}]}),
            "track 0 needs an occurrence",
        ),
        (
            json!({"fps": 24, "tracks": [
                {"occurrence": "Pin:1", "transforms": [pose, pose]},
                {"occurrence": "Plate:1", "transforms": [pose]}]}),
            "every track has one pose per frame",
        ),
        (
            json!({"fps": 24, "tracks": [
                {"occurrence": "Pin:1", "transforms": [pose]},
                {"occurrence": "Pin:1", "transforms": [pose]}]}),
            "Pin:1 is animated by more than one track",
        ),
        (
            json!({"fps": 24, "tracks": track(json!([[[2, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0]]]))}),
            "Pin:1, frame 0: a pose must be a rotation and a translation",
        ),
        (
            json!({"fps": 24, "tracks": track(json!([[[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [5, 0, 0, 1]]]))}),
            "is transposed",
        ),
        (
            json!({"fps": 24, "tracks": track(json!([[1, 0, 0]]))}),
            "a pose is three or four rows of four numbers, or 12 or 16 numbers, not 3",
        ),
        (
            json!({"fps": 24, "tracks": track(json!([pose])), "study": "Study1"}),
            "give tracks (poses per occurrence or body) or study, not both",
        ),
        (
            json!({"fps": 24, "tracks": track(json!([pose])), "frames": 3}),
            "frames is for a study",
        ),
        (json!({"fps": 24}), "give tracks"),
        (json!({"fps": 24, "study": "Nothing"}), "Nothing"),
    ];
    for (animation, expected) in cases {
        let error = refused(
            &mut m.doc,
            json!({"cmd": "export", "path": path, "animation": animation}),
        );
        assert!(error.contains(expected), "{animation}: {error}");
        assert!(!std::path::Path::new(&path).exists(), "{animation}");
    }
    // Another format, the components' coordinates.
    let animation = json!({"fps": 24, "tracks": track(json!([pose]))});
    let error = refused(
        &mut m.doc,
        json!({"cmd": "export", "path": temp_path("a.stl"), "animation": animation}),
    );
    assert!(error.contains("animation is for USDZ export"), "{error}");
    let error = refused(
        &mut m.doc,
        json!({"cmd": "export", "path": path, "coordinates": "component", "animation": animation}),
    );
    assert!(error.contains("leave out coordinates component"), "{error}");
    // A hidden occurrence is not written: it cannot move.
    m.doc.set_occurrence_visible(m.pin, false).unwrap();
    let error = refused(
        &mut m.doc,
        json!({"cmd": "export", "path": path, "animation": animation}),
    );
    assert!(error.contains("Pin:1 is not written"), "{error}");
    m.doc.set_occurrence_visible(m.pin, true).unwrap();

    // Through the model: a pose that is not finite, a bad rate.
    let options = ExportOptions::new(ExportFormat::Usdz);
    let export = |doc: &Document<MockKernel>, animation: UsdzAnimation| {
        let placement = ExportPlacement {
            animation: Some(animation),
            ..ExportPlacement::default()
        };
        doc.export_bodies(&path, &[], &options, &Default::default(), &placement)
            .unwrap_err()
            .to_string()
    };
    let mut bad = moved(&at, [0.0; 3]);
    bad.translation[1] = f64::NAN;
    let tracks = |transforms: Vec<Transform>| AnimationPoses::Tracks {
        space: AnimationSpace::Parent,
        tracks: vec![AnimationTrack {
            target: AnimationTarget::Occurrence(vec![m.pin]),
            transforms,
        }],
    };
    let error = export(
        &m.doc,
        UsdzAnimation {
            fps: 24.0,
            times: vec![0.0, 1.0],
            poses: tracks(vec![at, bad]),
        },
    );
    assert!(
        error.contains("Pin:1, frame 1: the pose is not finite"),
        "{error}"
    );
    let error = export(
        &m.doc,
        UsdzAnimation {
            fps: f64::INFINITY,
            times: vec![0.0],
            poses: tracks(vec![at]),
        },
    );
    assert!(
        error.contains("positive number of frames per second"),
        "{error}"
    );
    let error = export(
        &m.doc,
        UsdzAnimation {
            fps: 24.0,
            times: vec![0.0, f64::NAN],
            poses: tracks(vec![at, at]),
        },
    );
    assert!(error.contains("finite"), "{error}");
    assert!(!std::path::Path::new(&path).exists());
}
