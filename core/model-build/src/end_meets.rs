// SPDX-License-Identifier: MIT
//! Ends that meet (mitcad#1940; `commands.md`, "BUILD: profile
//! assemblies", *Ends that meet*): of two profiles placed apart, the end of
//! one that touches, overlaps or comes within [`NEAR`] of the end of the
//! other in a corner (`build_end_meets`), and the shared miter of the two
//! (`build_edit`'s `shared_mitre`) that a corner placement's shared miter
//! makes (mitcad#443): both ends cut on the bisector of the axes, each by
//! half the angle between them and leaning towards the other part, and each
//! part lengthened or shortened at that end so that the outer corners meet,
//! its length the long point.
//!
//! An end moves out by changing the part's length when it is the part's
//! free end; the end it is placed by moves its joint's frame on the other
//! side (`shift`, along that frame's own axes) and changes its length, so
//! that its other end stays; a grounded part moves its occurrence. A member
//! along an edge or a sketch line (mitcad#1814, mitcad#1965) moves its end
//! by its edge frame's Start or End offset, so that it still follows the
//! edge or the line.
//!
//! The corners of members placed along sketch lines (mitcad#1965,
//! `lines.rs`) are treated here too ([`CornersCrateOps`]): a shared miter
//! where two ends meet in an L, a butt where an end meets another member's
//! line (a T), all in one undo step.

use crate::geometry::{
    V3, Z, add, angle_between, cross, dot, inverse, norm, parts, scale, sub, unit,
};
use crate::parts::PartInfo;
use crate::place::{edit_joint, edit_part, refuse};
use crate::prelude::*;
use mitcad_model::features::derived_part::{EndCut, Tilt};
use mitcad_model::features::{FeatureDef, GeomRef, JointDef, ValueInput};
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::parameters::ParamId;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, ModelError};
use std::collections::BTreeMap;

/// How far apart two ends may be and still meet, mm: the gap left between
/// two profiles placed apart.
pub const NEAR: f64 = 5.0;

/// How far the two axes may pass each other across the corner's plane, mm.
const ACROSS: f64 = 0.5;

/// The other end of a corner one end meets.
#[derive(Debug, Clone, PartialEq)]
pub struct EndMeet {
    pub other: ComponentUid,
    pub other_path: Vec<OccurrenceUid>,
    pub other_end_b: bool,
    /// Each end's cut: half the angle between the axes, radians.
    pub angle: f64,
    /// How far this end and the other move out along their axes for the
    /// outer corners to meet, mm (negative: in).
    pub lengthen: [f64; 2],
    /// The shared miter's tilts: this end's, leaning towards the other
    /// part, and the other end's, leaning towards this one.
    pub tilts: [Tilt; 2],
    /// This end's other tilts: away from the other part, and across the
    /// corner one way and the other.
    pub away: Tilt,
    pub across: [Tilt; 2],
    /// Why the shared miter cannot be made, in words.
    pub problem: Option<String>,
}

/// An end of a profile cut to length.
#[derive(Debug, Clone, PartialEq)]
pub struct PartEnd {
    pub component: ComponentUid,
    pub path: Vec<OccurrenceUid>,
    pub end_b: bool,
    /// Where its axis ends, and the axis out of the part there, in the
    /// BUILD host's coordinates.
    pub point: V3,
    pub out: V3,
    /// Where each tilt leans the cut's outward normal (`x`, `-x`, `y`,
    /// `-y`), in the host's coordinates.
    pub leans: [V3; 4],
    /// The end it meets, or why none, in words.
    pub meets: Result<EndMeet, String>,
}

/// A profile as the search sees it.
struct Profile {
    info: PartInfo,
    path: Vec<OccurrenceUid>,
    /// Its placement in the host.
    world: Transform,
    bounds: BoundingBox,
}

impl Profile {
    /// An end's point (in its own coordinates and in the host's) and its
    /// axis out of the part.
    fn end(&self, b: bool) -> (V3, V3, V3) {
        let local = self
            .info
            .mount(if b { "end_b" } else { "end_a" })
            .map(|t| t.translation)
            .unwrap_or(if b {
                [0.0, 0.0, self.bounds.max[2]]
            } else {
                [0.0; 3]
            });
        let axis = unit(self.world.apply_vector(Z)).unwrap_or(Z);
        (
            local,
            self.world.apply_point(local),
            if b { axis } else { scale(axis, -1.0) },
        )
    }

