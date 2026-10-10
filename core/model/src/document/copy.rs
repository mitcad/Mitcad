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
use crate::assembly::{Assembly, Occurrence};
use crate::expr::ParamSpec;
use crate::features::{ChamferSizeDef, FeatureDef, FeatureEntry, FilletSizeDef, OccurrencePath};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;
use crate::topo::{EdgeName, TopoName};
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

    /// Copies the component at `source_path` of `source` (this document's
    /// state before the copy when `same_document`, else another file's)
    /// with its features before the marker, the components placed in it and their
    /// occurrences, at the marker. `into` is already placed in the
    /// destination assembly. With `maker` (the same document only), the
    /// feature that made the source component is copied too and owns
    /// `into`. An empty source path copies another file's root component.
    pub(crate) fn copy_component(
        &mut self,
        source: &DocState,
        source_path: &[OccurrenceUid],
        into: ComponentUid,
        maker: Option<FeatureUid>,
        same_document: bool,
    ) -> Result<ComponentUid, ModelError> {
        let a = &source.assembly;
        let from =
            crate::joints::path_component(a, ComponentUid::ROOT, source_path).map_err(invalid)?;
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
        components.insert(from, into);
        if let Some(maker) = maker.and_then(|m| source.entry(m)) {
            components.insert(maker.component, maker.component);
        }
        // Make the whole assembly before validating features: links and
        // joints must already be able to resolve their copied occurrences.
        for c in &inside {
            if !components.contains_key(c) {
                let name = self.free_component_name(&a.name(*c));
                components.insert(*c, self.assembly.create(Some(&name), None));
            }
            if let Some(copy) = self.assembly.component_mut(components[c]) {
                copy.created_by = a
                    .component(*c)
                    .and_then(|d| d.created_by)
                    .and_then(|f| uids.get(&f).copied());
            }
        }
        let mut occurrences: HashMap<OccurrenceUid, OccurrenceUid> = HashMap::new();
        for o in a.occurrences.iter().filter(|o| inside.contains(&o.parent)) {
            let target =
                self.assembly
                    .place(components[&o.component], components[&o.parent], o.transform);
            set_flags(self.assembly.occurrence_mut(target).expect("placed"), o);
            occurrences.insert(o.uid, target);
        }
        let prefix = crate::links::first_path(&self.assembly, into, None).map_err(invalid)?;

        // The features, in timeline order, checked only after all their
        // reference names and occurrence paths have been mapped.
        for f in &features {
            let component = components[&f.component];
            let mut def = copy_def(&f.def, &params, &uids)?;
            let references = CopyReferences {
                source: a,
                destination: &self.assembly,
                source_path,
                prefix: &prefix,
                occurrences: &occurrences,
                uids: &uids,
            };
            references.links(&mut def, &f.def, component)?;
            match &mut def {
                FeatureDef::MoveOccurrence(m) => {
                    for o in &mut m.occurrences {
                        *o = occurrences.get(o).copied().unwrap_or(*o);
                    }
                }
                FeatureDef::CapturePosition(c) => {
                    for p in &mut c.positions {
                        p.occurrence = occurrences
                            .get(&p.occurrence)
                            .copied()
                            .unwrap_or(p.occurrence);
                    }
                }
                // Joints between occurrences (mitcad#55).
                def => {
                    crate::joints::remap_occurrences(def, &|o| {
                        occurrences.get(&o).copied().unwrap_or(o)
                    });
                }
            }
            let entry = FeatureEntry {
                uid: uids[&f.uid],
                name: self.default_name(def.base_name()),
                suppressed: f.suppressed,
                component,
                def,
            };
            self.check_feature(self.marker, &entry)
                .map_err(|e| invalid(format!("copy of {}: {e}", f.name)))?;
            // Components and placements, including ones made by these
            // features, were created above; insert_resolved would make
            // them a second time.
            self.features.insert(self.marker, Arc::new(entry));
            self.marker += 1;
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
                    .map(|(face, appearance)| (remap_reference(&face, &text), appearance))
                    .collect();
                Some((BodyUid::new(*feature, body.index), attributes))
            })
            .collect();
        self.body_attributes.extend(attributes);
        Ok(into)
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

