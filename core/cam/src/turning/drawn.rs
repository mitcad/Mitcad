// SPDX-License-Identifier: MIT
//! The lathe's parts as *Simulate* draws them beside the sub-spindle
//! (mitcad#2412): the machine's fixed zones and the zones moving with the
//! sub-spindle where they are as each operation cuts, and the turret's
//! body round the tool. Everything in the main spindle's work coordinates
//! where the view draws the operation: the sub-spindle's operations on the
//! part where it was before the hand-over pulled it back
//! ([`super::sub_face_shown`]), so a fixed zone is drawn that much nearer
//! the main spindle while the sub-spindle holds the part.

use serde::{Deserialize, Serialize};

use super::live::LiveSide;
use super::{Lathe, TurningProgram, sub_face_shown};
use crate::machine::{Bounds, Machine};

/// A zone as the view draws it while an operation cuts: its box in the
/// main spindle's work coordinates where the view draws the operation, X
/// the machine's (the turret's plane through the spindle axis, the view's
/// X), Y the machine's on a lathe with a Y axis (`[0, 0]` where it is not
/// limited in Y or the lathe has none), Z along the spindle; `moving` for
/// a zone moving with the sub-spindle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ZoneShown {
    pub name: String,
    pub moving: bool,
    pub min: [f64; 3],
    pub max: [f64; 3],
}

/// The turret's body as the view draws it round the tool while an
/// operation cuts: `x` and `z` from the tool's controlled point, X along
/// `x_axis` (the machine's X in the view's X and Y: `[1, 0]` for turning,
/// the plane's C for a live tool on a plane; none: away from the spindle
/// axis through the tool's point, for a live tool on the face or the
/// diameter), Z along the view's Z (away from the main spindle).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurretShown {
    pub x: [f64; 2],
    pub z: [f64; 2],
    pub x_axis: Option<[f64; 2]>,
}

/// The zones' Y as drawn: none on a lathe without a Y axis, nor for a zone
/// not limited in Y (its Y bounds equal).
fn drawn_y(lathe: &Lathe, b: &Bounds) -> [f64; 2] {
    if lathe.has_y() && b.max[1] > b.min[1] {
        [b.min[1], b.max[1]]
    } else {
        [0.0, 0.0]
    }
}

/// The zones the view draws while operation `index` cuts: the machine's
/// fixed zones (with `work_z`, the machine Z of the work zero; none
/// without it), then those moving with the sub-spindle where it is
/// ([`sub_face_shown`]; none where that is not known).
pub fn zones_shown(
    machine: &Machine,
    program: &TurningProgram,
    index: usize,
    work_z: Option<f64>,
) -> Vec<ZoneShown> {
    let mut out = Vec::new();
    let (Some(lathe), Some(op)) = (&machine.lathe, program.operations.get(index)) else {
        return out;
    };
    // The view keeps the part where it was before the hand-over pulled it
    // back: the machine is drawn that much nearer the main spindle.
    let pulled = program
        .hand_over
        .as_ref()
        .filter(|_| op.sub)
        .map_or(0.0, |h| h.withdraw);
    if let Some(work_z) = work_z {
        for zone in &machine.zones {
            let b = &zone.bounds;
            let y = drawn_y(lathe, b);
            out.push(ZoneShown {
                name: zone.name.clone(),
                moving: false,
                min: [b.min[0], y[0], b.min[2] - work_z - pulled],
                max: [b.max[0], y[1], b.max[2] - work_z - pulled],
            });
        }
    }
    if lathe.axes.sub_spindle
        && let Some(face) = sub_face_shown(lathe, program, index, work_z)
    {
        for zone in &lathe.sub_spindle.zones {
            let b = &zone.bounds;
            let y = drawn_y(lathe, b);
            out.push(ZoneShown {
                name: zone.name.clone(),
                moving: true,
                min: [b.min[0], y[0], face + b.min[2]],
                max: [b.max[0], y[1], face + b.max[2]],
            });
        }
    }
    out
}

/// The turret's body the view draws round the tool while operation
/// `index` cuts: its tool's station's ([`Lathe::turret_body_at`]); none
/// without one.
pub fn turret_shown(lathe: &Lathe, program: &TurningProgram, index: usize) -> Option<TurretShown> {
    let op = program.operations.get(index)?;
    let body = lathe.turret_body_at(op.tool.number)?;
    // The sub-spindle's operations are drawn turned round about X: Y the
    // other way.
    let turned = op.sub && program.hand_over.is_some();
    let x_axis = match op.live.as_ref().map(|live| live.side) {
        None => Some([1.0, 0.0]),
        Some(LiveSide::Plane { c, .. }) => {
            let (s, cos) = c.to_radians().sin_cos();
            Some([cos, if turned { -s } else { s }])
        }
        Some(LiveSide::Face | LiveSide::Diameter { .. }) => None,
    };
    Some(TurretShown {
        x: body.x,
        z: body.z,
        x_axis,
    })
}
