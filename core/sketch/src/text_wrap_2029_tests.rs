// SPDX-License-Identifier: MIT
//! A text in a frame that wraps to the frame's width (mitcad#2029): its
//! lines all lie within the frame's width at every alignment and height,
//! a line break starts a new line, an unwrapped text is one line, and a
//! wrapped text taller than its frame keeps its top and runs on below.

use std::collections::BTreeMap;

use crate::geometry::{Curve2, P2};
use crate::kernel::{FontGlyphs, Glyph};
use crate::text;
use crate::{EntityUid, SketchText};

/// A box glyph half an em wide and one high for every character but the
/// space, which has no outline; the line spacing is 1.5 em.
fn glyphs(text: &str) -> FontGlyphs {
    let square = vec![
        vec![[0.0, 0.0], [0.5, 0.0]],
        vec![[0.5, 0.0], [0.5, 1.0]],
        vec![[0.5, 1.0], [0.0, 1.0]],
        vec![[0.0, 1.0], [0.0, 0.0]],
    ];
    FontGlyphs {
        family: "Box".into(),
        fallback: false,
        ascender: 1.25,
        descender: -0.25,
        cap_height: 1.0,
        line_spacing: 1.5,
        glyphs: text
            .chars()
            .map(|c| Glyph {
                advance: 0.5,
                contours: if c == ' ' {
                    Vec::new()
                } else {
                    vec![square.clone()]
                },
            })
            .collect(),
    }
}

/// The frame's corners: p1 at the origin, p2 along x at `width`, p3 along
/// y at `height`.
fn frame(width: f64, height: f64) -> BTreeMap<EntityUid, P2> {
    BTreeMap::from([
        (EntityUid(1), [0.0, 0.0]),
        (EntityUid(2), [width, 0.0]),
        (EntityUid(3), [0.0, height]),
    ])
}

fn framed(content: &str, height: f64, align: &str, wrap: bool) -> SketchText {
    serde_json::from_value(serde_json::json!({
        "id": "t1", "text": content, "at": [0.0, 0.0], "height": height,
        "frame": ["p1", "p2", "p3"], "align": align, "valign": "top", "wrap": wrap
    }))
    .unwrap()
}

/// Each letter's lowest and highest x and its baseline (its lowest y).
fn letters(text: &SketchText, points: &BTreeMap<EntityUid, P2>) -> Vec<(f64, f64, f64)> {
    let outline = text::layout(text, &glyphs(&text.text), points, &BTreeMap::new()).unwrap();
    outline
        .contours
        .iter()
        .map(|c| {
            let ends: Vec<P2> = c
                .parts
                .iter()
                .map(|p| match p {
                    Curve2::Line { a, .. } => *a,
                    other => panic!("{other:?}"),
                })
                .collect();
            let lo = ends.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
            let hi = ends.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
            let base = ends.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
            (lo, hi, base)
        })
        .collect()
}

/// The lines the layout reports (the sketch query's `lines`).
fn line_count(text: &SketchText, points: &BTreeMap<EntityUid, P2>) -> usize {
    text::layout(text, &glyphs(&text.text), points, &BTreeMap::new())
        .unwrap()
        .lines
}

/// The distinct baselines, from the top down.
fn baselines(letters: &[(f64, f64, f64)]) -> Vec<f64> {
    let mut found: Vec<f64> = Vec::new();
    for &(_, _, base) in letters {
        if !found.iter().any(|b| (b - base).abs() < 1e-6) {
            found.push(base);
        }
    }
    found.sort_by(|a, b| b.total_cmp(a));
    found
}

const LONG: &str = "AA AA AA AA AA AA";

