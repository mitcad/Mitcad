// SPDX-License-Identifier: MIT
//! Sketch offsets (offset constraints): a record per offset with
//! its source chain, its distance (a parameter) and the curves it made.
//!
//! - A chain of lines and arcs, or a circle, is offset exactly: the copies
//!   are ordinary curves held by constraints and dimensions that use the
//!   offset's parameter (`modify.rs`), so they can be constrained further.
//! - A chain with an ellipse, an elliptical arc or a spline is *derived*:
//!   its curves are computed from the solved source after every solve and
//!   are not solver unknowns (constraints cannot refer to them). Lines and
//!   arcs offset exactly; an ellipse or a spline has no exact offset of its
//!   kind, so it becomes a C1 cubic B-spline through Hermite pieces of the
//!   exact offset curve, within [`TOLERANCE`]. Corners where the chain
//!   turns away from the offset side get a round arc about the corner (the
//!   exact offset); corners where it turns toward it are trimmed where the
//!   two offset curves cross. The kind of each corner and the pieces of
//!   each spline are fixed when the offset is made, so the curves keep
//!   their ids and names; an edit that changes a corner's kind fails: an
//!   offset fails when its topology would change. A drag or a move of
//!   what it computes moves the source chain by the same motion
//!   ([`SketchEdit::moved_points`]): the offset keeps its distance and
//!   follows the pointer.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::edit::SketchEdit;
use super::geometry::{
    Curve2, Nurbs, P2, add, angle_after, cross, dist, dot, norm, perp, scale, sub, unit,
};
use super::solve::Solved;
use super::{
    ConstraintKind, ConstraintUid, DimensionKind, Entity, EntityIndex, EntityKind, Ref, is_false,
};
use crate::ids::EntityUid;
use crate::parameters::ParamId;
use crate::values::ValueInput;
use mitcad_model_base::internal::ids::curve_serde;
use mitcad_model_base::internal::values::resolve_value;

/// The largest distance between a spline offset and the exact offset
/// curve, millimetres.
pub const TOLERANCE: f64 = 1e-4;

/// The most pieces a spline offset is made of.
const MAX_PIECES: usize = 512;

/// A seeded offset corner where a line and a circle, or two circles, come
/// within this much (relative to the radius, at least 1 mm) of touching is a
/// tangent corner, put where they touch (mitcad#641).
const TOUCH: f64 = 1e-9;

/// A round corner of a derived offset: the arc between the offsets of
/// curves `join` and `join + 1` (of the last and the first for `join =
/// n - 1` of a closed chain).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OffsetCorner {
    pub join: u32,
    #[serde(with = "curve_serde")]
    pub arc: EntityUid,
}

/// An offset of a sketch, `k<n>` (sharing the numbers of constraints).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchOffset<P> {
    pub id: ConstraintUid,
    /// The source chain in order.
    #[serde(with = "super::curves_serde")]
    pub curves: Vec<EntityUid>,
    /// The distance, not negative; `left` tells the side.
    pub distance: P,
    /// To the left of the chain's direction (a negative distance in the
    /// command); right by default.
    #[serde(default, skip_serializing_if = "is_false")]
    pub left: bool,
    /// The offset of each source curve, in chain order.
    #[serde(with = "super::curves_serde")]
    pub results: Vec<EntityUid>,
    /// Round corners of a derived offset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corners: Vec<OffsetCorner>,
    /// The construction lines an exact offset of arcs and circles holds
    /// its distance with (across the gap, on a radius; mitcad#358), with
    /// the points they join: edits that make the offset again remove them
    /// with the results.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        with = "super::curves_serde"
    )]
    pub helpers: Vec<EntityUid>,
    /// Computed after every solve (chains with ellipses or splines).
    #[serde(default, skip_serializing_if = "is_false")]
    pub derived: bool,
}

impl<P> SketchOffset<P> {
    pub fn slot(&self) -> String {
        format!("offsets[{}].distance", self.id)
    }

    pub fn map_value<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SketchOffset<Q>, E> {
        Ok(SketchOffset {
            id: self.id,
            curves: self.curves.clone(),
            distance: f(&self.slot(), &self.distance)?,
            left: self.left,
            results: self.results.clone(),
            corners: self.corners.clone(),
            helpers: self.helpers.clone(),
            derived: self.derived,
        })
    }

    /// The curves it made: results and corner arcs.
    pub fn made(&self) -> Vec<EntityUid> {
        let mut made = self.results.clone();
        made.extend(self.corners.iter().map(|c| c.arc));
        made
    }

    pub fn check(&self, index: &EntityIndex<'_>) -> Result<(), String> {
        let what = self.id.to_string();
        if self.curves.is_empty() {
            return Err(format!("{what}: no curves to offset"));
        }
        if self.derived && self.results.len() != self.curves.len() {
            return Err(format!("{what}: one result per curve"));
        }
        for id in self.curves.iter().chain(&self.made()) {
            match index.get(*id) {
                Some(e) if !e.is_point() => {}
                _ => return Err(format!("{what}: curve c{} does not exist", id.0)),
            }
        }
        Ok(())
    }
}

/// The entities derived offsets compute: their curves and those curves'
/// points.
pub fn derived_entities<P>(
    offsets: &[SketchOffset<P>],
    index: &EntityIndex<'_>,
) -> BTreeSet<EntityUid> {
    let mut out = BTreeSet::new();
    for offset in offsets.iter().filter(|o| o.derived) {
        for curve in offset.made() {
            out.insert(curve);
            if let Some(e) = index.get(curve) {
                out.extend(e.kind.points());
            }
        }
    }
    out
}

/// The direction of each curve of a stored chain: forward when it runs
/// from its start to its end along the chain.
pub fn chain_directions(curves: &[EntityUid], index: &EntityIndex<'_>) -> Vec<bool> {
    let ends = |id: EntityUid| index.get(id).and_then(|e| e.kind.ends());
    let n = curves.len();
    let mut out = Vec::with_capacity(n);
    let Some((s0, e0)) = curves.first().and_then(|c| ends(*c)) else {
        return vec![true; n];
    };
    // The first runs toward the second, as the chain was ordered.
    let first = n == 1 || ends(curves[1]).is_some_and(|(a, b)| e0 == a || e0 == b);
    out.push(first);
    let mut at = if first { e0 } else { s0 };
    for c in &curves[1..] {
        let forward = match ends(*c) {
            Some((s, e)) => {
                let forward = s == at;
                at = if forward { e } else { s };
                forward
            }
            None => true,
        };
        out.push(forward);
    }
    out
}

/// One source curve in chain direction, offset by `d` to its right.
#[derive(Debug, Clone)]
struct Piece {
    curve: Curve2,
    forward: bool,
    /// Signed distance along the source curve's left normal.
    left: f64,
}

impl Piece {
    /// The source parameter at chain parameter `u` in [0, 1].
    fn t(&self, u: f64) -> f64 {
        let (lo, hi) = self.curve.domain();
        if self.forward {
            lo + u * (hi - lo)
        } else {
            hi - u * (hi - lo)
        }
    }

    /// The exact offset point and its derivative by `u`.
    fn eval(&self, u: f64) -> (P2, P2) {
        let (lo, hi) = self.curve.domain();
        let dt = if self.forward { hi - lo } else { lo - hi };
        let [c, d1, d2] = self.curve.eval(self.t(u));
        let speed = norm(d1);
        let n = scale(perp(d1), 1.0 / speed);
        let dn = sub(
            scale(perp(d2), 1.0 / speed),
            scale(perp(d1), dot(d1, d2) / (speed * speed * speed)),
        );
        (
            add(c, scale(n, self.left)),
            scale(add(d1, scale(dn, self.left)), dt),
        )
    }

    /// The source point and the chain's unit tangent there.
    fn source(&self, u: f64) -> (P2, P2) {
        let [c, d1, _] = self.curve.eval(self.t(u));
        (c, unit(if self.forward { d1 } else { scale(d1, -1.0) }))
    }

    fn is_line(&self) -> bool {
        matches!(self.curve, Curve2::Line { .. })
    }

    /// Fails where the offset folds over (the distance passes the radius of
    /// curvature on its side).
    fn check_fit(&self) -> Result<(), String> {
        if self.is_line() {
            return Ok(());
        }
        let (lo, hi) = self.curve.domain();
        for t in self.curve.sample_params(lo, hi) {
            let [_, d1, d2] = self.curve.eval(t);
            let speed = norm(d1);
            let curvature = cross(d1, d2) / (speed * speed * speed);
            if 1.0 - self.left * curvature < 0.02 {
                return Err(
                    "the offset is larger than a curve's radius of curvature on that side"
                        .to_owned(),
                );
            }
        }
        Ok(())
    }
}

/// How the offsets of two curves meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// They meet already (a smooth join).
    Shared,
    /// They cross: both are trimmed there.
    Trim,
    /// They leave a gap: a round arc about the corner closes it.
    Round,
}

/// The computed offset of one source curve.
#[derive(Debug, Clone, PartialEq)]
pub enum Made {
    Line(P2, P2),
    /// Counter-clockwise from `start` to `end` about `center`.
    Arc {
        center: P2,
        start: P2,
        end: P2,
    },
    Circle {
        center: P2,
        radius: f64,
    },
    /// A cubic B-spline with double interior knots (C1): `knots` are the
    /// distinct ones on [0, 1], two control points per knot.
    Spline {
        knots: Vec<f64>,
        control: Vec<P2>,
    },
}

