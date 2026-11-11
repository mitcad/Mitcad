// SPDX-License-Identifier: MIT
//! Project Again (mitcad#1303): a linked projection that no longer follows
//! its source (it kept its last geometry, or some of its entities found
//! nothing like them in the source) is projected anew from the source's
//! curves as the last evaluation found them ([`ProjectionFollow::current`]),
//! as Project projects them, and what the sketch tied to its old entities
//! moves to the new ones where that is unambiguous:
//!
//! 1. The old outline is laid over the new one (its bounds moved and scaled
//!    onto the new outline's), so that an outline that moved or was resized
//!    covers itself.
//! 2. An old curve goes to the new curve of its kind (a circle and an arc
//!    count as one kind) that covers the most of it: the new curves nearest
//!    to more than half of the points along it, counting new curves that lie
//!    on one line or one circle as one (an arc split in two goes to its
//!    larger piece). A curve no new one covers so goes nowhere.
//! 3. A point of such a curve goes to the new curve's point in the same
//!    role (a line's end, either way round; an arc's centre and ends),
//!    another point to the new point nearest to where it lies, each only
//!    within a tenth of the new outline's size, and the nearest only when no
//!    other new point is within twice its distance.
//! 4. A constraint or a dimension on them moves with them when every one of
//!    its entities of the projection moves (one that needs an arc's ends or
//!    length stays off a circle), and so do the ends of the user's curves on
//!    its points and an offset whose curves all go one to one to curves of
//!    their kind.
//!
//! The old entities go. One that something still needs (a constraint or a
//! dimension that did not move, a pattern, an offset or a text, a curve of
//! the user's ending on a point that did not move) stays outside the
//! projection, a curve as a construction reference, and the result says
//! what holds it. A new entity that takes the place of old ones takes the
//! lowest of their ids, so that the regions keep their names where their
//! outline is the same. Points the projection shares with another
//! projection stay as they are, and the new curves join the other
//! projections of the same source body as Project joins them
//! ([`project::join_existing`]).

use std::collections::{BTreeMap, BTreeSet};

use super::cleanup::{remap_dimension, reverse_angle_ray};
use super::edit::SketchEdit;
use super::geometry::{Curve2, Nurbs, P2, angle_after, cross, dist, norm, sub};
use super::modify::remap;
use super::project::{self, ProjectionFollow};
use super::{ConstraintKind, ConstraintUid, DimensionKind, Entity, EntityIndex, EntityKind, Ref};
use crate::ids::EntityUid;

/// How many points along an old curve vote for the new curve it goes to.
const SAMPLES: usize = 17;

/// What [`SketchEdit::project_again`] did with one projection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectedAgain {
    /// The projection's source, as messages name it.
    pub source: String,
    /// The old entities and the new ones that took their place (the same
    /// id where the new one took it), in the order of the old ids.
    pub carried: Vec<(Ref, Ref)>,
    /// The old curves and points that stay outside the projection, a curve
    /// as a construction reference, each with what holds it: constraints and
    /// dimensions (`k7`), patterns and offsets (`k8`), texts (`t5`) and the
    /// user's curves (`c9`).
    pub kept: Vec<(Ref, Vec<String>)>,
    /// The constraints and dimensions that stay on the old geometry.
    pub left: Vec<ConstraintUid>,
    /// The curves of other projections already in the sketch that new
    /// curves were the same as (those are left out).
    pub same: Vec<EntityUid>,
}

