// SPDX-License-Identifier: MIT
//! Joint conditions of profiles along sketch lines (mitcad#2200;
//! `commands.md`, "BUILD: profile assemblies", *Joint conditions*): how
//! the members meet at each joint, kept with the joint and edited later.
//!
//! - An **L** (two ends meeting at a point): `mitre` (the shared miter),
//!   `butt` (one member runs on to the outer corner, the other stops square
//!   at its side) or `square` (both as drawn).
//! - A **T** (an end whose axis meets another member's side): `near_side`
//!   (the stem to the other member's near side), `far_side` (through to its
//!   far side) or `square`.
//! - **Three or more ends at one point**: the member of the line picked
//!   first runs on to the outer corner, the others stop at its side; a pair
//!   of them that would still overlap is left square, said in `warnings`.
//!
//! A condition is kept on each member's edge frame (`start_meets`,
//! `end_meets`, [`EdgeMeet`]): the other member's line and both sections
//! across their lines. The edge frame works its end out from the lines at
//! every evaluation, so that an end follows when the member it meets moves
//! with its line, and each member still follows its own line. A member
//! edited with `build_edit` (its alignment, turn, offsets, row or option
//! values) gets its sections kept anew in the edit's undo step
//! ([`ConditionsCrateOps::refresh_joints`], mitcad#2234). A shared miter's
//! angle and tilt follow both lines. A member removed with
//! `build_remove` leaves the ends that met it square
//! ([`ConditionsCrateOps::orphaned_joints`]).
//!
//! The joint refers to the other member's sketch line, not to the other
//! member: so no member binds to one placed later, and the timeline's order
//! of the members does not matter.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize};

use crate::edge::along_of;
use crate::end_meets::{ACROSS, EndMeetsCrateOps, Profile, meet};
use crate::geometry::{V3, Z, cross, dot, inverse, norm, scale, sub, unit};
use crate::lines::Line;
use crate::place::{edit_part, refuse};
use crate::prelude::*;
use crate::{EndFrom, NEAR};
use mitcad_model::features::derived_part::{EndCut, Tilt};
use mitcad_model::features::edge_frame::{
    EdgeFrameDef, EdgeMeet, LineFrame, MeetSide, line_frame, meet_offset,
};
use mitcad_model::features::{FeatureDef, ValueInput};
use mitcad_model::ids::{ComponentUid, FeatureUid};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::{Document, ModelError};

/// A joint's condition (mitcad#2200).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// An L's shared miter.
    Mitre,
    /// An L's butt: one member runs through to the outer corner.
    Butt,
    /// As drawn: the ends at their lines' ends.
    Square,
    /// A T's stem to the other member's near side.
    NearSide,
    /// A T's stem through to the other member's far side.
    FarSide,
}

impl Condition {
    pub fn as_str(self) -> &'static str {
        match self {
            Condition::Mitre => "mitre",
            Condition::Butt => "butt",
            Condition::Square => "square",
            Condition::NearSide => "near_side",
            Condition::FarSide => "far_side",
        }
    }
}

/// What Along sketch lines does at the corners (`lines.corners`,
/// mitcad#2200): the L's mitered, butted or left square; the T's take
/// `near_side` unless `none`. `true` and `false` read as `mitre` and
/// `none`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Corners {
    #[default]
    Mitre,
    Butt,
    None,
}

impl<'de> Deserialize<'de> for Corners {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Flag(bool),
            Text(String),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Flag(true) => Ok(Corners::Mitre),
            Raw::Flag(false) => Ok(Corners::None),
            Raw::Text(text) => match text.as_str() {
                "mitre" | "miter" => Ok(Corners::Mitre),
                "butt" => Ok(Corners::Butt),
                "none" | "square" => Ok(Corners::None),
                other => Err(serde::de::Error::custom(format!(
                    "corners is mitre, butt or none, not {other}"
                ))),
            },
        }
    }
}

/// `build_edit`'s `joint` (mitcad#2200): the condition of the joint an end
/// of the part is in.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointEdit {
    pub end: EndFrom,
    pub condition: Condition,
    /// A butt's member that runs through (a component's uid or name):
    /// left out, the one that runs through now, else the one placed first.
    #[serde(default)]
    pub through: Option<String>,
}

/// An L or a T.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointKind {
    L,
    T,
}

impl JointKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JointKind::L => "L",
            JointKind::T => "T",
        }
    }
}

/// The joint an end of a profile is in, as `build_end_meets` reports it
/// (mitcad#2200).
#[derive(Debug, Clone, PartialEq)]
pub struct JointNow {
    pub kind: JointKind,
    /// The other member: an L's other end, a T's member whose side the end
    /// meets.
    pub other: ComponentUid,
    /// An L's other end (end B: true); none for a T.
    pub other_end_b: Option<bool>,
    pub condition: Condition,
    /// A butt's member that runs through.
    pub through: Option<ComponentUid>,
    /// Why the condition cannot be kept and changed, in words; none when it
    /// can.
    pub problem: Option<String>,
    /// The stored sections are out of date, in words.
    pub warning: Option<String>,
}

/// A joint of the lines Along sketch lines picked, before anything is
/// placed (for the preview and the placing): `members` are indices in the
/// lines' order.
#[derive(Debug, Clone, PartialEq)]
pub struct LineJoint {
    /// Where the lines meet, in the BUILD host's coordinates.
    pub at: V3,
    /// `L`, `T` or `three` (three or more ends at one point).
    pub kind: &'static str,
    pub condition: Condition,
    pub members: Vec<usize>,
    /// The member that runs through, when one does.
    pub through: Option<usize>,
    pub ends: Vec<LineEnd>,
}

/// An end of a line joint: the member's end (end B: true) and where it
/// goes, at which other member; none: square.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineEnd {
    pub member: usize,
    pub end_b: bool,
    pub to: Option<(MeetSide, usize)>,
}

