// SPDX-License-Identifier: MIT
//! Values in commands and the model's command error (moved down from the
//! model's `features` and `document`, mitcad#780, so that the sketch below
//! the model's core resolves its dimension values as the document does):
//! [`ValueInput`], a parameter name, an expression or a number;
//! [`resolve_value`], which turns one into a parameter; [`ModelError`], a
//! rejected command. The model re-exports them at their old paths
//! (`mitcad_model::features::ValueInput`, `mitcad_model::ModelError`).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::expr::{ParamSpec, Unit, is_valid_name, value_to_expression};
use crate::ids::FeatureUid;
use crate::monitor::Cancelled;
use crate::parameters::{ParamId, ParameterError, Parameters, describe, fits_slot, slot_unit};

/// A value in a command: a parameter by name, or a number that becomes a
/// new dimension parameter of the feature (`d4`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValueInput {
    Number(f64),
    Name(String),
}

/// Rejected command; the document is left unchanged.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelError {
    NoSuchFeature(FeatureUid),
    /// A reference or a value the command cannot accept.
    Invalid(String),
    /// Deleting the feature would break the features that use it.
    Dependents {
        feature: String,
        dependents: Vec<String>,
    },
    Parameter(ParameterError),
    /// The computation was cancelled through the monitor (P7).
    Cancelled,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchFeature(uid) => write!(f, "feature {uid} does not exist"),
            Self::Invalid(message) => f.write_str(message),
            Self::Dependents {
                feature,
                dependents,
            } => write!(
                f,
                "{feature} is used by {}; delete them too, or change them first",
                dependents.join(", ")
            ),
            Self::Parameter(error) => error.fmt(f),
            Self::Cancelled => Cancelled.fmt(f),
        }
    }
}

impl std::error::Error for ModelError {}

impl From<ParameterError> for ModelError {
    fn from(error: ParameterError) -> Self {
        Self::Parameter(error)
    }
}

impl From<Cancelled> for ModelError {
    fn from(_: Cancelled) -> Self {
        Self::Cancelled
    }
}

fn invalid(message: impl Into<String>) -> ModelError {
    ModelError::Invalid(message.into())
}

/// Resolves one command value (see the model's `DocState::resolve`): an existing
/// parameter by name, or a number or an expression for the slot's own
/// parameter (`old`, when the feature owns it) or a new dimension
/// parameter. Numbers are millimetres or radians; new parameters show
/// lengths in millimetres and angles in degrees (see [`slot_unit`]).
#[doc(hidden)]
pub fn resolve_value(
    params: &mut Parameters,
    slot: &str,
    value: &ValueInput,
    owner: FeatureUid,
    comment: &str,
    old: Option<&ParamId>,
) -> Result<(ParamId, Option<String>), ModelError> {
    resolve_value_as(params, slot, None, value, owner, comment, old)
}

/// [`resolve_value`] with the slot's unit given (`unit`) instead of read
/// from its name: a derived part's `parameters.<name>` slot takes the
/// unit of its source's parameter (mitcad#801), a text, an angle or a
/// count. None: the slot's own unit ([`slot_unit`]).
#[doc(hidden)]
pub fn resolve_value_as(
    params: &mut Parameters,
    slot: &str,
    unit: Option<Unit>,
    value: &ValueInput,
    owner: FeatureUid,
    comment: &str,
    old: Option<&ParamId>,
) -> Result<(ParamId, Option<String>), ModelError> {
    let unit = match unit.unwrap_or_else(|| slot_unit(slot)) {
        Unit::MM => Unit::of_length(params.context().default_length_unit),
        unit => unit,
    };
    if unit.is_text()
        && let ValueInput::Number(number) = value
    {
        return Err(invalid(format!(
            "{slot}: the value is a text, not the number {number}"
        )));
    }
    let own = old.copied().filter(|id| params.owner(*id) == Some(owner));
    let in_slot = |e: ParameterError| invalid(format!("{slot}: {e}"));
    let expression = match value {
        ValueInput::Number(number) => {
            if !number.is_finite() {
                return Err(invalid(format!(
                    "{slot}: the value must be a finite number, got {number}"
                )));
            }
            if let Some(id) = own {
                params.set_value(id, *number).map_err(in_slot)?;
                return Ok((id, None));
            }
            value_to_expression(*number, unit)
        }
        ValueInput::Name(text) => {
            if let Some((name, rest)) = split_named(text) {
                return resolve_named(params, slot, unit, (name, rest), owner, comment, old);
            }
            if let Some(id) = params.find(text.trim()) {
                let param = params.get(id).expect("found");
                // A design from before mitcad#662 may use a length parameter
                // in a count slot (its load warning says so); the slot keeps
                // it until it is changed.
                if !fits_slot(param.unit(), unit) && old != Some(&id) {
                    return Err(invalid(format!(
                        "{slot}: parameter {} is {}, not {}",
                        param.name(),
                        describe(param.unit()),
                        describe(unit)
                    )));
                }
                return Ok((id, None));
            }
            if is_valid_name(text.trim()) {
                return Err(invalid(format!(
                    "{slot}: parameter {} does not exist",
                    text.trim()
                )));
            }
            if let Some(id) = own {
                params.set_expression(id, text).map_err(in_slot)?;
                return Ok((id, None));
            }
            text.clone()
        }
    };
    let id = params
        .add_dimension(&expression, unit, comment, Some(owner))
        .map_err(in_slot)?;
    Ok((id, Some(params.name(id))))
}

/// Splits `name = expression` (mitcad#231): a single `=` after a word.
/// Expressions compare with `==`, so a single `=` is never one of theirs.
#[doc(hidden)]
pub fn split_named(text: &str) -> Option<(&str, &str)> {
    let at = text.find('=')?;
    if text[at + 1..].starts_with('=') {
        return None;
    }
    let name = text[..at].trim();
    let word = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    word.then_some((name, text[at + 1..].trim()))
}

/// `name = expression` in a slot: the slot's own parameter (or the one it
/// uses, when that has the name) is renamed and takes the expression, or a
/// new dimension parameter of that name is made. A name another parameter
/// has, or an invalid one, is refused.
fn resolve_named(
    params: &mut Parameters,
    slot: &str,
    unit: Unit,
    (name, expression): (&str, &str),
    owner: FeatureUid,
    comment: &str,
    old: Option<&ParamId>,
) -> Result<(ParamId, Option<String>), ModelError> {
    let in_slot = |e: ParameterError| invalid(format!("{slot}: {e}"));
    if expression.is_empty() {
        return Err(invalid(format!("{slot}: “{name} =” needs a value")));
    }
    let target = old
        .copied()
        .filter(|id| params.owner(*id) == Some(owner) || params.name(*id) == name);
    if let Some(id) = target {
        params.rename(id, name).map_err(in_slot)?;
        params.set_expression(id, expression).map_err(in_slot)?;
        return Ok((id, None));
    }
    let spec = ParamSpec::model(name, expression, unit).with_comment(comment);
    let id = params.add(spec, Some(owner)).map_err(in_slot)?;
    Ok((id, Some(params.name(id))))
}
