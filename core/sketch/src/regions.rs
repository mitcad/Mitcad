// SPDX-License-Identifier: MIT
//! Profile regions: the bounded faces of the planar arrangement of the
//! sketch's profile curves (the profiles).
//!
//! 1. Every pair of curves is intersected (`intersect.rs`); curves are cut
//!    at the intersections and their own ends. Points closer than the
//!    tolerance are one vertex; overlapping pieces are kept once, as are
//!    closed curves on each other that nothing cuts (a circle drawn twice).
//! 2. Pieces with a free end (dangling curves) and bridges (pieces with the
//!    same face on both sides) bound nothing and are dropped, repeatedly.
//!    Pieces of one curve that meet only each other join again.
//! 3. Half-edges are sorted around each vertex by direction, ties (touching
//!    curves) by curvature, and faces are traced keeping the face on the
//!    left. A counter-clockwise cycle bounds a face; a clockwise one is the
//!    outside of a connected part, which becomes a hole of the smallest face
//!    of another part around it. A cycle through a vertex twice is split
//!    there into loops.
//! 4. A segment is named by its curve and the other curves at its ends, in
//!    the curve's direction (`c3[c2,c4]`, `c3[c2+c5,c4]`, `-` for none),
//!    with `#k` along the curve for repeats; a curve that is a loop by
//!    itself is `c5`. A region is named by its outer loop's segments.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::f64::consts::TAU;

use super::geometry::{
    Curve2, P2, add, cross, dist, dot, norm, polygon_area, polygon_centroid, polygon_contains,
    scale, sub,
};
use super::intersect::intersections;
use crate::profile::{ProfileLoop, ProfileRegion, ProfileSegment, SegmentGeometry};
use crate::topo::{CurveId, RegionKey, SegmentKey};

/// A curve that bounds profiles.
#[derive(Debug, Clone)]
pub struct RegionCurve {
    pub uid: CurveId,
    pub curve: Curve2,
    /// The pieces of a composite curve (a text glyph's contour), each on
    /// [0, 1], piece k covering the parameters [k, k + 1] of `curve`: a
    /// profile segment on it is made of the pieces, so that its faces
    /// follow them. Empty for an ordinary curve.
    pub parts: Vec<Curve2>,
}

impl RegionCurve {
    pub fn new(uid: impl Into<CurveId>, curve: Curve2) -> Self {
        Self {
            uid: uid.into(),
            curve,
            parts: Vec::new(),
        }
    }

    /// The profile geometry of the piece `[t0, t1]` (for a closed curve
    /// `t1` may pass the end of the domain), in the curve's direction.
    pub fn geometry(&self, t0: f64, t1: f64) -> Vec<SegmentGeometry> {
        if self.parts.is_empty() {
            return vec![self.curve.piece(t0, t1)];
        }
        let m = self.parts.len();
        let eps = 1e-9;
        let mut out = Vec::new();
        let mut t = t0;
        while t < t1 - eps {
            let mut k = t.floor();
            if t - k > 1.0 - eps {
                k += 1.0;
            }
            let end = (k + 1.0).min(t1);
            let part = &self.parts[(k as usize) % m];
            let (a, b) = ((t - k).max(0.0), (end - k).min(1.0));
            if b - a > eps {
                out.push(if a <= eps && b >= 1.0 - eps {
                    part.whole()
                } else {
                    part.piece(a, b)
                });
            }
            t = end;
        }
        out
    }

    /// The whole curve as profile geometry.
    pub fn whole(&self) -> Vec<SegmentGeometry> {
        if self.parts.is_empty() {
            vec![self.curve.whole()]
        } else {
            self.parts.iter().map(Curve2::whole).collect()
        }
    }
}

/// A region with its measures.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub profile: ProfileRegion,
    /// Area in mm² (outer loop less holes, from sampled boundaries).
    pub area: f64,
    pub centroid: P2,
    /// The outer loop sampled counter-clockwise.
    pub outline: Vec<P2>,
    /// The inner loops (holes) sampled.
    pub holes: Vec<Vec<P2>>,
}

#[derive(Debug, Clone, Copy)]
struct Split {
    t: f64,
    point: P2,
    /// The curve's own end point (exact), preferred as the vertex position.
    end: bool,
}

#[derive(Debug, Clone)]
struct Edge {
    curve: usize,
    t0: f64,
    /// After `t0`; for closed curves it may pass the end of the domain.
    t1: f64,
    /// None for a closed curve that is a loop by itself.
    v0: Option<usize>,
    v1: Option<usize>,
    /// A stored sketch curve bounds this piece, including one coincident
    /// with an implicit face edge. Both roles survive overlap deduplication.
    sketch_boundary: bool,
    face_boundary: bool,
    alive: bool,
}

struct Arrangement<'a> {
    curves: &'a [RegionCurve],
    /// The bounds of each whole curve ([`Curve2::bounds`]), computed once.
    bounds: Vec<(P2, P2)>,
    tol: f64,
    vertices: Vec<P2>,
    edges: Vec<Edge>,
}

/// Computes the regions of the curves (in any order; the result is sorted
/// by key).
pub fn regions(curves: &[RegionCurve]) -> Vec<Region> {
    regions_and_layout(curves).0
}

/// [`regions`] with how they lie against each other.
pub fn regions_and_layout(curves: &[RegionCurve]) -> (Vec<Region>, Layout) {
    if curves.is_empty() {
        return (Vec::new(), Layout::default());
    }
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    let bounds: Vec<(P2, P2)> = curves.iter().map(|c| c.curve.bounds()).collect();
    for (lo, hi) in &bounds {
        for k in 0..2 {
            min[k] = min[k].min(lo[k]);
            max[k] = max[k].max(hi[k]);
        }
    }
    let size = dist(min, max).max(1.0);
    // (Not below 1e-6 mm: the solved positions of imported designs differ
    // by that much where lines were meant to coincide.)
    let tol = (1e-9 * size).max(1e-6);
    let mut arrangement = Arrangement::new(curves, bounds, tol);
    arrangement.clean();
    arrangement.regions()
}

/// Regions bounded by the sketch and its supporting face. Face edges
/// split the arrangement without becoming sketch entities. Keep material
/// inside the face (including its islands, excluding its holes), together
/// with the sketch's own bounded regions outside it.
pub fn face_bounded_regions(curves: &[RegionCurve]) -> (Vec<Region>, Layout) {
    let (regions, mut layout) = regions_and_layout(curves);
    let outside = regions.len();
    let mut adjacent = vec![Vec::new(); outside + 1];
    for edge in &layout.edges {
        let a = edge.sides[0].map_or(outside, |s| s.region);
        let b = edge.sides[1].map_or(outside, |s| s.region);
        if a != b {
            let boundary = edge.face_boundary;
            adjacent[a].push((b, boundary, edge.sketch_boundary));
            adjacent[b].push((a, boundary, edge.sketch_boundary));
        }
    }
    // Crossing a face edge changes material parity. Crossing a sketch
    // curve just moves to another piece of the same face material.
    let mut material = vec![None; outside + 1];
    material[outside] = Some(false);
    let mut todo = vec![outside];
    while let Some(at) = todo.pop() {
        for &(next, boundary, _) in &adjacent[at] {
            if material[next].is_none() {
                material[next] = Some(material[at].unwrap_or(false) ^ boundary);
                todo.push(next);
            }
        }
    }
    // With the implicit face edges removed, regions reachable from the
    // outside are not bounded by the sketch's own curves. This also
    // rejects off-face pieces made by an otherwise dangling open line.
    let mut unbounded = vec![false; outside + 1];
    unbounded[outside] = true;
    todo.push(outside);
    while let Some(at) = todo.pop() {
        for &(next, _, sketch_boundary) in &adjacent[at] {
            if !sketch_boundary && !unbounded[next] {
                unbounded[next] = true;
                todo.push(next);
            }
        }
    }
    let mut indices = vec![None; regions.len()];
    let mut kept = Vec::new();
    for (i, mut region) in regions.into_iter().enumerate() {
        if material[i].unwrap_or(false) || !unbounded[i] {
            // A face edge in a hole still belongs to this region's
            // identity. Losing that face must not turn an outside ring
            // into the sketch's whole original region under the same key.
            if region.profile.face_bounded() {
                region.profile.key = RegionKey::new(
                    region.profile.key.segments().cloned().chain(
                        region
                            .profile
                            .loops
                            .iter()
                            .flat_map(|l| &l.segments)
                            .filter(|s| s.key.curve.is_face_edge())
                            .map(|s| s.key.clone()),
                    ),
                )
                .expect("a region has a boundary");
            }
            kept.push((i, region));
        }
    }
    kept.sort_by(|a, b| a.1.profile.key.cmp(&b.1.profile.key));
    for (index, (before, _)) in kept.iter().enumerate() {
        indices[*before] = Some(index);
    }
    for edge in &mut layout.edges {
        for side in &mut edge.sides {
            *side = side.and_then(|s| {
                Some(LayoutSide {
                    region: indices[s.region]?,
                    outer: s.outer,
                })
            });
        }
    }
    (kept.into_iter().map(|(_, region)| region).collect(), layout)
}