/// Reference mapping for geometry picked in assembly context. The same
/// component may also be placed outside the copied subtree: a link picked
/// there keeps both that occurrence and its original geometry ids.
struct CopyReferences<'a> {
    source: &'a Assembly,
    destination: &'a Assembly,
    source_path: &'a [OccurrenceUid],
    prefix: &'a [OccurrenceUid],
    occurrences: &'a HashMap<OccurrenceUid, OccurrenceUid>,
    uids: &'a HashMap<FeatureUid, FeatureUid>,
}

impl CopyReferences<'_> {
    fn body(&self, body: BodyUid) -> BodyUid {
        BodyUid::new(
            self.uids
                .get(&body.feature)
                .copied()
                .unwrap_or(body.feature),
            body.index,
        )
    }

    /// A root-relative path through the selected source occurrence, with
    /// its prefix replaced by the new copy's and the occurrences below
    /// it mapped. Paths outside that subtree remain external, even if
    /// they end in a copied component.
    fn path(&self, path: &OccurrencePath) -> Option<OccurrencePath> {
        let mut component = ComponentUid::ROOT;
        if !path.0.starts_with(self.source_path) {
            return None;
        }
        for uid in &path.0 {
            let occurrence = self.source.occurrence(*uid)?;
            if occurrence.parent != component {
                return None;
            }
            component = occurrence.component;
        }
        let suffix: Option<Vec<_>> = path.0[self.source_path.len()..]
            .iter()
            .map(|o| self.occurrences.get(o).copied())
            .collect();
        Some(OccurrencePath(
            self.prefix.iter().copied().chain(suffix?).collect(),
        ))
    }

    fn link(
        &self,
        link: &OccurrenceLink,
        component: ComponentUid,
    ) -> Result<(OccurrenceLink, bool), ModelError> {
        let source = self.path(&link.source);
        let internal = source.is_some();
        let target = self.path(&link.target);
        let target = crate::links::first_path(
            self.destination,
            component,
            target.as_ref().map(|p| p.0.as_slice()),
        )
        .map_err(invalid)?;
        Ok((
            OccurrenceLink {
                source: source.unwrap_or_else(|| link.source.clone()),
                target: OccurrencePath(target),
            },
            internal,
        ))
    }

    fn links(
        &self,
        copy: &mut FeatureDef,
        original: &FeatureDef,
        component: ComponentUid,
    ) -> Result<(), ModelError> {
        match (copy, original) {
            (FeatureDef::Combine(copy), FeatureDef::Combine(original)) => {
                let mut tools = HashMap::new();
                copy.tool_links.clear();
                for (tool, link) in &original.tool_links {
                    let (link, internal) = self.link(link, component)?;
                    let mapped = if internal { self.body(*tool) } else { *tool };
                    tools.insert(*tool, mapped);
                    copy.tool_links.insert(mapped, link);
                }
                copy.tools = original
                    .tools
                    .iter()
                    .map(|tool| tools.get(tool).copied().unwrap_or_else(|| self.body(*tool)))
                    .collect();
            }
            (FeatureDef::Sketch(copy), FeatureDef::Sketch(original)) => {
                if let Some(link) = &original.plane_link {
                    let (link, internal) = self.link(link, component)?;
                    copy.plane_link = Some(link);
                    if !internal {
                        copy.plane = original.plane.clone();
                    }
                }
                let text = self
                    .uids
                    .iter()
                    .map(|(a, b)| (a.to_string(), b.to_string()))
                    .collect();
                for (copy, original) in copy.projections.iter_mut().zip(&original.projections) {
                    let internal = if let Some(link) = &original.link {
                        let (link, internal) = self.link(link, component)?;
                        copy.link = Some(link);
                        internal
                    } else {
                        true
                    };
                    copy.source = if internal {
                        remap_reference(&original.source.to_string(), &text)
                            .parse()
                            .map_err(|e| invalid(format!("copy projection: {e}")))?
                    } else {
                        original.source.clone()
                    };
                    copy.body = original
                        .body
                        .map(|b| if internal { self.body(b) } else { b });
                }
            }
            _ => {}
        }
        Ok(())
    }
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
/// references are rewritten in the definition's JSON form, accepting only
/// feature ids, body ids and parsed topological names. Body-keyed links
/// and projection sources are handled in their typed context above.
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
    let mut copied = copied.map_params(&mut |slot, marker: &String| {
        marker
            .strip_prefix(PARAM)
            .and_then(|i| i.parse::<usize>().ok())
            .and_then(|i| slots.get(i).copied())
            .ok_or_else(|| invalid(format!("copy: {slot} lost its parameter")))
    })?;
    copy_measurement_sides(&mut copied, def, &text);
    Ok(copied)
}

