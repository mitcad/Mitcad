// SPDX-License-Identifier: MIT
//! Explicit destinations for Move to Component (mitcad#201). Legacy
//! automatic moves keep their separate, definition-wide child placement.

use serde::Deserialize;

use super::{
    DocState, Document, ModelError, MovePlan, PlannedGroup, follow_cut_scopes, has_origins,
    invalid, plane_features, set_component,
};
use crate::assembly::inverse;
use crate::document::same_placement;
use crate::features::extrude::{Extent, Side, Start};
use crate::features::extrude_scope::ComponentMove;
use crate::features::{FeatureDef, FrameDef, OccurrencePath};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::links::OccurrenceLink;
use crate::recompute::Read;
use crate::transform::Transform;
use std::collections::BTreeSet;
use std::sync::Arc;

/// A destination is an occurrence context, not only a shared definition.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MoveDestination {
    Existing {
        component: ComponentUid,
        occurrence: OccurrencePath,
    },
    New {
        parent: ComponentUid,
        parent_occurrence: OccurrencePath,
        #[serde(default)]
        name: Option<String>,
    },
}

pub struct MovedDestination {
    pub component: ComponentUid,
    pub occurrence: OccurrencePath,
    pub created: bool,
    pub bodies: Vec<BodyUid>,
    pub features: Vec<FeatureUid>,
}

struct DestinationContext {
    source: Vec<OccurrenceUid>,
    target: Vec<OccurrenceUid>,
    component: ComponentUid,
    /// From source coordinates to existing target coordinates, or the
    /// new occurrence's placement in its parent.
    transform: Transform,
}

impl<K: Kernel> Document<K> {
    /// Explicit moves never silently choose one instance of a shared
    /// definition. This also checks repeated ancestors of the occurrence.
    fn move_context(
        &self,
        component: ComponentUid,
        given: Option<&OccurrencePath>,
    ) -> Result<Vec<OccurrenceUid>, ModelError> {
        self.state.editable(component)?;
        let paths = self.assembly().paths_to(component);
        if paths.len() != 1 {
            return Err(invalid(format!(
                "{} must have exactly one occurrence in the design to move features; \
                 shared or unplaced components are not supported",
                self.assembly().name(component)
            )));
        }
        let path = &paths[0];
        if let Some(given) = given
            && given.0 != *path
        {
            return Err(invalid(format!(
                "occurrence {given} is not the placement of {}",
                self.assembly().name(component)
            )));
        }
        for uid in path {
            let placed = self.assembly().occurrence(*uid).expect("path exists");
            self.state.editable(placed.component)?;
        }
        Ok(path.clone())
    }

