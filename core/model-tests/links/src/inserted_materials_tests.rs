// SPDX-License-Identifier: MIT
//! The materials of inserted components (mitcad#1921) with the mock kernel:
//! a design of two blocks, one made of a material of the design's own
//! with an appearance of its own, the other of the design's default
//! material, one brought from a material library (its `reference`), is
//! inserted from a file and from a library, linked and as a copy. Each
//! inserted body is made of its source's material, and its mass follows
//! from that material's density; a linked one follows a later change of
//! its source (`update_links`, another version of a library part), unless
//! its material was set on it by hand or by a material binding. A body
//! whose source names no material has none of its own: the design's
//! default, where the design names one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::file::MemoryStore;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::library::{LibraryRequest, LibrarySource, LinkResolver};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FileFormat, InsertOptions, LibraryChange, LibraryPart};
use mitcad_model_api::DocumentApi;

const V1: &str = "1111111111111111111111111111111111111111";
const V2: &str = "2222222222222222222222222222222222222222";

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = doc.command(&command.to_string()).unwrap();
    serde_json::from_str(&text).unwrap()
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

/// The bodies of a component, in the order of their ids.
fn bodies(doc: &Document<MockKernel>, component: ComponentUid) -> Vec<BodyUid> {
    let mut bodies = doc.component_bodies(component);
    bodies.sort();
    bodies
}

/// The source design: two blocks; the first made of `own` (a material of
/// the design's own, `Test oak` at 0.71 g/cm³ or `Test ash` at
/// 0.67 g/cm³) and shown in the design's own appearance `Test red`, the
/// second of the design's default material `Test birch` (0.63 g/cm³), a
/// material library's.
fn source(own: &str) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    block(&mut doc, 0.0);
    block(&mut doc, 20.0);
    command(
        &mut doc,
        json!({"cmd": "create_material", "id": "test_oak", "name": "Test oak", "density": 0.71}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_material", "id": "test_ash", "name": "Test ash", "density": 0.67}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_material", "id": "test_birch", "name": "Test birch",
               "density": 0.63,
               "reference": {"source": "Test material library", "material": "Birch"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "test_red", "name": "Test red",
               "base_color": [0.8, 0.1, 0.1]}),
    );
    let blocks = bodies(&doc, ComponentUid::ROOT);
    assert_eq!(blocks.len(), 2, "{blocks:?}");
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": blocks[0].to_string(), "material": own}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": blocks[0].to_string(),
               "appearance": "test_red"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "C0", "material": "test_birch"}),
    );
    doc
}

/// A design to insert into, whose bodies the mock kernel measures.
fn holder() -> Document<MockKernel> {
    let doc = Document::new(MockKernel::default());
    doc.kernel().physical.set(true);
    doc
}

