// SPDX-License-Identifier: MIT
//! Conic curves of a sketch (mitcad#2236): a curve from a start to an end
//! point whose end tangents meet at a shoulder point (the apex), shaped by
//! Rho.
//!
//! A conic is exactly the rational quadratic Bezier with the control points
//! start, apex and end and the weights 1, `w`, 1, where `w = rho / (1 -
//! rho)`: Rho 0.5 (`w` 1) is a parabola, less an elliptical arc, more a
//! hyperbolic one. Its end tangents are the lines to the apex, which is
//! an ordinary sketch point: constraints and dimensions hold it as any
//! other point. Rho stays a number of its own (not a solver unknown); a
//! sketch can let it follow a parameter
//! ([`crate::def::SketchDef::conic_rhos`]).
//!
//! A piece of a conic is again a conic on the same curve: the piece's end
//! tangents meet at a new apex and it has a Rho of its own ([`piece`]).
//! That is how Trim, Break, Split and Extend keep a conic.

use crate::edit::{PointInput, SketchEdit};
use crate::geometry::{Nurbs, P2, add, cross, dist, norm, scale, sub};
use crate::ids::EntityUid;
use crate::values::ValueInput;
use crate::{ConicRho, Entity, EntityKind};
use mitcad_expr::Unit;
use mitcad_model_base::internal::values::resolve_value_as;

/// Rho a conic tool starts with: a parabola.
pub const DEFAULT_RHO: f64 = 0.5;

/// The middle weight of a conic of `rho`: `rho / (1 - rho)`.
pub fn weight(rho: f64) -> f64 {
    rho / (1.0 - rho)
}

/// The Rho of a conic whose middle weight (with end weights 1) is `w`.
pub fn rho_of_weight(w: f64) -> f64 {
    w / (1.0 + w)
}

/// Refuses a Rho outside the open range (0, 1), in words.
pub fn check_rho(rho: f64) -> Result<(), String> {
    if rho.is_finite() && rho > 0.0 && rho < 1.0 {
        Ok(())
    } else {
        Err(format!(
            "Rho must be between 0 and 1 (0.5 a parabola, less an ellipse, more a hyperbola), got {rho}"
        ))
    }
}

/// Refuses the points of a conic that make none: two of them at one place,
/// or the apex on the line through the ends (the end tangents then do not
/// meet in a shoulder).
pub fn check_points(start: P2, end: P2, apex: P2) -> Result<(), String> {
    let tolerance = 1e-9;
    if dist(start, end) <= tolerance {
        return Err("the conic's start and end are at the same place".to_owned());
    }
    if dist(start, apex) <= tolerance || dist(end, apex) <= tolerance {
        return Err("the conic's shoulder is at one of its ends".to_owned());
    }
    let chord = sub(end, start);
    let offset = cross(chord, sub(apex, start)).abs() / norm(chord);
    if offset <= tolerance * norm(chord).max(1.0) {
        return Err("the conic's shoulder is on the line through its ends".to_owned());
    }
    Ok(())
}

/// [`check_points`] and [`check_rho`].
pub fn check(start: P2, end: P2, apex: P2, rho: f64) -> Result<(), String> {
    check_rho(rho)?;
    check_points(start, end, apex)
}

/// The conic as a rational quadratic B-spline (one Bezier span on [0, 1]).
pub fn nurbs(start: P2, apex: P2, end: P2, rho: f64) -> Nurbs {
    Nurbs {
        degree: 2,
        control: vec![start, apex, end],
        weights: vec![1.0, weight(rho), 1.0],
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
    }
}

/// The point of the conic at parameter `t` (any real `t` where the curve's
/// denominator is not zero: the conic's own continuation outside [0, 1]).
pub fn point(start: P2, apex: P2, end: P2, rho: f64, t: f64) -> Option<P2> {
    let w = weight(rho);
    let (b0, b1, b2) = ((1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t) * w, t * t);
    let d = b0 + b1 + b2;
    if d.abs() <= 1e-15 {
        return None;
    }
    Some(scale(
        add(add(scale(start, b0), scale(apex, b1)), scale(end, b2)),
        1.0 / d,
    ))
}

