// SPDX-License-Identifier: MIT
//! The `print_job` query (mitcad#2008): what a print job sends to a
//! slicer, before Send. Each body of the job with the material it prints
//! with (the job's choice, else its own or its component's, None when
//! neither), the job-level common value (or mixed), the delivery
//! (geometry only or a print project), its status (complete, partial or
//! receiver-owned) with what comes from the receiver's defaults, what the
//! destination takes, and the chosen settings it leaves out. With
//! `choose`, a choice in the list made by the rules of
//! `mitcad_model::print_files::choose`; a refused one leaves the materials
//! as they were and says why in `refused`, in plain words. See
//! `commands.md`, "Print jobs: geometry only or a print project".

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::exchange::ExchangeOps;
use mitcad_model::kernel::Kernel;
use mitcad_model::print_files::{self, Common};
use mitcad_model::print_keys::Dialect;
use mitcad_model::{BodyUid, Document, EffectivePrint, MaterialSource, PrintTarget};

/// The `print_job` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrintJobQuery {
    /// The job's bodies (uid or name), in the list's order.
    bodies: Vec<String>,
    /// The slicer (`bambu_studio`, `orcaslicer`, `qidi_studio`,
    /// `prusaslicer`, `cura`).
    dialect: String,
    /// The job's materials so far per body (uid or name): a material id or
    /// null ("None (slicer's own)"). Bodies not listed take the design's,
    /// made sendable (`print_files::prefill`).
    #[serde(default)]
    materials: BTreeMap<String, Option<String>>,
    /// A choice to make in the list.
    #[serde(default)]
    choose: Option<JobChoice>,
}

/// A choice in a job's material list.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JobChoice {
    /// One body (uid or name); Set to all when left out.
    #[serde(default)]
    body: Option<String>,
    /// A material id; null for "None (slicer's own)".
    material: Option<String>,
}

/// Answers the query.
pub(super) fn query<K: Kernel>(
    doc: &Document<K>,
    query: &PrintJobQuery,
) -> Result<Value, ApiError> {
    let dialect = Dialect::from_id(&query.dialect).ok_or_else(|| {
        ApiError(format!(
            "{} is no slicer: bambu_studio, orcaslicer, qidi_studio, prusaslicer or cura",
            query.dialect
        ))
    })?;
    let known = |id: &str| -> Result<(), ApiError> {
        match doc.material(id) {
            Some(_) => Ok(()),
            None => Err(ApiError(format!("there is no material {id}"))),
        }
    };
    let mut bodies: Vec<BodyUid> = Vec::with_capacity(query.bodies.len());
    for body in &query.bodies {
        let uid = doc.body_by_uid_or_name(body)?;
        if !bodies.contains(&uid) {
            bodies.push(uid);
        }
    }
    let mut given: BTreeMap<BodyUid, Option<String>> = BTreeMap::new();
    for (body, material) in &query.materials {
        if let Some(id) = material {
            known(id)?;
        }
        given.insert(doc.body_by_uid_or_name(body)?, material.clone());
    }
    // The design's material of a body: its own or its component's.
    let design = |uid: BodyUid| -> Option<String> {
        let (material, source) = doc.body_material(uid, None);
        (source != MaterialSource::Default).then(|| material.id.clone())
    };
    let listed = bodies.iter().all(|b| given.contains_key(b));
    let mut materials: Vec<Option<String>> = bodies
        .iter()
        .map(|b| match given.get(b) {
            Some(m) => m.clone(),
            None => design(*b),
        })
        .collect();
    // What the list shows is what is sent: a mixed job made sendable,
    // unless the caller gave every body's choice.
    if !listed {
        materials = print_files::prefill(materials);
    }
    let mut refused: Option<String> = None;
    if let Some(choice) = &query.choose {
        if let Some(id) = &choice.material {
            known(id)?;
        }
        let index =
            match &choice.body {
                None => None,
                Some(body) => {
                    let uid = doc.body_by_uid_or_name(body)?;
                    Some(bodies.iter().position(|b| *b == uid).ok_or_else(|| {
                        ApiError(format!("{body} is not one of the job's bodies"))
                    })?)
                }
            };
        match print_files::choose(&materials, index, choice.material.as_deref()) {
            Ok(chosen) => materials = chosen,
            Err(message) => refused = Some(message),
        }
    }
    let settings: Vec<EffectivePrint> = bodies
        .iter()
        .zip(&materials)
        .map(|(uid, material)| {
            let base = doc.effective_print(&PrintTarget::Body(*uid, None));
            doc.job_print(base, *uid, material.as_deref())
        })
        .collect();
    let refs: Vec<&EffectivePrint> = settings.iter().collect();
    let ready = print_files::readiness(dialect, &refs);
    let capabilities = print_files::capabilities(dialect);
    let listed: Vec<Value> = bodies
        .iter()
        .zip(&materials)
        .map(|(uid, material)| {
            json!({
                "body": uid,
                "name": doc.body_name(*uid),
                "material": material,
                "material_name": material.as_deref().and_then(|id| doc.material(id)).map(|m| m.name.clone()),
            })
        })
        .collect();
    let common = match print_files::common(&materials) {
        Common::Same(material) => json!({"material": material, "mixed": false}),
        Common::Mixed => json!({"material": null, "mixed": true}),
    };
    let first = settings.first();
    let profile = |chosen: Option<&String>| match chosen {
        Some(name) => json!({"chosen": true, "name": name}),
        None => json!({"chosen": false, "name": null}),
    };
    let mut out = json!({
        "dialect": dialect.id(),
        "delivery": ready.delivery.as_str(),
        "status": ready.status.as_str(),
        "bodies": listed,
        "common": common,
        "printer": profile(first.and_then(|s| s.printer_name.as_ref())),
        "process": profile(first.and_then(|s| s.process_name.as_ref())),
        "notes": ready.notes,
        "left_out": ready.left_out,
        "capabilities": {
            "project": capabilities.project,
            "printer": capabilities.printer,
            "materials": capabilities.materials,
            "sends": capabilities.sends,
            "note": capabilities.note,
        },
    });
    if let Some(message) = refused {
        out["refused"] = json!(message);
    }
    Ok(out)
}