/// A derived offset computed: one curve per source curve in chain order
/// with its start and end in chain direction, the joins' kinds and round
/// corner arcs.
#[derive(Debug, Clone, PartialEq)]
pub struct Computed {
    pub made: Vec<Made>,
    pub ends: Vec<(P2, P2)>,
    pub joins: Vec<Join>,
    pub corners: Vec<(usize, Made)>,
    pub closed: bool,
}

/// What a recomputation keeps from when the offset was made.
pub struct Structure {
    /// The distinct knots of each spline result (None for other curves).
    pub knots: Vec<Option<Vec<f64>>>,
    pub joins: Vec<Join>,
}

/// The cubic Hermite B-spline of `piece` on chain parameters `[u0, u1]`
/// with distinct knots `w` on [0, 1].
fn hermite(piece: &Piece, u0: f64, u1: f64, w: &[f64]) -> Vec<P2> {
    let at = |wk: f64| {
        let (q, dq) = piece.eval(u0 + wk * (u1 - u0));
        (q, scale(dq, u1 - u0))
    };
    let n = w.len() - 1;
    let mut control = Vec::with_capacity(2 * n + 2);
    for k in 0..=n {
        let (q, dq) = at(w[k]);
        if k > 0 {
            let h = w[k] - w[k - 1];
            control.push(sub(q, scale(dq, h / 3.0)));
        } else {
            control.push(q);
        }
        if k < n {
            let h = w[k + 1] - w[k];
            control.push(add(q, scale(dq, h / 3.0)));
        } else {
            control.push(q);
        }
    }
    control
}

/// The B-spline of a Hermite control polygon (see [`Made::Spline`]).
pub fn spline_nurbs(knots: &[f64], control: &[P2]) -> Nurbs {
    let mut full = vec![knots[0]; 4];
    for k in &knots[1..knots.len() - 1] {
        full.push(*k);
        full.push(*k);
    }
    full.extend(std::iter::repeat_n(knots[knots.len() - 1], 4));
    Nurbs {
        degree: 3,
        control: control.to_vec(),
        weights: Vec::new(),
        knots: full,
    }
}

/// Knots on [0, 1] for a curve piece: a few per span of the source, split
/// until the Hermite curve is within a quarter of the tolerance of the
/// exact offset (the margin keeps it within the tolerance while the source
/// changes between edits; every sketch edit fits the knots again).
fn adaptive_knots(piece: &Piece, u0: f64, u1: f64) -> Vec<f64> {
    let spans = match &piece.curve {
        Curve2::Nurbs(n) => n.knots.windows(2).filter(|w| w[1] > w[0]).count().max(1),
        _ => 4,
    };
    let start = (spans * 2).clamp(4, MAX_PIECES);
    let mut w: Vec<f64> = (0..=start).map(|k| k as f64 / start as f64).collect();
    loop {
        let control = hermite(piece, u0, u1, &w);
        let curve = Curve2::Nurbs(spline_nurbs(&w, &control));
        let mut split = Vec::new();
        for k in 0..w.len() - 1 {
            let bad = [0.25, 0.5, 0.75].iter().any(|f| {
                let wk = w[k] + f * (w[k + 1] - w[k]);
                let (exact, _) = piece.eval(u0 + wk * (u1 - u0));
                dist(curve.point(wk), exact) > TOLERANCE / 4.0
            });
            if bad {
                split.push(k);
            }
        }
        if split.is_empty() || w.len() > MAX_PIECES {
            return w;
        }
        for k in split.into_iter().rev() {
            w.insert(k + 1, (w[k] + w[k + 1]) / 2.0);
        }
    }
}

/// Where the offsets of two pieces cross near their join: `u` near the end
/// of `a`, `v` near the start of `b` (Newton from the join).
fn crossing(a: &Piece, b: &Piece) -> Option<(f64, f64)> {
    let (mut u, mut v) = (1.0, 0.0);
    // Lines extend: start from their exact crossing.
    for _ in 0..60 {
        let (qa, da) = a.eval(u);
        let (qb, db) = b.eval(v);
        let r = sub(qa, qb);
        // [da, -db] [du, dv]^T = -r
        let det = cross(da, scale(db, -1.0));
        if det.abs() < 1e-300 {
            return None;
        }
        let du = cross(scale(r, -1.0), scale(db, -1.0)) / det;
        let dv = cross(da, scale(r, -1.0)) / det;
        u += du;
        v += dv;
        if du.abs() < 1e-14 && dv.abs() < 1e-14 {
            break;
        }
    }
    let (qa, _) = a.eval(u);
    let (qb, _) = b.eval(v);
    let size = 1.0 + norm(qa);
    (dist(qa, qb) <= 1e-9 * size && u > 0.0 && u <= 1.0 + 1e-9 && (-1e-9..1.0).contains(&v))
        .then_some((u.min(1.0), v.max(0.0)))
}

/// The arc about `corner` from `from` to `to`, the short way.
fn round_corner(corner: P2, from: P2, to: P2) -> Made {
    let a0 = sub(from, corner);
    let a1 = sub(to, corner);
    if cross(a0, a1) >= 0.0 {
        Made::Arc {
            center: corner,
            start: from,
            end: to,
        }
    } else {
        Made::Arc {
            center: corner,
            start: to,
            end: from,
        }
    }
}

/// Computes the offset of a chain of solved curves: `d` to the right of
/// the chain's direction (negative to the left). With a structure, the
/// joins and spline knots are those of the structure, and a join of
/// another kind now is an error.
pub fn compute(
    sources: &[(Curve2, bool)],
    d: f64,
    closed: bool,
    structure: Option<&Structure>,
) -> Result<Computed, String> {
    let pieces: Vec<Piece> = sources
        .iter()
        .map(|(curve, forward)| Piece {
            curve: curve.clone(),
            forward: *forward,
            left: if *forward { -d } else { d },
        })
        .collect();
    for p in &pieces {
        p.check_fit()?;
    }
    let n = pieces.len();
    let size = pieces
        .iter()
        .map(|p| {
            let (lo, hi) = p.curve.bounds();
            dist(lo, hi)
        })
        .fold(1.0, f64::max);
    let single_closed = n == 1 && sources[0].0.is_closed();
    let closed = closed && !single_closed;
    let joins_count = if closed { n } else { n.saturating_sub(1) };
    // Joins: kinds and the trimmed ranges.
    let mut range = vec![(0.0, 1.0); n];
    let mut joins = Vec::with_capacity(joins_count);
    let mut corners = Vec::new();
    for j in 0..joins_count {
        let (a, b) = (&pieces[j], &pieces[(j + 1) % n]);
        let (end_a, _) = a.eval(1.0);
        let (start_b, _) = b.eval(0.0);
        let (corner, ta) = a.source(1.0);
        let (_, tb) = b.source(0.0);
        let turn = cross(ta, tb);
        let kind = if dist(end_a, start_b) <= 1e-7 * size {
            Join::Shared
        } else if turn * d < 0.0 {
            // Turning toward the offset side (right for a positive d).
            Join::Trim
        } else {
            Join::Round
        };
        if let Some(s) = structure
            && s.joins.get(j) != Some(&kind)
        {
            return Err(format!(
                "the offset no longer fits: the corner at join {j} changed from {:?} to {kind:?} \
                 (round, trimmed or smooth); offset again",
                s.joins.get(j)
            ));
        }
        match kind {
            Join::Shared => {}
            Join::Trim => {
                let (u, v) = crossing(a, b)
                    .ok_or("the offset curves do not meet at a corner of the chain")?;
                range[j].1 = u;
                range[(j + 1) % n].0 = v;
            }
            Join::Round => corners.push((j, round_corner(corner, end_a, start_b))),
        }
        joins.push(kind);
    }
    // Each piece in chain direction: start and end, then its curve.
    let mut ends: Vec<(P2, P2)> = Vec::with_capacity(n);
    for (i, p) in pieces.iter().enumerate() {
        let (u0, u1) = range[i];
        if u1 <= u0 {
            return Err("the offset is larger than a curve of the chain".to_owned());
        }
        ends.push((p.eval(u0).0, p.eval(u1).0));
    }
    // Pieces that meet share one point exactly.
    for (j, join) in joins.iter().enumerate() {
        if *join != Join::Round {
            let next = (j + 1) % n;
            let p = scale(add(ends[j].1, ends[next].0), 0.5);
            ends[j].1 = p;
            ends[next].0 = p;
        }
    }
    let mut made = Vec::with_capacity(n);
    for (i, p) in pieces.iter().enumerate() {
        let (u0, u1) = range[i];
        let (from, to) = ends[i];
        made.push(match &p.curve {
            Curve2::Line { .. } => Made::Line(from, to),
            Curve2::Circle { center, radius, .. } if single_closed => {
                let r = radius + p.left;
                if r <= 0.0 {
                    return Err("the offset is larger than the circle's radius".to_owned());
                }
                Made::Circle {
                    center: *center,
                    radius: r,
                }
            }
            Curve2::Arc { center, radius, .. } => {
                if radius + p.left <= 0.0 {
                    return Err("the offset is larger than an arc's radius".to_owned());
                }
                if p.forward {
                    Made::Arc {
                        center: *center,
                        start: from,
                        end: to,
                    }
                } else {
                    Made::Arc {
                        center: *center,
                        start: to,
                        end: from,
                    }
                }
            }
            _ => {
                let knots = match structure.and_then(|s| s.knots.get(i).cloned().flatten()) {
                    Some(k) => k,
                    None => adaptive_knots(p, u0, u1),
                };
                let mut control = hermite(p, u0, u1, &knots);
                let last = control.len() - 1;
                control[0] = from;
                control[last] = if single_closed { from } else { to };
                Made::Spline { knots, control }
            }
        });
    }
    Ok(Computed {
        made,
        ends,
        joins,
        corners,
        closed,
    })
}

