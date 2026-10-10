// SPDX-License-Identifier: MIT
//! Writer tests: round trips through R2000 and R12.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use mitcad_dxf::{
    Drawing, DxfVersion, Geometry, Layer, Point2, Spline, Text, Units, WriteOptions, read_str,
    write_file, write_string,
};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn sample_drawing() -> Drawing {
    let mut d = Drawing::new(Units::Millimeters);
    let mut hidden = Layer::new("Construction");
    hidden.color = 3;
    hidden.visible = false;
    d.layers.push(hidden);
    d.push("0", Geometry::Point(p(1.0, 2.0)));
    d.push(
        "Outline",
        Geometry::Line {
            start: p(0.1, 0.2),
            end: p(10.0 / 3.0, -7.5),
        },
    );
    d.push(
        "Outline",
        Geometry::Circle {
            center: p(5.0, 5.0),
            radius: 2.5,
        },
    );
    d.push("Outline", Geometry::arc(p(-1.0, 0.0), 4.0, 0.25, 4.0));
    d.push(
        "Construction",
        Geometry::Ellipse {
            center: p(3.0, -2.0),
            major_axis: p(6.0, 2.0),
            ratio: 0.3,
            start_param: 0.0,
            end_param: TAU,
        },
    );
    d.push(
        "Outline",
        Geometry::Ellipse {
            center: p(0.0, 0.0),
            major_axis: p(0.0, 5.0),
            ratio: 0.6,
            start_param: 1.0,
            end_param: 2.5,
        },
    );
    d.push(
        "Curves",
        Geometry::Spline(Spline {
            degree: 2,
            control_points: vec![p(1.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)],
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            weights: vec![1.0, 0.5_f64.sqrt(), 1.0],
            ..Spline::default()
        }),
    );
    d.push(
        "Curves",
        Geometry::Spline(Spline {
            degree: 3,
            fit_points: vec![p(0.0, 0.0), p(2.0, 1.0), p(4.0, -1.0), p(6.0, 0.0)],
            ..Spline::default()
        }),
    );
    d.push(
        "Notes",
        Geometry::Text(Text {
            position: p(2.0, 8.0),
            height: 3.5,
            rotation: FRAC_PI_2,
            content: "Hole \u{2300}5 \u{E4}".into(),
        }),
    );
    d
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn near_point(a: Point2, b: Point2) -> bool {
    near(a.x, b.x) && near(a.y, b.y)
}

/// Equal geometry up to rounding (angles pass through degrees).
fn same(a: &Geometry, b: &Geometry) -> bool {
    use Geometry::*;
    match (a, b) {
        (Point(p1), Point(p2)) => near_point(*p1, *p2),
        (Line { start: s1, end: e1 }, Line { start: s2, end: e2 }) => {
            near_point(*s1, *s2) && near_point(*e1, *e2)
        }
        (
            Circle {
                center: c1,
                radius: r1,
            },
            Circle {
                center: c2,
                radius: r2,
            },
        ) => near_point(*c1, *c2) && near(*r1, *r2),
        (
            Arc {
                center: c1,
                radius: r1,
                start_angle: s1,
                end_angle: e1,
            },
            Arc {
                center: c2,
                radius: r2,
                start_angle: s2,
                end_angle: e2,
            },
        ) => near_point(*c1, *c2) && near(*r1, *r2) && near(*s1, *s2) && near(*e1, *e2),
        (
            Ellipse {
                center: c1,
                major_axis: m1,
                ratio: r1,
                start_param: s1,
                end_param: e1,
            },
            Ellipse {
                center: c2,
                major_axis: m2,
                ratio: r2,
                start_param: s2,
                end_param: e2,
            },
        ) => {
            near_point(*c1, *c2)
                && near_point(*m1, *m2)
                && near(*r1, *r2)
                && near(*s1, *s2)
                && near(*e1, *e2)
        }
        (Text(t1), Text(t2)) => {
            near_point(t1.position, t2.position)
                && near(t1.height, t2.height)
                && near(t1.rotation, t2.rotation)
                && t1.content == t2.content
        }
        _ => a == b,
    }
}

#[test]
fn r2000_round_trip() {
    let drawing = sample_drawing();
    let text = write_string(&drawing, &WriteOptions::default()).unwrap();
    assert!(text.contains("AC1015"));
    assert!(text.is_ascii());
    let back = read_str(&text).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    assert_eq!(back.version.as_deref(), Some("AC1015"));
    assert_eq!(back.units, Units::Millimeters);
    assert_eq!(back.entities.len(), drawing.entities.len());
    for (a, b) in drawing.entities.iter().zip(&back.entities) {
        assert_eq!(a.layer, b.layer);
        assert!(
            same(&a.geometry, &b.geometry),
            "{:?}\n!=\n{:?}",
            a.geometry,
            b.geometry
        );
    }
    let construction = back.layer("Construction").unwrap();
    assert!(!construction.visible);
    assert_eq!(construction.color, 3);
    for name in ["0", "Outline", "Curves", "Notes"] {
        assert!(back.layer(name).is_some(), "layer {name}");
    }
}

#[test]
fn r2000_handles_are_unique() {
    let text = write_string(&sample_drawing(), &WriteOptions::default()).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    // $HANDSEED, the next free handle, follows its name with group code 5.
    let index = lines.iter().position(|l| *l == "$HANDSEED").unwrap();
    let handseed = u64::from_str_radix(lines[index + 2], 16).unwrap();
    let handles: Vec<u64> = lines
        .chunks(2)
        .filter(|pair| matches!(pair[0].trim(), "5" | "105"))
        .map(|pair| u64::from_str_radix(pair[1], 16).unwrap())
        .filter(|&h| h != handseed)
        .collect();
    let mut sorted = handles.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), handles.len(), "duplicate handles");
    assert!(handles.len() > 20);
    assert!(sorted.iter().all(|&h| h < handseed));
}

