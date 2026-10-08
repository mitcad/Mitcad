// SPDX-License-Identifier: MIT
//! Comparison of two designs (P12c): what differs between two documents or
//! project files, in the model's terms rather than as lines of JSON, for
//! the version history (`mitcad-cli diff`, the Version History window).
//! The two designs are snapshots of whole documents (two files, or two
//! versions of one); nothing is computed unless the geometry is asked for.
//!
//! - **Parameters** by name: added, deleted, renamed (a feature value that
//!   used one parameter uses the other, or a user parameter with the same
//!   expression, unit and comment has another name), and changed
//!   expressions (references to renamed parameters left out), values (also
//!   those that follow other parameters), units, comments, kinds, owners
//!   and favourites.
//! - **Timeline** by feature uid: added, deleted, moved (the features that
//!   do not keep their order with the others: those outside a longest
//!   sequence that does) and modified: the name, suppression, component
//!   and visibility; the definition field by field in its file form
//!   (`extent.distance`, `entities[c3].start`, parameter renames left out);
//!   and the values of its parameter slots (`Extrude1: distance 10 mm ->
//!   15 mm`). A sketch's entities, constraints and dimensions are compared
//!   by id: their counts, those added, deleted and changed, and the
//!   entities of which only the solved position or size changed; its
//!   dimensions' values are slot values.
//! - **Document:** units, the timeline marker, the active component, the
//!   render settings (mitcad#47, by field: `render.environment.preset`).
//! - **Components** and **occurrences** by uid (placements as moves and
//!   turns), **bodies** (names and attributes) by uid, **timeline
//!   groups**, **named views** and **analyses** by name, the document's
//!   **appearances** by id.
//! - **Geometry** (optional, [`DesignDiff::add_geometry`]): the volume and
//!   area of each body of two computed documents.
//!
//! The display state (the Origin folder, Isolate) is per user and not
//! compared. Base features' B-rep data is compared by SHA-256, so the
//! references of version 3 files need no store. Every change has a `text`
//! for people; [`DesignDiff::to_text`] lists them by section.

#[cfg(test)]
mod tests;
mod text;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ops::Range;

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::assembly::{Assembly, matrix_rows};
use crate::document::{DocState, Document};
use crate::expr::{DEFAULT_DECIMALS, format_number};
use crate::features::FeatureDef;
use crate::file::{FileError, load_state};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::transform::Transform;

/// How an item differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Only in the second design (`to`).
    Added,
    /// Only in the first (`from`).
    Deleted,
    /// In both, changed.
    Modified,
    /// A feature in both whose only change is its place in the timeline.
    Moved,
}

/// A field that differs, in its file form.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldChange {
    /// Its path: `extent.distance`, `entities[c3].start` (an array's items
    /// by their id, or by index), `suppressed`, `units.length`.
    pub field: String,
    /// Its value in each design; null where it is not there.
    pub from: Value,
    pub to: Value,
    /// For people: `extent.distance d3 -> d7`, `suppressed`.
    pub text: String,
}

/// A parameter as one design has it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParameterState {
    pub expression: String,
    /// `""` for unitless.
    pub unit: String,
    /// Millimetres or radians.
    pub value: f64,
    /// The value in its unit: `12.5 mm`.
    pub text: String,
    pub comment: String,
    /// `user` or `model`.
    pub kind: &'static str,
    /// The feature whose dimension a model parameter is.
    pub owner: Option<FeatureUid>,
    pub favorite: bool,
}

/// A parameter that differs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParameterChange {
    pub kind: Kind,
    /// Its name in `to` (in `from` when deleted).
    pub name: String,
    /// Its name in `from` when it was renamed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed_from: Option<String>,
    /// The feature whose dimension it is, and that feature's name.
    pub owner: Option<FeatureUid>,
    pub owner_name: Option<String>,
    pub from: Option<ParameterState>,
    pub to: Option<ParameterState>,
    /// What differs: `name`, `expression`, `unit`, `value`, `comment`,
    /// `kind`, `owner`, `favorite`.
    pub fields: Vec<&'static str>,
    pub text: String,
}

/// A feature value (a slot bound to a parameter) whose value differs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValueChange {
    /// The slot (`extent.distance`, `dimensions[k5].length`) and a short
    /// label for it (`distance`, `k5 length`).
    pub slot: String,
    pub label: String,
    /// Its parameter in each design (the same unless renamed or replaced).
    pub from_parameter: String,
    pub to_parameter: String,
    /// Millimetres or radians, and as text in the parameter's unit.
    pub from: f64,
    pub to: f64,
    pub from_text: String,
    pub to_text: String,
    /// `distance 10 mm -> 15 mm`.
    pub text: String,
}

/// A sketch's entities, constraints or dimensions in both designs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SketchItems {
    /// How many each design has.
    pub from: usize,
    pub to: usize,
    /// The ids of those only in `to`, only in `from`, and changed in
    /// themselves: not counting an entity's solved position or size, nor
    /// where a dimension's value is shown.
    pub added: Vec<String>,
    pub deleted: Vec<String>,
    pub modified: Vec<String>,
}

impl SketchItems {
    fn is_empty(&self) -> bool {
        self.from == self.to
            && self.added.is_empty()
            && self.deleted.is_empty()
            && self.modified.is_empty()
    }
}

/// What differs in a sketch's entities, constraints and dimensions.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SketchChange {
    pub entities: SketchItems,
    pub constraints: SketchItems,
    pub dimensions: SketchItems,
    /// Entities of which only the solved position or size changed (they
    /// follow the dimensions).
    pub moved: Vec<String>,
}

impl SketchChange {
    fn is_empty(&self) -> bool {
        self.entities.is_empty()
            && self.constraints.is_empty()
            && self.dimensions.is_empty()
            && self.moved.is_empty()
    }
}

/// A feature that differs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeatureChange {
    pub kind: Kind,
    pub uid: FeatureUid,
    /// Its name in `to` (in `from` when deleted).
    pub name: String,
    #[serde(rename = "type")]
    pub feature_type: &'static str,
    /// Its place in each timeline (0 the first).
    pub from_index: Option<usize>,
    pub to_index: Option<usize>,
    /// Whether it no longer keeps its order with the other features of
    /// both designs.
    pub moved: bool,
    /// Its entry's and definition's fields.
    pub fields: Vec<FieldChange>,
    /// Its values whose parameters' values differ.
    pub values: Vec<ValueChange>,
    /// For a sketch: what differs in its entities, constraints and
    /// dimensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sketch: Option<SketchChange>,
    /// `Extrude1 (F2): distance 10 mm -> 15 mm`.
    pub text: String,
}

