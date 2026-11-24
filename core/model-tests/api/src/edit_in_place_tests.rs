// SPDX-License-Identifier: MIT
//! Edit in Place (mitcad#2321) with the mock kernel: `holder.mitcad` holds
//! a plate of its own (a 10 × 10 × 5 block in the component Plate) and
//! links `part.mitcad` (one block) as the component `part` at x = 100, both
//! in one folder. The part's own document, edited in place, shows the
//! plate in a read-only component of its own that no save writes; an edit
//! changes the part and, at Finish, the holder's bodies in one undo step;
//! a sketch on the plate's face is frozen at once with a line in words;
//! Undo in the mode goes back only through the steps made in it; undoing
//! the holder's step after the part was saved leaves the file as it is and
//! marks the link changed, and Update brings the edits back; the part's
//! file opens on its own.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::ComponentUid;
use mitcad_model::kernel::Plane;
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, FileFormat, InsertOptions};
use mitcad_model_api::{
    DocumentApi, begin_edit_in_place, finish_edit_in_place, linked_saved_in_place,
};

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

/// A block of 10 × 10 × 5 at x = `x` in the active component.
fn block(doc: &mut Document<MockKernel>, x: f64) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [x, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    doc.add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap();
}

/// A design of `blocks` blocks 20 mm apart.
fn design(blocks: usize) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    for i in 0..blocks {
        block(&mut doc, 20.0 * i as f64);
    }
    doc
}

/// A design file opened as the application opens it: read, its links
/// followed from its folder, computed, not modified.
fn open(path: &Path) -> Document<MockKernel> {
    let text = std::fs::read_to_string(path).unwrap();
    let mut doc = Document::from_json_at(&text, path, MockKernel::default()).unwrap();
    doc.update_links(path.parent()).unwrap();
    doc.recompute();
    doc.mark_saved();
    doc
}

/// The folder with `part.mitcad` (one block) and `holder.mitcad` (Plate's
/// block, and the part linked as `part` at x = 100).
struct Pair {
    dir: PathBuf,
    holder: PathBuf,
    part: PathBuf,
    component: ComponentUid,
}

