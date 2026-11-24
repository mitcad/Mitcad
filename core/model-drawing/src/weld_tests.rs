// SPDX-License-Identifier: MIT
//! Weld symbols laid out (mitcad#2350): each basic symbol on its side of
//! the reference line by ISO 2553 and by AWS A2.4, the identification line,
//! sizes and lengths written by each standard, the supplementary symbols
//! and the tail, a line running left; and the checks of what a symbol
//! keeps.

use super::*;
use crate::annotate::{
    AnnotationType, ViewAnnotation, WeldContour, WeldFinish, WeldKind, WeldSide, WeldSymbol,
};

const H: f64 = 3.5;
const TIP: P2 = [90.0, 90.0];
const KNEE: P2 = [100.0, 100.0];

fn drawn(weld: &WeldSymbol, standard: Standard) -> Group {
    let mut g = Group::new("weld-symbol");
    weld_symbol(&mut g, TIP, KNEE, weld, standard, H);
    g
}

fn side(kind: WeldKind) -> WeldSide {
    WeldSide::new(kind)
}

fn sized(kind: WeldKind, size: f64) -> WeldSide {
    WeldSide {
        size: Some(size),
        ..WeldSide::new(kind)
    }
}

fn texts(g: &Group) -> Vec<String> {
    g.items
        .iter()
        .filter_map(|i| match i {
            Item::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn level(a: P2, b: P2, y: f64) -> bool {
    (a[1] - y).abs() < 1e-9 && (b[1] - y).abs() < 1e-9
}

/// The reference line: the thin level line through the knee from it.
fn reference(g: &Group) -> (P2, P2) {
    g.items
        .iter()
        .find_map(|i| match i {
            Item::Line {
                pen: Pen::Thin,
                a,
                b,
            } if level(*a, *b, KNEE[1]) && *a != TIP => Some((*a, *b)),
            _ => None,
        })
        .expect("a reference line")
}

/// The dashed identification line, if drawn.
fn identification(g: &Group) -> Option<(P2, P2)> {
    g.items.iter().find_map(|i| match i {
        Item::Line {
            pen: Pen::Hidden,
            a,
            b,
        } => Some((*a, *b)),
        _ => None,
    })
}

/// The points of the symbol's lines and arcs: every line and arc but the
/// arrow's line, the reference line and the identification line (arcs by
/// their centres and their extreme points).
fn symbol_points(g: &Group) -> Vec<P2> {
    let (ra, rb) = reference(g);
    let mut out = Vec::new();
    for item in &g.items {
        match item {
            Item::Line { pen, a, b } => {
                let arrow_line = *a == TIP || *b == TIP;
                let reference_line = *a == ra && *b == rb;
                if arrow_line || reference_line || *pen == Pen::Hidden {
                    continue;
                }
                out.push(*a);
                out.push(*b);
            }
            Item::Arc {
                center,
                radius,
                start,
                end,
                ..
            } => {
                for k in 0..=8 {
                    let t = start + (end - start) * f64::from(k) / 8.0;
                    out.push([center[0] + radius * t.cos(), center[1] + radius * t.sin()]);
                }
            }
            _ => {}
        }
    }
    out
}

fn lines(g: &Group) -> Vec<(P2, P2)> {
    g.items
        .iter()
        .filter_map(|i| match i {
            Item::Line { a, b, .. } => Some((*a, *b)),
            _ => None,
        })
        .collect()
}

fn arcs(g: &Group) -> Vec<(P2, f64, f64, f64)> {
    g.items
        .iter()
        .filter_map(|i| match i {
            Item::Arc {
                center,
                radius,
                start,
                end,
                ..
            } => Some((*center, *radius, *start, *end)),
            _ => None,
        })
        .collect()
}

#[test]
fn iso_puts_the_arrow_side_above_the_line_and_the_other_side_under_the_dashed_line() {
    let weld = WeldSymbol::arrow(sized(WeldKind::Fillet, 5.0));
    let g = drawn(&weld, Standard::Iso);
    let (a, b) = reference(&g);
    assert_eq!(a, KNEE, "the line starts where the arrow's line meets it");
    assert!(b[0] > a[0], "it runs away from the arrow: {a:?} {b:?}");
    let (da, db) = identification(&g).expect("ISO's dashed identification line");
    let dashed = KNEE[1] - IDENTIFICATION_GAP * H;
    assert!(level(da, db, dashed), "{da:?} {db:?}");
    let points = symbol_points(&g);
    assert!(!points.is_empty());
    assert!(
        points.iter().all(|p| p[1] >= KNEE[1] - 1e-9),
        "the arrow side's fillet stands on the line: {points:?}"
    );
    assert!(texts(&g).contains(&"a5".to_owned()), "{:?}", texts(&g));
    // The other side hangs from the dashed line.
    let weld = WeldSymbol {
        other_side: Some(sized(WeldKind::Fillet, 5.0)),
        ..WeldSymbol::default()
    };
    let g = drawn(&weld, Standard::Iso);
    assert!(identification(&g).is_some());
    let points = symbol_points(&g);
    assert!(
        points.iter().all(|p| p[1] <= dashed + 1e-9),
        "the other side's fillet hangs from the dashed line: {points:?}"
    );
}

#[test]
fn a_symmetrical_iso_weld_has_no_identification_line() {
    let weld = WeldSymbol {
        arrow_side: Some(side(WeldKind::V)),
        other_side: Some(side(WeldKind::V)),
        ..WeldSymbol::default()
    };
    let g = drawn(&weld, Standard::Iso);
    assert!(identification(&g).is_none());
    let points = symbol_points(&g);
    assert!(points.iter().any(|p| p[1] > KNEE[1] + 1.0));
    assert!(points.iter().any(|p| p[1] < KNEE[1] - 1.0));
    // Two kinds: the dashed line tells the sides apart.
    let weld = WeldSymbol {
        arrow_side: Some(side(WeldKind::V)),
        other_side: Some(side(WeldKind::BackWeld)),
        ..WeldSymbol::default()
    };
    assert!(identification(&drawn(&weld, Standard::Iso)).is_some());
}

#[test]
fn aws_puts_the_arrow_side_below_and_the_other_side_above_one_line() {
    let weld = WeldSymbol::arrow(sized(WeldKind::Fillet, 6.0));
    let g = drawn(&weld, Standard::Asme);
    assert!(identification(&g).is_none(), "AWS has no dashed line");
    let points = symbol_points(&g);
    assert!(
        points.iter().all(|p| p[1] <= KNEE[1] + 1e-9),
        "the arrow side below: {points:?}"
    );
    // A fillet's leg written bare.
    assert!(texts(&g).contains(&"6".to_owned()), "{:?}", texts(&g));
    let weld = WeldSymbol {
        other_side: Some(side(WeldKind::Fillet)),
        ..WeldSymbol::default()
    };
    let points = symbol_points(&drawn(&weld, Standard::Asme));
    assert!(
        points.iter().all(|p| p[1] >= KNEE[1] - 1e-9),
        "the other side above: {points:?}"
    );
}

#[test]
fn every_basic_symbol_stands_on_its_side_in_its_own_shape() {
    let s = SYMBOL * H;
    let mut shapes: Vec<(WeldKind, Vec<P2>)> = Vec::new();
    for kind in WeldKind::ALL {
        for (standard, up) in [(Standard::Iso, 1.0), (Standard::Asme, -1.0)] {
            let g = drawn(&WeldSymbol::arrow(side(kind)), standard);
            let points = symbol_points(&g);
            assert!(!points.is_empty(), "{kind:?} {standard:?} draws nothing");
            // On the arrow side, within a symbol's height of the line.
            for p in &points {
                let v = (p[1] - KNEE[1]) * up;
                assert!(
                    (-1e-6..=s + 1e-6).contains(&v),
                    "{kind:?} {standard:?}: {p:?} off its side"
                );
            }
            // Mirrored, not otherwise changed, from one standard to the
            // other.
            if standard == Standard::Iso {
                shapes.push((kind, points));
            } else {
                let iso = &shapes.last().unwrap().1;
                assert_eq!(iso.len(), points.len(), "{kind:?}");
                for a in iso {
                    let mirrored = points.iter().any(|b| {
                        (a[0] - b[0]).abs() < 1e-6
                            && ((a[1] - KNEE[1]) + (b[1] - KNEE[1])).abs() < 1e-6
                    });
                    assert!(mirrored, "{kind:?}: {a:?} not mirrored in {points:?}");
                }
            }
        }
    }
    // Each kind its own shape.
    for (i, (a, pa)) in shapes.iter().enumerate() {
        for (b, pb) in &shapes[i + 1..] {
            let same = pa.len() == pb.len()
                && pa
                    .iter()
                    .zip(pb)
                    .all(|(p, q)| (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6);
            assert!(!same, "{a:?} and {b:?} are drawn alike");
        }
    }
}

#[test]
fn a_fillet_bevel_and_j_have_their_upright_leg_on_the_left() {
    let s = SYMBOL * H;
    for kind in [WeldKind::Fillet, WeldKind::Bevel, WeldKind::J] {
        let g = drawn(&WeldSymbol::arrow(side(kind)), Standard::Iso);
        let points = symbol_points(&g);
        let leftmost = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let upright = lines(&g).into_iter().any(|(a, b)| {
            (a[0] - leftmost).abs() < 1e-6
                && (b[0] - leftmost).abs() < 1e-6
                && ((a[1] - b[1]).abs() - s).abs() < 1e-6
        });
        assert!(upright, "{kind:?}: no upright leg at its left");
    }
    // A spot is a whole circle, a back weld half of one.
    let spot = drawn(&WeldSymbol::arrow(side(WeldKind::Spot)), Standard::Iso);
    assert!(
        arcs(&spot)
            .iter()
            .any(|(_, _, a, b)| (b - a - TAU).abs() < 1e-9)
    );
    let back = drawn(&WeldSymbol::arrow(side(WeldKind::BackWeld)), Standard::Iso);
    assert!(
        arcs(&back)
            .iter()
            .any(|(c, _, a, b)| { (b - a - PI).abs() < 1e-9 && (c[1] - KNEE[1]).abs() < 1e-9 })
    );
}

#[test]
fn sizes_and_lengths_are_written_by_each_standard() {
    let fillet = WeldSide {
        size: Some(5.0),
        count: Some(4),
        length: Some(50.0),
        pitch: Some(150.0),
        ..WeldSide::new(WeldKind::Fillet)
    };
    assert_eq!(size_text(&fillet, Standard::Iso), "a5");
    assert_eq!(length_text(&fillet, Standard::Iso), "4×50 (100)");
    assert_eq!(size_text(&fillet, Standard::Asme), "5");
    assert_eq!(length_text(&fillet, Standard::Asme), "50-150 (4)");
    let leg = WeldSide {
        measure: Some(FilletMeasure::Leg),
        size: Some(7.0),
        ..WeldSide::new(WeldKind::Fillet)
    };
    assert_eq!(size_text(&leg, Standard::Iso), "z7");
    let groove = WeldSide {
        size: Some(10.0),
        penetration: Some(8.5),
        ..WeldSide::new(WeldKind::V)
    };
    assert_eq!(size_text(&groove, Standard::Iso), "s8.5");
    assert_eq!(size_text(&groove, Standard::Asme), "10 (8.5)");
    assert_eq!(size_text(&sized(WeldKind::Spot, 6.0), Standard::Iso), "d6");
    assert_eq!(size_text(&sized(WeldKind::Seam, 4.0), Standard::Iso), "c4");
    // Drawn beside the symbol: the size left of it, the length right.
    let g = drawn(&WeldSymbol::arrow(fillet), Standard::Iso);
    let place = |wanted: &str| {
        g.items.iter().find_map(|i| match i {
            Item::Text { text, at, .. } if text == wanted => Some(*at),
            _ => None,
        })
    };
    let (size, length) = (place("a5").unwrap(), place("4×50 (100)").unwrap());
    assert!(size[0] < length[0]);
    let points = symbol_points(&g);
    let (lo, hi) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[0]), hi.max(p[0]))
        });
    assert!(
        size[0] <= lo && length[0] >= hi,
        "{size:?} {length:?} {lo} {hi}"
    );
    let (_, end) = reference(&g);
    assert!(end[0] > length[0], "the line runs under the length");
}

