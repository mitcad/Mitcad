// SPDX-License-Identifier: MIT
//! The design's summary and statistics (mitcad#1088, File › Design
//! Properties…; `document/design_summary.rs`, `file/statistics.rs`): the
//! `design_properties` query and the `set_design_summary` command. See
//! `commands.md`, "Design properties".

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, SUMMARY_SET};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(super) enum DesignPropertiesCommand {
        "set_design_summary" => SetDesignSummary {
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            subject: Option<String>,
            #[serde(default)]
            author: Option<String>,
            #[serde(default)]
            keywords: Option<String>,
            #[serde(default)]
            comments: Option<String>,
        },
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777).
pub(super) trait DesignPropertiesOps<K: Kernel> {
    fn run_design_properties_command(
        &mut self,
        command: DesignPropertiesCommand,
    ) -> Result<Value, ApiError>;

    /// The `design_properties` query: the summary (`title`, `subject`,
    /// `author`, `keywords`, `comments`, empty where not set) and the
    /// statistics (`created`, `last_saved`, `last_saved_by`,
    /// `last_saved_with`, null where not known).
    fn design_properties_json(&self) -> Value;
}

impl<K: Kernel> DesignPropertiesOps<K> for Document<K> {
    fn run_design_properties_command(
        &mut self,
        command: DesignPropertiesCommand,
    ) -> Result<Value, ApiError> {
        Ok(match command {
            DesignPropertiesCommand::SetDesignSummary {
                title,
                subject,
                author,
                keywords,
                comments,
            } => {
                let changes: Vec<(&str, String)> = [
                    ("Title", title),
                    ("Subject", subject),
                    ("Author", author),
                    ("Keywords", keywords),
                    ("Comments", comments),
                ]
                .into_iter()
                .filter_map(|(key, text)| text.map(|text| (key, text)))
                .collect();
                let changed = self.set_design_summary(&changes)?;
                json!({"changed": changed, "summary": summary_json(self)})
            }
        })
    }

    fn design_properties_json(&self) -> Value {
        let statistics = self.design_statistics();
        json!({
            "set": SUMMARY_SET,
            "summary": summary_json(self),
            "statistics": {
                "created": statistics.created,
                "last_saved": statistics.last_saved,
                "last_saved_by": statistics.last_saved_by,
                "last_saved_with": statistics.last_saved_with,
            },
        })
    }
}

/// The summary's texts by their keys in lower case (`title`, ...).
fn summary_json<K: Kernel>(doc: &Document<K>) -> Value {
    let mut summary = Map::new();
    for (key, text) in doc.design_summary() {
        summary.insert(key.to_ascii_lowercase(), json!(text));
    }
    Value::Object(summary)
}
