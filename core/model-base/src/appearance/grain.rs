// SPDX-License-Identifier: MIT
//! The `grain` projection's directions (mitcad#2186,
//! [`Projection::Grain`](super::Projection::Grain)): each face takes the
//! image in a plane of its own, the image's width (u) along the face's
//! longest side, so the grain of a wood runs along each panel's length
//! whatever way the body lies.
//!
//! The rule, which the application's C++ (`app/render/GrainProjection.hpp`,
//! for the shaded view and the render worker) follows exactly:
//!
//! 1. The face's triangles (counter-clockwise seen from outside) give its
//!    area, its area-weighted normal `N` and the second moment of its area
//!    about its centroid (exact for each triangle, so the result does not
//!    depend on how finely the face is meshed).
//! 2. The moment's eigenvectors of the largest and the middle eigenvalue
//!    are u and v: on a planar face the two directions in its plane, u
//!    along its longest extent (a rectangle's longer sides); on a curved
//!    face (a dowel's side) u along its length.
//! 3. u points so that its largest component is positive; v is turned so
//!    that u × v points along `N` (the image is not mirrored seen from
//!    outside).
//! 4. A face about as wide as long (the middle eigenvalue more than
//!    [`SQUARE`] of the largest) has no longest side: u is then the
//!    longest of the X, Y and Z axes as seen in the plane of u and v (the
//!    first of them on a tie), v = (u × v) × u.
//! 5. A point p's coordinates are (p · u, p · v) in mm: faces in one plane
//!    with the same directions continue each other's image. The texture's
//!    rotation and size then apply as in every projection.

use mitcad_vec3::{add, cross, dot, norm, scale, sub};

/// A face whose middle second moment is more than this share of its
/// largest one has no longest side ([module documentation](self), 4.).
pub const SQUARE: f64 = 0.9;

/// The grain's directions of a face: (u, v), unit vectors in the body's
/// coordinates, u along the face's longest side. Triangles counter-
/// clockwise seen from outside; a face without area gives X and Y.
pub fn grain_axes(triangles: impl IntoIterator<Item = [[f64; 3]; 3]>) -> ([f64; 3], [f64; 3]) {
    let mut area = 0.0;
    let mut normal = [0.0; 3];
    let mut first = [0.0; 3];
    // The second moment about the origin, ∫ p pᵀ dA: per triangle
    // A / 12 (a aᵀ + b bᵀ + c cᵀ + s sᵀ) with s = a + b + c.
    let mut second = [[0.0; 3]; 3];
    for [a, b, c] in triangles {
        let n = cross(sub(b, a), sub(c, a));
        let t = 0.5 * norm(n);
        if t.is_nan() || t <= 0.0 {
            continue;
        }
        area += t;
        normal = add(normal, scale(n, 0.5));
        let s = add(add(a, b), c);
        first = add(first, scale(s, t / 3.0));
        for p in [a, b, c, s] {
            for (row, pi) in second.iter_mut().zip(p) {
                for (cell, pj) in row.iter_mut().zip(p) {
                    *cell += t / 12.0 * pi * pj;
                }
            }
        }
    }
    if area.is_nan() || area <= 0.0 {
        return ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    }
    let centroid = scale(first, 1.0 / area);
    let moment: [[f64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| second[i][j] / area - centroid[i] * centroid[j])
    });
    let (values, vectors) = eigen(moment);
    // Largest, middle.
    let mut order = [0usize, 1, 2];
    order.sort_by(|&a, &b| values[b].total_cmp(&values[a]));
    let mut u = vectors[order[0]];
    let mut v = vectors[order[1]];
    if largest_negative(u) {
        u = scale(u, -1.0);
    }
    if dot(cross(u, v), normal) < 0.0 {
        v = scale(v, -1.0);
    }
    if values[order[1]] > SQUARE * values[order[0]] {
        // No longest side: the longest world axis in the plane.
        let w = cross(u, v);
        let mut best: Option<([f64; 3], f64)> = None;
        for axis in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
            let seen = sub(axis, scale(w, dot(axis, w)));
            let length = norm(seen);
            if best.is_none_or(|(_, l)| length > l + 1e-9) {
                best = Some((seen, length));
            }
        }
        if let Some((seen, length)) = best
            && length > 1e-9
        {
            u = scale(seen, 1.0 / length);
            v = cross(w, u);
        }
    }
    (u, v)
}

