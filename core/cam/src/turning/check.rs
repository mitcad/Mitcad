// SPDX-License-Identifier: MIT
//! The checks of a turning program: against the lathe (the spindle's
//! speeds, the feeds, the travel, the zones and the chuck) and of the
//! written program itself, read back the way a Fanuc lathe control reads
//! it (diameter mode, feeds per revolution, the cycles' blocks).

use serde::{Deserialize, Serialize};

use super::{PXZ, TurnMove, TurnSpindle, TurningProgram, move_points};
use crate::CamError;
use crate::machine::{Finding, Machine};

/// Where the work zero is on the lathe: the machine Z of the work zero
/// (machine X is the spindle axis for both), and the work Z of the chuck
/// jaws' face, which no tool may pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnPlacement {
    pub work_z: f64,
    #[serde(default)]
    pub chuck_z: Option<f64>,
}

/// Checks a turning program against the lathe: one finding per operation
/// and kind, in words, with the place in `at` (`[x, 0, z]`, work
/// coordinates, x a radius).
///
/// The sub-spindle (mitcad#2375): the hand-over within its travel and its
/// speeds and the main spindle's; its operations (turning and live tools)
/// at its speeds, their tool within the turret's travel and clear of the
/// zones and of both chucks where the sub-spindle holds the part (pulled
/// back as the hand-over leaves it, [`super::HandOver::held_z`]), their
/// places in `at` where the view draws them (the main spindle's work
/// coordinates where the part was, [`super::HandOver::main_point`]).
///
/// The sub-spindle's body and the zones moving with it (mitcad#2392,
/// [`sub_spindle_hits`]): every operation's tool clear of them where the
/// sub-spindle is as it cuts. The findings come by operation, the
/// hand-over's before its operation's own.
pub fn check_lathe(
    machine: &Machine,
    program: &TurningProgram,
    placement: &TurnPlacement,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let Some(lathe) = &machine.lathe else {
        out.push(Finding {
            message: "The setup's machine is not a lathe.".to_owned(),
            operation: 0,
            at: None,
        });
        return out;
    };
    let main_speeds = [machine.spindle.min_rpm, machine.spindle.max_rpm];
    let sub_speeds = lathe.sub_spindle.speeds(main_speeds);
    // The hand-over: the sub-spindle's jaws' face where it grips and where
    // it pulls the part back to, in its travel; the synchronized speed
    // within both spindles' (mitcad#2375).
    if let (Some(h), Some(index)) = (&program.hand_over, program.hand_over_at()) {
        let name = &program.operations[index].name;
        if let Some([lo, hi]) = lathe.sub_spindle.travel {
            let beyond = [h.grip_z, h.grip_z + h.withdraw].into_iter().find(|z| {
                let m = z + placement.work_z;
                m < lo - 1e-9 || m > hi + 1e-9
            });
            if let Some(z) = beyond {
                out.push(Finding {
                    message: format!(
                        "The hand-over before {name}: the sub-spindle goes beyond its travel."
                    ),
                    operation: index,
                    at: Some([0.0, 0.0, z]),
                });
            }
        }
        let within = |s: [f64; 2]| h.rpm >= s[0] - 1e-9 && h.rpm <= s[1] + 1e-9;
        if !within(main_speeds) || !within(sub_speeds) {
            out.push(Finding {
                message: format!(
                    "The hand-over before {name}: the synchronized speed is outside what the spindles can do."
                ),
                operation: index,
                at: None,
            });
        }
    }
    for (i, op) in program.operations.iter().enumerate() {
        let name = &op.name;
        let mut say = |message: String, at: Option<[f64; 3]>| {
            out.push(Finding {
                message,
                operation: i,
                at,
            })
        };
        if op.tool.number > lathe.turret_stations {
            say(
                format!("{name}: the tool's station is not on the turret."),
                None,
            );
        }
        // The sub-spindle holds the part (mitcad#2319, mitcad#2375).
        let held = program.hand_over.as_ref().filter(|_| op.sub);
        if op.sub && !lathe.axes.sub_spindle {
            let message = if op.live.is_some() {
                format!("{name}: the lathe has no sub-spindle to hold the part's back.")
            } else {
                format!("{name}: the lathe has no sub-spindle to turn the part's back.")
            };
            say(message, None);
            continue;
        }
        // A live tool's operation (mitcad#2067).
        if let Some(live) = &op.live {
            for (message, at) in check_live(machine, lathe, op, live, placement, held) {
                say(message, at);
            }
            continue;
        }
        let speeds = if op.sub { sub_speeds } else { main_speeds };
        match op.spindle {
            TurnSpindle::Css { max_rpm, .. } if max_rpm > speeds[1] => {
                let message = if op.sub {
                    format!("{name}: the spindle limit is above what the sub-spindle can do.")
                } else {
                    format!("{name}: the spindle limit is above what the spindle can do.")
                };
                say(message, None);
            }
            TurnSpindle::Rpm { rpm } if rpm < speeds[0] || rpm > speeds[1] => {
                let message = if op.sub {
                    format!("{name}: the spindle speed is outside what the sub-spindle can do.")
                } else {
                    format!("{name}: the spindle speed is outside what the spindle can do.")
                };
                say(message, None);
            }
            _ => {}
        }
        // A point's main spindle work Z where the part is as it cuts, and
        // where the view draws it.
        let held_z = |z: f64| held.map_or(z, |h| h.held_z(z));
        let shown = |p: PXZ| held.map_or(p, |h| [p[0], h.main_z(p[1])]);
        let mut fast = None;
        let mut beyond = None;
        let mut chuck = None;
        let mut zones: Vec<Option<PXZ>> = vec![None; machine.zones.len()];
        let mut from: Option<PXZ> = None;
        for m in &op.path.moves {
            let pts = turn_move_points(from, m);
            if let (Some(f), Some(feed)) = (from, m.feed()) {
                let x = f[0].abs().min(m.to()[0].abs());
                if fast.is_none() && feed * op.spindle.rpm_at(x) > machine.max_feed + 1e-9 {
                    fast = Some(m.to());
                }
            }
            for p in pts {
                let z = held_z(p[1]);
                let machine_p = [p[0], 0.0, z + placement.work_z];
                let t = &machine.travel;
                let inside = machine_p[0] >= t.min[0] - 1e-9
                    && machine_p[0] <= t.max[0] + 1e-9
                    && machine_p[2] >= t.min[2] - 1e-9
                    && machine_p[2] <= t.max[2] + 1e-9;
                if !inside && beyond.is_none() {
                    beyond = Some(p);
                }
                if let Some(cz) = placement.chuck_z
                    && chuck.is_none()
                    && z < cz - 1e-9
                    && p[0] < lathe.chuck_diameter.max(0.0) / 2.0 + 1e-9
                {
                    chuck = Some(p);
                }
                for (k, zone) in machine.zones.iter().enumerate() {
                    let b = &zone.bounds;
                    if zones[k].is_none()
                        && machine_p[0] >= b.min[0] - 1e-9
                        && machine_p[0] <= b.max[0] + 1e-9
                        && machine_p[2] >= b.min[2] - 1e-9
                        && machine_p[2] <= b.max[2] + 1e-9
                    {
                        zones[k] = Some(p);
                    }
                }
            }
            from = Some(m.to());
        }
        let at3 = |p: PXZ| {
            let p = shown(p);
            Some([p[0], 0.0, p[1]])
        };
        if let Some(p) = fast {
            say(
                format!("{name}: a feed is faster than the machine can cut."),
                at3(p),
            );
        }
        if let Some(p) = beyond {
            say(
                format!("{name}: the tool goes beyond the machine's travel."),
                at3(p),
            );
        }
        if let Some(p) = chuck {
            say(format!("{name}: the tool runs into the chuck."), at3(p));
        }
        for (k, hit) in zones.into_iter().enumerate() {
            if let Some(p) = hit {
                say(
                    format!("{name}: the tool runs into {}.", machine.zones[k].name),
                    at3(p),
                );
            }
        }
    }
    // The sub-spindle's body and the zones moving with it, where it is as
    // each operation cuts (mitcad#2392); each operation's findings
    // together, the hand-over's first.
    out.extend(sub_spindle_hits(machine, program, Some(placement.work_z)));
    out.sort_by_key(|f| f.operation);
    out
}

