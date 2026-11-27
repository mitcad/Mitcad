// SPDX-License-Identifier: MIT
//! Finishing into undercuts (mitcad#2503, GitHub mitcad/Mitcad#707): the
//! contour a tool cutting towards the chuck can follow when it may go down
//! walls that fall towards the chuck, and how deep that goes under the
//! bridged contour.
//!
//! The [`envelope`](super::profile::envelope) bridges every hollow: the
//! tool never goes down. Here the contour follows the part down a wall as
//! long as the wall falls no steeper than `angle` from the Z axis; where it
//! falls steeper, the contour leaves it along a line at `angle` until that
//! line meets the part again, and what lies under the line is left. With
//! the angle a little below the tool's back angle the trailing edge clears
//! the wall it has cut: Turn Finishing's "Tool geometry". At 90 degrees the
//! contour follows every wall down ("Free": the user's tool must reach
//! it); at 0 it is the envelope ("Don't plunge").
//!
//! Every line at `angle` from a point of the contour lies above the lines
//! from the points before it (they are parallel, and each point is on or
//! above the lines before it), so one walk from the face towards the chuck
//! keeps only the last point's line. A wall leaning back towards the face
//! (an overhang) is never followed: it becomes a shoulder, as in the
//! envelope.

use super::PXZ;
use super::profile::{PV, Piece, chain_of, piece_at, pieces};

const EPS: f64 = 1e-9;

/// How many points between its ends an arc is measured at (besides its
/// quarter points, where it is deepest along X).
const ARC_SAMPLES: usize = 32;

/// The contour a tool cutting towards the chuck can follow when it may go
/// down walls no steeper than `angle` (degrees from the Z axis); `inside`
/// for a bore.
pub fn reach(chain: &[PV], inside: bool, angle: f64) -> Vec<PV> {
    if inside {
        // The bore is the outside seen in a mirror across the axis.
        return mirror(&reach(&mirror(chain), false, angle));
    }
    let free = angle >= 90.0 - 1e-9;
    let t = if free {
        f64::INFINITY
    } else {
        angle.max(0.0).to_radians().tan()
    };
    let ps = pieces(chain);
    let Some(first) = ps.first() else {
        return chain.to_vec();
    };
    let mut out: Vec<Piece> = Vec::new();
    let mut cur = first.a;
    // Whether the contour is on a line at `angle` from `cur` rather than
    // on the part.
    let mut bridging = false;
    for p in &ps {
        // What the piece holds towards the face of `cur` raises the
        // contour there (an overhang becomes a shoulder; a step up is
        // followed).
        let zc = cur[1];
        let mut deepest = cur[0];
        for q in [Some(p.a), Some(p.b), piece_at(p, 1, zc)]
            .into_iter()
            .flatten()
        {
            if q[1] >= zc - EPS {
                deepest = deepest.max(q[0]);
            }
        }
        if deepest > cur[0] + EPS {
            let to = [deepest, zc];
            line(&mut out, cur, to);
            cur = to;
            bridging = true;
        }
        if p.b[1] > p.a[1] + EPS {
            // Back towards the face: never followed.
            continue;
        }
        let mut q = *p;
        // A piece starting towards the face of `cur` (after an overhang)
        // is taken from where it crosses `cur`'s Z.
        if q.a[1] > cur[1] + EPS {
            if q.b[1] > cur[1] - EPS {
                continue;
            }
            match piece_at(&q, 1, cur[1]) {
                Some(a) => q.a = a,
                None => continue,
            }
        }
        let mut from_bridge_start = false;
        loop {
            if bridging {
                let emerge = if !from_bridge_start && on_or_above(q.a, cur, t) {
                    Some(q.a)
                } else {
                    meets(&q, cur, t, from_bridge_start)
                };
                let Some(e) = emerge else {
                    break;
                };
                line(&mut out, cur, e);
                cur = e;
                bridging = false;
                q.a = e;
                if same(q.a, q.b) {
                    break;
                }
            }
            match if free { None } else { too_steep(&q, t) } {
                None => {
                    if !same(q.a, q.b) {
                        out.push(q);
                    }
                    cur = q.b;
                    break;
                }
                Some(f) => {
                    if !same(q.a, f) {
                        out.push(Piece { b: f, ..q });
                    }
                    cur = f;
                    bridging = true;
                    from_bridge_start = true;
                    q.a = f;
                    if same(q.a, q.b) {
                        break;
                    }
                }
            }
        }
    }
    let end = ps.last().map_or(cur, |p| p.b);
    if bridging && end[1] < cur[1] - EPS {
        let x = if free {
            end[0]
        } else {
            cur[0] - t * (cur[1] - end[1])
        };
        line(&mut out, cur, [x.max(end[0]), end[1]]);
    }
    chain_of(&out)
}

