// SPDX-License-Identifier: MIT
//! Components of other documents filled from those documents (mitcad#196).
//!
//! The replay places each occurrence of a component of another document as
//! an occurrence of an empty component standing for it
//! ([`crate::components`]). When the caller has imported the other
//! document from its own file (`mitcad-ffi`: a local reference folder, a
//! file the user chose), [`link_references`] copies it with its history
//! into that component once the replay is done, so that every occurrence
//! shows it where the file places it. The replay itself does not see the
//! copied bodies: it matches the file's own history, which keeps none of
//! them.
//!
//! The captured positions of occurrences inside those components (left
//! out of the replay: the components were empty) then place the copied
//! occurrences, as `capture_position` features of those components at the
//! end of the timeline: each path's levels inside the other document are
//! named by the GUIDs its occurrences carry. A copied component is shared
//! by all occurrences of the other document's component, so an occurrence
//! inside it takes its last captured placement, where the paths that place
//! it then agree.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use mitcad_dump::{Dump, OccurrenceNode};
use mitcad_model::assembly::{Assembly, Occurrence, inverse, matrix_rows};
use mitcad_model::{
    BodyUid, ComponentCopy, ComponentUid, Document, FeatureDef, FeatureUid, Kernel, OccurrenceUid,
    Transform, ValueInput,
};
use serde_json::json;

use crate::captures::INSIDE;
use crate::components::InsertedComponent;
use crate::report::{DesignReport, ItemReport, Outcome};
use crate::timeline_notes::partly;

/// Another document, imported on its own, to fill the components standing
/// for its components.
pub struct Referenced<'a, K: Kernel> {
    pub document: &'a Document<K>,
    /// Its occurrences by the GUIDs the file gives them (its import's
    /// [`crate::components::ComponentReport::occurrence_guids`], which this function also
    /// extends with those of the documents it filled in it).
    pub occurrence_guids: &'a HashMap<String, Vec<OccurrenceUid>>,
    /// Its occurrences by their object ids in its file (its import's
    /// [`crate::components::ComponentReport::occurrence_objects`]): the
    /// placements the design stores for paths inside it name them so
    /// ([`outer_placements`], mitcad#1892).
    pub occurrence_objects: &'a HashMap<u64, OccurrenceUid>,
    /// The name its component takes (made unique).
    pub name: String,
    /// Its bodies by the names its import gave the stored bodies they
    /// match (`BREP.<guid>.smbh#<record>`, [`crate::report::BodyReport::file`]),
    /// which the other design names bodies of it by
    /// (`_f3d.stored`).
    pub stored_bodies: &'a HashMap<String, BodyUid>,
    /// Its bodies by the body objects its file names them by
    /// ([`DesignReport::body_entities`]), which a design that derives them
    /// names (mitcad#324).
    pub entity_bodies: &'a BTreeMap<u64, BodyUid>,
    /// Its occurrences by their object ids in its file, with those of the
    /// documents it copies in turn (its import's
    /// [`crate::components::ComponentReport::object_guids`]): the paths a
    /// design stores through it name the levels deeper than its own
    /// occurrences so (mitcad#2528). None: only its own levels are
    /// resolved.
    pub object_guids: Option<Arc<ObjectGuids>>,
}

/// A component filled from another document.
#[derive(Debug, Clone, PartialEq)]
pub struct Linked {
    pub component: ComponentUid,
    /// The other document's URN.
    pub document: String,
    /// The component's name in this design.
    pub name: String,
    pub occurrences: usize,
}

/// What [`link_references`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkOutcome {
    pub linked: Vec<Linked>,
    /// Components that could not be filled: the document's URN and why.
    pub failed: Vec<(String, String)>,
    /// Captured positions inside the filled components that placed their
    /// occurrences.
    pub positions: usize,
    /// Those left out, by why.
    pub positions_left_out: BTreeMap<String, usize>,
}

/// Fills the components standing for components of other documents
/// (`report.components.inserted`) from `sources` (by the documents' URNs),
/// then places the occurrences inside them where the design's captured
/// positions put them ([`Linker`]). Components of documents not in
/// `sources` stay empty.
pub fn link_references<K: Kernel>(
    doc: &mut Document<K>,
    report: &mut DesignReport,
    sources: &HashMap<String, Referenced<'_, K>>,
) -> LinkOutcome {
    let mut linker = Linker::default();
    split_variants(doc, report, sources);
    let mut documents: Vec<String> = Vec::new();
    for i in &report.components.inserted {
        if !documents.contains(&i.document) {
            documents.push(i.document.clone());
        }
    }
    for document in documents {
        if let Some(source) = sources.get(&document) {
            linker.fill(doc, report, &document, source);
        }
    }
    linker.separate(doc, report);
    linker.finish(doc, report)
}

/// Most components `Linker::separate` makes for one design.
const MAX_SEPARATIONS: usize = 200;

/// [`link_references`] one document at a time, so that the caller can drop
/// each document once it is copied.
#[derive(Debug, Default)]
pub struct Linker {
    /// The occurrences inside each filled component, by their GUIDs.
    inside: HashMap<ComponentUid, HashMap<String, Vec<OccurrenceUid>>>,
    out: LinkOutcome,
    /// Where the captured positions inside put occurrences ([`Linker::plan`]).
    plan: Option<Plan>,
}