/// Whether `t` is an end of an open curve.
fn is_end(curve: &Curve2, t: f64) -> bool {
    if curve.is_closed() {
        return false;
    }
    let (lo, hi) = curve.domain();
    let eps = 1e-12 * (hi - lo).abs().max(1.0);
    (t - lo).abs() <= eps || (t - hi).abs() <= eps
}

impl<'a> Arrangement<'a> {
    fn new(curves: &'a [RegionCurve], bounds: Vec<(P2, P2)>, tol: f64) -> Self {
        let n = curves.len();
        let mut splits: Vec<Vec<Split>> = vec![Vec::new(); n];
        for (i, c) in curves.iter().enumerate() {
            if let Some((start, end)) = c.curve.ends() {
                let (lo, hi) = c.curve.domain();
                splits[i].push(Split {
                    t: lo,
                    point: start,
                    end: true,
                });
                splits[i].push(Split {
                    t: hi,
                    point: end,
                    end: true,
                });
            }
        }
        let reach = 10.0 * tol;
        for i in 0..n {
            for j in i + 1..n {
                let (a, b) = (bounds[i], bounds[j]);
                // Spline bounds are their control polygons, which contain them.
                if a.0[0] > b.1[0] + reach
                    || b.0[0] > a.1[0] + reach
                    || a.0[1] > b.1[1] + reach
                    || b.0[1] > a.1[1] + reach
                {
                    continue;
                }
                let (ci, cj) = (&curves[i].curve, &curves[j].curve);
                for hit in intersections(ci, cj, tol) {
                    let (end_i, end_j) = (is_end(ci, hit.ta), is_end(cj, hit.tb));
                    // The exact end point of whichever curve ends there.
                    let point = if end_i {
                        ci.point(hit.ta)
                    } else if end_j {
                        cj.point(hit.tb)
                    } else {
                        hit.point
                    };
                    splits[i].push(Split {
                        t: ci.wrap(hit.ta),
                        point,
                        end: end_i || end_j,
                    });
                    splits[j].push(Split {
                        t: cj.wrap(hit.tb),
                        point,
                        end: end_i || end_j,
                    });
                }
            }
        }

        // Vertices: split points closer than the tolerance are one.
        let mut all: Vec<(usize, usize)> = Vec::new();
        for (i, list) in splits.iter().enumerate() {
            for k in 0..list.len() {
                all.push((i, k));
            }
        }
        let point = |&(i, k): &(usize, usize)| splits[i][k].point;
        let mut order: Vec<usize> = (0..all.len()).collect();
        order.sort_by(|a, b| point(&all[*a])[0].total_cmp(&point(&all[*b])[0]));
        let mut parent: Vec<usize> = (0..all.len()).collect();
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for (oi, &a) in order.iter().enumerate() {
            let pa = point(&all[a]);
            for &b in &order[oi + 1..] {
                let pb = point(&all[b]);
                if pb[0] - pa[0] > reach {
                    break;
                }
                if dist(pa, pb) <= reach {
                    let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                    if ra != rb {
                        parent[ra.max(rb)] = ra.min(rb);
                    }
                }
            }
        }
        let mut vertex_of_root: BTreeMap<usize, usize> = BTreeMap::new();
        let mut vertices: Vec<P2> = Vec::new();
        let mut is_exact: Vec<bool> = Vec::new();
        let mut vertex_of = vec![0; all.len()];
        for a in 0..all.len() {
            let root = find(&mut parent, a);
            let (i, k) = all[a];
            let s = splits[i][k];
            let v = *vertex_of_root.entry(root).or_insert_with(|| {
                vertices.push(s.point);
                is_exact.push(s.end);
                vertices.len() - 1
            });
            if s.end && !is_exact[v] {
                vertices[v] = s.point;
                is_exact[v] = true;
            }
            vertex_of[a] = v;
        }

        // Edges: the pieces between consecutive cuts of each curve.
        let mut edges = Vec::new();
        let mut index = 0;
        for (i, list) in splits.iter().enumerate() {
            let curve = &curves[i].curve;
            let (lo, hi) = curve.domain();
            let mut cuts: Vec<(f64, usize)> = list
                .iter()
                .enumerate()
                .map(|(k, s)| (s.t, vertex_of[index + k]))
                .collect();
            index += list.len();
            cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
            // One cut per vertex in a row: the same point found twice.
            cuts.dedup_by(|b, a| a.1 == b.1 && (b.0 - a.0).abs() <= 1e-9 * (hi - lo).max(1.0));
            if curve.is_closed() {
                if cuts.len() > 1 && cuts[0].1 == cuts[cuts.len() - 1].1 {
                    let first = cuts[0].0 + (hi - lo);
                    let last = cuts[cuts.len() - 1].0;
                    if (first - last).abs() <= 1e-9 * (hi - lo).max(1.0) {
                        cuts.pop();
                    }
                }
                match cuts.len() {
                    0 => edges.push(Edge {
                        curve: i,
                        t0: lo,
                        t1: hi,
                        v0: None,
                        v1: None,
                        sketch_boundary: !curves[i].uid.is_face_edge(),
                        face_boundary: curves[i].uid.is_face_edge(),
                        alive: true,
                    }),
                    m => {
                        for k in 0..m {
                            let (t0, v0) = cuts[k];
                            let (t1, v1) = if k + 1 < m {
                                cuts[k + 1]
                            } else {
                                (cuts[0].0 + (hi - lo), cuts[0].1)
                            };
                            edges.push(Edge {
                                curve: i,
                                t0,
                                t1,
                                v0: Some(v0),
                                v1: Some(v1),
                                sketch_boundary: !curves[i].uid.is_face_edge(),
                                face_boundary: curves[i].uid.is_face_edge(),
                                alive: true,
                            });
                        }
                    }
                }
            } else {
                for w in cuts.windows(2) {
                    edges.push(Edge {
                        curve: i,
                        t0: w[0].0,
                        t1: w[1].0,
                        v0: Some(w[0].1),
                        v1: Some(w[1].1),
                        sketch_boundary: !curves[i].uid.is_face_edge(),
                        face_boundary: curves[i].uid.is_face_edge(),
                        alive: true,
                    });
                }
            }
        }
        let mut arrangement = Self {
            curves,
            bounds,
            tol,
            vertices,
            edges,
        };
        arrangement.drop_degenerate();
        arrangement
    }

    fn curve(&self, e: usize) -> &Curve2 {
        &self.curves[self.edges[e].curve].curve
    }

    /// A point and derivatives of an edge's curve, with the parameter of a
    /// closed spline wrapped.
    fn eval(&self, e: usize, t: f64) -> [P2; 3] {
        let curve = self.curve(e);
        let (lo, hi) = curve.domain();
        let t = if matches!(curve, Curve2::Nurbs(_)) && t > hi {
            lo + (t - hi)
        } else {
            t
        };
        curve.eval(t)
    }

