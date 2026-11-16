// SPDX-License-Identifier: MIT
//! The file's captured positions (`Snapshot` items, mitcad#75; what the
//! decoder gives: SCHEMA.md §5.3 `Snapshot`) as Mitcad's
//! `capture_position` features, and where they leave the occurrences.
//!
//! A captured position lists occurrence paths from its component with
//! their placements in that component. The transforms the file stores for
//! the occurrences are older than the captured positions: the joints and
//! as-built joints of the file hold at the captured placements, not at the
//! stored transforms, also those before the captured position *(the
//! designs read)*. So the file places an occurrence at the end of its
//! timeline at its last captured placement, else at its stored transform
//! ([`Captures::final_placement`]); the occurrences take those as their
//! own placements ([`Captures::last`]), and each captured position comes
//! in at its point of the timeline.
//!
//! The items before an occurrence's first captured position were made
//! with it at its stored transform, and those between two captured
//! positions with it at the earlier one: where the import moves geometry
//! from one component's coordinates into another's (a sketch of one
//! component used by an item of another, faces of other components as
//! planes), it takes the placements at the item's point of the timeline
//! ([`Captures::placement_at`], mitcad#86), not the ones the occurrences
//! start at.
//!
//! A path level whose occurrence the decoder does not find (the file names
//! some top-level occurrences in captured positions by other ids than
//! their own) is the occurrence that places the component the next level
//! sits in, where one occurrence does; the same id elsewhere is then that
//! occurrence too. Levels inside components of other documents are left
//! out, and so are grounded occurrences (Mitcad does not move them).

use std::collections::{BTreeMap, HashMap};

use mitcad_dump::{Dump, Mat4, OccurrenceNode};
use mitcad_model::assembly::{Assembly, inverse};
use mitcad_model::{ComponentUid, OccurrenceUid, Transform};
use serde_json::Value;

use crate::components::{self, Components};
use crate::references::Steps;

/// The occurrences one captured position places: in their parents'
/// coordinates, and why others it lists are left out.
#[derive(Debug, Clone, Default)]
pub struct Captured {
    pub positions: Vec<(OccurrenceUid, Transform)>,
    pub left_out: Vec<String>,
}

/// A captured position of an occurrence inside a component of another
/// document ([`Captures::inside`]).
#[derive(Debug, Clone, PartialEq)]
pub struct InsidePosition {
    /// The captured position's timeline index.
    pub index: i64,
    /// The path's occurrences in this design, from the captured position's
    /// component down to the occurrence of the other document's component.
    pub outer: Vec<OccurrenceUid>,
    /// The GUIDs of the levels inside that component, from the outermost
    /// down (the other document's occurrences carry them).
    pub inner: Vec<String>,
    /// The last level's placement in the captured position's component
    /// (mm).
    pub placement: Transform,
}

/// The file's captured positions in Mitcad's terms.
#[derive(Debug, Default, Clone)]
pub struct Captures {
    /// Each occurrence's last placement in its parent: where its
    /// placements put it last, else its last captured placement.
    last: HashMap<OccurrenceUid, Transform>,
    /// Each captured occurrence's stored transform and its captured
    /// placements by timeline index, in timeline order.
    placements: HashMap<OccurrenceUid, (Transform, Vec<(i64, Transform)>)>,
    /// Each captured position by its timeline index.
    pub items: HashMap<i64, Captured>,
    /// Where the timeline's items put each occurrence (mitcad#81): its
    /// path's placement in the root component after each item (by its
    /// timeline index), in timeline order.
    steps: HashMap<OccurrenceUid, Vec<(Option<i64>, Transform)>>,
    /// The same in the occurrence's parent, for the items with an index.
    local_steps: HashMap<OccurrenceUid, Vec<(i64, Transform)>>,
    /// Each step in the occurrence's parent, made relative to the parent
    /// along the path the step is of, in timeline order: an occurrence
    /// inside a component placed several times has one placement in its
    /// parent but a placement in the root per path, which tells no parent
    /// placement apart ([`Captures::steps`] keeps the first path's).
    path_steps: HashMap<OccurrenceUid, Vec<(Option<i64>, Transform)>>,
    /// The positions of occurrences inside components of other documents
    /// (left out of the replay: those components are empty until the
    /// import fills them from the documents' files, mitcad#196).
    pub(crate) inside: Vec<InsidePosition>,
    /// The parts of items in other components (mitcad#169), by index:
    /// the index of their item, whose point of the timeline they are at
    /// (their own indices come after the timeline's items).
    parts: HashMap<i64, i64>,
}

