// SPDX-License-Identifier: MIT
//! Extend to Meet (mitcad#2178): two curves (lines and arcs) extended or
//! shortened to the point where they meet, so that they end in one shared
//! corner point.
//!
//! The meeting points are found on the whole carriers: the endless line
//! through a line's ends and the whole circle of an arc (two lines: one
//! point; a line and an arc or two arcs: none, one where they touch, or
//! two). Each curve keeps the part that was picked, as Trim reads its pick:
//! the end on the meeting point's side of the pick moves, and an arc
//! reaches a meeting point in its gap either way round, the end nearer the
//! pick first. The results are ranked by how near their meeting point is to
//! the two picks (the sum of the distances; results at one point in the
//! order above), and `result` takes one of them, counted round, so that the
//! application's Tab steps through them. The application's tool computes
//! the same list for its preview (`app/sketch/ExtendToMeet.cpp`, which
//! follows this file formula for formula).

use std::f64::consts::TAU;

use super::{SketchEdit, TANGENT_JOIN, sketch_size};
use crate::geometry::{Curve2, P2, add, cross, dot, scale, sub};
use crate::ids::EntityUid;
use crate::{ConstraintKind, ConstraintUid, EntityKind, Ref};

/// One curve's part of a result: whether its end (else its start) moves
/// to the meeting point, and the curve as it is then.
#[derive(Debug, Clone, PartialEq)]
pub struct MeetEnd {
    pub moves_end: bool,
    pub curve: Curve2,
}

/// A way the two curves meet: the point and each curve after.
#[derive(Debug, Clone, PartialEq)]
pub struct MeetResult {
    pub point: P2,
    pub a: MeetEnd,
    pub b: MeetEnd,
}

/// Why two curves have no meeting point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoMeet {
    /// A curve is neither a line nor an arc (or a line without length).
    NotLineOrArc,
    /// Two parallel lines.
    Parallel,
    /// Two lines on one line: no single point.
    Collinear,
    /// A line that misses an arc's circle, or two circles apart (or one
    /// inside the other).
    Apart,
    /// Two arcs about one centre.
    Concentric,
    /// Meeting points there are, but at each one a curve would shrink to
    /// nothing (it would end where it starts, or an arc close).
    Collapses,
}

/// What `sketch.extend_to_meet` did: the shared corner point, how many
/// results there were and which one was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Met {
    pub point: EntityUid,
    pub results: usize,
    pub result: usize,
}

/// The endless carrier of a line or an arc.
#[derive(Debug, Clone, Copy)]
enum Carrier {
    /// Through `p` along the unit direction `d`.
    Line {
        p: P2,
        d: P2,
    },
    Circle {
        c: P2,
        r: f64,
    },
}

fn length(v: P2) -> f64 {
    v[0].hypot(v[1])
}

fn distance(a: P2, b: P2) -> f64 {
    length(sub(a, b))
}

/// An angle in `[0, 2π)`.
fn turn(angle: f64) -> f64 {
    angle.rem_euclid(TAU)
}

fn carrier(curve: &Curve2) -> Option<Carrier> {
    match *curve {
        Curve2::Line { a, b } => {
            let l = distance(a, b);
            (l > 0.0).then(|| Carrier::Line {
                p: a,
                d: scale(sub(b, a), 1.0 / l),
            })
        }
        Curve2::Arc { center, radius, .. } => Some(Carrier::Circle {
            c: center,
            r: radius,
        }),
        _ => None,
    }
}

/// The size the tolerances are taken of: one plus the largest coordinate
/// (an arc's centre's plus its radius) of the two curves.
fn meet_size(a: &Curve2, b: &Curve2) -> f64 {
    let extent = |c: &Curve2| match *c {
        Curve2::Line { a, b } => a[0].abs().max(a[1].abs()).max(b[0].abs()).max(b[1].abs()),
        Curve2::Arc { center, radius, .. } => center[0].abs().max(center[1].abs()) + radius,
        _ => 0.0,
    };
    1.0 + extent(a).max(extent(b))
}

/// Where a line through `p` along the unit `d` meets a circle: none, the
/// foot of the centre where it touches, or two points along `d`.
fn line_circle(p: P2, d: P2, c: P2, r: f64, size: f64) -> Result<Vec<P2>, NoMeet> {
    let foot = add(p, scale(d, dot(sub(c, p), d)));
    let h = distance(foot, c);
    let touch = 1e-6 * size;
    if h > r + touch {
        return Err(NoMeet::Apart);
    }
    if h >= r - touch {
        return Ok(vec![foot]);
    }
    let k = (r * r - h * h).sqrt();
    Ok(vec![sub(foot, scale(d, k)), add(foot, scale(d, k))])
}

