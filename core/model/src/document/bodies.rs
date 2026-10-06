// SPDX-License-Identifier: MIT
//! Body attributes: visibility (the browser's light bulb), physical material and
//! appearance of each body. They are part of the definition state, so they
//! are saved with the body's name and undo restores them; they do not
//! change the geometry.

use super::{Document, ModelError, invalid};
use crate::analysis::{self, DEFAULT_MATERIAL, MATERIALS};
use crate::ids::BodyUid;
use crate::kernel::Kernel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyAttributes {
    pub visible: bool,
    /// A material id from [`analysis::MATERIALS`]; None for the default
    /// material (steel).
    pub material: Option<String>,
    /// An appearance id for display; None for the material's appearance.
    pub appearance: Option<String>,
}

impl Default for BodyAttributes {
    fn default() -> Self {
        Self {
            visible: true,
            material: None,
            appearance: None,
        }
    }
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
