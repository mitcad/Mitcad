// SPDX-License-Identifier: MIT
//! Widths and patterns chosen in a view (mitcad#1546) on the mock kernel:
//! an edge's lines by the edge's name, kept when the model changes; a
//! body's lines; the edge's choice winning over its body's; an occurrence's
//! choice ranked below a body's; the long dash double dotted pen; a reset;
//! refusals; the choices in the file, the undo, the SVG, PDF and DXF
//! exports (the DXF file's line types and line weights); the pen of the
//! lines a pick's mark lies on, for the application to draw it as wide
//! (mitcad#1618).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::ids::{BodyUid, OccurrenceUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_drawing::annotate::{PenPattern, PenWidth};
use mitcad_model_drawing::def::LineStyle;

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

fn sheet(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0].clone()
}

fn view(doc: &Document<MockKernel>, id: u64) -> Value {
    sheet(doc)["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no view {id}"))
}

fn line(start: [f64; 3], end: [f64; 3]) -> CurveGeometry {
    CurveGeometry::Line { start, end }
}

fn edge_names(doc: &Document<MockKernel>) -> Vec<String> {
    let body: BodyUid = "F2.b0".parse().unwrap();
    doc.kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .collect()
}

fn extrude(sketch: &str, distance: f64) -> Value {
    json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": R}],
           "extent": {"type": "distance", "distance": distance}, "operation": "new_body"})
}

/// A 40 x 30 x 20 block at the origin (F2.b0) and a 20 x 10 x 10 block at
/// x 50 (F4.b0); the first block's edge 0 the front's top edge, its edge 1
/// the front's bottom edge; a drawing with a front view at 1:1 (the model's
/// (x, z) at the view's place plus (x, z) on the sheet).
fn drawn() -> (Document<MockKernel>, Vec<String>) {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": extrude("F1", 20.0)},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [50, 0], "width": 20, "height": 10},
            {"cmd": "add_feature", "def": extrude("F3", 10.0)},
        ]),
    );
    doc.kernel().lists_geometry.set(true);
    let names = edge_names(&doc);
    assert!(names.len() >= 2, "{names:?}");
    {
        let mut curves = doc.kernel().edge_curves.borrow_mut();
        curves.insert(names[0].clone(), line([0.0, 0.0, 20.0], [40.0, 0.0, 20.0]));
        curves.insert(names[1].clone(), line([0.0, 0.0, 0.0], [40.0, 0.0, 0.0]));
    }
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    (doc, names)
}

