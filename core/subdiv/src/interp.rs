// SPDX-License-Identifier: MIT
//! Cubic B-spline interpolation at evenly spaced parameters, for the
//! patches of the limit surface that are not exactly bicubic.
//!
//! `spans + 1` points at the parameters `i / spans` are interpolated by a
//! clamped cubic B-spline of `spans + 1` poles whose inner knots are the
//! means of three successive parameters (`2 / spans` to `(spans - 2) /
//! spans`). Every cubic polynomial is such a spline, and the interpolant is
//! unique, so points on a cubic come back as that cubic exactly. A surface
//! ([`Interpolator::surface`]) interpolates rows, then columns: its
//! boundary curves are the curves through the boundary's points alone,
//! which is what lets two patches that share a boundary's points share the
//! curve.

use mitcad_vec3::{Vec3, add, scale};

/// The B-spline basis functions of degree `degree` over the full knot
/// vector `knots` at `t`: one value for each pole.
pub fn basis(knots: &[f64], degree: usize, t: f64) -> Vec<f64> {
    let count = knots.len() - degree - 1;
    let mut values = vec![0.0; count];
    // The span: knots[span] <= t < knots[span + 1], the last one for the
    // end of the range.
    let span = if t >= knots[count] {
        count - 1
    } else {
        (degree..count)
            .rev()
            .find(|&k| knots[k] <= t)
            .unwrap_or(degree)
    };
    let mut n = vec![0.0; degree + 1];
    let mut left = vec![0.0; degree + 1];
    let mut right = vec![0.0; degree + 1];
    n[0] = 1.0;
    for j in 1..=degree {
        left[j] = t - knots[span + 1 - j];
        right[j] = knots[span + j] - t;
        let mut saved = 0.0;
        for r in 0..j {
            let denominator = right[r + 1] + left[j - r];
            let temp = if denominator == 0.0 {
                0.0
            } else {
                n[r] / denominator
            };
            n[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        n[j] = saved;
    }
    for (r, value) in n.iter().enumerate() {
        values[span - degree + r] = *value;
    }
    values
}

/// The inverse of a square matrix by Gauss-Jordan elimination with
/// partial pivoting; None when it is singular.
fn inverse(mut a: Vec<Vec<f64>>) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    let mut inv: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect();
    for col in 0..n {
        let pivot = (col..n).max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))?;
        if a[pivot][col].abs() < 1e-14 {
            return None;
        }
        a.swap(col, pivot);
        inv.swap(col, pivot);
        let d = a[col][col];
        for x in a[col].iter_mut() {
            *x /= d;
        }
        for x in inv[col].iter_mut() {
            *x /= d;
        }
        let pivot_a = a[col].clone();
        let pivot_inv = inv[col].clone();
        for row in 0..n {
            if row == col {
                continue;
            }
            let f = a[row][col];
            if f == 0.0 {
                continue;
            }
            for (x, p) in a[row].iter_mut().zip(&pivot_a) {
                *x -= f * p;
            }
            for (x, p) in inv[row].iter_mut().zip(&pivot_inv) {
                *x -= f * p;
            }
        }
    }
    Some(inv)
}

/// Interpolation of `spans + 1` evenly spaced points (see the module).
#[derive(Debug, Clone)]
pub struct Interpolator {
    spans: usize,
    /// The full knot vector.
    knots: Vec<f64>,
    /// The inverse of the collocation matrix.
    inverse: Vec<Vec<f64>>,
}

impl Interpolator {
    /// For `spans` of 3 or more.
    pub fn new(spans: usize) -> Self {
        let spans = spans.max(3);
        let mut knots = vec![0.0; 4];
        knots.extend((2..=spans - 2).map(|k| k as f64 / spans as f64));
        knots.extend([1.0; 4]);
        let matrix: Vec<Vec<f64>> = (0..=spans)
            .map(|i| basis(&knots, 3, i as f64 / spans as f64))
            .collect();
        let inverse = inverse(matrix).expect("the collocation matrix of even parameters");
        Self {
            spans,
            knots,
            inverse,
        }
    }

    /// How many points (and poles) along a side.
    pub fn count(&self) -> usize {
        self.spans + 1
    }

