// SPDX-License-Identifier: MIT
//! Rigid matching of points to a target (mitcad#2516; the matching a mesh
//! Align shares, mitcad#2515): the motion that takes points onto others
//! ([`rigid_fit`], Horn's closed form with unit quaternions), the motion of
//! picked point pairs ([`pairs_motion`]), a rough start by the points'
//! shape ([`shape_starts`]: their middles and main axes matched), and the
//! best fit ([`best_fit`]: iterative closest points) refining a start.
//!
//! A target is anything that tells the nearest point of itself to a place
//! ([`Target`]): another cloud's points ([`PointsTarget`]), a mesh's
//! triangles ([`TrianglesTarget`], exact closest points through a tree of
//! the triangles' boxes) or, in the document, a body's exact faces through
//! the kernel.

use mitcad_vec3::{add, add_scaled, cross, dot, norm, scale, sub};

use super::kd::KdTree;
use super::{Vec3, centroid_and_scatter, symmetric_eigen};
use crate::transform::Transform;

/// The fewest points a best fit works with.
pub const MIN_POINTS: usize = 3;

/// The nearest points of a target.
pub trait Target {
    /// The target's nearest point to each point, in the points' order;
    /// none where the target has none.
    fn nearest(&self, points: &[Vec3]) -> Result<Vec<Option<Vec3>>, String>;
}

/// Another cloud's points as a target.
pub struct PointsTarget {
    tree: KdTree,
}

impl PointsTarget {
    pub fn new(points: Vec<Vec3>) -> Self {
        Self {
            tree: KdTree::new(points),
        }
    }
}

impl Target for PointsTarget {
    fn nearest(&self, points: &[Vec3]) -> Result<Vec<Option<Vec3>>, String> {
        Ok(points
            .iter()
            .map(|&p| self.tree.nearest(p).map(|(i, _)| self.tree.point(i)))
            .collect())
    }
}

/// A node of the tree of triangles' boxes: a leaf holds `count` triangles
/// from `start` of the order, an inner node its two children.
#[derive(Debug, Clone)]
struct Node {
    low: Vec3,
    high: Vec3,
    start: u32,
    count: u32,
    left: u32,
    right: u32,
}

/// The triangles a leaf holds at most.
const LEAF: usize = 4;

/// A mesh's triangles as a target: the exact closest point of the nearest
/// triangle.
pub struct TrianglesTarget {
    vertices: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
    order: Vec<u32>,
    nodes: Vec<Node>,
}

