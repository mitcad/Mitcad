// SPDX-License-Identifier: MIT
//! Commands and queries of image references (mitcad#782,
//! `document/image_references.rs`): images placed on a plane or a planar
//! face to model from, calibrated by two points, with their opacity and
//! light bulb. See `commands.md`, "Image references".

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::datum::Vec3;
use mitcad_model::features::OccurrencePath;
use mitcad_model::features::geom_ref::GeomRef;
use mitcad_model::image_header;
use mitcad_model::kernel::Kernel;
use mitcad_model::{CalibrationPoint, Document, ImageEdit, ImageSize, ImageSource, ImageTarget};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum ImageReferenceCommand {
        "insert_image_reference" => InsertImageReference(Box<InsertImage>),
        "edit_image_reference" => EditImageReference(Box<EditImage>),
        "calibrate_image_reference" => CalibrateImageReference(Box<Calibration>),
        "set_image_reference_visible" => SetImageReferenceVisible {
            name: String,
            visible: bool,
        },
        "rename_image_reference" => RenameImageReference {
            name: String,
            new_name: String,
        },
        "delete_image_reference" => DeleteImageReference {
            name: String,
        },
    }
}

/// `insert_image_reference`: the image (`path` or `data`), its plane and
/// its settings, each optional.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InsertImage {
    /// The image file to embed.
    #[serde(default)]
    path: Option<String>,
    /// Or its bytes, base64.
    #[serde(default)]
    data: Option<String>,
    /// The file's name kept with `data` (the path's own with `path`).
    #[serde(default)]
    file_name: Option<String>,
    plane: GeomRef,
    #[serde(default)]
    occurrence: Option<OccurrencePath>,
    /// A point (design coordinates) the image's centre is put on,
    /// projected onto the plane.
    #[serde(default)]
    at: Option<Vec3>,
    /// The next free `Image<n>` when left out.
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    center: Option<[f64; 2]>,
    #[serde(default)]
    angle: Option<f64>,
    #[serde(default)]
    flip: Option<bool>,
    #[serde(default)]
    scale: Option<f64>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    opacity: Option<f64>,
    #[serde(default)]
    through: Option<bool>,
}

/// `edit_image_reference`: what changes; a field left out stays.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditImage {
    name: String,
    #[serde(default)]
    plane: Option<GeomRef>,
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
    flip: Option<bool>,
    #[serde(default)]
    scale: Option<f64>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    opacity: Option<f64>,
    #[serde(default)]
    through: Option<bool>,
}

/// `calibrate_image_reference`: two points (design coordinates) or two
/// pixels, and the distance between them.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Calibration {
    name: String,
    #[serde(default)]
    points: Option<[Vec3; 2]>,
    #[serde(default)]
    pixels: Option<[[f64; 2]; 2]>,
    distance: f64,
}

/// The `image_reference_outline` query: where an image of `pixels` would
/// lie on a plane with these settings.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OutlineQuery {
    plane: GeomRef,
    #[serde(default)]
    occurrence: Option<OccurrencePath>,
    #[serde(default)]
    at: Option<Vec3>,
    pixels: [u32; 2],
    #[serde(default)]
    center: Option<[f64; 2]>,
    #[serde(default)]
    angle: Option<f64>,
    #[serde(default)]
    flip: Option<bool>,
    #[serde(default)]
    scale: Option<f64>,
    #[serde(default)]
    width: Option<f64>,
}

fn read_file(path: &str) -> Result<Vec<u8>, ApiError> {
    std::fs::read(path).map_err(|e| ApiError(format!("cannot read {path}: {e}")))
}

fn decode(data: &str) -> Result<Vec<u8>, ApiError> {
    mitcad_model::internal::base64::decode(data)
        .map_err(|e| ApiError(format!("the image's data is no base64: {e}")))
}

/// The image of a command: read from `path`, or decoded from `data` (a
/// decal's too, `decals.rs`).
pub(super) fn source(
    path: Option<String>,
    data: Option<String>,
    file_name: Option<String>,
) -> Result<Option<ImageSource>, ApiError> {
    let bytes = match (&path, &data) {
        (Some(_), Some(_)) => {
            return Err(ApiError(
                "give the image's `path` or its `data`, not both".to_owned(),
            ));
        }
        (None, None) => return Ok(None),
        (Some(path), None) => read_file(path)?,
        (None, Some(data)) => decode(data)?,
    };
    let path = path.as_deref().map(std::path::Path::new);
    let own_name = path.and_then(std::path::Path::file_name);
    let file_name = match (file_name, own_name) {
        (Some(name), _) => name,
        (None, Some(name)) => name.to_string_lossy().into_owned(),
        (None, None) => String::new(),
    };
    Ok(Some(ImageSource { bytes, file_name }))
}

