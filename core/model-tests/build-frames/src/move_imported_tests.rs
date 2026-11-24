// SPDX-License-Identifier: MIT
//! Moving what a STEP import brought into a BUILD frame (mitcad#1647,
//! GitHub mitcad/Mitcad#233), with the mock kernel. The report: a STEP
//! file imported into the active component of a frame built with BUILD
//! (profiles placed on each other by joints), then moved with Move/Copy;
//! a profile of the frame moved too, and Undo and Redo moved both again.
//!
//! The frame here is that arrangement in small: a component of its own,
//! active, holding a grounded profile, a cross profile on its side and an
//! upright on the cross profile's end. Into it:
//! - a single-part file, wrapped in a component named after the file
//!   (mitcad#1629), moved by a Free Move that turns it a quarter about X,
//!   as the report's Move did: only the import's bodies move, each by
//!   exactly that motion, the profiles stay (within 1e-6 mm), through
//!   Undo, Redo and a recompute from the start;
//! - a STEP assembly whose sub-assembly (a caster: a wheel placed twice
//!   and a fork) is placed twice, so that both placements share its
//!   definition: the wrapper moved moves the whole import and nothing of
//!   the frame; one caster moved moves that caster alone, not the other
//!   placement of the same definition; a wheel moved inside the shared
//!   definition moves that wheel in both casters, as a shared definition
//!   is, and nothing else;
//! - the same file imported twice: each import has definitions of its
//!   own, so a move inside one import's caster leaves the other import
//!   where it is.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::exchange::{ImportAssembly, ImportBody, ImportInstance, ImportPart};
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::transform::Transform;
use mitcad_model_tests_support::test_library::*;

/// A profile of the design's source placed on a slot of `host`'s side,
/// `offset` from its end A.
fn on_side(
    doc: &mut Document<MockKernel>,
    host: &Value,
    slot: &str,
    offset: f64,
    length: f64,
) -> Value {
    let hp = derived(doc, component(host));
    let placed = place(
        doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": host["occurrence"],
                      "geometry": {"mount": format!("{hp}:mount({slot})")},
                      "offset": offset},
               "length": length}),
    );
    all_ok(doc);
    placed
}

/// A profile of the design's source placed on `host`'s end B, along
/// `direction` (a corner).
fn on_end(doc: &mut Document<MockKernel>, host: &Value, direction: [f64; 3], length: f64) -> Value {
    let hp = derived(doc, component(host));
    let placed = place(
        doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": host["occurrence"],
                      "geometry": {"mount": format!("{hp}:mount(end_b)")},
                      "direction": direction},
               "length": length}),
    );
    all_ok(doc);
    placed
}

/// The frame of the report in small: a component "Base Frame", active,
/// with a grounded profile, a cross profile on its side (a joint with an
/// offset) and an upright on the cross profile's end. Returns the frame's
/// occurrence and the three profiles' answers.
fn frame(doc: &mut Document<MockKernel>) -> (OccurrenceUid, Vec<Value>) {
    let (_, frame) = doc
        .create_component(Some("Base Frame"), Transform::IDENTITY, true)
        .unwrap();
    let rail = first(doc, "p30", [0.0; 3], 1000.0);
    let cross = on_side(doc, &rail, "slot_x+_1", 500.0, 460.0);
    let upright = on_end(doc, &cross, [0.0, 0.0, 1.0], 400.0);
    (frame, vec![rail, cross, upright])
}

fn body(name: &str, faces: usize) -> ImportBody<MockShape> {
    ImportBody {
        name: Some(name.to_owned()),
        color: None,
        shape: MockShape::imported(&name.to_lowercase(), faces),
        face_colors: Vec::new(),
    }
}

fn at(part: usize, offset: [f64; 3]) -> ImportInstance {
    ImportInstance {
        part,
        name: None,
        transform: Transform::translation(offset),
    }
}

