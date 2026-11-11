// SPDX-License-Identifier: MIT
//! Equal spacing (mitcad#221): one constraint that keeps the successive
//! intervals between independent objects equal, without copying them,
//! aligning them or binding their sizes.
//!
//! - Anchors are explicit: a point, the centre of a circle or an arc, the
//!   midpoint of a line. Nothing else (no box centres of arbitrary
//!   curves); two anchors for the same point are refused.
//! - Intervals are A-B, B-C, C-D in the stored order, never every pair.
//!   `horizontal` and `vertical` measure x and y offsets, `along` the
//!   offset projected on a line's direction (the line is kept, so it can
//!   turn later), `point_to_point` the distances. The directional modes
//!   compare signed offsets, so the order holds; nothing across them is
//!   constrained.
//! - Order: a directional spacing is sorted along its direction when it is
//!   made or edited, unless the caller keeps the order given; two anchors
//!   at the same place along it are refused with their names (the caller
//!   orders them). A point-to-point chain takes the order given. The order
//!   is stored and never sorted again by later geometry edits.
//! - The common spacing is optional: one `spacing` dimension
//!   ([`DimensionKind::Spacing`]) drives every interval through the
//!   solver's `Spacing` equation on the first interval, which the equal
//!   intervals pass on. Removing it leaves the equality with a free size;
//!   removing the constraint takes it along.
//! - Deleting an anchor's entity takes it out of the chain (its neighbours
//!   become successive); a chain left with fewer than three anchors, or
//!   without its direction line, is removed with its dimension
//!   (`SketchEdit::drop_entities`).
//! - Anchors that are all instances of one pattern are refused: the
//!   pattern's spacing already governs them (Edit Pattern changes it).
//!   Redundant or conflicting spacings are refused like other constraints
//!   (`sketch.add_equal_spacing`).

use super::edit::SketchEdit;
use super::geometry::{Curve2, P2, dot, norm, sub, unit};
use super::solve::{Solved, SolverItem};
use super::{
    ConstraintKind, ConstraintUid, DimensionKind, EntityIndex, EntityKind, Ref, SpacingMode,
};
use crate::def::SketchDef;
use crate::ids::EntityUid;
use crate::values::ValueInput;

/// What a new or edited equal spacing asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct SpacingInput {
    pub anchors: Vec<Ref>,
    pub mode: SpacingMode,
    /// The line an `along` spacing measures along.
    pub direction: Option<EntityUid>,
    /// Keep the anchors in the order given (a directional spacing is
    /// otherwise sorted along its direction).
    pub keep_order: bool,
}

/// The common spacing a new equal spacing gets.
#[derive(Debug, Clone, PartialEq)]
pub enum SpacingDimension {
    /// No dimension: the common spacing is free.
    None,
    /// A driving dimension with this value, or with the mean of the
    /// current intervals when None.
    Driving(Option<ValueInput>),
    /// A driven dimension (it only measures).
    Driven,
}

/// The points an anchor stands for: a point, or a line's two ends (its
/// midpoint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorPoints {
    Point(EntityUid),
    Mid(EntityUid, EntityUid),
}

/// The points of an anchor: a point itself, a circle's or an arc's centre,
/// a line's ends; None for anything else.
pub fn anchor_points(index: &EntityIndex<'_>, anchor: Ref) -> Option<AnchorPoints> {
    let entity = index.get(anchor.uid())?;
    if entity.is_point() != anchor.is_point() {
        return None;
    }
    Some(match entity.kind {
        EntityKind::Point { .. } => AnchorPoints::Point(entity.id),
        EntityKind::Circle { center, .. } | EntityKind::Arc { center, .. } => {
            AnchorPoints::Point(center)
        }
        EntityKind::Line { start, end, .. } => AnchorPoints::Mid(start, end),
        _ => return None,
    })
}