    fn samples(&self, e: usize) -> Vec<P2> {
        let edge = &self.edges[e];
        self.curve(e)
            .sample_params(edge.t0, edge.t1)
            .into_iter()
            .map(|t| self.eval(e, t)[0])
            .collect()
    }

    fn length(&self, e: usize) -> f64 {
        self.samples(e).windows(2).map(|w| dist(w[0], w[1])).sum()
    }

    /// Drops pieces of no length and pieces that repeat another.
    fn drop_degenerate(&mut self) {
        for e in 0..self.edges.len() {
            if self.edges[e].v0.is_some()
                && self.edges[e].v0 == self.edges[e].v1
                && self.length(e) <= 100.0 * self.tol
            {
                self.edges[e].alive = false;
            }
        }
        // Overlapping curves: the same ends and the same middle.
        let mids: Vec<Option<P2>> = (0..self.edges.len())
            .map(|e| {
                let edge = &self.edges[e];
                (edge.alive && edge.v0.is_some())
                    .then(|| self.eval(e, (edge.t0 + edge.t1) / 2.0)[0])
            })
            .collect();
        let ends = |x: &Edge| {
            let (a, b) = (x.v0.unwrap_or(usize::MAX), x.v1.unwrap_or(usize::MAX));
            (a.min(b), a.max(b))
        };
        // The pieces with a middle by their ends, in order: only pieces with
        // the same ends can repeat each other (mitcad#889: every pair of 5 000
        // pieces was compared).
        let mut by_ends: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (e, mid) in mids.iter().enumerate() {
            if mid.is_some() {
                by_ends.entry(ends(&self.edges[e])).or_default().push(e);
            }
        }
        for e in 0..self.edges.len() {
            let Some(me) = mids[e] else { continue };
            if !self.edges[e].alive {
                continue;
            }
            let same_ends = &by_ends[&ends(&self.edges[e])];
            let later = same_ends.partition_point(|&f| f <= e);
            for &f in &same_ends[later..] {
                let Some(mf) = mids[f] else { continue };
                if self.edges[f].alive && (dist(me, mf) <= 100.0 * self.tol || self.run_along(e, f))
                {
                    // A coincident stored curve keeps its identity. Of
                    // other overlapping curves prefer the spline (a
                    // projected edge) and then the lower curve id.
                    let (ue, uf) = (
                        &self.curves[self.edges[e].curve].uid,
                        &self.curves[self.edges[f].curve].uid,
                    );
                    let spline = |x: usize| matches!(self.curve(x), Curve2::Nurbs(_));
                    let drop_e = if ue.is_face_edge() != uf.is_face_edge() {
                        ue.is_face_edge()
                    } else if spline(e) != spline(f) {
                        spline(f)
                    } else {
                        uf < ue
                    };
                    let sketch_boundary =
                        self.edges[e].sketch_boundary || self.edges[f].sketch_boundary;
                    let face_boundary = self.edges[e].face_boundary || self.edges[f].face_boundary;
                    self.edges[e].sketch_boundary = sketch_boundary;
                    self.edges[f].sketch_boundary = sketch_boundary;
                    self.edges[e].face_boundary = face_boundary;
                    self.edges[f].face_boundary = face_boundary;
                    if drop_e {
                        self.edges[e].alive = false;
                        break;
                    }
                    self.edges[f].alive = false;
                }
            }
        }
        // Closed curves that are loops by themselves and lie on each other
        // (a circle drawn twice; imported sketches have them): nothing cuts
        // them, so the check above does not see them, and each would
        // bound a face of its own and be a hole of the face around them,
        // twice in one place (mitcad#93).
        let whole: Vec<usize> = (0..self.edges.len())
            .filter(|&e| self.edges[e].alive && self.edges[e].v0.is_none())
            .collect();
        for (k, &e) in whole.iter().enumerate() {
            for &f in &whole[k + 1..] {
                if !self.edges[e].alive {
                    break;
                }
                if !self.edges[f].alive || !self.on_each_other(e, f) {
                    continue;
                }
                let (ue, uf) = (
                    &self.curves[self.edges[e].curve].uid,
                    &self.curves[self.edges[f].curve].uid,
                );
                let drop_e = (ue.is_face_edge() && !uf.is_face_edge())
                    || (uf.is_face_edge() == ue.is_face_edge() && uf < ue);
                let sketch_boundary =
                    self.edges[e].sketch_boundary || self.edges[f].sketch_boundary;
                let face_boundary = self.edges[e].face_boundary || self.edges[f].face_boundary;
                self.edges[e].sketch_boundary = sketch_boundary;
                self.edges[f].sketch_boundary = sketch_boundary;
                self.edges[e].face_boundary = face_boundary;
                self.edges[f].face_boundary = face_boundary;
                if drop_e {
                    self.edges[e].alive = false;
                } else {
                    self.edges[f].alive = false;
                }
            }
        }
        self.drop_face_pieces_along_sketch();
    }

    /// Pieces of a supporting face's edges that together run along a
    /// sketch piece from one of its ends to the other, within a micron
    /// (mitcad#1159): the sketch's projection of those very edges, whose
    /// spline deviates that much from them. A corner where two such edges
    /// meet lies off the sketch curve, so nothing cuts the curve there and
    /// the pieces are not repeats of it; kept, they bound a sliver between
    /// the two curves (of no area, or sampled inside out) that makes no
    /// profile. The sketch piece stands for them as the face's boundary.
    fn drop_face_pieces_along_sketch(&mut self) {
        let face = |a: &Self, e: usize| a.curves[a.edges[e].curve].uid.is_face_edge();
        let pieces: Vec<usize> = (0..self.edges.len())
            .filter(|&e| self.edges[e].alive && self.edges[e].v0.is_some())
            .collect();
        if !pieces.iter().any(|&e| face(self, e)) {
            return;
        }
        let reach = 1000.0 * self.tol;
        let near = |a: (P2, P2), b: (P2, P2)| {
            (0..2).all(|k| a.0[k] <= b.1[k] + reach && b.0[k] <= a.1[k] + reach)
        };
        for &f in &pieces {
            if face(self, f) || !self.edges[f].alive {
                continue;
            }
            let (Some(start), Some(end)) = (self.edges[f].v0, self.edges[f].v1) else {
                continue;
            };
            let along: Vec<usize> = pieces
                .iter()
                .copied()
                .filter(|&e| {
                    self.edges[e].alive
                        && face(self, e)
                        && near(
                            self.bounds[self.edges[e].curve],
                            self.bounds[self.edges[f].curve],
                        )
                        && self.lies_along(e, f)
                })
                .collect();
            // A chain of them from the sketch piece's start to its end, each
            // end of the chain at the piece's or within a micron of it (where
            // the face's edge crosses another sketch curve beside the
            // corner).
            let close = |a: usize, b: usize| dist(self.vertices[a], self.vertices[b]) <= reach;
            let other = |e: usize, v: usize| {
                let edge = &self.edges[e];
                if edge.v0 == Some(v) { edge.v1 } else { edge.v0 }.expect("a piece")
            };
            let mut chain = Vec::new();
            let mut at = start;
            if let Some(&first) = along.iter().find(|&&e| {
                [self.edges[e].v0, self.edges[e].v1]
                    .into_iter()
                    .flatten()
                    .any(|v| close(v, start))
            }) {
                let v0 = self.edges[first].v0.expect("a piece");
                let v = if close(v0, start) {
                    v0
                } else {
                    self.edges[first].v1.expect("a piece")
                };
                chain.push(first);
                at = other(first, v);
                while !close(at, end) {
                    let Some(&next) = along.iter().find(|&&e| {
                        !chain.contains(&e)
                            && (self.edges[e].v0 == Some(at) || self.edges[e].v1 == Some(at))
                    }) else {
                        break;
                    };
                    chain.push(next);
                    at = other(next, at);
                }
            }
            if chain.is_empty() || !close(at, end) {
                continue;
            }
            for e in chain {
                self.edges[e].alive = false;
            }
            self.edges[f].face_boundary = true;
        }
    }

