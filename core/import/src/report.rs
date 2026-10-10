// SPDX-License-Identifier: MIT
//! The import report: per design, how each timeline item came in
//! (parametric, as a fallback body, or not at all, with the reason), the
//! parameters, the ASM history used for verification, and the agreement
//! of the final bodies with the bodies stored in the file.

use std::collections::BTreeMap;
use std::fmt::Write as _;

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
    /// How far the volume it adds or removes is from the file's change for
    /// it, relative to the change ([`crate::history::Change`]); None when
    /// not compared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_difference: Option<f64>,
    /// What its faces said where its change alone did not settle it
    /// (`crate::geometric`, mitcad#138); None when they were not asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometric_check: Option<String>,
    /// Why it is not parametric, or what was simplified or guessed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The Mitcad component it went into, when not the root (F6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ParameterReport {
    /// Parameters created with their expressions.
    pub imported: usize,
    /// Created with the value only: the expression did not evaluate.
    pub literal: Vec<String>,
    /// Renamed because Mitcad reserves the name: (name in the file, new name).
    pub renamed: Vec<(String, String)>,
    /// Not created (text parameters), with the reason.
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
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DesignReport {
    /// `name.f3d`, `pkg.f3z!<document>` or the dump file.
    pub label: String,
    /// `f3d_stream` (the file's design streams) or an external dump's mode.
    pub source: String,
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
                "  components: {} made, {} occurrences placed, {} of other documents placed empty",
                c.components, c.occurrences, c.external
            );
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
