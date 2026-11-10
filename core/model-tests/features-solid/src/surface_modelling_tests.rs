// SPDX-License-Identifier: MIT
//! Surface modelling on the mock kernel (mitcad#455): the surface bodies
//! the surface features make, the names of their faces, what the kernel is
//! asked for and what they refuse. The geometry is checked with OCCT by
//! `geometry/tests/test_surface_modelling.cpp`
//! (`geometry.surface_modelling`) and the `.solidcraft` test designs
//! `surface-*` (`cli.import_solidcraft`).

use serde_json::{Value, json};

use mitcad_model::exchange::BodyKind;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::test_support::{Block, block, corner, def, num, side, top_edge};
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::RegionKey;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

fn refused(doc: &mut Document<MockKernel>, value: Value) -> String {
    match doc.add_feature(&def(value.clone()), None) {
        Ok(added) => panic!("{value} was added as {}", added.uid),
        Err(e) => e.to_string(),
    }
}

fn failure(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match doc.status(uid).cloned().expect("recomputed") {
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("{uid} did not fail: {other:?}"),
    }
}

fn ok(doc: &Document<MockKernel>, uid: FeatureUid) {
    let status = doc.status(uid).cloned().expect("recomputed");
    assert!(status.is_ok(), "{uid}: {status:?}");
}

fn kind(doc: &Document<MockKernel>, body: BodyUid) -> BodyKind {
    let shape = doc.body_shape(body).expect("the body");
    doc.kernel().body_kind(shape).unwrap()
}

fn faces(doc: &Document<MockKernel>, body: BodyUid) -> Vec<String> {
    doc.body_shape(body)
        .expect("the body")
        .faces
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn body(text: &str) -> BodyUid {
    text.parse().unwrap()
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

fn line(doc: &mut Document<MockKernel>, sketch: FeatureUid, a: [f64; 2], b: [f64; 2]) -> String {
    let value = json!({"cmd": "sketch.add_line", "sketch": sketch, "start": a, "end": b});
    let result: Value = serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap();
    result["made"][0].as_str().unwrap().to_owned()
}

/// Sketch1 (F1) with a 20 x 10 rectangle at the origin and an open chain
/// of two lines beside it (from (40, 0) to (50, 0) to (50, 10)).
fn rectangle_and_chain() -> (Document<MockKernel>, FeatureUid, RegionKey, [String; 2]) {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(20.0), &num(10.0))
        .unwrap()
        .region;
    let a = line(&mut doc, sketch, [40.0, 0.0], [50.0, 0.0]);
    let b = line(&mut doc, sketch, [50.0, 0.0], [50.0, 10.0]);
    (doc, sketch, region, [a, b])
}

fn sides(feature: &str, region: &RegionKey) -> Vec<String> {
    region
        .segments()
        .map(|s| format!("{feature}:side({s})"))
        .collect()
}

#[test]
fn a_surface_extrusion_of_a_profile_is_its_walls_without_caps() {
    let (mut doc, sketch, region, _) = rectangle_and_chain();
    let uid = add(
        &mut doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 15}, "operation": "new_body",
               "surface": true}),
    );
    ok(&doc, uid);
    let sheet = body("F2.b0");
    assert_eq!(kind(&doc, sheet), BodyKind::Sheet);
    let mut found = faces(&doc, sheet);
    found.sort();
    let mut expected = sides("F2", &region);
    expected.sort();
    assert_eq!(found, expected);
    assert_eq!(doc.bodies().len(), 1);
    assert_eq!(
        specs(&doc, "open_faces"),
        ["open_faces(F2: start,end,mid of 6 faces)"]
    );
    assert!(matches!(
        &doc.feature(uid).unwrap().def,
        mitcad_model::features::FeatureDef::Extrude(e) if e.surface
    ));
}