/// Where the carriers meet.
fn carrier_meets(a: Carrier, b: Carrier, size: f64) -> Result<Vec<P2>, NoMeet> {
    match (a, b) {
        (Carrier::Line { p: p1, d: d1 }, Carrier::Line { p: p2, d: d2 }) => {
            let den = cross(d1, d2);
            if den.abs() <= 1e-12 {
                return Err(if cross(sub(p2, p1), d1).abs() <= 1e-9 * size {
                    NoMeet::Collinear
                } else {
                    NoMeet::Parallel
                });
            }
            let s = cross(sub(p2, p1), d2) / den;
            Ok(vec![add(p1, scale(d1, s))])
        }
        (Carrier::Line { p, d }, Carrier::Circle { c, r })
        | (Carrier::Circle { c, r }, Carrier::Line { p, d }) => line_circle(p, d, c, r, size),
        (Carrier::Circle { c: c1, r: r1 }, Carrier::Circle { c: c2, r: r2 }) => {
            let dv = sub(c2, c1);
            let d = length(dv);
            if d <= 1e-9 * size {
                return Err(NoMeet::Concentric);
            }
            let touch = 1e-6 * size;
            if d > r1 + r2 + touch || d < (r1 - r2).abs() - touch {
                return Err(NoMeet::Apart);
            }
            let along = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
            let m = add(c1, scale(dv, along / d));
            let h2 = r1 * r1 - along * along;
            if (d - (r1 + r2)).abs() <= touch || (d - (r1 - r2).abs()).abs() <= touch || h2 <= 0.0 {
                return Ok(vec![m]);
            }
            let h = h2.sqrt();
            let n = [-dv[1] / d, dv[0] / d];
            Ok(vec![sub(m, scale(n, h)), add(m, scale(n, h))])
        }
    }
}

/// The ways a curve can end at `x` keeping the part around `pick`: one
/// for a line (or none where it would shrink to nothing), one for a point
/// on an arc, and two for a point in an arc's gap (either way round, the
/// end nearer the pick first).
fn curve_ends(curve: &Curve2, pick: P2, x: P2, size: f64) -> Vec<MeetEnd> {
    match *curve {
        Curve2::Line { a, b } => {
            let l = distance(a, b);
            let u = scale(sub(b, a), 1.0 / l);
            let tp = dot(sub(pick, a), u).clamp(0.0, l);
            let tx = dot(sub(x, a), u);
            let least = 1e-9 * size;
            if tx >= tp {
                if tx > least {
                    return vec![MeetEnd {
                        moves_end: true,
                        curve: Curve2::Line { a, b: x },
                    }];
                }
            } else if l - tx > least {
                return vec![MeetEnd {
                    moves_end: false,
                    curve: Curve2::Line { a: x, b },
                }];
            }
            Vec::new()
        }
        Curve2::Arc {
            center,
            radius,
            start,
            end,
        } => {
            let span = end - start;
            let least = 1e-9;
            let angle = |p: P2| {
                let v = sub(p, center);
                turn(v[1].atan2(v[0]) - start)
            };
            let mut fp = angle(pick);
            if fp > span {
                fp = if fp - span < TAU - fp { span } else { 0.0 };
            }
            let mut fx = angle(x);
            if fx > TAU - least {
                fx = 0.0;
            }
            let arc = |s: f64, e: f64| Curve2::Arc {
                center,
                radius,
                start: s,
                end: e,
            };
            if fx <= span + least {
                if fx >= fp {
                    if fx > least {
                        return vec![MeetEnd {
                            moves_end: true,
                            curve: arc(start, start + fx),
                        }];
                    }
                } else if span - fx > least {
                    return vec![MeetEnd {
                        moves_end: false,
                        curve: arc(start + fx, end),
                    }];
                }
                return Vec::new();
            }
            let forward = MeetEnd {
                moves_end: true,
                curve: arc(start, start + fx),
            };
            let back = MeetEnd {
                moves_end: false,
                curve: arc(start + fx - TAU, end),
            };
            if span - fp <= fp {
                vec![forward, back]
            } else {
                vec![back, forward]
            }
        }
        _ => Vec::new(),
    }
}

