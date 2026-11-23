// SPDX-License-Identifier: MIT
//! Equation curves (mitcad#2237): a curve given by formulas in the
//! design's expression language, drawn as a fit point spline through
//! points the formulas place.
//!
//! - **Modes.** `explicit`: `y = f(x)` over a range of `x`; `parametric`:
//!   `x(t)` and `y(t)` over a range of `t`; `polar`: `r(t)` with `t` the
//!   angle, the point `(r cos t, r sin t)`.
//! - **Units.** Inside a formula the variable is a plain number: `x` in
//!   the design's length unit, `t` as it is (a polar curve's `t` in
//!   radians), and a plain number given to `sin`, `cos` or `tan` is in
//!   radians, so that `x^2 / 20` and `r * (cos(t) + t * sin(t))` read as
//!   on paper. Parameters keep their units (a length parameter is a
//!   length); a formula gives a length, or a plain number in the design's
//!   length unit. The range: an explicit curve's in the design's length
//!   unit, a polar curve's in its angle unit, a parametric curve's plain
//!   numbers (an angle in radians). The variable hides a parameter of the
//!   same name inside the formulas.
//! - **A variable with a unit** (mitcad#2264, [`VariableUnit`]): the
//!   variable may instead be a length (an explicit curve's `x`, a
//!   parametric curve's `t`) or an angle (a parametric or polar curve's
//!   `t`); inside the formulas it is then that quantity (`t^2 / 10 mm` is a
//!   length), and the range is read as one (a bare number in the design's
//!   length or angle unit).
//! - **Points.** Evenly spaced values of the variable over the range, as
//!   many as keep the spline within the tolerance of the formulas (checked
//!   midway between the points), at most [`MAX_POINTS`]. A curve that ends
//!   where it starts is closed, smooth there. The points are fixed for the
//!   solver; other curves may share them (a line from the curve's end).
//! - **Following the parameters.** Before every solve (an edit's and the
//!   sketch's evaluation) the curve's points are placed again from the
//!   formulas with the parameters as they are, the same number of them;
//!   an edit of the curve places as many as its tolerance needs.
//! - **The record.** The sketch keeps the formulas beside the spline
//!   ([`EquationCurve`], `SketchDef::equations`); the spline is an
//!   ordinary fit point spline for everything else (profiles, offsets,
//!   exports). A curve that is no longer a fit point spline (deleted,
//!   trimmed, broken, a point inserted) loses its record and stays the
//!   geometry it is.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::edit::SketchEdit;
use crate::expr::{
    AngleUnit, ColumnError, Dims, EvalContext, Expr, Lookup, ParameterTable, Quantity, TableColumn,
    Unit, replace_references,
};
use crate::geometry::{Curve2, Nurbs, dist};
use crate::ids::EntityUid;
use crate::values::ValueInput;
use crate::{Entity, EntityKind, Ref};
use mitcad_model_base::internal::ids::curve_serde;

/// The tolerance of a curve that names none, mm.
pub const DEFAULT_TOLERANCE: f64 = 0.01;
/// The most points a curve gets: a spline through more takes the solver
/// too long.
pub const MAX_POINTS: usize = 200;
/// The fewest points a curve gets.
const MIN_POINTS: usize = 8;
/// The smallest and largest tolerance, mm.
const TOLERANCE_RANGE: (f64, f64) = (1e-6, 10.0);

/// How the formulas give the curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquationMode {
    /// `y` of `x`.
    #[default]
    Explicit,
    /// `x` and `y` of `t`.
    Parametric,
    /// `r` of the angle `t`.
    Polar,
}

impl EquationMode {
    /// The variable the formulas use: `x` or `t`.
    pub fn variable(self) -> &'static str {
        match self {
            Self::Explicit => "x",
            Self::Parametric | Self::Polar => "t",
        }
    }

    /// The name in files and commands.
    pub fn name(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Parametric => "parametric",
            Self::Polar => "polar",
        }
    }

    /// The formula fields the mode has, in order.
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Explicit => &["y"],
            Self::Parametric => &["x", "y"],
            Self::Polar => &["r"],
        }
    }
}

/// What an equation curve's variable is inside its formulas and its range
/// (mitcad#2264).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableUnit {
    /// A plain number (`none`): an explicit curve's `x` in the design's
    /// length unit, a parametric curve's `t` as it is, a polar curve's `t`
    /// in radians.
    #[default]
    #[serde(rename = "none")]
    Plain,
    /// An angle: a parametric or polar curve's `t`.
    Angle,
    /// A length: an explicit curve's `x` or a parametric curve's `t`.
    Length,
}

impl VariableUnit {
    /// The name in files and commands.
    pub fn name(self) -> &'static str {
        match self {
            Self::Plain => "none",
            Self::Angle => "angle",
            Self::Length => "length",
        }
    }

    /// A plain number: what a record leaves out.
    pub fn is_plain(&self) -> bool {
        *self == Self::Plain
    }
}

