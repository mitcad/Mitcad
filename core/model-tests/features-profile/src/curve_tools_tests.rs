// SPDX-License-Identifier: MIT
//! The curve tools (mitcad#1777) with the mock kernel: a Helix Curve on a
//! cylindrical face of 20 mm diameter, pitch 5 and 4 turns, lies on the
//! face within 1e-6 mm from its low end up, and drives a Pipe; on a conical
//! face it follows the cone (a growing helix from the kernel); its
//! refusals; a Helix of a sketch line is a new surface body; an
//! Intersection Curve of that surface and a body is one curve, a face of a
//! body taken as it is, and its refusals. The geometry is checked with OCCT
//! by `geometry/tests/test_curve_tools.cpp` (`geometry.curve_tools`) and
//! `tools/cli/tests/curve_tools.json` (`cli.curve_tools`).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::{MockKernel, RoundFace};
use mitcad_model::topo::TopoName;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    let result = doc
        .command(&value.to_string())
        .unwrap_or_else(|e| panic!("{value}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn uid(name: &str) -> FeatureUid {
    name.parse().unwrap()
}

fn status(doc: &Document<MockKernel>, feature: &str) -> FeatureStatus {
    doc.status(uid(feature)).cloned().expect("recomputed")
}

fn error(doc: &Document<MockKernel>, feature: &str) -> String {
    match status(doc, feature) {
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn add(doc: &mut Document<MockKernel>, def: Value) -> String {
    command(doc, json!({"cmd": "add_feature", "def": def}))["uid"]
        .as_str()
        .expect("a uid")
        .to_owned()
}

fn specs(doc: &Document<MockKernel>, prefix: &str) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|s| s.starts_with(prefix))
        .cloned()
        .collect()
}

/// The block's side face over c1, which the tests make round.
fn side() -> String {
    "F2:side(c1[c4,c2])"
        .parse::<TopoName>()
        .unwrap()
        .to_string()
}

/// The block's top face, a plane.
fn end() -> String {
    "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})"
        .parse::<TopoName>()
        .unwrap()
        .to_string()
}

/// Sketch1 (F1) and a 60 x 40 block 20 high (Extrude1, F2, body F2.b0)
/// whose side face over c1 is the round face `round` with the surface
/// `surface`.
fn block_with(round: RoundFace, surface: SurfaceGeometry) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60,
               "height": 40}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    doc.kernel().round_faces.borrow_mut().insert(side(), round);
    doc.kernel().surfaces.borrow_mut().insert(side(), surface);
    doc
}

/// A cylinder of 20 mm diameter, 20 high, about the Z axis from z = 0.
fn cylinder_block() -> Document<MockKernel> {
    block_with(
        RoundFace::cylinder([0.0, 0.0, 0.0], 10.0, 20.0, [1.0, 0.0, 0.0]),
        SurfaceGeometry::Cylinder {
            origin: [0.0, 0.0, 10.0],
            axis: [0.0, 0.0, 1.0],
            radius: 10.0,
        },
    )
}

/// The point of a helix at its parameter `t`.
fn helix_at(curve: &Curve3, t: f64) -> [f64; 3] {
    let Curve3::Helix {
        center,
        axis,
        x_axis,
        radius,
        pitch,
        ..
    } = curve
    else {
        panic!("not a helix: {curve:?}");
    };
    let y = [
        axis[1] * x_axis[2] - axis[2] * x_axis[1],
        axis[2] * x_axis[0] - axis[0] * x_axis[2],
        axis[0] * x_axis[1] - axis[1] * x_axis[0],
    ];
    std::array::from_fn(|k| {
        center[k]
            + radius * (t.cos() * x_axis[k] + t.sin() * y[k])
            + pitch * t / std::f64::consts::TAU * axis[k]
    })
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= 1e-6)
}

