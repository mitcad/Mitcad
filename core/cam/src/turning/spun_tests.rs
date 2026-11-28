// SPDX-License-Identifier: MIT
//! The spun target (mitcad#2629, GitHub mitcad/Mitcad#690) of meshes made
//! here: a turned shaft keeps the section's profile as it is, its fillet
//! an arc; a boss outside the section's plane widens the outside over its
//! Z to the farthest it reaches, whatever angle the setup turns it to; a
//! key reaching into a bore narrows the bore there and leaves the outside
//! as it is.

use std::f64::consts::{FRAC_PI_2, TAU};

use super::*;

const DEVIATION: f64 = 0.01;

/// A closed half section `(x, z)` (x a radius) turned about Z in `n`
/// steps starting at `turn` (radians), as a mesh.
fn revolved(section: &[[f64; 2]], n: usize, turn: f64) -> Vec<Triangle> {
    let at = |p: [f64; 2], k: usize| {
        let a = turn + TAU * k as f64 / n as f64;
        [p[0] * a.cos(), p[0] * a.sin(), p[1]]
    };
    let mut out = Vec::new();
    for i in 0..section.len() {
        let (p, q) = (section[i], section[(i + 1) % section.len()]);
        for k in 0..n {
            out.push([at(p, k), at(q, k), at(q, k + 1)]);
            out.push([at(p, k), at(q, k + 1), at(p, k + 1)]);
        }
    }
    out
}

/// A box's mesh, turned about Z by `turn` (radians).
fn block(lo: [f64; 3], hi: [f64; 3], turn: f64) -> Vec<Triangle> {
    let corner = |i: usize| -> [f64; 3] {
        let p: [f64; 3] = std::array::from_fn(|k| if (i >> k) & 1 == 1 { hi[k] } else { lo[k] });
        let (c, s) = (turn.cos(), turn.sin());
        [c * p[0] - s * p[1], s * p[0] + c * p[1], p[2]]
    };
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    quads
        .iter()
        .flat_map(|q| {
            [
                [corner(q[0]), corner(q[1]), corner(q[2])],
                [corner(q[0]), corner(q[2]), corner(q[3])],
            ]
        })
        .collect()
}

/// The largest (or smallest) radius a chain of lines has at `z`.
fn radius_at(chain: &[PV], z: f64, largest: bool) -> f64 {
    let mut best = if largest {
        f64::NEG_INFINITY
    } else {
        f64::INFINITY
    };
    for w in chain.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (lo, hi) = (a.z.min(b.z), a.z.max(b.z));
        if z < lo - 1e-9 || z > hi + 1e-9 {
            continue;
        }
        let x = if hi - lo < 1e-12 {
            if largest { a.x.max(b.x) } else { a.x.min(b.x) }
        } else {
            a.x + (z - a.z) / (b.z - a.z) * (b.x - a.x)
        };
        best = if largest { best.max(x) } else { best.min(x) };
    }
    best
}

/// A shaft 16 mm across to Z -20 with an R2 fillet at its face, then 30
/// mm across to Z -40: its outside as the section gives it, and its half
/// section closed along the axis.
fn shaft() -> (Vec<PV>, Vec<[f64; 2]>) {
    let outside = vec![
        PV::new(0.0, 0.0),
        // About (6, -2), counter-clockwise as the profile draws it (Z
        // right, X up): from 0 to 90 degrees round the centre, a round
        // on the corner (a negative bulge would hollow it out).
        PV::arc(6.0, 0.0, (std::f64::consts::PI / 8.0).tan()),
        PV::new(8.0, -2.0),
        PV::new(8.0, -20.0),
        PV::new(15.0, -20.0),
        PV::new(15.0, -40.0),
    ];
    // The fillet about (6, -2): from (6, 0) round to (8, -2).
    let mut section = vec![[0.0, 0.0]];
    for k in 0..=16 {
        let a = FRAC_PI_2 * k as f64 / 16.0;
        section.push([6.0 + 2.0 * a.sin(), -2.0 + 2.0 * a.cos()]);
    }
    section.extend([[8.0, -20.0], [15.0, -20.0], [15.0, -40.0], [0.0, -40.0]]);
    (outside, section)
}

