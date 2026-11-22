// SPDX-License-Identifier: MIT
//! Linked parts of a design (mitcad#2130) with the mock kernel, whose
//! shapes record where they were moved (`moved(…,x,y,z)` in a linked
//! body's data): `insert_component` with `parts` holds only the chosen
//! occurrences' and root bodies' bodies, placed as the design places them;
//! a component placed twice comes in once per chosen occurrence; an
//! occurrence chosen inside an unchosen parent comes in alone at the
//! parent's placement; reading the file again takes a chosen part's change
//! and ignores an unchosen one, and a part gone keeps its saved bodies
//! with a message in words; the design file keeps the parts, and a file
//! without them links the whole design; `set_link_parts` changes the
//! choice in one undo step, and a joint on a part no longer chosen keeps
//! its place with a warning in words. Through the application:
//! `tools/ui-link-parts-test.sh` and `app.ui-windows-link-parts`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|text| serde_json::from_str(&text).unwrap())
        .map_err(|e| e.to_string())
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn shift(x: f64, y: f64, z: f64) -> Transform {
    Transform::translation([x, y, z])
}

/// A block of 10 × 10 × 5 at x = `x` in the active component.
fn block(doc: &mut Document<MockKernel>, x: f64) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [x, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

/// The design to link and its parts.
struct Source {
    root_body: BodyUid,
    /// Bracket:1 at (100, 0, 0) and Bracket:2 at (300, 0, 0), one
    /// component placed twice.
    bracket1: OccurrenceUid,
    bracket2: OccurrenceUid,
    /// Board:1 at (0, 200, 0), with Chip:1 at (0, 0, 10) in it.
    board: OccurrenceUid,
    chip: OccurrenceUid,
}

/// A root body, a bracket placed twice (`bracket_blocks` blocks each), a
/// board (`board_blocks` blocks) with a chip in it unless `chip` is false;
/// the same uids in every variant.
fn source(
    bracket_blocks: usize,
    board_blocks: usize,
    chip: bool,
) -> (Document<MockKernel>, Source) {
    let mut doc = Document::new(MockKernel::default());
    let root_body = block(&mut doc, 0.0);
    let (_, bracket1) = doc
        .create_component(Some("Bracket"), shift(100.0, 0.0, 0.0), true)
        .unwrap();
    for i in 0..bracket_blocks {
        block(&mut doc, 20.0 * i as f64);
    }
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let bracket2 = doc
        .copy_occurrence(bracket1, Some(shift(300.0, 0.0, 0.0)))
        .unwrap();
    let (_, board) = doc
        .create_component(Some("Board"), shift(0.0, 200.0, 0.0), true)
        .unwrap();
    for i in 0..board_blocks {
        block(&mut doc, 20.0 * i as f64);
    }
    let chip = if chip {
        let (_, chip) = doc
            .create_component(Some("Chip"), shift(0.0, 0.0, 10.0), true)
            .unwrap();
        block(&mut doc, 0.0);
        chip
    } else {
        OccurrenceUid(u32::MAX)
    };
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let ids = Source {
        root_body,
        bracket1,
        bracket2,
        board,
        chip,
    };
    (doc, ids)
}

/// The design written to `assembly.mitcad` in a folder of its own.
fn saved_source(name: &str) -> (PathBuf, Source) {
    let dir = mitcad_model::testing::unique_temp_dir(name);
    let (doc, ids) = source(1, 1, true);
    std::fs::write(dir.join("assembly.mitcad"), doc.to_json()).unwrap();
    (dir, ids)
}

/// The design file changed to `doc`.
fn write_source(dir: &Path, doc: &Document<MockKernel>) {
    std::fs::write(dir.join("assembly.mitcad"), doc.to_json()).unwrap();
}

fn base(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}

/// Inserts the design linked with `parts` (null: the whole design); the
/// reply.
fn insert(doc: &mut Document<MockKernel>, dir: &Path, parts: Value) -> Value {
    let mut cmd = json!({"cmd": "insert_component", "path": "assembly.mitcad",
                         "base": base(dir)});
    // The first one named Linked; later ones take the file's name, made
    // unique.
    let taken = doc.assembly().components.iter().any(|c| c.name == "Linked");
    if !taken {
        cmd["name"] = json!("Linked");
    }
    if !parts.is_null() {
        cmd["parts"] = parts;
    }
    command(doc, cmd).unwrap()
}

fn component(reply: &Value) -> ComponentUid {
    reply["component"].as_str().unwrap().parse().unwrap()
}

/// The data of the linked component's bodies: where the design placed
/// them (`…,x,y,z)` at the end, nothing for the identity).
fn placed(doc: &Document<MockKernel>, component: ComponentUid) -> Vec<String> {
    let link = doc
        .assembly()
        .component(component)
        .and_then(|c| c.link.clone())
        .unwrap();
    let FeatureDef::Base(base) = &doc.feature(link.feature).unwrap().def else {
        panic!("a linked component's bodies are a base feature");
    };
    base.bodies
        .iter()
        .map(|b| String::from_utf8_lossy(b.brep.data().unwrap()).into_owned())
        .collect()
}

fn ends(data: &[String], at: &str) -> usize {
    data.iter().filter(|d| d.ends_with(at)).count()
}

fn link_parts(doc: &Document<MockKernel>, component: ComponentUid) -> Option<Vec<String>> {
    let link = doc
        .assembly()
        .component(component)
        .and_then(|c| c.link.clone())
        .unwrap();
    link.parts.map(|parts| {
        parts
            .iter()
            .map(|p| match p.body {
                Some(body) => body.to_string(),
                None => p
                    .occurrence
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("/"),
            })
            .collect()
    })
}

#[test]
fn an_insert_with_parts_holds_only_their_bodies() {
    let (dir, ids) = saved_source("2130-insert");
    let mut doc = Document::new(MockKernel::default());
    let reply = insert(
        &mut doc,
        &dir,
        json!({"occurrences": [ids.bracket1.to_string()]}),
    );
    let linked = component(&reply);
    assert_eq!(doc.component_bodies(linked).len(), 1, "{reply}");
    let data = placed(&doc, linked);
    assert_eq!(ends(&data, ",100,0,0)"), 1, "{data:?}");
    assert_eq!(
        reply["parts"]["occurrences"],
        json!([ids.bracket1.to_string()]),
        "{reply}"
    );
    assert_eq!(reply["parts"]["names"], json!(["Bracket:1"]), "{reply}");

    // A body of the root alone: where the root has it.
    let reply = insert(
        &mut doc,
        &dir,
        json!({"bodies": [ids.root_body.to_string()]}),
    );
    let root_only = component(&reply);
    let data = placed(&doc, root_only);
    assert_eq!(data.len(), 1, "{data:?}");
    assert!(!data[0].contains("moved("), "{data:?}");

    // What the design does not have is refused, and so is a copy with
    // chosen parts.
    let refused = command(
        &mut doc,
        json!({"cmd": "insert_component", "path": "assembly.mitcad", "base": base(&dir),
               "parts": {"occurrences": ["O99"]}}),
    )
    .unwrap_err();
    assert!(refused.contains("no occurrence O99"), "{refused}");
    let refused = command(
        &mut doc,
        json!({"cmd": "insert_component", "path": "assembly.mitcad", "base": base(&dir),
               "link": false, "parts": {"occurrences": [ids.bracket1.to_string()]}}),
    )
    .unwrap_err();
    assert!(refused.contains("only a linked design"), "{refused}");
    let refused = command(
        &mut doc,
        json!({"cmd": "insert_component", "path": "assembly.mitcad", "base": base(&dir),
               "parts": {}}),
    )
    .unwrap_err();
    assert!(refused.contains("at least one"), "{refused}");
}

#[test]
fn one_of_two_occurrences_of_a_component_comes_in_alone_where_the_source_places_it() {
    let (dir, ids) = saved_source("2130-twice");
    let mut doc = Document::new(MockKernel::default());
    let one = component(&insert(
        &mut doc,
        &dir,
        json!({"occurrences": [ids.bracket2.to_string()]}),
    ));
    let data = placed(&doc, one);
    assert_eq!(data.len(), 1, "{data:?}");
    assert_eq!(ends(&data, ",300,0,0)"), 1, "{data:?}");

    // Both ticked: both, each where the design places it; by their names.
    let both = component(&insert(
        &mut doc,
        &dir,
        json!({"occurrences": ["Bracket:1", "Bracket:2"]}),
    ));
    let data = placed(&doc, both);
    assert_eq!(data.len(), 2, "{data:?}");
    assert_eq!(ends(&data, ",100,0,0)"), 1, "{data:?}");
    assert_eq!(ends(&data, ",300,0,0)"), 1, "{data:?}");
}

#[test]
fn an_occurrence_inside_an_unticked_parent_comes_in_at_the_parents_placement() {
    let (dir, ids) = saved_source("2130-nested");
    let mut doc = Document::new(MockKernel::default());
    let chip = component(&insert(
        &mut doc,
        &dir,
        json!({"occurrences": [format!("{}/{}", ids.board, ids.chip)]}),
    ));
    let data = placed(&doc, chip);
    assert_eq!(data.len(), 1, "{data:?}");
    assert_eq!(ends(&data, ",0,200,10)"), 1, "{data:?}");

    // The board ticked: it comes with the chip in it.
    let board = component(&insert(&mut doc, &dir, json!({"occurrences": ["Board:1"]})));
    let data = placed(&doc, board);
    assert_eq!(data.len(), 2, "{data:?}");
    assert_eq!(ends(&data, ",0,200,0)"), 1, "{data:?}");
    assert_eq!(ends(&data, ",0,200,10)"), 1, "{data:?}");
}

#[test]
fn update_links_takes_a_change_of_a_chosen_part_and_ignores_an_unchosen_one() {
    let (dir, ids) = saved_source("2130-update");
    let mut doc = Document::new(MockKernel::default());
    let linked = component(&insert(
        &mut doc,
        &dir,
        json!({"occurrences": [ids.bracket1.to_string()]}),
    ));
    let first = placed(&doc, linked);

    // The board (not chosen) gets a second block: the link holds the same.
    let (changed, _) = source(1, 2, true);
    write_source(&dir, &changed);
    let messages = doc.update_links(Some(&dir)).unwrap();
    assert_eq!(placed(&doc, linked), first, "{messages:?}");

    // The bracket (chosen) gets a second block: the link takes it, at
    // Bracket:1's placement only.
    let (changed, _) = source(2, 2, true);
    write_source(&dir, &changed);
    let messages = doc.update_links(Some(&dir)).unwrap();
    assert!(
        messages
            .iter()
            .any(|m| m == "Linked is updated from assembly.mitcad"),
        "{messages:?}"
    );
    let data = placed(&doc, linked);
    assert_eq!(data.len(), 2, "{data:?}");
    assert_eq!(ends(&data, ",100,0,0)"), 2, "{data:?}");
}

#[test]
fn a_part_gone_from_the_design_keeps_its_saved_bodies_with_a_message() {
    let (dir, ids) = saved_source("2130-gone");
    let mut doc = Document::new(MockKernel::default());
    let linked = component(&insert(
        &mut doc,
        &dir,
        json!({"occurrences": [ids.bracket1.to_string(),
                               format!("{}/{}", ids.board, ids.chip)]}),
    ));
    let before = placed(&doc, linked);
    assert_eq!(before.len(), 2, "{before:?}");
    let bodies = doc.component_bodies(linked);

    // The chip is taken out of the design.
    let (changed, _) = source(1, 1, false);
    write_source(&dir, &changed);
    let messages = doc.update_links(Some(&dir)).unwrap();
    assert!(
        messages
            .iter()
            .any(|m| m == "Chip:1 is no longer in assembly.mitcad; its saved shape is kept"),
        "{messages:?}"
    );
    // In words: no ids, no paths.
    assert!(
        messages
            .iter()
            .all(|m| !m.contains('/') && !m.contains("O1")),
        "{messages:?}"
    );
    // Its saved body stays, under the same id; the bracket's too.
    assert_eq!(placed(&doc, linked), before);
    assert_eq!(doc.component_bodies(linked), bodies);
    assert_eq!(
        link_parts(&doc, linked).unwrap(),
        [
            ids.bracket1.to_string(),
            format!("{}/{}", ids.board, ids.chip)
        ]
    );
}

#[test]
fn the_design_file_keeps_the_parts_and_a_file_without_them_links_the_whole_design() {
    let (dir, ids) = saved_source("2130-file");
    let mut doc = Document::new(MockKernel::default());
    let linked = component(&insert(
        &mut doc,
        &dir,
        json!({"occurrences": [ids.bracket2.to_string()],
               "bodies": [ids.root_body.to_string()]}),
    ));
    let whole = component(&insert(&mut doc, &dir, Value::Null));
    // Without parts: the whole design, with its components beside the
    // root's own body (mitcad#2209): Bracket:1, Bracket:2 and Board:1.
    assert_eq!(placed(&doc, whole).len(), 1);
    assert_eq!(doc.assembly().children(whole).count(), 3);
    assert_eq!(link_parts(&doc, whole), None);

    let text = doc.to_json();
    let saved: Value = serde_json::from_str(&text).unwrap();
    let stored = saved["components"].as_array().unwrap();
    let with_parts: Vec<&Value> = stored
        .iter()
        .filter(|c| c["link"]["parts"].is_array())
        .collect();
    assert_eq!(with_parts.len(), 1, "{saved}");
    assert_eq!(
        with_parts[0]["link"]["parts"][0]["occurrence"],
        json!([ids.bracket2.to_string()]),
        "{saved}"
    );
    assert_eq!(
        with_parts[0]["link"]["parts"][1]["body"],
        json!(ids.root_body.to_string()),
        "{saved}"
    );

    // Read back and opened: the same parts and bodies, nothing changed.
    let path = dir.join("holder.mitcad");
    std::fs::write(&path, &text).unwrap();
    let mut opened = Document::from_json_at(&text, &path, MockKernel::default()).unwrap();
    let messages = opened.update_links(Some(&dir)).unwrap();
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(link_parts(&opened, linked), link_parts(&doc, linked));
    assert_eq!(placed(&opened, linked), placed(&doc, linked));
    assert_eq!(placed(&opened, whole).len(), 1);
    assert_eq!(opened.assembly().children(whole).count(), 3);
    assert_eq!(link_parts(&opened, whole), None);

    // The query the dialog lists them from.
    let listed = query(
        &opened,
        json!({"query": "link_parts", "component": linked.to_string(), "base": base(&dir)}),
    );
    assert_eq!(listed["file"], json!("assembly.mitcad"), "{listed}");
    assert_eq!(
        listed["chosen"]["occurrences"],
        json!([ids.bracket2.to_string()]),
        "{listed}"
    );
    let names: Vec<&str> = listed["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Bracket:1", "Bracket:2", "Board:1"], "{listed}");
    assert_eq!(
        listed["occurrences"][2]["children"][0]["path"],
        json!(format!("{}/{}", ids.board, ids.chip)),
        "{listed}"
    );
    assert_eq!(
        listed["bodies"].as_array().map(Vec::len),
        Some(1),
        "{listed}"
    );
}

