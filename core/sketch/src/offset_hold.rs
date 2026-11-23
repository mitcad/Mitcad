// SPDX-License-Identifier: MIT
//! Points of the sketch held on derived offsets (mitcad#2278).
//!
//! A derived offset (of a chain with a spline or an ellipse,
//! [`crate::offset`]) is computed from its solved source after the solve;
//! its curves and points are not solver unknowns. A Coincident of a point of
//! the sketch with one of its curves or points is solved as the exact
//! offset of the source curve instead, `p = S(t) + d n(t)` (the solver's
//! `PointOnOffset`), with `d` the offset's distance read for the solve (a
//! known value) and `n` the source curve's unit normal on the offset's side:
//!
//! - on a result curve: `t` free within what is kept of its source curve
//!   (the part the point is on, where the curve is cut at a neighbour's
//!   offset or where its offset folds over and vanishes, mitcad#1234,
//!   mitcad#1938);
//! - at a result's end where nothing follows it (the outer end of an open
//!   chain or of an open outline, after what is cut and dropped), at a
//!   round corner's end or at a smooth join: `t` at that end of the source
//!   curve;
//! - on a round corner arc: at the distance from the source's corner point,
//!   between the offset directions of the two curves there (the solver's
//!   `PointOnOffsetCorner`);
//! - at the centre of a result arc or of a round corner arc: at the source
//!   arc's centre or the source's corner point (a plain Coincident).
//!
//! A trimmed inner corner (where two results cross and are cut) moves along
//! both as the source changes and is refused in words, as are the inner
//! control points of a result spline and every other constraint or
//! dimension on what an offset computes.

use std::collections::{BTreeMap, BTreeSet};

use mitcad_solver::{Constraint as S, EntityId, OffsetAt, PointId};

use super::geometry::{Curve2, P2, add, cross, dist, norm, perp, scale, sub};
use super::offset::{Computed, Join, Made, SketchOffset, chain_directions, compute_stored, linked};
use super::{ConstraintKind, Entity, EntityIndex, EntityKind, Ref};
use crate::ids::EntityUid;
use crate::parameters::ParamId;

/// A derived offset as a solve takes it: its record and its distance as the
/// parameter gives it now (millimetres, not negative; the record's `left`
/// tells the side).
#[derive(Debug, Clone, Copy)]
pub struct DerivedOffset<'a> {
    pub offset: &'a SketchOffset<ParamId>,
    pub distance: f64,
}

/// One derived offset computed where the solve starts.
struct Now<'a> {
    offset: &'a SketchOffset<ParamId>,
    computed: Computed,
    /// Each source curve's direction along the chain.
    forward: Vec<bool>,
    /// Each source curve's geometry where the solve starts.
    sources: Vec<Curve2>,
    /// The signed distance, to the right of the chain.
    d: f64,
}

/// The derived offsets of a sketch where its solve starts, to hold points
/// on.
pub(crate) struct Holds<'a> {
    offsets: Vec<Now<'a>>,
    index: EntityIndex<'a>,
}

/// The ids a hold is written with: the solver's point and curve of a
/// sketch point or curve, and where a point is when the solve starts.
pub(crate) struct Ids<'a> {
    pub point: &'a dyn Fn(EntityUid) -> Result<PointId, String>,
    pub curve: &'a dyn Fn(EntityUid) -> Result<EntityId, String>,
    pub at: &'a dyn Fn(EntityUid) -> Option<P2>,
}

/// A hold's solver constraint, and where it puts the point when the solve
/// starts.
type Held = (S, Option<P2>);

/// Why a constraint or dimension on what a derived offset computes is
/// refused (mitcad#2278): all but a Coincident of a point of the sketch.
pub(crate) fn refusal(id: &dyn std::fmt::Display, r: Ref) -> String {
    format!(
        "{id}: {r} is computed from an offset of a spline or an ellipse: it takes only a \
         Coincident of a point of the sketch (on its curve or at its end); constrain its source \
         for the rest"
    )
}

