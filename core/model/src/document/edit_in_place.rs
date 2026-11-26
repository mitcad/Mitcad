// SPDX-License-Identifier: MIT
//! Edit in Place (mitcad#2321, GitHub mitcad/Mitcad#598): a linked
//! component edited inside the assembly that holds it, its changes going
//! to the linked design's own file.
//!
//! Two documents take part: the holder, whose component is linked from a
//! design file, and that design's own document. The holder gives the
//! assembly around the component ([`Document::in_place_context`]): every
//! other body as it is placed now, in the component's coordinates. The
//! linked design shows it while it is edited in place
//! ([`Document::begin_in_place`]): a component of its own (`in_place`, read
//! only, see `DocState::editable`) holding those bodies in a base feature,
//! so that they are drawn, picked and measured as any other geometry. That
//! feature is first in the timeline and not in the timeline the user sees
//! (mitcad#2353, [`Document::in_place_leading`]): the timeline query and
//! the positions of commands are the design's own. It
//! is runtime only: a save leaves it out ([`Document::saved_state`]), undo
//! does not go back past the steps made in the mode, and
//! [`Document::end_in_place`] takes it out of the state and of every step.
//!
//! A pick of that geometry cannot go with the design, which must still open
//! on its own: after each command in the mode, what the design's features
//! refer to in the assembly is frozen where it is now, as Save Component as
//! Design freezes what a component refers to outside it
//! (`save_component.rs`): a sketch on one of its faces lies on a fixed
//! plane, a projection keeps its curves unlinked, a body read becomes a
//! hidden copy ([`Document::freeze_in_place_picks`], in the command's undo
//! step, with a line in words for each). A joint to the assembly cannot be
//! frozen and is refused.
//!
//! When the mode ends, the holder takes the design's bodies as Update Link
//! does, in one undo step `Edit in Place <name>`
//! ([`Document::take_in_place_edit`]); its link records the file's digest
//! when the design is saved, else the one it had, so that the file saved
//! later is offered with Update, unless the holder's own Save wrote it
//! ([`Document::link_saved_in_place`]).

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use super::components::{digest, followed_link};
use super::link_updates::{LinkRead, LinksUpdated};
use super::save_component::{CutKind, CutReference};
use super::{DocState, Document, ModelError, invalid};
use crate::assembly::{ExternalLink, inverse};
use crate::exchange::ImportBody;
use crate::features::{BaseDef, FeatureDef, FeatureEntry, PatternObjects};
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::transform::Transform;

/// The assembly around a linked component, as the holder gives it to the
/// linked design edited in place.
pub struct InPlaceContext<S> {
    /// The linked component in the holder.
    pub component: ComponentUid,
    /// Its name in the holder.
    pub name: String,
    /// The holder's other bodies as they are placed now, in the
    /// component's coordinates (where its first occurrence places it).
    pub bodies: Vec<ImportBody<S>>,
}

/// A design edited in place (runtime only, never saved or undone).
#[derive(Debug, Clone, PartialEq)]
pub struct InPlaceMode {
    /// The holder's name, as the user knows it (its file's name).
    pub holder: String,
    /// The linked component's name in the holder.
    pub name: String,
    /// The component here that shows the assembly around the design.
    pub component: ComponentUid,
    /// Its occurrence, in the root.
    pub occurrence: OccurrenceUid,
    /// Its base feature, holding the assembly's bodies; None when the
    /// assembly has no other body.
    pub feature: Option<FeatureUid>,
    /// The revision as the mode began: the same revision when it ends
    /// means nothing was changed (or every change undone).
    pub revision: u64,
}

