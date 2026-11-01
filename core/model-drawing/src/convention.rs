// SPDX-License-Identifier: MIT
//! Drawing conventions applied to a view's projected lines (mitcad#1112),
//! all in Rust on what the kernel projected, so that they work alike on
//! the exact and the triangulated projections:
//!
//! - **Details smaller than the pen** ([`simplified`]): ISO 128-20 keeps
//!   parallel lines at least twice the visible line's width apart; closer
//!   lines merge into a blot. A fillet or chamfer whose size on the sheet
//!   is below that ([`detail_size`]) is drawn as a sharp corner: its arc
//!   (or the polyline or the short lines a triangulation made of it, or a
//!   chamfer's short line) is left out with the tangent edges that end at
//!   it, and the edges on either side are extended to meet. Tangent edges
//!   that lie that close to other edges all along are left out, and so is
//!   any edge shorter than its pen is wide (the round caps of the lines it
//!   joins cover the gap it leaves). `compose::render` draws what is left,
//!   at the view's scale.
//! - **Threads by ISO 6410** ([`with_threads`]): a thread (one the
//!   `threads` query lists: modelled, or cosmetic, a hole or a shaft with a
//!   thread's designation and no flanks) is drawn by convention instead of
//!   its geometry. In a view across its axis: the crests (an external
//!   thread's major diameter, an internal one's minor) with the visible
//!   pen, the roots with the thin one, the thread's end with a visible
//!   line across the major diameter where no face ends it; looking along
//!   its axis: the crest circle, and the root circle as a thin three-quarter
//!   arc. The thread's own lines (crests, helices, flanks, the curves where
//!   the end chamfers cut them) are left out. Each part takes the
//!   visibility of the lines it replaces along the axis, as the kernel's
//!   hidden-line removal gave it, so where a nut or a wall hides a thread
//!   its lines are hidden (dashed, or left out when the view shows no
//!   hidden edges). A pictorial view (at an angle to the axis) draws the
//!   thread on its cylinder the same way: ISO 6410 has no pictorial
//!   representation, and this is the simplified one ISO 128 allows there.
//!   A taper pipe thread's crest and root lines follow its taper.

use std::collections::{HashMap, HashSet};
use std::f64::consts::{PI, TAU};

use mitcad_model::datum::Vec3;
use mitcad_vec3::vec2::{self, cross as cross2, perp};
use mitcad_vec3::vec2::{add as add2, dot as dot2, norm as len2, scale as mul2, sub as sub2};
use mitcad_vec3::{add, cross, dot, norm, scale, sub, unit_div as unit};

use super::compose::edge_pen;
use super::sheet::Pen;
use super::{P2, ViewCurve, ViewEdge, ViewEdgeKind, ViewProjection, ViewSpec, ViewSpline};

/// A fillet or chamfer smaller than this many visible pen widths on the
/// sheet is drawn as a sharp corner.
pub const DETAIL_PENS: f64 = 2.0;

/// The size on the sheet (mm) below which a fillet or chamfer is drawn as
/// a sharp corner: twice the visible pen's width, ISO 128-20's least
/// distance between parallel lines.
pub fn detail_size() -> f64 {
    DETAIL_PENS * Pen::Thick.width()
}

// --- Curves --------------------------------------------------------------

fn arc_point(center: P2, radius: f64, angle: f64) -> P2 {
    [
        center[0] + radius * angle.cos(),
        center[1] + radius * angle.sin(),
    ]
}

fn is_full(start: f64, end: f64) -> bool {
    end - start >= TAU - 1e-9
}

/// A curve's two ends; none for a closed one.
fn ends_of(curve: &ViewCurve) -> Option<[P2; 2]> {
    match curve {
        ViewCurve::Line(a, b) => Some([*a, *b]),
        ViewCurve::Arc {
            center,
            radius,
            start,
            end,
        } => (!is_full(*start, *end)).then(|| {
            [
                arc_point(*center, *radius, *start),
                arc_point(*center, *radius, *end),
            ]
        }),
        ViewCurve::Polyline(points) | ViewCurve::Spline(ViewSpline { points, .. }) => {
            let (first, last) = (*points.first()?, *points.last()?);
            (points.len() >= 2 && vec2::distance(first, last) > 1e-12).then_some([first, last])
        }
    }
}

/// Points along a curve, its ends among them.
fn points_of(curve: &ViewCurve) -> Vec<P2> {
    match curve {
        ViewCurve::Line(a, b) => vec![*a, *b],
        ViewCurve::Arc {
            center,
            radius,
            start,
            end,
        } => {
            let steps = ((end - start).abs() / (PI / 16.0)).ceil().max(2.0) as usize;
            (0..=steps)
                .map(|k| {
                    arc_point(
                        *center,
                        *radius,
                        start + (end - start) * k as f64 / steps as f64,
                    )
                })
                .collect()
        }
        ViewCurve::Polyline(points) | ViewCurve::Spline(ViewSpline { points, .. }) => {
            points.clone()
        }
    }
}