/// A conic by its points and Rho.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConicPiece {
    pub start: P2,
    pub apex: P2,
    pub end: P2,
    pub rho: f64,
}

impl ConicPiece {
    /// The conic a solved curve is: one rational quadratic Bezier span on
    /// [0, 1] (as the solve makes a conic), its middle weight scaled to end
    /// weights 1. None for any other curve.
    pub fn of_curve(curve: &crate::geometry::Curve2) -> Option<Self> {
        let crate::geometry::Curve2::Nurbs(n) = curve else {
            return None;
        };
        if n.degree != 2 || n.control.len() != 3 || n.domain() != (0.0, 1.0) {
            return None;
        }
        let w = |i: usize| n.weights.get(i).copied().unwrap_or(1.0);
        let middle = w(1) / (w(0) * w(2)).sqrt();
        Some(Self {
            start: n.control[0],
            apex: n.control[1],
            end: n.control[2],
            rho: rho_of_weight(middle),
        })
    }

    /// [`piece`] of this conic.
    pub fn piece(&self, t0: f64, t1: f64) -> Option<Self> {
        piece(self.start, self.apex, self.end, self.rho, t0, t1)
    }

    /// [`point`] of this conic.
    pub fn point(&self, t: f64) -> Option<P2> {
        point(self.start, self.apex, self.end, self.rho, t)
    }

    /// The conic as a curve.
    pub fn curve(&self) -> crate::geometry::Curve2 {
        crate::geometry::Curve2::Nurbs(nurbs(self.start, self.apex, self.end, self.rho))
    }
}

/// The piece of the conic between parameters `t0 < t1` as a conic of its
/// own on the same curve: its ends the conic's points there, its apex where
/// its end tangents meet, its own Rho. The parameters may lie outside
/// [0, 1] (the conic's own continuation, for Extend). None where the piece
/// is no conic with a shoulder between its ends: it reaches the curve's
/// asymptote (a hyperbola) or turns through half an ellipse or more.
pub fn piece(start: P2, apex: P2, end: P2, rho: f64, t0: f64, t1: f64) -> Option<ConicPiece> {
    if t1 - t0 <= 1e-12 {
        return None;
    }
    let w = weight(rho);
    // Homogeneous control points (x w, y w, w) and their blossom.
    let q = [
        [start[0], start[1], 1.0],
        [apex[0] * w, apex[1] * w, w],
        [end[0], end[1], 1.0],
    ];
    let blossom = |u: f64, v: f64| -> [f64; 3] {
        let c0 = (1.0 - u) * (1.0 - v);
        let c1 = (1.0 - u) * v + u * (1.0 - v);
        let c2 = u * v;
        let mut out = [0.0; 3];
        for (k, o) in out.iter_mut().enumerate() {
            *o = c0 * q[0][k] + c1 * q[1][k] + c2 * q[2][k];
        }
        out
    };
    let (a, m, b) = (blossom(t0, t0), blossom(t0, t1), blossom(t1, t1));
    // The end weights are the curve's denominators there: positive on a
    // piece without an asymptote. The middle one, scaled to end weights 1,
    // must be positive for the end tangents to meet ahead of both ends.
    if a[2] <= 1e-12 || b[2] <= 1e-12 || m[2] <= 1e-12 {
        return None;
    }
    let w_piece = m[2] / (a[2] * b[2]).sqrt();
    let piece = ConicPiece {
        start: [a[0] / a[2], a[1] / a[2]],
        apex: [m[0] / m[2], m[1] / m[2]],
        end: [b[0] / b[2], b[1] / b[2]],
        rho: rho_of_weight(w_piece),
    };
    check(piece.start, piece.end, piece.apex, piece.rho)
        .ok()
        .map(|_| piece)
}