fn near(a: V3, b: V3) -> bool {
    norm(sub(a, b)) <= crate::lines::MEET
}

fn in_line(d: V3, e: V3) -> bool {
    norm(cross(d, e)) < 1e-6
}

/// The joints of the lines Along sketch lines picked (in their order) as
/// `corners` treats them; `flip`: each member's end A is at its line's end.
pub(crate) fn line_joints(lines: &[Line], flip: bool, corners: Corners) -> Vec<LineJoint> {
    if corners == Corners::None {
        return Vec::new();
    }
    // Each member's ends: (member, end_b, point, its line's direction).
    let mut ends = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let d = unit(sub(line.end, line.start)).unwrap_or([0.0, 0.0, 1.0]);
        let (a, b) = if flip {
            (line.end, line.start)
        } else {
            (line.start, line.end)
        };
        ends.push((i, false, a, d));
        ends.push((i, true, b, d));
    }
    let mut joints = Vec::new();
    let mut done = vec![false; ends.len()];
    for (k, &(i, ib, p, _)) in ends.iter().enumerate() {
        // A T: the end on another member's line, inside it.
        if let Some(j) = (0..lines.len()).find(|j| *j != i && lines[*j].holds_inside(p)) {
            joints.push(LineJoint {
                at: p,
                kind: "T",
                condition: Condition::NearSide,
                members: vec![i, j],
                through: Some(j),
                ends: vec![LineEnd {
                    member: i,
                    end_b: ib,
                    to: Some((MeetSide::NearSide, j)),
                }],
            });
            done[k] = true;
            continue;
        }
        if done[k] {
            continue;
        }
        let mut group: Vec<usize> = (0..ends.len())
            .filter(|m| *m == k || (ends[*m].0 != i && !done[*m] && near(ends[*m].2, p)))
            .collect();
        if group.len() < 2 {
            continue;
        }
        for m in &group {
            done[*m] = true;
        }
        // The member of the line picked first first.
        group.sort_by_key(|m| ends[*m].0);
        if let [a, b] = group.as_slice() {
            let ((pi, pb, _, pd), (qi, qb, _, qd)) = (ends[*a], ends[*b]);
            // In line: they continue each other.
            if in_line(pd, qd) {
                continue;
            }
            let (condition, to) = match corners {
                Corners::Butt => (
                    Condition::Butt,
                    [
                        (pi, pb, Some((MeetSide::FarSide, qi))),
                        (qi, qb, Some((MeetSide::NearSide, pi))),
                    ],
                ),
                _ => (
                    Condition::Mitre,
                    [
                        (pi, pb, Some((MeetSide::Corner, qi))),
                        (qi, qb, Some((MeetSide::Corner, pi))),
                    ],
                ),
            };
            joints.push(LineJoint {
                at: p,
                kind: "L",
                condition,
                members: vec![pi, qi],
                through: (condition == Condition::Butt).then_some(pi),
                ends: to
                    .iter()
                    .map(|(member, end_b, to)| LineEnd {
                        member: *member,
                        end_b: *end_b,
                        to: *to,
                    })
                    .collect(),
            });
            continue;
        }
        // Three or more: the first runs through to the outer corner, unless
        // another end continues it in line; the others stop at its side
        // (those in line with it as drawn).
        let (run, run_b, _, run_d) = ends[group[0]];
        let others: Vec<(usize, bool, bool)> = group[1..]
            .iter()
            .map(|m| (ends[*m].0, ends[*m].1, in_line(run_d, ends[*m].3)))
            .collect();
        let continued = others.iter().any(|o| o.2);
        let first_across = others.iter().find(|o| !o.2).map(|o| o.0);
        let mut joint_ends = vec![LineEnd {
            member: run,
            end_b: run_b,
            to: match (continued, first_across) {
                (false, Some(q)) => Some((MeetSide::FarSide, q)),
                _ => None,
            },
        }];
        for (member, end_b, along) in &others {
            joint_ends.push(LineEnd {
                member: *member,
                end_b: *end_b,
                to: (!along).then_some((MeetSide::NearSide, run)),
            });
        }
        let mut members = vec![run];
        members.extend(others.iter().map(|o| o.0));
        joints.push(LineJoint {
            at: p,
            kind: "three",
            condition: Condition::Butt,
            members,
            through: (!continued).then_some(run),
            ends: joint_ends,
        });
    }
    joints
}

/// Where an end goes in its joint and at which member; none: square.
type EndTo = Option<(MeetSide, ComponentUid)>;

/// An end's condition written: where it goes and at which member (none:
/// square, as drawn) and its end cut (none: square).
#[derive(Debug, Clone)]
pub(crate) struct EndWrite {
    pub component: ComponentUid,
    pub end_b: bool,
    pub to: EndTo,
    pub cut: Option<EndCut<ValueInput>>,
}

/// What [`ConditionsCrateOps::write_joints`] writes into a member's derived
/// part: its edge frame, each end's cut when written (none: square) and
/// whether that cut takes the angle the edge frame measures (a shared
/// miter, mitcad#2234).
struct PartWrite {
    frames: FeatureUid,
    cuts: [Option<Option<EndCut<ValueInput>>>; 2],
    linked: [bool; 2],
}

/// A member along a sketch line: its edge frame and its line in the BUILD
/// host's coordinates.
struct MemberLine {
    frames: FeatureUid,
    def: EdgeFrameDef,
    host: LineFrame,
}

/// A profile's box in its own coordinates without its end cuts: its source
/// evaluated with its values. The derived part's body carries its miters,
/// whose cuts must not widen the section a joint keeps. The body's box
/// when the source cannot be evaluated.
fn uncut_bounds<K: Kernel>(doc: &Document<K>, p: &Profile) -> BoundingBox {
    let (Some(source), Some(def)) = (&p.info.source, &p.info.def) else {
        return p.bounds;
    };
    let values: Vec<(String, f64)> = crate::parts::part_values(def, doc.parameters())
        .into_iter()
        .collect();
    crate::parts::source_state(source, def.config.as_deref(), &values)
        .ok()
        .and_then(|state| {
            mitcad_model::internal::features::evaluate_source(doc.kernel(), &state).ok()
        })
        .and_then(|evaluated| crate::place::bounds_of(doc.kernel(), &evaluated.bodies))
        .unwrap_or(p.bounds)
}

