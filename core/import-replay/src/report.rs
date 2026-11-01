// SPDX-License-Identifier: MIT
//! The import report: per design, how each timeline item came in
//! (parametric, as a fallback body, or not at all, with the reason), the
//! parameters, the ASM history used for verification, and the agreement
//! of the final bodies with the bodies stored in the file.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use mitcad_model::notice::{self, Notice, NoticeLevel};
use serde::Serialize;

pub use crate::components::ComponentReport;

/// The report of one file (an `.f3z` package has several designs).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportReport {
    pub file: String,
    pub designs: Vec<DesignReport>,
}

/// How a timeline item came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// As a Mitcad feature with its parameters.
    Parametric,
    /// As a feature, but simplified (a sketch without some constraints or
    /// dimensions, a feature with its values fixed).
    Partial,
    /// Replaced by a base feature holding the file's bodies after it.
    Fallback,
    /// Left out: nothing to import (no geometry), or it could not be.
    Skipped,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parametric => "parametric",
            Self::Partial => "partial",
            Self::Fallback => "fallback",
            Self::Skipped => "skipped",
        }
    }

    /// The outcome's level (mitcad#1040): an item that came in as asked is
    /// information; one that came in partly, as a fallback or not at all
    /// is a warning.
    pub fn level(self) -> NoticeLevel {
        match self {
            Self::Parametric => NoticeLevel::Info,
            Self::Partial | Self::Fallback | Self::Skipped => NoticeLevel::Warning,
        }
    }
}

/// Where the accepted candidate's design inputs came from. This is
/// independent of whether its geometry matched the stored result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputProvenance {
    /// Every required input is accounted for by a decoded field.
    Read,
    /// At least one input was selected by a heuristic or the history.
    Inferred,
    /// The translation does not yet account for every input's source.
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemReport {
    /// Timeline index.
    pub index: i64,
    pub name: String,
    /// The item's object type, `?` when unknown.
    #[serde(rename = "type")]
    pub object_type: String,
    pub outcome: Outcome,
    /// The Mitcad feature(s) made for it (`F7`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// The bodies after it matched the file's (ASM history); None when not
    /// checked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<bool>,
    /// Input provenance of an accepted modelling candidate; absent when
    /// no candidate was accepted (including a stored-body fallback).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_provenance: Option<InputProvenance>,
    /// How far the volume it adds or removes is from the file's change for
    /// it, relative to the change ([`crate::history::Change`]); None when
    /// not compared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_difference: Option<f64>,
    /// What its faces said where its change alone did not settle it
    /// (`crate::geometric`, mitcad#138); None when they were not asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometric_check: Option<String>,
    /// The largest distance (mm) beyond the exact tolerance at which a
    /// lookup of its decoded inputs took the replay's geometry for the
    /// file's (within `geometric_tolerance`, mitcad#224), or a sketch's
    /// solved points lie from the file's (mitcad#396); None when every
    /// lookup was exact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometric_deviation: Option<f64>,
    /// Why it is not parametric, or what was simplified or guessed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The Mitcad component it went into, when not the root (F6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Its retry on the file's state before it exactly, when it had one:
    /// the bodies kept and replaced (mitcad#283).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact_retry: Option<RetryReport>,
    /// Its decoded definition built a result that deviates from the file's
    /// (mitcad#379): how far off, where the item took the file's bodies
    /// instead. The import keeps it parametric when asked to
    /// (`Options::accept_deviations`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deviation: Option<crate::DeviationReport>,
    /// The deviating result it was kept with, parametric, at the user's
    /// choice (mitcad#379).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_deviation: Option<crate::DeviationReport>,
}

/// An item's retry on the file's state before it exactly (mitcad#283,
/// `crate::retry`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RetryReport {
    /// The history state.
    pub state: usize,
    /// How the bodies that match the state were treated: `keep` or
    /// `replace`.
    pub bodies: crate::RetryBodies,
    /// The replay's bodies that stayed: they match the state (exactly or
    /// within the geometric tolerance) and stay parametric.
    pub kept: Vec<String>,
    /// The replay's bodies the state's stored bodies replaced for the retry.
    pub replaced: Vec<String>,
    /// Whether the item gave its state on them; else the replacement was
    /// undone (or none was made: nothing differed).
    pub matched: bool,
}

