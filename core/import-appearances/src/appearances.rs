// SPDX-License-Identifier: MIT
//! The appearances and physical materials of an `.f3d` design's bodies,
//! faces and occurrences (mitcad#228), after the replay.
//!
//! The decoder (`mitcad_f3d_design::appearance`, `_f3d.appearance` of the
//! dump) names each body's appearance and material by the asset they are
//! in the document's packages, and the faces with appearances of their own
//! by their geometry in the stored design. Here:
//!
//! - each asset a body, face or occurrence uses becomes one document
//!   appearance ([`look`]: the asset's parameters on Mitcad's set), once
//!   per asset;
//! - a body gets its file body's appearance: the stored body the final
//!   comparison matched it with (`DesignReport::bodies`) is the one the
//!   design's body record names through the body map (blob and record);
//! - a face gets its own where the replay's body has faces through the
//!   stored face's point ([`mitcad_import_replay::refs::face_pieces`]);
//! - an occurrence's override goes to the bodies of its component when the
//!   component is placed once (Mitcad keeps appearances per body);
//! - a body gets Mitcad's material where the file's material is one of
//!   Mitcad's by name and density ([`mitcad_material`]); every material is
//!   listed in the report with its density.

use std::collections::{BTreeMap, HashMap};

use mitcad_dump::{AppearanceData, AssetDef, AssetRef, BodyLook};
use mitcad_model::appearance::{self, AppearanceChange, Color};
use mitcad_model::{BodyUid, ComponentUid, Kernel};
use serde_json::Value;

use mitcad_import_base::body_scope;
use mitcad_import_base::geom;
use mitcad_import_replay::Importer;
use mitcad_import_replay::internal::refs;
// The report's types, in the core's `appearance_report` with the design's
// report (mitcad#791); their paths here are kept.
pub use mitcad_import_replay::appearance_report::{
    AppearanceEntry, AppearanceReport, MaterialEntry, TextureEntry,
};

/// An asset's parameters on Mitcad's set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Look {
    pub name: String,
    pub change: AppearanceChange,
    pub notes: Vec<String>,
    pub texture: Option<TextureEntry>,
    /// The other maps' images (see [`AppearanceEntry::maps`]).
    pub maps: BTreeMap<String, String>,
}

fn props(a: &AssetDef) -> &BTreeMap<String, Value> {
    static EMPTY: std::sync::OnceLock<BTreeMap<String, Value>> = std::sync::OnceLock::new();
    a.properties
        .as_ref()
        .unwrap_or_else(|| EMPTY.get_or_init(BTreeMap::new))
}

fn color(a: &AssetDef, id: &str) -> Option<[f64; 4]> {
    let v = props(a).get(id)?.as_array()?;
    let c: Vec<f64> = v.iter().filter_map(Value::as_f64).collect();
    (c.len() == 4).then(|| [c[0], c[1], c[2], c[3]])
}

fn number(a: &AssetDef, id: &str) -> Option<f64> {
    let v = props(a).get(id)?;
    v.as_f64().or_else(|| v.get("value")?.as_f64())
}

fn boolean(a: &AssetDef, id: &str) -> Option<bool> {
    props(a).get(id)?.as_bool()
}

fn connected<'a>(a: &'a AssetDef, id: &str) -> &'a [String] {
    a.connections
        .as_ref()
        .and_then(|c| c.get(id))
        .map_or(&[], Vec::as_slice)
}

/// A length in mm from a quantity property (centimetre, millimetre, inch).
fn millimetres(a: &AssetDef, id: &str) -> Option<f64> {
    let v = props(a).get(id)?;
    let unit = v.get("unit")?.as_u64()?;
    let value = v.get("value")?.as_f64()?;
    mitcad_f3d_file::protein::length_mm(u32::try_from(unit).ok()?, value)
}

/// sRGB from the stored colour: as stored unless its colour space is
/// linear (choice 2).
fn srgb(a: &AssetDef, id: &str, c: [f64; 4]) -> Color {
    let linear = number(a, &format!("{id}_colorspace")) == Some(2.0);
    let encode = |x: f64| {
        let x = x.clamp(0.0, 1.0);
        if !linear {
            x
        } else if x <= 0.003_130_8 {
            12.92 * x
        } else {
            1.055 * x.powf(1.0 / 2.4) - 0.055
        }
    };
    [encode(c[0]), encode(c[1]), encode(c[2])]
}

