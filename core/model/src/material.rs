// SPDX-License-Identifier: MIT
//! Materials (mitcad#640, part 2): what a body is made of, as an object of
//! its own. A material has a physical side (its density), a look (an
//! appearance of the library or the document) and, for printing, a
//! filament (`Print Filament` values of the key map, `crate::print_keys`).
//!
//! The built-in materials are the fixed list of densities the model always
//! had (`crate::analysis::MATERIALS`), with their ids (`steel`, `pla`,
//! ...), now each with an appearance and, for plastics, a filament type. A
//! design keeps materials of its own (`document/materials.rs`): made by
//! the user, imported, or copied from a library; one with a built-in's id
//! takes that built-in's place in the design. A copy remembers where it
//! came from (`origin`: the library, the id and the version), so that a
//! newer version in a library can be offered and a library change never
//! changes the design by itself.
//!
//! **Variants** (mitcad#1054): one filament is one material. Its settings
//! for different printers and nozzles are its variants: each names a
//! printer (as its printer profiles name it) and a nozzle diameter, and
//! holds only the filament values (and slicer keys) that differ from the
//! material's own, its base. One level, no deeper inheritance. The
//! printer a body prints with picks the variant ([`Material::variant_for`]):
//! the one of that printer and nozzle, else one of that printer for any
//! nozzle, else none (the base). A colour is never a variant's: filaments
//! whose colours print differently are materials of their own.
//!
//! **Appearances** (mitcad#1055): a filament's colour is its appearance,
//! as filaments are matte, glossy or translucent. A material offers a list
//! of appearances, its `appearance` the default among them
//! ([`Material::appearance_list`]); a body has a material and one of them
//! (`document/materials.rs`), and the slicer's filament colour is that
//! appearance's. A material keeps no `filament_colour` of its own: one
//! from before (a design, a library file, a command that sets it) becomes
//! an appearance of its list (`DocState::adopt_filament_colour`).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::analysis;
pub use crate::material_units::Quantity;
use crate::print_keys::{self, Dialect, PrintSet};
use crate::print_library::BasedOn;

/// A material: its id, name, density, appearance and filament.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    /// The id bodies and components refer to (`steel`, `pla`, `custom1`,
    /// a vendor's `<vendor>/<product>`).
    pub id: String,
    pub name: String,
    /// g/cm³.
    pub density: f64,
    /// The appearance a body gets with the material: an id of the
    /// library's or the document's appearances. It is the default of the
    /// appearances the material offers (mitcad#1055).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<String>,
    /// The other appearances it offers (mitcad#1055), in their order: a
    /// filament of several colours (PLA Basic in Bright Green, Black,
    /// White). Never the default. Designs and library files from before
    /// have none: the material offers its default alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub appearances: Vec<String>,
    /// The filament's print settings: values of keys of the `Print
    /// Filament` set by Mitcad key, in their JSON form (a key that names a
    /// dialect's own preset holds an object of a value per dialect id).
    /// The density is the material's own.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub filament: BTreeMap<String, Value>,
    /// A slicer's filament settings that have no counterpart in the key
    /// map, by slicer id: its key and text, written only to that slicer
    /// (mitcad#640, part 5).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub slicer: BTreeMap<String, BTreeMap<String, String>>,
    /// Its values for printers and nozzles that differ from these
    /// (mitcad#1054). Designs and library files from before have none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<MaterialVariant>,
    /// The material it was made from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
    /// Its class (mitcad#655, mitcad#1102): metal, plastic (General
    /// Plastic), fdm_plastic, resin, wood (mitcad#1552) or other, the
    /// group it is listed in.
    /// A material with print settings is an FDM plastic whatever it stores
    /// (a filament keeps `plastic`, which older versions read); without
    /// one (designs and library files before it) `material_class` works it
    /// out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<MaterialClass>,
    /// Its version in its library (counts up with each change there).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// Where the design's copy came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<MaterialOrigin>,
    /// Its mechanical and thermal properties (mitcad#1213), each optional.
    /// Designs and library files from before have none.
    #[serde(default, skip_serializing_if = "MaterialProperties::is_empty")]
    pub properties: MaterialProperties,
    /// Where its values came from (mitcad#1213): the bundled material
    /// dataset's material and grade, or a source the user named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<MaterialReference>,
}

/// A material's mechanical and thermal properties (mitcad#1213), each
/// optional: a value not given is none, never 0. In the model's units
/// (`crate::material_units`): MPa, °C and µm/(m·°C); the application shows
/// them in the design's. They are data: no analysis uses them yet.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialProperties {
    /// Young's modulus (the modulus of elasticity), MPa.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub youngs_modulus: Option<f64>,
    /// Poisson's ratio, without a unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poissons_ratio: Option<f64>,
    /// The yield strength, MPa.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yield_strength: Option<f64>,
    /// The highest temperature it serves at, °C.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_service_temperature: Option<f64>,
    /// The coefficient of thermal expansion, µm/(m·°C).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal_expansion: Option<f64>,
}

/// The group a material property is shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyGroup {
    Mechanical,
    Thermal,
}

impl PropertyGroup {
    /// Its id in queries: `mechanical` or `thermal`.
    pub fn id(self) -> &'static str {
        match self {
            PropertyGroup::Mechanical => "mechanical",
            PropertyGroup::Thermal => "thermal",
        }
    }

    /// Its title: Mechanical, Thermal.
    pub fn title(self) -> &'static str {
        match self {
            PropertyGroup::Mechanical => "Mechanical",
            PropertyGroup::Thermal => "Thermal",
        }
    }
}

/// A material property: its key in commands, files and queries, its name,
/// its group and what it measures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropertyDef {
    pub key: &'static str,
    pub name: &'static str,
    pub group: PropertyGroup,
    pub quantity: Quantity,
}

/// The material properties in the order they are shown (mitcad#1213):
/// Mechanical, then Thermal.
pub static PROPERTY_DEFS: [PropertyDef; 5] = [
    PropertyDef {
        key: "youngs_modulus",
        name: "Young's modulus",
        group: PropertyGroup::Mechanical,
        quantity: Quantity::Stress,
    },
    PropertyDef {
        key: "poissons_ratio",
        name: "Poisson's ratio",
        group: PropertyGroup::Mechanical,
        quantity: Quantity::Ratio,
    },
    PropertyDef {
        key: "yield_strength",
        name: "Yield strength",
        group: PropertyGroup::Mechanical,
        quantity: Quantity::Stress,
    },
    PropertyDef {
        key: "max_service_temperature",
        name: "Maximum service temperature",
        group: PropertyGroup::Thermal,
        quantity: Quantity::Temperature,
    },
    PropertyDef {
        key: "thermal_expansion",
        name: "Coefficient of thermal expansion",
        group: PropertyGroup::Thermal,
        quantity: Quantity::Expansion,
    },
];

/// A material property by its key.
pub fn property_def(key: &str) -> Option<&'static PropertyDef> {
    PROPERTY_DEFS.iter().find(|d| d.key == key)
}

/// The print settings' keys a material card shows as its Printing group
/// (mitcad#1213: the extruder and the bed temperature are the filament's
/// own values, no second set): the key, its name.
pub const PRINTING_KEYS: [(&str, &str); 2] = [
    ("temperature", "Extruder temperature"),
    ("bed_temperature", "Bed temperature"),
];