/// How deep (radius) the contour `reached` goes under the contour
/// `bridged` (both from the face towards the chuck, `inside` for a bore),
/// and the point of `reached` where it is deepest: at each point, the
/// distance along X from it up to `bridged` at its Z, zero where it is on
/// it (a shoulder both have is no depth). Measured along curved spans
/// too, not only at the vertices: at each arc's quarter points (where it
/// is deepest along X), at points along it, and at the Z of the other
/// contour's vertices and arc points.
pub fn depth_under(bridged: &[PV], reached: &[PV], inside: bool) -> (f64, PXZ) {
    let s = if inside { -1.0 } else { 1.0 };
    let bp = pieces(bridged);
    let rp = pieces(reached);
    let mut zs: Vec<f64> = Vec::new();
    for p in bp.iter().chain(&rp) {
        zs.push(p.a[1]);
        zs.push(p.b[1]);
        if p.center.is_some() {
            for k in 1..ARC_SAMPLES {
                let u = k as f64 / ARC_SAMPLES as f64;
                zs.push(p.a[1] + u * (p.b[1] - p.a[1]));
            }
        }
    }
    let mut best = (0.0, rp.first().map_or([0.0, 0.0], |p| p.a));
    for p in &rp {
        let mut points = vec![p.a, p.b];
        points.extend(zs.iter().filter_map(|&z| piece_at(p, 1, z)));
        for q in points {
            let d = depth_at(&bp, q, s);
            if d > best.0 {
                best = (d, q);
            }
        }
    }
    best
}

/// The distance along X (away from the material, `s`) from `q` up to the
/// nearest of the pieces at its Z: zero on one of them, zero when none is
/// above it.
fn depth_at(pieces: &[Piece], q: PXZ, s: f64) -> f64 {
    let mut best: Option<f64> = None;
    for p in pieces {
        let d = if (p.a[1] - p.b[1]).abs() < EPS {
            // Along X at one Z (a wall): zero on it, else up to its foot.
            if (p.a[1] - q[1]).abs() > 1e-7 {
                continue;
            }
            let (lo, hi) = {
                let (u, v) = (s * p.a[0], s * p.b[0]);
                (u.min(v), u.max(v))
            };
            let x = s * q[0];
            if x > hi + 1e-7 {
                continue;
            }
            (lo - x).max(0.0)
        } else {
            let Some(b) = piece_at(p, 1, q[1]) else {
                continue;
            };
            let d = s * (b[0] - q[0]);
            if d < -1e-7 {
                continue;
            }
            d.max(0.0)
        };
        best = Some(best.map_or(d, |b| b.min(d)));
    }
    best.unwrap_or(0.0)
}

/// The chain mirrored across the spindle axis (a bore as an outside).
fn mirror(chain: &[PV]) -> Vec<PV> {
    chain
        .iter()
        .map(|v| PV {
            x: -v.x,
            z: v.z,
            bulge: -v.bulge,
        })
        .collect()
}

fn same(a: PXZ, b: PXZ) -> bool {
    (a[0] - b[0]).abs() < EPS && (a[1] - b[1]).abs() < EPS
}

fn line(out: &mut Vec<Piece>, a: PXZ, b: PXZ) {
    if !same(a, b) {
        out.push(Piece {
            a,
            b,
            center: None,
            ccw: false,
        });
    }
}

/// Whether `q` is on or above the line at slope `t` falling towards the
/// chuck from `from`.
fn on_or_above(q: PXZ, from: PXZ, t: f64) -> bool {
    if t.is_infinite() {
        // A line straight down: anything towards the chuck of it, or at
        // its top.
        return q[1] < from[1] - EPS || q[0] >= from[0] - EPS;
    }
    q[0] >= from[0] - t * (from[1] - q[1]) - 1e-7
}

