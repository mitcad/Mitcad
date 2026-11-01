// SPDX-License-Identifier: MIT
//! Scaling a sketch (mitcad#860).
//!
//! - `sketch.scale` ([`SketchEdit::scale`]) scales the whole sketch by a
//!   factor about a point: every point of its own geometry, the radii of
//!   circles and ellipses, the texts' places, heights and line spacings
//!   (a code's module size), the places of the dimensions' values, and the
//!   values of its driving length dimensions, offset distances and
//!   rectangular pattern spacings, so that the sketch keeps its shape and
//!   its constraints. Angles stay. The sketch's origin and axes stay where
//!   they are. Refused for a sketch with projected geometry (it follows
//!   its source) and for a length value that is another parameter or an
//!   expression of one (it is changed there, not here).
//! - The first driving dimension of a sketch can scale it to itself
//!   (`sketch.add_dimension` with `scale_sketch`, the application's
//!   preference "Scale the sketch to its first dimension"): a sketch drawn
//!   out of proportion is first scaled uniformly about the sketch origin so
//!   that the dimension holds, then the dimension is added, in one edit.
//!   Only a length, distance, radius or diameter, not an angle, and only
//!   when the sketch has no other dimension, offset distance or
//!   rectangular pattern spacing, no projected, intersected or linked
//!   geometry, and no fixed entity other than its origin and axes
//!   ([`super::axes`]). Otherwise nothing is scaled.

use std::collections::BTreeSet;

use super::edit::SketchEdit;
use super::geometry::P2;
use super::pattern::PatternKind;
use super::solve::measure_in;
use super::{ConstraintUid, DimensionKind, EntityKind};
use crate::parameters::ParamId;

/// The smallest change of size the first-dimension rule scales for: a
/// factor closer to 1 leaves the sketch as it is.
const SCALE_EPSILON: f64 = 1e-12;

