// SPDX-License-Identifier: MIT
//! The outcomes of the imports that build their items in `mitcad-ffi`,
//! without an import crate of their own (mitcad#1051): an `.ipt` part's
//! bodies ([`BodyOutcome`]), an `.iam` assembly's referenced files
//! ([`FileOutcome`]) and a `.dwg` drawing's entities ([`EntityOutcome`]).
//! Each maps onto the notices' levels as the other imports' outcomes do
//! (`docs/ui-conventions.md` "Messages and severity"): what came in as
//! asked is information; what came in partly, as a fallback or not at all
//! is a warning; an item of which nothing could be built is an error. The
//! import's notices say how many items had each outcome that is a warning
//! or an error, the worst first, then its warnings; the outcomes keep
//! their own words in the import's report.

use mitcad_model::notice::{self, Notice, NoticeLevel};

/// Notices of the outcomes that are warnings or errors, then one warning
/// per text of `warnings`; the worst first.
fn with_warnings<O: Copy>(
    counts: impl IntoIterator<Item = (O, usize)>,
    level: impl Fn(O) -> NoticeLevel,
    describe: impl Fn(O, usize) -> String,
    warnings: &[String],
) -> Vec<Notice> {
    let mut notices = notice::outcome_notices(counts, level, describe);
    notices.extend(warnings.iter().map(|w| Notice::warning(w.as_str())));
    // Stable: the notices of one level keep their order.
    notices.sort_by_key(|n| std::cmp::Reverse(n.level));
    notices
}

/// How an `.ipt` part's body came in: a stored body built, a replayed
/// one, or a mesh feature's mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BodyOutcome {
    /// Built, and valid (or a sheet or a mesh).
    Imported,
    /// A solid built that is not valid.
    NotValid,
    /// It could not be built.
    NotBuilt,
}

impl BodyOutcome {
    pub fn level(self) -> NoticeLevel {
        match self {
            BodyOutcome::Imported => NoticeLevel::Info,
            BodyOutcome::NotValid => NoticeLevel::Warning,
            BodyOutcome::NotBuilt => NoticeLevel::Error,
        }
    }

    fn describe(self, n: usize) -> String {
        match self {
            BodyOutcome::Imported => format!("{} imported", notice::count(n, "body", "bodies")),
            BodyOutcome::NotValid => {
                format!("{} not valid", notice::count(n, "solid is", "solids are"))
            }
            BodyOutcome::NotBuilt => {
                format!("{} could not be built", notice::count(n, "body", "bodies"))
            }
        }
    }

    /// The part's notices: its bodies' outcomes, then `warnings`.
    pub fn notices(
        counts: impl IntoIterator<Item = (BodyOutcome, usize)>,
        warnings: &[String],
    ) -> Vec<Notice> {
        with_warnings(counts, BodyOutcome::level, BodyOutcome::describe, warnings)
    }
}

/// How an `.iam` assembly's referenced file came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileOutcome {
    /// A part replayed with its design history.
    Replayed,
    /// A part as its stored bodies: asked so, or it has no history.
    Bodies,
    /// A sub-assembly, its occurrences placed.
    Assembly,
    /// A part whose replay failed or hung: its stored bodies instead.
    Fallback,
    /// Not found: its component is empty.
    Missing,
    /// A sub-assembly that contains itself: left out.
    Cycle,
    /// A part that could not be imported: no component.
    Failed,
}

impl FileOutcome {
    pub fn level(self) -> NoticeLevel {
        match self {
            FileOutcome::Replayed | FileOutcome::Bodies | FileOutcome::Assembly => {
                NoticeLevel::Info
            }
            FileOutcome::Fallback | FileOutcome::Missing | FileOutcome::Cycle => {
                NoticeLevel::Warning
            }
            FileOutcome::Failed => NoticeLevel::Error,
        }
    }

    fn describe(self, n: usize) -> String {
        let one = n == 1;
        match self {
            FileOutcome::Replayed => format!("{} replayed", notice::count(n, "part", "parts")),
            FileOutcome::Bodies => {
                format!("{} as stored bodies", notice::count(n, "part", "parts"))
            }
            FileOutcome::Assembly => {
                format!(
                    "{} placed",
                    notice::count(n, "sub-assembly", "sub-assemblies")
                )
            }
            FileOutcome::Fallback if one => {
                "1 part came in as its stored bodies: its replay failed".to_owned()
            }
            FileOutcome::Fallback => {
                format!("{n} parts came in as their stored bodies: their replay failed")
            }
            FileOutcome::Missing if one => {
                "1 referenced file was not found: its component is empty".to_owned()
            }
            FileOutcome::Missing => {
                format!("{n} referenced files were not found: their components are empty")
            }
            FileOutcome::Cycle if one => "1 sub-assembly contains itself: left out".to_owned(),
            FileOutcome::Cycle => format!("{n} sub-assemblies contain themselves: left out"),
            FileOutcome::Failed => {
                format!(
                    "{} could not be imported",
                    notice::count(n, "part", "parts")
                )
            }
        }
    }

    /// The assembly's notices: its files' outcomes, then `warnings`.
    pub fn notices(
        counts: impl IntoIterator<Item = (FileOutcome, usize)>,
        warnings: &[String],
    ) -> Vec<Notice> {
        with_warnings(counts, FileOutcome::level, FileOutcome::describe, warnings)
    }
}

