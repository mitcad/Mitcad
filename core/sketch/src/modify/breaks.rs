// SPDX-License-Identifier: MIT
//! Break and Split at Point (mitcad#2108): a curve split into pieces that
//! all stay, at the curves crossing it around a pick (Break, as Trim finds
//! them) or at one point of it (Split at Point). The pieces share their end
//! points, a line's pieces are held `collinear`, an arc's or an elliptical
//! arc's stay on one curve by their shared centre and ends, and the
//! constraints and dimensions of the old curve go to the piece they belong
//! to.

use std::collections::BTreeSet;

use super::{Cut, SketchEdit, sketch_size};
use crate::geometry::{Curve2, P2, dist};
use crate::ids::EntityUid;
use crate::{ConstraintKind, ConstraintUid, DimensionKind, EntityKind, Ref};

/// Where a curve is split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakMode {
    /// At the crossings that bound the piece around the pick, on one or
    /// both sides (Break).
    Crossings,
    /// At the curve point nearest the pick, or at an existing point
    /// (Split at Point).
    Point,
}

/// The pick of a break: a position, or an existing point (Split at Point:
/// it becomes the pieces' shared end).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BreakAt {
    At(P2),
    Point(EntityUid),
}

/// One place a curve is split: its parameter on the solved curve, the
/// pieces' shared end there and the existing point it joined.
struct Split {
    t: f64,
    point: EntityUid,
    joined: Option<EntityUid>,
}

/// A piece and its parameter range on the old curve; `t1 < t0` runs across
/// a closed curve's seam.
struct Piece {
    id: EntityUid,
    t0: f64,
    t1: f64,
}

impl Piece {
    fn holds(&self, t: f64) -> bool {
        if self.t0 <= self.t1 {
            t >= self.t0 && t <= self.t1
        } else {
            t >= self.t0 || t <= self.t1
        }
    }
}

/// How many samples along the old curve find where another curve touches
/// it (a tangent without a point of its own).
const TOUCH_SAMPLES: usize = 256;

