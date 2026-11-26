// SPDX-License-Identifier: MIT
//! A mirror of features rebuilds an extrusion whose end depends on bodies
//! at the mirrored place (mitcad#2430), as a pattern's Adjust does: its
//! profile and direction reflected, its end found again there. With
//! `identical` the finished tool is reflected, as before; a mirror saved
//! without `compute` (a design written before) keeps reflecting it until
//! it is edited. With the mock kernel, which builds every extrusion up to
//! an object 10 mm long, the tests check what the kernel is asked to
//! build; the CLI fixture `cli.mirror_to_next` checks the geometry: the
//! image ends on its own next face where the body's far side is lower.
//!
//! Ribs and webs To Next (mitcad#2431) are rebuilt at the image the same
//! way, their sides taken about the reflected frame: a rib's side 1 and
//! side 2 swap, its flip, direction and Normal to Curves far side are the
//! images of the original's, and a web is flipped. The CLI fixture
//! `cli.mirror_rib_to_next` checks the geometry.
//!
//! A pattern with Adjust of such a mirror builds the image again at each
//! of its copies (mitcad#2432; geometry: `cli.mirror_pattern_to_next`).

use mitcad_model::features::{ComputeOption, FeatureDef, MirrorDef};
use mitcad_model::ids::FeatureUid;
use mitcad_model::test_support::{block, def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

fn mirror_of(doc: &Document<MockKernel>, uid: FeatureUid) -> MirrorDef {
    let FeatureDef::Mirror(mirror) = &doc.feature(uid).unwrap().def else {
        panic!("a mirror")
    };
    mirror.clone()
}

/// The extrusions the kernel was asked to build for `feature`: its own and
/// the copies rebuilt from its definition.
fn built(doc: &Document<MockKernel>, feature: FeatureUid) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|s| s.starts_with(&format!("extrude {feature} ")))
        .cloned()
        .collect()
}

/// `value` without the key `key` in any of its objects.
fn without(value: &mut Value, key: &str) {
    match value {
        Value::Object(map) => {
            map.remove(key);
            for v in map.values_mut() {
                without(v, key);
            }
        }
        Value::Array(items) => {
            for v in items {
                without(v, key);
            }
        }
        _ => {}
    }
}

/// The block (60 x 40 x 20, `F2.b0`) with a boss sketched on the XY plane
/// at (10, 10), started 30 mm above it and extruded down to the next face
/// of the block (the body, not through it), joined; and the mirror of
/// the boss across the plane x = 30 with `extra` fields.
fn boss_and_mirror(extra: Value) -> (Document<MockKernel>, FeatureUid, Value) {
    let mut b = block();
    let sketch = b
        .doc
        .add_sketch(mitcad_model::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    let region = b
        .doc
        .add_circle(sketch, [10.0, 10.0], &num(10.0))
        .unwrap()
        .region;
    let boss = add(
        &mut b.doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "start": {"type": "offset", "offset": 30}, "flip": true,
               "extent": {"type": "to_object", "object": {"type": "body", "body": "F2.b0"}},
               "operation": "join", "participants": ["F2.b0"]}),
    );
    let mut mirror = json!({"type": "mirror",
        "objects": {"type": "features", "features": [boss]},
        "plane": {"type": "fixed", "origin": [30, 0, 0], "normal": [1, 0, 0]}});
    for (key, item) in extra.as_object().unwrap() {
        mirror[key] = item.clone();
    }
    (b.doc, boss, mirror)
}

#[test]
fn a_mirror_rebuilds_an_extrusion_to_the_next_face_at_the_image() {
    let (mut doc, boss, mirror) = boss_and_mirror(json!({}));
    let original = built(&doc, boss);
    assert_eq!(original.len(), 1, "{original:?}");
    assert!(
        original[0].contains("from offset 30: to body"),
        "{original:?}"
    );
    let uid = add(&mut doc, mirror.clone());
    // Added without a compute option: Adjust, stored with it.
    assert_eq!(mirror_of(&doc, uid).compute, Some(ComputeOption::Adjust));
    assert!(mirror_of(&doc, uid).rebuilds_reflected_tools());
    // The boss is built again for the image, down to the block's next
    // face from there. The reflected profile's frame (its x axis turned
    // over) has its own normal downwards: the start 30 mm above the XY
    // plane is -30 along it, and the extrusion still runs down.
    let all = built(&doc, boss);
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(all[1].contains("from offset -30: to body"), "{all:?}");
    assert!(all[1].contains("through false"), "{all:?}");
    let along = all[1]
        .split("along [")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .expect("a direction");
    let z: f64 = along.split(", ").nth(2).unwrap().parse().unwrap();
    assert!((z + 1.0).abs() < 1e-12, "{all:?}");

    // Identical reflects the boss's finished tool: nothing is built again.
    let mut identical = mirror.clone();
    identical["compute"] = json!("identical");
    doc.edit_feature(uid, &def(identical)).unwrap();
    assert_eq!(mirror_of(&doc, uid).compute, Some(ComputeOption::Identical));
    assert!(!mirror_of(&doc, uid).rebuilds_reflected_tools());
    assert_eq!(built(&doc, boss).len(), 2);
    // Back to Adjust (its earlier result comes from the cache).
    doc.edit_feature(uid, &def(mirror)).unwrap();
    assert_eq!(mirror_of(&doc, uid).compute, Some(ComputeOption::Adjust));
}

