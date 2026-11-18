// SPDX-License-Identifier: MIT
//! A component's BOM Structure and the Bill of Materials' structured and
//! parts-only views (mitcad#1985).
//!
//! **BOM Structure** (`Bill of Materials / BOM Structure`) says how a
//! component enters a Bill of Materials. It is worked out as a library's
//! values are, Purchased for a library part that is not cut to length and
//! Normal otherwise, and never stored; a stored value of the same set and
//! key, the user's choice, overrides it.
//!
//! - **Normal**: listed, numbered and counted.
//! - **Purchased**: one line item: its children are listed in neither
//!   view.
//! - **Phantom**: never listed itself; its children move up to its level,
//!   their quantities multiplied by its own.
//! - **Reference**: listed without an item number, and left out of the
//!   quantities, the mass and the volume; its children are not listed.
//! - **Inseparable**: one item in the parts-only view (its children are
//!   not listed there); the structured view lists its children below it.
//!
//! The **structured view** lists the design's components as its assembly
//! nests them, each under its parent with how many times the parent holds
//! it and an item number that says where (`2.1`); the **parts-only view**
//! lists the parts the design is made of, each once with how many the
//! whole design needs, numbered 1, 2, …. Both give each row's mass and
//! volume of one (an assembly's with what it holds) and the totals.

use std::collections::BTreeMap;

use super::{Document, MaterialSource, PartsListRow};
use crate::ids::{BodyUid, ComponentUid};
use crate::kernel::Kernel;
use crate::properties::{BOM_SET, BOM_STRUCTURE_KEY, PropertyOwner, PropertyValue};

/// How a component enters a Bill of Materials.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum BomStructure {
    #[default]
    Normal,
    Purchased,
    Phantom,
    Reference,
    Inseparable,
}

impl BomStructure {
    /// Every structure, in the order the property's choices list them.
    pub const ALL: [Self; 5] = [
        Self::Normal,
        Self::Purchased,
        Self::Phantom,
        Self::Reference,
        Self::Inseparable,
    ];

    /// As the property and the lists write it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Purchased => "Purchased",
            Self::Phantom => "Phantom",
            Self::Reference => "Reference",
            Self::Inseparable => "Inseparable",
        }
    }

    /// A structure by its text, without regard to case.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|s| s.as_str().eq_ignore_ascii_case(text.trim()))
    }

    /// Its children are listed in neither view below it.
    fn closed(self) -> bool {
        matches!(self, Self::Purchased | Self::Reference)
    }
}

/// Which of a Bill of Materials' views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BomView {
    /// As the assembly nests the components, indented.
    Structured,
    /// The parts the design is made of, each once.
    Parts,
}

/// A row of a structured or parts-only view ([`Document::bom_view`]).
#[derive(Debug, Clone, PartialEq)]
pub struct BomViewRow {
    pub component: ComponentUid,
    /// A body of the root's own, a row of its own (its component the root).
    pub body: Option<BodyUid>,
    /// 1 right below the design; deeper in the structured view.
    pub level: usize,
    /// `2`, `2.1`; none for a Reference.
    pub item: Option<String>,
    /// How many: in its parent (structured), in the whole design (parts
    /// only); 0 for a Reference.
    pub quantity: usize,
    /// How many times it is placed there, a Reference's too.
    pub placed: usize,
    pub structure: BomStructure,
    /// The mass of one, kg (none while a body has no material), and its
    /// volume, mm³: its own bodies and those of what it holds, without
    /// References.
    pub mass: Option<f64>,
    pub volume: f64,
}

/// A view's rows with the whole design's mass (kg; none while a counted
/// body has no material) and volume (mm³), References left out.
#[derive(Debug, Clone, PartialEq)]
pub struct BomViewList {
    pub rows: Vec<BomViewRow>,
    pub mass: Option<f64>,
    pub volume: f64,
}

/// The deepest assembly a view walks into.
const MAX_DEPTH: usize = 64;

impl<K: Kernel> Document<K> {
    /// What a component's BOM Structure is worked out as (mitcad#1985):
    /// Purchased for a library part that is not cut to length, Normal
    /// otherwise.
    pub fn bom_structure_default(
        &self,
        component: ComponentUid,
        row: Option<&PartsListRow>,
    ) -> BomStructure {
        let (library, _, cut) = self.library_and_profile(component, row);
        if library && !cut {
            BomStructure::Purchased
        } else {
            BomStructure::Normal
        }
    }