impl<K: Kernel> Document<K> {
    /// The link of a component that may be edited in place (mitcad#2321): a
    /// design linked from a file, shown in full. Refused with the reason
    /// in words for anything else: a library part, a component inside a
    /// design linked with its components (mitcad#2209), a design linked
    /// with options (its options are its own) and one shown by its
    /// envelope.
    fn in_place_link(&self, component: ComponentUid) -> Result<&ExternalLink, ModelError> {
        let a = &self.state.assembly;
        if !a.exists(component) || component.is_root() {
            return Err(invalid(format!(
                "{} is not a linked design; only a design linked from a file is edited in place",
                a.name(component)
            )));
        }
        let name = a.name(component);
        if let Some(linked) = a.linked_in(component) {
            return Err(invalid(format!(
                "{name} is part of {}, a linked design: edit {} in place, or open it",
                a.name(linked),
                a.name(linked)
            )));
        }
        let def = a.component(component).expect("exists");
        let Some(link) = def.link.as_ref() else {
            return Err(invalid(format!(
                "{name} is not a linked design; only a design linked from a file is edited in place"
            )));
        };
        if def.library.is_some() {
            return Err(invalid(format!(
                "{name} is a library part; it changes only in its library"
            )));
        }
        if self.state.options_part(component).is_some() {
            return Err(invalid(format!(
                "{name} is linked with options; change its options, or open its design"
            )));
        }
        if link.envelope.is_some() {
            return Err(invalid(format!(
                "{name} is shown by its envelope; show it in full first"
            )));
        }
        Ok(link)
    }

    /// The assembly around a linked component, to edit its design in place
    /// (mitcad#2321): the holder's other visible bodies as they are placed
    /// now, in the coordinates of the component where its first occurrence
    /// places it (of the design's root, where the component stands for
    /// the design's one component, mitcad#2429). The bodies of the component's own occurrences are left
    /// out. Refused for a component that is not edited in place
    /// ([`Document::in_place_link`]).
    pub fn in_place_context(
        &self,
        component: ComponentUid,
    ) -> Result<InPlaceContext<K::Shape>, ModelError> {
        let link = self.in_place_link(component)?;
        // A component that stands for its design's one component
        // (mitcad#2429) lies where the design's root places that one.
        let top = link
            .tree
            .as_ref()
            .and_then(|t| t.top)
            .map_or(Transform::IDENTITY, |t| t.placement);
        let a = &self.state.assembly;
        let path = crate::links::first_path(a, component, None).map_err(invalid)?;
        let into = top.after(&inverse(&self.path_transform(&path)));
        let own = |o: &OccurrenceUid| a.occurrence(*o).is_some_and(|o| o.component == component);
        let mut bodies = Vec::new();
        for instance in self.instances() {
            if !instance.visible || instance.path.iter().any(own) {
                continue;
            }
            let Some(shape) = self.body_shape(instance.body) else {
                continue;
            };
            let transform = into.after(&instance.transform);
            let shape = if transform.is_identity() {
                shape.clone()
            } else {
                self.kernel()
                    .transform_shape(shape, &transform, None)
                    .map_err(|e| invalid(format!("{}: {e}", instance.name)))?
            };
            bodies.push(ImportBody {
                name: Some(instance.name),
                color: None,
                face_colors: Vec::new(),
                shape,
            });
        }
        Ok(InPlaceContext {
            component,
            name: a.name(component),
            bodies,
        })
    }

    /// Starts editing this design in place in `holder` (its name as the
    /// user knows it): the assembly around it (`context`) shown in a read
    /// only component of its own, and the root active. No undo step, and
    /// a design that was saved stays so. Refused while it is edited in
    /// place already.
    pub fn begin_in_place(
        &mut self,
        context: InPlaceContext<K::Shape>,
        holder: &str,
    ) -> Result<InPlaceMode, ModelError> {
        if let Some(mode) = &self.in_place {
            return Err(invalid(format!(
                "this design is edited in place in {} already; finish there first",
                mode.holder
            )));
        }
        let mut next = self.state.clone();
        // On a design with components the root is active (mitcad#2321).
        next.assembly.active = ComponentUid::ROOT;
        let name = next.free_component_name(&format!("{holder} (assembly)"));
        let made = next.assembly.create(Some(&name), None);
        if let Some(c) = next.assembly.component_mut(made) {
            c.in_place = true;
        }
        let occurrence = next
            .assembly
            .place(made, ComponentUid::ROOT, Transform::IDENTITY);
        let feature = if context.bodies.is_empty() {
            None
        } else {
            let bodies = self.base_bodies(&context.bodies)?;
            let uid = next.new_uid();
            let def = FeatureDef::Base(BaseDef {
                source: Some(format!("the assembly around {} in {holder}", context.name)),
                ..BaseDef::new(bodies)
            });
            let entry = FeatureEntry {
                uid,
                name: name.clone(),
                suppressed: false,
                component: made,
                def,
            };
            // First in the timeline (mitcad#2353): the timeline the user
            // sees starts after it ([`Document::in_place_leading`]), so
            // that its positions are the design's own.
            next.features.insert(0, Arc::new(entry));
            next.marker += 1;
            Some(uid)
        };
        self.replace_quietly(next)?;
        let mode = InPlaceMode {
            holder: holder.to_owned(),
            name: context.name,
            component: made,
            occurrence,
            feature,
            revision: self.revision,
        };
        self.in_place = Some(mode.clone());
        Ok(mode)
    }

