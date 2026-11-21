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
//! - Sketch Scale (mitcad#2109, [`SketchEdit::scale_selection`]) scales
//!   selected points, curves and texts, with the points their curves use,
//!   by a factor about a point. The constraints among them stay; their
//!   driving length dimensions, offset distances and rectangular pattern
//!   spacings are multiplied by the factor ([`ScaleDimensions::Scale`]) or
//!   keep their values, the scaled geometry solved with them
//!   ([`ScaleDimensions::Keep`]). A constraint or a dimension that ties
//!   the selection to the rest stays while it still holds after the scale,
//!   and is removed otherwise, counted in the edit's notes; nothing outside
//!   the selection moves. With Create Copy the copies are scaled and the
//!   selection stays.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::conflict;
use super::edit::SketchEdit;
use super::geometry::{P2, dist};
use super::modify::Map2;
use super::pattern::PatternKind;
use super::solve::{Extra, Goal, measure_in, unmet};
use super::{ConstraintUid, DimensionKind, EntityKind, Ref};
use crate::ids::EntityUid;
use crate::parameters::ParamId;

/// The smallest change of size the first-dimension rule scales for: a
/// factor closer to 1 leaves the sketch as it is.
const SCALE_EPSILON: f64 = 1e-12;

/// How much a size of the selection may change, relative to it, and still
/// count as kept: a scale whose kept dimensions bring every size back did
/// not scale (mitcad#2109).
const SIZE_EPSILON: f64 = 1e-6;

/// What Sketch Scale does with the driving length dimensions, offset
/// distances and rectangular pattern spacings among the scaled geometry
/// (mitcad#2109): `sketch.scale`'s `dimensions`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleDimensions {
    /// Multiplied by the factor, so that the geometry keeps its shape.
    #[default]
    Scale,
    /// Kept; the scaled geometry is solved with them.
    Keep,
}

/// A refused factor or center.
fn check_scale(factor: f64, center: P2) -> Result<(), String> {
    if !(factor.is_finite() && factor > 0.0) {
        return Err(format!(
            "the scale factor must be greater than zero, got {factor}"
        ));
    }
    if !center.iter().all(|v| v.is_finite()) {
        return Err(format!("the center must be finite, got {center:?}"));
    }
    Ok(())
}

/// Whether a curve has a size of its own besides its points: a circle's
/// radius, an ellipse's minor radius.
fn has_size(kind: &EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Circle { .. } | EntityKind::Ellipse { .. } | EntityKind::EllipticalArc { .. }
    )
}

/// What a selection scale changes (mitcad#2109).
#[derive(Debug, Default)]
struct Scaled {
    /// The points that move: the selected points and the points of the
    /// selected curves, but the origin, the axes and projected points.
    points: BTreeSet<EntityUid>,
    /// The curves scaled whole: selected ones whose points all move, and
    /// the curves without a size of their own whose points all move.
    curves: BTreeSet<EntityUid>,
    /// The selected circles and ellipses, whose radii scale.
    sized: BTreeSet<EntityUid>,
    /// The curves whose geometry changes: the selected ones and those
    /// with a moving point.
    touched: BTreeSet<EntityUid>,
}

impl Scaled {
    /// Whether a constraint's or a dimension's reference is scaled whole.
    fn inside(&self, r: &Ref) -> bool {
        match r {
            Ref::Point(p) => self.points.contains(p),
            Ref::Curve(c) => self.curves.contains(c),
        }
    }

    /// Whether the scale changes a reference's geometry.
    fn touches(&self, r: &Ref) -> bool {
        match r {
            Ref::Point(p) => self.points.contains(p),
            Ref::Curve(c) => self.touched.contains(c),
        }
    }
}

