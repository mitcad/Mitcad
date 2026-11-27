// SPDX-License-Identifier: MIT
//! The simulation of the turned stock: its section through the spindle
//! axis, revolved. At each node of a row along Z the stock is the ring
//! between an inner and an outer radius; each tool takes away what its
//! insert covers there as it moves (the nose's circle of a turning tool
//! or a boring bar and its edge behind it, rising at the tool's back
//! angle when it has one, the width of a grooving or parting insert). A
//! thread's helix leaves the crest standing in any section, so threading
//! takes nothing away here.

use serde::{Deserialize, Serialize};

use super::profile::{PV, piece_at, pieces};
use super::tool::{TurningTool, TurningToolKind};
use super::{PXZ, TurnMove, TurnPart, TurningProgram, move_points};
use crate::machine::Finding;

/// The longest a trailing edge reaches back along Z from the nose (mm):
/// an insert's edge is shorter.
const TRAILING_RUN: f64 = 25.0;

/// The stock as cut: per node `z[i]`, the ring from `inner[i]` to
/// `outer[i]` (radii); none left there when they meet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnStock {
    pub z: Vec<f64>,
    pub inner: Vec<f64>,
    pub outer: Vec<f64>,
}

impl TurnStock {
    /// The part's stock from `z_min` to its face, nodes at most `step`
    /// apart.
    pub fn new(part: &TurnPart, z_min: f64, step: f64) -> Self {
        let z_max = part.stock_face;
        let n = (((z_max - z_min) / step).ceil() as usize).clamp(1, 1_000_000);
        let z: Vec<f64> = (0..=n)
            .map(|i| z_min + (z_max - z_min) * i as f64 / n as f64)
            .collect();
        TurnStock {
            inner: vec![part.stock_inner_radius; z.len()],
            outer: vec![part.stock_radius; z.len()],
            z,
        }
    }

    /// The stock turned round about `back`: each node's Z becomes `back -
    /// z`, from the lowest up (the sub-spindle's coordinates and back,
    /// mitcad#2319).
    pub fn turn_round(&mut self, back: f64) {
        self.z = self.z.iter().rev().map(|z| back - z).collect();
        self.inner.reverse();
        self.outer.reverse();
    }

    fn step(&self) -> f64 {
        if self.z.len() < 2 {
            1.0
        } else {
            self.z[1] - self.z[0]
        }
    }

    /// The nodes within `lo..=hi`.
    fn nodes(&self, lo: f64, hi: f64) -> std::ops::Range<usize> {
        if self.z.is_empty() {
            return 0..0;
        }
        let z0 = self.z[0];
        let step = self.step();
        let a = ((lo - z0) / step - 1e-9).ceil().max(0.0) as usize;
        let b = (((hi - z0) / step + 1e-9).floor() + 1.0).max(0.0) as usize;
        a.min(self.z.len())..b.min(self.z.len())
    }

