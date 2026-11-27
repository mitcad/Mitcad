// SPDX-License-Identifier: MIT
//! Derived designs (mitcad#324): a `DerivedContext` item brings bodies of
//! other documents into its component (SCHEMA.md §5.3, `DerivedContext`).
//!
//! The decoder names each body in the document that holds it (a part an
//! assembly places), by its body object there, with the placement of the
//! body's occurrence in the source design and the placement of the deriving
//! design there. The import treats each body's document as a document the
//! design refers to (mitcad#196: found and imported from its own file,
//! [`crate::components::external_documents`]): before the replay a hidden
//! occurrence of the component standing for it is placed in the item's
//! component, where the derive puts the body
//! (`placement(design)⁻¹ · placement(body)`), and filled with the document's
//! copy like any other ([`crate::prefill`]). The item becomes Mitcad's
//! `derive` feature: a linked copy of the copy's body (the body its own
//! import matched to the body object, [`DesignReport::body_entities`]),
//! checked against the file's history like any other item. A derive whose
//! record lists no entities brings nothing.

use std::collections::{HashMap, HashSet};

use mitcad_dump::{Dump, Mat4, TimelineItem};
use mitcad_model::assembly::inverse;
use mitcad_model::features::OccurrencePath;
use mitcad_model::{BodyUid, ComponentUid, Kernel, OccurrenceUid, Transform};
use serde_json::{Map, Value, json};

use crate::components::{InsertedComponent, transform};
use crate::report::DesignReport;
use crate::{Candidate, CandidateCore, Importer, InputProvenance};

/// The note of a derive that brings nothing.
pub const BRINGS_NOTHING: &str =
    "no geometry: a derive whose record lists no entities of the other design";

/// The name of the component standing for a derived body's document until
/// its copy names it.
const SOURCE: &str = "Derived source";

/// Where the import placed a derived entity's document
/// ([`crate::components::ComponentReport::derived`]).
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedSource {
    /// The derive's timeline index and the entity's position in it.
    pub item: i64,
    pub entity: usize,
    /// The component standing for the entity's document.
    pub component: ComponentUid,
    /// Its hidden occurrence in the item's component.
    pub occurrence: OccurrenceUid,
    /// Where the derive puts the entity's occurrence in the item's
    /// component (mm).
    pub placement: Transform,
}

/// A derive item's entities (the decoder's `detail.entities`).
fn entities(item: &TimelineItem) -> Option<&Vec<Value>> {
    if item.object_type() != Some("DerivedContext") {
        return None;
    }
    match item.detail.as_ref()? {
        mitcad_dump::Detail::Other(map) => map.get("entities")?.as_array(),
        _ => None,
    }
}

/// The detail of a derive item.
fn detail(item: &TimelineItem) -> Option<&Map<String, Value>> {
    match item.detail.as_ref()? {
        mitcad_dump::Detail::Other(map) => Some(map),
        _ => None,
    }
}

/// Whether a derive's record lists no entities: it brings nothing.
pub(crate) fn brings_nothing(item: &TimelineItem) -> bool {
    entities(item).is_some_and(Vec::is_empty)
}

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.pointer(&format!("/_f3d/{key}"))?.as_str()
}

/// A placement's matrix (cm) in millimetres.
fn placement(v: Option<&Value>) -> Option<Transform> {
    let m: Mat4 = serde_json::from_value(v?.get("transform")?.clone()).ok()?;
    transform(&m)
}

/// The documents the derives' entities are in: URN, document GUID and key,
/// each once.
pub(crate) fn documents(dump: &Dump) -> Vec<(String, Option<String>, Option<String>)> {
    let mut out: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    for item in dump.timeline_items() {
        for e in entities(item).into_iter().flatten() {
            let Some(urn) = text(e, "urn") else { continue };
            if !out.iter().any(|(u, _, _)| u == urn) {
                out.push((
                    urn.to_owned(),
                    text(e, "document_guid").map(str::to_owned),
                    text(e, "key").map(str::to_owned),
                ));
            }
        }
    }
    out
}

/// The derives' source designs (each once): their files are read for the
/// placements the derives keep no matrix for
/// (`mitcad_f3d_design::resolve_derive_placements`), not imported.
pub fn source_documents(dump: &Dump) -> Vec<crate::components::ExternalDocument> {
    let mut out: Vec<crate::components::ExternalDocument> = Vec::new();
    for item in dump.timeline_items() {
        let Some(source) = entities(item)
            .filter(|e| !e.is_empty())
            .and_then(|_| detail(item))
            .and_then(|d| d.get("source"))
        else {
            continue;
        };
        let Some(urn) = text(source, "urn") else {
            continue;
        };
        if !out.iter().any(|d| d.urn == urn) {
            out.push(crate::components::ExternalDocument {
                urn: urn.to_owned(),
                guid: text(source, "document_guid").map(str::to_owned),
                key: text(source, "key").map(str::to_owned),
            });
        }
    }
    out
}

