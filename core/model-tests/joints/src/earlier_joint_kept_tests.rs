// SPDX-License-Identifier: MIT
//! A new joint moves only what it needs to hold (mitcad#2199): a component
//! an earlier joint holds on an ungrounded carrier keeps its place on the
//! carrier when a new joint from a third part to the carrier is added, in
//! the new joint's preview and after `add_joint`, for every joint kind of
//! the earlier joint.
//!
//! Links are components without bodies: their joints are on points of
//! their coordinates (a frame along the component's axes).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::assembly::inverse;
use mitcad_model::datum::Vec3;
use mitcad_model::ids::OccurrenceUid;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

/// How far the carrier turns about the base's pivot until it meets the
/// grounded screw.
const TURN: f64 = 0.7;
/// Where the cover's joint is on the carrier in the loop.
const COVER_AT: Vec3 = [50.0, 0.0, 10.0];
/// Where the screw's joint is on the carrier.
const SCREW_AT: Vec3 = [0.0, 40.0, 0.0];
/// Where the free screw is before its joint.
const SCREW_FROM: Vec3 = [300.0, -200.0, 40.0];

/// The earlier joint's kinds: every kind with a free motion, and a rigid
/// one. The slider slides along y, which the carrier's turn moves the
/// cover along.
const KINDS: [(&str, Option<&str>); 7] = [
    ("rigid", None),
    ("revolute", None),
    ("slider", Some("y")),
    ("cylindrical", None),
    ("pin_slot", None),
    ("planar", None),
    ("ball", None),
];

fn run(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn shift(p: Vec3) -> Transform {
    Transform::translation(p)
}

fn turn(angle: f64) -> Transform {
    Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], angle).unwrap()
}

/// A component without bodies, its occurrence at `at` in the root.
fn link(doc: &mut Document<MockKernel>, name: &str, at: Transform) -> OccurrenceUid {
    let (_, o) = doc.create_component(Some(name), at, false).unwrap();
    o
}

/// An origin on a point of an occurrence's component.
fn pin(occurrence: OccurrenceUid, p: Vec3) -> Value {
    json!({"occurrence": occurrence.to_string(), "geometry": {"point": p}})
}

/// A joint of `kind` (with its slide axis) from the cover's origin to the
/// carrier's point `at`.
fn earlier_joint(
    (kind, slide): (&str, Option<&str>),
    cover: OccurrenceUid,
    carrier: OccurrenceUid,
    at: Vec3,
) -> Value {
    let mut def = json!({"kind": kind, "a": pin(cover, [0.0; 3]), "b": pin(carrier, at)});
    if let Some(axis) = slide {
        def["slide_axis"] = json!(axis);
    }
    def
}

/// Adds a joint (`add_joint`'s fields).
fn add_joint(doc: &mut Document<MockKernel>, mut def: Value) {
    def["cmd"] = json!("add_joint");
    let result = run(doc, def);
    assert_eq!(result["error"], Value::Null, "{result}");
}

/// A Move feature at the end of the timeline: `occurrences` by `motion`
/// (a `move_occurrence` transform).
fn move_occurrences(doc: &mut Document<MockKernel>, occurrences: &[OccurrenceUid], motion: Value) {
    let uids: Vec<String> = occurrences.iter().map(|o| o.to_string()).collect();
    let result = run(
        doc,
        json!({"cmd": "add_feature",
               "def": {"type": "move_occurrence", "occurrences": uids, "transform": motion}}),
    );
    assert_eq!(result["error"], Value::Null, "{result}");
}

/// A turn about the root's z axis.
fn about_z(angle: f64) -> Value {
    json!({"type": "rotate", "axis": {"type": "origin", "axis": "z"}, "angle": angle})
}

/// The new joint: rigid, from the screw's origin to the carrier's point
/// [`SCREW_AT`], as an `add_feature` definition.
fn screw_joint(screw: OccurrenceUid, carrier: OccurrenceUid) -> Value {
    json!({"type": "joint", "kind": "rigid",
           "a": pin(screw, [0.0; 3]), "b": pin(carrier, SCREW_AT)})
}

