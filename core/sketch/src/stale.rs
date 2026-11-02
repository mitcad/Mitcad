// SPDX-License-Identifier: MIT
//! Clean Up Stale Projections (mitcad#252): projected curves that no
//! longer match their source (the source changed shape since: an imported
//! projection, a face edited upstream) are tidied away.
//!
//! The stale curves are those the last evaluation found matching none of
//! their source's current curves ([`ProjectionFollow::stale`]: curves of a
//! projection that kept its geometry, of a lost source, or that followed
//! nothing). Of them:
//!
//! 1. a curve that matters to nothing goes: it bounds (or names a segment
//!    of) no region a later feature uses, no pattern, offset or text uses
//!    it, no constraint or dimension ties it or the points that go with it
//!    to other geometry, and no curve of the user's ends on it;
//! 2. a stale line that stays because it bounds a used region, and runs
//!    past it, is trimmed to where the region's last segment on it ends,
//!    when nothing else holds its end there and every used region keeps
//!    its name and area;
//! 3. the projections that still do not follow all the way (kept, lost,
//!    or with stale or unfollowed entities left) are unlinked: their
//!    entities stay as fixed references and the sketch stops warning.
//!    They are not linked again by geometry: a projection that follows by
//!    geometry moves its entities to the nearest curves of its source,
//!    which can change the profiles later features see (mitcad#252).

use std::collections::{BTreeMap, BTreeSet};

use super::edit::SketchEdit;
use super::geometry::{Curve2, P2, dist, dot, norm, scale, sub};
use super::modify::Item;
use super::project::ProjectionFollow;
use super::regions::Region;
use super::text::TextOutline;
use super::{ConstraintKind, Entity, EntityKind, Ref};
use crate::ids::EntityUid;
use crate::profile::SegmentGeometry;
use crate::topo::CurveId;

/// How far, relative to the size of the line, its end must run past the
/// used region to be trimmed.
const TRIM: f64 = 1e-6;

/// Why a stale curve stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// It bounds, or names a segment of, a region a later feature uses.
    Region,
    /// A curve of the user's ends on it.
    UserCurve,
    /// A constraint or a dimension ties it, or a point that would go with
    /// it, to other geometry.
    Constrained,
    /// A pattern, an offset or a text uses it.
    Used,
}

impl Keep {
    pub fn name(self) -> &'static str {
        match self {
            Self::Region => "region",
            Self::UserCurve => "user_curve",
            Self::Constrained => "constrained",
            Self::Used => "used",
        }
    }

    /// The reason in words, for the command's warnings.
    pub fn words(self) -> &'static str {
        match self {
            Self::Region => "bounding regions later features use",
            Self::UserCurve => "with curves of the user's ending on them",
            Self::Constrained => "with constraints or dimensions on them",
            Self::Used => "used by patterns, offsets or texts",
        }
    }
}

/// What [`SketchEdit::clean_up_stale_projections`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StaleCleanUp {
    /// The stale curves removed.
    pub removed: Vec<EntityUid>,
    /// The stale lines trimmed to the regions later features use.
    pub trimmed: Vec<EntityUid>,
    /// The sources of the projections unlinked.
    pub unlinked: Vec<String>,
    /// The stale curves that stay, and why.
    pub kept: Vec<(EntityUid, Keep)>,
    /// The removed curves' geometry before the clean-up (their kind's name
    /// and solved curve), for a preview to show what goes.
    pub removed_geometry: Vec<(EntityUid, &'static str, Curve2)>,
    /// The pieces trimmed off lines: the line, and where each piece ran
    /// (the end's old place, then its new one).
    pub cut: Vec<(EntityUid, P2, P2)>,
}