/// The name and density of what `body` is made of, and its mass and
/// volume, as the `properties` query gives them; the name None for no
/// material.
fn made_of(doc: &Document<MockKernel>, body: BodyUid) -> (Option<String>, f64, f64) {
    let answer = query(doc, json!({"query": "properties"}));
    let entry = answer["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["uid"] == body.to_string())
        .unwrap_or_else(|| panic!("no {body} in {answer}"))
        .clone();
    let volume = entry["volume"].as_f64().unwrap();
    assert!(volume > 0.0, "{entry}");
    let Some(id) = entry["material"].as_str() else {
        assert_eq!(entry.get("mass"), None, "{entry}");
        return (None, 0.0, 0.0);
    };
    let name = doc.material(id).unwrap().name.clone();
    let density = entry["density"].as_f64().unwrap();
    let mass = entry["mass"].as_f64().unwrap();
    // g/cm³ × mm³: grams / 1000; the mass is in kg.
    let expected = density * volume / 1e6;
    assert!(
        (mass - expected).abs() <= 1e-9 * expected.max(1.0),
        "{entry}"
    );
    (Some(name), density, mass)
}

/// Checks that the two inserted bodies are made of `first` and `Test
/// birch`, the first shown in `Test red`.
fn check_bodies(doc: &Document<MockKernel>, inserted: &[BodyUid], first: (&str, f64)) {
    assert_eq!(inserted.len(), 2, "{inserted:?}");
    let (name, density, _) = made_of(doc, inserted[0]);
    assert_eq!(name.as_deref(), Some(first.0));
    assert!((density - first.1).abs() < 1e-12, "{density}");
    let look = doc
        .body_look(inserted[0], None)
        .and_then(|id| doc.appearance(id))
        .map(|a| a.name.clone());
    assert_eq!(look.as_deref(), Some("Test red"));
    let (name, density, _) = made_of(doc, inserted[1]);
    assert_eq!(name.as_deref(), Some("Test birch"));
    assert!((density - 0.63).abs() < 1e-12, "{density}");
}

/// A folder with the source design saved as `parts/block.mitcad`.
fn saved_source(name: &str) -> PathBuf {
    let dir = mitcad_model::testing::unique_temp_dir(name);
    std::fs::create_dir_all(dir.join("parts")).unwrap();
    save_source(&dir, "test_oak");
    dir
}

fn save_source(dir: &Path, own: &str) {
    source(own)
        .save_file(&dir.join("parts").join("block.mitcad"), FileFormat::Auto)
        .unwrap();
}

fn from(dir: &Path, link: bool) -> InsertOptions {
    InsertOptions {
        link,
        base: Some(dir.to_path_buf()),
        ..InsertOptions::default()
    }
}

#[test]
fn a_linked_file_brings_its_materials_and_follows_them() {
    let dir = saved_source("inserted-materials-linked");
    let mut doc = holder();
    let (first, _) = doc
        .insert_component("parts/block.mitcad", &from(&dir, true))
        .unwrap();
    let first = bodies(&doc, first);
    check_bodies(&doc, &first, ("Test oak", 0.71));
    // The source's own materials and appearance are copied in once: a
    // second and third insert find them again.
    let (second, _) = doc
        .insert_component("parts/block.mitcad", &from(&dir, true))
        .unwrap();
    let (third, _) = doc
        .insert_component("parts/block.mitcad", &from(&dir, true))
        .unwrap();
    let second = bodies(&doc, second);
    let third = bodies(&doc, third);
    check_bodies(&doc, &second, ("Test oak", 0.71));
    let names: Vec<&str> = doc
        .custom_materials()
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, ["Test oak", "Test birch"]);
    // The library's material keeps its reference.
    let birch = doc
        .custom_materials()
        .iter()
        .find(|m| m.name == "Test birch")
        .unwrap();
    assert_eq!(
        birch.reference.as_ref().map(|r| r.source.as_str()),
        Some("Test material library")
    );
    let reds = query(&doc, json!({"query": "appearances"}));
    assert_eq!(
        reds.to_string().matches("\"Test red").count(),
        1,
        "the appearance is copied once: {reds}"
    );
    // A material set by hand, and one bound to an expression, stay.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": second[0].to_string(),
               "material": "aluminum"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": third[0].to_string(),
               "expression": "'aluminum'"}),
    );

    // The source's first block is made of ash now: the link follows.
    save_source(&dir, "test_ash");
    let messages = doc.update_links(Some(&dir)).unwrap();
    assert_eq!(messages.len(), 3, "{messages:?}");
    check_bodies(&doc, &first, ("Test ash", 0.67));
    let (name, ..) = made_of(&doc, second[0]);
    assert_eq!(
        doc.body_attributes(second[0]).material.as_deref(),
        Some("aluminum")
    );
    assert_ne!(name.as_deref(), Some("Test ash"));
    assert_eq!(
        doc.body_attributes(third[0]).material.as_deref(),
        Some("aluminum")
    );
    assert!(doc.body_attributes(third[0]).material_expr.is_some());
    // Their other body follows the source as it did.
    let (name, ..) = made_of(&doc, second[1]);
    assert_eq!(name.as_deref(), Some("Test birch"));

    // Saved and opened again, the link keeps what the source gave it.
    let saved = dir.join("holder.mitcad");
    doc.save_file(&saved, FileFormat::Auto).unwrap();
    let text = std::fs::read_to_string(&saved).unwrap();
    let mut again = Document::from_json_at(&text, &saved, MockKernel::default()).unwrap();
    again.kernel().physical.set(true);
    let messages = again.update_links(Some(&dir)).unwrap();
    assert_eq!(messages, Vec::<String>::new());
    again.recompute();
    check_bodies(&again, &first, ("Test ash", 0.67));
}

