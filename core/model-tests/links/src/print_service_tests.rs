// SPDX-License-Identifier: MIT
//! Components not for print services (mitcad#2214), with the mock kernel
//! and a resolver in memory: the library's flag recorded on an inserted
//! part (linked or a copy) and kept when the design is saved and opened
//! without the library, taken anew when the part is updated, kept by a
//! component saved as a design; the `print_service_check` query per body
//! (the design's own bodies and other components printable, only the
//! flagged component's bodies not, at every occurrence, nested too), for
//! chosen bodies and components; the component's properties row.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use mitcad_model::file::MemoryStore;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::library::{LibraryRequest, LibrarySource, LinkResolver};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, InsertOptions, LibraryChange, LibraryPart};
use mitcad_model_api::DocumentApi;

const V1: &str = "1111111111111111111111111111111111111111";
const V2: &str = "2222222222222222222222222222222222222222";
const URL: &str = "https://example.org/handles.git";

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
}

/// A supplier's library of blocks (`handle`, `knob`); the components
/// in `flagged` are not for print services, at every version.
#[derive(Default)]
struct Shop {
    flagged: Mutex<BTreeSet<String>>,
}

impl Shop {
    fn new(flagged: &[&str]) -> Arc<Self> {
        let shop = Self::default();
        shop.flag(flagged);
        Arc::new(shop)
    }

    fn flag(&self, flagged: &[&str]) {
        *self.flagged.lock().unwrap() = flagged.iter().map(|s| (*s).to_owned()).collect();
    }
}

impl LinkResolver for Shop {
    fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
        let rev = match request.rev.as_str() {
            "v1.0.0" => V1,
            "v1.1.0" => V2,
            other => other,
        };
        let mut name = request.component.clone();
        if let Some(first) = name.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        Ok(LibrarySource {
            text: mitcad_model::test_support::block().doc.to_json(),
            store: MemoryStore::new(),
            rev: rev.to_owned(),
            label: if rev == V1 { "v1.0.0" } else { "v1.1.0" }.to_owned(),
            library: "test-handles".to_owned(),
            url: URL.to_owned(),
            library_name: "Test handles".to_owned(),
            component: request.component.clone(),
            authors: vec!["Handle makers".to_owned()],
            path: format!("handles/{}.mitcad", request.component),
            name,
            standard: String::new(),
            license: "CC-BY-4.0".to_owned(),
            designation: String::new(),
            supplier: None,
            part_number: None,
            supplier_url: None,
            build: None,
            rows: Default::default(),
            non_printable: self.flagged.lock().unwrap().contains(&request.component),
        })
    }
}

fn part(component: &str) -> LibraryPart {
    LibraryPart {
        library: "test-handles".to_owned(),
        url: URL.to_owned(),
        rev: "v1.0.0".to_owned(),
        component: component.to_owned(),
        config: None,
        standard: String::new(),
    }
}

fn insert(doc: &mut Document<MockKernel>, component: &str, link: bool) -> ComponentUid {
    let options = InsertOptions {
        link,
        ..InsertOptions::default()
    };
    doc.insert_library_component(&part(component), &options)
        .unwrap()
        .0
}

fn flagged(doc: &Document<MockKernel>, component: ComponentUid) -> bool {
    doc.assembly()
        .component(component)
        .unwrap()
        .library
        .as_ref()
        .unwrap()
        .non_printable
}

fn body(doc: &Document<MockKernel>, component: ComponentUid) -> BodyUid {
    let bodies = doc.component_bodies(component);
    assert_eq!(bodies.len(), 1, "{bodies:?}");
    bodies[0]
}

fn check(doc: &Document<MockKernel>, fields: Value) -> Value {
    let mut asked = fields;
    asked["query"] = json!("print_service_check");
    query(doc, asked)
}

/// The `Library/Print service` row of a component's properties.
fn print_row(doc: &Document<MockKernel>, component: ComponentUid) -> Option<Value> {
    let levels = query(
        doc,
        json!({"query": "property_sets", "component": component.to_string()}),
    )["levels"]
        .clone();
    levels[0]["sets"]
        .as_array()?
        .iter()
        .find(|s| s["name"] == "Library")?["properties"]
        .as_array()?
        .iter()
        .find(|p| p["key"] == "Print service")
        .cloned()
}