/// The grain coordinates (mm) of a point: along u and along v.
pub fn grain_point(axes: &([f64; 3], [f64; 3]), p: [f64; 3]) -> [f64; 2] {
    [dot(p, axes.0), dot(p, axes.1)]
}

/// Whether the component of largest magnitude is negative.
fn largest_negative(v: [f64; 3]) -> bool {
    let k = (1..3).fold(0, |k, i| {
        if v[i].abs() > v[k].abs() + 1e-12 {
            i
        } else {
            k
        }
    });
    v[k] < 0.0
}

/// Eigenvalues and unit eigenvectors of a symmetric 3 × 3 matrix (cyclic
/// Jacobi rotations).
fn eigen(m: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut a = m;
    // Columns of `r` are the eigenvectors.
    let mut r = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..32 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        let scale_of = a[0][0].abs() + a[1][1].abs() + a[2][2].abs();
        if off <= 1e-15 * scale_of.max(1e-300) {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q].abs() < 1e-300 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            // a = Jᵀ a J for the rotation J in the (p, q) plane.
            for row in &mut a {
                let akp = row[p];
                let akq = row[q];
                row[p] = c * akp - s * akq;
                row[q] = s * akp + c * akq;
            }
            let (ap, aq) = (a[p], a[q]);
            a[p] = std::array::from_fn(|k| c * ap[k] - s * aq[k]);
            a[q] = std::array::from_fn(|k| s * ap[k] + c * aq[k]);
            for row in &mut r {
                let rp = row[p];
                let rq = row[q];
                row[p] = c * rp - s * rq;
                row[q] = s * rp + c * rq;
            }
        }
    }
    let values = [a[0][0], a[1][1], a[2][2]];
    let vectors = [0, 1, 2].map(|k| [r[0][k], r[1][k], r[2][k]]);
    (values, vectors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rectangle's two triangles, counter-clockwise seen from `normal`'s
    /// side: corner `o`, sides `a` and `b` (a × b along the normal).
    fn rectangle(o: [f64; 3], a: [f64; 3], b: [f64; 3]) -> Vec<[[f64; 3]; 3]> {
        let p = |s: f64, t: f64| add(add(o, scale(a, s)), scale(b, t));
        vec![
            [p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0)],
            [p(0.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)],
        ]
    }

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        norm(sub(a, b)) < 1e-9
    }

    /// A shelf panel 800 × 300 × 18 mm lying flat: its top's and bottom's
    /// grain runs along its length, its long edges' too, and its short
    /// ends' along their own longest side (the panel's width).
    #[test]
    fn the_grain_runs_along_each_faces_longest_side() {
        let (l, w, t) = (800.0, 300.0, 18.0);
        let top = rectangle([0.0, 0.0, t], [l, 0.0, 0.0], [0.0, w, 0.0]);
        let (u, v) = grain_axes(top);
        assert!(close(u, [1.0, 0.0, 0.0]), "{u:?}");
        assert!(close(v, [0.0, 1.0, 0.0]), "{v:?}");
        // The bottom, seen from below: not mirrored (u × v points down).
        let bottom = rectangle([0.0, 0.0, 0.0], [0.0, w, 0.0], [l, 0.0, 0.0]);
        let (u, v) = grain_axes(bottom);
        assert!(close(u, [1.0, 0.0, 0.0]), "{u:?}");
        assert!(close(cross(u, v), [0.0, 0.0, -1.0]), "{v:?}");
        // A long edge (the front, facing -Y).
        let front = rectangle([0.0, 0.0, 0.0], [l, 0.0, 0.0], [0.0, 0.0, t]);
        let (u, v) = grain_axes(front);
        assert!(close(u, [1.0, 0.0, 0.0]), "{u:?}");
        assert!(close(cross(u, v), [0.0, -1.0, 0.0]), "{v:?}");
        // A short end (facing +X): along the width.
        let end = rectangle([l, 0.0, 0.0], [0.0, w, 0.0], [0.0, 0.0, t]);
        let (u, _) = grain_axes(end);
        assert!(close(u, [0.0, 1.0, 0.0]), "{u:?}");
        // The same panel standing upright along Z (a cabinet's side): its
        // big faces take the grain along Z.
        let side = rectangle([0.0, 0.0, 0.0], [0.0, w, 0.0], [0.0, 0.0, l]);
        let (u, _) = grain_axes(side);
        assert!(close(u, [0.0, 0.0, 1.0]), "{u:?}");
    }

    /// A rectangle turned in its plane: the grain follows its length, not
    /// a world axis; a fine mesh of the same face gives the same.
    #[test]
    fn a_turned_face_keeps_its_grain_whatever_its_mesh() {
        let (c, s) = (0.5f64.cos(), 0.5f64.sin());
        let long = [600.0 * c, 600.0 * s, 0.0];
        let across = [-200.0 * s, 200.0 * c, 0.0];
        let (u, v) = grain_axes(rectangle([10.0, 20.0, 5.0], long, across));
        assert!(close(u, [c, s, 0.0]), "{u:?}");
        assert!(close(v, [-s, c, 0.0]), "{v:?}");
        // The same rectangle cut into 10 × 3 cells.
        let mut fine = Vec::new();
        for i in 0..10 {
            for j in 0..3 {
                let o = add(
                    add([10.0, 20.0, 5.0], scale(long, f64::from(i) / 10.0)),
                    scale(across, f64::from(j) / 3.0),
                );
                fine.extend(rectangle(o, scale(long, 0.1), scale(across, 1.0 / 3.0)));
            }
        }
        let (fu, fv) = grain_axes(fine);
        assert!(close(fu, u) && close(fv, v), "{fu:?} {fv:?}");
        // Coordinates in mm along the directions.
        let axes = (u, v);
        let [a, b] = grain_point(&axes, [c * 100.0, s * 100.0, 7.0]);
        assert!((a - 100.0).abs() < 1e-9 && b.abs() < 1e-9, "{a} {b}");
    }

    /// A square face has no longest side: the grain takes the world axis
    /// that lies longest in its plane (X first); a face without area
    /// takes X and Y.
    #[test]
    fn square_faces_take_a_world_axis() {
        let square = rectangle([0.0, 0.0, 0.0], [0.0, 400.0, 0.0], [0.0, 0.0, 400.0]);
        let (u, v) = grain_axes(square);
        // In the YZ plane, facing +X: Y and Z lie in it equally, Y first.
        assert!(close(u, [0.0, 1.0, 0.0]), "{u:?}");
        assert!(close(v, [0.0, 0.0, 1.0]), "{v:?}");
        let flat = rectangle([0.0, 0.0, 0.0], [400.0, 0.0, 0.0], [0.0, 400.0, 0.0]);
        let (u, _) = grain_axes(flat);
        assert!(close(u, [1.0, 0.0, 0.0]), "{u:?}");
        let (u, v) = grain_axes([[[0.0; 3]; 3]]);
        assert_eq!((u, v), ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
    }

    /// A dowel's side (a cylinder along Z, longer than round): the grain
    /// runs along it.
    #[test]
    fn a_dowels_grain_runs_along_its_axis() {
        let (r, h, n) = (10.0, 200.0, 48);
        let mut triangles = Vec::new();
        for i in 0..n {
            let a0 = std::f64::consts::TAU * f64::from(i) / f64::from(n);
            let a1 = std::f64::consts::TAU * f64::from(i + 1) / f64::from(n);
            let p = |a: f64, z: f64| [r * a.cos(), r * a.sin(), z];
            triangles.push([p(a0, 0.0), p(a1, 0.0), p(a1, h)]);
            triangles.push([p(a0, 0.0), p(a1, h), p(a0, h)]);
        }
        let (u, _) = grain_axes(triangles);
        assert!(close(u, [0.0, 0.0, 1.0]), "{u:?}");
    }
}
