// SPDX-License-Identifier: MIT
//! Picks against what a view shows (mitcad#717) on the mock kernel: a
//! section sees what lies behind its plane and picks the faces it cuts; a
//! detail and a broken view see the parts of edges they show; edges seen
//! on one another are told apart by the eye; arcs are where they are; a
//! pictorial view measures in 3D; a round face ends at its own edges and a
//! half hole gets its centre mark; datum letters are one set with the
//! model's; a component's copied drawing maps its picks; a view retrieves
//! the sketch dimensions of its own bodies, once.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry};
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
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

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn point(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
}

fn sheet(doc: &Document<MockKernel>, drawing: &str) -> Value {
    query(doc, json!({"query": "drawing", "drawing": drawing}))["sheets"][0].clone()
}

fn view(doc: &Document<MockKernel>, id: u64) -> Value {
    sheet(doc, "D1")["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no view {id}"))
}

fn annotation(doc: &Document<MockKernel>, drawing: &str, id: u64) -> Value {
    sheet(doc, drawing)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id}"))
}

/// The groups of a class on the first sheet of D1.
fn groups(doc: &Document<MockKernel>, class: &str) -> Vec<Value> {
    sheet(doc, "D1")["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["class"] == class)
        .cloned()
        .collect()
}

fn pick_at(doc: &Document<MockKernel>, view: u64, at: [f64; 2], want: &str) -> Value {
    query(
        doc,
        json!({"query": "drawing_pick", "drawing": "D1", "view": view, "at": at, "want": want}),
    )
}

fn line(start: [f64; 3], end: [f64; 3]) -> CurveGeometry {
    CurveGeometry::Line { start, end }
}

fn edge(name: &str) -> Value {
    json!({"body": "F2.b0", "edge": name})
}

/// A 40 x 30 x 20 block at the origin (F2.b0) and a 20 x 10 x 10 block at
/// x 50 (F4.b0).
fn blocks() -> Document<MockKernel> {
    blocks_with(false)
}

/// The blocks, the kernel measuring selections (the model's annotations)
/// when `measured`.
fn blocks_with(measured: bool) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(measured);
    command(
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
    );
    doc.kernel().lists_geometry.set(true);
    doc.kernel().cuts_sections.set(true);
    doc
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

/// The blocks with seven edges of the first given curves: 0 the front's
/// left upright side, 1 its right one, 2 the back's bottom edge (listed
/// before the front's), 3 the front's bottom edge, 4 its top edge, 5 a
/// whole circle of radius 5 on the top, 6 an arc of radius 8 about it (its
/// half towards +y); a drawing with a front view at 1:1 (the block's front
/// 65..105 x 110..130 on the sheet) and the top view above it.
fn picked() -> (Document<MockKernel>, Vec<String>) {
    picked_with(false)
}

fn picked_with(measured: bool) -> (Document<MockKernel>, Vec<String>) {
    let mut doc = blocks_with(measured);
    let names = edge_names(&doc);
    assert!(names.len() >= 7, "{names:?}");
    let curves = [
        line([0.0, 0.0, 0.0], [0.0, 0.0, 20.0]),
        line([40.0, 0.0, 0.0], [40.0, 0.0, 20.0]),
        line([0.0, 30.0, 0.0], [40.0, 30.0, 0.0]),
        line([0.0, 0.0, 0.0], [40.0, 0.0, 0.0]),
        line([0.0, 0.0, 20.0], [40.0, 0.0, 20.0]),
        CurveGeometry::Circle {
            center: [20.0, 15.0, 20.0],
            normal: [0.0, 0.0, 1.0],
            radius: 5.0,
            start: [25.0, 15.0, 20.0],
            end: [25.0, 15.0, 20.0],
        },
        CurveGeometry::Circle {
            center: [20.0, 15.0, 20.0],
            normal: [0.0, 0.0, 1.0],
            radius: 8.0,
            start: [28.0, 15.0, 20.0],
            end: [12.0, 15.0, 20.0],
        },
    ];
    {
        let mut edge_curves = doc.kernel().edge_curves.borrow_mut();
        for (name, curve) in names.iter().zip(curves) {
            edge_curves.insert(name.clone(), curve);
        }
    }
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": 1,
               "side": "above"}),
    );
    (doc, names)
}

