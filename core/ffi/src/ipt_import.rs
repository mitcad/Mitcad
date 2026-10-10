// SPDX-License-Identifier: MIT
//! The `.ipt` import (mitcad#60): `mitcad_ipt` reads the file, the `.f3d`
//! reader's ASM code converts the B-rep record's bodies, and OCCT builds
//! them.
//!
//! - With the history (stages 2 and 3, the default when the file has a
//!   definitions segment with features or parameters): the part's
//!   parameters and features as the import's dump IR
//!   (`mitcad_ipt::design`), replayed by `mitcad-import` against the ASM
//!   history of the B-rep record (`ipt_history.rs`), as the `.f3d` import
//!   replays a design.
//! - `bodies_only` (stage 1, and files without definitions): the bodies
//!   as base features, one per body (`build_brep_body`).
//!
//! Both set the document's units (an empty document only) and the
//! material. The `import_ipt` document command and `mitcad-cli
//! import-ipt`; with `reference`, the solids are compared with a STEP file
//! of the same part (volume and area of each solid).

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use mitcad_model::api::ApiError;
use mitcad_model::exchange::ReadOptions;
use mitcad_model::{BaseInput, BodyUid, ImportBody, Kernel};

use cxx::SharedPtr;

use crate::Document;
use crate::brep_import::{Source, to_ffi};
use crate::f3d_import::{
    HANG_RETRIES, Handover, MEMORY_POLL, MemoryGuard, TryEnd, TryThread, spawn_import,
};
use crate::kernel::exchange::ffi::{build_brep_body, catch_occt_crashes};
use crate::kernel::shape::ffi::new_shape_list;
use crate::kernel::{OcctKernel, Shape};

/// A part read once for all tries of a watched replay.
struct Prepared {
    file: String,
    options: Options,
    info: mitcad_ipt::DocumentInfo,
    stored: Stored,
    design: mitcad_ipt::design::Design,
    /// The meshes of mesh features (each try builds its own shapes).
    meshes: Vec<Result<mitcad_ipt::mesh::Mesh, String>>,
    warnings: Vec<String>,
    read_seconds: f64,
}

/// The part's B-rep records as every try of the replay uses them, read
/// once: the bodies of their history states and their lines of the report.
struct Stored {
    states: Arc<mitcad_ipt::states::States>,
    records: Vec<Value>,
}

impl Stored {
    fn new(
        records: &[mitcad_ipt::BrepRecord],
        results: &std::collections::BTreeMap<i64, i64>,
        rolled: bool,
    ) -> Result<Self, String> {
        let states = Arc::new(mitcad_ipt::states::States::new(records, results, rolled)?);
        let records = records
            .iter()
            .zip(mitcad_ipt::read_bodies(records))
            .map(|(record, blob)| match blob {
                Ok(blob) => json!({
                    "source": record.place(),
                    "asm_version": blob.header.asm_version,
                    "bodies": blob.bodies.len(),
                    "history_states": mitcad_ipt::history_states(&record.asm),
                    "truncated": blob.truncated,
                }),
                Err(e) => json!({"source": record.place(), "error": e}),
            })
            .collect();
        Ok(Self { states, records })
    }
}

/// What a try of the replay is told by the watchdog.
#[derive(Clone, Default)]
struct Try {
    /// Items the geometry kernel hung on before.
    hung: Vec<i64>,
    /// The final bodies by volume only (the kernel hung comparing them).
    no_compare: bool,
    /// The stored bodies without the timeline.
    bodies_only: bool,
    /// Items whose definitions gave no state in earlier tries.
    failed: Vec<(i64, String)>,
    progress: Option<Arc<mitcad_import::Progress>>,
}

/// The options of `import_ipt` (JSON).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    /// A readable report instead of JSON.
    #[serde(default)]
    text: bool,
    /// Also write the JSON report to this file.
    #[serde(default)]
    report_path: Option<String>,
    /// A STEP file of the same part to compare the solids with.
    #[serde(default)]
    reference: Option<String>,
    /// The largest relative difference of a solid's volume and area from
    /// its STEP counterpart that passes.
    #[serde(default = "default_relative")]
    max_relative: f64,
    /// Also measure the surface deviation from the STEP file (sampled; mm).
    #[serde(default)]
    deviation: bool,
    /// The bodies only, as base features, without parameters and history.
    #[serde(default)]
    bodies_only: bool,
    /// Do not check the replay against the ASM history.
    #[serde(default)]
    no_verify: bool,
    /// Leave out features that cannot be replayed instead of using the
    /// file's bodies.
    #[serde(default)]
    no_fallback: bool,
    /// Compare the final bodies by volume only.
    #[serde(default)]
    no_compare: bool,
    /// Seconds after which the remaining features take the file's bodies.
    #[serde(default)]
    time_limit: Option<f64>,
    /// Seconds without progress after which a geometry kernel call counts
    /// as hung: the replay runs again with the item it hung on taking the
    /// file's bodies (as the `.f3d` import's `hang_limit`, mitcad#82).
    #[serde(default)]
    hang_limit: Option<f64>,
    /// Also write the decoded design (the dump IR, JSON) to this file.
    #[serde(default)]
    dump_path: Option<String>,
    /// Replay this dump (JSON, as `dump_path` writes it) instead of the
    /// design decoded from the file (to try changes to it).
    #[serde(default)]
    design_path: Option<String>,
}

fn default_relative() -> f64 {
    1e-6
}

fn file_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