impl RetryReport {
    /// One line for the text report.
    pub fn text(&self) -> String {
        let list = |bodies: &[String]| {
            if bodies.is_empty() {
                "none".to_owned()
            } else {
                bodies.join(", ")
            }
        };
        format!(
            "retry on state {} ({}): kept {}; replaced {}; {}",
            self.state,
            self.bodies.as_str(),
            list(&self.kept),
            list(&self.replaced),
            if self.matched {
                "it gave its state"
            } else if self.replaced.is_empty() {
                "nothing to replace: not tried again"
            } else {
                "no state there either"
            }
        )
    }
}

/// The user's answer to the question what retries on the file's exact
/// state do with the bodies that match it (mitcad#283).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RetryQuestionReport {
    /// The item the question came at.
    pub item: String,
    pub answer: crate::RetryBodies,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ParameterReport {
    /// Parameters created with their expressions.
    pub imported: usize,
    /// Created with the value only: the expression did not evaluate.
    pub literal: Vec<String>,
    /// Renamed because Mitcad reserves the name: (name in the file, new name).
    pub renamed: Vec<(String, String)>,
    /// Not created (no expression and no value to fall back on), with the
    /// reason.
    pub skipped: Vec<(String, String)>,
    /// Evaluated to another value than the file stores.
    pub mismatched: Vec<String>,
}

/// The ASM history (the `.smbh` blobs' rolled-back body states) used to
/// check the replay and to take fallback bodies from.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HistoryReport {
    /// Body states, oldest first (0 when the file has no history).
    pub states: usize,
    /// States built for comparisons.
    pub built: usize,
    /// States whose bodies could not be rebuilt from the rolled-back ASM
    /// data (the items there are not checked).
    pub unbuilt: usize,
    /// Items whose result matched a state.
    pub matched: usize,
    /// The replay followed the history to its last state.
    pub reached_end: bool,
}

/// One body stored in the file against the replay.
#[derive(Debug, Clone, Serialize)]
pub struct BodyReport {
    /// The body's name in the file, or its blob and record.
    pub file: String,
    pub file_volume: f64,
    /// The Mitcad body matched to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mitcad: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mitcad_volume: Option<f64>,
    /// (Mitcad − file) / file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_difference: Option<f64>,
    /// Largest distance between the surfaces, mm (geometric comparison).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_deviation: Option<f64>,
    /// (|A − B| + |B − A|) / max(|A|, |B|) from the boolean differences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_difference: Option<f64>,
    /// The stored body's blob and record (`StoredBody::source`), and the
    /// Mitcad body matched to it: what the appearances join on
    /// (mitcad#228).
    #[serde(skip)]
    pub source: String,
    #[serde(skip)]
    pub uid: Option<mitcad_model::BodyUid>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DesignReport {
    /// `name.f3d`, `pkg.f3z!<document>` or the dump file.
    pub label: String,
    /// `f3d_stream` (the file's design streams) or an external dump's mode.
    pub source: String,
    /// The geometric tolerance the import used (mm,
    /// `Options::geometric_tolerance`, mitcad#224).
    pub geometric_tolerance: f64,
    pub parameters: ParameterReport,
    pub items: Vec<ItemReport>,
    pub history: HistoryReport,
    /// The file's final bodies against the replayed ones.
    pub bodies: Vec<BodyReport>,
    /// Replayed bodies that match no body of the file.
    pub extra_bodies: Vec<String>,
    pub warnings: Vec<String>,
    /// The file's components and occurrences as Mitcad's (F6).
    pub components: ComponentReport,
    /// The light bulbs of sketches and construction geometry (mitcad#6).
    pub light_bulbs: LightBulbReport,
    /// The file's joints, as-built joints, joint origins and ground items
    /// (mitcad#55).
    pub joints: JointReport,
    /// Where the import was stopped on request (T1e), when it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped: Option<StopReport>,
    /// Where the process ran low on memory (mitcad#80), when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low_memory: Option<LowMemoryReport>,
    /// The file's assembly relationships (mitcad#155).
    #[serde(skip_serializing_if = "RelationshipReport::is_empty")]
    pub relationships: RelationshipReport,
    /// The modelling items suppressed in the file (mitcad#180).
    #[serde(skip_serializing_if = "crate::SuppressedReport::is_empty")]
    pub suppressed: crate::SuppressedReport,
    /// The documents the design refers to (its components of other
    /// documents, mitcad#196): where their files were found and how they
    /// came in.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<ReferenceReport>,
    /// The file's appearances and physical materials (mitcad#228).
    #[serde(skip_serializing_if = "crate::appearance_report::AppearanceReport::is_empty")]
    pub appearances: crate::appearance_report::AppearanceReport,
    /// The user's answer, when the import asked what retries on the file's
    /// exact state do with the bodies that match it (mitcad#283).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_question: Option<RetryQuestionReport>,
    /// The design's bodies by the body objects the file names them by: what
    /// a design that derives them names (mitcad#324).
    #[serde(skip)]
    pub body_entities: BTreeMap<u64, mitcad_model::BodyUid>,
}

/// A document a design refers to (mitcad#196; filled in by the caller that
/// imports the documents, `mitcad-ffi`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ReferenceReport {
    /// The document's URN in the design.
    pub document: String,
    /// Its name, where the reference folder gives it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The version the design uses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// Its file, when found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// How the file was found (`reference folder`, `chosen`, `by its
    /// GUID`, `package`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found: Option<String>,
    /// `linked` (its components filled from the file), `missing` (no
    /// file: they stay empty), `skipped` (the user skipped it) or `failed`
    /// (see `note`).
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Mitcad's components filled from it (or standing for it, empty).
    pub components: Vec<String>,
    /// The design's occurrences of its components.
    pub occurrences: usize,
    /// Its own import's timeline items by outcome.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub items: BTreeMap<String, usize>,
    /// The documents it refers to in turn.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<ReferenceReport>,
}

