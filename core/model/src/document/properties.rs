// SPDX-License-Identifier: MIT
//! Typed property sets on components, occurrences and bodies (mitcad#496,
//! [`crate::properties`]): the commands that set and remove properties
//! and define own types (each an undo step that recomputes nothing), what
//! the document works out when asked (computed values, a library part's
//! data; never stored, so a recompute leaves the design file as it was),
//! and the answers of the `property_sets`, `property_types` and
//! `property_search` queries.
//!
//! An occurrence shows its component's properties as inherited; a value
//! set on the occurrence overrides the component's for that occurrence
//! only.

use serde_json::{Value, json};

use super::{DocState, Document, ModelError, PartsListRow, invalid};
use crate::ids::ComponentUid;
use crate::kernel::Kernel;
use crate::properties::{
    BaseType, COMPUTED_SET, DisplayUnit, LIBRARY_SET, Property, PropertyFilter, PropertyOwner,
    PropertyStore, PropertyType, PropertyValue, Source, check_key, check_set_name, format_value,
    parse_value, same_name,
};

/// The keys the document works out (in [`COMPUTED_SET`]).
const COMPUTED_KEYS: [&str; 5] = ["Volume", "Area", "Mass", "Length", "Material"];

/// A property as a level shows it: stored, inherited from the component
/// or worked out.
#[derive(Debug, Clone)]
pub(crate) struct ShownProperty {
    pub set: String,
    pub property: Property,
    /// Computed: it cannot be set.
    pub read_only: bool,
    /// The component's, shown on its occurrence.
    pub inherited: bool,
    /// The occurrence's own value over its component's.
    pub overrides: bool,
    /// The value a library gives where a stored one overrides it.
    pub library: Option<PropertyValue>,
}

/// A change of an own type: what is given changes.
#[derive(Debug, Clone, Default)]
pub struct PropertyTypeChange {
    pub new_name: Option<String>,
    pub base: Option<BaseType>,
    pub values: Option<Vec<Value>>,
    /// `Some(None)` removes the unit.
    pub unit: Option<Option<String>>,
    /// `Some(None)` removes the default.
    pub default: Option<Option<Value>>,
    pub description: Option<String>,
}

impl DocState {
    /// Copies the properties of copied components, occurrences and the
    /// bodies of copied features from `source` (a component's copy, a
    /// design inserted as a component), with the own types they use that
    /// this design does not define.
    pub(crate) fn copy_properties(
        &mut self,
        source: &DocState,
        components: &std::collections::BTreeMap<ComponentUid, ComponentUid>,
        occurrences: &std::collections::HashMap<
            crate::ids::OccurrenceUid,
            crate::ids::OccurrenceUid,
        >,
        features: &std::collections::HashMap<crate::ids::FeatureUid, crate::ids::FeatureUid>,
    ) {
        for (owner, sets) in &source.properties.owners {
            let target = match owner {
                PropertyOwner::Component(c) => {
                    components.get(c).map(|c| PropertyOwner::Component(*c))
                }
                PropertyOwner::Occurrence(o) => {
                    occurrences.get(o).map(|o| PropertyOwner::Occurrence(*o))
                }
                PropertyOwner::Body(b) => features
                    .get(&b.feature)
                    .map(|f| PropertyOwner::Body(crate::ids::BodyUid::new(*f, b.index))),
            };
            let Some(target) = target else {
                continue;
            };
            for group in sets.iter() {
                for property in &group.properties {
                    if BaseType::parse(&property.ty).is_none()
                        && self.properties.own_type(&property.ty).is_none()
                        && let Some(ty) = source.properties.own_type(&property.ty)
                    {
                        self.properties.types.push(ty.clone());
                    }
                    let mut property = property.clone();
                    // A print profile chosen comes with it (mitcad#640).
                    if matches!(
                        crate::print_profile::print_set_of(&group.name),
                        Some(crate::print_profile::PrintPropertySet::Layer(_))
                    ) && same_name(&property.key, crate::print_profile::PROFILE_KEY)
                        && let PropertyValue::Text(id) = &property.value
                        && let Some(mine) = self.copy_print_profile(source, id)
                    {
                        property.value = PropertyValue::Text(mine);
                    }
                    self.properties.put(target, &group.name, property);
                }
            }
        }
        self.sync_profile_types();
    }

