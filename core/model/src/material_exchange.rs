// SPDX-License-Identifier: MIT
//! Materials as files (mitcad#1213): the export and import of materials as
//! JSON and CSV, in the format `docs/material-files.md` documents.
//!
//! A file states its units: a JSON file in its `units` object (`density`,
//! `stress`, `temperature`, `expansion`), a CSV file in its header, each
//! column of a quantity as `density [g/cm3]`. The export writes the units
//! it is asked for (the design's system and the user's temperature unit);
//! the import reads any of the units [`parse_unit`] knows and converts the
//! values into the model's. Each material is checked on its own: a row
//! that is not valid is reported with its place in the file and the
//! reason, and the others are read. A file the import cannot read at all
//! (not JSON, another format, a header without a name column) is an error
//! of its own.
//!
//! A material's printing values are its print settings' (mitcad#1213:
//! no second set): `extruder_temperature` is the filament's `temperature`,
//! `extruder_temperature_min` and `_max` its recommended range,
//! `bed_temperature` its `bed_temperature`. Its reference (`source`,
//! `material`, `grade`, `revision`) goes with it. Its id is written for
//! people and tools; the import names materials by their names.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::material::{
    self, Material, MaterialChange, MaterialClass, MaterialProperties, MaterialReference,
    PropertiesChange, property_def,
};
use crate::material_units::{
    Quantity, ShownUnit, TemperatureUnit, UnitSystem, file_unit, parse_unit, shown_unit,
    significant,
};
use crate::parameters_csv::{records, separator, write_record};

/// A JSON materials file's `format`.
pub const FORMAT: &str = "mitcad-materials";
/// The version of the format this model writes and reads.
pub const VERSION: u64 = 1;
/// The largest file the import reads, in bytes.
const MAX_TEXT: usize = 16 * 1024 * 1024;
/// The most materials one file may hold.
const MAX_ROWS: usize = 10_000;

/// A materials file's format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    Json,
    Csv,
}

impl FileFormat {
    /// Its id in commands and queries: `json` or `csv`.
    pub fn id(self) -> &'static str {
        match self {
            FileFormat::Json => "json",
            FileFormat::Csv => "csv",
        }
    }

    /// A format by its id (any case).
    pub fn from_id(text: &str) -> Option<FileFormat> {
        match text.trim().to_ascii_lowercase().as_str() {
            "json" => Some(FileFormat::Json),
            "csv" => Some(FileFormat::Csv),
            _ => None,
        }
    }
}

/// What a column (a JSON field) holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Id,
    Name,
    Class,
    Density,
    Property,
    /// A print setting of the filament: its key, and whether it is a whole
    /// number.
    Print(&'static str, bool),
    Reference,
}

/// A column of a materials file: its key, what it holds and the quantity
/// whose unit it is written in (none: a text or a ratio).
#[derive(Debug, Clone, Copy)]
struct Column {
    key: &'static str,
    kind: Kind,
    quantity: Option<Quantity>,
}

const fn column(key: &'static str, kind: Kind, quantity: Option<Quantity>) -> Column {
    Column {
        key,
        kind,
        quantity,
    }
}

/// The columns in the order the files write them.
static COLUMNS: [Column; 17] = [
    column("id", Kind::Id, None),
    column("name", Kind::Name, None),
    column("class", Kind::Class, None),
    column("density", Kind::Density, Some(Quantity::Density)),
    column("youngs_modulus", Kind::Property, Some(Quantity::Stress)),
    column("poissons_ratio", Kind::Property, None),
    column("yield_strength", Kind::Property, Some(Quantity::Stress)),
    column(
        "max_service_temperature",
        Kind::Property,
        Some(Quantity::Temperature),
    ),
    column(
        "thermal_expansion",
        Kind::Property,
        Some(Quantity::Expansion),
    ),
    column(
        "extruder_temperature",
        Kind::Print("temperature", false),
        Some(Quantity::Temperature),
    ),
    column(
        "extruder_temperature_min",
        Kind::Print("nozzle_temperature_range_low", true),
        Some(Quantity::Temperature),
    ),
    column(
        "extruder_temperature_max",
        Kind::Print("nozzle_temperature_range_high", true),
        Some(Quantity::Temperature),
    ),
    column(
        "bed_temperature",
        Kind::Print("bed_temperature", false),
        Some(Quantity::Temperature),
    ),
    column("source", Kind::Reference, None),
    column("material", Kind::Reference, None),
    column("grade", Kind::Reference, None),
    column("revision", Kind::Reference, None),
];

