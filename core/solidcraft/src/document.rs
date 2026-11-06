// SPDX-License-Identifier: MIT
//! The design document (SolidCraft's `crates/doc/src/document.rs`,
//! `assembly.rs`): what the import maps, typed; the rest kept as JSON.
//! Lengths are millimetres, angles radians, as in the file. Feature
//! geometry is in its component's coordinates.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// A 4×4 transform as SolidCraft stores it: `m[i]` for i < 3 the images
/// of the axes, `m[3]` the translation (a point p goes to
/// `m[0]·x + m[1]·y + m[2]·z + m[3]`).
pub type Mat = [[f64; 4]; 4];

pub const IDENTITY: Mat = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// A named parameter: user parameters, and the model parameters
/// (`d1`, …) of sketch dimensions.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Param {
    pub name: String,
    pub expr: String,
    /// `mm`, `deg` (or another unit), or empty: unit-less.
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub comment: String,
    /// Made by a dimension or a feature, not by the user.
    #[serde(default)]
    pub model: bool,
}

/// A timeline feature: the fields every kind has, and the kind's own JSON
/// (`def`, the whole object with `type`), typed by [`crate::kinds`].
#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub id: u64,
    pub name: String,
    pub suppressed: bool,
    /// Names of the bodies the feature makes, in order.
    pub body_names: Vec<String>,
    /// Parameter names of the feature's inputs ([`crate::inputs`]).
    pub param_names: Vec<String>,
    /// The component it belongs to (0: the root).
    pub component: u64,
    /// Persistent names of the picked edges (fillets, chamfers).
    pub edge_names: Vec<String>,
    /// Persistent names of the faces at the feature's face points.
    pub face_names: Vec<String>,
    /// Bodies a join, cut or intersection may change (`*`: every body).
    pub participants: Vec<String>,
    /// The pieces it splits a body into are named in the kernel's shell
    /// order (designs saved before format 14).
    pub shell_piece_order: bool,
    /// `sketch`, `extrude`, … (`FeatureKind`'s snake-case names).
    pub kind: String,
    pub def: Value,
}

impl<'de> Deserialize<'de> for Feature {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let def = Value::deserialize(d)?;
        let o = def
            .as_object()
            .ok_or_else(|| D::Error::custom("a feature is not an object"))?;
        let strings = |key: &str| -> Vec<String> {
            o.get(key)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|x| x.as_str().unwrap_or_default().to_owned())
                        .collect()
                })
                .unwrap_or_default()
        };
        let id = o
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| D::Error::custom("a feature without an id"))?;
        let kind = o
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| D::Error::custom(format!("feature {id} has no type")))?
            .to_owned();
        Ok(Feature {
            id,
            name: o
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            suppressed: o
                .get("suppressed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            body_names: strings("body_names"),
            param_names: strings("param_names"),
            component: o.get("component").and_then(Value::as_u64).unwrap_or(0),
            edge_names: strings("edge_names"),
            face_names: strings("face_names"),
            participants: strings("participants"),
            shell_piece_order: o
                .get("shell_piece_order")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            kind,
            def,
        })
    }
}

impl Feature {
    /// The kind's JSON as a typed definition.
    pub fn typed<T: serde::de::DeserializeOwned>(&self) -> Result<T, String> {
        serde_json::from_value(self.def.clone())
            .map_err(|e| format!("{} ({}): {e}", self.name, self.kind))
    }

    /// SolidCraft's display name of the kind (`Extrude`, `SurfaceLoft`, …).
    pub fn kind_title(&self) -> String {
        self.kind
            .split('_')
            .map(|w| {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                    .unwrap_or_default()
            })
            .collect()
    }
}

/// A component below the root (id 0, the design itself).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Component {
    pub id: u64,
    pub name: String,
    /// The parent component (0: the root).
    #[serde(default)]
    pub parent: u64,
}

/// A placement of a component in its parent.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Occurrence {
    pub id: u64,
    pub component: u64,
    #[serde(default)]
    pub parent: u64,
    pub name: String,
    #[serde(default = "identity")]
    pub transform: Mat,
    #[serde(default)]
    pub grounded: bool,
}

fn identity() -> Mat {
    IDENTITY
}

/// What was hidden when the design was saved.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Visibility {
    pub hidden_bodies: Vec<String>,
    pub hidden_occurrences: Vec<Vec<u64>>,
    pub hidden_origin: Vec<String>,
    pub hidden_sketches: Vec<u64>,
    pub shown_sketches: Vec<u64>,
}

/// A feature-history folder.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct TimelineGroup {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub parent: Option<u64>,
    #[serde(default)]
    pub features: Vec<u64>,
}

