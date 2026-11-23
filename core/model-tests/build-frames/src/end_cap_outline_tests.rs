// SPDX-License-Identifier: MIT
//! End caps of closed sections that are neither rectangular nor round
//! (mitcad#2270) with the mock kernel: a hexagonal tube's and an oval
//! tube's caps are their end face's outer loop, set in by the inset,
//! outward past the end and inward with the tube cut back; a hexagon's
//! corners take a chamfer, an oval and an ellipse, which have none,
//! refuse it in words. Hollow sections whose outline turns inward (an L
//! tube, a tube with a notch, mitcad#2287) are closed too: their caps
//! follow the outline set in, its inward corners rounded by the inset; a
//! solid L (an angle) and an L with a bore keep the rectangle; a D-shaped
//! tube's corners between its straight side and its arc take a chamfer.
//! The mock kernel's bodies are boxes: a test gives
//! the member's end faces the boundary the geometry kernel reads off a
//! real tube (its outer and its inner loop), and the plates' areas are
//! the model's own outline's.

use std::f64::consts::PI;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName};
use mitcad_model_tests_support::test_library::*;

/// Caps ends; returns the answer or the refusal.
fn cap(doc: &mut Document<MockKernel>, fields: Value) -> Result<Value, String> {
    let mut fields = fields;
    fields["cmd"] = json!("build_end_cap");
    command(doc, fields)
}

/// Gives a member's end face (`a` or `b`) its boundary, as the geometry
/// kernel reads it off a real section (in the part's coordinates).
fn boundary(doc: &Document<MockKernel>, member: &Value, end: &str, curves: &[Curve3]) {
    let part = derived(doc, component(member));
    let face = FaceName::new(part, if end == "b" { "end_b" } else { "end_a" }, None);
    let edge = EdgeName::new(face.clone(), face.clone());
    doc.kernel().face_boundaries.borrow_mut().insert(
        face.to_string(),
        curves.iter().map(|c| (edge.clone(), c.clone())).collect(),
    );
}

fn line(a: [f64; 2], b: [f64; 2], z: f64) -> Curve3 {
    Curve3::Line {
        start: [a[0], a[1], z],
        end: [b[0], b[1], z],
    }
}

/// A regular hexagon `across` its flats at height `z`, centred on the
/// axis, counter-clockwise or clockwise.
fn hexagon(across: f64, z: f64, clockwise: bool) -> Vec<Curve3> {
    let r = across / 3f64.sqrt();
    let at = |k: usize| {
        let t = PI / 3.0 * k as f64;
        [r * t.cos(), r * t.sin()]
    };
    (0..6)
        .map(|k| {
            if clockwise {
                line(at(k + 1), at(k), z)
            } else {
                line(at(k), at(k + 1), z)
            }
        })
        .collect()
}

/// A hexagonal tube's end face: 40 mm across its flats, a 3 mm wall.
fn hexagonal_tube(z: f64) -> Vec<Curve3> {
    let mut curves = hexagon(40.0, z, false);
    curves.extend(hexagon(34.0, z, true));
    curves
}

/// A regular hexagon's area by its apothem (half across its flats).
fn hexagon_area(apothem: f64) -> f64 {
    2.0 * 3f64.sqrt() * apothem * apothem
}

/// A flat oval at height `z`: half circles of radius `r` about (±`l`, 0)
/// joined by straight sides `2 l` long.
fn oval(r: f64, l: f64, z: f64) -> Vec<Curve3> {
    let half = |cx: f64, x_axis: [f64; 3]| Curve3::Conic {
        center: [cx, 0.0, z],
        normal: [0.0, 0.0, 1.0],
        x_axis,
        major: r,
        minor: r,
        start: 0.0,
        end: PI,
        closed: false,
    };
    vec![
        line([-l, -r], [l, -r], z),
        half(l, [0.0, -1.0, 0.0]),
        line([l, r], [-l, r], z),
        half(-l, [0.0, 1.0, 0.0]),
    ]
}

/// An oval tube's end face: 60 x 30, a 3 mm wall.
fn oval_tube(z: f64) -> Vec<Curve3> {
    let mut curves = oval(15.0, 15.0, z);
    curves.extend(oval(12.0, 15.0, z));
    curves
}

fn oval_area(r: f64, l: f64) -> f64 {
    4.0 * r * l + PI * r * r
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

fn cut_list(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "build_cut_list"}))
}