impl ReferenceReport {
    /// Its name and version for messages: `Part (version 2)`; without a
    /// name, `the document of Inserted component (version 2)` (the
    /// component standing for it), else its URN.
    pub fn title(&self) -> String {
        let name = self.name.clone().unwrap_or_else(|| {
            if self.components.is_empty() {
                self.document.clone()
            } else {
                format!("the document of {}", self.components.join(", "))
            }
        });
        match self.version {
            Some(v) => format!("{name} (version {v})"),
            None => name,
        }
    }

    fn text(&self, out: &mut String, depth: usize) {
        let indent = "  ".repeat(depth + 2);
        let items = if self.items.is_empty() {
            String::new()
        } else {
            let n = |o: &str| self.items.get(o).copied().unwrap_or(0);
            format!(
                "; its items: {} parametric, {} partial, {} fallback, {} skipped",
                n("parametric"),
                n("partial"),
                n("fallback"),
                n("skipped")
            )
        };
        let _ = writeln!(
            out,
            "{indent}{}: {}, occurrences: {}{}{}{items}{}",
            self.title(),
            self.status,
            self.occurrences,
            self.file
                .as_deref()
                .map_or(String::new(), |f| format!(", from {f}")),
            self.found
                .as_deref()
                .map_or(String::new(), |f| format!(" ({f})")),
            self.note
                .as_deref()
                .map_or(String::new(), |n| format!(": {n}"))
        );
        for r in &self.references {
            r.text(out, depth + 1);
        }
    }
}

/// How the file's assembly relationships came in (mitcad#155;
/// `relationships.rs`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RelationshipReport {
    /// Relationship items that came in as constraint sets.
    pub sets: usize,
    /// The constraints (rows) of the relationship items.
    pub rows: usize,
    /// Of them, those that came in as constraint rows.
    pub translated: usize,
    /// Of their sides, those on fixed geometry (no face or edge of the
    /// component gives the file's plane, axis or point).
    pub fixed_sides: usize,
    /// Of their sides, those on a component of another document (fixed
    /// geometry of the empty component standing for it).
    pub inserted_sides: usize,
    /// Tangent relationships.
    pub tangents: usize,
    /// Rows and tangent relationships left out, by why.
    pub left_out: BTreeMap<String, usize>,
}

impl RelationshipReport {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Counts a row or tangent relationship left out.
    pub fn skipped(&mut self, why: &str) {
        *self.left_out.entry(why.to_owned()).or_default() += 1;
    }
}

/// The light bulbs of the imported sketches and construction features
/// (mitcad#6). Where the file's light bulb is not known (not decoded, an
/// older file), Mitcad's default decides: a sketch is hidden while a
/// feature uses it, construction geometry is shown.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LightBulbReport {
    /// Sketches imported (each once, not the copies made for other
    /// components).
    pub sketch_bulbs: usize,
    /// Of them, those whose light bulb the file does not tell.
    pub sketch_bulbs_unknown: usize,
    /// Construction planes, axes and points imported.
    pub construction_bulbs: usize,
    /// Of them, those whose light bulb the file does not tell.
    pub construction_bulbs_unknown: usize,
    /// Light bulbs kept because they differ from the default: a used
    /// sketch shown, an unused one hidden, construction geometry hidden.
    pub bulbs_set: usize,
}

