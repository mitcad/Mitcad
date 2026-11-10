// SPDX-License-Identifier: MIT
//! Appearances: how bodies look (mitcad#46). An appearance is a small
//! physically based parameter set, a subset of OpenPBR's and the Principled
//! BSDF's: base colour, metalness, roughness, specular, transmission and
//! index of refraction, coat, emission and opacity, with an optional
//! texture for the base colour (mitcad#53: an image file, or the image
//! embedded in the design file, projected onto the body) and, with it, the
//! other maps of a physically based texture set: normal, roughness,
//! metalness and occlusion (mitcad#313), and where the set came from
//! (`source`). The shaded view colours a body with the appearance's
//! display colour, its base colour; the rendered view
//! (`docs/rendering.md`) uses all parameters and the texture.
//!
//! An embedded image is an [`ImageData`], shared (one copy in memory) by
//! every texture that uses the same image and by the states undo keeps; a
//! design file stores each image once by its SHA-256 (`file/v2.rs`).
//!
//! Mitcad's library ([`library`]) is built in and read-only; a document
//! keeps its own appearances (`document/appearances.rs`), saved in the
//! design file. A body names its appearance by id
//! (`set_body_appearance`), and single faces of it can have their own
//! (`set_face_appearance`, mitcad#53); an id that is in neither (an
//! imported one) shows the default look.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, LazyLock, OnceLock};

use serde::{Deserialize, Serialize};

pub use crate::material_class::MaterialClass;

/// An sRGB colour (the encoded values a colour picker shows), each
/// component in [0, 1].
pub type Color = [f64; 3];

/// An appearance: its id, name and physically based parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    /// The id bodies refer to (`paint_red`, `custom1`).
    pub id: String,
    pub name: String,
    /// The colour of a dielectric's diffuse reflection, a metal's
    /// reflection, and the tint of transmitted light.
    pub base_color: Color,
    /// 0 for a dielectric (plastic, paint, glass), 1 for a metal.
    pub metalness: f64,
    /// 0 for a mirror finish, 1 for a fully diffuse one.
    pub roughness: f64,
    /// The weight of a dielectric's specular reflection (1: as its index of
    /// refraction gives, 0: none).
    pub specular: f64,
    /// The share of light going through (glass, clear plastic).
    pub transmission: f64,
    /// Index of refraction (1.5: glass and most plastics).
    pub ior: f64,
    /// A clear coat over the base (lacquer, car paint): its weight and its
    /// roughness.
    pub coat: f64,
    pub coat_roughness: f64,
    /// Emitted light: its strength (0: none; 1: the colour's own radiance)
    /// and colour.
    pub emission: f64,
    pub emission_color: Color,
    /// 1 opaque, 0 invisible (a cut-out, not glass: glass transmits).
    pub opacity: f64,
    /// An image for the base colour, projected onto the body, with the
    /// other maps of its set; the rendered view draws it, the shaded view
    /// only the base colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture: Option<Texture>,
    /// Its class (mitcad#1102): what it is a look of, by which the pickers
    /// offer a material's class first. Set by the user, or on a filament's
    /// colour (FDM Plastic); none: a library's by its id
    /// ([`library_class`]), else Other ([`Appearance::material_class`]).
    /// Written only when set: older versions refuse the field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<MaterialClass>,
}

/// An image for an appearance's base colour (PNG or JPEG), and the other
/// maps of a physically based texture set (mitcad#313), which share its
/// size, rotation and projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Texture {
    /// The image file, relative to the design file's folder or absolute.
    /// With `data`, the name of the file the image was embedded from.
    pub path: String,
    /// How large one repeat of the image is on the body (width, height), in
    /// mm.
    #[serde(default = "default_texture_size")]
    pub size: [f64; 2],
    /// The image's rotation about the projection's axis, in radians.
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub projection: Projection,
    /// The image file's bytes, when the image is embedded in the design
    /// file (mitcad#53); else the file at `path` is read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ImageData>,
    /// Commands only, never kept: the texture keeps the embedded image the
    /// appearance (or `based_on`) has, as the `appearances` query lists an
    /// embedded texture (`"embedded": true` instead of its data).
    #[serde(default, skip_serializing)]
    pub embedded: bool,
    /// Commands only, never kept: the digest the `appearances` query lists
    /// with `embedded`; with it, `embedded` keeps the image of that digest
    /// the document embeds (mitcad#313).
    #[serde(default, skip_serializing)]
    pub image_sha256: Option<String>,
    /// Design files only, never kept: the embedded image by its digest in
    /// the file's `images` (mitcad#313), instead of `data`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// The other maps of the set (mitcad#313), each optional: a normal map
    /// (tangent space, the OpenGL convention: green up), and roughness,
    /// metalness and ambient occlusion images (grey values 0 to 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal: Option<TextureMap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roughness: Option<TextureMap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metalness: Option<TextureMap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occlusion: Option<TextureMap>,
    /// Where the set came from (mitcad#313).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<TextureSource>,
}

impl Default for Texture {
    /// No image yet, 100 x 100 mm, box projection.
    fn default() -> Self {
        Self {
            path: String::new(),
            size: default_texture_size(),
            rotation: 0.0,
            projection: Projection::default(),
            data: None,
            embedded: false,
            image_sha256: None,
            image: None,
            normal: None,
            roughness: None,
            metalness: None,
            occlusion: None,
            source: None,
        }
    }
}