fn find_column(key: &str) -> Option<&'static Column> {
    COLUMNS.iter().find(|c| c.key == key)
}

/// Whether a column holds a number.
fn numeric(column: &Column) -> bool {
    matches!(
        column.kind,
        Kind::Density | Kind::Property | Kind::Print(..)
    )
}

/// A quantity's name in a message: densities, stresses, temperatures,
/// expansion coefficients.
fn plural(quantity: Quantity) -> &'static str {
    match quantity {
        Quantity::Stress => "stresses",
        Quantity::Density => "densities",
        Quantity::Temperature => "temperatures",
        Quantity::Expansion => "expansion coefficients",
        Quantity::Ratio => "ratios",
    }
}

/// One material of a file: what it gives, in the model's units; a value
/// not given is none.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MaterialRow {
    /// Its place in the file: the CSV line, the JSON list's number from 1.
    pub row: usize,
    /// Its id in the file (the dataset's entry id); the import makes its
    /// own.
    pub id: Option<String>,
    pub name: String,
    pub class: Option<MaterialClass>,
    /// g/cm³.
    pub density: Option<f64>,
    pub properties: MaterialProperties,
    /// Its print settings' values by filament key.
    pub filament: BTreeMap<String, Value>,
    pub reference: Option<MaterialReference>,
}

impl MaterialRow {
    /// A material's row: its values, its class as it is listed.
    pub fn of(material: &Material) -> MaterialRow {
        let filament = COLUMNS
            .iter()
            .filter_map(|c| match c.kind {
                Kind::Print(key, _) => material.filament.get(key).map(|v| (key, v)),
                _ => None,
            })
            .filter(|(_, v)| v.is_number())
            .map(|(k, v)| (k.to_owned(), v.clone()))
            .collect();
        MaterialRow {
            row: 0,
            id: Some(material.id.clone()),
            name: material.name.clone(),
            class: Some(material::material_class(material)),
            density: Some(material.density),
            properties: material.properties,
            filament,
            reference: material.reference.clone(),
        }
    }

    /// The change that gives its values: for a new material, or over one
    /// of the same name whose other values stay ("replace").
    pub fn change(&self) -> MaterialChange {
        MaterialChange {
            name: Some(self.name.clone()),
            density: self.density,
            class: self.class,
            properties: (!self.properties.is_empty())
                .then(|| PropertiesChange::giving(&self.properties)),
            filament: (!self.filament.is_empty()).then(|| self.filament.clone()),
            reference: self.reference.clone().map(Some),
            ..MaterialChange::default()
        }
    }

    /// A material of its values (no appearance; 1 g/cm³ without a
    /// density), with `id`.
    pub fn material(&self, id: &str) -> Material {
        let mut material = Material {
            id: id.to_owned(),
            name: self.name.clone(),
            density: self.density.unwrap_or(1.0),
            appearance: None,
            appearances: Vec::new(),
            filament: self.filament.clone(),
            slicer: BTreeMap::new(),
            variants: Vec::new(),
            based_on: None,
            class: None,
            version: None,
            origin: None,
            properties: self.properties,
            reference: self.reference.clone(),
        };
        material.class = self
            .class
            .map(|class| material::stored_class(&material, class));
        material
    }

    /// The value of a column in the model's unit; none when not given.
    fn number(&self, column: &Column) -> Option<f64> {
        match column.kind {
            Kind::Density => self.density,
            Kind::Property => self.properties.get(column.key),
            Kind::Print(key, _) => self.filament.get(key).and_then(Value::as_f64),
            _ => None,
        }
    }

