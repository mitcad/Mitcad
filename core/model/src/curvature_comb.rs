// SPDX-License-Identifier: MIT
//! Curvature combs of curves (mitcad#2520): along a curve, at points
//! equally far apart along it, a tooth at right angles to the curve whose
//! length is the curvature there times a scale, on the curve's outer side
//! (away from the centre of curvature); the teeth's tips make the comb's
//! envelope. Where the curve's bend turns from one side to the other (an
//! inflection), the comb changes sides; those points are found to the
//! parameter.
//!
//! Sketch curves ([`sketch_curve_comb`], in sketch coordinates) and the
//! curves of edges ([`curve3_comb`], [`Curve3`]) are evaluated here with
//! their first and second derivatives; a straight curve has no comb. The
//! `curvature_comb` query (`model-api`) places the combs in the design and
//! scales them.

use mitcad_model_base::kernel::Curve3;
use mitcad_sketch::geometry::{Curve2, Nurbs};
use mitcad_vec3::{Vec3, add, add_scaled, cross, distance, dot, norm, scale, sub};

/// Teeth per curve when nothing else is asked for.
pub const DEFAULT_TEETH: usize = 50;
/// The fewest and the most teeth per curve a comb takes.
pub const MIN_TEETH: usize = 2;
pub const MAX_TEETH: usize = 1000;

/// One tooth: where it stands on the curve, the curve's parameter there,
/// the unit direction it points to (at right angles to the curve, away from
/// the centre of curvature; zero where the curve is straight) and the
/// curvature there (1/mm, never negative).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tooth {
    pub t: f64,
    pub point: Vec3,
    pub normal: Vec3,
    pub curvature: f64,
}

impl Tooth {
    /// The tooth's free end at a scale (mm per 1/mm): `curvature × scale`
    /// from the curve.
    pub fn tip(&self, scale_by: f64) -> Vec3 {
        add_scaled(self.point, self.normal, self.curvature * scale_by)
    }

    /// The tooth's length at a scale.
    pub fn length(&self, scale_by: f64) -> f64 {
        self.curvature * scale_by
    }
}

/// Where the curve's bend changes sides: its parameter and point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inflection {
    pub t: f64,
    pub point: Vec3,
}

/// A curve's comb before it is scaled.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Comb {
    /// Along the curve, equally far apart; none for a straight curve.
    pub teeth: Vec<Tooth>,
    /// In the order of the parameter.
    pub inflections: Vec<Inflection>,
    /// A closed curve's last tooth is followed by its first.
    pub closed: bool,
    /// The largest curvature along the curve (1/mm); 0 when straight.
    pub max_curvature: f64,
    /// The curve's length (mm), measured on a fine polyline.
    pub length: f64,
}

impl Comb {
    /// Whether the curve is straight (no teeth).
    pub fn is_straight(&self) -> bool {
        self.teeth.is_empty()
    }

    /// The comb moved by a map of points and one of directions (a sketch's
    /// frame, an occurrence's placement): rigid motions keep the
    /// curvatures, so only the points and the teeth's directions move.
    pub fn moved(&self, point: impl Fn(Vec3) -> Vec3, vector: impl Fn(Vec3) -> Vec3) -> Comb {
        let direction = |v: Vec3| {
            let moved = vector(v);
            let n = norm(moved);
            if n > 0.0 {
                scale(moved, 1.0 / n)
            } else {
                moved
            }
        };
        Comb {
            teeth: self
                .teeth
                .iter()
                .map(|tooth| Tooth {
                    point: point(tooth.point),
                    normal: direction(tooth.normal),
                    ..*tooth
                })
                .collect(),
            inflections: self
                .inflections
                .iter()
                .map(|inflection| Inflection {
                    point: point(inflection.point),
                    ..*inflection
                })
                .collect(),
            ..self.clone()
        }
    }
}