impl Captures {
    /// The point of the timeline of the item at `index`: its own index,
    /// or for a part of an item in another component that item's.
    pub(crate) fn timeline_at(&self, index: i64) -> i64 {
        self.parts.get(&index).copied().unwrap_or(index)
    }

    /// The indices of the parts of the item at `index` in other
    /// components, in order.
    pub(crate) fn parts_of(&self, index: i64) -> Vec<i64> {
        let mut parts: Vec<i64> = self
            .parts
            .iter()
            .filter(|&(_, &of)| of == index)
            .map(|(&part, _)| part)
            .collect();
        parts.sort_unstable();
        parts
    }

    /// Where the file places an occurrence at the end of its timeline:
    /// its last captured placement, else its stored transform (`stored`).
    pub fn final_placement(&self, occurrence: OccurrenceUid, stored: Transform) -> Transform {
        self.last.get(&occurrence).copied().unwrap_or(stored)
    }

    /// Where the file places an occurrence in its parent right after the
    /// item at timeline index `index`, where its placements say (None
    /// before its first or when it has none).
    pub fn placement_after(
        &self,
        assembly: &Assembly,
        occurrence: OccurrenceUid,
        index: i64,
    ) -> Option<Transform> {
        let index = self.timeline_at(index);
        let world = self.world_after(occurrence, index)?;
        let parent_component = assembly.occurrence(occurrence).map(|o| o.parent);
        let parent = parent_component.and_then(|c| self.parent_world_after(assembly, c, index));
        Some(match parent {
            Some(p) => inverse(&p).after(&world),
            None if parent_component.is_none_or(|c| c.is_root()) => world,
            // A parent placed several times: along the steps' own paths.
            None => self.path_step_after(occurrence, index)?,
        })
    }

    /// The occurrence's placement in its parent after the item at `index`
    /// along the paths its steps are of ([`Captures::path_steps`]).
    fn path_step_after(&self, occurrence: OccurrenceUid, index: i64) -> Option<Transform> {
        self.path_steps
            .get(&occurrence)?
            .iter()
            .filter(|(i, _)| i.is_some_and(|i| i <= index))
            .max_by_key(|(i, _)| *i)
            .map(|(_, t)| *t)
    }

    /// The path's placement in the root component after the item at
    /// `index`, by the occurrence's own placements.
    fn world_after(&self, occurrence: OccurrenceUid, index: i64) -> Option<Transform> {
        self.steps
            .get(&occurrence)?
            .iter()
            .filter(|(i, _)| i.is_some_and(|i| i <= index))
            .max_by_key(|(i, _)| *i)
            .map(|(_, t)| *t)
    }

    /// The placement in the root component of the (one) occurrence
    /// placing `component` after the item at `index`; None for the root.
    fn parent_world_after(
        &self,
        assembly: &Assembly,
        component: ComponentUid,
        index: i64,
    ) -> Option<Transform> {
        if component.is_root() {
            return None;
        }
        let o = only_placement_anywhere(assembly, component)?;
        let up = assembly
            .occurrence(o)
            .and_then(|x| self.parent_world_after(assembly, x.parent, index));
        self.world_after(o, index).or_else(|| {
            let local = self.final_placement(o, assembly.occurrence(o)?.transform);
            Some(up.map_or(local, |u| u.after(&local)))
        })
    }

    /// An occurrence inside a copied component of another document that
    /// the design's captured position at `index` placed (`references.rs`):
    /// its placement in its parent from then on, `stored` before.
    pub(crate) fn add_inside(
        &mut self,
        occurrence: OccurrenceUid,
        stored: Transform,
        index: i64,
        local: Transform,
    ) {
        self.last.insert(occurrence, local);
        let (_, by_index) = self
            .placements
            .entry(occurrence)
            .or_insert_with(|| (stored, Vec::new()));
        by_index.retain(|(at, _)| *at != index);
        by_index.push((index, local));
        by_index.sort_by_key(|(at, _)| *at);
    }