impl Linker {
    /// Fills the components standing for components of the document
    /// `document` (its URN) with copies of `source`.
    pub fn fill<K: Kernel>(
        &mut self,
        doc: &mut Document<K>,
        report: &mut DesignReport,
        document: &str,
        source: &Referenced<'_, K>,
    ) {
        let components = &mut report.components;
        for inserted in components.inserted.clone() {
            if inserted.document != document {
                continue;
            }
            match doc.fill_component_copy(source.document, inserted.component, Some(&source.name)) {
                Ok(copies) => {
                    // Its sketches and construction features shown or
                    // hidden as in the document's own import (mitcad#2449).
                    for (original, copy) in &copies.features {
                        let Some(shown) = source.document.feature_shown(*original) else {
                            continue;
                        };
                        report.light_bulbs.copied += 1;
                        if doc.feature_shown(*copy) != Some(shown) {
                            report.light_bulbs.copies_differ += 1;
                        }
                    }
                    let guids: HashMap<String, Vec<OccurrenceUid>> = source
                        .occurrence_guids
                        .iter()
                        .filter_map(|(g, list)| {
                            let copied: Vec<OccurrenceUid> = list
                                .iter()
                                .filter_map(|o| copies.occurrences.get(o).copied())
                                .collect();
                            (!copied.is_empty()).then(|| (g.clone(), copied))
                        })
                        .collect();
                    // Its stored bodies, as the copies' bodies.
                    if let Some(guid) = inserted.document_guid.as_deref() {
                        for (stored, body) in source.stored_bodies {
                            if let Some(feature) = copies.features.get(&body.feature) {
                                components
                                    .external_bodies
                                    .entry(format!("{}|{stored}", guid.to_lowercase()))
                                    .or_default()
                                    .push(BodyUid::new(*feature, body.index));
                            }
                        }
                        // By the body objects a derive names (mitcad#324).
                        for (object, body) in source.entity_bodies {
                            if let Some(feature) = copies.features.get(&body.feature) {
                                components
                                    .external_entities
                                    .entry(format!("{}|{object}", guid.to_lowercase()))
                                    .or_default()
                                    .push(BodyUid::new(*feature, body.index));
                            }
                        }
                    }
                    for (g, copied) in &guids {
                        let list = components.occurrence_guids.entry(g.clone()).or_default();
                        for o in copied {
                            if !list.contains(o) {
                                list.push(*o);
                            }
                        }
                    }
                    self.inside.insert(inserted.component, guids);
                    components.filled.push(inserted.component);
                    components.linked += inserted.occurrences;
                    self.out.linked.push(Linked {
                        component: inserted.component,
                        document: inserted.document.clone(),
                        name: doc.assembly().name(inserted.component),
                        occurrences: inserted.occurrences,
                    });
                }
                Err(e) => self
                    .out
                    .failed
                    .push((inserted.document.clone(), e.to_string())),
            }
        }
    }

    /// Makes the occurrences of a copy of a component inside a filled
    /// component ([`Document::make_unique`]) known by the GUIDs of the
    /// occurrences they copy.
    pub(crate) fn register_copy(&mut self, copy: &ComponentCopy, report: &mut DesignReport) {
        let name = |lists: &mut HashMap<String, Vec<OccurrenceUid>>| {
            for list in lists.values_mut() {
                let more: Vec<OccurrenceUid> = list
                    .iter()
                    .filter_map(|o| copy.occurrences.get(o).copied())
                    .collect();
                for o in more {
                    if !list.contains(&o) {
                        list.push(o);
                    }
                }
            }
        };
        for lists in self.inside.values_mut() {
            name(lists);
        }
        name(&mut report.components.occurrence_guids);
        // Its bodies matched to stored bodies of the document: the copy's.
        for bodies in report.components.external_bodies.values_mut() {
            let more: Vec<BodyUid> = bodies
                .iter()
                .filter_map(|b| Some(BodyUid::new(*copy.features.get(&b.feature)?, b.index)))
                .collect();
            bodies.extend(more);
        }
        report.components.copies += 1;
    }

    /// Gives the occurrences inside the filled components that the design's
    /// captured positions place differently on different paths a copy of
    /// their component each. A component placed several times inside a
    /// copy (a bearing in an assembly) has one set of occurrences, so a
    /// design that arranges what is inside each occurrence of it
    /// differently needs a component of its own for each arrangement
    /// ([`Document::make_unique`]); the positions then name the
    /// occurrences of the copy. Returns how many it made.
    pub fn separate<K: Kernel>(
        &mut self,
        doc: &mut Document<K>,
        report: &mut DesignReport,
    ) -> usize {
        let mut made = 0;
        for _ in 0..MAX_SEPARATIONS {
            let Some(conflict) = plan_inside(doc, report, &self.inside, true).conflict else {
                break;
            };
            // The first level where the two paths part inside the copy:
            // the occurrence of the later one gets a copy of its component.
            let parting = (conflict.outer..conflict.first.len().min(conflict.second.len()))
                .find(|k| conflict.first[*k] != conflict.second[*k]);
            let Some(occurrence) = parting.map(|k| conflict.second[k]) else {
                break;
            };
            match doc.make_unique(occurrence) {
                Ok((_, copy)) => {
                    self.register_copy(&copy, report);
                    made += 1;
                }
                Err(e) => {
                    if crate::tracing() {
                        eprintln!(
                            "import: {} could not get a component of its own: {e}",
                            doc.assembly().occurrence_name(occurrence)
                        );
                    }
                    break;
                }
            }
        }
        made
    }

