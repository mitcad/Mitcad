// SPDX-License-Identifier: MIT
//! The geometry of a sketch fillet at a corner of a line, circular arc or
//! conic (mitcad#1192, mitcad#2255): which circle of a radius tangent to both
//! curves rounds the corner, and where it touches them. The sketch
//! edit ([`crate::modify`]) makes the entities and constraints.

use super::conic::ConicPiece;
use super::geometry::{Curve2, P2, add, cross, dist, dot, perp, scale, sub, unit};

/// The sine of the angle between two curves' directions at a corner at or
/// below which they are taken as tangent there (or as leaving it the same
/// way): no corner to round.
pub(crate) const TANGENT: f64 = 1e-7;

/// A curve as it leaves a corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Leg {
    /// A line from the corner along `dir` (a unit vector) for `len`.
    Line { dir: P2, len: f64 },
    /// A circular arc about `center` of `radius` that leaves the corner
    /// counter-clockwise when `ccw` (else clockwise), `sweep` radians long.
    Arc {
        center: P2,
        radius: f64,
        ccw: bool,
        sweep: f64,
    },
    /// A rational conic leaving the corner from its start or its end.
    Conic { piece: ConicPiece, from_start: bool },
}

/// The circle that rounds a corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fillet {
    pub center: P2,
    /// Where it touches the first and the second curve.
    pub touch: [P2; 2],
    /// How far from the corner along each curve it touches it: for conics
    /// its fraction of the original parameter span, otherwise mm.
    pub along: [f64; 2],
}

/// The path of a fillet's centre beside one curve: the curve offset by
/// the fillet's radius.
#[derive(Debug, Clone, Copy)]
enum CenterPath {
    Line { at: P2, dir: P2 },
    Circle { center: P2, radius: f64 },
}

impl Leg {
    /// The unit direction in which the curve leaves `corner`.
    pub(crate) fn direction(&self, corner: P2) -> P2 {
        match *self {
            Leg::Line { dir, .. } => dir,
            Leg::Arc { center, ccw, .. } => {
                let tangent = perp(unit(sub(corner, center)));
                if ccw { tangent } else { scale(tangent, -1.0) }
            }
            Leg::Conic { piece, from_start } => {
                let end = if from_start { piece.start } else { piece.end };
                unit(sub(piece.apex, end))
            }
        }
    }

    /// Its extent: for a conic the unit parameter span, otherwise mm.
    pub(crate) fn length(&self) -> f64 {
        match *self {
            Leg::Line { len, .. } => len,
            Leg::Arc { radius, sweep, .. } => radius * sweep,
            Leg::Conic { .. } => 1.0,
        }
    }

    /// Where a circle about `x` tangent to the curve touches it, and how
    /// far from `corner` along the curve that is (negative behind it).
    fn touch(&self, corner: P2, x: P2) -> (P2, f64) {
        match *self {
            Leg::Line { dir, .. } => {
                let along = dot(sub(x, corner), dir);
                (add(corner, scale(dir, along)), along)
            }
            Leg::Arc {
                center,
                radius,
                ccw,
                ..
            } => {
                let (from, to) = (sub(corner, center), sub(x, center));
                let at = add(center, scale(unit(to), radius));
                let turn = cross(from, to).atan2(dot(from, to));
                let turn = if ccw { turn } else { -turn };
                (at, turn * radius)
            }
            Leg::Conic { .. } => {
                unreachable!("conic offsets are intersected with their parameters")
            }
        }
    }

    /// The path of the centre of a circle of `r` tangent to the curve on
    /// the side of the unit normal `side` (at the corner): an arc's circle
    /// grows by `r` where the fillet lies outside it and shrinks where it
    /// lies inside it, which takes `r` less than its radius.
    fn center_path(&self, corner: P2, side: P2, r: f64) -> Option<CenterPath> {
        match *self {
            Leg::Line { dir, .. } => Some(CenterPath::Line {
                at: add(corner, scale(side, r)),
                dir,
            }),
            Leg::Arc { center, radius, .. } => {
                let inside = dot(sub(center, corner), side) > 0.0;
                let radius = if inside { radius - r } else { radius + r };
                (radius > 0.0).then_some(CenterPath::Circle { center, radius })
            }
            Leg::Conic { .. } => None,
        }
    }

    /// The leg's geometry relative to the corner, and whether its forward
    /// direction leaves the corner.
    fn curve(&self, corner: P2) -> (Curve2, bool) {
        match *self {
            Leg::Line { dir, len } => (
                Curve2::Line {
                    a: [0.0; 2],
                    b: scale(dir, len),
                },
                true,
            ),
            Leg::Arc {
                center,
                radius,
                ccw,
                sweep,
            } => {
                let radial = sub(corner, center);
                let angle = radial[1].atan2(radial[0]);
                let (start, end) = if ccw {
                    (angle, angle + sweep)
                } else {
                    (angle - sweep, angle)
                };
                (
                    Curve2::Arc {
                        center: sub(center, corner),
                        radius,
                        start,
                        end,
                    },
                    ccw,
                )
            }
            Leg::Conic { piece, from_start } => {
                let relative = ConicPiece {
                    start: sub(piece.start, corner),
                    apex: sub(piece.apex, corner),
                    end: sub(piece.end, corner),
                    rho: piece.rho,
                };
                (relative.curve(), from_start)
            }
        }
    }
}