/// A name that is a library preset (`Prism-093`, `PrismMaterial-018`,
/// `ThermalSolid-092-2`) rather than one a person gave.
fn is_preset(s: &str) -> bool {
    !s.contains(' ')
        && s.rsplit_once('-').is_some_and(|(a, n)| {
            !a.is_empty() && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())
        })
}

/// The name an appearance of the file goes by: the one a person gave it,
/// else its description, else the asset's name.
pub fn display_name(a: &AssetDef) -> String {
    let name = a.name.clone().unwrap_or_default();
    let nonempty = |s: &Option<String>| s.clone().filter(|s| !s.trim().is_empty());
    if let Some(ui) = nonempty(&a.ui_name)
        && ui != name
        && !is_preset(&ui)
        && !name.ends_with(&format!(":{ui}"))
    {
        return ui;
    }
    nonempty(&a.description)
        .or_else(|| (!name.is_empty()).then_some(name))
        .or_else(|| a.id.clone())
        .unwrap_or_default()
}

/// The property that holds an appearance's base colour (by its schema's
/// shader role) and its value.
fn base_color(a: &AssetDef) -> Option<(&'static str, [f64; 4])> {
    if boolean(a, "common_Tint_toggle") == Some(true) {
        return color(a, "common_Tint_color").map(|c| ("common_Tint_color", c));
    }
    let id = match a.schema.as_deref()? {
        "GenericSchema" => "generic_diffuse",
        "MetalSchema" => "metal_color",
        "MetallicPaintSchema" => "metallicpaint_base_color",
        "PlasticVinylSchema" => "plasticvinyl_color",
        "CeramicSchema" => "ceramic_color",
        "GlazingSchema" => "glazing_transmittance_map",
        "PrismOpaqueSchema" => "opaque_albedo",
        "PrismMetalSchema" => "metal_f0",
        "PrismTransparentSchema" => "transparent_color",
        "PrismLayeredSchema" => "layered_diffuse",
        s if s.starts_with("Prism") => "surface_albedo",
        _ => return None,
    };
    color(a, id).map(|c| (id, c))
}

