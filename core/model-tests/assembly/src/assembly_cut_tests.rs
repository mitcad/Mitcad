// SPDX-License-Identifier: MIT
//! Assembly cuts (mitcad#2131) with the mock kernel: an Extrude Cut made in
//! the root machines occurrences of a linked component and of a component
//! placed twice, in this assembly only. The linked design's file and the
//! shared definition stay as they are; the occurrence reached shows pieces
//! of its own (`instances`), the others the definition; the pieces keep
//! where the cut lay on the part when a joint turns it; Update Link applies
//! the cut again to the new geometry, or warns when it no longer reaches
//! it; save and reopen keep it all.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::features::{FeatureDef, SketchPlane, ValueInput};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::{Document, FeatureStatus, FileFormat, InsertOptions, InstanceView, Transform};
use mitcad_model_api::DocumentApi;

/// The mock cuts only what the tool overlaps by its bounds.
fn kernel() -> MockKernel {
    let kernel = MockKernel::default();
    kernel.cuts_by_bounds.set(true);
    kernel
}

/// A block of 10 × 10 × 5 at x = `x` in the active component.
fn block(doc: &mut Document<MockKernel>, x: f64) -> BodyUid {
    let uid = doc
        .add_feature(
            &def(json!({"type":"box", "plane":"xy", "corner":[x, 0],
                "length":10, "width":10, "height":5, "operation":"new_body"})),
            None,
        )
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

/// A cut of a 4 × 4 square at (3, 3) in the root's XY plane, 10 mm up
/// through the blocks there, that includes `targets` by name.
fn cut(doc: &mut Document<MockKernel>, targets: &[(ComponentUid, OccurrenceUid)]) -> Value {
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [3.0, 3.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let included: Vec<Value> = targets
        .iter()
        .map(|(c, o)| json!({"component": c, "occurrence": o.to_string()}))
        .collect();
    json!({"type":"extrude",
        "profiles":[{"sketch":sketch, "region":region}],
        "extent":{"type":"distance","distance":10}, "operation":"cut",
        "cut_scope":{"context":{"component":"C0","occurrence":""},
            "targets":{"type":"explicit","included":included}}})
}

fn cut_definition(value: &Value) -> FeatureDef<ValueInput> {
    def(value.clone())
}

fn is_cut(shape: &MockShape) -> bool {
    shape.history.starts_with("cut(")
}

/// What the occurrence at `path` shows.
fn shown(doc: &Document<MockKernel>, path: &[OccurrenceUid]) -> Vec<InstanceView> {
    doc.instances()
        .into_iter()
        .filter(|i| i.path == path)
        .collect()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// The design file at `path` opened as the application opens it.
fn open(path: &Path) -> Document<MockKernel> {
    let text = std::fs::read_to_string(path).unwrap();
    let mut doc = Document::from_json_at(&text, path, kernel()).unwrap();
    doc.update_links(path.parent()).unwrap();
    doc.recompute();
    doc
}

/// A project with a part (one block at x = 0) linked into a holder at its
/// origin, and the holder's cut machining the linked occurrence; the
/// holder saved.
struct Linked {
    dir: PathBuf,
    holder: PathBuf,
    part: PathBuf,
    doc: Document<MockKernel>,
    component: ComponentUid,
    occurrence: OccurrenceUid,
    cut: FeatureUid,
}

fn save_part(path: &Path, x: f64) {
    let mut part = Document::new(kernel());
    block(&mut part, x);
    part.save_file(path, FileFormat::Project).unwrap();
}

fn linked(name: &str) -> Linked {
    let dir = mitcad_model::testing::unique_temp_path(name);
    let project = dir.join("project");
    mitcad_model::Project::init(&project).unwrap();
    let designs = project.join("designs");
    let parts = project.join("parts");
    std::fs::create_dir_all(&designs).unwrap();
    std::fs::create_dir_all(&parts).unwrap();
    let part = parts.join("plate.mitcad");
    save_part(&part, 0.0);
    let mut doc = Document::new(kernel());
    let options = InsertOptions {
        base: Some(designs.clone()),
        ..InsertOptions::default()
    };
    let (component, occurrence) = doc
        .insert_component(part.to_str().unwrap(), &options)
        .unwrap();
    let value = cut(&mut doc, &[(component, occurrence)]);
    let cut = doc.add_feature(&cut_definition(&value), None).unwrap().uid;
    let holder = designs.join("holder.mitcad");
    doc.save_file(&holder, FileFormat::Project).unwrap();
    Linked {
        dir,
        holder,
        part,
        doc,
        component,
        occurrence,
        cut,
    }
}

/// The occurrence is machined by the cut alone, its one piece cut, and the
/// definition's body as it was.
fn assert_machined(
    doc: &Document<MockKernel>,
    component: ComponentUid,
    o: OccurrenceUid,
    cut: FeatureUid,
) {
    assert_eq!(doc.status(cut), Some(&FeatureStatus::Ok));
    let machined = doc.machined_occurrences().at(&[o]).expect("machined");
    assert_eq!(machined.component, component);
    assert_eq!(machined.cuts, vec![cut]);
    let definition = doc.component_bodies(component);
    assert_eq!(definition.len(), 1);
    assert!(
        !is_cut(doc.body_shape(definition[0]).unwrap()),
        "the definition stays as it is"
    );
    let shown = shown(doc, &[o]);
    assert_eq!(shown.len(), 1);
    let piece = shown[0].body;
    assert_ne!(piece, definition[0]);
    assert_eq!(piece.feature, cut, "a piece is the cut's");
    assert_eq!(shown[0].component, component);
    assert!(is_cut(doc.body_shape(piece).unwrap()));
    assert_eq!(doc.body_component(piece), Some(component));
    assert_eq!(
        doc.machined_occurrences().replaced_by(piece),
        Some(definition[0])
    );
}

#[test]
fn a_cut_in_an_assembly_machines_a_linked_part_and_leaves_its_file() {
    let before = {
        let dir = mitcad_model::testing::unique_temp_path("2131-file");
        let project = dir.join("project");
        mitcad_model::Project::init(&project).unwrap();
        let path = project.join("plate.mitcad");
        save_part(&path, 0.0);
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        text
    };
    let l = linked("2131-linked");
    assert_machined(&l.doc, l.component, l.occurrence, l.cut);
    // The linked design's file is as it was written: nothing of the cut
    // went into it.
    assert_eq!(std::fs::read_to_string(&l.part).unwrap(), before);
    let reopened_part = open(&l.part);
    assert!(reopened_part.machined_occurrences().is_empty());
    for body in reopened_part.bodies() {
        assert!(!is_cut(body.shape), "the part opens without the cut");
    }
    // The queries say which occurrence is machined, and by what.
    let components = query(&l.doc, json!({"query": "components"}));
    let row = &components["occurrences"][0];
    assert_eq!(row["machined"]["cuts"], json!([l.cut]), "{components}");
    assert!(
        row["machined"]["note"]
            .as_str()
            .unwrap()
            .contains("Machined in this assembly"),
        "{components}"
    );
    let instances = query(&l.doc, json!({"query": "instances"}));
    let definition = l.doc.component_bodies(l.component)[0];
    assert_eq!(
        instances[0]["machined_from"],
        json!(definition),
        "{instances}"
    );
    // Saved and opened again: the cut machines the occurrence as before.
    let reopened = open(&l.holder);
    assert_machined(&reopened, l.component, l.occurrence, l.cut);
    let _ = std::fs::remove_dir_all(&l.dir);
}

#[test]
fn a_part_placed_twice_and_reached_once_is_machined_there_only() {
    let mut doc = Document::new(kernel());
    let (plate, first) = doc
        .create_component(Some("Plate"), Transform::IDENTITY, true)
        .unwrap();
    let body = block(&mut doc, 0.0);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let second = doc
        .copy_occurrence(first, Some(Transform::translation([100.0, 0.0, 0.0])))
        .unwrap();

    // An automatic scope leaves the shared definition out, and says that
    // checking it machines that occurrence only.
    let mut automatic = cut(&mut doc, &[]);
    automatic["cut_scope"]["targets"] = json!({"type": "automatic"});
    let report = doc.preview_add(&cut_definition(&automatic), None).unwrap();
    let targets = report.cut_targets.unwrap();
    let row = targets.rows.iter().find(|r| r.component == plate).unwrap();
    assert_eq!(row.occurrence.0, vec![first]);
    assert!(!row.eligible && !row.checked && row.assembly_cut);
    assert!(
        row.reason
            .as_deref()
            .unwrap()
            .contains("Placed more than once")
    );
    assert!(
        targets.rows.iter().all(|r| r.occurrence.0 != vec![second]),
        "the occurrence the cut does not reach has no row"
    );

    // Included by name: machined at the first occurrence only.
    let value = cut(&mut doc, &[(plate, first)]);
    let uid = doc.add_feature(&cut_definition(&value), None).unwrap().uid;
    assert_machined(&doc, plate, first, uid);
    let other = shown(&doc, &[second]);
    assert_eq!(other.len(), 1);
    assert_eq!(
        other[0].body, body,
        "the second occurrence shows the definition"
    );
    assert!(!is_cut(doc.body_shape(body).unwrap()));
    assert!(doc.machined_occurrences().at(&[second]).is_none());
    // The pieces are no component's own bodies: the definition lists its
    // body alone, and the Bill of Materials one row of two.
    assert_eq!(doc.component_bodies(plate), vec![body]);
    let rows: Vec<_> = doc
        .bom_rows(false)
        .into_iter()
        .filter(|r| r.part.component == plate)
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].part.quantity, 2);
    // The frame it was made in is kept with the scope.
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let frames = saved["features"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|f| f["cut_scope"]["frames"].as_array().cloned())
        .expect("frames kept");
    assert_eq!(frames.len(), 1, "{frames:?}");
    assert_eq!(frames[0]["occurrence"], json!(first.to_string()));

    // Saved and opened again, all of it holds.
    let mut loaded = Document::from_json(&doc.to_json(), kernel()).unwrap();
    loaded.recompute();
    assert_machined(&loaded, plate, first, uid);
    assert_eq!(shown(&loaded, &[second])[0].body, body);
    assert_eq!(loaded.to_json(), doc.to_json());
    // Undone, the occurrence is the definition again.
    doc.undo();
    assert!(doc.machined_occurrences().is_empty());
    assert_eq!(shown(&doc, &[first])[0].body, body);
}

#[test]
fn the_removed_material_moves_with_its_part_through_a_joint_drive() {
    let mut doc = Document::new(kernel());
    let (lever, first) = doc
        .create_component(Some("Lever"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut doc, 0.0);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.copy_occurrence(first, Some(Transform::translation([100.0, 0.0, 0.0])))
        .unwrap();
    // The lever turns about the root's origin.
    let joint = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "revolute",
               "a": {"occurrence": "Lever:1", "geometry": {"point": [0.0, 0.0, 0.0]}},
               "b": {"geometry": {"point": [0.0, 0.0, 0.0]}}}),
    );
    assert_eq!(joint["error"], Value::Null, "{joint}");
    let value = cut(&mut doc, &[(lever, first)]);
    let uid = doc.add_feature(&cut_definition(&value), None).unwrap().uid;
    assert_machined(&doc, lever, first, uid);
    let piece = shown(&doc, &[first])[0].body;
    let made = doc.body_shape(piece).unwrap().clone();
    let placed = doc.placement(first).unwrap();

    // Turned by a quarter: the occurrence moves, its piece with it, cut
    // where it was on the lever (the cut keeps its frame on the part).
    command(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"rz": "90 deg"}}),
    );
    assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
    let turned = doc.placement(first).unwrap();
    assert_ne!(turned, placed, "the joint turned the lever");
    let now = shown(&doc, &[first]);
    assert_eq!(now.len(), 1);
    assert_eq!(now[0].body, piece, "the same piece");
    assert_eq!(now[0].transform, turned, "where the lever is");
    let shape = doc.body_shape(piece).unwrap();
    assert_eq!(shape.history, made.history);
    assert_eq!(shape.bounds, made.bounds, "cut where it was on the lever");
}

