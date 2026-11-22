// SPDX-License-Identifier: MIT
//! The face's own edges and the outline a sketch got when it was made do
//! not split an area the sketch's own curves close (mitcad#2197), with the
//! mock kernel: a circle across the face's edge is one disc, with and
//! without the automatic outline; an outline the user projected still
//! splits it; the outline's flag is saved, and Convert to Sketch Geometry
//! drops it; a reference to a piece the old rule made keeps its area; a 3D
//! sketch on the face follows the same rule.

use std::f64::consts::PI;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::FeatureUid;
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

/// The top face's edges of the 60 x 40 block, at its top (z = 20), with
/// their lines: the face's boundary (the face-bounded regions), its
/// outline (Project of the face) and each edge's line.
fn set_block(doc: &Document<MockKernel>) {
    set_block_kernel(doc.kernel());
}

/// A mock kernel with the block's top face, to read a saved design with.
fn block_kernel() -> MockKernel {
    let kernel = MockKernel::default();
    set_block_kernel(&kernel);
    kernel
}

fn set_block_kernel(kernel: &MockKernel) {
    let top: FaceName = FACE.parse().unwrap();
    let region: RegionKey = REGION.parse().unwrap();
    let c = [[0.0, 0.0], [60.0, 0.0], [60.0, 40.0], [0.0, 40.0]];
    let edges: Vec<(EdgeName, Curve3)> = region
        .segments()
        .enumerate()
        .map(|(i, segment)| {
            let side = FaceName::side(FeatureUid(2), segment.clone());
            let (a, b) = (c[i], c[(i + 1) % 4]);
            (
                EdgeName::new(top.clone(), side),
                Curve3::Line {
                    start: [a[0], a[1], 20.0],
                    end: [b[0], b[1], 20.0],
                },
            )
        })
        .collect();
    kernel
        .face_boundaries
        .borrow_mut()
        .insert(top.to_string(), edges.clone());
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
    set_block(&doc);
    doc
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-3 * b.abs().max(1.0)
}

/// Sketch F3 on the block's top face (with the face's outline when
/// `outline`) and a circle of diameter 20 about the middle of its right
/// edge, half on the face; the circle's id.
fn circle_across_the_edge(doc: &mut Document<MockKernel>, outline: bool) -> String {
    command(
        doc,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}, "project_face": outline}),
    );
    command(
        doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F3", "center": [60, 20], "diameter": 20}),
    );
    circle_id(&sketch(doc, "F3"))
}

fn circle_id(s: &Value) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "circle")
        .unwrap_or_else(|| panic!("no circle: {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn regions(s: &Value) -> &Vec<Value> {
    s["regions"].as_array().unwrap()
}

/// The region keyed by the circle alone (the whole disc), if any.
fn disc<'a>(s: &'a Value, circle: &str) -> Option<&'a Value> {
    let key = format!("r{{{circle}}}");
    regions(s).iter().find(|r| r["key"] == key.as_str())
}

#[test]
fn a_circle_across_the_face_edge_is_one_disc_with_and_without_the_outline() {
    for outline in [false, true] {
        let mut doc = block();
        let circle = circle_across_the_edge(&mut doc, outline);
        let s = sketch(&doc, "F3");
        assert_eq!(regions(&s).len(), 2, "{outline}: {s}");
        let disc = disc(&s, &circle).unwrap_or_else(|| panic!("{outline}: no whole disc: {s}"));
        assert!(close(disc["area"].as_f64().unwrap(), PI * 100.0), "{s}");
        assert_eq!(disc["loops"].as_array().unwrap().len(), 1, "{s}");
        // The face's leftover: the face less the half disc on it.
        let rest = regions(&s)
            .iter()
            .find(|r| r["key"] != disc["key"])
            .unwrap();
        assert!(
            close(rest["area"].as_f64().unwrap(), 2400.0 - PI * 50.0),
            "{s}"
        );
        // Extrude takes the whole disc by its key.
        command(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "extrude",
                   "profiles": [{"sketch": "F3", "region": disc["key"]}],
                   "extent": {"type": "distance", "distance": 5}, "operation": "join"}}),
        );
        assert!(
            matches!(
                doc.status(FeatureUid(4)),
                Some(mitcad_model::FeatureStatus::Ok)
            ),
            "{}",
            query(&doc, json!({"query": "timeline"}))
        );
    }
}

#[test]
fn an_outline_the_user_projected_still_splits_the_circle() {
    let mut doc = block();
    let circle = circle_across_the_edge(&mut doc, false);
    command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "linked": true}),
    );
    let s = sketch(&doc, "F3");
    let projections = s["projections"].as_array().unwrap();
    assert_eq!(projections.len(), 1, "{s}");
    assert!(projections[0].get("automatic").is_none(), "{s}");
    // The two halves of the disc and the face less its half.
    assert_eq!(regions(&s).len(), 3, "{s}");
    assert!(disc(&s, &circle).is_none(), "{s}");
}

