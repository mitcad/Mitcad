// SPDX-License-Identifier: MIT
//! `model_render`'s line art (mitcad#1736) on the mock kernel, whose
//! projection is each body's box, all twelve edges seen: the SVG's groups
//! and paths, its frame in the picture's pixels, the same text twice,
//! colors, highlight and explode as the PNG applies them, and the options
//! refused. The OCCT kernel's hidden lines (a box from the iso view: nine
//! edges seen, three hidden) are tested end to end by the CLI test
//! `cli.mcp` (`tools/cli/mcp-test.py`, `test_model_render_line_art`).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::preview::{LineArt, Projection, RenderArgs};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const SIZE: [u32; 2] = [200, 100];

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

/// A 20 x 20 x 20 block on a rectangle on XY at `x`, its sketch `sketch`.
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

/// One block, F2.b0.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    add_block(&mut doc, "F1", 0.0);
    doc
}

fn args(arguments: Value) -> RenderArgs {
    RenderArgs::from_json(&arguments).unwrap_or_else(|e| panic!("{arguments}: {e}"))
}

fn lines(doc: &Document<MockKernel>, arguments: Value) -> LineArt {
    doc.render_lines(&args(arguments.clone()))
        .unwrap_or_else(|e| panic!("{arguments}: {e}"))
}

/// The text of the group `id` (empty when there is none).
fn group<'a>(svg: &'a str, id: &str) -> &'a str {
    let open = format!("<g id=\"{id}\"");
    let Some(start) = svg.find(&open) else {
        return "";
    };
    let rest = &svg[start..];
    &rest[..rest.find("</g>").unwrap_or(rest.len())]
}

fn paths(text: &str) -> usize {
    text.matches("<path ").count()
}

/// Every number of the paths' `d` attributes, as x, y pairs.
fn points(svg: &str) -> Vec<[f64; 2]> {
    let mut found = Vec::new();
    for piece in svg.split(" d=\"").skip(1) {
        let d = &piece[..piece.find('"').unwrap()];
        let numbers: Vec<f64> = d
            .split(' ')
            .filter_map(|word| word.parse::<f64>().ok())
            .collect();
        for pair in numbers.chunks(2) {
            if let [x, y] = pair {
                found.push([*x, *y]);
            }
        }
    }
    found
}