#[test]
fn a_surface_extrusion_sweeps_open_curves_and_profiles_into_two_bodies() {
    let (mut doc, sketch, region, [a, b]) = rectangle_and_chain();
    let uid = add(
        &mut doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "curves": {"sketch": sketch, "curves": [a, b]},
               "extent": {"type": "two_sides",
                          "side1": {"type": "distance", "distance": 15},
                          "side2": {"type": "distance", "distance": 5}},
               "operation": "new_body", "surface": true}),
    );
    ok(&doc, uid);
    assert_eq!(doc.bodies().len(), 2);
    assert_eq!(
        faces(&doc, body("F2.b1")),
        [format!("F2:side({a})"), format!("F2:side({b})")]
    );
    assert_eq!(kind(&doc, body("F2.b1")), BodyKind::Sheet);
    let spec = specs(&doc, "extrude_sheet");
    assert_eq!(spec.len(), 1);
    assert!(spec[0].contains("[0, 0, 1] 15 back 5"), "{spec:?}");
    assert!(
        spec[0].contains(&format!("{a}>{b}")) || spec[0].contains(&format!("{b}>{a}")),
        "{spec:?}"
    );

    // Curves only.
    doc.undo();
    let uid = add(
        &mut doc,
        json!({"type": "extrude", "curves": {"sketch": sketch, "curves": [a]},
               "extent": {"type": "symmetric", "distance": 4}, "operation": "new_body",
               "surface": true}),
    );
    ok(&doc, uid);
    assert_eq!(faces(&doc, body("F2.b0")), [format!("F2:side({a})")]);
}

