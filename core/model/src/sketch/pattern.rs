// SPDX-License-Identifier: MIT
//! Sketch patterns (circular and rectangular pattern constraints):
//! copies of points and curves bound to their originals.
//!
//! A pattern is a copy group in the sketch definition, one record per
//! pattern with its originals, its values and the copy of every original
//! point and curve per instance. The quantity is part of the record: a new
//! quantity regenerates the instances ([`SketchEdit::sync_patterns`]),
//! keeping the copies of the instances that stay. The places of the copies
//! are not stored constraints: every solve binds each copy point to its
//! original with the pattern's transform ([`SketchPattern::bindings`],
//! solver constraints `Translated`, `Rotated`, `TurnedDirection` and
//! `EqualSize`), so the copies follow every edit of the originals and the
//! angle and spacings (parameters) can change afterwards. A copy point that
//! its curve already keeps at a distance from the centre (an arc's end, the
//! ends of an elliptical arc) is bound by its direction only, so no binding
//! is redundant and the copies add no degrees of freedom.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use super::edit::{PointInput, SketchEdit};
use super::geometry::{P2, add, norm, perp, scale, sub, unit};
use super::{ConstraintUid, Entity, EntityIndex, EntityKind, Ref, point_serde};
use crate::document::resolve_value;
use crate::features::ValueInput;
use crate::ids::EntityUid;
use crate::parameters::ParamId;

/// What a pattern does and its values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PatternKind<P> {
    /// `count` instances (the originals included) turned about `center`
    /// over `angle`: a full turn spreads them evenly, a smaller angle puts
    /// the last one at the angle. Counter-clockwise for a positive angle.
    Circular {
        #[serde(with = "point_serde")]
        center: EntityUid,
        count: u32,
        angle: P,
    },
    /// Rows and columns: `count[0]` along `direction` (a unit vector in
    /// sketch coordinates), `count[1]` a quarter turn from it, `spacing`
    /// apart.
    Rectangular {
        direction: [f64; 2],
        count: [u32; 2],
        spacing: [P; 2],
    },
}

/// One instance of a pattern: its place in the pattern (`[k, 0]` for the
/// k-th of a circular pattern, `[i, j]` for row i and column j) and the copy
/// of every original point and curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternCopy {
    pub index: [u32; 2],
    pub entities: BTreeMap<Ref, Ref>,
}

/// A pattern of a sketch, `k<n>` (constraints, dimensions and patterns
/// share the numbers).
#[derive(Debug, Clone, PartialEq)]
pub struct SketchPattern<P> {
    pub id: ConstraintUid,
    pub kind: PatternKind<P>,
    /// The patterned points and curves.
    pub entities: Vec<Ref>,
    pub copies: Vec<PatternCopy>,
}

#[derive(Serialize)]
struct PatternOut<'a, P> {
    id: ConstraintUid,
    #[serde(flatten)]
    kind: &'a PatternKind<P>,
    #[serde(with = "super::uids_serde")]
    entities: &'a [Ref],
    copies: &'a [PatternCopy],
}

impl<P: Serialize> Serialize for SketchPattern<P> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PatternOut {
            id: self.id,
            kind: &self.kind,
            entities: &self.entities,
            copies: &self.copies,
        }
        .serialize(serializer)
    }
}

impl<'de, P: Deserialize<'de>> Deserialize<'de> for SketchPattern<P> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = Map::<String, Value>::deserialize(deserializer)?;
        let id: ConstraintUid = match map.remove("id") {
            Some(Value::String(text)) => text.parse().map_err(D::Error::custom)?,
            _ => return Err(D::Error::missing_field("id")),
        };
        let entities: Vec<String> = match map.remove("entities") {
            Some(v) => serde_json::from_value(v).map_err(D::Error::custom)?,
            None => return Err(D::Error::missing_field("entities")),
        };
        let entities = entities
            .iter()
            .map(|e| e.parse::<Ref>().map_err(D::Error::custom))
            .collect::<Result<_, _>>()?;
        let copies = match map.remove("copies") {
            Some(v) => serde_json::from_value(v).map_err(D::Error::custom)?,
            None => Vec::new(),
        };
        let kind = PatternKind::<P>::deserialize(Value::Object(map)).map_err(D::Error::custom)?;
        Ok(Self {
            id,
            kind,
            entities,
            copies,
        })
    }
}