#[test]
fn update_link_applies_the_cut_again_or_warns_when_it_no_longer_reaches() {
    let l = linked("2131-update");
    let mut doc = open(&l.holder);
    assert_machined(&doc, l.component, l.occurrence, l.cut);
    let base = l.holder.parent().unwrap().to_string_lossy().to_string();

    // The part's block moves 1 mm: the cut still reaches it and machines
    // the new geometry.
    save_part(&l.part, 1.0);
    command(
        &mut doc,
        json!({"cmd": "update_link", "component": l.component.to_string(), "base": base}),
    );
    assert_machined(&doc, l.component, l.occurrence, l.cut);
    assert!(doc.warnings(l.cut).is_empty(), "{:?}", doc.warnings(l.cut));
    let definition = doc.component_bodies(l.component)[0];
    let piece = shown(&doc, &[l.occurrence])[0].body;
    // The piece is cut from the definition's new body (the mock's history
    // names what it cut).
    let machined = &doc.body_shape(piece).unwrap().history;
    let source = &doc.body_shape(definition).unwrap().history;
    assert!(machined.contains(source.as_str()), "{machined} / {source}");
    assert_eq!(
        doc.machined_occurrences().replaced_by(piece),
        Some(definition)
    );

    // The block moves out of the cut's way: the part is left as it is,
    // with a warning in words on the cut, which still succeeds. (The mock
    // keeps no bounds of the linked bodies it reads back, so it is told
    // that the cut touches nothing.)
    *doc.kernel().touch.borrow_mut() = Some(vec![false]);
    save_part(&l.part, 100.0);
    command(
        &mut doc,
        json!({"cmd": "update_link", "component": l.component.to_string(), "base": base}),
    );
    assert_eq!(doc.status(l.cut), Some(&FeatureStatus::Ok));
    assert!(
        doc.warnings(l.cut)
            .iter()
            .any(|w| w.contains("no longer reaches a component it machines")),
        "{:?}",
        doc.warnings(l.cut)
    );
    assert!(doc.machined_occurrences().is_empty());
    let shown_now = shown(&doc, &[l.occurrence]);
    assert_eq!(shown_now.len(), 1);
    assert_eq!(shown_now[0].body, doc.component_bodies(l.component)[0]);
    assert!(!is_cut(doc.body_shape(shown_now[0].body).unwrap()));
    let _ = std::fs::remove_dir_all(&l.dir);
}

