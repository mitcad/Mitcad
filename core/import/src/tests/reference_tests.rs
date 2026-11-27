// SPDX-License-Identifier: MIT
//! Components of other documents filled from those documents' own imports
//! (mitcad#196) on synthetic dumps with the mock kernel: a two-document
//! assembly whose part has a component of its own, placed by a captured
//! position of the assembly.

use super::*;
use crate::references::{Referenced, link_references};
use mitcad_model::OccurrenceUid;
use mitcad_model_api::DocumentApi;

/// No bodies named by body objects (no derive names them).
static NO_ENTITIES: std::collections::BTreeMap<u64, BodyUid> = std::collections::BTreeMap::new();

/// The part: the block of [`block_dump`] with an occurrence of a component
/// of its own (`Pin`, 3 cm up), which carries a GUID.
fn part_dump() -> Dump {
    let mut v = serde_json::to_value(block_dump()).unwrap();
    v["document"] = json!({"name": "Part", "root_component": "Part"});
    v["components"] = json!([
        {"name": "Part", "is_root": true, "_f3d": {"object_id": 1}},
        {"name": "Pin", "_f3d": {"object_id": 5}}
    ]);
    v["occurrences"] = json!([{
        "component": "Pin",
        "transform": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 3.0],
                      [0.0, 0.0, 0.0, 1.0]],
        "children": [],
        "_f3d": {"object_id": 80, "component_object": 5, "guids": ["c0ffee00-0000-4000-8000-0000000000b1"]}
    }]);
    dump(v)
}

const PART_URN: &str = "urn:mitcad.test:fs.file:vf.part?version=1";

/// The assembly: two occurrences of the part's root component (1 cm along
/// x and 5 cm along y), and a captured position of the pin inside the
/// first at (1, 2, 0) cm.
fn assembly_dump() -> Dump {
    let occurrence = |id: u64, x: f64, y: f64, guid: &str| {
        json!({
            "transform": [[1.0, 0.0, 0.0, x], [0.0, 1.0, 0.0, y], [0.0, 0.0, 1.0, 0.0],
                          [0.0, 0.0, 0.0, 1.0]],
            "isReferencedComponent": true,
            "children": [],
            "_f3d": {"object_id": id, "component_object": 1, "external_key": "00000000000000aa",
                     "external_document": PART_URN, "guids": [guid]}
        })
    };
    dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "assembly.f3d"},
        "document": {"name": "Assembly", "root_component": "Assembly"},
        "components": [{"name": "Assembly", "is_root": true, "_f3d": {"object_id": 1}}],
        "occurrences": [
            occurrence(10, 1.0, 0.0, "c0ffee00-0000-4000-8000-0000000000a1"),
            occurrence(11, 0.0, 5.0, "c0ffee00-0000-4000-8000-0000000000a2")
        ],
        "timeline": {"items": [{
            "index": 0, "name": "Position1", "objectType": "Snapshot",
            "detail": {"positions": [{
                "occurrence": {"kind": "occurrence", "_f3d": {
                    "path": [10, null],
                    "path_guids": ["c0ffee00-0000-4000-8000-0000000000a1",
                                   "C0FFEE00-0000-4000-8000-0000000000B1"],
                    "context_component": 1}},
                "transform": [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 2.0], [0.0, 0.0, 1.0, 0.0],
                              [0.0, 0.0, 0.0, 1.0]]
            }]},
            "_f3d": {"component": 1}
        }]},
        "_f3d": {"external_documents": [{"key": "00000000000000aa", "urn": PART_URN, "version": 1}]}
    }))
}

#[test]
fn components_of_other_documents_are_filled_from_their_imports() {
    let mut part = Document::new(MockKernel::default());
    let part_report = import(&mut part, &part_dump());
    assert_eq!(part_report.items[1].outcome, Outcome::Parametric);
    assert_eq!(part_report.components.occurrence_guids.len(), 1);

    let mut doc = Document::new(MockKernel::default());
    let mut report = import(&mut doc, &assembly_dump());
    // The replay places the occurrences of an empty component and leaves
    // the position inside it out.
    let inserted = report.components.inserted.clone();
    assert_eq!(inserted.len(), 1, "{:?}", report.components);
    assert_eq!(inserted[0].document, PART_URN);
    assert_eq!(inserted[0].occurrences, 2);
    assert_eq!(report.components.external, 2);
    assert_eq!(report.components.positions_inside.len(), 1);
    let component = inserted[0].component;
    assert!(doc.component_bodies(component).is_empty());

    let stored = HashMap::new();
    let sources = HashMap::from([(
        PART_URN.to_owned(),
        Referenced {
            document: &part,
            occurrence_guids: &part_report.components.occurrence_guids,
            occurrence_objects: &part_report.components.occurrence_objects,
            name: "Part".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: None,
        },
    )]);
    let outcome = link_references(&mut doc, &mut report, &sources);
    assert!(outcome.failed.is_empty(), "{outcome:?}");
    // The position arranges the inside of the first occurrence only, so
    // the occurrences are not alike: the second one has a copy of its own.
    assert_eq!(outcome.linked.len(), 2, "{outcome:?}");
    assert_eq!(outcome.linked[0].name, "Part");
    assert_eq!(outcome.linked[1].name, "Part (1)");
    assert_eq!(report.components.copies, 1);
    assert_eq!(doc.assembly().name(component), "Part");
    let occurrences: Vec<_> = doc.assembly().occurrences_of(component).collect();
    assert_eq!(occurrences.len(), 1);
    let other = doc
        .assembly()
        .occurrences
        .iter()
        .find(|o| o.component != component);
    let second = other
        .expect("the second occurrence has another component")
        .component;
    assert_eq!(doc.assembly().name(second), "Part (1)");
    assert_eq!(report.components.inserted.len(), 2);
    assert_eq!(
        report
            .components
            .inserted
            .iter()
            .map(|i| i.occurrences)
            .sum::<usize>(),
        2
    );
    // The part's block, with its history: the copy's features are the
    // component's.
    doc.recompute();
    assert_eq!(doc.component_bodies(component).len(), 1);
    assert_eq!(doc.component_bodies(second).len(), 1);
    assert!(doc.features().filter(|f| f.component == component).count() >= 2);
    // The pin inside the part, where the captured position puts it in the
    // first occurrence: (10, 20, 0) mm in the assembly, (0, 20, 0) mm in
    // the part.
    assert_eq!(outcome.positions, 1, "{outcome:?}");
    let pin = report.components.occurrence_guids["c0ffee00-0000-4000-8000-0000000000b1"][0];
    let placed = doc.assembly().occurrence(pin).unwrap();
    assert_eq!(placed.parent, component);
    doc.recompute();
    let t = doc.placement(pin).unwrap().translation;
    assert!(
        (t[0]).abs() < 1e-9 && (t[1] - 20.0).abs() < 1e-9 && t[2].abs() < 1e-9,
        "{t:?}"
    );
    // The second occurrence's copy keeps the pin where the part has it
    // (3 cm up), and the GUID names both pins.
    let pins = &report.components.occurrence_guids["c0ffee00-0000-4000-8000-0000000000b1"];
    assert_eq!(pins.len(), 2);
    let other_pin = doc.assembly().occurrence(pins[1]).unwrap();
    assert_eq!(other_pin.parent, second);
    let t = doc.placement(pins[1]).unwrap().translation;
    assert!(
        t[0].abs() < 1e-9 && t[1].abs() < 1e-9 && (t[2] - 30.0).abs() < 1e-9,
        "{t:?}"
    );
    // The captured position came in as a feature of the part's component,
    // and its report entry says so.
    let item = &report.items[0];
    assert_eq!(item.outcome, Outcome::Parametric, "{item:?}");
    assert_eq!(item.note, None);
    let feature: FeatureUid = item.features[0].parse().unwrap();
    assert_eq!(
        doc.features().find(|f| f.uid == feature).unwrap().component,
        component
    );
    assert_eq!(doc.import_note(feature), None);
    // A second link has nothing left to fill.
    let again = link_references(&mut doc, &mut report, &sources);
    assert_eq!(again.failed.len(), 2);
}