/// The scale at which the longest tooth of `combs` is `size` times a
/// quarter of the diagonal of the box round their curves' points: what a
/// comb is drawn at unless a scale is given. 0 when every curve is
/// straight.
pub fn auto_scale<'a>(combs: impl IntoIterator<Item = &'a Comb>, size: f64) -> f64 {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut most = 0.0_f64;
    for comb in combs {
        most = most.max(comb.max_curvature);
        for tooth in &comb.teeth {
            lo = std::array::from_fn(|i| lo[i].min(tooth.point[i]));
            hi = std::array::from_fn(|i| hi[i].max(tooth.point[i]));
        }
    }
    if most <= 0.0 || !lo[0].is_finite() {
        return 0.0;
    }
    let diagonal = distance(lo, hi);
    // A comb of one point's size (a tiny circle): its own radius.
    let reach = if diagonal > 0.0 {
        diagonal / 4.0
    } else {
        1.0 / most
    };
    size * reach / most
}

/// A curve's point and its first and second derivatives at a parameter.
pub type Derivatives = [Vec3; 3];

/// The curvature (1/mm) of a curve with these derivatives and the unit
/// direction from the curve away from its centre of curvature; none where
/// the curve stands still or is straight.
fn bend(d: &Derivatives) -> (f64, Option<Vec3>) {
    let [_, d1, d2] = *d;
    let speed = norm(d1);
    if speed <= 0.0 {
        return (0.0, None);
    }
    let curvature = norm(cross(d1, d2)) / (speed * speed * speed);
    // d2 without its part along the curve points to the centre.
    let along = scale(d1, dot(d2, d1) / (speed * speed));
    let inward = sub(d2, along);
    let n = norm(inward);
    if n <= 0.0 || curvature <= 0.0 {
        return (0.0, None);
    }
    (curvature, Some(scale(inward, -1.0 / n)))
}

