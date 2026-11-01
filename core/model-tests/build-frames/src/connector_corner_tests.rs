// SPDX-License-Identifier: MIT
//! Corner connectors of a frame (mitcad#1115) with the mock kernel: at
//! each corner of a frame of four profiles, also at the opposite and the
//! mirrored corners, with the rungs turned about their axes or placed by
//! their end B and the posts running down, the connector lies inside the
//! frame against both profiles' faces and its nut lies in the slot of the
//! profile it is screwed to, pointing into it as the connector's frame
//! does, and its screw's head bears on the connector's wall; at a shared
//! mitre the connector seats against the other profile's inner face. The
//! preview, the placed parts, a recompute and the design saved and
//! reopened agree. A design whose connectors the first rule placed (b's
//! own axis, a nut flipped, a screw's head on the profile's face) opens,
//! after `build_upgrade`, with the current placement, a connector moved
//! along its slot since staying where it is.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ValueInput;
use mitcad_model::features::{FeatureDef, GeomRef, JointDef};
use mitcad_model::ids::{ComponentUid, FeatureUid};
use mitcad_model::internal::build::{V3, dot, inverse, moved_box, parts, scale, sub, z_axis};
use mitcad_model::joints::Motion;
use mitcad_model::kernel::BoundingBox;
use mitcad_model::parameters::ParamId;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_build::CONNECT_RULE;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

/// Geometric comparisons allow a nanometre.
const EPS: f64 = 1e-6;

/// How a test frame is built: two posts 1000 long, 500 apart, and two
/// rungs 470 long between them, flush with the posts' ends.
#[derive(Debug, Clone, Copy)]
struct Variant {
    /// The first post's side the rungs stand on: `x+` (the frame towards
    /// +X) or `x-` (mirrored, towards -X).
    side: &'static str,
    /// The rungs turned about their axes, degrees.
    rotate: u32,
    /// The rungs placed by their end B.
    flip: bool,
    /// The posts running down from z = 1000.
    down: bool,
}

const VARIANTS: [Variant; 5] = [
    Variant {
        side: "x+",
        rotate: 0,
        flip: false,
        down: false,
    },
    Variant {
        side: "x-",
        rotate: 0,
        flip: false,
        down: false,
    },
    Variant {
        side: "x+",
        rotate: 90,
        flip: true,
        down: true,
    },
    Variant {
        side: "x-",
        rotate: 180,
        flip: true,
        down: false,
    },
    Variant {
        side: "x+",
        rotate: 270,
        flip: false,
        down: true,
    },
];

/// A frame of four profiles of 30 x 30 in the XZ plane; returns the design
/// and the opening between them: its x range (z is 30 to 970, y -15 to 15).
fn frame(v: Variant) -> (Document<MockKernel>, [f64; 2]) {
    let mut doc = design();
    // Segments cross the boxes' faces: a connector's wall is measured.
    doc.kernel().box_faces.set(true);
    let sign = if v.side == "x+" { 1.0 } else { -1.0 };
    let post = |doc: &mut Document<MockKernel>, x: f64| {
        let (z, d) = if v.down { (1000.0, -1.0) } else { (0.0, 1.0) };
        place(
            doc,
            json!({"part": part("p30"), "at": {"point": [x, 0.0, z], "direction": [0.0, 0.0, d]},
                   "length": 1000}),
        )
    };
    let first_post = post(&mut doc, 0.0);
    let pp = derived(&doc, component(&first_post));
    for from in ["a", "b"] {
        place(
            &mut doc,
            json!({"part": part("p30"),
                   "at": {"occurrence": first_post["occurrence"],
                          "geometry": {"mount": format!("{pp}:mount(slot_{}_1)", v.side)},
                          "from": from, "offset": 0},
                   "rotate": format!("{} deg", v.rotate), "flip": v.flip, "length": 470}),
        );
    }
    post(&mut doc, sign * 500.0);
    all_ok(&doc);
    let opening = if sign > 0.0 {
        [15.0, 485.0]
    } else {
        [-485.0, -15.0]
    };
    (doc, opening)
}

/// A component's placement in the design.
fn world(doc: &Document<MockKernel>, c: ComponentUid) -> Transform {
    let path = doc.placed_in_host(c).unwrap();
    doc.host_placement().after(&doc.path_transform(&path))
}