    /// The occurrences a captured position places, each at its last
    /// captured placement in its parent.
    pub(crate) fn last(&self) -> BTreeMap<OccurrenceUid, Transform> {
        self.last.iter().map(|(o, t)| (*o, *t)).collect()
    }

    /// Where the file has a captured occurrence at the item `index` of
    /// its timeline: where its placements put it before the item
    /// (mitcad#81), else its last captured placement before the item, else
    /// its stored transform. None for an occurrence that neither places
    /// (it stays at its stored transform).
    pub(crate) fn placement_at(&self, occurrence: OccurrenceUid, index: i64) -> Option<Transform> {
        let index = self.timeline_at(index);
        if let Some(steps) = self.local_steps.get(&occurrence) {
            return steps
                .iter()
                .filter(|(at, _)| *at < index)
                .max_by_key(|(at, _)| *at)
                .or(steps.first())
                .map(|(_, t)| *t);
        }
        let (stored, captured) = self.placements.get(&occurrence)?;
        Some(
            captured
                .iter()
                .take_while(|(at, _)| *at < index)
                .last()
                .map_or(*stored, |(_, t)| *t),
        )
    }
}

/// One position of a captured position as the decoder gives it.
struct Entry {
    index: i64,
    /// The component its path starts in.
    context: ComponentUid,
    path: Vec<Option<u64>>,
    guids: Vec<String>,
    matrix: Option<Mat4>,
}