impl SketchEdit<'_> {
    /// Projects the linked projections at `projections` (their indices in
    /// the definition's `projections`) again from their sources' curves as
    /// the last evaluation found them, carrying the constraints and
    /// dimensions on their old entities over to the new ones (see the
    /// module documentation). `follow` is the last evaluation's
    /// ([`crate::def::SketchOutput::follow`]). Fails for a projection that
    /// followed its source all the way, and for one whose source is gone.
    pub fn project_again(
        &mut self,
        follow: &[ProjectionFollow],
        projections: &[usize],
    ) -> Result<Vec<ProjectedAgain>, String> {
        let mut out = Vec::with_capacity(projections.len());
        for &k in projections {
            let projection = self.def.projections.get(k).ok_or_else(|| {
                crate::said(
                    "the sketch has no such projection",
                    format!("the sketch has no projection {k}"),
                )
            })?;
            let Some(report) = follow.iter().find(|r| r.projection == k) else {
                return Err(format!(
                    "the projection of {topo} follows its source: nothing to project again",
                    topo = projection.source
                ));
            };
            if report.lost || report.current.is_empty() {
                return Err(format!(
                    "the projection of {topo} cannot be projected again: its source is gone \
                     (Clean Up Stale Projections unlinks it)",
                    topo = projection.source
                ));
            }
            out.push(self.project_one_again(k, report)?);
        }
        Ok(out)
    }

    fn project_one_again(
        &mut self,
        k: usize,
        report: &ProjectionFollow,
    ) -> Result<ProjectedAgain, String> {
        let projection = self.def.projections[k].clone();
        let mut out = ProjectedAgain {
            source: projection.source.to_string(),
            ..ProjectedAgain::default()
        };
        // Its entities; those another projection lists too, or whose curves
        // end on, stay as they are.
        let listed: BTreeSet<EntityUid> = projection.entities.iter().map(|r| r.uid()).collect();
        let elsewhere: BTreeSet<EntityUid> = self
            .def
            .projections
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != k)
            .flat_map(|(_, p)| p.entities.iter().map(|r| r.uid()))
            .collect();
        let shared: BTreeSet<EntityUid> = self
            .def
            .entities
            .iter()
            .filter(|e| elsewhere.contains(&e.id))
            .flat_map(|e| e.kind.points())
            .chain(elsewhere.iter().copied())
            .collect();
        let own: BTreeSet<EntityUid> = listed
            .iter()
            .copied()
            .filter(|id| !shared.contains(id) && self.def.entity(*id).is_some())
            .collect();
        // The new entities, as Project makes them, joined to the other
        // projections of the same body.
        let mut joinable = project::joinable(
            &self.def,
            Some((projection.body, projection.link.as_ref())),
            projection.source.sketch().map(|e| e.sketch),
        );
        joinable.retain(|id| !listed.contains(id));
        let mut next = project::FRESH;
        let made = project::entities(&report.current, &mut || {
            next += 1;
            EntityUid(next)
        });
        let joined = project::join_existing(&self.def, &joinable, made, true);
        out.same = joined.same.iter().map(|(_, kept)| *kept).collect();
        let fresh: Vec<Entity> = joined.entities;
        let plan = Plan::new(&self.def.entities, &own, &fresh);
        // What moves with them, and what stays on the old geometry.
        let moves =
            |id: EntityUid| plan.curve_to.contains_key(&id) || plan.point_to.contains_key(&id);
        let changed = |id: EntityUid| plan.changed.contains(&id);
        let mut holders: BTreeMap<EntityUid, Vec<String>> = BTreeMap::new();
        let mut hold = |refs: &[Ref], by: String| {
            for r in refs.iter().filter(|r| own.contains(&r.uid())) {
                holders.entry(r.uid()).or_default().push(by.clone());
            }
        };
        let mut carried_constraints: BTreeSet<ConstraintUid> = BTreeSet::new();
        for c in &self.def.constraints {
            let refs = c.kind.refs();
            if !refs.iter().any(|r| own.contains(&r.uid())) {
                continue;
            }
            let all = refs
                .iter()
                .all(|r| !own.contains(&r.uid()) || moves(r.uid()));
            let kinds = !refs.iter().any(|r| changed(r.uid())) || round_constraint(&c.kind);
            if all && kinds {
                carried_constraints.insert(c.id);
            } else {
                hold(&refs, c.id.to_string());
                out.left.push(c.id);
            }
        }
        let mut carried_dimensions: BTreeSet<ConstraintUid> = BTreeSet::new();
        for d in &self.def.dimensions {
            let refs = d.kind.refs();
            if !refs.iter().any(|r| own.contains(&r.uid())) {
                continue;
            }
            let all = refs
                .iter()
                .all(|r| !own.contains(&r.uid()) || moves(r.uid()));
            let kinds = !refs.iter().any(|r| changed(r.uid())) || round_dimension(&d.kind);
            if all && kinds {
                carried_dimensions.insert(d.id);
            } else {
                hold(&refs, d.id.to_string());
                out.left.push(d.id);
            }
        }
        for p in &self.def.patterns {
            let mut refs: Vec<Ref> = p.entities.clone();
            for copy in &p.copies {
                refs.extend(copy.entities.iter().flat_map(|(a, b)| [*a, *b]));
            }
            if let super::pattern::PatternKind::Circular { center, .. } = &p.kind {
                refs.push(Ref::Point(*center));
            }
            hold(&refs, p.id.to_string());
        }
        // An offset of the projection moves when its curves go one to one
        // to new curves of their kind.
        let mut carried_offsets: BTreeSet<ConstraintUid> = BTreeSet::new();
        for o in &self.def.offsets {
            let on: Vec<EntityUid> = o
                .curves
                .iter()
                .chain(&o.results)
                .chain(&o.helpers)
                .copied()
                .filter(|c| own.contains(c))
                .collect();
            if on.is_empty() {
                continue;
            }
            let targets: BTreeSet<EntityUid> = on
                .iter()
                .filter_map(|c| plan.curve_to.get(c).copied())
                .collect();
            let one_to_one = o.results.iter().all(|c| !own.contains(c))
                && o.helpers.iter().all(|c| !own.contains(c))
                && targets.len() == on.len()
                && on.iter().all(|c| !changed(*c));
            if one_to_one {
                carried_offsets.insert(o.id);
            } else {
                let refs: Vec<Ref> = on.iter().map(|c| Ref::Curve(*c)).collect();
                hold(&refs, o.id.to_string());
            }
        }
        for t in &self.def.texts {
            let mut refs: Vec<Ref> = t.frame.iter().map(|p| Ref::Point(*p)).collect();
            refs.extend(t.path.map(|p| Ref::Curve(p.curve)));
            hold(&refs, format!("t{}", t.id.0));
        }
        // The user's curves on its points that do not move.
        for e in &self.def.entities {
            if listed.contains(&e.id) {
                continue;
            }
            for p in e.kind.points() {
                if own.contains(&p) && !plan.point_to.contains_key(&p) {
                    holders
                        .entry(p)
                        .or_default()
                        .push(Ref::Curve(e.id).to_string());
                }
            }
        }
        // What stays, with the points its curves need.
        let mut kept: BTreeSet<EntityUid> = holders.keys().copied().collect();
        let needed: Vec<EntityUid> = kept
            .iter()
            .filter_map(|id| self.def.entity(*id))
            .flat_map(|e| e.kind.points())
            .filter(|p| own.contains(p))
            .collect();
        kept.extend(needed);
        // The ids the new entities take: the lowest of the old ones that go
        // to them, else new ones. Only new entities take ids: an old point
        // that goes to a point of the sketch the new curves end on (another
        // projection's, mitcad#1605) goes, its constraints and curves moved
        // onto that point.
        let fresh_ids: BTreeSet<EntityUid> = fresh.iter().map(|e| e.id).collect();
        let mut final_id: BTreeMap<EntityUid, EntityUid> = BTreeMap::new();
        for (old, new) in plan.curve_to.iter().chain(&plan.point_to) {
            if kept.contains(old) || !fresh_ids.contains(new) {
                continue;
            }
            let taken = final_id.entry(*new).or_insert(*old);
            if *old < *taken {
                *taken = *old;
            }
        }
        let unnamed: Vec<EntityUid> = fresh
            .iter()
            .map(|e| e.id)
            .filter(|id| !final_id.contains_key(id))
            .collect();
        let ids = self.ids(unnamed.len());
        final_id.extend(unnamed.into_iter().zip(ids));
        let name = |id: EntityUid| -> EntityUid {
            if fresh_ids.contains(&id) {
                final_id.get(&id).copied().unwrap_or(id)
            } else {
                id
            }
        };
        let reused: BTreeSet<EntityUid> = final_id
            .values()
            .copied()
            .filter(|id| own.contains(id))
            .collect();
        // Each old entity that moves, onto the new one's id.
        let map: BTreeMap<EntityUid, EntityUid> = plan
            .curve_to
            .iter()
            .chain(&plan.point_to)
            .map(|(old, new)| (*old, name(*new)))
            .collect();
        // The new entities with their ids, an old curve's flags kept.
        let mut renamed: Vec<Entity> = Vec::with_capacity(fresh.len());
        for f in &fresh {
            let mut e = f.clone();
            e.id = name(f.id);
            for p in f.kind.points() {
                e.kind.replace_point(p, name(p));
            }
            if let Some(old) = self.def.entity(e.id).filter(|_| reused.contains(&e.id)) {
                e.construction = old.construction && !old.is_point();
                if let (
                    EntityKind::Line { centerline, .. },
                    EntityKind::Line {
                        centerline: was, ..
                    },
                ) = (&mut e.kind, &old.kind)
                {
                    *centerline = *was;
                }
            }
            renamed.push(e);
        }
        // What moves with them refers to the new entities.
        for c in &mut self.def.constraints {
            if carried_constraints.contains(&c.id) {
                remap(&mut c.kind, &map);
            }
        }
        for d in &mut self.def.dimensions {
            if carried_dimensions.contains(&d.id) {
                for line in &plan.reversed {
                    reverse_angle_ray(&mut d.kind, *line);
                }
                remap_dimension(&mut d.kind, &map);
            }
        }
        // Those that now say nothing (two arcs Equal that became one circle).
        let idle = |refs: Vec<Ref>| {
            let ids: BTreeSet<EntityUid> = refs.iter().map(|r| r.uid()).collect();
            ids.len() < refs.len()
        };
        let silent: BTreeSet<ConstraintUid> = self
            .def
            .constraints
            .iter()
            .filter(|c| carried_constraints.contains(&c.id) && idle(c.kind.refs()))
            .map(|c| c.id)
            .chain(
                self.def
                    .dimensions
                    .iter()
                    .filter(|d| carried_dimensions.contains(&d.id) && idle(d.kind.refs()))
                    .map(|d| d.id),
            )
            .collect();
        self.drop_constraints(&silent);
        for o in &mut self.def.offsets {
            if carried_offsets.contains(&o.id) {
                for c in &mut o.curves {
                    if let Some(to) = map.get(c) {
                        *c = *to;
                    }
                }
            }
        }
        for e in &mut self.def.entities {
            if listed.contains(&e.id) {
                continue;
            }
            for p in e.kind.points() {
                if own.contains(&p)
                    && let Some(to) = map.get(&p)
                {
                    e.kind.replace_point(p, *to);
                }
            }
        }
        // The projection's new entities: those that took an old id where
        // that one was, the others added.
        for e in renamed.iter().cloned() {
            if reused.contains(&e.id) {
                if let Some(target) = self.def.entity_mut(e.id) {
                    *target = e;
                }
            } else if !self.def.entities.iter().any(|x| x.id == e.id) {
                self.push(e);
            }
        }
        self.def.projections[k].entities = renamed.iter().map(Entity::as_ref).collect();
        self.def.projections[k].by_geometry = true;
        // The old ones that went, and those that stay as references.
        let gone: BTreeSet<EntityUid> = own
            .iter()
            .copied()
            .filter(|id| !kept.contains(id) && !reused.contains(id))
            .collect();
        self.drop_entities(&gone);
        for id in &kept {
            if let Some(e) = self.def.entity_mut(*id) {
                e.construction = !e.is_point();
                e.fixed = true;
                e.reference = true;
            }
        }
        let as_ref = |id: EntityUid| {
            if plan.points.contains(&id) {
                Ref::Point(id)
            } else {
                Ref::Curve(id)
            }
        };
        out.carried = map
            .iter()
            .map(|(old, new)| (as_ref(*old), as_ref_like(*old, *new, &plan)))
            .collect();
        out.kept = kept
            .iter()
            .map(|id| (as_ref(*id), holders.get(id).cloned().unwrap_or_default()))
            .collect();
        out.left.sort_unstable();
        out.left.dedup();
        Ok(out)
    }
}

