// SPDX-License-Identifier: MIT
//! Insert Points (mitcad#2232): the point file reader, the airfoil layouts,
//! the plan of what goes into the sketch and the least-squares fit. The
//! airfoils are made here from the NACA 4-digit formula; no coordinate
//! file of a database is committed.

use std::f64::consts::PI;
use std::fmt::Write as _;

use crate::geometry::{Nurbs, P2};
use crate::point_file::{AirfoilLayout, PlannedCurve, PointOptions, THROUGH_MAX, plan, read};
use crate::point_fit::{self, Ends, MAX_CONTROL};

/// A point of a NACA 4-digit airfoil of chord 1 at chord station `x`:
/// camber `m`, its place `p`, thickness `t`; `sharp` closes the trailing
/// edge (the 0.1036 coefficient instead of 0.1015).
fn naca_point(m: f64, p: f64, t: f64, x: f64, sharp: bool, upper: bool) -> P2 {
    let a4 = if sharp { 0.1036 } else { 0.1015 };
    let yt = 5.0
        * t
        * (0.2969 * x.sqrt() - 0.1260 * x - 0.3516 * x * x + 0.2843 * x.powi(3) - a4 * x.powi(4));
    let (yc, slope) = if m == 0.0 {
        (0.0, 0.0)
    } else if x < p {
        (
            m / (p * p) * (2.0 * p * x - x * x),
            2.0 * m / (p * p) * (p - x),
        )
    } else {
        (
            m / ((1.0 - p) * (1.0 - p)) * ((1.0 - 2.0 * p) + 2.0 * p * x - x * x),
            2.0 * m / ((1.0 - p) * (1.0 - p)) * (p - x),
        )
    };
    let theta = slope.atan();
    if upper {
        [x - yt * theta.sin(), yc + yt * theta.cos()]
    } else {
        [x + yt * theta.sin(), yc - yt * theta.cos()]
    }
}

/// A surface of `count` points from the leading edge to the trailing edge,
/// spaced closer at both edges (cosine spacing).
fn naca_surface(code: [f64; 3], count: usize, sharp: bool, upper: bool) -> Vec<P2> {
    (0..count)
        .map(|k| {
            let x = 0.5 * (1.0 - (PI * k as f64 / (count - 1) as f64).cos());
            naca_point(code[0], code[1], code[2], x, sharp, upper)
        })
        .collect()
}

const NACA2412: [f64; 3] = [0.02, 0.4, 0.12];

fn line(out: &mut String, p: P2, decimals: usize) {
    writeln!(out, " {:.d$}  {:.d$}", p[0], p[1], d = decimals).unwrap();
}

/// A Selig file: the name, then from the trailing edge over the upper
/// surface round the leading edge to the trailing edge.
fn selig_text(code: [f64; 3], upper: usize, lower: usize, sharp: bool, decimals: usize) -> String {
    let mut out = String::from("NACA 2412\n");
    for p in naca_surface(code, upper, sharp, true).iter().rev() {
        line(&mut out, *p, decimals);
    }
    for p in naca_surface(code, lower, sharp, false).iter().skip(1) {
        line(&mut out, *p, decimals);
    }
    out
}

/// A Lednicer file: the name, the counts, then each surface from the
/// leading edge, blank lines between.
fn lednicer_text(code: [f64; 3], upper: usize, lower: usize, decimals: usize) -> String {
    let mut out = format!("NACA 2412\n{upper}.  {lower}.\n\n");
    for p in naca_surface(code, upper, false, true) {
        line(&mut out, p, decimals);
    }
    out.push('\n');
    for p in naca_surface(code, lower, false, false) {
        line(&mut out, p, decimals);
    }
    out
}

fn near(a: P2, b: P2, within: f64) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) <= within
}

/// The curvature's sign changes along samples, those of curvatures
/// smaller than `ignore` left out.
fn sign_changes(curvatures: impl IntoIterator<Item = f64>, ignore: f64) -> usize {
    let mut last = 0.0f64;
    let mut changes = 0;
    for k in curvatures {
        if k.abs() <= ignore {
            continue;
        }
        if last != 0.0 && k.signum() != last {
            changes += 1;
        }
        last = k.signum();
    }
    changes
}

fn curvature(d1: P2, d2: P2) -> f64 {
    (d1[0] * d2[1] - d1[1] * d2[0]) / d1[0].hypot(d1[1]).powi(3)
}