/// The texture a property is connected to.
fn texture(assets: &[AssetDef], a: &AssetDef, id: &str) -> Option<TextureEntry> {
    let t = connected(a, id)
        .iter()
        .find_map(|g| assets.iter().find(|x| x.id.as_deref() == Some(g)))?;
    let image = props(t)
        .iter()
        .filter(|(k, _)| k.ends_with("_Bitmap"))
        .find_map(|(_, v)| {
            v.get("paths")?
                .as_array()?
                .first()?
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let pair = |x: &str, y: &str| Some([millimetres(t, x)?, millimetres(t, y)?]);
    Some(TextureEntry {
        image,
        size: pair("texture_RealWorldScaleX", "texture_RealWorldScaleY"),
        offset: pair("texture_RealWorldOffsetX", "texture_RealWorldOffsetY"),
        rotation: number(t, "texture_WAngle").unwrap_or(0.0).to_radians(),
    })
}

/// An appearance asset's parameters on Mitcad's set (`assets`: the other
/// assets, for its textures).
pub fn look(a: &AssetDef, assets: &[AssetDef]) -> Look {
    let mut c = AppearanceChange::default();
    let mut notes = Vec::new();
    let schema = a.schema.clone().unwrap_or_default();
    let mut tex = None;
    match base_color(a) {
        Some((id, rgba)) => {
            c.base_color = Some(srgb(a, id, rgba));
            if !connected(a, id).is_empty() {
                tex = texture(assets, a, id);
                notes.push(format!(
                    "the color is a texture image ({}), which the file does not hold: the \
                     stored color is used",
                    tex.as_ref().map_or("", |t| t.image.as_str())
                ));
            }
        }
        None => notes.push(format!("no base color read for {schema}")),
    }
    let unit = |x: f64| x.clamp(0.0, 1.0);
    match schema.as_str() {
        "PrismOpaqueSchema"
        | "PrismMetalSchema"
        | "PrismTransparentSchema"
        | "PrismLayeredSchema" => {
            c.roughness = number(a, "surface_roughness").map(unit);
            if !connected(a, "surface_roughness").is_empty() {
                notes.push("the roughness is a texture image: the stored value is used".into());
            }
        }
        "GenericSchema" => {
            c.roughness = number(a, "generic_glossiness").map(|g| unit(1.0 - g));
        }
        _ => {}
    }
    match schema.as_str() {
        "PrismMetalSchema" | "MetalSchema" => c.metalness = Some(1.0),
        "GenericSchema" => {
            c.metalness = boolean(a, "generic_is_metal").map(|m| if m { 1.0 } else { 0.0 });
        }
        _ => c.metalness = Some(0.0),
    }
    match schema.as_str() {
        "PrismOpaqueSchema" => {
            // The reflectance at normal incidence: ((n − 1) / (n + 1))².
            if let Some(f0) = number(a, "opaque_f0").filter(|f| (0.0..1.0).contains(f)) {
                let r = f0.sqrt();
                c.ior = Some(((1.0 + r) / (1.0 - r)).clamp(1.0, 5.0));
            }
            if boolean(a, "opaque_translucency") == Some(true) {
                notes.push(format!(
                    "translucency ({} mm deep) is not held",
                    millimetres(a, "opaque_mfp").map_or("?".to_owned(), |d| format!("{d}"))
                ));
            }
            if boolean(a, "opaque_emission") == Some(true) {
                notes.push(format!(
                    "emission ({} cd/m²) is not held",
                    number(a, "opaque_luminance").unwrap_or(0.0)
                ));
            }
        }
        "PrismTransparentSchema" => {
            c.transmission = Some(1.0);
            c.ior = number(a, "transparent_ior").map(|n| n.clamp(1.0, 5.0));
        }
        "GenericSchema" => {
            let t = number(a, "generic_transparency").map(unit);
            c.transmission = t;
            if t.is_some_and(|t| t > 0.0) {
                c.ior = number(a, "generic_refraction_index").map(|n| n.clamp(1.0, 5.0));
            }
            c.opacity = number(a, "generic_cutout_opacity").map(unit);
            if number(a, "generic_self_illum_luminance").is_some_and(|l| l > 0.0) {
                notes.push("self-illumination is not held".into());
            }
        }
        "GlazingSchema" => c.transmission = Some(1.0),
        _ => {}
    }
    for (id, what) in [
        ("surface_normal", "a bump or normal map"),
        ("surface_cutout", "a cut-out image"),
        ("generic_bump_map", "a bump map"),
    ] {
        if !connected(a, id).is_empty() {
            notes.push(format!("{what} is not held"));
        }
    }
    // The other maps' images by name, for the report (mitcad#313): the file
    // holds no image, so no map of the texture set can be made from them.
    let maps = [
        ("surface_roughness", "roughness"),
        ("surface_normal", "normal"),
        ("generic_bump_map", "bump"),
        ("surface_cutout", "cutout"),
    ]
    .into_iter()
    .filter_map(|(id, map)| {
        let image = texture(assets, a, id)?.image;
        (!image.is_empty()).then(|| (map.to_owned(), image))
    })
    .collect();
    Look {
        name: display_name(a),
        change: c,
        notes,
        texture: tex,
        maps,
    }
}

/// The density of a physical material, kg/m³: its structural aspect's
/// (unit tag `0x11001`, kg/m³).
pub fn density(material: &AssetDef, assets: &[AssetDef]) -> Option<f64> {
    connected(material, "physmat_aspects").iter().find_map(|g| {
        let aspect = assets.iter().find(|x| x.id.as_deref() == Some(g))?;
        let v = props(aspect).get("structural_Density")?;
        (v.get("unit")?.as_u64()? == 0x11001).then(|| v.get("value")?.as_f64())?
    })
}

/// Mitcad's material for a file's: one whose name is the file material's
/// (its description up to the first comma, or the last part of its
/// classification) and whose density is the file's within 2 %.
pub fn mitcad_material(material: &AssetDef, density: Option<f64>) -> Option<&'static str> {
    let density = density? / 1000.0;
    let description = material.description.clone().unwrap_or_default();
    let first = description
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let classification = props(material)
        .get("physmat_classification")
        .and_then(Value::as_str)
        .and_then(|c| c.rsplit('.').next())
        .unwrap_or("")
        .trim()
        .to_lowercase();
    // Names written out that Mitcad abbreviates.
    const WRITTEN_OUT: [(&str, &str); 1] = [("acrylonitrile butadiene styrene", "abs")];
    mitcad_model::analysis::MATERIALS
        .iter()
        .find(|m| {
            let name = m.name.to_lowercase();
            // The name, or the name and a grade (`Aluminum 6061`).
            let named = |s: &str| s == name || s.starts_with(&format!("{name} "));
            (named(&first)
                || named(&classification)
                || WRITTEN_OUT.contains(&(first.as_str(), m.id)))
                && (m.density - density).abs() <= 0.02 * m.density
        })
        .map(|m| m.id)
}

/// The report's entry of a physical material.
pub fn material_entry(m: &AssetDef, assets: &[AssetDef]) -> MaterialEntry {
    let d = density(m, assets);
    MaterialEntry {
        asset: m.id.clone().unwrap_or_default(),
        preset: m.name.clone().unwrap_or_default(),
        name: m.description.clone().unwrap_or_default(),
        category: m.category.clone().unwrap_or_default(),
        classification: props(m)
            .get("physmat_classification")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        density: d,
        mitcad: mitcad_material(m, d).map(str::to_owned),
        bodies: 0,
    }
}

/// A document appearance id from a name: `f3d_` and its letters and
/// digits, the rest as `_`.
fn id_of(name: &str) -> String {
    let mut id = String::from("f3d_");
    let mut last = '_';
    for c in name.chars() {
        let c = if c.is_ascii_alphanumeric() {
            c.to_ascii_lowercase()
        } else {
            '_'
        };
        if c == '_' && last == '_' {
            continue;
        }
        id.push(c);
        last = c;
    }
    id.trim_end_matches('_').to_owned()
}

/// The asset a reference names.
fn asset_of<'a>(data: &'a AppearanceData, r: &AssetRef) -> Option<&'a AssetDef> {
    let g = r.asset.as_deref()?;
    data.assets
        .iter()
        .flatten()
        .find(|a| a.id.as_deref() == Some(g))
}

