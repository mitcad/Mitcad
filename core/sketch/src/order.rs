// SPDX-License-Identifier: MIT
//! The order in which a sketch's points, curves, constraints and dimensions
//! were made (mitcad#1492): sketch mode lists them so in the timeline's
//! place.
//!
//! The sketch keeps no order of its own across its two id spaces, but each
//! space gives out its numbers in the order things are made (one more than
//! the largest number in use, `SketchDef::next_entity` and
//! `next_constraint`), so the points and curves are in creation order by
//! their numbers, and the constraints and dimensions by theirs. The two are
//! merged by what a constraint or dimension refers to: it was made after
//! the points and curves it names, and is placed right after the newest of
//! them (and after the constraints and dimensions made before it), ahead of
//! the points and curves made later. A curve comes with its points: those
//! numbered after it (a tool that numbers its curves first, as the
//! rectangle does) follow it at once, so that a constraint on the curve
//! does not come before the curve's own points.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::def::SketchDef;
use crate::ids::EntityUid;
use crate::{ConstraintUid, Entity, Ref};

/// An item of a sketch in [`SketchDef::creation_order`]: a point or a curve,
/// a constraint or a dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderItem {
    Entity(Ref),
    Constraint(ConstraintUid),
    Dimension(ConstraintUid),
}

impl fmt::Display for OrderItem {
    /// The item's id: `p3`, `c4`, `k5`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entity(entity) => write!(f, "{entity}"),
            Self::Constraint(id) | Self::Dimension(id) => write!(f, "{id}"),
        }
    }
}

/// The items listed so far, and which entities they hold.
struct Listing<'a> {
    by_id: HashMap<EntityUid, &'a Entity>,
    listed: HashSet<EntityUid>,
    order: Vec<OrderItem>,
}

impl Listing<'_> {
    /// Lists an entity unless it is listed already; a curve with its points
    /// that are not listed yet.
    fn entity(&mut self, entity: &Entity) {
        if !self.listed.insert(entity.id) {
            return;
        }
        self.order.push(OrderItem::Entity(entity.as_ref()));
        for point in entity.kind.points() {
            if let Some(p) = self.by_id.get(&point).copied()
                && self.listed.insert(point)
            {
                self.order.push(OrderItem::Entity(p.as_ref()));
            }
        }
    }

    /// Whether an entity the sketch has is not listed yet.
    fn waits_for(&self, id: EntityUid) -> bool {
        self.by_id.contains_key(&id) && !self.listed.contains(&id)
    }
}

impl<P> SketchDef<P> {
    /// The points, curves, constraints and dimensions in the order they were
    /// made (see the module's description). Texts, patterns, offsets and
    /// projections are not listed.
    pub fn creation_order(&self) -> Vec<OrderItem> {
        let mut entities: Vec<&Entity> = self.entities.iter().collect();
        entities.sort_by_key(|e| e.id);
        let mut later: Vec<(ConstraintUid, OrderItem, Vec<Ref>)> = self
            .constraints
            .iter()
            .map(|c| (c.id, OrderItem::Constraint(c.id), c.kind.refs()))
            .chain(
                self.dimensions
                    .iter()
                    .map(|d| (d.id, OrderItem::Dimension(d.id), d.kind.refs())),
            )
            .collect();
        later.sort_by_key(|(id, _, _)| *id);
        let mut listing = Listing {
            by_id: entities.iter().map(|e| (e.id, *e)).collect(),
            listed: HashSet::new(),
            order: Vec::with_capacity(entities.len() + later.len()),
        };
        let mut next = 0;
        for (_, item, refs) in later {
            // Up to the newest entity it names.
            while next < entities.len() && refs.iter().any(|r| listing.waits_for(r.uid())) {
                listing.entity(entities[next]);
                next += 1;
            }
            listing.order.push(item);
        }
        for entity in &entities[next..] {
            listing.entity(entity);
        }
        listing.order
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Constraint, ConstraintKind, Dimension, DimensionKind, EntityKind};

    fn line(id: u32, start: u32, end: u32) -> Entity {
        Entity::new(
            EntityUid(id),
            EntityKind::Line {
                start: EntityUid(start),
                end: EntityUid(end),
                centerline: false,
            },
        )
    }

    fn names(def: &SketchDef<()>) -> Vec<String> {
        def.creation_order()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn constraints_follow_what_they_refer_to() {
        let mut def = SketchDef::<()>::default();
        // A line, its horizontal constraint and length, then a second line
        // made later with its own vertical constraint.
        def.entities.push(Entity::point(EntityUid(1), [0.0, 0.0]));
        def.entities.push(Entity::point(EntityUid(2), [10.0, 0.0]));
        def.entities.push(line(3, 1, 2));
        def.entities.push(Entity::point(EntityUid(4), [10.0, 10.0]));
        def.entities.push(line(5, 2, 4));
        def.constraints.push(Constraint::new(
            ConstraintUid(1),
            ConstraintKind::Horizontal { line: EntityUid(3) },
        ));
        def.constraints.push(Constraint::new(
            ConstraintUid(3),
            ConstraintKind::Vertical { line: EntityUid(5) },
        ));
        def.dimensions.push(Dimension::new(
            ConstraintUid(2),
            DimensionKind::Length { line: EntityUid(3) },
            Some(()),
        ));
        assert_eq!(
            names(&def),
            ["p1", "p2", "c3", "k1", "k2", "p4", "c5", "k3"]
        );
    }

    #[test]
    fn numbers_not_the_lists_give_the_order() {
        let mut def = SketchDef::<()>::default();
        def.entities.push(Entity::point(EntityUid(7), [0.0, 0.0]));
        def.entities.push(Entity::point(EntityUid(2), [1.0, 0.0]));
        def.constraints.push(Constraint::new(
            ConstraintUid(4),
            ConstraintKind::HorizontalPoints {
                a: EntityUid(2),
                b: EntityUid(7),
            },
        ));
        def.dimensions.push(Dimension::new(
            ConstraintUid(1),
            DimensionKind::Distance {
                a: EntityUid(2),
                b: EntityUid(2),
            },
            None,
        ));
        // k1 names only p2: made before p7; k4 names both.
        assert_eq!(names(&def), ["p2", "k1", "p7", "k4"]);
    }

    #[test]
    fn a_curve_comes_with_its_points() {
        // As the rectangle numbers them: its lines first, then its corners.
        let mut def = SketchDef::<()>::default();
        def.entities.push(line(1, 3, 4));
        def.entities.push(line(2, 4, 5));
        for (id, x) in [(3, 0.0), (4, 10.0), (5, 20.0)] {
            def.entities.push(Entity::point(EntityUid(id), [x, 0.0]));
        }
        def.constraints.push(Constraint::new(
            ConstraintUid(1),
            ConstraintKind::Horizontal { line: EntityUid(1) },
        ));
        def.constraints.push(Constraint::new(
            ConstraintUid(2),
            ConstraintKind::Horizontal { line: EntityUid(2) },
        ));
        def.dimensions.push(Dimension::new(
            ConstraintUid(3),
            DimensionKind::Length { line: EntityUid(1) },
            Some(()),
        ));
        assert_eq!(
            names(&def),
            ["c1", "p3", "p4", "k1", "c2", "p5", "k2", "k3"]
        );
        // Without constraints: each curve with its points all the same.
        def.constraints.clear();
        def.dimensions.clear();
        assert_eq!(names(&def), ["c1", "p3", "p4", "c2", "p5"]);
    }
}