/// The 4 x 4 a preview's `placements` give `o`, else its placement now.
fn previewed(report: &Value, doc: &Document<MockKernel>, o: OccurrenceUid) -> Transform {
    let found = report["placements"].as_array().and_then(|placements| {
        placements
            .iter()
            .find(|p| p["path"] == o.to_string())
            .map(|p| &p["transform"])
    });
    let Some(m) = found else {
        return doc.placement(o).unwrap();
    };
    let at = |r: usize, c: usize| m[r][c].as_f64().unwrap();
    Transform {
        linear: std::array::from_fn(|r| std::array::from_fn(|c| at(r, c))),
        translation: std::array::from_fn(|r| at(r, 3)),
    }
}

/// `b` is `a` to 0.000001 mm at points around the parts.
fn assert_same(a: &Transform, b: &Transform, what: &str) {
    for p in [
        [0.0; 3],
        [100.0, 0.0, 0.0],
        [0.0, 100.0, 0.0],
        [0.0, 0.0, 100.0],
    ] {
        let (pa, pb) = (a.apply_point(p), b.apply_point(p));
        assert!(
            (0..3).all(|i| (pa[i] - pb[i]).abs() < 1e-6),
            "{what}: {pa:?} != {pb:?}"
        );
    }
}

/// A placement in `carrier`'s coordinates.
fn on(carrier: &Transform, o: &Transform) -> Transform {
    inverse(carrier).after(o)
}

/// The cover's placement on the carrier stays as the earlier joint holds
/// it while the new joint `def` (an `add_feature` definition) is added:
/// in its preview and after `add_joint`. The carrier's placement after
/// the joint.
fn assert_cover_kept(
    doc: &mut Document<MockKernel>,
    cover: OccurrenceUid,
    carrier: OccurrenceUid,
    def: Value,
    what: &str,
) -> Transform {
    let before = on(
        &doc.placement(carrier).unwrap(),
        &doc.placement(cover).unwrap(),
    );
    let text = json!({"cmd": "add_feature", "def": def}).to_string();
    let report: Value = serde_json::from_str(&doc.preview(&text).unwrap()).unwrap();
    assert_eq!(report["status"], "ok", "{what}: {report}");
    let previewed_on = on(
        &previewed(&report, doc, carrier),
        &previewed(&report, doc, cover),
    );
    assert_same(
        &previewed_on,
        &before,
        &format!("{what}, preview: {report}"),
    );
    let mut joint = def;
    joint.as_object_mut().unwrap().remove("type");
    add_joint(doc, joint);
    let carrier_now = doc.placement(carrier).unwrap();
    let after = on(&carrier_now, &doc.placement(cover).unwrap());
    assert_same(&after, &before, &format!("{what}, after add_joint"));
    carrier_now
}

/// A grounded base with the carrier on a revolute joint about its z axis
/// (the carrier itself ungrounded), the cover on the carrier by an earlier
/// joint of each kind, and a grounded screw where the carrier's point
/// [`SCREW_AT`] is when the carrier has turned by [`TURN`]: the new rigid
/// joint from the screw to the carrier closes a loop that turns the
/// carrier, which the solver iterates. The cover follows the carrier
/// instead of keeping still while the earlier joint's free motions take
/// up the turn.
#[test]
fn a_loop_a_new_joint_closes_keeps_what_an_earlier_joint_holds_on_the_carrier() {
    for kind in KINDS {
        let mut doc = Document::new(MockKernel::default());
        let base = link(&mut doc, "Base", Transform::IDENTITY);
        doc.set_occurrence_grounded(base, true).unwrap();
        let carrier = link(&mut doc, "Carrier", Transform::IDENTITY);
        let cover = link(&mut doc, "Cover", shift(COVER_AT));
        let screw = link(&mut doc, "Screw", turn(TURN).after(&shift(SCREW_AT)));
        doc.set_occurrence_grounded(screw, true).unwrap();
        add_joint(
            &mut doc,
            json!({"kind": "revolute", "a": pin(carrier, [0.0; 3]), "b": pin(base, [0.0; 3])}),
        );
        add_joint(&mut doc, earlier_joint(kind, cover, carrier, COVER_AT));
        let what = format!("an earlier {} joint in a loop", kind.0);
        let carrier_now =
            assert_cover_kept(&mut doc, cover, carrier, screw_joint(screw, carrier), &what);
        // The carrier turned onto the screw.
        assert_same(&carrier_now, &turn(TURN), &format!("{what}: the carrier"));
    }
}