#[test]
fn all_around_field_weld_and_tail_are_drawn_at_the_line_ends() {
    let weld = WeldSymbol {
        all_around: true,
        field: true,
        tail: Some("141".to_owned()),
        ..WeldSymbol::arrow(sized(WeldKind::Fillet, 4.0))
    };
    for standard in [Standard::Iso, Standard::Asme] {
        let g = drawn(&weld, standard);
        // The circle about the knee.
        assert!(
            arcs(&g).iter().any(|(c, r, a, b)| *c == KNEE
                && (r - 0.4 * H).abs() < 1e-9
                && (b - a - TAU).abs() < 1e-9),
            "{standard:?}: no all around circle"
        );
        // The flag above the knee, toward the tail.
        let flag = g.items.iter().find_map(|i| match i {
            Item::Fill { points } if points.iter().all(|p| p[1] > KNEE[1]) => Some(points.clone()),
            _ => None,
        });
        let flag = flag.expect("a field weld's flag");
        assert!(flag.iter().any(|p| p[0] > KNEE[0]));
        // The fork at the far end with the process number beyond it.
        let (_, end) = reference(&g);
        let fork = lines(&g)
            .into_iter()
            .filter(|(a, b)| *a == end && b[0] > end[0])
            .count();
        assert_eq!(fork, 2, "{standard:?}: the tail's fork");
        let tail = g.items.iter().find_map(|i| match i {
            Item::Text { text, at, .. } if text == "141" => Some(*at),
            _ => None,
        });
        assert!(tail.expect("the tail's text")[0] > end[0]);
    }
    // No tail without its text.
    let g = drawn(&WeldSymbol::arrow(side(WeldKind::Fillet)), Standard::Iso);
    let (_, end) = reference(&g);
    assert!(!lines(&g).iter().any(|(a, b)| *a == end && b[0] > end[0]));
}