impl<P> SketchPattern<P> {
    pub fn type_name(&self) -> &'static str {
        match self.kind {
            PatternKind::Circular { .. } => "circular_pattern",
            PatternKind::Rectangular { .. } => "rectangular_pattern",
        }
    }

    /// The slots of its values: `patterns[k5].angle`, or
    /// `patterns[k5].spacing1` and `.spacing2`.
    pub fn slots(&self) -> Vec<String> {
        match self.kind {
            PatternKind::Circular { .. } => vec![format!("patterns[{}].angle", self.id)],
            PatternKind::Rectangular { .. } => vec![
                format!("patterns[{}].spacing1", self.id),
                format!("patterns[{}].spacing2", self.id),
            ],
        }
    }

    pub fn map_value<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SketchPattern<Q>, E> {
        let slots = self.slots();
        let kind = match &self.kind {
            PatternKind::Circular {
                center,
                count,
                angle,
            } => PatternKind::Circular {
                center: *center,
                count: *count,
                angle: f(&slots[0], angle)?,
            },
            PatternKind::Rectangular {
                direction,
                count,
                spacing,
            } => PatternKind::Rectangular {
                direction: *direction,
                count: *count,
                spacing: [f(&slots[0], &spacing[0])?, f(&slots[1], &spacing[1])?],
            },
        };
        Ok(SketchPattern {
            id: self.id,
            kind,
            entities: self.entities.clone(),
            copies: self.copies.clone(),
        })
    }

    /// The values in slot order.
    pub fn values(&self) -> Vec<&P> {
        match &self.kind {
            PatternKind::Circular { angle, .. } => vec![angle],
            PatternKind::Rectangular { spacing, .. } => spacing.iter().collect(),
        }
    }

    /// The instance indices the counts ask for, the originals' `[0, 0]`
    /// left out.
    pub fn indices(&self) -> Vec<[u32; 2]> {
        match self.kind {
            PatternKind::Circular { count, .. } => (1..count).map(|k| [k, 0]).collect(),
            PatternKind::Rectangular { count, .. } => (0..count[0])
                .flat_map(|i| (0..count[1]).map(move |j| [i, j]))
                .filter(|ij| *ij != [0, 0])
                .collect(),
        }
    }

    /// Every copy's entity, as a set.
    pub fn copy_ids(&self) -> BTreeSet<EntityUid> {
        self.copies
            .iter()
            .flat_map(|c| c.entities.values().map(|r| r.uid()))
            .collect()
    }

    /// Checks the counts, the direction and that the entities exist.
    pub fn check(&self, index: &EntityIndex<'_>) -> Result<(), String> {
        let what = self.id.to_string();
        match &self.kind {
            PatternKind::Circular { center, count, .. } => {
                if !(2..=1000).contains(count) {
                    return Err(format!("{what}: a pattern has 2 to 1000 instances"));
                }
                match index.get(*center) {
                    Some(e) if e.is_point() => {}
                    _ => return Err(format!("{what}: the centre p{} is not a point", center.0)),
                }
            }
            PatternKind::Rectangular {
                direction, count, ..
            } => {
                let total = count[0] as u64 * count[1] as u64;
                if count.contains(&0) || !(2..=1000).contains(&total) {
                    return Err(format!("{what}: a pattern has 2 to 1000 instances"));
                }
                if !direction.iter().all(|v| v.is_finite()) || (norm(*direction) - 1.0).abs() > 1e-9
                {
                    return Err(format!("{what}: the direction must be a unit vector"));
                }
            }
        }
        if self.entities.is_empty() {
            return Err(format!("{what}: nothing to pattern"));
        }
        let mut seen = BTreeSet::new();
        for r in &self.entities {
            match index.get(r.uid()) {
                Some(e) if e.is_point() == r.is_point() => {}
                _ => return Err(format!("{what}: {r} does not exist")),
            }
            if !seen.insert(r.uid()) {
                return Err(format!("{what}: {r} is listed twice"));
            }
        }
        for copy in &self.copies {
            for (original, made) in &copy.entities {
                for r in [original, made] {
                    match index.get(r.uid()) {
                        Some(e) if e.is_point() == r.is_point() => {}
                        _ => return Err(format!("{what}: {r} does not exist")),
                    }
                }
            }
        }
        Ok(())
    }
}

