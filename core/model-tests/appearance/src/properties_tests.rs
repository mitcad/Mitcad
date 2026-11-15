// SPDX-License-Identifier: MIT
//! Typed property sets (mitcad#496) with the mock kernel: set, override
//! and remove on each level, own types with allowed values, units in the
//! design's units, imported values the user changed, computed values that
//! are never stored, the design file, the search and the parts list's
//! columns.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::properties::{BaseType, Property, PropertyOwner, PropertyValue, Source};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string()).unwrap_err().0
}

/// A design with a component `Wall` placed twice, its 10 x 10 x 5 block
/// body, and the shapes measured.
struct Design {
    doc: Document<MockKernel>,
    component: ComponentUid,
    first: OccurrenceUid,
    second: OccurrenceUid,
    body: BodyUid,
}

fn design() -> Design {
    let kernel = MockKernel::default();
    kernel.measures.set(true);
    let mut doc = Document::new(kernel);
    let (component, first) = doc
        .create_component(Some("Wall"), Transform::IDENTITY, true)
        .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let second = doc
        .copy_occurrence(first, Some(Transform::translation([0.0, 50.0, 0.0])))
        .unwrap();
    Design {
        doc,
        component,
        first,
        second,
        body: BodyUid::new(uid, 0),
    }
}

/// A property of a level of the `property_sets` answer, by set and key.
fn find<'a>(level: &'a Value, set: &str, key: &str) -> Option<&'a Value> {
    level["sets"]
        .as_array()?
        .iter()
        .find(|s| s["name"] == set)?["properties"]
        .as_array()?
        .iter()
        .find(|p| p["key"] == key)
}

fn levels(doc: &Document<MockKernel>, q: Value) -> Vec<Value> {
    let mut q = q;
    q["query"] = json!("property_sets");
    query(doc, q)["levels"].as_array().unwrap().clone()
}

/// The first level of the `property_sets` answer.
fn level(doc: &Document<MockKernel>, q: Value) -> Value {
    levels(doc, q).swap_remove(0)
}

#[test]
fn set_override_and_remove_on_each_level() {
    let mut d = design();
    let c = d.component.to_string();
    let depth = d.doc.undo_depth();
    let reply = command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": c, "set": "Identity", "key": "Mark",
               "value": "W-12"}),
    );
    // One undo step that recomputes nothing.
    assert_eq!(reply["recomputed"], 0);
    assert_eq!(reply["type"], "text");
    assert_eq!(d.doc.undo_depth(), depth + 1);

    // Both occurrences show the component's value as inherited.
    for occurrence in [d.first, d.second] {
        let levels = levels(&d.doc, json!({"occurrence": occurrence.to_string()}));
        assert_eq!(levels.len(), 2, "the occurrence and its component");
        assert_eq!(levels[0]["level"], "occurrence");
        assert_eq!(levels[1]["level"], "component");
        assert_eq!(levels[1]["name"], "Wall");
        let mark = find(&levels[0], "Identity", "Mark").unwrap();
        assert_eq!(mark["text"], "W-12");
        assert_eq!(mark["inherited"], true);
        assert_eq!(mark["source"], "user");
    }

    // The first occurrence overrides it; the second keeps the component's.
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": d.first.to_string(), "set": "identity",
               "key": "mark", "value": "W-13"}),
    );
    let first = &level(&d.doc, json!({"occurrence": d.first.to_string()}));
    let mark = find(first, "Identity", "Mark").unwrap();
    assert_eq!(mark["text"], "W-13");
    assert_eq!(mark["overrides"], true);
    assert_eq!(mark["inherited"], false);
    let second = &level(&d.doc, json!({"occurrence": d.second.to_string()}));
    assert_eq!(find(second, "Identity", "Mark").unwrap()["text"], "W-12");

    // A body's own property, with its component's level beside it.
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": d.body.to_string(), "set": "Pset_Common",
               "key": "Reference", "value": "R1"}),
    );
    let body_levels = levels(&d.doc, json!({"body": d.body.to_string()}));
    assert_eq!(body_levels[0]["level"], "body");
    assert_eq!(body_levels[1]["owner"], c);
    assert_eq!(
        find(&body_levels[0], "Pset_Common", "Reference").unwrap()["text"],
        "R1"
    );

    // Removing the override shows the component's value again.
    command(
        &mut d.doc,
        json!({"cmd": "remove_property", "owner": d.first.to_string(), "set": "Identity",
               "key": "Mark"}),
    );
    let first = &level(&d.doc, json!({"occurrence": d.first.to_string()}));
    let mark = find(first, "Identity", "Mark").unwrap();
    assert_eq!(mark["text"], "W-12");
    assert_eq!(mark["inherited"], true);
    // Removing the component's leaves no Identity set.
    command(
        &mut d.doc,
        json!({"cmd": "remove_property", "owner": c, "set": "Identity", "key": "Mark"}),
    );
    let first = &level(&d.doc, json!({"occurrence": d.first.to_string()}));
    assert!(find(first, "Identity", "Mark").is_none());
    let error = refused(
        &mut d.doc,
        json!({"cmd": "remove_property", "owner": c, "set": "Identity", "key": "Mark"}),
    );
    assert!(error.contains("has no property"), "{error}");

    // Each command was one step: undo brings the override back.
    command(&mut d.doc, json!({"cmd": "undo"}));
    command(&mut d.doc, json!({"cmd": "undo"}));
    let first = &level(&d.doc, json!({"occurrence": d.first.to_string()}));
    assert_eq!(find(first, "Identity", "Mark").unwrap()["text"], "W-13");
}

