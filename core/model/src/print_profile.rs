// SPDX-License-Identifier: MIT
//! Printer and process profiles (mitcad#640, part 3): named sets of print
//! settings of the key map ([`crate::print_keys`]) that a design keeps and
//! its components, occurrences and bodies choose.
//!
//! A **printer profile** carries only what Mitcad uses (the bed, its
//! excluded areas, the printable height, the nozzle, the filament slots,
//! the printer model and the slicers' preset names); a **process profile**
//! any key of the `Print Process` set. A profile is complete in the design:
//! all its values are the design's, so that a library change never
//! changes a design's print settings by itself. A copy from a library
//! remembers where it came from (`origin`, as a material's).
//!
//! Settings a slicer has and the key map does not are kept per slicer as
//! text (`slicer`) and written back only to that slicer; a printer
//! imported from a Bambu-family project also keeps that file's whole
//! printer block (`raw`), hidden and not editable, for a slicer that does
//! not have the printer's preset.
//!
//! On components, occurrences and bodies the settings are property sets
//! (mitcad#496): `Print Printer` and `Print Process` hold overrides under
//! Mitcad's keys and the profile chosen (`profile`), `Print Slicer
//! (<slicer>)` a slicer's own keys as text. This module says how the key
//! map's values are kept as properties; `document/print_settings.rs`
//! resolves them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::material::{self, MaterialOrigin};
use crate::print_keys::{self, Dialect, KeyDef, Kind, PrintSet, Unit};
use crate::print_library::BasedOn;
use crate::properties::{BaseType, PropertyValue, same_name};

/// The key of a level's profile choice in `Print Printer` and `Print
/// Process`.
pub const PROFILE_KEY: &str = "profile";

/// What a profile sets: a printer or a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileKind {
    Printer,
    Process,
}

impl ProfileKind {
    pub const ALL: [ProfileKind; 2] = [ProfileKind::Printer, ProfileKind::Process];

    pub fn id(self) -> &'static str {
        match self {
            ProfileKind::Printer => "printer",
            ProfileKind::Process => "process",
        }
    }

    pub fn parse(text: &str) -> Option<ProfileKind> {
        ProfileKind::ALL.into_iter().find(|k| k.id() == text)
    }

    /// The layer of print settings it sets.
    pub fn set(self) -> PrintSet {
        match self {
            ProfileKind::Printer => PrintSet::Printer,
            ProfileKind::Process => PrintSet::Process,
        }
    }

    /// The own property type of the profile choice: its values are the
    /// design's profiles of the kind.
    pub fn choice_type(self) -> &'static str {
        match self {
            ProfileKind::Printer => "Print printer profile",
            ProfileKind::Process => "Print process profile",
        }
    }

    pub fn of_set(set: PrintSet) -> Option<ProfileKind> {
        match set {
            PrintSet::Printer => Some(ProfileKind::Printer),
            PrintSet::Process => Some(ProfileKind::Process),
            PrintSet::Filament => None,
        }
    }
}

/// A printer or process profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintProfile {
    /// Letters, digits, `_`, `-`, and `/` between parts.
    pub id: String,
    pub name: String,
    pub kind: ProfileKind,
    /// Values of the kind's set by Mitcad key, in their JSON form
    /// ([`print_keys::Value::to_json`]); a key that names a slicer's own
    /// preset holds `{"<slicer id>": value, ...}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, Value>,
    /// A slicer's settings that have no counterpart in the key map, by
    /// slicer id: its key and text, written only to that slicer.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub slicer: BTreeMap<String, BTreeMap<String, String>>,
    /// A printer's whole block as a Bambu-family project gave it, by
    /// slicer id: not shown, not edited, written back to that slicer only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub raw: BTreeMap<String, BTreeMap<String, Value>>,
    /// The profile it was made from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
    /// Its version in its library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// Where the design's copy came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<MaterialOrigin>,
}

