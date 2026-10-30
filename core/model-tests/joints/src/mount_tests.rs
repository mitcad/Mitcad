// SPDX-License-Identifier: MIT
//! Mount frames of any part (mitcad#525) with the mock kernel and a
//! library in memory: a joint origin's mount checked and kept in the
//! file; a linked part's frames as joint origins of its component, a
//! copy's as the design's own; a part placed on a face, into a hole and
//! on a grid's cells with the joint its frame asks for, carried by what
//! it stands on; a taken cell refused; the preview and the frames
//! queries.
//!
//! The mock's blocks are boxes from their sketch's corner: a plate 40 x
//! 40 x 10 has its top face's middle at (20, 20, 10), the normal +Z.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Value, json};

use mitcad_model::datum::{CurveGeometry, Vec3};
use mitcad_model::features::joint::{MountKind, MountSpec};
use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::file::MemoryStore;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::library::{LibraryRequest, LibrarySource, LinkResolver};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::EdgeName;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, GeomRef};
use mitcad_model_api::DocumentApi;

const V1: &str = "1111111111111111111111111111111111111111";

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let text = query.to_string();
    match doc.query(&text) {
        Ok(answer) => serde_json::from_str(&answer).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string()).unwrap_err().to_string()
}

fn assert_near(a: Vec3, b: Vec3) {
    assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-6), "{a:?} != {b:?}");
}

/// A design of one box `size` from the origin and joint origins on fixed
/// planes, each `(origin, normal, mount)`.
fn part(size: [f64; 3], frames: &[(Vec3, Vec3, Value)]) -> String {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(size[0]), &num(size[1]))
        .unwrap()
        .region;
    doc.add_feature(&extrude(sketch, &[&region], size[2], "new_body", &[]), None)
        .unwrap();
    for (origin, normal, mount) in frames {
        command(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "joint_origin",
                "geometry": {"origin": origin, "normal": normal, "x_axis": [1, 0, 0]},
                "mount": mount}}),
        );
    }
    doc.to_json()
}

/// The library in memory: a design and a name per component.
struct Parts {
    designs: BTreeMap<String, (String, String)>,
}

impl LinkResolver for Parts {
    fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
        let (name, text) = self
            .designs
            .get(&request.component)
            .cloned()
            .ok_or_else(|| format!("the library has no {}", request.component))?;
        Ok(LibrarySource {
            text,
            store: MemoryStore::new(),
            rev: V1.to_owned(),
            label: "v1.0.0".to_owned(),
            library: "test-mounts".to_owned(),
            url: "https://example.org/mounts.git".to_owned(),
            library_name: "Test mounts".to_owned(),
            component: request.component.clone(),
            authors: vec!["Mount makers".to_owned()],
            path: format!("parts/{}.mitcad", request.component),
            name,
            standard: String::new(),
            license: "CC0-1.0".to_owned(),
            designation: String::new(),
            supplier: None,
            part_number: None,
            supplier_url: None,
            build: None,
            rows: Default::default(),
        })
    }
}

