// SPDX-License-Identifier: MIT
//! A library part's material (mitcad#1817): a part placed from a library
//! is made of the material its library gives, as it shows the library's
//! appearance. The material is the one the source design's shown bodies
//! all name (a sheet's `plywood_birch`; a material of the source design's
//! own is copied in), else the one the `material` block of the source's
//! `build` block names for the part's configuration row: a timber
//! member's species, a sheet's or a metal part's product, with the
//! block's density (kg/m³, the timber group files' values). That block
//! becomes a material of the design (`Spruce`, 0.45 g/cm³), found again
//! by its name and density for every part that names it; a block that
//! gives only a density is a built-in material of that density, if one
//! has it.
//!
//! BUILD gives it when it places, connects or swaps a part
//! (`mitcad-model-build`, `parts::inherit_appearance`), and a design saved
//! before gets it on open (`DocState::settle_library_materials`): a body
//! of a derived part without a material of its own (and without a
//! material binding) takes its library's. Nothing is ever steel for lack
//! of a material.

use std::collections::BTreeSet;

use super::{BuildMeta, LibrarySourceDef, MaterialMeta};
use crate::document::DocState;
use crate::features::FeatureDef;
use crate::ids::{BodyUid, FeatureUid};
use crate::material::{self, Material, MaterialClass, MaterialReference};

/// Densities this close (g/cm³) are the same.
const SAME_DENSITY: f64 = 1e-9;

/// The words of a product that make it a metal.
const METAL_WORDS: [&str; 7] = [
    "steel", "iron", "alumin", "brass", "copper", "zinc", "bronze",
];

/// A text with its first letter upper case (`spruce` → `Spruce`).
fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// An id from a name: lower case letters and digits, others `_`
/// (`Steel, galvanised` → `steel_galvanised`).
fn id_of(name: &str) -> String {
    let mut id = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c);
        } else if !id.ends_with('_') && !id.is_empty() {
            id.push('_');
        }
    }
    id.trim_end_matches('_').to_owned()
}

/// The name a library's `material` block gives its material: the species
/// (`Spruce`), the product (`Steel, galvanised`), or both (`Plywood -
/// spruce`, as the dataset names its panels); None when it names neither.
pub fn library_material_name(meta: &MaterialMeta) -> Option<String> {
    let species = meta.species.trim();
    let product = meta.product.trim();
    match (product.is_empty(), species.is_empty()) {
        (true, true) => None,
        (true, false) => Some(capitalised(species)),
        (false, true) => Some(capitalised(product)),
        (false, false) => Some(format!("{} - {}", capitalised(product), species)),
    }
}

/// The density (g/cm³) of a library's `material` block, whose `density`
/// is kg/m³; None without one above zero.
pub fn library_material_density(meta: &MaterialMeta) -> Option<f64> {
    meta.density
        .as_ref()?
        .millimetres()
        .filter(|d| d.is_finite() && *d > 0.0)
        .map(|kg_per_m3| kg_per_m3 / 1000.0)
}

