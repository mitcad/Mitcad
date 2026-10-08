// SPDX-License-Identifier: MIT
//! Body attributes: visibility (the browser's light bulb), physical material and
//! appearance of each body, and the appearances of single faces
//! (mitcad#53). They are part of the definition state, so they are saved
//! with the body's name and undo restores them; they do not change the
//! geometry.
//!
//! A face appearance names its face by topological name, as features
//! refer to faces, so it follows the face through recomputes: a later
//! feature that changes the face keeps its name, pieces of a split face
//! (`#k`) all keep it. When a change of the sketch renames the face (a
//! side face's segment ends on other curves), the name still finds the
//! face of the same feature and role whose key has most in common with it
//! (the same curve for a side face), as a profile's region key does; the
//! name kept is the one assigned.

use std::collections::BTreeMap;
use std::str::FromStr;

use super::{Document, ModelError, invalid};
use crate::analysis::{self, DEFAULT_MATERIAL, MATERIALS};
use crate::ids::BodyUid;
use crate::kernel::{Kernel, KernelError};
use crate::topo::{CurveId, FaceName, RoleKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyAttributes {
    pub visible: bool,
    /// A material id from [`analysis::MATERIALS`]; None for the default
    /// material (steel).
    pub material: Option<String>,
    /// An appearance id for display; None for the material's appearance.
    pub appearance: Option<String>,
    /// Appearances of single faces, which override the body's: appearance
    /// ids by the face's topological name as it was assigned (canonical
    /// text).
    pub face_appearances: BTreeMap<String, String>,
}

impl Default for BodyAttributes {
    fn default() -> Self {
        Self {
            visible: true,
            material: None,
            appearance: None,
            face_appearances: BTreeMap::new(),
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

    /// The material that gives the body its density.
    pub fn material(&self) -> &'static analysis::Material {
        self.material
            .as_deref()
            .and_then(analysis::material)
            .or_else(|| analysis::material(DEFAULT_MATERIAL))
            .expect("the default material exists")
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

    /// Shows or hides a body at the timeline marker.
    pub fn set_body_visible(&mut self, uid: BodyUid, visible: bool) -> Result<(), ModelError> {
        let verb = if visible { "Show" } else { "Hide" };
        self.change_body(uid, verb, |a| {
            a.visible = visible;
            Ok(())
        })
    }

    /// Sets a body's material by id, or back to the default with None.
    pub fn set_body_material(
        &mut self,
        uid: BodyUid,
        material: Option<&str>,
    ) -> Result<(), ModelError> {
        if let Some(id) = material
            && analysis::material(id).is_none()
        {
            let known: Vec<&str> = MATERIALS.iter().map(|m| m.id).collect();
            return Err(invalid(format!(
                "unknown material '{id}' (known: {})",
                known.join(", ")
            )));
        }
        self.change_body(uid, "Set Material of", |a| {
            a.material = material.map(str::to_owned);
            Ok(())
        })
    }

    /// Sets a body's appearance by id, or back to its material's with None.
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
        let shape = self
            .body_shape(uid)
            .ok_or_else(|| invalid(format!("body {uid} does not exist at the timeline marker")))?;
        let mut names = Vec::with_capacity(faces.len());
        for face in faces {
            let name = FaceName::from_str(face.trim())
                .map_err(|e| invalid(format!("'{face}' is no face name: {e}")))?;
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
        }
        let count = names.len();
        let what = if count == 1 {
            "Face".to_owned()
        } else {
            format!("{count} Faces")
        };
        let verb = if appearance.is_some() {
            format!("Set Appearance of {what} of")
        } else {
            format!("Clear Appearance of {what} of")
        };
        self.set_attributes(uid, &verb, attributes)
    }

    /// Takes the appearances of all faces of a body away.
    pub fn clear_face_appearances(&mut self, uid: BodyUid) -> Result<(), ModelError> {
        if self.body_shape(uid).is_none() {
            return Err(invalid(format!(
                "body {uid} does not exist at the timeline marker"
            )));
        }
        let mut attributes = self.body_attributes(uid);
        attributes.face_appearances.clear();
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

    /// Sets a body's attributes as one undo step that recomputes nothing
    /// (none when they do not change).
    fn set_attributes(
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

/// A name of each face that `reference` finds among the faces (each face's
/// names): those it names (all pieces without `#k`), else those of the same
/// feature and role whose key has most in common with it.
fn faces_named(faces: &[Vec<FaceName>], reference: &FaceName) -> Vec<String> {
    let exact: Vec<String> = faces
        .iter()
        .filter_map(|names| names.iter().find(|n| n.matches(reference)))
        .map(ToString::to_string)
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    let scored: Vec<(usize, &FaceName)> = faces
        .iter()
        .filter_map(|names| {
            names
                .iter()
                .filter_map(|n| likeness(reference, n).map(|score| (score, n)))
                .max_by_key(|(score, _)| *score)
        })
        .collect();
    let Some(best) = scored.iter().map(|(score, _)| *score).max() else {
        return Vec::new();
    };
    scored
        .into_iter()
        .filter(|(score, _)| *score == best)
        .map(|(_, n)| n.to_string())
        .collect()
}

/// How much a face's name has in common with a reference that names no
/// face: None unless the feature and the role are the same and the keys
/// share a curve (a side face: its own curve), else the number of curves
/// they share.
fn likeness(reference: &FaceName, name: &FaceName) -> Option<usize> {
    if reference.feature != name.feature || reference.role != name.role {
        return None;
    }
    let curves = |key: &Option<RoleKey>| -> Option<Vec<CurveId>> {
        match key {
            Some(RoleKey::Segment(segment)) => Some(segment.curves().collect()),
            Some(RoleKey::Region(region)) => {
                Some(region.segments().flat_map(|s| s.curves()).collect())
            }
            _ => None,
        }
    };
    match (&reference.key, &name.key) {
        (Some(RoleKey::Segment(a)), Some(RoleKey::Segment(b))) if a.curve != b.curve => None,
        (Some(RoleKey::Segment(_)), Some(RoleKey::Segment(_)))
        | (Some(RoleKey::Region(_)), Some(RoleKey::Region(_))) => {
            let a = curves(&reference.key)?;
            let b = curves(&name.key)?;
            let shared = a.iter().filter(|c| b.contains(c)).count();
            (shared > 0).then_some(shared)
        }
        (None, None) => Some(1),
        _ => None,
    }
}
