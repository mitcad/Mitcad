// SPDX-License-Identifier: MIT
//! Print settings in the design (mitcad#640, part 3): the design's printer
//! and process profiles ([`crate::print_profile`]), the `Print` property
//! sets of components, occurrences and bodies, and what a body prints
//! with.
//!
//! **Inheritance.** A body's settings come from the root, then the
//! components down the occurrence path that places it (each occurrence
//! after its component), then the body itself; the nearest value wins. On
//! each level a value set there comes before the values of the profile
//! chosen there (`Print Printer / profile`, `Print Process / profile`), so
//! a component that chooses a profile prints with that profile's values
//! whatever the root set, unless the component sets a value itself. The
//! filament is the body's material's (`document/materials.rs`): its
//! filament values, its density as `filament_density` and the colour of
//! the appearance it prints with as `filament_colour` (mitcad#1055: the
//! body's appearance, one of its material's; a material keeps no colour
//! of its own).
//!
//! Without an occurrence path a body inherits from its own component and
//! the root, as its material does. Nothing else of #496 changes: the
//! `property_sets` query still shows an occurrence only its component's
//! values as inherited; `print_settings` gives the resolved values.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use super::library_items::profile_title;
use super::{DocState, Document, InstanceView, ModelError, invalid};
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::material::{DEFAULT_MATERIAL, Material, VariantMatch};
use crate::print_keys::{self, Dialect, KeyDef, PrintSet};
use crate::print_library::{BasedOn, ItemData, ItemKind};
use crate::print_profile::{
    PROFILE_KEY, PrintProfile, PrintPropertySet, ProfileChange, ProfileKind, PropertyForm,
    copy_name, free_id, from_property, new_item_name, print_set_of, property_form, slicer_set,
    slug, to_property, value_of,
};
use crate::properties::{
    BaseType, Property, PropertyOwner, PropertyType, PropertyValue, Source, same_name,
};

/// What print settings are asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum PrintTarget {
    /// A body, in the occurrence path that places it (None: its own
    /// component and the root).
    Body(BodyUid, Option<Vec<OccurrenceUid>>),
    /// An occurrence, by its path from the root.
    Occurrence(Vec<OccurrenceUid>),
    /// A component (C0: the root, the design's own settings).
    Component(ComponentUid),
}

/// Where an effective value comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum PrintFrom {
    /// Set on this owner.
    Owner(PropertyOwner),
    /// A value of the profile the owner chose.
    Profile {
        profile: String,
        owner: PropertyOwner,
    },
    /// The material (its id).
    Material(String),
    /// The print plate it prints on (its id, mitcad#1050): a level between
    /// the components and the design's own settings.
    Plate(String),
}

/// The settings something prints with, resolved.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectivePrint {
    /// The printer and process profiles chosen (the nearest), with the
    /// owner that chose each.
    pub printer: Option<(String, PropertyOwner)>,
    pub process: Option<(String, PropertyOwner)>,
    /// Their names.
    pub printer_name: Option<String>,
    pub process_name: Option<String>,
    /// Printer and process values by Mitcad key, in their JSON form (a key
    /// kept per slicer as its object).
    pub values: BTreeMap<String, (Value, PrintFrom)>,
    /// The material's id, name and filament values.
    pub material: String,
    pub material_name: String,
    pub filament: BTreeMap<String, (Value, PrintFrom)>,
    /// The appearance it prints with (mitcad#1055): the body's, else its
    /// component's for its material, else the material's default. Its
    /// colour is the filament's `filament_colour`; a material and an
    /// appearance are one filament slot of an export.
    pub appearance: Option<String>,
    /// The material's variant the printer picked (mitcad#1054): its index
    /// in the material's variants and how it was picked; none: its own
    /// values.
    pub variant: Option<(usize, VariantMatch)>,
    /// Slicers' own keys by slicer id.
    pub slicer: BTreeMap<String, BTreeMap<String, (String, PrintFrom)>>,
    /// The printer profile's raw blocks by slicer id.
    pub raw: BTreeMap<String, BTreeMap<String, Value>>,
}

impl EffectivePrint {
    /// A value of a printer, process or filament key as the key map's (none
    /// for a key kept per slicer).
    pub fn value(&self, key: &str) -> Option<print_keys::Value> {
        let def = print_keys::key(key)?;
        if def.per_dialect {
            return None;
        }
        let (value, _) = self.values.get(key).or_else(|| self.filament.get(key))?;
        value_of(def, value).ok()
    }

    /// A key kept per slicer (a preset's name) for one slicer.
    pub fn dialect_text(&self, key: &str, dialect: Dialect) -> Option<String> {
        let (value, _) = self.values.get(key).or_else(|| self.filament.get(key))?;
        crate::print_profile::dialect_value(value, dialect)
            .and_then(Value::as_str)
            .map(str::to_owned)
    }
}

/// The own type of a kind's profile choice: the design's profiles of the
/// kind as its values.
fn profile_choice_type(profiles: &[PrintProfile], kind: ProfileKind) -> PropertyType {
    PropertyType {
        name: kind.choice_type().to_owned(),
        base: BaseType::Text,
        values: profiles
            .iter()
            .filter(|p| p.kind == kind)
            .map(|p| PropertyValue::Text(p.id.clone()))
            .collect(),
        unit: None,
        default: None,
        description: format!("One of the design's {} profiles", kind.id()),
    }
}