    /// How far its section reaches from its axis (at the local point `at`)
    /// towards `w`, a direction across the axis in the host's coordinates.
    fn reach(&self, at: V3, w: V3) -> f64 {
        let l = inverse(&self.world).apply_vector(w);
        let across = [l[0], l[1], 0.0];
        self.bounds
            .corners()
            .map(|c| dot(sub(c, at), across))
            .fold(0.0_f64, f64::max)
    }

    /// A direction of the host in the part's section (its z left out).
    fn local(&self, v: V3) -> V3 {
        inverse(&self.world).apply_vector(v)
    }
}

fn opposite(t: Tilt) -> Tilt {
    match t {
        Tilt::X => Tilt::MinusX,
        Tilt::MinusX => Tilt::X,
        Tilt::Y => Tilt::MinusY,
        Tilt::MinusY => Tilt::Y,
    }
}

/// How an end of `p` meets an end of `q` in a corner, with how near (the
/// smaller the nearer); None when they do not.
fn meet(p: &Profile, pb: bool, q: &Profile, qb: bool) -> Option<(f64, EndMeet)> {
    let (pl, po, u) = p.end(pb);
    let (ql, qo, v) = q.end(qb);
    let c = dot(u, v);
    if c.abs() > 1.0 - 1e-6 {
        return None;
    }
    let n = unit(cross(u, v))?;
    let r = sub(qo, po);
    if dot(r, n).abs() > ACROSS {
        return None;
    }
    let sin2 = 1.0 - c * c;
    let sin = sin2.sqrt();
    // The corner where the axes cross: `s` along u from p's end, `t` along
    // v from q's end.
    let along = |r: V3| {
        let (ru, rv) = (dot(r, u), dot(r, v));
        let t = (c * ru - rv) / sin2;
        (ru + c * t, t)
    };
    let (s, t) = along(r);
    // Across each part in the corner's plane, away from the other part's
    // body: e_p across p (v's part square to u), e_q across q. The outer
    // sides are p's towards +e_p and q's towards +e_q.
    let e_q = unit(sub(u, scale(v, c)))?;
    let e_p = unit(sub(v, scale(u, c)))?;
    // Each end within the other part's section along its own axis, or a
    // gap of NEAR from it: p's end at s from the corner crosses q's section
    // from -(its reach towards -e_q) to +(towards +e_q), over sin.
    let within = |d: f64, near: f64, far: f64| d >= -far / sin - NEAR && d <= near / sin + NEAR;
    if !within(s, q.reach(ql, scale(e_q, -1.0)), q.reach(ql, e_q))
        || !within(t, p.reach(pl, scale(e_p, -1.0)), p.reach(pl, e_p))
    {
        return None;
    }
    // The outer corner: where the outer sides cross.
    let hp = p.reach(pl, e_p);
    let hq = q.reach(ql, e_q);
    let outer = sub(add(qo, scale(e_q, hq)), add(po, scale(e_p, hp)));
    let (sp, tq) = along(outer);
    let towards = Tilt::towards(p.local(scale(v, -1.0)))?;
    let other = Tilt::towards(q.local(scale(u, -1.0)))?;
    let across = [
        Tilt::towards(p.local(n)).unwrap_or(Tilt::Y),
        Tilt::towards(p.local(scale(n, -1.0))).unwrap_or(Tilt::MinusY),
    ];
    Some((
        s.abs() + t.abs(),
        EndMeet {
            other: q.info.component,
            other_path: q.path.clone(),
            other_end_b: qb,
            angle: angle_between(u, scale(v, -1.0)) / 2.0,
            lengthen: [sp, tq],
            tilts: [towards, other],
            away: opposite(towards),
            across,
            problem: None,
        },
    ))
}