/// The curve of a made offset.
pub fn made_curve(made: &Made) -> Curve2 {
    match made {
        Made::Line(a, b) => Curve2::Line { a: *a, b: *b },
        Made::Arc { center, start, end } => {
            let (vs, ve) = (sub(*start, *center), sub(*end, *center));
            let a0 = vs[1].atan2(vs[0]);
            Curve2::Arc {
                center: *center,
                radius: norm(vs),
                start: a0,
                end: angle_after(a0, ve[1].atan2(ve[0])),
            }
        }
        Made::Circle { center, radius } => Curve2::Circle {
            center: *center,
            radius: *radius,
            start: 0.0,
        },
        Made::Spline { knots, control } => Curve2::Nurbs(spline_nurbs(knots, control)),
    }
}

/// Positions of points by id, and a circle's radius.
type Positions = (Vec<(EntityUid, P2)>, Option<f64>);

/// The positions a made curve gives the points of its entity (points in
/// the entity's order), and a circle's radius.
fn positions(made: &Made, kind: &EntityKind) -> Result<Positions, String> {
    let mismatch = || "an offset curve was changed; offset again".to_owned();
    Ok(match (made, kind) {
        (Made::Line(a, b), EntityKind::Line { start, end, .. }) => {
            (vec![(*start, *a), (*end, *b)], None)
        }
        (
            Made::Arc {
                center: c,
                start: s,
                end: e,
            },
            EntityKind::Arc { center, start, end },
        ) => (vec![(*center, *c), (*start, *s), (*end, *e)], None),
        (Made::Circle { center: c, radius }, EntityKind::Circle { center, .. }) => {
            (vec![(*center, *c)], Some(*radius))
        }
        (Made::Spline { control, .. }, EntityKind::Spline { control: ids, .. })
            if ids.len() == control.len() =>
        {
            (
                ids.iter().copied().zip(control.iter().copied()).collect(),
                None,
            )
        }
        _ => return Err(mismatch()),
    })
}

/// The distinct knots of a stored spline result, scaled to [0, 1].
fn stored_knots(kind: &EntityKind) -> Option<Vec<f64>> {
    let EntityKind::Spline { knots, .. } = kind else {
        return None;
    };
    let mut distinct: Vec<f64> = Vec::new();
    for k in knots {
        if distinct.last() != Some(k) {
            distinct.push(*k);
        }
    }
    Some(distinct)
}

/// The chain of an offset's source curves with their solved geometry.
fn sources(
    offset: &SketchOffset<ParamId>,
    index: &EntityIndex<'_>,
    curves: &std::collections::BTreeMap<EntityUid, Curve2>,
) -> Result<(Vec<(Curve2, bool)>, bool), String> {
    let forward = chain_directions(&offset.curves, index);
    let mut out = Vec::new();
    for (id, fwd) in offset.curves.iter().zip(forward.iter().copied()) {
        let c = curves
            .get(id)
            .cloned()
            .ok_or_else(|| format!("{}: c{} has no geometry", offset.id, id.0))?;
        out.push((c, fwd));
    }
    // Curves join only through shared points, as when the offset was made
    // (mitcad#360): ends that merely lie at the same place do not close it.
    let end = |curve: EntityUid, forward: bool, last: bool| {
        index
            .get(curve)
            .and_then(|e| e.kind.ends())
            .map(|(s, e)| if forward != last { s } else { e })
    };
    let closed = match (offset.curves.first(), offset.curves.last()) {
        (Some(first), Some(last)) if offset.curves.len() > 1 => {
            let (ff, fl) = (forward[0], forward[offset.curves.len() - 1]);
            let (start, finish) = (end(*first, ff, false), end(*last, fl, true));
            start.is_some() && start == finish
        }
        _ => false,
    };
    Ok((out, closed))
}

/// Computes every derived offset from the solved geometry and puts its
/// curves and points into `solved`. `value` gives a parameter's value.
pub fn regenerate(
    offsets: &[SketchOffset<ParamId>],
    entities: &[Entity],
    solved: &mut Solved,
    value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
) -> Result<(), String> {
    let index = EntityIndex::new(entities);
    for offset in offsets.iter().filter(|o| o.derived) {
        let d = value(offset.distance)?;
        let d = if offset.left { -d } else { d };
        let (chain, closed) = sources(offset, &index, &solved.curves)?;
        let kinds: Vec<EntityKind> = offset
            .results
            .iter()
            .map(|r| {
                index
                    .get(*r)
                    .map(|e| e.kind.clone())
                    .ok_or("an offset curve is gone")
            })
            .collect::<Result<_, _>>()?;
        let structure = Structure {
            knots: kinds.iter().map(stored_knots).collect(),
            joins: stored_joins(offset, &chain, closed),
        };
        let computed = compute(&chain, d, closed, Some(&structure))
            .map_err(|e| format!("offset {}: {e}", offset.id))?;
        let mut write = |made: &Made, curve: EntityUid| -> Result<(), String> {
            let kind = &index.get(curve).ok_or("an offset curve is gone")?.kind;
            let (points, radius) =
                positions(made, kind).map_err(|e| format!("offset {}: {e}", offset.id))?;
            for (p, at) in points {
                solved.points.insert(p, at);
            }
            if let Some(r) = radius {
                solved.radii.insert(curve, r);
            }
            solved.curves.insert(curve, made_curve(made));
            Ok(())
        };
        for (made, curve) in computed.made.iter().zip(&offset.results) {
            write(made, *curve)?;
        }
        for (join, made) in &computed.corners {
            let arc = offset
                .corners
                .iter()
                .find(|c| c.join as usize == *join)
                .map(|c| c.arc)
                .ok_or_else(|| format!("offset {}: a corner arc is gone", offset.id))?;
            write(made, arc)?;
        }
    }
    Ok(())
}

/// An arc or circle of an offset's chain, as [`seed`] places its helper:
/// the centre, the source's radius, the result's, and an arc's angles.
type Round = (P2, f64, f64, Option<(f64, f64)>);