/// A lamp (10 x 10 x 5, free on a face from its bottom), a block whose
/// bottom is fixed on a face (`mount:base`, BUILD's form of a name), a
/// screw (4 x 4 x 20, its shaft frame on its axis at its top, z down the
/// shank, cylindrical), a baseplate (a 3 x 2 grid of 42 mm on its top)
/// and bins of 1 x 1 and 2 x 1 cells (their feet in the middle of their
/// bottoms).
fn library() -> Arc<Parts> {
    let mut designs = BTreeMap::new();
    let mut add = |id: &str, name: &str, text: String| {
        designs.insert(id.to_owned(), (name.to_owned(), text));
    };
    add(
        "lamp",
        "Lamp",
        part(
            [10.0, 10.0, 5.0],
            &[(
                [5.0, 5.0, 0.0],
                [0.0, 0.0, -1.0],
                json!({"name": "base", "kind": "face", "joint": "planar"}),
            )],
        ),
    );
    // A frame named after itself, without a mount (BUILD's parts).
    let mut block =
        Document::from_json(&part([10.0, 10.0, 5.0], &[]), MockKernel::default()).unwrap();
    command(
        &mut block,
        json!({"cmd": "add_feature", "name": "mount:base", "def": {"type": "joint_origin",
            "geometry": "xy", "flip": true}}),
    );
    add("block", "Block", block.to_json());
    add(
        "screw",
        "Screw",
        part(
            [4.0, 4.0, 20.0],
            &[(
                [2.0, 2.0, 20.0],
                [0.0, 0.0, -1.0],
                json!({"name": "head", "kind": "shaft", "joint": "cylindrical"}),
            )],
        ),
    );
    add(
        "baseplate",
        "Baseplate",
        part(
            [126.0, 84.0, 5.0],
            &[(
                [0.0, 0.0, 5.0],
                [0.0, 0.0, 1.0],
                json!({"name": "grid", "kind": "grid", "pitch": [42, 42], "cells": [3, 2]}),
            )],
        ),
    );
    add(
        "bin1x1",
        "Bin 1x1",
        part(
            [42.0, 42.0, 21.0],
            &[(
                [21.0, 21.0, 0.0],
                [0.0, 0.0, -1.0],
                json!({"name": "foot", "kind": "foot", "footprint": [1, 1], "pitch": [42, 42]}),
            )],
        ),
    );
    add(
        "bin2x1",
        "Bin 2x1",
        part(
            [84.0, 42.0, 21.0],
            &[(
                [42.0, 21.0, 0.0],
                [0.0, 0.0, -1.0],
                json!({"name": "foot", "kind": "foot", "footprint": [2, 1], "pitch": [42, 42]}),
            )],
        ),
    );
    Arc::new(Parts { designs })
}

fn library_part(component: &str) -> Value {
    json!({"id": "test-mounts", "url": "https://example.org/mounts.git", "rev": V1,
           "component": component})
}

/// A design with a plate (`Plate:1`, a 40 x 40 x 10 block at the origin)
/// and the library; the root is active.
struct Bench {
    doc: Document<MockKernel>,
    plate_body: BodyUid,
}

fn bench() -> Bench {
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(library()));
    doc.create_component(Some("Plate"), Transform::IDENTITY, true)
        .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(40.0), &num(40.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 10.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    doc.activate_component(ComponentUid::ROOT).unwrap();
    Bench {
        doc,
        plate_body: BodyUid::new(uid, 0),
    }
}

impl Bench {
    fn top(&self) -> Value {
        let face = self
            .doc
            .body_shape(self.plate_body)
            .unwrap()
            .faces
            .iter()
            .find(|f| f.role == "end")
            .unwrap()
            .to_string();
        json!({"body": self.plate_body, "face": face})
    }

    fn placement(&self, occurrence: &Value) -> Transform {
        let uid: OccurrenceUid = occurrence.as_str().unwrap().parse().unwrap();
        self.doc.placement(uid).unwrap()
    }

    fn joint(&self, uid: &Value) -> Value {
        let joints = query(&self.doc, json!({"query": "joints"}));
        joints["joints"]
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["uid"] == *uid)
            .cloned()
            .unwrap_or_else(|| panic!("no joint {uid} in {joints}"))
    }
}

fn mount_of(doc: &Document<MockKernel>, uid: &Value) -> Option<MountSpec> {
    let uid: FeatureUid = uid.as_str().unwrap().parse().unwrap();
    match &doc.feature(uid).unwrap().def {
        FeatureDef::JointOrigin(def) => def.mount.clone(),
        _ => None,
    }
}

