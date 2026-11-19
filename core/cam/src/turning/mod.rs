// SPDX-License-Identifier: MIT
//! Turning on a 2-axis lathe (mitcad#1869, docs/cam.md, "Turning"): the
//! lathe's own machine data, the turning tools, the part's revolved
//! profile, the operations (facing, roughing and finishing of outside and
//! inside diameters, grooving, parting off, threading), the Fanuc turning
//! dialect, the program check and the simulation of the turned stock.
//!
//! **Coordinates.** Everything is in the plane through the spindle axis:
//! `z` along the spindle (positive away from the chuck, zero on the part's
//! face by default) and `x` the distance from the axis (a radius). The
//! post writes X as a diameter. Drawn with Z to the right and X up (the
//! upper half of the part, the usual programming view), a clockwise arc is
//! G2.
//!
//! **The controlled point** is the tool's imaginary tip: where the tangents
//! of the nose radius along X and Z meet, the point the operator touches
//! off. For an outside tool the nose's centre is `nose_radius` above and to
//! the right of it, for an inside tool below and to the right. The
//! toolpaths compensate the nose radius in our code (G40 in the program),
//! as the milling paths do (docs/cam.md, Decision 3).
//!
//! **Feeds** are per revolution (mm/rev, G99), as lathes cut; the spindle
//! runs at a constant surface speed (G96, m/min) under a limit (G50) or at
//! a constant speed (G97, rpm).
//!
//! The machine's kinematics are kept general (the lathe's [`LatheAxes`]):
//! a C axis, live tools, a Y axis or a tilting head come later.

use serde::{Deserialize, Serialize};

use crate::CamError;
use crate::program::{Coolant, SpindleDirection};

pub mod check;
pub mod lathe;
pub mod ops;
pub mod post;
pub mod profile;
pub mod stock;
pub mod tool;

#[cfg(test)]
mod tests;

pub use check::{ProgramCheck, ReadMove, TurnPlacement, check_lathe, check_program};
pub use lathe::{Lathe, LatheAxes};
pub use ops::{
    FaceOp, FinishOp, GrooveOp, PartOp, RoughOp, ThreadOp, TurnSide, face, finish, groove,
    part_off, rough, thread,
};
pub use post::{DialectInfo, TurnOutput, decimal};
pub use post::{FanucTurning, TurningPost, turning_dialect, turning_dialects, turning_post};
pub use profile::{PV, TurnPart};
pub use stock::{TurnStock, TurnedStock, simulate_turning};
pub use tool::{TurningTool, TurningToolKind};

/// A point in the lathe's plane: `[x, z]`, x a radius (mm).
pub type PXZ = [f64; 2];

/// One move of the tool's controlled point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnMove {
    /// At the machine's rapid rate, through the air.
    Rapid { to: PXZ },
    /// A straight cut at `feed` (mm/rev).
    Line { to: PXZ, feed: f64 },
    /// An arc about `center` (`[x, z]`), clockwise as drawn with Z right
    /// and X up (G2) when `cw`.
    Arc {
        to: PXZ,
        center: PXZ,
        cw: bool,
        feed: f64,
    },
    /// A threading move, synchronised with the spindle: `lead` mm per
    /// revolution (G32).
    Thread { to: PXZ, lead: f64 },
}

impl TurnMove {
    pub fn to(&self) -> PXZ {
        match *self {
            TurnMove::Rapid { to }
            | TurnMove::Line { to, .. }
            | TurnMove::Arc { to, .. }
            | TurnMove::Thread { to, .. } => to,
        }
    }

    /// The feed per revolution; none for a rapid.
    pub fn feed(&self) -> Option<f64> {
        match *self {
            TurnMove::Rapid { .. } => None,
            TurnMove::Line { feed, .. } | TurnMove::Arc { feed, .. } => Some(feed),
            TurnMove::Thread { lead, .. } => Some(lead),
        }
    }
}

/// A turning cycle a post may write as the controller's canned cycle in
/// place of the moves it stands for, `moves[first..first + count]`; the
/// moves stay for the checks, the simulation and the controllers without
/// cycles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnCycle {
    /// Stock removal along Z (G71) from `start`, at most `depth` deep per
    /// pass (radius), backing off `retract`, leaving `allowance_x` (radius)
    /// and `allowance_z` for finishing; `profile` is the finished contour
    /// (its first move a rapid in X alone), `feed` per revolution.
    Rough {
        first: usize,
        count: usize,
        start: PXZ,
        depth: f64,
        retract: f64,
        allowance_x: f64,
        allowance_z: f64,
        feed: f64,
        profile: Vec<TurnMove>,
    },
    /// Finishing along a contour (G70) from `start`: the same profile as
    /// an earlier [`TurnCycle::Rough`], whose blocks it runs again.
    Finish {
        first: usize,
        count: usize,
        start: PXZ,
        profile: Vec<TurnMove>,
    },
    /// Threading (G76) from `start`: down to `root_x` (radius) at `end_z`,
    /// `height` the thread's depth (radius), the first pass `first_depth`
    /// deep, no pass under `min_depth`, `finish_allowance` for the last
    /// pass, `spring_passes` finishing passes, the tool's `angle`
    /// (degrees), `chamfer` the pull-out at the end in leads (0 to 9.9),
    /// `lead` per revolution.
    Thread {
        first: usize,
        count: usize,
        start: PXZ,
        root_x: f64,
        end_z: f64,
        height: f64,
        first_depth: f64,
        min_depth: f64,
        finish_allowance: f64,
        spring_passes: u32,
        angle: f64,
        chamfer: f64,
        lead: f64,
    },
}

