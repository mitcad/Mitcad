// SPDX-License-Identifier: MIT
//! The point inputs of a drawing command as the caller wrote them
//! (mitcad#1897): which point each input became, and coordinates written
//! as expressions of parameters, kept as driving dimensions.
//!
//! - A point input is `"p3"` (an existing point) or `[x, y]` (a new point).
//!   A coordinate is a number (millimetres) or a text: an expression of the
//!   document's parameters (`"w"`, `"w/2"`, `"h + 3"`, `"20"`), read as a
//!   dimension's value is read (bare numbers in the document's length
//!   unit). The text is evaluated when the command runs and the point is
//!   drawn there; after the tool has drawn it, a signed
//!   `horizontal_distance` (x) or `vertical_distance` (y) from the sketch's
//!   origin point (made when the sketch has none, `axes.rs`) holds it
//!   there, with the text as its value: changing a parameter the text uses
//!   moves the point. Number coordinates stay free, as before.
//! - Which point an input became: an existing point is itself; a position
//!   is the one new point of the edit there. Some inputs make none (an
//!   arc's `through`, a circumscribed polygon's `vertex`): they have no
//!   point, and refuse expression coordinates.
//! - An arc's `start` and `end` are the points at the inputs as written,
//!   also where the arc is stored the other way round: arcs run
//!   counter-clockwise, so a clockwise arc is stored from its `end` to its
//!   `start`.

use std::cell::RefCell;

use serde_json::Value;

use super::edit::{PointInput, SketchEdit};
use super::{ConstraintUid, DimensionKind, EntityKind, Ref};
use crate::ids::EntityUid;
use crate::values::ValueInput;
use mitcad_model_base::internal::values::resolve_value;

/// What a malformed point input is told (the text of `modify::point_input`).
const POINT_FORM: &str = "a point is [x, y] or \"p<n>\"";

/// A command's point inputs, read one by one while its fields are read,
/// and what they became once its tool has run ([`DrawnInputs::finish`]).
#[derive(Debug, Default)]
pub struct DrawnInputs {
    read: RefCell<Vec<Drawn>>,
}

/// One point input as it was read.
#[derive(Debug, Clone)]
struct Drawn {
    field: String,
    index: Option<usize>,
    input: PointInput,
    /// The coordinates written as text, x and y.
    expressions: [Option<String>; 2],
}

impl Drawn {
    fn name(&self) -> String {
        match self.index {
            Some(i) => format!("{}[{i}]", self.field),
            None => self.field.clone(),
        }
    }
}

/// What the point inputs of a command became (mitcad#1897).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputPoints {
    /// Each input in the order it was read, with the point it became.
    pub points: Vec<InputPoint>,
    /// The dimensions that hold the coordinates written as expressions.
    pub coordinates: Vec<CoordinateDimension>,
}

/// A point input and the point it became: None where the command made no
/// point of its own there.
#[derive(Debug, Clone, PartialEq)]
pub struct InputPoint {
    pub field: String,
    /// Its place in a list of point inputs (a spline's `points`).
    pub index: Option<usize>,
    pub point: Option<EntityUid>,
}

/// A coordinate written as an expression and the dimension that holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinateDimension {
    pub point: EntityUid,
    /// `"x"` or `"y"`.
    pub axis: &'static str,
    pub dimension: ConstraintUid,
    /// The text as written.
    pub expression: String,
    /// The parameter the dimension uses: the one named, or its own.
    pub parameter: String,
}

impl DrawnInputs {
    /// Reads the point input `field`: `"p<n>"`, or `[x, y]` whose
    /// coordinates are numbers or expressions, evaluated with the
    /// document's parameters as they are now.
    pub fn read(
        &self,
        edit: &SketchEdit<'_>,
        field: &str,
        value: &Value,
    ) -> Result<PointInput, String> {
        self.read_at(edit, field, None, value)
    }

    /// [`DrawnInputs::read`] of the input at `index` of a list field.
    pub fn read_at(
        &self,
        edit: &SketchEdit<'_>,
        field: &str,
        index: Option<usize>,
        value: &Value,
    ) -> Result<PointInput, String> {
        let mut drawn = Drawn {
            field: field.to_owned(),
            index,
            input: PointInput::At([0.0, 0.0]),
            expressions: [None, None],
        };
        drawn.input = match value {
            Value::String(text) => {
                PointInput::Existing(super::point_serde::parse(text).map_err(|e| e.to_string())?)
            }
            Value::Array(xy) if xy.len() == 2 => {
                let mut at = [0.0; 2];
                for (k, axis) in ["x", "y"].into_iter().enumerate() {
                    match &xy[k] {
                        Value::Number(number) => {
                            at[k] = number.as_f64().ok_or(POINT_FORM)?;
                        }
                        Value::String(text) if !text.trim().is_empty() => {
                            let slot = format!("{}.{axis}", drawn.name());
                            at[k] = edit.length_of(&slot, text.trim())?;
                            drawn.expressions[k] = Some(text.trim().to_owned());
                        }
                        _ => return Err(POINT_FORM.to_owned()),
                    }
                }
                PointInput::At(at)
            }
            _ => return Err(POINT_FORM.to_owned()),
        };
        let input = drawn.input;
        self.read.borrow_mut().push(drawn);
        Ok(input)
    }

