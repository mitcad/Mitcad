// SPDX-License-Identifier: MIT
//! Faces coloured on their own in STEP and IGES files (mitcad#2313).
//!
//! - Import: a body read from a file carries the faces whose colour is not
//!   the body's ([`ImportBody::face_colors`], by the face's index in the
//!   kernel's face order). The base feature that adds the body names face
//!   `i` `<feature>:import(<first_face + i>)`; each colour becomes one
//!   document appearance, `Imported color <r> <g> <b>` (its sRGB bytes),
//!   made once for all the faces of the import that have it (an unchanged
//!   one of the document of that name is used again), and the faces get it
//!   as their appearance in the import's undo step. The body's colour stays
//!   as it was (the base body's colour).
//! - Bodies (mitcad#2423): a body's own colour in the file, which the base
//!   body keeps, becomes the body's appearance in the same way (the
//!   appearance of its colour, shared with faces of that colour), so that
//!   the view, which draws appearances, shows it: a STEP or IGES import
//!   flat or as components. The appearances are made before the base
//!   features and set on the bodies in the change that adds them
//!   ([`Document::body_color_looks`]), so that the import's recompute stays
//!   its last (the `recomputed` count of its result).
//! - Export: the faces with appearances of their own are written in their
//!   appearances' display colours beside the body's colour
//!   ([`ExportBody::face_colors`]); the kernel writes them to STEP and IGES
//!   files as the faces' colours.

use std::collections::BTreeMap;

use super::{BaseInput, FaceColor};
use crate::appearance::AppearanceChange;
use crate::document::{Document, ModelError};
use crate::features::Operation;
use crate::ids::BodyUid;
use crate::kernel::Kernel;

#[cfg(doc)]
use super::{ExportBody, ImportBody};

/// A body an import added with faces coloured on their own: its colour
/// in the file and those faces ([`ImportBody::face_colors`]).
#[derive(Debug, Clone)]
pub(crate) struct FaceInput {
    pub body: BodyUid,
    pub color: Option<[f64; 3]>,
    pub faces: Vec<FaceColor>,
}