impl TrianglesTarget {
    /// The target of the triangles (indices into `vertices`); triangles
    /// with an index out of range are left out.
    pub fn new(vertices: Vec<Vec3>, triangles: Vec<[u32; 3]>) -> Self {
        let triangles: Vec<[u32; 3]> = triangles
            .into_iter()
            .filter(|t| t.iter().all(|&i| (i as usize) < vertices.len()))
            .collect();
        let centres: Vec<Vec3> = triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| vertices[i as usize]);
                std::array::from_fn(|k| (a[k] + b[k] + c[k]) / 3.0)
            })
            .collect();
        let mut target = Self {
            vertices,
            order: (0..triangles.len()).map(|i| i as u32).collect(),
            triangles,
            nodes: Vec::new(),
        };
        if !target.triangles.is_empty() {
            let n = target.triangles.len();
            target.build(&centres, 0, n);
        }
        target
    }

    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    fn corners(&self, t: u32) -> [Vec3; 3] {
        self.triangles[t as usize].map(|i| self.vertices[i as usize])
    }

    /// Adds the node of `order[lo..hi]` and its children; returns its index.
    fn build(&mut self, centres: &[Vec3], lo: usize, hi: usize) -> u32 {
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        let mut centre_low = [f64::INFINITY; 3];
        let mut centre_high = [f64::NEG_INFINITY; 3];
        for &t in &self.order[lo..hi] {
            for p in self.corners(t) {
                for k in 0..3 {
                    low[k] = low[k].min(p[k]);
                    high[k] = high[k].max(p[k]);
                }
            }
            let c = centres[t as usize];
            for k in 0..3 {
                centre_low[k] = centre_low[k].min(c[k]);
                centre_high[k] = centre_high[k].max(c[k]);
            }
        }
        let index = self.nodes.len() as u32;
        self.nodes.push(Node {
            low,
            high,
            start: lo as u32,
            count: (hi - lo) as u32,
            left: 0,
            right: 0,
        });
        if hi - lo <= LEAF {
            return index;
        }
        let mut axis = 0;
        for k in 1..3 {
            if centre_high[k] - centre_low[k] > centre_high[axis] - centre_low[axis] {
                axis = k;
            }
        }
        let mid = (lo + hi) / 2;
        self.order[lo..hi].select_nth_unstable_by(mid - lo, |&a, &b| {
            centres[a as usize][axis].total_cmp(&centres[b as usize][axis])
        });
        let left = self.build(centres, lo, mid);
        let right = self.build(centres, mid, hi);
        let node = &mut self.nodes[index as usize];
        node.count = 0;
        node.left = left;
        node.right = right;
        index
    }

    /// The closest point of the triangles to `q` and its squared distance.
    pub fn closest(&self, q: Vec3) -> Option<(Vec3, f64)> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best: Option<(Vec3, f64)> = None;
        let mut stack = vec![0u32];
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n as usize];
            if best.is_some_and(|(_, d2)| box_distance2(q, node.low, node.high) >= d2) {
                continue;
            }
            if node.count > 0 {
                let (start, count) = (node.start as usize, node.count as usize);
                for &t in &self.order[start..start + count] {
                    let [a, b, c] = self.corners(t);
                    let p = closest_on_triangle(q, a, b, c);
                    let d2 = norm(sub(p, q)).powi(2);
                    if best.is_none_or(|(_, best2)| d2 < best2) {
                        best = Some((p, d2));
                    }
                }
                continue;
            }
            // The nearer child last, so that it is searched first.
            let (l, r) = (
                &self.nodes[node.left as usize],
                &self.nodes[node.right as usize],
            );
            let (dl, dr) = (
                box_distance2(q, l.low, l.high),
                box_distance2(q, r.low, r.high),
            );
            if dl < dr {
                stack.push(node.right);
                stack.push(node.left);
            } else {
                stack.push(node.left);
                stack.push(node.right);
            }
        }
        best
    }
}

impl Target for TrianglesTarget {
    fn nearest(&self, points: &[Vec3]) -> Result<Vec<Option<Vec3>>, String> {
        Ok(points
            .iter()
            .map(|&p| self.closest(p).map(|(q, _)| q))
            .collect())
    }
}

/// The squared distance from a point to a box (0 inside it).
fn box_distance2(q: Vec3, low: Vec3, high: Vec3) -> f64 {
    (0..3)
        .map(|k| {
            let d = (low[k] - q[k]).max(q[k] - high[k]).max(0.0);
            d * d
        })
        .sum()
}

/// The point of the triangle `a b c` closest to `p` (by the regions of its
/// corners, edges and inside).
pub fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add_scaled(a, ab, d1 / (d1 - d3));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add_scaled(a, ac, d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add_scaled(b, sub(c, b), w);
    }
    let sum = va + vb + vc;
    let inside = add(a, add(scale(ab, vb / sum), scale(ac, vc / sum)));
    if inside.iter().all(|v| v.is_finite()) {
        inside
    } else {
        // A triangle without area: its nearest corner.
        [a, b, c]
            .into_iter()
            .min_by(|x, y| norm(sub(*x, p)).total_cmp(&norm(sub(*y, p))))
            .unwrap_or(a)
    }
}

