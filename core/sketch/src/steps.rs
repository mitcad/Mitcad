// SPDX-License-Identifier: MIT
//! The work steps of a sketch (mitcad#1750): which use of a tool made which
//! points, curves, constraints, dimensions and texts. Sketch mode lists the
//! steps in the timeline's place, one cell per step, as the timeline lists
//! the features.
//!
//! A step is recorded by the edit that makes items ([`record`]) with the
//! kind its command names (`rectangle`, `project`, `fillet`, …); several
//! commands that one tool runs in one go (an edge chain projected, a line
//! with the constraints its snaps add) become one step when their undo
//! steps are merged ([`merge_new`]). Items deleted later leave their step,
//! and a step with no items left goes. Items a later step changes (a trim,
//! an extension) stay with the step that made them; the later step is a
//! step of its own that names them as `changed`, also when it made nothing.
//!
//! The steps are a record of how the sketch was made, not geometry: two
//! definitions that differ only in their steps are equal
//! ([`SketchSteps`]'s `PartialEq`), so a merge of steps neither evaluates
//! the sketch again nor changes what depends on it. Designs saved before
//! steps were recorded, and imports, have none: [`in_order`] shows each of
//! their points, curves, constraints and dimensions as a step of its own.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::def::SketchDef;
use crate::ids::EntityUid;
use crate::order::OrderItem;
use crate::{ConstraintUid, Ref};

/// Kinds of steps that a tool runs first for its inputs (a body edge
/// projected for a snap, an axis or the origin made for a pick): a step
/// merged from them and the tool's own command takes the tool's kind.
const HELPER_KINDS: &[&str] = &["project", "axis", "add_point"];

/// One use of a tool in a sketch: its id (written `s<n>`), its kind (the
/// command that made it, without `sketch.`), the items it made, and the
/// curves it changed that other steps made (a trim, an extension).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchStep {
    pub id: u32,
    pub kind: String,
    /// Its points and curves.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<Ref>,
    /// Its constraints and dimensions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<ConstraintUid>,
    /// Its texts (`t<n>`).
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "texts_serde")]
    pub texts: Vec<EntityUid>,
    /// The points and curves it changed that are another step's (or no
    /// step's): they stay with the step that made them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<Ref>,
}

impl SketchStep {
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
            && self.constraints.is_empty()
            && self.texts.is_empty()
            && self.changed.is_empty()
    }
}

/// Text ids in a step: `["t7", "t9"]`.
mod texts_serde {
    use crate::ids::{EntityUid, IdError};
    use mitcad_model_base::internal::ids::parse_number;

    pub fn serialize<S: serde::Serializer>(ids: &[EntityUid], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(ids.iter().map(|id| format!("t{}", id.0)))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EntityUid>, D::Error> {
        let texts: Vec<String> = serde::Deserialize::deserialize(d)?;
        texts
            .iter()
            .map(|text| {
                parse_number(text, "t")
                    .map(EntityUid)
                    .ok_or_else(|| IdError::new("text", text, "t<number>"))
                    .map_err(serde::de::Error::custom)
            })
            .collect()
    }
}

/// A sketch's steps in the order they were recorded. Always equal to any
/// other: a sketch's definition is the same whatever its steps say (see
/// the module's description).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SketchSteps(pub Vec<SketchStep>);

impl PartialEq for SketchSteps {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl SketchSteps {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The steps' ids.
    pub fn ids(&self) -> BTreeSet<u32> {
        self.0.iter().map(|s| s.id).collect()
    }