/// The parameter of the conic at `p`, a point of the curve or of its
/// continuation (near it), by Newton's method on the distance from a
/// sampled start, also outside [0, 1] within `[lo, hi]`.
pub fn parameter_of(start: P2, apex: P2, end: P2, rho: f64, p: P2, lo: f64, hi: f64) -> f64 {
    let at = |t: f64| point(start, apex, end, rho, t);
    let mut best = (0.0, f64::INFINITY);
    let steps = 400;
    for i in 0..=steps {
        let t = lo + (hi - lo) * i as f64 / steps as f64;
        if let Some(q) = at(t) {
            let d = dist(q, p);
            if d < best.1 {
                best = (t, d);
            }
        }
    }
    // Refine by golden-section on the distance around the best sample.
    let h = (hi - lo) / steps as f64;
    let (mut a, mut b) = ((best.0 - h).max(lo), (best.0 + h).min(hi));
    let g = 0.5 * (5f64.sqrt() - 1.0);
    let d = |t: f64| at(t).map_or(f64::INFINITY, |q| dist(q, p));
    for _ in 0..100 {
        let c = b - g * (b - a);
        let e = a + g * (b - a);
        if d(c) < d(e) {
            b = e;
        } else {
            a = c;
        }
    }
    0.5 * (a + b)
}

