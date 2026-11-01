// SPDX-License-Identifier: MIT
//! Appearances kept in the document (mitcad#46): the user's own physically
//! based appearances, next to Mitcad's library (`crate::appearance`).
//! Creating, editing and deleting one is an undo step that recomputes
//! nothing; they are saved in the design file. Bodies refer to an
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
    /// built-in ones), its `display_color`, its `class` (its own, a
    /// library's by its id, else `other`; mitcad#1102), the bodies that use
    /// it and the faces (`{"body", "face"}`). An embedded texture image (of the base
    /// colour or another map, mitcad#313) is listed as `"embedded": true`
    /// with its `image_sha256` instead of its data (the `appearance_image`
    /// query gives it).
    pub fn appearances_json(&self) -> Value {
        let library = appearance::library().iter().map(|a| (a, true));
        let custom = self.state.appearances.iter().map(|a| (a, false));
        Value::Array(
            library
                .chain(custom)
                .map(|(a, library)| {
                    let mut value = serde_json::to_value(a).expect("appearances serialize");
                    if let (Some(texture), Some(t)) = (value.get_mut("texture"), &a.texture) {
                        appearance::each_image_json(texture, |map, image| {
                            if let Some(data) = t.image_data(map) {
                                image.remove("data");
                                image.insert("embedded".to_owned(), json!(true));
                                image.insert("image_sha256".to_owned(), json!(data.sha256()));
                            }
                        });
                    }
                    value["library"] = json!(library);
                    value["display_color"] = json!(a.display_color());
                    // Its class, its own or worked out (mitcad#1102).
                    value["class"] = json!(a.material_class().id());
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

    /// The `appearance_image` query: an embedded image of an appearance's
    /// texture, the base colour's or that of another map (`map`,
    /// mitcad#313), as base64 (`data`), with its `format` (`png`, `jpeg`)
    /// and `sha256`.
    pub fn appearance_image_json(&self, id: &str, map: Option<&str>) -> Result<Value, ModelError> {
        let a = self
            .appearance(id)
            .ok_or_else(|| invalid(format!("there is no appearance {id}")))?;
        let map = map.unwrap_or(appearance::BASE_COLOR);
        if map != appearance::BASE_COLOR && !appearance::MAPS.contains(&map) {
            return Err(invalid(format!(
                "“{map}” is no map: base_color, normal, roughness, metalness or occlusion"
            )));
        }
        let data = a
            .texture
            .as_ref()
            .and_then(|t| t.image_data(map))
            .ok_or_else(|| {
                invalid(if map == appearance::BASE_COLOR {
                    format!("{} has no embedded texture image", a.name)
                } else {
                    format!("{} has no embedded {map} image", a.name)
                })
            })?;
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
                .ok_or_else(|| invalid(format!("there is no appearance {id}")))?,
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
        self.keep_embedded(&mut new, based_on.as_ref())?;
        if !appearance::valid_id(&new.id) {
            return Err(invalid(format!(
                "{} is no appearance id: use letters, digits, '_' and '-'",
                new.id
            )));
        }
        if self.appearance(&new.id).is_some() {
            return Err(invalid(format!("an appearance {} exists", new.id)));
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
        let old = self.state.appearances[i].texture.clone();
        self.keep_embedded(&mut edited, old.as_ref())?;
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
            // The materials' lists and the components' choices of it
            // (mitcad#1055).
            for material in &mut state.materials {
                material.appearances.retain(|a| a != id);
            }
            state.component_appearances.retain(|_, a| a != id);
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
                "an appearance is named {}",
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
                "{id} is a library appearance, which cannot change: \
                 create_appearance with based_on copies it"
            )));
        }
        self.state
            .appearances
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| invalid(format!("there is no appearance {id}")))
    }
}

/// What deleting an appearance changed: the bodies that used it and the
/// faces (their bodies and names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedAppearance {
    pub bodies: Vec<BodyUid>,
    pub faces: Vec<(BodyUid, String)>,
}

impl<K: Kernel> Document<K> {
    /// An image of the texture given as `"embedded": true` (the
    /// `appearances` query's form) keeps an embedded image: with
    /// `image_sha256`, the document's image of that digest (also another
    /// appearance's, mitcad#313); without it, the image of the same map in
    /// the texture it replaces (`old`). Then every embedded image the
    /// document already has is shared, not copied.
    fn keep_embedded(
        &self,
        appearance: &mut Appearance,
        old: Option<&Texture>,
    ) -> Result<(), ModelError> {
        let Some(texture) = appearance.texture.as_mut() else {
            return Ok(());
        };
        let mut images = appearance::embedded_images(&self.state.appearances);
        if let Some(old) = old {
            for (_, data) in old.images() {
                images
                    .entry(data.sha256().to_owned())
                    .or_insert_with(|| data.clone());
            }
        }
        for slot in texture.slots_mut() {
            let what = appearance::slot_name(slot.map);
            if slot.image.is_some() {
                return Err(invalid(format!(
                    "{what}: `image` is for design files only: send `\"embedded\": true` \
                     with `image_sha256`"
                )));
            }
            let digest = slot.image_sha256.take();
            if !std::mem::take(slot.embedded) || slot.data.is_some() {
                continue;
            }
            *slot.data = Some(match digest {
                Some(digest) => images.get(&digest).cloned().ok_or_else(|| {
                    invalid(format!(
                        "{what}: the design embeds no image with the SHA-256 {digest}"
                    ))
                })?,
                None => old
                    .and_then(|t| t.image_data(slot.map))
                    .cloned()
                    .ok_or_else(|| {
                        invalid(format!("{what}: there is no embedded image to keep"))
                    })?,
            });
        }
        appearance::share_images(texture, &mut images);
        Ok(())
    }
}