impl SketchEdit<'_> {
    /// Removes stale projected curves that matter to nothing, trims stale
    /// lines that run past the regions later features use, and unlinks the
    /// projections that still do not follow (see the module
    /// documentation). `follow` is the last evaluation's
    /// ([`crate::def::SketchOutput::follow`]), `used` the regions later
    /// features use and `texts` the texts' outlines, as they evaluated.
    /// Fails when there is nothing to clean up.
    pub fn clean_up_stale_projections(
        &mut self,
        follow: &[ProjectionFollow],
        used: &[Region],
        texts: &[TextOutline],
    ) -> Result<StaleCleanUp, String> {
        let stale: BTreeSet<EntityUid> = follow
            .iter()
            .flat_map(|r| r.stale.iter().copied())
            .filter(|id| self.def.entity(*id).is_some_and(|e| !e.is_point()))
            .collect();
        // The curves the used regions are named by (their segments' and
        // those at the segments' ends), holes included.
        let named: BTreeSet<EntityUid> = used
            .iter()
            .flat_map(|r| &r.profile.loops)
            .flat_map(|l| &l.segments)
            .flat_map(|s| s.key.curves())
            .filter_map(|curve| curve.entity())
            .collect();
        let mut keep: BTreeMap<EntityUid, Keep> = BTreeMap::new();
        for id in &stale {
            if named.contains(id) {
                keep.insert(*id, Keep::Region);
            } else if self.used_by_others(*id) {
                keep.insert(*id, Keep::Used);
            }
        }
        // What ties a curve to geometry that stays, until nothing more
        // stays.
        loop {
            let going: BTreeSet<EntityUid> = stale
                .iter()
                .copied()
                .filter(|id| !keep.contains_key(id))
                .collect();
            let points = self.points_going(&going);
            let tied: Vec<(EntityUid, Keep)> = going
                .iter()
                .filter_map(|id| Some((*id, self.ties(*id, &going, &points)?)))
                .collect();
            if tied.is_empty() {
                break;
            }
            keep.extend(tied);
        }
        let going: BTreeSet<EntityUid> = stale
            .iter()
            .copied()
            .filter(|id| !keep.contains_key(id))
            .collect();
        let points = self.points_going(&going);
        let mut out = StaleCleanUp::default();
        // The projections that would still not follow are unlinked first;
        // one whose entities all go goes with them.
        let mut unlink: Vec<usize> = Vec::new();
        for report in follow {
            let Some(projection) = self.def.projections.get(report.projection) else {
                continue;
            };
            let left: Vec<EntityUid> = projection
                .entities
                .iter()
                .map(|r| r.uid())
                .filter(|id| {
                    !going.contains(id) && !points.contains(id) && self.def.entity(*id).is_some()
                })
                .collect();
            let warns = report.lost
                || report.kept
                || left
                    .iter()
                    .any(|id| stale.contains(id) || report.unfollowed.contains(id));
            if !left.is_empty() && warns && !unlink.contains(&report.projection) {
                unlink.push(report.projection);
            }
        }
        unlink.sort_unstable();
        for k in unlink.into_iter().rev() {
            let projection = self.def.projections.remove(k);
            out.unlinked.insert(0, projection.source.to_string());
        }
        if !going.is_empty() {
            // Their geometry as it is now, for a preview to show.
            if let Ok(solved) = self.solved() {
                for id in &going {
                    if let (Some(e), Some(curve)) = (self.def.entity(*id), solved.curves.get(id)) {
                        out.removed_geometry
                            .push((*id, e.kind.type_name(), curve.clone()));
                    }
                }
            }
            let items: Vec<Item> = going
                .iter()
                .map(|id| Item::Entity(Ref::Curve(*id)))
                .collect();
            self.remove(&items)?;
            out.removed = going.iter().copied().collect();
        }
        // Stale lines that bound used regions and run past them.
        for id in &stale {
            if !named.contains(id) {
                continue;
            }
            let pieces = self.trim_to_used(*id, &stale, used, texts)?;
            if !pieces.is_empty() {
                out.trimmed.push(*id);
                out.cut
                    .extend(pieces.into_iter().map(|(from, to)| (*id, from, to)));
            }
        }
        out.kept = keep.into_iter().collect();
        if out.removed.is_empty() && out.trimmed.is_empty() && out.unlinked.is_empty() {
            return Err(if stale.is_empty() {
                "nothing to clean up: every linked projection matches its source".to_owned()
            } else {
                "nothing to clean up: every stale projected curve is still used".to_owned()
            });
        }
        Ok(out)
    }

    /// The points that go with `curves`: theirs that no other entity uses.
    fn points_going(&self, curves: &BTreeSet<EntityUid>) -> BTreeSet<EntityUid> {
        let mut out = BTreeSet::new();
        for id in curves {
            let Some(e) = self.def.entity(*id) else {
                continue;
            };
            for p in e.kind.points() {
                let elsewhere = self
                    .def
                    .entities
                    .iter()
                    .any(|o| !curves.contains(&o.id) && o.kind.points().contains(&p));
                if !elsewhere {
                    out.insert(p);
                }
            }
        }
        out
    }

    /// What keeps a curve of `going` (with `points`, those that go with
    /// them): a curve of the user's on one of its points, or a constraint,
    /// a dimension, a pattern or a text on it or a point of it that goes,
    /// that also refers to something that stays.
    fn ties(
        &self,
        curve: EntityUid,
        going: &BTreeSet<EntityUid>,
        points: &BTreeSet<EntityUid>,
    ) -> Option<Keep> {
        let own = self.def.entity(curve)?.kind.points();
        // A curve of the user's that uses a point.
        let user = |p: EntityUid| {
            self.def.entities.iter().any(|o| {
                o.id != curve && !o.reference && !o.is_point() && o.kind.points().contains(&p)
            })
        };
        if own.iter().any(|p| user(*p)) {
            return Some(Keep::UserCurve);
        }
        let mine =
            |r: &Ref| r.uid() == curve || (own.contains(&r.uid()) && points.contains(&r.uid()));
        let inside = |r: &Ref| going.contains(&r.uid()) || points.contains(&r.uid());
        let tying = |refs: &[Ref]| refs.iter().any(mine) && !refs.iter().all(inside);
        for c in &self.def.constraints {
            if tying(c.kind.refs().as_slice()) {
                return Some(match c.kind {
                    ConstraintKind::Coincident { point, .. } if user(point) => Keep::UserCurve,
                    _ => Keep::Constrained,
                });
            }
        }
        if self
            .def
            .dimensions
            .iter()
            .any(|d| tying(d.kind.refs().as_slice()))
        {
            return Some(Keep::Constrained);
        }
        let goes = |p: &EntityUid| own.contains(p) && points.contains(p);
        let used = self
            .def
            .patterns
            .iter()
            .any(|p| p.entities.iter().any(|r| goes(&r.uid())))
            || self.def.texts.iter().any(|t| t.frame.iter().any(goes));
        used.then_some(Keep::Used)
    }

    /// Trims a stale line's ends that run past the used regions to where
    /// the last of their segments on it ends. Only a free end moves (no
    /// constraint, dimension, pattern or text on its point, and no other
    /// curve but `stale` lines, from which it is parted), only a line no
    /// projection and no dimension holds, and only when the points
    /// constrained onto it stay on what is left; the trim is undone unless
    /// every used region keeps its name and area.
    fn trim_to_used(
        &mut self,
        line: EntityUid,
        stale: &BTreeSet<EntityUid>,
        used: &[Region],
        texts: &[TextOutline],
    ) -> Result<Vec<(P2, P2)>, String> {
        let Some(EntityKind::Line { start, end, .. }) = self.def.entity(line).map(|e| &e.kind)
        else {
            return Ok(Vec::new());
        };
        let (start, end) = (*start, *end);
        let held = self
            .def
            .projections
            .iter()
            .any(|p| p.entities.iter().any(|r| r.uid() == line))
            || self
                .def
                .dimensions
                .iter()
                .any(|d| d.kind.refs().contains(&Ref::Curve(line)));
        if held {
            return Ok(Vec::new());
        }
        let (a, b) = (self.at(start)?, self.at(end)?);
        let length = dist(a, b);
        let tol = TRIM * norm(a).max(norm(b)).max(length).max(1.0);
        if length <= tol {
            return Ok(Vec::new());
        }
        let u = scale(sub(b, a), 1.0 / length);
        let param = |p: P2| dot(sub(p, a), u);
        // The used segments' extent along it.
        let mut lo: Option<(f64, P2)> = None;
        let mut hi: Option<(f64, P2)> = None;
        for s in used
            .iter()
            .flat_map(|r| &r.profile.loops)
            .flat_map(|l| &l.segments)
        {
            if s.key.curve != CurveId::Curve(line) {
                continue;
            }
            let SegmentGeometry::Line { start: s0, end: s1 } = &s.geometry else {
                continue;
            };
            for q in [*s0, *s1] {
                let t = param(q);
                if lo.is_none_or(|(l, _)| t < l) {
                    lo = Some((t, q));
                }
                if hi.is_none_or(|(h, _)| t > h) {
                    hi = Some((t, q));
                }
            }
        }
        let (Some((lo, lo_at)), Some((hi, hi_at))) = (lo, hi) else {
            return Ok(Vec::new());
        };
        let mut moves: Vec<(EntityUid, P2, bool)> = Vec::new();
        if lo > tol
            && let Some(detach) = self.end_move(start, line, stale)
        {
            moves.push((start, lo_at, detach));
        }
        if hi < length - tol
            && let Some(detach) = self.end_move(end, line, stale)
        {
            moves.push((end, hi_at, detach));
        }
        if moves.is_empty() {
            return Ok(Vec::new());
        }
        // What is left of it, and what is constrained onto it.
        let trims = |p: EntityUid| moves.iter().any(|m| m.0 == p);
        let (from, to) = (
            if trims(start) { lo } else { 0.0 },
            if trims(end) { hi } else { length },
        );
        for c in &self.def.constraints {
            if !c.kind.refs().contains(&Ref::Curve(line)) {
                continue;
            }
            let ConstraintKind::Coincident {
                point,
                entity: Ref::Curve(_),
            } = c.kind
            else {
                return Ok(Vec::new());
            };
            let t = param(self.at(point)?);
            if t < from - tol || t > to + tol {
                return Ok(Vec::new());
            }
        }
        // The pieces cut off: from each end where it was to where it goes.
        let mut pieces: Vec<(P2, P2)> = Vec::new();
        for (p, at, _) in &moves {
            pieces.push((self.at(*p)?, *at));
        }
        let before = self.def.entities.clone();
        let mut made: Vec<EntityUid> = Vec::new();
        for (p, at, detach) in moves {
            if detach {
                // A point of its own where the corner it shared was.
                let id = self.id();
                let mut point = Entity::point(id, at);
                point.fixed = true;
                point.reference = true;
                self.def.entities.push(point);
                if let Some(e) = self.def.entity_mut(line) {
                    e.kind.replace_point(p, id);
                }
                made.push(id);
            } else if let Some(EntityKind::Point { at: was }) =
                self.def.entity_mut(p).map(|e| &mut e.kind)
            {
                *was = at;
            }
        }
        if self.regions_kept(used, texts) {
            self.report
                .entities
                .extend(made.into_iter().map(Ref::Point));
            return Ok(pieces);
        }
        self.def.entities = before;
        Ok(Vec::new())
    }

    /// How a trimmed line's end at `point` moves: the point itself when
    /// nothing but the line refers to it (false), a point of its own for
    /// the line when only other stale reference lines share it, at a
    /// corner of the old outline (true); None when something else holds
    /// it.
    fn end_move(
        &self,
        point: EntityUid,
        line: EntityUid,
        stale: &BTreeSet<EntityUid>,
    ) -> Option<bool> {
        if !self.referred_to(point, line) {
            return Some(false);
        }
        let refers = |refs: &[Ref]| refs.iter().any(|r| r.uid() == point);
        let held = self
            .def
            .constraints
            .iter()
            .any(|c| refers(c.kind.refs().as_slice()))
            || self
                .def
                .dimensions
                .iter()
                .any(|d| refers(d.kind.refs().as_slice()))
            || self
                .def
                .patterns
                .iter()
                .any(|p| refers(p.entities.as_slice()))
            || self.def.texts.iter().any(|t| t.frame.contains(&point));
        let shared_by_stale_lines = self
            .def
            .entities
            .iter()
            .filter(|e| e.id != line && e.kind.points().contains(&point))
            .all(|e| {
                e.reference && stale.contains(&e.id) && matches!(e.kind, EntityKind::Line { .. })
            });
        (!held && shared_by_stale_lines).then_some(true)
    }

    /// Whether every used region is still there with its name and area.
    fn regions_kept(&self, used: &[Region], texts: &[TextOutline]) -> bool {
        let Ok(solved) = self.solved() else {
            return false;
        };
        let regions = self.def.regions_of(&solved, texts);
        used.iter().all(|u| {
            regions.iter().any(|r| {
                r.profile.key == u.profile.key
                    && (r.area - u.area).abs() <= 1e-6 * u.area.abs().max(1.0)
            })
        })
    }
}
