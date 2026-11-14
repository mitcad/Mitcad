// SPDX-License-Identifier: MIT
//! The tool changes of a program (docs/cam.md, "Simulation and checks"):
//! a macro changer's own moves, read back from the posted program
//! ([`crate::gcode`]) with the spindle assembly swept along them, and the
//! rapid travel from where a change leaves the spindle to the next
//! operation's first point at the safe height.
//!
//! - The posted program is read line by line. On the changer's `unload`
//!   lines the spindle carries the tool of the operation before; on its
//!   `load` lines and after them the operation's tool (on the trip to the
//!   rack the spindle may in fact be empty: taking the tool along is the
//!   safe side).
//! - The keep-out zones a rack slot is in (the slot's X and Y inside the
//!   zone's box) are the rack: the changer's own moves go into them and
//!   are not checked against them. Every other zone, the fixtures and the
//!   stock are, every part of the assembly, the cutter too (a changer does
//!   not cut). The operations' moves and the travel after a change are
//!   checked against the rack as against any zone.
//! - After a change the program drives (a macro changer's `load` lines),
//!   the travel starts where the lines leave the spindle. After one it
//!   does not (the operator's, or the controller's own changer), it starts
//!   at the machine's park position, or without one at the top of the Z
//!   travel above where the spindle was. The first operation of a program
//!   whose first tool is already in the spindle (decision 9) has no
//!   travel. The travel goes as the post writes it: to the safe height
//!   first, then across above the first point (up to it first when it is
//!   higher), where the operation's own check starts.
//! - A move from where the reader does not know the position (a program's
//!   first change, before its first X and Y) is not checked.
//! - One contact per part kind and obstacle in a change, the first; its
//!   `line` is the line of the posted program, its `change` says which
//!   step and which of the changer's lines.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    Best, CollisionOptions, CollisionReport, Leg, LegAt, Obstacle, ObstacleKind, ObstacleShape,
    PreparedObstacle, PreparedPart, check_collisions, check_leg, default_arc_tolerance, legs,
    prepare_obstacle, prepare_part,
};
use crate::gcode::{GMove, GcodeError, GcodeReader, Known, Motion, Plane, known};
use crate::machine::{Machine, Placement, ToolChanger};
use crate::post::Posted;
use crate::program::{Program, ProgramOp};
use crate::spindle::{PartKind, SpindleAssembly, SpindleModel};
use crate::tool::Tool;
use crate::toolpath::{Move, P3};

/// Which moves of a tool change a contact is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum ChangeMove {
    /// A line of the changer's `unload` lines (1-based in that list): the
    /// tool in the spindle on its way into its slot.
    Unload { line: usize },
    /// A line of the changer's `load` lines (1-based): the operation's
    /// tool taken from its slot.
    Load { line: usize },
    /// The rapid travel from where the change left the spindle to above
    /// the operation's first point at the safe height.
    Travel,
}

/// What the reader found of one operation's tool change.
#[derive(Default)]
struct ChangeRead {
    unload: Vec<GMove>,
    load: Vec<GMove>,
    /// Where the gauge line was when the change's lines began and when
    /// they ended (machine coordinates).
    before: Known,
    after: Known,
    /// The line of the first move after the change's lines.
    next_line: Option<usize>,
}

