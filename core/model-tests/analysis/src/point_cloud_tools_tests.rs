// SPDX-License-Identifier: MIT
//! The point cloud tools through the API (mitcad#2516), with the mock
//! kernel: a scan of a block's faces moved and turned, aligned back onto
//! the block by the best fit (its placement kept as an undo step, the query
//! moving nothing) and by picked point pairs; a cloud aligned to another
//! cloud; the refusals in words (no target, too few points, no overlap,
//! itself); outliers filtered out of a sphere's points into a new cloud,
//! the cloud they came from hidden; a sphere's points rebuilt as a closed
//! mesh body.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn cloud(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "point_clouds"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no point cloud {name}"))
        .clone()
}

/// A 60 x 40 x 20 block (F2.b0) whose faces the mock kernel measures as its
/// bounds' (`copies_by_bounds`).
fn block() -> Document<MockKernel> {
    let doc = Document::new(MockKernel::default());
    doc.kernel().copies_by_bounds.set(true);
    let mut doc = doc;
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc
}

fn insert(doc: &mut Document<MockKernel>, name: &str, points: &[[f64; 3]]) {
    let text: String = points
        .iter()
        .map(|p| format!("{} {} {}\n", p[0], p[1], p[2]))
        .collect();
    command(
        doc,
        json!({"cmd": "insert_point_cloud", "name": name, "file_name": "scan.xyz",
               "data": mitcad_model::internal::base64::encode(text.as_bytes())}),
    );
}

/// The block's faces, a point every 5 mm, edges included.
fn block_faces() -> Vec<[f64; 3]> {
    let size = [60.0, 40.0, 20.0];
    let mut points = Vec::new();
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let (nu, nv) = ((size[u] / 5.0) as usize, (size[v] / 5.0) as usize);
        for side in [0.0, size[axis]] {
            for i in 0..=nu {
                for j in 0..=nv {
                    let mut p = [0.0; 3];
                    p[axis] = side;
                    p[u] = 5.0 * i as f64;
                    p[v] = 5.0 * j as f64;
                    points.push(p);
                }
            }
        }
    }
    points
}

/// A turn of `degrees` about z through (30, 20, 10), then a move.
fn moved(points: &[[f64; 3]], degrees: f64, by: [f64; 3]) -> Vec<[f64; 3]> {
    let (s, c) = degrees.to_radians().sin_cos();
    points
        .iter()
        .map(|p| {
            let (x, y) = (p[0] - 30.0, p[1] - 20.0);
            [
                30.0 + c * x - s * y + by[0],
                20.0 + s * x + c * y + by[1],
                p[2] + by[2],
            ]
        })
        .collect()
}

fn near(a: &Value, b: [f64; 3], tolerance: f64) -> bool {
    (0..3).all(|k| (a[k].as_f64().unwrap() - b[k]).abs() <= tolerance)
}

/// The ticket's test through the API: a scan moved and turned aligns back
/// onto the block within a tolerance; the query alone moves nothing.
#[test]
fn a_moved_scan_aligns_back_onto_the_block() {
    let mut doc = block();
    insert(
        &mut doc,
        "Scan",
        &moved(&block_faces(), 2.0, [1.5, -1.0, 0.8]),
    );
    let before = cloud(&doc, "Scan");
    let asked = json!({"query": "point_cloud_alignment", "name": "Scan", "body": "F2.b0",
                       "max_iterations": 200});
    let preview = query(&doc, asked);
    assert!(preview["rms_mm"].as_f64().unwrap() < 1e-3, "{preview}");
    assert_eq!(cloud(&doc, "Scan"), before, "the query moved the cloud");
    let aligned = command(
        &mut doc,
        json!({"cmd": "align_point_cloud", "name": "Scan", "body": "F2.b0", "max_iterations": 200}),
    );
    assert!(aligned["rms_mm"].as_f64().unwrap() < 1e-3, "{aligned}");
    assert!(aligned["max_mm"].as_f64().unwrap() < 1e-2, "{aligned}");
    assert_eq!(aligned["sampled"], json!(block_faces().len()));
    assert_eq!(undo_label(&doc), json!("Align Scan"));
    let placement = &cloud(&doc, "Scan")["placement"];
    let scan = moved(&block_faces(), 2.0, [1.5, -1.0, 0.8]);
    for (s, p) in scan.iter().zip(block_faces()) {
        let at = placed(placement, *s);
        assert!(distance(at, p) < 1e-2, "{s:?} goes to {at:?}, not {p:?}");
    }
}