    /// The design edited in place, while it is (mitcad#2321).
    pub fn in_place_mode(&self) -> Option<&InPlaceMode> {
        self.in_place.as_ref()
    }

    /// The features at the start of the timeline that show the assembly
    /// around the design edited in place (mitcad#2353): its base feature,
    /// which [`Document::begin_in_place`] puts first; none outside the
    /// mode. The timeline the user sees leaves them out and starts after
    /// them: its positions (the marker, an insert or reorder position) are
    /// these plus this number in [`Document::features`].
    pub fn in_place_leading(&self) -> usize {
        self.in_place.as_ref().map_or(0, |mode| {
            self.state
                .features
                .iter()
                .take_while(|f| f.component == mode.component)
                .count()
        })
    }

    /// Freezes what the design's features refer to in the assembly around
    /// it while it is edited in place (mitcad#2321), as Save Component as
    /// Design freezes what a component refers to outside it: run after
    /// each command, the freezing is part of that command's undo step.
    /// Returns a line in words for each reference frozen (or left out);
    /// none outside the mode. A joint or a pattern of the assembly's
    /// occurrence cannot be frozen: the command is undone and refused.
    pub fn freeze_in_place_picks(&mut self) -> Result<Vec<String>, ModelError> {
        let Some(mode) = self.in_place.clone() else {
            return Ok(Vec::new());
        };
        let around = mode.occurrence;
        if self.state.assembly.occurrence(around).is_none() {
            return Ok(Vec::new());
        }
        // Joints and patterns that place the assembly's occurrence.
        let joined = self.state.features[..self.state.marker]
            .iter()
            .filter(|f| f.component != mode.component)
            .find(|f| {
                let patterned = match &f.def {
                    FeatureDef::PatternAtPoints(p) => match &p.objects {
                        PatternObjects::Components { occurrences } => {
                            occurrences.iter().any(|path| path.0.contains(&around))
                        }
                        _ => false,
                    },
                    _ => false,
                };
                patterned || crate::joints::occurrences_used(&f.def).contains(&around)
            })
            .map(|f| f.name.clone());
        if let Some(name) = joined {
            self.undo();
            return Err(invalid(format!(
                "{name} would join or place the assembly around the design, which it cannot \
                 while edited in place; make joints to the assembly in the assembly itself"
            )));
        }
        let inside: BTreeSet<ComponentUid> = std::iter::once(ComponentUid::ROOT)
            .chain(
                self.state
                    .assembly
                    .components
                    .iter()
                    .map(|c| c.uid)
                    .filter(|c| *c != mode.component),
            )
            .collect();
        let is_outside = |p: &[OccurrenceUid]| p.first() == Some(&around);
        let mut next = self.state.clone();
        let mut references = Vec::new();
        self.freeze_scoped(&mut next, None, &is_outside, &inside, &mut references)?;
        if references.is_empty() {
            return Ok(Vec::new());
        }
        let depth = self.undo_depth();
        let label = self.undo_label().unwrap_or("Edit in Place").to_owned();
        let named = label.clone();
        self.apply(move |state| {
            *state = next;
            Ok((named, ()))
        })?;
        if depth > 0 {
            self.merge_undo(depth - 1, &label);
        }
        Ok(references.iter().map(in_place_notice).collect())
    }

