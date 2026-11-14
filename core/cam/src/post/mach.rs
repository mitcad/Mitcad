// SPDX-License-Identifier: MIT
//! Mach3 and Mach4. Their milling G-code is the same for what the
//! operations write today: `T M6` runs the controller's tool change (its
//! M6 macro stops for a manual change or drives a changer; a DIY changer
//! is written there), followed by the tool length offset from the tool
//! table (G43 H). Arcs use I and J relative to the start (G91.1 in the
//! header). They stay two dialects because their macros differ (Mach3's
//! VB macro files, Mach4's Lua scripts) and so will their changer calls.
//! A dwell is in seconds (the owner's decision of 12.11.2026, mitcad#1013):
//! Mach3's "G4 in milliseconds" setting must be off. Both have the canned
//! drilling cycles (G81, G83, G73 with G98, G80).

use super::grbl::work_offset;
use super::{CycleStyle, Job, Output, PostProcessor, ToolChange};
use crate::CamError;

pub struct Mach3;
pub struct Mach4;

fn header(out: &mut Output, job: &Job) {
    out.line("G90 G94 G91.1 G40 G49 G17");
    let units = out.units_code();
    out.line(units);
    out.line(work_offset(job.program.work_offset));
}

fn change(out: &mut Output, change: &ToolChange) {
    out.line(format!("T{} M6", change.tool.number));
}

impl PostProcessor for Mach3 {
    fn id(&self) -> &'static str {
        "mach3"
    }
    fn label(&self) -> &'static str {
        "Mach3"
    }
    fn canned_cycles(&self) -> CycleStyle {
        CycleStyle::Modal
    }
    fn extension(&self) -> &'static str {
        "tap"
    }
    fn tool_length_offset(&self) -> bool {
        true
    }
    fn header(&self, out: &mut Output, job: &Job) -> Result<(), CamError> {
        header(out, job);
        Ok(())
    }
    fn footer(&self, out: &mut Output, _job: &Job) {
        out.line("M30");
    }
    fn tool_change(&self, out: &mut Output, c: &ToolChange) -> Result<(), CamError> {
        change(out, c);
        Ok(())
    }
}

impl PostProcessor for Mach4 {
    fn id(&self) -> &'static str {
        "mach4"
    }
    fn label(&self) -> &'static str {
        "Mach4"
    }
    fn canned_cycles(&self) -> CycleStyle {
        CycleStyle::Modal
    }
    fn extension(&self) -> &'static str {
        "tap"
    }
    fn tool_length_offset(&self) -> bool {
        true
    }
    fn header(&self, out: &mut Output, job: &Job) -> Result<(), CamError> {
        header(out, job);
        Ok(())
    }
    fn footer(&self, out: &mut Output, _job: &Job) {
        out.line("M30");
    }
    fn tool_change(&self, out: &mut Output, c: &ToolChange) -> Result<(), CamError> {
        change(out, c);
        Ok(())
    }
}