#[test]
fn components_of_documents_not_found_stay_empty() {
    let mut doc = Document::new(MockKernel::default());
    let mut report = import(&mut doc, &assembly_dump());
    let outcome = link_references(&mut doc, &mut report, &HashMap::new());
    assert!(outcome.linked.is_empty() && outcome.failed.is_empty());
    assert_eq!(
        outcome
            .positions_left_out
            .get("a position inside a component of a document not found"),
        Some(&1)
    );
    let component = report.components.inserted[0].component;
    assert!(doc.component_bodies(component).is_empty());
}

/// The assembly with a rigid as-built joint of the part's pin (inside the
/// first occurrence) to the second occurrence, as the file names the
/// pin: by the GUID the part's occurrence carries.
fn joined_assembly_dump() -> Dump {
    let mut v = serde_json::to_value(assembly_dump()).unwrap();
    v["timeline"]["items"].as_array_mut().unwrap().push(json!({
        "index": 1, "name": "As-built1", "objectType": "AsBuiltJoint",
        "_f3d": {"component": 1},
        "detail": {
            "occurrenceOne": {"kind": "occurrence", "_f3d": {
                "path": [10, null],
                "path_guids": ["c0ffee00-0000-4000-8000-0000000000a1",
                               "C0FFEE00-0000-4000-8000-0000000000B1"],
                "context_component": 1}},
            "occurrenceTwo": {"kind": "occurrence", "_f3d": {
                "path": [11], "path_guids": ["c0ffee00-0000-4000-8000-0000000000a2"],
                "context_component": 1}},
            "geometry": null,
            "jointMotion": {"_type": "RigidJointMotion",
                            "_f3d": {"type_code": 11, "values": []}}
        }
    }));
    dump(v)
}

#[test]
fn components_filled_before_the_replay_take_the_joints_in_place() {
    let mut part = Document::new(MockKernel::default());
    let part_report = import(&mut part, &part_dump());
    let dump = joined_assembly_dump();
    let stored = HashMap::new();
    let sources = HashMap::from([(
        PART_URN.to_owned(),
        Referenced {
            document: &part,
            occurrence_guids: &part_report.components.occurrence_guids,
            occurrence_objects: &part_report.components.occurrence_objects,
            name: "Part".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: None,
        },
    )]);
    let mut doc = Document::new(MockKernel::default());
    let report = crate::import_design_with(
        &mut doc,
        &dump,
        &mut NoGeometry,
        &Options::default(),
        Some(&sources),
    );
    // The component is filled, and the pin inside it is where the design's
    // captured position puts it.
    let link = &report.components.link;
    assert_eq!(link.linked.len(), 2, "{link:?}");
    assert_eq!(link.positions, 1, "{link:?}");
    let component = link.linked[0].component;
    assert!(report.components.filled.contains(&component));
    // The joint came in where its item is, not after the replay.
    let item = &report.items[1];
    assert_eq!(item.outcome, Outcome::Parametric, "{item:?}");
    assert_eq!(report.joints.as_built, 1);
    let uid: FeatureUid = item.features[0].parse().unwrap();
    let FeatureDef::AsBuiltJoint(def) = doc.feature(uid).unwrap().def.clone() else {
        panic!("an as-built joint");
    };
    let t = def.relative.unwrap().translation;
    assert!(
        (t[0] - 10.0).abs() < 1e-9 && (t[1] + 30.0).abs() < 1e-9 && t[2].abs() < 1e-9,
        "{t:?}"
    );
    // The import ended: the document's bodies include the part's block
    // again (its history was copied).
    doc.recompute();
    assert_eq!(doc.component_bodies(component).len(), 1);
    assert_eq!(doc.bodies().len(), 2);
    // The captured position's report entry says it was placed.
    assert_eq!(report.items[0].outcome, Outcome::Parametric);
    // (The scopes were the import's: gone with it.)
    assert!(doc.import_scope("c0ffee00").is_empty());

    // A document not found: the joint comes out as without references.
    let plain = import(&mut Document::new(MockKernel::default()), &dump);
    let mut doc = Document::new(MockKernel::default());
    let none = HashMap::new();
    let report = crate::import_design_with(
        &mut doc,
        &dump,
        &mut NoGeometry,
        &Options::default(),
        Some(&none),
    );
    assert_eq!(report.items[1].outcome, Outcome::Skipped);
    assert!(
        report.items[1]
            .note
            .as_deref()
            .is_some_and(|n| n.contains("inside a component of another document")),
        "{:?}",
        report.items[1]
    );
    assert_eq!(report.joints.skipped, plain.joints.skipped);
    assert!(report.components.filled.is_empty());
    assert_eq!(
        report
            .components
            .link
            .positions_left_out
            .get("a position inside a component of a document not found"),
        Some(&1)
    );
}