/// The cut list's plate row of the cap an answer made.
fn plate_row(doc: &Document<MockKernel>, answer: &Value) -> Value {
    let c = &answer["caps"][0]["component"];
    let list = cut_list(doc);
    list["plates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["components"].as_array().unwrap().contains(c))
        .cloned()
        .unwrap_or_else(|| panic!("no plate row of {c}: {list}"))
}

/// The cut list's length of a member.
fn cut_length(doc: &Document<MockKernel>, member: &Value) -> f64 {
    let list = cut_list(doc);
    list["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["components"]
                .as_array()
                .unwrap()
                .contains(&member["component"])
        })
        .and_then(|r| r["length"].as_f64())
        .unwrap_or_else(|| panic!("no row of {}: {list}", member["component"]))
}

/// Checks a cap's plate: its area the outline's set in (`area`), its
/// volume (from its mass, the tube's 7850 kg/m³) the area times the
/// thickness, both within 1e-6 relative.
fn check_plate(doc: &Document<MockKernel>, answer: &Value, area: f64, thickness: f64) {
    let plate = plate_row(doc, answer);
    let got = number(&plate["area"]);
    assert!((got - area).abs() / area < 1e-6, "{got} {area}: {plate}");
    assert!((number(&plate["thickness"]) - thickness).abs() < 1e-9);
    let volume = number(&plate["mass"]) * 1e9 / 7850.0;
    assert!(
        (volume - area * thickness).abs() / (area * thickness) < 1e-6,
        "{volume}: {plate}"
    );
    assert_eq!(plate["disc"], false, "{plate}");
}

#[test]
fn a_hexagonal_tube_s_caps_are_its_hexagon_set_in() {
    let mut doc = design();
    let tube = first(&mut doc, "tube40", [0.0; 3], 1000.0);
    boundary(&doc, &tube, "a", &hexagonal_tube(0.0));
    boundary(&doc, &tube, "b", &hexagonal_tube(1000.0));
    // Outward, set in 1 mm: the hexagon 38 across its flats.
    let a = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "inset": 1}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &a, hexagon_area(19.0), 5.0);
    // Its size: across the corners 2 x 19 / cos 30°, across the flats 38.
    let plate = plate_row(&doc, &a);
    let mut size = [number(&plate["width"]), number(&plate["height"])];
    size.sort_by(f64::total_cmp);
    assert!((size[0] - 38.0).abs() < 1e-6, "{plate}");
    assert!((size[1] - 76.0 / 3f64.sqrt()).abs() < 1e-6, "{plate}");
    // Inward, set in 2 mm: the tube 4 mm shorter, the hexagon 36 across.
    let b = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}],
               "thickness": 4, "inset": 2, "direction": "inward"}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &b, hexagon_area(18.0), 4.0);
    assert!((cut_length(&doc, &tube) - 996.0).abs() < 1e-9);
    // A chamfer of 3 mm on its six corners: each loses a triangle with
    // legs of 3 mm at 120 degrees.
    let chamfered = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "chamfer": 3}),
    )
    .unwrap();
    all_ok(&doc);
    let area = hexagon_area(20.0) - 6.0 * 0.5 * 9.0 * (2.0 * PI / 3.0).sin();
    check_plate(&doc, &chamfered, area, 5.0);
}

#[test]
fn an_oval_tube_s_caps_are_its_oval_set_in() {
    let mut doc = design();
    let tube = first(&mut doc, "tube40", [0.0; 3], 800.0);
    boundary(&doc, &tube, "a", &oval_tube(0.0));
    boundary(&doc, &tube, "b", &oval_tube(800.0));
    // Outward, set in 1 mm: 58 x 28, its half circles of 14 mm.
    let a = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "inset": 1}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &a, oval_area(14.0, 15.0), 5.0);
    let plate = plate_row(&doc, &a);
    let mut size = [number(&plate["width"]), number(&plate["height"])];
    size.sort_by(f64::total_cmp);
    assert!((size[0] - 28.0).abs() < 1e-6, "{plate}");
    assert!((size[1] - 58.0).abs() < 1e-6, "{plate}");
    // Inward, set in 2 mm (a ratio of the thickness: 0.5 x 4).
    let b = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}],
               "thickness": 4, "ratio": 0.5, "direction": "inward"}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &b, oval_area(13.0, 15.0), 4.0);
    assert!((cut_length(&doc, &tube) - 796.0).abs() < 1e-9);
    // The preview's outline follows the oval: more points than a
    // rectangle's corners, all on the end's plane.
    let preview = query(
        &doc,
        json!({"query": "build_end_caps",
               "ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "inset": 1}),
    );
    let outline = preview["plates"][0]["outline"].as_array().unwrap();
    assert!(outline.len() > 8, "{preview}");
    assert!((number(&preview["plates"][0]["area"]) - oval_area(14.0, 15.0)).abs() < 1e-6);
}

#[test]
fn a_chamfer_on_a_section_without_corners_is_refused_in_words() {
    let mut doc = design();
    let tube = first(&mut doc, "tube40", [0.0; 3], 600.0);
    boundary(&doc, &tube, "a", &oval_tube(0.0));
    let refused = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "chamfer": 2}),
    )
    .unwrap_err();
    assert!(refused.contains("no corners to chamfer"), "{refused}");
    // An elliptical tube has none either.
    let ellipse = |major: f64, minor: f64| Curve3::Conic {
        center: [0.0, 0.0, 600.0],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major,
        minor,
        start: 0.0,
        end: 2.0 * PI,
        closed: true,
    };
    boundary(
        &doc,
        &tube,
        "b",
        &[ellipse(30.0, 20.0), ellipse(27.0, 17.0)],
    );
    let refused = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}],
               "thickness": 5, "chamfer": 2}),
    )
    .unwrap_err();
    assert!(refused.contains("no corners to chamfer"), "{refused}");
    // Without the chamfer it is capped: the ellipse's area, π a b.
    let b = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}], "thickness": 5}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &b, PI * 600.0, 5.0);
}

