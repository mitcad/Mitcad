// SPDX-License-Identifier: MIT
//! The bead's plane geometry (mitcad#2645): every profile's two surfaces
//! a thickness apart everywhere (within 1e-9 mm), smooth and joined, its
//! refusals in words, centre lines joined into chains and the outline a
//! bead leaves on its face.

use super::*;

fn section(profile: BeadProfile, direction: BeadDirection, ends: BeadEnd) -> BeadSection {
    BeadSection {
        profile,
        width: 12.0,
        depth: 3.0,
        radius: 1.5,
        angle: 60f64.to_radians(),
        direction,
        ends,
        run_out: 8.0,
        thickness: 1.0,
    }
}

/// The distance from `p` to a piece of a curve.
fn distance(p: [f64; 2], piece: &Piece) -> f64 {
    match *piece {
        Piece::Line { a, b } => segment_distance(p, p, a, b),
        Piece::Arc { c, r, start, sweep } => {
            let angle = (p[1] - c[1]).atan2(p[0] - c[0]);
            let (lo, hi) = (start.min(start + sweep), start.max(start + sweep));
            let within = (0..3).any(|k| {
                let a = angle + (k as f64 - 1.0) * TAU;
                a >= lo - 1e-12 && a <= hi + 1e-12
            });
            if within {
                (len2(sub2(p, c)) - r).abs()
            } else {
                len2(sub2(p, piece.start())).min(len2(sub2(p, piece.end())))
            }
        }
    }
}

fn points(pieces: &[Piece]) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for piece in pieces {
        for k in 0..=64 {
            let f = k as f64 / 64.0;
            out.push(match *piece {
                Piece::Line { a, b } => add2(a, scale2(sub2(b, a), f)),
                Piece::Arc { c, r, start, sweep } => polar(c, r, start + f * sweep),
            });
        }
    }
    out
}

fn nearest(p: [f64; 2], curve: &[Piece]) -> f64 {
    curve
        .iter()
        .map(|piece| distance(p, piece))
        .fold(f64::INFINITY, f64::min)
}

/// The direction a piece runs in at its start and at its end.
fn tangents(piece: &Piece) -> ([f64; 2], [f64; 2]) {
    match *piece {
        Piece::Line { a, b } => (unit2(sub2(b, a)), unit2(sub2(b, a))),
        Piece::Arc { start, sweep, .. } => {
            let at = |a: f64| scale2([-a.sin(), a.cos()], sweep.signum());
            (at(start), at(start + sweep))
        }
    }
}

fn assert_smooth(curve: &[Piece]) {
    for pair in curve.windows(2) {
        let gap = len2(sub2(pair[0].end(), pair[1].start()));
        assert!(gap < 1e-9, "a gap of {gap} in {curve:?}");
        let (_, out) = tangents(&pair[0]);
        let (into, _) = tangents(&pair[1]);
        assert!(len2(sub2(out, into)) < 1e-9, "a corner in {curve:?}");
    }
}

#[test]
fn every_profile_keeps_the_thickness_across_the_bead() {
    for profile in [
        BeadProfile::Round,
        BeadProfile::U,
        BeadProfile::V,
        BeadProfile::Trapezoid,
    ] {
        for direction in [BeadDirection::In, BeadDirection::Out] {
            let mut values = section(profile, direction, BeadEnd::Round);
            if profile == BeadProfile::U {
                values.depth = 9.0;
            }
            let s = values.section().unwrap();
            assert_smooth(&s.face);
            assert_smooth(&s.other);
            assert_smooth(&s.face_half);
            // Each surface a thickness from the other, both ways.
            for p in points(&s.face) {
                let d = nearest(p, &s.other);
                assert!((d - 1.0).abs() < 1e-9, "{profile:?} {direction:?}: {d}");
            }
            for p in points(&s.other) {
                let d = nearest(p, &s.face);
                assert!((d - 1.0).abs() < 1e-9, "{profile:?} {direction:?}: {d}");
            }
            // Both leave their planes at the same distance from the centre
            // line, the face from its plane at 0, the other at the
            // thickness towards the material.
            let (first, last) = (s.face[0].start(), s.face.last().unwrap().end());
            assert!(len2(sub2(first, [-s.half, 0.0])) < 1e-9);
            assert!(len2(sub2(last, [s.half, 0.0])) < 1e-9);
            let level = match direction {
                BeadDirection::In => 1.0,
                BeadDirection::Out => -1.0,
            };
            assert_eq!(s.other_level, level);
            let back = s.other.last().unwrap().end();
            assert!(len2(sub2(back, [s.half, level])) < 1e-9, "{back:?}");
            // The halves start on the centre line.
            assert!(s.face_half[0].start()[0].abs() < 1e-9);
            assert!(s.other_half[0].start()[0].abs() < 1e-9);
            assert!(len2(sub2(s.face_half.last().unwrap().end(), last)) < 1e-9);
        }
    }
}

