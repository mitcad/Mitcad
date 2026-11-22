// SPDX-License-Identifier: MIT
//! A BUILD part of a library source not for print services (mitcad#2214)
//! with the mock kernel: the source's record in `library_sources` keeps
//! the flag, also when the design opens without the library; the part's
//! bodies are left out by `print_service_check`, another part of a
//! printable source stays printable, and the part's properties say so.

use std::sync::Arc;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::ComponentUid;
use mitcad_model::library::{LibraryRequest, LibrarySource, LinkResolver};
use mitcad_model::testing::MockKernel;
use mitcad_model_tests_support::test_library::*;

/// The test library with its `p30` not for print services.
struct NoPrint(Arc<TestLibrary>);

impl LinkResolver for NoPrint {
    fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
        let mut source = self.0.library_source(request)?;
        source.non_printable = request.component == "p30";
        Ok(source)
    }
}

fn check(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "print_service_check"}))
}

fn bodies(doc: &Document<MockKernel>, component: ComponentUid) -> Value {
    json!(doc.component_bodies(component))
}

fn print_row(doc: &Document<MockKernel>, component: ComponentUid) -> Value {
    let levels = query(
        doc,
        json!({"query": "property_sets", "component": component.to_string()}),
    )["levels"]
        .clone();
    levels[0]["sets"]
        .as_array()
        .and_then(|sets| sets.iter().find(|s| s["name"] == "Library"))
        .and_then(|set| set["properties"].as_array())
        .and_then(|rows| rows.iter().find(|p| p["key"] == "Print service"))
        .map_or(Value::Null, |row| row["text"].clone())
}

#[test]
fn a_build_part_of_a_non_printable_source_is_left_out() {
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(Arc::new(NoPrint(TestLibrary::new()))));
    let p30 = component(&first(&mut doc, "p30", [0.0; 3], 500.0));
    let p20 = component(&first(&mut doc, "p20", [200.0, 0.0, 0.0], 300.0));
    doc.recompute();
    all_ok(&doc);

    let answer = check(&doc);
    assert_eq!(answer["not_printable"], bodies(&doc, p30), "{answer}");
    assert_eq!(answer["printable"], bodies(&doc, p20), "{answer}");
    assert_eq!(answer["parts"][0]["library"], LIBRARY);
    assert!(
        answer["parts"][0]["message"]
            .as_str()
            .unwrap()
            .ends_with(&format!(
                "from the library {LIBRARY}: not for print services"
            )),
        "{answer}"
    );
    assert_eq!(print_row(&doc, p30), "Not for print services");
    assert_eq!(print_row(&doc, p20), Value::Null);

    // The source's record keeps it in the file; opened without the
    // library, the part is still left out.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let sources = file["library_sources"].as_array().unwrap();
    let flags: Vec<(String, bool)> = sources
        .iter()
        .map(|s| {
            (
                s["library"]["component"].as_str().unwrap().to_owned(),
                s["library"]["non_printable"].as_bool().unwrap_or(false),
            )
        })
        .collect();
    assert!(flags.contains(&("p30".to_owned(), true)), "{flags:?}");
    assert!(flags.contains(&("p20".to_owned(), false)), "{flags:?}");
    let mut elsewhere = Document::from_json(&saved, MockKernel::default()).unwrap();
    elsewhere.recompute();
    let answer = check(&elsewhere);
    assert_eq!(answer["not_printable"], bodies(&elsewhere, p30), "{answer}");
    assert_eq!(answer["printable"], bodies(&elsewhere, p20), "{answer}");
}
