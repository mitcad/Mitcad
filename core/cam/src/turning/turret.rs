// SPDX-License-Identifier: MIT
//! The turret's body against the main chuck and the machine's fixed zones
//! (mitcad#2412): the box round the tool ([`super::TurretBody`], the tool's
//! station's own where it has one, [`super::Lathe::turret_body_at`]) at
//! each of the tool's points along its moves, as the tool's own point is
//! checked against them in [`super::check_lathe`]. Its hits against the
//! sub-spindle and the zones moving with it are
//! [`super::sub_spindle_hits`]' (mitcad#2405).

use super::PXZ;
use super::TurningProgram;
use super::check::{TurnPlacement, live_move_points, turn_move_points};
use super::lathe::nearest_zero;
use crate::machine::{Bounds, Finding, Machine};

const EPS: f64 = 1e-9;

/// Whether a lathe's zone reaches the machine's Y `y` (mitcad#2412): a
/// zone whose Y bounds are equal, as a lathe's zones were entered before
/// they had a Y (and are on a lathe without a Y axis), is not limited in
/// Y.
pub(crate) fn zone_reaches_y(b: &Bounds, y: f64) -> bool {
    b.max[1] <= b.min[1] || (y >= b.min[1] - EPS && y <= b.max[1] + EPS)
}

/// The moves that bring the turret's body into the main chuck (no part of
/// it behind the chuck's jaws' face within its diameter) or into one of the
/// machine's fixed zones (X and Z, and Y on a lathe with a Y axis, turning
/// at its Y 0): one finding per operation and part, in words ("Turn Facing
/// 1: the turret runs into the chuck.", "Drill 1: the turret runs into
/// Tailstock."), its place in `at` the tool's where the view draws the
/// move, as [`super::check_lathe`] gives the tool's. The sub-spindle's
/// operations where it holds the part (pulled back as the hand-over leaves
/// it). Operations without a turret body, and those the lathe cannot cut
/// (a sub-spindle or live tools it does not have), give none.
pub fn turret_hits(
    machine: &Machine,
    program: &TurningProgram,
    placement: &TurnPlacement,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let Some(lathe) = &machine.lathe else {
        return out;
    };
    let chuck_r = lathe.chuck_diameter.max(0.0) / 2.0;
    for (i, op) in program.operations.iter().enumerate() {
        let Some(body) = lathe.turret_body_at(op.tool.number) else {
            continue;
        };
        if (op.sub && !lathe.axes.sub_spindle) || (op.live.is_some() && !lathe.mills()) {
            continue;
        }
        let held = program.hand_over.as_ref().filter(|_| op.sub);
        // Y on a lathe with a Y axis, turning at its Y 0.
        let y_checked = lathe.has_y();
        let mut chuck: Option<[f64; 3]> = None;
        let mut zones: Vec<Option<[f64; 3]>> = vec![None; machine.zones.len()];
        // At a point of the tool: the machine's X and Y, the main
        // spindle's work Z where the part is as it cuts, and where the view
        // draws it.
        let mut test = |mx: f64, my: f64, z: f64, shown: [f64; 3]| {
            let (x, along) = body.at(mx, z);
            if let Some(cz) = placement.chuck_z
                && chuck.is_none()
                && chuck_r > 0.0
                && along[0] < cz - EPS
                && nearest_zero(x).hypot(my) < chuck_r + EPS
            {
                chuck = Some(shown);
            }
            let machine_z = [along[0] + placement.work_z, along[1] + placement.work_z];
            for (k, zone) in machine.zones.iter().enumerate() {
                let b = &zone.bounds;
                let overlaps = |r: [f64; 2], axis: usize| {
                    r[1] >= b.min[axis] - EPS && r[0] <= b.max[axis] + EPS
                };
                if zones[k].is_none()
                    && overlaps(x, 0)
                    && overlaps(machine_z, 2)
                    && (!y_checked || zone_reaches_y(b, my))
                {
                    zones[k] = Some(shown);
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
                    // The machine's Y: the sub-spindle's turned round.
                    let y = if held.is_some() { -m[1] } else { m[1] };
                    test(m[0], y, z, shown);
                }
            }
            None => {
                let mut from: Option<PXZ> = None;
                for m in &op.path.moves {
                    for p in turn_move_points(from, m) {
                        let z = held.map_or(p[1], |h| h.held_z(p[1]));
                        let shown = held.map_or(p[1], |h| h.main_z(p[1]));
                        test(p[0], 0.0, z, [p[0], 0.0, shown]);
                    }
                    from = Some(m.to());
                }
            }
        }
        let name = &op.name;
        if let Some(at) = chuck {
            out.push(Finding {
                message: format!("{name}: the turret runs into the chuck."),
                operation: i,
                at: Some(at),
            });
        }
        for (k, hit) in zones.into_iter().enumerate() {
            if let Some(at) = hit {
                out.push(Finding {
                    message: format!("{name}: the turret runs into {}.", machine.zones[k].name),
                    operation: i,
                    at: Some(at),
                });
            }
        }
    }
    out
}
