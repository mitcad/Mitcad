// SPDX-License-Identifier: MIT
//! Contact sets while joints move (mitcad#454): the bodies of a contact
//! set's occurrences may touch but not pass through each other.
//!
//! The bodies of a set are the visible bodies placed in the design whose
//! occurrence paths start with one of the set's occurrences (a body belongs
//! to the innermost one); bodies of different occurrences of one set are
//! the pairs to check, once each however many sets hold them. Suppressed
//! sets do nothing.
//!
//! A pair passes through when the kernel finds more than [`MIN_OVERLAP`]
//! of common volume ([`Kernel::overlap_volumes`]); pairs whose boxes, moved
//! by their placements, stay more than [`TOLERANCE`] apart cost no kernel
//! call. A motion (Drive Joint, dragging, animating, a study's playback)
//! is sampled from where it starts ([`sweep`]); at the first sample with a
//! pair passing through, the step before it is bisected down to where the
//! bodies touch, and the motion stops there. Pairs that already pass
//! through each other where the motion starts are left out, so that an
//! assembly that overlaps can still move apart.

use std::collections::{BTreeMap, BTreeSet};

use crate::document::DocState;
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::joints::path_placement;
use crate::kernel::{BoundingBox, Kernel, KernelError, PlacedPair};
use crate::recompute::Recomputed;
use crate::transform::Transform;

/// Faces of two bodies nearer than this (mm) touch: the boxes' gap that
/// still asks the kernel, and the kernel's tolerance.
pub(crate) const TOLERANCE: f64 = 0.01;
/// Common volume (mm³) beyond which two bodies pass through each other.
pub(crate) const MIN_OVERLAP: f64 = 1e-3;
/// Halvings of the step in which the first contact lies.
const BISECTIONS: usize = 14;

/// A body of a contact set as placed in the design.
#[derive(Debug, Clone, PartialEq)]
pub struct ContactBody {
    /// The occurrences from the root to its component.
    pub path: Vec<OccurrenceUid>,
    pub body: BodyUid,
}

/// Two bodies of a contact set that a motion would push through each
/// other: the motion stopped where they touch.
#[derive(Debug, Clone, PartialEq)]
pub struct ContactHit {
    /// The contact set's name.
    pub set: String,
    pub a: ContactBody,
    pub b: ContactBody,
    /// Their common volume where the motion was stopped short of, mm³.
    pub volume: f64,
}

/// The bodies of the active contact sets and the pairs to check.
#[doc(hidden)]
pub struct ContactPairs<S> {
    bodies: Vec<(ContactBody, S, Option<BoundingBox>)>,
    /// Indices into `bodies`, with the set that holds the pair.
    pairs: Vec<(usize, usize, usize)>,
    sets: Vec<String>,
}