/// A component's body box in the design.
fn world_box(doc: &Document<MockKernel>, c: ComponentUid) -> BoundingBox {
    moved_box(&doc.part_bounds(c).unwrap(), &world(doc, c))
}

/// A mount frame of a component in the design.
fn frame_of(doc: &Document<MockKernel>, c: ComponentUid, name: &str) -> Transform {
    let local = *doc.part_info(c).unwrap().mount(name).unwrap();
    world(doc, c).after(&local)
}

/// The nut a connector brought: the part it added with a `slot` frame.
fn nut_of(doc: &Document<MockKernel>, connector: ComponentUid) -> ComponentUid {
    let nuts: Vec<ComponentUid> = doc
        .assembly()
        .components
        .iter()
        .filter(|c| {
            c.build
                .as_ref()
                .is_some_and(|b| b.added_by == Some(connector))
        })
        .map(|c| c.uid)
        .filter(|c| {
            doc.part_info(*c)
                .is_some_and(|i| i.mounts.contains_key("slot"))
        })
        .collect();
    assert_eq!(nuts.len(), 1, "the nut of {connector}: {nuts:?}");
    nuts[0]
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= EPS
}

fn same(a: V3, b: V3) -> bool {
    (0..3).all(|i| near(a[i], b[i]))
}

/// A frame of the preview's json.
fn json_frame(v: &Value) -> [V3; 4] {
    let p = |key: &str| -> V3 {
        let a = v[key].as_array().unwrap_or_else(|| panic!("{key} in {v}"));
        [0, 1, 2].map(|i| a[i].as_f64().unwrap())
    };
    [p("origin"), p("x_axis"), p("y_axis"), p("z_axis")]
}

fn same_frame(a: [V3; 4], b: [V3; 4]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| same(*x, *y))
}

/// Whether box `inner` lies in box `outer`.
fn within(inner: &BoundingBox, outer: &BoundingBox) -> bool {
    (0..3).all(|i| inner.min[i] >= outer.min[i] - EPS && inner.max[i] <= outer.max[i] + EPS)
}

/// Whether two boxes share no volume (they may touch).
fn apart(a: &BoundingBox, b: &BoundingBox) -> bool {
    (0..3).any(|i| a.max[i] <= b.min[i] + EPS || b.max[i] <= a.min[i] + EPS)
}

/// The nut's checks: in the slot of profile `a`, pointing into it, on the
/// connector's frame and out of the connector's body.
fn check_nut(doc: &Document<MockKernel>, connector: ComponentUid, a: ComponentUid, what: &str) {
    let nut = nut_of(doc, connector);
    let nut_frame = frame_of(doc, nut, "slot");
    // In the connector's coordinates: on its mount_a, z as its z (both
    // point into the profile).
    let in_connector = inverse(&world(doc, connector)).after(&nut_frame);
    let mount_a = *doc.part_info(connector).unwrap().mount("mount_a").unwrap();
    assert!(
        same_frame(parts(&in_connector), parts(&mount_a)),
        "{what}: the nut's frame in the connector {:?}, its mount_a {:?}",
        parts(&in_connector),
        parts(&mount_a)
    );
    // In the profile's coordinates: on a slot's plane, its z against the
    // slot frame's (into the profile).
    let info = doc.part_info(a).unwrap();
    let in_profile = inverse(&world(doc, a)).after(&nut_frame);
    let z = z_axis(&in_profile);
    let (slot, slot_frame) = info
        .slots()
        .into_iter()
        .max_by(|x, y| (-dot(z_axis(x.1), z)).total_cmp(&-dot(z_axis(y.1), z)))
        .unwrap();
    assert!(
        near(dot(z_axis(slot_frame), z), -1.0),
        "{what}: the nut's z {z:?} in the profile is not into its {slot} ({:?})",
        z_axis(slot_frame)
    );
    assert!(
        near(
            dot(
                sub(in_profile.translation, slot_frame.translation),
                z_axis(slot_frame)
            ),
            0.0
        ),
        "{what}: the nut's frame {:?} is not on {slot}'s face",
        in_profile.translation
    );
    // Its body in the profile, not in the connector.
    let nut_box = world_box(doc, nut);
    assert!(
        within(&nut_box, &world_box(doc, a)),
        "{what}: the nut {nut_box:?} is not in the profile {:?}",
        world_box(doc, a)
    );
    assert!(
        apart(&nut_box, &world_box(doc, connector)),
        "{what}: the nut {nut_box:?} is in the connector {:?}",
        world_box(doc, connector)
    );
}

