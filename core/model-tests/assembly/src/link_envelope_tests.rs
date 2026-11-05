// SPDX-License-Identifier: MIT
//! A linked component shown by its envelope (mitcad#1271) with the mock
//! kernel, whose meshes are the bodies' boxes: `set_link_envelope` puts
//! one envelope body into the component's base feature instead of the
//! linked design's bodies, records its detail size in the link and the
//! design file, keeps it when the file changes, takes the bodies back in
//! full, undoes in one step, and refuses what is not a linked design.
//! Through the OCCT kernel and the application: `app.ui-windows-envelope`
//! (`tools/ui-windows-envelope-test.ps1`) and `tools/ui-envelope-test.sh`.

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, InsertOptions};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|text| serde_json::from_str(&text).unwrap())
        .map_err(|e| e.to_string())
}

/// A block of `width` × 10 × 5 at x = `x` in the active component.
fn block(doc: &mut Document<MockKernel>, x: f64, width: f64) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [x, 0.0], &num(width), &num(10.0))
        .unwrap()
        .region;
    doc.add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap();
}

/// A machine of two blocks 90 mm apart.
fn machine() -> Document<MockKernel> {
    let mut part = Document::new(MockKernel::default());
    block(&mut part, 0.0, 10.0);
    block(&mut part, 100.0, 10.0);
    part
}

fn component_bodies(doc: &Document<MockKernel>, component: ComponentUid) -> Vec<BodyUid> {
    doc.bodies()
        .iter()
        .filter(|b| doc.body_component(b.uid) == Some(component))
        .map(|b| b.uid)
        .collect()
}