/// Whether two curves that leave `corner` make a corner a fillet can
/// round: not tangent there, nor leaving it the same way.
pub(crate) fn is_corner(corner: P2, a: &Leg, b: &Leg) -> bool {
    cross(a.direction(corner), b.direction(corner)).abs() > TANGENT
}

/// The fillet of radius `r` at `corner` of the curves `a` and `b`: the
/// circle tangent to both on the side where they bound the corner
/// (between their directions, as for two lines), touching both ahead of
/// the corner, the one nearest to it. None when no such circle exists (an
/// arc's circle too small for the radius inside it, curves tangent at the
/// corner). Whether it fits within the curves' lengths is the caller's to
/// check against [`Leg::length`].
pub(crate) fn fillet_at(corner: P2, a: &Leg, b: &Leg, r: f64) -> Option<Fillet> {
    let (da, db) = (a.direction(corner), b.direction(corner));
    let turn = cross(da, db);
    if turn.abs() <= TANGENT {
        return None;
    }
    if matches!(a, Leg::Conic { .. }) || matches!(b, Leg::Conic { .. }) {
        let (center, touch, [u, v]) =
            super::offset::fillet_crossing(a.curve(corner), b.curve(corner), turn, r)?;
        return Some(Fillet {
            center: add(center, corner),
            touch: touch.map(|point| add(point, corner)),
            along: [u * a.length(), v * b.length()],
        });
    }
    // The normals of the two curves into the corner's side.
    let side_a = scale(perp(da), turn.signum());
    let side_b = scale(perp(db), -turn.signum());
    let path_a = a.center_path(corner, side_a, r)?;
    let path_b = b.center_path(corner, side_b, r)?;
    crossings(path_a, path_b)
        .into_iter()
        .filter_map(|x| {
            let (ta, la) = a.touch(corner, x);
            let (tb, lb) = b.touch(corner, x);
            (la > 0.0 && lb > 0.0).then_some(Fillet {
                center: x,
                touch: [ta, tb],
                along: [la, lb],
            })
        })
        .min_by(|p, q| (p.along[0] + p.along[1]).total_cmp(&(q.along[0] + q.along[1])))
}

