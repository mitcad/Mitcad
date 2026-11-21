// SPDX-License-Identifier: MIT
//! The order of a component's occurrences among their siblings
//! (mitcad#2104): the browser lists a parent's occurrences in the order
//! `Assembly::occurrences` keeps them, which the design file keeps too, and
//! the queries list them so. Reordering changes only that order: no
//! placement, joint or timeline feature, and nothing is recomputed.

use super::{Document, ModelError, invalid};
use crate::ids::OccurrenceUid;
use crate::kernel::Kernel;

impl<K: Kernel> Document<K> {
    /// Moves occurrences of one parent to `index` among the occurrences
    /// with that parent (counted without them), in the order given, so
    /// that the first one is at `index` and the others follow it: one undo
    /// step (`Reorder Arm:2`, `Reorder 3 Components`) that recomputes
    /// nothing, none when the order does not change. Refused: an index
    /// past the end, occurrences of different parents, and a parent linked
    /// from another file (its order comes from that file).
    pub fn reorder_occurrences(
        &mut self,
        moved: &[OccurrenceUid],
        index: usize,
    ) -> Result<(), ModelError> {
        let Some(first) = moved.first() else {
            return Ok(());
        };
        let parent = self.state.require_occurrence(*first)?.parent;
        for uid in moved {
            if self.state.require_occurrence(*uid)?.parent != parent {
                return Err(invalid(format!(
                    "{} and {} are not placed in the same component",
                    self.state.assembly.occurrence_name(*first),
                    self.state.assembly.occurrence_name(*uid)
                )));
            }
        }
        self.state.editable(parent)?;
        let mut moving: Vec<OccurrenceUid> = Vec::new();
        for uid in moved {
            if !moving.contains(uid) {
                moving.push(*uid);
            }
        }
        // The siblings' places in the assembly's list, and their order.
        let a = &self.state.assembly;
        let places: Vec<usize> = (0..a.occurrences.len())
            .filter(|i| a.occurrences[*i].parent == parent)
            .collect();
        let siblings: Vec<OccurrenceUid> = places.iter().map(|i| a.occurrences[*i].uid).collect();
        let mut order: Vec<OccurrenceUid> = siblings
            .iter()
            .copied()
            .filter(|uid| !moving.contains(uid))
            .collect();
        if index > order.len() {
            return Err(invalid(format!(
                "index {index} is out of range: {} has {} components, so the index is from 0 to {}",
                a.name(parent),
                siblings.len(),
                order.len()
            )));
        }
        order.splice(index..index, moving.iter().copied());
        if order == siblings {
            return Ok(());
        }
        let label = match moving.as_slice() {
            [one] => format!("Reorder {}", a.occurrence_name(*one)),
            several => format!("Reorder {} Components", several.len()),
        };
        self.apply_with(|state| {
            // The siblings take their new order in the places they had;
            // every other occurrence stays where it is.
            let taken: Vec<_> = places
                .iter()
                .map(|i| state.assembly.occurrences[*i].clone())
                .collect();
            for (place, uid) in places.iter().zip(&order) {
                let occurrence = taken
                    .iter()
                    .find(|o| o.uid == *uid)
                    .cloned()
                    .expect("a sibling");
                state.assembly.occurrences[*place] = occurrence;
            }
            Ok((label, (), false))
        })
    }

    /// An occurrence's place among the occurrences of its parent
    /// (mitcad#2104).
    pub fn occurrence_index(&self, uid: OccurrenceUid) -> Option<usize> {
        let a = &self.state.assembly;
        let parent = a.occurrence(uid)?.parent;
        a.children(parent).position(|o| o.uid == uid)
    }
}
