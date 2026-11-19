// SPDX-License-Identifier: MIT
//! The document's render settings (mitcad#47, `crate::render_settings`):
//! changed by undoable commands that recompute nothing, saved in the
//! design file.

use serde_json::{Map, Value};

use super::{Document, ModelError, invalid};
use crate::kernel::Kernel;
use crate::render_settings::{
    Light, LightKind, MAX_LIGHTS, MAX_SHOTS, RenderSettings, RenderSettingsChange, Shot,
};

impl<K: Kernel> Document<K> {
    pub fn render_settings(&self) -> &RenderSettings {
        &self.state.render
    }

    /// The `render_settings` query: every section with every field (the
    /// optional `image` and `height` when set).
    pub fn render_settings_json(&self) -> Value {
        let mut value =
            serde_json::to_value(&self.state.render).expect("render settings serialize");
        // The shots are listed when there are none too (a file leaves them
        // out then).
        if let Some(object) = value.as_object_mut() {
            object
                .entry("shots")
                .or_insert_with(|| Value::Array(Vec::new()));
        }
        value
    }

    /// Changes the given fields; a change that changes nothing is no undo
    /// step. Returns whether anything changed.
    pub fn set_render_settings(
        &mut self,
        change: &RenderSettingsChange,
    ) -> Result<bool, ModelError> {
        let next = self.state.render.changed(change).map_err(invalid)?;
        self.replace_render_settings(next, "Change Render Settings")
    }

    /// The default settings again (one undo step unless they are already).
    /// The shots (mitcad#2013) are cameras, not settings: they stay.
    pub fn reset_render_settings(&mut self) -> Result<bool, ModelError> {
        let next = RenderSettings {
            shots: self.state.render.shots.clone(),
            ..RenderSettings::default()
        };
        self.replace_render_settings(next, "Reset Render Settings")
    }

    /// Adds a light (mitcad#54) with the given fields (`type` first: a
    /// kind's own power when none is given), named `Light<n>` with the id
    /// `light<n>` unless given; one undo step. Returns its id.
    pub fn add_render_light(&mut self, fields: &Map<String, Value>) -> Result<String, ModelError> {
        let render = &self.state.render;
        if render.lights.len() >= MAX_LIGHTS {
            return Err(invalid(format!(
                "a design keeps at most {MAX_LIGHTS} lights"
            )));
        }
        let free = (1..)
            .find(|n| {
                render.light(&format!("light{n}")).is_none()
                    && !render.lights.iter().any(|l| l.name == format!("Light{n}"))
            })
            .expect("a free number");
        let mut light = Light {
            id: match fields.get("id") {
                Some(Value::String(id)) => id.trim().to_owned(),
                Some(other) => return Err(invalid(format!("a light's id is text, got {other}"))),
                None => format!("light{free}"),
            },
            name: format!("Light{free}"),
            ..Light::default()
        };
        if render.light(&light.id).is_some() {
            return Err(invalid(format!("a light {} exists", light.id)));
        }
        let kind: LightKind = match fields.get("type") {
            Some(kind) => {
                serde_json::from_value(kind.clone()).map_err(|e| invalid(format!("lights: {e}")))?
            }
            None => LightKind::Point,
        };
        light.kind = kind;
        light.power = kind.default_power();
        let mut rest = fields.clone();
        rest.remove("id");
        let light = light.changed(&rest).map_err(invalid)?;
        let label = format!("Add Light {}", light.name);
        let id = light.id.clone();
        self.apply_with(|state| {
            state.render.lights.push(light);
            Ok((label, (), false))
        })?;
        Ok(id)
    }

    /// Changes the given fields of a light (null: the field's default); a
    /// change that changes nothing is no undo step. Returns whether
    /// anything changed.
    pub fn edit_render_light(
        &mut self,
        id: &str,
        fields: &Map<String, Value>,
    ) -> Result<bool, ModelError> {
        let index = self.light_index(id)?;
        let light = &self.state.render.lights[index];
        let next = light.changed(fields).map_err(invalid)?;
        if next == *light {
            return Ok(false);
        }
        let label = format!("Change Light {}", next.name);
        self.apply_with(|state| {
            state.render.lights[index] = next;
            Ok((label, (), false))
        })?;
        Ok(true)
    }

