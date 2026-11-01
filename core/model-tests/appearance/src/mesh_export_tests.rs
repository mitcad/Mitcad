// SPDX-License-Identifier: MIT
//! Save As Mesh's options in the model's `export` (mitcad#734): STL, 3MF
//! and OBJ files in a length unit, meshes with a longest edge, OBJ files
//! on the print bed, and a file per body, each body on its own print bed
//! face. The mock kernel's meshes are the bodies' boxes.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::exchange::{ExportFormat, LengthUnit};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const TOL: f64 = 1e-6;

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

/// A 20 x 20 x 20 block (F2.b0, Body1) on a rectangle on XY.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc
}

/// The block and a second body under it, 10 high (F3.b0, Body2).
fn two_bodies() -> Document<MockKernel> {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
               "extent": {"type": "distance", "distance": -10}, "operation": "new_body"}}),
    );
    doc
}

/// A folder of the test's own for the files it writes.
fn folder(name: &str) -> std::path::PathBuf {
    mitcad_model::testing::unique_temp_dir(&format!("mesh-export-{name}"))
}

fn path_in(dir: &std::path::Path, name: &str) -> String {
    dir.join(name).to_str().unwrap().to_owned()
}

/// The triangles of a text STL file.
fn stl_triangles(path: &str) -> Vec<[[f64; 3]; 3]> {
    let text = std::fs::read_to_string(path).unwrap();
    let points: Vec<[f64; 3]> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("vertex "))
        .map(|numbers| {
            let p: Vec<f64> = numbers
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            [p[0], p[1], p[2]]
        })
        .collect();
    points.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
}

fn bounds(points: impl IntoIterator<Item = [f64; 3]>) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in points {
        min = std::array::from_fn(|i| min[i].min(p[i]));
        max = std::array::from_fn(|i| max[i].max(p[i]));
    }
    (min, max)
}