/// Checks an equal spacing's definition: at least three distinct anchors
/// of the kinds it takes, and a direction line exactly for `along`.
pub fn check(
    index: &EntityIndex<'_>,
    what: &str,
    anchors: &[Ref],
    mode: SpacingMode,
    direction: Option<EntityUid>,
) -> Result<(), String> {
    if anchors.len() < 3 {
        return Err(format!(
            "{what}: an equal spacing needs at least three anchors, got {}",
            anchors.len()
        ));
    }
    let mut seen: Vec<(Ref, AnchorPoints)> = Vec::new();
    for r in anchors {
        let entity = index
            .get(r.uid())
            .ok_or_else(|| format!("{what}: {r} does not exist"))?;
        if entity.is_point() != r.is_point() {
            return Err(format!("{what}: {r} is a {}", entity.kind.type_name()));
        }
        let points = anchor_points(index, *r).ok_or_else(|| {
            format!(
                "{what}: {r} is a {}; anchors are points, circles and arcs (their centers) and \
                 lines (their midpoints)",
                entity.kind.type_name()
            )
        })?;
        if let Some((other, _)) = seen.iter().find(|(other, p)| *other == *r || *p == points) {
            return Err(if *other == *r {
                format!("{what}: {r} is listed twice")
            } else {
                format!("{what}: {other} and {r} stand for the same point")
            });
        }
        seen.push((*r, points));
    }
    match (mode, direction) {
        (SpacingMode::Along, None) => {
            Err(format!("{what}: an along spacing needs a direction line"))
        }
        (SpacingMode::Along, Some(line)) => {
            match index.get(line).map(|e| &e.kind) {
                Some(EntityKind::Line { .. }) => {}
                Some(other) => {
                    return Err(format!(
                        "{what}: the direction c{} is a {}, not a line",
                        line.0,
                        other.type_name()
                    ));
                }
                None => return Err(format!("{what}: the direction c{} does not exist", line.0)),
            }
            if anchors.contains(&Ref::Curve(line)) {
                return Err(format!(
                    "{what}: c{} is both an anchor and the direction",
                    line.0
                ));
            }
            Ok(())
        }
        (_, Some(line)) => Err(format!(
            "{what}: only an along spacing has a direction (c{})",
            line.0
        )),
        (_, None) => Ok(()),
    }
}

/// Where an anchor is in solved geometry.
pub fn anchor_at(solved: &Solved, anchor: Ref) -> Option<P2> {
    match anchor {
        Ref::Point(p) => solved.points.get(&p).copied(),
        Ref::Curve(c) => match solved.curves.get(&c)? {
            Curve2::Line { a, b } => Some([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]),
            Curve2::Circle { center, .. } | Curve2::Arc { center, .. } => Some(*center),
            _ => None,
        },
    }
}

/// The unit direction a directional spacing measures along.
pub fn direction_of(
    solved: &Solved,
    mode: SpacingMode,
    direction: Option<EntityUid>,
) -> Option<P2> {
    match mode {
        SpacingMode::Horizontal => Some([1.0, 0.0]),
        SpacingMode::Vertical => Some([0.0, 1.0]),
        SpacingMode::Along => match solved.curves.get(&direction?)? {
            Curve2::Line { a, b } if norm(sub(*b, *a)) > 0.0 => Some(unit(sub(*b, *a))),
            _ => None,
        },
        SpacingMode::PointToPoint => None,
    }
}

/// The successive intervals of an equal spacing in solved geometry: signed
/// offsets along the direction, or distances.
pub fn intervals(
    solved: &Solved,
    anchors: &[Ref],
    mode: SpacingMode,
    direction: Option<EntityUid>,
) -> Option<Vec<f64>> {
    let at: Vec<P2> = anchors
        .iter()
        .map(|r| anchor_at(solved, *r))
        .collect::<Option<_>>()?;
    let along = direction_of(solved, mode, direction);
    Some(
        at.windows(2)
            .map(|w| {
                let d = sub(w[1], w[0]);
                match along {
                    Some(u) => dot(d, u),
                    None => norm(d),
                }
            })
            .collect(),
    )
}

/// The anchors sorted along the direction; `Err` names two anchors at the
/// same place along it (within a millionth of the row's length, at least
/// 1e-6 mm).
pub fn sorted_along(at: &[(Ref, f64)]) -> Result<Vec<Ref>, (Ref, Ref)> {
    let mut sorted: Vec<(Ref, f64)> = at.to_vec();
    sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (low, high) = (
        sorted.first().map_or(0.0, |s| s.1),
        sorted.last().map_or(0.0, |s| s.1),
    );
    let tie = 1e-6 * (high - low).max(1.0);
    if let Some(w) = sorted.windows(2).find(|w| w[1].1 - w[0].1 <= tie) {
        // Name them in the order they were given.
        let (a, b) = (w[0].0, w[1].0);
        let first = at.iter().position(|x| x.0 == a) < at.iter().position(|x| x.0 == b);
        return Err(if first { (a, b) } else { (b, a) });
    }
    Ok(sorted.into_iter().map(|s| s.0).collect())
}