    /// The distinct knots and their multiplicities.
    pub fn knots(&self) -> (Vec<f64>, Vec<u32>) {
        let mut knots: Vec<f64> = Vec::new();
        let mut mults: Vec<u32> = Vec::new();
        for &k in &self.knots {
            match knots.last() {
                Some(&last) if last == k => *mults.last_mut().expect("a knot") += 1,
                _ => {
                    knots.push(k);
                    mults.push(1);
                }
            }
        }
        (knots, mults)
    }

    /// The poles of the curve through the points (`count` of them).
    pub fn curve(&self, points: &[Vec3]) -> Vec<Vec3> {
        self.inverse
            .iter()
            .map(|row| {
                row.iter()
                    .zip(points)
                    .fold([0.0; 3], |sum, (w, p)| add(sum, scale(*p, *w)))
            })
            .collect()
    }

    /// The poles of the surface through a grid of points, `count` by
    /// `count`, the point at (i, j) (i along u) at `i * count + j`; the
    /// poles in the same order.
    pub fn surface(&self, points: &[Vec3]) -> Vec<Vec3> {
        let n = self.count();
        // Along u for each j.
        let mut rows = vec![[0.0; 3]; n * n];
        for j in 0..n {
            let column: Vec<Vec3> = (0..n).map(|i| points[i * n + j]).collect();
            for (i, pole) in self.curve(&column).into_iter().enumerate() {
                rows[i * n + j] = pole;
            }
        }
        // Then along v for each i.
        let mut poles = vec![[0.0; 3]; n * n];
        for i in 0..n {
            let line: Vec<Vec3> = (0..n).map(|j| rows[i * n + j]).collect();
            for (j, pole) in self.curve(&line).into_iter().enumerate() {
                poles[i * n + j] = pole;
            }
        }
        poles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The point of a clamped cubic B-spline at `t`.
    fn point(knots: &[f64], poles: &[Vec3], t: f64) -> Vec3 {
        basis(knots, 3, t)
            .iter()
            .zip(poles)
            .fold([0.0; 3], |sum, (w, p)| add(sum, scale(*p, *w)))
    }

    #[test]
    fn the_basis_is_a_partition_of_unity() {
        let interpolator = Interpolator::new(8);
        for k in 0..=40 {
            let t = k as f64 / 40.0;
            let sum: f64 = basis(&interpolator.knots, 3, t).iter().sum();
            assert!((sum - 1.0).abs() < 1e-14, "{t}: {sum}");
        }
        let (knots, mults) = interpolator.knots();
        assert_eq!(knots.len(), 7);
        assert_eq!(mults, [4, 1, 1, 1, 1, 1, 4]);
        assert_eq!(mults.iter().sum::<u32>() as usize, interpolator.count() + 4);
    }

    /// Points of a cubic come back as that cubic, between them too.
    #[test]
    fn a_cubic_is_reproduced() {
        let interpolator = Interpolator::new(8);
        let cubic = |t: f64| [t, 2.0 * t * t - t, 3.0 * t * t * t - 1.5 * t + 0.25];
        let points: Vec<Vec3> = (0..=8).map(|i| cubic(i as f64 / 8.0)).collect();
        let poles = interpolator.curve(&points);
        for k in 0..=50 {
            let t = k as f64 / 50.0;
            let (a, b) = (point(&interpolator.knots, &poles, t), cubic(t));
            assert!(
                (0..3).all(|c| (a[c] - b[c]).abs() < 1e-12),
                "{t}: {a:?} {b:?}"
            );
        }
    }

    /// A surface's boundary is the curve through its boundary points.
    #[test]
    fn a_surface_boundary_is_its_boundary_curve() {
        let interpolator = Interpolator::new(4);
        let n = interpolator.count();
        let points: Vec<Vec3> = (0..n * n)
            .map(|k| {
                let (i, j) = ((k / n) as f64, (k % n) as f64);
                [i, j, (i * 0.7).sin() + (j * 1.3).cos()]
            })
            .collect();
        let poles = interpolator.surface(&points);
        let boundary: Vec<Vec3> = (0..n).map(|i| points[i * n]).collect();
        let curve = interpolator.curve(&boundary);
        for i in 0..n {
            let (a, b) = (poles[i * n], curve[i]);
            assert!((0..3).all(|c| (a[c] - b[c]).abs() < 1e-12), "{i}");
        }
    }
}