/// A colour's sRGB bytes: faces of colours with the same bytes share an
/// appearance, and a face of its body's bytes gets none.
fn bytes(color: [f64; 3]) -> [u8; 3] {
    color.map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// The name of the appearance of an imported body or face colour.
fn appearance_name(color: [f64; 3]) -> String {
    let [r, g, b] = bytes(color);
    format!("Imported color {r} {g} {b}")
}

impl<K: Kernel> Document<K> {
    /// The faces of a body with appearances of their own, by their index
    /// in the kernel's face order, in their appearances' display colours:
    /// what STEP and IGES files write as the faces' colours.
    pub(crate) fn face_colors(&self, uid: BodyUid) -> Vec<FaceColor> {
        let mut colors: BTreeMap<usize, [f64; 3]> = BTreeMap::new();
        for (id, faces) in self.face_appearance_indices(uid) {
            let Some(appearance) = self.appearance(&id) else {
                continue;
            };
            let color = appearance.display_color();
            for face in faces {
                colors.insert(face, color);
            }
        }
        colors
            .into_iter()
            .map(|(face, color)| FaceColor { face, color })
            .collect()
    }

    /// The document appearance of an imported body or face colour: the one
    /// made for its bytes in this import, else an appearance of the
    /// document of its name and colour, else a new one.
    fn color_appearance(
        &mut self,
        color: [f64; 3],
        made: &mut BTreeMap<[u8; 3], String>,
    ) -> Result<String, ModelError> {
        let key = bytes(color);
        if let Some(id) = made.get(&key) {
            return Ok(id.clone());
        }
        let base = appearance_name(color);
        let same = self
            .custom_appearances()
            .iter()
            .find(|a| a.name == base && bytes(a.display_color()) == key)
            .map(|a| a.id.clone());
        let id = match same {
            Some(id) => id,
            None => {
                let taken = |name: &str| self.custom_appearances().iter().any(|a| a.name == name);
                let name = std::iter::once(base.clone())
                    .chain((2..).map(|n| format!("{base} ({n})")))
                    .take(1000)
                    .find(|n| !taken(n))
                    .ok_or_else(|| {
                        ModelError::Invalid(format!("no free appearance name for {base}"))
                    })?;
                self.create_appearance(&AppearanceChange {
                    name: Some(name),
                    base_color: Some(color),
                    ..AppearanceChange::default()
                })?
            }
        };
        made.insert(key, id.clone());
        Ok(id)
    }

    /// Gives the faces of imported bodies coloured on their own the
    /// appearances of their colours (see the module documentation): each
    /// body's faces of one colour in one change. Each change is an undo
    /// step of its own; the import merges them into its step. Returns what
    /// could not be done, in words.
    pub(crate) fn paint_imported_faces(&mut self, bodies: &[FaceInput]) -> Vec<String> {
        let mut warnings = Vec::new();
        let mut made = BTreeMap::new();
        for input in bodies {
            let Some(shape) = self.body_shape(input.body) else {
                continue;
            };
            let label = self.body_name(input.body);
            // Face i's name: an imported body's face has one,
            // `<feature>:import(<first_face + i>)`.
            let names: Vec<Option<String>> = match self.kernel().faces(shape) {
                Ok(faces) => faces
                    .into_iter()
                    .map(|f| f.names.into_iter().next())
                    .collect(),
                Err(e) => {
                    warnings.push(format!("{label}: the colors of its faces: {e}"));
                    continue;
                }
            };
            let body_key = input.color.map(bytes);
            let mut groups: BTreeMap<[u8; 3], ([f64; 3], Vec<String>)> = BTreeMap::new();
            let mut lost = 0usize;
            for face in &input.faces {
                let key = bytes(face.color);
                if Some(key) == body_key {
                    continue;
                }
                match names.get(face.face).cloned().flatten() {
                    Some(name) => groups
                        .entry(key)
                        .or_insert_with(|| (face.color, Vec::new()))
                        .1
                        .push(name),
                    None => lost += 1,
                }
            }
            for (color, faces) in groups.into_values() {
                let set = self
                    .color_appearance(color, &mut made)
                    .and_then(|id| self.set_face_appearance(input.body, &faces, Some(&id)));
                if let Err(e) = set {
                    warnings.push(format!("{label}: the colors of its faces: {e}"));
                }
            }
            if lost > 0 {
                warnings.push(format!(
                    "{label}: {lost} faces of their own color not found in the body: left in the body's color"
                ));
            }
        }
        warnings
    }

    /// The appearances of the colours of the new bodies of base features an
    /// import is about to add (mitcad#2423, see the module documentation),
    /// made now: per feature, per body the appearance id (none for a body
    /// without a colour, which keeps its material's look, and for a feature
    /// that makes no new bodies), and per feature what could not be done,
    /// in words. Each appearance made is an undo step of its own; the
    /// import merges them into its step.
    #[allow(clippy::type_complexity)]
    pub(crate) fn body_color_looks(
        &mut self,
        features: &[BaseInput<K::Shape>],
    ) -> (Vec<Vec<Option<String>>>, Vec<Vec<String>>) {
        let mut made = BTreeMap::new();
        let mut looks = Vec::with_capacity(features.len());
        let mut warnings = Vec::with_capacity(features.len());
        for base in features {
            let mut ids = Vec::with_capacity(base.bodies.len());
            let mut said = Vec::new();
            let new_bodies = matches!(base.operation, Operation::NewBody | Operation::NewComponent);
            for (i, body) in base.bodies.iter().enumerate() {
                let Some(color) = body.color.filter(|_| new_bodies) else {
                    ids.push(None);
                    continue;
                };
                match self.color_appearance(color, &mut made) {
                    Ok(id) => ids.push(Some(id)),
                    Err(e) => {
                        let label = match body.name.as_deref().map(str::trim) {
                            Some(name) if !name.is_empty() => name.to_owned(),
                            _ => format!("body {i}"),
                        };
                        said.push(format!("{label}: its color: {e}"));
                        ids.push(None);
                    }
                }
            }
            looks.push(ids);
            warnings.push(said);
        }
        (looks, warnings)
    }
}
