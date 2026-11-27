// SPDX-License-Identifier: MIT
//! Exploded views of the document (mitcad#2113; the data in
//! `crate::explode`): adding, editing and deleting them (undo steps that
//! recompute nothing), Auto Explode, and an explode worked out at a time
//! of its animation: each step's motion, the offsets of what it moves, its
//! trail lines, and where the bodies are in the design.
//!
//! Nothing here changes the design's placements: an explode is worked out
//! from the recompute's placements whenever it is asked for, so it shows
//! the design as it is now (an edit made while a view is shown is shown
//! exploded again).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::{DocState, Document, ModelError, invalid};
use crate::datum::Vec3;
use crate::explode::{DEFAULT_AUTO_FACTOR, ExplodeAxis, ExplodeStep, ExplodeView, value_text};
use crate::expr::{Dims, Expr, Unit};
use crate::features::{JointOrigin, OccurrencePath, References};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::{JointKind, SlideAxis, path_component, resolved_origin_frame};
use crate::kernel::Kernel;
use crate::parameters::unit_for;
use crate::transform::Transform;
use mitcad_vec3::{add, dot, norm, scale, sub, unit};

/// What a step moves: an occurrence (its path from the view's component,
/// with all it places) or a body of the view's component itself.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExplodePart {
    Occurrence(Vec<OccurrenceUid>),
    Body(BodyUid),
}

/// A step of an explode worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedStep {
    /// Its own name, else `Step <n>`.
    pub name: String,
    /// How far it has gone (0 to 1) at the time asked for.
    pub fraction: f64,
    /// Its time in the animation (seconds).
    pub seconds: f64,
    /// Its whole move (mm) along `direction` (a unit vector in the view
    /// component's coordinates); 0 and None when it does not move.
    pub distance: f64,
    pub direction: Option<Vec3>,
    /// Its whole turn (radians) about the axis through `axis.0` along
    /// `axis.1`; 0 and None when it does not turn.
    pub angle: f64,
    pub axis: Option<(Vec3, Vec3)>,
    /// What it does at its fraction, in the view component's coordinates:
    /// the turn, then the move.
    pub transform: Transform,
    /// What it moves (those that are gone left out).
    pub parts: Vec<ExplodePart>,
}

impl ResolvedStep {
    /// Whether it moves what lies at `path` (a body of the view's
    /// component itself when `path` is empty and `body` is given).
    fn moves(&self, path: &[OccurrenceUid], body: Option<BodyUid>) -> bool {
        self.parts.iter().any(|part| match part {
            ExplodePart::Occurrence(o) => !o.is_empty() && path.starts_with(o),
            ExplodePart::Body(b) => path.is_empty() && body == Some(*b),
        })
    }
}

/// A trail line: from a moved part's reference point where a step found it
/// to where the step left it, in the view component's coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplodeTrail {
    /// The step's index.
    pub step: usize,
    pub part: ExplodePart,
    pub from: Vec3,
    pub to: Vec3,
}

/// An explode view worked out at a time of its animation.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplodeState {
    pub name: String,
    pub component: ComponentUid,
    /// The animation's length (seconds).
    pub seconds: f64,
    /// The time asked for; None: every step done.
    pub time: Option<f64>,
    pub steps: Vec<ResolvedStep>,
    pub trails: Vec<ExplodeTrail>,
    /// What it could not do, in words: occurrences and bodies that are
    /// gone, values and geometry it could not read (such a step does not
    /// move or turn).
    pub warnings: Vec<String>,
}

impl ExplodeState {
    /// Where the explode takes what lies at `path` below the view's
    /// component (a body of the component itself: `path` empty and
    /// `body`), in the component's coordinates: the motions of the steps
    /// that move it or an occurrence it lies in, one after another.
    pub fn offset(&self, path: &[OccurrenceUid], body: Option<BodyUid>) -> Transform {
        self.offset_through(self.steps.len(), path, body)
    }

    /// [`ExplodeState::offset`] after the first `steps` steps.
    fn offset_through(
        &self,
        steps: usize,
        path: &[OccurrenceUid],
        body: Option<BodyUid>,
    ) -> Transform {
        offset_in(&self.steps[..steps], path, body)
    }

    /// Each part the steps move, once, in the order they first move it,
    /// with where it ends.
    pub fn part_offsets(&self) -> Vec<(ExplodePart, Transform)> {
        let mut out: Vec<(ExplodePart, Transform)> = Vec::new();
        for step in &self.steps {
            for part in &step.parts {
                if out.iter().any(|(p, _)| p == part) {
                    continue;
                }
                let offset = match part {
                    ExplodePart::Occurrence(path) => self.offset(path, None),
                    ExplodePart::Body(b) => self.offset(&[], Some(*b)),
                };
                out.push((part.clone(), offset));
            }
        }
        out
    }
}

fn offset_in(steps: &[ResolvedStep], path: &[OccurrenceUid], body: Option<BodyUid>) -> Transform {
    steps
        .iter()
        .filter(|s| s.moves(path, body))
        .fold(Transform::IDENTITY, |t, s| s.transform.after(&t))
}