/// A section's box across a line in the host: `[x min, x max, y min, y
/// max]` of the profile's uncut corners in the line's frame.
fn section_of<K: Kernel>(doc: &Document<K>, p: &Profile, line: &LineFrame) -> [f64; 4] {
    let mut b = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for c in uncut_bounds(doc, p).corners() {
        let d = sub(p.world.apply_point(c), line.start);
        let (x, y) = (dot(d, line.x), dot(d, line.n));
        b = [b[0].min(x), b[1].max(x), b[2].min(y), b[3].max(y)];
    }
    b.map(|v| (v * 1e9).round() / 1e9)
}

/// An oriented box of a placed profile in the host: its middle, axes and
/// half sizes.
struct Box3 {
    middle: V3,
    axes: [V3; 3],
    half: [f64; 3],
}

fn box_of(p: &Profile) -> Box3 {
    let (min, max) = (p.bounds.min, p.bounds.max);
    let middle = [0, 1, 2].map(|i| (min[i] + max[i]) / 2.0);
    Box3 {
        middle: p.world.apply_point(middle),
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], Z]
            .map(|a| unit(p.world.apply_vector(a)).unwrap_or(a)),
        half: [0, 1, 2].map(|i| (max[i] - min[i]) / 2.0),
    }
}

/// Whether two boxes overlap by more than a touch (the separating axes).
fn overlap(a: &Box3, b: &Box3) -> bool {
    let mut axes: Vec<V3> = a.axes.iter().chain(&b.axes).copied().collect();
    for x in &a.axes {
        for y in &b.axes {
            if let Some(c) = unit(cross(*x, *y)) {
                axes.push(c);
            }
        }
    }
    let between = sub(b.middle, a.middle);
    !axes.iter().any(|l| {
        let reach = |q: &Box3| {
            q.axes
                .iter()
                .zip(q.half)
                .map(|(axis, h)| h * dot(*axis, *l).abs())
                .sum::<f64>()
        };
        dot(between, *l).abs() >= reach(a) + reach(b) - 1e-6
    })
}

/// How an end of `p` meets the side of `q` in a T: the end's axis crosses
/// `q`'s axis between its ends, and the end is within `q`'s section or a
/// gap of [`NEAR`] from it; with how near (the smaller the nearer).
fn side_meet(p: &Profile, pb: bool, q: &Profile) -> Option<f64> {
    let (_, po, u) = p.end(pb);
    let (_, qa, _) = q.end(false);
    let (_, qb, _) = q.end(true);
    let a = unit(q.world.apply_vector(Z))?;
    let c = dot(u, a);
    if c.abs() > 1.0 - 1e-6 {
        return None;
    }
    let n = unit(cross(u, a))?;
    let r = sub(qa, po);
    if dot(r, n).abs() > ACROSS {
        return None;
    }
    let sin2 = 1.0 - c * c;
    let (ru, ra) = (dot(r, u), dot(r, a));
    // The crossing: `s` along u from p's end, `t` along a from q's end A.
    let t = (c * ru - ra) / sin2;
    let s = ru + c * t;
    if t <= 0.0 || t >= norm(sub(qb, qa)) {
        return None;
    }
    let sin = sin2.sqrt();
    let towards = scale(sub(u, scale(a, c)), -1.0 / sin);
    let crossing = crate::geometry::add(po, scale(u, s));
    let local = inverse(&q.world).apply_point(crossing);
    let near_side = q.reach(local, towards);
    let far_side = q.reach(local, scale(towards, -1.0));
    (s >= -far_side / sin - NEAR && s <= near_side / sin + NEAR).then_some(s.abs())
}

/// A shared miter's cut of an end of `p` meeting an end of `q` in an L
/// (mitcad#2234), from their axes alone: the angle from square (half the
/// turn from `p`'s way out into `q`), the same for both ends, and each
/// end's tilt, leaning towards the other member, as [`meet`]'s. Unlike
/// [`meet`] it does not ask that the ends meet as placed: a joint kept
/// with its lines is one even where a sharp miter's long points run past.
/// None for axes in line.
fn mitre_cut(p: &Profile, pb: bool, q: &Profile, qb: bool) -> Option<(f64, [Tilt; 2])> {
    let (_, _, u) = p.end(pb);
    let (_, _, v) = q.end(qb);
    let local = |r: &Profile, w: V3| inverse(&r.world).apply_vector(w);
    let tilts = [
        Tilt::towards(local(p, scale(v, -1.0)))?,
        Tilt::towards(local(q, scale(u, -1.0)))?,
    ];
    let into = scale(v, -1.0);
    let sin = norm(cross(u, into));
    if sin < 1e-6 {
        return None;
    }
    Some((sin.atan2(dot(u, into)) / 2.0, tilts))
}

/// The document's BUILD methods of joint conditions, used inside the crate.
pub(crate) trait ConditionsCrateOps<K: Kernel> {
    /// The joint an end of a profile is in now (an L first, else a T), of
    /// the profiles given.
    fn joint_now(&self, profiles: &[Profile], p: &Profile, pb: bool) -> Option<JointNow>;

    /// Why the member a kept joint refers to can no longer be found.
    fn missing_joint_member(&self, profiles: &[Profile], p: &Profile, end: bool) -> Option<String>;

    /// Writes the ends' conditions in one undo step named `label`.
    fn write_joints(&mut self, writes: &[EndWrite], label: &str) -> Result<(), ModelError>;

