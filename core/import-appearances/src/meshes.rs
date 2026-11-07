// SPDX-License-Identifier: MIT
//! The physical materials and appearances of the file's mesh bodies
//! (mitcad#1376).
//!
//! A mesh body record names them in its two presentation envelopes, the
//! physical material and the appearance it brings, as a body record names
//! a body's (`_f3d.appearance.meshes`, joined to the mesh bodies by their
//! records' ids). They come in as a body's do (`appearances.rs`):
//! the appearance's asset as one document appearance, once per asset; the
//! material as MitCad's where it is one of MitCad's, listed in the report
//! either way. The timeline import applies them after the replay, after
//! the occurrences' overrides, which a mesh body keeps as a body does; the
//! import of the stored bodies alone through [`mesh_appearances`].

use std::collections::HashMap;

use mitcad_dump::{AppearanceData, AssetRef};
use mitcad_model::appearance;
use mitcad_model::{BodyUid, Document, Kernel};

use crate::appearances::{
    AppearanceEntry, AppearanceReport, MaterialEntry, asset_of, id_of, look, material_entry,
};

/// The document appearance of the asset a reference names: made once per
/// asset (`made`, by asset GUID), an equal one of an earlier import used
/// again, as the bodies' are.
fn appearance_id<K: Kernel>(
    doc: &mut Document<K>,
    data: &AppearanceData,
    made: &mut HashMap<String, Option<String>>,
    report: &mut AppearanceReport,
    r: &AssetRef,
) -> Option<String> {
    let a = asset_of(data, r)?;
    let key = a.id.clone()?;
    if let Some(id) = made.get(&key) {
        return id.clone();
    }
    let assets = data.assets.as_deref().unwrap_or(&[]);
    let mut l = look(a, assets);
    let base = id_of(&l.name);
    let mut out = None;
    for k in 1..1000 {
        let (id, name) = if k == 1 {
            (base.clone(), l.name.clone())
        } else {
            (format!("{base}_{k}"), format!("{} ({k})", l.name))
        };
        match doc.appearance(&id) {
            // The same appearance from an earlier import: used again.
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
                l.change.id = Some(id);
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
}

/// The materials and appearances of the mesh bodies `bodies` (each with
/// its mesh body record) that `data` names, counted in `report` with the
/// bodies' (`bodies`, `bodies_set`, `materials`, `materials_set`); the
/// materials join `materials`, the appearances made `made`. A mesh body
/// whose appearance an occurrence's override set keeps it. A record whose
/// mesh body did not come in is a gap.
pub(crate) fn apply<K: Kernel>(
    doc: &mut Document<K>,
    data: &AppearanceData,
    bodies: &[(u64, BodyUid)],
    made: &mut HashMap<String, Option<String>>,
    materials: &mut Vec<MaterialEntry>,
    report: &mut AppearanceReport,
) {
    let assets = data.assets.as_deref().unwrap_or(&[]);
    for m in data.meshes.iter().flatten() {
        let record = m.entity.unwrap_or(0);
        let uid = bodies
            .iter()
            .find(|(r, _)| Some(*r) == m.entity)
            .map(|&(_, uid)| uid)
            .filter(|&uid| doc.body_shape(uid).is_some());
        let why_none = "it did not come in (or is not in the design at the timeline marker)";
        if let Some(r) = &m.appearance {
            report.bodies += 1;
            match uid {
                None => report.gaps.push(format!("mesh body {record}: {why_none}")),
                // An occurrence's override covers it already.
                Some(uid) if doc.body_attributes(uid).appearance.is_some() => {
                    report.bodies_set += 1;
                }
                Some(uid) => match appearance_id(doc, data, made, report, r) {
                    Some(id) => match doc.set_body_appearance(uid, Some(&id)) {
                        Ok(()) => report.bodies_set += 1,
                        Err(e) => report.gaps.push(format!("mesh body {record}: {e}")),
                    },
                    None => report.gaps.push(format!(
                        "mesh body {record}: its appearance {} is not in the file's assets",
                        r.preset.as_deref().unwrap_or("")
                    )),
                },
            }
        }
        if let Some(r) = &m.material {
            report.materials += 1;
            let Some(asset) = asset_of(data, r) else {
                report.gaps.push(format!(
                    "mesh body {record}: its material {} is not in the file's assets",
                    r.preset.as_deref().unwrap_or("")
                ));
                continue;
            };
            let key = asset.id.clone().unwrap_or_default();
            let i = match materials.iter().position(|x| x.asset == key) {
                Some(i) => i,
                None => {
                    materials.push(material_entry(asset, assets));
                    materials.len() - 1
                }
            };
            materials[i].bodies += 1;
            match (uid, materials[i].mitcad.clone()) {
                (Some(uid), Some(id)) => match doc.set_body_material(uid, Some(&id)) {
                    Ok(()) => report.materials_set += 1,
                    Err(e) => report.gaps.push(format!("mesh body {record}: {e}")),
                },
                (None, _) => report
                    .gaps
                    .push(format!("mesh body {record}: its material: {why_none}")),
                // Not one of MitCad's: listed with its density.
                (Some(_), None) => {}
            }
        }
    }
}

/// The file's materials and appearances on the mesh bodies of an import of
/// the stored bodies alone (`import_f3d` without the timeline,
/// mitcad#1376): `bodies` are the mesh bodies brought in, each with its
/// mesh body record; the report as the timeline import's.
pub fn mesh_appearances<K: Kernel>(
    doc: &mut Document<K>,
    data: &AppearanceData,
    bodies: &[(u64, BodyUid)],
) -> AppearanceReport {
    let mut report = AppearanceReport::default();
    let mut made = HashMap::new();
    let mut materials = Vec::new();
    apply(doc, data, bodies, &mut made, &mut materials, &mut report);
    report.physical_materials = materials;
    report
}