    /// Whether two pieces with the same ends run along each other: points
    /// along the one lie within a micron (1000 tolerances: a spline that
    /// stands for an edge deviates that much from it) of the other, on its own
    /// stretch (a spline that follows a circle has other parameters at the
    /// same place, so their middles differ), and the other way round.
    fn run_along(&self, e: usize, f: usize) -> bool {
        self.lies_along(e, f) && self.lies_along(f, e)
    }

    /// Whether the points along the piece `e` lie on the stretch of the piece
    /// `f` of its curve.
    fn lies_along(&self, e: usize, f: usize) -> bool {
        let reach = 1000.0 * self.tol;
        let target = self.curve(f);
        let (lo, hi) = target.domain();
        let span = hi - lo;
        let (f0, f1) = (self.edges[f].t0, self.edges[f].t1);
        let extent = f1 - f0;
        let slack = 0.05 * extent;
        let (e0, e1) = (self.edges[e].t0, self.edges[e].t1);
        (1..8).all(|k| {
            let p = self.eval(e, e0 + (e1 - e0) * f64::from(k) / 8.0)[0];
            let (s, d) = target.closest(p);
            let mut off = s - f0;
            if target.is_closed() {
                off = off.rem_euclid(span);
                if off > span - slack {
                    off -= span;
                }
            }
            d <= reach && off >= -slack && off <= extent + slack
        })
    }

    /// Whether the whole curves of two edges lie on each other within the
    /// tolerance of overlapping pieces.
    fn on_each_other(&self, e: usize, f: usize) -> bool {
        let reach = 100.0 * self.tol;
        let (a, b) = (
            self.bounds[self.edges[e].curve],
            self.bounds[self.edges[f].curve],
        );
        if (0..2).any(|k| (a.0[k] - b.0[k]).abs() > reach || (a.1[k] - b.1[k]).abs() > reach) {
            return false;
        }
        let on = |from: usize, to: usize| {
            let target = self.curve(to);
            self.samples(from)
                .into_iter()
                .all(|p| target.closest(p).1 <= reach)
        };
        on(e, f) && on(f, e)
    }

    fn degrees(&self) -> Vec<usize> {
        let mut degree = vec![0; self.vertices.len()];
        for edge in self.edges.iter().filter(|e| e.alive) {
            for v in [edge.v0, edge.v1].into_iter().flatten() {
                degree[v] += 1;
            }
        }
        degree
    }

