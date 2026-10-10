// SPDX-License-Identifier: MIT
//! Local and Cloud projects for C++ (mitcad#89, `mitcad_vcs::projects`):
//! the commands without a project (what a folder is, what a remote holds,
//! a project made or opened from a remote, a shared folder, SSH host keys)
//! with a [`SyncControl`] for their progress and cancellation, as the
//! library commands take one. The commands of a project (its settings,
//! its author) are the version history's `Project` commands.

use mitcad_vcs::{VcsError, api};

use crate::remote::SyncControl;

/// Runs a command without a project (commands.md, "Projects"); the answer
/// in JSON.
pub fn projects_command(json: &str, control: &SyncControl) -> Result<String, VcsError> {
    api::projects_command(json, Some(control.control()))
}

/// The same answered in text for people (mitcad-cli); a failure in the
/// answer's error is an error.
pub fn projects_command_text(json: &str, control: &SyncControl) -> Result<String, VcsError> {
    api::projects_command_text(json, Some(control.control()))
}