    /// The conditions of the joints Along sketch lines placed: each end
    /// kept with its joint, in one undo step; three or more ends whose
    /// stopping members would still overlap left square. Returns the
    /// warnings, in words.
    fn treat_line_joints(
        &mut self,
        joints: &[LineJoint],
        members: &[ComponentUid],
        label: &str,
    ) -> Result<Vec<String>, ModelError>;

    /// `build_edit`'s `joint` (mitcad#2200): the condition of the joint
    /// an end of `component` is in, both members in one undo step.
    fn build_joint(
        &mut self,
        component: ComponentUid,
        edit: &JointEdit,
    ) -> Result<Vec<String>, ModelError>;

    /// The sections the joints keep of a member worked out again after it
    /// changed (its alignment, turn, row or offsets), in one undo step;
    /// nothing when no joint keeps them.
    fn refresh_joints(
        &mut self,
        component: ComponentUid,
        preserve_working_features: bool,
    ) -> Result<(), ModelError>;

    /// The ends of the other members whose joints meet the line of a
    /// member of `removed` that no member left goes along (mitcad#2234):
    /// each written square (no joint, no cut), with what happened to it
    /// in words. Worked out before they go.
    fn orphaned_joints(&self, removed: &[ComponentUid]) -> Vec<(EndWrite, String)>;
}

/// The document's BUILD methods of joint conditions inside this module.
trait MemberLineOps<K: Kernel> {
    /// A member along a sketch line; why not, in words.
    fn member_line(&self, component: ComponentUid) -> Result<MemberLine, String>;

    /// The condition an end keeps now: its edge frame's joint there.
    fn kept(&self, component: ComponentUid, end_b: bool) -> Option<EdgeMeet>;
}

impl<K: Kernel> MemberLineOps<K> for Document<K> {
    fn member_line(&self, component: ComponentUid) -> Result<MemberLine, String> {
        let name = || {
            self.part_info(component)
                .map_or_else(|| component.to_string(), |i| i.name)
        };
        let not_along = || {
            format!(
                "{} is not placed along a sketch line: joint conditions are kept for profiles \
                 along sketch lines; use the shared miter or Length's to a face instead",
                name()
            )
        };
        let along = along_of(self, component).ok_or_else(not_along)?;
        if along.line.is_none() {
            return Err(not_along());
        }
        let Some(FeatureDef::EdgeFrame(def)) = self.feature(along.frames).map(|e| &e.def) else {
            return Err(not_along());
        };
        let local = line_frame(&mut self.resolver(), &def.edge)?;
        let placement = self.path_transform(&def.occurrence.0);
        let host = LineFrame {
            start: placement.apply_point(local.start),
            end: placement.apply_point(local.end),
            x: placement.apply_vector(local.x),
            n: placement.apply_vector(local.n),
            z: placement.apply_vector(local.z),
        };
        Ok(MemberLine {
            frames: along.frames,
            def: def.clone(),
            host,
        })
    }

    fn kept(&self, component: ComponentUid, end_b: bool) -> Option<EdgeMeet> {
        let along = along_of(self, component)?;
        let Some(FeatureDef::EdgeFrame(def)) = self.feature(along.frames).map(|e| &e.def) else {
            return None;
        };
        if end_b {
            def.end_meets.clone()
        } else {
            def.start_meets.clone()
        }
    }
}

impl<K: Kernel> ConditionsCrateOps<K> for Document<K> {
    fn joint_now(&self, profiles: &[Profile], p: &Profile, pb: bool) -> Option<JointNow> {
        let others = || {
            profiles
                .iter()
                .filter(|q| q.info.component != p.info.component)
        };
        // A joint kept at the end (mitcad#2234): the member along the line
        // it meets, an L when that member keeps a joint with this one's
        // line at an end (the one nearer), else a T. So a joint whose lines
        // moved stays the joint it is, also where the ends no longer meet
        // as drawn (a sharp miter's long points).
        let kept = self.kept(p.info.component, pb).and_then(|m| {
            let mine = self.member_line(p.info.component).ok()?;
            let (_, at, _) = p.end(pb);
            others()
                .filter(|q| {
                    self.member_line(q.info.component).is_ok_and(|line| {
                        line.def.edge == m.edge && line.def.occurrence == mine.def.occurrence
                    })
                })
                .map(|q| {
                    let back = [false, true]
                        .into_iter()
                        .filter(|qb| {
                            self.kept(q.info.component, *qb)
                                .is_some_and(|k| k.edge == mine.def.edge)
                        })
                        .min_by(|a, b| {
                            let d = |qb: bool| norm(sub(q.end(qb).1, at));
                            d(*a).total_cmp(&d(*b))
                        });
                    (q, back)
                })
                .min_by(|a, b| {
                    let d = |(q, back): &(&Profile, Option<bool>)| match back {
                        Some(qb) => norm(sub(q.end(*qb).1, at)),
                        None => f64::INFINITY,
                    };
                    d(a).total_cmp(&d(b))
                })
        });
        let (kind, q, qb) = match kept {
            Some((q, Some(qb))) => (JointKind::L, q, Some(qb)),
            Some((q, None)) => (JointKind::T, q, None),
            None => {
                // An L: the nearest end it meets in a corner.
                let l = others()
                    .flat_map(|q| [false, true].map(|qb| (q, qb)))
                    .filter_map(|(q, qb)| meet(p, pb, q, qb).map(|(d, _)| (d, q, qb)))
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                match l {
                    Some((_, q, qb)) => (JointKind::L, q, Some(qb)),
                    None => {
                        let (_, q) = others()
                            .filter_map(|q| side_meet(p, pb, q).map(|d| (d, q)))
                            .min_by(|a, b| a.0.total_cmp(&b.0))?;
                        (JointKind::T, q, None)
                    }
                }
            }
        };
        let (pc, qc) = (p.info.component, q.info.component);
        let problem = match (self.member_line(pc), self.member_line(qc)) {
            (Ok(a), Ok(b)) if a.def.occurrence == b.def.occurrence => None,
            (Ok(_), Ok(_)) => Some(
                "the two profiles go along lines of different components: their joint cannot be kept"
                    .to_owned(),
            ),
            (Err(e), _) | (_, Err(e)) => Some(e),
        };
        let own = self.kept(pc, pb);
        let (condition, through) = match kind {
            JointKind::L => {
                let theirs = qb.and_then(|b| self.kept(qc, b));
                let side = |m: &Option<EdgeMeet>| m.as_ref().map(|m| m.to);
                match (side(&own), side(&theirs)) {
                    (Some(MeetSide::Corner), _) => (Condition::Mitre, None),
                    (Some(MeetSide::FarSide), _) | (_, Some(MeetSide::NearSide)) => {
                        (Condition::Butt, Some(pc))
                    }
                    (Some(MeetSide::NearSide), _) | (_, Some(MeetSide::FarSide)) => {
                        (Condition::Butt, Some(qc))
                    }
                    // Not kept: a shared miter made before (its cut), else
                    // square.
                    _ if p.info.def.as_ref().is_some_and(|d| {
                        if pb {
                            d.ends.b.is_some()
                        } else {
                            d.ends.a.is_some()
                        }
                    }) =>
                    {
                        (Condition::Mitre, None)
                    }
                    _ => (Condition::Square, None),
                }
            }
            JointKind::T => match own.as_ref().map(|m| m.to) {
                Some(MeetSide::NearSide) => (Condition::NearSide, None),
                Some(MeetSide::FarSide) => (Condition::FarSide, None),
                Some(MeetSide::Corner) => (Condition::Mitre, None),
                None => (Condition::Square, None),
            },
        };
        let warning = own.as_ref().and_then(|kept| {
            let (mine, theirs) = (self.member_line(pc).ok()?, self.member_line(qc).ok()?);
            let changed = |stored: [f64; 4], current: [f64; 4]| {
                stored
                    .into_iter()
                    .zip(current)
                    .any(|(a, b)| (a - b).abs() > 1e-6)
            };
            (changed(kept.own, section_of(self, p, &mine.host))
                || changed(kept.section, section_of(self, q, &theirs.host)))
            .then(|| {
                format!(
                    "the joint of {} and {} is stale: a section changed; set the joint again",
                    p.info.name, q.info.name
                )
            })
        });
        Some(JointNow {
            kind,
            other: qc,
            other_end_b: qb,
            condition,
            through,
            problem,
            warning,
        })
    }

