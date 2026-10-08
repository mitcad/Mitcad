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

use serde::Deserialize;
use serde_json::{Value, json};

use mitcad_model::api::ApiError;
use mitcad_model::exchange::ReadOptions;
use mitcad_model::{BaseInput, BodyUid, ImportBody, Kernel};

use crate::Document;
use crate::brep_import::{Source, to_ffi};
use crate::kernel::exchange::ffi::{build_brep_body, catch_occt_crashes};
use crate::kernel::shape::ffi::new_shape_list;

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
fn mitcad_material(name: &str) -> Option<&'static str> {
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
            return self.import_ipt_history(
                file,
                options,
                &info,
                &records,
                design,
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
                let data = to_ffi(&source, body);
                let built = build_brep_body(&data, list.pin_mut());
                let index = list.size() - 1;
                if !built.built {
                    skipped.push(json!({"source": place, "error": built.error.to_string()}));
                    continue;
                }
                found.push(json!({
                    "source": place, "solid": built.solid, "valid": built.valid,
                    "volume": built.volume, "area": built.area, "faces": built.faces,
                    "issues": built.issues
                }));
                sources.push((place, index));
            }
        }
        if sources.is_empty() {
            return Err(ApiError(format!(
                "{file} has no bodies to import ({} could not be built)",
                skipped.len()
            )));
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
        let imported = self.0.add_base_features(&label, inputs)?;
        let units = self.apply_part_units(&info, empty, &mut warnings)?;
        let material = info.material.as_deref().and_then(mitcad_material);
        if let Some(id) = material {
            for body in imported.iter().flat_map(|f| &f.bodies) {
                self.0.set_body_material(body.uid, Some(id))?;
            }
        }
        self.0.merge_undo(depth, &label);

        for (report, feature) in found.iter_mut().zip(&imported) {
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
        mut warnings: Vec<String>,
        read_seconds: f64,
    ) -> Result<Value, ApiError> {
        let started = std::time::Instant::now();
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
        let mut geometry = crate::ipt_history::IptGeometry::new(records, &design.results)
            .map_err(|e| ApiError(format!("{file}: {e}")))?;
        let record_reports: Vec<Value> = records
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
        let empty = self.0.features().next().is_none();
        let depth = self.0.undo_depth();
        let label = format!("Import {file}");
        let units = self.apply_part_units(info, empty, &mut warnings)?;
        let import_options = mitcad_import::Options {
            verify: !options.no_verify,
            fallback: !options.no_fallback,
            compare: !options.no_compare,
            undo_label: Some(label.clone()),
            time_limit: options.time_limit.filter(|t| *t > 0.0),
            ..mitcad_import::Options::default()
        };
        let mut report =
            mitcad_import::import_design(&mut self.0, &design.dump, &mut geometry, &import_options);
        let material = info.material.as_deref().and_then(mitcad_material);
        if let Some(id) = material {
            let bodies: Vec<BodyUid> = self.0.bodies().iter().map(|b| b.uid).collect();
            for uid in bodies {
                self.0.set_body_material(uid, Some(id))?;
            }
        }
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
            let solid = kernel.body_kind(body.shape).ok() == Some(mitcad_model::BodyKind::Solid);
            let uid = body.uid.to_string();
            found.push(json!({
                "source": body.name,
                "feature": uid.split('.').next().unwrap_or_default(),
                "solid": solid,
                "valid": crate::kernel::exchange::ffi::shape_is_valid(body.shape),
                "volume": props.as_ref().map(|p| p.volume),
                "area": props.as_ref().map(|p| p.area),
                "faces": kernel.face_count(body.shape).ok(),
                "bodies": [{"uid": uid, "name": body.name, "kind": if solid { "solid" } else { "sheet" }}],
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
            "skipped": [],
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
        if let Some(path) = &options.report_path {
            let text = serde_json::to_string_pretty(&value).expect("the report serializes");
            std::fs::write(path, format!("{text}\n"))
                .map_err(|e| ApiError(format!("{path}: {e}")))?;
        }
        Ok(value)
    }

    /// Sets the document's units to the part's when the document had
    /// nothing yet; the unit set, or a warning why not.
    fn apply_part_units(
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
             expressions: {} translated, {} agree with the stored values, {} differ, {} not translated\n",
            p["model"],
            p["user"],
            p["records"],
            p["outside"],
            count(&p["unread"]),
            e["translated"],
            e["agree"],
            count(&e["differ"]),
            count(&e["not_translated"])
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
            "  {} {} ({}): {} {}, volume {:.3} mm3, area {:.3} mm2, {} faces{} from {}\n",
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
            } else {
                "sheet"
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
