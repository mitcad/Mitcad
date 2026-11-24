// SPDX-License-Identifier: MIT
//! Makera's desktop machines: the Carvera, with its automatic tool
//! changer, and the Carvera Air, whose tools are changed by hand in quick
//! change collets (mitcad#2349).
//!
//! Both run Makera's controller firmware (built on Smoothieware), and both
//! write the same G-code but for the tool change. Written from Makera's
//! published G-code and M-code lists and from the output of its published
//! post-processors (docs/cam.md, "Post-processors"):
//!
//! - The header is `G90 G94`, `G17`, the units (`G21`, or `G20`) and the
//!   work offset; the end goes to the machine's clearance position
//!   (`G28`, which is not a homing move on these machines) and ends with
//!   `M30`. The clearance position is the machine's park, so a park
//!   position of the machine is not written.
//! - `M6 T<n>` changes the tool and measures its length on the machine's
//!   tool setter, which sets the tool length offset: no length offset is
//!   written. On the Carvera the changer holds tools 1 to 6; another
//!   number is refused. On the Carvera Air the same `M6 T<n>` takes the
//!   spindle to the change position and waits for the operator, so every
//!   change is announced with a comment. Tool 0 is the machines' probe and
//!   is refused as a cutting tool.
//! - The spindle is `M3 S<rpm>`; the machines are not wired to turn it
//!   counter-clockwise, so `M4` is refused. Air blast is `M7`, off `M9`;
//!   there is no flood or mist coolant, which is refused in words.
//! - Dwells are `G4 P` in seconds; a pause for the operator is `M600`.
//! - Arcs are G2 and G3 with I and J from the start point. The controller
//!   has no canned drilling cycles, so drilling is written as moves.
//! - Comments are in parentheses; the file extension is `.cnc`.
//!
//! The optional 4th axis is not written: its moves need more than an A
//! word (the controller's `G92.4` to bring the axis back to 0 to 360
//! degrees), and MitCad's milling is 2.5-axis.

use super::grbl::work_offset;
use super::{Job, Output, PostProcessor, ToolChange, change_message};
use crate::CamError;
use crate::program::{Coolant, SpindleDirection};
use crate::tool::Tool;
use crate::toolpath::P3;

pub struct Carvera;
pub struct CarveraAir;

/// The tools the Carvera's changer holds.
const CHANGER_SLOTS: u32 = 6;

fn header(out: &mut Output, job: &Job) {
    out.line("G90 G94");
    out.line("G17");
    let units = out.units_code();
    out.line(units);
    out.line(work_offset(job.program.work_offset));
}

fn footer(out: &mut Output) {
    // G28 is the clearance position on these machines, not homing.
    out.line("G28");
    out.line("M30");
}

/// Tool 0 is the probe on both machines.
fn check_not_probe(label: &str, tool: &Tool) -> Result<(), CamError> {
    if tool.number == 0 {
        return Err(CamError::new(format!(
            "{label} uses tool 0 for its probe; give {} another number.",
            tool.name
        )));
    }
    Ok(())
}

fn spindle_on(
    label: &str,
    out: &mut Output,
    rpm: f64,
    direction: SpindleDirection,
) -> Result<(), CamError> {
    if direction == SpindleDirection::Ccw {
        return Err(CamError::new(format!(
            "{label} has no command for a counter-clockwise spindle."
        )));
    }
    out.line(format!("M3 S{}", out.rpm(rpm)));
    Ok(())
}

fn coolant(label: &str, out: &mut Output, coolant: Coolant) -> Result<(), CamError> {
    out.line(match coolant {
        Coolant::Off => "M9",
        Coolant::Air => "M7",
        Coolant::Flood | Coolant::Mist => {
            return Err(CamError::new(format!(
                "{label} has no flood or mist coolant; use air blast."
            )));
        }
    });
    Ok(())
}

fn pause(out: &mut Output, message: &str) {
    out.comment(message);
    out.line("M600");
}

impl PostProcessor for Carvera {
    fn id(&self) -> &'static str {
        "carvera"
    }
    fn label(&self) -> &'static str {
        "Makera Carvera"
    }
    fn extension(&self) -> &'static str {
        "cnc"
    }
    fn header(&self, out: &mut Output, job: &Job) -> Result<(), CamError> {
        header(out, job);
        Ok(())
    }
    fn footer(&self, out: &mut Output, _job: &Job) {
        footer(out);
    }
    fn tool_change(&self, out: &mut Output, change: &ToolChange) -> Result<(), CamError> {
        let tool = change.tool;
        check_not_probe(self.label(), tool)?;
        if tool.number > CHANGER_SLOTS {
            return Err(CamError::new(format!(
                "{}'s tool changer holds tools 1 to {CHANGER_SLOTS}; give {} a number from 1 to {CHANGER_SLOTS}.",
                self.label(),
                tool.name
            )));
        }
        // The changer takes the tool and measures its length.
        out.line(format!("M6 T{}", tool.number));
        Ok(())
    }
    fn length_offset(&self, _out: &mut Output, _tool: &Tool) -> Result<(), CamError> {
        Ok(())
    }
    fn pause(&self, out: &mut Output, message: &str) {
        pause(out, message);
    }
    fn spindle_on(
        &self,
        out: &mut Output,
        rpm: f64,
        direction: SpindleDirection,
    ) -> Result<(), CamError> {
        spindle_on(self.label(), out, rpm, direction)
    }
    fn coolant(&self, out: &mut Output, c: Coolant) -> Result<(), CamError> {
        coolant(self.label(), out, c)
    }
    /// The machine goes to its clearance position at the end (G28 in the
    /// footer).
    fn park(&self, _out: &mut Output, _at: P3) {}
}

impl PostProcessor for CarveraAir {
    fn id(&self) -> &'static str {
        "carvera_air"
    }
    fn label(&self) -> &'static str {
        "Makera Carvera Air"
    }
    fn extension(&self) -> &'static str {
        "cnc"
    }
    fn header(&self, out: &mut Output, job: &Job) -> Result<(), CamError> {
        header(out, job);
        Ok(())
    }
    fn footer(&self, out: &mut Output, _job: &Job) {
        footer(out);
    }
    fn tool_change(&self, out: &mut Output, change: &ToolChange) -> Result<(), CamError> {
        let tool = change.tool;
        check_not_probe(self.label(), tool)?;
        // M6 takes the spindle to the change position and waits for the
        // operator, then measures the new tool's length.
        out.comment(&change_message(tool));
        out.line(format!("M6 T{}", tool.number));
        Ok(())
    }
    fn length_offset(&self, _out: &mut Output, _tool: &Tool) -> Result<(), CamError> {
        Ok(())
    }
    fn pause(&self, out: &mut Output, message: &str) {
        pause(out, message);
    }
    fn spindle_on(
        &self,
        out: &mut Output,
        rpm: f64,
        direction: SpindleDirection,
    ) -> Result<(), CamError> {
        spindle_on(self.label(), out, rpm, direction)
    }
    fn coolant(&self, out: &mut Output, c: Coolant) -> Result<(), CamError> {
        coolant(self.label(), out, c)
    }
    /// The machine goes to its clearance position at the end (G28 in the
    /// footer).
    fn park(&self, _out: &mut Output, _at: P3) {}
}
