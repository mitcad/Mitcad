// SPDX-License-Identifier: MIT
//! The bodies of an `.ipt` part for the import with its history
//! (mitcad#60, stage 3): the ASM history the B-rep record carries, rolled
//! back state by state as the `.f3d` import rolls back its `.smbh` blobs,
//! gives the bodies after each operation. The definitions segment's state
//! table names the state each feature's operation made
//! (`mitcad_ipt::design::history_states`), so every feature is checked
//! against its own state.
//!
//! Bodies are converted to the neutral model when the file is opened and
//! built with OCCT only when the importer asks for a state.

use std::collections::{BTreeMap, HashMap};

use cxx::SharedPtr;
use mitcad_f3d::asm::AsmFile;
use mitcad_f3d::asm::history::History;
use mitcad_f3d::convert::{self, ConvertedBody};
use mitcad_import::{StoredBody, StoredGeometry};
use mitcad_ipt::BrepRecord;

use crate::brep_import::{Source, to_ffi};
use crate::kernel::Shape;
use crate::kernel::exchange::ffi::f3d_build_body;

/// The largest distance (mm) from an edge's end to its vertex that a body
/// of a history state may have and still be built (as for `.f3d` files).
const MAX_VERTEX_GAP: f64 = 1e-2;

struct Variant {
    /// The B-rep record it comes from (index into `places`).
    record: usize,
    body: ConvertedBody,
    built: Option<Option<SharedPtr<Shape>>>,
}

/// The bodies of a part's B-rep records.
pub struct IptGeometry {
    places: Vec<(String, String)>,
    variants: Vec<Variant>,
    /// Distinct body sets of the history, oldest first (variants).
    states: Vec<Vec<usize>>,
    /// ASM state id → index into `states` of the bodies after it.
    ops: HashMap<i64, usize>,
    /// Timeline index → index into `states`.
    item_states: HashMap<i64, usize>,
    /// The bodies of records without a history (in the stored design).
    plain: Vec<usize>,
    /// The record whose history gives the states.
    history_record: Option<usize>,
}