#[test]
fn a_joint_origins_mount_is_checked_and_kept_in_the_file() {
    let mut doc = Document::new(MockKernel::default());
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "joint_origin", "geometry": "xy",
            "mount": {"name": "grid", "kind": "grid", "pitch": [42, 42], "cells": [3, 2]}}}),
    );
    let text = doc.to_json();
    assert!(text.contains("\"mount\""), "{text}");
    let reopened = Document::from_json(&text, MockKernel::default()).unwrap();
    let uid: FeatureUid = added["uid"].as_str().unwrap().parse().unwrap();
    let FeatureDef::JointOrigin(def) = &reopened.feature(uid).unwrap().def else {
        panic!("a joint origin");
    };
    let mount = def.mount.clone().unwrap();
    assert_eq!(mount.kind, MountKind::Grid);
    assert_eq!(mount.cells, Some([3, 2]));
    assert_eq!(mount.pitch, Some([42.0, 42.0]));
    // A rigid joint is not written.
    assert!(!text.contains("\"joint\": \"rigid\""), "{text}");

    for (mount, expected) in [
        (
            json!({"name": "grid", "kind": "grid", "pitch": [42, 42]}),
            "a grid gives its cells",
        ),
        (
            json!({"name": "grid", "kind": "grid", "cells": [3, 2]}),
            "a grid gives its pitch",
        ),
        (
            json!({"name": "foot", "kind": "foot"}),
            "a foot gives its footprint in cells",
        ),
        (
            json!({"name": "foot", "kind": "foot", "footprint": [0, 1]}),
            "a foot covers at least one cell each way",
        ),
        (
            json!({"name": "foot", "kind": "foot", "footprint": [1, 1], "joint": "planar"}),
            "a foot frame's joint is rigid, not planar",
        ),
        (
            json!({"name": "base", "kind": "face", "joint": "cylindrical"}),
            "a face frame's joint is rigid, planar, revolute, not cylindrical",
        ),
        (
            json!({"name": "base", "kind": "face", "cells": [1, 1]}),
            "a face frame has no cells",
        ),
        (
            json!({"name": "a b", "kind": "face"}),
            "'a b' is no mount frame name",
        ),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "joint_origin", "geometry": "xy",
                   "mount": mount}}),
        );
        assert!(error.contains(expected), "{expected}: {error}");
    }
    let spec = MountSpec::new("hole_1", MountKind::Hole);
    assert!(spec.check().is_ok());
}

#[test]
fn a_linked_part_keeps_its_frames_as_joint_origins_and_a_copy_its_own() {
    let mut b = bench();
    let lamp = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("lamp")}),
    );
    // Dropped in at the origin, as before: no joint.
    assert_eq!(lamp["joint"], Value::Null);
    assert_eq!(lamp["mount"], Value::Null);
    assert_near(b.placement(&lamp["occurrence"]).translation, [0.0; 3]);
    let frames = lamp["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 1, "{lamp}");
    assert_eq!(frames[0]["name"], "base");
    assert_eq!(frames[0]["kind"], "face");
    assert_eq!(frames[0]["joint"], "planar");
    assert_eq!(frames[0]["occurrence"], "Lamp:1");
    // A joint origin of the linked part on a fixed plane where the design
    // has it, named after the frame.
    let uid: FeatureUid = frames[0]["feature"].as_str().unwrap().parse().unwrap();
    let entry = b.doc.feature(uid).unwrap();
    assert_eq!(entry.name, "mount:base");
    let FeatureDef::JointOrigin(def) = &entry.def else {
        panic!("a joint origin");
    };
    assert!(
        matches!(def.geometry, GeomRef::FixedPlane { origin, normal, .. }
            if origin == [5.0, 5.0, 0.0] && normal == [0.0, 0.0, -1.0]),
        "{:?}",
        def.geometry
    );
    // The frames query: in the design, through the part's placement.
    let listed = query(
        &b.doc,
        json!({"query": "mount_frames", "occurrence": "Lamp:1"}),
    );
    assert_eq!(listed["frames"][0]["name"], "base");
    assert_eq!(listed["frames"][0]["feature"], frames[0]["feature"]);
    assert_eq!(
        listed["frames"][0]["frame"]["origin"],
        json!([5.0, 5.0, 0.0])
    );
    // A second lamp's frame has a name of its own.
    let second = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("lamp")}),
    );
    let uid: FeatureUid = second["frames"][0]["feature"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(b.doc.feature(uid).unwrap().name, "mount:base (2)");

    // A copy: the design's own joint origin, with its mount; the block's
    // frame named `mount:base` gets a mount of its kind, face.
    let copy = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("block"), "link": false}),
    );
    let frames = copy["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 1, "{copy}");
    let mount = mount_of(&b.doc, &frames[0]["feature"]).unwrap();
    assert_eq!(mount.name, "base");
    assert_eq!(mount.kind, MountKind::Face);

    // Saved and opened again, the frames are there.
    let text = b.doc.to_json();
    let reopened = Document::from_json(&text, MockKernel::default()).unwrap();
    assert!(mount_of(&reopened, &frames[0]["feature"]).is_some());
}

