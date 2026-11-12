// SPDX-License-Identifier: MIT
//! Explicit assembly target scope for Extrude Cut (mitcad#200). Targets
//! retain their component-local shapes and ownership; instances are never
//! silently converted to independent geometry or shared-definition edits.
//! The extrusion's own component is the exception (mitcad#769): it is cut
//! in its own definition, as its other features are, and every occurrence
//! shows the result.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    BodyChange, CheckContext, Env, EvalContext, FeatureEntry, FeatureOutput, OccurrencePath,
};
use crate::assembly::{Assembly, inverse};
use crate::fingerprint::Fingerprint;
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::{BooleanOp, BooleanOutput, Kernel, KernelError};
use crate::recompute::Read;
use crate::transform::Transform;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentTarget {
    pub component: ComponentUid,
    pub occurrence: OccurrencePath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyTarget {
    pub component: ComponentUid,
    pub occurrence: OccurrencePath,
    pub body: BodyUid,
}

impl BodyTarget {
    fn target(&self) -> ComponentTarget {
        ComponentTarget {
            component: self.component,
            occurrence: self.occurrence.clone(),
        }
    }
}

/// One thing a scope leaves alone: an excluded component at its path, or
/// an excluded body (mitcad#1236).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Exclusion {
    Component(ComponentTarget),
    Body(BodyTarget),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetScope {
    Automatic {
        #[serde(default)]
        excluded: Vec<ComponentTarget>,
        #[serde(default)]
        excluded_bodies: Vec<BodyTarget>,
    },
    Explicit {
        included: Vec<ComponentTarget>,
        #[serde(default)]
        excluded_bodies: Vec<BodyTarget>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutScope {
    pub context: ComponentTarget,
    pub targets: TargetScope,
    /// A modern boss has not chosen Cut targets yet; the editor samples
    /// visibility on conversion instead of freezing it at boss creation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub initialize_on_cut: bool,
}

/// What a stored place of a [`CutScope`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaceKind {
    Context,
    /// An excluded or included component.
    Component,
    /// An excluded body.
    Body(BodyUid),
}

/// One stored place of a [`CutScope`], to be rewritten in place.
pub(crate) struct PlaceMut<'a> {
    pub kind: PlaceKind,
    pub component: &'a mut ComponentUid,
    pub occurrence: &'a mut OccurrencePath,
}

/// Features of one component moved into another (Move to Component,
/// Move History into Component; mitcad#859).
pub(crate) struct ComponentMove<'a> {
    pub from: ComponentUid,
    pub to: ComponentUid,
    /// Where a path from the root that places `from` places `to` now.
    pub path: &'a dyn Fn(&OccurrencePath) -> OccurrencePath,
    /// The bodies that went into `to`.
    pub bodies: &'a [BodyUid],
    /// `from` still has bodies after the move.
    pub keeps: bool,
}