#[test]
fn a_block_is_twelve_paths_framed_in_the_picture() {
    let doc = block();
    let art = lines(&doc, json!({"view": "top", "size": SIZE, "format": "svg"}));
    assert_eq!(art.camera.projection, Projection::Orthographic);
    assert_eq!(art.bodies.len(), 1);
    assert_eq!((art.width, art.height), (200, 100));
    assert_eq!((art.visible, art.hidden, art.highlighted), (12, 0, 0));
    assert!(art.svg.starts_with("<?xml"));
    assert!(art.svg.contains(r#"viewBox="0 0 200 100""#));
    assert_eq!(paths(group(&art.svg, "visible")), 12);
    // No hidden group unless asked for, no background.
    assert!(group(&art.svg, "hidden").is_empty());
    assert!(!art.svg.contains("<rect"));
    // From the top, the block is centred: its 20 mm square 85 per cent of
    // the 100 pixels' height.
    let all = points(&art.svg);
    let (min_x, max_x) = all.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
        (lo.min(p[0]), hi.max(p[0]))
    });
    let (min_y, max_y) = all.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
        (lo.min(p[1]), hi.max(p[1]))
    });
    assert!((min_x + max_x - 200.0).abs() < 1e-2, "{min_x} {max_x}");
    assert!((min_y + max_y - 100.0).abs() < 1e-2, "{min_y} {max_y}");
    assert!((max_y - min_y - 85.0).abs() < 1.0, "{min_y} {max_y}");
    assert!((max_x - min_x - (max_y - min_y)).abs() < 1e-2);
    // The kernel projected the block once.
    assert_eq!(doc.kernel().projected.borrow().len(), 1);
    // The same arguments, the same text; the hidden group when asked for.
    let again = lines(&doc, json!({"view": "top", "size": SIZE, "format": "svg"}));
    assert_eq!(again.svg, art.svg);
    let dashed = lines(
        &doc,
        json!({"view": "top", "size": SIZE, "format": "svg", "hidden_edges": true}),
    );
    assert!(group(&dashed.svg, "hidden").contains("stroke-dasharray"));
    assert_eq!(dashed.visible, 12);
    // A background colour is a rectangle under the lines.
    let white = lines(
        &doc,
        json!({"view": "iso", "size": SIZE, "format": "svg", "background": "white"}),
    );
    assert!(white.svg.contains(r##"fill="#ffffff""##));
}

#[test]
fn colors_and_highlight_stroke_the_lines() {
    let doc = block();
    let base = json!({"view": "iso", "size": SIZE, "format": "svg"});
    let plain = lines(&doc, base.clone());
    assert!(!plain.svg.contains("#1f7aff"));
    let mut colored = base.clone();
    colored["colors"] = json!({"F2.b0": "#e01010"});
    let colored = lines(&doc, colored);
    assert_eq!(
        group(&colored.svg, "visible")
            .matches(r##"stroke="#e01010""##)
            .count(),
        12
    );
    let mut lit = base.clone();
    lit["highlight"] = json!(["F2"]);
    let lit = lines(&doc, lit);
    assert_eq!(
        group(&lit.svg, "visible")
            .matches(r##"stroke="#1f7aff""##)
            .count(),
        12
    );
    assert!(lit.warnings.is_empty(), "{:?}", lit.warnings);
    // An unknown reference is a warning, the lines as they were.
    let mut unknown = base;
    unknown["highlight"] = json!(["F9.b0"]);
    let unknown = lines(&doc, unknown);
    assert_eq!(unknown.svg, plain.svg);
    assert_eq!(unknown.warnings.len(), 1, "{:?}", unknown.warnings);
}

#[test]
fn explode_moves_the_blocks_apart_as_the_picture_does() {
    let mut doc = block();
    add_block(&mut doc, "F3", 30.0);
    let base = json!({"view": "front", "size": SIZE, "format": "svg"});
    let together = lines(&doc, base.clone());
    assert_eq!(together.visible, 24);
    let mut apart = base;
    apart["explode"] = json!(1);
    let apart = lines(&doc, apart);
    assert_eq!(apart.visible, 24);
    assert!(apart.camera.height > together.camera.height * 1.2);
    // The PNG's camera for the same arguments.
    let same = args(json!({
        "view": "front", "size": SIZE, "explode": 1, "projection": "orthographic"
    }));
    let picture = doc.preview_camera(&same).unwrap();
    assert_eq!(picture.camera, apart.camera);
}

#[test]
fn the_png_s_options_and_a_perspective_are_refused() {
    let doc = block();
    for (extra, word) in [
        (json!({"projection": "perspective"}), "orthographic"),
        (
            json!({"section": {"origin": [0, 0, 10], "normal": [0, 0, 1]}}),
            "section",
        ),
        (json!({"overlay": "overhang"}), "overlay"),
        (json!({"edges": false}), "edges"),
        (json!({"print_colors": true}), "print_colors"),
    ] {
        let mut arguments = json!({"view": "iso", "format": "svg"});
        for (key, value) in extra.as_object().unwrap() {
            arguments[key] = value.clone();
        }
        match doc.render_lines(&args(arguments.clone())) {
            Ok(_) => panic!("{arguments} was drawn"),
            Err(refused) => assert!(refused.to_string().contains(word), "{refused}"),
        }
    }
    // A view without a projection of its own is drawn orthographic.
    let home = lines(&doc, json!({"format": "svg"}));
    assert_eq!(home.camera.projection, Projection::Orthographic);
}
