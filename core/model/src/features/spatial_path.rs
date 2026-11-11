// SPDX-License-Identifier: MIT
//! Paths along curves of a 3D sketch (mitcad#1523): what a Pipe or a Sweep
//! runs along when its route is a 3D sketch, and what the other features
//! that take a path take from one (mitcad#1550): a loft's centre line and
//! rails, a path pattern, and construction planes and points along a
//! path ([`PathRef::Spatial`](super::PathRef::Spatial)). The typed path
//! names the sketch, the curves picked and the way the path runs:
//!
//! | JSON | Meaning |
//! |---|---|
//! | `{"spatial_sketch": "F3", "curves": ["c3", "c6", "c9"]}` | the curves, in any order, joined end to end |
//! | `{"spatial_sketch": "F3", "curves": ["c3"], "chain": true}` | also every curve joined to them end to end (construction curves only when listed) |
//! | `"reversed": true` | the other way |
//!
//! **Pieces.** Lines, arcs, elliptical arcs and splines (control-point and
//! fit-point ones) in any mix, construction curves too when they are
//! listed. A curve without ends (a circle, an ellipse, a closed fit-point
//! spline) is a closed path by itself (mitcad#1550), and so is a curve
//! that starts and ends at one point; it is no piece of a longer path. A
//! reference curve (geometry from outside the sketch) is no path piece:
//! draw a curve on it.
//!
//! **Closed paths** (mitcad#1550). Curves that close a loop without
//! branching are a closed path: it runs from the first listed curve's start
//! (its end when `reversed`) round to it again, along that curve's own
//! direction, the other way when `reversed`. A closed curve runs along its
//! own direction (a circle or an ellipse counter-clockwise about its
//! normal, a spline from its first point).
//!
//! **Joints are the sketch's own.** Curves join where they share a point,
//! or where their ends are points made coincident (a Coincident relation;
//! in a sketch converted from a planar one also an in-plane coincidence
//! of two points, whose ends must then still meet). Two ends that only lie
//! near each other do not join, and curves that cross do not meet there.
//! The joints, not the positions, decide the order, so a recompute keeps
//! it whatever the solve does; the curves of the path must make one chain,
//! open or closed, without branches. Chain selection (`chain`) adds the curves
//! joined to the picked ones in the same way, construction curves left out.
//!
//! **Direction.** The path runs through its first listed curve along that
//! curve's own direction (a line from its start to its end, an arc
//! counter-clockwise about its normal, a spline from its first point), the
//! other way when `reversed`: the first curve and the direction are kept
//! across recomputes and edits. Each piece is that curve's model curve in
//! the component's coordinates, mapped once from the sketch's frame, and
//! turned round when the path runs against it.
//!
//! **Associativity.** The path is evaluated from the sketch's current
//! output on every recompute (a read of it for the recompute cache): an
//! edit of the sketch moves the pipe, and a sketch that does not solve
//! fails the feature using it (no last geometry is kept). Before the
//! evaluation only the selection is checked ([`SpatialPath::check`]): the
//! sketch is a 3D sketch of the same component before the feature, and
//! each listed curve is a curve of it that can be a piece. Whether the
//! curves still make one chain is found when the feature is evaluated, so
//! an edit of the sketch that breaks the chain fails the Pipe or Sweep,
//! with the message naming the curves and points, and the sketch stays
//! editable.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::path::curves_serde;
use super::spatial_sketch::{
    SpatialConstraintKind, SpatialCurve, SpatialCurveKind, SpatialSketchDef, SpatialSketchOutput,
    spatial_output,
};
use super::{CheckContext, EvalContext, FeatureDef, References, is_false};
use crate::ids::{EntityUid, FeatureUid};
use crate::kernel::Kernel;
// Refusals in words with the points they are about, the ids and
// coordinates for agents (mitcad#1644).
use crate::notice::{NoticePlace, said, said_at};
use crate::profile::SketchFrame;
use crate::sketch::{ConstraintKind, Ref};
use crate::sweeps::PathCurve;
use mitcad_model_base::curves::{curve_ends, reversed, tolerance};
use mitcad_vec3::distance;

