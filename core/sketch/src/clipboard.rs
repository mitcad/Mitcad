// SPDX-License-Identifier: MIT
//! Copy, Cut and Paste of sketch geometry (mitcad#788): a fragment of a
//! sketch that another sketch, of this design or of another, takes in.
//!
//! - [`copy`] makes a [`Fragment`] of selected points, curves and texts: the
//!   entities with the points their curves use (a text brings the points of
//!   its frame and the curve it runs along), and the constraints and
//!   dimensions that reference only those or the sketch's origin and axes
//!   (mitcad#500). The others that reference copied geometry are left out
//!   and counted ([`Fragment::left_out`]).
//! - The origin and the axes are not copied: the fragment names the
//!   entities that stood for them ([`Fragment::references`]), and a paste
//!   takes the target sketch's own (made when it has none). A curve that
//!   uses the origin point stays on the target's origin.
//! - A driving dimension keeps its expression when it uses other parameters
//!   ([`FragmentValue`]): a paste takes the expression where every one of
//!   them exists in the target design, else the value. A text that follows
//!   a text parameter (mitcad#664) keeps it the same way.
//! - Projected and intersected geometry becomes ordinary geometry: free,
//!   not reference geometry and following nothing (as Convert to Sketch
//!   Geometry makes it, mitcad#257). Patterns and offsets are pasted as the
//!   geometry they made.
//! - [`SketchEdit::paste`] adds a fragment moved by an offset, with new ids;
//!   the base point ([`Fragment::base`]) is the first selected item's first
//!   point, so that a paste at the same place keeps the coordinates (also
//!   on a sketch of another plane). [`held`] says in which directions the
//!   kept constraints and dimensions hold the fragment to the origin and
//!   axes: a paste places it only in the others.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::cleanup::remap_dimension;
use super::edit::SketchEdit;
use super::geometry::{P2, add};
use super::modify::{Item, remap};
use super::{
    Constraint, ConstraintKind, ConstraintUid, Dimension, DimensionKind, Entity, EntityKind, Ref,
    SketchAxis, SketchText, axes,
};
use crate::def::SketchDef;
use crate::ids::{EntityUid, FeatureUid};
use crate::parameters::{ParamId, Parameters};
use crate::values::ValueInput;

/// The fragment format's version: a paste refuses a newer one.
pub const FRAGMENT_VERSION: u32 = 1;

/// What an entity of the source stood for (mitcad#500).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchReference {
    /// The sketch's origin point.
    Origin,
    /// The line of the x or y axis.
    X,
    Y,
    /// The far end point of the x or y axis's line.
    XEnd,
    YEnd,
}

impl SketchReference {
    fn axis(self) -> Option<SketchAxis> {
        match self {
            Self::X | Self::XEnd => Some(SketchAxis::X),
            Self::Y | Self::YEnd => Some(SketchAxis::Y),
            Self::Origin => None,
        }
    }
}

/// An entity id of the fragment that stands for the origin or an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FragmentReference {
    pub id: Ref,
    pub is: SketchReference,
}

/// A driving dimension's value in a fragment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FragmentValue {
    /// The value, millimetres or radians.
    pub value: f64,
    /// The expression of its parameter when it uses other parameters (or
    /// the name of a parameter it uses as it is); None for a plain value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    /// The parameters the expression uses, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<String>,
}

/// A text that follows a text parameter (mitcad#664), in a fragment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FragmentTextValue {
    #[serde(with = "super::text_serde")]
    pub text: EntityUid,
    pub expression: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<String>,
}

/// A piece of a sketch on the clipboard (see the module documentation).
/// Its ids are the source sketch's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fragment {
    pub version: u32,
    /// Where the pasted geometry is held: a paste at `at` moves it by
    /// `at - base`.
    pub base: P2,
    /// Points and curves, in the source's order.
    #[serde(default)]
    pub entities: Vec<Entity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<SketchText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<Dimension<FragmentValue>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_values: Vec<FragmentTextValue>,
    /// The ids that stood for the source's origin and axes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<FragmentReference>,
    /// The constraints and dimensions on copied geometry that were left
    /// out: they also reference geometry that was not copied.
    #[serde(default)]
    pub left_out: usize,
}