    /// Ends editing this design in place (mitcad#2321): the assembly
    /// around it goes from the state and from every undo and redo step,
    /// without an undo step; a design that was saved stays so. Returns
    /// whether the design changed in the mode (false also when every
    /// change was undone); false outside it.
    pub fn end_in_place(&mut self) -> Result<bool, ModelError> {
        let Some(mode) = self.in_place.clone() else {
            return Ok(false);
        };
        let changed = self.revision != mode.revision;
        let state = without_context(&self.state, &mode);
        self.replace_quietly(state)?;
        for step in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            if step.state.assembly.component(mode.component).is_some() {
                step.state = without_context(&step.state, &mode);
            }
        }
        self.in_place = None;
        Ok(changed)
    }

    /// The state a save writes: while the design is edited in place, without
    /// the assembly around it (mitcad#2321).
    pub(crate) fn saved_state(&self) -> Cow<'_, DocState> {
        match &self.in_place {
            Some(mode) => Cow::Owned(without_context(&self.state, mode)),
            None => Cow::Borrowed(&self.state),
        }
    }

    /// Whether an undo or redo step may be taken while the design is edited
    /// in place (mitcad#2321): only the steps made in the mode, whose
    /// states show the assembly.
    pub(super) fn in_place_step_allowed(&self, state: &DocState) -> bool {
        self.in_place
            .as_ref()
            .is_none_or(|mode| state.assembly.component(mode.component).is_some())
    }

    /// The holder takes the design edited in place (mitcad#2321): the
    /// linked component's bodies (with its components, mitcad#2209) become
    /// those of `source`, the design's state at the end of the mode, as
    /// Update Link takes a file, in one undo step `Edit in Place <name>`.
    /// `saved`: the design's file holds `source`, so the link takes the
    /// file's digest; otherwise it keeps its own, and the file saved later
    /// is offered with Update (or recorded by
    /// [`Document::link_saved_in_place`]). `base` is the holder's folder
    /// (the one it was read from when None). A design that cannot be taken
    /// leaves the component as it was, with a line saying why.
    pub fn take_in_place_edit(
        &mut self,
        component: ComponentUid,
        source: &DocState,
        saved: bool,
        base: Option<&Path>,
    ) -> Result<LinksUpdated, ModelError> {
        let link = self.in_place_link(component)?.clone();
        let name = self.state.assembly.name(component);
        let base = base
            .map(Path::to_path_buf)
            .or_else(|| self.source_directory());
        let resolved = followed_link(&link.path, base.as_deref())
            .map_err(|why| invalid(format!("{name}: {}", why.problem())))?;
        let digest = if saved {
            let text = std::fs::read_to_string(&resolved)
                .map_err(|e| invalid(format!("{name}: cannot read {} ({e})", link.path)))?;
            digest(&text)
        } else {
            link.digest.clone()
        };
        let updated_from = format!("{name} is updated from {}", link.path);
        let read = LinkRead {
            component,
            source: Ok(source.clone()),
            resolved: &resolved,
            digest,
            once: false,
        };
        let mut messages = Vec::new();
        let mut trees = Vec::new();
        let mut updates = Vec::new();
        self.read_linked_design(link, read, &mut messages, &mut trees, &mut updates)?;
        for line in &mut messages {
            if *line == updated_from {
                *line = format!("{name} takes the edits made in place");
            }
        }
        if updates.is_empty() && trees.is_empty() {
            return Ok(LinksUpdated {
                updated: Vec::new(),
                messages,
            });
        }
        let label = format!("Edit in Place {name}");
        let before = self.state.clone();
        let more = self.apply(|state| {
            super::link_updates::apply_link_updates(state, updates)?;
            let more = super::link_tree::apply_tree_updates(state, trees)?;
            state.prune_library_sources(&before);
            Ok((label, more))
        })?;
        messages.extend(more);
        Ok(LinksUpdated {
            updated: vec![component],
            messages,
        })
    }

    /// The holder's Save wrote the design it took from editing in place
    /// (mitcad#2321): the link records the file's digest now, without an
    /// undo step, so that the file is not offered as changed. Returns
    /// whether the digest changed.
    pub fn link_saved_in_place(
        &mut self,
        component: ComponentUid,
        base: Option<&Path>,
    ) -> Result<bool, ModelError> {
        let link = self.in_place_link(component)?.clone();
        let name = self.state.assembly.name(component);
        let base = base
            .map(Path::to_path_buf)
            .or_else(|| self.source_directory());
        let resolved = followed_link(&link.path, base.as_deref())
            .map_err(|why| invalid(format!("{name}: {}", why.problem())))?;
        let text = std::fs::read_to_string(&resolved)
            .map_err(|e| invalid(format!("{name}: cannot read {} ({e})", link.path)))?;
        let digest = digest(&text);
        if digest == link.digest {
            return Ok(false);
        }
        if let Some(l) = self
            .state
            .assembly
            .component_mut(component)
            .and_then(|c| c.link.as_mut())
        {
            l.digest = digest;
        }
        self.state_changed();
        Ok(true)
    }

    /// Makes `state` the state without an undo step, computed first; a
    /// design that matched its saved file still does (the change is the
    /// assembly shown while it is edited in place, which a save leaves out).
    fn replace_quietly(&mut self, state: DocState) -> Result<(), ModelError> {
        let result = self.compute(&state)?;
        let saved = self.saved_revision == Some(self.revision);
        self.state = state;
        self.state_changed();
        if saved {
            self.saved_revision = Some(self.revision);
        }
        self.set_result(result);
        Ok(())
    }
}

