// SPDX-License-Identifier: MIT
//! A sketch on a planar face starts with the face's outline
//! (`sketch.create` with `project_face`), and Convert to Sketch Geometry
//! takes a profile (`sketch.convert_projected` with `profiles`;
//! mitcad#2051), with the mock kernel.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName, RegionKey, TopoName};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "sketch", "uid": uid}))
}

/// The block's region in Sketch1 and its top face.
const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const FACE: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";

fn line(a: [f64; 2], b: [f64; 2]) -> Curve3 {
    Curve3::Line {
        start: [a[0], a[1], 20.0],
        end: [b[0], b[1], 20.0],
    }
}

/// The top face's edges of a block `width` wide and 40 deep: its four
/// edges, named as the block's top and side faces meet, with their lines
/// at the top (z = 20).
fn top_edges(width: f64) -> Vec<(EdgeName, Curve3)> {
    let top: FaceName = FACE.parse().unwrap();
    let region: RegionKey = REGION.parse().unwrap();
    let c = [[0.0, 0.0], [width, 0.0], [width, 40.0], [0.0, 40.0]];
    region
        .segments()
        .enumerate()
        .map(|(i, segment)| {
            let side = FaceName::side(mitcad_model::ids::FeatureUid(2), segment.clone());
            (EdgeName::new(top.clone(), side), line(c[i], c[(i + 1) % 4]))
        })
        .collect()
}

/// The mock's geometry of the block `width` wide: the top face's boundary
/// (the face-bounded regions), its outline (Project of the face) and each
/// edge's line (Project of an edge).
fn set_block(doc: &Document<MockKernel>, width: f64) {
    let edges = top_edges(width);
    let kernel = doc.kernel();
    kernel
        .face_boundaries
        .borrow_mut()
        .insert(FACE.parse::<FaceName>().unwrap().to_string(), edges.clone());
    *kernel.curves.borrow_mut() = edges.iter().map(|(_, c)| c.clone()).collect();
    let mut named = kernel.named_curves.borrow_mut();
    for (edge, curve) in edges {
        named.insert(TopoName::Edge(edge).to_string(), curve);
    }
}

/// A 60 x 40 block 20 high: Sketch1 (F1) and Extrude1 (F2).
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": REGION}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    set_block(&doc, 60.0);
    doc
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

/// The sketch's lines, each as its two ends (sorted), and whether it is
/// projected (fixed reference geometry).
fn lines(s: &Value) -> Vec<([[f64; 2]; 2], bool)> {
    let at = |p: &Value| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()];
    let mut out: Vec<([[f64; 2]; 2], bool)> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "line")
        .map(|e| {
            let mut ends = [at(&e["geometry"]["start"]), at(&e["geometry"]["end"])];
            ends.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
            (ends, e["reference"] == true && e["fixed"] == true)
        })
        .collect();
    out.sort_by(|a, b| {
        let key = |l: &([[f64; 2]; 2], bool)| [l.0[0][0], l.0[0][1], l.0[1][0], l.0[1][1]];
        key(a).partial_cmp(&key(b)).unwrap()
    });
    out
}

/// Whether the lines are the outline of the block `width` wide.
fn outline_of(lines: &[([[f64; 2]; 2], bool)], width: f64) -> bool {
    let wanted = [
        [[0.0, 0.0], [0.0, 40.0]],
        [[0.0, 0.0], [width, 0.0]],
        [[0.0, 40.0], [width, 40.0]],
        [[width, 0.0], [width, 40.0]],
    ];
    lines.len() == 4
        && lines.iter().zip(wanted).all(|((ends, _), w)| {
            (0..2).all(|k| close(ends[k][0], w[k][0]) && close(ends[k][1], w[k][1]))
        })
}

fn linked(s: &Value) -> usize {
    s["projections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["linked"] != false)
        .map(|p| p["entities"].as_array().unwrap().len())
        .sum()
}

/// The regions' areas, sorted.
fn areas(s: &Value) -> Vec<f64> {
    let mut list: Vec<f64> = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["area"].as_f64().unwrap())
        .collect();
    list.sort_by(f64::total_cmp);
    list
}

#[test]
fn a_sketch_on_a_face_starts_with_its_linked_outline_in_one_undo_step() {
    let mut doc = block();
    let depth = doc.undo_depth();
    let made = command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}, "upright": true,
               "project_face": true}),
    );
    assert_eq!(made["uid"], "F3", "{made}");
    assert!(
        made["projected"].as_array().is_some_and(|p| p.len() >= 4),
        "{made}"
    );
    assert_eq!(doc.undo_depth(), depth + 1);
    let s = sketch(&doc, "F3");
    let outline = lines(&s);
    assert!(outline_of(&outline, 60.0), "{s}");
    assert!(outline.iter().all(|(_, projected)| *projected), "{s}");
    assert!(linked(&s) >= 4, "the outline is a linked projection: {s}");
    // One undo takes the sketch and its outline back together.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(doc.undo_depth(), depth);
    assert!(doc.feature(mitcad_model::ids::FeatureUid(3)).is_none());
}

