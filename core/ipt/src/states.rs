// SPDX-License-Identifier: MIT
//! The bodies of a part's B-rep records at the states of their ASM
//! history (mitcad#60, stage 3), read once for all tries of an import: the
//! import with its history checks every feature against the bodies of the
//! state its operation made, rolled back as the `.f3d` import rolls back
//! its `.smbh` blobs (`mitcad-ffi`'s `ipt_history.rs` builds them).
//!
//! Every state's bodies are converted to the neutral model to find the
//! distinct ones, but only what tells them apart is kept: the step of the
//! history a distinct body was converted at. A body is converted again
//! when its state is asked for ([`States::converted`]). Kept converted, a
//! long history of a large body (a thousand states of a body of 15 000
//! faces) took more memory than the import may have.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};

use mitcad_f3d::asm::AsmFile;
use mitcad_f3d::asm::history::History;
use mitcad_f3d::convert::{self, ConvertedBody};

use crate::BrepRecord;

/// The largest distance (mm) from an edge's end to its vertex that a body
/// of a history state may have and still be built (as for `.f3d` files).
const MAX_VERTEX_GAP: f64 = 1e-2;

/// A distinct body of the part's states.
#[derive(Debug)]
pub struct Variant {
    /// The B-rep record it comes from (index into [`States::places`]).
    pub record: usize,
    /// Its `body` record in the record's ASM data.
    pub body_record: usize,
    /// The history step it was converted at (operations rolled back).
    pub step: usize,
    /// Whether its conversion lost faces ([`lost_faces`]): its state's
    /// bodies cannot be rebuilt.
    pub lost: bool,
    /// Whether it is a body the history deleted (a combine's tool, its
    /// copy standing for it in the states before the deletion).
    pub deleted: bool,
    /// The converted body of a record without a history (kept: it has one
    /// state only).
    body: Option<ConvertedBody>,
}

/// The history record's data, to convert a state's bodies again.
struct Rollback {
    file: AsmFile,
    history: History,
}

/// The bodies of a part's B-rep records and their history states.
pub struct States {
    /// Each record's place (`PmBRepSegment#107`) and ASM version.
    pub places: Vec<(String, String)>,
    pub variants: Vec<Variant>,
    /// Distinct body sets of the history, oldest first (variants); the
    /// last is the stored design.
    pub states: Vec<Vec<usize>>,
    /// Timeline index → index into `states`.
    pub item_states: HashMap<i64, usize>,
    /// Timeline indices whose state the history does not hold, between
    /// states it does (a feature suppressed or failed in the file).
    pub without_result: HashSet<i64>,
    /// The bodies of records without a history (in the stored design).
    pub plain: Vec<usize>,
    /// The record whose history gives the states.
    pub history_record: Option<usize>,
    rollback: Option<Rollback>,
}

/// Whether the conversion of a body lost faces (as for `.f3d` files):
/// faces left out, broken topology, or a sheet where the stored design has
/// a solid (`stored_solids`: the `body` records that are solids in the last
/// state). A body of the last state (`last`) is the stored design itself,
/// as the bodies-only import builds it (a sheet with edges of three faces,
/// edges farther from their vertices than the bound): it lost nothing, and
/// the state's sheets come in from it.
fn lost_faces(body: &ConvertedBody, last: bool, stored_solids: &HashSet<usize>) -> bool {
    if last {
        return false;
    }
    let check = &body.check;
    let gaps = check.vertex_mismatches > 0 && check.max_vertex_gap > MAX_VERTEX_GAP;
    if body.skipped_faces > 0 || check.unpaired_coedges > 0 || check.open_loops > 0 || gaps {
        return true;
    }
    !body.body.is_solid() && stored_solids.contains(&body.record)
}