/// How a part's end moves out by a length.
#[derive(Debug, Clone)]
enum Move {
    /// Its length changes: the free end.
    Length(f64),
    /// Its joint's frame on the other side moves by a shift along its own
    /// axes, and its length changes: the end it is placed by.
    Joint {
        joint: FeatureUid,
        shift: V3,
        length: f64,
    },
    /// Its occurrence moves (in its parent's coordinates) and its length
    /// changes: a grounded part's end A.
    Grounded {
        occurrence: OccurrenceUid,
        by: V3,
        length: f64,
    },
    /// Its edge frame's offset at that end changes (`start` for end A,
    /// `end` for end B) and its bound length follows: a member along an
    /// edge or a sketch line (mitcad#1965).
    Along {
        frames: FeatureUid,
        end_b: bool,
        offset: f64,
    },
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait EndMeetsBuild<K: Kernel> {
    /// The ends of the profiles cut to length, with the ends they meet
    /// (`meets`): of `only`'s ends when given, else of every end; the
    /// others only where they are.
    fn build_end_meets(&self, only: Option<ComponentUid>, meets: bool) -> Vec<PartEnd>;

    /// The shared miter at a part's end with the end it meets
    /// (`build_edit`'s `shared_mitre`); returns what changed.
    fn build_shared_mitre(
        &mut self,
        component: ComponentUid,
        end_b: bool,
    ) -> Result<Vec<String>, ModelError>;
}

/// The document's BUILD methods of this block used inside the crate.
trait EndMeetsCrateOps<K: Kernel> {
    fn end_profiles(&self) -> Vec<Profile>;

    /// A joint's frame on its other side (b) moved by `moved` (in the
    /// host): its override's new shift, along that frame's own axes.
    fn shifted(&self, j: &JointDef<ParamId>, moved: V3) -> Result<V3, String>;

    /// How a part's end moves out by `by` mm; None when it need not move.
    fn plan_move(&self, p: &Profile, end_b: bool, by: f64, out: V3)
    -> Result<Option<Move>, String>;

    /// Edge frames with new offsets at their start and their end (mm),
    /// ready to be edited in.
    fn frame_defs(
        &self,
        offsets: &BTreeMap<FeatureUid, [Option<f64>; 2]>,
    ) -> Result<Vec<(FeatureUid, FeatureDef<ValueInput>)>, ModelError>;
}

impl<K: Kernel> EndMeetsCrateOps<K> for Document<K> {
    fn end_profiles(&self) -> Vec<Profile> {
        self.build_parts_info()
            .into_iter()
            .filter(|i| i.is_cut_to_length())
            .filter(|i| i.meta.role != crate::BuildRole::Panel)
            .filter_map(|info| {
                let path = self.placed_in_host(info.component)?;
                let world = self.path_transform(&path);
                let bounds = self.part_bounds(info.component)?;
                Some(Profile {
                    info,
                    path,
                    world,
                    bounds,
                })
            })
            .collect()
    }

    fn shifted(&self, j: &JointDef<ParamId>, moved: V3) -> Result<V3, String> {
        let local =
            mitcad_model::internal::joints::origin_frame(&mut self.resolver(), &j.b.geometry)?;
        let turned = match &j.b.frame_override {
            Some(o) => mitcad_model::internal::joints::apply_override(&local, o)?,
            None => local,
        };
        let frame = self.path_transform(&j.b.occurrence.0).after(&turned);
        let [_, x, y, z] = parts(&frame);
        let old = j.b.frame_override.and_then(|o| o.shift).unwrap_or([0.0; 3]);
        Ok([
            old[0] + dot(moved, x),
            old[1] + dot(moved, y),
            old[2] + dot(moved, z),
        ])
    }

