// SPDX-License-Identifier: MIT
//! Linked parts of a design (mitcad#2130).
//!
//! A linked insert takes the whole design, or only chosen parts of it: the
//! occurrences of the design (each by its path from the design's root, with
//! the components in it) and the bodies of its root that were ticked
//! (`insert_component`'s `parts`). The link records them
//! (`ExternalLink::parts`), and reading the file again (`update_links`,
//! `update_link`, Show as Envelope) takes only their visible bodies, placed
//! as the design places them. A body lies in the most particular chosen
//! part it is in: an occurrence chosen inside a chosen one is a part of
//! its own.
//!
//! A chosen part the design no longer has keeps the bodies saved with the
//! link, with the message "Bracket:1 is no longer in board.mitcad; its
//! saved shape is kept". Each part's bodies keep their numbers in the base
//! feature (`<feature>.b<number>`) and their face names where they can, so
//! that what uses one part keeps working when another one is taken out or
//! added.
//!
//! `set_link_parts` (Choose Linked Parts...) changes the choice in one undo
//! step. The joint origins on a part no longer chosen keep the frames they
//! had (`ExternalLink::unlinked`), with a warning in words on their joint;
//! choosing the part again lets them follow it again.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;

use super::components::{digest, file_label};
use super::copy::CopySource;
use super::linked_looks::SourceLook;
use super::mounts::SourceMount;
use super::{DocState, Document, ModelError, invalid};
use crate::assembly::{Assembly, EnvelopeFrame, Instance, LinkPart, same_parts};
use crate::exchange::ImportBody;
use crate::features::{BaseBody, FeatureDef, GeomRef};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::recompute::{Cache, recompute};

/// The parts of a design to link (`insert_component`'s and
/// `set_link_parts`' `parts`): occurrence paths from the design's root
/// (`O1/O4`, or names: `Arm:1/Pin:2`) and body uids of its root (`F3.b0`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkPartsInput {
    #[serde(default)]
    pub occurrences: Vec<String>,
    #[serde(default)]
    pub bodies: Vec<String>,
}

/// The bodies a design gives (all of them, or those of the chosen parts),
/// from one evaluation (`Document::design_shapes_chosen`).
pub(super) struct ChosenShapes<S> {
    pub bodies: Vec<ImportBody<S>>,
    pub mounts: Vec<SourceMount>,
    /// What the design gives each body, when asked for.
    pub looks: Vec<SourceLook>,
    /// Per body, the chosen part it lies in (0 for the whole design).
    pub part_of: Vec<usize>,
    /// Per body, its place among the bodies the whole design gives.
    pub whole_index: Vec<u32>,
    /// Per chosen part, whether the design still has it.
    pub found: Vec<bool>,
    /// Per chosen part, its name in the design (the saved one when gone).
    pub names: Vec<String>,
}

/// The chosen parts of a design read for a link: the base feature's
/// bodies (each numbered, with its first face), its mount frames, what the
/// design gives the bodies it gave now (one per body that is not kept, in
/// order), the parts with their names and body numbers now, and a line per
/// part gone.
pub(super) struct ChosenRead {
    pub bodies: Vec<BaseBody>,
    pub mounts: Vec<SourceMount>,
    pub looks: Vec<SourceLook>,
    pub parts: Vec<LinkPart>,
    pub messages: Vec<String>,
}

/// What the `link_parts` query answers: the design's occurrences and its
/// root's bodies, to choose from, and the parts a linked component holds.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkPartsTree {
    /// The design file's name.
    pub file: String,
    pub occurrences: Vec<LinkPartNode>,
    /// The bodies of the design's root at its marker: uid, name, shown.
    pub bodies: Vec<(BodyUid, String, bool)>,
    /// The linked component's parts (None: the whole design, or no
    /// component asked about).
    pub chosen: Option<Vec<LinkPart>>,
}

/// An occurrence of a design to choose from, with those in it.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkPartNode {
    pub uid: OccurrenceUid,
    pub name: String,
    pub path: Vec<OccurrenceUid>,
    pub visible: bool,
    pub children: Vec<LinkPartNode>,
}

