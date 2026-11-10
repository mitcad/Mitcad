// SPDX-License-Identifier: MIT
//! Animated USDZ export (mitcad#1597): a mechanism's motion as
//! time-sampled placements of the occurrences' `Xform`s and the bodies'
//! meshes, the meshes and materials written once (`mitcad_usdz`'s
//! [`mitcad_usdz::Animation`]).
//!
//! - **Frames:** `fps` frames per second (the layer's `timeCodesPerSecond`
//!   and `framesPerSecond`), at the time codes `times` (increasing; the
//!   first is the layer's `startTimeCode`, the last its `endTimeCode`).
//!   Readers interpolate linearly between the samples, matrix element by
//!   element: exact at the frames, so large turns want dense frames.
//! - **Poses, given:** a track per target, one rigid transform per frame
//!   (MitCad's layout, `p' = M p`: rows of the rotation and the translation
//!   in the last column). An occurrence target is a path from the root:
//!   each placement of a component placed twice, or of a nested
//!   occurrence, is its own path and moves on its own. A body target moves
//!   the body's mesh; it is refused when the body is written by more than
//!   one placement (ambiguous: name the occurrence instead). In the
//!   `parent` space (the default) a pose is in the coordinates of the
//!   target's parent: an occurrence's placement in its parent occurrence
//!   (as its own transform is), a body's motion in its component's
//!   coordinates (identity: where it is). In the `design` space a pose is
//!   in the design's coordinates: where an occurrence's component, or the
//!   body, is in the design at that frame (identity: at the design's
//!   origin); it is turned into the parent's coordinates at each frame,
//!   with the parents' own motion. Targets not given stay where the
//!   design has them.
//! - **Poses from a motion study:** each frame's local placements of the
//!   occurrences, the study's keyed joints driven to their values at that
//!   frame's step and solved as Play Study solves them
//!   ([`Document::study_placements`]); the occurrences written whose
//!   placement differs from the design's at some frame are sampled.
//! - Millimetres and the design's Z up, as the still export: the root's
//!   quarter turn (Z up to Y up) applies to the samples too.
//! - Sampling changes nothing in the document: not its pose, not its undo
//!   steps.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use mitcad_usdz::{Node, Placement};

use super::invalid;
use crate::assembly::{inverse, is_rigid};
use crate::document::{Document, ModelError};
use crate::ids::{BodyUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::transform::Transform;

/// The most frames of an animation.
pub const MAX_FRAMES: usize = 100_000;
/// The most samples (frames times moving targets) of an animation: each is
/// a 128-byte matrix in the file.
pub const MAX_SAMPLES: usize = 1_000_000;

/// Time-sampled placements written into a USDZ file (mitcad#1597; see the
/// module documentation).
#[derive(Debug, Clone, PartialEq)]
pub struct UsdzAnimation {
    /// Frames (time codes) per second; positive.
    pub fps: f64,
    /// The time code of each frame, increasing.
    pub times: Vec<f64>,
    pub poses: AnimationPoses,
}

/// Where an animation's poses come from.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimationPoses {
    /// Given: per target, one transform per frame.
    Tracks {
        space: AnimationSpace,
        tracks: Vec<AnimationTrack>,
    },
    /// A motion study's evaluated placements: per frame, the occurrences'
    /// placements in their parents ([`Document::study_placements`]).
    Study {
        name: String,
        placements: Vec<BTreeMap<OccurrenceUid, Transform>>,
    },
}

/// The coordinates of given poses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationSpace {
    /// The target's parent's: an occurrence's placement in its parent
    /// occurrence, a body's motion in its component.
    #[default]
    Parent,
    /// The design's.
    Design,
}

/// What a track moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimationTarget {
    /// An occurrence by its path from the root.
    Occurrence(Vec<OccurrenceUid>),
    /// A body written by one placement.
    Body(BodyUid),
}

/// A target's pose at each frame.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationTrack {
    pub target: AnimationTarget,
    pub transforms: Vec<Transform>,
}

/// What an animated export wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimationReport {
    pub fps: f64,
    /// The first and last time codes.
    pub start: f64,
    pub end: f64,
    pub frames: usize,
    /// What moves: occurrence paths by name (`Arm:1/Pin:1`) and bodies by
    /// name, in the order written.
    pub moved: Vec<String>,
}

/// Where the meshes of each written body are: per placement, the path of
/// the node (empty: the root) and the mesh's index in it.
pub(super) type MeshPlaces = BTreeMap<BodyUid, Vec<(Vec<OccurrenceUid>, usize)>>;

