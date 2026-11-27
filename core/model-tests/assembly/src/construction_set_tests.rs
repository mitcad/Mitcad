// SPDX-License-Identifier: MIT
//! Construction sets (mitcad#2514, GitHub mitcad/Mitcad#656) with the mock
//! kernel: a set created and made active collects a 3D guide sketch, a
//! construction plane, a sketch and the surface body made from it (not the
//! surface feature, not a solid body), each still editable and recomputed
//! as before; another set made active or none collects the new items while
//! the earlier ones keep their sets, and another component made active
//! leaves the set inactive; undo and redo take an item and its membership
//! together; the design file keeps the sets and a design without one has
//! none; members are added, taken out, deleted and the set renamed and
//! deleted, the items staying, none of it recomputing.

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::test_support::num;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::RegionKey;
use mitcad_model::transform::Transform;
use mitcad_model::{ConstructionMember, Document};
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

/// The fields `keys` of a command's result (the API adds its own, such as
/// `recomputed`, to every result).
fn picked(result: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .map(|k| ((*k).to_owned(), result[*k].clone()))
            .collect(),
    )
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string()).unwrap_err().to_string()
}

fn uid(value: &Value) -> FeatureUid {
    value["uid"].as_str().unwrap().parse().unwrap()
}

fn sets(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "construction_sets"}))
        .as_array()
        .unwrap()
        .clone()
}

/// A set's members as `kind name` (`feature 3DSketch1`, `body Body1`).
fn members(doc: &Document<MockKernel>, set: &str) -> Vec<String> {
    let all = sets(doc);
    let set = all
        .iter()
        .find(|s| s["name"] == set)
        .unwrap_or_else(|| panic!("no set {set}"));
    set["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            format!(
                "{} {}",
                m["kind"].as_str().unwrap(),
                m["name"].as_str().unwrap()
            )
        })
        .collect()
}

/// The name of the active set, if one is.
fn active(doc: &Document<MockKernel>) -> Option<String> {
    sets(doc)
        .iter()
        .find(|s| s["active"] == true)
        .map(|s| s["name"].as_str().unwrap().to_owned())
}

fn guide_sketch(doc: &mut Document<MockKernel>) -> FeatureUid {
    uid(&command(doc, json!({"cmd": "sketch3d.create"})))
}

fn plane(doc: &mut Document<MockKernel>, distance: f64) -> FeatureUid {
    uid(&command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": distance}}}),
    ))
}

fn sketch(doc: &mut Document<MockKernel>) -> FeatureUid {
    doc.add_sketch(SketchPlane::Xy).unwrap().uid
}

/// The definition of an extrusion of a sketch's region, `distance` long: a
/// surface (`surface`) or a solid.
fn extrude_def(sketch: FeatureUid, region: &RegionKey, surface: bool, distance: f64) -> Value {
    json!({"type": "extrude",
           "profiles": [{"sketch": sketch, "region": region}],
           "extent": {"type": "distance", "distance": distance},
           "operation": "new_body", "surface": surface})
}

/// The extrusion of a 20 x 10 rectangle of a new sketch, `distance` long:
/// a surface (`surface`) or a solid. The sketch, the feature and its body.
fn extrusion(
    doc: &mut Document<MockKernel>,
    surface: bool,
    distance: f64,
) -> (FeatureUid, FeatureUid, BodyUid) {
    let sketch = sketch(doc);
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(20.0), &num(10.0))
        .unwrap()
        .region;
    let def = extrude_def(sketch, &region, surface, distance);
    let feature = uid(&command(doc, json!({"cmd": "add_feature", "def": def})));
    (sketch, feature, BodyUid::new(feature, 0))
}

