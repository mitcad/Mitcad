// SPDX-License-Identifier: MIT
//! What a Bill of Materials says of a component's form and origin
//! (mitcad#1980), and the Sheet Metal list. Its BOM Structure is in
//! `bom_structure.rs` (mitcad#1985).
//!
//! - **Kind** (the computed property `Computed / Kind`): worked out from
//!   the component's own bodies, each by the first that holds of: Sheet
//!   metal (a body with a valid sheet state), Build profile (a BUILD part
//!   that is a profile or cut to length), Library part (a library or BUILD
//!   part), Imported (a body without history, from an imported file),
//!   Solid. Bodies of different kinds make the component Mixed; a
//!   component without bodies of its own is an Assembly (a library or
//!   BUILD part without them keeps its own kind).
//! - **The Sheet Metal list** (the `sheet_metal_list` query): each
//!   sheet-metal body with its component's quantity, its thickness and
//!   material, and its developed blank's size, area and bend count, from
//!   its Flat Pattern's settings (a new pattern's when it has none).

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{Document, MaterialSource, PartsListRow, properties::NO_MATERIAL};
use crate::build::BuildRole;
use crate::features::FeatureDef;
use crate::ids::{BodyUid, ComponentUid};
use crate::kernel::Kernel;

/// A component's form and origin in a Bill of Materials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentKind {
    SheetMetal,
    BuildProfile,
    LibraryPart,
    Imported,
    Solid,
    /// Bodies of different kinds.
    Mixed,
    /// No bodies of its own: a subassembly.
    Assembly,
}

impl ComponentKind {
    /// Every kind, in the order the precedence gives them.
    pub const ALL: [Self; 7] = [
        Self::SheetMetal,
        Self::BuildProfile,
        Self::LibraryPart,
        Self::Imported,
        Self::Solid,
        Self::Mixed,
        Self::Assembly,
    ];

    /// As the property and the lists write it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SheetMetal => "Sheet metal",
            Self::BuildProfile => "Build profile",
            Self::LibraryPart => "Library part",
            Self::Imported => "Imported",
            Self::Solid => "Solid",
            Self::Mixed => "Mixed",
            Self::Assembly => "Assembly",
        }
    }

    /// A kind by its text, without regard to case.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(text.trim()))
    }
}

/// One sheet-metal body of the Sheet Metal list.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetMetalRow {
    pub body: BodyUid,
    /// The body's name as the Browser shows it.
    pub name: String,
    pub component: ComponentUid,
    pub component_name: String,
    /// How many times the design places its component.
    pub quantity: usize,
    /// The material's thickness, mm.
    pub thickness: Option<f64>,
    /// Its material's name, or "No material".
    pub material: String,
    /// Its developed blank: the outline's box (width, height, mm), its
    /// area (mm²) and its bends; none when it cannot be developed.
    pub size: Option<[f64; 2]>,
    pub area: Option<f64>,
    pub bends: Option<usize>,
    /// Why it cannot be developed, in words.
    pub problem: Option<String>,
}

impl<K: Kernel> Document<K> {
    /// Whether a body at the marker is sheet metal: it has a valid sheet
    /// state that belongs to its current shape.
    pub fn is_sheet_metal_body(&self, body: BodyUid) -> bool {
        self.sheet_state_current(body)
            .is_some_and(|state| state.status() == "valid")
    }

    /// A body's sheet state when it belongs to the body's current shape
    /// and component (as the `sheet_state` query checks it).
    fn sheet_state_current(&self, body: BodyUid) -> Option<&crate::sheet::SheetState> {
        let result = self.last_result();
        let state = result.sheets.get(&body)?;
        let shape = result.bodies.get(&body)?;
        (state.value.shape_generation == format!("{:032x}", shape.version)
            && result.owners.get(&body) == Some(&state.value.owner))
        .then_some(&state.value)
    }

    /// A body made by an imported file's base feature (a body without
    /// history).
    fn is_imported_body(&self, body: BodyUid) -> bool {
        self.feature(body.feature)
            .is_some_and(|f| matches!(f.def, FeatureDef::Base(_)))
    }

    /// Whether a component is a library or BUILD part, and whether it is a
    /// BUILD profile or cut to length.
    pub(super) fn library_and_profile(
        &self,
        component: ComponentUid,
        row: Option<&PartsListRow>,
    ) -> (bool, bool, bool) {
        let def = self.assembly().component(component);
        let info = self.part_info(component);
        let library = def.is_some_and(|d| d.library.is_some())
            || info.is_some()
            || row.is_some_and(|r| r.library.is_some());
        let cut = info.as_ref().is_some_and(|i| i.length.is_some());
        let profile = cut || info.is_some_and(|i| i.meta.role == BuildRole::Profile);
        (library, profile, cut)
    }

    /// The component's own bodies at the marker.
    fn own_bodies(&self, component: ComponentUid) -> Vec<BodyUid> {
        let result = self.last_result();
        result
            .owners
            .iter()
            .filter(|(body, owner)| **owner == component && result.bodies.contains_key(body))
            .map(|(body, _)| *body)
            .collect()
    }

