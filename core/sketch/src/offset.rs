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
//!   their ids and names; an edit that changes a corner's kind fails. A
//!   drag or a move of what it computes moves the source chain by the same
//!   motion ([`SketchEdit::moved_points`]): the offset keeps its distance
//!   and follows the pointer.
//! - What vanishes at the distance is left out, in both kinds (mitcad#1234):
//!   an arc whose offset radius is not above zero, a curve whose offset
//!   folds over or turns inside out because its neighbours' offsets cross
//!   beyond it. The offsets of the curves on either side are cut or run on
//!   to where they cross, so that the overlap on the source's side is
//!   trimmed away and the outer outline is kept (a hexagon with 2 mm
//!   corner arcs offset 2 mm or more inward is a sharp hexagon). Where
//!   they do not cross (parallel lines) the offset fails naming the curve.
//!   The record lists those curves as `dropped`; an edit after which other
//!   curves vanish (the distance, or the source, changed) makes the offset
//!   again ([`SketchEdit::settle_offsets`]): the curves of source curves
//!   kept before and after keep their ids, one that comes back gets a new
//!   id, and the user's constraints on the results go with them, but for
//!   those on a curve that vanished, which a warning names. A parameter
//!   set outside the sketch or a linked projection that follows its source
//!   makes the offset again at the sketch's evaluation the same way
//!   ([`retrim_for_evaluation`]). Curves whose offsets lie wholly in the
//!   overlap on the source's side (`buried`: walls of a neck whose
//!   offsets pass each other) are left out as well.
//! - What is left may fall into several closed outlines, or leave two
//!   lines on one line (mitcad#1244). Across a waist narrower than twice
//!   the distance, the offsets of the curves on either side of it do not
//!   meet; each meets the offset of a curve on the other side of the waist
//!   instead, so the outline is split into two, which the record lists in
//!   `outlines`. Two lines whose offsets lie on one line across what
//!   vanished (a notch the offset closes, a wall on either side of a waist)
//!   are one result, that of the first ([`OffsetMerge`]): the later line is
//!   listed in `dropped` and in `merged`. An offset of an offset's results
//!   is made again after it when it is made again
//!   ([`SketchEdit::remake_offset`]), from the results it has then.
//! - Open chains and loops that cross (mitcad#1246): an open chain falls
//!   into parts the same way, and where the offsets on either side of what
//!   vanished do not meet at all, it ends there on both sides when those
//!   ends are clear of the chain (`ends` lists the last result of each open
//!   outline). Two arcs on one circle across what vanished are one result,
//!   as two lines on one line are; one left alone runs round its whole
//!   circle (a circle with a notch the offset closes). Two offsets that
//!   cross each other on the outline's way, at a point of the offset, split
//!   the outline there as a waist does: the loop on the source's side is
//!   trimmed as above (a tapered waist). An outline too small to keep any
//!   curve of its own (a lobe narrower than twice the distance) is left out,
//!   and a note says so, when the rest is kept.

use std::collections::{BTreeMap, BTreeSet};

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

/// A source curve whose offset runs on along the result of another
/// (mitcad#1244): two lines whose offsets lie on one line across what
/// vanished (a notch the offset closes) have one result, `into`'s, held by
/// `into`'s constraints. `curve` is listed in `dropped` as well.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OffsetMerge {
    #[serde(with = "curve_serde")]
    pub curve: EntityUid,
    #[serde(with = "curve_serde")]
    pub into: EntityUid,
}

/// Outlines as lists of curve names (`[["c20", "c21"], ["c22", "c23"]]`).
mod outlines_serde {
    use serde::{Deserialize, Serialize};

    use crate::ids::EntityUid;