/// What a curve's range is read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RangeKind {
    Length,
    Angle,
    Plain,
}

/// The range of a curve of `mode` whose variable is `unit`: an explicit
/// curve's a length, a polar curve's an angle, a parametric curve's what
/// its variable is.
fn range_kind(mode: EquationMode, unit: VariableUnit) -> RangeKind {
    match (mode, unit) {
        (_, VariableUnit::Length) | (EquationMode::Explicit, _) => RangeKind::Length,
        (_, VariableUnit::Angle) | (EquationMode::Polar, _) => RangeKind::Angle,
        (EquationMode::Parametric, VariableUnit::Plain) => RangeKind::Plain,
    }
}

/// The variable's value as the formulas see it, `value` in the design's
/// (`design`) length or angle unit when it has one, else as it is: what
/// the `formula_help` query evaluates a formula with.
pub fn variable_value(value: f64, unit: VariableUnit, design: &EvalContext) -> Quantity {
    match unit {
        VariableUnit::Plain => Quantity::unitless(value),
        VariableUnit::Length => Quantity::new(
            value * design.default_length_unit.millimetres(),
            Dims::LENGTH,
        ),
        VariableUnit::Angle => {
            Quantity::new(value * design.default_angle_unit.radians(), Dims::ANGLE)
        }
    }
}

fn default_tolerance() -> f64 {
    DEFAULT_TOLERANCE
}

/// An equation curve's formulas, kept beside its spline `curve`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquationCurve {
    #[serde(with = "curve_serde")]
    pub curve: EntityUid,
    pub mode: EquationMode,
    /// A parametric curve's `x(t)`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub x: String,
    /// An explicit curve's `y(x)`, a parametric curve's `y(t)`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub y: String,
    /// A polar curve's `r(t)`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub r: String,
    /// The range of the variable, as expressions of the parameters.
    pub start: String,
    pub end: String,
    /// How far the spline may be from the formulas, mm.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    /// What the variable is (mitcad#2264): a plain number (`none`, left
    /// out), an angle or a length.
    #[serde(default, skip_serializing_if = "VariableUnit::is_plain")]
    pub variable_unit: VariableUnit,
}

impl EquationCurve {
    /// The formula of a field (`x`, `y` or `r`).
    pub fn formula(&self, field: &str) -> &str {
        match field {
            "x" => &self.x,
            "y" => &self.y,
            _ => &self.r,
        }
    }

    fn formula_mut(&mut self, field: &str) -> &mut String {
        match field {
            "x" => &mut self.x,
            "y" => &mut self.y,
            _ => &mut self.r,
        }
    }

    /// The mode's formulas with their fields.
    pub fn formulas(&self) -> Vec<(&'static str, &str)> {
        self.mode
            .fields()
            .iter()
            .map(|f| (*f, self.formula(f)))
            .collect()
    }

    /// The formulas the mode does not use emptied.
    pub fn normalize(&mut self) {
        for field in ["x", "y", "r"] {
            if !self.mode.fields().contains(&field) {
                self.formula_mut(field).clear();
            }
        }
        for text in [
            &mut self.x,
            &mut self.y,
            &mut self.r,
            &mut self.start,
            &mut self.end,
        ] {
            *text = text.trim().to_owned();
        }
    }

    /// The checks that need no parameters: the mode's formulas and the
    /// range are there and parse, the others empty, the tolerance in range.
    pub fn check_fields(&self) -> Result<(), String> {
        for field in ["x", "y", "r"] {
            let used = self.mode.fields().contains(&field);
            let text = self.formula(field).trim();
            if used && text.is_empty() {
                return Err(format!(
                    "{} curve needs the formula {field}",
                    article(self.mode)
                ));
            }
            if !used && !text.is_empty() {
                return Err(format!(
                    "{} curve has no formula {field}: it has {}",
                    article(self.mode),
                    self.mode.fields().join(" and ")
                ));
            }
            if used {
                Expr::parse(text)
                    .map_err(|e| format!("the formula {field} does not read: {}", e.message()))?;
            }
        }
        for (name, text) in [("start", &self.start), ("end", &self.end)] {
            if text.trim().is_empty() {
                return Err(format!("the curve needs its {name}"));
            }
            Expr::parse(text.trim())
                .map_err(|e| format!("{name} does not read: {}", e.message()))?;
        }
        match (self.mode, self.variable_unit) {
            (EquationMode::Explicit, VariableUnit::Angle) => {
                return Err(
                    "an explicit curve's variable x is a plain number or a length, not an angle"
                        .to_owned(),
                );
            }
            (EquationMode::Polar, VariableUnit::Length) => {
                return Err(
                    "a polar curve's variable t is its angle: a plain number or an angle, not a \
                     length"
                        .to_owned(),
                );
            }
            _ => {}
        }
        let (low, high) = TOLERANCE_RANGE;
        if !(self.tolerance.is_finite() && self.tolerance >= low && self.tolerance <= high) {
            return Err(format!(
                "the tolerance must be from {low} mm to {high} mm, got {}",
                self.tolerance
            ));
        }
        Ok(())
    }

