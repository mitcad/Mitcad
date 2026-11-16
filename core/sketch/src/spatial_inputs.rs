// SPDX-License-Identifier: MIT
//! The point inputs of a 3D sketch's drawing command as the caller wrote
//! them (mitcad#1912), as [`crate::inputs`] reads a planar sketch's
//! (mitcad#1897): coordinates written as expressions of parameters, kept as
//! driving dimensions so that the point follows the parameters.
//!
//! - A point input is `"p3"` (an existing point) or `[x, y, z]` (a new
//!   point, local coordinates of the sketch's frame). A coordinate is a
//!   number (millimetres) or a text: an expression of the document's
//!   parameters (`"w"`, `"w/2"`, `"h + 3"`), read as a dimension's value is
//!   read (bare numbers in the document's length unit). The text is
//!   evaluated when the command runs and the point is drawn there; after
//!   the tool has drawn it, a signed Offset dimension along the frame's X,
//!   Y or Z from the sketch's origin point holds it there, with the text as
//!   its value: changing a parameter the text uses moves the point. The
//!   origin point is a point attached to the sketch's origin, made in the
//!   same edit when the sketch has none. Number coordinates stay free.
//! - Which point an input became: an existing point is itself; a position
//!   is the point the command says it made there (the `known` points of
//!   [`SpatialDrawnInputs::finish`]), else the one new point of the edit at it. A
//!   place the command makes no point at (an arc's `through`, a
//!   circumscribed polygon's `vertex`) refuses expression coordinates in
//!   words.

use std::cell::RefCell;

use serde_json::Value;

use crate::ConstraintUid;
use crate::ids::EntityUid;
use crate::spatial::{SpatialAxis, SpatialConstraintKind, SpatialDimensionKind, SpatialSource};
use crate::spatial_edit::{SpatialEdit, SpatialPointInput};
use crate::values::ValueInput;
use mitcad_model_base::internal::values::resolve_value;

/// What a malformed point input is told.
const POINT_FORM: &str = "a point is [x, y, z] (a new point) or a point id like \"p3\"";

/// What a coordinate that is neither a number nor an expression is told.
const NUMBERS: &str = "a point's coordinates are numbers (millimetres)";

/// A command's point inputs, read one by one, and what they became once
/// its tool has run ([`SpatialDrawnInputs::finish`]).
#[derive(Debug, Default)]
pub struct SpatialDrawnInputs {
    read: RefCell<Vec<Drawn>>,
}

/// One point input as it was read.
#[derive(Debug, Clone)]
struct Drawn {
    field: String,
    index: Option<usize>,
    input: SpatialPointInput,
    /// The coordinates written as text, x, y and z.
    expressions: [Option<String>; 3],
    /// A place only: the command makes no point there.
    place: bool,
}

impl Drawn {
    fn name(&self) -> String {
        match self.index {
            Some(i) => format!("{}[{i}]", self.field),
            None => self.field.clone(),
        }
    }
}

/// What the point inputs of a 3D sketch's command became.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialInputPoints {
    /// Each input in the order it was read, with the point it became.
    pub points: Vec<SpatialInputPoint>,
    /// The dimensions that hold the coordinates written as expressions.
    pub coordinates: Vec<SpatialCoordinateDimension>,
}

/// A point input and the point it became: None where the command made no
/// point of its own there.
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialInputPoint {
    pub field: String,
    /// Its place in a list of point inputs (a spline's `points`).
    pub index: Option<usize>,
    pub point: Option<EntityUid>,
}

/// A coordinate written as an expression and the dimension that holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialCoordinateDimension {
    pub point: EntityUid,
    /// `"x"`, `"y"` or `"z"`.
    pub axis: &'static str,
    pub dimension: ConstraintUid,
    /// The text as written.
    pub expression: String,
    /// The parameter the dimension uses: the one named, or its own.
    pub parameter: String,
}

/// The refusal of expression coordinates where the command makes no point
/// of its own: the words of the planar sketch's (mitcad#1897).
fn no_point_there(name: &str) -> String {
    format!(
        "{name}: the command makes no point of its own there, so its coordinates cannot \
         follow parameters: give numbers, and dimension a point afterwards"
    )
}

