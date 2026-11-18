// SPDX-License-Identifier: MIT
//! A BUILD part's Kind (mitcad#1980) and BOM Structure (mitcad#1985), with
//! BUILD's test library: a profile cut to length is a Build profile,
//! Normal; a cap (a library part not cut to length) is a Library part,
//! Purchased; the user's choice overrides either. The other kinds are
//! tested on sheet metal and plain solids (`sheet/src/bom_kind_tests.rs`).

use serde_json::{Value, json};

use mitcad_model::{BomStructure, ComponentKind};
use mitcad_model_tests_support::test_library::*;

fn parts_row(
    doc: &mitcad_model::Document<mitcad_model::testing::MockKernel>,
    component: &Value,
) -> Value {
    query(
        doc,
        json!({"query": "parts_list", "properties": ["Kind", "BOM Structure"]}),
    )["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["component"] == *component)
        .cloned()
        .unwrap_or_else(|| panic!("no row of {component}"))
}

#[test]
fn a_build_profile_is_normal_and_a_library_part_purchased() {
    let mut doc = design();
    let profile = first(&mut doc, "p30", [0.0; 3], 600.0);
    let cap = place(
        &mut doc,
        json!({"part": part("cap30"), "at": {"point": [100, 0, 0]}}),
    );
    all_ok(&doc);
    let (p, c) = (component(&profile), component(&cap));
    assert_eq!(doc.component_kind(p, None), ComponentKind::BuildProfile);
    assert_eq!(doc.component_kind(c, None), ComponentKind::LibraryPart);
    assert_eq!(doc.bom_structure(p, None), BomStructure::Normal);
    assert_eq!(doc.bom_structure(c, None), BomStructure::Purchased);
    let row = parts_row(&doc, &profile["component"]);
    assert_eq!(row["kind"], "Build profile", "{row}");
    assert_eq!(row["bom_structure"], "Normal", "{row}");
    assert_eq!(row["properties"]["Kind"], "Build profile", "{row}");
    assert_eq!(row["properties"]["BOM Structure"], "Normal", "{row}");
    let row = parts_row(&doc, &cap["component"]);
    assert_eq!(row["kind"], "Library part", "{row}");
    assert_eq!(row["bom_structure"], "Purchased", "{row}");
    // The filters: the purchased parts, the profiles.
    let bought = query(
        &doc,
        json!({"query": "parts_list", "bom_structure": ["Purchased"]}),
    )["rows"]
        .clone();
    assert_eq!(bought.as_array().unwrap().len(), 1, "{bought}");
    assert_eq!(bought[0]["component"], cap["component"]);
    let profiles = query(
        &doc,
        json!({"query": "parts_list", "kind": ["Build profile"]}),
    )["rows"]
        .clone();
    assert_eq!(profiles.as_array().unwrap().len(), 1, "{profiles}");
    assert_eq!(profiles[0]["component"], profile["component"]);
    // The user makes the cap a Normal part after all: their choice over
    // the library's.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": cap["component"], "set": "Bill of Materials",
               "key": "BOM Structure", "value": "Normal"}),
    )
    .unwrap();
    assert_eq!(doc.bom_structure(c, None), BomStructure::Normal);
    assert_eq!(doc.bom_structure_default(c, None), BomStructure::Purchased);
    let row = parts_row(&doc, &cap["component"]);
    assert_eq!(row["bom_structure"], "Normal", "{row}");
    assert_eq!(row["kind"], "Library part", "{row}");
    let bought = query(
        &doc,
        json!({"query": "parts_list", "bom_structure": ["purchased"]}),
    )["rows"]
        .clone();
    assert_eq!(bought, json!([]), "{bought}");
}