/// The rigid motion that takes `from[i]` nearest onto `to[i]` in the least
/// squares (Horn's closed form: the unit quaternion of the largest
/// eigenvalue of the pairs' 4 x 4 matrix). None for fewer than one pair.
pub fn rigid_fit(from: &[Vec3], to: &[Vec3]) -> Option<Transform> {
    let n = from.len().min(to.len());
    if n == 0 {
        return None;
    }
    let (from, to) = (&from[..n], &to[..n]);
    let (ca, _) = centroid_and_scatter(from);
    let (cb, _) = centroid_and_scatter(to);
    let mut s = [[0.0; 3]; 3];
    for (a, b) in from.iter().zip(to) {
        let (a, b) = (sub(*a, ca), sub(*b, cb));
        for r in 0..3 {
            for c in 0..3 {
                s[r][c] += a[r] * b[c];
            }
        }
    }
    let (sxx, sxy, sxz) = (s[0][0], s[0][1], s[0][2]);
    let (syx, syy, syz) = (s[1][0], s[1][1], s[1][2]);
    let (szx, szy, szz) = (s[2][0], s[2][1], s[2][2]);
    let m = [
        [sxx + syy + szz, syz - szy, szx - sxz, sxy - syx],
        [syz - szy, sxx - syy - szz, sxy + syx, szx + sxz],
        [szx - sxz, sxy + syx, -sxx + syy - szz, syz + szy],
        [sxy - syx, szx + sxz, syz + szy, -sxx - syy + szz],
    ];
    let (_, vectors) = symmetric_eigen(m);
    let q: [f64; 4] = std::array::from_fn(|r| vectors[r][3]);
    let length = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !(length > 0.0 && length.is_finite()) {
        return None;
    }
    let [w, x, y, z] = q.map(|v| v / length);
    let linear = [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
        ],
        [
            2.0 * (x * y + w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - w * x),
        ],
        [
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ];
    let rotation = Transform {
        linear,
        translation: [0.0; 3],
    };
    let turned = rotation.apply_vector(ca);
    Some(Transform {
        linear,
        translation: sub(cb, turned),
    })
}

/// The root mean square of the distances from the moved points to theirs.
pub fn pairs_rms(motion: &Transform, from: &[Vec3], to: &[Vec3]) -> f64 {
    let n = from.len().min(to.len());
    if n == 0 {
        return 0.0;
    }
    let sum: f64 = from
        .iter()
        .zip(to)
        .map(|(a, b)| norm(sub(motion.apply_point(*a), *b)).powi(2))
        .sum();
    (sum / n as f64).sqrt()
}

/// The motion of picked point pairs (`from[i]` onto `to[i]`): at least
/// three pairs whose first points do not lie on one line, else why not.
pub fn pairs_motion(from: &[Vec3], to: &[Vec3]) -> Result<Transform, String> {
    if from.len() != to.len() {
        return Err("each picked point needs its partner".to_owned());
    }
    if from.len() < 3 {
        return Err(format!(
            "aligning by point pairs needs at least three pairs, got {}",
            from.len()
        ));
    }
    let on_a_line = |points: &[Vec3]| {
        let (_, scatter) = centroid_and_scatter(points);
        let (values, _) = symmetric_eigen(scatter);
        !(values[1] > 1e-12 * values[2].max(1e-300) && values[1] > 1e-18)
    };
    if on_a_line(from) {
        return Err(
            "the points picked on the cloud lie on one line, so they do not fix a turn about it: \
             pick a point off that line"
                .to_owned(),
        );
    }
    if on_a_line(to) {
        return Err(
            "the points picked on the target lie on one line, so they do not fix a turn about \
             it: pick a point off that line"
                .to_owned(),
        );
    }
    rigid_fit(from, to).ok_or_else(|| "the point pairs give no motion".to_owned())
}

/// The rough starts of points matched to target points by their shape: the
/// middles matched and the main axes turned onto each other, each of the
/// 24 turns that map axes onto axes (a shape's axes are told apart only by
/// its spread along them, which can be alike).
pub fn shape_starts(points: &[Vec3], target: &[Vec3]) -> Vec<Transform> {
    if points.is_empty() || target.is_empty() {
        return Vec::new();
    }
    let (ca, sa) = centroid_and_scatter(points);
    let (cb, sb) = centroid_and_scatter(target);
    let (_, va) = symmetric_eigen(sa);
    let (_, vb) = symmetric_eigen(sb);
    let column = |m: &[[f64; 3]; 3], k: usize| -> Vec3 { [m[0][k], m[1][k], m[2][k]] };
    let a: [Vec3; 3] = std::array::from_fn(|k| column(&va, k));
    let b: [Vec3; 3] = std::array::from_fn(|k| column(&vb, k));
    let mut starts = Vec::with_capacity(24);
    const PERMUTATIONS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for perm in PERMUTATIONS {
        for signs in 0..8u32 {
            let sign = |k: usize| if signs & (1 << k) == 0 { 1.0 } else { -1.0 };
            // Axis k of the points goes onto ±axis perm[k] of the target.
            let to: [Vec3; 3] = std::array::from_fn(|k| scale(b[perm[k]], sign(k)));
            // R = sum over k of to_k a_k^T.
            let linear: [[f64; 3]; 3] = std::array::from_fn(|r| {
                std::array::from_fn(|c| (0..3).map(|k| to[k][r] * a[k][c]).sum())
            });
            let determinant = dot(linear[0], cross(linear[1], linear[2]));
            if determinant <= 0.0 {
                continue;
            }
            let rotation = Transform {
                linear,
                translation: [0.0; 3],
            };
            starts.push(Transform {
                linear,
                translation: sub(cb, rotation.apply_vector(ca)),
            });
        }
    }
    starts
}