/// A closed polygon's sides at height `z`, in the points' order.
fn polygon(points: &[[f64; 2]], z: f64) -> Vec<Curve3> {
    let n = points.len();
    (0..n)
        .map(|k| line(points[k], points[(k + 1) % n], z))
        .collect()
}

/// An L-shaped tube's end face at height `z`: 60 x 60, its legs 20 wide,
/// its heel at (−30, −30), a 3 mm wall (the inside listed clockwise).
fn l_tube(z: f64) -> Vec<Curve3> {
    let shift = |points: &[[f64; 2]]| -> Vec<[f64; 2]> {
        points.iter().map(|p| [p[0] - 30.0, p[1] - 30.0]).collect()
    };
    let mut curves = polygon(
        &shift(&[
            [0.0, 0.0],
            [60.0, 0.0],
            [60.0, 20.0],
            [20.0, 20.0],
            [20.0, 60.0],
            [0.0, 60.0],
        ]),
        z,
    );
    curves.extend(polygon(
        &shift(&[
            [3.0, 3.0],
            [3.0, 57.0],
            [17.0, 57.0],
            [17.0, 17.0],
            [57.0, 17.0],
            [57.0, 3.0],
        ]),
        z,
    ));
    curves
}

/// The L's outline (2000 mm², 240 mm round) set in by `r`: less `r`
/// times its length, plus r² for each of its five right-angled outward
/// corners (trimmed), less the quarter circle of radius `r` about its
/// inward corner (rounded).
fn l_area(r: f64) -> f64 {
    2000.0 - 240.0 * r + 5.0 * r * r - 0.25 * PI * r * r
}

/// A rectangular tube 60 x 40 with a notch 20 wide and 10 deep in the
/// middle of its top, a 3 mm wall, centred on the axis, at height `z`.
fn notched_tube(z: f64) -> Vec<Curve3> {
    let shift = |points: &[[f64; 2]]| -> Vec<[f64; 2]> {
        points.iter().map(|p| [p[0] - 30.0, p[1] - 20.0]).collect()
    };
    let mut curves = polygon(
        &shift(&[
            [0.0, 0.0],
            [60.0, 0.0],
            [60.0, 40.0],
            [40.0, 40.0],
            [40.0, 30.0],
            [20.0, 30.0],
            [20.0, 40.0],
            [0.0, 40.0],
        ]),
        z,
    );
    curves.extend(polygon(
        &shift(&[
            [3.0, 3.0],
            [3.0, 37.0],
            [17.0, 37.0],
            [17.0, 27.0],
            [43.0, 27.0],
            [43.0, 37.0],
            [57.0, 37.0],
            [57.0, 3.0],
        ]),
        z,
    ));
    curves
}

/// The notched outline (2200 mm², 220 mm round) set in by `r`: six
/// right-angled outward corners trimmed, two inward ones rounded.
fn notched_area(r: f64) -> f64 {
    2200.0 - 220.0 * r + 6.0 * r * r - 2.0 * 0.25 * PI * r * r
}

#[test]
fn an_l_shaped_tube_s_caps_are_its_l_set_in() {
    let mut doc = design();
    let tube = first(&mut doc, "tube40", [0.0; 3], 1000.0);
    boundary(&doc, &tube, "a", &l_tube(0.0));
    boundary(&doc, &tube, "b", &l_tube(1000.0));
    // Outward, set in 1 mm: the L 58 x 58, its inward corner rounded by 1.
    let a = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "inset": 1}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &a, l_area(1.0), 5.0);
    let plate = plate_row(&doc, &a);
    assert!((number(&plate["width"]) - 58.0).abs() < 1e-6, "{plate}");
    assert!((number(&plate["height"]) - 58.0).abs() < 1e-6, "{plate}");
    // Inward, set in 2 mm: the tube 4 mm shorter.
    let b = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}],
               "thickness": 4, "inset": 2, "direction": "inward"}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &b, l_area(2.0), 4.0);
    assert!((cut_length(&doc, &tube) - 996.0).abs() < 1e-9);
    // A chamfer of 3 mm cuts its five outward corners, not the inward one.
    let chamfered = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "chamfer": 3}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &chamfered, 2000.0 - 5.0 * 4.5, 5.0);
}

