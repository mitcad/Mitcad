// SPDX-License-Identifier: MIT
//! FreeCAD's colours as document appearances (mitcad#793).
//!
//! `GuiDocument.xml` keeps each object's colours: up to 0.21 `ShapeColor`
//! (the shape's), `Transparency` (in percent) and `DiffuseColor` (one per
//! face, its alpha the face's transparency); from 1.0 `ShapeAppearance`
//! (materials, one for the shape or one per face, each with its
//! transparency).
//!
//! - Each colour (its RGB bytes and opacity) is one document appearance,
//!   `FreeCAD colour <r> <g> <b>`, made once for all the bodies and faces
//!   that have it.
//! - A body gets its object's colour: the shape's, or with colours per
//!   face the one most faces have. FreeCAD's default grey (0.21's, 1.0's
//!   default appearance) makes none: the body keeps Mitcad's look.
//! - A face whose colour is not its body's gets that colour's appearance
//!   (also FreeCAD's grey, on a body of another colour). FreeCAD numbers
//!   the faces in its stored shape's order: on a body of the stored shape
//!   (stage 1) that is the face at the same index; on a body the history
//!   made (rebuilt, or ending in a fallback's base feature) it is the face
//!   of the same geometry, found as references are
//!   ([`Importer::planned_faces`]). Faces not found (a face FreeCAD's
//!   `Refine` merged from several of Mitcad's) are counted in a warning.
//! - Physical materials are not imported.

use std::collections::BTreeMap;

use mitcad_freecad::{Color, FcstdFile, Object, Value};
use mitcad_model::appearance::AppearanceChange;
use mitcad_model::{BodyUid, Kernel};

use super::Importer;

/// FreeCAD's default shape colours (0.21's grey, 1.0's default
/// appearance): no appearance is made of them for a body, so that the
/// bodies keep Mitcad's look.
const DEFAULT_COLORS: [[u8; 3]; 2] = [[204, 204, 204], [204, 204, 230]];

/// A colour as the import tells colours apart: its RGB bytes and its
/// opacity in percent.
pub(super) type ShadeKey = ([u8; 3], u8);

/// A colour of FreeCAD's view: sRGB components in [0, 1] and the opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Shade {
    pub rgb: [f64; 3],
    pub opacity: f64,
}

impl Shade {
    fn new(color: Color, transparency: f64) -> Self {
        Self {
            rgb: color.rgb(),
            opacity: (1.0 - transparency).clamp(0.0, 1.0),
        }
    }

    /// The colour as FreeCAD tells colours apart: its bytes, and the
    /// opacity in percent.
    fn key(&self) -> ShadeKey {
        let bytes = self
            .rgb
            .map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8);
        (
            bytes,
            (self.opacity * 100.0).round().clamp(0.0, 100.0) as u8,
        )
    }

    fn is_default(&self) -> bool {
        let (bytes, opacity) = self.key();
        DEFAULT_COLORS.contains(&bytes) && opacity == 100
    }
}

/// An object's colours: its body's (None for FreeCAD's default) and the
/// faces of another colour, by FreeCAD's face index (from 0).
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Look {
    pub body: Option<Shade>,
    pub faces: Vec<(usize, Shade)>,
}

/// The colours FreeCAD's view gives an object's shape.
pub(super) fn look(file: &FcstdFile, o: &Object) -> Look {
    let Some(view) = file.view(&o.name) else {
        return Look::default();
    };
    let transparency = view.transparency().unwrap_or(0.0) / 100.0;
    // Colours per face: a list of more than one entry (one entry is the
    // whole shape's).
    let faces: Vec<Shade> = match view.value("ShapeAppearance") {
        Some(Value::MaterialList(materials)) if materials.len() > 1 => materials
            .iter()
            .map(|m| Shade::new(m.diffuse, m.transparency))
            .collect(),
        _ => match view.value("DiffuseColor") {
            Some(Value::ColorList(colors)) if colors.len() > 1 => {
                colors.iter().map(|c| Shade::new(*c, c.0[3])).collect()
            }
            _ => Vec::new(),
        },
    };
    let shape = match view.value("ShapeAppearance") {
        Some(Value::MaterialList(materials)) => materials
            .first()
            .map(|m| Shade::new(m.diffuse, m.transparency)),
        _ => match view.value("ShapeColor") {
            Some(Value::Color(c)) => Some(Shade::new(*c, transparency)),
            _ => None,
        },
    };
    // With colours per face the body's is the one most faces have (the
    // first of those as many have).
    let body = if faces.is_empty() {
        shape
    } else {
        let mut counts: Vec<(ShadeKey, usize, Shade)> = Vec::new();
        for f in &faces {
            match counts.iter_mut().find(|(k, _, _)| *k == f.key()) {
                Some(entry) => entry.1 += 1,
                None => counts.push((f.key(), 1, *f)),
            }
        }
        let most = counts.iter().map(|(_, n, _)| *n).max().unwrap_or(0);
        counts
            .into_iter()
            .find(|(_, n, _)| *n == most)
            .map(|(_, _, shade)| shade)
    };
    let body_key = body.map(|b| b.key());
    let faces = faces
        .into_iter()
        .enumerate()
        .filter(|(_, f)| Some(f.key()) != body_key)
        .collect();
    Look {
        body: body.filter(|b| !b.is_default()),
        faces,
    }
}

