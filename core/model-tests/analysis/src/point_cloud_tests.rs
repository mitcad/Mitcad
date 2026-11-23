// SPDX-License-Identifier: MIT
//! Point clouds and Point Cloud Deviation (mitcad#2240), with the mock
//! kernel: a cloud inserted from generated XYZ and PLY data, listed, moved,
//! hidden, renamed and deleted as undo steps, saved and read back; a
//! deviation of points on a block's faces moved out and in, its signs and
//! means, its sampling, an open surface refusing a sign, and the analysis
//! kept in the document following its cloud's rename. mitcad#2265: a
//! deviation from some of the body's faces only, asked and kept; a cloud
//! following its component to another parent, its deviation unchanged.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
/// The points are kept single precision from the cloud's origin: a point
/// 10 mm from it is within half a micrometre of where it was given.
const TOL: f64 = 1e-5;

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

fn query_error(doc: &Document<MockKernel>, query: Value) -> String {
    match doc.query(&query.to_string()) {
        Ok(answer) => panic!("{query} was answered: {answer}"),
        Err(e) => e.to_string(),
    }
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn clouds(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "point_clouds"}))
        .as_array()
        .unwrap()
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

fn base64(bytes: &[u8]) -> String {
    mitcad_model::internal::base64::encode(bytes)
}

fn xyz(points: &[[f64; 3]]) -> String {
    points
        .iter()
        .map(|p| format!("{} {} {}\n", p[0], p[1], p[2]))
        .collect()
}

/// Inserts the points as an XYZ file's data; returns the command's result.
fn insert(doc: &mut Document<MockKernel>, points: &[[f64; 3]], extra: Value) -> Value {
    let mut cmd = json!({
        "cmd": "insert_point_cloud",
        "data": base64(xyz(points).as_bytes()),
        "file_name": "scan.xyz",
    });
    for (k, v) in extra.as_object().unwrap() {
        cmd[k] = v.clone();
    }
    command(doc, cmd)
}

/// Points on the block's faces away from its edges, moved out of the
/// material by `out` on the top, front and right faces and into it by
/// `inward` on the bottom, back and left ones.
fn on_faces(out: f64, inward: f64) -> (Vec<[f64; 3]>, usize, usize) {
    let mut points = Vec::new();
    let (mut outside, mut inside) = (0, 0);
    for i in 1..5 {
        for j in 1..4 {
            let (u, v) = (f64::from(i), f64::from(j));
            // Top (z 20) out, bottom (z 0) in.
            points.push([12.0 * u, 10.0 * v, 20.0 + out]);
            points.push([12.0 * u, 10.0 * v, inward]);
            // Front (y 0) out, back (y 40) in.
            points.push([12.0 * u, -out, 5.0 * v]);
            points.push([12.0 * u, 40.0 - inward, 5.0 * v]);
            outside += 2;
            inside += 2;
        }
    }
    for j in 1..4 {
        for k in 1..4 {
            let (v, w) = (f64::from(j), f64::from(k));
            // Right (x 60) out, left (x 0) in.
            points.push([60.0 + out, 10.0 * v, 5.0 * w]);
            points.push([inward, 10.0 * v, 5.0 * w]);
            outside += 1;
            inside += 1;
        }
    }
    (points, outside, inside)
}

#[test]
fn a_cloud_is_inserted_listed_and_kept_in_the_file() {
    let mut doc = block();
    let points = [[0.0, 0.0, 0.0], [10.0, 20.0, 30.0], [-4.0, 2.0, 6.0]];
    let result = insert(&mut doc, &points, json!({}));
    assert_eq!(result["name"], "PointCloud1");
    assert_eq!(result["count"], 3);
    assert_eq!(result["warnings"], json!([]));
    assert_eq!(undo_label(&doc), "Insert PointCloud1");
    let listed = clouds(&doc);
    assert_eq!(listed.len(), 1);
    let cloud = &listed[0];
    assert_eq!(cloud["file_name"], "scan.xyz");
    assert_eq!(cloud["visible"], true);
    assert_eq!(cloud["colored"], false);
    assert_eq!(
        cloud["bounds"],
        json!([[-4.0, 0.0, 0.0], [10.0, 20.0, 30.0]])
    );
    // The points for drawing: from the origin, the origin in the translation.
    let drawn = query(
        &doc,
        json!({"query": "point_cloud_points", "name": "PointCloud1"}),
    );
    assert_eq!(drawn["count"], 3);
    assert_eq!(drawn["translation"], json!([3.0, 10.0, 15.0]));
    assert!(drawn["colors"].is_null());
    // Saved and read back.
    let saved = doc.to_json();
    assert!(saved.contains("\"point_clouds\""));
    let mut back = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(back.to_json(), saved);
    command(&mut back, json!({"cmd": "recompute"}));
    assert_eq!(clouds(&back), listed);
    // Another file's name, and a name taken refused.
    let second = insert(&mut doc, &points, json!({"name": "Scan"}));
    assert_eq!(second["name"], "Scan");
    let error = refused(
        &mut doc,
        json!({"cmd": "insert_point_cloud", "data": base64(b"1 2 3\n"), "file_name": "a.xyz", "name": "Scan"}),
    );
    assert!(error.contains("a point cloud Scan exists"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "insert_point_cloud", "file_name": "a.xyz"}),
    );
    assert!(error.contains("`path` or its `data`"), "{error}");
}

