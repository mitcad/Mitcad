// SPDX-License-Identifier: MIT
//! Assembly cuts (mitcad#2131): an Extrude, Revolve or Hole cut made in an
//! assembly machines the occurrences it reaches of components that are
//! linked or placed more than once, in that assembly only.
//!
//! A component's bodies are one definition that all its occurrences show,
//! and a linked component's definition is its design file. A cut made in
//! an assembly may still change such a component, occurrence by
//! occurrence (a weldment drilled as a whole): the cut leaves the
//! definition alone and gives the occurrences it reaches bodies of their
//! own, the **machined pieces**:
//! - each piece is a body of its own, made by the cut (its id is the
//!   cut's, [`piece_uid`]), in the coordinates of the component it
//!   machines, so the machined occurrence moves with its joints as any
//!   occurrence does (Drive Joint, motion studies, drags);
//! - the recompute keeps the pieces apart from every component's bodies,
//!   under [`MACHINED_HOLDER`]: no feature of the component sees them (its
//!   definition stays as it is), no other occurrence shows them;
//! - [`Machined`] says, occurrence by occurrence (by its path from the
//!   root), which definition bodies the pieces stand for there and which
//!   cuts made them. The occurrences of the design show the pieces in place
//!   of those bodies (`Document::instances`), and with them every consumer
//!   that goes by the instances: drawings, exports, mass properties,
//!   interference, contact sets.
//!
//! A later cut that reaches a machined occurrence cuts its pieces again
//! (they are what the occurrence is at that point) and replaces them. A
//! cut is re-applied by its own definition whenever the timeline is, a
//! linked design's new geometry after Update Link included.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::features::OccurrencePath;
use crate::fingerprint::Fingerprint;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};

/// The key under which the recompute keeps the machined pieces among the
/// components' bodies (and their owner while it goes through the
/// timeline): no component of a design has it, so no feature reads them
/// as a component's bodies. The document answers the component a piece
/// machines ([`Machined::component_of`]).
pub const MACHINED_HOLDER: ComponentUid = ComponentUid(u32::MAX - 1);

/// What a cut did to one body of one occurrence: the definition body it
/// machined there and the pieces that occurrence shows instead (none when
/// the cut took all of it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachinedChange {
    /// The occurrence's path from the root.
    pub occurrence: OccurrencePath,
    /// The component it places.
    pub component: ComponentUid,
    /// The definition body (of `component`) the pieces stand for.
    pub replaces: BodyUid,
    pub pieces: Vec<BodyUid>,
}

/// One machined occurrence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MachinedOccurrence {
    /// The component the occurrence places.
    pub component: ComponentUid,
    /// Each definition body the cuts changed here, with the pieces shown
    /// in its place (none: cut away).
    pub replaced: BTreeMap<BodyUid, Vec<BodyUid>>,
    /// The cuts that machined it, in timeline order.
    pub cuts: Vec<FeatureUid>,
}

/// The machined occurrences at a point of the timeline, by their paths
/// from the root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Machined(pub BTreeMap<Vec<OccurrenceUid>, MachinedOccurrence>);

impl Machined {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The machined occurrence at `path` (from the root), if it is one.
    pub fn at(&self, path: &[OccurrenceUid]) -> Option<&MachinedOccurrence> {
        self.0.get(path)
    }

    /// Records what the cut `feature` did.
    pub(crate) fn apply(&mut self, feature: FeatureUid, changes: &[MachinedChange]) {
        for change in changes {
            let entry = self.0.entry(change.occurrence.0.clone()).or_default();
            entry.component = change.component;
            entry
                .replaced
                .insert(change.replaces, change.pieces.clone());
            if !entry.cuts.contains(&feature) {
                entry.cuts.push(feature);
            }
        }
    }

    /// The definition body a piece stands for, if `body` is a piece.
    pub fn replaced_by(&self, body: BodyUid) -> Option<BodyUid> {
        self.0.values().find_map(|o| {
            o.replaced
                .iter()
                .find(|(_, pieces)| pieces.contains(&body))
                .map(|(replaced, _)| *replaced)
        })
    }

    /// The occurrence a piece is shown at and the component it machines,
    /// if `body` is a piece.
    pub fn component_of(&self, body: BodyUid) -> Option<(&[OccurrenceUid], ComponentUid)> {
        self.0.iter().find_map(|(path, o)| {
            o.replaced
                .values()
                .any(|pieces| pieces.contains(&body))
                .then_some((path.as_slice(), o.component))
        })
    }

    /// The bodies the occurrence at `path` shows: the definition's
    /// `bodies`, each one machined there replaced by its pieces. A machined
    /// body no longer in the definition takes its pieces with it.
    pub fn shown(&self, path: &[OccurrenceUid], bodies: Vec<BodyUid>) -> Vec<BodyUid> {
        let Some(machined) = self.0.get(path) else {
            return bodies;
        };
        let mut shown = Vec::with_capacity(bodies.len());
        for body in bodies {
            match machined.replaced.get(&body) {
                Some(pieces) => shown.extend(pieces.iter().copied()),
                None => shown.push(body),
            }
        }
        shown
    }