/// The front view's lines longer than 1 mm: their pen and ends, sheet mm.
fn lines(doc: &Document<MockKernel>) -> Vec<(String, [f64; 2], [f64; 2])> {
    let sheet = sheet(doc);
    let group = sheet["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["class"] == "view")
        .cloned()
        .expect("the view's group");
    let point = |v: &Value| [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()];
    group["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item.get("line").is_some())
        .map(|item| {
            (
                item["pen"].as_str().unwrap().to_owned(),
                point(&item["line"][0]),
                point(&item["line"][1]),
            )
        })
        .filter(|(_, a, b)| (a[0] - b[0]).hypot(a[1] - b[1]) > 1.0)
        .collect()
}

/// The pens of the view's horizontal lines at the model's height `z` of
/// the block whose model x lies in `x0..x1`.
fn pens_at(doc: &Document<MockKernel>, z: f64, x0: f64, x1: f64) -> Vec<String> {
    let place = view(doc, 1)["place"].clone();
    let (px, py) = (place[0].as_f64().unwrap(), place[1].as_f64().unwrap());
    lines(doc)
        .into_iter()
        .filter(|(_, a, b)| {
            (a[1] - py - z).abs() < 1e-6
                && (b[1] - py - z).abs() < 1e-6
                && a[0].min(b[0]) >= px + x0 - 1e-6
                && a[0].max(b[0]) <= px + x1 + 1e-6
        })
        .map(|(pen, ..)| pen)
        .collect()
}

fn set_line(doc: &mut Document<MockKernel>, fields: Value) -> Value {
    let mut c = json!({"cmd": "edit_drawing_line", "drawing": "D1", "view": 1});
    for (k, v) in fields.as_object().unwrap() {
        c[k] = v.clone();
    }
    command(doc, c)
}

#[test]
fn an_edges_line_follows_its_edge_by_name_when_the_model_changes() {
    let (mut doc, names) = drawn();
    // Before: every visible line of the view wide and continuous.
    assert!(lines(&doc).iter().all(|(pen, ..)| pen == "thick"));
    let answer = set_line(
        &mut doc,
        json!({"body": "F2.b0", "edge": names[0], "width": "thick", "pattern": "phantom"}),
    );
    assert_eq!(answer["line"]["pattern"], "phantom", "{answer}");
    assert_eq!(answer["line"]["edge"], names[0].as_str(), "{answer}");
    let top = pens_at(&doc, 20.0, 0.0, 40.0);
    assert!(
        !top.is_empty() && top.iter().all(|p| p == "phantom_thick"),
        "{top:?}"
    );
    // The rest as before: the bottom, the other block.
    let bottom = pens_at(&doc, 0.0, 0.0, 40.0);
    assert!(
        !bottom.is_empty() && bottom.iter().all(|p| p == "thick"),
        "{bottom:?}"
    );
    let other = pens_at(&doc, 10.0, 50.0, 70.0);
    assert!(
        !other.is_empty() && other.iter().all(|p| p == "thick"),
        "{other:?}"
    );
    // The upright sides are not on the edge.
    let phantoms = lines(&doc)
        .into_iter()
        .filter(|(pen, ..)| pen == "phantom_thick")
        .count();
    assert_eq!(phantoms, top.len());
    // The pens' table has the long dash double dotted pen.
    let pens = sheet(&doc)["pens"].clone();
    assert_eq!(
        pens["phantom_thick"]["dashes"],
        json!([12.0, 1.5, 0.5, 1.5, 0.5, 1.5]),
        "{pens}"
    );
    assert_eq!(pens["phantom_thick"]["width"], json!(0.35), "{pens}");
    // The view lists its choice.
    let listed = view(&doc, 1)["lines"].clone();
    assert_eq!(listed.as_array().unwrap().len(), 1, "{listed}");
    assert_eq!(listed[0]["body"], "F2.b0");
    assert_eq!(listed[0]["width"], "thick");

    // The block made 25 high: the edge keeps its name and lies higher; its
    // line still takes the choice.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": extrude("F1", 25.0)}),
    );
    assert!(edge_names(&doc).contains(&names[0]), "{names:?}");
    doc.kernel()
        .edge_curves
        .borrow_mut()
        .insert(names[0].clone(), line([0.0, 0.0, 25.0], [40.0, 0.0, 25.0]));
    command(&mut doc, json!({"cmd": "recompute"}));
    let top = pens_at(&doc, 25.0, 0.0, 40.0);
    assert!(
        !top.is_empty() && top.iter().all(|p| p == "phantom_thick"),
        "{top:?}"
    );
    assert!(pens_at(&doc, 20.0, 0.0, 40.0).is_empty());
    assert_eq!(view(&doc, 1)["lines"][0]["edge"], names[0].as_str());
    assert!(view(&doc, 1).get("warnings").is_none_or(Value::is_null));

    // Kept in the file and read again as it was.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let kept = &file["drawings"][0]["sheets"][0]["views"][0]["lines"];
    assert_eq!(kept[0]["pattern"], "phantom", "{kept}");
    assert_eq!(kept[0]["edge"], names[0].as_str(), "{kept}");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);

    // A body whose choice the view keeps, deleted: the view warns and
    // keeps the choice.
    set_line(
        &mut doc,
        json!({"body": "F4.b0", "width": "thin", "pattern": "phantom"}),
    );
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F4"}));
    let warnings = view(&doc, 1)["warnings"].clone();
    assert!(
        warnings
            .as_array()
            .is_some_and(|w| w
                .iter()
                .any(|w| w.as_str().is_some_and(|w| w
                    .contains("the view no longer shows the body F4.b0: its line style is kept")))),
        "{warnings}"
    );
    assert_eq!(view(&doc, 1)["lines"].as_array().unwrap().len(), 2);
}

