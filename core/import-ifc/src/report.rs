// SPDX-License-Identifier: MIT
//! The IFC import's report: the file, the structure made of it, and a line
//! for every product the file lists (each accounted for: a body, a mesh,
//! an occurrence, a component, axes, a cut, or why it was not imported),
//! with the checks against the file's element quantities.

use std::collections::BTreeMap;

use mitcad_model::notice::{self, Notice, NoticeLevel};
use serde::Serialize;

/// How a product came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Every item as features (sketches, extrusions, revolutions, cuts).
    Parametric,
    /// Every item as the file's bodies (breps), base features.
    Stored,
    /// Face sets: mesh bodies.
    Mesh,
    /// Some items as features, others as the file's bodies or meshes.
    Mixed,
    /// Occurrences of the components of its type's representation maps.
    Occurrence,
    /// Some items built, others not (see the notes).
    Partial,
    /// A site, building, storey or facility: a component.
    Component,
    /// A grid: construction axes.
    Axes,
    /// An opening: cut from its element.
    Cut,
    /// Not imported (the reason in the notes).
    NotImported,
    /// Nothing of its shape could be built (the reason in the notes).
    Failed,
}

impl Outcome {
    pub fn name(self) -> &'static str {
        match self {
            Outcome::Parametric => "parametric",
            Outcome::Stored => "stored",
            Outcome::Mesh => "mesh",
            Outcome::Mixed => "mixed",
            Outcome::Occurrence => "occurrence",
            Outcome::Partial => "partial",
            Outcome::Component => "component",
            Outcome::Axes => "axes",
            Outcome::Cut => "cut",
            Outcome::NotImported => "not_imported",
            Outcome::Failed => "failed",
        }
    }

    /// The outcome's level (mitcad#1040): a product that came in (as
    /// features, the file's bodies, meshes, occurrences, a component, axes
    /// or a cut) is information; one that came in partly or was not
    /// imported is a warning; one of which nothing could be built is an
    /// error.
    pub fn level(self) -> NoticeLevel {
        match self {
            Outcome::Parametric
            | Outcome::Stored
            | Outcome::Mesh
            | Outcome::Mixed
            | Outcome::Occurrence
            | Outcome::Component
            | Outcome::Axes
            | Outcome::Cut => NoticeLevel::Info,
            Outcome::Partial | Outcome::NotImported => NoticeLevel::Warning,
            Outcome::Failed => NoticeLevel::Error,
        }
    }
}

/// One product of the file.
#[derive(Debug, Clone, Serialize)]
pub struct ProductLine {
    /// Its instance number.
    pub id: u64,
    pub global_id: String,
    /// Its type (`IFCWALL`) and what it is to the import.
    pub kind: String,
    pub class: String,
    pub name: String,
    pub outcome: Outcome,
    /// The component it went into (or is, for a spatial element).
    pub component: Option<String>,
    /// The bodies made of it.
    pub bodies: Vec<String>,
    /// The occurrences that stand for it.
    pub occurrences: Vec<String>,
    /// The features made of it.
    pub features: usize,
    /// Its items: the kinds built as features and as stored bodies.
    pub parametric: Vec<String>,
    pub stored: Vec<String>,
    /// Its bodies' volume (mm³; meshes when closed).
    pub volume: Option<f64>,
    /// Its bodies' area (mm²).
    pub area: Option<f64>,
    /// The volume its element quantities state (name, mm³), and the
    /// relative difference.
    pub quantity: Option<(String, f64)>,
    pub quantity_difference: Option<f64>,
    pub notes: Vec<String>,
}

/// The volumes compared with the file's element quantities.
#[derive(Debug, Clone, Default, Serialize)]
pub struct QuantityCheck {
    pub compared: usize,
    /// Within [`QUANTITY_TOLERANCE`].
    pub agree: usize,
    pub worst: f64,
    pub differ: Vec<String>,
}

/// The relative difference within which a body's volume agrees with the
/// file's quantity (the file's quantities are its writer's own measures,
/// often of the solid before openings or joins).
pub const QUANTITY_TOLERANCE: f64 = 1e-3;

/// A representation map made a component.
#[derive(Debug, Clone, Serialize)]
pub struct MapLine {
    pub map: u64,
    pub name: String,
    pub component: Option<String>,
    pub occurrences: usize,
    pub bodies: usize,
    /// Its bodies' volume (mm³).
    pub volume: Option<f64>,
    /// Its bodies' area (mm²).
    pub area: Option<f64>,
    pub notes: Vec<String>,
}

/// The whole report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct IfcReport {
    pub file: String,
    pub schema: String,
    pub schema_name: String,
    pub view_definition: Option<String>,
    pub originating_system: String,
    pub project: String,
    pub length_unit: String,
    pub angle_unit: String,
    /// The document's units were set to the file's (an empty document).
    pub units: Option<String>,
    pub products: Vec<ProductLine>,
    /// Products by outcome.
    pub outcomes: BTreeMap<String, usize>,
    pub maps: Vec<MapLine>,
    pub components: usize,
    pub storey_planes: usize,
    pub grid_axes: usize,
    pub appearances: usize,
    pub features: usize,
    pub bodies: usize,
    pub quantities: QuantityCheck,
    /// The properties the elements brought (identity, property sets,
    /// quantities, their types' sets; mitcad#496).
    pub properties: usize,
    /// The volume of every body made (mm³): the elements', and each map's
    /// times its occurrences (closed meshes included).
    pub total_volume: f64,
    /// The area of every body made (mm²), counted likewise.
    pub total_area: f64,
    /// The reader's warnings and what was assumed.
    pub warnings: Vec<String>,
    /// When the process ran low on memory: what was in use, and the
    /// product the import stopped at.
    pub low_memory: Option<String>,
    pub seconds: f64,
}

