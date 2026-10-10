// SPDX-License-Identifier: MIT
//! A project shared onto a repository's files (mitcad#89, `connect` with
//! `onto_files`): Project Settings' Local → Cloud with a remote that holds
//! files (a README, a licence) but no project and no version in common.
//! The remote is set and fetched, then the project's versions are replayed
//! after the remote's latest one as a sync replays unpublished versions
//! (`remote/sync.rs` with [`SyncOptions::onto_files`]): `.gitignore` and
//! `.gitattributes` on both sides are merged by lines, another path on
//! both sides is a conflict answered with the sync's choices. The branch
//! then follows the remote's default branch. A replay that stops before
//! it changed the project (a conflict without a choice, changes in the
//! folder, a cancel, the network) removes the remote again.

use std::fs;

use crate::remote::{Connected, Control, RemoteCheck, SyncOptions, remote_url};
use crate::{Identity, ProjectRepo, VcsError};

impl ProjectRepo {
    /// Connects the project onto the files of the remote `check` describes
    /// (see the module's documentation).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn connect_onto(
        &self,
        check: RemoteCheck,
        url: &str,
        name: &str,
        author: &Identity,
        push: bool,
        options: &SyncOptions,
        control: Option<&Control>,
    ) -> Result<Connected, VcsError> {
        let added = remote_url(&self.fresh()?, name).is_none();
        let config_path = self.repo.git_dir().join("config");
        let config_before = fs::read(&config_path).ok();
        let undo = || {
            if added {
                let _ = self.remote_remove(name);
            } else if let Some(config) = &config_before {
                let _ = mitcad_model::file::write_atomically(&config_path, config);
            }
        };
        let mut set = match self.remote_set(url, name, author) {
            Ok(set) => set,
            Err(error) => {
                undo();
                return Err(error);
            }
        };
        // The branch follows the remote's default branch.
        if let Some(remote_branch) = check.default_branch.clone()
            && remote_branch != set.branch
        {
            let key = format!("branch.{}.merge", set.branch);
            let value = format!("refs/heads/{remote_branch}");
            if let Err(error) = self.git_ok(
                &format!("Setting the upstream of {}", set.branch),
                &["config", &key, &value],
                None,
            ) {
                undo();
                return Err(error);
            }
            set.upstream = format!("{name}/{remote_branch}");
        }
        let mut connected = Connected {
            check,
            set,
            fetch: None,
            push: None,
            ahead: None,
            behind: None,
            error: None,
            sync: None,
            undone: false,
        };
        let options = SyncOptions {
            fetch: false,
            push,
            onto_files: true,
            ..options.clone()
        };
        let result = self.fetch(control).and_then(|fetched| {
            connected.fetch = Some(fetched);
            self.sync(&options, Some(author), control)
        });
        match result {
            Ok(outcome) => {
                connected.error = outcome.error.clone();
                connected.push = outcome.push.clone();
                if outcome.error.is_some() && outcome.backup.is_none() {
                    undo();
                    connected.undone = true;
                }
                connected.sync = Some(outcome);
            }
            Err(VcsError::Remote(error)) => {
                undo();
                connected.undone = true;
                connected.error = Some(error);
            }
            Err(other) => {
                undo();
                return Err(other);
            }
        }
        if !connected.undone {
            let info = self.remote_info(false)?;
            (connected.ahead, connected.behind) = (info.ahead, info.behind);
        }
        Ok(connected)
    }
}