impl States {
    /// Reads the bodies of the B-rep records; `results` maps timeline
    /// indices to the ASM state ids their operations made. Without `rolled`
    /// (a design without timeline items) only the stored bodies are read:
    /// no item asks for the states before.
    pub fn new(
        records: &[BrepRecord],
        results: &BTreeMap<i64, i64>,
        rolled: bool,
    ) -> Result<Self, String> {
        let options = crate::convert_options();
        let mut g = States {
            places: Vec::new(),
            variants: Vec::new(),
            states: Vec::new(),
            item_states: HashMap::new(),
            without_result: HashSet::new(),
            plain: Vec::new(),
            history_record: None,
            rollback: None,
        };
        let mut ops_by_id: HashMap<i64, usize> = HashMap::new();
        let mut stored_solids: HashSet<usize> = HashSet::new();
        let mut other_history = false;
        for (index, record) in records.iter().enumerate() {
            let place = record.place();
            let file = AsmFile::parse(&record.asm).map_err(|e| format!("{place}: {e}"))?;
            g.places
                .push((place.clone(), file.header.asm_version.clone()));
            let bodies: Vec<usize> = convert::body_records(&file)
                .into_iter()
                .filter(|(_, top)| *top)
                .map(|(r, _)| r)
                .collect();
            let history = History::parse(&file).ok().flatten();
            if history.is_some() && g.history_record.is_some() {
                // (Its states are not looked at: the first history's ids
                // alone do not tell a suppressed feature.)
                other_history = true;
            }
            let history = history.filter(|_| g.history_record.is_none());
            let Some(h) = history else {
                for &r in &bodies {
                    let body = convert::convert_body(&file, r, &options);
                    g.plain.push(g.variants.len());
                    g.variants.push(Variant {
                        record: index,
                        body_record: body.record,
                        step: 0,
                        lost: false,
                        deleted: false,
                        body: Some(body),
                    });
                }
                continue;
            };
            g.history_record = Some(index);
            // Bodies a later operation deleted (a combine's tools) exist in
            // the states before it, their copies standing for them.
            let deleted = crate::deleted_bodies(&h, &file);
            // From the current state back: k operations undone, the bodies
            // after the operation of state k. Only the newer distinct
            // state's bodies stay converted, to tell whether the next one
            // differs; a body as it is in the newer state is that state's
            // variant.
            let mut newest_first: Vec<Vec<usize>> = Vec::new();
            let mut newer: Vec<(usize, ConvertedBody)> = Vec::new();
            let mut ops: Vec<(i64, usize)> = Vec::new();
            // The view of step k, grown state by state (as `History::view`).
            let mut view: HashMap<usize, Option<usize>> = HashMap::new();
            let steps = if rolled { h.states.len() } else { 0 };
            for k in 0..=steps {
                if let Some(state) = k.checked_sub(1).and_then(|j| h.states.get(j)) {
                    for &(before, after) in &state.bulletins {
                        if let Some(entity) = after {
                            view.insert(entity, before);
                        }
                    }
                }
                let gone = deleted
                    .iter()
                    .filter(|&&(_, j)| j < k)
                    .map(|&(copy, _)| copy);
                let set: Vec<ConvertedBody> = bodies
                    .iter()
                    .copied()
                    .chain(gone)
                    .filter(|r| view.get(r) != Some(&None))
                    .map(|r| {
                        if k == 0 {
                            convert::convert_body(&file, r, &options)
                        } else {
                            convert::convert_body_at(&file, r, &options, Some(&view))
                        }
                    })
                    .collect();
                if k == 0 {
                    stored_solids
                        .extend(set.iter().filter(|b| b.body.is_solid()).map(|b| b.record));
                }
                let same = !newest_first.is_empty()
                    && newer.len() == set.len()
                    && newer.iter().zip(&set).all(|((_, a), b)| a.body == b.body);
                if !same {
                    let mut kept: Vec<(usize, ConvertedBody)> = Vec::with_capacity(set.len());
                    for body in set {
                        let known = newer
                            .iter()
                            .find(|(_, old)| old.record == body.record && old.body == body.body)
                            .map(|(v, _)| *v);
                        let v = known.unwrap_or_else(|| {
                            g.variants.push(Variant {
                                record: index,
                                body_record: body.record,
                                step: k,
                                lost: lost_faces(&body, k == 0, &stored_solids),
                                deleted: deleted.iter().any(|&(copy, _)| copy == body.record),
                                body: None,
                            });
                            g.variants.len() - 1
                        });
                        kept.push((v, body));
                    }
                    newest_first.push(kept.iter().map(|(v, _)| *v).collect());
                    newer = kept;
                }
                if let Some(s) = h.states.get(k) {
                    ops.push((s.id, newest_first.len() - 1));
                }
            }
            let count = newest_first.len();
            ops_by_id = ops.into_iter().map(|(id, j)| (id, count - 1 - j)).collect();
            g.states = newest_first.into_iter().rev().collect();
            g.rollback = Some(Rollback { file, history: h });
        }
        // The bodies of records without a history, against the last state's
        // solids (known once the history is read).
        for &v in &g.plain {
            let variant = &mut g.variants[v];
            if let Some(body) = &variant.body {
                variant.lost = lost_faces(body, false, &stored_solids);
            }
        }
        g.item_states = results
            .iter()
            .filter_map(|(item, id)| Some((*item, *ops_by_id.get(id)?)))
            .collect();
        // A feature suppressed in the file (or failed there) keeps the
        // state ids it had, while its states are gone from the history,
        // which holds states before and after them (ids are not reused):
        // its result is not kept. (Of the older corpus' 351 such items none
        // gave a state, and the history showed no change by them.) Only
        // when the history was read in full (`rolled`).
        let low = ops_by_id.keys().min().copied();
        let high = ops_by_id.keys().max().copied();
        g.without_result = results
            .iter()
            .filter(|(_, id)| {
                rolled
                    && !other_history
                    && !ops_by_id.contains_key(id)
                    && low.is_some_and(|l| l < **id)
                    && high.is_some_and(|h| **id < h)
            })
            .map(|(item, _)| *item)
            .collect();
        if std::env::var_os("MITCAD_IMPORT_TRACE_STATES").is_some() {
            let mut ops: Vec<_> = ops_by_id.iter().collect();
            ops.sort();
            eprintln!("import: ASM states (id -> state): {ops:?}");
            eprintln!("import: items' ASM states: {results:?}");
        }
        Ok(g)
    }