#[test]
fn a_line_running_left_keeps_its_texts_reading_from_below() {
    let mut g = Group::new("weld-symbol");
    let weld = WeldSymbol {
        tail: Some("ISO 4063-135".to_owned()),
        ..WeldSymbol::arrow(sized(WeldKind::Fillet, 5.0))
    };
    let (tip, knee) = ([120.0, 90.0], [100.0, 100.0]);
    weld_symbol(&mut g, tip, knee, &weld, Standard::Iso, H);
    let line = g
        .items
        .iter()
        .find_map(|i| match i {
            Item::Line {
                pen: Pen::Thin,
                a,
                b,
            } if level(*a, *b, knee[1]) => Some((*a, *b)),
            _ => None,
        })
        .unwrap();
    assert_eq!(line.1, knee, "the line ends at the knee, left of it");
    assert!(line.0[0] < knee[0]);
    // The size still left of the symbol, the tail's text left of the
    // fork, every text level.
    for item in &g.items {
        if let Item::Text { angle, .. } = item {
            assert_eq!(*angle, 0.0);
        }
    }
    let tail = g.items.iter().find_map(|i| match i {
        Item::Text {
            text, at, anchor, ..
        } if text == "ISO 4063-135" => Some((*at, *anchor)),
        _ => None,
    });
    let (at, anchor) = tail.unwrap();
    assert!(at[0] < line.0[0]);
    assert_eq!(anchor, Anchor::End);
}

