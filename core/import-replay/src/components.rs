// SPDX-License-Identifier: MIT
//! The file's components and occurrences as Mitcad's (F6).
//!
//! The dump's components become Mitcad components placed by its occurrence
//! tree before the timeline is replayed. Top-level occurrences of the
//! stream decoder carry their `transform` (relative to the root), nested
//! ones `_f3d.local_transform` (relative to their parent component, T2);
//! an external dump's `transform2` places a full path in the design, which is
//! made relative to the parent occurrence. Occurrences of components that
//! live in other documents (`isReferencedComponent`: inserted parts,
//! fasteners) are placed as occurrences of empty components, one per
//! component of another document, named after the item that inserted it:
//! their bodies are not in the file, but joints and captured positions
//! name them (mitcad#75).
//!
//! Every timeline item then goes into its component (the file has a single
//! timeline): an external dump names it (`component`); for the stream
//! decoder the ASM history tells whose bodies the item's operation changed
//! (each component keeps its bodies in its own blob, in its own
//! coordinates), sketches and construction geometry follow the first
//! feature that uses them, and items with neither follow the next item
//! that has a component (else the one before).

use std::collections::{BTreeMap, HashMap};

use mitcad_dump::{Dump, Mat4, OccurrenceNode, TimelineItem};
use mitcad_model::assembly::{Assembly, Occurrence, from_rows, inverse};
use mitcad_model::{BodyUid, ComponentUid, Document, Kernel, OccurrenceUid, Transform};
use serde::Serialize;
use serde_json::Value;

use crate::history::Oracle;

/// What the import made of the file's components.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ComponentReport {
    /// Components made (the root excluded).
    pub components: usize,
    /// Occurrences placed.
    pub occurrences: usize,
    /// Occurrences of components in other documents, placed as
    /// occurrences of empty components (not counted above).
    pub external: usize,
    /// Of them, those whose components were filled from their documents'
    /// files (mitcad#196).
    pub linked: usize,
    /// Captured positions of occurrences inside those components that
    /// placed the copied occurrences.
    pub placed_inside: usize,
    /// Components made for occurrences of a component of another document
    /// that the captured positions arrange differently inside (each is
    /// filled with a copy of its own, mitcad#196).
    pub copies: usize,
    /// Timeline items by the component they went into (the file's names).
    pub items: BTreeMap<String, usize>,
    /// The empty components standing for components of other documents,
    /// for the import to fill from those documents' files (mitcad#196).
    #[serde(skip)]
    pub inserted: Vec<InsertedComponent>,
    /// The occurrences by the GUIDs the file gives them (occurrence paths
    /// of a design that inserts this one name them so, mitcad#196). A GUID
    /// names several where the copies of a document placed with different
    /// arrangements inside have each their own (the one whose parent is
    /// the path's component is meant).
    #[serde(skip)]
    pub occurrence_guids: HashMap<String, Vec<OccurrenceUid>>,
    /// The occurrences by their object ids in the file (the first placed
    /// where a component placed several times shares one): a design that
    /// inserts this one names the levels of its paths inside it so in the
    /// placements it stores (mitcad#1892).
    #[serde(skip)]
    pub occurrence_objects: HashMap<u64, OccurrenceUid>,
    /// The captured positions of occurrences inside the components of other
    /// documents, which the replay leaves out (mitcad#196).
    #[serde(skip)]
    pub positions_inside: Vec<crate::InsidePosition>,
    /// The empty components that were filled from their documents'
    /// files ([`crate::references::Linker::fill`]), for the items that name
    /// things inside them ([`crate::prefill`]).
    #[serde(skip)]
    pub filled: Vec<ComponentUid>,
    /// What filling the components from their documents did, once the
    /// import has ended ([`crate::prefill`]).
    #[serde(skip)]
    pub link: crate::references::LinkOutcome,
    /// The bodies of the filled components that match the stored bodies of
    /// their documents, by `<document GUID>|<stored body>` (`_f3d.document`,
    /// `_f3d.stored`): where a design's reference to a body of another
    /// document is found (mitcad#196): one body per copy of the document.
    #[serde(skip)]
    pub external_bodies: HashMap<String, Vec<mitcad_model::BodyUid>>,
    /// Where the derives' entities are (mitcad#324): the hidden occurrences
    /// of the components standing for their documents.
    #[serde(skip)]
    pub derived: Vec<crate::derive::DerivedSource>,
    /// The bodies of the filled components by the body objects their
    /// documents name them by, as `<document GUID>|<object id>`: what a
    /// derive names (mitcad#324); one body per copy of the document.
    #[serde(skip)]
    pub external_entities: HashMap<String, Vec<mitcad_model::BodyUid>>,
}

