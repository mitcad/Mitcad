// SPDX-License-Identifier: MIT
//! Align (mitcad#2515): points (a mesh's vertices, a point cloud) moved
//! onto a target mesh by a rigid move.
//!
//! 1. Picked point pairs (three or more, not all on one line) give the
//!    first move: the rotation and shift that bring the first points of the
//!    pairs nearest to the second in the least-squares sense (Horn's
//!    closed form by unit quaternions). Without pairs the points start
//!    where they are.
//! 2. Best fit (iterative closest points, point to plane): an even sample
//!    of the points is moved, each paired with the target's nearest point,
//!    pairs farther apart than three times the median left out, and the
//!    small rotation and shift that most reduce the distances along the
//!    target's normals solved and applied; until the move is below a
//!    billionth of the size, or [`MAX_ITERATIONS`].
//!
//! The deviation left is measured over all the points: the root mean
//! square and the largest distance to the target.

use mitcad_vec3::{add, cross, dot, norm, scale, sub};

use super::bvh::Bvh;
use super::{Point, TriangleMesh, area_normal, solve};

/// The most best-fit iterations.
pub const MAX_ITERATIONS: usize = 200;

/// The most points a best-fit iteration pairs.
pub const MAX_SAMPLES: usize = 8000;

/// A rigid move: `rotation · p + translation`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rigid {
    /// Rows of the rotation matrix.
    pub rotation: [[f64; 3]; 3],
    pub translation: Point,
}

