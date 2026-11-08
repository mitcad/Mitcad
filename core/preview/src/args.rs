// SPDX-License-Identifier: MIT
//! `model_render`'s arguments (mitcad#1486), read from the tool call's JSON
//! once for every quality and server. Fields this module does not know
//! (another quality's, e.g. mitcad#1489's `quality` and `samples`) are left
//! to their reader; the tool's schema refuses unknown ones before.

use serde_json::Value;

use crate::camera::{Aabb, Projection, View};
use crate::inspect::Inspect;
use crate::overlay::Overlay;

/// The picture's size when none is given, pixels.
pub const DEFAULT_SIZE: [u32; 2] = [1024, 768];
/// The smallest and largest side, pixels.
pub const MIN_SIDE: u32 = 16;
pub const MAX_SIDE: u32 = 4096;

/// What the camera frames.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Fit {
    /// Not given: the bodies drawn for a named view or angles; a saved or
    /// explicit camera stays as it is.
    #[default]
    Default,
    /// `"all"`: the bodies drawn.
    All,
    /// Body, component, occurrence or feature ids (or body names).
    Items(Vec<String>),
    /// A box in the design's coordinates.
    Box(Aabb),
}

/// Which bodies are drawn.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Selection {
    /// The bodies the design shows (its light bulbs and isolation).
    #[default]
    Visible,
    /// Every body, hidden ones too.
    All,
    /// Only these (shown or not): body, component, occurrence or feature
    /// ids, or body names.
    Items(Vec<String>),
}

/// The picture's background.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Background {
    /// The application's default light gradient.
    #[default]
    Theme,
    /// None: a PNG with an alpha channel.
    Transparent,
    /// One sRGB colour.
    Color([u8; 3]),
}

/// The arguments of `model_render` (without `document`).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderArgs {
    pub view: View,
    /// None: the explicit or saved view's own, else perspective.
    pub projection: Option<Projection>,
    pub fit: Fit,
    pub bodies: Selection,
    /// Left out of what `bodies` draws.
    pub hidden: Vec<String>,
    /// Width and height, pixels.
    pub size: [u32; 2],
    /// Face boundaries and silhouettes drawn.
    pub edges: bool,
    pub background: Background,
    /// Also written to this file (the server checks it).
    pub path: Option<String>,
    /// The PNG's largest size in bytes (the server's budget rules).
    pub max_bytes: Option<u64>,
    /// colors, highlight, section and explode (mitcad#1487).
    pub inspect: Inspect,
    /// What an analysis finds, drawn over the bodies (mitcad#1488).
    pub overlay: Option<Overlay>,
}

impl Default for RenderArgs {
    fn default() -> Self {
        Self {
            view: View::default(),
            projection: None,
            fit: Fit::Default,
            bodies: Selection::Visible,
            hidden: Vec::new(),
            size: DEFAULT_SIZE,
            edges: true,
            background: Background::Theme,
            path: None,
            max_bytes: None,
            inspect: Inspect::default(),
            overlay: None,
        }
    }
}

