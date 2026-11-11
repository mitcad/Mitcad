// SPDX-License-Identifier: MIT
//! Bodies handed out as soon as no later feature changes them (mitcad#881).
//!
//! Opening a design recomputes its timeline on one thread and then meshes
//! the shown bodies for display; on a design whose bodies are final long
//! before the timeline's end the meshing could run beside the recompute.
//! A recompute given a feed ([`FinalBodies`], `Document::feed_final_bodies`)
//! puts each shown body into it once no feature after the one it has come
//! to can change the body: the application meshes a copy of it on a thread
//! of its own meanwhile and gives the body the copy's triangulations once
//! the recompute is over.
//!
//! **Which features change a body.** Before the recompute goes through the
//! timeline, every feature before the marker that is not suppressed is
//! classified by its definition alone ([`writes`]): it changes no body
//! (sketches, construction geometry, joints and the other features that
//! only place occurrences or write parameters, placeholders), the bodies it
//! names (participants of an extrusion or a revolution that joins, cuts or
//! intersects, the body of a fillet, chamfer, Offset Face or thread, the
//! target and tools of a combine, the bodies of a move, a body transfer or
//! Component from Bodies, the bodies of a pattern or mirror, or of the
//! features a pattern repeats), any body of its component (an operation
//! on all bodies, and every other kind of feature), or any body at all (a
//! cut across components, a pattern of occurrences, a feature that made a
//! component). The bodies a feature makes are new and change nothing
//! before them. A body is final after the feature at position `p` when
//! `p` is at or after the last feature that may change any body, the last
//! that may change any body of the body's component, and the last that
//! names it. A body set by a feature is put into the feed then when it is
//! final already, else once the recompute has gone through that position
//! (its component looked up again then: a body moved into another
//! component waits for that one's features).
//!
//! **What is put in.** Only bodies the design shows (a component with a
//! shown occurrence, a body not hidden) and only those whose shapes the
//! body state holds: a body restored from the result store with its shape
//! still packed (mitcad#858) is left out. Each body once, with its version,
//! its shape (shared with the body state: whoever reads it must not write
//! into it) and the position after which it went in.
//!
//! **A classification that missed a change** costs only work: a body put
//! in and then changed by a later feature is counted
//! ([`FinalBodies::changed_later`]); the application compares the shapes
//! it meshed with the bodies' at the end and meshes a changed one again.
//!
//! The recompute closes the feed when it ends, completed, cancelled or
//! failed; a recompute that does not go through the timeline from its start
//! (one that goes on after earlier results, a preview's) puts nothing in.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use super::{BodyState, ChangeHead, Output, Version};
use crate::document::DocState;
use crate::features::{FeatureDef, FeatureEntry, Operation, PatternObjects};
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::transform::Transform;

/// A body put into the feed: final from `position` on (the timeline's
/// index of the feature after which no feature changes it).
#[derive(Debug, Clone)]
pub struct FinalBody<S> {
    pub uid: BodyUid,
    pub component: ComponentUid,
    pub version: Version,
    pub shape: S,
    pub position: usize,
}

/// The feed of final bodies of one recompute (the module comment): the
/// recompute puts bodies in, another thread takes them. Clones share it.
pub struct FinalBodies<S> {
    shared: Arc<Shared<S>>,
}

struct Shared<S> {
    queue: Mutex<Queue<S>>,
    changed: Condvar,
}

struct Queue<S> {
    bodies: Vec<FinalBody<S>>,
    closed: bool,
    given: usize,
    changed_later: usize,
}

impl<S> Clone for FinalBodies<S> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<S> Default for FinalBodies<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> FinalBodies<S> {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                queue: Mutex::new(Queue {
                    bodies: Vec::new(),
                    closed: false,
                    given: 0,
                    changed_later: 0,
                }),
                changed: Condvar::new(),
            }),
        }
    }

    fn queue(&self) -> std::sync::MutexGuard<'_, Queue<S>> {
        self.shared
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The bodies put in since the last take, waiting up to `wait` for the
    /// first; none when none came meanwhile or the feed is closed and
    /// empty.
    pub fn take(&self, wait: Duration) -> Vec<FinalBody<S>> {
        let queue = self.queue();
        let (mut queue, _) = self
            .shared
            .changed
            .wait_timeout_while(queue, wait, |q| q.bodies.is_empty() && !q.closed)
            .unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut queue.bodies)
    }

    /// Whether the recompute ended (nothing more comes) and every body was
    /// taken.
    pub fn is_done(&self) -> bool {
        let queue = self.queue();
        queue.closed && queue.bodies.is_empty()
    }

    /// Whether the recompute ended: nothing more comes.
    pub fn is_closed(&self) -> bool {
        self.queue().closed
    }

    /// The bodies put in so far.
    pub fn given(&self) -> usize {
        self.queue().given
    }

    /// Bodies put in that a later feature changed or removed all the same:
    /// what the classification missed (0 when it is right).
    pub fn changed_later(&self) -> usize {
        self.queue().changed_later
    }

    fn push(&self, body: FinalBody<S>) {
        let mut queue = self.queue();
        if queue.closed {
            return;
        }
        queue.bodies.push(body);
        queue.given += 1;
        drop(queue);
        self.shared.changed.notify_all();
    }

    fn note_changed_later(&self) {
        self.queue().changed_later += 1;
    }

    /// Nothing more comes; a thread waiting in [`FinalBodies::take`] wakes.
    pub fn close(&self) {
        self.queue().closed = true;
        self.shared.changed.notify_all();
    }
}

