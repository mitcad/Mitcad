// SPDX-License-Identifier: MIT
//! Moving an occurrence to another parent (mitcad#219): Move to... on a
//! component occurrence in the browser. The occurrence keeps its identity,
//! its component (with that component's features, bodies and the
//! occurrences placed in it), its number, light bulb and grounding; only
//! its parent and its own placement change, the placement so that it
//! stays where it is in the design at the timeline marker. Occurrence
//! paths that lead through it follow it in the same undo step: links of
//! sketch planes, projections and modelling features to other components,
//! cut scopes, Envelopes' sources (mitcad#1276), joints and assembly
//! constraints, section analyses,
//! Isolate, the model's annotations and the drawings' views, dimensions
//! and balloons (mitcad#743).
//!
//! The timeline features that place the occurrence (mitcad#301) go with
//! it in the same step: its joints, as-built joints, assembly constraint
//! rows, rigid groups, Moves and Capture Positions. Each becomes a feature
//! of the new parent, the one component where it can still place the
//! occurrence (a joint moves only occurrences placed directly in its
//! component), keeps its place in the timeline, and has its occurrence
//! paths rewritten relative to the new parent (`Body1` of the old parent
//! becomes `Box:1/Body1`). A Move or Capture Position is turned into the
//! new parent's coordinates. A feature that also places other occurrences
//! is split: the rows, positions or members of the moved occurrence go
//! into a new feature of the new parent right after it, the rest stay.
//! See [`Carry`].
//!
//! Several occurrences of one parent move together in one step
//! (mitcad#1847: the selected rows, or a rigid group's members): a feature
//! that places only moved occurrences (a rigid group of them, a joint
//! between two of them) goes into the new parent whole, as it is.
//!
//! [`Document::plan_reparent`] is the one place that decides whether a
//! move is allowed: the destination list, the preview and the command use
//! it (design policies, mitcad#220, add their checks there). It refuses,
//! naming the blocker, what the shared-definition model cannot represent
//! or what would change meaning:
//!
//! - the root, an occurrence made by Pattern at Points;
//! - a source parent or a destination placed more than once: the move
//!   would change every placement of that shared component;
//! - linked (read-only) parents and destinations;
//! - the current parent, and the occurrence's own component or anything
//!   placed in it (a cycle);
//! - joints and constraints whose moved occurrence would change, or whose
//!   component the occurrence would leave;
//! - Pattern at Points features and instances that refer to it;
//! - a component's drawing whose views or annotations lead through the
//!   occurrence when the occurrence would leave that component;
//! - references across the occurrence while a parent on the way from the
//!   old parent or to the new one is placed by a timeline feature: the
//!   relative placement could then differ at the reference's point of the
//!   timeline;
//! - a placing feature that cannot keep its meaning in the new parent: it
//!   also joins the occurrence to geometry that is not in the new parent
//!   (the new parent would have to be a component above it, where the
//!   feature would move another occurrence); a Move along the old parent's
//!   axes or by its geometry when the axes turn or the geometry is not the
//!   new parent's; a parametric Move of several occurrences (it cannot be
//!   split); and an occurrence it joins the moved one to, or a parent on
//!   the way, that a later feature moves, which the moved occurrence would
//!   no longer follow.
//!
//! The command computes the new state before taking it and refuses it
//! when a feature that built before would fail or an occurrence would be
//! somewhere else in the design at the marker.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::components::check_transform;
use super::{DocState, Document, Follow, ModelError, invalid, same_placement};
use crate::assembly::{Assembly, inverse, matrix_rows};
use crate::document::AnalysisDef;
use crate::drawing::def::Drawing;
use crate::features::extrude_scope::CutScope;
use crate::features::moves::MoveSpec;
use crate::features::{
    CapturePositionDef, ConstraintSetDef, FeatureDef, FeatureEntry, MoveOccurrenceDef,
    OccurrencePath, RigidGroupDef,
};
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;
use crate::print_plates::PlateTarget;
use crate::recompute::FeatureStatus;
use crate::transform::Transform;

/// A timeline feature that places the moved occurrence and goes with it
/// (mitcad#301).
#[derive(Debug, Clone, PartialEq)]
pub struct Carried {
    /// The feature's name.
    pub name: String,
    /// Its component, and the new parent it goes into.
    pub from: ComponentUid,
    pub to: ComponentUid,
    /// When the feature also places other occurrences: the new feature of
    /// the new parent that takes the moved occurrence's part of it.
    pub split: Option<String>,
}

/// One occurrence of a planned move.
#[derive(Debug, Clone)]
pub struct MovedOccurrence {
    pub occurrence: OccurrenceUid,
    /// The component it places.
    pub component: ComponentUid,
    /// Its path from the root before the move.
    pub source: Vec<OccurrenceUid>,
    /// Its placement in the destination: the same place in the design.
    pub transform: Transform,
}

/// A validated move of one occurrence, or of several of one parent
/// (mitcad#1847), to another parent.
#[derive(Debug, Clone)]
pub struct ReparentPlan {
    /// The occurrences that move, the first one first.
    pub moved: Vec<MovedOccurrence>,
    /// The component they are placed in before the move.
    pub parent: ComponentUid,
    /// The component they go into, and the path to that component.
    pub destination: ComponentUid,
    pub destination_path: Vec<OccurrenceUid>,
    /// What follows them: the features, analyses and Isolate whose
    /// occurrence paths lead through them.
    pub updates: Vec<String>,
    /// The features that place them and go with them, in timeline order.
    pub carried: Vec<Carried>,
    changes: Rewritten,
}

impl ReparentPlan {
    /// The first occurrence that moves.
    pub fn first(&self) -> &MovedOccurrence {
        &self.moved[0]
    }

    /// The first occurrence's path from the root after the move.
    pub fn path(&self) -> Vec<OccurrenceUid> {
        self.path_of(self.first())
    }

    /// An occurrence's path from the root after the move.
    pub fn path_of(&self, moved: &MovedOccurrence) -> Vec<OccurrenceUid> {
        let mut path = self.destination_path.clone();
        path.push(moved.occurrence);
        path
    }

    /// The parent's path from the root before the move.
    pub fn from_path(&self) -> &[OccurrenceUid] {
        let source = &self.first().source;
        &source[..source.len() - 1]
    }
}

/// The new references, kept apart until the move is taken.
#[derive(Debug, Clone, Default)]
struct Rewritten {
    features: Vec<(usize, FeatureDef)>,
    /// Section analyses' paths: the analysis, whether the path is its
    /// axis' (mitcad#823) rather than its plane's, and the new path.
    analyses: Vec<(usize, bool, OccurrencePath)>,
    isolated: Vec<(usize, String)>,
    /// Contact sets' occurrences (mitcad#454).
    contact_sets: Vec<(usize, Vec<OccurrencePath>)>,
    /// Annotations' picks (mitcad#649, mitcad#702): the annotation, which
    /// pick (0: `a`, 1: `b`, 2: its plane's) and its new occurrence path.
    measurements: Vec<(usize, usize, OccurrencePath)>,
    /// Drawings whose views and picks follow it (mitcad#743): the drawing
    /// and its new definition.
    drawings: Vec<(usize, Drawing)>,
    /// Placing features that go into the new parent whole.
    components: Vec<(usize, ComponentUid)>,
    /// New features of the new parent, each right after the feature at
    /// the index it was split from.
    inserted: Vec<(usize, FeatureEntry)>,
    /// Parameters of the parts that went into a new feature.
    owners: Vec<(ParamId, FeatureUid)>,
}

