// SPDX-License-Identifier: MIT
//! A tree of boxes over a mesh's triangles (mitcad#2515): the nearest point
//! of the mesh to a point (Align's best fit, the distance an offset lies
//! from its mesh) and the pairs of triangles whose boxes overlap (where a
//! mesh may cross itself).
//!
//! Each node's box holds its triangles; a node of more than [`LEAF`]
//! triangles splits at the middle one along its box's longest side.

use mitcad_vec3::{add, dot, scale, sub};

use super::{Point, TriangleMesh, corners};

/// The most triangles in a leaf.
const LEAF: usize = 4;

#[derive(Debug, Clone, Copy)]
struct Node {
    min: Point,
    max: Point,
    /// Leaf: its triangles `order[start..start + count]`; otherwise
    /// `count` is 0 and its children are `start` and `start + 1`.
    start: usize,
    count: usize,
}

/// The tree over a mesh's triangles.
#[derive(Debug, Clone)]
pub(crate) struct Bvh {
    nodes: Vec<Node>,
    order: Vec<usize>,
    triangles: Vec<[Point; 3]>,
}

/// The nearest point of the mesh found by [`Bvh::closest`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Closest {
    pub triangle: usize,
    pub point: Point,
    pub distance: f64,
}

fn grow(min: &mut Point, max: &mut Point, p: Point) {
    for i in 0..3 {
        min[i] = min[i].min(p[i]);
        max[i] = max[i].max(p[i]);
    }
}

/// The squared distance from `p` to the box (0 inside).
fn box_distance2(min: &Point, max: &Point, p: Point) -> f64 {
    (0..3)
        .map(|i| {
            let d = (min[i] - p[i]).max(0.0).max(p[i] - max[i]);
            d * d
        })
        .sum()
}

fn boxes_overlap(a_min: &Point, a_max: &Point, b_min: &Point, b_max: &Point, slack: f64) -> bool {
    (0..3).all(|i| a_min[i] <= b_max[i] + slack && b_min[i] <= a_max[i] + slack)
}

impl Bvh {
    /// The tree over the mesh's triangles.
    pub fn new(mesh: &TriangleMesh) -> Self {
        let triangles: Vec<[Point; 3]> = (0..mesh.triangles.len())
            .map(|t| corners(mesh, t))
            .collect();
        Self::of_triangles(triangles)
    }

    /// The tree over triangles given by their corners.
    pub fn of_triangles(triangles: Vec<[Point; 3]>) -> Self {
        let mut tree = Bvh {
            nodes: Vec::with_capacity(2 * triangles.len() / LEAF + 1),
            order: (0..triangles.len()).collect(),
            triangles,
        };
        if tree.triangles.is_empty() {
            return tree;
        }
        let centres: Vec<Point> = tree.triangles.iter().map(super::centroid).collect();
        tree.nodes.push(Node {
            min: [0.0; 3],
            max: [0.0; 3],
            start: 0,
            count: 0,
        });
        // (node, first, end) to build.
        let mut stack = vec![(0usize, 0usize, tree.order.len())];
        while let Some((node, first, end)) = stack.pop() {
            let mut min = [f64::INFINITY; 3];
            let mut max = [f64::NEG_INFINITY; 3];
            for &t in &tree.order[first..end] {
                for p in tree.triangles[t] {
                    grow(&mut min, &mut max, p);
                }
            }
            tree.nodes[node].min = min;
            tree.nodes[node].max = max;
            if end - first <= LEAF {
                tree.nodes[node].start = first;
                tree.nodes[node].count = end - first;
                continue;
            }
            let extent = sub(max, min);
            let axis = if extent[0] >= extent[1] && extent[0] >= extent[2] {
                0
            } else if extent[1] >= extent[2] {
                1
            } else {
                2
            };
            let middle = first + (end - first) / 2;
            tree.order[first..end].select_nth_unstable_by(middle - first, |&a, &b| {
                centres[a][axis].total_cmp(&centres[b][axis])
            });
            let left = tree.nodes.len();
            for _ in 0..2 {
                tree.nodes.push(Node {
                    min: [0.0; 3],
                    max: [0.0; 3],
                    start: 0,
                    count: 0,
                });
            }
            tree.nodes[node].start = left;
            tree.nodes[node].count = 0;
            stack.push((left, first, middle));
            stack.push((left + 1, middle, end));
        }
        tree
    }

    /// The triangle's corners.
    pub fn triangle(&self, t: usize) -> &[Point; 3] {
        &self.triangles[t]
    }