/// The maps of a texture set besides the base colour, in the order they
/// are listed and saved.
pub const MAPS: [&str; 4] = ["normal", "roughness", "metalness", "occlusion"];

/// The base colour's name among the maps (the `appearance_image` query's
/// `map`).
pub const BASE_COLOR: &str = "base_color";

/// How messages name an image of the set: `texture` (the base colour) or
/// `texture: normal`.
pub fn slot_name(map: &str) -> String {
    if map == BASE_COLOR {
        "texture".to_owned()
    } else {
        format!("texture: {map}")
    }
}

/// One map of a texture set other than the base colour: an image file or
/// an embedded image, as the base colour's.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureMap {
    /// The image file, relative to the design file's folder or absolute.
    /// With `data`, the name of the file the image was embedded from.
    pub path: String,
    /// The embedded image file's bytes; else the file at `path` is read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ImageData>,
    /// Commands only, as the base colour's.
    #[serde(default, skip_serializing)]
    pub embedded: bool,
    #[serde(default, skip_serializing)]
    pub image_sha256: Option<String>,
    /// Design files only, as the base colour's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// Where a texture set came from (mitcad#313): the texture source, the
/// asset in it, the resolution fetched, its licence and its web page. It
/// lets the application show the licence and fetch the images again in
/// another resolution; the model never fetches anything.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureSource {
    /// The source's id (`polyhaven`, `folder`): letters, digits, `_`, `-`.
    pub id: String,
    /// The asset's id in the source (`oak_veneer_01`; a folder source's
    /// material name).
    pub asset: String,
    /// The resolution the images were fetched in (`1k`, `2k`, `4k`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    /// The licence of the images, an SPDX identifier (`CC0-1.0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// The asset's web page (http or https).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl TextureSource {
    fn check(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(format!(
                "texture: source: {} is no source id: use letters, digits, '_' and '-'",
                self.id
            ));
        }
        let text = |field: &str, value: &str| {
            if value.trim().is_empty() || value.trim() != value {
                Err(format!(
                    "texture: source: the {field} is empty or has spaces around it"
                ))
            } else if value.len() > 2048 || value.chars().any(char::is_control) {
                Err(format!(
                    "texture: source: the {field} is longer than 2048 characters or has \
                     control characters"
                ))
            } else {
                Ok(())
            }
        };
        text("asset", &self.asset)?;
        for (field, value) in [
            ("resolution", &self.resolution),
            ("license", &self.license),
            ("url", &self.url),
        ] {
            if let Some(value) = value {
                text(field, value)?;
            }
        }
        if let Some(url) = &self.url
            && !(url.starts_with("https://") || url.starts_with("http://"))
        {
            return Err(format!(
                "texture: source: the url must be an http or https address, got “{url}”"
            ));
        }
        Ok(())
    }
}

/// One image of a texture set, the base colour or a map, for code that
/// handles them all alike.
pub struct ImageSlot<'a> {
    /// `base_color` or one of [`MAPS`].
    pub map: &'static str,
    pub path: &'a mut String,
    pub data: &'a mut Option<ImageData>,
    pub embedded: &'a mut bool,
    pub image_sha256: &'a mut Option<String>,
    pub image: &'a mut Option<String>,
}

impl Texture {
    /// A map by name (one of [`MAPS`]).
    pub fn map(&self, name: &str) -> Option<&TextureMap> {
        match name {
            "normal" => self.normal.as_ref(),
            "roughness" => self.roughness.as_ref(),
            "metalness" => self.metalness.as_ref(),
            "occlusion" => self.occlusion.as_ref(),
            _ => None,
        }
    }

    /// Every image of the set: its map name, its path and its embedded
    /// image, the base colour first, then the maps there are in the order
    /// of [`MAPS`].
    pub fn slots(&self) -> impl Iterator<Item = (&'static str, &str, Option<&ImageData>)> {
        std::iter::once((BASE_COLOR, self.path.as_str(), self.data.as_ref())).chain(
            MAPS.iter()
                .filter_map(|m| self.map(m).map(|t| (*m, t.path.as_str(), t.data.as_ref()))),
        )
    }

    /// The embedded image of the base colour (`base_color`) or a map.
    pub fn image_data(&self, map: &str) -> Option<&ImageData> {
        if map == BASE_COLOR {
            self.data.as_ref()
        } else {
            self.map(map)?.data.as_ref()
        }
    }

    /// The embedded images: the base colour's, then the maps' in the order
    /// of [`MAPS`], with their map names.
    pub fn images(&self) -> impl Iterator<Item = (&'static str, &ImageData)> {
        self.slots().filter_map(|(m, _, d)| Some((m, d?)))
    }

