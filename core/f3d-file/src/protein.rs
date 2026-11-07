// SPDX-License-Identifier: MIT
//! The appearance and material assets of a document
//! (`ProteinAssets.BlobParts/*.protein`, mitcad#228).
//!
//! A package is a ZIP archive (inside the `.f3d` ZIP) with one folder per
//! asset set: `Schemas/*.xml` (the property schemas, each naming the schema
//! it is based on) and `AssetData/`, whose `InstanceProperties.bin` holds
//! the assets' values and `DefinitionIteratorProperties.bin` their
//! descriptions. Layouts (verified on the appearance variant models, every
//! record decoding to its exact end; see `core/import/docs/f3d-appearances.md`,
//! *Appearances and materials*):
//!
//! ```text
//! stream   u32 page size P (136) | 12 bytes | pages of P bytes
//! page     8 bytes of framing | P − 8 bytes of body
//!          start of a record:  u32 any | u16 P − 8 | u16 1  (whole body)
//!          continuation:       u32 any | u16 P − 8 | u16 0  (whole body)
//!          last page:          u32 0xffffffff | u16 used | u16 any
//!                              (the first `used` bytes of the body)
//! instance str schema | str asset GUID | str asset name | str library id
//!          then the schema's properties in byte order of their ids
//!          (the schema and those it is based on; a derived schema's
//!          property replaces the base's of the same id), leaving out the
//!          library id (in the header), `readonly` ones and those marked
//!          `definitionIteratorData`
//! value    Boolean u8 | Integer, Choice u32 | Float f64 (with a `unit`
//!          attribute: u32 unit tag, f64) | Distance u32 unit tag, f64 |
//!          String, Uuid, URL str | Color 4 × f64 (RGBA in [0, 1]) |
//!          Reference nothing | TextureURI u8 kind: 0 u32 n, n × str;
//!          1 str. `allowmultiplevalues` (but TextureURI): u32 n, n values.
//!          A property with `allowconnectedassets` is followed by u8 0, or
//!          u8 1 u8 1 u32 n, n × str (GUIDs of texture assets)
//! description  str schema | u8 | str UI name | str asset name |
//!          u32 version | version 2: str category, str (`Default`),
//!          str description | u32 n, n × str keyword | u32 n, n × str
//!          preview
//! str      u32 length, bytes (ASCII)
//! ```
//!
//! The unit tag of a quantity is `class << 12 | unit`: lengths are class 2,
//! `0x200d` centimetre, `0x200e` millimetre, `0x2016` inch.
//!
//! A schema whose properties include `TextureMap2dSchema`'s writes one of
//! its integer map channel properties less: the record decodes to its end
//! with one of them left out (`texture_MapChannel_ID_Advanced` or
//! `texture_MapChannel`), which keeps the schema's default.

use std::collections::{BTreeMap, HashMap};

use crate::zip::Archive;

/// One property's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    /// Integer and Choice (a choice's stored number).
    Int(i64),
    Float(f64),
    /// A Distance, or a Float with a unit: the unit tag and the value in
    /// that unit.
    Quantity {
        unit: u32,
        value: f64,
    },
    Text(String),
    /// RGBA, each in [0, 1].
    Color([f64; 4]),
    /// A texture's image paths (kind 0: library resources; kind 1: one
    /// file name).
    Paths {
        kind: u8,
        paths: Vec<String>,
    },
    /// A Reference (its connections only).
    None,
    /// The values of a property that takes several.
    List(Vec<Value>),
}

/// A property of an asset: its value and the assets connected to it
/// (texture assets by GUID).
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub value: Value,
    pub connections: Vec<String>,
}

/// An asset: an appearance, a physical material, one of a material's
/// aspects, or a texture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Asset {
    /// The schema (`PrismOpaqueSchema`, `PhysMatSchema`, ...).
    pub schema: String,
    /// The asset's GUID (the design streams name appearances by it).
    pub guid: String,
    /// The asset's name: a library preset (`Prism-093`,
    /// `PrismMaterial-018`) or an id of its own.
    pub name: String,
    /// The library the name belongs to (a GUID, or a path).
    pub library: String,
    /// From the descriptions: the name shown, its category and
    /// description, when the package describes the asset.
    pub ui_name: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    /// Every property, by id.
    pub properties: BTreeMap<String, Property>,
}