/// Where an explode shows the design's bodies, in the design's
/// coordinates (the view's component at each of its placements).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExplodePlacements {
    /// Occurrences that move (paths from the root, each with every
    /// occurrence below it that moves with it) and where they are.
    pub occurrences: Vec<(Vec<OccurrenceUid>, Transform)>,
    /// Bodies of the view's component itself that move: the path that
    /// places the component (empty for the root), the body and where it
    /// is.
    pub bodies: Vec<(Vec<OccurrenceUid>, BodyUid, Transform)>,
    /// The trail lines (from, to) at each placement.
    pub trails: Vec<(Vec3, Vec3)>,
}

/// A body below a component: its path from the component and its
/// placement in the component's coordinates.
struct Below {
    path: Vec<OccurrenceUid>,
    body: BodyUid,
    transform: Transform,
}

/// The inverse of a rigid motion.
fn rigid_inverse(t: &Transform) -> Transform {
    let linear: [[f64; 3]; 3] = std::array::from_fn(|r| std::array::from_fn(|c| t.linear[c][r]));
    let back = Transform {
        linear,
        translation: [0.0; 3],
    };
    let moved = back.apply_vector(t.translation);
    Transform {
        linear,
        translation: scale(moved, -1.0),
    }
}

/// Whether `step` moves every one of `bodies` lying at `path` below its
/// view's component: with an occurrence the path leads through, or as
/// bodies of the component itself when the path is empty.
fn moves_all(step: &ExplodeStep, path: &[OccurrenceUid], bodies: &[BodyUid]) -> bool {
    step.occurrences
        .iter()
        .any(|o| !o.is_empty() && path.starts_with(&o.0))
        || (path.is_empty() && !bodies.is_empty() && bodies.iter().all(|b| step.bodies.contains(b)))
}

/// A box's centre and its diagonal.
fn centre_and_size(min: Vec3, max: Vec3) -> (Vec3, f64) {
    (scale(add(min, max), 0.5), norm(sub(max, min)))
}

impl DocState {
    /// Drops what deleted occurrences and components leave behind: views
    /// of components that are gone, occurrences of steps that no longer
    /// exist where their paths say (`run_checked_action` of the api says
    /// so in words, mitcad#2113).
    pub(crate) fn prune_explode_views(&mut self) {
        if self.explode_views.is_empty() {
            return;
        }
        let assembly = &self.assembly;
        self.explode_views.retain(|v| assembly.exists(v.component));
        for view in &mut self.explode_views {
            for step in &mut view.steps {
                step.occurrences
                    .retain(|p| path_component(assembly, view.component, &p.0).is_ok());
            }
        }
    }

    /// Explode views follow an occurrence moved to another parent
    /// (Reparent Occurrence, mitcad#2328; `DocState::follow`): a path of a
    /// step, or of a direction's or axis' geometry, that leads through
    /// `old` (the occurrence's path from the root before the move) leads
    /// through `new` (after) when the view's component still holds it. A
    /// path whose occurrence leaves the view's component stays as it was:
    /// [`DocState::prune_explode_views`] then drops it from its step, and
    /// the command says so in words; such a direction or axis reports its
    /// geometry gone.
    pub(crate) fn follow_explode_moved(&mut self, old: &[OccurrenceUid], new: &[OccurrenceUid]) {
        let Some(moved) = old.last().copied() else {
            return;
        };
        let assembly = &self.assembly;
        for view in &mut self.explode_views {
            // The view's component where it holds the occurrence: a
            // placement of it not through the occurrence (the paths of a
            // view of the occurrence's own component, or of one inside it,
            // are below it and stay). The occurrence's parent is placed
            // once, so one placement at most leads to it.
            let found = assembly
                .paths_to(view.component)
                .into_iter()
                .find_map(|place| {
                    if place.contains(&moved) {
                        return None;
                    }
                    let from = old.strip_prefix(place.as_slice())?;
                    (!from.is_empty()).then(|| (from.to_vec(), new.strip_prefix(place.as_slice())))
                });
            let Some((from, Some(to))) = found else {
                continue;
            };
            let follow = |path: &mut OccurrencePath| {
                if path.0.starts_with(&from) {
                    let rest = path.0.split_off(from.len());
                    path.0.clear();
                    path.0.extend_from_slice(to);
                    path.0.extend(rest);
                }
            };
            for step in &mut view.steps {
                for path in &mut step.occurrences {
                    follow(path);
                }
                for axis in [&mut step.direction, &mut step.axis].into_iter().flatten() {
                    follow(&mut axis.occurrence);
                }
            }
        }
    }

    /// Explode views follow bodies moved into a new component placed in
    /// `source` by `occurrence` (Move Features into a Component, Move
    /// History into Component; mitcad#2328): a step of a view of `source`
    /// that moved every one of those bodies moves the occurrence instead
    /// (a step that moved only some of them reports the others gone, as it
    /// cannot move part of an occurrence); a direction's or axis' geometry
    /// of the moved features, read in `source`, is read through the
    /// occurrence.
    pub(crate) fn follow_explode_into(
        &mut self,
        source: ComponentUid,
        occurrence: OccurrenceUid,
        features: &BTreeSet<FeatureUid>,
        bodies: &[BodyUid],
    ) {
        let assembly = &self.assembly;
        for view in &mut self.explode_views {
            let component = view.component;
            for step in &mut view.steps {
                if component == source
                    && !bodies.is_empty()
                    && bodies.iter().all(|b| step.bodies.contains(b))
                {
                    step.bodies.retain(|b| !bodies.contains(b));
                    let path = OccurrencePath(vec![occurrence]);
                    if !step.occurrences.contains(&path) {
                        step.occurrences.push(path);
                    }
                }
                for axis in [&mut step.direction, &mut step.axis].into_iter().flatten() {
                    let Some(geometry) = &axis.geometry else {
                        continue;
                    };
                    let mut references = References::default();
                    geometry.add_to(&mut references);
                    if references.features.iter().any(|f| features.contains(f))
                        && path_component(assembly, component, &axis.occurrence.0) == Ok(source)
                    {
                        axis.occurrence.0.push(occurrence);
                    }
                }
            }
        }
    }