    /// Every image of the set, the base colour first, then the maps there
    /// are in the order of [`MAPS`].
    pub fn slots_mut(&mut self) -> Vec<ImageSlot<'_>> {
        let mut slots = vec![ImageSlot {
            map: BASE_COLOR,
            path: &mut self.path,
            data: &mut self.data,
            embedded: &mut self.embedded,
            image_sha256: &mut self.image_sha256,
            image: &mut self.image,
        }];
        for (map, texture) in [
            ("normal", &mut self.normal),
            ("roughness", &mut self.roughness),
            ("metalness", &mut self.metalness),
            ("occlusion", &mut self.occlusion),
        ] {
            if let Some(t) = texture.as_mut() {
                slots.push(ImageSlot {
                    map,
                    path: &mut t.path,
                    data: &mut t.data,
                    embedded: &mut t.embedded,
                    image_sha256: &mut t.image_sha256,
                    image: &mut t.image,
                });
            }
        }
        slots
    }

    /// The texture's part of [`Appearance::check`].
    fn check(&self) -> Result<(), String> {
        if !self.size.iter().all(|s| s.is_finite() && *s > 0.0) {
            return Err(format!(
                "texture: the size must be positive, got {:?}",
                self.size
            ));
        }
        if !self.rotation.is_finite() {
            return Err("texture: the rotation must be a finite angle".to_owned());
        }
        for (map, path, data) in self.slots() {
            let what = slot_name(map);
            if path.trim().is_empty() {
                return Err(format!("{what}: the image path is empty"));
            }
            if let Some(data) = data {
                data.bytes().map_err(|e| format!("{what}: {e}"))?;
            }
        }
        if let Some(source) = &self.source {
            source.check()?;
        }
        Ok(())
    }
}

/// The largest image a design file embeds (decoded), 32 MB; the limit
/// holds per image (each map of a set is one).
pub const MAX_EMBEDDED_IMAGE: usize = 32 << 20;

/// An embedded image: the file's bytes as base64, shared by the textures
/// that use the same image and by the copies of the document's state that
/// undo keeps. Its SHA-256 is computed once.
#[derive(Clone)]
pub struct ImageData(Arc<ImageInner>);

struct ImageInner {
    base64: Box<str>,
    sha256: OnceLock<String>,
}

impl ImageData {
    /// The image file's bytes, as base64.
    pub fn new(base64: &str) -> Self {
        Self(Arc::new(ImageInner {
            base64: Box::from(base64),
            sha256: OnceLock::new(),
        }))
    }

    /// The image file's bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let image = Self::new(&crate::base64::encode(bytes));
        let _ = image
            .0
            .sha256
            .set(crate::sha256::Sha256::of(bytes).to_string());
        image
    }

    pub fn base64(&self) -> &str {
        &self.0.base64
    }

    /// The image file's bytes, or why they are no PNG or JPEG image.
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        let bytes = crate::base64::decode(&self.0.base64)
            .map_err(|e| format!("the embedded image is no base64 data: {e}"))?;
        if bytes.len() > MAX_EMBEDDED_IMAGE {
            return Err(format!(
                "the embedded image is {} MB, more than the {} MB a design file embeds \
                 per image",
                bytes.len() >> 20,
                MAX_EMBEDDED_IMAGE >> 20
            ));
        }
        image_format(&bytes)
            .map(|_| bytes.clone())
            .ok_or_else(|| "the embedded image is no PNG or JPEG file".to_owned())
    }

    /// The SHA-256 of the image file's bytes (hex): names the image.
    pub fn sha256(&self) -> &str {
        self.0.sha256.get_or_init(|| {
            let bytes = crate::base64::decode(&self.0.base64).unwrap_or_default();
            crate::sha256::Sha256::of(&bytes).to_string()
        })
    }

    /// Whether both are the same copy in memory (not only equal).
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl PartialEq for ImageData {
    fn eq(&self, other: &Self) -> bool {
        self.same(other) || self.0.base64 == other.0.base64
    }
}

impl Eq for ImageData {}

impl fmt::Debug for ImageData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ImageData({} base64 characters)", self.0.base64.len())
    }
}

impl Serialize for ImageData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.base64)
    }
}

impl<'de> Deserialize<'de> for ImageData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(Self::new(&text))
    }
}

/// The embedded images of `appearances` by digest: one copy of each,
/// the first found.
pub fn embedded_images<'a>(
    appearances: impl IntoIterator<Item = &'a Appearance>,
) -> HashMap<String, ImageData> {
    let mut images = HashMap::new();
    for texture in appearances.into_iter().filter_map(|a| a.texture.as_ref()) {
        for (_, data) in texture.images() {
            images
                .entry(data.sha256().to_owned())
                .or_insert_with(|| data.clone());
        }
    }
    images
}

/// Calls `f` with the JSON object of each image of a texture's JSON form:
/// the texture itself (the base colour), then each map's.
#[doc(hidden)]
pub fn each_image_json(
    texture: &mut serde_json::Value,
    mut f: impl FnMut(&'static str, &mut serde_json::Map<String, serde_json::Value>),
) {
    let Some(object) = texture.as_object_mut() else {
        return;
    };
    f(BASE_COLOR, object);
    for map in MAPS {
        if let Some(map_object) = object.get_mut(map).and_then(|m| m.as_object_mut()) {
            f(map, map_object);
        }
    }
}

/// Makes the embedded images of `texture` the copies in `images` where
/// these have the same image, so the document keeps one copy of each;
/// adds the new ones to `images`.
pub fn share_images(texture: &mut Texture, images: &mut HashMap<String, ImageData>) {
    for slot in texture.slots_mut() {
        if let Some(data) = slot.data.as_mut() {
            let shared = images
                .entry(data.sha256().to_owned())
                .or_insert_with(|| data.clone());
            if !shared.same(data) {
                *data = shared.clone();
            }
        }
    }
}

/// `png` or `jpeg` by the file's first bytes.
pub fn image_format(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpeg")
    } else {
        None
    }
}