/// The diagonal of the box of some points.
fn extent(points: &[P2]) -> f64 {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in points {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    if lo[0] > hi[0] {
        return 0.0;
    }
    len2(sub2(hi, lo))
}

/// The longer side of the box of some points: the size a detail is measured
/// by, so that a quarter arc of radius r is as big as a chamfer of r (its
/// diagonal would be r·√2).
fn span(points: &[P2]) -> f64 {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in points {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    if lo[0] > hi[0] {
        return 0.0;
    }
    (hi[0] - lo[0]).max(hi[1] - lo[1])
}

/// The unit direction of a curve at one of its ends, pointing into it.
fn inward(curve: &ViewCurve, at_start: bool) -> Option<P2> {
    match curve {
        ViewCurve::Line(a, b) => vec2::unit(if at_start { sub2(*b, *a) } else { sub2(*a, *b) }),
        ViewCurve::Arc { start, end, .. } => Some(if at_start {
            [-start.sin(), start.cos()]
        } else {
            [end.sin(), -end.cos()]
        }),
        ViewCurve::Polyline(points) | ViewCurve::Spline(ViewSpline { points, .. }) => {
            let n = points.len();
            if at_start {
                (1..n).find_map(|i| vec2::unit(sub2(points[i], points[0])))
            } else {
                (0..n.saturating_sub(1))
                    .rev()
                    .find_map(|i| vec2::unit(sub2(points[i], points[n - 1])))
            }
        }
    }
}

/// How far a point is from a segment.
fn segment_distance(p: P2, a: P2, b: P2) -> f64 {
    let d = sub2(b, a);
    let length = dot2(d, d);
    let t = if length > 1e-24 {
        (dot2(sub2(p, a), d) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    vec2::distance(p, add2(a, mul2(d, t)))
}

/// How far a point is from a curve.
fn curve_distance(p: P2, curve: &ViewCurve) -> f64 {
    match curve {
        ViewCurve::Line(a, b) => segment_distance(p, *a, *b),
        ViewCurve::Arc {
            center,
            radius,
            start,
            end,
        } => {
            let angle = (p[1] - center[1]).atan2(p[0] - center[0]);
            let t = (angle - start).rem_euclid(TAU);
            if t <= end - start + 1e-12 {
                (vec2::distance(p, *center) - radius).abs()
            } else {
                vec2::distance(p, arc_point(*center, *radius, *start))
                    .min(vec2::distance(p, arc_point(*center, *radius, *end)))
            }
        }
        ViewCurve::Polyline(points) | ViewCurve::Spline(ViewSpline { points, .. }) => points
            .windows(2)
            .map(|w| segment_distance(p, w[0], w[1]))
            .fold(f64::INFINITY, f64::min),
    }
}

/// Points along a curve no further apart than `step` (at most `limit`).
fn dense_points(curve: &ViewCurve, step: f64, limit: usize) -> Vec<P2> {
    let coarse = points_of(curve);
    let mut out = Vec::new();
    for w in coarse.windows(2) {
        let pieces = (vec2::distance(w[0], w[1]) / step)
            .ceil()
            .clamp(1.0, limit as f64) as usize;
        for k in 0..pieces {
            out.push(vec2::lerp(w[0], w[1], k as f64 / pieces as f64));
        }
    }
    out.extend(coarse.last());
    out
}

// --- Details smaller than the pen ----------------------------------------

/// Points within a tolerance of each other as one node.
struct Nodes {
    eps: f64,
    cell: f64,
    points: Vec<P2>,
    grid: HashMap<(i64, i64), Vec<usize>>,
}

impl Nodes {
    fn new(eps: f64) -> Self {
        Self {
            eps,
            cell: (2.0 * eps).max(1e-12),
            points: Vec::new(),
            grid: HashMap::new(),
        }
    }

    fn key(&self, p: P2) -> (i64, i64) {
        (
            (p[0] / self.cell).floor() as i64,
            (p[1] / self.cell).floor() as i64,
        )
    }

    fn of(&mut self, p: P2) -> usize {
        let (kx, ky) = self.key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = self.grid.get(&(kx + dx, ky + dy)) {
                    for &i in list {
                        if vec2::distance(self.points[i], p) <= self.eps {
                            return i;
                        }
                    }
                }
            }
        }
        self.points.push(p);
        let i = self.points.len() - 1;
        self.grid.entry((kx, ky)).or_default().push(i);
        i
    }
}

/// Edges by the cells their points fall in, for "which edges come near
/// this point".
struct EdgeGrid {
    cell: f64,
    cells: HashMap<(i64, i64), Vec<usize>>,
}

impl EdgeGrid {
    fn new(cell: f64) -> Self {
        Self {
            cell: cell.max(1e-12),
            cells: HashMap::new(),
        }
    }

    fn key(&self, p: P2) -> (i64, i64) {
        (
            (p[0] / self.cell).floor() as i64,
            (p[1] / self.cell).floor() as i64,
        )
    }

    fn insert(&mut self, edge: usize, curve: &ViewCurve) {
        let mut last = None;
        for p in dense_points(curve, self.cell / 2.0, 100_000) {
            let k = self.key(p);
            if last != Some(k) {
                let list = self.cells.entry(k).or_default();
                if list.last() != Some(&edge) {
                    list.push(edge);
                }
                last = Some(k);
            }
        }
    }

    /// The edges with a point in the cells about `p` (within a cell).
    fn near(&self, p: P2) -> HashSet<usize> {
        let (kx, ky) = self.key(p);
        let mut out = HashSet::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = self.cells.get(&(kx + dx, ky + dy)) {
                    out.extend(list.iter().copied());
                }
            }
        }
        out
    }
}

