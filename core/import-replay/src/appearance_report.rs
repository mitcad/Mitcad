// SPDX-License-Identifier: MIT
//! The report of the file's appearances and physical materials
//! (mitcad#228): what `DesignReport::appearances` holds. The appearances
//! themselves come in in `mitcad-import-appearances`, a crate above the
//! core (mitcad#791); the report's types are here, where the design's
//! report is.

use std::collections::BTreeMap;

use mitcad_model::appearance::Color;
use serde::Serialize;

/// How the file's appearances and materials came in.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AppearanceReport {
    /// The file's bodies with an appearance, and the replay's bodies that
    /// got theirs.
    pub bodies: usize,
    pub bodies_set: usize,
    /// The file's faces with an appearance of their own (one per face
    /// GUID and body), and those that got it.
    pub faces: usize,
    pub faces_set: usize,
    /// The file's bodies with a physical material, those whose material is
    /// one of Mitcad's and was set.
    pub materials: usize,
    pub materials_set: usize,
    /// Occurrences whose appearance overrides their bodies', and those
    /// applied.
    pub overrides: usize,
    pub overrides_set: usize,
    /// Body records of bodies the stored design does not hold (a snapshot
    /// blob's copies of bodies the history holds, bodies taken by later
    /// operations), left out.
    pub not_in_design: usize,
    /// The file's bodies whose light bulb is off, and the replay's bodies
    /// hidden for them (mitcad#1175).
    #[serde(skip_serializing_if = "is_zero")]
    pub hidden: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub hidden_set: usize,
    /// The document appearances made.
    pub appearances: Vec<AppearanceEntry>,
    /// The file's physical materials.
    pub physical_materials: Vec<MaterialEntry>,
    /// What did not come in, and why.
    pub gaps: Vec<String>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

impl AppearanceReport {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A document appearance made from an asset.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AppearanceEntry {
    pub id: String,
    pub name: String,
    /// The asset's GUID, schema and library preset.
    pub asset: String,
    pub schema: String,
    pub preset: String,
    pub base_color: Color,
    /// What of the asset Mitcad's parameters do not hold.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture: Option<TextureEntry>,
    /// The images the asset's other maps come from (mitcad#313), by
    /// Mitcad's map name (`roughness`, `normal`, `bump`, `cutout`), as the
    /// asset names them: the file does not hold them either.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub maps: BTreeMap<String, String>,
}

/// A texture image an appearance's colour comes from.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TextureEntry {
    /// The image as the asset names it (a library resource; the image
    /// itself is not in the file).
    pub image: String,
    /// One repeat of the image, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<[f64; 2]>,
    /// Its offset, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<[f64; 2]>,
    /// Its rotation, radians.
    pub rotation: f64,
}

/// A physical material of the file.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MaterialEntry {
    pub asset: String,
    pub preset: String,
    /// The asset's description (`Steel, mill finish`).
    pub name: String,
    pub category: String,
    pub classification: String,
    /// kg/m³, where the file gives it in a known unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<f64>,
    /// Mitcad's material set for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mitcad: Option<String>,
    /// The file's bodies with it.
    pub bodies: usize,
}
