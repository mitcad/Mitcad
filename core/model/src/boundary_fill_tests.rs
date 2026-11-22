// SPDX-License-Identifier: MIT
//! Boundary Fill (mitcad#457) in documents, with the mock kernel: its
//! cells are worked out from the tools' bounding boxes
//! (`MockKernel::boundary_cells`).

use serde_json::{Value, json};

use crate::features::FeatureDef;
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::BoundingBox;
use crate::notice::NoticePlace;
use crate::test_support::def;
use crate::testing::{MockKernel, MockShape};
use crate::topo::FaceName;
use crate::{Document, FeatureStatus};

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

fn shape(doc: &Document<MockKernel>, body: &str) -> MockShape {
    let uid: BodyUid = body.parse().unwrap();
    doc.body_shape(uid)
        .unwrap_or_else(|| panic!("no body {body}"))
        .clone()
}

fn bounds(doc: &Document<MockKernel>, body: &str) -> BoundingBox {
    shape(doc, body).bounds.expect("bounds")
}

fn uids(doc: &Document<MockKernel>) -> Vec<String> {
    doc.bodies().iter().map(|b| b.uid.to_string()).collect()
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn face(name: &str) -> FaceName {
    name.parse().unwrap()
}

/// A cube of `size` with its lowest corner at `corner`, a new body.
fn cube(doc: &mut Document<MockKernel>, corner: [f64; 3], size: f64) -> FeatureUid {
    add(
        doc,
        json!({"type": "box",
               "plane": {"type": "fixed", "origin": corner, "normal": [0, 0, 1], "x_axis": [1, 0, 0]},
               "length": size, "width": size, "height": size, "operation": "new_body"}),
    )
}

/// Two overlapping 20 mm cubes: A (F1) at the origin and B (F2) at
/// (10, 10, 10); they share the cube from 10 to 20.
fn two_cubes() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    cube(&mut doc, [0.0, 0.0, 0.0], 20.0);
    cube(&mut doc, [10.0, 10.0, 10.0], 20.0);
    doc
}

fn fill(cells: Value, operation: &str, extra: Value) -> Value {
    let mut value = json!({"type": "boundary_fill", "tools": ["F1.b0", "F2.b0"],
        "cells": cells, "operation": operation});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    value
}

#[test]
fn cells_become_new_bodies_with_the_tools_face_names() {
    let mut doc = two_cubes();
    let added = doc
        .add_feature(
            &def(fill(
                json!([[15, 15, 15], [5, 5, 5]]),
                "new_body",
                json!({}),
            )),
            None,
        )
        .unwrap();
    assert_eq!(added.name, "BoundaryFill1");
    assert!(added.parameters.is_empty());
    let uid = added.uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // One body per cell, in the order of the points; the tools stay.
    assert_eq!(uids(&doc), ["F1.b0", "F2.b0", "F3.b0", "F3.b1"]);
    assert_eq!(doc.kernel().count("boundary_cells"), 1);
    // The common cell: inside both tools, bounded by faces of both.
    let common = shape(&doc, "F3.b0");
    assert!(
        common.history.starts_with("cell(box("),
        "{}",
        common.history
    );
    assert!(common.history.ends_with("|)"), "{}", common.history);
    assert_eq!(
        bounds(&doc, "F3.b0"),
        BoundingBox {
            min: [10.0; 3],
            max: [20.0; 3]
        }
    );
    assert!(common.faces.contains(&face("F1:top")));
    assert!(common.faces.contains(&face("F2:bottom")));
    // The cell inside A only: B bounds it too.
    let outer = shape(&doc, "F3.b1");
    assert!(!outer.history.ends_with("|)"), "{}", outer.history);
    assert!(outer.faces.contains(&face("F1:side0")));
    assert!(outer.faces.contains(&face("F2:side0")));
    // The tools are not changed.
    assert!(shape(&doc, "F1.b0").history.starts_with("box("));

    // Removing the tools leaves the cells.
    doc.edit_feature(
        uid,
        &def(fill(
            json!([[15, 15, 15], [5, 5, 5]]),
            "new_body",
            json!({"remove_tools": true}),
        )),
    )
    .unwrap();
    assert_eq!(uids(&doc), ["F3.b0", "F3.b1"]);
}