/// Where a view point of the top view (view 2: the model's x and y) is on
/// the sheet.
fn on_top(doc: &Document<MockKernel>, x: f64, y: f64) -> [f64; 2] {
    let place = point(&view(doc, 2)["place"]);
    [place[0] + x, place[1] + y]
}

#[test]
fn a_section_sees_what_lies_behind_its_plane_and_picks_the_faces_it_cuts() {
    let (mut doc, names) = picked();
    let top = format!("F2:end({R})");
    let bottom = format!("F2:start({R})");
    {
        let mut surfaces = doc.kernel().surfaces.borrow_mut();
        surfaces.insert(
            top.clone(),
            SurfaceGeometry::Plane {
                origin: [0.0, 0.0, 20.0],
                normal: [0.0, 0.0, 1.0],
            },
        );
        surfaces.insert(
            bottom.clone(),
            SurfaceGeometry::Plane {
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, -1.0],
            },
        );
    }
    // A section at x 20 of the front view: what lies at x < 20 goes, the
    // view looks along +x (its x the model's -y, its y the model's z).
    let section = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
               "line": [[85, 105], [85, 135]], "at": [300, 120]}),
    )["view"]
        .as_u64()
        .unwrap();
    // The front's left side lies in front of the plane: not seen.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": section,
               "type": "linear", "picks": [edge(&names[0]), edge(&names[1])]}),
    );
    assert!(error.contains("in front of the section's plane"), "{error}");
    // The cut face's top edge is the top face's cut: picked as that face.
    let shown = view(&doc, section);
    let [lo, hi] = [point(&shown["box"][0]), point(&shown["box"][1])];
    assert!(
        near(hi[0] - lo[0], 30.0) && near(hi[1] - lo[1], 20.0),
        "{shown}"
    );
    let picked = pick_at(&doc, section, [lo[0] + 15.0, hi[1] - 0.2], "line");
    assert_eq!(picked["pick"]["face"], top.as_str(), "{picked}");
    assert_eq!(picked["seen"], "line");
    // A dimension between the top's and the bottom's cuts: 20 high.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": section,
               "type": "linear", "along": "vertical",
               "picks": [{"body": "F2.b0", "face": top}, {"body": "F2.b0", "face": bottom}]}),
    );
    let dimension = annotation(&doc, "D1", added["annotation"].as_u64().unwrap());
    assert_eq!(dimension["broken"], false, "{dimension}");
    assert!(
        near(dimension["value"].as_f64().unwrap(), 20.0),
        "{dimension}"
    );
    // Off a section a planar face is no pick.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "diameter",
               "picks": [{"body": "F2.b0", "face": top}]}),
    );
    assert!(error.contains("not round"), "{error}");
}

#[test]
fn breaks_and_details_see_the_parts_of_edges_they_show() {
    let (mut doc, names) = picked();
    // A detail of the top view's front edge near its right end: the edge's
    // middle lies outside the circle.
    let c = on_top(&doc, 38.0, 2.0);
    let detail = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "detail", "parent": 2,
               "center": c, "radius": 4, "at": [330, 60]}),
    )["view"]
        .as_u64()
        .unwrap();
    // The front's top and bottom edges lie on one another seen from above:
    // the top one, nearer the eye.
    let picked = pick_at(&doc, detail, [330.0, 56.0], "line");
    assert_eq!(picked["pick"]["edge"], names[4].as_str(), "{picked}");
    // A break takes view x 10..30 out of the front view: the bottom edges'
    // middles lie in it, their ends beside it.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "add_break": {"direction": "horizontal", "from": [75, 120], "to": [95, 120]}}),
    );
    // The front's and the back's bottom edges lie on one another seen from
    // the front, the back's listed first: the front's, nearer the eye.
    let picked = pick_at(&doc, 1, [70.0, 110.3], "line");
    assert_eq!(picked["pick"]["edge"], names[3].as_str(), "{picked}");
    // In the gap the band closed to (sheet x 75 to 81) nothing is seen.
    let nothing = pick_at(&doc, 1, [78.0, 110.3], "line");
    assert!(nothing["pick"].is_null(), "{nothing}");
}