fn placement(t: &Transform) -> Placement {
    Placement {
        linear: t.linear,
        translation: t.translation,
    }
}

/// Whether two transforms are the same within rounding.
fn same(a: &Transform, b: &Transform) -> bool {
    let close = |x: f64, y: f64| (x - y).abs() <= 1e-12 * (1.0 + x.abs().max(y.abs()));
    (0..3).all(|r| (0..3).all(|c| close(a.linear[r][c], b.linear[r][c])))
        && (0..3).all(|k| close(a.translation[k], b.translation[k]))
}

/// Why an animation's timing cannot be written, if it cannot.
pub fn check_timing(fps: f64, times: &[f64]) -> Result<(), String> {
    if !(fps.is_finite() && fps > 0.0) {
        return Err(format!(
            "the frame rate (fps) must be a positive number of frames per second, not {fps}"
        ));
    }
    if times.is_empty() {
        return Err("an animation needs at least one frame".to_owned());
    }
    if times.len() > MAX_FRAMES {
        return Err(format!(
            "an animation has at most {MAX_FRAMES} frames, not {}",
            times.len()
        ));
    }
    if times.iter().any(|t| !t.is_finite()) {
        return Err("the frames' time codes must be finite numbers".to_owned());
    }
    if let Some(i) = times.windows(2).position(|w| w[1] <= w[0]) {
        return Err(format!(
            "the frames' time codes must increase: {} follows {}",
            times[i + 1],
            times[i]
        ));
    }
    Ok(())
}

impl<K: Kernel> Document<K> {
    /// A target's name in messages: an occurrence's path by names, a body's
    /// name.
    fn target_name(&self, target: &AnimationTarget) -> String {
        match target {
            AnimationTarget::Occurrence(path) => self.assembly().path_name(path),
            AnimationTarget::Body(body) => self.body_name(*body),
        }
    }