    fn plan_move(
        &self,
        p: &Profile,
        end_b: bool,
        by: f64,
        out: V3,
    ) -> Result<Option<Move>, String> {
        let info = &p.info;
        if crate::tilt::is_tilted(info.record.tilt) {
            return Err(format!(
                "{} is tilted on its side: its ends are cut by its tilt",
                info.name
            ));
        }
        if by.abs() <= 1e-6 {
            return Ok(None);
        }
        let Some((_, length)) = info.length else {
            return Err(format!(
                "{} is not cut to length: it has no miters",
                info.name
            ));
        };
        // Along an edge or a sketch line (mitcad#1965): its edge frame's
        // offset at the end, so that it still follows.
        if let Some(along) = crate::edge::along_of(self, info.component) {
            let Some(FeatureDef::EdgeFrame(frames)) = self.feature(along.frames).map(|e| &e.def)
            else {
                return Err(format!("{} has no edge frame", info.name));
            };
            let slot = if end_b { frames.end } else { frames.start };
            let old = slot
                .and_then(|id| self.parameters().value(id))
                .unwrap_or(0.0);
            let new = length + by;
            if let Some(problem) = info.meta.length.as_ref().and_then(|l| l.outside(new)) {
                return Err(problem);
            }
            return Ok(Some(Move::Along {
                frames: along.frames,
                end_b,
                offset: ((old - by) * 1e6).round() / 1e6,
            }));
        }
        if info
            .record
            .bound
            .iter()
            .any(|b| self.state().position(*b).is_some())
        {
            return Err(format!(
                "{} has its length bound to another part: give it a length of its own to miter it at the corner",
                info.name
            ));
        }
        if info.record.points.is_some() {
            return Err(format!(
                "{} is placed by two points: move its points to the corner instead",
                info.name
            ));
        }
        let new = length + by;
        if let Some(problem) = info.meta.length.as_ref().and_then(|l| l.outside(new)) {
            return Err(problem);
        }
        let new = (new * 1e6).round() / 1e6;
        let moved = scale(out, by);
        match info.record.joint {
            Some(joint) => {
                let Some(FeatureDef::Joint(j)) = self.feature(joint).map(|e| &e.def) else {
                    return Err(format!("{} has no joint", info.name));
                };
                let placed_b = match &j.a.geometry {
                    GeomRef::Mount { name, .. } if name == "end_a" => false,
                    GeomRef::Mount { name, .. } if name == "end_b" => true,
                    _ => {
                        return Err(format!(
                            "{} is placed by a frame of its own: its end cannot move to the corner",
                            info.name
                        ));
                    }
                };
                if placed_b != end_b {
                    return Ok(Some(Move::Length(new)));
                }
                // The joint's frame on the other side moved by `moved`,
                // along its own axes.
                let shift = self.shifted(j, moved)?;
                Ok(Some(Move::Joint {
                    joint,
                    shift,
                    length: new,
                }))
            }
            None => {
                // Grounded: placed by its own coordinates, end A at z = 0.
                if end_b {
                    return Ok(Some(Move::Length(new)));
                }
                let Some((occurrence, parent)) = p.path.split_last() else {
                    return Err(format!("{} has no joint", info.name));
                };
                let by = inverse(&self.path_transform(parent)).apply_vector(moved);
                Ok(Some(Move::Grounded {
                    occurrence: *occurrence,
                    by,
                    length: new,
                }))
            }
        }
    }

    fn frame_defs(
        &self,
        offsets: &BTreeMap<FeatureUid, [Option<f64>; 2]>,
    ) -> Result<Vec<(FeatureUid, FeatureDef<ValueInput>)>, ModelError> {
        let mut defs = Vec::new();
        for (uid, [start, end]) in offsets {
            let entry = self.feature(*uid).ok_or(ModelError::NoSuchFeature(*uid))?;
            let FeatureDef::EdgeFrame(mut def) =
                crate::parts::as_input(&entry.def, self.parameters())
            else {
                return Err(refuse("the member's edge frame is gone"));
            };
            if let Some(v) = start {
                def.start = (v.abs() > 1e-9).then_some(ValueInput::Number(*v));
            }
            if let Some(v) = end {
                def.end = (v.abs() > 1e-9).then_some(ValueInput::Number(*v));
            }
            defs.push((*uid, FeatureDef::EdgeFrame(def)));
        }
        Ok(defs)
    }
}

impl<K: Kernel> EndMeetsBuild<K> for Document<K> {
    fn build_end_meets(&self, only: Option<ComponentUid>, with_meets: bool) -> Vec<PartEnd> {
        let profiles = self.end_profiles();
        let mut out = Vec::new();
        for p in &profiles {
            for pb in [false, true] {
                let (_, point, axis) = p.end(pb);
                let leans = [Tilt::X, Tilt::MinusX, Tilt::Y, Tilt::MinusY]
                    .map(|t| p.world.apply_vector(t.lean()));
                let meets = if !with_meets || only.is_some_and(|c| c != p.info.component) {
                    Err(String::new())
                } else {
                    let found = profiles
                        .iter()
                        .filter(|q| q.info.component != p.info.component)
                        .flat_map(|q| [false, true].map(|qb| (q, qb)))
                        .filter_map(|(q, qb)| meet(p, pb, q, qb).map(|m| (q, m)))
                        .min_by(|a, b| a.1.0.total_cmp(&b.1.0));
                    match found {
                        Some((q, (_, mut m))) => {
                            let (_, _, v) = q.end(m.other_end_b);
                            m.problem = self
                                .plan_move(p, pb, m.lengthen[0], axis)
                                .and_then(|_| self.plan_move(q, m.other_end_b, m.lengthen[1], v))
                                .err();
                            Ok(m)
                        }
                        None => Err(why_none(&profiles, p, pb)),
                    }
                };
                out.push(PartEnd {
                    component: p.info.component,
                    path: p.path.clone(),
                    end_b: pb,
                    point,
                    out: axis,
                    leans,
                    meets,
                });
            }
        }
        out
    }