/// The cells of an [`EdgeGrid`] of a view's lines: at least `least`, and
/// large enough that the grid holds at most a few hundred thousand entries
/// however long the lines are against it.
fn grid_cell(work: &[Option<ViewEdge>], least: f64) -> f64 {
    let total: f64 = work
        .iter()
        .flatten()
        .map(|e| {
            points_of(&e.curve)
                .windows(2)
                .map(|w| vec2::distance(w[0], w[1]))
                .sum::<f64>()
        })
        .sum();
    least.max(total / 200_000.0)
}

/// A neighbour of a detail at one of its ends: the edge, the end of it
/// there (none: a line passing through), and its direction away from the
/// detail.
struct Neighbour {
    edge: usize,
    end: Option<usize>,
    direction: P2,
}

/// The lines a view draws with its details smaller than the pen
/// simplified (see the module's text), at `scale` sheet millimetres per
/// view millimetre.
pub fn simplified(edges: &[ViewEdge], scale: f64) -> Vec<ViewEdge> {
    if !(scale > 0.0 && scale.is_finite()) || edges.is_empty() {
        return edges.to_vec();
    }
    let detail = detail_size() / scale;
    let mut work: Vec<Option<ViewEdge>> = edges.iter().cloned().map(Some).collect();
    let mut protected = vec![false; work.len()];
    sharp_corners(&mut work, &mut protected, detail);
    close_tangent_edges(&mut work, &protected, detail);
    // Edges shorter than their pen is wide.
    for (edge, keep) in work.iter_mut().zip(&protected) {
        let short = edge.as_ref().is_some_and(|e| {
            extent(&points_of(&e.curve)) < edge_pen(e.visible, e.kind).width() / scale
        });
        if short && !keep {
            *edge = None;
        }
    }
    work.into_iter().flatten().collect()
}

fn is_smooth(e: &ViewEdge) -> bool {
    e.kind == ViewEdgeKind::Smooth
}

/// Whether an edge is small enough to be a detail: a line shorter than
/// `detail`, an arc of a smaller radius, a polyline within a box of that
/// size ([`span`]).
fn is_small(curve: &ViewCurve, detail: f64) -> bool {
    match curve {
        ViewCurve::Line(a, b) => vec2::distance(*a, *b) < detail,
        ViewCurve::Arc {
            radius, start, end, ..
        } => *radius < detail && !is_full(*start, *end),
        ViewCurve::Polyline(points) | ViewCurve::Spline(ViewSpline { points, .. }) => {
            span(points) < detail
        }
    }
}