/// An assembly whose own block is cut by a body of the part (a tool in a
/// component of another document), named as the decoder gives such a tool:
/// by the document's GUID and the stored body it found it as.
fn cut_by_a_referenced_body_dump(guid: &str) -> Dump {
    let mut v = serde_json::to_value(block_dump()).unwrap();
    v["document"] = json!({"name": "Assembly", "root_component": "Assembly"});
    v["components"] = json!([{"name": "Assembly", "is_root": true, "_f3d": {"object_id": 1}}]);
    // The part, 5 mm along x.
    v["occurrences"] = json!([{
        "transform": [[1.0, 0.0, 0.0, 0.5], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0],
                      [0.0, 0.0, 0.0, 1.0]],
        "isReferencedComponent": true,
        "children": [],
        "_f3d": {"object_id": 10, "component_object": 1, "external_key": "00000000000000bb",
                 "external_document": PART_URN, "guids": ["c0ffee00-0000-4000-8000-0000000000c1"]}
    }]);
    v["_f3d"] = json!({"external_documents": [{
        "key": "00000000000000bb", "urn": PART_URN, "version": 1, "document_guid": guid}]});
    let items = v["timeline"]["items"].as_array_mut().unwrap();
    items.push(json!({
        "index": 2, "name": "Combine1", "objectType": "CombineFeature",
        "detail": {
            "operation": "CutFeatureOperation", "isKeepToolBodies": true,
            "targetBody": {"kind": "body", "objectType": "BRepBody",
                           "_f3d": {"producer": 1, "body_index": 0}},
            "toolBodies": [{"kind": "body", "objectType": "BRepBody",
                            "_f3d": {"document": guid, "stored": "BREP.part.smbh#1",
                                     "entities": [[{"tag": "1", "kind": 3, "ops": []}]]}}]}
    }));
    dump(v)
}

#[test]
fn a_combine_takes_a_tool_in_a_component_of_another_document() {
    let guid = "0a0a0a0a-0000-4000-8000-00000000d0c0";
    let mut part = Document::new(MockKernel::default());
    let part_report = import(&mut part, &part_dump());
    // The part's block is the body its import matched to the stored one.
    let block = format!("{}.b0", part_report.items[1].features[0]);
    let stored = HashMap::from([("BREP.part.smbh#1".to_owned(), block.parse().unwrap())]);
    let sources = HashMap::from([(
        PART_URN.to_owned(),
        Referenced {
            document: &part,
            occurrence_guids: &part_report.components.occurrence_guids,
            occurrence_objects: &part_report.components.occurrence_objects,
            name: "Part".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: None,
        },
    )]);
    let dump = cut_by_a_referenced_body_dump(guid);

    let mut doc = Document::new(MockKernel::default());
    let report = crate::import_design_with(
        &mut doc,
        &dump,
        &mut NoGeometry,
        &Options::default(),
        Some(&sources),
    );
    let combine = &report.items[2];
    assert_eq!(combine.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(report.components.external_bodies.len(), 1);
    let copy = report.components.external_bodies[&format!("{guid}|BREP.part.smbh#1")][0];
    let uid: FeatureUid = combine.features[0].parse().unwrap();
    let def: Value = serde_json::from_str(
        &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
            .unwrap(),
    )
    .unwrap();
    // The tool is the copy's block, taken by its occurrence link.
    assert_eq!(def["def"]["tools"], json!([copy.to_string()]));
    assert!(
        def["def"]["tool_links"][copy.to_string()].is_object(),
        "{def}"
    );
    // The own block was cut by it.
    let target = format!("{}.b0", report.items[1].features[0]);
    let cut = &doc.body_shape(target.parse().unwrap()).unwrap().history;
    assert!(cut.starts_with("cut("), "{cut}");

    // Without the document the tool is not found.
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_ne!(report.items[2].outcome, Outcome::Parametric);
}

const SUB_URN: &str = "urn:mitcad.test:fs.file:vf.sub?version=1";

/// An occurrence of a root component of another document.
fn external_occurrence(id: u64, key: &str, urn: &str, at: [f64; 3], guid: &str) -> Value {
    json!({
        "transform": [[1.0, 0.0, 0.0, at[0]], [0.0, 1.0, 0.0, at[1]], [0.0, 0.0, 1.0, at[2]],
                      [0.0, 0.0, 0.0, 1.0]],
        "isReferencedComponent": true,
        "children": [],
        "_f3d": {"object_id": id, "component_object": 1, "external_key": key,
                 "external_document": urn, "guids": [guid]}
    })
}

/// A sub-assembly that places the part twice (10 and 30 cm along x).
fn sub_dump() -> Dump {
    dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "sub.f3d"},
        "document": {"name": "Sub", "root_component": "Sub"},
        "components": [{"name": "Sub", "is_root": true, "_f3d": {"object_id": 1}}],
        "occurrences": [
            external_occurrence(20, "00000000000000aa", PART_URN, [10.0, 0.0, 0.0],
                                "c0ffee00-0000-4000-8000-0000000000a1"),
            external_occurrence(21, "00000000000000aa", PART_URN, [30.0, 0.0, 0.0],
                                "c0ffee00-0000-4000-8000-0000000000a2")
        ],
        "timeline": {"items": []},
        "_f3d": {"external_documents": [{"key": "00000000000000aa", "urn": PART_URN, "version": 1}]}
    }))
}

