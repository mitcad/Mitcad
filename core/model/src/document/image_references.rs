// SPDX-License-Identifier: MIT
//! Image references (mitcad#782): pictures placed in the design to model
//! from, such as a photo or a scanned drawing, listed in the browser's
//! Images folder and saved in the design file with the image's bytes
//! embedded (no path to the file is kept). They change no geometry:
//! inserting, editing, calibrating, showing, hiding, renaming and deleting
//! one is an undo step that recomputes nothing.
//!
//! An image lies on a plane reference as a section analysis does (an
//! origin plane, a construction plane, a planar face), resolved at the
//! timeline marker whenever it is asked for, so it follows its plane or
//! face when the model changes. Its place on the plane is its centre in
//! the plane's frame (mm), an angle about the plane's normal and a flip
//! (mirrored left to right); its size is the size of one pixel (`scale`,
//! mm), which calibration sets from two points and the distance between
//! them, scaling the image about the first point.
//!
//! Pixels are counted from the image's top left corner, `u` to the right
//! and `v` down; seen from the side the plane's normal points to, an image
//! that is not flipped reads as it does in a picture viewer.

use mitcad_vec3::{add, dot, scale, sub};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Document, ModelError, invalid};
use crate::appearance::{ImageData, MAX_EMBEDDED_IMAGE};
use crate::datum::{DatumPlane, Vec3};
use crate::features::geom_ref::GeomRef;
use crate::features::{OccurrencePath, is_false};
use crate::image_header;
use crate::kernel::Kernel;

/// The opacity of a new image (0 to 1).
pub const DEFAULT_OPACITY: f64 = 0.5;

/// An image placed in the design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageReference {
    pub name: String,
    /// Its light bulb; left out of files when true.
    #[serde(default = "shown", skip_serializing_if = "is_shown")]
    pub visible: bool,
    /// A plane reference: an origin plane, a construction plane, a planar
    /// face or a fixed plane.
    pub plane: GeomRef,
    /// The plane's occurrence from the design root, for a plane of a
    /// component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
    /// The image's centre in the plane's frame, mm.
    #[serde(default, skip_serializing_if = "is_origin")]
    pub center: [f64; 2],
    /// Turned about the plane's normal, degrees, counter-clockwise seen
    /// from the side the normal points to.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub angle: f64,
    /// Mirrored left to right.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// The size of a pixel, mm.
    pub scale: f64,
    /// 0 (not seen) to 1 (opaque).
    pub opacity: f64,
    /// Drawn through the bodies (over them) instead of hidden behind them.
    #[serde(default, skip_serializing_if = "is_false")]
    pub through: bool,
    /// The name of the file it came from, for the user; no path.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file_name: String,
    pub image: EmbeddedImage,
}

fn shown() -> bool {
    true
}

fn is_shown(visible: &bool) -> bool {
    *visible
}

fn is_origin(center: &[f64; 2]) -> bool {
    *center == [0.0, 0.0]
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// An image file's bytes as the design keeps them, with what its header
/// says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddedImage {
    /// `png`, `jpeg`, `bmp`, `webp` or `heif`.
    pub format: String,
    /// In pixels.
    pub width: u32,
    pub height: u32,
    /// The file's bytes (base64 in files).
    pub data: ImageData,
}

impl EmbeddedImage {
    /// Why the bytes are not the image the fields say, if they are not.
    fn check(&self) -> Result<(), String> {
        let bytes = crate::base64::decode(self.data.base64())
            .map_err(|e| format!("the image is no base64 data: {e}"))?;
        let header = image_header::read(&bytes)?;
        if header.format != self.format || [header.width, header.height] != self.pixels() {
            return Err(format!(
                "the image is a {} × {} {} file, not the {} × {} {} it says",
                header.width, header.height, header.format, self.width, self.height, self.format
            ));
        }
        Ok(())
    }

    fn pixels(&self) -> [u32; 2] {
        [self.width, self.height]
    }

    fn size(&self) -> [f64; 2] {
        [f64::from(self.width), f64::from(self.height)]
    }
}

/// An image file to embed: its bytes and its name (no path).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSource {
    pub bytes: Vec<u8>,
    pub file_name: String,
}

impl ImageSource {
    /// The image as the design keeps it and the size of a pixel its file
    /// states (or 96 to the inch), or why it cannot be kept.
    fn embed(&self) -> Result<(EmbeddedImage, f64), String> {
        if self.bytes.len() > MAX_EMBEDDED_IMAGE {
            return Err(format!(
                "the image is {} MB, more than the {} MB a design embeds",
                self.bytes.len() >> 20,
                MAX_EMBEDDED_IMAGE >> 20
            ));
        }
        let header = image_header::read(&self.bytes)?;
        let image = EmbeddedImage {
            format: header.format.to_owned(),
            width: header.width,
            height: header.height,
            data: ImageData::from_bytes(&self.bytes),
        };
        Ok((image, header.pixel_size_or_default()))
    }
}

