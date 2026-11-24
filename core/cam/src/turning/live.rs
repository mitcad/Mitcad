// SPDX-License-Identifier: MIT
//! Live tools on a lathe with a C axis (mitcad#2067; docs/cam.md, "Live
//! tools on the C axis"): milling and drilling on the part's face and
//! diameter with a driven tool on the turret, the spindle held as a C axis.
//!
//! **The path** of a live operation is a milling toolpath
//! ([`crate::Toolpath`]) of the milling operations, in the frame its side
//! gives ([`LiveSide`]), the tool along the frame's Z:
//!
//! - **On the face** (an axial holder): the work coordinates of the lathe
//!   setup, x and y on the face (they turn with the part), z along the
//!   spindle away from the chuck.
//! - **On the diameter** (a radial holder): the cylinder of a `radius`
//!   about the spindle axis unrolled: x the length round it (its angle
//!   times the radius, from the work X towards the work Y), y the work Z,
//!   z the distance from the axis. A wall's offset is exact on that
//!   cylinder (the floor of what it mills).
//!
//! **C** is the angle about the spindle axis of the point under the tool,
//! from the work X towards the work Y (degrees); the post writes X as a
//! diameter as in turning.
//!
//! **How it is written** (the Fanuc dialect): a path whose cuts all run
//! along the tool (drilling) with the C axis indexed and clamped for each
//! hole, as G83 on the face and G87 on the diameter with
//! `canned_cycles`; any other path on the face in polar coordinate
//! interpolation (G12.1: X a diameter and C the length across it), on the
//! diameter in cylindrical interpolation (G7.1 with the radius: Z and C,
//! arcs with R). Neither takes G0, so the path's rapids are written there
//! as G1 at the operation's `rapid_feed`. Feeds are per minute (G98); the
//! main spindle stops and the C axis is switched on with the lathe's
//! codes ([`super::lathe::MillTurn`]) before the first live operation and
//! off before the next turning one.
//!
//! **On a plane** (mitcad#2319, [`LiveSide::Plane`]) the C axis is indexed
//! and clamped at the plane's angle and the tool stands on the plane: along
//! the spindle (B at 0, the face with the Y axis: `G17`, X a diameter, Y
//! and Z), across it (B at 90, a flat milled with the Y axis: `G19`, X a
//! diameter for the depth, Y and Z) or tilted by a B head (the tilted
//! working plane `G68.2` with `G53.1`, X, Y and Z in its frame, `G69`
//! after). Its paths are written as a mill's: G0 to G3, arcs with I and J
//! (K in G19), drilling as `G83` (`G87` across) with the C axis clamped.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::post::{TurnOutput, TurningPost};
use super::stock::TurnStock;
use super::{TurnPart, TurningProgram, TurningProgramOp, simulate_turning};
use crate::CamError;
use crate::program::{Program, ProgramOp, SpindleDirection};
use crate::stock::{Grid, HeightMap, Simulation, simulate};
use crate::tool::Tool;
use crate::toolpath::{CycleKind, DrillCycle, Move, P3, Toolpath, arc_sweep};

/// Where a live tool cuts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveSide {
    /// On the face, the tool along the spindle axis.
    Face,
    /// On the diameter, the tool towards the axis; the path on the
    /// cylinder of `radius` unrolled.
    Diameter { radius: f64 },
    /// On a plane (mitcad#2319): the C axis at `c` (degrees) and the tool
    /// tilted `b` degrees from the spindle axis towards the machine's X
    /// (0 along the spindle, 90 across it; others need a B head). The path
    /// is in the plane's frame: with the C axis at zero and B at 0 the work
    /// coordinates; B turns it about the machine's Y (z along the tool, y
    /// the Y axis, x towards the chuck at 90), C about the spindle axis.
    Plane { c: f64, b: f64 },
}

/// How a plane is written (mitcad#2319).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneKind {
    /// B at 0: the face with the Y axis (G17).
    Face,
    /// B at 90: a flat with the Y axis (G19).
    Side,
    /// A B head's tilted working plane (G68.2).
    Tilted,
}

/// The plane frame's axes in the machine's frame (C at zero) for a tilt of
/// `b` degrees: x, y and z.
fn plane_axes(b: f64) -> [P3; 3] {
    let (s, c) = b.to_radians().sin_cos();
    [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]]
}

fn rotate_z(p: P3, degrees: f64) -> P3 {
    let (s, c) = degrees.to_radians().sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]]
}

impl LiveSide {
    /// The lathe's axes at a point of the path: `[x (a radius), c
    /// (degrees), z]`; C is none on the axis, where any angle will do. On
    /// a plane x is the machine's X, off the axis by the Y axis.
    pub fn axes(&self, p: P3) -> ([f64; 2], Option<f64>) {
        match *self {
            LiveSide::Face => {
                let r = p[0].hypot(p[1]);
                let c = (r > 1e-9).then(|| p[1].atan2(p[0]).to_degrees());
                ([r, p[2]], c)
            }
            LiveSide::Diameter { radius } => ([p[2], p[1]], Some((p[0] / radius).to_degrees())),
            LiveSide::Plane { c, .. } => {
                let m = self.machine(p);
                ([m[0], m[2]], Some(c))
            }
        }
    }

