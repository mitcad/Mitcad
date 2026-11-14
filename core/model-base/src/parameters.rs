// SPDX-License-Identifier: MIT
//! The document's parameters, as the Change Parameters dialog shows them: an
//! expression table ([`ParameterTable`]: names, expressions with units,
//! comments, evaluation in dependency order, cycle and unit checks, renames
//! that rewrite references) and the feature that owns each model parameter.
//! Values are millimetres and radians.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::expr::{
    AngleUnit, Dims, EvalContext, Expr, LengthUnit, ParamKind, ParamSpec, ParameterTable,
    TableError, TableSource, Unit,
};
use crate::expr::{Bounds, ParamLimits};
pub use crate::expr::{ChangeSet, ParamId, Parameter};
use crate::ids::FeatureUid;

#[derive(Debug, Clone, PartialEq)]
pub enum ParameterError {
    Unknown(String),
    NotFinite {
        name: String,
        value: f64,
    },
    /// The parameter is still used by these features.
    InUse {
        name: String,
        users: Vec<String>,
    },
    /// A name, expression, unit or reference the table refuses.
    Table(TableError),
    /// Changing the unit would give a feature a value of another kind.
    UnitInUse {
        name: String,
        unit: Unit,
        user: String,
        /// The unit of the user's slot.
        expected: Unit,
    },
    /// A driven dimension's reference parameter: read-only (mitcad#231).
    ReadOnly(String),
}

impl fmt::Display for ParameterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(name) => write!(f, "unknown parameter {name}"),
            Self::NotFinite { name, value } => {
                write!(f, "parameter {name} must be a finite number, got {value}")
            }
            Self::InUse { name, users } => {
                write!(f, "parameter {name} is used by {}", users.join(", "))
            }
            Self::ReadOnly(name) => write!(
                f,
                "parameter {name} is the value a driven dimension measures: it cannot be set \
                 (make the dimension driving to set it)"
            ),
            Self::Table(error) => error.fmt(f),
            Self::UnitInUse {
                name,
                unit,
                user,
                expected,
            } => write!(
                f,
                "parameter {name} cannot change to {unit}: {user} uses it as {}",
                describe(*expected)
            ),
        }
    }
}

impl std::error::Error for ParameterError {}

impl From<TableError> for ParameterError {
    fn from(error: TableError) -> Self {
        Self::Table(error)
    }
}

/// The unit a new parameter gets for a value of `dims`: the document's
/// length unit, degrees for angles, unitless, or millimetres and degrees
/// for other powers.
pub fn unit_for(dims: Dims, context: &EvalContext) -> Option<Unit> {
    if dims == Dims::LENGTH {
        Some(Unit::of_length(context.default_length_unit))
    } else if dims == Dims::ANGLE {
        Some(Unit::of_angle(context.default_angle_unit))
    } else {
        Unit::display_for(dims)
    }
}

