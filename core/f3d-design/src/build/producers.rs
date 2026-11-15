// SPDX-License-Identifier: MIT
//! The feature that made a body (mitcad#96) *(read from the corpus'
//! combines, splits and extrusions, and checked against the bodies the
//! history accepted for them with the learning dump)*.
//!
//! A body input ([`BODY_REF`]) refers to the body's recipe (its names,
//! [`recipe`]) and to the body's record ([`BODY_RECORD`], class version 2),
//! whose references are
//!
//! ```text
//! [null] | body object (D3937028) | producer | [null] | feature manager
//! ```
//!
//! The producer is the timeline item whose operation made the body: an
//! extrusion, revolve, sweep or loft of a new body, a pattern's or
//! mirror's copy, a split's piece, a base feature. Bodies a later item
//! changes keep their record, so the producer names a body also where its
//! recipe does not decode (pipes' bodies, bodies changed by later
//! features). The producer item lists its own records in the order it made
//! the bodies (in object order too, in every item of the corpora): a
//! record's position among them is the body's index. Some producers are
//! not on the design's timeline (base features and features of inserted
//! designs); their bodies have no producer here.
//!
//! The import finds the producer's bodies among its own (the bodies the
//! item made, or the base feature that took its state) and tries them
//! before the bodies found by their names.

use super::super::classes::*;
use super::super::ir::*;
use super::super::recipe;
use super::Builder;

impl Builder<'_> {
    /// The timeline item that made a body record's body: the first of its
    /// references that is a timeline item, or a part of one (mitcad#641:
    /// the base feature inside a paste of bodies or a derive from another
    /// design, which holds the bodies the item brings, is listed among the
    /// item's sub-features, not on the timeline).
    fn record_producer(&self, record: u64) -> Option<u64> {
        self.seg
            .ref_ids(self.seg.data_of(record))
            .into_iter()
            .find_map(|r| {
                if self.pos_of.contains_key(&r) {
                    Some(r)
                } else {
                    self.sub_item.get(&r).copied()
                }
            })
    }

    /// The producer of a body input's body (its timeline index) and the
    /// body's index among the bodies the producer made.
    pub(super) fn producer(&self, input: u64) -> Option<(i64, i64)> {
        let seg = self.seg;
        let record = seg
            .ref_ids(seg.data_of(input))
            .into_iter()
            .find(|&r| seg.guid_of(r) == Some(BODY_RECORD))?;
        let producer = self.record_producer(record)?;
        let mut own: Vec<u64> = Vec::new();
        for r in seg.ref_ids(seg.data_of(producer)) {
            if seg.guid_of(r) == Some(BODY_RECORD)
                && !own.contains(&r)
                && self.record_producer(r) == Some(producer)
            {
                own.push(r);
            }
        }
        // (Every producer of the corpora lists its records; else their
        // object order, which is the same in all of them.)
        if !own.contains(&record) {
            own = seg
                .objects_of(BODY_RECORD)
                .map(|o| o.id)
                .filter(|&r| self.record_producer(r) == Some(producer))
                .collect();
        }
        let index = own.iter().position(|&r| r == record)?;
        Some((self.pos_of[&producer] as i64, index as i64))
    }

    /// The body object (class `D3937028`) of a body input's record
    /// (mitcad#1802): the record's first reference of that class.
    fn body_entity(&self, input: u64) -> Option<u64> {
        let seg = self.seg;
        let record = seg
            .ref_ids(seg.data_of(input))
            .into_iter()
            .find(|&r| seg.guid_of(r) == Some(BODY_RECORD))?;
        seg.ref_ids(seg.data_of(record))
            .into_iter()
            .find(|&r| seg.guid_of(r) == Some(BODY))
    }

    /// A body input as a reference: a body fingerprint with the body's
    /// names (`_f3d`, resolved by [`super::super::inputs`]) and the item
    /// that made it (`_f3d.producer`, `_f3d.body_index`). `None` when
    /// neither the recipe nor the record decodes.
    pub(super) fn body_input(&self, input: u64) -> Option<Reference> {
        let recipe = recipe::of_input(self.seg, input).filter(|r| r.kind == "body");
        let producer = self.producer(input);
        let body_entity = self.body_entity(input);
        if recipe.is_none() && producer.is_none() && body_entity.is_none() {
            return None;
        }
        let (kind, entities) = recipe.map(|r| (r.kind, r.entities)).unzip();
        Some(Reference::Body(Box::new(Fingerprint {
            object_type: Some("BRepBody".to_owned()),
            f3d: Some(FingerprintF3d {
                object_id: Some(input),
                recipe: kind,
                entities,
                producer: producer.map(|p| p.0),
                body_index: producer.map(|p| p.1),
                body_entity,
                document: recipe::owner_guid(self.seg, input),
                context: super::super::decode::slot_contexts(self.seg)
                    .get(&input)
                    .copied(),
                ..FingerprintF3d::default()
            }),
            ..Fingerprint::default()
        })))
    }

    /// The bodies of a fillet's or chamfer's edges: each edge (or face)
    /// input of the sets refers to its body's record like a body input
    /// does, so the item that made the body goes into the edge's `_f3d`
    /// too (`producer`, `body_index`). Bodies that touch share edges in the
    /// same place (the slot of one is the wedge of another): the position
    /// alone does not tell which body the file's edge is on.
    pub(super) fn edge_set_bodies(&self, sets: &mut [EdgeSet]) {
        for r in sets.iter_mut().flat_map(|s| s.edges.iter_mut().flatten()) {
            let (Reference::Edge(fp) | Reference::Face(fp)) = r else {
                continue;
            };
            let Some(f3d) = fp.f3d.as_mut() else {
                continue;
            };
            let Some((producer, index)) = f3d.object_id.and_then(|input| self.producer(input))
            else {
                continue;
            };
            f3d.producer = Some(producer);
            f3d.body_index = Some(index);
        }
    }
}