/// The file's captured positions (see the module documentation).
pub(crate) fn read(
    dump: &Dump,
    components: &Components,
    assembly: &Assembly,
    outer: &HashMap<u64, Steps>,
) -> Captures {
    let mut entries = Vec::new();
    let mut out = Captures::default();
    let mut snapshots = Vec::new();
    for (position, item) in dump.timeline_items().iter().enumerate() {
        if let (Some(index), Some(of)) = (item.index, item.f3d.as_ref().and_then(|f| f.part_of)) {
            out.parts.insert(index, of);
        }
        if item.object_type() != Some("Snapshot") {
            continue;
        }
        let index = item.index.unwrap_or(position as i64);
        snapshots.push(index);
        let owner = item
            .f3d
            .as_ref()
            .and_then(|f| f.component)
            .and_then(|c| components.of_object(c))
            .unwrap_or(ComponentUid::ROOT);
        let detail = item
            .detail
            .as_ref()
            .and_then(|d| serde_json::to_value(d).ok())
            .unwrap_or(Value::Null);
        let Some(positions) = detail["positions"].as_array() else {
            continue;
        };
        out.items.insert(index, Captured::default());
        for p in positions {
            let f3d = &p["occurrence"]["_f3d"];
            let ids = |key: &str| f3d[key].as_array().cloned().unwrap_or_default();
            entries.push(Entry {
                index,
                context: f3d["context_component"]
                    .as_u64()
                    .and_then(|c| components.of_object(c))
                    .unwrap_or(owner),
                path: ids("path").iter().map(Value::as_u64).collect(),
                guids: ids("path_guids")
                    .iter()
                    .map(|g| g.as_str().unwrap_or_default().to_owned())
                    .collect(),
                matrix: serde_json::from_value(p["transform"].clone()).ok(),
            });
        }
    }
    // Levels not found: the occurrence placing the next level's parent,
    // learnt by their ids for the other paths.
    let mut learnt: HashMap<String, OccurrenceUid> = HashMap::new();
    for e in &entries {
        let mut component = e.context;
        for (k, id) in e.path.iter().enumerate() {
            let here = match id.and_then(|o| components.occurrence(o)) {
                Some(o) => Some(o),
                None => e
                    .path
                    .get(k + 1)
                    .copied()
                    .flatten()
                    .and_then(|next| components.occurrence(next))
                    .and_then(|next| assembly.occurrence(next))
                    .and_then(|next| only_placement(assembly, next.parent, component))
                    .inspect(|o| {
                        if let Some(g) = e.guids.get(k).filter(|g| !g.is_empty()) {
                            learnt.insert(g.clone(), *o);
                        }
                    }),
            };
            let Some(o) = here.and_then(|o| assembly.occurrence(o)) else {
                break;
            };
            component = o.component;
        }
    }
    // Each position in its parent's coordinates, parents first; where a
    // path's parents are not captured, their placements so far.
    let mut by_index: BTreeMap<i64, Vec<&Entry>> = BTreeMap::new();
    for e in &entries {
        by_index.entry(e.index).or_default().push(e);
    }
    for (index, mut list) in by_index {
        list.sort_by_key(|e| e.path.len());
        let captured = out.items.entry(index).or_default();
        for e in list {
            let path = match resolve(e, components, assembly, &learnt) {
                Ok(path) => path,
                Err(Unresolved::Inside(outer, inner)) => {
                    if let Some(placement) = e.matrix.as_ref().and_then(components::transform) {
                        out.inside.push(InsidePosition {
                            index,
                            outer,
                            inner,
                            placement,
                        });
                    }
                    captured.left_out.push(INSIDE.to_owned());
                    continue;
                }
                Err(Unresolved::Why(why)) => {
                    captured.left_out.push(why);
                    continue;
                }
            };
            let Some(world) = e.matrix.as_ref().and_then(components::transform) else {
                captured
                    .left_out
                    .push("its placement is not rigid".to_owned());
                continue;
            };
            let (&last, parents) = path.split_last().expect("a path");
            let Some(occurrence) = assembly.occurrence(last) else {
                continue;
            };
            if occurrence.grounded {
                captured
                    .left_out
                    .push(format!("{} is grounded", assembly.occurrence_name(last)));
                continue;
            }
            let parent = parents.iter().fold(Transform::IDENTITY, |t, o| {
                let stored = assembly
                    .occurrence(*o)
                    .map_or(Transform::IDENTITY, |x| x.transform);
                t.after(&out.last.get(o).copied().unwrap_or(stored))
            });
            let local = inverse(&parent).after(&world);
            captured.positions.retain(|(o, _)| *o != last);
            captured.positions.push((last, local));
            out.last.insert(last, local);
            let (_, by_index) = out
                .placements
                .entry(last)
                .or_insert_with(|| (occurrence.transform, Vec::new()));
            by_index.retain(|(at, _)| *at != index);
            by_index.push((index, local));
        }
    }
    read_steps(dump, components, assembly, outer, &mut out);
    // What the occurrences' placements say a captured position placed
    // beyond the positions decoded: all of them where none are (those of
    // joints' values), else those its paths do not name (an empty path).
    for index in snapshots {
        let mut placed: Vec<OccurrenceUid> = out
            .path_steps
            .iter()
            .filter(|(_, steps)| steps.iter().any(|(i, _)| *i == Some(index)))
            .map(|(o, _)| *o)
            .collect();
        placed.sort();
        let mut added = Vec::new();
        let mut grounded = Vec::new();
        for o in placed {
            let Some(occurrence) = assembly.occurrence(o) else {
                continue;
            };
            let known = out
                .items
                .get(&index)
                .is_some_and(|c| c.positions.iter().any(|(p, _)| *p == o));
            if known {
                continue;
            }
            if occurrence.grounded {
                grounded.push(format!("{} is grounded", assembly.occurrence_name(o)));
                continue;
            }
            if let Some(local) = out.placement_after(assembly, o, index) {
                added.push((o, local));
            }
        }
        if added.is_empty() && grounded.is_empty() {
            continue;
        }
        let captured = out.items.entry(index).or_default();
        for _ in &added {
            // An empty path the placements made up for.
            if let Some(k) = captured
                .left_out
                .iter()
                .position(|w| w == "a position names no occurrence")
            {
                captured.left_out.remove(k);
            }
        }
        captured.positions.extend(added);
        captured.left_out.extend(grounded);
    }
    out
}

