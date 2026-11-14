// SPDX-License-Identifier: MIT
//! What the 3-axis operations share: a grid of the cutter's resting
//! heights, the edges of the area where the tip may be at a height (the
//! constant-height contours of the cutter's offset surface), polylines
//! drawn within a tolerance, outlines as polygons and the way between two
//! cuts.
//!
//! **Contours.** The nodes of a grid are blocked or free at a height (the
//! cutter dropped there rests above it, or the node is outside the
//! boundary). The edge of the free area runs through each cell whose
//! corners differ (marching squares), crossing each grid edge between a
//! free and a blocked node where the test flips, found by bisection to a
//! precision (each crossing is the free end of its last interval, so it
//! is free itself). A cell with two blocked corners across from each
//! other asks its center which way the area joins. The contours keep the
//! free area on their left: outer edges counter-clockwise, the edges of
//! holes (the part's islands) clockwise, which is climb milling round
//! the part with a clockwise spindle.

use std::collections::{BTreeMap, BTreeSet};

use super::surface::Surface;
use crate::CamError;
use crate::geom::Contour;
use crate::toolpath::{PathBuilder, arc_sweep};

/// The most nodes a grid may have.
const MAX_NODES: usize = 16_000_000;

/// A grid of nodes `h` apart in XY, row by row (`j * nx + i`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Grid {
    pub x0: f64,
    pub y0: f64,
    pub h: f64,
    pub nx: usize,
    pub ny: usize,
}

impl Grid {
    /// Nodes over the box `[min, max]` and a step beyond it on every side.
    pub(crate) fn covering(min: [f64; 2], max: [f64; 2], h: f64) -> Result<Grid, CamError> {
        let count = |lo: f64, hi: f64| ((hi - lo) / h).ceil().max(0.0) + 3.0;
        let (cx, cy) = (count(min[0], max[0]), count(min[1], max[1]));
        if !(cx.is_finite() && cy.is_finite()) || cx * cy > MAX_NODES as f64 {
            return Err(CamError::new(
                "The area is too large to machine at this tolerance: raise the tolerance or pick a smaller boundary.",
            ));
        }
        Ok(Grid {
            x0: min[0] - h,
            y0: min[1] - h,
            h,
            nx: cx as usize,
            ny: cy as usize,
        })
    }

    pub(crate) fn node(&self, i: usize, j: usize) -> [f64; 2] {
        [self.x0 + i as f64 * self.h, self.y0 + j as f64 * self.h]
    }

    /// Every node's place, row by row.
    pub(crate) fn nodes(&self) -> Vec<[f64; 2]> {
        let mut out = Vec::with_capacity(self.nx * self.ny);
        for j in 0..self.ny {
            for i in 0..self.nx {
                out.push(self.node(i, j));
            }
        }
        out
    }
}

/// The grid's step for a cutter of `radius` and a `tolerance`: a chord
/// between two crossings of a contour of curvature `radius` stays within
/// the tolerance of it. Two crossings in a row are on the edges of one
/// cell, up to its diagonal apart: a chord `L` long sags `L² / 8r`, so the
/// step is `√(8 r tol) / √2`.
pub(crate) fn grid_step(radius: f64, tolerance: f64) -> f64 {
    (4.0 * radius * tolerance)
        .sqrt()
        .min(radius / 2.0)
        .max(1e-3)
}

/// A contour: its points in order (each free), closed or ending at the
/// grid's edge, with the blocked end of each point's last interval (a
/// place where the cutter rests on what blocks it).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Chain {
    pub points: Vec<[f64; 2]>,
    pub inner: Vec<[f64; 2]>,
    pub closed: bool,
}

/// An edge of the grid: 0 from node `(i, j)` to `(i + 1, j)`, 1 from
/// `(i, j)` to `(i, j + 1)`.
type Key = (u8, usize, usize);

