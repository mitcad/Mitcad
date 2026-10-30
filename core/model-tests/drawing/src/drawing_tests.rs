// SPDX-License-Identifier: MIT
//! Drawings (mitcad#471): the layout of a sheet from views as the kernel
//! projects them, the writers' files, and the `export_drawing` command on
//! the mock kernel (whose views are the bodies' boxes). The OCCT kernel's
//! views are tested in `geometry/tests/test_drawing.cpp` and the command
//! end to end by the CLI test `cli.export_drawing`.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::internal::drawing::civil_from_days;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_drawing::compose::{HoleInput, SheetInput, SketchDimension, ViewInput, compose};
use mitcad_model_drawing::sheet::{Item, text_width};
use mitcad_model_drawing::{
    DrawingOptions, P2, RoundHole, Units, ViewCurve, ViewEdge, ViewEdgeKind, ViewName,
    ViewProjection, ViewSpec, parse_scale, pdf, scale_text, svg, trim_number,
};

fn line(a: P2, b: P2, visible: bool) -> ViewEdge {
    ViewEdge {
        visible,
        kind: ViewEdgeKind::Sharp,
        curve: ViewCurve::Line(a, b),
    }
}

fn rectangle(w: f64, h: f64) -> Vec<ViewEdge> {
    vec![
        line([0.0, 0.0], [w, 0.0], true),
        line([w, 0.0], [w, h], true),
        line([w, h], [0.0, h], true),
        line([0.0, h], [0.0, 0.0], true),
    ]
}

fn circle(center: P2, radius: f64) -> ViewEdge {
    ViewEdge {
        visible: true,
        kind: ViewEdgeKind::Sharp,
        curve: ViewCurve::Arc {
            center,
            radius,
            start: 0.0,
            end: std::f64::consts::TAU,
        },
    }
}

fn view(name: ViewName, edges: Vec<ViewEdge>) -> ViewInput {
    let (toward_eye, x_axis) = name.axes();
    ViewInput {
        name: Some(name),
        spec: ViewSpec {
            toward_eye,
            x_axis,
            hidden: false,
            smooth: false,
            exact: true,
            quick: false,
            section: None,
            deflection: 0.01,
        },
        projection: ViewProjection {
            edges,
            cuts: Vec::new(),
            polygonal: false,
        },
    }
}