#[test]
fn a_linked_component_is_shown_by_its_envelope_and_in_full_again() {
    let dir = mitcad_model::testing::unique_temp_dir("1271-envelope");
    let path = dir.join("machine.mitcad");
    std::fs::write(&path, machine().to_json()).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let options = InsertOptions {
        base: Some(dir.clone()),
        ..InsertOptions::default()
    };
    let (linked, occurrence) = doc.insert_component("machine.mitcad", &options).unwrap();
    assert_eq!(component_bodies(&doc, linked).len(), 2);
    let base = dir.to_string_lossy().to_string();

    // Shown by its envelope: one body (two boxes, further apart than
    // 4 mm: twelve faces) instead of the design's two.
    let shown = command(
        &mut doc,
        json!({"cmd": "set_link_envelope", "occurrence": occurrence.to_string(),
               "detail": 4, "base": base}),
    )
    .unwrap();
    assert_eq!(shown["envelope"], json!(true), "{shown}");
    assert_eq!(shown["detail"], json!(4.0), "{shown}");
    assert_eq!(shown["changed"], json!(true), "{shown}");
    assert_eq!(shown["bodies"].as_array().map(Vec::len), Some(1), "{shown}");
    let bodies = component_bodies(&doc, linked);
    assert_eq!(bodies.len(), 1);
    assert_eq!(doc.body_shape(bodies[0]).unwrap().faces.len(), 12);
    let link = doc
        .assembly()
        .component(linked)
        .unwrap()
        .link
        .clone()
        .unwrap();
    assert_eq!(link.envelope, Some(4.0));
    let components: Value = serde_json::from_str(
        &doc.query(&json!({"query": "components"}).to_string())
            .unwrap(),
    )
    .unwrap();
    let listed = components["components"].as_array().unwrap();
    assert!(
        listed.iter().any(|c| c["envelope"] == json!(4.0)),
        "{components}"
    );
    // Kept in the design file.
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let stored = saved["components"].as_array().unwrap();
    assert!(
        stored.iter().any(|c| c["link"]["envelope"] == json!(4.0)),
        "{saved}"
    );
    // Asked again: nothing changes.
    let again = command(
        &mut doc,
        json!({"cmd": "set_link_envelope", "occurrence": occurrence.to_string(),
               "detail": 4, "base": base}),
    )
    .unwrap();
    assert_eq!(again["changed"], json!(false), "{again}");

    // The file changes (the second block made 95 wide, 5 mm from the
    // first, closer than the detail size of 20): read again, the envelope
    // is made again.
    command(
        &mut doc,
        json!({"cmd": "set_link_envelope", "occurrence": occurrence.to_string(),
               "detail": 20, "base": base}),
    )
    .unwrap();
    let mut changed = Document::new(MockKernel::default());
    block(&mut changed, 0.0, 95.0);
    block(&mut changed, 100.0, 10.0);
    std::fs::write(&path, changed.to_json()).unwrap();
    let messages = doc.update_links(Some(dir.as_path())).unwrap();
    assert!(
        messages.iter().any(|m| m.contains("is updated from")),
        "{messages:?}"
    );
    let bodies = component_bodies(&doc, linked);
    assert_eq!(bodies.len(), 1);
    assert_ne!(doc.body_shape(bodies[0]).unwrap().faces.len(), 12);
    let link = doc
        .assembly()
        .component(linked)
        .unwrap()
        .link
        .clone()
        .unwrap();
    assert_eq!(link.envelope, Some(20.0));

    // In full again: the file's two bodies.
    let full = command(
        &mut doc,
        json!({"cmd": "set_link_envelope", "occurrence": occurrence.to_string(),
               "envelope": false, "base": base}),
    )
    .unwrap();
    assert_eq!(full["envelope"], json!(false), "{full}");
    assert_eq!(full["detail"], Value::Null, "{full}");
    assert_eq!(component_bodies(&doc, linked).len(), 2);
    assert_eq!(
        doc.assembly()
            .component(linked)
            .unwrap()
            .link
            .clone()
            .unwrap()
            .envelope,
        None
    );
    // One undo step: the envelope again.
    assert!(doc.undo().is_some());
    assert_eq!(component_bodies(&doc, linked).len(), 1);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn only_a_linked_design_is_shown_by_its_envelope() {
    let dir = mitcad_model::testing::unique_temp_dir("1271-envelope-refused");
    std::fs::write(dir.join("machine.mitcad"), machine().to_json()).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let options = InsertOptions {
        base: Some(dir.clone()),
        ..InsertOptions::default()
    };
    let (_, linked) = doc.insert_component("machine.mitcad", &options).unwrap();
    let copy = InsertOptions {
        link: false,
        base: Some(dir.clone()),
        ..InsertOptions::default()
    };
    let (_, copied) = doc.insert_component("machine.mitcad", &copy).unwrap();
    let base = dir.to_string_lossy().to_string();
    let error = command(
        &mut doc,
        json!({"cmd": "set_link_envelope", "occurrence": copied.to_string(), "base": base}),
    )
    .unwrap_err();
    assert!(error.contains("not a linked component"), "{error}");
    let error = command(
        &mut doc,
        json!({"cmd": "set_link_envelope", "occurrence": linked.to_string(),
               "envelope": false, "detail": 5, "base": base}),
    )
    .unwrap_err();
    assert!(error.contains("a detail size goes with"), "{error}");
    let error = command(
        &mut doc,
        json!({"cmd": "set_link_envelope", "occurrence": linked.to_string(),
               "detail": 0, "base": base}),
    )
    .unwrap_err();
    assert!(error.contains("more than 0"), "{error}");
    // Shown in full already: nothing to do.
    let full = command(
        &mut doc,
        json!({"cmd": "set_link_envelope", "occurrence": linked.to_string(),
               "envelope": false, "base": base}),
    )
    .unwrap();
    assert_eq!(full["changed"], json!(false), "{full}");
    // The default detail size: a share of the design's diagonal, at least
    // 5 mm.
    let shown = command(
        &mut doc,
        json!({"cmd": "set_link_envelope", "occurrence": linked.to_string(), "base": base}),
    )
    .unwrap();
    assert_eq!(shown["detail"], json!(5.0), "{shown}");
    mitcad_model::testing::remove_temp(&dir);
}

// Each occurrence by its envelope or in full, and joints to the design's
// faces while it is shown by its envelope (mitcad#1276).

fn placed(doc: &Document<MockKernel>, occurrence: OccurrenceUid) -> ComponentUid {
    doc.assembly().occurrence(occurrence).unwrap().component
}

fn component_uid(value: &Value) -> ComponentUid {
    value["component"].as_str().unwrap().parse().unwrap()
}

/// A layout with the machine linked twice: Machine:1 (`first`) at the
/// origin and Machine:2 (`second`) 200 mm along y, both of `linked`.
struct Layout {
    dir: std::path::PathBuf,
    doc: Document<MockKernel>,
    linked: ComponentUid,
    first: OccurrenceUid,
    second: OccurrenceUid,
}

impl Layout {
    fn new(name: &str) -> Self {
        let dir = mitcad_model::testing::unique_temp_dir(name);
        std::fs::write(dir.join("machine.mitcad"), machine().to_json()).unwrap();
        let mut doc = Document::new(MockKernel::default());
        let options = InsertOptions {
            base: Some(dir.clone()),
            name: Some("Machine".to_owned()),
            ..InsertOptions::default()
        };
        let (linked, first) = doc.insert_component("machine.mitcad", &options).unwrap();
        let second = doc
            .copy_occurrence(first, Some(Transform::translation([0.0, 200.0, 0.0])))
            .unwrap();
        Layout {
            dir,
            doc,
            linked,
            first,
            second,
        }
    }

    fn show(&mut self, occurrence: OccurrenceUid, extra: Value) -> Value {
        let mut cmd = json!({"cmd": "set_link_envelope", "occurrence": occurrence.to_string(),
                             "base": self.dir.to_string_lossy()});
        for (key, value) in extra.as_object().unwrap() {
            cmd[key] = value.clone();
        }
        command(&mut self.doc, cmd).unwrap()
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        mitcad_model::testing::remove_temp(&self.dir);
    }
}

#[test]
fn each_occurrence_is_shown_by_its_envelope_or_in_full() {
    let mut layout = Layout::new("1276-envelope-occurrences");
    let (linked, first, second) = (layout.linked, layout.first, layout.second);

    // Machine:2 by its envelope: it goes into a component of its own that
    // links the same file; Machine:1 keeps the design's two bodies.
    let shown = layout.show(second, json!({"detail": 4}));
    assert_eq!(shown["changed"], json!(true), "{shown}");
    assert_eq!(shown["detail"], json!(4.0), "{shown}");
    let twin = component_uid(&shown);
    assert_ne!(twin, linked);
    let doc = &layout.doc;
    assert_eq!(placed(doc, second), twin);
    assert_eq!(placed(doc, first), linked);
    assert_eq!(component_bodies(doc, linked).len(), 2);
    assert_eq!(component_bodies(doc, twin).len(), 1);
    let made = doc.assembly().component(twin).unwrap();
    assert_eq!(made.name, "Machine Envelope");
    let link = made.link.clone().unwrap();
    assert_eq!(link.envelope, Some(4.0));
    assert_eq!(
        Some(link.path),
        doc.assembly()
            .component(linked)
            .and_then(|c| c.link.clone())
            .map(|l| l.path)
    );
    // Where it was, with its number.
    assert_eq!(
        doc.placement(second),
        Some(Transform::translation([0.0, 200.0, 0.0]))
    );
    assert_eq!(doc.assembly().occurrence(second).unwrap().number, 2);
    assert_eq!(doc.assembly().occurrence_name(second), "Machine Envelope:2");
    // Asked again: nothing changes.
    let again = layout.show(second, json!({"detail": 4}));
    assert_eq!(again["changed"], json!(false), "{again}");

    // One undo step: Machine:2 in the linked component again, the
    // component made for it gone.
    assert!(layout.doc.undo().is_some());
    assert_eq!(placed(&layout.doc, second), linked);
    assert!(layout.doc.assembly().component(twin).is_none());
    assert!(layout.doc.redo().is_some());
    assert_eq!(placed(&layout.doc, second), twin);

    // Machine:1 too, with the component's detail size (none given): the
    // last occurrence of the linked component joins Machine:2, and the
    // linked component, which nothing else uses, goes.
    let both = layout.show(first, json!({}));
    assert_eq!(component_uid(&both), twin, "{both}");
    assert_eq!(both["detail"], json!(4.0), "{both}");
    assert_eq!(placed(&layout.doc, first), twin);
    assert!(layout.doc.assembly().component(linked).is_none());

    // Machine:2 in full again: no component of the file shows it in full,
    // so its own does, and Machine:1 goes into one made for the envelope.
    let full = layout.show(second, json!({"envelope": false}));
    assert_eq!(full["envelope"], json!(false), "{full}");
    assert_eq!(component_uid(&full), twin, "{full}");
    let doc = &layout.doc;
    assert_eq!(component_bodies(doc, twin).len(), 2);
    assert_eq!(doc.assembly().name(twin), "Machine");
    let other = placed(doc, first);
    assert_ne!(other, twin);
    assert_eq!(
        doc.assembly()
            .component(other)
            .and_then(|c| c.link.clone())
            .and_then(|l| l.envelope),
        Some(4.0)
    );
    assert_eq!(component_bodies(doc, other).len(), 1);

    // Show All as Envelope: every occurrence of Machine:2's component.
    let all = layout.show(second, json!({"detail": 6, "all": true}));
    assert_eq!(component_uid(&all), twin, "{all}");
    assert_eq!(all["detail"], json!(6.0), "{all}");
    assert_eq!(component_bodies(&layout.doc, twin).len(), 1);
    assert_eq!(placed(&layout.doc, first), other);
}

#[test]
fn a_joint_to_a_linked_design_keeps_its_placement_while_shown_by_its_envelope() {
    use mitcad_model::FeatureStatus;
    use mitcad_model::datum::SurfaceGeometry;

    let mut layout = Layout::new("1276-envelope-joint");
    let (linked, first, second) = (layout.linked, layout.first, layout.second);
    // A plate under the machines: a block in a component of its own.
    let (plate_component, plate) = layout
        .doc
        .create_component(
            Some("Plate"),
            Transform::translation([0.0, 0.0, -50.0]),
            true,
        )
        .unwrap();
    block(&mut layout.doc, 0.0, 10.0);
    layout.doc.activate_component(ComponentUid::ROOT).unwrap();
    layout.doc.set_occurrence_grounded(plate, true).unwrap();
    let plate_body = component_bodies(&layout.doc, plate_component)[0];
    let plate_top = layout
        .doc
        .body_shape(plate_body)
        .unwrap()
        .faces
        .iter()
        .find(|f| f.role == "end")
        .unwrap()
        .to_string();
    // A face of the machine's first body: a plane at its bottom.
    let machine_body = component_bodies(&layout.doc, linked)[0];
    let machine_face = layout.doc.body_shape(machine_body).unwrap().faces[0].to_string();
    layout.doc.kernel().surfaces.borrow_mut().insert(
        machine_face.clone(),
        SurfaceGeometry::Plane {
            origin: [5.0, 5.0, 0.0],
            normal: [0.0, 0.0, -1.0],
        },
    );
    // Machine:2's face joined to the plate's top: Machine:2 moves onto it.
    let joint = command(
        &mut layout.doc,
        json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": second.to_string(),
                     "geometry": {"body": machine_body, "face": machine_face}},
               "b": {"occurrence": plate.to_string(),
                     "geometry": {"body": plate_body, "face": plate_top}}}),
    )
    .unwrap();
    let joint: mitcad_model::ids::FeatureUid = joint["uid"].as_str().unwrap().parse().unwrap();
    assert_eq!(layout.doc.status(joint), Some(&FeatureStatus::Ok));
    let joined = layout.doc.placement(second).unwrap();
    assert_ne!(joined, Transform::translation([0.0, 200.0, 0.0]));

    // Machine:2 by its envelope (a component of its own): the joint finds
    // its face in the linked component, which shows the design in full.
    let shown = layout.show(second, json!({"detail": 4}));
    assert_ne!(component_uid(&shown), linked);
    assert_eq!(layout.doc.status(joint), Some(&FeatureStatus::Ok));
    assert_eq!(layout.doc.placement(second), Some(joined));

    // Machine:1 by its envelope as well: the linked component, used by
    // the joint, is switched itself and keeps the joint's frame.
    let shown = layout.show(first, json!({}));
    assert_eq!(component_uid(&shown), linked, "{shown}");
    assert_eq!(component_bodies(&layout.doc, linked).len(), 1);
    let link = layout
        .doc
        .assembly()
        .component(linked)
        .and_then(|c| c.link.clone())
        .unwrap();
    assert_eq!(link.envelope, Some(4.0));
    assert_eq!(link.frames.len(), 1, "{:?}", link.frames);
    assert_eq!(layout.doc.status(joint), Some(&FeatureStatus::Ok));
    assert_eq!(layout.doc.placement(second), Some(joined));
    // The frame is kept in the design file.
    let saved: Value = serde_json::from_str(&layout.doc.to_json()).unwrap();
    assert!(
        saved["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["link"]["frames"].as_array().is_some_and(|f| f.len() == 1)),
        "{saved}"
    );

    // In full again: the faces are there, the frame no longer kept.
    layout.show(first, json!({"envelope": false}));
    let link = layout
        .doc
        .assembly()
        .component(linked)
        .and_then(|c| c.link.clone())
        .unwrap();
    assert!(link.frames.is_empty());
    assert_eq!(layout.doc.status(joint), Some(&FeatureStatus::Ok));
    assert_eq!(layout.doc.placement(second), Some(joined));
}