/// A STEP assembly of a caster placed twice and a plate once: the caster
/// is a sub-assembly of a wheel placed twice and a fork.
fn casters() -> ImportAssembly<MockShape> {
    ImportAssembly {
        parts: vec![
            ImportPart {
                name: Some("Wheel".to_owned()),
                bodies: vec![body("Wheel", 3)],
                instances: Vec::new(),
            },
            ImportPart {
                name: Some("Fork".to_owned()),
                bodies: vec![body("Fork", 6)],
                instances: Vec::new(),
            },
            ImportPart {
                name: Some("Caster".to_owned()),
                bodies: Vec::new(),
                instances: vec![
                    at(0, [0.0, -20.0, 0.0]),
                    at(0, [0.0, 20.0, 0.0]),
                    at(1, [0.0, 0.0, 30.0]),
                ],
            },
            ImportPart {
                name: Some("Plate".to_owned()),
                bodies: vec![body("Plate", 6)],
                instances: Vec::new(),
            },
        ],
        roots: vec![
            at(2, [0.0, 0.0, 0.0]),
            at(2, [300.0, 0.0, 0.0]),
            at(3, [150.0, 0.0, 60.0]),
        ],
    }
}

/// Placed bodies' transforms into the design, by path and body.
type Placed = BTreeMap<(Vec<OccurrenceUid>, BodyUid), Transform>;

/// Every placed body where the design shows it, by its path and body.
fn world(doc: &Document<MockKernel>) -> Placed {
    doc.instances()
        .into_iter()
        .map(|i| ((i.path, i.body), i.transform))
        .collect()
}

fn same(a: &Transform, b: &Transform) -> bool {
    close(a.translation, b.translation) && (0..3).all(|r| close(a.linear[r], b.linear[r]))
}

/// Checks that from `before` to `after` exactly the placed bodies whose
/// path passes through `moved` changed, each by `by` (a motion in the
/// design's coordinates), and every other placed body stayed. Returns how
/// many moved.
fn only_moved(
    before: &Placed,
    after: &Placed,
    moved: &[OccurrenceUid],
    by: &Transform,
    what: &str,
) -> usize {
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "{what}: the placed bodies changed"
    );
    let mut count = 0;
    for (key, was) in before {
        let now = &after[key];
        let (path, body) = key;
        if path.iter().any(|o| moved.contains(o)) {
            let wanted = by.after(was);
            assert!(
                same(now, &wanted),
                "{what}: {body} in {path:?} is at {now:?}, not moved by {by:?} from {was:?}"
            );
            count += 1;
        } else {
            assert!(
                same(now, was),
                "{what}: {body} in {path:?} moved: {now:?}, was {was:?}"
            );
        }
    }
    count
}