/// The exact profile's curvature times the chord, along it in the Selig
/// order: u from -π (the upper trailing edge) through 0 (the leading edge)
/// to π (the lower trailing edge).
fn exact_curvatures(code: [f64; 3], samples: usize) -> Vec<f64> {
    let at = |u: f64| {
        let x = 0.5 * (1.0 - u.cos());
        naca_point(code[0], code[1], code[2], x, false, u < 0.0)
    };
    let h: f64 = 1e-4;
    (0..samples)
        .map(|k| {
            let u = -PI + (k as f64 + 0.5) * 2.0 * PI / samples as f64;
            // Within one surface: no difference straddles the leading edge.
            let h = h.min(0.4 * u.abs()).min(0.4 * (PI - u.abs()));
            let (a, b, c) = (at(u - h), at(u), at(u + h));
            let d1 = [(c[0] - a[0]) / (2.0 * h), (c[1] - a[1]) / (2.0 * h)];
            let d2 = [
                (c[0] - 2.0 * b[0] + a[0]) / (h * h),
                (c[1] - 2.0 * b[1] + a[1]) / (h * h),
            ];
            curvature(d1, d2)
        })
        .collect()
}

fn fitted_curvatures(nurbs: &Nurbs, chord: f64, samples: usize) -> Vec<f64> {
    (0..samples)
        .map(|k| {
            let t = (k as f64 + 0.5) / samples as f64;
            let [_, d1, d2] = nurbs.eval(t);
            curvature(d1, d2) * chord
        })
        .collect()
}