impl SpatialDrawnInputs {
    /// Reads the point input `field`: `"p<n>"`, or `[x, y, z]` whose
    /// coordinates are numbers or expressions, evaluated with the
    /// document's parameters as they are now.
    pub fn read(
        &self,
        edit: &SpatialEdit<'_>,
        field: &str,
        value: &Value,
    ) -> Result<SpatialPointInput, String> {
        self.read_as(edit, field, None, value, false)
    }

    /// [`SpatialDrawnInputs::read`] of the input at `index` of a list field.
    pub fn read_at(
        &self,
        edit: &SpatialEdit<'_>,
        field: &str,
        index: usize,
        value: &Value,
    ) -> Result<SpatialPointInput, String> {
        self.read_as(edit, field, Some(index), value, false)
    }

    /// [`SpatialDrawnInputs::read`] of a point input that is a place only
    /// in this use (a circumscribed polygon's `vertex`): an existing point
    /// or numbers; expression coordinates are refused in words.
    pub fn read_place(
        &self,
        edit: &SpatialEdit<'_>,
        field: &str,
        value: &Value,
    ) -> Result<SpatialPointInput, String> {
        self.read_as(edit, field, None, value, true)
    }

    fn read_as(
        &self,
        edit: &SpatialEdit<'_>,
        field: &str,
        index: Option<usize>,
        value: &Value,
        place: bool,
    ) -> Result<SpatialPointInput, String> {
        let mut drawn = Drawn {
            field: field.to_owned(),
            index,
            input: SpatialPointInput::At([0.0; 3]),
            expressions: [None, None, None],
            place,
        };
        drawn.input = match value {
            Value::String(text) => SpatialPointInput::Existing(
                crate::point_serde::parse(text).map_err(|e| e.to_string())?,
            ),
            Value::Array(xyz) if xyz.len() == 3 => {
                let mut at = [0.0; 3];
                for (k, axis) in ["x", "y", "z"].into_iter().enumerate() {
                    match &xyz[k] {
                        Value::Number(number) => {
                            at[k] = number.as_f64().ok_or(NUMBERS)?;
                        }
                        Value::String(text) if !text.trim().is_empty() => {
                            if place {
                                return Err(no_point_there(&drawn.name()));
                            }
                            let slot = format!("{}.{axis}", drawn.name());
                            at[k] = length_of(edit, &slot, text.trim())?;
                            drawn.expressions[k] = Some(text.trim().to_owned());
                        }
                        _ => return Err(NUMBERS.to_owned()),
                    }
                }
                SpatialPointInput::At(at)
            }
            Value::Array(xyz) => {
                return Err(format!(
                    "a 3D sketch's point has three coordinates [x, y, z], not {}",
                    xyz.len()
                ));
            }
            _ => return Err(POINT_FORM.to_owned()),
        };
        let input = drawn.input;
        self.read.borrow_mut().push(drawn);
        Ok(input)
    }