fn pair(name: &str) -> Pair {
    let dir = mitcad_model::testing::unique_temp_dir(name);
    let part = dir.join("part.mitcad");
    design(1).save_file(&part, FileFormat::Single).unwrap();
    let mut holder = Document::new(MockKernel::default());
    holder
        .create_component(Some("Plate"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut holder, 0.0);
    holder.activate_component(ComponentUid::ROOT).unwrap();
    let options = InsertOptions {
        base: Some(dir.clone()),
        transform: Transform::translation([100.0, 0.0, 0.0]),
        ..InsertOptions::default()
    };
    let (component, _) = holder
        .insert_component(part.to_str().unwrap(), &options)
        .unwrap();
    let path = dir.join("holder.mitcad");
    holder.save_file(&path, FileFormat::Single).unwrap();
    Pair {
        dir,
        holder: path,
        part,
        component,
    }
}

impl Pair {
    fn base(&self) -> String {
        self.dir.to_string_lossy().into_owned()
    }

    fn begin(&self, linked: &mut Document<MockKernel>, holder: &Document<MockKernel>) -> Value {
        let request = json!({"component": self.component.to_string(), "holder": "holder.mitcad"});
        let answer = begin_edit_in_place(linked, holder, &request.to_string())
            .unwrap_or_else(|e| panic!("begin: {e}"));
        serde_json::from_str(&answer).unwrap()
    }

    fn finish(
        &self,
        holder: &mut Document<MockKernel>,
        linked: &mut Document<MockKernel>,
    ) -> Value {
        let request = json!({"component": self.component.to_string(), "base": self.base()});
        let answer = finish_edit_in_place(holder, linked, &request.to_string())
            .unwrap_or_else(|e| panic!("finish: {e}"));
        serde_json::from_str(&answer).unwrap()
    }

    /// Whether the `links` query says the part's file changed.
    fn link_changed(&self, holder: &Document<MockKernel>) -> bool {
        let links = query(holder, json!({"query": "links", "base": self.base()}));
        links["links"][0]["changed"].as_bool().unwrap()
    }
}

/// Every feature of a design built.
fn all_built(doc: &Document<MockKernel>) {
    for f in doc.features() {
        assert!(
            doc.status(f.uid).is_some_and(|s| s.is_ok()),
            "{}: {:?}",
            f.name,
            doc.status(f.uid)
        );
    }
}

#[test]
fn an_edit_in_place_changes_the_linked_design_and_the_holders_bodies() {
    let pair = pair("2321-edit");
    let mut holder = open(&pair.holder);
    let mut linked = open(&pair.part);
    let saved = std::fs::read_to_string(&pair.part).unwrap();
    assert_eq!(holder.component_bodies(pair.component).len(), 1);

    let begun = pair.begin(&mut linked, &holder);
    // The assembly around it: the plate's block, in a component of its own
    // that is no change to the design.
    assert_eq!(begun["name"], json!("part"), "{begun}");
    assert_eq!(begun["bodies"].as_array().unwrap().len(), 1, "{begun}");
    assert!(!linked.is_modified());
    assert_eq!(linked.undo_depth(), 0);
    let doc = query(&linked, json!({"query": "document"}));
    assert_eq!(doc["in_place"]["holder"], json!("holder.mitcad"), "{doc}");
    assert_eq!(doc["in_place"]["context"], begun["context"], "{doc}");
    // Read only: it cannot be activated or changed.
    let refused = linked
        .command(&json!({"cmd": "activate_component", "component": begun["context"]}).to_string())
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("the assembly around the design"),
        "{refused}"
    );
    // What a save writes has no assembly in it.
    let written = Document::from_json(&linked.to_json(), MockKernel::default()).unwrap();
    assert!(written.assembly().components.is_empty());

    // An edit: a second block in the part.
    block(&mut linked, 30.0);
    assert_eq!(linked.component_bodies(ComponentUid::ROOT).len(), 2);
    assert!(linked.is_modified());

    // Finish: the holder takes the part's bodies, one undo step; the part's
    // file is not written, and the assembly is gone from the part.
    let done = pair.finish(&mut holder, &mut linked);
    assert_eq!(done["changed"], json!(true), "{done}");
    assert_eq!(done["saved"], json!(false), "{done}");
    assert_eq!(done["label"], json!("Edit in Place part"), "{done}");
    assert_eq!(
        done["messages"],
        json!(["part takes the edits made in place"]),
        "{done}"
    );
    assert_eq!(holder.component_bodies(pair.component).len(), 2);
    assert_eq!(holder.undo_depth(), 1);
    assert_eq!(holder.undo_label(), Some("Edit in Place part"));
    assert!(linked.in_place_mode().is_none());
    assert!(linked.assembly().components.is_empty());
    assert!(linked.assembly().occurrences.is_empty());
    assert_eq!(std::fs::read_to_string(&pair.part).unwrap(), saved);
    // The file was not saved: the link keeps its digest, not changed.
    assert!(!pair.link_changed(&holder));

    // Nothing changed in the mode: no undo step in the holder.
    pair.begin(&mut linked, &holder);
    let done = pair.finish(&mut holder, &mut linked);
    assert_eq!(done["changed"], json!(false), "{done}");
    assert_eq!(holder.undo_depth(), 1);
    let _ = std::fs::remove_dir_all(&pair.dir);
}

