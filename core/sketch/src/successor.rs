// SPDX-License-Identifier: MIT
//! A linked projection of an edge that was merged with, or split from,
//! other edges on the same curve (mitcad#1784). A model change can make one
//! edge of several that lay on one curve (a disc's rim that the corners of
//! a prism touching it split into arcs, the corners moved off it) or split
//! one into several (a new face across it). The projection's source is
//! then gone, or its name finds the pieces in its place and nothing of its
//! curve's kind is like its curve. Its curve then follows the edges on the
//! same carrier, of its kind: a circle or arc about one centre (its radius
//! may change, as for a resize), a line on one line.
//!
//! - The edges (those that took a lost source's place, found by the model
//!   among the edges between the faces the source lay between, or the
//!   pieces its name finds now) must all lie on one carrier.
//! - Its used points (those a constraint, a dimension or a curve outside the
//!   projection needs) must have one place: the piece that holds all of its
//!   used ends, of those the one that holds most of its ends.
//! - A line or an arc that lies within that piece keeps its span where it
//!   was when an end of it is used, and becomes the piece otherwise; one
//!   that does not lie within it becomes the piece. An arc whose ends
//!   nothing uses keeps its span on the carrier where several pieces would
//!   do (its centre has one place).
//! - An arc of what is now a whole circle becomes that circle when nothing
//!   holds it that a circle could not hold (mitcad#1794, `takes_circle`):
//!   its ends are used by nothing, and only constraints and dimensions that
//!   hold a circle as an arc are on it (no arc length, no pattern, offset or
//!   text). Its kind changes while it follows, its id and its centre kept;
//!   its ends stay in the evaluated sketch as loose points on the circle,
//!   so that it pairs entity for entity with the stored one (the solve's
//!   steps, the moved entities), and the sketch as it evaluated
//!   ([`crate::project::with_moved`]: what queries show and edits start
//!   from) takes the circle and leaves its ends out. Otherwise it keeps its
//!   span: an end used, or something on it that needs an arc.
//! - A circle follows the circle through the pieces.
//!
//! Otherwise nothing follows: a lost source keeps its last geometry and
//! warns as before, with the edges that could have taken its place for
//! Project Again ([`crate::project::ProjectionFollow::candidates`]).

use std::collections::BTreeSet;
use std::f64::consts::TAU;

use super::geometry::dot as dot2;
use super::geometry::{Curve2, P2, add, angle_after, cross, dist, norm, scale, sub, unit};
use super::project::{Projected, distinct, project};
use super::{Entity, EntityKind, Ref};
use crate::def::SketchDef;
use crate::ids::EntityUid;
use crate::kernel::Curve3;
use crate::profile::SketchFrame;

/// An edge that may have taken the place of a linked projection's source:
/// its name and its curves, in the sketch's component's coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Successor {
    pub name: String,
    pub curves: Vec<Curve3>,
}

/// What [`follow`] found.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The projection follows: the new kinds of its entities (points
    /// included, the old ids kept), the name of the edge it follows, and
    /// the edges on the carrier as projected.
    Followed {
        kinds: Vec<(EntityUid, EntityKind)>,
        name: String,
        current: Vec<Projected>,
    },
    /// Nothing follows: the edges of its curve's kind that could have
    /// taken its place (none, or several that leave its curve no one
    /// place), each with its curves as projected.
    Open(Vec<(String, Vec<Projected>)>),
}

/// How far apart, relative to the size of the geometry, curves may be and
/// still lie on one carrier (as [`crate::project`] matches a projected
/// curve with its source's).
const MATCH: f64 = 1e-6;

/// A piece on the carrier: the name of its edge and its curve.
struct Piece {
    name: String,
    curve: Curve2,
}

/// The projection's curve as it was.
enum Was {
    Line {
        start: EntityUid,
        end: EntityUid,
        a: P2,
        b: P2,
    },
    /// Counter-clockwise from `from` to `to` (radians about its centre).
    Arc {
        id: EntityUid,
        center: EntityUid,
        start: EntityUid,
        end: EntityUid,
        from: f64,
        to: f64,
    },
    Circle {
        id: EntityUid,
        center: EntityUid,
        start: f64,
    },
}