/// What [`Document::set_link_parts`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPartsSet {
    /// False when the component already linked those parts (no undo step).
    pub changed: bool,
    /// The parts it links now (None: the whole design).
    pub parts: Option<Vec<LinkPart>>,
    /// A line per chosen part the design no longer has.
    pub messages: Vec<String>,
}

/// Whether the design whose assembly is `assembly` still has the part:
/// its occurrence path from the root, or a body of its root (`root_bodies`).
pub(super) fn part_in(assembly: &Assembly, root_bodies: &[BodyUid], part: &LinkPart) -> bool {
    if let Some(body) = part.body {
        return part.occurrence.is_empty() && root_bodies.contains(&body);
    }
    if part.occurrence.is_empty() {
        return false;
    }
    let mut parent = ComponentUid::ROOT;
    for o in &part.occurrence {
        match assembly.occurrence(*o) {
            Some(found) if found.parent == parent => parent = found.component,
            _ => return false,
        }
    }
    true
}

/// The chosen part a placed body lies in: the most particular occurrence
/// on its path, or the root body it is; None when it lies in none.
pub(super) fn part_of(parts: &[LinkPart], found: &[bool], instance: &Instance) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    for (k, part) in parts.iter().enumerate() {
        if !found.get(k).copied().unwrap_or(false) {
            continue;
        }
        let depth = match part.body {
            Some(body) if instance.path.is_empty() && instance.body == body => usize::MAX,
            Some(_) => continue,
            None if !part.occurrence.is_empty() && instance.path.starts_with(&part.occurrence) => {
                part.occurrence.len()
            }
            None => continue,
        };
        if best.is_none_or(|(_, d)| depth > d) {
            best = Some((k, depth));
        }
    }
    best.map(|(k, _)| k)
}

/// The parts `input` names in the design `source`, in their order, each
/// once. An occurrence path's steps are uids or names (an occurrence's,
/// or a component's placed once there); bodies are uids of the root's
/// bodies (checked when the design is evaluated). Refused: no parts, and
/// an occurrence the design does not have.
pub(super) fn resolve_parts(
    source: &DocState,
    input: &LinkPartsInput,
) -> Result<Vec<LinkPart>, ModelError> {
    if input.occurrences.is_empty() && input.bodies.is_empty() {
        return Err(invalid(
            "choose at least one occurrence or body of the design to link",
        ));
    }
    let assembly = &source.assembly;
    let mut parts: Vec<LinkPart> = Vec::new();
    for text in &input.occurrences {
        let mut parent = ComponentUid::ROOT;
        let mut path = Vec::new();
        for step in text.split('/').map(str::trim) {
            let children: Vec<(OccurrenceUid, ComponentUid)> = assembly
                .children(parent)
                .map(|o| (o.uid, o.component))
                .collect();
            let by_uid = step.parse::<OccurrenceUid>().ok();
            let mut found: Vec<(OccurrenceUid, ComponentUid)> = children
                .iter()
                .copied()
                .filter(|(uid, _)| Some(*uid) == by_uid || assembly.occurrence_name(*uid) == step)
                .collect();
            if found.is_empty() {
                found = children
                    .iter()
                    .copied()
                    .filter(|(_, c)| assembly.name(*c) == step)
                    .collect();
            }
            match found.as_slice() {
                [(uid, component)] => {
                    path.push(*uid);
                    parent = *component;
                }
                [] => {
                    return Err(invalid(format!(
                        "the design to link has no occurrence {text}"
                    )));
                }
                _ => {
                    return Err(invalid(format!(
                        "{step} names more than one occurrence of the design to link; name \
                         one by its number"
                    )));
                }
            }
        }
        let part = LinkPart {
            occurrence: path,
            body: None,
            name: String::new(),
            bodies: Vec::new(),
            gone: false,
        };
        if !parts.iter().any(|p| p.same_part(&part)) {
            parts.push(part);
        }
    }
    for text in &input.bodies {
        let body: BodyUid = text
            .trim()
            .parse()
            .map_err(|_| invalid(format!("{text} is not a body id like F3.b0")))?;
        let part = LinkPart {
            occurrence: Vec::new(),
            body: Some(body),
            name: String::new(),
            bodies: Vec::new(),
            gone: false,
        };
        if !parts.iter().any(|p| p.same_part(&part)) {
            parts.push(part);
        }
    }
    Ok(parts)
}