/// Where a derive puts an entity's occurrence in the item's component:
/// the entity's placement in the source design seen from the deriving
/// design's placement there (mm).
fn entity_placement(derive: &Map<String, Value>, entity: &Value) -> Option<Transform> {
    let body = placement(entity.get("placement"))?;
    let design = derive
        .get("placements")
        .and_then(Value::as_array)
        .and_then(|p| placement(p.first()))
        .unwrap_or(Transform::IDENTITY);
    Some(inverse(&design).after(&body))
}

impl DesignReport {
    /// This design's bodies by the body objects the file names them by
    /// (`_f3d.appearance.bodies[].entity`, joined on their stored body):
    /// what a design that derives them names them by (mitcad#324).
    pub(crate) fn set_body_entities(&mut self, dump: &Dump) {
        let Some(bodies) = dump
            .f3d
            .as_ref()
            .and_then(|f| f.appearance.as_ref())
            .and_then(|a| a.bodies.as_ref())
        else {
            return;
        };
        let by_source: HashMap<&str, BodyUid> = self
            .bodies
            .iter()
            .filter_map(|b| Some((b.source.as_str(), b.uid?)))
            .collect();
        for b in bodies {
            let (Some(entity), Some(blob), Some(record)) = (b.entity, &b.blob, b.record) else {
                continue;
            };
            if let Some(uid) = by_source.get(format!("{blob}#{record}").as_str()) {
                self.body_entities.insert(entity, *uid);
            }
        }
    }
}

impl<K: Kernel> Importer<'_, K> {
    /// The hidden occurrences of derives' sources: this design's, and those
    /// inside the copies of other designs (the hidden occurrences their
    /// derives' links go through). Their bodies lie where the derives'
    /// copies are, so sketches' faces and linked curves are looked for on
    /// the copies only.
    pub(crate) fn derive_sources(&self) -> HashSet<OccurrenceUid> {
        let mut out: HashSet<OccurrenceUid> = self
            .report
            .components
            .derived
            .iter()
            .map(|s| s.occurrence)
            .collect();
        let assembly = self.doc.assembly();
        for f in self.doc.features() {
            if let mitcad_model::FeatureDef::Derive(d) = &f.def {
                for link in d.links.values() {
                    out.extend(
                        link.source
                            .0
                            .iter()
                            .filter(|o| assembly.occurrence(**o).is_some_and(|x| !x.visible)),
                    );
                }
            }
        }
        out
    }

    /// Places a hidden occurrence of the component standing for each
    /// derived entity's document in the derive's component, where the derive
    /// puts the entity (before the components are filled).
    pub(crate) fn place_derive_sources(&mut self) {
        let dump = self.dump;
        for (position, item) in dump.timeline_items().iter().enumerate() {
            let Some(list) = entities(item) else { continue };
            let Some(derive) = detail(item) else { continue };
            let index = item.index.unwrap_or(position as i64);
            let parent = self.components.of_item(index);
            for (j, e) in list.iter().enumerate() {
                let (Some(urn), Some(key)) = (text(e, "urn"), text(e, "key")) else {
                    continue;
                };
                let Some(local) = entity_placement(derive, e) else {
                    self.report.warnings.push(format!(
                        "derive {}: entity {j} has no placement",
                        item.name().unwrap_or("?")
                    ));
                    continue;
                };
                let known = self.components.inserted_of_key(key);
                let made = match known {
                    Some(c) => self.doc.add_occurrence(c, parent, local).map(|o| (c, o)),
                    None => self.doc.add_component(Some(SOURCE), parent, local),
                };
                let (component, occurrence) = match made {
                    Ok(made) => made,
                    Err(e) => {
                        self.report.warnings.push(format!(
                            "derive {}: the source of entity {j}: {e}",
                            item.name().unwrap_or("?")
                        ));
                        continue;
                    }
                };
                let _ = self.doc.set_occurrence_visible(occurrence, false);
                let components = &mut self.report.components;
                if known.is_none() {
                    self.components.add_inserted(key, component);
                    components.inserted.push(InsertedComponent {
                        component,
                        document: urn.to_owned(),
                        key: key.to_owned(),
                        document_guid: text(e, "document_guid").map(str::to_owned),
                        component_object: 0,
                        occurrences: 0,
                    });
                }
                if let Some(i) = components
                    .inserted
                    .iter_mut()
                    .find(|i| i.component == component)
                {
                    i.occurrences += 1;
                }
                components.derived.push(DerivedSource {
                    item: index,
                    entity: j,
                    component,
                    occurrence,
                    placement: local,
                });
            }
        }
    }

    /// A derive as Mitcad's `derive` feature: the bodies of the copies of
    /// their documents, linked through the hidden occurrences placed for
    /// them.
    pub(crate) fn derive(
        &mut self,
        index: i64,
        item: &TimelineItem,
    ) -> Result<Vec<Candidate>, String> {
        let list = entities(item).ok_or("the derive record was not decoded")?;
        if list.is_empty() {
            return Err(BRINGS_NOTHING.to_owned());
        }
        // Each entity's body and its link, in order.
        let mut copies: Vec<(String, Value)> = Vec::new();
        for (j, e) in list.iter().enumerate() {
            let kind = e.get("kind").and_then(Value::as_str).unwrap_or("entity");
            if kind != "body" {
                return Err(format!(
                    "derives a {kind} of another document: only bodies are translated"
                ));
            }
            let document = text(e, "urn").unwrap_or("?");
            let source = self
                .report
                .components
                .derived
                .iter()
                .find(|s| s.item == index && s.entity == j)
                .cloned()
                .ok_or_else(|| format!("its body of {document} has no placement"))?;
            if !self.components.is_filled(source.component) {
                return Err(format!(
                    "its bodies come from {document}, which was not imported (its file was not \
                     found among the references)"
                ));
            }
            let guid = text(e, "document_guid").unwrap_or("").to_lowercase();
            let object = e
                .pointer("/_f3d/object_id")
                .and_then(Value::as_u64)
                .ok_or("an entity's object was not decoded")?;
            let assembly = self.doc.assembly();
            let candidates = self
                .report
                .components
                .external_entities
                .get(&format!("{guid}|{object}"))
                .ok_or_else(|| {
                    format!("body {object} of {document} was not matched to a body of its import")
                })?;
            // The copy's body and its path through the hidden occurrence.
            let mut found = None;
            for body in candidates {
                let Some(component) = self.doc.body_component(*body) else {
                    continue;
                };
                if let Some(path) = assembly
                    .paths_to(component)
                    .into_iter()
                    .find(|p| p.contains(&source.occurrence))
                {
                    found = Some((*body, path));
                    break;
                }
            }
            let (body, path) = found.ok_or_else(|| {
                format!("body {object} of {document} is not in the copy placed for the derive")
            })?;
            // The occurrence goes where the body's own occurrence is: the
            // levels inside the copy are its own.
            let at = path
                .iter()
                .position(|o| *o == source.occurrence)
                .expect("found");
            let inside = self.doc.path_transform(&path[at + 1..]);
            let wanted = source.placement.after(&inverse(&inside));
            let current = assembly
                .occurrence(source.occurrence)
                .map_or(Transform::IDENTITY, |o| o.transform);
            if !same(&current, &wanted) {
                self.doc
                    .set_occurrence_transform(source.occurrence, wanted, false)
                    .map_err(|e| e.to_string())?;
            }
            copies.push((
                body.to_string(),
                json!({"source": OccurrencePath(path).to_string()}),
            ));
        }
        let mut c = Candidate {
            core: CandidateCore::of(derives(copies)),
            ..Candidate::new(Value::Null)
        };
        c.input_provenance = InputProvenance::Read;
        Ok(vec![c])
    }
}