/// A component, occurrence, body, timeline group or named view that
/// differs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemChange {
    pub kind: Kind,
    /// Its id (`C2`, `O1`, `F2.b0`); None for items known by name (timeline
    /// groups, named views).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    /// Its name in `to` (in `from` when deleted).
    pub name: String,
    pub fields: Vec<FieldChange>,
    pub text: String,
}

/// A measured quantity in each design.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Measure {
    pub from: Option<f64>,
    pub to: Option<f64>,
    /// `to - from`, and that relative to `from`.
    pub change: Option<f64>,
    pub relative: Option<f64>,
}

impl Measure {
    fn new(from: Option<f64>, to: Option<f64>) -> Self {
        let change = from.zip(to).map(|(a, b)| b - a);
        let relative = from
            .zip(change)
            .filter(|(a, _)| *a != 0.0)
            .map(|(a, c)| c / a);
        Self {
            from,
            to,
            change,
            relative,
        }
    }

    fn differs(&self) -> bool {
        match (self.from, self.to) {
            (Some(a), Some(b)) => !close(a, b, 1e-9),
            (None, None) => false,
            _ => true,
        }
    }
}

/// A body whose volume or area differs, or that only one design has.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BodyGeometry {
    pub kind: Kind,
    pub uid: BodyUid,
    pub name: String,
    /// Cubic millimetres.
    pub volume: Measure,
    /// Square millimetres.
    pub area: Measure,
    pub text: String,
}

/// The volumes and areas of the bodies at the timeline marker of two
/// computed documents (each in its component's coordinates).
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct GeometryDiff {
    pub bodies: Vec<BodyGeometry>,
    /// Over all bodies measured.
    pub volume: Measure,
    pub area: Measure,
    /// Bodies whose properties could not be measured.
    pub errors: Vec<String>,
}

/// What differs between two designs, `from` and `to`.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct DesignDiff {
    /// Nothing differs.
    pub identical: bool,
    /// One line for people: `d3 20 mm -> 25 mm, +1 feature`.
    pub summary: String,
    /// Units, the timeline marker, the active component.
    pub document: Vec<FieldChange>,
    pub parameters: Vec<ParameterChange>,
    /// In `to`'s timeline order, deleted features where they were.
    pub features: Vec<FeatureChange>,
    pub components: Vec<ItemChange>,
    pub occurrences: Vec<ItemChange>,
    pub bodies: Vec<ItemChange>,
    pub groups: Vec<ItemChange>,
    pub views: Vec<ItemChange>,
    /// Analyses kept in the document (mitcad#41), by name.
    pub analyses: Vec<ItemChange>,
    /// The document's own appearances (mitcad#46), by id.
    pub appearances: Vec<ItemChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<GeometryDiff>,
}

/// The definition state of a project file (version 1, 2 or 3) for
/// [`diff_states`]; B-rep references are not resolved (they need not be).
pub fn read_design(json: &str) -> Result<DocState, FileError> {
    load_state(json).map(|(state, _)| state)
}

/// Compares two project files' texts without computing them.
pub fn diff_files(from: &str, to: &str) -> Result<DesignDiff, FileError> {
    Ok(diff_states(&read_design(from)?, &read_design(to)?))
}

/// Compares two documents' definitions (what their project files save,
/// without the display state). Nothing is computed; see
/// [`DesignDiff::add_geometry`] for the bodies.
pub fn diff_documents<K: Kernel>(from: &Document<K>, to: &Document<K>) -> DesignDiff {
    diff_states(from.state(), to.state())
}

/// Compares two definition states.
pub fn diff_states(from: &DocState, to: &DocState) -> DesignDiff {
    let a = Side::new(from);
    let b = Side::new(to);
    let renames = renames(&a, &b);
    let a_features = features(from, &|name| renames.get(&name).cloned().unwrap_or(name));
    let b_features = features(to, &|name| name);
    let mut diff = DesignDiff {
        document: document_changes(from, to),
        parameters: parameter_changes(&a, &b, &renames),
        features: feature_changes(&a, &b, &a_features, &b_features),
        components: component_changes(&from.assembly, &to.assembly),
        occurrences: occurrence_changes(&from.assembly, &to.assembly),
        bodies: body_changes(from, to),
        groups: group_changes(from, to),
        views: view_changes(from, to),
        analyses: analysis_changes(from, to),
        appearances: appearance_changes(from, to),
        ..DesignDiff::default()
    };
    diff.finish();
    diff
}

impl DesignDiff {
    /// Adds the volume and area of the bodies at the timeline marker of
    /// the two documents as last computed (the caller recomputes them).
    pub fn add_geometry<K: Kernel>(&mut self, from: &Document<K>, to: &Document<K>) {
        let mut errors = Vec::new();
        let a = measure(from, "from", &mut errors);
        let b = measure(to, "to", &mut errors);
        let uids: BTreeSet<BodyUid> = a.keys().chain(b.keys()).copied().collect();
        let mut bodies = Vec::new();
        let total = |side: &Measured, area: bool| {
            let values: Vec<f64> = side
                .values()
                .filter_map(|(_, m)| m.map(|(v, a)| if area { a } else { v }))
                .collect();
            (!values.is_empty() || side.is_empty()).then(|| values.iter().sum())
        };
        for uid in uids {
            let (ma, mb) = (a.get(&uid), b.get(&uid));
            let kind = match (ma, mb) {
                (None, _) => Kind::Added,
                (_, None) => Kind::Deleted,
                _ => Kind::Modified,
            };
            let value = |m: Option<&(String, Option<(f64, f64)>)>, area: bool| {
                m.and_then(|(_, p)| p.map(|(v, a)| if area { a } else { v }))
            };
            let volume = Measure::new(value(ma, false), value(mb, false));
            let area = Measure::new(value(ma, true), value(mb, true));
            if kind == Kind::Modified && !volume.differs() && !area.differs() {
                continue;
            }
            let name = mb.or(ma).map(|(n, _)| n.clone()).unwrap_or_default();
            let text = text::body_geometry(kind, &uid, &name, &volume, &area);
            bodies.push(BodyGeometry {
                kind,
                uid,
                name,
                volume,
                area,
                text,
            });
        }
        self.geometry = Some(GeometryDiff {
            bodies,
            volume: Measure::new(total(&a, false), total(&b, false)),
            area: Measure::new(total(&a, true), total(&b, true)),
            errors,
        });
        self.finish();
    }

