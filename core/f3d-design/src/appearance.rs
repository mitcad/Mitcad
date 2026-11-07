// SPDX-License-Identifier: MIT
//! Appearances and physical materials of bodies, faces and occurrences
//! (mitcad#228).
//!
//! The design stream keeps each body in a body record (class `D3937028`,
//! its object id the body's design entity `E`); the body map records
//! (classes `74CA4562` and `40DCC3F4`) pair entities with the blob that
//! holds the body and its selector there. The assets the records name are
//! in the document's packages ([`mitcad_f3d_file::protein`]). Layouts (verified on
//! the appearance variant models and the corpora, `core/import/docs/f3d-appearances.md`,
//! *Appearances and materials*):
//!
//! ```text
//! name     str16 token | str16 library | (u32 0 | str16 display name) |
//!          str16 preset
//!          token: the asset's GUID (an appearance's or a physical
//!          material's); library: BA5EE55E-… (appearances), C1EEA57C-…
//!          (physical materials and the appearances they bring)
//! body     … u8 1 | u32 n | n × face entry | … | str16 browser node GUID |
//!          … | material name | ref E′ (CD57BC48) | ref E + 1 |
//!          str16 body name | … | appearance name | f32 0 | f32 1 | …
//! face     str16 face GUID (lower case) | 4 × f32 RGBA | u8 1 | u8 1 |
//!          9 × u8 0 | u8 form | appearance name | f32 0 | f32 1
//! body map … u32 n | n × (u64 selector, u64 entity) | ref | 0–4 × u8 0 |
//!          str16 `BREP.<guid>.smbh` (or `.smb`) to the record's end
//! override … refs (component, occurrence) | 4 × f32 RGBA | u8 1 | u8 1 |
//!          10 × u8 | appearance name | f32 0 | f32 1   (class 10506394)
//! ```
//!
//! The face GUID is the string of the face's `NEUTRON_Material_attrib_def`
//! attribute in the blob ([`mitcad_f3d_names::NamedState::face_appearance_guid`]).
//! A body's selector is its record's key (the header's second field) where
//! a body of the blob has that key, else its position among the blob's
//! bodies.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};

use super::ir::{
    AppearanceData, AssetDef, AssetRef, BodyLook, FaceLook, Fingerprint, FingerprintF3d,
    OccurrenceLook,
};
use super::stream::{Segment, str16_at, u32_at, u64_at};
use mitcad_asm::asm::AsmFile;
use mitcad_f3d_file::container::F3dFile;
use mitcad_f3d_file::protein::{self, Package};
use mitcad_f3d_names::NamedState;

/// The appearance library's id.
pub const APPEARANCE_LIBRARY: &str = "BA5EE55E-9982-449B-9D66-9F036540E140";
/// The physical material library's id.
pub const MATERIAL_LIBRARY: &str = "C1EEA57C-3F56-45FC-B8CB-A9EC46A9994C";
/// The pair of ids that stands for an appearance without a library.
const UNNAMED_LIBRARY: [&str; 2] = [
    "08861000-1D69-CF2A-C082-CBD98E7E5D7F",
    "005E1000-55CE-AFB6-81A1-36E3EF077C5F",
];

const BODY_CLASS: &str = "D3937028-C20C-4E65-B010-94AD418A5C20";
const BODY_MAP_CLASSES: [&str; 2] = [
    "74CA4562-59F8-4C97-8AD1-F8297C21F9AA",
    "40DCC3F41D-E94F-B3B1-3E95-9B6ABE572C",
];
const OVERRIDE_CLASS: &str = "10506394-5BEB-4090-9B33-CF4AC5EBF26D";
/// Face references (a feature's face input; class `BFB592FE` of them
/// carries an appearance).
const FACE_REFERENCE_CLASS: &str = "5662F619-2281-4AD9-93DC-1FCFDA1062E7";

/// An asset name in an object's data.
#[derive(Clone, Debug, PartialEq)]
struct Name {
    start: usize,
    end: usize,
    token: String,
    library: String,
    display: Option<String>,
    preset: Option<String>,
}

