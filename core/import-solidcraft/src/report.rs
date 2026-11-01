// SPDX-License-Identifier: MIT
//! The `.solidcraft` import's report: every timeline feature of the file
//! with how it came in, the parameters, the sketches with their constraint
//! counts, the components and joints, the bodies measured after the
//! import, and the check against expected values when given.

use mitcad_model::notice::{self, Notice, NoticeLevel};
use serde::Serialize;
use serde_json::{Value, json};

/// How a feature of the file came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// As MitCad features with its parameters.
    Parametric,
    /// As MitCad features, with something of it left out or fixed (the
    /// notes say what).
    Partial,
    /// MitCad has no counterpart: a suppressed placeholder holds its place.
    Unmapped,
    /// Translated, but the MitCad feature failed: a suppressed placeholder
    /// holds its place.
    Failed,
    /// Nothing to bring in (folders of the file's own, a body transfer
    /// that the components already make).
    Skipped,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Parametric => "parametric",
            Outcome::Partial => "partial",
            Outcome::Unmapped => "unmapped",
            Outcome::Failed => "failed",
            Outcome::Skipped => "skipped",
        }
    }

    /// The outcome's level (mitcad#1040): a feature that came in as asked,
    /// or that had nothing to bring in, is information; one that came in
    /// partly or has no counterpart is a warning; one whose feature failed
    /// is an error (its result is missing).
    pub fn level(self) -> NoticeLevel {
        match self {
            Outcome::Parametric | Outcome::Skipped => NoticeLevel::Info,
            Outcome::Partial | Outcome::Unmapped => NoticeLevel::Warning,
            Outcome::Failed => NoticeLevel::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeatureReport {
    /// The file's feature id and name.
    pub id: u64,
    pub name: String,
    /// The file's kind (`extrude`, `surface_loft`, …).
    pub kind: String,
    pub outcome: Outcome,
    /// The MitCad features it became (`F7`), in timeline order.
    pub features: Vec<String>,
    /// Suppressed in the file (and so in MitCad).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
    /// After the file's timeline marker (rolled back).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub rolled_back: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParameterReport {
    pub name: String,
    /// The file's expression and MitCad's.
    pub expression: String,
    pub mitcad: String,
    /// The MitCad parameter's name when it differs (a reserved name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed: Option<String>,
    /// A user parameter, a sketch dimension's or a feature input's.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SketchReport {
    pub id: u64,
    pub name: String,
    pub points: usize,
    pub curves: usize,
    /// The file's constraints and dimensions, and how many came in.
    pub constraints: usize,
    pub dimensions: usize,
    pub constraints_mapped: usize,
    pub dimensions_mapped: usize,
    /// Constraints, curves and links left out, with why.
    pub dropped: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BodyReport {
    /// The file's body name (the MitCad body's display name).
    pub name: String,
    pub body: String,
    /// The occurrence path it is placed by (empty: the root's).
    pub path: String,
    pub volume_mm3: f64,
    pub area_mm2: f64,
    /// World coordinates.
    pub bbox_min: [f64; 3],
    pub bbox_max: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JointReport {
    pub id: u64,
    pub name: String,
    pub kind: String,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    pub notes: Vec<String>,
}

/// A difference from the expected values.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckReport {
    pub passed: bool,
    pub checks: usize,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SolidcraftReport {
    pub file: String,
    pub design: String,
    /// The format the file was saved in, and the upgrade steps that ran.
    pub format: u32,
    pub upgrades: Vec<String>,
    pub container: bool,
    pub units: String,
    pub features: Vec<FeatureReport>,
    pub parameters: Vec<ParameterReport>,
    pub sketches: Vec<SketchReport>,
    pub components: usize,
    pub occurrences: usize,
    pub joints: Vec<JointReport>,
    /// Motion links (mitcad#466): `kind` is the coupling, `id` the link's
    /// number in the file.
    pub links: Vec<JointReport>,
    pub bodies: Vec<BodyReport>,
    /// Kinds of the file without a MitCad counterpart, each with its count.
    pub unmapped: Vec<(String, usize)>,
    /// Parameters changed after the import (`set_parameters`).
    pub changed: Vec<(String, String)>,
    pub notes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<CheckReport>,
}

impl SolidcraftReport {
    pub fn count(&self, outcome: Outcome) -> usize {
        self.features
            .iter()
            .filter(|f| f.outcome == outcome)
            .count()
    }

    pub fn counts(&self) -> Vec<(Outcome, usize)> {
        [
            Outcome::Parametric,
            Outcome::Partial,
            Outcome::Unmapped,
            Outcome::Failed,
            Outcome::Skipped,
        ]
        .into_iter()
        .map(|o| (o, self.count(o)))
        .collect()
    }

    /// The summary line.
    pub fn summary(&self) -> String {
        let counts: Vec<String> = self
            .counts()
            .into_iter()
            .map(|(o, n)| format!("{n} {}", o.as_str()))
            .collect();
        format!(
            "{} (SolidCraft format {}): {} features: {}; {} bodies",
            self.file,
            self.format,
            self.features.len(),
            counts.join(", "),
            self.bodies.len()
        )
    }

    /// A readable report.
    pub fn text(&self) -> String {
        let mut out = format!("{}\n", self.summary());
        if !self.upgrades.is_empty() {
            out += &format!("upgraded: {}\n", self.upgrades.join(", "));
        }
        for f in &self.features {
            out += &format!(
                "  {:<24} {:<18} {:<11} {}{}\n",
                f.name,
                f.kind,
                f.outcome.as_str(),
                f.features.join(" "),
                if f.notes.is_empty() {
                    String::new()
                } else {
                    format!(": {}", f.notes.join("; "))
                }
            );
        }
        for s in &self.sketches {
            out += &format!(
                "  sketch {}: {} curves, constraints {}/{}, dimensions {}/{}{}\n",
                s.name,
                s.curves,
                s.constraints_mapped,
                s.constraints,
                s.dimensions_mapped,
                s.dimensions,
                if s.dropped.is_empty() {
                    String::new()
                } else {
                    format!(" (left out: {})", s.dropped.join("; "))
                }
            );
        }
        for p in self.parameters.iter().filter(|p| p.error.is_some()) {
            out += &format!(
                "  parameter {} = {}: {}\n",
                p.name,
                p.expression,
                p.error.as_deref().unwrap_or_default()
            );
        }
        for j in &self.joints {
            out += &format!(
                "  joint {} {} {}{}\n",
                j.name,
                j.kind,
                j.outcome.as_str(),
                if j.notes.is_empty() {
                    String::new()
                } else {
                    format!(": {}", j.notes.join("; "))
                }
            );
        }
        for l in &self.links {
            out += &format!(
                "  link {} {} {}{}\n",
                l.name,
                l.kind,
                l.outcome.as_str(),
                if l.notes.is_empty() {
                    String::new()
                } else {
                    format!(": {}", l.notes.join("; "))
                }
            );
        }
        if !self.unmapped.is_empty() {
            let kinds: Vec<String> = self
                .unmapped
                .iter()
                .map(|(k, n)| format!("{k} ({n})"))
                .collect();
            out += &format!("without a MitCad counterpart: {}\n", kinds.join(", "));
        }
        for b in &self.bodies {
            out += &format!(
                "  body {} ({}{}): volume {:.3} mm3, box [{:.3}, {:.3}, {:.3}] .. [{:.3}, {:.3}, {:.3}]\n",
                b.name,
                b.body,
                if b.path.is_empty() {
                    String::new()
                } else {
                    format!(" in {}", b.path)
                },
                b.volume_mm3,
                b.bbox_min[0],
                b.bbox_min[1],
                b.bbox_min[2],
                b.bbox_max[0],
                b.bbox_max[1],
                b.bbox_max[2]
            );
        }
        for n in &self.notes {
            out += &format!("note: {n}\n");
        }
        if let Some(c) = &self.check {
            out += &format!(
                "check: {} ({} checks{})\n",
                if c.passed { "passed" } else { "FAILED" },
                c.checks,
                if c.failures.is_empty() {
                    String::new()
                } else {
                    format!(": {}", c.failures.join("; "))
                }
            );
        }
        out
    }

    /// The compact result of the `import_solidcraft` command.
    pub fn command_result(&self) -> Value {
        let counts: serde_json::Map<String, Value> = self
            .counts()
            .into_iter()
            .map(|(o, n)| (o.as_str().to_owned(), json!(n)))
            .collect();
        json!({
            "file": self.file,
            "design": self.design,
            "format": "solidcraft",
            "items": self.features.len(),
            "bodies": self.bodies.len(),
            "counts": counts,
            // What the outcomes left out (mitcad#1040).
            "notices": notice::to_json(&self.notices()),
            "report": serde_json::to_value(self).expect("the report serializes"),
        })
    }

    /// The import's notices (mitcad#1040): what its features' outcomes
    /// left out or could not build, the worst first.
    pub fn notices(&self) -> Vec<Notice> {
        let features = |n| notice::count(n, "feature", "features");
        notice::outcome_notices(self.counts(), Outcome::level, |o, n| match o {
            Outcome::Partial => format!("{} imported partly", features(n)),
            Outcome::Unmapped => format!(
                "{} without a counterpart, kept as placeholders",
                features(n)
            ),
            Outcome::Failed => format!("{} failed, kept as placeholders", features(n)),
            Outcome::Parametric | Outcome::Skipped => format!("{} imported", features(n)),
        })
    }
}

#[cfg(test)]
mod notice_tests {
    use super::*;

    fn feature(outcome: Outcome) -> FeatureReport {
        FeatureReport {
            id: 1,
            name: "Extrude1".to_owned(),
            kind: "extrude".to_owned(),
            outcome,
            features: Vec::new(),
            suppressed: false,
            rolled_back: false,
            notes: Vec::new(),
        }
    }

    #[test]
    fn outcomes_map_onto_the_levels() {
        assert_eq!(Outcome::Parametric.level(), NoticeLevel::Info);
        assert_eq!(Outcome::Skipped.level(), NoticeLevel::Info);
        assert_eq!(Outcome::Partial.level(), NoticeLevel::Warning);
        assert_eq!(Outcome::Unmapped.level(), NoticeLevel::Warning);
        assert_eq!(Outcome::Failed.level(), NoticeLevel::Error);
        let report = SolidcraftReport {
            features: vec![
                feature(Outcome::Parametric),
                feature(Outcome::Unmapped),
                feature(Outcome::Unmapped),
                feature(Outcome::Failed),
            ],
            ..SolidcraftReport::default()
        };
        assert_eq!(
            report.notices(),
            vec![
                Notice::error("1 feature failed, kept as placeholders"),
                Notice::warning("2 features without a counterpart, kept as placeholders"),
            ]
        );
        assert_eq!(report.command_result()["notices"][0]["level"], "error");
    }
}
