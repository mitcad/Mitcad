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
    /// references that is a timeline item.
    fn record_producer(&self, record: u64) -> Option<u64> {
        self.seg
            .ref_ids(self.seg.data_of(record))
            .into_iter()
            .find(|r| self.pos_of.contains_key(r))
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

    /// A body input as a reference: a body fingerprint with the body's
    /// names (`_f3d`, resolved by [`super::super::inputs`]) and the item
    /// that made it (`_f3d.producer`, `_f3d.body_index`). `None` when
    /// neither the recipe nor the record decodes.
    pub(super) fn body_input(&self, input: u64) -> Option<Reference> {
        let recipe = recipe::of_input(self.seg, input).filter(|r| r.kind == "body");
        let producer = self.producer(input);
        if recipe.is_none() && producer.is_none() {
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
                ..FingerprintF3d::default()
            }),
            ..Fingerprint::default()
        })))
    }
}
