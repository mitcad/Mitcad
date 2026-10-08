// SPDX-License-Identifier: MIT
//! Appearances: how bodies look (mitcad#46). An appearance is a small
//! physically based parameter set, a subset of OpenPBR's and the Principled
//! BSDF's: base colour, metalness, roughness, specular, transmission and
//! index of refraction, coat, emission and opacity, with an optional
//! texture for the base colour (mitcad#53: an image file, or the image
//! embedded in the project file, projected onto the body). The shaded view
//! colours a body with the appearance's display colour, its base colour;
//! the rendered view (`docs/rendering.md`) uses all parameters and the
//! texture.
//!
//! Mitcad's library ([`library`]) is built in and read-only; a document
//! keeps its own appearances (`document/appearances.rs`), saved in the
//! project file. A body names its appearance by id
//! (`set_body_appearance`), and single faces of it can have their own
//! (`set_face_appearance`, mitcad#53); an id that is in neither (an
//! imported one) shows the default look.

use std::fmt;
use std::sync::{Arc, LazyLock};

use serde::{Deserialize, Serialize};

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
    /// An image for the base colour, projected onto the body; the rendered
    /// view draws it, the shaded view only the base colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture: Option<Texture>,
}

/// An image for an appearance's base colour (PNG or JPEG).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Texture {
    /// The image file, relative to the project file's folder or absolute.
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
    /// The image file's bytes, when the image is embedded in the project
    /// file (mitcad#53); else the file at `path` is read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ImageData>,
    /// Commands only, never kept: the texture keeps the embedded image the
    /// appearance (or `based_on`) has, as the `appearances` query lists an
    /// embedded texture (`"embedded": true` instead of its data).
    #[serde(default, skip_serializing)]
    pub embedded: bool,
    /// Commands only, never kept: the digest the `appearances` query lists
    /// with `embedded`, accepted so that its form can be sent back.
    #[serde(default, skip_serializing)]
    pub image_sha256: Option<String>,
}

/// The largest image a project file embeds (decoded), 32 MB.
pub const MAX_EMBEDDED_IMAGE: usize = 32 << 20;

/// An embedded image: the file's bytes as base64, shared by the copies of
/// the document's state that undo keeps.
#[derive(Clone, PartialEq, Eq)]
pub struct ImageData(Arc<str>);

impl ImageData {
    /// The image file's bytes, as base64.
    pub fn new(base64: &str) -> Self {
        Self(Arc::from(base64))
    }

    pub fn base64(&self) -> &str {
        &self.0
    }

    /// The image file's bytes, or why they are no PNG or JPEG image.
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        let bytes = crate::base64::decode(&self.0)
            .map_err(|e| format!("texture: the embedded image is no base64 data: {e}"))?;
        if bytes.len() > MAX_EMBEDDED_IMAGE {
            return Err(format!(
                "texture: the embedded image is {} MB, more than the {} MB a project file \
                 embeds",
                bytes.len() >> 20,
                MAX_EMBEDDED_IMAGE >> 20
            ));
        }
        image_format(&bytes)
            .map(|_| bytes.clone())
            .ok_or_else(|| "texture: the embedded image is no PNG or JPEG file".to_owned())
    }

    /// The SHA-256 of the image file's bytes (hex): names the image.
    pub fn sha256(&self) -> String {
        let bytes = crate::base64::decode(&self.0).unwrap_or_default();
        crate::sha256::Sha256::of(&bytes).to_string()
    }
}

impl fmt::Debug for ImageData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ImageData({} base64 characters)", self.0.len())
    }
}

impl Serialize for ImageData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ImageData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(Self(Arc::from(text)))
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
        }
    }
}

impl Appearance {
    /// The colour the shaded view and exports give the body: its base
    /// colour.
    pub fn display_color(&self) -> Color {
        self.base_color
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
                    "{field}: colour components must be between 0 and 1, got {color:?}"
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
            if texture.path.trim().is_empty() {
                return Err("texture: the image path is empty".to_owned());
            }
            if !texture.size.iter().all(|s| s.is_finite() && *s > 0.0) {
                return Err(format!(
                    "texture: the size must be positive, got {:?}",
                    texture.size
                ));
            }
            if !texture.rotation.is_finite() {
                return Err("texture: the rotation must be a finite angle".to_owned());
            }
            if let Some(data) = &texture.data {
                data.bytes()?;
            }
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
    }
}

const fn metal(id: &'static str, name: &'static str, rgb: u32, roughness: f64) -> Entry {
    Entry {
        metalness: 1.0,
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
    dielectric("plastic_black", "Plastic - Black", 0x34363a, 0.4),
    dielectric("plastic_orange", "Plastic - Orange", 0xe87a1e, 0.35),
    Entry {
        specular: 0.5,
        ..dielectric("rubber", "Rubber", 0x222224, 0.85)
    },
    clear("glass", "Glass", 0xa8d0e0, 0.0),
    Entry {
        specular: 0.5,
        ..dielectric("wood_oak", "Wood - Oak", 0xb08450, 0.6)
    },
    metal("chrome", "Chrome", 0xc4c7ca, 0.05),
    metal("gold_polished", "Gold - Polished", 0xf5d27a, 0.12),
    metal("steel_polished", "Steel - Polished", 0xb8bcc2, 0.12),
    dielectric("plastic_red", "Plastic - Red", 0xc8201e, 0.3),
    dielectric("plastic_white", "Plastic - White", 0xeeeeea, 0.35),
    dielectric("plastic_blue", "Plastic - Blue", 0x1f4fb0, 0.3),
    clear("plastic_clear", "Plastic - Clear", 0xf4f6f8, 0.1),
    clear("glass_clear", "Glass - Clear", 0xf6fafa, 0.0),
    clear("glass_frosted", "Glass - Frosted", 0xf0f4f4, 0.45),
    Entry {
        specular: 0.5,
        ..dielectric("wood_walnut", "Wood - Walnut", 0x6b4a30, 0.55)
    },
    Entry {
        specular: 0.5,
        ..dielectric("wood_maple", "Wood - Maple", 0xd8b888, 0.55)
    },
    Entry {
        emission: 5.0,
        ..dielectric("emissive_white", "Emissive - White", 0xffffff, 0.5)
    },
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
                    rotation: 0.0,
                    projection: Projection::Box,
                    data: None,
                    embedded: false,
                    image_sha256: None,
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