/// Points along a turning move from `from` at most half a millimetre
/// apart (its end included, its start not); the move's end alone when
/// where it starts is not known.
fn turn_move_points(from: Option<PXZ>, m: &TurnMove) -> Vec<PXZ> {
    let Some(f) = from else {
        return vec![m.to()];
    };
    let mut pts = Vec::new();
    let mut prev = f;
    for q in move_points(f, m, 0.01) {
        let d = (q[0] - prev[0]).hypot(q[1] - prev[1]);
        let n = ((d / 0.5).ceil() as usize).max(1);
        for k in 1..=n {
            let t = k as f64 / n as f64;
            pts.push([
                prev[0] + (q[0] - prev[0]) * t,
                prev[1] + (q[1] - prev[1]) * t,
            ]);
        }
        prev = q;
    }
    pts
}

/// Points along a live operation's moves at most half a millimetre apart,
/// in its side's frame.
fn live_move_points(live: &super::LiveOp) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    let mut from: Option<[f64; 3]> = None;
    for (p, _) in live.points(0.01) {
        match from {
            Some(f) => {
                let d =
                    ((p[0] - f[0]).powi(2) + (p[1] - f[1]).powi(2) + (p[2] - f[2]).powi(2)).sqrt();
                let n = ((d / 0.5).ceil() as usize).max(1);
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    out.push(std::array::from_fn(|i| f[i] + (p[i] - f[i]) * t));
                }
            }
            None => out.push(p),
        }
        from = Some(p);
    }
    out
}

/// Where the sub-spindle's jaws' face is (the main spindle's work Z) while
/// operation `index` cuts (mitcad#2392): where the hand-over left it for
/// the sub-spindle's operations, where it grips for the parting off it
/// holds the part through, else where it waits (`work_z`: the machine Z
/// of the work zero, which that needs); none when not known.
pub fn sub_face_at(
    lathe: &super::Lathe,
    program: &TurningProgram,
    index: usize,
    work_z: Option<f64>,
) -> Option<f64> {
    if !lathe.axes.sub_spindle {
        return None;
    }
    let op = program.operations.get(index)?;
    if op.sub {
        return program.hand_over.as_ref().map(|h| h.grip_z + h.withdraw);
    }
    if program.hand_over_at() == Some(index) {
        return program.hand_over.as_ref().map(|h| h.grip_z);
    }
    Some(lathe.sub_spindle.waiting_z()? - work_z?)
}

/// Where the view draws the sub-spindle's jaws' face while operation
/// `index` cuts (mitcad#2405): the main spindle's work Z on the part where
/// the view draws it, which for the sub-spindle's operations is where the
/// part was before the hand-over pulled it back (at the grip), else where
/// [`sub_face_at`] has it; none when not known.
pub fn sub_face_shown(
    lathe: &super::Lathe,
    program: &TurningProgram,
    index: usize,
    work_z: Option<f64>,
) -> Option<f64> {
    let face = sub_face_at(lathe, program, index, work_z)?;
    match (&program.hand_over, program.operations.get(index)) {
        (Some(h), Some(op)) if op.sub => Some(face - h.withdraw),
        _ => Some(face),
    }
}

/// The first place each part of the sub-spindle's body and each zone
/// moving with it is hit, by the tool's controlled point or by the
/// turret's body (mitcad#2392, mitcad#2405).
struct SubHits {
    body: [Option<[f64; 3]>; 3],
    zones: Vec<Option<[f64; 3]>>,
}

impl SubHits {
    fn new(zones: usize) -> Self {
        SubHits {
            body: [None; 3],
            zones: vec![None; zones],
        }
    }

    fn hit(&mut self, part: super::SubBody, at: [f64; 3]) {
        let k = match part {
            super::SubBody::Chuck => 0,
            super::SubBody::Housing => 1,
            super::SubBody::Profile => 2,
        };
        self.body[k].get_or_insert(at);
    }
}

const SUB_PARTS: [super::SubBody; 3] = [
    super::SubBody::Chuck,
    super::SubBody::Housing,
    super::SubBody::Profile,
];

