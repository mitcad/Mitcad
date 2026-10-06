// SPDX-License-Identifier: MIT
//! Numbers for residual evaluation.
//!
//! Every residual is written once, generically over [`Scalar`]. Evaluating it
//! with `f64` gives the value; evaluating it with [`Dual`] (forward-mode
//! automatic differentiation) gives the value and up to [`LANES`] partial
//! derivatives at once. Equations with more variables are differentiated in
//! several passes, so the Jacobian is exact without hand-written derivatives.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// Number of partial derivatives carried by one [`Dual`].
pub(crate) const LANES: usize = 8;

pub(crate) trait Scalar:
    Copy
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
{
    fn cst(v: f64) -> Self;
    fn val(self) -> f64;
    fn sqrt(self) -> Self;
    fn sin(self) -> Self;
    fn cos(self) -> Self;
    fn atan2(self, x: Self) -> Self;
    /// Multiplication by a constant.
    fn mulf(self, k: f64) -> Self;
}

impl Scalar for f64 {
    fn cst(v: f64) -> Self {
        v
    }
    fn val(self) -> f64 {
        self
    }
    fn sqrt(self) -> Self {
        f64::sqrt(self)
    }
    fn sin(self) -> Self {
        f64::sin(self)
    }
    fn cos(self) -> Self {
        f64::cos(self)
    }
    fn atan2(self, x: Self) -> Self {
        f64::atan2(self, x)
    }
    fn mulf(self, k: f64) -> Self {
        self * k
    }
}

/// A value with partial derivatives along [`LANES`] seed directions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Dual {
    pub v: f64,
    pub d: [f64; LANES],
}

impl Dual {
    /// A variable seeded on `lane` (or a constant when `lane` is `None`).
    pub fn var(v: f64, lane: Option<usize>) -> Self {
        let mut d = [0.0; LANES];
        if let Some(l) = lane {
            d[l] = 1.0;
        }
        Dual { v, d }
    }

    fn chain(self, v: f64, k: f64) -> Self {
        let mut d = self.d;
        for x in &mut d {
            *x *= k;
        }
        Dual { v, d }
    }
}

impl Add for Dual {
    type Output = Dual;
    fn add(self, o: Dual) -> Dual {
        let mut d = self.d;
        for (a, b) in d.iter_mut().zip(o.d) {
            *a += b;
        }
        Dual { v: self.v + o.v, d }
    }
}

impl Sub for Dual {
    type Output = Dual;
    fn sub(self, o: Dual) -> Dual {
        let mut d = self.d;
        for (a, b) in d.iter_mut().zip(o.d) {
            *a -= b;
        }
        Dual { v: self.v - o.v, d }
    }
}

impl Mul for Dual {
    type Output = Dual;
    fn mul(self, o: Dual) -> Dual {
        let mut d = [0.0; LANES];
        for (i, x) in d.iter_mut().enumerate() {
            *x = self.d[i] * o.v + o.d[i] * self.v;
        }
        Dual { v: self.v * o.v, d }
    }
}

impl Div for Dual {
    type Output = Dual;
    fn div(self, o: Dual) -> Dual {
        let inv = 1.0 / o.v;
        let q = self.v * inv;
        let mut d = [0.0; LANES];
        for (i, x) in d.iter_mut().enumerate() {
            *x = (self.d[i] - q * o.d[i]) * inv;
        }
        Dual { v: q, d }
    }
}

impl Neg for Dual {
    type Output = Dual;
    fn neg(self) -> Dual {
        self.chain(-self.v, -1.0)
    }
}

