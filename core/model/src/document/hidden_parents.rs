// SPDX-License-Identifier: MIT
//! Hidden parents of new content (mitcad#1995, GitHub mitcad/Mitcad#430):
//! a component placed only under hidden occurrences cannot be seen, its
//! own light bulb and its occurrence's on or not. What a command adds there
//! (a new component, an inserted design, an import, a new body) would not
//! show, with no word why; the commands that add content show those
//! occurrences again in their own undo step (`model-api`'s
//! `run_checked_action`, which merges it into the command's step).
//!
//! A component is in sight when it is the root or one of its occurrences
//! has every occurrence on its path from the root shown. Out of sight, the
//! hidden occurrences on the path of its first placement hide it: those are
//! shown, the outermost first. One that follows a Show When condition
//! (mitcad#800) keeps it: setting its value by hand would drop the
//! condition, which the user set on purpose.

use std::collections::BTreeSet;

use super::{Document, ModelError};
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;

impl<K: Kernel> Document<K> {
    /// Whether a component can be seen: the root, or one of its placements
    /// with every occurrence on its path from the root shown.
    pub fn component_in_sight(&self, component: ComponentUid) -> bool {
        let a = self.assembly();
        a.paths_to(component).iter().any(|path| {
            path.iter()
                .all(|o| a.occurrence(*o).is_none_or(|occurrence| occurrence.visible))
        })
    }

    /// The hidden occurrences that keep a component out of sight, the
    /// outermost first, its own occurrence last when it is hidden itself:
    /// those on the path of its first placement. None when it is in sight.
    pub fn hiding_occurrences(&self, component: ComponentUid) -> Vec<OccurrenceUid> {
        if self.component_in_sight(component) {
            return Vec::new();
        }
        let a = self.assembly();
        a.paths_to(component)
            .first()
            .map(|path| {
                path.iter()
                    .copied()
                    .filter(|o| {
                        a.occurrence(*o)
                            .is_some_and(|occurrence| !occurrence.visible)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Shows the occurrences of `existing` that keep the components out of
    /// sight, in one undo step (`Show Fairlead:1`), and returns them; none
    /// (and no step) when every component is in sight. Occurrences that
    /// follow a Show When condition stay as they are, and so do those not
    /// in `existing`: what the command itself made hidden (an import's
    /// hidden parts) is its own.
    pub fn show_hiding_occurrences(
        &mut self,
        components: &[ComponentUid],
        existing: &BTreeSet<OccurrenceUid>,
    ) -> Result<Vec<OccurrenceUid>, ModelError> {
        let mut shown: Vec<OccurrenceUid> = Vec::new();
        for component in components {
            for o in self.hiding_occurrences(*component) {
                let bound = self.state.bindings.occurrence_visible_when.contains_key(&o);
                if !bound && existing.contains(&o) && !shown.contains(&o) {
                    shown.push(o);
                }
            }
        }
        if shown.is_empty() {
            return Ok(shown);
        }
        let list = shown.clone();
        self.apply_with(move |state| {
            let names: Vec<String> = list
                .iter()
                .map(|o| state.assembly.occurrence_name(*o))
                .collect();
            for o in &list {
                if let Some(occurrence) = state.assembly.occurrence_mut(*o) {
                    occurrence.visible = true;
                }
            }
            Ok((format!("Show {}", names.join(", ")), (), false))
        })?;
        Ok(shown)
    }

    /// The bodies at the marker, by id: what a command adds is told by
    /// the ones it did not have before (mitcad#1995).
    pub fn body_uids(&self) -> BTreeSet<BodyUid> {
        self.result.owners.keys().copied().collect()
    }
}