#[test]
fn set_link_parts_changes_the_choice_in_one_undo_step() {
    let (dir, ids) = saved_source("2130-set");
    let mut doc = Document::new(MockKernel::default());
    let linked = component(&insert(
        &mut doc,
        &dir,
        json!({"occurrences": [ids.bracket1.to_string()]}),
    ));
    let first = doc.component_bodies(linked);
    assert_eq!(first.len(), 1);

    // Choose Linked Parts... adds Bracket:2: the first body keeps its id.
    let set = command(
        &mut doc,
        json!({"cmd": "set_link_parts", "component": "Linked", "base": base(&dir),
               "parts": {"occurrences": [ids.bracket1.to_string(), ids.bracket2.to_string()]}}),
    )
    .unwrap();
    assert_eq!(set["changed"], json!(true), "{set}");
    let now = doc.component_bodies(linked);
    assert_eq!(now.len(), 2, "{set}");
    assert!(now.contains(&first[0]), "{now:?} {first:?}");
    let data = placed(&doc, linked);
    assert_eq!(ends(&data, ",100,0,0)"), 1, "{data:?}");
    assert_eq!(ends(&data, ",300,0,0)"), 1, "{data:?}");

    // Asked again: nothing changes, no undo step.
    let again = command(
        &mut doc,
        json!({"cmd": "set_link_parts", "component": "Linked", "base": base(&dir),
               "parts": {"occurrences": [ids.bracket2.to_string(), ids.bracket1.to_string()]}}),
    )
    .unwrap();
    assert_eq!(again["changed"], json!(false), "{again}");

    // Undo takes it back in one step.
    let undone = doc.undo();
    assert_eq!(undone.as_deref(), Some("Choose Linked Parts of Linked"));
    assert_eq!(doc.component_bodies(linked), first);
    assert_eq!(
        link_parts(&doc, linked).unwrap(),
        [ids.bracket1.to_string()]
    );

    // A part the design does not have is refused; null links the whole
    // design again.
    let refused = command(
        &mut doc,
        json!({"cmd": "set_link_parts", "component": "Linked", "base": base(&dir),
               "parts": {"occurrences": ["O99"]}}),
    )
    .unwrap_err();
    assert!(refused.contains("no occurrence O99"), "{refused}");
    command(
        &mut doc,
        json!({"cmd": "set_link_parts", "component": "Linked", "base": base(&dir),
               "parts": null}),
    )
    .unwrap();
    assert_eq!(doc.component_bodies(linked).len(), 5);
    assert_eq!(link_parts(&doc, linked), None);
}

