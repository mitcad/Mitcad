// SPDX-License-Identifier: MIT
//! `model_render`'s overlay (mitcad#1488) on the mock kernel's box meshes:
//! a body with one 60-degree overhang whose pixels take the overlay's color
//! while the rest do not, with the area Print Analysis gives; thin walls
//! from `print_thickness`' own result; faces under the draft; and the body
//! a failed feature read, marked as the failure left it.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::preview::{Preview, RenderArgs, png};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const TOL: f64 = 1e-6;
const SIZE: [u32; 2] = [160, 120];

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

fn close(value: &Value, expected: f64) -> bool {
    (number(value) - expected).abs() <= TOL
}

fn mesh_calls(doc: &Document<MockKernel>) -> usize {
    doc.kernel()
        .calls
        .borrow()
        .get("triangle_mesh")
        .copied()
        .unwrap_or(0)
}

/// A 20 x 20 x 20 block (F2.b0) on a rectangle on XY at `x`.
fn add_block(doc: &mut Document<MockKernel>, sketch: &str, x: f64) {
    command(
        doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": [x, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": sketch, "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
}

fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    add_block(&mut doc, "F1", 0.0);
    doc
}

fn render(doc: &Document<MockKernel>, arguments: Value) -> Preview {
    let args = RenderArgs::from_json(&arguments).unwrap_or_else(|e| panic!("{arguments}: {e}"));
    doc.render_preview(&args, None)
        .unwrap_or_else(|e| panic!("{arguments}: {e}"))
}

/// The overlay's color from its summary (`#rrggbb`).
fn color_of(summary: &Value) -> [u8; 3] {
    let text = summary["color"].as_str().unwrap();
    let byte = |i: usize| u8::from_str_radix(&text[i..i + 2], 16).unwrap();
    [byte(1), byte(3), byte(5)]
}