#[test]
fn a_part_free_on_a_face_lies_where_clicked_and_moves_with_the_face() {
    let mut b = bench();
    let top = b.top();
    let lamp = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("lamp"),
               "mount": {"at": {"occurrence": "Plate:1", "geometry": top,
                                "point": [30, 10, 10]}}}),
    );
    assert_eq!(lamp["mount"]["frame"], "base");
    assert_eq!(lamp["mount"]["kind"], "face");
    assert_eq!(lamp["mount"]["joint"], "planar");
    assert_eq!(lamp["mount"]["target"]["occurrence"], "Plate:1");
    assert_eq!(b.doc.undo_label(), Some("Insert Lamp"));
    // Its frame (the middle of its bottom) on the click: the lamp stands
    // on the plate's top, not turned.
    let placement = b.placement(&lamp["occurrence"]);
    assert_near(placement.translation, [25.0, 5.0, 10.0]);
    assert_near(placement.apply_vector([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
    let joint = b.joint(&lamp["joint"]);
    assert_eq!(joint["kind"], "planar");
    assert_eq!(joint["solved"], true, "{joint}");
    // Free to slide and turn on the face: three motions.
    let dof = query(
        &b.doc,
        json!({"query": "joint_dof", "occurrence": "Lamp:1"}),
    );
    assert_eq!(dof["dof"], 3, "{dof}");
    // A drag moves it within the face, never off it. The plate grounded
    // for it: free, the plate moved (and tilted) with the lamp.
    command(
        &mut b.doc,
        json!({"cmd": "ground_occurrence", "occurrence": "Plate:1"}),
    );
    let dragged = query(
        &b.doc,
        json!({"query": "joint_drag", "occurrence": "Lamp:1", "point": [30, 10, 10],
               "target": [40, 20, 30]}),
    );
    let moved = dragged["placements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "Lamp:1")
        .unwrap_or_else(|| panic!("the lamp moves: {dragged}"));
    let rows = &moved["transform"];
    assert!((rows[2][3].as_f64().unwrap() - 10.0).abs() < 1e-6, "{rows}");
    assert!((rows[0][3].as_f64().unwrap() - 35.0).abs() < 1e-2, "{rows}");
    assert!((rows[1][3].as_f64().unwrap() - 15.0).abs() < 1e-2, "{rows}");
    // Moving the plate carries it.
    command(
        &mut b.doc,
        json!({"cmd": "ground_occurrence", "occurrence": "Plate:1", "grounded": false}),
    );
    command(
        &mut b.doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": "Plate:1",
               "transform": {"translation": [0, 0, 100]}}),
    );
    assert_near(
        b.placement(&lamp["occurrence"]).translation,
        [25.0, 5.0, 110.0],
    );

    // Turned and lifted: the frame's z is the joint's.
    let mut b = bench();
    let top = b.top();
    let lamp = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("lamp"),
               "mount": {"at": {"occurrence": "Plate:1", "geometry": top,
                                "point": [20, 20, 10]},
                         "rotate": "90 deg", "offset": 2}}),
    );
    let placement = b.placement(&lamp["occurrence"]);
    // The bottom's middle (5, 5, 0) at (20, 20, 12), turned a quarter.
    assert_near(placement.apply_point([5.0, 5.0, 0.0]), [20.0, 20.0, 12.0]);
    assert_near(placement.apply_vector([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]);

    // A frame fixed on a face: a rigid joint.
    let top = b.top();
    let block = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("block"),
               "mount": {"at": {"occurrence": "Plate:1", "geometry": top,
                                "point": [5, 5, 10]}}}),
    );
    assert_eq!(block["mount"]["joint"], "rigid");
    assert_eq!(b.joint(&block["joint"])["kind"], "rigid");
    assert_near(
        b.placement(&block["occurrence"]).translation,
        [5.0, 5.0, 10.0],
    );

    // A part that lies on a face does not go on an axis.
    let error = refused(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("lamp"),
               "mount": {"at": {"occurrence": "Plate:1", "geometry": "z"}}}),
    );
    assert!(error.contains("Lamp lies on a face"), "{error}");
    // A frame the part has not.
    let top = b.top();
    let error = refused(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("lamp"),
               "mount": {"frame": "wall", "at": {"occurrence": "Plate:1", "geometry": top}}}),
    );
    assert!(
        error.contains("Lamp has no mount frame wall (its frames: base)"),
        "{error}"
    );
}

