// SPDX-License-Identifier: MIT
//! Turning on a 2-axis lathe (mitcad#1869, docs/cam.md, "Turning"): the
//! lathe's own machine data, the turning tools, the part's revolved
//! profile, the operations (facing, roughing and finishing of outside and
//! inside diameters, grooving, parting off, threading), the Fanuc and
//! LinuxCNC turning dialects (mitcad#2425), the program check and the
//! simulation of the turned stock.
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
//! a lathe with a C axis and live tools mills and drills on the part's
//! face and diameter ([`live`], mitcad#2067); with a Y axis on planes off
//! the spindle axis, with a tilting head (a B axis) on tilted planes, and a
//! sub-spindle takes the part over for its back ([`HandOver`]); [`solid`]
//! cuts the whole program out of the bar in 3D (mitcad#2319). Live tools
//! cut the part's back too while the sub-spindle holds it, on its own C
//! axis (mitcad#2375).

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
// Live tools on the C axis (mitcad#2067).
pub mod live;
// The mill-turn's stock in 3D (mitcad#2319).
pub mod solid;
// The turret's body against the main chuck and the fixed zones, the
// machine's parts as Simulate draws them (mitcad#2412).
pub mod drawn;
pub mod turret;

#[cfg(test)]
mod axes_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod tests;
// Live tools on the sub-spindle and its machine data (mitcad#2375).
#[cfg(test)]
mod sub_live_tests;
// The sub-spindle's body and moving zones, a control that mirrors only Z
// there, the main spindle's operations after it (mitcad#2392).
#[cfg(test)]
mod sub_body_tests;
// The sub-spindle drawn in Simulate, the turret's body against it, its own
// profile (mitcad#2405).
#[cfg(test)]
mod sub_turret_tests;
// The fixed and moving zones and the turret drawn in Simulate, the
// turret's body against the main chuck and the fixed zones, a body per
// station, the zones' Y on a lathe with a Y axis (mitcad#2412).
#[cfg(test)]
mod turret_tests;
// LinuxCNC on a lathe (mitcad#2425).
#[cfg(test)]
mod linuxcnc_tests;

pub use check::{
    ProgramCheck, ReadMove, TurnPlacement, check_lathe, check_program, check_program_with,
    sub_face_at, sub_face_shown, sub_spindle_hits,
};
// LinuxCNC on a lathe (mitcad#2425).
pub use check::{Control, check_program_as};
pub use drawn::{TurretShown, ZoneShown, turret_shown, zones_shown};
pub use lathe::{Lathe, LatheAxes, MillTurn, StationBody, SubBody, SubSpindle, TurretBody};
pub use live::{LiveOp, LiveSide, LiveSim, simulate_live};
pub use ops::{
    FaceOp, FinishOp, GrooveOp, PartOp, RoughOp, ThreadOp, TurnSide, face, finish, groove,
    part_off, rough, thread,
};
pub use post::LinuxCncTurning;
pub use post::{DialectInfo, TurnOutput, decimal};
pub use post::{FanucTurning, TurningPost, turning_dialect, turning_dialects, turning_post};
pub use profile::{PV, TurnPart};
pub use solid::{SolidSim, Voxels, simulate_solid};
pub use stock::{TurnStock, TurnedStock, simulate_turning};
pub use tool::{TurningTool, TurningToolKind};
pub use turret::turret_hits;

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
    /// A live tool operation (mitcad#2067): its tool is the station's
    /// ([`TurningToolKind::Live`]), its spindle the live tool's speed and its
    /// `path` empty; the moves are the live operation's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<live::LiveOp>,
    /// It turns the part's back on the sub-spindle, after the program's
    /// hand-over (mitcad#2319), or cuts it there with a live tool, the
    /// sub-spindle's C axis holding it (mitcad#2375): its moves in the
    /// sub-spindle's work coordinates ([`HandOver`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sub: bool,
}

/// The sub-spindle taking the part over (mitcad#2319): it comes up to the
/// part with its chuck open, both spindles synchronised, grips it, the
/// parting off before the first operation on the sub-spindle (when the
/// main spindle's last operation parts off) cuts the part free while both
/// hold it, and the sub-spindle pulls it back. The operations after it
/// turn the part's back in the sub-spindle's work coordinates: the zero on
/// the spindle axis at the part's back (`back_z` in the main spindle's
/// work Z), Z turned round (away from the sub-spindle's chuck), X as on
/// the main spindle: the part turned round about the work X, so Y turns
/// round with Z. Its live tools' C is the angle in these coordinates, from
/// their X towards their Y (mitcad#2375).
///
/// While the sub-spindle holds the part it stays where the hand-over pulled
/// it back to: its jaws' face at `grip_z + withdraw` and a point at `z` of
/// its work coordinates at `back_z + withdraw - z` of the main spindle's
/// (the lathe check's machine places, mitcad#2375); the view draws the
/// sub-spindle's moves where the part was before ([`HandOver::main_z`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandOver {
    /// Where the sub-spindle's jaws' face is when it grips the part, in
    /// the main spindle's work Z; its axis is written there.
    pub grip_z: f64,
    /// How far the sub-spindle pulls the part back from where it gripped.
    pub withdraw: f64,
    /// The synchronised spindles' speed (rpm).
    pub rpm: f64,
    /// The sub-spindle's feed as it comes up to the part (mm/min).
    pub feed: f64,
    /// The sub-spindle's work offset: 1 to 6 for G54 to G59.
    pub work_offset: u8,
    /// The part's back in the main spindle's work Z: the sub-spindle's work
    /// zero.
    pub back_z: f64,
}