/// A text's 64-bit FNV-1a hash, the same on every computer and in every
/// build (a recompute keeps it in stored results).
fn text_bits(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Parameters in creation order, with their owners.
#[derive(Debug, Clone, Default)]
pub struct Parameters {
    table: ParameterTable,
    /// The feature whose dimension a model parameter is; it is deleted with
    /// the feature unless something else still uses it.
    owners: BTreeMap<ParamId, FeatureUid>,
}

impl PartialEq for Parameters {
    fn eq(&self, other: &Self) -> bool {
        self.table.context() == other.table.context()
            && self.owners == other.owners
            && self.table.iter().eq(other.table.iter())
    }
}

impl Parameters {
    /// No parameters; `context` gives the document's default units.
    pub fn new(context: EvalContext) -> Self {
        Self {
            table: ParameterTable::new(context),
            owners: BTreeMap::new(),
        }
    }

    /// Parameters loaded from a file, with their saved order and owners.
    pub fn build(
        context: EvalContext,
        specs: Vec<(ParamSpec, Option<FeatureUid>)>,
    ) -> Result<Self, TableError> {
        Self::build_with_tables(context, specs, None)
    }

    /// [`Parameters::build`] whose expressions look values up in the
    /// document's `tables` (mitcad#667).
    pub fn build_with_tables(
        context: EvalContext,
        specs: Vec<(ParamSpec, Option<FeatureUid>)>,
        tables: Option<Arc<dyn TableSource>>,
    ) -> Result<Self, TableError> {
        let mut owners = BTreeMap::new();
        let mut table_specs = Vec::with_capacity(specs.len());
        for (i, (mut spec, owner)) in specs.into_iter().enumerate() {
            let id = ParamId::from_raw(u32::try_from(i).expect("fewer than 2^32 parameters"));
            if let Some(owner) = owner {
                owners.insert(id, owner);
                spec.kind = ParamKind::Model;
            }
            table_specs.push((id, spec));
        }
        Ok(Self {
            table: ParameterTable::build_with_tables(context, table_specs, tables)?,
            owners,
        })
    }

    /// The expression table.
    pub fn table(&self) -> &ParameterTable {
        &self.table
    }

    /// The document's default units.
    pub fn context(&self) -> EvalContext {
        self.table.context()
    }

    pub fn find(&self, name: &str) -> Option<ParamId> {
        self.table.find(name)
    }

    pub fn get(&self, id: ParamId) -> Option<&Parameter> {
        self.table.get(id)
    }

    /// The value in millimetres or radians.
    pub fn value(&self, id: ParamId) -> Option<f64> {
        self.get(id).map(Parameter::value)
    }

    /// The value of a text parameter (mitcad#664); None for a number or a
    /// missing parameter.
    pub fn text(&self, id: ParamId) -> Option<&str> {
        self.get(id).and_then(Parameter::text)
    }

    /// The value as a feature's recompute records it (the model's `recompute`):
    /// a number's bits, or for a text parameter a hash of its text.
    pub fn read_bits(&self, id: ParamId) -> Option<u64> {
        let param = self.get(id)?;
        Some(match param.text() {
            Some(text) => text_bits(text),
            None => param.value().to_bits(),
        })
    }

    /// The name of a parameter, or a placeholder for a missing one.
    pub fn name(&self, id: ParamId) -> String {
        self.get(id).map_or_else(
            || format!("<parameter {}>", id.raw()),
            |p| p.name().to_owned(),
        )
    }

    /// The feature that owns a model parameter.
    pub fn owner(&self, id: ParamId) -> Option<FeatureUid> {
        self.owners.get(&id).copied()
    }

    /// Parameters in creation order.
    pub fn iter(&self) -> impl Iterator<Item = &Parameter> {
        self.table.iter().map(|(_, p)| p)
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    fn require(&self, id: ParamId) -> Result<&Parameter, ParameterError> {
        self.get(id)
            .ok_or_else(|| ParameterError::Unknown(format!("<parameter {}>", id.raw())))
    }

    /// [`Parameters::require`] of a parameter whose value may be set: not a
    /// driven dimension's reference.
    fn require_settable(&self, id: ParamId) -> Result<&Parameter, ParameterError> {
        let param = self.require(id)?;
        if param.kind() == ParamKind::Reference {
            return Err(ParameterError::ReadOnly(param.name().to_owned()));
        }
        Ok(param)
    }

    /// Whether a parameter is a driven dimension's reference.
    pub fn is_reference(&self, id: ParamId) -> bool {
        self.get(id)
            .is_some_and(|p| p.kind() == ParamKind::Reference)
    }

    /// Adds a driven dimension's reference parameter with the next free
    /// name (`d<n>`), owned by its sketch (mitcad#231). Its value is what
    /// the dimension measures, set by the recompute.
    pub fn add_reference(
        &mut self,
        value: f64,
        unit: Unit,
        comment: &str,
        owner: FeatureUid,
    ) -> Result<ParamId, ParameterError> {
        let name = self.table.unused_name("d");
        let spec =
            ParamSpec::from_value(ParamKind::Reference, &name, value, unit).with_comment(comment);
        self.add(spec, Some(owner))
    }

    /// Sets a reference parameter to the value its dimension measures;
    /// returns the parameters whose values changed.
    pub fn set_measured(&mut self, id: ParamId, value: f64) -> Result<ChangeSet, ParameterError> {
        let name = self.require(id)?.name().to_owned();
        if !value.is_finite() {
            return Err(ParameterError::NotFinite { name, value });
        }
        Ok(self.table.set_value(id, value)?)
    }

    /// Makes a parameter a reference or a model parameter again; its owner
    /// stays.
    pub fn set_kind(&mut self, id: ParamId, kind: ParamKind) {
        let _ = self.table.set_kind(id, kind);
    }

    /// Adds a parameter; a parameter with an owner is a model parameter.
    pub fn add(
        &mut self,
        mut spec: ParamSpec,
        owner: Option<FeatureUid>,
    ) -> Result<ParamId, ParameterError> {
        if owner.is_some() && spec.kind != ParamKind::Reference {
            spec.kind = ParamKind::Model;
        }
        let id = self.table.add(spec)?;
        if let Some(owner) = owner {
            self.owners.insert(id, owner);
        }
        Ok(id)
    }

    /// Adds a model parameter with the next free name (`d1`,
    /// `d2`, ...).
    pub fn add_dimension(
        &mut self,
        expression: &str,
        unit: Unit,
        comment: &str,
        owner: Option<FeatureUid>,
    ) -> Result<ParamId, ParameterError> {
        let name = self.table.unused_name("d");
        let spec = ParamSpec::model(&name, expression, unit).with_comment(comment);
        self.add(spec, owner)
    }

    /// Changes the expression; returns the parameters whose values changed.
    pub fn set_expression(
        &mut self,
        id: ParamId,
        expression: &str,
    ) -> Result<ChangeSet, ParameterError> {
        self.require_settable(id)?;
        Ok(self.table.set_expression(id, expression)?)
    }

    /// Changes expressions atomically, keeping their units and owners;
    /// returns the parameters whose final values changed.
    pub fn set_expressions(
        &mut self,
        expressions: &[(ParamId, &str)],
    ) -> Result<ChangeSet, ParameterError> {
        for &(id, _) in expressions {
            self.require_settable(id)?;
        }
        Ok(self.table.set_expressions(expressions)?)
    }

    /// Sets the expression to a plain value in millimetres or radians,
    /// written in the parameter's unit.
    pub fn set_value(&mut self, id: ParamId, value: f64) -> Result<ChangeSet, ParameterError> {
        let name = self.require_settable(id)?.name().to_owned();
        if !value.is_finite() {
            return Err(ParameterError::NotFinite { name, value });
        }
        Ok(self.table.set_value(id, value)?)
    }

    /// Changes the expression and the unit together.
    pub fn update(
        &mut self,
        id: ParamId,
        expression: &str,
        unit: Unit,
    ) -> Result<ChangeSet, ParameterError> {
        self.require_settable(id)?;
        Ok(self.table.update(id, expression, unit)?)
    }

    pub fn rename(&mut self, id: ParamId, name: &str) -> Result<Vec<ParamId>, ParameterError> {
        self.require(id)?;
        Ok(self.table.rename(id, name)?)
    }

    pub fn set_comment(&mut self, id: ParamId, comment: &str) -> Result<(), ParameterError> {
        Ok(self.table.set_comment(id, comment)?)
    }

    /// Sets a parameter's limits (mitcad#665); a driven dimension's
    /// reference has none. Returns the parameters whose values changed.
    pub fn set_limits(
        &mut self,
        id: ParamId,
        limits: ParamLimits,
    ) -> Result<ChangeSet, ParameterError> {
        self.require_settable(id)?;
        Ok(self.table.set_limits(id, limits)?)
    }

    /// Changes the expression, the unit and, when given, the limits in one
    /// step (mitcad#665); returns the parameters whose values changed.
    pub fn redefine(
        &mut self,
        id: ParamId,
        expression: &str,
        unit: Unit,
        limits: Option<ParamLimits>,
    ) -> Result<ChangeSet, ParameterError> {
        self.require_settable(id)?;
        Ok(self.table.redefine(id, expression, unit, limits)?)
    }

    /// Changes the document's default units; bare numbers in expressions
    /// are read in them.
    pub fn set_context(&mut self, context: EvalContext) -> Result<ChangeSet, ParameterError> {
        Ok(self.table.set_context(context)?)
    }

    pub fn set_owner(&mut self, id: ParamId, owner: Option<FeatureUid>) {
        let kind = match owner {
            Some(owner) => {
                self.owners.insert(id, owner);
                ParamKind::Model
            }
            None => {
                self.owners.remove(&id);
                ParamKind::User
            }
        };
        let _ = self.table.set_kind(id, kind);
    }

    /// Removes a parameter no other parameter references.
    pub fn remove(&mut self, id: ParamId) -> Result<Parameter, ParameterError> {
        let removed = self.table.remove(id)?;
        self.owners.remove(&id);
        Ok(removed)
    }

    /// Parameters `owner` owns, in creation order.
    pub fn owned_by(&self, owner: FeatureUid) -> Vec<ParamId> {
        self.owners
            .iter()
            .filter(|(_, o)| **o == owner)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Removes the parameters `owner` owned that neither a feature
    /// (`used`) nor another parameter uses, repeatedly, so a dimension only
    /// referenced by another removed one goes too. Used ones become user
    /// parameters when the owner is gone (`owner_exists` false).
    pub fn release(
        &mut self,
        owner: FeatureUid,
        owner_exists: bool,
        used: &dyn Fn(ParamId) -> bool,
    ) {
        loop {
            let removable: Vec<ParamId> = self
                .owned_by(owner)
                .into_iter()
                .filter(|id| !used(*id) && self.table.dependents(*id).is_empty())
                .collect();
            if removable.is_empty() {
                break;
            }
            for id in removable {
                let _ = self.remove(id);
            }
        }
        if !owner_exists {
            for id in self.owned_by(owner) {
                self.set_owner(id, None);
            }
        }
    }

    // Tables (mitcad#667): the document's tables, which the expressions
    // look values up in. The model keeps them (`DocState::tables`); the
    // parameters evaluate with the same tables.

    /// The tables lookups read.
    pub fn tables(&self) -> Option<&Arc<dyn TableSource>> {
        self.table.tables()
    }

    /// Parameters whose expressions or limits name the table `name`, in
    /// creation order.
    pub fn table_users(&self, name: &str) -> Vec<ParamId> {
        self.table.table_users(name)
    }

    /// Replaces the tables and re-evaluates the parameters that use one of
    /// the tables `changed`, and their dependents, in one step; on failure
    /// nothing changes. Returns the parameters whose values changed.
    pub fn set_tables(
        &mut self,
        tables: Option<Arc<dyn TableSource>>,
        changed: &[&str],
    ) -> Result<ChangeSet, ParameterError> {
        Ok(self.table.set_tables(tables, changed)?)
    }

    /// Renames a table (`column` None) or one of its columns in every
    /// expression and limit; `tables` are the tables after the rename.
    /// Returns the parameters that were rewritten.
    pub fn rename_table_references(
        &mut self,
        tables: Option<Arc<dyn TableSource>>,
        table: &str,
        column: Option<&str>,
        new_name: &str,
    ) -> Result<Vec<ParamId>, ParameterError> {
        Ok(self
            .table
            .rename_table_references(tables, table, column, new_name)?)
    }
}

/// The unit of a feature value slot: slots whose name has `angle` in it
/// (`size.angle`, `extent.angle`, `extent.angle2`, `dimensions[k3].angle`)
/// or ends in `taper` (`extent.taper`, `side1.taper`) hold angles; counts,
/// factors and ratios (`helix.revolutions`, `extent.fraction`,
/// `start_condition.weight`, `tangency_weight`, `direction1.quantity`,
/// `scale.factor`, a thread's `starts`) are unitless; everything else
/// lengths.
pub fn slot_unit(slot: &str) -> Unit {
    let field = slot.rsplit('.').next().unwrap_or(slot);
    // A sketch text's text (`texts[t5].text`, mitcad#664).
    if field == "text" && slot.starts_with("texts[") {
        return Unit::TEXT;
    }
    if field.contains("angle") || field.ends_with("taper") {
        Unit::DEG
    } else if field == "revolutions"
        || field == "end_scale"
        || field.starts_with("fraction")
        || field.ends_with("weight")
        || slot == "allowance.k"
        // A motion link's ratio (mitcad#466).
        || slot == "coupling.ratio"
        // A gear's teeth and profile shift (mitcad#472).
        || field.ends_with("teeth")
        || field == "profile_shift"
        // A pulley's set screws (mitcad#562).
        || slot == "set_screws.count"
        // A printable bearing's elements (mitcad#577).
        || slot == "element_count"
        // A thread's or a tapped hole's starts (mitcad#893).
        || matches!(slot, "starts" | "thread.starts")
        // A press-fit hole's crush ribs (mitcad#1517).
        || slot == "press_fit.ribs"
        // A mortise and tenon's tenons (mitcad#1063).
        || slot == "count"
        // Every count, factor and fraction (mitcad#662): pattern quantities,
        // a pattern's or mirror's scale factors and their runs, a scale's
        // factors, a boss's ribs (construction paths' fractions are
        // `definition.fraction`).
        || field == "quantity"
        || matches!(slot, "scale" | "scale_groups" | "scale_copies" | "ribs")
        || (slot.starts_with("scales[") && matches!(field, "scale" | "groups" | "copies"))
        || matches!(slot, "scale.factor" | "scale.x" | "scale.y" | "scale.z")
    {
        Unit::NONE
    } else {
        Unit::MM
    }
}

/// Whether an angle slot of a feature of type `feature` means nothing more
/// beyond one turn (mitcad#714): a move's or move face's rotation
/// (`transform.angle`), a revolve's angles, a draft's, a chamfer's angle, an
/// extrusion's tapers, a hole's taper, countersink and tip angles, a
/// construction plane's angle, a sketch's rotation on its plane and an
/// emboss's Around, Rotation and Taper Angle. Helices, coils, sweeps'
/// twists and other slots that take several turns are not such slots.
#[doc(hidden)]
pub fn one_turn_slot(feature: &str, slot: &str) -> bool {
    let field = slot.rsplit('.').next().unwrap_or(slot);
    match feature {
        "move" | "move_face" => slot == "transform.angle",
        "revolve" => matches!(slot, "extent.angle" | "extent.angle1" | "extent.angle2"),
        "draft" => matches!(slot, "angle" | "angle2"),
        "chamfer" => field == "angle",
        "extrude" => field == "taper",
        "hole" => matches!(
            slot,
            "taper" | "kind.angle" | "tip_angle" | "press_fit.angle"
        ),
        "construction_plane" => slot == "definition.angle",
        // A sketch's rotation on its plane (mitcad#787).
        "sketch" => slot == "placement.angle",
        // An emboss's wrap turned about the axis and on the face (mitcad#1259)
        // and its taper (mitcad#1547).
        "emboss" => matches!(slot, "around_angle" | "rotation_angle" | "taper_angle"),
        // A coordinate system's turn about its Z axis (mitcad#1775).
        "coordinate_system" => slot == "angle",
        _ => false,
    }
}

/// Whether a parameter in `unit` can give a slot of `slot_unit` its value:
/// the same dimensions, or a unitless parameter (its number is used as is).
/// A text slot takes only a text parameter, and a text parameter fits only
/// a text slot (mitcad#664).
pub fn fits_slot(unit: Unit, slot_unit: Unit) -> bool {
    if unit.is_text() || slot_unit.is_text() {
        return unit.is_text() && slot_unit.is_text();
    }
    unit.is_unitless() || unit.dims() == slot_unit.dims()
}

/// `message` about a count, with a hint when `value` is a finite number
/// that is not whole (mitcad#662): counts are never rounded silently.
#[doc(hidden)]
pub fn count_error(message: String, value: f64) -> String {
    if value.is_finite() && (value - value.round()).abs() > 1e-9 {
        format!("{message}: use floor(…) or round(…) for a whole number")
    } else {
        message
    }
}

/// What kind of value a unit measures, for messages.
pub fn describe(unit: Unit) -> &'static str {
    let dims = unit.dims();
    if unit.is_text() {
        "a text"
    } else if dims == Dims::LENGTH {
        "a length"
    } else if dims == Dims::ANGLE {
        "an angle"
    } else if dims.is_dimensionless() {
        "a unitless value"
    } else {
        "a derived quantity"
    }
}

/// Parses an expression and returns its dimensions with the document's
/// values, for a parameter whose unit is not given.
#[doc(hidden)]
pub fn expression_dims(parameters: &Parameters, expression: &str) -> Result<Dims, ParameterError> {
    let expr = Expr::parse(expression).map_err(|error| {
        ParameterError::Table(TableError::Parse {
            parameter: String::new(),
            error,
        })
    })?;
    let context = parameters.context();
    let value = expr.eval(parameters.table(), &context).map_err(|error| {
        ParameterError::Table(TableError::Eval {
            parameter: String::new(),
            error,
        })
    })?;
    Ok(value.dims)
}

/// Whether an expression gives a text with the document's values
/// (mitcad#664): a parameter without a unit given is then a text
/// parameter.
#[doc(hidden)]
pub fn expression_is_text(parameters: &Parameters, expression: &str) -> bool {
    parameters
        .table()
        .evaluate_value(expression)
        .is_ok_and(|value| matches!(value, crate::expr::Value::Text(_)))
}

/// The default evaluation context of new documents: millimetres and
/// degrees.
pub fn default_context() -> EvalContext {
    EvalContext::new(LengthUnit::Millimetre, AngleUnit::Degree)
}

/// A feature's own limits of a value slot (mitcad#665), in canonical units
/// (millimetres, radians): what the feature refuses outside them, reported
/// with the parameter that gives the slot its value (the `parameters`
/// query's `intrinsic`) so that a client can show them before an error.
/// They are fixed: a parameter's own limits are set separately.
/// `feature_type` is the definition's `type` (`rectangular_pattern`).
pub fn slot_limits(feature_type: &str, slot: &str) -> Option<Bounds> {
    let field = slot.rsplit('.').next().unwrap_or(slot);
    let whole = |min: f64, max: Option<f64>| {
        Some(Bounds {
            min: Some(min),
            max,
            step: Some(1.0),
        })
    };
    let is_pattern = feature_type.ends_with("pattern") || feature_type == "mirror";
    match (feature_type, field) {
        // Elements of a pattern, the original included (`pattern.rs`,
        // `quantity`: 1 to 10 000).
        (_, "quantity") if is_pattern => whole(1.0, Some(10_000.0)),
        (_, "scale_copies" | "copies") if is_pattern => whole(1.0, Some(10_000.0)),
        // A gear's teeth (`gear_profile.rs`: at least 5, whole), pressure
        // angle (10 to 35 degrees) and profile shift (-1 to 0.8).
        ("gear", "teeth") if slot == "teeth" => whole(5.0, None),
        ("gear", "pressure_angle") => Some(Bounds {
            min: Some(10f64.to_radians()),
            max: Some(35f64.to_radians()),
            step: None,
        }),
        ("gear", "profile_shift") => Some(Bounds {
            min: Some(-1.0),
            max: Some(0.8),
            step: None,
        }),
        // A mortise and tenon's tenons (`mortise_tenon.rs`, `MAX_TENONS`).
        ("mortise_tenon", "count") if slot == "count" => whole(1.0, Some(100.0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_stay_stable_when_parameters_are_removed_or_renamed() {
        let mut params = Parameters::default();
        let d1 = params
            .add_dimension("60", Unit::MM, "Sketch1 width", Some(FeatureUid(1)))
            .unwrap();
        let d2 = params.add_dimension("40", Unit::MM, "", None).unwrap();
        params.remove(d1).unwrap();
        // The freed name is reused, the id is not.
        let d1_again = params.add_dimension("5", Unit::MM, "", None).unwrap();
        assert_ne!(d1_again, d1);
        assert_eq!(params.name(d1_again), "d1");
        assert_eq!(params.value(d2), Some(40.0));
        params.rename(d2, "height").unwrap();
        assert_eq!(params.find("height"), Some(d2));
        assert_eq!(params.value(d1), None);
        assert_eq!(params.owner(d2), None);
        assert_eq!(params.owner(d1_again), None);
    }

    #[test]
    fn expressions_follow_their_references() {
        let mut params = Parameters::default();
        let d1 = params
            .add_dimension("60", Unit::MM, "", Some(FeatureUid(1)))
            .unwrap();
        let d2 = params
            .add_dimension("d1 * 2 + 5 mm", Unit::MM, "", Some(FeatureUid(1)))
            .unwrap();
        assert_eq!(params.value(d2), Some(125.0));
        let changed = params.set_value(d1, 10.0).unwrap();
        assert_eq!(changed.into_iter().collect::<Vec<_>>(), vec![d1, d2]);
        assert_eq!(params.value(d2), Some(25.0));
        assert_eq!(params.get(d1).unwrap().expression(), "10 mm");
        // Renaming rewrites the reference; removing a referenced one fails.
        params.rename(d1, "width").unwrap();
        assert_eq!(params.get(d2).unwrap().expression(), "width * 2 + 5 mm");
        assert!(matches!(
            params.remove(d1),
            Err(ParameterError::Table(TableError::InUse { .. }))
        ));
        // A cycle is refused and changes nothing.
        let error = params.set_expression(d1, "d2 / 2").unwrap_err();
        assert_eq!(
            error.to_string(),
            "circular reference: width -> d2 -> width"
        );
        assert_eq!(params.value(d1), Some(10.0));
        assert!(params.set_value(d1, f64::NAN).is_err());
    }

    #[test]
    fn releasing_removes_unused_dimensions_in_order() {
        let mut params = Parameters::default();
        let owner = FeatureUid(3);
        let a = params
            .add_dimension("10", Unit::MM, "", Some(owner))
            .unwrap();
        let b = params
            .add_dimension("d1 * 2", Unit::MM, "", Some(owner))
            .unwrap();
        let c = params
            .add_dimension("5", Unit::MM, "", Some(owner))
            .unwrap();
        // c is still used by a feature; a only by b, which goes first.
        params.release(owner, false, &|id| id == c);
        assert!(params.get(a).is_none() && params.get(b).is_none());
        assert_eq!(params.owner(c), None);
        assert_eq!(params.get(c).unwrap().kind(), ParamKind::User);
    }

    #[test]
    fn slots_ending_in_angle_hold_angles() {
        assert_eq!(slot_unit("size.angle"), Unit::DEG);
        assert_eq!(slot_unit("dimensions[k3].angle"), Unit::DEG);
        assert_eq!(slot_unit("extent.distance"), Unit::MM);
        assert!(fits_slot(Unit::NONE, Unit::MM));
        assert!(fits_slot(Unit::IN, Unit::MM));
        assert!(!fits_slot(Unit::DEG, Unit::MM));
    }

    #[test]
    fn text_slots_take_text_parameters_only() {
        // mitcad#664.
        assert_eq!(slot_unit("texts[t5].text"), Unit::TEXT);
        assert_eq!(slot_unit("text"), Unit::MM);
        assert!(fits_slot(Unit::TEXT, Unit::TEXT));
        assert!(!fits_slot(Unit::TEXT, Unit::NONE));
        assert!(!fits_slot(Unit::TEXT, Unit::MM));
        assert!(!fits_slot(Unit::NONE, Unit::TEXT));
        assert!(!fits_slot(Unit::MM, Unit::TEXT));
        assert_eq!(describe(Unit::TEXT), "a text");
        let mut params = Parameters::default();
        let label = params
            .add(ParamSpec::user("Label", "'6qt'", Unit::TEXT), None)
            .unwrap();
        let width = params.add_dimension("10", Unit::MM, "", None).unwrap();
        assert_eq!(params.text(label), Some("6qt"));
        assert_eq!(params.text(width), None);
        assert_ne!(params.read_bits(label), params.read_bits(width));
        assert_eq!(params.read_bits(width), Some(10f64.to_bits()));
        let before = params.read_bits(label);
        params.set_expression(label, "'8qt'").unwrap();
        assert_ne!(params.read_bits(label), before);
        assert!(expression_is_text(&params, "Label & 'x'"));
        assert!(!expression_is_text(&params, "d1 * 2"));
    }

    #[test]
    fn counts_factors_and_fractions_are_unitless() {
        // mitcad#662.
        for slot in [
            "quantity",
            "direction1.quantity",
            "direction2.quantity",
            "scale",
            "scale_groups",
            "scale_copies",
            "scales[1].scale",
            "scales[1].groups",
            "scales[1].copies",
            "scale.factor",
            "scale.x",
            "scale.z",
            "ribs",
            "definition.fraction",
            "extent.fraction",
            "teeth",
            "set_screws.count",
            "count",
        ] {
            assert!(slot_unit(slot).is_unitless(), "{slot}");
        }
        for slot in ["definition.distance", "direction1.distance", "rib_length"] {
            assert_eq!(slot_unit(slot), Unit::MM, "{slot}");
        }
        assert_eq!(
            count_error("2.5 teeth".to_owned(), 2.5),
            "2.5 teeth: use floor(…) or round(…) for a whole number"
        );
        assert_eq!(count_error("0 teeth".to_owned(), 0.0), "0 teeth");
    }
}