/// An empty component standing for a component of another document
/// ([`ComponentReport::inserted`]).
#[derive(Debug, Clone, PartialEq)]
pub struct InsertedComponent {
    pub component: ComponentUid,
    /// The other document's URN (`_f3d.external_document`).
    pub document: String,
    /// Its key in this design (`_f3d.external_key`).
    pub key: String,
    /// The GUID that tells its file (`_f3d.external_documents[].document_guid`).
    pub document_guid: Option<String>,
    /// The component's object id in the other document.
    pub component_object: u64,
    /// Its occurrences placed.
    pub occurrences: usize,
}

/// The file's components and the items in them, in Mitcad's terms.
#[derive(Debug, Default, Clone)]
pub struct Components {
    /// By the component's object id in the file (`_f3d.object_id`).
    by_object: HashMap<u64, ComponentUid>,
    /// By the component's name.
    by_name: HashMap<String, ComponentUid>,
    /// Timeline item index → its component.
    items: HashMap<i64, ComponentUid>,
    /// Items whose component the dump or the history gives (not inferred
    /// from their neighbours).
    known: std::collections::HashSet<i64>,
    /// Items whose owning component the decoder gives (mitcad#37).
    owned: std::collections::HashSet<i64>,
    /// Occurrences by their object id in the file (`_f3d.object_id` of the
    /// occurrence tree; joint paths name them, mitcad#55).
    occurrences: HashMap<u64, OccurrenceUid>,
    /// The empty components standing for components of other documents,
    /// by document key and component object.
    inserted: HashMap<(String, u64), ComponentUid>,
    /// Of those, the components filled from their documents' files
    /// (known to the items that name things inside them, `prefill.rs`).
    filled: std::collections::HashSet<ComponentUid>,
}

impl Components {
    /// The component of a body of the history (by its component's object
    /// id); the root when unknown.
    pub fn of_body(&self, component: Option<u64>) -> ComponentUid {
        component
            .and_then(|c| self.by_object.get(&c))
            .copied()
            .unwrap_or(ComponentUid::ROOT)
    }

    /// The component of a body of the history, when its component's object
    /// id is one of the file's components.
    pub fn known_body(&self, component: Option<u64>) -> Option<ComponentUid> {
        component.and_then(|c| self.by_object.get(&c)).copied()
    }

    /// The component of a timeline item.
    pub fn of_item(&self, index: i64) -> ComponentUid {
        self.items
            .get(&index)
            .copied()
            .unwrap_or(ComponentUid::ROOT)
    }

    /// Whether the dump or the history gives the item's component.
    pub fn is_known(&self, index: i64) -> bool {
        self.known.contains(&index)
    }

    /// Whether the decoder gives the component that owns the item, so that
    /// its component is not a guess.
    pub fn is_owned(&self, index: i64) -> bool {
        self.owned.contains(&index)
    }

    /// The component of the file's component object `object`.
    pub fn of_object(&self, object: u64) -> Option<ComponentUid> {
        self.by_object.get(&object).copied()
    }

    /// The occurrence the file's occurrence object `object` became.
    pub fn occurrence(&self, object: u64) -> Option<OccurrenceUid> {
        self.occurrences.get(&object).copied()
    }

    /// Whether `component` stands for a component of another document.
    pub fn is_inserted(&self, component: ComponentUid) -> bool {
        self.inserted.values().any(|c| *c == component)
    }

    /// Registers a component made for some of the occurrences of the
    /// inserted component `of` (`references::split_variants`).
    pub(crate) fn add_inserted_copy(&mut self, of: ComponentUid, component: ComponentUid) {
        let found = self
            .inserted
            .iter()
            .find(|(_, c)| **c == of)
            .map(|((key, object), _)| (key.clone(), *object));
        if let Some((key, object)) = found {
            let n = self.inserted.len();
            self.inserted
                .insert((format!("{key}#{n}"), object), component);
        }
    }

    /// Whether `component` stands for a component of another document
    /// and has been filled from that document's file.
    pub fn is_filled(&self, component: ComponentUid) -> bool {
        self.filled.contains(&component)
    }

    /// Takes the components filled from their documents as filled.
    pub fn set_filled(&mut self, filled: &[ComponentUid]) {
        self.filled.extend(filled.iter().copied());
    }

    // Derived designs (mitcad#324).

    /// The empty component standing for a component of the document of
    /// key `key`, if one was made.
    pub(crate) fn inserted_of_key(&self, key: &str) -> Option<ComponentUid> {
        self.inserted
            .iter()
            .filter(|((k, _), _)| k == key)
            .min_by_key(|((_, object), _)| *object)
            .map(|(_, c)| *c)
    }

    /// Registers the component made for the derived entities of the
    /// document of key `key`.
    pub(crate) fn add_inserted(&mut self, key: &str, component: ComponentUid) {
        self.inserted.insert((key.to_owned(), 0), component);
    }
}

