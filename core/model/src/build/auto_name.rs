// SPDX-License-Identifier: MIT
//! A BUILD part's automatic name follows its designation (mitcad#2030;
//! `commands.md`, "BUILD: a part's automatic name"). A placed part is
//! named after its designation (`Profile 30x30 B8 L=300`, made unique with
//! ` (1)`, ` (2)`, ...); when a command changes what the designation is
//! made of (a typed length, a bound length that follows its face, an along
//! edge member's edge or offsets, a length formula's parameters, a row or
//! an option), the name takes the new designation in that command's undo
//! step. A name the user gave stays.

use std::collections::BTreeMap;

use super::part_info::{designation, part_values};
use super::{LibrarySourceDef, SourceId};
use crate::document::DocState;
use crate::features::FeatureDef;
use crate::ids::{ComponentUid, FeatureUid};

/// Whether `name` is `designation`, or `designation (k)` as
/// `DocState::free_component_name` makes a name unique.
pub fn is_automatic_name(name: &str, designation: &str) -> bool {
    if designation.is_empty() {
        return false;
    }
    if name == designation {
        return true;
    }
    name.strip_prefix(designation)
        .and_then(|rest| rest.strip_prefix(" ("))
        .and_then(|rest| rest.strip_suffix(')'))
        .is_some_and(|k| !k.is_empty() && k.bytes().all(|b| b.is_ascii_digit()))
}

/// What a BUILD part's designation is made of in a state: its source, its
/// row and its parameters' values.
struct Made<'a> {
    source: SourceId,
    def: &'a LibrarySourceDef,
    config: Option<&'a str>,
    values: BTreeMap<String, f64>,
}

impl Made<'_> {
    fn designation(&self) -> String {
        designation(self.def, self.config, &self.values)
    }

    fn same(&self, other: &Made<'_>) -> bool {
        self.source == other.source && self.config == other.config && self.values == other.values
    }
}

/// What the derived part `part` is made of in `state`.
fn made_of(state: &DocState, part: FeatureUid) -> Option<Made<'_>> {
    let FeatureDef::DerivedPart(def) = &state.entry(part)?.def else {
        return None;
    };
    Some(Made {
        source: def.source,
        def: state.library_sources.get(def.source)?,
        config: def.config.as_deref(),
        values: part_values(def, &state.parameters),
    })
}

impl DocState {
    /// Renames the BUILD parts whose designation the step from `before`
    /// changed, while their names are automatic: the designation they came
    /// from (the record's `named_after`; in a design saved without it, the
    /// designation before the step), or that with the suffix a free name
    /// adds. Each takes the new designation, made unique among the
    /// components, and the record keeps it. Only parts whose source, row
    /// or values changed are configured; a part the step made, or renamed
    /// itself (Swap Fastener), keeps the name the step gave it. Called by
    /// the document's `apply` after the step's recompute (a bound length's
    /// value is its measured one then), before its undo step is kept.
    pub(crate) fn follow_build_designations(&mut self, before: &DocState) {
        let candidates: Vec<(ComponentUid, FeatureUid, String, Option<String>)> = self
            .assembly
            .components
            .iter()
            .filter_map(|c| {
                let record = c.build.as_ref()?;
                Some((
                    c.uid,
                    record.part?,
                    c.name.clone(),
                    record.named_after.clone(),
                ))
            })
            .collect();
        // The parts to rename and their new designations; whether the name
        // changes (one already named after it keeps its name).
        let mut follows: Vec<(ComponentUid, String, bool)> = Vec::new();
        for (component, part, name, named_after) in candidates {
            let was = before.assembly.component(component);
            if was.is_none_or(|c| c.build.is_none() || c.name != name) {
                continue;
            }
            let (new, from) = {
                let (Some(now), Some(then)) = (made_of(self, part), made_of(before, part)) else {
                    continue;
                };
                if now.same(&then) {
                    continue;
                }
                let from = named_after.clone().unwrap_or_else(|| then.designation());
                (now.designation(), from)
            };
            if new.is_empty() || !is_automatic_name(&name, &from) {
                continue;
            }
            let rename = !is_automatic_name(&name, &new);
            follows.push((component, new, rename));
        }
        // The old names go first, so that two parts that trade lengths in
        // one step each take the plain designation the other leaves.
        for (component, _, rename) in &follows {
            if *rename && let Some(c) = self.assembly.component_mut(*component) {
                c.name.clear();
            }
        }
        for (component, new, rename) in follows {
            let name = rename.then(|| self.free_component_name(&new));
            let Some(c) = self.assembly.component_mut(component) else {
                continue;
            };
            if let Some(name) = name {
                c.name = name;
            }
            if let Some(record) = c.build.as_mut() {
                record.named_after = Some(new);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_automatic_name;

    #[test]
    fn a_name_is_automatic_as_the_designation_or_with_a_free_names_suffix() {
        let d = "Profile 30x30 B8 L=300";
        assert!(is_automatic_name(d, d));
        assert!(is_automatic_name("Profile 30x30 B8 L=300 (1)", d));
        assert!(is_automatic_name("Profile 30x30 B8 L=300 (12)", d));
        assert!(!is_automatic_name("Profile 30x30 B8 L=300 ()", d));
        assert!(!is_automatic_name("Profile 30x30 B8 L=300 (a)", d));
        assert!(!is_automatic_name("Profile 30x30 B8 L=3000", d));
        assert!(!is_automatic_name("Leg", d));
        assert!(!is_automatic_name("", ""));
    }
}