/// The picture's pixel where the point `(right, up)` of the design is, in
/// an orthographic view: millimeters along the camera's right and up from
/// its target.
fn pixel(preview: &Preview, right: f64, up: f64) -> [u8; 3] {
    let (width, height, channels, pixels) = png::decode(&preview.full.png).unwrap();
    let scale = f64::from(height) / preview.camera.height;
    let x = (f64::from(width) / 2.0 + right * scale).floor() as usize;
    let y = (f64::from(height) / 2.0 - up * scale).floor() as usize;
    let i = (y * width as usize + x) * channels;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

/// The middle of the picture.
fn middle(preview: &Preview) -> [u8; 3] {
    pixel(preview, 0.0, 0.0)
}

#[test]
fn a_60_degree_overhang_takes_the_overlay_color_with_print_analysis_area() {
    let doc = block();
    // Printed with up tilted 30 degrees about Y: the bottom face, 60
    // degrees from the vertical, overhangs; the side the tilt turns down
    // is 30 degrees from the vertical and prints.
    let half = 3f64.sqrt() / 2.0;
    let up = [-0.5, 0.0, half];
    let overlay = json!({"type": "overhang", "up": up});
    let arguments = |view: &str| json!({"view": view, "projection": "orthographic", "size": SIZE, "overlay": overlay});
    let below = render(&doc, arguments("bottom"));
    let summary = below.overlay.clone().expect("the overlay's summary");
    assert_eq!(summary["type"], "overhang");
    assert_eq!(summary["color"], "#7e3fd0");
    assert_eq!(summary["angle"], 45.0);
    let body = &summary["bodies"][0];
    assert_eq!(body["body"], "F2.b0");
    assert_eq!(body["triangles"], 2);
    assert!(close(&body["overhang_area"], 400.0), "{summary}");
    assert!(close(&body["area"], 2400.0), "{summary}");
    assert!(close(&summary["overhang_area"], 400.0), "{summary}");
    // Print Analysis' numbers for the same body and direction.
    let analysis = query(
        &doc,
        json!({"query": "print_overhangs", "bodies": ["F2.b0"], "down": [0.5, 0.0, -half]}),
    );
    let analysed = &analysis["bodies"][0];
    assert!(close(
        &body["overhang_area"],
        number(&analysed["overhang_area"])
    ));
    assert!(close(&body["area"], number(&analysed["area"])));
    assert!(close(&body["share"], number(&analysed["share"])));
    // The bottom's pixels take the overlay's color; seen from the top and
    // the front, the faces do not.
    let color = color_of(&summary);
    assert_eq!(middle(&below), color);
    assert_eq!(pixel(&below, -5.0, 5.0), color);
    for view in ["top", "front", "right"] {
        let seen = render(&doc, arguments(view));
        assert_ne!(middle(&seen), color, "{view}");
    }
    // Without the overlay the bottom keeps the body's color.
    let plain = render(
        &doc,
        json!({"view": "bottom", "projection": "orthographic", "size": SIZE}),
    );
    assert!(plain.overlay.is_none());
    assert_ne!(middle(&plain), color);
    // Printed with +Z up (the default) the block stands on its bottom:
    // nothing overhangs, nothing is overlaid.
    let upright = render(
        &doc,
        json!({"view": "bottom", "projection": "orthographic", "size": SIZE, "overlay": "overhang"}),
    );
    let summary = upright.overlay.clone().unwrap();
    assert!(close(&summary["overhang_area"], 0.0), "{summary}");
    assert_eq!(summary["bodies"][0]["bed_face"], false);
    assert_eq!(middle(&upright), middle(&plain));
}

#[test]
fn the_print_bed_face_orients_the_overhangs() {
    let mut doc = block();
    let top = format!("F2:end({R})");
    command(
        &mut doc,
        json!({"cmd": "set_print_bed_face", "uid": "F2.b0", "face": top}),
    );
    // On its top: the top lies on the bed, nothing overhangs, as Print
    // Analysis has it on that face.
    let preview = render(&doc, json!({"size": SIZE, "overlay": "overhang"}));
    let summary = preview.overlay.unwrap();
    let body = &summary["bodies"][0];
    assert_eq!(body["bed_face"], true);
    assert_eq!(body["down"], json!([0.0, 0.0, 1.0]));
    let analysis = query(
        &doc,
        json!({"query": "print_overhangs", "bodies": ["F2.b0"],
               "bed_face": {"body": "F2.b0", "face": top}}),
    );
    assert!(close(
        &body["overhang_area"],
        number(&analysis["bodies"][0]["overhang_area"])
    ));
    assert!(close(&body["overhang_area"], 0.0));
    // Up given wins over the bed face.
    let preview = render(
        &doc,
        json!({"size": SIZE, "overlay": {"type": "overhang", "up": [0, 0, -1]}}),
    );
    let summary = preview.overlay.unwrap();
    assert_eq!(summary["bodies"][0]["bed_face"], false);
    assert!(close(&summary["overhang_area"], 0.0));
}

#[test]
fn thin_walls_are_print_thickness_own() {
    let doc = block();
    let front = |overlay: Value| {
        render(
            &doc,
            json!({"view": "front", "projection": "orthographic", "size": SIZE, "overlay": overlay}),
        )
    };
    // Every wall of the 20 mm block is thinner than 25 mm.
    let thin = front(json!({"type": "thickness", "minimum": 25}));
    let summary = thin.overlay.clone().unwrap();
    assert_eq!(summary["type"], "thickness");
    assert_eq!(summary["minimum"], 25.0);
    assert!(close(&summary["thinnest"], 20.0), "{summary}");
    assert_eq!(summary["regions"], 1);
    assert_eq!(summary["thin_triangles"], 12);
    assert!(close(&summary["thin_area"], 2400.0), "{summary}");
    assert_eq!(middle(&thin), color_of(&summary));
    // print_thickness answers from the same result: the mesh is not made
    // again, and the numbers agree.
    let meshed = mesh_calls(&doc);
    let analysis = query(
        &doc,
        json!({"query": "print_thickness", "bodies": ["F2.b0"], "minimum": 25}),
    );
    assert_eq!(mesh_calls(&doc), meshed);
    assert!(close(&analysis["thinnest"], number(&summary["thinnest"])));
    assert_eq!(analysis["regions"], summary["regions"]);
    assert_eq!(analysis["thin_triangles"], summary["thin_triangles"]);
    // At the default 0.8 mm nothing is thin.
    let thick = front(json!("thickness"));
    let summary = thick.overlay.clone().unwrap();
    assert_eq!(summary["minimum"], 0.8);
    assert_eq!(summary["thin_triangles"], 0);
    assert_ne!(middle(&thick), color_of(&summary));
    // A minimum that is none is refused.
    let args =
        RenderArgs::from_json(&json!({"overlay": {"type": "thickness", "minimum": 0}})).unwrap();
    assert!(doc.render_preview(&args, None).is_err());
}

#[test]
fn faces_under_the_draft() {
    let doc = block();
    // Pulled along Z the four sides have no draft; the top and the bottom
    // have all they need.
    let preview = render(
        &doc,
        json!({"view": "front", "projection": "orthographic", "size": SIZE, "overlay": "draft"}),
    );
    let summary = preview.overlay.clone().unwrap();
    assert_eq!(summary["type"], "draft");
    assert_eq!(summary["angle"], 1.0);
    assert!(close(&summary["under_area"], 1600.0), "{summary}");
    assert!(close(&summary["area"], 2400.0), "{summary}");
    assert_eq!(summary["bodies"][0]["triangles"], 8);
    assert_eq!(middle(&preview), color_of(&summary));
    let top = render(
        &doc,
        json!({"view": "top", "projection": "orthographic", "size": SIZE, "overlay": "draft"}),
    );
    assert_ne!(middle(&top), color_of(&summary));
    // Pulled along X: the front is under the draft still, in a color given.
    let preview = render(
        &doc,
        json!({"view": "front", "projection": "orthographic", "size": SIZE,
               "overlay": {"type": "draft", "pull": [1, 0, 0], "color": "#00ff00"}}),
    );
    let summary = preview.overlay.clone().unwrap();
    assert_eq!(summary["color"], "#00ff00");
    assert_eq!(middle(&preview), [0, 255, 0]);
}

#[test]
fn a_failed_feature_marks_the_body_it_read() {
    let mut doc = block();
    // A second block beside it, which no failure touches.
    add_block(&mut doc, "F3", 40.0);
    let top = |doc: &Document<MockKernel>| {
        render(
            doc,
            json!({"view": "top", "projection": "orthographic", "size": SIZE, "overlay": "failed"}),
        )
    };
    // Nothing failed: nothing is marked.
    let preview = top(&doc);
    let summary = preview.overlay.clone().unwrap();
    assert_eq!(summary["features"], json!([]));
    assert_eq!(summary["bodies"], json!([]));
    let color = color_of(&summary);
    let centre = |preview: &Preview, x: f64| {
        pixel(
            preview,
            x - preview.camera.target[0],
            10.0 - preview.camera.target[1],
        )
    };
    assert_ne!(centre(&preview, 10.0), color);
    // A fillet of the first block that fails: the block stays as it was,
    // marked; the second is not.
    doc.kernel().fail.borrow_mut().insert("fillet");
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
               "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 3}}),
    );
    let preview = top(&doc);
    let summary = preview.overlay.clone().unwrap();
    let features = summary["features"].as_array().unwrap();
    assert_eq!(features.len(), 1, "{summary}");
    assert_eq!(features[0]["feature"], "F5");
    assert!(!features[0]["error"].as_str().unwrap().is_empty());
    assert_eq!(features[0]["bodies"], json!(["F2.b0"]));
    assert_eq!(summary["bodies"], json!(["F2.b0"]));
    assert_eq!(summary["not_drawn"], json!([]));
    assert_eq!(centre(&preview, 10.0), color);
    assert_ne!(centre(&preview, 50.0), color);
    // The marked body not drawn is said so.
    let only = render(
        &doc,
        json!({"view": "top", "size": SIZE, "bodies": ["F4.b0"], "overlay": "failed"}),
    );
    let summary = only.overlay.unwrap();
    assert_eq!(summary["bodies"], json!([]));
    assert_eq!(summary["not_drawn"], json!(["F2.b0"]));
}