/// An assembly that places the sub-assembly once, with one captured
/// position that arranges the pin inside each of the part's occurrences
/// differently (the pin is one occurrence in the part's component, which
/// the sub-assembly places twice).
fn nested_dump() -> Dump {
    let position = |part: &str, at: [f64; 3]| {
        json!({
            "occurrence": {"kind": "occurrence", "_f3d": {
                "path": [10, null, null],
                "path_guids": ["c0ffee00-0000-4000-8000-0000000000e1", part,
                               "C0FFEE00-0000-4000-8000-0000000000B1"],
                "context_component": 1}},
            "transform": [[1.0, 0.0, 0.0, at[0]], [0.0, 1.0, 0.0, at[1]], [0.0, 0.0, 1.0, at[2]],
                          [0.0, 0.0, 0.0, 1.0]]
        })
    };
    dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "nested.f3d"},
        "document": {"name": "Nested", "root_component": "Nested"},
        "components": [{"name": "Nested", "is_root": true, "_f3d": {"object_id": 1}}],
        "occurrences": [external_occurrence(
            10, "00000000000000bb", SUB_URN, [0.0, 0.0, 0.0],
            "c0ffee00-0000-4000-8000-0000000000e1")],
        "timeline": {"items": [{
            "index": 0, "name": "Position1", "objectType": "Snapshot",
            "detail": {"positions": [
                position("c0ffee00-0000-4000-8000-0000000000a1", [11.0, 2.0, 0.0]),
                position("c0ffee00-0000-4000-8000-0000000000a2", [35.0, 4.0, 0.0])
            ]},
            "_f3d": {"component": 1}
        }]},
        "_f3d": {"external_documents": [{"key": "00000000000000bb", "urn": SUB_URN, "version": 1}]}
    }))
}

#[test]
fn a_component_placed_twice_inside_a_copy_gets_a_copy_for_each_arrangement() {
    // The part, and the sub-assembly that places it twice (filled from the
    // part).
    let mut part = Document::new(MockKernel::default());
    let part_report = import(&mut part, &part_dump());
    let stored = HashMap::new();
    let part_sources = HashMap::from([(
        PART_URN.to_owned(),
        Referenced {
            document: &part,
            occurrence_guids: &part_report.components.occurrence_guids,
            occurrence_objects: &part_report.components.occurrence_objects,
            name: "Part".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: None,
        },
    )]);
    let mut sub = Document::new(MockKernel::default());
    let sub_report = crate::import_design_with(
        &mut sub,
        &sub_dump(),
        &mut NoGeometry,
        &Options::default(),
        Some(&part_sources),
    );
    assert_eq!(sub_report.components.linked, 2, "{}", sub_report.text());
    // One component for both occurrences: the pin has one occurrence.
    let pins = &sub_report.components.occurrence_guids["c0ffee00-0000-4000-8000-0000000000b1"];
    assert_eq!(pins.len(), 1);

    let sources = HashMap::from([(
        SUB_URN.to_owned(),
        Referenced {
            document: &sub,
            occurrence_guids: &sub_report.components.occurrence_guids,
            occurrence_objects: &sub_report.components.occurrence_objects,
            name: "Sub".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: None,
        },
    )]);
    let mut doc = Document::new(MockKernel::default());
    let report = crate::import_design_with(
        &mut doc,
        &nested_dump(),
        &mut NoGeometry,
        &Options::default(),
        Some(&sources),
    );
    // The two positions place the pin differently, so the second
    // occurrence of the part inside the copy has a part copy of its own.
    assert_eq!(report.components.copies, 1, "{}", report.text());
    assert_eq!(
        report.components.link.positions, 2,
        "{:?}",
        report.components.link
    );
    assert!(report.components.link.positions_left_out.is_empty());
    assert_eq!(
        report.items[0].outcome,
        Outcome::Parametric,
        "{:?}",
        report.items[0]
    );
    let pins = report.components.occurrence_guids["c0ffee00-0000-4000-8000-0000000000b1"].clone();
    assert_eq!(pins.len(), 2);
    doc.recompute();
    // Where the pins are in the design: (11, 2, 0) cm and (35, 4, 0) cm.
    let world = |pin: OccurrenceUid| {
        let a = doc.assembly();
        let mut chain = vec![pin];
        let mut at = a.occurrence(pin).unwrap().parent;
        while !at.is_root() {
            let o = a.occurrences_of(at).next().expect("placed");
            chain.push(o.uid);
            at = o.parent;
        }
        chain.reverse();
        doc.path_transform(&chain).translation
    };
    let mut places: Vec<[f64; 3]> = pins.iter().map(|p| world(*p)).collect();
    places.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let want = [[110.0, 20.0, 0.0], [350.0, 40.0, 0.0]];
    for (got, want) in places.iter().zip(want) {
        assert!(
            (0..3).all(|i| (got[i] - want[i]).abs() < 1e-6),
            "{places:?} against {want:?}"
        );
    }
}

