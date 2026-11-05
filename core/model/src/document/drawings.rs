// SPDX-License-Identifier: MIT
//! Drawings kept in the design (mitcad#707, `crate::drawing::def`): the
//! root's and each component's Drawings folder. A drawing stores its
//! definition only; its lines are projected from the model when the
//! `drawing` query asks (`model-api/src/drawings.rs`). Adding, changing and deleting
//! one is an undo step that recomputes nothing.
//!
//! A component's drawings belong to it: a copy of the component (Paste
//! New, Insert Component as a copy) brings copies of them, with their
//! bodies and occurrences mapped, and deleting the component deletes them.

use std::collections::{BTreeMap, HashMap};

use super::{DocState, Document, ModelError, invalid};
use crate::drawing::def::{Drawing, DrawingUid, Subject, SubjectBody};
use crate::features::OccurrencePath;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;

impl DocState {
    /// The next free drawing id.
    pub(crate) fn next_drawing_uid(&self) -> DrawingUid {
        DrawingUid(self.drawings.iter().map(|d| d.uid.0).max().unwrap_or(0) + 1)
    }

    /// The next free `Drawing<n>` of a component's Drawings folder.
    pub(crate) fn next_drawing_name(&self, component: ComponentUid) -> String {
        (1..)
            .map(|n| format!("Drawing{n}"))
            .find(|name| {
                !self
                    .drawings
                    .iter()
                    .any(|d| d.component == component && d.name == *name)
            })
            .expect("a free name")
    }

    /// The drawings of components that are gone go with them.
    pub(crate) fn drop_orphan_drawings(&mut self) {
        let assembly = &self.assembly;
        self.drawings.retain(|d| assembly.exists(d.component));
    }

    /// Moves the drawings of one component into another's Drawings folder
    /// (a made component replaced by an existing one); a name the folder
    /// already has gets the next free `Drawing<n>`.
    pub(crate) fn move_drawings(&mut self, from: ComponentUid, to: ComponentUid) {
        let moving: Vec<DrawingUid> = self
            .drawings
            .iter()
            .filter(|d| d.component == from)
            .map(|d| d.uid)
            .collect();
        for uid in moving {
            let Some(i) = self.drawings.iter().position(|d| d.uid == uid) else {
                continue;
            };
            let clash = self
                .drawings
                .iter()
                .any(|d| d.component == to && d.name == self.drawings[i].name);
            let name = if clash {
                self.next_drawing_name(to)
            } else {
                self.drawings[i].name.clone()
            };
            let drawing = &mut self.drawings[i];
            drawing.component = to;
            drawing.name = name;
        }
    }

    /// The first datum letter not in use: A, B, C, … (I, O and Q left
    /// out), then AA, AB, …; neither a datum label's nor one a frame still
    /// refers to, of the model or of a drawing's views, nor one of `also`.
    pub(crate) fn next_datum_letter(&self, also: &[String]) -> Result<String, String> {
        let letters: Vec<char> = ('A'..='Z')
            .filter(|c| !matches!(c, 'I' | 'O' | 'Q'))
            .collect();
        letters
            .iter()
            .map(|c| c.to_string())
            .chain(
                letters
                    .iter()
                    .flat_map(|a| letters.iter().map(move |b| format!("{a}{b}"))),
            )
            .find(|letter| !also.contains(letter) && !self.datum_letter_used(letter))
            .ok_or_else(|| "every datum letter is in use".to_owned())
    }

    /// Whether a datum label of the model or of a drawing has this letter,
    /// or a frame refers to it.
    fn datum_letter_used(&self, letter: &str) -> bool {
        let named = |list: Vec<String>| list.iter().any(|l| l == letter);
        self.annotations.iter().any(|a| {
            (a.kind == crate::document::MeasureKind::Datum && a.letter.as_deref() == Some(letter))
                || a.frame
                    .as_ref()
                    .is_some_and(|f| f.datums.iter().any(|l| l == letter))
        }) || self
            .drawings
            .iter()
            .any(|d| named(d.datum_letters()) || named(d.frame_datums()))
    }

