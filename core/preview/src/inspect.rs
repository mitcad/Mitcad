// SPDX-License-Identifier: MIT
//! `model_render`'s inspection options (mitcad#1487), read from the tool
//! call's JSON beside [`crate::args`]: per-body colours (`colors`), faces,
//! edges and bodies drawn in an accent colour (`highlight`), a clip plane
//! with the cut faces capped (`section`) and bodies moved apart from the
//! design's centre (`explode`). What the references name is the model's to
//! find (`mitcad_model::preview`): a reference that names nothing is a
//! warning there, not a refusal here.

use mitcad_vec3::{Vec3, norm, unit};
use serde_json::{Map, Value};

/// The largest `explode` factor.
pub const MAX_EXPLODE: f64 = 10.0;

/// What a highlight names: the kind given with an object, or a text the
/// model reads (an id, a face or edge name, `body/face`).
#[derive(Debug, Clone, PartialEq)]
pub enum Highlight {
    /// A text: a body, feature, component or occurrence id or a body's
    /// name; a face name (`F2:side(c1[c4,c2])`) or an edge name
    /// (`E{…|…}`) in any body drawn; or `<body>/<face or edge name>`.
    Text(String),
    /// `{"body", "face"}`, `{"body", "edge"}` (optional `occurrence`), or
    /// an entity as `ui_state` lists it (`{"kind", "owner", "name"}`).
    Entity {
        kind: EntityKind,
        /// The body (an id or a name); None: every body drawn.
        body: Option<String>,
        /// An occurrence path the placements must be under.
        occurrence: Option<String>,
        /// The face's or edge's name (empty for a body).
        name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    Body,
    Face,
    Edge,
}

/// A clip plane: what lies on the side its normal points to is cut away
/// (as a Section Analysis cuts).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    pub origin: Vec3,
    /// A unit vector.
    pub normal: Vec3,
}

/// The inspection options of a `model_render` call; the default draws the
/// design as it is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Inspect {
    /// Ids as `bodies` takes them, each with an sRGB colour, in the order
    /// the call gave them.
    pub colors: Vec<(String, [u8; 3])>,
    pub highlight: Vec<Highlight>,
    pub section: Option<Section>,
    /// 0: no explode.
    pub explode: f64,
}

impl Inspect {
    /// The options of a tool call's arguments (fields it does not know are
    /// left to their readers).
    pub fn from_json(object: &Map<String, Value>) -> Result<Self, String> {
        let mut inspect = Self::default();
        if let Some(colors) = object.get("colors") {
            let map = colors.as_object().ok_or(
                "colors must be an object of ids and colors, e.g. {\"F2.b0\": \"#a52a2a\"}",
            )?;
            for (id, color) in map {
                let id = id.trim();
                if id.is_empty() {
                    return Err("colors must not have an empty id".to_owned());
                }
                let rgb = color
                    .as_str()
                    .and_then(hex_color)
                    .ok_or_else(|| format!("colors.{id} must be a color \"#rrggbb\""))?;
                inspect.colors.push((id.to_owned(), rgb));
            }
        }
        if let Some(highlight) = object.get("highlight") {
            let items = highlight
                .as_array()
                .ok_or("highlight must be a list of references")?;
            if items.is_empty() {
                return Err("highlight must not be an empty list".to_owned());
            }
            inspect.highlight = items.iter().map(highlight_of).collect::<Result<_, _>>()?;
        }
        if let Some(section) = object.get("section") {
            inspect.section = Some(section_of(section)?);
        }
        if let Some(explode) = object.get("explode") {
            let factor = explode
                .as_f64()
                .filter(|f| f.is_finite() && (0.0..=MAX_EXPLODE).contains(f))
                .ok_or_else(|| format!("explode must be a factor from 0 to {MAX_EXPLODE}"))?;
            inspect.explode = factor;
        }
        Ok(inspect)
    }

    /// The options given, by their argument names (those a quality without
    /// them refuses).
    pub fn given(&self) -> Vec<&'static str> {
        let mut given = Vec::new();
        if !self.colors.is_empty() {
            given.push("colors");
        }
        if !self.highlight.is_empty() {
            given.push("highlight");
        }
        if self.section.is_some() {
            given.push("section");
        }
        if self.explode != 0.0 {
            given.push("explode");
        }
        given
    }
}