/// A matrix of the dump (cm) as a rigid transform in millimetres; None when it
/// is not a rotation and a translation.
pub fn transform(m: &Mat4) -> Option<Transform> {
    let rows: Vec<Vec<f64>> = (0..3)
        .map(|r| vec![m[r][0], m[r][1], m[r][2], m[r][3] * 10.0])
        .collect();
    from_rows(&rows).ok()
}

/// Makes the dump's components and occurrences in the document.
/// The part numbers and descriptions the file gives its components, as
/// the components' properties with the source `f3d` (mitcad#496), in one
/// step (the import's own when it merges its steps). The count added.
pub(crate) fn import_properties<K: Kernel>(
    doc: &mut Document<K>,
    dump: &Dump,
    components: &Components,
) -> usize {
    use mitcad_model::properties::{BaseType, Property, PropertyOwner, PropertyValue, Source};
    let mut items = Vec::new();
    for c in dump.components.iter().flatten() {
        let object = c.f3d.as_ref().and_then(|f| f.object_id);
        let Some(uid) = object
            .and_then(|o| components.of_object(o))
            .or_else(|| (c.is_root == Some(true)).then_some(ComponentUid::ROOT))
        else {
            continue;
        };
        for (key, value) in [
            ("Part Number", &c.part_number),
            ("Description", &c.description),
        ] {
            if let Some(text) = value.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                items.push((
                    PropertyOwner::Component(uid),
                    "Component".to_owned(),
                    Property::new(
                        key,
                        BaseType::Text,
                        PropertyValue::Text(text.to_owned()),
                        Source::F3d,
                    ),
                ));
            }
        }
    }
    if items.is_empty() {
        return 0;
    }
    doc.import_properties("Component properties", items)
}