impl MaterialProperties {
    /// Whether no value is given.
    pub fn is_empty(&self) -> bool {
        PROPERTY_DEFS.iter().all(|d| self.get(d.key).is_none())
    }

    /// A value by its key (none for a key it has not, or one not given).
    pub fn get(&self, key: &str) -> Option<f64> {
        match key {
            "youngs_modulus" => self.youngs_modulus,
            "poissons_ratio" => self.poissons_ratio,
            "yield_strength" => self.yield_strength,
            "max_service_temperature" => self.max_service_temperature,
            "thermal_expansion" => self.thermal_expansion,
            _ => None,
        }
    }

    /// Sets a value by its key (none: not given); false for an unknown key.
    pub fn set(&mut self, key: &str, value: Option<f64>) -> bool {
        let slot = match key {
            "youngs_modulus" => &mut self.youngs_modulus,
            "poissons_ratio" => &mut self.poissons_ratio,
            "yield_strength" => &mut self.yield_strength,
            "max_service_temperature" => &mut self.max_service_temperature,
            "thermal_expansion" => &mut self.thermal_expansion,
            _ => return false,
        };
        *slot = value;
        true
    }

    /// Whether the values are valid: Young's modulus and the yield
    /// strength above 0, Poisson's ratio above -1 and at most 0.5, the
    /// service temperature above absolute zero, every value finite.
    pub fn check(&self) -> Result<(), String> {
        for def in &PROPERTY_DEFS {
            let Some(value) = self.get(def.key) else {
                continue;
            };
            check_property(def, value)?;
        }
        Ok(())
    }
}

/// Whether a value fits a property (in the model's unit): the reason in
/// plain words when not.
pub fn check_property(def: &PropertyDef, value: f64) -> Result<(), String> {
    let name = def.name;
    if !value.is_finite() {
        return Err(format!("{name} must be a number"));
    }
    match def.key {
        "youngs_modulus" | "yield_strength" if value <= 0.0 => {
            Err(format!("{name} must be greater than zero, got {value}"))
        }
        "poissons_ratio" if value <= -1.0 || value > 0.5 => Err(format!(
            "{name} must be greater than -1 and at most 0.5, got {value}"
        )),
        "max_service_temperature" if value <= -273.15 => Err(format!(
            "{name} must be above absolute zero (-273.15 °C), got {value} °C"
        )),
        _ => Ok(()),
    }
}

/// Where a material's values came from (mitcad#1213): a dataset or a
/// supplier's data sheet (`source`), the material and its grade there, and
/// the data's revision. A copy keeps it; the export and import of
/// materials carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialReference {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

/// A change of a material's properties in a command (mitcad#1213): the
/// values given; `null` takes one away.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertiesChange {
    #[serde(default, deserialize_with = "some")]
    pub youngs_modulus: Option<Option<f64>>,
    #[serde(default, deserialize_with = "some")]
    pub poissons_ratio: Option<Option<f64>>,
    #[serde(default, deserialize_with = "some")]
    pub yield_strength: Option<Option<f64>>,
    #[serde(default, deserialize_with = "some")]
    pub max_service_temperature: Option<Option<f64>>,
    #[serde(default, deserialize_with = "some")]
    pub thermal_expansion: Option<Option<f64>>,
}

impl PropertiesChange {
    /// The change that gives every value of `properties` and leaves the
    /// others as they are.
    pub fn giving(properties: &MaterialProperties) -> PropertiesChange {
        let mut change = PropertiesChange::default();
        for def in &PROPERTY_DEFS {
            if let Some(value) = properties.get(def.key) {
                change.set(def.key, Some(Some(value)));
            }
        }
        change
    }

    /// The change of a value by its key: none leaves it, `Some(None)`
    /// takes it away; false for an unknown key.
    pub fn set(&mut self, key: &str, change: Option<Option<f64>>) -> bool {
        let slot = match key {
            "youngs_modulus" => &mut self.youngs_modulus,
            "poissons_ratio" => &mut self.poissons_ratio,
            "yield_strength" => &mut self.yield_strength,
            "max_service_temperature" => &mut self.max_service_temperature,
            "thermal_expansion" => &mut self.thermal_expansion,
            _ => return false,
        };
        *slot = change;
        true
    }

    /// The change of a value by its key.
    pub fn get(&self, key: &str) -> Option<Option<f64>> {
        match key {
            "youngs_modulus" => self.youngs_modulus,
            "poissons_ratio" => self.poissons_ratio,
            "yield_strength" => self.yield_strength,
            "max_service_temperature" => self.max_service_temperature,
            "thermal_expansion" => self.thermal_expansion,
            _ => None,
        }
    }

    /// Applies the values given.
    pub fn apply_to(&self, properties: &mut MaterialProperties) {
        for def in &PROPERTY_DEFS {
            if let Some(value) = self.get(def.key) {
                properties.set(def.key, value);
            }
        }
    }
}

/// A material's class (mitcad#655, mitcad#1102): what it is made of, by
/// which the lists group it: Metal, General Plastic (`plastic`), FDM
/// Plastic (the filaments), Resin, Wood, Other.
pub use mitcad_model_base::material_class::MaterialClass;

/// Where a design's material came from: the library (`base`, `user`,
/// `project`, a 3MF import's `3mf`), its id there and the version copied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialOrigin {
    pub library: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// A newer library version the design kept its own values over
    /// ("Keep these", mitcad#655): not offered again until the library
    /// changes again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declined: Option<u32>,
}

/// A slicer's own keys and texts by slicer id.
pub type SlicerKeys = BTreeMap<String, BTreeMap<String, String>>;

/// A material's values for one printer and nozzle (mitcad#1054): only
/// those that differ from the material's own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialVariant {
    /// The printer, as its printer profiles name it: a profile's printer
    /// model or its name without the nozzle ([`PrinterKey`]).
    pub printer: String,
    /// The nozzle diameter in mm; none: any nozzle of the printer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nozzle: Option<f64>,
    /// Filament values by Mitcad key, as the material's `filament`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub filament: BTreeMap<String, Value>,
    /// A slicer's own keys by slicer id, as the material's `slicer`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub slicer: BTreeMap<String, BTreeMap<String, String>>,
}

impl MaterialVariant {
    /// An empty variant of a printer and nozzle.
    pub fn new(printer: &str, nozzle: Option<f64>) -> MaterialVariant {
        MaterialVariant {
            printer: printer.trim().to_owned(),
            nozzle,
            filament: BTreeMap::new(),
            slicer: BTreeMap::new(),
        }
    }

    /// Whether it is the variant of `printer` and `nozzle` (none: the one
    /// for any nozzle).
    pub fn is_for(&self, printer: &str, nozzle: Option<f64>) -> bool {
        same_printer(&self.printer, printer)
            && match (self.nozzle, nozzle) {
                (Some(a), Some(b)) => same_nozzle(a, b),
                (None, None) => true,
                _ => false,
            }
    }

    /// Its name for people: "Bench X, 0.4 mm nozzle", "Bench X, any
    /// nozzle".
    pub fn label(&self) -> String {
        variant_label(&self.printer, self.nozzle)
    }
}

/// A printer and nozzle's name for people: "Bench X, 0.4 mm nozzle".
pub fn variant_label(printer: &str, nozzle: Option<f64>) -> String {
    match nozzle {
        Some(n) => format!("{}, {n} mm nozzle", printer.trim()),
        None => format!("{}, any nozzle", printer.trim()),
    }
}