    /// Whether a property owner is part of the definition: a component or
    /// occurrence that exists, a body of a feature on the timeline. The
    /// design file keeps only these owners' properties.
    pub(crate) fn property_owner_kept(&self, owner: PropertyOwner) -> bool {
        match owner {
            PropertyOwner::Component(c) => self.assembly.exists(c),
            PropertyOwner::Occurrence(o) => self.assembly.occurrence(o).is_some(),
            // (By the features' index: the design file and the importers
            // ask for every body, mitcad#841.)
            PropertyOwner::Body(b) => self.position(b.feature).is_some(),
        }
    }
}

impl<K: Kernel> Document<K> {
    /// The design's own property types and stored properties.
    pub fn property_store(&self) -> &PropertyStore {
        &self.state.properties
    }

    /// Why an owner cannot take properties, if it cannot.
    fn check_property_owner(&self, owner: PropertyOwner) -> Result<(), ModelError> {
        let there = self.state.property_owner_kept(owner)
            && match owner {
                PropertyOwner::Body(b) => {
                    self.body_shape(b).is_some() || self.state.body_names.contains_key(&b)
                }
                _ => true,
            };
        if there {
            Ok(())
        } else {
            Err(invalid(format!(
                "{} {owner} does not exist",
                owner.level().as_str()
            )))
        }
    }

    /// The unit a type's quantities are shown and typed in.
    fn property_unit(&self, ty: &str) -> Option<DisplayUnit> {
        let store = &self.state.properties;
        let base = store.base_of(ty).ok()?;
        let own = store.own_type(ty).and_then(|t| t.unit.as_deref());
        DisplayUnit::of(base, own, &self.parameters().context())
            .ok()
            .flatten()
    }

    /// A value given for a type: in `unit` when one is given (a quantity's
    /// number or text without its own unit), else as [`parse_value`] reads
    /// it.
    fn property_value_of(
        &self,
        ty: &str,
        input: &Value,
        unit: Option<&str>,
    ) -> Result<PropertyValue, ModelError> {
        let store = &self.state.properties;
        let base = store.base_of(ty).map_err(invalid)?;
        let context = self.parameters().context();
        let shown = match unit.filter(|_| base.is_quantity()) {
            Some(unit) => Some(DisplayUnit::parse(base, unit).map_err(invalid)?),
            None => self.property_unit(ty),
        };
        let value = match (input, unit, shown) {
            (Value::Number(n), Some(_), Some(shown)) if base.is_quantity() => {
                let n = n
                    .as_f64()
                    .ok_or_else(|| invalid(format!("{input} is not a number")))?;
                PropertyValue::Number(shown.to_canonical(n))
            }
            _ => parse_value(base, input, shown, &context).map_err(invalid)?,
        };
        if let Some(own) = store.own_type(ty)
            && !own.allows(&value)
        {
            let allowed: Vec<String> = own
                .values
                .iter()
                .map(|v| format_value(base, v, shown, own.unit.as_deref()))
                .collect();
            return Err(invalid(format!(
                "{} allows {}; not {}",
                own.name,
                allowed.join(", "),
                format_value(base, &value, shown, own.unit.as_deref())
            )));
        }
        Ok(value)
    }