/// The `derive` definitions of a derive's copies (body, link), in order:
/// one, unless a body comes more than once (the same body of another
/// document at several placements, mitcad#2493: a derive lists each body
/// once, with one link), then one more for each repeat, each in the order
/// of the entities.
fn derives(copies: Vec<(String, Value)>) -> Vec<Value> {
    let mut groups: Vec<(Vec<String>, Map<String, Value>)> = Vec::new();
    for (body, link) in copies {
        let at = match groups
            .iter()
            .position(|(bodies, _)| !bodies.contains(&body))
        {
            Some(at) => at,
            None => {
                groups.push((Vec::new(), Map::new()));
                groups.len() - 1
            }
        };
        let (bodies, links) = &mut groups[at];
        bodies.push(body.clone());
        links.insert(body, link);
    }
    groups
        .into_iter()
        .map(|(bodies, links)| json!({"type": "derive", "bodies": bodies, "links": links}))
        .collect()
}

/// Whether two placements are the same within rounding.
fn same(a: &Transform, b: &Transform) -> bool {
    let (ra, rb) = (
        mitcad_model::assembly::matrix_rows(a),
        mitcad_model::assembly::matrix_rows(b),
    );
    ra.iter()
        .flatten()
        .zip(rb.iter().flatten())
        .all(|(x, y)| (x - y).abs() < 1e-9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_at_two_placements_takes_a_derive_each() {
        let link = |s: &str| json!({"source": s});
        // Distinct bodies: one derive.
        let one = derives(vec![
            ("F1.b0".into(), link("O1/O2")),
            ("F1.b1".into(), link("O1/O2")),
        ]);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0]["bodies"], json!(["F1.b0", "F1.b1"]));
        // The same body through two occurrences (mitcad#2493): two derives,
        // each with its own link.
        let two = derives(vec![
            ("F1.b0".into(), link("O3/O4")),
            ("F1.b0".into(), link("O5/O6")),
            ("F1.b1".into(), link("O5/O6")),
        ]);
        assert_eq!(two.len(), 2);
        assert_eq!(two[0]["bodies"], json!(["F1.b0", "F1.b1"]));
        assert_eq!(two[0]["links"]["F1.b0"]["source"], "O3/O4");
        assert_eq!(two[1]["bodies"], json!(["F1.b0"]));
        assert_eq!(two[1]["links"]["F1.b0"]["source"], "O5/O6");
        assert!(two.iter().all(|d| d["type"] == "derive"));
    }
}
