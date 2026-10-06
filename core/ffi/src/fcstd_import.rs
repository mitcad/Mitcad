// SPDX-License-Identifier: MIT
//! FreeCAD `.FCStd` import with OCCT: `mitcad-import`'s FreeCAD front end
//! (the document's structure, sketches and history; the stored shapes are
//! OCCT B-rep, which the kernel reads as it is), as the `import_fcstd`
//! document command and for `mitcad-cli import-fcstd`.

use serde::Deserialize;
use serde_json::{Value, json};

use mitcad_import::freecad::{FcstdOptions, FcstdReport, import_fcstd};
use mitcad_model::api::ApiError;

use crate::Document;

/// The options of `import_fcstd` (JSON).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    /// The bodies only (FreeCAD's stored shapes), without the history and
    /// the sketches.
    #[serde(default)]
    bodies_only: bool,
    /// A readable report instead of JSON.
    #[serde(default)]
    text: bool,
    /// Also write the JSON report to this file.
    #[serde(default)]
    report_path: Option<String>,
    /// A dump of the document by FreeCAD (tools/freecad-export/dump.py)
    /// to compare the import with.
    #[serde(default)]
    reference: Option<String>,
    /// Parameters to change after the import, before the report measures
    /// it: `{"name": "expression"}` (against a dump of FreeCAD's document
    /// with the same change as `reference`).
    #[serde(default)]
    set_parameters: std::collections::BTreeMap<String, String>,
}

fn file_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

impl Document {
    fn import_fcstd_report(
        &mut self,
        path: &str,
        options: &Options,
    ) -> Result<FcstdReport, ApiError> {
        // OCCT crashes in reading a shape are errors of that shape.
        crate::kernel::exchange::ffi::catch_occt_crashes();
        let reference = match &options.reference {
            Some(file) => {
                let text =
                    std::fs::read_to_string(file).map_err(|e| ApiError(format!("{file}: {e}")))?;
                Some(serde_json::from_str(&text).map_err(|e| ApiError(format!("{file}: {e}")))?)
            }
            None => None,
        };
        let import_options = FcstdOptions {
            bodies_only: options.bodies_only,
            undo_label: Some(format!("Import {}", file_name(path))),
            reference,
            set_parameters: options
                .set_parameters
                .iter()
                .map(|(n, e)| (n.clone(), e.clone()))
                .collect(),
        };
        let report = import_fcstd(&mut self.0, std::path::Path::new(path), &import_options)
            .map_err(|e| ApiError(format!("{}: {e}", file_name(path))))?;
        if let Some(file) = &options.report_path {
            let json = serde_json::to_string_pretty(&report).expect("the report serializes");
            std::fs::write(file, format!("{json}\n"))
                .map_err(|e| ApiError(format!("{file}: {e}")))?;
        }
        Ok(report)
    }

    /// The bridge's `import_fcstd`: the report as JSON, or text.
    pub(crate) fn import_fcstd(&mut self, path: &str, json: &str) -> Result<String, ApiError> {
        let json = if json.trim().is_empty() { "{}" } else { json };
        let options: Options = serde_json::from_str(json)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let report = self.import_fcstd_report(path, &options)?;
        Ok(if options.text {
            report.text()
        } else {
            serde_json::to_string_pretty(&report).expect("the report serializes")
        })
    }

    /// `{"cmd": "import_fcstd", "path": ..., ...}`: the import as a
    /// document command (the model's commands cannot read the file's
    /// shapes).
    pub(crate) fn import_fcstd_command(&mut self, mut command: Value) -> Result<Value, ApiError> {
        let path = command
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError("import_fcstd needs a \"path\"".to_owned()))?
            .to_owned();
        if let Some(map) = command.as_object_mut() {
            map.remove("cmd");
            map.remove("path");
        }
        let options: Options = serde_json::from_value(command)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let report = self.import_fcstd_report(&path, &options)?;
        let counts: serde_json::Map<String, Value> = report
            .counts()
            .into_iter()
            .map(|(o, n)| (o.as_str().to_owned(), json!(n)))
            .collect();
        Ok(json!({
            "file": report.file,
            "design": report.label,
            "items": report.items.len(),
            "bodies": report.bodies.len(),
            "counts": counts,
            "report": serde_json::to_value(&report).expect("the report serializes"),
        }))
    }
}