    /// Sets a property (the `set_property` command): its type the one
    /// given, else the property's, else what the value is (text, integer,
    /// number, yes/no); its value the one given, else the own type's
    /// default. A user's value over an imported one keeps the imported
    /// value, so the property shows that it was changed. One undo step.
    #[allow(clippy::too_many_arguments)]
    pub fn set_property(
        &mut self,
        owner: PropertyOwner,
        set: &str,
        key: &str,
        ty: Option<&str>,
        value: Option<&Value>,
        unit: Option<&str>,
        source: Source,
    ) -> Result<Property, ModelError> {
        self.check_property_owner(owner)?;
        check_set_name(set).map_err(invalid)?;
        check_key(key).map_err(invalid)?;
        // The print sets keep the key map's keys and types (mitcad#640).
        if let Some(print) = crate::print_profile::print_set_of(set) {
            return self.set_print_property(owner, print, key, value, source);
        }
        // An occurrence's own value over its component's takes the
        // component's spelling of the set and key.
        let (set, key) = match owner {
            PropertyOwner::Occurrence(o) => self
                .assembly()
                .occurrence(o)
                .map(|o| PropertyOwner::Component(o.component))
                .and_then(|c| {
                    let group = self
                        .state
                        .properties
                        .sets(c)
                        .iter()
                        .find(|g| same_name(&g.name, set))?;
                    let property = group.properties.iter().find(|p| same_name(&p.key, key))?;
                    Some((group.name.clone(), property.key.clone()))
                })
                .unwrap_or_else(|| (set.trim().to_owned(), key.trim().to_owned())),
            _ => (set.trim().to_owned(), key.trim().to_owned()),
        };
        let (set, key) = (set.as_str(), key.as_str());
        if source == Source::Computed {
            return Err(invalid("computed values are not stored"));
        }
        let store = &self.state.properties;
        let existing = store.get(owner, set, key).cloned();
        let ty = match (ty, &existing, value) {
            (Some(ty), _, _) => {
                // A standard type by its name, an own type by its spelling.
                match store.own_type(ty.trim()) {
                    Some(own) => own.name.clone(),
                    None => store
                        .base_of(ty.trim())
                        .map_err(invalid)?
                        .as_str()
                        .to_owned(),
                }
            }
            (None, Some(existing), _) => existing.ty.clone(),
            (None, None, Some(value)) => match value {
                Value::String(_) => "text",
                Value::Bool(_) => "boolean",
                Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
                Value::Number(_) => "number",
                _ => return Err(invalid(format!("{value} is no property value"))),
            }
            .to_owned(),
            (None, None, None) => {
                return Err(invalid(format!("{key} needs a type or a value")));
            }
        };
        let base = store.base_of(&ty).map_err(invalid)?;
        let value = match value {
            Some(value) => self.property_value_of(&ty, value, unit)?,
            None => match (
                store.own_type(&ty).and_then(|t| t.default.clone()),
                &existing,
            ) {
                (Some(default), _) => default,
                (None, Some(existing)) if same_name(&existing.ty, &ty) => existing.value.clone(),
                _ => return Err(invalid(format!("{key} needs a value"))),
            },
        };
        // A plain number's label: the one given, else the property's.
        let label = if base == BaseType::Number {
            unit.map(str::to_owned)
                .or_else(|| existing.as_ref().and_then(|e| e.unit.clone()))
        } else {
            None
        };
        let mut property = Property::new(key, base, value, source).with_unit(label);
        property.ty = ty;
        if let Some(existing) = &existing
            && source == Source::User
            && existing.source != Source::User
        {
            // The imported value stays beside the user's.
            let imported = existing
                .imported
                .clone()
                .unwrap_or_else(|| existing.value.clone());
            property.source = existing.source;
            property.imported = (!imported.same(&property.value)).then_some(imported);
        }
        let label = format!("Set {key}");
        let stored = property.clone();
        let set = set.to_owned();
        self.apply_with(|state| {
            state.properties.put(owner, &set, property);
            Ok((label, (), false))
        })?;
        Ok(stored)
    }

    /// Removes a property (the `remove_property` command). One undo step.
    pub fn remove_property(
        &mut self,
        owner: PropertyOwner,
        set: &str,
        key: &str,
    ) -> Result<(), ModelError> {
        if self.state.properties.get(owner, set, key).is_none() {
            return Err(invalid(format!("{owner} has no property {set} / {key}")));
        }
        let label = format!("Remove {key}");
        self.apply_with(|state| {
            state.properties.remove(owner, set, key);
            Ok((label, (), false))
        })
    }

    /// Adds properties an importer read (`rvt`, `ifc`, `ipt`, `f3d`), each
    /// of a standard type or an own type the store has, as one undo step
    /// named `label`; an importer merges it into its own. Properties that
    /// do not fit (an owner that is not there, a value not of its type)
    /// are left out: the count of those added. Nothing is added, and no
    /// step made, when none fits.
    pub fn import_properties(
        &mut self,
        label: &str,
        items: Vec<(PropertyOwner, String, Property)>,
    ) -> usize {
        let store = &self.state.properties;
        let mut kept = Vec::with_capacity(items.len());
        for (owner, set, mut property) in items {
            if !self.state.property_owner_kept(owner)
                || check_set_name(&set).is_err()
                || check_key(&property.key).is_err()
                || property.source == Source::Computed
            {
                continue;
            }
            let Ok(base) = store.base_of(&property.ty) else {
                continue;
            };
            let Ok(value) = property.value.clone().conform(base) else {
                continue;
            };
            property.value = value;
            kept.push((owner, set, property));
        }
        let count = kept.len();
        if count == 0 {
            return 0;
        }
        let label = label.to_owned();
        let applied = self.apply_with(|state| {
            for (owner, set, property) in kept {
                state.properties.put(owner, &set, property);
            }
            Ok((label, (), false))
        });
        if applied.is_ok() { count } else { 0 }
    }

