// SPDX-License-Identifier: MIT
//! Commands and queries of filament changes at print heights
//! (mitcad#1596, `document/filament_changes.rs`,
//! `mitcad_model::print_changes`): the schedule of the design or of a
//! print plate set as one undo step, and listed with each event's layer.
//! See `commands.md`, "Filament changes at print heights".

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::Document;
use mitcad_model::kernel::Kernel;
use mitcad_model::print_changes::FilamentChange;

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(super) enum FilamentChangeCommand {
        "set_filament_changes" => SetFilamentChanges {
            /// A print plate by id or name; the design's own schedule when
            /// left out.
            #[serde(default)]
            plate: Option<String>,
            /// The whole schedule: `{height | at (and body), kind,
            /// material, appearance, message}` per event (mitcad#1643: `at`
            /// a construction plane, an origin plane or a planar face);
            /// empty takes it away.
            changes: Vec<FilamentChange>,
        },
    }
}

/// The `filament_changes` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FilamentChangesQuery {
    /// A print plate by id or name; the design's own schedule when left
    /// out.
    #[serde(default)]
    plate: Option<String>,
    /// An event not in the schedule (mitcad#1800: a plane or face picked
    /// in Filament Change): the result's `trial` says where it would be,
    /// `{height, place, body, warning}`.
    #[serde(default)]
    trial: Option<FilamentChange>,
}

/// The `filament_change_preview` query (mitcad#1643): the print colours
/// and the changes' rings that Send to Slicer draws over the bodies.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FilamentChangePreviewQuery {
    /// Only these bodies (uids); every shown printed body when empty.
    #[serde(default)]
    bodies: Vec<mitcad_model::BodyUid>,
}

/// Runs the command: `{"changes"}`, the schedule as kept (sorted).
pub(super) fn run_filament_change_command<K: Kernel>(
    doc: &mut Document<K>,
    command: FilamentChangeCommand,
) -> Result<Value, ApiError> {
    let FilamentChangeCommand::SetFilamentChanges { plate, changes } = command;
    doc.set_filament_changes(plate.as_deref(), &changes)?;
    let id = match plate.as_deref() {
        Some(plate) => {
            let i = doc.plate_index(plate)?;
            Some(doc.print_plates()[i].id.clone())
        }
        None => None,
    };
    Ok(json!({"changes": doc.filament_changes(id.as_deref())}))
}

/// The `filament_changes` query.
pub(super) fn query<K: Kernel>(
    doc: &Document<K>,
    query: &FilamentChangesQuery,
) -> Result<Value, ApiError> {
    let mut listed = doc.filament_changes_json(query.plate.as_deref())?;
    if let Some(trial) = &query.trial {
        listed["trial"] = doc.filament_change_trial_json(query.plate.as_deref(), trial)?;
    }
    Ok(listed)
}

/// The `filament_change_preview` query.
pub(super) fn preview<K: Kernel>(
    doc: &Document<K>,
    query: &FilamentChangePreviewQuery,
) -> Result<Value, ApiError> {
    Ok(doc.filament_change_preview_json(&query.bodies))
}
