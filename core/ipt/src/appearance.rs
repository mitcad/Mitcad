// SPDX-License-Identifier: MIT
//! The appearances and materials of a part's bodies and faces
//! (mitcad#1367), as the import's dump gives them (`_f3d.appearance`,
//! which `mitcad-import-appearances` puts on the replayed bodies and faces
//! as it does the `.f3d` import's).
//!
//! - **The assets**: the root stream `Protein` is a u32 length and a ZIP
//!   archive of that length (length 0: no package), the same asset package
//!   as an `.f3d` document's `.protein` entries, read by
//!   `mitcad_f3d_file::protein`.
//! - **The styles** (the application segment, `PmAppSegment`): a rendering
//!   style (`6fd85967…`) or a material style (`6ed85967…`) is a prefix of 27
//!   bytes (25 in segments of major 22 and before), its name, its long name
//!   (u32-counted UTF-16), a u16 and its label (`"1:<name>\n"`); from
//!   major 17 on the asset's GUID (empty for a library preset), the preset
//!   (`Metal-045`, `InvGen-071`, empty for an asset of the document), the
//!   library, two u16 and a 16-byte id. Then come the style's own colours:
//!   four RGBA f32 (ambient, diffuse, specular, emissive) and an f32
//!   shininess. The document's defaults (`cdecfb11…`) refer to the
//!   material style (u32 at 6) and the rendering style (u32 at 10) the
//!   part's bodies have.
//! - **The faces** (the graphics segment, `PmGraphicsSegment`): a face
//!   (`a3e99451…`) is a u32, a u16, u32 flags, a reference to its style
//!   list at 10, three more u32, its edges (a list: u16 2, u16 `0x3000`,
//!   u32 n and, when not empty, two u32 and n references), a byte, its box
//!   (six f64) and its key (u32), then two u32. A style list (`0786eb48…`)
//!   is one such list; a face with a colour of its own lists one primary
//!   colour style (`0f5648af…`, 94 bytes), whose bytes 20..88 are a
//!   rendering style's colours and shininess. The key is the history id
//!   (the second header field) of the faces of the stored bodies the
//!   graphics face stands for: the pieces of a split face share it.
//!
//! A face's colour names the rendering style of the same colours; a body
//! whose faces all have one such style takes it as its own, else the
//! document's default, and its faces of other styles keep theirs. A style
//! whose asset the package does not hold (a library preset) comes in from
//! its own diffuse colour. See `README.md`, *Appearances*.

use std::collections::{BTreeMap, HashMap};

use mitcad_asm::asm::AsmFile;
use mitcad_asm::convert;
use mitcad_dump::{
    AppearanceData, AssetDef, AssetRef, BodyLook, FaceLook, Fingerprint, FingerprintF3d, Geometry,
};
use mitcad_f3d_file::protein::{self, Package};
use mitcad_f3d_names::NamedState;
use serde_json::{Value, json};

use crate::dc::{self, Definitions, Reader};
use crate::{BrepRecord, IptFile};

/// The application segment's name.
pub const APP_SEGMENT: &str = "PmAppSegment";
/// The graphics segment's name.
pub const GRAPHICS_SEGMENT: &str = "PmGraphicsSegment";
/// The root stream of the asset package.
pub const PROTEIN_STREAM: &str = "/Protein";

/// The document's default styles (application segment), as stored.
pub const DEFAULT_STYLES: [u8; 16] = dc::type_id("cdecfb11d1116b250008ebbb21eddc09");
/// A rendering style (application segment).
pub const RENDERING_STYLE: [u8; 16] = dc::type_id("6fd85967d2113878600094b70b02ecb0");
/// A material style (application segment).
pub const MATERIAL_STYLE: [u8; 16] = dc::type_id("6ed85967d2113878600094b70b02ecb0");
/// A face (graphics segment).
pub const GRAPHICS_FACE: [u8; 16] = dc::type_id("a3e99451d2119b2860006ab72c39cdb0");
/// A face's list of styles (graphics segment).
pub const STYLE_LIST: [u8; 16] = dc::type_id("0786eb48d2110c076000f99ac5361ab0");
/// A primary colour style (graphics segment).
pub const PRIMARY_COLOR: [u8; 16] = dc::type_id("0f5648afd411c78d1000d58dc04a0ab5");

/// A style's colours as stored: four RGBA f32 (ambient, diffuse, specular,
/// emissive) and the shininess.
pub type Colors = [u8; 68];