    /// The text of a column; empty when not given.
    fn text(&self, column: &Column) -> String {
        let reference = self.reference.as_ref();
        match (column.kind, column.key) {
            (Kind::Id, _) => self.id.clone().unwrap_or_default(),
            (Kind::Name, _) => self.name.clone(),
            (Kind::Class, _) => self.class.map(|c| c.id().to_owned()).unwrap_or_default(),
            (Kind::Reference, "source") => reference.map(|r| r.source.clone()).unwrap_or_default(),
            (Kind::Reference, "material") => reference
                .and_then(|r| r.material.clone())
                .unwrap_or_default(),
            (Kind::Reference, "grade") => {
                reference.and_then(|r| r.grade.clone()).unwrap_or_default()
            }
            (Kind::Reference, "revision") => reference
                .and_then(|r| r.revision.clone())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Sets a column's number, written in `unit` (none: the file states no
    /// unit for it), converted into the model's unit and checked.
    fn set_number(
        &mut self,
        column: &Column,
        value: f64,
        unit: Option<ShownUnit>,
    ) -> Result<(), String> {
        let key = column.key;
        let value = match column.quantity {
            Some(quantity) => {
                let unit = unit.ok_or_else(|| {
                    format!("{key}: the file states no unit of {}", plural(quantity))
                })?;
                to_model(unit, value)
            }
            None => value,
        };
        if !value.is_finite() {
            return Err(format!("{key}: not a number"));
        }
        match column.kind {
            Kind::Density => {
                if value <= 0.0 {
                    return Err(format!(
                        "density must be greater than zero, got {}",
                        significant(value, 6)
                    ));
                }
                self.density = Some(value);
            }
            Kind::Property => {
                let def = property_def(key).expect("a property's column");
                material::check_property(def, value)?;
                self.properties.set(key, Some(value));
            }
            Kind::Print(setting, whole) => {
                let json = if whole {
                    json!(value.round() as i64)
                } else {
                    json!(value)
                };
                material::check_filament_value(setting, &json)
                    .map_err(|e| format!("{key}: {e}"))?;
                self.filament.insert(setting.to_owned(), json);
            }
            _ => return Err(format!("{key}: a text, not a number")),
        }
        Ok(())
    }

    /// Sets a column's text (a number column's read as a number); an empty
    /// one is not given.
    fn set_text(
        &mut self,
        column: &Column,
        text: &str,
        unit: Option<ShownUnit>,
    ) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        let key = column.key;
        match column.kind {
            Kind::Id => self.id = Some(text.to_owned()),
            Kind::Name => text.clone_into(&mut self.name),
            Kind::Class => {
                self.class = Some(MaterialClass::from_id(text).ok_or_else(|| {
                    format!(
                        "class: {text} is no class: metal, plastic, fdm_plastic, resin, wood or other"
                    )
                })?);
            }
            Kind::Reference => {
                let reference = self.reference.get_or_insert_with(|| MaterialReference {
                    source: String::new(),
                    material: None,
                    grade: None,
                    revision: None,
                });
                let value = Some(text.to_owned());
                match key {
                    "source" => text.clone_into(&mut reference.source),
                    "material" => reference.material = value,
                    "grade" => reference.grade = value,
                    _ => reference.revision = value,
                }
            }
            _ => {
                let value: f64 = text
                    .parse()
                    .map_err(|_| format!("{key}: {text} is no number"))?;
                self.set_number(column, value, unit)?;
            }
        }
        Ok(())
    }

    /// The row when it is complete: a name, a source when it names a
    /// reference.
    fn finished(self) -> Result<MaterialRow, String> {
        if self.name.trim().is_empty() {
            return Err("the name is empty".to_owned());
        }
        if self
            .reference
            .as_ref()
            .is_some_and(|r| r.source.trim().is_empty())
        {
            return Err("the reference names no source".to_owned());
        }
        Ok(self)
    }
}

/// A value in `unit` in the model's unit; a converted one without the
/// noise of the conversion (12 significant digits: 212 °F is 100 °C).
fn to_model(unit: ShownUnit, value: f64) -> f64 {
    let converted = unit.to_model(value);
    if unit.scale == 1.0 && unit.offset == 0.0 {
        converted
    } else {
        significant(converted, 12).parse().unwrap_or(converted)
    }
}

/// A value of the model's unit as a file writes it in `unit`: the shortest
/// text that reads back without a conversion, else 12 significant digits.
fn number_text(value: f64, unit: Option<ShownUnit>) -> String {
    match unit {
        Some(unit) if unit.scale != 1.0 || unit.offset != 0.0 => {
            significant(unit.from_model(value), 12)
        }
        _ => format!("{value}"),
    }
}

/// A material a file gives that is not valid: its place in the file, its
/// name when it has one, and the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowError {
    pub row: usize,
    pub name: String,
    pub message: String,
}