/// Renumbering may reverse the canonical faces of an edge. Distances
/// measured on its first face must still refer to that same copied face.
fn copy_measurement_sides(
    copy: &mut FeatureDef,
    original: &FeatureDef,
    uids: &HashMap<String, String>,
) {
    let edges = |old: &[EdgeName], new: &[EdgeName]| -> (Vec<EdgeName>, Vec<EdgeName>) {
        let (mut kept, mut swapped) = (Vec::new(), Vec::new());
        for (old, new) in old.iter().zip(new) {
            if remap_reference(&old.faces()[0].to_string(), uids) == new.faces()[0].to_string() {
                kept.push(new.clone());
            } else {
                swapped.push(new.clone());
            }
        }
        (kept, swapped)
    };
    match (copy, original) {
        (FeatureDef::Fillet(copy), FeatureDef::Fillet(original)) => {
            for (mut set, old) in std::mem::take(&mut copy.sets)
                .into_iter()
                .zip(&original.sets)
            {
                let mut split = false;
                if set.reference_face.is_none()
                    && matches!(set.size, FilletSizeDef::Asymmetric { .. })
                {
                    let (kept, swapped) = edges(&old.edges, &set.edges);
                    if !swapped.is_empty() {
                        split = true;
                        let mut other = set.clone();
                        other.edges = swapped;
                        other.faces.clear();
                        if let FilletSizeDef::Asymmetric { flip, .. } = &mut other.size {
                            *flip = !*flip;
                        }
                        copy.sets.push(other);
                        set.edges = kept;
                    }
                }
                if !split || !set.edges.is_empty() || !set.faces.is_empty() {
                    copy.sets.push(set);
                }
            }
        }
        (FeatureDef::Chamfer(copy), FeatureDef::Chamfer(original)) => {
            for (mut set, old) in std::mem::take(&mut copy.sets)
                .into_iter()
                .zip(&original.sets)
            {
                let mut split = false;
                if set.reference_face.is_none()
                    && !matches!(set.size, ChamferSizeDef::EqualDistance { .. })
                {
                    let (kept, swapped) = edges(&old.edges, &set.edges);
                    if !swapped.is_empty() {
                        split = true;
                        let mut other = set.clone();
                        other.edges = swapped;
                        other.faces.clear();
                        other.flip = !other.flip;
                        copy.sets.push(other);
                        set.edges = kept;
                    }
                }
                if !split || !set.edges.is_empty() || !set.faces.is_empty() {
                    copy.sets.push(set);
                }
            }
        }
        _ => {}
    }
}

/// Fields of definitions that are texts, not references.
const TEXT_FIELDS: [&str; 5] = ["name", "source", "text", "font", "brep"];