    fn build_shared_mitre(
        &mut self,
        component: ComponentUid,
        end_b: bool,
    ) -> Result<Vec<String>, ModelError> {
        let name = self
            .part_info(component)
            .map_or_else(|| component.to_string(), |i| i.name);
        let not_cut = || refuse(format!("{name} is not cut to length: it has no miters"));
        let ends = self.build_end_meets(Some(component), true);
        let end = ends
            .iter()
            .find(|e| e.component == component && e.end_b == end_b)
            .ok_or_else(not_cut)?;
        let meet = end.meets.clone().map_err(refuse)?;
        if let Some(problem) = &meet.problem {
            return Err(refuse(problem.clone()));
        }
        let profiles = self.end_profiles();
        let find = |c: ComponentUid| {
            profiles
                .iter()
                .find(|p| p.info.component == c)
                .ok_or_else(not_cut)
        };
        let (p, q) = (find(component)?, find(meet.other)?);
        let (_, _, v) = q.end(meet.other_end_b);
        let moves = [
            self.plan_move(p, end_b, meet.lengthen[0], end.out)
                .map_err(refuse)?,
            self.plan_move(q, meet.other_end_b, meet.lengthen[1], v)
                .map_err(refuse)?,
        ];
        // What is placed on a moved end's frame (a part on it, as at a
        // corner placement) would follow it: its joint's frame is moved back
        // by as much, so that it stays (on top of its own move, for the
        // other part of the corner placed there).
        let mut shifts: BTreeMap<FeatureUid, V3> = BTreeMap::new();
        for planned in moves.iter().flatten() {
            if let Move::Joint { joint, shift, .. } = planned {
                shifts.insert(*joint, *shift);
            }
        }
        for (prof, b, by, out) in [
            (p, end_b, meet.lengthen[0], end.out),
            (q, meet.other_end_b, meet.lengthen[1], v),
        ] {
            let Some(part) = prof.info.part else {
                continue;
            };
            if by.abs() <= 1e-6 {
                continue;
            }
            let frame = if b { "end_b" } else { "end_a" };
            let on: Vec<(FeatureUid, JointDef<ParamId>)> = self
                .features()
                .filter_map(|e| match &e.def {
                    FeatureDef::Joint(j)
                        if matches!(&j.b.geometry, GeomRef::Mount { feature, name }
                            if *feature == part && name == frame) =>
                    {
                        Some((e.uid, j.clone()))
                    }
                    _ => None,
                })
                .collect();
            for (uid, mut j) in on {
                // From the shift planned already, when it moves too.
                if let Some(planned) = shifts.get(&uid) {
                    let mut o = j.b.frame_override.unwrap_or_default();
                    o.shift = Some(*planned);
                    j.b.frame_override = Some(o);
                }
                let back = self.shifted(&j, scale(out, -by)).map_err(refuse)?;
                shifts.insert(uid, back);
            }
        }
        let parts_of = [
            (
                p.info
                    .part
                    .ok_or_else(|| refuse(format!("{} has no derived part", p.info.name)))?,
                p.info.meta.length.as_ref().map(|l| l.parameter.clone()),
                end_b,
            ),
            (
                q.info
                    .part
                    .ok_or_else(|| refuse(format!("{} has no derived part", q.info.name)))?,
                q.info.meta.length.as_ref().map(|l| l.parameter.clone()),
                meet.other_end_b,
            ),
        ];
        let label = format!("Shared miter of {} and {}", p.info.name, q.info.name);
        let angle = meet.angle;
        let tilts = meet.tilts;
        // The edge frames' offsets of members along an edge or a line.
        let mut offsets: BTreeMap<FeatureUid, [Option<f64>; 2]> = BTreeMap::new();
        for planned in moves.iter().flatten() {
            if let Move::Along {
                frames,
                end_b,
                offset,
            } = planned
            {
                offsets.entry(*frames).or_default()[usize::from(*end_b)] = Some(*offset);
            }
        }
        let frame_defs = self.frame_defs(&offsets)?;
        self.apply(|state| {
            let mut changed = vec!["ends".to_owned()];
            for (i, ((part, length_name, b), planned)) in parts_of.iter().zip(&moves).enumerate() {
                let part = *part;
                let cut = EndCut {
                    angle: ValueInput::Number(angle),
                    tilt: Some(tilts[i]),
                };
                let length = match planned {
                    Some(Move::Length(l)) => Some(*l),
                    Some(Move::Joint { length, .. }) => {
                        changed.push("moved".to_owned());
                        Some(*length)
                    }
                    Some(Move::Grounded {
                        occurrence,
                        by,
                        length,
                    }) => {
                        if let Some(o) = state.assembly.occurrence_mut(*occurrence) {
                            o.transform = Transform::translation(*by).after(&o.transform);
                        }
                        changed.push("moved".to_owned());
                        Some(*length)
                    }
                    // Its edge frame's offset, below; its length follows.
                    Some(Move::Along { .. }) => {
                        changed.push("along".to_owned());
                        None
                    }
                    None => None,
                };
                let name = length_name.clone();
                edit_part(state, part, |def| {
                    if *b {
                        def.ends.b = Some(cut);
                    } else {
                        def.ends.a = Some(cut);
                    }
                    if let (Some(l), Some(name)) = (length, name) {
                        def.parameters.insert(name, ValueInput::Number(l));
                    }
                })?;
                if length.is_some() {
                    changed.push("length".to_owned());
                }
            }
            for (uid, def) in &frame_defs {
                state.edit_feature(*uid, def)?;
            }
            // The joints' frames moved: the parts' own, and those placed
            // on a moved end, back.
            for (joint, shift) in &shifts {
                let shift = *shift;
                edit_joint(state, *joint, |j| {
                    let mut o = j.b.frame_override.unwrap_or_default();
                    o.shift = Some(shift);
                    j.b.frame_override = Some(o);
                })?;
            }
            changed.sort();
            changed.dedup();
            Ok((label, changed))
        })
    }
}

/// Why no end meets a part's end, in words.
fn why_none(profiles: &[Profile], p: &Profile, pb: bool) -> String {
    let (_, po, u) = p.end(pb);
    // Another end on its axis, facing it: the parts continue each other.
    let inline = profiles
        .iter()
        .filter(|q| q.info.component != p.info.component)
        .flat_map(|q| [false, true].map(|qb| q.end(qb)))
        .any(|(_, qo, v)| {
            dot(u, v) < -1.0 + 1e-6 && {
                let r = sub(qo, po);
                norm(sub(r, scale(u, dot(r, u)))) <= ACROSS && dot(r, u).abs() <= NEAR
            }
        });
    if inline {
        "the parts are on one axis: a shared miter cuts nothing".to_owned()
    } else {
        format!(
            "no other component's end comes within {} mm of this end",
            NEAR
        )
    }
}

// The corners of members along sketch lines (mitcad#1965).

/// A corner of members placed along sketch lines to treat.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Corner {
    /// Two ends that meet in an L: a shared miter.
    Mitre {
        p: ComponentUid,
        pb: bool,
        q: ComponentUid,
        qb: bool,
    },
    /// An end on another member's line (a T): butted against its side.
    /// `at` is where the line ends, in the BUILD host's coordinates.
    Butt {
        stem: ComponentUid,
        end_b: bool,
        through: ComponentUid,
        at: V3,
    },
}