    /// A type from its fields as a command gives them: values and default
    /// read as the base type's (in the type's unit).
    #[allow(clippy::too_many_arguments)]
    fn property_type_of(
        &self,
        name: &str,
        base: BaseType,
        values: &[Value],
        unit: Option<&str>,
        default: Option<&Value>,
        description: &str,
    ) -> Result<PropertyType, ModelError> {
        let context = self.parameters().context();
        let unit = unit.map(str::trim).filter(|u| !u.is_empty());
        let shown = DisplayUnit::of(base, unit, &context).map_err(invalid)?;
        let read = |v: &Value| parse_value(base, v, shown, &context).map_err(invalid);
        let ty = PropertyType {
            name: name.trim().to_owned(),
            base,
            values: values.iter().map(read).collect::<Result<_, _>>()?,
            unit: unit.map(str::to_owned),
            default: default.map(read).transpose()?,
            description: description.trim().to_owned(),
        };
        ty.check().map_err(invalid)?;
        ty.conformed().map_err(invalid)
    }

    /// Defines an own type (the `define_property_type` command). One undo
    /// step.
    pub fn define_property_type(
        &mut self,
        name: &str,
        base: BaseType,
        values: &[Value],
        unit: Option<&str>,
        default: Option<&Value>,
        description: &str,
    ) -> Result<PropertyType, ModelError> {
        let ty = self.property_type_of(name, base, values, unit, default, description)?;
        if self.state.properties.own_type(&ty.name).is_some() {
            return Err(invalid(format!("a property type {} exists", ty.name)));
        }
        let label = format!("Define {}", ty.name);
        let made = ty.clone();
        self.apply_with(|state| {
            state.properties.types.push(ty);
            Ok((label, (), false))
        })?;
        Ok(made)
    }

    /// Changes an own type (the `edit_property_type` command): a new name
    /// renames the properties of the type too; new values must allow every
    /// value its properties have; the base changes only while no property
    /// uses the type. One undo step.
    pub fn edit_property_type(
        &mut self,
        name: &str,
        change: &PropertyTypeChange,
    ) -> Result<PropertyType, ModelError> {
        let store = &self.state.properties;
        let old = store
            .own_type(name)
            .cloned()
            .ok_or_else(|| invalid(format!("there is no own property type {name}")))?;
        let uses = store.uses_of(&old.name);
        let base = change.base.unwrap_or(old.base);
        if base != old.base && !uses.is_empty() {
            return Err(invalid(format!(
                "{} is used by {} properties; its base type cannot change",
                old.name,
                uses.len()
            )));
        }
        let unit = match &change.unit {
            Some(unit) => unit.clone(),
            None if base == old.base => old.unit.clone(),
            None => None,
        };
        // Unchanged values and default as they are kept (canonical numbers).
        let kept = |v: &PropertyValue| v.to_json();
        let values: Vec<Value> = match &change.values {
            Some(values) => values.clone(),
            None => old.values.iter().map(kept).collect(),
        };
        let default = match &change.default {
            Some(default) => default.clone(),
            None => old.default.as_ref().map(kept),
        };
        let new_name = change.new_name.as_deref().unwrap_or(&old.name);
        let description = change.description.as_deref().unwrap_or(&old.description);
        let ty = self.property_type_of(
            new_name,
            base,
            &values,
            unit.as_deref(),
            default.as_ref(),
            description,
        )?;
        if !same_name(&ty.name, &old.name) && store.own_type(&ty.name).is_some() {
            return Err(invalid(format!("a property type {} exists", ty.name)));
        }
        for (owner, set, key) in &uses {
            let property = store.get(*owner, set, key).expect("listed by uses_of");
            if !ty.allows(&property.value) {
                return Err(invalid(format!(
                    "{owner} {set} / {key} has the value {}, which {} would not allow",
                    format_value(base, &property.value, None, None),
                    ty.name
                )));
            }
        }
        let label = format!("Edit {}", old.name);
        let changed = ty.clone();
        self.apply_with(|state| {
            let store = &mut state.properties;
            if let Some(slot) = store
                .types
                .iter_mut()
                .find(|t| same_name(&t.name, &old.name))
            {
                *slot = ty.clone();
            }
            if ty.name != old.name {
                let renamed = ty.name.clone();
                store.change_type_uses(&old.name, |p| p.ty = renamed.clone());
            }
            Ok((label, (), false))
        })?;
        Ok(changed)
    }