    fn missing_joint_member(&self, profiles: &[Profile], p: &Profile, end: bool) -> Option<String> {
        let kept = self.kept(p.info.component, end)?;
        let own = self.member_line(p.info.component).ok()?;
        let found = profiles.iter().any(|q| {
            q.info.component != p.info.component
                && self.member_line(q.info.component).is_ok_and(|line| {
                    line.def.edge == kept.edge && line.def.occurrence == own.def.occurrence
                })
        });
        (!found).then(|| {
            format!(
                "the joint of {} at end {} is stale: no profile remains along the line it meets",
                p.info.name,
                if end { "B" } else { "A" },
            )
        })
    }

    fn write_joints(&mut self, writes: &[EndWrite], label: &str) -> Result<(), ModelError> {
        let profiles = self.end_profiles();
        let find = |c: ComponentUid| {
            profiles
                .iter()
                .find(|p| p.info.component == c)
                .ok_or_else(|| refuse(format!("{c} is not a profile cut to length")))
        };
        let mut frames: BTreeMap<FeatureUid, (EdgeFrameDef, f64)> = BTreeMap::new();
        let mut parts: BTreeMap<FeatureUid, PartWrite> = BTreeMap::new();
        // Each frame's ends written: whether the end is a shared miter
        // whose cut angle the frame measures (mitcad#2234), and that cut's
        // first angle.
        let mut mitres: BTreeMap<FeatureUid, [Option<Option<f64>>; 2]> = BTreeMap::new();
        for w in writes {
            let mine = self.member_line(w.component).map_err(refuse)?;
            let p = find(w.component)?;
            let kept = match w.to {
                Some((to, other)) => {
                    let theirs = self.member_line(other).map_err(refuse)?;
                    if theirs.def.occurrence != mine.def.occurrence {
                        return Err(refuse(
                            "the two profiles go along lines of different components: their joint cannot be kept",
                        ));
                    }
                    Some(EdgeMeet {
                        to,
                        edge: theirs.def.edge.clone(),
                        section: section_of(self, find(other)?, &theirs.host),
                        own: section_of(self, p, &mine.host),
                    })
                }
                None => None,
            };
            let length = norm(sub(mine.host.end, mine.host.start));
            let (def, _) = frames
                .entry(mine.frames)
                .or_insert_with(|| (mine.def.clone(), length));
            if w.end_b {
                def.end_meets = kept;
                def.end = None;
            } else {
                def.start_meets = kept;
                def.start = None;
            }
            let part = p
                .info
                .part
                .ok_or_else(|| refuse(format!("{} has no derived part", p.info.name)))?;
            let end = usize::from(w.end_b);
            // A shared miter's cut follows the lines: its angle is the edge
            // frame's, measured at every recompute.
            let mitre = match (&w.to, &w.cut) {
                (Some((MeetSide::Corner, _)), Some(cut)) => Some(match cut.angle {
                    ValueInput::Number(angle) => angle,
                    _ => 0.0,
                }),
                _ => None,
            };
            mitres.entry(mine.frames).or_default()[end] = Some(mitre);
            let entry = parts.entry(part).or_insert_with(|| PartWrite {
                frames: mine.frames,
                cuts: [None, None],
                linked: [false; 2],
            });
            entry.cuts[end] = Some(w.cut.clone());
            entry.linked[end] = mitre.is_some();
        }
        // Each member keeps a length: its ends worked out as the edge frame
        // will.
        let mut defs = Vec::new();
        let mut drops: Vec<(FeatureUid, [bool; 2])> = Vec::new();
        for (uid, (def, length)) in frames {
            let mut resolver = self.resolver();
            let mut offsets = [0.0; 2];
            for (at_end, m) in def.meets() {
                offsets[usize::from(at_end)] =
                    meet_offset(&mut resolver, &def.edge, def.reversed, at_end, m)
                        .map_err(refuse)?;
            }
            let value = |id: Option<mitcad_model::parameters::ParamId>| {
                id.and_then(|id| self.parameters().value(id)).unwrap_or(0.0)
            };
            let left = length - offsets[0] - value(def.start) - offsets[1] - value(def.end);
            if left <= 1e-6 {
                let name = profiles
                    .iter()
                    .find(|p| {
                        p.info.part.is_some()
                            && along_of(self, p.info.component).is_some_and(|a| a.frames == uid)
                    })
                    .map_or_else(|| "a profile".to_owned(), |p| p.info.name.clone());
                return Err(refuse(format!(
                    "{name} would have no length left at its joints: leave an end square"
                )));
            }
            let entry = self.feature(uid).ok_or(ModelError::NoSuchFeature(uid))?;
            let FeatureDef::EdgeFrame(mut input) =
                crate::parts::as_input(&entry.def, self.parameters())
            else {
                return Err(refuse("the member's edge frame is gone"));
            };
            input.start_meets = def.start_meets.clone();
            input.end_meets = def.end_meets.clone();
            if def.start.is_none() {
                input.start = None;
            }
            if def.end.is_none() {
                input.end = None;
            }
            // A new shared miter's angle: its first value, measured from
            // then on; one kept stays. An end no longer a shared miter lets
            // its angle go once its part no longer takes it (below).
            let written = mitres.get(&uid).copied().unwrap_or_default();
            let mut drop = [false; 2];
            for (end, slot) in [&mut input.start_angle, &mut input.end_angle]
                .into_iter()
                .enumerate()
            {
                match written[end] {
                    Some(Some(angle)) if slot.is_none() => {
                        *slot = Some(ValueInput::Number(angle));
                    }
                    Some(None) if slot.is_some() => drop[end] = true,
                    _ => {}
                }
            }
            if drop.iter().any(|d| *d) {
                drops.push((uid, drop));
            }
            defs.push((uid, FeatureDef::EdgeFrame(input)));
        }
        let label = label.to_owned();
        self.apply(|state| {
            for (uid, def) in &defs {
                state.edit_feature(*uid, def)?;
            }
            for (part, write) in parts {
                // The angles the edge frame measures, by name.
                let names = match state.entry(write.frames).map(|e| &e.def) {
                    Some(FeatureDef::EdgeFrame(f)) => [f.start_angle, f.end_angle]
                        .map(|id| id.map(|id| state.parameters.name(id))),
                    _ => [None, None],
                };
                edit_part(state, part, |def| {
                    for (end, slot) in [&mut def.ends.a, &mut def.ends.b].into_iter().enumerate() {
                        let Some(cut) = write.cuts[end].clone() else {
                            continue;
                        };
                        *slot = cut.map(|mut cut| {
                            if write.linked[end]
                                && let Some(name) = &names[end]
                            {
                                cut.angle = ValueInput::Name(name.clone());
                            }
                            cut
                        });
                    }
                })?;
            }
            for (uid, drop) in &drops {
                let Some(entry) = state.entry(*uid) else {
                    continue;
                };
                let FeatureDef::EdgeFrame(mut input) =
                    crate::parts::as_input(&entry.def, &state.parameters)
                else {
                    continue;
                };
                if drop[0] {
                    input.start_angle = None;
                }
                if drop[1] {
                    input.end_angle = None;
                }
                state.edit_feature(*uid, &FeatureDef::EdgeFrame(input))?;
            }
            Ok((label, ()))
        })
    }

