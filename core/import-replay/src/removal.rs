// SPDX-License-Identifier: MIT
//! Bodies the file took out of the design (a removal item).
//!
//! A removal has no history state of its own and the history keeps the
//! removed body as it was, so it cannot be checked against a state as other
//! items are. It names its body: the replay's body that is the stored body
//! of that name goes, and the stored body leaves the history's states from
//! the replay's state on ([`Oracle::retire`]), as the tools a combine
//! consumed do (`producers.rs`).

use mitcad_dump::{Fingerprint, Reference, TimelineItem};

use crate::history::{Sig, bodies_distance};

impl<K: crate::ImportKernel> crate::Importer<'_, K> {
    /// A removal as a `remove_body` feature. The stored counterpart of the
    /// removed body is the stored body of the state the replay stands for
    /// that has its measures; the bodies left must be that state's others.
    /// False (nothing done) when the history does not settle it: the item
    /// then takes the usual route for items without a state.
    pub(crate) fn remove_body_item(&mut self, at: usize, index: i64, item: &TimelineItem) -> bool {
        let name = crate::item_name(item);
        let not = |why: &str| {
            if crate::tracing() {
                eprintln!("import: {name}: not matched to the state: {why}");
            }
            false
        };
        if !self.pending.is_empty() {
            return self.remove_behind_pending(at, index, item)
                || not("items before it are pending");
        }
        let Some(q) = self.oracle.cursor else {
            return not("the replay stands for no state");
        };
        let kernel = self.doc.kernel();
        let Ok(stored) = self.oracle.state(kernel, q).map(<[_]>::to_vec) else {
            return not("the state was not rebuilt");
        };
        let stored_sigs: Vec<Sig> = stored.iter().map(|(_, s)| *s).collect();
        let current = self.current_bodies();
        let current_sigs: Vec<Sig> = current.iter().map(|(_, s)| *s).collect();
        if bodies_distance(&stored_sigs, &current_sigs).is_none() {
            return not("the bodies are not the state's");
        }
        // The body by its record first (mitcad#1802): the item that made
        // it counts its bodies among those it still has, so after a removal
        // of one of them its index names another (a reported design's
        // removals took 70 bodies the file shows so).
        let by_record = self.removed_by_record(item, &stored);
        let candidate = match &by_record {
            Some((c, _)) => c.clone(),
            None => match self.translate(index, item) {
                Ok(mut c) if !c.is_empty() => c.remove(0),
                Ok(_) => return not("nothing to try"),
                Err(e) => return not(&e),
            },
        };
        let removed = candidate.defs[0]["bodies"][0].as_str().unwrap_or_default();
        if crate::tracing() {
            eprintln!(
                "import: {name}: state {q} has {:?}; the replay has {:?}; removes {removed}",
                stored_sigs
                    .iter()
                    .map(|s| (s.volume, s.faces))
                    .collect::<Vec<_>>(),
                current_sigs
                    .iter()
                    .map(|s| (s.volume, s.faces))
                    .collect::<Vec<_>>()
            );
        }
        let Some(sig) = current
            .iter()
            .find(|(uid, _)| uid.to_string() == removed)
            .map(|(_, s)| *s)
        else {
            return not("the removed body is not among the replay's");
        };
        // The stored body with the same faces and measures, else the
        // nearest within the tolerance (a replayed split piece can have its
        // faces split otherwise than the file's).
        let nearest = |near: &dyn Fn(&Sig) -> bool| {
            stored
                .iter()
                .filter(|(b, s)| b.id.is_some() && near(s))
                .min_by(|(_, a), (_, b)| a.distance(&sig).total_cmp(&b.distance(&sig)))
                .and_then(|(b, _)| b.id)
        };
        let recorded = by_record.as_ref().map(|(_, id)| *id);
        let Some(id) = recorded
            .or_else(|| nearest(&|s| s.same(&sig)))
            .or_else(|| nearest(&|s| s.matches(&sig)))
        else {
            return not("no stored body is the removed one");
        };
        // Never a body the file keeps in the design (mitcad#1802): where
        // the records name the stored body found and it is not one the file
        // took out, the removal found the wrong one (a pasted copy and its
        // original lie in one place with the same measures).
        if recorded.is_none()
            && let Some((b, _)) = stored.iter().find(|(b, _)| b.id == Some(id))
            && self.kept_in_design(crate::stored_record(&b.source))
        {
            return not("the body found is one the file keeps in the design");
        }
        let depth = self.doc.undo_depth();
        let uids = match self.try_candidate(index, &candidate, item.name()) {
            Ok(uids) => uids,
            Err(e) => {
                self.undo_to(depth);
                return not(&e);
            }
        };
        let expected: Vec<Sig> = stored
            .iter()
            .filter(|(b, _)| b.id != Some(id))
            .map(|(_, s)| *s)
            .collect();
        if bodies_distance(&expected, &self.current_sigs()).is_none() {
            self.undo_to(depth);
            return not("the bodies left are not the state's others");
        }
        if crate::tracing() {
            let source = stored
                .iter()
                .find(|(b, _)| b.id == Some(id))
                .map(|(b, _)| &b.source);
            eprintln!(
                "import: {}: retires {source:?} from state {q}",
                crate::item_name(item)
            );
        }
        self.oracle.retire(q, &[id]);
        self.report.history.matched += 1;
        self.accepted(at, index, &uids, &candidate, Some(true));
        true
    }

    /// A removal while items before it are pending (mitcad#1802: in a
    /// reported design 262 removals came after items the history had not
    /// settled yet, and their bodies stayed in the design). The replay does
    /// not stand for a state then, so the removal is not checked against
    /// one; its body is found by its record in the state the replay last
    /// stood for ([`Importer::removed_by_record`]) and removed, and the
    /// stored body leaves the states from there on, so that the fallback
    /// the pending items may take does not bring it back. False when the
    /// record does not settle it.
    fn remove_behind_pending(&mut self, at: usize, index: i64, item: &TimelineItem) -> bool {
        let Some(q) = self.oracle.cursor else {
            return false;
        };
        let kernel = self.doc.kernel();
        let Ok(stored) = self.oracle.state(kernel, q).map(<[_]>::to_vec) else {
            return false;
        };
        let Some((candidate, id)) = self.removed_by_record(item, &stored) else {
            return false;
        };
        let depth = self.doc.undo_depth();
        let uids = match self.try_candidate(index, &candidate, item.name()) {
            Ok(uids) => uids,
            Err(_) => {
                self.undo_to(depth);
                return false;
            }
        };
        if crate::tracing() {
            let source = stored
                .iter()
                .find(|(b, _)| b.id == Some(id))
                .map(|(b, _)| &b.source);
            eprintln!(
                "import: {}: retires {source:?} from state {q}",
                crate::item_name(item)
            );
        }
        self.oracle.retire(q, &[id]);
        self.accepted(at, index, &uids, &candidate, None);
        true
    }

    /// The body input of a removal (`itemToRemove`).
    fn removed_input(item: &TimelineItem) -> Option<Fingerprint> {
        let detail = item
            .detail
            .as_ref()
            .and_then(|d| serde_json::to_value(d).ok())?;
        let map = detail.get("itemToRemove")?.as_object()?.clone();
        match Reference::from_map(map) {
            Reference::Body(fp) => Some(*fp),
            _ => None,
        }
    }

    /// The removal of the replay's body that is the stored body the item's
    /// body record names (mitcad#1802), with that stored body's id: the
    /// item's body reference names its body object (`_f3d.body_entity`),
    /// whose record in the file's body records gives its blob and ASM
    /// record; the stored body of the state `stored` from that record is
    /// the removed one, and the replay's body that is it
    /// ([`Importer::body_of_record`]: in its component, by its faces and
    /// measures, where several are by the item that made it) the one to
    /// remove. None when any step does not settle it.
    fn removed_by_record(
        &self,
        item: &TimelineItem,
        stored: &[(crate::history::StoredBody<K::Shape>, Sig)],
    ) -> Option<(crate::Candidate, u64)> {
        let fp = Self::removed_input(item)?;
        let source = self.record_source(&fp)?;
        let producer = fp.f3d.as_ref().and_then(|f| f.producer);
        let (uid, id) = match self.body_of_record(&source, producer, stored) {
            Ok(found) => found,
            Err(why) => {
                if crate::tracing() {
                    eprintln!(
                        "import: {}: its record {source} does not settle the body: {why:?}",
                        crate::item_name(item)
                    );
                }
                return None;
            }
        };
        let mut c = crate::Candidate::new(
            serde_json::json!({"type": "remove_body", "bodies": [uid.to_string()]}),
        );
        c.input_provenance = crate::InputProvenance::Read;
        Some((c, id))
    }
}
