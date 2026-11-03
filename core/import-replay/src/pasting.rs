// SPDX-License-Identifier: MIT
//! Bodies the file cut and pasted into another component.
//!
//! A cut and paste moves bodies from one component into another one of the
//! design (an existing one, placed by the file's occurrences), and changes
//! nothing else: the bodies keep their shape and their place in the design.
//! Mitcad's Move to Component with an existing destination does the same:
//! the bodies and the features that made them go into the destination, and
//! the bodies keep their identity (`move_features.rs` of the model). Where
//! the features cannot go there, a body transfer in the destination moves
//! the bodies alone at the item's point of the timeline, as the item did
//! (mitcad#465, `features/body_transfer.rs` of the model). The
//! item has no state of its own to be checked against, so the replay's
//! bodies must be as they were and be the destination's after the move.
//!
//! A cut pasted into the component it came from moves nothing; where the
//! file's history holds a state of its own for it that is not the bodies
//! as they are (it keeps the body and adds a copy of it), its translation
//! is that copy (mitcad#1175), which the usual route checks against the
//! state.

use mitcad_dump::TimelineItem;
use mitcad_model::BodyUid;
use mitcad_model::{Document, Kernel};

use crate::history::bodies_distance;

/// A run of one of the model's JSON commands on a document: the command's
/// result as JSON, or why it failed.
pub type RunCommand<K> = fn(&mut Document<K>, &str) -> Result<String, String>;

/// The model's JSON commands as the import runs them (mitcad#778,
/// [`Options::commands`](crate::Options::commands)): the generic command
/// dispatch stays in the api crate, which the import does not depend on,
/// and the import's caller passes it in for the document's kernel, e.g.
/// `Commands::new::<OcctKernel>(|doc, json| doc.command(json).map_err(|e|
/// e.to_string()))` with the api's `DocumentApi` in scope.
#[derive(Clone)]
pub struct Commands(std::sync::Arc<dyn std::any::Any + Send + Sync>);

impl Commands {
    /// The commands for documents of the kernel `K`.
    pub fn new<K: Kernel + 'static>(run: RunCommand<K>) -> Self {
        Self(std::sync::Arc::new(run))
    }

    /// The run for documents of the kernel `K`; None when they were given
    /// for another kernel.
    pub fn get<K: Kernel + 'static>(&self) -> Option<RunCommand<K>> {
        self.0.downcast_ref::<RunCommand<K>>().copied()
    }
}

impl std::fmt::Debug for Commands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Commands")
    }
}

/// The run of a command the import was given for its kernel. The unit
/// tests of `mitcad-import`, which have the api as a dev-dependency, give
/// the api's commands when they give none (mitcad#785).
fn run_command<K: crate::ImportKernel>(commands: Option<&Commands>) -> Option<RunCommand<K>> {
    commands.and_then(Commands::get::<K>)
}