    /// The differences as JSON (this structure's serde form).
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("a comparison serializes")
    }

    /// Sets `identical` and the summary from the lists.
    fn finish(&mut self) {
        self.identical = self.document.is_empty()
            && self.parameters.is_empty()
            && self.features.is_empty()
            && self.components.is_empty()
            && self.occurrences.is_empty()
            && self.bodies.is_empty()
            && self.groups.is_empty()
            && self.views.is_empty()
            && self.analyses.is_empty()
            && self.appearances.is_empty()
            && self
                .geometry
                .as_ref()
                .is_none_or(|g| g.bodies.is_empty() && g.errors.is_empty());
        self.summary = text::summary(self);
    }
}

/// Each body's name and its volume and area, by uid.
type Measured = BTreeMap<BodyUid, (String, Option<(f64, f64)>)>;

/// The volume and area of each body at the marker, by uid, with its name;
/// None where the kernel could not measure it (the error goes to
/// `errors`).
fn measure<K: Kernel>(doc: &Document<K>, side: &str, errors: &mut Vec<String>) -> Measured {
    doc.bodies()
        .into_iter()
        .map(|body| {
            let properties = match doc.kernel().mass_properties(body.shape) {
                Ok(mass) => Some((mass.volume, mass.area)),
                Err(e) => {
                    errors.push(format!("{side}: {} ({}): {e}", body.name, body.uid));
                    None
                }
            };
            (body.uid, (body.name, properties))
        })
        .collect()
}

/// Whether two numbers are the same within a relative tolerance (and
/// absolutely near zero).
fn close(a: f64, b: f64, tolerance: f64) -> bool {
    a == b || (a - b).abs() <= tolerance * a.abs().max(b.abs()).max(1.0)
}

/// Whether two JSON values are the same, numbers within rounding.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => close(x, y, 1e-12),
            _ => x == y,
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(key, value)| y.get(key).is_some_and(|other| same(value, other)))
        }
        _ => a == b,
    }
}

// The two designs.

/// A parameter as compared.
struct Param {
    name: String,
    /// The references in its expression, for renames.
    references: Vec<(String, Range<usize>)>,
    state: ParameterState,
}

/// A design's parameters and the parameters of its features' value slots.
struct Side<'a> {
    state: &'a DocState,
    params: Vec<Param>,
    by_name: HashMap<String, usize>,
    /// For each feature, the parameter of each value slot.
    slots: HashMap<FeatureUid, BTreeMap<String, String>>,
}

impl<'a> Side<'a> {
    fn new(state: &'a DocState) -> Self {
        let parameters = &state.parameters;
        let params: Vec<Param> = parameters
            .iter()
            .map(|p| Param {
                name: p.name().to_owned(),
                references: p
                    .expr()
                    .reference_spans()
                    .into_iter()
                    .map(|(name, span)| (name.to_owned(), span.start..span.end))
                    .collect(),
                state: ParameterState {
                    expression: p.expression().to_owned(),
                    unit: p.unit().to_string(),
                    value: p.value(),
                    text: p.formatted_value(DEFAULT_DECIMALS),
                    comment: p.comment().to_owned(),
                    kind: p.kind().as_str(),
                    owner: parameters.owner(p.id()),
                    favorite: state.favorites.contains(&p.id()),
                },
            })
            .collect();
        let by_name = params
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.clone(), i))
            .collect();
        let slots = state
            .features
            .iter()
            .map(|entry| {
                let mut slots = BTreeMap::new();
                let _ = entry.def.map_params(&mut |slot, id| {
                    slots.insert(slot.to_owned(), parameters.name(*id));
                    Ok::<_, ()>(())
                });
                (entry.uid, slots)
            })
            .collect();
        Self {
            state,
            params,
            by_name,
            slots,
        }
    }

    fn param(&self, name: &str) -> Option<&Param> {
        self.by_name.get(name).map(|i| &self.params[*i])
    }

    fn feature_name(&self, uid: FeatureUid) -> Option<String> {
        self.state.entry(uid).map(|f| f.name.clone())
    }
}

/// The parameters of `a` that `b` has under another name: name in `a` ->
/// name in `b`. A pair is a rename when the same slot of a feature in both
/// uses them (and nothing pairs either with another), or else when they
/// are the only user parameters with the same expression, unit, comment and
/// owner among those only one design has.
fn renames(a: &Side, b: &Side) -> HashMap<String, String> {
    let only_a: HashSet<&str> = a
        .params
        .iter()
        .map(|p| p.name.as_str())
        .filter(|n| !b.by_name.contains_key(*n))
        .collect();
    let only_b: HashSet<&str> = b
        .params
        .iter()
        .map(|p| p.name.as_str())
        .filter(|n| !a.by_name.contains_key(*n))
        .collect();
    let mut pairs: BTreeSet<(&str, &str)> = BTreeSet::new();
    for (uid, a_slots) in &a.slots {
        let Some(b_slots) = b.slots.get(uid) else {
            continue;
        };
        for (slot, x) in a_slots {
            if let Some(y) = b_slots.get(slot)
                && only_a.contains(x.as_str())
                && only_b.contains(y.as_str())
            {
                pairs.insert((x, y));
            }
        }
    }
    let unique = |pairs: &BTreeSet<(&str, &str)>, x: &str, y: &str| {
        pairs.iter().filter(|(p, _)| *p == x).count() == 1
            && pairs.iter().filter(|(_, q)| *q == y).count() == 1
    };
    let mut out: HashMap<String, String> = pairs
        .iter()
        .filter(|(x, y)| unique(&pairs, x, y))
        .map(|(x, y)| ((*x).to_owned(), (*y).to_owned()))
        .collect();
    let taken: HashSet<String> = out.values().cloned().collect();
    let same_user = |p: &Param, q: &Param| {
        p.state.kind == "user"
            && q.state.kind == "user"
            && p.state.expression == q.state.expression
            && p.state.unit == q.state.unit
            && p.state.comment == q.state.comment
            && p.state.owner == q.state.owner
    };
    let free_a: Vec<&Param> = a
        .params
        .iter()
        .filter(|p| only_a.contains(p.name.as_str()) && !out.contains_key(&p.name))
        .collect();
    let free_b: Vec<&Param> = b
        .params
        .iter()
        .filter(|q| only_b.contains(q.name.as_str()) && !taken.contains(&q.name))
        .collect();
    for p in &free_a {
        let matches: Vec<&&Param> = free_b.iter().filter(|q| same_user(p, q)).collect();
        if let [q] = matches.as_slice()
            && free_a.iter().filter(|other| same_user(other, q)).count() == 1
        {
            out.insert(p.name.clone(), q.name.clone());
        }
    }
    out
}