/// Where a new image goes: its plane and, optionally, a point (design
/// coordinates) its centre is put on, projected onto the plane.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageTarget {
    pub plane: GeomRef,
    pub occurrence: Option<OccurrencePath>,
    pub at: Option<Vec3>,
}

/// An image's size: the size of a pixel, or the image's width (mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageSize {
    Scale(f64),
    Width(f64),
}

/// What an edit changes; `None` keeps a field as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageEdit {
    pub plane: Option<(GeomRef, Option<OccurrencePath>)>,
    pub center: Option<[f64; 2]>,
    pub angle: Option<f64>,
    pub flip: Option<bool>,
    pub size: Option<ImageSize>,
    pub opacity: Option<f64>,
    pub through: Option<bool>,
    /// Another image in its place, at the same place and pixel size.
    pub image: Option<ImageSource>,
}

/// A point calibration measures from: a pixel of the image (from its top
/// left corner) or a point of the design, projected onto the image's
/// plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalibrationPoint {
    Pixel([f64; 2]),
    Point(Vec3),
}

/// The image's frame in the plane's frame (2D): the centre, the way its
/// columns grow and the way up its rows, unit vectors.
#[derive(Debug, Clone, Copy)]
struct Placement {
    center: [f64; 2],
    right: [f64; 2],
    up: [f64; 2],
}

impl Placement {
    fn of(center: [f64; 2], angle: f64, flip: bool) -> Self {
        let (sin, cos) = angle.to_radians().sin_cos();
        let sign = if flip { -1.0 } else { 1.0 };
        Self {
            center,
            right: [sign * cos, sign * sin],
            up: [-sin, cos],
        }
    }

    /// A pixel's place in the plane's frame, at `scale` mm per pixel.
    fn at(&self, size: [f64; 2], scale: f64, pixel: [f64; 2]) -> [f64; 2] {
        let x = (pixel[0] - size[0] / 2.0) * scale;
        let y = (size[1] / 2.0 - pixel[1]) * scale;
        [
            self.center[0] + self.right[0] * x + self.up[0] * y,
            self.center[1] + self.right[1] * x + self.up[1] * y,
        ]
    }

    /// The pixel at a place of the plane's frame.
    fn pixel(&self, size: [f64; 2], scale: f64, at: [f64; 2]) -> [f64; 2] {
        let d = [at[0] - self.center[0], at[1] - self.center[1]];
        let x = d[0] * self.right[0] + d[1] * self.right[1];
        let y = d[0] * self.up[0] + d[1] * self.up[1];
        [size[0] / 2.0 + x / scale, size[1] / 2.0 - y / scale]
    }
}

/// A place of the plane's frame in the design.
fn on_plane(plane: &DatumPlane, at: [f64; 2]) -> Vec3 {
    let along = add(scale(plane.x_axis, at[0]), scale(plane.y_axis, at[1]));
    add(plane.origin, along)
}

/// A point of the design in the plane's frame (projected onto it).
fn in_plane(plane: &DatumPlane, point: Vec3) -> [f64; 2] {
    let d = sub(point, plane.origin);
    [dot(d, plane.x_axis), dot(d, plane.y_axis)]
}

/// The corners of an image on a plane in the design: top left, top
/// right, bottom right, bottom left.
pub fn image_corners(
    plane: &DatumPlane,
    pixels: [u32; 2],
    center: [f64; 2],
    angle: f64,
    flip: bool,
    scale: f64,
) -> [Vec3; 4] {
    let size = [f64::from(pixels[0]), f64::from(pixels[1])];
    let placement = Placement::of(center, angle, flip);
    [[0.0, 0.0], [size[0], 0.0], size, [0.0, size[1]]]
        .map(|pixel| on_plane(plane, placement.at(size, scale, pixel)))
}

/// Why an image's size is no size, if it is not.
fn check_size(size: ImageSize) -> Result<(), String> {
    match size {
        ImageSize::Scale(scale) if scale > 0.0 && scale.is_finite() => Ok(()),
        ImageSize::Width(width) if width > 0.0 && width.is_finite() => Ok(()),
        ImageSize::Scale(scale) => Err(format!(
            "the size of a pixel must be greater than zero, got {scale}"
        )),
        ImageSize::Width(width) => Err(format!(
            "the image's width must be greater than zero, got {width}"
        )),
    }
}