/// The source's entities that stand for its origin and axes.
fn special_entities(def: &SketchDef) -> BTreeMap<EntityUid, SketchReference> {
    let mut special = BTreeMap::new();
    if let Some(origin) = axes::origin_point(def) {
        special.insert(origin, SketchReference::Origin);
    }
    for (axis, line, end) in [
        (SketchAxis::X, SketchReference::X, SketchReference::XEnd),
        (SketchAxis::Y, SketchReference::Y, SketchReference::YEnd),
    ] {
        let Some(id) = axes::axis_line(def, axis) else {
            continue;
        };
        special.insert(id, line);
        if let Some(EntityKind::Line { end: far, .. }) = def.entity(id).map(|e| &e.kind) {
            special.insert(*far, end);
        }
    }
    special
}

/// A driving dimension's or a text's value as a fragment keeps it: the
/// parameter's expression with the parameters it uses, or its name when it
/// is not the sketch's own.
fn value_of(params: &Parameters, owner: FeatureUid, id: ParamId) -> (Option<String>, Vec<String>) {
    let Some(p) = params.get(id) else {
        return (None, Vec::new());
    };
    if params.owner(id) != Some(owner) {
        return (Some(p.name().to_owned()), vec![p.name().to_owned()]);
    }
    let uses: Vec<String> = p.dependencies().iter().map(|d| params.name(*d)).collect();
    if uses.is_empty() {
        (None, Vec::new())
    } else {
        (Some(p.expression().to_owned()), uses)
    }
}