#[test]
fn a_notched_tube_s_caps_are_its_outline_set_in() {
    let mut doc = design();
    let tube = first(&mut doc, "tube40", [0.0; 3], 800.0);
    boundary(&doc, &tube, "a", &notched_tube(0.0));
    boundary(&doc, &tube, "b", &notched_tube(800.0));
    let a = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "inset": 1.5}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &a, notched_area(1.5), 5.0);
    // Inward, set in 2 mm (a ratio of the thickness: 0.5 x 4).
    let b = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}],
               "thickness": 4, "ratio": 0.5, "direction": "inward"}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &b, notched_area(2.0), 4.0);
    assert!((cut_length(&doc, &tube) - 796.0).abs() < 1e-9);
}

#[test]
fn a_solid_l_and_an_l_with_a_bore_keep_the_rectangle() {
    let mut doc = design();
    let tube = first(&mut doc, "tube40", [0.0; 3], 600.0);
    // A solid L is an angle: the rectangle round the member (the mock
    // kernel's 40 x 40 box).
    let solid: Vec<Curve3> = l_tube(0.0).into_iter().take(6).collect();
    boundary(&doc, &tube, "a", &solid);
    let a = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}], "thickness": 5}),
    )
    .unwrap();
    all_ok(&doc);
    let plate = plate_row(&doc, &a);
    assert!((number(&plate["area"]) - 1600.0).abs() < 1e-9, "{plate}");
    // A bore in its heel is no wall's inside (a slot profile's core bore).
    let mut bored: Vec<Curve3> = l_tube(600.0).into_iter().take(6).collect();
    bored.push(Curve3::Conic {
        center: [-20.0, -20.0, 600.0],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: 4.0,
        minor: 4.0,
        start: 0.0,
        end: 2.0 * PI,
        closed: true,
    });
    boundary(&doc, &tube, "b", &bored);
    let b = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}], "thickness": 5}),
    )
    .unwrap();
    all_ok(&doc);
    let plate = plate_row(&doc, &b);
    assert!((number(&plate["area"]) - 1600.0).abs() < 1e-9, "{plate}");
}

/// A D-shaped tube's end face at height `z`: a half disc of radius 20
/// on the x axis (a straight side and a half circle, with a corner at
/// either end of the side), a 3 mm wall.
fn d_tube(z: f64) -> Vec<Curve3> {
    let arc = |r: f64, from: f64| Curve3::Conic {
        center: [0.0, 0.0, z],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: r,
        minor: r,
        start: from,
        end: PI - from,
        closed: false,
    };
    let x = 280f64.sqrt();
    vec![
        line([-20.0, 0.0], [20.0, 0.0], z),
        arc(20.0, 0.0),
        line([-x, 3.0], [x, 3.0], z),
        arc(17.0, (3.0f64 / 17.0).asin()),
    ]
}

#[test]
fn a_d_shaped_tube_s_corners_between_its_side_and_its_arc_take_a_chamfer() {
    let mut doc = design();
    let tube = first(&mut doc, "tube40", [0.0; 3], 900.0);
    boundary(&doc, &tube, "a", &d_tube(0.0));
    boundary(&doc, &tube, "b", &d_tube(900.0));
    // Each of the two corners loses the triangle of the chamfer's legs
    // (3 mm along the side, a chord of 3 mm along the arc) and the arc's
    // segment off that chord.
    let (r, c): (f64, f64) = (20.0, 3.0);
    let theta = 2.0 * (0.5 * c / r).asin();
    let corner = 0.5 * c * r * theta.sin() + 0.5 * r * r * (theta - theta.sin());
    let area = 0.5 * PI * r * r - 2.0 * corner;
    let a = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "chamfer": 3}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &a, area, 5.0);
    let b = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}],
               "thickness": 4, "chamfer": 3, "direction": "inward"}),
    )
    .unwrap();
    all_ok(&doc);
    check_plate(&doc, &b, area, 4.0);
    assert!((cut_length(&doc, &tube) - 896.0).abs() < 1e-9);
    // Legs longer than half its straight side leave nothing of it.
    let refused = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "chamfer": 21}),
    )
    .unwrap_err();
    assert!(refused.contains("leaves no straight side"), "{refused}");
}
