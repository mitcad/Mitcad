// SPDX-License-Identifier: MIT
//! Save Component as Design (mitcad#1057): a component written as a design
//! file of its own, its definition the new design's root: its features
//! and timeline, sketches, construction geometry, the parameters its
//! expressions use, the components placed in it (linked and library
//! components inside it stay linked), its materials, appearances, property
//! sets and design policy. The copy is the one Insert Component's unlinked
//! insert makes the other way round (`copy.rs`).
//!
//! What it refers to outside itself cannot go with it. Each such
//! reference is frozen at its current geometry first, on a copy of this
//! design's state, so that the copy is self-contained:
//!
//! - a sketch on another component's face or plane lies on that plane as
//!   a fixed frame; a projection of another component's geometry keeps its
//!   curves where they are now, unlinked (as Clean Up Stale Projections
//!   unlinks one);
//! - construction geometry on other components' geometry becomes fixed
//!   geometry where it is now;
//! - bodies of other components that a feature reads (a combine's tools,
//!   an extrusion's or revolution's objects, a sweep's path and guides, a
//!   split's tools, a derive's bodies, a seat's shaft, the bodies moved in
//!   by a body transfer) become hidden copies of those bodies as they are
//!   now, in a base feature of the component before the feature; the
//!   references to their faces, edges and vertices follow the copies;
//! - bodies another component's feature made in it (a new-component
//!   operation, Create Components from Bodies) become a base feature of
//!   its own, as those bodies are now;
//! - a move's points and axes and a point pattern's points picked on
//!   other components (mitcad#1069) become fixed points and axes where
//!   the feature last found them, in its component's coordinates;
//! - an assembly cut of other components drops those targets, and joints
//!   of other components to it stay in this design.
//!
//! The command lists them ([`CutReference`]); this design does not change.
//! With `replace`, the component in this design then becomes a linked
//! component of the saved file in one undo step: its features and the
//! components only it holds go, a base feature holds the bodies it shows,
//! and what refers to its bodies from outside (joints, links) follows the
//! base feature's faces, keeping its occurrences, their placements,
//! joints and grounding.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use serde_json::Value;

use super::components::{design_digest, file_label, recorded_link_path, resolve_path};
use super::copy::CopySource;
use super::{Annotation, BodyAttributes, ComponentCopy, DocState, Document, ModelError, invalid};
use crate::assembly::{Assembly, ExternalLink, inverse};
use crate::datum::Datum;
use crate::exchange::ImportBody;
use crate::features::construction::{AxisDefinition, PlaneDefinition, PointDefinition};
use crate::features::extrude_scope::TargetScope;
use crate::features::geom_ref::GeomRef;
use crate::features::moves::{MotionBasis, MoveSpec};
use crate::features::{
    BaseDef, FeatureDef, FeatureEntry, FrameDef, OccurrencePath, PatternObjects, RemoveBodyDef,
    SketchPlane,
};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::links::{OccurrenceLink, OccurrenceLinkExt};
use crate::parameters::ParamId;
use crate::topo::{EdgeName, FaceName, RoleKey, TopoName, VertexName};
use crate::transform::Transform;

/// What a reference to geometry outside the saved component is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutKind {
    /// A sketch lies on another component's face or plane.
    SketchPlane,
    /// A sketch projects another component's geometry.
    Projection,
    /// Construction geometry on another component's geometry.
    Construction,
    /// A feature reads a body of another component.
    Body,
    /// A feature of another component made bodies in it.
    MadeBodies,
    /// An assembly cut of another component.
    CutTarget,
    /// A joint (or rigid group, motion link, assembly constraint) of
    /// another component that joins it.
    Joint,
    /// A move's or a point pattern's point picked on another component
    /// (mitcad#1069).
    Point,
    /// A move's axis picked on another component (mitcad#1069).
    Axis,
}

impl CutKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SketchPlane => "sketch_plane",
            Self::Projection => "projection",
            Self::Construction => "construction",
            Self::Body => "body",
            Self::MadeBodies => "made_bodies",
            Self::CutTarget => "cut_target",
            Self::Joint => "joint",
            Self::Point => "point",
            Self::Axis => "axis",
        }
    }
}

/// A reference of the saved component to something outside it, which a
/// design of its own cannot carry (mitcad#1057).
#[derive(Debug, Clone, PartialEq)]
pub struct CutReference {
    /// The feature that refers to it: one of the component's, or, for made
    /// bodies and joints, one outside it.
    pub feature: FeatureUid,
    pub feature_name: String,
    pub kind: CutKind,
    /// What it refers to: an occurrence path's name (`Plate:1`), or the
    /// component a feature outside it is in.
    pub target: String,
    /// Frozen at its current geometry in the new design; false: left out.
    pub frozen: bool,
}

impl CutReference {
    /// One line for the user: what refers to what, and what the new
    /// design does with it.
    pub fn message(&self) -> String {
        let name = &self.feature_name;
        let target = &self.target;
        let done = if self.frozen {
            "frozen at its current geometry"
        } else {
            "left out"
        };
        match self.kind {
            CutKind::SketchPlane => format!("{name}: its plane on {target}, {done}"),
            CutKind::Projection => format!("{name}: its projection of {target}, {done}"),
            CutKind::Construction => format!("{name}: its geometry on {target}, {done}"),
            CutKind::Body => format!("{name}: a body of {target}, {done}"),
            CutKind::MadeBodies => format!("{name} in {target}: the bodies it made, {done}"),
            CutKind::CutTarget => format!("{name}: its cut of {target}, {done}"),
            CutKind::Joint => format!("{name}: its joint to {target}, {done}"),
            CutKind::Point => format!("{name}: its point on {target}, {done}"),
            CutKind::Axis => format!("{name}: its axis on {target}, {done}"),
        }
    }
}

/// Options of [`Document::save_component_as_design`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SaveComponentOptions {
    /// The component in this design becomes a linked component of the
    /// saved file, in one undo step.
    pub replace: bool,
    /// The folder of this design's file: relative link paths are relative
    /// to it. The folder it was read from or saved to when None.
    pub base: Option<PathBuf>,
}

/// What [`Document::save_component_as_design`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedComponent {
    /// The design file written.
    pub path: PathBuf,
    /// The references to things outside the component, frozen or left out.
    pub references: Vec<CutReference>,
    /// With `replace`: the link's path as this design records it.
    pub link: Option<String>,
}

/// The new design's state and what it could not carry.
struct ComponentDesign {
    state: DocState,
    references: Vec<CutReference>,
}

/// A body of the saved component as its linked component shows it: in
/// the component's coordinates, with the names of its faces.
struct LinkedBody<S> {
    body: BodyUid,
    name: String,
    shape: S,
    faces: Vec<Vec<String>>,
    attributes: BodyAttributes,
}

impl<K: Kernel> Document<K> {
    /// The references of a component to geometry outside it that Save
    /// Component as Design freezes or leaves out (mitcad#1057), for the
    /// dialog to list before saving. Nothing changes. Refused for the root,
    /// a linked component and a library component, and for a component
    /// whose features name occurrences outside it in a way that cannot be
    /// frozen (its own joints, bound lengths or patterns of occurrences
    /// outside it).
    pub fn component_design_references(
        &self,
        component: ComponentUid,
    ) -> Result<Vec<CutReference>, ModelError> {
        Ok(self.component_design(component, None, None)?.references)
    }

    /// Save Component as Design (mitcad#1057): writes the component as a
    /// design file of its own at `path` (single file, version 2), its
    /// definition the new design's root, with what it refers to outside
    /// itself frozen at its current geometry ([`CutReference`]). This
    /// design does not change, unless `options.replace`: then the component
    /// becomes a linked component of the new file, keeping its occurrences,
    /// their placements, joints and grounding, in one undo step.
    pub fn save_component_as_design(
        &mut self,
        component: ComponentUid,
        path: &Path,
        options: &SaveComponentOptions,
    ) -> Result<SavedComponent, ModelError> {
        let absolute = std::path::absolute(path)
            .map_err(|e| invalid(format!("cannot resolve {}: {e}", path.display())))?;
        // Relative link paths are relative to the folder the command gives,
        // else to the one this design was read from: the same folder for
        // the links inside the component as for the replace link.
        let base = options.base.clone().or_else(|| self.source_directory());
        let design = self.component_design(component, absolute.parent(), base.as_deref())?;
        let linked = if options.replace {
            Some(self.link_plan(component)?)
        } else {
            None
        };
        let text = crate::file::single_file(&design.state);
        crate::file::write_atomically(&absolute, text.as_bytes())
            .map_err(|e| invalid(format!("cannot write {}: {e}", absolute.display())))?;
        let mut saved = SavedComponent {
            path: absolute.clone(),
            references: design.references,
            link: None,
        };
        if let Some(bodies) = linked {
            let shown = absolute.to_string_lossy().into_owned();
            let recorded = recorded_link_path(&shown, &absolute, base.as_deref());
            let link = ExternalLink {
                path: recorded.clone(),
                digest: design_digest(&text),
                feature: FeatureUid(0),
            };
            self.replace_with_link(component, link, bodies)?;
            saved.link = Some(recorded);
        }
        Ok(saved)
    }

