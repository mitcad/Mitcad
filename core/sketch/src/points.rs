// SPDX-License-Identifier: MIT
//! Points that become one: a curve end that Extend brings to an existing
//! point joins it (mitcad#259), as does a Trim cut end on one (mitcad#1191),
//! and a Coincident constraint between the ends of two curves merges them
//! (mitcad#260), so that the curves share a point as curves drawn from the
//! same point do. Offset and the chain selection join curves only through
//! shared points.
//!
//! [`SketchEdit::merge_points`] moves everything that refers to one point
//! (curve ends, constraints, dimensions, text frames) to the other, drops
//! the constraints and dimensions that then say nothing, and removes the
//! point. Points the model makes again or moves itself are never merged
//! away: projected points (they follow their source), points a derived
//! offset computes, and the points of patterns (originals, copies and
//! centres).

use std::collections::{BTreeMap, BTreeSet};

use super::cleanup::remap_dimension;
use super::edit::SketchEdit;
use super::geometry::{P2, dist};
use super::modify::remap;
use super::pattern::PatternKind;
use super::{ConstraintKind, ConstraintUid, EntityIndex, Ref};
use crate::ids::EntityUid;

impl SketchEdit<'_> {
    /// Merges the point `drop` into `keep`: curve ends, constraints,
    /// dimensions and text frames on `drop` refer to `keep`, which is fixed
    /// when either was; the constraints on `keep` that then say nothing (a
    /// point at itself, a point on a curve it ends, the same constraint
    /// twice) and the dimensions between the two go, and `drop` is
    /// removed. Positions are not changed: the next solve places `keep`.
    /// Refused for a point the model makes or moves itself (see the module
    /// documentation).
    pub fn merge_points(&mut self, keep: EntityUid, drop: EntityUid) -> Result<(), String> {
        if keep == drop {
            return Err(format!("p{} cannot be merged with itself", keep.0));
        }
        self.at(keep)?;
        self.at(drop)?;
        if !self.mergeable_point(drop) {
            return Err(format!(
                "p{} is projected, computed by an offset or part of a pattern: it cannot be \
                 merged into another point",
                drop.0
            ));
        }
        let map = BTreeMap::from([(drop, keep)]);
        for e in &mut self.def.entities {
            e.kind.replace_point(drop, keep);
        }
        for c in &mut self.def.constraints {
            remap(&mut c.kind, &map);
            // A point symmetric to itself about a line lies on the line.
            if let ConstraintKind::Symmetric {
                a: Ref::Point(a),
                b: Ref::Point(b),
                axis,
                ..
            } = c.kind
                && a == b
            {
                c.kind = ConstraintKind::Coincident {
                    point: a,
                    entity: Ref::Curve(axis),
                };
            }
        }
        for d in &mut self.def.dimensions {
            remap_dimension(&mut d.kind, &map);
        }
        for t in &mut self.def.texts {
            for p in &mut t.frame {
                if *p == drop {
                    *p = keep;
                }
            }
        }
        if self.def.entity(drop).is_some_and(|e| e.fixed)
            && let Some(e) = self.def.entity_mut(keep)
        {
            e.fixed = true;
        }
        self.drop_idle_on(keep);
        self.drop_entities(&BTreeSet::from([drop]));
        Ok(())
    }

    /// Removes the constraints and dimensions on `point` that say nothing
    /// after a merge: one that names it twice, a point-on-curve on a curve
    /// it is a point of, and a constraint the same as an earlier one. Of
    /// the same constraint twice the active one stays (mitcad#723): an
    /// inactive duplicate of an active one holds nothing.
    pub(super) fn drop_idle_on(&mut self, point: EntityUid) {
        let index = EntityIndex::new(&self.def.entities);
        let on_point = |refs: &[Ref]| refs.contains(&Ref::Point(point));
        let twice = |refs: &[Ref]| {
            let ids: BTreeSet<EntityUid> = refs.iter().map(|r| r.uid()).collect();
            ids.len() < refs.len()
        };
        let mut gone: BTreeSet<ConstraintUid> = BTreeSet::new();
        // Two points coincident say the same either way round.
        let normal = |kind: &ConstraintKind| match *kind {
            ConstraintKind::Coincident {
                point: p,
                entity: Ref::Point(q),
            } if q < p => ConstraintKind::Coincident {
                point: q,
                entity: Ref::Point(p),
            },
            ref other => other.clone(),
        };
        let mut seen: Vec<ConstraintKind> = Vec::new();
        // The active ones first, so that a duplicate keeps the active one.
        let active_first = self
            .def
            .constraints
            .iter()
            .filter(|c| c.active)
            .chain(self.def.constraints.iter().filter(|c| !c.active));
        for c in active_first {
            let refs = c.kind.refs();
            if !on_point(&refs) {
                continue;
            }
            let own = matches!(c.kind, ConstraintKind::Coincident { point: p, entity: Ref::Curve(on) }
                if index.get(on).is_some_and(|e| e.kind.points().contains(&p)));
            let kind = normal(&c.kind);
            if twice(&refs) || own || seen.contains(&kind) {
                gone.insert(c.id);
            } else {
                seen.push(kind);
            }
        }
        for d in &self.def.dimensions {
            let refs = d.kind.refs();
            if on_point(&refs) && twice(&refs) {
                gone.insert(d.id);
            }
        }
        self.drop_constraints(&gone);
    }

    /// Whether a point may be merged into another or become the end of
    /// another curve: not projected (a reference point, or one a linked
    /// projection lists), not computed by a derived offset and not a point
    /// of a pattern.
    pub(crate) fn mergeable_point(&self, p: EntityUid) -> bool {
        let Some(e) = self.def.entity(p) else {
            return false;
        };
        let projected = e.reference
            || self
                .def
                .projections
                .iter()
                .any(|pr| pr.entities.contains(&Ref::Point(p)));
        !projected && self.computing_offset(p).is_none() && !self.pattern_point(p)
    }

    /// Whether a pattern uses a point: its centre, an original or a point
    /// of an original curve, or a copy.
    fn pattern_point(&self, p: EntityUid) -> bool {
        self.def.patterns.iter().any(|pattern| {
            matches!(pattern.kind, PatternKind::Circular { center, .. } if center == p)
                || pattern.entities.iter().any(|r| {
                    r.uid() == p
                        || self
                            .def
                            .entity(r.uid())
                            .is_some_and(|e| e.kind.points().contains(&p))
                })
                || pattern
                    .copies
                    .iter()
                    .any(|c| c.entities.iter().any(|(o, m)| o.uid() == p || m.uid() == p))
        })
    }

    /// Whether a point is an end of a curve.
    fn curve_end(&self, p: EntityUid) -> bool {
        self.def
            .entities
            .iter()
            .any(|e| e.kind.ends().is_some_and(|(s, t)| s == p || t == p))
    }

    /// The points a Coincident constraint of the command merges (mitcad#260):
    /// (kept, removed) when it puts the end of a curve at the end of another
    /// curve, else None and it stays a constraint. A constraint stays when
    /// either point is no curve end (a lone point, a centre, a spline's
    /// inner point), when one curve has both points, when both are fixed,
    /// and for points the model makes or moves itself
    /// ([`SketchEdit::mergeable_point`]). Kept is a fixed point over a free
    /// one, else the one more constraints and dimensions name, else the
    /// lower id.
    pub fn coincident_merge(&self, kind: &ConstraintKind) -> Option<(EntityUid, EntityUid)> {
        let ConstraintKind::Coincident {
            point: a,
            entity: Ref::Point(b),
        } = *kind
        else {
            return None;
        };
        let (ea, eb) = (self.def.entity(a)?, self.def.entity(b)?);
        if a == b || !ea.is_point() || !eb.is_point() {
            return None;
        }
        if !self.curve_end(a) || !self.curve_end(b) || (ea.fixed && eb.fixed) {
            return None;
        }
        let together = self.def.entities.iter().any(|e| {
            let points = e.kind.points();
            points.contains(&a) && points.contains(&b)
        });
        if together || !self.mergeable_point(a) || !self.mergeable_point(b) {
            return None;
        }
        let weight = |p: EntityUid| {
            let names = |refs: Vec<Ref>| refs.contains(&Ref::Point(p));
            self.def
                .constraints
                .iter()
                .filter(|c| names(c.kind.refs()))
                .count()
                + self
                    .def
                    .dimensions
                    .iter()
                    .filter(|d| names(d.kind.refs()))
                    .count()
        };
        let keep_a = match (ea.fixed, eb.fixed) {
            (true, false) => true,
            (false, true) => false,
            _ => match weight(a).cmp(&weight(b)) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Equal => a < b,
            },
        };
        Some(if keep_a { (a, b) } else { (b, a) })
    }

    /// A Coincident constraint `id` just added between two curve ends
    /// ([`SketchEdit::coincident_merge`]) becomes the merge of `drop` into
    /// `keep`: the sketch is solved with it first, where it solves, so the
    /// kept point is where the solve puts the coincidence; then the points
    /// merge, and the constraint, which now names one point twice, is
    /// neither in the result's new constraints nor in its removed ones.
    pub fn merge_coincident(
        &mut self,
        id: ConstraintUid,
        keep: EntityUid,
        drop: EntityUid,
    ) -> Result<(), String> {
        // A constraint or dimension between the two (a distance) may keep
        // it from solving; the merge drops it, and the solve after the edit
        // decides.
        if let Ok(solved) = self.solved() {
            solved.store(&mut self.def.entities);
        }
        self.merge_points(keep, drop)?;
        let name = id.to_string();
        self.report.constraints.retain(|k| *k != id);
        self.report.removed.retain(|r| *r != name);
        Ok(())
    }

    /// The existing point an extension (or a trim's cut, mitcad#1191) that
    /// ends at `at` on the curve `target` reaches (mitcad#259): one within
    /// `tol` of `at` (where `points`, the solved positions, have it), the
    /// target's own points first, then any other, the lower id where
    /// several are; never a point of the extended (or trimmed) curve
    /// `curve`. With whether the curve can end at it ([`SketchEdit::mergeable_point`]); one
    /// it cannot is held there by a point-to-point coincident constraint.
    pub(super) fn point_reached(
        &self,
        at: P2,
        points: &BTreeMap<EntityUid, P2>,
        target: EntityUid,
        curve: EntityUid,
        tol: f64,
    ) -> Option<(EntityUid, bool)> {
        let own: Vec<EntityUid> = self
            .def
            .entity(curve)
            .map(|e| e.kind.points())
            .unwrap_or_default();
        let target_points: Vec<EntityUid> = self
            .def
            .entity(target)
            .map(|e| e.kind.points())
            .unwrap_or_default();
        let near = |p: &EntityUid| {
            !own.contains(p)
                && points
                    .get(p)
                    .copied()
                    .or_else(|| self.at(*p).ok())
                    .is_some_and(|q| dist(q, at) <= tol)
        };
        let found = target_points
            .iter()
            .copied()
            .filter(|p| near(p))
            .min()
            .or_else(|| {
                self.def
                    .entities
                    .iter()
                    .filter(|e| e.is_point())
                    .map(|e| e.id)
                    .filter(|p| near(p))
                    .min()
            })?;
        Some((found, self.mergeable_point(found)))
    }
}