#[test]
fn the_automatic_outline_is_saved_and_read_again() {
    let mut doc = block();
    let circle = circle_across_the_edge(&mut doc, true);
    let s = sketch(&doc, "F3");
    let projections = s["projections"].as_array().unwrap();
    assert_eq!(projections.len(), 1, "{s}");
    assert_eq!(projections[0]["automatic"], true, "{s}");
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert!(file.to_string().contains("\"automatic\":true"), "{saved}");
    let mut loaded = Document::from_json(&saved, block_kernel()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    // A design read is evaluated by its first recompute.
    loaded.recompute();
    let s = sketch(&loaded, "F3");
    assert_eq!(s["projections"][0]["automatic"], true, "{s}");
    assert!(disc(&s, &circle).is_some(), "{s}");
    assert_eq!(regions(&s).len(), 2, "{s}");
}

#[test]
fn convert_to_sketch_geometry_drops_the_flag_with_the_projection() {
    let mut doc = block();
    let circle = circle_across_the_edge(&mut doc, true);
    let s = sketch(&doc, "F3");
    let lines: Vec<Value> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "line")
        .map(|e| e["id"].clone())
        .collect();
    assert_eq!(lines.len(), 4, "{s}");
    command(
        &mut doc,
        json!({"cmd": "sketch.convert_projected", "sketch": "F3", "entities": lines}),
    );
    let s = sketch(&doc, "F3");
    assert!(
        s["projections"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("automatic").is_none()),
        "{s}"
    );
    // The outline's lines are the user's now: they split the circle.
    assert!(disc(&s, &circle).is_none(), "{s}");
    assert_eq!(regions(&s).len(), 3, "{s}");
}

/// A design saved before mitcad#2197 whose Extrude uses the half of a
/// circle that the face's edge cut off: the half keeps its key and its
/// area, so the Extrude makes the same body.
#[test]
fn a_reference_to_a_half_the_face_edge_cut_keeps_its_area() {
    let mut doc = block();
    let circle = circle_across_the_edge(&mut doc, false);
    let output = doc.sketch_output(FeatureUid(3)).unwrap();
    let earlier = output.own.earlier();
    assert_eq!(earlier.len(), 2, "{earlier:?}");
    // The half on the face (its centroid left of the edge at x = 60).
    let half = earlier.iter().find(|r| r.centroid[0] < 60.0).unwrap();
    assert!(close(half.area, PI * 50.0), "{}", half.area);
    let key = half.profile.key.clone();
    assert!(key.to_string().contains(&circle), "{key}");
    assert!(output.region(&key).is_none(), "not a region now: {key}");
    let found = output.found(&key).unwrap();
    assert!(found.exact);
    assert!(close(found.regions[0].area, half.area));
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": key.to_string()}],
               "extent": {"type": "distance", "distance": 5}, "operation": "join"}}),
    );
    assert!(
        matches!(
            doc.status(FeatureUid(4)),
            Some(mitcad_model::FeatureStatus::Ok)
        ),
        "{}",
        query(&doc, json!({"query": "timeline"}))
    );
    let profile = query(
        &doc,
        json!({"query": "profile", "sketch": "F3", "region": key.to_string()}),
    );
    assert_eq!(profile["region"], key.to_string(), "{profile}");
    assert_eq!(profile["exact"], true, "{profile}");
    // No warning about a region found under another key.
    let timeline = query(&doc, json!({"query": "timeline"})).to_string();
    assert!(!timeline.contains("found as"), "{timeline}");
    // Saved and read again, it still takes the half.
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, block_kernel()).unwrap();
    loaded.recompute();
    assert!(
        matches!(
            loaded.status(FeatureUid(4)),
            Some(mitcad_model::FeatureStatus::Ok)
        ),
        "{}",
        query(&loaded, json!({"query": "timeline"}))
    );
    let output = loaded.sketch_output(FeatureUid(3)).unwrap();
    let found = output.found(&key).unwrap();
    assert!(found.exact);
    assert!(close(found.regions[0].area, PI * 50.0));
}

#[test]
fn a_3d_sketch_on_the_face_takes_a_circle_across_its_edge_whole() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "sketch3d.create", "support": {"face": FACE}}),
    );
    let made = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_circle", "sketch": "F3", "center": [60, 20, 0],
               "radius": 10, "normal_axis": "z"}),
    );
    let s = query(&doc, json!({"query": "sketch3d", "uid": "F3"}));
    let circle = s["curves"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "circle")
        .unwrap_or_else(|| panic!("no circle: {made} {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let keys: Vec<String> = s["regions"]
        .as_array()
        .unwrap_or_else(|| panic!("no regions: {s}"))
        .iter()
        .map(|r| r.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(keys.len(), 2, "{s}");
    let whole = format!("r{{{circle}}}");
    assert!(keys.contains(&whole), "{keys:?}");
    let output = doc.sketch_output(FeatureUid(3)).unwrap();
    let disc = output
        .region_info
        .iter()
        .find(|r| r.profile.key.to_string() == whole)
        .unwrap();
    assert!(close(disc.area, PI * 100.0), "{}", disc.area);
    assert_eq!(output.own.earlier().len(), 2);
}
