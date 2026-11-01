// SPDX-License-Identifier: MIT
//! The IFC import (mitcad#273): `mitcad_ifc` reads the file (`.ifc`, or
//! `.ifczip`), `mitcad_import_ifc` builds its model through the OCCT
//! kernel, under the import process's memory guard (mitcad#80: low on
//! memory, the import stops before the next element and reports the rest
//! as not imported). The `import_ifc` document command and `mitcad-cli
//! import-ifc`.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use mitcad_ifc::{IfcFile, ReadOptions};
use mitcad_model_api::ApiError;

use crate::Document;
use crate::f3d_import::MemoryGuard;
use crate::kernel::exchange::ffi::catch_occt_crashes;

/// The options of `import_ifc` (JSON).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    /// A readable report instead of JSON.
    #[serde(default)]
    text: bool,
    /// Also write the JSON report to this file.
    #[serde(default)]
    report_path: Option<String>,
    /// Cut the elements' openings from them.
    #[serde(default = "yes")]
    openings: bool,
    /// Uses of a type's shape as occurrences of one component.
    #[serde(default = "yes")]
    occurrences: bool,
    /// The most memory (MiB) the import may use before it stops; the
    /// process's limits otherwise.
    #[serde(default)]
    memory_limit: Option<f64>,
}

fn yes() -> bool {
    true
}

fn file_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

impl Document {
    fn import_ifc_report(&mut self, path: &str, options: &Options) -> Result<Value, ApiError> {
        let name = file_name(path);
        let started = std::time::Instant::now();
        let file = IfcFile::open_with(std::path::Path::new(path), &ReadOptions::import())
            .map_err(|e| ApiError(format!("{name}: {e}")))?;
        let read_seconds = started.elapsed().as_secs_f64();
        catch_occt_crashes();
        let progress = Arc::new(mitcad_import::Progress::new());
        let import_options = mitcad_import_ifc::IfcOptions {
            openings: options.openings,
            occurrences: options.occurrences,
            progress: Some(progress.clone()),
            ..Default::default()
        };
        let report = MemoryGuard::with_limit(options.memory_limit).around(&progress, || {
            mitcad_import_ifc::import_ifc(&mut self.0, &file, name, &import_options)
        });
        let mut value = serde_json::to_value(&report).expect("the report serializes");
        value["format"] = json!("ifc");
        // What the outcomes left out and the warnings (mitcad#1040).
        value["notices"] = mitcad_model::notice::to_json(&report.notices());
        value["text_report"] = json!(report.text());
        value["read_seconds"] = json!(read_seconds);
        value["instances"] = json!(file.entities.len());
        value["left_out_instances"] = json!(file.left_out);
        if let Some(path) = &options.report_path {
            let text = serde_json::to_string_pretty(&value).expect("the report serializes");
            std::fs::write(path, format!("{text}\n"))
                .map_err(|e| ApiError(format!("{path}: {e}")))?;
        }
        Ok(value)
    }

    fn ifc_options(json: Value) -> Result<Options, ApiError> {
        serde_json::from_value(json).map_err(|e| ApiError(format!("invalid import options: {e}")))
    }

    /// The bridge's `import_ifc`: the report as JSON, or text.
    pub(crate) fn import_ifc(&mut self, path: &str, json: &str) -> Result<String, ApiError> {
        let json = if json.trim().is_empty() { "{}" } else { json };
        let value: Value = serde_json::from_str(json)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let options = Self::ifc_options(value)?;
        let report = self.without_job(|document| document.import_ifc_report(path, &options))?;
        Ok(if options.text {
            report["text_report"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        } else {
            serde_json::to_string_pretty(&report).expect("the report serializes")
        })
    }

    /// `{"cmd": "import_ifc", "path": ..., ...}`.
    pub(crate) fn import_ifc_command(&mut self, mut command: Value) -> Result<Value, ApiError> {
        let path = command
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError("import_ifc needs a \"path\"".to_owned()))?
            .to_owned();
        if let Some(map) = command.as_object_mut() {
            map.remove("cmd");
            map.remove("path");
        }
        let options = Self::ifc_options(command)?;
        let report = self.without_job(|document| document.import_ifc_report(&path, &options))?;
        Ok(json!({
            "file": report["file"],
            "products": report["products"].as_array().map_or(0, Vec::len),
            "bodies": report["bodies"],
            "report": report,
        }))
    }
}

/// The IFC reader's test building, for `mitcad-cli import-ifc --write-test`.
pub(crate) fn ifc_test_building() -> String {
    mitcad_ifc::testdata::BUILDING.to_owned()
}