/// The connectors' and nuts' placements, by component.
fn placements(doc: &Document<MockKernel>, of: &[ComponentUid]) -> Vec<[V3; 4]> {
    of.iter().map(|c| parts(&world(doc, *c))).collect()
}

#[test]
fn corner_connectors_lie_inside_the_frame_with_their_nuts_in_the_slots() {
    for v in VARIANTS {
        let (mut doc, [lo, hi]) = frame(v);
        let junctions = doc.build_junctions(None);
        assert_eq!(
            junctions.iter().map(|j| j.kind).collect::<Vec<_>>(),
            vec!["L"; 4],
            "{v:?}: {junctions:?}"
        );
        let opening = BoundingBox {
            min: [lo, -15.0, 30.0],
            max: [hi, 15.0, 970.0],
        };
        let mut corners = Vec::new();
        let mut placed_parts = Vec::new();
        for j in &junctions {
            let what = format!("{v:?} at {}", j.id);
            // Into b: towards the frame's inside (b's farther end).
            let b_box = world_box(&doc, j.b.component);
            let middle: V3 = [0, 1, 2].map(|i| (b_box.min[i] + b_box.max[i]) / 2.0);
            assert!(
                dot(j.b_axis, sub(middle, j.a.frame.translation)) > 0.0,
                "{what}: b_axis {:?} points out of the frame",
                j.b_axis
            );
            let preview = query(
                &doc,
                json!({"query": "build_preview", "junction": j.id, "part": part("bracket30")}),
            );
            assert_eq!(preview["problems"], json!([]), "{what}: {preview}");
            let answer = command(
                &mut doc,
                json!({"cmd": "build_connect", "junction": j.id, "part": part("bracket30")}),
            )
            .unwrap_or_else(|e| panic!("{what}: {e}"));
            all_ok(&doc);
            let connector = component(&answer);
            // The preview showed it where it went, its nut on its frame.
            assert!(
                same_frame(
                    json_frame(&preview["frame"]),
                    parts(&world(&doc, connector))
                ),
                "{what}: preview {} placed {:?}",
                preview["frame"],
                parts(&world(&doc, connector))
            );
            let nut = nut_of(&doc, connector);
            let shown = &preview["fasteners"][0];
            assert_eq!(shown["frame"], "mount_a", "{what}: {preview}");
            let nut_frame = parts(&frame_of(&doc, nut, "slot"));
            let shown_frame = json_frame(&shown["at"]);
            assert!(
                same(shown_frame[0], nut_frame[0])
                    && same(shown_frame[1], nut_frame[1])
                    && same(shown_frame[3], nut_frame[3]),
                "{what}: preview's nut frame {shown} placed {nut_frame:?}"
            );
            // Inside the frame, flat in its plane (40 x 20 x 40), in a
            // corner of the opening: against both profiles' faces.
            let c = world_box(&doc, connector);
            assert!(within(&c, &opening), "{what}: {c:?} not in {opening:?}");
            assert!(
                near(c.max[0] - c.min[0], 40.0)
                    && near(c.max[1] - c.min[1], 20.0)
                    && near(c.max[2] - c.min[2], 40.0),
                "{what}: {c:?}"
            );
            let corner = [lo, hi]
                .into_iter()
                .flat_map(|x| [30.0, 970.0].map(|z| (x, z)))
                .find(|(x, z)| {
                    (near(c.min[0], *x) || near(c.max[0], *x))
                        && (near(c.min[2], *z) || near(c.max[2], *z))
                });
            let Some(corner) = corner else {
                panic!("{what}: {c:?} is in no corner of the opening");
            };
            corners.push(corner);
            check_nut(&doc, connector, j.a.component, &what);
            // The block is its wall: the head on its far face, 40 back.
            check_screw(&doc, connector, 40.0, &what);
            placed_parts.extend([connector, nut, screw_of(&doc, connector)]);
        }
        // One at each corner: the opposite and the mirrored ones alike.
        corners.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        corners.dedup();
        assert_eq!(corners.len(), 4, "{v:?}: {corners:?}");
        // A recompute and the design saved and reopened keep them there.
        let before = placements(&doc, &placed_parts);
        doc.recompute();
        all_ok(&doc);
        assert!(
            before
                .iter()
                .zip(placements(&doc, &placed_parts))
                .all(|(a, b)| same_frame(*a, b)),
            "{v:?}: a recompute moved them"
        );
        let text = doc.to_json();
        let mut reopened = Document::from_json(&text, MockKernel::default()).unwrap();
        reopened.recompute();
        all_ok(&reopened);
        assert!(
            before
                .iter()
                .zip(placements(&reopened, &placed_parts))
                .all(|(a, b)| same_frame(*a, b)),
            "{v:?}: reopened, they moved"
        );
    }
}