/// The new entity an old one went to, as a reference of the old one's
/// kind.
fn as_ref_like(old: EntityUid, new: EntityUid, plan: &Plan) -> Ref {
    if plan.points.contains(&old) {
        Ref::Point(new)
    } else {
        Ref::Curve(new)
    }
}

/// A constraint that holds for a circle as for an arc, so that it moves
/// with an arc that became a circle or the other way round.
fn round_constraint(kind: &ConstraintKind) -> bool {
    matches!(
        kind,
        ConstraintKind::Coincident { .. }
            | ConstraintKind::Tangent { at: None, .. }
            | ConstraintKind::Equal { .. }
            | ConstraintKind::Concentric { .. }
            | ConstraintKind::Symmetric { .. }
    )
}

/// A dimension that measures a circle as an arc (not an arc's length).
fn round_dimension(kind: &DimensionKind) -> bool {
    matches!(
        kind,
        DimensionKind::Radius { .. }
            | DimensionKind::Diameter { .. }
            | DimensionKind::ConcentricDistance { .. }
            | DimensionKind::TangentDistance { .. }
            | DimensionKind::TangentCircleDistance { .. }
            | DimensionKind::TangentPointDistance { .. }
    )
}

/// Where the old entities of a projection go among the new ones.
struct Plan {
    /// Old curve → the new curve it goes to.
    curve_to: BTreeMap<EntityUid, EntityUid>,
    /// Old point → the new point it goes to.
    point_to: BTreeMap<EntityUid, EntityUid>,
    /// The old curves whose new one is of another kind (an arc that became
    /// a circle).
    changed: BTreeSet<EntityUid>,
    /// The old lines whose new one runs the other way.
    reversed: BTreeSet<EntityUid>,
    /// The old points.
    points: BTreeSet<EntityUid>,
}