/// The band between the surfaces is the face's length times the thickness
/// (the curve turns as much one way as the other), and the whole section
/// has its centre of area on the centre line: a bead along a line adds
/// `t (length of the face - 2 half) L` to the sheet's volume, along an arc
/// of radius `R` by an angle `a` that times `R a / L`.
#[test]
fn the_band_between_the_surfaces_has_the_faces_length_times_the_thickness() {
    for profile in [BeadProfile::Round, BeadProfile::V, BeadProfile::Trapezoid] {
        for direction in [BeadDirection::In, BeadDirection::Out] {
            let s = section(profile, direction, BeadEnd::Round)
                .section()
                .unwrap();
            let area: f64 = s
                .face
                .iter()
                .map(|p| match *p {
                    Piece::Line { .. } => p.length(),
                    Piece::Arc { r, sweep, .. } => {
                        let other = r - s.material * sweep.signum();
                        sweep.abs() * (r * r - other * other).abs() / 2.0
                    }
                })
                .sum();
            let length: f64 = s.face.iter().map(Piece::length).sum();
            assert!((area - length).abs() < 1e-9, "{profile:?} {direction:?}");
            let moment: f64 = s.face.iter().map(|p| p.band_moment(1.0, s.material)).sum();
            assert!(moment.abs() < 1e-9, "{profile:?} {direction:?}: {moment}");
            let half: f64 = s
                .face_half
                .iter()
                .map(|p| p.band_moment(1.0, s.material))
                .sum();
            assert!(half > 0.0);
        }
    }
}

#[test]
fn a_round_bead_reaches_its_depth_and_its_lips_round_at_the_bend_radius() {
    let s = section(BeadProfile::Round, BeadDirection::In, BeadEnd::Round)
        .section()
        .unwrap();
    // The arc through the lips and (0, 3): its bottom on the centre line.
    assert!(len2(sub2(s.face_half[0].start(), [0.0, 3.0])) < 1e-9);
    // Pressed in, the face is the lips' outside: radius r + t on it, r on
    // the other surface.
    let Piece::Arc { r, .. } = s.face[0] else {
        panic!("{:?}", s.face[0]);
    };
    assert!((r - 2.5).abs() < 1e-12);
    let Piece::Arc { r, .. } = s.other[0] else {
        panic!("{:?}", s.other[0]);
    };
    assert!((r - 1.5).abs() < 1e-12);
    // Standing out, the other way round.
    let s = section(BeadProfile::Round, BeadDirection::Out, BeadEnd::Round)
        .section()
        .unwrap();
    let Piece::Arc { r, .. } = s.face[0] else {
        panic!("{:?}", s.face[0]);
    };
    assert!((r - 1.5).abs() < 1e-12);
}

#[test]
fn a_run_out_ramps_from_the_face_to_the_crown() {
    let s = section(BeadProfile::Trapezoid, BeadDirection::Out, BeadEnd::RunOut)
        .section()
        .unwrap();
    assert_smooth(&s.ramp);
    assert_smooth(&s.ramp_other);
    assert!(s.foot() > 0.0 && s.foot() < 8.0);
    assert!(s.crown() > 8.0);
    assert!((s.ramp.last().unwrap().end()[1] - 3.0).abs() < 1e-9);
    assert!((s.ramp[0].start()[1]).abs() < 1e-12);
    for p in points(&s.ramp) {
        assert!((nearest(p, &s.ramp_other) - 1.0).abs() < 1e-9);
    }
    assert_eq!(s.reach(), s.half.max(s.foot()));
}

#[test]
fn sizes_that_make_no_bead_are_refused_in_words() {
    let refused = |change: &dyn Fn(&mut BeadSection)| {
        let mut values = section(BeadProfile::Round, BeadDirection::In, BeadEnd::Round);
        change(&mut values);
        values.section().unwrap_err()
    };
    assert!(refused(&|s| s.depth = 7.0).contains("at most half as deep"));
    assert!(refused(&|s| s.width = 0.0).contains("above zero"));
    assert!(refused(&|s| s.radius = 0.0).contains("bend radius"));
    let crownless = refused(&|s| {
        s.profile = BeadProfile::Trapezoid;
        s.angle = 20f64.to_radians();
    });
    assert!(crownless.contains("crown has no width"), "{crownless}");
    let tight = refused(&|s| {
        s.profile = BeadProfile::V;
        s.radius = 20.0;
    });
    assert!(tight.contains("do not fit"), "{tight}");
    // A U as deep as its round bottom leaves no room for the lips' bends.
    let shallow_u = refused(&|s| s.profile = BeadProfile::U);
    assert!(shallow_u.contains("do not fit"), "{shallow_u}");
    let run_out = refused(&|s| {
        s.ends = BeadEnd::RunOut;
        s.run_out = 0.0;
    });
    assert!(run_out.contains("run-out"), "{run_out}");
    for message in [crownless, tight, run_out] {
        assert!(!message.chars().any(|c| c.is_ascii_digit()), "{message}");
    }
}