    /// The parameters its formulas and range name (the variable is none).
    pub fn parameter_names(&self) -> BTreeSet<String> {
        let variable = self.mode.variable();
        let mut names = BTreeSet::new();
        for (_, text) in self.formulas() {
            if let Ok(expr) = Expr::parse(text) {
                names.extend(
                    expr.references()
                        .into_iter()
                        .filter(|n| *n != variable)
                        .map(str::to_owned),
                );
            }
        }
        for text in [&self.start, &self.end] {
            if let Ok(expr) = Expr::parse(text) {
                names.extend(expr.references().into_iter().map(str::to_owned));
            }
        }
        names
    }

    /// Rewrites the references to a renamed parameter; whether any
    /// changed. Inside the formulas the variable's name is the variable.
    pub fn rename_parameter(&mut self, old: &str, new: &str) -> bool {
        let variable = self.mode.variable();
        let mut changed = false;
        let mut rename = |text: &mut String, formula: bool| {
            if formula && old == variable {
                return;
            }
            if let Ok(expr) = Expr::parse(text)
                && expr.references().contains(&old)
            {
                *text = replace_references(text, &expr, old, new);
                changed = true;
            }
        };
        for field in self.mode.fields() {
            rename(self.formula_mut(field), true);
        }
        rename(&mut self.start, false);
        rename(&mut self.end, false);
        changed
    }
}

/// "An explicit", "a parametric", "a polar".
fn article(mode: EquationMode) -> &'static str {
    match mode {
        EquationMode::Explicit => "an explicit",
        EquationMode::Parametric => "a parametric",
        EquationMode::Polar => "a polar",
    }
}

/// Where the formulas put a curve's points.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub points: Vec<[f64; 2]>,
    /// It ends where it starts: a closed spline through the points.
    pub closed: bool,
    /// The largest distance of the spline from the formulas found midway
    /// between the points, mm.
    pub deviation: f64,
    /// Its points reached [`MAX_POINTS`] with the deviation still above
    /// the tolerance.
    pub capped: bool,
    /// The curve crosses itself.
    pub crosses: bool,
}

/// The parameters with a formula's variable on top, `value` (a plain
/// number, or a length or an angle, mitcad#2264) named `name`: what an
/// equation curve's formula is evaluated with, and the `formula_help`
/// query with its `variable` (mitcad#2237).
pub struct WithVariable<'a> {
    pub table: &'a ParameterTable,
    pub name: &'a str,
    pub value: Quantity,
}

/// The context of an equation curve's formulas in a design whose own is
/// `design`: its length unit, and radians for plain numbers given to
/// `sin`, `cos` and `tan`.
pub fn formula_context(design: EvalContext) -> EvalContext {
    EvalContext::new(design.default_length_unit, AngleUnit::Radian)
}

/// What a formula's value is in millimetres: a length, or a plain number
/// in the length unit of `context`; None for a value with other units.
pub fn formula_length(value: Quantity, context: &EvalContext) -> Option<f64> {
    if value.dims == Dims::LENGTH {
        Some(value.value)
    } else if value.dims == Dims::NONE {
        Some(value.value * Unit::of_length(context.default_length_unit).to_canonical(1.0))
    } else {
        None
    }
}

impl Lookup for WithVariable<'_> {
    fn lookup(&self, name: &str) -> Option<Quantity> {
        if name == self.name {
            Some(self.value)
        } else {
            self.table.lookup(name)
        }
    }

    fn lookup_text(&self, name: &str) -> Option<String> {
        if name == self.name {
            None
        } else {
            self.table.lookup_text(name)
        }
    }

    fn column(&self, table: &str, column: &str) -> Result<TableColumn<'_>, ColumnError> {
        self.table.column(table, column)
    }
}

/// A curve's formulas read, with its range in the variable's own terms:
/// a length in millimetres (an explicit curve's, a parametric curve's
/// length variable), an angle in radians (a polar curve's, a parametric
/// curve's angle variable), a parametric curve's plain variable as numbers.
struct Prepared<'a> {
    table: &'a ParameterTable,
    mode: EquationMode,
    unit: VariableUnit,
    formulas: Vec<(&'static str, Expr)>,
    start: f64,
    end: f64,
    /// Millimetres per unit of the design's length unit.
    length_factor: f64,
    /// The design's own context: its length and angle units.
    design: EvalContext,
    /// The formulas' context: the design's length unit, radians.
    context: EvalContext,
}