#[test]
fn new_guides_planes_sketches_and_surfaces_join_the_active_set() {
    let mut doc = Document::new(MockKernel::default());
    let added = command(
        &mut doc,
        json!({"cmd": "add_construction_set", "name": "Guides"}),
    );
    assert_eq!(
        picked(&added, &["name", "component", "active"]),
        json!({"name": "Guides", "component": "C0", "active": true})
    );
    assert_eq!(active(&doc).as_deref(), Some("Guides"));

    let guide = guide_sketch(&mut doc);
    let plane = plane(&mut doc, 10.0);
    let profile = sketch(&mut doc);
    let region = doc
        .add_rectangle(profile, [0.0, 0.0], &num(20.0), &num(10.0))
        .unwrap()
        .region;
    let def = extrude_def(profile, &region, true, 15.0);
    let surface = uid(&command(
        &mut doc,
        json!({"cmd": "add_feature", "def": def}),
    ));
    let sheet = BodyUid::new(surface, 0);
    let names: Vec<String> = [guide, plane, profile]
        .iter()
        .map(|f| format!("feature {}", doc.feature(*f).unwrap().name))
        .chain([format!("body {}", doc.body_name(sheet))])
        .collect();
    assert_eq!(members(&doc, "Guides"), names);
    // The surface feature stays in the timeline only: its body is the
    // member.
    assert!(
        !doc.construction_sets()[0]
            .members
            .contains(&ConstructionMember::Feature(surface))
    );
    // A solid made while the set is active is no surface: it stays in the
    // Bodies folder (its sketch joins).
    let (solid_sketch, _, solid) = extrusion(&mut doc, false, 5.0);
    let list = &doc.construction_sets()[0].members;
    assert!(list.contains(&ConstructionMember::Feature(solid_sketch)));
    assert!(!list.contains(&ConstructionMember::Body(solid)));

    // The inputs stay editable and the geometry recomputes as before: the
    // plane moved, the surface made longer, each still in the set.
    let before = doc.recompute_count();
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": plane, "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 25}}}),
    );
    let def = extrude_def(profile, &region, true, 30.0);
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": surface, "def": def}),
    );
    assert!(doc.recompute_count() > before);
    for feature in [plane, surface] {
        assert!(doc.status(feature).unwrap().is_ok(), "{feature}");
    }
    assert!(doc.body_component(sheet).is_some());
    let list = &doc.construction_sets()[0].members;
    for member in [
        ConstructionMember::Feature(guide),
        ConstructionMember::Feature(plane),
        ConstructionMember::Feature(profile),
        ConstructionMember::Body(sheet),
    ] {
        assert!(list.contains(&member), "{member:?}");
    }
    // The items keep their component and their place in the timeline.
    assert_eq!(doc.feature(plane).unwrap().component, ComponentUid::ROOT);
    assert_eq!(doc.state().position(plane), Some(1));
}

#[test]
fn another_set_or_none_takes_the_new_items_and_the_earlier_keep_theirs() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_construction_set", "name": "Guides"}),
    );
    let first = guide_sketch(&mut doc);
    // A new set is the active one at once; Guides keeps its sketch.
    command(
        &mut doc,
        json!({"cmd": "add_construction_set", "name": "Skins"}),
    );
    assert_eq!(active(&doc).as_deref(), Some("Skins"));
    let datum = plane(&mut doc, 5.0);
    // Back to Guides.
    let activated = command(
        &mut doc,
        json!({"cmd": "activate_construction_set", "name": "Guides"}),
    );
    assert_eq!(
        picked(&activated, &["name", "component"]),
        json!({"name": "Guides", "component": "C0"})
    );
    let second = sketch(&mut doc);
    // None active: a new sketch is in no set.
    assert_eq!(
        command(&mut doc, json!({"cmd": "deactivate_construction_set"}))["name"],
        json!("Guides")
    );
    assert_eq!(active(&doc), None);
    let loose = sketch(&mut doc);
    let name = |doc: &Document<MockKernel>, f: FeatureUid| {
        format!("feature {}", doc.feature(f).unwrap().name)
    };
    assert_eq!(
        members(&doc, "Guides"),
        [name(&doc, first), name(&doc, second)]
    );
    assert_eq!(members(&doc, "Skins"), [name(&doc, datum)]);
    assert!(
        doc.construction_sets()
            .iter()
            .all(|s| !s.members.contains(&ConstructionMember::Feature(loose)))
    );
    // Deactivating with none active changes nothing.
    assert_eq!(
        command(&mut doc, json!({"cmd": "deactivate_construction_set"}))["name"],
        json!(null)
    );

    // Another component made active leaves the set inactive in that step:
    // its items go to that component's usual places.
    command(
        &mut doc,
        json!({"cmd": "activate_construction_set", "name": "Skins"}),
    );
    let (rail, _) = doc
        .create_component(Some("Rail"), Transform::IDENTITY, true)
        .unwrap();
    assert_eq!(active(&doc), None);
    let rail_sketch = sketch(&mut doc);
    assert_eq!(doc.feature(rail_sketch).unwrap().component, rail);
    assert_eq!(members(&doc, "Skins"), [name(&doc, datum)]);
    // A set of Rail, made active, activates Rail; activating Guides again
    // goes back to the root.
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let added = command(
        &mut doc,
        json!({"cmd": "add_construction_set", "name": "Rail guides", "component": "Rail"}),
    );
    assert_eq!(added["component"], json!(rail));
    assert_eq!(doc.active_component(), rail);
    let rail_guide = guide_sketch(&mut doc);
    assert_eq!(members(&doc, "Rail guides"), [name(&doc, rail_guide)]);
    command(
        &mut doc,
        json!({"cmd": "activate_construction_set", "name": "Guides"}),
    );
    assert_eq!(doc.active_component(), ComponentUid::ROOT);
    // A set's component's item only: a root sketch is not Rail's.
    let refused = rejected(
        &mut doc,
        json!({"cmd": "set_construction_set_members", "name": "Rail guides",
               "add": [{"feature": loose}]}),
    );
    assert!(refused.contains("is not in Rail"), "{refused}");
}

