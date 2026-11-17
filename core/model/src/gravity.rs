// SPDX-License-Identifier: MIT
//! The direction of gravity (mitcad#1713, Part B) and the hanging slack of
//! a Flex item with a fixed length. Gravity is for flexible items only: a
//! Flex item hangs toward it (its fixed length's slack in every
//! evaluation, and a hanging cable's session, `Document::flex_hang`).
//!
//! The direction is stored in the design: a vector, and what it was taken
//! from (a planar face or a plane: against its normal; an axis: along it),
//! so that it can be shown and taken again. Recompute reads the vector
//! only, so a design recomputes the same however its references move.
//!
//! (The rigid parts that fell and were dragged under gravity were taken
//! out on 15.11.2026; that code is kept on the branch
//! `archive/flex-rigid-gravity`.)

use serde::{Deserialize, Serialize};

use crate::features::{GeomRef, OccurrencePath, is_false};

type Vec3 = [f64; 3];

/// The design's direction of gravity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gravity {
    /// The unit direction gravity pulls in, in the design's coordinates.
    pub direction: Vec3,
    /// What the direction was taken from; None for a direction given as
    /// a vector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<GravitySource>,
}

/// What a direction of gravity was taken from: a planar face, a plane or
/// an axis, on the occurrence path from the root its component was picked
/// at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GravitySource {
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub occurrence: OccurrencePath,
    pub geometry: GeomRef,
    /// The other way: along a plane's normal, against an axis.
    #[serde(default, skip_serializing_if = "is_false")]
    pub reverse: bool,
}

impl Gravity {
    /// Why it cannot be kept, if it cannot.
    pub fn check(&self) -> Result<(), String> {
        unit(self.direction).map(|_| ()).ok_or_else(|| {
            "the direction of gravity must be a finite vector that is not zero".into()
        })
    }
}

/// A unit vector along `v`, None for a zero or not finite one.
pub fn unit_vector(v: Vec3) -> Option<Vec3> {
    unit(v)
}

pub(crate) fn unit(v: Vec3) -> Option<Vec3> {
    let length = mitcad_vec3::norm(v);
    (length.is_finite() && length > 1e-12).then(|| mitcad_vec3::scale(v, 1.0 / length))
}

/// The points a hanging stretch of a Flex item passes through between
/// `a` and `b` (not themselves), sagging toward `down` (a unit vector) so
/// that the curve through them is about `length` long: in the plane of the
/// chord and gravity, its sag along the part of `down` that is square to
/// the chord, bent no tighter than `bend_radius` ([`sag_curve`]). `points`
/// of them, evenly along the curve. None when the stretch cannot hang
/// (gravity along the chord, or no slack).
pub(crate) fn hanging(
    a: Vec3,
    b: Vec3,
    down: Vec3,
    length: f64,
    points: usize,
    bend_radius: Option<f64>,
) -> Option<Vec<Vec3>> {
    use mitcad_vec3::{add, dot, norm, scale, sub};
    let chord = sub(b, a);
    let span = norm(chord);
    if span.is_nan() || span <= 1e-9 || length <= span * (1.0 + 1e-9) {
        return None;
    }
    let along = scale(chord, 1.0 / span);
    let sideways = sub(down, scale(along, dot(down, along)));
    if norm(sideways) < 1e-3 {
        return None;
    }
    let n = unit(sideways)?;
    // In the plane: x along the chord, y down; the shape of a sag `h`.
    let curve = |h: f64| sag_curve(span, h, bend_radius);
    let length_of = |line: &[[f64; 2]]| -> f64 {
        line.windows(2)
            .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
            .sum()
    };
    let (mut low, mut high) = (0.0, length);
    for _ in 0..60 {
        let mid = 0.5 * (low + high);
        if length_of(&curve(mid)) < length {
            low = mid;
        } else {
            high = mid;
        }
    }
    let line = curve(0.5 * (low + high));
    // Points evenly along it.
    let total = length_of(&line);
    let mut out = Vec::with_capacity(points);
    let mut walked = 0.0;
    let mut k = 1;
    for w in line.windows(2) {
        let step = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        while k <= points && walked + step >= total * k as f64 / (points + 1) as f64 {
            let t = if step > 0.0 {
                (total * k as f64 / (points + 1) as f64 - walked) / step
            } else {
                0.0
            };
            let x = w[0][0] + t * (w[1][0] - w[0][0]);
            let y = w[0][1] + t * (w[1][1] - w[0][1]);
            out.push(add(add(a, scale(along, x)), scale(n, y)));
            k += 1;
        }
        walked += step;
    }
    (out.len() == points).then_some(out)
}

