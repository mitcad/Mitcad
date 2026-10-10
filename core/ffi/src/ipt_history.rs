// SPDX-License-Identifier: MIT
//! The bodies of an `.ipt` part for the import with its history
//! (mitcad#60, stage 3): the ASM history the B-rep record carries, rolled
//! back state by state as the `.f3d` import rolls back its `.smbh` blobs,
//! gives the bodies after each operation. The definitions segment's state
//! table names the state each feature's operation made
//! (`mitcad_ipt::design::history_states`), so every feature is checked
//! against its own state.
//!
//! The states are read once for all tries of an import
//! (`mitcad_ipt::states::States`, which keeps the distinct bodies by the
//! step of the history they were converted at); each try converts a body
//! again and builds it with OCCT only when the importer asks for its state
//! ([`IptGeometry`]).

use std::sync::Arc;

use cxx::SharedPtr;
use mitcad_import::{StoredBody, StoredGeometry};
use mitcad_ipt::states::States;

use crate::brep_import::{Source, to_ffi};
use crate::f3d_import::build_stored_body;
use crate::kernel::Shape;

/// A try's view of the part's states: the bodies it built.
pub struct IptGeometry {
    states: Arc<States>,
    built: Vec<Option<Option<SharedPtr<Shape>>>>,
    /// Each variant's lumps once looked at ([`IptGeometry::lumps`]).
    lumps: Vec<Option<Vec<SharedPtr<Shape>>>>,
}

impl IptGeometry {
    pub fn new(states: Arc<States>) -> Self {
        let built = vec![None; states.variants.len()];
        let lumps = vec![None; states.variants.len()];
        Self {
            states,
            built,
            lumps,
        }
    }

    /// The number of items whose history state was found.
    pub fn items_with_states(&self) -> usize {
        self.states.items_with_states()
    }

    fn build(&mut self, v: usize) -> Option<SharedPtr<Shape>> {
        if let Some(built) = &self.built[v] {
            return built.clone();
        }
        let states = &self.states;
        let variant = &states.variants[v];
        let (place, asm_version) = &states.places[variant.record];
        let source = Source {
            document: "",
            blob: place,
            asm_version,
            history: Some(variant.record) == states.history_record,
            history_step: 0,
        };
        let shape = states
            .converted(v)
            .and_then(|body| build_stored_body(&to_ffi(&source, &body)));
        self.built[v] = Some(shape.clone());
        shape
    }

    fn bodies(&mut self, variants: &[usize]) -> (Vec<StoredBody<SharedPtr<Shape>>>, usize) {
        let mut broken = 0;
        let mut out = Vec::new();
        for &v in variants {
            // A deleted body that does not build cleanly is left out of the
            // state (as the states held no deleted bodies before), so that
            // the others still check the items.
            let deleted = self.states.variants[v].deleted;
            let Some(shape) = self.build(v) else {
                if !deleted {
                    broken += 1;
                }
                continue;
            };
            let states = &self.states;
            let variant = &states.variants[v];
            if variant.lost {
                if deleted {
                    continue;
                }
                broken += 1;
            }
            let source = format!(
                "{}/{}",
                states.places[variant.record].0, variant.body_record
            );
            let lumps = self.lumps(v, &shape);
            if lumps.len() < 2 {
                out.push(StoredBody {
                    shape,
                    name: None,
                    source,
                    component: None,
                    id: Some(v as u64),
                });
                continue;
            }
            for (k, lump) in (0u64..).zip(lumps) {
                out.push(StoredBody {
                    shape: lump,
                    name: None,
                    source: format!("{source}#{k}"),
                    component: None,
                    id: Some(v as u64 | ((k + 1) << 32)),
                });
            }
        }
        (out, broken)
    }

    /// The lumps of a variant's body ([`mitcad_import::history::lumps`]),
    /// looked at once (and again once its shape was released).
    fn lumps(&mut self, v: usize, shape: &SharedPtr<Shape>) -> Vec<SharedPtr<Shape>> {
        if let Some(lumps) = &self.lumps[v] {
            return lumps.clone();
        }
        let lumps = mitcad_import::history::lumps(&crate::kernel::OcctKernel, shape);
        self.lumps[v] = Some(lumps.clone());
        lumps
    }
}

impl StoredGeometry<SharedPtr<Shape>> for IptGeometry {
    fn state_count(&mut self) -> usize {
        self.states.states.len()
    }

    fn item_state(&mut self, index: i64) -> Option<usize> {
        self.states.item_states.get(&index).copied()
    }

    fn item_without_result(&mut self, index: i64) -> bool {
        self.states.without_result.contains(&index)
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<SharedPtr<Shape>>>, String> {
        let variants = self
            .states
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

    /// Drops the shapes built for the variants of the states before
    /// `state` alone (mitcad#80: memory got tight); they are built again
    /// when asked for. A variant that did not build is not tried again.
    fn release_before(&mut self, state: usize) {
        let states = &self.states;
        let mut kept = vec![false; states.variants.len()];
        for &v in states
            .states
            .iter()
            .skip(state)
            .flatten()
            .chain(&states.plain)
        {
            kept[v] = true;
        }
        for ((built, lumps), kept) in self.built.iter_mut().zip(&mut self.lumps).zip(kept) {
            if !kept && matches!(built, Some(Some(_))) {
                *built = None;
                *lumps = None;
            }
        }
    }

    /// The last history state's bodies and the bodies of the records
    /// without a history.
    fn final_bodies(&mut self) -> Result<Vec<StoredBody<SharedPtr<Shape>>>, String> {
        let mut variants = self.states.states.last().cloned().unwrap_or_default();
        variants.extend(self.states.plain.iter().copied());
        Ok(self.bodies(&variants).0)
    }
}