/// Where the test flips along an edge of the grid between a free and a
/// blocked node: the free and the blocked end of the last interval of the
/// bisection.
fn crossing(
    grid: &Grid,
    blocked: &[bool],
    is_blocked: &dyn Fn([f64; 2]) -> bool,
    precision: f64,
    key: Key,
) -> ([f64; 2], [f64; 2]) {
    let (kind, i, j) = key;
    let (i1, j1) = if kind == 0 { (i + 1, j) } else { (i, j + 1) };
    let (a, b) = (grid.node(i, j), grid.node(i1, j1));
    let (mut free, mut solid) = if blocked[j * grid.nx + i] {
        (b, a)
    } else {
        (a, b)
    };
    for _ in 0..64 {
        if (free[0] - solid[0]).hypot(free[1] - solid[1]) <= precision {
            break;
        }
        let mid = [(free[0] + solid[0]) / 2.0, (free[1] + solid[1]) / 2.0];
        if is_blocked(mid) {
            solid = mid;
        } else {
            free = mid;
        }
    }
    (free, solid)
}

/// The edges of the free nodes' area (see the module documentation):
/// `blocked` per node, `is_blocked` the same test anywhere.
pub(crate) fn free_chains(
    grid: &Grid,
    blocked: &[bool],
    is_blocked: &dyn Fn([f64; 2]) -> bool,
    precision: f64,
) -> Vec<Chain> {
    let nx = grid.nx;
    let at = |i: usize, j: usize| blocked[j * nx + i];
    let mut points: BTreeMap<Key, ([f64; 2], [f64; 2])> = BTreeMap::new();
    let mut next: BTreeMap<Key, Key> = BTreeMap::new();
    for j in 0..grid.ny.saturating_sub(1) {
        for i in 0..nx.saturating_sub(1) {
            // The corners counter-clockwise, edge k from corner k to k + 1.
            let corners = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
            let count = corners.iter().filter(|&&b| b).count();
            if count == 0 || count == 4 {
                continue;
            }
            let edges: [Key; 4] = [(0, i, j), (1, i + 1, j), (0, i, j + 1), (1, i, j)];
            let mut enters = Vec::new();
            let mut leaves = Vec::new();
            for k in 0..4 {
                let (a, b) = (corners[k], corners[(k + 1) % 4]);
                if !a && b {
                    enters.push(k);
                } else if a && !b {
                    leaves.push(k);
                }
            }
            let mut pairs: Vec<(usize, usize)> = Vec::new();
            if enters.len() == 1 && leaves.len() == 1 {
                pairs.push((enters[0], leaves[0]));
            } else {
                // Two blocked corners across from each other: joined
                // through the cell's center or apart.
                let c = grid.node(i, j);
                let joined = is_blocked([c[0] + grid.h / 2.0, c[1] + grid.h / 2.0]);
                for &e in &enters {
                    pairs.push((e, if joined { (e + 3) % 4 } else { (e + 1) % 4 }));
                }
            }
            for (e, l) in pairs {
                let (from, to) = (edges[e], edges[l]);
                for key in [from, to] {
                    points
                        .entry(key)
                        .or_insert_with(|| crossing(grid, blocked, is_blocked, precision, key));
                }
                next.insert(from, to);
            }
        }
    }
    // The chains that end at the grid's edge first (from their heads),
    // then the closed ones.
    let targets: BTreeSet<Key> = next.values().copied().collect();
    let heads: Vec<Key> = next
        .keys()
        .copied()
        .filter(|k| !targets.contains(k))
        .collect();
    let mut chains = Vec::new();
    let mut follow = |start: Key, next: &mut BTreeMap<Key, Key>| {
        let mut key = start;
        let mut pts = vec![points[&start].0];
        let mut inner = vec![points[&start].1];
        let mut closed = false;
        while let Some(to) = next.remove(&key) {
            if to == start {
                closed = true;
                break;
            }
            pts.push(points[&to].0);
            inner.push(points[&to].1);
            key = to;
        }
        chains.push(Chain {
            points: pts,
            inner,
            closed,
        });
    };
    for head in heads {
        follow(head, &mut next);
    }
    while let Some(start) = next.keys().next().copied() {
        follow(start, &mut next);
    }
    chains.retain(|c| c.points.len() >= 2);
    chains
}

/// The distance from `p` to the segment from `a` to `b`.
pub(crate) fn segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// The indices of the points of a polyline that draw it within `tol`
/// (Douglas and Peucker), its ends kept.
pub(crate) fn simplified(points: &[[f64; 2]], tol: f64) -> Vec<usize> {
    let n = points.len();
    if n <= 2 {
        return (0..n).collect();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0, n - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let mut far = a;
        let mut most = -1.0;
        for (k, p) in points.iter().enumerate().take(b).skip(a + 1) {
            let d = segment_distance(*p, points[a], points[b]);
            if d > most {
                most = d;
                far = k;
            }
        }
        if most > tol {
            keep[far] = true;
            stack.push((a, far));
            stack.push((far, b));
        }
    }
    (0..n).filter(|&k| keep[k]).collect()
}