impl Scalar for Dual {
    fn cst(v: f64) -> Self {
        Dual { v, d: [0.0; LANES] }
    }
    fn val(self) -> f64 {
        self.v
    }
    fn sqrt(self) -> Self {
        let s = self.v.sqrt();
        // The derivative is unbounded at zero; zero keeps the Jacobian finite
        // (the row is then rank deficient, which the analysis reports).
        let k = if s > 0.0 { 0.5 / s } else { 0.0 };
        self.chain(s, k)
    }
    fn sin(self) -> Self {
        self.chain(self.v.sin(), self.v.cos())
    }
    fn cos(self) -> Self {
        self.chain(self.v.cos(), -self.v.sin())
    }
    fn atan2(self, x: Self) -> Self {
        let r2 = self.v * self.v + x.v * x.v;
        let v = self.v.atan2(x.v);
        let mut d = [0.0; LANES];
        if r2 > 0.0 {
            for (i, o) in d.iter_mut().enumerate() {
                *o = (x.v * self.d[i] - self.v * x.d[i]) / r2;
            }
        }
        Dual { v, d }
    }
    fn mulf(self, k: f64) -> Self {
        self.chain(self.v * k, k)
    }
}

/// A 2D vector of scalars.
#[derive(Clone, Copy, Debug)]
pub(crate) struct V2<T> {
    pub x: T,
    pub y: T,
}

impl<T: Scalar> V2<T> {
    pub fn new(x: T, y: T) -> Self {
        V2 { x, y }
    }
    pub fn add(self, o: Self) -> Self {
        V2::new(self.x + o.x, self.y + o.y)
    }
    pub fn sub(self, o: Self) -> Self {
        V2::new(self.x - o.x, self.y - o.y)
    }
    pub fn mul(self, k: T) -> Self {
        V2::new(self.x * k, self.y * k)
    }
    pub fn mulf(self, k: f64) -> Self {
        V2::new(self.x.mulf(k), self.y.mulf(k))
    }
    pub fn dot(self, o: Self) -> T {
        self.x * o.x + self.y * o.y
    }
    pub fn cross(self, o: Self) -> T {
        self.x * o.y - self.y * o.x
    }
    pub fn norm(self) -> T {
        self.dot(self).sqrt()
    }
    /// Rotated by +90 degrees.
    pub fn perp(self) -> Self {
        V2::new(-self.y, self.x)
    }
    /// Unit vector; a zero vector stays zero instead of becoming NaN.
    pub fn unit(self) -> Self {
        let n = self.norm();
        if n.val() > 0.0 {
            V2::new(self.x / n, self.y / n)
        } else {
            self
        }
    }
    pub fn get(self, axis: usize) -> T {
        if axis == 0 { self.x } else { self.y }
    }
}

/// Wraps an angle to (-pi, pi].
pub(crate) fn wrap_angle(a: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let mut r = a % tau;
    if r <= -std::f64::consts::PI {
        r += tau;
    } else if r > std::f64::consts::PI {
        r -= tau;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f<T: Scalar>(x: T, y: T) -> T {
        (x * y + x.sin()).sqrt() / (y.cos() + T::cst(2.0)) + y.atan2(x).mulf(3.0) - (-x)
    }

    #[test]
    fn dual_matches_finite_differences() {
        let (x, y) = (0.7, 1.3);
        let d = f(Dual::var(x, Some(0)), Dual::var(y, Some(1)));
        let h = 1e-6;
        let dx = (f(x + h, y) - f(x - h, y)) / (2.0 * h);
        let dy = (f(x, y + h) - f(x, y - h)) / (2.0 * h);
        assert!((d.v - f(x, y)).abs() < 1e-15);
        assert!((d.d[0] - dx).abs() < 1e-8, "{} {}", d.d[0], dx);
        assert!((d.d[1] - dy).abs() < 1e-8, "{} {}", d.d[1], dy);
    }

    #[test]
    fn wrap() {
        use std::f64::consts::PI;
        assert!((wrap_angle(3.0 * PI) - PI).abs() < 1e-12);
        assert!((wrap_angle(-3.0 * PI) - PI).abs() < 1e-12);
        assert!((wrap_angle(0.5) - 0.5).abs() < 1e-15);
        assert!((wrap_angle(-PI + 0.1) - (-PI + 0.1)).abs() < 1e-12);
    }
}
