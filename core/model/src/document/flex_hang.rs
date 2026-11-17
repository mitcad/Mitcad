// SPDX-License-Identifier: MIT
//! A Flex item let hang (mitcad#1713; `commands.md`, "Hanging cables"):
//! with a direction of gravity in the design, the item's centre line
//! becomes a chain of short segments ([`crate::flex_drape`]) that falls
//! toward gravity, lies limp on what is below, wraps round curved faces and
//! edges and passes through no body, held where it is pinned (by default
//! both ends and every node attached to something, as clips; any node can
//! be pinned or let go) and where the pointer pulls it. The session is the
//! document's runtime state, as an edit's preview is: nothing of the design changes until it is locked, and any change of
//! the design ends it, but a new direction of gravity, toward which it
//! settles again (`hang_follow_gravity`).
//!
//! Locking keeps where the chain rests as the item's nodes: the pinned
//! nodes as they were, and between them the fewest points of the chain
//! through which the item's route is the chain's shape
//! ([`crate::Kernel::flex_drape_nodes`]), on the faces they lie on; one
//! undo step (`Edit Flex1`). No simulation ever runs in a recompute: the
//! item is an ordinary Flex item again, its route worked out from its
//! nodes as any item's.
//!
//! The obstacles are every body of the design at the marker where its
//! occurrences place it, in the item's component's coordinates, but the
//! item's own and those of features after it (as its route's obstacles,
//! hidden ones too). Of them, a step asks the kernel only about those
//! within the item's reach of its pinned points (all of them while none is
//! pinned but a grabbed one): the chain cannot come nearer the others.
//! The kernel's step simulates whole substeps; the session carries what
//! is left of one to the next step.

use super::{Document, ModelError, invalid};
use crate::features::flex::{FlexNode, FlexSupport};
use crate::features::{GeomRef, ValueInput};
use crate::flex::FlexSection;
use crate::flex_drape::{
    DRAPE_DEFAULT_DAMPING, DrapeContact, DrapeNodesSpec, DrapePort, DrapeStepSpec, drape_segment,
};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::path_placement;
use crate::kernel::{BoundingBox, Kernel};
use crate::transform::Transform;

type Vec3 = [f64; 3];

/// The longest a step may simulate, seconds.
pub const HANG_MOST_SECONDS: f64 = 10.0;
/// What a step simulates when the caller does not say, seconds.
pub const HANG_STEP_SECONDS: f64 = 1.0 / 30.0;
/// How much longer than its length the chain may be pulled between its
/// pins before it is said to be pulled tight.
const TAUT: f64 = 0.01;

/// An obstacle of the hanging item: the body, where it was picked when it
/// is of another component (its occurrence path; None for the item's own
/// component) and its shape in the item's component's coordinates, and
/// a box that holds it there (None when the kernel gives none).
struct HangObstacle<S> {
    body: BodyUid,
    path: Option<Vec<OccurrenceUid>>,
    shape: S,
    bounds: Option<BoundingBox>,
}

/// A node of the item while it hangs.
#[derive(Debug, Clone, PartialEq)]
pub struct HangNode {
    pub id: String,
    /// Its point of the chain.
    pub index: usize,
    pub pinned: bool,
    /// What it was attached to before it was let hang.
    pub support: FlexSupport,
    /// It was pinned somewhere else than where it was (dragged while
    /// pinned): it is kept as a free node there.
    pub moved: bool,
}

/// The hanging item (runtime state of the document).
pub(crate) struct HangSession<S> {
    flex: FeatureUid,
    revision: u64,
    obstacles: Vec<HangObstacle<S>>,
    section: FlexSection,
    bend_radius: Option<f64>,
    bend_radius_edge: Option<f64>,
    max_twist: Option<f64>,
    /// The unit direction of gravity in the item's component.
    gravity: Vec3,
    points: Vec<Vec3>,
    velocities: Vec<Vec3>,
    normals: Vec<Vec3>,
    contacts: Vec<Option<DrapeContact>>,
    /// Each point: held where it is.
    pinned: Vec<bool>,
    segment: f64,
    /// The item's length (it stays, unless set).
    length: f64,
    length_set: bool,
    nodes: Vec<HangNode>,
    /// The ports of its first and last node, when they are on one.
    start_port: Option<DrapePort>,
    end_port: Option<DrapePort>,
    grab: Option<usize>,
    still: f64,
    at_rest: bool,
    /// How strongly the air holds it back, 0 to 1.
    damping: f64,
    /// The obstacles the chain can reach (their indices), which a step
    /// gives the kernel (`reach`), and what the last step left of a
    /// substep, seconds.
    active: Vec<usize>,
    carry: f64,
}

