// SPDX-License-Identifier: MIT
//! Clean Up Projections (mitcad#232): projected curves that are the same as
//! another (a face projected twice, faces that share an edge) go, and with
//! `merge` projected lines that lie on a longer collinear one go too, or
//! two collinear unlinked ones that overlap become one. What referred to a
//! removed curve and its points (constraints, dimensions, other curves'
//! ends) refers to the kept curve and its points; constraints that then
//! say nothing (a curve parallel to itself) go.
//!
//! The kept curve is the one with the lower id, the one that lies along
//! the other, or the longer: the region finder keeps the lower id of
//! curves that lie on each other, so removing the same curve twice keeps
//! the regions' names. A kept curve follows only its own projection's
//! source: the removed one's link is gone with it.

use std::collections::{BTreeMap, BTreeSet};

use super::edit::SketchEdit;
use super::geometry::{P2, cross, dist, dot, norm, scale, sub};
use super::modify::remap;
use super::project::{SAME, unlikeness};
use super::{ConstraintKind, DimensionKind, Entity, EntityIndex, EntityKind, Ref};
use crate::ids::EntityUid;

/// Why a curve was removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    /// The same as the kept curve.
    Same,
    /// A line on the kept, longer one.
    On,
    /// A line joined to the kept one it overlaps.
    Joined,
}

impl Removal {
    pub fn name(self) -> &'static str {
        match self {
            Self::Same => "same",
            Self::On => "on",
            Self::Joined => "joined",
        }
    }
}

/// A curve the clean-up removed.
#[derive(Debug, Clone, PartialEq)]
pub struct Removed {
    pub curve: EntityUid,
    pub kept: EntityUid,
    pub how: Removal,
    /// The source the removed curve followed when the kept one does not
    /// follow it (another projection's, or none).
    pub link: Option<String>,
}