#[test]
fn owners_by_name_and_refused_owners_and_sets() {
    let mut d = design();
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "Wall:2", "set": "Identity", "key": "Mark",
               "value": "W-2"}),
    );
    assert_eq!(
        d.doc
            .property_store()
            .get(PropertyOwner::Occurrence(d.second), "Identity", "Mark")
            .unwrap()
            .value,
        PropertyValue::Text("W-2".to_owned())
    );
    let error = refused(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "C9", "set": "S", "key": "K", "value": 1}),
    );
    assert!(error.contains("does not exist"), "{error}");
    let error = refused(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "Wall", "set": "Computed", "key": "Mass",
               "value": 1}),
    );
    assert!(error.contains("computed values"), "{error}");
    let error = refused(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "Wall", "set": "S", "key": "a/b", "value": 1}),
    );
    assert!(error.contains("'/'"), "{error}");
    // The value's own type when none is given.
    let reply = command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "Wall", "set": "S", "key": "Count", "value": 3}),
    );
    assert_eq!(reply["type"], "integer");
    let reply = command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "Wall", "set": "S", "key": "Load",
               "value": "true", "type": "Yes/No"}),
    );
    assert_eq!(reply["type"], "boolean");
    assert_eq!(reply["value"], true);
    let error = refused(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "Wall", "set": "S", "key": "Built",
               "value": "2026-02-30", "type": "date"}),
    );
    assert!(error.contains("not a date"), "{error}");
}

#[test]
fn an_own_type_with_allowed_values_refuses_another() {
    let mut d = design();
    command(
        &mut d.doc,
        json!({"cmd": "define_property_type", "name": "FireRating", "base": "text",
               "values": ["EI30", "EI60", "EI90"], "default": "EI30",
               "description": "Fire resistance class"}),
    );
    let c = d.component.to_string();
    // Its default when no value is given; its spelling whatever the case.
    let reply = command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": c, "set": "Pset_WallCommon",
               "key": "FireRating", "type": "firerating"}),
    );
    assert_eq!(reply["type"], "FireRating");
    assert_eq!(reply["value"], "EI30");
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": c, "set": "Pset_WallCommon",
               "key": "FireRating", "value": "EI60"}),
    );
    let error = refused(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": c, "set": "Pset_WallCommon",
               "key": "FireRating", "value": "EI45"}),
    );
    assert!(
        error.contains("FireRating allows EI30, EI60, EI90; not EI45"),
        "{error}"
    );
    // The choices for the panel.
    let component = &level(&d.doc, json!({"component": c}));
    let rating = find(component, "Pset_WallCommon", "FireRating").unwrap();
    assert_eq!(rating["text"], "EI60");
    assert_eq!(rating["base"], "text");
    assert_eq!(rating["choices"], json!(["EI30", "EI60", "EI90"]));
    let types = query(&d.doc, json!({"query": "property_types"}));
    assert_eq!(types["types"][0]["name"], "FireRating");
    assert_eq!(types["types"][0]["uses"], 1);
    assert!(
        types["standard"]
            .as_array()
            .unwrap()
            .contains(&json!("length"))
    );

    // Values that would leave a property out are refused; a rename follows.
    let error = refused(
        &mut d.doc,
        json!({"cmd": "edit_property_type", "name": "FireRating", "values": ["EI30"],
               "default": null}),
    );
    assert!(error.contains("would not allow"), "{error}");
    command(
        &mut d.doc,
        json!({"cmd": "edit_property_type", "name": "FireRating", "new_name": "FireClass",
               "values": ["EI30", "EI60", "EI90", "EI120"]}),
    );
    let component = &level(&d.doc, json!({"component": c}));
    assert_eq!(
        find(component, "Pset_WallCommon", "FireRating").unwrap()["type"],
        "FireClass"
    );
    // A standard type's name is no own type's.
    let error = refused(
        &mut d.doc,
        json!({"cmd": "define_property_type", "name": "Length", "base": "text"}),
    );
    assert!(error.contains("standard type"), "{error}");
    // Removing the type keeps the values as text.
    let reply = command(
        &mut d.doc,
        json!({"cmd": "remove_property_type", "name": "FireClass"}),
    );
    assert_eq!(reply["converted"], 1);
    let component = &level(&d.doc, json!({"component": c}));
    let rating = find(component, "Pset_WallCommon", "FireRating").unwrap();
    assert_eq!(rating["type"], "text");
    assert_eq!(rating["text"], "EI60");
    assert!(rating.get("choices").is_none());
}

