// SPDX-License-Identifier: MIT
//! A machining program: the operations' toolpaths in order with their
//! tools, spindle and coolant, ready for a post-processor.

use serde::{Deserialize, Serialize};

use crate::CamError;
use crate::tool::Tool;
use crate::toolpath::{Move, Toolpath};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpindleDirection {
    /// Clockwise seen from above (M3), the usual one.
    #[default]
    Cw,
    /// Counter-clockwise (M4).
    Ccw,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coolant {
    #[default]
    Off,
    /// Flood coolant (M8).
    Flood,
    /// Mist coolant (M7).
    Mist,
    /// Air blast, where the controller has an output for it.
    Air,
}

/// One operation of a program.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProgramOp {
    /// The operation's name, written as a comment.
    pub name: String,
    pub tool: Tool,
    pub spindle_rpm: f64,
    #[serde(default)]
    pub spindle: SpindleDirection,
    #[serde(default)]
    pub coolant: Coolant,
    pub toolpath: Toolpath,
}

fn g54() -> u8 {
    1
}

/// A program: what one setup of the stock cuts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub name: String,
    /// The work offset the program runs in: 1 to 6 for G54 to G59.
    #[serde(default = "g54")]
    pub work_offset: u8,
    /// Z (work coordinates) clear of everything: tool changes, the start
    /// and the end go here.
    pub safe_z: f64,
    pub operations: Vec<ProgramOp>,
}

impl Program {
    pub fn new(name: &str, safe_z: f64) -> Self {
        Program {
            name: name.to_string(),
            work_offset: 1,
            safe_z,
            operations: Vec::new(),
        }
    }

    /// The tools in the order they are first used, each once.
    pub fn tools(&self) -> Vec<&Tool> {
        let mut out: Vec<&Tool> = Vec::new();
        for op in &self.operations {
            if !out.iter().any(|t| t.number == op.tool.number) {
                out.push(&op.tool);
            }
        }
        out
    }

    pub fn validate(&self) -> Result<(), CamError> {
        if !(1..=6).contains(&self.work_offset) {
            return Err(CamError::new("The work offset must be one of G54 to G59."));
        }
        if !self.safe_z.is_finite() {
            return Err(CamError::new("The safe height must be a number."));
        }
        for op in &self.operations {
            op.tool.validate().map_err(CamError::new)?;
            if !(op.spindle_rpm.is_finite() && op.spindle_rpm >= 0.0) {
                return Err(CamError::new(format!(
                    "The spindle speed of {} must be zero or more.",
                    op.name
                )));
            }
            if !matches!(op.toolpath.moves.first(), None | Some(Move::Rapid { .. })) {
                return Err(CamError::new(format!(
                    "The toolpath of {} must start with a rapid.",
                    op.name
                )));
            }
            for m in &op.toolpath.moves {
                let finite = m.to().iter().all(|v| v.is_finite());
                let feed_ok = match *m {
                    Move::Rapid { .. } => true,
                    Move::Line { feed, .. } | Move::Arc { feed, .. } => {
                        feed.is_finite() && feed > 0.0
                    }
                };
                if !finite || !feed_ok {
                    return Err(CamError::new(format!(
                        "The toolpath of {} has a move that is not valid.",
                        op.name
                    )));
                }
            }
        }
        for t in self.tools() {
            let other = self
                .operations
                .iter()
                .find(|o| o.tool.number == t.number && o.tool != *t)
                .map(|o| o.name.clone());
            if let Some(name) = other {
                return Err(CamError::new(format!(
                    "Tool {} is a different tool in {} than before; give it its own number.",
                    t.number, name
                )));
            }
        }
        Ok(())
    }
}