fn default_texture_size() -> [f64; 2] {
    [100.0, 100.0]
}

/// How a texture is put onto a body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    /// From the three axis directions, each point of a surface from the
    /// one it faces most.
    #[default]
    Box,
    /// Along the Z axis of the body's coordinates.
    Planar,
}

impl Default for Appearance {
    /// The look of a body without an appearance: a light grey dielectric.
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            base_color: [0.8, 0.8, 0.8],
            metalness: 0.0,
            roughness: 0.5,
            specular: 1.0,
            transmission: 0.0,
            ior: 1.5,
            coat: 0.0,
            coat_roughness: 0.03,
            emission: 0.0,
            emission_color: [1.0, 1.0, 1.0],
            opacity: 1.0,
            texture: None,
            class: None,
        }
    }
}

impl Appearance {
    /// The colour the shaded view and exports give the body: its base
    /// colour.
    pub fn display_color(&self) -> Color {
        self.base_color
    }

    /// Its class (mitcad#1102): its own, else a library appearance's by
    /// its id, else Other.
    pub fn material_class(&self) -> MaterialClass {
        self.class
            .or_else(|| library_class(&self.id))
            .unwrap_or(MaterialClass::Other)
    }

    /// Why the parameters cannot be kept, if they cannot (the id is checked
    /// by the document).
    pub fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the appearance needs a name".to_owned());
        }
        for (field, color) in [
            ("base_color", self.base_color),
            ("emission_color", self.emission_color),
        ] {
            if !color.iter().all(|c| (0.0..=1.0).contains(c)) {
                return Err(format!(
                    "{field}: color components must be between 0 and 1, got {color:?}"
                ));
            }
        }
        for (field, value) in [
            ("metalness", self.metalness),
            ("roughness", self.roughness),
            ("specular", self.specular),
            ("transmission", self.transmission),
            ("coat", self.coat),
            ("coat_roughness", self.coat_roughness),
            ("opacity", self.opacity),
        ] {
            if !(0.0..=1.0).contains(&value) {
                return Err(format!("{field} must be between 0 and 1, got {value}"));
            }
        }
        if !(1.0..=5.0).contains(&self.ior) {
            return Err(format!("ior must be between 1 and 5, got {}", self.ior));
        }
        if !(0.0..=1000.0).contains(&self.emission) {
            return Err(format!(
                "emission must be between 0 and 1000, got {}",
                self.emission
            ));
        }
        if let Some(texture) = &self.texture {
            texture.check()?;
        }
        Ok(())
    }
}

/// The fields of `create_appearance` and `edit_appearance`: those given
/// change, the others stay (those of `based_on`, or the default, for a new
/// one).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceChange {
    pub id: Option<String>,
    /// create_appearance: the library or document appearance whose
    /// parameters a new one starts from.
    pub based_on: Option<String>,
    pub name: Option<String>,
    pub base_color: Option<Color>,
    pub metalness: Option<f64>,
    pub roughness: Option<f64>,
    pub specular: Option<f64>,
    pub transmission: Option<f64>,
    pub ior: Option<f64>,
    pub coat: Option<f64>,
    pub coat_roughness: Option<f64>,
    pub emission: Option<f64>,
    pub emission_color: Option<Color>,
    pub opacity: Option<f64>,
    /// A texture, or null for none.
    #[serde(default, deserialize_with = "present")]
    pub texture: Option<Option<Texture>>,
    /// Its class (mitcad#1102), or null for none (Other); `other` is
    /// kept as none.
    #[serde(default, deserialize_with = "present")]
    pub class: Option<Option<MaterialClass>>,
}

/// A field that is there, also as null (`Some(None)`).
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl AppearanceChange {
    /// Sets the given parameters and name of `appearance`.
    pub fn apply_to(&self, appearance: &mut Appearance) {
        fn set<T: Clone>(target: &mut T, value: &Option<T>) {
            if let Some(value) = value {
                *target = value.clone();
            }
        }
        if let Some(name) = &self.name {
            appearance.name = name.trim().to_owned();
        }
        set(&mut appearance.base_color, &self.base_color);
        set(&mut appearance.metalness, &self.metalness);
        set(&mut appearance.roughness, &self.roughness);
        set(&mut appearance.specular, &self.specular);
        set(&mut appearance.transmission, &self.transmission);
        set(&mut appearance.ior, &self.ior);
        set(&mut appearance.coat, &self.coat);
        set(&mut appearance.coat_roughness, &self.coat_roughness);
        set(&mut appearance.emission, &self.emission);
        set(&mut appearance.emission_color, &self.emission_color);
        set(&mut appearance.opacity, &self.opacity);
        set(&mut appearance.texture, &self.texture);
        if let Some(class) = self.class {
            appearance.class = class.filter(|c| *c != MaterialClass::Other);
        }
    }
}

/// Whether `id` can name an appearance: letters, digits, `_` and `-`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// One entry of the library: sRGB as 0xRRGGBB and the parameters that
/// differ from the default.
struct Entry {
    id: &'static str,
    name: &'static str,
    rgb: u32,
    metalness: f64,
    roughness: f64,
    specular: f64,
    transmission: f64,
    coat: f64,
    emission: f64,
    /// Its class (mitcad#1102), never written: the id gives it.
    class: MaterialClass,
}