/// Every way the curves `a` (picked at `at_a`) and `b` (at `at_b`) can
/// meet, nearest the picks first (see the module documentation).
pub fn meet_results(a: &Curve2, at_a: P2, b: &Curve2, at_b: P2) -> Result<Vec<MeetResult>, NoMeet> {
    let (Some(ca), Some(cb)) = (carrier(a), carrier(b)) else {
        return Err(NoMeet::NotLineOrArc);
    };
    let size = meet_size(a, b);
    let points = carrier_meets(ca, cb, size)?;
    let mut results = Vec::new();
    for x in points {
        for ea in curve_ends(a, at_a, x, size) {
            for eb in curve_ends(b, at_b, x, size) {
                results.push(MeetResult {
                    point: x,
                    a: ea.clone(),
                    b: eb,
                });
            }
        }
    }
    if results.is_empty() {
        return Err(NoMeet::Collapses);
    }
    // A stable sort: results at one point keep their order.
    let key = |r: &MeetResult| distance(r.point, at_a) + distance(r.point, at_b);
    results.sort_by(|p, q| key(p).total_cmp(&key(q)));
    Ok(results)
}

/// The direction of a solved line or arc at its point `x`.
fn direction_at(curve: &Curve2, x: P2) -> P2 {
    match *curve {
        Curve2::Line { a, b } => sub(b, a),
        Curve2::Arc { center, .. } => {
            let v = sub(x, center);
            [-v[1], v[0]]
        }
        _ => [0.0, 0.0],
    }
}

/// The refusal for curves that do not meet: a message in words, the
/// curves named in its detail for agents and the log.
fn refusal(why: NoMeet, a: EntityUid, b: EntityUid, lines: (bool, bool)) -> String {
    let named = |text: &str| format!("c{} and c{}: {text}", a.0, b.0);
    let said = match why {
        NoMeet::NotLineOrArc => "only lines and arcs can be extended to meet",
        NoMeet::Parallel => "the two lines are parallel and never meet",
        NoMeet::Collinear => "the two lines lie on one line and have no single point to meet at",
        NoMeet::Apart => match lines {
            (true, false) | (false, true) => "the line misses the arc's circle, so they never meet",
            _ => "the circles of the two arcs do not reach each other, so the arcs never meet",
        },
        NoMeet::Concentric => "the two arcs have the same centre and never meet",
        NoMeet::Collapses => {
            "the curves meet only where one of them would shrink to nothing or close on itself"
        }
    };
    crate::said(said, named(said))
}