/// How many times `to` is placed under `from` (once in itself).
fn paths<K: Kernel>(
    doc: &mitcad_model::Document<K>,
    from: ComponentUid,
    to: ComponentUid,
) -> usize {
    if to == from {
        return 1;
    }
    if to == ComponentUid::ROOT {
        return 0;
    }
    doc.assembly()
        .occurrences_of(to)
        .map(|o| paths(doc, from, o.parent))
        .sum()
}

/// A component and the components placed inside it, with how many times
/// each is placed under it.
fn subtree<K: Kernel>(
    doc: &mitcad_model::Document<K>,
    component: ComponentUid,
) -> Vec<(ComponentUid, usize)> {
    std::iter::once(ComponentUid::ROOT)
        .chain(doc.assembly().components.iter().map(|c| c.uid))
        .map(|c| (c, paths(doc, component, c)))
        .filter(|&(_, n)| n > 0)
        .collect()
}

/// The appearances family's methods on the replay's `Importer`
/// (mitcad#791): the core's timeline walk reaches them through its
/// `Families` trait.
pub trait Appearances<K: Kernel> {
    /// The file's appearances and materials on the replay's bodies and
    /// faces (part of the import's undo step).
    fn appearances(&mut self);
}

impl<K: Kernel> Appearances<K> for Importer<'_, K> {
    fn appearances(&mut self) {
        let dump = self.dump;
        let Some(data) = dump.f3d.as_ref().and_then(|f| f.appearance.as_ref()) else {
            return;
        };
        let assets: Vec<AssetDef> = data.assets.clone().unwrap_or_default();
        let mut report = AppearanceReport::default();
        // The replay's body for each stored one (blob#record).
        let by_source: HashMap<String, BodyUid> = self
            .report
            .bodies
            .iter()
            .filter_map(|b| Some((b.source.clone(), b.uid?)))
            .collect();
        let mut made: HashMap<String, Option<String>> = HashMap::new();
        let mut appearance_id = |doc: &mut mitcad_model::Document<K>,
                                 report: &mut AppearanceReport,
                                 r: &AssetRef|
         -> Option<String> {
            let a = asset_of(data, r)?;
            let key = a.id.clone()?;
            if let Some(id) = made.get(&key) {
                return id.clone();
            }
            let mut l = look(a, &assets);
            let base = id_of(&l.name);
            let mut out = None;
            for k in 1..1000 {
                let id = if k == 1 {
                    base.clone()
                } else {
                    format!("{base}_{k}")
                };
                let name = if k == 1 {
                    l.name.clone()
                } else {
                    format!("{} ({k})", l.name)
                };
                match doc.appearance(&id) {
                    // The same appearance from an earlier import: reuse it.
                    Some(existing) => {
                        let mut wanted = existing.clone();
                        l.change.apply_to(&mut wanted);
                        if *existing == wanted && existing.name == name {
                            out = Some(id);
                            break;
                        }
                    }
                    None => {
                        let taken = appearance::library().iter().any(|x| x.name == name)
                            || doc.custom_appearances().iter().any(|x| x.name == name);
                        if taken {
                            continue;
                        }
                        l.change.id = Some(id.clone());
                        l.change.name = Some(name);
                        match doc.create_appearance(&l.change) {
                            Ok(id) => out = Some(id),
                            Err(e) => report.gaps.push(format!("appearance {}: {e}", l.name)),
                        }
                        break;
                    }
                }
            }
            if let Some(id) = &out {
                report.appearances.push(AppearanceEntry {
                    id: id.clone(),
                    name: l.name.clone(),
                    asset: key.clone(),
                    schema: a.schema.clone().unwrap_or_default(),
                    preset: a.name.clone().unwrap_or_default(),
                    base_color: l.change.base_color.unwrap_or([0.8; 3]),
                    notes: l.notes.clone(),
                    texture: l.texture.clone(),
                    maps: l.maps.clone(),
                });
            }
            made.insert(key, out.clone());
            out
        };
        let bodies: &[BodyLook] = data.bodies.as_deref().unwrap_or(&[]);
        let mut materials: Vec<MaterialEntry> = Vec::new();
        // Each body's faces and edges, read once for all its faces (an
        // appearance changes no shape).
        let mut indexes: HashMap<BodyUid, Option<refs::PieceIndex>> = HashMap::new();
        // The stored design's solids (the final comparison's).
        let stored: Vec<&str> = self
            .report
            .bodies
            .iter()
            .map(|b| b.source.as_str())
            .collect();
        let source = |b: &BodyLook| match (&b.blob, b.record) {
            (Some(blob), Some(record)) => Some(format!("{blob}#{record}")),
            _ => None,
        };
        // Stored solids no body record names.
        for s in &stored {
            if !bodies.iter().any(|b| source(b).as_deref() == Some(*s)) {
                report.bodies += 1;
                report
                    .gaps
                    .push(format!("stored body {s}: no body record names it"));
            }
        }
        for b in bodies {
            let entity = b.entity.unwrap_or(0);
            // A body record of a body the stored design does not hold (a
            // snapshot blob's copy, one a later operation took): not a body
            // of the design.
            // One whose blob or record is not known is not joined (the
            // stored solid it may be counts above).
            let in_design = source(b).is_some_and(|s| stored.contains(&s.as_str()));
            if !in_design {
                match source(b) {
                    Some(_) => report.not_in_design += 1,
                    None => report.gaps.push(format!(
                        "body record {entity}: {}",
                        match b.blob {
                            None => "the body map names no blob for it",
                            Some(_) => "its selector names no body of its blob",
                        }
                    )),
                }
                continue;
            }
            let uid = source(b).and_then(|s| by_source.get(&s).copied());
            let why_none = || match (&b.blob, b.record) {
                (None, _) => "the body map names no blob for it".to_owned(),
                (Some(_), None) => "its selector names no body of its blob".to_owned(),
                _ => "no body of the replay was matched with it".to_owned(),
            };
            // A body the file hides (its light bulb off, mitcad#1175): a
            // copy kept beside the body made from it would cover that
            // body's features in the view.
            if b.light_bulb == Some(false) {
                report.hidden += 1;
                match uid {
                    Some(uid) => match self.doc.set_body_visible(uid, false) {
                        Ok(()) => report.hidden_set += 1,
                        Err(e) => report.gaps.push(format!("body {entity}: hidden: {e}")),
                    },
                    None => report
                        .gaps
                        .push(format!("body {entity}: hidden: {}", why_none())),
                }
            }
            if b.appearance.is_none() {
                report.bodies += 1;
                report
                    .gaps
                    .push(format!("body {entity}: its record names no appearance"));
            }
            if let Some(r) = &b.appearance {
                report.bodies += 1;
                match (uid, appearance_id(self.doc, &mut report, r)) {
                    (Some(uid), Some(id)) => match self.doc.set_body_appearance(uid, Some(&id)) {
                        Ok(()) => report.bodies_set += 1,
                        Err(e) => report.gaps.push(format!("body {entity}: {e}")),
                    },
                    (None, _) => report.gaps.push(format!("body {entity}: {}", why_none())),
                    (_, None) => report.gaps.push(format!(
                        "body {entity}: its appearance {} is not in the file's assets",
                        r.token.as_deref().unwrap_or("")
                    )),
                }
            }
            if let Some(r) = &b.material {
                report.materials += 1;
                match asset_of(data, r) {
                    None => report.gaps.push(format!(
                        "body {entity}: its material {} is not in the file's assets",
                        r.preset.as_deref().unwrap_or("")
                    )),
                    Some(m) => {
                        let key = m.id.clone().unwrap_or_default();
                        let i = match materials.iter().position(|x| x.asset == key) {
                            Some(i) => i,
                            None => {
                                materials.push(material_entry(m, &assets));
                                materials.len() - 1
                            }
                        };
                        materials[i].bodies += 1;
                        if let (Some(uid), Some(id)) = (uid, materials[i].mitcad.clone()) {
                            match self.doc.set_body_material(uid, Some(&id)) {
                                Ok(()) => report.materials_set += 1,
                                Err(e) => report.gaps.push(format!("body {entity}: {e}")),
                            }
                        }
                    }
                }
            }
            for f in b.faces.iter().flatten() {
                report.faces += 1;
                let guid = f.guid.as_deref().unwrap_or("");
                let Some(uid) = uid else {
                    report
                        .gaps
                        .push(format!("face {guid} of body {entity}: {}", why_none()));
                    continue;
                };
                let Some(fps) = &f.faces else {
                    report.gaps.push(format!(
                        "face {guid} of body {entity}: {}",
                        f.why.as_deref().unwrap_or("not found in the stored design")
                    ));
                    continue;
                };
                let Some(id) = f
                    .appearance
                    .as_ref()
                    .and_then(|r| appearance_id(self.doc, &mut report, r))
                else {
                    report.gaps.push(format!(
                        "face {guid} of body {entity}: its appearance is not in the file's assets"
                    ));
                    continue;
                };
                let mut names: Vec<String> = Vec::new();
                let clock = std::time::Instant::now();
                for fp in fps {
                    let index = indexes
                        .entry(uid)
                        .or_insert_with(|| refs::PieceIndex::new(self.doc, uid));
                    let mut pieces = match index {
                        Some(index) => refs::face_pieces_in(self.doc, uid, fp, index),
                        None => Vec::new(),
                    };
                    if pieces.is_empty()
                        && let Some((_, face)) =
                            refs::resolve_face_where(self.doc, fp, &|b| b == uid)
                    {
                        pieces.push(face);
                    }
                    for p in pieces {
                        let p = p.to_string();
                        if !names.contains(&p) {
                            names.push(p);
                        }
                    }
                }
                if mitcad_import_replay::tracing() {
                    eprintln!(
                        "import: appearance of face {guid} of body {entity}: {} pieces of {} \
                         stored faces in {:.2} s",
                        names.len(),
                        fps.len(),
                        clock.elapsed().as_secs_f64()
                    );
                }
                if names.is_empty() {
                    // How far the nearest face of the replayed body is.
                    let near = fps
                        .first()
                        .and_then(|fp| fp.point_on_face)
                        .map(geom::mm3)
                        .and_then(|p| {
                            let shape = body_scope::shape(self.doc, uid)?;
                            let kernel = self.doc.kernel();
                            kernel
                                .faces(shape)
                                .ok()?
                                .iter()
                                .filter_map(|f| f.names.first()?.parse().ok())
                                .filter_map(|name| {
                                    let near = kernel.face_point_normal(shape, &name, p).ok()?;
                                    Some(geom::distance(near.point, p))
                                })
                                .min_by(f64::total_cmp)
                        });
                    report.gaps.push(format!(
                        "face {guid} of body {entity}: the replay's body has no face there{}",
                        near.map_or(String::new(), |d| format!(
                            " (its nearest face is {d:.4} mm from the stored face's point)"
                        ))
                    ));
                    continue;
                }
                match self.doc.set_face_appearance(uid, &names, Some(&id)) {
                    Ok(()) => report.faces_set += 1,
                    Err(e) => report
                        .gaps
                        .push(format!("face {guid} of body {entity}: {e}")),
                }
            }
        }
        // Overrides: a component's own (the root's) first, then the
        // occurrences'; each covers the bodies of its component and of the
        // components placed inside it.
        let mut overrides: Vec<_> = data.occurrences.iter().flatten().collect();
        overrides.sort_by_key(|o| o.object_id.is_some());
        for o in overrides {
            report.overrides += 1;
            let what = match o.object_id {
                Some(object) => format!("override of occurrence {object}"),
                None => format!("override of component {}", o.component.unwrap_or(0)),
            };
            let component = match o.object_id {
                Some(object) => self
                    .components
                    .occurrence(object)
                    .and_then(|occurrence| self.doc.assembly().occurrence(occurrence))
                    .map(|x| x.component),
                None => o.component.and_then(|c| self.components.of_object(c)),
            };
            let Some(component) = component else {
                report.gaps.push(format!("{what}: it did not come in"));
                continue;
            };
            let below = subtree(self.doc, component);
            // Each placed only under it, and it once.
            let once = paths(self.doc, ComponentUid::ROOT, component) == 1;
            if let Some((c, n, inside)) = below.iter().find_map(|&(c, inside)| {
                let n = paths(self.doc, ComponentUid::ROOT, c);
                (!once || n != inside).then_some((c, n, inside))
            }) {
                report.gaps.push(format!(
                    "{what}: component {c} is placed {n} times, {inside} of them under it, and \
                     Mitcad keeps one appearance per body"
                ));
                continue;
            }
            let Some(id) = o
                .appearance
                .as_ref()
                .and_then(|r| appearance_id(self.doc, &mut report, r))
            else {
                report.gaps.push(format!(
                    "{what}: its appearance is not in the file's assets"
                ));
                continue;
            };
            let mut all = true;
            for &(c, _) in &below {
                for uid in self.doc.component_bodies(c) {
                    if let Err(e) = self.doc.set_body_appearance(uid, Some(&id)) {
                        report.gaps.push(format!("{what}: {e}"));
                        all = false;
                    }
                }
            }
            if all {
                report.overrides_set += 1;
            }
        }
        for w in data.warnings.iter().flatten() {
            report.gaps.push(format!("decoding: {w}"));
        }
        // Face references to faces the stored design does not have (not
        // faces of the design): noted only.
        for f in data.faces.iter().flatten() {
            report.gaps.push(format!(
                "face reference {} (not a face of the design): {}",
                f.guid.as_deref().unwrap_or(""),
                f.why.as_deref().unwrap_or("not found")
            ));
        }
        report.physical_materials = materials;
        self.report.appearances = report;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn asset(v: Value) -> AssetDef {
        serde_json::from_value(v).unwrap()
    }

    /// Transparent, metal, generic and textured assets on Mitcad's set.
    #[test]
    fn assets_map_to_mitcads_parameters() {
        let clear = asset(json!({
            "id": "A", "schema": "PrismTransparentSchema", "name": "Prism-172",
            "ui_name": "Prism-172", "description": "Polycarbonate - clear",
            "properties": {"transparent_color": [0.96, 0.96, 0.95, 1.0],
                           "transparent_ior": 1.585, "surface_roughness": 0.0}}));
        let l = look(&clear, &[]);
        assert_eq!(l.name, "Polycarbonate - clear");
        assert_eq!(l.change.transmission, Some(1.0));
        assert_eq!(l.change.ior, Some(1.585));
        assert_eq!(l.change.base_color, Some([0.96, 0.96, 0.95]));

        // A linear colour is encoded as sRGB; a name given wins.
        let brass = asset(json!({
            "id": "B", "schema": "PrismMetalSchema", "name": "Prism-042",
            "ui_name": "My brass", "description": "Brass - polished",
            "properties": {"metal_f0": [0.5, 0.0, 1.0, 1.0], "metal_f0_colorspace": 2,
                           "surface_roughness": 0.08}}));
        let l = look(&brass, &[]);
        assert_eq!(l.name, "My brass");
        assert_eq!(l.change.metalness, Some(1.0));
        let c = l.change.base_color.unwrap();
        assert!(
            (c[0] - 0.7354).abs() < 1e-3 && c[1] == 0.0 && (c[2] - 1.0).abs() < 1e-12,
            "{c:?}"
        );

        let generic = asset(json!({
            "id": "C", "schema": "GenericSchema", "name": "Generic",
            "properties": {"generic_diffuse": [0.2, 0.4, 0.6, 1.0], "generic_glossiness": 0.7,
                           "generic_is_metal": false, "generic_transparency": 0.0,
                           "common_Tint_toggle": false}}));
        let l = look(&generic, &[]);
        assert!((l.change.roughness.unwrap() - 0.3).abs() < 1e-12);
        assert_eq!(l.change.metalness, Some(0.0));

        let texture = asset(json!({
            "id": "T", "schema": "UnifiedBitmapSchema", "name": "Prism-144_opaque_albedo",
            "properties": {"unifiedbitmap_Bitmap": {"kind": 1, "paths": ["oak.jpg"]},
                           "texture_RealWorldScaleX": {"unit": 8214, "value": 18.0},
                           "texture_RealWorldScaleY": {"unit": 8214, "value": 36.0},
                           "texture_RealWorldOffsetX": {"unit": 8206, "value": 0.0},
                           "texture_RealWorldOffsetY": {"unit": 8206, "value": 0.0},
                           "texture_WAngle": 90.0}}));
        let oak = asset(json!({
            "id": "O", "schema": "PrismOpaqueSchema", "name": "Prism-144",
            "description": "Wood - oak",
            "properties": {"opaque_albedo": [1.0, 1.0, 1.0, 1.0], "opaque_f0": 0.04},
            "connections": {"opaque_albedo": ["T"], "surface_normal": ["N"],
                            "surface_roughness": ["R"]}}));
        let rough = asset(json!({
            "id": "R", "schema": "UnifiedBitmapSchema",
            "properties": {"unifiedbitmap_Bitmap": {"kind": 1, "paths": ["oak_rough.png"]}}}));
        let l = look(&oak, &[texture, rough]);
        // The roughness map's image is named; the normal map's asset is not
        // in the file.
        assert_eq!(
            l.maps,
            BTreeMap::from([("roughness".to_owned(), "oak_rough.png".to_owned())])
        );
        let t = l.texture.unwrap();
        assert_eq!(t.image, "oak.jpg");
        assert_eq!(t.size, Some([18.0 * 25.4, 36.0 * 25.4]));
        assert!((t.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!((l.change.ior.unwrap() - 1.5).abs() < 1e-9);
        assert_eq!(l.notes.len(), 3, "{:?}", l.notes);
    }

    #[test]
    fn materials_are_mitcads_by_name_and_density() {
        let steel = asset(json!({"id": "S", "schema": "PhysMatSchema",
                                 "description": "Steel, mill finish"}));
        assert_eq!(mitcad_material(&steel, Some(7850.0)), Some("steel"));
        assert_eq!(mitcad_material(&steel, Some(7000.0)), None);
        assert_eq!(mitcad_material(&steel, None), None);
        let aluminium = asset(json!({"id": "A", "schema": "PhysMatSchema",
            "description": "Aluminum, magnesium silicon alloy",
            "properties": {"physmat_classification": "Metal.Non Ferrous.Aluminum Alloy"}}));
        assert_eq!(mitcad_material(&aluminium, Some(2700.0)), Some("aluminum"));
        let abs = asset(json!({"id": "P", "schema": "PhysMatSchema",
            "description": "Plastic, generic",
            "properties": {"physmat_classification": "Plastic.ABS Plastic"}}));
        assert_eq!(mitcad_material(&abs, Some(1060.0)), Some("abs"));
        let grade = asset(json!({"id": "G", "schema": "PhysMatSchema",
            "description": "Aluminum 6061"}));
        assert_eq!(mitcad_material(&grade, Some(2700.0)), Some("aluminum"));
        let written = asset(json!({"id": "W", "schema": "PhysMatSchema",
            "description": "Acrylonitrile butadiene styrene"}));
        assert_eq!(mitcad_material(&written, Some(1060.0)), Some("abs"));
    }

    #[test]
    fn names_and_ids() {
        assert!(is_preset("Prism-093") && is_preset("ThermalSolid-092-2"));
        assert!(!is_preset("Custom Blue") && !is_preset("Generic"));
        assert_eq!(
            id_of("Paint - enamel red glossy"),
            "f3d_paint_enamel_red_glossy"
        );
        assert_eq!(id_of("Steel, mill (finish)"), "f3d_steel_mill_finish");
        let satin = asset(
            json!({"id": "X", "name": "PrismMaterial-018_physmat_aspects:Prism-256",
                                 "ui_name": "Prism-256", "description": "Steel - satin"}),
        );
        assert_eq!(display_name(&satin), "Steel - satin");
    }
}