/// How the file's joints, as-built joints, joint origins and ground items
/// came in (mitcad#55; `joints.rs`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct JointReport {
    /// Joints that came in as joints: they hold where the file places the
    /// occurrences.
    pub joints: usize,
    /// Of their sides, those on a fixed frame (no face, edge or joint
    /// origin of the replay gives the file's frame).
    pub fixed_sides: usize,
    /// Of their sides, those on a component of another document (on the
    /// origin of the empty component standing for it, mitcad#75).
    pub inserted_sides: usize,
    /// Joints kept as as-built joints: they do not hold where the file
    /// places the occurrences (or their sides are not known).
    pub kept_as_built: usize,
    /// As-built joints that came in as as-built joints.
    pub as_built: usize,
    /// Joint origins that came in as joint origins.
    pub origins: usize,
    /// Ground items whose occurrence is grounded.
    pub grounded: usize,
    /// Captured positions that came in as `capture_position` features
    /// (mitcad#75).
    pub positions: usize,
    /// Joints, as-built joints, joint origins, ground items and captured
    /// positions left out (a level inside a component of another
    /// document, a motion not decoded, ...).
    pub skipped: usize,
    /// Rigid groups that came in as rigid groups (mitcad#81).
    pub rigid_groups: usize,
}

impl JointReport {
    fn any(&self) -> bool {
        *self != Self::default()
    }
}

/// Where an import was stopped on request (T1e, `Options::stop`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct StopReport {
    /// The timeline index of the modelling item it stopped at: it and the
    /// ones after it took the file's bodies without being replayed. None:
    /// stopped after the last one (only the final comparison was cut
    /// short, by volume only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<i64>,
    /// That item's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Where an import first ran low on memory (mitcad#80,
/// `Progress::low_memory`): the definition being tried then was cut short,
/// and the modelling items after it took the file's bodies without being
/// replayed while the memory stayed low.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LowMemoryReport {
    /// The timeline index of the item being replayed; None before the
    /// first item or after the last one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<i64>,
    /// That item's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// How much memory was in use, against what limit.
    pub memory: String,
    /// How many modelling items took the file's bodies while it was low
    /// (their notes say "the import ran low on memory").
    pub items: usize,
}

impl DesignReport {
    /// Items per outcome.
    pub fn counts(&self) -> BTreeMap<Outcome, usize> {
        let mut counts = BTreeMap::new();
        for item in &self.items {
            *counts.entry(item.outcome).or_default() += 1;
        }
        counts
    }

    /// The import's notices (mitcad#1040): what its items' outcomes left
    /// out, the worst first, then its warnings.
    pub fn notices(&self) -> Vec<Notice> {
        let items = |n| notice::count(n, "item", "items");
        let mut notices = notice::outcome_notices(self.counts(), Outcome::level, |o, n| match o {
            Outcome::Parametric => format!("{} imported", items(n)),
            Outcome::Partial => format!("{} imported partly", items(n)),
            Outcome::Fallback => format!("{} replaced by the file's bodies", items(n)),
            Outcome::Skipped => format!("{} skipped", items(n)),
        });
        notices.extend(self.warnings.iter().map(|w| Notice::warning(w.as_str())));
        notices
    }

    /// (type, outcome) → count, for coverage tables.
    pub fn coverage(&self) -> BTreeMap<(String, Outcome), usize> {
        let mut counts = BTreeMap::new();
        for item in &self.items {
            *counts
                .entry((item.object_type.clone(), item.outcome))
                .or_default() += 1;
        }
        counts
    }

    /// Accepted modelling candidates by input provenance. Unknown is
    /// counted explicitly; a low candidate rank is not proof of decoding.
    pub fn input_counts(&self) -> BTreeMap<InputProvenance, usize> {
        let mut counts = BTreeMap::new();
        for provenance in self.items.iter().filter_map(|i| i.input_provenance) {
            *counts.entry(provenance).or_default() += 1;
        }
        counts
    }

    /// The largest relative volume difference of the final bodies, None
    /// when no body was compared.
    pub fn worst_volume_difference(&self) -> Option<f64> {
        self.bodies
            .iter()
            .map(|b| b.volume_difference.map_or(1.0, f64::abs))
            .reduce(f64::max)
    }