#[test]
fn at_a_shared_mitre_the_connector_seats_on_the_other_profile_s_inner_face() {
    let mut doc = design();
    doc.kernel().box_faces.set(true);
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    // Q on T's end B, along +Y, 400 long: both ends cut by the shared
    // mitre through the outer corner (y = -15, z = 1000).
    let q = place(
        &mut doc,
        json!({"part": part("p30"),
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(end_b)")},
                      "direction": [0.0, 1.0, 0.0]},
               "length": 400, "mitre": {"cut": "shared"}}),
    );
    all_ok(&doc);
    let junctions = doc.build_junctions(None);
    assert_eq!(junctions.len(), 1, "{junctions:?}");
    let j = &junctions[0];
    assert_eq!(j.kind, "L");
    assert_eq!(j.a.component, component(&q));
    // Without its hole: a's end A frame lies on the mitre's long point.
    let answer = command(
        &mut doc,
        json!({"cmd": "build_connect", "junction": j.id, "part": part("bracket30"),
               "machining": false}),
    )
    .unwrap();
    all_ok(&doc);
    let connector = component(&answer);
    // In the inner corner, against T's inner side (y = 15) and Q's
    // underside (z = 970), not 30 into T.
    let c = world_box(&doc, connector);
    let expected = BoundingBox {
        min: [-10.0, 15.0, 930.0],
        max: [10.0, 55.0, 970.0],
    };
    assert!(
        same(c.min, expected.min) && same(c.max, expected.max),
        "{c:?}"
    );
    check_nut(&doc, connector, component(&q), "the shared mitre");
    check_screw(&doc, connector, 40.0, "the shared mitre");
}

/// The screw a connector brought: the part it added without a `slot`
/// frame.
fn screw_of(doc: &Document<MockKernel>, connector: ComponentUid) -> ComponentUid {
    let screws: Vec<ComponentUid> = doc
        .assembly()
        .components
        .iter()
        .filter(|c| {
            c.build
                .as_ref()
                .is_some_and(|b| b.added_by == Some(connector))
        })
        .map(|c| c.uid)
        .filter(|c| {
            !doc.part_info(*c)
                .is_some_and(|i| i.mounts.contains_key("slot"))
        })
        .collect();
    assert_eq!(screws.len(), 1, "the screw of {connector}: {screws:?}");
    screws[0]
}

/// The screw's checks: its head's underside (its design's origin) on the
/// connector's wall, `wall` back from the connector's mount_a away from the
/// profile, its shank (its -z) into the profile.
fn check_screw(doc: &Document<MockKernel>, connector: ComponentUid, wall: f64, what: &str) {
    let screw = world(doc, screw_of(doc, connector));
    let mount = frame_of(doc, connector, "mount_a");
    let head = screw.apply_point([0.0; 3]);
    let seat = sub(mount.translation, scale(z_axis(&mount), wall));
    assert!(
        same(head, seat),
        "{what}: the screw's head at {head:?}, the wall's face at {seat:?}"
    );
    assert!(
        same(
            screw.apply_vector([0.0, 0.0, 1.0]),
            scale(z_axis(&mount), -1.0)
        ),
        "{what}: the screw's axis {:?}",
        screw.apply_vector([0.0, 0.0, 1.0])
    );
}