    /// Works out where the design's captured positions put the occurrences
    /// inside the filled components (once the filled components' placements
    /// are where the file has them): the occurrences, each with its last
    /// placement in its parent and the timeline index of the position that
    /// gave it. The positions that cannot be placed are counted for
    /// [`Linker::finish`].
    pub fn plan<K: Kernel>(
        &mut self,
        doc: &Document<K>,
        report: &DesignReport,
    ) -> Vec<(i64, OccurrenceUid, Transform)> {
        let plan = plan_inside(doc, report, &self.inside, false);
        let placed = plan
            .items
            .iter()
            .flat_map(|(index, item)| {
                item.placed
                    .values()
                    .flatten()
                    .map(move |(o, t)| (*index, *o, *t))
            })
            .collect();
        self.plan = Some(plan);
        placed
    }

    /// Every placement the captured positions gave the occurrences inside
    /// the filled components ([`Linker::plan`] gives the last of each), in
    /// timeline order: the occurrence, its placement in its parent from
    /// the position on.
    pub fn trail(&self) -> Vec<(i64, OccurrenceUid, Transform)> {
        self.plan
            .as_ref()
            .map(|p| p.trail.clone())
            .unwrap_or_default()
    }

    /// Places the occurrences inside the filled components where the
    /// design's captured positions put them: a `capture_position` feature
    /// in each component per captured position, at the end of the
    /// timeline; the captured position's report entry and timeline note no
    /// longer say they were left out.
    pub fn finish<K: Kernel>(
        mut self,
        doc: &mut Document<K>,
        report: &mut DesignReport,
    ) -> LinkOutcome {
        if self.plan.is_none() {
            self.plan(doc, report);
        }
        if let Some(plan) = self.plan.take() {
            add_inside(doc, report, plan, &mut self.out);
        }
        report.components.placed_inside += self.out.positions;
        self.out
    }
}

/// What became of the positions inside components of other documents of
/// one captured position.
#[derive(Debug, Default)]
struct ItemPositions {
    /// Their occurrences, by the component they are placed in, with their
    /// placements there.
    placed: BTreeMap<ComponentUid, Vec<(OccurrenceUid, Transform)>>,
    /// Why others are left out.
    left_out: Vec<String>,
}

/// Two paths to one occurrence of a copy that the same captured position
/// places differently: the occurrences they lead through differ where a
/// component is placed more than once inside the copy.
#[derive(Debug, Clone, PartialEq)]
struct Conflict {
    first: Vec<OccurrenceUid>,
    second: Vec<OccurrenceUid>,
    /// How many of their first occurrences are the design's own (outside
    /// the copy).
    outer: usize,
}

/// Where the captured positions inside filled components put the
/// occurrences ([`Linker::plan`]), by captured position.
#[derive(Debug, Default)]
struct Plan {
    items: BTreeMap<i64, ItemPositions>,
    /// The first conflict, when the plan stopped at it.
    conflict: Option<Conflict>,
    /// Every placement the positions gave an occurrence, in timeline order
    /// (the occurrence's placement then, for the items between).
    trail: Vec<(i64, OccurrenceUid, Transform)>,
}