#[test]
fn arcs_are_picked_where_they_are_and_whole_circles_have_no_ends() {
    let (doc, names) = picked();
    // On the arc's missing half: nothing.
    let missing = pick_at(&doc, 2, on_top(&doc, 20.0, 6.0), "any");
    assert!(missing["pick"].is_null(), "{missing}");
    // On the arc: it, drawn as an arc.
    let on = pick_at(&doc, 2, on_top(&doc, 20.0, 23.5), "any");
    assert_eq!(on["pick"]["edge"], names[6].as_str(), "{on}");
    assert!(on["items"][0].get("arc").is_some(), "{on}");
    // A point on the whole circle's rim where it starts: its centre, as it
    // has no ends.
    let rim = pick_at(&doc, 2, on_top(&doc, 25.0, 15.5), "point");
    assert_eq!(rim["pick"]["edge"], names[5].as_str(), "{rim}");
    assert_eq!(rim["pick"]["point"], "center", "{rim}");
    // The arc's ends are picked.
    let end = pick_at(&doc, 2, on_top(&doc, 12.2, 15.0), "point");
    assert_eq!(end["pick"]["edge"], names[6].as_str(), "{end}");
    assert_eq!(end["pick"]["point"], "end", "{end}");
}

#[test]
fn a_pictorial_view_measures_in_three_dimensions() {
    let (mut doc, names) = picked();
    let iso = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "iso",
               "at": [300, 220], "scale": "1:1"}),
    )["view"]
        .as_u64()
        .unwrap();
    let ends = json!([
        {"body": "F2.b0", "edge": names[0], "point": "start"},
        {"body": "F2.b0", "edge": names[0], "point": "end"},
    ]);
    // Along the view's x: foreshortened, refused; an angle too.
    for (kind, along) in [
        ("linear", json!("horizontal")),
        ("ordinate", json!("vertical")),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": iso,
                   "type": kind, "along": along, "picks": ends}),
        );
        assert!(error.contains("foreshortened"), "{error}");
    }
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": iso, "type": "angle",
               "picks": [edge(&names[0]), edge(&names[3])]}),
    );
    assert!(error.contains("foreshortened"), "{error}");
    // Aligned (the default there): the upright side's true length.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": iso, "type": "linear",
               "picks": ends}),
    );
    let aligned = annotation(&doc, "D1", added["annotation"].as_u64().unwrap());
    assert_eq!(aligned["along"], "aligned");
    assert!(near(aligned["value"].as_f64().unwrap(), 20.0), "{aligned}");
    assert_eq!(aligned["label"], "20");
    // Turned horizontal: refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1",
               "annotation": added["annotation"], "along": "horizontal"}),
    );
    assert!(error.contains("foreshortened"), "{error}");

    // An angle's arc keeps a radius however it is given.
    let angle = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "angle",
               "picks": [edge(&names[0]), edge(&names[3])]}),
    )["annotation"]
        .clone();
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": angle,
               "line_offset": -5}),
    );
    let kept = annotation(&doc, "D1", angle.as_u64().unwrap());
    assert!(near(kept["line_offset"].as_f64().unwrap(), 2.0), "{kept}");
    assert!(
        near(kept["value"].as_f64().unwrap(), std::f64::consts::FRAC_PI_2),
        "{kept}"
    );
}