impl TurnCycle {
    /// The moves it stands for: `(first, count)`.
    pub fn span(&self) -> (usize, usize) {
        match *self {
            TurnCycle::Rough { first, count, .. }
            | TurnCycle::Finish { first, count, .. }
            | TurnCycle::Thread { first, count, .. } => (first, count),
        }
    }
}

/// A turning toolpath: the controlled point's moves from wherever the
/// tool is when it starts; the first move is a rapid.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnPath {
    pub moves: Vec<TurnMove>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<TurnCycle>,
}

impl TurnPath {
    /// The extent of the move end points and arcs: `(min, max)` of
    /// `[x, z]`.
    pub fn bounds(&self) -> Option<(PXZ, PXZ)> {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        let mut at: Option<PXZ> = None;
        for m in &self.moves {
            let pts = match at {
                Some(from) => move_points(from, m, 0.01),
                None => vec![m.to()],
            };
            for p in pts {
                for k in 0..2 {
                    min[k] = min[k].min(p[k]);
                    max[k] = max[k].max(p[k]);
                }
            }
            at = Some(m.to());
        }
        min[0].is_finite().then_some((min, max))
    }

    /// The length of the cutting moves (mm).
    pub fn cut_length(&self) -> f64 {
        let mut at: Option<PXZ> = None;
        let mut total = 0.0;
        for m in &self.moves {
            if let (Some(from), false) = (at, matches!(m, TurnMove::Rapid { .. })) {
                total += move_length(from, m);
            }
            at = Some(m.to());
        }
        total
    }

    /// The time the moves take (minutes), the feeds per revolution at
    /// `rpm_at(x)` (the spindle's speed with the tool at radius `x`) and
    /// rapids at `rapid` (mm/min), without the machine's acceleration.
    pub fn duration(&self, rapid: f64, rpm_at: impl Fn(f64) -> f64) -> f64 {
        let mut at: Option<PXZ> = None;
        let mut total = 0.0;
        for m in &self.moves {
            if let Some(from) = at {
                let length = move_length(from, m);
                let rate = match m.feed() {
                    None => rapid,
                    Some(f) => f * rpm_at(0.5 * (from[0] + m.to()[0]).abs()),
                };
                if rate > 0.0 {
                    total += length / rate;
                }
            }
            at = Some(m.to());
        }
        total
    }
}

/// How the spindle turns in an operation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnSpindle {
    /// A constant surface speed (G96, m/min), never faster than `max_rpm`
    /// (G50).
    Css { surface_speed: f64, max_rpm: f64 },
    /// A constant speed (G97, rpm).
    Rpm { rpm: f64 },
}

impl TurnSpindle {
    /// The spindle's speed with the tool at radius `x` (mm).
    pub fn rpm_at(&self, x: f64) -> f64 {
        match *self {
            TurnSpindle::Rpm { rpm } => rpm,
            TurnSpindle::Css {
                surface_speed,
                max_rpm,
            } => {
                let d = 2.0 * x.abs();
                if d < 1e-9 {
                    max_rpm
                } else {
                    (surface_speed * 1000.0 / (std::f64::consts::PI * d)).min(max_rpm)
                }
            }
        }
    }

    pub fn validate(&self, name: &str) -> Result<(), CamError> {
        let ok = match *self {
            TurnSpindle::Rpm { rpm } => rpm.is_finite() && rpm > 0.0,
            TurnSpindle::Css {
                surface_speed,
                max_rpm,
            } => {
                surface_speed.is_finite()
                    && surface_speed > 0.0
                    && max_rpm.is_finite()
                    && max_rpm > 0.0
            }
        };
        if ok {
            Ok(())
        } else {
            Err(CamError::new(format!(
                "{name}: the spindle needs a speed and a limit above zero."
            )))
        }
    }
}

/// One operation of a turning program.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurningProgramOp {
    /// The operation's name, written as a comment.
    pub name: String,
    pub tool: TurningTool,
    pub spindle: TurnSpindle,
    #[serde(default)]
    pub direction: SpindleDirection,
    #[serde(default)]
    pub coolant: Coolant,
    pub path: TurnPath,
}

fn g54() -> u8 {
    1
}

