// SPDX-License-Identifier: MIT
//! Commands and queries of decals (mitcad#2244,
//! `mitcad_model::Decal`): images put on planar, cylindrical and conical
//! faces of bodies, following the faces, with their size, angle, flip,
//! opacity and light bulb. See `commands.md`, "Decals".

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::image_references::source;
use mitcad_model::datum::Vec3;
use mitcad_model::features::OccurrencePath;
use mitcad_model::features::geom_ref::GeomRef;
use mitcad_model::kernel::Kernel;
use mitcad_model::{DecalEdit, DecalTarget, Document};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum DecalCommand {
        "insert_decal" => InsertDecal(Box<InsertDecal>),
        "edit_decal" => EditDecal(Box<EditDecal>),
        "set_decal_visible" => SetDecalVisible {
            name: String,
            visible: bool,
        },
        "rename_decal" => RenameDecal {
            name: String,
            new_name: String,
        },
        "delete_decal" => DeleteDecal {
            name: String,
        },
    }
}

/// `insert_decal`: the image (`path` or `data`), its face and its
/// settings, each optional.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InsertDecal {
    /// The image file to embed.
    #[serde(default)]
    path: Option<String>,
    /// Or its bytes, base64.
    #[serde(default)]
    data: Option<String>,
    /// The file's name kept with `data` (the path's own with `path`).
    #[serde(default)]
    file_name: Option<String>,
    face: GeomRef,
    #[serde(default)]
    occurrence: Option<OccurrencePath>,
    /// A point (design coordinates) the decal's centre is put on: the
    /// face's point nearest to it.
    #[serde(default)]
    at: Option<Vec3>,
    /// The next free `Decal<n>` when left out.
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    center: Option<[f64; 2]>,
    #[serde(default)]
    angle: Option<f64>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    lock_aspect: Option<bool>,
    #[serde(default)]
    flip: Option<bool>,
    #[serde(default)]
    opacity: Option<f64>,
}

/// `edit_decal`: what changes; a field left out stays.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditDecal {
    name: String,
    #[serde(default)]
    face: Option<GeomRef>,
    #[serde(default)]
    occurrence: Option<OccurrencePath>,
    /// Another image in its place.
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default)]
    center: Option<[f64; 2]>,
    #[serde(default)]
    angle: Option<f64>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    lock_aspect: Option<bool>,
    #[serde(default)]
    flip: Option<bool>,
    #[serde(default)]
    opacity: Option<f64>,
}

/// The settings a command or the outline query gives, as an edit.
macro_rules! settings {
    ($from:expr) => {
        DecalEdit {
            center: $from.center,
            angle: $from.angle,
            width: $from.width,
            height: $from.height,
            lock_aspect: $from.lock_aspect,
            flip: $from.flip,
            opacity: $from.opacity,
            ..DecalEdit::default()
        }
    };
}

/// The `decal_outline` query: where a decal of an image `pixels` large
/// would lie on a face with these settings.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OutlineQuery {
    face: GeomRef,
    #[serde(default)]
    occurrence: Option<OccurrencePath>,
    #[serde(default)]
    at: Option<Vec3>,
    pixels: [u32; 2],
    /// Its width without `width` or `height` (mm); 20 when left out.
    #[serde(default)]
    default_width: Option<f64>,
    #[serde(default)]
    center: Option<[f64; 2]>,
    #[serde(default)]
    angle: Option<f64>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    lock_aspect: Option<bool>,
    #[serde(default)]
    flip: Option<bool>,
    /// Not used by the outline; taken so that a panel may send its
    /// settings as they are.
    #[serde(default)]
    opacity: Option<f64>,
}

/// The width of a decal the outline query is not told the size of (mm).
const OUTLINE_WIDTH: f64 = 20.0;

/// The commands of decals.
pub(crate) fn run<K: Kernel>(
    doc: &mut Document<K>,
    command: DecalCommand,
) -> Result<Value, ApiError> {
    Ok(match command {
        DecalCommand::InsertDecal(insert) => {
            let insert = *insert;
            let Some(image) = source(insert.path, insert.data, insert.file_name)? else {
                return Err(ApiError(
                    "give the decal's image: its `path` or its `data`".to_owned(),
                ));
            };
            let target = DecalTarget {
                face: insert.face,
                occurrence: insert.occurrence,
                at: insert.at,
            };
            let edit = settings!(insert);
            let name = insert.name.as_deref();
            json!({"name": doc.insert_decal(image, target, edit, name)?})
        }
        DecalCommand::EditDecal(edit) => {
            let edit = *edit;
            // A new face without an occurrence is in the root; an
            // occurrence alone keeps the face.
            let face = match (edit.face, edit.occurrence) {
                (None, None) => None,
                (Some(face), occurrence) => Some((face, occurrence)),
                (None, Some(occurrence)) => doc
                    .decals()
                    .iter()
                    .find(|d| d.name == edit.name)
                    .map(|decal| (decal.face.clone(), Some(occurrence))),
            };
            let changes = DecalEdit {
                face,
                image: source(edit.path, edit.data, edit.file_name)?,
                ..settings!(edit)
            };
            doc.edit_decal(&edit.name, changes)?;
            json!({})
        }
        DecalCommand::SetDecalVisible { name, visible } => {
            doc.set_decal_visible(&name, visible)?;
            json!({})
        }
        DecalCommand::RenameDecal { name, new_name } => {
            doc.rename_decal(&name, &new_name)?;
            json!({})
        }
        DecalCommand::DeleteDecal { name } => {
            doc.delete_decal(&name)?;
            json!({})
        }
    })
}

/// The `decal_outline` query.
pub(crate) fn outline_json<K: Kernel>(
    doc: &Document<K>,
    query: &OutlineQuery,
) -> Result<Value, ApiError> {
    let target = DecalTarget {
        face: query.face.clone(),
        occurrence: query.occurrence.clone(),
        at: query.at,
    };
    let width = query.default_width.unwrap_or(OUTLINE_WIDTH);
    Ok(doc.decal_outline(&target, query.pixels, &settings!(query), width)?)
}

/// The decals an export of `bodies` leaves out (mitcad#2244): the shown
/// decals on those bodies' faces, by name. No export format writes decals
/// yet; the export's report says so.
pub(crate) fn left_out<K: Kernel>(
    doc: &Document<K>,
    bodies: &[mitcad_model::ids::BodyUid],
) -> Vec<String> {
    doc.decals()
        .iter()
        .filter(|decal| decal.visible)
        .filter(|decal| match &decal.face {
            GeomRef::Face { body, .. } => bodies.contains(body),
            _ => false,
        })
        .map(|decal| decal.name.clone())
        .collect()
}