impl Plan {
    /// Where the old entities `own` (of `entities`) go among `fresh`.
    fn new(entities: &[Entity], own: &BTreeSet<EntityUid>, fresh: &[Entity]) -> Self {
        let index = EntityIndex::new(entities);
        let fresh_points: BTreeMap<EntityUid, P2> = fresh
            .iter()
            .filter_map(|e| match e.kind {
                EntityKind::Point { at } => Some((e.id, at)),
                _ => None,
            })
            .collect();
        let new_at = |id: EntityUid| fresh_points.get(&id).copied().or_else(|| index.point(id));
        let old_at = |id: EntityUid| index.point(id);
        let olds: Vec<&Entity> = own.iter().filter_map(|id| index.get(*id)).collect();
        let old_curves: Vec<&Entity> = olds.iter().copied().filter(|e| !e.is_point()).collect();
        let new_curves: Vec<&Entity> = fresh.iter().filter(|e| !e.is_point()).collect();
        let old_shapes: Vec<Option<Curve2>> = old_curves
            .iter()
            .map(|e| placed(&e.kind, &old_at))
            .collect();
        let new_shapes: Vec<Option<Curve2>> = new_curves
            .iter()
            .map(|e| placed(&e.kind, &new_at))
            .collect();
        // The new points: the new entities' and those of the sketch their
        // curves end on.
        let mut new_points: Vec<(EntityUid, P2)> = Vec::new();
        for e in fresh {
            let ids = if e.is_point() {
                vec![e.id]
            } else {
                e.kind.points()
            };
            for id in ids {
                if !new_points.iter().any(|(p, _)| *p == id)
                    && let Some(at) = new_at(id)
                {
                    new_points.push((id, at));
                }
            }
        }
        // The old outline laid over the new one.
        let old_spread: Vec<P2> = old_shapes
            .iter()
            .flatten()
            .flat_map(samples)
            .chain(olds.iter().filter_map(|e| match e.kind {
                EntityKind::Point { at } => Some(at),
                _ => None,
            }))
            .collect();
        let new_spread: Vec<P2> = new_shapes
            .iter()
            .flatten()
            .flat_map(samples)
            .chain(new_points.iter().map(|(_, at)| *at))
            .collect();
        let lay = Lay::between(&old_spread, &new_spread);
        let near = 0.1 * lay.size;
        let mut extent = 1.0_f64;
        for p in &new_spread {
            extent = extent.max(norm(*p));
        }
        let same = 1e-6 * extent;
        let mut plan = Plan {
            curve_to: BTreeMap::new(),
            point_to: BTreeMap::new(),
            changed: BTreeSet::new(),
            reversed: BTreeSet::new(),
            points: olds.iter().filter(|e| e.is_point()).map(|e| e.id).collect(),
        };
        // 2. Each old curve to the new one that covers the most of it.
        for (o, shape) in old_curves.iter().zip(&old_shapes) {
            let Some(shape) = shape else {
                continue;
            };
            let mut counts = vec![0_usize; new_curves.len()];
            let along = samples(shape);
            for s in &along {
                let s = lay.at(*s);
                let nearest = new_shapes
                    .iter()
                    .enumerate()
                    .filter_map(|(j, n)| Some((j, n.as_ref()?.closest(s).1)))
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((j, _)) = nearest {
                    counts[j] += 1;
                }
            }
            let fits = |j: usize| counts[j] > 0 && compatible(&o.kind, &new_curves[j].kind);
            let mut best: Option<(usize, usize, usize)> = None;
            for j in (0..new_curves.len()).filter(|j| fits(*j)) {
                let votes: usize = (0..new_curves.len())
                    .filter(|i| fits(*i) && same_support(&new_shapes[j], &new_shapes[*i], same))
                    .map(|i| counts[i])
                    .sum();
                if best.is_none_or(|(_, v, c)| (votes, counts[j]) > (v, c)) {
                    best = Some((j, votes, counts[j]));
                }
            }
            let Some((j, votes, _)) = best else {
                continue;
            };
            if 2 * votes <= along.len() {
                continue;
            }
            let n = new_curves[j];
            plan.curve_to.insert(o.id, n.id);
            if o.kind.type_name() != n.kind.type_name() {
                plan.changed.insert(o.id);
            }
            // 3. Its points in the same role, where they lie near.
            let laid = |id: EntityUid| old_at(id).map(|p| lay.at(p));
            let (pairs, reversed) = roles(&o.kind, &n.kind, &laid, &new_at);
            if reversed {
                plan.reversed.insert(o.id);
            }
            for (from, to) in pairs {
                let close = laid(from)
                    .zip(new_at(to))
                    .is_some_and(|(a, b)| dist(a, b) <= near);
                if close && own.contains(&from) {
                    plan.point_to.entry(from).or_insert(to);
                }
            }
        }
        // The other points to the new point nearest to where they lie.
        for e in olds.iter().filter(|e| e.is_point()) {
            if plan.point_to.contains_key(&e.id) {
                continue;
            }
            let Some(at) = old_at(e.id).map(|p| lay.at(p)) else {
                continue;
            };
            let mut found: Vec<(f64, EntityUid)> = new_points
                .iter()
                .map(|(id, p)| (dist(at, *p), *id))
                .collect();
            found.sort_by(|a, b| a.0.total_cmp(&b.0));
            let Some(&(d, id)) = found.first() else {
                continue;
            };
            let alone = found.get(1).is_none_or(|(next, _)| *next > 2.0 * d);
            if d <= near && alone {
                plan.point_to.insert(e.id, id);
            }
        }
        plan
    }
}