/// `p`'s expression with the references to renamed parameters under their
/// new names.
fn renamed_expression(p: &Param, renames: &HashMap<String, String>) -> String {
    let expression = &p.state.expression;
    let mut out = String::with_capacity(expression.len());
    let mut copied = 0;
    for (name, range) in &p.references {
        if let Some(new) = renames.get(name) {
            out.push_str(&expression[copied..range.start]);
            out.push_str(new);
            copied = range.end;
        }
    }
    out.push_str(&expression[copied..]);
    out
}

/// An item's index in each of two lists.
type Pair = (Option<usize>, Option<usize>);

/// The items of two lists in one order: `b`'s, each item only `a` has
/// placed before the next item of `a` that `b` has too. `matched[i]` is
/// the index in `b` of `a`'s item `i`; the pairs are (index in `a`, index
/// in `b`).
fn merged_order(matched: &[Option<usize>], b_len: usize) -> Vec<Pair> {
    let mut b_to_a = vec![None; b_len];
    for (i, j) in matched.iter().enumerate() {
        if let Some(j) = j {
            b_to_a[*j] = Some(i);
        }
    }
    let mut keyed: Vec<((usize, usize, usize), Pair)> = (0..b_len)
        .map(|j| ((j, 1, 0), (b_to_a[j], Some(j))))
        .collect();
    let mut next = b_len;
    for i in (0..matched.len()).rev() {
        match matched[i] {
            Some(j) => next = j,
            None => keyed.push(((next, 0, i), (Some(i), None))),
        }
    }
    keyed.sort_by_key(|(key, _)| *key);
    keyed.into_iter().map(|(_, pair)| pair).collect()
}