    pub fn step(&self, id: u32) -> Option<&SketchStep> {
        self.0.iter().find(|s| s.id == id)
    }
}

/// The ids of a sketch's points, curves, constraints, dimensions and texts,
/// to tell what an edit made ([`record`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Items {
    pub entities: BTreeSet<EntityUid>,
    pub constraints: BTreeSet<ConstraintUid>,
    pub texts: BTreeSet<EntityUid>,
}

impl Items {
    pub fn of<P>(def: &SketchDef<P>) -> Self {
        Self {
            entities: def.entities.iter().map(|e| e.id).collect(),
            constraints: def
                .constraints
                .iter()
                .map(|c| c.id)
                .chain(def.dimensions.iter().map(|d| d.id))
                .collect(),
            texts: def.texts.iter().map(|t| t.id).collect(),
        }
    }
}

/// Keeps the steps to the items the sketch has: deleted items leave their
/// step (and the steps that changed them), a step left with none goes, and
/// an id given to another kind of entity (a point's number taken by a
/// curve) is no longer the step's.
pub fn prune<P>(def: &mut SketchDef<P>) {
    if def.steps.is_empty() {
        return;
    }
    let refs: BTreeSet<Ref> = def.entities.iter().map(|e| e.as_ref()).collect();
    let items = Items::of(def);
    for step in &mut def.steps.0 {
        step.entities.retain(|r| refs.contains(r));
        step.constraints.retain(|c| items.constraints.contains(c));
        step.texts.retain(|t| items.texts.contains(t));
        step.changed.retain(|r| refs.contains(r));
    }
    def.steps.0.retain(|s| !s.is_empty());
}

/// Records what an edit made as a step of `kind` (with none, the edit
/// records nothing: an import, an internal edit), after keeping the steps
/// to the items there are ([`prune`]). `before` is what the sketch had
/// before the edit; `changed` the curves the command changes (a trim's or
/// an extension's curve), recorded as the step's `changed` where they are
/// still there and were there before. Returns the new step's id; none when
/// the edit made and changed nothing (a drag, a new value).
pub fn record<P>(
    def: &mut SketchDef<P>,
    before: &Items,
    kind: Option<&str>,
    changed: &[EntityUid],
) -> Option<u32> {
    prune(def);
    let kind = kind?;
    let now = Items::of(def);
    let entities: Vec<Ref> = def
        .entities
        .iter()
        .filter(|e| !before.entities.contains(&e.id))
        .map(|e| e.as_ref())
        .collect::<BTreeSet<Ref>>()
        .into_iter()
        .collect();
    let constraints: Vec<ConstraintUid> = now
        .constraints
        .difference(&before.constraints)
        .copied()
        .collect();
    let texts: Vec<EntityUid> = now.texts.difference(&before.texts).copied().collect();
    let changed: Vec<Ref> = changed
        .iter()
        .filter(|uid| before.entities.contains(*uid))
        .filter_map(|uid| def.entity(*uid).map(|e| e.as_ref()))
        .collect::<BTreeSet<Ref>>()
        .into_iter()
        .collect();
    if entities.is_empty() && constraints.is_empty() && texts.is_empty() && changed.is_empty() {
        return None;
    }
    // A number used again (the newest item deleted, then one made) is the
    // new step's only.
    let new_entities: BTreeSet<EntityUid> = entities.iter().map(|r| r.uid()).collect();
    for step in &mut def.steps.0 {
        step.entities.retain(|r| !new_entities.contains(&r.uid()));
        step.changed.retain(|r| !new_entities.contains(&r.uid()));
        step.constraints.retain(|c| !constraints.contains(c));
        step.texts.retain(|t| !texts.contains(t));
    }
    def.steps.0.retain(|s| !s.is_empty());
    let id = def.steps.0.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    def.steps.0.push(SketchStep {
        id,
        kind: kind.to_owned(),
        entities,
        constraints,
        texts,
        changed,
    });
    Some(id)
}

/// Makes the steps recorded since `before` (the step ids the sketch had)
/// one step: what one tool made in one go through several commands, whose
/// undo steps are merged. It keeps the first one's id; its kind is the
/// first that is not a helper's ([`HELPER_KINDS`]: a projection or an axis
/// made for an input), else the first one's. What one of them made and
/// another changed is the merged step's own. Returns whether it merged
/// any.
pub fn merge_new<P>(def: &mut SketchDef<P>, before: &BTreeSet<u32>) -> bool {
    let new: Vec<usize> = def
        .steps
        .0
        .iter()
        .enumerate()
        .filter(|(_, s)| !before.contains(&s.id))
        .map(|(i, _)| i)
        .collect();
    if new.len() < 2 {
        return false;
    }
    let kind = new
        .iter()
        .map(|&i| def.steps.0[i].kind.as_str())
        .find(|k| !HELPER_KINDS.contains(k))
        .unwrap_or(def.steps.0[new[0]].kind.as_str())
        .to_owned();
    let mut entities: BTreeSet<Ref> = BTreeSet::new();
    let mut constraints: BTreeSet<ConstraintUid> = BTreeSet::new();
    let mut texts: BTreeSet<EntityUid> = BTreeSet::new();
    let mut changed: BTreeSet<Ref> = BTreeSet::new();
    for &i in &new {
        let step = &def.steps.0[i];
        entities.extend(step.entities.iter().copied());
        constraints.extend(step.constraints.iter().copied());
        texts.extend(step.texts.iter().copied());
        changed.extend(step.changed.iter().copied());
    }
    let changed: Vec<Ref> = changed.difference(&entities).copied().collect();
    let first = new[0];
    let merged = SketchStep {
        id: def.steps.0[first].id,
        kind,
        entities: entities.into_iter().collect(),
        constraints: constraints.into_iter().collect(),
        texts: texts.into_iter().collect(),
        changed,
    };
    let mut index = 0;
    def.steps.0.retain(|_| {
        let keep = index == first || !new.contains(&index);
        index += 1;
        keep
    });
    if let Some(step) = def.steps.0.iter_mut().find(|s| s.id == merged.id) {
        *step = merged;
    }
    true
}

/// A step as sketch mode lists it ([`in_order`]): a recorded step with its
/// id and kind, or an item no recorded step holds, alone (no id, no kind).
#[derive(Debug, Clone, PartialEq)]
pub struct ShownStep {
    pub id: Option<u32>,
    pub kind: Option<String>,
    /// The ids of its points, curves, constraints and dimensions, made or
    /// changed, in the order they were made
    /// ([`SketchDef::creation_order`]), then its texts.
    pub items: Vec<String>,
    /// Of them, those it changed that another step made (or none did).
    pub changed: Vec<String>,
}

/// The key an item is held by a step under: its entity's or its
/// constraint's number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Entity(EntityUid),
    Constraint(ConstraintUid),
}

