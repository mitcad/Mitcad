// SPDX-License-Identifier: MIT
//! Material classes (mitcad#655, mitcad#1102): what a material is made of,
//! by which the lists group materials, and what an appearance is a look of,
//! by which the pickers order appearances (a material's class first).
//!
//! General Plastic keeps the id `plastic` and the group `plastics` it had
//! when it was the only plastic: library parts and scripts that name it
//! read as before. FDM Plastic holds the filaments: a material with print
//! settings is one, whatever class it stores (the model works it out,
//! `mitcad_model::material::material_class`). Resin is the photopolymer
//! resins of resin printers. Wood is solid wood and wood-based panels
//! (mitcad#1552: plywood, MDF, OSB, chipboard), with a wood look as the
//! class's default appearance.

use serde::{Deserialize, Serialize};

/// A material's or an appearance's class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialClass {
    Metal,
    /// General Plastic: moulded, cast, machined or sheet plastic.
    Plastic,
    /// FDM Plastic: the filaments of FDM printers.
    FdmPlastic,
    Resin,
    /// Solid wood and wood-based panels (mitcad#1552).
    Wood,
    Other,
}

impl MaterialClass {
    /// Every class, in the lists' order.
    pub const ALL: [MaterialClass; 6] = [
        MaterialClass::Metal,
        MaterialClass::Plastic,
        MaterialClass::FdmPlastic,
        MaterialClass::Resin,
        MaterialClass::Wood,
        MaterialClass::Other,
    ];

    /// Its id in commands and files.
    pub fn id(self) -> &'static str {
        match self {
            MaterialClass::Metal => "metal",
            MaterialClass::Plastic => "plastic",
            MaterialClass::FdmPlastic => "fdm_plastic",
            MaterialClass::Resin => "resin",
            MaterialClass::Wood => "wood",
            MaterialClass::Other => "other",
        }
    }

    /// The group of the lists (`library_items`' `group`).
    pub fn group(self) -> &'static str {
        match self {
            MaterialClass::Metal => "metals",
            MaterialClass::Plastic => "plastics",
            MaterialClass::FdmPlastic => "fdm_plastics",
            MaterialClass::Resin => "resins",
            MaterialClass::Wood => "wood",
            MaterialClass::Other => "other",
        }
    }

    /// Its name for people.
    pub fn title(self) -> &'static str {
        match self {
            MaterialClass::Metal => "Metal",
            MaterialClass::Plastic => "General Plastic",
            MaterialClass::FdmPlastic => "FDM Plastic",
            MaterialClass::Resin => "Resin",
            MaterialClass::Wood => "Wood",
            MaterialClass::Other => "Other",
        }
    }

    /// The library appearance a new material of the class starts with
    /// when none is given (mitcad#1552): oak for Wood; none for the
    /// others, whose materials have the default look until one is chosen.
    pub fn default_appearance(self) -> Option<&'static str> {
        match self {
            MaterialClass::Wood => Some("wood_oak"),
            _ => None,
        }
    }

    /// The class of an id (`metal`, `plastic`, `fdm_plastic`, `resin`,
    /// `wood`, `other`).
    pub fn from_id(id: &str) -> Option<MaterialClass> {
        MaterialClass::ALL.into_iter().find(|c| c.id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_groups_and_serde_agree() {
        for class in MaterialClass::ALL {
            assert_eq!(MaterialClass::from_id(class.id()), Some(class));
            let text = serde_json::to_value(class).unwrap();
            assert_eq!(text, serde_json::json!(class.id()));
            let back: MaterialClass = serde_json::from_value(text).unwrap();
            assert_eq!(back, class);
        }
        assert_eq!(MaterialClass::Plastic.group(), "plastics");
        assert_eq!(MaterialClass::FdmPlastic.group(), "fdm_plastics");
        assert_eq!(MaterialClass::Resin.group(), "resins");
        assert_eq!(MaterialClass::Plastic.title(), "General Plastic");
        // Wood (mitcad#1552): its own id and group, before Other.
        assert_eq!(MaterialClass::from_id("wood"), Some(MaterialClass::Wood));
        assert_eq!(MaterialClass::Wood.group(), "wood");
        assert_eq!(MaterialClass::Wood.title(), "Wood");
        assert_eq!(MaterialClass::ALL[4], MaterialClass::Wood);
        assert_eq!(MaterialClass::Wood.default_appearance(), Some("wood_oak"));
        assert_eq!(MaterialClass::Metal.default_appearance(), None);
        assert!(MaterialClass::from_id("stone").is_none());
    }
}