/// The materials a file gives and the ones that are not valid.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReadMaterials {
    pub rows: Vec<MaterialRow>,
    pub errors: Vec<RowError>,
}

/// Writes materials as a file of `format` in `system`'s units with
/// temperatures in `temperature`.
pub fn write(
    materials: &[Material],
    format: FileFormat,
    system: UnitSystem,
    temperature: TemperatureUnit,
) -> String {
    let rows: Vec<MaterialRow> = materials.iter().map(MaterialRow::of).collect();
    let unit = |c: &Column| c.quantity.map(|q| shown_unit(q, system, temperature));
    match format {
        FileFormat::Json => {
            let units: Vec<String> = Quantity::WITH_UNITS
                .iter()
                .map(|q| {
                    let word = file_unit(shown_unit(*q, system, temperature));
                    format!("{}: {}", json!(q.id()), json!(word))
                })
                .collect();
            let entries: Vec<String> = rows
                .iter()
                .map(|row| {
                    let mut fields: Vec<String> = Vec::new();
                    for c in COLUMNS.iter().filter(|c| c.kind != Kind::Reference) {
                        if numeric(c) {
                            if let Some(value) = row.number(c) {
                                fields.push(format!(
                                    "{}: {}",
                                    json!(c.key),
                                    number_text(value, unit(c))
                                ));
                            }
                        } else {
                            let text = row.text(c);
                            if !text.is_empty() {
                                fields.push(format!("{}: {}", json!(c.key), json!(text)));
                            }
                        }
                    }
                    if let Some(reference) = &row.reference {
                        let value =
                            serde_json::to_value(reference).expect("a reference serializes");
                        fields.push(format!("\"reference\": {value}"));
                    }
                    format!("    {{{}}}", fields.join(", "))
                })
                .collect();
            format!(
                "{{\n  \"format\": {},\n  \"version\": {VERSION},\n  \"units\": {{{}}},\n  \"materials\": [\n{}\n  ]\n}}\n",
                json!(FORMAT),
                units.join(", "),
                entries.join(",\n")
            )
        }
        FileFormat::Csv => {
            let mut out = String::new();
            let header: Vec<String> = COLUMNS
                .iter()
                .map(|c| match unit(c) {
                    Some(u) => format!("{} [{}]", c.key, file_unit(u)),
                    None => c.key.to_owned(),
                })
                .collect();
            let header: Vec<&str> = header.iter().map(String::as_str).collect();
            write_record(&mut out, &header);
            for row in &rows {
                let cells: Vec<String> = COLUMNS
                    .iter()
                    .map(|c| {
                        if numeric(c) {
                            row.number(c)
                                .map(|v| number_text(v, unit(c)))
                                .unwrap_or_default()
                        } else {
                            row.text(c)
                        }
                    })
                    .collect();
                let cells: Vec<&str> = cells.iter().map(String::as_str).collect();
                write_record(&mut out, &cells);
            }
            out
        }
    }
}

