// SPDX-License-Identifier: MIT
//! Copies of components with their history (F6): Paste New, and inserting
//! a copy of another project file. The copied features get new uids and
//! default names; references among them follow (feature and body ids,
//! topological names, occurrences), their own parameters are copied with
//! their expressions, and other parameters are shared (the same document)
//! or brought along by name (another file).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use serde_json::Value;

use super::{DocState, ModelError, invalid};
use crate::assembly::Occurrence;
use crate::expr::ParamSpec;
use crate::features::{FeatureDef, FeatureEntry};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::parameters::ParamId;
use crate::transform::Transform;

/// Marks a parameter in the JSON form of a copied definition.
const PARAM: char = '\u{1}';

impl DocState {
    /// Adds a feature with resolved values at the marker: checked, and a
    /// new-component operation makes its component, placed in the
    /// feature's own. Returns the component it made.
    pub(crate) fn insert_resolved(
        &mut self,
        entry: FeatureEntry,
    ) -> Result<Option<ComponentUid>, ModelError> {
        let position = self.marker;
        self.check_feature(position, &entry)
            .map_err(ModelError::Invalid)?;
        let made = entry.def.info().new_component().then(|| {
            let made = self.assembly.create(None, Some(entry.uid));
            self.assembly
                .place(made, entry.component, Transform::IDENTITY);
            made
        });
        self.features.insert(position, Arc::new(entry));
        self.marker += 1;
        Ok(made)
    }

    /// Copies component `from` of `source` (this document's state before
    /// the copy when `same_document`, else another file's) with its
    /// features before the marker, the components placed in it and their
    /// occurrences, at the marker. The copy is `into`, or, with `maker`
    /// (the same document only), the component that a copy of that
    /// feature, which made `from`, makes in the maker's own component.
    /// Returns the copy.
    pub(crate) fn copy_component(
        &mut self,
        source: &DocState,
        from: ComponentUid,
        into: Option<ComponentUid>,
        maker: Option<FeatureUid>,
        same_document: bool,
    ) -> Result<ComponentUid, ModelError> {
        let a = &source.assembly;
        // The components to copy: `from` and everything placed in it.
        let mut inside = BTreeSet::from([from]);
        let mut todo = vec![from];
        while let Some(c) = todo.pop() {
            for o in a.children(c) {
                if inside.insert(o.component) {
                    todo.push(o.component);
                }
            }
        }
        let features: Vec<&FeatureEntry> = source.features[..source.marker]
            .iter()
            .filter(|f| inside.contains(&f.component) || Some(f.uid) == maker)
            .map(|f| &**f)
            .collect();
        let mut uids: HashMap<FeatureUid, FeatureUid> = HashMap::new();
        for f in &features {
            uids.insert(f.uid, self.new_uid());
        }
        let params = self.copy_parameters(source, &features, &uids, same_document)?;

        let mut components: BTreeMap<ComponentUid, ComponentUid> = BTreeMap::new();
        if let Some(into) = into {
            components.insert(from, into);
        }
        if let Some(maker) = maker.and_then(|m| source.entry(m)) {
            components.insert(maker.component, maker.component);
        }
        // Components that no copied feature makes are made now.
        for c in &inside {
            let made_by_copy = a
                .component(*c)
                .and_then(|d| d.created_by)
                .is_some_and(|f| uids.contains_key(&f));
            if components.contains_key(c) || made_by_copy {
                continue;
            }
            let name = self.free_component_name(&a.name(*c));
            components.insert(*c, self.assembly.create(Some(&name), None));
        }

        // The features, in timeline order.
        let mut copied = Vec::with_capacity(features.len());
        for f in &features {
            let component = *components.get(&f.component).ok_or_else(|| {
                invalid(format!(
                    "cannot copy {}: it comes before the feature that makes {}",
                    f.name,
                    a.name(f.component)
                ))
            })?;
            let def = copy_def(&f.def, &params, &uids)?;
            let uid = uids[&f.uid];
            let entry = FeatureEntry {
                uid,
                name: self.default_name(def.base_name()),
                suppressed: f.suppressed,
                component,
                def,
            };
            let made = self
                .insert_resolved(entry)
                .map_err(|e| invalid(format!("copy of {}: {e}", f.name)))?;
            if let (Some(made), Some(original)) = (made, a.created_by(f.uid)) {
                let name = self.free_component_name(&a.name(original));
                if let Some(c) = self.assembly.component_mut(made) {
                    c.name = name;
                }
                components.insert(original, made);
            }
            copied.push(uid);
        }
        let copy = *components
            .get(&from)
            .ok_or_else(|| invalid("the copy made no component"))?;

        // Occurrences in the copied components; the first of a component
        // that a copied feature made is the one that feature placed.
        let mut occurrences: HashMap<OccurrenceUid, OccurrenceUid> = HashMap::new();
        let mut auto_used = BTreeSet::new();
        for o in a.occurrences.iter().filter(|o| inside.contains(&o.parent)) {
            let (Some(&parent), Some(&component)) =
                (components.get(&o.parent), components.get(&o.component))
            else {
                continue;
            };
            let automatic = self
                .assembly
                .occurrences_of(component)
                .find(|t| t.parent == parent && !auto_used.contains(&t.uid))
                .filter(|_| {
                    a.component(o.component)
                        .and_then(|d| d.created_by)
                        .is_some_and(|f| uids.contains_key(&f))
                })
                .map(|t| t.uid);
            let target = match automatic {
                Some(t) => t,
                None => self.assembly.place(component, parent, o.transform),
            };
            auto_used.insert(target);
            set_flags(self.assembly.occurrence_mut(target).expect("placed"), o);
            occurrences.insert(o.uid, target);
        }
        // Moves of occurrences name the copies.
        for uid in &copied {
            let position = self.position(*uid).expect("inserted");
            let mut entry = (*self.features[position]).clone();
            let changed = match &mut entry.def {
                FeatureDef::MoveOccurrence(m) => {
                    for o in &mut m.occurrences {
                        *o = occurrences.get(o).copied().unwrap_or(*o);
                    }
                    true
                }
                FeatureDef::CapturePosition(c) => {
                    for p in &mut c.positions {
                        p.occurrence = occurrences
                            .get(&p.occurrence)
                            .copied()
                            .unwrap_or(p.occurrence);
                    }
                    true
                }
                // Joints between occurrences (mitcad#55).
                def => crate::joints::remap_occurrences(def, &|o| {
                    occurrences.get(&o).copied().unwrap_or(o)
                }),
            };
            if changed {
                self.features[position] = Arc::new(entry);
            }
        }
        // Body attributes follow the bodies, face appearances their faces.
        let text: HashMap<String, String> = uids
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let attributes: Vec<_> = source
            .body_attributes
            .iter()
            .filter_map(|(body, attributes)| {
                let feature = uids.get(&body.feature)?;
                let mut attributes = attributes.clone();
                attributes.face_appearances = attributes
                    .face_appearances
                    .into_iter()
                    .map(|(face, appearance)| (remap_text(&face, &text), appearance))
                    .collect();
                Some((BodyUid::new(*feature, body.index), attributes))
            })
            .collect();
        self.body_attributes.extend(attributes);
        Ok(copy)
    }