impl SketchEdit<'_> {
    /// Scales the whole sketch by `factor` about `center` (see the module
    /// documentation).
    pub fn scale(&mut self, factor: f64, center: P2) -> Result<(), String> {
        check_scale(factor, center)?;
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

// Sketch Scale of a selection (mitcad#2109).
impl SketchEdit<'_> {
    /// Scales `entities` (points and curves) and `texts` by `factor` about
    /// `center` (see the module documentation): `dimensions` says what the
    /// driving lengths among them do; with `copy`, their copies are scaled
    /// and they stay. Returns the copies made (`made`): of each of
    /// `entities` that has one, then of each text.
    pub fn scale_selection(
        &mut self,
        entities: &[Ref],
        texts: &[EntityUid],
        factor: f64,
        center: P2,
        dimensions: ScaleDimensions,
        copy: bool,
    ) -> Result<Vec<String>, String> {
        check_scale(factor, center)?;
        let (objects, texts) = self.scale_objects(entities, texts)?;
        if copy {
            return self.copy_scaled(entities, &objects, &texts, factor, center);
        }
        let scaled = self.scaled_set(&objects);
        let (offsets, patterns) = self.whole_parts(&scaled, center)?;

        // The dimensions inside the selection, the values that scale, and
        // the constraints and dimensions that tie the selection to the rest.
        let mut inside_dimensions: BTreeSet<ConstraintUid> = BTreeSet::new();
        let mut values: Vec<(ConstraintUid, ParamId)> = Vec::new();
        let mut linking: BTreeSet<ConstraintUid> = BTreeSet::new();
        for d in &self.def.dimensions {
            let refs = match d.kind {
                DimensionKind::Spacing { constraint } => self
                    .def
                    .constraints
                    .iter()
                    .find(|c| c.id == constraint)
                    .map(|c| c.kind.refs())
                    .unwrap_or_default(),
                _ => d.kind.refs(),
            };
            if refs.is_empty() {
                continue;
            }
            if refs.iter().all(|r| scaled.inside(r)) {
                inside_dimensions.insert(d.id);
                if let (Some(p), false) = (d.value, matches!(d.kind, DimensionKind::Angle { .. })) {
                    values.push((d.id, p));
                }
            } else if d.active && d.value.is_some() && refs.iter().any(|r| scaled.touches(r)) {
                linking.insert(d.id);
            }
        }
        for c in &self.def.constraints {
            let refs = c.kind.refs();
            if c.active
                && !refs.iter().all(|r| scaled.inside(r))
                && refs.iter().any(|r| scaled.touches(r))
            {
                linking.insert(c.id);
            }
        }
        for o in self.def.offsets.iter().filter(|o| offsets.contains(&o.id)) {
            values.push((o.id, o.distance));
        }
        for p in self
            .def
            .patterns
            .iter()
            .filter(|p| patterns.contains(&p.id))
        {
            if let PatternKind::Rectangular { spacing, .. } = &p.kind {
                values.extend(spacing.iter().map(|s| (p.id, *s)));
            }
        }
        if dimensions == ScaleDimensions::Scale {
            self.check_scaled_values(&values)?;
        }

        let before = self.unmet_now();
        let positions: BTreeMap<EntityUid, P2> = scaled
            .points
            .iter()
            .filter_map(|p| Some((*p, self.at(*p).ok()?)))
            .collect();
        let radii = self.radii_of(&scaled.sized);
        self.scale_parts(&scaled, &texts, &inside_dimensions, factor, center);
        if dimensions == ScaleDimensions::Scale {
            let params: BTreeSet<ParamId> = values.iter().map(|v| v.1).collect();
            for id in params {
                let value = self.params.value(id).unwrap_or(0.0);
                self.params
                    .set_value(id, value * factor)
                    .map_err(|e| e.to_string())?;
            }
        }

        // What tied the selection to the rest and no longer holds.
        let after = self.unmet_now();
        let (broken, ties): (BTreeSet<ConstraintUid>, BTreeSet<ConstraintUid>) = linking
            .into_iter()
            .partition(|k| after.contains(k) && !before.contains(k));
        if !broken.is_empty() {
            self.note_untied(&broken);
            self.drop_constraints(&broken);
        }

        if dimensions == ScaleDimensions::Keep && !values.is_empty() {
            let kept: Vec<ConstraintUid> = values.iter().map(|v| v.0).collect();
            self.solve_kept(&scaled, &kept, &ties, &positions, &radii, factor)?;
        }
        Ok(Vec::new())
    }

    /// The objects of a selection scale: the entities, but the origin point
    /// and the axes, which stay; and the rectangles of the texts in frames,
    /// which the texts follow. Refused for projected geometry, which
    /// follows its source, and for a text on a path without its curve.
    fn scale_objects(
        &self,
        entities: &[Ref],
        texts: &[EntityUid],
    ) -> Result<(Vec<Ref>, Vec<EntityUid>), String> {
        let origin = super::axes::origin_point(&self.def);
        let projected: BTreeSet<EntityUid> = self
            .def
            .projections
            .iter()
            .flat_map(|p| p.entities.iter().map(|r| r.uid()))
            .collect();
        let mut objects: Vec<Ref> = Vec::new();
        for r in entities {
            let e = self
                .def
                .entity(r.uid())
                .ok_or_else(|| format!("{r} does not exist"))?;
            if e.is_point() != r.is_point() {
                return Err(format!("{r} is a {}", e.kind.type_name()));
            }
            if e.axis.is_some() || Some(e.id) == origin {
                continue;
            }
            if e.reference || projected.contains(&e.id) {
                return Err(crate::said(
                    "projected geometry follows its source and is not scaled: leave it out of \
                     the selection",
                    format!("{r} is projected geometry: it follows its source and is not scaled"),
                ));
            }
            if !objects.contains(r) {
                objects.push(*r);
            }
        }
        let mut chosen: Vec<EntityUid> = Vec::new();
        for &t in texts {
            let text = self
                .def
                .texts
                .iter()
                .find(|x| x.id == t)
                .ok_or_else(|| format!("text t{} does not exist", t.0))?;
            match &text.path {
                Some(path) if !objects.contains(&Ref::Curve(path.curve)) => {
                    return Err(crate::said(
                        "a text on a path follows its curve: scale the curve with it",
                        format!(
                            "t{} runs along {}: scale the curve with it",
                            t.0,
                            Ref::Curve(path.curve)
                        ),
                    ));
                }
                Some(_) => {}
                None => {
                    for r in self.frame_rectangle(&text.frame) {
                        if !objects.contains(&r) {
                            objects.push(r);
                        }
                    }
                }
            }
            if !chosen.contains(&t) {
                chosen.push(t);
            }
        }
        if objects.is_empty() && chosen.is_empty() {
            return Err(
                "nothing to scale: the sketch's origin point and axes stay where they are"
                    .to_owned(),
            );
        }
        Ok((objects, chosen))
    }

    /// What scaling `objects` changes.
    fn scaled_set(&self, objects: &[Ref]) -> Scaled {
        let origin = super::axes::origin_point(&self.def);
        let stays = |id: EntityUid| {
            Some(id) == origin
                || self
                    .def
                    .entity(id)
                    .is_none_or(|e| e.reference || e.axis.is_some())
        };
        let mut scaled = Scaled::default();
        let mut selected: BTreeSet<EntityUid> = BTreeSet::new();
        for r in objects {
            let Some(e) = self.def.entity(r.uid()) else {
                continue;
            };
            if e.is_point() {
                if !stays(e.id) {
                    scaled.points.insert(e.id);
                }
                continue;
            }
            selected.insert(e.id);
            scaled
                .points
                .extend(e.kind.points().into_iter().filter(|p| !stays(*p)));
            if has_size(&e.kind) {
                scaled.sized.insert(e.id);
            }
        }
        for e in &self.def.entities {
            if e.is_point() || e.reference || e.axis.is_some() {
                continue;
            }
            let points = e.kind.points();
            let all = !points.is_empty() && points.iter().all(|p| scaled.points.contains(p));
            if all && (selected.contains(&e.id) || !has_size(&e.kind)) {
                scaled.curves.insert(e.id);
            }
            if selected.contains(&e.id) || points.iter().any(|p| scaled.points.contains(p)) {
                scaled.touched.insert(e.id);
            }
        }
        scaled
    }

    /// The offsets and the patterns the selection scales whole: (offsets,
    /// patterns). A circular pattern's center that does not move counts
    /// as scaled when it is the center of the scale. Refused for an offset
    /// or a pattern the selection scales only in part: what is left out
    /// would no longer follow it.
    fn whole_parts(
        &self,
        scaled: &Scaled,
        center: P2,
    ) -> Result<(BTreeSet<ConstraintUid>, BTreeSet<ConstraintUid>), String> {
        let at_center = |p: EntityUid| {
            self.def.entity(p).is_some_and(
                |e| matches!(e.kind, EntityKind::Point { at } if dist(at, center) <= 1e-9),
            )
        };
        let mut offsets = BTreeSet::new();
        let mut patterns = BTreeSet::new();
        let mut partly: Vec<ConstraintUid> = Vec::new();
        for o in &self.def.offsets {
            let members: Vec<Ref> = o
                .curves
                .iter()
                .chain(&o.results)
                .map(|c| Ref::Curve(*c))
                .collect();
            if members.iter().all(|r| scaled.inside(r)) {
                offsets.insert(o.id);
            } else if members.iter().any(|r| scaled.touches(r)) {
                partly.push(o.id);
            }
        }
        for p in &self.def.patterns {
            let mut members: Vec<Ref> = p.entities.clone();
            members.extend(
                p.copy_ids()
                    .into_iter()
                    .filter_map(|id| self.def.entity(id).map(super::Entity::as_ref)),
            );
            let (center_inside, center_touched) = match p.kind {
                PatternKind::Circular { center: c, .. } => (
                    scaled.points.contains(&c) || at_center(c),
                    scaled.points.contains(&c),
                ),
                PatternKind::Rectangular { .. } => (true, false),
            };
            if center_inside && members.iter().all(|r| scaled.inside(r)) {
                patterns.insert(p.id);
            } else if center_touched || members.iter().any(|r| scaled.touches(r)) {
                partly.push(p.id);
            }
        }
        if !partly.is_empty() {
            let named: Vec<String> = partly.iter().map(ToString::to_string).collect();
            return Err(format!(
                "{}{}",
                crate::said(
                    "a pattern or an offset is only partly in the selection: select all of it, \
                     or none of it",
                    format!(
                        "{} only partly in the selection: select all of it, or none of it",
                        named.join(", ")
                    ),
                ),
                conflict::trailer(&partly)
            ));
        }
        Ok((offsets, patterns))
    }

    /// Refuses values to be multiplied that are not the sketch's own plain
    /// numbers: a parameter or an expression is changed there, not here.
    /// The refusal lists their dimensions, offsets or patterns for the
    /// application to show.
    fn check_scaled_values(&self, values: &[(ConstraintUid, ParamId)]) -> Result<(), String> {
        let mut refused: Vec<ConstraintUid> = Vec::new();
        let mut named: Vec<String> = Vec::new();
        for &(k, id) in values {
            let own = self.params.owner(id) == Some(self.owner());
            let plain = self
                .params
                .get(id)
                .is_some_and(|p| p.dependencies().is_empty());
            if own && plain {
                continue;
            }
            if !refused.contains(&k) {
                refused.push(k);
            }
            named.push(match self.params.get(id) {
                Some(p) if own => format!("{k} takes the expression {}", p.expression()),
                _ => format!("{k} takes the parameter {}", self.params.name(id)),
            });
        }
        if refused.is_empty() {
            return Ok(());
        }
        let dimension = refused
            .iter()
            .any(|k| self.def.dimensions.iter().any(|d| d.id == *k));
        let said = if dimension {
            "a dimension of the selection takes its value from a parameter: keep the \
             dimensions' values, or change the parameter instead"
        } else {
            "an offset distance or a pattern spacing of the selection takes its value from a \
             parameter: keep the values, or change the parameter instead"
        };
        Err(format!(
            "{}{}",
            crate::said(
                said,
                format!(
                    "{}: keep the values, or change the parameter instead",
                    named.join(", ")
                ),
            ),
            conflict::trailer(&refused)
        ))
    }

    /// The constraints and the driving dimensions the geometry does not
    /// meet as it is now, without solving.
    fn unmet_now(&self) -> BTreeSet<ConstraintUid> {
        let params = &*self.params;
        let Ok(inputs) = self
            .def
            .dimension_inputs(&mut |id| params.value(id).ok_or_else(String::new), &|id| {
                params.name(id)
            })
        else {
            return BTreeSet::new();
        };
        unmet(
            &self.def.entities,
            &self.def.constraints,
            &inputs,
            &Extra::default(),
        )
        .into_iter()
        .collect()
    }

    /// The radii of circles and the minor radii of ellipses.
    fn radii_of(&self, curves: &BTreeSet<EntityUid>) -> BTreeMap<EntityUid, f64> {
        curves
            .iter()
            .filter_map(|c| match self.def.entity(*c)?.kind {
                EntityKind::Circle { radius, .. } => Some((*c, radius)),
                EntityKind::Ellipse { minor_radius, .. }
                | EntityKind::EllipticalArc { minor_radius, .. } => Some((*c, minor_radius)),
                _ => None,
            })
            .collect()
    }

    /// Moves and sizes what `scaled` names, the texts and the value places
    /// of `dimensions` by `factor` about `center`.
    fn scale_parts(
        &mut self,
        scaled: &Scaled,
        texts: &[EntityUid],
        dimensions: &BTreeSet<ConstraintUid>,
        factor: f64,
        center: P2,
    ) {
        let map = |p: P2| -> P2 {
            [
                center[0] + factor * (p[0] - center[0]),
                center[1] + factor * (p[1] - center[1]),
            ]
        };
        for e in &mut self.def.entities {
            if scaled.points.contains(&e.id) {
                if let EntityKind::Point { at } = &mut e.kind {
                    *at = map(*at);
                }
            } else if scaled.sized.contains(&e.id) {
                match &mut e.kind {
                    EntityKind::Circle { radius, .. } => *radius *= factor,
                    EntityKind::Ellipse { minor_radius, .. }
                    | EntityKind::EllipticalArc { minor_radius, .. } => *minor_radius *= factor,
                    _ => {}
                }
            }
        }
        // A text in a frame takes its frame's corner when the edit
        // finishes; one on a path runs along its curve.
        for t in self.def.texts.iter_mut().filter(|t| texts.contains(&t.id)) {
            if t.path.is_none() {
                t.at = map(t.at);
            }
            t.height *= factor;
            t.line_spacing *= factor;
        }
        for d in self
            .def
            .dimensions
            .iter_mut()
            .filter(|d| dimensions.contains(&d.id))
        {
            if let Some(at) = d.text {
                d.text = Some(map(at));
            }
        }
    }

    /// Says in the edit's notes how many constraints and dimensions that
    /// tied the scaled geometry to the rest were removed.
    fn note_untied(&mut self, removed: &BTreeSet<ConstraintUid>) {
        let dimensions = removed
            .iter()
            .filter(|k| self.def.dimensions.iter().any(|d| d.id == **k))
            .count();
        let constraints = removed.len() - dimensions;
        // One whole message per case, the counts as numbers, so that each
        // translates without English words put into it (docs/translations.md).
        let message = match (constraints, dimensions) {
            (0, d) => {
                format!("Dimensions that tied the scaled geometry to the rest were removed: {d}")
            }
            (c, 0) => {
                format!("Constraints that tied the scaled geometry to the rest were removed: {c}")
            }
            (c, d) => format!(
                "Constraints and dimensions that tied the scaled geometry to the rest were removed: {c} and {d}"
            ),
        };
        let named: Vec<String> = removed.iter().map(ToString::to_string).collect();
        self.report.notes.push(crate::said(
            message,
            format!(
                "{} tied the scaled geometry to the rest and no longer held: removed",
                named.join(", ")
            ),
        ));
    }

    /// Solves the scaled geometry with the values of `kept` kept: its
    /// points and radii pulled toward where the scale put them, and the
    /// geometry outside the selection that `ties` (the constraints and
    /// dimensions to the rest that still hold) refer to held where it is,
    /// so that nothing outside the selection moves. Refused when the values
    /// cannot hold, or when they bring every size of the selection back
    /// (`positions` and `radii` before the scale): then nothing would be
    /// scaled.
    fn solve_kept(
        &mut self,
        scaled: &Scaled,
        kept: &[ConstraintUid],
        ties: &BTreeSet<ConstraintUid>,
        positions: &BTreeMap<EntityUid, P2>,
        radii: &BTreeMap<EntityUid, f64>,
        factor: f64,
    ) -> Result<(), String> {
        let mut tied: Vec<Ref> = Vec::new();
        for c in self.def.constraints.iter().filter(|c| ties.contains(&c.id)) {
            tied.extend(c.kind.refs());
        }
        for d in self.def.dimensions.iter().filter(|d| ties.contains(&d.id)) {
            tied.extend(d.kind.refs());
        }
        let mut held: BTreeSet<EntityUid> = BTreeSet::new();
        for r in tied {
            let Some(e) = self.def.entity(r.uid()) else {
                continue;
            };
            let points = e.kind.points();
            // A circle's or an ellipse's own size, when none of its points
            // moves.
            if e.is_point()
                || (has_size(&e.kind)
                    && !scaled.sized.contains(&e.id)
                    && !points.iter().any(|p| scaled.points.contains(p)))
            {
                held.insert(e.id);
            }
            held.extend(points);
        }
        let held: Vec<EntityUid> = held
            .into_iter()
            .filter(|id| !scaled.points.contains(id))
            .filter(|id| {
                self.def
                    .entity(*id)
                    .is_some_and(|e| !e.fixed && !e.reference && e.axis.is_none())
            })
            .collect();
        let mut goals: Vec<Goal> = scaled
            .points
            .iter()
            .filter_map(|p| Some(Goal::Point(*p, self.at(*p).ok()?)))
            .collect();
        for (c, r) in self.radii_of(&scaled.sized) {
            if matches!(
                self.def.entity(c).map(|e| &e.kind),
                Some(EntityKind::Circle { .. })
            ) {
                goals.push(Goal::Radius(c, r));
            }
        }
        let hold = |def: &mut super::def::SketchDef, on: bool| {
            for id in &held {
                if let Some(e) = def.entity_mut(*id) {
                    e.fixed = on;
                }
            }
        };
        hold(&mut self.def, true);
        let params = &*self.params;
        let solved = self.def.solve_goals(
            &mut |id| {
                params
                    .value(id)
                    .ok_or_else(|| format!("parameter {} does not exist", params.name(id)))
            },
            &|id| params.name(id),
            &goals,
        );
        hold(&mut self.def, false);
        let solved = solved.map_err(|e| {
            format!(
                "{}{}",
                crate::said(
                    "the dimensions kept at their values can't hold on the scaled geometry: \
                     scale them with the geometry instead",
                    format!(
                        "the kept values can't hold on the scaled geometry: {}",
                        e.message
                    ),
                ),
                conflict::trailer(kept)
            )
        })?;
        solved.store(&mut self.def.entities);

        // Every size as it was: the kept values undid the scale.
        let moved: Vec<(P2, P2)> = positions
            .iter()
            .filter_map(|(p, before)| Some((*before, self.at(*p).ok()?)))
            .collect();
        let same =
            |before: f64, after: f64| (after - before).abs() <= SIZE_EPSILON * before.max(1.0);
        let mut sizes = 0;
        let mut kept_all = true;
        for (i, a) in moved.iter().enumerate() {
            for b in &moved[i + 1..] {
                let before = dist(a.0, b.0);
                if before <= 1e-9 {
                    continue;
                }
                sizes += 1;
                kept_all &= same(before, dist(a.1, b.1));
            }
        }
        let now = self.radii_of(&radii.keys().copied().collect());
        for (c, before) in radii {
            if let Some(after) = now.get(c) {
                sizes += 1;
                kept_all &= same(*before, *after);
            }
        }
        if sizes > 0 && kept_all && (factor - 1.0).abs() > SCALE_EPSILON {
            return Err(format!(
                "{}{}",
                crate::said(
                    "the dimensions kept at their values fix the size of the selection, so it \
                     can't be scaled with them: scale them with the geometry instead",
                    "the kept values fix every size of the selection, so it can't be scaled \
                     with them: scale them with the geometry instead",
                ),
                conflict::trailer(kept)
            ));
        }
        Ok(())
    }

    /// Sketch Scale's Create Copy: copies of `objects` and `texts` scaled
    /// by `factor` about `center`, with the constraints among them; the
    /// originals stay. Returns the copies of `entities`, then of the texts.
    fn copy_scaled(
        &mut self,
        entities: &[Ref],
        objects: &[Ref],
        texts: &[EntityUid],
        factor: f64,
        center: P2,
    ) -> Result<Vec<String>, String> {
        let map = Map2::scaling(center, factor);
        let copies = if objects.is_empty() {
            BTreeMap::new()
        } else {
            self.copy_entities(objects, &map, &BTreeSet::new(), true)?
        };
        let mut made: Vec<String> = entities
            .iter()
            .filter_map(|r| {
                let c = *copies.get(&r.uid())?;
                Some(
                    match r {
                        Ref::Point(_) => Ref::Point(c),
                        Ref::Curve(_) => Ref::Curve(c),
                    }
                    .to_string(),
                )
            })
            .collect();
        for t in self.copy_texts(texts, &map, &copies)? {
            if let Some(text) = self.def.texts.iter_mut().find(|x| x.id == t) {
                text.height *= factor;
                text.line_spacing *= factor;
                // A text on a path runs along its curve's copy.
                if let Some(path) = &mut text.path
                    && let Some(c) = copies.get(&path.curve)
                {
                    path.curve = *c;
                }
            }
            made.push(format!("t{}", t.0));
        }
        Ok(made)
    }
}
