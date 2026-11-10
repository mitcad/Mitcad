// SPDX-License-Identifier: MIT
//! Commands and queries of print plates (mitcad#1050,
//! `document/print_plates.rs`, `exchange/plates.rs`,
//! `mitcad_model::print_plates`): plates added, edited, ordered and
//! deleted, bodies and occurrences put on them with their counts, the
//! plates listed and a plate's layout drawn beside the design. See
//! `commands.md`, "Print plates".

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::kernel::Kernel;
use mitcad_model::print_plates::{PlateMove, PlateTarget, path_text};
use mitcad_model::{Document, PlateChange};

/// A body (uid or name) or an occurrence (`O1/O4` or `Arm:1/Pin:2`) on a
/// plate, with its count (left out: the count it has, else 1).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlateItemRef {
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    occurrence: Option<String>,
    #[serde(default)]
    count: Option<u32>,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum PlateCommand {
        "add_print_plate" => AddPrintPlate {
            /// `Plate <n>` when left out.
            #[serde(default)]
            name: Option<String>,
        },
        "edit_print_plate" => EditPrintPlate {
            /// By id or name.
            plate: String,
            #[serde(default)]
            name: Option<String>,
            /// Process values by key; a key's `null` takes it away.
            #[serde(default)]
            values: Option<BTreeMap<String, Value>>,
        },
        "delete_print_plate" => DeletePrintPlate {
            plate: String,
        },
        "move_print_plate" => MovePrintPlate {
            plate: String,
            /// Its place in the order, from 0.
            index: usize,
        },
        "place_on_plate" => PlaceOnPlate {
            plate: String,
            items: Vec<PlateItemRef>,
        },
        "remove_from_plates" => RemoveFromPlates {
            /// The counts are not read.
            items: Vec<PlateItemRef>,
        },
    }
}

/// The `plate_layout` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlateLayoutQuery {
    /// By id or name.
    plate: String,
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait PrintPlatesCrateOps<K: Kernel> {
    fn run_plate_command(&mut self, command: PlateCommand) -> Result<Value, ApiError>;

    /// A plate item's body or occurrence.
    fn plate_target(&self, item: &PlateItemRef) -> Result<PlateTarget, ApiError>;

    /// An item that `place_on_plate` took off another plate, as its result
    /// lists it (mitcad#1595): `{"body" | "occurrence", "name", "count",
    /// "from": {"id", "name", "count"}, "to": {"id", "name"}}`.
    fn plate_move_json(&self, moved: &PlateMove) -> Value;
}

/// A moved item of `place_on_plate`'s result (`moved`) for people
/// (mitcad#1595): `Body1 moved from one to two (count 2)`, with `, was 3`
/// when the count given replaced the one it had.
pub(crate) fn plate_move_text(moved: &Value) -> String {
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    let name = text(&moved["name"]);
    let from = text(&moved["from"]["name"]);
    let to = text(&moved["to"]["name"]);
    let count = moved["count"].as_u64().unwrap_or(1);
    match moved["from"]["count"].as_u64() {
        Some(was) if was != count => {
            format!("{name} moved from {from} to {to} (count {count}, was {was})")
        }
        _ => format!("{name} moved from {from} to {to} (count {count})"),
    }
}

impl<K: Kernel> PrintPlatesCrateOps<K> for Document<K> {
    fn run_plate_command(&mut self, command: PlateCommand) -> Result<Value, ApiError> {
        Ok(match command {
            PlateCommand::AddPrintPlate { name } => {
                let (id, name) = self.add_print_plate(name.as_deref())?;
                json!({"id": id, "name": name})
            }
            PlateCommand::EditPrintPlate {
                plate,
                name,
                values,
            } => {
                self.edit_print_plate(&plate, &PlateChange { name, values })?;
                json!({})
            }
            PlateCommand::DeletePrintPlate { plate } => {
                self.delete_print_plate(&plate)?;
                json!({})
            }
            PlateCommand::MovePrintPlate { plate, index } => {
                self.move_print_plate(&plate, index)?;
                json!({})
            }
            PlateCommand::PlaceOnPlate { plate, items } => {
                let targets = items
                    .iter()
                    .map(|item| Ok((self.plate_target(item)?, item.count)))
                    .collect::<Result<Vec<_>, ApiError>>()?;
                let moves = self.place_on_plate(&plate, &targets)?;
                let moved: Vec<Value> = moves.iter().map(|m| self.plate_move_json(m)).collect();
                let warnings: Vec<String> = moved.iter().map(plate_move_text).collect();
                json!({"moved": moved, "warnings": warnings})
            }
            PlateCommand::RemoveFromPlates { items } => {
                let targets = items
                    .iter()
                    .map(|item| self.plate_target(item))
                    .collect::<Result<Vec<_>, ApiError>>()?;
                self.remove_from_plates(&targets)?;
                json!({})
            }
        })
    }

    fn plate_target(&self, item: &PlateItemRef) -> Result<PlateTarget, ApiError> {
        match (&item.body, &item.occurrence) {
            (Some(body), None) => Ok(PlateTarget::Body(self.body_by_uid_or_name(body)?)),
            (None, Some(path)) => {
                let found = self
                    .assembly()
                    .find_path(path)
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| ApiError(format!("there is no occurrence {path}")))?;
                Ok(PlateTarget::Occurrence(found))
            }
            _ => Err(ApiError(
                "a plate item names a body or an occurrence".to_owned(),
            )),
        }
    }

    fn plate_move_json(&self, moved: &PlateMove) -> Value {
        let mut out = match &moved.target {
            PlateTarget::Body(uid) => json!({"body": uid}),
            PlateTarget::Occurrence(path) => json!({"occurrence": path_text(path)}),
        };
        out["name"] = json!(self.plate_target_name(&moved.target));
        out["count"] = json!(moved.count);
        out["from"] = json!({"id": moved.from_id, "name": moved.from_name,
                             "count": moved.from_count});
        out["to"] = json!({"id": moved.to_id, "name": moved.to_name});
        out
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait PrintPlatesOps<K: Kernel> {
    fn print_plates_query(&self) -> Value;

    fn plate_layout_query(&self, query: &PlateLayoutQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> PrintPlatesOps<K> for Document<K> {
    fn print_plates_query(&self) -> Value {
        self.print_plates_json()
    }

    fn plate_layout_query(&self, query: &PlateLayoutQuery) -> Result<Value, ApiError> {
        let index = self.plate_index(&query.plate)?;
        Ok(self.plate_layout_json(index)?)
    }
}