/// The indices of a longest strictly increasing subsequence of `values`.
fn longest_increasing(values: &[usize]) -> Vec<usize> {
    let mut tails: Vec<usize> = Vec::new();
    let mut previous: Vec<Option<usize>> = vec![None; values.len()];
    for (i, &value) in values.iter().enumerate() {
        let at = tails.partition_point(|&t| values[t] < value);
        if at > 0 {
            previous[i] = Some(tails[at - 1]);
        }
        if at == tails.len() {
            tails.push(i);
        } else {
            tails[at] = i;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(i) = at {
        out.push(i);
        at = previous[i];
    }
    out.reverse();
    out
}

// Parameters.

fn parameter_changes(
    a: &Side,
    b: &Side,
    renames: &HashMap<String, String>,
) -> Vec<ParameterChange> {
    let reverse: HashMap<&str, &str> = renames
        .iter()
        .map(|(x, y)| (y.as_str(), x.as_str()))
        .collect();
    // A name that went with a deleted feature's dimension and came again
    // with a new feature's (`d6`) is two parameters.
    let mut matched = vec![None; a.params.len()];
    for (j, q) in b.params.iter().enumerate() {
        let name = reverse.get(q.name.as_str()).copied().unwrap_or(&q.name);
        if let Some(i) = a.by_name.get(name) {
            let owners = (a.params[*i].state.owner, q.state.owner);
            if !matches!(owners, (Some(x), Some(y)) if x != y) {
                matched[*i] = Some(j);
            }
        }
    }
    let owner_name = |side: &Side, p: &Param| p.state.owner.and_then(|o| side.feature_name(o));
    let mut out = Vec::new();
    for pair in merged_order(&matched, b.params.len()) {
        let change = match pair {
            (None, Some(j)) => {
                let q = &b.params[j];
                ParameterChange {
                    kind: Kind::Added,
                    name: q.name.clone(),
                    renamed_from: None,
                    owner: q.state.owner,
                    owner_name: owner_name(b, q),
                    from: None,
                    to: Some(q.state.clone()),
                    fields: Vec::new(),
                    text: text::parameter_line(&q.name, &q.state, "added"),
                }
            }
            (Some(i), None) => {
                let p = &a.params[i];
                ParameterChange {
                    kind: Kind::Deleted,
                    name: p.name.clone(),
                    renamed_from: None,
                    owner: p.state.owner,
                    owner_name: owner_name(a, p),
                    from: Some(p.state.clone()),
                    to: None,
                    fields: Vec::new(),
                    text: text::parameter_line(&p.name, &p.state, "deleted"),
                }
            }
            (Some(i), Some(j)) => {
                let (p, q) = (&a.params[i], &b.params[j]);
                let (x, y) = (&p.state, &q.state);
                let mut fields = Vec::new();
                if p.name != q.name {
                    fields.push("name");
                }
                if renamed_expression(p, renames) != y.expression {
                    fields.push("expression");
                }
                if x.unit != y.unit {
                    fields.push("unit");
                }
                if !close(x.value, y.value, 1e-12) {
                    fields.push("value");
                }
                if x.comment != y.comment {
                    fields.push("comment");
                }
                if x.kind != y.kind {
                    fields.push("kind");
                }
                if x.owner != y.owner {
                    fields.push("owner");
                }
                if x.favorite != y.favorite {
                    fields.push("favorite");
                }
                if fields.is_empty() {
                    continue;
                }
                let owners = (owner_name(a, p), owner_name(b, q));
                let text = text::parameter_modified(&p.name, &q.name, x, y, &fields, &owners);
                ParameterChange {
                    kind: Kind::Modified,
                    name: q.name.clone(),
                    renamed_from: (p.name != q.name).then(|| p.name.clone()),
                    owner: y.owner,
                    owner_name: owners.1,
                    from: Some(x.clone()),
                    to: Some(y.clone()),
                    fields,
                    text,
                }
            }
            (None, None) => continue,
        };
        out.push(change);
    }
    out
}

// The timeline.

/// A feature as compared.
struct Feature {
    uid: FeatureUid,
    name: String,
    type_name: &'static str,
    suppressed: bool,
    component: ComponentUid,
    visible: Option<bool>,
    /// Its definition in the file form without `type`: parameters by name
    /// (through `rename`), B-rep data by SHA-256 and size.
    def: Value,
}

fn features(state: &DocState, rename: &dyn Fn(String) -> String) -> Vec<Feature> {
    let params = &state.parameters;
    state
        .features
        .iter()
        .map(|entry| {
            let mut def = entry
                .def
                .map_params(&mut |_, id| Ok::<_, ()>(rename(params.name(*id))))
                .expect("names never fail");
            if let FeatureDef::Base(base) = &mut def {
                for body in &mut base.bodies {
                    body.brep = body.brep.to_reference();
                }
            }
            let mut value = serde_json::to_value(&def).expect("definitions serialize");
            if let Value::Object(fields) = &mut value {
                fields.remove("type");
                if let Some(Value::Array(bodies)) = fields.get_mut("bodies") {
                    for body in bodies {
                        if let Some(brep) =
                            body.get_mut("brep").filter(|b| b.get("sha256").is_some())
                        {
                            *brep = json!({"sha256": brep["sha256"], "size": brep["size"]});
                        }
                    }
                }
            }
            Feature {
                uid: entry.uid,
                name: entry.name.clone(),
                type_name: entry.def.type_name(),
                suppressed: entry.suppressed,
                component: entry.component,
                visible: state.feature_visibility.get(&entry.uid).copied(),
                def: value,
            }
        })
        .collect()
}

fn feature_changes(a: &Side, b: &Side, xs: &[Feature], ys: &[Feature]) -> Vec<FeatureChange> {
    let index: HashMap<FeatureUid, usize> =
        ys.iter().enumerate().map(|(j, f)| (f.uid, j)).collect();
    let matched: Vec<Option<usize>> = xs.iter().map(|f| index.get(&f.uid).copied()).collect();
    // The features in both that keep their order.
    let common: Vec<usize> = matched.iter().flatten().copied().collect();
    let kept: HashSet<usize> = longest_increasing(&common)
        .into_iter()
        .map(|k| common[k])
        .collect();
    let mut out = Vec::new();
    for pair in merged_order(&matched, ys.len()) {
        let change = match pair {
            (None, Some(j)) => {
                let y = &ys[j];
                let after = j.checked_sub(1).map(|k| ys[k].name.as_str());
                FeatureChange {
                    kind: Kind::Added,
                    uid: y.uid,
                    name: y.name.clone(),
                    feature_type: y.type_name,
                    from_index: None,
                    to_index: Some(j),
                    moved: false,
                    fields: Vec::new(),
                    values: Vec::new(),
                    sketch: None,
                    text: text::feature_added(y.uid, &y.name, y.type_name, after),
                }
            }
            (Some(i), None) => {
                let x = &xs[i];
                FeatureChange {
                    kind: Kind::Deleted,
                    uid: x.uid,
                    name: x.name.clone(),
                    feature_type: x.type_name,
                    from_index: Some(i),
                    to_index: None,
                    moved: false,
                    fields: Vec::new(),
                    values: Vec::new(),
                    sketch: None,
                    text: format!("{} ({}, {}): deleted", x.name, x.uid, x.type_name),
                }
            }
            (Some(i), Some(j)) => {
                let (x, y) = (&xs[i], &ys[j]);
                let moved = !kept.contains(&j);
                let mut fields = Vec::new();
                if x.name != y.name {
                    fields.push(field_change("name", json!(x.name), json!(y.name)));
                }
                if x.suppressed != y.suppressed {
                    fields.push(field_change(
                        "suppressed",
                        json!(x.suppressed),
                        json!(y.suppressed),
                    ));
                }
                if x.component != y.component {
                    let names = (
                        a.state.assembly.name(x.component),
                        b.state.assembly.name(y.component),
                    );
                    fields.push(FieldChange {
                        field: "component".to_owned(),
                        from: json!(x.component),
                        to: json!(y.component),
                        text: format!("component {} -> {}", names.0, names.1),
                    });
                }
                if x.visible != y.visible {
                    fields.push(field_change("visible", json!(x.visible), json!(y.visible)));
                }
                let mut sketch = None;
                if x.type_name != y.type_name {
                    fields.push(field_change("type", json!(x.type_name), json!(y.type_name)));
                } else if y.type_name == "sketch" {
                    sketch = sketch_change(&x.def, &y.def, &mut fields);
                } else {
                    diff_json("", &x.def, &y.def, &mut fields);
                }
                let values = value_changes(a, b, y.uid);
                if fields.is_empty() && values.is_empty() && sketch.is_none() && !moved {
                    continue;
                }
                let kind = if fields.is_empty() && values.is_empty() && sketch.is_none() {
                    Kind::Moved
                } else {
                    Kind::Modified
                };
                let after = moved.then(|| j.checked_sub(1).map(|k| ys[k].name.as_str()));
                let text = text::feature_modified(
                    y.uid,
                    &y.name,
                    &fields,
                    &values,
                    sketch.as_ref(),
                    after,
                );
                FeatureChange {
                    kind,
                    uid: y.uid,
                    name: y.name.clone(),
                    feature_type: y.type_name,
                    from_index: Some(i),
                    to_index: Some(j),
                    moved,
                    fields,
                    values,
                    sketch,
                    text,
                }
            }
            (None, None) => continue,
        };
        out.push(change);
    }
    out
}

/// The values of feature `uid`'s slots whose parameters' values differ.
fn value_changes(a: &Side, b: &Side, uid: FeatureUid) -> Vec<ValueChange> {
    let (Some(xs), Some(ys)) = (a.slots.get(&uid), b.slots.get(&uid)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (slot, y) in ys {
        let Some(x) = xs.get(slot) else {
            continue;
        };
        let (Some(p), Some(q)) = (a.param(x), b.param(y)) else {
            continue;
        };
        if close(p.state.value, q.state.value, 1e-12) {
            continue;
        }
        out.push(ValueChange {
            slot: slot.clone(),
            label: slot_label(slot),
            from_parameter: p.name.clone(),
            to_parameter: q.name.clone(),
            from: p.state.value,
            to: q.state.value,
            from_text: p.state.text.clone(),
            to_text: q.state.text.clone(),
            text: String::new(),
        });
    }
    // Labels that would name two values are the slots themselves.
    let labels: Vec<String> = out.iter().map(|v| v.label.clone()).collect();
    for value in &mut out {
        if labels.iter().filter(|l| **l == value.label).count() > 1 {
            value.label = value.slot.clone();
        }
        value.text = format!("{} {} -> {}", value.label, value.from_text, value.to_text);
    }
    out
}

/// A slot's short label: its last field, after the id of the item it is
/// in (`dimensions[k5].length` -> `k5 length`, `extent.distance` ->
/// `distance`).
fn slot_label(slot: &str) -> String {
    let field = crate::features::slot_label(slot);
    let before = &slot[..slot.len() - field.len()];
    match before
        .strip_suffix("].")
        .and_then(|rest| rest.rsplit_once('['))
    {
        Some((_, id)) => format!("{id} {field}"),
        None => field.to_owned(),
    }
}

/// Fields of sketch entities that the solver sets: positions and sizes.
const SOLVED: [&str; 3] = ["at", "radius", "minor_radius"];

/// Compares two sketches' definitions: entities, constraints and
/// dimensions by id (their other fields go to `fields`).
fn sketch_change(a: &Value, b: &Value, fields: &mut Vec<FieldChange>) -> Option<SketchChange> {
    let mut a = a.as_object().cloned().unwrap_or_default();
    let mut b = b.as_object().cloned().unwrap_or_default();
    let mut change = SketchChange::default();
    for list in ["entities", "constraints", "dimensions"] {
        let take = |fields: &mut Map<String, Value>| match fields.remove(list) {
            Some(Value::Array(items)) => items,
            _ => Vec::new(),
        };
        let (xs, ys) = (take(&mut a), take(&mut b));
        let mut items = SketchItems {
            from: xs.len(),
            to: ys.len(),
            ..SketchItems::default()
        };
        // Where a dimension's value is shown is no change of the design.
        let ignored: &[&str] = match list {
            "entities" => &SOLVED,
            "dimensions" => &["text"],
            _ => &[],
        };
        let without = |item: &Value| -> Value {
            let mut item = item.clone();
            if let Value::Object(fields) = &mut item {
                for key in ignored {
                    fields.remove(*key);
                }
            }
            item
        };
        let by_id = |items: &[Value]| -> Vec<(String, Value)> {
            items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let id = item
                        .get("id")
                        .and_then(Value::as_str)
                        .map_or_else(|| i.to_string(), str::to_owned);
                    (id, item.clone())
                })
                .collect()
        };
        let (xs, ys) = (by_id(&xs), by_id(&ys));
        let in_a: HashMap<&str, &Value> = xs.iter().map(|(id, v)| (id.as_str(), v)).collect();
        let in_b: HashSet<&str> = ys.iter().map(|(id, _)| id.as_str()).collect();
        for (id, y) in &ys {
            match in_a.get(id.as_str()) {
                None => items.added.push(id.clone()),
                Some(x) if !same(&without(x), &without(y)) => items.modified.push(id.clone()),
                Some(x) if list == "entities" && !same(x, y) => change.moved.push(id.clone()),
                Some(_) => {}
            }
        }
        for (id, _) in &xs {
            if !in_b.contains(id.as_str()) {
                items.deleted.push(id.clone());
            }
        }
        match list {
            "entities" => change.entities = items,
            "constraints" => change.constraints = items,
            _ => change.dimensions = items,
        }
    }
    diff_json("", &Value::Object(a), &Value::Object(b), fields);
    (!change.is_empty()).then_some(change)
}

/// Appends the fields that differ between two JSON values at `path`.
/// Objects are compared key by key (a tagged object whose `type` differs
/// whole); arrays of objects by their items' `id`, or index by index when
/// as long; arrays of numbers and texts whole.
fn diff_json(path: &str, a: &Value, b: &Value, out: &mut Vec<FieldChange>) {
    if same(a, b) {
        return;
    }
    let atomic = path.rsplit('.').next() == Some("brep");
    match (a, b) {
        (Value::Object(x), Value::Object(y)) if !atomic && x.get("type") == y.get("type") => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for key in keys {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                let null = Value::Null;
                diff_json(
                    &child,
                    x.get(key).unwrap_or(&null),
                    y.get(key).unwrap_or(&null),
                    out,
                );
            }
        }
        (Value::Array(x), Value::Array(y)) if !(scalars(x) && scalars(y)) => {
            match (keyed(x), keyed(y)) {
                (Some(xk), Some(yk)) => {
                    let null = Value::Null;
                    let in_a: HashMap<&str, &Value> = xk.iter().copied().collect();
                    let in_b: HashSet<&str> = yk.iter().map(|(id, _)| *id).collect();
                    for (id, item) in &yk {
                        let other = in_a.get(id).copied().unwrap_or(&null);
                        diff_json(&format!("{path}[{id}]"), other, item, out);
                    }
                    for (id, item) in &xk {
                        if !in_b.contains(id) {
                            diff_json(&format!("{path}[{id}]"), item, &null, out);
                        }
                    }
                }
                _ if x.len() == y.len() => {
                    for (i, (x, y)) in x.iter().zip(y).enumerate() {
                        diff_json(&format!("{path}[{i}]"), x, y, out);
                    }
                }
                _ => out.push(field_change(path, a.clone(), b.clone())),
            }
        }
        _ => out.push(field_change(path, a.clone(), b.clone())),
    }
}