impl Was {
    fn of(e: &Entity, at: &dyn Fn(EntityUid) -> Option<P2>) -> Option<Self> {
        Some(match e.kind {
            EntityKind::Line { start, end, .. } => Self::Line {
                start,
                end,
                a: at(start)?,
                b: at(end)?,
            },
            EntityKind::Arc { center, start, end } => {
                let (c, s, t) = (at(center)?, at(start)?, at(end)?);
                let from = angle(c, s);
                Self::Arc {
                    id: e.id,
                    center,
                    start,
                    end,
                    from,
                    to: angle_after(from, angle(c, t)),
                }
            }
            EntityKind::Circle { center, start, .. } => Self::Circle {
                id: e.id,
                center,
                start,
            },
            _ => return None,
        })
    }

    /// Whether a curve is of its kind: a line for a line, a circle or an
    /// arc for an arc or a circle.
    fn takes(&self, curve: &Curve2) -> bool {
        match self {
            Self::Line { .. } => matches!(curve, Curve2::Line { .. }),
            Self::Arc { .. } | Self::Circle { .. } => {
                matches!(curve, Curve2::Circle { .. } | Curve2::Arc { .. })
            }
        }
    }
}

/// The angle of `p` about `c`.
fn angle(c: P2, p: P2) -> f64 {
    (p[1] - c[1]).atan2(p[0] - c[0])
}

/// The point of the circle about `c` of radius `r` at `angle`.
fn on_circle(c: P2, r: f64, angle: f64) -> P2 {
    [c[0] + r * angle.cos(), c[1] + r * angle.sin()]
}

/// One line, or one circle, that pieces lie on.
#[derive(Clone, Copy)]
enum Carrier {
    Line { at: P2, along: P2 },
    Round { center: P2, radius: f64 },
}

impl Carrier {
    fn of_curve(curve: &Curve2) -> Option<Self> {
        match *curve {
            Curve2::Line { a, b } => (dist(a, b) > 0.0).then(|| Self::Line {
                at: a,
                along: unit(sub(b, a)),
            }),
            Curve2::Circle { center, radius, .. } | Curve2::Arc { center, radius, .. } => {
                Some(Self::Round { center, radius })
            }
            _ => None,
        }
    }

    /// The carrier every piece lies on (within `tol`); None when they lie
    /// on several.
    fn of(pieces: &[Piece], tol: f64) -> Option<Self> {
        let first = Self::of_curve(&pieces.first()?.curve)?;
        pieces
            .iter()
            .all(|p| first.holds(&p.curve, tol))
            .then_some(first)
    }

    fn holds(&self, curve: &Curve2, tol: f64) -> bool {
        match (*self, curve) {
            (Self::Line { at, along }, Curve2::Line { a, b }) => [*a, *b]
                .iter()
                .all(|p| cross(sub(*p, at), along).abs() <= tol),
            (
                Self::Round { center, radius },
                Curve2::Circle {
                    center: c,
                    radius: r,
                    ..
                }
                | Curve2::Arc {
                    center: c,
                    radius: r,
                    ..
                },
            ) => dist(center, *c) <= tol && (radius - *r).abs() <= tol,
            _ => false,
        }
    }
}

/// The points of `own` (a projection's entities) that the rest of the
/// sketch uses: its constraints, its dimensions and the curves outside
/// `own` that end on them.
fn used_points(def: &SketchDef, own: &BTreeSet<EntityUid>) -> BTreeSet<EntityUid> {
    let mut out: BTreeSet<EntityUid> = BTreeSet::new();
    for c in &def.constraints {
        out.extend(c.kind.refs().iter().map(|r| r.uid()));
    }
    for d in &def.dimensions {
        out.extend(d.kind.refs().iter().map(|r| r.uid()));
    }
    for e in def.entities.iter().filter(|e| !own.contains(&e.id)) {
        out.extend(e.kind.points());
    }
    out.retain(|id| own.contains(id));
    out
}