/// The hanging item as the commands answer.
#[derive(Debug, Clone, PartialEq)]
pub struct HangState {
    pub flex: FeatureUid,
    /// The chain's points in the item's component, the section's `y` at
    /// each, and whether each lies on something.
    pub points: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub touching: Vec<bool>,
    pub nodes: Vec<HangNode>,
    pub length: f64,
    pub segment: f64,
    /// The point the pointer holds.
    pub grabbed: Option<usize>,
    pub at_rest: bool,
    /// Seconds the last step simulated.
    pub time: f64,
    /// The chain is pulled longer than its length between its pins (they
    /// are farther apart than it reaches).
    pub taut: bool,
    /// How strongly the air holds it back, 0 to 1.
    pub damping: f64,
}

/// What a lock kept.
#[derive(Debug, Clone, PartialEq)]
pub struct HangLocked {
    pub flex: FeatureUid,
    /// The item's nodes now, in order.
    pub nodes: Vec<String>,
    pub length: f64,
    /// The farthest the chain is from the route through the nodes, mm.
    pub deviation: Option<f64>,
}

fn no_session() -> ModelError {
    invalid("no flex item hangs")
}

/// A damping as given: from 0 to 1.
fn checked_damping(damping: f64) -> Result<f64, ModelError> {
    if (0.0..=1.0).contains(&damping) {
        Ok(damping)
    } else {
        Err(invalid(format!(
            "the damping must be from 0 to 1, got {damping}"
        )))
    }
}