#[test]
fn only_a_view_along_a_given_direction_is_square_to_its_own_bodies_faces() {
    // mitcad#722: a view square to a planar face counts as orthographic
    // only along a given direction and only for the body with that face;
    // an aligned dimension keeps measuring its picks' true distance.
    let (mut doc, names) = picked();
    let (s2, s3) = (std::f64::consts::FRAC_1_SQRT_2, 1.0 / 3f64.sqrt());
    // The first block's bottom turned to face the isometric preset's eye,
    // its top to face a given direction.
    {
        let mut surfaces = doc.kernel().surfaces.borrow_mut();
        surfaces.insert(
            format!("F2:start({R})"),
            SurfaceGeometry::Plane {
                origin: [0.0, 0.0, 0.0],
                normal: [-s3, s3, -s3],
            },
        );
        surfaces.insert(
            format!("F2:end({R})"),
            SurfaceGeometry::Plane {
                origin: [0.0, 0.0, 20.0],
                normal: [0.0, -s2, s2],
            },
        );
    }
    let iso = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "iso",
               "at": [300, 220], "scale": "1:1"}),
    )["view"]
        .as_u64()
        .unwrap();
    let ends = |edge: &str| {
        json!([
            {"body": "F2.b0", "edge": edge, "point": "start"},
            {"body": "F2.b0", "edge": edge, "point": "end"},
        ])
    };
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": iso,
               "type": "linear", "along": "horizontal", "picks": ends(&names[3])}),
    );
    assert!(error.contains("foreshortened"), "{error}");

    // The view along the given direction is square to the top.
    let given = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base",
               "direction": {"toward_eye": [0.0, -s2, s2], "x_axis": [1, 0, 0]},
               "at": [300, 90], "scale": "1:1"}),
    )["view"]
        .as_u64()
        .unwrap();
    // Along the view's x on that block: its bottom edge's length.
    let across = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": given,
               "type": "linear", "along": "horizontal", "picks": ends(&names[3])}),
    )["annotation"]
        .as_u64()
        .unwrap();
    let across = annotation(&doc, "D1", across);
    assert!(near(across["value"].as_f64().unwrap(), 40.0), "{across}");
    // Aligned on its upright side, which leaves the face's plane: the true
    // length (20), not the projected one (20 / sqrt 2), as before.
    let aligned = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": given,
               "type": "linear", "along": "aligned", "picks": ends(&names[0])}),
    )["annotation"]
        .as_u64()
        .unwrap();
    let aligned = annotation(&doc, "D1", aligned);
    assert!(near(aligned["value"].as_f64().unwrap(), 20.0), "{aligned}");
    // The other block has no face square to the view: still foreshortened.
    let other: BodyUid = "F4.b0".parse().unwrap();
    let other_edge = doc
        .kernel()
        .edges(doc.body_shape(other).unwrap())
        .unwrap()
        .into_iter()
        .find_map(|e| e.name)
        .unwrap();
    doc.kernel()
        .edge_curves
        .borrow_mut()
        .insert(other_edge.clone(), line([50.0, 0.0, 0.0], [70.0, 0.0, 0.0]));
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": given,
               "type": "linear", "along": "horizontal",
               "picks": [{"body": "F4.b0", "edge": other_edge, "point": "start"},
                         {"body": "F4.b0", "edge": other_edge, "point": "end"}]}),
    );
    assert!(error.contains("foreshortened"), "{error}");
}