/// A state without the assembly shown while the design is edited in place:
/// its component, occurrence, base feature and what was set on its bodies;
/// the counters of new components, occurrences and features back where
/// they were when nothing was made after it.
fn without_context(state: &DocState, mode: &InPlaceMode) -> DocState {
    let mut s = state.clone();
    let c = mode.component;
    let gone: BTreeSet<FeatureUid> = s
        .features
        .iter()
        .filter(|f| f.component == c)
        .map(|f| f.uid)
        .collect();
    if !gone.is_empty() {
        let before_marker = s.features[..s.marker]
            .iter()
            .filter(|f| gone.contains(&f.uid))
            .count();
        s.features.retain(|f| !gone.contains(&f.uid));
        s.marker -= before_marker;
        s.body_names.retain(|b, _| !gone.contains(&b.feature));
        s.body_attributes.retain(|b, _| !gone.contains(&b.feature));
        s.feature_visibility.retain(|f, _| !gone.contains(f));
        s.datum_display_sizes.retain(|f, _| !gone.contains(f));
    }
    s.assembly.occurrences.retain(|o| o.component != c);
    s.assembly.components.retain(|x| x.uid != c);
    if s.assembly.active == c {
        s.assembly.active = ComponentUid::ROOT;
    }
    if s.assembly.next_component == c.0 + 1 {
        s.assembly.next_component = c.0;
    }
    if s.assembly.next_occurrence == mode.occurrence.0 + 1 {
        s.assembly.next_occurrence = mode.occurrence.0;
    }
    if let Some(f) = mode.feature
        && s.next_uid == f.0 + 1
    {
        s.next_uid = f.0;
    }
    s
}

/// A line in words for a reference to the assembly frozen while the design
/// is edited in place (mitcad#2321).
fn in_place_notice(reference: &CutReference) -> String {
    let name = &reference.feature_name;
    let what = match reference.kind {
        CutKind::SketchPlane => "its plane on the assembly",
        CutKind::Projection => "its projection of the assembly",
        CutKind::Construction => "its geometry on the assembly",
        CutKind::Body => "a body of the assembly it reads",
        CutKind::MadeBodies => "the bodies the assembly made in it",
        CutKind::CutTarget => "its cut of the assembly",
        CutKind::Joint => "its joint to the assembly",
        CutKind::Point => "its point on the assembly",
        CutKind::Axis => "its axis on the assembly",
    };
    if reference.frozen {
        format!("{name}: {what} is fixed where it is now; it will not follow the assembly")
    } else {
        format!("{name}: {what} is left out; the design must open on its own")
    }
}