#[test]
fn a_whole_design_link_switched_to_parts_keeps_the_ids_of_their_bodies() {
    let (dir, ids) = saved_source("2130-whole");
    let mut doc = Document::new(MockKernel::default());
    // The whole design linked flat, as chosen parts made whole again (and
    // as links saved before mitcad#2209 have it): a whole design inserted
    // linked keeps its components, and takes no chosen parts
    // (`link_tree_tests.rs`).
    let linked = component(&insert(
        &mut doc,
        &dir,
        json!({"occurrences": [ids.bracket1.to_string()]}),
    ));
    command(
        &mut doc,
        json!({"cmd": "set_link_parts", "component": "Linked", "base": base(&dir),
               "parts": null}),
    )
    .unwrap();
    let bodies = doc.component_bodies(linked);
    assert_eq!(bodies.len(), 5);
    let data = placed(&doc, linked);
    let at = data.iter().position(|d| d.ends_with(",300,0,0)")).unwrap();
    let link = doc
        .assembly()
        .component(linked)
        .and_then(|c| c.link.clone())
        .unwrap();
    let bracket2_body = BodyUid::new(link.feature, u32::try_from(at).unwrap());
    assert!(bodies.contains(&bracket2_body), "{bodies:?}");

    command(
        &mut doc,
        json!({"cmd": "set_link_parts", "component": "Linked", "base": base(&dir),
               "parts": {"occurrences": [ids.bracket2.to_string()]}}),
    )
    .unwrap();
    assert_eq!(doc.component_bodies(linked), [bracket2_body]);
}