/// The fragment of `items` (points, curves and texts; constraints and
/// dimensions come with the geometry they reference) of the sketch `def`,
/// owned by `owner`. Refused when an item does not exist or nothing but the
/// origin and the axes is selected.
pub fn copy(
    def: &SketchDef,
    params: &Parameters,
    owner: FeatureUid,
    items: &[Item],
) -> Result<Fragment, String> {
    let special = special_entities(def);
    let mut copied: BTreeSet<EntityUid> = BTreeSet::new();
    let mut texts: BTreeSet<EntityUid> = BTreeSet::new();
    let mut base: Option<P2> = None;
    let position = |id: EntityUid| -> Option<P2> {
        match def.entity(id)?.kind {
            EntityKind::Point { at } => Some(at),
            _ => None,
        }
    };
    let take = |copied: &mut BTreeSet<EntityUid>, id: EntityUid| -> Result<(), String> {
        let e = def
            .entity(id)
            .ok_or_else(|| format!("entity {} does not exist", id.0))?;
        if special.contains_key(&id) {
            return Ok(());
        }
        copied.insert(id);
        for p in e.kind.points() {
            if !special.contains_key(&p) {
                copied.insert(p);
            }
        }
        Ok(())
    };
    for item in items {
        match item {
            Item::Entity(r) => {
                let e = def
                    .entity(r.uid())
                    .ok_or_else(|| format!("{r} does not exist"))?;
                if e.is_point() != r.is_point() {
                    return Err(format!("{r} is a {}", e.kind.type_name()));
                }
                take(&mut copied, r.uid())?;
                if base.is_none() && !special.contains_key(&e.id) {
                    let first = match &e.kind {
                        EntityKind::Point { at } => Some(*at),
                        kind => kind
                            .ends()
                            .map(|(start, _)| start)
                            .or_else(|| kind.points().first().copied())
                            .and_then(position),
                    };
                    base = first;
                }
            }
            Item::Text(t) => {
                let text = def
                    .texts
                    .iter()
                    .find(|x| x.id == *t)
                    .ok_or_else(|| format!("text t{} does not exist", t.0))?;
                texts.insert(*t);
                for p in &text.frame {
                    take(&mut copied, *p)?;
                }
                if let Some(path) = &text.path {
                    take(&mut copied, path.curve)?;
                }
                if base.is_none() {
                    base = Some(text.at);
                }
            }
            // Constraints and dimensions come with their geometry.
            Item::Constraint(_) => {}
        }
    }
    if copied.is_empty() && texts.is_empty() {
        return Err("nothing to copy: select sketch points, curves or texts".to_owned());
    }
    let mut used: BTreeSet<EntityUid> = BTreeSet::new();
    // The curves' own points that are the origin or an axis's end.
    for id in &copied {
        if let Some(e) = def.entity(*id) {
            used.extend(
                e.kind
                    .points()
                    .into_iter()
                    .filter(|p| special.contains_key(p)),
            );
        }
    }
    let mut left_out = 0;
    // Whether a constraint or a dimension on `refs` is kept: it references
    // copied geometry, and nothing else but the origin and the axes.
    let mut keeps = |refs: &[Ref], used: &mut BTreeSet<EntityUid>| -> bool {
        if !refs.iter().any(|r| copied.contains(&r.uid())) {
            return false;
        }
        if refs
            .iter()
            .all(|r| copied.contains(&r.uid()) || special.contains_key(&r.uid()))
        {
            used.extend(
                refs.iter()
                    .map(|r| r.uid())
                    .filter(|u| special.contains_key(u)),
            );
            true
        } else {
            left_out += 1;
            false
        }
    };
    let mut constraints = Vec::new();
    for c in &def.constraints {
        if keeps(&c.kind.refs(), &mut used) {
            constraints.push(c.clone());
        }
    }
    let kept: BTreeSet<ConstraintUid> = constraints.iter().map(|c| c.id).collect();
    let mut dimensions = Vec::new();
    for d in &def.dimensions {
        let keep = match d.kind {
            // The common spacing goes with its equal spacing.
            DimensionKind::Spacing { constraint } => kept.contains(&constraint),
            _ => keeps(&d.kind.refs(), &mut used),
        };
        if !keep {
            continue;
        }
        let value = d.value.map(|id| {
            let (expression, parameters) = value_of(params, owner, id);
            FragmentValue {
                value: params.value(id).unwrap_or(f64::NAN),
                expression,
                parameters,
            }
        });
        dimensions.push(Dimension {
            id: d.id,
            kind: d.kind.clone(),
            value,
            reference: None,
            text: d.text,
            active: d.active,
            inactive_reason: d.inactive_reason.clone(),
        });
    }
    let entities: Vec<Entity> = def
        .entities
        .iter()
        .filter(|e| copied.contains(&e.id))
        .map(|e| {
            let mut e = e.clone();
            // Projected geometry becomes ordinary, free geometry.
            if e.reference {
                e.reference = false;
                e.fixed = false;
            }
            e.axis = None;
            e
        })
        .collect();
    let text_values = def
        .text_values
        .iter()
        .filter(|v| texts.contains(&v.text))
        .filter_map(|v| {
            let (expression, parameters) = value_of(params, owner, v.value);
            // A text expression of no parameters is the text itself.
            Some(FragmentTextValue {
                text: v.text,
                expression: expression?,
                parameters,
            })
        })
        .collect();
    let references: Vec<FragmentReference> = used
        .iter()
        .map(|id| FragmentReference {
            id: def.entity(*id).expect("a special entity").as_ref(),
            is: special[id],
        })
        .collect();
    let base = base.unwrap_or_default();
    Ok(Fragment {
        version: FRAGMENT_VERSION,
        base,
        entities,
        texts: def
            .texts
            .iter()
            .filter(|t| texts.contains(&t.id))
            .cloned()
            .collect(),
        constraints,
        dimensions,
        text_values,
        references,
        left_out,
    })
}

