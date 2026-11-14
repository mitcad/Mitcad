// SPDX-License-Identifier: MIT
//! grbl 1.1 and grblHAL.
//!
//! grbl has no tool change (M6 is an unsupported command there): a manual
//! change stops the program with M0 and a comment, and the operator sets
//! the new tool's Z zero; with a changer the controller or the sender
//! handles, T and M6 are written for it. grblHAL changes tools itself (M6
//! in its tool change modes, `$341`, or an ATC driver or macro). Both
//! take arcs with I and J relative to the start and a dynamic tool length
//! offset (G43.1). grblHAL has the canned drilling cycles (G81, G83, G73
//! with G98); grbl has none, so its drilling is always written out.
//! Both act on `?`, `~` and `!` anywhere in the stream, so
//! comments and names lose them. Mist coolant (M7) is refused on grbl,
//! whose standard build has no M7.
//!
//! The output of both is run through grbl's and grblHAL's own code in
//! `core/cam/tests/controller_sim.rs` (ctests `core.cam_grbl_sim`,
//! `core.cam_grblhal_sim`).

use super::{CycleStyle, Job, Output, PostProcessor, ToolChange, change_message};
use crate::CamError;
use crate::program::Coolant;
use crate::tool::Tool;

pub struct Grbl;
pub struct GrblHal;

/// grbl's realtime commands: the status report, the cycle start and the
/// feed hold act at once wherever they are in the stream, even inside a
/// comment (the others are control and extended characters, which never
/// reach the output).
const REALTIME: &str = "?~!";

pub(crate) fn work_offset(n: u8) -> String {
    format!("G{}", 53 + u32::from(n))
}

fn header(out: &mut Output, job: &Job) {
    out.line("G90 G94 G17");
    let units = out.units_code();
    out.line(units);
    out.line(work_offset(job.program.work_offset));
}

fn dynamic_length(out: &mut Output, tool: &Tool) {
    out.line(format!("G43.1 Z{}", out.len(tool.length)));
}

impl PostProcessor for Grbl {
    fn id(&self) -> &'static str {
        "grbl"
    }
    fn label(&self) -> &'static str {
        "grbl"
    }
    fn stream_commands(&self) -> &'static str {
        REALTIME
    }
    fn coolant(&self, out: &mut Output, coolant: Coolant) -> Result<(), CamError> {
        out.line(match coolant {
            Coolant::Off => "M9",
            Coolant::Flood => "M8",
            // M7 is a build option grbl 1.1 is released without: the
            // standard build refuses it (error 20).
            Coolant::Mist => {
                return Err(CamError::new(
                    "grbl has no command for mist coolant in its standard build; use flood coolant.",
                ));
            }
            Coolant::Air => {
                return Err(CamError::new("grbl has no command for air blast."));
            }
        });
        Ok(())
    }
    fn header(&self, out: &mut Output, job: &Job) -> Result<(), CamError> {
        header(out, job);
        Ok(())
    }
    fn footer(&self, out: &mut Output, _job: &Job) {
        out.line("M30");
    }
    fn tool_change(&self, out: &mut Output, change: &ToolChange) -> Result<(), CamError> {
        if !change.manual {
            out.line(format!("T{} M6", change.tool.number));
        } else if change.first {
            // The first tool is in the spindle when the program starts and
            // its Z zero is set: no stop.
            out.comment(&format!(
                "Tool {}: {}",
                change.tool.number, change.tool.name
            ));
        } else {
            self.pause(
                out,
                &format!("{}, then set its Z zero", change_message(change.tool)),
            );
        }
        Ok(())
    }
    fn length_offset(&self, out: &mut Output, tool: &Tool) -> Result<(), CamError> {
        dynamic_length(out, tool);
        Ok(())
    }
}

impl PostProcessor for GrblHal {
    fn id(&self) -> &'static str {
        "grblhal"
    }
    fn label(&self) -> &'static str {
        "grblHAL"
    }
    fn canned_cycles(&self) -> CycleStyle {
        CycleStyle::Modal
    }
    fn stream_commands(&self) -> &'static str {
        REALTIME
    }
    fn header(&self, out: &mut Output, job: &Job) -> Result<(), CamError> {
        header(out, job);
        Ok(())
    }
    fn footer(&self, out: &mut Output, _job: &Job) {
        out.line("M30");
    }
    fn tool_change(&self, out: &mut Output, change: &ToolChange) -> Result<(), CamError> {
        // M6 runs grblHAL's own change: the manual modes stop and touch
        // off, an ATC or a tool change macro changes the tool.
        out.line(format!("T{} M6", change.tool.number));
        Ok(())
    }
    fn length_offset(&self, out: &mut Output, tool: &Tool) -> Result<(), CamError> {
        dynamic_length(out, tool);
        Ok(())
    }
}
