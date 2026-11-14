// SPDX-License-Identifier: MIT
//! The machine model: travel, spindle, feeds, the zones the tool and the
//! spindle must keep out of (clamps, a vise, a tool rack, the gantry's
//! parts), and the tool changer, with the check of a program against it
//! (docs/cam.md, "Machine model").
//!
//! Machine coordinates are the controller's: the controlled point is the
//! spindle's gauge line (the end of the spindle nose, where a tool's
//! length is measured from), so a tool's tip is `length` below it.

use serde::{Deserialize, Serialize};

use crate::program::Program;
use crate::tool::Tool;
use crate::toolpath::{Move, P3, move_points};

/// A box in machine coordinates (mm).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min: P3,
    pub max: P3,
}

impl Bounds {
    pub fn contains(&self, p: P3) -> bool {
        (0..3).all(|k| p[k] >= self.min[k] - 1e-9 && p[k] <= self.max[k] + 1e-9)
    }
}

/// A part of the machine or the fixture the tool and the spindle must
/// not touch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub name: String,
    pub bounds: Bounds,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spindle {
    pub min_rpm: f64,
    pub max_rpm: f64,
    /// The wait after starting the spindle before cutting (seconds).
    #[serde(default)]
    pub spin_up_seconds: f64,
    /// The diameter of the collet nut or holder and the spindle body
    /// above the tool, as one cylinder (mm).
    pub nose_diameter: f64,
    /// How far that cylinder reaches up from the gauge line (mm).
    pub nose_length: f64,
}

/// A tool's place in a rack, in machine coordinates: where the gauge line
/// is when the tool sits in its slot.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RackSlot {
    pub tool: u32,
    pub position: P3,
}

/// A tool changer the program drives itself: a rack of slots and the
/// G-code that puts a tool away and takes one (docs/cam.md, "Tool
/// changers"). In the lines, `{tool}` is the tool's number, `{x}`, `{y}`,
/// `{z}` its slot (machine coordinates, in the program's units),
/// `{safe_z}` the machine's safe Z for the moves to the rack, `{name}` the
/// tool's name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroChanger {
    pub slots: Vec<RackSlot>,
    /// Machine Z the spindle moves at to and from the rack.
    pub safe_z: f64,
    /// Puts the tool in the spindle away into its slot.
    pub unload: Vec<String>,
    /// Takes the next tool from its slot.
    pub load: Vec<String>,
}

impl MacroChanger {
    pub fn slot(&self, tool: u32) -> Option<&RackSlot> {
        self.slots.iter().find(|s| s.tool == tool)
    }
}

/// How tools are changed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolChanger {
    /// The operator changes the tool when the program stops for it.
    #[default]
    Manual,
    /// The controller's own changer: its tool change command runs the
    /// controller's macro or changer (a DIY changer configured in the
    /// controller).
    Controller,
    /// A DIY changer driven by the program's own moves.
    Macro(MacroChanger),
}

/// A CNC machine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Machine {
    pub name: String,
    /// The post-processor's id (`grbl`, `grblhal`, `reprapfirmware`,
    /// `millenniumos`, `mach3`, `mach4`, `linuxcnc`).
    pub controller: String,
    /// Where the gauge line can go.
    pub travel: Bounds,
    pub spindle: Spindle,
    /// The fastest cutting feed (mm/min).
    pub max_feed: f64,
    /// The rapid rate (mm/min), for time estimates.
    pub rapid_feed: f64,
    #[serde(default)]
    pub zones: Vec<Zone>,
    #[serde(default)]
    pub tool_changer: ToolChanger,
    /// Where the spindle goes at the end of a program (machine
    /// coordinates), if anywhere.
    #[serde(default)]
    pub park: Option<P3>,
}

/// Where the work offset is on the machine: the machine position of the
/// work coordinates' origin (the point a zero-length tool's tip would be
/// at with the work offset's zero), as the operator sets it up.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub work_origin: P3,
}

/// What was found wrong. `message` is for people, in words; the place is
/// in `at` (work coordinates) and `operation` for tools and agents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub message: String,
    /// The operation's index in the program.
    pub operation: usize,
    #[serde(default)]
    pub at: Option<P3>,
}