const fn dielectric(id: &'static str, name: &'static str, rgb: u32, roughness: f64) -> Entry {
    Entry {
        id,
        name,
        rgb,
        metalness: 0.0,
        roughness,
        specular: 1.0,
        transmission: 0.0,
        coat: 0.0,
        emission: 0.0,
        class: MaterialClass::Other,
    }
}

const fn metal(id: &'static str, name: &'static str, rgb: u32, roughness: f64) -> Entry {
    Entry {
        metalness: 1.0,
        class: MaterialClass::Metal,
        ..dielectric(id, name, rgb, roughness)
    }
}

const fn paint(id: &'static str, name: &'static str, rgb: u32) -> Entry {
    Entry {
        coat: 0.3,
        ..dielectric(id, name, rgb, 0.4)
    }
}

const fn clear(id: &'static str, name: &'static str, rgb: u32, roughness: f64) -> Entry {
    Entry {
        transmission: 1.0,
        ..dielectric(id, name, rgb, roughness)
    }
}

/// A General Plastic's look (mitcad#1102).
const fn plastic(entry: Entry) -> Entry {
    Entry {
        class: MaterialClass::Plastic,
        ..entry
    }
}

/// A resin print's look (mitcad#1102): a smooth, slightly glossy
/// photopolymer.
const fn resin(entry: Entry) -> Entry {
    Entry {
        class: MaterialClass::Resin,
        ..entry
    }
}

/// A wood's or a wood-based panel's look (mitcad#1552): dielectric, a
/// little specular, no coat.
const fn wood(entry: Entry) -> Entry {
    Entry {
        specular: 0.5,
        class: MaterialClass::Wood,
        ..entry
    }
}

/// A filament's look (mitcad#1133): a printed thermoplastic, dielectric,
/// the layered surface of a print rough, a little specular, no coat.
const fn filament(id: &'static str, name: &'static str, rgb: u32) -> Entry {
    Entry {
        specular: 0.5,
        class: MaterialClass::FdmPlastic,
        ..dielectric(id, name, rgb, 0.55)
    }
}

/// The library in the order the application lists it. The first sixteen
/// were colours only before mitcad#46 and keep their colours.
const ENTRIES: &[Entry] = &[
    metal("steel_satin", "Steel - Satin", 0xb4b8be, 0.35),
    metal("aluminum_anodized", "Aluminum - Anodized", 0xd2d6dc, 0.4),
    metal("brass_polished", "Brass - Polished", 0xc8a84b, 0.15),
    metal("copper", "Copper", 0xc06c3e, 0.25),
    metal("cast_iron", "Iron - Cast", 0x5a5c60, 0.7),
    paint("paint_red", "Paint - Red", 0xc82828),
    paint("paint_blue", "Paint - Blue", 0x2a5cc0),
    paint("paint_green", "Paint - Green", 0x3a9a48),
    paint("paint_yellow", "Paint - Yellow", 0xe8c420),
    paint("paint_black", "Paint - Black", 0x2a2c30),
    paint("paint_white", "Paint - White", 0xf0f0ec),
    plastic(dielectric(
        "plastic_black",
        "Plastic - Black",
        0x34363a,
        0.4,
    )),
    plastic(dielectric(
        "plastic_orange",
        "Plastic - Orange",
        0xe87a1e,
        0.35,
    )),
    Entry {
        specular: 0.5,
        ..dielectric("rubber", "Rubber", 0x222224, 0.85)
    },
    clear("glass", "Glass", 0xa8d0e0, 0.0),
    wood(dielectric("wood_oak", "Wood - Oak", 0xb08450, 0.6)),
    metal("chrome", "Chrome", 0xc4c7ca, 0.05),
    metal("gold_polished", "Gold - Polished", 0xf5d27a, 0.12),
    metal("steel_polished", "Steel - Polished", 0xb8bcc2, 0.12),
    plastic(dielectric("plastic_red", "Plastic - Red", 0xc8201e, 0.3)),
    plastic(dielectric(
        "plastic_white",
        "Plastic - White",
        0xeeeeea,
        0.35,
    )),
    plastic(dielectric("plastic_blue", "Plastic - Blue", 0x1f4fb0, 0.3)),
    plastic(clear("plastic_clear", "Plastic - Clear", 0xf4f6f8, 0.1)),
    clear("glass_clear", "Glass - Clear", 0xf6fafa, 0.0),
    clear("glass_frosted", "Glass - Frosted", 0xf0f4f4, 0.45),
    wood(dielectric("wood_walnut", "Wood - Walnut", 0x6b4a30, 0.55)),
    wood(dielectric("wood_maple", "Wood - Maple", 0xd8b888, 0.55)),
    Entry {
        emission: 5.0,
        ..dielectric("emissive_white", "Emissive - White", 0xffffff, 0.5)
    },
    // Resin prints (mitcad#1102).
    resin(dielectric("resin_grey", "Resin - Gray", 0x8c8f93, 0.3)),
    resin(dielectric("resin_white", "Resin - White", 0xf2f2ee, 0.3)),
    resin(clear("resin_clear", "Resin - Clear", 0xf2f6f6, 0.08)),
    // The basic colours of generic filaments (mitcad#1133). Their ids are
    // stable: never renamed.
    filament("filament_white", "Filament - White", 0xf4f4f1),
    filament("filament_black", "Filament - Black", 0x1f2022),
    filament("filament_grey", "Filament - Gray", 0x8c8f92),
    filament("filament_silver", "Filament - Silver", 0xb4b7ba),
    filament("filament_red", "Filament - Red", 0xc8102e),
    filament("filament_orange", "Filament - Orange", 0xf47b20),
    filament("filament_yellow", "Filament - Yellow", 0xf5d130),
    filament("filament_green", "Filament - Green", 0x2e9b48),
    filament("filament_blue", "Filament - Blue", 0x1f58b5),
    filament("filament_light_blue", "Filament - Light Blue", 0x6cb2e2),
    filament("filament_purple", "Filament - Purple", 0x6b3fa0),
    filament("filament_pink", "Filament - Pink", 0xf28cb6),
    filament("filament_brown", "Filament - Brown", 0x7a4a2a),
    filament("filament_beige", "Filament - Beige", 0xe5d4ae),
    Entry {
        transmission: 0.7,
        roughness: 0.45,
        ..filament("filament_translucent", "Filament - Translucent", 0xeef0ec)
    },
    // The looks of the bundled dataset's woods and panels (mitcad#1552),
    // colours only. Their ids are stable: never renamed.
    wood(dielectric("wood_birch", "Wood - Birch", 0xe0c49a, 0.55)),
    wood(dielectric("wood_cedar", "Wood - Cedar", 0xa8643c, 0.55)),
    wood(dielectric("wood_cherry", "Wood - Cherry", 0x9a4f2c, 0.5)),
    wood(dielectric(
        "wood_douglas_fir",
        "Wood - Douglas Fir",
        0xc89060,
        0.55,
    )),
    wood(dielectric(
        "wood_eucalyptus",
        "Wood - Eucalyptus",
        0xbf9a6a,
        0.55,
    )),
    wood(dielectric("wood_pine", "Wood - Pine", 0xe2c58e, 0.55)),
    wood(dielectric("wood_teak", "Wood - Teak", 0xa8743c, 0.5)),
    wood(dielectric("panel_chipboard", "Chipboard", 0xc8ab7c, 0.8)),
    wood(dielectric("panel_mdf", "MDF", 0xb38a5e, 0.75)),
    wood(dielectric("panel_osb", "OSB", 0xc89a5a, 0.75)),
    wood(dielectric(
        "plywood_birch",
        "Plywood - Birch",
        0xe6cfa4,
        0.6,
    )),
    wood(dielectric(
        "plywood_eucalyptus",
        "Plywood - Eucalyptus",
        0xc49a70,
        0.6,
    )),
    wood(dielectric("plywood_pine", "Plywood - Pine", 0xdcc08c, 0.6)),
    wood(dielectric(
        "plywood_spruce",
        "Plywood - Spruce",
        0xe8d2a6,
        0.6,
    )),
];

