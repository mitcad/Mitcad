// SPDX-License-Identifier: MIT
//! Definition-state occurrence allocation for Pattern at Points. Recompute
//! controls availability; edits reuse IDs by source path and target reference.

use super::{DocState, ModelError};
use crate::features::point_pattern::{PatternInstance, PointPatternDef};
use crate::features::{FeatureDef, PatternObjects};
use crate::ids::{ComponentUid, FeatureUid};
use crate::transform::Transform;

impl DocState {
    pub(crate) fn sync_point_pattern(&mut self, uid: FeatureUid) -> Result<(), ModelError> {
        let entry = self.entry(uid).expect("inserted feature").clone();
        let FeatureDef::PatternAtPoints(def) = &entry.def else {
            return Ok(());
        };
        let mut desired = Vec::new();
        if let PatternObjects::Components { occurrences } = &def.objects {
            for source in occurrences {
                let Ok(component) =
                    crate::joints::path_component(&self.assembly, ComponentUid::ROOT, &source.0)
                else {
                    continue;
                };
                for target in &def.targets {
                    let key = PatternInstance {
                        feature: uid,
                        source: source.clone(),
                        target: target.clone(),
                    };
                    let existing = self
                        .assembly
                        .occurrences
                        .iter()
                        .find(|o| o.pattern.as_ref() == Some(&key))
                        .map(|o| o.uid);
                    let instance = existing.unwrap_or_else(|| {
                        self.assembly
                            .place(component, entry.component, Transform::IDENTITY)
                    });
                    let occurrence = self.assembly.occurrence_mut(instance).expect("placed");
                    occurrence.component = component;
                    occurrence.parent = entry.component;
                    occurrence.pattern = Some(key);
                    desired.push(instance);
                }
            }
        }
        self.assembly.occurrences.retain(|o| {
            o.pattern
                .as_ref()
                .is_none_or(|p| p.feature != uid || desired.contains(&o.uid))
        });
        Ok(())
    }

    /// The occurrences Pattern at Points `uid` made keep their keys when
    /// its paths are re-pointed (mitcad#2466: parts it repeats or takes
    /// points on went one level down), so that the same copies stay at
    /// the same points: each instance's source and target become those at
    /// the same places of `after` as they were in `before`.
    pub(crate) fn follow_point_pattern(
        &mut self,
        uid: FeatureUid,
        before: &PointPatternDef,
        after: &PointPatternDef,
    ) {
        let sources = |def: &PointPatternDef| match &def.objects {
            PatternObjects::Components { occurrences } => occurrences.clone(),
            _ => Vec::new(),
        };
        let (old_sources, new_sources) = (sources(before), sources(after));
        if old_sources.len() != new_sources.len() || before.targets.len() != after.targets.len() {
            return;
        }
        for o in &mut self.assembly.occurrences {
            let Some(key) = o.pattern.as_mut().filter(|p| p.feature == uid) else {
                continue;
            };
            if let Some(i) = old_sources.iter().position(|s| *s == key.source) {
                key.source = new_sources[i].clone();
            }
            if let Some(i) = before.targets.iter().position(|t| *t == key.target) {
                key.target = after.targets[i].clone();
            }
        }
    }
}