/// A point moved by a cloud's placement as the `point_clouds` query gives
/// it (`linear` row by row, `translation`).
fn placed(placement: &Value, p: [f64; 3]) -> [f64; 3] {
    let m = |r: usize, c: usize| placement["linear"][r][c].as_f64().unwrap();
    std::array::from_fn(|r| {
        (0..3).map(|c| m(r, c) * p[c]).sum::<f64>() + placement["translation"][r].as_f64().unwrap()
    })
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt()
}

/// Picked point pairs alone (no best fit): three corners of the scan onto
/// the block's.
#[test]
fn point_pairs_place_the_scan() {
    let mut doc = block();
    let corners = [[0.0, 0.0, 0.0], [60.0, 0.0, 0.0], [60.0, 40.0, 20.0]];
    let scan_corners = moved(&corners, 30.0, [10.0, 5.0, -3.0]);
    insert(
        &mut doc,
        "Scan",
        &moved(&block_faces(), 30.0, [10.0, 5.0, -3.0]),
    );
    let pairs: Vec<Value> = scan_corners
        .iter()
        .zip(&corners)
        .map(|(s, t)| json!({"cloud": s, "target": t}))
        .collect();
    let aligned = command(
        &mut doc,
        json!({"cmd": "align_point_cloud", "name": "Scan", "body": "F2.b0", "pairs": pairs,
               "best_fit": false}),
    );
    assert!(
        aligned["pairs_rms_mm"].as_f64().unwrap() < 1e-4,
        "{aligned}"
    );
    assert_eq!(aligned["iterations"], json!(0));
    let placement = &cloud(&doc, "Scan")["placement"];
    let scan = moved(&block_faces(), 30.0, [10.0, 5.0, -3.0]);
    for (s, p) in scan.iter().zip(block_faces()) {
        assert!(distance(placed(placement, *s), p) < 1e-3, "{s:?}");
    }
}

/// Lopsided points: no symmetry, so one motion fits.
fn lopsided() -> Vec<[f64; 3]> {
    (0..400)
        .map(|i| {
            let t = f64::from(i);
            let (u, v, w) = (
                (t * 0.618_034).fract(),
                (t * 0.414_214).fract(),
                (t * 0.732_051).fract(),
            );
            [40.0 * u * u, 20.0 * v + 3.0 * u, 6.0 * w * w + 2.0 * u * v]
        })
        .collect()
}

#[test]
fn a_cloud_aligns_to_another_cloud() {
    let mut doc = Document::new(MockKernel::default());
    let reference = lopsided();
    insert(&mut doc, "Reference", &reference);
    insert(&mut doc, "Scan", &moved(&reference, 12.0, [3.0, -2.0, 1.0]));
    let picks = [5usize, 150, 390];
    let scan = moved(&reference, 12.0, [3.0, -2.0, 1.0]);
    let pairs: Vec<Value> = picks
        .iter()
        .map(|&i| json!({"cloud": scan[i], "target": [reference[i][0] + 0.1, reference[i][1], reference[i][2]]}))
        .collect();
    let aligned = command(
        &mut doc,
        json!({"cmd": "align_point_cloud", "name": "Scan", "target_cloud": "Reference",
               "pairs": pairs}),
    );
    assert!(aligned["rms_mm"].as_f64().unwrap() < 1e-4, "{aligned}");
    let placement = &cloud(&doc, "Scan")["placement"];
    for (s, r) in scan.iter().zip(&reference) {
        assert!(distance(placed(placement, *s), *r) < 1e-3, "{s:?}");
    }
}