    /// The parameters the copied features use: their own ones copied with
    /// their expressions (the names they refer to follow), others shared
    /// in the same document or brought from another by name.
    fn copy_parameters(
        &mut self,
        source: &DocState,
        features: &[&FeatureEntry],
        uids: &HashMap<FeatureUid, FeatureUid>,
        same_document: bool,
    ) -> Result<HashMap<ParamId, ParamId>, ModelError> {
        let table = source.parameters.table();
        let mut needed: BTreeSet<ParamId> = features.iter().flat_map(|f| f.def.params()).collect();
        let mut todo: Vec<ParamId> = needed.iter().copied().collect();
        while let Some(id) = todo.pop() {
            if let Some(p) = table.get(id) {
                for d in p.dependencies() {
                    if needed.insert(*d) {
                        todo.push(*d);
                    }
                }
            }
        }
        let mut map: HashMap<ParamId, ParamId> = HashMap::new();
        let mut names: HashMap<String, String> = HashMap::new();
        for id in table.evaluation_order() {
            if !needed.contains(&id) {
                continue;
            }
            let p = table.get(id).expect("in the table");
            let owner = source
                .parameters
                .owner(id)
                .and_then(|o| uids.get(&o))
                .copied();
            if owner.is_none() && same_document {
                map.insert(id, id);
                continue;
            }
            let name = if owner.is_some() && is_dimension_name(p.name()) {
                self.parameters.table().unused_name("d")
            } else {
                free_parameter_name(&self.parameters, p.name())
            };
            let expression = rename_references(p.expression(), p.expr(), &names);
            let spec = ParamSpec::user(&name, &expression, p.unit()).with_comment(p.comment());
            let new = self
                .parameters
                .add(spec, owner)
                .map_err(|e| invalid(format!("parameter {}: {e}", p.name())))?;
            names.insert(p.name().to_owned(), name);
            map.insert(id, new);
        }
        Ok(map)
    }
}

fn set_flags(target: &mut Occurrence, source: &Occurrence) {
    target.transform = source.transform;
    target.grounded = source.grounded;
    target.visible = source.visible;
}