    fn check_savable(&self, component: ComponentUid) -> Result<(), ModelError> {
        if component.is_root() {
            return Err(invalid(
                "the root component is the design itself: save the design with Save As",
            ));
        }
        let def = self
            .state
            .assembly
            .component(component)
            .ok_or_else(|| invalid(format!("no component {component}")))?;
        if let Some(link) = &def.link {
            return Err(invalid(format!(
                "{} is linked from {}, a design of its own already",
                def.name, link.path
            )));
        }
        if def.library.is_some() {
            return Err(invalid(format!("{} is a library component", def.name)));
        }
        Ok(())
    }

    /// The new design of a component: a copy of its definition as the
    /// root of an empty design, after what it refers to outside itself
    /// was frozen on a copy of this design's state. `folder` is where the
    /// new file goes (the links of linked components inside it are
    /// recorded relative to it); `base` is this design's folder, which the
    /// links it records are relative to (the folder it was read from when
    /// None).
    fn component_design(
        &self,
        component: ComponentUid,
        folder: Option<&Path>,
        base: Option<&Path>,
    ) -> Result<ComponentDesign, ModelError> {
        self.check_savable(component)?;
        let a = &self.state.assembly;
        let name = a.name(component);
        let path = crate::links::first_path(a, component, None).map_err(invalid)?;
        let inside = subtree(a, component);
        let mut frozen = self.state.clone();
        let mut references = Vec::new();
        self.freeze_outside(&mut frozen, component, &path, &inside, &mut references)?;
        let left = unfreezable(&frozen, &path, &inside);
        if !left.is_empty() {
            return Err(invalid(format!(
                "{name} cannot be saved as a design: {}; edit them to refer to geometry inside \
                 it first",
                left.join("; ")
            )));
        }
        self.joint_references(component, &inside, &mut references);
        let mut design = DocState::default();
        design
            .parameters
            .set_context(self.state.parameters.context())
            .map_err(|e| invalid(e.to_string()))?;
        // The library parts BUILD placed (mitcad#375) that a library or
        // derived component inside it may read.
        design.library_sources = self.state.library_sources.clone();
        let directory = base
            .map(Path::to_path_buf)
            .or_else(|| self.source_directory());
        let (_, copy) = design
            .copy_component_mapped(
                &frozen,
                &path,
                ComponentUid::ROOT,
                None,
                CopySource::Document(directory.as_deref()),
            )
            .map_err(|e| invalid(format!("cannot save {name} as a design: {e}")))?;
        keep_names(&mut design, &frozen, &copy);
        // The bodies keep their names, and with them their attributes in
        // the file, which keeps a body's material, appearance and
        // visibility only with its name (mitcad#1150: a hidden copy came
        // back shown); the frozen copies' as `freeze_bodies` named them.
        let mut names = super::display_names(&self.state, &self.result);
        names.extend(frozen.body_names.iter().map(|(b, n)| (*b, n.clone())));
        keep_body_names(&mut design, &names, &copy);
        keep_links(&mut design, &frozen, &copy, directory.as_deref(), folder);
        design
            .assembly
            .set_policy(ComponentUid::ROOT, a.policy(component));
        Ok(ComponentDesign {
            state: design,
            references,
        })
    }