#[test]
fn r12_round_trip() {
    let drawing = sample_drawing();
    let options = WriteOptions {
        version: DxfVersion::R12,
        tolerance: 0.001,
    };
    let text = write_string(&drawing, &options).unwrap();
    assert!(text.contains("AC1009"));
    assert!(!text.contains("ELLIPSE") && !text.contains("SPLINE"));
    let back = read_str(&text).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    // Exact entities come back as they were.
    for i in [0, 1, 2, 3, 8] {
        let original = &drawing.entities[i];
        assert!(
            back.entities
                .iter()
                .any(|e| e.layer == original.layer && same(&e.geometry, &original.geometry)),
            "{original:?} missing"
        );
    }
    // Ellipses and splines come back as lines within the tolerance.
    let construction: Vec<_> = back
        .entities
        .iter()
        .filter(|e| e.layer == "Construction")
        .collect();
    assert!(construction.len() > 50);
    for e in &construction {
        let Geometry::Line { start, .. } = e.geometry else {
            panic!("{:?}", e.geometry)
        };
        // Point on the ellipse center (3, -2), axes (6, 2) and 0.3 * (-2, 6).
        let q = start - p(3.0, -2.0);
        let major = p(6.0, 2.0);
        let minor = major.perp() * 0.3;
        let u = q.dot(major) / major.dot(major);
        let v = q.dot(minor) / minor.dot(minor);
        assert!((u * u + v * v - 1.0).abs() < 1e-9);
    }
    let first_ellipse_point = construction[0].geometry.clone();
    let last = construction.last().unwrap();
    // The full ellipse is a closed polyline.
    let (Geometry::Line { start, .. }, Geometry::Line { end, .. }) =
        (&first_ellipse_point, &last.geometry)
    else {
        panic!()
    };
    assert_eq!(start, end);
    let quarter: Vec<_> = back
        .entities
        .iter()
        .filter(|e| e.layer == "Curves")
        .filter_map(|e| match e.geometry {
            Geometry::Line { start, end } => Some((start, end)),
            _ => None,
        })
        .collect();
    // Rational quarter circle (radius 1), then the 3 fit point segments.
    assert!(quarter.len() > 10);
    for (start, _) in &quarter[..quarter.len() - 3] {
        assert!((start.length() - 1.0).abs() < 1e-9);
    }
    assert_eq!(quarter[quarter.len() - 1].1, p(6.0, 0.0));
    // Chords of the arc deviate about the tolerance at most (the check is
    // made at the parameter midpoint, not at the farthest point).
    for (start, end) in &quarter[..quarter.len() - 3] {
        let mid = (*start + *end) * 0.5;
        assert!(1.0 - mid.length() <= 0.0015);
    }
}

