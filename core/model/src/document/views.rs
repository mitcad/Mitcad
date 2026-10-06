// SPDX-License-Identifier: MIT
//! Named views (U5): cameras saved in the document. They change no
//! geometry: adding, changing, renaming and deleting one is an undo step
//! that recomputes nothing. The application treats a
//! view named `Home` as the document's home view.

use serde::{Deserialize, Serialize};

use super::{Document, ModelError, invalid};
use crate::features::is_false;
use crate::kernel::Kernel;

/// A saved camera, in the design's coordinates (millimetres).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedView {
    pub name: String,
    /// Where the camera is.
    pub eye: [f64; 3],
    /// The point it looks at.
    pub target: [f64; 3],
    /// Upwards on the screen; not along the line of sight.
    pub up: [f64; 3],
    /// A perspective projection; orthographic when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub perspective: bool,
    /// How much of the model the view shows: its height at the target.
    pub height: f64,
}

fn length(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

impl NamedView {
    /// Why the view cannot be saved, if it cannot.
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the view needs a name".to_owned());
        }
        let numbers = self.eye.iter().chain(&self.target).chain(&self.up);
        if !numbers.copied().all(f64::is_finite) {
            return Err(format!("view '{}': the camera must be finite", self.name));
        }
        let sight = [
            self.target[0] - self.eye[0],
            self.target[1] - self.eye[1],
            self.target[2] - self.eye[2],
        ];
        let (along, up) = (length(sight), length(self.up));
        if along <= 1e-9 || up <= 1e-12 {
            return Err(format!(
                "view '{}': the eye must differ from the target, and up must not be zero",
                self.name
            ));
        }
        let dot =
            (sight[0] * self.up[0] + sight[1] * self.up[1] + sight[2] * self.up[2]) / (along * up);
        if dot.abs() > 1.0 - 1e-9 {
            return Err(format!(
                "view '{}': up must not be along the line of sight",
                self.name
            ));
        }
        if !(self.height.is_finite() && self.height > 0.0) {
            return Err(format!(
                "view '{}': the height must be positive, got {}",
                self.name, self.height
            ));
        }
        Ok(())
    }
}

impl<K: Kernel> Document<K> {
    /// The named views in the order they were added.
    pub fn named_views(&self) -> &[NamedView] {
        &self.state.named_views
    }

    /// Saves a view. A view of the same name is replaced when `replace`
    /// is set, else refused. Without a name the view is `NamedView1`,
    /// `NamedView2`, ... (the next free number). Returns the name.
    pub fn add_named_view(
        &mut self,
        mut view: NamedView,
        replace: bool,
    ) -> Result<String, ModelError> {
        view.name = view.name.trim().to_owned();
        if view.name.is_empty() {
            view.name = (1..)
                .map(|n| format!("NamedView{n}"))
                .find(|name| !self.state.named_views.iter().any(|v| &v.name == name))
                .expect("a free name");
        }
        view.check().map_err(invalid)?;
        let existing = self
            .state
            .named_views
            .iter()
            .position(|v| v.name == view.name);
        if existing.is_some() && !replace {
            return Err(invalid(format!("a named view '{}' exists", view.name)));
        }
        if existing.is_some_and(|i| self.state.named_views[i] == view) {
            return Ok(view.name);
        }
        let name = view.name.clone();
        let label = match existing {
            Some(_) => format!("Change Named View {name}"),
            None => format!("Add Named View {name}"),
        };
        self.apply_with(|state| {
            match existing {
                Some(i) => state.named_views[i] = view,
                None => state.named_views.push(view),
            }
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    pub fn delete_named_view(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.view_index(name)?;
        let label = format!("Delete Named View {name}");
        self.apply_with(|state| {
            state.named_views.remove(i);
            Ok((label, (), false))
        })
    }

    pub fn rename_named_view(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        let i = self.view_index(name)?;
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(invalid("the view needs a name"));
        }
        if new_name == name {
            return Ok(());
        }
        if self.state.named_views.iter().any(|v| v.name == new_name) {
            return Err(invalid(format!("a named view '{new_name}' exists")));
        }
        let label = format!("Rename Named View {name}");
        let new_name = new_name.to_owned();
        self.apply_with(|state| {
            state.named_views[i].name = new_name;
            Ok((label, (), false))
        })
    }

    fn view_index(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .named_views
            .iter()
            .position(|v| v.name == name)
            .ok_or_else(|| invalid(format!("there is no named view '{name}'")))
    }
}