#[test]
fn surface_extrusions_refuse_what_does_not_make_a_surface() {
    let (mut doc, sketch, region, [a, _]) = rectangle_and_chain();
    let profile = json!([{"sketch": sketch, "region": region}]);
    let cases = [
        (
            json!({"type": "extrude", "profiles": profile, "operation": "join",
                   "extent": {"type": "distance", "distance": 5}, "surface": true}),
            "a surface extrusion is a new body or a new component",
        ),
        (
            json!({"type": "extrude", "curves": {"sketch": sketch, "curves": [a]},
                   "operation": "new_body", "extent": {"type": "distance", "distance": 5}}),
            "only a surface extrusion sweeps sketch curves",
        ),
        (
            json!({"type": "extrude", "operation": "new_body", "surface": true,
                   "extent": {"type": "distance", "distance": 5}}),
            "a surface extrusion needs profiles or sketch curves",
        ),
        (
            json!({"type": "extrude", "profiles": profile, "operation": "new_body",
                   "extent": {"type": "distance", "distance": 5}, "surface": true,
                   "thin": {"location": "center", "thickness": 1}}),
            "a surface extrusion has no thin walls",
        ),
    ];
    for (value, expected) in cases {
        let error = refused(&mut doc, value);
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn a_surface_revolution_turns_profiles_and_curves() {
    let (mut doc, sketch, region, [a, b]) = rectangle_and_chain();
    let uid = add(
        &mut doc,
        json!({"type": "revolve", "profiles": [{"sketch": sketch, "region": region}],
               "curves": {"sketch": sketch, "curves": [a, b]},
               "axis": "y", "extent": {"type": "angle", "angle": "90 deg"},
               "operation": "new_body", "surface": true}),
    );
    ok(&doc, uid);
    assert_eq!(doc.bodies().len(), 2);
    for k in 0..2 {
        assert_eq!(kind(&doc, body(&format!("F2.b{k}"))), BodyKind::Sheet);
    }
    let walls = faces(&doc, body("F2.b0"));
    assert!(walls.iter().all(|f| f.starts_with("F2:side(")), "{walls:?}");
    assert_eq!(walls.len(), 4);
    let spec = specs(&doc, "revolve_sheet");
    assert_eq!(spec.len(), 1);
    assert!(spec[0].contains("angles 1.5707963"), "{spec:?}");

    // Curves up to an object are not turned.
    doc.undo();
    let uid = add(
        &mut doc,
        json!({"type": "revolve", "curves": {"sketch": sketch, "curves": [a]},
               "axis": "y", "extent": {"type": "to_object",
                                       "object": {"type": "plane", "plane": "yz"}},
               "operation": "new_body", "surface": true}),
    );
    assert!(
        failure(&doc, uid).contains("turns by angles, not up to an object"),
        "{}",
        failure(&doc, uid)
    );
}

#[test]
fn a_surface_sweep_sweeps_curves_along_the_path() {
    let (mut doc, sketch, _, [a, b]) = rectangle_and_chain();
    let path = doc.add_sketch(SketchPlane::Xz).unwrap().uid;
    let rail = line(&mut doc, path, [40.0, 0.0], [40.0, 30.0]);
    let uid = add(
        &mut doc,
        json!({"type": "sweep", "curves": {"sketch": sketch, "curves": [a, b]},
               "path": {"sketch": path, "curves": [rail]},
               "end_scale": 2, "operation": "new_body", "surface": true}),
    );
    ok(&doc, uid);
    assert_eq!(kind(&doc, body("F3.b0")), BodyKind::Sheet);
    assert_eq!(faces(&doc, body("F3.b0")).len(), 2);
    let spec = specs(&doc, "sweep_sheet");
    assert_eq!(spec.len(), 1);
    assert!(spec[0].contains(&format!("path {rail}")), "{spec:?}");
    assert!(spec[0].ends_with("end scale 2)"), "{spec:?}");
    doc.undo();

    let error = refused(
        &mut doc,
        json!({"type": "sweep", "curves": {"sketch": sketch, "curves": [a]},
               "path": {"sketch": path, "curves": [rail]}, "twist_angle": "10 deg",
               "operation": "new_body", "surface": true}),
    );
    assert!(error.contains("takes no twist"), "{error}");
}

#[test]
fn a_surface_loft_lofts_sections_of_curves() {
    let mut doc = Document::new(MockKernel::default());
    let low = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let a = line(&mut doc, low, [0.0, 0.0], [10.0, 0.0]);
    let b = line(&mut doc, low, [10.0, 0.0], [10.0, 10.0]);
    let high = doc.add_sketch(SketchPlane::Xz).unwrap().uid;
    let c = line(&mut doc, high, [0.0, 20.0], [10.0, 20.0]);
    let section = |sketch: FeatureUid, curves: &[&String]| json!({"type": "curves", "sketch": sketch, "curves": curves});
    let uid = add(
        &mut doc,
        json!({"type": "loft", "sections": [section(low, &[&a, &b]), section(high, &[&c])],
               "ruled": true, "operation": "new_body", "surface": true}),
    );
    ok(&doc, uid);
    assert_eq!(kind(&doc, body("F3.b0")), BodyKind::Sheet);
    assert_eq!(
        specs(&doc, "loft_sheet"),
        [format!(
            "loft_sheet(F3: {a}>{b} | {c} ruled true closed false)"
        )]
    );
    doc.undo();

    let error = refused(
        &mut doc,
        json!({"type": "loft", "sections": [section(low, &[&a]), section(high, &[&c])],
               "operation": "new_body"}),
    );
    assert!(
        error.contains("only a surface loft has sections of curves"),
        "{error}"
    );
}

/// Block (F2.b0) and its walls as a surface (Extrude2, F3: the block's
/// profile extruded as a surface, F3.b0).
fn block_and_walls() -> (Block, BodyUid) {
    let mut b = block();
    let walls = add(
        &mut b.doc,
        json!({"type": "extrude", "profiles": [{"sketch": b.sketch, "region": b.region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body",
               "surface": true}),
    );
    ok(&b.doc, walls);
    (b, body("F3.b0"))
}

#[test]
fn a_trim_previews_its_extended_tool_past_the_body() {
    let (mut b, walls) = block_and_walls();
    let target = b.doc.body_shape(walls).unwrap().bounds.unwrap();
    let face = json!({"body": b.body, "face": format!("F2:end({})", b.region)});
    // Extended (the default): the face grown past the walls on every side.
    let trim = def(json!({"type": "trim_surface", "body": walls, "tool": face,
                          "keep": [[0, 30, 5]]}));
    b.doc.preview_add(&trim, None).unwrap();
    let tool = b.doc.preview_tool().expect("an extended tool");
    assert_eq!(
        tool.history,
        format!("extended_tool(face F2:end({}),true)", b.region)
    );
    let bounds = tool.bounds.unwrap();
    for i in 0..3 {
        assert!(bounds.min[i] < target.min[i] - 0.5, "{bounds:?} {target:?}");
        assert!(bounds.max[i] > target.max[i] + 0.5, "{bounds:?} {target:?}");
    }
    // Only the preview makes it: committing the trim does not.
    let made = b.doc.kernel().count("extended_tool");
    b.doc.add_feature(&trim, None).unwrap();
    assert_eq!(b.doc.kernel().count("extended_tool"), made);
    b.doc.undo();
    // Not extended, or by sketch curves: no extended tool.
    let line = line(&mut b.doc, b.sketch, [-10.0, 20.0], [70.0, 20.0]);
    for tool in [
        json!({"type": "trim_surface", "body": walls, "tool": face, "extend": false,
               "keep": [[0, 30, 5]]}),
        json!({"type": "trim_surface", "body": walls,
               "tool": {"sketch": b.sketch, "curves": [line]}, "keep": [[0, 30, 5]]}),
    ] {
        b.doc.preview_add(&def(tool), None).unwrap();
        assert!(b.doc.preview_tool().is_none());
    }
}

#[test]
fn an_offset_surface_copies_faces_out_as_a_new_surface_body() {
    let (mut b, walls) = block_and_walls();
    // Every face of the walls.
    let uid = add(
        &mut b.doc,
        json!({"type": "offset_surface", "body": walls, "distance": 2}),
    );
    ok(&b.doc, uid);
    let offset = body("F4.b0");
    assert_eq!(kind(&b.doc, offset), BodyKind::Sheet);
    let names = faces(&b.doc, offset);
    assert_eq!(names.len(), 4);
    assert!(
        names.iter().all(|f| f.starts_with("F4:offset(F3:side(")),
        "{names:?}"
    );
    // The walls stay.
    assert_eq!(faces(&b.doc, walls).len(), 4);
    b.doc.undo();
    // One face of the solid, chained, against its normal.
    let face = side(b.extrude, 1, 0).to_string();
    let uid = add(
        &mut b.doc,
        json!({"type": "offset_surface", "body": b.body, "faces": [face], "distance": -1,
               "chain": true}),
    );
    ok(&b.doc, uid);
    assert_eq!(faces(&b.doc, body("F4.b0")), [format!("F4:offset({face})")]);
    assert_eq!(
        specs(&b.doc, "offset_surface"),
        [
            "offset_surface(F4: 4 faces, 2, chain false)",
            "offset_surface(F4: 1 faces, -1, chain true)"
        ]
    );
}

#[test]
fn a_ruled_surface_hangs_off_a_chain_of_edges() {
    let Block {
        mut doc,
        extrude,
        region,
        body: solid,
        ..
    } = block();
    let edges: Vec<String> = (0..4)
        .map(|i| top_edge(extrude, 1, &region, i).to_string())
        .collect();
    let uid = add(
        &mut doc,
        json!({"type": "ruled_surface", "body": solid, "edges": edges,
               "face": format!("F2:end({region})"), "distance": 5, "mode": "tangent",
               "corners": "round"}),
    );
    ok(&doc, uid);
    let strip = body("F3.b0");
    assert_eq!(kind(&doc, strip), BodyKind::Sheet);
    assert_eq!(faces(&doc, strip).len(), 4);
    assert!(faces(&doc, strip)[0].starts_with("F3:ruled(E{"));
    let spec = specs(&doc, "ruled_surface");
    assert!(
        spec[0].contains("4 edges, 0 openings, 5, Tangent") && spec[0].ends_with("Round)"),
        "{spec:?}"
    );
    doc.undo();

    let cases = [
        (
            json!({"type": "ruled_surface", "body": solid, "edges": [edges[0]], "distance": 5,
                   "mode": "direction"}),
            "needs the direction",
        ),
        (
            json!({"type": "ruled_surface", "body": solid, "edges": [edges[0]], "distance": 0}),
            "distance must be greater than zero",
        ),
        (
            json!({"type": "ruled_surface", "body": solid, "edges": [edges[0]], "distance": 3,
                   "mode": "direction", "direction": [0, 0, 0]}),
            "must have some length",
        ),
    ];
    for (value, expected) in cases {
        let error = refused(&mut doc, value);
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn extend_continues_a_surface_body_past_its_edges() {
    let (mut b, walls) = block_and_walls();
    let edge = corner(FeatureUid(3), 1, 1).to_string();
    let uid = add(
        &mut b.doc,
        json!({"type": "extend_surface", "body": walls, "edges": [edge], "distance": 4,
               "mode": "tangent"}),
    );
    ok(&b.doc, uid);
    // The body keeps its id and its faces; no new body.
    assert!(
        b.doc
            .body_shape(walls)
            .unwrap()
            .history
            .starts_with("extend(")
    );
    assert_eq!(faces(&b.doc, walls).len(), 4);
    assert_eq!(b.doc.bodies().len(), 2);
    assert_eq!(
        specs(&b.doc, "extend_surface"),
        ["extend_surface(1 edges, 0 openings, 4, tangent)"]
    );
    b.doc.undo();
    // A solid is refused.
    let edge = top_edge(b.extrude, 1, &b.region, 0).to_string();
    let uid = add(
        &mut b.doc,
        json!({"type": "extend_surface", "body": b.body, "edges": [edge], "distance": 4}),
    );
    assert!(
        failure(&b.doc, uid).contains("is a solid"),
        "{}",
        failure(&b.doc, uid)
    );
}

/// Free edges have no names: Extend and Ruled Surface take them as an
/// opening of the body, found by the faces it runs along (the mock kernel
/// has an opening along each face of a surface body).
#[test]
fn free_edges_are_taken_by_their_opening() {
    let (mut b, walls) = block_and_walls();
    let face = faces(&b.doc, walls)[1].clone();
    let opening = json!({"opening": {"faces": [face]}, "faces": [face]});
    let uid = add(
        &mut b.doc,
        json!({"type": "extend_surface", "body": walls, "openings": [opening], "distance": 2}),
    );
    ok(&b.doc, uid);
    assert_eq!(
        specs(&b.doc, "extend_surface"),
        ["extend_surface(0 edges, 1 openings, 2, natural)"]
    );
    let strip = add(
        &mut b.doc,
        json!({"type": "ruled_surface", "body": walls, "openings": [opening], "distance": 3}),
    );
    ok(&b.doc, strip);
    assert_eq!(faces(&b.doc, body("F5.b0")), [format!("F5:ruled({face})")]);
    // An opening the body does not have.
    let uid = add(
        &mut b.doc,
        json!({"type": "ruled_surface", "body": walls, "distance": 3,
               "openings": [{"opening": {"faces": [face], "index": 3}}]}),
    );
    assert!(
        failure(&b.doc, uid).contains("openings along the faces"),
        "{}",
        failure(&b.doc, uid)
    );
    // Every opening's free edges of the face (the mock has one opening a
    // face: four openings, the face's edges in its own).
    let uid = add(
        &mut b.doc,
        json!({"type": "extend_surface", "body": walls, "distance": 1,
               "openings": [{"faces": [face]}]}),
    );
    ok(&b.doc, uid);
    assert_eq!(
        specs(&b.doc, "extend_surface").last().unwrap(),
        "extend_surface(0 edges, 4 openings, 1, natural)"
    );
    // Nothing to extend.
    let error = refused(
        &mut b.doc,
        json!({"type": "extend_surface", "body": walls, "distance": 2}),
    );
    assert!(error.contains("no edges selected"), "{error}");
}

#[test]
fn a_trim_keeps_the_pieces_the_points_pick() {
    let (mut b, walls) = block_and_walls();
    let line = line(&mut b.doc, b.sketch, [-10.0, 20.0], [70.0, 20.0]);
    // By a plane, by open sketch curves (as chains) and by a face.
    let cases = [
        (
            json!("xz"),
            "trim_surface(plane, 1 keep points, extend true)",
        ),
        (
            json!({"sketch": b.sketch, "curves": [line]}),
            &*format!("trim_surface(curves {line}, 1 keep points, extend true)"),
        ),
        (
            json!({"body": b.body, "face": format!("F2:end({})", b.region)}),
            &*format!(
                "trim_surface(face F2:end({}), 1 keep points, extend true)",
                b.region
            ),
        ),
    ];
    for (tool, spec) in cases {
        let uid = add(
            &mut b.doc,
            json!({"type": "trim_surface", "body": walls, "tool": tool, "keep": [[0, 30, 5]]}),
        );
        ok(&b.doc, uid);
        assert!(
            b.doc
                .body_shape(walls)
                .unwrap()
                .history
                .starts_with("trim(")
        );
        assert_eq!(specs(&b.doc, "trim_surface").last().unwrap(), spec);
        b.doc.undo();
    }
    let cases = [
        (
            json!({"type": "trim_surface", "body": walls, "tool": {"body": walls},
                   "keep": [[0, 0, 0]]}),
            "cannot trim itself",
        ),
        (
            json!({"type": "trim_surface", "body": walls, "tool": "xz", "keep": []}),
            "no point picks",
        ),
    ];
    for (value, expected) in cases {
        let error = refused(&mut b.doc, value);
        assert!(error.contains(expected), "{error}");
    }
    // A solid is Split Body's.
    let uid = add(
        &mut b.doc,
        json!({"type": "trim_surface", "body": b.body, "tool": "xz", "keep": [[0, 0, 0]]}),
    );
    assert!(
        failure(&b.doc, uid).contains("Split Body"),
        "{}",
        failure(&b.doc, uid)
    );
}

#[test]
fn untrim_restores_a_face_as_a_new_body_or_in_place() {
    let (mut b, walls) = block_and_walls();
    let face = faces(&b.doc, walls)[0].clone();
    let uid = add(
        &mut b.doc,
        json!({"type": "untrim_surface", "body": walls, "face": face, "outer": true}),
    );
    ok(&b.doc, uid);
    assert_eq!(faces(&b.doc, body("F4.b0")), [format!("F4:untrim({face})")]);
    assert_eq!(
        specs(&b.doc, "untrim_surface"),
        [format!(
            "untrim_surface({face}, holes true, outer true, in place false)"
        )]
    );
    b.doc.undo();
    let uid = add(
        &mut b.doc,
        json!({"type": "untrim_surface", "body": walls, "face": face, "in_place": true}),
    );
    ok(&b.doc, uid);
    assert_eq!(b.doc.bodies().len(), 2);
    assert!(
        b.doc
            .body_shape(walls)
            .unwrap()
            .history
            .starts_with("untrim(")
    );
    b.doc.undo();
    let error = refused(
        &mut b.doc,
        json!({"type": "untrim_surface", "body": walls, "face": face, "holes": false}),
    );
    assert!(error.contains("takes the holes away"), "{error}");
}

#[test]
fn unstitch_takes_faces_apart_and_reverse_normal_turns_them() {
    let (mut b, walls) = block_and_walls();
    let names = faces(&b.doc, walls);
    // One face out: the walls keep three, a new body the one.
    let uid = add(
        &mut b.doc,
        json!({"type": "unstitch", "body": walls, "faces": [names[1]]}),
    );
    ok(&b.doc, uid);
    assert_eq!(faces(&b.doc, walls).len(), 3);
    assert_eq!(faces(&b.doc, body("F4.b0")), [names[1].clone()]);
    assert_eq!(kind(&b.doc, body("F4.b0")), BodyKind::Sheet);
    b.doc.undo();
    // The solid comes apart into six faces: the first stays the body.
    let uid = add(&mut b.doc, json!({"type": "unstitch", "body": b.body}));
    ok(&b.doc, uid);
    assert_eq!(faces(&b.doc, b.body).len(), 1);
    assert_eq!(b.doc.bodies().len(), 7);
    b.doc.undo();
    // Reverse Normal of the walls; a solid is refused.
    let uid = add(
        &mut b.doc,
        json!({"type": "reverse_normal", "body": walls, "faces": [names[0]]}),
    );
    ok(&b.doc, uid);
    assert!(
        b.doc
            .body_shape(walls)
            .unwrap()
            .history
            .starts_with("reversed(")
    );
    b.doc.undo();
    let uid = add(
        &mut b.doc,
        json!({"type": "reverse_normal", "body": b.body}),
    );
    assert!(
        failure(&b.doc, uid).contains("is a solid"),
        "{}",
        failure(&b.doc, uid)
    );
}

#[test]
fn a_blend_bridges_two_chains_as_a_new_body_or_sewn_on() {
    let (mut b, walls) = block_and_walls();
    let names = faces(&b.doc, walls);
    let top = top_edge(b.extrude, 1, &b.region, 0).to_string();
    let opening = json!({"opening": {"faces": [names[0]]}});
    let uid = add(
        &mut b.doc,
        json!({"type": "blend_surface",
               "from": {"body": b.body, "edges": [top], "face": format!("F2:end({})", b.region)},
               "to": {"body": walls, "openings": [opening], "continuity": "g2", "bulge": 0.5}}),
    );
    ok(&b.doc, uid);
    let blend = body("F4.b0");
    assert_eq!(kind(&b.doc, blend), BodyKind::Sheet);
    assert_eq!(faces(&b.doc, blend), ["F4:blend"]);
    assert_eq!(
        specs(&b.doc, "blend_surface"),
        ["blend_surface(F4: 1 edges, 0 openings, g1 bulge 0, 0 edges, 1 openings, g2 bulge 0.5)"]
    );
    b.doc.undo();
    // Sewn on: into the first body, the second consumed; no new body.
    let uid = add(
        &mut b.doc,
        json!({"type": "blend_surface", "stitch": true,
               "from": {"body": walls, "openings": [opening]},
               "to": {"body": b.body, "edges": [top], "continuity": "g0"}}),
    );
    ok(&b.doc, uid);
    assert_eq!(b.doc.bodies().len(), 1);
    assert!(
        b.doc
            .body_shape(walls)
            .unwrap()
            .history
            .starts_with("stitch")
    );
    b.doc.undo();
    let cases = [
        (
            json!({"type": "blend_surface", "from": {"body": walls}, "to": {"body": b.body, "edges": [top]}}),
            "the blend's first side: no edges selected",
        ),
        (
            json!({"type": "blend_surface", "from": {"body": walls, "openings": [opening]},
                   "to": {"body": b.body, "edges": [top], "bulge": -2}}),
            "bulge must be -0.9 or more",
        ),
    ];
    for (value, expected) in cases {
        let error = refused(&mut b.doc, value);
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn a_boundary_surface_fills_a_loop_or_runs_through_a_network() {
    let (mut b, walls) = block_and_walls();
    let sketch = b.sketch;
    let l1 = line(&mut b.doc, sketch, [100.0, 0.0], [110.0, 0.0]);
    let l2 = line(&mut b.doc, sketch, [110.0, 0.0], [110.0, 10.0]);
    let l3 = line(&mut b.doc, sketch, [110.0, 10.0], [100.0, 10.0]);
    let l4 = line(&mut b.doc, sketch, [100.0, 10.0], [100.0, 0.0]);
    let names = faces(&b.doc, walls);
    let curve = |c: &str| json!({"sketch": sketch, "curves": [c]});
    // A loop of sketch curves (tangent to the sketch's plane) and an
    // opening of the walls.
    let uid = add(
        &mut b.doc,
        json!({"type": "boundary_surface", "boundary": [
            {"sketch": sketch, "curves": [l1, l2], "continuity": "g1"},
            {"sketch": sketch, "curves": [l3, l4]},
            {"body": walls, "openings": [{"opening": {"faces": [names[0]]}}], "continuity": "g1"}]}),
    );
    ok(&b.doc, uid);
    assert_eq!(faces(&b.doc, body("F4.b0")), ["F4:boundary"]);
    assert_eq!(
        specs(&b.doc, "boundary_surface"),
        [format!(
            "boundary_surface(F4: loop [curves {l1}>{l2} g1; curves {l3}>{l4} g0; 0 edges, 1 openings g1])"
        )]
    );
    b.doc.undo();
    // A network two by two, and a loft to a point.
    let uid = add(
        &mut b.doc,
        json!({"type": "boundary_surface", "u": [curve(&l1), curve(&l3)], "v": [curve(&l2), curve(&l4)]}),
    );
    ok(&b.doc, uid);
    let uid = add(
        &mut b.doc,
        json!({"type": "boundary_surface", "u": [curve(&l1), {"point": [105, 5, 10]}]}),
    );
    ok(&b.doc, uid);
    assert_eq!(
        specs(&b.doc, "boundary_surface")[1..],
        [
            format!(
                "boundary_surface(F4: u [curves {l1} g0; curves {l3} g0] v [curves {l2} g0; curves {l4} g0])"
            ),
            format!("boundary_surface(F5: u [curves {l1} g0; point (105, 5, 10)] v [])"),
        ]
    );
    let cases = [
        (
            json!({"type": "boundary_surface", "u": [curve(&l1)]}),
            "two curves or more one way",
        ),
        (
            json!({"type": "boundary_surface", "boundary": [curve(&l1), curve(&l2)], "u": [curve(&l3), curve(&l4)]}),
            "not both",
        ),
        (
            json!({"type": "boundary_surface", "u": [curve(&l1), curve(&l3)], "v": [curve(&l2)]}),
            "crossing curves must be two or more",
        ),
        (
            json!({"type": "boundary_surface", "u": [{"point": [0, 0, 0]}, curve(&l3)], "v": [curve(&l2), curve(&l4)]}),
            "without crossing curves",
        ),
        (
            json!({"type": "boundary_surface", "u": [curve(&l1), {"sketch": sketch, "curves": [l3], "continuity": "g1"}]}),
            "continuity is for the curves of a boundary loop",
        ),
        (
            json!({"type": "boundary_surface", "boundary": [{"sketch": sketch, "curves": [l1], "body": walls}, curve(&l2)]}),
            "one of them",
        ),
    ];
    for (value, expected) in cases {
        let error = refused(&mut b.doc, value);
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn match_surface_rebuilds_a_face_against_a_chain_and_fair_surface_smooths_faces() {
    let (mut b, walls) = block_and_walls();
    let names = faces(&b.doc, walls);
    let top = top_edge(b.extrude, 1, &b.region, 0).to_string();
    let uid = add(
        &mut b.doc,
        json!({"type": "match_surface", "body": walls, "face": names[0], "target": b.body,
               "edges": [top], "continuity": "g1"}),
    );
    ok(&b.doc, uid);
    assert!(
        b.doc
            .body_shape(walls)
            .unwrap()
            .history
            .starts_with("matched(")
    );
    assert_eq!(b.doc.bodies().len(), 2);
    assert_eq!(
        specs(&b.doc, "match_surface"),
        [format!(
            "match_surface({} to 1 edges, 0 openings, g1)",
            names[0]
        )]
    );
    b.doc.undo();
    // Without a face, the body must have only one.
    let uid = add(
        &mut b.doc,
        json!({"type": "match_surface", "body": walls, "target": b.body, "edges": [top]}),
    );
    assert!(
        failure(&b.doc, uid).contains("name the one to match"),
        "{}",
        failure(&b.doc, uid)
    );
    b.doc.undo();
    let error = refused(
        &mut b.doc,
        json!({"type": "match_surface", "body": walls, "target": walls, "edges": [top]}),
    );
    assert!(error.contains("cannot be matched to itself"), "{error}");

    let uid = add(
        &mut b.doc,
        json!({"type": "fair_surface", "body": walls, "faces": [names[1]], "strength": 0.5,
               "iterations": 4, "keep_edges": false, "tolerance": 0.2}),
    );
    ok(&b.doc, uid);
    assert!(
        b.doc
            .body_shape(walls)
            .unwrap()
            .history
            .starts_with("faired(")
    );
    assert_eq!(
        specs(&b.doc, "fair_surface"),
        ["fair_surface(1 faces, strength 0.5, 4 iterations, keep edges false, 0.2)"]
    );
    b.doc.undo();
    let cases = [
        (
            json!({"type": "fair_surface", "body": walls, "tolerance": 0}),
            "tolerance must be greater than zero",
        ),
        (
            json!({"type": "fair_surface", "body": walls, "tolerance": 1, "strength": 1.5}),
            "at most 1",
        ),
        (
            json!({"type": "fair_surface", "body": walls, "tolerance": 1, "iterations": 0}),
            "iterations",
        ),
    ];
    for (value, expected) in cases {
        let error = refused(&mut b.doc, value);
        assert!(error.contains(expected), "{error}");
    }
}