#[test]
fn a_bodys_lines_take_its_choice_and_an_edges_choice_wins() {
    let (mut doc, names) = drawn();
    set_line(
        &mut doc,
        json!({"body": "F2.b0", "width": "extra_thick", "pattern": "chain"}),
    );
    set_line(
        &mut doc,
        json!({"body": "F4.b0", "width": "thin", "pattern": "phantom"}),
    );
    set_line(
        &mut doc,
        json!({"body": "F2.b0", "edge": names[0], "width": "thick", "pattern": "phantom"}),
    );
    let top = pens_at(&doc, 20.0, 0.0, 40.0);
    assert!(
        !top.is_empty() && top.iter().all(|p| p == "phantom_thick"),
        "{top:?}"
    );
    let bottom = pens_at(&doc, 0.0, 0.0, 40.0);
    assert!(
        !bottom.is_empty() && bottom.iter().all(|p| p == "chain_extra_thick"),
        "{bottom:?}"
    );
    let other = pens_at(&doc, 10.0, 50.0, 70.0);
    assert!(
        !other.is_empty() && other.iter().all(|p| p == "phantom"),
        "{other:?}"
    );
    assert_eq!(view(&doc, 1)["lines"].as_array().unwrap().len(), 3);

    // A width alone keeps the pattern chosen before.
    let answer = set_line(&mut doc, json!({"body": "F4.b0", "width": "thick"}));
    assert_eq!(answer["line"]["pattern"], "phantom", "{answer}");
    let other = pens_at(&doc, 10.0, 50.0, 70.0);
    assert!(other.iter().all(|p| p == "phantom_thick"), "{other:?}");

    // The edge's choice reset: its body's again; one undo step brings it
    // back.
    let reset = set_line(
        &mut doc,
        json!({"body": "F2.b0", "edge": names[0], "reset": true}),
    );
    assert!(reset["line"].is_null(), "{reset}");
    let top = pens_at(&doc, 20.0, 0.0, 40.0);
    assert!(top.iter().all(|p| p == "chain_extra_thick"), "{top:?}");
    command(&mut doc, json!({"cmd": "undo"}));
    let top = pens_at(&doc, 20.0, 0.0, 40.0);
    assert!(top.iter().all(|p| p == "phantom_thick"), "{top:?}");
    // Resetting what has no choice changes nothing.
    let none = set_line(
        &mut doc,
        json!({"body": "F2.b0", "edge": names[1], "reset": true}),
    );
    assert!(none["line"].is_null(), "{none}");
    assert_eq!(view(&doc, 1)["lines"].as_array().unwrap().len(), 3);
}

#[test]
fn a_click_names_the_edge_its_body_and_refusals_say_why() {
    let (mut doc, names) = drawn();
    let place = view(&doc, 1)["place"].clone();
    let (px, py) = (place[0].as_f64().unwrap(), place[1].as_f64().unwrap());
    // A click on the front's bottom edge: that edge.
    let at = json!([px + 20.0, py + 0.3]);
    let answer = set_line(&mut doc, json!({"at": at, "pattern": "dashed"}));
    assert_eq!(answer["line"]["edge"], names[1].as_str(), "{answer}");
    assert_eq!(answer["line"]["body"], "F2.b0", "{answer}");
    // Width left out: a new choice starts wide.
    assert_eq!(answer["line"]["width"], "thick", "{answer}");
    let bottom = pens_at(&doc, 0.0, 0.0, 40.0);
    assert!(bottom.iter().all(|p| p == "dashed_thick"), "{bottom:?}");
    // `of` the body: the body's choice.
    let body = set_line(
        &mut doc,
        json!({"at": at, "of": "body", "width": "thin", "pattern": "continuous"}),
    );
    assert!(body["line"].get("edge").is_none(), "{body}");
    assert_eq!(body["line"]["body"], "F2.b0", "{body}");
    // The drawing pick with `want` edge finds it too.
    let pick = query(
        &doc,
        json!({"query": "drawing_pick", "drawing": "D1", "view": 1, "at": at, "want": "edge"}),
    );
    assert_eq!(pick["pick"]["edge"], names[1].as_str(), "{pick}");

    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_line", "drawing": "D1", "view": 1, "at": at,
               "of": "component", "width": "thin"}),
    );
    assert!(error.contains("in no component of the view"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_line", "drawing": "D1", "view": 1, "at": at,
               "body": "F2.b0", "width": "thin"}),
    );
    assert!(error.contains("not both"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_line", "drawing": "D1", "view": 1, "body": "F2.b0"}),
    );
    assert!(error.contains("width or pattern"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_line", "drawing": "D1", "view": 1, "body": "F9.b0",
               "width": "thin"}),
    );
    assert!(error.contains("does not show the body F9.b0"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_line", "drawing": "D1", "view": 1, "edge": names[0],
               "width": "thin"}),
    );
    assert!(error.contains("names its body"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_line", "drawing": "D1", "view": 1,
               "at": [px - 40.0, py - 40.0], "width": "thin"}),
    );
    assert!(error.contains("no edge of view 1"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_line", "drawing": "D1", "view": 1, "body": "F2.b0",
               "reset": true, "width": "thin"}),
    );
    assert!(error.contains("a reset takes no width"), "{error}");
}