    /// Removes pieces with a free end, repeatedly.
    fn drop_dangling(&mut self) {
        loop {
            let degree = self.degrees();
            let mut changed = false;
            for edge in self.edges.iter_mut().filter(|e| e.alive) {
                if [edge.v0, edge.v1]
                    .into_iter()
                    .flatten()
                    .any(|v| degree[v] < 2)
                {
                    edge.alive = false;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Joins the pieces of a curve that meet only each other at a vertex.
    fn join_pieces(&mut self) {
        // The edges that end at each vertex or did (in order): the live ones
        // that still end there are picked from these instead of from every
        // edge (mitcad#889: a sketch of 5 000 pieces looked through all of
        // them at each vertex).
        let mut touching: Vec<Vec<usize>> = vec![Vec::new(); self.vertices.len()];
        for (e, edge) in self.edges.iter().enumerate() {
            for v in [edge.v0, edge.v1].into_iter().flatten() {
                if touching[v].last() != Some(&e) {
                    touching[v].push(e);
                }
            }
        }
        loop {
            let degree = self.degrees();
            let mut joined = false;
            for (v, d) in degree.iter().enumerate() {
                if *d != 2 {
                    continue;
                }
                let at: Vec<usize> = touching[v]
                    .iter()
                    .copied()
                    .filter(|&e| {
                        let edge = &self.edges[e];
                        edge.alive && (edge.v0 == Some(v) || edge.v1 == Some(v))
                    })
                    .collect();
                match at.as_slice() {
                    [e] => {
                        // A closed curve's single piece around to itself.
                        let edge = &mut self.edges[*e];
                        if edge.v0 == Some(v) && edge.v1 == Some(v) {
                            edge.v0 = None;
                            edge.v1 = None;
                            joined = true;
                        }
                    }
                    [a, b] => {
                        let (a, b) = (*a, *b);
                        if self.edges[a].curve != self.edges[b].curve
                            || self.edges[a].sketch_boundary != self.edges[b].sketch_boundary
                            || self.edges[a].face_boundary != self.edges[b].face_boundary
                        {
                            continue;
                        }
                        let (first, second) = if self.edges[a].v1 == Some(v)
                            && self.edges[b].v0 == Some(v)
                            && self.continues(a, b)
                        {
                            (a, b)
                        } else if self.edges[b].v1 == Some(v)
                            && self.edges[a].v0 == Some(v)
                            && self.continues(b, a)
                        {
                            (b, a)
                        } else {
                            continue;
                        };
                        let period = {
                            let (lo, hi) = self.curve(first).domain();
                            hi - lo
                        };
                        let mut t1 = self.edges[second].t1;
                        while t1 <= self.edges[first].t1 - 1e-12 {
                            t1 += period;
                        }
                        let v1 = self.edges[second].v1;
                        self.edges[first].t1 = t1;
                        self.edges[first].v1 = v1;
                        self.edges[second].alive = false;
                        if let Some(end) = v1
                            && let Err(k) = touching[end].binary_search(&first)
                        {
                            touching[end].insert(k, first);
                        }
                        joined = true;
                    }
                    _ => {}
                }
                if joined {
                    break;
                }
            }
            if !joined {
                break;
            }
        }
    }

    /// Whether piece `b` starts where piece `a` ends on their curve.
    fn continues(&self, a: usize, b: usize) -> bool {
        let curve = self.curve(a);
        let (lo, hi) = curve.domain();
        let gap = self.edges[a].t1 - self.edges[b].t0;
        let eps = 1e-9 * (hi - lo).max(1.0);
        if curve.is_closed() {
            let period = hi - lo;
            (gap - (gap / period).round() * period).abs() <= eps
        } else {
            gap.abs() <= eps
        }
    }

    /// Half-edge `h`: edge `h / 2`, forward when even.
    fn origin(&self, h: usize) -> usize {
        let edge = &self.edges[h / 2];
        if h.is_multiple_of(2) {
            edge.v0
        } else {
            edge.v1
        }
        .expect("a vertex")
    }

    /// Direction and signed curvature leaving the origin.
    fn leaving(&self, h: usize) -> (P2, f64) {
        let edge = &self.edges[h / 2];
        let (t, sign) = if h.is_multiple_of(2) {
            (edge.t0, 1.0)
        } else {
            (edge.t1, -1.0)
        };
        // At a knot of a spline the derivatives are those of the edge's own
        // side: a composite curve's pieces meet at corners there (a QR
        // code's contours touching at a corner, mitcad#473).
        let t = match self.curve(h / 2) {
            Curve2::Nurbs(n) => {
                let (lo, hi) = n.domain();
                let wrapped = if t > hi { lo + (t - hi) } else { t };
                let at_knot = n
                    .knots
                    .iter()
                    .any(|k| (k - wrapped).abs() <= 1e-12 * (hi - lo).max(1.0));
                if at_knot {
                    t + sign * 1e-9 * (hi - lo)
                } else {
                    t
                }
            }
            _ => t,
        };
        let [_, d1, d2] = self.eval(h / 2, t);
        let d1 = scale(d1, sign);
        let speed = norm(d1);
        let curvature = if speed > 0.0 {
            cross(d1, d2) / (speed * speed * speed)
        } else {
            0.0
        };
        (d1, curvature)
    }

    /// The next half-edge of each half-edge around its face (face on the
    /// left), for the live edges with vertices.
    fn next_links(&self) -> Vec<Option<usize>> {
        let mut out: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (e, edge) in self.edges.iter().enumerate() {
            if edge.alive && edge.v0.is_some() {
                out.entry(self.origin(2 * e)).or_default().push(2 * e);
                out.entry(self.origin(2 * e + 1))
                    .or_default()
                    .push(2 * e + 1);
            }
        }
        let mut position = vec![usize::MAX; 2 * self.edges.len()];
        let mut sorted_at: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (v, list) in out {
            let mut keyed: Vec<(f64, f64, usize)> = list
                .iter()
                .map(|&h| {
                    let (d, k) = self.leaving(h);
                    (d[1].atan2(d[0]), k, h)
                })
                .collect();
            // Cut the circle of directions in its widest gap, so that no
            // group of equal directions straddles the cut.
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            let n = keyed.len();
            let mut widest = (0.0, 0);
            for i in 0..n {
                let next = if i + 1 < n {
                    keyed[i + 1].0
                } else {
                    keyed[0].0 + TAU
                };
                if next - keyed[i].0 > widest.0 {
                    widest = (next - keyed[i].0, i);
                }
            }
            let cut = keyed[widest.1].0 + widest.0 / 2.0;
            for item in &mut keyed {
                item.0 = (item.0 - cut).rem_euclid(TAU);
            }
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            // Equal directions (touching curves) order by curvature.
            let mut groups: Vec<f64> = Vec::with_capacity(n);
            for i in 0..n {
                let same = i > 0 && (keyed[i].0 - keyed[i - 1].0).abs() <= 1e-9;
                groups.push(if same { groups[i - 1] } else { keyed[i].0 });
            }
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by(|&a, &b| {
                groups[a]
                    .total_cmp(&groups[b])
                    .then(keyed[a].1.total_cmp(&keyed[b].1))
                    .then(keyed[a].2.cmp(&keyed[b].2))
            });
            let list: Vec<usize> = order.iter().map(|&i| keyed[i].2).collect();
            for (i, &h) in list.iter().enumerate() {
                position[h] = i;
            }
            sorted_at.insert(v, list);
        }
        let mut next = vec![None; 2 * self.edges.len()];
        for (e, edge) in self.edges.iter().enumerate() {
            if !edge.alive || edge.v0.is_none() {
                continue;
            }
            for h in [2 * e, 2 * e + 1] {
                let twin = h ^ 1;
                let list = &sorted_at[&self.origin(twin)];
                let i = position[twin];
                next[h] = Some(list[(i + list.len() - 1) % list.len()]);
            }
        }
        next
    }

    fn cycles(&self) -> Vec<Vec<usize>> {
        let next = self.next_links();
        let mut seen = vec![false; next.len()];
        let mut cycles = Vec::new();
        for start in 0..next.len() {
            if seen[start] || next[start].is_none() {
                continue;
            }
            let mut cycle = Vec::new();
            let mut h = start;
            while !seen[h] {
                seen[h] = true;
                cycle.push(h);
                h = next[h].expect("linked");
            }
            cycles.push(cycle);
        }
        cycles
    }

    /// Drops dangling pieces and bridges until none are left, then joins
    /// pieces.
    fn clean(&mut self) {
        loop {
            self.drop_dangling();
            let mut bridges = Vec::new();
            for cycle in self.cycles() {
                for &h in &cycle {
                    if h.is_multiple_of(2) && cycle.contains(&(h + 1)) {
                        bridges.push(h / 2);
                    }
                }
            }
            if bridges.is_empty() {
                break;
            }
            for e in bridges {
                self.edges[e].alive = false;
            }
        }
        self.join_pieces();
    }

    /// The key of every live edge.
    fn keys(&self) -> Vec<Option<SegmentKey>> {
        let mut curves_at: Vec<Vec<CurveId>> = vec![Vec::new(); self.vertices.len()];
        for edge in self.edges.iter().filter(|e| e.alive) {
            for v in [edge.v0, edge.v1].into_iter().flatten() {
                curves_at[v].push(self.curves[edge.curve].uid.clone());
            }
        }
        let others = |v: usize, own: &CurveId| -> Vec<CurveId> {
            let mut list: Vec<CurveId> =
                curves_at[v].iter().filter(|c| *c != own).cloned().collect();
            list.sort_unstable();
            list.dedup();
            list
        };
        let mut keys: Vec<Option<SegmentKey>> = vec![None; self.edges.len()];
        for (e, edge) in self.edges.iter().enumerate() {
            if !edge.alive {
                continue;
            }
            let uid = &self.curves[edge.curve].uid;
            keys[e] = Some(match (edge.v0, edge.v1) {
                (Some(v0), Some(v1)) => {
                    SegmentKey::with_ends(uid.clone(), others(v0, uid), others(v1, uid))
                }
                _ => SegmentKey::closed(uid.clone()),
            });
        }
        // Repeats of a curve between the same curves: numbered along it.
        let mut groups: BTreeMap<SegmentKey, Vec<usize>> = BTreeMap::new();
        for (e, key) in keys.iter().enumerate() {
            if let Some(key) = key {
                groups.entry(key.clone()).or_default().push(e);
            }
        }
        for (_, mut members) in groups {
            if members.len() < 2 {
                continue;
            }
            members.sort_by(|a, b| self.edges[*a].t0.total_cmp(&self.edges[*b].t0));
            for (k, e) in members.into_iter().enumerate() {
                if let Some(key) = &mut keys[e] {
                    key.occurrence = Some(k as u32);
                }
            }
        }
        keys
    }

    /// Whether the point `probe` is inside the loop of half-edges (mitcad#364),
    /// decided on the loop's curves and not on its display samples: where the
    /// probe lies within the chord error of a sample segment, that segment
    /// is split (by halving its parameter range) until the chord no longer
    /// reaches the probe, so a loop near a curved boundary is on the side
    /// it really is.
    fn contains(&self, halves: &[usize], probe: P2) -> bool {
        /// How often a segment near the probe is halved: the chord error
        /// falls with the square of the length.
        const DEPTH: u32 = 24;

        fn refine(
            arrangement: &Arrangement<'_>,
            e: usize,
            from: (f64, P2),
            to: (f64, P2),
            probe: P2,
            depth: u32,
            out: &mut Vec<P2>,
        ) {
            let (ta, pa) = from;
            let (tb, pb) = to;
            let tm = (ta + tb) / 2.0;
            let pm = arrangement.eval(e, tm)[0];
            // The chord's error at the middle, doubled: the most the curve
            // lies from the chord between them.
            let slack = 2.0 * dist(pm, scale(add(pa, pb), 0.5));
            out.push(pa);
            if depth == 0 || segment_distance(pa, pb, probe) > slack {
                return;
            }
            out.pop();
            refine(arrangement, e, from, (tm, pm), probe, depth - 1, out);
            refine(arrangement, e, (tm, pm), to, probe, depth - 1, out);
        }

        let mut points = Vec::new();
        for &h in halves {
            let e = h / 2;
            let edge = &self.edges[e];
            let params = self.curve(e).sample_params(edge.t0, edge.t1);
            let at: Vec<(f64, P2)> = params.iter().map(|&t| (t, self.eval(e, t)[0])).collect();
            let mut forward = Vec::new();
            for w in at.windows(2) {
                refine(self, e, w[0], w[1], probe, DEPTH, &mut forward);
            }
            forward.push(at[at.len() - 1].1);
            if h % 2 == 1 {
                forward.reverse();
            }
            forward.pop();
            points.extend(forward);
        }
        polygon_contains(&points, probe)
    }

    /// The loop's points, sampled in its direction.
    fn outline(&self, halves: &[usize]) -> Vec<P2> {
        let mut points = Vec::new();
        for &h in halves {
            let mut samples = self.samples(h / 2);
            if h % 2 == 1 {
                samples.reverse();
            }
            samples.pop();
            points.extend(samples);
        }
        points
    }

    /// The area and first moments (∫x dA, ∫y dA) a loop of half-edges
    /// encloses, from Green's theorem: ∮ x dy, ∮ x²/2 dy and ∮ -y²/2 dx,
    /// with five-point Gauss quadrature between the sample parameters.
    fn moments(&self, halves: &[usize]) -> [f64; 3] {
        const GAUSS: [(f64, f64); 5] = [
            (-0.906_179_845_938_664, 0.236_926_885_056_189_1),
            (-0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
            (0.0, 0.568_888_888_888_888_9),
            (0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
            (0.906_179_845_938_664, 0.236_926_885_056_189_1),
        ];
        let mut m = [0.0; 3];
        for &h in halves {
            let e = h / 2;
            let edge = &self.edges[e];
            let sign = if h.is_multiple_of(2) { 1.0 } else { -1.0 };
            let params = self.curve(e).sample_params(edge.t0, edge.t1);
            for w in params.windows(2) {
                let (mid, half) = ((w[0] + w[1]) / 2.0, (w[1] - w[0]) / 2.0);
                for (x, weight) in GAUSS {
                    let [p, d, _] = self.eval(e, mid + half * x);
                    let f = sign * weight * half;
                    m[0] += f * p[0] * d[1];
                    m[1] += f * 0.5 * p[0] * p[0] * d[1];
                    m[2] -= f * 0.5 * p[1] * p[1] * d[0];
                }
            }
        }
        m
    }

    /// Splits a cycle that passes a vertex more than once into loops.
    fn split_cycle(&self, cycle: &[usize]) -> Vec<Vec<usize>> {
        let mut loops = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        for &h in cycle {
            stack.push(h);
            // The loop closes when this half-edge ends where an earlier one
            // in the stack started.
            let end = self.origin(h ^ 1);
            if let Some(i) = stack.iter().position(|&s| self.origin(s) == end) {
                loops.push(stack.split_off(i));
            }
        }
        if !stack.is_empty() {
            loops.push(stack);
        }
        loops
    }

    fn regions(&self) -> (Vec<Region>, Layout) {
        let keys = self.keys();
        // The segments of a half-edge in loop order: one, or the pieces of
        // a composite curve (backward along a backward half-edge).
        let segment = |h: usize| -> Vec<ProfileSegment> {
            let edge = &self.edges[h / 2];
            let key = keys[h / 2].clone().expect("a live edge");
            let mut pieces: Vec<ProfileSegment> = self.curves[edge.curve]
                .geometry(edge.t0, edge.t1)
                .into_iter()
                .map(|geometry| ProfileSegment {
                    key: key.clone(),
                    geometry,
                })
                .collect();
            if h % 2 == 1 {
                pieces.reverse();
            }
            pieces
        };

        // Loops with their signed areas; isolated closed curves are a loop
        // each way.
        struct Loop {
            segments: Vec<ProfileSegment>,
            outline: Vec<P2>,
            /// The half-edges of the loop, for the exact containment test.
            halves: Vec<usize>,
            /// Signed area and first moments (counter-clockwise positive).
            area: f64,
            moments: [f64; 2],
            component: usize,
        }
        let mut loops: Vec<Loop> = Vec::new();
        // Components: vertices joined by edges.
        let mut parent: Vec<usize> = (0..self.vertices.len()).collect();
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for edge in self.edges.iter().filter(|e| e.alive) {
            if let (Some(a), Some(b)) = (edge.v0, edge.v1) {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                if ra != rb {
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
        // A face's cycle may touch itself where a hole meets its boundary:
        // the largest part is the outer loop, the others are holes.
        let mut faces: Vec<(usize, Vec<usize>)> = Vec::new();
        for cycle in self.cycles() {
            let component = find(&mut parent, self.origin(cycle[0]));
            let whole = self.outline(&cycle);
            let positive = polygon_area(&whole) > 0.0;
            let mut parts: Vec<Loop> = self
                .split_cycle(&cycle)
                .into_iter()
                .map(|part| {
                    let outline = self.outline(&part);
                    let m = self.moments(&part);
                    Loop {
                        segments: part.iter().flat_map(|&h| segment(h)).collect(),
                        area: m[0],
                        moments: [m[1], m[2]],
                        outline,
                        halves: part.clone(),
                        component,
                    }
                })
                .collect();
            if positive {
                let outer = (0..parts.len())
                    .max_by(|a, b| parts[*a].area.total_cmp(&parts[*b].area))
                    .expect("a cycle has parts");
                let outer_loop = parts.remove(outer);
                let first = loops.len();
                loops.push(outer_loop);
                let touching: Vec<usize> = (0..parts.len()).map(|k| first + 1 + k).collect();
                loops.extend(parts);
                faces.push((first, touching));
            } else {
                // The outside of a connected part: a hole of the face around
                // it, if any.
                for part in &mut parts {
                    if part.area > 0.0 {
                        part.area = -part.area;
                        part.moments = part.moments.map(|v| -v);
                    }
                }
                loops.extend(parts);
            }
        }
        let first_isolated = self.vertices.len();
        for (e, edge) in self.edges.iter().enumerate() {
            if !edge.alive || edge.v0.is_some() {
                continue;
            }
            let outline = {
                let mut s = self.samples(e);
                s.pop();
                s
            };
            let mut m = self.moments(&[2 * e]);
            let reversed: Vec<P2> = outline.iter().rev().copied().collect();
            let key = keys[e].clone().expect("a live edge");
            let seg: Vec<ProfileSegment> = self.curves[edge.curve]
                .whole()
                .into_iter()
                .map(|geometry| ProfileSegment {
                    key: key.clone(),
                    geometry,
                })
                .collect();
            let ((ccw, ccw_half), (cw, cw_half)) = if m[0] >= 0.0 {
                ((outline, 2 * e), (reversed, 2 * e + 1))
            } else {
                m = m.map(|v| -v);
                ((reversed, 2 * e + 1), (outline, 2 * e))
            };
            faces.push((loops.len(), Vec::new()));
            loops.push(Loop {
                segments: seg.clone(),
                outline: ccw,
                halves: vec![ccw_half],
                area: m[0],
                moments: [m[1], m[2]],
                component: first_isolated + e,
            });
            loops.push(Loop {
                segments: seg,
                outline: cw,
                halves: vec![cw_half],
                area: -m[0],
                moments: [-m[1], -m[2]],
                component: first_isolated + e,
            });
        }

        // Every outside of a connected part goes into the smallest face of
        // another part that contains it.
        let mut holes: BTreeMap<usize, Vec<usize>> = faces
            .iter()
            .map(|(f, touching)| (*f, touching.clone()))
            .collect();
        let attached: BTreeSet<usize> = holes.values().flatten().copied().collect();
        let face_ids: Vec<usize> = faces.iter().map(|(f, _)| *f).collect();
        // Each face's bounds, from its curves' (mitcad#678): a probe off
        // them is outside it, without sampling its curves. The points the
        // containment test samples lie on the curves, inside these bounds,
        // and a polygon holds no point outside its bounds.
        // (Widened by the tolerance; in the order of `face_ids`.)
        let face_bounds: Vec<(P2, P2)> = face_ids
            .iter()
            .map(|&f| {
                let mut lo = [f64::INFINITY; 2];
                let mut hi = [f64::NEG_INFINITY; 2];
                for &h in &loops[f].halves {
                    let (a, b) = self.bounds[self.edges[h / 2].curve];
                    for k in 0..2 {
                        lo[k] = lo[k].min(a[k] - self.tol);
                        hi[k] = hi[k].max(b[k] + self.tol);
                    }
                }
                (lo, hi)
            })
            .collect();
        for i in 0..loops.len() {
            if loops[i].area > 0.0 || attached.contains(&i) {
                continue;
            }
            let probe = loops[i].outline[0];
            let around = face_ids
                .iter()
                .zip(&face_bounds)
                .filter(|&(&f, (lo, hi))| {
                    probe[0] >= lo[0]
                        && probe[0] <= hi[0]
                        && probe[1] >= lo[1]
                        && probe[1] <= hi[1]
                        && loops[f].component != loops[i].component
                        && self.contains(&loops[f].halves, probe)
                })
                .map(|(&f, _)| f)
                .min_by(|a, b| loops[*a].area.total_cmp(&loops[*b].area));
            if let Some(face) = around {
                holes.entry(face).or_default().push(i);
            }
        }

        let mut regions: Vec<(usize, Region)> = face_ids
            .into_iter()
            .map(|f| {
                let outer = &loops[f];
                let mut profile_loops = vec![ProfileLoop {
                    segments: outer.segments.clone(),
                }];
                let mut area = outer.area;
                let mut moment = outer.moments;
                let mut inner: Vec<&Loop> = holes
                    .get(&f)
                    .map(|h| h.iter().map(|&i| &loops[i]).collect())
                    .unwrap_or_default();
                inner.sort_by_key(|l| {
                    RegionKey::new(l.segments.iter().map(|s| s.key.clone()))
                        .map(|k| k.to_string())
                        .unwrap_or_default()
                });
                let mut holes = Vec::new();
                for hole in inner {
                    profile_loops.push(ProfileLoop {
                        segments: hole.segments.clone(),
                    });
                    area += hole.area;
                    moment = add(moment, hole.moments);
                    holes.push(hole.outline.clone());
                }
                let region = Region {
                    profile: ProfileRegion {
                        key: RegionKey::new(outer.segments.iter().map(|s| s.key.clone()))
                            .expect("a loop has segments"),
                        loops: profile_loops,
                    },
                    area,
                    centroid: if area.abs() > 0.0 {
                        scale(moment, 1.0 / area)
                    } else {
                        polygon_centroid(&outer.outline)
                    },
                    outline: outer.outline.clone(),
                    holes,
                };
                (f, region)
            })
            .collect();
        regions.sort_by(|a, b| a.1.profile.key.cmp(&b.1.profile.key));

        // The region on each side of every half-edge, for the layout.
        let mut side: Vec<Option<LayoutSide>> = vec![None; 2 * self.edges.len()];
        for (index, (f, _)) in regions.iter().enumerate() {
            let mut mark = |l: usize, outer: bool| {
                for &h in &loops[l].halves {
                    side[h] = Some(LayoutSide {
                        region: index,
                        outer,
                    });
                }
            };
            mark(*f, true);
            for &hole in holes.get(f).into_iter().flatten() {
                mark(hole, false);
            }
        }
        let layout = self.layout(&side);
        (regions.into_iter().map(|(_, r)| r).collect(), layout)
    }

    /// The live edges with the regions on their sides ([`Layout`]).
    fn layout(&self, side: &[Option<LayoutSide>]) -> Layout {
        let mut curves_at: Vec<Vec<CurveId>> = vec![Vec::new(); self.vertices.len()];
        let live: Vec<usize> = (0..self.edges.len())
            .filter(|&e| self.edges[e].alive)
            .collect();
        for &e in &live {
            let edge = &self.edges[e];
            for v in [edge.v0, edge.v1].into_iter().flatten() {
                curves_at[v].push(self.curves[edge.curve].uid.clone());
            }
        }
        for list in &mut curves_at {
            list.sort_unstable();
            list.dedup();
        }
        let mut starting: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, &e) in live.iter().enumerate() {
            if let Some(v) = self.edges[e].v0 {
                starting.entry(v).or_default().push(i);
            }
        }
        let edges = live
            .iter()
            .map(|&e| {
                let edge = &self.edges[e];
                let next = edge.v1.and_then(|v| {
                    starting.get(&v)?.iter().copied().find(|&i| {
                        let f = live[i];
                        self.edges[f].curve == edge.curve && self.continues(e, f)
                    })
                });
                let samples = self.samples(e);
                let area = samples.windows(2).map(|w| cross(w[0], w[1]) / 2.0).sum();
                LayoutEdge {
                    curve: self.curves[edge.curve].uid.clone(),
                    face_boundary: edge.face_boundary,
                    sketch_boundary: edge.sketch_boundary,
                    ends: edge.v0.zip(edge.v1),
                    next,
                    area,
                    sides: [side[2 * e], side[2 * e + 1]],
                }
            })
            .collect();
        Layout { curves_at, edges }
    }
}

/// How the regions of a sketch lie against each other: the pieces of the
/// curves between the vertices, each with the region on either side
/// (mitcad#407). A reference whose region a sketch change renamed or
/// split finds its pieces on it ([`Layout::pieces`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    /// The curves that meet at each vertex.
    curves_at: Vec<Vec<CurveId>>,
    edges: Vec<LayoutEdge>,
}

#[derive(Debug, Clone, PartialEq)]
struct LayoutEdge {
    curve: CurveId,
    face_boundary: bool,
    sketch_boundary: bool,
    /// The vertices at its start and end, in the curve's direction; None
    /// for a closed curve that is a loop by itself.
    ends: Option<(usize, usize)>,
    /// The edge that goes on along the curve from its end.
    next: Option<usize>,
    /// The area it sweeps about the origin forwards (counter-clockwise
    /// positive): a loop's sum is its signed area.
    area: f64,
    /// The region on its left (forwards) and on its right; None outside
    /// every region.
    sides: [Option<LayoutSide>; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LayoutSide {
    /// The index of the region in the sorted regions.
    region: usize,
    /// Whether the edge is on the region's outer boundary (else a hole's).
    outer: bool,
}

/// A stretch of edges of one curve, along it.
#[derive(Debug, Clone)]
struct Run {
    edges: Vec<usize>,
    /// The vertices at its start and end; None for a closed curve that is
    /// a loop by itself.
    ends: Option<(usize, usize)>,
}

/// How many choices the loop search tries before it gives up.
const LOOP_SEARCH_LIMIT: usize = 20_000;

impl Layout {
    /// The regions (indices into the sorted regions) that make up the area
    /// a region key outlines (mitcad#407): each of its segments `c[a,b]`
    /// is traced as the edges of curve `c` from where it meets the curves
    /// `a` to where it meets the curves `b`; when these stretches close one
    /// loop, the regions inside it are the pieces of the old region (one,
    /// renamed, or several, split by the change). A region inside the loop
    /// that lies in a hole of the others stays out, as the old region's
    /// holes did. None when the key's segments do not close a loop on the
    /// current curves.
    pub fn pieces(&self, key: &RegionKey) -> Option<Vec<usize>> {
        let candidates: Vec<Vec<Run>> = key.segments().map(|s| self.runs(s)).collect();
        if candidates.iter().any(Vec::is_empty) {
            return None;
        }
        let mut chosen = Vec::with_capacity(candidates.len());
        let mut budget = LOOP_SEARCH_LIMIT;
        let runs = self.close_loop(&candidates, &mut chosen, &mut budget)?;
        let mut on_loop = vec![false; self.edges.len()];
        // Each loop edge with whether the loop runs along it forwards.
        let mut walk: Vec<(usize, bool)> = Vec::new();
        for (e, forward) in self.walk(&runs)? {
            if std::mem::replace(&mut on_loop[e], true) {
                return None;
            }
            walk.push((e, forward));
        }
        // The loop's inside is on its left when it runs counter-clockwise.
        let area: f64 = walk
            .iter()
            .map(|&(e, forward)| {
                let a = self.edges[e].area;
                if forward { a } else { -a }
            })
            .sum();
        let mut inside = self.spread(&walk, &on_loop, area > 0.0)?;
        inside.sort_unstable();
        Some(inside)
    }

    /// The stretches of edges a segment of a key can be now, shortest
    /// first from each start.
    fn runs(&self, segment: &SegmentKey) -> Vec<Run> {
        let mine: Vec<usize> = (0..self.edges.len())
            .filter(|&e| self.edges[e].curve == segment.curve)
            .collect();
        let others = |v: usize| -> Vec<CurveId> {
            self.curves_at[v]
                .iter()
                .filter(|c| **c != segment.curve)
                .cloned()
                .collect()
        };
        let meets = |v: usize, wanted: &[CurveId]| {
            let at = others(v);
            wanted.iter().all(|c| at.contains(c))
        };
        let Some(ends) = &segment.ends else {
            // A closed curve by itself: the whole curve, around.
            let first = *mine.first().unwrap_or(&usize::MAX);
            if first == usize::MAX {
                return Vec::new();
            }
            if self.edges[first].ends.is_none() {
                return vec![Run {
                    edges: vec![first],
                    ends: None,
                }];
            }
            let mut edges = vec![first];
            let mut e = first;
            while let Some(n) = self.edges[e].next {
                if n == first {
                    break;
                }
                if edges.contains(&n) {
                    return Vec::new();
                }
                edges.push(n);
                e = n;
            }
            if self.edges[e].next != Some(first) || edges.len() != mine.len() {
                return Vec::new();
            }
            let v = self.edges[first].ends.map(|(v, _)| v).unwrap_or_default();
            return vec![Run {
                edges,
                ends: Some((v, v)),
            }];
        };
        let (start, end) = (&ends.0, &ends.1);
        let mut out = Vec::new();
        for &first in &mine {
            let Some((v0, _)) = self.edges[first].ends else {
                continue;
            };
            if !meets(v0, start) {
                continue;
            }
            let mut edges = vec![first];
            let mut e = first;
            loop {
                let (_, v1) = self.edges[e].ends.expect("an edge with ends");
                if meets(v1, end) {
                    out.push(Run {
                        edges: edges.clone(),
                        ends: Some((v0, v1)),
                    });
                    break;
                }
                match self.edges[e].next {
                    Some(n) if !edges.contains(&n) => {
                        edges.push(n);
                        e = n;
                    }
                    _ => break,
                }
            }
        }
        out.sort_by_key(|r| r.edges.len());
        out
    }

    /// One run for each segment such that they close one loop: every
    /// vertex they end at is met twice and none of their edges repeats.
    fn close_loop(
        &self,
        candidates: &[Vec<Run>],
        chosen: &mut Vec<usize>,
        budget: &mut usize,
    ) -> Option<Vec<Run>> {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let k = chosen.len();
        let runs: Vec<&Run> = chosen
            .iter()
            .enumerate()
            .map(|(i, &c)| &candidates[i][c])
            .collect();
        if k == candidates.len() {
            let mut degree: BTreeMap<usize, usize> = BTreeMap::new();
            for run in &runs {
                if let Some((a, b)) = run.ends {
                    *degree.entry(a).or_default() += 1;
                    *degree.entry(b).or_default() += 1;
                }
            }
            if degree.values().any(|&d| d != 2) {
                return None;
            }
            let owned: Vec<Run> = runs.into_iter().cloned().collect();
            return self.walk(&owned).map(|_| owned);
        }
        for c in 0..candidates[k].len() {
            let run = &candidates[k][c];
            let clash = runs
                .iter()
                .any(|r| r.edges.iter().any(|e| run.edges.contains(e)));
            let crowded = run.ends.is_some_and(|(a, b)| {
                let degree = |v: usize| {
                    runs.iter()
                        .filter_map(|r| r.ends)
                        .map(|(x, y)| usize::from(x == v) + usize::from(y == v))
                        .sum::<usize>()
                };
                if a == b {
                    degree(a) > 0
                } else {
                    degree(a) > 1 || degree(b) > 1
                }
            });
            if clash || crowded {
                continue;
            }
            chosen.push(c);
            if let Some(found) = self.close_loop(candidates, chosen, budget) {
                return Some(found);
            }
            chosen.pop();
            if *budget == 0 {
                return None;
            }
        }
        None
    }

    /// The edges of runs that close one loop, in order around it, each with
    /// whether the loop goes along it forwards; None when they make no
    /// single loop.
    fn walk(&self, runs: &[Run]) -> Option<Vec<(usize, bool)>> {
        let along = |run: &Run, forward: bool| -> Vec<(usize, bool)> {
            if forward {
                run.edges.iter().map(|&e| (e, true)).collect()
            } else {
                run.edges.iter().rev().map(|&e| (e, false)).collect()
            }
        };
        let first = runs.first()?;
        let Some((start, mut at)) = first.ends else {
            return (runs.len() == 1).then(|| along(first, true));
        };
        let mut used = vec![false; runs.len()];
        used[0] = true;
        let mut out = along(first, true);
        while at != start || used.iter().any(|u| !u) {
            let (i, forward) = runs.iter().enumerate().find_map(|(i, r)| {
                let (a, b) = r.ends?;
                if used[i] {
                    None
                } else if a == at {
                    Some((i, true))
                } else if b == at {
                    Some((i, false))
                } else {
                    None
                }
            })?;
            used[i] = true;
            let (a, b) = runs[i].ends?;
            at = if forward { b } else { a };
            out.extend(along(&runs[i], forward));
            if at == start && used.iter().any(|u| !u) {
                return None;
            }
        }
        Some(out)
    }

    /// The regions on one side of the loop (its left when `left`), spread
    /// across the edges off the loop that are outer boundaries on both
    /// sides (not into a hole: an island inside stays out); None when they
    /// reach the outside of every region.
    fn spread(&self, walk: &[(usize, bool)], on_loop: &[bool], left: bool) -> Option<Vec<usize>> {
        let mut found: Vec<usize> = Vec::new();
        for &(e, forward) in walk {
            let side = self.edges[e].sides[usize::from(forward != left)]?;
            if !found.contains(&side.region) {
                found.push(side.region);
            }
        }
        let mut k = 0;
        while k < found.len() {
            let region = found[k];
            k += 1;
            for (e, edge) in self.edges.iter().enumerate() {
                if on_loop[e] {
                    continue;
                }
                for (mine, other) in [(0, 1), (1, 0)] {
                    let Some(here) = edge.sides[mine] else {
                        continue;
                    };
                    if here.region != region || !here.outer {
                        continue;
                    }
                    let there = edge.sides[other]?;
                    if there.outer && !found.contains(&there.region) {
                        found.push(there.region);
                    }
                }
            }
        }
        Some(found)
    }
}

/// The region a reference whose key no longer exists means: the one with
/// the most curves in common with the key, then the most identical
/// segments, then the fewest other curves, then the largest; None when no
/// region shares a curve.
pub fn best_match<'a>(
    regions: impl IntoIterator<Item = &'a Region>,
    key: &RegionKey,
) -> Option<&'a Region> {
    let wanted: Vec<CurveId> = key.segments().map(|s| s.curve.clone()).collect();
    let wanted_segments: Vec<&SegmentKey> = key.segments().collect();
    regions
        .into_iter()
        .map(|r| {
            let curves: Vec<CurveId> = r.profile.key.segments().map(|s| s.curve.clone()).collect();
            let common = curves.iter().filter(|c| wanted.contains(c)).count();
            let exact = r
                .profile
                .key
                .segments()
                .filter(|s| wanted_segments.contains(s))
                .count();
            let extra = curves.len() - common;
            (common, exact, std::cmp::Reverse(extra), r)
        })
        .filter(|(common, ..)| *common > 0)
        .max_by(|a, b| {
            (a.0, a.1, a.2)
                .cmp(&(b.0, b.1, b.2))
                .then_with(|| a.3.area.abs().total_cmp(&b.3.area.abs()))
                .then_with(|| b.3.profile.key.cmp(&a.3.profile.key))
        })
        .map(|(.., r)| r)
}

/// The distance from `p` to the segment `a`-`b`.
fn segment_distance(a: P2, b: P2, p: P2) -> f64 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    let t = if len2 > 0.0 {
        (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    dist(p, add(a, scale(ab, t)))
}