/// A component with a block (below the root's XY plane) placed twice, the
/// second far away; its component and its two occurrences.
fn plate_twice(doc: &mut Document<MockKernel>) -> (ComponentUid, OccurrenceUid, OccurrenceUid) {
    let (plate, first) = doc
        .create_component(
            Some("Plate"),
            Transform::translation([0.0, 0.0, -5.0]),
            true,
        )
        .unwrap();
    block(doc, 0.0);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let second = doc
        .copy_occurrence(first, Some(Transform::translation([100.0, 0.0, -5.0])))
        .unwrap();
    (plate, first, second)
}

#[test]
fn a_hole_and_a_revolve_cut_made_in_the_assembly_machine_one_occurrence() {
    let mut doc = Document::new(kernel());
    let (plate, first, second) = plate_twice(&mut doc);
    let scope = json!({"context": {"component": "C0", "occurrence": ""},
        "targets": {"type": "explicit",
                    "included": [{"component": plate, "occurrence": first.to_string()}]}});
    // A hole from the root's XY plane down through the first plate.
    let hole = json!({"type": "hole",
        "placement": {"type": "plane", "plane": "xy", "points": [[5, 5, 0]]},
        "diameter": 3, "extent": {"type": "through_all"}, "cut_scope": scope});
    // A modelled thread is refused with a scope, in words.
    let mut threaded = hole.clone();
    threaded["thread"] = json!({"designation": "M3x0.5", "modeled": true});
    let refused = doc.add_feature(&def(threaded), None);
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| e.to_string().contains("modelled or taper thread")),
        "{:?}",
        refused.map(|added| added.uid)
    );
    let uid = doc.add_feature(&def(hole), None).unwrap().uid;
    assert_machined(&doc, plate, first, uid);
    assert!(doc.machined_occurrences().at(&[second]).is_none());
    doc.undo();
    assert!(doc.machined_occurrences().is_empty());

    // A Revolve Cut turning a square round the y axis, likewise.
    let value = cut(&mut doc, &[(plate, first)]);
    let revolve = def(
        json!({"type": "revolve", "profiles": value["profiles"].clone(),
        "axis": "y", "operation": "cut", "extent": {"type": "full"},
        "cut_scope": value["cut_scope"].clone()}),
    );
    let uid = doc.add_feature(&revolve, None).unwrap().uid;
    assert_machined(&doc, plate, first, uid);
    assert!(doc.machined_occurrences().at(&[second]).is_none());
}
