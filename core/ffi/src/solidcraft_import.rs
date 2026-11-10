// SPDX-License-Identifier: MIT
//! `.solidcraft` import with OCCT (mitcad#450): `mitcad-import`'s
//! SolidCraft front end, as the `import_solidcraft` document command and
//! for `mitcad-cli import-solidcraft`.

use serde::Deserialize;
use serde_json::Value;

use mitcad_import::solidcraft::{SolidcraftOptions, SolidcraftReport, import_solidcraft};
use mitcad_model_api::ApiError;

use crate::Document;

/// The options of `import_solidcraft` (JSON).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    /// A readable report instead of JSON.
    #[serde(default)]
    text: bool,
    /// Also write the JSON report to this file.
    #[serde(default)]
    report_path: Option<String>,
    /// Values to check the import against (a JSON file: SolidCraft's
    /// evaluation of the design, or a subset of it).
    #[serde(default)]
    expected: Option<String>,
    /// Parameters (the file's names) to change after the import, before
    /// the report measures the bodies: `{"name": "expression"}`.
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
    fn import_solidcraft_report(
        &mut self,
        path: &str,
        options: &Options,
    ) -> Result<SolidcraftReport, ApiError> {
        crate::kernel::exchange::ffi::catch_occt_crashes();
        let expected = match &options.expected {
            Some(file) => {
                let text =
                    std::fs::read_to_string(file).map_err(|e| ApiError(format!("{file}: {e}")))?;
                Some(serde_json::from_str(&text).map_err(|e| ApiError(format!("{file}: {e}")))?)
            }
            None => None,
        };
        let import_options = SolidcraftOptions {
            undo_label: Some(format!("Import {}", file_name(path))),
            set_parameters: options
                .set_parameters
                .iter()
                .map(|(n, e)| (n.clone(), e.clone()))
                .collect(),
            expected,
        };
        let report = import_solidcraft(&mut self.0, std::path::Path::new(path), &import_options)
            .map_err(|e| ApiError(format!("{}: {e}", file_name(path))))?;
        if let Some(file) = &options.report_path {
            let json = serde_json::to_string_pretty(&report).expect("the report serializes");
            std::fs::write(file, format!("{json}\n"))
                .map_err(|e| ApiError(format!("{file}: {e}")))?;
        }
        Ok(report)
    }

    /// The bridge's `import_solidcraft`: the report as JSON, or text.
    pub(crate) fn import_solidcraft(&mut self, path: &str, json: &str) -> Result<String, ApiError> {
        let json = if json.trim().is_empty() { "{}" } else { json };
        let options: Options = serde_json::from_str(json)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let report = self.import_solidcraft_report(path, &options)?;
        Ok(if options.text {
            report.text()
        } else {
            serde_json::to_string_pretty(&report).expect("the report serializes")
        })
    }

    /// `{"cmd": "import_solidcraft", "path": ..., ...}`: the import as a
    /// document command.
    pub(crate) fn import_solidcraft_command(
        &mut self,
        mut command: Value,
    ) -> Result<Value, ApiError> {
        let path = command
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError("import_solidcraft needs a \"path\"".to_owned()))?
            .to_owned();
        if let Some(map) = command.as_object_mut() {
            map.remove("cmd");
            map.remove("path");
        }
        let options: Options = serde_json::from_value(command)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let report = self.import_solidcraft_report(&path, &options)?;
        Ok(report.command_result())
    }
}

/// The bridge's `solidcraft_file_check` (mitcad#1622): "Not a SolidCraft
/// design: …" for a `.sc` file whose format key is not SolidCraft's (other
/// programs use `.sc` too), empty for any other file.
pub(crate) fn solidcraft_file_check(path: &str) -> String {
    mitcad_import::solidcraft::check_file(std::path::Path::new(path))
        .err()
        .unwrap_or_default()
}