#[test]
fn an_occurrences_choice_ranks_below_its_bodys_and_a_deeper_ones() {
    let parse = |text: &str| serde_json::from_value(json!(text)).unwrap();
    let style = |body: Option<&str>, occurrence: Option<&str>| LineStyle {
        body: body.map(|b| b.parse().unwrap()),
        occurrence: occurrence.map(parse),
        edge: None,
        width: PenWidth::Thin,
        pattern: PenPattern::Phantom,
    };
    let body: BodyUid = "F2.b0".parse().unwrap();
    let path: Vec<OccurrenceUid> = ["O1", "O3"].iter().map(|o| o.parse().unwrap()).collect();
    let own = style(Some("F2.b0"), Some("O1/O3"));
    let outer = style(None, Some("O1"));
    let inner = style(None, Some("O1/O3"));
    let elsewhere = style(None, Some("O2"));
    let plain = style(Some("F2.b0"), None);
    assert_eq!(own.rank(body, &path), Some(0));
    let (a, b) = (
        inner.rank(body, &path).unwrap(),
        outer.rank(body, &path).unwrap(),
    );
    assert!(0 < a && a < b, "{a} {b}");
    assert_eq!(elsewhere.rank(body, &path), None);
    // A body named without an occurrence is the body where it is itself.
    assert_eq!(plain.rank(body, &path), None);
    assert_eq!(plain.rank(body, &[]), Some(0));
    assert!(own.same_target(&own) && !own.same_target(&plain));
    assert_eq!(outer.target_text(), "the occurrence O1");
}

/// The group-code pairs of a DXF file's text.
fn dxf_pairs(text: &str) -> Vec<(i32, String)> {
    let lines: Vec<&str> = text.lines().collect();
    lines
        .chunks(2)
        .filter_map(|pair| {
            let code = pair.first()?.trim().parse().ok()?;
            Some((code, pair.get(1)?.trim().to_owned()))
        })
        .collect()
}