    /// A readable summary.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let counts = self.counts();
        let n = |o: Outcome| counts.get(&o).copied().unwrap_or(0);
        let _ = writeln!(
            out,
            "{}: {} timeline items: {} parametric, {} partial, {} fallback, {} skipped",
            self.label,
            self.items.len(),
            n(Outcome::Parametric),
            n(Outcome::Partial),
            n(Outcome::Fallback),
            n(Outcome::Skipped)
        );
        let p = &self.parameters;
        let inputs = self.input_counts();
        if !inputs.is_empty() {
            let n = |p| inputs.get(&p).copied().unwrap_or(0);
            let _ = writeln!(
                out,
                "  modeling inputs: {} read, {} inferred, {} unknown",
                n(InputProvenance::Read),
                n(InputProvenance::Inferred),
                n(InputProvenance::Unknown)
            );
        }
        let deviations: Vec<f64> = self
            .items
            .iter()
            .filter_map(|i| i.geometric_deviation)
            .collect();
        if !deviations.is_empty() {
            let _ = writeln!(
                out,
                "  geometric tolerance {} mm: {} items took inputs beyond 0.001 mm (up to {:.4} mm)",
                self.geometric_tolerance,
                deviations.len(),
                deviations.iter().copied().fold(0.0, f64::max)
            );
        }
        let _ = writeln!(
            out,
            "  parameters: {} imported, {} as values, {} renamed, {} skipped, {} mismatched",
            p.imported,
            p.literal.len(),
            p.renamed.len(),
            p.skipped.len(),
            p.mismatched.len()
        );
        if self.history.states > 0 {
            let _ = writeln!(
                out,
                "  ASM history: {} states, {} built, {} not rebuilt, {} items matched{}",
                self.history.states,
                self.history.built,
                self.history.unbuilt,
                self.history.matched,
                if self.history.reached_end {
                    ", reached the stored design"
                } else {
                    ""
                }
            );
        } else {
            let _ = writeln!(
                out,
                "  no ASM history: the replay is not verified step by step"
            );
        }
        let c = &self.components;
        if c.components + c.occurrences + c.external > 0 {
            let _ = writeln!(
                out,
                "  components: {} made, {} occurrences placed, {} of other documents ({} filled \
                 from their files){}",
                c.components,
                c.occurrences,
                c.external,
                c.linked,
                if c.placed_inside > 0 {
                    format!(
                        ", {} captured positions inside them placed",
                        c.placed_inside
                    )
                } else {
                    String::new()
                }
            );
        }
        if !self.references.is_empty() {
            let linked = self
                .references
                .iter()
                .filter(|r| r.status == "linked")
                .count();
            let _ = writeln!(
                out,
                "  documents referred to: {} linked from their files, {} not",
                linked,
                self.references.len() - linked
            );
            for r in &self.references {
                r.text(&mut out, 0);
            }
        }
        let a = &self.appearances;
        if !a.is_empty() {
            let _ = writeln!(
                out,
                "  appearances: bodies {}/{}, faces {}/{}, occurrence overrides {}/{}; {} \
                 appearances made; materials set {}/{} ({} materials, Mitcad's for {}); {} \
                 body records of bodies not in the stored design",
                a.bodies_set,
                a.bodies,
                a.faces_set,
                a.faces,
                a.overrides_set,
                a.overrides,
                a.appearances.len(),
                a.materials_set,
                a.materials,
                a.physical_materials.len(),
                a.physical_materials
                    .iter()
                    .filter(|m| m.mitcad.is_some())
                    .count(),
                a.not_in_design
            );
            for g in &a.gaps {
                let _ = writeln!(out, "    {g}");
            }
        }
        let b = &self.light_bulbs;
        if b.sketch_bulbs + b.construction_bulbs > 0 {
            let _ = writeln!(
                out,
                "  light bulbs: {} sketches ({} unknown), {} construction features ({} unknown), \
                 {} kept where they differ from Mitcad's default",
                b.sketch_bulbs,
                b.sketch_bulbs_unknown,
                b.construction_bulbs,
                b.construction_bulbs_unknown,
                b.bulbs_set
            );
        }
        let j = &self.joints;
        if j.any() {
            let _ = writeln!(
                out,
                "  joints: {} as joints ({} sides on fixed frames, {} on components of other \
                 documents), {} kept as as-built joints, {} as-built joints, {} joint origins, {} \
                 grounded, {} captured positions, {} rigid groups, {} left out",
                j.joints,
                j.fixed_sides,
                j.inserted_sides,
                j.kept_as_built,
                j.as_built,
                j.origins,
                j.grounded,
                j.positions,
                j.rigid_groups,
                j.skipped
            );
        }
        let r = &self.relationships;
        if !r.is_empty() {
            let _ = writeln!(
                out,
                "  relationships: {} constraint sets, {} of {} constraints as rows ({} sides on \
                 fixed geometry, {} on components of other documents), {} tangent relationships, \
                 {} left out",
                r.sets,
                r.translated,
                r.rows,
                r.fixed_sides,
                r.inserted_sides,
                r.tangents,
                r.left_out.values().sum::<usize>()
            );
        }
        let s = &self.suppressed;
        if s.rolled_back_as_features + s.rolled_back_as_placeholders > 0 {
            let _ = writeln!(
                out,
                "  rolled back in the file: {} modeling items as features after the marker, {} \
                 as placeholders",
                s.rolled_back_as_features, s.rolled_back_as_placeholders
            );
        }
        if s.suppressed_as_features + s.suppressed_as_placeholders > 0 {
            let indirect = if s.suppressed_indirectly > 0 {
                format!(
                    " ({} of them through another suppressed item)",
                    s.suppressed_indirectly
                )
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "  suppressed in the file: {} modeling items as suppressed features, {} as \
                 placeholders{indirect}",
                s.suppressed_as_features, s.suppressed_as_placeholders
            );
        }
        for item in &self.items {
            let features = if item.features.is_empty() {
                String::new()
            } else {
                format!(" -> {}", item.features.join(", "))
            };
            let verified = match item.verified {
                Some(true) => " (verified)",
                Some(false) => " (not verified)",
                None => "",
            };
            let features = match &item.component {
                Some(c) => format!("{features} in {c}"),
                None => features,
            };
            let _ = writeln!(
                out,
                "  {:>4} {:<24} {:<26} {:<10}{features}{verified}{}",
                item.index,
                truncate(&item.name, 24),
                truncate(&item.object_type, 26),
                item.outcome.as_str(),
                item.note
                    .as_deref()
                    .map_or(String::new(), |n| format!(": {n}"))
            );
            if let Some(retry) = &item.exact_retry {
                let _ = writeln!(out, "       {}", retry.text());
            }
            if let Some(d) = &item.deviation {
                let _ = writeln!(out, "       deviating result (not kept): {}", d.summary());
            }
        }
        if let Some(q) = &self.retry_question {
            let _ = writeln!(
                out,
                "  asked at {} what retries on the file's exact state do with the bodies that \
                 match it: {}",
                q.item,
                q.answer.as_str()
            );
        }
        for body in &self.bodies {
            let _ = match (&body.mitcad, body.volume_difference) {
                (Some(m), Some(dv)) => writeln!(
                    out,
                    "  body {}: {} {:.3} mm3, volume {:+.4} %{}",
                    body.file,
                    m,
                    body.mitcad_volume.unwrap_or_default(),
                    dv * 100.0,
                    body.max_deviation
                        .map_or(String::new(), |d| format!(", deviation {d:.4} mm"))
                ),
                _ => writeln!(
                    out,
                    "  body {}: {:.3} mm3, not in the replay",
                    body.file, body.file_volume
                ),
            };
        }
        for extra in &self.extra_bodies {
            let _ = writeln!(out, "  extra body in the replay: {extra}");
        }
        for warning in &self.warnings {
            let _ = writeln!(out, "  warning: {warning}");
        }
        out
    }
}

impl ImportReport {
    pub fn text(&self) -> String {
        self.designs.iter().map(DesignReport::text).collect()
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_owned()
    } else {
        let mut t: String = text.chars().take(width - 1).collect();
        t.push('~');
        t
    }
}

#[cfg(test)]
mod notice_tests {
    use super::*;

    #[test]
    fn outcomes_map_onto_the_levels() {
        // (mitcad#1040) Partial and fallback items and skipped ones are
        // warnings; the import's notices from them are tested on a real
        // import in the import's `timeline_note_tests`.
        assert_eq!(Outcome::Parametric.level(), NoticeLevel::Info);
        assert_eq!(Outcome::Partial.level(), NoticeLevel::Warning);
        assert_eq!(Outcome::Fallback.level(), NoticeLevel::Warning);
        assert_eq!(Outcome::Skipped.level(), NoticeLevel::Warning);
    }
}