pub(crate) fn import_components<K: Kernel>(
    doc: &mut Document<K>,
    dump: &Dump,
    names: &HashMap<u64, String>,
    report: &mut ComponentReport,
    warnings: &mut Vec<String>,
) -> Components {
    let mut out = Components::default();
    let root_name = dump
        .document
        .as_ref()
        .and_then(|d| d.root_component.clone());
    // The root takes the design's name.
    if let Some(name) = root_name.as_deref().filter(|n| !n.trim().is_empty())
        && let Err(e) = doc.rename_component(ComponentUid::ROOT, name)
    {
        warnings.push(format!("root component name: {e}"));
    }
    // The root: flagged, or named as the document's root component.
    for c in dump.components.iter().flatten() {
        let name = c.name.clone().flatten();
        let object = c.f3d.as_ref().and_then(|f| f.object_id);
        let root =
            c.is_root == Some(true) || (name.is_some() && name == root_name && c.is_root.is_none());
        if root {
            if let Some(o) = object {
                out.by_object.insert(o, ComponentUid::ROOT);
            }
            if let Some(n) = name {
                out.by_name.insert(n, ComponentUid::ROOT);
            }
        }
    }
    // Names of the file's components by object id, for occurrences that
    // only name theirs (external dumps).
    let mut objects: HashMap<String, u64> = names.iter().map(|(o, n)| (n.clone(), *o)).collect();
    for c in dump.components.iter().flatten() {
        if let (Some(n), Some(o)) = (
            c.name.clone().flatten(),
            c.f3d.as_ref().and_then(|f| f.object_id),
        ) {
            objects.insert(n, o);
        }
    }
    // The items that made occurrences (inserts, fasteners, pastes), by
    // occurrence object: they name the components of other documents.
    let mut makers: HashMap<u64, String> = HashMap::new();
    for item in dump.timeline_items() {
        let Some(name) = item.name().filter(|n| !n.trim().is_empty()) else {
            continue;
        };
        let detail = item
            .detail
            .as_ref()
            .and_then(|d| serde_json::to_value(d).ok());
        if let Some([Value::Number(o)]) = detail
            .as_ref()
            .and_then(|d| d.pointer("/occurrence/_f3d/path"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            && let Some(o) = o.as_u64()
            && item.object_type() != Some("GroundOccurrence")
        {
            makers.entry(o).or_insert_with(|| name.to_owned());
        }
    }
    // The GUIDs that tell the files of the other documents (mitcad#196).
    let document_guids: HashMap<String, String> = dump
        .f3d
        .as_ref()
        .and_then(|f| f.external_documents.as_ref())
        .into_iter()
        .flatten()
        .filter_map(|x| Some((x.key.clone()?, x.document_guid.clone()?)))
        .collect();
    let mut placer = Placer {
        doc,
        out: &mut out,
        objects: &objects,
        makers: &makers,
        document_guids: &document_guids,
        report,
        warnings,
    };
    for node in dump.occurrences.iter().flatten() {
        placer.place(node, ComponentUid::ROOT, Transform::IDENTITY, 0);
    }
    out
}

struct Placer<'a, K: Kernel> {
    doc: &'a mut Document<K>,
    out: &'a mut Components,
    objects: &'a HashMap<String, u64>,
    /// The name of the item that made each occurrence object.
    makers: &'a HashMap<u64, String>,
    /// The GUIDs of the other documents by their keys.
    document_guids: &'a HashMap<String, String>,
    report: &'a mut ComponentReport,
    warnings: &'a mut Vec<String>,
}

/// The name of an empty component standing for a component of another
/// document when no item names it.
const INSERTED: &str = "Inserted component";

impl<K: Kernel> Placer<'_, K> {
    /// Places an occurrence node in `parent` (whose placement in the design
    /// is `parent_world`), then its children in its component.
    fn place(
        &mut self,
        node: &OccurrenceNode,
        parent: ComponentUid,
        parent_world: Transform,
        depth: usize,
    ) {
        if depth > 32 {
            return;
        }
        let f3d = node.f3d.as_ref();
        let occurrence_object = f3d.and_then(|f| f.object_id);
        let external = node.is_referenced_component == Some(true);
        let name = if external {
            occurrence_object.and_then(|o| self.makers.get(&o).cloned())
        } else {
            node.component.clone().flatten()
        };
        let object = f3d
            .and_then(|f| f.component_object)
            .or_else(|| name.as_ref().and_then(|n| self.objects.get(n).copied()));
        // Its placement in the parent: an external dump's full path made
        // relative, or the decoder's own transform.
        let world_given = node.transform2.as_ref().and_then(transform);
        let local = match world_given {
            Some(world) => inverse(&parent_world).after(&world),
            None => {
                let matrix = if depth == 0 {
                    node.transform
                } else {
                    f3d.and_then(|f| f.local_transform)
                };
                match matrix {
                    Some(m) => transform(&m).unwrap_or_else(|| {
                        self.warnings.push(format!(
                            "occurrence of {}: its transform is not rigid; placed at the origin",
                            name.as_deref().unwrap_or("?")
                        ));
                        Transform::IDENTITY
                    }),
                    None => Transform::IDENTITY,
                }
            }
        };
        if external {
            self.place_inserted(node, name.as_deref(), parent, local);
            return;
        }
        let world = parent_world.after(&local);
        let known = object
            .and_then(|o| self.out.by_object.get(&o))
            .or_else(|| name.as_ref().and_then(|n| self.out.by_name.get(n)))
            .copied();
        let made = match known {
            Some(c) if c.is_root() => return,
            Some(c) => self.doc.add_occurrence(c, parent, local).map(|o| (c, o)),
            None => self.doc.add_component(name.as_deref(), parent, local),
        };
        let (component, occurrence) = match made {
            Ok(made) => made,
            Err(e) => {
                self.warnings.push(format!(
                    "occurrence of {}: {e}",
                    name.as_deref().unwrap_or("?")
                ));
                return;
            }
        };
        if known.is_none() {
            self.report.components += 1;
            if let Some(o) = object {
                self.out.by_object.insert(o, component);
            }
            if let Some(n) = &name {
                self.out.by_name.entry(n.clone()).or_insert(component);
            }
        }
        self.report.occurrences += 1;
        self.placed(node, occurrence);
        // A component placed again keeps the children it already has.
        if known.is_some() {
            return;
        }
        for child in node.children.iter().flatten() {
            self.place(child, component, world, depth + 1);
        }
    }

    /// What every placed occurrence takes from its node: its object id
    /// (joint paths name it), its ground flag and its light bulb.
    fn placed(&mut self, node: &OccurrenceNode, occurrence: OccurrenceUid) {
        if let Some(o) = node.f3d.as_ref().and_then(|f| f.object_id) {
            self.out.occurrences.entry(o).or_insert(occurrence);
            self.report
                .occurrence_objects
                .entry(o)
                .or_insert(occurrence);
        }
        for g in node.f3d.iter().flat_map(|f| f.guids.iter().flatten()) {
            let list = self
                .report
                .occurrence_guids
                .entry(g.to_lowercase())
                .or_default();
            if !list.contains(&occurrence) {
                list.push(occurrence);
            }
        }
        if node.is_grounded == Some(true) {
            let _ = self.doc.set_occurrence_grounded(occurrence, true);
        }
        if node.is_light_bulb_on.or(node.is_visible) == Some(false) {
            let _ = self.doc.set_occurrence_visible(occurrence, false);
        }
    }

    /// Places an occurrence of a component of another document as an
    /// occurrence of the empty component standing for it (one per
    /// component of each document), named after the item that inserted
    /// it (`name`).
    fn place_inserted(
        &mut self,
        node: &OccurrenceNode,
        name: Option<&str>,
        parent: ComponentUid,
        local: Transform,
    ) {
        let f3d = node.f3d.as_ref();
        let key = (
            f3d.and_then(|f| f.external_key.clone()).unwrap_or_default(),
            f3d.and_then(|f| f.component_object).unwrap_or_default(),
        );
        let known = self.out.inserted.get(&key).copied();
        let made = match known {
            Some(c) => self.doc.add_occurrence(c, parent, local).map(|o| (c, o)),
            None => self
                .doc
                .add_component(Some(name.unwrap_or(INSERTED)), parent, local),
        };
        let (component, occurrence) = match made {
            Ok(made) => made,
            Err(e) => {
                self.warnings.push(format!(
                    "occurrence of a component of another document ({}): {e}",
                    name.unwrap_or(INSERTED)
                ));
                return;
            }
        };
        if known.is_none() {
            self.report.inserted.push(InsertedComponent {
                component,
                document: f3d
                    .and_then(|f| f.external_document.clone())
                    .unwrap_or_default(),
                key: key.0.clone(),
                document_guid: self.document_guids.get(&key.0).cloned(),
                component_object: key.1,
                occurrences: 0,
            });
        }
        if let Some(i) = self
            .report
            .inserted
            .iter_mut()
            .find(|i| i.component == component)
        {
            i.occurrences += 1;
        }
        self.out.inserted.entry(key).or_insert(component);
        self.report.external += 1;
        self.placed(node, occurrence);
    }
}