/// Where two centre paths cross.
fn crossings(a: CenterPath, b: CenterPath) -> Vec<P2> {
    use CenterPath::{Circle, Line};
    match (a, b) {
        (Line { at: p, dir: d }, Line { at: q, dir: e }) => {
            let denom = cross(d, e);
            if denom.abs() <= 1e-12 {
                return Vec::new();
            }
            vec![add(p, scale(d, cross(sub(q, p), e) / denom))]
        }
        (Line { at, dir }, Circle { center, radius })
        | (Circle { center, radius }, Line { at, dir }) => {
            // at + s dir on the circle (dir is a unit vector).
            let w = sub(at, center);
            let half = dot(w, dir);
            let disc = half * half - (dot(w, w) - radius * radius);
            if disc < 0.0 {
                return Vec::new();
            }
            let root = disc.sqrt();
            vec![
                add(at, scale(dir, -half - root)),
                add(at, scale(dir, -half + root)),
            ]
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
        ) => {
            let d = dist(c1, c2);
            if d <= 1e-12 || d > r1 + r2 || d < (r1 - r2).abs() {
                return Vec::new();
            }
            let along = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
            let across = (r1 * r1 - along * along).max(0.0).sqrt();
            let u = scale(sub(c2, c1), 1.0 / d);
            let foot = add(c1, scale(u, along));
            vec![
                add(foot, scale(perp(u), across)),
                sub(foot, scale(perp(u), across)),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: P2, b: P2) -> bool {
        dist(a, b) < 1e-9
    }

    /// A line along -x from (10, 0) and the upper half circle of r 10 about
    /// the origin, counter-clockwise from (10, 0): a half disc's corner,
    /// rounded inside the circle.
    #[test]
    fn line_and_arc_inside_the_circle() {
        let corner = [10.0, 0.0];
        let line = Leg::Line {
            dir: [-1.0, 0.0],
            len: 20.0,
        };
        let arc = Leg::Arc {
            center: [0.0, 0.0],
            radius: 10.0,
            ccw: true,
            sweep: std::f64::consts::PI,
        };
        let f = fillet_at(corner, &line, &arc, 2.0).expect("a fillet");
        // Its centre 2 above the line, 8 from the origin.
        let x = 60f64.sqrt();
        assert!(close(f.center, [x, 2.0]), "{f:?}");
        assert!(close(f.touch[0], [x, 0.0]), "{f:?}");
        assert!(close(f.touch[1], scale([x, 2.0], 10.0 / 8.0)), "{f:?}");
        assert!((f.along[0] - (10.0 - x)).abs() < 1e-9, "{f:?}");
        // The same with the curves the other way round.
        let g = fillet_at(corner, &arc, &line, 2.0).expect("a fillet");
        assert!(close(g.center, f.center), "{g:?}");
        // A radius the circle cannot hold inside it.
        assert!(fillet_at(corner, &line, &arc, 10.0).is_none());
    }

    /// The line along +x from (10, 0) and the circle of r 10 about the
    /// origin leaving (10, 0) upwards: the corner lies outside the circle,
    /// so the fillet touches it from outside.
    #[test]
    fn line_and_arc_outside_the_circle() {
        let corner = [10.0, 0.0];
        let line = Leg::Line {
            dir: [1.0, 0.0],
            len: 20.0,
        };
        let arc = Leg::Arc {
            center: [0.0, 0.0],
            radius: 10.0,
            ccw: true,
            sweep: 1.0,
        };
        let f = fillet_at(corner, &line, &arc, 2.0).expect("a fillet");
        // 2 above the line, 12 from the origin.
        let x = 140f64.sqrt();
        assert!(close(f.center, [x, 2.0]), "{f:?}");
        assert!(close(f.touch[1], scale([x, 2.0], 10.0 / 12.0)), "{f:?}");
        assert!((f.along[0] - (x - 10.0)).abs() < 1e-9, "{f:?}");
    }

    /// Arcs of r 10 about (0, 0) and (10, 0) that bound the lens between
    /// the two circles: its top corner rounded inside both circles.
    #[test]
    fn two_arcs() {
        let corner = [5.0, 75f64.sqrt()];
        let left = Leg::Arc {
            center: [0.0, 0.0],
            radius: 10.0,
            ccw: false,
            sweep: 1.0,
        };
        let right = Leg::Arc {
            center: [10.0, 0.0],
            radius: 10.0,
            ccw: true,
            sweep: 1.0,
        };
        let f = fillet_at(corner, &left, &right, 1.0).expect("a fillet");
        // Inside both circles: 9 from both centres.
        assert!((dist(f.center, [0.0, 0.0]) - 9.0).abs() < 1e-9, "{f:?}");
        assert!((dist(f.center, [10.0, 0.0]) - 9.0).abs() < 1e-9, "{f:?}");
        assert!(f.center[1] < corner[1], "{f:?}");
        assert!(f.along[0] > 0.0 && f.along[1] > 0.0, "{f:?}");
    }

    #[test]
    fn conic_and_arc_and_two_conics_meet_on_their_exact_offsets() {
        let corner = [0.0, 0.0];
        let a = Leg::Conic {
            piece: ConicPiece {
                start: corner,
                apex: [10.0, 0.0],
                end: [20.0, 8.0],
                rho: 0.3,
            },
            from_start: true,
        };
        let conic = Leg::Conic {
            piece: ConicPiece {
                start: [8.0, 20.0],
                apex: [0.0, 10.0],
                end: corner,
                rho: 0.7,
            },
            from_start: false,
        };
        let arc = Leg::Arc {
            center: [-10.0, 0.0],
            radius: 10.0,
            ccw: true,
            sweep: 1.0,
        };
        for b in [arc, conic] {
            let f = fillet_at(corner, &a, &b, 1.0).expect("a fillet");
            for (leg, touch_index) in [a, b].into_iter().zip(0..2) {
                let (curve, forward) = leg.curve(corner);
                let (lo, hi) = curve.domain();
                let u = f.along[touch_index] / leg.length();
                let t = if forward {
                    lo + u * (hi - lo)
                } else {
                    hi - u * (hi - lo)
                };
                let [point, derivative, _] = curve.eval(t);
                let radial = sub(point, f.center);
                assert!(close(point, f.touch[touch_index]), "{f:?}");
                assert!((dist(point, f.center) - 1.0).abs() < 1e-9, "{f:?}");
                assert!(dot(unit(derivative), radial).abs() < 1e-9, "{f:?}");
            }
        }
    }

    #[test]
    fn tangent_curves_make_no_corner() {
        // A line running on into an arc tangent to it.
        let corner = [10.0, 0.0];
        let line = Leg::Line {
            dir: [0.0, -1.0],
            len: 5.0,
        };
        let arc = Leg::Arc {
            center: [0.0, 0.0],
            radius: 10.0,
            ccw: true,
            sweep: 1.0,
        };
        assert!(!is_corner(corner, &line, &arc));
        assert!(fillet_at(corner, &line, &arc, 1.0).is_none());
    }
}