/// The directions, `[x, y]`, in which a fragment's active constraints and
/// dimensions hold it to the sketch's origin and axes: a paste cannot move
/// it along them, since the solve pulls it back. A point at the origin
/// (or an axis's end) holds both, an axis's line the direction across it;
/// a horizontal or vertical distance or alignment with the origin holds
/// one, and parallels, perpendiculars, angles and sizes hold none.
pub fn held(fragment: &Fragment) -> [bool; 2] {
    let across = |id: EntityUid| -> [bool; 2] {
        match fragment.references.iter().find(|r| r.id.uid() == id) {
            Some(r) if r.is == SketchReference::X => [false, true],
            Some(r) if r.is == SketchReference::Y => [true, false],
            Some(_) => [true, true],
            None => [false, false],
        }
    };
    let all = |refs: &[Ref]| {
        refs.iter().fold([false, false], |h, r| {
            let a = across(r.uid());
            [h[0] || a[0], h[1] || a[1]]
        })
    };
    // Held along one direction when any of `refs` is a reference.
    let one = |refs: &[Ref], x: bool| {
        let a = all(refs);
        if a[0] || a[1] {
            [x, !x]
        } else {
            [false, false]
        }
    };
    let mut held = [false, false];
    let mut add = |h: [bool; 2]| {
        held[0] |= h[0];
        held[1] |= h[1];
    };
    // A curve on the origin point or an axis's end.
    for e in &fragment.entities {
        let points: Vec<Ref> = e.kind.points().into_iter().map(Ref::Point).collect();
        add(all(&points));
    }
    use ConstraintKind as K;
    for c in fragment.constraints.iter().filter(|c| c.active) {
        add(match &c.kind {
            K::HorizontalPoints { .. } => one(&c.kind.refs(), false),
            K::VerticalPoints { .. } => one(&c.kind.refs(), true),
            K::Horizontal { .. }
            | K::Vertical { .. }
            | K::Parallel { .. }
            | K::Perpendicular { .. }
            | K::Smooth { .. }
            | K::Equal { .. }
            | K::Concentric { .. } => [false, false],
            K::EqualSpacing { anchors, .. } => all(anchors),
            kind => all(&kind.refs()),
        });
    }
    use DimensionKind as D;
    for d in fragment.dimensions.iter().filter(|d| d.active) {
        add(match &d.kind {
            D::HorizontalDistance { .. } => one(&d.kind.refs(), true),
            D::VerticalDistance { .. } => one(&d.kind.refs(), false),
            D::Length { .. }
            | D::Angle { .. }
            | D::Radius { .. }
            | D::Diameter { .. }
            | D::ArcLength { .. }
            | D::MajorRadius { .. }
            | D::MinorRadius { .. }
            | D::ConcentricDistance { .. }
            | D::Spacing { .. } => [false, false],
            kind => all(&kind.refs()),
        });
    }
    held
}

/// What [`SketchEdit::paste`] made.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pasted {
    /// The new points and curves (`p<n>`, `c<n>`) and texts (`t<n>`), in
    /// the fragment's order.
    pub made: Vec<String>,
    /// The constraints and dimensions added.
    pub kept: usize,
    /// The constraints and dimensions left out: the fragment's, and any it
    /// holds on entities it lacks.
    pub left_out: usize,
    /// Dimensions and texts that took their value instead of their
    /// expression: a parameter it uses is not in the design.
    pub values: Vec<String>,
}