    fn destination_context(
        &self,
        plan: &MovePlan,
        source_occurrence: Option<&OccurrencePath>,
        destination: &MoveDestination,
    ) -> Result<DestinationContext, ModelError> {
        let source = self.move_context(plan.source, source_occurrence)?;
        let (component, occurrence) = match destination {
            MoveDestination::Existing {
                component,
                occurrence,
            } => {
                if *component == plan.source {
                    return Err(invalid("the destination already owns these features"));
                }
                (*component, occurrence)
            }
            MoveDestination::New {
                parent,
                parent_occurrence,
                name,
            } => {
                if let Some(name) = name {
                    self.state
                        .check_component_name(name, ComponentUid(u32::MAX))?;
                }
                (*parent, parent_occurrence)
            }
        };
        let target = self.move_context(component, Some(occurrence))?;
        let in_context = |uid: &OccurrenceUid| source.contains(uid) || target.contains(uid);
        // The placement map includes every ordinary assembly occurrence.
        // Only positioning features or a solved placement differing from the
        // assembly definition indicate a context that cannot be rebased here.
        let placed_by_feature = source
            .iter()
            .chain(&target)
            .any(|uid| !self.state.positioning_features(*uid).is_empty());
        let solved_placement = self.result.placements.iter().any(|(uid, placement)| {
            in_context(uid)
                && self
                    .assembly()
                    .occurrence(*uid)
                    .is_some_and(|occurrence| !same_placement(placement, &occurrence.transform))
        });
        if placed_by_feature || solved_placement {
            return Err(invalid(
                "the source or destination is placed by a timeline feature; \
                 moving its history between occurrence contexts is not supported",
            ));
        }
        let groups = plan.together.as_ref().unwrap_or(&plan.groups);
        if groups.len() != 1 {
            return Err(invalid(
                "an explicit destination accepts one group of features",
            ));
        }
        let moved: BTreeSet<_> = groups[0].features.iter().copied().collect();
        // A feature-created target can depend on the very history being
        // moved into it. Supporting that is the separate Move History command.
        for uid in &target {
            let c = self
                .assembly()
                .occurrence(*uid)
                .expect("path exists")
                .component;
            if self
                .assembly()
                .component(c)
                .is_some_and(|c| c.created_by.is_some())
            {
                return Err(invalid(
                    "a destination created by a modeling feature is not supported; \
                     use an independently created component",
                ));
            }
        }
        for feature in self.features() {
            if moved.contains(&feature.uid) && feature.def.info().new_component() {
                return Err(invalid(format!(
                    "{} creates a component; moving its ownership to an explicit destination \
                     could create a dependency cycle",
                    feature.name
                )));
            }
            if has_origins(&feature.def)
                && feature
                    .def
                    .references()
                    .features
                    .iter()
                    .any(|f| moved.contains(f))
            {
                return Err(invalid(format!(
                    "{} refers to moved geometry through an assembly constraint; \
                     edit that reference before choosing another destination",
                    feature.name
                )));
            }
        }
        let transform = inverse(&self.path_transform(&target)).after(&self.path_transform(&source));
        if !transform.is_finite() || !transform.is_rigid() {
            return Err(invalid(
                "the source and destination must have rigid placements",
            ));
        }
        if matches!(destination, MoveDestination::Existing { .. }) {
            self.check_destination_features(&groups[0], component, &transform)?;
        }
        Ok(DestinationContext {
            source,
            target,
            component,
            transform,
        })
    }

    /// Geometry must be equivariant under the change of coordinates.
    /// Unsupported definitions are refused before any document mutation.
    fn check_destination_features(
        &self,
        group: &PlannedGroup,
        target: ComponentUid,
        transform: &Transform,
    ) -> Result<(), ModelError> {
        let target_has_geometry = self
            .features()
            .any(|f| f.component == target && f.def.info().creates_bodies());
        for entry in self.features() {
            let moved = group.features.contains(&entry.uid);
            if !(entry.component == target || moved && target_has_geometry) {
                continue;
            }
            let implicit =
                entry.def.info().tool_use().is_some_and(|use_| {
                    !use_.operation.makes_bodies() && use_.participants.is_empty()
                });
            let reads_all = self
                .result
                .result(entry.uid)
                .and_then(|r| r.output.as_ref())
                .is_some_and(|o| o.reads.iter().any(|r| matches!(r, Read::AllBodies(_))));
            if implicit || reads_all {
                return Err(invalid(format!(
                    "{} reads all bodies in its component; specify its body references \
                     before moving features into a component with other geometry",
                    entry.name
                )));
            }
        }
        if transform.is_identity() {
            return Ok(());
        }
        for uid in &group.features {
            let entry = self.state.entry(*uid).expect("planned feature");
            let supported = match &entry.def {
                FeatureDef::Sketch(sketch) => {
                    sketch.plane_link.is_none() && sketch.plane.origin_frame().is_some()
                }
                FeatureDef::Extrude(extrude) => {
                    matches!(extrude.start, Start::ProfilePlane | Start::Offset { .. })
                        && match &extrude.extent {
                            Extent::Distance { .. } | Extent::Symmetric { .. } => true,
                            Extent::TwoSides { side1, side2, .. } => {
                                matches!(side1, Side::Distance { .. })
                                    && matches!(side2, Side::Distance { .. })
                            }
                            _ => false,
                        }
                }
                FeatureDef::Fillet(_) | FeatureDef::Chamfer(_) | FeatureDef::Combine(_) => true,
                _ => false,
            };
            if !supported {
                return Err(invalid(format!(
                    "{} ({}) cannot yet preserve its geometry between different component \
                     placements; choose Create New or a destination with the same placement",
                    entry.name,
                    entry.def.type_name()
                )));
            }
        }
        Ok(())
    }