/// The captured positions inside filled components (see the module
/// documentation): where they put the occurrences. With `stop`, at the
/// first conflict (two paths to one occurrence place it differently at the
/// same point of the timeline), which [`Plan::conflict`] gives; else the
/// occurrences with a conflict are left where they are.
fn plan_inside<K: Kernel>(
    doc: &Document<K>,
    report: &DesignReport,
    inside: &HashMap<ComponentUid, HashMap<String, Vec<OccurrenceUid>>>,
    stop: bool,
) -> Plan {
    if report.components.positions_inside.is_empty() {
        return Plan::default();
    }
    let mut items: BTreeMap<i64, ItemPositions> = BTreeMap::new();
    // Each occurrence's placement in its parent, by the positions in
    // timeline order: the last one, unless two paths disagree; the path
    // that gave it.
    type Placed = (i64, Option<Transform>, Vec<OccurrenceUid>);
    let mut placed: BTreeMap<OccurrenceUid, Placed> = BTreeMap::new();
    // The same placements for the levels above, as the positions put them
    // so far (a position names the levels it moves and the levels it does
    // not, whose placements it leaves).
    let mut planned: HashMap<OccurrenceUid, Transform> = HashMap::new();
    let mut trail: Vec<(i64, OccurrenceUid, Transform)> = Vec::new();
    let mut positions: Vec<_> = report.components.positions_inside.iter().collect();
    positions.sort_by_key(|p| (p.index, p.inner.len()));
    for p in positions {
        let a = doc.assembly();
        let item = items.entry(p.index).or_default();
        let mut skip = |why: &str| item.left_out.push(why.to_owned());
        let Some(start) = p
            .outer
            .last()
            .and_then(|o| a.occurrence(*o))
            .map(|o| o.component)
        else {
            skip("a position of an occurrence not in the occurrence tree");
            continue;
        };
        let Some(guids) = inside.get(&start) else {
            skip("a position inside a component of a document not found");
            continue;
        };
        let mut path = p.outer.clone();
        let mut component = start;
        for g in &p.inner {
            match inside_occurrence(a, guids, g, component) {
                Some(o) => {
                    path.push(o.uid);
                    component = o.component;
                }
                None => {
                    if crate::tracing() {
                        let known = guids.get(&g.to_lowercase()).map(Vec::len);
                        eprintln!(
                            "import: a position inside {}: {g} is not an occurrence placed in {} ({} known with that GUID); at level {} of {:?}",
                            a.name(start),
                            a.name(component),
                            known.unwrap_or(0),
                            path.len() - p.outer.len(),
                            p.inner
                        );
                    }
                    path.clear();
                    break;
                }
            }
        }
        if path.len() <= p.outer.len() {
            skip("a position of an occurrence the other document does not have");
            continue;
        }
        let (&last, parents) = path.split_last().expect("a path");
        if a.occurrence(last).is_some_and(|o| o.grounded) {
            skip("a position of a grounded occurrence");
            continue;
        }
        // Where the occurrences above it are: as the positions before put
        // them, else at the end of the timeline.
        let parent = parents.iter().fold(Transform::IDENTITY, |t, o| {
            t.after(
                &planned
                    .get(o)
                    .copied()
                    .or_else(|| doc.placement(*o))
                    .unwrap_or(Transform::IDENTITY),
            )
        });
        let local = inverse(&parent).after(&p.placement);
        // A shared component: another path that puts the occurrence
        // elsewhere at the same point of the timeline leaves it where it is.
        match placed.get_mut(&last) {
            Some((index, at, first)) if *index == p.index => {
                if at.is_some_and(|t| !same(&t, &local)) {
                    if crate::tracing() {
                        eprintln!(
                            "import: positions of {} put it in different places: {:?} and {:?}; path {:?} and {:?}",
                            a.occurrence_name(last),
                            at.map(|t| t.translation),
                            local.translation,
                            first
                                .iter()
                                .map(|o| a.occurrence_name(*o))
                                .collect::<Vec<_>>(),
                            path.iter()
                                .map(|o| a.occurrence_name(*o))
                                .collect::<Vec<_>>(),
                        );
                    }
                    if stop {
                        return Plan {
                            items,
                            trail,
                            conflict: Some(Conflict {
                                first: first.clone(),
                                second: path,
                                outer: p.outer.len(),
                            }),
                        };
                    }
                    *at = None;
                }
            }
            Some(entry) => {
                *entry = (p.index, Some(local), path.clone());
                planned.insert(last, local);
                trail.push((p.index, last, local));
            }
            None => {
                placed.insert(last, (p.index, Some(local), path.clone()));
                planned.insert(last, local);
                trail.push((p.index, last, local));
            }
        }
    }
    for (occurrence, (index, at, _)) in placed {
        let item = items.entry(index).or_default();
        let Some(at) = at else {
            item.left_out.push(
                "occurrences of the other document's component put it in different places"
                    .to_owned(),
            );
            continue;
        };
        let Some(parent) = doc.assembly().occurrence(occurrence).map(|o| o.parent) else {
            continue;
        };
        item.placed
            .entry(parent)
            .or_default()
            .push((occurrence, at));
    }
    Plan {
        items,
        conflict: None,
        trail,
    }
}

/// The `capture_position` features of the positions [`plan_inside`] placed,
/// and the captured positions' report entries.
fn add_inside<K: Kernel>(
    doc: &mut Document<K>,
    report: &mut DesignReport,
    plan: Plan,
    out: &mut LinkOutcome,
) {
    for (index, positions) in plan.items {
        let at = report.items.iter().position(|i| i.index == index);
        let name = at.map(|i| report.items[i].name.clone());
        let mut left_out = positions.left_out;
        let mut added = Vec::new();
        for (component, list) in positions.placed {
            let def = json!({"type": "capture_position", "positions": list
                .iter()
                .map(|(o, t)| json!({"occurrence": o.to_string(), "transform": matrix_rows(t)}))
                .collect::<Vec<_>>()});
            let result = serde_json::from_value::<FeatureDef<ValueInput>>(def)
                .map_err(|e| e.to_string())
                .and_then(|def| {
                    doc.add_feature_to(&def, name.as_deref(), Some(component))
                        .or_else(|_| doc.add_feature_to(&def, None, Some(component)))
                        .map_err(|e| e.to_string())
                });
            match result {
                Ok(feature) => {
                    out.positions += list.len();
                    added.push(feature.uid);
                }
                Err(e) => left_out.extend(list.iter().map(|_| e.clone())),
            }
        }
        for why in &left_out {
            *out.positions_left_out.entry(why.clone()).or_default() += 1;
        }
        if let Some(at) = at {
            renote(doc, &mut report.items[at], &added, &left_out);
        }
    }
}

