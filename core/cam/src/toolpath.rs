// SPDX-License-Identifier: MIT
//! Toolpaths: the tool tip's moves in the work coordinates (mm, mm/min),
//! independent of any controller. The post-processors turn them into
//! G-code; the checks and the simulation sample them.

use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::geom::{Contour, POS_EPS};

/// A point of the tool tip (mm).
pub type P3 = [f64; 3];

/// One move of the tool tip.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Move {
    /// At the machine's rapid rate, through the air.
    Rapid { to: P3 },
    /// A straight cut at `feed` (mm/min).
    Line { to: P3, feed: f64 },
    /// An arc in the XY plane about `center`, a helix when Z changes;
    /// `to` equal to the start is a full turn.
    Arc {
        to: P3,
        center: [f64; 2],
        ccw: bool,
        feed: f64,
    },
}

impl Move {
    pub fn to(&self) -> P3 {
        match *self {
            Move::Rapid { to } | Move::Line { to, .. } | Move::Arc { to, .. } => to,
        }
    }
}

/// A toolpath: its moves from wherever the tool is when it starts; the
/// first move is a rapid.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Toolpath {
    pub moves: Vec<Move>,
    /// Drilling cycles a post-processor may write as canned cycles in
    /// place of the moves they stand for (the moves stay, for the checks,
    /// the simulation and the controllers without cycles).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cycles: Vec<DrillCycle>,
}

/// Which canned drilling cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CycleKind {
    /// In one go (G81).
    Drill,
    /// Pecks, back out to the retract height after each (G83).
    Peck,
    /// Pecks, backing off a little after each to break the chip (G73).
    ChipBreak,
}

impl CycleKind {
    /// The G-code word.
    pub fn code(&self) -> &'static str {
        match self {
            CycleKind::Drill => "G81",
            CycleKind::Peck => "G83",
            CycleKind::ChipBreak => "G73",
        }
    }
}

/// Holes drilled the same way, standing for `moves[first..first + count]`
/// of the toolpath: from above the first hole at the `clearance` height
/// (where the move before them ends), each hole down from `retract` to
/// `bottom` at `feed`, in pecks of `peck` (zero: in one go), back up to
/// the clearance height between holes and after the last.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrillCycle {
    pub first: usize,
    pub count: usize,
    pub kind: CycleKind,
    pub holes: Vec<[f64; 2]>,
    pub clearance: f64,
    pub retract: f64,
    pub bottom: f64,
    #[serde(default)]
    pub peck: f64,
    pub feed: f64,
}

impl Toolpath {
    /// The bounding box of the move end points and arcs: `(min, max)`.
    pub fn bounds(&self) -> Option<(P3, P3)> {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        let mut grow = |p: P3| {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        };
        let mut at: Option<P3> = None;
        for m in &self.moves {
            if let (Move::Arc { .. }, Some(from)) = (m, at) {
                for p in move_points(from, m, 0.01) {
                    grow(p);
                }
            }
            grow(m.to());
            at = Some(m.to());
        }
        if min[0].is_finite() {
            Some((min, max))
        } else {
            None
        }
    }

    /// The length of the cutting moves (mm).
    pub fn cut_length(&self) -> f64 {
        let mut at: Option<P3> = None;
        let mut total = 0.0;
        for m in &self.moves {
            if let (Some(from), Move::Line { .. } | Move::Arc { .. }) = (at, m) {
                total += move_length(from, m);
            }
            at = Some(m.to());
        }
        total
    }

    /// The time the moves take at their feeds and `rapid` (mm/min), in
    /// minutes, without the controller's acceleration.
    pub fn duration(&self, rapid: f64) -> f64 {
        let mut at: Option<P3> = None;
        let mut total = 0.0;
        for m in &self.moves {
            if let Some(from) = at {
                let rate = match *m {
                    Move::Rapid { .. } => rapid,
                    Move::Line { feed, .. } | Move::Arc { feed, .. } => feed,
                };
                if rate > 0.0 {
                    total += move_length(from, m) / rate;
                }
            }
            at = Some(m.to());
        }
        total
    }
}

/// The sweep of an arc from `from` to `to` about `center` (radians,
/// positive counter-clockwise); a whole turn when they meet.
pub fn arc_sweep(from: [f64; 2], to: [f64; 2], center: [f64; 2], ccw: bool) -> f64 {
    let a0 = (from[1] - center[1]).atan2(from[0] - center[0]);
    let a1 = (to[1] - center[1]).atan2(to[0] - center[0]);
    let mut d = a1 - a0;
    if ccw {
        while d <= 1e-12 {
            d += TAU;
        }
        while d > TAU + 1e-12 {
            d -= TAU;
        }
    } else {
        while d >= -1e-12 {
            d -= TAU;
        }
        while d < -TAU - 1e-12 {
            d += TAU;
        }
    }
    d
}

/// The length of a move from `from`.
pub fn move_length(from: P3, m: &Move) -> f64 {
    match *m {
        Move::Rapid { to } | Move::Line { to, .. } => dist3(from, to),
        Move::Arc {
            to, center, ccw, ..
        } => {
            let r = (from[0] - center[0]).hypot(from[1] - center[1]);
            let sweep = arc_sweep([from[0], from[1]], [to[0], to[1]], center, ccw).abs();
            (r * sweep).hypot(to[2] - from[2])
        }
    }
}