/// Whether a new curve may take an old one's place: of one kind, a circle
/// and an arc counting as one.
fn compatible(old: &EntityKind, new: &EntityKind) -> bool {
    let round = |k: &EntityKind| matches!(k, EntityKind::Circle { .. } | EntityKind::Arc { .. });
    old.type_name() == new.type_name() || (round(old) && round(new))
}

/// Whether two new curves lie on one line or one circle (within `tol`):
/// pieces of what was one curve.
fn same_support(a: &Option<Curve2>, b: &Option<Curve2>, tol: f64) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    let round = |c: &Curve2| match c {
        Curve2::Circle { center, radius, .. } | Curve2::Arc { center, radius, .. } => {
            Some((*center, *radius))
        }
        _ => None,
    };
    match (a, b) {
        (Curve2::Line { a: a0, b: a1 }, Curve2::Line { a: b0, b: b1 }) => {
            let length = dist(*a0, *a1);
            if length <= tol {
                return false;
            }
            let u = [(a1[0] - a0[0]) / length, (a1[1] - a0[1]) / length];
            [*b0, *b1]
                .iter()
                .all(|q| cross(u, sub(*q, *a0)).abs() <= tol)
        }
        _ => match (round(a), round(b)) {
            (Some((c0, r0)), Some((c1, r1))) => dist(c0, c1) <= tol && (r0 - r1).abs() <= tol,
            _ => a == b,
        },
    }
}

