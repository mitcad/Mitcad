// SPDX-License-Identifier: MIT
//! The commands and the query of the direction of gravity (mitcad#1713,
//! Part B; `commands.md`, "Gravity"; the model's side is
//! `mitcad_model::gravity` and `Document::set_gravity`): the design's
//! direction of gravity, which Flex items hang toward (`flex_hang.rs`).

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::analysis::occurrence_path_of;
use super::dispatch::named_enum;
use mitcad_model::features::OccurrencePath;
use mitcad_model::gravity::{Gravity, GravitySource};
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, GeomRef};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(super) enum GravityCommand {
        /// The direction: a vector, or a planar face, a plane or an axis
        /// (`geometry`, picked at `occurrence`), `reverse` the other way.
        "gravity.set" => Set {
            #[serde(default)]
            direction: Option<[f64; 3]>,
            #[serde(default)]
            geometry: Option<Value>,
            #[serde(default)]
            occurrence: Option<String>,
            #[serde(default)]
            reverse: bool,
        },
        "gravity.clear" => Clear {},
    }
}

pub(super) fn run_gravity_command<K: Kernel>(
    doc: &mut Document<K>,
    command: GravityCommand,
) -> Result<Value, ApiError> {
    Ok(match command {
        GravityCommand::Set {
            direction,
            geometry,
            occurrence,
            reverse,
        } => {
            let gravity = match (direction, geometry) {
                (Some(direction), None) => {
                    if occurrence.is_some() || reverse {
                        return Err(ApiError(
                            "occurrence and reverse go with a geometry, not a direction".into(),
                        ));
                    }
                    let direction =
                        mitcad_model::gravity::unit_vector(direction).ok_or_else(|| {
                            ApiError(
                                "the direction of gravity must be a finite vector that is not \
                                 zero"
                                    .into(),
                            )
                        })?;
                    Gravity {
                        direction,
                        source: None,
                    }
                }
                (None, Some(geometry)) => {
                    let geometry: GeomRef = serde_json::from_value(geometry)
                        .map_err(|e| ApiError(format!("geometry: {e}")))?;
                    let occurrence = match occurrence.as_deref() {
                        None | Some("") => OccurrencePath::default(),
                        Some(text) => occurrence_path_of(doc, text)?,
                    };
                    let source = GravitySource {
                        occurrence,
                        geometry,
                        reverse,
                    };
                    Gravity {
                        direction: doc.gravity_direction_of(&source)?,
                        source: Some(source),
                    }
                }
                _ => {
                    return Err(ApiError(
                        "gravity.set takes a direction or a geometry".into(),
                    ));
                }
            };
            doc.set_gravity(Some(gravity))?;
            gravity_json(doc)
        }
        GravityCommand::Clear {} => {
            doc.set_gravity(None)?;
            gravity_json(doc)
        }
    })
}

/// The `gravity` query: the design's direction (and what it was taken
/// from, with the direction that gives at the marker now) and the Flex
/// item that hangs, when one does.
pub(super) fn gravity_json<K: Kernel>(doc: &Document<K>) -> Value {
    let (direction, source, now) = match doc.gravity() {
        None => (Value::Null, Value::Null, Value::Null),
        Some(g) => {
            let now = g
                .source
                .as_ref()
                .and_then(|s| doc.gravity_direction_of(s).ok())
                .map_or(Value::Null, |d| json!(d));
            (
                json!(g.direction),
                serde_json::to_value(&g.source).unwrap_or(Value::Null),
                now,
            )
        }
    };
    json!({
        "direction": direction,
        "source": source,
        "source_direction": now,
        "hanging": doc.flex_hang_item().map_or(Value::Null, |uid| json!(uid)),
    })
}