    /// A point of the path in the machine's axes with the C axis where it
    /// cuts it: `[x, y, z]`, x from the spindle axis, y the Y axis's, z
    /// the work Z.
    pub fn machine(&self, p: P3) -> P3 {
        match *self {
            LiveSide::Face => [p[0].hypot(p[1]), 0.0, p[2]],
            LiveSide::Diameter { .. } => [p[2], 0.0, p[1]],
            LiveSide::Plane { b, .. } => {
                let a = plane_axes(b);
                std::array::from_fn(|k| p[0] * a[0][k] + p[1] * a[1][k] + p[2] * a[2][k])
            }
        }
    }

    /// A point in the work coordinates of the lathe setup.
    pub fn to_work(&self, p: P3) -> P3 {
        match *self {
            LiveSide::Face => p,
            LiveSide::Diameter { radius } => {
                let a = p[0] / radius;
                [p[2] * a.cos(), p[2] * a.sin(), p[1]]
            }
            LiveSide::Plane { c, .. } => rotate_z(self.machine(p), c),
        }
    }

    /// A work point in a plane's frame (the others' work points as they
    /// are).
    pub fn from_work(&self, w: P3) -> P3 {
        match *self {
            LiveSide::Plane { c, b } => {
                let m = rotate_z(w, -c);
                let a = plane_axes(b);
                std::array::from_fn(|k| m[0] * a[k][0] + m[1] * a[k][1] + m[2] * a[k][2])
            }
            _ => w,
        }
    }

    /// The tool's direction at a point of the path in work coordinates,
    /// from its tip up its shank.
    pub fn tool_axis(&self, p: P3) -> P3 {
        match *self {
            LiveSide::Face => [0.0, 0.0, 1.0],
            LiveSide::Diameter { radius } => {
                let a = p[0] / radius;
                [a.cos(), a.sin(), 0.0]
            }
            LiveSide::Plane { c, b } => rotate_z(plane_axes(b)[2], c),
        }
    }

    /// How a plane is written; none for the face and the diameter.
    pub fn plane_kind(&self) -> Option<PlaneKind> {
        let LiveSide::Plane { b, .. } = *self else {
            return None;
        };
        Some(if b.abs() < 1e-9 {
            PlaneKind::Face
        } else if (b - 90.0).abs() < 1e-9 {
            PlaneKind::Side
        } else {
            PlaneKind::Tilted
        })
    }

    /// Whether it is the same side (the same cylinder, the same plane).
    pub fn same(&self, other: &LiveSide) -> bool {
        match (self, other) {
            (LiveSide::Face, LiveSide::Face) => true,
            (LiveSide::Diameter { radius: a }, LiveSide::Diameter { radius: b }) => {
                (a - b).abs() < 1e-9
            }
            (LiveSide::Plane { c: c1, b: b1 }, LiveSide::Plane { c: c2, b: b2 }) => {
                (c1 - c2).abs() < 1e-9 && (b1 - b2).abs() < 1e-9
            }
            _ => false,
        }
    }

    /// The side in words, for messages.
    pub fn words(&self) -> &'static str {
        match self.plane_kind() {
            None if *self == LiveSide::Face => "on the face",
            None => "on the diameter",
            Some(PlaneKind::Face) => "on the face with the Y axis",
            Some(PlaneKind::Side) => "on a flat with the Y axis",
            Some(PlaneKind::Tilted) => "on a tilted plane",
        }
    }

    /// The short name the simulation's summary gives it.
    pub fn name(&self) -> &'static str {
        match self.plane_kind() {
            None if *self == LiveSide::Face => "face",
            None => "diameter",
            Some(PlaneKind::Face) => "face plane",
            Some(PlaneKind::Side) => "flat",
            Some(PlaneKind::Tilted) => "tilted plane",
        }
    }
}

/// A live tool operation of a turning program: in a
/// [`TurningProgramOp`] whose tool is the station's
/// ([`super::TurningToolKind::Live`]) and whose spindle is the live
/// tool's constant speed ([`super::TurnSpindle::Rpm`]) in its
/// `direction`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveOp {
    pub side: LiveSide,
    /// The milling tool on the station: its shape and size.
    pub cutter: Tool,
    /// The milling toolpath in the side's frame; feeds per minute.
    pub path: Toolpath,
    /// The feed (mm/min) the path's rapids are written at where the C
    /// axis interpolates and G0 is not taken.
    pub rapid_feed: f64,
    /// The tool sits in the B head, which tilts to the side before it cuts
    /// (mitcad#2319).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub head: bool,
}