/// How a copy point is bound to its original, for the solver.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BindingKind {
    Translated {
        a: EntityUid,
        b: EntityUid,
        by: P2,
    },
    Rotated {
        center: EntityUid,
        a: EntityUid,
        b: EntityUid,
        angle: f64,
    },
    TurnedDirection {
        a_center: EntityUid,
        a: EntityUid,
        b_center: EntityUid,
        b: EntityUid,
        angle: f64,
    },
    EqualSize(EntityUid, EntityUid),
}

/// A binding with the pattern it comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Binding {
    pub owner: ConstraintUid,
    pub kind: BindingKind,
}

/// The transform of one instance: a turn about a point, or a move.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Placement {
    Turn { center: EntityUid, angle: f64 },
    Move(P2),
}

/// A circular pattern's step: a full turn spreads the instances evenly.
pub fn circular_step(count: u32, angle: f64) -> f64 {
    let full = (angle.abs() - TAU).abs() < 1e-9;
    if full {
        angle / count as f64
    } else {
        angle / (count.max(2) - 1) as f64
    }
}

/// The ends of curves that their curve keeps on its circle or ellipse
/// already, with the curve's centre: an arc's end, an elliptical arc's
/// start and end.
fn kept_ends(entities: &[&Entity]) -> BTreeMap<EntityUid, Vec<EntityUid>> {
    let mut kept: BTreeMap<EntityUid, Vec<EntityUid>> = BTreeMap::new();
    for e in entities {
        match e.kind {
            EntityKind::Arc { center, end, .. } => kept.entry(end).or_default().push(center),
            EntityKind::EllipticalArc {
                center, start, end, ..
            } => {
                kept.entry(start).or_default().push(center);
                kept.entry(end).or_default().push(center);
            }
            _ => {}
        }
    }
    kept
}

impl SketchPattern<ParamId> {
    /// The solver bindings of every copy with the pattern's values (angle,
    /// or the two spacings, in slot order).
    pub fn bindings(&self, values: &[f64], index: &EntityIndex<'_>) -> Vec<Binding> {
        let mut out = Vec::new();
        let originals: Vec<&Entity> = self
            .entities
            .iter()
            .filter_map(|r| index.get(r.uid()))
            .collect();
        let kept = kept_ends(&originals);
        for copy in &self.copies {
            let placement = self.placement(copy.index, values);
            let map = |u: EntityUid| -> Option<EntityUid> {
                copy.entities
                    .get(&Ref::Point(u))
                    .or_else(|| copy.entities.get(&Ref::Curve(u)))
                    .map(|r| r.uid())
            };
            let angle = match placement {
                Placement::Turn { angle, .. } => angle,
                Placement::Move(_) => 0.0,
            };
            for (original, made) in &copy.entities {
                let (a, b) = (original.uid(), made.uid());
                if a == b {
                    continue;
                }
                let bind = |kind| Binding {
                    owner: self.id,
                    kind,
                };
                match original {
                    Ref::Point(_) => match kept.get(&a).map(Vec::as_slice) {
                        None | Some([]) => out.push(bind(match placement {
                            Placement::Turn { center, angle } => BindingKind::Rotated {
                                center,
                                a,
                                b,
                                angle,
                            },
                            Placement::Move(by) => BindingKind::Translated { a, b, by },
                        })),
                        Some([center]) => {
                            if let Some(copy_center) = map(*center) {
                                out.push(bind(BindingKind::TurnedDirection {
                                    a_center: *center,
                                    a,
                                    b_center: copy_center,
                                    b,
                                    angle,
                                }));
                            }
                        }
                        // On two curves: their intersection fixes it.
                        Some(_) => {}
                    },
                    Ref::Curve(_) => {
                        if let Some(e) = index.get(a)
                            && matches!(
                                e.kind,
                                EntityKind::Circle { .. }
                                    | EntityKind::Ellipse { .. }
                                    | EntityKind::EllipticalArc { .. }
                            )
                        {
                            out.push(bind(BindingKind::EqualSize(a, b)));
                        }
                    }
                }
            }
        }
        out
    }