/// The records of a kind (`LAYER`, `LTYPE`): each one's pairs.
fn dxf_records(pairs: &[(i32, String)], kind: &str) -> Vec<Vec<(i32, String)>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < pairs.len() {
        if pairs[i].0 == 0 && pairs[i].1 == kind {
            let end = pairs[i + 1..]
                .iter()
                .position(|p| p.0 == 0)
                .map_or(pairs.len(), |e| i + 1 + e);
            out.push(pairs[i + 1..end].to_vec());
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

fn field(record: &[(i32, String)], code: i32) -> Option<&str> {
    record.iter().find(|p| p.0 == code).map(|p| p.1.as_str())
}

#[test]
fn the_exports_draw_the_chosen_lines_with_their_line_types_and_weights() {
    let (mut doc, names) = drawn();
    set_line(
        &mut doc,
        json!({"body": "F2.b0", "edge": names[0], "width": "thick", "pattern": "phantom"}),
    );
    let dir = mitcad_model::testing::unique_temp_dir("drawing-line-styles");
    let svg = dir.join("lines.svg");
    let pdf = dir.join("lines.pdf");
    let dxf = dir.join("lines.dxf");
    for path in [&svg, &pdf, &dxf] {
        command(
            &mut doc,
            json!({"cmd": "export_drawing", "drawing": "D1", "path": path.to_string_lossy()}),
        );
    }
    // SVG (and so print) and PDF: 0.35 mm wide, long dash double dotted.
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(
        text.contains(r#"stroke-width="0.35" stroke-linecap="round" stroke-linejoin="round" stroke-dasharray="12 1.5 0.5 1.5 0.5 1.5""#),
        "no long dash double dotted wide line in the SVG"
    );
    let bytes = std::fs::read(&pdf).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("0.35 w [12 1.5 0.5 1.5 0.5 1.5] 0 d"),
        "no long dash double dotted wide pen in the PDF"
    );

    // DXF: the edge's lines on PHANTOM_THICK, whose line type is PHANTOM
    // (defined with its dashes) and whose weight is 0.35 mm; the other
    // pens' layers with their own types and weights.
    let text = std::fs::read_to_string(&dxf).unwrap();
    let pairs = dxf_pairs(&text);
    let layers = dxf_records(&pairs, "LAYER");
    let layer = |name: &str| {
        layers
            .iter()
            .find(|l| field(l, 2) == Some(name))
            .unwrap_or_else(|| panic!("no layer {name}"))
            .clone()
    };
    let phantom = layer("PHANTOM_THICK");
    assert_eq!(field(&phantom, 6), Some("PHANTOM"), "{phantom:?}");
    assert_eq!(field(&phantom, 370), Some("35"), "{phantom:?}");
    let visible = layer("VISIBLE");
    assert_eq!(field(&visible, 6), Some("Continuous"), "{visible:?}");
    assert_eq!(field(&visible, 370), Some("35"), "{visible:?}");
    let hidden = layer("HIDDEN");
    assert_eq!(field(&hidden, 6), Some("DASHED"), "{hidden:?}");
    assert_eq!(field(&hidden, 370), Some("18"), "{hidden:?}");
    assert_eq!(field(&layer("EXTRA_THICK"), 370), Some("70"));
    let types = dxf_records(&pairs, "LTYPE");
    let phantom_type = types
        .iter()
        .find(|t| field(t, 2) == Some("PHANTOM"))
        .expect("the PHANTOM line type");
    let dashes: Vec<f64> = phantom_type
        .iter()
        .filter(|p| p.0 == 49)
        .map(|p| p.1.parse().unwrap())
        .collect();
    assert_eq!(
        dashes,
        [12.0, -1.5, 0.5, -1.5, 0.5, -1.5],
        "{phantom_type:?}"
    );
    assert_eq!(field(phantom_type, 73), Some("6"));
    assert!(types.iter().any(|t| field(t, 2) == Some("CHAIN")));
    // The edge's lines are on that layer, none of the others.
    let entities = dxf_records(&pairs, "LINE");
    let on_phantom = entities
        .iter()
        .filter(|e| field(e, 8) == Some("PHANTOM_THICK"))
        .count();
    assert!(on_phantom >= pens_at(&doc, 20.0, 0.0, 40.0).len());
    assert!(on_phantom > 0);
}

#[test]
fn a_picks_mark_carries_the_pen_of_the_lines_it_lies_on() {
    let (mut doc, names) = drawn();
    let place = view(&doc, 1)["place"].clone();
    let (px, py) = (place[0].as_f64().unwrap(), place[1].as_f64().unwrap());
    // What the Dimension tool's click on the front's bottom edge picks, and
    // the pen its mark is drawn as wide as (mitcad#1618).
    let pick = |doc: &Document<MockKernel>, at: [f64; 2]| {
        query(
            doc,
            json!({"query": "drawing_pick", "drawing": "D1", "view": 1, "at": at}),
        )
    };
    let bottom = [px + 20.0, py + 0.3];
    let plain = pick(&doc, bottom);
    assert_eq!(plain["pick"]["edge"], names[1].as_str(), "{plain}");
    let items = plain["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{plain}");
    assert!(items[0].get("line").is_some(), "{plain}");
    assert_eq!(items[0]["pen"], "thick", "{plain}");
    // The edge's lines chosen extra wide: its mark says so; the top edge's
    // stays wide.
    set_line(
        &mut doc,
        json!({"body": "F2.b0", "edge": names[1], "width": "extra_thick"}),
    );
    let wide = pick(&doc, bottom);
    assert_eq!(wide["items"][0]["pen"], "extra_thick", "{wide}");
    let top = pick(&doc, [px + 20.0, py + 19.7]);
    assert_eq!(top["pick"]["edge"], names[0].as_str(), "{top}");
    assert_eq!(top["items"][0]["pen"], "thick", "{top}");
    // A corner's mark is a point of its own size: no pen.
    let corner = pick(&doc, [px + 40.2, py + 0.2]);
    assert_eq!(corner["seen"], "point", "{corner}");
    let items = corner["items"].as_array().unwrap();
    assert!(
        items
            .iter()
            .all(|i| i.get("point").is_some() && i.get("pen").is_none()),
        "{corner}"
    );
}