/// The blocks with the first block's side `side(c1…)` a round face of
/// radius 5 about the axis x 20, y 15 (a hole), its own edges on the top
/// and the bottom half circles towards +y; with `other`, a whole circle of
/// that radius on the same axis 50 high on another edge. A drawing with a
/// front view at 1:1 (as `picked`'s) and a top view at [100, 220].
fn holed(other: bool) -> Document<MockKernel> {
    let mut doc = blocks();
    let side = "F2:side(c1[c4,c2])".to_owned();
    let names = edge_names(&doc);
    let find = |with: &str| {
        names
            .iter()
            .find(|n| n.contains(&side) && n.contains(with))
            .cloned()
            .unwrap_or_else(|| panic!("no edge of {side} and {with}: {names:?}"))
    };
    let (top, bottom) = (find(":end("), find(":start("));
    let elsewhere = names
        .iter()
        .find(|n| !n.contains("side(c1["))
        .cloned()
        .expect("an edge away from the side");
    let half = |z: f64| CurveGeometry::Circle {
        center: [20.0, 15.0, z],
        normal: [0.0, 0.0, 1.0],
        radius: 5.0,
        start: [25.0, 15.0, z],
        end: [15.0, 15.0, z],
    };
    {
        let mut curves = doc.kernel().edge_curves.borrow_mut();
        curves.insert(top, half(20.0));
        curves.insert(bottom, half(0.0));
        if other {
            curves.insert(
                elsewhere,
                CurveGeometry::Circle {
                    center: [20.0, 15.0, 50.0],
                    normal: [0.0, 0.0, 1.0],
                    radius: 5.0,
                    start: [25.0, 15.0, 50.0],
                    end: [25.0, 15.0, 50.0],
                },
            );
        }
    }
    doc.kernel().surfaces.borrow_mut().insert(
        side,
        SurfaceGeometry::Cylinder {
            origin: [20.0, 15.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 5.0,
        },
    );
    // The top's half circle is drawn (mitcad#1286); the hole's outlines
    // are not: the block hides them from the front.
    doc.kernel().drawn_curves.borrow_mut().push(
        (0..=16)
            .map(|i| {
                let a = std::f64::consts::PI * f64::from(i) / 16.0;
                [20.0 + 5.0 * a.cos(), 15.0 + 5.0 * a.sin(), 20.0]
            })
            .collect(),
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [100, 220], "scale": "1:1"}),
    );
    doc
}

/// The sheet y of the ends of the centre marks' lines drawn upright at x,
/// of those that start between `low` and `high`.
fn upright_marks(doc: &Document<MockKernel>, x: f64, low: f64, high: f64) -> Vec<f64> {
    groups(doc, "center-mark")
        .iter()
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter_map(|i| {
            let line = i.get("line")?;
            let (a, b) = (point(&line[0]), point(&line[1]));
            (near(a[0], x) && near(b[0], x) && a[1] >= low && a[1] <= high)
                .then(|| vec![a[1], b[1]])
        })
        .flatten()
        .collect()
}

#[test]
fn a_round_face_ends_at_its_own_edges_and_a_half_hole_is_marked() {
    // Half circles only: the top view marks their centre; the front view,
    // which the block hides the hole from, draws no centreline without
    // hidden lines (mitcad#1286; with them below).
    let doc = holed(false);
    let place = point(&view(&doc, 2)["place"]);
    let marks = groups(&doc, "center-mark");
    assert_eq!(marks.len(), 1, "{marks:?}");
    let (x, y) = (place[0] + 20.0, place[1] + 15.0);
    assert!(
        !upright_marks(&doc, x, y - 1.0, y + 1.0).is_empty(),
        "no center mark on the half hole in the top view: {marks:?}"
    );
    // With a circle of its radius on its axis elsewhere: the face still
    // reaches from z 0 to 20, not 50.
    let doc = holed(true);
    let picked = pick_at(&doc, 1, [90.0, 120.0], "circle");
    assert_eq!(picked["pick"]["face"], "F2:side(c1[c4,c2])", "{picked}");
    assert_eq!(picked["seen"], "axis");
    let side = &picked["items"][0]["line"];
    let ys = [point(&side[0])[1], point(&side[1])[1]];
    assert!(
        near(ys[0].min(ys[1]), 110.0) && near(ys[0].max(ys[1]), 130.0),
        "{picked}"
    );
    // The block hides the hole from the front: no centreline there
    // (mitcad#1286), until the view shows hidden lines.
    assert!(
        upright_marks(&doc, 85.0, 100.0, 140.0).is_empty(),
        "a centerline of the hidden hole in the front view"
    );
    let mut doc = doc;
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "style": {"hidden_lines": true}}),
    );
    // The front view's centreline, drawn from its middle out.
    let centreline = upright_marks(&doc, 85.0, 100.0, 140.0);
    assert!(!centreline.is_empty(), "no centerline in the front view");
    assert!(
        centreline.iter().all(|y| *y <= 133.0 && *y >= 107.0),
        "{centreline:?}"
    );
}