#[test]
fn a_tool_of_the_part_placed_twice_is_linked_through_the_occurrence_its_context_names() {
    // The part is placed twice (5 mm and 20 mm along x, no positions inside:
    // both share one copy); the combine's tool carries the placement of the
    // occurrence it was selected in (the second).
    let guid = "0a0a0a0a-0000-4000-8000-00000000d0c0";
    let mut part = Document::new(MockKernel::default());
    let part_report = import(&mut part, &part_dump());
    let block = format!("{}.b0", part_report.items[1].features[0]);
    let stored = HashMap::from([("BREP.part.smbh#1".to_owned(), block.parse().unwrap())]);
    let sources = HashMap::from([(
        PART_URN.to_owned(),
        Referenced {
            document: &part,
            occurrence_guids: &part_report.components.occurrence_guids,
            occurrence_objects: &part_report.components.occurrence_objects,
            name: "Part".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: None,
        },
    )]);
    let mut v = serde_json::to_value(cut_by_a_referenced_body_dump(guid)).unwrap();
    let mut second = v["occurrences"][0].clone();
    second["transform"][0][3] = json!(2.0);
    second["_f3d"]["object_id"] = json!(11);
    second["_f3d"]["guids"] = json!(["c0ffee00-0000-4000-8000-0000000000c2"]);
    v["occurrences"].as_array_mut().unwrap().push(second);
    let tool = &mut v["timeline"]["items"][2]["detail"]["toolBodies"][0]["_f3d"];
    tool["object_id"] = json!(77);
    tool["context"] = json!([
        [1.0, 0.0, 0.0, 2.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0]
    ]);
    let dump = dump(v);

    let mut doc = Document::new(MockKernel::default());
    let report = crate::import_design_with(
        &mut doc,
        &dump,
        &mut NoGeometry,
        &Options::default(),
        Some(&sources),
    );
    let combine = &report.items[2];
    assert_eq!(combine.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(report.components.copies, 0);
    let component = report.components.inserted[0].component;
    let paths = doc.assembly().paths_to(component);
    assert_eq!(paths.len(), 2);
    // The occurrence 20 mm along x.
    let second = paths
        .iter()
        .find(|p| {
            doc.assembly()
                .occurrence(p[0])
                .is_some_and(|o| (o.transform.translation[0] - 20.0).abs() < 1e-9)
        })
        .expect("the second occurrence");
    let uid: FeatureUid = combine.features[0].parse().unwrap();
    let def: Value = serde_json::from_str(
        &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
            .unwrap(),
    )
    .unwrap();
    let links = def["def"]["tool_links"].as_object().expect("tool links");
    assert_eq!(links.len(), 1, "{def}");
    let link = links.values().next().unwrap();
    assert_eq!(link["source"], json!(second[0].to_string()), "{def}");
}

#[test]
fn two_occurrences_of_one_part_named_by_one_item_get_a_copy_each() {
    // The combine's two tools are the part's block in each of its two
    // occurrences: the occurrences share one copy, so the tools would be the
    // same body; each gets a copy of its own and the tools differ.
    let guid = "0a0a0a0a-0000-4000-8000-00000000d0c0";
    let mut part = Document::new(MockKernel::default());
    let part_report = import(&mut part, &part_dump());
    let block = format!("{}.b0", part_report.items[1].features[0]);
    let stored = HashMap::from([("BREP.part.smbh#1".to_owned(), block.parse().unwrap())]);
    let sources = HashMap::from([(
        PART_URN.to_owned(),
        Referenced {
            document: &part,
            occurrence_guids: &part_report.components.occurrence_guids,
            occurrence_objects: &part_report.components.occurrence_objects,
            name: "Part".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: None,
        },
    )]);
    let mut v = serde_json::to_value(cut_by_a_referenced_body_dump(guid)).unwrap();
    let mut second = v["occurrences"][0].clone();
    second["transform"][0][3] = json!(2.0);
    second["_f3d"]["object_id"] = json!(11);
    second["_f3d"]["guids"] = json!(["c0ffee00-0000-4000-8000-0000000000c2"]);
    v["occurrences"].as_array_mut().unwrap().push(second);
    let at = |x: f64| {
        json!([
            [1.0, 0.0, 0.0, x],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0]
        ])
    };
    let tools = v["timeline"]["items"][2]["detail"]["toolBodies"]
        .as_array_mut()
        .unwrap();
    let mut other = tools[0].clone();
    tools[0]["_f3d"]["object_id"] = json!(77);
    tools[0]["_f3d"]["context"] = at(0.5);
    other["_f3d"]["object_id"] = json!(78);
    other["_f3d"]["context"] = at(2.0);
    tools.push(other);
    let dump = dump(v);

    let mut doc = Document::new(MockKernel::default());
    let report = crate::import_design_with(
        &mut doc,
        &dump,
        &mut NoGeometry,
        &Options::default(),
        Some(&sources),
    );
    let combine = &report.items[2];
    assert_eq!(combine.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(report.components.copies, 1);
    let uid: FeatureUid = combine.features[0].parse().unwrap();
    let def: Value = serde_json::from_str(
        &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
            .unwrap(),
    )
    .unwrap();
    let tools = def["def"]["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 2, "{def}");
    assert_ne!(tools[0], tools[1], "{def}");
    // Each tool is read through its own occurrence.
    let sources: Vec<&str> = def["def"]["tool_links"]
        .as_object()
        .unwrap()
        .values()
        .map(|l| l["source"].as_str().unwrap())
        .collect();
    assert_eq!(sources.len(), 2, "{def}");
    assert_ne!(sources[0], sources[1], "{def}");
}

/// An assembly whose one occurrence of the part the file places only along
/// paths inside it (mitcad#1892: a joint on a part inside an inserted
/// design moves the whole occurrence, and the file keeps the placement of
/// the path to each occurrence inside): its own placements leave it where
/// it was inserted (the identity), the path to the part's pin (3 cm up in
/// the part) puts the pin turned a quarter about x at (1, -1, 0) cm, and a
/// path to an occurrence the part does not have says nothing.
fn placed_inside_dump() -> Dump {
    let identity = json!([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0]
    ]);
    let pin = json!([
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0, -1.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0]
    ]);
    let elsewhere = json!([
        [1.0, 0.0, 0.0, 7.0],
        [0.0, 1.0, 0.0, 7.0],
        [0.0, 0.0, 1.0, 7.0],
        [0.0, 0.0, 0.0, 1.0]
    ]);
    let step = |t: &Value| json!([{"index": 0, "object_id": 50, "transform": t}]);
    dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "assembly.f3d"},
        "document": {"name": "Assembly", "root_component": "Assembly"},
        "components": [{"name": "Assembly", "is_root": true, "_f3d": {"object_id": 1}}],
        "occurrences": [{
            "transform": identity,
            "isReferencedComponent": true,
            "children": [],
            "_f3d": {"object_id": 10, "component_object": 1, "external_key": "00000000000000aa",
                     "external_document": PART_URN,
                     "guids": ["c0ffee00-0000-4000-8000-0000000000a1"],
                     "placements": step(&identity),
                     "inside_placements": [
                         {"inner": [999], "placements": step(&elsewhere)},
                         {"inner": [80], "placements": step(&pin)}
                     ]}
        }],
        "timeline": {"items": [{
            "index": 0, "name": "Component Insert1", "objectType": "ComponentInsert",
            "_f3d": {"component": 1, "object_id": 50}
        }]},
        "_f3d": {"external_documents": [{"key": "00000000000000aa", "urn": PART_URN, "version": 1}]}
    }))
}

