// SPDX-License-Identifier: MIT
//! Modifying a sketch (the sketch's Modify menu and the constraint and
//! dimension edits): delete, construction and fixed flags, dimension values,
//! drag, move and copy, trim and extend, fillet and chamfer, offset, mirror
//! and sketch patterns. Like the drawing tools, they make entities and
//! constraints.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::{FRAC_PI_2, TAU};

use super::edit::DRIVEN_SUFFIX;
use super::edit::{KeptParameter, PointInput, SketchEdit};
use super::fillet::{Leg, fillet_at, is_corner};
use super::geometry::{Curve2, P2, add, cross, dist, dot, norm, perp, scale, sub, unit};
use super::intersect::intersections;
use super::solve::{Goal, Solved, measure_in};
use super::{
    Constraint, ConstraintKind, ConstraintUid, DimensionKind, Entity, EntityKind, Ref, SpacingMode,
};
use crate::expr::ParamKind;
use crate::ids::EntityUid;
use crate::values::ValueInput;
use mitcad_model_base::internal::values::{resolve_value, value_uses_parameter};

/// Something of a sketch a command names: `p3`, `c4`, `t5` or `k6`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Item {
    Entity(Ref),
    Text(EntityUid),
    Constraint(ConstraintUid),
}

impl std::str::FromStr for Item {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        if let Ok(r) = text.parse::<Ref>() {
            return Ok(Self::Entity(r));
        }
        if let Ok(k) = text.parse::<ConstraintUid>() {
            return Ok(Self::Constraint(k));
        }
        if let Some(n) = mitcad_model_base::internal::ids::parse_number(text, "t") {
            return Ok(Self::Text(EntityUid(n)));
        }
        Err(format!(
            "“{text}” is not a point, curve, text, constraint or dimension (p3, c4, t5, k6)"
        ))
    }
}

/// An affine map of the plane: `p -> m p + t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Map2 {
    pub m: [[f64; 2]; 2],
    pub t: P2,
}

impl Map2 {
    pub fn translation(by: P2) -> Self {
        Self {
            m: [[1.0, 0.0], [0.0, 1.0]],
            t: by,
        }
    }

    pub fn rotation(center: P2, angle: f64) -> Self {
        let (s, c) = angle.sin_cos();
        let m = [[c, -s], [s, c]];
        let rotated = [c * center[0] - s * center[1], s * center[0] + c * center[1]];
        Self {
            m,
            t: sub(center, rotated),
        }
    }

    /// The mirror about the line through `a` and `b`.
    pub fn mirror(a: P2, b: P2) -> Self {
        let d = unit(sub(b, a));
        let m = [
            [d[0] * d[0] - d[1] * d[1], 2.0 * d[0] * d[1]],
            [2.0 * d[0] * d[1], d[1] * d[1] - d[0] * d[0]],
        ];
        let ma = [
            m[0][0] * a[0] + m[0][1] * a[1],
            m[1][0] * a[0] + m[1][1] * a[1],
        ];
        Self { m, t: sub(a, ma) }
    }

    pub fn apply(&self, p: P2) -> P2 {
        [
            self.m[0][0] * p[0] + self.m[0][1] * p[1] + self.t[0],
            self.m[1][0] * p[0] + self.m[1][1] * p[1] + self.t[1],
        ]
    }

    fn reverses(&self) -> bool {
        self.m[0][0] * self.m[1][1] - self.m[0][1] * self.m[1][0] < 0.0
    }

    /// The angle a turning map turns by (mitcad#1563), counter-clockwise;
    /// None for a translation or a mirror.
    fn turn(&self) -> Option<f64> {
        let identity = (self.m[0][0] - 1.0).abs() <= 1e-12
            && self.m[0][1].abs() <= 1e-12
            && self.m[1][0].abs() <= 1e-12
            && (self.m[1][1] - 1.0).abs() <= 1e-12;
        (!identity && !self.reverses()).then(|| self.m[1][0].atan2(self.m[0][0]))
    }
}

/// What a drag pulls on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragTarget {
    /// A point to a position.
    Point(EntityUid, P2),
    /// A curve's points by an offset.
    By(EntityUid, P2),
    /// A circle's radius.
    Radius(EntityUid, f64),
}