impl<'a> Holds<'a> {
    /// Computes each derived offset of `offsets` from its sources as `curve`
    /// has them where the solve starts.
    pub(crate) fn new(
        offsets: &[DerivedOffset<'a>],
        entities: &'a [Entity],
        curve: &dyn Fn(EntityUid) -> Option<Curve2>,
    ) -> Result<Self, String> {
        let index = EntityIndex::new(entities);
        let mut out = Vec::new();
        for o in offsets {
            let offset = o.offset;
            let d = if offset.left { -o.distance } else { o.distance };
            let mut curves = BTreeMap::new();
            let mut sources = Vec::new();
            for c in &offset.curves {
                let geometry = curve(*c).ok_or_else(|| {
                    format!(
                        "{}: c{} is computed by another offset; a point cannot be held on an \
                         offset of it",
                        offset.id, c.0
                    )
                })?;
                curves.insert(*c, geometry.clone());
                sources.push(geometry);
            }
            let computed = compute_stored(offset, &index, &curves, d)?;
            out.push(Now {
                offset,
                computed,
                forward: chain_directions(&offset.curves, &index),
                sources,
                d,
            });
        }
        Ok(Self {
            offsets: out,
            index,
        })
    }

    /// The solver constraint of constraint `id` of `kind`, which names
    /// something of `derived` (what derived offsets compute): a Coincident
    /// of a point of the sketch with a curve or a point of a derived offset,
    /// held on the exact offset of its source; anything else is refused in
    /// words. With it the point of the sketch it holds, and where the hold
    /// puts that point with the sources as they are when the solve starts:
    /// the solve starts it there (`Built::new`), so that holding it moves
    /// the point, not the source toward it. A source pulled toward a point
    /// far from its offset changed the offset's shape until it no longer
    /// fitted (an ellipse stretched until its offset folded over).
    pub(crate) fn hold(
        &self,
        id: &dyn std::fmt::Display,
        kind: &ConstraintKind,
        derived: &BTreeSet<EntityUid>,
        ids: &Ids<'_>,
    ) -> Result<(S, EntityUid, Option<P2>), String> {
        let is = |u: EntityUid| derived.contains(&u);
        let first = || {
            kind.refs()
                .into_iter()
                .find(|r| is(r.uid()))
                .unwrap_or(Ref::Point(EntityUid(0)))
        };
        let ConstraintKind::Coincident { point, entity } = *kind else {
            return Err(refusal(id, first()));
        };
        let (user, target) = match entity {
            Ref::Curve(e) if is(e) && !is(point) => (point, Ref::Curve(e)),
            Ref::Point(q) if is(q) && !is(point) => (point, Ref::Point(q)),
            Ref::Point(q) if is(point) && !is(q) => (q, Ref::Point(point)),
            _ => return Err(refusal(id, first())),
        };
        let now = self
            .offsets
            .iter()
            .find(|n| self.computes(n, target.uid()))
            .ok_or_else(|| refusal(id, target))?;
        let p = (ids.point)(user)?;
        let at = (ids.at)(user).unwrap_or([0.0, 0.0]);
        let (s, place) = match target {
            Ref::Curve(c) => self.on_curve(now, c, p, at, ids)?,
            Ref::Point(q) => self.on_point(id, now, q, p, ids)?,
        };
        Ok((s, user, place))
    }

    /// Whether an offset computes an entity: one of its curves or their
    /// points.
    fn computes(&self, now: &Now<'_>, id: EntityUid) -> bool {
        now.offset.made().iter().any(|c| {
            *c == id
                || self
                    .index
                    .get(*c)
                    .is_some_and(|e| e.kind.points().contains(&id))
        })
    }

    /// The signed distance of source curve `i`'s offset along its own left
    /// normal.
    fn left(now: &Now<'_>, i: usize) -> f64 {
        if now.forward[i] { -now.d } else { now.d }
    }

    /// Where on source curve `i` its parameter `t` is: at its start or end
    /// when it is one, else the parameter itself.
    fn param_at(now: &Now<'_>, i: usize, t: f64) -> OffsetAt {
        let curve = &now.sources[i];
        let (lo, hi) = curve.domain();
        let tiny = 1e-9 * (hi - lo).abs().max(1.0);
        if !curve.is_closed() && (t - lo).abs() <= tiny {
            OffsetAt::Start
        } else if !curve.is_closed() && (t - hi).abs() <= tiny {
            OffsetAt::End
        } else {
            OffsetAt::At(t)
        }
    }

    /// The exact offset point of source curve `i` at `t`, and the unit
    /// offset direction there (its left normal times the side).
    fn offset_point(now: &Now<'_>, i: usize, t: f64) -> (P2, P2) {
        let [c, d1, _] = now.sources[i].eval(t);
        let speed = norm(d1).max(1e-300);
        let n = scale(perp(d1), 1.0 / speed);
        let left = Self::left(now, i);
        (add(c, scale(n, left)), scale(n, left.signum()))
    }

    /// The source curve's id and solver curve of source index `i`.
    fn source(now: &Now<'_>, i: usize, ids: &Ids<'_>) -> Result<EntityId, String> {
        (ids.curve)(now.offset.curves[i])
    }

    /// A point of the sketch on a curve the offset computes: a result, or a
    /// round corner arc.
    fn on_curve(
        &self,
        now: &Now<'_>,
        c: EntityUid,
        p: PointId,
        at: P2,
        ids: &Ids<'_>,
    ) -> Result<Held, String> {
        if let Some(k) = now.offset.results.iter().position(|r| *r == c) {
            let parts = now.computed.spans.get(k).ok_or("an offset curve is gone")?;
            // The part the point is nearest, and the parameter there.
            let mut best: Option<(f64, usize, f64)> = None;
            for (n, &(i, t0, t1)) in parts.iter().enumerate() {
                let (t, gap) = nearest(|t| Self::offset_point(now, i, t).0, t0, t1, at);
                if best.is_none_or(|b| gap < b.0) {
                    best = Some((gap, n, t));
                }
            }
            let (_, n, t) = best.ok_or("an offset curve has no parts")?;
            let (i, t0, t1) = parts[n];
            let (lo, hi) = (t0.min(t1), t0.max(t1));
            // A whole closed curve, or an arc's offset that runs round its
            // whole circle (mitcad#1246): free all round.
            let source = &now.sources[i];
            let (dlo, dhi) = source.domain();
            let whole_closed = source.is_periodic()
                && (lo - dlo).abs() <= 1e-9 * (dhi - dlo)
                && (hi - dhi).abs() <= 1e-9 * (dhi - dlo);
            let round_whole = matches!(now.computed.made.get(k), Some(Made::Circle { .. }));
            let range = (!(whole_closed || round_whole)).then_some((lo, hi));
            let place = Self::offset_point(now, i, t).0;
            return Ok((
                S::PointOnOffset {
                    point: p,
                    curve: Self::source(now, i, ids)?,
                    left: Self::left(now, i),
                    at: OffsetAt::Along { t, range },
                },
                Some(place),
            ));
        }
        let corner = now
            .offset
            .corners
            .iter()
            .find(|k| k.arc == c)
            .ok_or_else(|| format!("c{} is not a curve of offset {}", c.0, now.offset.id))?;
        let (a, a_at, b, b_at, k) = self.corner_sides(now, corner.join as usize)?;
        let corner_at = (ids.at)(k).unwrap_or_else(|| now.sources[a].eval(a_at.1)[0]);
        let ua = Self::offset_point(now, a, a_at.1).1;
        let ub = Self::offset_point(now, b, b_at.1).1;
        // The share of the way from one side's direction to the other's
        // that points at the point.
        let w = sub(at, corner_at);
        let (ca, cb) = (cross(ua, w), cross(ub, w));
        let s = ca / (ca - cb);
        let s = if s.is_finite() {
            s.clamp(0.0, 1.0)
        } else {
            0.5
        };
        // As the solver's corner: the direction between the two sides'.
        let between = add(scale(ua, 1.0 - s), scale(ub, s));
        let place = add(
            corner_at,
            scale(between, now.d.abs() / norm(between).max(1e-300)),
        );
        Ok((
            S::PointOnOffsetCorner {
                point: p,
                corner: (ids.point)(k)?,
                a: Self::source(now, a, ids)?,
                a_at: a_at.0,
                a_left: Self::left(now, a),
                b: Self::source(now, b, ids)?,
                b_at: b_at.0,
                b_left: Self::left(now, b),
                s,
            },
            Some(place),
        ))
    }

    /// The round corner after result `j`: the source curve before it with
    /// where its chain end is, the one after it with its chain start, and
    /// the source's corner point.
    #[allow(clippy::type_complexity)]
    fn corner_sides(
        &self,
        now: &Now<'_>,
        j: usize,
    ) -> Result<(usize, (OffsetAt, f64), usize, (OffsetAt, f64), EntityUid), String> {
        let gone = || format!("a round corner of offset {} is gone", now.offset.id);
        let after = now
            .computed
            .next
            .get(j)
            .copied()
            .flatten()
            .ok_or_else(gone)?;
        let &(a, _, ta) = now.computed.spans[j].last().ok_or_else(gone)?;
        let &(b, tb, _) = now.computed.spans[after].first().ok_or_else(gone)?;
        let (s, e) = self
            .index
            .get(now.offset.curves[a])
            .and_then(|x| x.kind.ends())
            .ok_or_else(gone)?;
        let corner = if now.forward[a] { e } else { s };
        Ok((
            a,
            (Self::param_at(now, a, ta), ta),
            b,
            (Self::param_at(now, b, tb), tb),
            corner,
        ))
    }

    /// A point of the sketch at a point the offset computes.
    fn on_point(
        &self,
        id: &dyn std::fmt::Display,
        now: &Now<'_>,
        q: EntityUid,
        p: PointId,
        ids: &Ids<'_>,
    ) -> Result<Held, String> {
        let offset = now.offset;
        for (k, r) in offset.results.iter().enumerate() {
            let Some(e) = self.index.get(*r) else {
                continue;
            };
            let Some(&(first, _, _)) = now.computed.spans.get(k).and_then(|s| s.first()) else {
                continue;
            };
            match e.kind {
                // The centre of an arc's offset: the source arc's.
                EntityKind::Arc { center, .. } | EntityKind::Circle { center, .. }
                    if center == q =>
                {
                    let source = self.index.get(offset.curves[first]);
                    return match source.map(|s| &s.kind) {
                        Some(EntityKind::Arc { center, .. }) => {
                            Ok((S::Coincident(p, (ids.point)(*center)?), (ids.at)(*center)))
                        }
                        _ => Err(refusal(id, Ref::Point(q))),
                    };
                }
                _ => {}
            }
            if let Some((start, end)) = chain_ends(&e.kind, now.forward[first]) {
                if start == q {
                    return self.at_end(id, now, k, false, p, q, ids);
                }
                if end == q {
                    return self.at_end(id, now, k, true, p, q, ids);
                }
            }
        }
        for corner in &offset.corners {
            let Some(EntityKind::Arc { center, start, end }) =
                self.index.get(corner.arc).map(|e| e.kind.clone())
            else {
                continue;
            };
            let j = corner.join as usize;
            if center == q {
                let (.., k) = self.corner_sides(now, j)?;
                return Ok((S::Coincident(p, (ids.point)(k)?), (ids.at)(k)));
            }
            if start != q && end != q {
                continue;
            }
            // Which result's end the arc's end is: the made arc's ends are
            // those of the results before and after it.
            let after = now.computed.next.get(j).copied().flatten();
            let made = now.computed.corners.iter().find(|(c, _)| *c == j);
            if let (
                Some(after),
                Some((
                    _,
                    Made::Arc {
                        start: ms, end: me, ..
                    },
                )),
            ) = (after, made)
            {
                let mine = if start == q { *ms } else { *me };
                let before_end = now.computed.ends[j].1;
                let after_start = now.computed.ends[after].0;
                return if dist(mine, before_end) <= dist(mine, after_start) {
                    self.at_end(id, now, j, true, p, q, ids)
                } else {
                    self.at_end(id, now, after, false, p, q, ids)
                };
            }
        }
        Err(format!(
            "{id}: {} is an inner control point of a spline that an offset of a spline or an \
             ellipse computes: hold the point on the offset's curve or at its end instead",
            Ref::Point(q)
        ))
    }

    /// A point of the sketch at the start (`last` false) or the end of
    /// result `k` along the chain: where nothing follows it there (an
    /// outer end), at a round corner or at a smooth join, at the exact
    /// offset of that end of its source curve; at a trimmed corner refused.
    #[allow(clippy::too_many_arguments)]
    fn at_end(
        &self,
        id: &dyn std::fmt::Display,
        now: &Now<'_>,
        k: usize,
        last: bool,
        p: PointId,
        q: EntityUid,
        ids: &Ids<'_>,
    ) -> Result<Held, String> {
        let links = linked(&now.computed.next);
        let join = links
            .iter()
            .position(|&(j, after)| if last { j == k } else { after == k })
            .and_then(|n| now.computed.joins.get(n).copied());
        if join == Some(Join::Trim) {
            return Err(format!(
                "{id}: {} is a trimmed inner corner of offset {}, where two of its curves cross \
                 and are cut: it moves along both as the source changes, so a point cannot be \
                 held there; use a point on one of the offset's curves or at its end",
                Ref::Point(q),
                now.offset.id
            ));
        }
        let parts = &now.computed.spans[k];
        let (i, t) = if last {
            let &(i, _, t) = parts.last().ok_or("an offset curve has no parts")?;
            (i, t)
        } else {
            let &(i, t, _) = parts.first().ok_or("an offset curve has no parts")?;
            (i, t)
        };
        Ok((
            S::PointOnOffset {
                point: p,
                curve: Self::source(now, i, ids)?,
                left: Self::left(now, i),
                at: Self::param_at(now, i, t),
            },
            Some(Self::offset_point(now, i, t).0),
        ))
    }
}

/// A result curve's start and end points along the chain: a line's and a
/// spline's start first, an arc's as its source runs (an arc runs
/// counter-clockwise, so its start is the chain's start when its source
/// runs forward).
fn chain_ends(kind: &EntityKind, forward: bool) -> Option<(EntityUid, EntityUid)> {
    match kind {
        EntityKind::Arc { start, end, .. } => Some(if forward {
            (*start, *end)
        } else {
            (*end, *start)
        }),
        EntityKind::Circle { .. } => None,
        other => other.ends(),
    }
}

/// The parameter in `[t0, t1]` (either order) where `curve` comes nearest
/// to `at`, and how near: a sampling refined by a golden-section search.
fn nearest(curve: impl Fn(f64) -> P2, t0: f64, t1: f64, at: P2) -> (f64, f64) {
    let (lo, hi) = (t0.min(t1), t0.max(t1));
    if hi - lo <= 0.0 {
        return (lo, dist(curve(lo), at));
    }
    let f = |t: f64| dist(curve(t), at);
    const N: usize = 200;
    let mut best = (f64::INFINITY, lo);
    for k in 0..=N {
        let t = lo + (hi - lo) * k as f64 / N as f64;
        let v = f(t);
        if v < best.0 {
            best = (v, t);
        }
    }
    const G: f64 = 0.618_033_988_749_894_9;
    let step = (hi - lo) / N as f64;
    let (mut a, mut b) = ((best.1 - step).max(lo), (best.1 + step).min(hi));
    for _ in 0..100 {
        if b - a <= 1e-14 * (hi - lo).max(1.0) {
            break;
        }
        let (x1, x2) = (b - G * (b - a), a + G * (b - a));
        if f(x1) < f(x2) {
            b = x2;
        } else {
            a = x1;
        }
    }
    let t = 0.5 * (a + b);
    let v = f(t);
    if v < best.0 { (t, v) } else { (best.1, best.0) }
}