// The Conic Curve tool and its Rho (`sketch.conic`, `sketch.set_rho`).
impl SketchEdit<'_> {
    /// A conic from `start` to `end` whose end tangents meet at `apex`
    /// (each an existing point or a new one), shaped by `rho` (0.5 when
    /// None): a number, or a parameter's name or an expression of
    /// parameters, which becomes the conic's own Rho parameter
    /// ([`SketchEdit::set_conic_rho`]). Refused in words for a Rho outside
    /// (0, 1), two of the points at one place and the apex on the line
    /// through the ends.
    pub fn conic(
        &mut self,
        start: PointInput,
        end: PointInput,
        apex: PointInput,
        rho: Option<&ValueInput>,
        construction: bool,
    ) -> Result<EntityUid, String> {
        let at = [
            self.input_position(start)?,
            self.input_position(end)?,
            self.input_position(apex)?,
        ];
        check_points(at[0], at[1], at[2])?;
        if let Some(ValueInput::Number(r)) = rho {
            check_rho(*r)?;
        }
        let id = self.id();
        let s = self.point(start)?;
        let e = self.point(end)?;
        let a = self.point(apex)?;
        if s == e || s == a || e == a {
            return Err("two of the conic's points are the same point".to_owned());
        }
        let mut entity = Entity::new(
            id,
            EntityKind::Conic {
                start: s,
                end: e,
                apex: a,
                rho: DEFAULT_RHO,
            },
        );
        entity.construction = construction;
        let id = self.push(entity);
        if let Some(rho) = rho {
            self.set_conic_rho(id, rho)?;
        }
        Ok(id)
    }

    /// Sets the Rho of conic `id` (mitcad#2236): a number, which the conic
    /// keeps (or its own Rho parameter takes, when it has one), or a
    /// parameter's name or an expression of parameters, which the conic
    /// then follows (as its own parameter, slot `conics[c5].rho`, for an
    /// expression). An empty name stops it following a parameter; it keeps
    /// the value it has. Refused in words for a value outside (0, 1).
    pub fn set_conic_rho(&mut self, id: EntityUid, rho: &ValueInput) -> Result<(), String> {
        let current = match self.def.entity(id).map(|e| &e.kind) {
            Some(EntityKind::Conic { rho, .. }) => *rho,
            Some(other) => {
                return Err(crate::said(
                    "only a conic has a Rho",
                    format!("c{} is a {}, not a conic", id.0, other.type_name()),
                ));
            }
            None => return Err(format!("curve c{} does not exist", id.0)),
        };
        let old = self.def.conic_rho(id);
        let value = match rho {
            ValueInput::Name(name) if name.trim().is_empty() => {
                self.def.conic_rhos.retain(|r| r.curve != id);
                current
            }
            ValueInput::Number(r) if old.is_none() => {
                check_rho(*r)?;
                *r
            }
            _ => {
                let slot = format!("conics[c{}].rho", id.0);
                let comment = format!("{} c{} rho", self.owner_name(), id.0);
                let owner = self.owner();
                let (param, created) = resolve_value_as(
                    self.params,
                    &slot,
                    Some(Unit::NONE),
                    rho,
                    owner,
                    &comment,
                    old.as_ref(),
                )
                .map_err(|e| e.to_string())?;
                let v = self
                    .params
                    .value(param)
                    .ok_or_else(|| format!("{slot}: the parameter has no value"))?;
                check_rho(v)?;
                self.report.parameters.extend(created);
                match self.def.conic_rhos.iter_mut().find(|r| r.curve == id) {
                    Some(r) => r.value = param,
                    None => self.def.conic_rhos.push(ConicRho {
                        curve: id,
                        value: param,
                    }),
                }
                v
            }
        };
        if let Some(EntityKind::Conic { rho, .. }) = self.def.entity_mut(id).map(|e| &mut e.kind) {
            *rho = value;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Curve2;

    const S: P2 = [0.0, 0.0];
    const A: P2 = [10.0, 10.0];
    const E: P2 = [20.0, 0.0];

    fn curve(rho: f64) -> Curve2 {
        Curve2::Nurbs(nurbs(S, A, E, rho))
    }

    /// The implicit conic through the five points `pts`, its coefficients
    /// (a x² + b xy + c y² + d x + e y + f) with a = 1 (the conics of these
    /// tests, symmetric about x = 10, all have an x² term; they pass
    /// through the origin, so f is no coefficient to scale by).
    fn fit(pts: &[P2]) -> [f64; 6] {
        // b xy + c y² + d x + e y + f = -x², by Gauss-Jordan elimination.
        let mut m: Vec<[f64; 6]> = pts
            .iter()
            .map(|p| [p[0] * p[1], p[1] * p[1], p[0], p[1], 1.0, -p[0] * p[0]])
            .collect();
        for col in 0..5 {
            let pivot = (col..5)
                .max_by(|&i, &j| m[i][col].abs().total_cmp(&m[j][col].abs()))
                .unwrap();
            m.swap(col, pivot);
            let lead = m[col];
            for (row, r) in m.iter_mut().enumerate() {
                if row != col {
                    let f = r[col] / lead[col];
                    for (x, l) in r.iter_mut().zip(lead) {
                        *x -= f * l;
                    }
                }
            }
        }
        let mut c = [1.0; 6];
        for (i, r) in m.iter().enumerate() {
            c[i + 1] = r[5] / r[i];
        }
        c
    }

    fn kind(c: &[f64; 6]) -> f64 {
        // The discriminant b² - 4ac: zero a parabola, negative an ellipse,
        // positive a hyperbola (relative to the size of the terms).
        (c[1] * c[1] - 4.0 * c[0] * c[2]) / (c[0].abs() + c[1].abs() + c[2].abs()).powi(2)
    }

    /// Rho 0.5 is a parabola: its sampled points satisfy the parabola
    /// through the ends with its axis along the apex's direction, here
    /// y = x (20 - x) / 20, within 1e-6 mm; 0.3 and 0.7 are an elliptical
    /// and a hyperbolic arc.
    #[test]
    fn rho_chooses_parabola_ellipse_and_hyperbola() {
        let c = curve(0.5);
        for i in 0..=50 {
            let p = c.point(i as f64 / 50.0);
            let y = p[0] * (20.0 - p[0]) / 20.0;
            assert!((p[1] - y).abs() < 1e-6, "{p:?}");
        }
        let five = |rho: f64| {
            let c = curve(rho);
            let pts: Vec<P2> = [0.05, 0.25, 0.5, 0.7, 0.95]
                .iter()
                .map(|t| c.point(*t))
                .collect();
            let coefficients = fit(&pts);
            // Every other sample is on that conic too.
            for i in 0..=40 {
                let p = c.point(i as f64 / 40.0);
                let v = coefficients[0] * p[0] * p[0]
                    + coefficients[1] * p[0] * p[1]
                    + coefficients[2] * p[1] * p[1]
                    + coefficients[3] * p[0]
                    + coefficients[4] * p[1]
                    + coefficients[5];
                assert!(v.abs() < 1e-6, "rho {rho}: {v}");
            }
            kind(&coefficients)
        };
        assert!(five(0.3) < -1e-6, "an ellipse");
        assert!(five(0.7) > 1e-6, "a hyperbola");
    }

    /// The end tangents run through the apex.
    #[test]
    fn end_tangents_pass_through_the_apex() {
        for rho in [0.2, 0.5, 0.8] {
            let c = curve(rho);
            let [p0, d0, _] = c.eval(0.0);
            let [p1, d1, _] = c.eval(1.0);
            assert!(cross(d0, sub(A, p0)).abs() < 1e-9 * norm(d0));
            assert!(cross(d1, sub(A, p1)).abs() < 1e-9 * norm(d1));
            assert!(crate::geometry::dot(d0, sub(A, p0)) > 0.0);
            assert!(crate::geometry::dot(d1, sub(p1, A)) > 0.0);
        }
    }

    /// A piece is a conic on the same curve.
    #[test]
    fn a_piece_is_a_conic_on_the_same_curve() {
        for rho in [0.3, 0.5, 0.7] {
            let whole = curve(rho);
            let p = piece(S, A, E, rho, 0.2, 0.65).expect("a piece");
            let part = Curve2::Nurbs(nurbs(p.start, p.apex, p.end, p.rho));
            for i in 0..=30 {
                let q = part.point(i as f64 / 30.0);
                let (_, d) = whole.closest(q);
                assert!(d < 1e-6, "rho {rho}: {d}");
            }
            assert!(dist(p.start, whole.point(0.2)) < 1e-9);
            assert!(dist(p.end, whole.point(0.65)) < 1e-9);
            // The whole range gives the conic back.
            let same = piece(S, A, E, rho, 0.0, 1.0).unwrap();
            assert!((same.rho - rho).abs() < 1e-12);
            assert!(dist(same.apex, A) < 1e-9);
        }
    }

    /// Extending along the conic's own continuation, as far as it has a
    /// shoulder.
    #[test]
    fn extension_follows_the_curve() {
        let p = piece(S, A, E, 0.5, 0.0, 1.3).expect("a longer parabola");
        let longer = Curve2::Nurbs(nurbs(p.start, p.apex, p.end, p.rho));
        for i in 0..=30 {
            let q = longer.point(i as f64 / 30.0);
            let y = q[0] * (20.0 - q[0]) / 20.0;
            assert!((q[1] - y).abs() < 1e-6, "{q:?}");
        }
        // A hyperbola stops short of its asymptote.
        assert!(piece(S, A, E, 0.9, 0.0, 50.0).is_none());
    }

    #[test]
    fn refusals_are_in_words() {
        assert!(check_rho(0.0).unwrap_err().contains("between 0 and 1"));
        assert!(check_rho(1.0).is_err());
        assert!(check_rho(f64::NAN).is_err());
        assert!(check_points(S, S, A).unwrap_err().contains("same place"));
        assert!(check_points(S, E, S).unwrap_err().contains("shoulder"));
        assert!(
            check_points(S, E, [5.0, 0.0])
                .unwrap_err()
                .contains("line through its ends")
        );
        assert!(check(S, E, A, 0.5).is_ok());
    }

    #[test]
    fn parameters_are_found_back() {
        let t = parameter_of(S, A, E, 0.4, point(S, A, E, 0.4, 0.37).unwrap(), 0.0, 1.0);
        assert!((t - 0.37).abs() < 1e-7, "{t}");
        let t = parameter_of(S, A, E, 0.5, point(S, A, E, 0.5, 1.2).unwrap(), -1.0, 2.0);
        assert!((t - 1.2).abs() < 1e-7, "{t}");
    }
}