/// The earlier items an item refers to (`timeline_index`,
/// `sketch_timeline_index` anywhere in it).
pub(crate) fn references(value: &Value, out: &mut Vec<i64>) {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                if matches!(key.as_str(), "timeline_index" | "sketch_timeline_index")
                    && let Some(i) = v.as_i64()
                {
                    out.push(i);
                } else {
                    references(v, out);
                }
            }
        }
        Value::Array(items) => {
            for v in items {
                references(v, out);
            }
        }
        _ => {}
    }
}

/// Where a component is placed in the design, when it is placed once (the
/// root: as it is), each occurrence on its path placed in its parent by
/// `place`.
pub(crate) fn placement(
    assembly: &Assembly,
    component: ComponentUid,
    place: impl Fn(&Occurrence) -> Transform,
) -> Option<Transform> {
    let paths = assembly.paths_to(component);
    let [path] = paths.as_slice() else {
        return None;
    };
    let mut placed = Transform::IDENTITY;
    for o in path {
        placed = placed.after(&place(assembly.occurrence(*o)?));
    }
    Some(placed)
}

/// Where a component is placed in the design: [`placement`], and for a
/// component placed several times (an occurrence pattern) the placement of its
/// first occurrence, the original the file's features are made in.
pub(crate) fn original_placement(
    assembly: &Assembly,
    component: ComponentUid,
    place: impl Fn(&Occurrence) -> Transform,
) -> Option<Transform> {
    let paths = assembly.paths_to(component);
    let path = paths.iter().min()?;
    let mut placed = Transform::IDENTITY;
    for o in path {
        placed = placed.after(&place(assembly.occurrence(*o)?));
    }
    Some(placed)
}

/// Sketches and construction geometry, which follow the features that use
/// them.
fn follows_users(item: &TimelineItem) -> bool {
    item.object_type()
        .is_some_and(|t| t == "Sketch" || t.starts_with("Construction"))
}