/// How an extrusion relates to a [`ComponentMove`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MovedFeature {
    /// It went into `to` with the features.
    Moved,
    /// It stays in `from`: its own targets stay its own.
    StaysInSource,
    /// It is a feature of another component.
    Elsewhere,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutBody {
    pub body: BodyUid,
    pub name: String,
    pub checked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutTargetRow {
    pub component: ComponentUid,
    pub occurrence: OccurrencePath,
    pub name: String,
    pub path_name: String,
    pub intersects: bool,
    pub eligible: bool,
    pub checked: bool,
    pub partial: bool,
    pub reason: Option<String>,
    /// What an eligible row changes beyond the occurrence shown: the
    /// extrusion's own component with several occurrences (mitcad#769).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub bodies: Vec<CutBody>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutTargets {
    pub mode: String,
    pub rows: Vec<CutTargetRow>,
    pub selected_components: usize,
    pub selected_bodies: usize,
    pub can_commit: bool,
    /// The cut reaches no body, but would with its direction flipped
    /// (mitcad#1676): the panel suggests Flip. Left out when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flip_would_cut: bool,
}

impl CutScope {
    /// Feature replay currently applies tools in their owning component.
    /// Preserve that boundary rather than localizing an assembly selection.
    pub(crate) fn check_local_replay(&self) -> Result<(), String> {
        if matches!(&self.targets, TargetScope::Explicit { included, .. }
            if included.iter().any(|target| target != &self.context))
        {
            return Err(
                "unsupported cut scope for feature replay: cross-component or occurrence targets"
                    .to_owned(),
            );
        }
        Ok(())
    }

    /// Resolve an ordinary local cut's selection at the replay point. An
    /// automatic scope is local only while all selected body definitions
    /// belong to its unique context. These assembly and body reads also
    /// invalidate a cached replay when another target becomes available.
    pub(crate) fn local_replay_bodies<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<Vec<BodyUid>, String> {
        self.check_local_replay()?;
        let discovery = self.discover(ctx)?;
        let mut bodies = Vec::new();
        for candidate in discovery.candidates {
            if !self.checked(&candidate.target) {
                continue;
            }
            if candidate.target == self.context {
                if let Some(reason) = candidate.row.reason {
                    return Err(format!(
                        "unsupported cut scope for feature replay: {reason}"
                    ));
                }
                bodies.extend(candidate.shapes.into_iter().filter_map(|(body, shape)| {
                    (!ctx.is_mesh(&shape)
                        && !self.excluded_bodies().iter().any(|excluded| {
                            excluded.body == body && excluded.target() == self.context
                        }))
                    .then_some(body)
                }));
            } else if candidate.row.eligible
                && candidate.shapes.iter().any(|(body, shape)| {
                    !ctx.is_mesh(shape)
                        && !self.excluded_bodies().iter().any(|excluded| {
                            excluded.body == *body && excluded.target() == candidate.target
                        })
                })
            {
                return Err("unsupported cut scope for feature replay: automatic scope includes another component or occurrence".to_owned());
            }
        }
        // The shared tool application treats an empty list as all bodies.
        // A deliberate empty selection must therefore fail before it.
        if bodies.is_empty() {
            return Err("no eligible local bodies selected for cut feature replay".to_owned());
        }
        Ok(bodies)
    }

    pub fn stored_targets(&self) -> &[ComponentTarget] {
        match &self.targets {
            TargetScope::Automatic { excluded, .. } => excluded,
            TargetScope::Explicit { included, .. } => included,
        }
    }

    pub fn excluded_bodies(&self) -> &[BodyTarget] {
        match &self.targets {
            TargetScope::Automatic {
                excluded_bodies, ..
            }
            | TargetScope::Explicit {
                excluded_bodies, ..
            } => excluded_bodies,
        }
    }

    pub(crate) fn paths(&self) -> Vec<&OccurrencePath> {
        std::iter::once(&self.context.occurrence)
            .chain(self.stored_targets().iter().map(|t| &t.occurrence))
            .chain(self.excluded_bodies().iter().map(|t| &t.occurrence))
            .collect()
    }

    /// The components an explicit scope cuts by name; an automatic scope
    /// names none, it cuts what it finds.
    pub(crate) fn included(&self) -> &[ComponentTarget] {
        match &self.targets {
            TargetScope::Automatic { .. } => &[],
            TargetScope::Explicit { included, .. } => included,
        }
    }

    /// The paths the extrusion needs: its context and an explicit scope's
    /// included components. Exclusions name what the cut leaves alone and
    /// need nothing (mitcad#1236).
    pub(crate) fn reference_paths(&self) -> Vec<&OccurrencePath> {
        std::iter::once(&self.context.occurrence)
            .chain(self.included().iter().map(|t| &t.occurrence))
            .collect()
    }

    /// Its exclusions: an automatic scope's excluded components, then the
    /// excluded bodies.
    pub(crate) fn exclusions(&self) -> Vec<Exclusion> {
        let components: &[ComponentTarget] = match &self.targets {
            TargetScope::Automatic { excluded, .. } => excluded,
            TargetScope::Explicit { .. } => &[],
        };
        components
            .iter()
            .cloned()
            .map(Exclusion::Component)
            .chain(self.excluded_bodies().iter().cloned().map(Exclusion::Body))
            .collect()
    }

    /// Drops the exclusions of what is no longer in the design
    /// (mitcad#1236): a component excluded at a path that no longer places
    /// it, a body whose maker is gone (`made` false) or whose path no longer
    /// places its component. Returns the dropped ones.
    pub(crate) fn drop_gone_exclusions(
        &mut self,
        assembly: &Assembly,
        made: &dyn Fn(BodyUid) -> bool,
    ) -> Vec<Exclusion> {
        let placed = |component: ComponentUid, path: &OccurrencePath| {
            crate::joints::path_component(assembly, ComponentUid::ROOT, &path.0)
                .is_ok_and(|c| c == component)
        };
        let mut dropped = Vec::new();
        let (components, bodies) = match &mut self.targets {
            TargetScope::Automatic {
                excluded,
                excluded_bodies,
            } => (Some(excluded), excluded_bodies),
            TargetScope::Explicit {
                excluded_bodies, ..
            } => (None, excluded_bodies),
        };
        if let Some(components) = components {
            components.retain(|t| {
                let kept = placed(t.component, &t.occurrence);
                if !kept {
                    dropped.push(Exclusion::Component(t.clone()));
                }
                kept
            });
        }
        bodies.retain(|b| {
            let kept = made(b.body) && placed(b.component, &b.occurrence);
            if !kept {
                dropped.push(Exclusion::Body(b.clone()));
            }
            kept
        });
        dropped
    }

    /// Every stored place, the context first, for the edits that move
    /// them: Move to Component and moving an occurrence to another parent.
    pub(crate) fn places_mut(&mut self) -> Vec<PlaceMut<'_>> {
        let mut places = vec![PlaceMut {
            kind: PlaceKind::Context,
            component: &mut self.context.component,
            occurrence: &mut self.context.occurrence,
        }];
        let (targets, bodies) = match &mut self.targets {
            TargetScope::Automatic {
                excluded,
                excluded_bodies,
            } => (excluded, excluded_bodies),
            TargetScope::Explicit {
                included,
                excluded_bodies,
            } => (included, excluded_bodies),
        };
        places.extend(targets.iter_mut().map(|t| PlaceMut {
            kind: PlaceKind::Component,
            component: &mut t.component,
            occurrence: &mut t.occurrence,
        }));
        places.extend(bodies.iter_mut().map(|t| PlaceMut {
            kind: PlaceKind::Body(t.body),
            component: &mut t.component,
            occurrence: &mut t.occurrence,
        }));
        places
    }

    /// Follows features moved out of `m.from` into `m.to` (mitcad#859).
    /// A moved extrusion's context and its targets of `m.from` go with it;
    /// so do body targets of the bodies that moved, whoever stores them.
    /// What `m.from` keeps stays named by its old place: an automatic
    /// scope's exclusion of it, and another component's cut into it,
    /// cover both places then. True when anything changed.
    pub(crate) fn follow_move(&mut self, m: &ComponentMove<'_>, feature: MovedFeature) -> bool {
        let automatic = matches!(self.targets, TargetScope::Automatic { .. });
        let mut kept = Vec::new();
        let mut changed = false;
        for place in self.places_mut() {
            if *place.component != m.from {
                continue;
            }
            let follows = match (place.kind, feature) {
                (PlaceKind::Context, MovedFeature::Moved) => true,
                (PlaceKind::Context, _) => false,
                (PlaceKind::Body(body), _) => m.bodies.contains(&body),
                (PlaceKind::Component, MovedFeature::StaysInSource) => false,
                (PlaceKind::Component, MovedFeature::Moved | MovedFeature::Elsewhere) => {
                    let both = matches!(feature, MovedFeature::Elsewhere) || automatic;
                    if m.keeps && both {
                        kept.push(ComponentTarget {
                            component: *place.component,
                            occurrence: place.occurrence.clone(),
                        });
                    }
                    true
                }
            };
            if follows {
                *place.occurrence = (m.path)(place.occurrence);
                *place.component = m.to;
                changed = true;
            }
        }
        let targets = match &mut self.targets {
            TargetScope::Automatic { excluded, .. } => excluded,
            TargetScope::Explicit { included, .. } => included,
        };
        for target in kept {
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        changed
    }

    /// Checks the scope's own consistency. An excluded body need not be
    /// made before the extrusion (mitcad#1236): the scope refers to nothing
    /// it leaves alone.
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.initialize_on_cut {
            return Err("cut_scope.initialize_on_cut must be resolved before Cut".to_owned());
        }
        let targets = self.stored_targets();
        for (i, target) in targets.iter().enumerate() {
            if targets[..i].contains(target) {
                return Err("a cut-scope component is listed more than once".to_owned());
            }
        }
        for (i, target) in self.excluded_bodies().iter().enumerate() {
            if self.excluded_bodies()[..i].contains(target) {
                return Err("a cut-scope body is listed more than once".to_owned());
            }
        }
        Ok(())
    }

    pub(crate) fn check_components(
        &self,
        assembly: &Assembly,
        ctx: &CheckContext<'_>,
        component: ComponentUid,
    ) -> Result<(), String> {
        if self.context.component != component {
            return Err("cut_scope.context must name the extrusion's owning component".to_owned());
        }
        // Stored component identities also depend on their timeline creators.
        // Missing paths are left to recompute, so a failed cut can be repaired
        // after save/reopen instead of silently acquiring a different target.
        // Exclusions are no such identities (mitcad#1236).
        for component in std::iter::once(self.context.component)
            .chain(self.included().iter().map(|t| t.component))
        {
            if let Some(creator) = assembly.component(component).and_then(|c| c.created_by) {
                ctx.feature(creator)?;
            }
        }
        Ok(())
    }

    /// An explicit selection depends on its components' earlier geometry
    /// and makers, the context on its maker. Automatic discovery is a
    /// timeline-state read, and an exclusion names what the cut leaves
    /// alone (mitcad#1236): neither depends on anything.
    pub(crate) fn depends_on(&self, assembly: &Assembly, earlier: &FeatureEntry) -> bool {
        self.included()
            .iter()
            .any(|t| t.component == earlier.component)
            || std::iter::once(self.context.component)
                .chain(self.included().iter().map(|t| t.component))
                .any(|component| {
                    assembly
                        .component(component)
                        .is_some_and(|c| c.created_by == Some(earlier.uid))
                })
    }

    fn checked(&self, target: &ComponentTarget) -> bool {
        match &self.targets {
            TargetScope::Automatic { excluded, .. } => !excluded.contains(target),
            TargetScope::Explicit { included, .. } => included.contains(target),
        }
    }

    fn mode(&self) -> &'static str {
        match self.targets {
            TargetScope::Automatic { .. } => "automatic",
            TargetScope::Explicit { .. } => "explicit",
        }
    }

    fn report(&self) -> CutTargets {
        CutTargets {
            mode: self.mode().to_owned(),
            rows: Vec::new(),
            selected_components: 0,
            selected_bodies: 0,
            can_commit: false,
            flip_would_cut: false,
        }
    }
}