/// The indices of the points of a closed polyline that draw it within
/// `tol`: split at its first point and the point farthest from it.
pub(crate) fn simplified_loop(points: &[[f64; 2]], tol: f64) -> Vec<usize> {
    let n = points.len();
    if n <= 3 {
        return (0..n).collect();
    }
    let p0 = points[0];
    let far = (1..n)
        .max_by(|&a, &b| {
            let da = (points[a][0] - p0[0]).hypot(points[a][1] - p0[1]);
            let db = (points[b][0] - p0[0]).hypot(points[b][1] - p0[1]);
            da.total_cmp(&db)
        })
        .unwrap_or(1);
    let mut out = simplified(&points[..=far], tol);
    let mut back: Vec<[f64; 2]> = points[far..].to_vec();
    back.push(p0);
    let last = back.len() - 1;
    for k in simplified(&back, tol) {
        if k != 0 && k != last {
            out.push(far + k);
        }
    }
    out
}

/// A closed or open outline as points, its arcs within `tol`.
pub(crate) fn polygon(contour: &Contour, tol: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for s in contour.segments() {
        out.push(s.start);
        if let Some((center, ccw)) = s.arc {
            let r = (s.start[0] - center[0]).hypot(s.start[1] - center[1]);
            let sweep = arc_sweep(s.start, s.end, center, ccw);
            let step = if r > tol {
                2.0 * (1.0 - tol / r).clamp(-1.0, 1.0).acos()
            } else {
                std::f64::consts::TAU
            };
            let n = ((sweep.abs() / step.max(1e-3)).ceil() as usize).max(1);
            let a0 = (s.start[1] - center[1]).atan2(s.start[0] - center[0]);
            for i in 1..n {
                let a = a0 + sweep * i as f64 / n as f64;
                out.push([center[0] + r * a.cos(), center[1] + r * a.sin()]);
            }
        }
    }
    if !contour.closed
        && let Some(last) = contour.vertices.last()
    {
        out.push(last.pos());
    }
    out
}

/// The tolerance of the 3-axis operations when they give none (mm): how
/// far their paths may be from the exact ones between their points.
pub(crate) const TOLERANCE: f64 = 0.01;

/// What the 3-axis operations check of their tolerance, stock to leave and
/// retract height: the tool travels at the retract height above the
/// bodies.
pub(crate) fn check_surface_op(
    surface: &Surface,
    tolerance: f64,
    stock_to_leave: f64,
    retract: f64,
) -> Result<(), CamError> {
    if !(tolerance.is_finite() && tolerance > 0.0) {
        return Err(CamError::new("The tolerance must be more than zero."));
    }
    if !(stock_to_leave.is_finite() && stock_to_leave >= 0.0) {
        return Err(CamError::new("The stock to leave must be zero or more."));
    }
    if surface.bounds().1[2] + stock_to_leave >= retract - 1e-9 {
        return Err(CamError::new(
            "The bodies reach up to the retract height: raise it above them.",
        ));
    }
    Ok(())
}

/// A slope limit in degrees, checked.
pub(crate) fn check_slope(slope: Option<f64>) -> Result<(), CamError> {
    match slope {
        Some(s) if !(s.is_finite() && (0.0..=90.0).contains(&s)) => {
            Err(CamError::new("The slope must be from 0 to 90 degrees."))
        }
        _ => Ok(()),
    }
}