/// The carrier free (nothing grounded), the cover on it by an earlier
/// joint of each kind on the carrier's origin, placed by it at its start
/// values 0 (as the Joint command writes them) and then moved by a Move
/// along the joint's free motions (a turn about z, a slide along y; the
/// rigid joint's cover stays): a new rigid joint from a free screw to the
/// carrier moves the screw onto the carrier, and the carrier and the cover
/// stay where they are, the earlier joint not placed again at its start
/// values.
#[test]
fn a_new_joint_on_a_free_carrier_leaves_the_earlier_joint_where_it_is() {
    for kind in KINDS {
        let mut doc = Document::new(MockKernel::default());
        let carrier = link(&mut doc, "Carrier", Transform::IDENTITY);
        let cover = link(&mut doc, "Cover", Transform::IDENTITY);
        let screw = link(&mut doc, "Screw", shift(SCREW_FROM));
        let mut earlier = earlier_joint(kind, cover, carrier, [0.0; 3]);
        let (start, motion) = match kind.0 {
            "rigid" => (json!({}), None),
            "revolute" => (json!({"rz": 0}), Some(about_z(0.5))),
            "slider" => (
                json!({"ty": 0}),
                Some(json!({"type": "translate_xyz", "x": 0, "y": 30, "z": 0})),
            ),
            "cylindrical" => (json!({"tz": 0, "rz": 0}), Some(about_z(0.5))),
            "pin_slot" => (json!({"tx": 0, "rz": 0}), Some(about_z(0.5))),
            "planar" => (json!({"tx": 0, "ty": 0, "rz": 0}), Some(about_z(0.5))),
            _ => (json!({"rz": 0, "ry": 0, "rx": 0}), Some(about_z(0.5))),
        };
        earlier["start"] = start;
        add_joint(&mut doc, earlier);
        if let Some(motion) = motion {
            move_occurrences(&mut doc, &[cover], motion);
        }
        let what = format!("an earlier {} joint on a free carrier", kind.0);
        let carrier_was = doc.placement(carrier).unwrap();
        let carrier_now =
            assert_cover_kept(&mut doc, cover, carrier, screw_joint(screw, carrier), &what);
        assert_same(&carrier_now, &carrier_was, &format!("{what}: the carrier"));
        // The screw is on the carrier.
        assert_same(
            &doc.placement(screw).unwrap(),
            &carrier_now.after(&shift(SCREW_AT)),
            &format!("{what}: the screw"),
        );
    }
}

/// The carrier on a revolute joint to a grounded base, placed by it at its
/// start value 0 and then turned about the joint's axis with the cover a
/// rigid joint holds on it: a new joint from a free screw to the carrier
/// places the earlier joint again at the value it is at, not at its start
/// value, so the carrier and the cover stay where the Move put them.
#[test]
fn an_earlier_joint_placed_again_starts_where_it_is_not_at_its_start_values() {
    let mut doc = Document::new(MockKernel::default());
    let base = link(&mut doc, "Base", Transform::IDENTITY);
    doc.set_occurrence_grounded(base, true).unwrap();
    let carrier = link(&mut doc, "Carrier", Transform::IDENTITY);
    let cover = link(&mut doc, "Cover", shift(COVER_AT));
    let screw = link(&mut doc, "Screw", shift(SCREW_FROM));
    add_joint(
        &mut doc,
        json!({"kind": "revolute", "a": pin(carrier, [0.0; 3]), "b": pin(base, [0.0; 3]),
               "start": {"rz": 0}}),
    );
    add_joint(
        &mut doc,
        earlier_joint(("rigid", None), cover, carrier, COVER_AT),
    );
    move_occurrences(&mut doc, &[carrier, cover], about_z(0.5));
    assert_same(
        &doc.placement(carrier).unwrap(),
        &turn(0.5),
        "the carrier turned",
    );
    let carrier_now = assert_cover_kept(
        &mut doc,
        cover,
        carrier,
        screw_joint(screw, carrier),
        "on a pivot",
    );
    assert_same(&carrier_now, &turn(0.5), "the carrier stays turned");
    assert_same(
        &doc.placement(screw).unwrap(),
        &turn(0.5).after(&shift(SCREW_AT)),
        "the screw",
    );
}