/// What an automatic scope leaves alone that moves against the cut's own
/// occurrence through joints (mitcad#1667), as its row's reason.
pub(crate) const MOVES_THROUGH_JOINTS: &str =
    "Moves relative to this component through joints; use Select none, then check it to cut it.";

/// Which occurrences the design's joints let move against each other
/// (mitcad#1667). A unit is what the joint solver moves as one: an
/// occurrence placed in a joint's component with those that rigid joints
/// and rigid groups join to it. A unit moves when a joint with free motions
/// (any kind but rigid) connects it and nothing holds it: no member is
/// grounded or joined rigidly to its component's own geometry. Occurrences
/// no joint names are units of their own that do not move. Every joint,
/// as-built joint and rigid group of the design that is not suppressed
/// counts, wherever it is in the timeline: they say how the design moves,
/// not what it looks like at one point. Assembly constraint sets are left
/// out (how much they hold is the solver's to say).
pub(crate) struct JointMotion {
    /// Each named occurrence's unit (an occurrence standing for it).
    unit: BTreeMap<OccurrenceUid, OccurrenceUid>,
    /// The units that move.
    moving: BTreeSet<OccurrenceUid>,
}

impl JointMotion {
    pub(crate) fn of(features: &[Arc<FeatureEntry>], assembly: &Assembly) -> Self {
        use crate::features::FeatureDef;
        use crate::joints::JointKind;
        // Union-find over the occurrences the joints name.
        let mut parent: BTreeMap<OccurrenceUid, OccurrenceUid> = BTreeMap::new();
        fn root(
            parent: &mut BTreeMap<OccurrenceUid, OccurrenceUid>,
            o: OccurrenceUid,
        ) -> OccurrenceUid {
            let mut r = *parent.entry(o).or_insert(o);
            while parent[&r] != r {
                r = parent[&r];
            }
            parent.insert(o, r);
            r
        }
        fn join(
            parent: &mut BTreeMap<OccurrenceUid, OccurrenceUid>,
            a: OccurrenceUid,
            b: OccurrenceUid,
        ) {
            let (ra, rb) = (root(parent, a), root(parent, b));
            if ra != rb {
                parent.insert(ra.max(rb), ra.min(rb));
            }
        }
        // The occurrences connected by joints with free motions, and those
        // held by their component's own geometry.
        let mut connected = Vec::new();
        let mut held = Vec::new();
        for f in features.iter().filter(|f| !f.suppressed) {
            // The solver's body of a side: the path's first occurrence (in
            // the joint's component); none is the component's own geometry.
            let (kind, a, b) = match &f.def {
                FeatureDef::Joint(j) => (
                    j.kind,
                    j.a.occurrence.0.first().copied(),
                    j.b.occurrence.0.first().copied(),
                ),
                FeatureDef::AsBuiltJoint(j) => {
                    (j.kind, j.a.0.first().copied(), j.b.0.first().copied())
                }
                FeatureDef::RigidGroup(g) => {
                    for pair in g.occurrences.windows(2) {
                        join(&mut parent, pair[0], pair[1]);
                    }
                    if let Some(first) = g.occurrences.first() {
                        root(&mut parent, *first);
                    }
                    continue;
                }
                _ => continue,
            };
            match (kind, a, b) {
                (JointKind::Rigid, Some(a), Some(b)) => join(&mut parent, a, b),
                (JointKind::Rigid, Some(o), None) | (JointKind::Rigid, None, Some(o)) => {
                    root(&mut parent, o);
                    held.push(o);
                }
                (JointKind::Rigid, None, None) => {}
                (_, a, b) => {
                    for o in [a, b].into_iter().flatten() {
                        root(&mut parent, o);
                        connected.push(o);
                    }
                }
            }
        }
        let named: Vec<_> = parent.keys().copied().collect();
        for o in &named {
            if assembly.occurrence(*o).is_some_and(|o| o.grounded) {
                held.push(*o);
            }
        }
        let held: BTreeSet<_> = held.into_iter().map(|o| root(&mut parent, o)).collect();
        let moving = connected
            .into_iter()
            .map(|o| root(&mut parent, o))
            .filter(|r| !held.contains(r))
            .collect();
        let unit = named
            .into_iter()
            .map(|o| (o, root(&mut parent, o)))
            .collect();
        Self { unit, moving }
    }

