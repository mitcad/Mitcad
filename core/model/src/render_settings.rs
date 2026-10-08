// SPDX-License-Identifier: MIT
//! Render settings (mitcad#47): how the rendered view (`docs/rendering.md`)
//! lights and shows a design, kept per document so that a design keeps its
//! look. The settings are sections, each of fields with defaults:
//!
//! - `environment`: the light around the bodies, a built-in preset (three
//!   studios and an outdoor sky with a sun) or an HDR image, with its
//!   strength and rotation.
//! - `background`: what is behind the bodies: the view's own background, a
//!   solid colour, or the environment itself.
//! - `ground`: a ground under the bodies that catches their shadows and,
//!   when asked, their reflections, at the lowest body or a given height.
//! - `film`: exposure and the view transform (tone map) from the render's
//!   light to the screen's colours.
//! - `output` (mitcad#48): the final render to an image file: its size,
//!   samples or a time limit, denoising, a transparent background and the
//!   file format.
//! - `lights` (mitcad#54): lights of the user's own besides the
//!   environment's: point, spot, area and sun lights, placed in the design
//!   or relative to the camera. A list, changed by commands of its own
//!   (`add_render_light`, `edit_render_light`, `delete_render_light`).
//!
//! More sections are added as fields of [`RenderSettings`] and
//! [`RenderSettingsChange`] with defaults, so files without them open.
//! The commands are in `document/render_settings.rs`.

use std::f64::consts::{FRAC_PI_2, PI};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::appearance::Color;

/// Everything about rendering a document keeps.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RenderSettings {
    pub environment: Environment,
    pub background: Background,
    pub ground: Ground,
    pub film: Film,
    pub output: Output,
    pub lights: Vec<Light>,
}

/// The light around the bodies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Environment {
    pub preset: Preset,
    /// A factor on all of the environment's light (1: as designed).
    pub strength: f64,
    /// A turn of the studio's lights or of the image about the Z axis,
    /// counter-clockwise seen from above, in radians.
    pub rotation: f64,
    /// `outdoor`: the sun's height above the horizon, in radians (0 to
    /// pi/2).
    pub sun_elevation: f64,
    /// `outdoor`: where the sun is, counter-clockwise from the X axis seen
    /// from above, in radians (5/4 pi: front left, as the front view sees
    /// the design).
    pub sun_azimuth: f64,
    /// `image`: an equirectangular (latitude-longitude) HDR image, `.hdr`
    /// or `.exr`, relative to the project file's folder or absolute. Kept
    /// when another preset is chosen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// The environment's built-in setups, and the image.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// Soft key, fill and top lights in light grey surroundings.
    #[default]
    Studio,
    /// Bright white surroundings and very soft light (high key).
    StudioWhite,
    /// Dark surroundings with a key light and two rim lights.
    StudioDark,
    /// A clear sky and the sun (`sun_elevation`, `sun_azimuth`).
    Outdoor,
    /// An HDR image all around (`image`, `rotation`, `strength`).
    Image,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            preset: Preset::Studio,
            strength: 1.0,
            rotation: 0.0,
            sun_elevation: PI / 4.0,
            sun_azimuth: 1.25 * PI,
            image: None,
        }
    }
}

/// What the rendered view shows behind the bodies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Background {
    pub mode: BackgroundMode,
    /// `color`: sRGB, each component in [0, 1].
    pub color: Color,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundMode {
    /// The 3D view's own background (View > Environment).
    #[default]
    View,
    /// `color`.
    Color,
    /// The environment itself: its studio, sky or image.
    Environment,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            mode: BackgroundMode::View,
            color: [1.0, 1.0, 1.0],
        }
    }
}

/// The ground under the bodies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ground {
    /// A ground that shows only the bodies' shadows on it (a shadow
    /// catcher); without it there is no ground.
    pub shadows: bool,
    /// The ground is glossy and catches the bodies' reflections too (only
    /// with `shadows`).
    pub reflections: bool,
    /// Where the ground is on the Z axis, in mm; none: under the lowest
    /// body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

