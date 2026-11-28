// SPDX-License-Identifier: MIT
//! ASME's radius and diameter callouts and short vertical dimensions
//! (mitcad#2621): a callout's leader ends in a short shoulder beside its
//! text, the first line's middle level with it, on the side its drag takes
//! it to; a short vertical dimension keeps its value in line with its
//! arrows outside while the value fits between the feet, and takes it
//! outside on an elbow to the left or right when it does not. ISO keeps
//! its layout.

use std::f64::consts::FRAC_PI_2;

use super::*;
use crate::CutPlane;
use crate::compose::{SketchDimension, section_spec, sketch_dimension};

/// View (0, 0) at sheet (100, 100), 1:1.
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

const H: f64 = DIMENSION_TEXT;

fn words(text: &str) -> ValueText {
    ValueText {
        text: text.to_owned(),
        main: text.to_owned(),
        ..ValueText::default()
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The texts: their words, baselines' starts and angles.
fn texts(g: &Group) -> Vec<(String, P2, f64)> {
    g.items
        .iter()
        .filter_map(|i| match i {
            Item::Text {
                at, angle, text, ..
            } => Some((text.clone(), *at, *angle)),
            _ => None,
        })
        .collect()
}

/// The level lines, each from left to right.
fn level_lines(g: &Group) -> Vec<(P2, P2)> {
    g.items
        .iter()
        .filter_map(|i| match i {
            Item::Line { a, b, .. } if near(a[1], b[1]) && !near(a[0], b[0]) => {
                Some(if a[0] <= b[0] { (*a, *b) } else { (*b, *a) })
            }
            _ => None,
        })
        .collect()
}

/// The upright lines, each from bottom to top.
fn upright_lines(g: &Group) -> Vec<(P2, P2)> {
    g.items
        .iter()
        .filter_map(|i| match i {
            Item::Line { a, b, .. } if near(a[0], b[0]) && !near(a[1], b[1]) => {
                Some(if a[1] <= b[1] { (*a, *b) } else { (*b, *a) })
            }
            _ => None,
        })
        .collect()
}

/// The arrowheads' corners.
fn arrow_points(g: &Group) -> Vec<P2> {
    g.items
        .iter()
        .filter_map(|i| match i {
            Item::Fill { points } => Some(points.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// A circle of radius 5 seen end on at view (0, 0): sheet (100, 100); an
/// arc of it when `arc` is given.
fn circle(arc: Option<(f64, f64)>) -> Seen {
    Seen::Circle {
        center: [0.0, 0.0],
        radius: 5.0,
        end_on: true,
        across: [1.0, 0.0],
        arc,
    }
}

const C: P2 = [100.0, 100.0];

/// Where the default text offset puts a whole circle's leader's bend: 15
/// mm out from the centre, up and right.
fn default_bend() -> P2 {
    let out = 15.0 * FRAC_1_SQRT_2;
    [C[0] + out, C[1] + out]
}

/// A radius's (or diameter's) callout laid out on `seen`.
fn callout(
    kind: AnnotationType,
    seen: Seen,
    offset: Option<P2>,
    standard: Standard,
    value: &ValueText,
) -> Group {
    let map = map();
    let mut a = ViewAnnotation::new(1, kind, Vec::new());
    a.text_offset = offset;
    lay_out(&a, &[seen], &frame(&map, standard), Some(value), &[])
        .unwrap()
        .group
}

/// The shoulder of a callout whose leader bends at `bend`: the level line
/// starting or ending there.
fn shoulder(g: &Group, bend: P2) -> (P2, P2) {
    let lines = level_lines(g);
    lines
        .iter()
        .copied()
        .find(|(a, b)| len2(sub2(*a, bend)) < 1e-9 || len2(sub2(*b, bend)) < 1e-9)
        .unwrap_or_else(|| panic!("no shoulder at {bend:?}: {lines:?}"))
}

/// Where the leader from the bend meets the circle's rim: the other end of
/// the line from `bend` that ends on it.
fn leader_rim(g: &Group, bend: P2) -> Option<P2> {
    g.items.iter().find_map(|i| match i {
        Item::Line { a, b, .. } => [(*a, *b), (*b, *a)]
            .into_iter()
            .find(|(p, q)| len2(sub2(*p, bend)) < 1e-9 && near(len2(sub2(*q, C)), 5.0))
            .map(|(_, q)| q),
        _ => None,
    })
}

#[test]
fn an_asme_radius_ends_its_leader_in_a_short_shoulder_beside_its_text() {
    for kind in [AnnotationType::Radius, AnnotationType::Diameter] {
        let label = if kind == AnnotationType::Radius {
            "R5"
        } else {
            "Ø10"
        };
        let bend = default_bend();
        let g = callout(kind, circle(None), None, Standard::Asme, &words(label));
        // The shoulder: short, to the right of the bend.
        let (from, to) = shoulder(&g, bend);
        assert!(near(from[0], bend[0]), "{from:?}");
        assert!(near(to[0] - from[0], ASME_SHOULDER), "{from:?} {to:?}");
        // The text beside its end, its letters' middle level with it.
        let [(text, at, angle)] = &texts(&g)[..] else {
            panic!("{:?}", g.items)
        };
        assert_eq!(text, label);
        assert_eq!(*angle, 0.0);
        assert!(near(at[0], to[0] + TEXT_GAP), "{at:?}");
        assert!(near(at[1] + H / 2.0, bend[1]), "{at:?}");
        // The leader from the rim to the bend, the arrow on the rim.
        let rim = leader_rim(&g, bend).unwrap_or_else(|| panic!("{kind:?}: {:?}", g.items));
        assert!(arrow_points(&g).iter().any(|p| len2(sub2(*p, rim)) < 1e-9));
    }
}

#[test]
fn iso_keeps_its_callout_over_a_shoulder_as_long_as_the_text() {
    let label = words("R5");
    let bend = default_bend();
    let g = callout(
        AnnotationType::Radius,
        circle(None),
        None,
        Standard::Iso,
        &label,
    );
    let (from, to) = shoulder(&g, bend);
    assert!(near(to[0] - from[0], label.width(H) + 1.0));
    let [(_, at, _)] = &texts(&g)[..] else {
        panic!("{:?}", g.items)
    };
    assert!(
        near(at[0], bend[0] + 0.5) && near(at[1], bend[1] + 1.0),
        "{at:?}"
    );
}

#[test]
fn an_asme_callout_writes_its_lines_below_the_first_on_either_side() {
    // Three lines: "2X", the value with the text after's first line, and
    // that text's second line.
    let value = ValueText {
        text: "2X R5 TYP BOTH SIDES".to_owned(),
        main: "2X\nR5".to_owned(),
        after: "TYP\nBOTH SIDES".to_owned(),
        ..ValueText::default()
    };
    let lines: Vec<String> = value.lines().iter().map(|l| l.text.clone()).collect();
    assert_eq!(lines, ["2X", "R5 TYP", "BOTH SIDES"]);
    // One line stays itself.
    assert_eq!(words("R5").lines(), [words("R5")]);
    for (offset, side) in [([12.0, 9.0], 1.0), ([-12.0, 9.0], -1.0)] {
        let bend = add2(C, offset);
        for kind in [AnnotationType::Radius, AnnotationType::Diameter] {
            let g = callout(kind, circle(None), Some(offset), Standard::Asme, &value);
            let (from, to) = shoulder(&g, bend);
            assert!(near(to[0] - from[0], ASME_SHOULDER));
            let end = if side > 0.0 { to } else { from };
            assert!(near(end[0], bend[0] + side * ASME_SHOULDER), "{end:?}");
            let written = texts(&g);
            let words_at = |w: &str| {
                written
                    .iter()
                    .find(|(t, _, _)| t == w)
                    .unwrap_or_else(|| panic!("no {w}: {written:?}"))
                    .1
            };
            // The first line's middle level with the shoulder, the others
            // a line apart below it.
            let first = words_at("2X");
            assert!(near(first[1] + H / 2.0, bend[1]), "{first:?}");
            assert!(near(words_at("R5")[1], first[1] - LINE_PITCH * H));
            assert!(near(words_at("TYP")[1], first[1] - LINE_PITCH * H));
            assert!(near(
                words_at("BOTH SIDES")[1],
                first[1] - 2.0 * LINE_PITCH * H
            ));
            // Every line starts beyond the shoulder's end on its side, the
            // text gap between; on the left each ends there.
            for (t, at, angle) in &written {
                assert_eq!(*angle, 0.0);
                if side > 0.0 {
                    assert!(at[0] >= end[0] + TEXT_GAP - 1e-9, "{t} {at:?}");
                } else {
                    assert!(
                        at[0] + text_width(t, H) <= end[0] - TEXT_GAP + 1e-9,
                        "{t} {at:?}"
                    );
                }
            }
            let ends_at_gap = |w: &str| {
                let at = words_at(w);
                if side > 0.0 {
                    near(at[0], end[0] + TEXT_GAP)
                } else {
                    near(at[0] + text_width(w, H), end[0] - TEXT_GAP)
                }
            };
            assert!(ends_at_gap("2X") && ends_at_gap("BOTH SIDES"));
        }
    }
}

#[test]
fn an_asme_callout_dragged_across_the_centre_turns_to_the_other_side() {
    let map = map();
    let f = frame(&map, Standard::Asme);
    for kind in [AnnotationType::Radius, AnnotationType::Diameter] {
        let seen = [circle(None)];
        let mut a = ViewAnnotation::new(1, kind, Vec::new());
        // Dragged 25 mm left from its default place, right of the centre.
        moved(&mut a, &seen, &f, [-25.0, 0.0]);
        let bend = add2(default_bend(), [-25.0, 0.0]);
        assert!(bend[0] < C[0]);
        let g = lay_out(&a, &seen, &f, Some(&words("R5")), &[])
            .unwrap()
            .group;
        let (from, to) = shoulder(&g, bend);
        // The shoulder runs left from the bend, the text ends beside it.
        assert!(near(to[0], bend[0]) && near(from[0], bend[0] - ASME_SHOULDER));
        let [(_, at, _)] = &texts(&g)[..] else {
            panic!("{:?}", g.items)
        };
        assert!(near(at[0] + text_width("R5", H), from[0] - TEXT_GAP));
        assert!(near(at[1] + H / 2.0, bend[1]));
        // The leader comes in from the left of the circle.
        let rim = leader_rim(&g, bend).unwrap_or_else(|| panic!("{kind:?}: {:?}", g.items));
        assert!(rim[0] < C[0], "{rim:?}");
        // Dragged back: on the right again.
        moved(&mut a, &seen, &f, [25.0, 0.0]);
        let g = lay_out(&a, &seen, &f, Some(&words("R5")), &[])
            .unwrap()
            .group;
        let (from, _) = shoulder(&g, default_bend());
        assert!(near(from[0], default_bend()[0]));
    }
}

#[test]
fn a_drag_of_an_arcs_callout_starts_where_it_is_drawn() {
    // An arc in the upper left quarter: its text out from its middle, left
    // of the centre; a drag of nothing leaves it there.
    let map = map();
    let f = frame(&map, Standard::Asme);
    let seen = [circle(Some((FRAC_PI_2, PI)))];
    let mut a = ViewAnnotation::new(1, AnnotationType::Radius, Vec::new());
    let before = lay_out(&a, &seen, &f, Some(&words("R5")), &[])
        .unwrap()
        .group;
    moved(&mut a, &seen, &f, [0.0, 0.0]);
    let out = 15.0 * FRAC_1_SQRT_2;
    let offset = a.text_offset.unwrap();
    assert!(near(offset[0], -out) && near(offset[1], out), "{offset:?}");
    let after = lay_out(&a, &seen, &f, Some(&words("R5")), &[])
        .unwrap()
        .group;
    assert_eq!(before.items, after.items);
    let (from, to) = shoulder(&after, [C[0] - out, C[1] + out]);
    assert!(near(to[0] - from[0], ASME_SHOULDER) && near(to[0], C[0] - out));
}

/// A vertical dimension `length` high from view (0, 0), its line 15 mm
/// left of its picks (sheet x 85, from y 100 up).
fn short_vertical(length: f64) -> (ViewAnnotation, [Seen; 2]) {
    let mut d = ViewAnnotation::new(1, AnnotationType::Linear, Vec::new());
    d.along = Some(Along::Vertical);
    d.line_offset = Some(15.0);
    (d, [Seen::Point([0.0, 0.0]), Seen::Point([0.0, length])])
}

fn laid(d: &ViewAnnotation, seen: &[Seen], standard: Standard, value: &str) -> Group {
    let map = map();
    lay_out(d, seen, &frame(&map, standard), Some(&words(value)), &[])
        .unwrap()
        .group
}

/// A one-line text's box middle from its baseline's start, written
/// horizontal.
fn box_middle(at: P2, text: &str) -> P2 {
    [at[0] + text_width(text, H) / 2.0, at[1] + H / 2.0]
}

/// Whether every arrowhead lies outside the feet (below `low`, above
/// `high`), pointing in.
fn arrows_out(g: &Group, low: f64, high: f64) -> bool {
    let points = arrow_points(g);
    points.len() == 6
        && points
            .iter()
            .all(|p| p[1] <= low + 1e-9 || p[1] >= high - 1e-9)
}

#[test]
fn a_short_asme_vertical_dimension_keeps_its_value_in_line_with_the_arrows_outside() {
    for (length, value) in [(6.0, "6"), (8.0, "8")] {
        let (d, seen) = short_vertical(length);
        let g = laid(&d, &seen, Standard::Asme, value);
        let [(_, at, angle)] = &texts(&g)[..] else {
            panic!("{:?}", g.items)
        };
        assert_eq!(*angle, 0.0);
        // In the line's middle.
        let c = box_middle(*at, value);
        assert!(
            near(c[0], 85.0) && near(c[1], 100.0 + length / 2.0),
            "{c:?}"
        );
        // The arrows outside, pointing in; none in the text's box and its
        // gap, the line's pieces clear of it.
        let top = 100.0 + length;
        assert!(arrows_out(&g, 100.0, top), "{:?}", g.items);
        let clear = H / 2.0 + TEXT_GAP;
        for (a, b) in upright_lines(&g) {
            assert!(b[1] <= c[1] - clear + 1e-9 || a[1] >= c[1] + clear - 1e-9);
        }
        // Their short lines on beyond the feet.
        let lines = upright_lines(&g);
        assert!(
            lines
                .iter()
                .any(|(a, _)| near(a[1], 100.0 - ARROW_LENGTH - 2.0))
        );
        assert!(
            lines
                .iter()
                .any(|(_, b)| near(b[1], top + ARROW_LENGTH + 2.0))
        );
    }
    // With room for the arrows beside the value: inside as before.
    let (d, seen) = short_vertical(12.0);
    let g = laid(&d, &seen, Standard::Asme, "12");
    let points = arrow_points(&g);
    assert_eq!(points.len(), 6);
    assert!(
        points
            .iter()
            .all(|p| p[1] >= 100.0 - 1e-9 && p[1] <= 112.0 + 1e-9)
    );
}

#[test]
fn a_short_asme_vertical_dimension_takes_its_value_outside_on_an_elbow() {
    let (mut d, seen) = short_vertical(4.0);
    let half = text_width("4", H) / 2.0;
    // Left as placed: above the upper end, on a shoulder away from the
    // picks (left), the arrows outside.
    let g = laid(&d, &seen, Standard::Asme, "4");
    let knee = [85.0, 104.0 + ARROW_LENGTH + 2.0];
    let (from, to) = shoulder(&g, knee);
    assert!(
        near(to[0], 85.0) && near(from[0], 85.0 - ASME_SHOULDER),
        "{from:?}"
    );
    let [(_, at, angle)] = &texts(&g)[..] else {
        panic!("{:?}", g.items)
    };
    assert_eq!(*angle, 0.0);
    let c = box_middle(*at, "4");
    assert!(
        near(c[0] + half, from[0] - TEXT_GAP) && near(c[1], knee[1]),
        "{c:?}"
    );
    assert!(arrows_out(&g, 100.0, 104.0), "{:?}", g.items);
    // The dimension line between the feet, whole: no text on it.
    assert!(
        upright_lines(&g)
            .iter()
            .any(|(a, b)| near(a[1], 100.0) && near(b[1], 104.0))
    );
    // Its text offset's across part to the right: there, as far from the
    // line as it says.
    d.text_offset = Some([0.0, -8.0]);
    let g = laid(&d, &seen, Standard::Asme, "4");
    let (from, to) = shoulder(&g, knee);
    assert!(near(from[0], 85.0) && near(to[0], 85.0 + 8.0 - half - TEXT_GAP));
    let [(_, at, _)] = &texts(&g)[..] else {
        panic!("{:?}", g.items)
    };
    let c = box_middle(*at, "4");
    assert!(near(c[0], 93.0) && near(c[1], knee[1]), "{c:?}");
    // Its along part past the lower end's bend: below, the line run on
    // down to the bend.
    d.text_offset = Some([-15.0, 0.0]);
    let g = laid(&d, &seen, Standard::Asme, "4");
    let low_knee = [85.0, 102.0 - 15.0];
    let (from, _) = shoulder(&g, low_knee);
    assert!(near(from[0], 85.0 - ASME_SHOULDER));
    assert!(
        upright_lines(&g).iter().any(|(a, b)| {
            near(a[1], low_knee[1]) && near(b[1], 102.0 - 2.0 - ARROW_LENGTH - 2.0)
        })
    );
    let [(_, at, _)] = &texts(&g)[..] else {
        panic!("{:?}", g.items)
    };
    assert!(near(box_middle(*at, "4")[1], low_knee[1]));
    // A small drag along keeps it above: the lower end only past its bend.
    d.text_offset = Some([-5.0, 0.0]);
    let g = laid(&d, &seen, Standard::Asme, "4");
    shoulder(&g, knee);
}

#[test]
fn iso_keeps_a_short_vertical_dimension_along_its_line() {
    let (d, seen) = short_vertical(4.0);
    let g = laid(&d, &seen, Standard::Iso, "4");
    let [(_, _, angle)] = &texts(&g)[..] else {
        panic!("{:?}", g.items)
    };
    assert!(near(*angle, FRAC_PI_2));
    // No shoulder: the level lines are the extension lines, from the picks.
    assert!(
        level_lines(&g)
            .iter()
            .all(|(_, b)| near(b[0], 100.0 - EXTENSION_GAP))
    );
    assert!(arrows_out(&g, 100.0, 104.0));
}

#[test]
fn a_retrieved_short_vertical_sketch_dimension_takes_an_elbow_under_asme() {
    let plane = CutPlane {
        origin: [0.0; 3],
        normal: [0.0, 0.0, 1.0],
        bound: None,
    };
    let spec = section_spec(&plane, false, false, true);
    let map = map();
    let dim = SketchDimension::Linear {
        a: [0.0, 0.0, 0.0],
        b: [0.0, 4.0, 0.0],
        along: Some([0.0, 1.0, 0.0]),
        text: None,
        value: 4.0,
        normal: [0.0, 0.0, 1.0],
    };
    let asme = sketch_dimension(&dim, &spec, &map, "4", Standard::Asme).unwrap();
    let shoulders: Vec<(P2, P2)> = level_lines(&asme)
        .into_iter()
        .filter(|(a, b)| near(b[0] - a[0], ASME_SHOULDER))
        .collect();
    let [(from, to)] = shoulders[..] else {
        panic!("{:?}", asme.items)
    };
    let [(_, at, angle)] = &texts(&asme)[..] else {
        panic!("{:?}", asme.items)
    };
    assert_eq!(*angle, 0.0);
    // Its text (written from its middle) level with the shoulder, beside
    // one of its ends.
    assert!(near(at[1] + H / 2.0, from[1]));
    let half = text_width("4", H) / 2.0;
    assert!(
        near(at[0], to[0] + TEXT_GAP + half) || near(at[0], from[0] - TEXT_GAP - half),
        "{at:?} {from:?} {to:?}"
    );
    assert_eq!(arrow_points(&asme).len(), 6);
    // ISO: no shoulder, its text along the line.
    let iso = sketch_dimension(&dim, &spec, &map, "4", Standard::Iso).unwrap();
    assert!(
        level_lines(&iso)
            .iter()
            .all(|(a, b)| !near(b[0] - a[0], ASME_SHOULDER))
    );
}