impl<'a> Prepared<'a> {
    fn new(curve: &EquationCurve, table: &'a ParameterTable) -> Result<Self, String> {
        curve.check_fields()?;
        let design = table.context();
        let context = formula_context(design);
        let length_factor = Unit::of_length(design.default_length_unit).to_canonical(1.0);
        let mut formulas = Vec::new();
        for (field, text) in curve.formulas() {
            let expr = Expr::parse(text.trim())
                .map_err(|e| format!("the formula {field} does not read: {}", e.message()))?;
            formulas.push((field, expr));
        }
        let bound = |name: &str, text: &str| -> Result<f64, String> {
            let expr = Expr::parse(text.trim())
                .map_err(|e| format!("{name} does not read: {}", e.message()))?;
            let value = match range_kind(curve.mode, curve.variable_unit) {
                RangeKind::Length => {
                    expr.eval_as(table, &design, Unit::of_length(design.default_length_unit))
                        .map_err(|e| format!("{name}: {}", table.eval_message(&e, "")))?
                        .value
                }
                RangeKind::Angle => {
                    expr.eval_as(table, &design, Unit::of_angle(design.default_angle_unit))
                        .map_err(|e| format!("{name}: {}", table.eval_message(&e, "")))?
                        .value
                }
                RangeKind::Plain => {
                    let q = expr
                        .eval(table, &context)
                        .map_err(|e| format!("{name}: {}", table.eval_message(&e, "")))?;
                    if q.dims != Dims::NONE && q.dims != Dims::ANGLE {
                        return Err(format!(
                            "{name} gives a value with units ({}): a parametric curve's t is \
                             a plain number",
                            q.dims
                        ));
                    }
                    q.value
                }
            };
            if value.is_finite() {
                Ok(value)
            } else {
                Err(format!("{name} is not a finite value"))
            }
        };
        let start = bound("start", &curve.start)?;
        let end = bound("end", &curve.end)?;
        if end <= start {
            return Err(format!(
                "the range is empty or reversed: its end ({}) must be greater than its start ({})",
                curve.end.trim(),
                curve.start.trim()
            ));
        }
        Ok(Self {
            table,
            mode: curve.mode,
            unit: curve.variable_unit,
            formulas,
            start,
            end,
            length_factor,
            design,
            context,
        })
    }

    /// The variable's value as the formulas see it: a length or an angle
    /// as it is, a plain explicit `x` in the design's length unit.
    fn shown(&self, v: f64) -> Quantity {
        match (self.mode, self.unit) {
            (_, VariableUnit::Length) => Quantity::new(v, Dims::LENGTH),
            (_, VariableUnit::Angle) => Quantity::new(v, Dims::ANGLE),
            (EquationMode::Explicit, VariableUnit::Plain) => {
                Quantity::unitless(v / self.length_factor)
            }
            (EquationMode::Parametric | EquationMode::Polar, VariableUnit::Plain) => {
                Quantity::unitless(v)
            }
        }
    }

    /// The variable's value at `v` in words: `2.5`, a length `5 mm` and an
    /// angle `90 deg` in the design's units.
    fn shown_text(&self, v: f64) -> String {
        let number = |value: f64| crate::expr::format_number(value, 6);
        match self.unit {
            VariableUnit::Plain => number(self.shown(v).value),
            VariableUnit::Length => format!(
                "{} {}",
                number(v / self.length_factor),
                self.design.default_length_unit.symbol()
            ),
            VariableUnit::Angle => format!(
                "{} {}",
                number(v / self.design.default_angle_unit.radians()),
                self.design.default_angle_unit.symbol()
            ),
        }
    }

    /// A formula's value at `v`, mm.
    fn value(&self, field: &str, expr: &Expr, v: f64) -> Result<f64, String> {
        let variable = self.mode.variable();
        let lookup = WithVariable {
            table: self.table,
            name: variable,
            value: self.shown(v),
        };
        let at = format!("{variable} = {}", self.shown_text(v));
        let q = expr.eval(&lookup, &self.context).map_err(|e| {
            format!(
                "the formula {field} cannot be evaluated at {at}: {}",
                self.table.eval_message(&e, "")
            )
        })?;
        let Some(mm) = formula_length(q, &self.context) else {
            return Err(format!(
                "the formula {field} gives a value with units {} at {at}: it is to give a length \
                 or a plain number",
                q.dims
            ));
        };
        if mm.is_finite() {
            Ok(mm)
        } else {
            Err(format!("the formula {field} gives no finite value at {at}"))
        }
    }

    /// The curve's point at `v`.
    fn point(&self, v: f64) -> Result<[f64; 2], String> {
        let value = |i: usize| -> Result<f64, String> {
            let (field, expr) = &self.formulas[i];
            self.value(field, expr, v)
        };
        Ok(match self.mode {
            EquationMode::Explicit => [v, value(0)?],
            EquationMode::Parametric => [value(0)?, value(1)?],
            EquationMode::Polar => {
                let r = value(0)?;
                [r * v.cos(), r * v.sin()]
            }
        })
    }