#[test]
fn an_occurrence_placed_along_paths_inside_it_takes_their_placement() {
    let mut part = Document::new(MockKernel::default());
    let part_report = import(&mut part, &part_dump());
    assert_eq!(part_report.components.occurrence_objects.len(), 1);
    let dump = placed_inside_dump();
    let stored = HashMap::new();
    let sources = HashMap::from([(
        PART_URN.to_owned(),
        Referenced {
            document: &part,
            occurrence_guids: &part_report.components.occurrence_guids,
            occurrence_objects: &part_report.components.occurrence_objects,
            name: "Part".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: None,
        },
    )]);
    let placed = |sources: Option<&HashMap<String, Referenced<'_, MockKernel>>>| {
        let mut doc = Document::new(MockKernel::default());
        let report = crate::import_design_with(
            &mut doc,
            &dump,
            &mut NoGeometry,
            &Options::default(),
            sources,
        );
        let o = report.components.occurrence_objects[&10];
        doc.assembly().occurrence(o).unwrap().transform
    };
    // The part's own placement: the pin's with the pin's 3 cm taken off,
    // within 1e-6 mm.
    let t = placed(Some(&sources));
    let want_linear = [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]];
    for (row, want_row) in t.linear.iter().zip(want_linear.iter()) {
        for (got, want) in row.iter().zip(want_row.iter()) {
            assert!((got - want).abs() < 1e-9, "{t:?}");
        }
    }
    let want = [10.0, 20.0, 0.0];
    assert!(
        (0..3).all(|i| (t.translation[i] - want[i]).abs() < 1e-6),
        "{t:?}"
    );
    // Without the part's file the paths inside cannot be undone: the
    // occurrence stays where its own placements put it.
    let t = placed(None);
    assert!(
        t.translation.iter().all(|v| v.abs() < 1e-6) && (t.linear[1][1] - 1.0).abs() < 1e-9,
        "{t:?}"
    );
}

#[test]
fn a_grounded_occurrence_placed_along_paths_inside_it_takes_their_placement() {
    // mitcad#2449: the shell of an assembly, a component of another
    // document grounded in the design, placed only along the paths inside
    // it, stayed at the identity of its insert (grounded occurrences were
    // not placed), every part in it off by the shell's placement.
    let mut part = Document::new(MockKernel::default());
    let part_report = import(&mut part, &part_dump());
    let mut v = serde_json::to_value(placed_inside_dump()).unwrap();
    v["occurrences"][0]["isGrounded"] = json!(true);
    let dump = dump(v);
    let stored = HashMap::new();
    let sources = HashMap::from([(
        PART_URN.to_owned(),
        Referenced {
            document: &part,
            occurrence_guids: &part_report.components.occurrence_guids,
            occurrence_objects: &part_report.components.occurrence_objects,
            name: "Part".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: None,
        },
    )]);
    let placed = |sources: Option<&HashMap<String, Referenced<'_, MockKernel>>>| {
        let mut doc = Document::new(MockKernel::default());
        let report = crate::import_design_with(
            &mut doc,
            &dump,
            &mut NoGeometry,
            &Options::default(),
            sources,
        );
        let o = report.components.occurrence_objects[&10];
        let occurrence = doc.assembly().occurrence(o).unwrap();
        (
            occurrence.transform,
            occurrence.grounded,
            report.warnings.clone(),
        )
    };
    // Where the paths inside put it, as when it is not grounded, and
    // still grounded.
    let (t, grounded, warnings) = placed(Some(&sources));
    assert!(grounded, "{warnings:?}");
    let want_linear = [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]];
    for (row, want_row) in t.linear.iter().zip(want_linear.iter()) {
        for (got, want) in row.iter().zip(want_row.iter()) {
            assert!((got - want).abs() < 1e-9, "{t:?} {warnings:?}");
        }
    }
    let want = [10.0, 20.0, 0.0];
    assert!(
        (0..3).all(|i| (t.translation[i] - want[i]).abs() < 1e-6),
        "{t:?} {warnings:?}"
    );
    assert!(
        !warnings.iter().any(|w| w.contains("is grounded")),
        "{warnings:?}"
    );
    // Without the part's file it stays where it is stored, grounded.
    let (t, grounded, _) = placed(None);
    assert!(grounded);
    assert!(
        t.translation.iter().all(|v| v.abs() < 1e-6) && (t.linear[1][1] - 1.0).abs() < 1e-9,
        "{t:?}"
    );
}

