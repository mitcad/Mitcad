// SPDX-License-Identifier: MIT
//! The `evaluate` query (U1): an expression evaluated with the document's
//! parameters the way a value slot of a feature would read it, so that a
//! command panel can check a value input and show its value while the user
//! types. See `commands.md`.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use mitcad_model::Document;
use mitcad_model::expr::{self, DEFAULT_DECIMALS, Dims, Expr, Limited, Unit};
use mitcad_model::kernel::Kernel;
use mitcad_model::parameters::unit_for;

/// What a value input holds; bare numbers take the document's unit of it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ValueKind {
    #[default]
    Length,
    Angle,
    Unitless,
    /// A text (mitcad#664): a sketch text's value input.
    Text,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvaluateQuery {
    expression: String,
    #[serde(default)]
    kind: ValueKind,
    #[serde(default)]
    printed_fits: bool,
    /// Evaluate as this parameter's new expression, in its unit, and say
    /// what its limits would make of it (mitcad#665).
    #[serde(default)]
    parameter: Option<String>,
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait EvaluateOps<K: Kernel> {
    /// `{"value"}` in millimetres or radians, `{"text"}` in the document's
    /// unit, the parameter names the expression uses and the `expression`
    /// with decimal points (as a parameter would keep it); an expression
    /// that does not evaluate to the kind rejects the query.
    fn evaluate_json(&self, query: &EvaluateQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> EvaluateOps<K> for Document<K> {
    fn evaluate_json(&self, query: &EvaluateQuery) -> Result<Value, ApiError> {
        let virtual_params;
        let params = if query.printed_fits {
            virtual_params =
                mitcad_model::internal::print_fits::evaluation_parameters(self.parameters())
                    .map_err(|e| ApiError(e.to_string()))?;
            &virtual_params
        } else {
            self.parameters()
        };
        let context = params.context();
        let dims = match query.kind {
            ValueKind::Length => Dims::LENGTH,
            ValueKind::Angle => Dims::ANGLE,
            ValueKind::Unitless | ValueKind::Text => Dims::NONE,
        };
        let mut unit = match query.kind {
            ValueKind::Text => Unit::TEXT,
            _ => unit_for(dims, &context).unwrap_or(Unit::NONE),
        };
        // A parameter's new expression (mitcad#665): its unit and limits.
        let parameter = match &query.parameter {
            Some(name) => {
                let id = params
                    .find(name)
                    .ok_or_else(|| ApiError(format!("unknown parameter {name}")))?;
                unit = params.get(id).expect("found").unit();
                Some(id)
            }
            None => None,
        };
        let parsed = Expr::parse(&query.expression).map_err(|e| ApiError(e.to_string()))?;
        // The table columns it looks values up in (mitcad#667): `Screws.dk`.
        let table_references: Vec<String> = parsed
            .table_references()
            .into_iter()
            .map(|(table, column)| format!("{table}.{column}"))
            .collect();
        // A text (mitcad#664): `text` is the text; no number, no limits.
        if unit.is_text() {
            let text = parsed
                .eval_text(params.table(), &context)
                .map_err(|e| ApiError(e.to_string()))?;
            return Ok(json!({
                "text": text,
                "references": parsed.references(),
                "table_references": table_references,
                "expression": expr::with_decimal_points(&query.expression),
            }));
        }
        let value = parsed
            .eval_as(params.table(), &context, unit)
            .map_err(|e| ApiError(e.to_string()))?;
        if !value.value.is_finite() {
            return Err(ApiError(format!(
                "{} is not a finite value",
                query.expression
            )));
        }
        let mut out = json!({
            "value": value.value,
            "text": expr::format_value(value.value, unit, DEFAULT_DECIMALS),
            "references": parsed.references(),
            "table_references": table_references,
            "expression": expr::with_decimal_points(&query.expression),
        });
        if let Some(id) = parameter {
            // What its limits make of it; a strict parameter's refusal
            // rejects the query with its reason.
            let (limited_value, _, limited) = params
                .table()
                .try_expression(id, &query.expression)
                .map_err(|e| ApiError(e.to_string()))?;
            out["limited"] = json!(limited.map(Limited::as_str));
            out["limited_value"] = json!(limited_value.value);
            out["limited_text"] = json!(expr::format_value(
                limited_value.value,
                unit,
                DEFAULT_DECIMALS
            ));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::DocumentApi;
    use mitcad_model::Document;
    use mitcad_model::testing::MockKernel;

    fn evaluate(doc: &Document<MockKernel>, query: Value) -> Result<Value, String> {
        doc.query(&query.to_string())
            .map(|r| serde_json::from_str(&r).unwrap())
            .map_err(|e| e.0)
    }

    #[test]
    fn evaluates_values_as_slots_read_them() {
        let mut doc = Document::new(MockKernel::default());
        doc.command(r#"{"cmd": "add_parameter", "name": "d1", "value": 20}"#)
            .unwrap();
        let result = evaluate(&doc, json!({"query": "evaluate", "expression": "d1 * 2"})).unwrap();
        assert_eq!(result["value"], json!(40.0));
        assert_eq!(result["text"], json!("40 mm"));
        assert_eq!(result["references"], json!(["d1"]));
        // A bare number takes the document's unit of the kind.
        let angle = evaluate(
            &doc,
            json!({"query": "evaluate", "expression": "90", "kind": "angle"}),
        )
        .unwrap();
        assert!((angle["value"].as_f64().unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert_eq!(angle["text"], json!("90 deg"));
        let count = evaluate(
            &doc,
            json!({"query": "evaluate", "expression": "3", "kind": "unitless"}),
        )
        .unwrap();
        assert_eq!(count["value"], json!(3.0));
        assert_eq!(count["expression"], json!("3"));
    }

    #[test]
    fn decimal_commas_come_back_as_points() {
        let mut doc = Document::new(MockKernel::default());
        doc.command(r#"{"cmd": "add_parameter", "name": "d1", "expression": "2,5 mm"}"#)
            .unwrap();
        let result = evaluate(
            &doc,
            json!({"query": "evaluate", "expression": "d1 * 1,5 + max(d1, 2)"}),
        )
        .unwrap();
        assert_eq!(result["value"], json!(6.25));
        assert_eq!(result["expression"], json!("d1 * 1.5 + max(d1, 2)"));
        let error = evaluate(&doc, json!({"query": "evaluate", "expression": "max(1,5)"}));
        assert!(error.unwrap_err().contains("“max(1; 5)”"));
    }

    #[test]
    fn evaluates_texts() {
        // mitcad#664: a text input, and a text parameter's new expression.
        let mut doc = Document::new(MockKernel::default());
        doc.command(r#"{"cmd": "add_parameter", "name": "GridX", "expression": "6"}"#)
            .unwrap();
        doc.command(r#"{"cmd": "add_parameter", "name": "Label", "expression": "'Box'"}"#)
            .unwrap();
        let result = evaluate(
            &doc,
            json!({"query": "evaluate", "expression": "Label & ' ' & GridX", "kind": "text"}),
        )
        .unwrap();
        assert_eq!(result["text"], json!("Box 6"));
        assert_eq!(result["references"], json!(["Label", "GridX"]));
        assert!(result.get("value").is_none());
        let result = evaluate(
            &doc,
            json!({"query": "evaluate", "expression": "'Lid ' & GridX", "parameter": "Label"}),
        )
        .unwrap();
        assert_eq!(result["text"], json!("Lid 6"));
        // A number is no text, a text no length.
        let error = evaluate(
            &doc,
            json!({"query": "evaluate", "expression": "GridX", "kind": "text"}),
        )
        .unwrap_err();
        assert!(error.contains("a number where text is expected"), "{error}");
        let error =
            evaluate(&doc, json!({"query": "evaluate", "expression": "Label"})).unwrap_err();
        assert!(error.contains("text where a number is expected"), "{error}");
    }

    #[test]
    fn rejects_unknown_names_syntax_and_wrong_units() {
        let doc = Document::new(MockKernel::default());
        let unknown = evaluate(&doc, json!({"query": "evaluate", "expression": "d9 * 2"}));
        assert!(unknown.unwrap_err().contains("d9"));
        assert!(evaluate(&doc, json!({"query": "evaluate", "expression": "2 *"})).is_err());
        assert!(
            evaluate(
                &doc,
                json!({"query": "evaluate", "expression": "30 deg", "kind": "length"})
            )
            .is_err()
        );
    }
}
