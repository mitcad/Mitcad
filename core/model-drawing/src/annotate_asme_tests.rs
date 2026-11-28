// SPDX-License-Identifier: MIT
//! A drawing's dimensions under ASME Y14.5 (mitcad#2047): values written
//! horizontal in a break of their dimension lines and arcs, placing and
//! dragging keeping them so, inch values without a leading zero; the same
//! dimensions under ISO 129-1 as before.

use std::f64::consts::FRAC_PI_2;

use super::*;
use crate::compose::{SketchDimension, section_spec, sketch_dimension, sketch_text};
use crate::{CutPlane, Units};

fn map() -> ViewMap {
    ViewMap::new(1.0, [0.0, 0.0], [100.0, 100.0])
}

fn frame(map: &ViewMap, standard: Standard) -> Frame<'_> {
    Frame {
        map,
        view_box: Bounds::empty(),
        text: DIMENSION_TEXT,
        standard,
    }
}

fn words(text: &str) -> ValueText {
    ValueText {
        text: text.to_owned(),
        main: text.to_owned(),
        ..ValueText::default()
    }
}

/// The texts' angles and their baselines' starts.
fn texts(g: &Group) -> Vec<(f64, P2)> {
    g.items
        .iter()
        .filter_map(|i| match i {
            Item::Text { angle, at, .. } => Some((*angle, *at)),
            _ => None,
        })
        .collect()
}

/// The lines parallel to `d` (the dimension line's pieces).
fn along_lines(g: &Group, d: P2) -> Vec<(P2, P2)> {
    g.items
        .iter()
        .filter_map(|i| match i {
            Item::Line { a, b, .. } => {
                let ab = sub2(*b, *a);
                let l = len2(ab);
                (l > 1e-9 && vec2::cross(mul2(ab, 1.0 / l), d).abs() < 1e-9).then_some((*a, *b))
            }
            _ => None,
        })
        .collect()
}

/// The gap between two pieces of a line along `d`, as positions along it.
fn gap(pieces: &[(P2, P2)], d: P2) -> (f64, f64) {
    assert_eq!(pieces.len(), 2, "{pieces:?}");
    let span = |(a, b): (P2, P2)| {
        let (s, t) = (dot2(a, d), dot2(b, d));
        (s.min(t), s.max(t))
    };
    let (a, b) = (span(pieces[0]), span(pieces[1]));
    let (first, second) = if a.0 < b.0 { (a, b) } else { (b, a) };
    assert!(first.1 < second.0, "{first:?} {second:?}");
    (first.1, second.0)
}

/// A one-line text's middle from its baseline's start, written horizontal.
fn middle_of(at: P2, text: &str) -> P2 {
    [
        at[0] + text_width(text, DIMENSION_TEXT) / 2.0,
        at[1] + DIMENSION_TEXT / 2.0,
    ]
}

fn vertical() -> (ViewAnnotation, [Seen; 2]) {
    let mut d = ViewAnnotation::new(1, AnnotationType::Linear, Vec::new());
    d.along = Some(Along::Vertical);
    d.line_offset = Some(15.0);
    (d, [Seen::Point([0.0, 0.0]), Seen::Point([0.0, 40.0])])
}

#[test]
fn a_vertical_dimension_is_written_horizontal_in_a_break_under_asme() {
    let map = map();
    let (d, seen) = vertical();
    // ISO: along the line, reading from the right; the line whole.
    let iso = lay_out(
        &d,
        &seen,
        &frame(&map, Standard::Iso),
        Some(&words("40")),
        &[],
    )
    .unwrap();
    let [(angle, _)] = texts(&iso.group)[..] else {
        panic!("{:?}", iso.group.items)
    };
    assert!((angle - FRAC_PI_2).abs() < 1e-9, "{angle}");
    let whole = along_lines(&iso.group, [0.0, 1.0]);
    assert_eq!(whole.len(), 1, "{whole:?}");
    assert!((len2(sub2(whole[0].1, whole[0].0)) - 40.0).abs() < 1e-9);
    // ASME: horizontal, its middle in the gap of the line's two pieces.
    let asme = lay_out(
        &d,
        &seen,
        &frame(&map, Standard::Asme),
        Some(&words("40")),
        &[],
    )
    .unwrap();
    let [(angle, at)] = texts(&asme.group)[..] else {
        panic!("{:?}", asme.group.items)
    };
    assert_eq!(angle, 0.0);
    let (low, high) = gap(&along_lines(&asme.group, [0.0, 1.0]), [0.0, 1.0]);
    let c = middle_of(at, "40");
    assert!((c[1] - 120.0).abs() < 1e-9, "{c:?}");
    assert!(low < c[1] - DIMENSION_TEXT / 2.0 && high > c[1] + DIMENSION_TEXT / 2.0);
    // The gap is the text's height and the text gap on each side.
    assert!((high - low - (DIMENSION_TEXT + 2.0 * TEXT_GAP)).abs() < 1e-9);
}