/// The points of an old curve and of the new curve it goes to that have
/// the same role (a line's ends, either way round; an arc's centre and
/// ends; a circle's centre, also of an arc that became one; an ellipse's
/// centre and major point, an elliptical arc's ends; a spline's control
/// points, either way round, when there are as many, else its ends), and
/// whether the new line or spline runs the other way. `old_at` gives the
/// old points laid over the new outline.
fn roles(
    old: &EntityKind,
    new: &EntityKind,
    old_at: &dyn Fn(EntityUid) -> Option<P2>,
    new_at: &dyn Fn(EntityUid) -> Option<P2>,
) -> (Vec<(EntityUid, EntityUid)>, bool) {
    let apart = |a: EntityUid, b: EntityUid| match (old_at(a), new_at(b)) {
        (Some(p), Some(q)) => dist(p, q),
        _ => f64::INFINITY,
    };
    // Either way round, whichever lies nearer.
    let either = |a: &[EntityUid], b: &[EntityUid]| -> (Vec<(EntityUid, EntityUid)>, bool) {
        let ahead: f64 = a.iter().zip(b).map(|(p, q)| apart(*p, *q)).sum();
        let back: f64 = a
            .iter()
            .zip(b.iter().rev())
            .map(|(p, q)| apart(*p, *q))
            .sum();
        if back < ahead {
            (
                a.iter().copied().zip(b.iter().rev().copied()).collect(),
                true,
            )
        } else {
            (a.iter().copied().zip(b.iter().copied()).collect(), false)
        }
    };
    use EntityKind as E;
    match (old, new) {
        (
            E::Line { start, end, .. },
            E::Line {
                start: s, end: e, ..
            },
        ) => either(&[*start, *end], &[*s, *e]),
        (
            E::Arc { center, start, end },
            E::Arc {
                center: c,
                start: s,
                end: e,
            },
        ) => (vec![(*center, *c), (*start, *s), (*end, *e)], false),
        (
            E::Circle { center, .. } | E::Arc { center, .. },
            E::Circle { center: c, .. } | E::Arc { center: c, .. },
        ) => (vec![(*center, *c)], false),
        (
            E::Ellipse { center, major, .. },
            E::Ellipse {
                center: c,
                major: m,
                ..
            },
        ) => (vec![(*center, *c), (*major, *m)], false),
        (
            E::EllipticalArc {
                center,
                major,
                start,
                end,
                ..
            },
            E::EllipticalArc {
                center: c,
                major: m,
                start: s,
                end: e,
                ..
            },
        ) => (
            vec![(*center, *c), (*major, *m), (*start, *s), (*end, *e)],
            false,
        ),
        (E::Spline { control: a, .. }, E::Spline { control: b, .. }) => {
            if a.len() == b.len() {
                either(a, b)
            } else {
                match (a.first().zip(a.last()), b.first().zip(b.last())) {
                    (Some((a0, a1)), Some((b0, b1))) => either(&[*a0, *a1], &[*b0, *b1]),
                    _ => (Vec::new(), false),
                }
            }
        }
        _ => (Vec::new(), false),
    }
}

