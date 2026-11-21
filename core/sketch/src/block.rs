// SPDX-License-Identifier: MIT
//! Sketch blocks (mitcad#2110): sketch geometry saved as a named block of
//! the design, and instances of it in sketches that follow the block.
//!
//! - A block ([`SketchBlock`]) keeps its points, curves, constraints and
//!   dimensions as a [`Fragment`] in its own frame: the base point at the
//!   origin, the base direction along +x. A block may hold instances of
//!   other blocks ([`SketchBlock::instances`], in the content's ids). The
//!   design keeps its blocks (the model's `DocState::sketch_blocks`).
//! - An instance ([`BlockInstance`], in a sketch's `blocks`) is the block's
//!   geometry as ordinary sketch entities, with a base point and a base
//!   direction of its own: a construction line from the base point to its
//!   tip. It keeps a copy of the block's entities ([`BlockInstance::shape`]),
//!   so the sketch solves on its own: every solve binds the instance's
//!   points rigidly to its base point and direction ([`bindings`]), so it
//!   moves and turns as one, with three degrees of freedom, and the block's
//!   own constraints and dimensions are not in the sketch. Constraints and
//!   dimensions of the sketch place it; one between its own points and
//!   curves is refused ([`SketchEdit::check_blocks`]): explode it first.
//! - [`SketchEdit::make_block`] turns a selection into a block and its
//!   instance in place; [`SketchEdit::insert_block`] adds an instance;
//!   [`SketchEdit::explode_block`] turns one into plain geometry with the
//!   block's constraints and dimensions; [`SketchEdit::open_block`] and
//!   [`SketchEdit::close_block`] edit a block in place (its geometry free,
//!   its constraints and dimensions in the sketch while it is open);
//!   [`SketchEdit::sync_block`] remakes the instances of an edited block
//!   where each is, and keeps what a constraint on removed geometry lost
//!   ([`BlockInstance::lost`]).
//! - A block of a turned instance opened or exploded has the block's
//!   horizontal and vertical lines parallel and perpendicular to the
//!   instance's base direction; its horizontal and vertical point
//!   alignments and distances have no direction there and are left out.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

use super::cleanup::remap_dimension;
use super::clipboard::{FRAGMENT_VERSION, Fragment, FragmentValue, mapped, value_of};
use super::edit::SketchEdit;
use super::geometry::{P2, add, norm, sub};
use super::modify::{Item, remap};
use super::pattern::{Binding, BindingKind, kept_ends};
use super::{
    Constraint, ConstraintKind, ConstraintUid, Dimension, DimensionKind, Entity, EntityIndex,
    EntityKind, Ref, axes, point_serde,
};
use crate::def::SketchDef;
use crate::ids::EntityUid;
use mitcad_model_base::internal::ids::curve_serde;

/// Lengths and angles closer than this are the same.
const TINY: f64 = 1e-9;

/// A block of a design: its name and its geometry in its own frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchBlock {
    /// Unique in the design; instances name their block by it.
    pub id: u32,
    pub name: String,
    /// The block's points, curves, constraints and dimensions, base point
    /// at the origin and base direction along +x (`base` is the origin;
    /// no references, no texts).
    pub content: Fragment,
    /// Instances of other blocks in it, in the content's ids.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<BlockInstance>,
}

impl SketchBlock {
    /// The blocks it holds instances of, directly.
    pub fn holds(&self) -> BTreeSet<u32> {
        self.instances.iter().map(|i| i.block).collect()
    }
}

/// Whether block `outer` holds `inner`, directly or through other blocks,
/// or is it.
pub fn holds(blocks: &[SketchBlock], outer: u32, inner: u32) -> bool {
    let mut seen = BTreeSet::new();
    let mut todo = vec![outer];
    while let Some(b) = todo.pop() {
        if b == inner {
            return true;
        }
        if !seen.insert(b) {
            continue;
        }
        if let Some(block) = blocks.iter().find(|x| x.id == b) {
            todo.extend(block.holds());
        }
    }
    false
}

/// The consistency of a design's blocks: ids and names unique, their
/// nested instances of blocks there are, none holding itself.
pub fn check_blocks(blocks: &[SketchBlock]) -> Result<(), String> {
    for (i, b) in blocks.iter().enumerate() {
        if b.name.trim().is_empty() {
            return Err(format!("block {} has no name", b.id));
        }
        if blocks[..i].iter().any(|o| o.id == b.id) {
            return Err(format!("block {} is listed more than once", b.id));
        }
        if blocks[..i].iter().any(|o| o.name == b.name) {
            return Err(format!("two blocks are named {}", b.name));
        }
        for n in &b.instances {
            if !blocks.iter().any(|o| o.id == n.block) {
                return Err(format!(
                    "block {} holds block {}, which is not there",
                    b.name, n.block
                ));
            }
        }
    }
    for b in blocks {
        if b.holds().iter().any(|n| holds(blocks, *n, b.id)) {
            return Err(format!("block {} holds itself", b.name));
        }
    }
    Ok(())
}

/// An instance of a block opened for an edit in place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenEdit {
    /// The entities numbered from this on were made during the edit: they
    /// go into the block when it closes.
    pub since: u32,
    /// The sketch was made for the edit (Edit Block from the Browser): the
    /// model removes it when the edit ends.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub temporary: bool,
}

/// A constraint or dimension of the sketch on an instance's geometry that
/// an edit of the block removed: what it was and where.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LostItem {
    /// The constraint's or dimension's type (`coincident`, `distance`).
    pub what: String,
    /// Where its geometry was, sketch coordinates.
    pub at: P2,
}

/// An instance of a block in a sketch (`k<n>`: constraints, dimensions,
/// patterns, offsets and instances share the numbers), or in a block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockInstance {
    pub id: ConstraintUid,
    pub block: u32,
    /// Its base point: where the block's origin is.
    #[serde(with = "point_serde")]
    pub base: EntityUid,
    /// The construction line from the base point along its base direction.
    #[serde(with = "curve_serde")]
    pub direction: EntityUid,
    /// The block's points and curves (its content's ids) to the sketch's.
    pub entities: BTreeMap<Ref, Ref>,
    /// The block's entities as the instance was last made from it.
    pub shape: Vec<Entity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<OpenEdit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lost: Vec<LostItem>,
}

impl BlockInstance {
    /// The direction line's tip point.
    pub fn tip(&self, index: &EntityIndex<'_>) -> Option<EntityUid> {
        match index.get(self.direction)?.kind {
            EntityKind::Line { start, end, .. } if start == self.base => Some(end),
            _ => None,
        }
    }

