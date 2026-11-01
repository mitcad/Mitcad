// SPDX-License-Identifier: MIT
//! Body attributes: visibility (the browser's light bulb), transparency
//! (mitcad#1093), physical material and
//! appearance of each body, the appearances of single faces (mitcad#53)
//! and the print bed face (mitcad#731, `print_bed.rs`). They are part of the definition state, so they are saved
//! with the body's name and undo restores them; they do not change the
//! geometry.
//!
//! A face appearance names its face by topological name, as features
//! refer to faces, so it follows the face through recomputes: a later
//! feature that changes the face keeps its name, pieces of a split face
//! (`#k`) all keep it. When a change of the sketch renames the face (a
//! side face's segment ends on other curves), the name still finds the
//! face of the same feature and role whose key has most in common with it
//! (the same curve for a side face), as a profile's region key does and
//! features' references do (`features/renamed.rs`, which has the rule);
//! unlike a feature's reference, a tie finds every face of it. The name
//! kept is the one assigned.

use std::collections::BTreeMap;
use std::str::FromStr;

use super::{Document, ModelError, invalid};
use crate::features::renamed::faces_found;
use crate::ids::BodyUid;
use crate::kernel::{Kernel, KernelError};
use crate::topo::FaceName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyAttributes {
    pub visible: bool,
    /// A material id (built in or the document's, `crate::material`); None
    /// for its component's default material (steel when none is set).
    pub material: Option<String>,
    /// An appearance id for display; None for the default look. Setting
    /// the material sets it to the material's (mitcad#640).
    pub appearance: Option<String>,
    /// Appearances of single faces, which override the body's: appearance
    /// ids by the face's topological name as it was assigned (canonical
    /// text).
    pub face_appearances: BTreeMap<String, String>,
    /// The planar face that lies on the print bed (mitcad#731,
    /// `print_bed.rs`): its topological name as it was assigned (canonical
    /// text), found as a face appearance's is. None: the design's −Z is
    /// down.
    pub print_bed_face: Option<String>,
    // Bindings (mitcad#800, `bindings.rs`): expressions of parameters that
    // the values above follow; None (or no entry) for a value set by hand.
    /// A text: an appearance id, else a unique appearance name.
    pub appearance_expr: Option<String>,
    /// A text: a material id, else a unique material name.
    pub material_expr: Option<String>,
    /// A condition: the body is shown while it holds.
    pub visible_when: Option<String>,
    /// Appearances of single faces from expressions, by the face's
    /// topological name as assigned (canonical text).
    pub face_appearance_exprs: BTreeMap<String, String>,
    /// Drawn see-through (mitcad#1093): a display state like `visible`,
    /// independent of it; the appearance and material stay as they are.
    pub transparent: bool,
}

impl Default for BodyAttributes {
    fn default() -> Self {
        Self {
            visible: true,
            material: None,
            appearance: None,
            face_appearances: BTreeMap::new(),
            print_bed_face: None,
            appearance_expr: None,
            material_expr: None,
            visible_when: None,
            face_appearance_exprs: BTreeMap::new(),
            transparent: false,
        }
    }
}

/// A face appearance of a body and the faces it applies to now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceAppearance {
    /// The face's name as assigned.
    pub face: String,
    pub appearance: String,
    /// A name of each face of the body at the marker the assigned name
    /// finds (its own, or that of the face renamed since); empty when the
    /// face is gone.
    pub faces: Vec<String>,
}

impl BodyAttributes {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl<K: Kernel> Document<K> {
    /// The attributes of a body (the defaults when none were set).
    pub fn body_attributes(&self, uid: BodyUid) -> BodyAttributes {
        self.state
            .body_attributes
            .get(&uid)
            .cloned()
            .unwrap_or_default()
    }

    /// Shows or hides a body at the timeline marker; a condition it
    /// followed (mitcad#800) is removed in the same undo step.
    pub fn set_body_visible(&mut self, uid: BodyUid, visible: bool) -> Result<(), ModelError> {
        let verb = if visible { "Show" } else { "Hide" };
        self.change_body(uid, verb, |a| {
            a.visible = visible;
            a.visible_when = None;
            Ok(())
        })
    }