#[test]
fn the_outline_follows_the_face() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}, "project_face": true}),
    );
    assert!(outline_of(&lines(&sketch(&doc, "F3")), 60.0));
    set_block(&doc, 80.0);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d1", "value": 80}),
    );
    let s = sketch(&doc, "F3");
    assert!(outline_of(&lines(&s), 80.0), "{s}");
}

#[test]
fn planes_and_calls_without_the_field_get_no_outline() {
    let mut doc = block();
    let origin = command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": "xy", "project_face": true}),
    );
    assert!(origin.get("projected").is_none(), "{origin}");
    assert!(
        sketch(&doc, "F3")["entities"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let plain = command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}}),
    );
    assert!(plain.get("projected").is_none(), "{plain}");
    assert!(
        sketch(&doc, "F4")["entities"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn the_outline_leaves_the_regions_of_a_circle_inside_as_they_were() {
    let mut doc = block();
    for project_face in [false, true] {
        command(
            &mut doc,
            json!({"cmd": "sketch.create", "plane": {"face": FACE},
                   "project_face": project_face}),
        );
    }
    for uid in ["F3", "F4"] {
        command(
            &mut doc,
            json!({"cmd": "sketch.add_circle", "sketch": uid, "center": [30, 20], "diameter": 10}),
        );
    }
    let (bare, outlined) = (sketch(&doc, "F3"), sketch(&doc, "F4"));
    assert_eq!(bare["regions"].as_array().unwrap().len(), 2, "{bare}");
    assert_eq!(
        outlined["regions"].as_array().unwrap().len(),
        2,
        "{outlined}"
    );
    for (a, b) in areas(&bare).into_iter().zip(areas(&outlined)) {
        assert!(close(a, b), "{a} against {b}: {outlined}");
        assert!(b > 1.0, "no region of no area: {outlined}");
    }
    // The ring with the outline on its boundary is one Convert takes.
    let ring = outlined["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["loops"].as_array().unwrap().len() == 2)
        .unwrap();
    assert_eq!(ring["projected"], true, "{outlined}");
    let circle = outlined["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["loops"].as_array().unwrap().len() == 1)
        .unwrap();
    assert!(circle.get("projected").is_none(), "{outlined}");
}

#[test]
fn the_face_bounded_region_of_a_sketch_without_projections_converts_to_its_outline() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}}),
    );
    let s = sketch(&doc, "F3");
    let region = s["regions"][0].clone();
    assert_eq!(region["face_bounded"], true, "{s}");
    let key = region["key"].as_str().unwrap().to_owned();
    assert!(key.contains("b{E{"), "{key}");
    let depth = doc.undo_depth();
    let result = command(
        &mut doc,
        json!({"cmd": "sketch.convert_projected", "sketch": "F3", "profiles": [key]}),
    );
    assert_eq!(doc.undo_depth(), depth + 1, "one undo step");
    let converted: Vec<&str> = result["converted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        converted.iter().filter(|c| c.starts_with('c')).count(),
        4,
        "{result}"
    );
    let s = sketch(&doc, "F3");
    let outline = lines(&s);
    assert!(outline_of(&outline, 60.0), "{s}");
    assert!(
        outline.iter().all(|(_, projected)| !*projected),
        "free curves: {s}"
    );
    assert_eq!(linked(&s), 0, "{s}");
    // The same one region, which the key from before still finds.
    assert_eq!(s["regions"].as_array().unwrap().len(), 1, "{s}");
    assert!(close(areas(&s)[0], 2400.0), "{s}");
    let found = query(
        &doc,
        json!({"query": "profile", "sketch": "F3", "region": key}),
    );
    assert!(
        !found.is_null(),
        "the key that named the face's edges resolves"
    );
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(
        sketch(&doc, "F3")["entities"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_profile_of_ordinary_curves_has_nothing_to_convert() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 20], "diameter": 10}),
    );
    let s = sketch(&doc, "F3");
    let circle = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["face_bounded"] != true)
        .unwrap_or_else(|| panic!("no region of the circle alone: {s}"));
    assert!(circle.get("projected").is_none(), "{s}");
    let error = doc
        .command(
            &json!({"cmd": "sketch.convert_projected", "sketch": "F3",
                    "profiles": [circle["key"]]})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("nothing to convert"), "{error}");
}