    /// Where its base point is, its turn and its direction line's length.
    pub fn pose(&self, index: &EntityIndex<'_>) -> Option<(P2, f64, f64)> {
        let base = index.point(self.base)?;
        let tip = index.point(self.tip(index)?)?;
        let d = sub(tip, base);
        Some((base, d[1].atan2(d[0]), norm(d)))
    }

    /// Its points and curves in the sketch: the block's, the base point,
    /// the direction line and its tip.
    pub fn members(&self, index: &EntityIndex<'_>) -> BTreeSet<EntityUid> {
        let mut out: BTreeSet<EntityUid> = self.entities.values().map(|r| r.uid()).collect();
        out.insert(self.base);
        out.insert(self.direction);
        out.extend(self.tip(index));
        out
    }

    fn sketch_of(&self, block_id: EntityUid) -> Option<EntityUid> {
        self.entities
            .get(&Ref::Point(block_id))
            .or_else(|| self.entities.get(&Ref::Curve(block_id)))
            .map(|r| r.uid())
    }
}

/// `p` turned by `angle` about the origin.
fn turn(p: P2, angle: f64) -> P2 {
    let (s, c) = angle.sin_cos();
    [c * p[0] - s * p[1], s * p[0] + c * p[1]]
}

/// A block point placed at `at`, turned by `angle`.
fn place(q: P2, at: P2, angle: f64) -> P2 {
    add(at, turn(q, angle))
}

/// A sketch point in the frame of an instance at `at` turned by `angle`.
fn unplace(p: P2, at: P2, angle: f64) -> P2 {
    turn(sub(p, at), -angle)
}

/// Whether an angle is a whole number of turns.
fn unturned(angle: f64) -> bool {
    let a = angle.rem_euclid(TAU);
    a < 1e-9 || TAU - a < 1e-9
}

/// The length of an instance's direction line: half the block's size, at
/// least 1 mm.
fn tip_length(content: &Fragment) -> f64 {
    let far = content
        .entities
        .iter()
        .filter_map(|e| match e.kind {
            EntityKind::Point { at } => Some(norm(at)),
            _ => None,
        })
        .fold(0.0, f64::max);
    if far > TINY {
        (far / 2.0).max(1.0)
    } else {
        10.0
    }
}

/// A block entity's kind placed at `at` turned by `angle`, its points
/// renamed by `map`.
fn placed_kind(
    kind: &EntityKind,
    map: &BTreeMap<EntityUid, EntityUid>,
    at: P2,
    angle: f64,
) -> EntityKind {
    match kind {
        EntityKind::Point { at: q } => EntityKind::Point {
            at: place(*q, at, angle),
        },
        EntityKind::Circle {
            center,
            radius,
            start,
        } => EntityKind::Circle {
            center: map.get(center).copied().unwrap_or(*center),
            radius: *radius,
            start: *start + angle,
        },
        kind => mapped(kind, map),
    }
}

/// The solver bindings of a closed instance: its direction line keeps its
/// length; every point of the block is held at its distance and direction
/// from the base point (by its direction from its curve's centre for an end
/// its arc keeps on its circle, as a pattern's copies are), and circles and
/// ellipses keep their sizes. So the instance has the three degrees of
/// freedom of its base point and turn.
pub fn bindings(instance: &BlockInstance, index: &EntityIndex<'_>) -> Vec<Binding> {
    let mut out = Vec::new();
    if instance.open.is_some() {
        return out;
    }
    let (Some(tip), Some((_, _, length))) = (instance.tip(index), instance.pose(index)) else {
        return out;
    };
    let bind = |kind| Binding {
        owner: instance.id,
        kind,
    };
    let base = instance.base;
    out.push(bind(BindingKind::Distance {
        a: base,
        b: tip,
        value: length,
    }));
    let shape: Vec<&Entity> = instance.shape.iter().collect();
    let kept = kept_ends(&shape);
    let local: BTreeMap<EntityUid, P2> = instance
        .shape
        .iter()
        .filter_map(|e| match e.kind {
            EntityKind::Point { at } => Some((e.id, at)),
            _ => None,
        })
        .collect();
    let map = |u: EntityUid| instance.sketch_of(u).filter(|s| index.get(*s).is_some());
    let direction = |v: P2| v[1].atan2(v[0]);
    for e in &instance.shape {
        let Some(s) = map(e.id) else {
            continue;
        };
        match e.kind {
            EntityKind::Point { at: q } => {
                if s == base {
                    continue;
                }
                match kept.get(&e.id).map(Vec::as_slice) {
                    Some([c]) => {
                        if let (Some(sc), Some(qc)) = (map(*c), local.get(c)) {
                            out.push(bind(BindingKind::TurnedDirection {
                                a_center: base,
                                a: tip,
                                b_center: sc,
                                b: s,
                                angle: direction(sub(q, *qc)),
                            }));
                        }
                    }
                    // On two curves: their intersection fixes it.
                    Some([_, _, ..]) => {}
                    _ => {
                        let r = norm(q);
                        if r < TINY {
                            out.push(bind(BindingKind::Translated {
                                a: base,
                                b: s,
                                by: [0.0, 0.0],
                            }));
                        } else {
                            out.push(bind(BindingKind::Distance {
                                a: base,
                                b: s,
                                value: r,
                            }));
                            out.push(bind(BindingKind::TurnedDirection {
                                a_center: base,
                                a: tip,
                                b_center: base,
                                b: s,
                                angle: direction(q),
                            }));
                        }
                    }
                }
            }
            EntityKind::Circle { radius, .. } => out.push(bind(BindingKind::Radius {
                curve: s,
                value: radius,
            })),
            EntityKind::Ellipse { minor_radius, .. }
            | EntityKind::EllipticalArc { minor_radius, .. } => {
                out.push(bind(BindingKind::MinorRadius {
                    curve: s,
                    value: minor_radius,
                }))
            }
            _ => {}
        }
    }
    out
}

/// What [`remake`] does to a container's entities.
struct Remade {
    /// Existing entities with their new kinds.
    changed: Vec<(EntityUid, EntityKind, bool)>,
    /// New entities.
    made: Vec<Entity>,
    /// The instance's entities the block no longer has.
    gone: BTreeSet<EntityUid>,
}