#[test]
fn quantities_convert_with_the_designs_units() {
    let mut d = design();
    let c = d.component.to_string();
    let set = |doc: &mut Document<MockKernel>, key: &str, ty: &str, value: Value| {
        command(
            doc,
            json!({"cmd": "set_property", "owner": "Wall", "set": "Dimensions", "key": key,
                   "type": ty, "value": value}),
        )
    };
    // Text with its own unit; a JSON number is canonical (mm, kg).
    set(&mut d.doc, "Height", "length", json!("1.2 m"));
    set(&mut d.doc, "Width", "length", json!(250.0));
    set(&mut d.doc, "Weight", "mass", json!("500 g"));
    set(&mut d.doc, "Face", "area", json!("2 m^2"));
    set(&mut d.doc, "Slope", "angle", json!("30 deg"));
    let store = d.doc.property_store();
    let owner = PropertyOwner::Component(d.component);
    assert_eq!(
        store.get(owner, "Dimensions", "Height").unwrap().value,
        PropertyValue::Number(1200.0)
    );
    assert_eq!(
        store.get(owner, "Dimensions", "Weight").unwrap().value,
        PropertyValue::Number(0.5)
    );
    let text = |doc: &Document<MockKernel>, key: &str| {
        let level = &level(doc, json!({"component": c}));
        find(level, "Dimensions", key).unwrap()["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(text(&d.doc, "Height"), "1200 mm");
    assert_eq!(text(&d.doc, "Weight"), "0.5 kg");
    assert_eq!(text(&d.doc, "Face"), "2000000 mm^2");
    assert_eq!(text(&d.doc, "Slope"), "30 deg");
    // In metres they show in metres, and a bare number is in metres.
    command(&mut d.doc, json!({"cmd": "set_units", "length": "m"}));
    assert_eq!(text(&d.doc, "Height"), "1.2 m");
    assert_eq!(text(&d.doc, "Width"), "0.25 m");
    assert_eq!(text(&d.doc, "Face"), "2 m^2");
    set(&mut d.doc, "Height", "length", json!("2,5"));
    assert_eq!(text(&d.doc, "Height"), "2.5 m");
    assert_eq!(
        d.doc
            .property_store()
            .get(owner, "Dimensions", "Height")
            .unwrap()
            .value,
        PropertyValue::Number(2500.0)
    );
    // A value of another dimension is refused.
    let error = refused(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "Wall", "set": "Dimensions", "key": "Height",
               "value": "30 deg"}),
    );
    assert!(error.contains("not a value of type length"), "{error}");
    // An own type with its unit shows in it.
    command(
        &mut d.doc,
        json!({"cmd": "define_property_type", "name": "Thickness", "base": "length",
               "unit": "mm"}),
    );
    set(&mut d.doc, "Thickness", "Thickness", json!("12"));
    assert_eq!(text(&d.doc, "Thickness"), "12 mm");
}