impl Rigid {
    pub const IDENTITY: Rigid = Rigid {
        rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0; 3],
    };

    pub fn apply(&self, p: Point) -> Point {
        let r = &self.rotation;
        add([dot(r[0], p), dot(r[1], p), dot(r[2], p)], self.translation)
    }

    /// This move, then `next`.
    pub fn then(&self, next: &Rigid) -> Rigid {
        let mut rotation = [[0.0; 3]; 3];
        for (i, row) in rotation.iter_mut().enumerate() {
            for (j, value) in row.iter_mut().enumerate() {
                *value = (0..3)
                    .map(|k| next.rotation[i][k] * self.rotation[k][j])
                    .sum();
            }
        }
        Rigid {
            rotation,
            translation: next.apply(self.translation),
        }
    }

    /// The rotation's angle (radians).
    pub fn angle(&self) -> f64 {
        let r = &self.rotation;
        ((r[0][0] + r[1][1] + r[2][2] - 1.0) / 2.0)
            .clamp(-1.0, 1.0)
            .acos()
    }

    /// The rotation by `angle` about the unit `axis` through `centre`.
    pub fn rotation_about(axis: Point, angle: f64, centre: Point) -> Rigid {
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        let [x, y, z] = axis;
        let rotation = [
            [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
            [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
            [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
        ];
        let turned = Rigid {
            rotation,
            translation: [0.0; 3],
        }
        .apply(centre);
        Rigid {
            rotation,
            translation: sub(centre, turned),
        }
    }

    /// The shift by `by`.
    pub fn shift(by: Point) -> Rigid {
        Rigid {
            translation: by,
            ..Rigid::IDENTITY
        }
    }
}

/// The eigenvalues (ascending) and unit eigenvectors (as columns, one per
/// value) of a symmetric 4 × 4 matrix, by Jacobi rotations.
fn symmetric_eigen4(m: [[f64; 4]; 4]) -> ([f64; 4], [[f64; 4]; 4]) {
    let mut a = m;
    let mut v = [[0.0; 4]; 4];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _ in 0..100 {
        let off: f64 = a
            .iter()
            .enumerate()
            .flat_map(|(p, row)| row.iter().skip(p + 1))
            .map(|x| x.abs())
            .sum();
        if off < 1e-300 {
            break;
        }
        for p in 0..4 {
            for q in p + 1..4 {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = if theta == 0.0 {
                    1.0
                } else {
                    theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt())
                };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in &mut a {
                    let (akp, akq) = (row[p], row[q]);
                    row[p] = c * akp - s * akq;
                    row[q] = s * akp + c * akq;
                }
                let (row_p, row_q) = (a[p], a[q]);
                for k in 0..4 {
                    a[p][k] = c * row_p[k] - s * row_q[k];
                    a[q][k] = s * row_p[k] + c * row_q[k];
                }
                for row in &mut v {
                    let (vp, vq) = (row[p], row[q]);
                    row[p] = c * vp - s * vq;
                    row[q] = s * vp + c * vq;
                }
            }
        }
    }
    let mut order = [0usize, 1, 2, 3];
    order.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    let values = order.map(|i| a[i][i]);
    let vectors = order.map(|i| [v[0][i], v[1][i], v[2][i], v[3][i]]);
    (values, vectors)
}

/// The rigid move that brings the first point of each pair nearest to its
/// second (least squares); refused for fewer than three pairs or pairs
/// whose first points lie on one line.
pub fn from_pairs(pairs: &[(Point, Point)]) -> Result<Rigid, String> {
    if pairs.len() < 3 {
        return Err(format!(
            "aligning by points needs three point pairs or more, got {}",
            pairs.len()
        ));
    }
    let count = pairs.len() as f64;
    let mut cp = [0.0; 3];
    let mut cq = [0.0; 3];
    for (p, q) in pairs {
        cp = add(cp, scale(*p, 1.0 / count));
        cq = add(cq, scale(*q, 1.0 / count));
    }
    let froms: Vec<Point> = pairs.iter().map(|(p, _)| *p).collect();
    let tos: Vec<Point> = pairs.iter().map(|(_, q)| *q).collect();
    for points in [&froms, &tos] {
        let (_, spread) = super::scatter(points);
        let (values, _) = super::symmetric_eigen(spread);
        let extent = super::size(points);
        if values[1] <= 1e-12 * extent * extent * count {
            return Err(
                "the picked points lie on one line; pick three or more that do not".to_owned(),
            );
        }
    }
    let mut s = [[0.0; 3]; 3];
    for (p, q) in pairs {
        let a = sub(*p, cp);
        let b = sub(*q, cq);
        for i in 0..3 {
            for j in 0..3 {
                s[i][j] += a[i] * b[j];
            }
        }
    }
    let [[sxx, sxy, sxz], [syx, syy, syz], [szx, szy, szz]] = s;
    let n = [
        [sxx + syy + szz, syz - szy, szx - sxz, sxy - syx],
        [syz - szy, sxx - syy - szz, sxy + syx, szx + sxz],
        [szx - sxz, sxy + syx, -sxx + syy - szz, syz + szy],
        [sxy - syx, szx + sxz, syz + szy, -sxx - syy + szz],
    ];
    let (_, vectors) = symmetric_eigen4(n);
    let [w, x, y, z] = vectors[3];
    let length = (w * w + x * x + y * y + z * z).sqrt();
    let (w, x, y, z) = (w / length, x / length, y / length, z / length);
    let rotation = [
        [
            w * w + x * x - y * y - z * z,
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
        ],
        [
            2.0 * (x * y + w * z),
            w * w - x * x + y * y - z * z,
            2.0 * (y * z - w * x),
        ],
        [
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            w * w - x * x - y * y + z * z,
        ],
    ];
    let turned = Rigid {
        rotation,
        translation: [0.0; 3],
    }
    .apply(cp);
    Ok(Rigid {
        rotation,
        translation: sub(cq, turned),
    })
}

/// What [`align`] found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aligned {
    /// The move of the points onto the target.
    pub transform: Rigid,
    /// The distances left from the moved points to the target: their root
    /// mean square and the largest (mm).
    pub rms: f64,
    pub max: f64,
    /// Best-fit iterations made (0 without it).
    pub iterations: usize,
    /// The best fit stopped by its move getting small, not by the most
    /// iterations.
    pub converged: bool,
}

