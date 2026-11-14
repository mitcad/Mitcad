// SPDX-License-Identifier: MIT
//! The `design_package` query (mitcad#1795): the open design and the files
//! it refers to written into a zip for a report (Help › Send Feedback's
//! Include the design), [`Document::write_design_package`]. See
//! `commands.md`, "The design packed for a report".

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, PackageExtra};

/// The `design_package` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DesignPackageQuery {
    /// The open design's file (absolute); left out for a design never
    /// saved.
    #[serde(default)]
    file: Option<String>,
    /// Where the zip goes (absolute).
    zip: String,
    /// Files added to the zip.
    #[serde(default)]
    extra: Vec<Extra>,
}

/// A file added to the zip: its `name` there and its content as `text`
/// (UTF-8) or the file at `path`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Extra {
    name: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

impl Extra {
    fn bytes(&self) -> Result<Vec<u8>, ApiError> {
        match (&self.text, &self.path) {
            (Some(text), None) => Ok(text.as_bytes().to_vec()),
            (None, Some(path)) => std::fs::read(path)
                .map_err(|e| ApiError(format!("extra {}: cannot read the file: {e}", self.name))),
            _ => Err(ApiError(format!(
                "extra {}: give either text or path",
                self.name
            ))),
        }
    }
}

/// Answers the query: writes the zip.
pub(crate) fn query<K: Kernel>(
    doc: &Document<K>,
    query: &DesignPackageQuery,
) -> Result<Value, ApiError> {
    let extra = query
        .extra
        .iter()
        .map(|e| {
            Ok(PackageExtra {
                name: e.name.clone(),
                data: e.bytes()?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let file = query.file.as_deref().map(PathBuf::from);
    let package = doc.write_design_package(file.as_deref(), Path::new(&query.zip), &extra)?;
    Ok(json!({
        "zip": package.zip.to_string_lossy(),
        "size": package.size,
        "design": package.design,
        "files": package.files,
        "missing": package.missing,
        "left_out": package.left_out,
    }))
}