/// The looks the library took with the woods and panels (mitcad#1552),
/// after designs could have used their ids for appearances of their own.
const WOOD_LOOKS_TAKEN_LATER: [&str; 14] = [
    "wood_birch",
    "wood_cedar",
    "wood_cherry",
    "wood_douglas_fir",
    "wood_eucalyptus",
    "wood_pine",
    "wood_teak",
    "panel_chipboard",
    "panel_mdf",
    "panel_osb",
    "plywood_birch",
    "plywood_eucalyptus",
    "plywood_pine",
    "plywood_spruce",
];

fn srgb(rgb: u32) -> Color {
    [16, 8, 0].map(|shift| f64::from((rgb >> shift) & 0xff) / 255.0)
}

/// Mitcad's appearance library, in the order the application lists it.
pub fn library() -> &'static [Appearance] {
    static LIBRARY: LazyLock<Vec<Appearance>> = LazyLock::new(|| {
        ENTRIES
            .iter()
            .map(|e| Appearance {
                id: e.id.to_owned(),
                name: e.name.to_owned(),
                base_color: srgb(e.rgb),
                metalness: e.metalness,
                roughness: e.roughness,
                specular: e.specular,
                transmission: e.transmission,
                coat: e.coat,
                emission: e.emission,
                ..Appearance::default()
            })
            .collect()
    });
    &LIBRARY
}

/// A library appearance by id.
pub fn library_appearance(id: &str) -> Option<&'static Appearance> {
    library().iter().find(|a| a.id == id)
}

/// Whether the library took `id` after designs could have used it for an
/// appearance of their own (mitcad#1133): the `filament_*` colours, and
/// the woods' and panels' looks of mitcad#1552. A design file reader gives
/// such an appearance of a design a free id; one with another library id
/// is refused, as no design could save it.
pub fn library_id_taken_later(id: &str) -> bool {
    (id.starts_with("filament_") || WOOD_LOOKS_TAKEN_LATER.contains(&id))
        && library_appearance(id).is_some()
}