/// A GUID, upper or lower case, with any number of `_Post2015` suffixes.
fn is_token(s: &str) -> bool {
    let mut base = s;
    while let Some(b) = base.strip_suffix("_Post2015") {
        base = b;
    }
    is_guid(base)
}

fn is_guid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn is_lower_guid(s: &str) -> bool {
    is_guid(s) && !s.chars().any(|c| c.is_ascii_uppercase())
}

/// The asset name starting at `p`, if one does.
fn name_at(d: &[u8], p: usize) -> Option<Name> {
    let (token, e) = str16_at(d, p)?;
    if !is_token(&token) {
        return None;
    }
    let (library, e) = str16_at(d, e)?;
    if library == UNNAMED_LIBRARY[0] {
        let e = if u32_at(d, e)? == 0 { e + 4 } else { e };
        let (second, e) = str16_at(d, e)?;
        return (second == UNNAMED_LIBRARY[1]).then_some(Name {
            start: p,
            end: e,
            token,
            library,
            display: None,
            preset: None,
        });
    }
    // A library's id: an upper-case GUID (the appearance and material
    // libraries, the generic one `314DE259-…`, older ones).
    if !is_guid(&library) || library.chars().any(|c| c.is_ascii_lowercase()) {
        return None;
    }
    let printable = |s: &str| !s.is_empty() && s.chars().all(|c| !c.is_control());
    if u32_at(d, e)? == 0 {
        let (preset, end) = str16_at(d, e + 4).filter(|(s, _)| printable(s))?;
        return Some(Name {
            start: p,
            end,
            token,
            library,
            display: None,
            preset: Some(preset),
        });
    }
    let (display, e) = str16_at(d, e).filter(|(s, _)| printable(s))?;
    let (preset, end) = str16_at(d, e).filter(|(s, _)| printable(s))?;
    Some(Name {
        start: p,
        end,
        token,
        library,
        display: Some(display),
        preset: Some(preset),
    })
}

/// Every asset name in an object's data, in order.
fn names_in(d: &[u8]) -> Vec<Name> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 8 < d.len() {
        // A token is 36 to a few hundred UTF-16 characters.
        if u32_at(d, p).is_some_and(|n| (36..=400).contains(&n))
            && d.get(p + 5) == Some(&0)
            && let Some(name) = name_at(d, p)
        {
            p = name.end;
            out.push(name);
            continue;
        }
        p += 1;
    }
    out
}

/// A face entry ending right before the name `n`: the face GUID and the
/// stored colour.
fn face_entry(d: &[u8], n: &Name) -> Option<(String, [f64; 4])> {
    let p = n.start.checked_sub(28)?;
    let tail = &d[p + 16..n.start];
    if tail[..2] != [1, 1] || tail[2..11].iter().any(|&b| b != 0) || tail[11] > 1 {
        return None;
    }
    let mut color = [0.0; 4];
    for (k, c) in color.iter_mut().enumerate() {
        let b = &d[p + 4 * k..p + 4 * k + 4];
        let v = f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        if !(0.0..=1.0).contains(&v) {
            return None;
        }
        *c = v;
    }
    let start = p.checked_sub(76)?;
    let (guid, end) = str16_at(d, start)?;
    (end == p && is_lower_guid(&guid)).then_some((guid, color))
}

/// The body map pairs of a map record: (selector, entity) and the blob.
fn body_map(seg: &Segment, d: &[u8]) -> Option<(Vec<(u64, u64)>, String)> {
    // The blob's name ends the record.
    let mut start = None;
    for p in (0..d.len().saturating_sub(8)).rev() {
        if let Some((s, e)) = str16_at(d, p)
            && e == d.len()
            && s.starts_with("BREP.")
        {
            start = Some((p, s));
            break;
        }
    }
    let (name_at, blob) = start?;
    let header = super::stream::header_end(d)?;
    for q in header..name_at.min(header + 40) {
        let Some(n) = u32_at(d, q).map(|n| n as usize) else {
            continue;
        };
        let after = q + 4 + 16 * n;
        // A reference, then up to four zero bytes before the name.
        let Some(r) = seg.ref_at(d, after) else {
            continue;
        };
        if r.end > name_at || name_at - r.end > 4 || d[r.end..name_at].iter().any(|&b| b != 0) {
            continue;
        }
        let pairs = (0..n)
            .map(|k| Some((u64_at(d, q + 4 + 16 * k)?, u64_at(d, q + 12 + 16 * k)?)))
            .collect::<Option<Vec<_>>>()?;
        return Some((pairs, blob));
    }
    None
}