#[test]
fn contours_finishes_and_blended_toes_stand_beyond_the_symbol() {
    let s = SYMBOL * H;
    for contour in [
        WeldContour::Flush,
        WeldContour::Convex,
        WeldContour::Concave,
    ] {
        let weld = WeldSymbol::arrow(WeldSide {
            contour: Some(contour),
            finish: Some(WeldFinish::Grind),
            ..WeldSide::new(WeldKind::V)
        });
        let g = drawn(&weld, Standard::Iso);
        let top = symbol_points(&g)
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(top > KNEE[1] + s, "{contour:?} beyond the symbol: {top}");
        assert!(texts(&g).contains(&"G".to_owned()));
    }
    let weld = WeldSymbol::arrow(WeldSide {
        toes_blended: true,
        ..WeldSide::new(WeldKind::Fillet)
    });
    let plain = arcs(&drawn(
        &WeldSymbol::arrow(side(WeldKind::Fillet)),
        Standard::Iso,
    ))
    .len();
    assert_eq!(arcs(&drawn(&weld, Standard::Iso)).len(), plain + 2);
}

#[test]
fn staggered_welds_offset_the_other_side() {
    let intermittent = WeldSide {
        length: Some(50.0),
        pitch: Some(200.0),
        ..WeldSide::new(WeldKind::Fillet)
    };
    let weld = WeldSymbol {
        arrow_side: Some(intermittent.clone()),
        other_side: Some(intermittent),
        staggered: true,
        ..WeldSymbol::default()
    };
    let g = drawn(&weld, Standard::Asme);
    let points = symbol_points(&g);
    let left_of = |below: bool| {
        points
            .iter()
            .filter(|p| (p[1] < KNEE[1] - 1e-6) == below && (p[1] - KNEE[1]).abs() > 1e-6)
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min)
    };
    assert!(left_of(false) > left_of(true) + 1.0);
}