/// Reads the posted program with the tool in the spindle that each
/// change's lines say.
fn read_changes(
    program: &Program,
    placement: &Placement,
    posted: &Posted,
) -> Result<Vec<ChangeRead>, GcodeError> {
    let ops = &program.operations;
    let mut offsets = [None; 6];
    offsets[usize::from(program.work_offset.clamp(1, 6)) - 1] = Some(placement.work_origin);
    let lengths: BTreeMap<u32, f64> = ops
        .iter()
        .map(|op| (op.tool.number, op.tool.length))
        .collect();
    let mut reader = GcodeReader::new(offsets, lengths);
    reader.follow_tools = false;
    let lines: Vec<&str> = posted.text.lines().collect();
    // Per line (1-based): the change it belongs to, and whether it is one
    // of the unload lines.
    let mut role: Vec<Option<(usize, bool)>> = vec![None; lines.len() + 1];
    for (i, change) in posted.changes.iter().enumerate() {
        let Some(c) = change else {
            continue;
        };
        for (span, unload) in [(c.unload, true), (c.load, false)] {
            for n in span.first..span.first + span.count {
                if let Some(r) = role.get_mut(n) {
                    *r = Some((i, unload));
                }
            }
        }
    }
    let mut reads: Vec<ChangeRead> = ops.iter().map(|_| ChangeRead::default()).collect();
    let mut current: Option<(usize, bool)> = None;
    let mut waiting: Option<usize> = None;
    for (k, text) in lines.iter().enumerate() {
        let n = k + 1;
        let here = role[n];
        if here != current {
            if let Some((i, _)) = current
                && here.is_none_or(|(j, _)| j != i)
            {
                reads[i].after = reader.at();
                waiting = Some(i);
            }
            if let Some((i, unload)) = here {
                if current.is_none_or(|(j, _)| j != i) {
                    reads[i].before = reader.at();
                }
                let tool = if unload {
                    i.checked_sub(1).map(|j| ops[j].tool.number)
                } else {
                    Some(ops[i].tool.number)
                };
                reader.set_tool(tool);
            }
            current = here;
        }
        let moves = reader.line(text, n)?;
        match here {
            Some((i, true)) => reads[i].unload.extend(moves),
            Some((i, false)) => reads[i].load.extend(moves),
            None => {
                if let Some(i) = waiting
                    && let Some(m) = moves.first()
                {
                    reads[i].next_line = Some(m.line);
                    waiting = None;
                }
            }
        }
    }
    if let Some((i, _)) = current {
        reads[i].after = reader.at();
    }
    Ok(reads)
}

/// Which obstacles are the rack: zones with a slot of the macro changer
/// inside their box in X and Y.
fn rack_zones(machine: &Machine, placement: &Placement, obstacles: &[Obstacle]) -> Vec<bool> {
    let ToolChanger::Macro(mc) = &machine.tool_changer else {
        return vec![false; obstacles.len()];
    };
    let o = placement.work_origin;
    obstacles
        .iter()
        .map(|ob| {
            let ObstacleShape::Box { bounds } = &ob.shape else {
                return false;
            };
            ob.kind == ObstacleKind::Zone
                && mc.slots.iter().any(|s| {
                    (0..2).all(|k| {
                        let v = s.position[k] - o[k];
                        (bounds.min[k] - 1e-9..=bounds.max[k] + 1e-9).contains(&v)
                    })
                })
        })
        .collect()
}

/// The tip's legs (work coordinates) of a read move of the gauge line,
/// with a tool of `length` in the spindle: straight moves as they are (a
/// controller's G0 runs straight), arcs in the XY plane as the check's
/// arcs, arcs in the other planes as chords within the tolerance.
fn read_legs(m: &GMove, origin: P3, length: f64, tolerance: f64) -> Vec<Leg> {
    let Some((a, b)) = m.ends() else {
        return Vec::new();
    };
    let tip = |p: P3| {
        [
            p[0] - origin[0],
            p[1] - origin[1],
            p[2] - origin[2] - length,
        ]
    };
    match m.motion {
        Motion::Rapid | Motion::Feed => vec![Leg::Line {
            a: tip(a),
            b: tip(b),
        }],
        Motion::Arc {
            center,
            ccw,
            plane: Plane::Xy,
        } => {
            let c = tip(center);
            let arc = Move::Arc {
                to: tip(b),
                center: [c[0], c[1]],
                ccw,
                feed: 1.0,
            };
            legs(tip(a), &arc).into_iter().map(|(leg, _)| leg).collect()
        }
        Motion::Arc { .. } => {
            let mut from = tip(a);
            m.points(tolerance)
                .into_iter()
                .map(|p| {
                    let to = tip(p);
                    let leg = Leg::Line { a: from, b: to };
                    from = to;
                    leg
                })
                .collect()
        }
    }
}

fn message(step: ChangeMove, op: &str, part: PartKind, obstacle: &str) -> String {
    match step {
        ChangeMove::Travel => format!(
            "The rapid move to {} after the tool change: {} runs into {}.",
            op,
            part.words(),
            obstacle
        ),
        ChangeMove::Unload { .. } | ChangeMove::Load { .. } => format!(
            "The tool change before {}: {} runs into {}.",
            op,
            part.words(),
            obstacle
        ),
    }
}

fn parts_of(model: &SpindleModel, tool: &Tool) -> Vec<PreparedPart> {
    SpindleAssembly::new(model, tool)
        .parts
        .iter()
        .map(prepare_part)
        .collect()
}