/// A change of a profile in a command: the fields given. In
/// `edit_print_profile` a value's `null` removes that key, and a slicer
/// key's `null` that key.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileChange {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    /// `create_print_profile` only.
    #[serde(default)]
    pub kind: Option<ProfileKind>,
    #[serde(default)]
    pub values: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub slicer: Option<BTreeMap<String, BTreeMap<String, Value>>>,
    /// `create_print_profile` only: the profile whose fields it starts
    /// from, by id, or a library's item `{library, id}` (mitcad#655).
    #[serde(default)]
    pub based_on: Option<BasedOn>,
    #[serde(default, deserialize_with = "some")]
    pub version: Option<Option<u32>>,
    #[serde(default, deserialize_with = "some")]
    pub origin: Option<Option<MaterialOrigin>>,
    /// The library the item lives in (`user` or `project`), when the
    /// command works there rather than on the design's own profile
    /// (mitcad#655).
    #[serde(default)]
    pub library: Option<String>,
    /// The project's folder, when the design is in one.
    #[serde(default)]
    pub project: Option<String>,
    /// `create_print_profile` in the design only: the design (the root)
    /// chooses the new profile in the same undo step (mitcad#655).
    #[serde(default)]
    pub choose: Option<bool>,
}

/// A field given as `null` is `Some(None)`; one left out stays `None`.
fn some<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl ProfileChange {
    /// Applies the given fields (not `id`, `kind` or `based_on`).
    pub(crate) fn apply_to(&self, profile: &mut PrintProfile) {
        if let Some(name) = &self.name {
            profile.name = name.trim().to_owned();
        }
        if let Some(values) = &self.values {
            for (key, value) in values {
                if value.is_null() {
                    profile.values.remove(key);
                } else {
                    profile.values.insert(key.clone(), value.clone());
                }
            }
        }
        if let Some(slicer) = &self.slicer {
            for (dialect, keys) in slicer {
                let own = profile.slicer.entry(dialect.clone()).or_default();
                for (key, value) in keys {
                    match value {
                        Value::Null => {
                            own.remove(key);
                        }
                        Value::String(text) => {
                            own.insert(key.clone(), text.clone());
                        }
                        other => {
                            own.insert(key.clone(), other.to_string());
                        }
                    }
                }
            }
            profile.slicer.retain(|_, keys| !keys.is_empty());
        }
        if let Some(version) = self.version {
            profile.version = version;
        }
        if let Some(origin) = &self.origin {
            profile.origin = origin.clone();
        }
    }
}

impl PrintProfile {
    /// An empty profile of a kind.
    pub fn new(id: &str, name: &str, kind: ProfileKind) -> PrintProfile {
        PrintProfile {
            id: id.to_owned(),
            name: name.to_owned(),
            kind,
            values: BTreeMap::new(),
            slicer: BTreeMap::new(),
            raw: BTreeMap::new(),
            based_on: None,
            version: None,
            origin: None,
        }
    }

    /// Whether the fields are valid: the id, a name, every value a value
    /// of a key of the kind's set, slicer ids known, a raw block only on a
    /// printer.
    pub fn check(&self) -> Result<(), String> {
        if !material::valid_id(&self.id) {
            return Err(format!(
                "{} is no profile id: use letters, digits, '_', '-' and '/' between parts",
                self.id
            ));
        }
        if self.name.trim().is_empty() {
            return Err(format!("profile {}: the name is empty", self.id));
        }
        for (key, value) in &self.values {
            check_value(self.kind.set(), key, value)
                .map_err(|e| format!("profile {}: {e}", self.id))?;
        }
        for dialect in self.slicer.keys().chain(self.raw.keys()) {
            if Dialect::from_id(dialect).is_none() {
                return Err(format!("profile {}: {dialect} is no slicer id", self.id));
            }
        }
        if !self.raw.is_empty() && self.kind != ProfileKind::Printer {
            return Err(format!(
                "profile {}: only a printer keeps a slicer's printer block",
                self.id
            ));
        }
        Ok(())
    }