    /// The variable at point `i` of `count`.
    fn param(&self, i: usize, count: usize, closed: bool) -> f64 {
        let segments = if closed { count } else { count - 1 };
        self.start + (self.end - self.start) * i as f64 / segments as f64
    }

    /// Whether the curve ends where it starts.
    fn closes(&self, tolerance: f64) -> Result<bool, String> {
        if self.mode == EquationMode::Explicit {
            return Ok(false);
        }
        Ok(dist(self.point(self.start)?, self.point(self.end)?) <= tolerance)
    }

    /// `count` points, the spline through them and its deviation.
    fn place_count(&self, count: usize, closed: bool, tolerance: f64) -> Result<Placement, String> {
        let least = if closed { 3 } else { 2 };
        let count = count.max(least);
        let variable = self.mode.variable();
        let points = (0..count)
            .map(|i| self.point(self.param(i, count, closed)))
            .collect::<Result<Vec<_>, _>>()?;
        let segments = if closed { count } else { count - 1 };
        let size = points
            .iter()
            .fold(0.0_f64, |m, p| m.max(p[0].abs()).max(p[1].abs()))
            .max(1.0);
        for i in 0..segments {
            let j = (i + 1) % count;
            if dist(points[i], points[j]) <= 1e-9 * size {
                let (a, b) = (
                    self.shown_text(self.param(i, count, closed)),
                    self.shown_text(self.param(i + 1, count, closed)),
                );
                return Err(format!(
                    "the formulas give the same point at {variable} = {a} and {variable} = {b}: a \
                     curve through it twice in a row cannot be drawn; change the range or the \
                     formulas"
                ));
            }
        }
        let curve = spline_through(&points, closed)?;
        let knots = match &curve {
            Curve2::Nurbs(n) => n.distinct_knots().0,
            _ => Vec::new(),
        };
        let mut middles = Vec::with_capacity(segments);
        let mut deviation = 0.0_f64;
        for i in 0..segments {
            let (v0, v1) = (
                self.param(i, count, closed),
                self.param(i + 1, count, closed),
            );
            // A quarter, half and three quarters of the way.
            for f in [0.25, 0.5, 0.75] {
                let at = self.point(v0 + f * (v1 - v0))?;
                let away = if knots.len() == segments + 1 {
                    let hint = knots[i] + f * (knots[i + 1] - knots[i]);
                    curve.closest_near(at, hint).1
                } else {
                    curve.closest(at).1
                };
                deviation = deviation.max(away);
                if f == 0.5 {
                    middles.push(at);
                }
            }
        }
        let crosses = self.mode != EquationMode::Explicit && crosses(&points, &middles, closed);
        Ok(Placement {
            points,
            closed,
            deviation,
            capped: deviation > tolerance,
            crosses,
        })
    }

    /// As many points as keep the spline within `tolerance`, at most
    /// [`MAX_POINTS`]: within four fifths of it where the deviation is
    /// checked, so that it holds between those places too.
    fn place(&self, tolerance: f64) -> Result<Placement, String> {
        let closed = self.closes(tolerance)?;
        let target = 0.8 * tolerance;
        let mut count = MIN_POINTS;
        loop {
            let placed = self.place_count(count, closed, tolerance)?;
            if placed.deviation <= target || count >= MAX_POINTS {
                return Ok(placed);
            }
            // A cubic spline's error falls with the fourth power of the
            // spacing.
            let ratio = (placed.deviation / target).powf(0.25) * 1.2;
            let next = (count as f64 * ratio.clamp(1.25, 4.0)).ceil() as usize;
            count = next.clamp(count + 1, MAX_POINTS);
        }
    }
}

/// The spline the sketch draws through `points`, as its solve makes it.
fn spline_through(points: &[[f64; 2]], closed: bool) -> Result<Curve2, String> {
    use mitcad_solver::{SplineEnd, System};
    let mut system = System::new();
    let ids: Vec<_> = points
        .iter()
        .map(|p| system.add_point(p[0], p[1]))
        .collect();
    let end = if closed {
        SplineEnd::Periodic
    } else {
        SplineEnd::Natural
    };
    let id = system
        .add_fitted_spline(&ids, [end, end])
        .map_err(|e| format!("no spline runs through the curve's points: {e}"))?;
    let g = system
        .spline(id)
        .ok_or("no spline runs through the curve's points")?;
    Ok(Curve2::Nurbs(Nurbs {
        degree: g.degree,
        control: g.control_points,
        weights: Vec::new(),
        knots: g.knots,
    }))
}