/// `#RRGGBB` of an sRGB colour in [0, 1].
pub(crate) fn hex_color(color: [f64; 3]) -> String {
    let [r, g, b] = color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// The plain name a print property shows (`Layer height`; `Profile`), if
/// it is one of the map's keys in its set.
pub fn print_key_name(set: &str, key: &str) -> Option<&'static str> {
    let Some(PrintPropertySet::Layer(layer)) = print_set_of(set) else {
        return None;
    };
    if same_name(key, PROFILE_KEY) {
        return Some("Profile");
    }
    print_keys::key(key.trim())
        .filter(|d| d.set == layer)
        .map(|d| d.name)
}

/// A value's text for people: a number with its unit, text as it is, a
/// per-slicer value per slicer.
pub(crate) fn value_text(def: &KeyDef, value: &Value) -> String {
    if def.per_dialect
        && let Value::Object(values) = value
    {
        return values
            .iter()
            .map(|(dialect, v)| {
                let name = Dialect::from_id(dialect).map_or(dialect.as_str(), |d| d.name());
                format!(
                    "{name}: {}",
                    v.as_str().map_or_else(|| v.to_string(), str::to_owned)
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
    }
    let unit = def.unit().symbol();
    match value_of(def, value) {
        Ok(print_keys::Value::Text(text)) => text,
        Ok(v @ (print_keys::Value::Number(_) | print_keys::Value::Integer(_))) => {
            let text = v.to_string();
            match unit {
                "" => text,
                "%" => format!("{text}%"),
                unit => format!("{text} {unit}"),
            }
        }
        Ok(v) => v.to_string(),
        Err(_) => value.to_string(),
    }
}

/// A material's variant as the queries give it (mitcad#1054): its
/// printer, nozzle, name for people and how the printer picked it.
pub(crate) fn variant_json(variant: &crate::material::MaterialVariant, how: VariantMatch) -> Value {
    json!({
        "printer": variant.printer,
        "nozzle": variant.nozzle,
        "label": variant.label(),
        "match": how.id(),
    })
}

/// Whether a point is in an outline (on its edges too, within `tolerance`).
fn inside_outline(point: [f64; 2], outline: &[[f64; 2]], tolerance: f64) -> bool {
    let n = outline.len();
    let mut inside = false;
    for i in 0..n {
        let a = outline[i];
        let b = outline[(i + 1) % n];
        // On the edge.
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length2 = dx * dx + dy * dy;
        let t = if length2 > 0.0 {
            (((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / length2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (cx, cy) = (a[0] + t * dx - point[0], a[1] + t * dy - point[1]);
        if cx * cx + cy * cy <= tolerance * tolerance {
            return true;
        }
        if (a[1] > point[1]) != (b[1] > point[1]) {
            let x = a[0] + (point[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if point[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

impl DocState {
    /// A profile of the design by id.
    pub(crate) fn print_profile(&self, id: &str) -> Option<&PrintProfile> {
        self.print_profiles.iter().find(|p| p.id == id)
    }

    /// Puts an own type of the print settings (a choice's, a profile
    /// choice's) in the place of one of the same name.
    pub(crate) fn put_print_type(&mut self, ty: PropertyType) {
        match self
            .properties
            .types
            .iter_mut()
            .find(|t| same_name(&t.name, &ty.name))
        {
            Some(slot) => *slot = ty,
            None => self.properties.types.push(ty),
        }
    }

    /// Keeps the profile choice types (those the design has) listing the
    /// design's profiles.
    pub(crate) fn sync_profile_types(&mut self) {
        for kind in ProfileKind::ALL {
            if self.properties.own_type(kind.choice_type()).is_some() {
                let ty = profile_choice_type(&self.print_profiles, kind);
                self.put_print_type(ty);
            }
        }
    }

    /// Whether another profile of the kind has the name (`own`: the index
    /// of the one being edited).
    fn profile_name_taken(&self, name: &str, kind: ProfileKind, own: Option<usize>) -> bool {
        self.print_profiles
            .iter()
            .enumerate()
            .any(|(i, p)| Some(i) != own && p.kind == kind && same_name(&p.name, name))
    }

    /// Adds a profile from elsewhere (another design, a library, an
    /// import): one the design has as it is stays; one whose id is taken
    /// by another gets a free id, one whose name is taken another name.
    /// The id it has here.
    pub(crate) fn add_print_profile(&mut self, profile: PrintProfile) -> String {
        if let Some(mine) = self.print_profile(&profile.id)
            && *mine == profile
        {
            return profile.id;
        }
        let mut copy = profile;
        if self.print_profile(&copy.id).is_some() {
            let base = copy.id.clone();
            copy.id = (1..)
                .map(|n| format!("{base}_{n}"))
                .find(|candidate| self.print_profile(candidate).is_none())
                .expect("a free id");
        }
        if self.profile_name_taken(&copy.name, copy.kind, None) {
            let base = copy.name.clone();
            copy.name = (1..)
                .map(|n| format!("{base} ({n})"))
                .find(|name| !self.profile_name_taken(name, copy.kind, None))
                .expect("a free name");
        }
        let id = copy.id.clone();
        self.print_profiles.push(copy);
        self.sync_profile_types();
        id
    }

    /// Makes an owner choose a profile of the design (its id), with the
    /// source of the choice (an import's).
    pub(crate) fn choose_print_profile(
        &mut self,
        owner: PropertyOwner,
        kind: ProfileKind,
        id: &str,
        source: Source,
    ) {
        let mut property = Property::new(
            PROFILE_KEY,
            BaseType::Text,
            PropertyValue::Text(id.to_owned()),
            source,
        );
        property.ty = kind.choice_type().to_owned();
        let ty = profile_choice_type(&self.print_profiles, kind);
        self.put_print_type(ty);
        self.properties
            .put(owner, kind.set().property_set(), property);
    }

    /// Copies a profile another document's state has (the choice of a
    /// copied component): the id it has here.
    pub(crate) fn copy_print_profile(&mut self, source: &DocState, id: &str) -> Option<String> {
        let theirs = source.print_profile(id)?.clone();
        Some(self.add_print_profile(theirs))
    }
}

impl<K: Kernel> Document<K> {
    /// The design's printer and process profiles.
    pub fn print_profiles(&self) -> &[PrintProfile] {
        &self.state.print_profiles
    }

    /// A profile of the design by id.
    pub fn print_profile(&self, id: &str) -> Option<&PrintProfile> {
        self.state.print_profile(id)
    }

    /// The fields are valid and the name not another profile's of the
    /// kind.
    pub(super) fn check_profile(
        &self,
        profile: &PrintProfile,
        own: Option<usize>,
    ) -> Result<(), ModelError> {
        profile.check().map_err(invalid)?;
        if self
            .state
            .profile_name_taken(&profile.name, profile.kind, own)
        {
            return Err(invalid(format!(
                "a {} profile is named {}",
                profile.kind.id(),
                profile.name
            )));
        }
        Ok(())
    }

    /// Makes a profile from `based_on`'s fields (a profile of the design,
    /// or a library's item, mitcad#655) or empty of `kind`, with the given
    /// fields. Without an id it is the name's (`fine-0-12mm`), else
    /// `printer1`, `process1`, ... Returns the id. One undo step.
    pub fn create_print_profile(&mut self, change: &ProfileChange) -> Result<String, ModelError> {
        if change.library.is_some() {
            return Err(invalid(
                "a profile of a library is made there (create_library_profile)",
            ));
        }
        let mut new = match change.based_on.as_ref() {
            Some(BasedOn::Design(id)) => {
                let mut base = self
                    .print_profile(id)
                    .cloned()
                    .ok_or_else(|| invalid(format!("there is no profile {id}")))?;
                base.based_on = Some(id.to_owned());
                base.origin = None;
                base.version = None;
                base
            }
            Some(BasedOn::Library(item)) => {
                let libraries = self.print_libraries(change.project.as_deref().map(Path::new));
                let library = super::library_kind(&item.library)?;
                let kind = Self::library_profile_kind(&libraries, library, change.kind, &item.id)?;
                let found = super::library_items::library_item(
                    &libraries,
                    library,
                    ItemKind::of_profile(kind),
                    &item.id,
                )?;
                let ItemData::Profile(mut base) = found.data.clone() else {
                    unreachable!("a profile's folder")
                };
                base.based_on = Some(item.id.clone());
                base.origin = None;
                base.version = None;
                base
            }
            None => {
                let kind = change.kind.ok_or_else(|| {
                    invalid("create_print_profile needs a kind: printer or process")
                })?;
                PrintProfile::new("", "", kind)
            }
        };
        if let Some(kind) = change.kind
            && kind != new.kind
        {
            return Err(invalid(format!(
                "{} is a {} profile, not a {} one",
                new.name,
                new.kind.id(),
                kind.id()
            )));
        }
        let kind = new.kind;
        let title = match kind {
            ProfileKind::Printer => "Printer",
            ProfileKind::Process => "Process",
        };
        // The libraries' names count too (mitcad#655): the boxes never show
        // two printers or processes alike.
        let libraries = self.print_libraries(change.project.as_deref().map(Path::new));
        let library_names: Vec<String> = libraries
            .items
            .iter()
            .filter(|i| i.data.kind() == ItemKind::of_profile(kind) && !i.shadowed)
            .map(|i| i.data.name().to_lowercase())
            .collect();
        let taken = |name: &str| {
            self.state.profile_name_taken(name, kind, None)
                || library_names.contains(&name.to_lowercase())
        };
        let free = (1..)
            .find(|n| {
                self.print_profile(&format!("{}{n}", kind.id())).is_none()
                    && !taken(&new_item_name(title, *n))
            })
            .expect("a free number");
        // A copy is named after its source, `<name> (copy)` (mitcad#655:
        // the user's own copy of a built-in or library item; mitcad#828), a
        // new one `Printer 1`.
        new.name = match change.based_on {
            Some(_) => copy_name(&new.name, taken),
            None => new_item_name(title, free),
        };
        change.apply_to(&mut new);
        new.normalize();
        new.id = match (change.id.as_deref(), change.name.as_deref()) {
            (Some(id), _) => id.trim().to_owned(),
            (None, Some(name)) => free_id(&slug(name), |id| self.print_profile(id).is_some()),
            (None, None) => format!("{}{free}", kind.id()),
        };
        if self.print_profile(&new.id).is_some() {
            return Err(invalid(format!("a profile {} exists", new.id)));
        }
        self.check_profile(&new, None)?;
        let label = format!("Create Profile {}", new.name);
        let id = new.id.clone();
        // Made the design's choice in the same step (mitcad#655).
        let choose = change.choose.unwrap_or(false);
        self.apply_with(|state| {
            state.print_profiles.push(new);
            state.sync_profile_types();
            if choose {
                state.choose_print_profile(
                    PropertyOwner::Component(ComponentUid::ROOT),
                    kind,
                    &id,
                    Source::User,
                );
            }
            Ok((label, (), false))
        })?;
        Ok(id)
    }

    /// Changes the given fields of a profile. One undo step unless nothing
    /// changes.
    pub fn edit_print_profile(
        &mut self,
        id: &str,
        change: &ProfileChange,
    ) -> Result<(), ModelError> {
        if change.based_on.is_some() {
            return Err(invalid("based_on is only for create_print_profile"));
        }
        if change.id.as_deref().is_some_and(|new| new != id) {
            return Err(invalid("a profile's id cannot change"));
        }
        let i = self
            .state
            .print_profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| invalid(format!("there is no profile {id}")))?;
        if change
            .kind
            .is_some_and(|k| k != self.state.print_profiles[i].kind)
        {
            return Err(invalid("a profile's kind cannot change"));
        }
        let mut edited = self.state.print_profiles[i].clone();
        change.apply_to(&mut edited);
        edited.normalize();
        if edited == self.state.print_profiles[i] {
            return Ok(());
        }
        self.check_profile(&edited, Some(i))?;
        let label = format!(
            "Edit {} {}",
            profile_title(self.state.print_profiles[i].kind),
            self.state.print_profiles[i].name
        );
        self.apply_with(|state| {
            state.print_profiles[i] = edited;
            Ok((label, (), false))
        })
    }

    /// Deletes a profile; the components, occurrences and bodies that
    /// chose it no longer choose one. Returns them. One undo step.
    pub fn delete_print_profile(&mut self, id: &str) -> Result<Vec<PropertyOwner>, ModelError> {
        let i = self
            .state
            .print_profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| invalid(format!("there is no profile {id}")))?;
        let set = self.state.print_profiles[i].kind.set().property_set();
        let chosen = PropertyValue::Text(id.to_owned());
        let owners: Vec<PropertyOwner> = self
            .state
            .properties
            .owners
            .keys()
            .copied()
            .filter(|o| {
                self.state
                    .properties
                    .get(*o, set, PROFILE_KEY)
                    .is_some_and(|p| p.value == chosen)
            })
            .collect();
        let label = format!("Delete Profile {}", self.state.print_profiles[i].name);
        let removed = owners.clone();
        self.apply_with(|state| {
            state.print_profiles.remove(i);
            for owner in &removed {
                state.properties.remove(*owner, set, PROFILE_KEY);
            }
            state.sync_profile_types();
            Ok((label, (), false))
        })?;
        Ok(owners)
    }

    /// Sets a property of a print set (`set_property` on `Print Printer`,
    /// `Print Process` or `Print Slicer (<slicer>)`): a key of the map
    /// with a value of its kind, kept as [`property_form`] says (a choice
    /// as its own type), the profile choice as one of the design's
    /// profiles, a slicer's key as text. `Print Filament` is the
    /// material's. One undo step.
    pub(crate) fn set_print_property(
        &mut self,
        owner: PropertyOwner,
        set: PrintPropertySet,
        key: &str,
        value: Option<&Value>,
        source: Source,
    ) -> Result<Property, ModelError> {
        let key = key.trim();
        let value = value.ok_or_else(|| invalid(format!("{key} needs a value")))?;
        if value.is_null() {
            return Err(invalid(format!(
                "{key} needs a value (remove_property takes one away)"
            )));
        }
        match set {
            PrintPropertySet::Layer(PrintSet::Filament) => Err(invalid(
                "Print Filament values are the body's material's: edit_material sets them",
            )),
            PrintPropertySet::Slicer(dialect) => {
                let text = match value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                let property =
                    Property::new(key, BaseType::Text, PropertyValue::Text(text), source);
                self.store_print_property(owner, &slicer_set(dialect), property, None, None)
            }
            PrintPropertySet::Layer(layer) => {
                let kind = ProfileKind::of_set(layer).expect("a printer or process set");
                if same_name(key, PROFILE_KEY) {
                    let id = value.as_str().map(str::trim).ok_or_else(|| {
                        invalid(format!("the {} profile is given by its id", kind.id()))
                    })?;
                    let Some(profile) = self.print_profile(id).filter(|p| p.kind == kind) else {
                        let ids: Vec<&str> = self
                            .print_profiles()
                            .iter()
                            .filter(|p| p.kind == kind)
                            .map(|p| p.id.as_str())
                            .collect();
                        return Err(invalid(format!(
                            "there is no {} profile {id} (the design's: {})",
                            kind.id(),
                            if ids.is_empty() {
                                "none".to_owned()
                            } else {
                                ids.join(", ")
                            }
                        )));
                    };
                    let mut property = Property::new(
                        PROFILE_KEY,
                        BaseType::Text,
                        PropertyValue::Text(profile.id.clone()),
                        source,
                    );
                    property.ty = kind.choice_type().to_owned();
                    let label = format!("Use {} {}", profile_title(kind), profile.name);
                    let ty = profile_choice_type(&self.state.print_profiles, kind);
                    return self.store_print_property(
                        owner,
                        layer.property_set(),
                        property,
                        Some(ty),
                        Some(label),
                    );
                }
                let def = print_keys::key(key)
                    .filter(|d| d.set == layer)
                    .ok_or_else(|| {
                        invalid(format!(
                            "{key} is no key of the {} set (the print_keys query lists them)",
                            layer.property_set()
                        ))
                    })?;
                if def.per_dialect {
                    return Err(invalid(format!(
                        "{key} names a slicer's own preset: a profile keeps it per slicer"
                    )));
                }
                let parsed = value_of(def, value).map_err(invalid)?;
                let stored = to_property(def, &parsed);
                let (base, unit, own) = match property_form(def) {
                    PropertyForm::Standard(base, unit) => (base, unit, None),
                    PropertyForm::Choice(name, names) => (
                        BaseType::Text,
                        None,
                        Some(PropertyType {
                            name,
                            base: BaseType::Text,
                            values: names
                                .iter()
                                .map(|(value, _)| PropertyValue::Text((*value).to_owned()))
                                .collect(),
                            unit: None,
                            default: None,
                            description: format!("{} (print setting {})", def.name, def.key),
                        }),
                    ),
                };
                let mut property =
                    Property::new(def.key, base, stored, source).with_unit(unit.map(str::to_owned));
                if let Some(own) = &own {
                    property.ty = own.name.clone();
                }
                self.store_print_property(owner, layer.property_set(), property, own, None)
            }
        }
    }

    /// Stores a print property (and the own type it needs); a user's value
    /// over an imported one keeps the imported value, as `set_property`
    /// does.
    fn store_print_property(
        &mut self,
        owner: PropertyOwner,
        set: &str,
        mut property: Property,
        own_type: Option<PropertyType>,
        label: Option<String>,
    ) -> Result<Property, ModelError> {
        if let Some(existing) = self.state.properties.get(owner, set, &property.key)
            && property.source == Source::User
            && existing.source != Source::User
        {
            let imported = existing
                .imported
                .clone()
                .unwrap_or_else(|| existing.value.clone());
            property.source = existing.source;
            property.imported = (!imported.same(&property.value)).then_some(imported);
        }
        let label = label.unwrap_or_else(|| format!("Set {}", property.key));
        let stored = property.clone();
        let set = set.to_owned();
        self.apply_with(|state| {
            if let Some(ty) = own_type {
                state.put_print_type(ty);
            }
            state.properties.put(owner, &set, property);
            Ok((label, (), false))
        })?;
        Ok(stored)
    }

    /// The owners whose print settings a target inherits, nearest first:
    /// the body, then each occurrence of the path from the last and its
    /// component, then the root.
    pub fn print_levels(&self, target: &PrintTarget) -> Vec<PropertyOwner> {
        let mut levels = Vec::new();
        let path_levels = |path: &[OccurrenceUid], levels: &mut Vec<PropertyOwner>| {
            for o in path.iter().rev() {
                levels.push(PropertyOwner::Occurrence(*o));
                if let Some(occurrence) = self.state.assembly.occurrence(*o) {
                    levels.push(PropertyOwner::Component(occurrence.component));
                }
            }
        };
        match target {
            PrintTarget::Body(body, path) => {
                levels.push(PropertyOwner::Body(*body));
                match path {
                    Some(path) => path_levels(path, &mut levels),
                    None => {
                        if let Some(c) = self
                            .body_component(*body)
                            .or_else(|| self.state.entry(body.feature).map(|f| f.component))
                            .filter(|c| !c.is_root())
                        {
                            levels.push(PropertyOwner::Component(c));
                        }
                    }
                }
            }
            PrintTarget::Occurrence(path) => path_levels(path, &mut levels),
            PrintTarget::Component(c) => {
                if !c.is_root() {
                    levels.push(PropertyOwner::Component(*c));
                }
            }
        }
        let root = PropertyOwner::Component(ComponentUid::ROOT);
        levels.retain(|l| *l != root);
        levels.push(root);
        levels
    }

    /// The profile of a kind an owner chose (one of the design's).
    fn level_profile(&self, owner: PropertyOwner, kind: ProfileKind) -> Option<&PrintProfile> {
        let property = self
            .state
            .properties
            .get(owner, kind.set().property_set(), PROFILE_KEY)?;
        let PropertyValue::Text(id) = &property.value else {
            return None;
        };
        self.print_profile(id.trim()).filter(|p| p.kind == kind)
    }

    /// The material of the nearest component level that has a default,
    /// else the built-in default, with that component's appearance
    /// (mitcad#1055; none: the material's default).
    fn levels_material(&self, levels: &[PropertyOwner]) -> (&Material, Option<&str>) {
        levels
            .iter()
            .filter_map(|o| match o {
                PropertyOwner::Component(c) => Some(*c),
                _ => None,
            })
            .find_map(|c| {
                self.component_material(c)
                    .and_then(|id| self.material(id))
                    .map(|m| (m, self.component_appearance(c)))
            })
            .or_else(|| self.material(DEFAULT_MATERIAL).map(|m| (m, None)))
            .expect("the default material is built in")
    }

    /// A material's filament values: its own with its variant's
    /// (`variant`, an index of its variants, mitcad#1054) over them, its
    /// density as `filament_density` and the colour of the appearance it
    /// prints with (`look`, mitcad#1055) as `filament_colour`: always that
    /// appearance's, never a colour of the material's own.
    pub(crate) fn filament_values(
        &self,
        material: &Material,
        variant: Option<usize>,
        look: Option<&str>,
    ) -> BTreeMap<String, (Value, PrintFrom)> {
        let from = || PrintFrom::Material(material.id.clone());
        let (values, _) = material.values_with(variant);
        let mut out: BTreeMap<String, (Value, PrintFrom)> =
            values.into_iter().map(|(k, v)| (k, (v, from()))).collect();
        out.insert(
            "filament_density".to_owned(),
            (json!(material.density), from()),
        );
        out.remove("filament_colour");
        if let Some(color) = look
            .and_then(|a| self.appearance(a))
            .map(|a| a.display_color())
        {
            out.insert(
                "filament_colour".to_owned(),
                (json!(hex_color(color)), from()),
            );
        }
        out
    }

    /// The settings a target prints with: printer and process values by
    /// the inheritance above, the material's filament, the slicers' own
    /// keys (set on the levels, then the profiles', then the material's)
    /// and the printer profile's raw blocks. A body or an occurrence takes
    /// the values of the print plate it prints on (mitcad#1050,
    /// [`Document::effective_print_on`]).
    pub fn effective_print(&self, target: &PrintTarget) -> EffectivePrint {
        self.effective_print_on(target, self.auto_plate(target))
    }

    /// [`Document::effective_print`] on a print plate (mitcad#1050), or on
    /// none: the plate's process values come after every level but the
    /// root (profile < plate < component < occurrence < body), so a
    /// component's, an occurrence's or a body's own value wins over the
    /// plate's, and the plate's over the design's own values and
    /// profiles.
    pub fn effective_print_on(
        &self,
        target: &PrintTarget,
        plate: Option<&crate::print_plates::PrintPlate>,
    ) -> EffectivePrint {
        let levels = self.print_levels(target);
        let root = PropertyOwner::Component(ComponentUid::ROOT);
        let mut out = EffectivePrint::default();
        for kind in ProfileKind::ALL {
            let set = kind.set();
            for owner in &levels {
                // The plate, just before the design's own level.
                if let Some(plate) = plate
                    && kind == ProfileKind::Process
                    && *owner == root
                {
                    for (key, value) in &plate.values {
                        if print_keys::key(key).is_some_and(|d| !d.per_dialect)
                            && !out.values.contains_key(key)
                        {
                            out.values.insert(
                                key.clone(),
                                (value.clone(), PrintFrom::Plate(plate.id.clone())),
                            );
                        }
                    }
                }
                if let Some(group) = self
                    .state
                    .properties
                    .sets(*owner)
                    .iter()
                    .find(|g| same_name(&g.name, set.property_set()))
                {
                    for property in &group.properties {
                        let Some(def) = print_keys::key(property.key.trim())
                            .filter(|d| d.set == set && !d.per_dialect)
                        else {
                            continue;
                        };
                        if out.values.contains_key(def.key) {
                            continue;
                        }
                        if let Ok(value) = from_property(def, &property.value) {
                            out.values.insert(
                                def.key.to_owned(),
                                (value.to_json(), PrintFrom::Owner(*owner)),
                            );
                        }
                    }
                }
                if let Some(profile) = self.level_profile(*owner, kind) {
                    let chosen = match kind {
                        ProfileKind::Printer => &mut out.printer,
                        ProfileKind::Process => &mut out.process,
                    };
                    if chosen.is_none() {
                        *chosen = Some((profile.id.clone(), *owner));
                    }
                    for (key, value) in &profile.values {
                        if !out.values.contains_key(key) {
                            out.values.insert(
                                key.clone(),
                                (
                                    value.clone(),
                                    PrintFrom::Profile {
                                        profile: profile.id.clone(),
                                        owner: *owner,
                                    },
                                ),
                            );
                        }
                    }
                }
            }
        }
        // The slicers' own keys: the levels', then the chosen profiles'.
        for owner in &levels {
            for group in self.state.properties.sets(*owner) {
                let Some(PrintPropertySet::Slicer(dialect)) = print_set_of(&group.name) else {
                    continue;
                };
                let keys = out.slicer.entry(dialect.id().to_owned()).or_default();
                for property in &group.properties {
                    let text = match &property.value {
                        PropertyValue::Text(text) => text.clone(),
                        other => other.to_json().to_string(),
                    };
                    keys.entry(property.key.clone())
                        .or_insert((text, PrintFrom::Owner(*owner)));
                }
            }
        }
        for (id, owner) in [&out.printer, &out.process]
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
        {
            let Some(profile) = self.print_profile(&id) else {
                continue;
            };
            for (dialect, keys) in &profile.slicer {
                let mine = out.slicer.entry(dialect.clone()).or_default();
                for (key, text) in keys {
                    mine.entry(key.clone()).or_insert((
                        text.clone(),
                        PrintFrom::Profile {
                            profile: id.clone(),
                            owner,
                        },
                    ));
                }
            }
            if profile.kind == ProfileKind::Printer {
                out.raw = profile.raw.clone();
            }
        }
        // The filament: the body's material, else the nearest component's
        // default.
        let (material, look) = match target {
            PrintTarget::Body(body, path) => (
                self.body_material(*body, path.as_deref()).0,
                self.body_look(*body, path.as_deref()),
            ),
            _ => {
                let (material, own) = self.levels_material(&levels);
                (material, own.or(material.appearance.as_deref()))
            }
        };
        out.appearance = look.map(str::to_owned);
        out.material = material.id.clone();
        out.material_name = material.name.clone();
        out.printer_name = out
            .printer
            .as_ref()
            .and_then(|(id, _)| self.print_profile(id))
            .map(|p| p.name.clone());
        out.process_name = out
            .process
            .as_ref()
            .and_then(|(id, _)| self.print_profile(id))
            .map(|p| p.name.clone());
        // Its variant for the printer (mitcad#1054): the one of the
        // printer and its nozzle, else of the printer, else none.
        let printer = out
            .printer
            .as_ref()
            .and_then(|(id, _)| self.print_profile(id))
            .map(PrintProfile::printer_key)
            .unwrap_or_default();
        out.variant = material.variant_for(&printer);
        let variant = out.variant.map(|(i, _)| i);
        out.filament = self.filament_values(material, variant, look);
        let (_, material_slicer) = material.values_with(variant);
        for (dialect, keys) in &material_slicer {
            let mine = out.slicer.entry(dialect.clone()).or_default();
            for (key, text) in keys {
                mine.entry(key.clone())
                    .or_insert((text.clone(), PrintFrom::Material(material.id.clone())));
            }
        }
        out.slicer.retain(|_, keys| !keys.is_empty());
        out
    }

    /// An owner's name for people.
    pub(crate) fn print_owner_name(&self, owner: PropertyOwner) -> String {
        match owner {
            PropertyOwner::Component(c) => self.assembly().name(c),
            PropertyOwner::Occurrence(o) => self.assembly().occurrence_name(o),
            PropertyOwner::Body(b) => self.body_name(b),
        }
    }

    /// Where a value comes from, as the queries give it.
    fn print_from_json(&self, from: &PrintFrom) -> Value {
        match from {
            PrintFrom::Owner(owner) => json!({
                "from": "property",
                "owner": owner,
                "level": owner.level().as_str(),
                "owner_name": self.print_owner_name(*owner),
            }),
            PrintFrom::Profile { profile, owner } => json!({
                "from": "profile",
                "profile": profile,
                "profile_name": self.print_profile(profile).map(|p| p.name.clone()),
                "owner": owner,
                "level": owner.level().as_str(),
                "owner_name": self.print_owner_name(*owner),
            }),
            PrintFrom::Material(id) => json!({
                "from": "material",
                "material": id,
                "material_name": self.material(id).map(|m| m.name.clone()),
            }),
            // mitcad#1050: the plate's name as the owner's.
            PrintFrom::Plate(id) => json!({
                "from": "plate",
                "plate": id,
                "level": "plate",
                "owner_name": self
                    .state
                    .print_plates
                    .iter()
                    .find(|p| p.id == *id)
                    .map(|p| p.name.clone()),
            }),
        }
    }

    /// The `print_settings` query: the effective values of the three sets
    /// for a target, each with where it comes from.
    pub fn print_settings_json(&self, target: &PrintTarget) -> Value {
        self.print_settings_json_on(target, self.auto_plate(target))
    }

    /// The `print_settings` query on a print plate (mitcad#1050), or on
    /// none; with a plate, `plate` names it (`{id, name}`).
    pub fn print_settings_json_on(
        &self,
        target: &PrintTarget,
        plate: Option<&crate::print_plates::PrintPlate>,
    ) -> Value {
        let mut out = self.print_settings_on(target, plate);
        out["plate"] = plate.map_or(Value::Null, |p| json!({"id": p.id, "name": p.name}));
        out
    }

    fn print_settings_on(
        &self,
        target: &PrintTarget,
        plate: Option<&crate::print_plates::PrintPlate>,
    ) -> Value {
        let effective = self.effective_print_on(target, plate);
        let entry = |def: &KeyDef, value: &Value, from: &PrintFrom| -> Value {
            let mut out = self.print_from_json(from);
            out["key"] = json!(def.key);
            out["name"] = json!(def.name);
            out["value"] = value.clone();
            out["text"] = json!(value_text(def, value));
            let unit = def.unit().symbol();
            if !unit.is_empty() {
                out["unit"] = json!(unit);
            }
            out
        };
        let layer = |set: PrintSet,
                     values: &BTreeMap<String, (Value, PrintFrom)>,
                     chosen: Option<&(String, PropertyOwner)>| {
            let values: Vec<Value> = print_keys::keys_in(set)
                .filter_map(|def| {
                    values
                        .get(def.key)
                        .map(|(value, from)| entry(def, value, from))
                })
                .collect();
            let profile = chosen.map(|(id, owner)| {
                json!({
                    "id": id,
                    "name": self.print_profile(id).map(|p| p.name.clone()),
                    "owner": owner,
                    "level": owner.level().as_str(),
                    "owner_name": self.print_owner_name(*owner),
                })
            });
            json!({"profile": profile, "values": values})
        };
        let slicer: Vec<Value> = effective
            .slicer
            .iter()
            .flat_map(|(dialect, keys)| {
                keys.iter()
                    .map(move |(key, (text, from))| (dialect, key, text, from))
            })
            .map(|(dialect, key, text, from)| {
                let mut out = self.print_from_json(from);
                out["slicer"] = json!(dialect);
                out["key"] = json!(key);
                out["value"] = json!(text);
                out
            })
            .collect();
        let material = self.material(&effective.material);
        // The variant the printer picked (mitcad#1054), and the values it
        // gives marked with it.
        let used = material.and_then(|m| {
            let (i, how) = effective.variant?;
            Some((m.variants.get(i)?, how))
        });
        let mut filament_values =
            layer(PrintSet::Filament, &effective.filament, None)["values"].clone();
        if let (Some((variant, _)), Value::Array(entries)) = (used, &mut filament_values) {
            for entry in entries.iter_mut() {
                let key = entry["key"].as_str().unwrap_or_default().to_owned();
                if variant.filament.contains_key(&key) {
                    entry["variant"] = json!(variant.label());
                }
            }
        }
        json!({
            "levels": self
                .print_levels(target)
                .into_iter()
                .map(|o| json!({"owner": o, "level": o.level().as_str(), "name": self.print_owner_name(o)}))
                .collect::<Vec<_>>(),
            "printer": layer(PrintSet::Printer, &effective.values, effective.printer.as_ref()),
            "process": layer(PrintSet::Process, &effective.values, effective.process.as_ref()),
            "filament": {
                "material": {"id": effective.material, "name": material.map(|m| m.name.clone())},
                // The appearance it prints with, its colour (mitcad#1055).
                "appearance": effective.appearance,
                "variant": used.map(|(v, how)| variant_json(v, how)),
                "variants": material.map_or(0, |m| m.variants.len()),
                "values": filament_values,
            },
            "slicer": slicer,
        })
    }

    /// The `build_volume` query: the design's printer (the root's
    /// effective one) as the view draws it, its bed centred on the origin
    /// with its surface at Z = 0, and the shown bodies that do not fit
    /// (their box beyond the bed's outline or its height, below the bed, or
    /// over an excluded area).
    pub fn build_volume_json(&self) -> Value {
        self.build_volume_in_json(
            &PrintTarget::Component(ComponentUid::ROOT),
            self.instances()
                .into_iter()
                .filter(|instance| instance.visible)
                .collect(),
        )
    }

    /// The same build-volume check for one body's placement and effective
    /// printer. A review's print queries use the first shown occurrence,
    /// whose printer and plate may differ from the root's.
    pub fn body_build_volume_json(&self, body: BodyUid, path: &[OccurrenceUid]) -> Value {
        let instance = self
            .instances()
            .into_iter()
            .find(|instance| instance.body == body && instance.path.as_slice() == path)
            .unwrap_or_else(|| InstanceView {
                path: path.to_vec(),
                path_name: self.assembly().path_name(path),
                component: self.body_component(body).unwrap_or(ComponentUid::ROOT),
                body,
                name: self.body_name(body),
                transform: self.path_transform(path),
                visible: true,
                transparent: false,
            });
        self.build_volume_in_json(
            &PrintTarget::Body(body, Some(path.to_vec())),
            vec![instance],
        )
    }

    fn build_volume_in_json(&self, target: &PrintTarget, instances: Vec<InstanceView>) -> Value {
        let effective = self.effective_print(target);
        let printer = effective
            .printer
            .as_ref()
            .and_then(|(id, _)| self.print_profile(id))
            .map(|p| json!({"id": p.id, "name": p.name}));
        let Some(print_keys::Value::Points(bed)) = effective.value("bed_shape") else {
            return json!({"printer": printer, "bed": null, "outside": []});
        };
        let (min, max) = print_keys::bounds(&bed);
        let shift = [-(min[0] + max[0]) / 2.0, -(min[1] + max[1]) / 2.0];
        let moved = |points: &[[f64; 2]]| -> Vec<[f64; 2]> {
            points
                .iter()
                .map(|p| [p[0] + shift[0], p[1] + shift[1]])
                .collect()
        };
        let bed = moved(&bed);
        let excluded: Vec<Vec<[f64; 2]>> = match effective.value("bed_exclude_area") {
            Some(print_keys::Value::Points(points)) => vec![moved(&points)],
            _ => Vec::new(),
        };
        let height = match effective.value("max_print_height") {
            Some(print_keys::Value::Number(h)) if h > 0.0 => Some(h),
            _ => None,
        };
        let tolerance = 1e-6;
        let mut outside = Vec::new();
        for instance in &instances {
            let Some(shape) = self.body_shape(instance.body) else {
                continue;
            };
            let Ok(Some(bbox)) = self.kernel().bounding_box(shape) else {
                continue;
            };
            let corners: Vec<[f64; 3]> = bbox
                .corners()
                .map(|c| instance.transform.apply_point(c))
                .collect();
            let lo: [f64; 3] =
                std::array::from_fn(|a| corners.iter().map(|c| c[a]).fold(f64::INFINITY, f64::min));
            let hi: [f64; 3] = std::array::from_fn(|a| {
                corners
                    .iter()
                    .map(|c| c[a])
                    .fold(f64::NEG_INFINITY, f64::max)
            });
            let footprint = [
                [lo[0], lo[1]],
                [hi[0], lo[1]],
                [hi[0], hi[1]],
                [lo[0], hi[1]],
            ];
            let on_bed = footprint
                .iter()
                .all(|p| inside_outline(*p, &bed, tolerance));
            let over_excluded = excluded.iter().any(|area| {
                let (a, b) = print_keys::bounds(area);
                lo[0] < b[0] - tolerance
                    && hi[0] > a[0] + tolerance
                    && lo[1] < b[1] - tolerance
                    && hi[1] > a[1] + tolerance
            });
            let in_height = lo[2] >= -tolerance && height.is_none_or(|h| hi[2] <= h + tolerance);
            if on_bed && !over_excluded && in_height {
                continue;
            }
            let mut why = Vec::new();
            if !on_bed {
                why.push("beyond the bed");
            }
            if over_excluded {
                why.push("over an excluded area");
            }
            if lo[2] < -tolerance {
                why.push("below the bed");
            }
            if height.is_some_and(|h| hi[2] > h + tolerance) {
                why.push("taller than the printable height");
            }
            outside.push(json!({
                "body": instance.body,
                "occurrence": instance
                    .path
                    .iter()
                    .map(|o| o.to_string())
                    .collect::<Vec<_>>()
                    .join("/"),
                "name": instance.name,
                "path_name": instance.path_name,
                "min": lo,
                "max": hi,
                "reasons": why,
            }));
        }
        json!({
            "printer": printer,
            "bed": bed,
            "excluded": excluded,
            "height": height,
            "shift": shift,
            "outside": outside,
        })
    }

    /// The `print_profiles` query: the design's profiles, each with its
    /// fields (not the raw printer blocks: which slicers have one) and the
    /// owners that chose it.
    pub fn print_profiles_json(&self) -> Value {
        Value::Array(
            self.state
                .print_profiles
                .iter()
                .map(|p| {
                    let mut value = serde_json::to_value(p).expect("profiles serialize");
                    if let Value::Object(fields) = &mut value {
                        fields.remove("raw");
                    }
                    value["raw_slicers"] = json!(p.raw.keys().collect::<Vec<_>>());
                    let chosen = PropertyValue::Text(p.id.clone());
                    let set = p.kind.set().property_set();
                    value["chosen_by"] = json!(
                        self.state
                            .properties
                            .owners
                            .keys()
                            .filter(|o| self
                                .state
                                .properties
                                .get(**o, set, PROFILE_KEY)
                                .is_some_and(|q| q.value == chosen))
                            .collect::<Vec<_>>()
                    );
                    value
                })
                .collect(),
        )
    }
}