#[test]
fn a_copy_of_a_file_brings_its_materials() {
    let dir = saved_source("inserted-materials-copy");
    let mut doc = holder();
    let (copy, _) = doc
        .insert_component("parts/block.mitcad", &from(&dir, false))
        .unwrap();
    check_bodies(&doc, &bodies(&doc, copy), ("Test oak", 0.71));
    // The copy's component takes the source's default material.
    let birch = doc.component_material(copy).unwrap().to_owned();
    assert_eq!(doc.material(&birch).unwrap().name, "Test birch");
}

#[test]
fn a_body_whose_source_names_no_material_takes_the_designs_default() {
    let dir = mitcad_model::testing::unique_temp_dir("inserted-materials-none");
    std::fs::create_dir_all(dir.join("parts")).unwrap();
    let mut plain = Document::new(MockKernel::default());
    block(&mut plain, 0.0);
    plain
        .save_file(&dir.join("parts").join("block.mitcad"), FileFormat::Auto)
        .unwrap();
    let mut doc = holder();
    let (linked, _) = doc
        .insert_component("parts/block.mitcad", &from(&dir, true))
        .unwrap();
    let body = bodies(&doc, linked)[0];
    assert_eq!(doc.body_attributes(body).material, None);
    // No material anywhere: none, and no mass (mitcad#1817).
    let (name, ..) = made_of(&doc, body);
    assert_eq!(name, None);
    // The design's default material, named as such.
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "C0", "material": "aluminum"}),
    );
    let (name, ..) = made_of(&doc, body);
    assert_eq!(
        name.as_deref(),
        Some(doc.material("aluminum").unwrap().name.as_str())
    );
    let (_, source) = doc.body_material(body, None);
    assert_eq!(
        source,
        mitcad_model::MaterialSource::Component(ComponentUid::ROOT)
    );
}

/// A library of the source design: at V1 its first block is oak, at V2
/// ash.
struct Library {
    versions: Mutex<BTreeMap<String, String>>,
}

impl Library {
    fn new() -> Arc<Self> {
        Self::of(source)
    }

    /// The library of the designs `make` makes, with oak at V1 and ash at
    /// V2.
    fn of(make: fn(&str) -> Document<MockKernel>) -> Arc<Self> {
        let versions = BTreeMap::from([
            (V1.to_owned(), make("test_oak").to_json()),
            (V2.to_owned(), make("test_ash").to_json()),
        ]);
        Arc::new(Self {
            versions: Mutex::new(versions),
        })
    }
}

impl LinkResolver for Library {
    fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
        let rev = match request.rev.as_str() {
            "v1.0.0" => V1,
            "v1.1.0" => V2,
            other => other,
        };
        let text = self
            .versions
            .lock()
            .unwrap()
            .get(rev)
            .cloned()
            .ok_or_else(|| format!("version {rev} is not in the library's copy here"))?;
        Ok(LibrarySource {
            text,
            store: MemoryStore::new(),
            rev: rev.to_owned(),
            label: if rev == V1 { "v1.0.0" } else { "v1.1.0" }.to_owned(),
            library: "test-blocks".to_owned(),
            url: "https://example.org/blocks.git".to_owned(),
            library_name: "Test blocks".to_owned(),
            component: request.component.clone(),
            authors: vec!["Block makers".to_owned()],
            path: "blocks/block.mitcad".to_owned(),
            name: "Two blocks".to_owned(),
            standard: String::new(),
            license: "CC0-1.0".to_owned(),
            designation: String::new(),
            supplier: None,
            part_number: None,
            supplier_url: None,
            build: None,
            rows: BTreeMap::new(),
            non_printable: false,
        })
    }
}

fn part(rev: &str) -> LibraryPart {
    LibraryPart {
        library: "test-blocks".to_owned(),
        url: "https://example.org/blocks.git".to_owned(),
        rev: rev.to_owned(),
        component: "block".to_owned(),
        config: None,
        standard: String::new(),
    }
}

#[test]
fn a_library_part_brings_its_materials_linked_and_as_a_copy() {
    let mut doc = holder();
    doc.set_link_resolver(Some(Library::new()));
    let (linked, _) = doc
        .insert_library_component(&part("v1.0.0"), &InsertOptions::default())
        .unwrap();
    let linked_bodies = bodies(&doc, linked);
    check_bodies(&doc, &linked_bodies, ("Test oak", 0.71));
    let copy_options = InsertOptions {
        link: false,
        ..InsertOptions::default()
    };
    let (copy, _) = doc
        .insert_library_component(&part("v1.0.0"), &copy_options)
        .unwrap();
    check_bodies(&doc, &bodies(&doc, copy), ("Test oak", 0.71));

    // Another version, whose first block is ash: the linked part follows.
    doc.update_library_parts(&[LibraryChange {
        component: linked,
        rev: Some("v1.1.0".to_owned()),
        config: None,
    }])
    .unwrap();
    check_bodies(&doc, &linked_bodies, ("Test ash", 0.67));
    // Undone, it is oak again.
    doc.undo().unwrap();
    check_bodies(&doc, &linked_bodies, ("Test oak", 0.71));
}