/// The moves that run into the sub-spindle's body (its chuck, its housing)
/// or into a zone moving with it (mitcad#2392), where the sub-spindle is as
/// each operation cuts ([`sub_face_at`]): one finding per operation and
/// part, in words, its place in `at` where the view draws the move (the
/// main spindle's work coordinates, on the part where it was). `work_z` is
/// the machine Z of the work zero; without it the main spindle's
/// operations, which the sub-spindle waits for, are not checked. The lathe
/// check gives these with its other findings; the simulation gives them as
/// its machine hits.
///
/// Its own profile (mitcad#2405) as a third part ("the sub-spindle's
/// body"), and the turret's body round the tool where the lathe has one
/// ([`super::TurretBody`]): at each of the tool's points the box against
/// the body and the zones, one finding per operation and part ("Drill 1:
/// the turret runs into the sub-spindle's housing."), after the tool's.
pub fn sub_spindle_hits(
    machine: &Machine,
    program: &TurningProgram,
    work_z: Option<f64>,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let Some(lathe) = &machine.lathe else {
        return out;
    };
    let sub = &lathe.sub_spindle;
    if !lathe.axes.sub_spindle || !sub.has_body() {
        return out;
    }
    for (i, op) in program.operations.iter().enumerate() {
        let Some(face) = sub_face_at(lathe, program, i, work_z) else {
            continue;
        };
        let held = program.hand_over.as_ref().filter(|_| op.sub);
        // The first place each part and each zone is hit by the tool and
        // by the turret's body: `[r, behind]` and the machine's X and Y of
        // a point give what it hits, `shown` where the view draws it.
        let mut tool = SubHits::new(sub.zones.len());
        let mut turret = SubHits::new(sub.zones.len());
        let within = |v: [f64; 2], b: &crate::machine::Bounds, axis: usize| {
            v[1] >= b.min[axis] - 1e-9 && v[0] <= b.max[axis] + 1e-9
        };
        let mut test = |r: f64, behind: f64, m: [f64; 2], shown: [f64; 3]| {
            if let Some(part) = sub.body_at(r, behind) {
                tool.hit(part, shown);
            }
            for (k, zone) in sub.zones.iter().enumerate() {
                if within([m[0]; 2], &zone.bounds, 0) && within([behind; 2], &zone.bounds, 2) {
                    tool.zones[k].get_or_insert(shown);
                }
            }
            // The turret's body round the tool, at the machine's X and Y
            // of its controlled point (mitcad#2405).
            if let Some(body) = &lathe.turret_body {
                let (x, along) = body.at(m[0], behind);
                let r_min = super::lathe::nearest_zero(x).hypot(m[1]);
                for part in sub.body_across(r_min, along) {
                    turret.hit(part, shown);
                }
                for (k, zone) in sub.zones.iter().enumerate() {
                    if within(x, &zone.bounds, 0) && within(along, &zone.bounds, 2) {
                        turret.zones[k].get_or_insert(shown);
                    }
                }
            }
        };
        match &op.live {
            Some(live) => {
                for q in live_move_points(live) {
                    let m = live.side.machine(q);
                    let w = live.side.to_work(q);
                    let z = held.map_or(w[2], |h| h.held_z(w[2]));
                    let shown = held.map_or(w, |h| h.main_point(w));
                    test(w[0].hypot(w[1]), z - face, [m[0], m[1]], shown);
                }
            }
            None => {
                let mut from: Option<PXZ> = None;
                for m in &op.path.moves {
                    for p in turn_move_points(from, m) {
                        let z = held.map_or(p[1], |h| h.held_z(p[1]));
                        let shown = held.map_or(p[1], |h| h.main_z(p[1]));
                        test(p[0].abs(), z - face, [p[0], 0.0], [p[0], 0.0, shown]);
                    }
                    from = Some(m.to());
                }
            }
        }
        let name = &op.name;
        let mut say = |message: String, at: [f64; 3]| {
            out.push(Finding {
                message,
                operation: i,
                at: Some(at),
            })
        };
        for (k, part) in SUB_PARTS.into_iter().enumerate() {
            if let Some(at) = tool.body[k] {
                say(part.finding(name), at);
            }
        }
        for (k, hit) in tool.zones.iter().enumerate() {
            if let Some(at) = *hit {
                say(
                    format!(
                        "{name}: the tool runs into {}, which moves with the sub-spindle.",
                        sub.zones[k].name
                    ),
                    at,
                );
            }
        }
        for (k, part) in SUB_PARTS.into_iter().enumerate() {
            if let Some(at) = turret.body[k] {
                say(part.turret_finding(name), at);
            }
        }
        for (k, hit) in turret.zones.iter().enumerate() {
            if let Some(at) = *hit {
                say(
                    format!(
                        "{name}: the turret runs into {}, which moves with the sub-spindle.",
                        sub.zones[k].name
                    ),
                    at,
                );
            }
        }
    }
    out
}

