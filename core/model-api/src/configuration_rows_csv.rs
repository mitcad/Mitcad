// SPDX-License-Identifier: MIT
//! A configuration table's rows as CSV (mitcad#1044,
//! `document/configuration_rows_csv.rs`): the `configuration_rows_csv`
//! query (the rows' CSV), the `configuration_rows_csv_preview` query (what
//! an import would do) and the `import_configuration_rows_csv` command.
//! The CSV is the text itself: the application reads and writes the file.
//! See `commands.md`, "Configuration rows as CSV".

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, RowsCsvImport};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(super) enum RowsCsvCommand {
        "import_configuration_rows_csv" => ImportConfigurationRowsCsv {
            /// The CSV text: the first column of values is the key option.
            csv: String,
        },
    }
}

/// The CSV text a preview reads.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RowsCsvPreviewQuery {
    csv: String,
}

/// An import's outcome as JSON, with its `summary`.
fn outcome_json(outcome: &RowsCsvImport) -> Value {
    let mut value = serde_json::to_value(outcome).unwrap_or_else(|_| json!({}));
    value["summary"] = json!(outcome.summary());
    value
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777).
pub(super) trait ConfigurationRowsCsvOps<K: Kernel> {
    fn run_rows_csv_command(&mut self, command: RowsCsvCommand) -> Result<Value, ApiError>;

    /// The `configuration_rows_csv` query: the rows' CSV and their number.
    fn configuration_rows_csv_json(&self) -> Value;

    /// The `configuration_rows_csv_preview` query.
    fn configuration_rows_csv_preview_json(
        &self,
        query: &RowsCsvPreviewQuery,
    ) -> Result<Value, ApiError>;
}

impl<K: Kernel> ConfigurationRowsCsvOps<K> for Document<K> {
    fn run_rows_csv_command(&mut self, command: RowsCsvCommand) -> Result<Value, ApiError> {
        Ok(match command {
            RowsCsvCommand::ImportConfigurationRowsCsv { csv } => {
                let outcome = self.import_configuration_rows_csv(&csv)?;
                let mut value = outcome_json(&outcome);
                // The cascade's moves, as `generate_configuration_rows`
                // lists them; none for an import that changes nothing.
                value["moves"] = if outcome.unchanged {
                    json!([])
                } else {
                    super::configurations::moves_json(self.configuration_moves())
                };
                value
            }
        })
    }

    fn configuration_rows_csv_json(&self) -> Value {
        let (csv, rows) = self.configuration_rows_csv();
        json!({"csv": csv, "rows": rows})
    }

    fn configuration_rows_csv_preview_json(
        &self,
        query: &RowsCsvPreviewQuery,
    ) -> Result<Value, ApiError> {
        Ok(outcome_json(
            &self.preview_configuration_rows_csv(&query.csv)?,
        ))
    }
}