impl LiveOp {
    pub fn validate(&self, name: &str) -> Result<(), CamError> {
        self.cutter
            .validate()
            .map_err(|e| CamError::new(format!("{name}: {e}")))?;
        if let LiveSide::Diameter { radius } = self.side
            && !(radius.is_finite() && radius > 0.0)
        {
            return Err(CamError::new(format!(
                "{name}: the cylinder it mills on needs a radius above zero."
            )));
        }
        if let LiveSide::Plane { c, b } = self.side
            && !(c.is_finite() && b.is_finite() && b.abs() <= 180.0)
        {
            return Err(CamError::new(format!(
                "{name}: the plane's angles must be numbers, B within half a turn."
            )));
        }
        if self.side.plane_kind() == Some(PlaneKind::Tilted) && !self.head {
            return Err(CamError::new(format!(
                "{name}: a tilted plane needs the tool in the B head."
            )));
        }
        if !(self.rapid_feed.is_finite() && self.rapid_feed > 0.0) {
            return Err(CamError::new(format!(
                "{name}: the feed of its rapids must be above zero."
            )));
        }
        if !matches!(self.path.moves.first(), None | Some(Move::Rapid { .. })) {
            return Err(CamError::new(format!(
                "The toolpath of {name} must start with a rapid."
            )));
        }
        let bad = self.path.moves.iter().any(|m| {
            !m.to().iter().all(|v| v.is_finite())
                || match *m {
                    Move::Rapid { .. } => false,
                    Move::Line { feed, .. } | Move::Arc { feed, .. } => {
                        !(feed.is_finite() && feed > 0.0)
                    }
                }
        });
        if bad {
            return Err(CamError::new(format!(
                "The toolpath of {name} has a move that is not valid."
            )));
        }
        Ok(())
    }

    /// Whether it only cuts along the tool (drilling): written with the C
    /// axis indexed and clamped.
    pub fn indexed(&self) -> bool {
        let mut at: Option<P3> = None;
        for m in &self.path.moves {
            let to = m.to();
            match (m, at) {
                (Move::Rapid { .. }, _) => {}
                (Move::Arc { .. }, _) | (_, None) => return false,
                (Move::Line { .. }, Some(f)) => {
                    if (to[0] - f[0]).abs() > 1e-9 || (to[1] - f[1]).abs() > 1e-9 {
                        return false;
                    }
                }
            }
            at = Some(to);
        }
        true
    }

    /// Points along its moves (each move's end, arcs within `tolerance`),
    /// each with whether it is a rapid's and its feed (mm/min).
    pub fn points(&self, tolerance: f64) -> Vec<(P3, Option<f64>)> {
        let mut out = Vec::new();
        let mut at: Option<P3> = None;
        for m in &self.path.moves {
            let feed = match *m {
                Move::Rapid { .. } => None,
                Move::Line { feed, .. } | Move::Arc { feed, .. } => Some(feed),
            };
            match at {
                Some(from) => {
                    for p in crate::toolpath::move_points(from, m, tolerance) {
                        out.push((p, feed));
                    }
                }
                None => out.push((m.to(), feed)),
            }
            at = Some(m.to());
        }
        out
    }
}

/// The modal words of the blocks being written in a live operation.
#[derive(Clone, Default)]
struct Modal {
    motion: Option<&'static str>,
    x: Option<String>,
    c: Option<String>,
    z: Option<String>,
    y: Option<String>,
    feed: Option<String>,
}

impl Modal {
    /// A block's words from the motion, the axes (each left out when it
    /// does not change, except the first `always`: an arc's plane, which a
    /// whole turn names though it ends where it starts), the arc's words
    /// and the feed; none when nothing moves.
    fn block(
        &mut self,
        motion: &'static str,
        axes: [(char, Option<String>); 3],
        always: usize,
        arc: &[String],
        feed: Option<String>,
    ) -> Option<String> {
        let mut words: Vec<String> = Vec::new();
        let mut moved = false;
        for (k, (letter, value)) in axes.into_iter().enumerate() {
            let Some(value) = value else { continue };
            let slot = match letter {
                'X' => &mut self.x,
                'C' => &mut self.c,
                'Y' => &mut self.y,
                _ => &mut self.z,
            };
            let changed = slot.as_deref() != Some(value.as_str());
            if changed || k < always {
                moved = true;
                words.push(format!("{letter}{value}"));
            }
            *slot = Some(value);
        }
        if !moved {
            return None;
        }
        if self.motion != Some(motion) {
            words.insert(0, motion.to_owned());
        }
        self.motion = Some(motion);
        words.extend(arc.iter().cloned());
        if let Some(f) = feed
            && self.feed.as_deref() != Some(f.as_str())
        {
            words.push(format!("F{f}"));
            self.feed = Some(f);
        }
        Some(words.join(" "))
    }
}

/// How the live operation's moves are written.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// G0 and G1 with the C axis indexed.
    Indexed,
    /// Polar coordinate interpolation on the face.
    Polar,
    /// Cylindrical interpolation on the diameter.
    Cylindrical(f64),
    /// On a plane with the C axis clamped (mitcad#2319).
    Plane(PlaneKind),
}

/// The state of the C axis and the live spindle between operations.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LiveState {
    /// The C axis is on (the main spindle is a C axis).
    pub c_axis: bool,
    pub clamped: bool,
    /// The live tool turns at this speed and sense.
    pub running: Option<(f64, SpindleDirection)>,
    /// Where the B head is tilted (mitcad#2319); none when not known.
    pub b: Option<f64>,
}

/// The words of a point of a plane's path (mitcad#2319): the plane's two
/// axes first (an arc names them), then the depth's.
fn plane_words(out: &TurnOutput, kind: PlaneKind, q: P3) -> [(char, Option<String>); 3] {
    match kind {
        PlaneKind::Face => [
            ('X', Some(out.dia(q[0]))),
            ('Y', Some(out.len(q[1]))),
            ('Z', Some(out.len(q[2]))),
        ],
        // x towards the chuck, y the Y axis, z the machine's X.
        PlaneKind::Side => [
            ('Y', Some(out.len(q[1]))),
            ('Z', Some(out.len(-q[0]))),
            ('X', Some(out.dia(q[2]))),
        ],
        PlaneKind::Tilted => [
            ('X', Some(out.len(q[0]))),
            ('Y', Some(out.len(q[1]))),
            ('Z', Some(out.len(q[2]))),
        ],
    }
}

