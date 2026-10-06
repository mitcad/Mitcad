// SPDX-License-Identifier: MIT
//! Profile regions: the bounded faces of the planar arrangement of the
//! sketch's profile curves (the profiles).
//!
//! 1. Every pair of curves is intersected (`intersect.rs`); curves are cut
//!    at the intersections and their own ends. Points closer than the
//!    tolerance are one vertex; overlapping pieces are kept once.
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

use std::collections::BTreeMap;
use std::f64::consts::TAU;

use super::geometry::{
    Curve2, P2, add, cross, dist, norm, polygon_area, polygon_centroid, polygon_contains, scale,
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
    alive: bool,
}

struct Arrangement<'a> {
    curves: &'a [RegionCurve],
    tol: f64,
    vertices: Vec<P2>,
    edges: Vec<Edge>,
}

/// Computes the regions of the curves (in any order; the result is sorted
/// by key).
pub fn regions(curves: &[RegionCurve]) -> Vec<Region> {
    if curves.is_empty() {
        return Vec::new();
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
    let tol = 1e-9 * size;
    let mut arrangement = Arrangement::new(curves, &bounds, tol);
    arrangement.clean();
    arrangement.regions()
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
    fn new(curves: &'a [RegionCurve], bounds: &[(P2, P2)], tol: f64) -> Self {
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
                        alive: true,
                    });
                }
            }
        }
        let mut arrangement = Self {
            curves,
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
        for e in 0..self.edges.len() {
            let Some(me) = mids[e] else { continue };
            if !self.edges[e].alive {
                continue;
            }
            let ends = |x: &Edge| {
                let (a, b) = (x.v0.unwrap_or(usize::MAX), x.v1.unwrap_or(usize::MAX));
                (a.min(b), a.max(b))
            };
            for (f, mid) in mids.iter().enumerate().skip(e + 1) {
                let Some(mf) = *mid else { continue };
                if self.edges[f].alive
                    && ends(&self.edges[e]) == ends(&self.edges[f])
                    && dist(me, mf) <= 100.0 * self.tol
                {
                    // Keep the lower curve id.
                    let (ue, uf) = (
                        self.curves[self.edges[e].curve].uid,
                        self.curves[self.edges[f].curve].uid,
                    );
                    if uf < ue {
                        self.edges[e].alive = false;
                        break;
                    }
                    self.edges[f].alive = false;
                }
            }
        }
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
        loop {
            let degree = self.degrees();
            let mut joined = false;
            for (v, d) in degree.iter().enumerate() {
                if *d != 2 {
                    continue;
                }
                let at: Vec<usize> = (0..self.edges.len())
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
                        if self.edges[a].curve != self.edges[b].curve {
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
                curves_at[v].push(self.curves[edge.curve].uid);
            }
        }
        let others = |v: usize, own: CurveId| -> Vec<CurveId> {
            let mut list: Vec<CurveId> =
                curves_at[v].iter().copied().filter(|c| *c != own).collect();
            list.sort_unstable();
            list.dedup();
            list
        };
        let mut keys: Vec<Option<SegmentKey>> = vec![None; self.edges.len()];
        for (e, edge) in self.edges.iter().enumerate() {
            if !edge.alive {
                continue;
            }
            let uid = self.curves[edge.curve].uid;
            keys[e] = Some(match (edge.v0, edge.v1) {
                (Some(v0), Some(v1)) => {
                    SegmentKey::with_ends(uid, others(v0, uid), others(v1, uid))
                }
                _ => SegmentKey::closed(uid),
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

    fn regions(&self) -> Vec<Region> {
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
            let (ccw, cw) = if m[0] >= 0.0 {
                (outline, reversed)
            } else {
                m = m.map(|v| -v);
                (reversed, outline)
            };
            faces.push((loops.len(), Vec::new()));
            loops.push(Loop {
                segments: seg.clone(),
                outline: ccw,
                area: m[0],
                moments: [m[1], m[2]],
                component: first_isolated + e,
            });
            loops.push(Loop {
                segments: seg,
                outline: cw,
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
        let attached: Vec<usize> = holes.values().flatten().copied().collect();
        let face_ids: Vec<usize> = faces.iter().map(|(f, _)| *f).collect();
        for i in 0..loops.len() {
            if loops[i].area > 0.0 || attached.contains(&i) {
                continue;
            }
            let probe = loops[i].outline[0];
            let around = face_ids
                .iter()
                .copied()
                .filter(|&f| {
                    loops[f].component != loops[i].component
                        && polygon_contains(&loops[f].outline, probe)
                })
                .min_by(|a, b| loops[*a].area.total_cmp(&loops[*b].area));
            if let Some(face) = around {
                holes.entry(face).or_default().push(i);
            }
        }

        let mut regions: Vec<Region> = face_ids
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
                Region {
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
                }
            })
            .collect();
        regions.sort_by(|a, b| a.profile.key.cmp(&b.profile.key));
        regions
    }
}

/// The region a reference whose key no longer exists means: the one with
/// the most curves in common with the key, then the most identical
/// segments, then the fewest other curves, then the largest; None when no
/// region shares a curve.
pub fn best_match<'a>(regions: &'a [Region], key: &RegionKey) -> Option<&'a Region> {
    let wanted: Vec<CurveId> = key.segments().map(|s| s.curve).collect();
    let wanted_segments: Vec<&SegmentKey> = key.segments().collect();
    regions
        .iter()
        .map(|r| {
            let curves: Vec<CurveId> = r.profile.key.segments().map(|s| s.curve).collect();
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