    fn unit(&self, o: OccurrenceUid) -> OccurrenceUid {
        self.unit.get(&o).copied().unwrap_or(o)
    }

    fn moves(&self, o: OccurrenceUid) -> bool {
        self.moving.contains(&self.unit(o))
    }

    /// Whether the bodies at absolute path `a` move against those at `b`
    /// as the joints turn and slide: below the occurrences the paths share,
    /// the first occurrences of each are different units of which one
    /// moves, or an occurrence deeper in either path moves inside it.
    pub(crate) fn between(&self, a: &[OccurrenceUid], b: &[OccurrenceUid]) -> bool {
        let shared = a.iter().zip(b).take_while(|(x, y)| x == y).count();
        let (a, b) = (&a[shared..], &b[shared..]);
        let top = match (a.first(), b.first()) {
            (Some(x), Some(y)) => {
                self.unit(*x) != self.unit(*y) && (self.moves(*x) || self.moves(*y))
            }
            (Some(x), None) | (None, Some(x)) => self.moves(*x),
            (None, None) => false,
        };
        // (The first occurrence of each tail is weighed above.)
        top || a
            .iter()
            .skip(1)
            .chain(b.iter().skip(1))
            .any(|o| self.moves(*o))
    }

    /// Writes what decides [`Self::between`] to a cache key.
    fn fingerprint(&self, key: &mut Fingerprint) {
        key.u64(self.unit.len() as u64);
        for (o, unit) in &self.unit {
            key.u32(o.0).u32(unit.0).bool(self.moving.contains(unit));
        }
    }
}