/// The arc words of a plane (mitcad#2319): from the start to the centre,
/// I and J in G17 and the tilted plane, J and K in G19.
fn plane_arc(out: &TurnOutput, kind: PlaneKind, from: P3, center: [f64; 2]) -> [String; 2] {
    let (dx, dy) = (center[0] - from[0], center[1] - from[1]);
    match kind {
        PlaneKind::Side => [format!("J{}", out.len(dy)), format!("K{}", out.len(-dx))],
        _ => [format!("I{}", out.len(dx)), format!("J{}", out.len(dy))],
    }
}

/// Tilts the B head to `b` (degrees) with its clamp, when it is elsewhere
/// (mitcad#2319).
fn tilt_head(out: &mut TurnOutput, program: &TurningProgram, state: &mut LiveState, b: f64) {
    if state.b.is_some_and(|now| (now - b).abs() < 1e-9) {
        return;
    }
    let codes = &program.mill_turn;
    out.line(codes.b_unclamp.trim());
    out.line(format!("G0 B{}", TurnOutput::angle(b)));
    out.line(codes.b_clamp.trim());
    state.b = Some(b);
}

/// The legs of a rapid as the posts write them: along the tool first
/// when it rises, across first when it sinks (`from` unknown: sinking).
fn rapid_legs(from: Option<P3>, to: P3) -> Vec<(bool, P3)> {
    let Some(f) = from else {
        return vec![(false, to), (true, to)];
    };
    let across = (to[0] - f[0]).abs() > 1e-12 || (to[1] - f[1]).abs() > 1e-12;
    let along = (to[2] - f[2]).abs() > 1e-12;
    match (across, along) {
        (true, true) if to[2] > f[2] => vec![(true, [f[0], f[1], to[2]]), (false, to)],
        (true, true) => vec![(false, [to[0], to[1], f[2]]), (true, to)],
        _ => vec![(along, to)],
    }
}

/// Writes a live operation's blocks after its tool change: the main
/// spindle stopped, the C axis on, feeds per minute, the live tool
/// started (the caller writes the coolant after), then its moves.
pub fn start(
    out: &mut TurnOutput,
    dialect: &dyn TurningPost,
    program: &TurningProgram,
    op: &TurningProgramOp,
    state: &mut LiveState,
) -> Result<(), CamError> {
    let codes = &program.mill_turn;
    if !dialect.mills() {
        return Err(CamError::new(format!(
            "{} has no live tools on a C axis here.",
            dialect.label()
        )));
    }
    if !state.c_axis {
        for line in &codes.c_axis_on {
            out.line(line.trim());
        }
        state.c_axis = true;
        state.clamped = false;
        dialect.feed_per_minute(out);
    }
    let rpm = op.spindle.rpm_at(0.0);
    if state.running != Some((rpm, op.direction)) {
        if state.running.is_some() {
            out.line(codes.live_stop.trim());
        }
        out.line(codes.live_start(rpm, op.direction == SpindleDirection::Cw));
        state.running = Some((rpm, op.direction));
    }
    Ok(())
}

/// Stops the live tool and switches the C axis off, back to turning with
/// feeds per revolution; nothing when the C axis is off.
pub fn stop(
    out: &mut TurnOutput,
    dialect: &dyn TurningPost,
    program: &TurningProgram,
    state: &mut LiveState,
) {
    let codes = &program.mill_turn;
    if state.running.take().is_some() {
        out.line(codes.live_stop.trim());
    }
    if state.c_axis {
        if state.clamped {
            out.line(codes.c_unclamp.trim());
            state.clamped = false;
        }
        for line in &codes.c_axis_off {
            out.line(line.trim());
        }
        state.c_axis = false;
        dialect.feed_per_rev(out);
    }
}

/// Stops the live tool at a tool change.
pub fn stop_tool(out: &mut TurnOutput, program: &TurningProgram, state: &mut LiveState) {
    if state.running.take().is_some() {
        out.line(program.mill_turn.live_stop.trim());
    }
}