    /// Writes every value that reads as one of its key in the key map's
    /// JSON form (`"15%"` of a percentage as 15, an outline's text as
    /// points); the others are left for [`PrintProfile::check`] to name.
    pub fn normalize(&mut self) {
        for (key, value) in self.values.iter_mut() {
            let Some(def) = print_keys::key(key).filter(|d| !d.per_dialect) else {
                continue;
            };
            if let Ok(parsed) = value_of(def, value) {
                *value = parsed.to_json();
            }
        }
    }

    /// The profile's value of a key as the key map's value (None when not
    /// set, or a key kept per slicer).
    pub fn value(&self, key: &str) -> Option<print_keys::Value> {
        let def = print_keys::key(key)?;
        if def.per_dialect {
            return None;
        }
        print_keys::Value::from_json(def, self.values.get(key)?).ok()
    }

    /// A printer's nozzle diameter in mm (mitcad#1054), when it sets one.
    pub fn nozzle(&self) -> Option<f64> {
        match self.value("nozzle_diameter")? {
            print_keys::Value::Number(n) if n > 0.0 && n.is_finite() => Some(n),
            _ => None,
        }
    }

    /// The texts of a key kept per slicer, every slicer's.
    fn per_slicer_texts(&self, key: &str) -> Vec<String> {
        match self.values.get(key) {
            Some(Value::Object(values)) => values
                .values()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .collect(),
            Some(Value::String(text)) if !text.trim().is_empty() => vec![text.trim().to_owned()],
            _ => Vec::new(),
        }
    }

    /// The printer a material's variant names for this printer profile
    /// (mitcad#1054): its printer model, else its name without the nozzle
    /// ("Bench X 0.4 nozzle" is Bench X).
    pub fn variant_printer(&self) -> String {
        self.per_slicer_texts("printer_model")
            .into_iter()
            .next()
            .unwrap_or_else(|| material::split_nozzle(&self.name).0)
    }

    /// The printer profile as a material's variants pick it (mitcad#1054):
    /// every name it goes by (its printer model, its name and its slicers'
    /// preset names, each also without the nozzle) and its nozzle.
    pub fn printer_key(&self) -> material::PrinterKey {
        let mut names = vec![self.variant_printer()];
        for name in [self.name.clone()]
            .into_iter()
            .chain(self.per_slicer_texts("printer_model"))
            .chain(self.per_slicer_texts("printer_settings_id"))
        {
            let bare = material::split_nozzle(&name).0;
            for candidate in [name, bare] {
                if !names.iter().any(|n| material::same_printer(n, &candidate)) {
                    names.push(candidate);
                }
            }
        }
        // The nozzle its name says when it sets none.
        let nozzle = self
            .nozzle()
            .or_else(|| material::split_nozzle(&self.name).1);
        material::PrinterKey { names, nozzle }
    }
}