/// `#rrggbb` as sRGB bytes.
pub fn hex_color(text: &str) -> Option<[u8; 3]> {
    let t = text.trim();
    if t.len() != 7 || !t.is_ascii() || !t.starts_with('#') {
        return None;
    }
    let hex = |s: &str| u8::from_str_radix(s, 16).ok();
    Some([hex(&t[1..3])?, hex(&t[3..5])?, hex(&t[5..7])?])
}

fn text_field(object: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(Some(s.trim().to_owned())),
        Some(_) => Err(format!("highlight's {key} must be a text")),
    }
}

fn highlight_of(value: &Value) -> Result<Highlight, String> {
    const FORM: &str = "highlight takes texts (an id, a body's name, a face or edge name, or body/face), {\"body\", \"face\"}, {\"body\", \"edge\"} or {\"kind\", \"owner\", \"name\"}";
    match value {
        Value::String(text) if !text.trim().is_empty() => {
            Ok(Highlight::Text(text.trim().to_owned()))
        }
        Value::Object(o) => {
            let occurrence = text_field(o, "occurrence")?;
            if let Some(kind) = text_field(o, "kind")? {
                let kind = match kind.as_str() {
                    "body" => EntityKind::Body,
                    "face" => EntityKind::Face,
                    "edge" => EntityKind::Edge,
                    other => {
                        return Err(format!(
                            "highlight's kind must be body, face or edge, not {other}"
                        ));
                    }
                };
                let owner = text_field(o, "owner")?;
                let name = text_field(o, "name")?;
                return match (kind, owner, name) {
                    (EntityKind::Body, Some(owner), _) | (EntityKind::Body, None, Some(owner)) => {
                        Ok(Highlight::Entity {
                            kind,
                            body: Some(owner),
                            occurrence,
                            name: String::new(),
                        })
                    }
                    (_, owner, Some(name)) => Ok(Highlight::Entity {
                        kind,
                        body: owner,
                        occurrence,
                        name,
                    }),
                    _ => Err(format!("{FORM}: the entity needs its owner and name")),
                };
            }
            let body = text_field(o, "body")?;
            let face = text_field(o, "face")?;
            let edge = text_field(o, "edge")?;
            match (body, face, edge) {
                (body, Some(face), None) => Ok(Highlight::Entity {
                    kind: EntityKind::Face,
                    body,
                    occurrence,
                    name: face,
                }),
                (body, None, Some(edge)) => Ok(Highlight::Entity {
                    kind: EntityKind::Edge,
                    body,
                    occurrence,
                    name: edge,
                }),
                (Some(body), None, None) => Ok(Highlight::Entity {
                    kind: EntityKind::Body,
                    body: Some(body),
                    occurrence,
                    name: String::new(),
                }),
                _ => Err(FORM.to_owned()),
            }
        }
        _ => Err(FORM.to_owned()),
    }
}

fn section_of(value: &Value) -> Result<Section, String> {
    const FORM: &str = "section must be a plane {\"origin\": [x, y, z], \"normal\": [x, y, z]} (mm; the side the normal points to is cut away)";
    let object = value.as_object().ok_or(FORM)?;
    let point = |key: &str| -> Result<Vec3, String> {
        let numbers: Vec<f64> = object
            .get(key)
            .and_then(Value::as_array)
            .filter(|a| a.len() == 3)
            .map(|a| a.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default();
        match numbers.as_slice() {
            [x, y, z] if [x, y, z].iter().all(|c| c.is_finite()) => Ok([*x, *y, *z]),
            _ => Err(FORM.to_owned()),
        }
    };
    if object.keys().any(|k| k != "origin" && k != "normal") {
        return Err(FORM.to_owned());
    }
    let origin = point("origin")?;
    let normal = point("normal")?;
    if norm(normal) < 1e-12 {
        return Err("section's normal must not be zero".to_owned());
    }
    Ok(Section {
        origin,
        normal: unit(normal).ok_or(FORM)?,
    })
}
