// SPDX-License-Identifier: MIT
//! Appearances kept in the document (mitcad#46): the user's own physically
//! based appearances, next to Mitcad's library (`crate::appearance`).
//! Creating, editing and deleting one is an undo step that recomputes
//! nothing; they are saved in the project file. Bodies refer to an
//! appearance by id (`set_body_appearance`), and so do single faces
//! (`set_face_appearance`, mitcad#53); deleting one gives the bodies that
//! use it the default look again and the faces their body's, in the same
//! step.

use serde_json::{Value, json};

use super::{Document, ModelError, invalid};
use crate::appearance::{self, Appearance, AppearanceChange, Texture};
use crate::ids::BodyUid;
use crate::kernel::Kernel;

impl<K: Kernel> Document<K> {
    /// The document's own appearances, in the order they were made.
    pub fn custom_appearances(&self) -> &[Appearance] {
        &self.state.appearances
    }

    /// An appearance of the library or of the document by id.
    pub fn appearance(&self, id: &str) -> Option<&Appearance> {
        appearance::library_appearance(id)
            .or_else(|| self.state.appearances.iter().find(|a| a.id == id))
    }

    /// The `appearances` query: the library's appearances, then the
    /// document's, each with its parameters, `library` (true for the
    /// built-in ones), its `display_color`, the bodies that use it and the
    /// faces (`{"body", "face"}`). An embedded texture image is listed as
    /// `"embedded": true` with its `image_sha256` instead of its data
    /// (the `appearance_image` query gives it).
    pub(crate) fn appearances_json(&self) -> Value {
        let library = appearance::library().iter().map(|a| (a, true));
        let custom = self.state.appearances.iter().map(|a| (a, false));
        Value::Array(
            library
                .chain(custom)
                .map(|(a, library)| {
                    let mut value = serde_json::to_value(a).expect("appearances serialize");
                    if let (Some(texture), Some(data)) = (
                        value.get_mut("texture").and_then(Value::as_object_mut),
                        a.texture.as_ref().and_then(|t| t.data.as_ref()),
                    ) {
                        texture.remove("data");
                        texture.insert("embedded".to_owned(), json!(true));
                        texture.insert("image_sha256".to_owned(), json!(data.sha256()));
                    }
                    value["library"] = json!(library);
                    value["display_color"] = json!(a.display_color());
                    value["bodies"] = json!(self.bodies_with_appearance(&a.id));
                    value["faces"] = json!(
                        self.faces_with_appearance(&a.id)
                            .into_iter()
                            .map(|(body, face)| json!({"body": body, "face": face}))
                            .collect::<Vec<_>>()
                    );
                    value
                })
                .collect(),
        )
    }

    /// The `appearance_image` query: the embedded texture image of an
    /// appearance, as base64 (`data`), with its `format` (`png`, `jpeg`)
    /// and `sha256`.
    pub(crate) fn appearance_image_json(&self, id: &str) -> Result<Value, ModelError> {
        let a = self
            .appearance(id)
            .ok_or_else(|| invalid(format!("there is no appearance '{id}'")))?;
        let data = a
            .texture
            .as_ref()
            .and_then(|t| t.data.as_ref())
            .ok_or_else(|| invalid(format!("{} has no embedded texture image", a.name)))?;
        let bytes = data.bytes().map_err(invalid)?;
        Ok(json!({
            "data": data.base64(),
            "format": appearance::image_format(&bytes),
            "sha256": data.sha256(),
        }))
    }

    /// Bodies whose appearance is `id` (also those not at the marker).
    fn bodies_with_appearance(&self, id: &str) -> Vec<BodyUid> {
        self.state
            .body_attributes
            .iter()
            .filter(|(_, a)| a.appearance.as_deref() == Some(id))
            .map(|(uid, _)| *uid)
            .collect()
    }

    /// Faces whose appearance is `id`: their bodies and names as assigned.
    fn faces_with_appearance(&self, id: &str) -> Vec<(BodyUid, String)> {
        self.state
            .body_attributes
            .iter()
            .flat_map(|(uid, a)| {
                a.face_appearances
                    .iter()
                    .filter(|(_, appearance)| appearance.as_str() == id)
                    .map(|(face, _)| (*uid, face.clone()))
            })
            .collect()
    }