#[test]
fn unticking_a_part_a_joint_uses_keeps_its_place_with_a_warning_in_words() {
    use mitcad_model::datum::SurfaceGeometry;
    let (dir, ids) = saved_source("2130-joint");
    let mut doc = Document::new(MockKernel::default());
    let reply = insert(
        &mut doc,
        &dir,
        json!({"occurrences": [ids.bracket1.to_string(), ids.bracket2.to_string()]}),
    );
    let linked = component(&reply);
    let occurrence = reply["occurrence"].as_str().unwrap().to_owned();
    let (plate_component, plate) = doc
        .create_component(Some("Plate"), shift(0.0, 0.0, -50.0), true)
        .unwrap();
    block(&mut doc, 0.0);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.set_occurrence_grounded(plate, true).unwrap();
    let plate_body = doc.component_bodies(plate_component)[0];
    let plate_top = doc
        .body_shape(plate_body)
        .unwrap()
        .faces
        .iter()
        .find(|f| f.role == "end")
        .unwrap()
        .to_string();
    // A face of Bracket:1's body in the link: a plane at its bottom.
    let bracket_body = doc.component_bodies(linked)[0];
    let bracket_face = doc.body_shape(bracket_body).unwrap().faces[0].to_string();
    doc.kernel().surfaces.borrow_mut().insert(
        bracket_face.clone(),
        SurfaceGeometry::Plane {
            origin: [105.0, 5.0, 0.0],
            normal: [0.0, 0.0, -1.0],
        },
    );
    let joint = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": occurrence,
                     "geometry": {"body": bracket_body, "face": bracket_face}},
               "b": {"occurrence": plate.to_string(),
                     "geometry": {"body": plate_body, "face": plate_top}}}),
    )
    .unwrap();
    let joint: mitcad_model::ids::FeatureUid = joint["uid"].as_str().unwrap().parse().unwrap();
    assert_eq!(doc.status(joint), Some(&FeatureStatus::Ok));
    let occurrence: OccurrenceUid = occurrence.parse().unwrap();
    let joined = doc.placement(occurrence).unwrap();

    // Bracket:1 unticked: the joint keeps the place it had, with a warning
    // in words (no names, ids or paths).
    command(
        &mut doc,
        json!({"cmd": "set_link_parts", "component": "Linked", "base": base(&dir),
               "parts": {"occurrences": [ids.bracket2.to_string()]}}),
    )
    .unwrap();
    assert!(!doc.component_bodies(linked).contains(&bracket_body));
    assert_eq!(doc.status(joint), Some(&FeatureStatus::Ok));
    assert_eq!(doc.placement(occurrence), Some(joined));
    let timeline = query(&doc, json!({"query": "timeline"}));
    let entry = timeline["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == json!(joint.to_string()))
        .unwrap()
        .clone();
    assert_eq!(entry["level"], json!("warning"), "{entry}");
    let text = entry["notices"].to_string();
    assert!(text.contains("no longer linked"), "{entry}");
    assert!(
        !text.contains(&bracket_body.to_string()) && !text.contains("Bracket"),
        "{entry}"
    );
    // Kept with the link in the design file.
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(
        saved["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["link"]["unlinked"]
                .as_array()
                .is_some_and(|f| f.len() == 1)),
        "{saved}"
    );

    // Undo: the part and its face are back, the warning gone.
    doc.undo();
    assert!(doc.component_bodies(linked).contains(&bracket_body));
    let timeline = query(&doc, json!({"query": "timeline"}));
    let entry = timeline["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == json!(joint.to_string()))
        .unwrap()
        .clone();
    assert_ne!(entry["level"], json!("warning"), "{entry}");
}

#[test]
fn chosen_parts_of_a_design_copied_into_the_project_are_linked_against_the_copy() {
    // mitcad#2137: Insert Component of a design outside the project with
    // Copy into the Project and Link and Choose parts... ticked. The parts
    // are listed from the file asked for, before anything is copied; the
    // copy has the same occurrences and bodies, so the link records them
    // against the copy.
    let (outside, ids) = saved_source("2137-outside");
    let dir = mitcad_model::testing::unique_temp_dir("2137-project");
    let project = dir.join("project");
    mitcad_model::Project::init(&project).unwrap();
    let designs = project.join("designs");
    std::fs::create_dir_all(&designs).unwrap();
    let file = outside.join("assembly.mitcad");
    let mut doc = Document::new(MockKernel::default());

    // The list the dialog shows: from the file outside, nothing copied yet.
    let listed = query(
        &doc,
        json!({"query": "link_parts", "path": file.to_string_lossy(), "base": base(&designs)}),
    );
    assert_eq!(
        listed["occurrences"][0]["name"],
        json!("Bracket:1"),
        "{listed}"
    );
    assert!(!project.join("parts").exists());

    let reply = command(
        &mut doc,
        json!({"cmd": "insert_component", "path": file.to_string_lossy(),
               "base": base(&designs), "copy_into_project": true,
               "parts": {"occurrences": [ids.bracket1.to_string()]}}),
    )
    .unwrap();
    let copy = project.join("parts").join("assembly.mitcad");
    assert_eq!(
        reply["copied"],
        json!([{"from": file.to_string_lossy(), "to": copy.to_string_lossy()}]),
        "{reply}"
    );
    assert_eq!(reply["name"], json!("assembly"), "{reply}");
    assert_eq!(reply["parts"]["names"], json!(["Bracket:1"]), "{reply}");
    let linked = component(&reply);
    // Only Bracket:1's body, where the design places it.
    assert_eq!(doc.component_bodies(linked).len(), 1, "{reply}");
    let data = placed(&doc, linked);
    assert_eq!(ends(&data, ",100,0,0)"), 1, "{data:?}");
    let link = doc
        .assembly()
        .component(linked)
        .and_then(|c| c.link.clone())
        .unwrap();
    assert_eq!(link.path, "../parts/assembly.mitcad");
    assert_eq!(
        link_parts(&doc, linked),
        Some(vec![ids.bracket1.to_string()])
    );

    // Saved and opened again: the parts are read from the copy.
    let path = designs.join("holder.mitcad");
    let text = doc.to_json();
    std::fs::write(&path, &text).unwrap();
    let mut opened = Document::from_json_at(&text, &path, MockKernel::default()).unwrap();
    let messages = opened.update_links(Some(&designs)).unwrap();
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(placed(&opened, linked), placed(&doc, linked));
    let listed = query(
        &opened,
        json!({"query": "link_parts", "component": linked.to_string(), "base": base(&designs)}),
    );
    assert_eq!(listed["file"], json!("assembly.mitcad"), "{listed}");
    assert_eq!(
        listed["chosen"]["occurrences"],
        json!([ids.bracket1.to_string()]),
        "{listed}"
    );

    // A change of the copy's chosen part comes in with Update Link; the
    // file outside is no longer read.
    let (changed, _) = source(2, 1, true);
    std::fs::write(&copy, changed.to_json()).unwrap();
    let messages = opened.update_links(Some(&designs)).unwrap();
    let data = placed(&opened, linked);
    assert_eq!(data.len(), 2, "{messages:?}");
    assert_eq!(ends(&data, ",100,0,0)"), 2, "{data:?}");
    mitcad_model::testing::remove_temp(&dir);
    mitcad_model::testing::remove_temp(&outside);
}