/// Cache input for structural changes which shape and placement reads alone
/// cannot detect (for example a hidden second occurrence or a new linked flag).
pub(crate) fn assembly_key<S>(env: &Env<'_, S>) -> u128 {
    let a = env.assembly;
    let mut key = Fingerprint::new("extrude-cut-assembly-v2");
    key.str(&a.root_name).u64(a.components.len() as u64);
    for c in &a.components {
        key.u32(c.uid.0)
            .str(&c.name)
            .bool(c.link.is_some())
            .option(c.created_by, |k, uid| {
                k.u64(uid.0);
            });
    }
    key.u64(a.occurrences.len() as u64);
    for o in &a.occurrences {
        key.u32(o.uid.0)
            .u32(o.parent.0)
            .u32(o.component.0)
            .u32(o.number)
            .bool(env.placements.contains_key(&o.uid));
    }
    // What moves against what through joints (mitcad#1667).
    JointMotion::of(env.features, a).fingerprint(&mut key);
    key.finish()
}

fn resolve<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    target: &ComponentTarget,
) -> Result<Transform, String> {
    let actual =
        crate::joints::path_component(ctx.env.assembly, ComponentUid::ROOT, &target.occurrence.0)?;
    if actual != target.component {
        return Err(format!(
            "occurrence {} no longer places component {}",
            target.occurrence, target.component
        ));
    }
    let mut transform = Transform::IDENTITY;
    for uid in &target.occurrence.0 {
        let placed = ctx.placement(*uid);
        if !ctx.env.placements.contains_key(uid) {
            return Err(format!(
                "occurrence {uid} is unavailable at this timeline position"
            ));
        }
        transform = transform.after(&placed);
    }
    Ok(transform)
}

/// The target's path places its component in the design, but not at this
/// timeline position: an occurrence on it is not placed yet.
fn unplaced<K: Kernel>(ctx: &EvalContext<'_, K>, target: &ComponentTarget) -> bool {
    crate::joints::path_component(ctx.env.assembly, ComponentUid::ROOT, &target.occurrence.0)
        .is_ok_and(|actual| actual == target.component)
        && target
            .occurrence
            .0
            .iter()
            .any(|uid| !ctx.env.placements.contains_key(uid))
}

struct Candidate<S> {
    target: ComponentTarget,
    world: Transform,
    shapes: Vec<(BodyUid, S)>,
    row: CutTargetRow,
}

pub(crate) struct Discovery<S> {
    source_world: Transform,
    candidates: Vec<Candidate<S>>,
}