/// Points `count` along the polyline, evenly by length, its ends at its
/// ends.
fn resample(line: &[Vec3], count: usize) -> Vec<Vec3> {
    use mitcad_vec3::{add, distance, scale, sub};
    let mut along = vec![0.0];
    for w in line.windows(2) {
        along.push(along.last().copied().unwrap_or(0.0) + distance(w[0], w[1]));
    }
    let total = along.last().copied().unwrap_or(0.0);
    let mut out = Vec::with_capacity(count);
    let mut k = 0;
    for i in 0..count {
        let at = if count > 1 {
            total * i as f64 / (count - 1) as f64
        } else {
            0.0
        };
        while k + 2 < along.len() && along[k + 1] < at {
            k += 1;
        }
        let point = if line.len() < 2 {
            line.first().copied().unwrap_or([0.0; 3])
        } else {
            let span = along[k + 1] - along[k];
            let t = if span > 0.0 {
                ((at - along[k]) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            add(line[k], scale(sub(line[k + 1], line[k]), t))
        };
        out.push(point);
    }
    out
}

impl<S> HangSession<S> {
    fn state(&self, time: f64) -> HangState {
        let pulled: f64 = self
            .points
            .windows(2)
            .map(|w| mitcad_vec3::distance(w[0], w[1]))
            .sum();
        HangState {
            flex: self.flex,
            points: self.points.clone(),
            normals: self.normals.clone(),
            touching: self.contacts.iter().map(Option::is_some).collect(),
            nodes: self.nodes.clone(),
            length: self.length,
            segment: self.segment,
            grabbed: self.grab,
            at_rest: self.at_rest,
            time,
            taut: pulled > self.length * (1.0 + TAUT),
            damping: self.damping,
        }
    }

    /// Pins the nodes `pins` (ids) and lets the others go; an end on a
    /// port that is pinned holds its next point too, so that it leaves
    /// along the port.
    fn set_pins(&mut self, pins: &[String]) -> Result<(), ModelError> {
        for id in pins {
            if !self.nodes.iter().any(|n| n.id == *id) {
                return Err(invalid(crate::notice::said(
                    "the item has no such node",
                    format!("the item has no node {id}"),
                )));
            }
        }
        self.pinned = vec![false; self.points.len()];
        let last = self.points.len() - 1;
        let count = self.nodes.len();
        for (k, node) in self.nodes.iter_mut().enumerate() {
            node.pinned = pins.contains(&node.id);
            if !node.pinned {
                continue;
            }
            self.pinned[node.index] = true;
            let port = node.support == FlexSupport::Port && !node.moved;
            if port && k == 0 && last >= 2 {
                self.pinned[1] = true;
            }
            if port && k + 1 == count && last >= 2 {
                self.pinned[last - 1] = true;
            }
        }
        self.at_rest = false;
        self.still = 0.0;
        self.reach();
        Ok(())
    }

    /// The obstacles the chain can reach (`active`): every point of the
    /// chain is no farther from a pinned point than the chain is long (a
    /// little more for a chain pulled longer than its length, and its
    /// section's resting distance), so only the obstacles whose boxes come
    /// that near every pinned point can be touched; those without a box are
    /// kept. A grabbed point goes where the pointer pulls it: it is no pin
    /// here, and without another pin every obstacle is kept.
    fn reach(&mut self) {
        let pins: Vec<Vec3> = self
            .points
            .iter()
            .zip(&self.pinned)
            .enumerate()
            .filter(|(i, (_, pinned))| **pinned && self.grab != Some(*i))
            .map(|(_, (point, _))| *point)
            .collect();
        if pins.is_empty() {
            self.active = (0..self.obstacles.len()).collect();
            return;
        }
        let pulled: f64 = self
            .points
            .windows(2)
            .map(|w| mitcad_vec3::distance(w[0], w[1]))
            .sum();
        let resting = self.section.resting();
        let reach = self.length.max(pulled) * (1.0 + 2.0 * TAUT) + 2.0 * resting;
        // The box every point stays in: within `reach` of each pin.
        let mut region = BoundingBox {
            min: [f64::NEG_INFINITY; 3],
            max: [f64::INFINITY; 3],
        };
        for pin in &pins {
            for (axis, at) in pin.iter().enumerate() {
                region.min[axis] = region.min[axis].max(at - reach);
                region.max[axis] = region.max[axis].min(at + reach);
            }
        }
        self.active = self
            .obstacles
            .iter()
            .enumerate()
            .filter(|(_, o)| o.bounds.is_none_or(|b| b.overlaps(&region, resting)))
            .map(|(k, _)| k)
            .collect();
    }

    /// Makes the chain `length` long: its points evenly along it as it is
    /// now, more or fewer of them, its pinned points where they are.
    fn set_length(&mut self, length: f64) {
        let count = ((length / drape_segment(length, self.section.resting(), self.bend_radius))
            .round() as usize
            + 1)
        .max(2);
        let old_last = (self.points.len() - 1).max(1) as f64;
        let mut points = resample(&self.points, count);
        let new_last = (count - 1) as f64;
        let held: Vec<(usize, Vec3, bool)> = self
            .nodes
            .iter()
            .map(|n| (n.index, self.points[n.index], n.pinned))
            .collect();
        let pins: Vec<String> = self
            .nodes
            .iter()
            .filter(|n| n.pinned)
            .map(|n| n.id.clone())
            .collect();
        let nodes = self.nodes.len();
        for (k, (node, (index, at, pinned))) in self.nodes.iter_mut().zip(held).enumerate() {
            node.index = if k == 0 {
                0
            } else if k + 1 == nodes {
                count - 1
            } else {
                ((index as f64 * new_last / old_last).round() as usize).min(count - 1)
            };
            if pinned {
                points[node.index] = at;
            }
        }
        self.points = points;
        self.velocities = Vec::new();
        self.normals = vec![[0.0, 0.0, 1.0]; count];
        self.contacts = vec![None; count];
        self.segment = length / new_last;
        self.length = length;
        self.length_set = true;
        self.grab = None;
        // (The ids are the session's own: no error.)
        let _ = self.set_pins(&pins);
    }
}

impl<K: Kernel> Document<K> {
    /// The Flex item that hangs, None when none does.
    pub fn flex_hang_item(&self) -> Option<FeatureUid> {
        let session = self
            .hang_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        session
            .as_ref()
            .filter(|s| s.revision == self.revision)
            .map(|s| s.flex)
    }

    /// The hanging item now, None when none hangs.
    pub fn flex_hang_state(&self) -> Option<HangState> {
        let session = self
            .hang_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        session
            .as_ref()
            .filter(|s| s.revision == self.revision)
            .map(|s| s.state(0.0))
    }

    /// The session, refused when none hangs or the design changed since
    /// (the session then ends).
    fn hang_mut(&mut self) -> Result<&mut HangSession<K::Shape>, ModelError> {
        let revision = self.revision;
        let session = self
            .hang_session
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match session.as_ref() {
            None => return Err(no_session()),
            Some(s) if s.revision != revision => {
                *session = None;
                return Err(invalid(
                    "the design changed since the item was let hang; let it hang again",
                ));
            }
            Some(_) => {}
        }
        session.as_mut().ok_or_else(no_session)
    }

    /// Lets the Flex item `uid` hang: its route becomes a chain of its
    /// length, its nodes `pins` pinned (when None, both ends and every node
    /// attached to something: `default_pins`) and the rest let go, the air
    /// holding it back by `damping` (0 to 1; the default when None).
    /// Nothing of the design changes; an item that hung before is put back.
    pub fn flex_hang(
        &mut self,
        uid: FeatureUid,
        pins: Option<Vec<String>>,
        damping: Option<f64>,
    ) -> Result<HangState, ModelError> {
        let damping = checked_damping(damping.unwrap_or(DRAPE_DEFAULT_DAMPING))?;
        let direction = self
            .state
            .gravity
            .as_ref()
            .map(|g| g.direction)
            .ok_or_else(|| invalid("set the direction of gravity first"))?;
        self.flex_hang_cancel();
        let def = self.flex_def(uid)?;
        let item_name = def.item.name.clone();
        let output = self.flex_output(uid).ok_or_else(|| {
            invalid("the item has not been made; it can hang once it has a route")
        })?;
        if output.samples.len() < 2 || !(output.length > 0.0 && output.length.is_finite()) {
            return Err(invalid("the item has no route to hang from"));
        }
        let entry = self
            .state
            .entry(uid)
            .ok_or_else(|| invalid("there is no such feature"))?;
        let component = entry.component;
        let own_path =
            crate::links::first_path(&self.state.assembly, component, None).map_err(invalid)?;
        let place = path_placement(
            &self.result.placements,
            &self.result.joints.flexed,
            &own_path,
        );
        let into_own = crate::assembly::inverse(&place);
        let gravity = crate::gravity::unit(into_own.apply_vector(direction))
            .ok_or_else(|| invalid("the direction of gravity cannot be taken into the item"))?;

        let section = output.section.clone();
        let bend_radius = output.bend_radius;
        let resting = section.resting();
        if !(resting > 0.0 && resting.is_finite()) {
            return Err(invalid(format!(
                "{item_name} has no thickness to hang with"
            )));
        }
        let length = output.length;
        let count =
            ((length / drape_segment(length, resting, bend_radius)).round() as usize + 1).max(2);
        let segment = length / (count - 1) as f64;
        let mut points = resample(&output.polyline(), count);
        let last = count - 1;
        let nodes: Vec<HangNode> = output
            .nodes
            .iter()
            .enumerate()
            .map(|(k, n)| HangNode {
                id: n.id.clone(),
                index: if k == 0 {
                    0
                } else if k + 1 == output.nodes.len() {
                    last
                } else {
                    ((n.along / segment).round() as usize).min(last)
                },
                pinned: false,
                support: n.support,
                moved: false,
            })
            .collect();
        for (node, out) in nodes.iter().zip(&output.nodes) {
            points[node.index] = out.point;
        }
        let port = |sample: &crate::flex::RouteSample, end: bool| DrapePort {
            direction: if end {
                mitcad_vec3::scale(sample.tangent, -1.0)
            } else {
                sample.tangent
            },
            x_axis: mitcad_vec3::unit(mitcad_vec3::cross(sample.normal, sample.tangent)),
        };
        let first_port = nodes
            .first()
            .is_some_and(|n| n.support == FlexSupport::Port);
        let last_port = nodes.last().is_some_and(|n| n.support == FlexSupport::Port);
        let start_port = output
            .samples
            .first()
            .filter(|_| first_port)
            .map(|s| port(s, false));
        let end_port = output
            .samples
            .last()
            .filter(|_| last_port)
            .map(|s| port(s, true));
        let obstacles = self.hang_obstacles(uid, component, &own_path, &into_own);
        let pins = pins.unwrap_or_else(|| default_pins(&nodes));
        let mut session = HangSession {
            flex: uid,
            revision: self.revision,
            obstacles,
            section,
            bend_radius,
            bend_radius_edge: output.bend_radius_edge,
            max_twist: if output.section.is_round() {
                None
            } else {
                def.item.max_twist
            },
            gravity,
            normals: output_normals(&output.samples, &points),
            points,
            velocities: Vec::new(),
            contacts: vec![None; count],
            pinned: vec![false; count],
            segment,
            length,
            length_set: false,
            nodes,
            start_port,
            end_port,
            grab: None,
            still: 0.0,
            at_rest: false,
            damping,
            active: Vec::new(),
            carry: 0.0,
        };
        session.set_pins(&pins)?;
        let state = session.state(0.0);
        *self
            .hang_session
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);
        Ok(state)
    }

    /// One step of the hanging item: `seconds` (1/30 s when None, at most
    /// 10) simulated; with `grab` (a point of the chain where the pointer
    /// took it, and where the pointer is) the chain's point nearest to the
    /// first grab's point is pulled there until a step without `grab`.
    pub fn flex_hang_step(
        &mut self,
        seconds: Option<f64>,
        grab: Option<(Vec3, Vec3)>,
    ) -> Result<HangState, ModelError> {
        let seconds = seconds.unwrap_or(HANG_STEP_SECONDS);
        if !(seconds > 0.0 && seconds <= HANG_MOST_SECONDS) {
            return Err(invalid(format!(
                "seconds must be greater than zero and at most {HANG_MOST_SECONDS}, got {seconds}"
            )));
        }
        // (The kernel is borrowed beside the session.)
        let revision = self.revision;
        let session = self
            .hang_session
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !matches!(session.as_ref(), Some(s) if s.revision == revision) {
            let changed = session.is_some();
            *session = None;
            return Err(if changed {
                invalid("the design changed since the item was let hang; let it hang again")
            } else {
                no_session()
            });
        }
        let s = session.as_mut().ok_or_else(no_session)?;
        let target = match grab {
            Some((point, target)) => {
                if !point.iter().chain(&target).all(|v| v.is_finite()) {
                    return Err(invalid("a grab's points must be finite numbers"));
                }
                let index = match s.grab {
                    Some(index) => index,
                    None => {
                        let nearest = (0..s.points.len())
                            .min_by(|a, b| {
                                mitcad_vec3::distance(s.points[*a], point)
                                    .total_cmp(&mitcad_vec3::distance(s.points[*b], point))
                            })
                            .unwrap_or(0);
                        s.grab = Some(nearest);
                        s.reach(); // a grabbed pin is no pin while held
                        nearest
                    }
                };
                // A pinned point dragged: pinned where it is left.
                if s.pinned[index]
                    && let Some(node) = s.nodes.iter_mut().find(|n| n.index == index)
                {
                    node.moved = true;
                }
                s.at_rest = false;
                Some((index, target))
            }
            None => {
                if s.grab.take().is_some() {
                    s.at_rest = false;
                    s.still = 0.0;
                    s.reach();
                }
                None
            }
        };
        if s.at_rest && target.is_none() {
            return Ok(s.state(0.0)); // still: nothing moves it
        }
        // Only the obstacles it can reach; their contacts by their indices
        // among all.
        let shapes: Vec<&K::Shape> = s.active.iter().map(|k| &s.obstacles[*k].shape).collect();
        let step = self
            .kernel
            .flex_drape_step(&DrapeStepSpec {
                obstacles: &shapes,
                points: &s.points,
                velocities: &s.velocities,
                pinned: &s.pinned,
                segment: s.segment,
                resting: s.section.resting(),
                bend_radius: s.bend_radius,
                gravity: s.gravity,
                seconds,
                grab: target,
                still: s.still,
                carry: s.carry,
                damping: s.damping,
            })
            .map_err(|e| {
                invalid(crate::notice::said(
                    "the item could not be let hang",
                    e.to_string(),
                ))
            })?;
        if step.points.len() != s.points.len() {
            return Err(invalid("the item could not be let hang"));
        }
        s.points = step.points;
        s.velocities = step.velocities;
        s.normals = step.normals;
        s.contacts = step
            .contacts
            .into_iter()
            .map(|c| {
                c.and_then(|mut c| {
                    c.obstacle = *s.active.get(c.obstacle)?;
                    Some(c)
                })
            })
            .collect();
        s.still = step.still;
        s.at_rest = step.at_rest;
        s.carry = step.carry;
        Ok(s.state(step.time))
    }

    /// Pins the nodes `pins` (ids; the others let go), makes the item
    /// `length` long and the air hold it back by `damping` (0 to 1), each
    /// when given. A new damping alone does not wake a chain at rest.
    pub fn flex_hang_set(
        &mut self,
        pins: Option<Vec<String>>,
        length: Option<f64>,
        damping: Option<f64>,
    ) -> Result<HangState, ModelError> {
        if let Some(length) = length
            && !(length > 0.0 && length.is_finite())
        {
            return Err(invalid(format!(
                "the length must be greater than zero, got {length}"
            )));
        }
        let damping = damping.map(checked_damping).transpose()?;
        let s = self.hang_mut()?;
        if let Some(length) = length {
            s.set_length(length);
        }
        if let Some(pins) = pins {
            s.set_pins(&pins)?;
        }
        if let Some(damping) = damping {
            s.damping = damping;
        }
        Ok(s.state(0.0))
    }

    /// The design's direction of gravity changed while the item hangs (as
    /// `set_gravity` calls it, right after its undo step): the item goes on
    /// hanging and settles again toward the new direction, among the bodies
    /// as they are now (a slack that hangs toward gravity changed with it);
    /// without a direction it is let go.
    pub(crate) fn hang_follow_gravity(&mut self) {
        let Some(mut session) = self
            .hang_session
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        else {
            return;
        };
        let Some(direction) = self.state.gravity.as_ref().map(|g| g.direction) else {
            return; // cleared: let go
        };
        let uid = session.flex;
        let Some(component) = self.state.entry(uid).map(|e| e.component) else {
            return;
        };
        let Ok(own_path) = crate::links::first_path(&self.state.assembly, component, None) else {
            return;
        };
        let place = path_placement(
            &self.result.placements,
            &self.result.joints.flexed,
            &own_path,
        );
        let into_own = crate::assembly::inverse(&place);
        let Some(gravity) = crate::gravity::unit(into_own.apply_vector(direction)) else {
            return;
        };
        session.gravity = gravity;
        session.obstacles = self.hang_obstacles(uid, component, &own_path, &into_own);
        session.revision = self.revision;
        session.at_rest = false;
        session.still = 0.0;
        session.reach();
        *self
            .hang_session
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);
    }

    /// Puts the hanging item back as it was; whether one hung.
    pub fn flex_hang_cancel(&mut self) -> bool {
        self.hang_session
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .is_some()
    }

    /// Keeps where the hanging item is as its nodes (one undo step) and
    /// ends the session. A lock that is refused leaves the item hanging.
    pub fn flex_hang_lock(&mut self) -> Result<HangLocked, ModelError> {
        self.hang_mut()?;
        let session = self
            .hang_session
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .ok_or_else(no_session)?;
        match self.lock_hang(&session) {
            Ok(locked) => Ok(locked),
            Err(error) => {
                // Still hanging, to be moved and locked again.
                *self
                    .hang_session
                    .get_mut()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);
                Err(error)
            }
        }
    }

    fn lock_hang(&mut self, s: &HangSession<K::Shape>) -> Result<HangLocked, ModelError> {
        // The obstacles the steps gave the kernel (whose mesh it keeps), and
        // any other a point lies on; the contacts by their indices there.
        let mut given = s.active.clone();
        for contact in s.contacts.iter().flatten() {
            if contact.obstacle < s.obstacles.len() && !given.contains(&contact.obstacle) {
                given.push(contact.obstacle);
            }
        }
        let shapes: Vec<&K::Shape> = given.iter().map(|k| &s.obstacles[*k].shape).collect();
        let contacts: Vec<Option<DrapeContact>> = s
            .contacts
            .iter()
            .map(|c| {
                c.as_ref().and_then(|c| {
                    let at = given.iter().position(|k| *k == c.obstacle)?;
                    Some(DrapeContact {
                        obstacle: at,
                        ..c.clone()
                    })
                })
            })
            .collect();
        let keep: Vec<usize> = s
            .nodes
            .iter()
            .filter(|n| n.pinned)
            .map(|n| n.index)
            .collect();
        let first_pinned = s.nodes.first().is_some_and(|n| n.pinned && !n.moved);
        let last_pinned = s.nodes.last().is_some_and(|n| n.pinned && !n.moved);
        let resting = s.section.resting();
        let found = self
            .kernel
            .flex_drape_nodes(&DrapeNodesSpec {
                obstacles: &shapes,
                points: &s.points,
                contacts: &contacts,
                keep: &keep,
                start_port: s.start_port.filter(|_| first_pinned),
                end_port: s.end_port.filter(|_| last_pinned),
                resting,
                half_width: s.section.half_width(),
                reach: s.section.reach(),
                bend_radius: s.bend_radius,
                bend_radius_edge: s.bend_radius_edge,
                max_twist: s.max_twist,
                tolerance: hang_tolerance(resting),
            })
            .map_err(|e| {
                invalid(crate::notice::said(
                    "the item's shape could not be kept",
                    e.to_string(),
                ))
            })?;
        let mut indices = found.nodes.clone();
        indices.sort_unstable();
        indices.dedup();
        if indices.first() != Some(&0) {
            indices.insert(0, 0);
        }
        if indices.last() != Some(&(s.points.len() - 1)) {
            indices.push(s.points.len() - 1);
        }
        let component = self
            .state
            .entry(s.flex)
            .map(|e| e.component)
            .unwrap_or(ComponentUid::ROOT);
        let old = self.flex_def(s.flex)?.nodes.clone();
        let mut next = old
            .iter()
            .filter_map(|n| n.id.strip_prefix('n')?.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        let mut nodes: Vec<FlexNode> = Vec::new();
        let mut links = Vec::new();
        for &index in &indices {
            let pinned = s.nodes.iter().find(|n| n.index == index && n.pinned);
            // A pinned node as it was.
            if let Some(node) = pinned
                && !node.moved
                && let Some(kept) = old.iter().find(|o| o.id == node.id)
            {
                nodes.push(kept.clone());
                continue;
            }
            let id = match pinned {
                Some(node) => node.id.clone(),
                None => {
                    next += 1;
                    format!("n{next}")
                }
            };
            let point = s.points[index];
            let mut node = FlexNode::free(point);
            node.id = id;
            // On the face it lies on (of another component through the
            // occurrence it is placed at), at the surface below the centre
            // line.
            let on_face = s.contacts[index].as_ref().and_then(|contact| {
                let face = contact.face.as_ref()?;
                let obstacle = s.obstacles.get(contact.obstacle)?;
                let link = match &obstacle.path {
                    None => None,
                    Some(path) => Some(
                        crate::links::link_for(&self.state.assembly, component, path, None)
                            .ok()??,
                    ),
                };
                let on = GeomRef::Face {
                    body: obstacle.body,
                    face: face.clone(),
                };
                let at = mitcad_vec3::sub(point, mitcad_vec3::scale(contact.normal, resting));
                Some((on, link, at))
            });
            if let Some((on, link, at)) = on_face {
                node.at = at;
                node.on = Some(on.clone());
                links.push((on, link));
            }
            nodes.push(node);
        }
        let length = s.length;
        let length_set = s.length_set;
        let ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let flex = s.flex;
        let chain: f64 = s
            .points
            .windows(2)
            .map(|w| mitcad_vec3::distance(w[0], w[1]))
            .sum();
        self.edit_flex_checked(
            flex,
            |def| {
                def.nodes = nodes;
                for (on, link) in links {
                    def.set_link(&on, link);
                }
                if length_set && def.fixed_length.is_some() {
                    def.fixed_length = Some(ValueInput::Number(length));
                }
                def.check_nodes()
            },
            |recomputed| kept_whole(recomputed.result(flex), chain, resting),
        )?;
        Ok(HangLocked {
            flex: s.flex,
            nodes: ids,
            length,
            deviation: found.deviation,
        })
    }

    /// The bodies a hanging item may not pass through (see the module).
    fn hang_obstacles(
        &self,
        uid: FeatureUid,
        component: ComponentUid,
        own_path: &[OccurrenceUid],
        into_own: &Transform,
    ) -> Vec<HangObstacle<K::Shape>> {
        let assembly = self.state.assembly.at_placements(&self.result.placements);
        let placement = |o: OccurrenceUid| {
            self.result
                .placements
                .get(&o)
                .copied()
                .or_else(|| assembly.occurrence(o).map(|o| o.transform))
                .unwrap_or(Transform::IDENTITY)
        };
        let owners = &self.result.owners;
        let bodies = |c: ComponentUid| -> Vec<BodyUid> {
            owners
                .iter()
                .filter(|(_, owner)| **owner == c)
                .map(|(b, _)| *b)
                .collect()
        };
        let position = self.state.position(uid).unwrap_or(usize::MAX);
        let mut out = Vec::new();
        for instance in assembly.instances(&placement, &bodies) {
            if instance.body.feature == uid
                || self
                    .state
                    .position(instance.body.feature)
                    .is_some_and(|p| p > position)
            {
                continue;
            }
            let Some(shape) = self.body_shape(instance.body) else {
                continue;
            };
            let transform = into_own.after(&instance.transform);
            let moved = if transform.is_identity() {
                shape.clone()
            } else {
                match self.kernel.transform_shape(shape, &transform, None) {
                    Ok(moved) => moved,
                    Err(_) => continue,
                }
            };
            let own = instance.component == component && instance.path == own_path;
            out.push(HangObstacle {
                body: instance.body,
                path: (!own).then(|| instance.path.clone()),
                bounds: self.kernel.enclosing_box(&moved).ok().flatten(),
                shape: moved,
            });
        }
        out
    }
}