/// The occurrences' placements (mitcad#81): each step in the root
/// component, and the last one in the parent as the occurrence's last
/// placement (it wins over the captured positions, which it includes).
fn read_steps(
    dump: &Dump,
    components: &Components,
    assembly: &Assembly,
    outer: &HashMap<u64, Steps>,
    out: &mut Captures,
) {
    fn walk(
        nodes: &[OccurrenceNode],
        parent: Option<&PathPlacements>,
        components: &Components,
        outer: &HashMap<u64, Steps>,
        out: &mut Captures,
    ) {
        for n in nodes {
            let f3d = n.f3d.as_ref();
            let matrix = match parent {
                None => n.transform,
                Some(_) => f3d.and_then(|f| f.local_transform),
            };
            let stored_local = matrix
                .as_ref()
                .and_then(components::transform)
                .unwrap_or(Transform::IDENTITY);
            let stored = match n.transform2.as_ref().and_then(components::transform) {
                Some(world) => world,
                None => parent.map_or(stored_local, |p| p.stored.after(&stored_local)),
            };
            // An occurrence of a component of another document placed along
            // paths inside it: its own placements worked out from those
            // (`references::outer_placements`, mitcad#1892).
            let worked_out = f3d
                .and_then(|f| f.object_id)
                .and_then(|id| outer.get(&id))
                .cloned();
            let read: Option<Vec<(Option<i64>, Transform)>> = f3d
                .and_then(|f| f.placements.as_ref())
                .and_then(|steps| {
                    steps
                        .iter()
                        .map(|s| Some((s.index, components::transform(s.transform.as_ref()?)?)))
                        .collect()
                })
                .filter(|r: &Vec<_>| !r.is_empty());
            let read = worked_out.or(read);
            let o = f3d
                .and_then(|f| f.object_id)
                .and_then(|id| components.occurrence(id));
            if let (Some(read), Some(o)) = (&read, o) {
                out.steps.entry(o).or_insert_with(|| read.clone());
                let local = out.path_steps.entry(o).or_default();
                for &(index, world) in read {
                    if local.iter().any(|(at, _)| *at == index) {
                        continue;
                    }
                    let placed = match parent {
                        Some(p) => inverse(&p.after(index)).after(&world),
                        None => world,
                    };
                    local.push((index, placed));
                }
                local.sort_by_key(|(at, _)| *at);
            }
            if n.is_referenced_component != Some(true) {
                // The children's parent path: this occurrence's steps, else
                // its parent's moved by its stored placement.
                let steps = match read {
                    Some(read) => read,
                    None => parent.map_or_else(Vec::new, |p| {
                        p.steps
                            .iter()
                            .map(|&(i, t)| (i, t.after(&stored_local)))
                            .collect()
                    }),
                };
                let path = PathPlacements { steps, stored };
                walk(
                    n.children.as_deref().unwrap_or_default(),
                    Some(&path),
                    components,
                    outer,
                    out,
                );
            }
        }
    }
    walk(
        dump.occurrences.as_deref().unwrap_or_default(),
        None,
        components,
        outer,
        out,
    );
    // Parents first, so that each local placement is made relative to the
    // parent's last one.
    let mut placed: Vec<(usize, OccurrenceUid)> = out
        .steps
        .keys()
        .map(|o| (depth(assembly, *o), *o))
        .collect();
    placed.sort();
    for (_, o) in placed {
        let Some(occurrence) = assembly.occurrence(o) else {
            continue;
        };
        if occurrence.grounded {
            continue;
        }
        let Some(world) = out.steps[&o].last().map(|(_, t)| *t) else {
            continue;
        };
        let parent = parent_world(assembly, out, occurrence.parent);
        let local = match parent {
            Some(p) => inverse(&p).after(&world),
            None if occurrence.parent.is_root() => world,
            // A parent placed several times: the last step along its path.
            None => match out.path_steps.get(&o).and_then(|s| s.last()) {
                Some(&(_, t)) => t,
                None => continue,
            },
        };
        out.last.insert(o, local);
    }
    let locals: HashMap<OccurrenceUid, Vec<(i64, Transform)>> = out
        .path_steps
        .iter()
        .map(|(o, steps)| {
            let list = steps
                .iter()
                .filter_map(|(i, _)| {
                    Some((
                        i.as_ref().copied()?,
                        out.placement_after(assembly, *o, (*i)?)?,
                    ))
                })
                .collect();
            (*o, list)
        })
        .collect();
    out.local_steps = locals;
}

/// Where one occurrence path is in the root component: after each item
/// that placed it (in timeline order) and as stored.
struct PathPlacements {
    steps: Vec<(Option<i64>, Transform)>,
    stored: Transform,
}