/// A hanging stretch over a chord of `span` with the sag `h`, as a
/// polyline in its plane (x along the chord, y down): a parabola while its
/// bottom bends no tighter than `bend_radius`, else straight sides tangent
/// to an arc of that radius at the bottom (a loop that cannot bend
/// tighter).
fn sag_curve(span: f64, h: f64, bend_radius: Option<f64>) -> Vec<[f64; 2]> {
    const STEPS: usize = 64;
    let parabola = || {
        (0..=STEPS)
            .map(|k| {
                let x = span * k as f64 / STEPS as f64;
                [x, 4.0 * h * x * (span - x) / (span * span)]
            })
            .collect()
    };
    let Some(r) = bend_radius.filter(|r| *r > 0.0) else {
        return parabola();
    };
    // The parabola's bottom bends with the radius span²/(8h).
    if h <= 0.0 || span * span / (8.0 * h) >= r {
        return parabola();
    }
    let center = [0.5 * span, h - r];
    let v = [-center[0], -center[1]];
    let d = v[0].hypot(v[1]);
    if d <= r {
        return parabola();
    }
    let alpha = (r / d).acos();
    let base = v[1].atan2(v[0]);
    let tangent = |angle: f64| [center[0] + r * angle.cos(), center[1] + r * angle.sin()];
    let (t1, t2) = (tangent(base + alpha), tangent(base - alpha));
    let left = if t1[0] <= t2[0] { t1 } else { t2 };
    // From the left tangent point down round the bottom (angle π/2, y
    // down) to its mirror image.
    let mut from = (left[1] - center[1]).atan2(left[0] - center[0]);
    if from < std::f64::consts::FRAC_PI_2 {
        from += std::f64::consts::TAU;
    }
    let to = std::f64::consts::PI - from;
    let mut line = vec![[0.0, 0.0]];
    for k in 0..=STEPS {
        let angle = from + (to - from) * k as f64 / STEPS as f64;
        line.push(tangent(angle));
    }
    line.push([span, 0.0]);
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stretch_hangs_below_its_chord_with_its_length() {
        let points = hanging(
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            120.0,
            7,
            None,
        )
        .expect("it hangs");
        assert_eq!(points.len(), 7);
        // The middle one lowest, below the chord, and the polyline about
        // as long as asked (the points are few: within 2 %).
        assert!(
            points[3][2] < -20.0 && points[3][2] > -45.0,
            "{:?}",
            points[3]
        );
        assert!(points.iter().all(|p| p[2] < 0.0 && p[1].abs() < 1e-12));
        let mut all = vec![[0.0, 0.0, 0.0]];
        all.extend(points);
        all.push([100.0, 0.0, 0.0]);
        let length: f64 = all
            .windows(2)
            .map(|w| mitcad_vec3::distance(w[0], w[1]))
            .sum();
        assert!((length - 120.0).abs() < 2.4, "{length}");
    }

    #[test]
    fn a_stretch_without_slack_or_along_gravity_does_not_hang() {
        let (a, b) = ([0.0, 0.0, 0.0], [0.0, 0.0, -100.0]);
        assert!(hanging(a, b, [0.0, 0.0, -1.0], 150.0, 5, None).is_none());
        assert!(hanging(a, [100.0, 0.0, 0.0], [0.0, 0.0, -1.0], 99.0, 5, None).is_none());
    }

    #[test]
    fn a_long_slack_hangs_in_a_loop_no_tighter_than_the_bend_radius() {
        // 300 mm over a chord of 100: a parabola would bend at its bottom
        // far tighter than 30 mm.
        let line = sag_curve(100.0, 120.0, Some(30.0));
        for three in line.windows(3) {
            let [a, b, c] = [three[0], three[1], three[2]];
            let (ab, bc, ca) = (
                (b[0] - a[0]).hypot(b[1] - a[1]),
                (c[0] - b[0]).hypot(c[1] - b[1]),
                (a[0] - c[0]).hypot(a[1] - c[1]),
            );
            let area2 = ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
            if area2 > 1e-9 {
                let radius = ab * bc * ca / (2.0 * area2);
                assert!(radius > 30.0 * 0.999, "bent to {radius} at {b:?}");
            }
        }
        let points = hanging(
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            300.0,
            9,
            Some(30.0),
        )
        .expect("it hangs");
        let mut all = vec![[0.0, 0.0, 0.0]];
        all.extend(points);
        all.push([100.0, 0.0, 0.0]);
        let length: f64 = all
            .windows(2)
            .map(|w| mitcad_vec3::distance(w[0], w[1]))
            .sum();
        assert!((length - 300.0).abs() < 9.0, "{length}");
    }
}