/// Whether the polyline through the points and the formulas' points
/// midway between them crosses itself.
fn crosses(points: &[[f64; 2]], middles: &[[f64; 2]], closed: bool) -> bool {
    let mut line = Vec::with_capacity(points.len() + middles.len() + 1);
    for (i, p) in points.iter().enumerate() {
        line.push(*p);
        if let Some(m) = middles.get(i) {
            line.push(*m);
        }
    }
    if closed {
        line.push(points[0]);
    }
    let n = line.len().saturating_sub(1);
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    for i in 0..n {
        let (a, b) = (line[i], line[i + 1]);
        let low = [a[0].min(b[0]), a[1].min(b[1])];
        let high = [a[0].max(b[0]), a[1].max(b[1])];
        for j in i + 2..n {
            // The first and last pieces of a closed curve meet.
            if closed && i == 0 && j == n - 1 {
                continue;
            }
            let (c, d) = (line[j], line[j + 1]);
            if c[0].max(d[0]) < low[0]
                || c[0].min(d[0]) > high[0]
                || c[1].max(d[1]) < low[1]
                || c[1].min(d[1]) > high[1]
            {
                continue;
            }
            let (d1, d2) = (cross(a, b, c), cross(a, b, d));
            let (d3, d4) = (cross(c, d, a), cross(c, d, b));
            if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
                return true;
            }
        }
    }
    false
}

/// Places a curve's points from its formulas with the parameters of
/// `table`: as many as its tolerance needs.
pub fn place(curve: &EquationCurve, table: &ParameterTable) -> Result<Placement, String> {
    Prepared::new(curve, table)?.place(curve.tolerance)
}

/// Places `count` points from a curve's formulas, open or `closed`: the
/// spline it has, again with the parameters as they are.
pub fn place_count(
    curve: &EquationCurve,
    table: &ParameterTable,
    count: usize,
    closed: bool,
) -> Result<Placement, String> {
    Prepared::new(curve, table)?.place_count(count, closed, curve.tolerance)
}

/// The parameters a sketch's equation curves name: what its evaluation
/// reads.
pub fn used_parameters(def: &crate::def::SketchDef) -> BTreeSet<String> {
    def.equations
        .iter()
        .flat_map(EquationCurve::parameter_names)
        .collect()
}

/// The spline of a curve's record: its points and whether it is closed;
/// None when the curve is gone or no longer a fit point spline.
fn spline_of(entities: &[Entity], curve: EntityUid) -> Option<(Vec<EntityUid>, bool)> {
    entities
        .iter()
        .find(|e| e.id == curve)
        .and_then(|e| match &e.kind {
            EntityKind::FittedSpline { points, closed } if points.len() >= 2 => {
                Some((points.clone(), *closed))
            }
            _ => None,
        })
}

/// Sets point `id` at `at`, fixed.
fn set_fixed_point(entities: &mut [Entity], id: EntityUid, at: [f64; 2]) {
    if let Some(e) = entities.iter_mut().find(|e| e.id == id) {
        e.kind = EntityKind::Point { at };
        e.fixed = true;
    }
}

/// A sketch's equation curves placed again from their formulas with the
/// parameters of `table`, the same number of points each, before a solve:
/// the records whose curves are no longer fit point splines are dropped,
/// and each curve's points are fixed where its formulas put them now. What
/// a person is to know comes back as warnings: a curve that keeps its last
/// shape because its formulas fail now (a parameter deleted, a value with
/// no finite result), one that deviates from its formulas more than its
/// tolerance with the points it has, a profile curve that crosses itself.
pub fn follow_formulas(def: &mut crate::def::SketchDef, table: &ParameterTable) -> Vec<String> {
    let mut warnings = Vec::new();
    let entities = &def.entities;
    def.equations
        .retain(|q| spline_of(entities, q.curve).is_some());
    for q in &def.equations {
        let Some((points, closed)) = spline_of(&def.entities, q.curve) else {
            continue;
        };
        let construction = def.entity(q.curve).is_some_and(|e| e.construction);
        let id = Ref::Curve(q.curve);
        match place_count(q, table, points.len(), closed) {
            Ok(placed) => {
                for (p, at) in points.iter().zip(&placed.points) {
                    set_fixed_point(&mut def.entities, *p, *at);
                }
                if placed.deviation > q.tolerance * (1.0 + 1e-6) {
                    warnings.push(crate::said(
                        format!(
                            "an equation curve deviates up to {} mm from its formulas with its {} \
                             points, more than its tolerance of {} mm: edit it to place more \
                             points",
                            crate::expr::format_number(placed.deviation, 4),
                            points.len(),
                            q.tolerance
                        ),
                        format!(
                            "equation curve {id} deviates up to {} mm from its formulas with its \
                             {} points (tolerance {} mm): sketch.edit_equation_curve places more",
                            crate::expr::format_number(placed.deviation, 4),
                            points.len(),
                            q.tolerance
                        ),
                    ));
                }
                if placed.crosses && !construction {
                    warnings.push(crate::said(
                        "an equation curve crosses itself, so it bounds no simple profile there",
                        format!("equation curve {id} crosses itself"),
                    ));
                }
            }
            Err(e) => {
                for p in &points {
                    if let Some(entity) = def.entities.iter_mut().find(|x| x.id == *p) {
                        entity.fixed = true;
                    }
                }
                warnings.push(crate::said(
                    format!("an equation curve keeps its last shape: {e}"),
                    format!("equation curve {id} keeps its last shape: {e}"),
                ));
            }
        }
    }
    warnings
}