/// Fillets and chamfers smaller than `detail` drawn as sharp corners: each
/// chain of small edges between two other edges is left out, the tangent
/// edges that end at it too, and its neighbours are extended to where they
/// meet. The neighbours changed and the lines added are `protected` from
/// the later steps.
fn sharp_corners(work: &mut Vec<Option<ViewEdge>>, protected: &mut Vec<bool>, detail: f64) {
    let count = work.len();
    let small: Vec<bool> = work
        .iter()
        .map(|e| {
            e.as_ref().is_some_and(|e| {
                !is_smooth(e)
                    && ends_of(&e.curve).is_some()
                    && is_small(&e.curve, detail)
                    // An edge seen end on (a point) goes with the short ones.
                    && extent(&points_of(&e.curve)) > 1e-12
            })
        })
        .collect();
    if !small.iter().any(|s| *s) {
        return;
    }
    let eps = detail * 0.02;
    let mut nodes = Nodes::new(eps);
    let mut edge_nodes: Vec<Option<[usize; 2]>> = vec![None; count];
    let mut at_node: Vec<Vec<(usize, usize)>> = Vec::new();
    for (i, e) in work.iter().enumerate() {
        let Some(ends) = e.as_ref().and_then(|e| ends_of(&e.curve)) else {
            continue;
        };
        let pair = [nodes.of(ends[0]), nodes.of(ends[1])];
        at_node.resize(nodes.points.len(), Vec::new());
        at_node[pair[0]].push((i, 0));
        at_node[pair[1]].push((i, 1));
        edge_nodes[i] = Some(pair);
    }
    // Lines by where they run, for the neighbours a detail ends on midway.
    let mut lines = EdgeGrid::new(grid_cell(work, detail));
    for (i, e) in work.iter().enumerate() {
        if let Some(e) = e
            && matches!(e.curve, ViewCurve::Line(..))
            && !is_smooth(e)
            && !small[i]
        {
            lines.insert(i, &e.curve);
        }
    }
    let visible = |work: &[Option<ViewEdge>], i: usize| work[i].as_ref().is_some_and(|e| e.visible);
    let sharp_at = |work: &[Option<ViewEdge>], node: usize| -> Vec<(usize, usize)> {
        at_node[node]
            .iter()
            .copied()
            .filter(|(e, _)| work[*e].as_ref().is_some_and(|e| !is_smooth(e)))
            .collect()
    };
    let mut visited = vec![false; count];
    for first in 0..count {
        if !small[first] || visited[first] || work[first].is_none() {
            continue;
        }
        let Some(first_nodes) = edge_nodes[first] else {
            continue;
        };
        let seen = visible(&work[..], first);
        // The chain: small edges of one visibility joined end to end where
        // no other edge meets them.
        let mut chain = vec![first];
        visited[first] = true;
        let mut tips = [(first, 0usize), (first, 1usize)];
        let mut closed = false;
        for (side, tip) in tips.iter_mut().enumerate() {
            let (mut edge, mut end) = (first, side);
            loop {
                let node = edge_nodes[edge].map_or(first_nodes[side], |n| n[end]);
                let here = sharp_at(&work[..], node);
                if here.len() != 2 {
                    break;
                }
                let Some(&(next, next_end)) = here.iter().find(|(e, _)| *e != edge) else {
                    break;
                };
                if next == first {
                    closed = true;
                    break;
                }
                if !small[next] || visited[next] || visible(&work[..], next) != seen {
                    break;
                }
                visited[next] = true;
                chain.push(next);
                edge = next;
                end = 1 - next_end;
            }
            *tip = (edge, end);
        }
        if closed {
            continue;
        }
        let points: Vec<P2> = chain
            .iter()
            .filter_map(|e| work[*e].as_ref())
            .flat_map(|e| points_of(&e.curve))
            .collect();
        if span(&points) >= detail {
            continue;
        }
        let in_chain = |e: usize| chain.contains(&e);
        // The neighbour at each tip: the other edge there of the same
        // visibility that continues the detail's direction best.
        let neighbour =
            |work: &[Option<ViewEdge>], (edge, end): (usize, usize)| -> Option<Neighbour> {
                let node = edge_nodes[edge]?[end];
                let at = nodes.points[node];
                let curve = &work[edge].as_ref()?.curve;
                let out = mul2(inward(curve, end == 0)?, -1.0);
                let mut best: Option<(f64, Neighbour)> = None;
                let mut offer = |score: f64, n: Neighbour| {
                    if best.as_ref().is_none_or(|(s, _)| score > *s) {
                        best = Some((score, n));
                    }
                };
                for &(e, e_end) in &at_node[node] {
                    let Some(other) = work[e].as_ref() else {
                        continue;
                    };
                    if in_chain(e) || small[e] || is_smooth(other) || other.visible != seen {
                        continue;
                    }
                    if let Some(direction) = inward(&other.curve, e_end == 0) {
                        offer(
                            dot2(direction, out),
                            Neighbour {
                                edge: e,
                                end: Some(e_end),
                                direction,
                            },
                        );
                    }
                }
                let mut passing: Vec<usize> = lines.near(at).into_iter().collect();
                passing.sort_unstable();
                for e in passing {
                    let Some(other) = work[e].as_ref() else {
                        continue;
                    };
                    let ViewCurve::Line(a, b) = other.curve else {
                        continue;
                    };
                    if in_chain(e) || other.visible != seen {
                        continue;
                    }
                    if vec2::distance(at, a) <= eps || vec2::distance(at, b) <= eps {
                        continue; // ends there: above
                    }
                    if segment_distance(at, a, b) > eps {
                        continue;
                    }
                    let Some(d) = vec2::unit(sub2(b, a)) else {
                        continue;
                    };
                    let direction = if dot2(d, out) >= 0.0 {
                        d
                    } else {
                        mul2(d, -1.0)
                    };
                    offer(
                        dot2(direction, out),
                        Neighbour {
                            edge: e,
                            end: None,
                            direction,
                        },
                    );
                }
                best.map(|(_, n)| n)
            };
        let (Some(n0), Some(n1)) = (neighbour(&work[..], tips[0]), neighbour(&work[..], tips[1]))
        else {
            continue;
        };
        if n0.edge == n1.edge {
            continue;
        }
        let tip_point =
            |(edge, end): (usize, usize)| edge_nodes[edge].map(|n| nodes.points[n[end]]);
        let (Some(p0), Some(p1)) = (tip_point(tips[0]), tip_point(tips[1])) else {
            continue;
        };
        // Where the neighbours' lines meet.
        let den = cross2(n0.direction, n1.direction);
        if den.abs() < (5.0f64).to_radians().sin() {
            continue;
        }
        let w = sub2(p1, p0);
        let s0 = cross2(w, n1.direction) / den;
        let s1 = cross2(w, n0.direction) / den;
        let corner = add2(p0, mul2(n0.direction, s0));
        let behind = |n: &Neighbour, s: f64| n.end.is_none() || s <= eps;
        if !behind(&n0, s0)
            || !behind(&n1, s1)
            || vec2::distance(corner, p0) > 3.0 * detail
            || vec2::distance(corner, p1) > 3.0 * detail
        {
            continue;
        }
        // The detail and its tangent edges go.
        let mut chain_nodes: Vec<usize> = Vec::new();
        for &e in &chain {
            if let Some(n) = edge_nodes[e] {
                chain_nodes.extend(n);
            }
            work[e] = None;
        }
        for node in chain_nodes {
            for &(e, _) in &at_node[node] {
                if work[e].as_ref().is_some_and(is_smooth) {
                    work[e] = None;
                }
            }
        }
        // The neighbours reach the corner.
        for (n, from) in [(&n0, p0), (&n1, p1)] {
            let Some(other) = work[n.edge].as_mut() else {
                continue;
            };
            let (visible, kind, source) = (other.visible, other.kind, other.source);
            let ViewCurve::Line(a, b) = &mut other.curve else {
                // A curve: a line on from its end to the corner.
                work.push(Some(ViewEdge {
                    visible,
                    kind,
                    curve: ViewCurve::Line(from, corner),
                    source,
                }));
                protected.push(true);
                continue;
            };
            match n.end {
                Some(0) => *a = corner,
                Some(_) => *b = corner,
                None => {
                    // A line the detail ends on midway: on to the corner
                    // when that lies beyond its ends.
                    let d = sub2(*b, *a);
                    let t = dot2(sub2(corner, *a), d) / dot2(d, d).max(1e-300);
                    if t < 0.0 {
                        *a = corner;
                    } else if t > 1.0 {
                        *b = corner;
                    }
                }
            }
            protected[n.edge] = true;
        }
    }
}