/// The piece's direction at a point on it, as (dx, dz).
fn direction(p: &Piece, q: PXZ) -> (f64, f64) {
    match p.center {
        None => (p.b[0] - p.a[0], p.b[1] - p.a[1]),
        Some(c) => {
            // In the (z, x) plane a counter-clockwise tangent turns the
            // radius by +90°.
            let (rz, rx) = (q[1] - c[1], q[0] - c[0]);
            let (dz, dx) = (-rx, rz);
            if p.ccw { (dx, dz) } else { (-dx, -dz) }
        }
    }
}

/// How steeply the piece falls at a point, towards the chuck: the drop in
/// X per Z; infinite straight down; zero or less when it does not fall.
fn fall(p: &Piece, q: PXZ) -> f64 {
    let (dx, dz) = direction(p, q);
    if dx >= -EPS {
        return 0.0;
    }
    if dz > -EPS {
        return f64::INFINITY;
    }
    dx / dz
}

/// The first point along a piece going towards the chuck where it falls
/// steeper than `t`; none when it never does.
fn too_steep(p: &Piece, t: f64) -> Option<PXZ> {
    let steep = |q: PXZ| fall(p, q) > t + 1e-9;
    if steep(p.a) {
        return Some(p.a);
    }
    if !steep(p.b) {
        // A quarter arc turns one way: its fall changes monotonically.
        return None;
    }
    let c = p.center?;
    // Where the tangent falls at exactly `t`: the radius is square to the
    // direction (dz, dx) = (-1, -t).
    let r = (p.a[0] - c[0]).hypot(p.a[1] - c[1]);
    let n = (1.0 + t * t).sqrt();
    for (rz, rx) in [(t / n, -1.0 / n), (-t / n, 1.0 / n)] {
        let q = [c[0] + r * rx, c[1] + r * rz];
        if within(p, q) && (fall(p, q) - t).abs() < 1e-6 {
            return Some(q);
        }
    }
    Some(p.a)
}

/// Whether a point of a piece's circle (or line) lies on the piece: inside
/// the box of its ends, as a monotone piece has it.
fn within(p: &Piece, q: PXZ) -> bool {
    let (x0, x1) = (p.a[0].min(p.b[0]), p.a[0].max(p.b[0]));
    let (z0, z1) = (p.a[1].min(p.b[1]), p.a[1].max(p.b[1]));
    q[0] >= x0 - 1e-7 && q[0] <= x1 + 1e-7 && q[1] >= z0 - 1e-7 && q[1] <= z1 + 1e-7
}

/// The first point along the piece (the highest Z) where it comes up to
/// the line at slope `t` from `from`; past `from` itself when `strict`.
fn meets(p: &Piece, from: PXZ, t: f64, strict: bool) -> Option<PXZ> {
    let past = |q: PXZ| !strict || q[1] < from[1] - 1e-7 || (q[0] - from[0]).abs() > 1e-7;
    if t.is_infinite() {
        // A line straight down. A piece along it (a step up at its Z)
        // comes up to it at its top; another where it reaches its Z.
        if (p.a[1] - from[1]).abs() < EPS && (p.b[1] - from[1]).abs() < EPS {
            return (p.b[0] >= from[0] - EPS)
                .then_some(p.b)
                .filter(|q| past(*q));
        }
        return piece_at(p, 1, from[1]).filter(|q| past(*q));
    }
    // The line: x = m z + k.
    let (m, k) = (t, from[0] - t * from[1]);
    let mut found: Vec<PXZ> = Vec::new();
    match p.center {
        None => {
            let f = |q: PXZ| q[0] - (m * q[1] + k);
            let (fa, fb) = (f(p.a), f(p.b));
            if (fb - fa).abs() > EPS {
                let u = -fa / (fb - fa);
                if (-1e-9..=1.0 + 1e-9).contains(&u) {
                    found.push([
                        p.a[0] + u * (p.b[0] - p.a[0]),
                        p.a[1] + u * (p.b[1] - p.a[1]),
                    ]);
                }
            }
            if fb.abs() <= 1e-7 {
                found.push(p.b);
            }
        }
        Some(c) => {
            let r = (p.a[0] - c[0]).hypot(p.a[1] - c[1]);
            let (cx, cz) = (c[0], c[1]);
            let a = 1.0 + m * m;
            let b = 2.0 * (-cz + m * (k - cx));
            let cc = cz * cz + (k - cx) * (k - cx) - r * r;
            let disc = b * b - 4.0 * a * cc;
            if disc >= -1e-9 {
                let s = disc.max(0.0).sqrt();
                for z in [(-b + s) / (2.0 * a), (-b - s) / (2.0 * a)] {
                    let q = [m * z + k, z];
                    if within(p, q) {
                        found.push(q);
                    }
                }
            }
        }
    }
    found
        .into_iter()
        .filter(|q| past(*q))
        .max_by(|a, b| a[1].total_cmp(&b[1]))
}

