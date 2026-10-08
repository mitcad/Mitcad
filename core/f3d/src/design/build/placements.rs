// SPDX-License-Identifier: MIT
//! Where the items of the timeline put each occurrence (mitcad#81)
//! *(read from the joint test model, whose joints and captured positions
//! move its parts, and the corpus' fasteners on joints)*.
//!
//! An occurrence's placements ([`OCCURRENCE_PLACEMENTS`]): after its root
//! part the [`OCCURRENCE`]s of its path (none for an occurrence whose path
//! the item that made it gives), the component the path starts in, then
//!
//! ```text
//! u32 | u32 n | n × (u8 1 | u8 0 | ref item | u8 0 + f64[16] matrix, or u8 1)
//! ```
//!
//! The first item is the one that made the occurrence (an occurrence item,
//! a fastener, an insert), the others those that moved it (joints,
//! captured positions, rigid groups through them); each matrix is the
//! path's placement in the component after that item (the identity for
//! `u8 1`). The occurrence's own stored transform is the first one: the
//! joints after it do not change it, so the last placement is where the
//! file shows the occurrence.

use std::collections::HashMap;

use super::super::classes::*;
use super::super::ir::{IDENTITY, Mat4, OccurrenceNode, PlacementStep};
use super::super::stream::{Segment, u32_at};
use super::{item_occurrence, rigid_matrix};

/// The most steps read from one object.
const MAX_STEPS: u32 = 100_000;

/// One occurrence's placements.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct History {
    /// The occurrences of the path, from the component down; empty when
    /// the item that made the occurrence names it.
    pub path: Vec<u64>,
    /// The component the path starts in.
    pub context: u64,
    /// Each item (object id) with the path's placement after it.
    pub steps: Vec<(u64, Mat4)>,
}

/// An object's placements (see the module documentation); None when it is
/// not one (the class also holds other lists) or a level is in another
/// document.
pub(super) fn history(seg: &Segment, id: u64) -> Option<History> {
    let o = seg.object(id)?;
    let d = seg.data(o);
    let refs = seg.refs_in(d, 0, seg.main_end(o));
    let k = refs
        .iter()
        .position(|(_, r)| seg.guid_of(r.id) == Some(COMPONENT))?;
    let (_, component) = &refs[k];
    if component.context.is_some() {
        return None;
    }
    let mut path = Vec::new();
    for (_, r) in &refs[..k] {
        if seg.guid_of(r.id) == Some(OCCURRENCE) {
            if r.context.is_some() {
                return None;
            }
            path.push(r.id);
        }
    }
    // The first step: the first reference after the component whose
    // count and marker fit.
    let (first, _) = refs[k + 1..]
        .iter()
        .find(|(p, _)| *p >= 6 && d.get(p - 2..*p) == Some(&[1, 0][..]))?;
    let n = u32_at(d, first - 6)?;
    if n == 0 || n > MAX_STEPS {
        return None;
    }
    let mut steps = Vec::new();
    let mut p = *first;
    for _ in 0..n {
        if d.get(p.checked_sub(2)?..p) != Some(&[1, 0][..]) {
            return None;
        }
        let r = seg.ref_at(d, p)?;
        let (matrix, end) = match *d.get(r.end)? {
            0 => (rigid_matrix(d.get(r.end + 1..r.end + 129)?)?, r.end + 129),
            1 => (IDENTITY, r.end + 1),
            _ => return None,
        };
        steps.push((r.id, matrix));
        p = end + 2;
    }
    Some(History {
        path,
        context: component.id,
        steps,
    })
}

/// Every occurrence's placements in the root component `root`: by path
/// where the object names it, else by the occurrence the first item made.
pub(super) struct Histories {
    by_path: HashMap<Vec<u64>, Vec<(u64, Mat4)>>,
    by_occurrence: HashMap<u64, Vec<(u64, Mat4)>>,
}

impl Histories {
    pub(super) fn new(seg: &Segment, root: Option<u64>) -> Self {
        let mut out = Histories {
            by_path: HashMap::new(),
            by_occurrence: HashMap::new(),
        };
        for o in seg.objects_of(OCCURRENCE_PLACEMENTS) {
            let Some(h) = history(seg, o.id) else {
                continue;
            };
            if Some(h.context) != root {
                continue;
            }
            if !h.path.is_empty() {
                out.by_path.insert(h.path, h.steps);
            } else if let Some(occurrence) = h
                .steps
                .first()
                .and_then(|(item, _)| item_occurrence(seg, *item))
            {
                out.by_occurrence.insert(occurrence, h.steps);
            }
        }
        out
    }

    /// Writes each node's placements (`_f3d.placements`), items by their
    /// timeline index (`index_of`). An occurrence its item names goes on
    /// its node only where the tree has one node of it.
    pub(super) fn attach(&self, nodes: &mut [OccurrenceNode], index_of: &HashMap<u64, usize>) {
        let mut count: HashMap<u64, usize> = HashMap::new();
        count_nodes(nodes, &mut count);
        let mut path = Vec::new();
        self.walk(nodes, &mut path, &count, index_of);
    }

    fn walk(
        &self,
        nodes: &mut [OccurrenceNode],
        path: &mut Vec<u64>,
        count: &HashMap<u64, usize>,
        index_of: &HashMap<u64, usize>,
    ) {
        for n in nodes {
            let Some(id) = n.f3d.as_ref().and_then(|f| f.object_id) else {
                continue;
            };
            path.push(id);
            let steps = self.by_path.get(&*path).or_else(|| {
                (count.get(&id) == Some(&1))
                    .then(|| self.by_occurrence.get(&id))
                    .flatten()
            });
            if let (Some(steps), Some(f3d)) = (steps, n.f3d.as_mut()) {
                f3d.placements = Some(
                    steps
                        .iter()
                        .map(|(item, m)| PlacementStep {
                            index: index_of.get(item).map(|&i| i as i64),
                            object_id: Some(*item),
                            transform: Some(*m),
                            ..PlacementStep::default()
                        })
                        .collect(),
                );
            }
            if let Some(children) = n.children.as_mut() {
                self.walk(children, path, count, index_of);
            }
            path.pop();
        }
    }
}

fn count_nodes(nodes: &[OccurrenceNode], count: &mut HashMap<u64, usize>) {
    for n in nodes {
        if let Some(id) = n.f3d.as_ref().and_then(|f| f.object_id) {
            *count.entry(id).or_default() += 1;
        }
        if let Some(children) = n.children.as_ref() {
            count_nodes(children, count);
        }
    }
}