/// `parts` chosen again over `old`: a part chosen before keeps its place,
/// name and body numbers; new ones come after, in their order.
fn merged_parts(old: Option<&[LinkPart]>, parts: Vec<LinkPart>) -> Vec<LinkPart> {
    let old = old.unwrap_or_default();
    let mut merged: Vec<LinkPart> = old
        .iter()
        .filter(|o| parts.iter().any(|p| p.same_part(o)))
        .cloned()
        .collect();
    for part in parts {
        if !merged.iter().any(|m| m.same_part(&part)) {
            merged.push(part);
        }
    }
    merged
}

/// The message for a chosen part the design no longer has: in words, no
/// ids or paths.
fn gone_message(name: &str, path: &str, kept: bool) -> String {
    let name = if name.trim().is_empty() {
        "A chosen item"
    } else {
        name
    };
    let file = file_label(path);
    if kept {
        format!("{name} is no longer in {file}; its saved shape is kept")
    } else {
        format!("{name} is no longer in {file}; it has no saved shape to keep")
    }
}

/// The body of `bodies` whose id is `<feature>.b<number>`.
fn numbered(bodies: &[BaseBody], number: u32) -> Option<&BaseBody> {
    bodies
        .iter()
        .zip(0u32..)
        .find(|(b, i)| b.number.unwrap_or(*i) == number)
        .map(|(b, _)| b)
}

impl<K: Kernel> Document<K> {
    /// The chosen `parts` of the design `source` (read from `path`, the
    /// images of its appearances through `origin`) as the bodies of a
    /// link's base feature (see the module docs). `previous`: the base
    /// feature's bodies now, each with its number and first face (None
    /// for a new link, or one shown by its envelope); a part gone keeps
    /// its bodies from there. `whole`: `previous` holds the whole design as
    /// this text gives it, so that a part's bodies take the numbers and
    /// face names they had there.
    pub(super) fn chosen_parts(
        &self,
        source: &DocState,
        path: &str,
        origin: &CopySource<'_>,
        parts: &[LinkPart],
        previous: Option<(&[BaseBody], bool)>,
    ) -> Result<ChosenRead, ModelError> {
        let shapes = self.design_shapes_chosen(source, path, Some(origin), Some(parts))?;
        let fresh = self.base_bodies(&shapes.bodies)?;
        let mut faces = Vec::with_capacity(shapes.bodies.len());
        for body in &shapes.bodies {
            let count = self
                .kernel
                .face_count(&body.shape)
                .map_err(|e| invalid(format!("{path}: {e}")))?;
            faces.push(u32::try_from(count).map_err(|_| invalid("too many faces"))?);
        }
        let (before, whole) = previous.unwrap_or((&[], false));
        let mut read = ChosenRead {
            bodies: Vec::new(),
            mounts: shapes.mounts,
            looks: Vec::new(),
            parts: Vec::new(),
            messages: Vec::new(),
        };
        let mut next_number = 0u32;
        let mut next_face = 0u32;
        // Takes a body at the first number and first face it can keep.
        let mut place =
            |body: &mut BaseBody, number: Option<u32>, first: Option<u32>, count: u32| {
                let number = number.filter(|n| *n >= next_number).unwrap_or(next_number);
                let first = first.filter(|f| *f >= next_face).unwrap_or(next_face);
                body.number = Some(number);
                body.first_face = Some(first);
                next_number = number.saturating_add(1);
                next_face = first.saturating_add(count);
                number
            };
        for (k, part) in parts.iter().enumerate() {
            let mut numbers = Vec::new();
            let mut gone = false;
            if shapes.found.get(k).copied().unwrap_or(false) {
                let mine = (0..fresh.len()).filter(|i| shapes.part_of[*i] == k);
                for (j, i) in mine.enumerate() {
                    let mut body = fresh[i].clone();
                    // The number it had: as this part's, else in the whole
                    // design.
                    let had = part.bodies.get(j).copied().or_else(|| {
                        (whole && part.bodies.is_empty()).then_some(shapes.whole_index[i])
                    });
                    let first = had
                        .and_then(|n| numbered(before, n))
                        .and_then(|b| b.first_face);
                    numbers.push(place(&mut body, had, first, faces[i]));
                    read.bodies.push(body);
                    if let Some(look) = shapes.looks.get(i) {
                        read.looks.push(look.clone());
                    }
                }
            } else {
                gone = true;
                let saved: Vec<BaseBody> = part
                    .bodies
                    .iter()
                    .filter_map(|n| numbered(before, *n).cloned())
                    .collect();
                read.messages
                    .push(gone_message(&part.name, path, !saved.is_empty()));
                for mut body in saved {
                    let count = self.saved_faces(&body)?;
                    let (number, first) = (body.number, body.first_face);
                    numbers.push(place(&mut body, number, first, count));
                    read.bodies.push(body);
                }
            }
            read.parts.push(LinkPart {
                name: shapes.names.get(k).cloned().unwrap_or_default(),
                bodies: numbers,
                gone,
                ..part.clone()
            });
        }
        if read.bodies.is_empty() {
            return Err(invalid(format!(
                "the chosen parts of {} have no bodies to show",
                file_label(path)
            )));
        }
        Ok(read)
    }

