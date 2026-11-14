// SPDX-License-Identifier: MIT
//! Facing: the stock's top is flattened down to a height by rows back and
//! forth along X across a rectangle, the turns outside it; or, when
//! asked, adaptively by loops from the outside in (`adaptive`).

use serde::{Deserialize, Serialize};

use super::adaptive::{Adaptive, adaptive_face};
use super::{Depths, Direction, Heights, check_mill};
use crate::CamError;
use crate::tool::{Cutting, Tool};
use crate::toolpath::{PathBuilder, Toolpath};

/// A facing operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FaceOp {
    /// The corners of the area to face (usually the stock's outline).
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub depths: Depths,
    pub heights: Heights,
    /// The distance between rows (mm), at most the diameter; `None`: 60 %
    /// of the diameter.
    #[serde(default)]
    pub stepover: Option<f64>,
    pub cutting: Cutting,
    /// The way round the adaptive loops (the rows go both ways).
    #[serde(default)]
    pub direction: Direction,
    /// Clear by loops from the outside in that keep the engagement within
    /// a limit, instead of the rows.
    #[serde(default)]
    pub adaptive: Option<Adaptive>,
}

/// The toolpath of a facing operation.
pub fn face(op: &FaceOp, tool: &Tool) -> Result<Toolpath, CamError> {
    if let Some(a) = &op.adaptive {
        return adaptive_face(op, tool, a);
    }
    check_mill(tool, &op.cutting, &op.depths)?;
    op.heights.validate(op.depths.top)?;
    let levels = op.depths.levels()?;
    let s = op.stepover.unwrap_or(tool.diameter * 0.6);
    if !(s.is_finite() && s > 0.0 && s <= tool.diameter + 1e-9) {
        return Err(CamError::new(
            "The stepover must be more than zero and not more than the tool's diameter.",
        ));
    }
    let (x0, x1) = (op.min[0].min(op.max[0]), op.min[0].max(op.max[0]));
    let (y0, y1) = (op.min[1].min(op.max[1]), op.min[1].max(op.max[1]));
    if x1 - x0 <= 0.0 || y1 - y0 <= 0.0 {
        return Err(CamError::new("The area to face is empty."));
    }
    let r = tool.radius();
    // Rows from edge to edge with the tool center on each edge, no more
    // than the stepover apart; the rows start and end clear of the area.
    let rows = ((y1 - y0) / s - 1e-9).ceil().max(0.0) as usize + 1;
    let ys: Vec<f64> = (0..rows)
        .map(|i| {
            if rows == 1 {
                (y0 + y1) / 2.0
            } else {
                y0 + (y1 - y0) * i as f64 / (rows - 1) as f64
            }
        })
        .collect();
    let (left, right) = (x0 - r - 1.0, x1 + r + 1.0);
    let mut b = PathBuilder::new();
    for (k, &z) in levels.iter().enumerate() {
        // Each level starts where the last ended, so that the rows are
        // walked back the other way.
        let rows_here: Vec<f64> = if k % 2 == 0 {
            ys.clone()
        } else {
            ys.iter().rev().copied().collect()
        };
        let mut forward = true;
        let start = [left, rows_here[0]];
        b.retract_to(start, op.heights.clearance);
        b.rapid([start[0], start[1], op.heights.retract]);
        b.line([start[0], start[1], z], op.cutting.plunge_feed);
        for (i, &y) in rows_here.iter().enumerate() {
            let here = b.at().expect("the tool is placed");
            if i > 0 {
                b.line([here[0], y, z], op.cutting.feed);
            }
            b.line([if forward { right } else { left }, y, z], op.cutting.feed);
            forward = !forward;
        }
        let end = b.at().expect("the tool is placed");
        b.rapid([end[0], end[1], op.heights.clearance]);
    }
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolpath::Move;

    #[test]
    fn rows_cover_the_area_and_turn_outside_it() {
        let tool = Tool::flat_end_mill(1, 10.0);
        let op = FaceOp {
            min: [0.0, 0.0],
            max: [100.0, 20.0],
            depths: Depths {
                top: 1.0,
                bottom: 0.0,
                step_down: 1.0,
            },
            heights: Heights {
                clearance: 10.0,
                retract: 3.0,
            },
            stepover: None,
            cutting: Cutting {
                spindle_rpm: 12000.0,
                feed: 2000.0,
                plunge_feed: 500.0,
            },
            direction: Direction::Climb,
            adaptive: None,
        };
        let p = face(&op, &tool).unwrap();
        let rows: Vec<f64> = p
            .moves
            .iter()
            .filter_map(|m| match m {
                Move::Line { to, .. } if to[0] == 106.0 || to[0] == -6.0 => Some(to[1]),
                _ => None,
            })
            .collect();
        // 20 mm at a 6 mm stepover: five rows 5 mm apart (the plunge, then
        // each row's step over and cut).
        assert_eq!(
            rows,
            vec![0.0, 0.0, 5.0, 5.0, 10.0, 10.0, 15.0, 15.0, 20.0, 20.0]
        );
        assert!(p.moves.iter().all(|m| m.to()[2] >= 0.0));
    }
}
