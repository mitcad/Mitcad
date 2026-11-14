// SPDX-License-Identifier: MIT
//! Drilling: straight down at points, in pecks when asked. The moves are
//! written out here, so that the checks and the simulation see them and
//! every controller can run them; the toolpath also records them as one
//! drilling cycle, which a post-processor writes as a canned cycle (G81,
//! G83, G73) where the controller has one and the settings ask for it
//! (docs/cam.md, "Canned drilling cycles").

use serde::{Deserialize, Serialize};

use super::{Depths, Heights};
use crate::CamError;
use crate::tool::{Cutting, Tool};
use crate::toolpath::{CycleKind, DrillCycle, PathBuilder, Toolpath};

/// A drilling operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrillOp {
    /// The hole centers.
    pub points: Vec<[f64; 2]>,
    /// From the top to the bottom of the hole (the tip's Z);
    /// `step_down` is the peck depth when `peck`.
    pub depths: Depths,
    pub heights: Heights,
    /// Pecks of `depths.step_down`, back up to the retract height between
    /// them to clear the chips.
    #[serde(default)]
    pub peck: bool,
    /// With `peck`: back off a little after each peck to break the chip
    /// instead of going out of the hole.
    #[serde(default)]
    pub chip_break: bool,
    /// Whether a post-processor may write the holes as a canned cycle
    /// (when its settings ask for cycles and the controller has them).
    #[serde(default = "yes")]
    pub canned_cycle: bool,
    pub cutting: Cutting,
}

fn yes() -> bool {
    true
}

/// How far a chip-breaking peck backs off (mm).
pub const CHIP_BREAK: f64 = 0.5;