    /// How many faces a saved base-feature body has, read as its base
    /// feature reads it.
    fn saved_faces(&self, body: &BaseBody) -> Result<u32, ModelError> {
        let data = body
            .brep
            .data()
            .ok_or_else(|| invalid("the saved shape of a linked part is missing"))?;
        self.kernel
            .import_brep(FeatureUid(0), data, 0)
            .map(|(_, faces)| faces)
            .map_err(|e| invalid(format!("the saved shape of a linked part: {e}")))
    }

    /// The design a linked component, or the file `path`, holds, to choose
    /// its parts from (the `link_parts` query): its occurrences and its
    /// root's bodies, and the parts the component links. `base`: the
    /// design file's folder (None: the folder it was read from).
    pub fn link_parts_tree(
        &self,
        component: Option<ComponentUid>,
        path: Option<&str>,
        base: Option<&Path>,
    ) -> Result<LinkPartsTree, ModelError> {
        let base = base
            .map(Path::to_path_buf)
            .or_else(|| self.source_directory());
        let (file, chosen) = match (component, path) {
            (Some(uid), None) => {
                let link = self.plain_design_link(uid)?;
                (linked_file(&link.path, base.as_deref())?, link.parts)
            }
            (None, Some(path)) => (linked_file(path, base.as_deref())?, None),
            _ => {
                return Err(invalid(
                    "link_parts needs a linked component or a path, not both",
                ));
            }
        };
        let label = file.to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&file)
            .map_err(|e| invalid(format!("cannot read {}: {e}", file_label(&label))))?;
        let (source, _) = crate::file::load_state_at(&text, &file)
            .map_err(|e| invalid(format!("{}: {e}", file_label(&label))))?;
        let result = recompute(
            &self.kernel,
            &source,
            &mut Cache::default(),
            self.monitor.as_ref(),
        )?;
        let names = super::display_names(&source, &result);
        let bodies = result
            .owners
            .iter()
            .filter(|(_, owner)| owner.is_root())
            .map(|(b, _)| {
                let shown = source.body_attributes.get(b).is_none_or(|a| a.visible);
                (*b, names.get(b).cloned().unwrap_or_default(), shown)
            })
            .collect();
        Ok(LinkPartsTree {
            file: file_label(&label),
            occurrences: part_nodes(&source.assembly, ComponentUid::ROOT, &[], 0),
            bodies,
            chosen,
        })
    }

    /// The link of a component linked from a design file as it is (not a
    /// library part, not a design linked with options).
    fn plain_design_link(
        &self,
        component: ComponentUid,
    ) -> Result<crate::assembly::ExternalLink, ModelError> {
        let def = self
            .state
            .assembly
            .component(component)
            .ok_or_else(|| invalid(format!("component {component} does not exist")))?;
        let name = &def.name;
        if def.library.is_some() || self.state.options_part(component).is_some() {
            return Err(invalid(format!(
                "{name} is a library part or a design linked with options; only a design linked \
                 as it is links chosen parts"
            )));
        }
        def.link.clone().ok_or_else(|| {
            invalid(format!(
                "{name} is not a linked component; only a linked design links chosen parts"
            ))
        })
    }

    /// Changes the parts of its design a linked component holds (Choose
    /// Linked Parts..., mitcad#2130): `parts` (None: the whole design), in
    /// one undo step `Choose Linked Parts of <name>`; nothing changes, and
    /// there is no undo step, when it holds them already. The file is read
    /// again from the link's path against `base` (the design file's folder;
    /// None: the folder it was read from), as Show as Envelope reads it:
    /// an action of its own, also where opening the design would not
    /// follow the link. A part chosen before keeps its bodies' numbers; a part
    /// chosen before that the design no longer has keeps its saved bodies,
    /// with a message; a new one must be in the design. The joint origins
    /// on the bodies of a part no longer chosen keep the frames they have
    /// now (see the module docs).
    pub fn set_link_parts(
        &mut self,
        component: ComponentUid,
        parts: Option<&LinkPartsInput>,
        base: Option<&Path>,
    ) -> Result<LinkPartsSet, ModelError> {
        let link = self.plain_design_link(component)?;
        let name = self.state.assembly.name(component);
        // Kept with its components (mitcad#2209): the parent's joints may
        // join them, which chosen parts would take apart.
        if link.tree.is_some() {
            return Err(invalid(format!(
                "{name} keeps the components of its design; to link chosen parts of it, insert \
                 the design again with Choose parts"
            )));
        }
        let folder = base
            .map(Path::to_path_buf)
            .or_else(|| self.source_directory());
        let file = linked_file(&link.path, folder.as_deref())?;
        let text = std::fs::read_to_string(&file)
            .map_err(|e| invalid(format!("cannot read {} ({e})", link.path)))?;
        let (source, _) = crate::file::load_state_at(&text, &file)
            .map_err(|e| invalid(format!("{}: {e}", link.path)))?;
        let wanted = match parts {
            Some(input) => Some(merged_parts(
                link.parts.as_deref(),
                resolve_parts(&source, input)?,
            )),
            None => None,
        };
        if same_parts(link.parts.as_deref(), wanted.as_deref()) {
            return Ok(LinkPartsSet {
                changed: false,
                parts: link.parts,
                messages: Vec::new(),
            });
        }
        let position = self.state.require(link.feature)?;
        let FeatureDef::Base(base_def) = &self.state.features[position].def else {
            return Err(invalid(format!("{name}'s linked bodies are missing")));
        };
        // The bodies now, each with its number and first face, unless
        // they are an envelope.
        let before = match link.envelope {
            Some(_) => Vec::new(),
            None => self.numbered_bodies(link.feature, &base_def.bodies),
        };
        let origin = CopySource::Document(file.parent());
        let unchanged = digest(&text) == link.digest;
        let mut messages = Vec::new();
        let (bodies, mounts, looks, chosen) = match (&wanted, link.envelope) {
            (Some(wanted), None) => {
                let whole = link.parts.is_none() && unchanged;
                let read = self.chosen_parts(
                    &source,
                    &link.path,
                    &origin,
                    wanted,
                    Some((before.as_slice(), whole)),
                )?;
                // A part chosen now must be in the design.
                for (part, read_part) in wanted.iter().zip(&read.parts) {
                    let new = link
                        .parts
                        .as_deref()
                        .is_none_or(|old| !old.iter().any(|o| o.same_part(part)));
                    if new && read_part.gone {
                        return Err(invalid(format!(
                            "{} is not in {}",
                            part_label(part),
                            file_label(&link.path)
                        )));
                    }
                }
                messages = read.messages;
                (read.bodies, read.mounts, read.looks, Some(read.parts))
            }
            (Some(wanted), Some(detail)) => {
                let shapes = self.design_shapes_chosen(&source, &link.path, None, Some(wanted))?;
                let parts: Vec<LinkPart> = wanted
                    .iter()
                    .zip(&shapes.names)
                    .map(|(p, n)| LinkPart {
                        name: n.clone(),
                        ..p.clone()
                    })
                    .collect();
                let (bodies, mounts, _) =
                    self.envelope_of(shapes.bodies, shapes.mounts, &link.path, Some(detail))?;
                (bodies, mounts, Vec::new(), Some(parts))
            }
            (None, None) => {
                let (bodies, mounts, looks) =
                    self.design_parts_looked(&source, &link.path, &origin)?;
                (bodies, mounts, looks, None)
            }
            (None, Some(detail)) => {
                let (bodies, mounts, _) =
                    self.design_envelope(&source, &link.path, Some(detail))?;
                (bodies, mounts, Vec::new(), None)
            }
        };
        // The joint origins on bodies that go keep their frames now.
        let numbers: BTreeSet<u32> = bodies
            .iter()
            .zip(0u32..)
            .map(|(b, i)| b.number.unwrap_or(i))
            .collect();
        let frames = match link.envelope {
            Some(_) => Vec::new(),
            None => {
                crate::joints::envelope_frames(&self.kernel, &self.state, &self.result, component)
            }
        };
        let feature = link.feature;
        let going: Vec<EnvelopeFrame> = frames
            .into_iter()
            .filter(|f| frame_body(&f.geometry).is_some_and(|b| b.feature == feature))
            .filter(|f| frame_body(&f.geometry).is_some_and(|b| !numbers.contains(&b.index)))
            .collect();
        let label = format!("Choose Linked Parts of {name}");
        let digest = digest(&text);
        let kept: BTreeSet<u32> = chosen
            .iter()
            .flatten()
            .filter(|p| p.gone)
            .flat_map(|p| p.bodies.iter().copied())
            .collect();
        let saved_parts = chosen.map(|parts| {
            parts
                .into_iter()
                .map(|p| LinkPart { gone: false, ..p })
                .collect::<Vec<_>>()
        });
        let reply_parts = saved_parts.clone();
        self.apply(|state| {
            super::mounts::update_linked_mounts(state, component, &mounts)?;
            let position = state.require(feature)?;
            let mut entry = (*state.features[position]).clone();
            if let FeatureDef::Base(base) = &mut entry.def {
                let mut bodies = bodies;
                if link.envelope.is_none() {
                    give_chosen_looks(state, feature, &base.bodies, &mut bodies, &looks, &kept);
                }
                base.bodies = bodies;
            }
            state.features[position] = Arc::new(entry);
            if let Some(l) = state
                .assembly
                .component_mut(component)
                .and_then(|c| c.link.as_mut())
            {
                l.digest = digest;
                l.parts = saved_parts;
                // Frames of origins whose bodies are back are dropped;
                // those of bodies that went are kept.
                l.unlinked.retain(|f| {
                    frame_body(&f.geometry).is_none_or(|b| !numbers.contains(&b.index))
                });
                for frame in going {
                    if !l.unlinked.iter().any(|f| f.geometry == frame.geometry) {
                        l.unlinked.push(frame);
                    }
                }
            }
            Ok((label, ()))
        })?;
        Ok(LinkPartsSet {
            changed: true,
            parts: reply_parts,
            messages,
        })
    }

    /// A base feature's bodies, each with its number and first face: the
    /// numbers and first faces they keep, else their places, the faces of
    /// the bodies before them counted where the last recompute has them.
    pub(super) fn numbered_bodies(
        &self,
        feature: FeatureUid,
        bodies: &[BaseBody],
    ) -> Vec<BaseBody> {
        let mut next_face = Some(0u32);
        bodies
            .iter()
            .zip(0u32..)
            .map(|(body, i)| {
                let mut body = body.clone();
                let number = body.number.unwrap_or(i);
                body.number = Some(number);
                if let Some(first) = body.first_face {
                    next_face = Some(first);
                }
                body.first_face = next_face;
                let faces = self
                    .body_shape(BodyUid::new(feature, number))
                    .and_then(|shape| self.kernel.face_count(shape).ok())
                    .and_then(|count| u32::try_from(count).ok());
                next_face = match (next_face, faces) {
                    (Some(first), Some(count)) => first.checked_add(count),
                    _ => None,
                };
                body
            })
            .collect()
    }
}