/// The nodes pinned when an item is let hang without pins given: both ends,
/// and every node attached to a face, an edge, a point or a port (a clip:
/// one whose attachment is lost too, where it was); only the nodes placed
/// freely in space are let go.
fn default_pins(nodes: &[HangNode]) -> Vec<String> {
    let last = nodes.len().saturating_sub(1);
    nodes
        .iter()
        .enumerate()
        .filter(|(k, n)| *k == 0 || *k == last || n.support != FlexSupport::Free)
        .map(|(_, n)| n.id.clone())
        .collect()
}

/// How far the route through a locked item's nodes may be from where it
/// hung: half its resting distance, at least 0.1 mm.
pub fn hang_tolerance(resting: f64) -> f64 {
    (0.5 * resting).max(0.1)
}

/// Whether a locked item is made in the shape it hung in (mitcad#1713: on
/// Keep Shape most of an item was lost): refused when its evaluation along
/// the new nodes failed, when its route turns back on itself
/// ([`turns_back`]), or when it is not about as long as the chain (half of
/// it at most: cut short; longer than half again and twice the resting
/// distance: turned back or looped), so that the item goes on hanging
/// instead of being kept in pieces.
fn kept_whole<S>(
    result: Option<&crate::recompute::FeatureResult<S>>,
    chain: f64,
    resting: f64,
) -> Result<(), ModelError> {
    const WORDS: &str =
        "the shape the item hangs in could not be kept; move it a little and keep it again";
    let Some(result) = result else {
        return Ok(()); // not evaluated (after the marker): nothing to tell
    };
    if let crate::recompute::FeatureStatus::Failed(error) = &result.status {
        let why = if error.detail.is_empty() {
            &error.message
        } else {
            &error.detail
        };
        return Err(invalid(crate::notice::said(
            WORDS,
            format!("the item could not be made along the nodes it would keep: {why}"),
        )));
    }
    let flex = result
        .output
        .as_deref()
        .and_then(|output| output.flex.as_deref());
    if let Some(flex) = flex {
        let points: Vec<Vec3> = flex.samples.iter().map(|s| s.point).collect();
        if turns_back(&points) {
            return Err(invalid(crate::notice::said(
                WORDS,
                "the route along the nodes it would keep turns back on itself".to_owned(),
            )));
        }
    }
    let route = flex.map(|flex| flex.length);
    if let Some(route) = route
        && (route < 0.5 * chain || route > 1.5 * chain + 2.0 * resting)
    {
        return Err(invalid(crate::notice::said(
            WORDS,
            format!(
                "the route along the nodes it would keep is {route:.3} mm long, the chain {chain:.3} mm"
            ),
        )));
    }
    Ok(())
}