fn relative(a: f64, b: f64) -> f64 {
    let scale = a.abs().max(b.abs());
    if scale == 0.0 {
        0.0
    } else {
        (a - b).abs() / scale
    }
}

/// The Mitcad material whose id or name the file's material is (case
/// does not matter).
pub(crate) fn mitcad_material(name: &str) -> Option<&'static str> {
    let wanted = name.trim().to_lowercase();
    mitcad_model::analysis::MATERIALS
        .iter()
        .find(|m| m.id == wanted || m.name.to_lowercase() == wanted)
        .map(|m| m.id)
}

impl Document {
    fn import_ipt_report(&mut self, path: &str, options: &Options) -> Result<Value, ApiError> {
        let file = file_name(path);
        let fail = |e: &dyn std::fmt::Display| ApiError(format!("{file}: {e}"));
        // OCCT crashes in building a body are errors of that body.
        catch_occt_crashes();
        let started = std::time::Instant::now();
        let ipt = mitcad_ipt::IptFile::open(std::path::Path::new(path)).map_err(|e| fail(&e))?;
        let info = ipt.document();
        let records = ipt.brep_records().map_err(|e| fail(&e))?;
        let mut warnings: Vec<String> = ipt.warnings.clone();
        if !ipt.is_part() {
            warnings.push(format!(
                "the root storage's class is {{{}}}, not a part's",
                mitcad_ipt::cfb::guid_text(&ipt.container.root().clsid)
            ));
        }
        let design = if options.bodies_only {
            None
        } else {
            mitcad_ipt::design::read(&ipt, file)
                .map_err(|e| fail(&e))?
                .filter(|d| !d.items.is_empty() || d.parameters.model + d.parameters.user > 0)
        };
        let read_seconds = started.elapsed().as_secs_f64();
        if let Some(design) = design {
            let meshes = ipt.meshes().map_err(|e| fail(&e))?;
            return self.import_ipt_history(
                file,
                options,
                &info,
                &records,
                design,
                meshes,
                warnings,
                read_seconds,
            );
        }

        // Convert and build every body of every B-rep record.
        let mut list = new_shape_list();
        let mut found = Vec::new();
        let mut skipped = Vec::new();
        let mut sources = Vec::new();
        let mut record_reports = Vec::new();
        let mut wires: Vec<String> = Vec::new();
        for (record, blob) in records.iter().zip(mitcad_ipt::read_bodies(&records)) {
            let place = record.place();
            let blob = match blob {
                Ok(blob) => blob,
                Err(e) => {
                    skipped.push(json!({"source": place, "error": e}));
                    continue;
                }
            };
            record_reports.push(json!({
                "source": place,
                "asm_version": blob.header.asm_version,
                "bodies": blob.bodies.len(),
                "history_states": mitcad_ipt::history_states(&record.asm),
                "truncated": blob.truncated,
            }));
            if let Some(t) = &blob.truncated {
                warnings.push(format!("{place}: the B-rep data ends early: {t}"));
            }
            let source = Source {
                document: "",
                blob: &place,
                asm_version: &blob.header.asm_version,
                history: false,
                history_step: 0,
            };
            for body in blob.bodies.iter().filter(|b| b.top_level) {
                let place = format!("{place}/{}", body.record);
                // A body of wires only (no faces) is no body of the part's
                // result: the file lists none for it.
                if body.body.faces.is_empty() && body.skipped_faces == 0 {
                    wires.push(place);
                    continue;
                }
                let data = to_ffi(&source, body);
                let built = build_brep_body(&data, list.pin_mut());
                let index = list.size() - 1;
                if !built.built {
                    skipped.push(json!({"source": place, "error": built.error.to_string()}));
                    continue;
                }
                let t = body.tidied;
                found.push(json!({
                    "source": place, "solid": built.solid, "valid": built.valid,
                    "volume": built.volume, "raw_volume": built.raw_volume,
                    "area": built.area, "faces": built.faces,
                    "issues": built.issues, "messages": built.messages,
                    "tidied": {
                        "slits": t.spurs, "pinched_loops": t.pinches,
                    },
                }));
                sources.push((place, index));
            }
        }
        // The meshes of mesh features.
        for (place, built) in build_meshes(&ipt.meshes().map_err(|e| fail(&e))?) {
            match built {
                Ok((shape, report)) => {
                    list.pin_mut().push(shape);
                    found.push(report);
                    sources.push((place, list.size() - 1));
                }
                Err(e) => skipped.push(json!({"source": place, "error": e})),
            }
        }
        if sources.is_empty() && !skipped.is_empty() {
            return Err(ApiError(format!(
                "{file} has no bodies to import ({} could not be built)",
                skipped.len()
            )));
        }
        if !wires.is_empty() {
            warnings.push(format!(
                "not imported: {} bodies of wires only (no faces): {}",
                wires.len(),
                wires.join(", ")
            ));
        }
        if sources.is_empty() {
            warnings.push(
                "the part has no bodies: its B-rep record holds none and it has no mesh features"
                    .into(),
            );
        }
        let built_seconds = started.elapsed().as_secs_f64() - read_seconds;

        // One undo step: the base features, the units and the material.
        // The part's units only for a document that had nothing yet.
        let empty = self.0.features().next().is_none();
        let depth = self.0.undo_depth();
        let inputs = sources
            .iter()
            .map(|(place, index)| BaseInput {
                source: Some(format!("{file}: {place}")),
                ..BaseInput::new(vec![ImportBody {
                    name: None,
                    color: None,
                    shape: list.at(*index),
                }])
            })
            .collect();
        let label = format!("Import {file}");
        let imported = if sources.is_empty() {
            Vec::new()
        } else {
            self.0.add_base_features(&label, inputs)?
        };
        let units = self.apply_part_units(&info, empty, &mut warnings)?;
        let material = info.material.as_deref().and_then(mitcad_material);
        if let Some(id) = material {
            for body in imported.iter().flat_map(|f| &f.bodies) {
                self.0.set_body_material(body.uid, Some(id))?;
            }
        }
        let candidates: Vec<BodyUid> = imported
            .iter()
            .flat_map(|f| &f.bodies)
            .map(|b| b.uid)
            .collect();
        let hidden = self.hide_stored_hidden(&info.hidden_bodies, &candidates, &mut warnings)?;
        self.0.merge_undo(depth, &label);

        for (report, feature) in found.iter_mut().zip(&imported) {
            report["hidden"] = json!(feature.bodies.iter().any(|b| hidden.contains(&b.uid)));
            report["feature"] = json!(feature.feature.uid);
            report["bodies"] = json!(
                feature
                    .bodies
                    .iter()
                    .map(|b| json!({"uid": b.uid, "name": b.name, "kind": b.kind.as_str()}))
                    .collect::<Vec<_>>()
            );
        }
        let mut report = json!({
            "file": file,
            "format": "ipt",
            "part_number": info.part_number,
            "material": info.material,
            "mitcad_material": material,
            "release": info.release,
            "units": units,
            "unit_code": info.length_unit_code,
            "records": record_reports,
            "imported": found,
            "skipped": skipped,
            "warnings": warnings,
            "seconds": {"read": read_seconds, "build": built_seconds},
        });
        if let Some(step) = &options.reference {
            let solids: Vec<(BodyUid, String)> = imported
                .iter()
                .flat_map(|f| &f.bodies)
                .filter(|b| b.kind == mitcad_model::BodyKind::Solid)
                .map(|b| (b.uid, b.name.clone()))
                .collect();
            let valid = report["imported"]
                .as_array()
                .is_some_and(|a| a.iter().all(|b| b["valid"] == true || b["solid"] == false));
            report["reference"] = self.compare_reference(step, &solids, valid, options)?;
        }
        if let Some(path) = &options.report_path {
            let text = serde_json::to_string_pretty(&report).expect("the report serializes");
            std::fs::write(path, format!("{text}\n"))
                .map_err(|e| ApiError(format!("{path}: {e}")))?;
        }
        Ok(report)
    }