/// An asset's properties as JSON.
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

/// The asset a name names: the token's own asset, or one with the token
/// and `_Post2015` suffixes (the appearance kept for the newer renderer
/// beside the older one under the token itself); the one whose name is
/// the preset first, then the newest.
fn find_asset<'a>(package: &'a Package, n: &Name) -> Option<&'a protein::Asset> {
    let token = n.token.to_ascii_uppercase();
    let mut fits: Vec<&protein::Asset> = package
        .assets
        .iter()
        .filter(|a| {
            let g = a.guid.to_ascii_uppercase();
            let mut base = g.as_str();
            while let Some(b) = base.strip_suffix("_POST2015") {
                base = b;
            }
            g == token || (base == token && g.len() > token.len())
        })
        .collect();
    fits.sort_by_key(|a| {
        (
            Some(a.name.as_str()) != n.preset.as_deref(),
            std::cmp::Reverse(a.guid.len()),
        )
    });
    fits.first().copied()
}

fn asset_ref(package: &Package, n: &Name) -> AssetRef {
    AssetRef {
        token: Some(n.token.clone()),
        library: Some(n.library.clone()),
        preset: n.preset.clone(),
        display: n.display.clone(),
        asset: find_asset(package, n).map(|a| a.guid.clone()),
        ..AssetRef::default()
    }
}

/// Whether a name names a physical material: its asset is one, or (no
/// asset) it is a material library preset of the form `PrismMaterial-<n>`.
fn is_material(package: &Package, n: &Name) -> bool {
    match find_asset(package, n) {
        Some(a) => a.schema == "PhysMatSchema",
        None => {
            n.library == MATERIAL_LIBRARY
                && n.preset.as_deref().is_some_and(|p| {
                    p.strip_prefix("PrismMaterial-")
                        .is_some_and(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_digit()))
                })
        }
    }
}

/// The body's name: the first printable string after the reference to
/// entity `E + 1`.
fn body_name(seg: &Segment, d: &[u8], entity: u64) -> Option<String> {
    let (_, r) = seg
        .refs_in(d, 0, d.len())
        .into_iter()
        .find(|(_, r)| r.id == entity + 1)?;
    str16_at(d, r.end).map(|(s, _)| s).filter(|s| !s.is_empty())
}

/// The body record's class version whose light bulb is read
/// ([`light_bulb`]): the layout was checked on it only.
const LIGHT_BULB_VERSION: u32 = 19;