impl PathPlacements {
    /// The path's placement after the item at `index` (its last step where
    /// the item has no index), else as stored.
    fn after(&self, index: Option<i64>) -> Transform {
        let step = match index {
            Some(index) => self
                .steps
                .iter()
                .filter(|(i, _)| i.is_some_and(|i| i <= index))
                .max_by_key(|(i, _)| *i),
            None => self.steps.last(),
        };
        step.map_or(self.stored, |(_, t)| *t)
    }
}

/// How deep an occurrence is placed (0: in the root component).
fn depth(assembly: &Assembly, occurrence: OccurrenceUid) -> usize {
    let mut n = 0;
    let mut component = assembly.occurrence(occurrence).map(|o| o.parent);
    while let Some(c) = component.filter(|c| !c.is_root()) {
        n += 1;
        component = only_placement_anywhere(assembly, c)
            .and_then(|o| assembly.occurrence(o))
            .map(|o| o.parent);
        if n > 64 {
            break;
        }
    }
    n
}

/// The last placement in the root component of the (one) occurrence
/// placing `component`; None for the root.
fn parent_world(assembly: &Assembly, out: &Captures, component: ComponentUid) -> Option<Transform> {
    if component.is_root() {
        return None;
    }
    let o = only_placement_anywhere(assembly, component)?;
    let x = assembly.occurrence(o)?;
    let local = out.final_placement(o, x.transform);
    Some(match parent_world(assembly, out, x.parent) {
        Some(up) => up.after(&local),
        None => local,
    })
}

/// The one occurrence placing `component` anywhere, if there is one.
fn only_placement_anywhere(assembly: &Assembly, component: ComponentUid) -> Option<OccurrenceUid> {
    let mut found = assembly
        .occurrences
        .iter()
        .filter(|o| o.component == component);
    let first = found.next()?;
    found.next().is_none().then_some(first.uid)
}

/// The one occurrence placing `component` in `parent`, if there is one.
fn only_placement(
    assembly: &Assembly,
    component: ComponentUid,
    parent: ComponentUid,
) -> Option<OccurrenceUid> {
    let mut found = assembly
        .occurrences
        .iter()
        .filter(|o| o.component == component && o.parent == parent);
    let first = found.next()?;
    found.next().is_none().then_some(first.uid)
}

/// A position's path as Mitcad's occurrences, each placed in the
/// component of the one before (the first in the captured position's
/// component).
fn resolve(
    e: &Entry,
    components: &Components,
    assembly: &Assembly,
    learnt: &HashMap<String, OccurrenceUid>,
) -> Result<Vec<OccurrenceUid>, Unresolved> {
    if e.path.is_empty() {
        return Err(Unresolved::Why("a position names no occurrence".to_owned()));
    }
    let mut component = e.context;
    let mut path = Vec::new();
    for (k, id) in e.path.iter().enumerate() {
        let found = id
            .and_then(|o| components.occurrence(o))
            .or_else(|| e.guids.get(k).and_then(|g| learnt.get(g)).copied());
        let Some(o) = found.and_then(|o| assembly.occurrence(o)) else {
            let inside = path
                .last()
                .and_then(|o| assembly.occurrence(*o))
                .is_some_and(|o| components.is_inserted(o.component));
            return Err(if inside {
                Unresolved::Inside(path, e.guids.get(k..).unwrap_or_default().to_vec())
            } else {
                Unresolved::Why("a position of an occurrence not in the occurrence tree".to_owned())
            });
        };
        if o.parent != component {
            return Err(Unresolved::Why(format!(
                "a position of {}, which is not placed where its path says",
                assembly.occurrence_name(o.uid)
            )));
        }
        path.push(o.uid);
        component = o.component;
    }
    Ok(path)
}

/// Why a captured position's path is not one of this design's
/// occurrences ([`resolve`]).
enum Unresolved {
    Why(String),
    /// Inside a component of another document: the path's occurrences
    /// down to the one of that component, and the GUIDs of the levels
    /// inside it.
    Inside(Vec<OccurrenceUid>, Vec<String>),
}

/// Why a captured position inside a component of another document is left
/// out of the replay.
pub(crate) const INSIDE: &str = "a position inside a component of another document";