    fn placement(&self, index: [u32; 2], values: &[f64]) -> Placement {
        match &self.kind {
            PatternKind::Circular { center, count, .. } => Placement::Turn {
                center: *center,
                angle: circular_step(*count, values[0]) * index[0] as f64,
            },
            PatternKind::Rectangular { direction, .. } => {
                let d1 = *direction;
                let d2 = perp(d1);
                Placement::Move(add(
                    scale(d1, values[0] * index[0] as f64),
                    scale(d2, values[1] * index[1] as f64),
                ))
            }
        }
    }
}

/// Checks a pattern value: an angle not zero and at most a turn, a
/// positive spacing.
pub fn check_value(slot: &str, v: f64) -> Result<(), String> {
    if slot.ends_with(".angle") {
        if v.is_finite() && v != 0.0 && v.abs() <= TAU + 1e-9 {
            Ok(())
        } else {
            Err(format!(
                "must be between -360 and 360 degrees and not zero, got {} deg",
                v.to_degrees()
            ))
        }
    } else if v.is_finite() && v > 0.0 {
        Ok(())
    } else {
        Err(format!("must be greater than zero, got {v}"))
    }
}

/// A change of a pattern (`sketch.edit_pattern`); what is left out stays.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PatternChange {
    pub count: Option<[u32; 2]>,
    pub angle: Option<ValueInput>,
    pub spacing: Option<[ValueInput; 2]>,
    pub direction: Option<P2>,
    pub center: Option<PointInput>,
    pub entities: Option<Vec<Ref>>,
}