    /// The point of the mesh nearest to `p`; None for a mesh without
    /// triangles.
    pub fn closest(&self, p: Point) -> Option<Closest> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best: Option<Closest> = None;
        let mut best2 = f64::INFINITY;
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = self.nodes[n];
            if box_distance2(&node.min, &node.max, p) >= best2 {
                continue;
            }
            if node.count > 0 {
                for &t in &self.order[node.start..node.start + node.count] {
                    let q = closest_on_triangle(p, &self.triangles[t]);
                    let d = sub(p, q);
                    let d2 = dot(d, d);
                    if d2 < best2 {
                        best2 = d2;
                        best = Some(Closest {
                            triangle: t,
                            point: q,
                            distance: d2.sqrt(),
                        });
                    }
                }
                continue;
            }
            // The nearer child last, so that it is looked at first.
            let (a, b) = (node.start, node.start + 1);
            let da = box_distance2(&self.nodes[a].min, &self.nodes[a].max, p);
            let db = box_distance2(&self.nodes[b].min, &self.nodes[b].max, p);
            if da < db {
                stack.push(b);
                stack.push(a);
            } else {
                stack.push(a);
                stack.push(b);
            }
        }
        best
    }

    /// The nearest triangle accepted by `accept` that the segment from
    /// `from` along `direction` (unit), `length` long, passes through:
    /// its number and how far along (mitcad#2564: where a mesh's sides
    /// come close).
    pub fn first_hit(
        &self,
        from: Point,
        direction: Point,
        length: f64,
        accept: impl Fn(usize) -> bool,
    ) -> Option<(usize, f64)> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best: Option<(usize, f64)> = None;
        let mut reach = length;
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = self.nodes[n];
            // The segment's part inside the node's box (slabs).
            let mut enter = 0.0f64;
            let mut leave = reach;
            for i in 0..3 {
                if direction[i].abs() < 1e-300 {
                    if from[i] < node.min[i] || from[i] > node.max[i] {
                        leave = -1.0;
                    }
                    continue;
                }
                let a = (node.min[i] - from[i]) / direction[i];
                let b = (node.max[i] - from[i]) / direction[i];
                enter = enter.max(a.min(b));
                leave = leave.min(a.max(b));
            }
            if enter > leave {
                continue;
            }
            if node.count == 0 {
                stack.push(node.start);
                stack.push(node.start + 1);
                continue;
            }
            for &t in &self.order[node.start..node.start + node.count] {
                if !accept(t) {
                    continue;
                }
                if let Some(at) = segment_hits(from, direction, &self.triangles[t])
                    && at <= reach
                {
                    reach = at;
                    best = Some((t, at));
                }
            }
        }
        best
    }

    /// Each pair of triangles `(a, b)`, `a < b`, whose boxes overlap (grown
    /// by `slack`).
    pub fn overlapping_pairs(&self, slack: f64, mut each: impl FnMut(usize, usize)) {
        if self.nodes.is_empty() {
            return;
        }
        let mut stack = Vec::new();
        for (a, tri) in self.triangles.iter().enumerate() {
            let mut min = [f64::INFINITY; 3];
            let mut max = [f64::NEG_INFINITY; 3];
            for p in tri {
                grow(&mut min, &mut max, *p);
            }
            stack.clear();
            stack.push(0usize);
            while let Some(n) = stack.pop() {
                let node = self.nodes[n];
                if !boxes_overlap(&min, &max, &node.min, &node.max, slack) {
                    continue;
                }
                if node.count == 0 {
                    stack.push(node.start);
                    stack.push(node.start + 1);
                    continue;
                }
                for &b in &self.order[node.start..node.start + node.count] {
                    if b <= a {
                        continue;
                    }
                    let other = &self.triangles[b];
                    let mut bmin = [f64::INFINITY; 3];
                    let mut bmax = [f64::NEG_INFINITY; 3];
                    for p in other {
                        grow(&mut bmin, &mut bmax, *p);
                    }
                    if boxes_overlap(&min, &max, &bmin, &bmax, slack) {
                        each(a, b);
                    }
                }
            }
        }
    }
}

/// How far along the ray from `from` along `direction` it passes through
/// the triangle (not behind its start); None where it misses.
fn segment_hits(from: Point, direction: Point, tri: &[Point; 3]) -> Option<f64> {
    let e1 = sub(tri[1], tri[0]);
    let e2 = sub(tri[2], tri[0]);
    let p = mitcad_vec3::cross(direction, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-300 {
        return None;
    }
    let s = sub(from, tri[0]);
    let u = dot(s, p) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = mitcad_vec3::cross(s, e1);
    let v = dot(direction, q) / det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(e2, q) / det;
    (t >= 0.0).then_some(t)
}

/// The point of the triangle nearest to `p` (by its regions: a corner, an
/// edge or the inside).
pub(crate) fn closest_on_triangle(p: Point, tri: &[Point; 3]) -> Point {
    closest_on_triangle_at(p, tri).0
}

/// Where on a triangle its point nearest to a point lies (mitcad#2564: the
/// sign of the distance to a closed mesh is taken from the normal there).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TriangleRegion {
    /// At corner `k`.
    Corner(usize),
    /// On the edge from corner `k` to corner `k + 1` (mod 3).
    Edge(usize),
    Inside,
}

/// [`closest_on_triangle`] with the region the point lies in.
pub(crate) fn closest_on_triangle_at(p: Point, tri: &[Point; 3]) -> (Point, TriangleRegion) {
    let [a, b, c] = *tri;
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return (a, TriangleRegion::Corner(0));
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return (b, TriangleRegion::Corner(1));
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (add(a, scale(ab, v)), TriangleRegion::Edge(0));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return (c, TriangleRegion::Corner(2));
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (add(a, scale(ac, w)), TriangleRegion::Edge(2));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (add(b, scale(sub(c, b), w)), TriangleRegion::Edge(1));
    }
    let denominator = va + vb + vc;
    if denominator.abs() < 1e-300 {
        // A triangle without area: the nearest of its corners.
        let distance2 = |x: Point| {
            let d = sub(p, x);
            dot(d, d)
        };
        let k = (0..3)
            .min_by(|&i, &j| distance2(tri[i]).total_cmp(&distance2(tri[j])))
            .unwrap_or(0);
        return (tri[k], TriangleRegion::Corner(k));
    }
    let v = vb / denominator;
    let w = vc / denominator;
    (
        add(a, add(scale(ab, v), scale(ac, w))),
        TriangleRegion::Inside,
    )
}