impl HandOver {
    /// The main spindle's work Z of a point at `z` in the sub-spindle's.
    pub fn main_z(&self, z: f64) -> f64 {
        self.back_z - z
    }

    /// The main spindle's work coordinates of a point in the sub-spindle's
    /// (mitcad#2375): turned round about the work X at the part's back.
    pub fn main_point(&self, p: [f64; 3]) -> [f64; 3] {
        [p[0], -p[1], self.back_z - p[2]]
    }

    /// The main spindle's work coordinates of a direction in the
    /// sub-spindle's.
    pub fn main_direction(&self, d: [f64; 3]) -> [f64; 3] {
        [d[0], -d[1], -d[2]]
    }

    /// The main spindle's work Z of a point at `z` in the sub-spindle's
    /// while the sub-spindle holds the part where the hand-over left it:
    /// pulled back by `withdraw` (mitcad#2375).
    pub fn held_z(&self, z: f64) -> f64 {
        self.back_z + self.withdraw - z
    }

    pub fn validate(&self) -> Result<(), CamError> {
        let finite = [self.grip_z, self.withdraw, self.rpm, self.feed, self.back_z]
            .iter()
            .all(|v| v.is_finite());
        if !finite || self.withdraw <= 0.0 || self.rpm <= 0.0 || self.feed <= 0.0 {
            return Err(CamError::new(
                "The hand-over needs where the sub-spindle grips, how far it pulls the part back, a speed and a feed above zero.",
            ));
        }
        if !(1..=6).contains(&self.work_offset) {
            return Err(CamError::new(
                "The sub-spindle's work offset must be one of G54 to G59.",
            ));
        }
        Ok(())
    }
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
    /// The lathe's codes of the C axis and the live tools, for live tool
    /// operations (mitcad#2067).
    #[serde(default, skip_serializing_if = "MillTurn::is_default")]
    pub mill_turn: MillTurn,
    /// The sub-spindle's hand-over before the operations on the part's
    /// back (mitcad#2319).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand_over: Option<HandOver>,
}

impl TurningProgram {
    pub fn new(name: &str, change_position: PXZ) -> Self {
        TurningProgram {
            name: name.to_owned(),
            number: program_number(),
            work_offset: 1,
            change_position,
            operations: Vec::new(),
            mill_turn: MillTurn::default(),
            hand_over: None,
        }
    }

    /// The index of the first operation on the sub-spindle.
    pub fn first_sub(&self) -> Option<usize> {
        self.operations.iter().position(|o| o.sub)
    }

    /// Where the hand-over is written: before the main spindle's parting
    /// off that comes last before the first operation on the sub-spindle,
    /// else before that operation; none without one (mitcad#2319).
    pub fn hand_over_at(&self) -> Option<usize> {
        let first = self.first_sub()?;
        self.hand_over.as_ref()?;
        match first.checked_sub(1) {
            Some(last)
                if matches!(
                    self.operations[last].tool.kind,
                    TurningToolKind::Parting { .. }
                ) =>
            {
                Some(last)
            }
            _ => Some(first),
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
        let live_ops = self.operations.iter().any(|o| o.live.is_some());
        if live_ops || self.hand_over.is_some() {
            self.mill_turn.validate().map_err(CamError::new)?;
        }
        // The sub-spindle's operations after the main spindle's, after a
        // hand-over (mitcad#2319); turning or with live tools on its C axis
        // (mitcad#2375).
        if let Some(first) = self.first_sub() {
            let Some(hand_over) = &self.hand_over else {
                return Err(CamError::new(format!(
                    "{} turns on the sub-spindle: the program needs a hand-over to it.",
                    self.operations[first].name
                )));
            };
            hand_over.validate()?;
            if let Some(op) = self.operations[first..].iter().find(|o| !o.sub) {
                return Err(CamError::new(format!(
                    "{} is on the main spindle after the hand-over: put it before the sub-spindle's operations.",
                    op.name
                )));
            }
        }
        for op in &self.operations {
            op.tool.validate().map_err(CamError::new)?;
            op.spindle.validate(&op.name)?;
            // A live tool's operation has its moves in its live part
            // (mitcad#2067).
            let live_tool = op.tool.kind == TurningToolKind::Live;
            match &op.live {
                Some(live) => {
                    if !live_tool || !op.path.moves.is_empty() {
                        return Err(CamError::new(format!(
                            "{} cuts with a live tool: its station must hold one.",
                            op.name
                        )));
                    }
                    if !matches!(op.spindle, TurnSpindle::Rpm { .. }) {
                        return Err(CamError::new(format!(
                            "{}: a live tool turns at a constant speed.",
                            op.name
                        )));
                    }
                    live.validate(&op.name)?;
                }
                None if live_tool => {
                    return Err(CamError::new(format!(
                        "{}: a live tool does not turn; give the operation a turning tool.",
                        op.name
                    )));
                }
                None => {}
            }
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
            // A live station's cutter is part of its tool.
            let cutter = |o: &TurningProgramOp| o.live.as_ref().map(|l| l.cutter.clone());
            let other = self.operations[..i].iter().find(|o| {
                o.tool.number == op.tool.number && (o.tool != op.tool || cutter(o) != cutter(op))
            });
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