/// Whether every item is a number, text, flag or an array of those.
fn scalars(items: &[Value]) -> bool {
    items.iter().all(|item| match item {
        Value::Array(inner) => inner.iter().all(|v| !v.is_array() && !v.is_object()),
        Value::Object(_) => false,
        _ => true,
    })
}

/// The items of an array of objects by their `id`, when every item has a
/// different one.
fn keyed(items: &[Value]) -> Option<Vec<(&str, &Value)>> {
    let mut seen = HashSet::new();
    items
        .iter()
        .map(|item| {
            let id = item.get("id")?.as_str()?;
            seen.insert(id).then_some((id, item))
        })
        .collect()
}

fn field_change(field: &str, from: Value, to: Value) -> FieldChange {
    let text = text::field(field, &from, &to);
    FieldChange {
        field: field.to_owned(),
        from,
        to,
        text,
    }
}

// The document, components and the rest.

/// Where the timeline marker is: its position, the feature before it and
/// whether it is at the end, and as text.
fn marker(state: &DocState) -> (Value, Option<FeatureUid>, bool, String) {
    let position = state.marker.min(state.features.len());
    let end = position == state.features.len();
    let after = position.checked_sub(1).map(|i| &state.features[i]);
    let text = match after {
        _ if end => "at the end".to_owned(),
        None => "at the start".to_owned(),
        Some(feature) => format!("after {}", feature.name),
    };
    let uid = after.map(|f| f.uid);
    (
        json!({"position": position, "after": uid, "end": end}),
        uid,
        end,
        text,
    )
}