    fn treat_line_joints(
        &mut self,
        joints: &[LineJoint],
        members: &[ComponentUid],
        label: &str,
    ) -> Result<Vec<String>, ModelError> {
        let mut warnings = Vec::new();
        let profiles = self.end_profiles();
        let find = |c: ComponentUid| profiles.iter().find(|p| p.info.component == c);
        let name = |c: ComponentUid| find(c).map_or_else(|| c.to_string(), |p| p.info.name.clone());
        let mut writes = Vec::new();
        for joint in joints {
            let ends: Vec<(ComponentUid, bool, EndTo)> = joint
                .ends
                .iter()
                .filter_map(|e| {
                    Some((
                        *members.get(e.member)?,
                        e.end_b,
                        match e.to {
                            Some((side, other)) => Some((side, *members.get(other)?)),
                            None => None,
                        },
                    ))
                })
                .collect();
            // A shared miter's cuts: as a corner placement's.
            let mut cuts: BTreeMap<(ComponentUid, bool), EndCut<ValueInput>> = BTreeMap::new();
            if joint.condition == Condition::Mitre
                && let [(p, pb, _), (q, qb, _)] = ends.as_slice()
            {
                match (find(*p), find(*q)) {
                    (Some(pp), Some(qq)) => match meet(pp, *pb, qq, *qb) {
                        Some((_, m)) => {
                            for ((c, b), tilt) in [((*p, *pb), m.tilts[0]), ((*q, *qb), m.tilts[1])]
                            {
                                cuts.insert(
                                    (c, b),
                                    EndCut {
                                        angle: ValueInput::Number(m.angle),
                                        tilt: Some(tilt),
                                    },
                                );
                            }
                        }
                        None => {
                            warnings.push(format!(
                                "{} and {} do not meet in a corner: their ends are left square",
                                name(*p),
                                name(*q)
                            ));
                            continue;
                        }
                    },
                    _ => continue,
                }
            }
            // Each end kept, or why not.
            let mut kept = Vec::new();
            for (c, b, to) in &ends {
                let checked = self.member_line(*c).and_then(|_| match to {
                    Some((_, other)) => self.member_line(*other).map(|_| ()),
                    None => Ok(()),
                });
                if let Err(e) = checked {
                    warnings.push(format!("{e}: its corner is left as it is"));
                    continue;
                }
                kept.push(EndWrite {
                    component: *c,
                    end_b: *b,
                    to: *to,
                    cut: cuts.get(&(*c, *b)).cloned(),
                });
            }
            writes.extend(kept);
        }
        if writes.is_empty() {
            return Ok(warnings);
        }
        self.write_joints(&writes, label)?;
        // Three or more ends: two stopping members that still overlap are
        // left square.
        let profiles = self.end_profiles();
        let find = |c: ComponentUid| profiles.iter().find(|p| p.info.component == c);
        let mut square = Vec::new();
        for joint in joints.iter().filter(|j| j.kind == "three") {
            let stopping: Vec<(ComponentUid, bool)> = joint
                .ends
                .iter()
                .filter(|e| matches!(e.to, Some((MeetSide::NearSide, _))))
                .filter_map(|e| Some((*members.get(e.member)?, e.end_b)))
                .collect();
            for (i, (a, ab)) in stopping.iter().enumerate() {
                for (b, bb) in &stopping[i + 1..] {
                    let (Some(pa), Some(pb)) = (find(*a), find(*b)) else {
                        continue;
                    };
                    if overlap(&box_of(pa), &box_of(pb)) {
                        warnings.push(format!(
                            "{} and {} would still overlap at their joint: their ends are left square",
                            pa.info.name, pb.info.name
                        ));
                        for (c, e) in [(*a, *ab), (*b, *bb)] {
                            square.push(EndWrite {
                                component: c,
                                end_b: e,
                                to: None,
                                cut: None,
                            });
                        }
                    }
                }
            }
        }
        if !square.is_empty() {
            self.write_joints(&square, label)?;
        }
        Ok(warnings)
    }