#[test]
fn a_screw_goes_into_a_hole_by_its_rim() {
    let mut b = bench();
    let shape = b.doc.body_shape(b.plate_body).unwrap().clone();
    let (x, y) = &shape.edges[0];
    let rim = format!("E{{{x}|{y}}}")
        .parse::<EdgeName>()
        .unwrap()
        .to_string();
    b.doc.kernel().edge_curves.borrow_mut().insert(
        rim.clone(),
        CurveGeometry::Circle {
            center: [20.0, 20.0, 10.0],
            normal: [0.0, 0.0, -1.0],
            radius: 2.0,
            start: [22.0, 20.0, 10.0],
            end: [22.0, 20.0, 10.0],
        },
    );
    let at = json!({"occurrence": "Plate:1", "geometry": {"body": b.plate_body, "edge": rim},
                    "toward": [0, 0, 1]});
    // The preview tells where it goes and changes nothing.
    let components = b.doc.assembly().components.len();
    let preview = query(
        &b.doc,
        json!({"query": "mount_preview", "library": library_part("screw"),
               "mount": {"at": at.clone()}}),
    );
    assert_eq!(preview["problems"], json!([]), "{preview}");
    assert_eq!(preview["frames"][0]["name"], "head");
    assert_eq!(preview["kind"], "shaft");
    assert_eq!(preview["joint"], "cylindrical");
    assert_eq!(preview["at"]["origin"], json!([20.0, 20.0, 10.0]));
    assert_eq!(preview["at"]["z_axis"], json!([0.0, 0.0, 1.0]));
    assert_eq!(b.doc.assembly().components.len(), components);
    let screw = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("screw"),
               "mount": {"at": at}}),
    );
    // Its head's bearing face on the rim, the shank down into the hole.
    let placement = b.placement(&screw["occurrence"]);
    assert_near(placement.apply_point([2.0, 2.0, 20.0]), [20.0, 20.0, 10.0]);
    assert_near(placement.apply_point([2.0, 2.0, 0.0]), [20.0, 20.0, -10.0]);
    let joint = b.joint(&screw["joint"]);
    assert_eq!(joint["kind"], "cylindrical");
    assert_eq!(joint["solved"], true, "{joint}");
    // From below (or flipped) it goes in from the other side.
    let below = query(
        &b.doc,
        json!({"query": "mount_preview", "library": library_part("screw"),
               "mount": {"at": {"occurrence": "Plate:1",
                                "geometry": {"body": b.plate_body, "edge": rim},
                                "toward": [0, 0, -1]}}}),
    );
    assert_eq!(below["at"]["z_axis"], json!([0.0, 0.0, -1.0]));
    // A screw does not lie on a face.
    let top = b.top();
    let error = refused(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("screw"),
               "mount": {"at": {"occurrence": "Plate:1", "geometry": top}}}),
    );
    assert!(
        error.contains("Screw goes into a hole or onto a shaft"),
        "{error}"
    );
}