fn line(start: [f64; 2], end: [f64; 2]) -> BeadElement {
    BeadElement::Line { start, end }
}

#[test]
fn centre_lines_join_into_chains_in_their_running_order() {
    // An L given leg by leg, the second leg drawn towards the corner.
    let l = chains(&[
        line([0.0, 0.0], [30.0, 0.0]),
        line([30.0, 20.0], [30.0, 0.0]),
    ])
    .unwrap();
    assert_eq!(l.len(), 1);
    assert!(!l[0].closed);
    assert_eq!(l[0].elements[1], line([30.0, 0.0], [30.0, 20.0]));
    assert!((turn(&l[0].elements[0], &l[0].elements[1]) - FRAC_PI_2).abs() < 1e-12);
    // A whole circle is a chain of its own, closed; a square closes.
    let circle = BeadElement::Arc {
        center: [0.0, 0.0],
        radius: 10.0,
        start: 0.0,
        sweep: TAU,
    };
    let square = [
        line([0.0, 0.0], [10.0, 0.0]),
        line([10.0, 0.0], [10.0, 10.0]),
        line([10.0, 10.0], [0.0, 10.0]),
        line([0.0, 10.0], [0.0, 0.0]),
    ];
    let mut all = vec![circle];
    all.extend(square);
    let found = chains(&all).unwrap();
    assert_eq!(found.len(), 2);
    assert!(found[0].closed && found[1].closed);
    assert_eq!(found[1].elements.len(), 4);
    // Three pieces at a point branch.
    let branch = chains(&[
        line([0.0, 0.0], [10.0, 0.0]),
        line([10.0, 0.0], [20.0, 0.0]),
        line([10.0, 0.0], [10.0, 10.0]),
    ])
    .unwrap_err();
    assert!(branch.contains("branch"), "{branch}");
}

#[test]
fn the_outline_rounds_the_ends_and_the_outer_corner_and_cuts_back_the_inner() {
    let s = section(BeadProfile::Round, BeadDirection::In, BeadEnd::Round)
        .section()
        .unwrap();
    let bead = SheetBead {
        panel: PanelId {
            feature: "F3".parse().unwrap(),
            slot: 0,
        },
        side: SheetReferenceSide::Front,
        chains: chains(&[
            line([0.0, 0.0], [30.0, 0.0]),
            line([30.0, 0.0], [30.0, 20.0]),
        ])
        .unwrap(),
        section: section(BeadProfile::Round, BeadDirection::In, BeadEnd::Round),
    };
    let outline = bead.outline(&s).unwrap();
    let h = s.half;
    let lines: Vec<_> = outline
        .iter()
        .filter_map(|g| match g {
            SegmentGeometry::Line { start, end } => Some((*start, *end)),
            _ => None,
        })
        .collect();
    let arcs: Vec<_> = outline
        .iter()
        .filter_map(|g| match g {
            SegmentGeometry::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => Some((*center, *radius, end_angle - start_angle)),
            _ => None,
        })
        .collect();
    // Four offsets; the inner pair meets at (30 - h, h).
    assert_eq!(lines.len(), 4);
    let inner = [30.0 - h, h];
    assert_eq!(
        lines
            .iter()
            .filter(|(a, b)| len2(sub2(*a, inner)) < 1e-9 || len2(sub2(*b, inner)) < 1e-9)
            .count(),
        2
    );
    // Two half circles at the ends and a quarter at the outer corner.
    assert_eq!(arcs.len(), 3);
    assert!(arcs.iter().all(|(_, r, _)| (r - h).abs() < 1e-12));
    let quarter = arcs
        .iter()
        .find(|(c, _, _)| len2(sub2(*c, [30.0, 0.0])) < 1e-12)
        .unwrap();
    assert!((quarter.2 - FRAC_PI_2).abs() < 1e-12);
    assert_eq!(bead.centre_lines().len(), 2);
    // Too short a leg for the width at its inner side.
    let short = SheetBead {
        chains: chains(&[
            line([0.0, 0.0], [30.0, 0.0]),
            line([30.0, 0.0], [30.0, 2.0]),
        ])
        .unwrap(),
        ..bead
    };
    assert_eq!(short.outline(&s).unwrap_err(), SHORT_CORNER);
}
