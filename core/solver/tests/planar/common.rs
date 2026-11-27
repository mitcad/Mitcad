// SPDX-License-Identifier: MIT
//! Helpers shared by the solver tests.
#![allow(dead_code)]

use mitcad_solver::{EntityId, PointId, SolveOptions, SolveResult, System};

pub const EPS: f64 = 1e-6;

pub fn opts() -> SolveOptions {
    SolveOptions::default()
}

pub fn solve_ok(s: &mut System) -> SolveResult {
    let r = s.solve(&opts());
    assert!(r.is_ok(), "solve failed: {r:?}");
    r
}

pub fn pt(s: &System, p: PointId) -> [f64; 2] {
    s.point(p).expect("point")
}

pub fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

pub fn assert_near(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() < EPS, "{what}: {a} vs {b}");
}

pub fn assert_pt(s: &System, p: PointId, x: f64, y: f64) {
    let v = pt(s, p);
    assert!(
        (v[0] - x).abs() < EPS && (v[1] - y).abs() < EPS,
        "point {p:?} at {v:?}, expected [{x}, {y}]"
    );
}

/// End points of a line.
pub fn ends(s: &System, l: EntityId) -> ([f64; 2], [f64; 2]) {
    let p = s.entity_points(l).expect("line");
    (pt(s, p[0]), pt(s, p[1]))
}

pub fn line_len(s: &System, l: EntityId) -> f64 {
    let (a, b) = ends(s, l);
    dist(a, b)
}

/// Direction angle of a line in radians.
pub fn line_angle(s: &System, l: EntityId) -> f64 {
    let (a, b) = ends(s, l);
    (b[1] - a[1]).atan2(b[0] - a[0])
}

/// Deterministic pseudo-random numbers in [0, 1) (xorshift).
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in [-a, a).
    pub fn sym(&mut self, a: f64) -> f64 {
        (2.0 * self.next() - 1.0) * a
    }
}

/// A rectangle from four shared corner points: bottom, right, top, left
/// lines, with horizontal and vertical constraints.
pub struct Rect {
    pub p: [PointId; 4],
    pub l: [EntityId; 4],
}

pub fn rect(s: &mut System, x: f64, y: f64, w: f64, h: f64) -> Rect {
    use mitcad_solver::Constraint::*;
    let p = [
        s.add_point(x, y),
        s.add_point(x + w, y),
        s.add_point(x + w, y + h),
        s.add_point(x, y + h),
    ];
    let l = [
        s.add_line(p[0], p[1]).unwrap(),
        s.add_line(p[1], p[2]).unwrap(),
        s.add_line(p[2], p[3]).unwrap(),
        s.add_line(p[3], p[0]).unwrap(),
    ];
    s.add_constraint(Horizontal(l[0])).unwrap();
    s.add_constraint(Horizontal(l[2])).unwrap();
    s.add_constraint(Vertical(l[1])).unwrap();
    s.add_constraint(Vertical(l[3])).unwrap();
    Rect { p, l }
}