/// The volume the triangles enclose, facing outwards.
fn volume(triangles: &[[[f64; 3]; 3]]) -> f64 {
    triangles
        .iter()
        .map(|[a, b, c]| {
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

fn longest_edge(triangles: &[[[f64; 3]; 3]]) -> f64 {
    let length =
        |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt();
    triangles
        .iter()
        .flat_map(|[a, b, c]| [length(*a, *b), length(*b, *c), length(*c, *a)])
        .fold(0.0, f64::max)
}

#[test]
fn mesh_files_are_written_in_the_unit() {
    let mut doc = block();
    let dir = folder("unit");
    // STL in centimetres: a tenth of the numbers.
    let stl = path_in(&dir, "block.stl");
    command(
        &mut doc,
        json!({"cmd": "export", "path": stl, "ascii": true, "unit": "cm"}),
    );
    let (min, max) = bounds(stl_triangles(&stl).into_iter().flatten());
    assert_eq!(min, [0.0; 3]);
    assert!((0..3).all(|i| (max[i] - 2.0).abs() <= TOL), "{max:?}");
    // In millimetres as before by default.
    command(
        &mut doc,
        json!({"cmd": "export", "path": stl, "ascii": true}),
    );
    let (_, max) = bounds(stl_triangles(&stl).into_iter().flatten());
    assert!((0..3).all(|i| (max[i] - 20.0).abs() <= TOL), "{max:?}");

    // 3MF in inches: the file says so and reads back at its size; the
    // result's volume is in mm³.
    let three = path_in(&dir, "block.3mf");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": three, "unit": "in"}),
    );
    assert!((result["meshes"][0]["volume"].as_f64().unwrap() - 8000.0).abs() <= TOL);
    let model = mitcad_3mf::read(&std::fs::read(&three).unwrap()).unwrap();
    // Within a micrometre of a millimetre: the file's inches keep a
    // nanometre (8 decimals), not a millimetre's 6.
    let (min, max) = bounds(model.parts[0].mesh.vertices.iter().copied());
    assert!(
        (0..3).all(|i| min[i].abs() <= TOL && (max[i] - 20.0).abs() <= TOL),
        "{min:?} {max:?}"
    );

    // OBJ: the kernel writes it in the unit.
    command(
        &mut doc,
        json!({"cmd": "export", "path": path_in(&dir, "block.obj"), "unit": "ft"}),
    );
    assert_eq!(
        doc.kernel().write_units.borrow().last(),
        Some(&LengthUnit::Foot)
    );
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_longest_edge_splits_the_mesh_keeping_its_shape() {
    let mut doc = block();
    let dir = folder("edges");
    let stl = path_in(&dir, "block.stl");
    // The presets have no longest edge: the box's 12 triangles.
    command(
        &mut doc,
        json!({"cmd": "export", "path": stl, "ascii": true, "refinement": "high"}),
    );
    assert_eq!(stl_triangles(&stl).len(), 12);
    command(
        &mut doc,
        json!({"cmd": "export", "path": stl, "ascii": true,
               "refinement": {"deviation": 0.03, "angle": 0.26, "max_edge": 5}}),
    );
    let triangles = stl_triangles(&stl);
    assert!(triangles.len() > 12);
    assert!(
        longest_edge(&triangles) <= 5.0 + TOL,
        "{}",
        longest_edge(&triangles)
    );
    assert!(
        (volume(&triangles) - 8000.0).abs() <= 1e-6,
        "{}",
        volume(&triangles)
    );
    let (min, max) = bounds(triangles.into_iter().flatten());
    assert_eq!((min, max), ([0.0; 3], [20.0; 3]));

    // 3MF too.
    let three = path_in(&dir, "block.3mf");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": three, "refinement": {"deviation": 0.03, "angle": 0.26, "max_edge": 5}}),
    );
    assert!(result["meshes"][0]["triangles"].as_u64().unwrap() > 12);
    assert!((result["meshes"][0]["volume"].as_f64().unwrap() - 8000.0).abs() <= 1e-6);

    // OBJ: the model's split mesh as a mesh body, written by the kernel.
    let before = doc.kernel().count("triangle_body");
    command(
        &mut doc,
        json!({"cmd": "export", "path": path_in(&dir, "block.obj"),
               "refinement": {"deviation": 0.03, "angle": 0.26, "max_edge": 5}}),
    );
    assert_eq!(doc.kernel().count("triangle_body"), before + 1);
    let written = doc.kernel().written.borrow().last().cloned().unwrap();
    assert_eq!(written.1, ExportFormat::Obj);
    assert_eq!(written.2, vec![("Body1".to_owned(), None)]);
    // Without a longest edge the kernel meshes the body itself.
    command(
        &mut doc,
        json!({"cmd": "export", "path": path_in(&dir, "plain.obj")}),
    );
    assert_eq!(doc.kernel().count("triangle_body"), before + 1);

    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": stl, "refinement": {"deviation": 0.03, "angle": 0.26, "max_edge": -1}}),
    );
    assert!(error.contains("maximum edge length"), "{error}");
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn an_obj_file_lies_on_the_print_bed_face() {
    let mut doc = block();
    let dir = folder("obj-bed");
    let top = format!("F2:end({R})");
    command(
        &mut doc,
        json!({"cmd": "set_print_bed_face", "uid": "F2.b0", "face": top}),
    );
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path_in(&dir, "print.obj"), "bodies": ["F2.b0"], "on_bed": true}),
    );
    assert_eq!(result["bed"]["face"], json!(top));
    assert_eq!(result["warnings"], json!([]));
    // The kernel gets the body at the placement that turns it onto its top,
    // the lowest point on the bed and centred on the plate.
    let placements = doc
        .kernel()
        .written_placements
        .borrow()
        .last()
        .cloned()
        .unwrap();
    assert_eq!(placements.len(), 1);
    assert_eq!(placements[0].len(), 1);
    let transform = placements[0][0].transform;
    let corners = (0..8).map(|i: usize| {
        transform.apply_point(std::array::from_fn(|axis| {
            if (i >> axis) & 1 == 1 { 20.0 } else { 0.0 }
        }))
    });
    let (min, max) = bounds(corners);
    assert!(min[2].abs() <= TOL, "{min:?}");
    assert!(
        ((min[0] + max[0]) / 2.0 - 128.0).abs() <= TOL,
        "{min:?} {max:?}"
    );
    assert!(
        ((min[1] + max[1]) / 2.0 - 128.0).abs() <= TOL,
        "{min:?} {max:?}"
    );
    let down = transform.apply_vector([0.0, 0.0, 1.0]);
    assert!((down[2] + 1.0).abs() <= TOL, "{down:?}");
    // The design's coordinates only, as for STL and 3MF.
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": path_in(&dir, "print.obj"), "on_bed": true, "coordinates": "component"}),
    );
    assert!(error.contains("design's coordinates"), "{error}");
    // Other formats are refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": path_in(&dir, "print.usdz"), "on_bed": true}),
    );
    assert!(error.contains("only 3MF, STL and OBJ"), "{error}");
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_file_per_body_puts_each_body_on_its_own_print_bed_face() {
    let mut doc = two_bodies();
    let dir = folder("per-body");
    command(
        &mut doc,
        json!({"cmd": "set_print_bed_face", "uid": "F2.b0", "face": format!("F2:end({R})")}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_print_bed_face", "uid": "F3.b0", "face": format!("F3:start({R})")}),
    );
    // One file: one piece, kept as assembled with two bed faces.
    let both = command(
        &mut doc,
        json!({"cmd": "export", "path": path_in(&dir, "both.stl"), "ascii": true, "on_bed": true}),
    );
    assert_eq!(both["bodies"], json!(["F2.b0", "F3.b0"]));
    assert_eq!(both["bed"]["kept"], json!(["F2.b0", "F3.b0"]));
    // A file per body (Save As Mesh's one file per body): an export of each
    // body alone, each turned onto its own face and centred on the plate.
    for (body, face, height) in [
        ("F2.b0", format!("F2:end({R})"), 20.0),
        ("F3.b0", format!("F3:start({R})"), 10.0),
    ] {
        let path = path_in(&dir, &format!("{body}.stl"));
        let one = command(
            &mut doc,
            json!({"cmd": "export", "path": path, "bodies": [body], "ascii": true, "on_bed": true, "unit": "mm"}),
        );
        assert_eq!(one["bodies"], json!([body]));
        assert_eq!(one["bed"]["body"], body);
        assert_eq!(one["bed"]["face"], json!(face));
        let (min, max) = bounds(stl_triangles(&path).into_iter().flatten());
        assert!(min[2].abs() <= TOL, "{body}: {min:?}");
        assert!((max[2] - height).abs() <= TOL, "{body}: {max:?}");
        assert!(
            ((min[0] + max[0]) / 2.0 - 128.0).abs() <= TOL,
            "{body}: {min:?} {max:?}"
        );
    }
    mitcad_model::testing::remove_temp(&dir);
}