fn document_changes(a: &DocState, b: &DocState) -> Vec<FieldChange> {
    let mut out = Vec::new();
    let (x, y) = (a.parameters.context(), b.parameters.context());
    for (field, from, to) in [
        (
            "units.length",
            x.default_length_unit.symbol(),
            y.default_length_unit.symbol(),
        ),
        (
            "units.angle",
            x.default_angle_unit.symbol(),
            y.default_angle_unit.symbol(),
        ),
    ] {
        if from != to {
            out.push(field_change(field, json!(from), json!(to)));
        }
    }
    let (from, after_a, end_a, text_a) = marker(a);
    let (to, after_b, end_b, text_b) = marker(b);
    if end_a != end_b || (!end_b && after_a != after_b) {
        out.push(FieldChange {
            field: "marker".to_owned(),
            from,
            to,
            text: format!("marker {text_a} -> {text_b}"),
        });
    }
    // Render settings (mitcad#47), by field.
    for (field, from, to) in a.render.differences(&b.render) {
        out.push(field_change(&format!("render.{field}"), from, to));
    }
    // The configuration table (mitcad#64), row by row.
    out.extend(configuration_changes(&a.configurations, &b.configurations));
    let (active_a, active_b) = (a.assembly.active, b.assembly.active);
    if active_a != active_b {
        out.push(FieldChange {
            field: "active_component".to_owned(),
            from: json!(active_a),
            to: json!(active_b),
            text: format!(
                "active component {} -> {}",
                a.assembly.name(active_a),
                b.assembly.name(active_b)
            ),
        });
    }
    out
}