/// Whether a value fits a key of a set: a key of the set, of its kind; a
/// key kept per slicer holds `{"<slicer id>": value, ...}`.
pub fn check_value(set: PrintSet, key: &str, value: &Value) -> Result<(), String> {
    let def = print_keys::key(key)
        .filter(|def| def.set == set)
        .ok_or_else(|| format!("{key} is no key of the {} set", set.property_set()))?;
    if def.per_dialect {
        let Value::Object(values) = value else {
            return Err(format!(
                "{key} holds a value per slicer: {{\"<slicer id>\": value, ...}}"
            ));
        };
        for (dialect, value) in values {
            if Dialect::from_id(dialect).is_none() {
                return Err(format!("{key}: {dialect} is no slicer id"));
            }
            print_keys::Value::from_json(def, value).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    print_keys::Value::from_json(def, value)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// A per-slicer value of a key kept per slicer (`{"prusaslicer": "..."}`)
/// for one slicer: its own, else a Bambu-family member's for another of
/// the family.
pub fn dialect_value(value: &Value, dialect: Dialect) -> Option<&Value> {
    let Value::Object(values) = value else {
        return Some(value);
    };
    values.get(dialect.id()).or_else(|| {
        dialect
            .bambu_family()
            .then(|| {
                Dialect::ALL
                    .into_iter()
                    .filter(|d| d.bambu_family())
                    .find_map(|d| values.get(d.id()))
            })
            .flatten()
    })
}

/// What property sets are about print settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintPropertySet {
    /// `Print Printer`, `Print Process` or `Print Filament`.
    Layer(PrintSet),
    /// `Print Slicer (<slicer name>)`: a slicer's own keys as text.
    Slicer(Dialect),
}

/// The print set a property set name is, if it is one (ignoring case).
pub fn print_set_of(name: &str) -> Option<PrintPropertySet> {
    if let Some(set) = PrintSet::ALL
        .into_iter()
        .find(|s| same_name(s.property_set(), name))
    {
        return Some(PrintPropertySet::Layer(set));
    }
    Dialect::ALL
        .into_iter()
        .find(|d| same_name(&slicer_set(*d), name) || same_name(&slicer_set_by_id(*d), name))
        .map(PrintPropertySet::Slicer)
}

/// The property set of a slicer's own keys: `Print Slicer (PrusaSlicer)`.
pub fn slicer_set(dialect: Dialect) -> String {
    format!("Print Slicer ({})", dialect.name())
}

/// The same by the slicer's id, which commands may use too.
fn slicer_set_by_id(dialect: Dialect) -> String {
    format!("Print Slicer ({})", dialect.id())
}

/// How a key's values are kept as properties: a standard type (with a
/// plain number's unit label), or an own type of the choices.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyForm {
    Standard(BaseType, Option<&'static str>),
    /// An own type: its name and allowed values (`(value, name)`).
    Choice(String, &'static [(&'static str, &'static str)]),
}

/// How a key of the map is kept as a property: yes/no, integers, lengths
/// in mm (and areas and volumes), other numbers with their unit as a label
/// (percentages `%`), choices as an own type `Print <key>` with the
/// choices as its values, and a number or a percentage, text, colours and
/// outlines as text.
pub fn property_form(def: &KeyDef) -> PropertyForm {
    match def.kind {
        Kind::Boolean => PropertyForm::Standard(BaseType::Boolean, None),
        Kind::Integer(_) => PropertyForm::Standard(BaseType::Integer, None),
        Kind::Number(Unit::Mm) => PropertyForm::Standard(BaseType::Length, None),
        Kind::Number(Unit::Mm2) => PropertyForm::Standard(BaseType::Area, None),
        Kind::Number(Unit::Mm3) => PropertyForm::Standard(BaseType::Volume, None),
        Kind::Number(unit) => {
            let symbol = unit.symbol();
            PropertyForm::Standard(BaseType::Number, (!symbol.is_empty()).then_some(symbol))
        }
        Kind::Percent => PropertyForm::Standard(BaseType::Number, Some("%")),
        Kind::Choice(names) => PropertyForm::Choice(choice_type(def), names),
        Kind::OrPercent(..) | Kind::Text | Kind::Color | Kind::Points => {
            PropertyForm::Standard(BaseType::Text, None)
        }
    }
}

/// The own type of a choice key's properties.
pub fn choice_type(def: &KeyDef) -> String {
    format!("Print {}", def.key)
}

/// A key map value as a property value (of [`property_form`]).
pub fn property_value(value: &print_keys::Value) -> PropertyValue {
    match value {
        print_keys::Value::Bool(b) => PropertyValue::Boolean(*b),
        print_keys::Value::Integer(i) => PropertyValue::Integer(*i),
        print_keys::Value::Number(x) => PropertyValue::Number(*x),
        print_keys::Value::Percent(p) => {
            PropertyValue::Text(format!("{}%", print_keys::number_text(*p)))
        }
        print_keys::Value::Text(text) => PropertyValue::Text(text.clone()),
        print_keys::Value::Points(points) => PropertyValue::Text(print_keys::points_text(points)),
    }
}

/// A key's property value for a key whose form is text: numbers written
/// as text.
fn property_value_of(def: &KeyDef, value: &print_keys::Value) -> PropertyValue {
    match (property_form(def), value) {
        (PropertyForm::Standard(BaseType::Text, _), print_keys::Value::Number(x)) => {
            PropertyValue::Text(print_keys::number_text(*x))
        }
        _ => property_value(value),
    }
}

/// A key map value as the property of a key keeps it.
pub fn to_property(def: &KeyDef, value: &print_keys::Value) -> PropertyValue {
    property_value_of(def, value)
}

/// A property value of a key as the key map's value.
pub fn from_property(def: &KeyDef, value: &PropertyValue) -> Result<print_keys::Value, String> {
    let parsed = match (def.kind, value) {
        (Kind::Points, PropertyValue::Text(text)) => {
            print_keys::Value::Points(print_keys::parse_points(text).map_err(|e| e.to_string())?)
        }
        (Kind::Number(_) | Kind::Percent, PropertyValue::Integer(i)) => {
            print_keys::Value::Number(*i as f64)
        }
        _ => print_keys::Value::from_json(def, &value.to_json()).map_err(|e| e.to_string())?,
    };
    def.check(&parsed).map_err(|e| e.to_string())?;
    Ok(parsed)
}

/// A JSON value of a key (a command's, a profile's) as the key map's
/// value: also text in the key's own form (`0.45`, `110%`, `0x0,256x0,...`)
/// and a number for a whole-number key.
pub fn value_of(def: &KeyDef, value: &Value) -> Result<print_keys::Value, String> {
    if let (Kind::Points, Value::String(text)) = (def.kind, value) {
        let points =
            print_keys::Value::Points(print_keys::parse_points(text).map_err(|e| e.to_string())?);
        def.check(&points).map_err(|e| e.to_string())?;
        return Ok(points);
    }
    print_keys::Value::from_json(def, value).map_err(|e| e.to_string())
}

/// A new item's name without one given (mitcad#655), the same pattern for
/// every kind: `Printer 1`, then `Printer 2`, `Printer 3`, ... Never
/// `New …` (mitcad#828): beside 3D Print's New printer… a `New Printer`
/// read as the action twice.
pub fn new_item_name(kind: &str, n: usize) -> String {
    format!("{kind} {n}")
}

/// A copy's name (mitcad#828): its source's with ` (copy)`, then ` (copy
/// 2)`, ` (copy 3)`, ... the first that `taken` does not hold. A copy of a
/// copy counts on from its source's name: `PLA (copy)` copied is `PLA
/// (copy 2)`, not `PLA (copy) (copy)`.
pub fn copy_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let base = copy_base(name.trim());
    std::iter::once(format!("{base} (copy)"))
        .chain((2..).map(|n| format!("{base} (copy {n})")))
        .find(|candidate| !taken(candidate))
        .expect("a free name")
}

/// A name without a copy's ` (copy)` or ` (copy <n>)` at its end.
fn copy_base(name: &str) -> &str {
    let Some(open) = name.strip_suffix(')').and_then(|rest| rest.rfind(" (copy")) else {
        return name;
    };
    let inner = &name[open + " (copy".len()..name.len() - 1];
    let counted = inner
        .strip_prefix(' ')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    if (inner.is_empty() || counted) && open > 0 {
        &name[..open]
    } else {
        name
    }
}

/// An id from a name: lower-case letters and digits, other characters as
/// `-` (`Generic PLA @0.4` is `generic-pla-0-4`); `profile` when nothing
/// is left.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "profile".to_owned()
    } else {
        out
    }
}