#[test]
fn a_wrapped_text_lies_within_its_frames_width_at_every_alignment_and_height() {
    let points = frame(10.0, 40.0);
    for align in ["left", "center", "right"] {
        for height in [2.0, 3.0, 4.0] {
            let found = letters(&framed(LONG, height, align, true), &points);
            assert_eq!(found.len(), 12, "{align} {height}");
            for &(lo, hi, _) in &found {
                assert!(
                    lo >= -1e-6 && hi <= 10.0 + 1e-6,
                    "{align} {height}: a letter from {lo} to {hi}"
                );
            }
            assert!(baselines(&found).len() > 1, "{align} {height}");
        }
    }
    // A word 2 mm wide and a space 1 mm: three words to a line of 10 mm.
    let found = letters(&framed(LONG, 2.0, "left", true), &frame(10.0, 40.0));
    assert_eq!(baselines(&found).len(), 2);
    assert_eq!(
        line_count(&framed(LONG, 2.0, "left", true), &frame(10.0, 40.0)),
        2
    );
    // A narrower frame rewraps it: two words to a line of 6 mm.
    let found = letters(&framed(LONG, 2.0, "left", true), &frame(6.0, 40.0));
    assert_eq!(baselines(&found).len(), 3);
    assert_eq!(
        line_count(&framed(LONG, 2.0, "left", true), &frame(6.0, 40.0)),
        3
    );
    for &(lo, hi, _) in &found {
        assert!(lo >= -1e-6 && hi <= 6.0 + 1e-6, "from {lo} to {hi}");
    }
}

#[test]
fn a_centred_or_right_aligned_wrapped_line_is_placed_within_the_frame() {
    let points = frame(10.0, 40.0);
    // The first line "AA AA AA" is 8 mm wide: 1 mm in from each side
    // centred, 2 mm in from the left right-aligned.
    let first = |align: &str| {
        let found = letters(&framed(LONG, 2.0, align, true), &points);
        let top = baselines(&found)[0];
        let row: Vec<_> = found.iter().filter(|l| (l.2 - top).abs() < 1e-6).collect();
        (
            row.iter().map(|l| l.0).fold(f64::INFINITY, f64::min),
            row.iter().map(|l| l.1).fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let (lo, hi) = first("center");
    assert!(
        (lo - 1.0).abs() < 1e-6 && (hi - 9.0).abs() < 1e-6,
        "{lo} {hi}"
    );
    let (lo, hi) = first("right");
    assert!(
        (lo - 2.0).abs() < 1e-6 && (hi - 10.0).abs() < 1e-6,
        "{lo} {hi}"
    );
}

#[test]
fn an_unwrapped_text_is_one_line_and_a_line_break_starts_another() {
    let points = frame(10.0, 40.0);
    let found = letters(&framed(LONG, 2.0, "left", false), &points);
    assert_eq!(baselines(&found).len(), 1);
    assert_eq!(line_count(&framed(LONG, 2.0, "left", false), &points), 1);
    let right = found.iter().map(|l| l.1).fold(f64::NEG_INFINITY, f64::max);
    assert!(right > 10.0, "{right}");
    // A line break, wrapped or not.
    for wrap in [false, true] {
        let found = letters(&framed("AA\nAA", 2.0, "left", wrap), &points);
        assert_eq!(baselines(&found).len(), 2, "{wrap}");
    }
}

#[test]
fn a_wrapped_text_taller_than_its_frame_keeps_its_top_and_runs_on_below() {
    // A frame 5 mm high: the first line's ascender at its top, the rest
    // below its bottom, nothing cut or scaled.
    let points = frame(10.0, 5.0);
    let found = letters(&framed(LONG, 4.0, "left", true), &points);
    assert_eq!(found.len(), 12);
    let lines = baselines(&found);
    assert!(lines.len() >= 3, "{lines:?}");
    // The ascender is 1.25 em: the first baseline 5 mm below the top.
    assert!(lines[0].abs() < 1e-6, "{lines:?}");
    assert!(*lines.last().unwrap() < 0.0, "{lines:?}");
    for &(lo, hi, _) in &found {
        assert!(hi - lo > 2.0 - 1e-6 && hi - lo < 2.0 + 1e-6, "{lo} {hi}");
    }
}