/// A path along curves of a 3D sketch (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialPath {
    /// The 3D sketch.
    #[serde(rename = "spatial_sketch")]
    pub sketch: FeatureUid,
    /// The curves picked; the first one sets the direction.
    #[serde(with = "curves_serde")]
    pub curves: Vec<EntityUid>,
    /// Runs against the first curve's own direction.
    #[serde(default, skip_serializing_if = "is_false")]
    pub reversed: bool,
    /// Adds the curves joined to them end to end.
    #[serde(default, skip_serializing_if = "is_false")]
    pub chain: bool,
}

/// A curve of a path and the way the path runs along it: `forward` from
/// its start to its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leg {
    pub curve: EntityUid,
    pub forward: bool,
}

fn point_name(id: EntityUid) -> String {
    format!("p{}", id.0)
}

/// `c1`, `c1 and c2`, `c1, c2 and c3`.
fn listed(curves: &[EntityUid]) -> String {
    let names: Vec<String> = curves.iter().map(|c| c.curve_name()).collect();
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// Why a curve is no path piece; None for every curve the sketch draws (a
/// closed one is a path by itself, see the module documentation).
pub fn not_a_piece(curve: &SpatialCurve) -> Option<&'static str> {
    match &curve.kind {
        SpatialCurveKind::Reference { .. } => Some(
            "is a reference curve, geometry from outside the sketch: it is no path piece (draw \
             a curve on it)",
        ),
        _ => None,
    }
}

/// Whether the route `legs` (as [`route`] gives it) is closed: one curve
/// without ends, or one whose last curve ends at the node the first starts
/// from.
pub fn is_closed_route(def: &SpatialSketchDef, legs: &[Leg]) -> bool {
    let joints = Joints::new(def);
    let node = |leg: &Leg, last: bool| {
        def.curve(leg.curve)
            .and_then(|c| joints.link(c))
            .map(|l| if leg.forward == last { l.end } else { l.start })
    };
    match (legs.first(), legs.last()) {
        (Some(first), Some(last)) => match (node(first, false), node(last, true)) {
            (None, _) | (_, None) => true,
            (Some(a), Some(b)) => a == b,
        },
        _ => false,
    }
}

/// The joints of a sketch: each point's node, the smallest of the points
/// made coincident with it (Coincident relations, and a converted sketch's
/// in-plane coincidences of two points), itself when there are none.
struct Joints {
    parent: BTreeMap<EntityUid, EntityUid>,
}

impl Joints {
    fn new(def: &SpatialSketchDef) -> Self {
        let mut joints = Self {
            parent: BTreeMap::new(),
        };
        for constraint in &def.constraints {
            let pair = match &constraint.kind {
                SpatialConstraintKind::Coincident { a, b } => Some((*a, *b)),
                SpatialConstraintKind::InPlane {
                    relation:
                        ConstraintKind::Coincident {
                            point,
                            entity: Ref::Point(other),
                        },
                    ..
                } => Some((*point, *other)),
                _ => None,
            };
            if let Some((a, b)) = pair {
                let (a, b) = (joints.node(a), joints.node(b));
                if a != b {
                    joints.parent.insert(a.max(b), a.min(b));
                }
            }
        }
        joints
    }

    fn node(&self, mut point: EntityUid) -> EntityUid {
        while let Some(next) = self.parent.get(&point) {
            point = *next;
        }
        point
    }

    /// A curve with the nodes of its start and end; None for one without
    /// ends.
    fn link(&self, curve: &SpatialCurve) -> Option<Link> {
        let [a, b] = curve.ends()?;
        Some(Link {
            curve: curve.id,
            start: self.node(a),
            end: self.node(b),
        })
    }
}

/// A curve between the nodes of its ends.
#[derive(Debug, Clone, Copy)]
struct Link {
    curve: EntityUid,
    start: EntityUid,
    end: EntityUid,
}

impl Link {
    fn touches(&self, node: EntityUid) -> bool {
        self.start == node || self.end == node
    }
}

