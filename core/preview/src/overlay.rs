// SPDX-License-Identifier: MIT
//! `model_render`'s overlay (mitcad#1488): what an analysis finds, drawn
//! over the bodies in one flat color, so that an agent sees where a print
//! needs support, where a wall is thin, where a face lacks draft, or what a
//! failed feature left as it was.
//!
//! This module reads the `overlay` argument and holds the triangles to
//! draw ([`OverlayMesh`]); the model works them out with Print Analysis'
//! functions (`mitcad_model::preview`), and the rasterizer draws them
//! (`raster`). The Print Analysis defaults (the overhang angle, the
//! minimum wall) are the model's: left out here, they are `None`.

use mitcad_vec3::Vec3;
use serde_json::{Map, Value};

/// The overlays' colors (sRGB): the 3D view's Print Analysis colors of
/// the light theme for overhangs and thin walls, amber for faces under the
/// draft, red for what a failed feature left.
pub const OVERHANG_COLOR: [u8; 3] = [0x7e, 0x3f, 0xd0];
pub const THIN_COLOR: [u8; 3] = [0xc2, 0x18, 0x8b];
pub const DRAFT_COLOR: [u8; 3] = [0xd9, 0x8a, 0x00];
pub const FAILED_COLOR: [u8; 3] = [0xd3, 0x2f, 0x2f];

/// The smallest draft (degrees) a face needs when `angle` is left out.
pub const DEFAULT_DRAFT_ANGLE: f64 = 1.0;

/// What is overlaid, with its settings.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayKind {
    /// The regions that need support: `angle` degrees from the vertical
    /// (Print Analysis' default when None), printed with `up` upward (else
    /// on the body's print bed face, else with +Z up).
    Overhang {
        angle: Option<f64>,
        up: Option<Vec3>,
    },
    /// Walls thinner than `minimum` mm (Print Analysis' default when None).
    Thickness { minimum: Option<f64> },
    /// Faces with less draft than `angle` degrees for a mold pulled
    /// along `pull` (either way).
    Draft { pull: Vec3, angle: f64 },
    /// The bodies failed features read, as the failures left them.
    Failed,
}

/// The `overlay` argument.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    pub kind: OverlayKind,
    /// sRGB.
    pub color: [u8; 3],
}

/// Triangles drawn over the bodies in one color.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OverlayMesh {
    /// Each triangle's corners in the design's coordinates.
    pub triangles: Vec<[Vec3; 3]>,
    /// sRGB.
    pub color: [u8; 3],
    /// How far (mm) the triangles are drawn off the surface along their
    /// normals, to lie in front of the body's own faces: the analysis'
    /// mesh and the picture's differ on curved faces by their deviations.
    pub lift: f64,
}

impl Overlay {
    /// The overlay a tool call asks for: a name (`"overhang"`,
    /// `"thickness"`, `"draft"`, `"failed"`) or an object with `type` and
    /// that type's settings, and `color` (`#rrggbb`) for any.
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let empty = Map::new();
        let (kind, fields) = match value {
            Value::String(name) => (name.trim(), &empty),
            Value::Object(fields) => (
                fields
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or("overlay.type must be overhang, thickness, draft or failed")?
                    .trim(),
                fields,
            ),
            _ => {
                return Err(
                    "overlay must be overhang, thickness, draft or failed, or an object with its type"
                        .to_owned(),
                );
            }
        };
        let allowed: &[&str] = match kind {
            "overhang" => &["type", "color", "angle", "up"],
            "thickness" => &["type", "color", "minimum"],
            "draft" => &["type", "color", "pull", "angle"],
            "failed" => &["type", "color"],
            other => {
                return Err(format!(
                    "overlay {other} is none of overhang, thickness, draft or failed"
                ));
            }
        };
        if let Some(field) = fields.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(format!(
                "overlay {kind} has no field {field} (it takes {})",
                allowed[1..].join(", ")
            ));
        }
        let number = |key: &str| -> Result<Option<f64>, String> {
            match fields.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(v) => v
                    .as_f64()
                    .filter(|n| n.is_finite())
                    .map(Some)
                    .ok_or_else(|| format!("overlay.{key} must be a number")),
            }
        };
        let direction = |key: &str| -> Result<Option<Vec3>, String> {
            match fields.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(v) => direction_of(v)
                    .map(Some)
                    .ok_or_else(|| format!("overlay.{key} must be a nonzero direction [x, y, z]")),
            }
        };
        let (kind, color) = match kind {
            "overhang" => (
                OverlayKind::Overhang {
                    angle: number("angle")?,
                    up: direction("up")?,
                },
                OVERHANG_COLOR,
            ),
            "thickness" => (
                OverlayKind::Thickness {
                    minimum: number("minimum")?,
                },
                THIN_COLOR,
            ),
            "draft" => {
                let angle = number("angle")?.unwrap_or(DEFAULT_DRAFT_ANGLE);
                if !(0.0..90.0).contains(&angle) {
                    return Err(format!(
                        "overlay.angle of draft must be from 0 to less than 90 degrees, got {angle}"
                    ));
                }
                (
                    OverlayKind::Draft {
                        pull: direction("pull")?.unwrap_or([0.0, 0.0, 1.0]),
                        angle,
                    },
                    DRAFT_COLOR,
                )
            }
            _ => (OverlayKind::Failed, FAILED_COLOR),
        };
        let color = match fields.get("color") {
            None | Some(Value::Null) => color,
            Some(v) => v
                .as_str()
                .and_then(hex_color)
                .ok_or("overlay.color must be a color #rrggbb")?,
        };
        Ok(Self { kind, color })
    }

    /// Its type's name, as the argument gives it.
    pub fn name(&self) -> &'static str {
        match self.kind {
            OverlayKind::Overhang { .. } => "overhang",
            OverlayKind::Thickness { .. } => "thickness",
            OverlayKind::Draft { .. } => "draft",
            OverlayKind::Failed => "failed",
        }
    }
}

/// A direction `[x, y, z]` of finite numbers, not zero.
fn direction_of(value: &Value) -> Option<Vec3> {
    let numbers: Vec<f64> = value
        .as_array()
        .filter(|a| a.len() == 3)?
        .iter()
        .filter_map(Value::as_f64)
        .filter(|n| n.is_finite())
        .collect();
    match numbers.as_slice() {
        [x, y, z] if *x != 0.0 || *y != 0.0 || *z != 0.0 => Some([*x, *y, *z]),
        _ => None,
    }
}

/// `#rrggbb` as sRGB bytes.
pub fn hex_color(text: &str) -> Option<[u8; 3]> {
    let text = text.trim();
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// sRGB bytes as `#rrggbb`.
pub fn color_text(color: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}