/// A design that places a level inside a copy otherwise than the other
/// document does (mitcad#2492: an assembly of an older layout whose wheel
/// the other document leaves where it was inserted; a lift arm the shell
/// placing it leaves elsewhere): the part has two pins (3 and 6 cm up),
/// the paths the design stores through its occurrence put the first where
/// the part has it and the second 9 cm up. The second pin's occurrence in
/// the copy is placed there (a position inside the copy at the end of the
/// timeline); the first keeps its place and makes none. Before, the paths
/// gave only the occurrence's own placement, and the second pin stayed at
/// 6 cm.
#[test]
fn the_paths_through_an_occurrence_place_the_levels_inside_its_copy() {
    let mut v = serde_json::to_value(part_dump()).unwrap();
    let pin = |id: u64, z: f64, guid: &str| {
        json!({
            "component": "Pin",
            "transform": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, z],
                          [0.0, 0.0, 0.0, 1.0]],
            "children": [],
            "_f3d": {"object_id": id, "component_object": 5, "guids": [guid]}
        })
    };
    v["occurrences"] = json!([
        pin(80, 3.0, "c0ffee00-0000-4000-8000-0000000000b1"),
        pin(81, 6.0, "c0ffee00-0000-4000-8000-0000000000b2")
    ]);
    let mut part = Document::new(MockKernel::default());
    let part_report = import(&mut part, &dump(v));
    assert_eq!(part_report.components.occurrence_objects.len(), 2);
    let up = |z: f64| {
        json!([{"index": 0, "object_id": 50, "transform": [
            [1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, z], [0.0, 0.0, 0.0, 1.0]]}])
    };
    let mut a = serde_json::to_value(placed_inside_dump()).unwrap();
    a["occurrences"][0]["_f3d"]["inside_placements"] = json!([
        {"inner": [80], "placements": up(3.0)},
        {"inner": [81], "placements": up(9.0)}
    ]);
    let assembly = dump(a);
    let stored = HashMap::new();
    let sources = HashMap::from([(
        PART_URN.to_owned(),
        Referenced {
            document: &part,
            occurrence_guids: &part_report.components.occurrence_guids,
            occurrence_objects: &part_report.components.occurrence_objects,
            name: "Part".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: None,
        },
    )]);
    let positions = crate::references::inside_positions(
        &assembly,
        &crate::components::Components::default(),
        &sources,
        &HashMap::new(),
    );
    // (Without the design's occurrences no path starts.)
    assert!(positions.is_empty(), "{positions:?}");
    let mut doc = Document::new(MockKernel::default());
    let report = crate::import_design_with(
        &mut doc,
        &assembly,
        &mut NoGeometry,
        &Options::default(),
        Some(&sources),
    );
    let text = report.text();
    // One position, for the second pin.
    let inside: Vec<_> = report
        .components
        .positions_inside
        .iter()
        .filter(|p| p.index == crate::references::INSIDE_PLACEMENTS_INDEX)
        .collect();
    assert_eq!(inside.len(), 1, "{inside:?}\n{text}");
    assert_eq!(
        inside[0].inner,
        vec!["c0ffee00-0000-4000-8000-0000000000b2".to_owned()]
    );
    let z = |guid: &str| {
        let o = report.components.occurrence_guids[guid]
            .iter()
            .filter_map(|o| doc.assembly().occurrence(*o))
            .find(|o| !o.parent.is_root())
            .unwrap_or_else(|| panic!("the copy's occurrence of {guid}\n{text}"));
        o.transform.translation[2]
    };
    assert!(
        (z("c0ffee00-0000-4000-8000-0000000000b1") - 30.0).abs() < 1e-6,
        "{text}"
    );
    assert!(
        (z("c0ffee00-0000-4000-8000-0000000000b2") - 90.0).abs() < 1e-6,
        "{text}"
    );
}

#[test]
fn the_copies_of_a_document_show_its_sketches_as_its_import_does() {
    // mitcad#2449: the copies of another document's design showed the
    // sketches the document's own import hides (a light bulb that
    // differs from the default was not copied). Here the part's file
    // shows its block's sketch, which its extrusion uses (hidden by
    // default).
    let mut part = Document::new(MockKernel::default());
    let part_report = import(&mut part, &part_dump());
    let sketch = part
        .features()
        .find(|f| matches!(f.def, FeatureDef::Sketch(_)))
        .map(|f| f.uid)
        .expect("the part's sketch");
    assert_eq!(part.feature_shown(sketch), Some(false));
    part.set_feature_visible(sketch, true).unwrap();

    let mut doc = Document::new(MockKernel::default());
    let mut report = import(&mut doc, &assembly_dump());
    let stored = HashMap::new();
    let sources = HashMap::from([(
        PART_URN.to_owned(),
        Referenced {
            document: &part,
            occurrence_guids: &part_report.components.occurrence_guids,
            occurrence_objects: &part_report.components.occurrence_objects,
            name: "Part".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: None,
        },
    )]);
    let outcome = link_references(&mut doc, &mut report, &sources);
    assert!(outcome.failed.is_empty(), "{outcome:?}");
    // Two copies of the part (the occurrences are arranged differently
    // inside), each with the sketch shown.
    let copies: Vec<FeatureUid> = doc
        .features()
        .filter(|f| f.component != ComponentUid::ROOT && matches!(f.def, FeatureDef::Sketch(_)))
        .map(|f| f.uid)
        .collect();
    assert_eq!(copies.len(), 2, "{copies:?}");
    for copy in &copies {
        assert_eq!(doc.feature_shown(*copy), Some(true), "{copy}");
    }
    // The report counts them: none differs from the part's import.
    assert!(report.light_bulbs.copied >= 2, "{:?}", report.light_bulbs);
    assert_eq!(
        report.light_bulbs.copies_differ, 0,
        "{:?}",
        report.light_bulbs
    );
    assert!(
        report
            .text()
            .contains("shown or hidden otherwise than in their documents"),
        "{}",
        report.text()
    );
}

const MID_URN: &str = "urn:mitcad.test:fs.file:vf.mid?version=1";