#[cfg(test)]
mod tests {
    use super::super::profile::envelope;
    use super::*;

    fn pts(c: &[PV]) -> Vec<[f64; 2]> {
        c.iter()
            .map(|v| [(v.x * 1e6).round() / 1e6, (v.z * 1e6).round() / 1e6])
            .collect()
    }

    /// A 30 dia bar with a neck: down at 45° to 20 dia, along it, then
    /// straight up again.
    fn neck() -> Vec<PV> {
        vec![
            PV::new(15.0, 0.0),
            PV::new(15.0, -10.0),
            PV::new(10.0, -15.0),
            PV::new(10.0, -25.0),
            PV::new(15.0, -25.0),
            PV::new(15.0, -40.0),
        ]
    }

    #[test]
    fn at_zero_it_is_the_envelope() {
        assert_eq!(
            pts(&reach(&neck(), false, 0.0)),
            pts(&envelope(&neck(), false))
        );
    }

    #[test]
    fn free_follows_the_whole_neck() {
        assert_eq!(pts(&reach(&neck(), false, 90.0)), pts(&neck()));
    }

    #[test]
    fn a_wall_no_steeper_than_the_angle_is_followed() {
        // The 45° wall at 50°: followed to the bottom.
        assert_eq!(pts(&reach(&neck(), false, 50.0)), pts(&neck()));
    }

    #[test]
    fn a_steeper_wall_is_left_under_a_line_at_the_angle() {
        // At 30° the contour leaves the wall's top along a 30° line, which
        // meets the neck's floor (x 10) 8.66 further on, at z -18.66.
        let r = pts(&reach(&neck(), false, 30.0));
        let drop = 5.0 / 30f64.to_radians().tan();
        let z = ((-10.0 - drop) * 1e6_f64).round() / 1e6;
        assert_eq!(
            r,
            vec![
                [15.0, 0.0],
                [15.0, -10.0],
                [10.0, z],
                [10.0, -25.0],
                [15.0, -25.0],
                [15.0, -40.0]
            ]
        );
    }

    #[test]
    fn a_short_neck_leaves_the_line_on_the_far_wall() {
        // At 10° the line from the top (x 15, z -10) falls 2.64 over the
        // 15 to the far wall at z -25: it meets it at x 12.36.
        let r = pts(&reach(&neck(), false, 10.0));
        let x = ((15.0 - 15.0 * 10f64.to_radians().tan()) * 1e6_f64).round() / 1e6;
        assert_eq!(
            r,
            vec![
                [15.0, 0.0],
                [15.0, -10.0],
                [x, -25.0],
                [15.0, -25.0],
                [15.0, -40.0]
            ]
        );
    }

    #[test]
    fn a_ball_is_followed_down_to_where_it_falls_at_the_angle() {
        // A ball of radius 10 about (0, -10) from the face over its top
        // down to (6, -18), then a 4 dia neck and a 20 dia collar. From the
        // top the arc falls ever steeper: at 40° the contour leaves it
        // where its tangent falls at 40°, 130° round from the face, and
        // goes on in a straight line to the neck.
        let sweep = 6f64.atan2(-8.0);
        let ball = vec![
            PV::arc(0.0, 0.0, (sweep / 4.0).tan()),
            PV::new(6.0, -18.0),
            PV::new(2.0, -18.0),
            PV::new(2.0, -30.0),
            PV::new(10.0, -30.0),
            PV::new(10.0, -40.0),
        ];
        let r = reach(&ball, false, 40.0);
        let a = 130f64.to_radians();
        let tp = [10.0 * a.sin(), -10.0 + 10.0 * a.cos()];
        assert!(
            r.iter()
                .any(|v| (v.x - tp[0]).abs() < 1e-6 && (v.z - tp[1]).abs() < 1e-6),
            "{r:?}"
        );
        // On to the neck's floor along the 40° line.
        let z = tp[1] - (tp[0] - 2.0) / 40f64.to_radians().tan();
        assert!(
            r.iter()
                .any(|v| (v.x - 2.0).abs() < 1e-6 && (v.z - z).abs() < 1e-6),
            "{r:?}"
        );
        // Free follows the ball and the neck.
        let free = pts(&reach(&ball, false, 90.0));
        assert!(
            free.contains(&[2.0, -18.0]) && free.contains(&[2.0, -30.0]),
            "{free:?}"
        );
    }