/// Puts the curves of each offset made by constraints where the offset of
/// its source is now, as the solve's start (mitcad#641). The solver keeps a
/// distance dimension on the side its curves start on: when the source has
/// moved by more than the distance before the solve (a linked projection
/// that followed its face a grid cell, 28 mm, with a 2 mm offset), the
/// stale results lay on the other side of the moved source and were solved
/// there, mirrored. Each result goes where the offset of its source curve
/// is (a line moved along its normal, an arc or circle about its centre,
/// an arc's ends at its source's angles); a corner two results share where
/// their offset curves cross (two lines, a line and an arc's circle, or
/// two arcs' circles; else between their ends), so that an offset already
/// in place stays as it is; the rest is left to the solve, whose
/// constraints join the results. An offset whose results are not of their sources' kinds keeps
/// its stored positions. Offsets in order, so that an offset of an offset starts from
/// its source's new place.
pub fn seed(
    offsets: &[SketchOffset<ParamId>],
    entities: &mut [Entity],
    value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
) {
    for offset in offsets.iter().filter(|o| !o.derived) {
        let Ok(d) = value(offset.distance) else {
            continue;
        };
        if !(d.is_finite() && d > 0.0) {
            continue;
        }
        let d = if offset.left { -d } else { d };
        let updates = {
            let index = EntityIndex::new(entities);
            let at = |p: EntityUid| match index.get(p).map(|e| &e.kind) {
                Some(EntityKind::Point { at }) => Some(*at),
                _ => None,
            };
            let mut curves = std::collections::BTreeMap::new();
            for id in &offset.curves {
                let curve = match index.get(*id).map(|e| &e.kind) {
                    Some(EntityKind::Line { start, end, .. }) => {
                        let (Some(a), Some(b)) = (at(*start), at(*end)) else {
                            continue;
                        };
                        Curve2::Line { a, b }
                    }
                    Some(EntityKind::Arc { center, start, end }) => {
                        let (Some(c), Some(a), Some(b)) = (at(*center), at(*start), at(*end))
                        else {
                            continue;
                        };
                        let (va, vb) = (sub(a, c), sub(b, c));
                        let start = va[1].atan2(va[0]);
                        Curve2::Arc {
                            center: c,
                            radius: norm(va),
                            start,
                            end: angle_after(start, vb[1].atan2(vb[0])),
                        }
                    }
                    Some(EntityKind::Circle {
                        center,
                        radius,
                        start,
                    }) => {
                        let Some(c) = at(*center) else { continue };
                        Curve2::Circle {
                            center: c,
                            radius: *radius,
                            start: *start,
                        }
                    }
                    _ => continue,
                };
                curves.insert(*id, curve);
            }
            // Each source curve with the direction it runs along the chain:
            // by its shared points, else (a chain joined at coincident ends,
            // as an import's offset of an offset is) where its ends meet.
            let chain = match sources(offset, &index, &curves) {
                Ok((chain, _)) => chain,
                Err(_) => {
                    let listed: Option<Vec<(Curve2, bool)>> = offset
                        .curves
                        .iter()
                        .map(|id| curves.get(id).map(|c| (c.clone(), true)))
                        .collect();
                    let Some(listed) = listed else {
                        continue;
                    };
                    joined_by_ends(&listed).0
                }
            };
            if chain.len() != offset.results.len() {
                continue;
            }
            // Each result where the offset of its own source curve is: the
            // offset's joins are its constraints', which the solve keeps. A
            // point two results share (a corner) takes where their offset
            // lines meet, else the middle of their ends.
            let mut points: Vec<(EntityUid, P2)> = Vec::new();
            let mut lines: Vec<(EntityUid, P2, P2)> = Vec::new();
            let mut radii: Vec<(EntityUid, f64)> = Vec::new();
            let mut circles: Vec<(EntityUid, P2, f64)> = Vec::new();
            // The arcs and circles in chain order: centre, the source's
            // radius and the result's, and an arc's range of angles.
            let mut rounds: Vec<Round> = Vec::new();
            let mut fits = true;
            for ((curve, forward), result) in chain.iter().zip(&offset.results) {
                // Along the left normal of the curve's own direction, as
                // `compute` offsets a piece.
                let left = if *forward { -d } else { d };
                match (curve, index.get(*result).map(|e| &e.kind)) {
                    (Curve2::Line { a, b }, Some(EntityKind::Line { start, end, .. })) => {
                        let length = norm(sub(*b, *a));
                        if !(length.is_finite() && length > 0.0) {
                            fits = false;
                            continue;
                        }
                        let along = unit(sub(*b, *a));
                        let n = perp(along);
                        let (a, b) = (add(*a, scale(n, left)), add(*b, scale(n, left)));
                        // The result keeps its own direction.
                        let stored = at(*start).zip(at(*end)).map(|(s, e)| sub(e, s));
                        let (a, b) = match stored {
                            Some(v) if dot(v, along) < 0.0 => (b, a),
                            _ => (a, b),
                        };
                        points.extend([(*start, a), (*end, b)]);
                        lines.extend([(*start, a, b), (*end, a, b)]);
                    }
                    (
                        Curve2::Arc {
                            center,
                            radius,
                            start: s,
                            end: e,
                        },
                        Some(EntityKind::Arc {
                            center: c,
                            start,
                            end,
                        }),
                    ) => {
                        let r = radius - left;
                        if !(r.is_finite() && r > 0.0) {
                            fits = false;
                            continue;
                        }
                        // The ends at the source's angles (a free end where
                        // the source's is, even where the source turned);
                        // a corner is placed below, where the curves cross.
                        let on = |t: f64| add(*center, [r * t.cos(), r * t.sin()]);
                        points.extend([(*c, *center), (*start, on(*s)), (*end, on(*e))]);
                        circles.extend([(*start, *center, r), (*end, *center, r)]);
                        rounds.push((*center, *radius, r, Some((*s, *e))));
                    }
                    (
                        Curve2::Circle { center, radius, .. },
                        Some(EntityKind::Circle { center: c, .. }),
                    ) => {
                        let r = radius - left;
                        if !(r.is_finite() && r > 0.0) {
                            fits = false;
                            continue;
                        }
                        points.push((*c, *center));
                        radii.push((*result, r));
                        rounds.push((*center, *radius, r, None));
                    }
                    _ => fits = false,
                }
            }
            if !fits {
                continue;
            }
            // The construction line each arc or circle holds its distance
            // with (one per arc, in chain order, from a point on the source
            // to one on the result through the centre) goes along with it:
            // left where it was when its arc had moved, it started a solve of
            // its own once the corners were held where the curves touch
            // (mitcad#890), and that solve did not find the arc again from
            // 17 mm away. It goes where the solve holds it (mitcad#1171,
            // [`independent`]): across an arc's middle, level with a
            // circle's centre on the side it is on.
            if offset.helpers.len() == rounds.len() {
                for (helper, (center, from, to, range)) in offset.helpers.iter().zip(&rounds) {
                    let Some(EntityKind::Line { start, end, .. }) =
                        index.get(*helper).map(|e| &e.kind)
                    else {
                        continue;
                    };
                    let (Some(p0), Some(p1)) = (at(*start), at(*end)) else {
                        continue;
                    };
                    // Out from the centre along the helper, the way the
                    // result lies from the source.
                    let along = sub(p1, p0);
                    let length = norm(along);
                    let mut radial = if length > 0.0 {
                        scale(along, if to > from { 1.0 } else { -1.0 } / length)
                    } else {
                        [1.0, 0.0]
                    };
                    match range {
                        Some((s, e)) => {
                            let middle = (s + e) / 2.0;
                            radial = [middle.cos(), middle.sin()];
                        }
                        None => radial = [radial[0].signum(), 0.0],
                    }
                    points.push((*start, add(*center, scale(radial, *from))));
                    points.push((*end, add(*center, scale(radial, *to))));
                }
            }
            // Corners: each point once.
            let mut placed: Vec<(EntityUid, P2)> = Vec::new();
            for (id, p) in &points {
                if placed.iter().any(|(q, _)| q == id) {
                    continue;
                }
                let all: Vec<P2> = points
                    .iter()
                    .filter(|(q, _)| q == id)
                    .map(|(_, p)| *p)
                    .collect();
                if all.len() == 1 {
                    placed.push((*id, *p));
                    continue;
                }
                let through: Vec<(P2, P2)> = lines
                    .iter()
                    .filter(|(q, _, _)| q == id)
                    .map(|(_, a, b)| (*a, *b))
                    .collect();
                let round: Vec<(P2, f64)> = circles
                    .iter()
                    .filter(|(q, _, _)| q == id)
                    .map(|(_, c, r)| (*c, *r))
                    .collect();
                let middle = scale(
                    all.iter().fold([0.0, 0.0], |s, p| add(s, *p)),
                    1.0 / all.len() as f64,
                );
                let meet = match (through.as_slice(), round.as_slice()) {
                    ([(a1, b1), (a2, b2)], []) => {
                        let (u, v) = (sub(*b1, *a1), sub(*b2, *a2));
                        let den = cross(u, v);
                        (den.abs() > 1e-9 * norm(u) * norm(v))
                            .then(|| add(*a1, scale(u, cross(sub(*a2, *a1), v) / den)))
                    }
                    // A line meeting an arc: their crossing nearest the
                    // middle of the two ends; a line touching the circle
                    // (within rounding, a tangent corner) at the point it
                    // touches, as the two crossings' distance from it is the
                    // square root of the rounding and would move the corner.
                    ([(a, b)], [(c, r)]) => {
                        let u = unit(sub(*b, *a));
                        let foot = add(*a, scale(u, dot(sub(*c, *a), u)));
                        let h = norm(sub(foot, *c));
                        if (r - h).abs() <= TOUCH * r.max(1.0) {
                            Some(foot)
                        } else {
                            (h < *r).then(|| {
                                let s = (r * r - h * h).sqrt();
                                let (p, q) = (add(foot, scale(u, s)), add(foot, scale(u, -s)));
                                if norm(sub(p, middle)) <= norm(sub(q, middle)) {
                                    p
                                } else {
                                    q
                                }
                            })
                        }
                    }
                    // Two arcs: their circles' crossing nearest the middle
                    // of the two ends; circles touching (within rounding) at
                    // the point they touch. Circles whose centres lie within
                    // rounding of each other (one circle split into two arcs
                    // with a centre point each, as a projection makes them)
                    // neither cross nor touch at a point: their "touching
                    // point" fell near the centre, far off the circle, and
                    // the solve started from there lost the profile
                    // (mitcad#709). Their corner is the middle of the ends.
                    ([], [(c1, r1), (c2, r2)]) => {
                        let d = norm(sub(*c2, *c1));
                        let touch = TOUCH * (r1 + r2).max(1.0);
                        (d > touch && d <= r1 + r2 + touch && d >= (r1 - r2).abs() - touch).then(
                            || {
                                let u = scale(sub(*c2, *c1), 1.0 / d);
                                let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
                                let foot = add(*c1, scale(u, a));
                                if (d - (r1 + r2)).abs() <= touch
                                    || (d - (r1 - r2).abs()).abs() <= touch
                                {
                                    return foot;
                                }
                                let h = (r1 * r1 - a * a).max(0.0).sqrt();
                                let (p, q) =
                                    (add(foot, scale(perp(u), h)), add(foot, scale(perp(u), -h)));
                                if norm(sub(p, middle)) <= norm(sub(q, middle)) {
                                    p
                                } else {
                                    q
                                }
                            },
                        )
                    }
                    _ => None,
                };
                placed.push((*id, meet.unwrap_or(middle)));
            }
            (placed, radii)
        };
        let (points, radii) = updates;
        for entity in entities.iter_mut() {
            match &mut entity.kind {
                EntityKind::Point { at } => {
                    if let Some((_, p)) = points.iter().find(|(id, _)| *id == entity.id) {
                        *at = *p;
                    }
                }
                EntityKind::Circle { radius, .. } => {
                    if let Some((_, r)) = radii.iter().find(|(id, _)| *id == entity.id) {
                        *radius = *r;
                    }
                }
                _ => {}
            }
        }
    }
}

