// SPDX-License-Identifier: MIT
//! Letters of a sketch text and the profiles picked of them (mitcad#1993):
//! a reference to a region between two outlines that a change moved apart
//! finds both outlines' regions, not the larger alone, and a glyph contour
//! that cannot bound a profile is told instead of dropped silently.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::SketchText;
use crate::def::SketchOutput;
use crate::geometry::Curve2;
use crate::kernel::{FontGlyphs, Glyph};
use crate::regions::{Region, RegionCurve, best_matches, regions_and_layout};
use crate::solve::Solved;
use crate::text;
use crate::topo::RegionKey;
use crate::{EntityUid, profile::SketchFrame};

/// Two circles of radius 5, c1 about the origin and c2 about `[x, 0]`.
fn circles(x: f64) -> Vec<RegionCurve> {
    [(1, 0.0), (2, x)]
        .into_iter()
        .map(|(id, cx)| {
            RegionCurve::new(
                EntityUid(id),
                Curve2::Circle {
                    center: [cx, 0.0],
                    radius: 5.0,
                    start: 0.0,
                },
            )
        })
        .collect()
}

/// The evaluated sketch of the curves, as features find regions in it.
fn output(curves: &[RegionCurve]) -> SketchOutput {
    let (info, layout) = regions_and_layout(curves);
    SketchOutput {
        frame: SketchFrame::XY,
        plane: SketchFrame::XY,
        base: SketchFrame::XY,
        regions: info.iter().map(|r| r.profile.clone()).collect(),
        region_info: info,
        layout: Arc::new(layout),
        solved: Arc::new(Solved::default()),
        texts: Default::default(),
        moved: Default::default(),
        follow: Default::default(),
        own: Default::default(),
        retrimmed: None,
        developed: None,
    }
}

fn keys(regions: &[&Region]) -> Vec<String> {
    let mut keys: Vec<String> = regions.iter().map(|r| r.profile.key.to_string()).collect();
    keys.sort();
    keys
}

#[test]
fn regions_between_outlines_moved_apart_find_both_outlines() {
    // Two overlapping circles, as two letters of a text on a tight arc:
    // the part of each outside the other and the lens between them.
    let overlapping = output(&circles(8.0));
    let old: Vec<RegionKey> = overlapping
        .region_info
        .iter()
        .map(|r| r.profile.key.clone())
        .collect();
    assert_eq!(old.len(), 3, "{old:?}");
    // Moved apart (the text flipped to the other side of the arc): each
    // old key names pieces of both outlines, which no longer cross, so it
    // traces no outline and both circles are as like it. Each finds both,
    // the circle of c2 too, which the larger-first tie (equal areas, then
    // the first key) once left out of every reference.
    let apart = output(&circles(20.0));
    assert_eq!(apart.region_info.len(), 2);
    for key in &old {
        let found = apart.found(key).unwrap_or_else(|| panic!("{key}"));
        assert!(!found.exact);
        assert_eq!(keys(&found.regions), ["r{c1}", "r{c2}"], "{key}");
        assert_eq!(
            keys(&best_matches(&apart.region_info, key)),
            ["r{c1}", "r{c2}"]
        );
    }
    // A key that names one circle's outline alone still finds it alone.
    let one: RegionKey = "r{c2}".parse().unwrap();
    assert_eq!(keys(&apart.found(&one).unwrap().regions), ["r{c2}"]);
}

#[test]
fn a_glyph_contour_that_does_not_close_is_told() {
    // "IJ": the I a closed box half an em wide, the J's pieces with a gap.
    let square = vec![
        vec![[0.0, 0.0], [0.5, 0.0]],
        vec![[0.5, 0.0], [0.5, 1.0]],
        vec![[0.5, 1.0], [0.0, 1.0]],
        vec![[0.0, 1.0], [0.0, 0.0]],
    ];
    let broken = vec![
        vec![[0.0, 0.0], [0.5, 0.0]],
        vec![[0.5, 0.1], [0.5, 1.0]],
        vec![[0.5, 1.0], [0.0, 1.0]],
        vec![[0.0, 1.0], [0.0, 0.0]],
    ];
    let glyphs = FontGlyphs {
        family: "Box".into(),
        fallback: false,
        ascender: 1.25,
        descender: -0.25,
        cap_height: 1.0,
        line_spacing: 1.5,
        glyphs: vec![
            Glyph {
                advance: 0.5,
                contours: vec![square],
            },
            Glyph {
                advance: 0.5,
                contours: vec![broken],
            },
        ],
    };
    let text: SketchText = serde_json::from_value(serde_json::json!({
        "id": "t1", "text": "IJ", "at": [0.0, 0.0], "height": 2.0
    }))
    .unwrap();
    let outline = text::layout(&text, &glyphs, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    // The I bounds a profile; the J bounds none and is told, with where it
    // lies: one em (2 mm) high, half an em wide, right of the I.
    assert_eq!(outline.contours.len(), 1);
    assert_eq!(outline.region_curves().len(), 1);
    assert_eq!(outline.dropped.len(), 1);
    let j = &outline.dropped[0];
    assert_eq!((j.character, j.glyph, j.contour), ('J', 1, 0));
    assert!((j.high[0] - j.low[0] - 1.0).abs() < 1e-9, "{j:?}");
    assert!((j.high[1] - j.low[1] - 2.0).abs() < 1e-9, "{j:?}");
    let i_right = outline.contours[0]
        .parts
        .iter()
        .filter_map(|p| match p {
            Curve2::Line { a, b } => Some(a[0].max(b[0])),
            _ => None,
        })
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(j.low[0] >= i_right - 1e-9, "{j:?} {i_right}");
}