#[test]
fn the_screw_head_seats_on_the_connector_s_wall() {
    // The angle's arm on the rung is a 4 mm plate: the head 4 mm back from
    // the rung's face, at every corner of the frame.
    let (mut doc, [lo, hi]) = frame(VARIANTS[0]);
    for j in doc.build_junctions(None) {
        let answer = command(
            &mut doc,
            json!({"cmd": "build_connect", "junction": j.id, "part": part("angle30")}),
        )
        .unwrap();
        all_ok(&doc);
        let connector = component(&answer);
        assert_eq!(answer["warnings"], json!([]), "{answer}");
        let c = world_box(&doc, connector);
        let opening = BoundingBox {
            min: [lo, -15.0, 30.0],
            max: [hi, 15.0, 970.0],
        };
        assert!(within(&c, &opening), "{}: {c:?}", j.id);
        check_nut(&doc, connector, j.a.component, &j.id);
        check_screw(&doc, connector, 4.0, &j.id);
    }
}

/// A joint's definition in command form, its values by their parameters'
/// names.
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

/// The joint that places a component.
fn joint_of(doc: &Document<MockKernel>, c: ComponentUid) -> FeatureUid {
    doc.part_info(c)
        .and_then(|i| i.record.joint)
        .or_else(|| {
            doc.assembly()
                .component(c)
                .and_then(|c| c.build.as_ref())
                .and_then(|b| b.joint)
        })
        .unwrap()
}

/// The rule in a component's record.
fn rule_of(doc: &Document<MockKernel>, c: ComponentUid) -> Option<u32> {
    doc.assembly()
        .component(c)
        .and_then(|c| c.build.as_ref())
        .and_then(|b| b.rule)
}