/// `d12` and the like: dimension names, which copies number afresh.
fn is_dimension_name(name: &str) -> bool {
    name.strip_prefix('d')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `name`, or `name_1`, `name_2`, ... the first one not in use.
fn free_parameter_name(parameters: &crate::parameters::Parameters, name: &str) -> String {
    std::iter::once(name.to_owned())
        .chain((1..).map(|k| format!("{name}_{k}")))
        .find(|n| parameters.find(n).is_none() && !crate::expr::is_reserved_name(n))
        .expect("a free name exists")
}

/// An expression with the references renamed by `names` all at once.
fn rename_references(
    text: &str,
    expr: &crate::expr::Expr,
    names: &HashMap<String, String>,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (name, span) in expr.reference_spans() {
        if let Some(new) = names.get(name) {
            out.push_str(&text[copied..span.start]);
            out.push_str(new);
            copied = span.end;
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// A definition with its parameters and feature references mapped. The
/// references are rewritten in the definition's JSON form: every feature
/// id in a reference text (`F3`, `F3.b0`, `E{F3:side(c1)|…}`).
fn copy_def(
    def: &FeatureDef,
    params: &HashMap<ParamId, ParamId>,
    uids: &HashMap<FeatureUid, FeatureUid>,
) -> Result<FeatureDef, ModelError> {
    let mut slots = Vec::new();
    let marked = def
        .map_params(&mut |_, id: &ParamId| {
            slots.push(*params.get(id).unwrap_or(id));
            Ok::<_, ()>(format!("{PARAM}{}", slots.len() - 1))
        })
        .expect("never fails");
    let mut value = serde_json::to_value(&marked).map_err(|e| invalid(e.to_string()))?;
    let text: HashMap<String, String> = uids
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    remap_value(&mut value, &text);
    let copied: FeatureDef<String> =
        serde_json::from_value(value).map_err(|e| invalid(format!("copy: {e}")))?;
    copied.map_params(&mut |slot, marker: &String| {
        marker
            .strip_prefix(PARAM)
            .and_then(|i| i.parse::<usize>().ok())
            .and_then(|i| slots.get(i).copied())
            .ok_or_else(|| invalid(format!("copy: {slot} lost its parameter")))
    })
}

/// Fields of definitions that are texts, not references.
const TEXT_FIELDS: [&str; 5] = ["name", "source", "text", "font", "brep"];

fn remap_value(value: &mut Value, uids: &HashMap<String, String>) {
    match value {
        Value::String(s) if !s.starts_with(PARAM) => *s = remap_text(s, uids),
        Value::Array(items) => {
            for item in items {
                remap_value(item, uids);
            }
        }
        Value::Object(fields) => {
            for (key, item) in fields.iter_mut() {
                if !TEXT_FIELDS.contains(&key.as_str()) {
                    remap_value(item, uids);
                }
            }
        }
        _ => {}
    }
}

/// Replaces the feature ids `F<n>` in a reference text: an `F` that does
/// not follow a letter, digit or `_`, then digits that no letter, digit or
/// `_` follows.
fn remap_text(text: &str, uids: &HashMap<String, String>) -> String {
    let bytes = text.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut copied = 0;
    while i < bytes.len() {
        if bytes[i] == b'F' && (i == 0 || !word(bytes[i - 1])) {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1
                && (j == bytes.len() || !word(bytes[j]))
                && let Some(new) = uids.get(&text[i..j])
            {
                out.push_str(&text[copied..i]);
                out.push_str(new);
                copied = j;
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out.push_str(&text[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_ids_in_references_are_renamed() {
        let uids: HashMap<String, String> = [("F2", "F9"), ("F3", "F10")]
            .into_iter()
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .collect();
        assert_eq!(remap_text("F2", &uids), "F9");
        assert_eq!(remap_text("F2.b0", &uids), "F9.b0");
        assert_eq!(
            remap_text("E{F2:side(c1[c4,c2])|F3:inst1(F2:end(r{c5})#1)}", &uids),
            "E{F9:side(c1[c4,c2])|F10:inst1(F9:end(r{c5})#1)}"
        );
        assert_eq!(remap_text("F23", &uids), "F23");
        assert_eq!(remap_text("XF2", &uids), "XF2");
        assert_eq!(remap_text("F2a", &uids), "F2a");
        assert_eq!(remap_text("r{c1[c4,c2]}", &uids), "r{c1[c4,c2]}");
    }

    #[test]
    fn expressions_rename_all_references_at_once() {
        let mut table = crate::expr::ParameterTable::new(crate::parameters::default_context());
        let unit = crate::expr::Unit::MM;
        table.add(ParamSpec::user("d1", "10 mm", unit)).unwrap();
        table.add(ParamSpec::user("d2", "20 mm", unit)).unwrap();
        let id = table
            .add(ParamSpec::user("d3", "d1 + d2 * 2", unit))
            .unwrap();
        let p = table.get(id).unwrap();
        let names: HashMap<String, String> = [("d1", "d2"), ("d2", "d7")]
            .into_iter()
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .collect();
        assert_eq!(
            rename_references(p.expression(), p.expr(), &names),
            "d2 + d7 * 2"
        );
    }
}