impl ImageReference {
    /// Why the image cannot be kept, if it cannot (not its plane).
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the image reference needs a name".to_owned());
        }
        check_size(ImageSize::Scale(self.scale)).map_err(|e| format!("{}: {e}", self.name))?;
        if !(0.0..=1.0).contains(&self.opacity) {
            return Err(format!(
                "{}: the opacity must be from 0 to 1, got {}",
                self.name, self.opacity
            ));
        }
        if !self.angle.is_finite() || !self.center.iter().all(|c| c.is_finite()) {
            return Err(format!(
                "{}: the center and the angle must be finite",
                self.name
            ));
        }
        Ok(())
    }

    /// Also checks that the embedded bytes are the image the fields say
    /// (a design file's).
    pub(crate) fn check_image(&self) -> Result<(), String> {
        self.check()?;
        self.image
            .check()
            .map_err(|e| format!("{}: {e}", self.name))
    }

    fn placement(&self) -> Placement {
        Placement::of(self.center, self.angle, self.flip)
    }

    /// The image's width and height, mm.
    pub fn size(&self) -> [f64; 2] {
        let pixels = self.image.size();
        [pixels[0] * self.scale, pixels[1] * self.scale]
    }

    fn set_size(&mut self, size: ImageSize) {
        self.scale = match size {
            ImageSize::Scale(scale) => scale,
            ImageSize::Width(width) => width / f64::from(self.image.width),
        };
    }

    fn apply(&mut self, edit: ImageEdit) -> Result<(), String> {
        if let Some((plane, occurrence)) = edit.plane {
            self.plane = plane;
            self.occurrence = occurrence;
        }
        if let Some(source) = edit.image {
            let (image, _) = source.embed()?;
            self.image = image;
            self.file_name = source.file_name;
        }
        self.center = edit.center.unwrap_or(self.center);
        self.angle = edit.angle.unwrap_or(self.angle);
        self.flip = edit.flip.unwrap_or(self.flip);
        if let Some(size) = edit.size {
            check_size(size)?;
            self.set_size(size);
        }
        self.opacity = edit.opacity.unwrap_or(self.opacity);
        self.through = edit.through.unwrap_or(self.through);
        Ok(())
    }
}

impl<K: Kernel> Document<K> {
    /// The image references in the order they were inserted.
    pub fn image_references(&self) -> &[ImageReference] {
        &self.state.image_references
    }