#[test]
fn two_points_in_one_cell_fill_it_once() {
    let mut doc = two_cubes();
    let uid = add(
        &mut doc,
        fill(json!([[15, 15, 15], [12, 18, 11]]), "new_body", json!({})),
    );
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(uids(&doc), ["F1.b0", "F2.b0", "F3.b0"]);
    assert!(
        doc.warnings(uid)
            .iter()
            .any(|w| w.contains("points 1 and 2 pick the same cell")),
        "{:?}",
        doc.warnings(uid)
    );
}

#[test]
fn cells_join_cut_or_intersect_the_participants() {
    let mut doc = two_cubes();
    // C (F3), a plate the cells reach into.
    cube(&mut doc, [12.0, 12.0, -5.0], 6.0);
    let uid = add(
        &mut doc,
        fill(
            json!([[15, 15, 15]]),
            "cut",
            json!({"participants": ["F3.b0"], "remove_tools": true}),
        ),
    );
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // The tools are gone before the cut; C is cut by the cell.
    assert_eq!(uids(&doc), ["F3.b0"]);
    let cut = shape(&doc, "F3.b0").history;
    assert!(cut.starts_with("cut(box("), "{cut}");
    assert!(cut.contains(",cell(box("), "{cut}");

    // Several cells join as one tool.
    let unions = doc.kernel().count("unite");
    doc.edit_feature(
        uid,
        &def(fill(
            json!([[15, 15, 15], [5, 5, 5]]),
            "join",
            json!({"participants": ["F3.b0"]}),
        )),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(uids(&doc), ["F1.b0", "F2.b0", "F3.b0"]);
    assert!(doc.kernel().count("unite") > unions);
    let joined = shape(&doc, "F3.b0").history;
    assert!(joined.starts_with("join(box("), "{joined}");
    assert!(joined.contains("unite(cell("), "{joined}");

    // An intersection that meets no participant fails.
    *doc.kernel().touch.borrow_mut() = Some(vec![false]);
    doc.edit_feature(
        uid,
        &def(fill(
            json!([[15, 15, 15]]),
            "intersect",
            json!({"participants": ["F3.b0"]}),
        )),
    )
    .unwrap();
    assert!(matches!(status(&doc, uid), FeatureStatus::Failed(m)
            if m.contains("the filled cells do not intersect any participant body")));
    *doc.kernel().touch.borrow_mut() = None;

    // Without participants a cut works on all bodies, the kept tools too.
    doc.edit_feature(uid, &def(fill(json!([[15, 15, 15]]), "cut", json!({}))))
        .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(uids(&doc), ["F1.b0", "F2.b0", "F3.b0"]);
    for body in ["F1.b0", "F2.b0", "F3.b0"] {
        let history = shape(&doc, body).history;
        assert!(history.starts_with("cut(box("), "{body}: {history}");
    }
}

#[test]
fn points_outside_the_tools_or_on_their_faces_fail() {
    let mut doc = two_cubes();
    let uid = add(&mut doc, fill(json!([[50, 50, 50]]), "new_body", json!({})));
    assert!(matches!(status(&doc, uid), FeatureStatus::Failed(m)
            if m.contains("is inside no tool body")));
    assert_eq!(uids(&doc), ["F1.b0", "F2.b0"]);
    doc.edit_feature(uid, &def(fill(json!([[20, 5, 5]]), "new_body", json!({}))))
        .unwrap();
    assert!(matches!(status(&doc, uid), FeatureStatus::Failed(m)
            if m.contains("lies on the boundary of tool 1")));
}

#[test]
fn definitions_are_checked() {
    let mut doc = two_cubes();
    for (value, message) in [
        (
            json!({"type": "boundary_fill", "tools": [], "cells": [[5, 5, 5]],
                   "operation": "new_body"}),
            "no tool bodies",
        ),
        (
            json!({"type": "boundary_fill", "tools": ["F1.b0", "F1.b0"],
                   "cells": [[5, 5, 5]], "operation": "new_body"}),
            "listed more than once",
        ),
        (
            json!({"type": "boundary_fill", "tools": ["F1.b0"], "cells": [],
                   "operation": "new_body"}),
            "no cells picked",
        ),
        (
            fill(
                json!([[5, 5, 5]]),
                "new_body",
                json!({"participants": ["F1.b0"]}),
            ),
            "a new body has no participant bodies",
        ),
        (
            fill(
                json!([[5, 5, 5]]),
                "cut",
                json!({"participants": ["F2.b0"], "remove_tools": true}),
            ),
            "is a tool the fill removes",
        ),
        (
            json!({"type": "boundary_fill", "tools": ["F9.b0"], "cells": [[5, 5, 5]],
                   "operation": "new_body"}),
            "does not exist",
        ),
    ] {
        let error = doc.add_feature(&def(value), None).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn the_definition_keeps_its_fields() {
    let value = fill(
        json!([[15.0, 15.0, 15.0], [5.0, 5.0, 5.0]]),
        "cut",
        json!({"participants": ["F3.b0"], "remove_tools": true}),
    );
    let parsed: FeatureDef<String> = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(parsed.type_name(), "boundary_fill");
    assert_eq!(parsed.base_name(), "BoundaryFill");
    assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
    // Defaults are left out.
    let plain = fill(json!([[15.0, 15.0, 15.0]]), "new_body", json!({}));
    let parsed: FeatureDef<String> = serde_json::from_value(plain.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), plain);
    let unknown = fill(json!([[1, 2, 3]]), "new_body", json!({"keep_tools": true}));
    let error = serde_json::from_value::<FeatureDef<String>>(unknown).unwrap_err();
    assert!(error.to_string().contains("unknown field"), "{error}");
}

// --- Surface bodies and planes as tools (mitcad#2149). The mock kernel
// works the regions out on the axes (`MockKernel::boundary_regions`): a
// flat surface body is a wall across its axis, a plane square to an axis
// one at its origin.

/// A 40 x 40 surface body, untrimmed, centred on the origin in an origin
/// plane: a sketch with the square and its patch (the patch's body).
fn sheet(doc: &mut Document<MockKernel>, plane: crate::features::SketchPlane) -> String {
    let sketch = doc.add_sketch(plane).unwrap().uid;
    let region = doc
        .add_rectangle(
            sketch,
            [-20.0, -20.0],
            &crate::test_support::num(40.0),
            &crate::test_support::num(40.0),
        )
        .unwrap()
        .region;
    let patch = add(
        doc,
        json!({"type": "patch", "profiles": [{"sketch": sketch, "region": region}]}),
    );
    format!("{patch}.b0")
}

/// Three surface bodies in the origin planes (F2.b0 in XY, F4.b0 in XZ,
/// F6.b0 in YZ), overlapping each other; with the planes x = 10, y = 10
/// and z = 10 they close off the cube from 0 to 10.
fn three_sheets() -> (Document<MockKernel>, Vec<String>) {
    use crate::features::SketchPlane;
    let mut doc = Document::new(MockKernel::default());
    let sheets = vec![
        sheet(&mut doc, SketchPlane::Xy),
        sheet(&mut doc, SketchPlane::Xz),
        sheet(&mut doc, SketchPlane::Yz),
    ];
    (doc, sheets)
}

fn plane_at(axis: usize, at: f64) -> Value {
    let mut origin = [0.0; 3];
    let mut normal = [0.0; 3];
    origin[axis] = at;
    normal[axis] = 1.0;
    json!({"origin": origin, "normal": normal})
}

fn surface_fill(tools: &[String], planes: Value, cells: Value, operation: &str) -> Value {
    json!({"type": "boundary_fill", "tools": tools, "planes": planes, "cells": cells,
           "operation": operation})
}

#[test]
fn surfaces_and_planes_close_off_a_cell_filled_as_a_new_body() {
    let (mut doc, sheets) = three_sheets();
    let planes = json!([plane_at(0, 10.0), plane_at(1, 10.0), plane_at(2, 10.0)]);
    let uid = add(
        &mut doc,
        surface_fill(&sheets, planes, json!([[5, 5, 5]]), "new_body"),
    );
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(uid.to_string(), "F7");
    // The cells are the closed regions, not cells of solids.
    assert_eq!(doc.kernel().count("boundary_regions"), 1);
    assert_eq!(doc.kernel().count("boundary_cells"), 0);
    // The one region, a new body; the surfaces stay.
    assert_eq!(uids(&doc), ["F2.b0", "F4.b0", "F6.b0", "F7.b0"]);
    assert_eq!(
        bounds(&doc, "F7.b0"),
        BoundingBox {
            min: [0.0; 3],
            max: [10.0; 3]
        }
    );
    let cell = shape(&doc, "F7.b0");
    assert!(
        cell.history.starts_with("region(patch("),
        "{}",
        cell.history
    );
    // The planes' pieces are named after the fill, the surfaces' keep
    // theirs.
    for plane in ["F7:plane1", "F7:plane2", "F7:plane3"] {
        assert!(cell.faces.contains(&face(plane)), "{:?}", cell.faces);
    }
    assert!(
        cell.faces.iter().any(|f| f.feature.to_string() == "F2"),
        "{:?}",
        cell.faces
    );

    // Origin planes are plane references too; a second point in the same
    // region fills it once.
    doc.edit_feature(
        uid,
        &def(surface_fill(
            &sheets,
            json!([
                "xy",
                plane_at(0, 10.0),
                plane_at(1, 10.0),
                plane_at(2, 10.0)
            ]),
            json!([[5, 5, 5], [2, 3, 4]]),
            "new_body",
        )),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(uids(&doc), ["F2.b0", "F4.b0", "F6.b0", "F7.b0"]);
    assert!(
        doc.warnings(uid)
            .iter()
            .any(|w| w.contains("points 1 and 2 pick the same cell")),
        "{:?}",
        doc.warnings(uid)
    );

    // Removing the tools leaves the cell.
    let mut removed = surface_fill(
        &sheets,
        json!([plane_at(0, 10.0), plane_at(1, 10.0), plane_at(2, 10.0)]),
        json!([[5, 5, 5]]),
        "new_body",
    );
    removed["remove_tools"] = json!(true);
    doc.edit_feature(uid, &def(removed)).unwrap();
    assert_eq!(uids(&doc), ["F7.b0"]);
}

#[test]
fn a_region_left_open_fails_in_words_with_its_gap_painted() {
    let (mut doc, sheets) = three_sheets();
    // No plane at z = 10: the region above the XY surface is open upwards.
    let uid = add(
        &mut doc,
        surface_fill(
            &sheets,
            json!([plane_at(0, 10.0), plane_at(1, 10.0)]),
            json!([[5, 5, 5]]),
            "new_body",
        ),
    );
    let FeatureStatus::Failed(error) = status(&doc, uid) else {
        panic!("the open region was filled");
    };
    assert!(
        error.message.contains("do not close off the region"),
        "{}",
        error.message
    );
    // In words: no coordinates in the message, the point and the gap in
    // the detail and the places.
    assert!(!error.message.contains('('), "{}", error.message);
    assert!(
        error.detail.contains("point 1 (5, 5, 5)"),
        "{}",
        error.detail
    );
    assert!(error.places.contains(&NoticePlace::point([5.0, 5.0, 5.0])));
    assert!(
        error.places.contains(&NoticePlace::point([5.0, 5.0, 20.0])),
        "{:?}",
        error.places
    );
    assert_eq!(uids(&doc), ["F2.b0", "F4.b0", "F6.b0"]);

    // A point on a face fails too.
    doc.edit_feature(
        uid,
        &def(surface_fill(
            &sheets,
            json!([plane_at(0, 10.0), plane_at(1, 10.0), plane_at(2, 10.0)]),
            json!([[5, 5, 10]]),
            "new_body",
        )),
    )
    .unwrap();
    let FeatureStatus::Failed(error) = status(&doc, uid) else {
        panic!("a point on a face was filled");
    };
    assert!(
        error.message.contains("lies on a face of the tools"),
        "{}",
        error.message
    );
}

#[test]
fn a_plane_across_a_solid_cuts_the_chosen_cell_out_of_it() {
    let mut doc = Document::new(MockKernel::default());
    cube(&mut doc, [0.0, 0.0, 0.0], 20.0);
    let uid = add(
        &mut doc,
        json!({"type": "boundary_fill", "tools": ["F1.b0"], "planes": [plane_at(2, 10.0)],
               "cells": [[5, 5, 15]], "operation": "cut", "participants": ["F1.b0"]}),
    );
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(doc.kernel().count("boundary_regions"), 1);
    assert_eq!(uids(&doc), ["F1.b0"]);
    let cut = shape(&doc, "F1.b0").history;
    assert!(cut.starts_with("cut(box("), "{cut}");
    assert!(cut.contains(",region(box("), "{cut}");
}

#[test]
fn solid_tools_alone_keep_the_cells_of_solids() {
    let mut doc = two_cubes();
    let uid = add(&mut doc, fill(json!([[15, 15, 15]]), "new_body", json!({})));
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(doc.kernel().count("boundary_cells"), 1);
    assert_eq!(doc.kernel().count("boundary_regions"), 0);
    // The definition is written as before: no planes.
    let FeatureDef::BoundaryFill(made) = &doc.feature(uid).unwrap().def else {
        panic!("not a boundary fill");
    };
    assert!(made.planes.is_empty());
    let written = serde_json::to_value(made).unwrap();
    assert!(written.get("planes").is_none(), "{written}");
}

#[test]
fn surface_and_plane_tools_are_checked() {
    let (mut doc, sheets) = three_sheets();
    for (value, message) in [
        (
            surface_fill(&sheets, json!(["xy", "xy"]), json!([[5, 5, 5]]), "new_body"),
            "plane 2 is listed more than once",
        ),
        (
            surface_fill(&sheets, json!(["x"]), json!([[5, 5, 5]]), "new_body"),
            "plane 1:",
        ),
        (
            surface_fill(
                &[],
                json!(["xy", "yz", "xz"]),
                json!([[5, 5, 5]]),
                "new_body",
            ),
            "four or more",
        ),
        (
            surface_fill(&[], json!([]), json!([[5, 5, 5]]), "new_body"),
            "no tool bodies or planes",
        ),
    ] {
        let error = doc.add_feature(&def(value), None).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn the_definition_keeps_its_planes() {
    let value = json!({"type": "boundary_fill", "tools": ["F2.b0"],
        "planes": ["xy", "F5", {"origin": [0.0, 0.0, 10.0], "normal": [0.0, 0.0, 1.0]}],
        "cells": [[5.0, 5.0, 5.0]], "operation": "new_body"});
    let parsed: FeatureDef<String> = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
    // Planes alone need no tools.
    let planes_only = json!({"type": "boundary_fill", "planes": ["xy", "yz", "xz",
        {"origin": [10.0, 10.0, 10.0], "normal": [1.0, 1.0, 1.0]}],
        "cells": [[1.0, 1.0, 1.0]], "operation": "new_body"});
    let parsed: FeatureDef<String> = serde_json::from_value(planes_only).unwrap();
    let written = serde_json::to_value(&parsed).unwrap();
    assert_eq!(written["tools"], json!([]));
}
