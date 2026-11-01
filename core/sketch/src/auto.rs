// SPDX-License-Identifier: MIT
//! Constraints a drawing tool adds automatically (mitcad#1024).
//!
//! A sketch tool adds the constraints the geometry it has just drawn
//! already satisfies (a line drawn level gets `horizontal`, a line ending
//! tangent on a circle `tangent`, an end at a circle's quadrant the
//! alignment with its centre), but only when that is safe:
//! `sketch.add_constraint` with `"auto": true` ([`SketchEdit::add_constraint_if_safe`])
//! adds the constraint only when
//!
//! - the others do not imply it, in full or in part: the redundant count
//!   does not rise, and no older constraint is removed in its favour (as a
//!   plain `sketch.add_constraint` does, mitcad#287);
//! - the sketch still solves with it (no conflict);
//! - the solve moves no point and changes no radius by more than
//!   [`AUTO_CONSTRAINT_TOLERANCE`]: the geometry stays exactly as drawn.
//!
//! Otherwise the command is refused, nothing changes, and the message says
//! why after `auto constraint k<n>: ` (`redundant`, `conflict: …`,
//! `moves p3 by 0.5 mm`), for the tool's log.

use super::edit::SketchEdit;
use super::geometry::dist;
use super::{ConstraintKind, ConstraintUid, Ref};

/// How far (mm) an automatic constraint may move a point or change a
/// radius in the solve: the geometry it is added to must satisfy it
/// already.
pub const AUTO_CONSTRAINT_TOLERANCE: f64 = 1e-9;

impl SketchEdit<'_> {
    /// Adds `kind` only when it is safe (see the module documentation);
    /// refused with the reason otherwise.
    pub fn add_constraint_if_safe(
        &mut self,
        kind: ConstraintKind,
    ) -> Result<ConstraintUid, String> {
        let before = self
            .solved()
            .map_err(|e| format!("auto constraint: the sketch does not solve: {e}"))?;
        // A coincident of two curve ends would join them into one point
        // (mitcad#260): not something to do on the side.
        if self.coincident_merge(&kind).is_some() {
            return Err(
                "auto constraint: a coincident of two curve ends joins them; add it as a \
                 plain constraint"
                    .to_owned(),
            );
        }
        let id = self.add_constraint(kind);
        let refuse = |reason: String| Err(format!("auto constraint {id}: {reason}"));
        let after = match self.solved() {
            Ok(after) => after,
            Err(e) => return refuse(format!("conflict: {e}")),
        };
        if after.status.conflicts.len() > before.status.conflicts.len() {
            return refuse("conflict".to_owned());
        }
        if after.status.redundant.len() > before.status.redundant.len() {
            return refuse("redundant".to_owned());
        }
        let mut moved: Option<(String, f64)> = None;
        let mut note = |what: String, by: f64| {
            if by > AUTO_CONSTRAINT_TOLERANCE && moved.as_ref().is_none_or(|(_, m)| by > *m) {
                moved = Some((what, by));
            }
        };
        for (point, at) in &before.points {
            if let Some(now) = after.points.get(point) {
                note(Ref::Point(*point).to_string(), dist(*at, *now));
            }
        }
        for (curve, radius) in &before.radii {
            if let Some(now) = after.radii.get(curve) {
                note(Ref::Curve(*curve).to_string(), (radius - now).abs());
            }
        }
        if let Some((what, by)) = moved {
            return refuse(format!("moves {what} by {by:.3e} mm"));
        }
        Ok(id)
    }
}