    /// Removes an own type (the `remove_property_type` command): its
    /// properties keep their values under its base type. The number of
    /// them. One undo step.
    pub fn remove_property_type(&mut self, name: &str) -> Result<usize, ModelError> {
        let store = &self.state.properties;
        let ty = store
            .own_type(name)
            .cloned()
            .ok_or_else(|| invalid(format!("there is no own property type {name}")))?;
        let count = store.uses_of(&ty.name).len();
        let label = format!("Remove {}", ty.name);
        self.apply_with(|state| {
            let store = &mut state.properties;
            store.types.retain(|t| !same_name(&t.name, &ty.name));
            let base = ty.base.as_str().to_owned();
            let unit = ty.unit.clone();
            store.change_type_uses(&ty.name, |p| {
                p.ty = base.clone();
                // A number keeps the label its type gave it.
                if ty.base == BaseType::Number && p.unit.is_none() {
                    p.unit = unit.clone();
                }
            });
            Ok((label, (), false))
        })?;
        Ok(count)
    }

    // What the document works out.

    /// The values the document works out for an owner (never stored): a
    /// body's volume, area, mass and material, a component's volume and
    /// mass (its own bodies) and a BUILD member's length, when `computed`;
    /// a component's library data always (cheap). `row`: the component's
    /// parts list row, when the caller has it.
    fn derived_properties(
        &self,
        owner: PropertyOwner,
        computed: bool,
        row: Option<&PartsListRow>,
    ) -> Vec<ShownProperty> {
        let mut out = Vec::new();
        let mut push =
            |set: &str, key: &str, base: BaseType, value: PropertyValue, source: Source| {
                out.push(ShownProperty {
                    set: set.to_owned(),
                    property: Property::new(key, base, value, source),
                    read_only: source == Source::Computed,
                    inherited: false,
                    overrides: false,
                    library: None,
                });
            };
        let number = PropertyValue::Number;
        match owner {
            PropertyOwner::Body(b) if computed => {
                let Some(shape) = self.body_shape(b) else {
                    return out;
                };
                let material = self.body_material(b, None).0;
                if let Ok(mass) = self.kernel().mass_properties(shape) {
                    let c = Source::Computed;
                    push(
                        COMPUTED_SET,
                        "Volume",
                        BaseType::Volume,
                        number(mass.volume),
                        c,
                    );
                    push(COMPUTED_SET, "Area", BaseType::Area, number(mass.area), c);
                    let kg = mass.volume * material.density * 1e-6;
                    push(COMPUTED_SET, "Mass", BaseType::Mass, number(kg), c);
                }
                push(
                    COMPUTED_SET,
                    "Material",
                    BaseType::Text,
                    PropertyValue::Text(material.name.to_owned()),
                    Source::Computed,
                );
            }
            PropertyOwner::Component(c) => {
                self.component_library_values(c, row, &mut push);
                if !computed {
                    return out;
                }
                let mut volume = 0.0;
                let mut kg = 0.0;
                let mut measured = 0;
                for body in self.bodies() {
                    if self.body_component(body.uid) != Some(c) {
                        continue;
                    }
                    let Ok(mass) = self.kernel().mass_properties(body.shape) else {
                        continue;
                    };
                    let density = self.body_material(body.uid, None).0.density;
                    volume += mass.volume;
                    kg += mass.volume * density * 1e-6;
                    measured += 1;
                }
                if measured > 0 {
                    let s = Source::Computed;
                    push(COMPUTED_SET, "Volume", BaseType::Volume, number(volume), s);
                    push(COMPUTED_SET, "Mass", BaseType::Mass, number(kg), s);
                }
                if let Some(length) = self.part_info(c).and_then(|info| info.length) {
                    let s = Source::Computed;
                    push(
                        COMPUTED_SET,
                        "Length",
                        BaseType::Length,
                        number(length.1),
                        s,
                    );
                }
            }
            _ => {}
        }
        out
    }