/// What a `sketch.equation_curve` or `sketch.edit_equation_curve` gives:
/// the formulas, `None` for a field left as it is (an edit) or the
/// default (a new curve).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EquationInput {
    pub mode: Option<EquationMode>,
    pub x: Option<String>,
    pub y: Option<String>,
    pub r: Option<String>,
    /// The range: an expression, or a number as commands give values (an
    /// explicit curve's in millimetres, a polar curve's in radians, a
    /// parametric curve's plain), kept as an expression in the design's
    /// units ([`bound_text`]).
    pub start: Option<ValueInput>,
    pub end: Option<ValueInput>,
    pub tolerance: Option<f64>,
    pub construction: Option<bool>,
    /// What the variable is (mitcad#2264); a number of the range is read
    /// as one (a length in millimetres, an angle in radians).
    pub variable_unit: Option<VariableUnit>,
}

/// A range value as the curve keeps it: an expression as written, a number
/// as an expression in the design's units (`context`): a length (an
/// explicit curve's, a parametric curve's length variable's) millimetres
/// in its length unit (`-10 mm`), an angle (a polar curve's, a parametric
/// curve's angle variable's) radians in its angle unit (`720 deg`), a
/// parametric curve's plain variable's number as it is.
pub fn bound_text(
    value: &ValueInput,
    mode: EquationMode,
    unit: VariableUnit,
    context: EvalContext,
) -> Result<String, String> {
    match value {
        ValueInput::Name(text) => Ok(text.trim().to_owned()),
        ValueInput::Number(v) if !v.is_finite() => {
            Err(format!("the range must be finite, got {v}"))
        }
        ValueInput::Number(v) => Ok(match range_kind(mode, unit) {
            RangeKind::Length => {
                crate::expr::value_to_expression(*v, Unit::of_length(context.default_length_unit))
            }
            RangeKind::Angle => {
                crate::expr::value_to_expression(*v, Unit::of_angle(context.default_angle_unit))
            }
            RangeKind::Plain => crate::expr::format_number_exact(*v),
        }),
    }
}

/// What a curve made or edited is.
#[derive(Debug, Clone, PartialEq)]
pub struct EquationMade {
    pub curve: EntityUid,
    pub points: Vec<EntityUid>,
    pub closed: bool,
    pub deviation: f64,
    pub capped: bool,
}

impl EquationInput {
    /// `curve` with the fields given, numbers of the range in the units of
    /// `context`.
    fn apply(&self, curve: &mut EquationCurve, context: EvalContext) -> Result<(), String> {
        if let Some(mode) = self.mode {
            curve.mode = mode;
        }
        if let Some(unit) = self.variable_unit {
            curve.variable_unit = unit;
        }
        for (field, value) in [("x", &self.x), ("y", &self.y), ("r", &self.r)] {
            if let Some(value) = value {
                *curve.formula_mut(field) = value.clone();
            }
        }
        if let Some(start) = &self.start {
            curve.start = bound_text(start, curve.mode, curve.variable_unit, context)?;
        }
        if let Some(end) = &self.end {
            curve.end = bound_text(end, curve.mode, curve.variable_unit, context)?;
        }
        if let Some(tolerance) = self.tolerance {
            curve.tolerance = tolerance;
        }
        curve.normalize();
        Ok(())
    }
}

