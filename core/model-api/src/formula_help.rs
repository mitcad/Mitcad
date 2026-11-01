// SPDX-License-Identifier: MIT
//! Read-only assistance for formula fields and the larger formula editor.

use super::tables::TablesOps;
use super::{ApiError, evaluate::ValueKind};
use mitcad_model::Document;
use mitcad_model::expr::{self, Dims, Expr, Span, Unit, assistance};
use mitcad_model::kernel::Kernel;
use mitcad_model::parameters::unit_for;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormulaHelpQuery {
    #[serde(default)]
    expression: String,
    #[serde(default)]
    cursor: usize,
    #[serde(default)]
    selection_end: Option<usize>,
    #[serde(default)]
    kind: ValueKind,
    #[serde(default)]
    parameter: Option<String>,
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait FormulaHelpOps<K: Kernel> {
    fn formula_help_json(&self, query: &FormulaHelpQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> FormulaHelpOps<K> for Document<K> {
    fn formula_help_json(&self, query: &FormulaHelpQuery) -> Result<Value, ApiError> {
        let catalog: Vec<Value> = assistance::function_catalog()
            .map(|help| {
                json!({
                    "name": help.name.to_ascii_uppercase(), "arguments": help.arguments,
                    "description": help.description, "example": help.example,
                })
            })
            .collect();
        let mut result =
            json!({"functions": catalog,"formatted":assistance::indent_let(&query.expression)});
        // The tables and their columns, for completion (mitcad#667).
        result["tables"] = self.table_names_json();
        if let Some((function, argument)) =
            assistance::signature_at(&query.expression, query.cursor)
        {
            let help = assistance::function_help(function);
            result["signature"] = json!({"name":help.name.to_ascii_uppercase(),
                "arguments":help.arguments,"argument":argument,"description":help.description});
        }
        if query.expression.trim().is_empty() {
            return Ok(result);
        }
        let params = self.parameters();
        let context = params.context();
        let dims = match query.kind {
            ValueKind::Length => Dims::LENGTH,
            ValueKind::Angle => Dims::ANGLE,
            ValueKind::Unitless | ValueKind::Text => Dims::NONE,
        };
        let unit = if let Some(name) = &query.parameter {
            let id = params
                .find(name)
                .ok_or_else(|| ApiError(format!("unknown parameter {name}")))?;
            params.get(id).expect("found parameter").unit()
        } else if query.kind == ValueKind::Text {
            Unit::TEXT
        } else {
            unit_for(dims, &context).unwrap_or(Unit::NONE)
        };
        let expression = match Expr::parse(&query.expression) {
            Ok(value) => value,
            Err(error) => {
                result["error"] = json!({"message":error.to_string(),"start":error.span.start,"end":error.span.end});
                return Ok(result);
            }
        };
        // A text (mitcad#664): its text as the result; no number to inspect.
        if unit.is_text() {
            match expression.eval_text(params.table(), &context) {
                Ok(text) => result["result"] = json!({"text": text}),
                Err(error) => {
                    let span = error.span().unwrap_or(Span::new(0, query.expression.len()));
                    result["error"] =
                        json!({"message":error.to_string(),"start":span.start,"end":span.end});
                }
            }
            return Ok(result);
        }
        match expression.eval_as(params.table(), &context, unit) {
            Ok(value) => {
                result["result"] = json!({"value":value.value,"text":expr::format_value(value.value,unit,expr::DEFAULT_DECIMALS)});
            }
            Err(error) => {
                let span = error.span().unwrap_or(Span::new(0, query.expression.len()));
                result["error"] =
                    json!({"message":error.to_string(),"start":span.start,"end":span.end});
            }
        }
        let selected = Span::new(
            query.cursor,
            query
                .selection_end
                .unwrap_or(query.cursor)
                .max(query.cursor),
        );
        let (span, value) = assistance::inspect(
            &expression,
            selected,
            params.table(),
            &context.for_unit(unit),
        );
        match value {
            Ok(value) => {
                let shown = unit_for(value.dims, &context)
                    .map(|u| expr::format_value(value.value, u, expr::DEFAULT_DECIMALS))
                    .unwrap_or_else(|| {
                        format!(
                            "{} {}",
                            expr::format_number(value.value, expr::DEFAULT_DECIMALS),
                            value.dims
                        )
                    });
                result["selection"] =
                    json!({"start":span.start,"end":span.end,"text":shown,"value":value.value});
            }
            Err(error) => {
                let at = error.span().unwrap_or(span);
                result["selection"] =
                    json!({"start":at.start,"end":at.end,"error":error.to_string()});
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentApi;
    use mitcad_model::testing::MockKernel;
    fn help(doc: &Document<MockKernel>, input: Value) -> Value {
        serde_json::from_str(&doc.query(&input.to_string()).unwrap()).unwrap()
    }
    #[test]
    fn incomplete_signature_and_localized_error_keep_the_catalog_available() {
        let doc = Document::new(MockKernel::default());
        let value = help(
            &doc,
            json!({"query":"formula_help","expression":"FLOOR(","cursor":6}),
        );
        assert_eq!(value["signature"]["name"], "FLOOR");
        assert_eq!(value["signature"]["argument"], 0);
        assert_eq!(
            value["functions"].as_array().unwrap().len(),
            mitcad_model::expr::assistance::function_catalog().count()
        );
        assert!(value["error"]["start"].is_number());
    }
    #[test]
    fn selected_subexpressions_preserve_units_and_never_mutate_parameters() {
        let mut doc = Document::new(MockKernel::default());
        doc.command(r#"{"cmd":"add_parameter","name":"length","expression":"40 mm"}"#)
            .unwrap();
        doc.command(r#"{"cmd":"add_parameter","name":"pitch","expression":"5 mm"}"#)
            .unwrap();
        let before = doc.query(r#"{"query":"parameters"}"#).unwrap();
        let value = help(
            &doc,
            json!({"query":"formula_help","expression":"length / pitch","cursor":0,"selection_end":14,"kind":"unitless"}),
        );
        assert_eq!(value["selection"]["text"], "8");
        assert_eq!(value["result"]["value"], 8.0);
        let error = help(
            &doc,
            json!({"query":"formula_help","expression":"length / 0","cursor":10}),
        );
        assert!(error["error"]["message"].as_str().unwrap().contains("zero"));
        assert_eq!(before, doc.query(r#"{"query":"parameters"}"#).unwrap());
    }
    #[test]
    fn text_formulas_give_their_text() {
        let mut doc = Document::new(MockKernel::default());
        doc.command(r#"{"cmd":"add_parameter","name":"Width","expression":"15 mm"}"#)
            .unwrap();
        doc.command(r#"{"cmd":"add_parameter","name":"Label","expression":"'6qt'"}"#)
            .unwrap();
        let value = help(
            &doc,
            json!({"query":"formula_help","expression":"'Box ' & Width","cursor":0,"kind":"text"}),
        );
        assert_eq!(value["result"]["text"], "Box 15 mm");
        let value = help(
            &doc,
            json!({"query":"formula_help","expression":"Label & \"!\"","cursor":0,"parameter":"Label"}),
        );
        assert_eq!(value["result"]["text"], "6qt!");
        let error = help(
            &doc,
            json!({"query":"formula_help","expression":"Width","cursor":0,"kind":"text"}),
        );
        assert!(error["error"]["message"].is_string());
    }
}