fn remap_value(value: &mut Value, uids: &HashMap<String, String>) {
    match value {
        Value::String(s) if !s.starts_with(PARAM) => *s = remap_reference(s, uids),
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

fn remap_reference(text: &str, uids: &HashMap<String, String>) -> String {
    if text.parse::<FeatureUid>().is_ok() || text.parse::<BodyUid>().is_ok() {
        return remap_text(text, uids);
    }
    if text.parse::<TopoName>().is_ok() {
        // Parsing again canonicalises nested edge/vertex face order after
        // renumbering (F9 sorts differently from F10).
        return remap_text(text, uids)
            .parse::<TopoName>()
            .expect("mapping feature ids preserves a topological name")
            .to_string();
    }
    text.to_owned()
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
    fn only_typed_references_are_mapped_and_nested_names_are_canonical() {
        let uids = HashMap::from([
            ("F2".to_owned(), "F9".to_owned()),
            ("F3".to_owned(), "F10".to_owned()),
        ]);
        assert_eq!(remap_reference("F2.b0", &uids), "F9.b0");
        assert_eq!(
            remap_reference("F3:fillet(E{F2:side(c1)|F3:end(r{c1})})", &uids),
            "F10:fillet(E{F10:end(r{c1})|F9:side(c1)})"
        );
        assert_eq!(remap_reference("caption F2", &uids), "caption F2");
        let mut value = serde_json::json!({
            "name": "F2", "source": "F2", "text": "F2", "font": "F2", "brep": "F2",
            "body": "F2.b0", "reference_face": "F2:end(r{c1})",
        });
        remap_value(&mut value, &uids);
        for field in TEXT_FIELDS {
            assert_eq!(value[field], "F2");
        }
        assert_eq!(value["body"], "F9.b0");
        assert_eq!(value["reference_face"], "F9:end(r{c1})");
    }

    #[test]
    fn whole_design_links_to_root_geometry_receive_the_copy_prefix() {
        let source = Assembly::default();
        let mut destination = Assembly::default();
        let copy = destination.create(Some("Copy"), None);
        let occurrence = destination.place(copy, ComponentUid::ROOT, Transform::IDENTITY);
        let prefix = [occurrence];
        let references = CopyReferences {
            source: &source,
            destination: &destination,
            source_path: &[],
            prefix: &prefix,
            occurrences: &HashMap::new(),
            uids: &HashMap::new(),
        };
        let (link, internal) = references.link(&OccurrenceLink::default(), copy).unwrap();
        assert!(internal);
        assert_eq!(link.source.0, prefix);
        assert_eq!(link.target.0, prefix);
    }

    #[test]
    fn copied_chamfers_keep_distances_on_the_original_faces() {
        use crate::features::{ChamferCorner, ChamferDef, ChamferSetDef};
        let old = FeatureDef::Chamfer(ChamferDef {
            body: "F2.b0".parse().unwrap(),
            sets: vec![ChamferSetDef {
                edges: vec![
                    "E{F2:a|F3:b}".parse().unwrap(),
                    "E{F2:a|F2:b}".parse().unwrap(),
                ],
                faces: Vec::new(),
                size: ChamferSizeDef::TwoDistances {
                    distance1: ParamId::from_raw(1),
                    distance2: ParamId::from_raw(2),
                },
                reference_face: None,
                flip: false,
                tangent_chain: true,
            }],
            corner: ChamferCorner::Chamfer,
        });
        let uids = HashMap::from([
            (FeatureUid(2), FeatureUid(9)),
            (FeatureUid(3), FeatureUid(10)),
        ]);
        let FeatureDef::Chamfer(copy) = copy_def(&old, &HashMap::new(), &uids).unwrap() else {
            panic!("copied a chamfer");
        };
        assert_eq!(copy.sets.len(), 2);
        assert!(copy.sets[0].flip);
        assert_eq!(copy.sets[0].edges[0].to_string(), "E{F10:b|F9:a}");
        assert!(!copy.sets[1].flip);
        assert_eq!(copy.sets[1].edges[0].to_string(), "E{F9:a|F9:b}");
        assert_eq!(copy.sets[0].size, copy.sets[1].size);
    }

    #[test]
    fn copied_fillets_map_reference_faces_vertices_and_asymmetric_sides() {
        let old: FeatureDef<String> = serde_json::from_value(serde_json::json!({
            "type": "fillet", "body": "F2.b0", "sets": [
                {"edges": ["E{F2:a|F3:b}"],
                 "size": {"type": "asymmetric", "distance1": "d1", "distance2": "d2"}},
                {"edges": ["E{F2:a|F3:b}"], "faces": ["F2:a"],
                 "reference_face": "F3:b",
                 "size": {"type": "variable", "start": "d1", "end": "d2",
                          "start_vertex": "V{F2:a|F2:b|F3:c}"}}
            ]
        }))
        .unwrap();
        let old = old
            .map_params(&mut |_, _| Ok::<_, ()>(ParamId::from_raw(1)))
            .unwrap();
        let uids = HashMap::from([
            (FeatureUid(2), FeatureUid(9)),
            (FeatureUid(3), FeatureUid(10)),
        ]);
        let FeatureDef::Fillet(copy) = copy_def(&old, &HashMap::new(), &uids).unwrap() else {
            panic!("copied a fillet");
        };
        assert_eq!(copy.sets.len(), 2);
        assert!(matches!(
            copy.sets[0].size,
            FilletSizeDef::Asymmetric { flip: true, .. }
        ));
        assert_eq!(copy.sets[0].edges[0].to_string(), "E{F10:b|F9:a}");
        assert_eq!(
            copy.sets[1].reference_face.as_ref().unwrap().to_string(),
            "F10:b"
        );
        assert_eq!(copy.sets[1].faces[0].to_string(), "F9:a");
        let FilletSizeDef::Variable { start_vertex, .. } = &copy.sets[1].size else {
            panic!("kept a variable fillet");
        };
        assert_eq!(
            start_vertex.as_ref().unwrap().to_string(),
            "V{F10:c|F9:a|F9:b}"
        );
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