#[test]
fn a_binary_ply_in_centimetres_is_thinned_with_a_warning() {
    let mut doc = block();
    let count = 10u32;
    let mut bytes = format!(
        "ply\nformat binary_little_endian 1.0\nelement vertex {count}\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nend_header\n"
    )
    .into_bytes();
    for i in 0..count {
        for c in [i as f32, 0.0, 1.0] {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
        bytes.extend_from_slice(&[200, 100, 0]);
    }
    let result = command(
        &mut doc,
        json!({"cmd": "insert_point_cloud", "data": base64(&bytes), "file_name": "scan.ply",
               "unit": "cm", "max_points": 4}),
    );
    assert_eq!(result["count"], 4);
    assert_eq!(result["file_points"], 10);
    let warning = result["warnings"][0].as_str().unwrap();
    assert!(warning.contains("every 3rd kept"), "{warning}");
    let cloud = &clouds(&doc)[0];
    assert_eq!(cloud["colored"], true);
    // 0, 3, 6 and 9 cm along x, 1 cm up.
    assert_eq!(
        cloud["bounds"],
        json!([[0.0, 0.0, 10.0], [90.0, 0.0, 10.0]])
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "insert_point_cloud", "data": base64(&bytes), "file_name": "s.ply", "unit": "yd"}),
    );
    assert!(error.contains("mm, cm, m, in or ft"), "{error}");
}