impl IptGeometry {
    /// Reads the bodies of the B-rep records; `results` maps timeline
    /// indices to the ASM state ids their operations made.
    pub fn new(records: &[BrepRecord], results: &BTreeMap<i64, i64>) -> Result<Self, String> {
        let options = mitcad_ipt::convert_options();
        let mut g = IptGeometry {
            places: Vec::new(),
            variants: Vec::new(),
            states: Vec::new(),
            ops: HashMap::new(),
            item_states: HashMap::new(),
            plain: Vec::new(),
            history_record: None,
        };
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
            let history = if g.history_record.is_none() {
                History::parse(&file).ok().flatten()
            } else {
                None
            };
            let Some(h) = history else {
                for &r in &bodies {
                    let body = convert::convert_body(&file, r, &options);
                    g.plain.push(g.variants.len());
                    g.variants.push(Variant {
                        record: index,
                        body,
                        built: None,
                    });
                }
                continue;
            };
            g.history_record = Some(index);
            // From the current state back: k operations undone, the bodies
            // after the operation of state k.
            let mut newest_first: Vec<Vec<ConvertedBody>> = Vec::new();
            let mut ops: Vec<(i64, usize)> = Vec::new();
            for k in 0..=h.states.len() {
                let view = h.view(k);
                let set: Vec<ConvertedBody> = bodies
                    .iter()
                    .filter(|r| view.get(r) != Some(&None))
                    .map(|&r| {
                        if k == 0 {
                            convert::convert_body(&file, r, &options)
                        } else {
                            convert::convert_body_at(&file, r, &options, Some(&view))
                        }
                    })
                    .collect();
                let same = newest_first.last().is_some_and(|last| {
                    last.len() == set.len() && last.iter().zip(&set).all(|(a, b)| a.body == b.body)
                });
                if !same {
                    newest_first.push(set);
                }
                if let Some(s) = h.states.get(k) {
                    ops.push((s.id, newest_first.len() - 1));
                }
            }
            let count = newest_first.len();
            g.ops = ops.into_iter().map(|(id, j)| (id, count - 1 - j)).collect();
            // A body as it was in the state before is that state's variant.
            let mut previous: Vec<usize> = Vec::new();
            for set in newest_first.into_iter().rev() {
                let ids: Vec<usize> = set
                    .into_iter()
                    .map(|body| {
                        previous
                            .iter()
                            .copied()
                            .find(|&v| {
                                let old = &g.variants[v].body;
                                old.record == body.record && old.body == body.body
                            })
                            .unwrap_or_else(|| {
                                g.variants.push(Variant {
                                    record: index,
                                    body,
                                    built: None,
                                });
                                g.variants.len() - 1
                            })
                    })
                    .collect();
                previous.clone_from(&ids);
                g.states.push(ids);
            }
        }
        g.item_states = results
            .iter()
            .filter_map(|(item, id)| Some((*item, *g.ops.get(id)?)))
            .collect();
        Ok(g)
    }

    /// The number of items whose history state was found.
    pub fn items_with_states(&self) -> usize {
        self.item_states.len()
    }

    fn build(&mut self, v: usize) -> Option<SharedPtr<Shape>> {
        if let Some(built) = &self.variants[v].built {
            return built.clone();
        }
        let variant = &self.variants[v];
        let (place, asm_version) = &self.places[variant.record];
        let source = Source {
            document: "",
            blob: place,
            asm_version,
            history: Some(variant.record) == self.history_record,
            history_step: 0,
        };
        let data = to_ffi(&source, &variant.body);
        let shape = f3d_build_body(&data);
        let shape = (!shape.is_null()).then_some(shape);
        self.variants[v].built = Some(shape.clone());
        shape
    }

    /// Whether the conversion of a variant lost faces (as for `.f3d`
    /// files): faces left out, broken topology, or a sheet where the stored
    /// design has a solid.
    fn lost_faces(&self, v: usize) -> bool {
        let body = &self.variants[v].body;
        let check = &body.check;
        let gaps = check.vertex_mismatches > 0 && check.max_vertex_gap > MAX_VERTEX_GAP;
        if body.skipped_faces > 0 || check.unpaired_coedges > 0 || check.open_loops > 0 || gaps {
            return true;
        }
        if body.body.is_solid() {
            return false;
        }
        self.states.last().is_some_and(|last| {
            last.iter().any(|&w| {
                let stored = &self.variants[w].body;
                stored.record == body.record && stored.body.is_solid()
            })
        })
    }

    fn bodies(&mut self, variants: &[usize]) -> (Vec<StoredBody<SharedPtr<Shape>>>, usize) {
        let mut broken = 0;
        let mut out = Vec::new();
        for &v in variants {
            let Some(shape) = self.build(v) else {
                broken += 1;
                continue;
            };
            if self.lost_faces(v) {
                broken += 1;
            }
            let variant = &self.variants[v];
            out.push(StoredBody {
                shape,
                name: None,
                source: format!("{}/{}", self.places[variant.record].0, variant.body.record),
                component: None,
                id: Some(v as u64),
            });
        }
        (out, broken)
    }
}

impl StoredGeometry<SharedPtr<Shape>> for IptGeometry {
    fn state_count(&mut self) -> usize {
        self.states.len()
    }

    fn item_state(&mut self, index: i64) -> Option<usize> {
        self.item_states.get(&index).copied()
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<SharedPtr<Shape>>>, String> {
        let variants = self
            .states
            .get(index)
            .cloned()
            .ok_or_else(|| format!("there is no history state {index}"))?;
        match self.bodies(&variants) {
            (bodies, 0) => Ok(bodies),
            (_, broken) => Err(format!(
                "{broken} of the bodies of history state {index} could not be rebuilt"
            )),
        }
    }

    /// The last history state's bodies and the bodies of the records
    /// without a history.
    fn final_bodies(&mut self) -> Result<Vec<StoredBody<SharedPtr<Shape>>>, String> {
        let mut variants = self.states.last().cloned().unwrap_or_default();
        variants.extend(self.plain.iter().copied());
        Ok(self.bodies(&variants).0)
    }
}