impl<S: Clone> Discovery<S> {
    /// All available candidates, including unchecked and disabled rows: Through
    /// All does not shrink when the user excludes the farthest component.
    pub(crate) fn in_source_frame<K: Kernel<Shape = S>>(
        &self,
        ctx: &EvalContext<'_, K>,
    ) -> Result<Vec<(BodyUid, S)>, String> {
        let local = inverse(&self.source_world);
        let mut bodies = Vec::new();
        for c in &self.candidates {
            let transform = local.after(&c.world);
            for (uid, shape) in &c.shapes {
                bodies.push((
                    *uid,
                    ctx.kernel
                        .transform_shape(shape, &transform, None)
                        .map_err(|e| e.to_string())?,
                ));
            }
        }
        Ok(bodies)
    }
}

fn row(assembly: &Assembly, target: &ComponentTarget, reason: Option<String>) -> CutTargetRow {
    CutTargetRow {
        component: target.component,
        occurrence: target.occurrence.clone(),
        name: assembly.name(target.component),
        path_name: if target.occurrence.is_empty() {
            assembly.root_name.clone()
        } else {
            format!(
                "{} / {}",
                assembly.root_name,
                assembly.path_name(&target.occurrence.0)
            )
        },
        intersects: false,
        eligible: reason.is_none(),
        checked: false,
        partial: false,
        reason,
        note: None,
        bodies: Vec::new(),
    }
}

impl CutScope {
    pub(crate) fn discover<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<Discovery<K::Shape>, String> {
        ctx.reads.push(Read::Assembly(assembly_key(&ctx.env)));
        // Missing path reads participate in failure-cache invalidation too.
        for path in self.paths() {
            for uid in &path.0 {
                ctx.placement(*uid);
            }
        }
        if self.context.component != ctx.component() {
            return Err("cut_scope.context must name the extrusion's owning component".to_owned());
        }
        let source_world = resolve(ctx, &self.context)?;
        let mut effective = ctx.env.assembly.clone();
        effective
            .occurrences
            .retain(|o| ctx.env.placements.contains_key(&o.uid));
        let components: Vec<_> = std::iter::once(ComponentUid::ROOT)
            .chain(effective.components.iter().map(|c| c.uid))
            .collect();
        // An automatic scope leaves alone what moves against the context
        // through joints (mitcad#1667): its bodies would change with the
        // mechanism's pose. An explicit scope cuts what the user chose.
        let automatic = matches!(self.targets, TargetScope::Automatic { .. });
        let motion = automatic.then(|| JointMotion::of(ctx.env.features, ctx.env.assembly));
        let mut candidates = Vec::new();
        for component in components {
            let shapes = ctx.bodies_in(component);
            let all_paths = effective.paths_to(component);
            let occurrences = all_paths.len();
            // The extrusion's own component is cut in its own definition
            // (mitcad#769), as any of its features is: its bodies are one
            // candidate, in the context's frame, however many occurrences
            // place it, and the result shows in all of them. Only another
            // component's shared definition stays refused below.
            let own = component == self.context.component;
            let paths = if own {
                vec![self.context.occurrence.0.clone()]
            } else {
                all_paths
            };
            for path in &paths {
                let target = ComponentTarget {
                    component,
                    occurrence: OccurrencePath(path.clone()),
                };
                let readonly = effective
                    .component(component)
                    .is_some_and(|c| c.link.is_some())
                    || path
                        .iter()
                        .filter_map(|uid| effective.occurrence(*uid))
                        .any(|o| {
                            effective
                                .component(o.component)
                                .is_some_and(|c| c.link.is_some())
                        });
                let reason = if readonly {
                    Some(
                        "Linked component is read-only; edit the source design instead.".to_owned(),
                    )
                } else if paths.len() != 1 {
                    Some("Shared component definition has multiple occurrences; occurrence-specific cuts are not supported.".to_owned())
                } else if !own
                    && motion
                        .as_ref()
                        .is_some_and(|m| m.between(path, &self.context.occurrence.0))
                {
                    Some(MOVES_THROUGH_JOINTS.to_owned())
                } else {
                    None
                };
                let world = resolve(ctx, &target)?;
                let mut target_row = row(&effective, &target, reason);
                if own && occurrences > 1 {
                    target_row.note = Some(format!(
                        "Component being edited; the cut changes all {occurrences} occurrences."
                    ));
                }
                candidates.push(Candidate {
                    row: target_row,
                    target,
                    world,
                    shapes: shapes.clone(),
                });
            }
        }
        let mut report = self.report();
        let mut error = None;
        // Each stored place, and whether it is an exclusion.
        let automatic = matches!(self.targets, TargetScope::Automatic { .. });
        let stored: Vec<_> = self
            .stored_targets()
            .iter()
            .map(|t| (t.clone(), automatic))
            .chain(self.excluded_bodies().iter().map(|b| (b.target(), true)))
            .collect();
        for (target, exclusion) in stored {
            // An exclusion of an occurrence not placed at this timeline
            // position (its maker comes later or is suppressed) leaves
            // nothing alone here (mitcad#1236).
            if exclusion && unplaced(ctx, &target) {
                continue;
            }
            if let Err(message) = resolve(ctx, &target) {
                error.get_or_insert(message.clone());
                if !report
                    .rows
                    .iter()
                    .any(|r| r.component == target.component && r.occurrence == target.occurrence)
                {
                    report
                        .rows
                        .push(row(ctx.env.assembly, &target, Some(message)));
                }
            }
        }
        // The own component is one target, at the context occurrence
        // (mitcad#769); including it at another occurrence is no
        // occurrence-specific cut either.
        if let TargetScope::Explicit { included, .. } = &self.targets
            && let Some(target) = included
                .iter()
                .find(|t| t.component == self.context.component && **t != self.context)
        {
            let message = format!(
                "component {} is cut in its own definition; include it at the context occurrence {}",
                target.component, self.context.occurrence
            );
            error.get_or_insert(message.clone());
            report
                .rows
                .push(row(ctx.env.assembly, target, Some(message)));
        }
        for body in self.excluded_bodies() {
            // A body not made yet at this position, or consumed before it,
            // leaves nothing alone (mitcad#1236); one in another component
            // is a stale choice.
            if ctx
                .env
                .owners
                .get(&body.body)
                .is_some_and(|owner| *owner != body.component)
            {
                let message = format!(
                    "excluded body {} is no longer in component {}",
                    body.body, body.component
                );
                error.get_or_insert(message.clone());
                report
                    .rows
                    .push(row(ctx.env.assembly, &body.target(), Some(message)));
            }
        }
        if let Some(message) = error {
            ctx.cut_targets = Some(report);
            return Err(message);
        }
        Ok(Discovery {
            source_world,
            candidates,
        })
    }