/// Writes a live operation's moves (`cycles`: its drilling cycles as the
/// dialect's canned cycles where it has them).
pub fn moves(
    out: &mut TurnOutput,
    dialect: &dyn TurningPost,
    program: &TurningProgram,
    live: &LiveOp,
    cycles: bool,
    state: &mut LiveState,
) -> Result<(), CamError> {
    let codes = &program.mill_turn;
    let side = live.side;
    let mode = match (side.plane_kind(), side) {
        (Some(kind), _) => Mode::Plane(kind),
        (None, _) if live.indexed() => Mode::Indexed,
        (None, LiveSide::Diameter { radius }) => Mode::Cylindrical(radius),
        (None, _) => Mode::Polar,
    };
    let moves = &live.path.moves;
    let Some(first) = moves.first() else {
        return Ok(());
    };
    out.forget();
    // A tool in the B head tilts to the side first; G53.1 tilts it to a
    // tilted plane (mitcad#2319).
    if live.head && mode != Mode::Plane(PlaneKind::Tilted) {
        let b = match side {
            LiveSide::Face => 0.0,
            LiveSide::Diameter { .. } => 90.0,
            LiveSide::Plane { b, .. } => b,
        };
        tilt_head(out, program, state, b);
    }
    let mut modal = Modal::default();
    let rapid_feed = out.per_min(live.rapid_feed);
    // The C axis turns to where the path starts, free.
    let unclamp = |out: &mut TurnOutput, state: &mut LiveState| {
        if state.clamped {
            out.line(codes.c_unclamp.trim());
            state.clamped = false;
        }
    };
    let mut at: Option<P3> = None;
    match mode {
        Mode::Indexed => {}
        Mode::Polar => {
            // From C at zero to the start's radius: there the polar
            // plane's x is the radius and its y zero.
            unclamp(out, state);
            let p = first.to();
            let r = p[0].hypot(p[1]);
            out.line(format!("G0 C{}", TurnOutput::angle(0.0)));
            out.line(format!("G0 X{} Z{}", out.dia(r), out.len(p[2])));
            dialect.polar(out, true);
            modal.x = Some(out.dia(r));
            modal.c = Some(out.len(0.0));
            modal.z = Some(out.len(p[2]));
            at = Some([r, 0.0, p[2]]);
        }
        Mode::Cylindrical(radius) => {
            unclamp(out, state);
            let p = first.to();
            let c = TurnOutput::angle((p[0] / radius).to_degrees());
            out.line(format!("G0 X{}", out.dia(p[2])));
            out.line(format!("G0 Z{} C{c}", out.len(p[1])));
            dialect.cylindrical(out, Some(radius));
            modal.x = Some(out.dia(p[2]));
            modal.c = Some(c);
            modal.z = Some(out.len(p[1]));
            at = Some(p);
        }
        Mode::Plane(kind) => {
            // The C axis to the plane's angle and clamped there, then the
            // plane: G17, G19, or the tilted working plane with the head
            // tilted to it (mitcad#2319).
            let LiveSide::Plane { c, b } = side else {
                unreachable!("a plane's mode is a plane's side");
            };
            unclamp(out, state);
            out.line(format!("G0 C{}", TurnOutput::angle(c)));
            out.line(codes.c_clamp.trim());
            state.clamped = true;
            match kind {
                PlaneKind::Face => out.line("G17"),
                PlaneKind::Side => out.line("G19"),
                PlaneKind::Tilted => {
                    out.line(codes.b_unclamp.trim());
                    out.line(format!(
                        "G68.2 X0. Y0. Z0. I90. J{} K-90.",
                        TurnOutput::angle(b)
                    ));
                    out.line("G53.1");
                    out.line(codes.b_clamp.trim());
                    state.b = Some(b);
                }
            }
        }
    }
    // The words of a point in the mode: X, C and Z (X, Y and Z on a
    // plane).
    let axes = |out: &TurnOutput, p: P3, modal: &Modal| -> [(char, Option<String>); 3] {
        match mode {
            Mode::Plane(kind) => plane_words(out, kind, p),
            Mode::Polar => [
                ('X', Some(out.dia(p[0]))),
                ('C', Some(out.len(p[1]))),
                ('Z', Some(out.len(p[2]))),
            ],
            Mode::Cylindrical(radius) => [
                ('Z', Some(out.len(p[1]))),
                ('C', Some(TurnOutput::angle((p[0] / radius).to_degrees()))),
                ('X', Some(out.dia(p[2]))),
            ],
            Mode::Indexed => {
                let (xz, c) = side.axes(p);
                // On the axis any angle will do: C stays.
                let c = c.map(TurnOutput::angle).or_else(|| modal.c.clone());
                match side {
                    LiveSide::Face | LiveSide::Plane { .. } => [
                        ('X', Some(out.dia(xz[0]))),
                        ('C', c),
                        ('Z', Some(out.len(xz[1]))),
                    ],
                    LiveSide::Diameter { .. } => [
                        ('Z', Some(out.len(xz[1]))),
                        ('C', c),
                        ('X', Some(out.dia(xz[0]))),
                    ],
                }
            }
        }
    };
    // The axes of a leg: along the tool alone, or across alone.
    let leg_axes = |out: &TurnOutput, p: P3, along: bool, modal: &Modal| {
        let mut a = axes(out, p, modal);
        let depth = match (mode, side) {
            (Mode::Polar, _)
            | (Mode::Indexed, LiveSide::Face)
            | (Mode::Plane(PlaneKind::Face | PlaneKind::Tilted), _) => 'Z',
            _ => 'X',
        };
        for (letter, value) in a.iter_mut() {
            if (*letter == depth) != along {
                *value = None;
            }
        }
        a
    };
    let skip: Vec<&DrillCycle> = if cycles && matches!(mode, Mode::Indexed | Mode::Plane(_)) {
        live.path
            .cycles
            .iter()
            .filter(|c| c.kind != CycleKind::ChipBreak)
            .collect()
    } else {
        Vec::new()
    };
    let across =
        matches!(side, LiveSide::Diameter { .. }) || side.plane_kind() == Some(PlaneKind::Side);
    let code = dialect.drill_cycle_code(across);
    let mut i = 0;
    while i < moves.len() {
        if let (Some(cycle), Some(code)) = (skip.iter().find(|c| c.first == i), code) {
            if mode == Mode::Indexed {
                unclamp(out, state);
            }
            write_cycle(out, code, side, cycle, &mut modal);
            let last = cycle.holes.last().copied().unwrap_or([0.0, 0.0]);
            at = Some([last[0], last[1], cycle.clearance]);
            modal.motion = None;
            modal.feed = None;
            i = cycle.first + cycle.count.max(1);
            continue;
        }
        let m = &moves[i];
        i += 1;
        match *m {
            Move::Rapid { to } => {
                // In the tilted working plane the tool's place is not known
                // at first: along the tool to the start's height, then
                // across (mitcad#2319).
                let legs = if at.is_none() && mode == Mode::Plane(PlaneKind::Tilted) {
                    vec![(true, to), (false, to)]
                } else {
                    rapid_legs(at, to)
                };
                for (along, p) in legs {
                    let a = leg_axes(out, p, along, &modal);
                    let (motion, feed) = match mode {
                        Mode::Indexed | Mode::Plane(_) => ("G0", None),
                        _ => ("G1", Some(rapid_feed.clone())),
                    };
                    if mode == Mode::Indexed
                        && !along
                        && a.iter().any(|(l, v)| *l == 'C' && v.is_some())
                    {
                        let c = a
                            .iter()
                            .find(|(l, _)| *l == 'C')
                            .and_then(|(_, v)| v.clone());
                        if c != modal.c {
                            unclamp(out, state);
                        }
                    }
                    if let Some(text) = modal.block(motion, a, 0, &[], feed) {
                        out.line(text);
                    }
                }
            }
            Move::Line { to, feed } => {
                if mode == Mode::Indexed && !state.clamped {
                    out.line(codes.c_clamp.trim());
                    state.clamped = true;
                }
                let a = axes(out, to, &modal);
                if let Some(text) = modal.block("G1", a, 0, &[], Some(out.per_min(feed))) {
                    out.line(text);
                }
            }
            Move::Arc {
                to,
                center,
                ccw,
                feed,
            } => {
                let from = at.unwrap_or(to);
                let motion = if ccw { "G3" } else { "G2" };
                match mode {
                    Mode::Plane(kind) => {
                        // The plane's two axes and its arc words; the
                        // depth only when it changes (a helix).
                        let words = plane_words(out, kind, to);
                        let arc = plane_arc(out, kind, from, center);
                        if let Some(text) =
                            modal.block(motion, words, 2, &arc, Some(out.per_min(feed)))
                        {
                            out.line(text);
                        }
                    }
                    Mode::Cylindrical(_) => {
                        // R alone, at most a third of a turn a block.
                        let r = (from[0] - center[0]).hypot(from[1] - center[1]);
                        let sweep = arc_sweep([from[0], from[1]], [to[0], to[1]], center, ccw);
                        let parts =
                            ((sweep.abs() / (2.0 * PI / 3.0)) - 1e-9).ceil().max(1.0) as usize;
                        let a0 = (from[1] - center[1]).atan2(from[0] - center[0]);
                        for k in 1..=parts {
                            let t = k as f64 / parts as f64;
                            let p = if k == parts {
                                to
                            } else {
                                let a = a0 + sweep * t;
                                [
                                    center[0] + r * a.cos(),
                                    center[1] + r * a.sin(),
                                    from[2] + (to[2] - from[2]) * t,
                                ]
                            };
                            let words = axes(out, p, &modal);
                            let arc = [format!("R{}", out.len(r))];
                            if let Some(text) =
                                modal.block(motion, words, 2, &arc, Some(out.per_min(feed)))
                            {
                                out.line(text);
                            }
                        }
                    }
                    _ => {
                        // Polar: I and J from the start to the centre; Z
                        // only when it changes (a helix).
                        let words = axes(out, to, &modal);
                        let arc = [
                            format!("I{}", out.len(center[0] - from[0])),
                            format!("J{}", out.len(center[1] - from[1])),
                        ];
                        if let Some(text) =
                            modal.block(motion, words, 2, &arc, Some(out.per_min(feed)))
                        {
                            out.line(text);
                        }
                    }
                }
            }
        }
        at = Some(m.to());
    }
    match mode {
        Mode::Indexed => {}
        Mode::Polar => dialect.polar(out, false),
        Mode::Cylindrical(_) => dialect.cylindrical(out, None),
        Mode::Plane(kind) => {
            if kind == PlaneKind::Tilted {
                out.line("G69");
            }
            out.line("G18");
        }
    }
    out.forget();
    Ok(())
}