#[test]
fn units_are_written() {
    for units in [Units::Inches, Units::Meters, Units::Unitless] {
        let mut d = Drawing::new(units);
        d.push(
            "0",
            Geometry::Circle {
                center: p(0.0, 0.0),
                radius: 1.0,
            },
        );
        for version in [DxfVersion::R12, DxfVersion::R2000] {
            let text = write_string(
                &d,
                &WriteOptions {
                    version,
                    tolerance: 0.01,
                },
            )
            .unwrap();
            assert_eq!(read_str(&text).unwrap().units, units);
        }
    }
}

#[test]
fn file_round_trip_and_imported_arcs() {
    // A full-circle arc and an arc crossing angle zero survive degrees.
    let mut d = Drawing::new(Units::Millimeters);
    d.push("0", Geometry::arc(p(0.0, 0.0), 1.0, 1.5 * PI, 2.5 * PI));
    let path = std::env::temp_dir().join(format!("mitcad-dxf-test-{}.dxf", std::process::id()));
    write_file(&d, &path, &WriteOptions::default()).unwrap();
    let back = mitcad_dxf::read_file(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(
        same(&d.entities[0].geometry, &back.entities[0].geometry),
        "{:?}",
        back.entities[0]
    );
}

#[test]
fn rejects_bad_options() {
    let options = WriteOptions {
        version: DxfVersion::R12,
        tolerance: 0.0,
    };
    assert!(write_string(&Drawing::default(), &options).is_err());
}

fn distance_to_segment(point: Point2, start: Point2, end: Point2) -> f64 {
    let delta = end - start;
    let length = delta.length();
    if length == 0.0 {
        return point.distance(start);
    }
    let direction = delta * (1.0 / length);
    let along = (point - start).dot(direction).clamp(0.0, length);
    point.distance(start + direction * along)
}

/// Check the DXF's actual finite line segments against dense samples of the
/// original NURBS, rather than just checking that subdivision added vertices.
fn assert_r12_spline_error(spline: &Spline, tolerance: f64) {
    let mut drawing = Drawing::new(Units::Millimeters);
    drawing.push("Curve", Geometry::Spline(spline.clone()));
    let text = write_string(
        &drawing,
        &WriteOptions {
            version: DxfVersion::R12,
            tolerance,
        },
    )
    .unwrap();
    let back = read_str(&text).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    let segments: Vec<_> = back
        .entities
        .iter()
        .map(|entity| {
            let Geometry::Line { start, end } = entity.geometry else {
                panic!("expected a polyline segment: {:?}", entity.geometry);
            };
            (start, end)
        })
        .collect();
    assert!(!segments.is_empty());
    let (start, end) = spline.domain().unwrap();
    for i in 0..=16_384 {
        let t = start + (end - start) * i as f64 / 16_384.0;
        let point = spline.point_at(t).unwrap();
        let error = segments
            .iter()
            .map(|&(a, b)| distance_to_segment(point, a, b))
            .fold(f64::INFINITY, f64::min);
        assert!(
            error <= tolerance + 1e-10,
            "t={t}: error {error} > {tolerance}"
        );
    }
    // The exact R2000 path must remain independent of tessellation.
    let exact = write_string(&drawing, &WriteOptions::default()).unwrap();
    let exact_back = read_str(&exact).unwrap();
    assert_eq!(
        exact_back.entities[0].geometry,
        drawing.entities[0].geometry
    );
}

#[test]
fn r12_inflected_cubic_respects_tolerance() {
    // Issue #113: x(t)=100t, y(t)=100(t-1/8)^3. The old midpoint
    // criterion misses the first inflection; the dense check includes 1/16.
    assert_r12_spline_error(
        &Spline {
            degree: 3,
            control_points: vec![
                p(0.0, -0.1953125),
                p(100.0 / 3.0, 1.3671875),
                p(200.0 / 3.0, -9.5703125),
                p(100.0, 66.9921875),
            ],
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            ..Spline::default()
        },
        0.01,
    );
}

#[test]
fn r12_tight_bend_and_collinear_doubleback_respect_tolerance() {
    for controls in [
        vec![
            p(0.0, 0.0),
            p(0.001, 100.0),
            p(-0.001, -100.0),
            p(0.01, 0.0),
        ],
        vec![p(0.0, 0.0), p(100.0, 0.0), p(-100.0, 0.0), p(1.0, 0.0)],
        vec![p(0.0, 0.0), p(10.0, 10.0), p(-10.0, 10.0), p(0.0, 0.0)],
    ] {
        assert_r12_spline_error(
            &Spline {
                degree: 3,
                control_points: controls,
                knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
                ..Spline::default()
            },
            0.001,
        );
    }
}

#[test]
fn r12_multiple_spans_and_repeated_knots_respect_tolerance() {
    assert_r12_spline_error(
        &Spline {
            degree: 3,
            control_points: vec![
                p(0.0, 0.0),
                p(1.0, 5.0),
                p(2.0, -3.0),
                p(3.0, 6.0),
                p(4.0, -4.0),
                p(5.0, 2.0),
                p(6.0, 0.0),
            ],
            knots: vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0],
            ..Spline::default()
        },
        0.001,
    );
    assert_r12_spline_error(
        &Spline {
            degree: 2,
            control_points: vec![
                p(1.0, 0.0),
                p(1.0, 1.0),
                p(0.0, 1.0),
                p(-1.0, 1.0),
                p(-1.0, 0.0),
            ],
            knots: vec![0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0],
            weights: vec![1.0, 0.5_f64.sqrt(), 1.0, 0.5_f64.sqrt(), 1.0],
            ..Spline::default()
        },
        0.001,
    );
}