/// The comb of a curve given by its derivatives over `domain`, with
/// `teeth` teeth (clamped to [`MIN_TEETH`]..=[`MAX_TEETH`]) equally far
/// apart along it; `closed`: the curve ends where it starts, and its last
/// tooth is not repeated at the start.
pub fn comb(
    eval: &dyn Fn(f64) -> Derivatives,
    domain: (f64, f64),
    closed: bool,
    teeth: usize,
) -> Comb {
    let teeth = teeth.clamp(MIN_TEETH, MAX_TEETH);
    let (t0, t1) = domain;
    if !t0.is_finite() || !t1.is_finite() || t1 <= t0 {
        return Comb::default();
    }
    // A fine walk along the curve: its length, its largest curvature and
    // where the bend changes sides.
    let fine = (teeth * 8).max(400);
    let at = |k: usize| t0 + (t1 - t0) * k as f64 / fine as f64;
    let samples: Vec<(f64, Derivatives)> = (0..=fine).map(|k| (at(k), eval(at(k)))).collect();
    let mut lengths = Vec::with_capacity(samples.len());
    let mut length = 0.0;
    let mut most = 0.0_f64;
    for (k, (_, d)) in samples.iter().enumerate() {
        if k > 0 {
            length += distance(samples[k - 1].1[0], d[0]);
        }
        lengths.push(length);
        most = most.max(bend(d).0);
    }
    // Straight (to rounding): no comb.
    if length.is_nan() || length <= 0.0 || most * length <= 1e-9 {
        return Comb {
            closed,
            length,
            ..Comb::default()
        };
    }
    let significant = most * 1e-9;
    // The bend's side: d1 × d2 against the last one seen with a bend.
    let mut inflections = Vec::new();
    let mut last: Option<(f64, Vec3)> = None;
    for (t, d) in &samples {
        if bend(d).0 <= significant {
            continue;
        }
        let side = cross(d[1], d[2]);
        if let Some((before, reference)) = last
            && dot(side, reference) < 0.0
        {
            let t = refine(eval, before, *t, reference);
            inflections.push(Inflection {
                t,
                point: eval(t)[0],
            });
        }
        let n = norm(side);
        last = Some((*t, scale(side, 1.0 / n)));
    }
    // The teeth, equally far apart along the curve.
    let count = if closed { teeth } else { teeth - 1 };
    let mut made = Vec::with_capacity(teeth);
    let mut j = 0;
    for k in 0..teeth {
        let target = length * k as f64 / count as f64;
        while j + 1 < lengths.len() - 1 && lengths[j + 1] < target {
            j += 1;
        }
        let (a, b) = (lengths[j], lengths[j + 1]);
        let f = if b > a {
            ((target - a) / (b - a)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let t = samples[j].0 + (samples[j + 1].0 - samples[j].0) * f;
        let d = eval(t);
        let (curvature, outward) = bend(&d);
        made.push(Tooth {
            t,
            point: d[0],
            normal: outward.unwrap_or([0.0; 3]),
            curvature: if outward.is_some() { curvature } else { 0.0 },
        });
    }
    Comb {
        teeth: made,
        inflections,
        closed,
        max_curvature: most,
        length,
    }
}

/// The parameter between `lo` and `hi` where the bend changes sides: where
/// d1 × d2 turns from along `reference` (at `lo`) to against it (at `hi`),
/// by halving.
fn refine(eval: &dyn Fn(f64) -> Derivatives, lo: f64, hi: f64, reference: Vec3) -> f64 {
    let side = |t: f64| {
        let d = eval(t);
        dot(cross(d[1], d[2]), reference)
    };
    let (mut lo, mut hi) = (lo, hi);
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if mid <= lo || mid >= hi {
            break;
        }
        let s = side(mid);
        if s == 0.0 {
            return mid;
        }
        if s > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

fn lift(p: [f64; 2]) -> Vec3 {
    [p[0], p[1], 0.0]
}

/// A sketch curve's comb in sketch coordinates (z = 0): none for a line.
pub fn sketch_curve_comb(curve: &Curve2, teeth: usize) -> Comb {
    if let Curve2::Line { .. } = curve {
        return Comb::default();
    }
    let eval = |t: f64| {
        let [p, d1, d2] = curve.eval(t);
        [lift(p), lift(d1), lift(d2)]
    };
    comb(&eval, curve.domain(), curve.is_closed(), teeth)
}

/// A 3D B-spline's derivatives, as two plane NURBS of the same weights and
/// knots (x and y, then z): a rational curve's coordinates are each their
/// own sum over the same weight function.
struct Spline3 {
    xy: Nurbs,
    z: Nurbs,
}

impl Spline3 {
    fn new(degree: u32, poles: &[[f64; 3]], weights: &[f64], knots: &[f64]) -> Option<Self> {
        let degree = degree as usize;
        if degree == 0 || poles.len() <= degree || knots.len() != poles.len() + degree + 1 {
            return None;
        }
        let make = |control: Vec<[f64; 2]>| Nurbs {
            degree,
            control,
            weights: weights.to_vec(),
            knots: knots.to_vec(),
        };
        Some(Self {
            xy: make(poles.iter().map(|p| [p[0], p[1]]).collect()),
            z: make(poles.iter().map(|p| [p[2], 0.0]).collect()),
        })
    }

    fn eval(&self, t: f64) -> Derivatives {
        let xy = self.xy.eval(t);
        let z = self.z.eval(t);
        std::array::from_fn(|k| [xy[k][0], xy[k][1], z[k][0]])
    }
}

/// The comb of an edge's curve, in the curve's coordinates: none for a
/// point, an empty one for a line.
pub fn curve3_comb(curve: &Curve3, teeth: usize) -> Option<Comb> {
    match curve {
        Curve3::Point(_) => None,
        Curve3::Line { .. } => Some(Comb::default()),
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => {
            let y_axis = cross(*normal, *x_axis);
            let y_axis = match norm(y_axis) {
                n if n > 0.0 => scale(y_axis, 1.0 / n),
                _ => return None,
            };
            let at = |a: f64, b: f64| add(scale(*x_axis, a), scale(y_axis, b));
            let eval = |t: f64| {
                let (s, c) = t.sin_cos();
                [
                    add(*center, at(major * c, minor * s)),
                    at(-major * s, minor * c),
                    at(-major * c, -minor * s),
                ]
            };
            let domain = if *closed {
                (*start, *start + std::f64::consts::TAU)
            } else {
                (*start, *end)
            };
            Some(comb(&eval, domain, *closed, teeth))
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => {
            let spline = Spline3::new(*degree, poles, weights, knots)?;
            let domain = spline.xy.domain();
            let closed = poles.len() > 1
                && distance(spline.eval(domain.0)[0], spline.eval(domain.1)[0])
                    <= 1e-9 * (1.0 + norm(poles[0]));
            Some(comb(&|t| spline.eval(t), domain, closed, teeth))
        }
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => {
            let y_axis = cross(*axis, *x_axis);
            let rise = pitch / std::f64::consts::TAU;
            let eval = |t: f64| {
                let (s, c) = t.sin_cos();
                let round = |a: f64, b: f64| add(scale(*x_axis, a), scale(y_axis, b));
                [
                    add(
                        add(*center, round(radius * c, radius * s)),
                        scale(*axis, rise * t),
                    ),
                    add(round(-radius * s, radius * c), scale(*axis, rise)),
                    round(-radius * c, -radius * s),
                ]
            };
            Some(comb(&eval, (*start, *end), false, teeth))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn a_circle_has_teeth_of_one_length_outside_it() {
        let circle = Curve2::Circle {
            center: [3.0, -2.0],
            radius: 8.0,
            start: 0.0,
        };
        let comb = sketch_curve_comb(&circle, 36);
        assert_eq!(comb.teeth.len(), 36);
        assert!(comb.closed);
        assert!(comb.inflections.is_empty(), "{:?}", comb.inflections);
        for tooth in &comb.teeth {
            assert!(close(tooth.curvature, 1.0 / 8.0, 1e-9), "{tooth:?}");
            assert!(close(tooth.length(40.0), 5.0, 1e-9));
            // Outward: the tip is 8 + 5 from the centre.
            assert!(close(
                distance(tooth.tip(40.0), [3.0, -2.0, 0.0]),
                13.0,
                1e-9
            ));
        }
        // Equally far apart along it.
        let gaps: Vec<f64> = comb
            .teeth
            .windows(2)
            .map(|w| distance(w[0].point, w[1].point))
            .collect();
        for gap in &gaps {
            assert!(close(*gap, gaps[0], 1e-3), "{gaps:?}");
        }
    }

    #[test]
    fn a_line_has_no_comb() {
        let line = Curve2::Line {
            a: [0.0, 0.0],
            b: [10.0, 5.0],
        };
        let comb = sketch_curve_comb(&line, 20);
        assert!(comb.is_straight());
        assert!(comb.inflections.is_empty());
        // Nor a spline whose control points lie on a line.
        let straight = Curve2::Nurbs(Nurbs {
            degree: 3,
            control: vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0]],
            weights: Vec::new(),
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        });
        assert!(sketch_curve_comb(&straight, 20).is_straight());
        assert_eq!(
            curve3_comb(
                &Curve3::Line {
                    start: [0.0; 3],
                    end: [1.0, 2.0, 3.0]
                },
                20
            ),
            Some(Comb::default())
        );
    }

    /// The cubic Bezier (0, 0), (1, 1), (2, −1), (3, 0): x′ = 3 and
    /// x′y″ − y′x″ = 54 (2t − 1), so it bends one way before t = 0.5 and
    /// the other way after.
    fn s_curve() -> Nurbs {
        Nurbs {
            degree: 3,
            control: vec![[0.0, 0.0], [1.0, 1.0], [2.0, -1.0], [3.0, 0.0]],
            weights: Vec::new(),
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        }
    }

    #[test]
    fn an_s_curve_has_one_inflection_where_its_bend_changes_sides() {
        let comb = sketch_curve_comb(&Curve2::Nurbs(s_curve()), 40);
        assert_eq!(comb.inflections.len(), 1, "{:?}", comb.inflections);
        let inflection = comb.inflections[0];
        assert!(close(inflection.t, 0.5, 1e-9), "{inflection:?}");
        assert!(
            distance(inflection.point, [1.5, 0.0, 0.0]) < 1e-9,
            "{inflection:?}"
        );
        // The teeth stand on the outer side: above the curve where it bends
        // down (before the inflection, y″ < 0), below it after.
        let first = comb.teeth.first().unwrap();
        let last = comb.teeth.last().unwrap();
        assert!(
            first.normal[1] > 0.0 && last.normal[1] < 0.0,
            "{first:?} {last:?}"
        );
        // Curvature at t = 0: |x′y″ − y′x″| / |d1|³ = 54 / (3² + 3²)^1.5.
        assert!(
            close(first.curvature, 54.0 / 18f64.powf(1.5), 1e-9),
            "{first:?}"
        );
    }

    #[test]
    fn an_edge_s_curve_in_space_and_a_circle_edge() {
        // The S curve stood up in the XZ plane, as an edge's B-spline.
        let s = s_curve();
        let poles: Vec<[f64; 3]> = s.control.iter().map(|p| [p[0], 5.0, p[1]]).collect();
        let edge = Curve3::BSpline {
            degree: 3,
            poles,
            weights: Vec::new(),
            knots: s.knots.clone(),
        };
        let comb = curve3_comb(&edge, 30).unwrap();
        assert!(!comb.closed);
        assert_eq!(comb.teeth.len(), 30);
        assert_eq!(comb.inflections.len(), 1);
        assert!(close(comb.inflections[0].t, 0.5, 1e-9));
        assert!(distance(comb.inflections[0].point, [1.5, 5.0, 0.0]) < 1e-9);
        // A circle edge of radius 4 tilted out of the XY plane.
        let circle = Curve3::Conic {
            center: [1.0, 2.0, 3.0],
            normal: [0.0, 0.6, 0.8],
            x_axis: [1.0, 0.0, 0.0],
            major: 4.0,
            minor: 4.0,
            start: 0.0,
            end: 0.0,
            closed: true,
        };
        let comb = curve3_comb(&circle, 24).unwrap();
        assert!(comb.closed);
        assert!(comb.inflections.is_empty());
        for tooth in &comb.teeth {
            assert!(close(tooth.length(10.0), 2.5, 1e-9), "{tooth:?}");
            assert!(close(distance(tooth.tip(10.0), [1.0, 2.0, 3.0]), 6.5, 1e-9));
        }
    }

    #[test]
    fn a_helix_bends_one_way_all_along() {
        let helix = Curve3::Helix {
            center: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            radius: 5.0,
            pitch: 2.0,
            start: 0.0,
            end: 4.0 * std::f64::consts::PI,
        };
        let comb = curve3_comb(&helix, 20).unwrap();
        assert!(comb.inflections.is_empty());
        // κ = r / (r² + c²), c the rise per radian.
        let c = 2.0 / std::f64::consts::TAU;
        for tooth in &comb.teeth {
            assert!(
                close(tooth.curvature, 5.0 / (25.0 + c * c), 1e-9),
                "{tooth:?}"
            );
        }
    }

    #[test]
    fn the_auto_scale_makes_the_longest_tooth_a_quarter_of_the_box() {
        let circle = Curve2::Circle {
            center: [0.0, 0.0],
            radius: 10.0,
            start: 0.0,
        };
        let comb = sketch_curve_comb(&circle, 4);
        // Teeth at the four quarter points: a 20 × 20 box.
        let scale_by = auto_scale([&comb], 1.0);
        let diagonal = 20.0 * std::f64::consts::SQRT_2;
        assert!(close(comb.teeth[0].length(scale_by), diagonal / 4.0, 1e-9));
        assert!(close(auto_scale([&comb], 2.0), 2.0 * scale_by, 1e-12));
        assert_eq!(auto_scale([&Comb::default()], 1.0), 0.0);
    }

    #[test]
    fn a_moved_comb_keeps_its_curvatures() {
        let comb = sketch_curve_comb(&Curve2::Nurbs(s_curve()), 10);
        // Into a frame with x along Y and y along Z, at (0, 0, 7).
        let moved = comb.moved(|p| [0.0, p[0], p[1] + 7.0], |v| [0.0, v[0], v[1]]);
        assert_eq!(moved.teeth.len(), comb.teeth.len());
        assert_eq!(moved.teeth[3].curvature, comb.teeth[3].curvature);
        assert!(distance(moved.inflections[0].point, [0.0, 1.5, 7.0]) < 1e-9);
        assert!(close(norm(moved.teeth[0].normal), 1.0, 1e-12));
    }
}