/// Whether the arc `arc` (its ends `ends`) may become a whole circle
/// (mitcad#1794): no pattern, offset or text holds it or its ends, and the
/// constraints and dimensions on it all hold a circle as they hold an arc
/// (as Project Again carries them over to a circle,
/// [`crate::again::round_constraint`]). Its ends' other uses are the
/// caller's ([`used_points`]).
fn takes_circle(def: &SketchDef, arc: EntityUid, ends: [EntityUid; 2]) -> bool {
    let ids = [arc, ends[0], ends[1]];
    let on = |refs: &[Ref]| refs.iter().any(|r| ids.contains(&r.uid()));
    let on_arc = |refs: &[Ref]| refs.iter().any(|r| r.uid() == arc);
    let constraints = def
        .constraints
        .iter()
        .all(|c| !on_arc(&c.kind.refs()) || crate::again::round_constraint(&c.kind));
    let dimensions = def
        .dimensions
        .iter()
        .all(|d| !on_arc(&d.kind.refs()) || crate::again::round_dimension(&d.kind));
    let patterns = def.patterns.iter().all(|p| {
        let mut refs: Vec<Ref> = p.entities.clone();
        for copy in &p.copies {
            refs.extend(copy.entities.iter().flat_map(|(a, b)| [*a, *b]));
        }
        if let crate::pattern::PatternKind::Circular { center, .. } = &p.kind {
            refs.push(Ref::Point(*center));
        }
        !on(&refs)
    });
    let offsets = def.offsets.iter().all(|o| {
        !o.curves
            .iter()
            .chain(&o.results)
            .chain(&o.helpers)
            .any(|c| ids.contains(c))
    });
    let texts = def.texts.iter().all(|t| {
        !t.frame.iter().any(|p| ids.contains(p)) && t.path.is_none_or(|p| !ids.contains(&p.curve))
    });
    constraints && dimensions && patterns && offsets && texts
}

/// The pieces that hold every used end of the old curve (`ends`, each its
/// id and its place along the carrier), of those the ones that hold most
/// of its ends, and how many they hold.
fn pick(
    count: usize,
    ends: &[(EntityUid, f64)],
    used: &BTreeSet<EntityUid>,
    holds: &dyn Fn(usize, f64) -> bool,
) -> (Vec<usize>, usize) {
    let mut best: Vec<usize> = Vec::new();
    let mut most = 0;
    for k in 0..count {
        if ends
            .iter()
            .any(|(id, at)| used.contains(id) && !holds(k, *at))
        {
            continue;
        }
        let held = ends.iter().filter(|(_, at)| holds(k, *at)).count();
        if best.is_empty() || held > most {
            best = vec![k];
            most = held;
        } else if held == most {
            best.push(k);
        }
    }
    (best, most)
}