#[test]
fn a_mirror_saved_before_reflects_the_finished_tool_until_it_is_edited() {
    let (mut doc, boss, mirror) = boss_and_mirror(json!({}));
    let uid = add(&mut doc, mirror.clone());
    let saved = doc.to_json();
    assert!(saved.contains("\"compute\""), "{saved}");
    let reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(
        mirror_of(&reopened, uid).compute,
        Some(ComputeOption::Adjust)
    );

    // A design written before mitcad#2430: no compute option stored.
    let mut old: Value = serde_json::from_str(&saved).unwrap();
    without(&mut old, "compute");
    let mut old = Document::from_json(&old.to_string(), MockKernel::default()).unwrap();
    old.recompute();
    assert_eq!(mirror_of(&old, uid).compute, None);
    assert!(!mirror_of(&old, uid).rebuilds_reflected_tools());
    // Only the boss itself is built: the mirror reflects its tool.
    assert_eq!(built(&old, boss).len(), 1, "{:?}", built(&old, boss));
    // Edited, it takes Adjust and builds the image again; undo brings the
    // old mirror back.
    old.edit_feature(uid, &def(mirror)).unwrap();
    assert_eq!(mirror_of(&old, uid).compute, Some(ComputeOption::Adjust));
    assert_eq!(built(&old, boss).len(), 2, "{:?}", built(&old, boss));
    old.undo();
    assert_eq!(mirror_of(&old, uid).compute, None);
}

#[test]
fn a_mirror_of_bodies_stores_no_compute_option() {
    let mut b = block();
    let uid = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "plane": {"type": "fixed", "origin": [80, 0, 0], "normal": [1, 0, 0]}}),
    );
    assert_eq!(mirror_of(&b.doc, uid).compute, None);
}

/// The extrusions built for `feature` since the specs were cleared, split
/// into those whose profile is the original's (started 30 mm above the XY
/// plane along its normal) and those whose profile is reflected (-30 along
/// the reflected frame's normal).
fn built_sides(doc: &Document<MockKernel>, feature: FeatureUid) -> (usize, usize) {
    let all = built(doc, feature);
    let moved = all.iter().filter(|s| s.contains("from offset 30:")).count();
    let reflected = all
        .iter()
        .filter(|s| s.contains("from offset -30:"))
        .count();
    assert_eq!(moved + reflected, all.len(), "{all:?}");
    (moved, reflected)
}

/// A rectangular pattern of two along y, spacing 20, of `features`: the
/// copies of the boss at (10, 30) and of its image at (50, 30) stand on the
/// block (60 x 40) as the originals do, so that each reaches it.
fn pattern_of(features: Value, compute: &str) -> Value {
    json!({"type": "rectangular_pattern",
           "objects": {"type": "features", "features": features},
           "direction1": {"axis": {"type": "origin", "axis": "y"}, "quantity": 2,
                          "distance": 20},
           "distance_type": "spacing", "compute": compute})
}

/// Clears the kernel's record of what it was asked to build.
fn forget_built(doc: &Document<MockKernel>) {
    doc.kernel().specs.borrow_mut().clear();
}

#[test]
fn a_pattern_of_a_mirror_rebuilds_the_mirrored_tool_at_each_copy() {
    let (mut doc, boss, mirror) = boss_and_mirror(json!({}));
    let uid = add(&mut doc, mirror);
    assert!(mirror_of(&doc, uid).rebuilds_reflected_tools());
    forget_built(&doc);
    // The pattern's copy holds the boss moved and the boss's image moved:
    // both are built again there (mitcad#2432), the image with the
    // reflected profile, so that it ends on the bodies at the copy.
    let pattern = add(&mut doc, pattern_of(json!([uid]), "adjust"));
    let (moved, reflected) = built_sides(&doc, boss);
    assert!(
        moved >= 1 && reflected >= 1,
        "{:?} {:?}",
        status(&doc, pattern),
        built(&doc, boss)
    );

    // A pattern with Identical moves every finished tool.
    forget_built(&doc);
    doc.edit_feature(pattern, &def(pattern_of(json!([uid]), "identical")))
        .unwrap();
    assert_eq!(built_sides(&doc, boss), (0, 0), "{:?}", built(&doc, boss));
}