#[test]
fn a_snap_near_no_point_goes_onto_a_centre_line() {
    // A section's line through a hole (mitcad#1362): the hole seen from
    // the side in the front view, its axis at x 85 from y 110 to 130.
    let doc = holed(false);
    let at = |answer: &Value| point(&answer["at"]);
    let axis = pick_at(&doc, 1, [86.0, 121.0], "snap");
    assert_eq!(axis["pick"]["face"], "F2:side(c1[c4,c2])", "{axis}");
    assert_eq!(axis["center_line"], true, "{axis}");
    assert!(
        near(at(&axis)[0], 85.0) && near(at(&axis)[1], 121.0),
        "{axis}"
    );
    // A point within reach wins: the top half circle's centre.
    let centre = pick_at(&doc, 1, [85.5, 111.0], "snap");
    assert_eq!(centre["pick"]["point"], "center", "{centre}");
    assert!(centre.get("center_line").is_none(), "{centre}");
    assert!(
        near(at(&centre)[0], 85.0) && near(at(&centre)[1], 110.0),
        "{centre}"
    );
    // The top view's centre mark on the half hole: 6 below its centre,
    // beyond the reach of its points, onto the mark's line.
    let place = point(&view(&doc, 2)["place"]);
    let (x, y) = (place[0] + 20.0, place[1] + 15.0);
    let mark = pick_at(&doc, 2, [x + 0.6, y - 6.0], "snap");
    assert_eq!(mark["center_line"], true, "{mark}");
    assert_eq!(mark["pick"]["point"], "center", "{mark}");
    assert!(
        near(at(&mark)[0], x) && near(at(&mark)[1], y - 6.0),
        "{mark}"
    );
    // Beyond the reach of every centre line too: nothing.
    let none = pick_at(&doc, 1, [100.0, 121.0], "snap");
    assert!(none["pick"].is_null(), "{none}");
}

#[test]
fn datum_letters_are_one_set_with_the_models() {
    let (mut doc, names) = picked_with(true);
    let top = json!({"body": "F2.b0", "face": format!("F2:end({R})")});
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "a": top}),
    );
    // The model has A: the drawing's first label takes B.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "datum",
               "picks": [edge(&names[0])]}),
    );
    let label = annotation(&doc, "D1", added["annotation"].as_u64().unwrap());
    assert_eq!(label["letter"], "B");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "datum",
               "picks": [edge(&names[1])], "letter": "A"}),
    );
    assert!(error.contains("datum A exists"), "{error}");
    // The model's next is C; B is the drawing's.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "a": top, "letter": "B"}),
    );
    assert!(error.contains("a datum label of Drawing1"), "{error}");
    let next = command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "datum", "a": top}),
    );
    assert_eq!(next["name"], "DatumC");
    // What a frame can refer to: every label's letter.
    let drawing = query(&doc, json!({"query": "drawing", "drawing": "D1"}));
    assert_eq!(drawing["datum_letters"], json!(["A", "B", "C"]));
}