    /// A component's library data: a library or BUILD part's designation,
    /// supplier, part number, library and version, and a BUILD part's
    /// material and grade.
    fn component_library_values(
        &self,
        component: ComponentUid,
        row: Option<&PartsListRow>,
        push: &mut impl FnMut(&str, &str, BaseType, PropertyValue, Source),
    ) {
        if component.is_root() {
            return;
        }
        let looked_up;
        let row = match row {
            Some(row) => Some(row),
            None => {
                looked_up = self
                    .parts_list()
                    .into_iter()
                    .find(|r| r.component == component);
                looked_up.as_ref()
            }
        };
        let info = self.part_info(component);
        let Some(row) = row.filter(|r| r.library.is_some() || info.is_some()) else {
            return;
        };
        let mut text = |key: &str, value: &str| {
            if !value.trim().is_empty() {
                push(
                    LIBRARY_SET,
                    key,
                    BaseType::Text,
                    PropertyValue::Text(value.trim().to_owned()),
                    Source::Library,
                );
            }
        };
        text("Designation", row.designation.as_str());
        text("Supplier", row.supplier.as_str());
        text("Part number", row.part_number.as_str());
        if let Some(library) = &row.library {
            text("Library", library.library.as_str());
            text("Version", library.version_text().as_str());
        }
        if let Some(info) = &info
            && let Some(material) = &info.meta.material
        {
            let name = if material.species.is_empty() {
                &material.product
            } else {
                &material.species
            };
            text("Material", name.as_str());
            text(
                "Grade",
                info.record
                    .grade
                    .as_deref()
                    .unwrap_or(material.grade.as_str()),
            );
            text("Finish", material.finish.as_str());
        }
    }

    /// What one level shows: stored properties, then the ones worked out
    /// that no stored one of the same set and key overrides.
    fn level_properties(
        &self,
        owner: PropertyOwner,
        computed: bool,
        row: Option<&PartsListRow>,
    ) -> Vec<ShownProperty> {
        let mut shown: Vec<ShownProperty> = Vec::new();
        for group in self.state.properties.sets(owner) {
            for property in &group.properties {
                shown.push(ShownProperty {
                    set: group.name.clone(),
                    property: property.clone(),
                    read_only: false,
                    inherited: false,
                    overrides: false,
                    library: None,
                });
            }
        }
        for derived in self.derived_properties(owner, computed, row) {
            let stored = shown.iter().position(|s| {
                same_name(&s.set, &derived.set) && same_name(&s.property.key, &derived.property.key)
            });
            match stored {
                Some(i) => {
                    if !shown[i].property.value.same(&derived.property.value) {
                        shown[i].library = Some(derived.property.value);
                    }
                }
                None => shown.push(derived),
            }
        }
        shown
    }

    /// An occurrence's properties: its component's as inherited, its own
    /// over them.
    fn occurrence_properties(
        &self,
        occurrence: crate::ids::OccurrenceUid,
        computed: bool,
    ) -> Vec<ShownProperty> {
        let Some(component) = self.assembly().occurrence(occurrence).map(|o| o.component) else {
            return Vec::new();
        };
        let mut shown: Vec<ShownProperty> = self
            .level_properties(PropertyOwner::Component(component), computed, None)
            .into_iter()
            .map(|mut s| {
                s.inherited = true;
                s
            })
            .collect();
        for own in self.level_properties(PropertyOwner::Occurrence(occurrence), false, None) {
            let inherited = shown.iter().position(|s| {
                !s.read_only
                    && same_name(&s.set, &own.set)
                    && same_name(&s.property.key, &own.property.key)
            });
            match inherited {
                Some(i) => {
                    shown[i] = ShownProperty {
                        overrides: true,
                        ..own
                    };
                }
                None => shown.push(own),
            }
        }
        shown
    }

    /// The properties shown for an owner on its level.
    pub(crate) fn shown_properties(
        &self,
        owner: PropertyOwner,
        computed: bool,
    ) -> Vec<ShownProperty> {
        match owner {
            PropertyOwner::Occurrence(o) => self.occurrence_properties(o, computed),
            _ => self.level_properties(owner, computed, None),
        }
    }

