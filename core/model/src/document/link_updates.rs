// SPDX-License-Identifier: MIT
//! Linked designs read again (mitcad#1416).
//!
//! `update_links` reads every linked design's file again when a design
//! opens, without an undo step. While the design is open, the `links`
//! query says which linked files changed since the design took their
//! bodies (the application's notice "<file> changed"), and `update_link`
//! reads them again at once: one component's file (Update Link) or every
//! linked design's (Update All Links), as one undo step. All of them read a
//! file only when it is in the design's project (mitcad#929), and a file
//! that cannot be read or taken leaves its component as it was, with a
//! message.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::components::{digest, followed_link};
use super::library_parts::LinkUpdate;
use super::{DocState, Document, ModelError, invalid};
use crate::assembly::ExternalLink;
use crate::features::FeatureDef;
use crate::ids::ComponentUid;
use crate::kernel::Kernel;

/// A linked design as the `links` query reports it (mitcad#1416).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkState {
    pub component: ComponentUid,
    pub name: String,
    /// The link's path as the design saves it (relative to its folder).
    pub path: String,
    /// The file the link is followed to; None when it is not followed (a
    /// file outside the design's project, or no folder to follow it from).
    pub file: Option<PathBuf>,
    /// Linked with options (a derived part follows the design, mitcad#801).
    pub options: bool,
    /// The file is not the text the component last took its bodies from.
    pub changed: bool,
    /// Why the file is not followed or cannot be read.
    pub problem: Option<String>,
}

/// What [`Document::update_link`] did: the components that took their
/// file again, and a line for each component read (taken, up to date, or
/// why not).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LinksUpdated {
    pub updated: Vec<ComponentUid>,
    pub messages: Vec<String>,
}

impl<K: Kernel> Document<K> {
    /// The linked designs (library parts are read from their libraries and
    /// are not listed), in the order of the components, each with the file
    /// it is followed to and whether that file changed. `base` is the
    /// design file's folder; left out, the folder the design was read from.
    pub fn link_states(&self, base: Option<&Path>) -> Vec<LinkState> {
        let base = self.link_base(base);
        self.state
            .assembly
            .components
            .iter()
            .filter(|c| c.library.is_none())
            .filter_map(|c| {
                let link = c.link.as_ref()?;
                let options = self.state.options_part(c.uid).is_some();
                if options && c.build.is_some() {
                    return None;
                }
                let (file, changed, problem) = match followed_link(&link.path, base.as_deref()) {
                    Err(why) => (None, false, Some(why.problem())),
                    Ok(file) => match std::fs::read_to_string(&file) {
                        Ok(text) => (Some(file), digest(&text) != link.digest, None),
                        Err(e) => (
                            Some(file),
                            false,
                            Some(format!("cannot read {} ({e})", link.path)),
                        ),
                    },
                };
                Some(LinkState {
                    component: c.uid,
                    name: c.name.clone(),
                    path: link.path.clone(),
                    file,
                    options,
                    changed,
                    problem,
                })
            })
            .collect()
    }

    /// Reads the files of linked designs again now (mitcad#1416): that of
    /// `component` (Update Link), or every linked design's when it is None
    /// (Update All Links). The components whose file changed take its
    /// bodies (a design linked with options its design) as one undo step,
    /// `Update Link <name>` or `Update All Links`; when no file changed
    /// nothing changes and there is no undo step. A file that is not
    /// followed, cannot be read or cannot be taken leaves its component as
    /// it was, with a message. `base` is the design file's folder; left
    /// out, the folder the design was read from.
    pub fn update_link(
        &mut self,
        component: Option<ComponentUid>,
        base: Option<&Path>,
    ) -> Result<LinksUpdated, ModelError> {
        if let Some(uid) = component {
            let a = &self.state.assembly;
            if !a.exists(uid) {
                return Err(invalid(format!("component {uid} does not exist")));
            }
            let linked = a
                .component(uid)
                .is_some_and(|c| c.link.is_some() && c.library.is_none());
            if !linked {
                return Err(invalid(format!(
                    "{} is not a linked design; only a design linked from a file is read again",
                    a.name(uid)
                )));
            }
        }
        let base = self.link_base(base);
        let only = |c: ComponentUid| component.is_none_or(|uid| uid == c);
        let mut messages = Vec::new();
        let sources = self.options_file_updates(base.as_deref(), &only, &mut messages);
        let updates = self.file_link_updates(base.as_deref(), &only, &mut messages)?;
        let updated: Vec<ComponentUid> = sources
            .iter()
            .map(|s| s.0)
            .chain(updates.iter().map(|u| u.0))
            .collect();
        if updated.is_empty() {
            if messages.is_empty() {
                messages.push(match component {
                    Some(uid) => {
                        let path = self
                            .state
                            .assembly
                            .component(uid)
                            .and_then(|c| c.link.as_ref())
                            .map(|l| l.path.clone())
                            .unwrap_or_default();
                        format!(
                            "{} is up to date with {path}",
                            self.state.assembly.name(uid)
                        )
                    }
                    None => "Every linked design is up to date".to_owned(),
                });
            }
            return Ok(LinksUpdated { updated, messages });
        }
        let label = match component {
            Some(uid) => format!("Update Link {}", self.state.assembly.name(uid)),
            None => "Update All Links".to_owned(),
        };
        let before = self.state.clone();
        self.apply(|state| {
            super::component_options::apply_file_updates(state, sources)?;
            apply_link_updates(state, updates)?;
            // A file read again leaves its old source unused (mitcad#801).
            state.prune_library_sources(&before);
            Ok((label, ()))
        })?;
        Ok(LinksUpdated { updated, messages })
    }

