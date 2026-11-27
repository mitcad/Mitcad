// SPDX-License-Identifier: MIT
//! Point cloud tools (mitcad#2516): what places a scan on the design,
//! cleans it and builds a mesh of it. Each works on plain points and
//! leaves its input as it is; `document/point_cloud_tools.rs` applies them
//! to the design's point clouds.
//!
//! - [`kd`]: a k-d tree of points: the nearest, the k nearest and those
//!   within a radius;
//! - [`matching`]: rigid matching of points to a target (another cloud's
//!   points, a mesh's triangles, a body's faces through the kernel): the
//!   motion of picked point pairs, a rough start by the points' shape and
//!   the best fit (iterative closest points) refining it. It is the
//!   matching code a mesh Align shares (mitcad#2515);
//! - [`filter`]: outliers (statistical or by radius) and noise taken out,
//!   and a crop by a box or a lasso;
//! - [`reconstruct`]: a closed mesh of a cloud: the points' normals
//!   estimated and oriented, a signed distance from their tangent planes
//!   sampled on a grid and its zero level meshed.

pub mod filter;
pub mod kd;
pub mod matching;
pub mod reconstruct;

#[cfg(test)]
mod tests;

pub(crate) type Vec3 = [f64; 3];

/// The eigenvalues and eigenvectors of a symmetric matrix (cyclic Jacobi):
/// the values ascending, the vectors as the columns of the second matrix in
/// the same order.
pub(crate) fn symmetric_eigen<const N: usize>(mut a: [[f64; N]; N]) -> ([f64; N], [[f64; N]; N]) {
    let mut v = [[0.0; N]; N];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _ in 0..100 {
        let mut off = 0.0;
        let mut diagonal = 0.0;
        for (p, row) in a.iter().enumerate() {
            diagonal += row[p] * row[p];
            off += row[p + 1..].iter().map(|x| x * x).sum::<f64>();
        }
        if off <= 1e-30 * diagonal.max(1e-300) {
            break;
        }
        for p in 0..N {
            for q in p + 1..N {
                if a[p][q] == 0.0 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = if theta >= 0.0 {
                    1.0 / (theta + (theta * theta + 1.0).sqrt())
                } else {
                    -1.0 / (-theta + (theta * theta + 1.0).sqrt())
                };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in a.iter_mut() {
                    let (kp, kq) = (row[p], row[q]);
                    row[p] = c * kp - s * kq;
                    row[q] = s * kp + c * kq;
                }
                let (row_p, row_q) = (a[p], a[q]);
                a[p] = std::array::from_fn(|k| c * row_p[k] - s * row_q[k]);
                a[q] = std::array::from_fn(|k| s * row_p[k] + c * row_q[k]);
                for row in v.iter_mut() {
                    let (kp, kq) = (row[p], row[q]);
                    row[p] = c * kp - s * kq;
                    row[q] = s * kp + c * kq;
                }
            }
        }
    }
    // Ascending by value.
    let mut order: [usize; N] = std::array::from_fn(|i| i);
    order.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    let values = std::array::from_fn(|k| a[order[k]][order[k]]);
    let vectors = std::array::from_fn(|r| std::array::from_fn(|k| v[r][order[k]]));
    (values, vectors)
}

/// The middle of the points and their covariance (not divided by the count).
pub(crate) fn centroid_and_scatter(points: &[Vec3]) -> (Vec3, [[f64; 3]; 3]) {
    let n = points.len().max(1) as f64;
    let mut c = [0.0; 3];
    for p in points {
        for k in 0..3 {
            c[k] += p[k];
        }
    }
    let c = c.map(|v| v / n);
    let mut m = [[0.0; 3]; 3];
    for p in points {
        let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        for r in 0..3 {
            for k in 0..3 {
                m[r][k] += d[r] * d[k];
            }
        }
    }
    (c, m)
}

/// Every `step`-th index of `count` so that at most `max` are taken.
pub(crate) fn thinned(count: usize, max: usize) -> Vec<usize> {
    let step = count.div_ceil(max.max(1)).max(1);
    (0..count).step_by(step).collect()
}