/// The lathe check of a live tool's operation (mitcad#2067): the C axis
/// and the live tools there, the live spindle's speed, the feeds (per
/// minute), and the travel, the zones and the chuck at the lathe's X (the
/// distance from the axis) and Z of each point along its moves. On the
/// sub-spindle (`held`, mitcad#2375) its points are where the sub-spindle
/// holds the part and its places in `at` where the view draws them; the
/// sub-spindle's body is [`sub_spindle_hits`]' (mitcad#2392).
fn check_live(
    machine: &Machine,
    lathe: &super::Lathe,
    op: &super::TurningProgramOp,
    live: &super::LiveOp,
    placement: &TurnPlacement,
    held: Option<&super::HandOver>,
) -> Vec<(String, Option<[f64; 3]>)> {
    let name = &op.name;
    let mut out = Vec::new();
    if !lathe.mills() {
        out.push((
            format!(
                "{name}: the lathe has no C axis with live tools to cut {}.",
                live.side.words()
            ),
            None,
        ));
        return out;
    }
    // A plane needs the Y axis, a tilted one or a tool in the head the B
    // head (mitcad#2319).
    let kind = live.side.plane_kind();
    if kind.is_some() && !lathe.has_y() {
        out.push((
            format!(
                "{name}: the lathe has no Y axis to cut {}.",
                live.side.words()
            ),
            None,
        ));
        return out;
    }
    if (kind == Some(super::live::PlaneKind::Tilted) || live.head) && !lathe.has_b() {
        out.push((
            format!("{name}: the lathe has no B head to tilt the tool."),
            None,
        ));
        return out;
    }
    let codes = &lathe.mill_turn;
    let rpm = op.spindle.rpm_at(0.0);
    if rpm < codes.live_min_rpm || rpm > codes.live_max_rpm {
        out.push((
            format!("{name}: the live tool's speed is outside what its spindle can do."),
            None,
        ));
    }
    // A work point where the view draws it, the machine's Y and the main
    // spindle's work Z of a point where the part is as it cuts (turned
    // round about X on the sub-spindle). The sub-spindle's body is
    // [`sub_spindle_hits`]'.
    let shown = |w: [f64; 3]| held.map_or(w, |h| h.main_point(w));
    let machine_y = |y: f64| if held.is_some() { -y } else { y };
    let held_z = |z: f64| held.map_or(z, |h| h.held_z(z));
    let mut fast = None;
    let mut beyond = None;
    let mut chuck = None;
    let mut zones: Vec<Option<[f64; 3]>> = vec![None; machine.zones.len()];
    // Points at most half a millimetre apart along the moves.
    let mut from: Option<[f64; 3]> = None;
    for (p, feed) in live.points(0.01) {
        let pts: Vec<[f64; 3]> = match from {
            Some(f) => {
                let d =
                    ((p[0] - f[0]).powi(2) + (p[1] - f[1]).powi(2) + (p[2] - f[2]).powi(2)).sqrt();
                let n = ((d / 0.5).ceil() as usize).max(1);
                (1..=n)
                    .map(|k| {
                        let t = k as f64 / n as f64;
                        std::array::from_fn(|i| f[i] + (p[i] - f[i]) * t)
                    })
                    .collect()
            }
            None => vec![p],
        };
        if fast.is_none() && feed.is_some_and(|f| f > machine.max_feed + 1e-9) {
            fast = Some(shown(live.side.to_work(p)));
        }
        // The machine's X, Y (a Y axis's, mitcad#2319) and Z of the tool's
        // tip; Y only on a lathe with a Y axis.
        let y_axis = lathe.has_y();
        for q in pts {
            let m = live.side.machine(q);
            let w = live.side.to_work(q);
            let z = held_z(w[2]);
            let r = w[0].hypot(w[1]);
            let machine_p = [m[0], machine_y(m[1]), held_z(m[2]) + placement.work_z];
            let t = &machine.travel;
            let within = |p: [f64; 3], lo: [f64; 3], hi: [f64; 3]| {
                (0..3)
                    .all(|k| (k == 1 && !y_axis) || (p[k] >= lo[k] - 1e-9 && p[k] <= hi[k] + 1e-9))
            };
            if !within(machine_p, t.min, t.max) && beyond.is_none() {
                beyond = Some(shown(w));
            }
            if let Some(cz) = placement.chuck_z
                && chuck.is_none()
                && z < cz - 1e-9
                && r < lathe.chuck_diameter.max(0.0) / 2.0 + 1e-9
            {
                chuck = Some(shown(w));
            }
            for (k, zone) in machine.zones.iter().enumerate() {
                let b = &zone.bounds;
                if zones[k].is_none() && within(machine_p, b.min, b.max) {
                    zones[k] = Some(shown(w));
                }
            }
        }
        from = Some(p);
    }
    if let Some(p) = fast {
        out.push((
            format!("{name}: a feed is faster than the machine can cut."),
            Some(p),
        ));
    }
    if let Some(p) = beyond {
        out.push((
            format!("{name}: the tool goes beyond the machine's travel."),
            Some(p),
        ));
    }
    if let Some(p) = chuck {
        out.push((format!("{name}: the tool runs into the chuck."), Some(p)));
    }
    for (k, hit) in zones.into_iter().enumerate() {
        if let Some(p) = hit {
            out.push((
                format!("{name}: the tool runs into {}.", machine.zones[k].name),
                Some(p),
            ));
        }
    }
    out
}

/// A move read back from a program: `x` a radius, in millimetres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReadMove {
    pub line: usize,
    #[serde(rename = "move")]
    pub motion: TurnMove,
}

/// What reading a program back found: its moves (the cycles' blocks as
/// they are written; the cycles themselves are listed by kind and line),
/// and the cycles.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProgramCheck {
    pub moves: Vec<ReadMove>,
    /// `G71`, `G70`, `G76` with their lines (1-based).
    pub cycles: Vec<(String, usize)>,
    /// Whether X was read as a diameter throughout (no radius mode).
    pub diameter_mode: bool,
    /// Lines that set a constant surface speed, each after a limit.
    pub css_lines: Vec<usize>,
    /// The live tools' moves (mitcad#2067): those in polar or cylindrical
    /// interpolation and those that index the C axis; the drilling cycles
    /// (`G83`, `G87`) are listed in `cycles`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub live_moves: Vec<LiveRead>,
}

/// How a live tool's block was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveMode {
    /// Polar coordinate interpolation (G12.1): `x` and `c` the place on
    /// the face (mm).
    Polar,
    /// Cylindrical interpolation (G7.1): `c` in degrees, `x` the distance
    /// from the axis.
    Cylindrical,
    /// The C axis positioned (degrees), `x` the distance from the axis.
    Indexed,
    /// On a plane with the Y axis, the C axis clamped (mitcad#2319): `x`
    /// the machine's X (written as a diameter), `y` and `z`.
    Plane,
    /// In the tilted working plane (G68.2, mitcad#2319): `x`, `y` and `z`
    /// in its frame.
    Tilted,
}

/// A live tool's move read back (millimetres, degrees), where it ends.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveRead {
    pub line: usize,
    pub mode: LiveMode,
    /// 0 to 3 for G0 to G3.
    pub motion: u32,
    pub x: f64,
    pub c: f64,
    pub z: f64,
    /// The Y axis's place on a plane (mitcad#2319).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub y: f64,
    /// The feed (mm/min) of a cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<f64>,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn word_value(text: &str) -> Option<f64> {
    text.parse::<f64>().ok()
}

/// One block's words: letter and value text.
fn words(line: &str) -> Result<Vec<(char, String)>, String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    let mut in_comment = false;
    while let Some(c) = chars.next() {
        if in_comment {
            if c == ')' {
                in_comment = false;
            }
            continue;
        }
        match c {
            '(' => in_comment = true,
            ' ' | '\t' => {}
            c if c.is_ascii_alphabetic() => {
                let mut v = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() || d == '.' || d == '-' || d == '+' {
                        v.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if v.is_empty() {
                    return Err(format!("a word {c} without a value"));
                }
                out.push((c.to_ascii_uppercase(), v));
            }
            other => return Err(format!("a character {other} it does not read")),
        }
    }
    if in_comment {
        return Err("a comment that is not closed".to_owned());
    }
    Ok(out)
}