/// A 40 x 30 x 20 plate with four through holes of 6.6 counterbored 11
/// wide and 6.8 deep from the top, and a tapped one, as the kernel shows it.
fn plate() -> SheetInput {
    let corners = [[8.0, 8.0], [32.0, 8.0], [8.0, 22.0], [32.0, 22.0]];
    let mut top = rectangle(40.0, 30.0);
    let mut holes = Vec::new();
    for c in corners {
        top.push(circle(c, 5.5));
        top.push(circle(c, 3.3));
        holes.push(HoleInput {
            hole: RoundHole {
                face: None,
                opening: [c[0], c[1], 20.0],
                direction: [0.0, 0.0, -1.0],
                diameter: 6.6,
                through: true,
                depth: 20.0,
                counterbore: Some((11.0, 6.8)),
                countersink: None,
            },
            thread: None,
        });
    }
    top.push(circle([20.0, 15.0], 2.5));
    holes.push(HoleInput {
        hole: RoundHole {
            face: None,
            opening: [20.0, 15.0, 20.0],
            direction: [0.0, 0.0, -1.0],
            diameter: 5.0,
            through: false,
            depth: 12.0,
            counterbore: None,
            countersink: None,
        },
        thread: Some("M6x1".to_owned()),
    });
    // A hole drilled from below is not seen from above.
    holes.push(HoleInput {
        hole: RoundHole {
            face: None,
            opening: [20.0, 5.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            diameter: 3.0,
            through: false,
            depth: 4.0,
            counterbore: None,
            countersink: None,
        },
        thread: None,
    });
    SheetInput {
        title: "Plate".to_owned(),
        bodies: vec!["Plate".to_owned()],
        date: "2026-10-20".to_owned(),
        views: vec![
            view(ViewName::Front, rectangle(40.0, 20.0)),
            view(ViewName::Top, top),
            view(ViewName::Right, rectangle(30.0, 20.0)),
        ],
        holes,
        sketch_dimensions: Vec::new(),
    }
}

fn texts(group: &mitcad_model_drawing::sheet::Group) -> Vec<String> {
    group
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_plate_has_three_views_their_sizes_hole_callouts_and_a_hole_table() {
    let options = DrawingOptions {
        sheet: mitcad_model_drawing::SheetSize::A3,
        ..DrawingOptions::default()
    };
    let composed = compose(&options, Units::MM, &plate());
    assert!(composed.warnings.is_empty(), "{:?}", composed.warnings);
    assert_eq!(composed.views, 3);
    // 40 mm wide on A3 beside the table: twice its size.
    assert_eq!(scale_text(composed.scale), "2:1");
    let sheet = &composed.sheet;
    assert_eq!((sheet.width, sheet.height), (420.0, 297.0));
    let ids: Vec<_> = sheet
        .groups_of("view")
        .map(|g| g.id.clone().unwrap())
        .collect();
    assert_eq!(ids, ["view-front", "view-top", "view-right"]);
    // Width and height of each view, and one callout per hole size.
    let dims: Vec<Vec<String>> = sheet.groups_of("dimension").map(texts).collect();
    for size in [["40", "20"], ["40", "30"], ["30", "20"]] {
        assert!(
            dims.contains(&vec![size[0].to_owned()]) && dims.contains(&vec![size[1].to_owned()]),
            "{dims:?}"
        );
    }
    assert!(
        dims.contains(&vec![
            "CBORE Ø11 DEPTH 6.8".to_owned(),
            "4x Ø6.6 THRU".to_owned()
        ]),
        "{dims:?}"
    );
    assert!(dims.contains(&vec!["M6x1 12".to_owned()]), "{dims:?}");
    assert_eq!(composed.dimensions, 8);
    // The table: five holes seen from above, by x then y.
    assert_eq!(composed.holes, 5);
    assert_eq!(composed.hole_rows, 5);
    let rows: Vec<Vec<String>> = sheet.groups_of("hole-row").map(texts).collect();
    assert_eq!(
        rows[0],
        ["A1", "8", "8", "Ø6.6 CBORE Ø11 DEPTH 6.8", "THRU", "-"]
    );
    assert_eq!(rows[2], ["A3", "20", "15", "Ø5", "12", "M6x1"]);
    assert_eq!(rows[4][0], "A5");
    let tags: Vec<_> = sheet.groups_of("hole-tag").collect();
    assert_eq!(tags.len(), 5);
    // The title block.
    let title: Vec<String> = sheet.groups_of("title-block").flat_map(texts).collect();
    for text in ["Plate", "2:1", "mm", "2026-10-20", "A3", "Mitcad"] {
        assert!(title.iter().any(|t| t == text), "{text}: {title:?}");
    }

    // The views line up: the top view above the front view, the right
    // view beside it (third angle).
    let center = |id: &str| {
        let g = sheet
            .groups_of("view")
            .find(|g| g.id.as_deref() == Some(id))
            .unwrap();
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for item in &g.items {
            if let Item::Line { a, b, .. } = item {
                for p in [a, b] {
                    for k in 0..2 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
            }
        }
        (
            [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0],
            [hi[0] - lo[0], hi[1] - lo[1]],
        )
    };
    let (front, size) = center("view-front");
    let (top, _) = center("view-top");
    let (right, _) = center("view-right");
    assert!((size[0] - 80.0).abs() < 1e-9 && (size[1] - 40.0).abs() < 1e-9);
    assert!((front[0] - top[0]).abs() < 1e-9 && top[1] > front[1]);
    assert!((front[1] - right[1]).abs() < 1e-9 && right[0] > front[0]);

    // First angle: the top view below, the right view on the left.
    let first = DrawingOptions {
        projection: mitcad_model_drawing::Projection::FirstAngle,
        ..options.clone()
    };
    let composed = compose(&first, Units::MM, &plate());
    let sheet = &composed.sheet;
    let place = |id: &str| {
        let g = sheet
            .groups_of("view")
            .find(|g| g.id.as_deref() == Some(id))
            .unwrap();
        match &g.items[0] {
            Item::Line { a, .. } => *a,
            other => panic!("{other:?}"),
        }
    };
    assert!(place("view-top")[1] < place("view-front")[1]);
    assert!(place("view-right")[0] < place("view-front")[0]);
}

#[test]
fn the_svg_has_groups_dashed_hidden_lines_and_the_texts() {
    let mut input = plate();
    input.views[0]
        .projection
        .edges
        .push(line([16.7, 0.0], [16.7, 13.2], false));
    let options = DrawingOptions {
        hidden_lines: true,
        ..DrawingOptions::default()
    };
    let composed = compose(&options, Units::MM, &input);
    let text = svg::write(&composed.sheet);
    assert!(text.starts_with("<?xml"));
    assert!(text.contains(r#"width="297mm" height="210mm" viewBox="0 0 297 210""#));
    assert_eq!(text.matches(r#"<g class="view""#).count(), 3);
    assert_eq!(text.matches(r#"<g class="hole-row""#).count(), 5);
    assert!(text.contains(">40</text>"));
    assert!(text.contains(">4x Ø6.6 THRU</text>"));
    assert_eq!(text.matches(r#"stroke-dasharray="3 1""#).count(), 1);
    assert!(text.trim_end().ends_with("</svg>"));
}

#[test]
fn the_pdf_has_a_page_per_sheet_and_a_valid_cross_reference_table() {
    let options = DrawingOptions::default();
    let one = compose(&options, Units::MM, &plate()).sheet;
    let bytes = pdf::write(&[one.clone(), one], "Plate (two)");
    // A byte per character, so that offsets are the file's.
    let text: String = bytes
        .iter()
        .map(|&b| if b.is_ascii() { b as char } else { '?' })
        .collect();
    assert!(text.starts_with("%PDF-1.4"));
    assert!(text.trim_end().ends_with("%%EOF"));
    assert_eq!(text.matches("/Type /Page ").count(), 2);
    assert!(text.contains("/Count 2"));
    assert!(text.contains("/Title (Plate \\(two\\))"));
    // Every object starts where the table says, and startxref points at it.
    let start: usize = text
        .rsplit("startxref\n")
        .next()
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(text[start..].starts_with("xref\n0 "));
    let entries: Vec<usize> = text[start..]
        .lines()
        .skip(3)
        .take_while(|l| l.ends_with(" n "))
        .map(|l| l[..10].parse().unwrap())
        .collect();
    assert_eq!(entries.len(), 8);
    for (i, offset) in entries.iter().enumerate() {
        assert!(
            text[*offset..].starts_with(&format!("{} 0 obj", i + 1)),
            "object {}",
            i + 1
        );
    }
    // Each stream is as long as its /Length says.
    let mut streams = 0;
    for (at, _) in text.match_indices("/Length ") {
        let rest = &text[at + 8..];
        let length: usize = rest[..rest.find(' ').unwrap()].parse().unwrap();
        let start = at + 8 + rest.find("stream\n").unwrap() + 7;
        assert_eq!(&bytes[start + length..start + length + 10], b"\nendstream");
        streams += 1;
    }
    assert_eq!(streams, 2);
    // "Ø" in WinAnsi.
    assert!(bytes.windows(5).any(|w| w == b"(4x \xd8"));
}

#[test]
fn the_dxf_has_a_layer_per_line_kind_and_the_texts() {
    let composed = compose(&DrawingOptions::default(), Units::MM, &plate());
    let drawing = mitcad_model_drawing::dxf::drawing(&composed.sheet);
    for layer in ["VISIBLE", "HIDDEN", "THIN", "HATCH", "FRAME", "TEXT"] {
        assert!(drawing.layer(layer).is_some(), "{layer}");
    }
    let text = mitcad_dxf::write_string(&drawing, &mitcad_dxf::WriteOptions::default()).unwrap();
    let back = mitcad_dxf::read_str(&text).unwrap();
    assert_eq!(back.units, mitcad_dxf::Units::Millimeters);
    assert!(back.entities.iter().any(|e| matches!(&e.geometry,
        mitcad_dxf::Geometry::Text(t) if t.content == "40")));
    assert!(
        back.entities
            .iter()
            .any(|e| e.layer == "VISIBLE"
                && matches!(e.geometry, mitcad_dxf::Geometry::Circle { .. }))
    );
}

#[test]
fn a_section_view_is_hatched_and_its_plane_drawn_in_the_front_view() {
    let mut input = plate();
    input.views.truncate(2); // front, top
    let plane =
        mitcad_model_drawing::compose::cut_plane([20.0, 15.0, 0.0], [0.0, -1.0, 0.0]).unwrap();
    let mut spec = mitcad_model_drawing::compose::section_spec(&plane, false, false, true);
    assert_eq!(spec.toward_eye, [0.0, -1.0, 0.0]);
    assert_eq!(spec.x_axis, [1.0, 0.0, 0.0]);
    spec.deflection = 0.01;
    input.views.push(ViewInput {
        name: None,
        spec,
        projection: ViewProjection {
            edges: rectangle(40.0, 20.0),
            cuts: vec![vec![vec![
                [0.0, 0.0],
                [16.7, 0.0],
                [16.7, 20.0],
                [0.0, 20.0],
            ]]],
            polygonal: false,
        },
    });
    let composed = compose(&DrawingOptions::default(), Units::MM, &input);
    assert_eq!(composed.views, 3);
    let section = composed
        .sheet
        .groups_of("view")
        .find(|g| g.id.as_deref() == Some("view-section"))
        .unwrap();
    let hatches = section
        .items
        .iter()
        .filter(|i| {
            matches!(
                i,
                Item::Line {
                    pen: mitcad_model_drawing::sheet::Pen::Hatch,
                    ..
                }
            )
        })
        .count();
    assert!(hatches > 3, "{hatches}");
    // The top view sees the plane edge on.
    assert_eq!(composed.sheet.groups_of("section-line").count(), 1);
    let label: Vec<String> = composed
        .sheet
        .groups_of("view-label")
        .flat_map(texts)
        .collect();
    assert_eq!(label, ["SECTION A-A"]);
}

#[test]
fn sketch_dimensions_go_to_the_view_along_their_normal() {
    let mut input = plate();
    input.sketch_dimensions = vec![
        SketchDimension::Linear {
            a: [0.0, 0.0, 20.0],
            b: [40.0, 0.0, 20.0],
            along: Some([1.0, 0.0, 0.0]),
            text: Some([20.0, -5.0, 20.0]),
            value: 40.0,
            normal: [0.0, 0.0, 1.0],
        },
        SketchDimension::Radial {
            center: [20.0, 15.0, 20.0],
            radius: 2.5,
            diameter: true,
            text: None,
            value: 5.0,
            normal: [0.0, 0.0, -1.0],
        },
        // No orthographic view looks along this one.
        SketchDimension::Radial {
            center: [0.0; 3],
            radius: 1.0,
            diameter: false,
            text: None,
            value: 1.0,
            normal: [0.0, 0.6, 0.8],
        },
    ];
    let options = DrawingOptions {
        sketch_dimensions: true,
        ..DrawingOptions::default()
    };
    let composed = compose(&options, Units::MM, &input);
    let dims: Vec<Vec<String>> = composed
        .sheet
        .groups_of("sketch-dimension")
        .map(texts)
        .collect();
    assert_eq!(dims, [vec!["40".to_owned()], vec!["Ø5".to_owned()]]);
}

#[test]
fn numbers_scales_units_and_dates() {
    assert_eq!(trim_number(40.0, 2), "40");
    assert_eq!(trim_number(6.6, 2), "6.6");
    assert_eq!(trim_number(-0.0001, 2), "0");
    assert_eq!(parse_scale("1:2"), Some(0.5));
    assert_eq!(parse_scale("2:1"), Some(2.0));
    assert_eq!(parse_scale("1:0"), None);
    assert_eq!(scale_text(0.4), "1:2.5");
    assert_eq!(scale_text(5.0), "5:1");
    let inch = Units::by_symbol("in").unwrap();
    assert_eq!(inch.length(25.4), "1");
    assert_eq!(inch.length(10.0), "0.394");
    assert!(Units::by_symbol("parsec").is_none());
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(20_746), (2026, 10, 20));
    assert!(text_width("40", 3.5) > text_width("4", 3.5));
    let options: DrawingOptions =
        serde_json::from_value(json!({"scale": "1:5", "views": ["front", "iso"]})).unwrap();
    assert_eq!(options.scale, mitcad_model_drawing::ScaleChoice::Fixed(0.2));
    assert!(serde_json::from_value::<DrawingOptions>(json!({"scale": "big"})).is_err());
    assert!(serde_json::from_value::<DrawingOptions>(json!({"sheets": "A3"})).is_err());
    let defaults = DrawingOptions::default();
    assert_eq!(defaults.sheet, mitcad_model_drawing::SheetSize::A4);
    assert_eq!(
        defaults.views,
        [ViewName::Front, ViewName::Top, ViewName::Right]
    );
}

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn run(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|r| serde_json::from_str(&r).unwrap())
        .map_err(|e| e.to_string())
}

/// Two blocks: 40 x 30 x 20 at the origin, 20 x 10 x 10 beside it.
fn two_blocks() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    run(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [50, 0], "width": 20, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
        ]),
    )
    .unwrap();
    doc
}

#[test]
fn export_drawing_writes_the_shown_bodies_on_one_sheet_or_one_each() {
    let mut doc = two_blocks();
    let dir = std::env::temp_dir().join(format!("mitcad-drawing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let svg_path = dir.join("blocks.svg").to_string_lossy().into_owned();
    let result = run(
        &mut doc,
        json!({"cmd": "export_drawing", "path": svg_path, "sheet": "A3", "title": "Blocks",
               "date": "2026-10-20"}),
    )
    .unwrap();
    assert_eq!(result["format"], "svg", "{result}");
    assert_eq!(result["views"], 3);
    assert_eq!(result["bodies"], json!(["F2.b0", "F4.b0"]));
    assert_eq!(result["sheet"], "A3");
    assert_eq!(result["scale"], "2:1");
    assert_eq!(result["sheets"], 1);
    assert_eq!(result["units"], "mm");
    let text = std::fs::read_to_string(&svg_path).unwrap();
    assert_eq!(text.matches(r#"<g class="view""#).count(), 3);
    // Both blocks: 70 wide, 30 deep, 20 high.
    for size in [">70<", ">30<", ">20<", ">Blocks<", ">Body1, Body2<"] {
        assert!(text.contains(size), "{size}");
    }

    // One sheet per body: a PDF of two pages; SVG files numbered.
    let pdf_path = dir.join("blocks.pdf").to_string_lossy().into_owned();
    let result = run(
        &mut doc,
        json!({"cmd": "export_drawing", "path": pdf_path, "per_body": true,
               "views": ["front", "top", "right", "iso"]}),
    )
    .unwrap();
    assert_eq!(result["pages"], 2, "{result}");
    assert_eq!(result["views"], 4);
    let bytes = std::fs::read(&pdf_path).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&bytes)
            .matches("/Type /Page ")
            .count(),
        2
    );
    let result = run(
        &mut doc,
        json!({"cmd": "export_drawing", "path": svg_path, "per_body": true, "bodies": ["Body2"]}),
    )
    .unwrap();
    assert_eq!(result["files"], json!([svg_path]), "{result}");
    let result = run(
        &mut doc,
        json!({"cmd": "export_drawing", "path": svg_path, "per_body": true}),
    )
    .unwrap();
    let second = dir.join("blocks-2.svg").to_string_lossy().into_owned();
    assert_eq!(result["files"], json!([svg_path, second]));
    assert!(std::path::Path::new(&second).exists());

    // Hidden bodies are left out; refusals.
    run(
        &mut doc,
        json!({"cmd": "set_body_visible", "uid": "F4.b0", "visible": false}),
    )
    .unwrap();
    let result = run(&mut doc, json!({"cmd": "export_drawing", "path": svg_path})).unwrap();
    assert_eq!(result["bodies"], json!(["F2.b0"]));
    for (command, message) in [
        (
            json!({"cmd": "export_drawing", "path": dir.join("x.png").to_string_lossy()}),
            "drawings are written to .pdf, .svg, .dxf or .dwg files",
        ),
        (
            json!({"cmd": "export_drawing", "path": svg_path, "units": "furlong"}),
            "unknown length unit 'furlong'",
        ),
        (
            json!({"cmd": "export_drawing", "path": svg_path, "views": []}),
            "a drawing needs at least one view",
        ),
        (
            json!({"cmd": "export_drawing", "path": svg_path, "section": {"analysis": "Nope"}}),
            "there is no analysis 'Nope'",
        ),
        (
            json!({"cmd": "export_drawing", "path": svg_path, "bodies": ["Nobody"]}),
            "no body at the timeline marker is named 'Nobody'",
        ),
    ] {
        let error = run(&mut doc, command).unwrap_err();
        assert!(error.contains(message), "{error}");
    }
    std::fs::remove_dir_all(&dir).ok();
}