    #[test]
    fn a_bore_is_the_mirror() {
        // A bore 20 dia with a recess 30 dia between a 45° wall and a
        // straight one.
        let bore = vec![
            PV::new(10.0, 0.0),
            PV::new(10.0, -5.0),
            PV::new(15.0, -10.0),
            PV::new(15.0, -20.0),
            PV::new(10.0, -20.0),
            PV::new(10.0, -30.0),
        ];
        assert_eq!(pts(&reach(&bore, true, 50.0)), pts(&bore));
        assert_eq!(pts(&reach(&bore, true, 0.0)), pts(&envelope(&bore, true)));
        // At 30° the recess's 45° wall is left under a 30° line from its
        // mouth, which meets the recess's floor 8.66 on.
        let drop = 5.0 / 30f64.to_radians().tan();
        let z = ((-5.0 - drop) * 1e6_f64).round() / 1e6;
        assert_eq!(
            pts(&reach(&bore, true, 30.0)),
            vec![
                [10.0, 0.0],
                [10.0, -5.0],
                [15.0, z],
                [15.0, -20.0],
                [10.0, -20.0],
                [10.0, -30.0]
            ]
        );
        // The recess is 5 deep (radius) under the bridged bore.
        let (d, at) = depth_under(&envelope(&bore, true), &reach(&bore, true, 90.0), true);
        assert!((d - 5.0).abs() < 1e-9 && at[0] > 14.9, "{d} at {at:?}");
    }

    /// The depth is measured along X up to the bridged contour at the same
    /// Z: the neck's floor is 5 under the bridge, and a shoulder both
    /// contours climb is no depth.
    #[test]
    fn the_depth_is_measured_up_to_the_bridge() {
        let (d, at) = depth_under(
            &envelope(&neck(), false),
            &reach(&neck(), false, 90.0),
            false,
        );
        assert!((d - 5.0).abs() < 1e-9, "{d}");
        assert!(
            (at[0] - 10.0).abs() < 1e-9 && at[1] <= -15.0 + 1e-9,
            "{at:?}"
        );
        // A pin stepping up from 10 to 15 at z -20: nothing under the
        // bridge, the shoulder's foot included.
        let pin = vec![
            PV::new(10.0, 0.0),
            PV::new(10.0, -20.0),
            PV::new(15.0, -20.0),
            PV::new(15.0, -30.0),
        ];
        let (d, _) = depth_under(&envelope(&pin, false), &reach(&pin, false, 90.0), false);
        assert!(d.abs() < 1e-9, "{d}");
    }

    /// A recess turned as one arc between two vertices (Ø30, 10 long, 2
    /// deep at its middle): no vertex is in it, yet its depth is the arc's
    /// 2 at z -15.
    #[test]
    fn an_arc_is_measured_between_its_vertices() {
        let recess = vec![
            PV::new(15.0, 0.0),
            PV::arc(15.0, -10.0, -0.4),
            PV::new(15.0, -20.0),
            PV::new(15.0, -30.0),
        ];
        let bridged = envelope(&recess, false);
        assert_eq!(pts(&bridged), vec![[15.0, 0.0], [15.0, -30.0]]);
        for angle in [50.0, 90.0] {
            let (d, at) = depth_under(&bridged, &reach(&recess, false, angle), false);
            assert!((d - 2.0).abs() < 1e-6, "{angle}: {d}");
            assert!(
                (at[0] - 13.0).abs() < 1e-6 && (at[1] + 15.0).abs() < 1e-6,
                "{angle}: {at:?}"
            );
        }
    }
}