#[test]
fn alignments_refuse_in_words() {
    let mut doc = block();
    insert(&mut doc, "Two", &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]);
    insert(
        &mut doc,
        "Far",
        &moved(&block_faces(), 0.0, [5000.0, 0.0, 0.0]),
    );
    let no_target = refused(&mut doc, json!({"cmd": "align_point_cloud", "name": "Far"}));
    assert!(
        no_target.contains("a `body` or a `target_cloud`"),
        "{no_target}"
    );
    let few = refused(
        &mut doc,
        json!({"cmd": "align_point_cloud", "name": "Two", "body": "F2.b0"}),
    );
    assert!(few.contains("at least 3 points"), "{few}");
    let apart = refused(
        &mut doc,
        json!({"cmd": "align_point_cloud", "name": "Far", "body": "F2.b0", "max_distance": 1.0}),
    );
    assert!(apart.contains("do not overlap"), "{apart}");
    let itself = refused(
        &mut doc,
        json!({"cmd": "align_point_cloud", "name": "Far", "target_cloud": "Far"}),
    );
    assert!(itself.contains("to itself"), "{itself}");
    let nothing = refused(
        &mut doc,
        json!({"cmd": "align_point_cloud", "name": "Far", "body": "F2.b0", "best_fit": false}),
    );
    assert!(nothing.contains("nothing to align"), "{nothing}");
}

/// The points of a sphere, evenly (a Fibonacci spiral), about (0, 0, 50).
fn sphere(count: usize, radius: f64) -> Vec<[f64; 3]> {
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    (0..count)
        .map(|i| {
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / count as f64;
            let r = (1.0 - z * z).sqrt();
            let a = golden * i as f64;
            [
                radius * r * a.cos(),
                radius * r * a.sin(),
                50.0 + radius * z,
            ]
        })
        .collect()
}

/// The ticket's test through the API: outliers go into no new cloud, every
/// point of the sphere stays; the cloud they came from is kept, hidden.
#[test]
fn filtering_makes_a_new_cloud_without_the_outliers() {
    let mut doc = Document::new(MockKernel::default());
    let mut points = sphere(1500, 10.0);
    for (i, dir) in sphere(12, 1.0).iter().enumerate() {
        let r = 30.0 + 2.0 * i as f64;
        points.push([dir[0] * r, dir[1] * r, 50.0 + (dir[2] - 50.0) * r]);
    }
    insert(&mut doc, "Scan", &points);
    let filtered = command(
        &mut doc,
        json!({"cmd": "filter_point_cloud", "name": "Scan",
               "statistical": {"neighbors": 8, "ratio": 2.0}}),
    );
    assert_eq!(filtered["name"], json!("Scan Filtered"));
    assert_eq!(filtered["count"], json!(1500));
    assert_eq!(filtered["statistical"], json!(12));
    assert_eq!(undo_label(&doc), json!("Filter Scan"));
    assert_eq!(cloud(&doc, "Scan")["visible"], json!(false));
    assert_eq!(cloud(&doc, "Scan")["count"], json!(1512));
    let kept = cloud(&doc, "Scan Filtered");
    assert_eq!(kept["visible"], json!(true));
    assert!(
        near(&kept["bounds"][0], [-10.0, -10.0, 40.0], 0.05),
        "{kept}"
    );
    assert!(near(&kept["bounds"][1], [10.0, 10.0, 60.0], 0.05), "{kept}");
    let empty = refused(
        &mut doc,
        json!({"cmd": "filter_point_cloud", "name": "Scan"}),
    );
    assert!(empty.contains("choose what the filter does"), "{empty}");
}

/// The ticket's test through the API: a sphere's points rebuilt as a
/// closed mesh body.
#[test]
fn a_sphere_s_points_become_a_closed_mesh_body() {
    let mut doc = Document::new(MockKernel::default());
    insert(&mut doc, "Scan", &sphere(2500, 10.0));
    let made = command(
        &mut doc,
        json!({"cmd": "reconstruct_point_cloud", "name": "Scan", "detail": 32}),
    );
    assert_eq!(made["closed"], json!(true), "{made}");
    assert_eq!(made["shells"], json!(1), "{made}");
    assert_eq!(made["body_name"], json!("Scan Mesh"));
    assert!(made["max_deviation_mm"].as_f64().unwrap() < 0.2, "{made}");
    let volume = 4.0 / 3.0 * std::f64::consts::PI * 1000.0;
    assert!(
        (made["volume_mm3"].as_f64().unwrap() - volume).abs() < 0.03 * volume,
        "{made}"
    );
    assert_eq!(undo_label(&doc), json!("Reconstruct Scan"));
    let bodies = query(&doc, json!({"query": "bodies"}));
    assert!(
        bodies
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["name"] == "Scan Mesh"),
        "{bodies}"
    );
    let few = refused(
        &mut doc,
        json!({"cmd": "reconstruct_point_cloud", "name": "Scan", "detail": 1000}),
    );
    assert!(few.contains("the detail must be"), "{few}");
}