    /// Whether a component's Drawings folder has any drawing.
    pub(crate) fn has_drawings(&self, component: ComponentUid) -> bool {
        self.drawings.iter().any(|d| d.component == component)
    }

    /// Copies the drawings of the copied components (`components`: each
    /// source component with its copy) into this state, their bodies'
    /// features and occurrences mapped; names stay unique per folder.
    pub(crate) fn copy_drawings(
        &mut self,
        source: &DocState,
        components: &BTreeMap<ComponentUid, ComponentUid>,
        occurrences: &HashMap<OccurrenceUid, OccurrenceUid>,
        features: &HashMap<FeatureUid, FeatureUid>,
    ) {
        let path = |p: &OccurrencePath| {
            OccurrencePath(
                p.0.iter()
                    .map(|o| occurrences.get(o).copied().unwrap_or(*o))
                    .collect(),
            )
        };
        let body = |b: BodyUid| match features.get(&b.feature) {
            Some(feature) => BodyUid::new(*feature, b.index),
            None => b,
        };
        // Annotations' edges and faces are named by the copied features.
        let text: HashMap<String, String> = features
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let renamed = |name: String| super::copy::remap_reference(&name, &text);
        for drawing in &source.drawings {
            let Some(&component) = components.get(&drawing.component) else {
                continue;
            };
            let mut copy = drawing.clone();
            copy.uid = self.next_drawing_uid();
            copy.component = component;
            if self
                .drawings
                .iter()
                .any(|d| d.component == component && d.name == copy.name)
            {
                copy.name = self.next_drawing_name(component);
            }
            for view in copy.sheets.iter_mut().flat_map(|s| s.views.iter_mut()) {
                match &mut view.subject {
                    Some(Subject::Occurrence { occurrence }) => *occurrence = path(occurrence),
                    Some(Subject::Bodies { bodies }) => {
                        for b in bodies {
                            b.body = body(b.body);
                            if let Some(p) = &mut b.occurrence {
                                *p = path(p);
                            }
                        }
                    }
                    Some(Subject::Design) | None => {}
                }
                for pick in view.annotations.iter_mut().flat_map(|a| a.picks.iter_mut()) {
                    pick.body = body(pick.body);
                    if let Some(p) = &mut pick.occurrence {
                        *p = path(p);
                    }
                    if let Some(edge) = &pick.edge
                        && let Ok(mapped) = renamed(edge.to_string()).parse()
                    {
                        pick.edge = Some(mapped);
                    }
                    if let Some(face) = &pick.face
                        && let Ok(mapped) = renamed(face.to_string()).parse()
                    {
                        pick.face = Some(mapped);
                    }
                }
            }
            // A cut sheet's parts (mitcad#853): their bodies, occurrences
            // and faces.
            if let Some(cut) = &mut copy.cut_sheet {
                for part in &mut cut.parts {
                    part.body = body(part.body);
                    if let Some(p) = &mut part.occurrence {
                        *p = path(p);
                    }
                    if let Some(face) = &part.face
                        && let Ok(mapped) = renamed(face.to_string()).parse()
                    {
                        part.face = Some(mapped);
                    }
                }
            }
            self.reletter_datums(&mut copy);
            self.drawings.push(copy);
        }
    }

    /// Gives a copied drawing's datum labels letters of their own where
    /// theirs are in use here already (the model's and the drawings' datum
    /// letters are one set, mitcad#718), and turns the copy's frames that
    /// refer to them to the new letters.
    fn reletter_datums(&self, copy: &mut Drawing) {
        let mut renamed: BTreeMap<String, String> = BTreeMap::new();
        let mut given: Vec<String> = Vec::new();
        for letter in copy.datum_letters() {
            if !self.datum_letter_used(&letter) {
                given.push(letter);
                continue;
            }
            // Free here and not one the copy keeps or was given.
            let mut taken = copy.datum_letters();
            taken.extend(given.iter().cloned());
            if let Ok(new) = self.next_datum_letter(&taken) {
                given.push(new.clone());
                renamed.insert(letter, new);
            }
        }
        copy.reletter(&renamed, true);
    }