/// How occurrence paths through the moved occurrence follow it.
struct Rewrite<'a> {
    assembly: &'a Assembly,
    occurrence: OccurrenceUid,
    /// Its path from the root before the move.
    old: &'a [OccurrenceUid],
    /// And after.
    new: Vec<OccurrenceUid>,
    /// Why references across it cannot follow it, when they cannot.
    varying: Option<String>,
}

impl Rewrite<'_> {
    fn name(&self) -> String {
        self.assembly.occurrence_name(self.occurrence)
    }

    /// Whether a path from the root leads through the occurrence.
    fn through(&self, path: &[OccurrenceUid]) -> bool {
        path.len() >= self.old.len() && path[..self.old.len()] == *self.old
    }

    /// A path from the root through the occurrence goes its new way.
    fn absolute(&self, path: &mut Vec<OccurrenceUid>) -> bool {
        if !self.through(path) {
            return false;
        }
        let rest = path.split_off(self.old.len());
        path.clone_from(&self.new);
        path.extend(rest);
        true
    }

    /// Both paths of a link to another component's geometry.
    fn link(&self, link: &mut OccurrenceLink) -> Result<bool, String> {
        let source = self.through(&link.source.0);
        let target = self.through(&link.target.0);
        if source != target
            && let Some(why) = &self.varying
        {
            return Err(why.clone());
        }
        let moved = self.absolute(&mut link.source.0);
        Ok(self.absolute(&mut link.target.0) || moved)
    }

    /// A joint's or constraint's path, relative to its feature's component:
    /// its first occurrence is the one the joint moves, which must stay.
    fn unit(&self, path: &mut Vec<OccurrenceUid>) -> Result<bool, String> {
        let Some(k) = path.iter().position(|o| *o == self.occurrence) else {
            return Ok(false);
        };
        let n = self.old.len();
        if k >= n || path[..k] != self.old[n - 1 - k..n - 1] {
            // Not a path to it: recompute reports such a path.
            return Ok(false);
        }
        if k == 0 {
            return Err(format!(
                "it moves {} itself, which would leave its component",
                self.name()
            ));
        }
        // The feature's component is placed at old[..j]; old[j] is what it
        // moves.
        let j = n - 1 - k;
        let parents = &self.new[..self.new.len() - 1];
        if parents.len() <= j || parents[..=j] != self.old[..=j] {
            return Err(format!(
                "{} would no longer be in {}, the occurrence it moves",
                self.name(),
                self.assembly.occurrence_name(self.old[j])
            ));
        }
        if let Some(why) = &self.varying {
            return Err(why.clone());
        }
        let mut moved = parents[j..].to_vec();
        moved.push(self.occurrence);
        moved.extend_from_slice(&path[k + 1..]);
        *path = moved;
        Ok(true)
    }

    /// A path relative to its feature's component that only reads what it
    /// leads to (an Envelope's source, mitcad#1276): it follows the
    /// occurrence while the occurrence stays in that component.
    fn relative(&self, path: &mut Vec<OccurrenceUid>) -> Result<bool, String> {
        let Some(k) = path.iter().position(|o| *o == self.occurrence) else {
            return Ok(false);
        };
        let n = self.old.len();
        if k >= n || path[..k] != self.old[n - 1 - k..n - 1] {
            // Not a path to it: recompute reports such a path.
            return Ok(false);
        }
        // The feature's component is placed at old[..j].
        let j = n - 1 - k;
        let parents = &self.new[..self.new.len() - 1];
        if parents.len() < j || parents[..j] != self.old[..j] {
            return Err(format!(
                "{} would no longer be in the component it reads it in",
                self.name()
            ));
        }
        if let Some(why) = &self.varying {
            return Err(why.clone());
        }
        let mut moved = parents[j..].to_vec();
        moved.push(self.occurrence);
        moved.extend_from_slice(&path[k + 1..]);
        *path = moved;
        Ok(true)
    }

    /// A cut scope's paths (from the root).
    fn scope(&self, scope: &mut CutScope) -> Result<bool, String> {
        if !scope.paths().iter().any(|p| self.through(&p.0)) {
            return Ok(false);
        }
        if let Some(why) = &self.varying {
            return Err(why.clone());
        }
        for place in scope.places_mut() {
            self.absolute(&mut place.occurrence.0);
        }
        Ok(true)
    }

    /// A feature's references; Ok(true) when they changed, Err with the
    /// blocker when they cannot follow.
    fn feature(&self, def: &mut FeatureDef) -> Result<bool, String> {
        let o = self.occurrence;
        let other = crate::joints::referenced_occurrences(def).contains(&o);
        let mut changed = false;
        match def {
            FeatureDef::Sketch(s) => {
                for link in s
                    .plane_link
                    .iter_mut()
                    .chain(s.projections.iter_mut().filter_map(|p| p.link.as_mut()))
                {
                    changed |= self.link(link)?;
                }
            }
            FeatureDef::Derive(d) => {
                for link in d.links.values_mut() {
                    changed |= self.link(link)?;
                }
            }
            // A body transfer's source and destination (mitcad#465).
            FeatureDef::BodyTransfer(t) => {
                changed |= self.link(&mut t.link)?;
            }
            FeatureDef::Combine(c) => {
                for (_, link) in c.links_mut() {
                    changed |= self.link(link)?;
                }
            }
            FeatureDef::Sweep(s) => {
                for link in s.body_links.values_mut() {
                    changed |= self.link(link)?;
                }
            }
            FeatureDef::SplitBody(s) => {
                for link in s.tool_links.values_mut() {
                    changed |= self.link(link)?;
                }
            }
            FeatureDef::Revolve(r) => {
                for link in r.object_links.values_mut() {
                    changed |= self.link(link)?;
                }
            }
            FeatureDef::ConstructionPlane(p) => {
                for link in p.links.values_mut().chain(p.datum_links.values_mut()) {
                    changed |= self.link(link)?;
                }
            }
            FeatureDef::ConstructionAxis(a) => {
                for link in a.links.values_mut().chain(a.datum_links.values_mut()) {
                    changed |= self.link(link)?;
                }
            }
            FeatureDef::ConstructionPoint(p) => {
                for link in p.links.values_mut().chain(p.datum_links.values_mut()) {
                    changed |= self.link(link)?;
                }
            }
            // A Flex item's nodes on other components' geometry
            // (mitcad#1713).
            FeatureDef::Flex(f) => {
                for link in f.links.values_mut().chain(f.datum_links.values_mut()) {
                    changed |= self.link(link)?;
                }
            }
            // A seat on another component's shaft (mitcad#578).
            seated @ (FeatureDef::Pulley(_)
            | FeatureDef::Gear(_)
            | FeatureDef::PrintableBearing(_)) => {
                if let Some((_, link)) = seated.seat_link_mut() {
                    changed |= self.link(link)?;
                }
            }
            FeatureDef::Extrude(x) => {
                for link in x.object_links.values_mut() {
                    changed |= self.link(link)?;
                }
                if let Some(scope) = &mut x.cut_scope {
                    changed |= self.scope(scope)?;
                }
            }
            FeatureDef::Joint(j) => {
                changed |= self.unit(&mut j.a.occurrence.0)?;
                changed |= self.unit(&mut j.b.occurrence.0)?;
            }
            // BUILD's bound lengths (mitcad#375).
            FeatureDef::BoundLength(b) => {
                changed |= self.unit(&mut b.from.occurrence.0)?;
                changed |= self.unit(&mut b.to.occurrence.0)?;
            }
            // An edge frame's path (mitcad#1814).
            FeatureDef::EdgeFrame(e) => {
                changed |= self.unit(&mut e.occurrence.0)?;
            }
            FeatureDef::AsBuiltJoint(j) => {
                changed |= self.unit(&mut j.a.0)?;
                changed |= self.unit(&mut j.b.0)?;
                if let Some(origin) = &mut j.origin {
                    changed |= self.unit(&mut origin.occurrence.0)?;
                }
            }
            FeatureDef::ConstraintSet(set) => {
                for row in &mut set.rows {
                    for origin in [&mut row.a, &mut row.b]
                        .into_iter()
                        .chain(row.a2.as_mut())
                        .chain(row.b2.as_mut())
                    {
                        changed |= self.unit(&mut origin.occurrence.0)?;
                    }
                }
            }
            // An Envelope's source (mitcad#1276).
            FeatureDef::Envelope(e) => {
                changed |= self.relative(&mut e.occurrence.0)?;
            }
            FeatureDef::RigidGroup(g) if g.occurrences.contains(&o) => {
                return Err(format!(
                    "it holds {} rigidly to other occurrences of its component",
                    self.name()
                ));
            }
            FeatureDef::MoveOccurrence(m) if m.occurrences.contains(&o) => {
                return Err(format!("it moves {}", self.name()));
            }
            FeatureDef::CapturePosition(c) if c.positions.iter().any(|p| p.occurrence == o) => {
                return Err(format!("it captures the position of {}", self.name()));
            }
            FeatureDef::PatternAtPoints(p) if p.paths().iter().any(|path| path.0.contains(&o)) => {
                return Err(format!(
                    "Pattern at Points refers to {}; moving its occurrences is not supported",
                    self.name()
                ));
            }
            _ if other => {
                return Err(format!(
                    "it refers to {} in a way the move cannot update",
                    self.name()
                ));
            }
            _ => {}
        }
        Ok(changed)
    }
}