    /// Freezes, in `state` (a copy of this design's), what the features of
    /// the component and the components inside it (`inside`, seen at
    /// `path`) refer to outside them, and lists it in `references`.
    fn freeze_outside(
        &self,
        state: &mut DocState,
        component: ComponentUid,
        path: &[OccurrenceUid],
        inside: &BTreeSet<ComponentUid>,
        references: &mut Vec<CutReference>,
    ) -> Result<(), ModelError> {
        let a = &self.state.assembly;
        let outside = |link: &OccurrenceLink| !link.source.0.starts_with(path);
        let uids: Vec<FeatureUid> = self.state.features[..self.state.marker]
            .iter()
            .filter(|f| inside.contains(&f.component))
            .map(|f| f.uid)
            .collect();
        // Bodies a feature of another component made in it come first.
        if let Some(renames) = self.freeze_made_bodies(state, component, &uids, references)? {
            renames.apply(state, &uids)?;
        }
        // The hidden copies made so far: a body read again as it was (the
        // same link, the same feature's result) takes the same copy, so
        // that two features reading one body leave one copy (mitcad#1150).
        let mut copies: Vec<(FrozenKey, Renames)> = Vec::new();
        for (i, uid) in uids.iter().enumerate() {
            let Some(position) = state.position(*uid) else {
                continue;
            };
            let entry = (*state.features[position]).clone();
            let mut def = entry.def.clone();
            // A body transfer from outside: its bodies frozen, the transfer
            // gone; a fixed conversion of its own instead of the links'.
            let (bodies, transfer) = take_outside_bodies(&mut def, &outside);
            match &mut def {
                FeatureDef::Sketch(sketch) => {
                    let output = self.sketch_output(*uid);
                    if let Some(link) = sketch.plane_link.clone().filter(|l| outside(l)) {
                        let frozen = match output {
                            Some(output) => {
                                // The plane's frame with the sketch's own,
                                // before its placement on the plane.
                                let frame = &output.base;
                                sketch.plane = SketchPlane::Xy;
                                sketch.plane_link = None;
                                sketch.frame = Some(Box::new(FrameDef {
                                    origin: frame.origin,
                                    x_axis: frame.x_axis,
                                    y_axis: frame.y_axis,
                                }));
                                true
                            }
                            None => false,
                        };
                        note(
                            references,
                            &entry,
                            CutKind::SketchPlane,
                            a.path_name(&link.source.0),
                            frozen,
                        );
                    }
                    let cut: Vec<usize> = sketch
                        .projections
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.link.as_ref().is_some_and(outside))
                        .map(|(k, _)| k)
                        .collect();
                    if !cut.is_empty() {
                        // Where their sources are now, then unlinked: the
                        // curves stay as fixed references.
                        let moved = output
                            .and_then(|o| crate::sketch::project::with_moved(&*sketch, &o.moved));
                        if let Some(moved) = moved {
                            *sketch = moved;
                        }
                        let mut targets = Vec::with_capacity(cut.len());
                        for k in cut.into_iter().rev() {
                            let projection = sketch.projections.remove(k);
                            targets.push(
                                projection
                                    .link
                                    .as_ref()
                                    .map_or_else(String::new, |l| a.path_name(&l.source.0)),
                            );
                        }
                        for target in targets.into_iter().rev() {
                            note(references, &entry, CutKind::Projection, target, true);
                        }
                    }
                }
                FeatureDef::ConstructionPlane(plane) => {
                    let link = plane
                        .links
                        .values()
                        .chain(plane.datum_links.values())
                        .find(|l| outside(l))
                        .cloned();
                    if let Some(link) = link {
                        let frozen = match self.datum(*uid) {
                            Some(Datum::Plane(p)) => {
                                plane.definition = PlaneDefinition::Fixed {
                                    origin: p.origin,
                                    x_axis: p.x_axis,
                                    y_axis: p.y_axis,
                                };
                                plane.flip = false;
                                plane.links.clear();
                                plane.datum_links.clear();
                                true
                            }
                            _ => false,
                        };
                        note(
                            references,
                            &entry,
                            CutKind::Construction,
                            a.path_name(&link.source.0),
                            frozen,
                        );
                    }
                }
                FeatureDef::ConstructionAxis(axis) => {
                    let link = axis
                        .links
                        .values()
                        .chain(axis.datum_links.values())
                        .find(|l| outside(l))
                        .cloned();
                    if let Some(link) = link {
                        let frozen = match self.datum(*uid) {
                            Some(Datum::Axis(x)) => {
                                axis.definition = AxisDefinition::Fixed {
                                    origin: x.origin,
                                    direction: x.direction,
                                };
                                axis.flip = false;
                                axis.links.clear();
                                axis.datum_links.clear();
                                true
                            }
                            _ => false,
                        };
                        note(
                            references,
                            &entry,
                            CutKind::Construction,
                            a.path_name(&link.source.0),
                            frozen,
                        );
                    }
                }
                FeatureDef::ConstructionPoint(point) => {
                    let link = point
                        .links
                        .values()
                        .chain(point.datum_links.values())
                        .find(|l| outside(l))
                        .cloned();
                    if let Some(link) = link {
                        let frozen = match self.datum(*uid) {
                            Some(Datum::Point(p)) => {
                                point.definition = PointDefinition::Fixed { point: p.point };
                                point.links.clear();
                                point.datum_links.clear();
                                true
                            }
                            _ => false,
                        };
                        note(
                            references,
                            &entry,
                            CutKind::Construction,
                            a.path_name(&link.source.0),
                            frozen,
                        );
                    }
                }
                FeatureDef::Extrude(extrude) => {
                    if let Some(scope) = extrude.cut_scope.as_mut() {
                        let mut dropped = Vec::new();
                        match &mut scope.targets {
                            TargetScope::Automatic {
                                excluded,
                                excluded_bodies,
                            } => {
                                excluded.retain(|t| t.occurrence.0.starts_with(path));
                                excluded_bodies.retain(|b| b.occurrence.0.starts_with(path));
                            }
                            TargetScope::Explicit {
                                included,
                                excluded_bodies,
                            } => {
                                included.retain(|t| {
                                    let kept = t.occurrence.0.starts_with(path);
                                    if !kept {
                                        dropped.push(t.occurrence.0.clone());
                                    }
                                    kept
                                });
                                excluded_bodies.retain(|b| b.occurrence.0.starts_with(path));
                            }
                        }
                        for target in dropped {
                            note(
                                references,
                                &entry,
                                CutKind::CutTarget,
                                a.path_name(&target),
                                false,
                            );
                        }
                    }
                }
                _ => {}
            }
            self.freeze_points(&entry, &mut def, path, references);
            // The renames of this feature, and of the copies made for it by
            // body (for the features after it).
            let mut renames = Renames::default();
            let mut fresh: Vec<(BodyUid, Renames)> = Vec::new();
            if !bodies.is_empty() {
                let mut shapes = Vec::with_capacity(bodies.len());
                let mut keys = Vec::with_capacity(bodies.len());
                for (body, link) in &bodies {
                    note(
                        references,
                        &entry,
                        CutKind::Body,
                        a.path_name(&link.source.0),
                        true,
                    );
                    let key = FrozenKey {
                        body: *body,
                        link: link.clone(),
                        component: entry.component,
                        maker: self.shape_maker(*uid, *body),
                    };
                    if transfer.is_none()
                        && let Some((_, copy)) = copies.iter().find(|(k, _)| *k == key)
                    {
                        renames.merge(copy);
                        continue;
                    }
                    shapes.push(self.frozen_shape(
                        *uid,
                        entry.component,
                        *body,
                        link,
                        transfer.flatten(),
                    )?);
                    keys.push(key);
                }
                if !shapes.is_empty() {
                    // The copies of bodies a feature reads are hidden; those
                    // a transfer moved in are the component's own bodies now.
                    let source = format!("bodies {} reads, frozen", entry.name);
                    let made = self.freeze_bodies(
                        state,
                        Some(*uid),
                        entry.component,
                        &shapes,
                        transfer.is_none(),
                        &source,
                        inside_features(&self.state, inside),
                    )?;
                    for (key, copy) in keys.into_iter().zip(made) {
                        renames.merge(&copy);
                        fresh.push((key.body, copy.clone()));
                        if transfer.is_none() {
                            copies.push((key, copy));
                        }
                    }
                }
            }
            let position = state
                .position(*uid)
                .ok_or_else(|| invalid(format!("{} is gone", entry.name)))?;
            if transfer.is_some() {
                state.features.remove(position);
                if position < state.marker {
                    state.marker -= 1;
                }
            } else if def != entry.def {
                state.features[position] = Arc::new(FeatureEntry { def, ..entry });
            }
            renames.apply(state, &[*uid])?;
            // The features after it name the new copies' faces where this
            // feature's result carries them (a combine's cut faces); one
            // that reads such a body from outside itself is left to its own
            // turn, which takes the copy of the body as it reads it.
            if !fresh.is_empty() {
                for later in &uids[i + 1..] {
                    let Some(position) = state.position(*later) else {
                        continue;
                    };
                    let read = outside_bodies(&state.features[position].def, &outside);
                    let mut renames = Renames::default();
                    for (body, copy) in &fresh {
                        if !read.contains(body) {
                            renames.merge(copy);
                        }
                    }
                    if !renames.is_empty() {
                        renames.apply(state, &[*later])?;
                    }
                }
            }
        }
        Ok(())
    }

    /// The feature whose result a feature reads a body from: the last one
    /// before it that set or removed the body, from the last recompute.
    fn shape_maker(&self, consumer: FeatureUid, body: BodyUid) -> Option<FeatureUid> {
        let position = self.state.position(consumer)?;
        self.result
            .results
            .iter()
            .take(position)
            .rev()
            .find_map(|result| {
                let output = result.output.as_deref()?;
                output
                    .changes
                    .iter()
                    .any(|change| match change {
                        crate::recompute::Change::Set(changed, _)
                        | crate::recompute::Change::Remove(changed) => *changed == body,
                    })
                    .then_some(result.uid)
            })
    }

    /// A move's points and axes and a point pattern's points picked in
    /// occurrences outside the component (seen at `path`) become fixed
    /// points and axes where the feature last found them, in its
    /// component's coordinates (mitcad#1069): a move's from the motion it
    /// recorded, a pattern's from its anchor and elements; otherwise (the
    /// axis of a move along it, a feature without a result) where the
    /// geometry and its occurrence are at the timeline marker. One that
    /// cannot be found stays as it is, listed as left out.
    fn freeze_points(
        &self,
        entry: &FeatureEntry,
        def: &mut FeatureDef,
        path: &[OccurrenceUid],
        references: &mut Vec<CutReference>,
    ) {
        let a = &self.state.assembly;
        let outside = |p: &OccurrencePath| !p.0.starts_with(path);
        let output = self.output_of(entry.uid).filter(|o| o.result.is_ok());
        // The picked geometry as this design names it (the frozen copy's
        // definition may name frozen bodies already).
        let original = self.state.entry(entry.uid).map(|e| &e.def);
        if let Some(spec) = def.move_spec_mut() {
            let axes = matches!(
                spec,
                MoveSpec::TranslateAlong { .. } | MoveSpec::Rotate { .. }
            );
            let recorded: Vec<GeomRef> = match (&*spec, output.and_then(|o| o.move_basis)) {
                (MoveSpec::Rotate { .. }, Some(MotionBasis::Rotate { origin, direction })) => {
                    vec![GeomRef::FixedAxis { origin, direction }]
                }
                (MoveSpec::PointToPoint { .. }, Some(MotionBasis::PointToPoint { from, to })) => {
                    vec![GeomRef::FixedPoint(from), GeomRef::FixedPoint(to)]
                }
                (
                    MoveSpec::PointToPosition { .. },
                    Some(MotionBasis::PointToPosition { point }),
                ) => vec![GeomRef::FixedPoint(point)],
                _ => Vec::new(),
            };
            let picked: Vec<GeomRef> = original
                .and_then(FeatureDef::move_spec)
                .map(|s| {
                    s.placed_references()
                        .into_iter()
                        .map(|(r, _)| r.clone())
                        .collect()
                })
                .unwrap_or_default();
            let kind = if axes { CutKind::Axis } else { CutKind::Point };
            for (k, (reference, occurrence)) in spec.placed_references_mut().into_iter().enumerate()
            {
                let Some(at) = occurrence.clone().filter(|p| outside(p)) else {
                    continue;
                };
                let fixed = recorded
                    .get(k)
                    .cloned()
                    .or_else(|| self.placed_now(picked.get(k)?, &at, entry.component, axes));
                let frozen = fixed.is_some();
                if let Some(fixed) = fixed {
                    *reference = fixed;
                    *occurrence = None;
                }
                note(references, entry, kind, a.path_name(&at.0), frozen);
            }
        }
        if let FeatureDef::PatternAtPoints(pattern) = def {
            // A target is the anchor moved by its element's translation.
            let anchor = output
                .and_then(|o| o.pattern_anchor.as_ref())
                .map(|p| p.point);
            let elements = output.map_or(&[][..], |o| o.elements.as_slice());
            let picked: Vec<GeomRef> = match original {
                Some(FeatureDef::PatternAtPoints(p)) => p
                    .targets
                    .iter()
                    .chain(p.anchor.iter())
                    .map(|t| t.point.clone())
                    .collect(),
                _ => Vec::new(),
            };
            let targets = pattern.targets.len();
            for (k, point) in pattern
                .targets
                .iter_mut()
                .chain(pattern.anchor.iter_mut())
                .enumerate()
            {
                let Some(at) = point.occurrence.clone().filter(|p| outside(p)) else {
                    continue;
                };
                let recorded = if k < targets {
                    anchor.zip(elements.get(k + 1)).map(|(anchor, element)| {
                        std::array::from_fn(|i| anchor[i] + element.translation[i])
                    })
                } else {
                    anchor
                };
                let fixed = recorded
                    .map(GeomRef::FixedPoint)
                    .or_else(|| self.placed_now(picked.get(k)?, &at, entry.component, false));
                let frozen = fixed.is_some();
                if let Some(fixed) = fixed {
                    point.point = fixed;
                    point.occurrence = None;
                }
                note(
                    references,
                    entry,
                    CutKind::Point,
                    a.path_name(&at.0),
                    frozen,
                );
            }
        }
    }

    /// The last recompute's result of a feature, if it has one.
    fn output_of(&self, uid: FeatureUid) -> Option<&crate::recompute::Output<K::Shape>> {
        let position = self.state.position(uid)?;
        self.result
            .results
            .get(position)
            .filter(|r| r.uid == uid)
            .and_then(|r| r.output.as_deref())
    }

    /// A point or an axis of the component the occurrence path `at` (from
    /// the root) ends in, as the geometry is at the timeline marker and
    /// where the occurrence places it now, in `component`'s coordinates
    /// (where its first occurrence places it).
    fn placed_now(
        &self,
        reference: &GeomRef,
        at: &OccurrencePath,
        component: ComponentUid,
        axis: bool,
    ) -> Option<GeomRef> {
        let a = &self.state.assembly;
        crate::joints::path_component(a, ComponentUid::ROOT, &at.0).ok()?;
        let placed = |path: &[OccurrenceUid]| {
            path.iter().try_fold(Transform::IDENTITY, |t, o| {
                Some(t.after(&self.placement(*o)?))
            })
        };
        let own = crate::links::first_path(a, component, None).ok()?;
        let transform = inverse(&placed(own.as_slice())?).after(&placed(at.0.as_slice())?);
        if axis {
            let x = self.resolve_axis(reference).ok()?;
            Some(GeomRef::FixedAxis {
                origin: transform.apply_point(x.origin),
                direction: transform.apply_vector(x.direction),
            })
        } else {
            let p = self.resolve_point(reference).ok()?;
            Some(GeomRef::FixedPoint(transform.apply_point(p)))
        }
    }

    /// The bodies a feature of another component made in the component
    /// (a new-component operation, Create Components from Bodies), as they
    /// are after that feature, as a base feature of the component before
    /// its first feature; the component is then made by no feature. The
    /// renames of their faces for the component's features.
    fn freeze_made_bodies(
        &self,
        state: &mut DocState,
        component: ComponentUid,
        uids: &[FeatureUid],
        references: &mut Vec<CutReference>,
    ) -> Result<Option<Renames>, ModelError> {
        let a = &self.state.assembly;
        let Some(maker) = a.component(component).and_then(|c| c.created_by) else {
            return Ok(None);
        };
        let (Some(entry), Some(position)) = (self.state.entry(maker), self.state.position(maker))
        else {
            return Ok(None);
        };
        if position >= self.state.marker {
            return Ok(None);
        }
        let made: Vec<BodyUid> = match &entry.def {
            FeatureDef::ComponentFromBodies(from) => from.bodies.clone(),
            _ => self
                .result
                .results
                .get(position)
                .filter(|r| r.uid == maker)
                .and_then(|r| r.output.as_deref())
                .map(|output| {
                    output
                        .changes
                        .iter()
                        .filter_map(|change| match change {
                            crate::recompute::Change::Set(body, _) => Some(*body),
                            crate::recompute::Change::Remove(_) => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        let mut shapes = Vec::with_capacity(made.len());
        for body in made {
            if let Some(shape) = self.body_after(maker, body) {
                shapes.push((body, shape.clone(), self.body_name(body)));
            }
        }
        if shapes.is_empty() {
            return Ok(None);
        }
        let source = format!("bodies {} made, frozen", entry.name);
        // Faces named by the component's own features (a fillet's) are the
        // component's; the others are the maker's and those before it.
        let own: BTreeSet<FeatureUid> = uids.iter().copied().collect();
        let mut renames = Renames::default();
        for made in self.freeze_bodies(
            state,
            uids.first().copied(),
            component,
            &shapes,
            false,
            &source,
            own,
        )? {
            renames.merge(&made);
        }
        note(
            references,
            entry,
            CutKind::MadeBodies,
            a.name(entry.component),
            true,
        );
        if let Some(c) = state.assembly.component_mut(component) {
            c.created_by = None;
        }
        Ok(Some(renames))
    }

    /// A body of another component as a feature of `component` reads it
    /// before `consumer`, moved into `component`'s coordinates by the
    /// link's placements now (or by `fixed`, a body transfer's own
    /// conversion), with its name.
    fn frozen_shape(
        &self,
        consumer: FeatureUid,
        component: ComponentUid,
        body: BodyUid,
        link: &OccurrenceLink,
        fixed: Option<Transform>,
    ) -> Result<(BodyUid, K::Shape, String), ModelError> {
        let name = self.body_name(body);
        let shape = self.body_before(consumer, body).ok_or_else(|| {
            invalid(format!(
                "{name} is not built before {consumer}, so it cannot be frozen"
            ))
        })?;
        let transform = match fixed {
            Some(transform) => transform,
            None => link
                .transform(&self.state.assembly, component, &mut |o| {
                    self.placement(o).unwrap_or(Transform::IDENTITY)
                })
                .map_err(invalid)?,
        };
        let shape = if transform.is_identity() {
            shape.clone()
        } else {
            self.kernel()
                .transform_shape(shape, &transform, None)
                .map_err(|e| invalid(format!("cannot freeze {name}: {e}")))?
        };
        Ok((body, shape, name))
    }

    /// Adds a base feature of `component` holding `shapes` to `state`,
    /// before `before` (at the marker when None), hidden or not; the
    /// bodies keep their attributes. Returns, for each body, the renames of
    /// it and of its faces (those not named by a feature of `own`) to the
    /// base feature's.
    #[allow(clippy::too_many_arguments)]
    fn freeze_bodies(
        &self,
        state: &mut DocState,
        before: Option<FeatureUid>,
        component: ComponentUid,
        shapes: &[(BodyUid, K::Shape, String)],
        hidden: bool,
        source: &str,
        own: BTreeSet<FeatureUid>,
    ) -> Result<Vec<Renames>, ModelError> {
        let uid = state.new_uid();
        let imports: Vec<ImportBody<K::Shape>> = shapes
            .iter()
            .map(|(_, shape, name)| ImportBody {
                name: Some(name.clone()),
                color: None,
                shape: shape.clone(),
            })
            .collect();
        let bodies = self.base_bodies(&imports)?;
        let mut all = Vec::with_capacity(shapes.len());
        let mut first = 0u32;
        // Names of their own (the file keeps a body's attributes, a hidden
        // copy's visibility among them, only with its name).
        let mut taken: BTreeSet<String> = super::display_names(&self.state, &self.result)
            .into_values()
            .chain(state.body_names.values().cloned())
            .collect();
        for (index, (body, shape, name)) in (0u32..).zip(shapes) {
            let frozen = BodyUid::new(uid, index);
            state
                .body_names
                .insert(frozen, crate::exchange::unique_name(name, &mut taken));
            let mut renames = Renames::default();
            renames.bodies.insert(body.to_string(), frozen.to_string());
            let faces = self
                .kernel()
                .faces(shape)
                .map_err(|e| invalid(format!("cannot freeze {name}: {e}")))?;
            let names: Vec<Vec<String>> = faces.into_iter().map(|f| f.names).collect();
            first = renames.add_faces(uid, first, &names, &|f| !own.contains(&f))?;
            all.push(renames);
            let mut attributes = self.body_attributes(*body);
            attributes.face_appearances.clear();
            attributes.face_appearance_exprs.clear();
            attributes.print_bed_face = None;
            if hidden {
                attributes.visible = false;
                attributes.visible_when = None;
            }
            if !attributes.is_default() {
                state.body_attributes.insert(frozen, attributes);
            }
        }
        let def = FeatureDef::Base(BaseDef {
            source: Some(source.to_owned()),
            ..BaseDef::new(bodies)
        });
        let position = match before {
            Some(before) => state
                .position(before)
                .ok_or_else(|| invalid(format!("{before} is gone")))?,
            None => state.marker,
        };
        let entry = FeatureEntry {
            uid,
            name: state.default_name(BaseDef::<ParamId>::BASE_NAME),
            suppressed: false,
            component,
            def,
        };
        state.features.insert(position, Arc::new(entry));
        if position <= state.marker {
            state.marker += 1;
        }
        Ok(all)
    }

    /// A body as it is right after the feature `uid`, from the last
    /// recompute; none when it was not there.
    fn body_after(&self, uid: FeatureUid, body: BodyUid) -> Option<&K::Shape> {
        let position = self.state.position(uid)?;
        self.result
            .results
            .iter()
            .take(position + 1)
            .rev()
            .filter_map(|result| result.output.as_deref())
            .find_map(|output| {
                output.changes.iter().rev().find_map(|change| match change {
                    crate::recompute::Change::Set(changed, shape) if *changed == body => {
                        Some(Some(&shape.value))
                    }
                    crate::recompute::Change::Remove(changed) if *changed == body => Some(None),
                    _ => None,
                })
            })
            .flatten()
    }

    /// Joints, rigid groups, motion links and assembly constraints of
    /// other components that join an occurrence inside the component:
    /// they stay in this design, left out of the new one.
    fn joint_references(
        &self,
        component: ComponentUid,
        inside: &BTreeSet<ComponentUid>,
        references: &mut Vec<CutReference>,
    ) {
        let a = &self.state.assembly;
        for entry in &self.state.features[..self.state.marker] {
            if inside.contains(&entry.component) {
                continue;
            }
            let joint = matches!(
                entry.def,
                FeatureDef::Joint(_)
                    | FeatureDef::AsBuiltJoint(_)
                    | FeatureDef::RigidGroup(_)
                    | FeatureDef::ConstraintSet(_)
                    | FeatureDef::MotionLink(_)
            );
            if !joint {
                continue;
            }
            let joined = crate::joints::occurrences_used(&entry.def)
                .into_iter()
                .find(|o| {
                    a.occurrence(*o)
                        .is_some_and(|o| o.component == component || inside.contains(&o.parent))
                });
            if let Some(joined) = joined {
                note(
                    references,
                    entry,
                    CutKind::Joint,
                    a.occurrence_name(joined),
                    false,
                );
            }
        }
    }

    /// The bodies the component shows, in its own coordinates (as the
    /// linked component of its new file will), with the names of their
    /// faces and their attributes: the visible bodies of it and of the
    /// components inside it where its first occurrence places them.
    fn link_plan(&self, component: ComponentUid) -> Result<Vec<LinkedBody<K::Shape>>, ModelError> {
        let a = &self.state.assembly;
        let name = a.name(component);
        let inside = subtree(a, component);
        if let Some(late) = self.state.features[self.state.marker..]
            .iter()
            .find(|f| inside.contains(&f.component))
        {
            return Err(invalid(format!(
                "{name} cannot be replaced by a link while {} is after the timeline marker: move \
                 the marker to the end first",
                late.name
            )));
        }
        let maker = a
            .component(component)
            .and_then(|c| c.created_by)
            .and_then(|f| self.state.entry(f));
        if let Some(maker) = maker
            && matches!(maker.def, FeatureDef::Combine(_))
        {
            return Err(invalid(format!(
                "{name} was made by {}, which combines bodies outside it: it cannot be replaced \
                 by a link",
                maker.name
            )));
        }
        // Construction geometry outside on its sketches and datums, or on
        // those of the components placed in it, would lose them: a link
        // keeps no sketches or datums (mitcad#1144).
        let only = only_held(a, component, &inside);
        let seen_in = |link: &OccurrenceLink| {
            link.source
                .0
                .iter()
                .any(|o| a.occurrence(*o).is_some_and(|o| o.component == component))
        };
        for entry in &self.state.features {
            if entry.component == component || only.contains(&entry.component) {
                continue;
            }
            let datum_links = match &entry.def {
                FeatureDef::ConstructionPlane(p) => &p.datum_links,
                FeatureDef::ConstructionAxis(x) => &x.datum_links,
                FeatureDef::ConstructionPoint(p) => &p.datum_links,
                _ => continue,
            };
            if let Some((datum, _)) = datum_links.iter().find(|&(_, link)| seen_in(link)) {
                let datum = self
                    .state
                    .entry(*datum)
                    .map_or_else(|| datum.to_string(), |e| e.name.clone());
                return Err(invalid(format!(
                    "{name} cannot be replaced by a link while {} is on {datum} in it: a linked \
                     component keeps no sketches or construction geometry; edit {} first",
                    entry.name, entry.name
                )));
            }
        }
        let path = crate::links::first_path(a, component, None).map_err(invalid)?;
        let world = path.iter().fold(Transform::IDENTITY, |t, o| {
            t.after(&self.placement(*o).unwrap_or(Transform::IDENTITY))
        });
        let into = inverse(&world);
        let mut bodies = Vec::new();
        let mut shown = BTreeSet::new();
        for instance in self.instances() {
            if !instance.visible || !instance.path.starts_with(&path) {
                continue;
            }
            // One body in two places: the outer references to it (joints,
            // links, annotations) name the body, not the placement, so they
            // could not follow the right one of its base bodies.
            if !shown.insert(instance.body) {
                let twice = self
                    .body_component(instance.body)
                    .map_or_else(|| instance.name.clone(), |c| a.name(c));
                return Err(invalid(format!(
                    "{name} cannot be replaced by a link while {twice} is placed in it more than \
                     once"
                )));
            }
            let Some(shape) = self.body_shape(instance.body) else {
                continue;
            };
            let placed = into.after(&instance.transform);
            let shape = if placed.is_identity() {
                shape.clone()
            } else {
                self.kernel()
                    .transform_shape(shape, &placed, None)
                    .map_err(|e| invalid(format!("{}: {e}", instance.name)))?
            };
            let faces = self
                .kernel()
                .faces(&shape)
                .map_err(|e| invalid(format!("{}: {e}", instance.name)))?
                .into_iter()
                .map(|f| f.names)
                .collect();
            let mut attributes = self.body_attributes(instance.body);
            attributes.face_appearances.clear();
            attributes.face_appearance_exprs.clear();
            attributes.print_bed_face = None;
            bodies.push(LinkedBody {
                body: instance.body,
                name: instance.name,
                shape,
                faces,
                attributes,
            });
        }
        if bodies.is_empty() {
            return Err(invalid(format!(
                "{name} has no bodies to show: it cannot be replaced by a link"
            )));
        }
        Ok(bodies)
    }

    /// Replace with a Link (mitcad#1057): the component becomes a linked
    /// component (`link`, its feature filled in here) whose base feature
    /// holds `bodies`. Its features, the components only it holds, the
    /// feature of another component that made it and the occurrences
    /// placed in it go; what refers to its bodies from outside follows the
    /// base feature's bodies and faces, and occurrence paths through it end
    /// at its occurrence. One undo step.
    fn replace_with_link(
        &mut self,
        component: ComponentUid,
        mut link: ExternalLink,
        bodies: Vec<LinkedBody<K::Shape>>,
    ) -> Result<(), ModelError> {
        let imports: Vec<ImportBody<K::Shape>> = bodies
            .iter()
            .map(|b| ImportBody {
                name: Some(b.name.clone()),
                color: None,
                shape: b.shape.clone(),
            })
            .collect();
        let base_bodies = self.base_bodies(&imports)?;
        let a = self.state.assembly.clone();
        let inside = subtree(&a, component);
        let name = a.name(component);
        let label = format!("Replace {name} with a Link to {}", file_label(&link.path));
        self.apply(|state| {
            // The components only it holds go with it.
            let only = only_held(&a, component, &inside);
            let mut removed: BTreeSet<FeatureUid> = state
                .features
                .iter()
                .filter(|f| f.component == component || only.contains(&f.component))
                .map(|f| f.uid)
                .collect();
            // The feature that made it: a new-component operation goes (its
            // bodies are the link's now); Create Components from Bodies
            // takes its bodies out of the design instead.
            if let Some(maker) = a.component(component).and_then(|c| c.created_by)
                && let Some(position) = state.position(maker)
            {
                let entry = (*state.features[position]).clone();
                let from_bodies = match &entry.def {
                    FeatureDef::ComponentFromBodies(from) => Some(from.bodies.clone()),
                    _ => None,
                };
                match from_bodies {
                    Some(moved) => {
                        let def = FeatureDef::RemoveBody(RemoveBodyDef::new(moved));
                        state.features[position] = Arc::new(FeatureEntry { def, ..entry });
                    }
                    None => {
                        removed.insert(maker);
                    }
                }
            }
            // Bodies moved in from outside by its body transfers leave the
            // design where they came from.
            let mut taken: Vec<(FeatureUid, ComponentUid, Vec<BodyUid>)> = Vec::new();
            for f in &state.features {
                if let FeatureDef::BodyTransfer(moved) = &f.def
                    && removed.contains(&f.uid)
                {
                    let from =
                        crate::joints::path_component(&a, ComponentUid::ROOT, &moved.link.source.0)
                            .map_err(invalid)?;
                    if !inside.contains(&from) {
                        taken.push((f.uid, from, moved.bodies.clone()));
                    }
                }
            }
            for (before, from, moved) in taken {
                let position = state.require(before)?;
                let def = FeatureDef::RemoveBody(RemoveBodyDef::new(moved));
                let entry = FeatureEntry {
                    uid: state.new_uid(),
                    name: state.default_name(RemoveBodyDef::<ParamId>::BASE_NAME),
                    suppressed: false,
                    component: from,
                    def,
                };
                state.features.insert(position, Arc::new(entry));
                state.marker += 1;
            }
            // The base feature where its first feature was.
            let uid = state.new_uid();
            let position = state
                .features
                .iter()
                .position(|f| removed.contains(&f.uid))
                .unwrap_or(state.marker);
            let def = FeatureDef::Base(BaseDef {
                source: Some(file_label(&link.path)),
                ..BaseDef::new(base_bodies)
            });
            let entry = FeatureEntry {
                uid,
                name: state.default_name(BaseDef::<ParamId>::BASE_NAME),
                suppressed: false,
                component,
                def,
            };
            state.features.insert(position, Arc::new(entry));
            if position <= state.marker {
                state.marker += 1;
            }
            // Not made by a feature any more, so removing its features
            // keeps it.
            if let Some(c) = state.assembly.component_mut(component) {
                c.created_by = None;
            }
            state.remove_features(&removed);
            for c in &only {
                state.assembly.remove(*c);
            }
            state.assembly.occurrences.retain(|o| o.parent != component);
            link.feature = uid;
            if let Some(c) = state.assembly.component_mut(component) {
                c.link = Some(link);
            }
            // Its bodies' attributes on the base feature's bodies, and what
            // refers to them from outside renamed to them.
            let mut renames = Renames::default();
            let mut first = 0u32;
            for (index, body) in (0u32..).zip(&bodies) {
                let linked = BodyUid::new(uid, index);
                renames
                    .bodies
                    .entry(body.body.to_string())
                    .or_insert_with(|| linked.to_string());
                first = renames.add_faces(uid, first, &body.faces, &|f| removed.contains(&f))?;
                if !body.attributes.is_default() {
                    state
                        .body_attributes
                        .insert(linked, body.attributes.clone());
                }
            }
            let outer: Vec<FeatureUid> = state
                .features
                .iter()
                .filter(|f| f.uid != uid)
                .map(|f| f.uid)
                .collect();
            let assembly = state.assembly.clone();
            // An occurrence path through the component ends at its
            // occurrence: the occurrences placed in it are gone.
            let cut = |path: &[OccurrenceUid]| -> Option<Vec<OccurrenceUid>> {
                let at = path
                    .iter()
                    .position(|o| a.occurrence(*o).is_some_and(|o| o.component == component))?;
                if at + 1 == path.len() || assembly.occurrence(path[at]).is_none() {
                    return None;
                }
                Some(path[..=at].to_vec())
            };
            let through = |text: &str| -> Option<String> {
                cut(occurrence_path(text)?.as_slice()).map(|path| OccurrencePath(path).to_string())
            };
            // The body link maps are keyed by body and their links' sources
            // are no plain texts (mitcad#1144): a link seen in the component
            // (at or through its occurrence) follows the base feature's body.
            let seen_in = |link: &OccurrenceLink| {
                link.source
                    .0
                    .iter()
                    .any(|o| a.occurrence(*o).is_some_and(|o| o.component == component))
            };
            let body = |b: BodyUid| {
                renames
                    .bodies
                    .get(&b.to_string())
                    .and_then(|text| text.parse::<BodyUid>().ok())
            };
            for f in outer {
                let Some(position) = state.position(f) else {
                    continue;
                };
                let renamed = {
                    let entry = &state.features[position];
                    let rename = |text: &str| through(text).or_else(|| renames.text(text));
                    let mapped = rename_def(&entry.def, &rename)?;
                    let changed = mapped.is_some();
                    let mut def = mapped.unwrap_or_else(|| entry.def.clone());
                    if !relink(&mut def, &seen_in, &body, &cut) && !changed {
                        continue;
                    }
                    FeatureEntry {
                        def,
                        ..(**entry).clone()
                    }
                };
                state.features[position] = Arc::new(renamed);
            }
            // The annotations on its bodies follow them too.
            let rename = |text: &str| through(text).or_else(|| renames.text(text));
            for annotation in &mut state.annotations {
                rename_picks(annotation, &rename)?;
            }
            Ok((label, ()))
        })
    }
}

/// An annotation's picks (its two and its plane's) with the references
/// `rename` renames: their bodies, faces, edges, vertices and occurrences.
fn rename_picks(
    annotation: &mut Annotation,
    rename: &dyn Fn(&str) -> Option<String>,
) -> Result<(), ModelError> {
    let name = annotation.name.clone();
    let picks = [
        Some(&mut annotation.a),
        annotation.b.as_mut(),
        annotation.plane.as_mut().and_then(|p| p.pick.as_mut()),
    ];
    for pick in picks.into_iter().flatten() {
        let mut value = serde_json::to_value(&*pick).map_err(|e| invalid(e.to_string()))?;
        if rename_value(&mut value, rename) {
            *pick = serde_json::from_value(value)
                .map_err(|e| invalid(format!("rename {name}: {e}")))?;
        }
    }
    Ok(())
}

/// The component and every component placed in it, through any depth.
fn subtree(a: &Assembly, component: ComponentUid) -> BTreeSet<ComponentUid> {
    let mut inside = BTreeSet::from([component]);
    let mut todo = vec![component];
    while let Some(c) = todo.pop() {
        for o in a.children(c) {
            if inside.insert(o.component) {
                todo.push(o.component);
            }
        }
    }
    inside
}

/// The components inside `component` (`inside`, its subtree) that only it
/// holds, through any depth: they go with it when it is replaced by a link.
fn only_held(
    a: &Assembly,
    component: ComponentUid,
    inside: &BTreeSet<ComponentUid>,
) -> BTreeSet<ComponentUid> {
    let mut only: BTreeSet<ComponentUid> =
        inside.iter().copied().filter(|c| *c != component).collect();
    loop {
        let kept: BTreeSet<ComponentUid> = only
            .iter()
            .copied()
            .filter(|c| {
                a.occurrences_of(*c)
                    .all(|o| o.parent == component || only.contains(&o.parent))
            })
            .collect();
        if kept.len() == only.len() {
            return only;
        }
        only = kept;
    }
}

/// The links of an outer feature's definition after Replace with a Link
/// (mitcad#1144), which the text renames cannot reach: the body link maps
/// are keyed by body, so a link `seen_in` the replaced component is
/// re-keyed by `body` to the base feature's body, and every link's source
/// and target through the component end at its occurrence (`cut`). True
/// when anything changed.
fn relink(
    def: &mut FeatureDef,
    seen_in: &dyn Fn(&OccurrenceLink) -> bool,
    body: &dyn Fn(BodyUid) -> Option<BodyUid>,
    cut: &dyn Fn(&[OccurrenceUid]) -> Option<Vec<OccurrenceUid>>,
) -> bool {
    let shorten = |link: &mut OccurrenceLink| {
        let mut changed = false;
        for path in [&mut link.source, &mut link.target] {
            if let Some(shorter) = cut(path.0.as_slice()) {
                path.0 = shorter;
                changed = true;
            }
        }
        changed
    };
    let rekey = |map: &mut BTreeMap<BodyUid, OccurrenceLink>| {
        let mut changed = false;
        for (key, mut link) in std::mem::take(map) {
            let renamed = if seen_in(&link) {
                body(key).unwrap_or(key)
            } else {
                key
            };
            changed |= renamed != key;
            changed |= shorten(&mut link);
            map.insert(renamed, link);
        }
        changed
    };
    let datums = |map: &mut BTreeMap<FeatureUid, OccurrenceLink>| {
        let mut changed = false;
        for link in map.values_mut() {
            changed |= shorten(link);
        }
        changed
    };
    if let Some((_, link)) = def.seat_link_mut() {
        return shorten(link);
    }
    match def {
        FeatureDef::Extrude(x) => rekey(&mut x.object_links),
        FeatureDef::Revolve(r) => rekey(&mut r.object_links),
        FeatureDef::Sweep(s) => rekey(&mut s.body_links),
        FeatureDef::SplitBody(s) => rekey(&mut s.tool_links),
        FeatureDef::Derive(d) => rekey(&mut d.links),
        FeatureDef::Combine(c) => {
            let mut changed = rekey(&mut c.tool_links);
            for link in c.tools.iter_mut().filter_map(|t| t.link.as_mut()) {
                changed |= shorten(link);
            }
            changed
        }
        FeatureDef::ConstructionPlane(p) => {
            let changed = rekey(&mut p.links);
            datums(&mut p.datum_links) || changed
        }
        FeatureDef::ConstructionAxis(x) => {
            let changed = rekey(&mut x.links);
            datums(&mut x.datum_links) || changed
        }
        FeatureDef::ConstructionPoint(p) => {
            let changed = rekey(&mut p.links);
            datums(&mut p.datum_links) || changed
        }
        FeatureDef::BodyTransfer(t) => shorten(&mut t.link),
        FeatureDef::Sketch(s) => {
            let mut changed = false;
            for link in s
                .plane_link
                .iter_mut()
                .chain(s.projections.iter_mut().filter_map(|p| p.link.as_mut()))
            {
                changed |= shorten(link);
            }
            changed
        }
        _ => false,
    }
}

/// The features of the components `inside`.
fn inside_features(state: &DocState, inside: &BTreeSet<ComponentUid>) -> BTreeSet<FeatureUid> {
    state
        .features
        .iter()
        .filter(|f| inside.contains(&f.component))
        .map(|f| f.uid)
        .collect()
}

/// What the features inside still name outside after freezing: joints of
/// occurrences outside, a pattern's occurrences outside. One text each.
/// (A move's and a pattern's points picked outside are frozen, or left as
/// they are when they never resolved: mitcad#1069.)
fn unfreezable(
    state: &DocState,
    path: &[OccurrenceUid],
    inside: &BTreeSet<ComponentUid>,
) -> Vec<String> {
    let a = &state.assembly;
    let within = |o: OccurrenceUid| {
        path.contains(&o)
            || a.occurrence(o)
                .is_some_and(|o| inside.contains(&o.component) || inside.contains(&o.parent))
    };
    let mut left = Vec::new();
    for entry in &state.features[..state.marker] {
        if !inside.contains(&entry.component) {
            continue;
        }
        let used = match &entry.def {
            // Its context the copy places anew.
            FeatureDef::PatternAtPoints(p) => match &p.objects {
                PatternObjects::Components { occurrences } => occurrences
                    .iter()
                    .flat_map(|path| path.0.iter().copied())
                    .collect(),
                _ => Vec::new(),
            },
            def => crate::joints::occurrences_used(def),
        };
        if let Some(o) = used.into_iter().find(|o| !within(*o)) {
            left.push(format!("{} refers to {}", entry.name, a.occurrence_name(o)));
        }
    }
    left
}

/// The bodies of other components a definition reads (those whose links
/// are `outside`), taken out of it: a seat's shaft, an extrusion's or a
/// revolution's objects, a sweep's bodies, a split's and a combine's tools,
/// a derive's bodies, and the bodies a body transfer moves in, with that
/// transfer's own conversion (`Some`).
fn take_outside_bodies(
    def: &mut FeatureDef,
    outside: &dyn Fn(&OccurrenceLink) -> bool,
) -> (Vec<(BodyUid, OccurrenceLink)>, Option<Option<Transform>>) {
    let mut bodies: Vec<(BodyUid, OccurrenceLink)> = Vec::new();
    let mut transfer: Option<Option<Transform>> = None;
    if let Some((body, link)) = def.seat_link().map(|(b, l)| (b, l.clone()))
        && outside(&link)
    {
        bodies.push((body, link));
        clear_seat_link(def);
    }
    match def {
        FeatureDef::Extrude(extrude) => take_links(&mut extrude.object_links, outside, &mut bodies),
        FeatureDef::Revolve(revolve) => {
            take_links(&mut revolve.object_links, outside, &mut bodies);
        }
        FeatureDef::Sweep(sweep) => take_links(&mut sweep.body_links, outside, &mut bodies),
        FeatureDef::SplitBody(split) => take_links(&mut split.tool_links, outside, &mut bodies),
        FeatureDef::Derive(derive) => take_links(&mut derive.links, outside, &mut bodies),
        FeatureDef::Combine(combine) => {
            take_links(&mut combine.tool_links, outside, &mut bodies);
            for tool in &mut combine.tools {
                if let Some(link) = tool.link.clone().filter(|l| outside(l)) {
                    bodies.push((tool.body, link));
                    tool.link = None;
                }
            }
        }
        FeatureDef::BodyTransfer(moved) if outside(&moved.link) => {
            for body in &moved.bodies {
                bodies.push((*body, moved.link.clone()));
            }
            transfer = Some(moved.transform);
        }
        _ => {}
    }
    (bodies, transfer)
}

/// The bodies of other components a definition reads.
fn outside_bodies(
    def: &FeatureDef,
    outside: &dyn Fn(&OccurrenceLink) -> bool,
) -> BTreeSet<BodyUid> {
    let mut copy = def.clone();
    take_outside_bodies(&mut copy, outside)
        .0
        .into_iter()
        .map(|(body, _)| body)
        .collect()
}

/// A body of another component as a feature of the saved component reads
/// it: read the same way again, it takes the same frozen copy.
#[derive(Debug, PartialEq)]
struct FrozenKey {
    body: BodyUid,
    link: OccurrenceLink,
    /// The reader's component, whose coordinates the copy is in.
    component: ComponentUid,
    /// The feature whose result the reader sees.
    maker: Option<FeatureUid>,
}

/// Body links of a definition whose geometry is outside, taken out of it.
fn take_links(
    links: &mut BTreeMap<BodyUid, OccurrenceLink>,
    outside: &dyn Fn(&OccurrenceLink) -> bool,
    out: &mut Vec<(BodyUid, OccurrenceLink)>,
) {
    let cut: Vec<BodyUid> = links
        .iter()
        .filter(|(_, link)| outside(link))
        .map(|(body, _)| *body)
        .collect();
    for body in cut {
        if let Some(link) = links.remove(&body) {
            out.push((body, link));
        }
    }
}

/// A pulley's, gear's or bearing's seat on its own component's body now.
fn clear_seat_link(def: &mut FeatureDef) {
    let seat = match def {
        FeatureDef::Pulley(p) => p.seat.as_mut(),
        FeatureDef::Gear(g) => g.seat.as_mut(),
        FeatureDef::PrintableBearing(b) => b.seat.as_mut(),
        _ => None,
    };
    if let Some(seat) = seat {
        seat.link = None;
    }
}

fn note(
    references: &mut Vec<CutReference>,
    entry: &FeatureEntry,
    kind: CutKind,
    target: String,
    frozen: bool,
) {
    references.push(CutReference {
        feature: entry.uid,
        feature_name: entry.name.clone(),
        kind,
        target,
        frozen,
    });
}

/// The copied features keep their names in the new design (a copy into
/// another component numbers them afresh; here they are the only ones).
fn keep_names(design: &mut DocState, source: &DocState, copy: &ComponentCopy) {
    for (from, to) in &copy.features {
        let (Some(name), Some(position)) = (
            source.entry(*from).map(|e| e.name.clone()),
            design.position(*to),
        ) else {
            continue;
        };
        let entry = &design.features[position];
        if entry.name != name {
            let renamed = FeatureEntry {
                name,
                ..(**entry).clone()
            };
            design.features[position] = Arc::new(renamed);
        }
    }
}

/// The copied bodies keep the names `names` gives them (a copy into a
/// document is named after its next recompute; the new design is written
/// from its state).
fn keep_body_names(design: &mut DocState, names: &BTreeMap<BodyUid, String>, copy: &ComponentCopy) {
    for (body, name) in names {
        if let Some(feature) = copy.features.get(&body.feature) {
            design
                .body_names
                .insert(BodyUid::new(*feature, body.index), name.clone());
        }
    }
}

/// Linked and library components inside the saved one stay linked: their
/// copies keep the link (its path recorded from the new file's folder),
/// the library record and BUILD's record.
fn keep_links(
    design: &mut DocState,
    source: &DocState,
    copy: &ComponentCopy,
    from: Option<&Path>,
    to: Option<&Path>,
) {
    for (original, copied) in &copy.occurrences {
        let (Some(o), Some(c)) = (
            source.assembly.occurrence(*original),
            design.assembly.occurrence(*copied),
        ) else {
            continue;
        };
        let target = c.component;
        let Some(def) = source.assembly.component(o.component).cloned() else {
            continue;
        };
        let link = def.link.as_ref().map(|l| {
            let resolved = resolve_path(&l.path, from);
            let resolved = std::path::absolute(&resolved).unwrap_or(resolved);
            ExternalLink {
                path: recorded_link_path(&resolved.to_string_lossy(), &resolved, to),
                digest: l.digest.clone(),
                feature: copy.features.get(&l.feature).copied().unwrap_or(l.feature),
            }
        });
        if let Some(c) = design.assembly.component_mut(target) {
            if c.link.is_none() {
                c.link = link;
            }
            if c.library.is_none() {
                c.library = def.library.clone();
            }
            if c.build.is_none() {
                c.build = def.build.clone();
            }
        }
    }
}

/// `O1/O4`: an occurrence path's text; None for any other text.
fn occurrence_path(text: &str) -> Option<Vec<OccurrenceUid>> {
    if text.is_empty() {
        return None;
    }
    text.split('/')
        .map(|part| part.parse::<OccurrenceUid>().ok())
        .collect()
}

/// New names of bodies and faces: bodies by their uid's text, faces by
/// their canonical name.
#[derive(Debug, Default, Clone)]
struct Renames {
    bodies: HashMap<String, String>,
    faces: HashMap<String, FaceName>,
}

impl Renames {
    fn is_empty(&self) -> bool {
        self.bodies.is_empty() && self.faces.is_empty()
    }

    /// Adds the renames of `other` that this one has no rename for.
    fn merge(&mut self, other: &Renames) {
        for (old, new) in &other.bodies {
            self.bodies
                .entry(old.clone())
                .or_insert_with(|| new.clone());
        }
        for (old, new) in &other.faces {
            self.faces.entry(old.clone()).or_insert_with(|| new.clone());
        }
    }

    /// The faces of a body of the base feature `uid`, named from
    /// `first`, by the names they had; names of features `keep` refuses
    /// stay. Returns the next body's first face.
    fn add_faces(
        &mut self,
        uid: FeatureUid,
        first: u32,
        faces: &[Vec<String>],
        keep: &dyn Fn(FeatureUid) -> bool,
    ) -> Result<u32, ModelError> {
        let mut next = first;
        for names in faces {
            let name = FaceName::new(uid, "import", Some(RoleKey::Index(next)));
            for old in names {
                let (key, feature) = match FaceName::from_str(old) {
                    Ok(face) => (face.to_string(), Some(face.feature)),
                    Err(_) => (old.clone(), None),
                };
                if feature.is_none_or(keep) {
                    self.faces.entry(key).or_insert_with(|| name.clone());
                }
            }
            next = next
                .checked_add(1)
                .ok_or_else(|| invalid("too many faces"))?;
        }
        Ok(next)
    }

    fn face(&self, face: &FaceName) -> Option<FaceName> {
        if let Some(found) = self.faces.get(&face.to_string()) {
            return Some(found.clone());
        }
        // A piece of a face split later keeps its piece numbers.
        if face.split.is_empty() {
            return None;
        }
        let mut whole = face.clone();
        whole.split.clear();
        let mut found = self.faces.get(&whole.to_string())?.clone();
        found.split = face.split.clone();
        Some(found)
    }

    fn topo(&self, name: &TopoName) -> Option<TopoName> {
        match name {
            TopoName::Face(face) => self.face(face).map(TopoName::Face),
            TopoName::Edge(edge) => {
                let [a, b] = edge.faces();
                let (new_a, new_b) = (self.face(a), self.face(b));
                if new_a.is_none() && new_b.is_none() {
                    return None;
                }
                let mut renamed = EdgeName::new(
                    new_a.unwrap_or_else(|| a.clone()),
                    new_b.unwrap_or_else(|| b.clone()),
                );
                if let Some(k) = edge.index {
                    renamed = renamed.with_index(k);
                }
                Some(TopoName::Edge(renamed))
            }
            TopoName::Vertex(vertex) => {
                let faces: Vec<Option<FaceName>> =
                    vertex.faces().iter().map(|f| self.face(f)).collect();
                if faces.iter().all(Option::is_none) {
                    return None;
                }
                let mut renamed = VertexName::new(
                    faces
                        .into_iter()
                        .zip(vertex.faces())
                        .map(|(new, old)| new.unwrap_or_else(|| old.clone())),
                )?;
                if let Some(k) = vertex.index {
                    renamed = renamed.with_index(k);
                }
                Some(TopoName::Vertex(renamed))
            }
        }
    }

    /// The new text of a reference: a body's uid, a face's, an edge's or a
    /// vertex's name; None when it stays.
    fn text(&self, text: &str) -> Option<String> {
        if let Some(body) = self.bodies.get(text) {
            return Some(body.clone());
        }
        let name = text.parse::<TopoName>().ok()?;
        self.topo(&name).map(|n| n.to_string())
    }

    /// The features `uids` of `state`, their references renamed.
    fn apply(&self, state: &mut DocState, uids: &[FeatureUid]) -> Result<(), ModelError> {
        for uid in uids {
            let Some(position) = state.position(*uid) else {
                continue;
            };
            let renamed = {
                let entry = &state.features[position];
                match rename_def(&entry.def, &|text| self.text(text))? {
                    Some(def) => FeatureEntry {
                        def,
                        ..(**entry).clone()
                    },
                    None => continue,
                }
            };
            state.features[position] = Arc::new(renamed);
        }
        Ok(())
    }
}

/// Marks a parameter in the JSON form of a renamed definition.
const PARAM: char = '\u{1}';

/// Fields of definitions that are texts, not references.
const TEXT_FIELDS: [&str; 5] = ["name", "source", "text", "font", "brep"];

/// A definition with the references `rename` renames, in its JSON form
/// (as `copy.rs` maps a copy's); None when none changed.
fn rename_def(
    def: &FeatureDef,
    rename: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<FeatureDef>, ModelError> {
    let mut slots = Vec::new();
    let marked = def
        .map_params(&mut |_, id: &ParamId| {
            slots.push(*id);
            Ok::<_, ()>(format!("{PARAM}{}", slots.len() - 1))
        })
        .expect("never fails");
    let mut value = serde_json::to_value(&marked).map_err(|e| invalid(e.to_string()))?;
    if !rename_value(&mut value, rename) {
        return Ok(None);
    }
    let renamed: FeatureDef<String> =
        serde_json::from_value(value).map_err(|e| invalid(format!("rename: {e}")))?;
    let renamed = renamed.map_params(&mut |slot, marker: &String| {
        marker
            .strip_prefix(PARAM)
            .and_then(|i| i.parse::<usize>().ok())
            .and_then(|i| slots.get(i).copied())
            .ok_or_else(|| invalid(format!("rename: {slot} lost its parameter")))
    })?;
    Ok(Some(renamed))
}

fn rename_value(value: &mut Value, rename: &dyn Fn(&str) -> Option<String>) -> bool {
    match value {
        Value::String(text) if !text.starts_with(PARAM) => match rename(text) {
            Some(new) if new != *text => {
                *text = new;
                true
            }
            _ => false,
        },
        Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= rename_value(item, rename);
            }
            changed
        }
        Value::Object(fields) => {
            let mut changed = false;
            for (key, item) in fields.iter_mut() {
                if !TEXT_FIELDS.contains(&key.as_str()) {
                    changed |= rename_value(item, rename);
                }
            }
            changed
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_edges_and_vertices_follow_the_frozen_copy() {
        let mut renames = Renames::default();
        renames
            .bodies
            .insert("F3.b0".to_owned(), "F9.b0".to_owned());
        let next = renames
            .add_faces(
                FeatureUid(9),
                0,
                &[
                    vec!["F3:top".to_owned()],
                    vec!["F3:bottom".to_owned()],
                    vec!["F5:fillet".to_owned()],
                ],
                &|f| f != FeatureUid(5),
            )
            .unwrap();
        assert_eq!(next, 3);
        assert_eq!(renames.text("F3.b0").as_deref(), Some("F9.b0"));
        assert_eq!(renames.text("F3:top").as_deref(), Some("F9:import(0)"));
        // A piece of a face split later.
        assert_eq!(renames.text("F3:top#1").as_deref(), Some("F9:import(0)#1"));
        // A face of the component's own feature stays.
        assert_eq!(renames.text("F5:fillet"), None);
        let edge = renames.text("E{F3:bottom|F3:top}").unwrap();
        assert_eq!(edge, "E{F9:import(0)|F9:import(1)}");
        assert_eq!(renames.text("F4.b0"), None);
        assert_eq!(renames.text("Sketch1"), None);
    }

    #[test]
    fn occurrence_paths_are_read_only_from_their_text() {
        assert_eq!(
            occurrence_path("O1/O4"),
            Some(vec![OccurrenceUid(1), OccurrenceUid(4)])
        );
        assert_eq!(occurrence_path(""), None);
        assert_eq!(occurrence_path("F1"), None);
        assert_eq!(occurrence_path("O1/F2"), None);
    }
}
