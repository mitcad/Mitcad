// SPDX-License-Identifier: MIT
//! Estlcam's CNC controller (mitcad#2625): the control program on the PC
//! that drives a machine through Estlcam's own controller board firmware
//! and runs G-code files from other CAM programs.
//!
//! Written from Estlcam's published requirements for CNC programs from
//! other CAM programs (docs/cam.md, "Post-processors"). Its interpreter is
//! basic and skips whatever it does not know without a message, so the
//! dialect writes only what it reads and refuses in words what it cannot
//! write:
//!
//! - Moves are G0, G1, G2 and G3, one G code per line, the motion on every
//!   move line; arcs only in the XY plane with I and J from the start
//!   point, helices allowed, no whole turns: a whole turn is written as two
//!   half turns ([`PostProcessor::full_circles`]). No canned cycles, so
//!   drilling is written as moves.
//! - Coordinates are absolute in the one work zero set in the controller:
//!   no work offsets (a setup's other work offset is refused), no machine
//!   coordinates (a machine's park position is not written, which a
//!   comment says; the program ends at the safe height).
//! - The units are the length and feed units chosen in Estlcam's settings;
//!   `G20` and `G21` are skipped by the controller. The program's units are
//!   written as `G21` or `G20` for simulators and a notice, written even
//!   when comments are off, says which units Estlcam must be set to.
//! - The M codes it reads: `M00` and `M01` pause, `M03` spindle on (there
//!   is no `M04`: a counter-clockwise spindle is refused), `M05` off, `M06`
//!   tool change, `M08` and `M09` its first output (used for flood
//!   coolant), `M10` and `M11` its second. One M code per line, written with
//!   two digits as in Estlcam's list. Mist coolant and air blast are
//!   refused: the first output is the coolant's.
//! - `M06` is Estlcam's whole tool change: it goes to the change position
//!   set in the controller, waits for the operator and measures the new
//!   tool when a tool length sensor is set up, so no length offset is
//!   written. The tool's number and name are a comment before it; the
//!   first tool of a manual change is in the spindle when the program
//!   starts and is only named.
//! - There is no dwell (`G4` is skipped): the spindle's start delay is a
//!   setting of the controller, so no spin-up dwell is written.
//! - Comments are in parentheses; there is no program end command (`M30`
//!   is skipped), so the program ends with its last move. Estlcam opens
//!   `.nc` files.

use super::{Job, Output, PostProcessor, ToolChange, change_message};
use crate::CamError;
use crate::program::{Coolant, SpindleDirection};
use crate::tool::Tool;
use crate::toolpath::P3;

pub struct Estlcam;

impl PostProcessor for Estlcam {
    fn id(&self) -> &'static str {
        "estlcam"
    }
    fn label(&self) -> &'static str {
        "Estlcam"
    }
    fn full_circles(&self) -> bool {
        false
    }
    fn header(&self, out: &mut Output, job: &Job) -> Result<(), CamError> {
        if job.program.work_offset != 1 {
            return Err(CamError::new(format!(
                "{} has one work zero, set in its controller; choose G54 as the setup's work offset.",
                self.label()
            )));
        }
        // Estlcam reads the numbers in the units of its own settings.
        let units = out.units_code();
        if units == "G20" {
            out.notice("Units: inches, feeds in inches per minute.");
            out.notice("Set Estlcam to these units before running it.");
        } else {
            out.notice("Units: millimeters, feeds in mm per minute.");
            out.notice("Set Estlcam to these units before running it.");
        }
        out.line(units);
        Ok(())
    }
    /// No end command: Estlcam skips `M30` and `M2`; the spindle and the
    /// coolant are off and the tool is at the safe height by then.
    fn footer(&self, _out: &mut Output, _job: &Job) {}
    fn tool_change(&self, out: &mut Output, change: &ToolChange) -> Result<(), CamError> {
        let tool = change.tool;
        if change.manual && change.first {
            // The first tool is in the spindle when the program starts and
            // its Z zero is set: no stop.
            out.comment(&format!("Tool {}: {}", tool.number, tool.name));
        } else {
            // M06 goes to the change position, waits for the operator and
            // measures the tool when a length sensor is set up.
            out.comment(&change_message(tool));
            out.line("M06");
        }
        Ok(())
    }
    /// `M06` measures the new tool itself; there is no length offset
    /// command.
    fn length_offset(&self, _out: &mut Output, _tool: &Tool) -> Result<(), CamError> {
        Ok(())
    }
    fn pause(&self, out: &mut Output, message: &str) {
        out.comment(message);
        out.line("M00");
    }
    fn spindle_on(
        &self,
        out: &mut Output,
        rpm: f64,
        direction: SpindleDirection,
    ) -> Result<(), CamError> {
        if direction == SpindleDirection::Ccw {
            return Err(CamError::new(format!(
                "{} has no command for a counter-clockwise spindle.",
                self.label()
            )));
        }
        out.line(format!("M03 S{}", out.rpm(rpm)));
        Ok(())
    }
    /// The spindle's start delay is set in Estlcam's controller settings;
    /// a dwell in the program would be skipped.
    fn spindle_waits(&self) -> bool {
        true
    }
    fn spindle_off(&self, out: &mut Output) {
        out.line("M05");
    }
    fn coolant(&self, out: &mut Output, coolant: Coolant) -> Result<(), CamError> {
        out.line(match coolant {
            Coolant::Off => "M09",
            Coolant::Flood => "M08",
            Coolant::Mist | Coolant::Air => {
                return Err(CamError::new(format!(
                    "{} switches only one output for coolant; use flood coolant.",
                    self.label()
                )));
            }
        });
        Ok(())
    }
    /// Estlcam has no dwell command.
    fn dwell(&self, _out: &mut Output, _seconds: f64) {}
    /// Estlcam has no moves in machine coordinates: the program ends at
    /// the safe height and says so.
    fn park(&self, out: &mut Output, _at: P3) {
        out.comment(
            "The machine's park position is not written: Estlcam has no machine coordinates.",
        );
    }
}