/// An old outline laid over a new one: its bounds moved onto the new
/// outline's and scaled along each axis where both have an extent there.
struct Lay {
    from: P2,
    to: P2,
    factor: P2,
    /// The new outline's size (its bounds' diagonal).
    size: f64,
}

impl Lay {
    fn between(old: &[P2], new: &[P2]) -> Self {
        let bounds = |points: &[P2]| -> (P2, P2) {
            let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for p in points {
                for i in 0..2 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
            if lo[0].is_finite() {
                (lo, hi)
            } else {
                ([0.0; 2], [0.0; 2])
            }
        };
        let (old_lo, old_hi) = bounds(old);
        let (new_lo, new_hi) = bounds(new);
        let factor: P2 = std::array::from_fn(|i| {
            let (was, now) = (old_hi[i] - old_lo[i], new_hi[i] - new_lo[i]);
            if was > 1e-12 && now > 1e-12 {
                now / was
            } else {
                1.0
            }
        });
        Self {
            from: std::array::from_fn(|i| 0.5 * (old_lo[i] + old_hi[i])),
            to: std::array::from_fn(|i| 0.5 * (new_lo[i] + new_hi[i])),
            factor,
            size: dist(new_lo, new_hi).max(1e-9),
        }
    }

    fn at(&self, p: P2) -> P2 {
        std::array::from_fn(|i| self.to[i] + (p[i] - self.from[i]) * self.factor[i])
    }
}

/// [`SAMPLES`] points along a curve, from its start to its end.
fn samples(curve: &Curve2) -> Vec<P2> {
    let (t0, t1) = curve.domain();
    (0..SAMPLES)
        .map(|k| curve.point(t0 + (t1 - t0) * k as f64 / (SAMPLES - 1) as f64))
        .collect()
}

/// A curve of `kind` with its points where `at` puts them; None for a
/// point, a spline by fit points or one without knots, and a missing
/// point.
fn placed(kind: &EntityKind, at: &dyn Fn(EntityUid) -> Option<P2>) -> Option<Curve2> {
    let angle = |v: P2| v[1].atan2(v[0]);
    Some(match kind {
        EntityKind::Line { start, end, .. } => Curve2::Line {
            a: at(*start)?,
            b: at(*end)?,
        },
        EntityKind::Circle {
            center,
            radius,
            start,
        } => Curve2::Circle {
            center: at(*center)?,
            radius: *radius,
            start: *start,
        },
        EntityKind::Arc { center, start, end } => {
            let (c, a, b) = (at(*center)?, at(*start)?, at(*end)?);
            let s = angle(sub(a, c));
            Curve2::Arc {
                center: c,
                radius: dist(a, c),
                start: s,
                end: angle_after(s, angle(sub(b, c))),
            }
        }
        EntityKind::Ellipse {
            center,
            major,
            minor_radius,
        } => {
            let (c, m) = (at(*center)?, at(*major)?);
            Curve2::Ellipse {
                center: c,
                major: dist(m, c),
                minor: *minor_radius,
                rotation: angle(sub(m, c)),
            }
        }
        EntityKind::EllipticalArc {
            center,
            major,
            minor_radius,
            start,
            end,
        } => {
            let (c, m) = (at(*center)?, at(*major)?);
            let whole = Curve2::Ellipse {
                center: c,
                major: dist(m, c),
                minor: *minor_radius,
                rotation: angle(sub(m, c)),
            };
            let s = whole.closest(at(*start)?).0;
            let e = whole.closest(at(*end)?).0;
            Curve2::EllipticalArc {
                center: c,
                major: dist(m, c),
                minor: *minor_radius,
                rotation: angle(sub(m, c)),
                start: s,
                end: angle_after(s, e),
            }
        }
        EntityKind::Spline {
            degree,
            control,
            weights,
            knots,
        } => {
            if knots.is_empty() {
                return None;
            }
            Curve2::Nurbs(Nurbs {
                degree: *degree as usize,
                control: control
                    .iter()
                    .map(|p| at(*p))
                    .collect::<Option<Vec<P2>>>()?,
                weights: weights.clone(),
                knots: knots.clone(),
            })
        }
        EntityKind::Point { .. } | EntityKind::FittedSpline { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::project::Projected;
    use super::*;
    use crate::def::SketchDef;
    use crate::ids::{BodyUid, FeatureUid};
    use crate::parameters::Parameters;
    use crate::{Projection, ProjectionSource};

    #[test]
    fn an_old_point_that_goes_to_another_projections_point_goes() {
        // (mitcad#1605) Projection A of a body: a line from (0, 0) to
        // (10, 0). Projection B of the same body: a line from its own
        // corner point P = p4 at (12, 0) to (20, 0), the user's line c8
        // ending on P. The model changes so that B's source starts where
        // A's line ends: the new line is joined to A's end point Q = p2, and
        // P goes to Q.
        let line = |id: u32, start: u32, end: u32| {
            Entity::new(
                EntityUid(id),
                EntityKind::Line {
                    start: EntityUid(start),
                    end: EntityUid(end),
                    centerline: false,
                },
            )
        };
        let projected = |mut e: Entity| {
            e.fixed = true;
            e.reference = true;
            e
        };
        let projection = |source: &str, ids: [u32; 3]| Projection {
            source: source.parse::<ProjectionSource>().unwrap(),
            body: Some(BodyUid::new(FeatureUid(1), 0)),
            entities: vec![
                Ref::Point(EntityUid(ids[0])),
                Ref::Point(EntityUid(ids[1])),
                Ref::Curve(EntityUid(ids[2])),
            ],
            link: None,
            by_geometry: true,
            section: false,
            chain: Vec::new(),
            after: None,
            outline: false,
        };
        let def: SketchDef = SketchDef {
            entities: vec![
                projected(Entity::point(EntityUid(1), [0.0, 0.0])),
                projected(Entity::point(EntityUid(2), [10.0, 0.0])),
                projected(line(3, 1, 2)),
                projected(Entity::point(EntityUid(4), [12.0, 0.0])),
                projected(Entity::point(EntityUid(5), [20.0, 0.0])),
                projected(line(6, 4, 5)),
                Entity::point(EntityUid(7), [12.0, 10.0]),
                line(8, 4, 7),
            ],
            projections: vec![
                projection("F1:top", [1, 2, 3]),
                projection("F1:bottom", [4, 5, 6]),
            ],
            ..SketchDef::default()
        };
        let follow = ProjectionFollow {
            projection: 1,
            unfollowed: vec![EntityUid(6)],
            current: vec![Projected::Curve(Curve2::Line {
                a: [10.0, 0.0],
                b: [20.0, 0.0],
            })],
            ..ProjectionFollow::default()
        };
        let mut params = Parameters::default();
        let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
        let done = edit.project_again(&[follow], &[1]).unwrap();
        assert!(
            done[0]
                .carried
                .contains(&(Ref::Point(EntityUid(4)), Ref::Point(EntityUid(2)))),
            "{done:?}"
        );
        assert!(done[0].kept.is_empty(), "{done:?}");
        let def = &edit.def;
        // The old point is gone, and the user's line ends on A's point.
        assert!(def.entity(EntityUid(4)).is_none(), "{:?}", def.entities);
        let user = def.entity(EntityUid(8)).unwrap();
        assert_eq!(
            user.kind.points(),
            vec![EntityUid(2), EntityUid(7)],
            "{user:?}"
        );
        // Every reference entity of the sketch is listed by a projection.
        let listed: BTreeSet<EntityUid> = def
            .projections
            .iter()
            .flat_map(|p| p.entities.iter().map(|r| r.uid()))
            .collect();
        for e in def.entities.iter().filter(|e| e.reference) {
            assert!(listed.contains(&e.id), "{e:?} is listed by no projection");
        }
        // B's new line took its old id and runs from Q to (20, 0).
        let index = EntityIndex::new(&def.entities);
        let line = def.entity(EntityUid(6)).unwrap();
        let ends: Vec<Option<P2>> = line.kind.points().iter().map(|p| index.point(*p)).collect();
        assert!(line.kind.points().contains(&EntityUid(2)), "{line:?}");
        assert!(ends.contains(&Some([20.0, 0.0])), "{line:?}: {ends:?}");
    }
}