#[test]
fn a_cloud_moves_hides_renames_and_goes_as_undo_steps() {
    let mut doc = block();
    insert(&mut doc, &[[1.0, 0.0, 0.0], [3.0, 0.0, 0.0]], json!({}));
    command(
        &mut doc,
        json!({"cmd": "move_point_cloud", "name": "PointCloud1", "translate": [0, 0, 5],
               "rotate": {"axis": [0, 0, 1], "angle": 90}}),
    );
    assert_eq!(undo_label(&doc), "Move PointCloud1");
    // Turned a quarter about z, then 5 up: x 1..3 becomes y 1..3.
    let bounds = clouds(&doc)[0]["bounds"].clone();
    let near = |v: &Value, e: f64| (v.as_f64().unwrap() - e).abs() < 1e-9;
    assert!(
        near(&bounds[0][1], 1.0) && near(&bounds[1][1], 3.0),
        "{bounds}"
    );
    assert!(
        near(&bounds[0][2], 5.0) && near(&bounds[1][0], 0.0),
        "{bounds}"
    );
    command(
        &mut doc,
        json!({"cmd": "place_point_cloud", "name": "PointCloud1", "reset": true}),
    );
    assert_eq!(
        clouds(&doc)[0]["bounds"],
        json!([[1.0, 0.0, 0.0], [3.0, 0.0, 0.0]])
    );
    command(
        &mut doc,
        json!({"cmd": "set_point_cloud_visible", "name": "PointCloud1", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Hide PointCloud1");
    assert_eq!(clouds(&doc)[0]["visible"], false);
    command(
        &mut doc,
        json!({"cmd": "rename_point_cloud", "name": "PointCloud1", "new_name": "Scan"}),
    );
    assert_eq!(clouds(&doc)[0]["name"], "Scan");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(clouds(&doc)[0]["name"], "PointCloud1");
    command(
        &mut doc,
        json!({"cmd": "delete_point_cloud", "name": "PointCloud1"}),
    );
    assert!(clouds(&doc).is_empty());
    let error = refused(
        &mut doc,
        json!({"cmd": "delete_point_cloud", "name": "PointCloud1"}),
    );
    assert!(
        error.contains("there is no point cloud PointCloud1"),
        "{error}"
    );
}

#[test]
fn a_deviation_signs_points_out_of_and_into_the_material() {
    let mut doc = block();
    let (points, outside, inside) = on_faces(0.5, 0.3);
    insert(&mut doc, &points, json!({}));
    let answer = query(
        &doc,
        json!({"query": "point_cloud_deviation", "cloud": "PointCloud1", "body": "F2.b0",
               "tolerance": 0.4, "distances": true}),
    );
    let n = (outside + inside) as f64;
    assert_eq!(answer["signed"], true);
    assert_eq!(answer["sampled"], outside + inside);
    assert_eq!(answer["outside"], outside);
    assert_eq!(answer["inside"], inside);
    assert_eq!(answer["on_boundary"], 0);
    assert_eq!(answer["failures"], 0);
    let close = |key: &str, expected: f64| {
        let actual = answer[key].as_f64().unwrap();
        assert!(
            (actual - expected).abs() < TOL,
            "{key}: {actual} != {expected}"
        );
    };
    close(
        "mean_signed_mm",
        (0.5 * outside as f64 - 0.3 * inside as f64) / n,
    );
    close(
        "mean_abs_mm",
        (0.5 * outside as f64 + 0.3 * inside as f64) / n,
    );
    close("max_abs_mm", 0.5);
    close("min_mm", -0.3);
    close("max_mm", 0.5);
    // The points moved in by 0.3 are within the tolerance of 0.4.
    assert_eq!(answer["within_tolerance"], inside);
    // Each sampled point's distance, the worst first with its closest point.
    let distances = answer["distances"].as_array().unwrap();
    assert_eq!(distances.len(), outside + inside);
    assert!((distances[0].as_f64().unwrap() - 0.5).abs() < TOL);
    assert!((distances[1].as_f64().unwrap() + 0.3).abs() < TOL);
    let worst = answer["worst"].as_array().unwrap();
    assert_eq!(worst.len(), 10);
    assert!((worst[0]["distance"].as_f64().unwrap() - 0.5).abs() < TOL);
    let point = worst[0]["point"].as_array().unwrap();
    let closest = worst[0]["closest"].as_array().unwrap();
    let gap: f64 = (0..3)
        .map(|k| (point[k].as_f64().unwrap() - closest[k].as_f64().unwrap()).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!((gap - 0.5).abs() < TOL, "{gap}");
    // Without a sign: every distance positive.
    let unsigned = query(
        &doc,
        json!({"query": "point_cloud_deviation", "cloud": "PointCloud1", "body": "F2.b0", "signed": false}),
    );
    assert_eq!(unsigned["signed"], false);
    assert_eq!(unsigned["inside"], 0);
    assert!((unsigned["min_mm"].as_f64().unwrap() - 0.3).abs() < TOL);
}

#[test]
fn a_deviation_follows_a_moved_cloud() {
    let mut doc = block();
    // On the top face, then moved 2 up: 2 out of the material.
    insert(
        &mut doc,
        &[[30.0, 20.0, 20.0], [10.0, 10.0, 20.0]],
        json!({}),
    );
    command(
        &mut doc,
        json!({"cmd": "move_point_cloud", "name": "PointCloud1", "translate": [0, 0, 2]}),
    );
    let answer = query(
        &doc,
        json!({"query": "point_cloud_deviation", "cloud": "PointCloud1", "body": "F2.b0"}),
    );
    assert!(
        (answer["mean_signed_mm"].as_f64().unwrap() - 2.0).abs() < TOL,
        "{answer}"
    );
}

#[test]
fn sampling_keeps_every_nth_point() {
    let mut doc = block();
    let points: Vec<[f64; 3]> = (0..10).map(|i| [5.0 + f64::from(i), 20.0, 21.0]).collect();
    insert(&mut doc, &points, json!({}));
    let answer = query(
        &doc,
        json!({"query": "point_cloud_deviation", "cloud": "PointCloud1", "body": "F2.b0",
               "max_samples": 3, "distances": true, "worst": 2}),
    );
    // 10 points, at most 3: every 4th (0, 4, 8).
    assert_eq!(answer["points"], 10);
    assert_eq!(answer["step"], 4);
    assert_eq!(answer["sampled"], 3);
    assert_eq!(answer["distances"].as_array().unwrap().len(), 3);
    let indices: Vec<u64> = answer["worst"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["index"].as_u64().unwrap())
        .collect();
    assert_eq!(indices.len(), 2);
    assert!(indices.iter().all(|i| i % 4 == 0), "{indices:?}");
}

#[test]
fn an_open_surface_refuses_a_sign() {
    let doc = Document::new(MockKernel::default());
    doc.kernel().copies_by_bounds.set(true);
    let mut doc = doc;
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
            {"cmd": "add_feature", "def": {"type": "patch", "profiles": [{"sketch": "F1", "region": R}]}},
        ]),
    );
    insert(
        &mut doc,
        &[[10.0, 10.0, 1.0], [20.0, 10.0, -1.0]],
        json!({}),
    );
    let error = query_error(
        &doc,
        json!({"query": "point_cloud_deviation", "cloud": "PointCloud1", "body": "F2.b0", "signed": true}),
    );
    assert!(error.contains("encloses no volume"), "{error}");
    // Without asking: measured without a sign.
    let answer = query(
        &doc,
        json!({"query": "point_cloud_deviation", "cloud": "PointCloud1", "body": "F2.b0"}),
    );
    assert_eq!(answer["signed"], false);
}

#[test]
fn a_deviation_analysis_is_kept_and_follows_its_cloud() {
    let mut doc = block();
    insert(&mut doc, &[[30.0, 20.0, 20.5]], json!({}));
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "point_cloud_deviation", "cloud": "PointCloud1",
               "body": "F2.b0", "tolerance": 0.2, "limit": 1.0}}),
    );
    assert_eq!(added["name"], "Deviation1");
    let analyses = query(&doc, json!({"query": "analyses"}));
    let analysis = &analyses[0];
    assert_eq!(analysis["type"], "point_cloud_deviation");
    assert_eq!(analysis["cloud"], "PointCloud1");
    assert_eq!(analysis["max_samples"], 100_000);
    assert!(analysis.get("error").is_none(), "{analysis}");
    command(
        &mut doc,
        json!({"cmd": "rename_point_cloud", "name": "PointCloud1", "new_name": "Scan"}),
    );
    assert_eq!(
        query(&doc, json!({"query": "analyses"}))[0]["cloud"],
        "Scan"
    );
    command(
        &mut doc,
        json!({"cmd": "delete_point_cloud", "name": "Scan"}),
    );
    let error = query(&doc, json!({"query": "analyses"}))[0]["error"].clone();
    assert_eq!(error, "the point cloud Scan is gone");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "point_cloud_deviation", "cloud": "Scan",
               "body": "F2.b0", "tolerance": 0}}),
    );
    assert!(
        error.contains("tolerance must be greater than zero"),
        "{error}"
    );
}