/// The name of the appearance of a colour.
fn appearance_name(shade: &Shade) -> String {
    let ([r, g, b], opacity) = shade.key();
    if opacity < 100 {
        format!("FreeCAD color {r} {g} {b}, {opacity} % opaque")
    } else {
        format!("FreeCAD color {r} {g} {b}")
    }
}

impl<K: Kernel> Importer<'_, '_, K> {
    /// The document appearance of a colour, made the first time it is
    /// asked for.
    fn appearance(&mut self, shade: Shade) -> Option<String> {
        let key = shade.key();
        if let Some(id) = self.appearances.get(&key) {
            return Some(id.clone());
        }
        let base = appearance_name(&shade);
        let taken = |name: &str| self.doc.custom_appearances().iter().any(|a| a.name == name);
        let name = std::iter::once(base.clone())
            .chain((2..).map(|n| format!("{base} ({n})")))
            .take(1000)
            .find(|n| !taken(n))?;
        let change = AppearanceChange {
            name: Some(name.clone()),
            base_color: Some(shade.rgb),
            opacity: (shade.opacity < 1.0).then_some(shade.opacity),
            ..AppearanceChange::default()
        };
        match self.doc.create_appearance(&change) {
            Ok(id) => {
                self.appearances.insert(key, id.clone());
                self.report.appearances += 1;
                Some(id)
            }
            Err(e) => {
                self.warn(format!("appearance {name}: {e}"));
                None
            }
        }
    }

    /// FreeCAD's colours on the bodies made, as they are at the end of the
    /// history: the bodies' and the faces' appearances.
    pub(super) fn apply_colors(&mut self) {
        let mut missed: Vec<String> = Vec::new();
        for index in 0..self.planned.len() {
            let p = &self.planned[index];
            let Some(first) = p.body else {
                continue;
            };
            let bodies: Vec<BodyUid> = std::iter::once(first)
                .chain(p.more.iter().copied())
                .collect();
            let label = p.name.clone();
            let source = &self.sources[p.key.doc];
            let Some(o) = source.object(&p.key.name) else {
                continue;
            };
            let look = look(&source.file, o);
            if let Some(shade) = look.body
                && let Some(id) = self.appearance(shade)
            {
                for body in &bodies {
                    match self.doc.set_body_appearance(*body, Some(&id)) {
                        Ok(()) => {
                            self.looks.entry(*body).or_default().0 = Some(id.clone());
                        }
                        Err(e) => self.warn(format!("{label}: {e}")),
                    }
                }
            }
            if look.faces.is_empty() {
                continue;
            }
            let indices: Vec<usize> = look.faces.iter().map(|(i, _)| *i).collect();
            let found = self.planned_faces(index, &indices);
            // The faces of each body by appearance: one change each.
            let mut groups: BTreeMap<(BodyUid, String), Vec<String>> = BTreeMap::new();
            let mut lost = 0;
            for ((_, shade), place) in look.faces.iter().zip(found) {
                let Some(id) = self.appearance(*shade) else {
                    continue;
                };
                match place {
                    Some((body, face)) => groups.entry((body, id)).or_default().push(face),
                    None => lost += 1,
                }
            }
            for ((body, id), faces) in groups {
                match self.doc.set_face_appearance(body, &faces, Some(&id)) {
                    Ok(()) => self.looks.entry(body).or_default().1 += faces.len(),
                    Err(e) => self.warn(format!("{label}: the colors of its faces: {e}")),
                }
            }
            if lost > 0 {
                missed.push(format!("{label} ({lost})"));
            }
        }
        if !missed.is_empty() {
            self.warn(format!(
                "faces whose color was not carried over (not found in Mitcad's body, \
                 e.g. a face FreeCAD merged from several): {}",
                missed.join(", ")
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_tell_apart_by_bytes_and_opacity() {
        let red = Shade::new(Color([1.0, 0.0, 0.0, 0.0]), 0.0);
        let glass = Shade::new(Color([1.0, 0.0, 0.0, 0.0]), 0.25);
        assert_ne!(red.key(), glass.key());
        assert_eq!(appearance_name(&red), "FreeCAD color 255 0 0");
        assert_eq!(
            appearance_name(&glass),
            "FreeCAD color 255 0 0, 75 % opaque"
        );
        let grey = Shade::new(Color::from_packed(0xCCCC_CC00), 0.0);
        assert!(grey.is_default());
        assert!(!Shade::new(Color::from_packed(0xCCCC_CC00), 0.5).is_default());
    }
}