/// How a best fit runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitOptions {
    /// At most this many rounds.
    pub max_iterations: usize,
    /// Points farther than this from the target (mm) are left out of a
    /// round; none: those farther than the mean distance and three standard
    /// deviations (and a micrometre).
    pub max_distance: Option<f64>,
    /// The rounds stop when the root mean square changes by less than this
    /// share of itself or by less than a micrometre.
    pub tolerance: f64,
}

impl Default for FitOptions {
    fn default() -> Self {
        Self {
            max_iterations: 200,
            max_distance: None,
            tolerance: 1e-7,
        }
    }
}

/// What a best fit found.
#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    /// The motion of the points (the start included).
    pub motion: Transform,
    /// The distances of the points matched, moved: root mean square, mean
    /// and largest (mm).
    pub rms: f64,
    pub mean: f64,
    pub max: f64,
    /// The points matched within the distance of the last round, of the
    /// points given.
    pub matched: usize,
    pub points: usize,
    pub iterations: usize,
    /// The root mean square stopped changing within the rounds.
    pub converged: bool,
}

/// Why a best fit did not run.
#[derive(Debug, Clone, PartialEq)]
pub enum FitError {
    TooFewPoints(usize),
    /// Fewer points than [`MIN_POINTS`] are near the target: `matched` of
    /// `points` within `distance` mm.
    NoOverlap {
        matched: usize,
        points: usize,
        distance: f64,
    },
    Target(String),
    Cancelled,
}