/// Paths that go on more than one document deep (mitcad#2528: the board
/// inside the electronics stack, the bearings inside the lift system inside
/// the shell). Three documents: the part (a pin 3 cm up), a middle
/// assembly placing the part 1 cm along x and two plates of its own, and a
/// design placing the middle assembly. The design's paths through it keep
/// the plates where the middle assembly has them (its own placement), move
/// the part to 2 cm along x, and leave the pin where the middle assembly
/// has it (1 cm along x, 3 cm up). The path to the pin names the part by
/// the middle assembly's object id and the pin by the part's: it is
/// resolved through the middle assembly's import (`object_guids`, which
/// knows the part's), and kept though the pin is where the middle assembly
/// has it, because the part above it moves. Before, the path was not
/// resolved and the pin moved with the part, 1 cm away from the file's
/// place.
#[test]
fn the_paths_more_than_one_document_deep_place_the_levels_there() {
    let mut part = Document::new(MockKernel::default());
    let part_report = import(&mut part, &part_dump());
    let pin_guid = "c0ffee00-0000-4000-8000-0000000000b1";
    assert_eq!(
        part_report.components.object_guids.occurrences[&80].guid,
        pin_guid
    );
    let stored = HashMap::new();
    let part_sources = HashMap::from([(
        PART_URN.to_owned(),
        Referenced {
            document: &part,
            occurrence_guids: &part_report.components.occurrence_guids,
            occurrence_objects: &part_report.components.occurrence_objects,
            name: "Part".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: Some(part_report.components.object_guids.clone()),
        },
    )]);
    let moved = |x: f64, y: f64, z: f64| {
        json!([
            [1.0, 0.0, 0.0, x],
            [0.0, 1.0, 0.0, y],
            [0.0, 0.0, 1.0, z],
            [0.0, 0.0, 0.0, 1.0]
        ])
    };
    let step = |t: Value| json!([{"index": 0, "object_id": 50, "transform": t}]);
    let plate = |id: u64, y: f64, guid: &str| {
        json!({
            "component": "Plate",
            "transform": moved(0.0, y, 0.0),
            "children": [],
            "_f3d": {"object_id": id, "component_object": 6, "guids": [guid]}
        })
    };
    let part_guid = "c0ffee00-0000-4000-8000-0000000000c1";
    let middle = dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "middle.f3d"},
        "document": {"name": "Middle", "root_component": "Middle"},
        "components": [{"name": "Middle", "is_root": true, "_f3d": {"object_id": 1}},
                       {"name": "Plate", "_f3d": {"object_id": 6}}],
        "occurrences": [
            {
                "transform": moved(1.0, 0.0, 0.0),
                "isReferencedComponent": true,
                "children": [],
                "_f3d": {"object_id": 20, "component_object": 1,
                         "external_key": "00000000000000aa", "external_document": PART_URN,
                         "guids": [part_guid], "placements": step(moved(1.0, 0.0, 0.0))}
            },
            plate(21, 5.0, "c0ffee00-0000-4000-8000-0000000000c2"),
            plate(22, 9.0, "c0ffee00-0000-4000-8000-0000000000c3")
        ],
        "timeline": {"items": [{
            "index": 0, "name": "Component Insert1", "objectType": "ComponentInsert",
            "_f3d": {"component": 1, "object_id": 50}
        }]},
        "_f3d": {"external_documents": [{"key": "00000000000000aa", "urn": PART_URN, "version": 1}]}
    }));
    let mut mid = Document::new(MockKernel::default());
    let mid_report = crate::import_design_with(
        &mut mid,
        &middle,
        &mut NoGeometry,
        &Options::default(),
        Some(&part_sources),
    );
    // The middle assembly knows its part's occurrence by its object id, and
    // the pin inside it by the part's.
    let level = &mid_report.components.object_guids.occurrences[&20];
    assert_eq!(level.guid, part_guid, "{}", mid_report.text());
    let inside = level.inside.as_ref().expect("the part's object ids");
    assert_eq!(inside.occurrences[&80].guid, pin_guid);
    assert!(
        mid_report.components.object_guids.occurrences[&21]
            .inside
            .is_none()
    );

    let sources = HashMap::from([(
        MID_URN.to_owned(),
        Referenced {
            document: &mid,
            occurrence_guids: &mid_report.components.occurrence_guids,
            occurrence_objects: &mid_report.components.occurrence_objects,
            name: "Middle".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &NO_ENTITIES,
            object_guids: Some(mid_report.components.object_guids.clone()),
        },
    )]);
    let design = dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "design.f3d"},
        "document": {"name": "Design", "root_component": "Design"},
        "components": [{"name": "Design", "is_root": true, "_f3d": {"object_id": 1}}],
        "occurrences": [{
            "transform": moved(0.0, 0.0, 0.0),
            "isReferencedComponent": true,
            "children": [],
            "_f3d": {"object_id": 10, "component_object": 1, "external_key": "00000000000000bb",
                     "external_document": MID_URN,
                     "guids": ["c0ffee00-0000-4000-8000-0000000000a1"],
                     "placements": step(moved(0.0, 0.0, 0.0)),
                     "inside_placements": [
                         {"inner": [21], "placements": step(moved(0.0, 5.0, 0.0))},
                         {"inner": [22], "placements": step(moved(0.0, 9.0, 0.0))},
                         {"inner": [20], "placements": step(moved(2.0, 0.0, 0.0))},
                         {"inner": [20, 80], "placements": step(moved(1.0, 0.0, 3.0))}
                     ]}
        }],
        "timeline": {"items": [{
            "index": 0, "name": "Component Insert1", "objectType": "ComponentInsert",
            "_f3d": {"component": 1, "object_id": 50}
        }]},
        "_f3d": {"external_documents": [{"key": "00000000000000bb", "urn": MID_URN, "version": 1}]}
    }));
    let mut doc = Document::new(MockKernel::default());
    let report = crate::import_design_with(
        &mut doc,
        &design,
        &mut NoGeometry,
        &Options::default(),
        Some(&sources),
    );
    let text = report.text();
    // Two positions: the part, and the pin below it (where the middle
    // assembly has it, but its part moves); none for the plates.
    let mut inside: Vec<Vec<String>> = report
        .components
        .positions_inside
        .iter()
        .filter(|p| p.index == crate::references::INSIDE_PLACEMENTS_INDEX)
        .map(|p| p.inner.clone())
        .collect();
    inside.sort();
    assert_eq!(
        inside,
        vec![
            vec![part_guid.to_owned()],
            vec![part_guid.to_owned(), pin_guid.to_owned()]
        ],
        "{text}"
    );
    let local = |guid: &str| {
        let o = report.components.occurrence_guids[guid]
            .iter()
            .filter_map(|o| doc.assembly().occurrence(*o))
            .find(|o| !o.parent.is_root())
            .unwrap_or_else(|| panic!("the copy's occurrence of {guid}\n{text}"));
        o.transform.translation
    };
    // The part 2 cm along x in the middle assembly's copy, the pin 1 cm
    // back from it and 3 cm up: 1 cm along x in the design, as the file
    // has it (2 cm before).
    let at = local(part_guid);
    assert!(
        (at[0] - 20.0).abs() < 1e-6 && at[1].abs() < 1e-6 && at[2].abs() < 1e-6,
        "{at:?}\n{text}"
    );
    let at = local(pin_guid);
    assert!(
        (at[0] + 10.0).abs() < 1e-6 && at[1].abs() < 1e-6 && (at[2] - 30.0).abs() < 1e-6,
        "{at:?}\n{text}"
    );
}