/// The check of the changes, its contacts and clearance.
struct Run<'a> {
    obstacles: &'a [PreparedObstacle],
    rack: &'a [bool],
    tolerance: f64,
    report: CollisionReport,
    best: Best,
}

impl Run<'_> {
    /// Sweeps the parts along one leg of a change.
    #[allow(clippy::too_many_arguments)]
    fn leg(
        &mut self,
        operation: usize,
        op: &ProgramOp,
        parts: &[PreparedPart],
        leg: Leg,
        step: ChangeMove,
        line: Option<usize>,
        rapid: bool,
        seen: &mut Vec<(PartKind, usize)>,
    ) {
        let in_rack = step != ChangeMove::Travel;
        let mut done: Vec<Vec<bool>> = parts
            .iter()
            .map(|p| {
                (0..self.obstacles.len())
                    .map(|b| (in_rack && self.rack[b]) || seen.contains(&(p.kind, b)))
                    .collect()
            })
            .collect();
        let before = self.report.collisions.len();
        // As a rapid, so that the cutter counts in the stock too.
        let at = LegAt {
            operation,
            move_index: 0,
            line: 0,
            rapid: true,
            leg,
        };
        check_leg(
            &at,
            parts,
            self.obstacles,
            // The stock whole, as before the cut: a change runs above it,
            // and taking it whole is the safe side.
            &[],
            &[],
            self.tolerance,
            &mut done,
            &mut self.best,
            &mut self.report,
            op,
            false,
        );
        for c in &mut self.report.collisions[before..] {
            seen.push((c.part, c.obstacle_index));
            c.message = message(
                step,
                &op.name,
                c.part,
                &self.obstacles[c.obstacle_index].name,
            );
            c.change = Some(step);
            c.line = line;
            c.rapid = rapid;
            c.leg = 0;
        }
        // Every clearance this run finds is on a change.
        if let Some(c) = &mut self.best.clearance
            && c.change.is_none()
        {
            c.change = Some(step);
        }
    }

    /// Sweeps the parts along the read moves of a changer's lines.
    #[allow(clippy::too_many_arguments)]
    fn lines(
        &mut self,
        operation: usize,
        op: &ProgramOp,
        tool: &Tool,
        parts: &[PreparedPart],
        moves: &[GMove],
        first_line: usize,
        unload: bool,
        origin: P3,
        seen: &mut Vec<(PartKind, usize)>,
    ) {
        for m in moves {
            let index = m.line + 1 - first_line.min(m.line);
            let step = if unload {
                ChangeMove::Unload { line: index }
            } else {
                ChangeMove::Load { line: index }
            };
            let rapid = m.motion == Motion::Rapid;
            for leg in read_legs(m, origin, tool.length, self.tolerance) {
                self.leg(operation, op, parts, leg, step, Some(m.line), rapid, seen);
            }
        }
    }
}

/// Where the travel after the change before operation `i` starts (the
/// gauge line, machine coordinates), if it has one.
fn travel_start(
    i: usize,
    by_program: bool,
    read: &ChangeRead,
    program: &Program,
    machine: &Machine,
    placement: &Placement,
) -> Option<P3> {
    if by_program && let Some(end) = known(read.after) {
        return Some(end);
    }
    if i == 0 && !by_program {
        return None;
    }
    if let Some(park) = machine.park {
        return Some(park);
    }
    let o = placement.work_origin;
    let first = program.operations[i].toolpath.moves.first()?.to();
    let (x, y) = match (read.before[0], read.before[1]) {
        (Some(x), Some(y)) => (x, y),
        _ => (first[0] + o[0], first[1] + o[1]),
    };
    Some([x, y, machine.travel.max[2]])
}