/// The document's BUILD methods of the corners, used inside the crate.
pub(crate) trait CornersCrateOps<K: Kernel> {
    /// Treats the corners in one undo step named `label`: each L's ends
    /// cut by the shared miter and moved out so that their outer corners
    /// meet, each T's end moved back to the side of the member it meets.
    /// Returns, in words, the corners left as they are.
    fn treat_corners(&mut self, corners: &[Corner], label: &str)
    -> Result<Vec<String>, ModelError>;
}

impl<K: Kernel> CornersCrateOps<K> for Document<K> {
    fn treat_corners(
        &mut self,
        corners: &[Corner],
        label: &str,
    ) -> Result<Vec<String>, ModelError> {
        let profiles = self.end_profiles();
        let find = |c: ComponentUid| profiles.iter().find(|p| p.info.component == c);
        let mut left = Vec::new();
        let mut offsets: BTreeMap<FeatureUid, [Option<f64>; 2]> = BTreeMap::new();
        let mut cuts: BTreeMap<FeatureUid, [Option<EndCut<ValueInput>>; 2]> = BTreeMap::new();
        let mut take = |planned: Option<Move>, left: &mut Vec<String>, name: &str| match planned {
            Some(Move::Along {
                frames,
                end_b,
                offset,
            }) => {
                offsets.entry(frames).or_default()[usize::from(end_b)] = Some(offset);
                true
            }
            None => true,
            Some(_) => {
                left.push(format!(
                    "{name} does not follow a line: its corner is left as it is"
                ));
                false
            }
        };
        for corner in corners {
            match corner {
                Corner::Mitre { p, pb, q, qb } => {
                    let (Some(pp), Some(qq)) = (find(*p), find(*q)) else {
                        continue;
                    };
                    let Some((_, m)) = meet(pp, *pb, qq, *qb) else {
                        left.push(format!(
                            "{} and {} do not meet in a corner: their ends are left square",
                            pp.info.name, qq.info.name
                        ));
                        continue;
                    };
                    let (_, _, u) = pp.end(*pb);
                    let (_, _, v) = qq.end(*qb);
                    let moves = match (
                        self.plan_move(pp, *pb, m.lengthen[0], u),
                        self.plan_move(qq, *qb, m.lengthen[1], v),
                    ) {
                        (Ok(a), Ok(b)) => [a, b],
                        (Err(e), _) | (_, Err(e)) => {
                            left.push(e);
                            continue;
                        }
                    };
                    let [a, b] = moves;
                    if !take(a, &mut left, &pp.info.name) || !take(b, &mut left, &qq.info.name) {
                        continue;
                    }
                    for (prof, b, tilt) in [(pp, *pb, m.tilts[0]), (qq, *qb, m.tilts[1])] {
                        if let Some(part) = prof.info.part {
                            cuts.entry(part).or_default()[usize::from(b)] = Some(EndCut {
                                angle: ValueInput::Number(m.angle),
                                tilt: Some(tilt),
                            });
                        }
                    }
                }
                Corner::Butt {
                    stem,
                    end_b,
                    through,
                    at,
                } => {
                    let (Some(pp), Some(qq)) = (find(*stem), find(*through)) else {
                        continue;
                    };
                    let Some(back) = butt_back(pp, *end_b, qq, *at) else {
                        left.push(format!(
                            "{} runs along {}: its end is left as it is",
                            pp.info.name, qq.info.name
                        ));
                        continue;
                    };
                    let (_, _, u) = pp.end(*end_b);
                    match self.plan_move(pp, *end_b, -back, u) {
                        Ok(planned) => {
                            take(planned, &mut left, &pp.info.name);
                        }
                        Err(e) => left.push(e),
                    }
                }
            }
        }
        let frame_defs = self.frame_defs(&offsets)?;
        if frame_defs.is_empty() && cuts.is_empty() {
            return Ok(left);
        }
        let label = label.to_owned();
        self.apply(|state| {
            for (uid, def) in &frame_defs {
                state.edit_feature(*uid, def)?;
            }
            for (part, [a, b]) in cuts {
                edit_part(state, part, |def| {
                    if let Some(a) = a {
                        def.ends.a = Some(a);
                    }
                    if let Some(b) = b {
                        def.ends.b = Some(b);
                    }
                })?;
            }
            Ok((label, ()))
        })?;
        Ok(left)
    }
}

/// How far an end of `p` (`end_b`), ending at `at` on the line of `q`,
/// moves back for its end to meet `q`'s side (a T): the reach of `q`'s
/// section from `at` towards `p`, over the sine of the angle between them.
/// None when `p` runs along `q`.
fn butt_back(p: &Profile, end_b: bool, q: &Profile, at: V3) -> Option<f64> {
    let (_, _, u) = p.end(end_b);
    let a = unit(q.world.apply_vector(Z))?;
    let square = sub(u, scale(a, dot(u, a)));
    let sin = norm(square);
    if sin < 1e-6 {
        return None;
    }
    // Towards p's body, square to q's axis.
    let towards = scale(square, -1.0 / sin);
    let local = inverse(&q.world).apply_point(at);
    Some(q.reach(local, towards) / sin)
}