#[test]
fn the_flag_is_recorded_kept_without_the_library_and_updated() {
    let shop = Shop::new(&["handle"]);
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(shop.clone()));
    let handle = insert(&mut doc, "handle", true);
    let knob = insert(&mut doc, "knob", true);
    assert!(flagged(&doc, handle));
    assert!(!flagged(&doc, knob));
    // The library_parts query gives the record with it.
    let parts = query(&doc, json!({"query": "library_parts"}))["parts"].clone();
    let record = |c: ComponentUid| {
        parts
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["component"] == json!(c))
            .unwrap()["library"]
            .clone()
    };
    assert_eq!(record(handle)["non_printable"], true, "{parts}");
    assert_eq!(record(knob).get("non_printable"), None, "{parts}");
    // The component's properties say so; nothing for a printable one.
    assert_eq!(
        print_row(&doc, handle).unwrap()["text"],
        "Not for print services"
    );
    assert_eq!(print_row(&doc, knob), None);

    // Saved, and opened on a computer without the library: the record
    // holds, and the check still leaves the handle out.
    let saved = doc.to_json();
    assert_eq!(
        saved.matches("\"non_printable\": true").count(),
        1,
        "{saved}"
    );
    let mut elsewhere = Document::from_json(&saved, MockKernel::default()).unwrap();
    let messages = elsewhere.update_links(None).unwrap();
    assert!(
        messages[0].contains("no component libraries are set up here"),
        "{messages:?}"
    );
    elsewhere.recompute();
    assert!(flagged(&elsewhere, handle));
    assert!(!flagged(&elsewhere, knob));
    let answer = check(&elsewhere, json!({}));
    assert_eq!(answer["not_printable"], json!([body(&elsewhere, handle)]));
    assert_eq!(answer["printable"], json!([body(&elsewhere, knob)]));
    assert_eq!(
        print_row(&elsewhere, handle).unwrap()["text"],
        "Not for print services"
    );

    // A newer version that flags the knob and allows the handle: an update
    // takes each part's flag of that version.
    shop.flag(&["knob"]);
    doc.update_library_parts(&[
        LibraryChange {
            component: knob,
            rev: Some(V2.to_owned()),
            config: None,
        },
        LibraryChange {
            component: handle,
            rev: Some(V2.to_owned()),
            config: None,
        },
    ])
    .unwrap();
    assert!(flagged(&doc, knob));
    assert!(!flagged(&doc, handle));
    doc.undo().unwrap();
    assert!(flagged(&doc, handle));
    assert!(!flagged(&doc, knob));
}

#[test]
fn copies_and_saved_designs_keep_the_flag() {
    // An editable copy keeps the record, as it keeps the licence.
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(Shop::new(&["handle"])));
    let copy = insert(&mut doc, "handle", false);
    assert!(flagged(&doc, copy));
    doc.recompute();
    let answer = check(&doc, json!({}));
    assert_eq!(
        answer["not_printable"],
        json!([body(&doc, copy)]),
        "{answer}"
    );
    let reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert!(flagged(&reopened, copy));

    // A drawer holding a linked handle, saved as a design of its own: the
    // handle in it keeps the flag, and the saved design leaves it out.
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(Shop::new(&["handle"])));
    let drawer = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Drawer"}),
    );
    let handle = insert(&mut doc, "handle", true);
    let knob = insert(&mut doc, "knob", true);
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    let dir = mitcad_model::testing::unique_temp_dir("2214-saved");
    let path = dir.join("Drawer.mitcad");
    command(
        &mut doc,
        json!({"cmd": "save_component_as_design", "component": drawer["component"],
               "path": path, "base": dir}),
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let mut opened = Document::from_json(&text, MockKernel::default()).unwrap();
    opened.recompute();
    let find = |name: &str| {
        opened
            .assembly()
            .components
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no {name}"))
            .uid
    };
    let (handle_there, knob_there) = (find("Handle"), find("Knob"));
    assert_ne!(handle_there, ComponentUid::ROOT);
    assert!(flagged(&opened, handle_there));
    assert!(!flagged(&opened, knob_there));
    let answer = check(&opened, json!({}));
    assert_eq!(
        answer["not_printable"],
        json!([body(&opened, handle_there)]),
        "{answer}"
    );
    assert_eq!(answer["printable"], json!([body(&opened, knob_there)]));
    // The design it came from is unchanged.
    assert!(flagged(&doc, handle) && !flagged(&doc, knob));
}