/// The distances of an answer asked with `"distances": true`.
fn distances_of(answer: &Value) -> Vec<f64> {
    answer["distances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_f64().unwrap())
        .collect()
}

fn near_all(actual: &[f64], expected: &[f64]) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .all(|(a, e)| (a - e).abs() < TOL)
}

/// mitcad#2265: a deviation from some of the body's faces only. A point
/// 0.5 above the block's top and one 0.3 above its bottom (inside the
/// material): from every face +0.5 and -0.3; from the top face (its end cap)
/// the inner point is 19.7 from it, still signed inside, its closest point
/// on the top; from the bottom (its start cap) the outer one is 20.5 away;
/// from both, each from the nearer. A face the body lacks is refused in
/// words. The faces are kept with the analysis, saved and read back.
#[test]
fn a_deviation_from_some_faces_only_is_asked_and_kept() {
    let mut doc = block();
    insert(
        &mut doc,
        &[[30.0, 20.0, 20.5], [30.0, 20.0, 0.3]],
        json!({}),
    );
    let ask = |doc: &Document<MockKernel>, faces: Value| {
        query(
            doc,
            json!({"query": "point_cloud_deviation", "cloud": "PointCloud1", "body": "F2.b0",
                   "faces": faces, "distances": true}),
        )
    };
    // The block's end caps, named with their region (`F2:end(r{…})`).
    let top_face = format!("F2:end({R})");
    let bottom_face = format!("F2:start({R})");
    let all = ask(&doc, json!([]));
    assert!(near_all(&distances_of(&all), &[0.5, -0.3]), "{all}");
    let top = ask(&doc, json!([top_face]));
    assert!(near_all(&distances_of(&top), &[0.5, -19.7]), "{top}");
    assert_eq!(top["signed"], true);
    assert_eq!(top["outside"], 1);
    assert_eq!(top["inside"], 1);
    assert!(
        (top["max_abs_mm"].as_f64().unwrap() - 19.7).abs() < TOL,
        "{top}"
    );
    // The farthest first, the nearest point of the top face with it.
    let worst = &top["worst"][0];
    assert_eq!(worst["index"], 1, "{worst}");
    let closest = worst["closest"].as_array().unwrap();
    assert!((closest[2].as_f64().unwrap() - 20.0).abs() < TOL, "{worst}");
    let bottom = ask(&doc, json!([bottom_face]));
    assert!(near_all(&distances_of(&bottom), &[20.5, -0.3]), "{bottom}");
    let both = ask(&doc, json!([top_face, bottom_face]));
    assert!(near_all(&distances_of(&both), &[0.5, -0.3]), "{both}");
    let error = query_error(
        &doc,
        json!({"query": "point_cloud_deviation", "cloud": "PointCloud1", "body": "F2.b0",
               "faces": ["F9:end"]}),
    );
    assert!(error.contains("F9:end"), "{error}");
    // Kept with the analysis, and in the file.
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "point_cloud_deviation", "cloud": "PointCloud1",
               "body": "F2.b0", "faces": [top_face]}}),
    );
    let kept = query(&doc, json!({"query": "analyses"}))[0].clone();
    assert_eq!(kept["faces"], json!([top_face]), "{kept}");
    assert!(kept.get("error").is_none(), "{kept}");
    let saved = doc.to_json();
    let back = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(back.to_json(), saved);
    assert_eq!(
        query(&back, json!({"query": "analyses"}))[0]["faces"],
        json!([top_face])
    );
}