// Components inserted with options (mitcad#1923): a derived part makes
// their bodies from the source at evaluation time; they take the source's
// materials as a linked design's bodies do. And links made before
// mitcad#1921 take them once on their next `update_links`.

/// The source design with the user parameter Depth (5 mm), so that it can
/// be inserted with options.
fn options_source(own: &str) -> Document<MockKernel> {
    let mut doc = source(own);
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Depth", "expression": "5 mm"}),
    );
    doc
}

/// Linked with the option Depth = 8 mm.
fn with_options(base: Option<&Path>) -> InsertOptions {
    InsertOptions {
        base: base.map(Path::to_path_buf),
        parameters: Some(BTreeMap::from([(
            "Depth".to_owned(),
            mitcad_model::ValueInput::Name("8 mm".to_owned()),
        )])),
        ..InsertOptions::default()
    }
}

#[test]
fn a_library_part_with_options_brings_its_materials_and_follows_them() {
    let mut doc = holder();
    doc.set_link_resolver(Some(Library::of(options_source)));
    let (first, _) = doc
        .insert_library_component(&part("v1.0.0"), &with_options(None))
        .unwrap();
    assert!(doc.component_options(first).is_some());
    let first = bodies(&doc, first);
    check_bodies(&doc, &first, ("Test oak", 0.71));
    // A second one finds the materials and the appearance brought in.
    let (second_uid, _) = doc
        .insert_library_component(&part("v1.0.0"), &with_options(None))
        .unwrap();
    let second = bodies(&doc, second_uid);
    check_bodies(&doc, &second, ("Test oak", 0.71));
    let names: Vec<&str> = doc
        .custom_materials()
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, ["Test oak", "Test birch"]);
    let reds = query(&doc, json!({"query": "appearances"}));
    assert_eq!(reds.to_string().matches("\"Test red").count(), 1, "{reds}");
    // A copy has the design's own parameters: options are refused, and a
    // copy brings its materials (mitcad#1921).
    let copy = InsertOptions {
        link: false,
        ..with_options(None)
    };
    assert!(
        doc.insert_library_component(&part("v1.0.0"), &copy)
            .is_err()
    );
    let (copied, _) = doc
        .insert_library_component(
            &part("v1.0.0"),
            &InsertOptions {
                link: false,
                ..InsertOptions::default()
            },
        )
        .unwrap();
    check_bodies(&doc, &bodies(&doc, copied), ("Test oak", 0.71));
    // A material set by hand stays.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": second[0].to_string(),
               "material": "aluminum"}),
    );

    // Another version, whose first block is ash: the parts follow it.
    let linked: Vec<ComponentUid> = doc
        .assembly()
        .components
        .iter()
        .filter(|c| doc.component_options(c.uid).is_some())
        .map(|c| c.uid)
        .collect();
    assert_eq!(linked.len(), 2);
    let changes: Vec<LibraryChange> = linked
        .iter()
        .map(|c| LibraryChange {
            component: *c,
            rev: Some("v1.1.0".to_owned()),
            config: None,
        })
        .collect();
    doc.update_library_parts(&changes).unwrap();
    check_bodies(&doc, &first, ("Test ash", 0.67));
    assert_eq!(
        doc.body_attributes(second[0]).material.as_deref(),
        Some("aluminum")
    );
    let (name, ..) = made_of(&doc, second[1]);
    assert_eq!(name.as_deref(), Some("Test birch"));
    // Undone, it is oak again.
    doc.undo().unwrap();
    check_bodies(&doc, &first, ("Test oak", 0.71));
}

