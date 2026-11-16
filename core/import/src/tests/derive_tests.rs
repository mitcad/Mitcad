// SPDX-License-Identifier: MIT
//! Derives (mitcad#324) on synthetic dumps with the mock kernel: a part
//! derives the block of another document from an assembly both are in, the
//! block placed where the derive puts it; a derive that lists nothing; a
//! derive whose document is not found.

use super::*;
use crate::references::Referenced;
use std::collections::BTreeMap;

const BLOCK_URN: &str = "urn:mitcad.test:fs.file:vf.block?version=1";
const BLOCK_GUID: &str = "9016787c-cb67-4d88-bd1a-387e9083cdb3";
const ASSEMBLY_URN: &str = "urn:mitcad.test:fs.file:vf.assembly?version=10";

/// A part whose first item derives `entities` from the assembly, which
/// places the part turned half a turn about x at (2, 1, 0) cm.
fn derive_dump(entities: Value) -> Dump {
    dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "part.f3d"},
        "document": {"name": "Part", "root_component": "Part"},
        "components": [{"name": "Part", "is_root": true, "_f3d": {"object_id": 1}}],
        "timeline": {"items": [{
            "index": 0, "name": "DerivedContext1", "objectType": "DerivedContext",
            "_f3d": {"component": 1},
            "detail": {
                "source": {"kind": "design", "name": "Assembly",
                           "_f3d": {"key": "00000000000000bb", "urn": ASSEMBLY_URN,
                                    "document_guid": "74c61ef7-0894-4b42-8532-24fb6b04aa22",
                                    "component_object": 3}},
                "placements": [
                    {"transform": [[1.0, 0.0, 0.0, 2.0], [0.0, -1.0, 0.0, 1.0],
                                   [0.0, 0.0, -1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
                    {}
                ],
                "entities": entities
            }
        }]},
        "_f3d": {"external_documents": [
            {"key": "00000000000000aa", "urn": BLOCK_URN, "version": 1,
             "document_guid": BLOCK_GUID},
            {"key": "00000000000000bb", "urn": ASSEMBLY_URN, "version": 10,
             "document_guid": "74c61ef7-0894-4b42-8532-24fb6b04aa22"}
        ]}
    }))
}

/// The block's body (object 2495 in its document), its occurrence in the
/// assembly at (-4, 1, 0) cm.
fn block_entity() -> Value {
    json!([{
        "kind": "body",
        "placement": {"transform": [[1.0, 0.0, 0.0, -4.0], [0.0, 1.0, 0.0, 1.0],
                                    [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
        "_f3d": {"key": "00000000000000aa", "urn": BLOCK_URN, "document_guid": BLOCK_GUID,
                 "object_id": 2495}
    }])
}

#[test]
fn a_derive_copies_the_body_of_another_document_where_it_puts_it() {
    let mut block = Document::new(MockKernel::default());
    let block_report = import(&mut block, &block_dump());
    let body = block.bodies()[0].uid;
    let entities = BTreeMap::from([(2495, body)]);
    let stored = HashMap::new();
    let sources = HashMap::from([(
        BLOCK_URN.to_owned(),
        Referenced {
            document: &block,
            occurrence_guids: &block_report.components.occurrence_guids,
            occurrence_objects: &block_report.components.occurrence_objects,
            name: "Block".to_owned(),
            stored_bodies: &stored,
            entity_bodies: &entities,
        },
    )]);
    let mut doc = Document::new(MockKernel::default());
    let report = crate::import_design_with(
        &mut doc,
        &derive_dump(block_entity()),
        &mut NoGeometry,
        &Options::default(),
        Some(&sources),
    );
    let item = &report.items[0];
    assert_eq!(item.outcome, Outcome::Parametric, "{item:?}");
    // The block's document stands as a hidden component, filled from its
    // import, placed where the derive puts the body: the part is turned
    // half a turn about x at (2, 1, 0) cm in the assembly and the block is
    // at (-4, 1, 0) cm, so it is at (-60, 0, 0) mm in the part, turned.
    let derived = &report.components.derived;
    assert_eq!(derived.len(), 1, "{:?}", report.components);
    let source = derived[0].clone();
    assert_eq!(doc.assembly().name(source.component), "Block");
    let occurrence = doc.assembly().occurrence(source.occurrence).unwrap();
    assert!(!occurrence.visible);
    assert_eq!(occurrence.parent, ComponentUid::ROOT);
    let t = occurrence.transform;
    assert!(
        (t.translation[0] + 60.0).abs() < 1e-9
            && t.translation[1].abs() < 1e-9
            && t.translation[2].abs() < 1e-9,
        "{t:?}"
    );
    assert!((t.linear[1][1] + 1.0).abs() < 1e-9 && (t.linear[2][2] + 1.0).abs() < 1e-9);
    // The item is Mitcad's derive of the copy's body, through that occurrence.
    let uid: FeatureUid = item.features[0].parse().unwrap();
    let entry = doc.feature(uid).unwrap();
    assert_eq!(entry.component, ComponentUid::ROOT);
    let FeatureDef::Derive(def) = entry.def.clone() else {
        panic!("a derive: {:?}", entry.def);
    };
    assert_eq!(def.bodies.len(), 1);
    let link = &def.links[&def.bodies[0]];
    assert_eq!(link.source.0, vec![source.occurrence]);
    assert_eq!(doc.body_component(def.bodies[0]), Some(source.component));
    doc.recompute();
    let copy = mitcad_model::BodyUid::new(uid, 0);
    assert_eq!(doc.body_component(copy), Some(ComponentUid::ROOT));
    let history = doc.body_shape(copy).unwrap().history.clone();
    assert!(history.ends_with(",-60,0,0,inst1)"), "{history}");
}

#[test]
fn a_derive_that_lists_nothing_brings_nothing() {
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &derive_dump(json!([])));
    let item = &report.items[0];
    assert_eq!(item.outcome, Outcome::Skipped, "{item:?}");
    assert_eq!(item.note.as_deref(), Some(crate::derive::BRINGS_NOTHING));
    assert!(report.components.derived.is_empty());
    assert!(doc.assembly().occurrences.is_empty());
}

#[test]
fn a_derive_whose_document_is_not_found_says_so() {
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &derive_dump(block_entity()));
    let item = &report.items[0];
    assert_ne!(item.outcome, Outcome::Parametric, "{item:?}");
    let note = item.note.clone().unwrap_or_default();
    assert!(note.contains("which was not imported"), "{note}");
    assert!(note.contains(BLOCK_URN), "{note}");
    // The document is one the design refers to, so the caller looks for it.
    let documents = crate::components::external_documents(&derive_dump(block_entity()));
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].urn, BLOCK_URN);
    assert_eq!(documents[0].guid.as_deref(), Some(BLOCK_GUID));
    let sources = crate::derive::source_documents(&derive_dump(block_entity()));
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].urn, ASSEMBLY_URN);
}