impl RenderArgs {
    /// The arguments of a tool call (an object; `document` and fields of
    /// other readers are ignored).
    pub fn from_json(arguments: &Value) -> Result<Self, String> {
        let mut args = Self::default();
        let Some(object) = arguments.as_object() else {
            return Err("model_render's arguments must be an object".to_owned());
        };
        if let Some(view) = object.get("view") {
            args.view = view_of(view)?;
        }
        if let Some(projection) = object.get("projection") {
            let text = projection
                .as_str()
                .ok_or("projection must be perspective or orthographic")?;
            args.projection = Some(Projection::parse(text)?);
        }
        if let Some(fit) = object.get("fit") {
            args.fit = match fit {
                Value::String(s) if s == "all" => Fit::All,
                Value::Array(_) => Fit::Items(ids(fit, "fit")?),
                Value::Object(o) => {
                    let min = point(o.get("min"), "fit.min")?;
                    let max = point(o.get("max"), "fit.max")?;
                    if (0..3).any(|i| min[i] > max[i]) {
                        return Err("fit's box must have min <= max on every axis".to_owned());
                    }
                    Fit::Box(Aabb { min, max })
                }
                _ => {
                    return Err("fit must be \"all\", a list of ids or a box {min, max}".to_owned());
                }
            };
        }
        if let Some(bodies) = object.get("bodies") {
            args.bodies = match bodies {
                Value::String(s) if s == "visible" => Selection::Visible,
                Value::String(s) if s == "all" => Selection::All,
                Value::Array(_) => Selection::Items(ids(bodies, "bodies")?),
                _ => {
                    return Err("bodies must be \"visible\", \"all\" or a list of ids".to_owned());
                }
            };
        }
        if let Some(hidden) = object.get("hidden") {
            args.hidden = ids(hidden, "hidden")?;
        }
        if let Some(size) = object.get("size") {
            let sides: Vec<u64> = size
                .as_array()
                .filter(|a| a.len() == 2)
                .map(|a| a.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default();
            if sides.len() != 2
                || sides
                    .iter()
                    .any(|s| !(u64::from(MIN_SIDE)..=u64::from(MAX_SIDE)).contains(s))
            {
                return Err(format!(
                    "size must be [width, height] in pixels, each {MIN_SIDE} to {MAX_SIDE}"
                ));
            }
            args.size = [sides[0] as u32, sides[1] as u32];
        }
        if let Some(edges) = object.get("edges") {
            args.edges = edges.as_bool().ok_or("edges must be true or false")?;
        }
        if let Some(background) = object.get("background") {
            args.background = background_of(background)?;
        }
        if let Some(path) = object.get("path") {
            args.path = Some(path.as_str().ok_or("path must be a text")?.to_owned());
        }
        if let Some(max_bytes) = object.get("max_bytes") {
            args.max_bytes = Some(max_bytes.as_u64().ok_or("max_bytes must be a count")?);
        }
        args.inspect = Inspect::from_json(object)?;
        if let Some(overlay) = object.get("overlay").filter(|o| !o.is_null()) {
            args.overlay = Some(Overlay::from_json(overlay)?);
        }
        Ok(args)
    }

    /// Width / height.
    pub fn aspect(&self) -> f64 {
        f64::from(self.size[0]) / f64::from(self.size[1])
    }
}

/// A view: a name, `{azimuth, elevation}` or `{eye, target, up}` (with
/// `height`, `fov` and `projection` optional: a camera `model_render`
/// answered is taken back as it is).
fn view_of(value: &Value) -> Result<View, String> {
    match value {
        Value::String(name) if !name.trim().is_empty() => Ok(View::Named(name.clone())),
        Value::Object(o) if o.contains_key("azimuth") || o.contains_key("elevation") => {
            let angle = |key: &str| {
                o.get(key)
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("view.{key} must be a number of degrees"))
            };
            Ok(View::Angles {
                azimuth: angle("azimuth")?,
                elevation: angle("elevation")?,
            })
        }
        Value::Object(o) => {
            let number = |key: &str| -> Result<Option<f64>, String> {
                match o.get(key) {
                    None | Some(Value::Null) => Ok(None),
                    Some(v) => v
                        .as_f64()
                        .map(Some)
                        .ok_or_else(|| format!("view.{key} must be a number")),
                }
            };
            let projection = match o.get("projection") {
                None | Some(Value::Null) => None,
                Some(p) => Some(Projection::parse(
                    p.as_str().ok_or("view.projection must be a text")?,
                )?),
            };
            Ok(View::Explicit {
                eye: point(o.get("eye"), "view.eye")?,
                target: point(o.get("target"), "view.target")?,
                up: match o.get("up") {
                    None => [0.0, 0.0, 1.0],
                    up => point(up, "view.up")?,
                },
                height: number("height")?,
                fov: number("fov")?,
                projection,
            })
        }
        _ => Err(
            "view must be a name (home, iso, front, back, left, right, top, bottom or a saved view), {azimuth, elevation} or {eye, target, up}"
                .to_owned(),
        ),
    }
}

fn point(value: Option<&Value>, what: &str) -> Result<[f64; 3], String> {
    let numbers: Vec<f64> = value
        .and_then(Value::as_array)
        .filter(|a| a.len() == 3)
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    match numbers.as_slice() {
        [x, y, z] if [x, y, z].iter().all(|c| c.is_finite()) => Ok([*x, *y, *z]),
        _ => Err(format!("{what} must be a point [x, y, z] in millimeters")),
    }
}

fn ids(value: &Value, what: &str) -> Result<Vec<String>, String> {
    let items = value
        .as_array()
        .ok_or_else(|| format!("{what} must be a list of ids"))?;
    if items.is_empty() {
        return Err(format!("{what} must not be an empty list"));
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_owned())
                .ok_or_else(|| format!("{what} must hold ids as texts"))
        })
        .collect()
}

fn background_of(value: &Value) -> Result<Background, String> {
    let text = value
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let hex = |s: &str| u8::from_str_radix(s, 16).ok();
    match text.as_str() {
        "theme" => Ok(Background::Theme),
        "transparent" => Ok(Background::Transparent),
        "white" => Ok(Background::Color([255, 255, 255])),
        "black" => Ok(Background::Color([0, 0, 0])),
        t if t.len() == 7 && t.is_ascii() && t.starts_with('#') => {
            match (hex(&t[1..3]), hex(&t[3..5]), hex(&t[5..7])) {
                (Some(r), Some(g), Some(b)) => Ok(Background::Color([r, g, b])),
                _ => Err(format!("background {t} is no colour #rrggbb")),
            }
        }
        _ => Err(
            "background must be \"theme\", \"transparent\", \"white\", \"black\" or \"#rrggbb\""
                .to_owned(),
        ),
    }
}