    /// Takes away what the tool covers with its controlled point at `tip`;
    /// returns the deepest it went into material (radius). A turning tool
    /// or a boring bar covers its nose's circle and, behind it, its edge as
    /// far as it cuts deep; a grooving or parting insert its width.
    fn cut(&mut self, tool: &TurningTool, tip: PXZ) -> f64 {
        let r = tool.nose_radius;
        let half = 0.5 * self.step();
        let round = r > half;
        let edge = tool.max_depth.max(2.0 * r).max(2.0 * half);
        let mut deepest: f64 = 0.0;
        let (hi_z, outside) = match tool.kind {
            TurningToolKind::Threading { .. } | TurningToolKind::Live => return 0.0,
            TurningToolKind::Turning => (tip[1] + edge, true),
            TurningToolKind::Boring { .. } => (tip[1] + edge, false),
            TurningToolKind::Grooving { width, inside } => (tip[1] + width, !inside),
            TurningToolKind::Parting { width } => (tip[1] + width, true),
        };
        let s = if outside { 1.0 } else { -1.0 };
        let nosed = matches!(
            tool.kind,
            TurningToolKind::Turning | TurningToolKind::Boring { .. }
        );
        // With its back angle known (mitcad#2503) the trailing edge rises
        // behind the nose at that angle, tangent to the nose, until it is
        // as high above the tip as the insert cuts deep.
        let trailing = tool.back_angle.filter(|_| nosed).map(|b| {
            let b = b.clamp(0.0, 89.0).to_radians();
            let (cx, cz) = (tip[0] + s * r, tip[1] + r);
            // Where the edge leaves the nose: square to it from the centre.
            let t = [cx - s * r * b.cos(), cz + r * b.sin()];
            (cx, cz, t, b.tan())
        });
        let hi_z = match trailing {
            Some((_, _, t, slope)) => {
                let run = if slope > 1e-6 {
                    (edge / slope).min(TRAILING_RUN)
                } else {
                    TRAILING_RUN
                };
                hi_z.max(t[1] + run)
            }
            None => hi_z,
        };
        for i in self.nodes(tip[1], hi_z) {
            if self.outer[i] <= self.inner[i] {
                continue;
            }
            let z = self.z[i];
            // How far the insert reaches towards the material at z.
            let reach = if let Some((cx, cz, t, slope)) = trailing {
                if z > t[1] {
                    t[0] + s * (z - t[1]) * slope
                } else if round {
                    let h = (r * r - (z - cz).powi(2)).max(0.0).sqrt();
                    cx - s * h
                } else {
                    tip[0]
                }
            } else if nosed && round {
                let cz = tip[1] + r;
                if z <= cz {
                    let h = (r * r - (z - cz).powi(2)).max(0.0).sqrt();
                    tip[0] + s * (r - h)
                } else {
                    tip[0] + s * r
                }
            } else {
                tip[0]
            };
            if outside {
                if reach < self.outer[i] {
                    deepest = deepest.max(self.outer[i] - reach.max(self.inner[i]));
                    self.outer[i] = reach.max(self.inner[i]);
                }
            } else if reach > self.inner[i] {
                deepest = deepest.max(reach.min(self.outer[i]) - self.inner[i]);
                self.inner[i] = reach.min(self.outer[i]);
            }
        }
        deepest
    }
}

/// What the simulation found.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnedStock {
    pub stock: TurnStock,
    /// The most material left over the part (radius, mm) and where
    /// (`[x, z]`).
    pub left: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_at: Option<PXZ>,
    /// Where the tools cut into the part beyond the tolerance (the first
    /// few places).
    pub gouges: Vec<PXZ>,
    /// Rapid moves that took a tool into the stock, one per operation.
    pub rapid_cuts: Vec<Finding>,
}

/// The extreme X of a chain at `z`: its largest, or its smallest when
/// `min`; none where it does not reach.
fn x_at(chain: &[PV], z: f64, min: bool) -> Option<f64> {
    let mut best: Option<f64> = None;
    for p in pieces(chain) {
        if let Some(q) = piece_at(&p, 1, z) {
            best = Some(match best {
                None => q[0],
                Some(b) if min => b.min(q[0]),
                Some(b) => b.max(q[0]),
            });
        }
    }
    best
}

/// Where the part-off before the hand-over (the operation before the
/// first on the sub-spindle, a parting blade's) cut the part free: the
/// work Z of the blade's side towards the face (mitcad#2319).
pub fn parted_at(program: &TurningProgram, first_sub: usize) -> Option<f64> {
    let op = program.operations.get(first_sub.checked_sub(1)?)?;
    let TurningToolKind::Parting { width } = op.tool.kind else {
        return None;
    };
    let lowest = op
        .path
        .moves
        .iter()
        .map(|m| m.to()[1])
        .fold(f64::INFINITY, f64::min);
    lowest.is_finite().then_some(lowest + width)
}

/// The stock as the sub-spindle holds it (mitcad#2319, mitcad#2375): the
/// remnant the part-off before the hand-over leaves on the main spindle
/// gone, turned round into the sub-spindle's work coordinates; as it is
/// without a hand-over or an operation on the sub-spindle.
pub fn on_sub_spindle(program: &TurningProgram, stock: &mut TurnStock) {
    let (Some(hand_over), Some(first)) = (&program.hand_over, program.first_sub()) else {
        return;
    };
    if let Some(cut) = parted_at(program, first) {
        for i in 0..stock.z.len() {
            if stock.z[i] < cut {
                stock.outer[i] = stock.inner[i];
            }
        }
    }
    stock.turn_round(hand_over.back_z);
}

