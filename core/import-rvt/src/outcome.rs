// SPDX-License-Identifier: MIT
//! How the elements came in, and the import's notices from it
//! (mitcad#1051, `docs/ui-conventions.md` "Messages and severity"): an
//! element that came in as features, as its parts, as a mesh or as the
//! reference's body is information; one that came in as its box is a
//! warning (the file gave nothing better); one of which no body could be
//! built is an error. Elements of the kinds built as features that came in
//! otherwise, the walls' openings that are missing and the links whose
//! models are not there are warnings too, with their counts; the report's
//! own words stay on its lines.

use std::collections::BTreeMap;

use mitcad_model::notice::{self, Notice, NoticeLevel};

use crate::Report;

/// How an element came in: an [`crate::ElementLine`]'s `outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Outcome {
    /// Rebuilt from its parameters as features.
    Features,
    /// Its parts are its shape (a curtain wall's panels and mullions, a
    /// railing's rails, a stair's runs), elements of their own.
    Parts,
    /// A mesh: of the geometry the file keeps, or the one its record saves.
    Mesh,
    /// The reference's bodies of its GlobalId.
    ReferenceBody,
    /// Its bounding box.
    Box,
    /// No body could be built.
    NotBuilt,
}

impl Outcome {
    pub const ALL: [Outcome; 6] = [
        Outcome::Features,
        Outcome::Parts,
        Outcome::Mesh,
        Outcome::ReferenceBody,
        Outcome::Box,
        Outcome::NotBuilt,
    ];

    /// The outcome's word on the report's lines.
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Features => "features",
            Outcome::Parts => "parts",
            Outcome::Mesh => "mesh",
            Outcome::ReferenceBody => "reference body",
            Outcome::Box => "box",
            Outcome::NotBuilt => "not built",
        }
    }

    /// The outcome of a report line's word; None for any other text.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.as_str() == text)
    }

    /// The outcome's level: what came in with a shape of its own is
    /// information, a box a warning, nothing built an error.
    pub fn level(self) -> NoticeLevel {
        match self {
            Outcome::Features | Outcome::Parts | Outcome::Mesh | Outcome::ReferenceBody => {
                NoticeLevel::Info
            }
            Outcome::Box => NoticeLevel::Warning,
            Outcome::NotBuilt => NoticeLevel::Error,
        }
    }
}

/// "1 element came in as its box", "3 elements came in as boxes".
fn describe(outcome: Outcome, n: usize) -> String {
    let elements = notice::count(n, "element", "elements");
    match outcome {
        Outcome::Box if n == 1 => format!("{elements} came in as its box"),
        Outcome::Box => format!("{elements} came in as boxes"),
        Outcome::NotBuilt => format!("{elements} could not be built"),
        other => format!("{elements} came in as {}", other.as_str()),
    }
}

impl Report {
    /// The import's elements by their outcome.
    pub fn outcome_counts(&self) -> BTreeMap<Outcome, usize> {
        let mut counts = BTreeMap::new();
        for line in &self.elements {
            if let Some(outcome) = Outcome::parse(&line.outcome) {
                *counts.entry(outcome).or_default() += 1;
            }
        }
        counts
    }

    /// The import's notices (mitcad#1051): what its elements' outcomes left
    /// out or could not build, the elements of the kinds built as features
    /// that came in otherwise, the walls' openings that are missing, the
    /// links whose models were not found, then its warnings; the worst
    /// first.
    pub fn notices(&self) -> Vec<Notice> {
        let mut notices = notice::outcome_notices(self.outcome_counts(), Outcome::level, describe);
        let fallbacks: usize = self.fallbacks.values().sum();
        if fallbacks > 0 {
            notices.push(Notice::warning(format!(
                "{} of the kinds built as features came in as meshes, bodies or boxes instead",
                notice::count(fallbacks, "element", "elements")
            )));
        }
        let missing: usize = self.openings_missing.values().sum();
        if missing > 0 {
            notices.push(Notice::warning(if missing == 1 {
                "1 opening of a wall is missing".to_owned()
            } else {
                format!("{missing} openings of the walls are missing")
            }));
        }
        for link in self.links.iter().filter(|l| l.found.is_none()) {
            let why = link.note.as_deref().unwrap_or("not imported");
            notices.push(Notice::warning(format!("link {}: {why}", link.file)));
        }
        notices.extend(self.warnings.iter().map(|w| Notice::warning(w.as_str())));
        // Stable: the notices of one level keep their order.
        notices.sort_by_key(|n| std::cmp::Reverse(n.level));
        notices
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ElementLine, LinkReport};

    #[test]
    fn outcomes_map_onto_the_levels() {
        // (mitcad#1051) What came in with a shape of its own is
        // information, a box a warning, nothing built an error.
        for outcome in [
            Outcome::Features,
            Outcome::Parts,
            Outcome::Mesh,
            Outcome::ReferenceBody,
        ] {
            assert_eq!(outcome.level(), NoticeLevel::Info, "{}", outcome.as_str());
        }
        assert_eq!(Outcome::Box.level(), NoticeLevel::Warning);
        assert_eq!(Outcome::NotBuilt.level(), NoticeLevel::Error);
        for outcome in Outcome::ALL {
            assert_eq!(Outcome::parse(outcome.as_str()), Some(outcome));
        }
        assert_eq!(Outcome::parse(""), None);
    }

    #[test]
    fn a_report_s_notices_count_what_was_left_out_the_worst_first() {
        let line = |outcome: Outcome| ElementLine {
            outcome: outcome.as_str().to_owned(),
            ..ElementLine::default()
        };
        let mut report = Report {
            elements: vec![
                line(Outcome::Features),
                line(Outcome::Mesh),
                line(Outcome::Box),
                line(Outcome::Box),
                line(Outcome::NotBuilt),
            ],
            warnings: vec!["a warning of the import".to_owned()],
            ..Report::default()
        };
        report.fallbacks.insert("wall".to_owned(), 1);
        report
            .openings_missing
            .insert("its wall came in as its box".to_owned(), 2);
        report.links.push(LinkReport {
            file: "site.rvt".to_owned(),
            note: Some("site.rvt was not found: its component is empty".to_owned()),
            ..LinkReport::default()
        });
        assert_eq!(
            report.notices(),
            vec![
                Notice::error("1 element could not be built"),
                Notice::warning("2 elements came in as boxes"),
                Notice::warning(
                    "1 element of the kinds built as features came in as meshes, bodies or boxes \
                     instead"
                ),
                Notice::warning("2 openings of the walls are missing"),
                Notice::warning("link site.rvt: site.rvt was not found: its component is empty"),
                Notice::warning("a warning of the import"),
            ]
        );
        // Everything as asked: no notices.
        let clean = Report {
            elements: vec![line(Outcome::Features), line(Outcome::Parts)],
            ..Report::default()
        };
        assert!(clean.notices().is_empty());
    }
}