/// A quarter turn about X with a shift: the report's Free Move of the
/// imported part.
fn quarter_about_x() -> (Value, Transform) {
    let matrix = json!({"type": "free",
                        "matrix": [[1.0, 0.0, 0.0, 0.0],
                                   [0.0, 0.0, -1.0, -12.0],
                                   [0.0, 1.0, 0.0, -183.5]]});
    let motion = Transform {
        linear: [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        translation: [0.0, -12.0, -183.5],
    };
    (matrix, motion)
}

fn uid_of(value: &Value) -> OccurrenceUid {
    value.as_str().unwrap().parse().unwrap()
}

/// The component an import made under `name`, with its occurrences.
fn made(result: &Value, name: &str) -> (ComponentUid, Vec<OccurrenceUid>) {
    let made = result["components"]
        .as_array()
        .unwrap_or_else(|| panic!("no components: {result}"))
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no component {name}: {result}"));
    (
        made["uid"].as_str().unwrap().parse().unwrap(),
        made["occurrences"]
            .as_array()
            .unwrap()
            .iter()
            .map(uid_of)
            .collect(),
    )
}

/// Adds a Move of `occurrences` in `component` (the active one when None).
fn move_occurrences(
    doc: &mut Document<MockKernel>,
    occurrences: &[OccurrenceUid],
    transform: Value,
    component: Option<ComponentUid>,
) {
    let mut add = json!({"cmd": "add_feature",
                         "def": {"type": "move_occurrence",
                                 "occurrences": occurrences
                                     .iter()
                                     .map(ToString::to_string)
                                     .collect::<Vec<_>>(),
                                 "transform": transform}});
    if let Some(component) = component {
        add["component"] = json!(component.to_string());
    }
    command(doc, add).unwrap_or_else(|e| panic!("the move: {e}"));
    all_ok(doc);
}

#[test]
fn moving_an_imported_part_in_a_build_frame_moves_it_alone() {
    let mut doc = design();
    let (frame_occurrence, profiles) = frame(&mut doc);
    let path = "/models/caster.step";
    doc.kernel().files.borrow_mut().insert(
        path.to_owned(),
        vec![body("Plate", 6), body("Stem", 3), body("Wheel", 3)],
    );
    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    all_ok(&doc);
    // Wrapped in a component of its own, placed in the active frame.
    let wrapper = uid_of(&result["component"]["occurrence"]);
    let frame_component = doc
        .assembly()
        .occurrence(frame_occurrence)
        .unwrap()
        .component;
    assert_eq!(
        doc.assembly().occurrence(wrapper).unwrap().parent,
        frame_component,
        "{result}"
    );
    let profiles_at: Vec<Transform> = profiles
        .iter()
        .map(|p| placed(&doc, occurrence(p)))
        .collect();
    let before = world(&doc);

    // The Move the report made: the import's occurrence turned a quarter
    // about X in the frame's coordinates (the frame at the identity).
    let (matrix, motion) = quarter_about_x();
    move_occurrences(&mut doc, &[wrapper], matrix, None);
    let after = world(&doc);
    assert_eq!(
        only_moved(&before, &after, &[wrapper], &motion, "the Move"),
        3
    );
    for (p, was) in profiles.iter().zip(&profiles_at) {
        assert!(same(&placed(&doc, occurrence(p)), was), "{p} moved");
    }
    // Undo puts the import back alone; Redo moves it alone again, twice.
    for _ in 0..2 {
        command(&mut doc, json!({"cmd": "undo"})).unwrap();
        all_ok(&doc);
        only_moved(&before, &world(&doc), &[], &Transform::IDENTITY, "Undo");
        command(&mut doc, json!({"cmd": "redo"})).unwrap();
        all_ok(&doc);
        only_moved(&before, &world(&doc), &[wrapper], &motion, "Redo");
    }
    // A recompute from the start places everything as before.
    doc.recompute();
    all_ok(&doc);
    only_moved(&before, &world(&doc), &[wrapper], &motion, "the recompute");
}

#[test]
fn moving_one_of_an_imported_assemblys_occurrences_moves_it_alone() {
    let mut doc = design();
    let (_, profiles) = frame(&mut doc);
    let path = "/models/casters.step";
    doc.kernel()
        .assemblies
        .borrow_mut()
        .insert(path.to_owned(), casters());
    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    all_ok(&doc);
    assert_eq!(result["structure"], "components", "{result}");
    let wrapper = uid_of(&result["component"]["occurrence"]);
    let wrapper_component: ComponentUid = result["component"]["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let (caster, caster_placed) = made(&result, "Caster");
    let (_, wheels) = made(&result, "Wheel");
    // The caster placed twice shares one definition, its wheels too.
    assert_eq!(caster_placed.len(), 2, "{result}");
    assert_eq!(wheels.len(), 2, "{result}");
    let profiles_at: Vec<Transform> = profiles
        .iter()
        .map(|p| placed(&doc, occurrence(p)))
        .collect();
    let before = world(&doc);
    // Two casters of a fork and two wheels each, and the plate.
    assert_eq!(
        before.keys().filter(|(p, _)| p.contains(&wrapper)).count(),
        7
    );

    // The whole import moved in the frame: nothing of the frame moves.
    let (matrix, motion) = quarter_about_x();
    move_occurrences(&mut doc, &[wrapper], matrix, None);
    assert_eq!(
        only_moved(
            &before,
            &world(&doc),
            &[wrapper],
            &motion,
            "the import moved"
        ),
        7
    );
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    all_ok(&doc);
    only_moved(&before, &world(&doc), &[], &Transform::IDENTITY, "Undo");

    // One caster moved in the import's component (at the identity in the
    // frame, which is at the identity): the other placement of the same
    // definition stays, and so does the frame.
    let along = Transform::translation([0.0, 250.0, 0.0]);
    move_occurrences(
        &mut doc,
        &[caster_placed[0]],
        json!({"type": "translate_xyz", "x": 0, "y": 250, "z": 0}),
        Some(wrapper_component),
    );
    assert_eq!(
        only_moved(
            &before,
            &world(&doc),
            &[caster_placed[0]],
            &along,
            "one caster moved"
        ),
        3
    );
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    all_ok(&doc);

    // A wheel moved inside the caster's definition moves that wheel in
    // both casters (one definition), nothing else.
    let up = Transform::translation([0.0, 0.0, 5.0]);
    move_occurrences(
        &mut doc,
        &[wheels[0]],
        json!({"type": "translate_xyz", "x": 0, "y": 0, "z": 5}),
        Some(caster),
    );
    assert_eq!(
        only_moved(
            &before,
            &world(&doc),
            &[wheels[0]],
            &up,
            "a wheel in the definition"
        ),
        2
    );
    for (p, was) in profiles.iter().zip(&profiles_at) {
        assert!(same(&placed(&doc, occurrence(p)), was), "{p} moved");
    }
}

#[test]
fn a_file_imported_twice_keeps_definitions_of_its_own() {
    let mut doc = design();
    let (_, profiles) = frame(&mut doc);
    let path = "/models/casters.step";
    doc.kernel()
        .assemblies
        .borrow_mut()
        .insert(path.to_owned(), casters());
    let first_import = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    let second_import = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    all_ok(&doc);
    assert_eq!(
        second_import["component"]["name"], "casters (1)",
        "{second_import}"
    );
    // The second import's components are new ones, named apart.
    let (caster, _) = made(&first_import, "Caster");
    let (other_caster, _) = made(&second_import, "Caster (1)");
    assert_ne!(
        caster, other_caster,
        "the second import shares the first's caster"
    );
    let (_, wheels) = made(&first_import, "Wheel");
    let (_, other_wheels) = made(&second_import, "Wheel (1)");
    assert!(
        wheels.iter().all(|w| !other_wheels.contains(w)),
        "{wheels:?} {other_wheels:?}"
    );
    let profiles_at: Vec<Transform> = profiles
        .iter()
        .map(|p| placed(&doc, occurrence(p)))
        .collect();
    let before = world(&doc);
    // A wheel moved in the first import's caster: the second import's
    // casters stay where they are, and so does the frame.
    let up = Transform::translation([0.0, 0.0, 5.0]);
    move_occurrences(
        &mut doc,
        &[wheels[0]],
        json!({"type": "translate_xyz", "x": 0, "y": 0, "z": 5}),
        Some(caster),
    );
    assert_eq!(
        only_moved(
            &before,
            &world(&doc),
            &[wheels[0]],
            &up,
            "the first import's wheel"
        ),
        2
    );
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    command(&mut doc, json!({"cmd": "redo"})).unwrap();
    all_ok(&doc);
    only_moved(&before, &world(&doc), &[wheels[0]], &up, "Undo and Redo");
    for (p, was) in profiles.iter().zip(&profiles_at) {
        assert!(same(&placed(&doc, occurrence(p)), was), "{p} moved");
    }
}
