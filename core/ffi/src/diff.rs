// SPDX-License-Identifier: MIT
//! Comparison of two documents for C++ (P12c, `mitcad_model::diff`): two
//! project files, two versions, or a version and the open document.

use mitcad_model::api::ApiError;
use serde::Deserialize;
use serde_json::json;

use crate::Document;

/// The options of `diff_documents`.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    /// Text for people instead of JSON.
    #[serde(default)]
    text: bool,
    /// Also the bodies' volumes and areas, as last computed.
    #[serde(default)]
    geometry: bool,
    /// Names of the two designs (files, versions) for the answer, and the
    /// project file whose versions they are; `to` None with `path`: the
    /// file as saved.
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

pub fn diff_documents(from: &Document, to: &Document, json: &str) -> Result<String, ApiError> {
    let options: Options = if json.trim().is_empty() {
        Options::default()
    } else {
        serde_json::from_str(json).map_err(|e| ApiError(format!("invalid diff options: {e}")))?
    };
    let mut diff = mitcad_model::diff::diff_documents(&from.0, &to.0);
    if options.geometry {
        diff.add_geometry(&from.0, &to.0);
    }
    let text = diff.to_text();
    if options.text {
        // The heading of the version history's `diff` command.
        let to = options.to.as_deref();
        let header = match (&options.path, &options.from) {
            (_, None) => String::new(),
            (Some(path), Some(from)) => format!(
                "Changes in {path} from {from} to {}:\n",
                to.unwrap_or("the saved file")
            ),
            (None, Some(from)) => format!("Changes from {from} to {}:\n", to.unwrap_or("?")),
        };
        return Ok(header + &text);
    }
    let mut answer = diff.to_json();
    if let Some(path) = options.path {
        answer["path"] = json!(path);
    }
    answer["from"] = json!(options.from);
    answer["to"] = json!(options.to);
    answer["text"] = json!(text);
    Ok(answer.to_string())
}