impl Default for Ground {
    fn default() -> Self {
        Self {
            shadows: true,
            reflections: false,
            height: None,
        }
    }
}

/// From the render's light to the screen's colours.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Film {
    /// Exposure in stops: each one doubles the light (0: as rendered).
    pub exposure: f64,
    pub view_transform: ViewTransform,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewTransform {
    /// sRGB of the light as it is; brighter than white clips.
    #[default]
    Standard,
    /// A filmic curve: highlights roll off softly and darks get contrast.
    Filmic,
    /// Colours as their base colours under white light, only highlights
    /// compressed (for product colours).
    Neutral,
}

/// The final render to an image file (mitcad#48, File > Render Image and
/// `mitcad-cli render`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Output {
    /// The image's width in pixels.
    pub width: u32,
    /// The image's height in pixels (with `aspect` `fixed`).
    pub height: u32,
    pub aspect: Aspect,
    /// Samples per pixel.
    pub samples: u32,
    /// Stop after this many seconds even before all samples are done (0:
    /// no limit).
    pub time_limit: f64,
    /// Denoise the image once its samples are done.
    pub denoise: bool,
    /// A transparent background (an alpha channel; not for JPEG): only the
    /// bodies and their shadows on the ground are in the image.
    pub transparent: bool,
    pub format: OutputFormat,
    /// JPEG's quality, 1 to 100.
    pub quality: u32,
}

/// How the image's height follows from its width.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    /// The view's aspect: the image is `width` wide and as high as the view
    /// is in proportion.
    #[default]
    View,
    /// `width` x `height`; the view shows the image's frame.
    Fixed,
}

/// The image file's format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// PNG with 8 bits per channel, as the view shows the render (exposure
    /// and view transform applied, sRGB).
    #[default]
    Png,
    /// PNG with 16 bits per channel, the same colours finer.
    Png16,
    /// JPEG, as the view shows the render; no transparency.
    Jpeg,
    /// OpenEXR with half floats: the render's linear light as it is
    /// (without exposure and view transform), alpha premultiplied.
    Exr,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            aspect: Aspect::View,
            samples: 128,
            time_limit: 0.0,
            denoise: true,
            transparent: false,
            format: OutputFormat::Png,
            quality: 90,
        }
    }
}

/// A light of the user's own (mitcad#54): where it is, where it shines,
/// its colour, power and size. Lengths in mm, angles in radians, Z up.
/// Every kind keeps every field; those of other kinds are ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Light {
    /// The id commands name it by (`light1`).
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: LightKind,
    /// Off: kept but not lighting.
    pub enabled: bool,
    /// Where `position` and `direction` are given: in the design, or
    /// relative to the camera (x to the right, y up, z toward the viewer,
    /// from the camera's eye), so that the light moves with the view.
    pub space: LightSpace,
    /// Where the light is (not for `sun`).
    pub position: [f64; 3],
    /// Where it shines (spot, area, sun; a unit vector): a sun's light
    /// travels along it.
    pub direction: [f64; 3],
    /// sRGB, each component in [0, 1].
    pub color: Color,
    /// Point, spot and area: the radiant power in watts (a 5 W point light
    /// 300 mm away gives 4.4 W/m² where it falls straight, a little more
    /// than the default studio's key light, 2.6). Sun: its irradiance in
    /// W/m² where it falls straight.
    pub power: f64,
    /// Point and spot: the diameter of the light's ball (0: a point, hard
    /// shadows); area: the rectangle's width or the disc's diameter.
    pub size: f64,
    /// Area: the rectangle's height.
    pub size_y: f64,
    /// Area: a rectangle or a disc.
    pub shape: LightShape,
    /// Spot: the cone's full angle.
    pub spot_angle: f64,
    /// Spot: how softly the cone's edge fades, 0 (sharp) to 1.
    pub spot_blend: f64,
    /// Sun: the angular diameter of the sun's disc (0: sharp shadows).
    pub angle: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightKind {
    /// Shines all around from a point (or a small ball).
    #[default]
    Point,
    /// A point light in a cone along `direction`.
    Spot,
    /// A glowing rectangle or disc facing `direction` (soft light).
    Area,
    /// Parallel light from far away along `direction` (no position).
    Sun,
}