#[test]
fn bins_stand_on_the_grid_cells_turned_in_quarter_turns_and_taken_cells_are_refused() {
    let mut b = bench();
    // The baseplate dropped in, as a plain insert: its grid is a joint
    // origin of its component, ready for the bins.
    let plate = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("baseplate")}),
    );
    let grid = plate["frames"][0]["feature"].clone();
    assert_eq!(plate["frames"][0]["kind"], "grid");
    assert_eq!(plate["frames"][0]["cells"], json!([3, 2]));
    let on = |point: Value| json!({"occurrence": "Baseplate:1", "geometry": grid, "point": point});

    // A 1 x 1 bin clicked in cell (1, 0).
    let bin = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("bin1x1"),
               "mount": {"at": on(json!([68, 21, 5]))}}),
    );
    assert_eq!(bin["mount"]["grid"]["cell"], json!([1, 0]));
    assert_eq!(bin["mount"]["grid"]["covers"], json!([[1, 0]]));
    assert_eq!(bin["mount"]["joint"], "rigid");
    let placement = b.placement(&bin["occurrence"]);
    assert_near(placement.translation, [42.0, 0.0, 5.0]);
    assert_eq!(b.joint(&bin["joint"])["solved"], true);

    // A 2 x 1 bin turned a quarter: 1 x 2 cells from (0, 0), from a click
    // anywhere on the baseplate (its body).
    let body = b.doc.component_bodies(
        plate["component"]
            .as_str()
            .unwrap()
            .parse::<ComponentUid>()
            .unwrap(),
    )[0];
    let long = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("bin2x1"),
               "mount": {"at": {"occurrence": "Baseplate:1", "geometry": {"body": body},
                                "point": [20, 40, 5]},
                         "rotate": "90 deg"}}),
    );
    assert_eq!(long["mount"]["grid"]["cell"], json!([0, 0]));
    assert_eq!(long["mount"]["grid"]["footprint"], json!([1, 2]));
    assert_eq!(long["mount"]["grid"]["turns"], 1);
    let placement = b.placement(&long["occurrence"]);
    assert_near(placement.translation, [42.0, 0.0, 5.0]);
    assert_near(placement.apply_vector([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]);
    // Its box covers x 0..42, y 0..84.
    assert_near(placement.apply_point([0.0, 0.0, 0.0]), [42.0, 0.0, 5.0]);
    assert_near(placement.apply_point([84.0, 42.0, 0.0]), [0.0, 84.0, 5.0]);

    // A taken cell is refused with who stands there.
    let error = refused(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("bin1x1"),
               "mount": {"at": on(json!([21, 63, 5]))}}),
    );
    assert!(
        error.contains("the cell (0, 1) of Baseplate:1's grid is taken by Bin 2x1:1"),
        "{error}"
    );
    let error = refused(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("bin2x1"),
               "mount": {"at": on(json!([42, 21, 5]))}}),
    );
    assert!(
        error.contains(
            "the cells (0, 0) and (1, 0) of Baseplate:1's grid are taken by Bin 1x1:1 and Bin \
             2x1:1"
        ),
        "{error}"
    );
    // The preview says so instead of failing.
    let preview = query(
        &b.doc,
        json!({"query": "mount_preview", "library": library_part("bin1x1"),
               "mount": {"at": on(json!([21, 21, 5]))}}),
    );
    assert!(
        preview["problems"][0]
            .as_str()
            .unwrap()
            .contains("is taken by Bin 2x1:1"),
        "{preview}"
    );
    // Only quarter turns; a footprint larger than the grid does not fit.
    let error = refused(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("bin1x1"),
               "mount": {"at": on(json!([105, 63, 5])), "rotate": "45 deg"}}),
    );
    assert!(error.contains("turns in quarter turns"), "{error}");
    // The free cell (2, 1) takes a bin.
    let last = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("bin1x1"),
               "mount": {"at": on(json!([105, 63, 5]))}}),
    );
    assert_eq!(last["mount"]["grid"]["cell"], json!([2, 1]));
    assert_near(
        b.placement(&last["occurrence"]).translation,
        [84.0, 42.0, 5.0],
    );
    // Moving the baseplate carries its bins.
    command(
        &mut b.doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": "Baseplate:1",
               "transform": {"translation": [0, 0, 50]}}),
    );
    assert_near(
        b.placement(&last["occurrence"]).translation,
        [84.0, 42.0, 55.0],
    );
}