/// An entity's kind with its points replaced at once by `map`.
fn mapped(kind: &EntityKind, map: &BTreeMap<EntityUid, EntityUid>) -> EntityKind {
    let m = |p: &EntityUid| map.get(p).copied().unwrap_or(*p);
    let mut kind = kind.clone();
    match &mut kind {
        EntityKind::Point { .. } => {}
        EntityKind::Line { start, end, .. } => {
            *start = m(start);
            *end = m(end);
        }
        EntityKind::Circle { center, .. } => *center = m(center),
        EntityKind::Arc { center, start, end } => {
            *center = m(center);
            *start = m(start);
            *end = m(end);
        }
        EntityKind::Ellipse { center, major, .. } => {
            *center = m(center);
            *major = m(major);
        }
        EntityKind::EllipticalArc {
            center,
            major,
            start,
            end,
            ..
        } => {
            *center = m(center);
            *major = m(major);
            *start = m(start);
            *end = m(end);
        }
        EntityKind::Spline {
            control: points, ..
        }
        | EntityKind::FittedSpline { points, .. } => {
            for p in points.iter_mut() {
                *p = m(p);
            }
        }
    }
    kind
}

impl SketchEdit<'_> {
    /// The target's entity that stands for a source's origin or axis, made
    /// when the sketch has none.
    fn reference_entity(&mut self, is: SketchReference) -> Result<EntityUid, String> {
        let Some(axis) = is.axis() else {
            return Ok(self.origin());
        };
        let line = self.axis(axis);
        if matches!(is, SketchReference::X | SketchReference::Y) {
            return Ok(line);
        }
        match self.def.entity(line).map(|e| &e.kind) {
            Some(EntityKind::Line { end, .. }) => Ok(*end),
            _ => Err("the sketch's axis is not a line".to_owned()),
        }
    }

    /// Adds a fragment moved by `by` (see the module documentation): its
    /// entities and texts with new ids, its constraints and dimensions on
    /// them and on this sketch's origin and axes.
    pub fn paste(&mut self, fragment: &Fragment, by: P2) -> Result<Pasted, String> {
        if fragment.version > FRAGMENT_VERSION {
            return Err(format!(
                "the copied geometry is of a newer version of MitCad (fragment version {})",
                fragment.version
            ));
        }
        if !by.iter().all(|v| v.is_finite()) {
            return Err(format!("the place must be finite, got {by:?}"));
        }
        if fragment.entities.is_empty() && fragment.texts.is_empty() {
            return Err("there is nothing to paste".to_owned());
        }
        // The source's ids, and what each stood for.
        let mut known: BTreeMap<EntityUid, bool> = BTreeMap::new();
        for e in &fragment.entities {
            if known.insert(e.id, e.is_point()).is_some() {
                return Err(format!("{} is in the copied geometry twice", e.as_ref()));
            }
        }
        for r in &fragment.references {
            if known.insert(r.id.uid(), r.id.is_point()).is_some() {
                return Err(format!("{} is in the copied geometry twice", r.id));
            }
        }
        for e in &fragment.entities {
            if let EntityKind::Point { at } = e.kind
                && !at.iter().all(|v| v.is_finite())
            {
                return Err(format!("{} is not at a finite place", e.as_ref()));
            }
            for p in e.kind.points() {
                if known.get(&p) != Some(&true) {
                    return Err(format!(
                        "{} uses p{}, which the copied geometry lacks",
                        e.as_ref(),
                        p.0
                    ));
                }
            }
        }
        let mut map: BTreeMap<EntityUid, EntityUid> = BTreeMap::new();
        for r in &fragment.references {
            map.insert(r.id.uid(), self.reference_entity(r.is)?);
        }
        let ids = self.ids(fragment.entities.len());
        for (e, id) in fragment.entities.iter().zip(&ids) {
            map.insert(e.id, *id);
        }
        let mut out = Pasted::default();
        for (e, id) in fragment.entities.iter().zip(ids) {
            let kind = match e.kind {
                EntityKind::Point { at } => EntityKind::Point { at: add(at, by) },
                ref kind => mapped(kind, &map),
            };
            let mut made = Entity::new(id, kind);
            made.construction = e.construction;
            made.fixed = e.fixed && !e.reference;
            out.made.push(made.as_ref().to_string());
            self.push(made);
        }
        let mut texts: BTreeMap<EntityUid, EntityUid> = BTreeMap::new();
        for t in &fragment.texts {
            let mut text = t.clone();
            text.id = self.id();
            text.at = add(t.at, by);
            for p in &mut text.frame {
                *p = *map
                    .get(p)
                    .ok_or_else(|| format!("text t{} has a frame point it lacks", t.id.0))?;
            }
            if let Some(path) = &mut text.path {
                path.curve = *map
                    .get(&path.curve)
                    .ok_or_else(|| format!("text t{} runs along a curve it lacks", t.id.0))?;
            }
            texts.insert(t.id, text.id);
            out.made.push(format!("t{}", text.id.0));
            self.report.texts.push(text.id);
            self.def.texts.push(text);
        }
        let mapped_all = |refs: &[Ref]| refs.iter().all(|r| map.contains_key(&r.uid()));
        let mut constraints: BTreeMap<ConstraintUid, ConstraintUid> = BTreeMap::new();
        out.left_out = fragment.left_out;
        for c in &fragment.constraints {
            if !mapped_all(&c.kind.refs()) {
                out.left_out += 1;
                continue;
            }
            let mut kind = c.kind.clone();
            remap(&mut kind, &map);
            let id = self.add_constraint(kind);
            // An inactive one stays inactive (mitcad#721).
            if !c.active
                && let Some(made) = self.def.constraints.iter_mut().find(|k| k.id == id)
            {
                made.active = false;
                made.inactive_reason = c.inactive_reason.clone();
            }
            constraints.insert(c.id, id);
            out.kept += 1;
        }
        for d in &fragment.dimensions {
            let mut kind = d.kind.clone();
            if let DimensionKind::Spacing { constraint } = &mut kind {
                // The common spacing goes with its equal spacing.
                let Some(made) = constraints.get(constraint) else {
                    out.left_out += 1;
                    continue;
                };
                *constraint = *made;
            } else if mapped_all(&kind.refs()) {
                remap_dimension(&mut kind, &map);
            } else {
                out.left_out += 1;
                continue;
            }
            let id = match &d.value {
                None => self.add_dimension(kind, None, true, None)?,
                Some(v) => self.paste_value(kind, v, d.id, &mut out.values)?,
            };
            if let Some(text) = d.text {
                self.set_text_position(id, add(text, by))?;
            }
            if !d.active
                && let Some(made) = self.def.dimensions.iter_mut().find(|k| k.id == id)
            {
                made.active = false;
                made.inactive_reason = d.inactive_reason.clone();
            }
            out.kept += 1;
        }
        for v in &fragment.text_values {
            let Some(text) = texts.get(&v.text).copied() else {
                continue;
            };
            let exists = v.parameters.iter().all(|p| self.params.find(p).is_some());
            let followed = exists
                && self
                    .set_text_value(text, Some(&ValueInput::Name(v.expression.clone())))
                    .is_ok();
            if !followed {
                out.values.push(format!("t{}", text.0));
            }
        }
        Ok(out)
    }

    /// A pasted driving dimension: with its expression where the design
    /// has every parameter it uses, else with its value (`source` names it
    /// in `values` then).
    fn paste_value(
        &mut self,
        kind: DimensionKind,
        value: &FragmentValue,
        source: ConstraintUid,
        values: &mut Vec<String>,
    ) -> Result<ConstraintUid, String> {
        if let Some(expression) = &value.expression {
            if value
                .parameters
                .iter()
                .all(|p| self.params.find(p).is_some())
                && let Ok(id) = self.add_dimension(
                    kind.clone(),
                    Some(&ValueInput::Name(expression.clone())),
                    false,
                    None,
                )
            {
                return Ok(id);
            }
            values.push(source.to_string());
        }
        if !value.value.is_finite() {
            return Err(format!("dimension {source} has no value"));
        }
        self.add_dimension(kind, Some(&ValueInput::Number(value.value)), false, None)
    }
}