/// Cuts one operation's toolpath out of the stock, its findings at the
/// main spindle's work Z (`hand_over`: on the sub-spindle, mitcad#2319).
fn cut_operation(
    stock: &mut TurnStock,
    index: usize,
    op: &super::TurningProgramOp,
    step: f64,
    rapid_cuts: &mut Vec<Finding>,
    hand_over: Option<&super::HandOver>,
) {
    let mut from: Option<PXZ> = None;
    let mut reported = false;
    for m in &op.path.moves {
        let Some(f) = from else {
            from = Some(m.to());
            continue;
        };
        let mut prev = f;
        for q in move_points(f, m, step / 4.0) {
            let d = (q[0] - prev[0]).hypot(q[1] - prev[1]);
            let n = ((d / (step / 2.0)).ceil() as usize).max(1);
            for k in 1..=n {
                let t = k as f64 / n as f64;
                let p = [
                    prev[0] + (q[0] - prev[0]) * t,
                    prev[1] + (q[1] - prev[1]) * t,
                ];
                let depth = stock.cut(&op.tool, p);
                if matches!(m, TurnMove::Rapid { .. }) && depth > 0.001 && !reported {
                    reported = true;
                    let z = hand_over.map_or(p[1], |h| h.main_z(p[1]));
                    rapid_cuts.push(Finding {
                        message: format!(
                            "{}: a rapid move takes the tool into the stock.",
                            op.name
                        ),
                        operation: index,
                        at: Some([p[0], 0.0, z]),
                    });
                }
            }
            prev = q;
        }
        from = Some(m.to());
    }
}

/// Cuts the program's toolpaths out of the part's stock, from `z_min`
/// (towards the chuck) to the stock's face on nodes `step` apart, and
/// compares what is left with the part within `tolerance`.
pub fn simulate_turning(
    part: &TurnPart,
    program: &TurningProgram,
    z_min: f64,
    step: f64,
    tolerance: f64,
) -> TurnedStock {
    let step = step.max(0.005);
    let mut stock = TurnStock::new(part, z_min, step);
    let mut rapid_cuts = Vec::new();
    for (index, op) in program.operations.iter().enumerate() {
        if !op.sub {
            cut_operation(&mut stock, index, op, step, &mut rapid_cuts, None);
        }
    }
    // The sub-spindle's operations (mitcad#2319): the remnant the part-off
    // leaves behind stays on the main spindle; the rest is cut in the
    // sub-spindle's coordinates, the stock turned round.
    if let (Some(hand_over), Some(first)) = (&program.hand_over, program.first_sub()) {
        on_sub_spindle(program, &mut stock);
        for (index, op) in program.operations.iter().enumerate().skip(first) {
            cut_operation(
                &mut stock,
                index,
                op,
                step,
                &mut rapid_cuts,
                Some(hand_over),
            );
        }
        stock.turn_round(hand_over.back_z);
    }
    // Compare with the part between its face and its back: at each node
    // the part's lowest and highest radius round it (a node on a wall or
    // the face is neither left material nor a gouge).
    let (face, back) = (part.face_z(), part.back_z());
    let step = stock.step();
    let around = |chain: &[PV], z: f64, min: bool| -> [f64; 2] {
        let xs: Vec<f64> = [z - step, z, z + step]
            .into_iter()
            .map(|zz| x_at(chain, zz, min).unwrap_or(0.0))
            .collect();
        [
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ]
    };
    let mut left = 0.0;
    let mut left_at = None;
    let mut gouges: Vec<PXZ> = Vec::new();
    for i in 0..stock.z.len() {
        let z = stock.z[i];
        let (inner, outer) = (stock.inner[i], stock.outer[i]);
        if z > face + 1e-9 {
            // In front of the face nothing should be left.
            let t = outer - inner;
            if t > tolerance && t > left {
                left = t;
                left_at = Some([outer, z]);
            }
            continue;
        }
        if z < back - 1e-9 {
            continue;
        }
        let [out_lo, out_hi] = around(&part.outside, z, false);
        let [in_lo, in_hi] = around(&part.inside, z, true);
        if outer <= inner {
            // Nothing left where the part is solid all round.
            if out_lo > in_hi + tolerance && gouges.len() < 20 {
                gouges.push([out_lo, z]);
            }
            continue;
        }
        if outer < out_lo - tolerance && gouges.len() < 20 {
            gouges.push([outer, z]);
        }
        if outer - out_hi > tolerance && outer - out_hi > left {
            left = outer - out_hi;
            left_at = Some([outer, z]);
        }
        if inner > in_hi + tolerance && gouges.len() < 20 {
            gouges.push([inner, z]);
        }
        if in_lo - inner > tolerance && in_lo - inner > left {
            left = in_lo - inner;
            left_at = Some([inner, z]);
        }
    }
    TurnedStock {
        stock,
        left,
        left_at,
        gouges,
        rapid_cuts,
    }
}