/// The largest distance of the points from the curve, measured on a fine
/// polyline of it (independent of the fit's own measure).
fn polyline_deviation(nurbs: &Nurbs, points: &[P2]) -> f64 {
    let samples = 20_000;
    let line: Vec<P2> = (0..=samples)
        .map(|k| nurbs.point(k as f64 / samples as f64))
        .collect();
    let to_segment = |p: P2, a: P2, b: P2| {
        let d = [b[0] - a[0], b[1] - a[1]];
        let len2 = d[0] * d[0] + d[1] * d[1];
        let s = if len2 > 0.0 {
            (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (p[0] - a[0] - s * d[0]).hypot(p[1] - a[1] - s * d[1])
    };
    points
        .iter()
        .map(|p| {
            line.windows(2)
                .map(|w| to_segment(*p, w[0], w[1]))
                .fold(f64::INFINITY, f64::min)
        })
        .fold(0.0, f64::max)
}

#[test]
fn a_csv_with_a_header_reads_its_points() {
    let file = read("x,y\n0,0\n10.5,2\n20,-3.25\n").unwrap();
    assert_eq!(file.points, vec![[0.0, 0.0], [10.5, 2.0], [20.0, -3.25]]);
    assert_eq!(file.name.as_deref(), Some("x,y"));
    assert_eq!(file.decimals, 2);
    assert!(file.airfoil.is_none());
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
    // Semicolons, CRLF and a byte order mark too.
    let file = read("\u{feff}x;y\r\n1;2\r\n3;4\r\n").unwrap();
    assert_eq!(file.points, vec![[1.0, 2.0], [3.0, 4.0]]);
}

#[test]
fn a_txt_with_tabs_reads_its_points() {
    let file = read("0\t0\n1.5\t2.25\n\t3\t-.5\n# a comment\n4.\t1e-1\n").unwrap();
    assert_eq!(
        file.points,
        vec![[0.0, 0.0], [1.5, 2.25], [3.0, -0.5], [4.0, 0.1]]
    );
    assert_eq!(file.name, None);
}

#[test]
fn a_third_column_is_ignored_with_a_warning() {
    let file = read("0 0 5\n1 1 5\n2 0 5\n").unwrap();
    assert_eq!(file.points, vec![[0.0, 0.0], [1.0, 1.0], [2.0, 0.0]]);
    assert_eq!(file.warnings.len(), 1, "{:?}", file.warnings);
    assert!(file.warnings[0].contains("third"), "{:?}", file.warnings);
}

#[test]
fn too_few_points_are_refused_in_words() {
    let none = read("just a name\nanother line\n").unwrap_err();
    assert!(none.starts_with("the file has no points"), "{none}");
    let empty = read("").unwrap_err();
    assert!(empty.starts_with("the file has no points"), "{empty}");
    let one = read("name\n1 2\n").unwrap_err();
    assert!(one.starts_with("the file has only one point"), "{one}");
    // A placeholder is no number.
    let dots = read("name\n...... ......\n1 2\n").unwrap_err();
    assert!(dots.starts_with("the file has only one point"), "{dots}");
}

#[test]
fn selig_and_lednicer_files_of_one_profile_give_the_same_points() {
    let selig = read(&selig_text(NACA2412, 61, 61, false, 6)).unwrap();
    let lednicer = read(&lednicer_text(NACA2412, 61, 61, 6)).unwrap();
    let (s, l) = (selig.airfoil.unwrap(), lednicer.airfoil.unwrap());
    assert_eq!(s.layout, AirfoilLayout::Selig);
    assert_eq!(l.layout, AirfoilLayout::Lednicer);
    assert_eq!(s.profile.len(), 121);
    assert_eq!(s.profile, l.profile);
    assert_eq!(selig.name.as_deref(), Some("NACA 2412"));
    assert_eq!(lednicer.name.as_deref(), Some("NACA 2412"));
    // The counts line is not a point: both surfaces' points, the leading
    // edge in each.
    assert_eq!(lednicer.points.len(), 122);
    assert!(!lednicer.points.contains(&[61.0, 61.0]));
}

#[test]
fn a_file_outside_0_to_1_is_no_airfoil() {
    let file = read("0 0\n50 10\n100 0\n50 -10\n0 0\n").unwrap();
    assert!(file.airfoil.is_none());
    assert!(
        file.not_airfoil.contains("not from about 0 to 1"),
        "{}",
        file.not_airfoil
    );
    let refused = plan(
        &file,
        &PointOptions {
            airfoil: Some(true),
            ..PointOptions::default()
        },
    )
    .unwrap_err();
    assert!(
        refused.starts_with("the file is not an airfoil: "),
        "{refused}"
    );
}

#[test]
fn a_rounded_naca_2412_of_200_points_is_fitted_smoothly_within_the_tolerance() {
    let text = selig_text(NACA2412, 100, 101, false, 5);
    let file = read(&text).unwrap();
    assert_eq!(file.points.len(), 200);
    assert_eq!(file.decimals, 5);
    let options = PointOptions::default();
    let planned = plan(&file, &options).unwrap();
    assert!(planned.airfoil);
    // The default tolerance: 0.01 mm (half the last place times 100 mm is
    // less).
    assert_eq!(planned.tolerance, 0.01);
    let PlannedCurve::Fitted(fit) = &planned.curve else {
        panic!("not fitted: {:?}", planned.curve);
    };
    assert!(fit.within, "{} mm", fit.deviation);
    assert!(planned.deviation <= 0.01, "{}", planned.deviation);
    assert!(
        fit.control.len() <= 40,
        "{} control points",
        fit.control.len()
    );
    assert!(planned.warnings.is_empty(), "{:?}", planned.warnings);
    let nurbs = fit.nurbs();
    let measured = polyline_deviation(&nurbs, &planned.points);
    assert!(measured <= 0.01 + 1e-4, "{measured} mm");
    // No wiggle: the curvature changes sign no more often than the exact
    // profile's (curvatures of a radius over 100 chords left out).
    let exact = sign_changes(exact_curvatures(NACA2412, 8000), 1e-2);
    let fitted = sign_changes(fitted_curvatures(&nurbs, 100.0, 8000), 1e-2);
    assert!(
        fitted <= exact,
        "the fit's curvature changes sign {fitted} times, the profile's {exact}"
    );
    // The ends on the trailing edge's two points; blunt: a closing line.
    assert_eq!(fit.control[0], planned.points[0]);
    assert_eq!(
        fit.control[fit.control.len() - 1],
        planned.points[planned.points.len() - 1]
    );
    assert!(!planned.shared_ends && planned.closing_line);
    assert!(!planned.closed);
}

#[test]
fn chord_and_angle_place_the_leading_and_trailing_edges() {
    let file = read(&selig_text(NACA2412, 40, 40, true, 6)).unwrap();
    let angle = 5f64.to_radians();
    let planned = plan(
        &file,
        &PointOptions {
            chord: 150.0,
            angle,
            at: [20.0, 10.0],
            ..PointOptions::default()
        },
    )
    .unwrap();
    assert!(near(planned.leading_edge.unwrap(), [20.0, 10.0], 1e-9));
    // The sharp trailing edge: both ends one point, 150 mm away at -5°.
    assert!(planned.shared_ends && !planned.closing_line);
    let te = planned.points[0];
    assert_eq!(planned.points[planned.points.len() - 1], te);
    let expected = [20.0 + 150.0 * angle.cos(), 10.0 - 150.0 * angle.sin()];
    assert!(near(te, expected, 1e-6), "{te:?} {expected:?}");
    // The quarter chord as the pivot: there at `at`, the leading edge
    // raised.
    let pivoted = plan(
        &file,
        &PointOptions {
            chord: 100.0,
            angle,
            pivot: 0.25,
            ..PointOptions::default()
        },
    )
    .unwrap();
    let le = pivoted.leading_edge.unwrap();
    assert!(
        near(le, [-25.0 * angle.cos(), 25.0 * angle.sin()], 1e-9),
        "{le:?}"
    );
}

#[test]
fn a_closed_list_is_closed_and_an_airfoil_never_is() {
    // A circle of 100 points, the first again at the end.
    let mut text = String::new();
    for k in 0..=100 {
        let a = 2.0 * PI * k as f64 / 100.0;
        let p = if k == 100 {
            [10.0, 0.0]
        } else {
            [10.0 * a.cos(), 10.0 * a.sin()]
        };
        writeln!(text, "{} {}", p[0], p[1]).unwrap();
    }
    let file = read(&text).unwrap();
    let planned = plan(&file, &PointOptions::default()).unwrap();
    assert!(planned.closed && planned.shared_ends);
    let PlannedCurve::Fitted(fit) = &planned.curve else {
        panic!("not fitted");
    };
    // Smooth at the join: the same tangent at both ends.
    let nurbs = fit.nurbs();
    let ([_, start, _], [_, end, _]) = (nurbs.eval(0.0), nurbs.eval(1.0));
    let cross = start[0] * end[1] - start[1] * end[0];
    assert!(cross.abs() <= 1e-9 * start[0].hypot(start[1]) * end[0].hypot(end[1]));
    assert!(start[0] * end[0] + start[1] * end[1] > 0.0);
    assert!(fit.within);
    // closed false: an open spline that ends where it starts.
    let open = plan(
        &file,
        &PointOptions {
            closed: Some(false),
            ..PointOptions::default()
        },
    )
    .unwrap();
    assert!(!open.closed && open.shared_ends);
    // An airfoil refuses closed.
    let airfoil = read(&selig_text(NACA2412, 40, 40, true, 6)).unwrap();
    let refused = plan(
        &airfoil,
        &PointOptions {
            closed: Some(true),
            ..PointOptions::default()
        },
    )
    .unwrap_err();
    assert!(
        refused.starts_with("an airfoil is one open spline"),
        "{refused}"
    );
}

#[test]
fn up_to_30_points_go_through_each_and_points_are_only_points() {
    let mut text = String::new();
    for k in 0..12 {
        writeln!(text, "{} {}", k as f64 * 5.0, (k as f64 * 0.7).sin() * 8.0).unwrap();
    }
    let file = read(&text).unwrap();
    let planned = plan(&file, &PointOptions::default()).unwrap();
    assert_eq!(planned.curve, PlannedCurve::Through);
    assert_eq!(planned.points.len(), 12);
    assert_eq!(planned.deviation, 0.0);
    const { assert!(12 <= THROUGH_MAX) };
    let points = plan(
        &file,
        &PointOptions {
            as_points: true,
            scale: 2.0,
            at: [1.0, 1.0],
            ..PointOptions::default()
        },
    )
    .unwrap();
    assert_eq!(points.curve, PlannedCurve::Points);
    assert_eq!(points.points[1], [11.0, 1.0 + 2.0 * (0.7f64).sin() * 8.0]);
    assert!(points.leading_edge.is_none() && !points.closing_line);
}

#[test]
fn the_cap_of_control_points_warns_with_the_deviation_reached() {
    // Thirty waves: more than 60 control points are needed at 1 µm.
    let mut text = String::new();
    for k in 0..1200 {
        let x = k as f64 * 0.2;
        writeln!(text, "{x} {}", (x * 30.0 * 2.0 * PI / 240.0).sin() * 10.0).unwrap();
    }
    let file = read(&text).unwrap();
    let planned = plan(
        &file,
        &PointOptions {
            tolerance: Some(0.001),
            ..PointOptions::default()
        },
    )
    .unwrap();
    let PlannedCurve::Fitted(fit) = &planned.curve else {
        panic!("not fitted");
    };
    assert!(!fit.within);
    assert_eq!(fit.control.len(), MAX_CONTROL);
    assert_eq!(planned.warnings.len(), 1, "{:?}", planned.warnings);
    assert!(
        planned.warnings[0].starts_with("the fitted spline stops at 60 control points"),
        "{:?}",
        planned.warnings
    );
}

#[test]
fn a_fit_puts_its_ends_on_the_first_and_last_point() {
    let points: Vec<P2> = (0..50)
        .map(|k| {
            let x = k as f64;
            [x, (x * 0.2).sin() * 5.0]
        })
        .collect();
    let fit = point_fit::fit(&points, 0.01, MAX_CONTROL, Ends::Open).unwrap();
    assert_eq!(fit.control[0], points[0]);
    assert_eq!(fit.control[fit.control.len() - 1], points[49]);
    assert_eq!(fit.knots.len(), fit.control.len() + 4);
    assert!(fit.within && fit.deviation <= 0.01);
    assert!(point_fit::fit(&points[..3], 0.01, MAX_CONTROL, Ends::Open).is_err());
}