#[test]
fn a_linked_file_with_options_brings_its_materials_and_follows_them() {
    let dir = mitcad_model::testing::unique_temp_dir("inserted-materials-options");
    std::fs::create_dir_all(dir.join("parts")).unwrap();
    let path = dir.join("parts").join("block.mitcad");
    options_source("test_oak")
        .save_file(&path, FileFormat::Auto)
        .unwrap();
    let mut doc = holder();
    let (linked, _) = doc
        .insert_component("parts/block.mitcad", &with_options(Some(&dir)))
        .unwrap();
    assert!(doc.component_options(linked).is_some());
    let inserted = bodies(&doc, linked);
    check_bodies(&doc, &inserted, ("Test oak", 0.71));

    // The file's first block is ash now: the part follows.
    options_source("test_ash")
        .save_file(&path, FileFormat::Auto)
        .unwrap();
    let messages = doc.update_links(Some(&dir)).unwrap();
    assert_eq!(messages.len(), 1, "{messages:?}");
    check_bodies(&doc, &inserted, ("Test ash", 0.67));
    // Read again with nothing changed: nothing more to do.
    let messages = doc.update_links(Some(&dir)).unwrap();
    assert_eq!(messages, Vec::<String>::new());
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_design_saved_before_its_links_took_materials_gets_them_once() {
    let dir = mitcad_model::testing::unique_temp_dir("inserted-materials-before");
    std::fs::create_dir_all(dir.join("parts")).unwrap();
    std::fs::create_dir_all(dir.join("options")).unwrap();
    save_source(&dir, "test_oak");
    options_source("test_oak")
        .save_file(&dir.join("options").join("block.mitcad"), FileFormat::Auto)
        .unwrap();
    let library = Library::of(options_source);
    let mut doc = holder();
    doc.set_link_resolver(Some(library.clone()));
    // A linked file, one with options, a library part and one with
    // options.
    let components = [
        doc.insert_component("parts/block.mitcad", &from(&dir, true))
            .unwrap()
            .0,
        doc.insert_component("options/block.mitcad", &with_options(Some(&dir)))
            .unwrap()
            .0,
        doc.insert_library_component(&part("v1.0.0"), &InsertOptions::default())
            .unwrap()
            .0,
        doc.insert_library_component(&part("v1.0.0"), &with_options(None))
            .unwrap()
            .0,
    ];
    let inserted: Vec<Vec<BodyUid>> = components.iter().map(|c| bodies(&doc, *c)).collect();
    for bodies in &inserted {
        check_bodies(&doc, bodies, ("Test oak", 0.71));
    }

    // The same design as one saved before its links brought materials:
    // its links did not take them, nor record them, and it has none of
    // its own; one body has a material set by hand.
    let mut state = doc.state().clone();
    for c in &mut state.assembly.components {
        if let Some(link) = c.link.as_mut() {
            link.looks = false;
            link.given.clear();
        }
    }
    for f in &mut state.features {
        if let FeatureDef::Base(base) = &mut Arc::make_mut(f).def {
            for body in &mut base.bodies {
                body.material = None;
                body.appearance = None;
            }
        }
    }
    for attributes in state.body_attributes.values_mut() {
        attributes.material = None;
        attributes.appearance = None;
    }
    state.materials.clear();
    state.appearances.clear();
    let by_hand = inserted[1][1];
    state.body_attributes.entry(by_hand).or_default().material = Some("aluminum".to_owned());
    let mut old = Document::from_state(MockKernel::default(), state);
    old.kernel().physical.set(true);
    old.set_link_resolver(Some(library));
    old.recompute();
    let (name, ..) = made_of(&old, inserted[0][0]);
    assert_eq!(name, None);

    // Its next update gives them, though no source changed.
    let messages = old.update_links(Some(&dir)).unwrap();
    assert_eq!(messages.len(), 4, "{messages:?}");
    assert!(
        messages.iter().all(|m| m.contains("takes the materials")),
        "{messages:?}"
    );
    for (i, bodies) in inserted.iter().enumerate() {
        if i == 1 {
            let (name, density, _) = made_of(&old, bodies[0]);
            assert_eq!(name.as_deref(), Some("Test oak"));
            assert!((density - 0.71).abs() < 1e-12);
            assert_eq!(
                old.body_attributes(by_hand).material.as_deref(),
                Some("aluminum")
            );
        } else {
            check_bodies(&old, bodies, ("Test oak", 0.71));
        }
    }
    // Once: the next update has nothing to do.
    let messages = old.update_links(Some(&dir)).unwrap();
    assert_eq!(messages, Vec::<String>::new());
    mitcad_model::testing::remove_temp(&dir);
}