/// An instance made again from `block` where it is: the entities it keeps
/// (by the block's ids) get their new shapes, new ones new ids from
/// `next`, and those the block lost are listed. The instance's map and
/// shape are updated.
fn remake(
    entities: &[Entity],
    instance: &mut BlockInstance,
    block: &SketchBlock,
    next: &mut dyn FnMut() -> EntityUid,
) -> Result<Remade, String> {
    let index = EntityIndex::new(entities);
    let (at, angle, _) = instance
        .pose(&index)
        .ok_or("a block instance lacks its base point or direction")?;
    let mut map: BTreeMap<EntityUid, EntityUid> = BTreeMap::new();
    let mut fresh: BTreeSet<EntityUid> = BTreeSet::new();
    for e in &block.content.entities {
        let kept = instance
            .entities
            .get(&e.as_ref())
            .map(|r| r.uid())
            .filter(|s| index.get(*s).is_some_and(|x| x.is_point() == e.is_point()));
        let id = match kept {
            Some(s) => s,
            None => {
                let s = next();
                fresh.insert(s);
                s
            }
        };
        map.insert(e.id, id);
    }
    let mut out = Remade {
        changed: Vec::new(),
        made: Vec::new(),
        gone: BTreeSet::new(),
    };
    for e in &block.content.entities {
        let id = map[&e.id];
        let kind = placed_kind(&e.kind, &map, at, angle);
        if fresh.contains(&id) {
            let mut made = Entity::new(id, kind);
            made.construction = e.construction;
            out.made.push(made);
        } else {
            out.changed.push((id, kind, e.construction));
        }
    }
    let kept: BTreeSet<EntityUid> = map.values().copied().collect();
    out.gone = instance
        .entities
        .values()
        .map(|r| r.uid())
        .filter(|s| !kept.contains(s) && *s != instance.base)
        .collect();
    instance.entities = block
        .content
        .entities
        .iter()
        .map(|e| {
            let s = map[&e.id];
            (
                e.as_ref(),
                if e.is_point() {
                    Ref::Point(s)
                } else {
                    Ref::Curve(s)
                },
            )
        })
        .collect();
    instance.shape = block.content.entities.clone();
    Ok(out)
}

/// The nested instances of `edited` in block `outer`'s content made again:
/// whether anything changed. What the edit removed goes from the content
/// with the constraints and dimensions on it.
pub fn sync_nested(outer: &mut SketchBlock, edited: &SketchBlock) -> Result<bool, String> {
    let mut changed = false;
    let mut instances = std::mem::take(&mut outer.instances);
    let result = (|| -> Result<(), String> {
        for instance in instances.iter_mut().filter(|i| i.block == edited.id) {
            let mut top = outer
                .content
                .entities
                .iter()
                .map(|e| e.id.0)
                .max()
                .unwrap_or(0);
            let mut next = || {
                top += 1;
                EntityUid(top)
            };
            let remade = remake(&outer.content.entities, instance, edited, &mut next)?;
            let content = &mut outer.content;
            for (id, kind, construction) in remade.changed {
                if let Some(e) = content.entities.iter_mut().find(|e| e.id == id) {
                    e.kind = kind;
                    e.construction = construction;
                }
            }
            content.entities.extend(remade.made);
            let mut gone = remade.gone;
            loop {
                let before = gone.len();
                for e in &content.entities {
                    if e.kind.points().iter().any(|p| gone.contains(p)) {
                        gone.insert(e.id);
                    }
                }
                if gone.len() == before {
                    break;
                }
            }
            let on_gone = |refs: Vec<Ref>| refs.iter().any(|r| gone.contains(&r.uid()));
            content.entities.retain(|e| !gone.contains(&e.id));
            content.constraints.retain(|c| !on_gone(c.kind.refs()));
            content.dimensions.retain(|d| {
                matches!(d.kind, DimensionKind::Spacing { .. }) || !on_gone(d.kind.refs())
            });
            changed = true;
        }
        Ok(())
    })();
    outer.instances = instances;
    result.map(|()| changed)
}

/// What [`SketchEdit::close_block`] made of an instance edited in place.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosedBlock {
    /// The block's new content and the instances of other blocks in it.
    pub content: Fragment,
    pub instances: Vec<BlockInstance>,
    /// The sketch was made for the edit.
    pub temporary: bool,
}

/// Why a selection or an edit cannot go into a block, in words.
fn projected_refusal() -> String {
    "projected or intersected geometry cannot go into a block: convert it to sketch geometry \
     first"
        .to_owned()
}

impl SketchDef {
    /// The solver bindings of the block instances (mitcad#2110).
    pub fn block_bindings(&self) -> Vec<Binding> {
        if self.blocks.is_empty() {
            return Vec::new();
        }
        let index = EntityIndex::new(&self.entities);
        self.blocks
            .iter()
            .flat_map(|b| bindings(b, &index))
            .collect()
    }

    /// The closed instance whose points and curves include `id`.
    pub fn instance_of(&self, id: EntityUid) -> Option<&BlockInstance> {
        let index = EntityIndex::new(&self.entities);
        self.blocks
            .iter()
            .filter(|b| b.open.is_none())
            .find(|b| b.members(&index).contains(&id))
    }

    /// Whether an entity is projected or intersected geometry.
    fn is_projected(&self, id: EntityUid) -> bool {
        self.entity(id).is_some_and(|e| e.reference)
            || self.projections.iter().any(|p| {
                p.entities.contains(&Ref::Curve(id)) || p.entities.contains(&Ref::Point(id))
            })
    }
}

impl<P> SketchDef<P> {
    /// The consistency of the instances: their base points, directions and
    /// entities exist, each entity in one instance.
    pub(crate) fn check_instances(&self) -> Result<(), String> {
        let index = EntityIndex::new(&self.entities);
        let mut owner: BTreeMap<EntityUid, ConstraintUid> = BTreeMap::new();
        for b in &self.blocks {
            if index.point(b.base).is_none() {
                return Err(format!(
                    "block instance {}: its base point is missing",
                    b.id
                ));
            }
            if b.tip(&index).is_none() {
                return Err(format!(
                    "block instance {}: its direction is not a line from its base point",
                    b.id
                ));
            }
            for (from, to) in &b.entities {
                match index.get(to.uid()) {
                    Some(e)
                        if e.is_point() == to.is_point() && from.is_point() == to.is_point() => {}
                    _ => {
                        return Err(format!(
                            "block instance {}: {to} is not in the sketch",
                            b.id
                        ));
                    }
                }
            }
            // An open instance's geometry holds the instances nested in
            // its block: only closed ones own their entities.
            if b.open.is_some() {
                continue;
            }
            for m in b.members(&index) {
                if let Some(other) = owner.insert(m, b.id) {
                    return Err(format!(
                        "block instances {other} and {} share an entity",
                        b.id
                    ));
                }
            }
        }
        Ok(())
    }
}