/// Checks the program's tool changes for collisions of the spindle
/// assembly with the obstacles (work coordinates, as for
/// [`check_collisions`]): a macro changer's own moves, read from the
/// posted program, and the rapid travel after each change (the module's
/// documentation). `posted` is the program as
/// [`crate::post::post_with_lines`] wrote it for this machine; a line the
/// reader does not follow is an error with its line.
pub fn check_tool_changes(
    program: &Program,
    machine: &Machine,
    placement: &Placement,
    model: &SpindleModel,
    obstacles: &[Obstacle],
    posted: &Posted,
    options: &CollisionOptions,
) -> Result<CollisionReport, GcodeError> {
    let ops = &program.operations;
    if posted.changes.len() != ops.len() {
        return Err(GcodeError {
            line: 0,
            message: "The posted program is not the program being checked.".into(),
        });
    }
    let tolerance = if options.arc_tolerance.is_finite() && options.arc_tolerance > 0.0 {
        options.arc_tolerance
    } else {
        default_arc_tolerance()
    };
    let reads = read_changes(program, placement, posted)?;
    let prepared: Vec<PreparedObstacle> = obstacles.iter().map(prepare_obstacle).collect();
    let rack = rack_zones(machine, placement, obstacles);
    let origin = placement.work_origin;
    let mut run = Run {
        obstacles: &prepared,
        rack: &rack,
        tolerance,
        report: CollisionReport::default(),
        best: Best { clearance: None },
    };
    for (i, op) in ops.iter().enumerate() {
        let Some(change) = posted.changes[i] else {
            continue;
        };
        let read = &reads[i];
        let mut seen: Vec<(PartKind, usize)> = Vec::new();
        if let Some(prev) = i.checked_sub(1).map(|j| &ops[j].tool)
            && !read.unload.is_empty()
        {
            let parts = parts_of(model, prev);
            run.lines(
                i,
                op,
                prev,
                &parts,
                &read.unload,
                change.unload.first,
                true,
                origin,
                &mut seen,
            );
        }
        let parts = parts_of(model, &op.tool);
        if change.by_program {
            run.lines(
                i,
                op,
                &op.tool,
                &parts,
                &read.load,
                change.load.first,
                false,
                origin,
                &mut seen,
            );
        }
        let Some(first) = op.toolpath.moves.first().map(Move::to) else {
            continue;
        };
        let Some(start) = travel_start(i, change.by_program, read, program, machine, placement)
        else {
            continue;
        };
        // The tip, in work coordinates, under the gauge line.
        let e = [
            start[0] - origin[0],
            start[1] - origin[1],
            start[2] - origin[2] - op.tool.length,
        ];
        let safe = [e[0], e[1], program.safe_z];
        let above = [first[0], first[1], first[2].max(program.safe_z)];
        let first_line = posted
            .moves
            .get(i)
            .and_then(|m| m.first())
            .filter(|l| l.count > 0)
            .map(|l| l.first);
        let mut travel: Vec<(Leg, Option<usize>)> = legs(e, &Move::Rapid { to: safe })
            .into_iter()
            .map(|(leg, _)| (leg, read.next_line.or(first_line)))
            .collect();
        travel.extend(
            legs(safe, &Move::Rapid { to: above })
                .into_iter()
                .map(|(leg, _)| (leg, first_line)),
        );
        for (leg, line) in travel {
            run.leg(
                i,
                op,
                &parts,
                leg,
                ChangeMove::Travel,
                line,
                true,
                &mut seen,
            );
        }
    }
    let mut report = run.report;
    report.clearance = run.best.clearance;
    Ok(report)
}

/// The whole check of a posted program: the operations' moves
/// ([`check_collisions`], with their lines) and the tool changes
/// ([`check_tool_changes`]), in the program's order.
pub fn check_with_tool_changes(
    program: &Program,
    machine: &Machine,
    placement: &Placement,
    model: &SpindleModel,
    obstacles: &[Obstacle],
    posted: &Posted,
    options: &CollisionOptions,
) -> Result<CollisionReport, GcodeError> {
    let mut report = check_collisions(program, model, obstacles, options);
    report.set_lines(&posted.moves);
    report.add_tool_changes(check_tool_changes(
        program, machine, placement, model, obstacles, posted, options,
    )?);
    Ok(report)
}

impl CollisionReport {
    /// Adds the contacts of the tool changes ([`check_tool_changes`]): each
    /// before its operation's own, and the smaller clearance.
    pub fn add_tool_changes(&mut self, changes: CollisionReport) {
        for c in changes.collisions {
            let at = self
                .collisions
                .iter()
                .position(|x| x.operation >= c.operation && x.change.is_none())
                .unwrap_or(self.collisions.len());
            self.collisions.insert(at, c);
        }
        if let Some(c) = changes.clearance
            && self
                .clearance
                .as_ref()
                .is_none_or(|mine| c.distance < mine.distance)
        {
            self.clearance = Some(c);
        }
    }
}

#[cfg(test)]
mod tests;