#[test]
fn an_imported_value_the_user_changes_shows_its_source() {
    let mut d = design();
    let owner = PropertyOwner::Body(d.body);
    let added = d.doc.import_properties(
        "Import properties",
        vec![
            (
                owner,
                "Identity".to_owned(),
                Property::new(
                    "Category",
                    BaseType::Text,
                    PropertyValue::Text("Walls".to_owned()),
                    Source::Rvt,
                ),
            ),
            // Not a value of its type: left out.
            (
                owner,
                "Identity".to_owned(),
                Property::new(
                    "Id",
                    BaseType::Integer,
                    PropertyValue::Text("x".to_owned()),
                    Source::Rvt,
                ),
            ),
        ],
    );
    assert_eq!(added, 1);
    let body = d.body.to_string();
    let shown = |doc: &Document<MockKernel>| {
        find(&level(doc, json!({"body": body})), "Identity", "Category")
            .unwrap()
            .clone()
    };
    assert_eq!(shown(&d.doc)["source"], "rvt");
    assert_eq!(shown(&d.doc)["edited"], false);
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": body, "set": "Identity", "key": "Category",
               "value": "Partitions"}),
    );
    let category = shown(&d.doc);
    assert_eq!(category["text"], "Partitions");
    assert_eq!(category["source"], "rvt");
    assert_eq!(category["edited"], true);
    assert_eq!(category["source_value"], "Walls");
    // Back to the source's value: no longer edited.
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": body, "set": "Identity", "key": "Category",
               "value": "Walls"}),
    );
    assert_eq!(shown(&d.doc)["edited"], false);
    assert!(shown(&d.doc).get("source_value").is_none());
}

#[test]
fn computed_values_are_answered_and_never_saved() {
    let mut d = design();
    // Steel as the design's default material: a body without a material
    // has no mass (mitcad#1817).
    command(
        &mut d.doc,
        json!({"cmd": "set_component_material", "component": "C0", "material": "steel"}),
    );
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "Wall", "set": "Identity", "key": "Mark",
               "value": "W-12"}),
    );
    let body = d.body.to_string();
    let saved = d.doc.to_json();
    let levels = levels(&d.doc, json!({"body": body, "computed": true}));
    let volume = find(&levels[0], "Computed", "Volume").unwrap();
    assert_eq!(volume["source"], "computed");
    assert_eq!(volume["read_only"], true);
    assert!((volume["value"].as_f64().unwrap() - 500.0).abs() < 1e-6);
    let mass = find(&levels[0], "Computed", "Mass").unwrap();
    // Steel, 7.85 g/cm³.
    assert!((mass["value"].as_f64().unwrap() - 500.0 * 7.85e-6).abs() < 1e-9);
    assert_eq!(
        find(&levels[0], "Computed", "Material").unwrap()["text"],
        "Steel"
    );
    let component = find(&levels[1], "Computed", "Volume").unwrap();
    assert!((component["value"].as_f64().unwrap() - 500.0).abs() < 1e-6);
    // Without `computed` they are not worked out.
    let plain = self::levels(&d.doc, json!({"body": body}));
    assert!(find(&plain[0], "Computed", "Volume").is_none());

    // Saving twice after a recompute gives the same file, without them.
    command(&mut d.doc, json!({"cmd": "recompute"}));
    let first = d.doc.to_json();
    let _ = self::levels(&d.doc, json!({"body": body, "computed": true}));
    let second = d.doc.to_json();
    assert_eq!(first, second);
    assert_eq!(saved, first);
    assert!(!first.contains("\"Computed\""), "{first}");
    assert!(first.contains("\"W-12\""));
}

#[test]
fn the_design_file_keeps_properties_and_types() {
    let mut d = design();
    command(
        &mut d.doc,
        json!({"cmd": "define_property_type", "name": "FireRating", "base": "text",
               "values": ["EI30", "EI60"]}),
    );
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "Wall", "set": "Pset_WallCommon",
               "key": "FireRating", "type": "FireRating", "value": "EI60"}),
    );
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": d.first.to_string(), "set": "Identity",
               "key": "Height", "type": "length", "value": "2.5 m"}),
    );
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": d.body.to_string(), "set": "Thermal",
               "key": "U", "type": "number", "value": 0.17, "unit": "W/(m2K)"}),
    );
    let json = d.doc.to_json();
    let file: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(file["property_types"][0]["name"], "FireRating");
    assert_eq!(file["properties"].as_array().unwrap().len(), 3);
    let loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    assert_eq!(loaded.property_store(), d.doc.property_store());
    let u = loaded
        .property_store()
        .get(PropertyOwner::Body(d.body), "Thermal", "U")
        .unwrap();
    assert_eq!(u.unit.as_deref(), Some("W/(m2K)"));

    // A stored set of computed values or a value off its type is refused.
    let mut bad = file.clone();
    bad["properties"][0]["sets"][0]["name"] = json!("Computed");
    let Err(error) = Document::from_json(&bad.to_string(), MockKernel::default()) else {
        panic!("a stored Computed set was read");
    };
    assert!(error.to_string().contains("computed values"), "{error}");
    let mut bad = file.clone();
    bad["properties"][0]["sets"][0]["properties"][0]["value"] = json!(true);
    assert!(Document::from_json(&bad.to_string(), MockKernel::default()).is_err());
}

