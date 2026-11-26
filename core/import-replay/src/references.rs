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