/// Checks a program against the machine: speeds and feeds within its
/// limits, the gauge line within the travel, and the tool and the spindle
/// nose clear of every zone. Moves are sampled at most a quarter of the
/// tool's radius apart (1 mm at most); a zone thinner than that between
/// two samples can be missed. One finding per operation and kind.
pub fn check(machine: &Machine, program: &Program, placement: &Placement) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, op) in program.operations.iter().enumerate() {
        if op.spindle_rpm > 0.0
            && (op.spindle_rpm < machine.spindle.min_rpm
                || op.spindle_rpm > machine.spindle.max_rpm)
        {
            out.push(Finding {
                message: format!(
                    "{}: the spindle speed is outside what the spindle can do.",
                    op.name
                ),
                operation: i,
                at: None,
            });
        }
        let too_fast = op.toolpath.moves.iter().any(|m| match *m {
            Move::Line { feed, .. } | Move::Arc { feed, .. } => feed > machine.max_feed,
            Move::Rapid { .. } => false,
        });
        if too_fast {
            out.push(Finding {
                message: format!("{}: a feed is faster than the machine can cut.", op.name),
                operation: i,
                at: None,
            });
        }
        out.extend(check_moves(
            machine,
            i,
            &op.name,
            &op.tool,
            &op.toolpath.moves,
            placement,
        ));
    }
    out
}

fn check_moves(
    machine: &Machine,
    i: usize,
    name: &str,
    tool: &Tool,
    moves: &[Move],
    placement: &Placement,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let step = (tool.radius() / 4.0).clamp(0.05, 1.0);
    let mut outside = false;
    let mut hit: Vec<bool> = vec![false; machine.zones.len()];
    let mut at: Option<P3> = None;
    for m in moves {
        let samples = match at {
            None => vec![m.to()],
            Some(from) => {
                let mut prev = from;
                let mut pts = Vec::new();
                for q in move_points(from, m, step / 4.0) {
                    let d = ((q[0] - prev[0]).powi(2)
                        + (q[1] - prev[1]).powi(2)
                        + (q[2] - prev[2]).powi(2))
                    .sqrt();
                    let n = ((d / step).ceil() as usize).max(1);
                    for k in 1..=n {
                        let t = k as f64 / n as f64;
                        pts.push([
                            prev[0] + (q[0] - prev[0]) * t,
                            prev[1] + (q[1] - prev[1]) * t,
                            prev[2] + (q[2] - prev[2]) * t,
                        ]);
                    }
                    prev = q;
                }
                pts
            }
        };
        for p in samples {
            let tip = [
                p[0] + placement.work_origin[0],
                p[1] + placement.work_origin[1],
                p[2] + placement.work_origin[2],
            ];
            let gauge = [tip[0], tip[1], tip[2] + tool.length];
            if !outside && !machine.travel.contains(gauge) {
                outside = true;
                out.push(Finding {
                    message: format!("{}: the tool goes beyond the machine's travel.", name),
                    operation: i,
                    at: Some(p),
                });
            }
            for (z, zone) in machine.zones.iter().enumerate() {
                if hit[z] {
                    continue;
                }
                let tool_hits = cylinder_hits_box(tip, tool.radius(), tool.length, &zone.bounds);
                let nose_hits = cylinder_hits_box(
                    gauge,
                    machine.spindle.nose_diameter / 2.0,
                    machine.spindle.nose_length,
                    &zone.bounds,
                );
                if tool_hits || nose_hits {
                    hit[z] = true;
                    let what = if tool_hits { "the tool" } else { "the spindle" };
                    out.push(Finding {
                        message: format!("{}: {} runs into {}.", name, what, zone.name),
                        operation: i,
                        at: Some(p),
                    });
                }
            }
        }
        at = Some(m.to());
    }
    out
}