impl IfcReport {
    /// Every product has a line, and none is missing an outcome.
    pub fn accounted(&self) -> usize {
        self.products.len()
    }

    /// The import's notices (mitcad#1040): what its products' outcomes
    /// left out or could not build, the worst first, then its warnings.
    pub fn notices(&self) -> Vec<Notice> {
        let mut counts: BTreeMap<Outcome, usize> = BTreeMap::new();
        for product in &self.products {
            *counts.entry(product.outcome).or_default() += 1;
        }
        let products = |n| notice::count(n, "product", "products");
        let mut notices = notice::outcome_notices(counts, Outcome::level, |o, n| match o {
            Outcome::Partial => format!("{} imported partly", products(n)),
            Outcome::NotImported => format!("{} not imported", products(n)),
            Outcome::Failed => format!("{} could not be built", products(n)),
            _ => format!("{} imported", products(n)),
        });
        notices.extend(self.warnings.iter().map(|w| Notice::warning(w.as_str())));
        notices
    }

    /// The report as text.
    pub fn text(&self) -> String {
        let mut t = format!(
            "Imported {} ({} {}, view {}, from {}): {} products in {} components\n",
            self.file,
            self.schema,
            self.schema_name,
            self.view_definition.as_deref().unwrap_or("-"),
            if self.originating_system.is_empty() {
                "-"
            } else {
                &self.originating_system
            },
            self.products.len(),
            self.components,
        );
        let outcomes: Vec<String> = self
            .outcomes
            .iter()
            .map(|(k, v)| format!("{v} {k}"))
            .collect();
        t += &format!("  outcomes: {}\n", outcomes.join(", "));
        t += &format!(
            "  {} features, {} bodies, {} representation maps as components, {} storey planes, {} grid axes, {} appearances; units: {}, {}{}\n",
            self.features,
            self.bodies,
            self.maps.len(),
            self.storey_planes,
            self.grid_axes,
            self.appearances,
            self.length_unit,
            self.angle_unit,
            match &self.units {
                Some(u) => format!(" (the document's set to {u})"),
                None => String::new(),
            }
        );
        t += &format!(
            "  total volume of the bodies: {:.3} mm3, area {:.3} mm2\n",
            self.total_volume, self.total_area
        );
        let q = &self.quantities;
        t += &format!(
            "  check: {} of {} element volumes agree with the file's quantities within {:e} (largest difference {:.2e})\n",
            q.agree, q.compared, QUANTITY_TOLERANCE, q.worst
        );
        for d in &q.differ {
            t += &format!("    {d}\n");
        }
        for p in &self.products {
            let what = match p.outcome {
                Outcome::Occurrence => format!("{} occurrences", p.occurrences.len()),
                Outcome::Component | Outcome::Axes | Outcome::Cut | Outcome::NotImported => {
                    String::new()
                }
                _ => format!(
                    "{} bodies, {} features{}",
                    p.bodies.len(),
                    p.features,
                    match p.volume {
                        Some(v) => format!(", volume {v:.1} mm3"),
                        None => String::new(),
                    }
                ),
            };
            t += &format!(
                "  #{} {} '{}' {}: {}{}{}\n",
                p.id,
                p.kind,
                p.name,
                p.global_id,
                p.outcome.name(),
                if what.is_empty() {
                    String::new()
                } else {
                    format!(" ({what})")
                },
                match &p.component {
                    Some(c) => format!(" in {c}"),
                    None => String::new(),
                }
            );
            for n in &p.notes {
                t += &format!("    {n}\n");
            }
        }
        for m in &self.maps {
            t += &format!(
                "  map #{} '{}': {} occurrences, {} bodies{}\n",
                m.map,
                m.name,
                m.occurrences,
                m.bodies,
                match &m.component {
                    Some(c) => format!(" in {c}"),
                    None => String::new(),
                }
            );
            for n in &m.notes {
                t += &format!("    {n}\n");
            }
        }
        for w in &self.warnings {
            t += &format!("  warning: {w}\n");
        }
        if let Some(m) = &self.low_memory {
            t += &format!("  low on memory: {m}\n");
        }
        t
    }
}

#[cfg(test)]
mod notice_tests {
    use super::*;

    #[test]
    fn outcomes_map_onto_the_levels() {
        // (mitcad#1040) What came in is information, what was left out a
        // warning, what could not be built an error.
        for outcome in [
            Outcome::Parametric,
            Outcome::Stored,
            Outcome::Mesh,
            Outcome::Mixed,
            Outcome::Occurrence,
            Outcome::Component,
            Outcome::Axes,
            Outcome::Cut,
        ] {
            assert_eq!(outcome.level(), NoticeLevel::Info, "{}", outcome.name());
        }
        assert_eq!(Outcome::Partial.level(), NoticeLevel::Warning);
        assert_eq!(Outcome::NotImported.level(), NoticeLevel::Warning);
        assert_eq!(Outcome::Failed.level(), NoticeLevel::Error);
    }
}