/// Compares items by key: `from` and `to` in order with their names and
/// JSON forms; `describe` may rewrite the fields' texts.
fn item_changes<K: Eq + std::hash::Hash + Clone + ToString>(
    from: Vec<(K, String, Value)>,
    to: Vec<(K, String, Value)>,
    with_uid: bool,
    describe: &dyn Fn(&K, &mut FieldChange),
) -> Vec<ItemChange> {
    let index: HashMap<K, usize> = to
        .iter()
        .enumerate()
        .map(|(j, (key, ..))| (key.clone(), j))
        .collect();
    let matched: Vec<Option<usize>> = from
        .iter()
        .map(|(key, ..)| index.get(key).copied())
        .collect();
    let label = |key: &K, name: &str| {
        if with_uid {
            format!("{name} ({})", key.to_string())
        } else {
            name.to_owned()
        }
    };
    let mut out = Vec::new();
    for pair in merged_order(&matched, to.len()) {
        let (kind, key, name, fields) = match pair {
            (None, Some(j)) => (Kind::Added, &to[j].0, &to[j].1, Vec::new()),
            (Some(i), None) => (Kind::Deleted, &from[i].0, &from[i].1, Vec::new()),
            (Some(i), Some(j)) => {
                let mut fields = Vec::new();
                diff_json("", &from[i].2, &to[j].2, &mut fields);
                if fields.is_empty() {
                    continue;
                }
                for field in &mut fields {
                    describe(&to[j].0, field);
                }
                (Kind::Modified, &to[j].0, &to[j].1, fields)
            }
            (None, None) => continue,
        };
        let what = match kind {
            Kind::Added => "added".to_owned(),
            Kind::Deleted => "deleted".to_owned(),
            _ => fields
                .iter()
                .map(|f| f.text.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        };
        out.push(ItemChange {
            kind,
            uid: with_uid.then(|| key.to_string()),
            name: name.clone(),
            text: format!("{}: {what}", label(key, name)),
            fields,
        });
    }
    out
}

fn component_changes(a: &Assembly, b: &Assembly) -> Vec<ItemChange> {
    let list = |assembly: &Assembly| -> Vec<(ComponentUid, String, Value)> {
        let root = (
            ComponentUid::ROOT,
            assembly.root_name.clone(),
            json!({"name": assembly.root_name}),
        );
        std::iter::once(root)
            .chain(assembly.components.iter().map(|c| {
                (
                    c.uid,
                    c.name.clone(),
                    json!({"name": c.name, "created_by": c.created_by, "link": c.link,
                           "library": c.library}),
                )
            }))
            .collect()
    };
    item_changes(list(a), list(b), true, &|_, _| {})
}

fn occurrence_changes(a: &Assembly, b: &Assembly) -> Vec<ItemChange> {
    let list = |assembly: &Assembly| -> Vec<(OccurrenceUid, String, Value)> {
        assembly
            .occurrences
            .iter()
            .map(|o| {
                (
                    o.uid,
                    assembly.occurrence_name(o.uid),
                    json!({"component": o.component, "parent": o.parent, "number": o.number,
                           "transform": matrix_rows(&o.transform), "grounded": o.grounded,
                           "visible": o.visible}),
                )
            })
            .collect()
    };
    item_changes(list(a), list(b), true, &|uid, field| {
        let transforms = a.occurrence(*uid).zip(b.occurrence(*uid));
        match (field.field.as_str(), transforms) {
            ("transform", Some((x, y))) => {
                field.text = text::transform(&x.transform, &y.transform);
            }
            ("component" | "parent", _) => {
                let name = |assembly: &Assembly, value: &Value| {
                    value
                        .as_str()
                        .and_then(|text| text.parse().ok())
                        .map_or_else(|| text::value(value), |uid| assembly.name(uid))
                };
                field.text = format!(
                    "{} {} -> {}",
                    field.field,
                    name(a, &field.from),
                    name(b, &field.to)
                );
            }
            _ => {}
        }
    })
}

fn body_changes(a: &DocState, b: &DocState) -> Vec<ItemChange> {
    let list = |state: &DocState| -> Vec<(BodyUid, String, Value)> {
        let uids: BTreeSet<BodyUid> = state
            .body_names
            .keys()
            .chain(state.body_attributes.keys())
            .copied()
            .collect();
        uids.into_iter()
            .map(|uid| {
                let name = state
                    .body_names
                    .get(&uid)
                    .cloned()
                    .unwrap_or_else(|| uid.to_string());
                let attributes = state.body_attributes.get(&uid).cloned().unwrap_or_default();
                let value = json!({"name": name, "visible": attributes.visible,
                                   "material": attributes.material,
                                   "appearance": attributes.appearance,
                                   "face_appearances": attributes.face_appearances});
                (uid, name, value)
            })
            .collect()
    };
    item_changes(list(a), list(b), true, &|_, _| {})
}

fn group_changes(a: &DocState, b: &DocState) -> Vec<ItemChange> {
    let list = |state: &DocState| -> Vec<(String, String, Value)> {
        state
            .groups
            .iter()
            .map(|g| {
                (
                    g.name.clone(),
                    g.name.clone(),
                    json!({"features": g.features}),
                )
            })
            .collect()
    };
    item_changes(list(a), list(b), false, &|_, _| {})
}

fn view_changes(a: &DocState, b: &DocState) -> Vec<ItemChange> {
    let list = |state: &DocState| -> Vec<(String, String, Value)> {
        state
            .named_views
            .iter()
            .map(|view| {
                let mut value = serde_json::to_value(view).expect("views serialize");
                if let Value::Object(fields) = &mut value {
                    fields.remove("name");
                }
                (view.name.clone(), view.name.clone(), value)
            })
            .collect()
    };
    item_changes(list(a), list(b), false, &|_, _| {})
}

fn analysis_changes(a: &DocState, b: &DocState) -> Vec<ItemChange> {
    let list = |state: &DocState| -> Vec<(String, String, Value)> {
        state
            .analyses
            .iter()
            .map(|analysis| {
                let mut value = serde_json::to_value(analysis).expect("analyses serialize");
                if let Value::Object(fields) = &mut value {
                    fields.remove("name");
                }
                (analysis.name.clone(), analysis.name.clone(), value)
            })
            .collect()
    };
    item_changes(list(a), list(b), false, &|_, _| {})
}

fn appearance_changes(a: &DocState, b: &DocState) -> Vec<ItemChange> {
    let list = |state: &DocState| -> Vec<(String, String, Value)> {
        state
            .appearances
            .iter()
            .map(|appearance| {
                let mut value = serde_json::to_value(appearance).expect("appearances serialize");
                if let Value::Object(fields) = &mut value {
                    fields.remove("id");
                }
                // An embedded image by its digest, not its data (mitcad#53).
                if let (Some(texture), Some(data)) = (
                    value.get_mut("texture").and_then(Value::as_object_mut),
                    appearance.texture.as_ref().and_then(|t| t.data.as_ref()),
                ) {
                    texture.insert(
                        "data".to_owned(),
                        json!(format!("sha256 {}", data.sha256())),
                    );
                }
                (appearance.id.clone(), appearance.name.clone(), value)
            })
            .collect()
    };
    item_changes(list(a), list(b), true, &|_, _| {})
}

/// The rotation angle (radians) and the translation from `a` to `b`.
fn motion(a: &Transform, b: &Transform) -> (f64, [f64; 3]) {
    // The rotation b * a^T; its angle from its trace.
    let mut trace = 0.0;
    for i in 0..3 {
        for k in 0..3 {
            trace += b.linear[i][k] * a.linear[i][k];
        }
    }
    let angle = ((trace - 1.0) / 2.0).clamp(-1.0, 1.0).acos();
    let shift = [
        b.translation[0] - a.translation[0],
        b.translation[1] - a.translation[1],
        b.translation[2] - a.translation[2],
    ];
    (angle, shift)
}

/// A number for people: at most six decimals.
fn number(value: f64) -> String {
    format_number(value, DEFAULT_DECIMALS)
}

/// Changes of the configuration table (mitcad#64): its selectors,
/// parameters and default, and each row by name (`configurations.M5x16:
/// dk 8.5 mm -> 8.52 mm`).
fn configuration_changes(
    a: &crate::configurations::Configurations,
    b: &crate::configurations::Configurations,
) -> Vec<FieldChange> {
    let mut out = Vec::new();
    for (field, from, to) in [
        (
            "configurations.selectors",
            json!(a.selectors),
            json!(b.selectors),
        ),
        (
            "configurations.parameters",
            json!(a.parameters),
            json!(b.parameters),
        ),
        ("configurations.default", json!(a.default), json!(b.default)),
    ] {
        if from != to {
            out.push(field_change(field, from, to));
        }
    }
    for row in &a.rows {
        match b.row(&row.name) {
            None => out.push(FieldChange {
                field: format!("configurations.{}", row.name),
                from: json!(row),
                to: Value::Null,
                text: format!("configuration {} deleted", row.name),
            }),
            Some(other) if other != row => {
                let mut parts = Vec::new();
                let names: std::collections::BTreeSet<&String> =
                    row.values.keys().chain(other.values.keys()).collect();
                for name in names {
                    let (x, y) = (row.values.get(name), other.values.get(name));
                    if x != y {
                        parts.push(format!(
                            "{name} {} -> {}",
                            x.map_or("-", String::as_str),
                            y.map_or("-", String::as_str)
                        ));
                    }
                }
                if row.select != other.select {
                    parts.push("selection changed".to_owned());
                }
                if row.designation != other.designation {
                    parts.push(format!(
                        "designation {} -> {}",
                        row.designation.as_deref().unwrap_or("-"),
                        other.designation.as_deref().unwrap_or("-")
                    ));
                }
                out.push(FieldChange {
                    field: format!("configurations.{}", row.name),
                    from: json!(row),
                    to: json!(other),
                    text: format!("configuration {}: {}", row.name, parts.join(", ")),
                });
            }
            Some(_) => {}
        }
    }
    for row in &b.rows {
        if a.row(&row.name).is_none() {
            out.push(FieldChange {
                field: format!("configurations.{}", row.name),
                from: Value::Null,
                to: json!(row),
                text: format!("configuration {} added", row.name),
            });
        }
    }
    out
}