/// The points moved onto the target (see the module docs): by the pairs
/// (`from` on the points, `to` on the target), then by best fit when
/// `refine`.
pub fn align(
    points: &[Point],
    target: &TriangleMesh,
    pairs: &[(Point, Point)],
    refine: bool,
) -> Result<Aligned, String> {
    if points.is_empty() {
        return Err("there are no points to align".to_owned());
    }
    if target.triangles.is_empty() {
        return Err("the target has no triangles".to_owned());
    }
    if pairs.is_empty() && !refine {
        return Err("Align needs point pairs, best fit or both".to_owned());
    }
    let mut transform = if pairs.is_empty() {
        Rigid::IDENTITY
    } else {
        from_pairs(pairs)?
    };
    let tree = Bvh::new(target);
    let normals: Vec<Point> = (0..target.triangles.len())
        .map(|t| mitcad_vec3::unit(area_normal(tree.triangle(t))).unwrap_or([0.0; 3]))
        .collect();
    let extent = super::size(points).max(super::size(&target.vertices));
    let step = points.len().div_ceil(MAX_SAMPLES).max(1);
    let sample: Vec<Point> = points.iter().step_by(step).copied().collect();
    let mut iterations = 0;
    let mut converged = !refine;
    if refine {
        for _ in 0..MAX_ITERATIONS {
            iterations += 1;
            let mut matched: Vec<(Point, Point, Point, f64)> = Vec::with_capacity(sample.len());
            for p in &sample {
                let x = transform.apply(*p);
                if let Some(closest) = tree.closest(x) {
                    matched.push((
                        x,
                        closest.point,
                        normals[closest.triangle],
                        closest.distance,
                    ));
                }
            }
            let mut distances: Vec<f64> = matched.iter().map(|m| m.3).collect();
            let middle = distances.len() / 2;
            distances.select_nth_unstable_by(middle, f64::total_cmp);
            let cutoff = (3.0 * distances[middle]).max(1e-9 * extent);
            let kept: Vec<&(Point, Point, Point, f64)> =
                matched.iter().filter(|m| m.3 <= cutoff).collect();
            if kept.len() < 6 {
                return Err(
                    "too few points lie near the target to fit them to it; pick point pairs to \
                     bring them near first"
                        .to_owned(),
                );
            }
            let mut centre = [0.0; 3];
            for m in &kept {
                centre = add(centre, scale(m.0, 1.0 / kept.len() as f64));
            }
            let mut a = vec![0.0; 36];
            let mut b = vec![0.0; 6];
            for (x, q, n, _) in &kept {
                let arm = cross(sub(*x, centre), *n);
                let row = [arm[0], arm[1], arm[2], n[0], n[1], n[2]];
                let e = dot(sub(*x, *q), *n);
                for i in 0..6 {
                    for j in 0..6 {
                        a[i * 6 + j] += row[i] * row[j];
                    }
                    b[i] -= row[i] * e;
                }
            }
            let trace: f64 = (0..6).map(|i| a[i * 6 + i]).sum();
            for i in 0..6 {
                a[i * 6 + i] += 1e-12 * trace.max(1e-300);
            }
            let Some(u) = solve(a, b) else {
                break;
            };
            let r = [u[0], u[1], u[2]];
            let t = [u[3], u[4], u[5]];
            let angle = norm(r);
            let mut step_move = match mitcad_vec3::unit(r) {
                Some(axis) => Rigid::rotation_about(axis, angle, centre),
                None => Rigid::IDENTITY,
            };
            step_move = step_move.then(&Rigid::shift(t));
            transform = transform.then(&step_move);
            if angle * extent < 1e-10 * extent.max(1.0) && norm(t) < 1e-10 * extent.max(1.0) {
                converged = true;
                break;
            }
        }
    }
    let mut sum2 = 0.0;
    let mut max: f64 = 0.0;
    for p in points {
        let x = transform.apply(*p);
        if let Some(closest) = tree.closest(x) {
            sum2 += closest.distance * closest.distance;
            max = max.max(closest.distance);
        }
    }
    Ok(Aligned {
        transform,
        rms: (sum2 / points.len() as f64).sqrt(),
        max,
        iterations,
        converged,
    })
}