#[test]
fn a_design_placed_by_the_first_rule_opens_with_the_current_placement() {
    let (mut doc, [lo, hi]) = frame(VARIANTS[0]);
    let opening = BoundingBox {
        min: [lo, -15.0, 30.0],
        max: [hi, 15.0, 970.0],
    };
    let mut connectors = Vec::new();
    for j in doc.build_junctions(None) {
        let answer = command(
            &mut doc,
            json!({"cmd": "build_connect", "junction": j.id, "part": part("angle30")}),
        )
        .unwrap();
        connectors.push((component(&answer), j));
    }
    all_ok(&doc);
    // The current placement of each connector, its nut and its screw.
    let hardware =
        |doc: &Document<MockKernel>, c: ComponentUid| [c, nut_of(doc, c), screw_of(doc, c)];
    let current: Vec<[[V3; 4]; 3]> = connectors
        .iter()
        .map(|(c, _)| hardware(&doc, *c).map(|p| parts(&world(&doc, p))))
        .collect();
    for (c, _) in &connectors {
        for p in hardware(&doc, *c) {
            assert_eq!(rule_of(&doc, p), Some(CONNECT_RULE));
        }
    }

    // The same design as the first rule placed it: each connector on its
    // profile a's slot that faces profile b's own +Z axis, its nut's joint
    // flipped, its screw's head on the profile's face (no shift), no rule
    // in the records. One connector at a corner near b's end B, where that
    // slot faces out of the frame, was then moved 5 mm along its slot.
    let mut edits: Vec<(FeatureUid, JointDef<ValueInput>)> = Vec::new();
    let mut moved = None;
    let mut first_rule_outside = Vec::new();
    for (c, j) in &connectors {
        let a = doc.part_info(j.a.component).unwrap();
        let b_up = world(&doc, j.b.component).apply_vector([0.0, 0.0, 1.0]);
        let world_a = world(&doc, j.a.component);
        let (slot, _) = a
            .slots()
            .into_iter()
            .max_by(|x, y| {
                let d = |t: &Transform| dot(world_a.apply_vector(z_axis(t)), b_up);
                d(x.1).total_cmp(&d(y.1))
            })
            .unwrap();
        let joint = joint_of(&doc, *c);
        let mut def = joint_def(&doc, joint);
        let GeomRef::Mount { name, .. } = &mut def.b.geometry else {
            panic!("{c}: its joint is not on a mount");
        };
        if name != slot {
            *name = slot.clone();
            first_rule_outside.push(*c);
            if moved.is_none() {
                let FeatureDef::Joint(raw) = &doc.feature(joint).unwrap().def else {
                    panic!("no joint");
                };
                let tx = doc
                    .parameters()
                    .value(*raw.position.get(&Motion::Tx).unwrap())
                    .unwrap();
                def.position
                    .insert(Motion::Tx, ValueInput::Number(tx + 5.0));
                moved = Some(*c);
            }
        }
        edits.push((joint, def));
        let nut = joint_of(&doc, nut_of(&doc, *c));
        let mut nut_def = joint_def(&doc, nut);
        nut_def.flip = true;
        edits.push((nut, nut_def));
        let screw = joint_of(&doc, screw_of(&doc, *c));
        let mut screw_def = joint_def(&doc, screw);
        assert!(
            screw_def.a.frame_override.is_some(),
            "{c}: the screw's seat"
        );
        screw_def.a.frame_override = None;
        edits.push((screw, screw_def));
    }
    assert_eq!(first_rule_outside.len(), 2, "the corners near b's end B");
    let moved = moved.unwrap();
    let all: Vec<ComponentUid> = connectors
        .iter()
        .flat_map(|(c, _)| hardware(&doc, *c))
        .collect();
    doc.apply(|state| {
        for (joint, def) in &edits {
            state.edit_feature(*joint, &FeatureDef::Joint(def.clone()))?;
        }
        for c in &all {
            if let Some(record) = state
                .assembly
                .component_mut(*c)
                .and_then(|c| c.build.as_mut())
            {
                record.rule = None;
            }
        }
        Ok(("Placed by the first rule".to_owned(), ()))
    })
    .unwrap();
    all_ok(&doc);
    let old_text = doc.to_json();
    assert!(!old_text.contains("\"rule\""), "{old_text}");

    // Opened: as the first rule placed them, two connectors outside the
    // frame and every nut in its connector.
    let mut opened = Document::from_json(&old_text, MockKernel::default()).unwrap();
    opened.kernel().box_faces.set(true);
    opened.recompute();
    all_ok(&opened);
    for c in &first_rule_outside {
        assert!(
            !within(&world_box(&opened, *c), &opening),
            "{c} was inside the frame by the first rule"
        );
    }
    for (c, _) in &connectors {
        let nut_box = world_box(&opened, nut_of(&opened, *c));
        assert!(
            !apart(&nut_box, &world_box(&opened, *c)),
            "{c}: by the first rule its nut lay in it"
        );
    }
    let moved_at = parts(&world(&opened, moved));

    // Placed again by the current rule, in one undo step.
    let depth = opened.undo_depth();
    let answer = command(&mut opened, json!({"cmd": "build_upgrade"})).unwrap();
    all_ok(&opened);
    assert_eq!(opened.undo_depth(), depth + 1);
    assert_eq!(
        opened.undo_label(),
        Some("Place connectors by the current rule")
    );
    // The three unmoved connectors that moved (one stays as it was, the
    // nut and the screw of each of the four).
    assert_eq!(answer["count"], json!(1 + 4 * 2), "{answer}");
    for ((c, j), expected) in connectors.iter().zip(&current) {
        let what = format!("{c} at {}", j.id);
        if *c == moved {
            // Moved since: it stays; its nut and screw seat on it.
            assert!(same_frame(parts(&world(&opened, *c)), moved_at), "{what}");
            assert_eq!(rule_of(&opened, *c), None, "{what}");
        } else {
            let got = hardware(&opened, *c).map(|p| parts(&world(&opened, p)));
            for (g, e) in got.iter().zip(expected) {
                assert!(same_frame(*g, *e), "{what}: {g:?} not {e:?}");
            }
            assert!(within(&world_box(&opened, *c), &opening), "{what}");
            assert_eq!(rule_of(&opened, *c), Some(CONNECT_RULE), "{what}");
        }
        check_nut(&opened, *c, j.a.component, &what);
        check_screw(&opened, *c, 4.0, &what);
        assert_eq!(rule_of(&opened, nut_of(&opened, *c)), Some(CONNECT_RULE));
        assert_eq!(rule_of(&opened, screw_of(&opened, *c)), Some(CONNECT_RULE));
    }

    // Saved and opened again: the same; nothing more to place again.
    let text = opened.to_json();
    let mut again = Document::from_json(&text, MockKernel::default()).unwrap();
    again.kernel().box_faces.set(true);
    again.recompute();
    all_ok(&again);
    let depth = again.undo_depth();
    let answer = command(&mut again, json!({"cmd": "build_upgrade"})).unwrap();
    assert_eq!(answer["count"], json!(0), "{answer}");
    assert_eq!(again.undo_depth(), depth);
    for (c, _) in &connectors {
        for p in hardware(&opened, *c) {
            assert!(same_frame(
                parts(&world(&again, p)),
                parts(&world(&opened, p))
            ));
        }
    }
}
