// SPDX-License-Identifier: MIT
//! A k-d tree of points (mitcad#2516): the nearest point to a place, the k
//! nearest and those within a radius. The tree is implicit: the points'
//! indices are ordered so that each range's middle index splits it along
//! the widest axis of the range's points.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use super::Vec3;

/// A k-d tree of the points it holds.
#[derive(Debug, Clone)]
pub struct KdTree {
    points: Vec<Vec3>,
    /// The points' indices in the tree's order.
    order: Vec<u32>,
    /// The axis each range's middle splits along, by its place in `order`.
    axes: Vec<u8>,
}

/// A point found, by its squared distance (the farthest first in a heap).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Found(f64, u32);

impl Eq for Found {}

impl PartialOrd for Found {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Found {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0).then(self.1.cmp(&other.1))
    }
}

fn distance2(a: Vec3, b: Vec3) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

impl KdTree {
    /// The tree of the points (at most `u32::MAX` of them).
    pub fn new(points: Vec<Vec3>) -> Self {
        let n = points.len();
        let mut order: Vec<u32> = (0..n).map(|i| i as u32).collect();
        let mut axes = vec![0u8; n];
        build(&points, &mut order, &mut axes, 0, n);
        Self {
            points,
            order,
            axes,
        }
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    pub fn point(&self, i: usize) -> Vec3 {
        self.points[i]
    }

    /// The nearest point's index and its distance; none of an empty tree.
    pub fn nearest(&self, q: Vec3) -> Option<(usize, f64)> {
        self.nearest_k(q, 1).into_iter().next()
    }

    /// The `k` nearest points' indices with their distances, the nearest
    /// first.
    pub fn nearest_k(&self, q: Vec3, k: usize) -> Vec<(usize, f64)> {
        if k == 0 || self.points.is_empty() {
            return Vec::new();
        }
        let mut heap = BinaryHeap::with_capacity(k + 1);
        self.search_k(q, k, 0, self.points.len(), &mut heap);
        let mut found = heap.into_sorted_vec();
        found.truncate(k);
        found
            .into_iter()
            .map(|Found(d2, i)| (i as usize, d2.sqrt()))
            .collect()
    }

    fn search_k(&self, q: Vec3, k: usize, lo: usize, hi: usize, heap: &mut BinaryHeap<Found>) {
        if lo >= hi {
            return;
        }
        let mid = (lo + hi) / 2;
        let i = self.order[mid];
        let p = self.points[i as usize];
        let d2 = distance2(p, q);
        if heap.len() < k {
            heap.push(Found(d2, i));
        } else if heap.peek().is_some_and(|top| d2 < top.0) {
            heap.pop();
            heap.push(Found(d2, i));
        }
        if hi - lo == 1 {
            return;
        }
        let axis = usize::from(self.axes[mid]);
        let diff = q[axis] - p[axis];
        let (near, far) = if diff < 0.0 {
            ((lo, mid), (mid + 1, hi))
        } else {
            ((mid + 1, hi), (lo, mid))
        };
        self.search_k(q, k, near.0, near.1, heap);
        if heap.len() < k || heap.peek().is_some_and(|top| diff * diff < top.0) {
            self.search_k(q, k, far.0, far.1, heap);
        }
    }

    /// The indices of the points within `radius` of `q` (in no order).
    pub fn within(&self, q: Vec3, radius: f64) -> Vec<usize> {
        let mut out = Vec::new();
        if radius >= 0.0 {
            self.search_within(q, radius * radius, 0, self.points.len(), &mut out);
        }
        out
    }

    fn search_within(&self, q: Vec3, r2: f64, lo: usize, hi: usize, out: &mut Vec<usize>) {
        if lo >= hi {
            return;
        }
        let mid = (lo + hi) / 2;
        let i = self.order[mid] as usize;
        let p = self.points[i];
        if distance2(p, q) <= r2 {
            out.push(i);
        }
        if hi - lo == 1 {
            return;
        }
        let axis = usize::from(self.axes[mid]);
        let diff = q[axis] - p[axis];
        if diff <= 0.0 || diff * diff <= r2 {
            self.search_within(q, r2, lo, mid, out);
        }
        if diff >= 0.0 || diff * diff <= r2 {
            self.search_within(q, r2, mid + 1, hi, out);
        }
    }
}

/// Orders `order[lo..hi]` as the tree's range: its middle the median along
/// the range's widest axis, the smaller before it, the larger after.
fn build(points: &[Vec3], order: &mut [u32], axes: &mut [u8], lo: usize, hi: usize) {
    if hi - lo <= 1 {
        return;
    }
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for &i in &order[lo..hi] {
        let p = points[i as usize];
        for k in 0..3 {
            low[k] = low[k].min(p[k]);
            high[k] = high[k].max(p[k]);
        }
    }
    let mut axis = 0;
    for k in 1..3 {
        if high[k] - low[k] > high[axis] - low[axis] {
            axis = k;
        }
    }
    let mid = (lo + hi) / 2;
    order[lo..hi].select_nth_unstable_by(mid - lo, |&a, &b| {
        points[a as usize][axis].total_cmp(&points[b as usize][axis])
    });
    axes[mid] = axis as u8;
    build(points, order, axes, lo, mid);
    build(points, order, axes, mid + 1, hi);
}