/// What a feature may change of the bodies, by its definition (the module
/// comment). The bodies it makes itself are new and not counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Writes {
    Nothing,
    Bodies(BTreeSet<BodyUid>),
    Component,
    All,
}

impl Writes {
    fn bodies(list: impl IntoIterator<Item = BodyUid>) -> Self {
        Writes::Bodies(list.into_iter().collect())
    }

    fn union(self, other: Writes) -> Writes {
        match (self, other) {
            (Writes::All, _) | (_, Writes::All) => Writes::All,
            (Writes::Component, _) | (_, Writes::Component) => Writes::Component,
            (Writes::Nothing, other) | (other, Writes::Nothing) => other,
            (Writes::Bodies(mut a), Writes::Bodies(b)) => {
                a.extend(b);
                Writes::Bodies(a)
            }
        }
    }
}

/// An operation of an extrusion, revolution, primitive or base feature:
/// new bodies change nothing, the others change their participants, or
/// every body of the component without any.
fn by_operation(operation: Operation, participants: &[BodyUid]) -> Writes {
    match operation {
        Operation::NewBody | Operation::NewComponent => Writes::Nothing,
        _ if participants.is_empty() => Writes::Component,
        _ => Writes::bodies(participants.iter().copied()),
    }
}

/// What a pattern or a mirror repeats changes: the bodies (their copies
/// are new, a combine joins them to the bodies), the face's body, or what
/// the repeated features change.
fn pattern(objects: &PatternObjects, earlier: &HashMap<FeatureUid, Writes>) -> Writes {
    match objects {
        PatternObjects::Bodies { bodies } => Writes::bodies(bodies.iter().copied()),
        PatternObjects::Faces { body, .. } => Writes::bodies([*body]),
        PatternObjects::Features { features } => {
            features.iter().fold(Writes::Nothing, |all, feature| {
                all.union(earlier.get(feature).cloned().unwrap_or(Writes::Component))
            })
        }
        PatternObjects::Components { .. } => Writes::All,
    }
}

/// What the feature may change of the bodies (the module comment);
/// `earlier` holds the features before it.
pub(crate) fn writes(
    entry: &FeatureEntry,
    state: &DocState,
    earlier: &HashMap<FeatureUid, Writes>,
) -> Writes {
    use FeatureDef as D;
    match &entry.def {
        D::Sketch(_)
        | D::ConstructionPlane(_)
        | D::ConstructionAxis(_)
        | D::ConstructionPoint(_)
        | D::Joint(_)
        | D::AsBuiltJoint(_)
        | D::JointOrigin(_)
        | D::RigidGroup(_)
        | D::ConstraintSet(_)
        | D::MotionLink(_)
        | D::CapturePosition(_)
        | D::MoveOccurrence(_)
        | D::BoundLength(_)
        | D::Placeholder(_)
        | D::IntersectionCurve(_)
        | D::SpatialSketch(_) => Writes::Nothing,
        D::Extrude(e) if e.active_cut_scope().is_some() => Writes::All,
        D::Extrude(e) => by_operation(e.operation, &e.participants),
        D::Revolve(r) => by_operation(r.operation, &r.participants),
        D::Box(p) => by_operation(p.operation, &p.participants),
        D::Cylinder(p) => by_operation(p.operation, &p.participants),
        D::Sphere(p) => by_operation(p.operation, &p.participants),
        D::Torus(p) => by_operation(p.operation, &p.participants),
        D::Base(b) => by_operation(b.operation, &b.participants)
            .union(Writes::bodies(b.replaces.iter().copied())),
        D::Fillet(f) => Writes::bodies([f.body]),
        D::Chamfer(c) => Writes::bodies([c.body]),
        D::OffsetFace(o) => Writes::bodies([o.body]),
        D::Thread(t) => Writes::bodies(t.faces.iter().map(|face| face.body)),
        D::Combine(c) => {
            Writes::bodies(std::iter::once(c.target).chain(c.tools.iter().map(|tool| tool.body)))
        }
        D::Move(m) => Writes::bodies(m.bodies.iter().copied()),
        D::ComponentFromBodies(c) => Writes::bodies(c.bodies.iter().copied()),
        D::BodyTransfer(t) => Writes::bodies(t.bodies.iter().copied()),
        D::RectangularPattern(p) => pattern(&p.objects, earlier),
        D::CircularPattern(p) => pattern(&p.objects, earlier),
        D::Mirror(m) => pattern(&m.objects, earlier),
        _ if state.assembly.created_by(entry.uid).is_some() => Writes::All,
        _ => Writes::Component,
    }
}