impl SketchEdit<'_> {
    /// Splits the curve `id` into pieces that all stay (Break, Split at
    /// Point; mitcad#2108). Returns the pieces in order along the curve,
    /// `id` first (it keeps the first piece; of a closed curve, the piece
    /// away from the pick).
    pub fn break_curve(
        &mut self,
        id: EntityUid,
        at: BreakAt,
        mode: BreakMode,
    ) -> Result<Vec<EntityUid>, String> {
        self.curve(id)?;
        if self.def.entity(id).is_some_and(|e| e.axis.is_some()) {
            return Err(crate::said(
                "the sketch's axes cannot be split",
                format!("c{} is a sketch axis", id.0),
            ));
        }
        // As Trim: an offset's curve is first freed from its source
        // (mitcad#1755), a pattern's from its pattern (mitcad#2007); a
        // projected curve is refused.
        self.free_offsets_of(&[id])?;
        self.free_patterns_of(&[id]);
        self.shape_editable(id)?;
        let solved = self.solved()?;
        let curve = solved
            .curves
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("curve c{} does not exist", id.0))?;
        let kind = self.curve(id)?.kind.clone();
        let size = sketch_size(&solved.curves);
        let reach = 1e-7 * size;
        let pick = match at {
            BreakAt::At(p) => p,
            BreakAt::Point(q) => solved
                .points
                .get(&q)
                .copied()
                .map_or_else(|| self.existing_point(q), Ok)?,
        };
        let (lo, hi) = curve.domain();
        let closed = curve.is_closed();
        let mut splits: Vec<Split> = Vec::new();
        match mode {
            BreakMode::Crossings => {
                let (_, cuts) = self.crossings(id, &solved)?;
                let (t_at, _) = curve.closest(pick);
                let chosen: Vec<&Cut> = if closed {
                    if cuts.len() < 2 {
                        return Err(crate::said(
                            "a closed curve breaks between two curves crossing it, and fewer \
                             cross this one",
                            format!("fewer than two curves cross c{}", id.0),
                        ));
                    }
                    let after = cuts.iter().position(|c| c.t > t_at).unwrap_or(0);
                    let before = (after + cuts.len() - 1) % cuts.len();
                    if (cuts[after].t - cuts[before].t).abs() <= 1e-9 * (hi - lo) {
                        return Err(crate::said(
                            "the curves crossing this closed curve meet it at one place only",
                            format!("the crossings of c{} are all at one place", id.0),
                        ));
                    }
                    vec![&cuts[before], &cuts[after]]
                } else {
                    let below = cuts.iter().rev().find(|c| c.t < t_at);
                    let above = cuts.iter().find(|c| c.t > t_at);
                    if below.is_none() && above.is_none() {
                        return Err(crate::said(
                            "no other curve crosses this curve, so there is nowhere to break it",
                            format!("no curve crosses c{}", id.0),
                        ));
                    }
                    below.into_iter().chain(above).collect()
                };
                for cut in chosen {
                    let (point, joined) = self.cut_end(id, &curve, cut, &solved);
                    splits.push(Split {
                        t: cut.t,
                        point,
                        joined,
                    });
                }
            }
            BreakMode::Point => {
                if closed {
                    return Err(crate::said(
                        "a closed curve cannot be opened at one point: break it between two \
                         curves crossing it instead",
                        format!("c{} is closed", id.0),
                    ));
                }
                let (t, off) = curve.closest(pick);
                let on = curve.point(t);
                let (start, end) = curve.ends().expect("an open curve");
                let own = kind.points();
                let at_end = dist(on, start) <= reach
                    || dist(on, end) <= reach
                    || matches!(at, BreakAt::Point(q)
                        if kind.ends().is_some_and(|(s, e)| s == q || e == q));
                if at_end {
                    return Err(crate::said(
                        "the curve already ends there",
                        format!("c{} ends at the split point", id.0),
                    ));
                }
                let (point, joined) = match at {
                    BreakAt::Point(q) => {
                        if off > reach.max(1e-6 * size) {
                            return Err(crate::said(
                                "the point is not on the curve",
                                format!("p{} is not on c{}", q.0, id.0),
                            ));
                        }
                        if own.contains(&q) {
                            // A spline's own inner point: a new end there.
                            (self.add_point(on), None)
                        } else if self.mergeable_point(q) {
                            (q, Some(q))
                        } else {
                            // A projected or computed point holds a new end
                            // (as Trim's cut ends, `cut_end`).
                            let p = self.add_point(pick);
                            if self.computing_offset(q).is_none() {
                                self.add_constraint(ConstraintKind::Coincident {
                                    point: p,
                                    entity: Ref::Point(q),
                                });
                            }
                            (p, Some(q))
                        }
                    }
                    BreakAt::At(_) => (self.add_point(on), None),
                };
                splits.push(Split { t, point, joined });
            }
        }
        // The old ends, which a line length's driven distance measures.
        let old_ends = kind.ends();
        let pieces = if closed {
            self.split_closed(id, &kind, &curve, &splits)?
        } else {
            self.split_open(id, &kind, &curve, &mut splits)?
        };
        let ids: Vec<EntityUid> = pieces.iter().map(|p| p.id).collect();
        self.hand_over(
            id,
            &curve,
            &pieces,
            &solved.points,
            &solved.curves,
            old_ends,
        )?;
        let joined: Vec<EntityUid> = splits.iter().filter_map(|s| s.joined).collect();
        self.drop_idle_after_trim(&joined, &ids);
        Ok(ids)
    }

    /// The pieces of an open curve between its splits: `id` keeps the
    /// first, copies take the others.
    fn split_open(
        &mut self,
        id: EntityUid,
        kind: &EntityKind,
        curve: &Curve2,
        splits: &mut [Split],
    ) -> Result<Vec<Piece>, String> {
        splits.sort_by(|a, b| a.t.total_cmp(&b.t));
        let (lo, hi) = curve.domain();
        let mut bounds = vec![lo];
        bounds.extend(splits.iter().map(|s| s.t));
        bounds.push(hi);
        let mut pieces = vec![Piece {
            id,
            t0: lo,
            t1: splits[0].t,
        }];
        // The copies first: a spline's piece takes its control points from
        // the solved curve, and the old inner points go with the last.
        for k in 1..=splits.len() {
            let copy = self.id();
            let mut entity = self.curve(id)?.clone();
            entity.id = copy;
            self.push(entity);
            let end = splits.get(k).map(|s| s.point);
            self.reshape(
                copy,
                curve,
                bounds[k],
                bounds[k + 1],
                Some(splits[k - 1].point),
                end,
            )?;
            pieces.push(Piece {
                id: copy,
                t0: bounds[k],
                t1: bounds[k + 1],
            });
        }
        self.reshape(id, curve, lo, splits[0].t, None, Some(splits[0].point))?;
        self.hold_pieces(kind, &pieces);
        Ok(pieces)
    }

    /// The two pieces of a closed curve between the splits around the
    /// pick: `id` keeps the rest, from the split after the pick round to
    /// the one before it; a copy takes the piece around the pick.
    fn split_closed(
        &mut self,
        id: EntityUid,
        kind: &EntityKind,
        curve: &Curve2,
        splits: &[Split],
    ) -> Result<Vec<Piece>, String> {
        let (before, after) = (&splits[0], &splits[1]);
        let copy = self.id();
        let mut entity = self.curve(id)?.clone();
        entity.id = copy;
        let arcs = match kind {
            EntityKind::Circle { center, .. } => Some((
                EntityKind::Arc {
                    center: *center,
                    start: after.point,
                    end: before.point,
                },
                EntityKind::Arc {
                    center: *center,
                    start: before.point,
                    end: after.point,
                },
            )),
            EntityKind::Ellipse {
                center,
                major,
                minor_radius,
            } => Some((
                EntityKind::EllipticalArc {
                    center: *center,
                    major: *major,
                    minor_radius: *minor_radius,
                    start: after.point,
                    end: before.point,
                },
                EntityKind::EllipticalArc {
                    center: *center,
                    major: *major,
                    minor_radius: *minor_radius,
                    start: before.point,
                    end: after.point,
                },
            )),
            EntityKind::Spline { .. } | EntityKind::FittedSpline { .. } => None,
            _ => {
                return Err(crate::said(
                    "this closed curve cannot be split",
                    format!("c{} cannot be split", id.0),
                ));
            }
        };
        match arcs {
            Some((rest, around)) => {
                entity.kind = around;
                self.push(entity);
                self.def.entity_mut(id).expect("a curve").kind = rest;
            }
            None => {
                // A closed spline (as Trim's, mitcad#783): both pieces as
                // splines by control points, either across the seam.
                self.push(entity);
                self.reshape(
                    copy,
                    curve,
                    before.t,
                    after.t,
                    Some(before.point),
                    Some(after.point),
                )?;
                self.reshape(
                    id,
                    curve,
                    after.t,
                    before.t,
                    Some(after.point),
                    Some(before.point),
                )?;
            }
        }
        let pieces = vec![
            Piece {
                id,
                t0: after.t,
                t1: before.t,
            },
            Piece {
                id: copy,
                t0: before.t,
                t1: after.t,
            },
        ];
        self.hold_pieces(kind, &pieces);
        Ok(pieces)
    }

    /// Keeps the pieces one curve: a line's pieces `collinear`, each with
    /// the one before; a spline's
    /// tangent where they meet, when the sketch still solves with it. An
    /// arc's or an elliptical arc's pieces need nothing: they share the
    /// centre (and an ellipse's major point) and an end at each split, so
    /// they stay on one circle or ellipse, and an `equal` between them (as
    /// Trim's halves have, mitcad#1314, which share no point) would be
    /// redundant.
    fn hold_pieces(&mut self, kind: &EntityKind, pieces: &[Piece]) {
        match kind {
            // Each with the one before: their shared end leaves one
            // equation each, none redundant.
            EntityKind::Line { .. } => {
                for pair in pieces.windows(2) {
                    self.add_constraint(ConstraintKind::Collinear {
                        a: pair[0].id,
                        b: pair[1].id,
                    });
                }
            }
            EntityKind::Spline { .. } | EntityKind::FittedSpline { .. } => {
                for pair in pieces.windows(2) {
                    self.add_tangent_if_safe(pair[0].id, pair[1].id);
                }
            }
            _ => {}
        }
    }

    /// The constraints and dimensions of the old curve go to the piece
    /// they belong to (mitcad#2108): a point on the curve to the piece it
    /// lies on, a tangent to the piece that touches; a midpoint goes (no
    /// piece has the old middle), and what measured the whole curve: a line
    /// length becomes the driven distance between its old ends, an arc
    /// length, a line's equal length and a symmetry of the whole curve go.
    /// Everything else stays on the first piece, which the others follow
    /// (`collinear`, `equal`). What went is counted in the notes, in words.
    fn hand_over(
        &mut self,
        id: EntityUid,
        curve: &Curve2,
        pieces: &[Piece],
        points: &std::collections::BTreeMap<EntityUid, P2>,
        curves: &std::collections::BTreeMap<EntityUid, Curve2>,
        old_ends: Option<(EntityUid, EntityUid)>,
    ) -> Result<(), String> {
        let is_line = matches!(curve, Curve2::Line { .. });
        let piece_at = |t: f64| -> EntityUid {
            pieces
                .iter()
                .find(|p| p.holds(t))
                .map_or(pieces[0].id, |p| p.id)
        };
        let piece_of_point = |q: EntityUid| -> Option<EntityUid> {
            let at = points.get(&q).copied()?;
            Some(piece_at(curve.closest(at).0))
        };
        let piece_touching = |other: EntityUid| -> Option<EntityUid> {
            let c = curves.get(&other)?;
            let (lo, hi) = curve.domain();
            let t = (0..=TOUCH_SAMPLES)
                .map(|i| lo + (hi - lo) * i as f64 / TOUCH_SAMPLES as f64)
                .map(|t| (t, c.closest(curve.point(t)).1))
                .min_by(|a, b| a.1.total_cmp(&b.1))?
                .0;
            Some(piece_at(t))
        };
        let ends_of = |this: &Self, piece: EntityUid| -> Vec<EntityUid> {
            this.def
                .entity(piece)
                .and_then(|e| e.kind.ends())
                .map(|(s, e)| vec![s, e])
                .unwrap_or_default()
        };
        let mut whole: Vec<ConstraintUid> = Vec::new();
        let mut retargeted: Vec<(usize, ConstraintKind)> = Vec::new();
        for (i, c) in self.def.constraints.iter().enumerate() {
            if !c.kind.refs().contains(&Ref::Curve(id)) {
                continue;
            }
            match c.kind {
                ConstraintKind::Coincident {
                    point,
                    entity: Ref::Curve(_),
                } => {
                    if let Some(piece) = piece_of_point(point)
                        && piece != id
                    {
                        retargeted.push((
                            i,
                            ConstraintKind::Coincident {
                                point,
                                entity: Ref::Curve(piece),
                            },
                        ));
                    }
                }
                ConstraintKind::Tangent { a, b, at } | ConstraintKind::Smooth { a, b, at } => {
                    let other = if a == id { b } else { a };
                    let piece = match at {
                        Some(p) => pieces
                            .iter()
                            .map(|piece| piece.id)
                            .find(|piece| ends_of(self, *piece).contains(&p))
                            .or_else(|| piece_of_point(p)),
                        None => piece_touching(other),
                    };
                    if let Some(piece) = piece.filter(|piece| *piece != id) {
                        let (a, b) = if a == id { (piece, b) } else { (a, piece) };
                        let kind = match c.kind {
                            ConstraintKind::Tangent { .. } => ConstraintKind::Tangent { a, b, at },
                            _ => ConstraintKind::Smooth { a, b, at },
                        };
                        retargeted.push((i, kind));
                    }
                }
                ConstraintKind::Equal { .. } if is_line => whole.push(c.id),
                ConstraintKind::Symmetric { a, b, .. }
                    if a == Ref::Curve(id) || b == Ref::Curve(id) =>
                {
                    whole.push(c.id)
                }
                _ => {}
            }
        }
        for (i, kind) in retargeted {
            self.def.constraints[i].kind = kind;
        }
        let midpoint_ids: BTreeSet<ConstraintUid> = self
            .def
            .constraints
            .iter()
            .filter(|c| matches!(c.kind, ConstraintKind::Midpoint { curve: m, .. } if m == id))
            .map(|c| c.id)
            .collect();
        // Dimensions: a line's length becomes the driven distance between
        // its old ends; an arc length goes; the rest stay on the first piece.
        let mut lengths: Vec<ConstraintUid> = Vec::new();
        for d in &self.def.dimensions {
            match d.kind {
                DimensionKind::Length { line } if line == id => lengths.push(d.id),
                DimensionKind::ArcLength { arc } if arc == id => whole.push(d.id),
                _ => {}
            }
        }
        if !midpoint_ids.is_empty() {
            self.drop_constraints(&midpoint_ids);
            let midpoints = midpoint_ids.len();
            let said = if midpoints == 1 {
                "a midpoint constraint on the split curve was removed: no piece has the old \
                 middle"
                    .to_owned()
            } else {
                format!(
                    "{midpoints} midpoint constraints on the split curve were removed: no piece \
                     has the old middle"
                )
            };
            self.report.notes.push(crate::said(
                said,
                format!(
                    "{} removed: midpoint of c{}, which no piece has",
                    names(&midpoint_ids),
                    id.0
                ),
            ));
        }
        if !whole.is_empty() {
            let gone: BTreeSet<ConstraintUid> = whole.iter().copied().collect();
            self.drop_constraints(&gone);
            let said = if gone.len() == 1 {
                "a constraint or dimension that held the whole split curve was removed".to_owned()
            } else {
                format!(
                    "{} constraints or dimensions that held the whole split curve were removed",
                    gone.len()
                )
            };
            self.report.notes.push(crate::said(
                said,
                format!("{} removed: they held the whole of c{}", names(&gone), id.0),
            ));
        }
        if let (false, Some((start, end))) = (lengths.is_empty(), old_ends) {
            for length in &lengths {
                if let Some(d) = self.def.dimensions.iter_mut().find(|d| d.id == *length) {
                    d.kind = DimensionKind::Distance { a: start, b: end };
                }
                self.set_driven(*length, true)?;
            }
            let shown: BTreeSet<ConstraintUid> = lengths.iter().copied().collect();
            self.report.notes.push(crate::said(
                "the line's length dimension now measures the whole of its pieces, as a driven \
                 dimension",
                format!(
                    "{} now a driven distance between p{} and p{}, the ends of the pieces of c{}",
                    names(&shown),
                    start.0,
                    end.0,
                    id.0
                ),
            ));
        }
        Ok(())
    }
}

fn names(ids: &BTreeSet<ConstraintUid>) -> String {
    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