/// A captured position's report entry once the positions inside
/// components of other documents came in (`added`, its new features) or
/// not (`left_out`): its note and outcome, and its features' timeline
/// notes.
fn renote<K: Kernel>(
    doc: &mut Document<K>,
    item: &mut ItemReport,
    added: &[FeatureUid],
    left_out: &[String],
) {
    // The note's reasons, without those of the positions inside.
    let note = item.note.clone().unwrap_or_default();
    let mut reasons: Vec<String> = Vec::new();
    for part in note.split("; ") {
        let part = part.strip_prefix("left out: ").unwrap_or(part);
        if !part.is_empty() && !part.ends_with(INSIDE) {
            reasons.push(part.to_owned());
        }
    }
    reasons.extend(left_out.iter().cloned());
    item.features
        .extend(added.iter().map(|uid| uid.to_string()));
    if item.features.is_empty() {
        item.note = (!reasons.is_empty()).then(|| reasons.join("; "));
        return;
    }
    item.outcome = if reasons.is_empty() {
        Outcome::Parametric
    } else {
        Outcome::Partial
    };
    item.note = (!reasons.is_empty()).then(|| format!("left out: {}", reasons.join("; ")));
    let timeline = match item.outcome {
        Outcome::Partial => partly(item.note.as_deref()),
        _ => String::new(),
    };
    let notes: Vec<(FeatureUid, String)> = item
        .features
        .iter()
        .filter_map(|f| f.parse().ok())
        .map(|uid| (uid, timeline.clone()))
        .collect();
    let _ = doc.set_import_notes(&notes);
}

/// The occurrence of a copy that a GUID of the other document names in
/// `parent` (a GUID names one per copy of the occurrence's component).
fn inside_occurrence<'a>(
    a: &'a Assembly,
    guids: &HashMap<String, Vec<OccurrenceUid>>,
    guid: &str,
    parent: ComponentUid,
) -> Option<&'a Occurrence> {
    guids
        .get(&guid.to_lowercase())?
        .iter()
        .filter_map(|o| a.occurrence(*o))
        .find(|o| o.parent == parent)
}

/// Where the captured positions put the occurrences inside one occurrence
/// of a component of another document: by the GUIDs of the levels, each
/// level's placement in that occurrence's coordinates (the last captured
/// position's).
type Arrangement = BTreeMap<Vec<String>, Transform>;

/// The arrangement the captured positions give the inside of `occurrence`.
fn arrangement<K: Kernel>(
    doc: &Document<K>,
    report: &DesignReport,
    occurrence: OccurrenceUid,
) -> Arrangement {
    let mut positions: Vec<_> = report
        .components
        .positions_inside
        .iter()
        .filter(|p| p.outer.last() == Some(&occurrence))
        .collect();
    positions.sort_by_key(|p| p.index);
    let mut out = Arrangement::new();
    for p in positions {
        // The occurrence's coordinates in the position's component.
        let frame = p.outer.iter().fold(Transform::IDENTITY, |t, o| {
            t.after(&doc.placement(*o).unwrap_or(Transform::IDENTITY))
        });
        let key: Vec<String> = p.inner.iter().map(|g| g.to_lowercase()).collect();
        out.insert(key, inverse(&frame).after(&p.placement));
    }
    out
}

/// Where a level of the other document is in the coordinates of the
/// occurrence of its root component, as that document places it itself;
/// None for a path it does not have.
fn default_placement<K: Kernel>(source: &Referenced<'_, K>, path: &[String]) -> Option<Transform> {
    let a = source.document.assembly();
    let mut at = ComponentUid::ROOT;
    let mut t = Transform::IDENTITY;
    for guid in path {
        let o = inside_occurrence(a, source.occurrence_guids, guid, at)?;
        t = t.after(&source.document.placement(o.uid)?);
        at = o.component;
    }
    Some(t)
}

/// Whether two occurrences of a component of another document have the
/// same arrangement inside: each level either is placed the same by their
/// positions or, where only one is placed, is where the other document
/// leaves it.
fn same_arrangement<K: Kernel>(
    source: &Referenced<'_, K>,
    a: &Arrangement,
    b: &Arrangement,
) -> bool {
    a.keys().chain(b.keys()).all(|path| {
        let Some(default) = default_placement(source, path) else {
            // A level the other document does not have (left out anyway).
            return true;
        };
        let of = |x: &Arrangement| x.get(path).copied().unwrap_or(default);
        same(&of(a), &of(b))
    })
}

/// Gives the occurrences of a component of another document that the
/// design's captured positions arrange differently inside each a component
/// of their own, so that each can be filled with its own copy
/// ([`Linker::fill`]): the occurrences with the same arrangement share one
/// (as the file's occurrences of one document do), and those that are
/// arranged alike by every position or by none keep the first component.
/// Components of documents not in `sources` stay as they are. Returns each
/// component made with the component it was made from.
pub(crate) fn split_variants<K: Kernel>(
    doc: &mut Document<K>,
    report: &mut DesignReport,
    sources: &HashMap<String, Referenced<'_, K>>,
) -> Vec<(ComponentUid, ComponentUid)> {
    let mut made = Vec::new();
    if report.components.positions_inside.is_empty() {
        return made;
    }
    for inserted in report.components.inserted.clone() {
        let Some(source) = sources.get(&inserted.document) else {
            continue;
        };
        let occurrences: Vec<OccurrenceUid> = doc
            .assembly()
            .occurrences_of(inserted.component)
            .map(|o| o.uid)
            .collect();
        if occurrences.len() < 2 {
            continue;
        }
        let arrangements: Vec<Arrangement> = occurrences
            .iter()
            .map(|o| arrangement(doc, report, *o))
            .collect();
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for i in 0..occurrences.len() {
            match groups
                .iter_mut()
                .find(|g| same_arrangement(source, &arrangements[g[0]], &arrangements[i]))
            {
                Some(group) => group.push(i),
                None => groups.push(vec![i]),
            }
        }
        for group in groups.iter().skip(1) {
            let name = doc.assembly().name(inserted.component);
            let Ok(component) =
                doc.give_occurrence_own_component(occurrences[group[0]], Some(&name))
            else {
                continue;
            };
            let mut moved = 1;
            for &i in &group[1..] {
                if doc.place_other_component(occurrences[i], component).is_ok() {
                    moved += 1;
                }
            }
            report.components.inserted.push(InsertedComponent {
                component,
                occurrences: moved,
                ..inserted.clone()
            });
            if let Some(first) = report
                .components
                .inserted
                .iter_mut()
                .find(|i| i.component == inserted.component)
            {
                first.occurrences -= moved;
            }
            report.components.copies += 1;
            made.push((inserted.component, component));
        }
    }
    made
}