/// Writes a drilling cycle: the first hole with the cycle's words, the
/// others by their place, then the cancel. The tool is at the clearance
/// above the first hole; R is the retract's distance from there.
fn write_cycle(
    out: &mut TurnOutput,
    code: &'static str,
    side: LiveSide,
    cycle: &DrillCycle,
    modal: &mut Modal,
) {
    let peck = if cycle.kind == CycleKind::Peck && cycle.peck > 0.0 {
        format!(" Q{}", out.increments(cycle.peck))
    } else {
        String::new()
    };
    for (k, hole) in cycle.holes.iter().enumerate() {
        let (xz, c) = side.axes([hole[0], hole[1], cycle.bottom]);
        let c = c.map(TurnOutput::angle).unwrap_or_else(|| "0.".to_owned());
        // On a plane (mitcad#2319) the C axis is clamped at its angle and
        // the holes are placed in the plane's axes.
        let place = match (side.plane_kind(), side) {
            (Some(PlaneKind::Face), _) => {
                format!("X{} Y{}", out.dia(hole[0]), out.len(hole[1]))
            }
            (Some(PlaneKind::Side), _) => {
                format!("Z{} Y{}", out.len(-hole[0]), out.len(hole[1]))
            }
            (Some(PlaneKind::Tilted), _) => {
                format!("X{} Y{}", out.len(hole[0]), out.len(hole[1]))
            }
            (None, LiveSide::Diameter { .. }) => format!("Z{} C{c}", out.len(xz[1])),
            (None, _) => format!("X{} C{c}", out.dia(xz[0])),
        };
        if k == 0 {
            let depth = match side.plane_kind() {
                Some(PlaneKind::Side) => format!("X{}", out.dia(cycle.bottom)),
                Some(PlaneKind::Tilted) => format!("Z{}", out.len(cycle.bottom)),
                _ => match side {
                    LiveSide::Diameter { .. } => format!("X{}", out.dia(cycle.bottom)),
                    _ => format!("Z{}", out.len(cycle.bottom)),
                },
            };
            out.line(format!(
                "{code} {place} {depth} R{}{peck} F{}",
                out.len(cycle.retract - cycle.clearance),
                out.per_min(cycle.feed)
            ));
        } else {
            out.line(place);
        }
    }
    out.line("G80");
    *modal = Modal::default();
}

