// SPDX-License-Identifier: MIT
//! The document's parameters, as the Change Parameters dialog shows them: an
//! expression table ([`ParameterTable`]: names, expressions with units,
//! comments, evaluation in dependency order, cycle and unit checks, renames
//! that rewrite references) and the feature that owns each model parameter.
//! Values are millimetres and radians.

use std::collections::BTreeMap;
use std::fmt;

use crate::expr::{
    AngleUnit, Dims, EvalContext, Expr, LengthUnit, ParamKind, ParamSpec, ParameterTable,
    TableError, Unit,
};
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
}

impl fmt::Display for ParameterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(name) => write!(f, "unknown parameter '{name}'"),
            Self::NotFinite { name, value } => {
                write!(f, "parameter '{name}' must be a finite number, got {value}")
            }
            Self::InUse { name, users } => {
                write!(f, "parameter '{name}' is used by {}", users.join(", "))
            }
            Self::Table(error) => error.fmt(f),
            Self::UnitInUse {
                name,
                unit,
                user,
                expected,
            } => write!(
                f,
                "parameter '{name}' cannot change to '{unit}': {user} uses it as {}",
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
    pub(crate) fn build(
        context: EvalContext,
        specs: Vec<(ParamSpec, Option<FeatureUid>)>,
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
            table: ParameterTable::build_with_ids(context, table_specs)?,
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

    /// Adds a parameter; a parameter with an owner is a model parameter.
    pub fn add(
        &mut self,
        mut spec: ParamSpec,
        owner: Option<FeatureUid>,
    ) -> Result<ParamId, ParameterError> {
        if owner.is_some() {
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
        self.require(id)?;
        Ok(self.table.set_expression(id, expression)?)
    }

    /// Sets the expression to a plain value in millimetres or radians,
    /// written in the parameter's unit.
    pub fn set_value(&mut self, id: ParamId, value: f64) -> Result<ChangeSet, ParameterError> {
        let name = self.require(id)?.name().to_owned();
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
        self.require(id)?;
        Ok(self.table.update(id, expression, unit)?)
    }

    pub fn rename(&mut self, id: ParamId, name: &str) -> Result<Vec<ParamId>, ParameterError> {
        self.require(id)?;
        Ok(self.table.rename(id, name)?)
    }

    pub fn set_comment(&mut self, id: ParamId, comment: &str) -> Result<(), ParameterError> {
        Ok(self.table.set_comment(id, comment)?)
    }

    /// Changes the document's default units; bare numbers in expressions
    /// are read in them.
    pub fn set_context(&mut self, context: EvalContext) -> Result<ChangeSet, ParameterError> {
        Ok(self.table.set_context(context)?)
    }

    pub(crate) fn set_owner(&mut self, id: ParamId, owner: Option<FeatureUid>) {
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
    pub(crate) fn remove(&mut self, id: ParamId) -> Result<Parameter, ParameterError> {
        let removed = self.table.remove(id)?;
        self.owners.remove(&id);
        Ok(removed)
    }

    /// Parameters `owner` owns, in creation order.
    pub(crate) fn owned_by(&self, owner: FeatureUid) -> Vec<ParamId> {
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
    pub(crate) fn release(
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
}

/// The unit of a feature value slot: slots whose name has `angle` in it
/// (`size.angle`, `extent.angle`, `extent.angle2`, `dimensions[k3].angle`)
/// or ends in `taper` (`extent.taper`, `side1.taper`) hold angles; counts
/// and ratios (`helix.revolutions`, `extent.fraction`, `start_condition.weight`,
/// `tangency_weight`) are unitless; everything else lengths.
pub fn slot_unit(slot: &str) -> Unit {
    let field = slot.rsplit('.').next().unwrap_or(slot);
    if field.contains("angle") || field.ends_with("taper") {
        Unit::DEG
    } else if field == "revolutions" || field.starts_with("fraction") || field.ends_with("weight") {
        Unit::NONE
    } else {
        Unit::MM
    }
}

/// Whether a parameter in `unit` can give a slot of `slot_unit` its value:
/// the same dimensions, or a unitless parameter (its number is used as is).
pub fn fits_slot(unit: Unit, slot_unit: Unit) -> bool {
    unit.is_unitless() || unit.dims() == slot_unit.dims()
}

/// What kind of value a unit measures, for messages.
pub fn describe(unit: Unit) -> &'static str {
    let dims = unit.dims();
    if dims == Dims::LENGTH {
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
pub(crate) fn expression_dims(
    parameters: &Parameters,
    expression: &str,
) -> Result<Dims, ParameterError> {
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

/// The default evaluation context of new documents: millimetres and
/// degrees.
pub fn default_context() -> EvalContext {
    EvalContext::new(LengthUnit::Millimetre, AngleUnit::Degree)
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
}
