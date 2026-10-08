// SPDX-License-Identifier: MIT
//! Remote repositories for C++ (P12 remote, `mitcad_vcs::remote`): the
//! remote commands of a [`Project`] with a [`SyncControl`] for their
//! progress and cancellation, a project opened from a remote, and the git
//! program's version.
//!
//! The network work takes seconds to minutes, so the application runs it
//! on a thread of its own with a `Project` of its own (a project is used by
//! one thread) and shows the control's progress; Cancel ends git.

use std::path::Path;

use mitcad_vcs::VcsError;
use mitcad_vcs::api;
use mitcad_vcs::remote::Control;

use crate::ffi::SyncProgress;
use crate::vcs::Project;

/// The progress and cancel request of a remote operation. It is `Sync`
/// (an atomic and a mutex), so both threads may use it through
/// `&SyncControl` at once. A cancelled control stays cancelled.
pub struct SyncControl(Control);

pub fn new_sync_control() -> Box<SyncControl> {
    Box::new(SyncControl(Control::new()))
}

impl SyncControl {
    /// The control for `mitcad_vcs` (the libraries' commands).
    pub(crate) fn control(&self) -> &Control {
        &self.0
    }

    pub fn cancel(&self) {
        self.0.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    pub fn progress(&self) -> SyncProgress {
        let progress = self.0.progress();
        SyncProgress {
            text: progress.text,
            percent: progress.percent.map_or(-1, i32::from),
            cancelled: self.0.is_cancelled(),
        }
    }
}

impl Project {
    pub fn command_with(&self, json: &str, control: &SyncControl) -> Result<String, VcsError> {
        self.0.command_with(json, &control.0)
    }
}

pub fn clone_project(
    url: &str,
    dir: &str,
    control: &SyncControl,
    text: bool,
) -> Result<String, VcsError> {
    api::clone_project(url, Path::new(dir), Some(&control.0), text)
}

pub fn git_info(text: bool) -> String {
    api::git_info(text)
}

pub fn git_info_at(program: &str) -> String {
    api::git_info_at(program)
}

pub fn describe_remote(command: &str, answer: &str) -> Result<String, VcsError> {
    api::describe_answer(command, answer)
}