pub(crate) fn dist3(a: P3, b: P3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Points along a move from `from` (its end included, its start not),
/// arcs within `tolerance` (mm) of the true arc.
pub fn move_points(from: P3, m: &Move, tolerance: f64) -> Vec<P3> {
    match *m {
        Move::Rapid { to } | Move::Line { to, .. } => vec![to],
        Move::Arc {
            to, center, ccw, ..
        } => {
            let r = (from[0] - center[0]).hypot(from[1] - center[1]);
            let sweep = arc_sweep([from[0], from[1]], [to[0], to[1]], center, ccw);
            let tol = tolerance.max(1e-6).min(r);
            // The chord of a step of angle t sags r (1 - cos(t / 2)).
            let step = if r > 0.0 {
                2.0 * (1.0 - tol / r).clamp(-1.0, 1.0).acos()
            } else {
                TAU
            };
            let n = ((sweep.abs() / step.max(1e-3)).ceil() as usize).max(1);
            let a0 = (from[1] - center[1]).atan2(from[0] - center[0]);
            (1..=n)
                .map(|i| {
                    if i == n {
                        return to;
                    }
                    let t = i as f64 / n as f64;
                    let a = a0 + sweep * t;
                    [
                        center[0] + r * a.cos(),
                        center[1] + r * a.sin(),
                        from[2] + (to[2] - from[2]) * t,
                    ]
                })
                .collect()
        }
    }
}

/// Builds a toolpath, leaving out moves that go nowhere.
#[derive(Clone, Debug, Default)]
pub struct PathBuilder {
    moves: Vec<Move>,
    at: Option<P3>,
}

impl PathBuilder {
    pub fn new() -> Self {
        PathBuilder::default()
    }

    /// A builder whose tool is already at `at` (the moves before are
    /// someone else's).
    pub(crate) fn starting_at(at: P3) -> Self {
        PathBuilder {
            moves: Vec::new(),
            at: Some(at),
        }
    }

    /// Where the tool is after the last move.
    pub fn at(&self) -> Option<P3> {
        self.at
    }

    fn here(&self, to: P3) -> bool {
        self.at.is_some_and(|a| dist3(a, to) <= POS_EPS)
    }

    pub fn rapid(&mut self, to: P3) {
        if !self.here(to) {
            self.moves.push(Move::Rapid { to });
            self.at = Some(to);
        }
    }

    pub fn line(&mut self, to: P3, feed: f64) {
        if !self.here(to) {
            self.moves.push(Move::Line { to, feed });
            self.at = Some(to);
        }
    }

    pub fn arc(&mut self, to: P3, center: [f64; 2], ccw: bool, feed: f64) {
        self.moves.push(Move::Arc {
            to,
            center,
            ccw,
            feed,
        });
        self.at = Some(to);
    }

    /// Goes up to `z` where it is, then across to `xy` there: the way
    /// between two cuts.
    pub fn retract_to(&mut self, xy: [f64; 2], z: f64) {
        if let Some(at) = self.at
            && at[2] < z
        {
            self.rapid([at[0], at[1], z]);
        }
        self.rapid([xy[0], xy[1], z]);
    }

    /// Cuts along `contour` at height `z` from its first vertex (where
    /// the tool must be), round to the first vertex again when closed.
    pub fn follow(&mut self, contour: &Contour, z: f64, feed: f64) {
        for s in contour.segments() {
            let to = [s.end[0], s.end[1], z];
            match s.arc {
                Some((center, ccw)) => self.arc(to, center, ccw, feed),
                None => self.line(to, feed),
            }
        }
    }

    pub fn finish(self) -> Toolpath {
        Toolpath {
            moves: self.moves,
            cycles: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_sweep_runs_the_given_way() {
        let s = arc_sweep([1.0, 0.0], [0.0, 1.0], [0.0, 0.0], true);
        assert!((s - TAU / 4.0).abs() < 1e-12);
        let s = arc_sweep([1.0, 0.0], [0.0, 1.0], [0.0, 0.0], false);
        assert!((s + 3.0 * TAU / 4.0).abs() < 1e-12);
        let full = arc_sweep([1.0, 0.0], [1.0, 0.0], [0.0, 0.0], true);
        assert!((full - TAU).abs() < 1e-12);
    }

    #[test]
    fn builder_follows_a_circle_and_measures_it() {
        let mut b = PathBuilder::new();
        let c = Contour::circle([0.0, 0.0], 10.0);
        b.rapid([10.0, 0.0, 5.0]);
        b.line([10.0, 0.0, -1.0], 100.0);
        b.line([10.0, 0.0, -1.0], 100.0);
        b.follow(&c, -1.0, 500.0);
        let p = b.finish();
        assert_eq!(p.moves.len(), 4);
        assert!((p.cut_length() - (6.0 + TAU * 10.0)).abs() < 1e-9);
        let (min, max) = p.bounds().unwrap();
        assert!((min[0] + 10.0).abs() < 0.02 && (max[1] - 10.0).abs() < 0.02);
        assert_eq!(min[2], -1.0);
    }

    #[test]
    fn arc_points_stay_within_the_tolerance() {
        let m = Move::Arc {
            to: [-10.0, 0.0, -2.0],
            center: [0.0, 0.0],
            ccw: true,
            feed: 1.0,
        };
        let pts = move_points([10.0, 0.0, 0.0], &m, 0.01);
        assert!(pts.len() > 10);
        let mut prev = [10.0, 0.0, 0.0];
        for p in &pts {
            let mid = [(prev[0] + p[0]) / 2.0, (prev[1] + p[1]) / 2.0];
            assert!(10.0 - mid[0].hypot(mid[1]) <= 0.01 + 1e-9);
            assert!(p[1] >= -1e-9);
            prev = *p;
        }
        assert_eq!(*pts.last().unwrap(), [-10.0, 0.0, -2.0]);
    }
}