/// Tangent edges that lie within `detail` of other edges all along are
/// left out: they would merge with them.
fn close_tangent_edges(work: &mut [Option<ViewEdge>], protected: &[bool], detail: f64) {
    if !work.iter().flatten().any(is_smooth) {
        return;
    }
    let mut grid = EdgeGrid::new(grid_cell(work, detail));
    for (i, e) in work.iter().enumerate() {
        if let Some(e) = e
            && !is_smooth(e)
        {
            grid.insert(i, &e.curve);
        }
    }
    let mut gone = Vec::new();
    for (i, e) in work.iter().enumerate() {
        let Some(e) = e else { continue };
        if !is_smooth(e) || protected.get(i).copied().unwrap_or(false) {
            continue;
        }
        let step = (extent(&points_of(&e.curve)) / 8.0).max(detail / 4.0);
        let covered = dense_points(&e.curve, step, 64).into_iter().all(|p| {
            grid.near(p).into_iter().any(|o| {
                work[o]
                    .as_ref()
                    .is_some_and(|o| curve_distance(p, &o.curve) <= detail)
            })
        });
        if covered {
            gone.push(i);
        }
    }
    for i in gone {
        work[i] = None;
    }
}

// --- Threads by ISO 6410 -------------------------------------------------

/// A thread to draw by convention, in the coordinates of the view's
/// model: its axis from where it starts to where it ends (the `threads`
/// query's `start` and `end`, placed), its major and minor diameters,
/// whether it is internal (a hole's), and a taper pipe thread's taper.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadInput {
    pub start: Vec3,
    pub end: Vec3,
    pub major: f64,
    pub minor: f64,
    pub internal: bool,
    /// The diameters' change per length along the axis (1/16 for a taper
    /// pipe thread, mitcad#733), 0 for a straight thread. The diameters
    /// above are those where the thread starts, at its free end: an
    /// external thread grows from there, an internal one narrows.
    pub taper: f64,
    /// The shape the thread is on: its index among those projected (the
    /// edges' `source`). Only its lines are left out and replaced; every
    /// other body's stay as the kernel drew them. None: any line.
    pub source: Option<usize>,
}

impl ThreadInput {
    /// Whether an edge may be one of the thread's: of its shape, or of
    /// a shape not known.
    fn owns(&self, edge: &ViewEdge) -> bool {
        self.source.is_none() || edge.source.is_none() || edge.source == self.source
    }
}

/// A view's projection with its threads drawn by ISO 6410 (see the
/// module's text): before a detail's circle or a break cuts it. The lines
/// one thread draws are not taken for another's.
pub fn with_threads(
    projection: &ViewProjection,
    spec: &ViewSpec,
    threads: &[ThreadInput],
) -> ViewProjection {
    let mut edges = projection.edges.clone();
    let mut added = Vec::new();
    for thread in threads {
        if !(thread.major > thread.minor && thread.minor > 0.0) {
            continue;
        }
        let Some(axis) = unit(sub(thread.end, thread.start)) else {
            continue;
        };
        let length = norm(sub(thread.end, thread.start));
        let along = dot(axis, spec.toward_eye);
        let s = spec.project(thread.start);
        if along.abs() > 1.0 - 1e-6 {
            end_on(&mut edges, &mut added, s, thread, length);
            continue;
        }
        let Some(u) = vec2::unit(sub2(spec.project(thread.end), s)) else {
            continue;
        };
        let frame = Axial {
            s,
            u,
            n: perp(u),
            along: along.abs(),
            per_length: (1.0 - along * along).max(0.0).sqrt(),
            length,
            start: thread.start,
            axis,
            toward_eye: spec.toward_eye,
        };
        across(&mut edges, &mut added, &frame, thread, spec);
    }
    edges.extend(added);
    ViewProjection {
        edges,
        cuts: projection.cuts.clone(),
        polygonal: projection.polygonal,
    }
}

/// A thread's axis in a view that does not look along it: from `s` along
/// `u` (`n` across), `per_length` view millimetres per millimetre of the
/// axis; `along` how far the axis turns towards the eye (0 square to the
/// view, a view across the axis; else a pictorial view), and the axis in
/// the model.
struct Axial {
    s: P2,
    u: P2,
    n: P2,
    along: f64,
    per_length: f64,
    length: f64,
    start: Vec3,
    axis: Vec3,
    toward_eye: Vec3,
}

impl Axial {
    /// A view point in the axis' coordinates: along (view millimetres),
    /// across.
    fn local(&self, p: P2) -> [f64; 2] {
        let d = sub2(p, self.s);
        [dot2(d, self.u), dot2(d, self.n)]
    }

    /// The view point `v` across the axis at `t` along it (model
    /// millimetres from the start).
    fn at(&self, t: f64, v: f64) -> P2 {
        add2(
            add2(self.s, mul2(self.u, self.per_length * t)),
            mul2(self.n, v),
        )
    }