/// Reads a turning program back the way a Fanuc lathe control in G-code
/// system A does, refusing in words (with the line) what would make it
/// cut other than meant: a number without a decimal point where lengths
/// are meant, a feed before the feed mode is per revolution, a constant
/// surface speed before its limit, threading at a constant surface speed,
/// a cycle whose blocks are missing or do not run one way, words it does
/// not know. For live tools (mitcad#2067) it reads the C axis (an angle,
/// or in polar interpolation a length), polar (G12.1, G13.1) and
/// cylindrical (G7.1) interpolation and the drilling cycles (G83, G87 to
/// G80), and refuses a rapid (G0) or a tool change in interpolation, a cut
/// while the main spindle stands at feeds per revolution or while it turns
/// at feeds per minute, and a program that ends in interpolation.
///
/// The lathe's codes are the default set ([`check_program_with`] reads a
/// program written with others).
pub fn check_program(text: &str) -> Result<ProgramCheck, CamError> {
    check_program_with(text, &super::MillTurn::default())
}

/// [`check_program`] for a lathe with its own codes (mitcad#2375): the
/// sub-spindle's stop block stops the spindle that holds the part, as M5
/// does, so that its live tools cut with it standing.
pub fn check_program_with(text: &str, codes: &super::MillTurn) -> Result<ProgramCheck, CamError> {
    let lines: Vec<&str> = text.lines().collect();
    // The block that stops the sub-spindle, besides M5.
    let sub_stop: Option<Vec<(char, String)>> =
        words(codes.sub_stop.trim()).ok().filter(|w| !w.is_empty());
    let fail = |n: usize, what: String| CamError::new(format!("Line {n}: {what}."));
    // Sequence numbers and their lines.
    let mut labels: Vec<(u32, usize)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if let Ok(ws) = words(l)
            && let Some((_, v)) = ws.iter().find(|(c, _)| *c == 'N')
            && let Ok(n) = v.parse::<u32>()
        {
            labels.push((n, i));
        }
    }
    let mut check = ProgramCheck {
        diameter_mode: true,
        ..ProgramCheck::default()
    };
    let mut motion: Option<u32> = None;
    let mut per_rev = false;
    let mut css = false;
    let mut limit = false;
    let mut feed: Option<f64> = None;
    let mut at: [Option<f64>; 2] = [None, None];
    let mut scale = 1.0;
    let mut units = false;
    // A G71 or G76 waiting for its second block.
    let mut pending: Option<u32> = None;
    // The last line of a G71's contour blocks, which the control reads for
    // the cycle and then passes by.
    let mut skip_to: Option<usize> = None;
    let mut tape = 0;
    // Live tools (mitcad#2067): the feeds per minute, the main spindle
    // turning (M3, M4) or not (M5), the plane, polar or cylindrical
    // interpolation, a drilling cycle until G80, the C axis's place.
    let mut per_min = false;
    let mut main_running = false;
    let mut plane = 18;
    let mut polar = false;
    let mut cylindrical = false;
    let mut drilling: Option<u32> = None;
    let mut at_c: Option<f64> = None;
    // Planes with the Y axis and the tilted working plane (mitcad#2319):
    // a plane chosen while the main spindle stands is milled on, the Y
    // axis's place.
    let mut on_plane = false;
    let mut tilted = false;
    let mut at_y: Option<f64> = None;
    for (i, raw) in lines.iter().enumerate() {
        let n = i + 1;
        let l = raw.trim();
        if l.is_empty() {
            continue;
        }
        if l == "%" {
            tape += 1;
            continue;
        }
        let ws = words(l).map_err(|e| fail(n, e))?;
        if ws.is_empty() {
            continue;
        }
        let get = |c: char| ws.iter().find(|(w, _)| *w == c).map(|(_, v)| v.as_str());
        let num = |c: char| -> Result<Option<f64>, CamError> {
            match get(c) {
                None => Ok(None),
                Some(v) => word_value(v)
                    .map(Some)
                    .ok_or_else(|| fail(n, format!("the {c} word is not a number"))),
            }
        };
        // Whole G codes, and those with a decimal (G7.1, G12.1, G13.1) as
        // ten times their number.
        let mut gs: Vec<u32> = Vec::new();
        let mut gd: Vec<u32> = Vec::new();
        for (_, v) in ws.iter().filter(|(c, _)| *c == 'G') {
            if let Ok(g) = v.parse::<u32>() {
                gs.push(g);
                continue;
            }
            match v.parse::<f64>() {
                Ok(g) if g >= 0.0 && ((g * 10.0).round() - g * 10.0).abs() < 1e-9 => {
                    gd.push((g * 10.0).round() as u32);
                }
                _ => return Err(fail(n, format!("G{v} is not a command it knows"))),
            }
        }
        for (c, v) in &ws {
            // Y, the B head and the sub-spindle's axis (A, B or E) too
            // (mitcad#2319).
            if matches!(
                c,
                'X' | 'Z' | 'I' | 'K' | 'U' | 'W' | 'R' | 'C' | 'J' | 'H' | 'Y' | 'A' | 'B' | 'E'
            ) && !v.contains('.')
            {
                return Err(fail(
                    n,
                    format!(
                        "{c} needs a decimal point, or the control reads it in its smallest steps"
                    ),
                ));
            }
            if !matches!(
                c,
                'N' | 'O'
                    | 'G'
                    | 'M'
                    | 'T'
                    | 'S'
                    | 'F'
                    | 'X'
                    | 'Z'
                    | 'I'
                    | 'K'
                    | 'U'
                    | 'W'
                    | 'R'
                    | 'P'
                    | 'Q'
                    | 'C'
                    | 'J'
                    | 'H'
                    | 'Y'
                    | 'A'
                    | 'B'
                    | 'E'
            ) {
                return Err(fail(n, format!("the word {c} is not one it reads")));
            }
        }
        if ws[0].0 == 'O' {
            continue;
        }
        // The main spindle, and a tool change or the end in interpolation
        // (mitcad#2067).
        let interpolation = if polar {
            Some("polar coordinate interpolation")
        } else if cylindrical {
            Some("cylindrical interpolation")
        } else {
            None
        };
        // Neither a tool change nor the end in the tilted working plane
        // (mitcad#2319).
        let held = interpolation.or(tilted.then_some("the tilted working plane"));
        // The sub-spindle stopped: it holds the part after the hand-over
        // (mitcad#2375).
        let block: Vec<(char, String)> = ws.iter().filter(|(c, _)| *c != 'N').cloned().collect();
        if sub_stop.as_ref() == Some(&block) {
            main_running = false;
        }
        for (_, v) in ws.iter().filter(|(c, _)| *c == 'M') {
            match v.parse::<u32>() {
                Ok(3 | 4) => main_running = true,
                Ok(5) => main_running = false,
                Ok(2 | 30) => {
                    if let Some(mode) = held {
                        return Err(fail(n, format!("the program ends in {mode}")));
                    }
                }
                _ => {}
            }
        }
        if let Some(mode) = held
            && get('T').is_some()
        {
            return Err(fail(n, format!("a tool change in {mode}")));
        }
        if let Some(end) = skip_to {
            if i >= end {
                skip_to = None;
            }
            continue;
        }
        if let Some(kind) = pending.take() {
            if !gs.contains(&kind) {
                return Err(fail(n, format!("G{kind} needs its second block here")));
            }
            match kind {
                71 => {
                    let (Some(p), Some(q)) = (num('P')?, num('Q')?) else {
                        return Err(fail(
                            n,
                            "G71 needs the blocks of its contour (P and Q)".to_owned(),
                        ));
                    };
                    if num('F')?.is_none() {
                        return Err(fail(n, "G71 needs a feed".to_owned()));
                    }
                    let u = num('U')?.unwrap_or(0.0);
                    let end = check_contour(&lines, &labels, p as u32, q as u32, Some(u < 0.0), n)?;
                    if end.0 != i + 1 {
                        return Err(fail(n, "the contour's blocks must follow G71".to_owned()));
                    }
                    skip_to = Some(end.1);
                    check.cycles.push(("G71".to_owned(), n));
                }
                76 => {
                    for w in ['X', 'Z', 'P', 'Q', 'F'] {
                        if get(w).is_none() {
                            return Err(fail(n, format!("G76 needs its {w} word")));
                        }
                    }
                    if css {
                        return Err(fail(n, "threading at a constant surface speed".to_owned()));
                    }
                    check.cycles.push(("G76".to_owned(), n));
                }
                _ => {}
            }
            // The tool is back where the cycle started; the motion and the
            // feed are written again.
            motion = None;
            feed = None;
            continue;
        }
        // A block that moves no axis though it has their words (G28 H,
        // G7.1 C).
        let mut no_move = false;
        for &g in &gs {
            match g {
                0..=3 | 32 => {
                    motion = Some(g);
                    // A motion ends a drilling cycle.
                    drilling = None;
                }
                20 => {
                    scale = 25.4;
                    units = true;
                }
                21 => {
                    scale = 1.0;
                    units = true;
                }
                17..=19 => {
                    plane = g;
                    // A plane chosen with the main spindle standing is
                    // milled on (mitcad#2319); G18 is the lathe's own.
                    on_plane = g != 18 && !main_running;
                }
                40 | 54..=59 => {}
                // The tilted working plane's cancel (mitcad#2319).
                69 => {
                    if tilted {
                        at = [None, None];
                        at_y = None;
                    }
                    tilted = false;
                }
                98 => {
                    per_rev = false;
                    per_min = true;
                }
                99 => {
                    per_rev = true;
                    per_min = false;
                }
                // Live tools (mitcad#2067): the C axis to its reference,
                // the drilling cycles and their cancel.
                28 => {
                    if get('X').is_some() || get('Z').is_some() || get('U').is_some() {
                        return Err(fail(
                            n,
                            "G28 sends X or Z to the reference here, which the program must not"
                                .to_owned(),
                        ));
                    }
                    if get('H').is_none() {
                        return Err(fail(
                            n,
                            "G28 without the C axis (H) it references".to_owned(),
                        ));
                    }
                    no_move = true;
                }
                80 => drilling = None,
                83 | 87 => {
                    if let Some(mode) = interpolation {
                        return Err(fail(n, format!("a drilling cycle in {mode}")));
                    }
                    let depth = if g == 83 { 'Z' } else { 'X' };
                    for w in [depth, 'R', 'F'] {
                        if get(w).is_none() {
                            return Err(fail(n, format!("G{g} needs its {w} word")));
                        }
                    }
                    if main_running && !per_rev {
                        return Err(fail(
                            n,
                            "a feed before the feeds are per revolution (G99)".to_owned(),
                        ));
                    }
                    if !main_running && !per_min {
                        return Err(fail(
                            n,
                            "a cut with the main spindle stopped before the feeds are per minute (G98)"
                                .to_owned(),
                        ));
                    }
                    drilling = Some(g);
                    check.cycles.push((format!("G{g}"), n));
                }
                96 | 97 if get('S').is_some() && get('M').is_some() => {
                    // A spindle started with its speed: the main one, or
                    // the sub-spindle with its own code (mitcad#2319).
                    if g == 96 {
                        if !limit {
                            return Err(fail(
                                n,
                                "a constant surface speed without a spindle limit (G50 S) before it"
                                    .to_owned(),
                            ));
                        }
                        css = true;
                        check.css_lines.push(n);
                    } else {
                        css = false;
                    }
                    main_running = true;
                }
                96 => {
                    if !limit {
                        return Err(fail(
                            n,
                            "a constant surface speed without a spindle limit (G50 S) before it"
                                .to_owned(),
                        ));
                    }
                    css = true;
                    check.css_lines.push(n);
                }
                97 => css = false,
                50 => {
                    if get('X').is_some() || get('Z').is_some() {
                        return Err(fail(
                            n,
                            "G50 sets the coordinates here, which the program must not".to_owned(),
                        ));
                    }
                    if get('S').is_none() {
                        return Err(fail(n, "G50 without a spindle limit".to_owned()));
                    }
                    limit = true;
                }
                71 | 76 => {
                    if g == 71 && (get('U').is_none() || get('R').is_none()) {
                        return Err(fail(n, "G71 needs its depth of cut and retract".to_owned()));
                    }
                    if g == 76 && (get('P').is_none() || get('Q').is_none() || get('R').is_none()) {
                        return Err(fail(
                            n,
                            "G76 needs its passes, smallest depth and allowance".to_owned(),
                        ));
                    }
                    pending = Some(g);
                }
                70 => {
                    let (Some(p), Some(q)) = (num('P')?, num('Q')?) else {
                        return Err(fail(
                            n,
                            "G70 needs the blocks of its contour (P and Q)".to_owned(),
                        ));
                    };
                    check_contour(&lines, &labels, p as u32, q as u32, None, n)?;
                    check.cycles.push(("G70".to_owned(), n));
                    motion = None;
                    feed = None;
                }
                other => return Err(fail(n, format!("G{other} is not a command it reads"))),
            }
        }
        // Polar and cylindrical interpolation (mitcad#2067), each in a
        // block of its own.
        for &g in &gd {
            match g {
                121 => {
                    if plane != 17 {
                        return Err(fail(
                            n,
                            "polar coordinate interpolation outside the XY plane (G17)".to_owned(),
                        ));
                    }
                    if cylindrical {
                        return Err(fail(
                            n,
                            "polar coordinate interpolation in cylindrical interpolation"
                                .to_owned(),
                        ));
                    }
                    polar = true;
                    // The place in the face's plane: X across, C the
                    // length along it.
                    match (at[0], at_c) {
                        (Some(r), Some(angle)) => {
                            let a = angle.to_radians();
                            at[0] = Some(r * a.cos());
                            at_c = Some(r * a.sin());
                        }
                        _ => {
                            at[0] = None;
                            at_c = None;
                        }
                    }
                }
                131 => {
                    if polar {
                        match (at[0], at_c) {
                            (Some(x), Some(y)) => {
                                at[0] = Some(x.hypot(y));
                                at_c = Some(y.atan2(x).to_degrees());
                            }
                            _ => {
                                at[0] = None;
                                at_c = None;
                            }
                        }
                    }
                    polar = false;
                }
                71 => {
                    let Some(r) = num('C')? else {
                        return Err(fail(n, "G7.1 without the cylinder's radius (C)".to_owned()));
                    };
                    if r > 0.0 {
                        if plane != 19 {
                            return Err(fail(
                                n,
                                "cylindrical interpolation outside the YZ plane (G19)".to_owned(),
                            ));
                        }
                        if polar {
                            return Err(fail(
                                n,
                                "cylindrical interpolation in polar coordinate interpolation"
                                    .to_owned(),
                            ));
                        }
                        cylindrical = true;
                    } else {
                        cylindrical = false;
                    }
                }
                // The tilted working plane (G68.2) and the tool turned to
                // it (G53.1), mitcad#2319.
                682 => {
                    if polar || cylindrical {
                        return Err(fail(
                            n,
                            "a tilted working plane in polar or cylindrical interpolation"
                                .to_owned(),
                        ));
                    }
                    if main_running {
                        return Err(fail(
                            n,
                            "a tilted working plane while the main spindle turns: stop it (M5) first"
                                .to_owned(),
                        ));
                    }
                    for w in ['X', 'Y', 'Z', 'I', 'J', 'K'] {
                        if get(w).is_none() {
                            return Err(fail(n, format!("G68.2 needs its {w} word")));
                        }
                    }
                    tilted = true;
                    at = [None, None];
                    at_y = None;
                }
                531 => {
                    if !tilted {
                        return Err(fail(
                            n,
                            "G53.1 turns the tool to a tilted working plane, and there is none"
                                .to_owned(),
                        ));
                    }
                }
                other => {
                    return Err(fail(
                        n,
                        format!("G{} is not a command it reads", f64::from(other) / 10.0),
                    ));
                }
            }
            no_move = true;
        }
        if pending.is_some() || gs.contains(&70) || no_move {
            continue;
        }
        if let Some(f) = num('F')? {
            feed = Some(f * scale);
        }
        let x = num('X')?;
        let z = num('Z')?;
        let c = num('C')?;
        let y = num('Y')?;
        if get('U').is_some() || get('W').is_some() || get('H').is_some() {
            return Err(fail(n, "incremental moves are not read".to_owned()));
        }
        // The B head's tilt and the sub-spindle's axis move on their own
        // (mitcad#2319).
        let other = ['A', 'B', 'E'].into_iter().find(|&w| get(w).is_some());
        if let Some(w) = other {
            if x.is_some() || z.is_some() || c.is_some() || y.is_some() {
                return Err(fail(
                    n,
                    format!("{w} moves with the tool's axes here, which the program must not"),
                ));
            }
            if motion.is_none() {
                return Err(fail(n, "a move without a motion (G0 to G3)".to_owned()));
            }
            continue;
        }
        // A drilling cycle's holes are its blocks' places.
        if drilling.is_some() {
            continue;
        }
        if x.is_none() && z.is_none() && c.is_none() && y.is_none() {
            continue;
        }
        if !units {
            return Err(fail(
                n,
                "a move before the program says its units".to_owned(),
            ));
        }
        let Some(mo) = motion else {
            return Err(fail(n, "a move without a motion (G0 to G3)".to_owned()));
        };
        // Feeds per revolution while the main spindle turns, per minute
        // while it stands (a live tool's, mitcad#2067).
        let need_feed = || -> Result<f64, CamError> {
            if main_running && !per_rev {
                return Err(fail(
                    n,
                    "a feed before the feeds are per revolution (G99)".to_owned(),
                ));
            }
            if !main_running && !per_min {
                return Err(fail(
                    n,
                    "a cut with the main spindle stopped before the feeds are per minute (G98)"
                        .to_owned(),
                ));
            }
            feed.ok_or_else(|| fail(n, "a cut without a feed".to_owned()))
        };
        // On a plane with the Y axis (X a diameter) or in the tilted
        // working plane (X a length), the C axis clamped (mitcad#2319). A
        // rapid may come from where the tool is not known (the tilted
        // plane's first); a cut may not.
        if (on_plane || tilted || y.is_some()) && interpolation.is_none() {
            if main_running {
                return Err(fail(
                    n,
                    "a move with the Y axis while the main spindle turns: stop it (M5) first"
                        .to_owned(),
                ));
            }
            let half = if tilted { 1.0 } else { 0.5 };
            let lx = x.map(|v| v * scale * half).or(at[0]);
            let lz = z.map(|v| v * scale).or(at[1]);
            let ly = y.map(|v| v * scale).or(at_y);
            at = [lx, lz];
            at_y = ly;
            let (Some(lx), Some(ly), Some(lz)) = (lx, ly, lz) else {
                if mo == 0 {
                    continue;
                }
                return Err(fail(n, "a cut from where the tool is not known".to_owned()));
            };
            let arc_words = match (tilted, plane) {
                (true, _) | (false, 17) => ['I', 'J'],
                (false, 19) => ['J', 'K'],
                _ => ['I', 'K'],
            };
            if matches!(mo, 2 | 3) {
                for w in arc_words {
                    if num(w)?.is_none() {
                        return Err(fail(
                            n,
                            format!(
                                "an arc without its center ({} and {})",
                                arc_words[0], arc_words[1]
                            ),
                        ));
                    }
                }
            }
            if mo == 32 {
                return Err(fail(n, "threading with the Y axis".to_owned()));
            }
            let cut = if mo == 0 { None } else { Some(need_feed()?) };
            check.live_moves.push(LiveRead {
                line: n,
                mode: if tilted {
                    LiveMode::Tilted
                } else {
                    LiveMode::Plane
                },
                motion: mo,
                x: lx,
                c: at_c.unwrap_or(0.0),
                z: lz,
                y: ly,
                feed: cut,
            });
            continue;
        }
        // A live tool's move: in interpolation, or with the C axis.
        if let Some(mode) = interpolation.or(c.map(|_| "")) {
            if mo == 0 && !mode.is_empty() {
                return Err(fail(
                    n,
                    format!("a rapid (G0) in {mode}, which the control does not take"),
                ));
            }
            if main_running && !mode.is_empty() {
                return Err(fail(
                    n,
                    format!("{mode} while the main spindle turns: stop it (M5) first"),
                ));
            }
            let kind = if polar {
                LiveMode::Polar
            } else if cylindrical {
                LiveMode::Cylindrical
            } else {
                LiveMode::Indexed
            };
            let lx = x.map(|v| v * scale / 2.0).or(at[0]);
            let lz = z.map(|v| v * scale).or(at[1]);
            let lc = c.map(|v| if polar { v * scale } else { v }).or(at_c);
            let (Some(lx), Some(lz), Some(lc)) = (lx, lz, lc) else {
                return Err(fail(
                    n,
                    "a move from where the tool is not known".to_owned(),
                ));
            };
            match (mo, kind) {
                (2 | 3, LiveMode::Polar) if num('I')?.is_none() || num('J')?.is_none() => {
                    return Err(fail(n, "an arc without its centre (I and J)".to_owned()));
                }
                (2 | 3, LiveMode::Cylindrical) if num('R')?.is_none() => {
                    return Err(fail(n, "an arc without its radius (R)".to_owned()));
                }
                (2 | 3, LiveMode::Indexed) => {
                    return Err(fail(
                        n,
                        "an arc with the C axis indexed: arcs need interpolation".to_owned(),
                    ));
                }
                (32, _) => {
                    return Err(fail(n, "threading with the C axis".to_owned()));
                }
                _ => {}
            }
            let cut = if mo == 0 { None } else { Some(need_feed()?) };
            check.live_moves.push(LiveRead {
                line: n,
                mode: kind,
                motion: mo,
                x: lx,
                c: lc,
                z: lz,
                y: 0.0,
                feed: cut,
            });
            at = [Some(lx), Some(lz)];
            at_c = Some(lc);
            continue;
        }
        let to = [
            x.map(|v| v * scale / 2.0).or(at[0]),
            z.map(|v| v * scale).or(at[1]),
        ];
        let (Some(tx), Some(tz)) = (to[0], to[1]) else {
            return Err(fail(
                n,
                "a move from where the tool is not known".to_owned(),
            ));
        };
        let to = [tx, tz];
        let m = match mo {
            0 => TurnMove::Rapid { to },
            1 => TurnMove::Line {
                to,
                feed: need_feed()?,
            },
            2 | 3 => {
                let (Some(i_), Some(k_)) = (num('I')?, num('K')?) else {
                    return Err(fail(n, "an arc without its centre (I and K)".to_owned()));
                };
                let (Some(fx), Some(fz)) = (at[0], at[1]) else {
                    return Err(fail(
                        n,
                        "an arc from where the tool is not known".to_owned(),
                    ));
                };
                TurnMove::Arc {
                    to,
                    center: [fx + i_ * scale, fz + k_ * scale],
                    cw: mo == 2,
                    feed: need_feed()?,
                }
            }
            32 => {
                if css {
                    return Err(fail(n, "threading at a constant surface speed".to_owned()));
                }
                TurnMove::Thread {
                    to,
                    lead: feed.ok_or_else(|| fail(n, "a thread without its lead".to_owned()))?,
                }
            }
            _ => unreachable!("motions are 0 to 3 and 32"),
        };
        at = [Some(to[0]), Some(to[1])];
        check.moves.push(ReadMove { line: n, motion: m });
    }
    if pending.is_some() {
        return Err(CamError::new("The program ends in the middle of a cycle."));
    }
    if polar || cylindrical {
        return Err(CamError::new(
            "The program ends in polar or cylindrical interpolation.",
        ));
    }
    if tilted {
        return Err(CamError::new(
            "The program ends in the tilted working plane.",
        ));
    }
    if tape == 1 {
        return Err(CamError::new(
            "The program has a tape mark at its start but not at its end.",
        ));
    }
    Ok(check)
}