/// A rendering or material style of the application segment.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Its record in the segment.
    pub record: usize,
    pub name: String,
    pub long_name: String,
    /// The asset's GUID (empty for a library preset).
    pub guid: String,
    /// The library preset (empty for an asset of the document).
    pub preset: String,
    pub library: String,
    /// Its own colours (a rendering style's), where the record has them.
    pub colors: Option<Colors>,
}

impl Style {
    /// The diffuse colour of its own colours (RGBA).
    pub fn diffuse(&self) -> Option<[f64; 4]> {
        self.colors.as_ref().map(diffuse)
    }
}

/// The diffuse colour (the second RGBA) of a style's colours.
pub fn diffuse(c: &Colors) -> [f64; 4] {
    let f = |k: usize| {
        let at = 16 + 4 * k;
        f64::from(f32::from_le_bytes([c[at], c[at + 1], c[at + 2], c[at + 3]]))
    };
    [f(0), f(1), f(2), f(3)]
}

/// A graphics face with a colour of its own: its key and the colours.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceColor {
    pub key: u32,
    pub colors: Colors,
}

/// What a part says about its appearances.
#[derive(Clone, Debug, Default)]
pub struct Appearances {
    /// The assets of the `Protein` stream.
    pub package: Package,
    pub styles: Vec<Style>,
    pub materials: Vec<Style>,
    /// The bodies' default rendering style (index into `styles`) and
    /// material style (index into `materials`).
    pub default_style: Option<usize>,
    pub default_material: Option<usize>,
    pub faces: Vec<FaceColor>,
    /// What did not read.
    pub warnings: Vec<String>,
}

/// The package of a `Protein` stream: a u32 length and a ZIP archive of
/// that length; None for the empty stream (length 0).
pub fn package(stream: &[u8]) -> Result<Option<Package>, String> {
    let Some(b) = stream.get(..4) else {
        return Err("the stream is shorter than its length".to_owned());
    };
    let n = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
    if n == 0 {
        return Ok(None);
    }
    let zip = stream.get(4..4 + n).ok_or_else(|| {
        format!(
            "{n} bytes announced, {} in the stream",
            stream.len().saturating_sub(4)
        )
    })?;
    Package::read(zip.to_vec()).map(Some)
}

/// A style record at the prefix length `at`; None when it is not laid out
/// so (the label must be `"1:<name>\n"`).
fn style_at(bytes: &[u8], record: usize, major: u8, at: usize) -> Option<Style> {
    let mut r = Reader::at(bytes, at);
    let name = r.text().ok()?;
    let long_name = r.text().ok()?;
    r.skip(2).ok()?;
    let label = r.text().ok()?;
    if !label.ends_with('\n') {
        return None;
    }
    let (mut guid, mut preset, mut library) = (String::new(), String::new(), String::new());
    if major > 16 {
        guid = r.text().ok()?;
        preset = r.text().ok()?;
        library = r.text().ok()?;
        r.skip(4 + 16).ok()?;
    }
    let colors = r.take(68).ok().map(|b| {
        let mut c = [0u8; 68];
        c.copy_from_slice(b);
        c
    });
    Some(Style {
        record,
        name,
        long_name,
        guid,
        preset,
        library,
        colors,
    })
}

/// A rendering or material style record (see the module docs).
pub fn style(bytes: &[u8], record: usize, major: u8) -> Option<Style> {
    let prefixes = if major > 22 { [27, 25] } else { [25, 27] };
    prefixes
        .into_iter()
        .find_map(|at| style_at(bytes, record, major, at))
}

/// A list of the graphics segment at `at`: its items and where it ends.
fn list_at(b: &[u8], at: usize) -> Option<(Vec<u32>, usize)> {
    let mut r = Reader::at(b, at);
    if r.u16().ok()? != 2 || r.u16().ok()? != 0x3000 {
        return None;
    }
    let n = r.u32().ok()? as usize;
    if n == 0 {
        return Some((Vec::new(), r.pos));
    }
    r.skip(8).ok()?;
    if n > r.left() / 4 {
        return None;
    }
    let items = (0..n).map(|_| r.u32().ok()).collect::<Option<Vec<u32>>>()?;
    Some((items, r.pos))
}