/// A library appearance's class by its id (mitcad#1102): Steel, Aluminum,
/// Brass, Copper, Iron, Chrome and Gold are Metal, the Plastic ones General
/// Plastic, the Filament ones FDM Plastic (mitcad#1133), the Resin ones
/// Resin, the Wood, Plywood and panel ones Wood (mitcad#1552), the others
/// Other. Never stored.
pub fn library_class(id: &str) -> Option<MaterialClass> {
    ENTRIES.iter().find(|e| e.id == id).map(|e| e.class)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_is_valid_with_unique_ids_and_names() {
        let mut ids: Vec<&str> = library().iter().map(|a| a.id.as_str()).collect();
        let mut names: Vec<&str> = library().iter().map(|a| a.name.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        names.sort_unstable();
        names.dedup();
        assert_eq!(ids.len(), library().len());
        assert_eq!(names.len(), library().len());
        for appearance in library() {
            assert!(valid_id(&appearance.id), "{}", appearance.id);
            appearance
                .check()
                .unwrap_or_else(|e| panic!("{}: {e}", appearance.id));
        }
        // The colours from before mitcad#46 are kept.
        let red = library_appearance("paint_red").unwrap();
        assert_eq!(
            red.display_color(),
            [200.0 / 255.0, 40.0 / 255.0, 40.0 / 255.0]
        );
        assert_eq!(library_appearance("chrome").unwrap().metalness, 1.0);
        assert_eq!(library_appearance("glass").unwrap().transmission, 1.0);
    }

    /// mitcad#1102: a built-in's class comes from its id and is not
    /// written; one of the user's round-trips; `other` is none.
    #[test]
    fn library_classes_come_from_the_ids_and_own_ones_round_trip() {
        let class = |id: &str| library_appearance(id).unwrap().material_class();
        for id in [
            "steel_satin",
            "aluminum_anodized",
            "brass_polished",
            "copper",
            "cast_iron",
            "chrome",
            "gold_polished",
            "steel_polished",
        ] {
            assert_eq!(class(id), MaterialClass::Metal, "{id}");
        }
        for id in [
            "plastic_black",
            "plastic_orange",
            "plastic_red",
            "plastic_white",
            "plastic_blue",
            "plastic_clear",
        ] {
            assert_eq!(class(id), MaterialClass::Plastic, "{id}");
        }
        for id in ["paint_red", "rubber", "glass", "emissive_white"] {
            assert_eq!(class(id), MaterialClass::Other, "{id}");
        }
        // The woods are Wood (mitcad#1552), those from before too.
        for id in ["wood_oak", "wood_walnut", "wood_maple"]
            .into_iter()
            .chain(WOOD_LOOKS_TAKEN_LATER)
        {
            assert_eq!(class(id), MaterialClass::Wood, "{id}");
        }
        for id in ["resin_grey", "resin_white", "resin_clear"] {
            assert_eq!(class(id), MaterialClass::Resin, "{id}");
        }
        for appearance in library() {
            assert_eq!(appearance.class, None, "{}", appearance.id);
            let text = serde_json::to_value(appearance).unwrap();
            assert!(text.get("class").is_none(), "{text}");
        }
        // One of the user's: none is Other and not written; a class set
        // round-trips; `other` sets none.
        let mut own = Appearance {
            id: "my_red".to_owned(),
            name: "My Red".to_owned(),
            ..Appearance::default()
        };
        assert_eq!(own.material_class(), MaterialClass::Other);
        let change: AppearanceChange =
            serde_json::from_value(serde_json::json!({"class": "fdm_plastic"})).unwrap();
        change.apply_to(&mut own);
        let text = serde_json::to_value(&own).unwrap();
        assert_eq!(text["class"], "fdm_plastic");
        let back: Appearance = serde_json::from_value(text).unwrap();
        assert_eq!(back, own);
        assert_eq!(back.material_class(), MaterialClass::FdmPlastic);
        let change: AppearanceChange =
            serde_json::from_value(serde_json::json!({"class": "other"})).unwrap();
        change.apply_to(&mut own);
        assert_eq!(own.class, None);
        assert!(serde_json::to_value(&own).unwrap().get("class").is_none());
        let stone =
            serde_json::from_value::<AppearanceChange>(serde_json::json!({"class": "stone"}));
        assert!(stone.is_err());
    }

    /// mitcad#1133: the basic filament colours are FDM Plastic, each with
    /// its documented colour (docs/user-guide.md, Appearances), and the
    /// library from before them keeps its ids, order and colours.
    #[test]
    fn filament_colours_are_fdm_plastic_and_the_older_library_is_unchanged() {
        const FILAMENTS: [(&str, &str, u32); 15] = [
            ("filament_white", "Filament - White", 0xf4f4f1),
            ("filament_black", "Filament - Black", 0x1f2022),
            ("filament_grey", "Filament - Gray", 0x8c8f92),
            ("filament_silver", "Filament - Silver", 0xb4b7ba),
            ("filament_red", "Filament - Red", 0xc8102e),
            ("filament_orange", "Filament - Orange", 0xf47b20),
            ("filament_yellow", "Filament - Yellow", 0xf5d130),
            ("filament_green", "Filament - Green", 0x2e9b48),
            ("filament_blue", "Filament - Blue", 0x1f58b5),
            ("filament_light_blue", "Filament - Light Blue", 0x6cb2e2),
            ("filament_purple", "Filament - Purple", 0x6b3fa0),
            ("filament_pink", "Filament - Pink", 0xf28cb6),
            ("filament_brown", "Filament - Brown", 0x7a4a2a),
            ("filament_beige", "Filament - Beige", 0xe5d4ae),
            ("filament_translucent", "Filament - Translucent", 0xeef0ec),
        ];
        const BEFORE: [(&str, u32); 31] = [
            ("steel_satin", 0xb4b8be),
            ("aluminum_anodized", 0xd2d6dc),
            ("brass_polished", 0xc8a84b),
            ("copper", 0xc06c3e),
            ("cast_iron", 0x5a5c60),
            ("paint_red", 0xc82828),
            ("paint_blue", 0x2a5cc0),
            ("paint_green", 0x3a9a48),
            ("paint_yellow", 0xe8c420),
            ("paint_black", 0x2a2c30),
            ("paint_white", 0xf0f0ec),
            ("plastic_black", 0x34363a),
            ("plastic_orange", 0xe87a1e),
            ("rubber", 0x222224),
            ("glass", 0xa8d0e0),
            ("wood_oak", 0xb08450),
            ("chrome", 0xc4c7ca),
            ("gold_polished", 0xf5d27a),
            ("steel_polished", 0xb8bcc2),
            ("plastic_red", 0xc8201e),
            ("plastic_white", 0xeeeeea),
            ("plastic_blue", 0x1f4fb0),
            ("plastic_clear", 0xf4f6f8),
            ("glass_clear", 0xf6fafa),
            ("glass_frosted", 0xf0f4f4),
            ("wood_walnut", 0x6b4a30),
            ("wood_maple", 0xd8b888),
            ("emissive_white", 0xffffff),
            ("resin_grey", 0x8c8f93),
            ("resin_white", 0xf2f2ee),
            ("resin_clear", 0xf2f6f6),
        ];
        // The older library first, in its order and with its colours.
        assert!(library().len() >= BEFORE.len() + FILAMENTS.len());
        for ((id, rgb), appearance) in BEFORE.iter().zip(library()) {
            assert_eq!(appearance.id, *id);
            assert_eq!(appearance.base_color, srgb(*rgb), "{id}");
            assert_ne!(library_class(id), Some(MaterialClass::FdmPlastic), "{id}");
            assert!(!library_id_taken_later(id), "{id}");
        }
        let filaments: Vec<&Appearance> = library()
            .iter()
            .filter(|a| a.id.starts_with("filament_"))
            .collect();
        assert_eq!(filaments.len(), FILAMENTS.len());
        for (id, name, rgb) in FILAMENTS {
            let a = library_appearance(id).unwrap_or_else(|| panic!("{id} is missing"));
            assert_eq!(a.name, name);
            assert_eq!(a.base_color, srgb(rgb), "{id}");
            assert_eq!(library_class(id), Some(MaterialClass::FdmPlastic), "{id}");
            assert!(library_id_taken_later(id), "{id}");
            assert_eq!(a.material_class(), MaterialClass::FdmPlastic, "{id}");
            // Never written: the id gives it.
            assert_eq!(a.class, None, "{id}");
            // A printed thermoplastic's look.
            assert_eq!(a.metalness, 0.0, "{id}");
            assert_eq!(a.coat, 0.0, "{id}");
            assert_eq!(a.emission, 0.0, "{id}");
            assert!(a.roughness >= 0.4 && a.roughness <= 0.7, "{id}");
            assert!(a.specular > 0.0 && a.specular < 1.0, "{id}");
            let translucent = id == "filament_translucent";
            assert_eq!(a.transmission > 0.0, translucent, "{id}");
        }
        // Unique ids and names among the whole library.
        for a in &filaments {
            assert_eq!(library().iter().filter(|b| b.id == a.id).count(), 1);
            assert_eq!(library().iter().filter(|b| b.name == a.name).count(), 1);
        }
    }

    #[test]
    fn parameters_out_of_range_are_refused() {
        let ok = Appearance {
            name: "A".to_owned(),
            ..Appearance::default()
        };
        assert!(ok.check().is_ok());
        let cases = [
            Appearance {
                roughness: 1.5,
                ..ok.clone()
            },
            Appearance {
                ior: 0.5,
                ..ok.clone()
            },
            Appearance {
                base_color: [1.2, 0.0, 0.0],
                ..ok.clone()
            },
            Appearance {
                emission: -1.0,
                ..ok.clone()
            },
            Appearance {
                opacity: f64::NAN,
                ..ok.clone()
            },
            Appearance {
                name: " ".to_owned(),
                ..ok.clone()
            },
            Appearance {
                texture: Some(Texture {
                    path: "wood.png".to_owned(),
                    size: [0.0, 10.0],
                    ..Texture::default()
                }),
                ..ok.clone()
            },
            Appearance {
                texture: Some(Texture {
                    path: "wood.png".to_owned(),
                    normal: Some(TextureMap::default()),
                    ..Texture::default()
                }),
                ..ok.clone()
            },
            Appearance {
                texture: Some(Texture {
                    path: "wood.png".to_owned(),
                    source: Some(TextureSource {
                        id: "a source".to_owned(),
                        asset: "wood".to_owned(),
                        ..TextureSource::default()
                    }),
                    ..Texture::default()
                }),
                ..ok.clone()
            },
            Appearance {
                texture: Some(Texture {
                    path: "wood.png".to_owned(),
                    source: Some(TextureSource {
                        id: "folder".to_owned(),
                        asset: "wood".to_owned(),
                        url: Some("ftp://example.org/wood".to_owned()),
                        ..TextureSource::default()
                    }),
                    ..Texture::default()
                }),
                ..ok.clone()
            },
        ];
        for case in cases {
            assert!(case.check().is_err(), "{case:?}");
        }
        assert!(valid_id("my_red-2"));
        assert!(!valid_id("my red"));
        assert!(!valid_id(""));
    }
}