/// Whether a route's points turn back on themselves anywhere: a step that
/// turns by more than a right angle from the one before (mitcad#1713: the
/// route of a locked hanging cable dipped into a bore narrower than itself
/// and out again; no section is swept round that, and its solid was the
/// cable in a few short pieces). Points nearer than 1e-7 mm are one.
fn turns_back(points: &[Vec3]) -> bool {
    let mut before: Option<Vec3> = None;
    for pair in points.windows(2) {
        let step = mitcad_vec3::sub(pair[1], pair[0]);
        if mitcad_vec3::dot(step, step) < 1e-14 {
            continue;
        }
        if let Some(before) = before
            && mitcad_vec3::dot(before, step)
                < -1e-9 * (mitcad_vec3::dot(before, before) * mitcad_vec3::dot(step, step)).sqrt()
        {
            return true;
        }
        before = Some(step);
    }
    false
}

/// The section's `y` at each point of a new chain: the nearest sample's.
fn output_normals(samples: &[crate::flex::RouteSample], points: &[Vec3]) -> Vec<Vec3> {
    points
        .iter()
        .map(|p| {
            samples
                .iter()
                .min_by(|a, b| {
                    mitcad_vec3::distance(a.point, *p)
                        .total_cmp(&mitcad_vec3::distance(b.point, *p))
                })
                .map_or([0.0, 0.0, 1.0], |s| s.normal)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_polyline_is_resampled_evenly_with_its_ends() {
        let line = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]];
        let points = resample(&line, 5);
        assert_eq!(points.len(), 5);
        assert_eq!(points[0], [0.0, 0.0, 0.0]);
        assert_eq!(points[2], [10.0, 0.0, 0.0]);
        assert_eq!(points[4], [10.0, 10.0, 0.0]);
        assert_eq!(points[1], [5.0, 0.0, 0.0]);
    }

    #[test]
    fn a_route_that_dips_and_comes_back_turns_back() {
        // Over a hole and on (a right angle at a corner is no turn back),
        // and the same dipping into the hole and out again (mitcad#1713).
        let over = [
            [0.0, 0.0, 2.5],
            [0.6, 0.0, 2.5],
            [1.2, 0.0, 2.5],
            [1.2, 0.0, 2.5],
            [1.2, 0.0, 1.9],
            [1.2, 0.0, 1.3],
        ];
        assert!(!turns_back(&over));
        let dip = [
            [0.0, 0.0, 2.5],
            [0.6, 0.0, 2.3],
            [0.9, 0.0, 1.8],
            [0.8, 0.0, 1.3],
            [0.5, 0.0, 1.9],
            [0.9, 0.0, 2.5],
        ];
        assert!(turns_back(&dip));
        assert!(!turns_back(&[]));
    }

    #[test]
    fn the_tolerance_is_half_the_resting_distance() {
        assert_eq!(hang_tolerance(5.0), 2.5);
        assert_eq!(hang_tolerance(0.1), 0.1);
    }
}
