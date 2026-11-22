// SPDX-License-Identifier: MIT
//! Read-only assistance for formula fields and the larger formula editor.

use super::tables::TablesOps;
use super::{ApiError, evaluate::ValueKind};
use mitcad_model::Document;
use mitcad_model::expr::assistance::TokenKind;
use mitcad_model::expr::{self, Dims, Expr, Span, Unit, assistance};
use mitcad_model::kernel::Kernel;
use mitcad_model::parameters::unit_for;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormulaHelpQuery {
    /// The unit the value is to have, a new parameter's as Add User
    /// Parameter checks it while it is typed (mitcad#2181): "mm", "deg",
    /// "mm^2", "" for a number, "Text"; in place of `kind`'s.
    #[serde(default)]
    unit: Option<String>,
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
    /// Only `tokens`, for colouring the text on each key (mitcad#2179).
    #[serde(default)]
    tokens_only: bool,
}

/// The formula's pieces with their kinds (mitcad#2179): byte ranges, and a
/// name no parameter of the design has, or a table it has none of, as
/// `unknown`.
fn tokens_json<K: Kernel>(doc: &Document<K>, text: &str) -> Value {
    let params = doc.parameters();
    Value::Array(
        assistance::tokens(text)
            .into_iter()
            .map(|token| {
                let source = &text[token.span.start..token.span.end];
                let unknown = match token.kind {
                    TokenKind::Name => params.find(source).is_none(),
                    TokenKind::Table => doc.table(source).is_none(),
                    _ => false,
                };
                let kind = if unknown {
                    "unknown"
                } else {
                    token.kind.name()
                };
                json!({"kind": kind, "start": token.span.start, "end": token.span.end})
            })
            .collect(),
    )
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait FormulaHelpOps<K: Kernel> {
    fn formula_help_json(&self, query: &FormulaHelpQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> FormulaHelpOps<K> for Document<K> {
    fn formula_help_json(&self, query: &FormulaHelpQuery) -> Result<Value, ApiError> {
        if query.tokens_only {
            return Ok(json!({"tokens": tokens_json(self, &query.expression)}));
        }
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
        // The units and the text's pieces (mitcad#2179).
        result["units"] = json!(assistance::unit_names().collect::<Vec<_>>());
        result["tokens"] = tokens_json(self, &query.expression);
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
        } else if let Some(unit) = &query.unit {
            super::parse_unit(unit)?
        } else if query.kind == ValueKind::Text {
            Unit::TEXT
        } else {
            unit_for(dims, &context).unwrap_or(Unit::NONE)
        };
        let expression = match Expr::parse(&query.expression) {
            Ok(value) => value,
            Err(error) => {
                result["error"] = formula_error(error.message(), error.to_string(), error.span);
                return Ok(result);
            }
        };
        // A text (mitcad#664): its text as the result; no number to inspect.
        if unit.is_text() {
            match expression.eval_text(params.table(), &context) {
                Ok(text) => result["result"] = json!({"text": text}),
                Err(error) => {
                    let span = error.span().unwrap_or(Span::new(0, query.expression.len()));
                    result["error"] = formula_error(
                        eval_words(&error, params.table(), query),
                        error.to_string(),
                        span,
                    );
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
                result["error"] = formula_error(
                    eval_words(&error, params.table(), query),
                    error.to_string(),
                    span,
                );
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
                result["selection"] = json!({"start":at.start,"end":at.end,"error":error.message(),"detail":error.to_string()});
            }
        }
        Ok(result)
    }
}

/// The formula's error (mitcad#2181): `message` in words a person reads,
/// without the parser's byte offset, which the field marks from `start`
/// and `end`; `detail` the agents' and the log's text with the offset.
fn formula_error(message: String, detail: String, span: Span) -> Value {
    json!({"message": message, "detail": detail, "start": span.start, "end": span.end})
}

/// An evaluation error in words (mitcad#2181): an unknown name with its
/// near miss among the design's parameters, other than the parameter whose
/// formula it is.
fn eval_words(
    error: &expr::EvalError,
    parameters: &expr::ParameterTable,
    query: &FormulaHelpQuery,
) -> String {
    parameters.eval_message(error, query.parameter.as_deref().unwrap_or_default())
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
    fn tokens_mark_unknown_names_and_come_alone_on_request() {
        let mut doc = Document::new(MockKernel::default());
        doc.command(r#"{"cmd":"add_parameter","name":"width","expression":"40 mm"}"#)
            .unwrap();
        let text = "width + spacing * 2 mm";
        let value = help(
            &doc,
            json!({"query":"formula_help","expression":text,"cursor":0,"tokens_only":true}),
        );
        // Only the tokens: no catalog, no evaluation.
        assert_eq!(value.as_object().unwrap().len(), 1);
        let kinds: Vec<(String, String)> = value["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let start = t["start"].as_u64().unwrap() as usize;
                let end = t["end"].as_u64().unwrap() as usize;
                (
                    text[start..end].to_owned(),
                    t["kind"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let expected = [
            ("width", "name"),
            ("+", "operator"),
            ("spacing", "unknown"),
            ("*", "operator"),
            ("2", "number"),
            ("mm", "unit"),
        ];
        assert_eq!(
            kinds,
            expected
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect::<Vec<_>>()
        );
        // The whole answer has them too, and the units for completion.
        let value = help(
            &doc,
            json!({"query":"formula_help","expression":text,"cursor":0}),
        );
        assert_eq!(value["tokens"].as_array().unwrap().len(), expected.len());
        assert!(value["units"].as_array().unwrap().iter().any(|u| u == "mm"));
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
