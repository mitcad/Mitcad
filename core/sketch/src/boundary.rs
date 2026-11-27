// SPDX-License-Identifier: MIT
//! Profile Boundary (mitcad#2551): curves of the face's outline that the
//! sketch got when it was made ([`crate::Projection::automatic`]) chosen
//! to bound profiles as the user's own curves do. A boss's circle on the
//! face then makes a hole in an area the user's curves close around it (an
//! inset of the outline), while the circle stays linked to the face's
//! edge ([`crate::Projection::boundaries`]).

use std::collections::BTreeSet;

use super::edit::SketchEdit;
use crate::ids::EntityUid;

impl SketchEdit<'_> {
    /// Makes `curves` bound profiles as own curves (`on`) or go back to
    /// automatic boundaries (not `on`). Each must be a curve of an
    /// automatic outline; a curve already so is left as it is.
    pub fn set_profile_boundary(&mut self, curves: &[EntityUid], on: bool) -> Result<(), String> {
        if curves.is_empty() {
            return Err("\"curves\" is empty".to_owned());
        }
        for id in curves {
            self.curve(*id)?;
            let automatic =
                self.def.projections.iter().any(|p| {
                    p.automatic && p.entities.iter().any(|r| !r.is_point() && r.uid() == *id)
                });
            if !automatic {
                return Err(crate::said(
                    "only curves of the face's outline that the sketch got when it was made \
                     can be chosen to bound profiles",
                    format!(
                        "c{} is not a curve of the sketch's automatic face outline",
                        id.0
                    ),
                ));
            }
        }
        let chosen: BTreeSet<EntityUid> = curves.iter().copied().collect();
        for projection in self.def.projections.iter_mut().filter(|p| p.automatic) {
            let listed: Vec<EntityUid> = projection
                .entities
                .iter()
                .filter(|r| !r.is_point())
                .map(|r| r.uid())
                .filter(|id| chosen.contains(id))
                .collect();
            if on {
                for id in listed {
                    if !projection.boundaries.contains(&id) {
                        projection.boundaries.push(id);
                    }
                }
                projection.boundaries.sort_unstable();
            } else {
                projection.boundaries.retain(|id| !listed.contains(id));
            }
        }
        Ok(())
    }
}