/// Checks the blocks `p` to `q` a cycle runs and gives their lines
/// (0-based): for G71 (`rough` with the side, `Some(inside)`) the first
/// moves in X alone and X runs one way, out from the axis or towards it
/// inside.
fn check_contour(
    lines: &[&str],
    labels: &[(u32, usize)],
    p: u32,
    q: u32,
    rough: Option<bool>,
    n: usize,
) -> Result<(usize, usize), CamError> {
    let fail = |what: &str| CamError::new(format!("Line {n}: {what}."));
    let find = |label: u32| labels.iter().find(|(l, _)| *l == label).map(|(_, i)| *i);
    let (Some(a), Some(b)) = (find(p), find(q)) else {
        return Err(fail("the cycle's blocks are not in the program"));
    };
    if b < a {
        return Err(fail("the cycle's blocks are not where it runs them"));
    }
    let Some(inside) = rough else {
        return Ok((a, b));
    };
    let mut last_x: Option<f64> = None;
    for (k, l) in lines[a..=b].iter().enumerate() {
        let ws = words(l).map_err(|e| CamError::new(format!("Line {}: {e}.", a + k + 1)))?;
        let x = ws
            .iter()
            .find(|(c, _)| *c == 'X')
            .and_then(|(_, v)| word_value(v));
        if k == 0 && ws.iter().any(|(c, _)| *c == 'Z') {
            return Err(fail("the first block of the contour must move in X alone"));
        }
        if let Some(x) = x {
            if k > 0
                && let Some(prev) = last_x
                && (if inside {
                    x > prev + 1e-9
                } else {
                    x < prev - 1e-9
                })
            {
                return Err(fail("the contour does not run one way in X"));
            }
            last_x = Some(x);
        }
    }
    Ok((a, b))
}