    fn build_joint(
        &mut self,
        component: ComponentUid,
        edit: &JointEdit,
    ) -> Result<Vec<String>, ModelError> {
        let end_b = edit.end == EndFrom::B;
        let profiles = self.end_profiles();
        let find = |c: ComponentUid| profiles.iter().find(|p| p.info.component == c);
        let name = |c: ComponentUid| find(c).map_or_else(|| c.to_string(), |p| p.info.name.clone());
        let p = find(component).ok_or_else(|| {
            refuse(format!(
                "{} is not a profile cut to length: it has no joints",
                name(component)
            ))
        })?;
        let joint = self.joint_now(&profiles, p, end_b).ok_or_else(|| {
            refuse(format!(
                "end {} of {} meets no other profile within {NEAR} mm",
                if end_b { "B" } else { "A" },
                p.info.name
            ))
        })?;
        if let Some(problem) = &joint.problem {
            return Err(refuse(problem.clone()));
        }
        let other = joint.other;
        let q = find(other).ok_or_else(|| refuse("the other profile is gone"))?;
        let writes = match (joint.kind, edit.condition) {
            (JointKind::L, Condition::Mitre) => {
                let qb = joint.other_end_b.unwrap_or(false);
                // From the axes (mitcad#2234): the L joint_now found, also a
                // kept one whose sharp miter runs past the ends as drawn.
                let (angle, tilts) = mitre_cut(p, end_b, q, qb).ok_or_else(|| {
                    refuse(format!(
                        "{} and {} do not meet in a corner",
                        p.info.name, q.info.name
                    ))
                })?;
                let cut = |tilt| EndCut {
                    angle: ValueInput::Number(angle),
                    tilt: Some(tilt),
                };
                vec![
                    EndWrite {
                        component,
                        end_b,
                        to: Some((MeetSide::Corner, other)),
                        cut: Some(cut(tilts[0])),
                    },
                    EndWrite {
                        component: other,
                        end_b: qb,
                        to: Some((MeetSide::Corner, component)),
                        cut: Some(cut(tilts[1])),
                    },
                ]
            }
            (JointKind::L, Condition::Butt) => {
                let qb = joint.other_end_b.unwrap_or(false);
                let through = match &edit.through {
                    Some(text) => {
                        let c = self.component_by_ref(text)?;
                        if c != component && c != other {
                            return Err(refuse(format!(
                                "{text} is not in the joint: a butt runs {} or {} through",
                                p.info.name, q.info.name
                            )));
                        }
                        c
                    }
                    None => match joint.through {
                        Some(c) => c,
                        // The one placed first.
                        None => {
                            let at = |c: ComponentUid| {
                                self.member_line(c)
                                    .ok()
                                    .and_then(|m| self.state().position(m.frames))
                                    .unwrap_or(usize::MAX)
                            };
                            if at(other) < at(component) {
                                other
                            } else {
                                component
                            }
                        }
                    },
                };
                let (stop, stop_b, run, run_b) = if through == component {
                    (other, qb, component, end_b)
                } else {
                    (component, end_b, other, qb)
                };
                vec![
                    EndWrite {
                        component: run,
                        end_b: run_b,
                        to: Some((MeetSide::FarSide, stop)),
                        cut: None,
                    },
                    EndWrite {
                        component: stop,
                        end_b: stop_b,
                        to: Some((MeetSide::NearSide, run)),
                        cut: None,
                    },
                ]
            }
            (JointKind::L, Condition::Square) => vec![
                EndWrite {
                    component,
                    end_b,
                    to: None,
                    cut: None,
                },
                EndWrite {
                    component: other,
                    end_b: joint.other_end_b.unwrap_or(false),
                    to: None,
                    cut: None,
                },
            ],
            (JointKind::L, _) => {
                return Err(refuse(format!(
                    "an L takes Miter, Butt or Square: end {} of {} meets an end of {}",
                    if end_b { "B" } else { "A" },
                    p.info.name,
                    q.info.name
                )));
            }
            (JointKind::T, Condition::NearSide | Condition::FarSide | Condition::Square) => {
                vec![EndWrite {
                    component,
                    end_b,
                    to: match edit.condition {
                        Condition::NearSide => Some((MeetSide::NearSide, other)),
                        Condition::FarSide => Some((MeetSide::FarSide, other)),
                        _ => None,
                    },
                    cut: None,
                }]
            }
            (JointKind::T, _) => {
                return Err(refuse(format!(
                    "a T takes Near Side, Far Side or Square: end {} of {} meets the side of {}",
                    if end_b { "B" } else { "A" },
                    p.info.name,
                    q.info.name
                )));
            }
        };
        let label = format!(
            "Joint of {} and {}: {}",
            p.info.name,
            q.info.name,
            match edit.condition {
                Condition::Mitre => "Miter",
                Condition::Butt => "Butt",
                Condition::Square => "Square",
                Condition::NearSide => "Near Side",
                Condition::FarSide => "Far Side",
            }
        );
        self.write_joints(&writes, &label)?;
        let mut changed = vec!["joint".to_owned(), "along".to_owned()];
        if writes.iter().any(|w| w.cut.is_some()) || joint.condition == Condition::Mitre {
            changed.push("ends".to_owned());
        }
        Ok(changed)
    }