    /// A component's BOM Structure: the user's stored choice, else what it
    /// is worked out as.
    pub fn bom_structure(
        &self,
        component: ComponentUid,
        row: Option<&PartsListRow>,
    ) -> BomStructure {
        self.state
            .properties
            .get(
                PropertyOwner::Component(component),
                BOM_SET,
                BOM_STRUCTURE_KEY,
            )
            .and_then(|p| match &p.value {
                PropertyValue::Text(text) => BomStructure::parse(text),
                _ => None,
            })
            .unwrap_or_else(|| self.bom_structure_default(component, row))
    }

    /// Every component's BOM Structure (the root's is Normal).
    pub fn bom_structures(&self) -> BTreeMap<ComponentUid, BomStructure> {
        let mut map: BTreeMap<ComponentUid, BomStructure> = self
            .assembly()
            .components
            .iter()
            .map(|c| (c.uid, self.bom_structure(c.uid, None)))
            .collect();
        map.insert(ComponentUid::ROOT, BomStructure::Normal);
        map
    }

    /// A Bill of Materials' structured or parts-only view (mitcad#1985;
    /// the module's documentation says how each structure acts).
    pub fn bom_view(&self, view: BomView) -> BomViewList {
        let walk = Walk {
            structures: self.bom_structures(),
            masses: std::cell::RefCell::new(BTreeMap::new()),
            doc: self,
        };
        let mut rows = Vec::new();
        match view {
            BomView::Structured => walk.structured(ComponentUid::ROOT, 1, "", 0, &mut rows),
            BomView::Parts => {
                let mut found: Vec<(ComponentUid, usize, BomStructure)> = Vec::new();
                walk.parts(ComponentUid::ROOT, 1, 0, &mut found);
                let mut number = 0;
                for (component, placed, structure) in found {
                    let reference = structure == BomStructure::Reference;
                    let (mass, volume) = walk.mass(component, 0);
                    if !reference {
                        number += 1;
                    }
                    rows.push(BomViewRow {
                        component,
                        body: None,
                        level: 1,
                        item: (!reference).then(|| number.to_string()),
                        quantity: if reference { 0 } else { placed },
                        placed,
                        structure,
                        mass,
                        volume,
                    });
                }
            }
        }
        // The root's own bodies: rows of their own after its components.
        let mut number = rows
            .iter()
            .filter(|r| r.level == 1 && r.item.is_some())
            .count();
        for body in self.own_bodies_sorted(ComponentUid::ROOT) {
            number += 1;
            let (mass, volume) = walk.body_mass(body);
            rows.push(BomViewRow {
                component: ComponentUid::ROOT,
                body: Some(body),
                level: 1,
                item: Some(number.to_string()),
                quantity: 1,
                placed: 1,
                structure: BomStructure::Normal,
                mass,
                volume,
            });
        }
        let (mass, volume) = walk.mass(ComponentUid::ROOT, 0);
        BomViewList { rows, mass, volume }
    }

    /// A component's bodies at the marker, in the order of their ids.
    fn own_bodies_sorted(&self, component: ComponentUid) -> Vec<BodyUid> {
        let mut bodies: Vec<BodyUid> = self
            .component_bodies(component)
            .into_iter()
            .filter(|body| self.body_shape(*body).is_some())
            .collect();
        bodies.sort();
        bodies
    }
}

/// A view's walk through the assembly: the structures, and each
/// component's mass and volume once.
struct Walk<'d, K: Kernel> {
    doc: &'d Document<K>,
    structures: BTreeMap<ComponentUid, BomStructure>,
    #[allow(clippy::type_complexity)]
    masses: std::cell::RefCell<BTreeMap<ComponentUid, (Option<f64>, f64)>>,
}