    /// Writes the machined occurrences to a cache key: a cut reads what
    /// the occurrences it reaches are (`extrude_scope::assembly_key`).
    pub(crate) fn fingerprint(&self, key: &mut Fingerprint) {
        key.u64(self.0.len() as u64);
        for (path, o) in &self.0 {
            key.u64(path.len() as u64);
            for uid in path {
                key.u32(uid.0);
            }
            key.u32(o.component.0).u64(o.replaced.len() as u64);
            for (replaced, pieces) in &o.replaced {
                key.u64(replaced.feature.0)
                    .u32(replaced.index)
                    .u64(pieces.len() as u64);
                for piece in pieces {
                    key.u64(piece.feature.0).u32(piece.index);
                }
            }
        }
    }
}

/// The pieces' ids are the cut's, above those it numbers one by one
/// ([`crate::features::EvalContext::new_body`]): from this index on.
const PIECE_BASE: u32 = 0x4000_0000;

/// The id of the `n`th piece the cut `feature` leaves of the definition
/// body `replaces` at the occurrence `path`: the same whatever else the
/// cut reaches, so that what refers to a machined face by its body (a
/// joint, a later feature's reference) keeps finding it when another
/// occurrence comes into the cut or goes out of it. `taken` are the ids
/// the cut gave already (two hashes that meet take the next free one).
pub(crate) fn piece_uid(
    feature: FeatureUid,
    path: &[OccurrenceUid],
    replaces: BodyUid,
    n: usize,
    taken: &[BodyUid],
) -> BodyUid {
    let mut key = Fingerprint::new("assembly-cut-piece-v1");
    key.u64(path.len() as u64);
    for uid in path {
        key.u32(uid.0);
    }
    key.u64(replaces.feature.0)
        .u32(replaces.index)
        .u64(n as u64);
    let hash = key.finish();
    let mut index = PIECE_BASE | ((hash as u32) & (PIECE_BASE - 1));
    loop {
        let uid = BodyUid::new(feature, index);
        if !taken.contains(&uid) {
            return uid;
        }
        index = PIECE_BASE | (index.wrapping_add(1) & (PIECE_BASE - 1));
    }
}

/// The words a machined occurrence's row and selection card say
/// (docs/ui-conventions.md "What a message says": no names or ids).
pub const MACHINED_NOTE: &str = "Machined in this assembly; its own design stays as it is.";

#[cfg(test)]
mod tests {
    use super::*;

    fn body(feature: u64, index: u32) -> BodyUid {
        BodyUid::new(FeatureUid(feature), index)
    }

    #[test]
    fn an_occurrence_shows_its_pieces_in_place_of_the_bodies_they_machine() {
        let mut machined = Machined::default();
        let path = vec![OccurrenceUid(2)];
        machined.apply(
            FeatureUid(9),
            &[MachinedChange {
                occurrence: OccurrencePath(path.clone()),
                component: ComponentUid(1),
                replaces: body(1, 0),
                pieces: vec![body(9, PIECE_BASE + 3)],
            }],
        );
        assert_eq!(
            machined.shown(&path, vec![body(1, 0), body(1, 1)]),
            vec![body(9, PIECE_BASE + 3), body(1, 1)]
        );
        // Another occurrence of the same component shows the definition.
        assert_eq!(
            machined.shown(&[OccurrenceUid(3)], vec![body(1, 0)]),
            vec![body(1, 0)]
        );
        assert_eq!(
            machined.component_of(body(9, PIECE_BASE + 3)),
            Some((path.as_slice(), ComponentUid(1)))
        );
        assert_eq!(
            machined.replaced_by(body(9, PIECE_BASE + 3)),
            Some(body(1, 0))
        );
        assert_eq!(
            machined.at(&path).map(|o| o.cuts.clone()),
            Some(vec![FeatureUid(9)])
        );
    }

    #[test]
    fn piece_ids_depend_on_the_occurrence_and_body_only() {
        let a = piece_uid(FeatureUid(5), &[OccurrenceUid(1)], body(1, 0), 0, &[]);
        let again = piece_uid(FeatureUid(5), &[OccurrenceUid(1)], body(1, 0), 0, &[]);
        let other = piece_uid(FeatureUid(5), &[OccurrenceUid(2)], body(1, 0), 0, &[]);
        assert_eq!(a, again);
        assert_ne!(a, other);
        assert!(a.index >= PIECE_BASE);
        let next = piece_uid(FeatureUid(5), &[OccurrenceUid(1)], body(1, 0), 0, &[a]);
        assert_ne!(next, a);
    }
}
