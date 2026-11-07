// SPDX-License-Identifier: MIT
//! A part on a side placed from the middle of the free stretch between the
//! profiles standing on it (mitcad#1401; the planning is the BUILD crate's
//! `gap.rs`): its joint's slide is an expression of those profiles'
//! slides. When one of them goes (deleted, or its joint taken away), the
//! part is bound again to the end of the side nearer where it stands, at
//! its current place, so that a delete neither fails nor leaves its slide
//! on a parameter nothing places any more.

use super::{BuildMeta, SideFrom, round};
use crate::document::DocState;
use crate::features::FeatureDef;
use crate::ids::{ComponentUid, FeatureUid};
use crate::parameters::ParamId;

/// ` + k` or ` - k` after an expression; nothing for 0.
fn plus(k: f64) -> String {
    if k.abs() < 1e-9 {
        String::new()
    } else if k > 0.0 {
        format!(" + {}", round(k))
    } else {
        format!(" - {}", round(-k))
    }
}

impl DocState {
    /// Binds every part centred in a free stretch whose slide uses a
    /// parameter of a feature no longer in the timeline to the nearer end
    /// of its side, in place: from end A its slide as it is now, from end
    /// B the side's length less its distance from there. Called while
    /// features are removed, before their parameters are released, so
    /// that the gone profile's slide parameter goes with it.
    pub(crate) fn fall_back_gaps(&mut self) {
        let parts: Vec<(ComponentUid, FeatureUid)> = self
            .assembly
            .components
            .iter()
            .filter_map(|c| {
                let record = c.build.as_ref()?;
                if record.from != Some(SideFrom::Gap) {
                    return None;
                }
                Some((c.uid, record.joint?))
            })
            .collect();
        for (component, joint) in parts {
            let Some(entry) = self.entry(joint) else {
                continue;
            };
            let FeatureDef::Joint(j) = &entry.def else {
                continue;
            };
            let Some(&slide) = j.position.get(&j.slide_axis.motion()) else {
                continue;
            };
            let gone = self.parameters.get(slide).is_some_and(|p| {
                p.dependencies().iter().any(|d| {
                    self.parameters
                        .owner(*d)
                        .is_some_and(|f| self.position(f).is_none())
                })
            });
            if !gone {
                continue;
            }
            let Some(tx) = self.parameters.value(slide) else {
                continue;
            };
            let length =
                crate::joints::path_component(&self.assembly, entry.component, &j.b.occurrence.0)
                    .ok()
                    .and_then(|target| self.build_length(target));
            let from = match length {
                Some((id, l)) if tx > l / 2.0 => {
                    let expression = format!("{}{}", self.parameters.name(id), plus(tx - l));
                    if self.parameters.set_expression(slide, &expression).is_ok() {
                        SideFrom::B
                    } else {
                        let _ = self.parameters.set_value(slide, tx);
                        SideFrom::A
                    }
                }
                _ => {
                    let _ = self.parameters.set_value(slide, tx);
                    SideFrom::A
                }
            };
            if let Some(record) = self
                .assembly
                .component_mut(component)
                .and_then(|c| c.build.as_mut())
            {
                record.from = Some(from);
                record.gap = None;
            }
        }
    }

    /// A BUILD part's length parameter and its value (as
    /// `Document::part_info` reads it).
    fn build_length(&self, component: ComponentUid) -> Option<(ParamId, f64)> {
        let record = self.assembly.component(component)?.build.as_ref()?;
        let FeatureDef::DerivedPart(def) = &self.entry(record.part?)?.def else {
            return None;
        };
        let source = self.library_sources.get(record.source?)?;
        let meta = BuildMeta::read_row(source.build.as_ref()?, def.config.as_deref()).ok()?;
        let id = *def.parameters.get(&meta.length.as_ref()?.parameter)?;
        Some((id, self.parameters.value(id)?))
    }
}