    /// The plane an image lies on at the marker, in design coordinates.
    fn image_plane(
        &self,
        plane: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<DatumPlane, String> {
        self.analysis_plane(plane, occurrence)
    }

    /// An image's corners at the marker (top left, top right, bottom
    /// right, bottom left), or why its plane is not there.
    pub fn image_reference_corners(&self, image: &ImageReference) -> Result<[Vec3; 4], String> {
        let plane = self.image_plane(&image.plane, image.occurrence.as_ref())?;
        Ok(image_corners(
            &plane,
            image.image.pixels(),
            image.center,
            image.angle,
            image.flip,
            image.scale,
        ))
    }

    /// The `image_references` query: each image's settings without its
    /// bytes, with its pixels, size, digest and corners at the marker
    /// (`corners`) or why its plane is not there (`error`).
    pub fn image_references_json(&self) -> Value {
        Value::Array(
            self.state
                .image_references
                .iter()
                .map(|image| {
                    let mut value = serde_json::to_value(image).expect("images serialize");
                    if let Some(fields) = value.as_object_mut() {
                        fields.remove("image");
                    }
                    value["visible"] = json!(image.visible);
                    value["center"] = json!(image.center);
                    value["angle"] = json!(image.angle);
                    value["flip"] = json!(image.flip);
                    value["through"] = json!(image.through);
                    value["file_name"] = json!(image.file_name);
                    value["format"] = json!(image.image.format);
                    value["pixels"] = json!(image.image.pixels());
                    value["size"] = json!(image.size());
                    value["sha256"] = json!(image.image.data.sha256());
                    match self.image_reference_corners(image) {
                        Ok(corners) => value["corners"] = json!(corners),
                        Err(error) => value["error"] = json!(error),
                    }
                    value
                })
                .collect(),
        )
    }

    /// The `image_reference_image` query: an image's file, base64.
    pub fn image_reference_image_json(&self, name: &str) -> Result<Value, ModelError> {
        let image = &self.state.image_references[self.image_index(name)?];
        Ok(json!({
            "name": image.name,
            "format": image.image.format,
            "pixels": image.image.pixels(),
            "sha256": image.image.data.sha256(),
            "data": image.image.data.base64(),
        }))
    }

    /// The `image_reference_outline` query: where an image of `pixels`
    /// would lie with these settings: its corners in the design and its
    /// centre in the plane's frame (`at` projected, for a panel's X and Y).
    pub fn image_outline(
        &self,
        target: &ImageTarget,
        pixels: [u32; 2],
        edit: &ImageEdit,
    ) -> Result<([Vec3; 4], [f64; 2]), ModelError> {
        let plane = self
            .image_plane(&target.plane, target.occurrence.as_ref())
            .map_err(|e| invalid(format!("the image's plane: {e}")))?;
        let center = edit
            .center
            .or_else(|| target.at.map(|at| in_plane(&plane, at)))
            .unwrap_or_default();
        let scale = match edit.size {
            Some(size) => {
                check_size(size).map_err(invalid)?;
                match size {
                    ImageSize::Scale(scale) => scale,
                    ImageSize::Width(width) => width / f64::from(pixels[0].max(1)),
                }
            }
            None => image_header::DEFAULT_PIXEL_SIZE,
        };
        let corners = image_corners(
            &plane,
            pixels,
            center,
            edit.angle.unwrap_or(0.0),
            edit.flip.unwrap_or(false),
            scale,
        );
        Ok((corners, center))
    }

    /// Inserts an image on a plane, shown: centred on `target.at` (or the
    /// plane's origin), a pixel the size its file states (or 96 to the
    /// inch) unless `edit` sets a size, half opaque unless it sets an
    /// opacity. Without a name it is `Image1`, `Image2`, ... (the next free
    /// number). Returns the name.
    pub fn insert_image_reference(
        &mut self,
        source: ImageSource,
        target: ImageTarget,
        edit: ImageEdit,
        name: Option<&str>,
    ) -> Result<String, ModelError> {
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => name.to_owned(),
            None => (1..)
                .map(|n| format!("Image{n}"))
                .find(|name| self.image_index(name).is_err())
                .expect("a free name"),
        };
        if self.image_index(&name).is_ok() {
            return Err(invalid(format!("an image reference {name} exists")));
        }
        let plane = self
            .image_plane(&target.plane, target.occurrence.as_ref())
            .map_err(|e| invalid(format!("the image's plane: {e}")))?;
        let (image, pixel_size) = source.embed().map_err(invalid)?;
        let mut reference = ImageReference {
            name: name.clone(),
            visible: true,
            plane: target.plane,
            occurrence: target.occurrence,
            center: target.at.map_or([0.0, 0.0], |at| in_plane(&plane, at)),
            angle: 0.0,
            flip: false,
            scale: pixel_size,
            opacity: DEFAULT_OPACITY,
            through: false,
            file_name: source.file_name,
            image,
        };
        reference
            .apply(ImageEdit {
                plane: None,
                image: None,
                ..edit
            })
            .map_err(invalid)?;
        reference.check().map_err(invalid)?;
        let label = format!("Insert {name}");
        self.apply_with(|state| {
            state.image_references.push(reference);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Changes an image's settings, its plane or the image itself. A
    /// change that changes nothing is no undo step.
    pub fn edit_image_reference(&mut self, name: &str, edit: ImageEdit) -> Result<(), ModelError> {
        let i = self.image_index(name)?;
        let moved = edit.plane.is_some();
        let mut image = self.state.image_references[i].clone();
        image.apply(edit).map_err(invalid)?;
        image.check().map_err(invalid)?;
        if moved {
            self.image_plane(&image.plane, image.occurrence.as_ref())
                .map_err(|e| invalid(format!("the image's plane: {e}")))?;
        }
        if image == self.state.image_references[i] {
            return Ok(());
        }
        let label = format!("Edit {name}");
        self.apply_with(|state| {
            state.image_references[i] = image;
            Ok((label, (), false))
        })
    }

    /// Scales an image about the first point so that the two points are
    /// `distance` mm apart. Returns the new size of a pixel.
    pub fn calibrate_image_reference(
        &mut self,
        name: &str,
        points: [CalibrationPoint; 2],
        distance: f64,
    ) -> Result<f64, ModelError> {
        if distance <= 0.0 || !distance.is_finite() {
            return Err(invalid(format!(
                "the distance must be greater than zero, got {distance}"
            )));
        }
        let i = self.image_index(name)?;
        let mut image = self.state.image_references[i].clone();
        let placement = image.placement();
        let size = image.image.size();
        let mut plane = None;
        let mut pixels = [[0.0; 2]; 2];
        for (pixel, point) in pixels.iter_mut().zip(points) {
            *pixel = match point {
                CalibrationPoint::Pixel(pixel) => pixel,
                CalibrationPoint::Point(point) => {
                    if plane.is_none() {
                        let found = self
                            .image_plane(&image.plane, image.occurrence.as_ref())
                            .map_err(|e| invalid(format!("the image's plane: {e}")))?;
                        plane = Some(found);
                    }
                    let at = in_plane(plane.as_ref().expect("found"), point);
                    placement.pixel(size, image.scale, at)
                }
            };
            if !pixel.iter().all(|c| c.is_finite()) {
                return Err(invalid("the calibration's points must be finite"));
            }
        }
        let du = pixels[1][0] - pixels[0][0];
        let dv = pixels[1][1] - pixels[0][1];
        let apart = du.hypot(dv);
        if apart < 1e-9 {
            return Err(invalid("the calibration's two points are the same point"));
        }
        let scale = distance / apart;
        // The first point stays where it is.
        let first = placement.at(size, image.scale, pixels[0]);
        let turned = Placement::of([0.0, 0.0], image.angle, image.flip);
        let moved = turned.at(size, scale, pixels[0]);
        image.center = [first[0] - moved[0], first[1] - moved[1]];
        image.scale = scale;
        image.check().map_err(invalid)?;
        let label = format!("Calibrate {name}");
        self.apply_with(|state| {
            state.image_references[i] = image;
            Ok((label, (), false))
        })?;
        Ok(scale)
    }

    /// Shows or hides an image; an undo step `Show Image1` / `Hide Image1`
    /// unless nothing changes.
    pub fn set_image_reference_visible(
        &mut self,
        name: &str,
        visible: bool,
    ) -> Result<(), ModelError> {
        let i = self.image_index(name)?;
        if self.state.image_references[i].visible == visible {
            return Ok(());
        }
        let label = format!("{} {name}", if visible { "Show" } else { "Hide" });
        self.apply_with(|state| {
            state.image_references[i].visible = visible;
            Ok((label, (), false))
        })
    }

    pub fn rename_image_reference(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        let i = self.image_index(name)?;
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(invalid("the image reference needs a name"));
        }
        if new_name == name {
            return Ok(());
        }
        if self.image_index(new_name).is_ok() {
            return Err(invalid(format!("an image reference {new_name} exists")));
        }
        let label = format!("Rename {name}");
        let new_name = new_name.to_owned();
        self.apply_with(|state| {
            state.image_references[i].name = new_name;
            Ok((label, (), false))
        })
    }

    pub fn delete_image_reference(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.image_index(name)?;
        let label = format!("Delete {name}");
        self.apply_with(|state| {
            state.image_references.remove(i);
            Ok((label, (), false))
        })
    }

    fn image_index(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .image_references
            .iter()
            .position(|image| image.name == name)
            .ok_or_else(|| invalid(format!("there is no image reference {name}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn pixels_and_places_are_inverse() {
        let size = [200.0, 100.0];
        for (angle, flip) in [(0.0, false), (30.0, false), (90.0, true), (-45.0, true)] {
            let placement = Placement::of([5.0, -7.0], angle, flip);
            for pixel in [[0.0, 0.0], [200.0, 100.0], [13.5, 77.25]] {
                let at = placement.at(size, 0.25, pixel);
                assert!(near(placement.pixel(size, 0.25, at), pixel));
            }
            // The centre pixel is the centre.
            assert!(near(placement.at(size, 0.25, [100.0, 50.0]), [5.0, -7.0]));
        }
    }

    #[test]
    fn corners_turn_and_flip_on_the_plane() {
        let xy = DatumPlane {
            origin: [0.0, 0.0, 0.0],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
        };
        let corners = image_corners(&xy, [200, 100], [0.0, 0.0], 0.0, false, 0.5);
        assert_eq!(corners[0], [-50.0, 25.0, 0.0]);
        assert_eq!(corners[2], [50.0, -25.0, 0.0]);
        // Flipped: the left edge on the right.
        let flipped = image_corners(&xy, [200, 100], [0.0, 0.0], 0.0, true, 0.5);
        assert_eq!(flipped[0], [50.0, 25.0, 0.0]);
        // A quarter turn: the top edge on the left.
        let turned = image_corners(&xy, [200, 100], [10.0, 0.0], 90.0, false, 0.5);
        let top_left = turned[0];
        assert!((top_left[0] - (10.0 - 25.0)).abs() < 1e-9);
        assert!((top_left[1] - -50.0).abs() < 1e-9);
    }
}