impl<S: Clone> ContactPairs<S> {
    /// The pairs of the active contact sets of `state` with the bodies a
    /// recompute of it gave (the shapes do not change while joints move).
    pub fn new<K: Kernel<Shape = S>>(kernel: &K, state: &DocState, result: &Recomputed<S>) -> Self {
        let mut out = Self {
            bodies: Vec::new(),
            pairs: Vec::new(),
            sets: Vec::new(),
        };
        let active: Vec<_> = state
            .contact_sets
            .iter()
            .filter(|s| !s.suppressed && s.occurrences.len() > 1)
            .collect();
        if active.is_empty() {
            return out;
        }
        let assembly = state.assembly.at_placements(&result.placements);
        let placement = |o: OccurrenceUid| {
            result
                .placements
                .get(&o)
                .copied()
                .or_else(|| assembly.occurrence(o).map(|o| o.transform))
                .unwrap_or(Transform::IDENTITY)
        };
        let owners = &result.owners;
        let bodies = |c: ComponentUid| -> Vec<BodyUid> {
            owners
                .iter()
                .filter(|(_, owner)| **owner == c)
                .map(|(b, _)| *b)
                .collect()
        };
        // Inside flexible occurrences where they are for them (mitcad#1793).
        let instances: Vec<_> = assembly
            .instances_flexed(&placement, &result.joints.flexed, &bodies)
            .into_iter()
            .filter(|i| i.visible && state.body_attributes.get(&i.body).is_none_or(|a| a.visible))
            .collect();
        let mut index: BTreeMap<(Vec<OccurrenceUid>, BodyUid), usize> = BTreeMap::new();
        let mut seen: BTreeSet<(usize, usize)> = BTreeSet::new();
        for set in active {
            // Each instance's member: the longest of the set's paths it
            // starts with.
            let tagged: Vec<(usize, usize)> = instances
                .iter()
                .enumerate()
                .filter_map(|(i, instance)| {
                    set.occurrences
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| instance.path.starts_with(&p.0))
                        .max_by_key(|(_, p)| p.0.len())
                        .map(|(m, _)| (i, m))
                })
                .collect();
            if tagged.is_empty() {
                continue;
            }
            let set_index = out.sets.len();
            out.sets.push(set.name.clone());
            let mut slot = |i: usize, out: &mut Self| -> Option<usize> {
                let instance = &instances[i];
                let key = (instance.path.clone(), instance.body);
                if let Some(at) = index.get(&key) {
                    return Some(*at);
                }
                let shape = result.bodies.get(&instance.body)?.value.clone();
                let bounds = kernel.bounding_box(&shape).ok().flatten();
                out.bodies.push((
                    ContactBody {
                        path: instance.path.clone(),
                        body: instance.body,
                    },
                    shape,
                    bounds,
                ));
                index.insert(key, out.bodies.len() - 1);
                Some(out.bodies.len() - 1)
            };
            for (k, (i, mi)) in tagged.iter().enumerate() {
                for (j, mj) in &tagged[k + 1..] {
                    if mi == mj {
                        continue;
                    }
                    let (Some(a), Some(b)) = (slot(*i, &mut out), slot(*j, &mut out)) else {
                        continue;
                    };
                    let pair = (a.min(b), a.max(b));
                    if a != b && seen.insert(pair) {
                        out.pairs.push((set_index, pair.0, pair.1));
                    }
                }
            }
        }
        out
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// The pairs (by index) that pass through each other with these
    /// placements of the occurrences (inside flexible ones where `flexed`
    /// has them, mitcad#1793), and their common volumes; the pairs
    /// in `skip` are left out.
    pub fn overlapping<K: Kernel<Shape = S>>(
        &self,
        kernel: &K,
        placements: &BTreeMap<OccurrenceUid, Transform>,
        flexed: &crate::joints::Flexed,
        skip: &BTreeSet<usize>,
    ) -> Result<Vec<(usize, f64)>, KernelError> {
        let transforms: Vec<Transform> = self
            .bodies
            .iter()
            .map(|(body, _, _)| path_placement(placements, flexed, &body.path))
            .collect();
        let boxes: Vec<Option<BoundingBox>> = self
            .bodies
            .iter()
            .zip(&transforms)
            .map(|((_, _, bounds), t)| bounds.map(|b| moved_box(&b, t)))
            .collect();
        let mut asked = Vec::new();
        let mut pairs = Vec::new();
        for (p, (_, a, b)) in self.pairs.iter().enumerate() {
            if skip.contains(&p) {
                continue;
            }
            // Boxes apart: no kernel call. A body without a box is asked.
            if let (Some(ba), Some(bb)) = (&boxes[*a], &boxes[*b])
                && !ba.overlaps(bb, TOLERANCE)
            {
                continue;
            }
            asked.push(p);
            pairs.push(PlacedPair {
                a: &self.bodies[*a].1,
                a_placement: transforms[*a],
                b: &self.bodies[*b].1,
                b_placement: transforms[*b],
            });
        }
        if pairs.is_empty() {
            return Ok(Vec::new());
        }
        let volumes = kernel.overlap_volumes(&pairs, TOLERANCE)?;
        Ok(asked
            .into_iter()
            .zip(volumes)
            .filter(|(_, v)| *v > MIN_OVERLAP)
            .collect())
    }

    /// A pair as the contact a motion stopped at.
    pub fn hit(&self, pair: usize, volume: f64) -> ContactHit {
        let (set, a, b) = self.pairs[pair];
        ContactHit {
            set: self.sets[set].clone(),
            a: self.bodies[a].0.clone(),
            b: self.bodies[b].0.clone(),
            volume,
        }
    }
}

/// A box moved by a rigid transform: the box around its moved corners.
fn moved_box(b: &BoundingBox, t: &Transform) -> BoundingBox {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for corner in b.corners() {
        let p = t.apply_point(corner);
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    BoundingBox { min, max }
}

/// Where a sampled motion stopped.
pub(crate) struct Stop<P> {
    /// How far along the motion, 0 to 1.
    pub fraction: f64,
    /// The pose there.
    pub pose: P,
    /// The contact that stopped it; None when it went the whole way.
    pub hit: Option<ContactHit>,
}

/// Samples a motion at `samples` evenly spaced points from its start
/// (`step(t)`, t from 0 to 1: the pose there and a contact it has); at the
/// first one with a contact, bisects the step before it [`BISECTIONS`]
/// times and stops at the last pose without one.
pub(crate) fn sweep<P, E>(
    samples: usize,
    mut step: impl FnMut(f64) -> Result<(P, Option<ContactHit>), E>,
) -> Result<Stop<P>, E> {
    let samples = samples.max(1);
    let mut lo = 0.0;
    let mut lo_pose: Option<P> = None;
    for k in 1..=samples {
        let t = k as f64 / samples as f64;
        let (pose, hit) = step(t)?;
        if let Some(mut found) = hit {
            let mut hi = t;
            for _ in 0..BISECTIONS {
                let mid = 0.5 * (lo + hi);
                let (pose, hit) = step(mid)?;
                match hit {
                    None => {
                        lo = mid;
                        lo_pose = Some(pose);
                    }
                    Some(h) => {
                        hi = mid;
                        found = h;
                    }
                }
            }
            let pose = match lo_pose {
                Some(pose) => pose,
                None => step(lo)?.0,
            };
            return Ok(Stop {
                fraction: lo,
                pose,
                hit: Some(found),
            });
        }
        lo = t;
        lo_pose = Some(pose);
    }
    Ok(Stop {
        fraction: 1.0,
        pose: lo_pose.expect("at least one sample"),
        hit: None,
    })
}
