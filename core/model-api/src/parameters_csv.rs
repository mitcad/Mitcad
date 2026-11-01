// SPDX-License-Identifier: MIT
//! The parameters as CSV (mitcad#666, `document/parameters_csv.rs`): the
//! `parameters_csv` query (Mitcad's table), the `parameters_csv_preview`
//! query (what an import would do) and the `import_parameters_csv`
//! command. The CSV is the text itself: the application reads and writes
//! the file. See `commands.md`, "Parameters as CSV".

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::kernel::Kernel;
use mitcad_model::{CsvImport, Document};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(super) enum ParametersCsvCommand {
        "import_parameters_csv" => ImportParametersCsv {
            /// The CSV text: Mitcad's table or the `.f3d` application's
            /// parameter table.
            csv: String,
        },
    }
}

/// The CSV text a preview reads.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ParametersCsvQuery {
    csv: String,
}

/// An import's outcome as JSON: `format`, the counts and `rows`.
fn outcome_json(outcome: &CsvImport) -> Value {
    let mut value = serde_json::to_value(outcome).unwrap_or_else(|_| json!({}));
    value["summary"] = json!(outcome.summary());
    value
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait ParametersCsvOps<K: Kernel> {
    fn run_parameters_csv_command(
        &mut self,
        command: ParametersCsvCommand,
    ) -> Result<Value, ApiError>;

    /// The `parameters_csv` query: Mitcad's table and the number of
    /// parameters in it.
    fn parameters_csv_json(&self) -> Value;

    /// The `parameters_csv_preview` query.
    fn parameters_csv_preview_json(&self, query: &ParametersCsvQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> ParametersCsvOps<K> for Document<K> {
    fn run_parameters_csv_command(
        &mut self,
        command: ParametersCsvCommand,
    ) -> Result<Value, ApiError> {
        Ok(match command {
            ParametersCsvCommand::ImportParametersCsv { csv } => {
                let revision = self.revision();
                let mut answer = outcome_json(&self.import_parameters_csv(&csv)?);
                // The cascade of configuration rules (mitcad#799).
                if self.revision() != revision && !self.configuration_moves().is_empty() {
                    answer["moves"] = super::configurations::moves_json(self.configuration_moves());
                }
                answer
            }
        })
    }

    fn parameters_csv_json(&self) -> Value {
        json!({"csv": self.parameters_csv(), "parameters": self.parameters().len()})
    }

    fn parameters_csv_preview_json(&self, query: &ParametersCsvQuery) -> Result<Value, ApiError> {
        Ok(outcome_json(&self.preview_parameters_csv(&query.csv)?))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::DocumentApi;
    use mitcad_model::Document;
    use mitcad_model::test_support::block;
    use mitcad_model::testing::MockKernel;

    fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
        let result = doc
            .command(&command.to_string())
            .unwrap_or_else(|e| panic!("{command}: {e}"));
        serde_json::from_str(&result).unwrap()
    }

    fn query(doc: &Document<MockKernel>, query: Value) -> Value {
        serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
    }

    #[test]
    fn export_preview_and_import_through_the_api() {
        let mut b = block();
        let doc = &mut b.doc;
        let exported = query(doc, json!({"query": "parameters_csv"}));
        assert_eq!(exported["parameters"], 3);
        let csv = exported["csv"].as_str().unwrap().to_owned();
        assert!(
            csv.starts_with(
                "name,kind,unit,expression,value,comment,favorite,min,max,step,strict,values\r\n"
            ),
            "{csv}"
        );
        assert!(csv.contains("\r\nd1,model,mm,60 mm,60,"), "{csv}");

        let table = "Name,Unit,Expression,Value,Comments,Favorite\n\
                     d1,mm,Width + 5 mm,35,,false\n\
                     Width,mm,30 mm,30,\"Outer width, without the flange\",true\n";
        let undo_before = query(doc, json!({"query": "document"}))["undo_depth"].clone();
        let preview = query(
            doc,
            json!({"query": "parameters_csv_preview", "csv": table}),
        );
        assert_eq!(preview["format"], "parameter_table");
        assert_eq!(preview["summary"], "1 to add, 1 to change");
        assert_eq!(preview["rows"][0]["action"], "change");
        assert_eq!(preview["rows"][0]["changes"], json!(["expression"]));
        assert_eq!(preview["rows"][0]["value"], 35.0);
        assert_eq!(preview["rows"][1]["action"], "add");
        // The preview changed nothing.
        assert_eq!(
            query(doc, json!({"query": "document"}))["undo_depth"],
            undo_before
        );
        assert!(doc.parameters().find("Width").is_none());

        let done = command(doc, json!({"cmd": "import_parameters_csv", "csv": table}));
        assert_eq!(done["summary"], "1 to add, 1 to change");
        assert_eq!(done["error"], Value::Null);
        assert_eq!(
            query(doc, json!({"query": "document"}))["undo"],
            "Import Parameters"
        );
        let params = query(doc, json!({"query": "parameters"}));
        let width = params
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "Width")
            .unwrap();
        assert_eq!(width["comment"], "Outer width, without the flange");
        assert_eq!(width["favorite"], true);

        // Not a parameter CSV: refused with the reason, nothing changes.
        let error = doc
            .query(&json!({"query": "parameters_csv_preview", "csv": "a,b\n1,2\n"}).to_string())
            .unwrap_err();
        assert!(error.0.contains("no name column"), "{error}");
        let error = doc
            .command(&json!({"cmd": "import_parameters_csv", "csv": ""}).to_string())
            .unwrap_err();
        assert_eq!(error.0, "the CSV file is empty");
    }
}