    /// The import with the part's parameters and features (see the module
    /// docs): one undo step with the units and the material.
    #[allow(clippy::too_many_arguments)]
    fn import_ipt_history(
        &mut self,
        file: &str,
        options: &Options,
        info: &mitcad_ipt::DocumentInfo,
        records: &[mitcad_ipt::BrepRecord],
        design: mitcad_ipt::design::Design,
        meshes: Vec<Result<mitcad_ipt::mesh::Mesh, String>>,
        warnings: Vec<String>,
        read_seconds: f64,
    ) -> Result<Value, ApiError> {
        let mut design = design;
        if let Some(path) = &options.design_path {
            let text =
                std::fs::read_to_string(path).map_err(|e| ApiError(format!("{path}: {e}")))?;
            design.dump = mitcad_f3d::design::ir::Dump::from_json(&text)
                .map_err(|e| ApiError(format!("{path}: {e}")))?;
        }
        if let Some(path) = &options.dump_path {
            let text = serde_json::to_string_pretty(&design.dump).expect("the dump serializes");
            std::fs::write(path, format!("{text}\n"))
                .map_err(|e| ApiError(format!("{path}: {e}")))?;
        }
        // The bodies of the history states, read once for all tries.
        let clock = std::time::Instant::now();
        let stored = Stored::new(records, &design.results, !design.items.is_empty())
            .map_err(|e| ApiError(format!("{file}: {e}")))?;
        let read_seconds = read_seconds + clock.elapsed().as_secs_f64();
        let fresh = self.0.features().next().is_none() && self.0.undo_depth() == 0;
        let value = match options.hang_limit.filter(|s| *s > 0.0) {
            Some(limit) if fresh => {
                let prepared = Arc::new(Prepared {
                    file: file.to_owned(),
                    options: options.clone(),
                    info: info.clone(),
                    stored,
                    design,
                    meshes,
                    warnings,
                    read_seconds,
                });
                self.import_ipt_watched(prepared, limit)?
            }
            _ => {
                // The memory guard tells the replay when memory gets tight
                // or low (mitcad#80), as in a watched import.
                let progress = Arc::new(mitcad_import::Progress::new());
                let attempt = Try {
                    progress: Some(progress.clone()),
                    ..Try::default()
                };
                MemoryGuard::with_limit(None).around(&progress, || {
                    self.import_ipt_try(
                        file,
                        options,
                        info,
                        &stored,
                        &design,
                        &meshes,
                        warnings,
                        read_seconds,
                        &attempt,
                    )
                })?
            }
        };
        if let Some(path) = &options.report_path {
            let text = serde_json::to_string_pretty(&value).expect("the report serializes");
            std::fs::write(path, format!("{text}\n"))
                .map_err(|e| ApiError(format!("{path}: {e}")))?;
        }
        Ok(value)
    }

