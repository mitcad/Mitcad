// SPDX-License-Identifier: MIT
//! The `evaluate` query (U1): an expression evaluated with the document's
//! parameters the way a value slot of a feature would read it, so that a
//! command panel can check a value input and show its value while the user
//! types. See `commands.md`.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use crate::document::Document;
use crate::expr::{self, DEFAULT_DECIMALS, Dims, Expr, Unit};
use crate::kernel::Kernel;
use crate::parameters::unit_for;

/// What a value input holds; bare numbers take the document's unit of it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ValueKind {
    #[default]
    Length,
    Angle,
    Unitless,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvaluateQuery {
    expression: String,
    #[serde(default)]
    kind: ValueKind,
}

impl<K: Kernel> Document<K> {
    /// `{"value"}` in millimetres or radians, `{"text"}` in the document's
    /// unit, the parameter names the expression uses and the `expression`
    /// with decimal points (as a parameter would keep it); an expression
    /// that does not evaluate to the kind rejects the query.
    pub(super) fn evaluate_json(&self, query: &EvaluateQuery) -> Result<Value, ApiError> {
        let params = self.parameters();
        let context = params.context();
        let dims = match query.kind {
            ValueKind::Length => Dims::LENGTH,
            ValueKind::Angle => Dims::ANGLE,
            ValueKind::Unitless => Dims::NONE,
        };
        let unit = unit_for(dims, &context).unwrap_or(Unit::NONE);
        let parsed = Expr::parse(&query.expression).map_err(|e| ApiError(e.to_string()))?;
        let value = parsed
            .eval_as(params.table(), &context, unit)
            .map_err(|e| ApiError(e.to_string()))?;
        if !value.value.is_finite() {
            return Err(ApiError(format!(
                "{} is not a finite value",
                query.expression
            )));
        }
        Ok(json!({
            "value": value.value,
            "text": expr::format_value(value.value, unit, DEFAULT_DECIMALS),
            "references": parsed.references(),
            "expression": expr::with_decimal_points(&query.expression),
        }))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::document::Document;
    use crate::testing::MockKernel;

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
        assert!(error.unwrap_err().contains("'max(1; 5)'"));
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