impl SketchEdit<'_> {
    /// Removes projected curves the same as another and, with `merge`,
    /// projected lines on or overlapping another (see the module
    /// documentation). Fails when there is nothing to clean up.
    pub fn clean_up_projections(&mut self, merge: bool) -> Result<Vec<Removed>, String> {
        let mut removed = Vec::new();
        let rounds: &[Removal] = if merge {
            &[Removal::Same, Removal::On, Removal::Joined]
        } else {
            &[Removal::Same]
        };
        for how in rounds {
            for (curve, kept, join) in self.cleanups(*how) {
                // An earlier removal of this round may have taken one.
                if self.def.entity(curve).is_none() || self.def.entity(kept).is_none() {
                    continue;
                }
                let link = self.lost_link(curve, kept);
                if self.remove_into(curve, kept, join) {
                    removed.push(Removed {
                        curve,
                        kept,
                        how: *how,
                        link,
                    });
                }
            }
        }
        if removed.is_empty() {
            return Err(
                "nothing to clean up: no projected curve is the same as another or \
                        lies on another"
                    .to_owned(),
            );
        }
        Ok(removed)
    }

    /// The source of the linked projection `curve` is in, when `kept` is
    /// not in it.
    fn lost_link(&self, curve: EntityUid, kept: EntityUid) -> Option<String> {
        let of = |id: EntityUid| {
            self.def
                .projections
                .iter()
                .position(|p| p.entities.iter().any(|r| r.uid() == id))
        };
        let from = &self.def.projections[of(curve)?];
        // Another projection of the same source (projected twice) is no
        // link lost.
        let same = of(kept).is_some_and(|k| {
            let to = &self.def.projections[k];
            to.source == from.source && to.body == from.body && to.link == from.link
        });
        (!same).then(|| from.source.to_string())
    }

    /// The removals of one kind: (removed, kept, the kept line's new ends
    /// for a join), each curve removed or kept once.
    fn cleanups(&self, how: Removal) -> Vec<(EntityUid, EntityUid, Option<[EntityUid; 2]>)> {
        let index = EntityIndex::new(&self.def.entities);
        let at = |id: EntityUid| index.point(id);
        let curves: Vec<&Entity> = self
            .def
            .entities
            .iter()
            .filter(|e| e.reference && !e.is_point())
            .collect();
        let mut size = 1.0_f64;
        for e in &curves {
            for p in e.kind.points() {
                size = size.max(at(p).map_or(0.0, norm));
            }
        }
        let tol = SAME * size;
        let linked = |id: EntityUid| {
            self.def
                .projections
                .iter()
                .any(|p| p.entities.iter().any(|r| r.uid() == id))
        };
        let line = |e: &Entity| match e.kind {
            EntityKind::Line { start, end, .. } => Some((start, end, at(start)?, at(end)?)),
            _ => None,
        };
        let mut out = Vec::new();
        let mut used: BTreeSet<EntityUid> = BTreeSet::new();
        for (i, a) in curves.iter().enumerate() {
            for b in &curves[i + 1..] {
                if used.contains(&a.id) || used.contains(&b.id) || a.construction != b.construction
                {
                    continue;
                }
                match how {
                    Removal::Same => {
                        if unlikeness(&a.kind, &at, &b.kind, &at).is_some_and(|(c, _)| c <= tol) {
                            out.push((b.id, a.id, None));
                            used.insert(b.id);
                        }
                    }
                    Removal::On | Removal::Joined => {
                        let (Some(la), Some(lb)) = (line(a), line(b)) else {
                            continue;
                        };
                        let Some((lo, hi)) = along(la.2, la.3, lb.2, lb.3, tol) else {
                            continue;
                        };
                        let length = dist(la.2, la.3);
                        let a_holds_b = lo >= -tol && hi <= length + tol;
                        let b_holds_a = lo <= tol && hi >= length - tol;
                        let overlap = hi.min(length) - lo.max(0.0);
                        if how == Removal::On {
                            if a_holds_b {
                                out.push((b.id, a.id, None));
                                used.insert(b.id);
                            } else if b_holds_a {
                                out.push((a.id, b.id, None));
                                used.insert(a.id);
                            }
                        } else if overlap > tol && !linked(a.id) && !linked(b.id) {
                            // The joined line from the outer ends, in a's
                            // direction.
                            let (s0, s1) = (param(la.2, la.3, lb.2), param(la.2, la.3, lb.3));
                            let (b_lo, b_hi) = if s0 <= s1 { (lb.0, lb.1) } else { (lb.1, lb.0) };
                            let start = if lo < -tol { b_lo } else { la.0 };
                            let end = if hi > length + tol { b_hi } else { la.1 };
                            out.push((b.id, a.id, Some([start, end])));
                            used.insert(a.id);
                            used.insert(b.id);
                        }
                    }
                }
            }
        }
        out
    }

    /// Removes `curve` in favour of `kept` (with its new ends for a join):
    /// what referred to the removed curve and its points refers to the kept
    /// one and the kept one's points where they are. False (nothing done)
    /// when a pattern, an offset or a text uses the removed curve.
    fn remove_into(
        &mut self,
        curve: EntityUid,
        kept: EntityUid,
        join: Option<[EntityUid; 2]>,
    ) -> bool {
        if self.used_by_others(curve) {
            return false;
        }
        let kept_before = self
            .def
            .entity(kept)
            .map(|e| e.kind.points())
            .unwrap_or_default();
        if let Some([start, end]) = join
            && let Some(EntityKind::Line {
                start: s, end: e, ..
            }) = self.def.entity_mut(kept).map(|k| &mut k.kind)
        {
            *s = start;
            *e = end;
        }
        let index = EntityIndex::new(&self.def.entities);
        let (Some(removed), Some(target)) = (index.get(curve), index.get(kept)) else {
            return false;
        };
        let mut size = 1.0_f64;
        for p in removed
            .kind
            .points()
            .into_iter()
            .chain(target.kind.points())
        {
            size = size.max(index.point(p).map_or(0.0, norm));
        }
        let tol = SAME * size;
        // The removed curve's points onto the kept one's where they are.
        let kept_points = target.kind.points();
        let mut map: BTreeMap<EntityUid, EntityUid> = BTreeMap::new();
        map.insert(curve, kept);
        for p in removed.kind.points() {
            if kept_points.contains(&p) {
                continue;
            }
            let Some(at) = index.point(p) else { continue };
            if let Some(k) = kept_points
                .iter()
                .find(|k| index.point(**k).is_some_and(|q| dist(at, q) <= tol))
            {
                map.insert(p, *k);
            }
        }
        // A kept line that runs the other way: the angle dimensions on the
        // removed one keep their sides (mitcad#296).
        let direction = |kind: &EntityKind| match kind {
            EntityKind::Line { start, end, .. } => {
                Some(sub(index.point(*end)?, index.point(*start)?))
            }
            _ => None,
        };
        let reversed = matches!(
            (direction(&removed.kind), direction(&target.kind)),
            (Some(r), Some(k)) if dot(r, k) < 0.0
        );
        let removed_points = removed.kind.points();
        for e in &mut self.def.entities {
            if e.id == curve {
                continue;
            }
            for (from, to) in &map {
                e.kind.replace_point(*from, *to);
            }
        }
        for c in &mut self.def.constraints {
            remap(&mut c.kind, &map);
        }
        for d in &mut self.def.dimensions {
            if reversed {
                reverse_angle_ray(&mut d.kind, curve);
            }
            remap_dimension(&mut d.kind, &map);
        }
        // Constraints and dimensions that now say nothing.
        let index = EntityIndex::new(&self.def.entities);
        let idle = |refs: Vec<Ref>| {
            let ids: BTreeSet<EntityUid> = refs.iter().map(|r| r.uid()).collect();
            ids.len() < refs.len()
        };
        let mut gone: BTreeSet<_> = self
            .def
            .constraints
            .iter()
            .filter(|c| {
                idle(c.kind.refs())
                    || matches!(c.kind, ConstraintKind::Coincident { point, entity: Ref::Curve(on) }
                        if index.get(on).is_some_and(|e| e.kind.points().contains(&point)))
            })
            .map(|c| c.id)
            .collect();
        gone.extend(
            self.def
                .dimensions
                .iter()
                .filter(|d| idle(d.kind.refs()))
                .map(|d| d.id),
        );
        self.drop_constraints(&gone);
        // The removed curve, its points now the kept one's and those
        // nothing else uses.
        let mut entities: BTreeSet<EntityUid> = BTreeSet::new();
        entities.insert(curve);
        for p in removed_points {
            if map.contains_key(&p) || !self.referred_to(p, curve) {
                entities.insert(p);
            }
        }
        // A joined line's former ends inside it.
        for p in kept_before {
            if !self.referred_to(p, curve) {
                entities.insert(p);
            }
        }
        self.drop_entities(&entities);
        true
    }

    /// Whether a pattern, an offset or a text uses a curve.
    pub(super) fn used_by_others(&self, curve: EntityUid) -> bool {
        self.def.patterns.iter().any(|p| {
            p.entities.iter().any(|r| r.uid() == curve)
                || p.copies.iter().any(|c| {
                    c.entities
                        .iter()
                        .any(|(o, m)| o.uid() == curve || m.uid() == curve)
                })
        }) || self
            .def
            .offsets
            .iter()
            .any(|o| o.curves.contains(&curve) || o.results.contains(&curve))
            || self
                .def
                .texts
                .iter()
                .any(|t| t.path.is_some_and(|p| p.curve == curve))
    }

    /// Whether anything but the curve `except` refers to a point: another
    /// curve, a constraint, a dimension, a pattern or a text.
    pub(super) fn referred_to(&self, point: EntityUid, except: EntityUid) -> bool {
        let refers = |refs: Vec<Ref>| refs.iter().any(|r| r.uid() == point);
        self.def
            .entities
            .iter()
            .any(|e| e.id != except && e.kind.points().contains(&point))
            || self.def.constraints.iter().any(|c| refers(c.kind.refs()))
            || self.def.dimensions.iter().any(|d| refers(d.kind.refs()))
            || self.def.patterns.iter().any(|p| refers(p.entities.clone()))
            || self.def.texts.iter().any(|t| t.frame.contains(&point))
    }
}