#[test]
fn a_pick_of_the_assembly_is_frozen_with_its_notice() {
    let pair = pair("2321-pick");
    let holder = open(&pair.holder);
    let mut linked = open(&pair.part);
    let begun = pair.begin(&mut linked, &holder);
    let body = begun["bodies"][0].as_str().unwrap().to_owned();
    let feature = begun["feature"].as_str().unwrap();
    let occurrence = begun["occurrence"].as_str().unwrap().to_owned();
    // The plate's top face, 5 mm up (in the part's coordinates as in the
    // holder's: the plate is not turned).
    let face = format!("{feature}:import(0)");
    // The mock kernel has the geometry of an imported body's faces only
    // where a test gives it: the plane, and the surface a sketch's plane is
    // read from.
    linked.kernel().measures_selections.set(true);
    linked.kernel().side_planes.borrow_mut().insert(
        face.clone(),
        Plane {
            origin: [5.0, 5.0, 5.0],
            normal: [0.0, 0.0, 1.0],
        },
    );
    linked.kernel().surfaces.borrow_mut().insert(
        face.clone(),
        SurfaceGeometry::Plane {
            origin: [5.0, 5.0, 5.0],
            normal: [0.0, 0.0, 1.0],
        },
    );
    let depth = linked.undo_depth();

    // A sketch on it: frozen at once, in the same undo step, with a line.
    let made = command(
        &mut linked,
        json!({"cmd": "sketch.create", "plane": {"face": face, "body": body},
               "occurrence": occurrence}),
    );
    let sketch = made["uid"].as_str().unwrap().to_owned();
    let name = linked
        .features()
        .find(|f| f.uid.to_string() == sketch)
        .map(|f| f.name.clone())
        .unwrap();
    assert_eq!(
        made["in_place_notices"],
        json!([format!(
            "{name}: its plane on the assembly is fixed where it is now; it will not follow \
             the assembly"
        )]),
        "{made}"
    );
    assert_eq!(linked.undo_depth(), depth + 1);
    let s = query(&linked, json!({"query": "sketch", "uid": sketch}));
    assert_eq!(s["plane_link"], Value::Null, "{s}");
    assert!(s["frame"].is_object(), "{s}");
    all_built(&linked);

    // A command that refers to nothing of the assembly says nothing.
    let renamed = command(
        &mut linked,
        json!({"cmd": "rename_feature", "uid": sketch, "name": "On the plate"}),
    );
    assert!(renamed.get("in_place_notices").is_none(), "{renamed}");

    // Saved, the part opens on its own: the sketch on its fixed plane.
    linked.save_file(&pair.part, FileFormat::Single).unwrap();
    let opened = open(&pair.part);
    assert!(opened.assembly().components.is_empty());
    assert!(opened.features().any(|f| f.name == "On the plate"));
    all_built(&opened);
    let _ = std::fs::remove_dir_all(&pair.dir);
}

#[test]
fn undo_in_the_mode_goes_back_through_its_own_steps_only() {
    let pair = pair("2321-undo");
    let holder = open(&pair.holder);
    let mut linked = open(&pair.part);
    // A step before the mode (the part edited in its tab before).
    block(&mut linked, 60.0);
    let before = linked.undo_depth();
    pair.begin(&mut linked, &holder);
    assert_eq!(linked.undo_depth(), before);
    block(&mut linked, 30.0);
    assert_eq!(linked.component_bodies(ComponentUid::ROOT).len(), 3);
    let steps = linked.undo_depth() - before;
    assert!(steps > 0);

    // Undo: the part's steps made in the mode.
    for _ in 0..steps {
        assert!(linked.undo().is_some());
    }
    assert_eq!(linked.component_bodies(ComponentUid::ROOT).len(), 2);
    assert_eq!(linked.undo_depth(), before);
    // Not past the mode's start: the assembly stays.
    assert_eq!(linked.undo(), None);
    assert_eq!(linked.undo_depth(), before);
    assert!(linked.in_place_mode().is_some());
    let context = linked.in_place_mode().unwrap().component;
    assert!(linked.assembly().component(context).is_some());
    // Redo takes them again.
    for _ in 0..steps {
        assert!(linked.redo().is_some());
    }
    assert_eq!(linked.component_bodies(ComponentUid::ROOT).len(), 3);

    // Once the mode ends, the steps before it are there again, without the
    // assembly.
    linked.end_in_place().unwrap();
    while linked.undo().is_some() {}
    assert_eq!(linked.undo_depth(), 0);
    assert_eq!(linked.component_bodies(ComponentUid::ROOT).len(), 1);
    assert!(linked.assembly().components.is_empty());
    let _ = std::fs::remove_dir_all(&pair.dir);
}