#[test]
fn an_aligned_dimension_is_written_horizontal_in_a_break_under_asme() {
    let map = map();
    let mut d = ViewAnnotation::new(1, AnnotationType::Linear, Vec::new());
    d.along = Some(Along::Aligned);
    d.line_offset = Some(10.0);
    let seen = [Seen::Point([0.0, 0.0]), Seen::Point([30.0, 30.0])];
    let dir = [FRAC_1_SQRT_2, FRAC_1_SQRT_2];
    let iso = lay_out(
        &d,
        &seen,
        &frame(&map, Standard::Iso),
        Some(&words("42.43")),
        &[],
    )
    .unwrap();
    assert!((texts(&iso.group)[0].0 - PI / 4.0).abs() < 1e-9);
    assert_eq!(along_lines(&iso.group, dir).len(), 1);
    let asme = lay_out(
        &d,
        &seen,
        &frame(&map, Standard::Asme),
        Some(&words("42.43")),
        &[],
    )
    .unwrap();
    let [(angle, at)] = texts(&asme.group)[..] else {
        panic!("{:?}", asme.group.items)
    };
    assert_eq!(angle, 0.0);
    let (low, high) = gap(&along_lines(&asme.group, dir), dir);
    let c = dot2(middle_of(at, "42.43"), dir);
    assert!(low < c && c < high, "{low} {c} {high}");
}

/// The arcs of a group: their starts and ends, radians.
fn arcs(g: &Group) -> Vec<(f64, f64)> {
    g.items
        .iter()
        .filter_map(|i| match i {
            Item::Arc { start, end, .. } => Some((*start, *end)),
            _ => None,
        })
        .collect()
}

#[test]
fn an_angle_is_written_horizontal_in_a_break_of_its_arc_under_asme() {
    let map = map();
    let mut a = ViewAnnotation::new(3, AnnotationType::Angle, Vec::new());
    a.line_offset = Some(20.0);
    let seen = [
        Seen::Line([0.0, 0.0], [40.0, 0.0]),
        Seen::Line([0.0, 0.0], [0.0, 40.0]),
    ];
    let iso = lay_out(
        &a,
        &seen,
        &frame(&map, Standard::Iso),
        Some(&words("90°")),
        &[],
    )
    .unwrap();
    assert_eq!(arcs(&iso.group).len(), 1);
    assert!(texts(&iso.group)[0].0.abs() > 0.1);
    let asme = lay_out(
        &a,
        &seen,
        &frame(&map, Standard::Asme),
        Some(&words("90°")),
        &[],
    )
    .unwrap();
    let [(angle, at)] = texts(&asme.group)[..] else {
        panic!("{:?}", asme.group.items)
    };
    assert_eq!(angle, 0.0);
    let pieces = arcs(&asme.group);
    assert_eq!(pieces.len(), 2, "{pieces:?}");
    // The text's middle on the arc's middle, in the gap between its pieces.
    let c = middle_of(at, "90°");
    let polar = (c[1] - 100.0).atan2(c[0] - 100.0);
    assert!((polar - PI / 4.0).abs() < 1e-9, "{polar}");
    let (first, second) = if pieces[0].0 < pieces[1].0 {
        (pieces[0], pieces[1])
    } else {
        (pieces[1], pieces[0])
    };
    assert!(first.1 < polar && polar < second.0, "{pieces:?}");
    assert!((first.0 - 0.0).abs() < 1e-9 && (second.1 - FRAC_PI_2).abs() < 1e-9);
}

#[test]
fn an_ordinate_is_written_horizontal_under_asme() {
    let map = map();
    let mut o = ViewAnnotation::new(4, AnnotationType::Ordinate, Vec::new());
    o.along = Some(Along::Horizontal);
    o.line_offset = Some(20.0);
    let seen = [Seen::Point([0.0, 0.0]), Seen::Point([25.0, 0.0])];
    // ISO: along its leader, which runs up.
    let iso = lay_out(
        &o,
        &seen,
        &frame(&map, Standard::Iso),
        Some(&words("25")),
        &[],
    )
    .unwrap();
    assert!(texts(&iso.group)[0].0.abs() > 0.1);
    let asme = lay_out(
        &o,
        &seen,
        &frame(&map, Standard::Asme),
        Some(&words("25")),
        &[],
    )
    .unwrap();
    assert_eq!(texts(&asme.group)[0].0, 0.0);
}

