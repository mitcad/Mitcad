// SPDX-License-Identifier: MIT
//! The sketch's own axes and origin as entities (mitcad#500), made on
//! demand when a constraint or a dimension refers to them.
//!
//! - The origin is a fixed point at [0, 0] (the one drawing snaps to,
//!   mitcad#355): the first such point that is not projected.
//! - An axis is a fixed construction line from the origin point along the
//!   axis, [`AXIS_LENGTH`] long, with its far end a fixed point; both carry
//!   the entity flag `axis` (`"x"` or `"y"`). Constraints and dimensions
//!   take the line as the infinite axis (a point on it, a distance from it,
//!   an angle to it). Being construction geometry it bounds no profile.
//! - One of each per sketch: asking again gives the one there is.

use super::edit::SketchEdit;
use super::{Entity, EntityKind, SketchAxis};
use crate::def::SketchDef;
use crate::ids::EntityUid;

/// How long an axis line is (mm). Only its direction counts: constraints
/// and dimensions take the infinite line, and the app draws the axis
/// across the view.
pub const AXIS_LENGTH: f64 = 100.0;

/// What a `sketch.axis` command asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisPick {
    Origin,
    Axis(SketchAxis),
}

impl AxisPick {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "origin" => Ok(Self::Origin),
            "x" => Ok(Self::Axis(SketchAxis::X)),
            "y" => Ok(Self::Axis(SketchAxis::Y)),
            other => Err(format!("unknown axis “{other}” (x, y or origin)")),
        }
    }
}

/// The sketch's origin point: a fixed point stored at [0, 0], not
/// projected and not an axis's end.
pub fn origin_point(def: &SketchDef) -> Option<EntityUid> {
    def.entities.iter().find_map(|e| match e.kind {
        EntityKind::Point { at }
            if e.fixed
                && !e.reference
                && e.axis.is_none()
                && at[0].abs() < 1e-9
                && at[1].abs() < 1e-9 =>
        {
            Some(e.id)
        }
        _ => None,
    })
}

/// The line that stands for an axis, when the sketch has it.
pub fn axis_line(def: &SketchDef, axis: SketchAxis) -> Option<EntityUid> {
    def.entities
        .iter()
        .find(|e| e.axis == Some(axis) && matches!(e.kind, EntityKind::Line { .. }))
        .map(|e| e.id)
}

/// The entity that stands for `pick` when the sketch has it.
pub fn existing(def: &SketchDef, pick: AxisPick) -> Option<EntityUid> {
    match pick {
        AxisPick::Origin => origin_point(def),
        AxisPick::Axis(axis) => axis_line(def, axis),
    }
}

impl SketchEdit<'_> {
    /// The origin point, made when the sketch has none.
    pub fn origin(&mut self) -> EntityUid {
        if let Some(id) = origin_point(&self.def) {
            return id;
        }
        let id = self.add_point([0.0, 0.0]);
        self.def.entity_mut(id).expect("added").fixed = true;
        id
    }

    /// The line that stands for an axis, made (with the origin point) when
    /// the sketch has none.
    pub fn axis(&mut self, axis: SketchAxis) -> EntityUid {
        if let Some(id) = axis_line(&self.def, axis) {
            return id;
        }
        let origin = self.origin();
        let [dx, dy] = axis.direction();
        let line = self.id();
        let end = self.id();
        let mut far = Entity::point(end, [dx * AXIS_LENGTH, dy * AXIS_LENGTH]);
        far.fixed = true;
        far.axis = Some(axis);
        let mut entity = Entity::new(
            line,
            EntityKind::Line {
                start: origin,
                end,
                centerline: false,
            },
        );
        entity.construction = true;
        entity.fixed = true;
        entity.axis = Some(axis);
        self.push(far);
        self.push(entity)
    }

    /// The entity that stands for `pick`, made when the sketch has none.
    pub fn axis_pick(&mut self, pick: AxisPick) -> EntityUid {
        match pick {
            AxisPick::Origin => self.origin(),
            AxisPick::Axis(axis) => self.axis(axis),
        }
    }
}