#[test]
fn undoing_the_holders_step_after_save_marks_the_link_changed() {
    let pair = pair("2321-saved");
    let mut holder = open(&pair.holder);
    let mut linked = open(&pair.part);
    pair.begin(&mut linked, &holder);
    block(&mut linked, 30.0);
    // Saved in the mode: the file has the edit and no assembly.
    linked.save_file(&pair.part, FileFormat::Single).unwrap();
    linked.mark_saved();
    let saved = std::fs::read_to_string(&pair.part).unwrap();

    let done = pair.finish(&mut holder, &mut linked);
    assert_eq!(done["saved"], json!(true), "{done}");
    assert!(!linked.is_modified());
    assert_eq!(holder.component_bodies(pair.component).len(), 2);
    assert_eq!(holder.undo_depth(), 1);
    // The link has the file's digest: nothing to update.
    assert!(!pair.link_changed(&holder));

    // Undone: the holder's bodies only; the file keeps the edit, so the
    // link says it changed, and Update brings the edit back.
    assert_eq!(holder.undo().as_deref(), Some("Edit in Place part"));
    assert_eq!(holder.component_bodies(pair.component).len(), 1);
    assert_eq!(std::fs::read_to_string(&pair.part).unwrap(), saved);
    assert!(pair.link_changed(&holder));
    let updated = command(
        &mut holder,
        json!({"cmd": "update_link", "component": pair.component.to_string(),
               "base": pair.base()}),
    );
    assert_eq!(updated["updated"], json!([pair.component]), "{updated}");
    assert_eq!(holder.component_bodies(pair.component).len(), 2);

    // The part's file opens on its own, with both blocks.
    let opened = open(&pair.part);
    assert_eq!(opened.component_bodies(ComponentUid::ROOT).len(), 2);
    assert!(opened.assembly().components.is_empty());
    all_built(&opened);
    let _ = std::fs::remove_dir_all(&pair.dir);
}

#[test]
fn the_holders_save_records_the_part_it_wrote() {
    let pair = pair("2321-holder-save");
    let mut holder = open(&pair.holder);
    let mut linked = open(&pair.part);
    pair.begin(&mut linked, &holder);
    block(&mut linked, 30.0);
    // Finished unsaved; the holder's Save then writes the part first.
    let done = pair.finish(&mut holder, &mut linked);
    assert_eq!(done["saved"], json!(false), "{done}");
    linked.save_file(&pair.part, FileFormat::Single).unwrap();
    linked.mark_saved();
    assert!(pair.link_changed(&holder));
    let request = json!({"component": pair.component.to_string(), "base": pair.base()});
    let answer: Value =
        serde_json::from_str(&linked_saved_in_place(&mut holder, &request.to_string()).unwrap())
            .unwrap();
    assert_eq!(answer["changed"], json!(true), "{answer}");
    assert!(!pair.link_changed(&holder));
    // No undo step of its own: the holder's step is still the edit.
    assert_eq!(holder.undo_depth(), 1);
    let _ = std::fs::remove_dir_all(&pair.dir);
}

#[test]
fn only_a_linked_design_is_edited_in_place() {
    let pair = pair("2321-refused");
    let holder = open(&pair.holder);
    let mut linked = open(&pair.part);
    let plate = holder
        .assembly()
        .components
        .iter()
        .find(|c| c.name == "Plate")
        .unwrap()
        .uid;
    let request = json!({"component": plate.to_string()});
    let refused = begin_edit_in_place(&mut linked, &holder, &request.to_string()).unwrap_err();
    assert!(
        refused.to_string().contains("Plate is not a linked design"),
        "{refused}"
    );
    assert!(linked.in_place_mode().is_none());
    // Not twice at once.
    pair.begin(&mut linked, &holder);
    let request = json!({"component": pair.component.to_string()});
    let refused = begin_edit_in_place(&mut linked, &holder, &request.to_string()).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("edited in place in holder.mitcad already"),
        "{refused}"
    );
    let _ = std::fs::remove_dir_all(&pair.dir);
}