/// A graphics face's key: after its edges, a byte, six f64, the key and two
/// u32 to the record's end.
fn face_key(b: &[u8]) -> Option<u32> {
    let (_, end) = list_at(b, 26)?;
    if end + 61 != b.len() {
        return None;
    }
    Reader::at(b, end + 49).u32().ok()
}

/// The graphics faces with a colour of their own: those whose style list
/// holds exactly one primary colour style.
pub fn face_colors(g: &Definitions) -> Vec<FaceColor> {
    let mut out = Vec::new();
    for i in g.of_type(&GRAPHICS_FACE) {
        let b = g.bytes(i);
        let Some(list) = Reader::at(b, 10)
            .reference()
            .ok()
            .flatten()
            .filter(|&l| g.is(l, &STYLE_LIST))
        else {
            continue;
        };
        let Some((items, _)) = list_at(g.bytes(list), 0) else {
            continue;
        };
        let primary: Vec<usize> = items
            .iter()
            .filter_map(|&x| dc::reference(x))
            .filter(|&k| g.is(k, &PRIMARY_COLOR))
            .collect();
        let &[p] = primary.as_slice() else {
            continue;
        };
        let (Some(colors), Some(key)) = (g.bytes(p).get(20..88), face_key(b)) else {
            continue;
        };
        let mut c = [0u8; 68];
        c.copy_from_slice(colors);
        out.push(FaceColor { key, colors: c });
    }
    out
}

impl Appearances {
    /// Reads the package, the styles and the faces' colours.
    pub fn read(file: &IptFile) -> Appearances {
        let mut out = Appearances::default();
        if file.container.find(PROTEIN_STREAM).is_some() {
            match file
                .container
                .read_path(PROTEIN_STREAM)
                .map_err(|e| e.to_string())
                .and_then(|d| package(&d))
            {
                Ok(Some(p)) => out.package = p,
                Ok(None) => {}
                Err(e) => out.warnings.push(format!("Protein: {e}")),
            }
        }
        match file.segment_records(APP_SEGMENT) {
            Ok(Some(app)) => out.read_styles(&app),
            Ok(None) => {}
            Err(e) => out.warnings.push(e.to_string()),
        }
        match file.segment_records(GRAPHICS_SEGMENT) {
            Ok(Some(g)) => out.faces = face_colors(&g),
            Ok(None) => {}
            Err(e) => out.warnings.push(e.to_string()),
        }
        out
    }

    /// The styles of the application segment and the defaults.
    pub fn read_styles(&mut self, app: &Definitions) {
        for i in 0..app.len() {
            let rendering = app.is(i, &RENDERING_STYLE);
            if !rendering && !app.is(i, &MATERIAL_STYLE) {
                continue;
            }
            match style(app.bytes(i), i, app.major) {
                Some(s) if rendering => self.styles.push(s),
                Some(s) => self.materials.push(s),
                None => self.warnings.push(format!(
                    "{APP_SEGMENT}#{i}: a {} style that does not read",
                    if rendering { "rendering" } else { "material" }
                )),
            }
        }
        if let Some(d) = app.of_type(&DEFAULT_STYLES).next() {
            let mut r = Reader::at(app.bytes(d), 6);
            let material = r.reference().ok().flatten();
            let rendering = r.reference().ok().flatten();
            self.default_material =
                material.and_then(|m| self.materials.iter().position(|s| s.record == m));
            self.default_style =
                rendering.and_then(|m| self.styles.iter().position(|s| s.record == m));
            if self.default_style.is_none() {
                self.warnings.push(format!(
                    "{APP_SEGMENT}#{d}: the default rendering style {rendering:?} is not a style"
                ));
            }
        }
    }

    /// The asset of the package a style names: by its GUID (also with
    /// `_Post2015` suffixes), else by its preset in its library (an asset
    /// named so, or `<material>:<preset>`, a material's appearance); only
    /// assets `wanted` accepts.
    pub fn asset(
        &self,
        s: &Style,
        wanted: impl Fn(&protein::Asset) -> bool,
    ) -> Option<&protein::Asset> {
        let assets = &self.package.assets;
        if !s.guid.is_empty() {
            let post = format!("{}_POST2015", s.guid.to_ascii_uppercase());
            let found = assets
                .iter()
                .filter(|&a| wanted(a))
                .find(|a| a.guid.eq_ignore_ascii_case(&s.guid))
                .or_else(|| {
                    assets
                        .iter()
                        .filter(|&a| wanted(a))
                        .find(|a| a.guid.to_ascii_uppercase().starts_with(&post))
                });
            if found.is_some() {
                return found;
            }
        }
        if s.preset.is_empty() {
            return None;
        }
        let suffix = format!(":{}", s.preset);
        assets.iter().filter(|&a| wanted(a)).find(|a| {
            (a.name == s.preset || a.name.ends_with(&suffix))
                && (s.library.is_empty() || a.library.eq_ignore_ascii_case(&s.library))
        })
    }