    /// After the command's tool has run: the point each input became
    /// (`known`: the points the command says it made at its inputs, by
    /// field and index, where the tool may have moved one, as a line's end
    /// put on its axis), and the coordinates written as expressions held by
    /// signed Offset dimensions from the sketch's origin point. Refused
    /// (the whole edit with it) when such an input made no point of its
    /// own.
    pub fn finish(
        &self,
        edit: &mut SpatialEdit<'_>,
        known: &[(&str, Option<usize>, EntityUid)],
    ) -> Result<SpatialInputPoints, String> {
        let read = self.read.take();
        let mut made = SpatialInputPoints::default();
        let mut held: Vec<(EntityUid, [Option<String>; 3])> = Vec::new();
        for drawn in read {
            let point = match drawn.input {
                SpatialPointInput::Existing(id) => Some(id),
                SpatialPointInput::At(_) if drawn.place => None,
                SpatialPointInput::At(at) => known
                    .iter()
                    .find(|(field, index, _)| *field == drawn.field && *index == drawn.index)
                    .map(|(_, _, id)| *id)
                    .or_else(|| match new_points_at(edit, at).as_slice() {
                        [one] => Some(*one),
                        _ => None,
                    }),
            };
            if drawn.expressions.iter().any(Option::is_some) {
                let Some(p) = point else {
                    return Err(no_point_there(&drawn.name()));
                };
                held.push((p, drawn.expressions));
            }
            made.points.push(SpatialInputPoint {
                field: drawn.field,
                index: drawn.index,
                point,
            });
        }
        if held.is_empty() {
            return Ok(made);
        }
        let origin = origin_point(edit)?;
        for (point, expressions) in held {
            for (k, expression) in expressions.into_iter().enumerate() {
                let (Some(expression), Some(axis)) = (expression, SpatialAxis::of_index(k)) else {
                    continue;
                };
                let kind = SpatialDimensionKind::Offset {
                    a: origin,
                    b: point,
                    axis,
                };
                let value = ValueInput::Name(expression.clone());
                let dimension = edit.add_dimension(kind, Some(&value), false)?;
                let parameter = edit
                    .def
                    .dimension(dimension)
                    .and_then(|d| d.value)
                    .map(|p| edit.params().name(p))
                    .unwrap_or_default();
                made.coordinates.push(SpatialCoordinateDimension {
                    point,
                    axis: ["x", "y", "z"][k],
                    dimension,
                    expression,
                    parameter,
                });
            }
        }
        Ok(made)
    }
}

/// A place `[x, y, z]` of numbers (mitcad#1912): where a command makes no
/// point of its own, a coordinate written as an expression is refused in
/// words, as the drawing tools refuse it there.
pub fn place_only(field: &str, value: &Value) -> Result<[f64; 3], String> {
    match value {
        Value::Array(xyz) if xyz.len() == 3 => {
            let mut at = [0.0; 3];
            for (k, item) in xyz.iter().enumerate() {
                match item {
                    Value::Number(number) => at[k] = number.as_f64().ok_or(NUMBERS)?,
                    Value::String(_) => return Err(no_point_there(field)),
                    _ => return Err(format!("\"{field}\" is a place [x, y, z] of numbers")),
                }
            }
            Ok(at)
        }
        _ => Err(format!("\"{field}\" is a place [x, y, z] of numbers")),
    }
}

/// The sketch's origin point: a point attached to the sketch's origin,
/// made (with its attachment) when the sketch has none.
fn origin_point(edit: &mut SpatialEdit<'_>) -> Result<EntityUid, String> {
    let existing = edit.def.constraints.iter().find_map(|c| match &c.kind {
        SpatialConstraintKind::Attach {
            point,
            source: SpatialSource::Origin,
            projected: false,
        } => Some(*point),
        _ => None,
    });
    if let Some(point) = existing {
        return Ok(point);
    }
    let point = edit.add_point([0.0; 3])?;
    edit.attach(point, SpatialSource::Origin)?;
    Ok(point)
}

/// The new points of the edit at `at` (within a nanometre or so).
fn new_points_at(edit: &SpatialEdit<'_>, at: [f64; 3]) -> Vec<EntityUid> {
    let size = at.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    let tolerance = 1e-9 * (1.0 + size);
    let mut found: Vec<EntityUid> = Vec::new();
    for id in &edit.report.points {
        let here = edit
            .def
            .position(*id)
            .is_some_and(|p| (0..3).all(|k| (p[k] - at[k]).abs() <= tolerance));
        if here && !found.contains(id) {
            found.push(*id);
        }
    }
    found
}

/// The value in millimetres of `text`, an expression of the document's
/// parameters read as a length, as a dimension's value would read it;
/// nothing of the document changes. `slot` names the input in a refusal
/// (`end.x: parameter w does not exist`).
fn length_of(edit: &SpatialEdit<'_>, slot: &str, text: &str) -> Result<f64, String> {
    let mut scratch = edit.params().clone();
    let value = ValueInput::Name(text.to_owned());
    let (id, _) = resolve_value(&mut scratch, slot, &value, edit.owner(), slot, None)
        .map_err(|e| e.to_string())?;
    match scratch.value(id) {
        Some(length) if length.is_finite() => Ok(length),
        _ => Err(format!("{slot}: “{text}” is not a length")),
    }
}