/// Where a recompute puts the final bodies into a feed (the module
/// comment): made before it goes through the timeline, told after each
/// feature what the feature set or removed. Closes the feed when dropped.
pub(super) struct Planner<S> {
    feed: FinalBodies<S>,
    /// The last features that may change any body, any body of a
    /// component, and the bodies they name.
    last_all: usize,
    last_component: HashMap<ComponentUid, usize>,
    last_body: HashMap<BodyUid, usize>,
    /// Components with an occurrence shown, and the bodies hidden.
    shown: HashSet<ComponentUid>,
    hidden: HashSet<BodyUid>,
    /// Bodies set before the position at which they are final, by it.
    scheduled: BTreeMap<usize, Vec<BodyUid>>,
    given: HashSet<BodyUid>,
}

impl<S> Drop for Planner<S> {
    fn drop(&mut self) {
        self.feed.close();
    }
}

impl<S: Clone> Planner<S> {
    /// The planner of a recompute of `state` that goes through its first
    /// `total` features.
    pub(super) fn new(state: &DocState, total: usize, feed: FinalBodies<S>) -> Self {
        let mut earlier = HashMap::new();
        let mut last_all = 0;
        let mut last_component = HashMap::new();
        let mut last_body = HashMap::new();
        for (position, entry) in state.features.iter().enumerate().take(total) {
            if entry.suppressed {
                continue;
            }
            let writes = writes(entry, state, &earlier);
            match &writes {
                Writes::Nothing => {}
                Writes::Bodies(bodies) => {
                    for body in bodies {
                        last_body.insert(*body, position);
                    }
                }
                Writes::Component => {
                    last_component.insert(entry.component, position);
                }
                Writes::All => last_all = position,
            }
            earlier.insert(entry.uid, writes);
        }
        // A component's bodies are shown where one of its occurrences is
        // (the instances' paths, as the `instances` query walks them).
        let instances = state
            .assembly
            .instances(&|_| Transform::IDENTITY, &|component| {
                vec![BodyUid::new(FeatureUid(0), component.0)]
            });
        let shown = instances
            .iter()
            .filter(|instance| instance.visible)
            .map(|instance| instance.component)
            .collect();
        let hidden = state
            .body_attributes
            .iter()
            .filter(|(_, attributes)| !attributes.visible)
            .map(|(uid, _)| *uid)
            .collect();
        Self {
            feed,
            last_all,
            last_component,
            last_body,
            shown,
            hidden,
            scheduled: BTreeMap::new(),
            given: HashSet::new(),
        }
    }

    /// The position from which the body is final while it is in
    /// `component`.
    fn final_from(&self, body: BodyUid, component: ComponentUid) -> usize {
        let of_component = self.last_component.get(&component).copied().unwrap_or(0);
        let of_body = self.last_body.get(&body).copied().unwrap_or(0);
        self.last_all.max(of_component).max(of_body)
    }