#[test]
fn a_turned_part_keeps_its_profile() {
    let (outside, section) = shaft();
    let mesh = revolved(&section, 128, 0.3);
    let spun = spun(&outside, &[], &mesh, DEVIATION);
    assert!(!spun.changed);
    assert_eq!(spun.outside, outside);
    assert!(spun.inside.is_empty());
}

#[test]
fn a_boss_off_the_section_stays_inside_the_spun_target() {
    let (outside, section) = shaft();
    // A boss 4 wide reaching 14 from the axis along Y, between Z -15 and
    // -10: the section through the axis and X does not cut it.
    let boss = |turn: f64| block([-2.0, 7.0, -15.0], [2.0, 14.0, -10.0], turn);
    let farthest = 2.0_f64.hypot(14.0);
    let mut first: Option<Vec<PV>> = None;
    for turn in [0.0, 0.5, FRAC_PI_2, 2.0, 3.5, 5.9] {
        let mut mesh = revolved(&section, 128, 0.0);
        let boss_mesh = boss(turn);
        mesh.extend(boss_mesh.iter().copied());
        let spun = spun(&outside, &[], &mesh, DEVIATION);
        assert!(spun.changed);
        // Every corner of the boss lies within the spun outside.
        for t in &boss_mesh {
            for p in t {
                let r = p[0].hypot(p[1]);
                assert!(
                    r <= radius_at(&spun.outside, p[2], true) + 1e-9,
                    "turned {turn}: {p:?} outside {:?}",
                    spun.outside
                );
            }
        }
        // Over the boss's Z the outside is the farthest it reaches, and
        // the mesh's deviation.
        let at = radius_at(&spun.outside, -12.5, true);
        assert!((at - (farthest + DEVIATION)).abs() < 1e-6, "{at}");
        // Elsewhere the shaft's own diameters, from the face to the back.
        assert!((radius_at(&spun.outside, -5.0, true) - 8.0).abs() < 1e-6);
        assert!((radius_at(&spun.outside, -30.0, true) - 15.0).abs() < 1e-6);
        assert_eq!(spun.outside.first().map(|v| [v.x, v.z]), Some([0.0, 0.0]));
        assert_eq!(spun.outside.last().map(|v| [v.x, v.z]), Some([15.0, -40.0]));
        // Whatever the angle, the same target.
        match &first {
            None => first = Some(spun.outside.clone()),
            Some(f) => {
                assert_eq!(f.len(), spun.outside.len(), "{f:?} {:?}", spun.outside);
                for (a, b) in f.iter().zip(&spun.outside) {
                    assert!((a.x - b.x).abs() < 1e-6 && (a.z - b.z).abs() < 1e-6);
                }
            }
        }
    }
}

#[test]
fn a_key_in_the_bore_narrows_it() {
    // A tube 40 across with a bore 16 across, 30 long, and a key 2 wide
    // reaching into the bore to 5 from the axis between Z -20 and -10.
    let outside = vec![PV::new(8.0, 0.0), PV::new(20.0, 0.0), PV::new(20.0, -30.0)];
    let inside = vec![PV::new(8.0, 0.0), PV::new(8.0, -30.0)];
    let mut mesh = revolved(
        &[[8.0, 0.0], [20.0, 0.0], [20.0, -30.0], [8.0, -30.0]],
        128,
        0.0,
    );
    mesh.extend(block([-1.0, 5.0, -20.0], [1.0, 9.0, -10.0], 1.0));
    let spun = spun(&outside, &inside, &mesh, DEVIATION);
    assert!(spun.changed);
    assert_eq!(spun.outside, outside);
    let at = radius_at(&spun.inside, -15.0, false);
    assert!((at - (5.0 - DEVIATION)).abs() < 1e-6, "{:?}", spun.inside);
    assert!((radius_at(&spun.inside, -5.0, false) - 8.0).abs() < 1e-6);
    assert!((radius_at(&spun.inside, -25.0, false) - 8.0).abs() < 1e-6);
    assert_eq!(spun.inside.first().map(|v| [v.x, v.z]), Some([8.0, 0.0]));
    assert_eq!(spun.inside.last().map(|v| [v.x, v.z]), Some([8.0, -30.0]));
}