#[test]
fn r12_unclamped_periodic_spline_respects_tolerance() {
    assert_r12_spline_error(
        &Spline {
            degree: 2,
            control_points: vec![
                p(0.0, 0.0),
                p(2.0, 0.0),
                p(1.0, 2.0),
                p(0.0, 0.0),
                p(2.0, 0.0),
            ],
            knots: (0..8).map(f64::from).collect(),
            closed: true,
            periodic: true,
            ..Spline::default()
        },
        0.001,
    );
}

#[test]
fn r12_reports_an_unreachable_spline_tolerance() {
    let spline = Spline {
        degree: 2,
        control_points: vec![p(0.0, 0.0), p(1.0, 2.0), p(2.0, 0.0)],
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        ..Spline::default()
    };
    let mut drawing = Drawing::default();
    drawing.push("0", Geometry::Spline(spline));
    let options = WriteOptions {
        version: DxfVersion::R12,
        tolerance: 1e-100,
    };
    let error = write_string(&drawing, &options).unwrap_err();
    assert!(error.message.contains("entity 0"), "{error}");
    assert!(error.message.contains("subdivision limit"), "{error}");
    assert!(
        write_string(
            &drawing,
            &WriteOptions {
                version: DxfVersion::R2000,
                ..options
            }
        )
        .is_ok()
    );
}