/// The printer a material's variant is picked by (mitcad#1054): the names
/// a printer profile goes by (its printer model, its name without the
/// nozzle, its slicers' preset names) and its nozzle diameter.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrinterKey {
    pub names: Vec<String>,
    pub nozzle: Option<f64>,
}

impl PrinterKey {
    /// A printer of one name.
    pub fn new(printer: &str, nozzle: Option<f64>) -> PrinterKey {
        PrinterKey {
            names: vec![printer.trim().to_owned()],
            nozzle,
        }
    }

    /// Whether the printer goes by `name`.
    pub fn is(&self, name: &str) -> bool {
        self.names.iter().any(|n| same_printer(n, name))
    }
}

/// How a variant was picked for a printer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantMatch {
    /// The variant of the printer and its nozzle.
    Exact,
    /// A variant of the printer for any nozzle, or for another one.
    Printer,
}

impl VariantMatch {
    pub fn id(self) -> &'static str {
        match self {
            VariantMatch::Exact => "exact",
            VariantMatch::Printer => "printer",
        }
    }
}

/// A printer's name in one form: lower case, single spaces.
fn printer_words(name: &str) -> String {
    name.split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether two names name the same printer (any case and spacing).
pub fn same_printer(a: &str, b: &str) -> bool {
    let a = printer_words(a);
    !a.is_empty() && a == printer_words(b)
}

/// Whether two nozzle diameters are the same (0.4 and 0.40).
pub fn same_nozzle(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// A diameter written as `0.4` or `0.4mm`.
fn diameter(word: &str) -> Option<f64> {
    let lower = word.trim().to_ascii_lowercase();
    let number = lower.strip_suffix("mm").unwrap_or(&lower);
    number
        .parse::<f64>()
        .ok()
        .filter(|n| *n > 0.0 && n.is_finite())
}

/// A nozzle phrase's diameter: `0.4 nozzle`, `0.4mm nozzle`, `0.4 mm
/// nozzle`.
fn nozzle_phrase(words: &[&str]) -> Option<(usize, f64)> {
    let (last, rest) = words.split_last()?;
    if !last.eq_ignore_ascii_case("nozzle") {
        return None;
    }
    match rest {
        [.., number, mm] if mm.eq_ignore_ascii_case("mm") => {
            diameter(number).map(|n| (words.len() - 3, n))
        }
        [.., number] => diameter(number).map(|n| (words.len() - 2, n)),
        [] => None,
    }
}

/// A printer preset's name without its nozzle, and the nozzle's diameter:
/// "Bench X 0.4 nozzle" is Bench X with 0.4 mm, as is "Bench X (0.4
/// nozzle)"; a name without one is itself.
pub fn split_nozzle(name: &str) -> (String, Option<f64>) {
    let trimmed = name.trim();
    if let Some(open) = trimmed.rfind('(')
        && trimmed.ends_with(')')
    {
        let inner: Vec<&str> = trimmed[open + 1..trimmed.len() - 1]
            .split_whitespace()
            .collect();
        if let Some((0, n)) = nozzle_phrase(&inner) {
            return (trim_separators(&trimmed[..open]), Some(n));
        }
    }
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if let Some((at, n)) = nozzle_phrase(&words)
        && at > 0
    {
        return (trim_separators(&words[..at].join(" ")), Some(n));
    }
    (trimmed.to_owned(), None)
}

fn trim_separators(text: &str) -> String {
    text.trim()
        .trim_end_matches(['-', ',', '/', '@'])
        .trim()
        .to_owned()
}

/// A filament preset's name as the filament's and what follows its `@`:
/// "Shop PLA @Bench X 0.6 nozzle" is Shop PLA for Bench X 0.6 nozzle.
pub fn split_filament_name(name: &str) -> (String, Option<String>) {
    if let Some(at) = name.find('@') {
        let base = name[..at].trim();
        let rest = name[at + 1..].trim();
        if !base.is_empty() {
            return (base.to_owned(), (!rest.is_empty()).then(|| rest.to_owned()));
        }
    }
    (name.trim().to_owned(), None)
}

/// A bare last word of a filament preset's `@` part read as a nozzle: a
/// decimal number in a nozzle's range, 0.1 to 2 mm ("0.6", "1.0",
/// "0.4mm"). A whole number is a model number, as the 3 of "Ender 3" or the
/// 2 of "Kobra 2" (mitcad#1147); a nozzle written as a whole number needs
/// the "… nozzle" phrase ("Bench X 1 nozzle").
fn bare_nozzle(word: &str) -> Option<f64> {
    diameter(word).filter(|n| word.contains('.') && (0.1..=2.0).contains(n))
}

/// The printer and nozzle a filament preset's `@` part names: "Bench X 0.6
/// nozzle", "MK4 0.6" (a bare diameter last), "Bench X"; "Ender 3" is a
/// printer, as a bare whole number is a model number, not a nozzle.
pub fn suffix_printer(suffix: &str) -> Option<(String, Option<f64>)> {
    let (printer, nozzle) = split_nozzle(suffix);
    if nozzle.is_some() {
        return (!printer.is_empty()).then_some((printer, nozzle));
    }
    let words: Vec<&str> = printer.split_whitespace().collect();
    if let [rest @ .., last] = words.as_slice()
        && !rest.is_empty()
        && let Some(n) = bare_nozzle(last)
    {
        return Some((rest.join(" "), Some(n)));
    }
    (!printer.is_empty()).then_some((printer, None))
}

/// A variant's change in `edit_material` (mitcad#1054): the variant of a
/// printer and nozzle, made when it has none; its filament values as
/// `filament` gives them (`null` removes one), or the variant removed.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantChange {
    pub printer: String,
    #[serde(default)]
    pub nozzle: Option<f64>,
    #[serde(default)]
    pub filament: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub remove: bool,
}

/// A change of a material in a command: the fields given; in
/// `edit_material`, `null` takes an optional field away and a filament
/// key's `null` removes that key.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialChange {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub density: Option<f64>,
    #[serde(default, deserialize_with = "some")]
    pub appearance: Option<Option<String>>,
    /// Every appearance it offers, in order (mitcad#1055), in the place of
    /// its list: the default stays when it is among them, else the first
    /// is the default (none when the list is empty). With `appearance`,
    /// that one is the default.
    #[serde(default)]
    pub appearances: Option<Vec<String>>,
    #[serde(default)]
    pub filament: Option<BTreeMap<String, Value>>,
    /// `create_material` only: the material whose fields it starts from,
    /// by id, or a library's item `{library, id}` (mitcad#655).
    #[serde(default)]
    pub based_on: Option<BasedOn>,
    /// The class (mitcad#655).
    #[serde(default)]
    pub class: Option<MaterialClass>,
    #[serde(default, deserialize_with = "some")]
    pub version: Option<Option<u32>>,
    #[serde(default, deserialize_with = "some")]
    pub origin: Option<Option<MaterialOrigin>>,
    /// The library the item lives in (`user` or `project`), when the
    /// command works there rather than on the design's own material
    /// (mitcad#655).
    #[serde(default)]
    pub library: Option<String>,
    /// The project's folder, when the design is in one.
    #[serde(default)]
    pub project: Option<String>,
    /// A slicer's own keys by slicer id (mitcad#1054); a key's `null`
    /// removes it.
    #[serde(default)]
    pub slicer: Option<BTreeMap<String, BTreeMap<String, Value>>>,
    /// Every variant, in the place of those it has (mitcad#1054).
    #[serde(default)]
    pub variants: Option<Vec<MaterialVariant>>,
    /// One variant changed, made or removed (mitcad#1054), after
    /// `variants`.
    #[serde(default)]
    pub variant: Option<VariantChange>,
    /// Its mechanical and thermal properties (mitcad#1213): those given;
    /// a property's `null` takes it away.
    #[serde(default)]
    pub properties: Option<PropertiesChange>,
    /// Where its values came from (mitcad#1213); `null` takes it away.
    #[serde(default, deserialize_with = "some")]
    pub reference: Option<Option<MaterialReference>>,
    /// `create_material` only: the bundled material dataset's entry
    /// (`aluminum/6061/t6`, `crate::material_dataset`) whose values it
    /// starts from (mitcad#1213), named as the entry (numbered when the
    /// name is taken) and remembering it in its `reference`.
    #[serde(default)]
    pub dataset: Option<String>,
}