#[test]
fn a_part_dropped_in_is_joined_afterwards_by_its_mount_frame() {
    let mut b = bench();
    let lamp = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("lamp"),
               "transform": {"translation": [100, 0, 0]}}),
    );
    // Joint uses the frame as origin A, as any joint origin.
    let top = b.top();
    let joint = command(
        &mut b.doc,
        json!({"cmd": "add_joint", "kind": "rigid", "flip": true,
               "a": {"occurrence": "Lamp:1", "geometry": lamp["frames"][0]["feature"]},
               "b": {"occurrence": "Plate:1", "geometry": top}}),
    );
    assert_eq!(joint["solved"], true, "{joint}");
    // The lamp's bottom middle on the plate's top middle.
    let placement = b.placement(&lamp["occurrence"]);
    assert_near(placement.apply_point([5.0, 5.0, 0.0]), [20.0, 20.0, 10.0]);
}

#[test]
fn a_linked_parts_frames_follow_its_row() {
    // A grid whose cells follow a parameter, in two rows.
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "h", "expression": "5 mm"}),
    );
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(84.0), &num(42.0))
        .unwrap()
        .region;
    doc.add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap();
    let plane = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
            "definition": {"type": "offset", "plane": "xy", "distance": "h"}}}),
    )["uid"]
        .clone();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "joint_origin", "geometry": plane,
            "mount": {"name": "grid", "kind": "grid", "pitch": [42, 42], "cells": [2, 1]}}}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "selectors": ["Height"], "parameters": ["h"], "default": "low",
            "rows": [{"name": "low", "select": {"Height": "5"}, "values": {"h": "5 mm"}},
                     {"name": "high", "select": {"Height": "7"}, "values": {"h": "7 mm"}}]}}),
    );
    let mut designs = BTreeMap::new();
    designs.insert("tray".to_owned(), ("Tray".to_owned(), doc.to_json()));
    let mut b = bench();
    b.doc.set_link_resolver(Some(Arc::new(Parts { designs })));
    let tray = command(
        &mut b.doc,
        json!({"cmd": "insert_component", "library": library_part("tray")}),
    );
    let feature: FeatureUid = tray["frames"][0]["feature"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let origin = |doc: &Document<MockKernel>| match &doc.feature(feature).unwrap().def {
        FeatureDef::JointOrigin(def) => match def.geometry {
            GeomRef::FixedPlane { origin, .. } => origin,
            _ => panic!("a fixed plane"),
        },
        _ => panic!("a joint origin"),
    };
    assert_near(origin(&b.doc), [0.0, 0.0, 5.0]);
    command(
        &mut b.doc,
        json!({"cmd": "update_library_parts", "changes": [
            {"component": tray["component"], "config": "high"}]}),
    );
    // The same joint origin, at the row's height.
    assert_near(origin(&b.doc), [0.0, 0.0, 7.0]);
}