/// Puts every timeline item into a component (see the module docs).
/// `place` places an occurrence in its parent at an item of the timeline
/// (the file's captured positions, mitcad#86).
pub(crate) fn assign_items<S: Clone>(
    dump: &Dump,
    assembly: &Assembly,
    place: &dyn Fn(&Occurrence, i64) -> Transform,
    oracle: &mut Oracle<'_, S>,
    components: &mut Components,
    report: &mut ComponentReport,
) {
    let items = dump.timeline_items();
    let index_of = |position: usize| items[position].index.unwrap_or(position as i64);
    // Known from the item itself: its name (external dumps) or the bodies its
    // operation changed (history).
    let mut known: HashMap<i64, ComponentUid> = HashMap::new();
    // The component that owns each item, from the decoder (mitcad#37).
    let mut owners: HashMap<i64, ComponentUid> = HashMap::new();
    // Only components whose bodies the history keeps apart own items: the
    // fallbacks put the bodies of the others where their blobs are, and
    // the items go where the bodies are, as without an owner.
    let with_history = oracle.history_components();
    for (position, item) in items.iter().enumerate() {
        let index = index_of(position);
        let decoded = item.f3d.as_ref().and_then(|f| f.component);
        let owner = decoded
            .filter(|o| with_history.as_ref().is_none_or(|h| h.contains(o)))
            .and_then(|o| components.by_object.get(&o).copied());
        // (The decoder's name of the owner is no more than its object id.)
        let named = item
            .component
            .clone()
            .flatten()
            .filter(|_| decoded.is_none())
            .and_then(|n| components.by_name.get(&n).copied());
        let mut from_history = || {
            let changed: Vec<ComponentUid> = oracle
                .item_components(index)?
                .into_iter()
                .map(|o| components.of_body(Some(o)))
                .collect();
            let first = *changed.first()?;
            changed.iter().all(|c| *c == first).then_some(first)
        };
        // The decoder's owner comes after the history: a feature of one
        // component may change another's bodies (the file's assembly
        // context), and goes where they are.
        let component = match owner {
            Some(o) => {
                owners.insert(index, o);
                // (Sketches and construction geometry: below.)
                from_history().or((!follows_users(item)).then_some(o))
            }
            None => named.or_else(from_history),
        };
        if let Some(c) = component {
            known.insert(index, c);
        }
        if crate::tracing() {
            eprintln!(
                "import: item {index} {}: changed components {:?}, component {:?}",
                item.name().unwrap_or("?"),
                oracle.item_components(index),
                known.get(&index)
            );
        }
    }
    components.known = known.keys().copied().collect();
    components.owned = owners.keys().copied().collect();
    // Sketches and construction geometry follow the first item that uses
    // them.
    let mut used: HashMap<i64, ComponentUid> = HashMap::new();
    // The index of each such item's first user.
    let mut first_use: HashMap<i64, i64> = HashMap::new();
    for position in (0..items.len()).rev() {
        let index = index_of(position);
        let Some(c) = known.get(&index).or_else(|| used.get(&index)).copied() else {
            continue;
        };
        let Ok(value) = serde_json::to_value(&items[position]) else {
            continue;
        };
        let mut referenced = Vec::new();
        references(&value, &mut referenced);
        for r in referenced.into_iter().filter(|r| *r < index) {
            if !known.contains_key(&r) {
                used.insert(r, c);
                first_use.insert(r, index);
            }
        }
    }
    // A sketch or construction item whose owner the decoder gives (its
    // geometry is in the owner's coordinates) goes there when no feature
    // uses it, or when its first user's component is placed elsewhere than
    // its owner where the user is in the timeline (the user then gets a
    // copy moved by the two placements); else it follows its user, as
    // without an owner.
    for position in 0..items.len() {
        let index = index_of(position);
        let Some(&owner) = owners.get(&index) else {
            continue;
        };
        if known.contains_key(&index) {
            continue;
        }
        let at = first_use.get(&index).copied().unwrap_or(index);
        let placed = |c: ComponentUid| placement(assembly, c, |o| place(o, at));
        let elsewhere = |user: ComponentUid| match (placed(owner), placed(user)) {
            (Some(o), Some(u)) => !inverse(&u).after(&o).is_identity(),
            _ => false,
        };
        match used.get(&index) {
            Some(&user) if user == owner || !elsewhere(user) => {}
            _ => {
                used.insert(index, owner);
            }
        }
    }
    // The rest follow the next item with a component, else the one before.
    let mut next: Option<ComponentUid> = None;
    let mut assigned: Vec<Option<ComponentUid>> = vec![None; items.len()];
    for position in (0..items.len()).rev() {
        let index = index_of(position);
        let own = known.get(&index).or_else(|| used.get(&index)).copied();
        if own.is_some() {
            next = own;
        }
        assigned[position] = own.or(next);
    }
    let mut previous = ComponentUid::ROOT;
    for (position, a) in assigned.iter().enumerate() {
        let c = a.unwrap_or(previous);
        previous = c;
        components.items.insert(index_of(position), c);
    }
    // The report counts items by the file's component names.
    let names: HashMap<ComponentUid, String> = components
        .by_name
        .iter()
        .map(|(n, c)| (*c, n.clone()))
        .collect();
    for c in components.items.values() {
        let name = names.get(c).cloned().unwrap_or_else(|| c.to_string());
        *report.items.entry(name).or_default() += 1;
    }
}