    /// A shown property as the queries give it.
    pub(crate) fn property_json(&self, shown: &ShownProperty) -> Value {
        let store = &self.state.properties;
        let p = &shown.property;
        let base = store.base_of(&p.ty).unwrap_or(BaseType::Text);
        let own = store.own_type(&p.ty);
        let unit = self.property_unit(&p.ty);
        let label = p
            .unit
            .as_deref()
            .or_else(|| own.and_then(|t| t.unit.as_deref()));
        let text = |v: &PropertyValue| format_value(base, v, unit, label);
        let mut value = json!({
            "set": shown.set,
            "key": p.key,
            "type": p.ty,
            "base": base.as_str(),
            "value": p.value.to_json(),
            "text": text(&p.value),
            "source": p.source.as_str(),
            "read_only": shown.read_only,
            "inherited": shown.inherited,
            "overrides": shown.overrides,
            "edited": p.edited() || shown.library.is_some(),
        });
        match unit {
            Some(unit) => value["unit"] = json!(unit.symbol()),
            None => {
                if let Some(label) = label.filter(|_| base == BaseType::Number) {
                    value["unit"] = json!(label);
                }
            }
        }
        // What the source gave, where the value differs from it.
        if let Some(imported) = &p.imported {
            value["source_value"] = json!(text(imported));
        } else if let Some(library) = &shown.library {
            value["source_value"] = json!(text(library));
            value["source_of_value"] = json!(Source::Library.as_str());
        }
        if let Some(own) = own.filter(|t| !t.values.is_empty()) {
            value["choices"] = json!(own.values.iter().map(text).collect::<Vec<_>>());
        }
        // A print setting's plain name (mitcad#640); the key is its tooltip.
        if let Some(name) = super::print_key_name(&shown.set, &p.key) {
            value["name"] = json!(name);
        }
        value
    }

    /// One level of the `property_sets` query: the owner, its name and its
    /// sets with their properties.
    pub fn property_level_json(&self, owner: PropertyOwner, computed: bool) -> Value {
        let name = match owner {
            PropertyOwner::Component(c) => self.assembly().name(c),
            PropertyOwner::Occurrence(o) => self.assembly().occurrence_name(o),
            PropertyOwner::Body(b) => self.body_name(b),
        };
        let mut sets: Vec<(String, Vec<Value>)> = Vec::new();
        for shown in self.shown_properties(owner, computed) {
            let value = self.property_json(&shown);
            match sets
                .iter_mut()
                .find(|(name, _)| same_name(name, &shown.set))
            {
                Some((_, properties)) => properties.push(value),
                None => sets.push((shown.set.clone(), vec![value])),
            }
        }
        let sets: Vec<Value> = sets
            .into_iter()
            .map(|(name, properties)| json!({"name": name, "properties": properties}))
            .collect();
        let mut value = json!({
            "level": owner.level().as_str(),
            "owner": owner,
            "name": name,
            "sets": sets,
        });
        if let PropertyOwner::Occurrence(o) = owner
            && let Some(occurrence) = self.assembly().occurrence(o)
        {
            value["component"] = json!(occurrence.component);
        }
        value
    }

    /// The `property_types` query: the standard types and the design's
    /// own, each own type with its values as numbers and text and how many
    /// properties use it.
    pub fn property_types_json(&self) -> Value {
        let store = &self.state.properties;
        let standard: Vec<&str> = BaseType::ALL.iter().map(|t| t.as_str()).collect();
        let types: Vec<Value> = store
            .types
            .iter()
            .map(|t| {
                let unit = self.property_unit(&t.name);
                let label = t.unit.as_deref();
                let text = |v: &PropertyValue| format_value(t.base, v, unit, label);
                let values: Vec<Value> = t
                    .values
                    .iter()
                    .map(|v| json!({"value": v.to_json(), "text": text(v)}))
                    .collect();
                json!({
                    "name": t.name,
                    "base": t.base.as_str(),
                    "values": values,
                    "unit": t.unit,
                    "default": t.default.as_ref().map(|v| json!({"value": v.to_json(), "text": text(v)})),
                    "description": t.description,
                    "uses": store.uses_of(&t.name).len(),
                })
            })
            .collect();
        json!({"standard": standard, "types": types})
    }