fn key(item: &OrderItem) -> Key {
    match item {
        OrderItem::Entity(r) => Key::Entity(r.uid()),
        OrderItem::Constraint(id) | OrderItem::Dimension(id) => Key::Constraint(*id),
    }
}

/// The sketch's steps as sketch mode lists them: each recorded step where
/// its first item comes in the creation order, after every recorded step
/// made before it (a step that made no point, curve, constraint or
/// dimension, only texts or changes, comes before the next recorded one, or
/// last); each point, curve, constraint or dimension no step holds as a
/// step of its own, in the creation order (designs saved before steps were
/// recorded, imports). Texts are listed only in the steps that made them.
pub fn in_order<P>(def: &SketchDef<P>) -> Vec<ShownStep> {
    let order = def.creation_order();
    let mut steps: Vec<&SketchStep> = def.steps.0.iter().collect();
    steps.sort_by_key(|s| s.id);
    // Which step holds an item: the newest that names it.
    let mut owner: HashMap<Key, usize> = HashMap::new();
    for (index, step) in steps.iter().enumerate() {
        for r in &step.entities {
            owner.insert(Key::Entity(r.uid()), index);
        }
        for c in &step.constraints {
            owner.insert(Key::Constraint(*c), index);
        }
    }
    let changed: Vec<HashSet<Key>> = steps
        .iter()
        .map(|s| s.changed.iter().map(|r| Key::Entity(r.uid())).collect())
        .collect();
    let texts: BTreeSet<EntityUid> = def.texts.iter().map(|t| t.id).collect();
    // Each step's items in the creation order, then its texts; whether it
    // holds any item of the creation order as its own.
    let mut held: Vec<ShownStep> = steps
        .iter()
        .map(|s| ShownStep {
            id: Some(s.id),
            kind: Some(s.kind.clone()),
            items: Vec::new(),
            changed: Vec::new(),
        })
        .collect();
    for item in &order {
        let k = key(item);
        let made = owner.get(&k).copied();
        for (index, shown) in held.iter_mut().enumerate() {
            if made == Some(index) {
                shown.items.push(item.to_string());
            } else if changed[index].contains(&k) {
                shown.items.push(item.to_string());
                shown.changed.push(item.to_string());
            }
        }
    }
    for (index, step) in steps.iter().enumerate() {
        for t in step.texts.iter().filter(|t| texts.contains(*t)) {
            held[index].items.push(format!("t{}", t.0));
        }
    }
    let mut shown = Vec::new();
    let mut done = vec![false; steps.len()];
    let emit = |index: usize, shown: &mut Vec<ShownStep>, done: &mut [bool]| {
        done[index] = true;
        shown.push(held[index].clone());
    };
    for item in &order {
        match owner.get(&key(item)) {
            Some(&index) => {
                if done[index] {
                    continue;
                }
                let earlier: Vec<usize> = (0..index)
                    .filter(|&e| !done[e] && !held[e].items.is_empty())
                    .collect();
                for e in earlier {
                    emit(e, &mut shown, &mut done);
                }
                emit(index, &mut shown, &mut done);
            }
            None => shown.push(ShownStep {
                id: None,
                kind: None,
                items: vec![item.to_string()],
                changed: Vec::new(),
            }),
        }
    }
    // The steps that hold none of the creation order's items as their own
    // and came after every other: last.
    let rest: Vec<usize> = (0..steps.len())
        .filter(|&e| !done[e] && !held[e].items.is_empty())
        .collect();
    for e in rest {
        emit(e, &mut shown, &mut done);
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Constraint, ConstraintKind, Dimension, DimensionKind, Entity, EntityKind, SketchText,
    };

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

    /// A line from p<a> to p<a+1> numbered c<a+2>.
    fn add_line(def: &mut SketchDef<()>, a: u32) {
        def.entities
            .push(Entity::point(EntityUid(a), [0.0, f64::from(a)]));
        def.entities
            .push(Entity::point(EntityUid(a + 1), [10.0, f64::from(a)]));
        def.entities.push(line(a + 2, a, a + 1));
    }

    fn shown(def: &SketchDef<()>) -> Vec<(Option<String>, Vec<String>)> {
        in_order(def)
            .into_iter()
            .map(|s| (s.kind, s.items))
            .collect()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn an_edit_records_what_it_made_as_one_step() {
        let mut def = SketchDef::<()>::default();
        let before = Items::of(&def);
        add_line(&mut def, 1);
        def.constraints.push(Constraint::new(
            ConstraintUid(1),
            ConstraintKind::Horizontal { line: EntityUid(3) },
        ));
        assert_eq!(record(&mut def, &before, Some("add_line"), &[]), Some(1));
        let step = def.steps.step(1).unwrap();
        assert_eq!(step.kind, "add_line");
        assert_eq!(
            step.entities,
            [
                Ref::Point(EntityUid(1)),
                Ref::Point(EntityUid(2)),
                Ref::Curve(EntityUid(3))
            ]
        );
        assert_eq!(step.constraints, [ConstraintUid(1)]);
        // An edit that made nothing records nothing; one without a kind
        // neither.
        let before = Items::of(&def);
        assert_eq!(record(&mut def, &before, Some("drag"), &[]), None);
        add_line(&mut def, 4);
        assert_eq!(record(&mut def, &before, None, &[]), None);
        assert_eq!(def.steps.0.len(), 1);
    }

    #[test]
    fn deleted_items_leave_their_step_and_an_empty_step_goes() {
        let mut def = SketchDef::<()>::default();
        let before = Items::of(&def);
        add_line(&mut def, 1);
        record(&mut def, &before, Some("add_line"), &[]);
        let before = Items::of(&def);
        add_line(&mut def, 4);
        record(&mut def, &before, Some("add_line"), &[]);
        // The first line's curve deleted: its points stay in its step.
        def.entities.retain(|e| e.id != EntityUid(3));
        prune(&mut def);
        assert_eq!(def.steps.step(1).unwrap().entities.len(), 2);
        // The second line wholly: its step goes.
        def.entities.retain(|e| e.id.0 < 4);
        prune(&mut def);
        assert_eq!(def.steps.ids(), BTreeSet::from([1]));
    }

    #[test]
    fn a_number_used_again_belongs_to_the_new_step() {
        let mut def = SketchDef::<()>::default();
        let before = Items::of(&def);
        add_line(&mut def, 1);
        record(&mut def, &before, Some("add_line"), &[]);
        // c3 deleted, then a circle made with its number.
        def.entities.retain(|e| e.id != EntityUid(3));
        let before = Items::of(&def);
        def.entities.push(Entity::new(
            EntityUid(3),
            EntityKind::Circle {
                center: EntityUid(1),
                radius: 5.0,
                start: 0.0,
            },
        ));
        record(&mut def, &before, Some("circle"), &[]);
        assert_eq!(
            def.steps.step(1).unwrap().entities,
            [Ref::Point(EntityUid(1)), Ref::Point(EntityUid(2))]
        );
        assert_eq!(
            def.steps.step(2).unwrap().entities,
            [Ref::Curve(EntityUid(3))]
        );
    }

    #[test]
    fn steps_of_one_go_merge_under_the_tools_kind() {
        let mut def = SketchDef::<()>::default();
        let before = Items::of(&def);
        add_line(&mut def, 1);
        record(&mut def, &before, Some("rectangle"), &[]);
        let kept = def.steps.ids();
        // A projection made for a snap, the line, and its constraint.
        let before = Items::of(&def);
        def.entities.push(Entity::point(EntityUid(4), [5.0, 5.0]));
        record(&mut def, &before, Some("project"), &[]);
        let before = Items::of(&def);
        def.entities.push(Entity::point(EntityUid(5), [6.0, 6.0]));
        def.entities.push(line(6, 4, 5));
        record(&mut def, &before, Some("add_line"), &[]);
        let before = Items::of(&def);
        def.constraints.push(Constraint::new(
            ConstraintUid(1),
            ConstraintKind::Horizontal { line: EntityUid(6) },
        ));
        record(&mut def, &before, Some("add_constraint"), &[]);
        assert_eq!(def.steps.0.len(), 4);
        assert!(merge_new(&mut def, &kept));
        assert_eq!(def.steps.0.len(), 2);
        let merged = def.steps.step(2).unwrap();
        assert_eq!(merged.kind, "add_line");
        assert_eq!(merged.entities.len(), 3);
        assert_eq!(merged.constraints, [ConstraintUid(1)]);
        // Nothing new: nothing merged.
        let kept = def.steps.ids();
        assert!(!merge_new(&mut def, &kept));
        // Only projections: a Project step.
        let before = Items::of(&def);
        def.entities.push(Entity::point(EntityUid(7), [1.0, 1.0]));
        record(&mut def, &before, Some("project"), &[]);
        let before = Items::of(&def);
        def.entities.push(Entity::point(EntityUid(8), [2.0, 1.0]));
        record(&mut def, &before, Some("project"), &[]);
        assert!(merge_new(&mut def, &kept));
        assert_eq!(def.steps.step(3).unwrap().kind, "project");
        assert_eq!(def.steps.step(3).unwrap().entities.len(), 2);
    }

    #[test]
    fn steps_are_listed_in_the_order_they_were_made() {
        let mut def = SketchDef::<()>::default();
        // Two lines from before steps were recorded, each item alone.
        add_line(&mut def, 1);
        // A line made as a step, then a horizontal on the first, older
        // line: the step of the newer line comes first.
        let before = Items::of(&def);
        add_line(&mut def, 4);
        record(&mut def, &before, Some("add_line"), &[]);
        let before = Items::of(&def);
        def.constraints.push(Constraint::new(
            ConstraintUid(1),
            ConstraintKind::Horizontal { line: EntityUid(3) },
        ));
        record(&mut def, &before, Some("add_constraint"), &[]);
        let before = Items::of(&def);
        def.dimensions.push(Dimension::new(
            ConstraintUid(2),
            DimensionKind::Length { line: EntityUid(6) },
            Some(()),
        ));
        record(&mut def, &before, Some("add_dimension"), &[]);
        assert_eq!(
            shown(&def),
            [
                (None, strings(&["p1"])),
                (None, strings(&["p2"])),
                (None, strings(&["c3"])),
                (Some("add_line".to_owned()), strings(&["p4", "p5", "c6"])),
                (Some("add_constraint".to_owned()), strings(&["k1"])),
                (Some("add_dimension".to_owned()), strings(&["k2"])),
            ]
        );
    }

    #[test]
    fn a_trim_that_only_shortens_is_a_step_of_its_own() {
        let mut def = SketchDef::<()>::default();
        let before = Items::of(&def);
        add_line(&mut def, 1);
        record(&mut def, &before, Some("add_line"), &[]);
        // The trim shortened c3 and made nothing.
        let before = Items::of(&def);
        assert_eq!(
            record(&mut def, &before, Some("trim"), &[EntityUid(3)]),
            Some(2)
        );
        let trim = def.steps.step(2).unwrap();
        assert!(trim.entities.is_empty());
        assert_eq!(trim.changed, [Ref::Curve(EntityUid(3))]);
        // c3 stays the line's; the trim lists it as changed, after the
        // line, and a line made later comes after the trim.
        let before = Items::of(&def);
        add_line(&mut def, 4);
        record(&mut def, &before, Some("add_line"), &[]);
        let listed = in_order(&def);
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].items, strings(&["p1", "p2", "c3"]));
        assert_eq!(listed[1].kind.as_deref(), Some("trim"));
        assert_eq!(listed[1].items, strings(&["c3"]));
        assert_eq!(listed[1].changed, strings(&["c3"]));
        assert_eq!(listed[2].items, strings(&["p4", "p5", "c6"]));
        // The curve deleted: the trim's step, left with nothing, goes.
        def.entities.retain(|e| e.id != EntityUid(3));
        prune(&mut def);
        assert!(def.steps.step(2).is_none());
    }

    #[test]
    fn a_text_is_its_steps_item() {
        let mut def = SketchDef::<()>::default();
        let before = Items::of(&def);
        def.texts
            .push(SketchText::new(EntityUid(1), "Label", [0.0, 0.0], 5.0));
        assert_eq!(record(&mut def, &before, Some("add_text"), &[]), Some(1));
        assert_eq!(def.steps.step(1).unwrap().texts, [EntityUid(1)]);
        // Listed though texts are not in the creation order.
        let listed = in_order(&def);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].items, strings(&["t1"]));
        // Stored as text ids.
        let stored = serde_json::to_value(&def.steps).unwrap();
        assert_eq!(stored[0]["texts"], serde_json::json!(["t1"]));
        let back: SketchSteps = serde_json::from_value(stored).unwrap();
        assert_eq!(back.0, def.steps.0);
        // Deleted, it takes its step with it.
        def.texts.clear();
        prune(&mut def);
        assert!(def.steps.is_empty());
    }

    #[test]
    fn a_definition_is_the_same_whatever_its_steps() {
        let mut def = SketchDef::<()>::default();
        add_line(&mut def, 1);
        let plain = def.clone();
        record(&mut def, &Items::default(), Some("add_line"), &[]);
        assert_eq!(def, plain);
        assert!(!def.steps.is_empty());
    }
}