    /// A rendering style as the dump names it: its asset, or (no asset in
    /// the package) an asset of its own diffuse colour added to `assets`.
    fn style_ref(&self, s: &Style, assets: &mut Vec<AssetDef>) -> AssetRef {
        let asset = self
            .asset(s, protein::Asset::is_appearance)
            .map(|a| a.guid.clone())
            .or_else(|| {
                let c = s.diffuse()?;
                let id = format!("ipt-style-{}", s.record);
                if !assets.iter().any(|a| a.id.as_deref() == Some(id.as_str())) {
                    assets.push(own_color(s, &id, c));
                }
                Some(id)
            });
        AssetRef {
            token: Some(if s.guid.is_empty() {
                s.preset.clone()
            } else {
                s.guid.clone()
            }),
            library: Some(s.library.clone()),
            preset: Some(s.preset.clone()),
            display: Some(s.name.clone()),
            asset,
            ..AssetRef::default()
        }
    }

    /// The default material style as the dump names it, where the package
    /// holds its physical material.
    fn material_ref(&self) -> Option<AssetRef> {
        let s = &self.materials[self.default_material?];
        let a = self.asset(s, |a| a.schema == "PhysMatSchema")?;
        Some(AssetRef {
            token: Some(a.guid.clone()),
            library: Some(s.library.clone()),
            preset: Some(s.preset.clone()),
            display: Some(s.name.clone()),
            asset: Some(a.guid.clone()),
            ..AssetRef::default()
        })
    }

    /// The rendering style a face colour names: the default style when the
    /// colours are its, else the first style of the same colours.
    fn style_of(&self, c: &Colors) -> Option<usize> {
        self.default_style
            .filter(|&d| self.styles[d].colors.as_ref() == Some(c))
            .or_else(|| {
                self.styles
                    .iter()
                    .position(|s| s.colors.as_ref() == Some(c))
            })
    }

    /// The appearances of the stored bodies of the B-rep records, as the
    /// dump's: each top-level body (its source `<record place>/<body
    /// record>`, as the import names its stored bodies) with its
    /// appearance and material, and its faces of other styles found by
    /// their keys in the stored state.
    pub fn data(&self, records: &[BrepRecord]) -> AppearanceData {
        let mut assets: Vec<AssetDef> = self.package.assets.iter().map(asset_def).collect();
        let mut warnings = self.warnings.clone();
        warnings.extend(self.package.errors.iter().map(|e| format!("Protein: {e}")));
        let default = self
            .default_style
            .map(|k| self.style_ref(&self.styles[k], &mut assets));
        let material = self.material_ref();
        let mut by_key: HashMap<u32, usize> = HashMap::new();
        let mut unmatched = 0;
        for f in &self.faces {
            match self.style_of(&f.colors) {
                Some(s) => {
                    by_key.insert(f.key, s);
                }
                None => unmatched += 1,
            }
        }
        if unmatched > 0 {
            warnings.push(format!(
                "{unmatched} coloured faces have the colours of no rendering style"
            ));
        }
        let mut bodies = Vec::new();
        for record in records {
            let place = record.place();
            let file = match AsmFile::parse(&record.asm) {
                Ok(f) => f,
                Err(e) => {
                    warnings.push(format!("{place}: {e}"));
                    continue;
                }
            };
            let tops: Vec<usize> = convert::body_records(&file)
                .into_iter()
                .filter(|(_, top)| *top)
                .map(|(r, _)| r)
                .collect();
            // The stored faces with their keys, where faces have colours.
            let state = (!by_key.is_empty()).then(|| NamedState::new(&file, None));
            for body_record in tops {
                let mut look = BodyLook {
                    entity: Some(body_record as u64),
                    source: Some(format!("{place}/{body_record}")),
                    appearance: default.clone(),
                    material: material.clone(),
                    ..BodyLook::default()
                };
                if let Some(state) = &state
                    && let Some(b) = state.bodies.iter().position(|x| x.record == body_record)
                {
                    self.body_faces(state, b, &by_key, &mut look, &mut assets, &mut warnings);
                }
                bodies.push(look);
            }
        }
        AppearanceData {
            assets: Some(assets),
            bodies: Some(bodies),
            warnings: (!warnings.is_empty()).then_some(warnings),
            ..AppearanceData::default()
        }
    }