impl<K: crate::ImportKernel> crate::Importer<'_, K> {
    /// Whether the replay's solids are the state the file's history keeps
    /// for item `index`; true where the history holds no state of its own
    /// for it (or cannot build it): nothing to check against.
    pub(crate) fn state_is_current(&mut self, index: i64) -> bool {
        if !self.oracle.enabled {
            return true;
        }
        let Some(state) = self.oracle.item_state(index) else {
            return true;
        };
        let kernel = self.doc.kernel();
        let Some(expected) = self.oracle.try_sigs(kernel, state) else {
            return true;
        };
        bodies_distance(&expected, &self.current_sigs()).is_some()
    }

    /// A cut and paste as the Move to Component command its translation
    /// gives ([`Importer::cut_paste_command`](crate::ops)). False (nothing
    /// done) when the move is refused or changes the bodies: the item then
    /// takes the usual route.
    pub(crate) fn cut_paste_item(&mut self, at: usize, index: i64, item: &TimelineItem) -> bool {
        let name = crate::item_name(item);
        let not = |why: &str| {
            if crate::tracing() {
                eprintln!("import: {name}: not moved: {why}");
            }
            false
        };
        if !self.pending.is_empty() {
            return not("items before it are pending");
        }
        let candidates = match self.translate(index, item) {
            Ok(c) => c,
            Err(e) => return not(&e),
        };
        let before = self.current_sigs();
        let mut last = "nothing to try".to_owned();
        for candidate in candidates {
            let transfer = |d: &&serde_json::Value| d["type"] == "body_transfer";
            let Some(command) = candidate
                .defs
                .first()
                .filter(|d| d.get("cmd").is_some() || transfer(d))
            else {
                continue;
            };
            let moved: Vec<BodyUid> = command["bodies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|b| b.as_str()?.parse().ok())
                .collect();
            let into = self.component;
            // Nothing to move: the bodies are the destination's already.
            if command["cmd"] == "none" {
                if moved
                    .iter()
                    .any(|b| self.doc.body_component(*b) != Some(into))
                {
                    last = "the bodies are not the destination's".to_owned();
                    continue;
                }
                self.accepted(at, index, &[], &candidate, None);
                return true;
            }
            // The bodies alone, by a body transfer in the destination
            // (mitcad#465): the features that made them stay.
            if transfer(&command) {
                // (The tries before it were taken back: the state is as it was.)
                let world_before = self.world_sigs();
                let depth = self.doc.undo_depth();
                let uid = match self.add_in(command, Some(&name), into) {
                    Ok(uid) => uid,
                    Err(e) => {
                        if crate::tracing() {
                            eprintln!("import: {name}: no body transfer: {e}");
                        }
                        self.undo_to(depth);
                        continue;
                    }
                };
                // Their place in the design, not in their component's
                // coordinates, which the transfer changes.
                if bodies_distance(&world_before, &self.world_sigs()).is_none()
                    || moved
                        .iter()
                        .any(|b| self.doc.body_component(*b) != Some(into))
                {
                    if crate::tracing() {
                        eprintln!(
                            "import: {name}: no body transfer: the bodies are not as they were \
                             in the design, or not the destination's"
                        );
                    }
                    self.undo_to(depth);
                    continue;
                }
                let mut accepted = candidate.clone();
                accepted.note = Some(format!(
                    "moves {} into {} by a body transfer; the features that made {} stay \
                     ({last})",
                    if moved.len() == 1 {
                        "its body".to_owned()
                    } else {
                        format!("its {} bodies", moved.len())
                    },
                    self.doc.assembly().name(into),
                    if moved.len() == 1 { "it" } else { "them" },
                ));
                self.accepted(at, index, &[uid], &accepted, None);
                return true;
            }
            // The model's commands as the caller gave them (mitcad#778).
            let Some(run) = run_command::<K>(self.options.commands.as_ref()) else {
                last = "no commands to run the move with".to_owned();
                continue;
            };
            let depth = self.doc.undo_depth();
            let text = command.to_string();
            // The file's design as it is: no design policy (mitcad#220).
            let result = self.interruptible(|doc| doc.without_policy_checks(|doc| run(doc, &text)));
            let features = match result {
                Ok(done) => serde_json::from_str::<serde_json::Value>(&done)
                    .ok()
                    .and_then(|v| v["components"][0]["features"].as_array().map(Vec::len))
                    .unwrap_or(0),
                Err(e) => {
                    last = e;
                    self.undo_to(depth);
                    continue;
                }
            };
            // Nothing but the bodies' component changed.
            if bodies_distance(&before, &self.current_sigs()).is_none() {
                last = "the bodies changed".to_owned();
                self.undo_to(depth);
                continue;
            }
            if moved
                .iter()
                .any(|b| self.doc.body_component(*b) != Some(into))
            {
                last = "the bodies are not the destination's".to_owned();
                self.undo_to(depth);
                continue;
            }
            let note = format!(
                "moves {} into {} with {features} features that made {}",
                if moved.len() == 1 {
                    "its body".to_owned()
                } else {
                    format!("its {} bodies", moved.len())
                },
                self.doc.assembly().name(into),
                if moved.len() == 1 { "it" } else { "them" },
            );
            let mut accepted = candidate.clone();
            accepted.note = Some(note);
            self.accepted(at, index, &[], &accepted, None);
            return true;
        }
        not(&last)
    }

    /// The bodies' signatures with their centres where the design places
    /// them (their first instance).
    fn world_sigs(&self) -> Vec<crate::history::Sig> {
        let instances = self.doc.instances();
        self.current_bodies()
            .into_iter()
            .map(|(uid, mut sig)| {
                if let Some(i) = instances.iter().find(|i| i.body == uid) {
                    sig.center = i.transform.apply_point(sig.center);
                }
                sig
            })
            .collect()
    }
}