#[test]
fn a_pattern_of_a_mirror_with_identical_moves_the_reflected_tool() {
    // The mirror with Identical reflects the finished tool: the pattern
    // moves that reflected tool and builds the moved boss only. (A design
    // of its own: an edit back to an earlier definition takes the earlier
    // results from the cache, building nothing.)
    let (mut doc, boss, mirror) = boss_and_mirror(json!({"compute": "identical"}));
    let uid = add(&mut doc, mirror);
    assert!(!mirror_of(&doc, uid).rebuilds_reflected_tools());
    forget_built(&doc);
    let pattern = add(&mut doc, pattern_of(json!([uid]), "adjust"));
    let (moved, reflected) = built_sides(&doc, boss);
    assert!(
        moved >= 1 && reflected == 0,
        "{:?} {:?}",
        status(&doc, pattern),
        built(&doc, boss)
    );
}

#[test]
fn a_pattern_of_a_mirror_saved_before_moves_the_reflected_tool() {
    let (mut doc, boss, mirror) = boss_and_mirror(json!({}));
    let uid = add(&mut doc, mirror);
    add(&mut doc, pattern_of(json!([uid]), "adjust"));
    // A design written before mitcad#2430: no compute option stored; the
    // pattern's is Adjust, the default, which is not written either.
    let mut old: Value = serde_json::from_str(&doc.to_json()).unwrap();
    without(&mut old, "compute");
    let mut old = Document::from_json(&old.to_string(), MockKernel::default()).unwrap();
    old.recompute();
    assert!(!mirror_of(&old, uid).rebuilds_reflected_tools());
    // The boss itself and its moved copy are built; the image and its
    // copy are the reflected finished tool.
    let (moved, reflected) = built_sides(&old, boss);
    assert!(moved >= 2 && reflected == 0, "{:?}", built(&old, boss));
}

// Ribs and webs To Next (mitcad#2431).

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

/// The ribs or webs the kernel was asked to build for `feature`: its own
/// and the copies rebuilt from its definition.
fn thin_built(doc: &Document<MockKernel>, kind: &str, feature: FeatureUid) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|s| s.starts_with(&format!("{kind} {feature} ")))
        .cloned()
        .collect()
}

/// A sketch on XY with lines through `points` one after another; the
/// sketch and the lines' curve ids.
fn sketch_lines(doc: &mut Document<MockKernel>, points: &[[f64; 2]]) -> (String, Vec<String>) {
    let created: Value =
        serde_json::from_str(&doc.command(r#"{"cmd": "sketch.create"}"#).unwrap()).unwrap();
    let sketch = created["uid"].as_str().unwrap().to_owned();
    let mut curves = Vec::new();
    for pair in points.windows(2) {
        let command = json!({"cmd": "sketch.add_line", "sketch": sketch,
                             "start": pair[0], "end": pair[1]});
        let added: Value =
            serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap();
        curves.push(added["made"][0].as_str().unwrap().to_owned());
    }
    (sketch, curves)
}

/// The mirror of `feature` across the plane x = 30 (Adjust, the default).
fn mirror_at_30(feature: FeatureUid) -> Value {
    json!({"type": "mirror", "objects": {"type": "features", "features": [feature]},
           "plane": {"type": "fixed", "origin": [30, 0, 0], "normal": [1, 0, 0]}})
}

/// The point the mirror across x = 30 takes `p` to.
fn reflected(p: [f64; 3]) -> [f64; 3] {
    [60.0 - p[0], p[1], p[2]]
}

/// The numbers of a `[x, y, z]` the mock kernel wrote.
fn point(text: &str) -> [f64; 3] {
    let numbers: Vec<f64> = text
        .trim_matches(|c| c == '[' || c == ']')
        .split(", ")
        .map(|n| n.parse().unwrap())
        .collect();
    [numbers[0], numbers[1], numbers[2]]
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
}

/// The direction a rib's spec sends.
fn direction(spec: &str) -> [f64; 3] {
    let rest = spec
        .split(" direction [")
        .nth(1)
        .unwrap_or_else(|| panic!("no direction in {spec}"));
    point(rest.split(']').next().unwrap())
}

/// The far side's pieces of a rib's spec: each one's name and ends.
fn far_side(spec: &str) -> Vec<(String, [f64; 3], [f64; 3])> {
    let (_, far) = spec
        .split_once(" far ")
        .unwrap_or_else(|| panic!("no far side in {spec}"));
    far.split("; ")
        .map(|piece| {
            let (name, rest) = piece.split_once(' ').unwrap();
            let (start, end) = rest.split_once("] [").unwrap();
            (name.to_owned(), point(start), point(end))
        })
        .collect()
}

#[test]
fn a_mirror_rebuilds_a_rib_to_the_next_face_at_the_image() {
    let mut b = block();
    let (sketch, curves) = sketch_lines(&mut b.doc, &[[10.0, 5.0], [10.0, 35.0]]);
    // Side 1 (along the sketch normal, +Z), grown along the sketch's -x.
    let rib = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": sketch, "curves": [curves[0]]},
               "thickness": 4, "thickness_location": "side1", "extent": {"type": "to_next"},
               "direction": "sketch_x", "flip": true}),
    );
    assert_eq!(status(&b.doc, rib), FeatureStatus::Ok);
    let original = thin_built(&b.doc, "rib", rib);
    assert_eq!(original.len(), 1, "{original:?}");
    assert!(
        original[0].contains("thickness 4 Side1 depth None flip true"),
        "{original:?}"
    );
    assert!(
        near(direction(&original[0]), [1.0, 0.0, 0.0]),
        "{original:?}"
    );

    let mirror = mirror_at_30(rib);
    let uid = add(&mut b.doc, mirror.clone());
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    // Built again for the image. The reflected frame's normal is -Z, so
    // the original's side 1 (+Z, which the upright mirror plane keeps) is
    // the image's side 2. The flip stays and the direction is reflected:
    // the image grows along +X, away from the mirror plane as the
    // original grows along -X.
    let all = thin_built(&b.doc, "rib", rib);
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(
        all[1].contains("thickness 4 Side2 depth None flip true"),
        "{all:?}"
    );
    assert!(near(direction(&all[1]), [-1.0, 0.0, 0.0]), "{all:?}");

    // Identical reflects the rib's finished tool: nothing is built again.
    let mut identical = mirror.clone();
    identical["compute"] = json!("identical");
    b.doc.edit_feature(uid, &def(identical)).unwrap();
    assert_eq!(thin_built(&b.doc, "rib", rib).len(), 2);
    // Back to Adjust (its earlier result comes from the cache, as in
    // a_mirror_rebuilds_an_extrusion_to_the_next_face_at_the_image).
    b.doc.edit_feature(uid, &def(mirror)).unwrap();
    assert_eq!(mirror_of(&b.doc, uid).compute, Some(ComputeOption::Adjust));
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
}