impl SketchEdit<'_> {
    fn pattern_index(&self, id: ConstraintUid) -> Result<usize, String> {
        self.def
            .patterns
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| format!("pattern {id} does not exist"))
    }

    /// The parameter of a pattern value: a number or an expression sets
    /// the pattern's own parameter, a name uses that parameter.
    fn pattern_value(
        &mut self,
        slot: &str,
        value: &ValueInput,
        old: Option<ParamId>,
    ) -> Result<ParamId, String> {
        let label = slot.rsplit('.').next().unwrap_or(slot);
        let comment = format!("{} pattern {label}", self.owner_name());
        let owner = self.owner();
        let (param, created) =
            resolve_value(self.params, slot, value, owner, &comment, old.as_ref())
                .map_err(|e| e.to_string())?;
        self.report.parameters.extend(created);
        Ok(param)
    }

    fn check_originals(&self, entities: &[Ref]) -> Result<(), String> {
        if entities.is_empty() {
            return Err("nothing to pattern".to_owned());
        }
        let copies: BTreeSet<EntityUid> = self
            .def
            .patterns
            .iter()
            .flat_map(|p| p.copy_ids())
            .collect();
        for r in entities {
            let e = self
                .def
                .entity(r.uid())
                .ok_or_else(|| format!("{r} does not exist"))?;
            if e.is_point() != r.is_point() {
                return Err(format!("{r} is a {}", e.kind.type_name()));
            }
            if copies.contains(&r.uid()) {
                return Err(format!(
                    "{r} is a copy of a pattern; pattern its original or edit that pattern"
                ));
            }
        }
        Ok(())
    }

    /// Copies entities around a centre (sketch Circular Pattern):
    /// `count` instances (the originals included) over `angle` (a full turn
    /// spreads them evenly). The copies follow the originals, and the
    /// angle is a parameter.
    pub fn circular_pattern(
        &mut self,
        entities: &[Ref],
        center: PointInput,
        count: u32,
        angle: &ValueInput,
    ) -> Result<ConstraintUid, String> {
        self.check_originals(entities)?;
        if !(2..=1000).contains(&count) {
            return Err(format!("a pattern has 2 to 1000 instances, got {count}"));
        }
        // A centre given by its position stays there.
        let new = matches!(center, PointInput::At(_));
        let center = self.point(center)?;
        if new {
            self.def.entity_mut(center).expect("made").fixed = true;
        }
        if entities.iter().any(|r| r.uid() == center) {
            return Err("the centre of a pattern cannot be patterned".to_owned());
        }
        let id = self.constraint_id();
        let angle = self.pattern_value(&format!("patterns[{id}].angle"), angle, None)?;
        self.def.patterns.push(SketchPattern {
            id,
            kind: PatternKind::Circular {
                center,
                count,
                angle,
            },
            entities: entities.to_vec(),
            copies: Vec::new(),
        });
        self.report.constraints.push(id);
        self.sync_patterns()?;
        Ok(id)
    }

    /// Copies entities in rows and columns along `direction` and a quarter
    /// turn from it (sketch Rectangular Pattern), `spacing` apart.
    pub fn rectangular_pattern(
        &mut self,
        entities: &[Ref],
        direction: P2,
        count: [u32; 2],
        spacing: [&ValueInput; 2],
    ) -> Result<ConstraintUid, String> {
        self.check_originals(entities)?;
        let total = count[0] as u64 * count[1] as u64;
        if count.contains(&0) || !(2..=1000).contains(&total) {
            return Err("a pattern has 2 to 1000 instances".to_owned());
        }
        let direction = unit(direction);
        if !(direction.iter().all(|v| v.is_finite()) && norm(direction) > 0.5) {
            return Err("the pattern needs a direction".to_owned());
        }
        let id = self.constraint_id();
        let s1 = self.pattern_value(&format!("patterns[{id}].spacing1"), spacing[0], None)?;
        let s2 = self.pattern_value(&format!("patterns[{id}].spacing2"), spacing[1], None)?;
        self.def.patterns.push(SketchPattern {
            id,
            kind: PatternKind::Rectangular {
                direction,
                count,
                spacing: [s1, s2],
            },
            entities: entities.to_vec(),
            copies: Vec::new(),
        });
        self.report.constraints.push(id);
        self.sync_patterns()?;
        Ok(id)
    }

    /// Changes a pattern's quantity, values, direction, centre or entities
    /// and regenerates its instances.
    pub fn edit_pattern(
        &mut self,
        id: ConstraintUid,
        change: &PatternChange,
    ) -> Result<(), String> {
        let i = self.pattern_index(id)?;
        if let Some(entities) = &change.entities {
            // The pattern's own copies are not originals.
            let own = self.def.patterns[i].copy_ids();
            if entities.iter().any(|r| own.contains(&r.uid())) {
                return Err("a copy of the pattern cannot be one of its originals".to_owned());
            }
            let others: Vec<SketchPattern<ParamId>> = self
                .def
                .patterns
                .iter()
                .filter(|p| p.id != id)
                .cloned()
                .collect();
            let saved = std::mem::replace(&mut self.def.patterns, others);
            let checked = self.check_originals(entities);
            self.def.patterns = saved;
            checked?;
            self.def.patterns[i].entities = entities.clone();
        }
        let center = match change.center {
            Some(c) => {
                let p = self.point(c)?;
                if matches!(c, PointInput::At(_)) {
                    self.def.entity_mut(p).expect("made").fixed = true;
                }
                Some(p)
            }
            None => None,
        };
        let pattern = self.def.patterns[i].clone();
        let slots = pattern.slots();
        let kind = match pattern.kind {
            PatternKind::Circular {
                center: old_center,
                count,
                angle,
            } => {
                if change.spacing.is_some() || change.direction.is_some() {
                    return Err(format!(
                        "{id} is a circular pattern: no spacing or direction"
                    ));
                }
                let angle = match &change.angle {
                    Some(v) => self.pattern_value(&slots[0], v, Some(angle))?,
                    None => angle,
                };
                let count = match change.count {
                    Some([n, 1] | [n, 0]) => n,
                    Some(other) => {
                        return Err(format!("a circular pattern has one count, got {other:?}"));
                    }
                    None => count,
                };
                if !(2..=1000).contains(&count) {
                    return Err(format!("a pattern has 2 to 1000 instances, got {count}"));
                }
                PatternKind::Circular {
                    center: center.unwrap_or(old_center),
                    count,
                    angle,
                }
            }
            PatternKind::Rectangular {
                direction,
                count,
                spacing,
            } => {
                if change.angle.is_some() || change.center.is_some() {
                    return Err(format!("{id} is a rectangular pattern: no angle or centre"));
                }
                let spacing = match &change.spacing {
                    Some([a, b]) => [
                        self.pattern_value(&slots[0], a, Some(spacing[0]))?,
                        self.pattern_value(&slots[1], b, Some(spacing[1]))?,
                    ],
                    None => spacing,
                };
                let count = change.count.unwrap_or(count);
                let total = count[0] as u64 * count[1] as u64;
                if count.contains(&0) || !(2..=1000).contains(&total) {
                    return Err("a pattern has 2 to 1000 instances".to_owned());
                }
                let direction = match change.direction {
                    Some(d) => {
                        let d = unit(d);
                        if !(d.iter().all(|v| v.is_finite()) && norm(d) > 0.5) {
                            return Err("the pattern needs a direction".to_owned());
                        }
                        d
                    }
                    None => direction,
                };
                PatternKind::Rectangular {
                    direction,
                    count,
                    spacing,
                }
            }
        };
        self.def.patterns[i].kind = kind;
        self.sync_patterns()
    }

    /// The value of a pattern's slot with the current parameters.
    fn pattern_values(&self, pattern: &SketchPattern<ParamId>) -> Result<Vec<f64>, String> {
        pattern
            .values()
            .into_iter()
            .map(|id| {
                self.params
                    .value(*id)
                    .ok_or_else(|| format!("parameter {} does not exist", self.params.name(*id)))
            })
            .collect()
    }

    /// Brings every pattern's copies in line with its originals and counts:
    /// instances that are no longer asked for go, missing ones are made,
    /// originals that are gone take their copies with them, and a copy
    /// whose original changed kind or points is made again. New copies are
    /// placed where the pattern puts them, so that the solve starts there.
    pub fn sync_patterns(&mut self) -> Result<(), String> {
        let mut doomed: BTreeSet<EntityUid> = BTreeSet::new();
        for i in 0..self.def.patterns.len() {
            let mut pattern = self.def.patterns[i].clone();
            pattern
                .entities
                .retain(|r| self.def.entity(r.uid()).is_some());
            if pattern.entities.is_empty() {
                doomed.extend(pattern.copy_ids());
                self.def.patterns.remove(i);
                return self.finish_sync(doomed, true);
            }
            let wanted: BTreeSet<[u32; 2]> = pattern.indices().into_iter().collect();
            // Instances no longer asked for.
            for copy in &pattern.copies {
                if !wanted.contains(&copy.index) {
                    doomed.extend(copy.entities.values().map(|r| r.uid()));
                }
            }
            pattern.copies.retain(|c| wanted.contains(&c.index));
            // The originals with the points of their curves.
            let mut originals: Vec<Ref> = Vec::new();
            for r in &pattern.entities {
                let e = self.def.entity(r.uid()).expect("kept above");
                for p in e.kind.points() {
                    if !originals.contains(&Ref::Point(p)) {
                        originals.push(Ref::Point(p));
                    }
                }
                if !originals.contains(r) {
                    originals.push(*r);
                }
            }
            let values = self.pattern_values(&pattern)?;
            let mut copies = Vec::new();
            for index in pattern.indices() {
                let mut copy = pattern
                    .copies
                    .iter()
                    .find(|c| c.index == index)
                    .cloned()
                    .unwrap_or(PatternCopy {
                        index,
                        entities: BTreeMap::new(),
                    });
                // Copies of originals that left, or that are gone.
                let stale: Vec<Ref> = copy
                    .entities
                    .iter()
                    .filter(|(o, c)| !originals.contains(o) || self.def.entity(c.uid()).is_none())
                    .map(|(o, _)| *o)
                    .collect();
                for o in stale {
                    if let Some(c) = copy.entities.remove(&o) {
                        doomed.insert(c.uid());
                    }
                }
                self.fill_copy(&pattern, &originals, &mut copy, &values, &mut doomed)?;
                copies.push(copy);
            }
            pattern.copies = copies;
            self.def.patterns[i] = pattern;
        }
        self.finish_sync(doomed, false)
    }

    /// Removes the entities of dropped copies, then syncs again if a
    /// pattern went.
    fn finish_sync(&mut self, doomed: BTreeSet<EntityUid>, again: bool) -> Result<(), String> {
        let live: BTreeSet<EntityUid> = doomed
            .into_iter()
            .filter(|u| self.def.entity(*u).is_some())
            .collect();
        if !live.is_empty() {
            self.drop_entities(&live);
        }
        if again { self.sync_patterns() } else { Ok(()) }
    }

    /// Makes the missing copies of one instance: points first, then curves
    /// on the copied points; a curve whose copy has another kind or other
    /// points is made again.
    fn fill_copy(
        &mut self,
        pattern: &SketchPattern<ParamId>,
        originals: &[Ref],
        copy: &mut PatternCopy,
        values: &[f64],
        doomed: &mut BTreeSet<EntityUid>,
    ) -> Result<(), String> {
        let place = |edit: &SketchEdit<'_>, p: P2| -> Result<P2, String> {
            Ok(match pattern.placement(copy.index, values) {
                Placement::Turn { center, angle } => {
                    let c = edit.at(center)?;
                    let (s, k) = angle.sin_cos();
                    let r = sub(p, c);
                    add(c, [k * r[0] - s * r[1], s * r[0] + k * r[1]])
                }
                Placement::Move(by) => add(p, by),
            })
        };
        // Curves whose copy no longer matches: made again.
        for o in originals.iter().filter(|r| !r.is_point()) {
            let Some(made) = copy.entities.get(o).copied() else {
                continue;
            };
            let original = self.def.entity(o.uid()).expect("an original").kind.clone();
            let current = self.def.entity(made.uid()).map(|e| e.kind.clone());
            let mut expected = original.clone();
            for p in original.points() {
                if let Some(Ref::Point(c)) = copy.entities.get(&Ref::Point(p)) {
                    expected.replace_point(p, *c);
                }
            }
            let same = current.as_ref().is_some_and(|k| {
                std::mem::discriminant(k) == std::mem::discriminant(&expected)
                    && k.points() == expected.points()
            });
            if !same {
                copy.entities.remove(o);
                doomed.insert(made.uid());
            }
        }
        // Curves take their ids first, as the drawing tools number them.
        let missing: Vec<Ref> = originals
            .iter()
            .filter(|r| !r.is_point() && !copy.entities.contains_key(r))
            .copied()
            .collect();
        let mut curve_ids = self.ids(missing.len()).into_iter();
        for o in originals.iter().filter(|r| r.is_point()) {
            if copy.entities.contains_key(o) {
                continue;
            }
            let at = place(self, self.at(o.uid())?)?;
            let id = self.add_point(at);
            copy.entities.insert(*o, Ref::Point(id));
        }
        for o in &missing {
            let original = self.def.entity(o.uid()).expect("an original").clone();
            let mut kind = original.kind.clone();
            for p in original.kind.points() {
                if let Some(Ref::Point(c)) = copy.entities.get(&Ref::Point(p)) {
                    kind.replace_point(p, *c);
                }
            }
            let id = curve_ids.next().expect("one id per missing curve");
            let mut made = Entity::new(id, kind);
            made.construction = original.construction;
            self.push(made);
            copy.entities.insert(*o, Ref::Curve(id));
        }
        Ok(())
    }
}