    /// Makes a body transparent or opaque again at the timeline marker
    /// (mitcad#1093): one undo step (`Make Body1 Transparent`, `Make Body1
    /// Opaque`) that recomputes nothing, none when nothing changes. Its
    /// visibility, appearance and material stay as they are.
    pub fn set_body_transparent(
        &mut self,
        uid: BodyUid,
        transparent: bool,
    ) -> Result<(), ModelError> {
        if self.body_shape(uid).is_none() {
            return Err(invalid(format!(
                "body {uid} does not exist at the timeline marker"
            )));
        }
        let mut attributes = self.body_attributes(uid);
        if attributes.transparent == transparent {
            return Ok(());
        }
        attributes.transparent = transparent;
        let label = transparency_label(&self.body_name(uid), transparent);
        self.apply_with(|state| {
            if attributes.is_default() {
                state.body_attributes.remove(&uid);
            } else {
                state.body_attributes.insert(uid, attributes);
            }
            Ok((label, (), false))
        })
    }

    /// Sets a body's material by id (a built-in one or the document's), or
    /// back to its component's default with None; its appearance stays as
    /// it is (`set_body_material_with` can give it the material's,
    /// mitcad#640).
    pub fn set_body_material(
        &mut self,
        uid: BodyUid,
        material: Option<&str>,
    ) -> Result<(), ModelError> {
        self.set_body_material_with(uid, material, super::AppearanceChoice::Keep)
    }

    /// Sets a body's appearance by id, or back to its material's with None;
    /// an expression it followed (mitcad#800) is removed in the same undo
    /// step.
    pub fn set_body_appearance(
        &mut self,
        uid: BodyUid,
        appearance: Option<&str>,
    ) -> Result<(), ModelError> {
        self.change_body(uid, "Set Appearance of", |a| {
            if appearance.is_some_and(|id| id.trim().is_empty()) {
                return Err(invalid("the appearance id is empty"));
            }
            a.appearance = appearance.map(str::to_owned);
            a.appearance_expr = None;
            Ok(())
        })
    }

    /// Gives faces of a body an appearance by id, or with None takes their
    /// own away (they show the body's again): one undo step, which
    /// recomputes nothing, unless nothing changes. The faces are
    /// topological names of faces of the body at the marker.
    pub fn set_face_appearance(
        &mut self,
        uid: BodyUid,
        faces: &[String],
        appearance: Option<&str>,
    ) -> Result<(), ModelError> {
        if faces.is_empty() {
            return Err(invalid("no faces are given"));
        }
        if appearance.is_some_and(|id| id.trim().is_empty()) {
            return Err(invalid("the appearance id is empty"));
        }
        let names = self.checked_faces(uid, faces)?;
        let mut attributes = self.body_attributes(uid);
        for name in &names {
            match appearance {
                Some(id) => {
                    attributes
                        .face_appearances
                        .insert(name.clone(), id.to_owned());
                }
                None => {
                    attributes.face_appearances.remove(name);
                }
            }
            // A value set by hand ends the face's binding (mitcad#800).
            attributes.face_appearance_exprs.remove(name);
        }
        let verb = if appearance.is_some() {
            format!("Set Appearance of {} of", faces_label(names.len()))
        } else {
            format!("Clear Appearance of {} of", faces_label(names.len()))
        };
        self.set_attributes(uid, &verb, attributes)
    }

    /// The canonical names of faces of a body at the marker, each checked
    /// to find a face there.
    pub(super) fn checked_faces(
        &self,
        uid: BodyUid,
        faces: &[String],
    ) -> Result<Vec<String>, ModelError> {
        if faces.is_empty() {
            return Err(invalid("no faces are given"));
        }
        let shape = self
            .body_shape(uid)
            .ok_or_else(|| invalid(format!("body {uid} does not exist at the timeline marker")))?;
        let mut names = Vec::with_capacity(faces.len());
        for face in faces {
            let name = FaceName::from_str(face.trim())
                .map_err(|e| invalid(format!("{face} is no face name: {e}")))?;
            match self.kernel().count_faces(shape, &name) {
                Ok(0) => {
                    return Err(invalid(format!(
                        "{} has no face {name} at the timeline marker",
                        self.body_name(uid)
                    )));
                }
                Ok(_) | Err(KernelError::Unsupported(_)) => {}
                Err(e) => return Err(invalid(e.to_string())),
            }
            names.push(name.to_string());
        }
        Ok(names)
    }

    /// Takes the appearances of all faces of a body away, those that
    /// followed expressions too.
    pub fn clear_face_appearances(&mut self, uid: BodyUid) -> Result<(), ModelError> {
        if self.body_shape(uid).is_none() {
            return Err(invalid(format!(
                "body {uid} does not exist at the timeline marker"
            )));
        }
        let mut attributes = self.body_attributes(uid);
        attributes.face_appearances.clear();
        attributes.face_appearance_exprs.clear();
        self.set_attributes(uid, "Clear Face Appearances of", attributes)
    }