    /// Sets the samples of the nodes and meshes an animation moves
    /// (mitcad#1597; see the module documentation). `nodes` are the
    /// written occurrences' nodes by path (every path's parents among
    /// them), their placements not yet set; `meshes` where each written
    /// body's meshes are.
    pub(super) fn usdz_samples(
        &self,
        animation: &UsdzAnimation,
        root: &mut Node,
        nodes: &mut BTreeMap<Vec<OccurrenceUid>, Node>,
        meshes: &MeshPlaces,
    ) -> Result<AnimationReport, ModelError> {
        check_timing(animation.fps, &animation.times).map_err(invalid)?;
        let frames = animation.times.len();
        let local = |o: OccurrenceUid| self.placement(o).unwrap_or(Transform::IDENTITY);
        let mut moved = Vec::new();
        match &animation.poses {
            AnimationPoses::Tracks { space, tracks } => {
                if tracks.is_empty() {
                    return Err(invalid("an animation needs at least one track"));
                }
                if tracks.len().saturating_mul(frames) > MAX_SAMPLES {
                    return Err(invalid(format!(
                        "{} tracks of {frames} frames are more than {MAX_SAMPLES} samples",
                        tracks.len()
                    )));
                }
                // Each track's checks, and the node of each target.
                let mut occurrence_tracks: BTreeMap<Vec<OccurrenceUid>, &[Transform]> =
                    BTreeMap::new();
                let mut body_tracks: Vec<(Vec<OccurrenceUid>, usize, &[Transform])> = Vec::new();
                for (i, track) in tracks.iter().enumerate() {
                    let name = self.target_name(&track.target);
                    if tracks[..i].iter().any(|t| t.target == track.target) {
                        return Err(invalid(format!(
                            "{name} is animated by more than one track"
                        )));
                    }
                    if track.transforms.len() != frames {
                        return Err(invalid(format!(
                            "{name} has {} poses for {frames} frames: give one pose per frame",
                            track.transforms.len()
                        )));
                    }
                    for (k, t) in track.transforms.iter().enumerate() {
                        if !t.is_finite() {
                            return Err(invalid(format!(
                                "{name}, frame {k}: the pose is not finite"
                            )));
                        }
                        if !is_rigid(t) {
                            return Err(invalid(format!(
                                "{name}, frame {k}: a pose must be a rotation and a translation \
                                 (rows of the rotation with the translation last; no scale or mirror)"
                            )));
                        }
                    }
                    match &track.target {
                        AnimationTarget::Occurrence(path) => {
                            if path.is_empty() {
                                return Err(invalid(
                                    "the design itself does not move: animate its occurrences or bodies",
                                ));
                            }
                            if !nodes.contains_key(path) {
                                return Err(invalid(format!(
                                    "{name} is not written (hidden, outside the occurrence exported, \
                                     or without bodies written): it cannot move"
                                )));
                            }
                            occurrence_tracks.insert(path.clone(), track.transforms.as_slice());
                        }
                        AnimationTarget::Body(body) => {
                            let places = meshes.get(body).ok_or_else(|| {
                                invalid(format!("{name} is not among the bodies written"))
                            })?;
                            if let [(path, index)] = places.as_slice() {
                                body_tracks.push((
                                    path.clone(),
                                    *index,
                                    track.transforms.as_slice(),
                                ));
                            } else {
                                let paths: Vec<String> = places
                                    .iter()
                                    .map(|(p, _)| self.assembly().path_name(p))
                                    .collect();
                                return Err(invalid(format!(
                                    "{name} is written by {} placements ({}): animate one of them by \
                                     its occurrence instead",
                                    places.len(),
                                    paths.join(", ")
                                )));
                            }
                        }
                    }
                    moved.push(name);
                }
                // The design's coordinates of the parents at each frame, with
                // their own motion: for poses in the design's space.
                let mut worlds: BTreeMap<Vec<OccurrenceUid>, Vec<Transform>> = BTreeMap::new();
                if *space == AnimationSpace::Design {
                    let mut needed: BTreeSet<Vec<OccurrenceUid>> = BTreeSet::new();
                    for path in occurrence_tracks.keys() {
                        needed.extend((1..path.len()).map(|n| path[..n].to_vec()));
                    }
                    for (path, _, _) in &body_tracks {
                        needed.extend((1..=path.len()).map(|n| path[..n].to_vec()));
                    }
                    worlds.insert(Vec::new(), vec![Transform::IDENTITY; frames]);
                    // Parents before children.
                    let mut needed: Vec<Vec<OccurrenceUid>> = needed.into_iter().collect();
                    needed.sort_by_key(Vec::len);
                    for path in needed {
                        let parent = &worlds[&path[..path.len() - 1]];
                        let world: Vec<Transform> = match occurrence_tracks.get(&path) {
                            Some(given) => given.to_vec(),
                            None => {
                                let own = local(*path.last().expect("not the root"));
                                parent.iter().map(|p| p.after(&own)).collect()
                            }
                        };
                        worlds.insert(path, world);
                    }
                }
                // The parent's coordinates of a pose at a frame.
                let in_parent = |parent: &[OccurrenceUid], k: usize, pose: &Transform| match space {
                    AnimationSpace::Parent => *pose,
                    AnimationSpace::Design => inverse(&worlds[parent][k]).after(pose),
                };
                for (path, given) in &occurrence_tracks {
                    let parent = &path[..path.len() - 1];
                    let samples = (0..frames)
                        .map(|k| placement(&in_parent(parent, k, &given[k])))
                        .collect();
                    nodes.get_mut(path).expect("checked").samples = samples;
                }
                for (path, index, given) in &body_tracks {
                    let samples = (0..frames)
                        .map(|k| placement(&in_parent(path, k, &given[k])))
                        .collect();
                    let node = if path.is_empty() {
                        &mut *root
                    } else {
                        nodes.get_mut(path).expect("a written body's node")
                    };
                    node.meshes[*index].samples = samples;
                }
            }
            AnimationPoses::Study { name, placements } => {
                if placements.len() != frames {
                    return Err(invalid(format!(
                        "{name} was sampled at {} steps for {frames} frames",
                        placements.len()
                    )));
                }
                for (path, node) in nodes.iter_mut() {
                    let occurrence = *path.last().expect("not the root");
                    let own = local(occurrence);
                    let poses: Vec<Transform> = placements
                        .iter()
                        .map(|frame| frame.get(&occurrence).copied().unwrap_or(own))
                        .collect();
                    if poses.iter().all(|p| same(p, &own)) {
                        continue;
                    }
                    if moved.len().saturating_add(1).saturating_mul(frames) > MAX_SAMPLES {
                        return Err(invalid(format!(
                            "{name} moves too many occurrences over {frames} frames: more than \
                             {MAX_SAMPLES} samples"
                        )));
                    }
                    node.samples = poses.iter().map(placement).collect();
                    moved.push(self.assembly().path_name(path));
                }
                if moved.is_empty() {
                    return Err(invalid(format!(
                        "{name} moves none of the occurrences written over the frames asked for"
                    )));
                }
            }
        }
        Ok(AnimationReport {
            fps: animation.fps,
            start: animation.times[0],
            end: *animation.times.last().expect("checked"),
            frames,
            moved,
        })
    }
}
