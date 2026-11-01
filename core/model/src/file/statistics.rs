// SPDX-License-Identifier: MIT
//! The design's statistics (mitcad#1088): when it was created and last
//! saved, by whom and with which MitCad, as the design file's
//! `statistics`. They are no part of the design's definition: they are
//! kept beside the document's state (not undone, not compared, not in a
//! design comparison), written with it, and changed only by a save that
//! is stamped ([`crate::Document::save_file_stamped`], the application's
//! Save) and that changes the file.
//!
//! ```json
//! "statistics": { "created": "2026-11-01T09:41:38Z",
//!   "last_saved": "2026-11-01T10:02:11Z", "last_saved_by": "anna",
//!   "last_saved_with": "MitCad 0.18.0" }
//! ```
//!
//! Times are UTC, `YYYY-MM-DDTHH:MM:SSZ`. Each field is left out while it
//! is not known: a design whose file was saved before MitCad kept
//! statistics has no `created`, and keeps none when it is saved again.

use serde::{Deserialize, Serialize};

/// The design file's `statistics`. Unknown fields are ignored, so that a
/// later MitCad may add some without this one refusing the file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignStatistics {
    /// When the design was first saved with its statistics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// When the file was last saved with a change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_saved: Option<String>,
    /// The operating system's user name of that save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_saved_by: Option<String>,
    /// The MitCad of that save (`MitCad 0.18.0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_saved_with: Option<String>,
}

impl DesignStatistics {
    /// Nothing known: the file has no `statistics`.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// What a stamped save writes: the time, the user and the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveStamp {
    /// UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    pub time: String,
    /// The operating system's user name; empty when not known.
    pub by: String,
    /// `MitCad <version>`.
    pub with: String,
}

impl SaveStamp {
    /// Now, by the user the process runs as, with this MitCad.
    pub fn now() -> Self {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            time: utc_text(seconds),
            by: os_user(),
            with: format!("MitCad {}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// The statistics a document keeps beside its state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct KeptStatistics {
    /// As the file had them, or as the last stamped save wrote them.
    pub(crate) file: DesignStatistics,
    /// The next stamped save sets `created`: a new design (also one an
    /// import made, or an empty design file), not one whose file was
    /// saved before MitCad kept statistics.
    pub(crate) created_at_next_save: bool,
}

impl KeptStatistics {
    /// A new design's: nothing known, created when first saved.
    pub(crate) fn new_design() -> Self {
        Self {
            file: DesignStatistics::default(),
            created_at_next_save: true,
        }
    }

    /// What a design file read gives: its statistics when it has any;
    /// without them, created at the next save only when `new` (see
    /// [`KeptStatistics::created_at_next_save`]).
    pub(crate) fn read(file: Option<DesignStatistics>, new: bool) -> Self {
        match file {
            Some(file) => Self {
                file,
                created_at_next_save: false,
            },
            None => Self {
                file: DesignStatistics::default(),
                created_at_next_save: new,
            },
        }
    }

    /// The statistics after a save stamped `stamp`.
    pub(crate) fn stamped(&self, stamp: &SaveStamp) -> Self {
        let created = self
            .file
            .created
            .clone()
            .or_else(|| self.created_at_next_save.then(|| stamp.time.clone()));
        Self {
            file: DesignStatistics {
                created,
                last_saved: Some(stamp.time.clone()),
                last_saved_by: (!stamp.by.trim().is_empty()).then(|| stamp.by.trim().to_owned()),
                last_saved_with: Some(stamp.with.clone()),
            },
            created_at_next_save: false,
        }
    }
}

/// Seconds since 1970-01-01 as UTC `YYYY-MM-DDTHH:MM:SSZ`.
pub fn utc_text(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let (y, m, d) = crate::drawing::civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

/// The user name the process runs as (`USER`, `USERNAME` on Windows,
/// `LOGNAME`); empty when none is set.
fn os_user() -> String {
    ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|name| name.trim().to_owned())
        .find(|name| !name.is_empty())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(time: &str) -> SaveStamp {
        SaveStamp {
            time: time.to_owned(),
            by: "anna".to_owned(),
            with: "MitCad 1.2.3".to_owned(),
        }
    }

    #[test]
    fn utc_text_is_iso_8601() {
        assert_eq!(utc_text(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc_text(1_793_525_000), "2026-11-01T09:23:20Z");
        assert_eq!(utc_text(951_782_400 + 3_661), "2000-02-29T01:01:01Z");
    }

    #[test]
    fn a_new_design_is_created_at_its_first_save_and_keeps_it() {
        let first = KeptStatistics::new_design().stamped(&stamp("2026-11-01T10:00:00Z"));
        assert_eq!(first.file.created.as_deref(), Some("2026-11-01T10:00:00Z"));
        assert_eq!(
            first.file.last_saved.as_deref(),
            Some("2026-11-01T10:00:00Z")
        );
        assert_eq!(first.file.last_saved_by.as_deref(), Some("anna"));
        assert_eq!(first.file.last_saved_with.as_deref(), Some("MitCad 1.2.3"));
        let second = first.stamped(&stamp("2026-11-02T08:00:00Z"));
        assert_eq!(second.file.created.as_deref(), Some("2026-11-01T10:00:00Z"));
        assert_eq!(
            second.file.last_saved.as_deref(),
            Some("2026-11-02T08:00:00Z")
        );
    }

    #[test]
    fn a_file_saved_before_statistics_stays_created_unknown() {
        let old = KeptStatistics::read(None, false).stamped(&stamp("2026-11-01T10:00:00Z"));
        assert_eq!(old.file.created, None);
        assert_eq!(old.file.last_saved.as_deref(), Some("2026-11-01T10:00:00Z"));
        // Read back, still unknown after the next save.
        let again =
            KeptStatistics::read(Some(old.file), false).stamped(&stamp("2026-11-03T10:00:00Z"));
        assert_eq!(again.file.created, None);
        // No user name: none written.
        let nobody = SaveStamp {
            by: " ".to_owned(),
            ..stamp("2026-11-01T10:00:00Z")
        };
        assert_eq!(
            KeptStatistics::new_design()
                .stamped(&nobody)
                .file
                .last_saved_by,
            None
        );
    }
}