#[test]
fn placing_and_dragging_keep_an_asme_value_horizontal() {
    let map = map();
    let f = frame(&map, Standard::Asme);
    let (mut d, seen) = vertical();
    d.line_offset = None;
    // Placed through a point right of the picks: the line there, the text
    // in its middle.
    placed_at(&mut d, &seen, &f, [130.0, 118.0]);
    assert_eq!(d.line_offset, Some(-30.0));
    let laid = lay_out(&d, &seen, &f, Some(&words("40")), &[]).unwrap();
    let [(angle, at)] = texts(&laid.group)[..] else {
        panic!("{:?}", laid.group.items)
    };
    assert_eq!(angle, 0.0);
    assert!((middle_of(at, "40")[0] - 130.0).abs() < 1e-9);
    // Dragged 5 up along the line: the text and the break follow it.
    moved(&mut d, &seen, &f, [0.0, 5.0]);
    let laid = lay_out(&d, &seen, &f, Some(&words("40")), &[]).unwrap();
    let [(angle, at)] = texts(&laid.group)[..] else {
        panic!("{:?}", laid.group.items)
    };
    assert_eq!(angle, 0.0);
    let c = middle_of(at, "40");
    assert!((c[1] - 125.0).abs() < 1e-9, "{c:?}");
    let (low, high) = gap(&along_lines(&laid.group, [0.0, 1.0]), [0.0, 1.0]);
    assert!(((low + high) / 2.0 - 125.0).abs() < 1e-9);
    // Dragged past the upper end: still horizontal, the line run on to it.
    moved(&mut d, &seen, &f, [0.0, 30.0]);
    let laid = lay_out(&d, &seen, &f, Some(&words("40")), &[]).unwrap();
    let [(angle, at)] = texts(&laid.group)[..] else {
        panic!("{:?}", laid.group.items)
    };
    assert_eq!(angle, 0.0);
    let c = middle_of(at, "40");
    assert!((c[1] - 155.0).abs() < 1e-9, "{c:?}");
    let run_on = along_lines(&laid.group, [0.0, 1.0])
        .iter()
        .any(|(a, b)| a[1].max(b[1]) > 145.0 && a[1].max(b[1]) < c[1]);
    assert!(run_on, "{:?}", laid.group.items);
}

#[test]
fn an_asme_value_that_does_not_fit_goes_beyond_the_line_horizontal() {
    // A horizontal line too short for its value between its feet: beyond
    // its end, along it (an upright one takes an elbow, mitcad#2621).
    let map = map();
    let mut d = ViewAnnotation::new(1, AnnotationType::Linear, Vec::new());
    d.along = Some(Along::Horizontal);
    d.line_offset = Some(-15.0);
    let seen = [Seen::Point([0.0, 0.0]), Seen::Point([6.0, 0.0])];
    let laid = lay_out(
        &d,
        &seen,
        &frame(&map, Standard::Asme),
        Some(&words("6.25")),
        &[],
    )
    .unwrap();
    let [(angle, at)] = texts(&laid.group)[..] else {
        panic!("{:?}", laid.group.items)
    };
    assert_eq!(angle, 0.0);
    assert!(middle_of(at, "6.25")[0] > 106.0 + ARROW_LENGTH);
}

#[test]
fn inch_lengths_drop_their_leading_zero_under_asme() {
    let inches = Units::by_symbol("in").unwrap();
    let mm = Units::MM;
    let in_mm = 0.44 * 25.4;
    assert_eq!(inches.length(in_mm), "0.44");
    assert_eq!(inches.under(Standard::Asme).length(in_mm), ".44");
    assert_eq!(inches.under(Standard::Asme).length(2.375 * 25.4), "2.375");
    assert_eq!(mm.length(0.44), "0.44");
    assert_eq!(mm.under(Standard::Asme).length(0.44), "0.44");
    assert_eq!(crate::without_leading_zero("-0.005"), "-.005");
    assert_eq!(crate::without_leading_zero("+0.1"), "+.1");
    assert_eq!(crate::without_leading_zero("10.5"), "10.5");
    // A retrieved sketch dimension's words.
    let radial = SketchDimension::Radial {
        center: [0.0; 3],
        radius: in_mm,
        diameter: false,
        text: None,
        value: in_mm,
        normal: [0.0, 0.0, 1.0],
    };
    assert_eq!(sketch_text(&radial, inches.under(Standard::Asme)), "R.44");
    assert_eq!(sketch_text(&radial, inches), "R0.44");
}

#[test]
fn a_retrieved_vertical_sketch_dimension_is_horizontal_under_asme() {
    let plane = CutPlane {
        origin: [0.0; 3],
        normal: [0.0, 0.0, 1.0],
        bound: None,
    };
    let spec = section_spec(&plane, false, false, true);
    let map = map();
    let dim = SketchDimension::Linear {
        a: [0.0, 0.0, 0.0],
        b: [0.0, 40.0, 0.0],
        along: Some([0.0, 1.0, 0.0]),
        text: None,
        value: 40.0,
        normal: [0.0, 0.0, 1.0],
    };
    let iso = sketch_dimension(&dim, &spec, &map, "40", Standard::Iso).unwrap();
    assert!((texts(&iso)[0].0 - FRAC_PI_2).abs() < 1e-9);
    assert_eq!(along_lines(&iso, [0.0, 1.0]).len(), 1);
    let asme = sketch_dimension(&dim, &spec, &map, "40", Standard::Asme).unwrap();
    assert_eq!(texts(&asme)[0].0, 0.0);
    let (low, high) = gap(&along_lines(&asme, [0.0, 1.0]), [0.0, 1.0]);
    assert!(low < 120.0 && 120.0 < high);
}