#[test]
fn a_mirrored_web_to_the_next_face_is_flipped_in_the_reflected_frame() {
    let mut b = block();
    let (sketch, curves) = sketch_lines(&mut b.doc, &[[10.0, 5.0], [10.0, 35.0]]);
    // Down (against the sketch normal +Z) to the next faces, its thickness
    // to the left of the line (side 1, -X).
    let web = add(
        &mut b.doc,
        json!({"type": "web", "curves": {"sketch": sketch, "curves": [curves[0]]},
               "thickness": 2, "thickness_location": "side1", "extent": {"type": "to_next"},
               "flip": true}),
    );
    assert_eq!(status(&b.doc, web), FeatureStatus::Ok);
    let uid = add(&mut b.doc, mirror_at_30(web));
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    // The reflected frame's normal is -Z: down is along it, no flip. Side
    // 1 to the left of the reflected line about that normal is +X, the
    // image of the original's -X: kept.
    let all = thin_built(&b.doc, "web", web);
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(
        all[0].contains("thickness 2 Side1 depth None flip true"),
        "{all:?}"
    );
    assert!(
        all[1].contains("thickness 2 Side1 depth None flip false"),
        "{all:?}"
    );
}

#[test]
fn a_mirrored_rib_normal_to_curves_has_the_reflected_far_side() {
    let mut b = block();
    let (sketch, curves) = sketch_lines(&mut b.doc, &[[5.0, 5.0], [15.0, 5.0], [25.0, 15.0]]);
    // Normal to Curves runs a depth; Extend Ends makes it depend on the
    // bodies, so the mirror rebuilds it.
    let rib = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": sketch, "curves": curves},
               "thickness": 2, "extent": {"type": "depth", "depth": 4},
               "direction": "normal", "extend_ends": true}),
    );
    assert_eq!(status(&b.doc, rib), FeatureStatus::Ok);
    let uid = add(&mut b.doc, mirror_at_30(rib));
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let all = thin_built(&b.doc, "rib", rib);
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(all[1].contains("flip false"), "{all:?}");
    // The image's far side is the original's reflected, piece by piece:
    // the reflected chain's offset to its left about the reflected frame's
    // normal.
    let (original, image) = (far_side(&all[0]), far_side(&all[1]));
    assert_eq!(original.len(), image.len(), "{all:?}");
    assert!(!original.is_empty());
    for ((name, start, end), (image_name, image_start, image_end)) in original.iter().zip(&image) {
        assert_eq!(name, image_name, "{all:?}");
        assert!(near(reflected(*start), *image_start), "{all:?}");
        assert!(near(reflected(*end), *image_end), "{all:?}");
    }
}