    /// The replay on a thread of its own, watched (as the `.f3d` import's,
    /// `f3d_import.rs`, mitcad#82): when it makes no progress for `limit`
    /// seconds (a geometry kernel call that does not return), the try is
    /// given up and the replay runs again on a new document with the item
    /// it hung on taking the file's bodies (and the items before it whose
    /// definitions gave no state taking them at once); hung while comparing
    /// the final bodies, again comparing volumes only; hung elsewhere, with
    /// the stored bodies without the timeline.
    fn import_ipt_watched(
        &mut self,
        prepared: Arc<Prepared>,
        limit: f64,
    ) -> Result<Value, ApiError> {
        use std::sync::atomic::Ordering;
        let mut attempt = Try::default();
        let mut notes: Vec<String> = Vec::new();
        for _ in 0..=HANG_RETRIES {
            let progress = Arc::new(mitcad_import::Progress::new());
            attempt.progress = Some(progress.clone());
            // Measured at every poll: the replay drops what it can do
            // without when memory gets tight, and cuts its definitions short
            // when it is low, before an allocation fails (mitcad#80).
            let mut guard = MemoryGuard::with_limit(None);
            guard.check(&progress);
            let (sender, receiver) = std::sync::mpsc::channel();
            let (shared, try_owned, watched) =
                (prepared.clone(), attempt.clone(), progress.clone());
            let thread = TryThread::new();
            let ended = TryEnd(thread.clone());
            let started = spawn_import(move || {
                // Dropped last, once the try's document and shapes are gone.
                let _ended = ended;
                catch_occt_crashes();
                let mut document = Document(mitcad_model::Document::new(OcctKernel));
                let p = &*shared;
                let result = document.import_ipt_try(
                    &p.file,
                    &p.options,
                    &p.info,
                    &p.stored,
                    &p.design,
                    &p.meshes,
                    p.warnings.clone(),
                    p.read_seconds,
                    &try_owned,
                );
                drop(shared);
                drop(try_owned);
                // A try given up: what it made is dropped, never reported.
                if !watched.abandoned() {
                    let _ = sender.send(Handover((document, result)));
                }
            });
            if let Err(e) = started {
                return self.import_ipt_here(&prepared, attempt, notes, &e);
            }
            let mut step = u64::MAX;
            let mut moved = std::time::Instant::now();
            let hung = loop {
                match receiver.recv_timeout(MEMORY_POLL) {
                    Ok(Handover((document, result))) => {
                        let mut value = result?;
                        if let Some(w) = value["warnings"].as_array_mut() {
                            w.extend(notes.iter().map(|n| json!(n)));
                        }
                        *self = document;
                        return Ok(value);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        guard.check(&progress);
                        let now = progress.step.load(Ordering::Relaxed);
                        if now != step {
                            step = now;
                            moved = std::time::Instant::now();
                        } else if moved.elapsed().as_secs_f64() > limit {
                            let item = progress.item.load(Ordering::Relaxed);
                            progress.abandon();
                            thread.abandon();
                            break item;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(ApiError("the import stopped without a result".to_owned()));
                    }
                }
            };
            for (item, reason) in progress.failed_items() {
                if !attempt.failed.iter().any(|(i, _)| *i == item) {
                    attempt.failed.push((item, reason));
                }
            }
            // The item whose definitions ran the process low on memory takes
            // the file's bodies at once too: the abandoned try still holds
            // its memory, and the same definitions took gigabytes.
            if let Some((_, item)) = progress.first_low_memory().filter(|(_, i)| *i >= 0)
                && !attempt.failed.iter().any(|(i, _)| *i == item)
            {
                attempt.failed.push((
                    item,
                    "the import ran low on memory on it in an earlier try".to_owned(),
                ));
            }
            let what = match hung {
                mitcad_import::Progress::FINISHING if !attempt.no_compare => {
                    attempt.no_compare = true;
                    "comparing the final bodies; they are compared by volume only".to_owned()
                }
                i if i >= 0 && !attempt.hung.contains(&i) && !attempt.bodies_only => {
                    attempt.hung.push(i);
                    format!("timeline item {i}; it takes the file's bodies")
                }
                _ if !attempt.bodies_only => {
                    attempt.bodies_only = true;
                    "the import; the stored bodies come in without the timeline".to_owned()
                }
                _ => break,
            };
            notes.push(format!(
                "the geometry kernel did not return for {limit} s on {what}"
            ));
        }
        Err(ApiError(format!(
            "the import of {} hung in the geometry kernel",
            prepared.file
        )))
    }

    /// A try on this thread, when no import thread could be started (its
    /// stack could not be reserved, `error`): the process is short of
    /// memory, an abandoned try holding its own. The first try runs here
    /// without the watchdog; after a hang, the stored bodies come in
    /// without the timeline, which takes little more memory. The memory
    /// guard watches it as any try.
    fn import_ipt_here(
        &mut self,
        prepared: &Prepared,
        mut attempt: Try,
        mut notes: Vec<String>,
        error: &std::io::Error,
    ) -> Result<Value, ApiError> {
        notes.push(if notes.is_empty() {
            format!(
                "no thread could be started for the import ({error}): it ran without its watchdog"
            )
        } else {
            attempt.bodies_only = true;
            format!(
                "no thread could be started for another try of the import ({error}): \
                 the stored bodies come in without the timeline"
            )
        });
        let progress = Arc::new(mitcad_import::Progress::new());
        attempt.progress = Some(progress.clone());
        let p = prepared;
        let mut value = MemoryGuard::with_limit(None).around(&progress, || {
            self.import_ipt_try(
                &p.file,
                &p.options,
                &p.info,
                &p.stored,
                &p.design,
                &p.meshes,
                p.warnings.clone(),
                p.read_seconds,
                &attempt,
            )
        })?;
        if let Some(w) = value["warnings"].as_array_mut() {
            w.extend(notes.iter().map(|n| json!(n)));
        }
        Ok(value)
    }

    /// One try of the replay into this document (see the module docs):
    /// one undo step with the units and the material.
    #[allow(clippy::too_many_arguments)]
    fn import_ipt_try(
        &mut self,
        file: &str,
        options: &Options,
        info: &mitcad_ipt::DocumentInfo,
        stored: &Stored,
        design: &mitcad_ipt::design::Design,
        meshes: &[Result<mitcad_ipt::mesh::Mesh, String>],
        mut warnings: Vec<String>,
        read_seconds: f64,
        attempt: &Try,
    ) -> Result<Value, ApiError> {
        let started = std::time::Instant::now();
        let mut geometry = crate::ipt_history::IptGeometry::new(stored.states.clone());
        let record_reports = &stored.records;
        let empty = self.0.features().next().is_none();
        let depth = self.0.undo_depth();
        let label = format!("Import {file}");
        let units = self.apply_part_units(info, empty, &mut warnings)?;
        let import_options = mitcad_import::Options {
            verify: !options.no_verify,
            fallback: !options.no_fallback,
            compare: !options.no_compare && !attempt.no_compare,
            undo_label: Some(label.clone()),
            time_limit: options.time_limit.filter(|t| *t > 0.0),
            hung_items: attempt.hung.clone(),
            progress: attempt.progress.clone(),
            failed_items: attempt.failed.clone(),
            // A boolean is checked only near its change: a replay must not
            // leave an invalid body where the file's are valid.
            validate: true,
            ..mitcad_import::Options::default()
        };
        // After a hang outside the items: the stored bodies, without the
        // timeline (the importer brings them for an empty replay).
        let stored_only = mitcad_f3d::design::ir::Dump {
            source: design.dump.source.clone(),
            parameters: design.dump.parameters.clone(),
            ..mitcad_f3d::design::ir::Dump::default()
        };
        let dump = if attempt.bodies_only {
            &stored_only
        } else {
            &design.dump
        };
        let mut report =
            mitcad_import::import_design(&mut self.0, dump, &mut geometry, &import_options);
        // The meshes of mesh features after the replay, as base features
        // (the history does not hold them).
        let mut skipped = Vec::new();
        let mut mesh_inputs = Vec::new();
        for (place, built) in build_meshes(meshes) {
            match built {
                Ok((shape, _)) => mesh_inputs.push(BaseInput {
                    source: Some(format!("{file}: {place}")),
                    ..BaseInput::new(vec![ImportBody {
                        name: None,
                        color: None,
                        shape,
                    }])
                }),
                Err(e) => skipped.push(json!({"source": place, "error": e})),
            }
        }
        if !mesh_inputs.is_empty() {
            self.0.add_base_features(&label, mesh_inputs)?;
        }
        let material = info.material.as_deref().and_then(mitcad_material);
        if let Some(id) = material {
            let bodies: Vec<BodyUid> = self.0.bodies().iter().map(|b| b.uid).collect();
            for uid in bodies {
                self.0.set_body_material(uid, Some(id))?;
            }
        }
        let candidates: Vec<BodyUid> = self.0.bodies().iter().map(|b| b.uid).collect();
        let hidden = self.hide_stored_hidden(&info.hidden_bodies, &candidates, &mut warnings)?;
        self.0.merge_undo(depth, &label);
        report
            .warnings
            .extend(design.left_out.iter().map(|l| format!("not imported: {l}")));
        report
            .warnings
            .extend(design.notes.iter().map(|l| format!("not decoded: {l}")));

        // The document's bodies, measured.
        let kernel = self.0.kernel();
        let mut found = Vec::new();
        for body in self.0.bodies() {
            let props = kernel.mass_properties(body.shape).ok();
            let kind = kernel.body_kind(body.shape).ok();
            let solid = kind == Some(mitcad_model::BodyKind::Solid);
            let mesh = kind == Some(mitcad_model::BodyKind::Mesh);
            let uid = body.uid.to_string();
            found.push(json!({
                "source": body.name,
                "feature": uid.split('.').next().unwrap_or_default(),
                "solid": solid,
                "mesh": mesh,
                // A mesh body has no surfaces to check (null when the
                // checker ran out of memory).
                "valid": if mesh {
                    Some(true)
                } else {
                    crate::kernel::exchange::ffi::shape_is_valid(body.shape).ok()
                },
                "volume": props.as_ref().map(|p| p.volume),
                "area": props.as_ref().map(|p| p.area),
                "faces": kernel.face_count(body.shape).ok(),
                "hidden": hidden.contains(&body.uid),
                "bodies": [{
                    "uid": uid, "name": body.name,
                    "kind": if solid { "solid" } else if mesh { "mesh" } else { "sheet" }
                }],
            }));
        }
        let counts: HashMap<&str, usize> = report
            .counts()
            .into_iter()
            .map(|(o, n)| (o.as_str(), n))
            .collect();
        let mut value = json!({
            "file": file,
            "format": "ipt",
            "history": true,
            "part_number": info.part_number,
            "material": info.material,
            "mitcad_material": material,
            "release": info.release,
            "units": units,
            "unit_code": info.length_unit_code,
            "records": record_reports,
            "imported": found,
            "skipped": skipped,
            "warnings": warnings,
            "parameters": design.parameters,
            "expressions": design.expressions,
            "features": design.features(),
            "features_with_states": geometry.items_with_states(),
            "counts": counts,
            "design_text": report.text(),
            "design": report,
            "seconds": {"read": read_seconds, "import": started.elapsed().as_secs_f64()},
        });
        if let Some(step) = &options.reference {
            let solids: Vec<(BodyUid, String)> = self
                .0
                .bodies()
                .iter()
                .filter(|b| kernel.body_kind(b.shape).ok() == Some(mitcad_model::BodyKind::Solid))
                .map(|b| (b.uid, b.name.clone()))
                .collect();
            let valid = value["imported"]
                .as_array()
                .is_some_and(|a| a.iter().all(|b| b["valid"] == true || b["solid"] == false));
            value["reference"] = self.compare_reference(step, &solids, valid, options)?;
        }
        Ok(value)
    }

    /// Hides the bodies the file keeps hidden: each hidden range box (cm)
    /// of the result list (`mitcad_ipt::result`) hides the candidate whose
    /// bounding box is closest to it, when that is within 1% of the box's
    /// diagonal (at least 0.05 mm). The bodies hidden.
    fn hide_stored_hidden(
        &mut self,
        ranges: &[[[f64; 3]; 2]],
        candidates: &[BodyUid],
        warnings: &mut Vec<String>,
    ) -> Result<Vec<BodyUid>, ApiError> {
        let mut hidden = Vec::new();
        if ranges.is_empty() {
            return Ok(hidden);
        }
        let boxes: Vec<(BodyUid, [f64; 3], [f64; 3])> = candidates
            .iter()
            .filter_map(|&uid| {
                let shape = self.0.body_shape(uid)?;
                let b = self.0.kernel().bounding_box(shape).ok()??;
                Some((uid, b.min, b.max))
            })
            .collect();
        for range in ranges {
            let (min, max) = (range[0].map(|v| v * 10.0), range[1].map(|v| v * 10.0));
            let diagonal = (0..3)
                .map(|k| (max[k] - min[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            let tolerance = (0.01 * diagonal).max(0.05);
            let best = boxes
                .iter()
                .filter(|(uid, _, _)| !hidden.contains(uid))
                .map(|(uid, lo, hi)| {
                    let d = (0..3)
                        .map(|k| (lo[k] - min[k]).abs().max((hi[k] - max[k]).abs()))
                        .fold(0.0, f64::max);
                    (*uid, d)
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            match best {
                Some((uid, d)) if d <= tolerance => {
                    self.0.set_body_visible(uid, false)?;
                    hidden.push(uid);
                }
                _ => warnings.push(format!(
                    "the file keeps a body hidden (range [{:.3} {:.3} {:.3}] [{:.3} {:.3} {:.3}] mm) \
                     that no imported body matches",
                    min[0], min[1], min[2], max[0], max[1], max[2]
                )),
            }
        }
        Ok(hidden)
    }

    /// Sets the document's units to the part's when the document had
    /// nothing yet; the unit set, or a warning why not.
    pub(crate) fn apply_part_units(
        &mut self,
        info: &mitcad_ipt::DocumentInfo,
        empty: bool,
        warnings: &mut Vec<String>,
    ) -> Result<Option<&'static str>, ApiError> {
        match info
            .length_unit
            .and_then(|u| mitcad_model::expr::Unit::parse(u).ok()?.length())
        {
            Some((length, 1)) if empty => {
                self.0.set_units(length)?;
                return Ok(info.length_unit);
            }
            Some(_) => warnings.push(format!(
                "the part's length unit ({}) is not applied to a document that has features",
                info.length_unit.unwrap_or_default()
            )),
            None => {
                if let Some(code) = info.length_unit_code {
                    warnings.push(format!(
                        "unknown length unit {code}: the document keeps its units"
                    ));
                }
            }
        }
        Ok(None)
    }

    /// Each STEP solid matched with the imported solid of the closest
    /// volume; volumes and areas compared relatively.
    fn compare_reference(
        &self,
        step: &str,
        solids: &[(BodyUid, String)],
        valid: bool,
        options: &Options,
    ) -> Result<Value, ApiError> {
        let kernel = self.0.kernel();
        let failed = |e: &dyn std::fmt::Display| ApiError(format!("{}: {e}", file_name(step)));
        let mut theirs = Vec::new();
        for body in kernel
            .read_file(step, &ReadOptions::default())
            .map_err(|e| failed(&e))?
        {
            for solid in kernel.solids(&body.shape).map_err(|e| failed(&e))? {
                theirs.push(kernel.mass_properties(&solid).map_err(|e| failed(&e))?);
            }
        }
        let mut ours = Vec::new();
        for (uid, name) in solids {
            let shape = self
                .0
                .body_shape(*uid)
                .ok_or_else(|| ApiError(format!("{uid} has no shape")))?;
            let props = kernel.mass_properties(shape).map_err(|e| failed(&e))?;
            ours.push((uid, name, props, false));
        }
        let mut pairs = Vec::new();
        let mut worst: f64 = 0.0;
        let mut unmatched_step = Vec::new();
        for (i, step_props) in theirs.iter().enumerate() {
            let best = ours
                .iter()
                .enumerate()
                .filter(|(_, o)| !o.3)
                .min_by(|a, b| {
                    relative(a.1.2.volume, step_props.volume)
                        .total_cmp(&relative(b.1.2.volume, step_props.volume))
                })
                .map(|(j, _)| j);
            let Some(j) = best else {
                unmatched_step.push(
                    json!({"step_body": i, "volume": step_props.volume, "area": step_props.area}),
                );
                continue;
            };
            ours[j].3 = true;
            let (uid, name, props, _) = &ours[j];
            let volume = relative(props.volume, step_props.volume);
            let area = relative(props.area, step_props.area);
            worst = worst.max(volume).max(area);
            pairs.push(json!({
                "step_body": i, "step_volume": step_props.volume, "step_area": step_props.area,
                "body": uid, "name": name, "volume": props.volume, "area": props.area,
                "volume_difference": volume, "area_difference": area,
            }));
        }
        let unmatched: Vec<Value> = ours
            .iter()
            .filter(|o| !o.3)
            .map(|(uid, name, props, _)| json!({"body": uid, "name": name, "volume": props.volume}))
            .collect();
        let mut value = json!({
            "file": file_name(step),
            "step_solids": theirs.len(),
            "solids": pairs,
            "unmatched_step": unmatched_step,
            "unmatched_bodies": unmatched,
            "max_difference": worst,
            "max_relative": options.max_relative,
            "valid": valid,
        });
        if options.deviation && !solids.is_empty() {
            let shapes: Vec<_> = solids
                .iter()
                .filter_map(|(uid, _)| self.0.body_shape(*uid))
                .collect();
            let comparison = kernel
                .compare_step(&shapes, step, &Default::default())
                .map_err(|e| failed(&e))?;
            value["max_deviation"] = json!(comparison.max_deviation);
        }
        let pass = valid
            && value["unmatched_step"]
                .as_array()
                .is_some_and(Vec::is_empty)
            && value["unmatched_bodies"]
                .as_array()
                .is_some_and(Vec::is_empty)
            && worst <= options.max_relative;
        value["pass"] = json!(pass);
        Ok(value)
    }

    /// The bridge's `import_ipt`: the report as JSON, or text.
    pub(crate) fn import_ipt(&mut self, path: &str, json: &str) -> Result<String, ApiError> {
        // An assembly (.iam) has an import of its own.
        if crate::iam_import::is_assembly(path) {
            return self.import_iam(path, json);
        }
        let json = if json.trim().is_empty() { "{}" } else { json };
        let options: Options = serde_json::from_str(json)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        // A job does not cancel it (the import is one undo step).
        let report = self.without_job(|document| document.import_ipt_report(path, &options))?;
        Ok(if options.text {
            summary(&report)
        } else {
            serde_json::to_string_pretty(&report).expect("the report serializes")
        })
    }

    /// `{"cmd": "import_ipt", "path": ..., ...}`: the import as a document
    /// command (the model's commands cannot build the file's bodies).
    pub(crate) fn import_ipt_command(&mut self, mut command: Value) -> Result<Value, ApiError> {
        let path = command
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError("import_ipt needs a \"path\"".to_owned()))?
            .to_owned();
        if crate::iam_import::is_assembly(&path) {
            return self.import_iam_command(command);
        }
        if let Some(map) = command.as_object_mut() {
            map.remove("cmd");
            map.remove("path");
        }
        let options: Options = serde_json::from_value(command)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let report = self.without_job(|document| document.import_ipt_report(&path, &options))?;
        let bodies = report["imported"].as_array().map_or(0, Vec::len);
        Ok(json!({
            "file": report["file"],
            "design": report["part_number"],
            "bodies": bodies,
            "report": report,
        }))
    }
}

/// A mesh body built, with its line of the report, or why it was not.
type BuiltMesh = Result<(SharedPtr<Shape>, Value), String>;

/// The mesh bodies of the part's mesh features (`mitcad_ipt::mesh`,
/// `IptFile::meshes`), each with where it comes from.
fn build_meshes(meshes: &[Result<mitcad_ipt::mesh::Mesh, String>]) -> Vec<(String, BuiltMesh)> {
    meshes
        .iter()
        .map(|mesh| match mesh {
            Ok(mesh) => {
                let place = mesh.place();
                let built = crate::kernel::mesh::ffi::mesh_from_triangles(
                    &mesh.vertices_mm(),
                    mesh.triangles.as_flattened(),
                )
                .map_err(|e| e.to_string())
                .map(|shape| {
                    let (volume, area) = mesh.volume_area();
                    let report = json!({
                        "source": place, "solid": false, "mesh": true, "valid": true,
                        "closed": mesh.is_closed(), "volume": volume, "area": area,
                        "faces": 1, "triangles": mesh.triangles.len(), "issues": 0
                    });
                    (shape, report)
                });
                (place, built)
            }
            Err(e) => match e.split_once(": ") {
                Some((place, why)) => (place.to_owned(), Err(why.to_owned())),
                None => (String::new(), Err(e.clone())),
            },
        })
        .collect()
}

/// The import report as text.
fn summary(report: &Value) -> String {
    let mut text = String::new();
    let imported = report["imported"].as_array().map_or(&[][..], Vec::as_slice);
    let skipped = report["skipped"].as_array().map_or(&[][..], Vec::as_slice);
    let count = |key: &str| imported.iter().filter(|b| b[key] == true).count();
    let field = |key: &str| report[key].as_str().map(str::to_owned);
    text += &format!(
        "Imported {} bodies of {} ({} solids, {} valid; {} not built)\n",
        imported.len(),
        report["file"].as_str().unwrap_or_default(),
        count("solid"),
        count("valid"),
        skipped.len()
    );
    text += &format!(
        "  part number: {}; material: {}{}; units: {}; saved by release {}\n",
        field("part_number").unwrap_or_else(|| "-".into()),
        field("material").unwrap_or_else(|| "-".into()),
        match report["mitcad_material"].as_str() {
            Some(id) => format!(" (Mitcad's {id})"),
            None if report["material"].is_string() =>
                " (not in Mitcad's library: bodies keep the default)".into(),
            None => String::new(),
        },
        field("units").unwrap_or_else(|| "mm".into()),
        field("release").unwrap_or_else(|| "unknown".into()),
    );
    for record in report["records"].as_array().into_iter().flatten() {
        text += &format!(
            "  B-rep record {}: {}, {} bodies{}\n",
            record["source"].as_str().unwrap_or_default(),
            record["asm_version"].as_str().unwrap_or_default(),
            record["bodies"],
            match record["history_states"].as_u64() {
                Some(n) => format!(", history of {n} states"),
                None => String::new(),
            }
        );
    }
    if report["history"] == true {
        let p = &report["parameters"];
        let e = &report["expressions"];
        let count = |v: &Value| v.as_array().map_or(0, Vec::len);
        text += &format!(
            "  parameters: {} model, {} user ({} records, {} outside the part's table, {} not read); \
             expressions: {} translated, {} agree with the stored values, {} differ, {} not translated, \
             {} computed by the model, {} unused kept as values\n",
            p["model"],
            p["user"],
            p["records"],
            p["outside"],
            count(&p["unread"]),
            e["translated"],
            e["agree"],
            count(&e["differ"]),
            count(&e["not_translated"]),
            count(&e["computed"]),
            count(&e["unused"])
        );
        text += &format!(
            "  features: {} ({} with their history state)\n",
            report["features"], report["features_with_states"]
        );
        for line in report["design_text"].as_str().unwrap_or_default().lines() {
            text += &format!("  {line}\n");
        }
    }
    for body in imported {
        let name = &body["bodies"][0];
        text += &format!(
            "  {} {} ({}): {} {}{}, volume {:.3} mm3, area {:.3} mm2, {} faces{} from {}\n",
            body["feature"].as_str().unwrap_or_default(),
            name["name"].as_str().unwrap_or_default(),
            name["uid"].as_str().unwrap_or_default(),
            if body["valid"] == true {
                "valid"
            } else {
                "INVALID"
            },
            if body["solid"] == true {
                "solid"
            } else if body["mesh"] == true {
                "mesh"
            } else {
                "sheet"
            },
            if body["hidden"] == true {
                " (hidden)"
            } else {
                ""
            },
            body["volume"].as_f64().unwrap_or_default(),
            body["area"].as_f64().unwrap_or_default(),
            body["faces"],
            match body["issues"].as_u64() {
                Some(0) | None => String::new(),
                Some(n) => format!(", {n} issues"),
            },
            body["source"].as_str().unwrap_or_default()
        );
        // Why a body is not valid.
        if body["valid"] != true {
            for message in body["messages"].as_array().into_iter().flatten() {
                text += &format!("    {}\n", message.as_str().unwrap_or_default());
            }
        }
    }
    for body in skipped {
        text += &format!(
            "  not built: {}: {}\n",
            body["source"].as_str().unwrap_or_default(),
            body["error"].as_str().unwrap_or_default()
        );
    }
    for warning in report["warnings"].as_array().into_iter().flatten() {
        text += &format!("  warning: {}\n", warning.as_str().unwrap_or_default());
    }
    let reference = &report["reference"];
    if reference.is_object() {
        text += &format!(
            "reference {}: {} STEP solids, largest relative difference {:.3e} (limit {:e}){}: {}\n",
            reference["file"].as_str().unwrap_or_default(),
            reference["step_solids"],
            reference["max_difference"].as_f64().unwrap_or_default(),
            reference["max_relative"].as_f64().unwrap_or_default(),
            match reference["max_deviation"].as_f64() {
                Some(d) => format!(", surface deviation {d:.2e} mm"),
                None => String::new(),
            },
            if reference["pass"] == true {
                "pass"
            } else {
                "FAILED"
            }
        );
        for pair in reference["solids"].as_array().into_iter().flatten() {
            text += &format!(
                "    STEP solid {}: volume {:.3} mm3, area {:.3} mm2 ~ {} ({}): volume {:.2e}, area {:.2e}\n",
                pair["step_body"],
                pair["step_volume"].as_f64().unwrap_or_default(),
                pair["step_area"].as_f64().unwrap_or_default(),
                pair["name"].as_str().unwrap_or_default(),
                pair["body"].as_str().unwrap_or_default(),
                pair["volume_difference"].as_f64().unwrap_or_default(),
                pair["area_difference"].as_f64().unwrap_or_default()
            );
        }
        for step in reference["unmatched_step"].as_array().into_iter().flatten() {
            text += &format!(
                "    STEP solid {} has no imported solid (volume {:.3} mm3)\n",
                step["step_body"],
                step["volume"].as_f64().unwrap_or_default()
            );
        }
        for body in reference["unmatched_bodies"]
            .as_array()
            .into_iter()
            .flatten()
        {
            text += &format!(
                "    {} ({}) has no STEP solid (volume {:.3} mm3)\n",
                body["name"].as_str().unwrap_or_default(),
                body["body"].as_str().unwrap_or_default(),
                body["volume"].as_f64().unwrap_or_default()
            );
        }
        if reference["valid"] != true {
            text += "    an imported solid is not valid\n";
        }
    }
    text
}