#[test]
fn a_helix_curve_on_a_cylinder_lies_on_it_and_drives_a_pipe() {
    let mut doc = cylinder_block();
    let helix = add(
        &mut doc,
        json!({"type": "helix_curve", "axis": {"body": "F2.b0", "face": side()},
               "pitch": 5, "revolutions": 4}),
    );
    assert_eq!(helix, "F3");
    assert_eq!(status(&doc, "F3"), FeatureStatus::Ok);
    let curve = doc.feature_curve(uid("F3")).expect("a curve");
    assert_eq!(curve.len(), 1);
    assert_eq!(curve[0].name, "F3:curve1");
    let Curve3::Helix { start, end, .. } = curve[0].curve else {
        panic!("an exact helix: {:?}", curve[0].curve);
    };
    // From the face's low end at the X axis, right-handed, up to its top.
    assert!(near(helix_at(&curve[0].curve, start), [10.0, 0.0, 0.0]));
    assert!(near(helix_at(&curve[0].curve, end), [10.0, 0.0, 20.0]));
    assert!(near(
        helix_at(&curve[0].curve, start + std::f64::consts::FRAC_PI_2),
        [0.0, 10.0, 1.25]
    ));
    for i in 0..=1000 {
        let p = helix_at(
            &curve[0].curve,
            start + (end - start) * f64::from(i) / 1000.0,
        );
        assert!(
            (p[0].hypot(p[1]) - 10.0).abs() <= 1e-6,
            "{p:?} is off the cylinder"
        );
        assert!(
            p[2] >= -1e-6 && p[2] <= 20.0 + 1e-6,
            "{p:?} is off the face"
        );
    }
    // It is construction geometry: the feature_curves query lists it.
    let curves: Value = serde_json::from_str(
        &doc.query(&json!({"query": "feature_curves"}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(curves[0]["name"], "HelixCurve1", "{curves}");

    // A Pipe along it.
    let pipe = add(
        &mut doc,
        json!({"type": "pipe", "path": {"feature": "F3"}, "size": 2, "operation": "new_body"}),
    );
    assert_eq!(status(&doc, &pipe), FeatureStatus::Ok);
    let piped = specs(&doc, &format!("pipe {pipe} "));
    assert!(
        piped.last().is_some_and(|s| s.contains("path F3:curve1 ")),
        "{piped:?}"
    );

    // Left-handed: a quarter turn in it is at -Y.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F3", "def": {"type": "helix_curve",
               "axis": {"body": "F2.b0", "face": side()}, "pitch": 5, "revolutions": 4,
               "left_handed": true}}),
    );
    assert_eq!(status(&doc, "F3"), FeatureStatus::Ok);
    let curve = doc.feature_curve(uid("F3")).expect("a curve");
    let Curve3::Helix { start, .. } = curve[0].curve else {
        panic!("an exact helix");
    };
    assert!(near(
        helix_at(&curve[0].curve, start + std::f64::consts::FRAC_PI_2),
        [0.0, -10.0, 1.25]
    ));
    assert_eq!(status(&doc, &pipe), FeatureStatus::Ok);
}

#[test]
fn a_helix_curve_about_an_axis_takes_its_radius_and_start_angle() {
    let mut doc = Document::new(MockKernel::default());
    let helix = add(
        &mut doc,
        json!({"type": "helix_curve", "axis": "z", "radius": 30, "pitch": 10,
               "revolutions": 2.5, "start_angle": "90 deg", "flip": true}),
    );
    assert_eq!(status(&doc, &helix), FeatureStatus::Ok);
    let curve = doc.feature_curve(uid(&helix)).expect("a curve");
    let Curve3::Helix { start, end, .. } = curve[0].curve else {
        panic!("an exact helix");
    };
    // Down the Z axis from the origin, starting a quarter turn round from
    // +X about -Z (at -Y), 25 down after two and a half turns (at +Y).
    assert!(near(helix_at(&curve[0].curve, start), [0.0, -30.0, 0.0]));
    assert!(near(helix_at(&curve[0].curve, end), [0.0, 30.0, -25.0]));
}

#[test]
fn a_helix_curve_on_a_cone_follows_it() {
    // A cone from radius 10 at z = 0 to 14 at z = 20: 0.2 out per mm up.
    let mut doc = block_with(
        RoundFace {
            base: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            seam: [1.0, 0.0, 0.0],
            radius: 10.0,
            top_radius: 14.0,
            height: 20.0,
        },
        SurfaceGeometry::Cone {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 10.0,
            half_angle: 0.2_f64.atan(),
        },
    );
    let helix = add(
        &mut doc,
        json!({"type": "helix_curve", "axis": {"body": "F2.b0", "face": side()},
               "pitch": 5, "revolutions": 4}),
    );
    assert_eq!(status(&doc, &helix), FeatureStatus::Ok);
    let asked = specs(&doc, "helix_curve ");
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(
        asked[0].contains("radius 10 pitch 5 revolutions 4 left false growth 1"),
        "{asked:?}"
    );
    let curve = doc.feature_curve(uid(&helix)).expect("a curve");
    let ends = |piece: &Curve3| match piece {
        Curve3::Line { start, end } => (*start, *end),
        other => panic!("the mock makes lines, not {other:?}"),
    };
    assert!(near(ends(&curve[0].curve).0, [10.0, 0.0, 0.0]));
    assert!(near(
        ends(&curve[curve.len() - 1].curve).1,
        [14.0, 0.0, 20.0]
    ));
    // With a radius of its own it keeps it: an exact helix.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": helix, "def": {"type": "helix_curve",
               "axis": {"body": "F2.b0", "face": side()}, "radius": 12, "pitch": 5,
               "revolutions": 4}}),
    );
    let curve = doc.feature_curve(uid(&helix)).expect("a curve");
    assert!(matches!(curve[0].curve, Curve3::Helix { radius, .. } if radius == 12.0));
}