    /// The drawings (but `except`) whose frames refer to a relettered
    /// datum (`renamed`: old letter to new), with those frames turned to
    /// the new letters and checked again (mitcad#745): each drawing's
    /// index and its new definition.
    pub(crate) fn drawings_relettered(
        &self,
        renamed: &BTreeMap<String, String>,
        except: Option<DrawingUid>,
    ) -> Result<Vec<(usize, Drawing)>, ModelError> {
        let mut out = Vec::new();
        if renamed.is_empty() {
            return Ok(out);
        }
        for (i, drawing) in self.drawings.iter().enumerate() {
            if Some(drawing.uid) == except {
                continue;
            }
            let mut changed = drawing.clone();
            if changed.reletter(renamed, false) {
                changed.check().map_err(invalid)?;
                out.push((i, changed));
            }
        }
        Ok(out)
    }
}

impl<K: Kernel> Document<K> {
    /// The drawings, in the order they were made.
    pub fn drawings(&self) -> &[Drawing] {
        &self.state.drawings
    }

    /// A drawing by its id (`D1`), or by its name when one drawing has it.
    pub fn find_drawing(&self, text: &str) -> Result<&Drawing, ModelError> {
        if let Ok(uid) = text.parse::<DrawingUid>()
            && let Some(d) = self.state.drawings.iter().find(|d| d.uid == uid)
        {
            return Ok(d);
        }
        let named: Vec<&Drawing> = self
            .state
            .drawings
            .iter()
            .filter(|d| d.name == text.trim())
            .collect();
        match named.as_slice() {
            [one] => Ok(one),
            [] => Err(invalid(format!("there is no drawing “{text}”"))),
            _ => Err(invalid(format!(
                "several drawings are named “{text}”: give its id (D1, ...)"
            ))),
        }
    }

    /// The drawing (other than `except`) one of whose views has a datum
    /// label of this letter: the drawings' datum labels and the model's
    /// are one set of letters (mitcad#717).
    pub fn drawing_with_datum(&self, letter: &str, except: Option<DrawingUid>) -> Option<&Drawing> {
        self.state
            .drawings
            .iter()
            .filter(|d| Some(d.uid) != except)
            .find(|d| d.datum_letters().iter().any(|l| l == letter))
    }

    /// Every datum letter a datum label has, the model's and the
    /// drawings', each once, sorted: what a frame can refer to.
    pub fn all_datum_letters(&self) -> Vec<String> {
        let mut letters: Vec<String> = self
            .annotations()
            .iter()
            .filter(|a| a.kind == crate::document::MeasureKind::Datum)
            .filter_map(|a| a.letter.clone())
            .chain(self.state.drawings.iter().flat_map(Drawing::datum_letters))
            .collect();
        letters.sort_by(|a, b| (a.len(), a).cmp(&(b.len(), b)));
        letters.dedup();
        letters
    }

    /// The next free name in a component's Drawings folder.
    pub fn next_drawing_name(&self, component: ComponentUid) -> String {
        self.state.next_drawing_name(component)
    }