impl<K: crate::ImportKernel> crate::Importer<'_, K> {
    /// A component made from bodies (mitcad#156) as Mitcad's: the item's
    /// operation is the one that put its bodies into the new component's
    /// history (`_f3d.made_result_no`); the replay's bodies of the item's
    /// own component that are those bodies move into the component placed
    /// for the file's by a `component_from_bodies` feature
    /// ([`mitcad_model::Document::move_made_component`]). The history of
    /// the item's own component keeps them as they were, though the design
    /// no longer has them there: they leave its states from the item's on
    /// ([`Oracle::retire`]). False (nothing done) when the history does not
    /// settle it: the item then falls back as before.
    pub(crate) fn component_from_bodies(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
    ) -> bool {
        use crate::history::{Sig, bodies_distance};
        let name = crate::item_name(item);
        let not = |why: &str| {
            if crate::tracing() {
                eprintln!("import: {name}: not moved: {why}");
            }
            false
        };
        // A sibling item of one that made all the components of its state.
        if let Some((uid, note)) = self.made_ahead.remove(&index) {
            self.features.insert(index, uid);
            let owner = item
                .f3d
                .as_ref()
                .and_then(|f| f.component)
                .and_then(|o| self.components.of_object(o))
                .unwrap_or(ComponentUid::ROOT);
            let report = &mut self.report.items[at];
            report.features.push(uid.to_string());
            report.verified = Some(true);
            report.component = (!owner.is_root()).then(|| self.doc.assembly().name(owner));
            self.note(at, crate::report::Outcome::Parametric, note);
            return true;
        }
        if !self.pending.is_empty() {
            return not("items before it are pending");
        }
        let Some(state) = self.oracle.item_state(index) else {
            return not("no state of its own");
        };
        let Some(mut made) = self.oracle.item_components(index) else {
            return not("not a component's history");
        };
        made.sort_unstable();
        made.dedup();
        // The items that made them together: the file's state holds the
        // components of all of them, each item takes the next one.
        let dump = self.dump;
        let mut siblings: Vec<(usize, String)> = Vec::new();
        if made.len() > 1 {
            for (position, other) in dump.timeline_items().iter().enumerate() {
                let j = other.index.unwrap_or(position as i64);
                if other.object_type() == Some("ComponentFromBodies")
                    && self.oracle.state_of_item(j) == Some(state)
                    && self.oracle.item_components(j).is_some_and(|mut c| {
                        c.sort_unstable();
                        c.dedup();
                        c == made
                    })
                {
                    siblings.push((position, crate::item_name(other)));
                }
            }
            if siblings.first().map(|(p, _)| *p) != Some(at) {
                return not("a component made together with an item before it");
            }
        }
        let owner = item
            .f3d
            .as_ref()
            .and_then(|f| f.component)
            .and_then(|o| self.components.of_object(o));
        let Some(owner) = owner else {
            return not("its components are not known");
        };
        let mut targets = Vec::new();
        for m in &made {
            let Some(target) = self.components.known_body(Some(*m)) else {
                return not("its components are not known");
            };
            targets.push(target);
        }
        // Into components the replay has not put anything in yet.
        for &target in &targets {
            if target == owner {
                return not("the new component is its own");
            }
            if let Some(f) = self.doc.features().find(|f| f.component == target) {
                return not(&format!("the new component has {} already", f.name));
            }
            if self
                .current_bodies()
                .iter()
                .any(|(uid, _)| self.doc.body_component(*uid) == Some(target))
            {
                return not("the new component has bodies already");
            }
        }
        let kernel = self.doc.kernel();
        let before = state
            .checked_sub(1)
            .and_then(|s| self.oracle.try_sigs(kernel, s));
        let current = self.current_sigs();
        if before.is_none_or(|b| bodies_distance(&b, &current).is_none()) {
            return not("the bodies are not the state before it");
        }
        let Ok(stored) = self.oracle.state(kernel, state).map(<[_]>::to_vec) else {
            return not("its state was not rebuilt");
        };
        // Each moved body: a body of the replay's in the item's component,
        // and the same body as the file's own component's history keeps it.
        let mine: Vec<(BodyUid, Sig)> = self
            .current_bodies()
            .into_iter()
            .filter(|(uid, _)| self.doc.body_component(*uid) == Some(owner))
            .collect();
        let mut groups: Vec<Vec<BodyUid>> = Vec::new();
        let mut retired = Vec::new();
        for &target in &targets {
            let moved: Vec<Sig> = stored
                .iter()
                .filter(|(b, _)| self.components.of_body(b.component) == target)
                .map(|(_, s)| *s)
                .collect();
            if moved.is_empty() {
                return not("no body of the new component in its state");
            }
            let mut bodies: Vec<BodyUid> = Vec::new();
            for sig in &moved {
                // The same solid, else the nearest within the matching
                // tolerance: a body imported within the geometric tolerance
                // stays as the replay made it (mitcad#285).
                let free = |uid: &BodyUid| {
                    !bodies.contains(uid) && !groups.iter().flatten().any(|g| g == uid)
                };
                let Some((uid, _)) = mine
                    .iter()
                    .find(|(uid, s)| free(uid) && s.same(sig))
                    .or_else(|| {
                        mine.iter()
                            .filter(|(uid, s)| free(uid) && s.matches(sig))
                            .min_by(|a, b| sig.distance(&a.1).total_cmp(&sig.distance(&b.1)))
                    })
                else {
                    return not("a body of the new component is none of its own component's");
                };
                bodies.push(*uid);
                if let Some(id) = stored.iter().find_map(|(b, s)| {
                    (self.components.of_body(b.component) == owner && s.same(sig))
                        .then_some(b.id)
                        .flatten()
                        .filter(|id| !retired.contains(id))
                }) {
                    retired.push(id);
                }
            }
            groups.push(bodies);
        }
        // The state as the design has it: without the copies the item's
        // own component's history keeps.
        let expected: Vec<Sig> = stored
            .iter()
            .filter(|(b, _)| b.id.is_none_or(|id| !retired.contains(&id)))
            .map(|(_, s)| *s)
            .collect();
        let depth = self.doc.undo_depth();
        // One feature per component; the first sibling item has the first.
        let mut made_features = Vec::new();
        for (k, (&target, bodies)) in targets.iter().zip(&groups).enumerate() {
            let feature_name = siblings.get(k).map_or(name.as_str(), |(_, n)| n.as_str());
            let def = serde_json::json!({"type": "component_from_bodies",
                "bodies": bodies.iter().map(ToString::to_string).collect::<Vec<_>>()});
            let added = self
                .add_in(&def, Some(feature_name), owner)
                .and_then(|uid| {
                    self.doc
                        .move_made_component(uid, target)
                        .map(|()| uid)
                        .map_err(|e| e.to_string())
                });
            match added {
                Ok(uid) => made_features.push(uid),
                Err(e) => {
                    self.undo_to(depth);
                    return not(&e);
                }
            }
        }
        if bodies_distance(&expected, &self.current_sigs()).is_none()
            || targets
                .iter()
                .zip(&groups)
                .any(|(&target, bodies)| self.doc.body_component(bodies[0]) != Some(target))
        {
            self.undo_to(depth);
            return not("the moved bodies do not give its state");
        }
        self.oracle.retire(state, &retired);
        self.oracle.cursor = Some(state);
        self.report.history.matched += 1;
        let notes: Vec<String> = targets
            .iter()
            .zip(&groups)
            .map(|(&target, bodies)| {
                format!(
                    "moves {} into {}",
                    if bodies.len() == 1 {
                        "its body".to_owned()
                    } else {
                        format!("its {} bodies", bodies.len())
                    },
                    self.doc.assembly().name(target)
                )
            })
            .collect();
        // The later siblings take the next components' features.
        for (k, (position, _)) in siblings.iter().enumerate().skip(1) {
            if let (Some(&uid), Some(note)) = (made_features.get(k), notes.get(k)) {
                let j = dump.timeline_items()[*position]
                    .index
                    .unwrap_or(*position as i64);
                self.made_ahead.insert(j, (uid, note.clone()));
            }
        }
        let uid = made_features[0];
        self.features.insert(index, uid);
        let report = &mut self.report.items[at];
        report.features.push(uid.to_string());
        report.verified = Some(true);
        report.component = (!owner.is_root()).then(|| self.doc.assembly().name(owner));
        let note = notes.into_iter().next().unwrap_or_default();
        self.note(at, crate::report::Outcome::Parametric, note);
        true
    }
}