impl SketchEdit<'_> {
    /// Extends or shortens the curves `a` and `b` (lines and arcs) to a
    /// point where they meet, the two ends becoming one point (Extend to
    /// Meet, mitcad#2178): of the results [`meet_results`] lists for the
    /// picks `at_a` and `at_b`, the one `result` names, counted round. The
    /// end that moves keeps its constraints and dimensions where this
    /// curve alone uses it (they measure the new place, as with Extend), a
    /// length or an arc length takes the new value (mitcad#362), a
    /// midpoint on either curve goes; where the curves meet tangentially, a
    /// `tangent` holds them when the sketch still solves with it. Refused
    /// in words for one curve picked twice, a curve that is no line or arc,
    /// curves that do not meet and ends that already meet there; projected
    /// and computed curves first need converting, and an offset's or a
    /// pattern's curve is first freed, as with Trim.
    pub fn extend_to_meet(
        &mut self,
        a: EntityUid,
        at_a: P2,
        b: EntityUid,
        at_b: P2,
        result: usize,
    ) -> Result<Met, String> {
        if a == b {
            return Err(crate::said(
                "pick two different curves to extend to meet",
                format!("c{} was picked twice", a.0),
            ));
        }
        let mut lines = (false, false);
        for (id, line) in [(a, &mut lines.0), (b, &mut lines.1)] {
            match &self.curve(id)?.kind {
                EntityKind::Line { .. } => *line = true,
                EntityKind::Arc { .. } => {}
                other => {
                    return Err(crate::said(
                        "only lines and arcs can be extended to meet",
                        format!("c{} is a {}, not a line or an arc", id.0, other.type_name()),
                    ));
                }
            }
        }
        // Offsets' curves are first freed from their sources (mitcad#1755),
        // patterns' from their patterns (mitcad#2007).
        self.free_offsets_of(&[a, b])?;
        self.free_patterns_of(&[a, b]);
        self.shape_editable(a)?;
        self.shape_editable(b)?;
        let solved = self.solved()?;
        let curve_of = |id: EntityUid| {
            solved
                .curves
                .get(&id)
                .cloned()
                .ok_or_else(|| format!("curve c{} does not exist", id.0))
        };
        let (ca, cb) = (curve_of(a)?, curve_of(b)?);
        let results =
            meet_results(&ca, at_a, &cb, at_b).map_err(|why| refusal(why, a, b, lines))?;
        let index = result % results.len();
        let chosen = results[index].clone();
        let x = chosen.point;
        let tol = 1e-9 * sketch_size(&solved.curves) * 10.0;
        let old_end = |id: EntityUid, moves_end: bool| -> Result<EntityUid, String> {
            let (s, e) = self.curve(id)?.kind.ends().ok_or("an open curve")?;
            Ok(if moves_end { e } else { s })
        };
        let old_a = old_end(a, chosen.a.moves_end)?;
        let old_b = old_end(b, chosen.b.moves_end)?;
        let at = |p: EntityUid| solved.points.get(&p).copied();
        if old_a == old_b && at(old_a).is_some_and(|p| distance(p, x) <= tol) {
            return Err(crate::said(
                "the two curves already meet there",
                format!("c{} and c{} already end at p{}", a.0, b.0, old_a.0),
            ));
        }
        // A midpoint on either curve: its middle moves.
        let midpoints: std::collections::BTreeSet<ConstraintUid> = self
            .def
            .constraints
            .iter()
            .filter(|c| {
                matches!(c.kind, ConstraintKind::Midpoint { curve, .. } if curve == a || curve == b)
            })
            .map(|c| c.id)
            .collect();
        self.drop_constraints(&midpoints);
        let shared: &[EntityUid] = if old_a == old_b { &[a, b] } else { &[a] };
        let pa = self.meet_end(shared, old_a, x, &solved, tol);
        let pb = if old_a == old_b {
            pa
        } else {
            self.meet_end(&[b], old_b, x, &solved, tol)
        };
        let corner = if pa == pb {
            pa
        } else if self.mergeable_point(pb) {
            self.merge_points(pa, pb)?;
            pa
        } else if self.mergeable_point(pa) {
            self.merge_points(pb, pa)?;
            pb
        } else {
            self.add_constraint(ConstraintKind::Coincident {
                point: pb,
                entity: Ref::Point(pa),
            });
            pa
        };
        // The dimensions on the corner and on the curves measure the new
        // place: a length or an arc length takes the new value.
        let mut after = solved.clone();
        for p in [pa, pb] {
            after.points.insert(p, x);
        }
        after.curves.insert(a, chosen.a.curve.clone());
        after.curves.insert(b, chosen.b.curve.clone());
        self.remeasure_dimensions(
            &[Ref::Point(pa), Ref::Point(pb), Ref::Curve(a), Ref::Curve(b)],
            &after,
        );
        // Meeting tangentially: held tangent, as a trim's joined end is
        // (mitcad#1220).
        let (da, db) = (
            direction_at(&chosen.a.curve, x),
            direction_at(&chosen.b.curve, x),
        );
        let touching = length(da) > 0.0
            && length(db) > 0.0
            && (cross(da, db) / (length(da) * length(db))).abs() <= TANGENT_JOIN;
        if touching && self.takes_tangent(a, b) && !self.tangent_holds(a, b, &[corner]) {
            self.add_tangent_if_safe(a, b);
        }
        Ok(Met {
            point: corner,
            results: results.len(),
            result: index,
        })
    }

    /// The point a curve's end `old` becomes at the meeting point `x`:
    /// `old` itself where it is there already, or moved there (keeping
    /// its constraints and dimensions) when only the `curves` use it and it
    /// may move; else a new point at `x` that takes its place in the
    /// curves, `old` going when nothing uses it any more.
    fn meet_end(
        &mut self,
        curves: &[EntityUid],
        old: EntityUid,
        x: P2,
        solved: &crate::solve::Solved,
        tol: f64,
    ) -> EntityUid {
        let there = solved
            .points
            .get(&old)
            .is_some_and(|p| distance(*p, x) <= tol);
        if there {
            return old;
        }
        let only_ours = self.users(old).iter().all(|u| curves.contains(u));
        let movable = only_ours
            && self.mergeable_point(old)
            && self.def.entity(old).is_some_and(|e| !e.fixed);
        if movable {
            if let Some(e) = self.def.entity_mut(old) {
                e.kind = EntityKind::Point { at: x };
            }
            return old;
        }
        let p = self.add_point(x);
        for c in curves {
            if let Some(e) = self.def.entity_mut(*c) {
                e.kind.replace_point(old, p);
            }
        }
        self.drop_if_unused(old);
        p
    }
}
