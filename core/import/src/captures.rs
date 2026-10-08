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

use mitcad_f3d::design::ir::{Dump, Mat4, OccurrenceNode};
use mitcad_model::assembly::{Assembly, inverse};
use mitcad_model::{ComponentUid, OccurrenceUid, Transform};
use serde_json::Value;

use crate::components::{self, Components};

/// The occurrences one captured position places: in their parents'
/// coordinates, and why others it lists are left out.
#[derive(Debug, Clone, Default)]
pub(crate) struct Captured {
    pub positions: Vec<(OccurrenceUid, Transform)>,
    pub left_out: Vec<String>,
}

/// The file's captured positions in Mitcad's terms.
#[derive(Debug, Default)]
pub(crate) struct Captures {
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
}

impl Captures {
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
        let world = self.world_after(occurrence, index)?;
        let parent = assembly
            .occurrence(occurrence)
            .and_then(|o| self.parent_world_after(assembly, o.parent, index));
        Some(match parent {
            Some(p) => inverse(&p).after(&world),
            None => world,
        })
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

    /// The occurrences a captured position places, each at its last
    /// captured placement in its parent.
    pub fn last(&self) -> BTreeMap<OccurrenceUid, Transform> {
        self.last.iter().map(|(o, t)| (*o, *t)).collect()
    }

    /// Where the file has a captured occurrence at the item `index` of
    /// its timeline: where its placements put it before the item
    /// (mitcad#81), else its last captured placement before the item, else
    /// its stored transform. None for an occurrence that neither places
    /// (it stays at its stored transform).
    pub fn placement_at(&self, occurrence: OccurrenceUid, index: i64) -> Option<Transform> {
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
pub(crate) fn read(dump: &Dump, components: &Components, assembly: &Assembly) -> Captures {
    let mut entries = Vec::new();
    let mut out = Captures::default();
    let mut snapshots = Vec::new();
    for (position, item) in dump.timeline_items().iter().enumerate() {
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
                Err(why) => {
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
    read_steps(dump, components, assembly, &mut out);
    // What the occurrences' placements say a captured position placed
    // beyond the positions decoded: all of them where none are (those of
    // joints' values), else those its paths do not name (an empty path).
    for index in snapshots {
        let mut placed: Vec<OccurrenceUid> = out
            .steps
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
fn read_steps(dump: &Dump, components: &Components, assembly: &Assembly, out: &mut Captures) {
    fn walk(nodes: &[OccurrenceNode], components: &Components, out: &mut Captures) {
        for n in nodes {
            let f3d = n.f3d.as_ref();
            if let (Some(steps), Some(o)) = (
                f3d.and_then(|f| f.placements.as_ref()),
                f3d.and_then(|f| f.object_id)
                    .and_then(|id| components.occurrence(id)),
            ) {
                let read: Option<Vec<(Option<i64>, Transform)>> = steps
                    .iter()
                    .map(|s| Some((s.index, components::transform(s.transform.as_ref()?)?)))
                    .collect();
                if let Some(read) = read.filter(|r| !r.is_empty()) {
                    out.steps.insert(o, read);
                }
            }
            if n.is_referenced_component != Some(true) {
                walk(n.children.as_deref().unwrap_or_default(), components, out);
            }
        }
    }
    walk(
        dump.occurrences.as_deref().unwrap_or_default(),
        components,
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
            None => world,
        };
        out.last.insert(o, local);
    }
    let locals: HashMap<OccurrenceUid, Vec<(i64, Transform)>> = out
        .steps
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
) -> Result<Vec<OccurrenceUid>, String> {
    if e.path.is_empty() {
        return Err("a position names no occurrence".to_owned());
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
                "a position inside a component of another document".to_owned()
            } else {
                "a position of an occurrence not in the occurrence tree".to_owned()
            });
        };
        if o.parent != component {
            return Err(format!(
                "a position of {}, which is not placed where its path says",
                assembly.occurrence_name(o.uid)
            ));
        }
        path.push(o.uid);
        component = o.component;
    }
    Ok(path)
}