#[test]
fn undo_and_redo_take_an_item_and_its_membership_together() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_construction_set", "name": "Guides"}),
    );
    let guide = guide_sketch(&mut doc);
    let (profile, surface, sheet) = extrusion(&mut doc, true, 15.0);
    let all = [
        ConstructionMember::Feature(guide),
        ConstructionMember::Feature(profile),
        ConstructionMember::Body(sheet),
    ];
    assert_eq!(doc.construction_sets()[0].members, all);

    // The surface, then its sketch, undone: they leave the set with
    // their undo steps, and come back into it with redo.
    doc.undo().unwrap();
    assert!(doc.feature(surface).is_none());
    assert_eq!(doc.construction_sets()[0].members, all[..2]);
    while doc.feature(profile).is_some() {
        doc.undo().unwrap();
    }
    assert_eq!(doc.construction_sets()[0].members, all[..1]);
    while doc.feature(surface).is_none() {
        doc.redo().unwrap();
    }
    assert_eq!(doc.construction_sets()[0].members, all);

    // A membership change and an activation are undo steps of their own.
    command(
        &mut doc,
        json!({"cmd": "set_construction_set_members", "name": "Guides",
               "remove": [{"feature": guide}]}),
    );
    command(&mut doc, json!({"cmd": "deactivate_construction_set"}));
    assert_eq!(
        doc.undo().as_deref(),
        Some("Deactivate Construction Set Guides")
    );
    assert_eq!(active(&doc).as_deref(), Some("Guides"));
    assert_eq!(
        doc.undo().as_deref(),
        Some("Change Construction Set Guides")
    );
    assert!(
        doc.construction_sets()[0]
            .members
            .contains(&ConstructionMember::Feature(guide))
    );
    // Undoing the set's creation takes the set; the items it collected
    // after it are undone first.
    while doc.construction_sets().len() == 1 {
        doc.undo().unwrap();
    }
    assert!(doc.features().next().is_none());
}

#[test]
fn the_design_file_keeps_the_sets_and_a_design_without_one_has_none() {
    let mut doc = Document::new(MockKernel::default());
    let guide = guide_sketch(&mut doc);
    // An existing design: no sets, nothing in the file.
    assert!(!doc.to_json().contains("construction_sets"));
    assert!(sets(&doc).is_empty());
    command(
        &mut doc,
        json!({"cmd": "add_construction_set", "name": "Guides"}),
    );
    let datum = plane(&mut doc, 10.0);
    let (profile, _, sheet) = extrusion(&mut doc, true, 15.0);
    command(
        &mut doc,
        json!({"cmd": "add_construction_set", "name": "Skins", "activate": false}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_construction_set_members", "name": "Skins",
               "add": [{"body": sheet}, {"feature": guide}]}),
    );
    // An item added to Skins left Guides.
    assert_eq!(
        doc.construction_sets()[0].members,
        [
            ConstructionMember::Feature(datum),
            ConstructionMember::Feature(profile)
        ]
    );
    let json = doc.to_json();
    let file: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        file["construction_sets"],
        json!([
            {"name": "Guides", "active": true,
             "members": [{"feature": datum}, {"feature": profile}]},
            {"name": "Skins", "members": [{"body": sheet}, {"feature": guide}]}
        ])
    );
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    loaded.recompute();
    assert_eq!(sets(&loaded), sets(&doc));
    // Reopened, the active set still collects.
    let more = sketch(&mut loaded);
    assert!(
        loaded.construction_sets()[0]
            .members
            .contains(&ConstructionMember::Feature(more))
    );

    // A member whose item the file does not have is left out when read.
    let mut gone = file.clone();
    gone["construction_sets"][1]["members"]
        .as_array_mut()
        .unwrap()
        .push(json!({"feature": "F99"}));
    let loaded = Document::from_json(&gone.to_string(), MockKernel::default()).unwrap();
    assert_eq!(loaded.construction_sets()[1].members.len(), 2);
    // A name listed twice, and two active sets, are refused.
    let mut twice = file.clone();
    let first = twice["construction_sets"][0].clone();
    twice["construction_sets"]
        .as_array_mut()
        .unwrap()
        .push(first);
    assert!(Document::from_json(&twice.to_string(), MockKernel::default()).is_err());
    let mut two_active = file;
    two_active["construction_sets"][1]["active"] = json!(true);
    assert!(Document::from_json(&two_active.to_string(), MockKernel::default()).is_err());
}