    /// Where along the axis (model millimetres) a view point lies.
    fn t_of(&self, q: [f64; 2]) -> f64 {
        q[0] / self.per_length.max(1e-9)
    }

    fn pictorial(&self) -> bool {
        self.along > 1e-4
    }

    /// Whether a point (in the axis' coordinates) lies on the cylinder of
    /// radius `half` from `-ext` to `length + ext` as the view sees it: the
    /// end circles' ellipses and what lies between them.
    fn inside(&self, q: [f64; 2], half: f64, ext: f64) -> bool {
        if q[1].abs() > half {
            return false;
        }
        let w = self.along * (half * half - q[1] * q[1]).max(0.0).sqrt();
        q[0] >= -ext * self.per_length - w && q[0] <= (self.length + ext) * self.per_length + w
    }

    /// Where along the axis the circle about it lies that the points
    /// (in the axis' coordinates) follow, within `tol`: a face's edge
    /// square to the axis, which a pictorial view shows as an ellipse.
    fn circle_at(&self, points: &[[f64; 2]], tol: f64) -> Option<f64> {
        if points.len() < 5 || !self.pictorial() {
            return None;
        }
        // b² + ((a - c)/al)² = r²: y = b² + a²/al² is linear in a.
        let al2 = self.along * self.along;
        let n = points.len() as f64;
        let (mut sa, mut sy, mut saa, mut say) = (0.0, 0.0, 0.0, 0.0);
        for q in points {
            let y = q[1] * q[1] + q[0] * q[0] / al2;
            sa += q[0];
            sy += y;
            saa += q[0] * q[0];
            say += q[0] * y;
        }
        let den = n * saa - sa * sa;
        if den.abs() < 1e-18 {
            return None;
        }
        let slope = (n * say - sa * sy) / den;
        let center = slope * al2 / 2.0;
        let r2: Vec<f64> = points
            .iter()
            .map(|q| q[1] * q[1] + (q[0] - center) * (q[0] - center) / al2)
            .collect();
        let radius = (r2.iter().sum::<f64>() / n).max(0.0).sqrt();
        let spread = extent(points);
        let fits = r2
            .iter()
            .all(|r| (r.max(0.0).sqrt() - radius).abs() <= tol / self.along.min(1.0));
        (fits && radius > tol && spread > radius).then(|| center / self.per_length.max(1e-9))
    }
}

/// Spans along an axis, joined where they overlap or lie within `gap`.
fn merged(mut spans: Vec<[f64; 2]>, gap: f64) -> Vec<[f64; 2]> {
    spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut out: Vec<[f64; 2]> = Vec::new();
    for s in spans {
        match out.last_mut() {
            Some(last) if s[0] <= last[1] + gap => last[1] = last[1].max(s[1]),
            _ => out.push(s),
        }
    }
    out
}

/// What of `spans` the spans of `minus` leave.
fn without(spans: &[[f64; 2]], minus: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for s in spans {
        let mut pieces = vec![*s];
        for m in minus {
            pieces = pieces
                .into_iter()
                .flat_map(|p| {
                    let mut left = Vec::new();
                    if m[0] > p[0] {
                        left.push([p[0], m[0].min(p[1])]);
                    }
                    if m[1] < p[1] {
                        left.push([m[1].max(p[0]), p[1]]);
                    }
                    left.into_iter().filter(|q| q[1] > q[0])
                })
                .collect();
        }
        out.extend(pieces);
    }
    out
}