/// mitcad#2265: a cloud placed in a component follows the component's
/// occurrence to another parent in the same undo step (`updated` names
/// it): its occurrence path leads through the new parent, it stays where
/// it was in the design, and the deviation kept of it finds it and
/// measures the same distances. Undo takes it back.
#[test]
fn a_cloud_follows_its_component_to_another_parent() {
    let doc = Document::new(MockKernel::default());
    doc.kernel().copies_by_bounds.set(true);
    let mut doc = doc;
    // Arm, 50 up, with a 20 x 20 x 20 block (F2.b0).
    let arm = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Arm", "transform": {"translation": [0, 0, 50]}}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    // Cell, 100 along X.
    let cell = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Cell", "transform": {"translation": [100, 0, 0]}}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    // Two points 0.5 above the block's top, in Arm's coordinates.
    insert(
        &mut doc,
        &[[5.0, 5.0, 20.5], [15.0, 15.0, 20.5]],
        json!({"occurrence": arm}),
    );
    let placed = clouds(&doc)[0].clone();
    assert_eq!(placed["occurrence"], json!(arm), "{placed}");
    assert_eq!(
        placed["bounds"],
        json!([[5.0, 5.0, 70.5], [15.0, 15.0, 70.5]])
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "point_cloud_deviation", "cloud": "PointCloud1",
               "body": "F2.b0"}}),
    );
    let measure = |doc: &Document<MockKernel>| {
        query(
            doc,
            json!({"query": "point_cloud_deviation", "cloud": "PointCloud1", "body": "F2.b0",
                   "distances": true}),
        )
    };
    let before = measure(&doc);
    assert!(near_all(&distances_of(&before), &[0.5, 0.5]), "{before}");

    let result = command(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": arm, "parent": cell}),
    );
    let updated = result["updated"].as_array().unwrap();
    assert!(updated.contains(&json!("PointCloud1")), "{result}");
    let moved = clouds(&doc)[0].clone();
    assert_eq!(
        moved["occurrence"],
        json!(format!("{cell}/{arm}")),
        "{moved}"
    );
    // Where it was in the design.
    let corners: Vec<f64> = moved["bounds"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|corner| corner.as_array().unwrap().clone())
        .map(|c| c.as_f64().unwrap())
        .collect();
    assert!(
        near_all(&corners, &[5.0, 5.0, 70.5, 15.0, 15.0, 70.5]),
        "{moved}"
    );
    // The kept deviation still finds its cloud and body, and measures the
    // same.
    let analysis = query(&doc, json!({"query": "analyses"}))[0].clone();
    assert_eq!(analysis["cloud"], "PointCloud1");
    assert!(analysis.get("error").is_none(), "{analysis}");
    let after = measure(&doc);
    assert!(
        near_all(&distances_of(&after), &distances_of(&before)),
        "{after}"
    );

    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(clouds(&doc)[0]["occurrence"], json!(arm));
}