    /// A read-only check used by the destination picker and by the command.
    pub fn check_move_destination(
        &self,
        plan: &MovePlan,
        source_occurrence: Option<&OccurrencePath>,
        destination: &MoveDestination,
    ) -> Result<(), ModelError> {
        self.destination_context(plan, source_occurrence, destination)?;
        // The design policy's containment (mitcad#220), so that the picker
        // explains it; the command checks every setting again, links too.
        let owner = match destination {
            MoveDestination::Existing { component, .. } => *component,
            MoveDestination::New { parent, .. } => {
                if let Some(refused) =
                    self.creation_refusal(*parent, crate::policy::Setting::Components)
                {
                    return Err(invalid(refused));
                }
                return Ok(());
            }
        };
        for f in plan.groups.iter().flat_map(|g| &g.features) {
            let setting = self
                .state
                .entry(*f)
                .and_then(|e| crate::policy::containment(&e.def));
            if let Some(refused) = setting.and_then(|s| self.creation_refusal(owner, s)) {
                return Err(invalid(format!(
                    "{}: {refused}",
                    self.state
                        .entry(*f)
                        .map_or_else(|| f.to_string(), |e| e.name.clone())
                )));
            }
        }
        Ok(())
    }

    pub fn move_to_destination(
        &mut self,
        source: ComponentUid,
        groups: &[super::MoveGroup],
        source_occurrence: Option<&OccurrencePath>,
        destination: &MoveDestination,
        together: bool,
    ) -> Result<MovedDestination, ModelError> {
        self.ensure_results()?;
        let plan = self.plan_move_to_components(source, groups)?;
        if let Some(problem) = plan.problems.first() {
            return Err(invalid(problem.clone()));
        }
        let context = self.destination_context(&plan, source_occurrence, destination)?;
        let groups = match (&plan.together, together) {
            (Some(merged), true) => merged,
            (Some(_), false) => {
                return Err(invalid(format!(
                    "{}; keep those bodies together, or change the features first",
                    plan.conflicts
                        .iter()
                        .map(|c| c.message.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                )));
            }
            (None, _) => &plan.groups,
        };
        let group = &groups[0];
        let default_name = if !plan.everything && group.bodies.len() == 1 {
            self.state
                .free_component_name(&self.body_name(group.bodies[0]))
        } else {
            self.assembly().unused_name("Component")
        };
        let label_source = self.assembly().name(source);
        let keeps = self
            .result
            .owners
            .iter()
            .any(|(b, c)| *c == source && !group.bodies.contains(b));
        self.apply_move(|state| {
            let created = matches!(destination, MoveDestination::New { .. });
            let (component, target) = if let MoveDestination::New { name, .. } = destination {
                let name = name.as_ref().unwrap_or(&default_name);
                state.check_component_name(name, ComponentUid(u32::MAX))?;
                let component = state.assembly.create(Some(name), None);
                // The link settings of where the features were (mitcad#220).
                let policy = state.assembly.policy(source).for_moved_content();
                state.assembly.set_policy(component, policy);
                let occurrence =
                    state
                        .assembly
                        .place(component, context.component, context.transform);
                state
                    .assembly
                    .occurrence_mut(occurrence)
                    .expect("placed")
                    .grounded = plan.everything;
                let mut target = context.target.clone();
                target.push(occurrence);
                (component, target)
            } else {
                rebase_sketches(state, &group.features, &context.transform);
                (context.component, context.target.clone())
            };
            set_component(state, &group.features, component);
            // The extrusions' cut scopes take the destination (mitcad#859).
            let path = OccurrencePath(target.clone());
            follow_cut_scopes(
                state,
                &group.features,
                &ComponentMove {
                    from: source,
                    to: component,
                    path: &|_: &OccurrencePath| path.clone(),
                    bodies: &group.bodies,
                    keeps,
                },
            );
            relocate_links(
                state,
                &context.source,
                &target,
                &group.features,
                &group.bodies,
            );
            let label = format!(
                "Move Features of {label_source} into {}",
                state.assembly.name(component)
            );
            Ok((
                label,
                MovedDestination {
                    component,
                    occurrence: OccurrencePath(target),
                    created,
                    bodies: group.bodies.clone(),
                    features: group.features.clone(),
                },
            ))
        })
    }
}

fn rebase_sketches(state: &mut DocState, moved: &[FeatureUid], transform: &Transform) {
    if transform.is_identity() {
        return;
    }
    for entry in &mut state.features {
        if !moved.contains(&entry.uid) {
            continue;
        }
        if let FeatureDef::Sketch(sketch) = &mut Arc::make_mut(entry).def {
            let plane = sketch.plane.origin_frame().expect("validated origin plane");
            let desired = transform.apply_frame(&sketch.place(&plane));
            let to_plane = inverse(&Transform::between_frames(
                &crate::profile::SketchFrame::XY,
                &plane,
            ));
            let local = to_plane.apply_frame(&desired);
            sketch.frame = Some(Box::new(FrameDef {
                origin: local.origin,
                x_axis: local.x_axis,
                y_axis: local.y_axis,
            }));
        }
    }
}

/// Links are rooted at the design, so a sibling/ancestor destination
/// replaces the path instead of appending a new child occurrence.
fn relocate_links(
    state: &mut DocState,
    source: &[OccurrenceUid],
    target: &[OccurrenceUid],
    moved: &[FeatureUid],
    bodies: &[BodyUid],
) {
    let follow = |link: &mut OccurrenceLink, names: &BTreeSet<FeatureUid>, own: bool| {
        if names.iter().any(|f| moved.contains(f)) && link.source.0.as_slice() == source {
            link.source.0 = target.to_vec();
        }
        if own {
            link.target.0 = target.to_vec();
        }
    };
    for entry in &mut state.features {
        let own = moved.contains(&entry.uid);
        let mut def = entry.def.clone();
        match &mut def {
            FeatureDef::Sketch(sketch) => {
                if let Some(link) = &mut sketch.plane_link {
                    follow(link, &plane_features(&sketch.plane), own);
                }
                for projection in &mut sketch.projections {
                    let mut names = crate::features::sketch::topo_features(&projection.source);
                    if let Some(body) = projection.body {
                        names.insert(body.feature);
                    }
                    if let Some(link) = &mut projection.link {
                        follow(link, &names, own);
                    }
                }
            }
            FeatureDef::Combine(combine) => {
                for (body, link) in combine.links_mut() {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            FeatureDef::Sweep(sweep) => {
                for (body, link) in &mut sweep.body_links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            FeatureDef::SplitBody(split) => {
                for (body, link) in &mut split.tool_links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            FeatureDef::Extrude(extrude) => {
                for (body, link) in &mut extrude.object_links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            FeatureDef::Revolve(revolve) => {
                for (body, link) in &mut revolve.object_links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            FeatureDef::ConstructionPlane(plane) => {
                for (body, link) in &mut plane.links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
                for (datum, link) in &mut plane.datum_links {
                    follow(link, &BTreeSet::from([*datum]), own);
                }
            }
            FeatureDef::ConstructionAxis(axis) => {
                for (body, link) in &mut axis.links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
                for (datum, link) in &mut axis.datum_links {
                    follow(link, &BTreeSet::from([*datum]), own);
                }
            }
            FeatureDef::ConstructionPoint(point) => {
                for (body, link) in &mut point.links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
                for (datum, link) in &mut point.datum_links {
                    follow(link, &BTreeSet::from([*datum]), own);
                }
            }
            FeatureDef::Derive(derive) => {
                for (body, link) in &mut derive.links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            // A body transfer's source and destination (mitcad#465).
            FeatureDef::BodyTransfer(transfer) => {
                let names = transfer.bodies.iter().map(|b| b.feature).collect();
                follow(&mut transfer.link, &names, own);
            }
            // A seat on another component's shaft (mitcad#578).
            seated @ (FeatureDef::Pulley(_)
            | FeatureDef::Gear(_)
            | FeatureDef::PrintableBearing(_)) => {
                if let Some((body, link)) = seated.seat_link_mut() {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            _ => {}
        }
        if def != entry.def {
            Arc::make_mut(entry).def = def;
        }
    }
    let source = OccurrencePath(source.to_vec()).to_string();
    let target = OccurrencePath(target.to_vec()).to_string();
    state.display.isolated = state
        .display
        .isolated
        .iter()
        .map(|item| {
            let mut item = item.clone();
            if item.occurrence == source && item.body.is_some_and(|b| bodies.contains(&b)) {
                item.occurrence = target.clone();
            }
            item
        })
        .collect();
}