/// Where a line `(q0, q1)` lies along the line `(p0, p1)`: the lowest and
/// highest parameters (distances from `p0` towards `p1`) of its ends, or
/// None when it is not on the line through `p0` and `p1`.
fn along(p0: P2, p1: P2, q0: P2, q1: P2, tol: f64) -> Option<(f64, f64)> {
    let length = dist(p0, p1);
    if length <= tol {
        return None;
    }
    let u = scale(sub(p1, p0), 1.0 / length);
    let off = |q: P2| cross(u, sub(q, p0)).abs();
    if off(q0) > tol || off(q1) > tol {
        return None;
    }
    let (s0, s1) = (param(p0, p1, q0), param(p0, p1, q1));
    Some((s0.min(s1), s0.max(s1)))
}

/// The distance of `q` from `p0` towards `p1`.
fn param(p0: P2, p1: P2, q: P2) -> f64 {
    let length = dist(p0, p1).max(f64::MIN_POSITIVE);
    dot(sub(p1, p0), sub(q, p0)) / length
}

/// An angle dimension with a side (mitcad#296) measuring from the other
/// ray of `line`: for a line that is to run the other way.
pub(super) fn reverse_angle_ray(kind: &mut DimensionKind, line: EntityUid) {
    if kind.angle_side().is_none() {
        return;
    }
    if let DimensionKind::Angle {
        a,
        b,
        reverse_a,
        reverse_b,
    } = kind
    {
        for (l, reverse) in [(*a, reverse_a), (*b, reverse_b)] {
            if l == line {
                *reverse = Some(!reverse.unwrap_or(false));
            }
        }
    }
}

/// A dimension's entities replaced by `map`.
pub(super) fn remap_dimension(kind: &mut DimensionKind, map: &BTreeMap<EntityUid, EntityUid>) {
    let m = |u: &mut EntityUid| {
        if let Some(c) = map.get(u) {
            *u = *c;
        }
    };
    use DimensionKind as D;
    match kind {
        D::Distance { a, b }
        | D::HorizontalDistance { a, b, .. }
        | D::VerticalDistance { a, b, .. }
        | D::LineDistance { a, b }
        | D::ConcentricDistance { a, b }
        | D::Angle { a, b, .. } => {
            m(a);
            m(b);
        }
        D::PointLineDistance { point, line } => {
            m(point);
            m(line);
        }
        D::TangentDistance { circle, line } => {
            m(circle);
            m(line);
        }
        D::Length { line } => m(line),
        D::Radius { curve } | D::Diameter { curve } => m(curve),
        D::ArcLength { arc } => m(arc),
        D::MajorRadius { ellipse } | D::MinorRadius { ellipse } => m(ellipse),
        D::LinearDiameter { axis, entity } => {
            m(axis);
            match entity {
                Ref::Point(u) | Ref::Curve(u) => m(u),
            }
        }
        D::Spacing { .. } => {}
        D::TangentCircleDistance { a, b, .. } => {
            m(a);
            m(b);
        }
        D::TangentPointDistance { circle, point, .. } => {
            m(circle);
            m(point);
        }
    }
}