/// The material a library's `material` block makes, by its name and
/// density, of the class Wood for a species and Metal for a metal
/// product; `library` is the library it comes from (its `reference`), and
/// `appearance` its parts' look. None without a name or a density.
pub fn library_material(
    meta: &MaterialMeta,
    library: &str,
    appearance: Option<&str>,
) -> Option<Material> {
    let name = library_material_name(meta)?;
    let density = library_material_density(meta)?;
    let product = meta.product.to_lowercase();
    let class = if !meta.species.trim().is_empty() && product.is_empty() {
        Some(MaterialClass::Wood)
    } else if METAL_WORDS.iter().any(|w| product.contains(w)) {
        Some(MaterialClass::Metal)
    } else if !meta.species.trim().is_empty() {
        Some(MaterialClass::Wood)
    } else {
        None
    };
    let source = if library.trim().is_empty() {
        "library".to_owned()
    } else {
        library.trim().to_owned()
    };
    Some(Material {
        sheet_process: None,
        id: id_of(&name),
        name,
        density,
        appearance: appearance.map(str::to_owned),
        filament: Default::default(),
        slicer: Default::default(),
        variants: Vec::new(),
        appearances: Vec::new(),
        based_on: None,
        class,
        version: None,
        origin: None,
        properties: Default::default(),
        reference: Some(MaterialReference {
            source,
            material: Some(
                [meta.product.trim(), meta.species.trim()]
                    .into_iter()
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            grade: None,
            revision: None,
        }),
    })
}

impl DocState {
    /// The id of the design's material for a library's `material` block
    /// (`library_material`): a material of the design or a built-in one of
    /// its name and density (a library's material of the design offers the
    /// part's appearance from then on), else the block's own, added to the
    /// design (its id and name made free). A block with a density only is
    /// a built-in material of that density, if one has it. None when the
    /// block gives no material.
    pub fn library_block_material(
        &mut self,
        meta: &MaterialMeta,
        library: &str,
        appearance: Option<&str>,
    ) -> Option<String> {
        let density = library_material_density(meta)?;
        let same_density = |m: &Material| (m.density - density).abs() <= SAME_DENSITY;
        let Some(mut made) = library_material(meta, library, appearance) else {
            // A density only (a steel tube's 7850 kg/m³): the built-in
            // material of it, the design's taking a built-in's place.
            return material::built_in()
                .iter()
                .filter_map(|m| self.material(&m.id))
                .find(|m| same_density(m))
                .map(|m| m.id.clone());
        };
        let same = |m: &Material| m.name.eq_ignore_ascii_case(&made.name) && same_density(m);
        if let Some(i) = self.materials.iter().position(same) {
            let own = &mut self.materials[i];
            if let Some(look) = appearance
                && own.reference.as_ref().map(|r| r.source.as_str())
                    == made.reference.as_ref().map(|r| r.source.as_str())
                && !own.offers(look)
            {
                own.appearances.push(look.to_owned());
            }
            return Some(own.id.clone());
        }
        if let Some(found) = material::built_in()
            .iter()
            .filter_map(|m| self.material(&m.id))
            .find(|m| same(m))
        {
            return Some(found.id.clone());
        }
        let base = if made.id.is_empty() {
            "library_material".to_owned()
        } else {
            made.id.clone()
        };
        made.id = std::iter::once(base.clone())
            .chain((2..).map(|n| format!("{base}_{n}")))
            .find(|id| self.material(id).is_none())
            .expect("a free id");
        made.name = material::numbered_name(&made.name, |name| {
            self.material_name_taken(name, &made.id, None)
        });
        let id = made.id.clone();
        self.materials.push(made);
        Some(id)
    }

    /// The material a derived part `part` made from `source` is of, as its
    /// library gives it (see the module): the one its source's shown
    /// bodies all name (a material of the source design's own copied in),
    /// else its build block's `material` for the part's row. None when
    /// the library gives none.
    pub fn library_part_material(
        &mut self,
        part: FeatureUid,
        source: &LibrarySourceDef,
    ) -> Option<String> {
        let build = source.build.as_ref()?;
        build.get("material")?;
        let design = source.design.state().ok();
        let shown: Vec<&crate::document::BodyAttributes> = design
            .as_deref()
            .map(|d| d.body_attributes.values().filter(|a| a.visible).collect())
            .unwrap_or_default();
        let common = |pick: &dyn Fn(&crate::document::BodyAttributes) -> Option<String>| {
            let values: Vec<String> = shown.iter().filter_map(|a| pick(a)).collect();
            match values.first() {
                Some(first) if values.iter().all(|v| v == first) => Some(first.clone()),
                _ => None,
            }
        };
        if let Some(id) = common(&|a| a.material.clone()) {
            // A material of the library design's own comes with it.
            if let Some(design) = design.as_deref()
                && design.materials.iter().any(|m| m.id == id)
            {
                let mapped = self.copy_materials(design, &BTreeSet::from([id.clone()]));
                return Some(mapped.get(&id).cloned().unwrap_or(id));
            }
            return Some(id);
        }
        let appearance = common(&|a| a.appearance.clone());
        let row = match self.entry(part).map(|e| &e.def) {
            Some(FeatureDef::DerivedPart(def)) => def.config.clone(),
            _ => None,
        };
        let meta = BuildMeta::read_row(build, row.as_deref()).ok()?;
        let block = meta.material.as_ref()?;
        // A library design's own appearance has no meaning in another
        // design until it is copied there (BUILD copies it with the part).
        let look = appearance.as_deref().filter(|id| self.look(id).is_some());
        self.library_block_material(block, &source.library.library, look)
    }

    /// The library material of derived part `part` as it is now (see
    /// [`DocState::library_part_material`]); None for a part of no library
    /// source or whose library gives none.
    pub fn library_material_of(&mut self, part: FeatureUid) -> Option<String> {
        let source = match self.entry(part).map(|e| &e.def) {
            Some(FeatureDef::DerivedPart(def)) => self.library_sources.get(def.source).cloned(),
            _ => None,
        }?;
        self.library_part_material(part, &source)
    }

    /// After a part's row changed (a member's size or species): its bodies
    /// made of `before`, the material its library gave it, are made of the
    /// one it gives now; a material set by hand stays.
    pub fn follow_library_material(&mut self, part: FeatureUid, before: Option<&str>) {
        let Some(before) = before else {
            return;
        };
        let Some(after) = self.library_material_of(part) else {
            return;
        };
        for (uid, attributes) in self.body_attributes.iter_mut() {
            if uid.feature == part && attributes.material.as_deref() == Some(before) {
                attributes.material = Some(after.clone());
            }
        }
    }

    /// How many bodies a derived part made from `source` has: as many as
    /// its source design shows, at least one.
    pub fn library_part_body_count(source: &LibrarySourceDef) -> usize {
        source
            .design
            .state()
            .map(|d| d.body_attributes.values().filter(|a| a.visible).count())
            .unwrap_or(0)
            .max(1)
    }

    /// Gives the first `count` bodies of `part` that have no material of
    /// their own and follow no material binding `material`; whether any
    /// changed.
    pub fn give_part_material(&mut self, part: FeatureUid, count: usize, material: &str) -> bool {
        let mut changed = false;
        for i in 0..count {
            let entry = self
                .body_attributes
                .entry(BodyUid::new(part, i as u32))
                .or_default();
            if entry.material.is_none() && entry.material_expr.is_none() {
                entry.material = Some(material.to_owned());
                changed = true;
            }
        }
        changed
    }

    /// On open (mitcad#1817): the bodies of every derived part of a
    /// library source that have no material of their own get their
    /// library's, as BUILD gives it when it places the part. A design
    /// saved before BUILD gave materials opens with them.
    pub(crate) fn settle_library_materials(&mut self) {
        let parts: Vec<(FeatureUid, crate::build::SourceId)> = self
            .features
            .iter()
            .filter_map(|f| match &f.def {
                FeatureDef::DerivedPart(def) => Some((f.uid, def.source)),
                _ => None,
            })
            .collect();
        for (part, source) in parts {
            let Some(source) = self.library_sources.get(source).cloned() else {
                continue;
            };
            // A source whose design is still to be read from the project's
            // store waits until it is (`file::resolve` settles again).
            if source.design.text().is_none()
                || source
                    .build
                    .as_ref()
                    .is_none_or(|b| b.get("material").is_none())
            {
                continue;
            }
            let count = Self::library_part_body_count(&source);
            let missing = (0..count).any(|i| {
                self.body_attributes
                    .get(&BodyUid::new(part, i as u32))
                    .is_none_or(|a| a.material.is_none() && a.material_expr.is_none())
            });
            if !missing {
                continue;
            }
            if let Some(material) = self.library_part_material(part, &source) {
                self.give_part_material(part, count, &material);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::MetaValue;

    fn block(species: &str, product: &str, density: f64) -> MaterialMeta {
        MaterialMeta {
            species: species.to_owned(),
            product: product.to_owned(),
            density: Some(MetaValue::Number(density)),
            ..MaterialMeta::default()
        }
    }

    #[test]
    fn a_block_names_its_material_by_species_or_product() {
        let spruce = library_material(&block("spruce", "", 450.0), "parts", None).unwrap();
        assert_eq!(
            (spruce.id.as_str(), spruce.name.as_str()),
            ("spruce", "Spruce")
        );
        assert!((spruce.density - 0.45).abs() < 1e-12);
        assert_eq!(spruce.class, Some(MaterialClass::Wood));
        let galvanised =
            library_material(&block("", "steel, galvanised", 7850.0), "parts", None).unwrap();
        assert_eq!(galvanised.id, "steel_galvanised");
        assert_eq!(galvanised.name, "Steel, galvanised");
        assert_eq!(galvanised.class, Some(MaterialClass::Metal));
        let plywood = library_material(&block("spruce", "plywood", 460.0), "parts", None).unwrap();
        assert_eq!(plywood.name, "Plywood - spruce");
        assert!(library_material(&block("", "", 7850.0), "parts", None).is_none());
        let mut none = block("spruce", "", 0.0);
        assert!(library_material(&none, "parts", None).is_none());
        none.density = None;
        assert!(library_material(&none, "parts", None).is_none());
    }

    #[test]
    fn a_block_is_found_again_and_a_density_alone_is_a_built_in() {
        let mut state = DocState::default();
        let first = state
            .library_block_material(&block("spruce", "", 450.0), "parts", Some("wood_oak"))
            .unwrap();
        let again = state
            .library_block_material(&block("spruce", "", 450.0), "parts", Some("wood_maple"))
            .unwrap();
        assert_eq!(first, again);
        assert_eq!(state.materials.len(), 1);
        // The other look of the same library's spruce is offered too.
        assert!(state.materials[0].offers("wood_maple"));
        // A density only: the built-in of it, steel's 7.85 g/cm³.
        assert_eq!(
            state
                .library_block_material(&block("", "", 7850.0), "parts", None)
                .as_deref(),
            Some("steel")
        );
        assert_eq!(
            state.library_block_material(&block("", "", 1234.0), "parts", None),
            None
        );
        // Another density of a taken name: a name and id of its own.
        let other = state
            .library_block_material(&block("spruce", "", 470.0), "parts", None)
            .unwrap();
        assert_eq!(other, "spruce_2");
        assert_eq!(state.materials[1].name, "Spruce (2)");
    }
}