    /// Adds a drawing to a component's Drawings folder (the root's for
    /// `C0`); an undo step `Add Drawing1` that recomputes nothing. Its id
    /// and, when it has none, its name are given here. Returns its id.
    pub fn add_drawing(&mut self, mut drawing: Drawing) -> Result<DrawingUid, ModelError> {
        self.state.editable(drawing.component)?;
        drawing.uid = self.state.next_drawing_uid();
        if drawing.name.trim().is_empty() {
            drawing.name = self.state.next_drawing_name(drawing.component);
        }
        drawing.name = drawing.name.trim().to_owned();
        self.check_drawing_name(&drawing, None)?;
        drawing.check().map_err(invalid)?;
        let uid = drawing.uid;
        let label = format!("Add {}", drawing.name);
        self.apply_with(|state| {
            state.drawings.push(drawing);
            Ok((label, (), false))
        })?;
        Ok(uid)
    }

    /// Replaces a drawing with a changed one (its id and folder stay); an
    /// undo step under `label` unless nothing changes.
    pub fn set_drawing(
        &mut self,
        uid: DrawingUid,
        label: String,
        mut changed: Drawing,
    ) -> Result<(), ModelError> {
        let i = self.drawing_index(uid)?;
        let old = &self.state.drawings[i];
        self.state.editable(old.component)?;
        changed.uid = uid;
        changed.component = old.component;
        changed.name = changed.name.trim().to_owned();
        if changed == *old {
            return Ok(());
        }
        self.check_drawing_name(&changed, Some(uid))?;
        changed.check().map_err(invalid)?;
        self.apply_with(|state| {
            state.drawings[i] = changed;
            Ok((label, (), false))
        })
    }

    /// Replaces a drawing with a changed one whose datum labels took new
    /// letters (`renamed`: old letter to new), as one undo step under
    /// `label`: the frames that refer to the old letters follow them, the
    /// drawing's own, the other drawings' and the model's (mitcad#745).
    pub fn set_drawing_relettered(
        &mut self,
        uid: DrawingUid,
        label: String,
        mut changed: Drawing,
        renamed: &BTreeMap<String, String>,
    ) -> Result<(), ModelError> {
        if renamed.is_empty() {
            return self.set_drawing(uid, label, changed);
        }
        let i = self.drawing_index(uid)?;
        let old = &self.state.drawings[i];
        self.state.editable(old.component)?;
        changed.uid = uid;
        changed.component = old.component;
        changed.name = changed.name.trim().to_owned();
        changed.reletter(renamed, false);
        self.check_drawing_name(&changed, Some(uid))?;
        changed.check().map_err(invalid)?;
        let others = self.state.drawings_relettered(renamed, Some(uid))?;
        let mut annotations = self.state.annotations.clone();
        for annotation in &mut annotations {
            if let Some(frame) = &mut annotation.frame {
                for datum in &mut frame.datums {
                    if let Some(new) = renamed.get(datum.as_str()) {
                        datum.clone_from(new);
                    }
                }
            }
        }
        for (j, annotation) in annotations.iter().enumerate() {
            if *annotation != self.state.annotations[j] {
                annotation.check().map_err(invalid)?;
            }
        }
        self.apply_with(|state| {
            state.drawings[i] = changed;
            for (j, drawing) in others {
                state.drawings[j] = drawing;
            }
            state.annotations = annotations;
            Ok((label, (), false))
        })
    }

    /// Deletes a drawing; an undo step `Delete Drawing1`.
    pub fn delete_drawing(&mut self, uid: DrawingUid) -> Result<(), ModelError> {
        let i = self.drawing_index(uid)?;
        self.state.editable(self.state.drawings[i].component)?;
        let label = format!("Delete {}", self.state.drawings[i].name);
        self.apply_with(|state| {
            state.drawings.remove(i);
            Ok((label, (), false))
        })
    }

    fn drawing_index(&self, uid: DrawingUid) -> Result<usize, ModelError> {
        self.state
            .drawings
            .iter()
            .position(|d| d.uid == uid)
            .ok_or_else(|| invalid(format!("there is no drawing {uid}")))
    }