/// From `node` on along the unused links that meet there, one after
/// another (where no node has more than two): the curves, each running
/// away from where the walk came from, and the node where it stopped.
fn walk(mut node: EntityUid, links: &[Link], used: &mut [bool]) -> (Vec<Leg>, EntityUid) {
    let mut legs = Vec::new();
    while let Some(i) = (0..links.len()).find(|i| !used[*i] && links[*i].touches(node)) {
        used[i] = true;
        let forward = links[i].start == node;
        node = if forward {
            links[i].end
        } else {
            links[i].start
        };
        legs.push(Leg {
            curve: links[i].curve,
            forward,
        });
    }
    (legs, node)
}

/// The listed curves are curves of the sketch that can be path pieces,
/// each listed once (`name` is the sketch's): what a set of a 3D sketch's
/// curves needs ([`super::SketchCurves::check_any`]).
pub(crate) fn check_curves(
    def: &SpatialSketchDef,
    curves: &[EntityUid],
    name: &str,
) -> Result<(), String> {
    check_selection(def, curves, name)
}

/// The curves of a 3D sketch as a set, not a path (mitcad#1550, see
/// [`super::SketchCurves::check_any`]): the sketch's support frame and the
/// listed curves in the component's coordinates, each along its own
/// direction, then with `chain` the curves joined to them at the sketch's
/// joints (construction curves left out), where the sketch has them now.
pub(crate) fn curve_set<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    sketch: FeatureUid,
    curves: &[EntityUid],
    chain: bool,
) -> Result<(SketchFrame, Vec<PathCurve>), String> {
    let output = spatial_output(ctx, sketch)?;
    let name = ctx.feature_name(sketch);
    let Some(FeatureDef::SpatialSketch(def)) = ctx.env.feature(sketch).map(|f| &f.def) else {
        return Err(format!("{name} is not a 3D sketch"));
    };
    check_selection(def, curves, &name)?;
    let mut ids = curves.to_vec();
    if chain {
        let joints = Joints::new(def);
        let seeds: Vec<Link> = curves
            .iter()
            .filter_map(|id| def.curve(*id).and_then(|c| joints.link(c)))
            .collect();
        ids.extend(joined(def, &joints, &seeds).iter().map(|l| l.curve));
    }
    let frame = output
        .frame
        .ok_or_else(|| format!("{name} has no support frame"))?;
    let pieces = ids
        .iter()
        .map(|id| {
            Ok(PathCurve {
                name: id.curve_name(),
                curve: output.curve(*id).ok_or_else(|| unsolved(&name, *id))?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((frame, pieces))
}

/// The listed curves are curves of the sketch that can be path pieces,
/// each listed once.
fn check_selection(def: &SpatialSketchDef, curves: &[EntityUid], name: &str) -> Result<(), String> {
    if curves.is_empty() {
        return Err("no curves selected".to_owned());
    }
    for (i, id) in curves.iter().enumerate() {
        if curves[..i].contains(id) {
            return Err(said(
                "a curve is listed more than once",
                format!("curve {} is listed more than once", id.curve_name()),
            ));
        }
        let curve = def.curve(*id).ok_or_else(|| {
            said(
                format!("{name} has no such curve"),
                format!("{name} has no curve {}", id.curve_name()),
            )
        })?;
        if let Some(why) = not_a_piece(curve) {
            return Err(said(
                format!(
                    "{name}'s curve is a reference curve, geometry from outside the sketch: it \
                     is no path piece (draw a curve on it)"
                ),
                format!("{name}'s {} {why}", id.curve_name()),
            ));
        }
    }
    Ok(())
}

/// The refusal of a curve of `name` without solved geometry: in words for
/// a person, its id for agents (mitcad#1644).
fn unsolved(name: &str, id: EntityUid) -> String {
    said(
        format!("a curve of {name} has no solved geometry"),
        format!("{name}'s {} has no solved geometry", id.curve_name()),
    )
}

/// The curves joined to `seeds` end to end, directly or through other
/// ones, in the order they are reached: curves that can be path pieces,
/// construction curves left out.
fn joined(def: &SpatialSketchDef, joints: &Joints, seeds: &[Link]) -> Vec<Link> {
    let mut nodes: BTreeSet<EntityUid> = seeds.iter().flat_map(|l| [l.start, l.end]).collect();
    let mut candidates: Vec<Link> = def
        .curves
        .iter()
        .filter(|c| !c.construction && not_a_piece(c).is_none())
        .filter(|c| !seeds.iter().any(|s| s.curve == c.id))
        .filter_map(|c| joints.link(c))
        .collect();
    let mut added = Vec::new();
    loop {
        let before = added.len();
        candidates.retain(|link: &Link| {
            if nodes.contains(&link.start) || nodes.contains(&link.end) {
                nodes.extend([link.start, link.end]);
                added.push(*link);
                false
            } else {
                true
            }
        });
        if added.len() == before {
            return added;
        }
    }
}

/// The curves of `path` in order along it, each with the way the path runs
/// along it (see the module documentation), from the sketch's definition
/// alone: its joints decide. `name` is the sketch's, `at` where a point is
/// now, for the messages.
pub fn route(
    def: &SpatialSketchDef,
    path: &SpatialPath,
    name: &str,
    at: &dyn Fn(EntityUid) -> Option<[f64; 3]>,
) -> Result<Vec<Leg>, String> {
    check_selection(def, &path.curves, name)?;
    // A curve without ends: a closed path by itself.
    if let Some(closed) = path
        .curves
        .iter()
        .find(|id| def.curve(**id).is_some_and(|c| c.ends().is_none()))
    {
        if path.curves.len() > 1 {
            return Err(said(
                "a closed curve is a closed path by itself, not a piece of a longer one",
                format!(
                    "{} is a closed curve: it is a closed path by itself, not a piece of a longer \
                     one",
                    closed.curve_name()
                ),
            ));
        }
        return Ok(vec![Leg {
            curve: *closed,
            forward: !path.reversed,
        }]);
    }
    let joints = Joints::new(def);
    let mut links: Vec<Link> = path
        .curves
        .iter()
        .filter_map(|id| def.curve(*id).and_then(|c| joints.link(c)))
        .collect();
    if path.chain {
        let more = joined(def, &joints, &links);
        links.extend(more);
    }
    let place = |node: EntityUid| match at(node) {
        Some(p) => format!(
            "{} ({:.6}, {:.6}, {:.6})",
            point_name(node),
            p[0],
            p[1],
            p[2]
        ),
        None => point_name(node),
    };
    // Where a point is, as a place painted with the message (mitcad#1644).
    let spot = |node: EntityUid| at(node).map(NoticePlace::point);
    if let Some(link) = links.iter().find(|l| l.start == l.end) {
        // A curve that closes on itself: a closed path by itself.
        if links.len() == 1 {
            return Ok(vec![Leg {
                curve: link.curve,
                forward: !path.reversed,
            }]);
        }
        return Err(said_at(
            "a curve starts and ends at one point: it closes on itself, so it is a closed path \
             by itself, not a piece of a longer one",
            format!(
                "{} starts and ends at {}: it closes on itself, so it is a closed path by \
                 itself, not a piece of a longer one",
                link.curve.curve_name(),
                place(link.start)
            ),
            spot(link.start),
        ));
    }
    let mut meeting: BTreeMap<EntityUid, Vec<EntityUid>> = BTreeMap::new();
    for link in &links {
        meeting.entry(link.start).or_default().push(link.curve);
        meeting.entry(link.end).or_default().push(link.curve);
    }
    if let Some((node, curves)) = meeting.iter().find(|(_, c)| c.len() > 2) {
        return Err(said_at(
            format!(
                "the path branches at a point: {} curves meet there",
                curves.len()
            ),
            format!(
                "the path branches at {}: {} meet there",
                place(*node),
                listed(curves)
            ),
            spot(*node),
        ));
    }
    let first = links[0];
    let mut used = vec![false; links.len()];
    used[0] = true;
    let (from, to) = if path.reversed {
        (first.end, first.start)
    } else {
        (first.start, first.end)
    };
    let not_joined = |i: usize| {
        said(
            "a curve does not join the others: the curves of a 3D sketch's path join only at a \
             shared point or at points made coincident",
            format!(
                "{} does not join the others: the curves of a 3D sketch's path join only at a \
                 shared point or at points made coincident",
                links[i].curve.curve_name()
            ),
        )
    };
    let (ahead, stop) = walk(to, &links, &mut used);
    if stop == from && !ahead.is_empty() {
        // A closed loop: from the first curve round to it again.
        if let Some(i) = used.iter().position(|u| !u) {
            return Err(not_joined(i));
        }
        let mut legs = vec![Leg {
            curve: first.curve,
            forward: !path.reversed,
        }];
        legs.extend(ahead);
        return Ok(legs);
    }
    let (behind, _) = walk(from, &links, &mut used);
    if let Some(i) = used.iter().position(|u| !u) {
        return Err(not_joined(i));
    }
    let mut legs: Vec<Leg> = behind
        .iter()
        .rev()
        .map(|leg| Leg {
            curve: leg.curve,
            forward: !leg.forward,
        })
        .collect();
    legs.push(Leg {
        curve: first.curve,
        forward: !path.reversed,
    });
    legs.extend(ahead);
    Ok(legs)
}

/// The pieces of a route in the component's coordinates, each running
/// along the path and named by its curve's id, from the sketch's output.
/// Ends that are joined must meet (points coincident only in the sketch
/// plane may have been lifted apart).
pub fn pieces(
    output: &SpatialSketchOutput,
    legs: &[Leg],
    name: &str,
) -> Result<Vec<PathCurve>, String> {
    let mut pieces = Vec::with_capacity(legs.len());
    for leg in legs {
        let curve = output
            .curve(leg.curve)
            .ok_or_else(|| unsolved(name, leg.curve))?;
        pieces.push(PathCurve {
            name: leg.curve.curve_name(),
            curve: if leg.forward { curve } else { reversed(curve) },
        });
    }
    let tolerance = tolerance(&pieces);
    for pair in pieces.windows(2) {
        let (Some((_, end)), Some((start, _))) =
            (curve_ends(&pair[0].curve), curve_ends(&pair[1].curve))
        else {
            continue;
        };
        let gap = distance(end, start);
        if gap > tolerance {
            return Err(said_at(
                format!("two joined curves of {name} have their ends {gap:.6} mm apart"),
                format!(
                    "{name}'s {} and {} are joined but their ends are {gap:.6} mm apart",
                    pair[0].name, pair[1].name
                ),
                [NoticePlace::point(end), NoticePlace::point(start)],
            ));
        }
    }
    Ok(pieces)
}

impl SpatialPath {
    pub fn add_references(&self, references: &mut References) {
        references.features.insert(self.sketch);
    }

    /// What is told before the evaluation: the sketch is a 3D sketch
    /// before the feature (the document checks that it is in the feature's
    /// component), and the curves are curves of it that can be path pieces,
    /// each listed once. Whether they make one chain is found by the
    /// evaluation (see the module documentation).
    pub fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        let entry = ctx.feature(self.sketch)?;
        let FeatureDef::SpatialSketch(def) = &entry.def else {
            return Err(said(
                format!("{} is not a 3D sketch", entry.name),
                format!(
                    "{} ({}) is not a 3D sketch: a planar sketch's curves are a path as \
                     {{\"sketch\": \"{}\", \"curves\": [...]}}",
                    entry.name, self.sketch, self.sketch
                ),
            ));
        };

        check_selection(def, &self.curves, &entry.name)
    }

    /// The path's curves in order along it, each running along it, where
    /// the sketch has them now; an error when the sketch did not solve or
    /// the curves do not make one chain.
    pub fn resolve<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<Vec<PathCurve>, String> {
        let output = spatial_output(ctx, self.sketch)?;
        let name = ctx.feature_name(self.sketch);
        let Some(FeatureDef::SpatialSketch(def)) = ctx.env.feature(self.sketch).map(|f| &f.def)
        else {
            return Err(format!("{name} is not a 3D sketch"));
        };
        let legs = route(def, self, &name, &|p| output.point(p))?;
        pieces(&output, &legs, &name)
    }
}

/// A chain of a 3D sketch as chain selection finds it: curves joined end
/// to end at their shared or coincident points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpatialChain {
    /// Its curves: an open chain's in order from the free end of the end
    /// curve with the smaller id, a closed one's from its smallest curve
    /// along that curve, a branched one's in id order (each `forward`).
    pub legs: Vec<Leg>,
    /// Closed: every point of it joins two of its curves.
    pub closed: bool,
    /// The points (nodes, named by their smallest point) where three or
    /// more of its curves meet.
    pub branches: Vec<EntityUid>,
}

impl SpatialChain {
    /// The path that takes the whole chain, running from its start: an
    /// open or closed chain's without branches; None for a branched one.
    pub fn path(&self, sketch: FeatureUid) -> Option<SpatialPath> {
        if !self.branches.is_empty() {
            return None;
        }
        let first = self.legs.first()?;
        Some(SpatialPath {
            sketch,
            curves: self.legs.iter().map(|l| l.curve).collect(),
            reversed: !first.forward,
            chain: false,
        })
    }
}

/// The chains of a 3D sketch: its curves that can be path pieces,
/// construction curves left out, grouped by their joints (see the module
/// documentation), in the order of their smallest curve ids. A curve
/// without ends (a circle, an ellipse, a closed spline) is a closed chain
/// of its own.
pub fn chains(def: &SpatialSketchDef) -> Vec<SpatialChain> {
    let mut found: Vec<SpatialChain> = def
        .curves
        .iter()
        .filter(|c| !c.construction && not_a_piece(c).is_none() && c.ends().is_none())
        .map(|c| SpatialChain {
            legs: vec![Leg {
                curve: c.id,
                forward: true,
            }],
            closed: true,
            branches: Vec::new(),
        })
        .collect();
    found.extend(joined_chains(def));
    found.sort_by_key(|chain| chain.legs.iter().map(|l| l.curve).min());
    found
}

/// The chains of the curves with ends (see [`chains`]).
fn joined_chains(def: &SpatialSketchDef) -> Vec<SpatialChain> {
    let joints = Joints::new(def);
    let mut links: Vec<Link> = def
        .curves
        .iter()
        .filter(|c| !c.construction && not_a_piece(c).is_none())
        .filter_map(|c| joints.link(c))
        .collect();
    links.sort_by_key(|l| l.curve);
    let mut taken = vec![false; links.len()];
    let mut chains = Vec::new();
    while let Some(seed) = taken.iter().position(|t| !t) {
        // The links joined to the seed, directly or through others.
        taken[seed] = true;
        let mut group = vec![seed];
        let mut nodes: BTreeSet<EntityUid> = [links[seed].start, links[seed].end].into();
        loop {
            let before = group.len();
            for (i, link) in links.iter().enumerate() {
                if !taken[i] && (nodes.contains(&link.start) || nodes.contains(&link.end)) {
                    taken[i] = true;
                    group.push(i);
                    nodes.extend([link.start, link.end]);
                }
            }
            if group.len() == before {
                break;
            }
        }
        group.sort_unstable();
        let members: Vec<Link> = group.iter().map(|i| links[*i]).collect();
        chains.push(chain_of(&members));
    }
    chains
}

/// One chain from its links, in id order.
fn chain_of(members: &[Link]) -> SpatialChain {
    let mut count: BTreeMap<EntityUid, usize> = BTreeMap::new();
    for link in members {
        *count.entry(link.start).or_default() += 1;
        *count.entry(link.end).or_default() += 1;
    }
    let branches: Vec<EntityUid> = count
        .iter()
        .filter(|(_, n)| **n > 2)
        .map(|(node, _)| *node)
        .collect();
    if !branches.is_empty() {
        return SpatialChain {
            legs: members
                .iter()
                .map(|l| Leg {
                    curve: l.curve,
                    forward: true,
                })
                .collect(),
            closed: false,
            branches,
        };
    }
    let free = |node: &EntityUid| count.get(node) == Some(&1);
    let start = members
        .iter()
        .find_map(|l| {
            if free(&l.start) {
                Some(l.start)
            } else if free(&l.end) {
                Some(l.end)
            } else {
                None
            }
        })
        .unwrap_or(members[0].start);
    let closed = !count.values().any(|n| *n == 1);
    let mut used = vec![false; members.len()];
    let (legs, _) = walk(start, members, &mut used);
    SpatialChain {
        legs,
        closed,
        branches,
    }
}