    /// A body's faces: one style on all its keyed faces is the body's;
    /// else its faces of styles other than the default, by style.
    fn body_faces(
        &self,
        state: &NamedState,
        body: usize,
        by_key: &HashMap<u32, usize>,
        look: &mut BodyLook,
        assets: &mut Vec<AssetDef>,
        warnings: &mut Vec<String>,
    ) {
        let keyed: Vec<(usize, Option<usize>)> = (0..state.faces.len())
            .filter(|&i| state.faces[i].body == body)
            .filter_map(|i| {
                let key = u32::try_from(state.face_key(i)?).ok()?;
                Some((i, by_key.get(&key).copied()))
            })
            .collect();
        let first = keyed.first().and_then(|f| f.1);
        if let Some(s) =
            first.filter(|&s| Some(s) != self.default_style && keyed.iter().all(|f| f.1 == Some(s)))
        {
            look.appearance = Some(self.style_ref(&self.styles[s], assets));
            return;
        }
        let mut by_style: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, s) in keyed {
            if let Some(s) = s.filter(|&s| Some(s) != self.default_style) {
                by_style.entry(s).or_default().push(i);
            }
        }
        let mut faces = Vec::new();
        for (s, members) in by_style {
            let style = &self.styles[s];
            let mut found = Vec::new();
            let mut why = Vec::new();
            for &i in &members {
                match face_fingerprint(state, i) {
                    Ok(fp) => found.push(fp),
                    Err(e) => why.push(e),
                }
            }
            let guid = format!("style {} ({} faces)", style.name, members.len());
            if !found.is_empty() && !why.is_empty() {
                warnings.push(format!(
                    "body {}: {} of the faces of {guid} not found: {}",
                    look.entity.unwrap_or(0),
                    why.len(),
                    why.join("; ")
                ));
            }
            faces.push(FaceLook {
                guid: Some(guid),
                appearance: Some(self.style_ref(style, assets)),
                color: style.diffuse(),
                why: found.is_empty().then(|| why.join("; ")),
                faces: (!found.is_empty()).then_some(found),
                ..FaceLook::default()
            });
        }
        if !faces.is_empty() {
            look.faces = Some(faces);
        }
    }
}

/// The appearances of a part as the dump's (`_f3d.appearance`); None when
/// it has neither styles nor assets.
pub fn read(file: &IptFile) -> Option<AppearanceData> {
    let a = Appearances::read(file);
    if a.styles.is_empty() && a.package.assets.is_empty() {
        return None;
    }
    let records = file.brep_records().unwrap_or_default();
    Some(a.data(&records))
}

/// A style without an asset in the package, as an asset of its own diffuse
/// colour (its alpha the opacity).
fn own_color(s: &Style, id: &str, c: [f64; 4]) -> AssetDef {
    let mut properties = BTreeMap::new();
    properties.insert("generic_diffuse".to_owned(), json!([c[0], c[1], c[2], 1.0]));
    properties.insert(
        "generic_transparency".to_owned(),
        json!((1.0 - c[3]).clamp(0.0, 1.0)),
    );
    AssetDef {
        id: Some(id.to_owned()),
        schema: Some("GenericSchema".to_owned()),
        name: Some(if s.preset.is_empty() {
            id.to_owned()
        } else {
            s.preset.clone()
        }),
        library: Some(s.library.clone()),
        ui_name: Some(s.name.clone()),
        properties: Some(properties),
        ..AssetDef::default()
    }
}

/// An asset's property value as JSON (as the `.f3d` decoder writes it).
fn json_value(v: &protein::Value) -> Value {
    match v {
        protein::Value::Bool(b) => json!(b),
        protein::Value::Int(i) => json!(i),
        protein::Value::Float(f) => json!(f),
        protein::Value::Quantity { unit, value } => json!({"unit": unit, "value": value}),
        protein::Value::Text(s) => json!(s),
        protein::Value::Color(c) => json!(c),
        protein::Value::Paths { kind, paths } => json!({"kind": kind, "paths": paths}),
        protein::Value::None => Value::Null,
        protein::Value::List(l) => Value::Array(l.iter().map(json_value).collect()),
    }
}