/// How a `.dwg` drawing's entity came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityOutcome {
    /// A body, a mesh or an insert: taken.
    Taken,
    /// A 2D entity: left out by design (the import takes the 3D content).
    Drawing,
    /// A body built that is not valid.
    NotValid,
    /// A 3D entity left out.
    LeftOut,
    /// A body that could not be built.
    NotBuilt,
}

impl EntityOutcome {
    pub fn level(self) -> NoticeLevel {
        match self {
            EntityOutcome::Taken | EntityOutcome::Drawing => NoticeLevel::Info,
            EntityOutcome::NotValid | EntityOutcome::LeftOut => NoticeLevel::Warning,
            EntityOutcome::NotBuilt => NoticeLevel::Error,
        }
    }

    fn describe(self, n: usize) -> String {
        match self {
            EntityOutcome::Taken => format!("{} taken", notice::count(n, "entity", "entities")),
            EntityOutcome::Drawing => {
                format!("{} left out", notice::count(n, "2D entity", "2D entities"))
            }
            EntityOutcome::NotValid => {
                format!("{} not valid", notice::count(n, "body is", "bodies are"))
            }
            EntityOutcome::LeftOut => {
                format!("{} left out", notice::count(n, "3D entity", "3D entities"))
            }
            EntityOutcome::NotBuilt => {
                format!("{} could not be built", notice::count(n, "body", "bodies"))
            }
        }
    }

    /// The drawing's notices: its entities' outcomes, then `warnings`.
    pub fn notices(
        counts: impl IntoIterator<Item = (EntityOutcome, usize)>,
        warnings: &[String],
    ) -> Vec<Notice> {
        with_warnings(
            counts,
            EntityOutcome::level,
            EntityOutcome::describe,
            warnings,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_s_bodies_map_onto_the_levels() {
        // (mitcad#1051) A body built is information, a solid that is not
        // valid a warning, a body that could not be built an error.
        assert_eq!(BodyOutcome::Imported.level(), NoticeLevel::Info);
        assert_eq!(BodyOutcome::NotValid.level(), NoticeLevel::Warning);
        assert_eq!(BodyOutcome::NotBuilt.level(), NoticeLevel::Error);
        let notices = BodyOutcome::notices(
            [
                (BodyOutcome::Imported, 4),
                (BodyOutcome::NotValid, 1),
                (BodyOutcome::NotBuilt, 2),
            ],
            &["the part has no bodies".to_owned()],
        );
        assert_eq!(
            notices,
            vec![
                Notice::error("2 bodies could not be built"),
                Notice::warning("1 solid is not valid"),
                Notice::warning("the part has no bodies"),
            ]
        );
        assert!(BodyOutcome::notices([(BodyOutcome::Imported, 3)], &[]).is_empty());
    }

    #[test]
    fn an_assembly_s_files_map_onto_the_levels() {
        // (mitcad#1051) Parts replayed or as their stored bodies and
        // sub-assemblies placed are information; a replay that fell back,
        // a file not found and a cycle warnings; a part that could not be
        // imported an error.
        for outcome in [
            FileOutcome::Replayed,
            FileOutcome::Bodies,
            FileOutcome::Assembly,
        ] {
            assert_eq!(outcome.level(), NoticeLevel::Info, "{outcome:?}");
        }
        for outcome in [
            FileOutcome::Fallback,
            FileOutcome::Missing,
            FileOutcome::Cycle,
        ] {
            assert_eq!(outcome.level(), NoticeLevel::Warning, "{outcome:?}");
        }
        assert_eq!(FileOutcome::Failed.level(), NoticeLevel::Error);
        let notices = FileOutcome::notices(
            [
                (FileOutcome::Replayed, 3),
                (FileOutcome::Fallback, 1),
                (FileOutcome::Missing, 2),
                (FileOutcome::Failed, 1),
            ],
            &[],
        );
        assert_eq!(
            notices,
            vec![
                Notice::error("1 part could not be imported"),
                Notice::warning("1 part came in as its stored bodies: its replay failed"),
                Notice::warning("2 referenced files were not found: their components are empty"),
            ]
        );
    }

    #[test]
    fn a_drawing_s_entities_map_onto_the_levels() {
        // (mitcad#1051) What was taken and the 2D entities left out by
        // design are information; a body that is not valid and a 3D entity
        // left out warnings; a body that could not be built an error.
        assert_eq!(EntityOutcome::Taken.level(), NoticeLevel::Info);
        assert_eq!(EntityOutcome::Drawing.level(), NoticeLevel::Info);
        assert_eq!(EntityOutcome::NotValid.level(), NoticeLevel::Warning);
        assert_eq!(EntityOutcome::LeftOut.level(), NoticeLevel::Warning);
        assert_eq!(EntityOutcome::NotBuilt.level(), NoticeLevel::Error);
        let notices = EntityOutcome::notices(
            [
                (EntityOutcome::Taken, 6),
                (EntityOutcome::Drawing, 1),
                (EntityOutcome::LeftOut, 1),
                (EntityOutcome::NotBuilt, 1),
            ],
            &["a blend is the file's approximation".to_owned()],
        );
        assert_eq!(
            notices,
            vec![
                Notice::error("1 body could not be built"),
                Notice::warning("1 3D entity left out"),
                Notice::warning("a blend is the file's approximation"),
            ]
        );
        // A drawing of which everything 3D came in, with its lines left
        // out: nothing to say.
        let clean = [(EntityOutcome::Taken, 6), (EntityOutcome::Drawing, 1)];
        assert!(EntityOutcome::notices(clean, &[]).is_empty());
    }
}