/// The turned stock's face as a height map over `grid` (work x and y):
/// at each node the highest Z with material at its radius; `bottom` where
/// there is none.
pub fn face_map(stock: &TurnStock, grid: Grid, bottom: f64) -> HeightMap {
    let mut map = HeightMap::flat(grid, bottom, bottom);
    let outer = stock.outer.iter().copied().fold(0.0, f64::max);
    if stock.z.is_empty() || outer <= 0.0 {
        return map;
    }
    // The face's height by radius, on a fine row of radii.
    let dr = (grid.step[0].min(grid.step[1]) / 2.0).max(1e-3);
    let n = ((outer / dr).ceil() as usize).clamp(1, 200_000);
    let heights: Vec<f64> = (0..=n)
        .map(|k| {
            let r = k as f64 * dr;
            (0..stock.z.len())
                .rev()
                .find(|&i| stock.inner[i] <= r && r < stock.outer[i])
                .map_or(bottom, |i| stock.z[i])
        })
        .collect();
    for j in 0..grid.ny {
        for i in 0..grid.nx {
            let r = grid.x(i).hypot(grid.y(j));
            let k = (r / dr).round() as usize;
            if let Some(h) = heights.get(k) {
                let index = grid.index(i, j);
                map.z[index] = h.max(bottom);
            }
        }
    }
    map
}

/// The turned stock's outside unrolled: over `grid` (the length round
/// the cylinder, work Z) the outer radius at each node's Z; zero where
/// there is no material.
pub fn diameter_map(stock: &TurnStock, grid: Grid) -> HeightMap {
    let mut map = HeightMap::flat(grid, 0.0, 0.0);
    if stock.z.len() < 2 {
        return map;
    }
    let z0 = stock.z[0];
    let step = stock.z[1] - stock.z[0];
    for j in 0..grid.ny {
        let z = grid.y(j);
        let k = ((z - z0) / step).round();
        if k < 0.0 || k as usize >= stock.z.len() {
            continue;
        }
        let k = k as usize;
        let r = if stock.outer[k] > stock.inner[k] {
            stock.outer[k]
        } else {
            0.0
        };
        for i in 0..grid.nx {
            let index = grid.index(i, j);
            map.z[index] = r;
        }
    }
    map
}

/// The turned stock seen along a plane's tool (mitcad#2319): over `grid`
/// (the plane's x and y) the highest z of the plane's frame with material,
/// found in steps of `step` down from the bar's outside; the lowest z the
/// bar reaches where there is none.
pub fn plane_map(stock: &TurnStock, side: LiveSide, grid: Grid, step: f64) -> HeightMap {
    let outer = stock.outer.iter().copied().fold(0.0, f64::max);
    let (z_lo, z_hi) = match (stock.z.first(), stock.z.last()) {
        (Some(&a), Some(&b)) => (a, b),
        _ => (0.0, 0.0),
    };
    // The bar's reach along the plane's z.
    let mut top = f64::NEG_INFINITY;
    let mut bottom = f64::INFINITY;
    for x in [-outer, outer] {
        for y in [-outer, outer] {
            for z in [z_lo, z_hi] {
                let q = side.from_work([x, y, z]);
                top = top.max(q[2]);
                bottom = bottom.min(q[2]);
            }
        }
    }
    let mut map = HeightMap::flat(grid, bottom, bottom);
    if stock.z.len() < 2 || outer <= 0.0 {
        return map;
    }
    let dz = stock.z[1] - stock.z[0];
    let solid = |w: P3| {
        let k = ((w[2] - z_lo) / dz).round();
        if k < 0.0 || k as usize >= stock.z.len() {
            return false;
        }
        let k = k as usize;
        let r = w[0].hypot(w[1]);
        stock.inner[k] <= r && r < stock.outer[k]
    };
    // Coarse steps down, then halving between the last empty one and the
    // first with material.
    let coarse = (step * 4.0).max(1e-3);
    let n = (((top - bottom) / coarse).ceil() as usize).clamp(1, 400);
    let at = |k: usize| top - (top - bottom) * k as f64 / n as f64;
    for j in 0..grid.ny {
        for i in 0..grid.nx {
            let (x, y) = (grid.x(i), grid.y(j));
            let inside = |z: f64| solid(side.to_work([x, y, z]));
            let Some(k) = (0..=n).find(|&k| inside(at(k))) else {
                continue;
            };
            let mut z = at(k);
            if k > 0 {
                let (mut empty, mut full) = (at(k - 1), z);
                for _ in 0..16 {
                    let mid = 0.5 * (empty + full);
                    if inside(mid) {
                        full = mid;
                    } else {
                        empty = mid;
                    }
                }
                z = full;
            }
            let index = grid.index(i, j);
            map.z[index] = z;
        }
    }
    map
}