/// A body record's light bulb (mitcad#1175): after its browser node GUID,
/// `f64 volume (cm³) | f64 | u32 | u32 1 | u8 0 | u32 | u8 1 | u8 light
/// bulb` (1 on, 0 off). Checked against the display scene the files keep
/// (in a sample of the corpora, of the version-19 records with a light
/// bulb byte, 1 of 54 off had a mesh there against 119 of 188 on, the
/// others not meshed: in hidden occurrences, not in the design) and against
/// a reported design whose preview shows the body hidden
/// (`docs/f3d-appearances.md`, *Bodies' light bulbs*). None when the record
/// is not laid out so.
fn light_bulb(d: &[u8]) -> Option<bool> {
    (0..d.len().saturating_sub(4)).find_map(|p| {
        if u32_at(d, p)? != 36 {
            return None;
        }
        let (_, e) = str16_at(d, p).filter(|(s, _)| is_lower_guid(s))?;
        let tail = d.get(e + 16..e + 31)?;
        if u32_at(tail, 4)? != 1 || tail[8] != 0 || tail[13] != 1 {
            return None;
        }
        match tail[14] {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    })
}

/// The design stream's part: the bodies' and occurrences' appearances and
/// materials, with the assets they name. The faces and the bodies'
/// records in their blobs are found by [`resolve`].
pub fn decode(seg: &Segment, doc: &F3dFile) -> AppearanceData {
    let package = protein::document_assets(doc);
    let mut warnings: Vec<String> = package
        .errors
        .iter()
        .map(|e| format!("assets: {e}"))
        .collect();
    // The body maps: entity -> (blob, selector); the history blobs' maps
    // first.
    let mut maps: HashMap<u64, (String, u64)> = HashMap::new();
    for class in BODY_MAP_CLASSES {
        for o in seg.objects_of(class) {
            let Some((pairs, blob)) = body_map(seg, seg.data(o)) else {
                continue;
            };
            for (selector, entity) in pairs {
                maps.entry(entity)
                    .or_insert_with(|| (blob.clone(), selector));
            }
        }
    }
    let mut used: Vec<String> = Vec::new();
    let mut note = |r: &AssetRef| {
        if let Some(a) = &r.asset
            && !used.contains(a)
        {
            used.push(a.clone());
        }
    };
    let mut bodies = Vec::new();
    for o in seg.objects_of(BODY_CLASS) {
        let d = seg.data(o);
        let names = names_in(d);
        if names.is_empty() {
            continue;
        }
        let mut body = BodyLook {
            entity: Some(o.id),
            name: body_name(seg, d, o.id),
            light_bulb: seg
                .version(o)
                .filter(|&v| v == LIGHT_BULB_VERSION)
                .and_then(|_| light_bulb(d)),
            ..BodyLook::default()
        };
        if let Some((blob, selector)) = maps.get(&o.id) {
            body.blob = Some(blob.clone());
            body.selector = Some(*selector);
        }
        let mut faces = Vec::new();
        let mut looks = Vec::new();
        for n in &names {
            if let Some((guid, color)) = face_entry(d, n) {
                let appearance = asset_ref(&package, n);
                note(&appearance);
                faces.push(FaceLook {
                    guid: Some(guid),
                    appearance: Some(appearance),
                    color: Some(color),
                    ..FaceLook::default()
                });
            } else if is_material(&package, n) && body.material.is_none() {
                let m = asset_ref(&package, n);
                note(&m);
                body.material = Some(m);
            } else {
                looks.push(n);
            }
        }
        match looks[..] {
            [n] => {
                let a = asset_ref(&package, n);
                note(&a);
                body.appearance = Some(a);
            }
            [] => {}
            _ => warnings.push(format!(
                "body {}: {} appearance names, none taken",
                o.id,
                looks.len()
            )),
        }
        if !faces.is_empty() {
            body.faces = Some(faces);
        }
        bodies.push(body);
    }
    // Face references that carry an appearance: the name, f32 0, f32 1
    // and the face's GUID.
    let mut faces = Vec::new();
    for o in seg.objects_of_kind(FACE_REFERENCE_CLASS) {
        let d = seg.data(o);
        for n in names_in(d) {
            let Some((guid, _)) = str16_at(d, n.end + 8).filter(|(g, _)| is_lower_guid(g)) else {
                continue;
            };
            let color = (n.start >= 28)
                .then(|| {
                    let p = n.start - 28;
                    let mut c = [0.0; 4];
                    for (k, x) in c.iter_mut().enumerate() {
                        let b = &d[p + 4 * k..p + 4 * k + 4];
                        *x = f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                    }
                    c
                })
                .filter(|c| c.iter().all(|x| (0.0..=1.0).contains(x)));
            let appearance = asset_ref(&package, &n);
            note(&appearance);
            faces.push(FaceLook {
                guid: Some(guid),
                appearance: Some(appearance),
                color,
                object_id: Some(o.id),
                ..FaceLook::default()
            });
        }
    }
    let mut occurrences = Vec::new();
    for o in seg.objects_of(OVERRIDE_CLASS) {
        let d = seg.data(o);
        let names = names_in(d);
        let [n] = &names[..] else {
            continue;
        };
        let refs = seg.refs_in(d, 0, n.start);
        let class_of = |id: u64| seg.guid_of(id).unwrap_or("");
        let component = refs
            .iter()
            .find(|(_, r)| class_of(r.id).starts_with("E03784ED"))
            .map(|(_, r)| r.id);
        let occurrence = refs
            .iter()
            .find(|(_, r)| class_of(r.id).starts_with("CE2913AA"))
            .map(|(_, r)| r.id);
        // Without an occurrence: the override of a component itself (the
        // root's: the whole design).
        if occurrence.is_none() && component.is_none() {
            warnings.push(format!(
                "appearance override {}: no occurrence or component",
                o.id
            ));
            continue;
        }
        let appearance = asset_ref(&package, n);
        note(&appearance);
        occurrences.push(OccurrenceLook {
            object_id: occurrence,
            component,
            appearance: Some(appearance),
            ..OccurrenceLook::default()
        });
    }
    // The mesh bodies' presentation envelopes (mitcad#1376): the physical
    // material and the appearance it brings, named as a body's are.
    let mut meshes = Vec::new();
    for o in seg.objects_of(super::classes::MESH_BODY) {
        let mut look = BodyLook {
            entity: Some(o.id),
            ..BodyLook::default()
        };
        let mut looks = Vec::new();
        for n in names_in(seg.data(o)) {
            if look.material.is_none() && is_material(&package, &n) {
                let m = asset_ref(&package, &n);
                note(&m);
                look.material = Some(m);
            } else {
                looks.push(n);
            }
        }
        match &looks[..] {
            [n] => {
                let a = asset_ref(&package, n);
                note(&a);
                look.appearance = Some(a);
            }
            [] => {}
            _ => warnings.push(format!(
                "mesh body {}: {} appearance names, none taken",
                o.id,
                looks.len()
            )),
        }
        if look.material.is_some() || look.appearance.is_some() {
            meshes.push(look);
        }
    }
    // The assets named, their textures, and their materials' aspects.
    let mut k = 0;
    while k < used.len() {
        if let Some(a) = package.by_guid(&used[k]) {
            for p in a.properties.values() {
                for c in &p.connections {
                    if package.by_guid(c).is_some() && !used.contains(c) {
                        used.push(c.clone());
                    }
                }
            }
        }
        k += 1;
    }
    let assets = used
        .iter()
        .filter_map(|g| package.by_guid(g))
        .map(asset_def)
        .collect();
    AppearanceData {
        assets: Some(assets),
        bodies: Some(bodies),
        faces: (!faces.is_empty()).then_some(faces),
        occurrences: (!occurrences.is_empty()).then_some(occurrences),
        warnings: (!warnings.is_empty()).then_some(warnings),
        meshes: (!meshes.is_empty()).then_some(meshes),
        ..AppearanceData::default()
    }
}

/// The geometry of each face of `faces` ([`super::inputs::fill_face`]),
/// on up to `threads` threads, each face on its own (the same with any
/// number).
fn fill_faces(
    state: &NamedState,
    faces: &[usize],
    threads: usize,
) -> HashMap<usize, Result<Fingerprint, String>> {
    let one = |f: usize| {
        let mut fp = Fingerprint {
            f3d: Some(FingerprintF3d::default()),
            ..Fingerprint::default()
        };
        super::inputs::fill_face(state, f, &mut fp).map(|()| fp)
    };
    let threads = threads.clamp(1, faces.len().max(1));
    if threads == 1 {
        return faces.iter().map(|&f| (f, one(f))).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out = std::sync::Mutex::new(HashMap::new());
    let work = || {
        loop {
            let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let Some(&f) = faces.get(k) else {
                break;
            };
            let filled = one(f);
            out.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(f, filled);
        }
    };
    std::thread::scope(|scope| {
        for _ in 1..threads {
            // (One that does not start leaves its faces to the others.)
            let _ = std::thread::Builder::new()
                .name("mitcad-import-appearances".to_owned())
                .stack_size(64 << 20)
                .spawn_scoped(scope, work);
        }
        work();
    });
    out.into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The part in the blobs: each body's record in its blob, and the faces
/// with appearances of their own, found by their GUIDs in the blob's
/// stored state. `threads`: how many threads work out the faces' points
/// (the same result with any number).
pub fn resolve(data: &mut AppearanceData, doc: &F3dFile, threads: usize) {
    let Some(bodies) = data.bodies.as_mut() else {
        return;
    };
    let mut warnings = Vec::new();
    // The face references, once per face: where several name one face, they
    // must agree.
    let mut loose: Vec<FaceLook> = Vec::new();
    for f in data.faces.take().unwrap_or_default() {
        let asset = |f: &FaceLook| f.appearance.as_ref().and_then(|a| a.asset.clone());
        match loose.iter_mut().find(|l| l.guid == f.guid) {
            None => loose.push(f),
            Some(l) if asset(l) == asset(&f) => {}
            Some(l) => {
                if l.why.is_none() {
                    warnings.push(format!(
                        "face {}: face references name different appearances",
                        f.guid.as_deref().unwrap_or("")
                    ));
                }
                l.why = Some("face references name different appearances".to_owned());
            }
        }
    }
    let mut wanted: Vec<String> = bodies.iter().filter_map(|b| b.blob.clone()).collect();
    wanted.sort();
    wanted.dedup();
    for blob in wanted {
        let Some(entry) = doc
            .body_blobs()
            .into_iter()
            .find(|b| b.entry.rsplit('/').next() == Some(blob.as_str()))
        else {
            warnings.push(format!("{blob}: not in the file"));
            continue;
        };
        let Some(file) = doc
            .read_large(&entry.entry)
            .ok()
            .and_then(|d| AsmFile::parse(&d).ok())
        else {
            warnings.push(format!("{blob}: does not read"));
            continue;
        };
        let state = NamedState::new(&file, None);
        let keys: Vec<Option<i64>> = (0..state.bodies.len()).map(|b| state.body_key(b)).collect();
        let tops: Vec<usize> = mitcad_asm::convert::body_records(&file)
            .into_iter()
            .filter(|&(_, top)| top)
            .map(|(r, _)| r)
            .collect();
        let named = bodies
            .iter()
            .filter(|b| b.blob.as_deref() == Some(blob.as_str()))
            .filter_map(|b| b.selector)
            .max()
            .map_or(0, |s| s as usize + 1);
        let skip = tops.len().saturating_sub(named);
        // Every face that carries a GUID (copies of a face, and the pieces
        // of a split one, carry its GUID too).
        let mut faces_by_guid: HashMap<String, Vec<usize>> = HashMap::new();
        let needs_faces = !loose.is_empty()
            || bodies
                .iter()
                .any(|b| b.blob.as_deref() == Some(blob.as_str()) && b.faces.is_some());
        if needs_faces {
            for f in 0..state.faces.len() {
                if let Some(g) = state.face_appearance_guid(f) {
                    faces_by_guid.entry(g).or_default().push(f);
                }
            }
        }
        // The geometry of every face carrying a GUID named here, worked
        // out on several threads (mitcad#311: a point inside a spline face
        // takes milliseconds, and a design's appearances can name
        // thousands of them).
        let named: std::collections::HashSet<&str> = bodies
            .iter()
            .filter(|b| b.blob.as_deref() == Some(blob.as_str()))
            .flat_map(|b| b.faces.iter().flatten())
            .chain(loose.iter())
            .filter_map(|f| f.guid.as_deref())
            .collect();
        let mut wanted: Vec<usize> = named
            .iter()
            .filter_map(|g| faces_by_guid.get(*g))
            .flatten()
            .copied()
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        let filled = fill_faces(&state, &wanted, threads);
        // The geometry of the faces of `body` carrying a GUID.
        let fill = |guid: &str, body: usize| -> Result<Vec<Fingerprint>, String> {
            let Some(all) = faces_by_guid.get(guid) else {
                return Err("no face of the blob carries its GUID".to_owned());
            };
            let mut out = Vec::new();
            for &f in all.iter().filter(|&&f| state.faces[f].body == body) {
                out.push(filled[&f].clone()?);
            }
            if out.is_empty() {
                return Err("only other bodies' faces carry its GUID".to_owned());
            }
            Ok(out)
        };
        for b in bodies
            .iter_mut()
            .filter(|b| b.blob.as_deref() == Some(blob.as_str()))
        {
            let Some(selector) = b.selector else { continue };
            let keyed: Vec<usize> = (0..keys.len())
                .filter(|&k| keys[k] == i64::try_from(selector).ok())
                .collect();
            let index = match keyed[..] {
                [k] => Some(k),
                // Else the position among the blob's design bodies: its
                // last top-level bodies, as many as the body map names (a
                // snapshot blob saves the faces the design refers to as
                // bodies before them).
                [] => usize::try_from(selector)
                    .ok()
                    .and_then(|k| tops.get(skip + k))
                    .and_then(|&r| state.bodies.iter().position(|b| b.record == r)),
                _ => None,
            };
            let Some(index) = index else {
                warnings.push(format!(
                    "body {}: selector {selector} names no body of {blob}",
                    b.entity.unwrap_or(0)
                ));
                continue;
            };
            b.record = Some(state.bodies[index].record as u64);
            for face in b.faces.iter_mut().flatten() {
                let Some(guid) = face.guid.as_deref() else {
                    continue;
                };
                match fill(guid, index) {
                    Ok(f) => face.faces = Some(f),
                    Err(e) => face.why = Some(e),
                }
            }
        }
        // The face references' faces in this blob, to their bodies (each
        // body with a face carrying the GUID).
        for l in loose.iter_mut().filter(|l| l.why.is_none()) {
            let Some(all) = l.guid.as_deref().and_then(|g| faces_by_guid.get(g)) else {
                continue;
            };
            let mut owners: Vec<usize> = all.iter().map(|&f| state.faces[f].body).collect();
            owners.sort_unstable();
            owners.dedup();
            for owner in owners {
                let record = state.bodies[owner].record as u64;
                let Some(b) = bodies
                    .iter_mut()
                    .find(|b| b.blob.as_deref() == Some(blob.as_str()) && b.record == Some(record))
                else {
                    continue;
                };
                let faces = b.faces.get_or_insert_with(Vec::new);
                if faces.iter().any(|x| x.guid == l.guid) {
                    // Its body's record names it already.
                    l.faces = Some(Vec::new());
                    continue;
                }
                let mut look = FaceLook {
                    faces: None,
                    ..l.clone()
                };
                match fill(l.guid.as_deref().unwrap_or(""), owner) {
                    Ok(f) => look.faces = Some(f),
                    Err(e) => look.why = Some(e),
                }
                faces.push(look);
                l.faces = Some(Vec::new());
            }
        }
    }
    // The face references whose faces were not found.
    let left: Vec<FaceLook> = loose
        .into_iter()
        .filter(|l| l.faces.is_none())
        .map(|mut l| {
            l.why
                .get_or_insert_with(|| "no face of the stored design carries its GUID".to_owned());
            l
        })
        .collect();
    data.faces = (!left.is_empty()).then_some(left);
    if !warnings.is_empty() {
        data.warnings.get_or_insert_with(Vec::new).extend(warnings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s16(s: &str) -> Vec<u8> {
        let mut out = (s.encode_utf16().count() as u32).to_le_bytes().to_vec();
        for u in s.encode_utf16() {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out
    }

    fn name(token: &str, library: &str, preset: &str) -> Vec<u8> {
        let mut d = s16(token);
        d.extend(s16(library));
        d.extend(0u32.to_le_bytes());
        d.extend(s16(preset));
        d
    }

    const TOKEN: &str = "23303B60-6683-46D0-BC5B-B92F9F3CA4E8";
    const FACE: &str = "0438b4bf-d97c-4f0d-9ef2-713f87b07603";

    #[test]
    fn reads_names_and_face_entries() {
        let mut d = vec![7u8; 5];
        d.extend(s16(FACE));
        for c in [0.5f32, 0.25, 0.0, 1.0] {
            d.extend(c.to_le_bytes());
        }
        d.extend([1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let at = d.len();
        d.extend(name(TOKEN, APPEARANCE_LIBRARY, "Prism-093"));
        d.extend([0, 0, 0, 0, 0, 0, 0x80, 0x3f]);
        d.extend(name(
            "4177D984-5662-4715-B8AF-E5D65A04EDEE",
            MATERIAL_LIBRARY,
            "PrismMaterial-018",
        ));
        let names = names_in(&d);
        assert_eq!(names.len(), 2);
        assert_eq!(names[0].start, at);
        assert_eq!(names[0].preset.as_deref(), Some("Prism-093"));
        let (guid, color) = face_entry(&d, &names[0]).unwrap();
        assert_eq!(guid, FACE);
        assert_eq!(color, [0.5, 0.25, 0.0, 1.0]);
        assert!(face_entry(&d, &names[1]).is_none());
        assert!(is_material(&Package::default(), &names[1]));
        assert!(!is_material(&Package::default(), &names[0]));
    }

    /// A body record's light bulb (mitcad#1175): the byte after its
    /// browser node GUID, the volume and area and the fields between, as
    /// the reported drill index's two records keep it.
    #[test]
    fn reads_a_body_records_light_bulb() {
        let record = |count: u32, bulb: u8| {
            let mut d = vec![0u8; 9];
            d.extend(s16("0_668"));
            d.extend(s16("c8324a15-a37e-4b88-aa35-8f76d5ff9d25"));
            d.extend(656.949_641_729_297_6f64.to_le_bytes());
            d.extend(0.002_073_461f64.to_le_bytes());
            d.extend(count.to_le_bytes());
            d.extend(1u32.to_le_bytes());
            d.extend([0, 0xac, 0x67, 0x23, 0x00, 1, bulb]);
            d.extend((-1.0f64).to_le_bytes());
            d.extend(s16("e78a62d0-01b8-4352-bec6-55ec3ffa1976"));
            d
        };
        assert_eq!(light_bulb(&record(1, 0)), Some(false));
        assert_eq!(light_bulb(&record(1, 1)), Some(true));
        // The count before the 1 varies between bodies.
        assert_eq!(light_bulb(&record(7, 0)), Some(false));
        // Not laid out so: no light bulb.
        assert_eq!(light_bulb(&record(1, 2)), None);
        let mut other = record(1, 0);
        let at = other.len() - 8 - 4 - 72 - 2;
        other[at] = 9;
        assert_eq!(light_bulb(&other), None);
        assert_eq!(light_bulb(&s16(FACE)), None);
    }

    #[test]
    fn prefers_the_asset_named_by_the_preset() {
        let asset = |guid: &str, name: &str, schema: &str| protein::Asset {
            guid: guid.to_owned(),
            name: name.to_owned(),
            schema: schema.to_owned(),
            ..protein::Asset::default()
        };
        let package = Package {
            assets: vec![
                asset(TOKEN, "Generic", "GenericSchema"),
                asset(
                    &format!("{TOKEN}_Post2015"),
                    "Prism-093",
                    "PrismOpaqueSchema",
                ),
            ],
            errors: Vec::new(),
        };
        let n = Name {
            start: 0,
            end: 0,
            token: TOKEN.to_owned(),
            library: APPEARANCE_LIBRARY.to_owned(),
            display: None,
            preset: Some("Prism-093".to_owned()),
        };
        assert_eq!(
            find_asset(&package, &n).map(|a| a.name.as_str()),
            Some("Prism-093")
        );
    }
}