    fn refresh_joints(
        &mut self,
        component: ComponentUid,
        preserve_working_features: bool,
    ) -> Result<(), ModelError> {
        let Ok(mine) = self.member_line(component) else {
            return Ok(());
        };
        // Every edge frame with joints: this member's own sections, the
        // others' sections of this member.
        let kept: Vec<(FeatureUid, EdgeFrameDef)> = self
            .features()
            .filter_map(|e| match &e.def {
                FeatureDef::EdgeFrame(def) if def.meets().next().is_some() => {
                    Some((e.uid, def.clone()))
                }
                _ => None,
            })
            .collect();
        if kept.is_empty() {
            return Ok(());
        }
        let profiles = self.end_profiles();
        let Some(p) = profiles.iter().find(|p| p.info.component == component) else {
            return Ok(());
        };
        let section = section_of(self, p, &mine.host);
        let mut defs = Vec::new();
        for (uid, def) in kept {
            let mut changed = def.clone();
            for slot in [&mut changed.start_meets, &mut changed.end_meets] {
                let Some(m) = slot else {
                    continue;
                };
                if uid == mine.frames {
                    m.own = section;
                }
                if m.edge == mine.def.edge && def.occurrence == mine.def.occurrence {
                    m.section = section;
                }
            }
            if changed != def {
                let entry = self.feature(uid).ok_or(ModelError::NoSuchFeature(uid))?;
                let FeatureDef::EdgeFrame(mut input) =
                    crate::parts::as_input(&entry.def, self.parameters())
                else {
                    continue;
                };
                input.start_meets = changed.start_meets;
                input.end_meets = changed.end_meets;
                defs.push((uid, FeatureDef::EdgeFrame(input)));
            }
        }
        if defs.is_empty() {
            return Ok(());
        }
        let worked: BTreeSet<_> = if preserve_working_features {
            self.features()
                .filter(|entry| self.status(entry.uid).is_some_and(|status| status.is_ok()))
                .map(|entry| entry.uid)
                .collect()
        } else {
            BTreeSet::new()
        };
        self.apply_checked(
            |state| {
                for (uid, def) in &defs {
                    state.edit_feature(*uid, def)?;
                }
                Ok(("Joints kept anew".to_owned(), ()))
            },
            |state, result| {
                for feature in &result.results {
                    if worked.contains(&feature.uid)
                        && let Some(error) = feature.status.failure()
                    {
                        let name = state.entry(feature.uid).map(|entry| entry.name.as_str());
                        let name = name.map_or_else(|| feature.uid.to_string(), str::to_owned);
                        return Err(refuse(format!("{name} would fail: {}", error.agent_text())));
                    }
                }
                Ok(())
            },
        )
    }

    fn orphaned_joints(&self, removed: &[ComponentUid]) -> Vec<(EndWrite, String)> {
        // The lines the members that go are along, with their names.
        let going: Vec<(MemberLine, String)> = removed
            .iter()
            .filter_map(|c| {
                let line = self.member_line(*c).ok()?;
                let name = self.part_info(*c).map_or_else(|| c.to_string(), |i| i.name);
                Some((line, name))
            })
            .collect();
        if going.is_empty() {
            return Vec::new();
        }
        let staying: Vec<(ComponentUid, MemberLine)> = self
            .end_profiles()
            .iter()
            .map(|p| p.info.component)
            .filter(|c| !removed.contains(c))
            .filter_map(|c| Some((c, self.member_line(c).ok()?)))
            .collect();
        let mut out = Vec::new();
        for (c, mine) in &staying {
            for end_b in [false, true] {
                let Some(m) = self.kept(*c, end_b) else {
                    continue;
                };
                let same_line = |line: &MemberLine| {
                    line.def.edge == m.edge && line.def.occurrence == mine.def.occurrence
                };
                // Another member still goes along the line: the joint stays.
                if staying.iter().any(|(_, other)| same_line(other)) {
                    continue;
                }
                let Some((_, gone)) = going.iter().find(|(line, _)| same_line(line)) else {
                    continue;
                };
                let name = self.part_info(*c).map_or_else(|| c.to_string(), |i| i.name);
                out.push((
                    EndWrite {
                        component: *c,
                        end_b,
                        to: None,
                        cut: None,
                    },
                    format!(
                        "{name}'s end {} is left square: {gone}, which it met, is removed.",
                        if end_b { "B" } else { "A" }
                    ),
                ));
            }
        }
        out
    }
}