impl SketchEdit<'_> {
    fn entity(&self, id: EntityUid) -> Result<&Entity, String> {
        self.def
            .entity(id)
            .ok_or_else(|| format!("entity {} does not exist", id.0))
    }

    /// The curves that use a point.
    fn users(&self, point: EntityUid) -> Vec<EntityUid> {
        self.def
            .entities
            .iter()
            .filter(|e| e.kind.points().contains(&point))
            .map(|e| e.id)
            .collect()
    }

    // Deleting.

    /// Deletes entities, texts, constraints and dimensions. A deleted point
    /// takes the curves that use it; a deleted curve takes its points no
    /// other curve uses; constraints and dimensions on deleted entities go
    /// too.
    pub fn remove(&mut self, items: &[Item]) -> Result<(), String> {
        let mut entities = BTreeSet::new();
        let mut texts = BTreeSet::new();
        let mut constraints = BTreeSet::new();
        for item in items {
            match item {
                Item::Entity(r) => {
                    let e = self.entity(r.uid())?;
                    // A part of a derived offset goes only with the offset
                    // (mitcad#361).
                    self.editable(r.uid())?;
                    if e.is_point() != r.is_point() {
                        return Err(format!("{r} is a {}", e.kind.type_name()));
                    }
                    entities.insert(r.uid());
                }
                Item::Text(t) => {
                    if !self.def.texts.iter().any(|x| x.id == *t) {
                        return Err(format!("text t{} does not exist", t.0));
                    }
                    texts.insert(*t);
                }
                Item::Constraint(k) => {
                    let exists = self.def.constraints.iter().any(|c| c.id == *k)
                        || self.def.dimensions.iter().any(|d| d.id == *k)
                        || self.def.patterns.iter().any(|p| p.id == *k)
                        || self.def.offsets.iter().any(|o| o.id == *k);
                    if !exists {
                        return Err(format!("{k} does not exist"));
                    }
                    constraints.insert(*k);
                }
            }
        }
        // Curves on deleted points, then points only deleted curves used.
        for e in &self.def.entities {
            if e.kind.points().iter().any(|p| entities.contains(p)) {
                entities.insert(e.id);
            }
        }
        let curves: Vec<EntityUid> = entities
            .iter()
            .copied()
            .filter(|id| self.def.entity(*id).is_some_and(|e| !e.is_point()))
            .collect();
        for c in curves {
            let points = self.entity(c)?.kind.points();
            for p in points {
                let still_used = self
                    .users(p)
                    .iter()
                    .any(|u| *u != c && !entities.contains(u));
                if !still_used {
                    entities.insert(p);
                }
            }
        }
        self.drop_entities(&entities);
        self.def.texts.retain(|t| !texts.contains(&t.id));
        // Their text parameters' links go with them (mitcad#664).
        self.def.text_values.retain(|v| !texts.contains(&v.text));
        self.drop_constraints(&constraints);
        for t in texts {
            self.report.removed.push(format!("t{}", t.0));
        }
        Ok(())
    }

    /// Removes entities and everything that refers to them: constraints,
    /// dimensions, projections, the copies patterns made of them and the
    /// curves on removed points.
    pub(crate) fn drop_entities(&mut self, entities: &BTreeSet<EntityUid>) {
        if entities.is_empty() {
            return;
        }
        let mut entities = entities.clone();
        loop {
            let before = entities.len();
            for pattern in &self.def.patterns {
                for copy in &pattern.copies {
                    for (original, made) in &copy.entities {
                        if entities.contains(&original.uid()) {
                            entities.insert(made.uid());
                        }
                    }
                }
            }
            for e in &self.def.entities {
                if e.kind.points().iter().any(|p| entities.contains(p)) {
                    entities.insert(e.id);
                }
            }
            if entities.len() == before {
                break;
            }
        }
        let entities = &entities;
        for e in &self.def.entities {
            if entities.contains(&e.id) {
                self.report.removed.push(e.as_ref().to_string());
            }
        }
        self.def.entities.retain(|e| !entities.contains(&e.id));
        // Equal spacings (mitcad#221) lose the removed anchors; one too
        // short or without its direction goes.
        let spacings = self.spacings_without(entities);
        let on_removed = |refs: Vec<Ref>| refs.iter().any(|r| entities.contains(&r.uid()));
        let constraints: BTreeSet<ConstraintUid> = self
            .def
            .constraints
            .iter()
            .filter(|c| on_removed(c.kind.refs()))
            .map(|c| c.id)
            .chain(spacings)
            .chain(
                self.def
                    .dimensions
                    .iter()
                    .filter(|d| on_removed(d.kind.refs()))
                    .map(|d| d.id),
            )
            .collect();
        self.drop_constraints(&constraints);
        for projection in &mut self.def.projections {
            projection.entities.retain(|r| !entities.contains(&r.uid()));
        }
        self.def.projections.retain(|p| !p.entities.is_empty());
        // Patterns forget what is gone; one without originals or centre
        // goes, leaving its remaining copies as free geometry.
        let mut gone = BTreeSet::new();
        for pattern in &mut self.def.patterns {
            pattern.entities.retain(|r| !entities.contains(&r.uid()));
            for copy in &mut pattern.copies {
                copy.entities.retain(|original, made| {
                    !entities.contains(&original.uid()) && !entities.contains(&made.uid())
                });
            }
            let centre_gone = matches!(
                pattern.kind,
                super::pattern::PatternKind::Circular { center, .. } if entities.contains(&center)
            );
            if pattern.entities.is_empty() || centre_gone {
                gone.insert(pattern.id);
            }
        }
        // A text whose frame or path is gone stays where it is.
        for t in &mut self.def.texts {
            if t.frame.iter().any(|p| entities.contains(p)) {
                t.frame.clear();
            }
            if t.path.is_some_and(|p| entities.contains(&p.curve)) {
                t.path = None;
            }
        }
        // Offsets forget removed results; one whose sources are gone goes.
        for offset in &mut self.def.offsets {
            offset.results.retain(|r| !entities.contains(r));
            // An open outline (mitcad#1246) stays open: the result before an
            // end that is gone ends it.
            let open: Vec<bool> = offset
                .outlines
                .iter()
                .map(|o| o.last().is_some_and(|r| offset.ends.contains(r)))
                .collect();
            for outline in &mut offset.outlines {
                outline.retain(|r| !entities.contains(r));
            }
            offset.ends = offset
                .outlines
                .iter()
                .zip(&open)
                .filter(|(_, open)| **open)
                .filter_map(|(o, _)| o.last().copied())
                .collect();
            offset.outlines.retain(|o| !o.is_empty());
            offset.helpers.retain(|h| !entities.contains(h));
            if offset.curves.iter().any(|c| entities.contains(c)) || offset.results.is_empty() {
                gone.insert(offset.id);
            }
        }
        self.drop_constraints(&gone);
    }

    pub(crate) fn drop_constraints(&mut self, ids: &BTreeSet<ConstraintUid>) {
        // An equal spacing's dimension goes with it (mitcad#221).
        let mut ids = ids.clone();
        for d in &self.def.dimensions {
            if let DimensionKind::Spacing { constraint } = d.kind
                && ids.contains(&constraint)
            {
                ids.insert(d.id);
            }
        }
        let ids = &ids;
        for c in &self.def.constraints {
            if ids.contains(&c.id) {
                self.report.removed.push(c.id.to_string());
            }
        }
        for d in &self.def.dimensions {
            if ids.contains(&d.id) {
                self.report.removed.push(d.id.to_string());
            }
        }
        for p in &self.def.patterns {
            if ids.contains(&p.id) {
                self.report.removed.push(p.id.to_string());
            }
        }
        for o in &self.def.offsets {
            if ids.contains(&o.id) {
                self.report.removed.push(o.id.to_string());
            }
        }
        self.def.constraints.retain(|c| !ids.contains(&c.id));
        self.def.dimensions.retain(|d| !ids.contains(&d.id));
        self.def.patterns.retain(|p| !ids.contains(&p.id));
        self.def.offsets.retain(|o| !ids.contains(&o.id));
    }

    // Flags.

    pub fn set_construction(&mut self, curves: &[EntityUid], on: bool) -> Result<(), String> {
        for id in curves {
            self.curve(*id)?;
            let e = self.def.entity_mut(*id).expect("checked");
            e.construction = on;
            if let EntityKind::Line { centerline, .. } = &mut e.kind
                && on
            {
                *centerline = false;
            }
        }
        Ok(())
    }

    pub fn set_centerline(&mut self, lines: &[EntityUid], on: bool) -> Result<(), String> {
        for id in lines {
            let e = self
                .def
                .entity_mut(*id)
                .ok_or_else(|| format!("line c{} does not exist", id.0))?;
            match &mut e.kind {
                EntityKind::Line { centerline, .. } => *centerline = on,
                other => {
                    return Err(crate::said(
                        format!("the curve is a {}, not a line", other.type_name()),
                        format!("c{} is a {}", id.0, other.type_name()),
                    ));
                }
            }
            if on {
                e.construction = false;
            }
        }
        Ok(())
    }

    pub fn set_fixed(&mut self, entities: &[Ref], on: bool) -> Result<(), String> {
        for r in entities {
            self.entity(r.uid())?;
            self.def.entity_mut(r.uid()).expect("checked").fixed = on;
        }
        Ok(())
    }

    /// Activates or deactivates constraints and dimensions (mitcad#721):
    /// an inactive one stays in the sketch but the solve leaves it out.
    /// `reason` says why one is deactivated (kept with it until it is
    /// activated). Whether the sketch still solves with those activated is
    /// for the command to check. An equal spacing takes its spacing
    /// dimension with it either way, and activating a spacing dimension
    /// activates its equal spacing, since the dimension applies only with
    /// it (mitcad#723); deactivating the dimension alone leaves the equal
    /// spacing. Returns the ids whose state changed.
    pub fn set_active(
        &mut self,
        ids: &[ConstraintUid],
        active: bool,
        reason: Option<&str>,
    ) -> Result<Vec<ConstraintUid>, String> {
        let reason = reason.map(str::trim).filter(|r| !r.is_empty());
        let mut all: Vec<ConstraintUid> = Vec::new();
        for id in ids {
            let spacing = self.def.dimensions.iter().find_map(|d| match d.kind {
                DimensionKind::Spacing { constraint } if d.id == *id && active => Some(constraint),
                _ => None,
            });
            let dimensions = self.def.dimensions.iter().filter_map(|d| match d.kind {
                DimensionKind::Spacing { constraint } if constraint == *id => Some(d.id),
                _ => None,
            });
            for k in spacing.into_iter().chain([*id]).chain(dimensions) {
                if !all.contains(&k) {
                    all.push(k);
                }
            }
        }
        let mut changed = Vec::new();
        for id in &all {
            let (flag, why) = if let Some(c) = self.def.constraints.iter_mut().find(|c| c.id == *id)
            {
                (&mut c.active, &mut c.inactive_reason)
            } else if let Some(d) = self.def.dimensions.iter_mut().find(|d| d.id == *id) {
                (&mut d.active, &mut d.inactive_reason)
            } else {
                return Err(format!("constraint or dimension {id} does not exist"));
            };
            if *flag != active {
                changed.push(*id);
            }
            *flag = active;
            *why = if active {
                None
            } else {
                reason.map(str::to_owned).or_else(|| why.take())
            };
        }
        Ok(changed)
    }

    // Dimensions.

    fn dimension_index(&self, id: ConstraintUid) -> Result<usize, String> {
        self.def
            .dimensions
            .iter()
            .position(|d| d.id == id)
            .ok_or_else(|| format!("dimension {id} does not exist"))
    }

    /// Sets a dimension's value: a number, an expression (its own parameter
    /// takes it) or another parameter's name. A driven dimension becomes
    /// driving.
    pub fn set_dimension(&mut self, id: ConstraintUid, value: &ValueInput) -> Result<(), String> {
        let i = self.dimension_index(id)?;
        let d = &self.def.dimensions[i];
        let slot = d.slot();
        let comment = format!("{} {}", self.owner_name(), d.kind.type_name());
        // A driven dimension's reference becomes its driving parameter,
        // keeping the name (mitcad#231).
        let reference = d
            .reference
            .filter(|id| self.params.owner(*id) == Some(self.owner()));
        if let Some(id) = reference {
            self.params.set_kind(id, ParamKind::Model);
            let kept = self.params.get(id).map(|p| p.comment().to_owned());
            if let Some(stem) = kept.as_deref().and_then(|c| c.strip_suffix(DRIVEN_SUFFIX)) {
                self.params
                    .set_comment(id, stem)
                    .map_err(|e| e.to_string())?;
            }
        }
        let mut old = d.value.or(reference);
        // A value that uses the dimension's own parameter (`BuckleDiameter
        // * 2` in the dimension of BuckleDiameter) keeps that parameter,
        // with its name and value, as a user parameter, and the dimension
        // takes the value as a parameter of its own (mitcad#1535): the bare
        // name makes it use the user parameter. A parameter another
        // dimension of the sketch uses as well stays as it is; this
        // dimension only stops using it.
        let used = old.filter(|p| {
            self.params.owner(*p) == Some(self.owner())
                && value_uses_parameter(self.params, value, *p)
        });
        let mut kept = None;
        if let Some(p) = used {
            let shared = self
                .def
                .dimensions
                .iter()
                .enumerate()
                .any(|(j, other)| j != i && other.value == Some(p));
            if !shared {
                self.params.set_owner(p, None);
                kept = Some(p);
            }
            old = None;
        }
        let (param, created) = resolve_value(
            self.params,
            &slot,
            value,
            self.owner(),
            &comment,
            old.as_ref(),
        )
        .map_err(|e| e.to_string())?;
        self.report.parameters.extend(created);
        if let Some(p) = kept {
            let kept_name = self.params.name(p);
            let shown = self
                .params
                .get(p)
                .map(|q| q.expression().to_owned())
                .unwrap_or_default();
            let now = self.params.name(param);
            let note = if param == p {
                crate::said(
                    format!("{kept_name} is now a user parameter ({shown}); the dimension uses it"),
                    format!(
                        "{kept_name} is now a user parameter ({shown}); dimension {id} uses it"
                    ),
                )
            } else {
                let expression = self
                    .params
                    .get(param)
                    .map(|q| q.expression().to_owned())
                    .unwrap_or_default();
                crate::said(
                    format!(
                        "{kept_name} is now a user parameter ({shown}); the dimension is {now} = \
                         {expression}"
                    ),
                    format!(
                        "{kept_name} is now a user parameter ({shown}); dimension {id} is {now} = \
                         {expression}"
                    ),
                )
            };
            self.report.notes.push(note);
            self.report.kept.push(KeptParameter {
                name: kept_name,
                dimension: id,
                parameter: now,
            });
        }
        self.def.dimensions[i].value = Some(param);
        self.def.dimensions[i].reference = None;
        Ok(())
    }

    /// Makes a dimension driven (it only measures) or driving again with
    /// the value it measures.
    pub fn set_driven(&mut self, id: ConstraintUid, driven: bool) -> Result<(), String> {
        let i = self.dimension_index(id)?;
        if self.def.dimensions[i].is_driven() == driven {
            return Ok(());
        }
        if driven {
            // Its own parameter becomes its reference, keeping the name
            // (mitcad#231); the value is set when the sketch solves.
            let owner = self.owner();
            let d = &mut self.def.dimensions[i];
            let own = d.value.filter(|id| self.params.owner(*id) == Some(owner));
            d.reference = own;
            d.value = None;
            if let Some(id) = own {
                self.params.set_kind(id, ParamKind::Reference);
                let comment = self.params.get(id).map(|p| p.comment().to_owned());
                if let Some(comment) = comment.filter(|c| !c.ends_with(DRIVEN_SUFFIX)) {
                    self.params
                        .set_comment(id, &format!("{comment}{DRIVEN_SUFFIX}"))
                        .map_err(|e| e.to_string())?;
                }
            }
            return Ok(());
        }
        let solved = self.solved()?;
        let value = measure_in(&self.def.dimensions[i].kind, &solved, &self.def.constraints)
            .ok_or("the dimension cannot be measured")?;
        self.set_dimension(id, &ValueInput::Number(value))
    }

    /// Makes a horizontal or vertical distance signed (its value is the
    /// offset `b - a`, which may be negative) or unsigned (its size),
    /// keeping the point where it is (mitcad#409): where `b` lies on the
    /// negative side of `a`, a driving value is negated, a constant as a
    /// number and an expression as `-(…)` (or without the `-(…)` it
    /// already has). A driven one only measures the other way.
    pub fn set_signed(&mut self, id: ConstraintUid, signed: bool) -> Result<(), String> {
        let i = self.dimension_index(id)?;
        let mut kind = self.def.dimensions[i].kind.clone();
        let flag = match &mut kind {
            DimensionKind::HorizontalDistance { signed, .. }
            | DimensionKind::VerticalDistance { signed, .. } => signed,
            other => {
                return Err(format!(
                    "dimension {id} is a {}, not a horizontal or vertical distance",
                    other.type_name()
                ));
            }
        };
        if *flag == signed {
            return Ok(());
        }
        *flag = true;
        let solved = self.solved()?;
        let offset = measure_in(&kind, &solved, &self.def.constraints)
            .ok_or("the dimension cannot be measured")?;
        // The positions as they are now, which the stored ones may not be
        // after parameter changes: the solve after the switch starts from
        // them, so an unsigned dimension keeps its point on this side.
        solved.store(&mut self.def.entities);
        if let DimensionKind::HorizontalDistance { signed: s, .. }
        | DimensionKind::VerticalDistance { signed: s, .. } = &mut kind
        {
            *s = signed;
        }
        let value = self.def.dimensions[i].value;
        self.def.dimensions[i].kind = kind;
        let Some(param) = value else {
            return Ok(());
        };
        let current = self.params.value(param).unwrap_or(0.0);
        if offset >= 0.0 || current == 0.0 {
            return Ok(());
        }
        let p = self
            .params
            .get(param)
            .ok_or_else(|| format!("parameter {} does not exist", self.params.name(param)))?;
        let own = self.params.owner(param) == Some(self.owner());
        let negated = if own && p.expr().is_constant() {
            ValueInput::Number(-current)
        } else if own {
            ValueInput::Name(negate_expression(p.expression()))
        } else {
            // Another parameter by name: an expression of its own now.
            ValueInput::Name(format!("-{}", p.name()))
        };
        self.set_dimension(id, &negated)
    }

    pub fn set_text_position(&mut self, id: ConstraintUid, at: P2) -> Result<(), String> {
        let i = self.dimension_index(id)?;
        if !at.iter().all(|v| v.is_finite()) {
            return Err("the position must be finite".to_owned());
        }
        self.def.dimensions[i].text = Some(at);
        Ok(())
    }

    // Drag, move and copy.

    /// Pulls points toward targets as far as the constraints allow (a
    /// drag), and stores the result. What a derived offset computes pulls
    /// its source chain by as much (see [`SketchEdit::moved_points`]).
    pub fn drag(&mut self, targets: &[DragTarget]) -> Result<(), String> {
        let mut goals = Vec::new();
        for target in targets {
            match *target {
                DragTarget::Point(p, to) => {
                    let at = self.at(p)?;
                    if self.computing_offset(p).is_some() {
                        let by = sub(to, at);
                        for q in self.moved_points(p)? {
                            goals.push(Goal::Point(q, add(self.at(q)?, by)));
                        }
                    } else {
                        goals.push(Goal::Point(p, to));
                    }
                }
                DragTarget::By(c, by) => {
                    self.curve(c)?;
                    for p in self.moved_points(c)? {
                        goals.push(Goal::Point(p, add(self.at(p)?, by)));
                    }
                }
                DragTarget::Radius(c, r) => {
                    self.editable(c)?;
                    if !matches!(self.curve(c)?.kind, EntityKind::Circle { .. }) {
                        return Err(crate::said(
                            "the curve is not a circle",
                            format!("c{} is not a circle", c.0),
                        ));
                    }
                    goals.push(Goal::Radius(c, r));
                }
            }
        }
        self.drag_goals(&goals)
    }

    /// Drags a selection together by an offset (mitcad#837): every point
    /// of the entities, the points of their curves included, is pulled by
    /// `by` as far as the constraints allow, in one solve as a single
    /// drag. A point shared by several of them (a selected end of a
    /// selected curve) is pulled once; constraints among them hold, so the
    /// set moves rigidly unless fixed geometry or a constraint to geometry
    /// outside it holds it back.
    pub fn drag_together(&mut self, entities: &[Ref], by: P2) -> Result<(), String> {
        self.move_entities(entities, &Map2::translation(by))
    }

    fn drag_goals(&mut self, goals: &[Goal]) -> Result<(), String> {
        let params = &*self.params;
        let solved = self
            .def
            .solve_goals(
                &mut |id| {
                    params
                        .value(id)
                        .ok_or_else(|| format!("parameter {} does not exist", params.name(id)))
                },
                &|id| params.name(id),
                goals,
            )
            .map_err(|e| e.message)?;
        solved.store(&mut self.def.entities);
        // What finishing the edit would solve again (mitcad#679).
        self.dragged = Some(Box::new((self.def.clone(), self.params.clone(), solved)));
        Ok(())
    }

    /// The points of entities: points themselves and the points of curves.
    fn points_of(&self, entities: &[Ref]) -> Result<BTreeSet<EntityUid>, String> {
        let mut points = BTreeSet::new();
        for r in entities {
            let e = self.entity(r.uid())?;
            if e.is_point() {
                points.insert(e.id);
            } else {
                points.extend(e.kind.points());
            }
        }
        Ok(points)
    }

    /// Moves entities by a map (a rigid motion), keeping the constraints
    /// (Move): the points are dragged to their new places. What a derived
    /// offset computes moves its source chain by the map, and follows it
    /// (see [`SketchEdit::moved_points`]).
    pub fn move_entities(&mut self, entities: &[Ref], map: &Map2) -> Result<(), String> {
        let mut points = BTreeSet::new();
        for r in entities {
            self.entity(r.uid())?;
            points.extend(self.moved_points(r.uid())?);
        }
        // A turn first turns the directions the moved geometry is held to
        // (mitcad#1563): kept, a horizontal line pulled to a vertical
        // place collapsed to a point.
        let turned = match map.turn() {
            Some(angle) => self.turn_directions(&points, angle)?,
            None => false,
        };
        let mut places = Vec::new();
        for p in points {
            places.push((p, map.apply(self.at(p)?)));
        }
        if turned {
            // When the turn turned what holds the geometry to the axes, the
            // solve starts where the turn puts the points: from where they
            // were, the turned directions held them and the drag settled
            // elsewhere (a rectangle's corners swapped, a line stopped
            // short). Otherwise it starts where they are, as before, so
            // that geometry held by what does not move (a polygon about its
            // fixed centre) comes as near as its constraints let it. Not
            // fixed or projected points, nor those of fixed or projected
            // curves: they stay for the solve to hold.
            for &(p, to) in &places {
                let held = |e: &Entity| e.fixed || e.reference;
                if self.def.entity(p).is_none_or(held)
                    || self
                        .users(p)
                        .iter()
                        .any(|c| self.def.entity(*c).is_some_and(held))
                {
                    continue;
                }
                if let Some(e) = self.def.entity_mut(p)
                    && let EntityKind::Point { at } = &mut e.kind
                {
                    *at = to;
                }
            }
        }
        let goals: Vec<Goal> = places
            .into_iter()
            .map(|(p, to)| Goal::Point(p, to))
            .collect();
        self.drag_goals(&goals)
    }

    /// The middle of entities (mitcad#1563): the center of the box round
    /// their points, a circle's whole round included. Rotate turns about
    /// it when no pivot is given.
    pub fn middle_of(&self, entities: &[Ref]) -> Result<P2, String> {
        let mut low = [f64::INFINITY; 2];
        let mut high = [f64::NEG_INFINITY; 2];
        let mut include = |p: P2| {
            low = [low[0].min(p[0]), low[1].min(p[1])];
            high = [high[0].max(p[0]), high[1].max(p[1])];
        };
        for r in entities {
            if let EntityKind::Circle { center, radius, .. } = self.entity(r.uid())?.kind {
                let c = self.at(center)?;
                include(sub(c, [radius, radius]));
                include(add(c, [radius, radius]));
            }
        }
        for p in self.points_of(entities)? {
            include(self.at(p)?);
        }
        if !(low[0].is_finite() && high[0].is_finite()) {
            return Err("\"entities\" is empty".to_owned());
        }
        Ok([(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0])
    }

    /// Turns what holds moved geometry to the sketch's axes by a rotation
    /// (mitcad#1563), for the constraints and dimensions whose geometry
    /// all moves (`moved`: the points that move). By a quarter turn (or
    /// three) a horizontal one becomes vertical and the other way round; a
    /// half turn keeps them; a signed distance's points swap where the
    /// turn would change its sign. By any other angle the horizontal and
    /// vertical lines are held parallel or perpendicular to the first of
    /// them instead, a horizontal or vertical distance between two points
    /// level or plumb becomes their distance, and the other directional
    /// constraints and dimensions are deactivated, each said in the edit's
    /// notes. Kept as they were, they pulled the turned geometry back onto
    /// the axes: a horizontal line turned by 90 degrees collapsed to a
    /// point.
    fn turn_directions(&mut self, moved: &BTreeSet<EntityUid>, angle: f64) -> Result<bool, String> {
        use ConstraintKind as K;
        let quarters = angle / FRAC_PI_2;
        let exact = (quarters - quarters.round()).abs() <= 1e-9;
        let turns = (quarters.round() as i64).rem_euclid(4);
        let curves: BTreeSet<EntityUid> = self
            .def
            .entities
            .iter()
            .filter(|e| {
                let points = e.kind.points();
                !e.is_point() && !points.is_empty() && points.iter().all(|p| moved.contains(p))
            })
            .map(|e| e.id)
            .collect();
        let inside = |r: &Ref| match r {
            Ref::Point(p) => moved.contains(p),
            Ref::Curve(c) => curves.contains(c),
        };
        // Horizontal and vertical lines (true: horizontal) and the other
        // directional constraints, by an angle that is no quarter turn.
        let mut lines: Vec<(ConstraintUid, EntityUid, bool)> = Vec::new();
        let mut off: Vec<ConstraintUid> = Vec::new();
        // Whether anything was turned, replaced or deactivated.
        let mut changed = false;
        for c in &mut self.def.constraints {
            if !c.kind.refs().iter().all(&inside) {
                continue;
            }
            let swapped = match &c.kind {
                K::Horizontal { line } => K::Vertical { line: *line },
                K::Vertical { line } => K::Horizontal { line: *line },
                K::HorizontalPoints { a, b } => K::VerticalPoints { a: *a, b: *b },
                K::VerticalPoints { a, b } => K::HorizontalPoints { a: *a, b: *b },
                K::EqualSpacing {
                    anchors,
                    mode: mode @ (SpacingMode::Horizontal | SpacingMode::Vertical),
                    direction,
                } => K::EqualSpacing {
                    anchors: anchors.clone(),
                    mode: if *mode == SpacingMode::Horizontal {
                        SpacingMode::Vertical
                    } else {
                        SpacingMode::Horizontal
                    },
                    direction: *direction,
                },
                _ => continue,
            };
            if exact {
                if turns % 2 == 1 {
                    c.kind = swapped;
                    changed = true;
                }
            } else if !c.active {
                // It holds nothing now; its meaning is the user's.
            } else if let K::Horizontal { line } | K::Vertical { line } = c.kind {
                lines.push((c.id, line, matches!(c.kind, K::Horizontal { .. })));
            } else {
                off.push(c.id);
            }
        }
        // Horizontal and vertical distances between moved points.
        let mut changes: Vec<(usize, DimensionKind)> = Vec::new();
        let mut aligned: Vec<String> = Vec::new();
        for (i, d) in self.def.dimensions.iter().enumerate() {
            let (a, b, horizontal, signed) = match d.kind {
                DimensionKind::HorizontalDistance { a, b, signed } => (a, b, true, signed),
                DimensionKind::VerticalDistance { a, b, signed } => (a, b, false, signed),
                _ => continue,
            };
            if !(moved.contains(&a) && moved.contains(&b)) {
                continue;
            }
            if exact {
                // A quarter turn counter-clockwise takes (dx, dy) to
                // (-dy, dx), three of them to (dy, -dx): where the new
                // offset is the old one negated, a signed distance
                // measures from its other point.
                let (now_horizontal, negated) = match turns {
                    1 => (!horizontal, !horizontal),
                    2 => (horizontal, true),
                    3 => (!horizontal, horizontal),
                    _ => (horizontal, false),
                };
                let (a, b) = if negated && signed { (b, a) } else { (a, b) };
                let kind = if now_horizontal {
                    DimensionKind::HorizontalDistance { a, b, signed }
                } else {
                    DimensionKind::VerticalDistance { a, b, signed }
                };
                if kind != d.kind {
                    changes.push((i, kind));
                }
            } else if d.active {
                let (pa, pb) = (self.at(a)?, self.at(b)?);
                let (along, across) = if horizontal {
                    (pb[0] - pa[0], pb[1] - pa[1])
                } else {
                    (pb[1] - pa[1], pb[0] - pa[0])
                };
                if !signed && across.abs() <= 1e-9 * along.abs().max(1.0) {
                    changes.push((i, DimensionKind::Distance { a, b }));
                    aligned.push(d.id.to_string());
                } else {
                    off.push(d.id);
                }
            }
        }
        changed = changed || !changes.is_empty() || !lines.is_empty() || !off.is_empty();
        for (i, kind) in changes {
            self.def.dimensions[i].kind = kind;
        }
        if let Some(&(_, first, first_horizontal)) = lines.first() {
            let removed: BTreeSet<ConstraintUid> = lines.iter().map(|l| l.0).collect();
            self.def.constraints.retain(|c| !removed.contains(&c.id));
            let mut held = BTreeSet::from([first]);
            for &(_, line, horizontal) in &lines {
                if !held.insert(line) {
                    continue;
                }
                self.add_constraint(if horizontal == first_horizontal {
                    K::Parallel { a: first, b: line }
                } else {
                    K::Perpendicular { a: first, b: line }
                });
            }
            let names: Vec<String> = removed.iter().map(ToString::to_string).collect();
            self.report.removed.extend(names.iter().cloned());
            let held = if names.len() == 1 {
                "its line"
            } else {
                "their lines"
            };
            self.report.notes.push(crate::said(
                format!(
                    "the rotation turned {} off the sketch's axes: {held} held parallel or \
                     perpendicular to the first line instead",
                    counted(names.len(), "constraint", "constraints")
                ),
                format!(
                    "the rotation turned {} off the sketch's axes: {held} held parallel or \
                     perpendicular to {} instead",
                    names.join(", "),
                    Ref::Curve(first)
                ),
            ));
        }
        if !aligned.is_empty() {
            self.report.notes.push(crate::said(
                format!(
                    "the rotation turned {} off the sketch's axes: now an aligned distance",
                    counted(aligned.len(), "dimension", "dimensions")
                ),
                format!(
                    "the rotation turned {} off the sketch's axes: now an aligned distance",
                    aligned.join(", ")
                ),
            ));
        }
        if !off.is_empty() {
            let reason = "turned off the sketch's axes by a rotation";
            self.set_active(&off, false, Some(reason))?;
            let names: Vec<String> = off.iter().map(ToString::to_string).collect();
            self.report.notes.push(crate::said(
                format!(
                    "{} deactivated: {reason}",
                    counted(names.len(), "constraint", "constraints")
                ),
                format!("{} deactivated: {reason}", names.join(", ")),
            ));
        }
        Ok(changed)
    }

    /// Copies entities mapped by `map`: points and curves, and the
    /// constraints among them. Points listed in `keep` (on a mirror axis)
    /// are shared, not copied. Returns the copy of each entity.
    pub fn copy_entities(
        &mut self,
        entities: &[Ref],
        map: &Map2,
        keep: &BTreeSet<EntityUid>,
        constraints: bool,
    ) -> Result<BTreeMap<EntityUid, EntityUid>, String> {
        let mut copies: BTreeMap<EntityUid, EntityUid> = BTreeMap::new();
        let mut curves = Vec::new();
        for r in entities {
            let e = self.entity(r.uid())?.clone();
            if !e.is_point() {
                curves.push(e);
            }
        }
        // Curves first in id order, as the tools make them.
        let curve_ids = self.ids(curves.len());
        for (e, id) in curves.iter().zip(&curve_ids) {
            copies.insert(e.id, *id);
        }
        for p in self.points_of(entities)? {
            if keep.contains(&p) {
                copies.insert(p, p);
                continue;
            }
            let at = map.apply(self.at(p)?);
            let id = self.id();
            self.push(Entity::point(id, at));
            copies.insert(p, id);
        }
        for (e, id) in curves.iter().zip(curve_ids) {
            let mut kind = e.kind.clone();
            for p in e.kind.points() {
                kind.replace_point(p, copies[&p]);
            }
            // A mirror turns counter-clockwise arcs around.
            if map.reverses() {
                match &mut kind {
                    EntityKind::Arc { start, end, .. }
                    | EntityKind::EllipticalArc { start, end, .. } => std::mem::swap(start, end),
                    _ => {}
                }
            }
            if let EntityKind::Circle { radius, .. } = &mut kind {
                let scale = (map.m[0][0] * map.m[1][1] - map.m[0][1] * map.m[1][0])
                    .abs()
                    .sqrt();
                *radius *= scale;
            }
            let mut copy = Entity::new(id, kind);
            copy.construction = e.construction;
            self.push(copy);
        }
        if constraints {
            // Turned copies are no longer horizontal or vertical.
            let turned = map.m != [[1.0, 0.0], [0.0, 1.0]];
            let all: Vec<Constraint> = self.def.constraints.clone();
            for c in all {
                let refs = c.kind.refs();
                let directional = matches!(
                    c.kind,
                    ConstraintKind::Horizontal { .. }
                        | ConstraintKind::Vertical { .. }
                        | ConstraintKind::HorizontalPoints { .. }
                        | ConstraintKind::VerticalPoints { .. }
                        | ConstraintKind::EqualSpacing {
                            mode: SpacingMode::Horizontal | SpacingMode::Vertical,
                            ..
                        }
                );
                if refs.iter().all(|r| copies.contains_key(&r.uid())) && !(turned && directional) {
                    let mut kind = c.kind.clone();
                    remap(&mut kind, &copies);
                    if kind != c.kind {
                        let id = self.add_constraint(kind);
                        // An inactive one's copy is inactive too (mitcad#721).
                        if !c.active
                            && let Some(made) = self.def.constraints.iter_mut().find(|k| k.id == id)
                        {
                            made.active = false;
                            made.inactive_reason = c.inactive_reason.clone();
                        }
                    }
                }
            }
        }
        Ok(copies)
    }

    /// Mirrors entities about a line (Mirror): copies with
    /// symmetric constraints; points on the axis are shared. The line
    /// itself among the entities (a window over the sketch takes it too)
    /// is left out: it mirrors onto itself (mitcad#876).
    pub fn mirror(&mut self, entities: &[Ref], axis: EntityUid) -> Result<Vec<Ref>, String> {
        let (a, b) = match self.curve(axis)?.kind {
            EntityKind::Line { start, end, .. } => (self.at(start)?, self.at(end)?),
            _ => {
                return Err(crate::said(
                    "the mirror line is not a line",
                    format!("the mirror line c{} is not a line", axis.0),
                ));
            }
        };
        let entities: Vec<Ref> = entities
            .iter()
            .copied()
            .filter(|r| r.uid() != axis)
            .collect();
        if entities.is_empty() {
            return Err("the mirror line cannot be mirrored: give other entities".to_owned());
        }
        let map = Map2::mirror(a, b);
        let d = unit(sub(b, a));
        let size = dist(a, b).max(1.0);
        let mut keep = BTreeSet::new();
        for p in self.points_of(&entities)? {
            if cross(d, sub(self.at(p)?, a)).abs() <= 1e-9 * size {
                keep.insert(p);
            }
        }
        // The points a mirrored line's, circle's or arc's symmetry holds
        // already (its ends, its centre), and those of the other curves,
        // which get a symmetry each below. A point picked as well, as a
        // window gives a circle with its centre, gets no constraint of its
        // own, which would only be redundant (mitcad#876).
        let mut held = BTreeSet::new();
        let mut of_others = BTreeSet::new();
        for r in &entities {
            let e = self.entity(r.uid())?;
            match e.kind {
                EntityKind::Point { .. } => {}
                EntityKind::Line { .. } | EntityKind::Circle { .. } | EntityKind::Arc { .. } => {
                    held.extend(e.kind.points());
                }
                _ => of_others.extend(e.kind.points()),
            }
        }
        let copies = self.copy_entities(&entities, &map, &keep, false)?;
        let mut made = Vec::new();
        // The points of the other curves given a symmetry so far: one each,
        // also where two of them share a point.
        let mut symmetric = BTreeSet::new();
        for r in &entities {
            let original = r.uid();
            let copy = copies[&original];
            let kind = self.entity(original)?.kind.clone();
            match kind {
                EntityKind::Point { .. } => {
                    if copy != original
                        && !held.contains(&original)
                        && !of_others.contains(&original)
                    {
                        self.add_constraint(ConstraintKind::Symmetric {
                            a: Ref::Point(original),
                            b: Ref::Point(copy),
                            axis,
                            radii: true,
                        });
                    }
                }
                EntityKind::Line { .. } | EntityKind::Circle { .. } | EntityKind::Arc { .. } => {
                    self.add_constraint(ConstraintKind::Symmetric {
                        a: Ref::Curve(original),
                        b: Ref::Curve(copy),
                        axis,
                        radii: true,
                    });
                }
                other => {
                    for p in other.points() {
                        if copies[&p] != p && !held.contains(&p) && symmetric.insert(p) {
                            self.add_constraint(ConstraintKind::Symmetric {
                                a: Ref::Point(p),
                                b: Ref::Point(copies[&p]),
                                axis,
                                radii: true,
                            });
                        }
                    }
                    // The points leave an ellipse's minor radius free: the
                    // copy keeps it equal to the original's (mitcad#363).
                    if matches!(
                        other,
                        EntityKind::Ellipse { .. } | EntityKind::EllipticalArc { .. }
                    ) {
                        self.add_constraint(ConstraintKind::Equal {
                            a: original,
                            b: copy,
                        });
                    }
                }
            }
            made.push(self.entity(copy)?.as_ref());
        }
        Ok(made)
    }

    // Trim and extend.

    /// Where other curves cross a curve, sorted along it, ends of an open
    /// curve left out.
    fn crossings(&self, id: EntityUid, solved: &Solved) -> Result<(Curve2, Vec<Cut>), String> {
        let curve = solved
            .curves
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("curve c{} does not exist", id.0))?;
        let size = sketch_size(&solved.curves);
        let tol = 1e-9 * size;
        // Extend's reach for an existing point (`extend`: ten times its
        // tolerance on ten times the size).
        let reach = 1e-7 * size;
        let (lo, hi) = curve.domain();
        let mut cuts = Vec::new();
        for (other, c) in &solved.curves {
            if *other == id {
                continue;
            }
            for hit in intersections(&curve, c, tol) {
                let t = curve.wrap(hit.ta);
                let at_end = !curve.is_closed()
                    && ((t - lo).abs() <= 1e-9 * (hi - lo) || (t - hi).abs() <= 1e-9 * (hi - lo));
                if at_end {
                    continue;
                }
                let touching = cross(unit(curve.eval(hit.ta)[1]), unit(c.eval(hit.tb)[1])).abs()
                    <= TANGENT_JOIN;
                cuts.push(Cut {
                    t,
                    curve: *other,
                    reach: if touching {
                        reach.max(TANGENT_REACH * size)
                    } else {
                        reach
                    },
                });
            }
        }
        cuts.sort_by(|a, b| a.t.total_cmp(&b.t));
        Ok((curve, cuts))
    }

    /// The end a trim gives a curve at a cut (mitcad#1191), as Extend joins
    /// a point (mitcad#259): an existing point there (one of the crossing
    /// curve first, then any other, never one of the trimmed curve `id`)
    /// becomes the end, held on the crossing curve when it is none of its
    /// points and nothing holds it there yet (a lone point, another curve's
    /// end); one the curve cannot end at (projected, computed by a derived
    /// offset or a pattern's, see `points.rs`) holds a new end with a
    /// point-to-point coincident constraint; elsewhere (a T) the end is a
    /// new point on the crossing curve. Returns the end and the point it
    /// joined.
    fn cut_end(
        &mut self,
        id: EntityUid,
        curve: &Curve2,
        cut: &Cut,
        solved: &Solved,
    ) -> (EntityUid, Option<EntityUid>) {
        let at = curve.point(cut.t);
        match self.point_reached(at, &solved.points, cut.curve, id, cut.reach) {
            Some((q, true)) => {
                let of_crossing = self
                    .def
                    .entity(cut.curve)
                    .is_some_and(|e| e.kind.points().contains(&q));
                let held = self.def.constraints.iter().any(|c| {
                    matches!(c.kind, ConstraintKind::Coincident { point, entity: Ref::Curve(on) }
                        if point == q && on == cut.curve)
                });
                if !of_crossing && !held && self.computing_offset(cut.curve).is_none() {
                    self.add_constraint(ConstraintKind::Coincident {
                        point: q,
                        entity: Ref::Curve(cut.curve),
                    });
                }
                (q, Some(q))
            }
            Some((q, false)) => {
                let p = self.add_point(solved.points.get(&q).copied().unwrap_or(at));
                if self.computing_offset(q).is_none() {
                    self.add_constraint(ConstraintKind::Coincident {
                        point: p,
                        entity: Ref::Point(q),
                    });
                }
                (p, Some(q))
            }
            None => (self.point_on_crossing(at, cut.curve), None),
        }
    }

    /// After a trim joined existing points: a point-on-curve of one of them
    /// on the trimmed curves, which its end there now holds, and what else
    /// then says nothing on a point that became an end (`points.rs`) go.
    fn drop_idle_after_trim(&mut self, joined: &[EntityUid], curves: &[EntityUid]) {
        let on: BTreeSet<ConstraintUid> = self
            .def
            .constraints
            .iter()
            .filter(|c| {
                matches!(c.kind, ConstraintKind::Coincident { point, entity: Ref::Curve(curve) }
                    if joined.contains(&point) && curves.contains(&curve))
            })
            .map(|c| c.id)
            .collect();
        self.drop_constraints(&on);
        for q in joined {
            if self.def.entity(*q).is_some() {
                self.drop_idle_on(*q);
            }
        }
    }

    /// After a trim's cut end `end` of the curve `id` joined the existing
    /// point `joined` (mitcad#1220): a tangent between the curve and each
    /// curve there that it meets tangentially (the directions within
    /// [`TANGENT_JOIN`]), the crossing curve and any other curve that ends
    /// at the joined point, as the Line and Arc tools add one when drawing
    /// tangentially. None where one already holds (an active tangent or
    /// smooth constraint between the two at that point), none between two
    /// lines or with an ellipse or a curve a derived offset computes, and
    /// none that the other constraints already imply or that conflicts with
    /// them (see `add_tangent_if_safe`). Returns whether one was added.
    fn tangent_at_join(
        &mut self,
        id: EntityUid,
        curve: &Curve2,
        cut: &Cut,
        end: EntityUid,
        joined: Option<EntityUid>,
        solved: &Solved,
    ) -> bool {
        let Some(q) = joined else {
            return false;
        };
        let Some(&at) = solved.points.get(&q) else {
            return false;
        };
        let along = unit(curve.eval(cut.t)[1]);
        let mut others = vec![cut.curve];
        others.extend(
            self.def
                .entities
                .iter()
                .filter(|e| e.id != id && e.id != cut.curve)
                .filter(|e| e.kind.ends().is_some_and(|(s, t)| s == q || t == q))
                .map(|e| e.id),
        );
        let mut added = false;
        for other in others {
            if !self.takes_tangent(id, other) || self.tangent_holds(id, other, &[end, q]) {
                continue;
            }
            let Some(c) = solved.curves.get(&other) else {
                continue;
            };
            let (t, _) = c.closest(at);
            if cross(along, unit(c.eval(t)[1])).abs() > TANGENT_JOIN {
                continue;
            }
            added |= self.add_tangent_if_safe(id, other);
        }
        added
    }

    /// Whether a tangent between the curves `a` and `b` is one the solver
    /// takes: lines, circles, arcs and splines, not two lines, and neither
    /// computed by a derived offset (whose curves take no constraints,
    /// mitcad#361).
    fn takes_tangent(&self, a: EntityUid, b: EntityUid) -> bool {
        let kind = |c: EntityUid| match self.def.entity(c).map(|e| &e.kind) {
            Some(EntityKind::Line { .. }) => Some(true),
            Some(
                EntityKind::Arc { .. }
                | EntityKind::Circle { .. }
                | EntityKind::Spline { .. }
                | EntityKind::FittedSpline { .. },
            ) => Some(false),
            _ => None,
        };
        match (kind(a), kind(b)) {
            (Some(true), Some(true)) | (None, _) | (_, None) => false,
            _ => self.computing_offset(a).is_none() && self.computing_offset(b).is_none(),
        }
    }

    /// Whether an active tangent or smooth constraint between the curves
    /// `a` and `b` applies at one of the points `at` (or, without a point
    /// of its own, where they meet).
    fn tangent_holds(&self, a: EntityUid, b: EntityUid, at: &[EntityUid]) -> bool {
        self.def.constraints.iter().any(|c| match c.kind {
            ConstraintKind::Tangent { a: x, b: y, at: on }
            | ConstraintKind::Smooth { a: x, b: y, at: on } => {
                c.active
                    && ((x == a && y == b) || (x == b && y == a))
                    && on.is_none_or(|p| at.contains(&p))
            }
            _ => false,
        })
    }

    /// Adds a tangent between the curves `a` and `b` only when the sketch
    /// still solves with it, no constraint more is redundant (the others
    /// already imply it) and none more conflicts; otherwise it is taken out
    /// again. Returns whether it stays.
    fn add_tangent_if_safe(&mut self, a: EntityUid, b: EntityUid) -> bool {
        let Ok(before) = self.solved() else {
            return false;
        };
        let id = self.add_constraint(ConstraintKind::tangent(a, b));
        let safe = self.solved().is_ok_and(|after| {
            after.status.conflicts.len() <= before.status.conflicts.len()
                && after.status.redundant.len() <= before.status.redundant.len()
        });
        if !safe {
            self.def.constraints.retain(|c| c.id != id);
            self.report.constraints.retain(|k| *k != id);
        }
        safe
    }

    /// After a trim cut off the end `old` of the curve `id` and a new
    /// tangent took its place at the new end (mitcad#1220): a tangent
    /// between the curve and one that met it at `old` (or that applies at
    /// `old`) and no longer touches it (the two share no point) held the
    /// joint that is gone, and goes with it. A tangent that still
    /// constrains something real stays: one at a point the curve still
    /// has, one between curves that still share a point, and every tangent
    /// where no new one took the old one's place.
    fn drop_replaced_tangents(&mut self, id: EntityUid, old: EntityUid) {
        let points_of = |c: EntityUid| -> Vec<EntityUid> {
            self.def
                .entity(c)
                .map(|e| e.kind.points())
                .unwrap_or_default()
        };
        let own = points_of(id);
        let stale: BTreeSet<ConstraintUid> = self
            .def
            .constraints
            .iter()
            .filter_map(|c| {
                let ConstraintKind::Tangent { a, b, at } = c.kind else {
                    return None;
                };
                let other = match (a == id, b == id) {
                    (true, false) => b,
                    (false, true) => a,
                    _ => return None,
                };
                let theirs = points_of(other);
                let met_there = match at {
                    Some(p) => p == old,
                    None => theirs.contains(&old),
                };
                let apart = !theirs.iter().any(|p| own.contains(p));
                (met_there && apart).then_some(c.id)
            })
            .collect();
        self.drop_constraints(&stale);
    }

    /// A new point at a position on the curve `on` (point-on-curve).
    fn point_on(&mut self, at: P2, on: EntityUid) -> EntityUid {
        let p = self.add_point(at);
        self.add_constraint(ConstraintKind::Coincident {
            point: p,
            entity: Ref::Curve(on),
        });
        p
    }

    /// A new point where a trim or an extension meets the curve `on`: held
    /// on it, unless a derived offset computes the curve, whose curves
    /// take no constraints (mitcad#361): then it is a plain point.
    fn point_on_crossing(&mut self, at: P2, on: EntityUid) -> EntityUid {
        if self.computing_offset(on).is_some() {
            self.add_point(at)
        } else {
            self.point_on(at, on)
        }
    }

    /// After the end of a curve moved, the dimensions on that point measure
    /// the new place (mitcad#362): a distance keeps its parameter and takes
    /// the new value, so that the end is not pulled back to what it
    /// measured before. A parameter that has an expression or is shared
    /// (with a dimension, an offset or an expression) keeps its value, and
    /// the dimension holds. An inactive one (mitcad#723) keeps its value:
    /// it holds nothing, and its value is what it asks for when activated
    /// (an imported one's value is the file's).
    fn remeasure_dimensions_on(&mut self, point: EntityUid, after: &Solved) {
        let ids: Vec<ConstraintUid> = self
            .def
            .dimensions
            .iter()
            .filter(|d| d.kind.refs().contains(&Ref::Point(point)) && self.def.dimension_applied(d))
            .map(|d| d.id)
            .collect();
        for id in ids {
            let Some(d) = self.def.dimensions.iter().find(|d| d.id == id) else {
                continue;
            };
            let Some(param) = d.value else {
                continue;
            };
            let plain = self
                .params
                .get(param)
                .is_some_and(|p| p.dependencies().is_empty());
            let shared = self
                .def
                .dimensions
                .iter()
                .filter(|o| o.value == Some(param))
                .count()
                > 1
                || self.def.offsets.iter().any(|o| o.distance == param)
                || self
                    .params
                    .iter()
                    .any(|p| p.dependencies().contains(&param));
            if !plain || shared {
                continue;
            }
            let Some(value) = measure_in(&d.kind, after, &self.def.constraints) else {
                continue;
            };
            // A value the parameter cannot take leaves the dimension as it is.
            let _ = self.set_dimension(id, &ValueInput::Number(value));
        }
    }

    /// Removes the dimensions that measure a curve's extent and the
    /// midpoint constraints on it, which a trim or an extend changes.
    /// An inactive length stays (mitcad#723): it holds nothing back, a line
    /// stays a line, and an imported one keeps the file's value until it is
    /// activated or deleted.
    fn drop_extent_constraints(&mut self, curve: EntityUid) {
        let ids: BTreeSet<ConstraintUid> = self
            .def
            .dimensions
            .iter()
            .filter(|d| {
                matches!(d.kind, DimensionKind::Length { line } if line == curve && d.active)
                    || matches!(d.kind, DimensionKind::ArcLength { arc } if arc == curve)
            })
            .map(|d| d.id)
            .chain(
                self.def
                    .constraints
                    .iter()
                    .filter(|c| {
                        matches!(c.kind, ConstraintKind::Midpoint { curve: m, .. } if m == curve)
                    })
                    .map(|c| c.id),
            )
            .collect();
        self.drop_constraints(&ids);
    }

    /// Removes a point no curve uses any more.
    fn drop_if_unused(&mut self, p: EntityUid) {
        if self.users(p).is_empty() {
            self.drop_entities(&BTreeSet::from([p]));
        }
    }

    /// Trims the piece of a curve around `at` between the curves that cross
    /// it (Trim). A curve nothing crosses is deleted. A cut end that lies on
    /// an existing point becomes that point, as an extended end does
    /// (mitcad#1191, see `cut_end`), so that both curves of a crossing
    /// trimmed there end in one point. A joined end that meets a curve
    /// there tangentially gets a tangent constraint to it, and a tangent
    /// that held the cut-off end to a curve no longer touching it goes
    /// (mitcad#1220, see `tangent_at_join` and `drop_replaced_tangents`).
    pub fn trim(&mut self, id: EntityUid, at: P2) -> Result<(), String> {
        self.shape_editable(id)?;
        let solved = self.solved()?;
        let (curve, cuts) = self.crossings(id, &solved)?;
        let kind = self.curve(id)?.kind.clone();
        let (t_at, _) = curve.closest(at);
        let (lo, hi) = curve.domain();
        if curve.is_closed() {
            if cuts.len() < 2 {
                return self.remove(&[Item::Entity(Ref::Curve(id))]);
            }
            // The cuts around the pick; the rest becomes an arc.
            let after = cuts.iter().position(|c| c.t > t_at).unwrap_or(0);
            let before = (after + cuts.len() - 1) % cuts.len();
            let (t_end, t_start) = (cuts[before].t, cuts[after].t);
            let (start, joined_start) = self.cut_end(id, &curve, &cuts[after], &solved);
            let (mut end, mut joined_end) = self.cut_end(id, &curve, &cuts[before], &solved);
            if end == start {
                // A touching curve found as two cuts a little apart, both
                // at one point: the arc does not close on it.
                end = self.point_on_crossing(curve.point(t_end), cuts[before].curve);
                joined_end = None;
            }
            let joined: Vec<EntityUid> = joined_start.into_iter().chain(joined_end).collect();
            self.drop_extent_constraints(id);
            if matches!(
                kind,
                EntityKind::Spline { .. } | EntityKind::FittedSpline { .. }
            ) {
                // A closed spline (mitcad#783): the rest from the start
                // round to the end, across its seam when it runs over it,
                // as a spline by control points.
                self.reshape(id, &curve, t_start, t_end, Some(start), Some(end))?;
                self.drop_idle_after_trim(&joined, &[id]);
                self.tangent_at_join(id, &curve, &cuts[after], start, joined_start, &solved);
                self.tangent_at_join(id, &curve, &cuts[before], end, joined_end, &solved);
                return Ok(());
            }
            let e = self.def.entity_mut(id).expect("a curve");
            e.kind = match kind {
                EntityKind::Circle { center, .. } => EntityKind::Arc { center, start, end },
                EntityKind::Ellipse {
                    center,
                    major,
                    minor_radius,
                } => EntityKind::EllipticalArc {
                    center,
                    major,
                    minor_radius,
                    start,
                    end,
                },
                _ => return Err("this closed curve cannot be trimmed".to_owned()),
            };
            self.drop_idle_after_trim(&joined, &[id]);
            self.tangent_at_join(id, &curve, &cuts[after], start, joined_start, &solved);
            self.tangent_at_join(id, &curve, &cuts[before], end, joined_end, &solved);
            return Ok(());
        }
        let below = cuts.iter().rev().find(|c| c.t < t_at);
        let above = cuts.iter().find(|c| c.t > t_at);
        let (old_start, old_end) = kind.ends().expect("an open curve");
        self.drop_extent_constraints(id);
        match (below, above) {
            (None, None) => return self.remove(&[Item::Entity(Ref::Curve(id))]),
            (None, Some(cut)) => {
                // Cut off the start.
                let (p, joined) = self.cut_end(id, &curve, cut, &solved);
                self.reshape(id, &curve, cut.t, hi, Some(p), None)?;
                self.drop_if_unused(old_start);
                self.drop_idle_after_trim(joined.as_slice(), &[id]);
                if self.tangent_at_join(id, &curve, cut, p, joined, &solved) {
                    self.drop_replaced_tangents(id, old_start);
                }
            }
            (Some(cut), None) => {
                let (p, joined) = self.cut_end(id, &curve, cut, &solved);
                self.reshape(id, &curve, lo, cut.t, None, Some(p))?;
                self.drop_if_unused(old_end);
                self.drop_idle_after_trim(joined.as_slice(), &[id]);
                if self.tangent_at_join(id, &curve, cut, p, joined, &solved) {
                    self.drop_replaced_tangents(id, old_end);
                }
            }
            (Some(cut0), Some(cut1)) => {
                // Keep both ends: this curve up to t0, a new one from t1.
                let (p0, joined0) = self.cut_end(id, &curve, cut0, &solved);
                let (p1, joined1) = self.cut_end(id, &curve, cut1, &solved);
                let copy = self.id();
                let mut second = self.curve(id)?.clone();
                second.id = copy;
                self.push(second);
                self.reshape(copy, &curve, cut1.t, hi, Some(p1), None)?;
                self.reshape(id, &curve, lo, cut0.t, None, Some(p0))?;
                match kind {
                    EntityKind::Line { .. } => {
                        self.add_constraint(ConstraintKind::Collinear { a: id, b: copy });
                    }
                    // The pieces share the centre (and an ellipse's major
                    // point); Equal keeps them on one curve, their radius or
                    // minor radius as one (mitcad#1314).
                    EntityKind::Arc { .. } | EntityKind::EllipticalArc { .. } => {
                        self.add_constraint(ConstraintKind::Equal { a: id, b: copy });
                    }
                    _ => {}
                }
                let joined: Vec<EntityUid> = joined0.into_iter().chain(joined1).collect();
                self.drop_idle_after_trim(&joined, &[id, copy]);
                // Tangents at the cut ends; the curve's own constraints stay
                // as they were (no end of it is gone, only split off).
                self.tangent_at_join(id, &curve, cut0, p0, joined0, &solved);
                self.tangent_at_join(copy, &curve, cut1, p1, joined1, &solved);
            }
        }
        Ok(())
    }

    /// Gives an open curve the piece `[t0, t1]` of its solved geometry,
    /// with new start or end points where given.
    fn reshape(
        &mut self,
        id: EntityUid,
        curve: &Curve2,
        t0: f64,
        t1: f64,
        start: Option<EntityUid>,
        end: Option<EntityUid>,
    ) -> Result<(), String> {
        let kind = self.curve(id)?.kind.clone();
        let new_kind = match kind {
            EntityKind::Line {
                start: s,
                end: e,
                centerline,
            } => EntityKind::Line {
                start: start.unwrap_or(s),
                end: end.unwrap_or(e),
                centerline,
            },
            EntityKind::Arc {
                center,
                start: s,
                end: e,
            } => EntityKind::Arc {
                center,
                start: start.unwrap_or(s),
                end: end.unwrap_or(e),
            },
            EntityKind::EllipticalArc {
                center,
                major,
                minor_radius,
                start: s,
                end: e,
            } => EntityKind::EllipticalArc {
                center,
                major,
                minor_radius,
                start: start.unwrap_or(s),
                end: end.unwrap_or(e),
            },
            EntityKind::Spline { .. } | EntityKind::FittedSpline { .. } => {
                // The piece as a B-spline by control points; the ends are
                // the given points (or the old end points).
                let Curve2::Nurbs(n) = curve else {
                    return Err("not a spline".to_owned());
                };
                // A closed spline's piece may run across its seam
                // (mitcad#783).
                let piece = if t1 < t0 {
                    n.piece_around(t0, t1)
                } else {
                    n.piece(t0, t1)
                };
                // A closed spline has no ends: both are given.
                let old = kind.ends();
                let last = piece.control.len() - 1;
                let mut control = Vec::with_capacity(piece.control.len());
                for (i, c) in piece.control.iter().enumerate() {
                    control.push(match (i, start, end, old) {
                        (0, Some(p), _, _) => p,
                        (0, None, _, Some((s, _))) => s,
                        (i, _, Some(p), _) if i == last => p,
                        (i, _, None, Some((_, e))) if i == last => e,
                        _ => self.add_point(*c),
                    });
                }
                // Old inner points no other curve uses go.
                let old_inner: Vec<EntityUid> = kind
                    .points()
                    .into_iter()
                    .filter(|p| old.is_none_or(|(s, e)| *p != s && *p != e))
                    .collect();
                let kind = EntityKind::Spline {
                    degree: piece.degree as u32,
                    control,
                    weights: piece.weights.clone(),
                    knots: piece.knots.clone(),
                };
                self.def.entity_mut(id).expect("a curve").kind = kind;
                for p in old_inner {
                    self.drop_if_unused(p);
                }
                return Ok(());
            }
            other => return Err(format!("a {} cannot be trimmed", other.type_name())),
        };
        self.def.entity_mut(id).expect("a curve").kind = new_kind;
        Ok(())
    }

    /// Extends the end of a line or arc nearest `at` to the nearest curve
    /// beyond it (Extend). An arc extended all the way round
    /// becomes a circle. An end that reaches an existing point joins it
    /// (mitcad#259): the point becomes the curve's end, or holds it with a
    /// point-to-point coincident constraint when it is projected or computed
    /// (see `points.rs`); returns that point. Elsewhere the end is a new
    /// point on the curve reached.
    pub fn extend(&mut self, id: EntityUid, at: P2) -> Result<Option<EntityUid>, String> {
        self.shape_editable(id)?;
        let solved = self.solved()?;
        let curve = solved
            .curves
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("curve c{} does not exist", id.0))?;
        let kind = self.curve(id)?.kind.clone();
        let (start, end) = curve.ends().ok_or("only lines and arcs can be extended")?;
        let at_end = dist(at, end) <= dist(at, start);
        let size = sketch_size(&solved.curves) * 10.0;
        let tol = 1e-9 * size;
        // The extension as a curve of its own.
        let (extension, from) = match &curve {
            Curve2::Line { a, b } => {
                let d = unit(sub(*b, *a));
                if at_end {
                    (
                        Curve2::Line {
                            a: *b,
                            b: add(*b, scale(d, size)),
                        },
                        *b,
                    )
                } else {
                    (
                        Curve2::Line {
                            a: *a,
                            b: sub(*a, scale(d, size)),
                        },
                        *a,
                    )
                }
            }
            Curve2::Arc {
                center,
                radius,
                start: a0,
                end: a1,
            } => {
                let gap = TAU - (a1 - a0);
                let (s, e) = if at_end {
                    (*a1, a1 + gap)
                } else {
                    (a0 - gap, *a0)
                };
                (
                    Curve2::Arc {
                        center: *center,
                        radius: *radius,
                        start: s,
                        end: e,
                    },
                    if at_end { end } else { start },
                )
            }
            _ => return Err("only lines and arcs can be extended".to_owned()),
        };
        let mut best: Option<(f64, P2, EntityUid)> = None;
        for (other, c) in &solved.curves {
            if *other == id {
                continue;
            }
            for hit in intersections(&extension, c, tol) {
                let d = dist(hit.point, from);
                // Distance along the extension.
                let along = match &extension {
                    Curve2::Line { .. } => d,
                    Curve2::Arc {
                        start: s, end: e, ..
                    } => {
                        if at_end {
                            hit.ta - s
                        } else {
                            e - hit.ta
                        }
                    }
                    _ => d,
                };
                if along > 1e-9 && best.is_none_or(|(b, ..)| along < b) {
                    best = Some((along, hit.point, *other));
                }
            }
        }
        let Some((_, point, target)) = best else {
            return Err("nothing to extend to".to_owned());
        };
        let (old_start, old_end) = kind.ends().expect("open");
        let moved = if at_end { old_end } else { old_start };
        self.drop_extent_constraints(id);
        // A full turn closes the arc.
        if let (EntityKind::Arc { center, .. }, true) = (
            &kind,
            dist(point, if at_end { start } else { end }) <= tol * 10.0,
        ) {
            let radius = dist(self.at(*center)?, point);
            let center = *center;
            self.def.entity_mut(id).expect("a curve").kind = EntityKind::Circle {
                center,
                radius,
                start: 0.0,
            };
            self.drop_if_unused(old_start);
            self.drop_if_unused(old_end);
            return Ok(None);
        }
        let reached = self.point_reached(point, &solved.points, target, id, tol * 10.0);
        // The end point stays, with the constraints and dimensions on it
        // (mitcad#362), when this curve alone uses it and the extension
        // joins no existing point; only the extent constraints went above.
        let keeps = !matches!(reached, Some((_, true)))
            && self.users(moved) == [id]
            && self.mergeable_point(moved)
            && self.def.entity(moved).is_some_and(|e| !e.fixed);
        if keeps {
            if let Some(e) = self.def.entity_mut(moved) {
                e.kind = EntityKind::Point { at: point };
            }
            let on = match reached {
                Some((q, _)) => self.computing_offset(q).is_none().then_some(Ref::Point(q)),
                None => (self.computing_offset(target).is_none()).then_some(Ref::Curve(target)),
            };
            if let Some(entity) = on {
                self.add_constraint(ConstraintKind::Coincident {
                    point: moved,
                    entity,
                });
            }
            // Dimensions of the moved end measure the new place.
            let mut after = solved.clone();
            after.points.insert(moved, point);
            if let Some(Curve2::Line { a, b }) = after.curves.get_mut(&id) {
                *(if at_end { b } else { a }) = point;
            }
            self.remeasure_dimensions_on(moved, &after);
            return Ok(reached.map(|(q, _)| q));
        }
        let p = match reached {
            Some((q, true)) => q,
            Some((q, false)) => {
                let p = self.add_point(point);
                if self.computing_offset(q).is_none() {
                    self.add_constraint(ConstraintKind::Coincident {
                        point: p,
                        entity: Ref::Point(q),
                    });
                }
                p
            }
            None => self.point_on_crossing(point, target),
        };
        let (s, e) = if at_end {
            (None, Some(p))
        } else {
            (Some(p), None)
        };
        let (lo, hi) = curve.domain();
        self.reshape(id, &curve, lo, hi, s, e)?;
        self.drop_if_unused(moved);
        Ok(reached.map(|(q, _)| q))
    }

    // Fillet and chamfer.

    /// The corner of two lines: their shared point, or where they meet; the
    /// end of each line at the corner and the direction from the corner
    /// along each line.
    fn corner(&self, a: EntityUid, b: EntityUid) -> Result<Corner, String> {
        let ends = |l: EntityUid| -> Result<(EntityUid, EntityUid), String> {
            match self.curve(l)?.kind {
                EntityKind::Line { start, end, .. } => Ok((start, end)),
                ref other => Err(crate::said(
                    format!("the curve is a {}, not a line", other.type_name()),
                    format!("c{} is a {}, not a line", l.0, other.type_name()),
                )),
            }
        };
        let (a0, a1) = ends(a)?;
        let (b0, b1) = ends(b)?;
        let (pa0, pa1, pb0, pb1) = (self.at(a0)?, self.at(a1)?, self.at(b0)?, self.at(b1)?);
        let (da, db) = (sub(pa1, pa0), sub(pb1, pb0));
        let denom = cross(da, db);
        if denom.abs() <= 1e-12 * norm(da) * norm(db) {
            return Err("the lines are parallel".to_owned());
        }
        let s = cross(sub(pb0, pa0), db) / denom;
        let point = add(pa0, scale(da, s));
        // The end of each line nearer the corner.
        let near = |p0: EntityUid, p1: EntityUid, q0: P2, q1: P2| {
            if dist(q0, point) <= dist(q1, point) {
                (p0, q1)
            } else {
                (p1, q0)
            }
        };
        let (end_a, far_a) = near(a0, a1, pa0, pa1);
        let (end_b, far_b) = near(b0, b1, pb0, pb1);
        Ok(Corner {
            point,
            end_a,
            end_b,
            dir_a: unit(sub(far_a, point)),
            dir_b: unit(sub(far_b, point)),
            len_a: dist(far_a, point),
            len_b: dist(far_b, point),
        })
    }

    /// Keeps the sharp corner of a fillet or chamfer as a point on both
    /// lines (a virtual sharp): the lines' shared end point when they
    /// had one, else a new point. Constraints on it stay, and a length
    /// dimension of either line becomes the distance from its far end to
    /// the corner, so the dimensions mean what they did.
    fn keep_corner(&mut self, corner: &Corner, a: EntityUid, b: EntityUid) -> EntityUid {
        let shared = corner.end_a == corner.end_b && self.def.entity(corner.end_a).is_some();
        let p = if shared {
            let p = corner.end_a;
            if let Some(EntityKind::Point { at }) = self.def.entity_mut(p).map(|e| &mut e.kind) {
                *at = corner.point;
            }
            p
        } else {
            for old in [corner.end_a, corner.end_b] {
                self.drop_if_unused(old);
            }
            self.add_point(corner.point)
        };
        for line in [a, b] {
            self.add_constraint(ConstraintKind::Coincident {
                point: p,
                entity: Ref::Curve(line),
            });
            let far = match self.def.entity(line).map(|e| &e.kind) {
                Some(EntityKind::Line { start, end, .. }) => {
                    let (s, e) = (*start, *end);
                    let near_end = self
                        .at(e)
                        .ok()
                        .zip(self.at(s).ok())
                        .is_some_and(|(pe, ps)| dist(pe, corner.point) < dist(ps, corner.point));
                    if near_end { s } else { e }
                }
                _ => continue,
            };
            for d in &mut self.def.dimensions {
                if d.kind == (DimensionKind::Length { line }) {
                    d.kind = DimensionKind::Distance { a: far, b: p };
                }
            }
        }
        p
    }

    /// Replaces a line's end at a corner with a new point.
    fn move_end(&mut self, line: EntityUid, old: EntityUid, new: EntityUid) {
        let e = self.def.entity_mut(line).expect("a line");
        e.kind.replace_point(old, new);
    }

    /// The value (mm or rad) a command value has now.
    fn value_of(&self, value: &ValueInput, unit: crate::expr::Unit) -> Result<f64, String> {
        match value {
            ValueInput::Number(n) => Ok(*n),
            ValueInput::Name(text) => self
                .params
                .table()
                .evaluate(text, unit)
                .map(|q| q.value)
                .map_err(|e| e.to_string()),
        }
    }

    /// Rounds the corner of two lines with an arc of `radius` tangent to
    /// both (sketch Fillet), dimensioned by its radius. The corner
    /// stays as a point on both lines, as in [`SketchEdit::chamfer`].
    /// A circular arc and a line or another arc (mitcad#1192) are rounded
    /// at the point where they end together.
    pub fn fillet(
        &mut self,
        a: EntityUid,
        b: EntityUid,
        radius: &ValueInput,
    ) -> Result<EntityUid, String> {
        self.shape_editable(a)?;
        self.shape_editable(b)?;
        let r = self.fillet_radius(radius)?;
        let arc = if self.is_line(a) && self.is_line(b) {
            self.fillet_arc(a, b, None, r)?
        } else {
            let at = self.corner_point_of(a, b)?;
            if !self.is_fillet_corner(a, b, at)? {
                return Err(format!(
                    "c{} and c{} are tangent at p{}: there is no corner to round",
                    a.0, b.0, at.0
                ));
            }
            self.check_fillets_fit(&[(a, b, Some(at))], r)?;
            self.fillet_arc(a, b, Some(at), r)?
        };
        self.add_dimension(
            DimensionKind::Radius { curve: arc },
            Some(radius),
            false,
            Some("fillet radius"),
        )?;
        Ok(arc)
    }

    /// The value of a fillet's radius, which must be positive.
    fn fillet_radius(&self, radius: &ValueInput) -> Result<f64, String> {
        let r = self.value_of(radius, crate::expr::Unit::MM)?;
        if !(r.is_finite() && r > 0.0) {
            return Err(format!("the fillet radius must be positive, got {r}"));
        }
        Ok(r)
    }

    /// Rounds several corners at once (mitcad#867): every corner where two
    /// of `curves` end at one point, and every one of `points` that ends
    /// exactly two lines (construction lines count only among
    /// construction lines, so a rectangle's diagonal does not spoil its
    /// corners). Circular arcs count as lines do (mitcad#1192). Each
    /// corner gets an arc of `radius` as in [`SketchEdit::fillet`]; the
    /// first arc has the radius dimension and the others are Equal to it,
    /// so the one dimension drives them all. Two lines without a shared
    /// end are one corner where they meet. The arcs are returned in the
    /// order of the corners.
    pub fn fillet_corners(
        &mut self,
        curves: &[EntityUid],
        points: &[EntityUid],
        radius: &ValueInput,
    ) -> Result<Vec<EntityUid>, String> {
        let r = self.fillet_radius(radius)?;
        let corners = self.corners_of(curves, points)?;
        for &(a, b, _) in &corners {
            self.shape_editable(a)?;
            self.shape_editable(b)?;
        }
        if let [(a, b, None)] = corners[..] {
            // Two lines that meet where they would cross.
            return Ok(vec![self.fillet(a, b, radius)?]);
        }
        self.check_fillets_fit(&corners, r)?;
        let mut arcs = Vec::with_capacity(corners.len());
        for &(a, b, at) in &corners {
            arcs.push(self.fillet_arc(a, b, at, r)?);
        }
        self.add_dimension(
            DimensionKind::Radius { curve: arcs[0] },
            Some(radius),
            false,
            Some("fillet radius"),
        )?;
        for &arc in &arcs[1..] {
            self.add_constraint(ConstraintKind::Equal { a: arcs[0], b: arc });
        }
        Ok(arcs)
    }

    /// A rectangle (as [`SketchEdit::rectangle`]) with its four corners
    /// rounded by `radius` (mitcad#867), as [`SketchEdit::fillet_corners`]
    /// rounds them: one radius dimension and the arcs Equal. Returns the
    /// four lines and the four arcs. A radius of half the shorter side or
    /// more is refused.
    pub fn rounded_rectangle(
        &mut self,
        mode: super::tools::RectangleMode,
        construction: bool,
        radius: &ValueInput,
    ) -> Result<(Vec<EntityUid>, Vec<EntityUid>), String> {
        let r = self.fillet_radius(radius)?;
        let lines = self.rectangle(mode, construction)?;
        let mut shorter = f64::INFINITY;
        for &l in &lines {
            let (s, e) = self.line_ends(l)?;
            shorter = shorter.min(dist(self.at(s)?, self.at(e)?));
        }
        if r >= shorter / 2.0 {
            return Err(format!(
                "the corner radius {} mm must be less than half the shorter side ({} mm)",
                short(r),
                short(shorter / 2.0)
            ));
        }
        let arcs = self.fillet_corners(&lines, &[], radius)?;
        Ok((lines, arcs))
    }

    /// The ends of a line, or why it is not one.
    fn line_ends(&self, id: EntityUid) -> Result<(EntityUid, EntityUid), String> {
        match self.curve(id)?.kind {
            EntityKind::Line { start, end, .. } => Ok((start, end)),
            ref other => Err(format!(
                "c{} is a {}: Sketch Fillet rounds corners of lines",
                id.0,
                other.type_name()
            )),
        }
    }

    /// Whether a curve is a line.
    fn is_line(&self, id: EntityUid) -> bool {
        matches!(
            self.def.entity(id).map(|e| &e.kind),
            Some(EntityKind::Line { .. })
        )
    }

    /// The ends of a curve whose corners Sketch Fillet rounds: a line or a
    /// circular arc (mitcad#1192), or why it is neither.
    fn fillet_ends(&self, id: EntityUid) -> Result<(EntityUid, EntityUid), String> {
        match self.curve(id)?.kind {
            EntityKind::Line { start, end, .. } | EntityKind::Arc { start, end, .. } => {
                Ok((start, end))
            }
            ref other => Err(not_filleted(id, other)),
        }
    }

    /// The point where two curves end together, the corner of an arc's
    /// fillet (mitcad#1192).
    fn corner_point_of(&self, a: EntityUid, b: EntityUid) -> Result<EntityUid, String> {
        let (a0, a1) = self.fillet_ends(a)?;
        let (b0, b1) = self.fillet_ends(b)?;
        let mut shared: Vec<EntityUid> = [a0, a1]
            .into_iter()
            .filter(|p| *p == b0 || *p == b1)
            .collect();
        shared.dedup();
        if shared.is_empty() {
            // Two ends held together at one place (mitcad#1192).
            shared = [a0, a1]
                .into_iter()
                .filter(|&p| [b0, b1].into_iter().any(|q| self.held_ends(p, q, a, b)))
                .collect();
        }
        match shared[..] {
            [p] => Ok(p),
            [] => Err(format!(
                "c{} and c{} do not end at one point: an arc is rounded where it ends \
                 together with the other curve",
                a.0, b.0
            )),
            _ => Err(format!(
                "c{} and c{} end together at p{} and p{}: pick the corner point to round",
                a.0, b.0, shared[0].0, shared[1].0
            )),
        }
    }

    /// Whether the end `p` of the curve `x` and the end `q` of the curve
    /// `y` are one corner although they are two points (mitcad#1192), as a
    /// Trim or an import leaves them: at the same place (within [`GAP`])
    /// and held together by a point-to-point coincident, or by a
    /// point-on-curve of one end on the other curve.
    fn held_ends(&self, p: EntityUid, q: EntityUid, x: EntityUid, y: EntityUid) -> bool {
        let near = matches!((self.at(p), self.at(q)), (Ok(a), Ok(b)) if dist(a, b) <= GAP);
        p != q
            && x != y
            && near
            && self.def.constraints.iter().any(|c| {
                c.active
                    && match c.kind {
                        ConstraintKind::Coincident {
                            point,
                            entity: Ref::Point(other),
                        } => (point == p && other == q) || (point == q && other == p),
                        ConstraintKind::Coincident {
                            point,
                            entity: Ref::Curve(on),
                        } => (point == p && on == y) || (point == q && on == x),
                        _ => false,
                    }
            })
    }

    /// The other side of a corner at the end `p` of the line or arc `x`
    /// that no other curve shares (mitcad#1192): another line or arc whose
    /// end is held to `p` ([`SketchEdit::held_ends`]), with that end; one
    /// of the two curves an arc, of one kind (drawn or construction).
    fn held_partner(&self, p: EntityUid, x: EntityUid) -> Option<(EntityUid, EntityUid)> {
        let entity = self.def.entity(x)?;
        let arc_x = matches!(entity.kind, EntityKind::Arc { .. });
        self.def.entities.iter().find_map(|e| {
            let arc = match e.kind {
                EntityKind::Arc { .. } => true,
                EntityKind::Line { .. } => false,
                _ => return None,
            };
            if e.construction != entity.construction || !(arc || arc_x) {
                return None;
            }
            let (s, t) = e.kind.ends()?;
            [s, t]
                .into_iter()
                .find(|&q| self.held_ends(p, q, x, e.id))
                .map(|q| (e.id, q))
        })
    }

    /// The end of `curve` at the corner `at` of `curve` and `other`: `at`
    /// itself, or the end of `curve` held to it ([`SketchEdit::held_ends`]).
    fn corner_end(
        &self,
        curve: EntityUid,
        other: EntityUid,
        at: EntityUid,
    ) -> Result<EntityUid, String> {
        let (s, e) = self.fillet_ends(curve)?;
        if s == at || e == at {
            return Ok(at);
        }
        [s, e]
            .into_iter()
            .find(|&q| self.held_ends(at, q, other, curve))
            .ok_or_else(|| {
                crate::said(
                    "the point is not an end of the curve",
                    format!("p{} is no end of c{}", at.0, curve.0),
                )
            })
    }

    /// The corner at `at` where the curves `a` and `b` (lines or circular
    /// arcs) end together, or where an end of one is held to `at`, the end
    /// of the other: its place and both curves as they leave it.
    fn fillet_legs(
        &self,
        a: EntityUid,
        b: EntityUid,
        at: EntityUid,
    ) -> Result<(P2, Leg, Leg), String> {
        let corner = self.at(at)?;
        let (end_a, end_b) = (self.corner_end(a, b, at)?, self.corner_end(b, a, at)?);
        Ok((
            corner,
            self.fillet_leg(a, end_a, corner)?,
            self.fillet_leg(b, end_b, corner)?,
        ))
    }

    /// A line or a circular arc as it leaves its end `at` (at `corner`).
    fn fillet_leg(&self, curve: EntityUid, at: EntityUid, corner: P2) -> Result<Leg, String> {
        let not_an_end = || {
            crate::said(
                "the point is not an end of the curve",
                format!("p{} is no end of c{}", at.0, curve.0),
            )
        };
        match self.curve(curve)?.kind {
            EntityKind::Line { start, end, .. } => {
                let far = match (start == at, end == at) {
                    (true, _) => end,
                    (_, true) => start,
                    _ => return Err(not_an_end()),
                };
                let far = self.at(far)?;
                Ok(Leg::Line {
                    dir: unit(sub(far, corner)),
                    len: dist(far, corner),
                })
            }
            EntityKind::Arc { center, start, end } => {
                // Counter-clockwise from its start, clockwise from its end.
                let ccw = match (start == at, end == at) {
                    (true, _) => true,
                    (_, true) => false,
                    _ => return Err(not_an_end()),
                };
                let c = self.at(center)?;
                let (s, e) = (sub(self.at(start)?, c), sub(self.at(end)?, c));
                let mut sweep = cross(s, e).atan2(dot(s, e));
                if sweep <= 0.0 {
                    sweep += TAU;
                }
                Ok(Leg::Arc {
                    center: c,
                    radius: dist(corner, c),
                    ccw,
                    sweep,
                })
            }
            ref other => Err(not_filleted(curve, other)),
        }
    }

    /// Whether the curves `a` and `b` that end together at `at` make a
    /// corner to round: lines not in line, curves not tangent there.
    fn is_fillet_corner(&self, a: EntityUid, b: EntityUid, at: EntityUid) -> Result<bool, String> {
        if self.is_line(a) && self.is_line(b) {
            return Ok(self.corner(a, b).is_ok());
        }
        let (corner, leg_a, leg_b) = self.fillet_legs(a, b, at)?;
        Ok(is_corner(corner, &leg_a, &leg_b))
    }

    /// The corners (pairs of lines or circular arcs, with the point where
    /// they end together) that `curves` and `points` name for
    /// [`SketchEdit::fillet_corners`], each once.
    fn corners_of(
        &self,
        curves: &[EntityUid],
        points: &[EntityUid],
    ) -> Result<Vec<FilletCorner>, String> {
        let mut lines: Vec<EntityUid> = Vec::new();
        for &c in curves {
            self.fillet_ends(c)?;
            if !lines.contains(&c) {
                lines.push(c);
            }
        }
        let mut corners: Vec<FilletCorner> = Vec::new();
        let mut at: Vec<EntityUid> = Vec::new();
        // Where two of the curves of one kind end together (a construction
        // diagonal at a rectangle's corner is no part of the corner); a
        // point where three or more of them end is no corner of two.
        let construction = |l: EntityUid| self.def.entity(l).is_some_and(|e| e.construction);
        let mut ending: BTreeMap<(EntityUid, bool), Vec<EntityUid>> = BTreeMap::new();
        for &l in &lines {
            let (s, e) = self.fillet_ends(l)?;
            for p in [s, e] {
                let list = ending.entry((p, construction(l))).or_default();
                if !list.contains(&l) {
                    list.push(l);
                }
            }
        }
        for &l in &lines {
            let (s, e) = self.fillet_ends(l)?;
            for p in [s, e] {
                let list = &ending[&(p, construction(l))];
                if list.len() > 2 {
                    return Err(format!(
                        "{} of the curves end at p{}: pick the corners of two curves",
                        list.len(),
                        p.0
                    ));
                }
                if list.len() == 2 && list[0] == l && !at.contains(&p) {
                    // Lines in line (a line drawn in two) and curves
                    // tangent where they meet make no corner.
                    if self.is_fillet_corner(list[0], list[1], p)? {
                        at.push(p);
                        corners.push((list[0], list[1], Some(p)));
                    }
                }
            }
        }
        // Ends of two of the curves held together at one place, as a Trim
        // leaves them (mitcad#1192).
        for &x in &lines {
            let (s, e) = self.fillet_ends(x)?;
            for p in [s, e] {
                let Some((y, q)) = self.held_partner(p, x) else {
                    continue;
                };
                let alone = |p: EntityUid, c: EntityUid| ending[&(p, construction(c))].len() == 1;
                if at.contains(&p)
                    || at.contains(&q)
                    || !lines.contains(&y)
                    || !alone(p, x)
                    || !alone(q, y)
                {
                    continue;
                }
                if self.is_fillet_corner(x, y, p)? {
                    at.extend([p, q]);
                    corners.push((x, y, Some(p)));
                }
            }
        }
        for &p in points {
            match self.def.entity(p) {
                Some(e) if e.is_point() => {}
                _ => return Err(format!("point p{} does not exist", p.0)),
            }
            if at.contains(&p) {
                continue;
            }
            // The curves that end there, of one kind: the drawn ones, else
            // the construction ones.
            let ends_at = |kind: bool| {
                self.def
                    .entities
                    .iter()
                    .filter(|e| {
                        e.construction == kind
                            && e.kind.ends().is_some_and(|(s, t)| s == p || t == p)
                    })
                    .collect::<Vec<&Entity>>()
            };
            let drawn = ends_at(false);
            let ends_here = if drawn.is_empty() {
                ends_at(true)
            } else {
                drawn
            };
            let rounds =
                |e: &Entity| matches!(e.kind, EntityKind::Line { .. } | EntityKind::Arc { .. });
            match ends_here[..] {
                [a, b] if rounds(a) && rounds(b) => {
                    if self.is_line(a.id) && self.is_line(b.id) {
                        self.corner(a.id, b.id).map_err(|e| {
                            crate::said(
                                format!("the point is no corner: {}", notice_message(&e)),
                                format!("p{} is no corner: {e}", p.0),
                            )
                        })?;
                    } else if !self.is_fillet_corner(a.id, b.id, p)? {
                        return Err(format!(
                            "p{} is no corner: c{} and c{} are tangent there",
                            p.0, a.id.0, b.id.0
                        ));
                    }
                    at.push(p);
                    corners.push((a.id, b.id, Some(p)));
                }
                // One curve ends there, held to the end of another
                // (mitcad#1192).
                [x] if rounds(x) && self.held_partner(p, x.id).is_some() => {
                    let (y, q) = self.held_partner(p, x.id).expect("a partner");
                    if at.contains(&q) {
                        continue;
                    }
                    if !self.is_fillet_corner(x.id, y, p)? {
                        return Err(format!(
                            "p{} is no corner: c{} and c{} are tangent there",
                            p.0, x.id.0, y.0
                        ));
                    }
                    at.extend([p, q]);
                    corners.push((x.id, y, Some(p)));
                }
                _ => {
                    return Err(format!(
                        "p{} is not the corner of two lines or arcs ({} curve(s) end there)",
                        p.0,
                        ends_here.len()
                    ));
                }
            }
        }
        if corners.is_empty() {
            if let ([a, b], []) = (&lines[..], points) {
                if self.is_line(*a) && self.is_line(*b) {
                    // Two lines that do not end together: where they meet.
                    return Ok(vec![(*a, *b, None)]);
                }
                // An arc that does not end with the other curve, is
                // tangent to it where it does, or is of the other kind.
                let at = self.corner_point_of(*a, *b)?;
                if self.is_fillet_corner(*a, *b, at)? {
                    return Err(format!(
                        "c{} and c{} are not both drawn or both construction curves",
                        a.0, b.0
                    ));
                }
                return Err(format!(
                    "c{} and c{} are tangent at p{}: there is no corner to round",
                    a.0, b.0, at.0
                ));
            }
            return Err(
                "the selection has no corner where two lines or arcs end together".to_owned(),
            );
        }
        Ok(corners)
    }

    /// Refuses fillets of `r` at `corners` (of lines or arcs with shared
    /// ends) that would take more of a curve than it has, naming the curve
    /// and the largest radius that fits it.
    fn check_fillets_fit(&self, corners: &[FilletCorner], r: f64) -> Result<(), String> {
        // What the corners take of each curve, and its length.
        let mut taken: BTreeMap<EntityUid, (f64, f64)> = BTreeMap::new();
        for &corner in corners {
            let Some(takes) = self.fillet_takes(corner, r)? else {
                // No arc of r touches the curves ahead of the corner: an
                // arc's circle too small to hold it.
                let (a, b, _) = corner;
                let (arc, other) = if self.is_line(a) { (b, a) } else { (a, b) };
                return Err(format!(
                    "the fillet radius {} mm does not fit on c{} at its corner with c{}, \
                     the radius must be less than {} mm",
                    short(r),
                    arc.0,
                    other.0,
                    short(self.largest_fillet(corners, arc, r)?)
                ));
            };
            for (curve, back, len) in takes {
                let entry = taken.entry(curve).or_insert((0.0, len));
                entry.0 += back;
                entry.1 = entry.1.max(len);
            }
        }
        for (curve, (need, len)) in taken {
            if need >= len {
                // A line's corners with lines take of it in proportion to
                // the radius.
                let linear = corners
                    .iter()
                    .filter(|(a, b, _)| *a == curve || *b == curve)
                    .all(|(a, b, _)| self.is_line(*a) && self.is_line(*b));
                let largest = if linear {
                    r * len / need
                } else {
                    self.largest_fillet(corners, curve, r)?
                };
                return Err(format!(
                    "the fillet radius {} mm does not fit on c{}: its corners take {} mm of its {} mm, \
                     the radius must be less than {} mm",
                    short(r),
                    curve.0,
                    short(need),
                    short(len),
                    short(largest)
                ));
            }
        }
        Ok(())
    }

    /// What a fillet of `r` takes of each of its two curves at a corner,
    /// from the corner along the curve (an arc's length), with the
    /// curve's length: None where no arc of `r` touches both curves
    /// ahead of the corner.
    fn fillet_takes(
        &self,
        (a, b, at): FilletCorner,
        r: f64,
    ) -> Result<Option<[FilletTake; 2]>, String> {
        if self.is_line(a) && self.is_line(b) {
            let corner = self.corner(a, b)?;
            let half = dot(corner.dir_a, corner.dir_b).clamp(-1.0, 1.0).acos() / 2.0;
            let back = r / half.tan();
            return Ok(Some([(a, back, corner.len_a), (b, back, corner.len_b)]));
        }
        let at = match at {
            Some(p) => p,
            None => self.corner_point_of(a, b)?,
        };
        let (corner, leg_a, leg_b) = self.fillet_legs(a, b, at)?;
        Ok(fillet_at(corner, &leg_a, &leg_b, r).map(|f| {
            [
                (a, f.along[0], leg_a.length()),
                (b, f.along[1], leg_b.length()),
            ]
        }))
    }

    /// The largest radius below `r` whose fillets at the corners of
    /// `curve` among `corners` fit on it, by bisection (the bound, which a
    /// message rounds to a micrometre): an arc's fillets do not take of
    /// it in proportion to the radius.
    fn largest_fillet(
        &self,
        corners: &[FilletCorner],
        curve: EntityUid,
        r: f64,
    ) -> Result<f64, String> {
        let mine: Vec<FilletCorner> = corners
            .iter()
            .copied()
            .filter(|(a, b, _)| *a == curve || *b == curve)
            .collect();
        let fits = |s: f64| -> Result<bool, String> {
            let (mut need, mut len) = (0.0, 0.0_f64);
            for &corner in &mine {
                let Some(takes) = self.fillet_takes(corner, s)? else {
                    return Ok(false);
                };
                for (c, back, l) in takes {
                    if c == curve {
                        need += back;
                        len = len.max(l);
                    }
                }
            }
            Ok(need < len)
        };
        let (mut lo, mut hi) = (0.0_f64, r);
        for _ in 0..64 {
            let mid = 0.5 * (lo + hi);
            if fits(mid)? {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Ok(lo)
    }

    /// Rounds a corner with an arc of `r` tangent to both curves, without
    /// a dimension; the corner stays as a point on both. Two lines meet at
    /// their shared end or where they would cross; a corner with a
    /// circular arc is the point `at` where both end (mitcad#1192).
    fn fillet_arc(
        &mut self,
        a: EntityUid,
        b: EntityUid,
        at: Option<EntityUid>,
        r: f64,
    ) -> Result<EntityUid, String> {
        if !(self.is_line(a) && self.is_line(b)) {
            let at = match at {
                Some(p) => p,
                None => self.corner_point_of(a, b)?,
            };
            return self.fillet_curves(a, b, at, r);
        }
        let corner = self.corner(a, b)?;
        let half = dot(corner.dir_a, corner.dir_b).clamp(-1.0, 1.0).acos() / 2.0;
        let back = r / half.tan();
        if back >= corner.len_a || back >= corner.len_b {
            return Err("the fillet does not fit on the lines".to_owned());
        }
        let ta = add(corner.point, scale(corner.dir_a, back));
        let tb = add(corner.point, scale(corner.dir_b, back));
        let bisector = unit(add(corner.dir_a, corner.dir_b));
        let center = add(corner.point, scale(bisector, r / half.sin()));
        let arc = self.id();
        let pa = self.add_point(ta);
        let pb = self.add_point(tb);
        let c = self.add_point(center);
        self.move_end(a, corner.end_a, pa);
        self.move_end(b, corner.end_b, pb);
        let (start, end) = if cross(sub(ta, center), sub(tb, center)) > 0.0 {
            (pa, pb)
        } else {
            (pb, pa)
        };
        self.push(Entity::new(
            arc,
            EntityKind::Arc {
                center: c,
                start,
                end,
            },
        ));
        self.add_constraint(ConstraintKind::tangent(a, arc));
        self.add_constraint(ConstraintKind::tangent(b, arc));
        self.keep_corner(&corner, a, b);
        Ok(arc)
    }

    /// Rounds the corner at `at`, where a circular arc ends together with
    /// a line or another arc (mitcad#1192), with an arc of `r` tangent to
    /// both, without a dimension. Of the circles of `r` tangent to both,
    /// the one on the side where the two curves bound the corner (between
    /// the directions in which they leave it), nearest to it. Both curves
    /// are trimmed back to where the arc touches them; an arc keeps its
    /// centre and radius (and their constraints and dimensions), and its
    /// arc length dimension and the midpoints on it, which measured the
    /// piece cut off, go. The corner stays as a point on both curves.
    fn fillet_curves(
        &mut self,
        a: EntityUid,
        b: EntityUid,
        at: EntityUid,
        r: f64,
    ) -> Result<EntityUid, String> {
        let (corner, leg_a, leg_b) = self.fillet_legs(a, b, at)?;
        let (end_a, end_b) = (self.corner_end(a, b, at)?, self.corner_end(b, a, at)?);
        let fillet = fillet_at(corner, &leg_a, &leg_b, r)
            .filter(|f| f.along[0] < leg_a.length() && f.along[1] < leg_b.length())
            .ok_or_else(|| {
                format!(
                    "the fillet radius {} mm does not fit at p{}",
                    short(r),
                    at.0
                )
            })?;
        let arc = self.id();
        let pa = self.add_point(fillet.touch[0]);
        let pb = self.add_point(fillet.touch[1]);
        let c = self.add_point(fillet.center);
        self.move_end(a, end_a, pa);
        self.move_end(b, end_b, pb);
        // Counter-clockwise from one touch to the other over the side that
        // faces the corner.
        let angle = |p: P2| {
            let v = sub(p, fillet.center);
            v[1].atan2(v[0])
        };
        let turn = |from: P2, to: P2| (angle(to) - angle(from)).rem_euclid(TAU);
        let [ta, tb] = fillet.touch;
        let (start, end) = if turn(ta, corner) < turn(ta, tb) {
            (pa, pb)
        } else {
            (pb, pa)
        };
        self.push(Entity::new(
            arc,
            EntityKind::Arc {
                center: c,
                start,
                end,
            },
        ));
        self.add_constraint(ConstraintKind::tangent(a, arc));
        self.add_constraint(ConstraintKind::tangent(b, arc));
        for curve in [a, b] {
            if !self.is_line(curve) {
                self.drop_extent_constraints(curve);
            }
        }
        self.keep_shared_corner([(a, end_a, pa), (b, end_b, pb)]);
        Ok(arc)
    }

    /// Keeps the shared end of two curves whose corner a fillet rounds as a
    /// point on both (mitcad#1192), as [`SketchEdit::keep_corner`] does for
    /// two lines: constraints on it stay, and a length dimension of a line
    /// becomes the distance from its other end to the corner. `curves` are
    /// the two curves with their old ends at the corner and their new ends
    /// at the fillet. Two old ends held together (two points,
    /// [`SketchEdit::held_ends`]) stay so, each on its own curve.
    fn keep_shared_corner(&mut self, curves: [(EntityUid, EntityUid, EntityUid); 2]) {
        for (curve, p, new_end) in curves {
            self.add_constraint(ConstraintKind::Coincident {
                point: p,
                entity: Ref::Curve(curve),
            });
            let far = match self.def.entity(curve).map(|e| &e.kind) {
                Some(EntityKind::Line { start, end, .. }) => {
                    if *start == new_end {
                        *end
                    } else {
                        *start
                    }
                }
                _ => continue,
            };
            for d in &mut self.def.dimensions {
                if d.kind == (DimensionKind::Length { line: curve }) {
                    d.kind = DimensionKind::Distance { a: far, b: p };
                }
            }
        }
    }

    /// Bevels the corner of two lines with a line (sketch Chamfer):
    /// `distance` along the first line and `distance2` (or `distance`)
    /// along the second, or `distance` and an `angle` from the first line.
    /// The corner stays as a point on both lines, so that the distances
    /// are dimensions.
    pub fn chamfer(
        &mut self,
        a: EntityUid,
        b: EntityUid,
        distance: &ValueInput,
        second: Option<&ValueInput>,
        angle: Option<&ValueInput>,
    ) -> Result<EntityUid, String> {
        self.shape_editable(a)?;
        self.shape_editable(b)?;
        let value = |v: &ValueInput, unit| self.value_of(v, unit);
        let d1 = value(distance, crate::expr::Unit::MM)?;
        let corner = self.corner(a, b)?;
        let d2 = match (second, angle) {
            (Some(_), Some(_)) => {
                return Err("give a second distance or an angle, not both".to_owned());
            }
            (Some(v), None) => value(v, crate::expr::Unit::MM)?,
            (None, Some(v)) => {
                // The chamfer line leaves the first line at the angle.
                let angle = value(v, crate::expr::Unit::DEG)?;
                let corner_angle = dot(corner.dir_a, corner.dir_b).clamp(-1.0, 1.0).acos();
                let third = std::f64::consts::PI - angle - corner_angle;
                if !(angle > 0.0 && third > 0.0) {
                    return Err("the chamfer angle does not fit the corner".to_owned());
                }
                d1 * angle.sin() / third.sin()
            }
            (None, None) => d1,
        };
        if !(d1.is_finite() && d1 > 0.0 && d2.is_finite() && d2 > 0.0) {
            return Err("the chamfer distances must be positive".to_owned());
        }
        if d1 >= corner.len_a || d2 >= corner.len_b {
            return Err("the chamfer does not fit on the lines".to_owned());
        }
        let line = self.id();
        let pa = self.add_point(add(corner.point, scale(corner.dir_a, d1)));
        let pb = self.add_point(add(corner.point, scale(corner.dir_b, d2)));
        self.move_end(a, corner.end_a, pa);
        self.move_end(b, corner.end_b, pb);
        self.push(Entity::new(
            line,
            EntityKind::Line {
                start: pa,
                end: pb,
                centerline: false,
            },
        ));
        let p = self.keep_corner(&corner, a, b);
        self.add_dimension(
            DimensionKind::Distance { a: p, b: pa },
            Some(distance),
            false,
            Some("chamfer distance"),
        )?;
        match (second, angle) {
            (_, Some(angle)) => {
                self.add_dimension(
                    DimensionKind::Angle {
                        a,
                        b: line,
                        reverse_a: None,
                        reverse_b: None,
                    },
                    Some(angle),
                    false,
                    Some("chamfer angle"),
                )?;
            }
            (Some(v), None) => {
                self.add_dimension(
                    DimensionKind::Distance { a: p, b: pb },
                    Some(v),
                    false,
                    Some("chamfer distance"),
                )?;
            }
            (None, None) => {
                // The same parameter for both.
                let first = self.def.dimensions.last().and_then(|d| d.value);
                let name = first.map(|id| self.params.name(id)).unwrap_or_default();
                self.add_dimension(
                    DimensionKind::Distance { a: p, b: pb },
                    Some(&ValueInput::Name(name)),
                    false,
                    None,
                )?;
            }
        }
        Ok(line)
    }

    // Offset.

    /// Offsets a chain of connected curves, or a closed curve, by
    /// `distance` to the right of the chain's direction (from the first
    /// curve to the next; circles and ellipses run counter-clockwise, so a
    /// positive distance goes out). The offset is a record
    /// (`k<n>`) whose parameter holds the distance (see `offset.rs`).
    ///
    /// Lines and arcs are offset exactly with constraints: corners of the
    /// copy meet, each copied line is parallel to its original at the
    /// distance, each arc shares its original's centre with a construction
    /// line of that length across the gap. A chain with an ellipse or a
    /// spline is derived: computed from its source after every solve.
    pub fn offset(
        &mut self,
        curves: &[EntityUid],
        distance: &ValueInput,
    ) -> Result<(ConstraintUid, Vec<EntityUid>), String> {
        let d = self.value_of(distance, crate::expr::Unit::MM)?;
        if !(d.is_finite() && d != 0.0) {
            return Err("the offset distance must not be zero".to_owned());
        }
        let magnitude = match distance {
            ValueInput::Name(text) if d < 0.0 => ValueInput::Name(format!("-({text})")),
            other => other.clone(),
        };
        self.offset_with(curves, &magnitude, d)
    }

    /// An offset by `d` (signed, now) whose parameter `distance` gives (its
    /// magnitude).
    pub(crate) fn offset_with(
        &mut self,
        curves: &[EntityUid],
        distance: &ValueInput,
        d: f64,
    ) -> Result<(ConstraintUid, Vec<EntityUid>), String> {
        for c in curves {
            self.editable(*c)?;
        }
        let solved = self.offset_solve()?;
        let chain = self.chain(curves, &solved)?;
        // A curve taken in place of the same one picked (mitcad#741).
        for (c, _) in &chain.curves {
            self.editable(*c)?;
        }
        self.report.warnings.extend(chain.warnings.iter().cloned());
        let exact = match chain.curves.as_slice() {
            [(id, _)] if matches!(self.curve(*id)?.kind, EntityKind::Circle { .. }) => true,
            list => list.iter().all(|(id, _)| {
                self.def.entity(*id).is_some_and(|e| {
                    matches!(e.kind, EntityKind::Line { .. } | EntityKind::Arc { .. })
                })
            }),
        };
        if !exact {
            return self.derived_offset(&chain.curves, chain.closed, distance, d);
        }
        let mut sources: Vec<(Curve2, bool)> = Vec::new();
        for (id, forward) in &chain.curves {
            let c = solved
                .curves
                .get(id)
                .cloned()
                .ok_or("a curve without geometry")?;
            sources.push((c, *forward));
        }
        // What vanishes at the distance is left out, and the curves on
        // either side meet where their offsets cross (mitcad#1234); lines on
        // one line across it are one, and the outline may fall into several
        // (mitcad#1244).
        let vanishing = if sources.len() > 1 {
            let name = |i: usize| format!("c{}", chain.curves[i].0.0);
            super::offset::exact_vanishing(&sources, d, chain.closed, &name)?
        } else {
            super::offset::Vanishing::none(1, false)
        };
        // Where a curve's offset ends at a corner across what vanished.
        let corner_at: BTreeMap<EntityUid, P2> = vanishing
            .across
            .iter()
            .map(|(i, p)| (chain.curves[*i].0, *p))
            .collect();
        let curves: Vec<(EntityUid, bool)> = chain
            .curves
            .iter()
            .enumerate()
            .filter(|(i, _)| !vanishing.dropped.contains(i))
            .map(|(_, c)| *c)
            .collect();
        let dropped: Vec<EntityUid> = vanishing
            .dropped
            .iter()
            .map(|i| chain.curves[*i].0)
            .collect();
        let merged: Vec<super::offset::OffsetMerge> = vanishing
            .merged
            .iter()
            .map(|&(curve, into)| super::offset::OffsetMerge {
                curve: chain.curves[curve].0,
                into: chain.curves[into].0,
            })
            .collect();
        // Left out as a part too small for the distance, and the arcs whose
        // offset runs round its whole circle (mitcad#1246).
        let small: Vec<EntityUid> = vanishing.left.iter().map(|i| chain.curves[*i].0).collect();
        let whole: BTreeSet<EntityUid> =
            vanishing.whole.iter().map(|i| chain.curves[*i].0).collect();
        // The curve after each along its outline, and where the last of an
        // open chain or outline ends when others run on along it.
        let next = vanishing.next;
        let reach: BTreeMap<EntityUid, P2> = vanishing
            .reach
            .iter()
            .map(|(i, p)| (chain.curves[*i].0, *p))
            .collect();
        let id = self.constraint_id();
        let first = self.report.constraints.len();
        let param = self.offset_param(id, distance, None)?;
        let parameter = Some(self.params.name(param));
        // The offset of each curve, in chain direction.
        let mut pieces: Vec<Offset> = Vec::new();
        for (id, forward) in &curves {
            let c = solved
                .curves
                .get(id)
                .cloned()
                .ok_or("a curve without geometry")?;
            pieces.push(match c {
                Curve2::Line { a, b } => {
                    let (a, b) = if *forward { (a, b) } else { (b, a) };
                    let right = scale(perp(unit(sub(b, a))), -d);
                    Offset::Line(add(a, right), add(b, right))
                }
                Curve2::Arc {
                    center,
                    radius,
                    start,
                    end,
                } => {
                    // Counter-clockwise travel has the centre on the left.
                    let r = if *forward { radius + d } else { radius - d };
                    if r <= 0.0 {
                        return Err("the offset is larger than an arc's radius".to_owned());
                    }
                    let (s, e) = if *forward { (start, end) } else { (end, start) };
                    if whole.contains(id) {
                        Offset::Circle(center, r)
                    } else {
                        Offset::Arc {
                            center,
                            radius: r,
                            start: s,
                            end: e,
                            ccw: *forward,
                        }
                    }
                }
                Curve2::Circle { center, radius, .. } if chain.curves.len() == 1 => {
                    let r = radius + d;
                    if r <= 0.0 {
                        return Err("the offset is larger than the circle's radius".to_owned());
                    }
                    Offset::Circle(center, r)
                }
                _ => {
                    return Err(crate::said(
                        "a curve cannot be offset exactly",
                        format!("c{} cannot be offset exactly", id.0),
                    ));
                }
            });
        }
        let n = pieces.len();
        let mut helpers = Vec::new();
        let ids = self.result_ids(&curves);
        // Corner points where offset curves that follow each other meet:
        // each curve's start and end.
        let mut starts: Vec<Option<EntityUid>> = vec![None; n];
        let mut ends: Vec<Option<EntityUid>> = vec![None; n];
        // A circle has no corners (a circle offset alone, or an arc's offset
        // round its whole circle, mitcad#1246).
        let round = |i: usize| matches!(pieces[i], Offset::Circle(..));
        for i in 0..n {
            let Some(j) = next.get(i).copied().flatten() else {
                continue;
            };
            if round(i) || round(j) {
                continue;
            }
            // Across a curve that vanished, where the offsets cross.
            let near = corner_at
                .get(&curves[i].0)
                .copied()
                .unwrap_or_else(|| pieces[i].end());
            let p = meet(&pieces[i], &pieces[j], near).ok_or_else(|| {
                format!(
                    "the offsets of c{} and c{} do not meet at their corner",
                    curves[i].0.0, curves[j].0.0
                )
            })?;
            let corner = self.add_point(p);
            ends[i] = Some(corner);
            starts[j] = Some(corner);
        }
        for i in 0..n {
            if starts[i].is_none() && !round(i) {
                starts[i] = Some(self.add_point(pieces[i].start()));
            }
        }
        for i in 0..n {
            if ends[i].is_none() && !round(i) {
                let end = reach
                    .get(&curves[i].0)
                    .copied()
                    .unwrap_or_else(|| pieces[i].end());
                ends[i] = Some(self.add_point(end));
            }
        }
        for (i, piece) in pieces.iter().enumerate() {
            let (original, _) = curves[i];
            let kind = match piece {
                Offset::Line(..) => EntityKind::Line {
                    start: starts[i].expect("a corner"),
                    end: ends[i].expect("a corner"),
                    centerline: false,
                },
                Offset::Arc { ccw, .. } => {
                    let center = self.center_of(original)?;
                    let (start, end) = (starts[i].expect("a corner"), ends[i].expect("a corner"));
                    if *ccw {
                        EntityKind::Arc { center, start, end }
                    } else {
                        EntityKind::Arc {
                            center,
                            start: end,
                            end: start,
                        }
                    }
                }
                Offset::Circle(_, r) => EntityKind::Circle {
                    center: self.center_of(original)?,
                    radius: *r,
                    start: 0.0,
                },
            };
            self.push(Entity::new(ids[i], kind));
            let value = ValueInput::Name(parameter.clone().expect("made above"));
            match piece {
                Offset::Line(..) => {
                    self.add_constraint(ConstraintKind::Parallel {
                        a: original,
                        b: ids[i],
                    });
                    self.add_dimension(
                        DimensionKind::LineDistance {
                            a: original,
                            b: ids[i],
                        },
                        Some(&value),
                        false,
                        Some("offset"),
                    )?;
                }
                Offset::Arc { center, radius, .. } | Offset::Circle(center, radius) => {
                    // Across an arc's middle (that of an arc whose offset
                    // runs round its whole circle too), a circle's angle 0.
                    let (old_radius, angle) = match solved.curves.get(&original) {
                        Some(Curve2::Arc {
                            radius, start, end, ..
                        }) => (*radius, (start + end) / 2.0),
                        Some(Curve2::Circle { radius, .. }) => (*radius, 0.0),
                        _ => return Err("an arc without geometry".to_owned()),
                    };
                    // A construction line across the gap, along a radius.
                    let p0 = self.point_on(Offset::at(*center, old_radius, angle), original);
                    let p1 = self.point_on(Offset::at(*center, *radius, angle), ids[i]);
                    let across = self.id();
                    helpers.push(across);
                    let mut line = Entity::new(
                        across,
                        EntityKind::Line {
                            start: p0,
                            end: p1,
                            centerline: false,
                        },
                    );
                    line.construction = true;
                    self.push(line);
                    let c = self.center_of(original)?;
                    self.add_constraint(ConstraintKind::Coincident {
                        point: c,
                        entity: Ref::Curve(across),
                    });
                    self.add_dimension(
                        DimensionKind::Length { line: across },
                        Some(&value),
                        false,
                        Some("offset"),
                    )?;
                }
            }
        }
        let (outlines, open) = super::offset::outlines_of(&next, &ids, chain.closed);
        self.note_trimmed(&dropped, &merged, &small, outlines.len());
        self.def.offsets.push(super::offset::SketchOffset {
            id,
            curves: chain.curves.iter().map(|(c, _)| *c).collect(),
            distance: param,
            left: d < 0.0,
            results: ids.clone(),
            dropped,
            merged,
            outlines,
            ends: open,
            corners: Vec::new(),
            helpers,
            derived: false,
        });
        // The offset's id first in the result, before its own constraints.
        self.report.constraints.insert(first, id);
        Ok((id, ids))
    }

    /// The curves in the order of the chain they make, each with whether it
    /// runs from its start to its end along it, as [`SketchEdit::offset`]
    /// takes them.
    pub(crate) fn chain_order(
        &self,
        curves: &[EntityUid],
        solved: &Solved,
    ) -> Result<Vec<(EntityUid, bool)>, String> {
        Ok(self.chain(curves, solved)?.curves)
    }

    /// The centre point of a circle or an arc.
    fn center_of(&self, id: EntityUid) -> Result<EntityUid, String> {
        match self.curve(id)?.kind {
            EntityKind::Arc { center, .. } | EntityKind::Circle { center, .. } => Ok(center),
            ref other => Err(crate::said(
                format!(
                    "the curve is a {}, not a circle or an arc",
                    other.type_name()
                ),
                format!("c{} is a {}", id.0, other.type_name()),
            )),
        }
    }

    /// Orders curves into a chain through shared end points. Curves that
    /// are the same (a curve projected twice, mitcad#741) count once: of
    /// such a pair the one that shares its ends with more of the others is
    /// taken, picked or not, and a warning says so. Fails with the reason
    /// when they are no chain; an open chain that ends where another curve
    /// ends without sharing its point warns.
    fn chain(&self, curves: &[EntityUid], solved: &Solved) -> Result<Chain, String> {
        if curves.is_empty() {
            return Err("no curves to offset".to_owned());
        }
        for id in curves {
            self.curve(*id)?;
        }
        let size = sketch_size(&solved.curves);
        let mut warnings = Vec::new();
        let (curves, twins) = self.without_twins(curves, solved, size, &mut warnings);
        let mut ends: Vec<(EntityUid, Option<(EntityUid, EntityUid)>)> = Vec::new();
        for id in &curves {
            ends.push((*id, self.curve(*id)?.kind.ends()));
        }
        if ends.len() == 1 {
            let closed = ends[0].1.is_none_or(|(s, e)| s == e);
            if let (false, Some((s, e))) = (closed, ends[0].1) {
                warnings.extend(self.loose_ends(&[s, e], &curves, &twins, solved, size));
            }
            return Ok(Chain {
                curves: vec![(ends[0].0, true)],
                closed,
                warnings,
            });
        }
        if ends.iter().any(|(_, e)| e.is_none()) {
            return Err("a closed curve is offset on its own".to_owned());
        }
        // Start at a curve with a free end, if any.
        let degree = |p: EntityUid| {
            ends.iter()
                .filter(|(_, e)| matches!(e, Some((s, t)) if *s == p || *t == p))
                .count()
        };
        let first = ends
            .iter()
            .position(|(_, e)| {
                let (s, t) = e.expect("open");
                degree(s) == 1 || degree(t) == 1
            })
            .unwrap_or(0);
        let (s0, t0) = ends[first].1.expect("open");
        let forward = degree(s0) == 1 || ends.len() < 2 || {
            // Closed: go from the first curve toward the second.
            let (s1, t1) = ends[(first + 1) % ends.len()].1.expect("open");
            t0 == s1 || t0 == t1
        };
        let mut order = vec![(ends[first].0, forward)];
        let mut used = vec![false; ends.len()];
        used[first] = true;
        let mut at = if forward { t0 } else { s0 };
        while order.len() < ends.len() {
            let next = (0..ends.len())
                .find(|i| !used[*i] && matches!(ends[*i].1, Some((s, t)) if s == at || t == at));
            let Some(i) = next else {
                return Err(self.not_a_chain(&ends, &used, solved, size));
            };
            used[i] = true;
            let (s, t) = ends[i].1.expect("open");
            let fwd = s == at;
            at = if fwd { t } else { s };
            order.push((ends[i].0, fwd));
        }
        let start = if forward { s0 } else { t0 };
        let closed = at == start;
        if !closed {
            warnings.extend(self.loose_ends(&[start, at], &curves, &twins, solved, size));
        }
        Ok(Chain {
            curves: order,
            closed,
            warnings,
        })
    }

    /// The curves without repeats (mitcad#741): each curve the same as
    /// another of the sketch (within [`super::project::SAME`] of its size)
    /// is one of the pair, the one that shares its ends with more of the
    /// other curves; picked first when that is as good. Also the repeats
    /// that were not taken, picked or not.
    fn without_twins(
        &self,
        curves: &[EntityUid],
        solved: &Solved,
        size: f64,
        warnings: &mut Vec<String>,
    ) -> (Vec<EntityUid>, BTreeSet<EntityUid>) {
        let tol = super::project::SAME * size;
        let ends = |id: EntityUid| self.def.entity(id).and_then(|e| e.kind.ends());
        let construction = |id: EntityUid| self.def.entity(id).is_some_and(|e| e.construction);
        let twins_of = |id: EntityUid| -> Vec<EntityUid> {
            let Some(c) = solved.curves.get(&id) else {
                return Vec::new();
            };
            solved
                .curves
                .iter()
                .filter(|(other, o)| {
                    **other != id
                        && construction(**other) == construction(id)
                        && ends(**other).is_some() == ends(id).is_some()
                        && super::project::same_curve(c, o, tol)
                })
                .map(|(other, _)| *other)
                .filter(|other| {
                    self.def.entity(*other).is_some_and(|e| e.axis.is_none())
                        && self.editable(*other).is_ok()
                })
                .collect()
        };
        // How many of `others` a curve shares an end point with.
        let joins = |id: EntityUid, others: &[EntityUid]| -> usize {
            let Some((s, t)) = ends(id) else {
                return 0;
            };
            others
                .iter()
                .filter(|o| ends(**o).is_some_and(|(a, b)| a == s || a == t || b == s || b == t))
                .count()
        };
        let mut out: Vec<EntityUid> = Vec::new();
        let mut left: BTreeSet<EntityUid> = BTreeSet::new();
        let mut handled: BTreeSet<EntityUid> = BTreeSet::new();
        for id in curves {
            if !handled.insert(*id) {
                continue;
            }
            let group: Vec<EntityUid> = std::iter::once(*id).chain(twins_of(*id)).collect();
            if group.len() == 1 {
                out.push(*id);
                continue;
            }
            handled.extend(group.iter().copied());
            let others: Vec<EntityUid> = curves
                .iter()
                .copied()
                .filter(|c| !group.contains(c))
                .collect();
            let mut best = *id;
            let mut score = joins(best, &others);
            for g in &group[1..] {
                let s = joins(*g, &others);
                if s > score {
                    best = *g;
                    score = s;
                }
            }
            out.push(best);
            for g in group.iter().filter(|g| **g != best) {
                left.insert(*g);
                if curves.contains(g) {
                    let projected = |c: EntityUid| self.def.entity(c).is_some_and(|e| e.reference);
                    warnings.push(format!(
                        "c{} lies on c{} (the same curve twice): c{} is offset{}",
                        g.0,
                        best.0,
                        best.0,
                        if projected(*g) && projected(best) {
                            "; Clean Up Projections removes the repeat"
                        } else {
                            ""
                        }
                    ));
                }
            }
        }
        (out, left)
    }

    /// Why curves are no chain: where they branch, or the curve that does
    /// not join the others and how far its nearest end is from theirs.
    fn not_a_chain(
        &self,
        ends: &[(EntityUid, Option<(EntityUid, EntityUid)>)],
        used: &[bool],
        solved: &Solved,
        size: f64,
    ) -> String {
        let all: Vec<EntityUid> = ends
            .iter()
            .filter_map(|(_, e)| *e)
            .flat_map(|(s, t)| [s, t])
            .collect();
        let degree = |p: EntityUid| {
            ends.iter()
                .filter(|(_, e)| matches!(e, Some((s, t)) if *s == p || *t == p))
                .count()
        };
        if let Some(p) = all.iter().find(|p| degree(**p) > 2) {
            return crate::said(
                format!(
                    "the curves are not one connected chain: {} of them end at one point (it \
                     branches there)",
                    degree(*p)
                ),
                format!(
                    "the curves are not one connected chain: {} of them end at p{} (it branches \
                     there)",
                    degree(*p),
                    p.0
                ),
            );
        }
        let at = |p: EntityUid| solved.points.get(&p).copied();
        // The nearest pair of ends of a curve left over and one taken.
        let mut nearest: Option<(f64, EntityUid, EntityUid, EntityUid, EntityUid)> = None;
        for (i, (u, e)) in ends.iter().enumerate() {
            let Some((s, t)) = e else { continue };
            if used[i] {
                continue;
            }
            for (j, (v, f)) in ends.iter().enumerate() {
                let Some((a, b)) = f else { continue };
                if !used[j] {
                    continue;
                }
                for p in [*s, *t] {
                    for q in [*a, *b] {
                        if let (Some(x), Some(y)) = (at(p), at(q)) {
                            let d = dist(x, y);
                            if nearest.is_none_or(|n| d < n.0) {
                                nearest = Some((d, *u, p, *v, q));
                            }
                        }
                    }
                }
            }
        }
        let Some((d, u, p, v, q)) = nearest else {
            return "the curves are not one connected chain".to_owned();
        };
        if d <= GAP {
            crate::said(
                format!(
                    "the curves are not one connected chain: a curve ends {} another curve's \
                     end but on a point of its own; a Coincident constraint joins them",
                    apart(d, size)
                ),
                format!(
                    "the curves are not one connected chain: c{} ends {} p{} of c{} but on a \
                     point of its own (p{}); a Coincident constraint joins them",
                    u.0,
                    apart(d, size),
                    q.0,
                    v.0,
                    p.0
                ),
            )
        } else {
            crate::said(
                format!(
                    "the curves are not one connected chain: a curve does not join the others \
                     (its nearest end is {} mm from another curve)",
                    gap_text(d)
                ),
                format!(
                    "the curves are not one connected chain: c{} does not join the others (its \
                     nearest end is {} mm from c{})",
                    u.0,
                    gap_text(d),
                    v.0
                ),
            )
        }
    }

    /// Warnings for the free ends of an open chain (`points`) where a curve
    /// that is not in it (nor the same as one in it) ends within [`GAP`]
    /// without sharing the point: the chain stops there (mitcad#741).
    fn loose_ends(
        &self,
        points: &[EntityUid],
        chain: &[EntityUid],
        twins: &BTreeSet<EntityUid>,
        solved: &Solved,
        size: f64,
    ) -> Vec<String> {
        let mut out = Vec::new();
        // Chain Selection joins curves of the chain's kind only.
        let construction = chain
            .first()
            .and_then(|c| self.def.entity(*c))
            .is_some_and(|e| e.construction);
        for p in points {
            let Some(x) = solved.points.get(p).copied() else {
                continue;
            };
            let other = self.def.entities.iter().find_map(|e| {
                if chain.contains(&e.id) || twins.contains(&e.id) || e.construction != construction
                {
                    return None;
                }
                let (s, t) = e.kind.ends()?;
                [s, t].into_iter().find_map(|q| {
                    let y = solved.points.get(&q).copied()?;
                    (q != *p && dist(x, y) <= GAP).then_some((e.id, q, dist(x, y)))
                })
            });
            if let Some((c, q, d)) = other {
                let place = if d <= 1e-9 * size {
                    "at the same place".to_owned()
                } else {
                    format!("{} mm away", gap_text(d))
                };
                out.push(crate::said(
                    format!(
                        "the chain ends where another curve ends {place} on a point of its own; \
                         a Coincident constraint joins them"
                    ),
                    format!(
                        "the chain ends at p{}, where c{} ends {place} on a point of its own \
                         (p{}); a Coincident constraint joins them",
                        p.0, c.0, q.0
                    ),
                ));
            }
        }
        out
    }
}

/// How far apart, millimetres, curve ends may be for the reason of a chain
/// that stops or breaks to name them as meant to meet: the import's
/// geometric tolerance (mitcad#224).
const GAP: f64 = 0.01;

/// A length in a message: rounded to a micrometre, without trailing zeros.
fn short(v: f64) -> String {
    let rounded = (v * 1e6).round() / 1e6;
    format!("{}", rounded + 0.0)
}

/// A small distance for a message: `2.0e-5` or `0.012`.
fn gap_text(d: f64) -> String {
    if d < 1e-3 {
        format!("{d:.1e}")
    } else {
        format!("{d:.3}")
    }
}

/// "at" or "2.0e-5 mm from", for ends `d` apart in a sketch of `size`.
fn apart(d: f64, size: f64) -> String {
    if d <= 1e-9 * size {
        "at".to_owned()
    } else {
        format!("{} mm from", gap_text(d))
    }
}

/// A corner Sketch Fillet rounds: two curves and the point where they end
/// together (none for two lines that meet where they would cross).
type FilletCorner = (EntityUid, EntityUid, Option<EntityUid>);

/// What a fillet takes of one curve at its corner: the curve, the length
/// taken from the corner along it, and the curve's length.
type FilletTake = (EntityUid, f64, f64);

/// Why Sketch Fillet does not round a corner of a curve.
fn not_filleted(id: EntityUid, kind: &EntityKind) -> String {
    format!(
        "c{} is a {}: Sketch Fillet rounds corners of lines and circular arcs",
        id.0,
        kind.type_name()
    )
}

struct Corner {
    point: P2,
    end_a: EntityUid,
    end_b: EntityUid,
    dir_a: P2,
    dir_b: P2,
    len_a: f64,
    len_b: f64,
}

/// Where a curve crossing another cuts it (Trim).
struct Cut {
    /// The parameter on the cut curve.
    t: f64,
    /// The crossing curve.
    curve: EntityUid,
    /// How far an existing point may lie from the cut and still become the
    /// end there (mitcad#1191).
    reach: f64,
}

/// How far an existing point may lie from a trim's cut where the curves
/// touch (are tangent) and still become the end there (mitcad#1191),
/// relative to the sketch size. A point the solve holds on two touching
/// curves within its tolerance (1e-9 of the size) can lie off the contact
/// along them by about the square root of twice that, so the computed
/// contact is only that accurate: two arcs trimmed one after the other at
/// their contact were left with ends 2e-6 mm apart.
const TANGENT_REACH: f64 = 5e-5;

/// How far from parallel (the sine of the angle between them) the
/// directions of two curves may be where they meet and still count as
/// touching: a trim's cut where the curves touch, and a cut end joined to a
/// curve it meets tangentially (mitcad#1220).
const TANGENT_JOIN: f64 = 1e-3;

struct Chain {
    /// Curves in order, and whether each runs forward along the chain.
    curves: Vec<(EntityUid, bool)>,
    closed: bool,
    /// What the offset is to say (mitcad#741): a repeat left out, a loose
    /// end.
    warnings: Vec<String>,
}

/// An offset curve in chain direction.
enum Offset {
    Line(P2, P2),
    Arc {
        center: P2,
        radius: f64,
        /// Angles in chain direction.
        start: f64,
        end: f64,
        ccw: bool,
    },
    Circle(P2, f64),
}

impl Offset {
    fn at(center: P2, r: f64, a: f64) -> P2 {
        [center[0] + r * a.cos(), center[1] + r * a.sin()]
    }

    fn start(&self) -> P2 {
        match self {
            Self::Line(a, _) => *a,
            Self::Arc {
                center,
                radius,
                start,
                ..
            } => Self::at(*center, *radius, *start),
            Self::Circle(c, r) => Self::at(*c, *r, 0.0),
        }
    }

    fn end(&self) -> P2 {
        match self {
            Self::Line(_, b) => *b,
            Self::Arc {
                center,
                radius,
                end,
                ..
            } => Self::at(*center, *radius, *end),
            Self::Circle(c, r) => Self::at(*c, *r, 0.0),
        }
    }

    fn as_curve(&self) -> Curve2 {
        match self {
            // Long enough to meet a neighbour beyond its end.
            Self::Line(a, b) => {
                let d = sub(*b, *a);
                Curve2::Line {
                    a: sub(*a, scale(d, 10.0)),
                    b: add(*b, scale(d, 10.0)),
                }
            }
            Self::Arc { center, radius, .. } | Self::Circle(center, radius) => Curve2::Circle {
                center: *center,
                radius: *radius,
                start: 0.0,
            },
        }
    }
}

/// Where two offset curves meet, nearest `near`.
fn meet(a: &Offset, b: &Offset, near: P2) -> Option<P2> {
    let (ca, cb) = (a.as_curve(), b.as_curve());
    let size = dist(a.start(), a.end())
        .max(dist(b.start(), b.end()))
        .max(1.0);
    let hits = intersections(&ca, &cb, 1e-9 * size * 100.0);
    if hits.is_empty() && dist(a.end(), b.start()) <= 1e-9 * size {
        return Some(a.end());
    }
    hits.into_iter()
        .map(|h| h.point)
        .min_by(|p, q| dist(*p, near).total_cmp(&dist(*q, near)))
}

/// The size of the sketch: the diagonal of all curves' bounds.
fn sketch_size(curves: &BTreeMap<EntityUid, Curve2>) -> f64 {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for c in curves.values() {
        let (lo, hi) = c.bounds();
        for k in 0..2 {
            min[k] = min[k].min(lo[k]);
            max[k] = max[k].max(hi[k]);
        }
    }
    if min[0].is_finite() {
        dist(min, max).max(1.0)
    } else {
        1.0
    }
}

/// Replaces entity references by their copies.
pub(super) fn remap(kind: &mut ConstraintKind, copies: &BTreeMap<EntityUid, EntityUid>) {
    let m = |u: &mut EntityUid| {
        if let Some(c) = copies.get(u) {
            *u = *c;
        }
    };
    let mr = |r: &mut Ref| match r {
        Ref::Point(u) | Ref::Curve(u) => {
            if let Some(c) = copies.get(u) {
                *u = *c;
            }
        }
    };
    use ConstraintKind as K;
    match kind {
        K::Coincident { point, entity } => {
            m(point);
            mr(entity);
        }
        K::Horizontal { line } | K::Vertical { line } => m(line),
        K::HorizontalPoints { a, b }
        | K::VerticalPoints { a, b }
        | K::Parallel { a, b }
        | K::Perpendicular { a, b }
        | K::Collinear { a, b }
        | K::Equal { a, b }
        | K::Concentric { a, b } => {
            m(a);
            m(b);
        }
        K::Tangent { a, b, at } | K::Smooth { a, b, at } => {
            m(a);
            m(b);
            if let Some(p) = at {
                m(p);
            }
        }
        K::Midpoint { point, curve } => {
            m(point);
            m(curve);
        }
        K::Symmetric { a, b, axis, .. } => {
            mr(a);
            mr(b);
            m(axis);
        }
        K::EqualSpacing {
            anchors, direction, ..
        } => {
            anchors.iter_mut().for_each(mr);
            if let Some(d) = direction {
                m(d);
            }
        }
    }
}

/// A point of a command: `[x, y]` or an existing point `"p3"`.
pub fn point_input(value: &serde_json::Value) -> Result<PointInput, String> {
    match value {
        serde_json::Value::String(text) => super::point_serde::parse(text)
            .map(PointInput::Existing)
            .map_err(|e| e.to_string()),
        serde_json::Value::Array(xy) if xy.len() == 2 => {
            let x = xy[0].as_f64().ok_or("a point is [x, y] or \"p<n>\"")?;
            let y = xy[1].as_f64().ok_or("a point is [x, y] or \"p<n>\"")?;
            Ok(PointInput::At([x, y]))
        }
        _ => Err("a point is [x, y] or \"p<n>\"".to_owned()),
    }
}

/// `-(expression)`, or the expression inside when it is that already, so
/// that switching a dimension back and forth does not nest negations.
fn negate_expression(expression: &str) -> String {
    let text = expression.trim();
    if let Some(inner) = text.strip_prefix("-(").and_then(|t| t.strip_suffix(')')) {
        // The parentheses must enclose all of it: `-(a) * (b)` is not one.
        let mut depth = 0i32;
        let enclosed = inner.chars().all(|c| {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            depth >= 0
        });
        if enclosed && depth == 0 {
            return inner.trim().to_owned();
        }
    }
    format!("-({text})")
}

/// "a constraint" or "3 constraints" (mitcad#1612: what a note counts,
/// its ids in the agents' detail).
fn counted(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!(
            "{} {one}",
            if one.starts_with(['a', 'e', 'i', 'o', 'u']) {
                "an"
            } else {
                "a"
            }
        )
    } else {
        format!("{n} {many}")
    }
}

/// The message a person reads of an error's text (its tail left out).
fn notice_message(text: &str) -> String {
    mitcad_model_base::notice::user_message(text)
}