/// An asset of the package as the dump's.
fn asset_def(a: &protein::Asset) -> AssetDef {
    let mut connections = BTreeMap::new();
    let mut properties = BTreeMap::new();
    for (id, p) in &a.properties {
        properties.insert(id.clone(), json_value(&p.value));
        if !p.connections.is_empty() {
            connections.insert(id.clone(), p.connections.clone());
        }
    }
    AssetDef {
        id: Some(a.guid.clone()),
        schema: Some(a.schema.clone()),
        name: Some(a.name.clone()),
        library: Some(a.library.clone()),
        ui_name: a.ui_name.clone(),
        category: a.category.clone(),
        description: a.description.clone(),
        properties: Some(properties),
        connections: (!connections.is_empty()).then_some(connections),
        ..AssetDef::default()
    }
}

/// A stored face as a feature's face input (cm): a plane's or cylinder's
/// geometry, or another surface's kind, and a point inside the face with
/// the normal there (as the `.f3d` decoder's `inputs::fill_face`).
fn face_fingerprint(state: &NamedState, i: usize) -> Result<Fingerprint, String> {
    let cm = |p: [f64; 3]| p.map(|x| x / 10.0);
    let mut fp = Fingerprint {
        object_type: Some("BRepFace".to_owned()),
        f3d: Some(FingerprintF3d::default()),
        ..Fingerprint::default()
    };
    if let (Err(_), Ok(p)) = (state.cylinder(i), state.plane(i)) {
        fp.geometry = Some(Geometry {
            geometry_type: Some("Plane".to_owned()),
            origin: Some(cm(p.point)),
            normal: Some(p.normal),
            ..Geometry::default()
        });
        fp.point_on_face = Some(cm(p.point));
        fp.normal_at_point = Some(p.normal);
        return Ok(fp);
    }
    match state.cylinder(i) {
        Ok(g) => {
            fp.geometry = Some(Geometry {
                geometry_type: Some("Cylinder".to_owned()),
                origin: Some(cm(g.origin)),
                axis: Some(g.axis),
                radius: Some(g.radius / 10.0),
                ..Geometry::default()
            });
            fp.point_on_face = Some(cm(g.point));
            fp.normal_at_point = Some(g.normal);
        }
        Err(_) => {
            let s = state.surface_point(i)?;
            let torus = s.minor.is_some();
            fp.geometry = Some(Geometry {
                geometry_type: Some(s.kind.to_owned()),
                origin: s.origin.map(cm),
                axis: s.axis,
                radius: s.radius.filter(|_| !torus).map(|r| r / 10.0),
                major_radius: s.minor.and(s.radius).map(|r| r / 10.0),
                minor_radius: s.minor.map(|r| r / 10.0),
                half_angle: s.half_angle,
                ..Geometry::default()
            });
            fp.point_on_face = Some(cm(s.point));
            fp.normal_at_point = Some(s.normal);
            if !s.spread.is_empty()
                && let Some(f) = fp.f3d.as_mut()
            {
                f.face_points = Some(s.spread.into_iter().map(cm).collect());
            }
        }
    }
    Ok(fp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_appearances_of_a_test_part() {
        let file = IptFile::parse(crate::testdata::test_part_with_appearances()).unwrap();
        let a = Appearances::read(&file);
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        assert_eq!(a.package.assets.len(), 1);
        let names: Vec<&str> = a.styles.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Semi-Polished", "Blue Grey", "Red"]);
        let default = &a.styles[a.default_style.unwrap()];
        assert_eq!(
            (default.name.as_str(), default.preset.as_str()),
            ("Semi-Polished", "Metal-045")
        );
        assert_eq!(a.materials[a.default_material.unwrap()].name, "Steel");
        // The library preset is not in the package, the document's asset is.
        assert!(a.asset(default, protein::Asset::is_appearance).is_none());
        let blue = &a.styles[1];
        assert_eq!(
            a.asset(blue, protein::Asset::is_appearance)
                .map(|x| x.guid.as_str()),
            Some(crate::testdata::BLUE_ASSET)
        );
        let keys: Vec<u32> = a.faces.iter().map(|f| f.key).collect();
        assert_eq!(keys, [12, 13]);
        assert_eq!(a.faces[0].colors, a.styles[1].colors.unwrap());

        let data = read(&file).unwrap();
        let bodies = data.bodies.as_ref().unwrap();
        assert_eq!(bodies.len(), 1);
        let body = &bodies[0];
        assert!(
            body.source
                .as_deref()
                .is_some_and(|s| s.starts_with("PmBRepSegment#") && s.ends_with("/1")),
            "{:?}",
            body.source
        );
        // The body: the default style, from its own colour.
        let appearance = body.appearance.as_ref().unwrap();
        assert_eq!(appearance.preset.as_deref(), Some("Metal-045"));
        let own = data
            .assets
            .iter()
            .flatten()
            .find(|x| x.id == appearance.asset)
            .unwrap();
        assert_eq!(own.schema.as_deref(), Some("GenericSchema"));
        assert_eq!(own.ui_name.as_deref(), Some("Semi-Polished"));
        let c = own.properties.as_ref().unwrap()["generic_diffuse"].clone();
        assert_eq!(c, json!([0.75, 0.75, 0.75, 1.0]));
        // Its material is not in the package: none.
        assert!(body.material.is_none());
        // The top (key 12) and the face x = 1 (key 13) are blue grey: one
        // face entry of two faces, their points on them.
        let faces = body.faces.as_ref().unwrap();
        assert_eq!(faces.len(), 1);
        let f = &faces[0];
        assert_eq!(
            f.appearance.as_ref().unwrap().asset.as_deref(),
            Some(crate::testdata::BLUE_ASSET)
        );
        assert_eq!(f.color, Some([0.25, 0.5, 0.75, 1.0]));
        let points: Vec<[f64; 3]> = f
            .faces
            .iter()
            .flatten()
            .map(|fp| fp.point_on_face.unwrap())
            .collect();
        assert_eq!(points.len(), 2);
        assert!((points[0][2] - 1.0).abs() < 1e-9, "{points:?}");
        assert!((points[1][0] - 1.0).abs() < 1e-9, "{points:?}");
    }

    /// All faces of one style: the style is the body's.
    #[test]
    fn one_style_on_every_face_is_the_bodys() {
        let file = IptFile::parse(crate::testdata::test_part_with_body_appearance()).unwrap();
        let data = read(&file).unwrap();
        let body = &data.bodies.as_ref().unwrap()[0];
        assert!(body.faces.is_none());
        assert_eq!(
            body.appearance.as_ref().unwrap().display.as_deref(),
            Some("Red")
        );
    }

    /// A part with a design carries its appearances in the design's dump,
    /// the same as the part's own (mitcad#1375: the import of the bodies
    /// only reads them from the part).
    #[test]
    fn a_designed_part_carries_its_appearances_in_its_design() {
        let file = IptFile::parse(crate::testdata::test_part_with_designed_appearances()).unwrap();
        let design = crate::design::read(&file, "part.ipt").unwrap().unwrap();
        assert!(!design.items.is_empty());
        let data = design.dump.f3d.as_ref().and_then(|f| f.appearance.as_ref());
        assert_eq!(data, read(&file).as_ref());
        assert!(data.is_some_and(|d| d.bodies.as_ref().is_some_and(|b| b.len() == 1)));
    }

    #[test]
    fn an_empty_stream_has_no_package() {
        assert!(package(&[0, 0, 0, 0]).unwrap().is_none());
        assert!(package(&[9, 0, 0, 0, 1]).is_err());
        assert!(package(&[1]).is_err());
    }

    #[test]
    fn styles_of_older_segments() {
        // Major 16: the label, then the colours.
        let s = crate::testdata::rendering_style("Old", "", "", "", [0.5; 4], 16);
        let read = style(&s, 3, 16).unwrap();
        assert_eq!((read.name.as_str(), read.preset.as_str()), ("Old", ""));
        assert_eq!(read.diffuse(), Some([0.5; 4]));
        // Major 22: a prefix of 25 bytes.
        let s = crate::testdata::rendering_style("Mid", "", "InvGen-071", "LIB", [0.25; 4], 22);
        let read = style(&s, 3, 22).unwrap();
        assert_eq!(
            (read.preset.as_str(), read.library.as_str()),
            ("InvGen-071", "LIB")
        );
        assert_eq!(read.diffuse(), Some([0.25; 4]));
    }
}