    /// After the feature at `position` was applied, with `output` its
    /// result: the bodies final now go into the feed.
    pub(super) fn after<P>(
        &mut self,
        position: usize,
        output: &Output<S>,
        components: &BTreeMap<ComponentUid, Arc<BodyState<S>>>,
        owners: &BTreeMap<BodyUid, ComponentUid>,
        pending: &BTreeMap<BodyUid, P>,
    ) {
        let mut now = Vec::new();
        for head in output.changes.heads() {
            let (ChangeHead::Set(uid, _) | ChangeHead::Remove(uid)) = *head;
            if self.given.contains(&uid) {
                self.feed.note_changed_later();
                continue;
            }
            if let ChangeHead::Set(..) = head {
                now.push(uid);
            }
        }
        // (Every position the planner waits for is gone through; those
        // before it too, should one be skipped.)
        let later = self.scheduled.split_off(&(position + 1));
        for (_, bodies) in std::mem::replace(&mut self.scheduled, later) {
            now.extend(bodies);
        }
        for uid in now {
            if self.given.contains(&uid) {
                continue;
            }
            let Some(component) = owners.get(&uid).copied() else {
                continue;
            };
            let from = self.final_from(uid, component);
            if from > position {
                self.scheduled.entry(from).or_default().push(uid);
                continue;
            }
            if pending.contains_key(&uid)
                || self.hidden.contains(&uid)
                || !self.shown.contains(&component)
            {
                continue;
            }
            let Some(body) = components
                .get(&component)
                .and_then(|bodies| bodies.get(&uid))
            else {
                continue;
            };
            self.given.insert(uid);
            self.feed.push(FinalBody {
                uid,
                component,
                version: body.version,
                shape: body.value.clone(),
                position,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::FinalBodies;
    use crate::ids::BodyUid;
    use crate::test_support::{Block, block, corner};
    use crate::testing::MockKernel;

    type Feed = FinalBodies<<MockKernel as crate::Kernel>::Shape>;

    /// Block1 (F2.b0, final after its fillet at position 2) and a cylinder
    /// (made at 4) joined to by name at 6, then a sketch.
    fn design() -> (Block, BodyUid) {
        let mut b = block();
        b.fillet(&[&corner(b.extrude, 1, 0)], 2.0);
        let made = b.circle_extrude("new_body", &[]);
        let cylinder = BodyUid::new(made, 0);
        b.circle_extrude("join", &[cylinder]);
        b.doc.add_sketch(crate::features::SketchPlane::Xy).unwrap();
        (b, cylinder)
    }

    /// Recomputes from the start with a feed; what went in, by uid, with
    /// the position each is final from.
    fn fed(b: &mut Block) -> (Feed, Vec<(BodyUid, usize)>) {
        let feed = Feed::new();
        b.doc.feed_final_bodies(feed.clone());
        b.doc.recompute();
        assert!(feed.is_closed());
        let mut given: Vec<_> = feed
            .take(Duration::ZERO)
            .into_iter()
            .map(|body| (body.uid, body.position))
            .collect();
        given.sort();
        assert!(feed.is_done());
        (feed, given)
    }

    #[test]
    fn bodies_go_in_once_no_later_feature_names_them() {
        let (mut b, cylinder) = design();
        let (feed, given) = fed(&mut b);
        // Each at its last change, with the shape and version it ends with.
        assert_eq!(given, vec![(b.body, 2), (cylinder, 6)]);
        assert_eq!(feed.given(), 2);
        assert_eq!(feed.changed_later(), 0);
        let taken = {
            let again = Feed::new();
            b.doc.feed_final_bodies(again.clone());
            b.doc.recompute();
            again.take(Duration::ZERO)
        };
        for body in taken {
            assert_eq!(Some(body.version), b.doc.body_version(body.uid));
        }
    }

    #[test]
    fn an_operation_on_all_bodies_holds_its_component_back() {
        let (mut b, cylinder) = design();
        // A cut of every body of the component at 9: both wait for it.
        b.circle_extrude("cut", &[]);
        let (feed, given) = fed(&mut b);
        assert_eq!(given, vec![(b.body, 9), (cylinder, 9)]);
        assert_eq!(feed.changed_later(), 0);
        // Suppressed, it changes nothing: as before.
        let cut = b.doc.state().features[9].uid;
        b.doc.set_suppressed(cut, true).unwrap();
        let (_, given) = fed(&mut b);
        assert_eq!(given, vec![(b.body, 2), (cylinder, 6)]);
    }

    #[test]
    fn hidden_bodies_and_other_recomputes_give_nothing() {
        let (mut b, cylinder) = design();
        b.doc.set_body_visible(b.body, false).unwrap();
        let (_, given) = fed(&mut b);
        assert_eq!(given, vec![(cylinder, 6)]);
        // A command's recompute goes on after the last results: the feed
        // is closed with nothing in it.
        let feed = Feed::new();
        b.doc.feed_final_bodies(feed.clone());
        b.doc.add_sketch(crate::features::SketchPlane::Xy).unwrap();
        assert!(feed.is_done());
        assert_eq!(feed.given(), 0);
    }
}
