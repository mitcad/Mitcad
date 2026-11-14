// SPDX-License-Identifier: MIT
//! LinuxCNC. The program is between `%` lines; the header sets path
//! blending to the arc tolerance's order (G64 P), and `T M6` runs the
//! configured tool change (the manual change dialog, or a DIY changer
//! remapped to M6 in the machine's configuration), followed by the tool
//! length offset from the tool table (G43 H). The end is M2. Drilling may
//! be written as canned cycles (G81, G83, G73 with G98, G80).

use super::grbl::work_offset;
use super::{CycleStyle, Job, Output, PostProcessor, ToolChange};
use crate::CamError;

pub struct LinuxCnc;

impl PostProcessor for LinuxCnc {
    fn id(&self) -> &'static str {
        "linuxcnc"
    }
    fn label(&self) -> &'static str {
        "LinuxCNC"
    }
    fn canned_cycles(&self) -> CycleStyle {
        CycleStyle::Modal
    }
    fn extension(&self) -> &'static str {
        "ngc"
    }
    fn tool_length_offset(&self) -> bool {
        true
    }
    fn start(&self, out: &mut Output) {
        out.raw("%");
    }
    fn header(&self, out: &mut Output, job: &Job) -> Result<(), CamError> {
        out.line("G17 G90 G94 G40 G49 G80");
        let units = out.units_code();
        out.line(units);
        // Blend corners no farther than this from the path.
        out.line(format!("G64 P{}", out.len(job.settings.arc_tolerance)));
        out.line(work_offset(job.program.work_offset));
        Ok(())
    }
    fn footer(&self, out: &mut Output, _job: &Job) {
        out.line("M2");
        out.raw("%");
    }
    fn tool_change(&self, out: &mut Output, change: &ToolChange) -> Result<(), CamError> {
        out.line(format!("T{} M6", change.tool.number));
        Ok(())
    }
}