/// The patterns an entity is an original or a copy of.
fn instance_of(def: &SketchDef, anchor: Ref) -> Vec<ConstraintUid> {
    def.patterns
        .iter()
        .filter(|p| {
            p.entities.contains(&anchor)
                || p.copies.iter().any(|c| {
                    c.entities.contains_key(&anchor) || c.entities.values().any(|m| *m == anchor)
                })
        })
        .map(|p| p.id)
        .collect()
}

/// The pattern all the anchors are instances of, if there is one: its
/// spacing already keeps them equally spaced.
pub fn governing_pattern(def: &SketchDef, anchors: &[Ref]) -> Option<ConstraintUid> {
    let first = instance_of(def, *anchors.first()?);
    first
        .into_iter()
        .find(|p| anchors.iter().all(|r| instance_of(def, *r).contains(p)))
}

fn list(items: &[Ref]) -> String {
    let texts: Vec<String> = items.iter().map(ToString::to_string).collect();
    names(&texts)
}

/// "a, b and c".
fn names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

impl SketchEdit<'_> {
    /// The constraint an input makes: checked, its anchors in the order it
    /// keeps (sorted along a directional spacing's direction unless the
    /// order is kept), and not one a pattern already governs.
    fn spacing_kind(&self, input: &SpacingInput) -> Result<ConstraintKind, String> {
        let index = EntityIndex::new(&self.def.entities);
        let direction = match input.mode {
            SpacingMode::Along => input.direction,
            _ => None,
        };
        check(
            &index,
            "equal spacing",
            &input.anchors,
            input.mode,
            direction,
        )?;
        let anchors = if input.mode.directional() && !input.keep_order {
            let solved = self.solved()?;
            let u = direction_of(&solved, input.mode, direction)
                .ok_or("equal spacing: the direction line has no length")?;
            let at = input
                .anchors
                .iter()
                .map(|r| {
                    anchor_at(&solved, *r)
                        .map(|p| (*r, dot(p, u)))
                        .ok_or_else(|| {
                            crate::said(
                                "equal spacing: an item is not where it can be measured",
                                format!("equal spacing: {r} is not where it can be measured"),
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            sorted_along(&at).map_err(|(a, b)| {
                // Whole messages, each one template (mitcad#1626).
                let (along, said) = match input.mode {
                    SpacingMode::Horizontal => (
                        "x axis",
                        "equal spacing: two items are at the same place along the x axis; give \
                         their order (keep the listed order) or measure another way",
                    ),
                    SpacingMode::Vertical => (
                        "y axis",
                        "equal spacing: two items are at the same place along the y axis; give \
                         their order (keep the listed order) or measure another way",
                    ),
                    _ => (
                        "direction",
                        "equal spacing: two items are at the same place along the direction; \
                         give their order (keep the listed order) or measure another way",
                    ),
                };
                crate::said(
                    said,
                    format!(
                        "equal spacing: {a} and {b} are at the same place along the {along}; give \
                         their order (keep the listed order) or measure another way"
                    ),
                )
            })?
        } else {
            input.anchors.clone()
        };
        if let Some(pattern) = governing_pattern(&self.def, &anchors) {
            return Err(crate::said(
                "equal spacing: these are instances of a pattern, whose spacing already keeps \
                 them equally spaced; change it with Edit Pattern",
                format!(
                    "equal spacing: {} are instances of pattern {pattern}, whose spacing already \
                     keeps them equally spaced; change it with Edit Pattern",
                    list(&anchors)
                ),
            ));
        }
        Ok(ConstraintKind::EqualSpacing {
            anchors,
            mode: input.mode,
            direction,
        })
    }

    /// The mean of the current intervals of an equal spacing (its common
    /// spacing once it holds).
    fn mean_interval(&self, kind: &ConstraintKind) -> Result<f64, String> {
        let ConstraintKind::EqualSpacing {
            anchors,
            mode,
            direction,
        } = kind
        else {
            return Err("not an equal spacing".to_owned());
        };
        let solved = self.solved()?;
        let v = intervals(&solved, anchors, *mode, *direction)
            .ok_or("equal spacing: the intervals cannot be measured")?;
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        Ok(mean.abs())
    }

    /// Adds an equal spacing and, as asked, its common spacing dimension
    /// (one edit, so one undo step). Returns the constraint and the
    /// dimension.
    pub fn add_equal_spacing(
        &mut self,
        input: &SpacingInput,
        dimension: &SpacingDimension,
    ) -> Result<(ConstraintUid, Option<ConstraintUid>), String> {
        let kind = self.spacing_kind(input)?;
        let measured = match dimension {
            SpacingDimension::Driving(None) => Some(self.mean_interval(&kind)?),
            _ => None,
        };
        let id = self.add_constraint(kind);
        let spacing = DimensionKind::Spacing { constraint: id };
        let dim = match dimension {
            SpacingDimension::None => None,
            SpacingDimension::Driven => Some(self.add_dimension(spacing, None, true, None)?),
            SpacingDimension::Driving(value) => {
                let value = match (value, measured) {
                    (Some(v), _) => v.clone(),
                    (None, Some(m)) => ValueInput::Number(m),
                    (None, None) => unreachable!("measured above"),
                };
                Some(self.add_dimension(spacing, Some(&value), false, Some("spacing"))?)
            }
        };
        Ok((id, dim))
    }

    /// Changes an equal spacing's anchors, order, mode or direction; what
    /// is not given stays. A directional spacing is sorted along its
    /// direction again unless the order is kept. Its dimension stays.
    pub fn edit_equal_spacing(
        &mut self,
        id: ConstraintUid,
        anchors: Option<Vec<Ref>>,
        mode: Option<SpacingMode>,
        direction: Option<EntityUid>,
        keep_order: bool,
    ) -> Result<(), String> {
        let i = self
            .def
            .constraints
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| format!("constraint {id} does not exist"))?;
        let ConstraintKind::EqualSpacing {
            anchors: old_anchors,
            mode: old_mode,
            direction: old_direction,
        } = &self.def.constraints[i].kind
        else {
            return Err(format!("{id} is not an equal spacing"));
        };
        let mode = mode.unwrap_or(*old_mode);
        let input = SpacingInput {
            anchors: anchors.unwrap_or_else(|| old_anchors.clone()),
            mode,
            direction: match mode {
                SpacingMode::Along => direction.or(*old_direction),
                _ => None,
            },
            keep_order,
        };
        let kind = self.spacing_kind(&input)?;
        self.def.constraints[i].kind = kind;
        Ok(())
    }

    /// Refuses a new or edited equal spacing (or its dimension) that the
    /// other constraints already imply, naming them: the redundant count
    /// was `before` without it. A contradicting one fails the solve.
    pub fn require_spacing_needed(&self, before: usize, id: ConstraintUid) -> Result<(), String> {
        let status = self.solved()?.status;
        if status.redundant.len() <= before {
            return Ok(());
        }
        let mine =
            |item: &SolverItem| match item {
                SolverItem::Constraint(k) => {
                    *k == id
                        || self.def.dimensions.iter().any(|d| {
                            d.id == *k && d.kind == DimensionKind::Spacing { constraint: id }
                        })
                }
                SolverItem::Fixed(_) => false,
            };
        let mut others: Vec<String> = Vec::new();
        for set in status
            .redundant_sets
            .iter()
            .filter(|set| set.iter().any(mine))
        {
            for item in set.iter().filter(|item| !mine(item)) {
                let name = item.to_string();
                if !others.contains(&name) {
                    others.push(name);
                }
            }
        }
        Err(if others.is_empty() {
            crate::said(
                "the equal spacing would over-constrain the sketch: the others already fix what \
                 it constrains",
                format!(
                    "{id} would over-constrain the sketch: the others already fix what it \
                     constrains"
                ),
            )
        } else {
            crate::said(
                "the equal spacing would over-constrain the sketch: it follows from other \
                 constraints, which already fix what it constrains",
                format!(
                    "{id} would over-constrain the sketch: it follows from {} (they already fix \
                     what it constrains)",
                    names(&others)
                ),
            )
        })
    }

    /// Equal spacings lose the anchors of removed entities; a chain with
    /// fewer than three left, or whose direction line is removed, is listed
    /// to be removed (with its dimension).
    pub(crate) fn spacings_without(
        &mut self,
        removed: &std::collections::BTreeSet<EntityUid>,
    ) -> Vec<ConstraintUid> {
        let mut gone = Vec::new();
        for c in &mut self.def.constraints {
            if let ConstraintKind::EqualSpacing {
                anchors, direction, ..
            } = &mut c.kind
            {
                anchors.retain(|r| !removed.contains(&r.uid()));
                if anchors.len() < 3 || direction.is_some_and(|d| removed.contains(&d)) {
                    gone.push(c.id);
                }
            }
        }
        gone
    }
}