    pub fn serialize<S: serde::Serializer>(
        outlines: &[Vec<EntityUid>],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let names: Vec<Vec<String>> = outlines
            .iter()
            .map(|o| o.iter().map(|id| format!("c{}", id.0)).collect())
            .collect();
        names.serialize(s)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Vec<Vec<EntityUid>>, D::Error> {
        Vec::<Vec<String>>::deserialize(d)?
            .iter()
            .map(|o| {
                o.iter()
                    .map(|n| EntityUid::parse_curve(n).map_err(serde::de::Error::custom))
                    .collect()
            })
            .collect()
    }
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
    /// The offset of each source curve, in chain order, but for those in
    /// `dropped`.
    #[serde(with = "super::curves_serde")]
    pub results: Vec<EntityUid>,
    /// The source curves that vanish at the distance (mitcad#1234): an arc
    /// whose offset radius is not above zero, a curve whose offset folds
    /// over or turns inside out between its neighbours'. They have no
    /// result; the results of the curves on either side meet where their
    /// offset curves cross. Also the curves in `merged`.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        with = "super::curves_serde"
    )]
    pub dropped: Vec<EntityUid>,
    /// The curves of `dropped` whose offset runs on along another curve's
    /// result (mitcad#1244), with that curve.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merged: Vec<OffsetMerge>,
    /// The results of each closed outline in order, when the offset falls
    /// into more than one (mitcad#1244: across a waist narrower than twice
    /// the distance); empty for one outline or an open chain, whose results
    /// follow each other in `results`' order. The corners of a derived
    /// offset count their `join` by the result before them in `results`.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        with = "outlines_serde"
    )]
    pub outlines: Vec<Vec<EntityUid>>,
    /// The last result of each open outline in `outlines` (mitcad#1246: an
    /// open chain that falls into parts, each ending where the offset ends
    /// on either side of what vanished); an outline whose last result is
    /// not listed closes on its first. Empty when every outline is closed.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        with = "super::curves_serde"
    )]
    pub ends: Vec<EntityUid>,
    /// Round corners of a derived offset, `join` counted between the
    /// results.
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
            dropped: self.dropped.clone(),
            merged: self.merged.clone(),
            outlines: self.outlines.clone(),
            ends: self.ends.clone(),
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

    /// The source curves that have a result, in chain order.
    pub fn kept(&self) -> Vec<EntityUid> {
        self.curves
            .iter()
            .copied()
            .filter(|c| !self.dropped.contains(c))
            .collect()
    }

    /// Each source curve that has a result, with its result.
    pub fn pairs(&self) -> Vec<(EntityUid, EntityUid)> {
        self.kept()
            .into_iter()
            .zip(self.results.iter().copied())
            .collect()
    }

    /// Whether every source curve has its result or vanishes (the user may
    /// have deleted a result of an offset made by constraints).
    pub fn whole(&self) -> bool {
        self.results.len() + self.dropped.len() == self.curves.len()
    }

    /// The indices in `curves` of those that vanish.
    pub fn dropped_indices(&self) -> Vec<usize> {
        (0..self.curves.len())
            .filter(|i| self.dropped.contains(&self.curves[*i]))
            .collect()
    }

    /// The position in `results` of the result after each along its
    /// outline: the next one (the first after the last of a `closed`
    /// chain), or as `outlines` link them (mitcad#1244); None at an open
    /// chain's end, or an open outline's (`ends`, mitcad#1246).
    pub fn links(&self, closed: bool) -> Vec<Option<usize>> {
        let m = self.results.len();
        if self.outlines.is_empty() {
            return (0..m)
                .map(|k| {
                    if k + 1 < m {
                        Some(k + 1)
                    } else if closed && m > 1 {
                        Some(0)
                    } else {
                        None
                    }
                })
                .collect();
        }
        let mut out = vec![None; m];
        for outline in &self.outlines {
            let at: Vec<usize> = outline
                .iter()
                .filter_map(|r| self.results.iter().position(|x| x == r))
                .collect();
            for (t, &k) in at.iter().enumerate() {
                out[k] = if self.ends.contains(&self.results[k]) {
                    None
                } else {
                    Some(at[(t + 1) % at.len()])
                };
            }
        }
        out
    }

    /// A source curve's name by its index in `curves`, for messages.
    pub fn namer(&self) -> impl Fn(usize) -> String + '_ {
        move |i| {
            self.curves
                .get(i)
                .map_or_else(|| format!("curve {}", i + 1), |c| format!("c{}", c.0))
        }
    }

    pub fn check(&self, index: &EntityIndex<'_>) -> Result<(), String> {
        let what = self.id.to_string();
        if self.curves.is_empty() {
            return Err(format!("{what}: no curves to offset"));
        }
        if self.derived && !self.whole() {
            return Err(format!("{what}: one result per curve that does not vanish"));
        }
        if let Some(c) = self.dropped.iter().find(|c| !self.curves.contains(c)) {
            return Err(format!("{what}: c{} vanishes but is not offset", c.0));
        }
        for m in &self.merged {
            if !self.dropped.contains(&m.curve) || !self.kept().contains(&m.into) {
                return Err(format!(
                    "{what}: c{} runs on along c{}, which is not a kept curve of the offset",
                    m.curve.0, m.into.0
                ));
            }
        }
        if !self.outlines.is_empty() {
            let listed: Vec<EntityUid> = self.outlines.iter().flatten().copied().collect();
            let once = listed.len() == self.results.len()
                && self.results.iter().all(|r| listed.contains(r));
            if !once {
                return Err(format!("{what}: the outlines do not list each result once"));
            }
        }
        let last = |c: &EntityUid| self.outlines.iter().any(|o| o.last() == Some(c));
        if let Some(c) = self.ends.iter().find(|c| !last(c)) {
            return Err(format!("{what}: c{} ends no outline of the offset", c.0));
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

    fn is_arc(&self) -> bool {
        matches!(self.curve, Curve2::Arc { .. })
    }

    /// An arc's offset radius: its radius less the distance toward its
    /// centre (the left of its own counter-clockwise direction).
    fn offset_radius(&self) -> Option<f64> {
        match self.curve {
            Curve2::Arc { radius, .. } => Some(radius - self.left),
            _ => None,
        }
    }

    /// Whether its offset may run on past its ends to meet a neighbour's: a
    /// line along itself, an arc round its circle.
    fn extends(&self) -> bool {
        matches!(self.curve, Curve2::Line { .. } | Curve2::Arc { .. })
    }

    /// About the length of its offset on chain parameters `[u0, u1]`.
    fn length(&self, u0: f64, u1: f64) -> f64 {
        let steps = if self.is_line() { 1 } else { 8 };
        (0..steps)
            .map(|k| {
                let at = |k: usize| u0 + (u1 - u0) * k as f64 / steps as f64;
                dist(self.eval(at(k)).0, self.eval(at(k + 1)).0)
            })
            .sum()
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

/// A derived offset computed: one curve per source curve that does not
/// vanish, in chain order, with its start and end in chain direction, the
/// position of the curve after each along its outline (`next`: the next
/// one, the first after the last of a closed chain, or across a split,
/// mitcad#1244; None at an open chain's end), the joins' kinds (after each
/// curve that has one after it) and round corner arcs (by join), the
/// indices of the source curves that have no curve of their own
/// (mitcad#1234), and of those the ones whose offset runs on along
/// another's, with that other (mitcad#1244), and the ones left out as a
/// part too small for the distance (mitcad#1246). The joins follow the
/// links in the order of the curves that have one ([`linked`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Computed {
    pub made: Vec<Made>,
    pub ends: Vec<(P2, P2)>,
    pub next: Vec<Option<usize>>,
    pub joins: Vec<Join>,
    pub corners: Vec<(usize, Made)>,
    pub closed: bool,
    pub dropped: Vec<usize>,
    pub merged: Vec<(usize, usize)>,
    pub left: Vec<usize>,
}

/// Each position of a curve that has one after it along its outline, with
/// that one's (`next`, [`Computed::next`]): in order, as the joins are.
pub(crate) fn linked(next: &[Option<usize>]) -> Vec<(usize, usize)> {
    next.iter()
        .enumerate()
        .filter_map(|(j, after)| Some((j, (*after)?)))
        .collect()
}

/// What a recomputation keeps from when the offset was made.
pub struct Structure {
    /// The distinct knots of each spline result (None for other curves).
    pub knots: Vec<Option<Vec<f64>>>,
    pub joins: Vec<Join>,
    /// The indices of the source curves that vanish.
    pub dropped: Vec<usize>,
    /// The position of the result after each ([`SketchOffset::links`]).
    pub next: Vec<Option<usize>>,
}

/// How near zero, relative to the chain's size, an arc's offset radius or
/// the length of a curve's offset counts as nothing: the curve vanishes
/// (mitcad#1234).
const VANISH: f64 = 1e-7;

/// Why the offset of a chain cannot leave out what vanishes (mitcad#1234),
/// by the indices of the source curves.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Vanish {
    /// Every curve vanishes.
    All,
    /// Every curve of a closed chain but one vanishes.
    Alone(usize),
    /// The curves `between` vanish, and the offsets of those on either
    /// side do not meet (parallel lines).
    Apart {
        before: usize,
        after: usize,
        between: Vec<usize>,
    },
    /// The offsets of two neighbours do not meet at their corner.
    NoCorner(usize, usize),
    /// A curve's offset folds over and what is left of it would not be
    /// clear of the others: the distance is larger than its radius of
    /// curvature on that side.
    Folds(usize),
    /// The outline still crosses itself after it was split where its
    /// offsets cross (mitcad#1246).
    Tangled,
}

impl Vanish {
    fn message(&self, name: &dyn Fn(usize) -> String) -> String {
        match self {
            Self::All => "every curve of the chain vanishes at this distance".to_owned(),
            Self::Alone(i) => format!(
                "every curve of the chain but {} vanishes at this distance",
                name(*i)
            ),
            Self::Apart {
                before,
                after,
                between,
            } => format!(
                "{} {} at this distance, and the offsets of {} and {} on either side do not \
                 meet",
                listed(&between.iter().map(|i| name(*i)).collect::<Vec<_>>()),
                if between.len() == 1 {
                    "vanishes"
                } else {
                    "vanish"
                },
                name(*before),
                name(*after)
            ),
            Self::NoCorner(a, b) => format!(
                "the offsets of {} and {} do not meet at their corner",
                name(*a),
                name(*b)
            ),
            Self::Folds(i) => format!(
                "the offset is larger than {}'s radius of curvature on that side",
                name(*i)
            ),
            Self::Tangled => {
                "the offset's outline crosses itself where it cannot be trimmed at this distance"
                    .to_owned()
            }
        }
    }
}

/// `c2`, `c2 and c4`, `c2, c4 and c6`.
pub(crate) fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A chain's offset with what vanishes left out: the kept curves' indices
/// in chain order, the position (in `kept`) of the curve after each along
/// its outline (None at an open chain's end), each curve's range of chain
/// parameters, the join after each kept curve that has one after it, the
/// curves whose offset runs on along another's, with that other
/// (mitcad#1244), the arcs whose offset runs round its whole circle and the
/// curves left out as a part too small for the distance (mitcad#1246).
struct Trimmed {
    kept: Vec<usize>,
    next: Vec<Option<usize>>,
    range: Vec<(f64, f64)>,
    joins: Vec<Join>,
    merged: Vec<(usize, usize)>,
    whole: Vec<usize>,
    left: Vec<usize>,
}

/// Two kept curves that follow each other along an outline, and the
/// curves that vanished between them.
#[derive(Debug, Clone)]
struct Link {
    from: usize,
    to: usize,
    passed: Vec<usize>,
}

impl Link {
    /// Whether the two curves are neighbours in a chain of `n` (they share
    /// their corner).
    fn adjacent(&self, n: usize) -> bool {
        self.passed.is_empty() && (self.from + 1) % n == self.to
    }
}

/// The first kept curve after `i` along `after` (the chain's order, or its
/// links across a split), and the curves passed on the way; None at an
/// open chain's end.
fn following(after: &[Option<usize>], kept: &[bool], i: usize) -> Option<(usize, Vec<usize>)> {
    let mut passed = Vec::new();
    let mut at = after[i]?;
    while !kept[at] {
        passed.push(at);
        if passed.len() > after.len() {
            return None;
        }
        at = after[at]?;
    }
    Some((at, passed))
}

/// Whether two pieces are lines whose offsets lie on one line and run the
/// same way, `b` on from `a` (mitcad#1244: the sides of a notch the offset
/// closes).
fn collinear(a: &Piece, b: &Piece, size: f64) -> bool {
    if !(a.is_line() && b.is_line()) {
        return false;
    }
    let (a0, a1) = (a.eval(0.0).0, a.eval(1.0).0);
    let (b0, b1) = (b.eval(0.0).0, b.eval(1.0).0);
    let (ua, ub) = (unit(sub(a1, a0)), unit(sub(b1, b0)));
    let off = |p: P2| cross(ua, sub(p, a0)).abs();
    dot(ua, ub) > 0.0
        && off(b0) <= 1e-7 * size
        && off(b1) <= 1e-7 * size
        && dot(sub(b0, a0), ua) > 0.0
}

/// Whether two pieces are arcs whose offsets lie on one circle and run the
/// same way round it (mitcad#1246: the arcs on either side of a notch the
/// offset closes).
fn concentric(a: &Piece, b: &Piece, size: f64) -> bool {
    match (&a.curve, &b.curve, a.offset_radius(), b.offset_radius()) {
        (Curve2::Arc { center: ca, .. }, Curve2::Arc { center: cb, .. }, Some(ra), Some(rb)) => {
            a.forward == b.forward
                && dist(*ca, *cb) <= 1e-7 * size
                && (ra - rb).abs() <= 1e-7 * size
        }
        _ => false,
    }
}

/// Whether the offset of `b` runs on along that of `a`: lines on one line
/// ([`collinear`]), arcs on one circle ([`concentric`]).
fn runs_on(a: &Piece, b: &Piece, size: f64) -> bool {
    collinear(a, b, size) || concentric(a, b, size)
}

/// The chain parameter on `a` of the point of `b`'s offset at chain
/// parameter `v`, where `b`'s offset runs on along `a`'s ([`runs_on`]): along
/// a line, or round an arc's circle past `a`'s end.
fn along(a: &Piece, b: &Piece, v: f64) -> f64 {
    if !(a.is_arc() && b.is_arc()) {
        let (a0, a1) = (a.eval(0.0).0, a.eval(1.0).0);
        let line = sub(a1, a0);
        return dot(sub(b.eval(v).0, a0), line) / dot(line, line);
    }
    let (lo, hi) = a.curve.domain();
    let turn = 2.0 * std::f64::consts::PI;
    let t = b.t(v);
    if a.forward {
        (hi + (t - hi).rem_euclid(turn) - lo) / (hi - lo)
    } else {
        (hi - (lo - (lo - t).rem_euclid(turn))) / (hi - lo)
    }
}

/// The kept curve a merged one runs on along (through those merged before
/// it), when it is `kept`.
fn merged_into(merged: &[(usize, usize)], curve: usize, kept: &[bool]) -> Option<usize> {
    let mut at = curve;
    for _ in 0..=merged.len() {
        at = merged.iter().find(|(c, _)| *c == at)?.1;
        if kept[at] {
            return Some(at);
        }
    }
    None
}

/// Whether the whole circle of an arc's offset lies no nearer any curve of
/// the chain than the distance (mitcad#1246: an arc left alone runs round
/// it).
fn round_clear(pieces: &[Piece], i: usize, d: f64, size: f64) -> bool {
    let Curve2::Arc { center, .. } = pieces[i].curve else {
        return false;
    };
    let r = pieces[i].offset_radius().unwrap_or(0.0);
    let turn = 2.0 * std::f64::consts::PI;
    (0..64).all(|k| {
        let a = turn * k as f64 / 64.0;
        let at = add(center, [r * a.cos(), r * a.sin()]);
        clear(pieces, at, d, size)
    })
}

/// Whether a point lies no nearer any curve of the chain than the distance:
/// a point of the offset.
fn clear(pieces: &[Piece], at: P2, d: f64, size: f64) -> bool {
    pieces
        .iter()
        .all(|p| p.curve.closest(at).1 >= d.abs() - 1e-6 * size)
}

/// How the offsets of two neighbouring pieces meet.
fn join_kind(a: &Piece, b: &Piece, d: f64, size: f64) -> Join {
    let (end_a, _) = a.eval(1.0);
    let (start_b, _) = b.eval(0.0);
    let (_, ta) = a.source(1.0);
    let (_, tb) = b.source(0.0);
    if dist(end_a, start_b) <= 1e-7 * size {
        Join::Shared
    } else if cross(ta, tb) * d < 0.0 {
        // Turning toward the offset side (right for a positive d).
        Join::Trim
    } else {
        Join::Round
    }
}

/// Leaves out what of a chain's offset vanishes at the distance
/// (mitcad#1234). An arc whose offset radius is not above zero collapses,
/// a curve of another kind whose offset folds over (the distance passes its
/// radius of curvature) goes too; then, as long as any is left, a curve
/// whose trimmed range is empty (its neighbours' offsets cross beyond it,
/// so that its offset would run backwards: a loop on the source's side) is
/// left out, and its neighbours meet where their offsets cross, cut or run
/// on. Loops away from the joins go as well: a curve whose offset lies
/// wholly nearer the source than the distance ([`buried`], the overlap on
/// the source's side: the walls of a neck narrower than twice the distance,
/// whose offsets pass each other) is left out where two neighbours do not
/// meet, and when everything else meets. The point where two curves meet
/// across what vanished must lie at least the distance from it, or what is
/// left out would still reach the offset (a spline only partly folded
/// over): the distance is too large there. Curves that end an open chain
/// keep their free ends. With `sharp` (an offset made by constraints), the
/// offsets of neighbours that leave a gap run on to where they cross as
/// well, instead of a round corner.
///
/// Two lines whose offsets lie on one line across what vanished are one
/// (mitcad#1244, [`collinear`]): the later runs on along the first's. Where
/// the neighbours across what vanished do not meet in two places (a waist
/// narrower than twice the distance), and each meets the other's neighbour
/// instead ([`split`]), the outline falls into two there.
///
/// Open chains and loops that cross (mitcad#1246): an open chain falls into
/// parts the same way, and where the neighbours across what vanished do not
/// meet and nothing else applies, it ends on both sides when those ends are
/// points of the offset ([`open_end`]). Two arcs whose offsets lie on one
/// circle across what vanished are one ([`concentric`]); an arc left alone
/// in its outline runs round its whole circle when that lies clear of the
/// chain ([`round_clear`]). Where the offsets of two curves that do not
/// follow each other cross at a point of the offset ([`crossed`]: the walls
/// of a tapered waist, whose offsets cross on the source's side), the
/// outline is split there as across a waist, and the loop on the source's
/// side is trimmed in the rounds after. An outline left with one curve that
/// cannot close, or a part between two neighbours that do not meet which
/// cannot close on its own while the curves on either side of it meet
/// ([`lobe`]), is left out when anything else is left: a lobe narrower than
/// twice the distance.
fn trim(pieces: &[Piece], d: f64, closed: bool, size: f64, sharp: bool) -> Result<Trimmed, Vanish> {
    let n = pieces.len();
    let nothing = VANISH * size;
    let folds: Vec<bool> = pieces
        .iter()
        .map(|p| p.offset_radius().is_none() && !p.is_line() && p.check_fit().is_err())
        .collect();
    let mut kept: Vec<bool> = pieces
        .iter()
        .zip(&folds)
        .map(|(p, fold)| match p.offset_radius() {
            Some(r) => r > nothing,
            None => !fold,
        })
        .collect();
    // The curve after each along its outline: the chain's next, until a
    // split links curves across (mitcad#1244) or an open chain ends on
    // either side of what vanished (mitcad#1246).
    let mut after: Vec<Option<usize>> = (0..n)
        .map(|i| {
            if i + 1 < n {
                Some(i + 1)
            } else if closed {
                Some(0)
            } else {
                None
            }
        })
        .collect();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    // The curves left out as parts too small for the distance.
    let mut left: Vec<usize> = Vec::new();
    // The fewest curves an offset is left with.
    let fewest = if closed && n > 1 { 2 } else { 1 };
    // Whether the offset keeps an outline without the curves `gone`: as
    // many curves as it needs, or one arc whose offset may run round its
    // whole circle.
    let enough = |kept: &[bool], gone: &[usize]| {
        let rest: Vec<usize> = (0..n).filter(|i| kept[*i] && !gone.contains(i)).collect();
        rest.len() >= fewest || (rest.len() == 1 && pieces[rest[0]].is_arc())
    };
    // Each round leaves out, joins or links something anew; splits where
    // offsets cross might undo each other, so the rounds are bounded.
    for _ in 0..8 * n + 16 {
        let idx: Vec<usize> = (0..n).filter(|&i| kept[i]).collect();
        let m = idx.len();
        if m == 0 {
            // A curve that folds over says so.
            let fold = (0..n).find(|&g| folds[g]);
            return Err(fold.map_or(Vanish::All, Vanish::Folds));
        }
        if m == 1 && closed && n > 1 && !pieces[idx[0]].is_arc() {
            return Err(Vanish::Alone(idx[0]));
        }
        let mut links = Vec::with_capacity(m);
        // Arcs alone in their outline whose offset runs round the whole
        // circle, and other curves alone in theirs.
        let mut whole = Vec::new();
        let mut alone = Vec::new();
        for &i in &idx {
            let Some((to, passed)) = following(&after, &kept, i) else {
                continue;
            };
            if to != i {
                links.push(Link {
                    from: i,
                    to,
                    passed,
                });
            } else if pieces[i].is_arc() && round_clear(pieces, i, d, size) {
                whole.push(i);
            } else {
                alone.push(i);
            }
        }
        // An outline of one curve that cannot close: a part too small for
        // the distance, left out when anything else is left.
        if let Some(&i) = alone.first() {
            if alone.len() == m {
                return Err(Vanish::Alone(i));
            }
            for g in alone {
                kept[g] = false;
                left.push(g);
            }
            continue;
        }
        // Lines on one line, or arcs on one circle, across what vanished:
        // the later runs on along the first's (one at a time, as the links
        // change).
        let joined = links
            .iter()
            .find(|l| !l.adjacent(n) && runs_on(&pieces[l.from], &pieces[l.to], size));
        if let Some(link) = joined {
            kept[link.to] = false;
            merged.push((link.to, link.from));
            continue;
        }
        let Meeting {
            mut range,
            joins,
            failures,
        } = meeting(pieces, &links, &merged, &kept, d, size, sharp);
        if let Some(failure) = failures.first() {
            // Two neighbours that do not meet: the curves whose whole
            // offset is in the overlap go, and those on either side of
            // them meet instead; else (nothing would be left, or nothing
            // lies in the overlap) the outline may fall into two where
            // neighbours on either side of a waist do not meet, a part
            // between two such that cannot close on its own is left out,
            // or an open chain ends on either side; else the offset fails.
            let gone: Vec<usize> = idx
                .iter()
                .copied()
                .filter(|&i| buried(pieces, i, (0.0, 1.0), d, size))
                .collect();
            if !gone.is_empty() && enough(&kept, &gone) {
                for i in gone {
                    kept[i] = false;
                }
                continue;
            }
            if let Some([i, l, k, j]) = split(pieces, &failures, d, size) {
                after[i] = Some(l);
                after[k] = Some(j);
                continue;
            }
            if let Some(run) = lobe(pieces, &failures, &after, &kept, d, size)
                && enough(&kept, &run)
            {
                for g in run {
                    kept[g] = false;
                    left.push(g);
                }
                continue;
            }
            if !closed && let Some(i) = open_end(pieces, &failures, &range, d, size) {
                after[i] = None;
                continue;
            }
            return Err(failure.clone());
        }
        let inverted: Vec<usize> = idx
            .iter()
            .copied()
            .filter(|&i| {
                let (u0, u1) = range[i];
                u1 <= u0 || pieces[i].length(u0, u1) <= nothing
            })
            .collect();
        if !inverted.is_empty() {
            for i in inverted {
                kept[i] = false;
            }
            continue;
        }
        // A loop away from the joins: its curves lie in the overlap.
        let gone: Vec<usize> = idx
            .iter()
            .copied()
            .filter(|&i| buried(pieces, i, range[i], d, size))
            .collect();
        if !gone.is_empty() && enough(&kept, &gone) {
            for i in gone {
                kept[i] = false;
            }
            continue;
        }
        // Two offsets that cross at a point of the offset: the outline is
        // split there, and the loop on the source's side is trimmed in the
        // rounds after.
        if let Some([p, q, k, j]) = crossed(pieces, &links, &range, d, size) {
            after[p] = Some(q);
            after[k] = Some(j);
            continue;
        }
        for link in &links {
            let corner = pieces[link.from].eval(range[link.from].1).0;
            for &g in &link.passed {
                let reach = pieces[g].curve.closest(corner).1;
                if reach < d.abs() - 1e-6 * size {
                    return Err(Vanish::Folds(g));
                }
            }
        }
        // A curve that folds over at an open chain's end has nothing to
        // trim it.
        let end = |g: usize| !closed && (g < idx[0] || g > idx[m - 1]);
        if let Some(g) = (0..n).find(|&g| folds[g] && end(g)) {
            return Err(Vanish::Folds(g));
        }
        let position = |piece: usize| idx.iter().position(|&i| i == piece);
        let mut next = vec![None; m];
        for link in &links {
            if let Some(k) = position(link.from) {
                next[k] = position(link.to);
            }
        }
        // Each merged curve with the kept one it runs on along (through
        // those merged before it).
        let merged: Vec<(usize, usize)> = merged
            .iter()
            .filter_map(|&(curve, _)| Some((curve, merged_into(&merged, curve, &kept)?)))
            .collect();
        // The last curve of an open outline runs on to the end of those
        // that run on along it.
        for &(curve, into) in &merged {
            if links.iter().all(|l| l.from != into) && !whole.contains(&into) {
                let reach = along(&pieces[into], &pieces[curve], 1.0);
                range[into].1 = range[into].1.max(reach);
            }
        }
        return Ok(Trimmed {
            kept: idx,
            next,
            range,
            joins,
            merged,
            whole,
            left,
        });
    }
    Err(Vanish::Tangled)
}

/// What [`meeting`] finds: each piece's range of chain parameters, the join
/// after each linked piece, and why the offsets of linked pieces do not
/// meet, where they do not.
struct Meeting {
    range: Vec<(f64, f64)>,
    joins: Vec<Join>,
    failures: Vec<Vanish>,
}

/// The ranges of the linked pieces where their offsets meet their
/// neighbours', and the joins between them, in the links' order. Where an
/// arc runs on along the piece before a link (`merged`, mitcad#1246), the
/// crossing is that of the arc's offset near its end, on the same circle.
fn meeting(
    pieces: &[Piece],
    links: &[Link],
    merged: &[(usize, usize)],
    kept: &[bool],
    d: f64,
    size: f64,
    sharp: bool,
) -> Meeting {
    let n = pieces.len();
    let mut range = vec![(0.0, 1.0); n];
    let mut joins = Vec::with_capacity(links.len());
    let mut failures = Vec::new();
    for link in links {
        let (i, j) = (link.from, link.to);
        let through = link
            .passed
            .iter()
            .rev()
            .copied()
            .find(|&g| pieces[g].is_arc() && merged_into(merged, g, kept) == Some(i));
        let (a, b) = (through.map_or(&pieces[i], |g| &pieces[g]), &pieces[j]);
        let adjacent = link.adjacent(n);
        let kind = if adjacent {
            join_kind(a, b, d, size)
        } else {
            Join::Trim
        };
        if kind == Join::Trim || (sharp && kind == Join::Round) {
            match crossing(a, b).or_else(|| beyond(a, b)) {
                Some((u, v)) => {
                    range[i].1 = through.map_or(u, |g| along(&pieces[i], &pieces[g], u));
                    range[j].0 = v;
                }
                None if adjacent => failures.push(Vanish::NoCorner(i, j)),
                None => failures.push(Vanish::Apart {
                    before: i,
                    after: j,
                    between: link.passed.clone(),
                }),
            }
        }
        joins.push(kind);
    }
    Meeting {
        range,
        joins,
        failures,
    }
}

/// The links whose offsets do not meet across what vanished, as the curves
/// before and after.
fn apart_links(failures: &[Vanish]) -> Vec<(usize, usize)> {
    failures
        .iter()
        .filter_map(|f| match f {
            Vanish::Apart { before, after, .. } => Some((*before, *after)),
            _ => None,
        })
        .collect()
}

/// Two links whose offsets do not meet, `i` to `j` and `k` to `l`, where
/// the offset of `i` meets that of `l` and the offset of `k` meets that of
/// `j` instead (mitcad#1244): across a waist narrower than twice the
/// distance, the offset falls into two outlines, `i` going on to `l` and
/// `k` to `j`. As `[i, l, k, j]`.
fn split(pieces: &[Piece], failures: &[Vanish], d: f64, size: f64) -> Option<[usize; 4]> {
    let apart = apart_links(failures);
    for (t, &(i, j)) in apart.iter().enumerate() {
        for &(k, l) in &apart[t + 1..] {
            if i == l || k == j {
                continue;
            }
            if meets(pieces, i, l, d, size) && meets(pieces, k, j, d, size) {
                return Some([i, l, k, j]);
            }
        }
    }
    None
}

/// A part of an outline between two links whose offsets do not meet, `i`
/// to `j` and `k` to `l`, that cannot close on its own (one curve, or the
/// offset of `k` does not meet that of `j`) while the offsets of `i` and
/// `l` on either side of it meet (mitcad#1246: what is left of a lobe
/// narrower than twice the distance beyond a waist that vanished): the kept
/// curves from `j` along the outline to `k`.
fn lobe(
    pieces: &[Piece],
    failures: &[Vanish],
    after: &[Option<usize>],
    kept: &[bool],
    d: f64,
    size: f64,
) -> Option<Vec<usize>> {
    let apart = apart_links(failures);
    for &(i, j) in &apart {
        for &(k, l) in &apart {
            if (i, j) == (k, l) || i == l || !meets(pieces, i, l, d, size) {
                continue;
            }
            if j != k && meets(pieces, k, j, d, size) {
                continue;
            }
            let mut run = vec![j];
            let mut at = j;
            while at != k && run.len() <= kept.len() {
                let Some((next, _)) = following(after, kept, at) else {
                    break;
                };
                if next == i || next == l || run.contains(&next) {
                    break;
                }
                run.push(next);
                at = next;
            }
            if at == k {
                return Some(run);
            }
        }
    }
    None
}

/// A link of an open chain whose offsets do not meet, where the outline may
/// end on both sides (mitcad#1246): the end of the offset before it and the
/// start of the one after it are points of the offset ([`clear`]). The
/// curve before it.
fn open_end(
    pieces: &[Piece],
    failures: &[Vanish],
    range: &[(f64, f64)],
    d: f64,
    size: f64,
) -> Option<usize> {
    apart_links(failures).into_iter().find_map(|(i, j)| {
        let ends = (pieces[i].eval(range[i].1).0, pieces[j].eval(range[j].0).0);
        (clear(pieces, ends.0, d, size) && clear(pieces, ends.1, d, size)).then_some(i)
    })
}

/// Two curves that do not follow each other whose offsets, on their
/// ranges, cross at a point of the offset ([`clear`]) away from their ends,
/// and run on past it in the overlap on the source's side ([`buried`])
/// (mitcad#1246: the walls of a tapered waist). As `[p, q, k, j]`: the
/// outline runs along the offset of `p` into the crossing and on along that
/// of `q` (where it turns toward the offset's side), `j` follows `p` and
/// `k` comes before `q`. The outline is split there, `p` going on to `q`
/// and `k` to `j`, as across a waist.
fn crossed(
    pieces: &[Piece],
    links: &[Link],
    range: &[(f64, f64)],
    d: f64,
    size: f64,
) -> Option<[usize; 4]> {
    let after = |i: usize| links.iter().find(|l| l.from == i).map(|l| l.to);
    let before = |i: usize| links.iter().find(|l| l.to == i).map(|l| l.from);
    // Whether `b` comes after `a` along the links.
    let reaches = |a: usize, b: usize| {
        let mut at = a;
        for _ in 0..links.len() {
            match after(at) {
                Some(x) if x == b => return true,
                Some(x) => at = x,
                None => return false,
            }
        }
        false
    };
    let mut curves: Vec<usize> = links.iter().flat_map(|l| [l.from, l.to]).collect();
    curves.sort_unstable();
    curves.dedup();
    let lines: Vec<Vec<(f64, P2)>> = curves
        .iter()
        .map(|&i| polyline(&pieces[i], range[i]))
        .collect();
    let margin = 1e-6 * size;
    for (s, &a) in curves.iter().enumerate() {
        for (t, &b) in curves.iter().enumerate().skip(s + 1) {
            if after(a) == Some(b) || after(b) == Some(a) {
                continue;
            }
            for (u, v) in crossings(&lines[s], &lines[t]) {
                let Some((u, v)) = newton_from(&pieces[a], &pieces[b], u, v) else {
                    continue;
                };
                let at = pieces[a].eval(u).0;
                let ends = [range[a].0, range[a].1].map(|w| pieces[a].eval(w).0);
                let others = [range[b].0, range[b].1].map(|w| pieces[b].eval(w).0);
                let near = ends.iter().chain(&others).any(|e| dist(*e, at) <= margin);
                let inside = range[a].0 < u && u < range[a].1 && range[b].0 < v && v < range[b].1;
                if near || !inside || !clear(pieces, at, d, size) {
                    continue;
                }
                let turn = cross(pieces[a].eval(u).1, pieces[b].eval(v).1);
                let ((p, up), (q, vq)) = if turn * d < 0.0 {
                    ((a, u), (b, v))
                } else {
                    ((b, v), (a, u))
                };
                // What runs on past the crossing lies in the overlap on the
                // source's side (an outline that merely crosses itself, as
                // a stroke's band may, is left as it is).
                let overlap = buried(pieces, p, (up, range[p].1), d, size)
                    && buried(pieces, q, (range[q].0, vq), d, size);
                if overlap
                    && let (Some(j), Some(k)) = (after(p), before(q))
                    && (reaches(p, q) || reaches(q, p))
                {
                    return Some([p, q, k, j]);
                }
            }
        }
    }
    None
}

/// The offset of a piece on chain parameters `range` as a polyline of
/// `(parameter, point)`.
fn polyline(piece: &Piece, range: (f64, f64)) -> Vec<(f64, P2)> {
    let (u0, u1) = range;
    let steps: u32 = if piece.is_line() { 1 } else { 24 };
    (0..=steps)
        .map(|k| {
            let u = u0 + (u1 - u0) * f64::from(k) / f64::from(steps);
            (u, piece.eval(u).0)
        })
        .collect()
}

/// The parameters where two polylines of `(parameter, point)` cross, by
/// their segments.
fn crossings(a: &[(f64, P2)], b: &[(f64, P2)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for sa in a.windows(2) {
        for sb in b.windows(2) {
            let (r, s) = (sub(sa[1].1, sa[0].1), sub(sb[1].1, sb[0].1));
            let den = cross(r, s);
            if den.abs() <= 1e-12 * norm(r) * norm(s) {
                continue;
            }
            let w = sub(sb[0].1, sa[0].1);
            let (x, y) = (cross(w, s) / den, cross(w, r) / den);
            if (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y) {
                let u = sa[0].0 + x * (sa[1].0 - sa[0].0);
                let v = sb[0].0 + y * (sb[1].0 - sb[0].0);
                out.push((u, v));
            }
        }
    }
    out
}

/// Whether the offsets of pieces `a` and `b` can follow each other across
/// what vanished: on one line or circle ([`runs_on`]), or crossing at a
/// point no nearer any curve of the chain than the distance (a point of the
/// offset).
fn meets(pieces: &[Piece], a: usize, b: usize, d: f64, size: f64) -> bool {
    let (pa, pb) = (&pieces[a], &pieces[b]);
    if runs_on(pa, pb, size) {
        return true;
    }
    let Some((u, _)) = crossing(pa, pb).or_else(|| beyond(pa, pb)) else {
        return false;
    };
    clear(pieces, pa.eval(u).0, d, size)
}

/// Whether the offset of piece `i` on chain parameters `range` lies wholly
/// nearer the chain than the distance (inside points, not its ends, where
/// it may meet a neighbour at the distance): it is in the overlap on the
/// source's side. A point of a valid offset lies the distance from its own
/// curve and no nearer any other.
fn buried(pieces: &[Piece], i: usize, range: (f64, f64), d: f64, size: f64) -> bool {
    let limit = d.abs() * (1.0 - 1e-3) - 1e-6 * size;
    let (u0, u1) = range;
    (1..6).all(|k| {
        let at = pieces[i].eval(u0 + (u1 - u0) * k as f64 / 6.0).0;
        pieces.iter().any(|q| q.curve.closest(at).1 < limit)
    })
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
    let (u, v) = newton(a, b)?;
    (u > 0.0 && u <= 1.0 + 1e-9 && (-1e-9..1.0).contains(&v)).then_some((u.min(1.0), v.max(0.0)))
}

/// [`crossing`] where the offsets may run on past their ends to meet
/// ([`Piece::extends`]; another kind within its own range), or cross before
/// them: a curve between two that vanished, or whose own offset vanishes
/// (mitcad#1234).
fn beyond(a: &Piece, b: &Piece) -> Option<(f64, f64)> {
    let (u, v) = newton(a, b)?;
    let fits = |p: &Piece, w: f64| p.extends() || (-1e-9..=1.0 + 1e-9).contains(&w);
    (fits(a, u) && fits(b, v)).then_some((u, v))
}

/// Where the offsets of two pieces cross, by Newton from the end of `a`
/// and the start of `b`.
fn newton(a: &Piece, b: &Piece) -> Option<(f64, f64)> {
    newton_from(a, b, 1.0, 0.0)
}

/// Where the offsets of two pieces cross, by Newton from chain parameters
/// `u` on `a` and `v` on `b`.
fn newton_from(a: &Piece, b: &Piece, u: f64, v: f64) -> Option<(f64, f64)> {
    let (mut u, mut v) = (u, v);
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
    (u.is_finite() && v.is_finite() && dist(qa, qb) <= 1e-9 * size).then_some((u, v))
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

/// The pieces of a chain offset by `d` to the right of its direction.
fn pieces_of(sources: &[(Curve2, bool)], d: f64) -> Vec<Piece> {
    sources
        .iter()
        .map(|(curve, forward)| Piece {
            curve: curve.clone(),
            forward: *forward,
            left: if *forward { -d } else { d },
        })
        .collect()
}

/// The size of a chain: its largest curve's diagonal, at least 1 mm.
fn chain_size(pieces: &[Piece]) -> f64 {
    pieces
        .iter()
        .map(|p| {
            let (lo, hi) = p.curve.bounds();
            dist(lo, hi)
        })
        .fold(1.0, f64::max)
}

/// What of a chain of lines and arcs offset by constraints vanishes
/// ([`exact_vanishing`]), by the indices of its curves.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Vanishing {
    /// The curves that have no result of their own.
    pub dropped: Vec<usize>,
    /// Of those, the curves whose offset runs on along another's, with
    /// that other (mitcad#1244).
    pub merged: Vec<(usize, usize)>,
    /// Of those, the curves left out as a part too small for the distance
    /// (mitcad#1246).
    pub left: Vec<usize>,
    /// Where the offsets of the curves on either side of what vanished
    /// meet, by the index of the curve before.
    pub across: Vec<(usize, P2)>,
    /// The position, among the curves that have a result, of the one after
    /// each along its outline (None at the end of an open chain or outline).
    pub next: Vec<Option<usize>>,
    /// Where the offset of a curve that ends an open outline ends when
    /// others run on along it, by its index.
    pub reach: Vec<(usize, P2)>,
    /// The arcs whose offset runs round its whole circle (mitcad#1246).
    pub whole: Vec<usize>,
}

impl Vanishing {
    /// Nothing vanishes from a chain of `n` curves.
    pub(crate) fn none(n: usize, closed: bool) -> Self {
        Self {
            dropped: Vec::new(),
            merged: Vec::new(),
            left: Vec::new(),
            across: Vec::new(),
            reach: Vec::new(),
            whole: Vec::new(),
            next: (0..n)
                .map(|k| {
                    if k + 1 < n {
                        Some(k + 1)
                    } else if closed && n > 1 {
                        Some(0)
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }
}

/// The curves of a chain of lines and arcs that vanish when it is offset
/// by constraints by `d` (mitcad#1234, see [`trim`]; its corners sharp),
/// where the offsets of the curves on either side of them meet, the lines
/// and arcs whose offsets run on along another's and how the curves left
/// follow each other (mitcad#1244, mitcad#1246: the outline may fall into
/// several, open ones too). Where nothing collapses (no arc's offset radius
/// is above zero) and the offset cannot be trimmed, nothing is left out:
/// its corners are made as they were before.
pub(crate) fn exact_vanishing(
    sources: &[(Curve2, bool)],
    d: f64,
    closed: bool,
    name: &dyn Fn(usize) -> String,
) -> Result<Vanishing, String> {
    let pieces = pieces_of(sources, d);
    let n = pieces.len();
    let size = chain_size(&pieces);
    let trimmed = match trim(&pieces, d, closed, size, true) {
        Ok(trimmed) => trimmed,
        Err(error) => {
            let collapses = pieces
                .iter()
                .any(|p| p.offset_radius().is_some_and(|r| r <= VANISH * size));
            if collapses {
                return Err(error.message(name));
            }
            return Ok(Vanishing::none(n, closed));
        }
    };
    let kept = &trimmed.kept;
    let dropped: Vec<usize> = (0..n).filter(|i| !kept.contains(i)).collect();
    let mut across = Vec::new();
    let mut reach = Vec::new();
    for (k, next) in trimmed.next.iter().enumerate() {
        let i = kept[k];
        let end = trimmed.range[i].1;
        let Some(after) = next else {
            if end > 1.0 && !trimmed.whole.contains(&i) {
                reach.push((i, pieces[i].eval(end).0));
            }
            continue;
        };
        if (i + 1) % n != kept[*after] {
            across.push((i, pieces[i].eval(end).0));
        }
    }
    Ok(Vanishing {
        dropped,
        merged: trimmed.merged,
        left: trimmed.left,
        across,
        next: trimmed.next,
        reach,
        whole: trimmed.whole,
    })
}

/// The results of each outline of an offset whose curves follow each other
/// as `next` says ([`Computed::next`]), when there are several (mitcad#1244),
/// with the last result of each open one (mitcad#1246: an open chain's
/// parts); empty for one outline that is open or closed as the chain is,
/// and for one curve alone. Each open outline is listed from its first
/// result, a closed one from its first in `results`; the outlines in the
/// order of their first in `results`.
pub(crate) fn outlines_of(
    next: &[Option<usize>],
    results: &[EntityUid],
    closed: bool,
) -> (Vec<Vec<EntityUid>>, Vec<EntityUid>) {
    let m = next.len();
    if m != results.len() || next.iter().flatten().any(|k| *k >= m) {
        return (Vec::new(), Vec::new());
    }
    let mut first = vec![true; m];
    for k in next.iter().flatten() {
        first[*k] = false;
    }
    // Open outlines from their first curve, then the closed ones.
    let starts: Vec<usize> = (0..m).filter(|k| first[*k]).chain(0..m).collect();
    let mut seen = vec![false; m];
    let mut parts: Vec<(Vec<usize>, bool)> = Vec::new();
    for start in starts {
        let mut part = Vec::new();
        let mut open = false;
        let mut k = start;
        while !seen[k] {
            seen[k] = true;
            part.push(k);
            match next[k] {
                Some(after) => k = after,
                None => open = true,
            }
        }
        if !part.is_empty() {
            parts.push((part, open));
        }
    }
    parts.sort_by_key(|(part, _)| part.iter().min().copied());
    let single = match parts.as_slice() {
        [(part, open)] => part.len() == 1 || *open != closed,
        _ => false,
    };
    if parts.len() < 2 && (parts.is_empty() || single) {
        return (Vec::new(), Vec::new());
    }
    let ends = parts
        .iter()
        .filter(|(_, open)| *open)
        .filter_map(|(part, _)| part.last().map(|k| results[*k]))
        .collect();
    let outlines = parts
        .into_iter()
        .map(|(part, _)| part.iter().map(|k| results[*k]).collect())
        .collect();
    (outlines, ends)
}

/// Computes the offset of a chain of solved curves: `d` to the right of
/// the chain's direction (negative to the left), with what vanishes at the
/// distance left out ([`trim`], mitcad#1234); `name` names a source curve
/// by its index for the errors. With a structure, the joins, the curves
/// that vanish and the spline knots are those of the structure, and a join
/// of another kind or another set of curves vanishing now is an error.
pub fn compute(
    sources: &[(Curve2, bool)],
    d: f64,
    closed: bool,
    structure: Option<&Structure>,
    name: &dyn Fn(usize) -> String,
) -> Result<Computed, String> {
    let pieces = pieces_of(sources, d);
    let n = pieces.len();
    let size = chain_size(&pieces);
    let single_closed = n == 1 && sources[0].0.is_closed();
    let closed = closed && !single_closed;
    let trimmed = if single_closed {
        for p in &pieces {
            p.check_fit()?;
        }
        Trimmed {
            kept: vec![0],
            next: vec![None],
            range: vec![(0.0, 1.0)],
            joins: Vec::new(),
            merged: Vec::new(),
            whole: Vec::new(),
            left: Vec::new(),
        }
    } else {
        trim(&pieces, d, closed, size, false).map_err(|e| e.message(name))?
    };
    let Trimmed {
        kept,
        next,
        range,
        joins,
        merged,
        whole,
        left,
    } = trimmed;
    // The joins follow the links of the curves that have one after them.
    let links = linked(&next);
    let dropped: Vec<usize> = (0..n).filter(|i| !kept.contains(i)).collect();
    if let Some(s) = structure {
        if s.dropped != dropped {
            let names = |list: &[usize]| {
                let all: Vec<String> = list.iter().map(|i| name(*i)).collect();
                if all.is_empty() {
                    "none".to_owned()
                } else {
                    listed(&all)
                }
            };
            return Err(format!(
                "the offset no longer fits: the curves that vanish at this distance changed \
                 ({} before, {} now); an edit of the sketch makes it again",
                names(&s.dropped),
                names(&dropped)
            ));
        }
        if s.next != next {
            return Err(
                "the offset no longer fits: the curves that follow each other changed (it falls \
                 into other outlines); an edit of the sketch makes it again"
                    .to_owned(),
            );
        }
        for (j, kind) in joins.iter().enumerate() {
            if s.joins.get(j) != Some(kind) {
                return Err(format!(
                    "the offset no longer fits: the corner at join {j} changed from {:?} to \
                     {kind:?} (round, trimmed or smooth); offset again",
                    s.joins.get(j)
                ));
            }
        }
    }
    let m = kept.len();
    // Round corners about the source's corner.
    let mut corners = Vec::new();
    for (&(j, after), join) in links.iter().zip(&joins) {
        if *join == Join::Round {
            let (a, b) = (&pieces[kept[j]], &pieces[kept[after]]);
            let (corner, _) = a.source(1.0);
            corners.push((j, round_corner(corner, a.eval(1.0).0, b.eval(0.0).0)));
        }
    }
    // Each kept piece in chain direction: start and end, then its curve.
    let mut ends: Vec<(P2, P2)> = Vec::with_capacity(m);
    for i in &kept {
        let (u0, u1) = range[*i];
        if u1 <= u0 {
            return Err(format!(
                "the offset is larger than {} of the chain",
                name(*i)
            ));
        }
        ends.push((pieces[*i].eval(u0).0, pieces[*i].eval(u1).0));
    }
    // Pieces that meet share one point exactly.
    for (&(j, after), join) in links.iter().zip(&joins) {
        if *join != Join::Round {
            let p = scale(add(ends[j].1, ends[after].0), 0.5);
            ends[j].1 = p;
            ends[after].0 = p;
        }
    }
    let mut made = Vec::with_capacity(m);
    for (k, i) in kept.iter().enumerate() {
        let p = &pieces[*i];
        let (u0, u1) = range[*i];
        let (from, to) = ends[k];
        made.push(match &p.curve {
            Curve2::Line { .. } => Made::Line(from, to),
            // Round its whole circle (mitcad#1246).
            Curve2::Arc { center, .. } if whole.contains(i) => Made::Circle {
                center: *center,
                radius: p.offset_radius().unwrap_or(0.0),
            },
            Curve2::Circle { center, radius, .. } if single_closed => {
                // Its radius less the distance toward its centre.
                let r = radius - p.left;
                if r <= 0.0 {
                    return Err("the offset is larger than the circle's radius".to_owned());
                }
                Made::Circle {
                    center: *center,
                    radius: r,
                }
            }
            Curve2::Arc { center, .. } => {
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
                let knots = match structure.and_then(|s| s.knots.get(k).cloned().flatten()) {
                    Some(knots) => knots,
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
        next,
        joins,
        corners,
        closed,
        dropped,
        merged,
        left,
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
            dropped: offset.dropped_indices(),
            next: offset.links(closed),
        };
        let computed = compute(&chain, d, closed, Some(&structure), &offset.namer())
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
            // The curves that have a result (mitcad#1234: the results on
            // either side of one that vanished share their corner, which is
            // placed below where their offset curves cross).
            let chain: Vec<(Curve2, bool)> = chain
                .into_iter()
                .zip(&offset.curves)
                .filter(|(_, c)| !offset.dropped.contains(c))
                .map(|(piece, _)| piece)
                .collect();
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
            let kept = offset.kept();
            for (((curve, forward), result), source) in chain.iter().zip(&offset.results).zip(&kept)
            {
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
                        // Over the lines that run on along it too
                        // (mitcad#1244): an open chain's end is free.
                        let (a, b) = offset
                            .merged
                            .iter()
                            .filter(|m| m.into == *source)
                            .filter_map(|m| match curves.get(&m.curve) {
                                Some(Curve2::Line { a, b }) => Some([*a, *b]),
                                _ => None,
                            })
                            .flatten()
                            .map(|q| add(q, scale(n, left)))
                            .fold((a, b), |(a, b), q| {
                                let t = dot(sub(q, a), along);
                                if t < 0.0 {
                                    (q, b)
                                } else if t > dot(sub(b, a), along) {
                                    (a, q)
                                } else {
                                    (a, b)
                                }
                            });
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
                        // the source's is, even where the source turned, or
                        // where the arcs that run on along it end,
                        // mitcad#1246); a corner is placed below, where the
                        // curves cross.
                        let run_on = offset
                            .merged
                            .iter()
                            .rev()
                            .filter(|m| m.into == *source)
                            .find_map(|m| match curves.get(&m.curve) {
                                Some(Curve2::Arc { start, end, .. }) => Some((*start, *end)),
                                _ => None,
                            });
                        let (s0, e0) = match run_on {
                            Some((ms, _)) if !*forward => (ms, *e),
                            Some((_, me)) => (*s, me),
                            None => (*s, *e),
                        };
                        let on = |t: f64| add(*center, [r * t.cos(), r * t.sin()]);
                        points.extend([(*c, *center), (*start, on(s0)), (*end, on(e0))]);
                        circles.extend([(*start, *center, r), (*end, *center, r)]);
                        rounds.push((*center, *radius, r, Some((*s, *e))));
                    }
                    // An arc whose offset runs round its whole circle
                    // (mitcad#1246): its helper across the arc's middle.
                    (
                        Curve2::Arc {
                            center,
                            radius,
                            start: s,
                            end: e,
                        },
                        Some(EntityKind::Circle { center: c, .. }),
                    ) => {
                        let r = radius - left;
                        if !(r.is_finite() && r > 0.0) {
                            fits = false;
                            continue;
                        }
                        points.push((*c, *center));
                        radii.push((*result, r));
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
        if n < 2 || !offset.whole() {
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
        // The curves that have a result, with their directions: across one
        // that vanished (mitcad#1234) the sources share no point, and the
        // corner is not held.
        let kept: Vec<(EntityUid, bool, EntityUid)> = offset
            .curves
            .iter()
            .zip(&forward)
            .filter(|(c, _)| !offset.dropped.contains(c))
            .zip(&offset.results)
            .map(|((c, f), r)| (*c, *f, *r))
            .collect();
        let m = kept.len();
        let corners = if closed { m } else { m.saturating_sub(1) };
        for i in 0..corners {
            let ((s1, f1, r1), (s2, f2, r2)) = (kept[i], kept[(i + 1) % m]);
            // The point the sources share, and the one the results share.
            let Some(at) = end(s1, f1, true).filter(|p| end(s2, f2, false) == Some(*p)) else {
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
        // The curves that have a result (mitcad#1234).
        let sources = offset.kept();
        let n = sources.len();
        if n == 0 || !offset.whole() {
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
        let helpers: Vec<Option<Helper>> = sources
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
        for (&source, &result) in sources.iter().zip(results) {
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
        for (h, &source) in helpers.iter().zip(&sources) {
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
        .pairs()
        .into_iter()
        .flat_map(|(s, r)| [(s, r), (r, s)])
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

/// The source curves of an offset that vanish at the signed distance `d`
/// with the sources' geometry `curves` (mitcad#1234), or None when that
/// cannot be told (a result the user deleted, a chain that does not offset
/// now: the solve says why).
fn vanishing_at(
    offset: &SketchOffset<ParamId>,
    entities: &[Entity],
    curves: &std::collections::BTreeMap<EntityUid, Curve2>,
    d: f64,
) -> Option<Vec<EntityUid>> {
    if !offset.whole() {
        return None;
    }
    let index = EntityIndex::new(entities);
    // A chain joined at coincident ends rather than shared points (an
    // import's offset of an offset) runs as its ends meet, as `seed` takes
    // it.
    let joined = offset
        .curves
        .windows(2)
        .all(|w| shared_end(&index, w[0], w[1]).is_some());
    let (chain, closed) = if joined {
        sources(offset, &index, curves).ok()?
    } else {
        let listed: Option<Vec<(Curve2, bool)>> = offset
            .curves
            .iter()
            .map(|id| curves.get(id).map(|c| (c.clone(), true)))
            .collect();
        joined_by_ends(&listed?)
    };
    let dropped = if offset.derived {
        let computed = compute(&chain, d, closed, None, &offset.namer()).ok()?;
        computed.dropped
    } else if chain.len() > 1 {
        exact_vanishing(&chain, d, closed, &offset.namer())
            .ok()?
            .dropped
    } else {
        Vec::new()
    };
    Some(dropped.iter().map(|i| offset.curves[*i]).collect())
}

/// Whether an offset of `def` no longer has the curves that vanish at its
/// distance, with the geometry `solved` (mitcad#1234): a parameter set
/// outside the sketch, or a linked projection that followed its source.
/// `value` gives a parameter's value.
pub fn stale_in(
    def: &crate::def::SketchDef,
    solved: &Solved,
    value: &mut dyn FnMut(ParamId) -> Option<f64>,
) -> bool {
    def.offsets.iter().any(|o| {
        let Some(d) = value(o.distance) else {
            return false;
        };
        let d = if o.left { -d } else { d };
        vanishing_at(o, &def.entities, &solved.curves, d).is_some_and(|now| now != o.dropped)
    })
}

/// A sketch whose offsets an evaluation made again (mitcad#1234): the
/// definition with them, its solution, and what an edit would have said.
#[derive(Debug)]
pub struct Retrimmed {
    pub def: crate::def::SketchDef,
    pub solved: Solved,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
}

/// Makes again, as an edit of the sketch does
/// ([`SketchEdit::settle_offsets`]), the offsets of `def` whose curves that
/// vanish at their distances changed outside the sketch: a parameter set
/// elsewhere, a linked projection that followed its source (mitcad#1234).
/// The results of source curves kept before and after keep their ids, so
/// the profile regions and the features that name them keep working. The
/// evaluation shows this definition; the stored one keeps the offsets as the
/// last edit made them until the sketch is edited, which starts from this
/// one. `solved` is the evaluation's solve of `def` when it solved, else the
/// sources are solved without the offsets' curves; `params` are the values
/// it used (copied: nothing is made in them). None when no offset is to be
/// made again, or it cannot be (the evaluation's own error stands).
pub fn retrim_for_evaluation(
    def: &crate::def::SketchDef,
    params: &crate::parameters::Parameters,
    owner: crate::ids::FeatureUid,
    solved: Option<&Solved>,
) -> Option<Retrimmed> {
    if def.offsets.is_empty() {
        return None;
    }
    let mut params = params.clone();
    let mut edit = SketchEdit::new(def.clone(), &mut params, owner, "");
    let sources = match solved {
        Some(solved) => solved.clone(),
        None => match edit.sources_solved(false) {
            Ok(solved) => solved,
            Err(_) => edit.sources_solved(true).ok()?,
        },
    };
    if !edit.retrim_offsets(&sources).ok()? {
        return None;
    }
    edit.refit_offsets(&sources).ok()?;
    let solved = edit.solved().ok()?;
    let mut def = edit.def;
    solved.store(&mut def.entities);
    Some(Retrimmed {
        def,
        solved,
        warnings: edit.report.warnings,
        notes: edit.report.notes,
    })
}

/// Whether every entity `refs` names is in the sketch.
fn all_exist(def: &crate::def::SketchDef, refs: &[Ref]) -> bool {
    refs.iter().all(|r| def.entity(r.uid()).is_some())
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

/// The joins as the offset was made, between the curves that have a result
/// (mitcad#1234): round where it has a corner arc, else shared when the
/// sources join smoothly, else trimmed (always across a curve that
/// vanished).
fn stored_joins(
    offset: &SketchOffset<ParamId>,
    chain: &[(Curve2, bool)],
    closed: bool,
) -> Vec<Join> {
    let n = chain.len();
    let dropped = offset.dropped_indices();
    let kept: Vec<usize> = (0..n).filter(|i| !dropped.contains(i)).collect();
    // Along the results' links (mitcad#1244: across a split).
    offset
        .links(closed)
        .into_iter()
        .enumerate()
        .filter_map(|(j, after)| Some((j, after?)))
        .filter(|&(j, after)| j < kept.len() && after < kept.len())
        .map(|(j, after)| {
            if offset.corners.iter().any(|c| c.join as usize == j) {
                return Join::Round;
            }
            let (i, k) = (kept[j], kept[after]);
            if (i + 1) % n != k {
                return Join::Trim;
            }
            let (a, b) = (chain[i].0.clone(), chain[k].0.clone());
            // A smooth join of the sources gives a smooth join of the
            // offsets.
            let ta = {
                let (lo, hi) = a.domain();
                let t = if chain[i].1 { hi } else { lo };
                let d = a.eval(t)[1];
                unit(if chain[i].1 { d } else { scale(d, -1.0) })
            };
            let tb = {
                let (lo, hi) = b.domain();
                let t = if chain[k].1 { lo } else { hi };
                let d = b.eval(t)[1];
                unit(if chain[k].1 { d } else { scale(d, -1.0) })
            };
            if cross(ta, tb).abs() <= 1e-7 && dot(ta, tb) > 0.0 {
                Join::Shared
            } else {
                Join::Trim
            }
        })
        .collect()
}

/// An offset taken out of the sketch to be made again
/// ([`SketchEdit::remake_offset`]): its record, each source curve with
/// whether it ran from its start to its end along the chain, the points of
/// each curve it made in their order, and the user's constraints and
/// dimensions on those.
struct TornDown {
    offset: SketchOffset<ParamId>,
    forward: Vec<(EntityUid, bool)>,
    points: Vec<(EntityUid, Vec<EntityUid>)>,
    constraints: Vec<super::Constraint>,
    dimensions: Vec<super::Dimension<ParamId>>,
}

/// An offset made again: its record before and after.
type Remade = (SketchOffset<ParamId>, SketchOffset<ParamId>);

/// The curves an offset made: its results, corner arcs and helpers.
fn made_and_helpers(offset: &SketchOffset<ParamId>) -> Vec<EntityUid> {
    let mut made = offset.made();
    made.extend(offset.helpers.iter().copied());
    made
}

/// The curves an offset of offsets' results takes after those were made
/// again (mitcad#1244, `remade` in order), and the results it took that
/// vanished. It keeps its other curves and the results kept; the result of
/// a curve whose offset now runs on along another's becomes that one's;
/// an offset of all of an offset's results takes all of its new ones,
/// those that came back too.
fn carried(curves: &[EntityUid], remade: &[Remade]) -> (Vec<EntityUid>, Vec<EntityUid>) {
    let mut out: Vec<EntityUid> = curves.to_vec();
    let mut lost = Vec::new();
    for (old, new) in remade {
        let gone: BTreeSet<EntityUid> = made_and_helpers(old).into_iter().collect();
        if !out.iter().any(|c| gone.contains(c)) {
            continue;
        }
        let whole = old.made().iter().all(|c| out.contains(c));
        let before = old.pairs();
        let after: BTreeMap<EntityUid, EntityUid> = new.pairs().into_iter().collect();
        let into = |s: EntityUid| {
            new.merged
                .iter()
                .find(|m| m.curve == s)
                .map_or(s, |m| m.into)
        };
        let mut taken: Vec<EntityUid> = Vec::new();
        for c in &out {
            let took = if !gone.contains(c) {
                Some(*c)
            } else if whole {
                continue;
            } else {
                before
                    .iter()
                    .find(|(_, r)| r == c)
                    .and_then(|(s, _)| after.get(&into(*s)).copied())
            };
            match took {
                Some(r) if !taken.contains(&r) => taken.push(r),
                Some(_) => {}
                None => lost.push(*c),
            }
        }
        if whole {
            for r in new.made() {
                if !taken.contains(&r) {
                    taken.push(r);
                }
            }
        }
        out = taken;
    }
    (out, lost)
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
        let solved = self.offset_solve()?;
        let mut sources = Vec::new();
        for (id, forward) in chain {
            let c = solved
                .curves
                .get(id)
                .cloned()
                .ok_or("a curve without geometry")?;
            sources.push((c, *forward));
        }
        let name = |i: usize| format!("c{}", chain[i].0.0);
        let computed = compute(&sources, d, closed, None, &name)?;
        // The curves that do not vanish (mitcad#1234).
        let kept: Vec<(EntityUid, bool)> = chain
            .iter()
            .enumerate()
            .filter(|(i, _)| !computed.dropped.contains(i))
            .map(|(_, c)| *c)
            .collect();
        let dropped: Vec<EntityUid> = computed.dropped.iter().map(|i| chain[*i].0).collect();
        let n = computed.made.len();
        // Curve ids first (results, then corner arcs), so that names do not
        // depend on how many points the splines take.
        let ids = self.result_ids(&kept);
        let arc_ids = self.ids(computed.corners.len());
        // Points: shared at smooth and trimmed joins, own at round corners.
        let mut starts: Vec<Option<EntityUid>> = vec![None; n];
        let mut ends: Vec<Option<EntityUid>> = vec![None; n];
        for (&(j, next), join) in linked(&computed.next).iter().zip(&computed.joins) {
            if *join != Join::Round {
                let p = self.add_point(computed.ends[j].1);
                ends[j] = Some(p);
                starts[next] = Some(p);
            }
        }
        let mut corner_points: Vec<(usize, EntityUid, EntityUid)> = Vec::new();
        for (j, _) in &computed.corners {
            let Some(next) = computed.next[*j] else {
                continue;
            };
            let a = self.add_point(computed.ends[*j].1);
            let b = self.add_point(computed.ends[next].0);
            ends[*j] = Some(a);
            starts[next] = Some(b);
            corner_points.push((*j, a, b));
        }
        let single_closed = sources.len() == 1 && sources[0].0.is_closed();
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
                    let (s, e) = if kept[i].1 { (s, e) } else { (e, s) };
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
        let merged: Vec<OffsetMerge> = computed
            .merged
            .iter()
            .map(|&(curve, into)| OffsetMerge {
                curve: chain[curve].0,
                into: chain[into].0,
            })
            .collect();
        let (outlines, open) = outlines_of(&computed.next, &ids, closed);
        let small: Vec<EntityUid> = computed.left.iter().map(|i| chain[*i].0).collect();
        self.note_trimmed(&dropped, &merged, &small, outlines.len());
        self.def.offsets.push(SketchOffset {
            id,
            curves: chain.iter().map(|(c, _)| *c).collect(),
            distance: param,
            left: d < 0.0,
            results: ids.clone(),
            dropped,
            merged,
            outlines,
            ends: open,
            corners,
            helpers: Vec::new(),
            derived: true,
        });
        self.report.constraints.push(id);
        Ok((id, ids))
    }

    /// Notes what of an offset's chain is left out (mitcad#1234,
    /// mitcad#1244): the curves that vanish, those whose offset runs on
    /// along another's (`c13's offset runs on along c1's at this distance`),
    /// those left out as a part too small for the distance (mitcad#1246:
    /// `the offset leaves out c5, a part too small for this distance`) and
    /// the number of outlines it falls into when more than one.
    pub(crate) fn note_trimmed(
        &mut self,
        dropped: &[EntityUid],
        merged: &[OffsetMerge],
        left: &[EntityUid],
        outlines: usize,
    ) {
        let vanished: Vec<EntityUid> = dropped
            .iter()
            .copied()
            .filter(|c| !merged.iter().any(|m| m.curve == *c) && !left.contains(c))
            .collect();
        self.note_vanished(&vanished);
        for m in merged {
            self.report.notes.push(format!(
                "c{}'s offset runs on along c{}'s at this distance",
                m.curve.0, m.into.0
            ));
        }
        if !left.is_empty() {
            let names: Vec<String> = left.iter().map(|c| format!("c{}", c.0)).collect();
            self.report.notes.push(format!(
                "the offset leaves out {}, a part too small for this distance",
                listed(&names)
            ));
        }
        if outlines > 1 {
            self.report.notes.push(format!(
                "the offset falls into {outlines} outlines at this distance"
            ));
        }
    }

    /// Ids for the results of source curves in chain order: the one a
    /// curve had before an offset is made again (mitcad#1234), else a new
    /// one.
    pub(crate) fn result_ids(&mut self, sources: &[(EntityUid, bool)]) -> Vec<EntityUid> {
        sources
            .iter()
            .map(|(source, _)| {
                let reused = self.reuse_results.get(source).copied();
                reused.unwrap_or_else(|| self.id())
            })
            .collect()
    }

    /// Says which curves of a chain vanish at the offset's distance
    /// (mitcad#1234), as a note: "corner arcs c4 and c7 vanish at this
    /// distance".
    pub(crate) fn note_vanished(&mut self, dropped: &[EntityUid]) {
        if dropped.is_empty() {
            return;
        }
        let kind = |id: &EntityUid| self.def.entity(*id).map(|e| e.kind.type_name());
        let what = if dropped.iter().all(|c| kind(c) == Some("arc")) {
            "corner arc"
        } else if dropped.iter().all(|c| kind(c) == Some("line")) {
            "line"
        } else {
            "curve"
        };
        let names: Vec<String> = dropped.iter().map(|c| format!("c{}", c.0)).collect();
        let note = if names.len() == 1 {
            format!("{what} {} vanishes at this distance", names[0])
        } else {
            format!("{what}s {} vanish at this distance", listed(&names))
        };
        self.report.notes.push(note);
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
                offset.pairs().into_iter().any(|(a, b)| match d.kind {
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
    /// parameter, a name uses that parameter) or side. An offset made by
    /// constraints whose curves that vanish change at the new distance is
    /// made again at once, from where its sources are (mitcad#1234); the
    /// others when the edit finishes ([`SketchEdit::settle_offsets`]).
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
            // The corners change kind: made again, on the other side.
            return self.remake_offset(id, true, false).map(|_| ());
        }
        if distance.is_some() && !offset.derived {
            // Before the solve: an offset that no longer fits its distance
            // may not solve, or pull a free source out of place.
            let offset = self.def.offsets[i].clone();
            let stale = self
                .stored_sources(&offset)
                .and_then(|curves| self.vanishing(&offset, &curves))
                .is_some_and(|now| now != offset.dropped);
            if stale {
                self.remake_offset(id, false, true)?;
            }
        }
        Ok(())
    }

    /// The solve that finishes an edit, with every offset fitted to the
    /// geometry first (mitcad#1234): an offset whose curves that vanish at
    /// its distance are no longer those it has is made again, and derived
    /// offsets fit their spline pieces again ([`SketchEdit::refit_offsets`]).
    /// The sources' geometry comes from the solve itself where it can (one
    /// solve when nothing changes), else from a solve of the sketch without
    /// the offsets' curves: a derived offset that no longer fits refuses the
    /// solve, and so may one made by constraints.
    pub(crate) fn settle_offsets(&mut self) -> Result<Solved, String> {
        if self.def.offsets.is_empty() {
            return self.solved();
        }
        if !self.def.offsets.iter().any(|o| o.derived) {
            return match self.solved() {
                Ok(solved) => {
                    if self.retrim_offsets(&solved)? {
                        self.solved()
                    } else {
                        Ok(solved)
                    }
                }
                Err(error) => {
                    let Ok(sources) = self.sources_solved(true) else {
                        return Err(error);
                    };
                    if self.retrim_offsets(&sources)? {
                        self.solved()
                    } else {
                        Err(error)
                    }
                }
            };
        }
        let sources = match self.sources_solved(false) {
            Ok(solved) => solved,
            Err(error) => self.sources_solved(true).map_err(|_| error)?,
        };
        self.retrim_offsets(&sources)?;
        self.refit_offsets(&sources)?;
        self.solved()
    }

    /// The sketch solved without derived offsets' curves (and with `all`,
    /// without any offset's curves and helpers and what refers to them): the
    /// sources' geometry, which those curves follow.
    fn sources_solved(&mut self, all: bool) -> Result<Solved, String> {
        let mut def = self.def.clone();
        let mut gone = {
            let index = EntityIndex::new(&def.entities);
            let mut gone = derived_entities(&def.offsets, &index);
            if all {
                for offset in &def.offsets {
                    for curve in offset.made().iter().chain(&offset.helpers) {
                        gone.insert(*curve);
                        if let Some(e) = index.get(*curve) {
                            gone.extend(e.kind.points());
                        }
                    }
                }
            }
            gone
        };
        // Never the points the sources use (an arc result's centre).
        for offset in &def.offsets {
            for curve in &offset.curves {
                if let Some(e) = def.entity(*curve) {
                    for p in e.kind.points() {
                        gone.remove(&p);
                    }
                }
            }
        }
        def.offsets.retain(|o| !all && !o.derived);
        def.entities.retain(|e| !gone.contains(&e.id));
        let touches = |refs: Vec<Ref>| refs.iter().any(|r| gone.contains(&r.uid()));
        def.constraints.retain(|c| !touches(c.kind.refs()));
        def.dimensions.retain(|d| !touches(d.kind.refs()));
        // Solved as the edit's own definition is.
        let edited = std::mem::replace(&mut self.def, def);
        let solved = self.solved();
        self.def = edited;
        solved
    }

    /// Makes again each offset whose curves that vanish at its distance are
    /// no longer those it has, by the sources in `solved` (mitcad#1234).
    /// Whether any was.
    fn retrim_offsets(&mut self, solved: &Solved) -> Result<bool, String> {
        let stale = self.stale_offsets(solved);
        if stale.is_empty() {
            return Ok(false);
        }
        // Made from these sources: the sketch with another stale offset
        // in it may not solve.
        self.known_sources = Some(solved.clone());
        // An offset of an offset is made again with it (mitcad#1244), and
        // not again from the sources found before.
        let mut done: BTreeSet<ConstraintUid> = BTreeSet::new();
        let remade = stale.iter().try_for_each(|id| {
            if !done.contains(id) {
                done.extend(self.remake_offset(*id, false, true)?);
            }
            Ok::<(), String>(())
        });
        self.known_sources = None;
        remade.map(|()| true)
    }

    /// The solve an offset is made from: the sources' geometry while
    /// offsets are made again (mitcad#1234), else the sketch solved now.
    pub(crate) fn offset_solve(&self) -> Result<Solved, String> {
        match &self.known_sources {
            Some(solved) => Ok(solved.clone()),
            None => self.solved(),
        }
    }

    /// The offsets whose curves that vanish at their distance are no longer
    /// those they have, by the sources in `solved`.
    fn stale_offsets(&self, solved: &Solved) -> Vec<ConstraintUid> {
        self.def
            .offsets
            .iter()
            .filter(|o| {
                self.vanishing(o, &solved.curves)
                    .is_some_and(|now| now != o.dropped)
            })
            .map(|o| o.id)
            .collect()
    }

    /// Whether an offset is to be made again for the geometry `solved`
    /// (mitcad#1234).
    pub(crate) fn offsets_stale(&self, solved: &Solved) -> bool {
        !self.def.offsets.is_empty() && !self.stale_offsets(solved).is_empty()
    }

    /// The source curves of an offset that vanish at its distance with the
    /// sources' geometry `curves` ([`vanishing_at`]).
    fn vanishing(
        &self,
        offset: &SketchOffset<ParamId>,
        curves: &std::collections::BTreeMap<EntityUid, Curve2>,
    ) -> Option<Vec<EntityUid>> {
        let d = self.params.value(offset.distance)?;
        let d = if offset.left { -d } else { d };
        vanishing_at(offset, &self.def.entities, curves, d)
    }

    /// The source curves of an offset made by constraints where their
    /// points are stored (lines, arcs and circles), or None.
    fn stored_sources(
        &self,
        offset: &SketchOffset<ParamId>,
    ) -> Option<std::collections::BTreeMap<EntityUid, Curve2>> {
        let at = |p: EntityUid| match self.def.entity(p).map(|e| &e.kind) {
            Some(EntityKind::Point { at }) => Some(*at),
            _ => None,
        };
        let mut out = std::collections::BTreeMap::new();
        for id in &offset.curves {
            out.insert(*id, self.curve_placed(*id, &at)?);
        }
        Some(out)
    }

    /// Makes an offset again from its sources at its distance (mitcad#1234),
    /// on the other side with `flip`. The result of a source curve that has
    /// one before and after keeps its id, one that comes back gets a new
    /// id. With `keep`, the user's constraints and dimensions on the results
    /// and their points go with them (onto the points made again in the same
    /// place of their curves; those on the result of a curve whose offset
    /// now runs on along another's onto that one's result, mitcad#1244);
    /// those on a curve that vanished are removed, and a warning names them.
    ///
    /// The offsets of its results (an offset of an offset, and theirs) are
    /// made again after it from the results it has then (mitcad#1244): from
    /// those it kept, and, for an offset of all of its results, from all of
    /// them, those that came back too; one whose curves all vanished is
    /// removed with a warning. The offsets made again, this one first.
    fn remake_offset(
        &mut self,
        id: ConstraintUid,
        flip: bool,
        keep: bool,
    ) -> Result<Vec<ConstraintUid>, String> {
        let dependents = self.dependents(id);
        // Taken out first, the last made first: their sources go with this
        // offset's curves.
        let mut torn: Vec<TornDown> = Vec::new();
        for dependent in dependents.iter().rev() {
            torn.push(self.tear_down(*dependent, true)?);
        }
        torn.reverse();
        let own = self.tear_down(id, keep)?;
        let mut map: BTreeMap<EntityUid, EntityUid> = BTreeMap::new();
        let curves = own.offset.curves.clone();
        let made = self.build_again(&own, &curves, flip, &mut map)?;
        let mut remade: Vec<Remade> = vec![(own.offset.clone(), made)];
        let mut done = vec![id];
        let known = self.known_sources.clone();
        for t in &torn {
            let (curves, mut lost) = carried(&t.offset.curves, &remade);
            // (Results of an offset removed before it are gone too.)
            let (curves, gone): (Vec<EntityUid>, Vec<EntityUid>) = curves
                .into_iter()
                .partition(|c| self.def.entity(*c).is_some());
            lost.extend(gone);
            if curves.is_empty() {
                self.report.removed.push(t.offset.id.to_string());
                self.report.warnings.push(format!(
                    "offset {} is removed: the curves it offset vanish at this distance",
                    t.offset.id
                ));
                continue;
            }
            // From the curves just made, where their points were placed.
            let fresh: BTreeSet<EntityUid> =
                remade.iter().flat_map(|(_, new)| new.made()).collect();
            let geometry = self.placed(&curves, &fresh, known.as_ref());
            let ordered = self.ordered_like(&curves, &t.forward, &geometry)?;
            self.known_sources = Some(geometry);
            let built = self.build_again(t, &ordered, false, &mut map);
            self.known_sources = known.clone();
            let built = built?;
            if !lost.is_empty() {
                let names: Vec<String> = lost.iter().map(|c| format!("c{}", c.0)).collect();
                self.report.notes.push(format!(
                    "offset {} is made again without {}, which {} at this distance",
                    t.offset.id,
                    listed(&names),
                    if names.len() == 1 {
                        "vanishes"
                    } else {
                        "vanish"
                    }
                ));
            }
            remade.push((t.offset.clone(), built));
            done.push(t.offset.id);
        }
        // The user's constraints and dimensions back on what they were on.
        let mut constraints = Vec::new();
        let mut dimensions = Vec::new();
        for t in std::iter::once(own).chain(torn) {
            constraints.extend(t.constraints);
            dimensions.extend(t.dimensions);
        }
        self.restore_on(constraints, dimensions, &map);
        // What is there again was not removed.
        let present: BTreeSet<String> = self
            .def
            .entities
            .iter()
            .map(|e| e.as_ref().to_string())
            .chain(self.def.constraints.iter().map(|c| c.id.to_string()))
            .chain(self.def.dimensions.iter().map(|d| d.id.to_string()))
            .chain(self.def.offsets.iter().map(|o| o.id.to_string()))
            .collect();
        self.report.removed.retain(|r| !present.contains(r));
        Ok(done)
    }

    /// The offsets of an offset's results and helpers, and of theirs
    /// (mitcad#1244), in the definition's order.
    fn dependents(&self, id: ConstraintUid) -> Vec<ConstraintUid> {
        let mut made: BTreeSet<EntityUid> = self
            .def
            .offsets
            .iter()
            .filter(|o| o.id == id)
            .flat_map(made_and_helpers)
            .collect();
        let mut found: BTreeSet<ConstraintUid> = BTreeSet::new();
        loop {
            let more: Vec<&SketchOffset<ParamId>> = self
                .def
                .offsets
                .iter()
                .filter(|o| o.id != id && !found.contains(&o.id))
                .filter(|o| o.curves.iter().any(|c| made.contains(c)))
                .collect();
            if more.is_empty() {
                break;
            }
            for o in more {
                found.insert(o.id);
                made.extend(made_and_helpers(o));
            }
        }
        self.def
            .offsets
            .iter()
            .map(|o| o.id)
            .filter(|k| found.contains(k))
            .collect()
    }

    /// Takes an offset out of the sketch to be made again: its record, the
    /// curves it made with the points only they use and, with `keep`, the
    /// user's constraints and dimensions on them (not the offset's own),
    /// which are kept.
    fn tear_down(&mut self, id: ConstraintUid, keep: bool) -> Result<TornDown, String> {
        let offset = self
            .def
            .offsets
            .iter()
            .find(|o| o.id == id)
            .cloned()
            .ok_or_else(|| format!("offset {id} does not exist"))?;
        let forward: Vec<(EntityUid, bool)> = {
            let index = EntityIndex::new(&self.def.entities);
            let directions = chain_directions(&offset.curves, &index);
            offset.curves.iter().copied().zip(directions).collect()
        };
        let made: BTreeSet<EntityUid> = made_and_helpers(&offset).into_iter().collect();
        // The points of each result and corner arc, in their order.
        let points: Vec<(EntityUid, Vec<EntityUid>)> = offset
            .made()
            .iter()
            .filter_map(|c| Some((*c, self.def.entity(*c)?.kind.points())))
            .collect();
        // What the user put on them: not the offset's own constraints.
        let mut constraints = Vec::new();
        let mut dimensions = Vec::new();
        if keep {
            let own = constraints_of(
                &offset,
                &self.def.entities,
                &self.def.constraints,
                &self.def.dimensions,
            );
            let mut on = made.clone();
            for (_, list) in &points {
                for p in list {
                    let elsewhere = self
                        .def
                        .entities
                        .iter()
                        .any(|e| !made.contains(&e.id) && e.kind.points().contains(p));
                    if !elsewhere {
                        on.insert(*p);
                    }
                }
            }
            let touches = |refs: Vec<Ref>| refs.iter().any(|r| on.contains(&r.uid()));
            let (taken, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.def.constraints)
                .into_iter()
                .partition(|c| !own.contains(&c.id) && touches(c.kind.refs()));
            self.def.constraints = rest;
            constraints = taken;
            let (taken, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.def.dimensions)
                .into_iter()
                .partition(|d| !own.contains(&d.id) && touches(d.kind.refs()));
            self.def.dimensions = rest;
            dimensions = taken;
        }
        self.drop_offset_curves(&made);
        self.def.offsets.retain(|o| o.id != id);
        Ok(TornDown {
            offset,
            forward,
            points,
            constraints,
            dimensions,
        })
    }

    /// Makes an offset taken out ([`SketchEdit::tear_down`]) again of
    /// `curves` (in chain order) at its distance, on the other side with
    /// `flip`, with its id and the ids of the results of the source curves
    /// it had results of. Adds to `map` the points made again in the same
    /// place of the same curve, and the result of a curve whose offset now
    /// runs on along another's (mitcad#1244) with its far end, onto that
    /// one's. The offset as made.
    fn build_again(
        &mut self,
        t: &TornDown,
        curves: &[EntityUid],
        flip: bool,
        map: &mut BTreeMap<EntityUid, EntityUid>,
    ) -> Result<SketchOffset<ParamId>, String> {
        let offset = &t.offset;
        let name = self.params.name(offset.distance);
        let value = self
            .params
            .value(offset.distance)
            .ok_or("the offset's parameter is gone")?;
        let left = offset.left != flip;
        let sign = if left { -1.0 } else { 1.0 };
        self.reuse_results = offset.pairs().into_iter().collect();
        let remade = if offset.derived {
            let index = EntityIndex::new(&self.def.entities);
            let forward = chain_directions(curves, &index);
            let chain: Vec<(EntityUid, bool)> = curves.iter().copied().zip(forward).collect();
            let closed = self.chain_closed(&chain);
            self.derived_offset(&chain, closed, &ValueInput::Name(name), sign * value)
        } else {
            self.offset_with(curves, &ValueInput::Name(name), sign * value)
        };
        self.reuse_results.clear();
        self.renumber_offset(remade?.0, offset.id);
        let made = self
            .def
            .offsets
            .iter()
            .find(|o| o.id == offset.id)
            .cloned()
            .ok_or_else(|| format!("offset {} was not made again", offset.id))?;
        // The points made again in the same place of the same curve.
        for (curve, old) in &t.points {
            if let Some(e) = self.def.entity(*curve) {
                let new = e.kind.points();
                if new.len() == old.len() {
                    map.extend(old.iter().copied().zip(new).filter(|(a, b)| a != b));
                }
            }
        }
        // A result whose source now runs on along another's: that one's
        // result, its end (along the chain, where both end) that one's end.
        let before: BTreeMap<EntityUid, EntityUid> = offset.pairs().into_iter().collect();
        let after: BTreeMap<EntityUid, EntityUid> = made.pairs().into_iter().collect();
        for m in &made.merged {
            let (Some(old), Some(into)) = (before.get(&m.curve), after.get(&m.into)) else {
                continue;
            };
            map.insert(*old, *into);
            // An arc's end along the chain is its start where the chain runs
            // clockwise round it (mitcad#1246: arcs on one circle; both run
            // the same way).
            let forward = !t.forward.contains(&(m.into, false));
            let old_end =
                t.points
                    .iter()
                    .find(|(c, _)| c == old)
                    .and_then(|(_, p)| match p.as_slice() {
                        [_, start, _] if !forward => Some(*start),
                        _ => p.last().copied(),
                    });
            let new_end = match self.def.entity(*into).map(|e| &e.kind) {
                Some(EntityKind::Line { end, .. }) => Some(*end),
                Some(EntityKind::Arc { end, .. }) if forward => Some(*end),
                Some(EntityKind::Arc { start, .. }) => Some(*start),
                _ => None,
            };
            if let (Some(a), Some(b)) = (old_end, new_end) {
                map.entry(a).or_insert(b);
            }
        }
        Ok(made)
    }

    /// Puts back the user's constraints and dimensions taken off offsets
    /// made again, moved by `map` (old ids to new); those on what is gone,
    /// and those whose curves are now one (mitcad#1244: on two results that
    /// became one line), are removed with a warning.
    fn restore_on(
        &mut self,
        constraints: Vec<super::Constraint>,
        dimensions: Vec<super::Dimension<ParamId>>,
        map: &BTreeMap<EntityUid, EntityUid>,
    ) {
        let distinct = |refs: &[Ref]| refs.iter().map(|r| r.uid()).collect::<BTreeSet<_>>().len();
        // Each removed one, and whether its curves are now one.
        let mut lost: Vec<(ConstraintUid, Vec<Ref>, bool)> = Vec::new();
        for mut c in constraints {
            let before = distinct(&c.kind.refs());
            super::modify::remap(&mut c.kind, map);
            let refs = c.kind.refs();
            if !all_exist(&self.def, &refs) {
                lost.push((c.id, refs, false));
            } else if distinct(&refs) < before {
                lost.push((c.id, refs, true));
            } else {
                self.def.constraints.push(c);
            }
        }
        for mut d in dimensions {
            let before = distinct(&d.kind.refs());
            super::cleanup::remap_dimension(&mut d.kind, map);
            let refs = d.kind.refs();
            if !all_exist(&self.def, &refs) {
                lost.push((d.id, refs, false));
            } else if distinct(&refs) < before {
                lost.push((d.id, refs, true));
            } else {
                self.def.dimensions.push(d);
            }
        }
        for (k, refs, one) in lost {
            self.report.removed.push(k.to_string());
            if one {
                self.report.warnings.push(format!(
                    "{k} is removed: the curves it was on are one line at this distance"
                ));
                continue;
            }
            let gone: Vec<String> = refs
                .iter()
                .filter(|r| self.def.entity(r.uid()).is_none())
                .map(ToString::to_string)
                .collect();
            self.report.warnings.push(format!(
                "{k} is removed: it was on {}, which vanishes at this distance",
                listed(&gone)
            ));
        }
    }

    /// The geometry an offset of `curves` is made from while offsets are
    /// made again (mitcad#1244): the sources' (`known`, else the sketch
    /// solved now), with the curves just made (`fresh`) and any curve that
    /// geometry lacks where their points are placed (lines, arcs, circles).
    fn placed(
        &self,
        curves: &[EntityUid],
        fresh: &BTreeSet<EntityUid>,
        known: Option<&Solved>,
    ) -> Solved {
        let mut solved = match known {
            Some(solved) => solved.clone(),
            None => match self.solved() {
                Ok(solved) => return solved,
                Err(_) => Solved::default(),
            },
        };
        let stored = |p: EntityUid| match self.def.entity(p).map(|e| &e.kind) {
            Some(EntityKind::Point { at }) => Some(*at),
            _ => None,
        };
        for c in curves {
            if !fresh.contains(c) && solved.curves.contains_key(c) {
                continue;
            }
            // Its points where they are placed, but those the geometry has
            // (a centre a result shares with its source).
            let points = self.def.entity(*c).map(|e| e.kind.points());
            for p in points.unwrap_or_default() {
                if !solved.points.contains_key(&p)
                    && let Some(at) = stored(p)
                {
                    solved.points.insert(p, at);
                }
            }
            let curve = self.curve_placed(*c, &|p| solved.points.get(&p).copied());
            if let Some(curve) = curve {
                solved.curves.insert(*c, curve);
            }
        }
        solved
    }

    /// `curves` in the order of the chain they make, running the way the
    /// curves they share with `before` (each with whether it ran from its
    /// start to its end) ran: an offset made again keeps its side.
    fn ordered_like(
        &self,
        curves: &[EntityUid],
        before: &[(EntityUid, bool)],
        geometry: &Solved,
    ) -> Result<Vec<EntityUid>, String> {
        let chain = self.chain_order(curves, geometry)?;
        let turned = chain
            .iter()
            .find_map(|(c, forward)| {
                before
                    .iter()
                    .find(|(b, _)| b == c)
                    .map(|(_, was)| was != forward)
            })
            .unwrap_or(false);
        let mut out: Vec<EntityUid> = chain.into_iter().map(|(c, _)| c).collect();
        if turned {
            out.reverse();
        }
        Ok(out)
    }

    /// A line, arc or circle of the sketch with its points where `at` puts
    /// them.
    fn curve_placed(&self, id: EntityUid, at: &dyn Fn(EntityUid) -> Option<P2>) -> Option<Curve2> {
        Some(match &self.def.entity(id)?.kind {
            EntityKind::Line { start, end, .. } => Curve2::Line {
                a: at(*start)?,
                b: at(*end)?,
            },
            EntityKind::Arc { center, start, end } => {
                let (c, a, b) = (at(*center)?, at(*start)?, at(*end)?);
                let (va, vb) = (sub(a, c), sub(b, c));
                let start = va[1].atan2(va[0]);
                Curve2::Arc {
                    center: c,
                    radius: norm(va),
                    start,
                    end: angle_after(start, vb[1].atan2(vb[0])),
                }
            }
            EntityKind::Circle {
                center,
                radius,
                start,
            } => Curve2::Circle {
                center: at(*center)?,
                radius: *radius,
                start: *start,
            },
            _ => return None,
        })
    }

    /// Fits the pieces of derived spline offsets to the sources' geometry
    /// `solved` again (every edit does): a source that changed shape may
    /// need more or fewer control points. Only inner control points change;
    /// the curves keep their ids.
    pub(crate) fn refit_offsets(&mut self, solved: &Solved) -> Result<(), String> {
        if !self.def.offsets.iter().any(|o| o.derived) {
            return Ok(());
        }
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
                    knots: Vec::new(),
                    joins: stored_joins(offset, &chain, closed),
                    dropped: offset.dropped_indices(),
                    next: offset.links(closed),
                };
                let computed = compute(&chain, d, closed, Some(&structure), &offset.namer())
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