/// The toolpath of a drilling operation (plunges at the plunge feed).
pub fn drill(op: &DrillOp, tool: &Tool) -> Result<Toolpath, CamError> {
    tool.validate().map_err(CamError::new)?;
    op.cutting.validate().map_err(CamError::new)?;
    op.heights.validate(op.depths.top)?;
    if op.points.is_empty() {
        return Err(CamError::new("The drilling has no holes."));
    }
    if op
        .points
        .iter()
        .any(|p| !(p[0].is_finite() && p[1].is_finite()))
    {
        return Err(CamError::new("A hole's position is not a number."));
    }
    if op.depths.top - op.depths.bottom > tool.flute_length + 1e-9 {
        return Err(CamError::new(
            "The hole is deeper than the tool's flutes are long.",
        ));
    }
    let levels = if op.peck {
        op.depths.levels()?
    } else {
        if op.depths.bottom >= op.depths.top {
            return Err(CamError::new("The bottom must be below the top."));
        }
        vec![op.depths.bottom]
    };
    let feed = op.cutting.plunge_feed;
    let chip_break = op.peck && op.chip_break;
    let mut b = PathBuilder::new();
    let mut order: Vec<[f64; 2]> = Vec::new();
    // Nearest hole next.
    let mut left: Vec<[f64; 2]> = op.points.clone();
    let mut at = [0.0, 0.0];
    while !left.is_empty() {
        let i = (0..left.len())
            .min_by(|&a, &c| {
                let da = (left[a][0] - at[0]).hypot(left[a][1] - at[1]);
                let dc = (left[c][0] - at[0]).hypot(left[c][1] - at[1]);
                da.total_cmp(&dc)
            })
            .expect("a hole");
        let p = left.remove(i);
        order.push(p);
        b.retract_to(p, op.heights.clearance);
        b.rapid([p[0], p[1], op.heights.retract]);
        let mut done = op.heights.retract;
        for (k, &z) in levels.iter().enumerate() {
            if chip_break {
                // Straight on from where the last peck backed off to.
                b.line([p[0], p[1], z], feed);
                if k + 1 < levels.len() {
                    b.rapid([p[0], p[1], (z + CHIP_BREAK).min(op.heights.retract)]);
                }
                continue;
            }
            if done < op.heights.retract {
                // Back into the hole fast to just above where the last
                // peck ended.
                b.rapid([p[0], p[1], (done + 0.5).min(op.heights.retract)]);
            }
            b.line([p[0], p[1], z], feed);
            done = z;
            b.rapid([p[0], p[1], op.heights.retract]);
        }
        if chip_break {
            b.rapid([p[0], p[1], op.heights.retract]);
        }
        at = p;
    }
    let end = b.at().expect("the tool is placed");
    b.rapid([end[0], end[1], op.heights.clearance]);
    let mut path = b.finish();
    if op.canned_cycle {
        // Everything after the first rapid (to above the first hole at the
        // clearance height) is the cycle.
        let kind = match (op.peck, chip_break) {
            (false, _) => CycleKind::Drill,
            (true, false) => CycleKind::Peck,
            (true, true) => CycleKind::ChipBreak,
        };
        let peck = if op.peck {
            (op.depths.top - op.depths.bottom) / levels.len() as f64
        } else {
            0.0
        };
        path.cycles.push(DrillCycle {
            first: 1,
            count: path.moves.len() - 1,
            kind,
            holes: order,
            clearance: op.heights.clearance,
            retract: op.heights.retract,
            bottom: op.depths.bottom,
            peck,
            feed,
        });
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolpath::Move;

    fn op(peck: bool) -> DrillOp {
        DrillOp {
            points: vec![[30.0, 0.0], [10.0, 0.0]],
            depths: Depths {
                top: 0.0,
                bottom: -9.0,
                step_down: 3.0,
            },
            heights: Heights {
                clearance: 10.0,
                retract: 2.0,
            },
            peck,
            chip_break: false,
            canned_cycle: true,
            cutting: Cutting {
                spindle_rpm: 3000.0,
                feed: 200.0,
                plunge_feed: 150.0,
            },
        }
    }

    #[test]
    fn holes_are_drilled_nearest_first_in_one_go() {
        let mut t = Tool::flat_end_mill(5, 5.0);
        t.shape = crate::tool::ToolShape::Drill { point_angle: 118.0 };
        let p = drill(&op(false), &t).unwrap();
        let cuts: Vec<[f64; 3]> = p
            .moves
            .iter()
            .filter(|m| matches!(m, Move::Line { .. }))
            .map(|m| m.to())
            .collect();
        assert_eq!(cuts, vec![[10.0, 0.0, -9.0], [30.0, 0.0, -9.0]]);
        assert_eq!(p.moves.last().unwrap().to()[2], 10.0);
    }

    #[test]
    fn pecks_retract_and_come_back_above_the_last_depth() {
        let t = Tool::flat_end_mill(5, 5.0);
        let p = drill(&op(true), &t).unwrap();
        let first_hole: Vec<Move> = p.moves.iter().take(9).cloned().collect();
        assert_eq!(
            first_hole,
            vec![
                Move::Rapid {
                    to: [10.0, 0.0, 10.0]
                },
                Move::Rapid {
                    to: [10.0, 0.0, 2.0]
                },
                Move::Line {
                    to: [10.0, 0.0, -3.0],
                    feed: 150.0
                },
                Move::Rapid {
                    to: [10.0, 0.0, 2.0]
                },
                Move::Rapid {
                    to: [10.0, 0.0, -2.5]
                },
                Move::Line {
                    to: [10.0, 0.0, -6.0],
                    feed: 150.0
                },
                Move::Rapid {
                    to: [10.0, 0.0, 2.0]
                },
                Move::Rapid {
                    to: [10.0, 0.0, -5.5]
                },
                Move::Line {
                    to: [10.0, 0.0, -9.0],
                    feed: 150.0
                },
            ]
        );
    }

    #[test]
    fn a_hole_deeper_than_the_flutes_is_refused() {
        let mut t = Tool::flat_end_mill(5, 2.0);
        t.flute_length = 5.0;
        assert!(drill(&op(false), &t).is_err());
    }

    #[test]
    fn chip_breaking_pecks_back_off_a_little_and_the_holes_are_one_cycle() {
        let t = Tool::flat_end_mill(5, 5.0);
        let mut o = op(true);
        o.chip_break = true;
        let p = drill(&o, &t).unwrap();
        let first_hole: Vec<[f64; 3]> = p.moves.iter().take(8).map(|m| m.to()).collect();
        assert_eq!(
            first_hole,
            vec![
                [10.0, 0.0, 10.0],
                [10.0, 0.0, 2.0],
                [10.0, 0.0, -3.0],
                [10.0, 0.0, -2.5],
                [10.0, 0.0, -6.0],
                [10.0, 0.0, -5.5],
                [10.0, 0.0, -9.0],
                [10.0, 0.0, 2.0],
            ]
        );
        assert_eq!(
            p.cycles,
            vec![DrillCycle {
                first: 1,
                count: p.moves.len() - 1,
                kind: CycleKind::ChipBreak,
                holes: vec![[10.0, 0.0], [30.0, 0.0]],
                clearance: 10.0,
                retract: 2.0,
                bottom: -9.0,
                peck: 3.0,
                feed: 150.0,
            }]
        );
        assert_eq!(
            drill(&op(false), &t).unwrap().cycles[0].kind,
            CycleKind::Drill
        );
        assert_eq!(
            drill(&op(true), &t).unwrap().cycles[0].kind,
            CycleKind::Peck
        );
        o.canned_cycle = false;
        assert!(drill(&o, &t).unwrap().cycles.is_empty());
    }
}
