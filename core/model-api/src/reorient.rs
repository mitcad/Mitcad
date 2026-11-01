// SPDX-License-Identifier: MIT
//! Reorient Design (mitcad#502, see `commands.md`, "Reorient Design"): the
//! whole design turned by quarter turns about an origin axis.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::Document;
use mitcad_model::datum::OriginDatum;
use mitcad_model::kernel::Kernel;
use mitcad_model::reorient::{Turn, TurnAxis};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum ReorientCommand {
        "reorient_design" => ReorientDesign {
            /// `x`, `y` or `z`, with `degrees`.
            #[serde(default)]
            axis: Option<String>,
            /// 90, 180 or 270 (-90 is 270), counter-clockwise looking against
            /// the axis.
            #[serde(default)]
            degrees: Option<i64>,
            /// Instead of an axis: the origin plane (`xz`, `yz`, `xy`) whose
            /// frame becomes XY's.
            #[serde(default)]
            plane: Option<String>,
        },
    }
}

/// The turn a command asks for.
pub(crate) fn requested_turn(
    axis: Option<&str>,
    degrees: Option<i64>,
    plane: Option<&str>,
) -> Result<(Turn, String), ApiError> {
    match (axis, degrees, plane) {
        (Some(axis), Some(degrees), None) => {
            let parsed = TurnAxis::parse(axis).ok_or_else(|| {
                ApiError(format!(
                    "reorient_design: unknown axis “{axis}” (x, y or z)"
                ))
            })?;
            let quarters = match degrees.rem_euclid(360) {
                90 => 1,
                180 => 2,
                270 => 3,
                _ => {
                    return Err(ApiError(format!(
                        "reorient_design: {degrees} degrees: the design turns by 90, 180 or 270"
                    )));
                }
            };
            Ok((
                Turn::about(parsed, quarters),
                format!(
                    "{} degrees about {}",
                    degrees.rem_euclid(360),
                    parsed.name()
                ),
            ))
        }
        (None, None, Some(plane)) => {
            let origin = OriginDatum::parse(plane);
            let turn = origin.and_then(Turn::plane_onto_xy).ok_or_else(|| {
                ApiError(format!(
                    "reorient_design: unknown origin plane “{plane}” (xy, xz or yz)"
                ))
            })?;
            let name = origin.map_or("", OriginDatum::display_name);
            Ok((turn, format!("{name} becomes XY")))
        }
        _ => Err(ApiError(
            "reorient_design takes \"axis\" with \"degrees\", or \"plane\"".to_owned(),
        )),
    }
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait ReorientCrateOps<K: Kernel> {
    fn run_reorient_command(&mut self, command: ReorientCommand) -> Result<Value, ApiError>;
}

impl<K: Kernel> ReorientCrateOps<K> for Document<K> {
    fn run_reorient_command(&mut self, command: ReorientCommand) -> Result<Value, ApiError> {
        match command {
            ReorientCommand::ReorientDesign {
                axis,
                degrees,
                plane,
            } => {
                let (turn, label) = requested_turn(axis.as_deref(), degrees, plane.as_deref())?;
                let report = self.reorient_design(turn)?;
                Ok(json!({
                    "turn": label,
                    "matrix": turn.matrix(),
                    "features": report.features,
                    "moves": report.moves,
                    "occurrences": report.occurrences,
                    "views": report.views,
                    "analyses": report.analyses,
                    "lights": report.lights,
                }))
            }
        }
    }
}