fn program_number() -> u32 {
    1000
}

/// A turning program: what one chucking of the stock cuts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurningProgram {
    pub name: String,
    /// The program's number (`O1000`), 1 to 9999.
    #[serde(default = "program_number")]
    pub number: u32,
    /// The work offset: 1 to 6 for G54 to G59.
    #[serde(default = "g54")]
    pub work_offset: u8,
    /// Where the turret indexes, `[x, z]` in work coordinates: clear of the
    /// stock and the chuck. The program starts and ends there.
    pub change_position: PXZ,
    pub operations: Vec<TurningProgramOp>,
}

impl TurningProgram {
    pub fn new(name: &str, change_position: PXZ) -> Self {
        TurningProgram {
            name: name.to_owned(),
            number: program_number(),
            work_offset: 1,
            change_position,
            operations: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), CamError> {
        if !(1..=9999).contains(&self.number) {
            return Err(CamError::new("The program number must be from 1 to 9999."));
        }
        if !(1..=6).contains(&self.work_offset) {
            return Err(CamError::new("The work offset must be one of G54 to G59."));
        }
        if !self.change_position.iter().all(|v| v.is_finite()) {
            return Err(CamError::new("The tool change position must be numbers."));
        }
        for op in &self.operations {
            op.tool.validate().map_err(CamError::new)?;
            op.spindle.validate(&op.name)?;
            if !matches!(op.path.moves.first(), None | Some(TurnMove::Rapid { .. })) {
                return Err(CamError::new(format!(
                    "The toolpath of {} must start with a rapid.",
                    op.name
                )));
            }
            let bad = op.path.moves.iter().any(|m| {
                !m.to().iter().all(|v| v.is_finite())
                    || m.feed().is_some_and(|f| !(f.is_finite() && f > 0.0))
            });
            if bad {
                return Err(CamError::new(format!(
                    "The toolpath of {} has a move that is not valid.",
                    op.name
                )));
            }
            if let Some(cycle) = &op.path.cycle {
                let (first, count) = cycle.span();
                if count == 0 || first + count > op.path.moves.len() {
                    return Err(CamError::new(format!(
                        "The cycle of {} does not fit its moves.",
                        op.name
                    )));
                }
            }
        }
        for (i, op) in self.operations.iter().enumerate() {
            let other = self.operations[..i]
                .iter()
                .find(|o| o.tool.number == op.tool.number && o.tool != op.tool);
            if other.is_some() {
                return Err(CamError::new(format!(
                    "Turret station {} holds a different tool in {} than before; give it its own station.",
                    op.tool.number, op.name
                )));
            }
        }
        Ok(())
    }
}

/// The length of a move from `from`.
pub fn move_length(from: PXZ, m: &TurnMove) -> f64 {
    match *m {
        TurnMove::Arc { to, center, cw, .. } => {
            let r = (from[0] - center[0]).hypot(from[1] - center[1]);
            r * arc_sweep(from, to, center, cw).abs()
        }
        _ => {
            let to = m.to();
            (to[0] - from[0]).hypot(to[1] - from[1])
        }
    }
}

/// The angle of `p` about `c` in the drawing (Z right, X up), radians.
fn angle(p: PXZ, c: PXZ) -> f64 {
    (p[0] - c[0]).atan2(p[1] - c[1])
}

/// The signed sweep of an arc (radians, positive counter-clockwise as
/// drawn), a whole turn when its ends meet.
pub fn arc_sweep(from: PXZ, to: PXZ, center: PXZ, cw: bool) -> f64 {
    use std::f64::consts::TAU;
    let mut d = angle(to, center) - angle(from, center);
    if cw {
        while d >= -1e-12 {
            d -= TAU;
        }
        while d < -TAU - 1e-12 {
            d += TAU;
        }
    } else {
        while d <= 1e-12 {
            d += TAU;
        }
        while d > TAU + 1e-12 {
            d -= TAU;
        }
    }
    d
}

/// Points along a move from `from` (its end included, its start not),
/// arcs within `tolerance` of the arc.
pub fn move_points(from: PXZ, m: &TurnMove, tolerance: f64) -> Vec<PXZ> {
    match *m {
        TurnMove::Arc { to, center, cw, .. } => {
            let r = (from[0] - center[0]).hypot(from[1] - center[1]);
            let sweep = arc_sweep(from, to, center, cw);
            let step = if r > tolerance {
                2.0 * (1.0 - tolerance / r).acos()
            } else {
                sweep.abs()
            };
            let n = ((sweep.abs() / step.max(1e-3)).ceil() as usize).clamp(1, 10_000);
            let a0 = angle(from, center);
            (1..=n)
                .map(|k| {
                    if k == n {
                        return to;
                    }
                    let a = a0 + sweep * k as f64 / n as f64;
                    [center[0] + r * a.sin(), center[1] + r * a.cos()]
                })
                .collect()
        }
        _ => vec![m.to()],
    }
}
