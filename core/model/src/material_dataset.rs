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
//!
//! Beside it, MitCad's own woods and wood-based panels (mitcad#1552,
//! `core/model/data/wood_materials.json`, MIT): their densities and the
//! woods' Young's moduli, each entry's `reference` naming the public
//! source its values were checked against (`docs/material-files.md`), in
//! the class Wood. Both are searched and listed as one dataset, the
//! third-party materials first.

use std::sync::LazyLock;

use crate::material::Material;
use crate::material_exchange::{self, FileFormat, MaterialRow};

/// The dataset as the exchange format writes it, in its own units.
const DATA: &str = include_str!("../../../third_party/material-properties/materials.json");
/// MitCad's own woods and panels (mitcad#1552), in the same format.
const WOOD_DATA: &str = include_str!("../data/wood_materials.json");
/// Their name, as the dataset's sources list them.
pub const WOOD_NAME: &str = "MitCad wood and panel materials";
/// Their licence's SPDX identifier: MitCad's own data.
pub const WOOD_LICENSE_ID: &str = "MIT";
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

/// The dataset's sources, `(name, SPDX licence)`, in its order.
pub fn sources() -> [(&'static str, &'static str); 2] {
    [(NAME, LICENSE_ID), (WOOD_NAME, WOOD_LICENSE_ID)]
}