impl SketchEdit<'_> {
    /// Scales the whole sketch by `factor` about `center` (see the module
    /// documentation).
    pub fn scale(&mut self, factor: f64, center: P2) -> Result<(), String> {
        if !(factor.is_finite() && factor > 0.0) {
            return Err(format!(
                "the scale factor must be greater than zero, got {factor}"
            ));
        }
        if !center.iter().all(|v| v.is_finite()) {
            return Err(format!("the center must be finite, got {center:?}"));
        }
        if self.has_projected_geometry() {
            return Err(
                "the sketch has projected geometry, which follows its source and is not scaled"
                    .to_owned(),
            );
        }
        let values = self.length_values()?;
        for id in values {
            let value = self.params.value(id).unwrap_or(0.0);
            self.params
                .set_value(id, value * factor)
                .map_err(|e| e.to_string())?;
        }
        self.scale_geometry(factor, center);
        Ok(())
    }

    /// The value the first-dimension rule measures for a new dimension of
    /// `kind` on the geometry as it is now, when the rule applies to it
    /// (see the module documentation); None when it does not.
    pub fn first_dimension_measure(&self, kind: &DimensionKind) -> Option<f64> {
        if matches!(kind, DimensionKind::Angle { .. })
            || !self.def.dimensions.is_empty()
            || !self.def.offsets.is_empty()
            || self
                .def
                .patterns
                .iter()
                .any(|p| matches!(p.kind, PatternKind::Rectangular { .. }))
            || self.has_projected_geometry()
        {
            return None;
        }
        let origin = super::axes::origin_point(&self.def);
        let other_fixed = self
            .def
            .entities
            .iter()
            .any(|e| e.fixed && e.axis.is_none() && Some(e.id) != origin);
        if other_fixed {
            return None;
        }
        let solved = self.solved().ok()?;
        let measured = measure_in(kind, &solved, &self.def.constraints)?;
        (measured.is_finite() && measured.abs() > 1e-9).then_some(measured)
    }

    /// Scales the sketch about its origin so that the new driving
    /// dimension `id`, which measured `measured` before it was added, holds
    /// (the first-dimension rule); returns the factor, or None when the
    /// sketch already has that size or the factor would not be positive (a
    /// signed distance to the other side).
    pub fn scale_to_dimension(
        &mut self,
        id: ConstraintUid,
        measured: f64,
    ) -> Result<Option<f64>, String> {
        let Some(param) = self
            .def
            .dimensions
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| d.value)
        else {
            return Ok(None);
        };
        let Some(target) = self.params.value(param) else {
            return Ok(None);
        };
        let factor = target / measured;
        if !(factor.is_finite() && factor > 0.0) || (factor - 1.0).abs() <= SCALE_EPSILON {
            return Ok(None);
        }
        self.scale_geometry(factor, [0.0, 0.0]);
        Ok(Some(factor))
    }

    /// Whether the sketch has projected, intersected or linked geometry.
    fn has_projected_geometry(&self) -> bool {
        !self.def.projections.is_empty() || self.def.entities.iter().any(|e| e.reference)
    }

    /// The parameters of the sketch's length values that a scale changes:
    /// driving dimensions other than angles, offset distances and
    /// rectangular pattern spacings. Refused when one is not the sketch's
    /// own plain value.
    fn length_values(&self) -> Result<BTreeSet<ParamId>, String> {
        let mut values: Vec<(String, ParamId)> = Vec::new();
        for d in &self.def.dimensions {
            if let (Some(p), false) = (d.value, matches!(d.kind, DimensionKind::Angle { .. })) {
                values.push((format!("dimension {}", d.id), p));
            }
        }
        for o in &self.def.offsets {
            values.push((format!("offset {}", o.id), o.distance));
        }
        for p in &self.def.patterns {
            if let PatternKind::Rectangular { spacing, .. } = &p.kind {
                for s in spacing {
                    values.push((format!("pattern {}", p.id), *s));
                }
            }
        }
        let mut ids = BTreeSet::new();
        for (what, id) in values {
            let own = self.params.owner(id) == Some(self.owner());
            let plain = self
                .params
                .get(id)
                .is_some_and(|p| p.dependencies().is_empty());
            if !(own && plain) {
                let source = match self.params.get(id) {
                    Some(p) if own => format!("the expression {}", p.expression()),
                    _ => format!("the parameter {}", self.params.name(id)),
                };
                return Err(format!(
                    "{what} takes its value from {source}: change that instead of scaling the \
                     sketch"
                ));
            }
            ids.insert(id);
        }
        Ok(ids)
    }

    /// Moves and sizes the sketch's own geometry, texts and dimension
    /// values' places by `factor` about `center`; the origin, the axes and
    /// projected geometry stay.
    fn scale_geometry(&mut self, factor: f64, center: P2) {
        let map = |p: P2| -> P2 {
            [
                center[0] + factor * (p[0] - center[0]),
                center[1] + factor * (p[1] - center[1]),
            ]
        };
        let origin = super::axes::origin_point(&self.def);
        for e in &mut self.def.entities {
            if e.reference || e.axis.is_some() || Some(e.id) == origin {
                continue;
            }
            match &mut e.kind {
                EntityKind::Point { at } => *at = map(*at),
                EntityKind::Circle { radius, .. } => *radius *= factor,
                EntityKind::Ellipse { minor_radius, .. }
                | EntityKind::EllipticalArc { minor_radius, .. } => *minor_radius *= factor,
                EntityKind::Line { .. }
                | EntityKind::Arc { .. }
                | EntityKind::Spline { .. }
                | EntityKind::FittedSpline { .. } => {}
            }
        }
        for t in &mut self.def.texts {
            t.at = map(t.at);
            t.height *= factor;
            t.line_spacing *= factor;
        }
        for d in &mut self.def.dimensions {
            if let Some(at) = d.text {
                d.text = Some(map(at));
            }
        }
    }
}