#[test]
fn members_and_sets_change_without_a_recompute_and_the_items_stay() {
    let mut doc = Document::new(MockKernel::default());
    let (_, solid_feature, solid) = extrusion(&mut doc, false, 5.0);
    command(&mut doc, json!({"cmd": "add_construction_set"}));
    // The next free name.
    assert_eq!(active(&doc).as_deref(), Some("ConstructionSet1"));
    let guide = guide_sketch(&mut doc);
    let datum = plane(&mut doc, 10.0);
    let recomputes = doc.recompute_count();
    let timeline = query(&doc, json!({"query": "timeline"}));

    // Only sketches, construction geometry and bodies of its component.
    let refused = rejected(
        &mut doc,
        json!({"cmd": "set_construction_set_members", "name": "ConstructionSet1",
               "add": [{"feature": solid_feature}]}),
    );
    assert!(
        refused.contains("not a sketch or construction geometry"),
        "{refused}"
    );
    command(
        &mut doc,
        json!({"cmd": "set_construction_set_members", "name": "ConstructionSet1",
               "add": [{"body": solid}], "remove": [{"feature": datum}]}),
    );
    assert_eq!(
        doc.construction_sets()[0].members,
        [
            ConstructionMember::Feature(guide),
            ConstructionMember::Body(solid)
        ]
    );
    // A name another set has is refused, an empty one too.
    command(
        &mut doc,
        json!({"cmd": "add_construction_set", "name": "Skins"}),
    );
    let refused = rejected(
        &mut doc,
        json!({"cmd": "rename_construction_set", "name": "ConstructionSet1", "new_name": "Skins"}),
    );
    assert!(
        refused.contains("a construction set Skins exists"),
        "{refused}"
    );
    assert!(
        doc.command(
            &json!({"cmd": "rename_construction_set", "name": "Skins", "new_name": " "})
                .to_string()
        )
        .is_err()
    );
    command(
        &mut doc,
        json!({"cmd": "rename_construction_set", "name": "ConstructionSet1", "new_name": "Guides"}),
    );
    // Deleting a set deletes none of its items.
    command(
        &mut doc,
        json!({"cmd": "delete_construction_set", "name": "Guides"}),
    );
    assert_eq!(doc.construction_sets().len(), 1);
    assert_eq!(query(&doc, json!({"query": "timeline"})), timeline);
    assert_eq!(doc.recompute_count(), recomputes);
    for label in [
        "Delete Construction Set Guides",
        "Rename Construction Set ConstructionSet1",
        "Add Construction Set Skins",
        "Change Construction Set ConstructionSet1",
    ] {
        assert_eq!(doc.undo().as_deref(), Some(label));
    }

    // A deleted item leaves its set in the step that deletes it.
    doc.delete_feature(guide, false).unwrap();
    assert_eq!(
        doc.construction_sets()[0].members,
        [ConstructionMember::Feature(datum)]
    );
    doc.undo().unwrap();
    assert_eq!(
        doc.construction_sets()[0].members,
        [
            ConstructionMember::Feature(guide),
            ConstructionMember::Feature(datum)
        ]
    );
    // The query lists each member's kind, type and state.
    let listed = &sets(&doc)[0]["members"][0];
    assert_eq!(listed["kind"], "feature");
    assert_eq!(listed["type"], "spatial_sketch");
    assert_eq!(listed["exists"], true);
    assert_eq!(listed["active"], true);
}