    fn check_drawing_name(
        &self,
        drawing: &Drawing,
        own: Option<DrawingUid>,
    ) -> Result<(), ModelError> {
        if drawing.name.is_empty() {
            return Err(invalid("a drawing needs a name"));
        }
        let taken = self.state.drawings.iter().any(|d| {
            Some(d.uid) != own && d.component == drawing.component && d.name == drawing.name
        });
        if taken {
            return Err(invalid(format!(
                "{} already has a drawing {}",
                self.state.assembly.name(drawing.component),
                drawing.name
            )));
        }
        Ok(())
    }

    /// The bodies of `component` that the 3D view shows at the timeline
    /// marker, as a base view placed with the whole component records them
    /// (mitcad#1293): each where an occurrence places it, a path from the
    /// component (none for the component's own bodies), in the order the
    /// design places them. A body shows when it is visible and so is every
    /// occurrence on its path below the component. None when the component
    /// is not placed in the design.
    pub fn shown_subject_bodies(&self, component: ComponentUid) -> Option<Vec<SubjectBody>> {
        let assembly = &self.state.assembly;
        let prefix = assembly.paths_to(component).into_iter().next()?;
        let shown_path = |path: &[OccurrenceUid]| {
            path.iter()
                .all(|o| assembly.occurrence(*o).is_some_and(|o| o.visible))
        };
        Some(
            self.instances()
                .into_iter()
                .filter(|i| i.path.starts_with(&prefix))
                .filter(|i| {
                    self.body_attributes(i.body).visible && shown_path(&i.path[prefix.len()..])
                })
                .map(|i| {
                    let below = i.path[prefix.len()..].to_vec();
                    SubjectBody {
                        body: i.body,
                        occurrence: (!below.is_empty()).then_some(OccurrencePath(below)),
                    }
                })
                .collect(),
        )
    }

    /// Records as their subject what the views of a design file written
    /// before mitcad#1293 show (base views, and laid out sections, whose
    /// subject is the whole component: its bodies shown in the 3D view):
    /// the bodies shown now, so that hiding, showing or adding components
    /// no longer changes them. No undo step and no change to save: the
    /// design file is read so. Each drawing pinned leaves a note
    /// ([`Document::take_drawing_notes`]).
    pub(crate) fn pin_drawing_subjects(&mut self) {
        if !self
            .state
            .drawings
            .iter()
            .any(Drawing::has_unrecorded_subject)
        {
            return;
        }
        let mut drawings = self.state.drawings.clone();
        let mut notes = Vec::new();
        for drawing in drawings.iter_mut().filter(|d| d.has_unrecorded_subject()) {
            // A component not placed shows nothing.
            let bodies = self
                .shown_subject_bodies(drawing.component)
                .unwrap_or_default();
            for view in drawing.sheets.iter_mut().flat_map(|s| s.views.iter_mut()) {
                if view.has_unrecorded_subject() {
                    view.subject = Some(Subject::Bodies {
                        bodies: bodies.clone(),
                    });
                }
            }
            let name = if drawing.component.is_root() {
                drawing.name.clone()
            } else {
                format!(
                    "{} of {}",
                    drawing.name,
                    self.state.assembly.name(drawing.component)
                )
            };
            let shown = match bodies.len() {
                1 => "1 body".to_owned(),
                n => format!("{n} bodies"),
            };
            notes.push(format!(
                "{name} now keeps what its views showed when the design was opened ({shown}): \
                 hiding, showing or adding components no longer changes them"
            ));
        }
        self.state.drawings = drawings;
        self.drawing_notes.extend(notes);
    }

    /// The notes of the drawings pinned since they were last taken
    /// (mitcad#1293: one per drawing of a design file written before
    /// it, information for the user).
    pub fn take_drawing_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.drawing_notes)
    }
}

/// The views a view's deletion takes with it: itself and those that come
/// from it.
pub fn deleted_views(sheet: &crate::drawing::def::DrawingSheet, id: u32) -> Vec<u32> {
    let mut out = vec![id];
    out.extend(sheet.dependents(id));
    out
}