    /// The `property_search` query: every component, occurrence (with its
    /// inherited values) and body at the marker that has a property the
    /// search matches (`FireRating = EI60`, `Pset_WallCommon/FireRating`,
    /// `Mass > 10 kg`); computed values only when the search names one.
    pub fn property_search_json(&self, text: &str) -> Result<Value, String> {
        let filter = PropertyFilter::parse(text)?;
        let computed = filter
            .set
            .as_deref()
            .is_none_or(|s| same_name(s, COMPUTED_SET))
            && COMPUTED_KEYS.iter().any(|k| same_name(k, &filter.key));
        let context = self.parameters().context();
        let a = self.assembly();
        let mut owners: Vec<PropertyOwner> = vec![PropertyOwner::Component(ComponentUid::ROOT)];
        owners.extend(a.components.iter().map(|c| PropertyOwner::Component(c.uid)));
        owners.extend(
            a.occurrences
                .iter()
                .map(|o| PropertyOwner::Occurrence(o.uid)),
        );
        owners.extend(self.bodies().iter().map(|b| PropertyOwner::Body(b.uid)));
        let store = &self.state.properties;
        let mut matches = Vec::new();
        for owner in owners {
            for shown in self.shown_properties(owner, computed) {
                let p = &shown.property;
                if !filter.names(&shown.set, &p.key) {
                    continue;
                }
                let base = store.base_of(&p.ty).unwrap_or(BaseType::Text);
                let unit = self.property_unit(&p.ty);
                if !filter.matches(base, &p.value, unit, &context) {
                    continue;
                }
                let mut found = self.property_level_header(owner);
                found["set"] = json!(shown.set);
                found["key"] = json!(p.key);
                found["text"] = self.property_json(&shown)["text"].clone();
                found["inherited"] = json!(shown.inherited);
                matches.push(found);
                break;
            }
        }
        Ok(json!({"matches": matches}))
    }

    /// An owner's level, id, name and component (of an occurrence or body).
    fn property_level_header(&self, owner: PropertyOwner) -> Value {
        let a = self.assembly();
        let (name, component) = match owner {
            PropertyOwner::Component(c) => (a.name(c), None),
            PropertyOwner::Occurrence(o) => (
                a.occurrence_name(o),
                a.occurrence(o).map(|occurrence| occurrence.component),
            ),
            PropertyOwner::Body(b) => (self.body_name(b), self.body_component(b)),
        };
        let mut value = json!({"level": owner.level().as_str(), "owner": owner, "name": name});
        if let Some(component) = component {
            value["component"] = json!(component);
        }
        value
    }

    /// The text of a component's property for a parts list column: `key`
    /// is `Key` or `Set/Key`; stored, library and computed values alike.
    pub fn component_property_text(
        &self,
        component: ComponentUid,
        row: &PartsListRow,
        key: &str,
    ) -> Option<String> {
        self.component_property_of(component, Some(row), key)
    }

    /// A mass in kilograms as the design's properties write it (its mass
    /// unit), for a drawing's title block (mitcad#707 phase 4).
    pub fn mass_text(&self, kg: f64) -> String {
        format_value(
            BaseType::Mass,
            &PropertyValue::Number(kg),
            self.property_unit(BaseType::Mass.as_str()),
            None,
        )
    }

    /// The text of a component's property, `Key` or `Set/Key`: stored,
    /// library and computed values alike; its parts list row looked up
    /// when not given (a drawing's title block, mitcad#707 phase 4).
    pub fn component_property_of(
        &self,
        component: ComponentUid,
        row: Option<&PartsListRow>,
        key: &str,
    ) -> Option<String> {
        let (set, name) = set_and_key(key);
        let computed = set.is_none_or(|s| same_name(s, COMPUTED_SET))
            && COMPUTED_KEYS.iter().any(|k| same_name(k, name));
        self.component_level_text(component, row, key, computed)
    }

    /// The text of a component's own property, `Key` or `Set/Key`: stored
    /// or from its library, never one worked out (a drawing's title block
    /// honours a stored `Mass` and works the mass out itself otherwise,
    /// mitcad#722).
    pub fn component_stored_property(&self, component: ComponentUid, key: &str) -> Option<String> {
        self.component_level_text(component, None, key, false)
    }

    fn component_level_text(
        &self,
        component: ComponentUid,
        row: Option<&PartsListRow>,
        key: &str,
        computed: bool,
    ) -> Option<String> {
        let (set, name) = set_and_key(key);
        let owner = PropertyOwner::Component(component);
        self.level_properties(owner, computed, row)
            .iter()
            .find(|s| {
                same_name(&s.property.key, name) && set.is_none_or(|set| same_name(set, &s.set))
            })
            .map(|s| {
                self.property_json(s)["text"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned()
            })
    }
}

/// A property's `Set/Key` or `Key` split, trimmed.
fn set_and_key(key: &str) -> (Option<&str>, &str) {
    match key.rsplit_once('/') {
        Some((set, key)) => (Some(set.trim()), key.trim()),
        None => (None, key.trim()),
    }
}