/// The document, upgraded to [`crate::PINNED`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Design {
    #[serde(default)]
    pub name: String,
    #[serde(default = "mm")]
    pub units: String,
    #[serde(default)]
    pub params: Vec<Param>,
    #[serde(default)]
    pub features: Vec<Feature>,
    /// Features at or after this index are rolled back.
    #[serde(default)]
    pub marker: Option<usize>,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default)]
    pub occurrences: Vec<Occurrence>,
    /// Joints, joint origins, motion links (SolidCraft's `joints.rs`).
    #[serde(default)]
    pub assembly: Value,
    /// Sheet metal rules (SolidCraft's `sheet.rs`).
    #[serde(default)]
    pub sheet: Value,
    /// Plastic rules.
    #[serde(default)]
    pub plastic: Value,
    /// Physical material per body name.
    #[serde(default)]
    pub materials: BTreeMap<String, String>,
    #[serde(default)]
    pub user_materials: Value,
    /// Appearances of bodies, faces and components.
    #[serde(default)]
    pub appearances: Value,
    /// Bodies moved into another component than their feature's.
    #[serde(default)]
    pub body_components: BTreeMap<String, u64>,
    #[serde(default)]
    pub body_offsets: BTreeMap<String, Mat>,
    /// Components linked to other design files.
    #[serde(default)]
    pub links: BTreeMap<u64, Value>,
    /// Components that are standard parts.
    #[serde(default)]
    pub parts: BTreeMap<u64, Value>,
    /// Configurations.
    #[serde(default)]
    pub configs: Value,
    /// Comments of feature-input parameters, by name.
    #[serde(default)]
    pub param_comments: BTreeMap<String, String>,
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub timeline_groups: Vec<TimelineGroup>,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default)]
    pub metadata: Value,
    /// Scanned points kept as a reference (format 43).
    #[serde(default)]
    pub point_clouds: Vec<Value>,
}

fn mm() -> String {
    "mm".to_owned()
}

impl Design {
    pub fn feature(&self, id: u64) -> Option<&Feature> {
        self.features.iter().find(|f| f.id == id)
    }

    pub fn component(&self, id: u64) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }

    /// Every component has an occurrence (SolidCraft's `normalise`): one at
    /// the identity in its parent for those without.
    pub fn add_missing_occurrences(&mut self) {
        let first = self
            .features
            .iter()
            .map(|f| f.id)
            .chain(self.components.iter().map(|c| c.id))
            .chain(self.occurrences.iter().map(|o| o.id))
            .max()
            .unwrap_or(0)
            + 1;
        let missing: Vec<(u64, u64, String)> = self
            .components
            .iter()
            .filter(|c| !self.occurrences.iter().any(|o| o.component == c.id))
            .map(|c| (c.id, c.parent, c.name.clone()))
            .collect();
        for (id, (component, parent, name)) in (first..).zip(missing) {
            self.occurrences.push(Occurrence {
                id,
                component,
                parent,
                name: format!("{name}:1"),
                transform: IDENTITY,
                grounded: false,
            });
        }
    }

    /// A list of the assembly (`joints`, `origins`, `links`,
    /// `contact_sets`, `studies`, `exploded`), as the file stores it.
    fn assembly_list(&self, key: &str) -> &[Value] {
        self.assembly
            .get(key)
            .and_then(Value::as_array)
            .map_or(&[], Vec::as_slice)
    }

    /// The joints between occurrences (SolidCraft's `joints.rs`, `Joint`).
    pub fn joints(&self) -> &[Value] {
        self.assembly_list("joints")
    }

    /// Named joint origins.
    pub fn joint_origins(&self) -> &[Value] {
        self.assembly_list("origins")
    }

    /// Motion links between joints (gears, racks, screws, cams).
    pub fn motion_links(&self) -> &[Value] {
        self.assembly_list("links")
    }

    /// Contact sets (SolidCraft's `motion.rs`, `ContactSet`).
    pub fn contact_sets(&self) -> &[Value] {
        self.assembly_list("contact_sets")
    }

    /// Whether contact sets take effect, and whether every occurrence is
    /// in contact with every other.
    pub fn contact(&self) -> (bool, bool) {
        let flag = |k: &str| {
            self.assembly
                .get(k)
                .and_then(Value::as_bool)
                .unwrap_or(false)
        };
        (flag("contact_enabled"), flag("contact_all"))
    }

    /// Motion studies (`motion.rs`, `MotionStudy`).
    pub fn motion_studies(&self) -> &[Value] {
        self.assembly_list("studies")
    }

    /// Exploded views (`motion.rs`, `ExplodedView`).
    pub fn exploded_views(&self) -> &[Value] {
        self.assembly_list("exploded")
    }

    /// The features in timeline order that are evaluated: before the
    /// marker.
    pub fn active_len(&self) -> usize {
        self.marker
            .map_or(self.features.len(), |m| m.min(self.features.len()))
    }

    /// The value of a parameter's or a feature input's name, as an
    /// expression: user and model parameters, then feature inputs.
    pub fn named_expression(&self, name: &str) -> Option<String> {
        if let Some(p) = self.params.iter().find(|p| p.name == name) {
            return Some(p.expr.clone());
        }
        for f in &self.features {
            if let Some(k) = f.param_names.iter().position(|n| n == name) {
                return crate::inputs::inputs(&f.def).get(k).map(|i| i.expr.clone());
            }
        }
        None
    }
}

/// `m` applied to a point.
pub fn apply_point(m: &Mat, p: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| m[0][i] * p[0] + m[1][i] * p[1] + m[2][i] * p[2] + m[3][i])
}

/// `m` applied to a direction.
pub fn apply_vector(m: &Mat, v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| m[0][i] * v[0] + m[1][i] * v[1] + m[2][i] * v[2])
}