/// The dataset's materials, each with its entry id (`aluminum/2014/t6`,
/// `wood/oak`), in its order (the third-party ones, then the woods and
/// panels), in the model's units.
pub fn entries() -> &'static [MaterialRow] {
    static ENTRIES: LazyLock<Vec<MaterialRow>> = LazyLock::new(|| {
        [DATA, WOOD_DATA]
            .into_iter()
            .flat_map(|text| {
                material_exchange::read(text, FileFormat::Json)
                    .map(|read| read.rows)
                    .unwrap_or_default()
            })
            .collect()
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

/// The library's looks of the woods and panels (mitcad#1552), one each.
const WOOD_LOOKS: &[(&str, &str)] = &[
    ("panel/chipboard", "panel_chipboard"),
    ("panel/mdf_raw", "panel_mdf"),
    ("panel/osb3", "panel_osb"),
    ("panel/plywood_birch", "plywood_birch"),
    ("panel/plywood_eucalyptus", "plywood_eucalyptus"),
    ("panel/plywood_pine", "plywood_pine"),
    ("panel/plywood_spruce", "plywood_spruce"),
    ("wood/birch", "wood_birch"),
    ("wood/cedar", "wood_cedar"),
    ("wood/cherry", "wood_cherry"),
    ("wood/douglas_fir", "wood_douglas_fir"),
    ("wood/eucalyptus", "wood_eucalyptus"),
    ("wood/maple", "wood_maple"),
    ("wood/oak", "wood_oak"),
    ("wood/pine", "wood_pine"),
    ("wood/teak", "wood_teak"),
    ("wood/walnut", "wood_walnut"),
];

/// The library's appearance of an entry: a wood's or a panel's own look,
/// else by its file: anodized aluminium, polished stainless steel, cast
/// iron, copper, white plastic for the printing materials, else satin
/// steel.
fn appearance_of(id: &str) -> &'static str {
    if let Some((_, look)) = WOOD_LOOKS.iter().find(|(entry, _)| *entry == id) {
        return look;
    }
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
        assert_eq!(read.rows.len(), 119);
        let wood = material_exchange::read(WOOD_DATA, FileFormat::Json).unwrap();
        assert!(wood.errors.is_empty(), "{:?}", wood.errors);
        assert_eq!(wood.rows.len(), 17);
        assert_eq!(entries().len(), 119 + 17);
        for (i, row) in entries().iter().enumerate() {
            let id = row.id.as_deref().unwrap();
            assert!(crate::material::valid_id(id), "{id}");
            assert!(row.density.is_some(), "{id}");
            // The third-party ones name their dataset, MitCad's own woods
            // the source of their values.
            let source = &row.reference.as_ref().unwrap().source;
            assert_eq!(source == NAME, i < 119, "{id}: {source}");
            assert_eq!(
                entries().iter().filter(|r| r.id == row.id).count(),
                1,
                "{id}"
            );
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
        // The woods by their names, ids and species (mitcad#1552).
        assert_eq!(names("oak"), ["Wood - Oak"]);
        assert_eq!(names("quercus"), ["Wood - Oak"]);
        assert_eq!(names("plywood birch"), ["Plywood - birch"]);
    }

    /// mitcad#1552: the woods and panels, each in the class Wood with its
    /// own look, its density, a wood's Young's modulus and nothing else
    /// (no value the source does not give), and its source.
    #[test]
    fn the_woods_and_panels_are_wood_with_their_values_and_looks() {
        // (id, name, kg/m³, GPa, source)
        let expected: [(&str, &str, f64, Option<f64>, &str); 17] = [
            ("panel/chipboard", "Chipboard", 611.0, None, "OEKOBAUDAT"),
            ("panel/mdf_raw", "MDF - raw", 736.0, None, "OEKOBAUDAT"),
            ("panel/osb3", "OSB/3", 610.0, None, "EGGER"),
            (
                "panel/plywood_birch",
                "Plywood - birch",
                680.0,
                None,
                "WISA",
            ),
            (
                "panel/plywood_eucalyptus",
                "Plywood - eucalyptus",
                600.0,
                None,
                "S&J Woods",
            ),
            (
                "panel/plywood_pine",
                "Plywood - pine",
                500.0,
                None,
                "Plytech",
            ),
            (
                "panel/plywood_spruce",
                "Plywood - spruce",
                460.0,
                None,
                "WISA",
            ),
            (
                "wood/birch",
                "Wood - Birch",
                690.0,
                Some(13.86),
                "The Wood Database",
            ),
            (
                "wood/cedar",
                "Wood - Cedar",
                370.0,
                Some(7.66),
                "The Wood Database",
            ),
            (
                "wood/cherry",
                "Wood - Cherry",
                560.0,
                Some(10.3),
                "The Wood Database",
            ),
            (
                "wood/douglas_fir",
                "Wood - Douglas Fir",
                510.0,
                Some(12.17),
                "The Wood Database",
            ),
            (
                "wood/eucalyptus",
                "Wood - Eucalyptus",
                820.0,
                Some(18.76),
                "The Wood Database",
            ),
            (
                "wood/maple",
                "Wood - Maple",
                705.0,
                Some(12.62),
                "The Wood Database",
            ),
            (
                "wood/oak",
                "Wood - Oak",
                755.0,
                Some(12.15),
                "The Wood Database",
            ),
            (
                "wood/pine",
                "Wood - Pine",
                550.0,
                Some(10.08),
                "The Wood Database",
            ),
            (
                "wood/teak",
                "Wood - Teak",
                655.0,
                Some(12.28),
                "The Wood Database",
            ),
            (
                "wood/walnut",
                "Wood - Walnut",
                610.0,
                Some(11.59),
                "The Wood Database",
            ),
        ];
        let woods: Vec<&MaterialRow> = entries()
            .iter()
            .filter(|r| r.class == Some(MaterialClass::Wood))
            .collect();
        assert_eq!(woods.len(), expected.len());
        let mut looks = Vec::new();
        for (id, name, density, modulus, source) in expected {
            let row = entry(id).unwrap_or_else(|| panic!("{id} is missing"));
            assert_eq!(row.name, name, "{id}");
            assert_eq!(row.class, Some(MaterialClass::Wood), "{id}");
            // kg/m³ as g/cm³, GPa as MPa.
            assert!(
                (row.density.unwrap() - density / 1000.0).abs() < 1e-9,
                "{id}"
            );
            match modulus {
                Some(gpa) => assert!(
                    (row.properties.youngs_modulus.unwrap() - gpa * 1000.0).abs() < 1e-6,
                    "{id}"
                ),
                None => assert_eq!(row.properties.youngs_modulus, None, "{id}"),
            }
            assert_eq!(row.properties.poissons_ratio, None, "{id}");
            assert_eq!(row.properties.yield_strength, None, "{id}");
            assert_eq!(row.properties.max_service_temperature, None, "{id}");
            assert_eq!(row.properties.thermal_expansion, None, "{id}");
            assert!(row.filament.is_empty(), "{id}");
            assert_eq!(row.reference.as_ref().unwrap().source, source, "{id}");
            // Its own look, of the class Wood.
            let made = material(id).unwrap();
            assert_eq!(made.class, Some(MaterialClass::Wood), "{id}");
            let look = made.appearance.unwrap();
            assert_eq!(
                crate::appearance::library_class(&look),
                Some(MaterialClass::Wood),
                "{id}: {look}"
            );
            assert!(!looks.contains(&look), "{id}: {look} twice");
            looks.push(look);
        }
        assert_eq!(
            material("wood/oak").unwrap().appearance.as_deref(),
            Some("wood_oak")
        );
        assert_eq!(sources()[1], (WOOD_NAME, "MIT"));
    }
}
