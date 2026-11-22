// SPDX-License-Identifier: MIT
//! Cubic B-spline helpers shared by the fits of the model: the mesh tools'
//! surface fit (`core/model/src/mesh_tools/fit.rs`) and Insert Points'
//! curve fit ([`crate::point_fit`], mitcad#2232). Clamped knot vectors on
//! [0, 1], written in full (`poles + DEGREE + 1` values).

/// The degree of the fitted curves and surfaces.
pub const DEGREE: usize = 3;

/// The full clamped uniform knot vector of `poles` poles of degree 3, on
/// [0, 1].
pub fn knot_vector(poles: usize) -> Vec<f64> {
    let spans = poles - DEGREE;
    let mut knots = vec![0.0; DEGREE];
    for i in 0..=spans {
        knots.push(i as f64 / spans as f64);
    }
    knots.extend(std::iter::repeat_n(1.0, DEGREE));
    knots
}

/// The basis functions' values at `t` (one per pole) of the full clamped
/// knot vector `knots` (uniform or not).
pub fn basis(knots: &[f64], poles: usize, t: f64) -> Vec<f64> {
    let t = t.clamp(0.0, 1.0);
    // The span: the last knot interval that starts at or before t.
    let mut span = DEGREE;
    while span + 1 < poles && knots[span + 1] <= t {
        span += 1;
    }
    let mut n = vec![0.0; poles];
    let mut local = [0.0; DEGREE + 1];
    local[0] = 1.0;
    let mut left = [0.0; DEGREE + 1];
    let mut right = [0.0; DEGREE + 1];
    for j in 1..=DEGREE {
        left[j] = t - knots[span + 1 - j];
        right[j] = knots[span + j] - t;
        let mut saved = 0.0;
        for r in 0..j {
            let denominator = right[r + 1] + left[j - r];
            let temp = if denominator.abs() > 0.0 {
                local[r] / denominator
            } else {
                0.0
            };
            local[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        local[j] = saved;
    }
    for (k, value) in local.iter().enumerate() {
        n[span - DEGREE + k] = *value;
    }
    n
}

/// The Greville abscissa of each pole: where it sits along the curve or
/// surface.
pub fn greville(knots: &[f64], poles: usize) -> Vec<f64> {
    (0..poles)
        .map(|i| knots[i + 1..=i + DEGREE].iter().sum::<f64>() / DEGREE as f64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_basis_sums_to_one_on_non_uniform_knots() {
        let knots = [0.0, 0.0, 0.0, 0.0, 0.1, 0.15, 0.7, 1.0, 1.0, 1.0, 1.0];
        let poles = knots.len() - DEGREE - 1;
        for i in 0..=100 {
            let t = i as f64 / 100.0;
            let sum: f64 = basis(&knots, poles, t).iter().sum();
            assert!((sum - 1.0).abs() < 1e-12, "{t}: {sum}");
        }
        assert_eq!(basis(&knots, poles, 0.0)[0], 1.0);
        assert_eq!(basis(&knots, poles, 1.0)[poles - 1], 1.0);
    }

    #[test]
    fn uniform_knots_and_their_greville_abscissae() {
        assert_eq!(knot_vector(4), vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
        let knots = knot_vector(5);
        assert_eq!(knots.len(), 9);
        let g = greville(&knots, 5);
        assert_eq!(g[0], 0.0);
        assert_eq!(g[4], 1.0);
        assert!(g.windows(2).all(|w| w[0] < w[1]));
    }
}