#[test]
fn a_helix_curve_refuses_what_gives_no_helix() {
    // A planar face without a radius.
    let mut doc = cylinder_block();
    let helix = add(
        &mut doc,
        json!({"type": "helix_curve", "axis": {"body": "F2.b0", "face": end()},
               "pitch": 5, "revolutions": 4}),
    );
    let message = error(&doc, &helix);
    assert!(message.contains("not a cylinder or a cone"), "{message}");
    // An axis without a radius.
    let helix = add(
        &mut doc,
        json!({"type": "helix_curve", "axis": "z", "pitch": 5, "revolutions": 4}),
    );
    let message = error(&doc, &helix);
    assert!(message.contains("needs a radius"), "{message}");
    // Narrowing to the axis before the last turn.
    let helix = add(
        &mut doc,
        json!({"type": "helix_curve", "axis": "z", "radius": 5, "pitch": 5, "revolutions": 4,
               "growth": -2}),
    );
    let message = error(&doc, &helix);
    assert!(message.contains("narrows to its axis"), "{message}");
    // No pitch.
    let refused = doc.command(
        &json!({"cmd": "add_feature", "def": {"type": "helix_curve", "axis": "z", "radius": 5,
                "pitch": 0, "revolutions": 4}})
        .to_string(),
    );
    assert!(refused.is_err(), "{refused:?}");
}

/// Sketch2 (F3) on XZ with a line from the Z axis out to 15 along X, and a
/// surface Helix of it about Z (Helix1, F4): 4 turns of pitch 5. The
/// helix's uid and the line's.
fn helicoid(doc: &mut Document<MockKernel>) -> (String, String) {
    command(doc, json!({"cmd": "sketch.create", "plane": "xz"}));
    let line = command(
        doc,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": [0, 0], "end": [15, 0]}),
    );
    let line = line["made"][0].as_str().unwrap().to_owned();
    let helix = add(
        doc,
        json!({"type": "helix", "curves": {"sketch": "F3", "curves": [line]}, "surface": true,
               "axis": "z", "pitch": 5, "revolutions": 4, "operation": "new_body"}),
    );
    (helix, line)
}

#[test]
fn a_helix_of_a_line_is_a_surface_body() {
    let mut doc = cylinder_block();
    let bodies = doc.bodies().len();
    let (helix, line) = helicoid(&mut doc);
    assert_eq!(helix, "F4");
    assert_eq!(status(&doc, "F4"), FeatureStatus::Ok);
    assert_eq!(doc.bodies().len(), bodies + 1);
    let sheet = specs(&doc, "helix_sheet(F4: ");
    assert_eq!(sheet.len(), 1, "{sheet:?}");
    assert!(
        sheet[0].contains("pitch 5 revolutions 4 left false"),
        "{sheet:?}"
    );
    let faces: Vec<String> = doc
        .body_shape("F4.b0".parse().unwrap())
        .expect("the surface body")
        .faces
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        !faces.is_empty() && faces.iter().all(|f| f.starts_with("F4:side(")),
        "{faces:?}"
    );
    // A solid helix takes no curves; a surface one no growth.
    let refused = doc.command(
        &json!({"cmd": "add_feature", "def": {"type": "helix",
                "curves": {"sketch": "F3", "curves": [line]}, "axis": "z", "pitch": 5,
                "revolutions": 4, "operation": "new_body"}})
        .to_string(),
    );
    assert!(refused.is_err(), "{refused:?}");
    let refused = doc.command(
        &json!({"cmd": "add_feature", "def": {"type": "helix",
                "curves": {"sketch": "F3", "curves": [line]}, "surface": true, "axis": "z",
                "pitch": 5, "revolutions": 4, "growth": 1, "operation": "new_body"}})
        .to_string(),
    );
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| e.to_string().contains("no growth")),
        "{refused:?}"
    );
}