    /// Makes an appearance of the document from `based_on`'s parameters
    /// (the default look without it) and the given ones. Without an id it
    /// is `custom1`, `custom2`, ... and without a name `Appearance1`, ...
    /// (the next free number). Returns the id.
    pub fn create_appearance(&mut self, change: &AppearanceChange) -> Result<String, ModelError> {
        let mut new = match change.based_on.as_deref() {
            Some(id) => self
                .appearance(id)
                .cloned()
                .ok_or_else(|| invalid(format!("there is no appearance '{id}'")))?,
            None => Appearance::default(),
        };
        let based_on = new.texture.clone();
        let free = (1..)
            .find(|n| {
                self.appearance(&format!("custom{n}")).is_none()
                    && !self.appearance_name_taken(&format!("Appearance{n}"), None)
            })
            .expect("a free number");
        new.id = match change.id.as_deref().map(str::trim) {
            Some(id) => id.to_owned(),
            None => format!("custom{free}"),
        };
        new.name = format!("Appearance{free}");
        change.apply_to(&mut new);
        keep_embedded(&mut new, based_on.as_ref())?;
        if !appearance::valid_id(&new.id) {
            return Err(invalid(format!(
                "'{}' is no appearance id: use letters, digits, '_' and '-'",
                new.id
            )));
        }
        if self.appearance(&new.id).is_some() {
            return Err(invalid(format!("an appearance '{}' exists", new.id)));
        }
        self.check_appearance(&new, None)?;
        let label = format!("Create Appearance {}", new.name);
        let id = new.id.clone();
        self.apply_with(|state| {
            state.appearances.push(new);
            Ok((label, (), false))
        })?;
        Ok(id)
    }

    /// Changes the given parameters or the name of a document appearance;
    /// the library's cannot change (`create_appearance` with `based_on`
    /// copies one). A change that changes nothing is no undo step.
    pub fn edit_appearance(
        &mut self,
        id: &str,
        change: &AppearanceChange,
    ) -> Result<(), ModelError> {
        if change.based_on.is_some() {
            return Err(invalid("based_on is only for create_appearance"));
        }
        if change.id.as_deref().is_some_and(|new| new != id) {
            return Err(invalid("an appearance's id cannot change"));
        }
        let i = self.custom_appearance_index(id)?;
        let mut edited = self.state.appearances[i].clone();
        change.apply_to(&mut edited);
        keep_embedded(&mut edited, self.state.appearances[i].texture.as_ref())?;
        if edited == self.state.appearances[i] {
            return Ok(());
        }
        self.check_appearance(&edited, Some(i))?;
        let label = format!("Edit Appearance {}", self.state.appearances[i].name);
        self.apply_with(|state| {
            state.appearances[i] = edited;
            Ok((label, (), false))
        })
    }

    /// Deletes a document appearance; the bodies that use it get the
    /// default look, the faces that use it their body's. Returns those
    /// bodies and faces.
    pub fn delete_appearance(&mut self, id: &str) -> Result<DeletedAppearance, ModelError> {
        let i = self.custom_appearance_index(id)?;
        let bodies = self.bodies_with_appearance(id);
        let faces = self.faces_with_appearance(id);
        let label = format!("Delete Appearance {}", self.state.appearances[i].name);
        self.apply_with(|state| {
            state.appearances.remove(i);
            for attributes in state.body_attributes.values_mut() {
                if attributes.appearance.as_deref() == Some(id) {
                    attributes.appearance = None;
                }
                attributes.face_appearances.retain(|_, a| a != id);
            }
            state.body_attributes.retain(|_, a| !a.is_default());
            Ok((label, (), false))
        })?;
        Ok(DeletedAppearance { bodies, faces })
    }

    /// The parameters are valid and the name is not another appearance's
    /// (`own`: the index of the one being edited).
    fn check_appearance(
        &self,
        appearance: &Appearance,
        own: Option<usize>,
    ) -> Result<(), ModelError> {
        appearance.check().map_err(invalid)?;
        if self.appearance_name_taken(&appearance.name, own) {
            return Err(invalid(format!(
                "an appearance is named '{}'",
                appearance.name
            )));
        }
        Ok(())
    }

    fn appearance_name_taken(&self, name: &str, own: Option<usize>) -> bool {
        appearance::library().iter().any(|a| a.name == name)
            || self
                .state
                .appearances
                .iter()
                .enumerate()
                .any(|(j, a)| Some(j) != own && a.name == name)
    }

    fn custom_appearance_index(&self, id: &str) -> Result<usize, ModelError> {
        if appearance::library_appearance(id).is_some() {
            return Err(invalid(format!(
                "'{id}' is a library appearance, which cannot change: \
                 create_appearance with based_on copies it"
            )));
        }
        self.state
            .appearances
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| invalid(format!("there is no appearance '{id}'")))
    }
}

/// What deleting an appearance changed: the bodies that used it and the
/// faces (their bodies and names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedAppearance {
    pub bodies: Vec<BodyUid>,
    pub faces: Vec<(BodyUid, String)>,
}

/// A texture given as `"embedded": true` (the `appearances` query's form)
/// keeps the embedded image of the texture it replaces.
fn keep_embedded(appearance: &mut Appearance, old: Option<&Texture>) -> Result<(), ModelError> {
    if let Some(texture) = appearance.texture.as_mut() {
        texture.image_sha256 = None;
    }
    let Some(texture) = appearance.texture.as_mut().filter(|t| t.embedded) else {
        return Ok(());
    };
    texture.embedded = false;
    if texture.data.is_none() {
        texture.data = Some(
            old.and_then(|t| t.data.clone())
                .ok_or_else(|| invalid("texture: there is no embedded image to keep"))?,
        );
    }
    Ok(())
}