impl Asset {
    pub fn get(&self, id: &str) -> Option<&Value> {
        self.properties.get(id).map(|p| &p.value)
    }

    pub fn color(&self, id: &str) -> Option<[f64; 4]> {
        match self.get(id)? {
            Value::Color(c) => Some(*c),
            _ => None,
        }
    }

    pub fn float(&self, id: &str) -> Option<f64> {
        match self.get(id)? {
            Value::Float(v) => Some(*v),
            Value::Quantity { value, .. } => Some(*value),
            Value::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// A length property in millimetres (known length units only).
    pub fn millimetres(&self, id: &str) -> Option<f64> {
        match self.get(id)? {
            Value::Quantity { unit, value } => length_mm(*unit, *value),
            _ => None,
        }
    }

    pub fn int(&self, id: &str) -> Option<i64> {
        match self.get(id)? {
            Value::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn bool(&self, id: &str) -> Option<bool> {
        match self.get(id)? {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn text(&self, id: &str) -> Option<&str> {
        match self.get(id)? {
            Value::Text(v) => Some(v),
            _ => None,
        }
    }

    /// The assets connected to a property (texture GUIDs).
    pub fn connections(&self, id: &str) -> &[String] {
        self.properties
            .get(id)
            .map_or(&[], |p| p.connections.as_slice())
    }

    /// Whether the asset is an appearance (a render material schema).
    pub fn is_appearance(&self) -> bool {
        self.schema.starts_with("Prism") && self.schema != "PrismCommonSchema"
            || matches!(
                self.schema.as_str(),
                "GenericSchema"
                    | "MetalSchema"
                    | "MetallicPaintSchema"
                    | "PlasticVinylSchema"
                    | "CeramicSchema"
                    | "ConcreteSchema"
                    | "GlazingSchema"
                    | "HardwoodSchema"
                    | "MasonryCMUSchema"
                    | "MirrorSchema"
                    | "SolidGlassSchema"
                    | "StoneSchema"
                    | "WallPaintSchema"
                    | "WaterSchema"
            )
    }
}

/// A length in millimetres from a unit tag (centimetre, millimetre, inch,
/// metre, foot); None for another unit.
pub fn length_mm(unit: u32, value: f64) -> Option<f64> {
    let scale = match unit {
        0x200c => 1000.0,
        0x200d => 10.0,
        0x200e => 1.0,
        0x2016 => 25.4,
        0x2017 => 304.8,
        _ => return None,
    };
    Some(value * scale)
}

/// The assets of a document's packages.
#[derive(Clone, Debug, Default)]
pub struct Package {
    pub assets: Vec<Asset>,
    /// Records that did not decode (asset GUID or record number, why).
    pub errors: Vec<String>,
}

impl Package {
    /// Reads a `.protein` package (the ZIP's bytes).
    pub fn read(data: Vec<u8>) -> Result<Package, String> {
        let zip = Archive::new(data).map_err(|e| format!("package: {e}"))?;
        let mut folders: Vec<String> = Vec::new();
        for e in zip.entries() {
            if let Some(f) = e.name.strip_suffix("AssetData/InstanceProperties.bin")
                && !folders.iter().any(|x| x == f)
            {
                folders.push(f.to_owned());
            }
        }
        let mut out = Package::default();
        for folder in folders {
            let mut schemas: HashMap<String, Schema> = HashMap::new();
            for e in zip.entries() {
                let Some(rest) = e.name.strip_prefix(&format!("{folder}Schemas/")) else {
                    continue;
                };
                if !rest.ends_with(".xml") || rest == "cnx.xml" || rest == "core.xml" {
                    continue;
                }
                let text = zip.read(e).map_err(|err| format!("{}: {err}", e.name))?;
                if let Some(s) = Schema::parse(&String::from_utf8_lossy(&text)) {
                    schemas.insert(s.uid.clone(), s);
                }
            }
            let read = |name: &str| zip.read_by_name(&format!("{folder}AssetData/{name}"));
            let instances = read("InstanceProperties.bin").map_err(|e| e.to_string())?;
            let descriptions = read("DefinitionIteratorProperties.bin").ok();
            let described: Vec<Description> = descriptions
                .map(|d| records(&d).iter().filter_map(|r| description(r)).collect())
                .unwrap_or_default();
            let instances = records(&instances);
            // The descriptions are written in the instances' order (an
            // edited copy of a library appearance names another asset in
            // its description): by position where the schemas agree
            // throughout, else by schema and asset name.
            let by_position = described.len() == instances.len()
                && instances
                    .iter()
                    .zip(&described)
                    .all(|(i, d)| Reader { d: i, p: 0 }.str().ok().as_ref() == Some(&d.schema));
            for (k, rec) in instances.into_iter().enumerate() {
                match instance(&rec, &schemas) {
                    Ok(mut a) => {
                        let d = if by_position {
                            described.get(k)
                        } else {
                            described
                                .iter()
                                .find(|d| d.schema == a.schema && d.asset == a.name)
                        };
                        if let Some(d) = d {
                            a.ui_name = Some(d.ui_name.clone());
                            a.category = d.category.clone();
                            a.description = d.description.clone();
                        }
                        out.assets.push(a);
                    }
                    Err(e) => out.errors.push(format!("record {k}: {e}")),
                }
            }
        }
        Ok(out)
    }

    pub fn by_guid(&self, guid: &str) -> Option<&Asset> {
        self.assets
            .iter()
            .find(|a| a.guid.eq_ignore_ascii_case(guid))
    }

    /// Adds another package's assets (a document can have several).
    pub fn extend(&mut self, other: Package) {
        for a in other.assets {
            if self.by_guid(&a.guid).is_none() {
                self.assets.push(a);
            }
        }
        self.errors.extend(other.errors);
    }
}

/// The logical records of a paged stream.
pub fn records(d: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let Some(page) = u32_at(d, 0).map(|p| p as usize) else {
        return out;
    };
    if !(16..=1 << 20).contains(&page) {
        return out;
    }
    let body = page - 8;
    let mut open: Option<Vec<u8>> = None;
    let mut p = 16;
    while p + page <= d.len() {
        let frame = &d[p..p + 8];
        let data = &d[p + 8..p + page];
        let used = u16::from_le_bytes([frame[4], frame[5]]) as usize;
        if frame[..4] == [0xff; 4] {
            let mut rec = open.take().unwrap_or_default();
            rec.extend_from_slice(&data[..used.min(body)]);
            out.push(rec);
        } else if used == body && frame[6..8] == [1, 0] {
            if let Some(rec) = open.take() {
                out.push(rec);
            }
            open = Some(data.to_vec());
        } else if used == body
            && frame[6..8] == [0, 0]
            && let Some(rec) = open.as_mut()
        {
            rec.extend_from_slice(data);
        }
        p += page;
    }
    if let Some(rec) = open {
        out.push(rec);
    }
    out
}

/// A property as a schema declares it.
#[derive(Clone, Debug)]
struct Declared {
    kind: String,
    id: String,
    serialized: bool,
    multiple: bool,
    connectable: bool,
    unit: bool,
}

/// A schema: its own properties and the schema it is based on.
#[derive(Clone, Debug)]
struct Schema {
    uid: String,
    base: Option<String>,
    properties: Vec<Declared>,
}

const PROPERTY_KINDS: [&str; 11] = [
    "Boolean",
    "Integer",
    "Choice",
    "Float",
    "Distance",
    "String",
    "Uuid",
    "URL",
    "Color",
    "Reference",
    "TextureURI",
];

impl Schema {
    fn parse(xml: &str) -> Option<Schema> {
        let mut uid = None;
        let mut base = None;
        let mut properties = Vec::new();
        for (name, attrs) in xml_elements(xml) {
            let get = |k: &str| attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
            match name.as_str() {
                "UID" => uid = get("val").map(str::to_owned),
                "Base" => base = get("val").map(str::to_owned),
                kind if PROPERTY_KINDS.contains(&kind) => {
                    let Some(id) = get("id") else { continue };
                    let yes = |k: &str| get(k) == Some("true");
                    properties.push(Declared {
                        kind: kind.to_owned(),
                        id: id.to_owned(),
                        serialized: !yes("readonly") && !yes("definitionIteratorData"),
                        multiple: yes("allowmultiplevalues"),
                        connectable: get("allowconnectedassets").is_some(),
                        unit: get("unit").is_some(),
                    });
                }
                _ => {}
            }
        }
        Some(Schema {
            uid: uid?,
            base,
            properties,
        })
    }
}

/// The start tags of an XML text: element name and attributes (values
/// unescaped). Enough for the schemas, which are flat lists of empty
/// elements.
fn xml_elements(xml: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(lt) = xml[i..].find('<').map(|k| i + k) {
        let Some(gt) = xml[lt..].find('>').map(|k| lt + k) else {
            break;
        };
        i = gt + 1;
        let tag = &xml[lt + 1..gt];
        if tag.starts_with('/') || tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        let tag = tag.trim_end_matches('/');
        let name_end = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
        let name = tag[..name_end].to_owned();
        let mut attrs = Vec::new();
        let mut rest = &tag[name_end..];
        while let Some(eq) = rest.find('=') {
            let key = rest[..eq].trim().to_owned();
            let after = rest[eq + 1..].trim_start();
            let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                break;
            };
            let Some(close) = after[1..].find(q) else {
                break;
            };
            attrs.push((key, unescape(&after[1..1 + close])));
            rest = &after[close + 2..];
        }
        out.push((name, attrs));
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The serialized properties of a schema and those it is based on, in
/// their order in a record.
fn closure(schemas: &HashMap<String, Schema>, uid: &str) -> Result<Vec<Declared>, String> {
    let mut chain = Vec::new();
    let mut at = Some(uid.to_owned());
    while let Some(u) = at {
        if chain.len() > 32 {
            return Err(format!("schema {uid}: its bases loop"));
        }
        let s = schemas
            .get(&u)
            .ok_or_else(|| format!("schema {u} is not in the package"))?;
        chain.push(s);
        at = s.base.clone();
    }
    let mut by_id: BTreeMap<String, Declared> = BTreeMap::new();
    // The base first, so a derived schema's property replaces it.
    for s in chain.iter().rev() {
        for p in &s.properties {
            by_id.insert(p.id.clone(), p.clone());
        }
    }
    Ok(by_id.into_values().filter(|p| p.serialized).collect())
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.p.checked_add(n).filter(|&e| e <= self.d.len());
        let end = end.ok_or_else(|| format!("record ends at {} (needs {n} more)", self.p))?;
        let s = &self.d[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f64(&mut self) -> Result<f64, String> {
        let b = self.take(8)?;
        Ok(f64::from_le_bytes(b.try_into().expect("8 bytes")))
    }
    fn str(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        if n > 1 << 20 {
            return Err(format!("string of {n} bytes at {}", self.p - 4));
        }
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn count(&mut self) -> Result<usize, String> {
        let n = self.u32()? as usize;
        if n > self.d.len() {
            return Err(format!("count {n} at {}", self.p - 4));
        }
        Ok(n)
    }
}

fn value(r: &mut Reader, p: &Declared) -> Result<Value, String> {
    Ok(match p.kind.as_str() {
        "Boolean" => Value::Bool(r.u8()? != 0),
        "Integer" | "Choice" => Value::Int(i64::from(r.u32()? as i32)),
        "Float" if p.unit => {
            let unit = r.u32()?;
            Value::Quantity {
                unit,
                value: r.f64()?,
            }
        }
        "Float" => Value::Float(r.f64()?),
        "Distance" => {
            let unit = r.u32()?;
            Value::Quantity {
                unit,
                value: r.f64()?,
            }
        }
        "String" | "Uuid" | "URL" => Value::Text(r.str()?),
        "Color" => Value::Color([r.f64()?, r.f64()?, r.f64()?, r.f64()?]),
        "Reference" => Value::None,
        "TextureURI" => {
            let kind = r.u8()?;
            let paths = match kind {
                0 => {
                    let n = r.count()?;
                    (0..n).map(|_| r.str()).collect::<Result<_, _>>()?
                }
                1 => vec![r.str()?],
                k => return Err(format!("{}: texture path kind {k}", p.id)),
            };
            Value::Paths { kind, paths }
        }
        k => return Err(format!("{}: property kind {k}", p.id)),
    })
}

fn property(r: &mut Reader, p: &Declared) -> Result<Property, String> {
    let value = if p.multiple && p.kind != "TextureURI" && p.kind != "Reference" {
        let n = r.count()?;
        Value::List((0..n).map(|_| value(r, p)).collect::<Result<_, _>>()?)
    } else {
        value(r, p)?
    };
    let mut connections = Vec::new();
    if p.connectable && r.u8()? != 0 {
        let flag = r.u8()?;
        if flag != 1 {
            return Err(format!("{}: connection flag {flag}", p.id));
        }
        let n = r.count()?;
        for _ in 0..n {
            connections.push(r.str()?);
        }
    }
    Ok(Property { value, connections })
}

/// An instance record: the asset with its properties.
fn instance(d: &[u8], schemas: &HashMap<String, Schema>) -> Result<Asset, String> {
    let mut r = Reader { d, p: 0 };
    let schema = r.str()?;
    let guid = r.str()?;
    let name = r.str()?;
    let library = r.str()?;
    let all: Vec<Declared> = closure(schemas, &schema)?
        .into_iter()
        .filter(|p| p.id != "AssetLibID")
        .collect();
    let start = r.p;
    let decode = |skip: Option<&str>| -> Result<BTreeMap<String, Property>, String> {
        let mut r = Reader { d, p: start };
        let mut out = BTreeMap::new();
        for p in all.iter().filter(|p| Some(p.id.as_str()) != skip) {
            let v = property(&mut r, p).map_err(|e| format!("{schema} {}: {e}", p.id))?;
            out.insert(p.id.clone(), v);
        }
        if r.p != d.len() {
            return Err(format!(
                "{schema} {guid}: the properties end at {} of {}",
                r.p,
                d.len()
            ));
        }
        Ok(out)
    };
    let properties = match decode(None) {
        Ok(p) => p,
        Err(first) => {
            // One map channel integer less (see the module documentation).
            ["texture_MapChannel_ID_Advanced", "texture_MapChannel"]
                .iter()
                .filter(|id| all.iter().any(|p| p.id == **id))
                .find_map(|id| decode(Some(id)).ok())
                .ok_or(first)?
        }
    };
    Ok(Asset {
        schema,
        guid,
        name,
        library,
        properties,
        ..Asset::default()
    })
}

/// A description record (`DefinitionIteratorProperties.bin`).
struct Description {
    schema: String,
    ui_name: String,
    asset: String,
    category: Option<String>,
    description: Option<String>,
}

fn description(d: &[u8]) -> Option<Description> {
    let mut r = Reader { d, p: 0 };
    let schema = r.str().ok()?;
    r.u8().ok()?;
    let ui_name = r.str().ok()?;
    let asset = r.str().ok()?;
    let version = r.u32().ok()?;
    // Version 0: the description; 1: group, description; 2: category,
    // group, description; 3: category, group, subgroup, description
    // (versions 0, 2 and 3 seen, in `.ipt` packages, mitcad#1367).
    let (category, description) = match version {
        0 => (None, Some(r.str().ok()?)),
        1 => {
            let _ = r.str().ok()?;
            (None, Some(r.str().ok()?))
        }
        2 | 3 => {
            let category = r.str().ok()?;
            for _ in 1..version {
                let _ = r.str().ok()?;
            }
            (Some(category), Some(r.str().ok()?))
        }
        _ => (None, None),
    };
    let nonempty = |s: Option<String>| s.filter(|s| !s.is_empty());
    Some(Description {
        schema,
        ui_name,
        asset,
        category: nonempty(category),
        description: nonempty(description),
    })
}

fn u32_at(d: &[u8], p: usize) -> Option<u32> {
    let b = d.get(p..p + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The packages of a document (`*.protein` entries), merged.
pub fn document_assets(file: &crate::F3dFile) -> Package {
    let mut out = Package::default();
    for e in file.entries() {
        if !e.name.ends_with(".protein") {
            continue;
        }
        match file
            .read(&e.name)
            .map_err(|e| e.to_string())
            .and_then(Package::read)
        {
            Ok(p) => out.extend(p),
            Err(err) => out.errors.push(format!("{}: {err}", e.name)),
        }
    }
    out
}

/// A paged stream of these records (page size 136), as a package keeps
/// them: for tests, here and in the `.ipt` reader's (mitcad#1367).
#[doc(hidden)]
pub fn paged_stream(recs: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&136u32.to_le_bytes());
    out.extend_from_slice(&[0xff; 8]);
    out.extend_from_slice(&[0; 4]);
    let mut page = 1u32;
    for rec in recs {
        let chunks: Vec<&[u8]> = rec.chunks(128).collect();
        for (k, c) in chunks.iter().enumerate() {
            let last = k + 1 == chunks.len() && c.len() < 128;
            if last {
                out.extend_from_slice(&[0xff; 4]);
                out.extend_from_slice(&(c.len() as u16).to_le_bytes());
                out.extend_from_slice(&[0, 0]);
            } else {
                out.extend_from_slice(&page.to_le_bytes());
                out.extend_from_slice(&128u16.to_le_bytes());
                out.extend_from_slice(&[u8::from(k == 0), 0]);
            }
            let mut body = c.to_vec();
            body.resize(128, 0xcd);
            out.extend_from_slice(&body);
            page += 1;
        }
        // A record that fills its last page ends on an empty one.
        if rec.len() % 128 == 0 {
            out.extend_from_slice(&[0xff; 4]);
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&[0xcd; 128]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn paged(recs: &[Vec<u8>]) -> Vec<u8> {
        paged_stream(recs)
    }

    #[test]
    fn pages_join_into_records() {
        let a: Vec<u8> = (0..200u8).collect();
        let b = vec![7u8; 128];
        let c = vec![9u8; 5];
        let d = paged(&[a.clone(), b.clone(), c.clone()]);
        assert_eq!(records(&d), vec![a, b, c]);
    }

    fn str32(s: &str) -> Vec<u8> {
        let mut out = (s.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(s.as_bytes());
        out
    }

    /// A package of one asset: its schema, values and description read
    /// back, the values in their id order, connections and units kept.
    #[test]
    fn reads_a_package() {
        let schema = r#"<?xml version="1.0"?><AssetSchema><UID val="TestSchema"/><String id="AssetLibID" val=""/><Integer id="SchemaVersion" readonly="true" val="2"/><String id="UIName" definitionIteratorData="true" val=""/><Color id="color" allowconnectedassets="single" valR="1" valG="1" valB="1" valA="1"/><Boolean id="a_flag" val="false"/><Float id="luminance" unit="CandelasPerSquareMeter" val="0"/><Distance id="depth" unit="MilliMeter" val="0.5"/><Reference id="normal" allowconnectedassets="single" val=""/><String id="words" allowmultiplevalues="true" val=""/><TextureURI id="image" allowmultiplevalues="true" val=""/></AssetSchema>"#;
        // Header, then by id: a_flag, color (+ connection), depth, image,
        // luminance, normal (connection only), words.
        let mut rec = Vec::new();
        for s in ["TestSchema", "GUID-1_Post2015", "Prism-001", "LIB"] {
            rec.extend(str32(s));
        }
        rec.push(1);
        for c in [0.5f64, 0.25, 0.125, 1.0] {
            rec.extend(c.to_le_bytes());
        }
        rec.extend([1, 1]);
        rec.extend(1u32.to_le_bytes());
        rec.extend(str32("TEX-1"));
        rec.extend(0x200eu32.to_le_bytes());
        rec.extend(3.75f64.to_le_bytes());
        rec.push(1);
        rec.extend(str32("oak.jpg"));
        rec.extend(0x37001u32.to_le_bytes());
        rec.extend(2.0f64.to_le_bytes());
        rec.push(0);
        rec.extend(2u32.to_le_bytes());
        rec.extend(str32("x"));
        rec.extend(str32("y"));
        let mut description = str32("TestSchema");
        description.push(0);
        for s in ["Custom", "Prism-001"] {
            description.extend(str32(s));
        }
        description.extend(2u32.to_le_bytes());
        for s in ["Paint/Glossy", "Default", "Paint - test"] {
            description.extend(str32(s));
        }
        description.extend(0u32.to_le_bytes());
        description.extend(0u32.to_le_bytes());
        let zip = crate::zip::stored_zip(&[
            ("F/Schemas/TestSchema.xml", schema.as_bytes()),
            ("F/AssetData/InstanceProperties.bin", &paged(&[rec])),
            (
                "F/AssetData/DefinitionIteratorProperties.bin",
                &paged(&[description]),
            ),
        ]);
        let p = Package::read(zip).unwrap();
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        let a = &p.assets[0];
        assert_eq!(
            (a.guid.as_str(), a.name.as_str(), a.library.as_str()),
            ("GUID-1_Post2015", "Prism-001", "LIB")
        );
        assert_eq!(a.ui_name.as_deref(), Some("Custom"));
        assert_eq!(a.description.as_deref(), Some("Paint - test"));
        assert_eq!(a.bool("a_flag"), Some(true));
        assert_eq!(a.color("color"), Some([0.5, 0.25, 0.125, 1.0]));
        assert_eq!(a.connections("color"), ["TEX-1"]);
        assert_eq!(a.millimetres("depth"), Some(3.75));
        assert_eq!(
            a.get("image"),
            Some(&Value::Paths {
                kind: 1,
                paths: vec!["oak.jpg".to_owned()]
            })
        );
        assert_eq!(a.float("luminance"), Some(2.0));
        assert_eq!(a.get("normal"), Some(&Value::None));
        assert_eq!(
            a.get("words"),
            Some(&Value::List(vec![
                Value::Text("x".into()),
                Value::Text("y".into())
            ]))
        );
        assert!(a.get("SchemaVersion").is_none() && a.get("UIName").is_none());
    }

    #[test]
    fn schema_closure_sorts_and_overrides() {
        let base = r#"<AssetSchema><UID val="B"/><String id="AssetLibID" val=""/><Integer id="z" val="1"/><Integer id="SchemaVersion" readonly="true" val="1"/><String id="hid" definitionIteratorData="true" val=""/></AssetSchema>"#;
        let top = r#"<AssetSchema><UID val="T"/><Base val="B"/><Color id="c" allowconnectedassets="single" valR="1" valG="1" valB="1" valA="1"/><Float id="f" unit="MilliMeter" val="0"/><PropertyAlias alias="x" id="z"/></AssetSchema>"#;
        let mut schemas = HashMap::new();
        for x in [base, top] {
            let s = Schema::parse(x).unwrap();
            schemas.insert(s.uid.clone(), s);
        }
        let ids: Vec<String> = closure(&schemas, "T")
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, ["AssetLibID", "c", "f", "z"]);
    }

    /// Descriptions of versions 0 (the description only) and 3 (category,
    /// group, subgroup, description), as `.ipt` packages keep them.
    #[test]
    fn reads_descriptions_of_other_versions() {
        let record = |version: u32, texts: &[&str]| {
            let mut d = str32("GenericSchema");
            d.push(0);
            for s in ["Shown", "Asset-1"] {
                d.extend(str32(s));
            }
            d.extend(version.to_le_bytes());
            for s in texts {
                d.extend(str32(s));
            }
            d.extend(0u32.to_le_bytes());
            d.extend(0u32.to_le_bytes());
            d
        };
        let v0 = description(&record(0, &["Metal."])).unwrap();
        assert_eq!(v0.category, None);
        assert_eq!(v0.description.as_deref(), Some("Metal."));
        let v3 = description(&record(
            3,
            &[
                "Metal/Steel",
                "Default",
                "Miscellaneous",
                "Generic material.",
            ],
        ))
        .unwrap();
        assert_eq!(v3.category.as_deref(), Some("Metal/Steel"));
        assert_eq!(v3.description.as_deref(), Some("Generic material."));
        assert_eq!(
            (v3.ui_name.as_str(), v3.asset.as_str()),
            ("Shown", "Asset-1")
        );
    }
}