#[test]
fn an_intersection_curve_of_a_helical_surface_and_a_body_is_one_helix() {
    let mut doc = cylinder_block();
    helicoid(&mut doc);
    // Where the helicoid meets the cylinder: the helix of radius 10, as the
    // kernel would give it.
    let helix = Curve3::Helix {
        center: [0.0, 0.0, 0.0],
        axis: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        radius: 10.0,
        pitch: 5.0,
        start: 0.0,
        end: 4.0 * std::f64::consts::TAU,
    };
    *doc.kernel().crossing.borrow_mut() = Some(vec![helix.clone()]);
    let curve = add(
        &mut doc,
        json!({"type": "intersection_curve", "first": {"geometry": [{"body": "F4.b0"}]},
               "second": {"geometry": [{"body": "F2.b0"}]}}),
    );
    assert_eq!(curve, "F5");
    assert_eq!(status(&doc, "F5"), FeatureStatus::Ok);
    let made = doc.feature_curve(uid("F5")).expect("a curve");
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].name, "F5:curve1");
    assert_eq!(made[0].curve, helix);
    let asked = specs(&doc, "shape_section ");
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(
        asked[0].starts_with("shape_section ") && asked[0].contains("helix_sheet(F4,sheet)"),
        "{asked:?}"
    );
    // A Pipe along it.
    let pipe = add(
        &mut doc,
        json!({"type": "pipe", "path": {"feature": "F5"}, "size": 2, "operation": "new_body"}),
    );
    assert_eq!(status(&doc, &pipe), FeatureStatus::Ok);

    // A face of the body as it is: the kernel gets its name.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F5", "def": {"type": "intersection_curve",
               "first": {"geometry": [{"body": "F4.b0"}]},
               "second": {"geometry": [{"body": "F2.b0", "face": side()}]}}}),
    );
    assert_eq!(status(&doc, "F5"), FeatureStatus::Ok);
    let asked = specs(&doc, "shape_section ");
    assert!(
        asked
            .last()
            .is_some_and(|s| s.ends_with(&format!("[{}]", side()))),
        "{asked:?}"
    );
    // The design file keeps the geometry and reads it back.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert!(
        file["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["type"] == "intersection_curve"
                && f["second"]["geometry"][0]["face"] == json!(side())),
        "{saved}"
    );

    // Where they do not meet: said in words.
    // The mock's crossing is no input of the feature: drop the result
    // cached for this definition when it was added, so that it is
    // evaluated again.
    doc.clear_memory_cache();
    *doc.kernel().crossing.borrow_mut() = Some(Vec::new());
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F5", "def": {"type": "intersection_curve",
               "first": {"geometry": [{"body": "F4.b0"}]},
               "second": {"geometry": [{"body": "F2.b0"}]}}}),
    );
    let message = error(&doc, "F5");
    assert!(message.contains("do not meet"), "{message}");
}

#[test]
fn an_intersection_curve_takes_both_inputs_of_one_kind() {
    let mut doc = cylinder_block();
    let (_, line) = helicoid(&mut doc);
    for (first, second) in [
        (
            json!({"sketch": "F3", "curves": [line]}),
            json!({"geometry": [{"body": "F2.b0"}]}),
        ),
        (
            json!({"geometry": [{"body": "F2.b0"}]}),
            json!({"sketch": "F3", "curves": [line]}),
        ),
    ] {
        let refused = doc.command(
            &json!({"cmd": "add_feature", "def": {"type": "intersection_curve",
                    "first": first, "second": second}})
            .to_string(),
        );
        assert!(
            refused
                .as_ref()
                .is_err_and(|e| e.to_string().contains("one input is sketch curves")),
            "{refused:?}"
        );
    }
    // A sketch is neither a face nor a body.
    let refused = doc.command(
        &json!({"cmd": "add_feature", "def": {"type": "intersection_curve",
                "first": {"geometry": ["F1"]}, "second": {"geometry": [{"body": "F2.b0"}]}}})
        .to_string(),
    );
    assert!(refused.is_err(), "{refused:?}");
    // Nothing picked.
    let refused = doc.command(
        &json!({"cmd": "add_feature", "def": {"type": "intersection_curve",
                "first": {"geometry": []}, "second": {"geometry": [{"body": "F2.b0"}]}}})
        .to_string(),
    );
    assert!(refused.is_err(), "{refused:?}");
}