impl LightKind {
    /// The kind's power when none is given: watts, or W/m² for a sun.
    pub fn default_power(self) -> f64 {
        match self {
            LightKind::Sun => 2.0,
            _ => 5.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LightKind::Point => "point",
            LightKind::Spot => "spot",
            LightKind::Area => "area",
            LightKind::Sun => "sun",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightSpace {
    /// In the design's coordinates.
    #[default]
    World,
    /// Relative to the camera: it follows the view.
    Camera,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightShape {
    #[default]
    Rectangle,
    Disc,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: LightKind::Point,
            enabled: true,
            space: LightSpace::World,
            position: [0.0, 0.0, 200.0],
            direction: [0.0, 0.0, -1.0],
            color: [1.0, 1.0, 1.0],
            power: LightKind::Point.default_power(),
            size: 20.0,
            size_y: 20.0,
            shape: LightShape::Rectangle,
            spot_angle: PI / 4.0,
            spot_blend: 0.15,
            angle: 0.02,
        }
    }
}

/// The most lights a document keeps.
pub const MAX_LIGHTS: usize = 64;
/// The largest power (W or W/m²) and size (mm) of a light.
pub const MAX_LIGHT_POWER: f64 = 1.0e6;
pub const MAX_LIGHT_SIZE: f64 = 1.0e6;
/// How far from the origin a light may be, in mm.
pub const MAX_LIGHT_DISTANCE: f64 = 1.0e7;

impl Light {
    /// Why the light cannot be kept, if it cannot (`at` names it).
    pub fn check(&self) -> Result<(), String> {
        let at = format!("lights.{}", self.id);
        if !crate::appearance::valid_id(&self.id) {
            return Err(format!(
                "'{}' is no light id: use letters, digits, '_' and '-'",
                self.id
            ));
        }
        if self.name.trim().is_empty() {
            return Err(format!("{at}.name is empty"));
        }
        if !self
            .position
            .iter()
            .all(|c| c.is_finite() && c.abs() <= MAX_LIGHT_DISTANCE)
        {
            return Err(format!(
                "{at}.position must be finite and within {MAX_LIGHT_DISTANCE} mm, got {:?}",
                self.position
            ));
        }
        let length = self.direction.iter().map(|c| c * c).sum::<f64>().sqrt();
        if !length.is_finite() || length < 1e-9 {
            return Err(format!(
                "{at}.direction must be a direction (not zero), got {:?}",
                self.direction
            ));
        }
        if !self.color.iter().all(|c| (0.0..=1.0).contains(c)) {
            return Err(format!(
                "{at}.color: colour components must be between 0 and 1, got {:?}",
                self.color
            ));
        }
        if !(0.0..=MAX_LIGHT_POWER).contains(&self.power) {
            return Err(format!(
                "{at}.power must be between 0 and {MAX_LIGHT_POWER}, got {}",
                self.power
            ));
        }
        for (field, value) in [("size", self.size), ("size_y", self.size_y)] {
            if !(0.0..=MAX_LIGHT_SIZE).contains(&value) {
                return Err(format!(
                    "{at}.{field} must be between 0 and {MAX_LIGHT_SIZE} mm, got {value}"
                ));
            }
        }
        if !(self.spot_angle > 0.0 && self.spot_angle <= PI) {
            return Err(format!(
                "{at}.spot_angle must be more than 0 and at most pi (180 deg), got {}",
                self.spot_angle
            ));
        }
        if !(0.0..=1.0).contains(&self.spot_blend) {
            return Err(format!(
                "{at}.spot_blend must be between 0 and 1, got {}",
                self.spot_blend
            ));
        }
        if !(0.0..=FRAC_PI_2).contains(&self.angle) {
            return Err(format!(
                "{at}.angle must be between 0 and pi/2 (90 deg), got {}",
                self.angle
            ));
        }
        Ok(())
    }

    /// This light with the fields `fields` gives (null: the field's
    /// default; `id` cannot change), its direction made a unit vector,
    /// checked.
    pub fn changed(&self, fields: &Map<String, Value>) -> Result<Self, String> {
        if let Some(id) = fields.get("id")
            && id.as_str() != Some(self.id.as_str())
        {
            return Err(format!("lights.{}: the id cannot change", self.id));
        }
        let mut next = self.clone();
        merge(
            &mut next,
            &Some(fields.clone()),
            &format!("lights.{}", self.id),
        )?;
        next.id = self.id.clone();
        let length = next.direction.iter().map(|c| c * c).sum::<f64>().sqrt();
        next.check()?;
        for c in &mut next.direction {
            *c /= length;
        }
        Ok(next)
    }

    /// How the comparison of versions names an added or removed light.
    pub fn summary(&self) -> String {
        format!("{} ({})", self.name, self.kind.name())
    }
}

/// The smallest and largest image side, in pixels.
pub const MIN_IMAGE_SIZE: u32 = 16;
pub const MAX_IMAGE_SIZE: u32 = 16384;
/// The most samples per pixel.
pub const MAX_SAMPLES: u32 = 65536;
/// The longest time limit, in seconds (a day).
pub const MAX_TIME_LIMIT: f64 = 86400.0;

/// The largest `strength`.
pub const MAX_STRENGTH: f64 = 100.0;
/// The largest exposure either way, in stops.
pub const MAX_EXPOSURE: f64 = 10.0;

impl RenderSettings {
    /// Whether these are the defaults (a project file leaves them out).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Why the settings cannot be kept, if they cannot.
    pub fn check(&self) -> Result<(), String> {
        let e = &self.environment;
        if !(0.0..=MAX_STRENGTH).contains(&e.strength) {
            return Err(format!(
                "environment.strength must be between 0 and {MAX_STRENGTH}, got {}",
                e.strength
            ));
        }
        for (field, value) in [("rotation", e.rotation), ("sun_azimuth", e.sun_azimuth)] {
            if !value.is_finite() || value.abs() > 4.0 * PI {
                return Err(format!(
                    "environment.{field} must be an angle within two turns either way, got {value}"
                ));
            }
        }
        if !(0.0..=FRAC_PI_2).contains(&e.sun_elevation) {
            return Err(format!(
                "environment.sun_elevation must be between 0 and pi/2 (90 deg), got {}",
                e.sun_elevation
            ));
        }
        if e.image
            .as_deref()
            .is_some_and(|path| path.trim().is_empty())
        {
            return Err("environment.image: the image path is empty".to_owned());
        }
        if !self
            .background
            .color
            .iter()
            .all(|c| (0.0..=1.0).contains(c))
        {
            return Err(format!(
                "background.color: colour components must be between 0 and 1, got {:?}",
                self.background.color
            ));
        }
        if self.ground.height.is_some_and(|h| !h.is_finite()) {
            return Err("ground.height must be a finite length".to_owned());
        }
        if !(-MAX_EXPOSURE..=MAX_EXPOSURE).contains(&self.film.exposure) {
            return Err(format!(
                "film.exposure must be between -{MAX_EXPOSURE} and {MAX_EXPOSURE} stops, got {}",
                self.film.exposure
            ));
        }
        let o = &self.output;
        for (field, value) in [("width", o.width), ("height", o.height)] {
            if !(MIN_IMAGE_SIZE..=MAX_IMAGE_SIZE).contains(&value) {
                return Err(format!(
                    "output.{field} must be between {MIN_IMAGE_SIZE} and {MAX_IMAGE_SIZE} pixels, got {value}"
                ));
            }
        }
        if !(1..=MAX_SAMPLES).contains(&o.samples) {
            return Err(format!(
                "output.samples must be between 1 and {MAX_SAMPLES}, got {}",
                o.samples
            ));
        }
        if !(0.0..=MAX_TIME_LIMIT).contains(&o.time_limit) {
            return Err(format!(
                "output.time_limit must be between 0 (none) and {MAX_TIME_LIMIT} seconds, got {}",
                o.time_limit
            ));
        }
        if !(1..=100).contains(&o.quality) {
            return Err(format!(
                "output.quality must be between 1 and 100, got {}",
                o.quality
            ));
        }
        if self.lights.len() > MAX_LIGHTS {
            return Err(format!(
                "a document keeps at most {MAX_LIGHTS} lights, got {}",
                self.lights.len()
            ));
        }
        for (i, light) in self.lights.iter().enumerate() {
            light.check()?;
            if self.lights[..i].iter().any(|other| other.id == light.id) {
                return Err(format!("there are two lights '{}'", light.id));
            }
        }
        Ok(())
    }

    pub fn light(&self, id: &str) -> Option<&Light> {
        self.lights.iter().find(|light| light.id == id)
    }

    /// These settings with the fields `change` gives (a field given as null
    /// goes back to its default: none for `image` and `height`), checked.
    pub fn changed(&self, change: &RenderSettingsChange) -> Result<Self, String> {
        let mut next = self.clone();
        merge(&mut next.environment, &change.environment, "environment")?;
        merge(&mut next.background, &change.background, "background")?;
        merge(&mut next.ground, &change.ground, "ground")?;
        merge(&mut next.film, &change.film, "film")?;
        merge(&mut next.output, &change.output, "output")?;
        next.check()?;
        Ok(next)
    }

    /// The fields that differ from `other`'s, by their path
    /// (`environment.preset`), with the values of `self` and `other`.
    pub fn differences(&self, other: &Self) -> Vec<(String, Value, Value)> {
        let mut out = Vec::new();
        let a = serde_json::to_value(self).expect("render settings serialize");
        let b = serde_json::to_value(other).expect("render settings serialize");
        for (section, fields) in a.as_object().into_iter().flatten() {
            if section == "lights" {
                continue; // by light, below
            }
            let empty = Map::new();
            let fields = fields.as_object().unwrap_or(&empty);
            let others = b[section].as_object().unwrap_or(&empty);
            let mut names: Vec<&String> = fields.keys().chain(others.keys()).collect();
            names.sort_unstable();
            names.dedup();
            for name in names {
                let (x, y) = (
                    fields.get(name).cloned().unwrap_or(Value::Null),
                    others.get(name).cloned().unwrap_or(Value::Null),
                );
                if x != y {
                    out.push((format!("{section}.{name}"), x, y));
                }
            }
        }
        // Lights by id: added, removed, or their fields.
        for light in &self.lights {
            let Some(theirs) = other.light(&light.id) else {
                out.push((
                    format!("lights.{}", light.id),
                    Value::String(light.summary()),
                    Value::Null,
                ));
                continue;
            };
            let x = serde_json::to_value(light).expect("lights serialize");
            let y = serde_json::to_value(theirs).expect("lights serialize");
            for (name, value) in x.as_object().into_iter().flatten() {
                if y.get(name) != Some(value) {
                    out.push((
                        format!("lights.{}.{name}", light.id),
                        value.clone(),
                        y.get(name).cloned().unwrap_or(Value::Null),
                    ));
                }
            }
        }
        for light in &other.lights {
            if self.light(&light.id).is_none() {
                out.push((
                    format!("lights.{}", light.id),
                    Value::Null,
                    Value::String(light.summary()),
                ));
            }
        }
        out
    }
}

/// The fields of `set_render_settings`: per section, the fields that
/// change (the others stay).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderSettingsChange {
    #[serde(default)]
    pub environment: Option<Map<String, Value>>,
    #[serde(default)]
    pub background: Option<Map<String, Value>>,
    #[serde(default)]
    pub ground: Option<Map<String, Value>>,
    #[serde(default)]
    pub film: Option<Map<String, Value>>,
    #[serde(default)]
    pub output: Option<Map<String, Value>>,
}

/// Sets the given fields of a section (null: the field's default, none
/// for an optional one) and reads it back, so that unknown fields and wrong
/// types are refused.
fn merge<T>(section: &mut T, fields: &Option<Map<String, Value>>, name: &str) -> Result<(), String>
where
    T: Serialize + for<'de> Deserialize<'de> + Default,
{
    let Some(fields) = fields else {
        return Ok(());
    };
    let defaults = serde_json::to_value(T::default()).expect("render settings serialize");
    let mut value = serde_json::to_value(&*section).expect("render settings serialize");
    let object = value.as_object_mut().expect("a section is an object");
    for (field, given) in fields {
        if !given.is_null() {
            object.insert(field.clone(), given.clone());
        } else if let Some(default) = defaults.get(field) {
            object.insert(field.clone(), default.clone());
        } else if OPTIONAL.contains(&(name, field.as_str())) {
            object.remove(field);
        } else {
            return Err(format!("{name}: unknown field `{field}`"));
        }
    }
    *section = serde_json::from_value(value).map_err(|e| format!("{name}: {e}"))?;
    Ok(())
}

/// The fields that can be left out (none), by section.
const OPTIONAL: [(&str, &str); 2] = [("environment", "image"), ("ground", "height")];

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn change(value: Value) -> RenderSettingsChange {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn the_defaults_are_valid_and_serialize_in_full() {
        let settings = RenderSettings::default();
        assert!(settings.check().is_ok());
        assert!(settings.is_default());
        let value = serde_json::to_value(&settings).unwrap();
        assert_eq!(value["environment"]["preset"], "studio");
        assert_eq!(value["background"]["mode"], "view");
        assert_eq!(value["ground"]["shadows"], true);
        assert_eq!(value["film"]["view_transform"], "standard");
        assert_eq!(value["output"]["format"], "png");
        assert_eq!(value["output"]["aspect"], "view");
        assert!(value["environment"].get("image").is_none());
        // Missing sections and fields are the defaults.
        let read: RenderSettings =
            serde_json::from_value(json!({"film": {"exposure": 1.5}})).unwrap();
        assert_eq!(read.film.exposure, 1.5);
        assert_eq!(read.environment, Environment::default());
    }

    #[test]
    fn a_change_sets_only_the_given_fields() {
        let settings = RenderSettings::default();
        let next = settings
            .changed(&change(json!({
                "environment": {"preset": "image", "image": "/env/sky.hdr", "strength": 2},
                "ground": {"height": -5}
            })))
            .unwrap();
        assert_eq!(next.environment.preset, Preset::Image);
        assert_eq!(next.environment.image.as_deref(), Some("/env/sky.hdr"));
        assert_eq!(next.environment.strength, 2.0);
        assert_eq!(next.environment.rotation, 0.0);
        assert_eq!(next.ground.height, Some(-5.0));
        assert!(next.ground.shadows);
        // Null: back to the default.
        let back = next
            .changed(&change(json!({
                "environment": {"image": null, "strength": null},
                "ground": {"height": null}
            })))
            .unwrap();
        assert_eq!(back.environment.image, None);
        assert_eq!(back.environment.strength, 1.0);
        assert_eq!(back.ground.height, None);
        assert_eq!(back.environment.preset, Preset::Image);
        let differences = settings.differences(&next);
        let paths: Vec<&str> = differences.iter().map(|(p, _, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            [
                "environment.image",
                "environment.preset",
                "environment.strength",
                "ground.height"
            ]
        );
    }

    #[test]
    fn wrong_values_and_unknown_fields_are_refused() {
        let settings = RenderSettings::default();
        for (given, message) in [
            (
                json!({"environment": {"preset": "moon"}}),
                "unknown variant",
            ),
            (json!({"environment": {"strength": -1}}), "strength"),
            (json!({"environment": {"strength": 101}}), "strength"),
            (
                json!({"environment": {"sun_elevation": 2}}),
                "sun_elevation",
            ),
            (json!({"environment": {"rotation": 100}}), "rotation"),
            (
                json!({"environment": {"image": " "}}),
                "image path is empty",
            ),
            (
                json!({"environment": {"colour": 1}}),
                "unknown field `colour`",
            ),
            (json!({"background": {"mode": "sky"}}), "unknown variant"),
            (
                json!({"background": {"color": [1.5, 0, 0]}}),
                "colour components",
            ),
            (
                json!({"ground": {"shadows": "yes"}}),
                "ground: invalid type",
            ),
            (json!({"film": {"exposure": 11}}), "exposure"),
            (
                json!({"film": {"view_transform": "agx"}}),
                "unknown variant",
            ),
            (json!({"output": {"width": 8}}), "output.width"),
            (json!({"output": {"height": 20000}}), "output.height"),
            (json!({"output": {"samples": 0}}), "output.samples"),
            (json!({"output": {"time_limit": -1}}), "output.time_limit"),
            (json!({"output": {"quality": 101}}), "output.quality"),
            (json!({"output": {"format": "tiff"}}), "unknown variant"),
            (json!({"output": {"aspect": "square"}}), "unknown variant"),
            (json!({"output": {"width": 1.5}}), "output: invalid type"),
        ] {
            let error = settings.changed(&change(given.clone())).unwrap_err();
            assert!(error.contains(message), "{given}: {error}");
        }
        assert!(
            serde_json::from_value::<RenderSettingsChange>(json!({"camera": {}})).is_err(),
            "unknown sections are refused"
        );
    }
}

#[cfg(test)]
mod output_tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_output_section_changes_alone_and_goes_back_to_its_defaults() {
        let settings = RenderSettings::default();
        let change: RenderSettingsChange = serde_json::from_value(json!({
            "output": {"width": 320, "height": 240, "aspect": "fixed", "samples": 16,
                       "time_limit": 2.5, "denoise": false, "transparent": true, "format": "png16"}
        }))
        .unwrap();
        let next = settings.changed(&change).unwrap();
        let o = &next.output;
        assert_eq!((o.width, o.height, o.aspect), (320, 240, Aspect::Fixed));
        assert_eq!((o.samples, o.time_limit, o.denoise), (16, 2.5, false));
        assert!(o.transparent);
        assert_eq!(o.format, OutputFormat::Png16);
        assert_eq!(o.quality, 90, "the others stay");
        assert_eq!(next.environment, settings.environment);
        let paths: Vec<String> = settings
            .differences(&next)
            .into_iter()
            .map(|(p, _, _)| p)
            .collect();
        assert!(paths.contains(&"output.format".to_owned()), "{paths:?}");
        let back: RenderSettingsChange =
            serde_json::from_value(json!({"output": {"width": null, "format": null}})).unwrap();
        let back = next.changed(&back).unwrap();
        assert_eq!(back.output.width, 1920);
        assert_eq!(back.output.format, OutputFormat::Png);
        assert_eq!(back.output.height, 240);
        // Files of mitcad#47 (without the section) read as the defaults.
        let old: RenderSettings =
            serde_json::from_value(json!({"film": {"exposure": 1.0}})).unwrap();
        assert_eq!(old.output, Output::default());
    }
}