/// How a linked projection of an edge (`old`: its entities, their points'
/// places from `old_at`) follows the edges that may have taken its
/// source's place (`candidates`), as the module documentation says. With
/// `strict` (the pieces its own name finds), every curve of them must be of
/// its curve's kind. `def` is the sketch, for the points it uses.
pub fn follow(
    def: &SketchDef,
    old: &[Entity],
    old_at: &dyn Fn(EntityUid) -> Option<P2>,
    candidates: &[Successor],
    frame: &SketchFrame,
    strict: bool,
) -> Outcome {
    let none = || Outcome::Open(Vec::new());
    // Its one curve.
    let mut curves = old.iter().filter(|e| !e.is_point());
    let (Some(curve), None) = (curves.next(), curves.next()) else {
        return none();
    };
    let Some(was) = Was::of(curve, old_at) else {
        return none();
    };
    // The candidates' curves of its kind.
    let mut pieces: Vec<Piece> = Vec::new();
    let mut listed: Vec<(String, Vec<Projected>)> = Vec::new();
    for candidate in candidates {
        let projected = distinct(project(&candidate.curves, frame));
        let mut own: Vec<Curve2> = Vec::new();
        for p in &projected {
            match p {
                Projected::Curve(c) if was.takes(c) => own.push(c.clone()),
                Projected::Curve(_) if strict => return none(),
                _ => {}
            }
        }
        if own.is_empty() {
            continue;
        }
        listed.push((candidate.name.clone(), projected));
        pieces.extend(own.into_iter().map(|curve| Piece {
            name: candidate.name.clone(),
            curve,
        }));
    }
    if pieces.is_empty() {
        return none();
    }
    let mut size = 1.0_f64;
    for e in old {
        if let EntityKind::Point { at } = e.kind {
            size = size.max(norm(at));
        }
    }
    for p in &pieces {
        let (lo, hi) = p.curve.bounds();
        size = size.max(norm(lo)).max(norm(hi));
    }
    let tol = MATCH * size;
    let Some(carrier) = Carrier::of(&pieces, tol) else {
        return Outcome::Open(listed);
    };
    let own: BTreeSet<EntityUid> = old.iter().map(|e| e.id).collect();
    let used = used_points(def, &own);
    let current: Vec<Projected> = pieces
        .iter()
        .map(|p| Projected::Curve(p.curve.clone()))
        .collect();
    let followed = |kinds: Vec<(EntityUid, EntityKind)>, k: usize| Outcome::Followed {
        kinds,
        name: pieces[k].name.clone(),
        current: current.clone(),
    };
    match (was, carrier) {
        (Was::Circle { id, center, start }, Carrier::Round { center: c, radius }) => {
            // The circle through the pieces; a whole one names it.
            let k = pieces
                .iter()
                .position(|p| matches!(p.curve, Curve2::Circle { .. }))
                .unwrap_or(0);
            followed(
                vec![
                    (
                        id,
                        EntityKind::Circle {
                            center,
                            radius,
                            start,
                        },
                    ),
                    (center, EntityKind::Point { at: c }),
                ],
                k,
            )
        }
        (Was::Line { start, end, a, b }, Carrier::Line { at, along }) => {
            let along_line = |p: P2| dot2(sub(p, at), along);
            let ends = [(start, along_line(a)), (end, along_line(b))];
            let holds = |k: usize, t: f64| match pieces[k].curve {
                Curve2::Line { a: p, b: q } => {
                    let (lo, hi) = (along_line(p), along_line(q));
                    t >= lo.min(hi) - tol && t <= lo.max(hi) + tol
                }
                _ => false,
            };
            let (best, most) = pick(pieces.len(), &ends, &used, &holds);
            let [k] = best.as_slice() else {
                return Outcome::Open(listed);
            };
            let used_end = ends.iter().any(|(id, _)| used.contains(id));
            let (p, q) = if most == 2 && used_end {
                // Its span where it was, on the line.
                (
                    add(at, scale(along, ends[0].1)),
                    add(at, scale(along, ends[1].1)),
                )
            } else {
                let Curve2::Line { a: p, b: q } = pieces[*k].curve else {
                    return Outcome::Open(listed);
                };
                // The way round it ran.
                if dist(a, p) + dist(b, q) <= dist(a, q) + dist(b, p) {
                    (p, q)
                } else {
                    (q, p)
                }
            };
            followed(
                vec![
                    (start, EntityKind::Point { at: p }),
                    (end, EntityKind::Point { at: q }),
                ],
                *k,
            )
        }
        (
            Was::Arc {
                id,
                center,
                start,
                end,
                from,
                to,
            },
            Carrier::Round { center: c, radius },
        ) => {
            let turn = tol / radius.max(tol).max(f64::MIN_POSITIVE);
            // A piece's span; None for a whole circle.
            let span = |k: usize| match pieces[k].curve {
                Curve2::Arc { start, end, .. } => Some((start, end)),
                _ => None,
            };
            let holds = |k: usize, t: f64| match span(k) {
                None => true,
                Some((s, e)) => {
                    let d = angle_after(s, t) - s;
                    d <= e - s + turn || TAU - d <= turn
                }
            };
            let ends = [(start, from), (end, to)];
            let (best, most) = pick(pieces.len(), &ends, &used, &holds);
            let used_end = ends.iter().any(|(id, _)| used.contains(id));
            let middle = from + 0.5 * (to - from);
            // The piece, and whether the arc keeps its span on it.
            let (k, keep) = match best.as_slice() {
                [k] => {
                    let within = most == 2 && holds(*k, middle);
                    (*k, span(*k).is_none() || (within && used_end))
                }
                // Its centre has one place: its span on the carrier, named
                // by a whole circle where there is one.
                [first, ..] if !used_end => (
                    best.iter()
                        .copied()
                        .find(|k| span(*k).is_none())
                        .unwrap_or(*first),
                    true,
                ),
                _ => return Outcome::Open(listed),
            };
            // An arc of what is now one whole circle, its ends used by
            // nothing: that circle (mitcad#1794), its ends left on it.
            if span(k).is_none() && !used_end && takes_circle(def, id, [start, end]) {
                return followed(
                    vec![
                        (
                            id,
                            EntityKind::Circle {
                                center,
                                radius,
                                start: from,
                            },
                        ),
                        (center, EntityKind::Point { at: c }),
                        (
                            start,
                            EntityKind::Point {
                                at: on_circle(c, radius, from),
                            },
                        ),
                        (
                            end,
                            EntityKind::Point {
                                at: on_circle(c, radius, to),
                            },
                        ),
                    ],
                    k,
                );
            }
            let (from, to) = match (keep, span(k)) {
                (false, Some(piece)) => piece,
                _ => (from, to),
            };
            followed(
                vec![
                    (center, EntityKind::Point { at: c }),
                    (
                        start,
                        EntityKind::Point {
                            at: on_circle(c, radius, from),
                        },
                    ),
                    (
                        end,
                        EntityKind::Point {
                            at: on_circle(c, radius, to),
                        },
                    ),
                ],
                k,
            )
        }
        _ => Outcome::Open(listed),
    }
}