    pub(crate) fn apply<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        discovery: Discovery<K::Shape>,
        tool: K::Shape,
    ) -> Result<FeatureOutput<K::Shape>, String> {
        let mut report = self.report();
        let mut output = FeatureOutput {
            tool: Some(tool.clone()),
            ..FeatureOutput::default()
        };
        let mut failed = None;
        for mut candidate in discovery.candidates {
            if matches!(&self.targets, TargetScope::Explicit { included, .. } if included.contains(&candidate.target))
                && let Some(reason) = &candidate.row.reason
            {
                failed.get_or_insert_with(|| format!("{}: {reason}", candidate.row.path_name));
            }
            let local_tool = ctx
                .kernel
                .transform_shape(
                    &tool,
                    &inverse(&candidate.world).after(&discovery.source_world),
                    None,
                )
                .map_err(|e| e.to_string())?;
            let tool_bounds = ctx
                .kernel
                .bounding_box(&local_tool)
                .map_err(|e| e.to_string())?;
            let requested = self.checked(&candidate.target) && candidate.row.eligible;
            let mut touched: Vec<(BodyUid, BooleanOutput<K::Shape>)> = Vec::new();
            let mut boolean_failed = false;
            for (uid, shape) in &candidate.shapes {
                // Mesh bodies are not cut (mitcad#399).
                if ctx.is_mesh(shape) {
                    continue;
                }
                let bounds = ctx.kernel.bounding_box(shape).map_err(|e| e.to_string())?;
                if let (Some(a), Some(b)) = (&tool_bounds, &bounds)
                    && !a.overlaps(b, 1e-7)
                {
                    continue;
                }
                match ctx.kernel.boolean(BooleanOp::Cut, &[shape], &local_tool) {
                    Ok(result) if result.touched.first() == Some(&true) => {
                        touched.push((*uid, result))
                    }
                    Ok(_) => {}
                    Err(error) => {
                        boolean_failed = true;
                        let detail = error.to_string();
                        if KernelError::is_out_of_memory(&detail)
                            || detail.contains("cancelled")
                            || detail.contains("canceled")
                        {
                            return Err(detail);
                        }
                        let message = format!("{}: {error}", candidate.row.path_name);
                        if requested
                            && !self
                                .excluded_bodies()
                                .iter()
                                .any(|b| b.body == *uid && b.target() == candidate.target)
                        {
                            failed.get_or_insert(message.clone());
                            candidate.row.eligible = false;
                        }
                        candidate.row.reason.get_or_insert(message);
                        // An excluded body's failed intersection check does
                        // not disable the other requested bodies in its row.
                        candidate.row.bodies.push(CutBody {
                            body: *uid,
                            name: uid.to_string(),
                            checked: false,
                        });
                    }
                }
            }
            candidate.row.intersects = !touched.is_empty();
            let selected = self.checked(&candidate.target) && candidate.row.eligible;
            for (body, result) in touched {
                let excluded = self
                    .excluded_bodies()
                    .iter()
                    .any(|b| b.body == body && b.target() == candidate.target);
                let checked = selected && !excluded;
                candidate.row.bodies.push(CutBody {
                    body,
                    name: body.to_string(),
                    checked,
                });
                if !checked {
                    continue;
                }
                report.selected_bodies += 1;
                let pieces: Vec<K::Shape> = result
                    .pieces
                    .into_iter()
                    .filter(|p| p.sources.contains(&0))
                    .map(|p| p.shape)
                    .collect();
                // The pieces' ids (mitcad#408); none left for the body
                // removes it.
                let named = ctx.piece_ids(body, pieces);
                if !named.iter().any(|(id, _)| *id == body) {
                    output.changes.push(BodyChange::Remove(body));
                }
                for (uid, piece) in named {
                    output.changes.push(BodyChange::Set(uid, piece));
                    output.owners.insert(uid, candidate.target.component);
                }
            }
            let checked_count = candidate.row.bodies.iter().filter(|b| b.checked).count();
            candidate.row.bodies.sort_by_key(|b| b.body);
            candidate.row.checked = checked_count > 0;
            candidate.row.partial =
                checked_count > 0 && checked_count != candidate.row.bodies.len();
            if candidate.row.checked {
                report.selected_components += 1;
            }
            if candidate.row.intersects
                || self.stored_targets().contains(&candidate.target)
                || self
                    .excluded_bodies()
                    .iter()
                    .any(|b| b.target() == candidate.target)
                || boolean_failed
            {
                report.rows.push(candidate.row);
            }
        }
        if report.selected_bodies == 0 {
            failed.get_or_insert_with(|| if matches!(&self.targets, TargetScope::Explicit { included, .. } if included.is_empty()) {
                "No components selected".to_owned()
            } else { NO_INTERSECTION.to_owned() });
        }
        report.can_commit = failed.is_none();
        ctx.cut_targets = Some(report);
        match failed {
            Some(message) => Err(message),
            None => Ok(output),
        }
    }

    /// Whether `tool` would cut a body this cut may take (mitcad#1676): a
    /// body of a checked, eligible component that is neither excluded nor
    /// a mesh. Nothing is reported or changed; a body whose intersection
    /// fails counts as not cut.
    pub(crate) fn would_cut<K: Kernel>(
        &self,
        ctx: &EvalContext<'_, K>,
        discovery: &Discovery<K::Shape>,
        tool: &K::Shape,
    ) -> Result<bool, String> {
        for candidate in &discovery.candidates {
            if !(self.checked(&candidate.target) && candidate.row.eligible) {
                continue;
            }
            let local_tool = ctx
                .kernel
                .transform_shape(
                    tool,
                    &inverse(&candidate.world).after(&discovery.source_world),
                    None,
                )
                .map_err(|e| e.to_string())?;
            let tool_bounds = ctx
                .kernel
                .bounding_box(&local_tool)
                .map_err(|e| e.to_string())?;
            for (uid, shape) in &candidate.shapes {
                if ctx.is_mesh(shape)
                    || self
                        .excluded_bodies()
                        .iter()
                        .any(|b| b.body == *uid && b.target() == candidate.target)
                {
                    continue;
                }
                let bounds = ctx.kernel.bounding_box(shape).map_err(|e| e.to_string())?;
                if let (Some(a), Some(b)) = (&tool_bounds, &bounds)
                    && !a.overlaps(b, 1e-7)
                {
                    continue;
                }
                match ctx.kernel.boolean(BooleanOp::Cut, &[shape], &local_tool) {
                    Ok(result) if result.touched.first() == Some(&true) => return Ok(true),
                    Ok(_) => {}
                    Err(error) => {
                        let detail = error.to_string();
                        if KernelError::is_out_of_memory(&detail)
                            || detail.contains("cancelled")
                            || detail.contains("canceled")
                        {
                            return Err(detail);
                        }
                    }
                }
            }
        }
        Ok(false)
    }
}

/// The failure of a cut that reaches no body it may take.
pub(crate) const NO_INTERSECTION: &str = "No eligible components intersect the cut";

/// [`NO_INTERSECTION`] when the cut flipped would reach one (mitcad#1676):
/// a cut drawn away from the body is the usual beginner's miss.
pub(crate) const NO_INTERSECTION_TRY_FLIP: &str =
    "No eligible components intersect the cut. Try Flip: the other direction cuts a component";