/// Whether two placements are the same within rounding.
fn same(a: &Transform, b: &Transform) -> bool {
    (0..3).all(|r| {
        (0..3).all(|c| (a.linear[r][c] - b.linear[r][c]).abs() <= 1e-9)
            && (a.translation[r] - b.translation[r]).abs() <= 1e-6
    })
}

/// One occurrence's placements: its path's placement in the root component
/// after each item (by timeline index), in timeline order.
pub(crate) type Steps = Vec<(Option<i64>, Transform)>;

/// Where the file places the occurrences of components of other documents
/// that it stores placements for only along paths inside them (mitcad#1892:
/// a joint on a part inside an inserted design moves the whole occurrence,
/// and the file keeps the placement of the path to each occurrence inside,
/// `_f3d.inside_placements`), by the occurrence's object id. Each path's
/// placements with its inner levels' placement in the other document taken
/// off are the occurrence's own; the paths whose inner levels the other
/// document's import has (`sources`) are used, and where they part (by
/// the last placement) the most of them agree on is taken.
pub(crate) fn outer_placements<K: Kernel>(
    dump: &Dump,
    sources: &HashMap<String, Referenced<'_, K>>,
) -> HashMap<u64, Steps> {
    let mut out = HashMap::new();
    let mut nodes: Vec<&OccurrenceNode> = dump.occurrences.iter().flatten().collect();
    while let Some(node) = nodes.pop() {
        nodes.extend(node.children.iter().flatten());
        let Some(f3d) = node.f3d.as_ref() else {
            continue;
        };
        let (Some(id), Some(inside)) = (f3d.object_id, f3d.inside_placements.as_ref()) else {
            continue;
        };
        let Some(source) = f3d.external_document.as_ref().and_then(|d| sources.get(d)) else {
            continue;
        };
        // The paths' own placements, grouped by where they leave it.
        let mut groups: Vec<Vec<Steps>> = Vec::new();
        for path in inside {
            let Some(inner) = inner_placement(source, path.inner.as_deref().unwrap_or_default())
            else {
                continue;
            };
            let undo = inverse(&inner);
            let steps: Option<Steps> = path
                .placements
                .iter()
                .flatten()
                .map(|s| {
                    let t = crate::components::transform(s.transform.as_ref()?)?;
                    Some((s.index, t.after(&undo)))
                })
                .collect();
            let Some(steps) = steps.filter(|s| !s.is_empty()) else {
                continue;
            };
            let last = |s: &Steps| s.last().map(|(_, t)| *t);
            match groups
                .iter_mut()
                .find(|g| matches!((last(&g[0]), last(&steps)), (Some(a), Some(b)) if same(&a, &b)))
            {
                Some(g) => g.push(steps),
                None => groups.push(vec![steps]),
            }
        }
        if groups.len() > 1 && crate::tracing() {
            eprintln!(
                "import: occurrence object {id}: the paths inside it place it {} ways",
                groups.len()
            );
        }
        // The largest group (the first of equal ones).
        let best = groups.into_iter().rev().max_by_key(Vec::len);
        if let Some(steps) = best.and_then(|g| g.into_iter().next()) {
            out.insert(id, steps);
        }
    }
    out
}

/// The timeline index the positions [`inside_positions`] makes take: after
/// every item of the timeline, as the placements they come from are where
/// the file leaves the occurrences at its end (and no item's report entry
/// is theirs).
pub const INSIDE_PLACEMENTS_INDEX: i64 = i64::MAX / 2;

