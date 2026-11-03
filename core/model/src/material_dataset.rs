// SPDX-License-Identifier: MIT
//! The bundled material dataset (mitcad#1213): typical mechanical,
//! thermal and printing values of common metals and printing materials,
//! offline, from which the user picks a material and grade to start a
//! material of their own (`create_material`'s `dataset`). The values are
//! typical: the application says so, to be checked against the supplier's
//! data.
//!
//! The data is third-party (Apache-2.0): `third_party/material-properties`
//! keeps it with its licence, its notice and how it was converted. It is
//! kept in the materials exchange format in its own units
//! (`crate::material_exchange`), which convert into the model's when it is
//! read. A material made from it is the user's own copy, which remembers
//! the entry (its `reference`: the dataset, material, grade and the data's
//! revision): a newer dataset never changes it.

use std::sync::LazyLock;

use crate::material::Material;
use crate::material_exchange::{self, FileFormat, MaterialRow};

/// The dataset as the exchange format writes it, in its own units.
const DATA: &str = include_str!("../../../third_party/material-properties/materials.json");
/// Its licence, which the application shows with the other licences.
pub const LICENSE: &str = include_str!("../../../third_party/material-properties/LICENSE");
/// Its notice: the attribution and the changes made to it.
pub const NOTICE: &str = include_str!("../../../third_party/material-properties/NOTICE");
/// Its name, as its materials' references give it.
pub const NAME: &str = "material-properties dataset";
/// Its licence's SPDX identifier.
pub const LICENSE_ID: &str = "Apache-2.0";
/// What the application says of its values, in one line.
pub const CAUTION: &str = "Typical values: check them against the supplier's data.";

/// The dataset's materials, each with its entry id (`aluminum/2014/t6`),
/// in its order, in the model's units.
pub fn entries() -> &'static [MaterialRow] {
    static ENTRIES: LazyLock<Vec<MaterialRow>> = LazyLock::new(|| {
        material_exchange::read(DATA, FileFormat::Json)
            .map(|read| read.rows)
            .unwrap_or_default()
    });
    &ENTRIES
}

/// An entry by its id.
pub fn entry(id: &str) -> Option<&'static MaterialRow> {
    entries().iter().find(|r| r.id.as_deref() == Some(id))
}

/// The entries a search finds: those whose name, material, grade or id
/// holds every word of `text` (any case); all for an empty text.
pub fn search(text: &str) -> Vec<&'static MaterialRow> {
    let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
    entries()
        .iter()
        .filter(|row| {
            let reference = row.reference.as_ref();
            let haystack = [
                Some(row.name.as_str()),
                row.id.as_deref(),
                reference.and_then(|r| r.material.as_deref()),
                reference.and_then(|r| r.grade.as_deref()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
            .replace('_', " ");
            words.iter().all(|w| haystack.contains(w.as_str()))
        })
        .collect()
}

/// A material of an entry's values (no id yet), with its reference and the
/// library's appearance of its kind.
pub fn material(id: &str) -> Option<Material> {
    entry(id).map(|row| {
        let mut material = row.material("");
        material.appearance = Some(appearance_of(id).to_owned());
        material
    })
}

/// The library's appearance of an entry's kind, by its file: anodized
/// aluminium, polished stainless steel, cast iron, copper, white plastic for
/// the printing materials, else satin steel.
fn appearance_of(id: &str) -> &'static str {
    match id.split('/').next().unwrap_or("") {
        "aluminum" => "aluminum_anodized",
        "stainlesssteel" => "steel_polished",
        "castiron" => "cast_iron",
        "copper" => "copper",
        "3dprinting" => "plastic_white",
        _ => "steel_satin",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::material::MaterialClass;

    #[test]
    fn the_dataset_reads_whole_in_the_models_units() {
        let read = material_exchange::read(DATA, FileFormat::Json).unwrap();
        assert!(read.errors.is_empty(), "{:?}", read.errors);
        assert_eq!(entries().len(), 119);
        for row in entries() {
            let id = row.id.as_deref().unwrap();
            assert!(crate::material::valid_id(id), "{id}");
            assert!(row.density.is_some(), "{id}");
            assert_eq!(row.reference.as_ref().unwrap().source, NAME, "{id}");
            row.material("x")
                .check()
                .unwrap_or_else(|e| panic!("{id}: {e}"));
            let look = material(id).unwrap().appearance.unwrap();
            assert!(
                crate::appearance::library_appearance(&look).is_some(),
                "{id}: {look}"
            );
        }
        // 0.101 lb/in³ and 10 500 000 psi, as g/cm³ and MPa.
        let al = entry("aluminum/2014/t6").unwrap();
        assert_eq!(al.name, "Aluminum 2014, T6");
        assert_eq!(al.class, Some(MaterialClass::Metal));
        assert!((al.density.unwrap() - 2.795_670_375_730_52).abs() < 1e-9);
        assert!((al.properties.youngs_modulus.unwrap() - 72_394.951_578_267_8).abs() < 1e-6);
        assert_eq!(al.properties.poissons_ratio, Some(0.33));
        // A printing material's temperatures are its print settings: 356
        // to 446 °F is 180 to 230 °C, the midpoint 205 °C.
        let pla = entry("3dprinting/pla/default").unwrap();
        assert_eq!(pla.filament["temperature"], json!(205.0));
        assert_eq!(pla.filament["nozzle_temperature_range_low"], json!(180));
        assert_eq!(pla.filament["nozzle_temperature_range_high"], json!(230));
        assert_eq!(pla.properties.max_service_temperature, Some(52.2222222222));
        let reference = pla.reference.as_ref().unwrap();
        assert_eq!(
            (reference.material.as_deref(), reference.grade.as_deref()),
            (Some("PLA"), Some("Default"))
        );
    }

    #[test]
    fn a_search_finds_by_name_material_and_grade() {
        let names =
            |text: &str| -> Vec<&str> { search(text).iter().map(|r| r.name.as_str()).collect() };
        assert_eq!(names("6061 t6"), ["Aluminum 6061, T6"]);
        assert_eq!(names("ti-6al-4v grade 5"), ["Titanium Ti-6Al-4V, Grade 5"]);
        assert!(names("petg").contains(&"PETG"));
        assert!(names("unobtainium").is_empty());
        assert_eq!(search("").len(), entries().len());
    }
}