/// Reads a materials file: its materials and those that are not valid;
/// an error for a file it cannot read at all.
pub fn read(text: &str, format: FileFormat) -> Result<ReadMaterials, String> {
    if text.len() > MAX_TEXT {
        return Err(format!(
            "the file is larger than {} MB",
            MAX_TEXT / (1024 * 1024)
        ));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let read = match format {
        FileFormat::Json => read_json(text)?,
        FileFormat::Csv => read_csv(text)?,
    };
    if read.rows.len() + read.errors.len() > MAX_ROWS {
        return Err(format!("the file holds more than {MAX_ROWS} materials"));
    }
    Ok(read)
}

fn read_json(text: &str) -> Result<ReadMaterials, String> {
    let file: Value =
        serde_json::from_str(text).map_err(|e| format!("not a materials file: {e}"))?;
    let Value::Object(file) = file else {
        return Err("not a materials file: the file is no JSON object".to_owned());
    };
    match file.get("format").and_then(Value::as_str) {
        Some(FORMAT) => {}
        Some(other) => {
            return Err(format!(
                "not a materials file: the format is {other}, not {FORMAT}"
            ));
        }
        None => return Err(format!("not a materials file: no \"format\": \"{FORMAT}\"")),
    }
    match file.get("version").and_then(Value::as_u64) {
        Some(version) if version <= VERSION => {}
        Some(version) => {
            return Err(format!(
                "the file is of version {version}, newer than this version of Mitcad reads ({VERSION})"
            ));
        }
        None => return Err("the file gives no version".to_owned()),
    }
    let mut units: BTreeMap<&'static str, ShownUnit> = BTreeMap::new();
    match file.get("units") {
        None | Some(Value::Null) => {}
        Some(Value::Object(given)) => {
            for (key, word) in given {
                let quantity = Quantity::WITH_UNITS
                    .into_iter()
                    .find(|q| q.id() == key)
                    .ok_or_else(|| {
                        format!(
                            "units: {key} is no quantity: density, stress, temperature or expansion"
                        )
                    })?;
                let word = word
                    .as_str()
                    .ok_or_else(|| format!("units: {key} is no text"))?;
                let unit = parse_unit(quantity, word)
                    .ok_or_else(|| format!("units: {word} is no unit of {}", plural(quantity)))?;
                units.insert(quantity.id(), unit);
            }
        }
        Some(_) => return Err("units: not an object".to_owned()),
    }
    let Some(Value::Array(entries)) = file.get("materials") else {
        return Err("not a materials file: no list of \"materials\"".to_owned());
    };
    let mut read = ReadMaterials::default();
    for (i, entry) in entries.iter().enumerate() {
        let row = i + 1;
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        match json_row(row, entry, &units) {
            Ok(material) => read.rows.push(material),
            Err(message) => read.errors.push(RowError { row, name, message }),
        }
    }
    Ok(read)
}

/// One material of a JSON file.
fn json_row(
    row: usize,
    entry: &Value,
    units: &BTreeMap<&'static str, ShownUnit>,
) -> Result<MaterialRow, String> {
    let Value::Object(fields) = entry else {
        return Err("not an object".to_owned());
    };
    let unit = |c: &Column| c.quantity.and_then(|q| units.get(q.id()).copied());
    let mut out = MaterialRow {
        row,
        ..MaterialRow::default()
    };
    for (key, value) in fields {
        if key == "reference" {
            match value {
                Value::Null => {}
                Value::Object(reference) => {
                    for (field, text) in reference {
                        let column = find_column(field)
                            .filter(|c| c.kind == Kind::Reference)
                            .ok_or_else(|| format!("reference: {field} is no field of it"))?;
                        match text {
                            Value::String(text) => out.set_text(column, text, None)?,
                            Value::Null => {}
                            _ => return Err(format!("reference: {field} is no text")),
                        }
                    }
                }
                _ => return Err("reference: not an object".to_owned()),
            }
            continue;
        }
        let column = find_column(key)
            .filter(|c| c.kind != Kind::Reference)
            .ok_or_else(|| format!("{key} is no field of a material"))?;
        match value {
            Value::Null => {}
            Value::Number(number) if numeric(column) => {
                out.set_number(column, number.as_f64().unwrap_or(f64::NAN), unit(column))?
            }
            Value::String(text) if !numeric(column) => out.set_text(column, text, None)?,
            _ if numeric(column) => return Err(format!("{key}: a number")),
            _ => return Err(format!("{key}: a text")),
        }
    }
    out.finished()
}

fn read_csv(text: &str) -> Result<ReadMaterials, String> {
    let mut records = records(text, separator(text))
        .map_err(|e| e.to_string())?
        .into_iter();
    let Some((_, header)) = records.next() else {
        return Err("the CSV file is empty".to_owned());
    };
    // The header's columns with their units: `density [g/cm3]`.
    let mut columns: Vec<(&'static Column, Option<ShownUnit>)> = Vec::new();
    for cell in &header {
        let cell = cell.trim();
        let (key, unit) = match cell.split_once('[') {
            Some((key, rest)) => (
                key.trim(),
                Some(rest.trim_end().trim_end_matches(']').trim()),
            ),
            None => (cell, None),
        };
        let column = find_column(key).ok_or_else(|| {
            format!(
                "the header's column {cell} is none of a materials file: {}",
                COLUMNS.iter().map(|c| c.key).collect::<Vec<_>>().join(", ")
            )
        })?;
        if columns.iter().any(|(c, _)| c.key == column.key) {
            return Err(format!("the header has the column {key} twice"));
        }
        let unit = match (column.quantity, unit) {
            (Some(quantity), Some(word)) => Some(parse_unit(quantity, word).ok_or_else(|| {
                format!(
                    "the column {key}: {word} is no unit of {}",
                    plural(quantity)
                )
            })?),
            (Some(quantity), None) => {
                let example = file_unit(shown_unit(
                    quantity,
                    UnitSystem::Metric,
                    TemperatureUnit::Celsius,
                ));
                return Err(format!(
                    "the column {key} states no unit: write it as {key} [{example}]"
                ));
            }
            (None, Some(_)) => return Err(format!("the column {key} takes no unit")),
            (None, None) => None,
        };
        columns.push((column, unit));
    }
    if !columns.iter().any(|(c, _)| c.kind == Kind::Name) {
        return Err("not a materials file: the header has no name column".to_owned());
    }
    let mut read = ReadMaterials::default();
    for (line, cells) in records {
        let mut out = MaterialRow {
            row: line,
            ..MaterialRow::default()
        };
        let name = columns
            .iter()
            .position(|(c, _)| c.kind == Kind::Name)
            .and_then(|i| cells.get(i))
            .map(|n| n.trim().to_owned())
            .unwrap_or_default();
        if cells.len() > columns.len()
            && cells[columns.len()..].iter().any(|c| !c.trim().is_empty())
        {
            read.errors.push(RowError {
                row: line,
                name,
                message: format!(
                    "{} cells, the header has {} columns",
                    cells.len(),
                    columns.len()
                ),
            });
            continue;
        }
        let mut result = Ok(());
        for ((column, unit), cell) in columns.iter().zip(&cells) {
            result = out.set_text(column, cell, *unit);
            if result.is_err() {
                break;
            }
        }
        match result.and_then(|()| out.finished()) {
            Ok(material) => read.rows.push(material),
            Err(message) => read.errors.push(RowError {
                row: line,
                name,
                message,
            }),
        }
    }
    Ok(read)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steel() -> Material {
        let mut m = material::built_in_material("steel").unwrap().clone();
        m.id = "shop_steel".to_owned();
        m.name = "Shop steel".to_owned();
        m.properties = MaterialProperties {
            youngs_modulus: Some(210_000.0),
            poissons_ratio: Some(0.3),
            yield_strength: Some(235.0),
            max_service_temperature: Some(400.0),
            thermal_expansion: Some(12.0),
        };
        m.reference = Some(MaterialReference {
            source: "Shop sheet, \"S235\"".to_owned(),
            material: Some("S235JR".to_owned()),
            grade: Some("JR".to_owned()),
            revision: None,
        });
        m
    }

    fn pla() -> Material {
        let mut m = material::built_in_material("pla").unwrap().clone();
        m.id = "shop_pla".to_owned();
        m.name = "Shop PLA".to_owned();
        m.filament.insert("temperature".to_owned(), json!(210));
        m.filament.insert("bed_temperature".to_owned(), json!(60));
        m.filament
            .insert("nozzle_temperature_range_low".to_owned(), json!(190));
        m.properties.max_service_temperature = Some(55.0);
        m
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
    }

    fn same_values(a: &MaterialRow, b: &MaterialRow) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.class, b.class);
        assert_eq!(a.reference, b.reference);
        assert!(close(a.density.unwrap(), b.density.unwrap()));
        for def in &material::PROPERTY_DEFS {
            match (a.properties.get(def.key), b.properties.get(def.key)) {
                (Some(x), Some(y)) => assert!(close(x, y), "{}: {x} {y}", def.key),
                (x, y) => assert_eq!(x, y, "{}", def.key),
            }
        }
        assert_eq!(a.filament.len(), b.filament.len());
        for (key, value) in &a.filament {
            assert!(
                close(value.as_f64().unwrap(), b.filament[key].as_f64().unwrap()),
                "{key}"
            );
        }
    }

    #[test]
    fn json_and_csv_round_trip_in_metric_and_imperial_units() {
        let materials = [steel(), pla()];
        let expected: Vec<MaterialRow> = materials.iter().map(MaterialRow::of).collect();
        for format in [FileFormat::Json, FileFormat::Csv] {
            for (system, temperature) in [
                (UnitSystem::Metric, TemperatureUnit::Celsius),
                (UnitSystem::Imperial, TemperatureUnit::Fahrenheit),
            ] {
                let text = write(&materials, format, system, temperature);
                let read = read(&text, format).unwrap_or_else(|e| panic!("{e}\n{text}"));
                assert!(read.errors.is_empty(), "{:?}\n{text}", read.errors);
                assert_eq!(read.rows.len(), 2);
                for (got, want) in read.rows.iter().zip(&expected) {
                    same_values(got, want);
                }
                // The file states its units.
                match (format, system) {
                    (FileFormat::Json, UnitSystem::Imperial) => {
                        assert!(text.contains("\"stress\": \"psi\""), "{text}");
                        assert!(text.contains("\"temperature\": \"F\""), "{text}");
                    }
                    (FileFormat::Csv, UnitSystem::Metric) => {
                        assert!(
                            text.starts_with("id,name,class,density [g/cm3],youngs_modulus [MPa]"),
                            "{text}"
                        );
                    }
                    _ => {}
                }
            }
        }
        // Metric values read back exactly.
        let text = write(
            &materials,
            FileFormat::Json,
            UnitSystem::Metric,
            TemperatureUnit::Celsius,
        );
        let read = read(&text, FileFormat::Json).unwrap();
        assert_eq!(read.rows[0].properties, steel().properties);
        assert_eq!(read.rows[1].filament["temperature"], json!(210.0));
    }

    #[test]
    fn imperial_values_are_converted_into_the_models_units() {
        let text = r#"{"format": "mitcad-materials", "version": 1,
            "units": {"density": "lb/in3", "stress": "psi", "temperature": "F", "expansion": "uin/(in*F)"},
            "materials": [{"name": "Al", "density": 0.1, "youngs_modulus": 10000000,
                           "max_service_temperature": 212, "thermal_expansion": 13,
                           "extruder_temperature": 455, "extruder_temperature_min": 428}]}"#;
        let read = read(text, FileFormat::Json).unwrap();
        assert!(read.errors.is_empty(), "{:?}", read.errors);
        let al = &read.rows[0];
        assert!(close(al.density.unwrap(), 2.767_990_471_020_31));
        assert!(close(
            al.properties.youngs_modulus.unwrap(),
            68_947.572_931_683_6
        ));
        assert_eq!(al.properties.max_service_temperature, Some(100.0));
        assert!(close(al.properties.thermal_expansion.unwrap(), 23.4));
        assert_eq!(al.filament["temperature"], json!(235.0));
        assert_eq!(al.filament["nozzle_temperature_range_low"], json!(220));
    }

    #[test]
    fn invalid_rows_are_reported_and_the_others_read() {
        let text = "name,density [g/cm3],youngs_modulus [GPa],poissons_ratio,bed_temperature [C]\r\n\
                    Good,7.85,210,0.3,\r\n\
                    Bad modulus,7.85,-1,,\r\n\
                    ,2.7,70,,\r\n\
                    Soft,1.1,abc,,\r\n\
                    Ratio,1.1,,0.7,\r\n";
        let read = read(text, FileFormat::Csv).unwrap();
        assert_eq!(read.rows.len(), 1);
        assert_eq!(read.rows[0].name, "Good");
        // GPa read as MPa.
        assert_eq!(read.rows[0].properties.youngs_modulus, Some(210_000.0));
        let errors: Vec<(usize, &str, &str)> = read
            .errors
            .iter()
            .map(|e| (e.row, e.name.as_str(), e.message.as_str()))
            .collect();
        assert_eq!(errors.len(), 4, "{errors:?}");
        assert_eq!(errors[0].0, 3);
        assert!(
            errors[0]
                .2
                .contains("Young's modulus must be greater than zero"),
            "{errors:?}"
        );
        assert_eq!((errors[1].0, errors[1].2), (4, "the name is empty"));
        assert!(errors[2].2.contains("abc is no number"), "{errors:?}");
        assert!(errors[3].2.contains("Poisson's ratio"), "{errors:?}");
    }

    #[test]
    fn files_that_cannot_be_read_are_refused_whole() {
        let refused = |text: &str, format| read(text, format).unwrap_err();
        assert!(refused("{}", FileFormat::Json).contains("no \"format\""));
        assert!(refused("[1]", FileFormat::Json).contains("no JSON object"));
        assert!(
            refused(
                r#"{"format": "mitcad-materials", "version": 9, "materials": []}"#,
                FileFormat::Json
            )
            .contains("newer")
        );
        assert!(
            refused(
                r#"{"format": "mitcad-materials", "version": 1, "units": {"stress": "bar"}, "materials": []}"#,
                FileFormat::Json
            )
            .contains("bar is no unit of stresses")
        );
        // A quantity's column must state its unit.
        assert!(refused("name,density\r\nX,1\r\n", FileFormat::Csv).contains("states no unit"));
        assert!(refused("name,hardness\r\nX,1\r\n", FileFormat::Csv).contains("hardness"));
        assert!(refused("density [g/cm3]\r\n1\r\n", FileFormat::Csv).contains("no name column"));
        assert!(refused("", FileFormat::Csv).contains("empty"));
        // A JSON row without its unit, or with an unknown field, is a row's
        // error.
        let read = read(
            r#"{"format": "mitcad-materials", "version": 1,
                "materials": [{"name": "X", "density": 1.2}, {"name": "Y", "colour": "red"}]}"#,
            FileFormat::Json,
        )
        .unwrap();
        assert!(read.rows.is_empty());
        assert!(
            read.errors[0].message.contains("no unit of densities"),
            "{:?}",
            read.errors
        );
        assert_eq!(read.errors[1].name, "Y");
        assert!(
            read.errors[1].message.contains("colour"),
            "{:?}",
            read.errors
        );
    }

    /// The example files of docs/material-files.md read whole.
    #[test]
    fn the_documented_examples_read_whole() {
        for (text, format) in [
            (
                include_str!("../../../docs/examples/materials.json"),
                FileFormat::Json,
            ),
            (
                include_str!("../../../docs/examples/materials.csv"),
                FileFormat::Csv,
            ),
        ] {
            let read = read(text, format).unwrap();
            assert!(read.errors.is_empty(), "{:?}", read.errors);
            assert_eq!(read.rows.len(), 2);
            assert_eq!(read.rows[0].properties.yield_strength, Some(235.0));
            assert_eq!(
                read.rows[0].reference.as_ref().unwrap().source,
                "Mill sheet"
            );
            assert_eq!(
                read.rows[1].filament["nozzle_temperature_range_high"],
                json!(230)
            );
            assert_eq!(read.rows[1].class, Some(MaterialClass::FdmPlastic));
        }
    }

    #[test]
    fn a_row_gives_a_change_of_its_values_only() {
        let text = "name,yield_strength [MPa],source,grade\r\nShop steel,300,Mill sheet,S355\r\n";
        let read = read(text, FileFormat::Csv).unwrap();
        let change = read.rows[0].change();
        let mut m = steel();
        change.apply_to(&mut m);
        // The yield strength and the reference replaced, the others kept.
        assert_eq!(m.properties.yield_strength, Some(300.0));
        assert_eq!(m.properties.youngs_modulus, Some(210_000.0));
        assert_eq!(m.density, steel().density);
        let reference = m.reference.unwrap();
        assert_eq!(
            (reference.source.as_str(), reference.grade.as_deref()),
            ("Mill sheet", Some("S355"))
        );
    }
}
