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

use mitcad_f3d::design::ir::{Dump, Mat4, OccurrenceNode, TimelineItem};
use mitcad_model::assembly::{Assembly, Occurrence, from_rows, inverse};
use mitcad_model::{ComponentUid, Document, Kernel, OccurrenceUid, Transform};
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
    /// Timeline items by the component they went into (the file's names).
    pub items: BTreeMap<String, usize>,
}

/// The file's components and the items in them, in Mitcad's terms.
#[derive(Debug, Default)]
pub(crate) struct Components {
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
}

/// A matrix of the dump (cm) as a rigid transform in millimetres; None when it
/// is not a rotation and a translation.
pub(crate) fn transform(m: &Mat4) -> Option<Transform> {
    let rows: Vec<Vec<f64>> = (0..3)
        .map(|r| vec![m[r][0], m[r][1], m[r][2], m[r][3] * 10.0])
        .collect();
    from_rows(&rows).ok()
}

/// Makes the dump's components and occurrences in the document.
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
    let mut placer = Placer {
        doc,
        out: &mut out,
        objects: &objects,
        makers: &makers,
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
