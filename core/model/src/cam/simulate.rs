// SPDX-License-Identifier: MIT
//! What the `cam_simulate` query asks (mitcad#1013, phase 3;
//! `document/cam/simulate.rs`): a setup's program cut out of its stock on
//! a height map (`mitcad_cam::stock`), up to a move or to the end, with
//! snapshots along the way, and the material left over the setup's bodies.

use serde::{Deserialize, Serialize};

/// A move of a setup's program: the operation's id and the move's index in
/// its toolpath (a later index than its last move is its last move).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulatePlace {
    pub operation: u32,
    #[serde(rename = "move")]
    pub move_index: usize,
}

/// What the simulation gives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulateResult {
    /// The height grid (compressed), its snapshots, the part's heights and
    /// the summary of the material left.
    #[default]
    Heights,
    /// The summary of the material left only.
    Remaining,
}

/// The `cam_simulate` query's options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SimulateOptions {
    /// Up to and with this move; the whole program when left out.
    #[serde(default)]
    pub until: Option<SimulatePlace>,
    /// Snapshots of the stock after these moves (in the program's order),
    /// each as its changes from the one before.
    #[serde(default)]
    pub frames: Vec<SimulatePlace>,
    /// The grid's step (mm); an eighth of the smallest tool's diameter when
    /// left out.
    #[serde(default)]
    pub resolution: Option<f64>,
    /// The most nodes of the grid (`mitcad_cam::stock::MAX_NODES` when left
    /// out).
    #[serde(default)]
    pub max_nodes: Option<usize>,
    /// Thicknesses up to this count as none in the material left (mm;
    /// [`REMAINING_TOLERANCE`] when left out).
    #[serde(default)]
    pub tolerance: Option<f64>,
    #[serde(default)]
    pub result: SimulateResult,
}

/// The material left up to this thick counts as none (mm).
pub const REMAINING_TOLERANCE: f64 = 0.01;

/// The most snapshots one query gives.
pub const MAX_FRAMES: usize = 1000;
