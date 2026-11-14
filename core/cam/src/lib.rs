// SPDX-License-Identifier: MIT
//! CAM for MitCad (docs/cam.md): tools and the tool library, the machine
//! model with its collision zones and tool changer, the toolpaths of the
//! 2.5-axis milling operations, and the post-processors that write them
//! as the G-code of a controller.
//!
//! The crate knows no document: the model and the api give it planar
//! outlines (from faces, sketches and the stock) in the work coordinates
//! and keep its operations; it gives back toolpaths, findings and G-code.
//! Lengths are millimetres and feeds millimetres per minute throughout;
//! a post-processor writes inches when asked.

pub mod geom;
pub mod machine;
pub mod ops;
pub mod post;
pub mod program;
pub mod tool;
pub mod toolpath;
// The spindle assembly and its collisions along a program (mitcad#1013,
// phase 2).
pub mod collision;
pub mod spindle;

pub use geom::{Contour, Region, Vertex};
pub use machine::{Machine, Placement, ToolChanger, check};
pub use post::{PostProcessor, PostSettings, Units, dialect, dialects, post};
pub use program::{Coolant, Program, ProgramOp, SpindleDirection};
pub use tool::{Cutting, Tool, ToolLibrary, ToolShape};
pub use toolpath::{CycleKind, DrillCycle, Move, Toolpath};
// The spindle assembly and its collisions (mitcad#1013, phase 2).
pub use collision::{
    Collision, CollisionOptions, CollisionReport, Obstacle, ObstacleKind, TriMesh, check_collisions,
};
pub use post::{MoveLines, Posted, post_with_lines};
pub use spindle::{PartKind, SpindleAssembly, SpindleModel, SpindlePart};
// The stock as it is cut: a height-map simulation (mitcad#1013, phase 3).
pub mod stock;
pub use collision::StockModel;
pub use stock::{Cutter, Grid, HeightMap, Place, RapidCut, Remaining, remaining, simulate};
// The G-code reader and the check of the tool changes' moves (mitcad#1013,
// phase 3).
pub mod gcode;
pub use collision::{ChangeMove, check_tool_changes, check_with_tool_changes};
pub use gcode::{GMove, GcodeError, GcodeReader};
pub use post::ChangeLines;

/// Why an operation or a post run could not be made: `message` says it
/// in words for the person using the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CamError {
    pub message: String,
}

impl CamError {
    pub fn new(message: impl Into<String>) -> Self {
        CamError {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CamError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::{Bounds, Spindle, Zone};
    use crate::ops::{ContourOp, Depths, Direction, Heights, PocketOp, Side};

    /// A plate 80 x 50 with a pocket and a hole, cut out: the operations,
    /// the program, the machine check and every dialect's G-code.
    #[test]
    fn a_plate_goes_from_outlines_to_gcode_for_every_controller() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let cutting = Cutting {
            spindle_rpm: 18000.0,
            feed: 1200.0,
            plunge_feed: 300.0,
        };
        let heights = Heights {
            clearance: 10.0,
            retract: 2.0,
        };
        let pocket = PocketOp {
            region: Region::new(
                Contour::rectangle([10.0, 10.0], [50.0, 40.0]),
                vec![Contour::circle([30.0, 25.0], 4.0)],
            ),
            stepover: None,
            direction: Direction::Climb,
            depths: Depths {
                top: 0.0,
                bottom: -5.0,
                step_down: 2.5,
            },
            heights,
            stock_to_leave: 0.0,
            cutting,
            entry: ops::Entry::Plunge,
            adaptive: None,
        };
        let outline = ContourOp {
            contours: vec![Contour::rectangle([0.0, 0.0], [80.0, 50.0])],
            side: Side::Outside,
            direction: Direction::Climb,
            depths: Depths {
                top: 0.0,
                bottom: -12.0,
                step_down: 4.0,
            },
            heights,
            stock_to_leave: 0.0,
            cutting,
            entry: ops::Entry::Plunge,
            tabs: None,
            lead: None,
        };
        let mut program = Program::new("Plate", 15.0);
        for (name, path) in [
            ("Pocket 1", ops::pocket(&pocket, &tool).unwrap()),
            ("Contour 1", ops::contour(&outline, &tool).unwrap()),
        ] {
            program.operations.push(ProgramOp {
                name: name.into(),
                tool: tool.clone(),
                spindle_rpm: 18000.0,
                spindle: SpindleDirection::Cw,
                coolant: Coolant::Off,
                toolpath: path,
            });
        }
        let machine = Machine {
            name: "Router".into(),
            controller: "grblhal".into(),
            travel: Bounds {
                min: [0.0, 0.0, -150.0],
                max: [800.0, 800.0, 0.0],
            },
            spindle: Spindle {
                min_rpm: 8000.0,
                max_rpm: 24000.0,
                spin_up_seconds: 3.0,
                nose_diameter: 50.0,
                nose_length: 80.0,
            },
            max_feed: 5000.0,
            rapid_feed: 8000.0,
            zones: vec![Zone {
                name: "the left clamp".into(),
                bounds: Bounds {
                    min: [40.0, 120.0, -150.0],
                    max: [80.0, 160.0, -80.0],
                },
            }],
            tool_changer: ToolChanger::Manual,
            park: None,
        };
        // The stock's corner at (100, 100), its top 100 mm down: the clamp
        // is 20 mm off the stock's left side and 20 mm above its top,
        // clear of the cutter running outside the outline (and of the
        // pocket's nose) but not of the 25 mm radius nose above the
        // outline's cut.
        let placement = Placement {
            work_origin: [100.0, 100.0, -100.0],
        };
        let findings = check(&machine, &program, &placement);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(
            findings[0].message,
            "Contour 1: the spindle runs into the left clamp."
        );
        assert_eq!(findings[0].operation, 1);
        for d in dialects() {
            let g = post(
                &program,
                d.as_ref(),
                &PostSettings::default(),
                Some(&machine),
            )
            .unwrap();
            assert!(g.lines().count() > 50, "{}", d.id());
            assert!(
                g.lines()
                    .any(|l| l.starts_with("G3 ") || l.starts_with("G2 ")),
                "{}",
                d.id()
            );
        }
        let total = program
            .operations
            .iter()
            .map(|o| o.toolpath.duration(machine.rapid_feed))
            .sum::<f64>();
        assert!(total > 1.0 && total < 10.0, "{total} minutes");
    }
}
