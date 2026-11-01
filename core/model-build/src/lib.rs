// SPDX-License-Identifier: MIT
//! The BUILD tab's planning (mitcad#375; `commands.md`, "BUILD: profile
//! assemblies"), a crate of its own above `mitcad-model` (mitcad#777):
//! placing, editing, connecting, panels and removing (`place.rs`,
//! `connect.rs`), timber members, joinery, spacing sets, frames and
//! fasteners, the junctions where profiles meet (`junctions.rs`) and the
//! cut list (`cut_list.rs`).
//!
//! What the design file keeps of BUILD (the sources, the records on
//! components, the libraries' `build` blocks) stays in the model
//! (`mitcad_model::build`) and is re-exported here, so that this crate is
//! the whole of BUILD for its users. The document's BUILD methods are
//! extension traits of [`Document`](mitcad_model::Document): import the
//! [`prelude`] to call them.

mod connect;
mod cut_list;
mod junctions;
pub(crate) mod parts;
pub(crate) mod place;
pub(crate) mod tilt;
// Timber construction (mitcad#478).
pub(crate) mod joinery;
pub(crate) mod spacing;
pub(crate) mod timber;
pub(crate) mod wood;
// Profiled boards (mitcad#477).
pub(crate) mod profiled;
// Timber frames (mitcad#479).
pub(crate) mod frames;
// Fasteners chosen and swapped (mitcad#497).
pub(crate) mod swap;

// What the design file keeps of BUILD: the sources, the records and the
// libraries' `build` blocks (mitcad#777).
pub use mitcad_model::build::*;
// The vector helpers, part facts and value reading BUILD planning shares
// with the model, under the module name the planning code uses (mitcad#811).
use mitcad_model::internal::build as geometry;

pub use connect::{CONNECT_RULE, ConnectInput, ConnectRule, PanelInput, RemoveResult};
pub use cut_list::{
    BoardColumns, CutList, CutListOptions, CutRow, MachiningItem, StockEntry, TimberColumns,
    TimberTotals,
};
pub use frames::{FrameInput, FrameKind, Opening, OpeningOffset, RoofKind};
pub use joinery::JointInput;
pub use junctions::{Junction, JunctionSide};
pub use place::{
    BindInput, EditAt, EditInput, EndCutInput, EndFrom, EndsInput, LengthInput, MitreCut,
    MitreInput, OwnMachining, PartInput, PlaceAt, PlaceInput, PlaceKind, Placed,
};
pub use spacing::{SpacingInput, SpacingMode};
pub use swap::{SwapInput, Swapped};
pub use timber::{Lie, PointTo};
pub use wood::EndInput;

// The document's BUILD methods (mitcad#777).
pub use connect::{ConnectBuild, SetMembersBuild};
pub use cut_list::CutListBuild;
pub use frames::FramesBuild;
pub use joinery::JoineryBuild;
pub use junctions::{JunctionCutsBuild, JunctionsBuild};
pub use parts::{LibraryPartsBuild, PartsBuild};
pub use place::{EditBuild, PlaceBuild};
pub use spacing::SpacingBuild;
pub use swap::SwapBuild;
pub use wood::WoodBuild;

/// The document's BUILD methods are extension traits (mitcad#777), so that
/// BUILD planning can live in a crate above the model: import the public
/// ones (`use mitcad_model_build::prelude::*`) to call `build_place`,
/// `part_info`, `build_junctions` and the others on a
/// [`Document`](mitcad_model::Document).
pub mod prelude {
    pub use super::connect::ConnectBuild;
    pub(crate) use super::connect::ConnectCrateOps;
    pub use super::connect::SetMembersBuild;
    pub use super::cut_list::CutListBuild;
    pub use super::frames::FramesBuild;
    pub(crate) use super::frames::FramesCrateOps;
    pub use super::joinery::JoineryBuild;
    pub(crate) use super::joinery::JoineryCrateOps;
    pub use super::junctions::JunctionCutsBuild;
    pub(crate) use super::junctions::JunctionCutsCrateOps;
    pub use super::junctions::JunctionsBuild;
    pub use super::parts::LibraryPartsBuild;
    pub use super::parts::PartsBuild;
    pub use super::place::EditBuild;
    pub use super::place::PlaceBuild;
    pub(crate) use super::place::PlaceCrateOps;
    pub use super::spacing::SpacingBuild;
    pub(crate) use super::spacing::SpacingCrateOps;
    pub use super::swap::SwapBuild;
    pub(crate) use super::swap::SwapCrateOps;
    pub(crate) use super::tilt::{BoundNormalCrateOps, RetiltCrateOps};
    pub(crate) use super::timber::{
        BetweenCrateOps, EditPointsCrateOps, FaceSheetCrateOps, PointsCrateOps,
    };
    pub use super::wood::WoodBuild;
    pub(crate) use super::wood::WoodCrateOps;
}

/// Items other Mitcad crates use; not part of this crate's API.
///
/// The JSON api (`mitcad-model-api`) and the BUILD test crates read the
/// plans BUILD works out before it changes the design, and a few of its
/// helpers (mitcad#811). BUILD's vector helpers and part facts are the
/// model's: `mitcad_model::internal::build`.
pub mod internal {
    pub use crate::connect::{ConnectPlan, Hole, PanelBound, PanelPlan, Piece};
    pub use crate::frames::Framed;
    pub use crate::joinery::CutPlan;
    pub use crate::parts::{
        SourceOptionValues, source_option_values, source_state, source_state_with_options,
    };
    pub use crate::place::{
        Closing, EndPlan, JointPlan, LengthPlan, Plan, SidePlan, SourceUse, Spot, Target,
        bounds_of, corner_shift,
    };
    pub use crate::spacing::SetPlan;
    pub use crate::timber::Between;
    pub use crate::wood::{
        Fastening, Member, WoodPlan, WoodSize, shaft_axis_is_aligned, wood_json,
    };
}