#[test]
fn search_finds_owners_by_their_values() {
    let mut d = design();
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "Wall", "set": "Pset_WallCommon",
               "key": "FireRating", "value": "EI60"}),
    );
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": d.second.to_string(), "set": "Pset_WallCommon",
               "key": "FireRating", "value": "EI90"}),
    );
    let search = |doc: &Document<MockKernel>, text: &str| -> Vec<String> {
        query(doc, json!({"query": "property_search", "text": text}))["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["owner"].as_str().unwrap().to_owned())
            .collect()
    };
    let c = d.component.to_string();
    let first = d.first.to_string();
    let second = d.second.to_string();
    // The component and the occurrence that inherits it.
    assert_eq!(
        search(&d.doc, "FireRating = EI60"),
        vec![c.clone(), first.clone()]
    );
    assert_eq!(
        search(&d.doc, "Pset_WallCommon/firerating = ei90"),
        vec![second.clone()]
    );
    assert_eq!(search(&d.doc, "FireRating != EI60"), vec![second.clone()]);
    assert_eq!(
        search(&d.doc, "FireRating ~ EI"),
        vec![c.clone(), first, second]
    );
    assert!(search(&d.doc, "Other/FireRating").is_empty());
    // Computed values when the search names one.
    let body = d.body.to_string();
    // The occurrences inherit their component's.
    assert_eq!(
        search(&d.doc, "Volume > 400 mm^3"),
        vec![c.clone(), d.first.to_string(), d.second.to_string(), body]
    );
    assert!(search(&d.doc, "Volume > 0.001 m^3").is_empty());
    let error = d
        .doc
        .query(&json!({"query": "property_search", "text": "= x"}).to_string())
        .unwrap_err();
    assert!(error.0.contains("names no property"), "{error}");
}

#[test]
fn the_parts_list_has_columns_of_chosen_keys() {
    let mut d = design();
    command(
        &mut d.doc,
        json!({"cmd": "set_component_material", "component": "C0", "material": "steel"}),
    );
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": "Wall", "set": "Identity", "key": "Mark",
               "value": "W-12"}),
    );
    let list = query(
        &d.doc,
        json!({"query": "parts_list", "properties": ["Mark", "Identity/Missing", "Mass"]}),
    );
    let row = &list["rows"][0];
    assert_eq!(row["name"], "Wall");
    assert_eq!(row["properties"]["Mark"], "W-12");
    assert_eq!(row["properties"]["Identity/Missing"], Value::Null);
    assert_eq!(row["properties"]["Mass"], "0.003925 kg");
    // Without keys the rows are as before.
    let plain = query(&d.doc, json!({"query": "parts_list"}));
    assert!(plain["rows"][0].get("properties").is_none());
}

#[test]
fn a_design_placed_as_a_component_brings_its_properties() {
    let mut part = Document::new(MockKernel::default());
    command(
        &mut part,
        json!({"cmd": "define_property_type", "name": "Grade", "base": "text",
               "values": ["A", "B"]}),
    );
    command(
        &mut part,
        json!({"cmd": "set_property", "owner": "C0", "set": "Part", "key": "Grade",
               "type": "Grade", "value": "B"}),
    );
    let mut doc = Document::new(MockKernel::default());
    let (component, _) = doc
        .add_component_copy(&part, Some("Bolt"), ComponentUid::ROOT, Transform::IDENTITY)
        .unwrap();
    let grade = doc
        .property_store()
        .get(PropertyOwner::Component(component), "Part", "Grade")
        .expect("the part's root's property on its component");
    assert_eq!(grade.value, PropertyValue::Text("B".to_owned()));
    assert_eq!(grade.ty, "Grade");
    assert!(doc.property_store().own_type("Grade").is_some());
}