    /// Explode views follow bodies moved to a chosen destination (Move
    /// Features into a Component with an explicit destination,
    /// mitcad#2572): the source and the destination each have one
    /// placement in the design, `from` and `to` their paths from the root.
    /// In a view whose component holds both, a step that moved every one
    /// of the bodies (as bodies of the view's component, or with an
    /// occurrence they lay in) moves them where they are now: as bodies of
    /// the view's component when that is the destination, else with the
    /// destination's occurrence, with all it places (a step cannot move
    /// part of an occurrence). A step that moved only some of them, or a
    /// view whose component does not hold the destination, keeps what it
    /// had and reports the bodies it no longer finds. A direction's or
    /// axis' geometry of the moved features, read in the source, is read in
    /// the destination.
    pub(crate) fn follow_explode_destination(
        &mut self,
        features: &BTreeSet<FeatureUid>,
        bodies: &[BodyUid],
        from: &[OccurrenceUid],
        to: &[OccurrenceUid],
    ) {
        let assembly = &self.assembly;
        for view in &mut self.explode_views {
            // The view's component's placement above the source: the
            // source's path is its only one, so the view's component has
            // one at most that leads to it.
            let places = assembly.paths_to(view.component);
            let below = |path: &[OccurrenceUid]| {
                places
                    .iter()
                    .find_map(|place| path.strip_prefix(place.as_slice()))
                    .map(<[OccurrenceUid]>::to_vec)
            };
            let Some(source) = below(from) else {
                continue;
            };
            let target = below(to);
            for step in &mut view.steps {
                if let Some(target) = &target
                    && moves_all(step, &source, bodies)
                {
                    if source.is_empty() {
                        step.bodies.retain(|b| !bodies.contains(b));
                    }
                    if !moves_all(step, target, bodies) {
                        if target.is_empty() {
                            for body in bodies {
                                if !step.bodies.contains(body) {
                                    step.bodies.push(*body);
                                }
                            }
                        } else {
                            step.occurrences.push(OccurrencePath(target.clone()));
                        }
                    }
                }
                for axis in [&mut step.direction, &mut step.axis].into_iter().flatten() {
                    let Some(geometry) = &axis.geometry else {
                        continue;
                    };
                    let mut references = References::default();
                    geometry.add_to(&mut references);
                    if let Some(target) = &target
                        && axis.occurrence.0 == source
                        && references.features.iter().any(|f| features.contains(f))
                    {
                        axis.occurrence.0.clone_from(target);
                    }
                }
            }
        }
    }
}

impl<K: Kernel> Document<K> {
    /// The explode views in the order they were added.
    pub fn explode_views(&self) -> &[ExplodeView] {
        &self.state.explode_views
    }

    fn explode_index(&self, name: &str) -> Result<usize, ModelError> {
        let name = name.trim();
        self.state
            .explode_views
            .iter()
            .position(|v| v.name == name)
            .ok_or_else(|| invalid(format!("there is no explode view {name}")))
    }

    /// The view by name.
    pub fn explode_view(&self, name: &str) -> Result<&ExplodeView, ModelError> {
        Ok(&self.state.explode_views[self.explode_index(name)?])
    }

    /// A value of a step: an expression of a length (mm) or an angle
    /// (radians), a plain number in the design's units.
    pub fn explode_value(&self, text: &str, dims: Dims) -> Result<f64, String> {
        let params = &self.state.parameters;
        let context = params.context();
        let unit = unit_for(dims, &context).unwrap_or(Unit::NONE);
        let parsed = Expr::parse(text).map_err(|e| e.to_string())?;
        let value = parsed
            .eval_as(params.table(), &context, unit)
            .map_err(|e| e.to_string())?;
        if value.value.is_finite() {
            Ok(value.value)
        } else {
            Err(format!("{text} is not a finite value"))
        }
    }

    /// The placement of a path from `component` in its coordinates, as the
    /// recompute has it.
    fn placement_below(&self, component: ComponentUid, path: &[OccurrenceUid]) -> Transform {
        let mut above = self.first_placement(component);
        let mut t = Transform::IDENTITY;
        for o in path {
            t = t.after(&self.occurrence_step(&above, *o));
            above.push(*o);
        }
        t
    }