    /// The folder relative links are followed from: `base`, else the folder
    /// the design was read from.
    fn link_base(&self, base: Option<&Path>) -> Option<PathBuf> {
        base.map(Path::to_path_buf)
            .or_else(|| self.source_directory())
    }

    /// The linked designs (not library parts, not designs linked with
    /// options) among `only` whose files changed, read again (part of
    /// `update_links` and `update_link`): each component's new bodies and
    /// mount frames with the file's digest, and a line per file that
    /// changed or could not be read or taken. Only a file in the design's
    /// project is read (mitcad#929).
    pub(super) fn file_link_updates(
        &self,
        base: Option<&Path>,
        only: &dyn Fn(ComponentUid) -> bool,
        messages: &mut Vec<String>,
    ) -> Result<Vec<LinkUpdate>, ModelError> {
        let links: Vec<(ComponentUid, ExternalLink)> = self
            .state
            .assembly
            .components
            .iter()
            .filter(|c| only(c.uid))
            // Library parts are read from their libraries (mitcad#64).
            .filter(|c| c.library.is_none())
            // A design linked with options is a source (mitcad#801).
            .filter(|c| self.state.options_part(c.uid).is_none())
            .filter_map(|c| Some((c.uid, c.link.clone()?)))
            .collect();
        let mut updates = Vec::new();
        for (component, link) in links {
            let name = self.state.assembly.name(component);
            let resolved = match followed_link(&link.path, base) {
                Ok(resolved) => resolved,
                Err(why) => {
                    messages.push(why.kept_bodies(&name));
                    continue;
                }
            };
            let text = match std::fs::read_to_string(&resolved) {
                Ok(text) => text,
                Err(e) => {
                    messages.push(format!(
                        "{name}: cannot read {} ({e}); it keeps the bodies saved with the project",
                        link.path
                    ));
                    continue;
                }
            };
            let new_digest = digest(&text);
            if new_digest == link.digest {
                continue;
            }
            let parts = match crate::file::load_state_at(&text, &resolved) {
                // A link shown by its envelope takes the file's envelope
                // again (mitcad#1271).
                Ok((source, _)) => match link.envelope {
                    Some(detail) => self
                        .design_envelope(&source, &link.path, Some(detail))
                        .map(|(bodies, mounts, _)| (bodies, mounts)),
                    None => self.design_parts(&source, &link.path),
                },
                Err(e) => Err(invalid(e.to_string())),
            };
            match parts {
                Ok((bodies, mounts)) => {
                    messages.push(format!("{name} is updated from {}", link.path));
                    updates.push((component, link, new_digest, bodies, mounts));
                }
                // Cancelled (P7): the document stays as it was.
                Err(ModelError::Cancelled) => return Err(ModelError::Cancelled),
                Err(e) => messages.push(format!(
                    "{name}: {e}; it keeps the bodies saved with the project"
                )),
            }
        }
        Ok(updates)
    }
}

/// Puts linked designs read again into a state (`update_links`,
/// `update_link`): each component's base feature takes the new bodies, its
/// mount frames follow the file (mitcad#525) and its link the new digest.
pub(super) fn apply_link_updates(
    state: &mut DocState,
    updates: Vec<LinkUpdate>,
) -> Result<(), ModelError> {
    for (component, link, digest, bodies, mounts) in updates {
        super::mounts::update_linked_mounts(state, component, &mounts)?;
        let position = state.require(link.feature)?;
        let mut entry = (*state.features[position]).clone();
        if let FeatureDef::Base(base) = &mut entry.def {
            base.bodies = bodies;
        }
        state.features[position] = Arc::new(entry);
        if let Some(l) = state
            .assembly
            .component_mut(component)
            .and_then(|c| c.link.as_mut())
        {
            l.digest = digest;
        }
    }
    Ok(())
}