/// The occurrences placed in `parent` of a design, each with those in it.
fn part_nodes(
    assembly: &Assembly,
    parent: ComponentUid,
    above: &[OccurrenceUid],
    depth: usize,
) -> Vec<LinkPartNode> {
    if depth > 64 {
        return Vec::new();
    }
    assembly
        .children(parent)
        .map(|o| {
            let mut path = above.to_vec();
            path.push(o.uid);
            LinkPartNode {
                uid: o.uid,
                name: assembly.occurrence_name(o.uid),
                children: part_nodes(assembly, o.component, &path, depth + 1),
                path,
                visible: o.visible,
            }
        })
        .collect()
}

/// The file a link's `path` names, against `base` (a directory) when it is
/// relative.
fn linked_file(path: &str, base: Option<&Path>) -> Result<std::path::PathBuf, ModelError> {
    std::path::absolute(super::components::resolve_path(path, base))
        .map_err(|e| invalid(format!("cannot resolve {path}: {e}")))
}

/// Refuses parts a design to link does not have (a new link's).
pub(super) fn check_found(parts: &[LinkPart], path: &str) -> Result<(), ModelError> {
    match parts.iter().find(|p| p.gone) {
        Some(part) => Err(invalid(format!(
            "{} is not in {}",
            part_label(part),
            file_label(path)
        ))),
        None => Ok(()),
    }
}