#[test]
fn the_check_answers_per_body_and_place() {
    // The design's own block, a drawer with a handle and a knob placed
    // twice.
    let mut doc = mitcad_model::test_support::block().doc;
    doc.set_link_resolver(Some(Shop::new(&["handle"])));
    doc.recompute();
    let own = doc.component_bodies(ComponentUid::ROOT)[0];
    let drawer = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Drawer"}),
    );
    let handle = insert(&mut doc, "handle", true);
    let knob = insert(&mut doc, "knob", true);
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": drawer["occurrence"]}),
    );
    doc.recompute();
    let (handle_body, knob_body) = (body(&doc, handle), body(&doc, knob));

    // Every body: only the handle's is not printable, at both drawers.
    let answer = check(&doc, json!({}));
    assert_eq!(answer["printable"], json!([own, knob_body]), "{answer}");
    assert_eq!(answer["not_printable"], json!([handle_body]), "{answer}");
    let entry = |answer: &Value, uid: BodyUid| {
        answer["bodies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["body"] == json!(uid))
            .unwrap()
            .clone()
    };
    let handle_entry = entry(&answer, handle_body);
    assert_eq!(handle_entry["printable"], false);
    assert_eq!(handle_entry["component_name"], "Handle");
    let places = handle_entry["places"].as_array().unwrap();
    assert_eq!(places.len(), 2, "{handle_entry}");
    let names: Vec<&str> = places
        .iter()
        .map(|p| p["path_name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Drawer:1/Handle:1", "Drawer:2/Handle:1"]);
    for place in places {
        assert_eq!(place["printable"], false);
        assert_eq!(place["component"], json!(handle));
        assert_eq!(place["library"], "test-handles");
    }
    let knob_entry = entry(&answer, knob_body);
    assert_eq!(knob_entry["printable"], true);
    assert_eq!(knob_entry["places"].as_array().unwrap().len(), 2);
    assert_eq!(entry(&answer, own)["printable"], true);
    assert_eq!(entry(&answer, own)["places"][0].get("path"), None);
    // Each placement of the part once, in words, without ids.
    let parts = answer["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2, "{answer}");
    assert_eq!(parts[0]["occurrence"], "Drawer:1/Handle:1");
    assert_eq!(parts[0]["license"], "CC-BY-4.0");
    assert_eq!(
        parts[0]["message"],
        "Handle at Drawer:1/Handle:1 from the library test-handles: not for print services"
    );
    let message = answer["message"].as_str().unwrap();
    assert!(
        message.starts_with(
            "1 body of 3 is not for print services and is left out: Handle at Drawer:1/Handle:1 \
             from the library test-handles: not for print services; Handle at Drawer:2/Handle:1 "
        ),
        "{message}"
    );
    assert!(
        message.ends_with("The other 2 may be sent to a print service."),
        "{message}"
    );
    for id in [own.to_string(), handle_body.to_string(), handle.to_string()] {
        assert!(!message.contains(&id), "{message}");
    }
    assert!(!message.contains("O1"), "{message}");

    // A chosen body: only that body.
    let answer = check(&doc, json!({"bodies": [own]}));
    assert_eq!(answer["printable"], json!([own]));
    assert_eq!(answer["not_printable"], json!([]));
    assert_eq!(
        answer["message"],
        "The body may be sent to a print service."
    );
    let answer = check(&doc, json!({"bodies": [handle_body]}));
    assert_eq!(answer["printable"], json!([]));
    assert_eq!(answer["not_printable"], json!([handle_body]));
    // A body that is not printable at either drawer: both places named.
    assert!(
        answer["message"]
            .as_str()
            .unwrap()
            .starts_with("The body may not be sent to a print service: Handle at Drawer:1/"),
        "{answer}"
    );
    // A chosen component: its bodies with its sub-components'.
    let answer = check(&doc, json!({"components": ["Drawer"]}));
    assert_eq!(answer["printable"], json!([knob_body]), "{answer}");
    assert_eq!(answer["not_printable"], json!([handle_body]));
    assert_eq!(answer["components"][0]["name"], "Drawer");
    // One drawer by its occurrence: one place of the handle.
    let answer = check(
        &doc,
        json!({"components": [{"component": "Drawer", "occurrence": "Drawer:2"}]}),
    );
    let places = entry(&answer, handle_body)["places"].clone();
    assert_eq!(places.as_array().unwrap().len(), 1, "{answer}");
    assert_eq!(places[0]["path_name"], "Drawer:2/Handle:1");

    // A design without such parts: every body may go.
    let mut plain = mitcad_model::test_support::block().doc;
    plain.recompute();
    let answer = check(&plain, json!({}));
    assert_eq!(answer["not_printable"], json!([]));
    assert_eq!(
        answer["message"],
        "The body may be sent to a print service."
    );
}