/// A name `taken` does not hold: `name` itself, else `name (2)`, `name
/// (3)`, ... (mitcad#1213: a dataset's material made twice, an imported one
/// kept beside one of its name).
pub fn numbered_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let name = name.trim();
    std::iter::once(name.to_owned())
        .chain((2..).map(|n| format!("{name} ({n})")))
        .find(|candidate| !taken(candidate))
        .expect("a free name")
}

/// A field given as `null` is `Some(None)`; one left out stays `None`.
fn some<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl MaterialChange {
    /// Applies the given fields to a material (not `id` or `based_on`).
    pub(crate) fn apply_to(&self, material: &mut Material) {
        if let Some(name) = &self.name {
            material.name = name.trim().to_owned();
        }
        if let Some(density) = self.density {
            material.density = density;
        }
        if let Some(list) = &self.appearances {
            material.set_appearance_list(list);
        }
        if let Some(appearance) = &self.appearance {
            let id = appearance.as_ref().map(|a| a.trim().to_owned());
            match id {
                // One of its list becomes the default; the old default
                // stays in the list (mitcad#1055).
                Some(id) if material.appearances.contains(&id) => {
                    material.set_default_appearance(&id);
                }
                // Another replaces the default (as before lists), unless the
                // change gives the list too: then the list keeps them all.
                other => {
                    if self.appearances.is_some()
                        && let Some(old) = material.appearance.take()
                    {
                        material.appearances.insert(0, old);
                    }
                    material.appearance = other;
                }
            }
            material.normalize_appearances();
        }
        if let Some(filament) = &self.filament {
            for (key, value) in filament {
                if value.is_null() {
                    material.filament.remove(key);
                } else {
                    material.filament.insert(key.clone(), value.clone());
                }
            }
        }
        if let Some(version) = self.version {
            material.version = version;
        }
        if let Some(origin) = &self.origin {
            material.origin = origin.clone();
        }
        if let Some(slicer) = &self.slicer {
            for (dialect, keys) in slicer {
                let own = material.slicer.entry(dialect.clone()).or_default();
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
            material.slicer.retain(|_, keys| !keys.is_empty());
        }
        if let Some(variants) = &self.variants {
            material.variants = variants.clone();
        }
        if let Some(change) = &self.variant {
            let found = material
                .variants
                .iter()
                .position(|v| v.is_for(&change.printer, change.nozzle));
            if change.remove {
                if let Some(i) = found {
                    material.variants.remove(i);
                }
            } else {
                let i = found.unwrap_or_else(|| {
                    material
                        .variants
                        .push(MaterialVariant::new(&change.printer, change.nozzle));
                    material.variants.len() - 1
                });
                let variant = &mut material.variants[i];
                for (key, value) in change.filament.iter().flatten() {
                    if value.is_null() {
                        variant.filament.remove(key);
                    } else {
                        variant.filament.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        if let Some(properties) = &self.properties {
            properties.apply_to(&mut material.properties);
        }
        if let Some(reference) = &self.reference {
            material.reference = reference.clone();
        }
        if let Some(class) = self.class {
            material.class = Some(class);
        }
        // With print settings it is an FDM plastic all the same: stored as
        // `plastic`, which older versions read (mitcad#1102).
        if let Some(class) = material.class {
            material.class = Some(stored_class(material, class));
        }
    }
}

/// The class to store for a material (mitcad#1102): `class`, but `plastic`
/// for FDM Plastic when the material has print settings, which make it one
/// whatever it stores; `fdm_plastic` is written only when chosen for a
/// material without them.
pub fn stored_class(material: &Material, class: MaterialClass) -> MaterialClass {
    if class == MaterialClass::FdmPlastic && material.has_print_settings() {
        MaterialClass::Plastic
    } else {
        class
    }
}

/// A new material's look by its class (mitcad#1552): a material made from
/// nothing (no `based_on`, no `dataset`) of a class with a default
/// appearance (oak for Wood) starts with it, unless the command names an
/// appearance or a list of them.
pub fn give_class_appearance(
    material: &mut Material,
    class: MaterialClass,
    change: &MaterialChange,
) {
    if material.appearance.is_none() && change.appearance.is_none() && change.appearances.is_none()
    {
        material.appearance = class.default_appearance().map(str::to_owned);
    }
}

/// Bodies and components without a material of their own are the default
/// material's, steel.
pub use analysis::DEFAULT_MATERIAL;

/// The built-in materials' appearances and filament types, by id.
const BUILT_IN_LOOKS: &[(&str, Option<&str>, Option<&str>)] = &[
    ("steel", Some("steel_satin"), None),
    ("stainless_steel", Some("steel_polished"), None),
    ("cast_iron", Some("cast_iron"), None),
    ("aluminum", Some("aluminum_anodized"), None),
    ("copper", Some("copper"), None),
    ("brass", Some("brass_polished"), None),
    ("titanium", Some("steel_satin"), None),
    ("abs", Some("plastic_black"), Some("ABS")),
    ("pla", Some("plastic_white"), Some("PLA")),
    ("nylon", Some("plastic_white"), Some("PA")),
    ("polycarbonate", Some("plastic_clear"), Some("PC")),
    ("glass", Some("glass_clear"), None),
    ("water", None, None),
    ("resin_standard", Some("resin_grey"), None),
];

/// The built-in materials, in the application's order.
pub fn built_in() -> &'static [Material] {
    static BUILT_IN: LazyLock<Vec<Material>> = LazyLock::new(|| {
        analysis::MATERIALS
            .iter()
            .map(|m| {
                let (appearance, filament_type) = BUILT_IN_LOOKS
                    .iter()
                    .find(|(id, ..)| *id == m.id)
                    .map(|(_, a, f)| (*a, *f))
                    .unwrap_or((None, None));
                Material {
                    id: m.id.to_owned(),
                    name: m.name.to_owned(),
                    density: m.density,
                    appearance: appearance.map(str::to_owned),
                    filament: filament_type
                        .map(|t| BTreeMap::from([("filament_type".to_owned(), Value::from(t))]))
                        .unwrap_or_default(),
                    slicer: BTreeMap::new(),
                    variants: Vec::new(),
                    appearances: Vec::new(),
                    based_on: None,
                    class: None,
                    version: None,
                    origin: None,
                    properties: MaterialProperties::default(),
                    reference: None,
                }
            })
            .collect()
    });
    &BUILT_IN
}

/// A built-in material by id.
pub fn built_in_material(id: &str) -> Option<&'static Material> {
    built_in().iter().find(|m| m.id == id)
}

/// The built-in metals.
const METALS: [&str; 7] = [
    "steel",
    "stainless_steel",
    "cast_iron",
    "aluminum",
    "copper",
    "brass",
    "titanium",
];

/// The built-in resins (mitcad#1102).
const RESINS: [&str; 1] = ["resin_standard"];

/// A material's class (mitcad#655, mitcad#1102): with print settings
/// (filament values, a slicer's own keys or variants per printer) an FDM
/// plastic, whatever it stores; else its own; without one (a design or a
/// library file from before classes) a built-in metal's or resin's id, or
/// the built-in it is based on, makes it a metal or a resin, being based
/// on a built-in plastic a General Plastic, else other. Worked out when
/// read: the stored class is not rewritten.
pub fn material_class(material: &Material) -> MaterialClass {
    if material.has_print_settings() {
        return MaterialClass::FdmPlastic;
    }
    if let Some(class) = material.class {
        return class;
    }
    let metal = |id: &str| METALS.contains(&id);
    let resin = |id: &str| RESINS.contains(&id);
    if metal(&material.id) || material.based_on.as_deref().is_some_and(metal) {
        MaterialClass::Metal
    } else if resin(&material.id) || material.based_on.as_deref().is_some_and(resin) {
        MaterialClass::Resin
    } else if material
        .based_on
        .as_deref()
        .and_then(built_in_material)
        .is_some_and(|base| base.has_print_settings())
    {
        MaterialClass::Plastic
    } else {
        MaterialClass::Other
    }
}

/// The group a material is listed in (mitcad#655): its class's, `metals`,
/// `plastics` (General Plastic), `fdm_plastics` (the filaments), `resins`,
/// `wood` or `other`. Print settings make it an FDM plastic (mitcad#1102).
pub fn material_group(material: &Material) -> &'static str {
    material_class(material).group()
}

/// Whether an id can name a material: letters, digits, `_`, `-`, and `/`
/// between parts (`<vendor>/<product>`).
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.split('/').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

impl Material {
    /// Whether it has print settings (mitcad#1102): filament values, a
    /// slicer's own keys or variants per printer. Such a material is an
    /// FDM plastic ([`material_class`]).
    pub fn has_print_settings(&self) -> bool {
        !self.filament.is_empty() || !self.slicer.is_empty() || !self.variants.is_empty()
    }

    /// The appearances it offers (mitcad#1055): its default first, then
    /// the others in their order; the default alone without a list; none
    /// when it has no appearance.
    pub fn appearance_list(&self) -> Vec<String> {
        let mut list: Vec<String> = Vec::new();
        for id in self.appearance.iter().chain(&self.appearances) {
            if !id.is_empty() && !list.contains(id) {
                list.push(id.clone());
            }
        }
        list
    }

    /// Whether it offers an appearance (its default or one of its list).
    pub fn offers(&self, appearance: &str) -> bool {
        self.appearance.as_deref() == Some(appearance)
            || self.appearances.iter().any(|a| a == appearance)
    }

    /// Names the appearance `from` as `to` in its list, its place kept (a
    /// library item's appearance that has another id in the design,
    /// mitcad#1067).
    pub fn rename_appearance(&mut self, from: &str, to: &str) {
        for id in self
            .appearance
            .iter_mut()
            .chain(self.appearances.iter_mut())
        {
            if id.as_str() == from {
                to.clone_into(id);
            }
        }
    }

    /// Adds an appearance to the ones it offers (the default when it has
    /// none); nothing when it offers it already.
    pub fn add_appearance(&mut self, appearance: &str) {
        if self.offers(appearance) {
            return;
        }
        if self.appearance.is_none() {
            self.appearance = Some(appearance.to_owned());
        } else {
            self.appearances.push(appearance.to_owned());
        }
    }

    /// Its list as `list` gives it (the default kept when it is among
    /// them, else the first the default; none when the list is empty).
    pub fn set_appearance_list(&mut self, list: &[String]) {
        let mut ids: Vec<String> = Vec::new();
        for id in list.iter().map(|a| a.trim()) {
            if !ids.iter().any(|a| a == id) {
                ids.push(id.to_owned());
            }
        }
        let keep = self
            .appearance
            .as_ref()
            .is_some_and(|default| ids.contains(default));
        if !keep {
            self.appearance = if ids.is_empty() {
                None
            } else {
                Some(ids[0].clone())
            };
        }
        let default = self.appearance.clone();
        ids.retain(|a| Some(a) != default.as_ref());
        self.appearances = ids;
    }

    /// One of its list becomes the default; the old default takes its
    /// place in the list.
    pub fn set_default_appearance(&mut self, appearance: &str) {
        let Some(at) = self.appearances.iter().position(|a| a == appearance) else {
            return;
        };
        match self.appearance.replace(appearance.to_owned()) {
            Some(old) => self.appearances[at] = old,
            None => {
                self.appearances.remove(at);
            }
        }
    }

    /// The list without the default or an id twice, and a default when
    /// it has a list.
    pub fn normalize_appearances(&mut self) {
        if self.appearance.is_none() && !self.appearances.is_empty() {
            self.appearance = Some(self.appearances.remove(0));
        }
        let mut seen: Vec<String> = self.appearance.iter().cloned().collect();
        self.appearances.retain(|a| {
            if seen.contains(a) {
                false
            } else {
                seen.push(a.clone());
                true
            }
        });
    }

    /// Whether the fields are valid (the name and appearance not empty,
    /// the density positive, the filament's keys and values those of the
    /// `Print Filament` set). Whether the appearance exists is the
    /// document's to say.
    pub fn check(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(format!(
                "{} is no material id: use letters, digits, '_', '-' and '/' between parts",
                self.id
            ));
        }
        if self.name.trim().is_empty() {
            return Err(format!("material {}: the name is empty", self.id));
        }
        if !(self.density > 0.0 && self.density.is_finite()) {
            return Err(format!(
                "material {}: the density must be greater than zero, got {}",
                self.id, self.density
            ));
        }
        if self.appearance.as_deref().is_some_and(|a| a.is_empty())
            || self.appearances.iter().any(|a| a.trim().is_empty())
        {
            return Err(format!("material {}: the appearance id is empty", self.id));
        }
        // Its properties (mitcad#1213).
        self.properties
            .check()
            .map_err(|e| format!("material {}: {e}", self.id))?;
        if self
            .reference
            .as_ref()
            .is_some_and(|r| r.source.trim().is_empty())
        {
            return Err(format!(
                "material {}: the reference names no source",
                self.id
            ));
        }
        for (key, value) in &self.filament {
            check_filament_value(key, value)
                .map_err(|e| format!("material {}: filament: {e}", self.id))?;
        }
        if let Some(dialect) = self.slicer.keys().find(|d| Dialect::from_id(d).is_none()) {
            return Err(format!("material {}: {dialect} is no slicer id", self.id));
        }
        for (i, variant) in self.variants.iter().enumerate() {
            let what = format!("material {}: variant {}", self.id, variant.label());
            if variant.printer.trim().is_empty() {
                return Err(format!("material {}: a variant names no printer", self.id));
            }
            if variant.nozzle.is_some_and(|n| !(n > 0.0 && n.is_finite())) {
                return Err(format!(
                    "{what}: the nozzle diameter must be greater than zero"
                ));
            }
            for (key, value) in &variant.filament {
                // Colours that print differently are materials of their
                // own (mitcad#1054).
                if key == "filament_colour" {
                    return Err(format!(
                        "{what}: the colour is the material's own; a filament whose colours print \
                         differently is a material per colour"
                    ));
                }
                check_filament_value(key, value).map_err(|e| format!("{what}: {e}"))?;
            }
            if let Some(dialect) = variant
                .slicer
                .keys()
                .find(|d| Dialect::from_id(d).is_none())
            {
                return Err(format!("{what}: {dialect} is no slicer id"));
            }
            if self.variants[..i]
                .iter()
                .any(|other| other.is_for(&variant.printer, variant.nozzle))
            {
                return Err(format!("{what} is there twice"));
            }
        }
        Ok(())
    }

    /// The variant a printer picks (mitcad#1054): the one of the printer
    /// and its nozzle, else one of the printer for any nozzle, else the
    /// printer's of the nearest nozzle; none: the material's own values.
    pub fn variant_for(&self, printer: &PrinterKey) -> Option<(usize, VariantMatch)> {
        let mine: Vec<usize> = (0..self.variants.len())
            .filter(|&i| printer.is(&self.variants[i].printer))
            .collect();
        if let Some(nozzle) = printer.nozzle
            && let Some(&i) = mine.iter().find(|&&i| {
                self.variants[i]
                    .nozzle
                    .is_some_and(|own| same_nozzle(own, nozzle))
            })
        {
            return Some((i, VariantMatch::Exact));
        }
        if let Some(&i) = mine.iter().find(|&&i| self.variants[i].nozzle.is_none()) {
            return Some((i, VariantMatch::Printer));
        }
        let distance = |i: usize| match (printer.nozzle, self.variants[i].nozzle) {
            (Some(wanted), Some(own)) => (wanted - own).abs(),
            _ => f64::INFINITY,
        };
        mine.iter()
            .copied()
            .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
            .map(|i| (i, VariantMatch::Printer))
    }

    /// The filament values and the slicers' own keys with a variant's
    /// over the material's own (`variant`: an index of `variants`).
    pub fn values_with(&self, variant: Option<usize>) -> (BTreeMap<String, Value>, SlicerKeys) {
        let mut filament = self.filament.clone();
        let mut slicer = self.slicer.clone();
        if let Some(v) = variant.and_then(|i| self.variants.get(i)) {
            for (key, value) in &v.filament {
                filament.insert(key.clone(), value.clone());
            }
            for (dialect, keys) in &v.slicer {
                let own = slicer.entry(dialect.clone()).or_default();
                for (key, text) in keys {
                    own.insert(key.clone(), text.clone());
                }
            }
        }
        (filament, slicer)
    }

    /// The filament's value of a key, as the key map's value (None when
    /// not set, or a key kept per dialect).
    pub fn filament_value(&self, key: &str) -> Option<print_keys::Value> {
        let def = print_keys::key(key)?;
        if def.per_dialect {
            return None;
        }
        print_keys::Value::from_json(def, self.filament.get(key)?).ok()
    }
}

/// Whether a filament value fits its key: a key of the `Print Filament`
/// set (not `filament_density`, which is the material's density), its
/// kind; a key kept per dialect holds `{"<dialect id>": value, ...}`.
pub fn check_filament_value(key: &str, value: &Value) -> Result<(), String> {
    let def = print_keys::key(key)
        .filter(|def| def.set == PrintSet::Filament)
        .ok_or_else(|| format!("{key} is no key of the Print Filament set"))?;
    if key == "filament_density" {
        return Err("the filament's density is the material's density".to_owned());
    }
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
    let parsed = print_keys::Value::from_json(def, value).map_err(|e| e.to_string())?;
    // A purge volume (mitcad#705) is zero or more.
    if print_keys::MITCAD_OWN.contains(&key)
        && let print_keys::Value::Number(volume) = parsed
    {
        crate::print_purge::check_volume(volume).map_err(|e| format!("{key}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn built_in_materials_keep_their_ids_with_looks() {
        assert_eq!(built_in().len(), analysis::MATERIALS.len());
        let pla = built_in_material("pla").unwrap();
        assert_eq!(pla.density, 1.24);
        assert_eq!(pla.appearance.as_deref(), Some("plastic_white"));
        assert_eq!(pla.filament["filament_type"], json!("PLA"));
        assert_eq!(
            built_in_material(DEFAULT_MATERIAL)
                .unwrap()
                .appearance
                .as_deref(),
            Some("steel_satin")
        );
        for m in built_in() {
            m.check().unwrap();
            if let Some(a) = &m.appearance {
                assert!(crate::appearance::library_appearance(a).is_some(), "{a}");
            }
        }
    }

    #[test]
    fn ids_and_filament_values_are_checked() {
        assert!(valid_id("acme/petg-hf"));
        assert!(!valid_id("acme//petg") && !valid_id("/petg") && !valid_id("pe tg"));
        let mut m = built_in_material("pla").unwrap().clone();
        m.filament.insert("temperature".to_owned(), json!(215));
        m.filament
            .insert("filament_colour".to_owned(), json!("#ff0000"));
        m.check().unwrap();
        assert_eq!(
            m.filament_value("temperature"),
            Some(print_keys::Value::Number(215.0))
        );
        m.filament.insert("layer_height".to_owned(), json!(0.2));
        assert!(m.check().unwrap_err().contains("Print Filament"));
        m.filament.remove("layer_height");
        m.filament.insert("filament_density".to_owned(), json!(1.2));
        assert!(m.check().is_err());
        m.filament.remove("filament_density");
        m.filament.insert(
            "filament_settings_id".to_owned(),
            json!({"prusaslicer": "Generic PLA"}),
        );
        m.check().unwrap();
        m.filament
            .insert("filament_settings_id".to_owned(), json!({"nobody": "x"}));
        assert!(m.check().is_err());
    }

    #[test]
    fn materials_are_grouped_by_their_class() {
        let group = |id: &str| material_group(built_in_material(id).unwrap());
        assert_eq!(group("steel"), "metals");
        assert_eq!(group("brass"), "metals");
        assert_eq!(group("pla"), "fdm_plastics");
        assert_eq!(group("glass"), "other");
        assert_eq!(group("water"), "other");
        assert_eq!(group("resin_standard"), "resins");
        // A material of its own with filament settings is an FDM plastic;
        // without them a General Plastic when based on a built-in plastic;
        // one based on a metal a metal (mitcad#1102).
        let mut petg = built_in_material("pla").unwrap().clone();
        petg.id = "acme_petg".to_owned();
        assert_eq!(material_class(&petg), MaterialClass::FdmPlastic);
        petg.filament.clear();
        assert_eq!(material_class(&petg), MaterialClass::Other);
        petg.based_on = Some("pla".to_owned());
        assert_eq!(material_class(&petg), MaterialClass::Plastic);
        let mut own_resin = built_in_material("resin_standard").unwrap().clone();
        own_resin.id = "shop_resin".to_owned();
        own_resin.based_on = Some("resin_standard".to_owned());
        assert_eq!(material_group(&own_resin), "resins");
        let mut own_steel = built_in_material("steel").unwrap().clone();
        own_steel.id = "shop_steel".to_owned();
        own_steel.based_on = Some("steel".to_owned());
        assert_eq!(material_group(&own_steel), "metals");
    }

    /// A PLA with a base and two variants (mitcad#1054): Bench X with a
    /// 0.4 mm nozzle and Bench Y for any nozzle.
    fn pla_with_variants() -> Material {
        let mut m = built_in_material("pla").unwrap().clone();
        m.id = "shop_pla".to_owned();
        m.filament.insert("temperature".to_owned(), json!(210));
        m.filament.insert("bed_temperature".to_owned(), json!(60));
        let mut x = MaterialVariant::new("Bench X", Some(0.4));
        x.filament.insert("temperature".to_owned(), json!(220));
        let mut y = MaterialVariant::new("Bench Y", None);
        y.filament.insert("temperature".to_owned(), json!(205));
        y.filament.insert("bed_temperature".to_owned(), json!(55));
        m.variants = vec![x, y];
        m
    }

    fn temperatures(m: &Material, printer: &PrinterKey) -> (Value, Value, Option<VariantMatch>) {
        let picked = m.variant_for(printer);
        let (filament, _) = m.values_with(picked.map(|(i, _)| i));
        (
            filament["temperature"].clone(),
            filament["bed_temperature"].clone(),
            picked.map(|(_, how)| how),
        )
    }

    #[test]
    fn a_printer_and_nozzle_pick_the_variant_else_the_base() {
        let m = pla_with_variants();
        m.check().unwrap();
        // Bench X with its 0.4 nozzle: its variant over the base.
        assert_eq!(
            temperatures(&m, &PrinterKey::new("Bench X", Some(0.4))),
            (json!(220), json!(60), Some(VariantMatch::Exact))
        );
        // Bench X with another nozzle: the printer's variant still.
        assert_eq!(
            temperatures(&m, &PrinterKey::new("bench  x", Some(0.6))),
            (json!(220), json!(60), Some(VariantMatch::Printer))
        );
        // Bench Y, any nozzle.
        assert_eq!(
            temperatures(&m, &PrinterKey::new("Bench Y", Some(0.8))),
            (json!(205), json!(55), Some(VariantMatch::Printer))
        );
        // An unknown printer, or none: the base.
        assert_eq!(
            temperatures(&m, &PrinterKey::new("Other", Some(0.4))),
            (json!(210), json!(60), None)
        );
        assert_eq!(
            temperatures(&m, &PrinterKey::default()),
            (json!(210), json!(60), None)
        );
        // With a variant of the nozzle too, it wins over the one for any.
        let mut more = m.clone();
        let mut y6 = MaterialVariant::new("Bench Y", Some(0.6));
        y6.filament.insert("temperature".to_owned(), json!(235));
        more.variants.push(y6);
        assert_eq!(
            temperatures(&more, &PrinterKey::new("Bench Y", Some(0.6))),
            (json!(235), json!(60), Some(VariantMatch::Exact))
        );
    }

    #[test]
    fn variants_are_checked_and_old_files_have_none() {
        let mut m = pla_with_variants();
        m.variants[0]
            .filament
            .insert("filament_colour".to_owned(), json!("#FF0000"));
        assert!(m.check().unwrap_err().contains("colour"));
        let mut m = pla_with_variants();
        m.variants[1].nozzle = Some(0.0);
        assert!(m.check().is_err());
        let mut m = pla_with_variants();
        m.variants.push(MaterialVariant::new("BENCH X", Some(0.40)));
        assert!(m.check().unwrap_err().contains("twice"));
        // A file from before variants reads as it was, and one with them
        // round-trips.
        let old: Material = serde_json::from_value(
            json!({"id": "x", "name": "X", "density": 1.2, "filament": {"temperature": 200}}),
        )
        .unwrap();
        assert!(old.variants.is_empty());
        assert!(
            serde_json::to_value(&old)
                .unwrap()
                .get("variants")
                .is_none()
        );
        let m = pla_with_variants();
        let back: Material = serde_json::from_value(serde_json::to_value(&m).unwrap()).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn a_change_edits_makes_and_removes_one_variant() {
        let mut m = pla_with_variants();
        let change: MaterialChange = serde_json::from_value(json!({
            "variant": {"printer": "Bench X", "nozzle": 0.4,
                        "filament": {"temperature": null, "max_fan_speed": 80}}}))
        .unwrap();
        change.apply_to(&mut m);
        assert_eq!(
            m.variants[0].filament,
            BTreeMap::from([("max_fan_speed".to_owned(), json!(80))])
        );
        let change: MaterialChange = serde_json::from_value(json!({
            "variant": {"printer": "Bench Z", "nozzle": 0.6, "filament": {"temperature": 230}}}))
        .unwrap();
        change.apply_to(&mut m);
        assert_eq!(m.variants.len(), 3);
        assert_eq!(m.variants[2].label(), "Bench Z, 0.6 mm nozzle");
        let change: MaterialChange =
            serde_json::from_value(json!({"variant": {"printer": "Bench Y", "remove": true}}))
                .unwrap();
        change.apply_to(&mut m);
        assert_eq!(m.variants.len(), 2);
        m.check().unwrap();
    }

    #[test]
    fn preset_names_give_the_filament_printer_and_nozzle() {
        assert_eq!(
            split_nozzle("Bench X 0.4 nozzle"),
            ("Bench X".to_owned(), Some(0.4))
        );
        assert_eq!(
            split_nozzle("Bench X (0.6 nozzle)"),
            ("Bench X".to_owned(), Some(0.6))
        );
        assert_eq!(
            split_nozzle("Bench X 0.25mm nozzle"),
            ("Bench X".to_owned(), Some(0.25))
        );
        assert_eq!(
            split_nozzle("Bench X 0.8 mm nozzle"),
            ("Bench X".to_owned(), Some(0.8))
        );
        assert_eq!(split_nozzle("Bench X"), ("Bench X".to_owned(), None));
        assert_eq!(
            split_filament_name("Shop PLA @Bench X 0.6 nozzle"),
            ("Shop PLA".to_owned(), Some("Bench X 0.6 nozzle".to_owned()))
        );
        assert_eq!(split_filament_name("Amber"), ("Amber".to_owned(), None));
        assert_eq!(
            suffix_printer("Bench X 0.6 nozzle"),
            Some(("Bench X".to_owned(), Some(0.6)))
        );
        assert_eq!(
            suffix_printer("MK4 0.6"),
            Some(("MK4".to_owned(), Some(0.6)))
        );
        assert_eq!(
            suffix_printer("Bench X"),
            Some(("Bench X".to_owned(), None))
        );
        // A trailing whole number is the printer's model number, not a
        // nozzle (mitcad#1147); a decimal one in a nozzle's range is.
        assert_eq!(
            suffix_printer("Ender 3"),
            Some(("Ender 3".to_owned(), None))
        );
        assert_eq!(
            suffix_printer("Kobra 2"),
            Some(("Kobra 2".to_owned(), None))
        );
        assert_eq!(
            suffix_printer("Bench X 1.0"),
            Some(("Bench X".to_owned(), Some(1.0)))
        );
        assert_eq!(
            suffix_printer("Bench X 0.4mm"),
            Some(("Bench X".to_owned(), Some(0.4)))
        );
        assert_eq!(
            suffix_printer("Bench X 2.5"),
            Some(("Bench X 2.5".to_owned(), None))
        );
        assert_eq!(
            suffix_printer("Bench X 1 nozzle"),
            Some(("Bench X".to_owned(), Some(1.0)))
        );
        assert!(same_printer("Bench  X", "bench x") && !same_printer("", ""));
    }

    #[test]
    fn a_stored_class_wins_unless_print_settings_make_it_fdm() {
        let mut m = built_in_material("glass").unwrap().clone();
        m.id = "cast_resin".to_owned();
        m.class = Some(MaterialClass::Plastic);
        assert_eq!(material_group(&m), "plastics");
        // A stored `plastic` without print settings stays General Plastic
        // and is written back as `plastic` (mitcad#1102).
        let text = serde_json::to_value(&m).unwrap();
        assert_eq!(text["class"], json!("plastic"));
        // With filament settings it reads as FDM Plastic; the file keeps
        // `plastic`.
        m.filament.insert("filament_type".to_owned(), json!("PLA"));
        assert_eq!(material_class(&m), MaterialClass::FdmPlastic);
        let text = serde_json::to_value(&m).unwrap();
        assert_eq!(text["class"], json!("plastic"));
        let back: Material = serde_json::from_value(text).unwrap();
        assert_eq!(back.class, Some(MaterialClass::Plastic));
        assert_eq!(material_group(&back), "fdm_plastics");
        // Choosing FDM Plastic for it stores `plastic`; another class is
        // stored, but print settings still make it an FDM plastic.
        let change: MaterialChange =
            serde_json::from_value(json!({"class": "fdm_plastic"})).unwrap();
        change.apply_to(&mut m);
        assert_eq!(m.class, Some(MaterialClass::Plastic));
        m.class = Some(MaterialClass::Other);
        assert_eq!(material_group(&m), "fdm_plastics");
        // Without print settings `fdm_plastic` and `resin` are written and
        // round-trip.
        m.filament.clear();
        for class in ["fdm_plastic", "resin"] {
            let change: MaterialChange = serde_json::from_value(json!({"class": class})).unwrap();
            change.apply_to(&mut m);
            let text = serde_json::to_value(&m).unwrap();
            assert_eq!(text["class"], json!(class));
            let back: Material = serde_json::from_value(text).unwrap();
            assert_eq!(back, m);
            assert_eq!(material_class(&back).id(), class);
        }
        // A variant per printer is a print setting too.
        m.variants.push(MaterialVariant::new("Bench X", Some(0.4)));
        assert_eq!(material_class(&m), MaterialClass::FdmPlastic);
        // A file from before classes has none.
        let old: Material =
            serde_json::from_value(json!({"id": "x", "name": "X", "density": 1.0})).unwrap();
        assert_eq!(old.class, None);
        assert_eq!(material_class(&old), MaterialClass::Other);
    }

    /// mitcad#1055: the appearances a material offers, its default first;
    /// one of them made the default, the list replaced, one added.
    #[test]
    fn a_material_offers_its_appearances_its_default_first() {
        let mut m = built_in_material("pla").unwrap().clone();
        assert_eq!(m.appearance_list(), ["plastic_white"]);
        assert!(m.offers("plastic_white") && !m.offers("plastic_black"));
        m.add_appearance("plastic_black");
        m.add_appearance("plastic_black");
        assert_eq!(m.appearance_list(), ["plastic_white", "plastic_black"]);
        assert_eq!(m.appearances, ["plastic_black"]);
        m.set_default_appearance("plastic_black");
        assert_eq!(m.appearance.as_deref(), Some("plastic_black"));
        assert_eq!(m.appearances, ["plastic_white"]);
        let list = ["plastic_red".to_owned(), "plastic_white".to_owned()];
        m.set_appearance_list(&list);
        assert_eq!(m.appearance.as_deref(), Some("plastic_red"));
        assert_eq!(m.appearance_list(), ["plastic_red", "plastic_white"]);
        // A change of the default to one of the list keeps the old one.
        let change: MaterialChange =
            serde_json::from_value(json!({"appearance": "plastic_white"})).unwrap();
        change.apply_to(&mut m);
        assert_eq!(m.appearance_list(), ["plastic_white", "plastic_red"]);
        // Another replaces it, as before lists.
        let change: MaterialChange =
            serde_json::from_value(json!({"appearance": "plastic_blue"})).unwrap();
        change.apply_to(&mut m);
        assert_eq!(m.appearance_list(), ["plastic_blue", "plastic_red"]);
        m.check().unwrap();
        // Without a default, a material with a list gets its first.
        let mut bare: Material =
            serde_json::from_value(json!({"id": "x", "name": "X", "density": 1.0,
                                          "appearances": ["plastic_red", "plastic_red"]}))
            .unwrap();
        bare.normalize_appearances();
        assert_eq!(bare.appearance.as_deref(), Some("plastic_red"));
        assert!(bare.appearances.is_empty());
        bare.appearances.push(" ".to_owned());
        assert!(bare.check().is_err());
    }

    /// mitcad#1213: a material's properties are optional, checked, given
    /// and taken away by a change, and a file from before has none.
    #[test]
    fn properties_are_optional_checked_and_changed() {
        let mut m = built_in_material("steel").unwrap().clone();
        assert!(m.properties.is_empty());
        assert!(
            serde_json::to_value(&m)
                .unwrap()
                .get("properties")
                .is_none()
        );
        let change: MaterialChange = serde_json::from_value(json!({
            "properties": {"youngs_modulus": 200000, "poissons_ratio": 0.29,
                           "thermal_expansion": 12},
            "reference": {"source": "Shop sheet", "grade": "S235"}}))
        .unwrap();
        change.apply_to(&mut m);
        m.check().unwrap();
        assert_eq!(m.properties.youngs_modulus, Some(200000.0));
        assert_eq!(m.properties.get("thermal_expansion"), Some(12.0));
        assert_eq!(m.properties.yield_strength, None);
        let text = serde_json::to_value(&m).unwrap();
        assert_eq!(
            text["properties"],
            json!({"youngs_modulus": 200000.0, "poissons_ratio": 0.29, "thermal_expansion": 12.0})
        );
        let back: Material = serde_json::from_value(text).unwrap();
        assert_eq!(back, m);
        // `null` takes one away; the others stay.
        let change: MaterialChange =
            serde_json::from_value(json!({"properties": {"poissons_ratio": null}})).unwrap();
        change.apply_to(&mut m);
        assert_eq!(m.properties.poissons_ratio, None);
        assert_eq!(m.properties.youngs_modulus, Some(200000.0));
        // A value out of range, 0 or an unknown key is refused.
        for (key, value) in [
            ("youngs_modulus", 0.0),
            ("yield_strength", -5.0),
            ("poissons_ratio", 0.6),
            ("max_service_temperature", -300.0),
        ] {
            let mut bad = m.clone();
            bad.properties.set(key, Some(value));
            assert!(bad.check().is_err(), "{key} = {value}");
        }
        assert!(
            serde_json::from_value::<MaterialChange>(json!({"properties": {"hardness": 3}}))
                .is_err()
        );
        let mut nameless = m.clone();
        nameless.reference = Some(MaterialReference {
            source: " ".to_owned(),
            material: None,
            grade: None,
            revision: None,
        });
        assert!(nameless.check().is_err());
        // A file from before properties reads as it was.
        let old: Material =
            serde_json::from_value(json!({"id": "x", "name": "X", "density": 1.0})).unwrap();
        assert!(old.properties.is_empty() && old.reference.is_none());
    }
}