/// A path from the root through one of the moved occurrences goes its new
/// way (the paths through different siblings never meet).
fn absolute_all(rewrites: &[Rewrite<'_>], path: &mut Vec<OccurrenceUid>) -> bool {
    let mut moved = false;
    for rewrite in rewrites {
        moved |= rewrite.absolute(path);
    }
    moved
}

/// The paths of the document's state that lead through the moved
/// occurrences, as they become, with what follows and what blocks the move.
fn rewrite_references(
    state: &DocState,
    rewrites: &[Rewrite<'_>],
    carriers: &BTreeSet<usize>,
) -> (Rewritten, Vec<String>, Vec<String>) {
    let mut out = Rewritten::default();
    let mut updates = Vec::new();
    let mut blockers = Vec::new();
    for (i, entry) in state.features.iter().enumerate() {
        // The placing features go with them ([`Carry`]).
        if carriers.contains(&i) {
            continue;
        }
        let mut def = entry.def.clone();
        let mut changed = false;
        let mut blocked = false;
        for rewrite in rewrites {
            let o = rewrite.occurrence;
            // Dormant cut choices are editor metadata, not active occurrence
            // references, but their saved paths must follow an assembly move.
            let saved_cut_path = matches!(&def, FeatureDef::Extrude(extrude)
                if extrude.cut_scope.as_ref().is_some_and(|scope|
                    scope.paths().iter().any(|path| path.0.contains(&o))));
            if !saved_cut_path && !crate::joints::referenced_occurrences(&def).contains(&o) {
                continue;
            }
            match rewrite.feature(&mut def) {
                Ok(moved) => changed |= moved,
                Err(why) => {
                    blockers.push(format!("{}: {why}", entry.name));
                    blocked = true;
                    break;
                }
            }
        }
        if changed && !blocked {
            updates.push(entry.name.clone());
            out.features.push((i, def));
        }
    }
    for (i, analysis) in state.analyses.iter().enumerate() {
        match &analysis.def {
            AnalysisDef::Section(section) => {
                let mut changed = false;
                for (axis, path) in [
                    (false, &section.occurrence),
                    (true, &section.axis_occurrence),
                ] {
                    if let Some(path) = path {
                        let mut moved = path.0.clone();
                        if absolute_all(rewrites, &mut moved) {
                            changed = true;
                            out.analyses.push((i, axis, OccurrencePath(moved)));
                        }
                    }
                }
                if changed {
                    updates.push(analysis.name.clone());
                }
            }
            // Bodies and a bed face by uid and name: no occurrence path.
            AnalysisDef::Print(_) => {}
        }
    }
    for (i, item) in state.display.isolated.iter().enumerate() {
        let uids: Result<Vec<OccurrenceUid>, _> = item
            .occurrence
            .split('/')
            .filter(|part| !part.is_empty())
            .map(str::parse::<OccurrenceUid>)
            .collect();
        if let Ok(mut path) = uids
            && absolute_all(rewrites, &mut path)
        {
            out.isolated.push((i, OccurrencePath(path).to_string()));
        }
    }
    if !out.isolated.is_empty() {
        updates.push("Isolate".to_owned());
    }
    // Contact sets' occurrences follow it (mitcad#454).
    for (i, set) in state.contact_sets.iter().enumerate() {
        let mut moved = false;
        let mut paths = set.occurrences.clone();
        for path in &mut paths {
            moved |= absolute_all(rewrites, &mut path.0);
        }
        if moved {
            updates.push(set.name.clone());
            out.contact_sets.push((i, paths));
        }
    }
    // Annotations' picks follow it (mitcad#649, mitcad#702).
    for (i, measurement) in state.annotations.iter().enumerate() {
        let mut moved = false;
        let plane = measurement.plane.as_ref().and_then(|p| p.pick.as_ref());
        for (pick, path) in [Some(&measurement.a), measurement.b.as_ref(), plane]
            .into_iter()
            .enumerate()
            .filter_map(|(pick, p)| Some((pick, p?.occurrence.as_ref()?)))
        {
            let mut path = path.0.clone();
            if absolute_all(rewrites, &mut path) {
                moved = true;
                out.measurements.push((i, pick, OccurrencePath(path)));
            }
        }
        if moved {
            updates.push(measurement.name.clone());
        }
    }
    // Drawings' views and picks follow it (mitcad#743). Their paths are
    // from the drawing's component: the root's go the new way; a placed
    // component's follow while the occurrence stays inside it.
    for (i, drawing) in state.drawings.iter().enumerate() {
        let placed = state.assembly.paths_to(drawing.component);
        let [at] = placed.as_slice() else {
            // Not placed once: no path of it leads through the occurrence,
            // whose parent is placed once.
            continue;
        };
        let mut changed = drawing.clone();
        let mut moved = false;
        let mut stuck = None;
        for rewrite in rewrites {
            // A drawing of the moved component or of one inside it keeps
            // its paths: they are below the occurrence.
            let Some(old) = rewrite
                .old
                .strip_prefix(at.as_slice())
                .filter(|old| !old.is_empty())
            else {
                continue;
            };
            let new = rewrite.new.strip_prefix(at.as_slice());
            for path in changed.occurrence_paths_mut() {
                if !path.0.starts_with(old) {
                    continue;
                }
                match new {
                    Some(new) => {
                        let rest = path.0.split_off(old.len());
                        path.0 = new.to_vec();
                        path.0.extend(rest);
                        moved = true;
                    }
                    None => stuck = Some(rewrite.name()),
                }
            }
        }
        if let Some(name) = stuck {
            blockers.push(format!(
                "{} of {}: its views lead through {name}, which would leave {}",
                drawing.name,
                state.assembly.name(drawing.component),
                state.assembly.name(drawing.component)
            ));
        } else if moved {
            updates.push(drawing.name.clone());
            out.drawings.push((i, changed));
        }
    }
    // Print plate items and comments follow it when it is taken
    // (`DocState::follow`, mitcad#1318).
    let through_any = |path: &[OccurrenceUid]| rewrites.iter().any(|r| r.through(path));
    let through = |target: &PlateTarget| match target {
        PlateTarget::Occurrence(path) => through_any(&path[..]),
        PlateTarget::Body(_) => false,
    };
    let plated = state
        .print_plates
        .iter()
        .flat_map(|p| &p.items)
        .any(|item| through(&item.target));
    if plated {
        updates.push("Print plates".to_owned());
    }
    let commented = state.comments.iter().any(|c| {
        c.target
            .occurrence
            .as_ref()
            .is_some_and(|path| through_any(&path.0[..]))
    });
    if commented {
        updates.push("Comments".to_owned());
    }
    // Occurrences made by Pattern at Points keep the paths they were made
    // from.
    for made in &state.assembly.occurrences {
        let Some(pattern) = &made.pattern else {
            continue;
        };
        for rewrite in rewrites {
            let o = rewrite.occurrence;
            if pattern.source.0.contains(&o)
                || pattern
                    .target
                    .occurrence
                    .as_ref()
                    .is_some_and(|p| p.0.contains(&o))
            {
                blockers.push(format!(
                    "{} is made by Pattern at Points from {}; moving it is not supported",
                    state.assembly.occurrence_name(made.uid),
                    rewrite.name()
                ));
            }
        }
    }
    (out, updates, blockers)
}

/// Whether a timeline feature places an occurrence of its component: moves
/// or sets it, or moves it as the occurrence a joint, constraint or rigid
/// group joins (the first occurrence of a path).
fn places(def: &FeatureDef, uid: OccurrenceUid) -> bool {
    let first = |path: &OccurrencePath| path.0.first() == Some(&uid);
    match def {
        FeatureDef::MoveOccurrence(m) => m.occurrences.contains(&uid),
        FeatureDef::CapturePosition(c) => c.positions.iter().any(|p| p.occurrence == uid),
        FeatureDef::Joint(j) => first(&j.a.occurrence) || first(&j.b.occurrence),
        FeatureDef::AsBuiltJoint(j) => first(&j.a) || first(&j.b),
        FeatureDef::RigidGroup(g) => g.occurrences.contains(&uid),
        FeatureDef::ConstraintSet(set) => set
            .rows
            .iter()
            .any(|row| row.origins().any(|(_, origin)| first(&origin.occurrence))),
        _ => false,
    }
}

/// Whether a placing feature places by solving with the joints of its
/// component (joints, constraints, rigid groups) rather than by a move.
fn solves(def: &FeatureDef) -> bool {
    matches!(
        def,
        FeatureDef::Joint(_)
            | FeatureDef::AsBuiltJoint(_)
            | FeatureDef::RigidGroup(_)
            | FeatureDef::ConstraintSet(_)
    )
}

/// The first timeline feature that places an occurrence ([`places`]), or
/// made it (Pattern at Points). The whole timeline counts, also after the
/// marker and suppressed features.
fn placing_feature(state: &DocState, uid: OccurrenceUid) -> Option<String> {
    let made_by: Option<FeatureUid> = state
        .assembly
        .occurrence(uid)
        .and_then(|o| o.pattern.as_ref())
        .map(|p| p.feature);
    state
        .features
        .iter()
        .find(|f| places(&f.def, uid) || made_by == Some(f.uid))
        .map(|f| f.name.clone())
}

/// Every occurrence's placement in the design by its path from the root,
/// with the placements of a recompute (else the own ones).
fn world_placements(
    assembly: &Assembly,
    placements: &BTreeMap<OccurrenceUid, Transform>,
) -> BTreeMap<Vec<OccurrenceUid>, Transform> {
    let mut out = BTreeMap::new();
    let mut todo = vec![(ComponentUid::ROOT, Vec::new(), Transform::IDENTITY)];
    while let Some((component, path, world)) = todo.pop() {
        if path.len() > 64 {
            continue; // a guard against cycles in loaded data
        }
        for o in assembly.children(component) {
            let Some(own) = placements
                .get(&o.uid)
                .copied()
                .or_else(|| o.pattern.is_none().then_some(o.transform))
            else {
                continue;
            };
            let mut next = path.clone();
            next.push(o.uid);
            let placed = world.after(&own);
            out.insert(next.clone(), placed);
            todo.push((o.component, next, placed));
        }
    }
    out
}

/// Whether two placements in the design are the same within the joint
/// solver's tolerances.
fn same_place(a: &Transform, b: &Transform) -> bool {
    let scale = a
        .translation
        .iter()
        .chain(&b.translation)
        .fold(1.0_f64, |m, v| m.max(v.abs()));
    (0..3).all(|r| (0..3).all(|c| (a.linear[r][c] - b.linear[r][c]).abs() <= 1e-6))
        && (0..3).all(|i| (a.translation[i] - b.translation[i]).abs() <= 1e-5 * scale)
}

/// How the features that place the moved occurrence go with it
/// (mitcad#301). Each becomes a feature of the new parent: the lowest
/// component holding every occurrence it refers to after the move must be
/// the new parent itself, since a joint, constraint or rigid group places
/// only the occurrences placed directly in its component (`crate::joints`)
/// and a Move or Capture Position only those (`recompute::place`). Paths
/// are rewritten relative to the new parent; Moves and Capture Positions
/// are turned into its coordinates.
///
/// What the feature's result depends on must not change after it in the
/// timeline in a way the moved occurrence would no longer follow:
/// occurrences below the new parent on the way to geometry it joins the
/// occurrence to, and the members of a rigid group, must not be placed by
/// a later feature; the parents on the way between the old parent and the
/// new one must not be placed by a later Move, Capture Position or
/// Pattern at Points (later joints of the new parent solve together with
/// the carried ones); for a Move or Capture Position, by no later feature
/// at all (it is in the parent's coordinates at its point of the
/// timeline).
///
/// With several occurrences moving together (mitcad#1847) a feature that
/// places only moved ones goes whole: their paths are the same in the new
/// parent.
struct Carry<'a> {
    state: &'a DocState,
    /// The occurrences that move.
    moved: &'a [OccurrenceUid],
    /// Their old parent and the path to it, the new parent and the path to it.
    parent: ComponentUid,
    parents: &'a [OccurrenceUid],
    target: ComponentUid,
    destination: &'a [OccurrenceUid],
    /// The old parent's coordinates into the new parent's.
    relative: Transform,
    /// The occurrences on the way between the old and the new parent.
    chain: Vec<OccurrenceUid>,
    /// The indices of the features that go with it.
    carriers: &'a BTreeSet<usize>,
}

/// What one carried feature becomes.
struct CarriedChange {
    /// The new definition of the feature (whole, or what stays of it).
    def: FeatureDef,
    /// The new parent, when it goes there whole.
    component: Option<ComponentUid>,
    /// The new feature with the moved occurrence's part, when it is split.
    split: Option<FeatureDef>,
    /// Parameters the new feature takes.
    params: Vec<ParamId>,
}

impl Carry<'_> {
    fn name(&self, uid: OccurrenceUid) -> String {
        self.state.assembly.occurrence_name(uid)
    }

    fn names(&self, uids: &[OccurrenceUid]) -> String {
        uids.iter()
            .map(|x| self.name(*x))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Whether an occurrence of the old parent moves.
    fn mine(&self, uid: OccurrenceUid) -> bool {
        self.moved.contains(&uid)
    }

    fn label(&self, path: &[OccurrenceUid]) -> String {
        let a = &self.state.assembly;
        if path.is_empty() {
            a.name(ComponentUid::ROOT)
        } else {
            a.path_name(path)
        }
    }

    /// The first feature after position `index`, other than the carried
    /// ones, that places `x`: any such feature with `any`, else a Move,
    /// Capture Position or Pattern at Points.
    fn placed_later(&self, x: OccurrenceUid, index: usize, any: bool) -> Option<&str> {
        let made_by = self
            .state
            .assembly
            .occurrence(x)
            .and_then(|o| o.pattern.as_ref())
            .map(|p| p.feature);
        self.state
            .features
            .iter()
            .enumerate()
            .skip(index + 1)
            .filter(|(j, _)| !self.carriers.contains(j))
            .find(|(_, f)| {
                made_by == Some(f.uid) || (places(&f.def, x) && (any || !solves(&f.def)))
            })
            .map(|(_, f)| f.name.as_str())
    }

    /// `subject`, a moved occurrence, would no longer follow `x`.
    fn later(&self, x: OccurrenceUid, by: &str, subject: OccurrenceUid) -> String {
        format!(
            "{} is placed by {by} later in the timeline, which {} in {} would no longer follow",
            self.name(x),
            self.name(subject),
            self.state.assembly.name(self.target)
        )
    }

    /// Why a joint's or constraint's side outside the new parent keeps
    /// `subject` from going there, and what to do instead (mitcad#1847).
    fn outside(&self, subject: OccurrenceUid, absolute: &[OccurrenceUid]) -> String {
        let target = self.state.assembly.name(self.target);
        let subject = self.name(subject);
        let why = format!(
            "it joins {subject} to {}, which is not in {target}; it places only occurrences placed \
             directly in its component, so it could not place {subject} there",
            self.label(absolute)
        );
        // The occurrence on the other side's way that is not in the new
        // parent; moving it there too keeps the joint, unless it holds the
        // moved occurrence itself (its old parent on the way).
        let common = absolute
            .iter()
            .zip(self.destination)
            .take_while(|(x, y)| x == y)
            .count();
        match absolute.get(common) {
            Some(&away) if self.parents.get(common) != Some(&away) => format!(
                "{why}. Move {} into {target} as well, or delete it and join {subject} again \
                 inside {target}",
                self.name(away)
            ),
            _ => format!("{why}. Delete it and join {subject} again inside {target}"),
        }
    }

    /// The occurrence paths of a joint's or constraint's sides (relative
    /// to the old parent) relative to the new parent; Err when one of them
    /// is not in it, or what the feature depends on moves later.
    fn sides(&self, paths: Vec<&mut Vec<OccurrenceUid>>, index: usize) -> Result<(), String> {
        let subject = paths
            .iter()
            .find_map(|p| p.first().copied().filter(|x| self.mine(*x)))
            .unwrap_or(self.moved[0]);
        let mut tight = Vec::new();
        let mut loose = self.chain.clone();
        for path in paths {
            let mine = path.first().is_some_and(|x| self.mine(*x));
            let mut absolute = if mine {
                self.destination.to_vec()
            } else {
                self.parents.to_vec()
            };
            absolute.extend_from_slice(path);
            if !absolute.starts_with(self.destination) {
                return Err(self.outside(subject, &absolute));
            }
            let relative = absolute.split_off(self.destination.len());
            if !mine && let Some((first, rest)) = relative.split_first() {
                loose.push(*first);
                tight.extend_from_slice(rest);
            }
            *path = relative;
        }
        for x in &tight {
            if let Some(by) = self.placed_later(*x, index, true) {
                return Err(self.later(*x, by, subject));
            }
        }
        for x in loose.iter().filter(|x| !tight.contains(x)) {
            if let Some(by) = self.placed_later(*x, index, false) {
                return Err(self.later(*x, by, subject));
            }
        }
        Ok(())
    }

    /// A Move or Capture Position works in the parent's coordinates at its
    /// point of the timeline: the parents on the way must stay after it.
    fn steady(&self, index: usize, subject: OccurrenceUid) -> Result<(), String> {
        for x in &self.chain {
            if let Some(by) = self.placed_later(*x, index, true) {
                return Err(format!(
                    "it places {} in {}'s coordinates, and {} is placed by {by} later in the \
                     timeline",
                    self.name(subject),
                    self.state.assembly.name(self.parent),
                    self.name(*x)
                ));
            }
        }
        Ok(())
    }

    /// A Move's motion in the new parent's coordinates.
    fn motion(&self, spec: &MoveSpec, subject: OccurrenceUid) -> Result<MoveSpec, String> {
        let r = &self.relative;
        let parent = self.state.assembly.name(self.parent);
        match spec {
            MoveSpec::Free { matrix } => {
                let rows: Vec<Vec<f64>> = matrix.iter().map(|row| row.to_vec()).collect();
                let by = crate::assembly::from_rows(&rows)?;
                let turned = r.after(&by).after(&inverse(r));
                Ok(MoveSpec::Free {
                    matrix: matrix_rows(&turned),
                })
            }
            MoveSpec::TranslateXyz { .. } => {
                let straight = (0..3).all(|i| {
                    (0..3).all(|j| (r.linear[i][j] - f64::from(u8::from(i == j))).abs() <= 1e-9)
                });
                if straight {
                    Ok(spec.clone())
                } else {
                    Err(format!(
                        "it moves {} along the x, y and z axes of {parent}, which are turned in \
                         {}; its distances would mean another motion there",
                        self.name(subject),
                        self.state.assembly.name(self.target)
                    ))
                }
            }
            _ => Err(format!(
                "it moves {} by geometry of {parent}, which a feature of {} cannot use",
                self.name(subject),
                self.state.assembly.name(self.target)
            )),
        }
    }

    /// What the carried feature at `index` becomes; Err with the blocker.
    fn carry(&self, index: usize, def: &FeatureDef) -> Result<CarriedChange, String> {
        let whole = |def: FeatureDef| CarriedChange {
            def,
            component: Some(self.target),
            split: None,
            params: Vec::new(),
        };
        match def {
            FeatureDef::Joint(j) => {
                let mut j = j.clone();
                self.sides(vec![&mut j.a.occurrence.0, &mut j.b.occurrence.0], index)?;
                Ok(whole(FeatureDef::Joint(j)))
            }
            FeatureDef::AsBuiltJoint(j) => {
                let mut j = j.clone();
                let mut paths = vec![&mut j.a.0, &mut j.b.0];
                if let Some(origin) = &mut j.origin {
                    paths.push(&mut origin.occurrence.0);
                }
                self.sides(paths, index)?;
                Ok(whole(FeatureDef::AsBuiltJoint(j)))
            }
            FeatureDef::ConstraintSet(set) => {
                let (mut moved, kept): (Vec<_>, Vec<_>) =
                    set.rows.iter().cloned().partition(|row| {
                        row.origins().any(|(_, origin)| {
                            origin.occurrence.0.first().is_some_and(|x| self.mine(*x))
                        })
                    });
                for row in &mut moved {
                    let paths: Vec<&mut Vec<OccurrenceUid>> = [&mut row.a, &mut row.b]
                        .into_iter()
                        .chain(row.a2.as_mut())
                        .chain(row.b2.as_mut())
                        .map(|origin| &mut origin.occurrence.0)
                        .collect();
                    self.sides(paths, index)?;
                }
                let moved = FeatureDef::ConstraintSet(ConstraintSetDef { rows: moved });
                if kept.is_empty() {
                    return Ok(whole(moved));
                }
                Ok(CarriedChange {
                    def: FeatureDef::ConstraintSet(ConstraintSetDef { rows: kept }),
                    component: None,
                    params: moved.params(),
                    split: Some(moved),
                })
            }
            FeatureDef::RigidGroup(g) => {
                let (mine, others): (Vec<OccurrenceUid>, Vec<OccurrenceUid>) =
                    g.occurrences.iter().copied().partition(|x| self.mine(*x));
                let subject = mine.first().copied().unwrap_or(self.moved[0]);
                if others.is_empty() {
                    // All its members move (mitcad#1847): it goes whole and
                    // holds them together in the new parent as before.
                    for x in &self.chain {
                        if let Some(by) = self.placed_later(*x, index, false) {
                            return Err(self.later(*x, by, subject));
                        }
                    }
                    return Ok(whole(def.clone()));
                }
                // The other members stay in the old parent: the new one
                // must hold it, and its occurrence there (`top`) stands for
                // them.
                let top = self
                    .parents
                    .get(self.destination.len())
                    .filter(|_| self.parents.starts_with(self.destination));
                let Some(&top) = top else {
                    return Err(format!(
                        "it holds {} rigidly to {}, which would not be in {}. Move the whole \
                         group together",
                        self.names(&mine),
                        self.names(&others),
                        self.state.assembly.name(self.target)
                    ));
                };
                for x in self.parents[self.destination.len() + 1..]
                    .iter()
                    .chain(&others)
                {
                    if let Some(by) = self.placed_later(*x, index, true) {
                        return Err(self.later(*x, by, subject));
                    }
                }
                if let Some(by) = self.placed_later(top, index, false) {
                    return Err(self.later(top, by, subject));
                }
                if others.len() == 1 {
                    let members = g
                        .occurrences
                        .iter()
                        .map(|x| if self.mine(*x) { *x } else { top })
                        .collect();
                    return Ok(whole(FeatureDef::RigidGroup(RigidGroupDef::new(members))));
                }
                let mut held = mine;
                held.push(top);
                Ok(CarriedChange {
                    def: FeatureDef::RigidGroup(RigidGroupDef::new(others)),
                    component: None,
                    split: Some(FeatureDef::RigidGroup(RigidGroupDef::new(held))),
                    params: Vec::new(),
                })
            }
            FeatureDef::MoveOccurrence(m) => {
                let (mine, others): (Vec<OccurrenceUid>, Vec<OccurrenceUid>) =
                    m.occurrences.iter().copied().partition(|x| self.mine(*x));
                let subject = mine.first().copied().unwrap_or(self.moved[0]);
                self.steady(index, subject)?;
                let transform = self.motion(&m.transform, subject)?;
                if others.is_empty() {
                    return Ok(whole(FeatureDef::MoveOccurrence(MoveOccurrenceDef {
                        occurrences: mine,
                        transform,
                    })));
                }
                if !matches!(m.transform, MoveSpec::Free { .. }) {
                    return Err(format!(
                        "it also moves {}; a Move by values cannot be split (its parameters \
                         belong to it): edit it to move {} alone",
                        self.names(&others),
                        self.names(&mine)
                    ));
                }
                Ok(CarriedChange {
                    def: FeatureDef::MoveOccurrence(MoveOccurrenceDef {
                        occurrences: others,
                        transform: m.transform.clone(),
                    }),
                    component: None,
                    split: Some(FeatureDef::MoveOccurrence(MoveOccurrenceDef {
                        occurrences: mine,
                        transform,
                    })),
                    params: Vec::new(),
                })
            }
            FeatureDef::CapturePosition(c) => {
                let (mut mine, others): (Vec<_>, Vec<_>) = c
                    .positions
                    .iter()
                    .cloned()
                    .partition(|p| self.mine(p.occurrence));
                let subject = mine.first().map_or(self.moved[0], |p| p.occurrence);
                self.steady(index, subject)?;
                for p in &mut mine {
                    p.transform = self.relative.after(&p.transform);
                }
                let moved = FeatureDef::CapturePosition(CapturePositionDef::new(mine));
                if others.is_empty() {
                    return Ok(whole(moved));
                }
                Ok(CarriedChange {
                    def: FeatureDef::CapturePosition(CapturePositionDef::new(others)),
                    component: None,
                    split: Some(moved),
                    params: Vec::new(),
                })
            }
            _ => Err(format!(
                "it places {} in a way the move cannot carry",
                self.names(self.moved)
            )),
        }
    }
}

/// The next free default names for new features, as `DocState::default_name`
/// gives them, each counted once given.
fn new_name(state: &DocState, base: &str, given: &[String]) -> String {
    let count = state
        .features
        .iter()
        .map(|f| f.def.base_name())
        .filter(|b| *b == base)
        .count()
        + given.iter().filter(|n| n.starts_with(base)).count();
    (count + 1..)
        .map(|n| format!("{base}{n}"))
        .find(|name| state.features.iter().all(|f| &f.name != name) && !given.contains(name))
        .expect("an unused name exists")
}

/// Takes a planned move into a state.
fn take(state: &mut DocState, plan: &ReparentPlan) {
    for moved in &plan.moved {
        if let Some(o) = state.assembly.occurrence_mut(moved.occurrence) {
            o.parent = plan.destination;
            o.transform = moved.transform;
        }
    }
    for (i, def) in &plan.changes.features {
        Arc::make_mut(&mut state.features[*i]).def = def.clone();
    }
    for (i, component) in &plan.changes.components {
        Arc::make_mut(&mut state.features[*i]).component = *component;
    }
    for (id, owner) in &plan.changes.owners {
        state.parameters.set_owner(*id, Some(*owner));
    }
    // From the last, so that the indices still hold.
    for (i, entry) in plan.changes.inserted.iter().rev() {
        let beside = state.features[*i].uid;
        state.features.insert(i + 1, Arc::new(entry.clone()));
        if *i < state.marker {
            state.marker += 1;
        }
        if let Some(g) = state.group_of(beside) {
            let members = &mut state.groups[g].features;
            let at = members
                .iter()
                .position(|f| *f == beside)
                .map_or(0, |p| p + 1);
            members.insert(at, entry.uid);
        }
        state.next_uid = state.next_uid.max(entry.uid.0 + 1);
    }
    for (i, axis, path) in &plan.changes.analyses {
        match &mut state.analyses[*i].def {
            AnalysisDef::Section(section) if *axis => section.axis_occurrence = Some(path.clone()),
            AnalysisDef::Section(section) => section.occurrence = Some(path.clone()),
            AnalysisDef::Print(_) => {}
        }
    }
    for (i, text) in &plan.changes.isolated {
        state.display.isolated[*i].occurrence.clone_from(text);
    }
    for (i, paths) in &plan.changes.contact_sets {
        state.contact_sets[*i].occurrences.clone_from(paths);
    }
    for (i, pick, path) in &plan.changes.measurements {
        let measurement = &mut state.annotations[*i];
        let pick = match *pick {
            0 => Some(&mut measurement.a),
            1 => measurement.b.as_mut(),
            _ => measurement
                .plane
                .as_mut()
                .and_then(|plane| plane.pick.as_mut()),
        };
        if let Some(pick) = pick {
            pick.occurrence = Some(path.clone());
        }
    }
    for (i, drawing) in &plan.changes.drawings {
        state.drawings[*i].clone_from(drawing);
    }
    // Print plate items and comment targets (mitcad#1318).
    for moved in &plan.moved {
        let new = plan.path_of(moved);
        state.follow(&Follow::Moved {
            old: &moved.source,
            new: &new,
        });
    }
}

/// The occurrences' names, for messages.
fn names_of(assembly: &Assembly, uids: impl IntoIterator<Item = OccurrenceUid>) -> String {
    uids.into_iter()
        .map(|uid| assembly.occurrence_name(uid))
        .collect::<Vec<_>>()
        .join(", ")
}

impl<K: Kernel> Document<K> {
    /// What places an occurrence other than its own transform: a timeline
    /// feature, or the joints where the solved placement differs.
    fn placed_by(&self, uid: OccurrenceUid) -> Option<String> {
        if let Some(name) = placing_feature(&self.state, uid) {
            return Some(name);
        }
        let own = self.state.assembly.occurrence(uid)?;
        let solved = self.result.placements.get(&uid)?;
        (!same_placement(solved, &own.transform)).then(|| "the joints".to_owned())
    }

    /// The checks of the occurrence to move, whatever the destination:
    /// its component and its parent.
    pub fn reparent_source(
        &self,
        source: &[OccurrenceUid],
    ) -> Result<(OccurrenceUid, ComponentUid, ComponentUid), ModelError> {
        let a = &self.state.assembly;
        let Some((&uid, parents)) = source.split_last() else {
            return Err(invalid("the root component cannot be moved"));
        };
        let component =
            crate::joints::path_component(a, ComponentUid::ROOT, source).map_err(invalid)?;
        let occurrence = self.state.require_occurrence(uid)?;
        let name = a.occurrence_name(uid);
        let parent = occurrence.parent;
        if occurrence.pattern.is_some() {
            return Err(invalid(format!(
                "{name} is made by Pattern at Points; edit that feature instead"
            )));
        }
        // Each placement of a shared parent has the occurrence: moving it
        // out of one would move it out of all.
        let placements = a.paths_to(parent);
        if placements.len() != 1 {
            return Err(invalid(format!(
                "{} is placed {} times and {name} is in each of them; moving it would change \
                 every placement of {}",
                a.name(parent),
                placements.len(),
                a.name(parent)
            )));
        }
        if placements[0] != parents {
            return Err(invalid(format!(
                "{} is not the placement of {name}",
                OccurrencePath(source.to_vec())
            )));
        }
        self.state.editable(parent)?;
        // The features that place it go with it (mitcad#301); a placement
        // that differs without one cannot follow.
        if placing_feature(&self.state, uid).is_none()
            && let Some(by) = self.placed_by(uid)
        {
            return Err(invalid(format!(
                "{name} is placed by {by}; edit or delete it before moving {name}"
            )));
        }
        Ok((uid, component, parent))
    }

    /// Whether the occurrence at `source` (its path from the root) can go
    /// into the component placed at `destination` (empty: the root), and
    /// what the move does: the one validation of hierarchy moves (design
    /// policies, mitcad#220, belong here too). Err lists the blockers.
    pub fn plan_reparent(
        &self,
        source: &[OccurrenceUid],
        destination: &[OccurrenceUid],
    ) -> Result<ReparentPlan, Vec<String>> {
        self.plan_reparent_all(&[source.to_vec()], destination)
    }

    /// The other members of the rigid groups that hold the occurrences at
    /// `sources` (their paths from the root, in one parent), and of the
    /// groups that hold those in turn, as paths from the root (mitcad#1847):
    /// what Move to Component offers to move with them.
    pub fn rigid_group_of(&self, sources: &[Vec<OccurrenceUid>]) -> Vec<Vec<OccurrenceUid>> {
        let Some((&first, parents)) = sources.first().and_then(|s| s.split_last()) else {
            return Vec::new();
        };
        let Some(parent) = self.state.assembly.occurrence(first).map(|o| o.parent) else {
            return Vec::new();
        };
        let mut held: Vec<OccurrenceUid> = sources
            .iter()
            .filter_map(|s| s.split_last())
            .filter(|(_, way)| *way == parents)
            .map(|(uid, _)| *uid)
            .collect();
        let given = held.len();
        loop {
            let mut grew = false;
            for f in &self.state.features {
                let FeatureDef::RigidGroup(g) = &f.def else {
                    continue;
                };
                if f.component != parent
                    || f.suppressed
                    || !g.occurrences.iter().any(|x| held.contains(x))
                {
                    continue;
                }
                for x in &g.occurrences {
                    if !held.contains(x) {
                        held.push(*x);
                        grew = true;
                    }
                }
            }
            if !grew {
                break;
            }
        }
        held[given..]
            .iter()
            .map(|x| {
                let mut path = parents.to_vec();
                path.push(*x);
                path
            })
            .collect()
    }

    /// Whether the occurrences at `sources` (their paths from the root, all
    /// placed in one component) can go together into the component placed
    /// at `destination` (empty: the root), and what the move does
    /// (mitcad#1847: several selected occurrences, or a rigid group's
    /// members). Err lists the blockers.
    pub fn plan_reparent_all(
        &self,
        sources: &[Vec<OccurrenceUid>],
        destination: &[OccurrenceUid],
    ) -> Result<ReparentPlan, Vec<String>> {
        let one = |e: ModelError| vec![e.to_string()];
        let a = &self.state.assembly;
        let mut moving: Vec<MovedOccurrence> = Vec::new();
        let mut from = None;
        for source in sources {
            let (uid, component, parent) = self.reparent_source(source).map_err(one)?;
            if moving.iter().any(|m| m.occurrence == uid) {
                continue;
            }
            match from {
                Some(p) if p != parent => {
                    return Err(vec![format!(
                        "{} and {} are in different components; occurrences move together only \
                         from one component",
                        a.occurrence_name(moving[0].occurrence),
                        a.occurrence_name(uid)
                    )]);
                }
                _ => from = Some(parent),
            }
            moving.push(MovedOccurrence {
                occurrence: uid,
                component,
                source: source.clone(),
                transform: Transform::IDENTITY,
            });
        }
        let Some(parent) = from else {
            return Err(vec!["there is no occurrence to move".to_owned()]);
        };
        let uids: Vec<OccurrenceUid> = moving.iter().map(|m| m.occurrence).collect();
        let name = names_of(a, uids.iter().copied());
        let target = crate::joints::path_component(a, ComponentUid::ROOT, destination)
            .map_err(|e| vec![e])?;
        if target == parent {
            // (Two whole sentences, so that each is translated whole.)
            let message = if uids.len() == 1 {
                format!("{name} is already in {}", a.name(target))
            } else {
                format!("{name} are already in {}", a.name(target))
            };
            return Err(vec![message]);
        }
        for m in &moving {
            if a.contains(m.component, target) {
                return Err(vec![format!(
                    "{} cannot go into {}, which is {}",
                    a.occurrence_name(m.occurrence),
                    a.name(target),
                    if target == m.component {
                        "its own component"
                    } else {
                        "placed in it"
                    }
                )]);
            }
        }
        let placements = a.paths_to(target);
        if placements.len() != 1 {
            return Err(vec![format!(
                "{} is placed {} times; {name} would appear in each of them. Choose a \
                 component placed once",
                a.name(target),
                placements.len()
            )]);
        }
        self.state.editable(target).map_err(one)?;
        // The placement that keeps each where it is: through the parents
        // the old and the new way do not share.
        let parents: Vec<OccurrenceUid> = moving[0].source[..moving[0].source.len() - 1].to_vec();
        let common = parents
            .iter()
            .zip(destination)
            .take_while(|(x, y)| x == y)
            .count();
        let relative = inverse(&self.path_transform(&destination[common..]))
            .after(&self.path_transform(&parents[common..]));
        for m in &mut moving {
            let own = a
                .occurrence(m.occurrence)
                .map_or(Transform::IDENTITY, |o| o.transform);
            m.transform = relative.after(&own);
            check_transform(&m.transform).map_err(one)?;
        }
        let varying = parents[common..]
            .iter()
            .chain(&destination[common..])
            .find_map(|x| self.placed_by(*x).map(|by| (*x, by)))
            .map(|(x, by)| {
                format!(
                    "{} is placed by {by} in the timeline, so references across {name} could \
                     change where they are evaluated",
                    a.occurrence_name(x)
                )
            });
        let rewrites: Vec<Rewrite<'_>> = moving
            .iter()
            .map(|m| {
                let mut new = destination.to_vec();
                new.push(m.occurrence);
                Rewrite {
                    assembly: a,
                    occurrence: m.occurrence,
                    old: &m.source,
                    new,
                    varying: varying.clone(),
                }
            })
            .collect();
        // The features that place them go with them (mitcad#301).
        let carriers: BTreeSet<usize> = self
            .state
            .features
            .iter()
            .enumerate()
            .filter(|(_, f)| f.component == parent && uids.iter().any(|uid| places(&f.def, *uid)))
            .map(|(i, _)| i)
            .collect();
        let (mut changes, updates, mut blockers) =
            rewrite_references(&self.state, &rewrites, &carriers);
        drop(rewrites);
        let carry = Carry {
            state: &self.state,
            moved: &uids,
            parent,
            parents: &parents,
            target,
            destination,
            relative,
            chain: parents[common..]
                .iter()
                .chain(&destination[common..])
                .copied()
                .collect(),
            carriers: &carriers,
        };
        let mut carried = Vec::new();
        let mut given = Vec::new();
        let mut next_uid = self.state.next_uid;
        for &i in &carriers {
            let entry = &self.state.features[i];
            let change = match carry.carry(i, &entry.def) {
                Ok(change) => change,
                Err(why) => {
                    blockers.push(format!("{}: {why}", entry.name));
                    continue;
                }
            };
            changes.features.push((i, change.def));
            if let Some(component) = change.component {
                changes.components.push((i, component));
            }
            let split = change.split.map(|def| {
                let name = new_name(&self.state, def.base_name(), &given);
                given.push(name.clone());
                let new = FeatureEntry {
                    uid: FeatureUid(next_uid),
                    name: name.clone(),
                    suppressed: entry.suppressed,
                    component: target,
                    def,
                };
                next_uid += 1;
                changes
                    .owners
                    .extend(change.params.iter().map(|id| (*id, new.uid)));
                changes.inserted.push((i, new));
                name
            });
            carried.push(Carried {
                name: entry.name.clone(),
                from: parent,
                to: target,
                split,
            });
        }
        if !blockers.is_empty() {
            return Err(blockers);
        }
        let plan = ReparentPlan {
            moved: moving,
            parent,
            destination: target,
            destination_path: destination.to_vec(),
            updates,
            carried,
            changes,
        };
        let mut next = self.state.clone();
        take(&mut next, &plan);
        // The carried features must stand where they are in the timeline,
        // also after the marker, where recompute does not check them.
        let checked: BTreeSet<FeatureUid> = carriers
            .iter()
            .map(|i| self.state.features[*i].uid)
            .chain(plan.changes.inserted.iter().map(|(_, e)| e.uid))
            .collect();
        for (i, entry) in next.features.iter().enumerate() {
            if checked.contains(&entry.uid)
                && let Err(why) = next.check_feature(i, entry)
            {
                return Err(vec![format!("{}: {why}", entry.name)]);
            }
        }
        // The design policy (mitcad#220): the destination must take
        // components, and no link may come to reach an owner that refuses
        // it. Shown with the plan, so the picker explains it before the
        // command (which checks it again) runs.
        if let Some(refused) = self.policy_refusal(&next) {
            return Err(vec![refused]);
        }
        Ok(plan)
    }

    /// Moves the occurrence at `source` into the component placed at
    /// `destination` (empty: the root), where it stays in the design, with
    /// the references through it and the features that place it; one undo
    /// step. Refused, changing nothing, when [`Document::plan_reparent`]
    /// refuses it, a feature that built would fail, or an occurrence would
    /// be somewhere else in the design at the marker.
    pub fn reparent_occurrence(
        &mut self,
        source: &[OccurrenceUid],
        destination: &[OccurrenceUid],
    ) -> Result<ReparentPlan, ModelError> {
        self.reparent_occurrences(&[source.to_vec()], destination)
    }

    /// Moves the occurrences at `sources` (all placed in one component)
    /// together into the component placed at `destination`, as
    /// [`Document::reparent_occurrence`] moves one, in one undo step
    /// (mitcad#1847); refused as [`Document::plan_reparent_all`] refuses it.
    pub fn reparent_occurrences(
        &mut self,
        sources: &[Vec<OccurrenceUid>],
        destination: &[OccurrenceUid],
    ) -> Result<ReparentPlan, ModelError> {
        let stale = self.result.results.len() != self.state.features.len()
            || self
                .result
                .results
                .iter()
                .zip(&self.state.features)
                .any(|(r, f)| r.uid != f.uid);
        if stale {
            self.try_recompute()?;
        }
        let plan = self
            .plan_reparent_all(sources, destination)
            .map_err(|blockers| invalid(blockers.join("; ")))?;
        let built: BTreeSet<FeatureUid> = self
            .result
            .results
            .iter()
            .filter(|r| r.status.is_ok())
            .map(|r| r.uid)
            .collect();
        // Where everything is in the design now, by the paths it will have.
        let ways: Vec<(Vec<OccurrenceUid>, Vec<OccurrenceUid>)> = plan
            .moved
            .iter()
            .map(|m| (m.source.clone(), plan.path_of(m)))
            .collect();
        let before: BTreeMap<Vec<OccurrenceUid>, Transform> =
            world_placements(&self.state.assembly, &self.result.placements)
                .into_iter()
                .map(|(mut path, placed)| {
                    if let Some((old, new)) = ways.iter().find(|(old, _)| path.starts_with(old)) {
                        let rest = path.split_off(old.len());
                        path.clone_from(new);
                        path.extend(rest);
                    }
                    (path, placed)
                })
                .collect();
        self.apply_checked(
            |state| {
                let label = format!(
                    "Move {} to {}",
                    names_of(&state.assembly, plan.moved.iter().map(|m| m.occurrence)),
                    state.assembly.name(plan.destination)
                );
                take(state, &plan);
                Ok((label, ()))
            },
            |state, result| {
                let failed: Vec<String> = result
                    .results
                    .iter()
                    .filter(|r| built.contains(&r.uid))
                    .filter_map(|r| match &r.status {
                        FeatureStatus::Failed(message) => Some(format!(
                            "{}: {message}",
                            state
                                .features
                                .iter()
                                .find(|f| f.uid == r.uid)
                                .map_or_else(|| r.uid.to_string(), |f| f.name.clone())
                        )),
                        _ => None,
                    })
                    .collect();
                if !failed.is_empty() {
                    return Err(invalid(format!(
                        "the move would make features fail: {}",
                        failed.join("; ")
                    )));
                }
                // The design looks the same at the marker (mitcad#301: the
                // carried features solve the same placements).
                let after = world_placements(&state.assembly, &result.placements);
                let moved = before.iter().find(|(path, placed)| {
                    after.get(*path).is_none_or(|now| !same_place(placed, now))
                });
                if let Some((path, _)) = moved {
                    let by = if plan.carried.is_empty() {
                        String::new()
                    } else {
                        format!(
                            ": {} would place the occurrences differently in {}",
                            plan.carried
                                .iter()
                                .map(|c| c.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", "),
                            state.assembly.name(plan.destination)
                        )
                    };
                    return Err(invalid(format!(
                        "the move would change where {} is in the design{by}",
                        state.assembly.path_name(path)
                    )));
                }
                Ok(())
            },
        )?;
        Ok(plan)
    }
}