/// A part as a command names it: its occurrence path or body uid.
fn part_label(part: &LinkPart) -> String {
    match part.body {
        Some(body) => body.to_string(),
        None => part
            .occurrence
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("/"),
    }
}

/// The body whose face, edge or vertex a joint origin's geometry is.
fn frame_body(geometry: &GeomRef) -> Option<BodyUid> {
    match geometry {
        GeomRef::Face { body, .. } | GeomRef::Edge { body, .. } | GeomRef::Vertex { body, .. } => {
            Some(*body)
        }
        _ => None,
    }
}

/// The bodies `after` of a linked base feature `feature` take what their
/// design gives them (`looks`, one per body that is not kept), as
/// `DocState::give_linked_looks` gives them; the kept bodies (their
/// numbers in `kept`: saved bodies of parts gone) keep theirs.
pub(super) fn give_chosen_looks(
    state: &mut DocState,
    feature: FeatureUid,
    before: &[BaseBody],
    after: &mut [BaseBody],
    looks: &[SourceLook],
    kept: &BTreeSet<u32>,
) {
    if kept.is_empty() {
        state.give_linked_looks(feature, before, after, looks);
        return;
    }
    let is_fresh = |b: &BaseBody| b.number.is_none_or(|n| !kept.contains(&n));
    let places: Vec<usize> = (0..after.len()).filter(|i| is_fresh(&after[*i])).collect();
    let mut bodies: Vec<BaseBody> = places.iter().map(|i| after[*i].clone()).collect();
    state.give_linked_looks(feature, before, &mut bodies, looks);
    for (i, body) in places.into_iter().zip(bodies) {
        after[i] = body;
    }
}