/// The way to the next cut: up to `hop` where the tool is (or no lower
/// than it is), across, and down to `hop`; never down and across at once.
pub(crate) fn travel(b: &mut PathBuilder, xy: [f64; 2], hop: f64) {
    match b.at() {
        None => b.rapid([xy[0], xy[1], hop]),
        Some(at) => {
            let z = at[2].max(hop);
            if at[2] < z {
                b.rapid([at[0], at[1], z]);
            }
            b.rapid([xy[0], xy[1], z]);
            if z > hop {
                b.rapid([xy[0], xy[1], hop]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disc of radius 2 about (1, 1) blocked: one clockwise contour
    /// round it, every crossing on the circle.
    #[test]
    fn a_blocked_disc_has_one_clockwise_contour_on_its_edge() {
        let grid = Grid::covering([-3.0, -3.0], [5.0, 5.0], 0.25).unwrap();
        let test = |p: [f64; 2]| (p[0] - 1.0).hypot(p[1] - 1.0) < 2.0;
        let blocked: Vec<bool> = grid.nodes().into_iter().map(test).collect();
        let chains = free_chains(&grid, &blocked, &test, 1e-10);
        assert_eq!(chains.len(), 1);
        let c = &chains[0];
        assert!(c.closed);
        assert!(c.points.len() > 40);
        for p in &c.points {
            let r = (p[0] - 1.0).hypot(p[1] - 1.0);
            assert!((2.0..2.0 + 1e-9).contains(&r), "{r}");
        }
        let area: f64 = (0..c.points.len())
            .map(|k| {
                let a = c.points[k];
                let b = c.points[(k + 1) % c.points.len()];
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            / 2.0;
        assert!(
            area < 0.0,
            "the free area on the left: clockwise round the hole"
        );
        assert!((area.abs() - std::f64::consts::PI * 4.0).abs() < 0.1);
    }

    /// Two blocked squares across a cell from each other are joined
    /// through it when the cell's center is blocked.
    #[test]
    fn a_saddle_asks_its_center() {
        // Nodes 1 apart: the squares block nodes (1..2, 1..2) and
        // (3..4, 3..4), the cell from (2, 2) to (3, 3) has them across.
        let grid = Grid::covering([0.0, 0.0], [5.0, 5.0], 1.0).unwrap();
        let apart = |p: [f64; 2]| {
            let inside = |c: [f64; 2]| (p[0] - c[0]).abs() < 0.9 && (p[1] - c[1]).abs() < 0.9;
            inside([1.5, 1.5]) || inside([3.5, 3.5])
        };
        let blocked: Vec<bool> = grid.nodes().into_iter().map(apart).collect();
        assert_eq!(free_chains(&grid, &blocked, &apart, 1e-9).len(), 2);
        let joined = |p: [f64; 2]| apart(p) || (p[0] - 2.5).hypot(p[1] - 2.5) < 0.6;
        let blocked: Vec<bool> = grid.nodes().into_iter().map(joined).collect();
        assert_eq!(free_chains(&grid, &blocked, &joined, 1e-9).len(), 1);
    }

    #[test]
    fn polylines_are_drawn_within_the_tolerance() {
        let pts: Vec<[f64; 2]> = (0..=100)
            .map(|k| {
                let x = k as f64 / 10.0;
                [x, 0.001 * (x * 3.0).sin()]
            })
            .collect();
        let kept = simplified(&pts, 0.01);
        assert_eq!(kept, vec![0, 100]);
        let kept = simplified(&pts, 1e-5);
        assert!(kept.len() > 10);
        let circle: Vec<[f64; 2]> = (0..360)
            .map(|k| {
                let a = (k as f64).to_radians();
                [10.0 * a.cos(), 10.0 * a.sin()]
            })
            .collect();
        let kept = simplified_loop(&circle, 0.01);
        assert!(kept.len() > 20 && kept.len() < 360);
        for w in 0..kept.len() {
            let a = circle[kept[w]];
            let b = circle[kept[(w + 1) % kept.len()]];
            let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            assert!(10.0 - mid[0].hypot(mid[1]) <= 0.01 + 1e-9);
        }
    }

    #[test]
    fn travel_goes_up_across_and_down() {
        let mut b = PathBuilder::new();
        travel(&mut b, [1.0, 1.0], 10.0);
        b.line([1.0, 1.0, -2.0], 100.0);
        travel(&mut b, [5.0, 1.0], 2.0);
        let p = b.finish();
        let to: Vec<[f64; 3]> = p.moves.iter().map(|m| m.to()).collect();
        assert_eq!(
            to,
            vec![
                [1.0, 1.0, 10.0],
                [1.0, 1.0, -2.0],
                [1.0, 1.0, 2.0],
                [5.0, 1.0, 2.0]
            ]
        );
    }
}
