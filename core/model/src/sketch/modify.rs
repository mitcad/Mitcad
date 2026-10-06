// SPDX-License-Identifier: MIT
//! Modifying a sketch (the sketch's Modify menu and the constraint and
//! dimension edits): delete, construction and fixed flags, dimension values,
//! drag, move and copy, trim and extend, fillet and chamfer, offset, mirror
//! and sketch patterns. Like the drawing tools, they make entities and
//! constraints.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;

use super::edit::{PointInput, SketchEdit};
use super::geometry::{Curve2, P2, add, cross, dist, dot, norm, perp, scale, sub, unit};
use super::intersect::intersections;
use super::solve::{Goal, measure};
use super::{Constraint, ConstraintKind, ConstraintUid, DimensionKind, Entity, EntityKind, Ref};
use crate::document::resolve_value;
use crate::features::ValueInput;
use crate::ids::EntityUid;

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
        if let Some(n) = crate::ids::parse_number(text, "t") {
            return Ok(Self::Text(EntityUid(n)));
        }
        Err(format!(
            "'{text}' is not a point, curve, text, constraint or dimension (p3, c4, t5, k6)"
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
        let on_removed = |refs: Vec<Ref>| refs.iter().any(|r| entities.contains(&r.uid()));
        let constraints: BTreeSet<ConstraintUid> = self
            .def
            .constraints
            .iter()
            .filter(|c| on_removed(c.kind.refs()))
            .map(|c| c.id)
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
            if offset.curves.iter().any(|c| entities.contains(c)) || offset.results.is_empty() {
                gone.insert(offset.id);
            }
        }
        self.drop_constraints(&gone);
    }

    pub(crate) fn drop_constraints(&mut self, ids: &BTreeSet<ConstraintUid>) {
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
                other => return Err(format!("c{} is a {}", id.0, other.type_name())),
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
        let old = d.value;
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
        self.def.dimensions[i].value = Some(param);
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
            self.def.dimensions[i].value = None;
            return Ok(());
        }
        let solved = self.solved()?;
        let value = measure(&self.def.dimensions[i].kind, &solved)
            .ok_or("the dimension cannot be measured")?;
        self.set_dimension(id, &ValueInput::Number(value))
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
                        return Err(format!("c{} is not a circle", c.0));
                    }
                    goals.push(Goal::Radius(c, r));
                }
            }
        }
        self.drag_goals(&goals)
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
        let mut goals = Vec::new();
        for p in points {
            goals.push(Goal::Point(p, map.apply(self.at(p)?)));
        }
        self.drag_goals(&goals)
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
                );
                if refs.iter().all(|r| copies.contains_key(&r.uid())) && !(turned && directional) {
                    let mut kind = c.kind.clone();
                    remap(&mut kind, &copies);
                    if kind != c.kind {
                        self.add_constraint(kind);
                    }
                }
            }
        }
        Ok(copies)
    }

    /// Mirrors entities about a line (Mirror): copies with
    /// symmetric constraints; points on the axis are shared.
    pub fn mirror(&mut self, entities: &[Ref], axis: EntityUid) -> Result<Vec<Ref>, String> {
        let (a, b) = match self.curve(axis)?.kind {
            EntityKind::Line { start, end, .. } => (self.at(start)?, self.at(end)?),
            _ => return Err(format!("the mirror line c{} is not a line", axis.0)),
        };
        if entities.iter().any(|r| r.uid() == axis) {
            return Err("the mirror line cannot be mirrored".to_owned());
        }
        let map = Map2::mirror(a, b);
        let d = unit(sub(b, a));
        let size = dist(a, b).max(1.0);
        let mut keep = BTreeSet::new();
        for p in self.points_of(entities)? {
            if cross(d, sub(self.at(p)?, a)).abs() <= 1e-9 * size {
                keep.insert(p);
            }
        }
        let copies = self.copy_entities(entities, &map, &keep, false)?;
        let mut made = Vec::new();
        for r in entities {
            let original = r.uid();
            let copy = copies[&original];
            let kind = self.entity(original)?.kind.clone();
            match kind {
                EntityKind::Point { .. } => {
                    if copy != original {
                        self.add_constraint(ConstraintKind::Symmetric {
                            a: Ref::Point(original),
                            b: Ref::Point(copy),
                            axis,
                        });
                    }
                }
                EntityKind::Line { .. } | EntityKind::Circle { .. } | EntityKind::Arc { .. } => {
                    self.add_constraint(ConstraintKind::Symmetric {
                        a: Ref::Curve(original),
                        b: Ref::Curve(copy),
                        axis,
                    });
                }
                other => {
                    for p in other.points() {
                        if copies[&p] != p {
                            self.add_constraint(ConstraintKind::Symmetric {
                                a: Ref::Point(p),
                                b: Ref::Point(copies[&p]),
                                axis,
                            });
                        }
                    }
                }
            }
            made.push(self.entity(copy)?.as_ref());
        }
        Ok(made)
    }

    // Trim and extend.

    /// Where other curves cross a curve: (parameter, the crossing curve),
    /// sorted along it, ends of an open curve left out.
    fn crossings(&self, id: EntityUid) -> Result<(Curve2, Vec<(f64, EntityUid)>), String> {
        let solved = self.solved()?;
        let curve = solved
            .curves
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("curve c{} does not exist", id.0))?;
        let size = sketch_size(&solved.curves);
        let tol = 1e-9 * size;
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
                if !at_end {
                    cuts.push((t, *other));
                }
            }
        }
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok((curve, cuts))
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

    /// Removes the dimensions that measure a curve's extent and the
    /// midpoint constraints on it, which a trim or an extend changes.
    fn drop_extent_constraints(&mut self, curve: EntityUid) {
        let ids: BTreeSet<ConstraintUid> = self
            .def
            .dimensions
            .iter()
            .filter(|d| {
                matches!(d.kind, DimensionKind::Length { line } if line == curve)
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
    /// it (Trim). A curve nothing crosses is deleted.
    pub fn trim(&mut self, id: EntityUid, at: P2) -> Result<(), String> {
        let (curve, cuts) = self.crossings(id)?;
        let kind = self.curve(id)?.kind.clone();
        let (t_at, _) = curve.closest(at);
        let (lo, hi) = curve.domain();
        if curve.is_closed() {
            if cuts.len() < 2 {
                return self.remove(&[Item::Entity(Ref::Curve(id))]);
            }
            // The cuts around the pick; the rest becomes an arc.
            let after = cuts.iter().position(|(t, _)| *t > t_at).unwrap_or(0);
            let before = (after + cuts.len() - 1) % cuts.len();
            let (t_end, c_end) = cuts[before];
            let (t_start, c_start) = cuts[after];
            let start = self.point_on(curve.point(t_start), c_start);
            let end = self.point_on(curve.point(t_end), c_end);
            self.drop_extent_constraints(id);
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
                _ => return Err("closed splines cannot be trimmed yet".to_owned()),
            };
            return Ok(());
        }
        let below = cuts.iter().rev().find(|(t, _)| *t < t_at).copied();
        let above = cuts.iter().find(|(t, _)| *t > t_at).copied();
        let (old_start, old_end) = kind.ends().expect("an open curve");
        self.drop_extent_constraints(id);
        match (below, above) {
            (None, None) => return self.remove(&[Item::Entity(Ref::Curve(id))]),
            (None, Some((t, c))) => {
                // Cut off the start.
                let p = self.point_on(curve.point(t), c);
                self.reshape(id, &curve, t, hi, Some(p), None)?;
                self.drop_if_unused(old_start);
            }
            (Some((t, c)), None) => {
                let p = self.point_on(curve.point(t), c);
                self.reshape(id, &curve, lo, t, None, Some(p))?;
                self.drop_if_unused(old_end);
            }
            (Some((t0, c0)), Some((t1, c1))) => {
                // Keep both ends: this curve up to t0, a new one from t1.
                let p0 = self.point_on(curve.point(t0), c0);
                let p1 = self.point_on(curve.point(t1), c1);
                let copy = self.id();
                let mut second = self.curve(id)?.clone();
                second.id = copy;
                self.push(second);
                self.reshape(copy, &curve, t1, hi, Some(p1), None)?;
                self.reshape(id, &curve, lo, t0, None, Some(p0))?;
                match kind {
                    EntityKind::Line { .. } => {
                        self.add_constraint(ConstraintKind::Collinear { a: id, b: copy });
                    }
                    EntityKind::Arc { .. } | EntityKind::EllipticalArc { .. } => {}
                    _ => {}
                }
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
                let piece = n.piece(t0, t1);
                let (old_s, old_e) = kind.ends().expect("open");
                let last = piece.control.len() - 1;
                let mut control = Vec::with_capacity(piece.control.len());
                for (i, c) in piece.control.iter().enumerate() {
                    control.push(match (i, start, end) {
                        (0, Some(p), _) => p,
                        (0, None, _) => old_s,
                        (i, _, Some(p)) if i == last => p,
                        (i, _, None) if i == last => old_e,
                        _ => self.add_point(*c),
                    });
                }
                // Old inner points no other curve uses go.
                let old_inner: Vec<EntityUid> = kind
                    .points()
                    .into_iter()
                    .filter(|p| *p != old_s && *p != old_e)
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
    /// becomes a circle.
    pub fn extend(&mut self, id: EntityUid, at: P2) -> Result<(), String> {
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
            self.def.entity_mut(id).expect("a curve").kind = EntityKind::Circle { center, radius };
            self.drop_if_unused(old_start);
            self.drop_if_unused(old_end);
            return Ok(());
        }
        let p = self.point_on(point, target);
        let (s, e) = if at_end {
            (None, Some(p))
        } else {
            (Some(p), None)
        };
        let (lo, hi) = curve.domain();
        self.reshape(id, &curve, lo, hi, s, e)?;
        self.drop_if_unused(moved);
        Ok(())
    }

    // Fillet and chamfer.

    /// The corner of two lines: their shared point, or where they meet; the
    /// end of each line at the corner and the direction from the corner
    /// along each line.
    fn corner(&self, a: EntityUid, b: EntityUid) -> Result<Corner, String> {
        let ends = |l: EntityUid| -> Result<(EntityUid, EntityUid), String> {
            match self.curve(l)?.kind {
                EntityKind::Line { start, end, .. } => Ok((start, end)),
                ref other => Err(format!("c{} is a {}, not a line", l.0, other.type_name())),
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
    pub fn fillet(
        &mut self,
        a: EntityUid,
        b: EntityUid,
        radius: &ValueInput,
    ) -> Result<EntityUid, String> {
        let r = self.value_of(radius, crate::expr::Unit::MM)?;
        if !(r.is_finite() && r > 0.0) {
            return Err(format!("the fillet radius must be positive, got {r}"));
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
        self.add_constraint(ConstraintKind::Tangent { a, b: arc });
        self.add_constraint(ConstraintKind::Tangent { a: b, b: arc });
        self.keep_corner(&corner, a, b);
        self.add_dimension(
            DimensionKind::Radius { curve: arc },
            Some(radius),
            false,
            Some("fillet radius"),
        )?;
        Ok(arc)
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
                    DimensionKind::Angle { a, b: line },
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
        let chain = self.chain(curves)?;
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
        let id = self.constraint_id();
        let param = self.offset_param(id, distance, None)?;
        let parameter = Some(self.params.name(param));
        let solved = self.solved()?;
        // The offset of each curve, in chain direction.
        let mut pieces: Vec<Offset> = Vec::new();
        for (id, forward) in &chain.curves {
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
                    Offset::Arc {
                        center,
                        radius: r,
                        start: s,
                        end: e,
                        ccw: *forward,
                        middle: (start + end) / 2.0,
                    }
                }
                Curve2::Circle { center, radius } if chain.curves.len() == 1 => {
                    let r = radius + d;
                    if r <= 0.0 {
                        return Err("the offset is larger than the circle's radius".to_owned());
                    }
                    Offset::Circle(center, r)
                }
                _ => return Err(format!("c{} cannot be offset exactly", id.0)),
            });
        }
        let n = pieces.len();
        let ids = self.ids(n);
        // Corner points where neighbouring offset curves meet.
        let mut corners: Vec<Option<EntityUid>> = vec![None; n + 1];
        if !matches!(pieces[0], Offset::Circle(..)) {
            let pairs = if chain.closed { n } else { n - 1 };
            for i in 0..pairs {
                let j = (i + 1) % n;
                let p = meet(&pieces[i], &pieces[j], pieces[i].end())
                    .ok_or("the offset curves do not meet at a corner")?;
                corners[i + 1] = Some(self.add_point(p));
            }
            if chain.closed {
                corners[0] = corners[n];
            }
            if corners[0].is_none() {
                corners[0] = Some(self.add_point(pieces[0].start()));
            }
            if corners[n].is_none() {
                corners[n] = Some(self.add_point(pieces[n - 1].end()));
            }
        }
        for (i, piece) in pieces.iter().enumerate() {
            let (original, _) = chain.curves[i];
            let kind = match piece {
                Offset::Line(..) => EntityKind::Line {
                    start: corners[i].expect("a corner"),
                    end: corners[i + 1].expect("a corner"),
                    centerline: false,
                },
                Offset::Arc { ccw, .. } => {
                    let center = self.center_of(original)?;
                    let (start, end) = (
                        corners[i].expect("a corner"),
                        corners[i + 1].expect("a corner"),
                    );
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
                    let angle = match piece {
                        Offset::Arc { middle, .. } => *middle,
                        _ => 0.0,
                    };
                    let old_radius = match solved.curves.get(&original) {
                        Some(Curve2::Arc { radius, .. } | Curve2::Circle { radius, .. }) => *radius,
                        _ => return Err("an arc without geometry".to_owned()),
                    };
                    // A construction line across the gap, along a radius.
                    let p0 = self.point_on(Offset::at(*center, old_radius, angle), original);
                    let p1 = self.point_on(Offset::at(*center, *radius, angle), ids[i]);
                    let across = self.id();
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
        self.def.offsets.push(super::offset::SketchOffset {
            id,
            curves: chain.curves.iter().map(|(c, _)| *c).collect(),
            distance: param,
            left: d < 0.0,
            results: ids.clone(),
            corners: Vec::new(),
            derived: false,
        });
        self.report.constraints.push(id);
        Ok((id, ids))
    }

    /// The centre point of a circle or an arc.
    fn center_of(&self, id: EntityUid) -> Result<EntityUid, String> {
        match self.curve(id)?.kind {
            EntityKind::Arc { center, .. } | EntityKind::Circle { center, .. } => Ok(center),
            ref other => Err(format!("c{} is a {}", id.0, other.type_name())),
        }
    }

    /// Orders curves into a chain through shared end points.
    fn chain(&self, curves: &[EntityUid]) -> Result<Chain, String> {
        if curves.is_empty() {
            return Err("no curves to offset".to_owned());
        }
        let mut ends: Vec<(EntityUid, Option<(EntityUid, EntityUid)>)> = Vec::new();
        for id in curves {
            ends.push((*id, self.curve(*id)?.kind.ends()));
        }
        if ends.len() == 1 {
            return Ok(Chain {
                curves: vec![(ends[0].0, true)],
                closed: ends[0].1.is_none_or(|(s, e)| s == e),
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
                return Err("the curves are not one connected chain".to_owned());
            };
            used[i] = true;
            let (s, t) = ends[i].1.expect("open");
            let fwd = s == at;
            at = if fwd { t } else { s };
            order.push((ends[i].0, fwd));
        }
        let start = if forward { s0 } else { t0 };
        Ok(Chain {
            curves: order,
            closed: at == start,
        })
    }
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

struct Chain {
    /// Curves in order, and whether each runs forward along the chain.
    curves: Vec<(EntityUid, bool)>,
    closed: bool,
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
        /// The original arc's middle angle.
        middle: f64,
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
fn remap(kind: &mut ConstraintKind, copies: &BTreeMap<EntityUid, EntityUid>) {
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
        | K::Tangent { a, b }
        | K::Smooth { a, b }
        | K::Equal { a, b }
        | K::Concentric { a, b } => {
            m(a);
            m(b);
        }
        K::Midpoint { point, curve } => {
            m(point);
            m(curve);
        }
        K::Symmetric { a, b, axis } => {
            mr(a);
            mr(b);
            m(axis);
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