impl std::fmt::Display for FitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewPoints(n) => write!(
                f,
                "a best fit needs at least {MIN_POINTS} points, the cloud has {n}"
            ),
            Self::NoOverlap {
                matched,
                points,
                distance,
            } => write!(
                f,
                "the cloud and the target do not overlap: {matched} of {points} points have a \
                 point of the target within {distance:.3} mm; move the cloud near the target \
                 first or pick point pairs"
            ),
            Self::Target(why) => write!(f, "the target's nearest points: {why}"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// The moved points, the target's nearest points and their distances.
type Measured = (Vec<Vec3>, Vec<Option<Vec3>>, Vec<f64>);

/// The distances of the moved points from the target's nearest ones (NaN
/// where it has none) and those nearest points.
fn distances(
    points: &[Vec3],
    target: &dyn Target,
    motion: &Transform,
) -> Result<Measured, FitError> {
    let moved: Vec<Vec3> = points.iter().map(|p| motion.apply_point(*p)).collect();
    let nearest = target.nearest(&moved).map_err(FitError::Target)?;
    if nearest.len() != moved.len() {
        return Err(FitError::Target(
            "the target answered for a different number of points".to_owned(),
        ));
    }
    let d = moved
        .iter()
        .zip(&nearest)
        .map(|(p, q)| q.map_or(f64::NAN, |q| norm(sub(*p, q))))
        .collect();
    Ok((moved, nearest, d))
}

/// The distance a round takes points within: the given one, else the mean
/// of the distances measured and three standard deviations (at least a
/// micrometre). Not a share of the median: when part of the cloud fits
/// first (the points on a block's top and bottom faces), the median drops
/// to nothing and the rest (the side faces' points, which fix the
/// sideways place) would be left out for good.
fn threshold(d: &[f64], max_distance: Option<f64>) -> f64 {
    if let Some(max) = max_distance {
        return max;
    }
    let finite: Vec<f64> = d.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return 0.0;
    }
    let n = finite.len() as f64;
    let mean = finite.iter().sum::<f64>() / n;
    let deviation = (finite.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
    (mean + 3.0 * deviation).max(1e-3)
}

/// The best fit of the points to the target from `start` (iterative
/// closest points: each round matches each point with the target's nearest
/// point, leaves out those farther than the round's distance and moves the
/// points by the rigid fit of the rest). `progress(round, rounds)` is
/// asked between rounds and stops the fit when it answers false.
pub fn best_fit(
    points: &[Vec3],
    target: &dyn Target,
    start: Transform,
    options: &FitOptions,
    progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<Fit, FitError> {
    if points.len() < MIN_POINTS {
        return Err(FitError::TooFewPoints(points.len()));
    }
    let mut motion = start;
    let mut previous = f64::INFINITY;
    let mut converged = false;
    let mut iterations = 0;
    let rounds = options.max_iterations;
    while iterations < rounds {
        if !progress(iterations, rounds) {
            return Err(FitError::Cancelled);
        }
        let (moved, nearest, d) = distances(points, target, &motion)?;
        let limit = threshold(&d, options.max_distance);
        let mut from = Vec::with_capacity(points.len());
        let mut to = Vec::with_capacity(points.len());
        let mut squares = 0.0;
        for ((p, q), d) in moved.iter().zip(&nearest).zip(&d) {
            if let Some(q) = q
                && *d <= limit
            {
                from.push(*p);
                to.push(*q);
                squares += d * d;
            }
        }
        if from.len() < MIN_POINTS {
            return Err(FitError::NoOverlap {
                matched: from.len(),
                points: points.len(),
                distance: limit,
            });
        }
        let rms = (squares / from.len() as f64).sqrt();
        iterations += 1;
        if (previous - rms).abs() <= (options.tolerance * rms).max(1e-6) {
            converged = true;
            break;
        }
        previous = rms;
        let step = rigid_fit(&from, &to)
            .ok_or_else(|| FitError::Target("the matched points give no motion".to_owned()))?;
        motion = step.after(&motion);
    }
    let (_, nearest, d) = distances(points, target, &motion)?;
    let limit = threshold(&d, options.max_distance);
    let used: Vec<f64> = nearest
        .iter()
        .zip(&d)
        .filter(|(q, d)| q.is_some() && **d <= limit)
        .map(|(_, d)| *d)
        .collect();
    if used.len() < MIN_POINTS {
        return Err(FitError::NoOverlap {
            matched: used.len(),
            points: points.len(),
            distance: limit,
        });
    }
    let n = used.len() as f64;
    Ok(Fit {
        motion,
        rms: (used.iter().map(|d| d * d).sum::<f64>() / n).sqrt(),
        mean: used.iter().sum::<f64>() / n,
        max: used.iter().copied().fold(0.0, f64::max),
        matched: used.len(),
        points: points.len(),
        iterations,
        converged,
    })
}

/// The best fit from each start, of `points` (a sample, for speed) with
/// `rounds` rounds each: the start whose fit is closest (the smallest root
/// mean square of the most points matched).
pub fn best_start(
    points: &[Vec3],
    target: &dyn Target,
    starts: &[Transform],
    rounds: usize,
    progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<Transform, FitError> {
    let options = FitOptions {
        max_iterations: rounds,
        ..FitOptions::default()
    };
    let mut best: Option<(f64, Transform)> = None;
    let mut last_error = None;
    for (i, start) in starts.iter().enumerate() {
        if !progress(i, starts.len()) {
            return Err(FitError::Cancelled);
        }
        match best_fit(points, target, *start, &options, &mut |_, _| true) {
            Ok(fit) => {
                // Matching fewer points counts as farther.
                let share = fit.matched as f64 / fit.points.max(1) as f64;
                let score = fit.rms / share.max(1e-9);
                if best.as_ref().is_none_or(|(s, _)| score < *s) {
                    best = Some((score, fit.motion));
                }
            }
            Err(FitError::Cancelled) => return Err(FitError::Cancelled),
            Err(error) => last_error = Some(error),
        }
    }
    match (best, last_error) {
        (Some((_, motion)), _) => Ok(motion),
        (None, Some(error)) => Err(error),
        (None, None) => Err(FitError::TooFewPoints(points.len())),
    }
}