    /// After the command's tool has run: the point each input became, and
    /// the coordinates written as expressions held by signed dimensions
    /// from the sketch's origin point. Refused (the whole edit with it)
    /// when such an input made no point of its own, and when the
    /// dimensions would over-constrain the sketch (a fixed point, a
    /// constraint that already holds the coordinate).
    pub fn finish(&self, edit: &mut SketchEdit<'_>) -> Result<InputPoints, String> {
        let read = self.read.take();
        let mut made = InputPoints::default();
        let mut held: Vec<(EntityUid, [Option<String>; 2])> = Vec::new();
        for drawn in read {
            let point = match drawn.input {
                PointInput::Existing(id) => Some(id),
                PointInput::At(at) => match new_points_at(edit, at).as_slice() {
                    [one] => Some(*one),
                    _ => None,
                },
            };
            if drawn.expressions.iter().any(Option::is_some) {
                let Some(p) = point else {
                    return Err(format!(
                        "{}: the command makes no point of its own there, so its coordinates \
                         cannot follow parameters: give numbers, and dimension a point \
                         afterwards",
                        drawn.name()
                    ));
                };
                held.push((p, drawn.expressions));
            }
            made.points.push(InputPoint {
                field: drawn.field,
                index: drawn.index,
                point,
            });
        }
        if held.is_empty() {
            return Ok(made);
        }
        // What the dimensions add is compared with the sketch as the tool
        // left it.
        let before = edit.redundancy().ok();
        let origin = edit.origin();
        for (point, expressions) in held {
            for (k, expression) in expressions.into_iter().enumerate() {
                let Some(expression) = expression else {
                    continue;
                };
                let (axis, kind) = if k == 0 {
                    (
                        "x",
                        DimensionKind::HorizontalDistance {
                            a: origin,
                            b: point,
                            signed: true,
                        },
                    )
                } else {
                    (
                        "y",
                        DimensionKind::VerticalDistance {
                            a: origin,
                            b: point,
                            signed: true,
                        },
                    )
                };
                let comment = format!("{} {axis}", Ref::Point(point));
                let value = ValueInput::Name(expression.clone());
                let dimension =
                    edit.add_dimension(kind, Some(&value), false, Some(comment.as_str()))?;
                let parameter = edit
                    .def
                    .dimensions
                    .iter()
                    .find(|d| d.id == dimension)
                    .and_then(|d| d.value)
                    .map(|p| edit.params.name(p))
                    .unwrap_or_default();
                made.coordinates.push(CoordinateDimension {
                    point,
                    axis,
                    dimension,
                    expression,
                    parameter,
                });
            }
        }
        if let Some(before) = before {
            let change = std::mem::replace(
                &mut edit.change,
                "the coordinates written as expressions".to_owned(),
            );
            let checked = edit.require_needed(before);
            edit.change = change;
            checked?;
        }
        Ok(made)
    }
}

/// The new points of the edit at `at` (within a nanometre or so).
fn new_points_at(edit: &SketchEdit<'_>, at: [f64; 2]) -> Vec<EntityUid> {
    let tolerance = 1e-9 * (1.0 + at[0].abs().max(at[1].abs()));
    let mut found: Vec<EntityUid> = Vec::new();
    for r in &edit.report.entities {
        let Ref::Point(id) = *r else {
            continue;
        };
        let here = matches!(
            edit.def.entity(id).map(|e| &e.kind),
            Some(EntityKind::Point { at: p })
                if (p[0] - at[0]).abs() <= tolerance && (p[1] - at[1]).abs() <= tolerance
        );
        if here && !found.contains(&id) {
            found.push(id);
        }
    }
    found
}

impl SketchEdit<'_> {
    /// The value in millimetres of `text`, an expression of the document's
    /// parameters read as a length, as a dimension's value would read it
    /// (mitcad#1897); nothing of the document changes. `slot` names the
    /// input in a refusal (`end.x: parameter w does not exist`).
    pub fn length_of(&self, slot: &str, text: &str) -> Result<f64, String> {
        let mut scratch = (*self.params).clone();
        let value = ValueInput::Name(text.to_owned());
        let (id, _) = resolve_value(&mut scratch, slot, &value, self.owner(), slot, None)
            .map_err(|e| e.to_string())?;
        match scratch.value(id) {
            Some(length) if length.is_finite() => Ok(length),
            _ => Err(format!("{slot}: “{text}” is not a length")),
        }
    }
}