/// The live operations of one side (and one cylinder) simulated together.
#[derive(Clone, Debug)]
pub struct LiveSim {
    pub side: LiveSide,
    /// The program's operations it cut, in order.
    pub operations: Vec<usize>,
    /// The stock before them: as the turning before the first of them left
    /// it.
    pub initial: HeightMap,
    /// Their cut: the rapids' findings with the program's operation
    /// indices.
    pub simulation: Simulation,
}

/// Cuts the program's live operations out of the turned stock: those on
/// the face on a height map of the face, those on each cylinder on its
/// unrolled map, each map as the turning operations before the first of
/// them left the stock (`z_min`, `step` as [`simulate_turning`] takes
/// them), its nodes `finest` apart when it can be with at most
/// `max_nodes`.
pub fn simulate_live(
    part: &TurnPart,
    program: &TurningProgram,
    z_min: f64,
    step: f64,
    finest: f64,
    max_nodes: usize,
) -> Vec<LiveSim> {
    // The groups in the order they first come.
    let mut groups: Vec<(LiveSide, Vec<usize>)> = Vec::new();
    for (i, op) in program.operations.iter().enumerate() {
        let Some(live) = &op.live else { continue };
        match groups.iter_mut().find(|(s, _)| s.same(&live.side)) {
            Some((_, ops)) => ops.push(i),
            None => groups.push((live.side, vec![i])),
        }
    }
    let mut out = Vec::new();
    for (side, ops) in groups {
        let first = ops[0];
        let before = TurningProgram {
            operations: program.operations[..first]
                .iter()
                .filter(|o| o.live.is_none())
                .cloned()
                .collect(),
            ..program.clone()
        };
        let turned = simulate_turning(part, &before, z_min, step, 0.0).stock;
        let lives: Vec<&LiveOp> = ops
            .iter()
            .filter_map(|&i| program.operations[i].live.as_ref())
            .collect();
        let widest = lives.iter().map(|l| l.cutter.diameter).fold(0.0, f64::max);
        let initial = match side {
            LiveSide::Face => {
                let r = part.stock_radius + widest;
                let grid = Grid::for_stock([-r, -r], [r, r], finest, max_nodes);
                face_map(&turned, grid, z_min)
            }
            LiveSide::Diameter { .. } => {
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                for l in &lives {
                    if let Some((a, b)) = l.path.bounds() {
                        lo = lo.min(a[0]);
                        hi = hi.max(b[0]);
                    }
                }
                if !lo.is_finite() {
                    continue;
                }
                let grid = Grid::for_stock(
                    [lo - widest, z_min],
                    [hi + widest, part.stock_face],
                    finest,
                    max_nodes,
                );
                diameter_map(&turned, grid)
            }
            // A plane's map over its paths (mitcad#2319).
            LiveSide::Plane { .. } => {
                let mut lo = [f64::INFINITY; 2];
                let mut hi = [f64::NEG_INFINITY; 2];
                for l in &lives {
                    if let Some((a, b)) = l.path.bounds() {
                        for k in 0..2 {
                            lo[k] = lo[k].min(a[k]);
                            hi[k] = hi[k].max(b[k]);
                        }
                    }
                }
                if !lo[0].is_finite() {
                    continue;
                }
                let grid = Grid::for_stock(
                    [lo[0] - widest, lo[1] - widest],
                    [hi[0] + widest, hi[1] + widest],
                    finest,
                    max_nodes,
                );
                plane_map(&turned, side, grid, finest)
            }
        };
        let mut milling = Program::new(&program.name, f64::NEG_INFINITY);
        for &i in &ops {
            let op = &program.operations[i];
            let Some(live) = &op.live else { continue };
            milling.operations.push(ProgramOp {
                name: op.name.clone(),
                tool: live.cutter.clone(),
                spindle_rpm: op.spindle.rpm_at(0.0),
                spindle: op.direction,
                coolant: op.coolant,
                toolpath: live.path.clone(),
            });
        }
        let mut simulation = simulate(&milling, initial.clone(), None, |_, _| {});
        for cut in &mut simulation.rapid_cuts {
            cut.operation = ops[cut.operation];
        }
        if let Some(last) = &mut simulation.last {
            last.operation = ops[last.operation];
        }
        out.push(LiveSim {
            side,
            operations: ops,
            initial,
            simulation,
        });
    }
    out
}