impl<K: Kernel> Walk<'_, K> {
    fn structure(&self, component: ComponentUid) -> BomStructure {
        self.structures.get(&component).copied().unwrap_or_default()
    }

    /// The components right below `parent` with how many times it holds
    /// each, in the order they were placed; a Phantom's children in its
    /// place, as many times more as it is placed.
    fn children(&self, parent: ComponentUid, depth: usize) -> Vec<(ComponentUid, usize)> {
        let mut out: Vec<(ComponentUid, usize)> = Vec::new();
        if depth > MAX_DEPTH {
            return out;
        }
        let mut add = |component: ComponentUid, count: usize| match out
            .iter_mut()
            .find(|(c, _)| *c == component)
        {
            Some((_, n)) => *n += count,
            None => out.push((component, count)),
        };
        let mut own: Vec<(ComponentUid, usize)> = Vec::new();
        for occurrence in self.doc.assembly().children(parent) {
            match own.iter_mut().find(|(c, _)| *c == occurrence.component) {
                Some((_, n)) => *n += 1,
                None => own.push((occurrence.component, 1)),
            }
        }
        for (component, count) in own {
            if self.structure(component) == BomStructure::Phantom {
                for (child, n) in self.children(component, depth + 1) {
                    add(child, n * count);
                }
            } else {
                add(component, count);
            }
        }
        out
    }

    /// The structured view's rows below `parent`, numbered after `prefix`.
    fn structured(
        &self,
        parent: ComponentUid,
        level: usize,
        prefix: &str,
        depth: usize,
        rows: &mut Vec<BomViewRow>,
    ) {
        let mut number = 0;
        for (component, placed) in self.children(parent, depth) {
            let structure = self.structure(component);
            let reference = structure == BomStructure::Reference;
            let item = if reference {
                None
            } else {
                number += 1;
                Some(format!("{prefix}{number}"))
            };
            let (mass, volume) = self.mass(component, 0);
            rows.push(BomViewRow {
                component,
                body: None,
                level,
                item: item.clone(),
                quantity: if reference { 0 } else { placed },
                placed,
                structure,
                mass,
                volume,
            });
            if let Some(item) = item
                && !structure.closed()
                && depth < MAX_DEPTH
            {
                self.structured(component, level + 1, &format!("{item}."), depth + 1, rows);
            }
        }
    }

    /// The parts-only view's components below `parent` (placed `times`),
    /// each with how many the design needs, in the order first found.
    fn parts(
        &self,
        parent: ComponentUid,
        times: usize,
        depth: usize,
        found: &mut Vec<(ComponentUid, usize, BomStructure)>,
    ) {
        fn add(
            found: &mut Vec<(ComponentUid, usize, BomStructure)>,
            component: ComponentUid,
            count: usize,
            structure: BomStructure,
        ) {
            match found.iter_mut().find(|(c, _, _)| *c == component) {
                Some((_, n, _)) => *n += count,
                None => found.push((component, count, structure)),
            }
        }
        for (component, placed) in self.children(parent, depth) {
            let structure = self.structure(component);
            let count = placed * times;
            match structure {
                BomStructure::Reference | BomStructure::Purchased | BomStructure::Inseparable => {
                    add(found, component, count, structure);
                    continue;
                }
                _ => {}
            }
            let has_children = self.doc.assembly().children(component).next().is_some();
            let has_bodies = !self.doc.own_bodies_sorted(component).is_empty();
            // A part: bodies of its own, or nothing below it.
            if structure != BomStructure::Phantom && (has_bodies || !has_children) {
                add(found, component, count, structure);
            }
            if depth < MAX_DEPTH {
                self.parts(component, count, depth + 1, found);
            }
        }
    }

    /// The mass (kg; none while a body has no material) and volume (mm³) of
    /// one body.
    fn body_mass(&self, body: BodyUid) -> (Option<f64>, f64) {
        let Some(shape) = self.doc.body_shape(body) else {
            return (Some(0.0), 0.0);
        };
        let Ok(properties) = self.doc.kernel().mass_properties(shape) else {
            return (Some(0.0), 0.0);
        };
        let (material, source) = self.doc.body_material(body, None);
        let volume = properties.volume.abs();
        // No mass while a body has no material (mitcad#1817).
        let mass = (source != MaterialSource::Default).then_some(volume * material.density * 1e-6);
        (mass, volume)
    }

    /// The mass and volume of one of a component: its own bodies and those
    /// of what it holds, References left out.
    fn mass(&self, component: ComponentUid, depth: usize) -> (Option<f64>, f64) {
        if let Some(known) = self.masses.borrow().get(&component) {
            return *known;
        }
        let mut mass = Some(0.0);
        let mut volume = 0.0;
        for body in self.doc.own_bodies_sorted(component) {
            let (m, v) = self.body_mass(body);
            mass = mass.zip(m).map(|(a, b)| a + b);
            volume += v;
        }
        if depth < MAX_DEPTH {
            let children: Vec<ComponentUid> = self
                .doc
                .assembly()
                .children(component)
                .map(|o| o.component)
                .collect();
            for child in children {
                if self.structure(child) == BomStructure::Reference {
                    continue;
                }
                let (m, v) = self.mass(child, depth + 1);
                mass = mass.zip(m).map(|(a, b)| a + b);
                volume += v;
            }
        }
        self.masses.borrow_mut().insert(component, (mass, volume));
        (mass, volume)
    }
}