    /// The number of items whose history state was found.
    pub fn items_with_states(&self) -> usize {
        self.item_states.len()
    }

    /// A variant's converted body: the one kept, or the history record's
    /// body converted again at its step.
    pub fn converted(&self, v: usize) -> Option<Cow<'_, ConvertedBody>> {
        let variant = self.variants.get(v)?;
        if let Some(body) = &variant.body {
            return Some(Cow::Borrowed(body));
        }
        let r = self.rollback.as_ref()?;
        let options = crate::convert_options();
        Some(Cow::Owned(if variant.step == 0 {
            convert::convert_body(&r.file, variant.body_record, &options)
        } else {
            let view = r.history.view(variant.step);
            convert::convert_body_at(&r.file, variant.body_record, &options, Some(&view))
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(asm: Vec<u8>) -> BrepRecord {
        BrepRecord {
            segment: "PmBRepSegment".to_owned(),
            record: Some(1),
            asm,
        }
    }

    #[test]
    fn distinct_states_are_kept_by_their_steps() {
        let records = [record(mitcad_f3d::testdata::cube_with_history_blob())];
        // Items 0, 1 and 2 made the states 1, 2 and 3.
        let results = BTreeMap::from([(0, 1), (1, 2), (2, 3)]);
        let g = States::new(&records, &results, true).unwrap();
        assert_eq!(g.history_record, Some(0));
        // Oldest first: no body (before state 1), the corner moved back
        // (after states 1 and 2, the same bodies), the stored cube.
        assert_eq!(g.states, vec![vec![], vec![1], vec![0]]);
        assert_eq!(g.variants.len(), 2);
        assert_eq!((g.variants[0].step, g.variants[1].step), (0, 1));
        assert_eq!(g.item_states, HashMap::from([(0, 1), (1, 1), (2, 2)]));
        assert_eq!(g.items_with_states(), 3);
        // The stored cube lost nothing; the rolled-back corner is far from
        // the edges' ends.
        assert!(!g.variants[0].lost);
        assert!(g.variants[1].lost);
        // Converted again, a body is the one the states were told apart by.
        let file = AsmFile::parse(&records[0].asm).unwrap();
        let history = History::parse(&file).unwrap().unwrap();
        let options = crate::convert_options();
        let view = history.view(1);
        let rolled = convert::convert_body_at(&file, 1, &options, Some(&view));
        assert!(g.converted(1).unwrap().body == rolled.body);
        let stored = convert::convert_body(&file, 1, &options);
        assert!(g.converted(0).unwrap().body == stored.body);
        assert!(rolled.body != stored.body);
        assert!(g.converted(2).is_none());
    }

    #[test]
    fn without_items_only_the_stored_bodies_are_read() {
        let records = [record(mitcad_f3d::testdata::cube_with_history_blob())];
        let g = States::new(&records, &BTreeMap::new(), false).unwrap();
        assert_eq!(g.states, vec![vec![0]]);
        assert_eq!(g.variants.len(), 1);
        assert!(g.item_states.is_empty());
    }

    #[test]
    fn records_without_a_history_keep_their_bodies() {
        let records = [record(mitcad_f3d::testdata::cube_blob())];
        let g = States::new(&records, &BTreeMap::new(), true).unwrap();
        assert_eq!(g.history_record, None);
        assert!(g.states.is_empty());
        assert_eq!(g.plain, vec![0]);
        assert!(!g.variants[0].lost);
        assert!(matches!(g.converted(0), Some(Cow::Borrowed(_))));
    }
}