/// The placements the design stores for paths that go on inside
/// occurrences of components of other documents (`_f3d.inside_placements`,
/// mitcad#1892) as positions inside them, which place the copied
/// occurrences inside as the captured positions inside do (mitcad#2492).
/// A design places the levels inside a copy otherwise than the other
/// document does itself (an assembly whose parts the other document leaves
/// where it inserted them and the design moved with its joints; an
/// assembly that turns a lift arm of a shell it places), and the file
/// keeps where only in those paths: before, they gave only the outer
/// occurrence's own placement ([`outer_placements`]), so the levels inside
/// stayed where the other document has them.
///
/// Each path's last placement (in the root component) is the position;
/// its levels are named by the GUIDs the other document's import gives
/// the occurrences its object ids name (each level placed in the one
/// before). A path whose last level is where the other document has it
/// already, given the outer occurrence's own last placement, makes none.
/// `outer` are the placements [`outer_placements`] worked out.
pub fn inside_positions<K: Kernel>(
    dump: &Dump,
    components: &crate::components::Components,
    sources: &HashMap<String, Referenced<'_, K>>,
    outer: &HashMap<u64, Steps>,
) -> Vec<crate::captures::InsidePosition> {
    let mut out = Vec::new();
    // (node, the design's occurrences down to it).
    let mut nodes: Vec<(&OccurrenceNode, Vec<OccurrenceUid>)> = dump
        .occurrences
        .iter()
        .flatten()
        .map(|n| (n, Vec::new()))
        .collect();
    while let Some((node, above)) = nodes.pop() {
        let Some(f3d) = node.f3d.as_ref() else {
            continue;
        };
        let Some(uid) = f3d.object_id.and_then(|id| components.occurrence(id)) else {
            continue;
        };
        let mut path = above;
        path.push(uid);
        nodes.extend(node.children.iter().flatten().map(|c| (c, path.clone())));
        let (Some(id), Some(inside)) = (f3d.object_id, f3d.inside_placements.as_ref()) else {
            continue;
        };
        let Some(source) = f3d.external_document.as_ref().and_then(|d| sources.get(d)) else {
            continue;
        };
        // The outer occurrence's own last placement in the root, where the
        // file gives one: worked out from the paths, else its own steps,
        // else (at the top) its stored transform.
        let own = outer
            .get(&id)
            .and_then(|s| s.last().map(|(_, t)| *t))
            .or_else(|| {
                f3d.placements
                    .iter()
                    .flatten()
                    .next_back()
                    .and_then(|s| crate::components::transform(s.transform.as_ref()?))
            })
            .or_else(|| {
                (path.len() == 1)
                    .then(|| {
                        node.transform
                            .as_ref()
                            .and_then(crate::components::transform)
                    })
                    .flatten()
            });
        // (Whether the other document has it there already, the position.)
        let mut paths: Vec<(bool, crate::captures::InsidePosition)> = Vec::new();
        for p in inside {
            let inner = p.inner.as_deref().unwrap_or_default();
            let Some(guids) = inner_guids(source, inner) else {
                continue;
            };
            let Some(last) = p
                .placements
                .iter()
                .flatten()
                .next_back()
                .and_then(|s| crate::components::transform(s.transform.as_ref()?))
            else {
                continue;
            };
            // (A path on into the documents the other one copies has no
            // object ids of its own beyond the first of them: its GUIDs
            // name the levels, mitcad#2528.)
            let default = inner_placement(source, inner).or_else(|| guid_placement(source, &guids));
            let there = matches!((own, default), (Some(own), Some(d)) if same_within(&own.after(&d), &last));
            paths.push((
                there,
                crate::captures::InsidePosition {
                    index: INSIDE_PLACEMENTS_INDEX,
                    outer: path.clone(),
                    inner: guids,
                    placement: last,
                },
            ));
        }
        // A path whose last level is where the other document has it makes
        // none, unless a level above it moves: then the levels below would
        // move with it, away from where the file has them (mitcad#2528: the
        // board inside the electronics stack, placed again by the top
        // design, took its printed circuit component, which the stack's own
        // paths had left where the file has it, 10.8 mm away).
        let moved: Vec<Vec<String>> = paths
            .iter()
            .filter(|(there, _)| !there)
            .map(|(_, p)| p.inner.clone())
            .collect();
        out.extend(paths.into_iter().filter_map(|(there, p)| {
            let below_moved = moved
                .iter()
                .any(|m| m.len() < p.inner.len() && p.inner.starts_with(m));
            (!there || below_moved).then_some(p)
        }));
    }
    out
}

/// The placement in the other document's root component of the path of
/// its occurrences named by `guids` (from the outermost down, each placed
/// in the one before), as its import has them, also inside the copies of
/// the documents it copies; None where a level is not found
/// (mitcad#2528).
fn guid_placement<K: Kernel>(source: &Referenced<'_, K>, guids: &[String]) -> Option<Transform> {
    let assembly = source.document.assembly();
    let mut placement = Transform::IDENTITY;
    let mut parent = ComponentUid::ROOT;
    for g in guids {
        let o = inside_occurrence(assembly, source.occurrence_guids, g, parent)?;
        placement = placement.after(&o.transform);
        parent = o.component;
    }
    Some(placement)
}