#[test]
fn a_weld_symbol_keeps_only_what_it_can_be() {
    let check = |weld: WeldSymbol| {
        let mut a = ViewAnnotation::new(1, AnnotationType::Weld, Vec::new());
        a.weld = Some(weld);
        a.weld.as_ref().unwrap().check()
    };
    assert!(check(WeldSymbol::arrow(side(WeldKind::Fillet))).is_ok());
    let none = check(WeldSymbol::default()).unwrap_err();
    assert!(none.contains("arrow side"), "{none}");
    let pitch = check(WeldSymbol::arrow(WeldSide {
        length: Some(50.0),
        pitch: Some(40.0),
        ..WeldSide::new(WeldKind::Fillet)
    }))
    .unwrap_err();
    assert!(pitch.contains("pitch"), "{pitch}");
    let measure = check(WeldSymbol::arrow(WeldSide {
        measure: Some(FilletMeasure::Leg),
        ..WeldSide::new(WeldKind::V)
    }))
    .unwrap_err();
    assert!(measure.contains("fillet"), "{measure}");
    let staggered = check(WeldSymbol {
        staggered: true,
        ..WeldSymbol::arrow(side(WeldKind::Fillet))
    })
    .unwrap_err();
    assert!(staggered.contains("both sides"), "{staggered}");
    let size = check(WeldSymbol::arrow(sized(WeldKind::Fillet, -1.0))).unwrap_err();
    assert!(size.contains("more than 0"), "{size}");
    let tail = check(WeldSymbol {
        tail: Some("  ".to_owned()),
        ..WeldSymbol::arrow(side(WeldKind::Fillet))
    })
    .unwrap_err();
    assert!(tail.contains("tail"), "{tail}");
    // Its label.
    let weld = WeldSymbol {
        all_around: true,
        tail: Some("141".to_owned()),
        ..WeldSymbol::arrow(sized(WeldKind::Fillet, 5.0))
    };
    assert_eq!(weld.label(), "fillet a5 (arrow side) / all around / 141");
}

#[test]
fn a_weld_annotation_is_laid_out_on_its_pick() {
    use crate::annotate::{Frame, Seen, lay_out};
    use crate::clip::ViewMap;
    let map = ViewMap::new(1.0, [0.0, 0.0], [0.0, 0.0]);
    let frame = Frame {
        map: &map,
        view_box: crate::compose::Bounds::empty(),
        text: H,
        standard: Standard::Iso,
    };
    let mut annotation = ViewAnnotation::new(7, AnnotationType::Weld, Vec::new());
    annotation.weld = Some(WeldSymbol::arrow(sized(WeldKind::Fillet, 5.0)));
    let seen = [Seen::Line([0.0, 0.0], [40.0, 0.0])];
    annotation.at = Some([20.0, 0.0]);
    let laid = lay_out(&annotation, &seen, &frame, None, &[]).unwrap();
    assert_eq!(laid.group.class, "weld-symbol");
    assert_eq!(laid.group.id.as_deref(), Some("A7"));
    // The arrow on the edge where it was clicked, the line from it to the
    // reference line by the default shift.
    assert!(laid.group.items.iter().any(|i| matches!(i,
        Item::Line { a, b, .. } if *a == [20.0, 0.0]
            && *b == [20.0 + crate::annotate::WELD_SHIFT[0], crate::annotate::WELD_SHIFT[1]])));
    assert!(texts(&laid.group).contains(&"a5".to_owned()));
}