/// The tangent corners of the offsets made by constraints, as solver
/// bindings (mitcad#890). Two results of such an offset share their corner
/// point and are held only by their own constraints (a line parallel to its
/// source at the distance, an arc about its source's centre); where the
/// source curves touch at their corner, the results touch there too, and the
/// corner is where the result line touches the result circle. That point is
/// fixed only to the second order: the corner slides along the line by the
/// square root of the solve's rounding (2e-5 mm for 1.6e-11 mm on a 12 mm
/// arc), so the arc ended past the touching point, the profile turned there
/// by 1.6e-6 rad, and a sweep along the extruded outline and its cut came out
/// in pieces after a grid design was widened. The binding holds the corner
/// where the curves touch (the solver's tangency at a shared point, which is
/// of the first order). A source corner touches by its definition: the two
/// source curves are tangent by a constraint, or both are fixed (projected
/// edges) and touch at their shared point, or they are results of an offset
/// whose corner is bound so. A corner of free curves that only happen to
/// touch is not bound: it may part when they move.
pub fn tangent_corners(
    offsets: &[SketchOffset<ParamId>],
    entities: &[Entity],
    constraints: &[super::Constraint],
) -> Vec<super::pattern::Binding> {
    use super::pattern::{Binding, BindingKind};
    let index = EntityIndex::new(entities);
    let pair = |a: EntityUid, b: EntityUid| if a <= b { (a, b) } else { (b, a) };
    let mut touching: BTreeSet<(EntityUid, EntityUid)> = constraints
        .iter()
        .filter(|c| c.active)
        .filter_map(|c| match c.kind {
            ConstraintKind::Tangent { a, b, .. } | ConstraintKind::Smooth { a, b, .. } => {
                Some(pair(a, b))
            }
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    for offset in offsets.iter().filter(|o| !o.derived) {
        let n = offset.curves.len();
        if n < 2 || offset.results.len() != n {
            continue;
        }
        let forward = chain_directions(&offset.curves, &index);
        let end = |curve: EntityUid, forward: bool, last: bool| {
            index
                .get(curve)
                .and_then(|e| e.kind.ends())
                .map(|(s, e)| if forward != last { s } else { e })
        };
        let closed = {
            let (start, finish) = (
                end(offset.curves[0], forward[0], false),
                end(offset.curves[n - 1], forward[n - 1], true),
            );
            start.is_some() && start == finish
        };
        let corners = if closed { n } else { n - 1 };
        for i in 0..corners {
            let j = (i + 1) % n;
            let (s1, s2) = (offset.curves[i], offset.curves[j]);
            let (r1, r2) = (offset.results[i], offset.results[j]);
            // The point the sources share, and the one the results share.
            let Some(at) =
                end(s1, forward[i], true).filter(|p| end(s2, forward[j], false) == Some(*p))
            else {
                continue;
            };
            let Some(corner) = shared_end(&index, r1, r2) else {
                continue;
            };
            let by_definition = touching.contains(&pair(s1, s2))
                || (fixed(&index, s1) && fixed(&index, s2) && touch_at(&index, s1, s2, at));
            // (Two lines touch only along each other: no corner. Results
            // tangent by a constraint of their own need no binding.)
            if by_definition
                && (round(&index, r1) || round(&index, r2))
                && touching.insert(pair(r1, r2))
            {
                out.push(Binding {
                    owner: offset.id,
                    kind: BindingKind::TangentAt {
                        a: r1,
                        b: r2,
                        at: corner,
                    },
                });
            }
        }
    }
    out
}

/// The helper line of a result arc or circle (mitcad#358) as
/// [`independent`] finds it by its constraints.
struct Helper {
    line: EntityUid,
    /// The source's (and the result's) centre.
    center: EntityUid,
    /// The end on the source and its point-on-curve constraint.
    on_source: EntityUid,
    held_on_source: ConstraintUid,
    /// The end on the result.
    on_result: EntityUid,
    /// The driving length that holds the result's radius.
    length: Option<ConstraintUid>,
}

/// What the solve holds the offsets made by constraints with besides their
/// stored constraints, and which of those it leaves out (mitcad#1171), so
/// that an offset adds no constraint that follows from the others and no
/// degree of freedom its shape does not have. `corners` are the offsets'
/// tangent corners ([`tangent_corners`]). Of a tangent chain each corner was
/// held three times (the line at its distance, the arc's radius by its
/// helper, the tangency), half of an offset's dimensions were reported
/// redundant, and each helper slid along its arc.
///
/// - A result line that touches a result arc at a held corner is placed by
///   that arc's circle (about the source's centre, its radius held): its
///   distance dimension is left out. One held so at both ends is the
///   tangent of two circles: its parallel is left out too.
/// - A result arc that touches the previous result arc at a held corner
///   takes its radius from it: its helper's length is left out (along the
///   chain from an arc whose own helper holds its radius; a closed chain of
///   arcs alone keeps its first arc's length only). Such a chain's arcs and
///   corners close the loop by themselves (their equations depend on each
///   other once); the solve does not report that, as it is the offset's own
///   ([`crate::solve::Extra::restating`]).
/// - Each helper's end on an arc source is held at the arc's middle (in
///   place of its point-on-curve), the one on a circle level with the
///   centre, so that the helper does not slide. A helper something else
///   constrains too is left as it is.
///
/// Nothing is stored: the stored constraints stay as they are, and older
/// designs solve the same way. Where a source's corner no longer touches by
/// its definition (no binding), the line keeps its own constraints.
pub fn independent<P>(
    offsets: &[SketchOffset<ParamId>],
    entities: &[Entity],
    constraints: &[super::Constraint],
    dimensions: &[super::Dimension<P>],
    corners: &[super::pattern::Binding],
) -> (Vec<super::pattern::Binding>, BTreeSet<ConstraintUid>) {
    use super::pattern::{Binding, BindingKind};
    use std::collections::BTreeMap;
    let index = EntityIndex::new(entities);
    let kind = |id: EntityUid| index.get(id).map(|e| &e.kind);
    let pair = |a: EntityUid, b: EntityUid| if a <= b { (a, b) } else { (b, a) };
    let active: Vec<&super::Constraint> = constraints.iter().filter(|c| c.active).collect();
    let mut on: BTreeMap<(EntityUid, EntityUid), ConstraintUid> = BTreeMap::new();
    let mut parallels: BTreeMap<(EntityUid, EntityUid), ConstraintUid> = BTreeMap::new();
    for c in &active {
        match c.kind {
            ConstraintKind::Coincident {
                point,
                entity: Ref::Curve(curve),
            } => {
                on.insert((point, curve), c.id);
            }
            ConstraintKind::Parallel { a, b } => {
                parallels.insert(pair(a, b), c.id);
            }
            _ => {}
        }
    }
    let mut distances: BTreeMap<(EntityUid, EntityUid), ConstraintUid> = BTreeMap::new();
    let mut lengths: BTreeMap<EntityUid, ConstraintUid> = BTreeMap::new();
    for d in dimensions.iter().filter(|d| d.active && d.value.is_some()) {
        match d.kind {
            DimensionKind::LineDistance { a, b } => {
                distances.insert(pair(a, b), d.id);
            }
            DimensionKind::Length { line } => {
                lengths.insert(line, d.id);
            }
            _ => {}
        }
    }
    // How many active constraints and dimensions name each entity.
    let mut uses: BTreeMap<Ref, usize> = BTreeMap::new();
    let refs = active.iter().flat_map(|c| c.kind.refs()).chain(
        dimensions
            .iter()
            .filter(|d| d.active)
            .flat_map(|d| d.kind.refs()),
    );
    for r in refs {
        *uses.entry(r).or_default() += 1;
    }
    let used = |r: Ref| uses.get(&r).copied().unwrap_or(0);
    let mut bindings = Vec::new();
    let mut left_out = BTreeSet::new();
    for offset in offsets.iter().filter(|o| !o.derived) {
        let n = offset.curves.len();
        if n == 0 || offset.results.len() != n {
            continue;
        }
        let helper = |source: EntityUid, result: EntityUid| -> Option<Helper> {
            let center = match kind(source)? {
                EntityKind::Arc { center, .. } | EntityKind::Circle { center, .. } => *center,
                _ => return None,
            };
            offset.helpers.iter().find_map(|&line| {
                let Some(EntityKind::Line { start, end, .. }) = kind(line) else {
                    return None;
                };
                if !on.contains_key(&(center, line)) {
                    return None;
                }
                // `a` on the source, `b` on the result.
                let held = |a: EntityUid, b: EntityUid| {
                    on.get(&(a, source))
                        .copied()
                        .filter(|_| on.contains_key(&(b, result)))
                };
                let (on_source, on_result, held_on_source) =
                    match (held(*start, *end), held(*end, *start)) {
                        (Some(k), _) => (*start, *end, k),
                        (None, Some(k)) => (*end, *start, k),
                        (None, None) => return None,
                    };
                Some(Helper {
                    line,
                    center,
                    on_source,
                    held_on_source,
                    on_result,
                    length: lengths.get(&line).copied(),
                })
            })
        };
        let helpers: Vec<Option<Helper>> = offset
            .curves
            .iter()
            .zip(&offset.results)
            .map(|(&s, &r)| helper(s, r))
            .collect();
        // Whose radius is held: by its helper's length, or by the arc
        // before it at a held corner.
        let mut held: Vec<bool> = helpers
            .iter()
            .map(|h| h.as_ref().is_some_and(|h| h.length.is_some()))
            .collect();
        let bound: BTreeSet<(EntityUid, EntityUid)> = corners
            .iter()
            .filter(|b| b.owner == offset.id)
            .filter_map(|b| match b.kind {
                BindingKind::TangentAt { a, b, .. } => Some(pair(a, b)),
                _ => None,
            })
            .collect();
        let results = &offset.results;
        let arcs_touch = |i: usize| {
            let j = (i + 1) % n;
            round(&index, results[i])
                && round(&index, results[j])
                && bound.contains(&pair(results[i], results[j]))
        };
        // (Two curves share both their ends only where they close a chain
        // of two, which has no tangent corners.)
        let closed = n > 2 && shared_end(&index, results[n - 1], results[0]).is_some();
        // A closed chain of arcs alone has one family of offsets: its
        // corners pass the radius all the way round, so one arc's own
        // length holds them all, from it round to the arc before it.
        let order: Vec<usize> = if closed {
            match (0..n).find(|&i| !arcs_touch(i)) {
                Some(k) => (1..=n).map(|t| (k + t) % n).collect(),
                None => match (0..n).find(|&i| held[i]) {
                    Some(k) => (0..n - 1).map(|t| (k + t) % n).collect(),
                    None => Vec::new(),
                },
            }
        } else {
            (0..n - 1).collect()
        };
        for i in order {
            let j = (i + 1) % n;
            if arcs_touch(i) && held[i] {
                if let Some(length) = helpers[j].as_ref().and_then(|h| h.length) {
                    left_out.insert(length);
                }
                held[j] = true;
            }
        }
        // Lines placed by the circles they touch.
        for (&source, &result) in offset.curves.iter().zip(results) {
            if !matches!(kind(result), Some(EntityKind::Line { .. })) {
                continue;
            }
            let touching = results
                .iter()
                .zip(&held)
                .filter(|&(&other, &h)| {
                    h && round(&index, other) && bound.contains(&pair(result, other))
                })
                .count();
            let key = pair(source, result);
            if touching >= 1
                && let Some(&k) = distances.get(&key)
            {
                left_out.insert(k);
            }
            if touching >= 2
                && let Some(&k) = parallels.get(&key)
            {
                left_out.insert(k);
            }
        }
        // Helpers held in place.
        for (h, &source) in helpers.iter().zip(&offset.curves) {
            let Some(h) = h else { continue };
            let own = 1 + usize::from(h.length.is_some());
            if used(Ref::Curve(h.line)) != own
                || used(Ref::Point(h.on_source)) != 1
                || used(Ref::Point(h.on_result)) != 1
            {
                continue;
            }
            let anchor = match kind(source) {
                Some(EntityKind::Arc { .. }) => {
                    left_out.insert(h.held_on_source);
                    BindingKind::Midpoint {
                        point: h.on_source,
                        curve: source,
                    }
                }
                Some(EntityKind::Circle { .. }) => BindingKind::HorizontalPoints {
                    a: h.center,
                    b: h.on_source,
                },
                _ => continue,
            };
            bindings.push(Binding {
                owner: offset.id,
                kind: anchor,
            });
        }
    }
    (bindings, left_out)
}

/// The constraints and dimensions an offset made by constraints holds its
/// results with (`modify.rs`): each line's parallel and distance to its
/// source, each helper's point-on-curve constraints and length; the
/// offset's own id for its tangent corners and helper anchors (mitcad#1171:
/// a warning about redundant constraints names the offset they come from).
pub fn constraints_of<P>(
    offset: &SketchOffset<ParamId>,
    entities: &[Entity],
    constraints: &[super::Constraint],
    dimensions: &[super::Dimension<P>],
) -> BTreeSet<ConstraintUid> {
    let index = EntityIndex::new(entities);
    let pairs: BTreeSet<(EntityUid, EntityUid)> = offset
        .curves
        .iter()
        .zip(&offset.results)
        .flat_map(|(&s, &r)| [(s, r), (r, s)])
        .collect();
    let ends: BTreeSet<EntityUid> = offset
        .helpers
        .iter()
        .filter_map(|h| index.get(*h)?.kind.ends())
        .flat_map(|(a, b)| [a, b])
        .collect();
    let mut out: BTreeSet<ConstraintUid> = constraints
        .iter()
        .filter(|c| match c.kind {
            ConstraintKind::Parallel { a, b } => pairs.contains(&(a, b)),
            ConstraintKind::Coincident {
                point,
                entity: Ref::Curve(curve),
            } => {
                offset.helpers.contains(&curve)
                    || (ends.contains(&point)
                        && (offset.curves.contains(&curve) || offset.results.contains(&curve)))
            }
            _ => false,
        })
        .map(|c| c.id)
        .collect();
    out.extend(
        dimensions
            .iter()
            .filter(|d| match d.kind {
                DimensionKind::LineDistance { a, b } => pairs.contains(&(a, b)),
                DimensionKind::Length { line } => offset.helpers.contains(&line),
                _ => false,
            })
            .map(|d| d.id),
    );
    out.insert(offset.id);
    out
}

/// The end point two curves share.
fn shared_end(index: &EntityIndex<'_>, a: EntityUid, b: EntityUid) -> Option<EntityUid> {
    let (a0, a1) = index.get(a)?.kind.ends()?;
    let (b0, b1) = index.get(b)?.kind.ends()?;
    [a0, a1].into_iter().find(|p| *p == b0 || *p == b1)
}

/// Whether a curve is fixed (a projected edge).
fn fixed(index: &EntityIndex<'_>, id: EntityUid) -> bool {
    index.get(id).is_some_and(|e| e.fixed)
}

/// Whether a curve is an arc or a circle.
fn round(index: &EntityIndex<'_>, id: EntityUid) -> bool {
    matches!(
        index.get(id).map(|e| &e.kind),
        Some(EntityKind::Arc { .. } | EntityKind::Circle { .. })
    )
}

/// Whether a line and an arc, or two arcs, touch at their shared point `at`:
/// the line runs along the circle's tangent there, or the two centres lie on
/// one line with it, within the angle at which a line [`TOUCH`]es a circle
/// (the square root of twice that, relative to the radius).
fn touch_at(index: &EntityIndex<'_>, a: EntityUid, b: EntityUid, at: EntityUid) -> bool {
    let point = |id: EntityUid| index.point(id);
    // An arc's centre, when its ends lie on one circle (fixed geometry that
    // contradicts itself, a projection followed past where its curves still
    // meet, is no corner to hold: the solve says what contradicts).
    let circle = |id: EntityUid| match index.get(id).map(|e| &e.kind) {
        Some(EntityKind::Arc { center, start, end }) => {
            let (c, s, e) = (point(*center)?, point(*start)?, point(*end)?);
            let (rs, re) = (norm(sub(s, c)), norm(sub(e, c)));
            ((rs - re).abs() <= TOUCH * rs.max(1.0)).then_some(c)
        }
        _ => None,
    };
    let line = |id: EntityUid| match index.get(id).map(|e| &e.kind) {
        Some(EntityKind::Line { start, end, .. }) => point(*start).zip(point(*end)),
        _ => None,
    };
    let Some(p) = point(at) else {
        return false;
    };
    // The sine of the angle the curves meet at, against the tangent's for
    // the smaller radius.
    let within = |sine: f64, r: f64| r > 0.0 && sine.abs() <= (2.0 * TOUCH * r.max(1.0) / r).sqrt();
    match (line(a).or(line(b)), circle(a), circle(b)) {
        (Some((s, e)), Some(c), None) | (Some((s, e)), None, Some(c)) => {
            let (u, v) = (sub(e, s), sub(p, c));
            let (lu, lv) = (norm(u), norm(v));
            lu > 0.0 && within(dot(u, v) / (lu * lv), lv)
        }
        // Two arcs of one circle (centres within rounding of each other,
        // mitcad#709) are no corner that touching holds.
        (None, Some(c1), Some(c2)) => {
            let (v1, v2) = (sub(p, c1), sub(p, c2));
            let (l1, l2) = (norm(v1), norm(v2));
            l1 > 0.0
                && l2 > 0.0
                && norm(sub(c2, c1)) > TOUCH * (l1 + l2).max(1.0)
                && within(cross(v1, v2) / (l1 * l2), l1.min(l2))
        }
        _ => false,
    }
}

/// A chain's curves (in its order) each running towards the next where
/// their ends meet, and whether the last meets the first: for a chain whose
/// curves meet at coincident ends, not shared points.
fn joined_by_ends(chain: &[(Curve2, bool)]) -> (Vec<(Curve2, bool)>, bool) {
    let ends = |c: &Curve2| {
        let (lo, hi) = c.domain();
        (c.eval(lo)[0], c.eval(hi)[0])
    };
    let near = |p: P2, c: &Curve2| {
        let (a, b) = ends(c);
        dist(p, a).min(dist(p, b))
    };
    let n = chain.len();
    let mut out = Vec::with_capacity(n);
    for (i, (curve, forward)) in chain.iter().enumerate() {
        let (start, end) = ends(curve);
        let forward = if n < 2 {
            *forward
        } else if i + 1 < n {
            let next = &chain[i + 1].0;
            near(end, next) <= near(start, next)
        } else {
            let previous = &chain[i - 1].0;
            near(start, previous) <= near(end, previous)
        };
        out.push((curve.clone(), forward));
    }
    let closed = n > 1 && {
        let size = chain
            .iter()
            .map(|(c, _)| {
                let (lo, hi) = c.bounds();
                dist(lo, hi)
            })
            .fold(1.0, f64::max);
        let (first, last) = (&out[0], &out[n - 1]);
        let start = if first.1 {
            ends(&first.0).0
        } else {
            ends(&first.0).1
        };
        let finish = if last.1 {
            ends(&last.0).1
        } else {
            ends(&last.0).0
        };
        dist(start, finish) <= (1e-6 * size).max(TOLERANCE)
    };
    (out, closed)
}

/// The joins as the offset was made: round where it has a corner arc,
/// else shared when the results share a point, else trimmed.
fn stored_joins(
    offset: &SketchOffset<ParamId>,
    chain: &[(Curve2, bool)],
    closed: bool,
) -> Vec<Join> {
    let n = chain.len();
    let count = if closed { n } else { n.saturating_sub(1) };
    (0..count)
        .map(|j| {
            if offset.corners.iter().any(|c| c.join as usize == j) {
                return Join::Round;
            }
            let (a, b) = (chain[j].0.clone(), chain[(j + 1) % n].0.clone());
            // A smooth join of the sources gives a smooth join of the
            // offsets.
            let ta = {
                let (lo, hi) = a.domain();
                let t = if chain[j].1 { hi } else { lo };
                let d = a.eval(t)[1];
                unit(if chain[j].1 { d } else { scale(d, -1.0) })
            };
            let tb = {
                let (lo, hi) = b.domain();
                let t = if chain[(j + 1) % n].1 { lo } else { hi };
                let d = b.eval(t)[1];
                unit(if chain[(j + 1) % n].1 {
                    d
                } else {
                    scale(d, -1.0)
                })
            };
            if cross(ta, tb).abs() <= 1e-7 && dot(ta, tb) > 0.0 {
                Join::Shared
            } else {
                Join::Trim
            }
        })
        .collect()
}

impl SketchEdit<'_> {
    /// The derived offset of a chain with ellipses or splines (see the
    /// module). `curves` are in chain order with their directions.
    pub(crate) fn derived_offset(
        &mut self,
        chain: &[(EntityUid, bool)],
        closed: bool,
        distance: &ValueInput,
        d: f64,
    ) -> Result<(ConstraintUid, Vec<EntityUid>), String> {
        let solved = self.solved()?;
        let mut sources = Vec::new();
        for (id, forward) in chain {
            let c = solved
                .curves
                .get(id)
                .cloned()
                .ok_or("a curve without geometry")?;
            sources.push((c, *forward));
        }
        let computed = compute(&sources, d, closed, None)?;
        let n = computed.made.len();
        // Curve ids first (results, then corner arcs), so that names do not
        // depend on how many points the splines take.
        let ids = self.ids(n);
        let arc_ids = self.ids(computed.corners.len());
        // Points: shared at smooth and trimmed joins, own at round corners.
        let mut starts: Vec<Option<EntityUid>> = vec![None; n];
        let mut ends: Vec<Option<EntityUid>> = vec![None; n];
        for (j, join) in computed.joins.iter().enumerate() {
            let next = (j + 1) % n;
            if *join != Join::Round {
                let p = self.add_point(computed.ends[j].1);
                ends[j] = Some(p);
                starts[next] = Some(p);
            }
        }
        let mut corner_points: Vec<(usize, EntityUid, EntityUid)> = Vec::new();
        for (j, _) in &computed.corners {
            let next = (j + 1) % n;
            let a = self.add_point(computed.ends[*j].1);
            let b = self.add_point(computed.ends[next].0);
            ends[*j] = Some(a);
            starts[next] = Some(b);
            corner_points.push((*j, a, b));
        }
        let single_closed = n == 1 && sources[0].0.is_closed();
        for i in 0..n {
            if matches!(computed.made[i], Made::Circle { .. }) {
                continue;
            }
            if starts[i].is_none() {
                starts[i] = Some(self.add_point(computed.ends[i].0));
            }
            if ends[i].is_none() {
                ends[i] = Some(if single_closed {
                    starts[i].expect("made above")
                } else {
                    self.add_point(computed.ends[i].1)
                });
            }
        }
        for (i, m) in computed.made.iter().enumerate() {
            let (s, e) = (starts[i], ends[i]);
            let kind = match m {
                Made::Line(..) => EntityKind::Line {
                    start: s.expect("a start"),
                    end: e.expect("an end"),
                    centerline: false,
                },
                Made::Arc { center, .. } => {
                    let c = self.add_point(*center);
                    // Arcs run counter-clockwise; a backward piece's chain
                    // end is its start.
                    let (s, e) = if chain[i].1 { (s, e) } else { (e, s) };
                    EntityKind::Arc {
                        center: c,
                        start: s.expect("a start"),
                        end: e.expect("an end"),
                    }
                }
                Made::Circle { center, radius } => EntityKind::Circle {
                    center: self.add_point(*center),
                    radius: *radius,
                    start: 0.0,
                },
                Made::Spline { knots, control } => {
                    let last = control.len() - 1;
                    let mut ids = Vec::with_capacity(control.len());
                    for (k, p) in control.iter().enumerate() {
                        ids.push(match k {
                            0 => s.expect("a start"),
                            k if k == last => e.expect("an end"),
                            _ => self.add_point(*p),
                        });
                    }
                    EntityKind::Spline {
                        degree: 3,
                        control: ids,
                        weights: Vec::new(),
                        knots: spline_nurbs(knots, control).knots,
                    }
                }
            };
            self.push(Entity::new(ids[i], kind));
        }
        let mut corners = Vec::new();
        for (((j, made), (_, a, b)), arc) in
            computed.corners.iter().zip(&corner_points).zip(arc_ids)
        {
            let Made::Arc { center, start, .. } = made else {
                continue;
            };
            let c = self.add_point(*center);
            // `a` ends the curve before the corner, `b` starts the next.
            let at_a = self.at(*a)?;
            let (s, e) = if dist(at_a, *start) < dist(self.at(*b)?, *start) {
                (*a, *b)
            } else {
                (*b, *a)
            };
            self.push(Entity::new(
                arc,
                EntityKind::Arc {
                    center: c,
                    start: s,
                    end: e,
                },
            ));
            corners.push(OffsetCorner {
                join: *j as u32,
                arc,
            });
        }
        let id = self.constraint_id();
        let param = self.offset_param(id, distance, None)?;
        self.def.offsets.push(SketchOffset {
            id,
            curves: chain.iter().map(|(c, _)| *c).collect(),
            distance: param,
            left: d < 0.0,
            results: ids.clone(),
            corners,
            helpers: Vec::new(),
            derived: true,
        });
        self.report.constraints.push(id);
        Ok((id, ids))
    }

    /// The parameter of an offset's distance (its magnitude).
    pub(crate) fn offset_param(
        &mut self,
        id: ConstraintUid,
        distance: &ValueInput,
        old: Option<ParamId>,
    ) -> Result<ParamId, String> {
        let value = match distance {
            ValueInput::Number(v) => ValueInput::Number(v.abs()),
            other => other.clone(),
        };
        let slot = format!("offsets[{id}].distance");
        let comment = format!("{} offset", self.owner_name());
        let owner = self.owner();
        let (param, created) =
            resolve_value(self.params, &slot, &value, owner, &comment, old.as_ref())
                .map_err(|e| e.to_string())?;
        self.report.parameters.extend(created);
        Ok(param)
    }

    /// Finds an exact offset's distance dimensions from its curve pairs.
    /// Older designs have the same structure, without stored dimension ids.
    fn offset_dimensions(&self, offset: &SketchOffset<ParamId>) -> BTreeSet<ConstraintUid> {
        let on = |point, curve| {
            self.def.constraints.iter().any(|c| {
                c.kind
                    == ConstraintKind::Coincident {
                        point,
                        entity: Ref::Curve(curve),
                    }
            })
        };
        self.def
            .dimensions
            .iter()
            .filter(|d| {
                offset
                    .curves
                    .iter()
                    .zip(&offset.results)
                    .any(|(&a, &b)| match d.kind {
                        DimensionKind::LineDistance { a: x, b: y } => {
                            (x == a && y == b) || (x == b && y == a)
                        }
                        DimensionKind::Length { line } => {
                            let Some(e) = self.def.entity(line) else {
                                return false;
                            };
                            let EntityKind::Line { start, end, .. } = e.kind else {
                                return false;
                            };
                            let Some(
                                EntityKind::Arc { center, .. } | EntityKind::Circle { center, .. },
                            ) = self.def.entity(a).map(|e| &e.kind)
                            else {
                                return false;
                            };
                            on(*center, line)
                                && ((on(start, a) && on(end, b)) || (on(end, a) && on(start, b)))
                        }
                        _ => false,
                    })
            })
            .map(|d| d.id)
            .collect()
    }

    /// Changes an offset's distance (a number or an expression sets its
    /// parameter, a name uses that parameter) or side.
    pub fn edit_offset(
        &mut self,
        id: ConstraintUid,
        distance: Option<&ValueInput>,
        flip: bool,
    ) -> Result<(), String> {
        let i = self
            .def
            .offsets
            .iter()
            .position(|o| o.id == id)
            .ok_or_else(|| format!("offset {id} does not exist"))?;
        let offset = self.def.offsets[i].clone();
        if let Some(v) = distance {
            if let ValueInput::Number(n) = v
                && *n < 0.0
            {
                return Err("the offset distance is not negative; flip the side instead".to_owned());
            }
            let param = self.offset_param(id, v, Some(offset.distance))?;
            if param != offset.distance && !offset.derived {
                // Other dimensions may deliberately share the old parameter.
                let dimensions = self.offset_dimensions(&offset);
                for d in &mut self.def.dimensions {
                    if dimensions.contains(&d.id) && d.value == Some(offset.distance) {
                        d.value = Some(param);
                    }
                }
            }
            self.def.offsets[i].distance = param;
        }
        if flip {
            let offset = self.def.offsets[i].clone();
            if offset.derived {
                // The corners change kind: made again.
                let made: BTreeSet<EntityUid> = offset.made().into_iter().collect();
                let name = self.params.name(offset.distance);
                self.drop_offset_curves(&made);
                self.def.offsets.retain(|o| o.id != id);
                let sign = if offset.left { 1.0 } else { -1.0 };
                let value = self
                    .params
                    .value(offset.distance)
                    .ok_or("the offset's parameter is gone")?;
                let index = EntityIndex::new(&self.def.entities);
                let forward = chain_directions(&offset.curves, &index);
                let chain: Vec<(EntityUid, bool)> =
                    offset.curves.iter().copied().zip(forward).collect();
                let closed = self.chain_closed(&chain);
                // The results keep their ids.
                self.reuse = offset.results.clone();
                let made =
                    self.derived_offset(&chain, closed, &ValueInput::Name(name), sign * value);
                self.reuse.clear();
                self.renumber_offset(made?.0, id);
            } else {
                let made: BTreeSet<EntityUid> = offset
                    .results
                    .iter()
                    .chain(&offset.helpers)
                    .copied()
                    .collect();
                let name = self.params.name(offset.distance);
                let value = self
                    .params
                    .value(offset.distance)
                    .ok_or("the offset's parameter is gone")?;
                self.drop_offset_curves(&made);
                self.def.offsets.retain(|o| o.id != id);
                let sign = if offset.left { 1.0 } else { -1.0 };
                self.reuse = offset.results.clone();
                let made = self.offset_with(&offset.curves, &ValueInput::Name(name), sign * value);
                self.reuse.clear();
                self.renumber_offset(made?.0, id);
            }
        }
        Ok(())
    }

    /// Fits the pieces of derived spline offsets to the current geometry
    /// again (every edit does): a source that changed shape may need more
    /// or fewer control points. Only inner control points change; the
    /// curves keep their ids.
    pub(crate) fn refit_offsets(&mut self) -> Result<(), String> {
        if !self.def.offsets.iter().any(|o| o.derived) {
            return Ok(());
        }
        let solved = self.solved()?;
        let mut updates: Vec<(EntityUid, Vec<f64>, Vec<P2>)> = Vec::new();
        {
            let index = EntityIndex::new(&self.def.entities);
            for offset in self.def.offsets.iter().filter(|o| o.derived) {
                let d = self
                    .params
                    .value(offset.distance)
                    .ok_or("the offset's parameter is gone")?;
                let d = if offset.left { -d } else { d };
                let (chain, closed) = sources(offset, &index, &solved.curves)?;
                let structure = Structure {
                    knots: vec![None; chain.len()],
                    joins: stored_joins(offset, &chain, closed),
                };
                let computed = compute(&chain, d, closed, Some(&structure))
                    .map_err(|e| format!("offset {}: {e}", offset.id))?;
                for (made, curve) in computed.made.iter().zip(&offset.results) {
                    if let Made::Spline { knots, control } = made {
                        updates.push((*curve, knots.clone(), control.clone()));
                    }
                }
            }
        }
        for (curve, knots, control) in updates {
            let Some(EntityKind::Spline {
                control: ids,
                knots: old_knots,
                ..
            }) = self.def.entity(curve).map(|e| e.kind.clone())
            else {
                continue;
            };
            let full = spline_nurbs(&knots, &control).knots;
            if ids.len() == control.len() && old_knots == full {
                continue;
            }
            let (first, last) = (ids[0], ids[ids.len() - 1]);
            let old_inner = &ids[1..ids.len() - 1];
            let wanted = control.len() - 2;
            let mut inner: Vec<EntityUid> = old_inner.iter().copied().take(wanted).collect();
            for p in &control[1 + inner.len()..control.len() - 1] {
                inner.push(self.add_point(*p));
            }
            let surplus: BTreeSet<EntityUid> = old_inner.iter().copied().skip(wanted).collect();
            for p in &surplus {
                self.report.removed.push(format!("p{}", p.0));
            }
            self.def.entities.retain(|e| !surplus.contains(&e.id));
            let mut all = vec![first];
            all.extend(inner);
            all.push(last);
            if let Some(EntityKind::Spline { control, knots, .. }) =
                self.def.entity_mut(curve).map(|e| &mut e.kind)
            {
                *control = all;
                *knots = full;
            }
        }
        Ok(())
    }

    /// Gives a remade offset its old id.
    fn renumber_offset(&mut self, new: ConstraintUid, old: ConstraintUid) {
        if let Some(o) = self.def.offsets.iter_mut().find(|o| o.id == new) {
            o.id = old;
        }
        self.report.constraints.retain(|c| *c != new);
    }

    /// Removes an offset's curves with the points only they use.
    fn drop_offset_curves(&mut self, curves: &BTreeSet<EntityUid>) {
        let mut doomed = curves.clone();
        for c in curves {
            if let Some(e) = self.def.entity(*c) {
                for p in e.kind.points() {
                    let used = self
                        .def
                        .entities
                        .iter()
                        .any(|o| !curves.contains(&o.id) && o.kind.points().contains(&p));
                    if !used {
                        doomed.insert(p);
                    }
                }
            }
        }
        // Keep the record while its curves go.
        let saved = std::mem::take(&mut self.def.offsets);
        self.drop_entities(&doomed);
        self.def.offsets = saved;
    }

    /// Whether a chain in order ends where it starts.
    pub(crate) fn chain_closed(&self, chain: &[(EntityUid, bool)]) -> bool {
        if chain.len() < 2 {
            return false;
        }
        let end = |(id, forward): (EntityUid, bool), last: bool| {
            self.def
                .entity(id)
                .and_then(|e| e.kind.ends())
                .map(|(s, e)| if forward != last { s } else { e })
        };
        end(chain[0], false) == end(chain[chain.len() - 1], true)
    }

    /// The derived offset that computes an entity (one of its curves or
    /// their points), if any.
    pub(crate) fn computing_offset(&self, id: EntityUid) -> Option<&SketchOffset<ParamId>> {
        let index = EntityIndex::new(&self.def.entities);
        self.def
            .offsets
            .iter()
            .filter(|o| o.derived)
            .find(|o| derived_entities(std::slice::from_ref(*o), &index).contains(&id))
    }

    /// The points a drag or a move of an entity pulls: a point itself, a
    /// curve's points. Of what a derived offset computes, the points of the
    /// offset's whole source chain: the curves follow their source at the
    /// offset's distance, so the source moves by the same motion and the
    /// offset goes with the pointer, as an exact offset's copy does with
    /// its distance dimension driving. The distance (a parameter) changes
    /// only in Edit Offset.
    pub(crate) fn moved_points(&self, id: EntityUid) -> Result<Vec<EntityUid>, String> {
        if let Some(offset) = self.computing_offset(id) {
            let mut points = BTreeSet::new();
            for c in &offset.curves {
                points.extend(self.curve(*c)?.kind.points());
            }
            return Ok(points.into_iter().collect());
        }
        match self.def.entity(id) {
            Some(e) if e.is_point() => Ok(vec![id]),
            Some(e) => Ok(e.kind.points()),
            None => Err(format!("entity {} does not exist", id.0)),
        }
    }

    /// Refuses a change of the shape of a curve a derived offset computes
    /// and of a projected one (mitcad#357): a
    /// trim, an extension, a fillet or a chamfer would stretch it back to
    /// its source at the next evaluation or leave a fixed piece. Convert
    /// it to sketch geometry first.
    pub(crate) fn shape_editable(&self, id: EntityUid) -> Result<(), String> {
        self.editable(id)?;
        let Some(e) = self.def.entity(id) else {
            return Ok(());
        };
        let listed = self
            .def
            .projections
            .iter()
            .any(|p| p.entities.contains(&Ref::Curve(id)));
        if e.reference || listed {
            return Err(format!(
                "{} is projected and keeps the shape of its source; convert it to sketch \
                 geometry first (sketch.convert_projected)",
                e.as_ref()
            ));
        }
        Ok(())
    }

    /// Refuses an edit of a curve or point a derived offset computes.
    pub(crate) fn editable(&self, id: EntityUid) -> Result<(), String> {
        let index = EntityIndex::new(&self.def.entities);
        for offset in self.def.offsets.iter().filter(|o| o.derived) {
            let made = derived_entities(std::slice::from_ref(offset), &index);
            if made.contains(&id) {
                return Err(format!(
                    "{} follows offset {}; change its source or the offset",
                    self.def
                        .entity(id)
                        .map_or_else(|| format!("{}", id.0), |e| e.as_ref().to_string()),
                    offset.id
                ));
            }
        }
        Ok(())
    }
}