/// A thread in a view across its axis or at an angle to it (pictorial).
///
/// Only the threaded body's own lines (by their `source`) are looked at;
/// every other body's stay as the kernel drew them. Of those, what lies on
/// the thread's cylinder is left out: lines wholly on it, curves also up to
/// twice the thread's depth beyond its ends (where its end chamfers cut the
/// flanks). Kept: across the axis, straight lines square to it that end
/// the thread (an end face, a shoulder, a hole's mouth) or that run right
/// across it (a groove's face); in a pictorial view, the circles about the
/// axis (faces' edges square to it). The convention's lines then follow the visibility of what they
/// replace along the axis, which the kernel's hidden-line removal gave:
/// where a nut or a wall hides the thread, its lines are hidden (dashed
/// when the view shows hidden edges, else left out).
///
/// ISO 6410 draws threads only in views square to their axis. In a
/// pictorial view the thread is drawn the same way on its cylinder (a
/// simplified representation as ISO 128 allows for pictorial views): the
/// crests' outlines wide, the roots' narrow, and where no face ends it,
/// the thread's end as the half of its crest circle the eye sees, wide.
fn across(
    edges: &mut Vec<ViewEdge>,
    added: &mut Vec<ViewEdge>,
    f: &Axial,
    thread: &ThreadInput,
    spec: &ViewSpec,
) {
    let length = f.length;
    let depth = (thread.major - thread.minor) / 2.0;
    let tol = 0.1 * depth + 1e-6;
    let reach = 2.0 * depth;
    // How much a radius changes over the thread.
    let change = thread.taper.max(0.0) * length / 2.0;
    let widest = thread.major / 2.0 + if thread.internal { 0.0 } else { change };
    let narrowest = thread.minor / 2.0 - if thread.internal { change } else { 0.0 };
    let half = widest + tol;
    let mut spans: [Vec<[f64; 2]>; 2] = [Vec::new(), Vec::new()];
    let mut ended = [false; 2];
    // Radii of what is left out, by where along the axis: the taper's way.
    let mut radii: Vec<(f64, f64)> = Vec::new();
    let mut kept = Vec::with_capacity(edges.len());
    for edge in edges.drain(..) {
        // Another body's lines stay as the kernel drew them, and say
        // nothing of the thread.
        if !thread.owns(&edge) {
            kept.push(edge);
            continue;
        }
        let seen = usize::from(edge.visible);
        let gone = match &edge.curve {
            ViewCurve::Line(a, b) => {
                let (la, lb) = (f.local(*a), f.local(*b));
                let direction = vec2::unit(sub2(*b, *a));
                let square = !f.pictorial() && direction.is_some_and(|d| dot2(d, f.u).abs() < 0.05);
                let (low, high) = (la[1].min(lb[1]), la[1].max(lb[1]));
                let crosses = low < widest && high > -widest;
                let right_across = low <= -0.9 * narrowest && high >= 0.9 * narrowest;
                let t = f.t_of([(la[0] + lb[0]) / 2.0, 0.0]);
                if square && crosses && t >= -reach && t <= tol {
                    // A face that ends the thread where it starts.
                    ended[0] = true;
                    false
                } else if square && crosses && t >= length - tol && t <= length + reach {
                    ended[1] = true;
                    false
                } else if square && right_across {
                    false
                } else {
                    let inside = f.inside(la, half, tol) && f.inside(lb, half, tol);
                    let lengthwise = direction.is_some_and(|d| dot2(d, f.u).abs() > 0.99);
                    if !inside && lengthwise && la[1].abs() <= half && lb[1].abs() <= half {
                        // A cylinder's outline that runs on beyond the
                        // thread: what of it runs over the thread.
                        if let Some(span) =
                            clipped_span([f.t_of(la), la[1]], [f.t_of(lb), lb[1]], length, half)
                        {
                            spans[seen].push(span);
                        }
                    }
                    inside
                }
            }
            curve => {
                let points: Vec<[f64; 2]> =
                    points_of(curve).into_iter().map(|p| f.local(p)).collect();
                if let Some(t) = f.circle_at(&points, tol) {
                    if t >= -reach && t <= tol {
                        ended[0] = true;
                    } else if t >= length - tol && t <= length + reach {
                        ended[1] = true;
                    }
                    false
                } else {
                    points.iter().all(|q| f.inside(*q, half, reach))
                }
            }
        };
        if gone {
            let ts: Vec<f64> = points_of(&edge.curve)
                .into_iter()
                .map(|p| {
                    let q = f.local(p);
                    radii.push((f.t_of(q), q[1].abs()));
                    f.t_of(q).clamp(0.0, length)
                })
                .collect();
            let lo = ts.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = ts.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if lo <= hi {
                spans[seen].push([lo, hi]);
            }
        } else {
            kept.push(edge);
        }
    }
    *edges = kept;
    let visible = merged(std::mem::take(&mut spans[1]), reach);
    let hidden = without(&merged(std::mem::take(&mut spans[0]), reach), &visible);
    if visible.is_empty() && hidden.is_empty() {
        return;
    }
    // A taper pipe thread starts at its narrow end when external, at its
    // wide end when internal: which end that is, from what was left out
    // (its crests are widest at the wide end), else the one a face ends
    // (the free end), else the axis' start.
    let starts_at_0 = {
        let widest_near = |from: f64, to: f64| {
            radii
                .iter()
                .filter(|(t, _)| *t >= from && *t <= to)
                .map(|(_, r)| *r)
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let (r0, r1) = (
            widest_near(-reach, 0.25 * length),
            widest_near(0.75 * length, length + reach),
        );
        if change > 0.0 && r0.is_finite() && r1.is_finite() && (r0 - r1).abs() > 0.6 * change {
            (r0 > r1) == thread.internal
        } else if ended[0] != ended[1] {
            ended[0]
        } else {
            true
        }
    };
    let sign = if thread.internal { -1.0 } else { 1.0 };
    let grown = |t: f64| {
        let from_start = if starts_at_0 { t } else { length - t };
        sign * thread.taper.max(0.0) / 2.0 * from_start
    };
    let (crest0, root0) = if thread.internal {
        (thread.minor / 2.0, thread.major / 2.0)
    } else {
        (thread.major / 2.0, thread.minor / 2.0)
    };
    let line = |visible: bool, kind: ViewEdgeKind, a: P2, b: P2| ViewEdge {
        visible,
        kind,
        curve: ViewCurve::Line(a, b),
        source: None,
    };
    for (seen, list) in [(true, &visible), (false, &hidden)] {
        for span in list {
            if span[1] - span[0] <= tol {
                continue;
            }
            let (t0, t1) = (span[0], span[1]);
            for side in [-1.0, 1.0] {
                added.push(line(
                    seen,
                    ViewEdgeKind::Sharp,
                    f.at(t0, side * (crest0 + grown(t0))),
                    f.at(t1, side * (crest0 + grown(t1))),
                ));
                added.push(line(
                    seen,
                    ViewEdgeKind::ThreadRoot,
                    f.at(t0, side * (root0 + grown(t0))),
                    f.at(t1, side * (root0 + grown(t1))),
                ));
            }
        }
    }
    // The thread's end, where no face ends it.
    for (k, t) in [(0usize, 0.0), (1, length)] {
        if ended[k] {
            continue;
        }
        let within = |list: &[[f64; 2]]| {
            list.iter()
                .any(|sp| t >= sp[0] - reach && t <= sp[1] + reach && sp[1] - sp[0] > tol)
        };
        let seen = if within(&visible[..]) {
            true
        } else if within(&hidden[..]) {
            false
        } else {
            continue;
        };
        let outer = thread.major / 2.0 + grown(t);
        if !f.pictorial() {
            // Across the major diameter.
            added.push(line(
                seen,
                ViewEdgeKind::Sharp,
                f.at(t, -outer),
                f.at(t, outer),
            ));
            continue;
        }
        // The half of the crest circle the eye sees: towards it on a
        // shaft, the far half inside a hole.
        let crest = crest0 + grown(t);
        let e1 = unit(cross(f.axis, f.toward_eye)).unwrap_or([0.0, 0.0, 0.0]);
        let e2 = cross(f.axis, e1);
        let facing = dot(e2, f.toward_eye) >= 0.0;
        let from = if facing != thread.internal { 0.0 } else { PI };
        let center = add(f.start, scale(f.axis, t));
        let points: Vec<P2> = (0..=24)
            .map(|i| {
                let phi = from + PI * f64::from(i) / 24.0;
                let radial = add(scale(e1, phi.cos()), scale(e2, phi.sin()));
                spec.project(add(center, scale(radial, crest)))
            })
            .collect();
        added.push(ViewEdge {
            visible: seen,
            kind: ViewEdgeKind::Sharp,
            curve: ViewCurve::Polyline(points),
            source: None,
        });
    }
}

/// The span along the axis of what of a line (in the thread's own
/// coordinates: along, across) lies over the thread: from 0 to `length`
/// along, within `half` across.
fn clipped_span(a: [f64; 2], b: [f64; 2], length: f64, half: f64) -> Option<[f64; 2]> {
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for (k, min, max) in [(0usize, 0.0, length), (1, -half, half)] {
        let d = b[k] - a[k];
        if d.abs() < 1e-15 {
            if a[k] < min || a[k] > max {
                return None;
            }
            continue;
        }
        let (r0, r1) = ((min - a[k]) / d, (max - a[k]) / d);
        lo = lo.max(r0.min(r1));
        hi = hi.min(r0.max(r1));
    }
    if hi <= lo {
        return None;
    }
    let t0 = a[0] + (b[0] - a[0]) * lo;
    let t1 = a[0] + (b[0] - a[0]) * hi;
    Some([t0.min(t1), t0.max(t1)])
}

/// A thread in a view along its axis, about `center`: the diameters where
/// it starts (a taper pipe thread's change over its `length` widens what
/// is left out).
fn end_on(
    edges: &mut Vec<ViewEdge>,
    added: &mut Vec<ViewEdge>,
    center: P2,
    thread: &ThreadInput,
    length: f64,
) {
    let outer = thread.major / 2.0;
    let inner = thread.minor / 2.0;
    let depth = outer - inner;
    let tol = 0.1 * depth + 1e-6;
    let change = thread.taper.max(0.0) * length / 2.0;
    // Down to a thread's depth inside the minor diameter: an end
    // chamfer's circle, which the convention leaves out.
    let (from, to) = ((inner - depth - change).max(0.0), outer + change + tol);
    let radius = |p: &P2| vec2::distance(*p, center);
    let (crest, root) = if thread.internal {
        (inner, outer)
    } else {
        (outer, inner)
    };
    // The visibility of what is left out, near a radius and all told.
    let mut near: [Option<bool>; 2] = [None, None];
    let mut seen: Option<bool> = None;
    let mut kept = Vec::with_capacity(edges.len());
    for edge in edges.drain(..) {
        // Another body's lines stay as the kernel drew them (a nut's
        // bore), and say nothing of the thread.
        if !thread.owns(&edge) {
            kept.push(edge);
            continue;
        }
        let points = dense_points(&edge.curve, depth.max(1e-6), 64);
        let inside = points.iter().all(|p| {
            let r = radius(p);
            r >= from && r <= to
        });
        if !inside {
            kept.push(edge);
            continue;
        }
        seen = Some(seen.unwrap_or(false) || edge.visible);
        for (k, at) in [crest, root].into_iter().enumerate() {
            if points.iter().any(|p| (radius(p) - at).abs() <= depth / 2.0) {
                near[k] = Some(near[k].unwrap_or(false) || edge.visible);
            }
        }
    }
    *edges = kept;
    let Some(seen) = seen else { return };
    // Each circle as what it stands for is seen: behind a nut's face the
    // crests are hidden while its bore shows the bolt's end.
    let crest_seen = near[0].unwrap_or(seen);
    let root_seen = near[1].unwrap_or(crest_seen);
    added.push(ViewEdge {
        visible: crest_seen,
        kind: ViewEdgeKind::Sharp,
        curve: ViewCurve::Arc {
            center,
            radius: crest,
            start: 0.0,
            end: TAU,
        },
        source: None,
    });
    // About three quarters of a circle, open at the upper right and clear
    // of the centre lines.
    added.push(ViewEdge {
        visible: root_seen,
        kind: ViewEdgeKind::ThreadRoot,
        curve: ViewCurve::Arc {
            center,
            radius: root,
            start: (95.0f64).to_radians(),
            end: (355.0f64).to_radians(),
        },
        source: None,
    });
}

#[cfg(test)]
#[path = "convention_tests.rs"]
mod tests;