#[test]
fn a_components_copied_drawing_maps_its_picks() {
    let mut doc = Document::new(MockKernel::default());
    let made = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bracket"}),
    );
    let occurrence = made["occurrence"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
        ]),
    );
    let names = edge_names(&doc);
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "component": "Bracket"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [150, 120], "scale": "1:1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
               "along": "horizontal", "picks": [edge(&names[0]), edge(&names[1])]}),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "paste_new", "occurrence": occurrence}),
    );
    let copied = query(&doc, json!({"query": "drawings"}))
        .as_array()
        .unwrap()
        .last()
        .cloned()
        .unwrap();
    let uid = copied["drawing"].as_str().unwrap().to_owned();
    assert_ne!(uid, "D1");
    let copy = annotation(&doc, &uid, 1);
    let body = copy["picks"][0]["body"].as_str().unwrap().to_owned();
    assert_ne!(body, "F2.b0", "{copy}");
    let feature = body.split('.').next().unwrap().to_owned();
    for pick in copy["picks"].as_array().unwrap() {
        let name = pick["edge"].as_str().unwrap();
        assert!(
            name.contains(&format!("{feature}:")) && !name.contains("F2:"),
            "{copy}"
        );
    }
    assert_eq!(copy["broken"], false, "{copy}");
}

#[test]
fn a_view_retrieves_the_sketch_dimensions_of_its_own_bodies_once() {
    let mut doc = blocks();
    for sketch in ["F1", "F3"] {
        command(
            &mut doc,
            json!({"cmd": "sketch.add_dimension", "sketch": sketch,
                   "dimension": {"type": "length", "line": "c4"}, "driven": true}),
        );
    }
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "size": "A3",
               "annotations": {"sketch_dimensions": true}}),
    );
    // A top view of the first block only: the sheet draws both sketches'
    // dimensions (the two sketches have as many).
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [150, 150], "scale": "1:1",
               "subject": {"type": "bodies", "bodies": [{"body": "F2.b0"}]}}),
    );
    assert!(groups(&doc, "retrieved-dimension").is_empty());
    let all = groups(&doc, "sketch-dimension").len();
    assert!(all >= 2 && all.is_multiple_of(2), "{all}");
    // Retrieving its own sketch's: those once, as the view retrieves them;
    // the other sketch's as the sheet draws them.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "retrieved_dimensions": true}),
    );
    let retrieved = groups(&doc, "retrieved-dimension").len();
    let sheet = groups(&doc, "sketch-dimension").len();
    assert_eq!(retrieved, all / 2, "{retrieved} of {all}");
    assert_eq!(sheet, all / 2, "{sheet} of {all}");
}

#[test]
fn a_sections_face_pick_spans_its_own_bodys_cut_only() {
    // mitcad#718: both blocks' bottoms lie at z 0, so their cuts lie on one
    // line; a pick of the first block's bottom is that block's cut, x 0..40,
    // not 0..70.
    let (mut doc, _) = picked();
    let bottom = format!("F2:start({R})");
    doc.kernel().surfaces.borrow_mut().insert(
        bottom.clone(),
        SurfaceGeometry::Plane {
            origin: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, -1.0],
        },
    );
    // A section at y 5 of the top view, seen from the front: what lies at
    // y < 5 goes; the view's x the model's x, its y the model's z.
    let line = [on_top(&doc, -5.0, 5.0), on_top(&doc, 75.0, 5.0)];
    let section = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 2,
               "line": line, "flip": true,
               "at": [300, 200]}),
    )["view"]
        .as_u64()
        .unwrap();
    let shown = view(&doc, section);
    let lo = point(&shown["box"][0]);
    let picked = pick_at(&doc, section, [lo[0] + 20.0, lo[1] + 0.2], "line");
    assert_eq!(picked["pick"]["face"], bottom.as_str(), "{picked}");
    let ends = &picked["items"][0]["line"];
    let (a, b) = (point(&ends[0]), point(&ends[1]));
    assert!(near((b[0] - a[0]).abs(), 40.0), "{picked}");
}