/// The GUIDs the other document's import gives the occurrences of the
/// path `inner` (object ids in its file, each placed in the one before):
/// for each, the GUID that names the fewest occurrences (of equal ones the
/// first in order); None where a level is not one of its occurrences, not
/// placed in the level before, or has no GUID.
///
/// A path goes on past a level whose component the other document filled
/// from a third document (the board inside the electronics stack, the
/// bearings inside the lift system inside the shell): the levels after it
/// are object ids in that document's file, named by the GUIDs its import
/// gives them ([`ObjectGuids`], `source.object_guids`), and so on down
/// (mitcad#2528). Before, such a path was not resolved, so the levels
/// more than one document deep stayed where the documents in between had
/// them.
fn inner_guids<K: Kernel>(source: &Referenced<'_, K>, inner: &[u64]) -> Option<Vec<String>> {
    if inner.is_empty() {
        return None;
    }
    let assembly = source.document.assembly();
    let mut parent = ComponentUid::ROOT;
    let mut out = Vec::new();
    let mut levels = inner.iter();
    // The levels in the other document itself, up to one filled from a
    // third document.
    let mut deeper: Option<Arc<ObjectGuids>> = None;
    for id in levels.by_ref() {
        let uid = *source.occurrence_objects.get(id)?;
        let o = assembly.occurrence(uid)?;
        if o.parent != parent {
            return None;
        }
        out.push(fewest_guid(source.occurrence_guids, uid)?);
        parent = o.component;
        deeper = source
            .object_guids
            .as_ref()
            .and_then(|t| t.occurrences.get(id))
            .and_then(|level| level.inside.clone());
        if deeper.is_some() {
            break;
        }
    }
    // The levels in the documents the copies come from.
    for id in levels {
        let (guid, inside) = {
            let level = deeper.as_ref()?.occurrences.get(id)?;
            (level.guid.clone(), level.inside.clone())
        };
        out.push(guid);
        if inside.is_some() {
            deeper = inside;
        }
    }
    Some(out)
}

/// The GUID of `guids` that names the occurrence `uid` and the fewest
/// others (of equal ones the first in order).
fn fewest_guid(guids: &HashMap<String, Vec<OccurrenceUid>>, uid: OccurrenceUid) -> Option<String> {
    guids
        .iter()
        .filter(|(_, list)| list.contains(&uid))
        .min_by(|a, b| a.1.len().cmp(&b.1.len()).then_with(|| a.0.cmp(b.0)))
        .map(|(guid, _)| guid.clone())
}

/// A document's occurrences by their object ids in its file, as a design
/// that inserts it names the levels of the paths it stores through it
/// (mitcad#2528): for each, the GUID its import names it by (the one that
/// names the fewest occurrences, as [`inner_guids`] takes it) and, where
/// its component was filled from another document, that document's in
/// turn. A path through a copy names the levels inside the documents the
/// copy copies by their own files' object ids.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectGuids {
    pub occurrences: HashMap<u64, ObjectGuid>,
}

/// One occurrence of [`ObjectGuids`].
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectGuid {
    pub guid: String,
    /// The document its component was filled from, where it was.
    pub inside: Option<Arc<ObjectGuids>>,
}

/// The [`ObjectGuids`] of the design `doc` once its components of other
/// documents are filled (`components`, its import's report; `sources`, the
/// documents it was filled from): each occurrence its file names by an
/// object id, with the GUID and the filled document's own.
pub fn object_guids<K: Kernel>(
    doc: &Document<K>,
    components: &crate::components::ComponentReport,
    sources: Option<&HashMap<String, Referenced<'_, K>>>,
) -> ObjectGuids {
    let assembly = doc.assembly();
    // The filled components by the documents they were filled from.
    let documents: HashMap<ComponentUid, &str> = components
        .inserted
        .iter()
        .filter(|i| components.filled.contains(&i.component))
        .map(|i| (i.component, i.document.as_str()))
        .collect();
    // Each occurrence's GUID: the one naming the fewest occurrences.
    let mut named: HashMap<OccurrenceUid, (usize, &String)> = HashMap::new();
    for (guid, list) in &components.occurrence_guids {
        for o in list {
            let better = named
                .get(o)
                .is_none_or(|(n, g)| (list.len(), guid) < (*n, *g));
            if better {
                named.insert(*o, (list.len(), guid));
            }
        }
    }
    let mut out = ObjectGuids::default();
    for (&id, &uid) in &components.occurrence_objects {
        let Some((_, guid)) = named.get(&uid) else {
            continue;
        };
        let inside = assembly
            .occurrence(uid)
            .and_then(|o| documents.get(&o.component))
            .and_then(|d| sources?.get(*d)?.object_guids.clone());
        out.occurrences.insert(
            id,
            ObjectGuid {
                guid: (*guid).clone(),
                inside,
            },
        );
    }
    out
}

/// Whether two placements (mm) are the same within the files' rounding
/// (as the decoder compares placements: 1e-5 in the rotation, 5 µm).
fn same_within(a: &Transform, b: &Transform) -> bool {
    (0..3).all(|r| {
        (0..3).all(|c| (a.linear[r][c] - b.linear[r][c]).abs() <= 1e-5)
            && (a.translation[r] - b.translation[r]).abs() <= 5e-3
    })
}

/// The placement in the other document's root component of the path of
/// its occurrences `inner` (object ids in its file, from the outermost
/// down), each placed in the one before; None where a level is not one of
/// its occurrences or not placed there.
fn inner_placement<K: Kernel>(source: &Referenced<'_, K>, inner: &[u64]) -> Option<Transform> {
    if inner.is_empty() {
        return None;
    }
    let assembly = source.document.assembly();
    let mut placement = Transform::IDENTITY;
    let mut parent = ComponentUid::ROOT;
    for id in inner {
        let o = assembly.occurrence(*source.occurrence_objects.get(id)?)?;
        if o.parent != parent {
            return None;
        }
        placement = placement.after(&o.transform);
        parent = o.component;
    }
    Some(placement)
}