    /// The first path from the root that places a component (empty for the
    /// root, or for a component placed nowhere).
    fn first_placement(&self, component: ComponentUid) -> Vec<OccurrenceUid> {
        self.state
            .assembly
            .paths_to(component)
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    /// Every body below a component, as the recompute places them in its
    /// coordinates.
    fn bodies_below(&self, component: ComponentUid) -> Vec<Below> {
        let a = self.state.assembly.at_placements(&self.result.placements);
        let above = self.first_placement(component);
        let mut out = Vec::new();
        let mut path = Vec::new();
        self.walk_below(
            &a,
            component,
            Transform::IDENTITY,
            &above,
            &mut path,
            &mut out,
        );
        out
    }

    fn walk_below(
        &self,
        a: &crate::assembly::Assembly,
        component: ComponentUid,
        transform: Transform,
        above: &[OccurrenceUid],
        path: &mut Vec<OccurrenceUid>,
        out: &mut Vec<Below>,
    ) {
        for body in self.component_bodies(component) {
            out.push(Below {
                path: path.clone(),
                body,
                transform,
            });
        }
        if path.len() >= 32 {
            return;
        }
        let children: Vec<(OccurrenceUid, ComponentUid)> = a
            .children(component)
            .map(|o| (o.uid, o.component))
            .collect();
        for (o, child) in children {
            if path.contains(&o) {
                continue;
            }
            let mut prefix = above.to_vec();
            prefix.extend_from_slice(path);
            let placed = transform.after(&self.occurrence_step(&prefix, o));
            path.push(o);
            self.walk_below(a, child, placed, above, path, out);
            path.pop();
        }
    }

    /// The box of what a part places, in the component's coordinates
    /// (min, max); None when it has no body with a box.
    fn part_box(
        &self,
        below: &[Below],
        part: &ExplodePart,
        boxes: &mut HashMap<BodyUid, Option<(Vec3, Vec3)>>,
    ) -> Option<(Vec3, Vec3)> {
        self.box_of(
            below,
            |b| match part {
                ExplodePart::Occurrence(p) => b.path.starts_with(p),
                ExplodePart::Body(body) => b.path.is_empty() && b.body == *body,
            },
            boxes,
        )
    }

    /// The box of the bodies `mine` takes, in the component's coordinates.
    fn box_of(
        &self,
        below: &[Below],
        mine: impl Fn(&Below) -> bool,
        boxes: &mut HashMap<BodyUid, Option<(Vec3, Vec3)>>,
    ) -> Option<(Vec3, Vec3)> {
        let mut found: Option<(Vec3, Vec3)> = None;
        for b in below.iter().filter(|b| mine(b)) {
            let own = *boxes.entry(b.body).or_insert_with(|| {
                let shape = self.body_shape(b.body)?;
                let bb = self.kernel.bounding_box(shape).ok()??;
                Some((bb.min, bb.max))
            });
            let Some((min, max)) = own else {
                continue;
            };
            let bb = crate::kernel::BoundingBox { min, max };
            for corner in bb.corners() {
                let p = b.transform.apply_point(corner);
                found = Some(match found {
                    None => (p, p),
                    Some((lo, hi)) => (
                        std::array::from_fn(|i| lo[i].min(p[i])),
                        std::array::from_fn(|i| hi[i].max(p[i])),
                    ),
                });
            }
        }
        found
    }

    /// A part's reference point where the design has it: its box's centre,
    /// else where it is placed.
    fn part_centre(
        &self,
        component: ComponentUid,
        below: &[Below],
        part: &ExplodePart,
        boxes: &mut HashMap<BodyUid, Option<(Vec3, Vec3)>>,
    ) -> Vec3 {
        match self.part_box(below, part, boxes) {
            Some((min, max)) => centre_and_size(min, max).0,
            None => match part {
                ExplodePart::Occurrence(p) => self.placement_below(component, p).translation,
                ExplodePart::Body(_) => [0.0; 3],
            },
        }
    }

    /// A step's direction or axis in the view component's coordinates: the
    /// point its line goes through (None for a vector alone) and the unit
    /// direction.
    pub fn explode_axis(
        &self,
        component: ComponentUid,
        axis: &ExplodeAxis,
    ) -> Result<(Option<Vec3>, Vec3), String> {
        let sign = if axis.flip { -1.0 } else { 1.0 };
        if let Some(v) = axis.vector {
            let u = unit(v).ok_or("the direction is zero")?;
            return Ok((None, scale(u, sign)));
        }
        let geometry = axis.geometry.clone().ok_or("no direction is given")?;
        path_component(&self.state.assembly, component, &axis.occurrence.0)?;
        let origin = JointOrigin {
            occurrence: axis.occurrence.clone(),
            geometry,
            frame_override: None,
        };
        let local =
            resolved_origin_frame(&self.kernel, &self.state, &self.result, component, &origin)
                .map_err(|e| {
                    crate::notice::FeatureError::new("its geometry is gone or gives no direction")
                        .with_detail(e)
                        .text()
                })?;
        let frame = self
            .placement_below(component, &axis.occurrence.0)
            .after(&local);
        let u =
            unit(frame.apply_vector([0.0, 0.0, 1.0])).ok_or("the geometry gives no direction")?;
        Ok((Some(frame.translation), scale(u, sign)))
    }

    /// Why a view cannot be kept: its own checks, its component, and each
    /// step's occurrences (paths from it), bodies and values.
    fn check_explode_view(&self, view: &ExplodeView) -> Result<(), ModelError> {
        view.check().map_err(invalid)?;
        let a = &self.state.assembly;
        if !a.exists(view.component) {
            return Err(invalid(format!("{}: its component is gone", view.name)));
        }
        if !view.component.is_root() && a.children(view.component).next().is_none() {
            return Err(invalid(format!(
                "{}: {} places no components to explode",
                view.name,
                a.name(view.component)
            )));
        }
        let bodies = self.component_bodies(view.component);
        for (i, step) in view.steps.iter().enumerate() {
            let at = |e: String| invalid(format!("{}, {}: {e}", view.name, step.shown_name(i)));
            for path in &step.occurrences {
                path_component(a, view.component, &path.0).map_err(at)?;
            }
            for body in &step.bodies {
                if !bodies.contains(body) {
                    return Err(at(format!(
                        "{} is not a body of {}",
                        self.body_name(*body),
                        a.name(view.component)
                    )));
                }
            }
            if let Some(text) = &step.distance {
                self.explode_value(text, Dims::LENGTH)
                    .map_err(|e| at(format!("distance: {e}")))?;
            }
            if let Some(text) = &step.angle {
                self.explode_value(text, Dims::ANGLE)
                    .map_err(|e| at(format!("angle: {e}")))?;
            }
            for (what, axis) in [("direction", &step.direction), ("axis", &step.axis)] {
                if let Some(axis) = axis {
                    path_component(a, view.component, &axis.occurrence.0)
                        .map_err(|e| at(format!("{what}: {e}")))?;
                }
            }
        }
        Ok(())
    }

    /// A free name: `Explode1`, `Explode2`, ...
    fn free_explode_name(&self) -> String {
        (1..)
            .map(|n| format!("Explode{n}"))
            .find(|n| self.explode_index(n).is_err())
            .expect("a free name")
    }

    /// Adds a view; without a name it is `Explode1`, `Explode2`, ...
    /// Returns the name. One undo step that recomputes nothing.
    pub fn add_explode_view(&mut self, mut view: ExplodeView) -> Result<String, ModelError> {
        view.name = view.name.trim().to_owned();
        if view.name.is_empty() {
            view.name = self.free_explode_name();
        }
        if self.explode_index(&view.name).is_ok() {
            return Err(invalid(format!("an explode view {} exists", view.name)));
        }
        self.check_explode_view(&view)?;
        let name = view.name.clone();
        let label = format!("Add {name}");
        self.apply_with(|state| {
            state.explode_views.push(view);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Changes a view: its name and its steps (all of them), each when
    /// given. A change that changes nothing is no undo step.
    pub fn edit_explode_view(
        &mut self,
        name: &str,
        new_name: Option<&str>,
        steps: Option<Vec<ExplodeStep>>,
    ) -> Result<(), ModelError> {
        let i = self.explode_index(name)?;
        let old = self.state.explode_views[i].clone();
        let mut view = old.clone();
        if let Some(new_name) = new_name.map(str::trim).filter(|n| *n != old.name) {
            if new_name.is_empty() {
                return Err(invalid("the explode view needs a name"));
            }
            if self.explode_index(new_name).is_ok() {
                return Err(invalid(format!("an explode view {new_name} exists")));
            }
            view.name = new_name.to_owned();
        }
        if let Some(steps) = steps {
            view.steps = steps;
        }
        if view == old {
            return Ok(());
        }
        self.check_explode_view(&view)?;
        let label = if view.name != old.name && view.steps == old.steps {
            format!("Rename {}", old.name)
        } else {
            format!("Edit {}", old.name)
        };
        self.apply_with(|state| {
            state.explode_views[i] = view;
            Ok((label, (), false))
        })
    }

    pub fn delete_explode_view(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.explode_index(name)?;
        let label = format!("Delete {}", self.state.explode_views[i].name);
        self.apply_with(|state| {
            state.explode_views.remove(i);
            Ok((label, (), false))
        })
    }

    /// An explode view worked out at a time of its animation (seconds from
    /// the assembled design; None: every step done). Changes nothing.
    pub fn explode_state(&self, name: &str, time: Option<f64>) -> Result<ExplodeState, ModelError> {
        if time.is_some_and(|t| !t.is_finite()) {
            return Err(invalid("the time must be finite"));
        }
        let view = self.explode_view(name)?.clone();
        Ok(self.explode_worked_out(&view, time))
    }

    /// [`Document::explode_state`] of a view not (yet) kept: Auto
    /// Explode's and the panel's previews.
    pub fn explode_worked_out(&self, view: &ExplodeView, time: Option<f64>) -> ExplodeState {
        let a = &self.state.assembly;
        let below = self.bodies_below(view.component);
        let own_bodies = self.component_bodies(view.component);
        let mut boxes: HashMap<BodyUid, Option<(Vec3, Vec3)>> = HashMap::new();
        let fractions = view.fractions(time);
        let mut steps: Vec<ResolvedStep> = Vec::new();
        let mut trails = Vec::new();
        let mut warnings = Vec::new();
        for (i, step) in view.steps.iter().enumerate() {
            let label = format!("{}, {}", view.name, step.shown_name(i));
            let mut warn = |text: String| {
                if !warnings.contains(&text) {
                    warnings.push(text);
                }
            };
            let mut parts = Vec::new();
            for path in &step.occurrences {
                if path_component(a, view.component, &path.0).is_ok() {
                    parts.push(ExplodePart::Occurrence(path.0.clone()));
                } else {
                    warn(format!("{label}: an occurrence it moved is gone"));
                }
            }
            for body in &step.bodies {
                if own_bodies.contains(body) {
                    parts.push(ExplodePart::Body(*body));
                } else {
                    warn(format!("{label}: a body it moved is gone"));
                }
            }
            if parts.is_empty() {
                warn(format!("{label}: it moves nothing"));
            }
            // The parts' reference points where the earlier steps left
            // them.
            let refs: Vec<Vec3> = parts
                .iter()
                .map(|part| {
                    let base = match step.point {
                        Some(p) => p,
                        None => self.part_centre(view.component, &below, part, &mut boxes),
                    };
                    let (path, body) = match part {
                        ExplodePart::Occurrence(p) => (p.as_slice(), None),
                        ExplodePart::Body(b) => (&[][..], Some(*b)),
                    };
                    offset_in(&steps, path, body).apply_point(base)
                })
                .collect();
            let value =
                |text: &Option<String>, dims: Dims, what: &str, warn: &mut dyn FnMut(String)| {
                    let text = text.as_ref()?;
                    match self.explode_value(text, dims) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            warn(format!("{label}: its {what} cannot be read ({e})"));
                            None
                        }
                    }
                };
            let line = |axis: &Option<ExplodeAxis>, what: &str, warn: &mut dyn FnMut(String)| {
                let axis = axis.as_ref()?;
                match self.explode_axis(view.component, axis) {
                    Ok(found) => Some(found),
                    Err(e) => {
                        warn(format!("{label}: its {what} cannot be found ({e})"));
                        None
                    }
                }
            };
            let distance = value(&step.distance, Dims::LENGTH, "distance", &mut warn);
            let direction = line(&step.direction, "direction", &mut warn).map(|(_, u)| u);
            let angle = value(&step.angle, Dims::ANGLE, "angle", &mut warn);
            let axis = line(&step.axis, "axis", &mut warn).map(|(point, u)| {
                // A vector alone turns about the moved parts' centre.
                let centre = if refs.is_empty() {
                    [0.0; 3]
                } else {
                    scale(
                        refs.iter().fold([0.0; 3], |s, p| add(s, *p)),
                        1.0 / refs.len() as f64,
                    )
                };
                (point.unwrap_or(centre), u)
            });
            let (distance, direction) = match (distance, direction) {
                (Some(d), Some(u)) => (d, Some(u)),
                _ => (0.0, None),
            };
            let (angle, axis) = match (angle, axis) {
                (Some(t), Some(line)) => (t, Some(line)),
                _ => (0.0, None),
            };
            let f = fractions[i];
            let mut transform = Transform::IDENTITY;
            if let Some((point, u)) = axis
                && let Some(turn) = Transform::rotation(point, u, angle * f)
            {
                transform = turn;
            }
            if let Some(u) = direction {
                transform = Transform::translation(scale(u, distance * f)).after(&transform);
            }
            if step.trails && f > 0.0 {
                for (part, from) in parts.iter().zip(&refs) {
                    let to = transform.apply_point(*from);
                    if norm(sub(to, *from)) > 1e-9 {
                        trails.push(ExplodeTrail {
                            step: i,
                            part: part.clone(),
                            from: *from,
                            to,
                        });
                    }
                }
            }
            steps.push(ResolvedStep {
                name: step.shown_name(i),
                fraction: f,
                seconds: step.seconds(),
                distance,
                direction,
                angle,
                axis,
                transform,
                parts,
            });
        }
        ExplodeState {
            name: view.name.clone(),
            component: view.component,
            seconds: view.seconds(),
            time,
            steps,
            trails,
            warnings,
        }
    }

    /// Where an explode shows the bodies in the design: the view's
    /// component at each of its placements (the root once).
    pub fn explode_placements(&self, explode: &ExplodeState) -> ExplodePlacements {
        let mut out = ExplodePlacements::default();
        let below = self.bodies_below(explode.component);
        let a = &self.state.assembly;
        let places: Vec<Vec<OccurrenceUid>> = a.paths_to(explode.component);
        for place in places {
            let world = crate::joints::path_placement(
                &self.result.placements,
                &self.result.joints.flexed,
                &place,
            );
            let mut seen: BTreeMap<Vec<OccurrenceUid>, ()> = BTreeMap::new();
            for b in &below {
                let body = b.path.is_empty().then_some(b.body);
                let offset = explode.offset(&b.path, body);
                if offset.is_identity() {
                    continue;
                }
                let placed = world.after(&offset).after(&b.transform);
                let mut full = place.clone();
                full.extend_from_slice(&b.path);
                if b.path.is_empty() {
                    out.bodies.push((full, b.body, placed));
                } else if seen.insert(b.path.clone(), ()).is_none() {
                    out.occurrences.push((full, placed));
                }
            }
            // Occurrences without bodies of their own that hold moved
            // ones: the paths above each moved body's.
            let mut above: Vec<(Vec<OccurrenceUid>, Transform)> = Vec::new();
            for b in &below {
                for k in 1..b.path.len() {
                    let path = &b.path[..k];
                    if seen.contains_key(path) || above.iter().any(|(p, _)| p == path) {
                        continue;
                    }
                    let offset = explode.offset(path, None);
                    if offset.is_identity() {
                        continue;
                    }
                    let placed = world
                        .after(&offset)
                        .after(&self.placement_below(explode.component, path));
                    above.push((path.to_vec(), placed));
                }
            }
            for (path, placed) in above {
                let mut full = place.clone();
                full.extend_from_slice(&path);
                out.occurrences.push((full, placed));
            }
            for trail in &explode.trails {
                out.trails
                    .push((world.apply_point(trail.from), world.apply_point(trail.to)));
            }
        }
        out
    }

    /// How an explode moves what lies at `path` (from the root; with
    /// `body`, that body of the component the path ends in) in the
    /// design's coordinates (mitcad#2328): where the design has it
    /// assembled to where the explode shows it. Measure measures the picks
    /// where the 3D view shows them by it. The identity for what the
    /// explode does not move.
    pub fn explode_motion(
        &self,
        explode: &ExplodeState,
        path: &[OccurrenceUid],
        body: Option<BodyUid>,
    ) -> Transform {
        for place in self.state.assembly.paths_to(explode.component) {
            let Some(below) = path.strip_prefix(place.as_slice()) else {
                continue;
            };
            let offset = explode.offset(below, body);
            if offset.is_identity() {
                return Transform::IDENTITY;
            }
            // The offset is in the view component's coordinates at this
            // placement of it.
            let world = crate::joints::path_placement(
                &self.result.placements,
                &self.result.joints.flexed,
                &place,
            );
            return world.after(&offset).after(&rigid_inverse(&world));
        }
        Transform::IDENTITY
    }

    /// The occurrences' placements in their parents with an explode, as
    /// a motion study's are given (mitcad#1597, the animated USDZ export):
    /// the moved occurrences of the explode, from the recompute's
    /// placements. A view of a component placed several times
    /// (mitcad#2328) gives each occurrence below it one placement in its
    /// parent, the same at every placement of the component, as the
    /// explode's offsets are in the component's coordinates.
    pub fn explode_parent_placements(
        &self,
        explode: &ExplodeState,
    ) -> BTreeMap<OccurrenceUid, Transform> {
        let component = explode.component;
        let mut out = self.result.placements.clone();
        let a = self.state.assembly.at_placements(&self.result.placements);
        // Each occurrence below the view's component once, through its
        // first path from the component.
        let mut stack: Vec<(Vec<OccurrenceUid>, ComponentUid)> = vec![(Vec::new(), component)];
        while let Some((path, component)) = stack.pop() {
            if path.len() >= 32 {
                continue;
            }
            for o in a.children(component) {
                if path.contains(&o.uid) {
                    continue;
                }
                let mut child = path.clone();
                child.push(o.uid);
                let parent_offset = explode.offset(&path, None);
                let own_offset = explode.offset(&child, None);
                if !own_offset.is_identity() || !parent_offset.is_identity() {
                    // placement' = (P_parent')^-1 · offset · P_parent · p
                    let parent = self.placement_below(component, &path);
                    let p = self.placement(o.uid).unwrap_or(Transform::IDENTITY);
                    let moved_parent = parent_offset.after(&parent);
                    let placed = rigid_inverse(&moved_parent)
                        .after(&own_offset)
                        .after(&parent)
                        .after(&p);
                    out.insert(o.uid, placed);
                }
                stack.push((child, o.component));
            }
        }
        out
    }

    /// The motions of the view component's own bodies an explode moves, in
    /// the component's coordinates (mitcad#2328, the animated USDZ export:
    /// a body's mesh moves in its component); those it does not move are
    /// left out.
    pub fn explode_body_motions(&self, explode: &ExplodeState) -> BTreeMap<BodyUid, Transform> {
        explode
            .part_offsets()
            .into_iter()
            .filter_map(|(part, offset)| match part {
                ExplodePart::Body(body) if !offset.is_identity() => Some((body, offset)),
                _ => None,
            })
            .collect()
    }

    /// Auto Explode's steps: the chosen occurrences (paths from the
    /// component; its top-level ones when none are given), each in a step
    /// of its own, out from the centre of all the component places: along
    /// the axis of a slider or cylindrical joint that lets it slide, else
    /// along the main axis (x, y or z of the component) of the line from
    /// that centre to its own; each by `factor` times its own size (its
    /// box's diagonal); the outermost first, inner parts last. A part at
    /// the centre with no joint to slide on stays.
    pub fn auto_explode_steps(
        &self,
        component: ComponentUid,
        occurrences: Option<Vec<OccurrencePath>>,
        factor: Option<f64>,
    ) -> Result<Vec<ExplodeStep>, ModelError> {
        let factor = factor.unwrap_or(DEFAULT_AUTO_FACTOR);
        if !(factor.is_finite() && factor > 0.0) {
            return Err(invalid("the factor must be a number more than 0"));
        }
        let a = &self.state.assembly;
        if !a.exists(component) {
            return Err(invalid("the component is gone"));
        }
        let chosen: Vec<Vec<OccurrenceUid>> = match occurrences {
            Some(list) => {
                for p in &list {
                    if p.is_empty() {
                        return Err(invalid(
                            "the component itself is not an occurrence it can move",
                        ));
                    }
                    path_component(a, component, &p.0).map_err(invalid)?;
                }
                list.into_iter().map(|p| p.0).collect()
            }
            None => a
                .children(component)
                .filter(|o| o.pattern.is_none() || self.result.placements.contains_key(&o.uid))
                .map(|o| vec![o.uid])
                .collect(),
        };
        if chosen.is_empty() {
            return Err(invalid(format!(
                "{} places no components to explode",
                a.name(component)
            )));
        }
        let below = self.bodies_below(component);
        let mut boxes = HashMap::new();
        let whole = self.box_of(&below, |_| true, &mut boxes);
        let (centre, overall) =
            whole.map_or(([0.0; 3], 0.0), |(min, max)| centre_and_size(min, max));
        let tiny = 1e-6 * overall.max(1.0);
        let mut planned: Vec<(f64, usize, ExplodeStep)> = Vec::new();
        for (order, path) in chosen.iter().enumerate() {
            let part = ExplodePart::Occurrence(path.clone());
            let (own_centre, size) = match self.part_box(&below, &part, &mut boxes) {
                Some((min, max)) => centre_and_size(min, max),
                None => (self.placement_below(component, path).translation, 0.0),
            };
            let out_from_centre = sub(own_centre, centre);
            let reach = norm(out_from_centre);
            let direction = match self.slide_direction(component, path) {
                Some(u) => Some(if dot(u, out_from_centre) < 0.0 {
                    scale(u, -1.0)
                } else {
                    u
                }),
                None if reach > tiny => {
                    let main = (0..3)
                        .max_by(|i, j| {
                            out_from_centre[*i]
                                .abs()
                                .total_cmp(&out_from_centre[*j].abs())
                        })
                        .expect("three axes");
                    let mut u = [0.0; 3];
                    u[main] = out_from_centre[main].signum();
                    Some(u)
                }
                None => None,
            };
            let Some(direction) = direction else {
                continue;
            };
            let distance = factor
                * if size > tiny {
                    size
                } else {
                    overall.max(1.0) * 0.25
                };
            let mut step = ExplodeStep::moving(
                vec![OccurrencePath(path.clone())],
                &value_text(distance, "mm"),
                direction.map(|v| (v * 1e9).round() / 1e9),
            );
            step.name = a.path_name(path);
            planned.push((reach, order, step));
        }
        if planned.is_empty() {
            return Err(invalid(
                "every chosen part is at the assembly's center with no joint to slide on",
            ));
        }
        // The outermost first, inner parts last.
        planned.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)));
        Ok(planned.into_iter().map(|(_, _, s)| s).collect())
    }

    /// The direction a slider or cylindrical joint of `component` lets an
    /// occurrence slide in (one side of the joint in it, the other not), in
    /// the component's coordinates. A grounded occurrence does not slide:
    /// the other side slides off it.
    fn slide_direction(&self, component: ComponentUid, path: &[OccurrenceUid]) -> Option<Vec3> {
        let a = &self.state.assembly;
        if path
            .iter()
            .any(|o| a.occurrence(*o).is_some_and(|o| o.grounded))
        {
            return None;
        }
        self.result.joints.joints.values().find_map(|j| {
            if j.component != component
                || !matches!(j.kind, JointKind::Slider | JointKind::Cylindrical)
            {
                return None;
            }
            let in_a = j.a.starts_with(path);
            let in_b = j.b.starts_with(path);
            let frame = match (in_a, in_b) {
                (true, false) => &j.frame_a,
                (false, true) => &j.frame_b,
                _ => return None,
            };
            let local = match (j.kind, j.slide) {
                (JointKind::Slider, SlideAxis::X) => [1.0, 0.0, 0.0],
                (JointKind::Slider, SlideAxis::Y) => [0.0, 1.0, 0.0],
                _ => [0.0, 0.0, 1.0],
            };
            unit(frame.apply_vector(local))
        })
    }

    /// Auto Explode: a view's steps made by [`Document::auto_explode_steps`]
    /// in place of its steps (after them with `append`), or a new view
    /// (named, or `Explode<n>`) of `component` with them. Returns the
    /// view's name. One undo step `Auto Explode`.
    pub fn auto_explode(
        &mut self,
        name: Option<&str>,
        component: ComponentUid,
        occurrences: Option<Vec<OccurrencePath>>,
        factor: Option<f64>,
        append: bool,
    ) -> Result<String, ModelError> {
        let existing = name.and_then(|n| self.explode_index(n).ok());
        let component = existing.map_or(component, |i| self.state.explode_views[i].component);
        let steps = self.auto_explode_steps(component, occurrences, factor)?;
        let (index, mut view) = match existing {
            Some(i) => (Some(i), self.state.explode_views[i].clone()),
            None => {
                let name = name
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map_or_else(|| self.free_explode_name(), str::to_owned);
                (None, ExplodeView::new(name, component))
            }
        };
        if append {
            view.steps.extend(steps);
        } else {
            view.steps = steps;
        }
        self.check_explode_view(&view)?;
        let name = view.name.clone();
        let label = format!("Auto Explode {name}");
        self.apply_with(|state| {
            match index {
                Some(i) => state.explode_views[i] = view,
                None => state.explode_views.push(view),
            }
            Ok((label, (), false))
        })?;
        Ok(name)
    }
}

#[cfg(test)]
mod tests {
    use super::rigid_inverse;
    use crate::transform::Transform;

    #[test]
    fn the_inverse_undoes_a_rigid_motion() {
        let t = Transform::translation([1.0, 2.0, 3.0])
            .after(&Transform::rotation([4.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.7).unwrap());
        let back = rigid_inverse(&t).after(&t);
        assert!(back.is_identity(), "{back:?}");
    }
}