    /// A component's Kind (mitcad#1980; the module's documentation says
    /// how). `row`: its parts list row, when the caller has it.
    pub fn component_kind(
        &self,
        component: ComponentUid,
        row: Option<&PartsListRow>,
    ) -> ComponentKind {
        let (library, profile, _) = self.library_and_profile(component, row);
        let mut kinds: Vec<ComponentKind> = Vec::new();
        for body in self.own_bodies(component) {
            let kind = self.kind_of_body(body, component, library, profile);
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        match kinds.as_slice() {
            [] if profile => ComponentKind::BuildProfile,
            [] if library => ComponentKind::LibraryPart,
            [] => ComponentKind::Assembly,
            [one] => *one,
            _ => ComponentKind::Mixed,
        }
    }

    /// One body's kind (a Bill of Materials' row of one body of a
    /// component of several): its component's kind as one of its bodies
    /// gives it.
    pub fn body_kind(&self, body: BodyUid) -> ComponentKind {
        let Some(component) = self.last_result().owners.get(&body).copied() else {
            return ComponentKind::Solid;
        };
        let (library, profile, _) = self.library_and_profile(component, None);
        self.kind_of_body(body, component, library, profile)
    }

    /// A body's kind by the precedence: Sheet metal, Build profile,
    /// Library part, Imported, Solid.
    fn kind_of_body(
        &self,
        body: BodyUid,
        component: ComponentUid,
        library: bool,
        profile: bool,
    ) -> ComponentKind {
        // A design linked as a component brings its bodies as bodies
        // without history: they are no import.
        let linked = self
            .assembly()
            .component(component)
            .is_some_and(|d| d.link.is_some());
        if self.is_sheet_metal_body(body) {
            ComponentKind::SheetMetal
        } else if profile {
            ComponentKind::BuildProfile
        } else if library {
            ComponentKind::LibraryPart
        } else if !linked && self.is_imported_body(body) {
            ComponentKind::Imported
        } else {
            ComponentKind::Solid
        }
    }

    /// The Sheet Metal list (mitcad#1980): every sheet-metal body at the
    /// marker whose component the design places, in the order of the
    /// bodies' ids.
    pub fn sheet_metal_list(&self) -> Vec<SheetMetalRow> {
        let mut quantities: BTreeMap<ComponentUid, usize> = self
            .parts_list()
            .into_iter()
            .map(|row| (row.component, row.quantity))
            .collect();
        quantities.insert(ComponentUid::ROOT, 1);
        let result = self.last_result();
        let bodies: Vec<BodyUid> = result
            .sheets
            .keys()
            .copied()
            .filter(|body| self.is_sheet_metal_body(*body))
            .collect();
        let mut rows = Vec::new();
        for body in bodies {
            let Some(component) = result.owners.get(&body).copied() else {
                continue;
            };
            let quantity = quantities.get(&component).copied().unwrap_or(0);
            if quantity == 0 {
                continue;
            }
            let thickness = result
                .sheets
                .get(&body)
                .and_then(|state| result.sheet_rules.get(&state.value.rules))
                .map(|rules| rules.value.thickness);
            let (material, source) = self.body_material(body, None);
            let material = if source == MaterialSource::Default {
                NO_MATERIAL.to_owned()
            } else {
                material.name.clone()
            };
            let mut row = SheetMetalRow {
                body,
                name: self.body_name(body),
                component,
                component_name: self.assembly().name(component),
                quantity,
                thickness,
                material,
                size: None,
                area: None,
                bends: None,
                problem: None,
            };
            match self.developed_flat_pattern(body) {
                Ok(outcome) => {
                    if let Ok(pattern) = &*outcome {
                        row.thickness = Some(pattern.thickness);
                        row.size = pattern
                            .bounds()
                            .map(|(lo, hi)| [hi[0] - lo[0], hi[1] - lo[1]]);
                        row.area = Some(pattern.area);
                        row.bends = Some(pattern.bends.len());
                    }
                }
                Err(error) => {
                    row.problem = Some(crate::notice::user_message(&error.to_string()));
                }
            }
            rows.push(row);
        }
        rows
    }

    /// The `sheet_metal_list` query: `rows` of the Sheet Metal list, each
    /// `body`, `name`, `component`, `component_name`, `quantity`,
    /// `thickness`, `material`, `width`, `height` (its developed blank's
    /// box, mm), `area` (mm²), `bends` and, when it cannot be developed,
    /// `problem` (in words).
    pub fn sheet_metal_list_json(&self) -> Value {
        let rows: Vec<Value> = self
            .sheet_metal_list()
            .into_iter()
            .map(|row| {
                let mut value = json!({
                    "body": row.body,
                    "name": row.name,
                    "component": row.component,
                    "component_name": row.component_name,
                    "quantity": row.quantity,
                    "thickness": row.thickness,
                    "material": row.material,
                    "width": row.size.map(|s| s[0]),
                    "height": row.size.map(|s| s[1]),
                    "area": row.area,
                    "bends": row.bends,
                });
                if let Some(problem) = row.problem {
                    value["problem"] = json!(problem);
                }
                value
            })
            .collect();
        json!({"rows": rows})
    }
}
