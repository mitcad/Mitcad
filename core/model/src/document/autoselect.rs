// SPDX-License-Identifier: MIT
//! Current profile groups of the final unused sketch, for Solid mode.

use std::collections::BTreeSet;

use serde::Serialize;

use super::Document;
use crate::features::FeatureDef;
use crate::ids::{ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::topo::{CurveId, RegionKey};

/// Disjoint origin groups and their union. An ineligible document has no
/// sketch, component or candidates. Region keys always name current regions.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct AutoselectProfiles {
    pub eligible: bool,
    pub sketch: Option<FeatureUid>,
    pub component: Option<ComponentUid>,
    pub projected: Vec<RegionKey>,
    pub drawn: Vec<RegionKey>,
    pub hybrid: Vec<RegionKey>,
    pub all: Vec<RegionKey>,
}

impl<K: Kernel> Document<K> {
    /// Selectable profiles of the last history item, only when it is an
    /// evaluated, unsuppressed sketch with no consumer. Rolling back does
    /// not offer an older sketch, and suppressed consumers still count.
    pub fn autoselect_profiles(&self) -> AutoselectProfiles {
        let empty = AutoselectProfiles::default;
        if self.state.marker != self.state.features.len() {
            return empty();
        }
        let Some(entry) = self.state.features.last() else {
            return empty();
        };
        let FeatureDef::Sketch(def) = &entry.def else {
            return empty();
        };
        if entry.suppressed || self.state.used_sketches().contains(&entry.uid) {
            return empty();
        }
        let Some(output) = self.sketch_output(entry.uid) else {
            return empty();
        };
        if output.regions.is_empty() {
            return empty();
        }

        // `reference` persists on unlinked projections too. A fixed drawing
        // is still a drawing: the solver's `fixed` flag is not provenance.
        // Also honor linked records in definitions from earlier producers.
        let projected: BTreeSet<_> = def
            .entities
            .iter()
            .filter(|entity| entity.reference)
            .map(|entity| entity.id)
            .chain(
                def.projections
                    .iter()
                    .flat_map(|projection| projection.entities.iter().map(|entity| entity.uid())),
            )
            .collect();

        let mut groups = AutoselectProfiles {
            eligible: true,
            sketch: Some(entry.uid),
            component: Some(entry.component),
            ..AutoselectProfiles::default()
        };
        for region in &output.regions {
            let mut has_projected = false;
            let mut has_drawn = false;
            // Every boundary participates, including holes. Classify the
            // original curve of each split segment, not its intersecting
            // neighbours or the outer-loop-only region key.
            for segment in region.loops.iter().flat_map(|boundary| &boundary.segments) {
                let is_projected = match &segment.key.curve {
                    CurveId::Curve(uid) => projected.contains(uid),
                    CurveId::Glyph { .. } => false,
                    CurveId::FaceEdge(_) => true,
                };
                has_projected |= is_projected;
                has_drawn |= !is_projected;
            }
            let group = match (has_projected, has_drawn) {
                (true, false) => &mut groups.projected,
                (false, true) => &mut groups.drawn,
                (true, true) => &mut groups.hybrid,
                (false, false) => continue,
            };
            group.push(region.key.clone());
            groups.all.push(region.key.clone());
        }
        if groups.all.is_empty() {
            return empty();
        }
        groups
    }
}