impl SketchEdit<'_> {
    fn instance_index(&self, id: ConstraintUid) -> Result<usize, String> {
        self.def
            .blocks
            .iter()
            .position(|b| b.id == id)
            .ok_or_else(|| {
                super::said(
                    "the block instance is not in the sketch",
                    format!("block instance {id} does not exist"),
                )
            })
    }

    /// Refuses a constraint or dimension between a closed instance's own
    /// points and curves: the instance keeps its block's shape. Horizontal
    /// and vertical constraints and distances turn or place it, so they
    /// are not its own.
    pub(crate) fn check_blocks(&self) -> Result<(), String> {
        if self.def.blocks.is_empty() {
            return Ok(());
        }
        let index = EntityIndex::new(&self.def.entities);
        let refusal = |id: ConstraintUid| {
            super::said(
                "a block instance keeps the shape of its block: a constraint or dimension \
                 between its own points and curves cannot be added; explode it first (Explode \
                 Block) or edit the block (Edit Block)",
                format!("{id} is inside a block instance"),
            )
        };
        for instance in self.def.blocks.iter().filter(|b| b.open.is_none()) {
            let members = instance.members(&index);
            let inside =
                |refs: &[Ref]| !refs.is_empty() && refs.iter().all(|r| members.contains(&r.uid()));
            for c in self.def.constraints.iter().filter(|c| c.active) {
                let framed = matches!(
                    c.kind,
                    ConstraintKind::Horizontal { .. }
                        | ConstraintKind::Vertical { .. }
                        | ConstraintKind::HorizontalPoints { .. }
                        | ConstraintKind::VerticalPoints { .. }
                );
                if !framed && inside(&c.kind.refs()) {
                    return Err(refusal(c.id));
                }
            }
            for d in self.def.dimensions.iter().filter(|d| d.active) {
                let framed = matches!(
                    d.kind,
                    DimensionKind::HorizontalDistance { .. }
                        | DimensionKind::VerticalDistance { .. }
                );
                if !framed && inside(&d.kind.refs()) {
                    return Err(refusal(d.id));
                }
            }
        }
        Ok(())
    }

    /// Refuses a change of shape (trim, extend, fillet, chamfer) of a
    /// closed instance's curve.
    pub(crate) fn block_editable(&self, id: EntityUid) -> Result<(), String> {
        match self.def.instance_of(id) {
            Some(instance) => Err(super::said(
                "the curve is part of a block instance, which keeps the shape of its block: \
                 explode it first (Explode Block) or edit the block (Edit Block)",
                format!("c{} is part of block instance {}", id.0, instance.id),
            )),
            None => Ok(()),
        }
    }

    /// Whole closed instances for any of their entities in `entities`
    /// (a delete takes an instance whole); their records go.
    pub(crate) fn whole_instances(&mut self, entities: &mut BTreeSet<EntityUid>) {
        let index = EntityIndex::new(&self.def.entities);
        let mut gone = Vec::new();
        for b in self.def.blocks.iter().filter(|b| b.open.is_none()) {
            let members = b.members(&index);
            if members.iter().any(|m| entities.contains(m)) {
                entities.extend(members);
                gone.push(b.id);
            }
        }
        if !gone.is_empty() {
            for id in &gone {
                self.report.removed.push(id.to_string());
            }
            self.def.blocks.retain(|b| !gone.contains(&b.id));
        }
    }

    /// The instances forget removed entities; one whose base point or
    /// direction is gone leaves its geometry as plain sketch geometry.
    pub(crate) fn forget_in_blocks(&mut self, entities: &BTreeSet<EntityUid>) {
        if self.def.blocks.is_empty() {
            return;
        }
        for b in &mut self.def.blocks {
            b.entities.retain(|_, s| !entities.contains(&s.uid()));
        }
        let gone: Vec<ConstraintUid> = self
            .def
            .blocks
            .iter()
            .filter(|b| entities.contains(&b.base) || entities.contains(&b.direction))
            .map(|b| b.id)
            .collect();
        for id in &gone {
            self.report.removed.push(id.to_string());
        }
        self.def.blocks.retain(|b| !gone.contains(&b.id));
    }

    /// Adds an instance of `block` with its base point at `at`, turned by
    /// `angle` (radians, counter-clockwise).
    pub fn insert_block(
        &mut self,
        block: &SketchBlock,
        at: P2,
        angle: f64,
    ) -> Result<ConstraintUid, String> {
        if !(at.iter().all(|v| v.is_finite()) && angle.is_finite()) {
            return Err(format!("the place must be finite, got {at:?} at {angle}"));
        }
        if block.content.entities.is_empty() {
            return Err(super::said(
                "the block has no geometry",
                format!("block {} has no entities", block.id),
            ));
        }
        let ids = self.ids(block.content.entities.len());
        let map: BTreeMap<EntityUid, EntityUid> = block
            .content
            .entities
            .iter()
            .map(|e| e.id)
            .zip(ids.iter().copied())
            .collect();
        let mut entities = BTreeMap::new();
        for (e, id) in block.content.entities.iter().zip(ids) {
            let mut made = Entity::new(id, placed_kind(&e.kind, &map, at, angle));
            made.construction = e.construction;
            entities.insert(e.as_ref(), made.as_ref());
            self.push(made);
        }
        let base = self.add_point(at);
        let tip = self.add_point(place([tip_length(&block.content), 0.0], at, angle));
        let direction = self.add_curve(EntityKind::Line {
            start: base,
            end: tip,
            centerline: false,
        });
        if let Some(line) = self.def.entity_mut(direction) {
            line.construction = true;
        }
        let id = self.constraint_id();
        self.def.blocks.push(BlockInstance {
            id,
            block: block.id,
            base,
            direction,
            entities,
            shape: block.content.entities.clone(),
            open: None,
            lost: Vec::new(),
        });
        Ok(id)
    }

    /// The block's constraints and dimensions on instance `index`'s
    /// geometry and its nested instances as instances of this sketch:
    /// the start of an explode and of an edit in place. Returns how many it
    /// left out (a horizontal or vertical alignment or distance of a turned
    /// instance).
    fn unfold(&mut self, index: usize, block: &SketchBlock) -> Result<usize, String> {
        let instance = self.def.blocks[index].clone();
        let at_index = EntityIndex::new(&self.def.entities);
        let (at, angle, _) = instance
            .pose(&at_index)
            .ok_or("a block instance lacks its base point or direction")?;
        let turned = !unturned(angle);
        let map: BTreeMap<EntityUid, EntityUid> = instance
            .entities
            .iter()
            .map(|(b, s)| (b.uid(), s.uid()))
            .collect();
        let mapped_all = |refs: &[Ref]| refs.iter().all(|r| map.contains_key(&r.uid()));
        let mut left_out = 0;
        let mut constraints: BTreeMap<ConstraintUid, ConstraintUid> = BTreeMap::new();
        for c in &block.content.constraints {
            if !mapped_all(&c.kind.refs()) {
                left_out += 1;
                continue;
            }
            let mut kind = c.kind.clone();
            remap(&mut kind, &map);
            if turned {
                kind = match kind {
                    ConstraintKind::Horizontal { line } => ConstraintKind::Parallel {
                        a: line,
                        b: instance.direction,
                    },
                    ConstraintKind::Vertical { line } => ConstraintKind::Perpendicular {
                        a: line,
                        b: instance.direction,
                    },
                    ConstraintKind::HorizontalPoints { .. }
                    | ConstraintKind::VerticalPoints { .. } => {
                        left_out += 1;
                        continue;
                    }
                    kind => kind,
                };
            }
            let id = self.add_constraint(kind);
            if !c.active
                && let Some(made) = self.def.constraints.iter_mut().find(|k| k.id == id)
            {
                made.active = false;
                made.inactive_reason = c.inactive_reason.clone();
            }
            constraints.insert(c.id, id);
        }
        let mut values = Vec::new();
        for d in &block.content.dimensions {
            let mut kind = d.kind.clone();
            if let DimensionKind::Spacing { constraint } = &mut kind {
                let Some(made) = constraints.get(constraint) else {
                    left_out += 1;
                    continue;
                };
                *constraint = *made;
            } else if !mapped_all(&kind.refs())
                || (turned
                    && matches!(
                        kind,
                        DimensionKind::HorizontalDistance { .. }
                            | DimensionKind::VerticalDistance { .. }
                    ))
            {
                left_out += 1;
                continue;
            } else {
                remap_dimension(&mut kind, &map);
            }
            let id = match &d.value {
                None => self.add_dimension(kind, None, true, None)?,
                Some(v) => self.paste_value(kind, v, d.id, &mut values)?,
            };
            if let Some(text) = d.text {
                self.set_text_position(id, place(text, at, angle))?;
            }
            if !d.active
                && let Some(made) = self.def.dimensions.iter_mut().find(|k| k.id == id)
            {
                made.active = false;
                made.inactive_reason = d.inactive_reason.clone();
            }
        }
        for nested in &block.instances {
            let m = |u: EntityUid| map.get(&u).copied();
            let (Some(base), Some(direction)) = (m(nested.base), m(nested.direction)) else {
                continue;
            };
            let entities = nested
                .entities
                .iter()
                .filter_map(|(b, s)| {
                    let to = m(s.uid())?;
                    Some((
                        *b,
                        if s.is_point() {
                            Ref::Point(to)
                        } else {
                            Ref::Curve(to)
                        },
                    ))
                })
                .collect();
            let id = self.constraint_id();
            self.def.blocks.push(BlockInstance {
                id,
                block: nested.block,
                base,
                direction,
                entities,
                shape: nested.shape.clone(),
                open: None,
                lost: Vec::new(),
            });
        }
        Ok(left_out)
    }

    /// Turns instance `id` into plain sketch geometry with its block's
    /// constraints and dimensions (Explode Block). Its base point and
    /// direction go unless a constraint or dimension of the sketch uses
    /// them. Returns how many of the block's it left out.
    pub fn explode_block(
        &mut self,
        id: ConstraintUid,
        block: &SketchBlock,
    ) -> Result<usize, String> {
        let index = self.instance_index(id)?;
        if self.def.blocks[index].open.is_some() {
            return Err(super::said(
                "the block instance is being edited: finish the block first",
                format!("block instance {id} is open"),
            ));
        }
        let left_out = self.unfold(index, block)?;
        let instance = self.def.blocks.remove(index);
        // The helpers go when nothing holds them.
        let used: BTreeSet<EntityUid> = self
            .def
            .constraints
            .iter()
            .flat_map(|c| c.kind.refs())
            .chain(self.def.dimensions.iter().flat_map(|d| d.kind.refs()))
            .map(|r| r.uid())
            .collect();
        let at_index = EntityIndex::new(&self.def.entities);
        let tip = instance.tip(&at_index);
        let block_points: BTreeSet<EntityUid> =
            instance.entities.values().map(|r| r.uid()).collect();
        let helpers = [Some(instance.direction), tip, Some(instance.base)];
        if !helpers
            .iter()
            .flatten()
            .any(|h| used.contains(h) || block_points.contains(h))
        {
            let gone: BTreeSet<EntityUid> = helpers.into_iter().flatten().collect();
            self.drop_entities(&gone);
        }
        self.report.removed.push(id.to_string());
        Ok(left_out)
    }

    /// Opens instance `id` for an edit of its block in place: its geometry
    /// free, with the block's constraints and dimensions. Returns how many
    /// of them it left out.
    pub fn open_block(
        &mut self,
        id: ConstraintUid,
        block: &SketchBlock,
        temporary: bool,
    ) -> Result<usize, String> {
        if self.def.blocks.iter().any(|b| b.open.is_some()) {
            return Err(super::said(
                "a block is being edited in this sketch: finish it first",
                "the sketch has an open block instance",
            ));
        }
        let index = self.instance_index(id)?;
        let left_out = self.unfold(index, block)?;
        let since = self.next_entity_number();
        self.def.blocks[index].open = Some(OpenEdit { since, temporary });
        Ok(left_out)
    }

    /// The geometry of the open instance `id` as its block's new content:
    /// the block's points and curves it still has and every one made since
    /// it opened, with the constraints and dimensions between them, in the
    /// block's frame. They leave the sketch; the instance closes, keeping
    /// its geometry, and the instances of other blocks among it become the
    /// block's.
    pub fn close_block(&mut self, id: ConstraintUid) -> Result<ClosedBlock, String> {
        let index = self.instance_index(id)?;
        let instance = self.def.blocks[index].clone();
        let open = instance.open.clone().ok_or_else(|| {
            super::said(
                "the block instance is not being edited",
                format!("block instance {id} is not open"),
            )
        })?;
        let at_index = EntityIndex::new(&self.def.entities);
        let (at, angle, _) = instance
            .pose(&at_index)
            .ok_or("a block instance lacks its base point or direction")?;
        let turned = !unturned(angle);
        let tip = instance.tip(&at_index);
        let origin = axes::origin_point(&self.def);
        let special = |e: &Entity| e.axis.is_some() || Some(e.id) == origin;
        // The block's entities: what the instance had, and what the edit
        // made (but the sketch's origin and axes).
        let mut members: BTreeSet<EntityUid> = instance
            .entities
            .values()
            .map(|r| r.uid())
            .filter(|s| at_index.get(*s).is_some())
            .collect();
        for e in &self.def.entities {
            if e.id.0 >= open.since && !special(e) {
                members.insert(e.id);
            }
        }
        members.remove(&instance.direction);
        if let Some(tip) = tip {
            members.remove(&tip);
        }
        if self.def.texts.iter().any(|t| t.id.0 >= open.since) {
            return Err(super::said(
                "a text cannot go into a block: delete it, or finish the block without it",
                "a text was made while the block was open",
            ));
        }
        if members.iter().any(|m| self.def.is_projected(*m)) {
            return Err(projected_refusal());
        }
        // The base point goes into the block when the block's own
        // constraints hold its geometry to it.
        let base_used = self.def.constraints.iter().any(|c| {
            let refs = c.kind.refs();
            refs.iter().any(|r| r.uid() == instance.base)
                && refs
                    .iter()
                    .all(|r| r.uid() == instance.base || members.contains(&r.uid()))
        });
        if base_used {
            members.insert(instance.base);
        }
        // The block's ids: kept for its entities, new ones for the rest.
        let mut ids: BTreeMap<EntityUid, EntityUid> = instance
            .entities
            .iter()
            .filter(|(_, s)| members.contains(&s.uid()))
            .map(|(b, s)| (s.uid(), b.uid()))
            .collect();
        let mut top = instance.shape.iter().map(|e| e.id.0).max().unwrap_or(0);
        for e in &self.def.entities {
            if members.contains(&e.id) && !ids.contains_key(&e.id) {
                top += 1;
                ids.insert(e.id, EntityUid(top));
            }
        }
        let mut entities = Vec::new();
        for e in self.def.entities.iter().filter(|e| members.contains(&e.id)) {
            let kind = match e.kind {
                EntityKind::Point { at: p } => EntityKind::Point {
                    at: unplace(p, at, angle),
                },
                EntityKind::Circle {
                    center,
                    radius,
                    start,
                } => EntityKind::Circle {
                    center: ids.get(&center).copied().unwrap_or(center),
                    radius,
                    start: start - angle,
                },
                ref kind => mapped(kind, &ids),
            };
            let mut made = Entity::new(ids[&e.id], kind);
            made.construction = e.construction;
            entities.push(made);
        }
        let inside =
            |refs: &[Ref]| !refs.is_empty() && refs.iter().all(|r| members.contains(&r.uid()));
        let along = |a: EntityUid, b: EntityUid| -> Option<EntityUid> {
            if b == instance.direction && members.contains(&a) {
                Some(a)
            } else if a == instance.direction && members.contains(&b) {
                Some(b)
            } else {
                None
            }
        };
        let mut moved: BTreeSet<ConstraintUid> = BTreeSet::new();
        let mut constraints = Vec::new();
        for c in &self.def.constraints {
            let kind = match c.kind {
                // The block's horizontals and verticals of a turned instance.
                ConstraintKind::Parallel { a, b } if along(a, b).is_some() => {
                    ConstraintKind::Horizontal {
                        line: along(a, b).unwrap_or(a),
                    }
                }
                ConstraintKind::Perpendicular { a, b } if along(a, b).is_some() => {
                    ConstraintKind::Vertical {
                        line: along(a, b).unwrap_or(a),
                    }
                }
                ref kind if inside(&kind.refs()) => {
                    let framed = matches!(
                        kind,
                        ConstraintKind::Horizontal { .. }
                            | ConstraintKind::Vertical { .. }
                            | ConstraintKind::HorizontalPoints { .. }
                            | ConstraintKind::VerticalPoints { .. }
                    );
                    // A turned instance's horizontals place it: they stay.
                    if turned && framed {
                        continue;
                    }
                    kind.clone()
                }
                _ => continue,
            };
            let mut kind = kind;
            remap(&mut kind, &ids);
            moved.insert(c.id);
            constraints.push(Constraint { kind, ..c.clone() });
        }
        let params = &*self.params;
        let owner = self.owner();
        let mut dimensions = Vec::new();
        for d in &self.def.dimensions {
            let keep = match d.kind {
                DimensionKind::Spacing { constraint } => moved.contains(&constraint),
                DimensionKind::HorizontalDistance { .. }
                | DimensionKind::VerticalDistance { .. }
                    if turned =>
                {
                    false
                }
                ref kind => inside(&kind.refs()),
            };
            if !keep {
                continue;
            }
            let mut kind = d.kind.clone();
            remap_dimension(&mut kind, &ids);
            let value = d.value.map(|p| {
                let (expression, parameters) = value_of(params, owner, p);
                FragmentValue {
                    value: params.value(p).unwrap_or(f64::NAN),
                    expression,
                    parameters,
                }
            });
            moved.insert(d.id);
            dimensions.push(Dimension {
                id: d.id,
                kind,
                value,
                reference: None,
                text: d.text.map(|t| unplace(t, at, angle)),
                active: d.active,
                inactive_reason: d.inactive_reason.clone(),
            });
        }
        // The instances of other blocks among its geometry become the
        // block's.
        let mut nested = Vec::new();
        let mut taken = Vec::new();
        for b in self.def.blocks.iter().filter(|b| b.id != id) {
            let parts = b.members(&at_index);
            let within = parts.iter().filter(|p| members.contains(p)).count();
            if within == 0 {
                continue;
            }
            if within < parts.len() {
                return Err(super::said(
                    "an instance of another block is only partly in the block: take all of it \
                     or none",
                    format!("block instance {} is partly in the block", b.id),
                ));
            }
            let m = |u: EntityUid| ids.get(&u).copied();
            let (Some(base), Some(direction)) = (m(b.base), m(b.direction)) else {
                continue;
            };
            nested.push(BlockInstance {
                id: b.id,
                block: b.block,
                base,
                direction,
                entities: b
                    .entities
                    .iter()
                    .filter_map(|(k, s)| {
                        let to = m(s.uid())?;
                        Some((
                            *k,
                            if s.is_point() {
                                Ref::Point(to)
                            } else {
                                Ref::Curve(to)
                            },
                        ))
                    })
                    .collect(),
                shape: b.shape.clone(),
                open: None,
                lost: Vec::new(),
            });
            taken.push(b.id);
        }
        self.drop_constraints(&moved);
        self.def.blocks.retain(|b| !taken.contains(&b.id));
        let index = self.instance_index(id)?;
        let record = &mut self.def.blocks[index];
        record.entities = ids
            .iter()
            .filter_map(|(s, b)| {
                let e = entities.iter().find(|e| e.id == *b)?;
                Some((
                    e.as_ref(),
                    if e.is_point() {
                        Ref::Point(*s)
                    } else {
                        Ref::Curve(*s)
                    },
                ))
            })
            .collect();
        record.shape = entities.clone();
        record.open = None;
        let content = Fragment {
            version: FRAGMENT_VERSION,
            base: [0.0, 0.0],
            entities,
            texts: Vec::new(),
            constraints,
            dimensions,
            text_values: Vec::new(),
            references: Vec::new(),
            left_out: 0,
            instances: Vec::new(),
            blocks: Vec::new(),
        };
        if content.entities.is_empty() {
            return Err(super::said(
                "the block would have no geometry: delete the instance instead",
                format!("block instance {id} has no geometry left"),
            ));
        }
        Ok(ClosedBlock {
            content,
            instances: nested,
            temporary: open.temporary,
        })
    }

    /// Ends the edit in place of instance `id` without changing its block:
    /// what it made goes, and the instance is made again from `block`.
    pub fn cancel_block(&mut self, id: ConstraintUid, block: &SketchBlock) -> Result<(), String> {
        let index = self.instance_index(id)?;
        let Some(open) = self.def.blocks[index].open.clone() else {
            return Ok(());
        };
        let at_index = EntityIndex::new(&self.def.entities);
        let members = self.def.blocks[index].members(&at_index);
        let origin = axes::origin_point(&self.def);
        let made: BTreeSet<EntityUid> = self
            .def
            .entities
            .iter()
            .filter(|e| e.id.0 >= open.since && e.axis.is_none() && Some(e.id) != origin)
            .map(|e| e.id)
            .collect();
        let own: BTreeSet<ConstraintUid> = self
            .def
            .constraints
            .iter()
            .map(|c| (c.id, c.kind.refs()))
            .chain(self.def.dimensions.iter().map(|d| (d.id, d.kind.refs())))
            .filter(|(_, refs)| {
                !refs.is_empty()
                    && refs
                        .iter()
                        .all(|r| members.contains(&r.uid()) || made.contains(&r.uid()))
            })
            .map(|(k, _)| k)
            .collect();
        // The nested instances the edit unfolded.
        let nested: Vec<ConstraintUid> = self
            .def
            .blocks
            .iter()
            .filter(|b| b.id != id && b.members(&at_index).iter().all(|m| members.contains(m)))
            .map(|b| b.id)
            .collect();
        self.drop_constraints(&own);
        self.def.blocks.retain(|b| !nested.contains(&b.id));
        self.drop_entities(&made);
        let index = self.instance_index(id)?;
        self.def.blocks[index].open = None;
        self.remake_instance(index, block)?;
        Ok(())
    }

    /// Makes every closed instance of `block` again from it, where each is
    /// (an edited block). A constraint or dimension of the sketch on what
    /// the block no longer has goes, kept as the instance's lost item.
    /// Returns how many were lost.
    pub fn sync_block(&mut self, block: &SketchBlock) -> Result<usize, String> {
        let mut lost = 0;
        let indices: Vec<usize> = (0..self.def.blocks.len())
            .filter(|i| {
                let b = &self.def.blocks[*i];
                b.block == block.id && b.open.is_none()
            })
            .collect();
        for index in indices {
            lost += self.remake_instance(index, block)?;
        }
        Ok(lost)
    }

    fn remake_instance(&mut self, index: usize, block: &SketchBlock) -> Result<usize, String> {
        let mut instance = self.def.blocks[index].clone();
        let mut top = self.next_entity_number();
        let first = top;
        let mut next = || {
            top += 1;
            EntityUid(top - 1)
        };
        let remade = remake(&self.def.entities, &mut instance, block, &mut next)?;
        // Reserve the ids it took.
        let taken = (top - first) as usize;
        let _ = self.ids(taken);
        for (id, kind, construction) in remade.changed {
            if let Some(e) = self.def.entity_mut(id) {
                e.kind = kind;
                e.construction = construction;
            }
        }
        for made in remade.made {
            self.push(made);
        }
        // What the sketch held on removed geometry, with where it was.
        let index_now = EntityIndex::new(&self.def.entities);
        let at = |id: EntityUid| -> Option<P2> {
            let e = index_now.get(id)?;
            match &e.kind {
                EntityKind::Point { at } => Some(*at),
                kind => kind.points().first().and_then(|p| index_now.point(*p)),
            }
        };
        let gone = &remade.gone;
        let mut lost = Vec::new();
        let items = self
            .def
            .constraints
            .iter()
            .map(|c| (c.kind.type_name(), c.kind.refs()))
            .chain(
                self.def
                    .dimensions
                    .iter()
                    .map(|d| (d.kind.type_name().to_owned(), d.kind.refs())),
            );
        for (what, refs) in items {
            if let Some(r) = refs.iter().find(|r| gone.contains(&r.uid())) {
                lost.push(LostItem {
                    what,
                    at: at(r.uid()).unwrap_or_default(),
                });
            }
        }
        let count = lost.len();
        instance.lost.extend(lost);
        self.def.blocks[index] = instance;
        self.drop_entities(gone);
        Ok(count)
    }

    /// Turns `items` of this sketch into block `id` named `name`, its base
    /// point at `base` and its base direction at `angle`: the block's
    /// content, and the selection becomes an instance of it in place,
    /// keeping its ids and the sketch's constraints and dimensions that
    /// place it (with `keep_placing`; without, those go). The block's own
    /// constraints and dimensions leave the sketch.
    pub fn make_block(
        &mut self,
        id: u32,
        name: &str,
        items: &[Item],
        base: P2,
        angle: f64,
        keep_placing: bool,
    ) -> Result<(SketchBlock, ConstraintUid), String> {
        if !(base.iter().all(|v| v.is_finite()) && angle.is_finite()) {
            return Err(format!("the base point must be finite, got {base:?}"));
        }
        let index = EntityIndex::new(&self.def.entities);
        let origin = axes::origin_point(&self.def);
        let mut members: BTreeSet<EntityUid> = BTreeSet::new();
        for item in items {
            match item {
                Item::Entity(r) => {
                    let e = self
                        .def
                        .entity(r.uid())
                        .ok_or_else(|| format!("{r} does not exist"))?;
                    if e.axis.is_some() || Some(e.id) == origin {
                        continue;
                    }
                    members.insert(e.id);
                    members.extend(e.kind.points());
                }
                Item::Text(_) => {
                    return Err(super::said(
                        "a text cannot go into a block",
                        "texts are not block geometry",
                    ));
                }
                Item::Constraint(_) => {}
            }
        }
        // The sketch's origin as a point of a curve becomes the block's own.
        if let Some(o) = origin {
            members.remove(&o);
            if self
                .def
                .entities
                .iter()
                .any(|e| members.contains(&e.id) && e.kind.points().contains(&o))
            {
                return Err(super::said(
                    "a curve of the selection ends on the sketch's origin, which cannot go into a \
                     block: move it off the origin first",
                    "the selection uses the origin point",
                ));
            }
        }
        if members.is_empty() {
            return Err(super::said(
                "nothing to make a block of: select sketch points and curves",
                "no entities selected",
            ));
        }
        if members.iter().any(|m| self.def.is_projected(*m)) {
            return Err(projected_refusal());
        }
        // Instances in the selection: whole ones nest, open ones refuse.
        let mut nested_ids = Vec::new();
        for b in &self.def.blocks {
            let parts = b.members(&index);
            let within = parts.iter().filter(|p| members.contains(p)).count();
            if within == 0 {
                continue;
            }
            if b.open.is_some() {
                return Err(super::said(
                    "a block being edited cannot go into another block: finish it first",
                    format!("block instance {} is open", b.id),
                ));
            }
            nested_ids.push(b.id);
            members.extend(parts);
        }
        // Points of the curves taken, again for the nested helpers.
        for e in &self.def.entities {
            if members.contains(&e.id) {
                members.extend(e.kind.points());
            }
        }
        let inside =
            |refs: &[Ref]| !refs.is_empty() && refs.iter().all(|r| members.contains(&r.uid()));
        let mut constraints = Vec::new();
        let mut own: BTreeSet<ConstraintUid> = BTreeSet::new();
        let mut placing: BTreeSet<ConstraintUid> = BTreeSet::new();
        let turned = !unturned(angle);
        for c in &self.def.constraints {
            let refs = c.kind.refs();
            if inside(&refs) {
                let framed = matches!(
                    c.kind,
                    ConstraintKind::Horizontal { .. }
                        | ConstraintKind::Vertical { .. }
                        | ConstraintKind::HorizontalPoints { .. }
                        | ConstraintKind::VerticalPoints { .. }
                );
                if turned && framed {
                    // In a turned frame it places the instance.
                    placing.insert(c.id);
                    continue;
                }
                own.insert(c.id);
                constraints.push(c.clone());
            } else if refs.iter().any(|r| members.contains(&r.uid())) {
                placing.insert(c.id);
            }
        }
        let params = &*self.params;
        let owner = self.owner();
        let mut dimensions = Vec::new();
        for d in &self.def.dimensions {
            let refs = d.kind.refs();
            let keep = match d.kind {
                DimensionKind::Spacing { constraint } => own.contains(&constraint),
                DimensionKind::HorizontalDistance { .. }
                | DimensionKind::VerticalDistance { .. }
                    if turned =>
                {
                    false
                }
                _ => inside(&refs),
            };
            if !keep {
                if refs.iter().any(|r| members.contains(&r.uid())) {
                    placing.insert(d.id);
                }
                continue;
            }
            own.insert(d.id);
            let value = d.value.map(|p| {
                let (expression, parameters) = value_of(params, owner, p);
                FragmentValue {
                    value: params.value(p).unwrap_or(f64::NAN),
                    expression,
                    parameters,
                }
            });
            dimensions.push(Dimension {
                id: d.id,
                kind: d.kind.clone(),
                value,
                reference: None,
                text: d.text.map(|t| unplace(t, base, angle)),
                active: d.active,
                inactive_reason: d.inactive_reason.clone(),
            });
        }
        // The block's frame: the selection's ids, positions from the base.
        let entities: Vec<Entity> = self
            .def
            .entities
            .iter()
            .filter(|e| members.contains(&e.id))
            .map(|e| {
                let kind = match e.kind {
                    EntityKind::Point { at } => EntityKind::Point {
                        at: unplace(at, base, angle),
                    },
                    EntityKind::Circle {
                        center,
                        radius,
                        start,
                    } => EntityKind::Circle {
                        center,
                        radius,
                        start: start - angle,
                    },
                    ref kind => kind.clone(),
                };
                let mut made = Entity::new(e.id, kind);
                made.construction = e.construction;
                made
            })
            .collect();
        let nested: Vec<BlockInstance> = self
            .def
            .blocks
            .iter()
            .filter(|b| nested_ids.contains(&b.id))
            .cloned()
            .collect();
        let block = SketchBlock {
            id,
            name: name.to_owned(),
            content: Fragment {
                version: FRAGMENT_VERSION,
                base: [0.0, 0.0],
                entities,
                texts: Vec::new(),
                constraints,
                dimensions,
                text_values: Vec::new(),
                references: Vec::new(),
                left_out: 0,
                instances: Vec::new(),
                blocks: Vec::new(),
            },
            instances: nested,
        };
        // The selection becomes the instance: its own constraints and
        // dimensions and the nested records leave the sketch.
        if !keep_placing {
            own.extend(placing);
        }
        self.drop_constraints(&own);
        self.def.blocks.retain(|b| !nested_ids.contains(&b.id));
        let tip = self.add_point(place([tip_length(&block.content), 0.0], base, angle));
        let base_point = self.add_point(base);
        let direction = self.add_curve(EntityKind::Line {
            start: base_point,
            end: tip,
            centerline: false,
        });
        if let Some(line) = self.def.entity_mut(direction) {
            line.construction = true;
        }
        let instance = self.constraint_id();
        self.def.blocks.push(BlockInstance {
            id: instance,
            block: id,
            base: base_point,
            direction,
            entities: block
                .content
                .entities
                .iter()
                .map(|e| (e.as_ref(), e.as_ref()))
                .collect(),
            shape: block.content.entities.clone(),
            open: None,
            lost: Vec::new(),
        });
        Ok((block, instance))
    }
}
