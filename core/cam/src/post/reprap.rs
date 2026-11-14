// SPDX-License-Identifier: MIT
//! RepRapFirmware (CNC mode) and MillenniumOS, the macro set for milling
//! that runs on RepRapFirmware.
//!
//! Both select a tool with `T` alone, which runs the firmware's tool
//! change macros (`tfree`, `tpre`, `tpost`): a DIY changer or a manual
//! change is written there, and the tool's offsets come with the tool, so
//! no length offset is written. Comments start with a semicolon; dwells
//! take seconds in S. MillenniumOS adds its tool table (M4000, from the
//! tools of the program), spindle commands that wait for the speed
//! (M3.9, M5.9), air blast (M7.1), its park (G27), probing the work zero
//! (G6600 with the work offset's number, a setup option written after
//! homing) and drilling cycles (G81, G83, G73: one whole line per hole,
//! each ending at the retract height). RepRapFirmware itself has no
//! canned cycles, so its drilling is always written out.

use super::grbl::work_offset;
use super::{CommentStyle, CycleStyle, Job, Output, PostProcessor, ToolChange, change_message};
use crate::CamError;
use crate::program::{Coolant, SpindleDirection};
use crate::toolpath::P3;

pub struct RepRapFirmware;
pub struct MillenniumOs;

/// A message in RepRapFirmware's quoted strings (a quote doubled, no line
/// breaks).
fn quoted(text: &str) -> String {
    let t: String = text
        .chars()
        .map(|c| {
            if c.is_ascii() && !c.is_ascii_control() {
                c
            } else {
                '?'
            }
        })
        .collect();
    format!("\"{}\"", t.replace('"', "\"\""))
}

fn pause(out: &mut Output, message: &str) {
    out.line(format!("M291 P{} R\"Tool change\" S2", quoted(message)));
}

fn select(out: &mut Output, change: &ToolChange, manual_pause: bool) {
    if change.manual && manual_pause && !change.first {
        pause(out, &change_message(change.tool));
    }
    out.line(format!("T{}", change.tool.number));
}

impl PostProcessor for RepRapFirmware {
    fn id(&self) -> &'static str {
        "reprapfirmware"
    }
    fn label(&self) -> &'static str {
        "RepRapFirmware"
    }
    fn extension(&self) -> &'static str {
        "gcode"
    }
    fn comment_style(&self) -> CommentStyle {
        CommentStyle::Semicolon
    }
    fn header(&self, out: &mut Output, job: &Job) -> Result<(), CamError> {
        out.line("G90");
        let units = out.units_code();
        out.line(units);
        out.line("G17");
        out.line(work_offset(job.program.work_offset));
        Ok(())
    }
    fn footer(&self, out: &mut Output, _job: &Job) {
        out.line("M0");
    }
    fn tool_change(&self, out: &mut Output, change: &ToolChange) -> Result<(), CamError> {
        select(out, change, true);
        Ok(())
    }
    fn length_offset(&self, _out: &mut Output, _tool: &crate::tool::Tool) -> Result<(), CamError> {
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
        let m = match direction {
            SpindleDirection::Cw => "M3",
            SpindleDirection::Ccw => "M4",
        };
        out.line(format!("{m} S{}", out.rpm(rpm)));
        Ok(())
    }
    fn dwell(&self, out: &mut Output, seconds: f64) {
        out.line(format!("G4 S{}", out.seconds(seconds)));
    }
}

impl PostProcessor for MillenniumOs {
    fn id(&self) -> &'static str {
        "millenniumos"
    }
    fn label(&self) -> &'static str {
        "MillenniumOS"
    }
    fn canned_cycles(&self) -> CycleStyle {
        CycleStyle::PerHole
    }
    fn probes_work_zero(&self) -> bool {
        true
    }
    fn extension(&self) -> &'static str {
        "gcode"
    }
    fn comment_style(&self) -> CommentStyle {
        CommentStyle::Semicolon
    }
    fn header(&self, out: &mut Output, job: &Job) -> Result<(), CamError> {
        let units = out.units_code();
        out.line(format!("G90 {units} G94"));
        for t in job.program.tools() {
            out.line(format!(
                "M4000 P{} R{} S{}",
                t.number,
                out.len(t.radius()),
                quoted(&t.name)
            ));
        }
        // MillenniumOS works on a homed machine.
        out.line("G28");
        if job.settings.probe_work_zero {
            // The operator probes the work zero of the program's offset
            // (G54 is 1).
            out.comment("Probe the work zero");
            out.line(format!("G6600 W{}", job.program.work_offset));
        }
        out.line(work_offset(job.program.work_offset));
        Ok(())
    }
    fn footer(&self, out: &mut Output, _job: &Job) {
        out.line("G27");
        out.line("M0");
    }
    fn tool_change(&self, out: &mut Output, change: &ToolChange) -> Result<(), CamError> {
        // MillenniumOS asks the operator for a manual change and measures
        // the tool itself.
        select(out, change, false);
        Ok(())
    }
    fn length_offset(&self, _out: &mut Output, _tool: &crate::tool::Tool) -> Result<(), CamError> {
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
        if direction == SpindleDirection::Ccw {
            return Err(CamError::new(
                "MillenniumOS has no command for a counter-clockwise spindle.",
            ));
        }
        out.line(format!("M3.9 S{}", out.rpm(rpm)));
        Ok(())
    }
    fn spindle_waits(&self) -> bool {
        true
    }
    fn spindle_off(&self, out: &mut Output) {
        out.line("M5.9");
    }
    fn coolant(&self, out: &mut Output, coolant: Coolant) -> Result<(), CamError> {
        out.line(match coolant {
            Coolant::Off => "M9",
            Coolant::Flood => "M8",
            Coolant::Mist => "M7",
            Coolant::Air => "M7.1",
        });
        Ok(())
    }
    fn dwell(&self, out: &mut Output, seconds: f64) {
        out.line(format!("G4 S{}", out.seconds(seconds)));
    }
    /// The machine parks itself at the end (G27 in the footer).
    fn park(&self, _out: &mut Output, _at: P3) {}
}