/// Whether an upright cylinder from `base` up by `height` with `radius`
/// overlaps the box (touching is not overlapping).
fn cylinder_hits_box(base: P3, radius: f64, height: f64, b: &Bounds) -> bool {
    let (z0, z1) = (base[2], base[2] + height);
    if z1 <= b.min[2] + 1e-9 || z0 >= b.max[2] - 1e-9 {
        return false;
    }
    let cx = base[0].clamp(b.min[0], b.max[0]);
    let cy = base[1].clamp(b.min[1], b.max[1]);
    (base[0] - cx).hypot(base[1] - cy) < radius - 1e-9
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::{Coolant, ProgramOp, SpindleDirection};
    use crate::toolpath::Toolpath;

    fn router() -> Machine {
        Machine {
            name: "Router".into(),
            controller: "grblhal".into(),
            travel: Bounds {
                min: [0.0, 0.0, -200.0],
                max: [800.0, 800.0, 0.0],
            },
            spindle: Spindle {
                min_rpm: 8000.0,
                max_rpm: 24000.0,
                spin_up_seconds: 3.0,
                nose_diameter: 40.0,
                nose_length: 60.0,
            },
            max_feed: 5000.0,
            rapid_feed: 8000.0,
            zones: vec![Zone {
                name: "the front clamp".into(),
                bounds: Bounds {
                    min: [90.0, 40.0, -200.0],
                    max: [130.0, 60.0, -150.0],
                },
            }],
            tool_changer: ToolChanger::Manual,
            park: Some([0.0, 780.0, 0.0]),
        }
    }

    fn program(moves: Vec<Move>, rpm: f64) -> Program {
        let mut p = Program::new("test", 10.0);
        p.operations.push(ProgramOp {
            name: "Contour 1".into(),
            tool: Tool::flat_end_mill(1, 6.0),
            spindle_rpm: rpm,
            spindle: SpindleDirection::Cw,
            coolant: Coolant::Off,
            toolpath: Toolpath {
                moves,
                ..Toolpath::default()
            },
        });
        p
    }

    // The work origin 100 mm in from the corner, the stock's top 160 mm
    // below the gauge line's top: the clamp's top is 10 mm below the
    // work's zero.
    const AT: Placement = Placement {
        work_origin: [100.0, 100.0, -140.0],
    };

    #[test]
    fn a_clear_program_has_no_findings() {
        let p = program(
            vec![
                Move::Rapid {
                    to: [0.0, 0.0, 10.0],
                },
                Move::Line {
                    to: [0.0, 0.0, -5.0],
                    feed: 300.0,
                },
            ],
            18000.0,
        );
        assert!(check(&router(), &p, &AT).is_empty());
    }

    #[test]
    fn a_cut_into_a_clamp_is_found_once_in_words() {
        // Down at 20 mm below zero across the clamp (y -60..-40 in work).
        let p = program(
            vec![
                Move::Rapid {
                    to: [10.0, -30.0, 5.0],
                },
                Move::Line {
                    to: [10.0, -30.0, -20.0],
                    feed: 300.0,
                },
                Move::Line {
                    to: [10.0, -80.0, -20.0],
                    feed: 1000.0,
                },
                Move::Line {
                    to: [15.0, -80.0, -20.0],
                    feed: 1000.0,
                },
            ],
            18000.0,
        );
        let f = check(&router(), &p, &AT);
        assert_eq!(f.len(), 1);
        assert_eq!(
            f[0].message,
            "Contour 1: the tool runs into the front clamp."
        );
        let at = f[0].at.unwrap();
        assert!(at[1] <= -40.0 + 3.0 + 0.3 && at[1] >= -40.0);
    }

    #[test]
    fn the_spindle_nose_is_checked_above_the_tool() {
        // High above the clamp the tool clears it but the 40 mm nose does
        // not reach down to it either; low beside it the nose catches it.
        let mut m = router();
        m.zones[0].bounds = Bounds {
            min: [90.0, 40.0, -200.0],
            max: [130.0, 60.0, -100.0],
        };
        // Tool tip 18 mm beside the clamp's edge: the 3 mm radius tool
        // clears it, the 20 mm radius nose 23 mm above the tip does not.
        let p = program(
            vec![
                Move::Rapid {
                    to: [10.0, -22.0, 5.0],
                },
                Move::Line {
                    to: [10.0, -22.0, -5.0],
                    feed: 300.0,
                },
            ],
            18000.0,
        );
        let f = check(&m, &p, &AT);
        assert_eq!(f.len(), 1);
        assert_eq!(
            f[0].message,
            "Contour 1: the spindle runs into the front clamp."
        );
    }

    #[test]
    fn travel_speed_and_feed_limits_are_found() {
        let p = program(
            vec![
                Move::Rapid {
                    to: [0.0, 0.0, 10.0],
                },
                Move::Line {
                    to: [750.0, 0.0, 10.0],
                    feed: 9000.0,
                },
            ],
            30000.0,
        );
        let f = check(&router(), &p, &AT);
        let messages: Vec<&str> = f.iter().map(|x| x.message.as_str()).collect();
        assert_eq!(
            messages,
            vec![
                "Contour 1: the spindle speed is outside what the spindle can do.",
                "Contour 1: a feed is faster than the machine can cut.",
                "Contour 1: the tool goes beyond the machine's travel.",
            ]
        );
    }

    #[test]
    fn machine_json_round_trips_with_a_macro_changer() {
        let mut m = router();
        m.tool_changer = ToolChanger::Macro(MacroChanger {
            slots: vec![RackSlot {
                tool: 1,
                position: [10.0, 790.0, -80.0],
            }],
            safe_z: -5.0,
            unload: vec!["G53 G0 Z{safe_z}".into()],
            load: vec!["G53 G0 X{x} Y{y}".into()],
        });
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"kind\":\"macro\""));
        let back: Machine = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }
}