/// An image's size from `scale` or `width`, at most one.
fn size(scale: Option<f64>, width: Option<f64>) -> Result<Option<ImageSize>, ApiError> {
    match (scale, width) {
        (Some(_), Some(_)) => Err(ApiError(
            "give the image's `scale` or its `width`, not both".to_owned(),
        )),
        (Some(scale), None) => Ok(Some(ImageSize::Scale(scale))),
        (None, Some(width)) => Ok(Some(ImageSize::Width(width))),
        (None, None) => Ok(None),
    }
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait ImageReferencesCrateOps<K: Kernel> {
    fn run_image_reference_command(
        &mut self,
        command: ImageReferenceCommand,
    ) -> Result<Value, ApiError>;
}

impl<K: Kernel> ImageReferencesCrateOps<K> for Document<K> {
    fn run_image_reference_command(
        &mut self,
        command: ImageReferenceCommand,
    ) -> Result<Value, ApiError> {
        Ok(match command {
            ImageReferenceCommand::InsertImageReference(insert) => {
                let insert = *insert;
                let given = source(insert.path, insert.data, insert.file_name)?;
                let Some(image) = given else {
                    return Err(ApiError("give the image's `path` or its `data`".to_owned()));
                };
                let target = ImageTarget {
                    plane: insert.plane,
                    occurrence: insert.occurrence,
                    at: insert.at,
                };
                let edit = ImageEdit {
                    center: insert.center,
                    angle: insert.angle,
                    flip: insert.flip,
                    size: size(insert.scale, insert.width)?,
                    opacity: insert.opacity,
                    through: insert.through,
                    ..ImageEdit::default()
                };
                let name = insert.name.as_deref();
                json!({"name": self.insert_image_reference(image, target, edit, name)?})
            }
            ImageReferenceCommand::EditImageReference(edit) => {
                let edit = *edit;
                // A new plane without an occurrence is in the root; an
                // occurrence alone keeps the plane.
                let plane = match (edit.plane, edit.occurrence) {
                    (None, None) => None,
                    (Some(plane), occurrence) => Some((plane, occurrence)),
                    (None, Some(occurrence)) => {
                        let images = self.image_references();
                        let current = images.iter().find(|i| i.name == edit.name);
                        current.map(|image| (image.plane.clone(), Some(occurrence)))
                    }
                };
                let image = source(edit.path, edit.data, edit.file_name)?;
                let changes = ImageEdit {
                    plane,
                    center: edit.center,
                    angle: edit.angle,
                    flip: edit.flip,
                    size: size(edit.scale, edit.width)?,
                    opacity: edit.opacity,
                    through: edit.through,
                    image,
                };
                self.edit_image_reference(&edit.name, changes)?;
                json!({})
            }
            ImageReferenceCommand::CalibrateImageReference(calibration) => {
                let points = match (calibration.points, calibration.pixels) {
                    (Some(points), None) => points.map(CalibrationPoint::Point),
                    (None, Some(pixels)) => pixels.map(CalibrationPoint::Pixel),
                    _ => {
                        return Err(ApiError(
                            "give the calibration's two `points` or two `pixels`".to_owned(),
                        ));
                    }
                };
                let name = &calibration.name;
                let scale = self.calibrate_image_reference(name, points, calibration.distance)?;
                json!({"scale": scale})
            }
            ImageReferenceCommand::SetImageReferenceVisible { name, visible } => {
                self.set_image_reference_visible(&name, visible)?;
                json!({})
            }
            ImageReferenceCommand::RenameImageReference { name, new_name } => {
                self.rename_image_reference(&name, &new_name)?;
                json!({})
            }
            ImageReferenceCommand::DeleteImageReference { name } => {
                self.delete_image_reference(&name)?;
                json!({})
            }
        })
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait ImageReferencesOps<K: Kernel> {
    fn image_outline_json(&self, query: &OutlineQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> ImageReferencesOps<K> for Document<K> {
    fn image_outline_json(&self, query: &OutlineQuery) -> Result<Value, ApiError> {
        let target = ImageTarget {
            plane: query.plane.clone(),
            occurrence: query.occurrence.clone(),
            at: query.at,
        };
        let edit = ImageEdit {
            center: query.center,
            angle: query.angle,
            flip: query.flip,
            size: size(query.scale, query.width)?,
            ..ImageEdit::default()
        };
        let (corners, center) = self.image_outline(&target, query.pixels, &edit)?;
        Ok(json!({"corners": corners, "center": center}))
    }
}

/// The `image_info` query: an image file's format, size in pixels and the
/// size of a pixel it states (else 96 to the inch: `stated` false).
pub(super) fn image_info_json(path: &str) -> Result<Value, ApiError> {
    let bytes = read_file(path)?;
    let header = image_header::read(&bytes).map_err(ApiError)?;
    let pixel_size = header.pixel_size_or_default();
    Ok(json!({
        "format": header.format,
        "pixels": [header.width, header.height],
        "pixel_size": pixel_size,
        "stated": header.pixel_size.is_some(),
        "size": [f64::from(header.width) * pixel_size, f64::from(header.height) * pixel_size],
    }))
}