impl SketchEdit<'_> {
    /// Places `curve`'s formulas and refuses a profile curve that crosses
    /// itself; the warning of a capped one is reported.
    fn place_equation(
        &mut self,
        curve: &EquationCurve,
        construction: bool,
    ) -> Result<Placement, String> {
        let placed = place(curve, self.params.table())?;
        if placed.crosses {
            if construction {
                self.report.warnings.push(
                    "the curve crosses itself: as construction geometry it bounds no profile"
                        .to_owned(),
                );
            } else {
                return Err(
                    "the curve crosses itself, so it cannot bound a profile: make it \
                            construction geometry, or change its range or formulas"
                        .to_owned(),
                );
            }
        }
        if placed.capped {
            self.report.warnings.push(format!(
                "with {MAX_POINTS} points, the most an equation curve takes, the curve is still \
                 up to {} mm from its formulas (its tolerance is {} mm): a larger tolerance or a \
                 shorter range follows them",
                crate::expr::format_number(placed.deviation, 4),
                curve.tolerance
            ));
        }
        Ok(placed)
    }

    /// Adds an equation curve (`sketch.equation_curve`, mitcad#2237): its
    /// fixed points where the formulas put them, then the fit point spline
    /// through them. Fields left out take their defaults: explicit, a
    /// plain variable, the tolerance [`DEFAULT_TOLERANCE`], not
    /// construction.
    pub fn equation_curve(&mut self, input: &EquationInput) -> Result<EquationMade, String> {
        let mut curve = EquationCurve {
            curve: EntityUid(0),
            mode: EquationMode::default(),
            x: String::new(),
            y: String::new(),
            r: String::new(),
            start: String::new(),
            end: String::new(),
            tolerance: DEFAULT_TOLERANCE,
            variable_unit: VariableUnit::Plain,
        };
        input.apply(&mut curve, self.params.context())?;
        let construction = input.construction.unwrap_or(false);
        let placed = self.place_equation(&curve, construction)?;
        let mut points = Vec::with_capacity(placed.points.len());
        for at in &placed.points {
            let id = self.add_point(*at);
            if let Some(e) = self.def.entity_mut(id) {
                e.fixed = true;
            }
            points.push(id);
        }
        let id = self.add_curve(EntityKind::FittedSpline {
            points: points.clone(),
            closed: placed.closed,
        });
        if let Some(e) = self.def.entity_mut(id) {
            e.construction = construction;
        }
        curve.curve = id;
        self.def.equations.push(curve);
        Ok(EquationMade {
            curve: id,
            points,
            closed: placed.closed,
            deviation: placed.deviation,
            capped: placed.capped,
        })
    }

    /// Changes an equation curve (`sketch.edit_equation_curve`): the
    /// fields given, the others as they are, its points placed again as
    /// many as its tolerance needs. The start point, and the end point of
    /// an open curve, keep their ids, so that geometry joined there stays
    /// joined; points it no longer needs are deleted with what holds them,
    /// unless another curve uses them (they then stay, no longer fixed).
    pub fn edit_equation_curve(
        &mut self,
        id: EntityUid,
        input: &EquationInput,
    ) -> Result<EquationMade, String> {
        let index = self
            .def
            .equations
            .iter()
            .position(|q| q.curve == id)
            .ok_or_else(|| match self.def.entity(id) {
                Some(_) => crate::said(
                    "the curve is not an equation curve",
                    format!("c{} is not an equation curve", id.0),
                ),
                None => format!("curve c{} does not exist", id.0),
            })?;
        let (old, old_closed) = spline_of(&self.def.entities, id).ok_or_else(|| {
            crate::said(
                "the curve is no longer an equation curve",
                format!("c{} is no longer a fit point spline", id.0),
            )
        })?;
        let mut curve = self.def.equations[index].clone();
        input.apply(&mut curve, self.params.context())?;
        let construction = input
            .construction
            .unwrap_or_else(|| self.def.entity(id).is_some_and(|e| e.construction));
        let placed = self.place_equation(&curve, construction)?;
        let count = placed.points.len();
        let interior_old: Vec<EntityUid> = if old_closed {
            old[1..].to_vec()
        } else {
            old[1..old.len() - 1].to_vec()
        };
        let old_end = (!old_closed).then(|| old[old.len() - 1]);
        let interior = if placed.closed { count - 1 } else { count - 2 };
        let mut points = vec![old[0]];
        for k in 0..interior {
            let at = placed.points[k + 1];
            let p = match interior_old.get(k) {
                Some(p) => *p,
                None => self.add_point(at),
            };
            points.push(p);
        }
        if !placed.closed {
            let at = placed.points[count - 1];
            let p = match old_end {
                Some(p) => p,
                None => self.add_point(at),
            };
            points.push(p);
        }
        for (p, at) in points.iter().zip(&placed.points) {
            set_fixed_point(&mut self.def.entities, *p, *at);
        }
        if let Some(e) = self.def.entity_mut(id) {
            e.kind = EntityKind::FittedSpline {
                points: points.clone(),
                closed: placed.closed,
            };
            e.construction = construction;
        }
        // The points it no longer needs.
        let kept: BTreeSet<EntityUid> = points.iter().copied().collect();
        let mut dropped = BTreeSet::new();
        for p in old.iter().filter(|p| !kept.contains(p)) {
            let used = self
                .def
                .entities
                .iter()
                .any(|e| e.id != id && e.kind.points().contains(p));
            if used {
                if let Some(e) = self.def.entity_mut(*p) {
                    e.fixed = false;
                }
            } else {
                dropped.insert(*p);
            }
        }
        self.drop_entities(&dropped);
        self.def.equations[index] = curve;
        Ok(EquationMade {
            curve: id,
            points,
            closed: placed.closed,
            deviation: placed.deviation,
            capped: placed.capped,
        })
    }
}

#[cfg(test)]
#[path = "equation_tests.rs"]
mod tests;