/// A document whose components the design places (mitcad#196).
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalDocument {
    /// Its URN (`…?version=N`).
    pub urn: String,
    /// The GUID that tells its file (`_f3d.external_documents[].document_guid`).
    pub guid: Option<String>,
    /// The design's key for it (`_f3d.external_key`).
    pub key: Option<String>,
}

/// The documents the occurrences of a dump's components come from, in the
/// order the occurrence tree places them, each once.
pub fn external_documents(dump: &Dump) -> Vec<ExternalDocument> {
    let guids: HashMap<String, String> = dump
        .f3d
        .as_ref()
        .and_then(|f| f.external_documents.as_ref())
        .into_iter()
        .flatten()
        .filter_map(|x| Some((x.key.clone()?, x.document_guid.clone()?)))
        .collect();
    fn walk<'a>(
        nodes: impl Iterator<Item = &'a OccurrenceNode>,
        guids: &HashMap<String, String>,
        out: &mut Vec<ExternalDocument>,
    ) {
        for node in nodes {
            let f3d = node.f3d.as_ref();
            if node.is_referenced_component == Some(true)
                && let Some(urn) = f3d.and_then(|f| f.external_document.clone())
            {
                let key = f3d.and_then(|f| f.external_key.clone());
                let guid = key.as_ref().and_then(|k| guids.get(k)).cloned();
                match out.iter_mut().find(|d| d.urn == urn) {
                    Some(known) => {
                        if known.guid.is_none() {
                            known.guid = guid;
                        }
                    }
                    None => out.push(ExternalDocument { urn, guid, key }),
                }
            }
            walk(node.children.iter().flatten(), guids, out);
        }
    }
    let mut out = Vec::new();
    walk(dump.occurrences.iter().flatten(), &guids, &mut out);
    // The documents of the bodies the derives bring (mitcad#324).
    for (urn, guid, key) in crate::derive::documents(dump) {
        if !out.iter().any(|d| d.urn == urn) {
            out.push(ExternalDocument { urn, guid, key });
        }
    }
    out
}