    /// Deletes a light; one undo step.
    pub fn delete_render_light(&mut self, id: &str) -> Result<(), ModelError> {
        let index = self.light_index(id)?;
        let label = format!("Delete Light {}", self.state.render.lights[index].name);
        self.apply_with(|state| {
            state.render.lights.remove(index);
            Ok((label, (), false))
        })?;
        Ok(())
    }

    fn light_index(&self, id: &str) -> Result<usize, ModelError> {
        self.state
            .render
            .lights
            .iter()
            .position(|light| light.id == id)
            .ok_or_else(|| invalid(format!("there is no light {id}")))
    }

    /// Adds a shot (mitcad#2013): a camera of the Render workspace, with
    /// the given fields (`eye`, `target`, `up`, `perspective`, `height`;
    /// `name` the next free `Shot<n>` and `id` the next free `shot<n>`
    /// unless given); one undo step. Returns its id.
    pub fn add_render_shot(&mut self, fields: &Map<String, Value>) -> Result<String, ModelError> {
        let render = &self.state.render;
        if render.shots.len() >= MAX_SHOTS {
            return Err(invalid(format!("a design keeps at most {MAX_SHOTS} shots")));
        }
        let free = (1..)
            .find(|n| {
                render.shot(&format!("shot{n}")).is_none()
                    && !render.shots.iter().any(|s| s.name == format!("Shot{n}"))
            })
            .expect("a free number");
        let shot = Shot {
            id: match fields.get("id") {
                Some(Value::String(id)) => id.trim().to_owned(),
                Some(other) => return Err(invalid(format!("a shot's id is text, got {other}"))),
                None => format!("shot{free}"),
            },
            name: format!("Shot{free}"),
            ..Shot::default()
        };
        if render.shot(&shot.id).is_some() {
            return Err(invalid(format!("a shot {} exists", shot.id)));
        }
        let mut rest = fields.clone();
        rest.remove("id");
        let shot = shot.changed(&rest).map_err(invalid)?;
        if render.shots.iter().any(|other| other.name == shot.name) {
            return Err(invalid(format!("a shot is named {} already", shot.name)));
        }
        let label = format!("Add Shot {}", shot.name);
        let id = shot.id.clone();
        self.apply_with(|state| {
            state.render.shots.push(shot);
            Ok((label, (), false))
        })?;
        Ok(id)
    }

    /// Changes the given fields of a shot (its name, or its camera: the
    /// workspace's Update to View); null gives a field its default. A
    /// change that changes nothing is no undo step. Returns whether
    /// anything changed.
    pub fn edit_render_shot(
        &mut self,
        id: &str,
        fields: &Map<String, Value>,
    ) -> Result<bool, ModelError> {
        let index = self.shot_index(id)?;
        let shot = &self.state.render.shots[index];
        let next = shot.changed(fields).map_err(invalid)?;
        if next == *shot {
            return Ok(false);
        }
        if self
            .state
            .render
            .shots
            .iter()
            .any(|other| other.id != next.id && other.name == next.name)
        {
            return Err(invalid(format!("a shot is named {} already", next.name)));
        }
        let label = if next.name != shot.name {
            format!("Rename Shot {} to {}", shot.name, next.name)
        } else {
            format!("Change Shot {}", next.name)
        };
        self.apply_with(|state| {
            state.render.shots[index] = next;
            Ok((label, (), false))
        })?;
        Ok(true)
    }

    /// Deletes a shot; one undo step.
    pub fn delete_render_shot(&mut self, id: &str) -> Result<(), ModelError> {
        let index = self.shot_index(id)?;
        let label = format!("Delete Shot {}", self.state.render.shots[index].name);
        self.apply_with(|state| {
            state.render.shots.remove(index);
            Ok((label, (), false))
        })?;
        Ok(())
    }

    fn shot_index(&self, id: &str) -> Result<usize, ModelError> {
        self.state
            .render
            .shots
            .iter()
            .position(|shot| shot.id == id)
            .ok_or_else(|| invalid(format!("there is no shot {id}")))
    }

    fn replace_render_settings(
        &mut self,
        next: RenderSettings,
        label: &str,
    ) -> Result<bool, ModelError> {
        if next == self.state.render {
            return Ok(false);
        }
        self.apply_with(|state| {
            state.render = next;
            Ok((label.to_owned(), (), false))
        })?;
        Ok(true)
    }
}