    /// The face appearances of a body, each with the faces it applies to at
    /// the marker (by their names; see the module documentation for how a
    /// renamed face is found). A body that is not at the marker has its
    /// faces' appearances with no faces.
    pub fn face_appearances(&self, uid: BodyUid) -> Vec<FaceAppearance> {
        let assigned = self
            .state
            .body_attributes
            .get(&uid)
            .map(|a| &a.face_appearances);
        let Some(assigned) = assigned.filter(|a| !a.is_empty()) else {
            return Vec::new();
        };
        // The names of the body's faces, each face's names together.
        let faces: Vec<Vec<FaceName>> = self
            .body_shape(uid)
            .and_then(|shape| self.kernel().faces(shape).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|face| {
                face.names
                    .iter()
                    .filter_map(|n| FaceName::from_str(n).ok())
                    .collect()
            })
            .collect();
        assigned
            .iter()
            .map(|(face, appearance)| FaceAppearance {
                face: face.clone(),
                appearance: appearance.clone(),
                faces: FaceName::from_str(face)
                    .map(|name| faces_named(&faces, &name))
                    .unwrap_or_default(),
            })
            .collect()
    }

    /// The faces of a body's shape that have appearances of their own, by
    /// their index in [`Kernel::faces`]' order: (appearance id, face
    /// indices) per face appearance that finds faces, as
    /// [`Document::face_appearances`] finds them.
    pub(crate) fn face_appearance_indices(&self, uid: BodyUid) -> Vec<(String, Vec<usize>)> {
        let assigned = self
            .state
            .body_attributes
            .get(&uid)
            .map(|a| &a.face_appearances);
        let Some(assigned) = assigned.filter(|a| !a.is_empty()) else {
            return Vec::new();
        };
        let faces: Vec<Vec<FaceName>> = self
            .body_shape(uid)
            .and_then(|shape| self.kernel().faces(shape).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|face| {
                face.names
                    .iter()
                    .filter_map(|n| FaceName::from_str(n).ok())
                    .collect()
            })
            .collect();
        assigned
            .iter()
            .filter_map(|(face, appearance)| {
                let name = FaceName::from_str(face).ok()?;
                let found: Vec<usize> = faces_found(&faces, &name)
                    .into_iter()
                    .map(|(i, _)| i)
                    .collect();
                (!found.is_empty()).then(|| (appearance.clone(), found))
            })
            .collect()
    }

    /// Sets a body's attributes as one undo step that recomputes nothing
    /// (none when they do not change).
    pub(super) fn set_attributes(
        &mut self,
        uid: BodyUid,
        verb: &str,
        attributes: BodyAttributes,
    ) -> Result<(), ModelError> {
        if attributes == self.body_attributes(uid) {
            return Ok(());
        }
        let label = format!("{verb} {}", self.body_name(uid));
        self.apply_with(|state| {
            if attributes.is_default() {
                state.body_attributes.remove(&uid);
            } else {
                state.body_attributes.insert(uid, attributes);
            }
            Ok((label, (), false))
        })
    }

    /// Changes the attributes of a body that exists at the marker; an
    /// unchanged value adds no undo step.
    fn change_body(
        &mut self,
        uid: BodyUid,
        verb: &str,
        change: impl FnOnce(&mut BodyAttributes) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        if self.body_shape(uid).is_none() {
            return Err(invalid(format!(
                "body {uid} does not exist at the timeline marker"
            )));
        }
        let mut attributes = self.body_attributes(uid);
        change(&mut attributes)?;
        if attributes == self.body_attributes(uid) {
            return Ok(());
        }
        let label = format!("{verb} {}", self.body_name(uid));
        self.apply(|state| {
            if attributes.is_default() {
                state.body_attributes.remove(&uid);
            } else {
                state.body_attributes.insert(uid, attributes);
            }
            Ok((label, ()))
        })
    }
}

/// The undo label of a change of transparency (mitcad#1093): `Make Body1
/// Transparent`, `Make Bracket:1 Opaque`.
pub(super) fn transparency_label(name: &str, transparent: bool) -> String {
    let state = if transparent { "Transparent" } else { "Opaque" };
    format!("Make {name} {state}")
}

/// `Face` or `3 Faces`, for undo labels.
pub(super) fn faces_label(count: usize) -> String {
    if count == 1 {
        "Face".to_owned()
    } else {
        format!("{count} Faces")
    }
}

/// A name of each face that `reference` finds among the faces (each face's
/// names): those it names (all pieces without `#k`), else those of the same
/// feature and role whose key has most in common with it.
pub(super) fn faces_named(faces: &[Vec<FaceName>], reference: &FaceName) -> Vec<String> {
    faces_found(faces, reference)
        .into_iter()
        .map(|(_, name)| name.to_string())
        .collect()
}