/// `base`, else `base-2`, `base-3`, ... the first that `taken` does not
/// hold.
pub fn free_id(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken(candidate))
        .expect("a free id")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn profiles_check_their_values() {
        let mut p = PrintProfile::new("fine", "Fine", ProfileKind::Process);
        p.values.insert("layer_height".to_owned(), json!(0.12));
        p.values.insert("fill_density".to_owned(), json!(15));
        p.check().unwrap();
        assert_eq!(
            p.value("layer_height"),
            Some(print_keys::Value::Number(0.12))
        );
        p.values
            .insert("bed_shape".to_owned(), json!([[0, 0], [1, 0], [1, 1]]));
        assert!(p.check().unwrap_err().contains("Print Process"));
        p.values.remove("bed_shape");
        p.values.insert(
            "print_settings_id".to_owned(),
            json!({"prusaslicer": "0.15mm"}),
        );
        p.check().unwrap();
        p.raw.insert("bambu_studio".to_owned(), BTreeMap::new());
        assert!(p.check().is_err());
    }

    #[test]
    fn values_go_to_properties_and_back() {
        for (key, value) in [
            ("layer_height", json!(0.2)),
            ("fill_density", json!(20)),
            ("perimeters", json!(3)),
            ("first_layer_height", json!("75%")),
            ("bed_shape", json!([[0, 0], [250, 0], [250, 210], [0, 210]])),
        ] {
            let def = print_keys::key(key).unwrap();
            let parsed = value_of(def, &value).unwrap();
            let property = to_property(def, &parsed);
            assert_eq!(from_property(def, &property).unwrap(), parsed, "{key}");
        }
        let def = print_keys::key("bed_shape").unwrap();
        assert_eq!(
            to_property(def, &value_of(def, &json!("0x0,250x0,250x210")).unwrap()),
            PropertyValue::Text("0x0,250x0,250x210".to_owned())
        );
        assert!(matches!(
            property_form(print_keys::key("layer_height").unwrap()),
            PropertyForm::Standard(BaseType::Length, None)
        ));
        assert!(matches!(
            property_form(print_keys::key("fill_density").unwrap()),
            PropertyForm::Standard(BaseType::Number, Some("%"))
        ));
    }

    #[test]
    fn print_sets_are_known_by_name() {
        assert_eq!(
            print_set_of("print process"),
            Some(PrintPropertySet::Layer(PrintSet::Process))
        );
        assert_eq!(
            print_set_of("Print Slicer (PrusaSlicer)"),
            Some(PrintPropertySet::Slicer(Dialect::PrusaSlicer))
        );
        assert_eq!(
            print_set_of("Print Slicer (cura)"),
            Some(PrintPropertySet::Slicer(Dialect::Cura))
        );
        assert_eq!(print_set_of("Identity"), None);
        assert_eq!(slug(" Generic PLA @0.4 "), "generic-pla-0-4");
        assert_eq!(free_id("a", |id| id == "a" || id == "a-2"), "a-3");
    }

    #[test]
    fn per_slicer_values_fall_back_within_the_family() {
        let value = json!({"bambu_studio": "Bambu Lab X1 Carbon 0.4 nozzle"});
        assert_eq!(
            dialect_value(&value, Dialect::OrcaSlicer),
            Some(&json!("Bambu Lab X1 Carbon 0.4 nozzle"))
        );
        assert_eq!(dialect_value(&value, Dialect::PrusaSlicer), None);
    }

    #[test]
    fn new_and_copied_items_are_named() {
        // mitcad#828: never "New …"; a copy " (copy)", then " (copy 2)".
        assert_eq!(new_item_name("Process", 1), "Process 1");
        assert_eq!(new_item_name("Printer", 3), "Printer 3");
        let free = |_: &str| false;
        assert_eq!(copy_name("PLA", free), "PLA (copy)");
        assert_eq!(copy_name("PLA (copy)", free), "PLA (copy)");
        assert_eq!(copy_name("PLA (copy 4)", free), "PLA (copy)");
        let taken = |name: &str| name == "PLA (copy)" || name == "PLA (copy 2)";
        assert_eq!(copy_name("PLA", taken), "PLA (copy 3)");
        assert_eq!(copy_name("PLA (copy)", taken), "PLA (copy 3)");
        // Not a copy's mark: kept whole.
        assert_eq!(copy_name("Bench (copying)", free), "Bench (copying) (copy)");
        assert_eq!(copy_name("Bench (copy x)", free), "Bench (copy x) (copy)");
        assert_eq!(copy_name("(copy)", free), "(copy) (copy)");
    }
}
