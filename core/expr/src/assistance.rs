// SPDX-License-Identifier: MIT
//! Formula editor assistance, using the expression tokenizer and syntax tree.

use super::ast::FUNCTIONS;
use super::lexer::{Tok, lex};
use super::parser::ParseErrorKind;
use super::units::{base_unit, is_unit_name};
use super::{
    Constant, EvalContext, EvalError, Expr, Function, Lookup, Node, NodeKind, Quantity, Span,
};

/// Help for a supported function. Examples are ordinary editable formulas.
pub struct FunctionHelp {
    pub name: &'static str,
    pub arguments: &'static [&'static str],
    pub description: &'static str,
    pub example: &'static str,
}

pub fn function_help(function: Function) -> FunctionHelp {
    use Function::*;
    let (arguments, description, example): (&'static [&'static str], &'static str, &'static str) =
        match function {
            Sin => (&["angle"], "Sine of an angle", "SIN(30 deg)"),
            Cos => (&["angle"], "Cosine of an angle", "COS(60 deg)"),
            Tan => (&["angle"], "Tangent of an angle", "TAN(45 deg)"),
            Asin => (&["value"], "Inverse sine, as an angle", "ASIN(0.5)"),
            Acos => (&["value"], "Inverse cosine, as an angle", "ACOS(0.5)"),
            Atan => (&["value"], "Inverse tangent, as an angle", "ATAN(1)"),
            Atan2 => (&["y", "x"], "Angle from two coordinates", "ATAN2(3; 4)"),
            Sinh => (&["value"], "Hyperbolic sine", "SINH(1)"),
            Cosh => (&["value"], "Hyperbolic cosine", "COSH(1)"),
            Tanh => (&["value"], "Hyperbolic tangent", "TANH(1)"),
            Asinh => (&["value"], "Inverse hyperbolic sine", "ASINH(1)"),
            Acosh => (&["value"], "Inverse hyperbolic cosine", "ACOSH(2)"),
            Atanh => (&["value"], "Inverse hyperbolic tangent", "ATANH(0.5)"),
            Sqrt => (
                &["value"],
                "Square root, retaining compatible units",
                "SQRT(25 mm^2)",
            ),
            Abs => (&["value"], "Absolute value", "ABS(-5 mm)"),
            Sign => (&["value"], "Sign: minus one, zero or one", "SIGN(-5)"),
            Exp => (&["value"], "Exponential of a number", "EXP(1)"),
            Ln => (&["value"], "Natural logarithm", "LN(10)"),
            Log => (&["value"], "Base-ten logarithm", "LOG(100)"),
            Floor => (
                &["value", "[step]"],
                "Round down, optionally to a step",
                "FLOOR(23 mm; 5 mm)",
            ),
            Ceil | Ceiling => (
                &["value", "[step]"],
                "Round up, optionally to a step",
                "CEILING(23 mm; 5 mm)",
            ),
            Round => (
                &["value", "[digits]"],
                "Round to a number of decimal places",
                "ROUND(12.345; 2)",
            ),
            RoundUp => (
                &["value", "[digits]"],
                "Round away from zero",
                "ROUNDUP(12.345; 2)",
            ),
            RoundDown | Trunc => (
                &["value", "[digits]"],
                "Truncate toward zero",
                "TRUNC(12.345; 2)",
            ),
            MRound => (
                &["value", "step"],
                "Round to the nearest step",
                "MROUND(23 mm; 5 mm)",
            ),
            Int => (&["value"], "Round down to a whole number", "INT(4.8)"),
            Mod => (
                &["value", "divisor"],
                "Remainder with the divisor's sign",
                "MOD(11; 3)",
            ),
            Quotient => (
                &["value", "divisor"],
                "Whole-number part of a quotient",
                "QUOTIENT(11; 3)",
            ),
            Even => (
                &["value"],
                "Round away from zero to an even whole number",
                "EVEN(5)",
            ),
            Odd => (
                &["value"],
                "Round away from zero to an odd whole number",
                "ODD(4)",
            ),
            Min => (
                &["value1", "value2", "..."],
                "Smallest value, also of a table column",
                "MIN(12 mm; 8 mm)",
            ),
            Max => (
                &["value1", "value2", "..."],
                "Largest value, also of a table column",
                "MAX(12 mm; 8 mm)",
            ),
            Sum => (
                &["value1", "..."],
                "Sum of values, also of a table column",
                "SUM(2 mm; 3 mm; 4 mm)",
            ),
            Average => (
                &["value1", "..."],
                "Arithmetic mean of values, also of a table column",
                "AVERAGE(2 mm; 4 mm)",
            ),
            Product => (&["value1", "..."], "Product of values", "PRODUCT(2; 3; 4)"),
            Median => (
                &["value1", "..."],
                "Middle value after sorting, also of a table column",
                "MEDIAN(2; 9; 4)",
            ),
            Pow => (
                &["value", "exponent"],
                "Raise a value to a power",
                "POW(3; 2)",
            ),
            Clamp => (
                &["value", "minimum", "maximum"],
                "Keep a formula result within bounds",
                "CLAMP(12 mm; 5 mm; 10 mm)",
            ),
            If => (
                &["condition", "if_true", "if_false"],
                "Choose between two values",
                "IF(3 > 2; 10 mm; 5 mm)",
            ),
            And => (
                &["condition1", "..."],
                "True when every condition is true",
                "AND(3 > 2; 5 < 9)",
            ),
            Or => (
                &["condition1", "..."],
                "True when any condition is true",
                "OR(3 > 8; 5 < 9)",
            ),
            Not => (&["condition"], "Reverse a condition", "NOT(3 > 8)"),
            Ifs => (
                &["condition1", "value1", "..."],
                "Value of the first true condition",
                "IFS(3 < 2; 5; 1; 10)",
            ),
            Switch => (
                &["value", "case1", "result1", "...", "[default]"],
                "Choose a result by matching a value",
                "SWITCH(2; 1; 10; 2; 20; 0)",
            ),
            Choose => (
                &["index", "value1", "..."],
                "Choose a value by its one-based index",
                "CHOOSE(2; 10; 20; 30)",
            ),
            IfError => (
                &["value", "fallback"],
                "Use a fallback when a value fails",
                "IFERROR(1 / 0; 0)",
            ),
            Let => (
                &["name", "value", "...", "result"],
                "Name intermediate values inside one formula",
                "LET(pitch; 5 mm; width; 40 mm; FLOOR(width / pitch))",
            ),
            Value => (
                &["value", "unit"],
                "Read a quantity as a number in the given unit",
                "VALUE(25.4 mm; \"in\")",
            ),
            Radians => (&["degrees"], "Convert degrees to radians", "RADIANS(180)"),
            Degrees => (&["radians"], "Convert radians to degrees", "DEGREES(PI)"),
            Concat => (
                &["text", "text"],
                "Join texts and numbers as text",
                "CONCAT(\"Box \"; Width)",
            ),
            Text => (
                &["value", "format"],
                "Write a number as text, optionally in a format",
                "TEXT(Width; \"0.00\")",
            ),
            Left => (
                &["text", "count"],
                "The first characters of a text",
                "LEFT(\"Box\"; 2)",
            ),
            Right => (
                &["text", "count"],
                "The last characters of a text",
                "RIGHT(\"Box\"; 2)",
            ),
            Len => (
                &["text"],
                "The number of characters in a text",
                "LEN(\"Box\")",
            ),
            XLookup => (
                &["key", "keys", "results", "[if_not_found]", "[mode]"],
                "Look a key up in a table column and give the cell of another column in that \
                 row; mode 0 exact, -1 or the next smaller, 1 or the next larger",
                "XLOOKUP('M5'; Screws.Size; Screws.dk)",
            ),
            Match => (
                &["key", "keys", "[mode]"],
                "The row number of a key in a table column; mode 0 exact, 1 or the next \
                 smaller, -1 or the next larger",
                "MATCH('M5'; Screws.Size)",
            ),
            Index => (
                &["column", "row"],
                "The cell of a table column in a row, counting from one",
                "INDEX(Screws.dk; 3)",
            ),
        };
    FunctionHelp {
        name: function.name(),
        arguments,
        description,
        example,
    }
}

pub fn function_catalog() -> impl Iterator<Item = FunctionHelp> {
    FUNCTIONS
        .iter()
        .map(|(function, _, _)| function_help(*function))
}

/// Innermost unfinished function call at a byte cursor, even before it parses.
pub fn signature_at(text: &str, cursor: usize) -> Option<(Function, usize)> {
    let cursor = cursor.min(text.len());
    if !text.is_char_boundary(cursor) {
        return None;
    }
    let tokens = match lex(&text[..cursor]) {
        Ok(tokens) => tokens,
        // Keep the known call stack while a unit string or number is being
        // typed. A lexical error carries the start of that unfinished token.
        Err(error) => lex(&text[..error.span.start]).ok()?,
    };
    let mut calls: Vec<(Option<Function>, usize)> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.tok {
            Tok::LParen => {
                let function = index.checked_sub(1).and_then(|previous| {
                    let t = tokens[previous];
                    (t.tok == Tok::Ident)
                        .then(|| Function::from_name(&text[t.span.start..t.span.end]))
                        .flatten()
                });
                calls.push((function, 0));
            }
            Tok::RParen => {
                calls.pop();
            }
            Tok::Semicolon | Tok::Comma => {
                if let Some((_, argument)) = calls.last_mut() {
                    *argument += 1;
                }
            }
            _ => {}
        }
    }
    calls
        .into_iter()
        .rev()
        .find_map(|(function, argument)| function.map(|f| (f, argument)))
}

/// Indent LET bindings without rewriting text inside quoted tokens.
pub fn indent_let(text: &str) -> String {
    let Ok(tokens) = lex(text) else {
        return text.to_owned();
    };
    let mut result = String::new();
    let mut previous = 0;
    let mut stack: Vec<(bool, usize)> = Vec::new();
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        if token.tok == Tok::RParen && stack.last().is_some_and(|(is_let, _)| *is_let) {
            depth = depth.saturating_sub(1);
            while result.ends_with(char::is_whitespace) {
                result.pop();
            }
            result.push('\n');
            result.push_str(&"    ".repeat(depth));
            previous = token.span.start;
        }
        result.push_str(&text[previous..token.span.end]);
        previous = token.span.end;
        let newline = match token.tok {
            Tok::LParen => {
                let is_let = index.checked_sub(1).is_some_and(|previous| {
                    let name = tokens[previous];
                    name.tok == Tok::Ident
                        && text[name.span.start..name.span.end].eq_ignore_ascii_case("let")
                });
                stack.push((is_let, 0));
                if is_let {
                    depth += 1;
                }
                is_let
            }
            Tok::RParen => {
                stack.pop();
                false
            }
            Tok::Semicolon | Tok::Comma => {
                if let Some((is_let, argument)) = stack.last_mut() {
                    *argument += 1;
                    *is_let && *argument % 2 == 0
                } else {
                    false
                }
            }
            _ => false,
        };
        if newline {
            result.push('\n');
            result.push_str(&"    ".repeat(depth));
            previous = tokens
                .get(index + 1)
                .map_or(text.len(), |next| next.span.start);
        }
    }
    result.push_str(&text[previous..]);
    result
}

/// The smallest syntax node containing a cursor or selected byte range.
/// Enclosing LET bindings remain in scope when evaluating that node.
pub fn inspect(
    expression: &Expr,
    selected: Span,
    lookup: &dyn Lookup,
    context: &EvalContext,
) -> (Span, Result<Quantity, EvalError>) {
    fn visit(
        node: &Node,
        selected: Span,
        scopes: &[Vec<Node>],
        best: &mut Option<(Node, Vec<Vec<Node>>)>,
    ) {
        if selected.start < node.span.start || selected.end > node.span.end {
            return;
        }
        *best = Some((node.clone(), scopes.to_vec()));
        match &node.kind {
            NodeKind::Neg(child) | NodeKind::WithUnit { value: child, .. } => {
                visit(child, selected, scopes, best)
            }
            NodeKind::Binary { lhs, rhs, .. } => {
                visit(lhs, selected, scopes, best);
                visit(rhs, selected, scopes, best);
            }
            NodeKind::Call { function, args } => {
                for (index, child) in args.iter().enumerate() {
                    let mut nested = scopes.to_vec();
                    if *function == Function::Let {
                        let pairs = if index == args.len() - 1 {
                            index
                        } else {
                            index / 2 * 2
                        };
                        if pairs > 0 {
                            nested.push(args[..pairs].to_vec());
                        }
                        if index != args.len() - 1 && index % 2 == 0 {
                            if selected.start >= child.span.start && selected.end <= child.span.end
                            {
                                // A declaration's name describes its bound value,
                                // with preceding bindings in scope, not the LET result.
                                let mut value = args[index + 1].clone();
                                value.span = child.span;
                                *best = Some((value, nested));
                            }
                            continue;
                        }
                    }
                    visit(child, selected, &nested, best);
                }
            }
            _ => {}
        }
    }
    let mut best = None;
    visit(expression.root(), selected, &[], &mut best);
    let (mut node, scopes) = best.unwrap_or_else(|| (expression.root().clone(), Vec::new()));
    let span = node.span;
    for mut bindings in scopes.into_iter().rev() {
        bindings.push(node);
        node = Node::new(
            NodeKind::Call {
                function: Function::Let,
                args: bindings,
            },
            span,
        );
    }
    (span, Expr::from_node(node).eval(lookup, context))
}

/// What a piece of a formula's text is, for the formula editor's colours
/// (mitcad#2179). The kinds come from the tokenizer the parser uses and
/// from where a name stands, never from a second parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Number,
    /// A unit after a value (`mm`, `deg`, the inch mark), or a unit's name
    /// where a value stands (an error the parser reports).
    Unit,
    /// A reference to a parameter (whether one has that name is the
    /// design's to tell).
    Name,
    /// A name a `LET` binds, where it is bound and where it is used.
    Local,
    /// The table of a column reference, `Screws` of `Screws.dk`.
    Table,
    /// The column of a column reference, `dk` of `Screws.dk`.
    Column,
    Function,
    /// `LET` and the constants (`PI`, `E`).
    Keyword,
    /// `+ - * / % ^ & < <= > >= = == <> !=` and the `.` of a column.
    Operator,
    Bracket,
    /// `;` and `,` between arguments.
    Separator,
    /// Quoted text, also one not closed yet.
    Text,
    /// What cannot stand there: a character the tokenizer refuses, a
    /// name after a value that is no unit, an unknown function, a name
    /// the design cannot evaluate.
    Error,
}

impl TokenKind {
    /// The kind's name in the `formula_help` query's `tokens`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Unit => "unit",
            Self::Name => "name",
            Self::Local => "local",
            Self::Table => "table",
            Self::Column => "column",
            Self::Function => "function",
            Self::Keyword => "keyword",
            Self::Operator => "operator",
            Self::Bracket => "bracket",
            Self::Separator => "separator",
            Self::Text => "text",
            Self::Error => "error",
        }
    }
}

/// A piece of a formula's text and what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormulaToken {
    pub kind: TokenKind,
    pub span: Span,
}

/// The formula's text in pieces, in order, for colouring it while it is
/// typed: also an unfinished or wrong formula. A part the tokenizer refuses
/// is an [`TokenKind::Error`] piece and the text after it is cut again; a
/// quoted text not closed yet runs to the end. White space is in no piece.
/// Nothing for a formula too long to evaluate.
pub fn tokens(text: &str) -> Vec<FormulaToken> {
    let mut raw: Vec<(Option<Tok>, Span)> = Vec::new();
    let shifted = |span: Span, by: usize| Span::new(span.start + by, span.end + by);
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let error = match lex(rest) {
            Ok(found) => {
                raw.extend(
                    found
                        .into_iter()
                        .map(|t| (Some(t.tok), shifted(t.span, at))),
                );
                break;
            }
            Err(error) => error,
        };
        if matches!(error.kind, ParseErrorKind::TooComplex) {
            return Vec::new();
        }
        let start = error.span.start.min(rest.len());
        if let Ok(found) = lex(&rest[..start]) {
            raw.extend(
                found
                    .into_iter()
                    .map(|t| (Some(t.tok), shifted(t.span, at))),
            );
        }
        if matches!(error.kind, ParseErrorKind::UnterminatedText) {
            raw.push((Some(Tok::Str), Span::new(at + start, text.len())));
            break;
        }
        // At least one character, on a character's boundary.
        let mut end = error.span.end.clamp(start, rest.len());
        if end == start {
            end = start + rest[start..].chars().next().map_or(1, char::len_utf8);
        }
        while end < rest.len() && !rest.is_char_boundary(end) {
            end += 1;
        }
        raw.push((None, Span::new(at + start, at + end)));
        at += end;
    }
    classify(text, &raw)
}

/// The unit names a formula may write after a value (`mm`, `deg`), for
/// completion.
pub fn unit_names() -> impl Iterator<Item = &'static str> {
    super::units::unit_names()
}

fn classify(text: &str, raw: &[(Option<Tok>, Span)]) -> Vec<FormulaToken> {
    let locals = let_locals(text, raw);
    let mut result: Vec<FormulaToken> = Vec::with_capacity(raw.len());
    for (index, &(tok, span)) in raw.iter().enumerate() {
        let kind = match tok {
            None => TokenKind::Error,
            Some(Tok::Number(_)) => TokenKind::Number,
            Some(Tok::Str) => TokenKind::Text,
            Some(Tok::LParen | Tok::RParen) => TokenKind::Bracket,
            Some(Tok::Semicolon | Tok::Comma) => TokenKind::Separator,
            Some(Tok::Ident) => {
                let previous = index.checked_sub(1).map(|i| (raw[i].0, result[i].kind));
                let next = |n: usize| raw.get(index + n).and_then(|t| t.0);
                ident_kind(
                    &text[span.start..span.end],
                    previous,
                    next(1),
                    next(2),
                    &locals,
                )
            }
            Some(_) => TokenKind::Operator,
        };
        result.push(FormulaToken { kind, span });
    }
    result
}

/// What a name is by where it stands: `previous` is the token before it
/// (its kind as classified), `next` and `after` the two after it.
fn ident_kind(
    name: &str,
    previous: Option<(Option<Tok>, TokenKind)>,
    next: Option<Tok>,
    after: Option<Tok>,
    locals: &[&str],
) -> TokenKind {
    if previous.is_some_and(|(tok, _)| tok == Some(Tok::Dot)) {
        return TokenKind::Column;
    }
    if next == Some(Tok::Dot) {
        return TokenKind::Table;
    }
    // After a value only a unit may follow (`30 mm`, `(a + b) in`, `2''`).
    let after_value = previous.is_some_and(|(tok, kind)| {
        tok == Some(Tok::RParen)
            || matches!(
                kind,
                TokenKind::Number
                    | TokenKind::Name
                    | TokenKind::Local
                    | TokenKind::Column
                    | TokenKind::Text
            )
            || (kind == TokenKind::Keyword && tok == Some(Tok::Ident))
    });
    if after_value {
        return if base_unit(name).is_some() {
            TokenKind::Unit
        } else {
            TokenKind::Error
        };
    }
    if next == Some(Tok::LParen) {
        return match Function::from_name(name) {
            Some(Function::Let) => TokenKind::Keyword,
            Some(_) => TokenKind::Function,
            // `PI()`, a constant with an empty argument list.
            None if Constant::from_name(name).is_some() && after == Some(Tok::RParen) => {
                TokenKind::Keyword
            }
            None => TokenKind::Error,
        };
    }
    if Constant::from_name(name).is_some() {
        TokenKind::Keyword
    } else if locals.contains(&name) {
        TokenKind::Local
    } else if Function::from_name(name).is_some_and(Function::is_reserved) {
        // A function's name before its `(` is typed.
        TokenKind::Function
    } else if is_unit_name(name) {
        TokenKind::Unit
    } else if super::is_unsupported_name(name) {
        TokenKind::Error
    } else {
        TokenKind::Name
    }
}

/// The names the formula's `LET`s bind: a name alone in a binding's place
/// (an odd argument's place, before its `;`).
fn let_locals<'a>(text: &'a str, raw: &[(Option<Tok>, Span)]) -> Vec<&'a str> {
    // Per open bracket: whether it is a LET's, its argument and whether
    // nothing of that argument came yet.
    let mut stack: Vec<(bool, usize, bool)> = Vec::new();
    let mut locals = Vec::new();
    for (index, &(tok, span)) in raw.iter().enumerate() {
        match tok {
            Some(Tok::LParen) => {
                let is_let = index.checked_sub(1).is_some_and(|i| {
                    raw[i].0 == Some(Tok::Ident)
                        && text[raw[i].1.start..raw[i].1.end].eq_ignore_ascii_case("let")
                });
                if let Some(top) = stack.last_mut() {
                    top.2 = false;
                }
                stack.push((is_let, 0, true));
            }
            Some(Tok::RParen) => {
                stack.pop();
            }
            Some(Tok::Semicolon | Tok::Comma) => {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                    top.2 = true;
                }
            }
            _ => {
                if let Some(top) = stack.last_mut() {
                    let binding = top.0 && top.1 % 2 == 0 && top.2;
                    let separated = matches!(
                        raw.get(index + 1).and_then(|t| t.0),
                        Some(Tok::Semicolon | Tok::Comma)
                    );
                    if binding && separated && tok == Some(Tok::Ident) {
                        locals.push(&text[span.start..span.end]);
                    }
                    top.2 = false;
                }
            }
        }
    }
    locals
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pieces' texts and kinds' names.
    fn kinds(text: &str) -> Vec<(&str, &'static str)> {
        tokens(text)
            .into_iter()
            .map(|t| (&text[t.span.start..t.span.end], t.kind.name()))
            .collect()
    }

    #[test]
    fn tokens_tell_names_units_functions_and_keywords_apart() {
        assert_eq!(
            kinds("width+(sheet_thickness*2)+(spacing*2 mm)"),
            vec![
                ("width", "name"),
                ("+", "operator"),
                ("(", "bracket"),
                ("sheet_thickness", "name"),
                ("*", "operator"),
                ("2", "number"),
                (")", "bracket"),
                ("+", "operator"),
                ("(", "bracket"),
                ("spacing", "name"),
                ("*", "operator"),
                ("2", "number"),
                ("mm", "unit"),
                (")", "bracket"),
            ]
        );
        assert_eq!(
            kinds("LET(p; 5 mm; FLOOR(len / p) * PI)"),
            vec![
                ("LET", "keyword"),
                ("(", "bracket"),
                ("p", "local"),
                (";", "separator"),
                ("5", "number"),
                ("mm", "unit"),
                (";", "separator"),
                ("FLOOR", "function"),
                ("(", "bracket"),
                ("len", "name"),
                ("/", "operator"),
                ("p", "local"),
                (")", "bracket"),
                ("*", "operator"),
                ("PI", "keyword"),
                (")", "bracket"),
            ]
        );
        assert_eq!(
            kinds("'Box ' & Screws.dk"),
            vec![
                ("'Box '", "text"),
                ("&", "operator"),
                ("Screws", "table"),
                (".", "operator"),
                ("dk", "column"),
            ]
        );
        // The inch and foot marks and the degree sign after a number.
        assert_eq!(
            kinds("3' 6\" + 30°"),
            vec![
                ("3", "number"),
                ("'", "unit"),
                ("6", "number"),
                ("\"", "unit"),
                ("+", "operator"),
                ("30", "number"),
                ("°", "unit"),
            ]
        );
    }

    #[test]
    fn unfinished_and_wrong_formulas_keep_their_pieces() {
        // A name after a value that is no unit, an unknown function.
        assert_eq!(
            kinds("5 apples + nofun(2)"),
            vec![
                ("5", "number"),
                ("apples", "error"),
                ("+", "operator"),
                ("nofun", "error"),
                ("(", "bracket"),
                ("2", "number"),
                (")", "bracket"),
            ]
        );
        // A character the tokenizer refuses, and what follows it.
        assert_eq!(
            kinds("a ? b"),
            vec![("a", "name"), ("?", "error"), ("b", "name")]
        );
        // A quoted text not closed yet runs to the end.
        assert_eq!(
            kinds("CONCAT(a; 'Box"),
            vec![
                ("CONCAT", "function"),
                ("(", "bracket"),
                ("a", "name"),
                (";", "separator"),
                ("'Box", "text"),
            ]
        );
        // A function's name before its `(`; nothing for nothing.
        assert_eq!(kinds("FLOOR"), vec![("FLOOR", "function")]);
        assert!(tokens("").is_empty());
        assert!(tokens("   ").is_empty());
        // A multi-byte character the tokenizer refuses stays whole.
        assert_eq!(
            kinds("a € b"),
            vec![("a", "name"), ("€", "error"), ("b", "name")]
        );
    }

    #[test]
    fn unit_names_are_the_parsers() {
        let names: Vec<&str> = unit_names().collect();
        assert!(names.contains(&"mm") && names.contains(&"deg") && names.contains(&"in"));
        for name in names {
            assert!(Expr::parse(&format!("1 {name}")).is_ok(), "{name}");
        }
    }

    #[test]
    fn incomplete_nested_calls_and_decimal_commas_use_real_tokens() {
        assert_eq!(signature_at("FLOOR(", 6), Some((Function::Floor, 0)));
        assert_eq!(
            signature_at("FLOOR(2; MAX(3; 4", 18),
            Some((Function::Max, 1))
        );
        assert_eq!(signature_at("FLOOR(2,5; ", 12), Some((Function::Floor, 1)));
        assert_eq!(signature_at("VALUE(2 mm; \"mm\")", 18), None);
        assert_eq!(
            signature_at("VALUE(2 mm; \"m", 15),
            Some((Function::Value, 1))
        );
    }
    #[test]
    fn inspect_keeps_units_and_lexical_let_bindings() {
        let source = "LET(pitch; 5 mm; width; 40 mm; FLOOR(width / pitch))";
        let expr = Expr::parse(source).unwrap();
        let start = source.find("width / pitch").unwrap();
        let (span, value) = inspect(
            &expr,
            Span::new(start, start + "width / pitch".len()),
            &(),
            &EvalContext::default(),
        );
        assert_eq!(&source[span.start..span.end], "width / pitch");
        assert_eq!(value.unwrap().value, 8.0);
    }
    #[test]
    fn every_supported_function_has_help() {
        assert_eq!(function_catalog().count(), FUNCTIONS.len());
        for help in function_catalog() {
            assert!(
                !help.description.is_empty()
                    && !help.example.is_empty()
                    && !help.arguments.is_empty()
            );
            assert!(Expr::parse(help.example).is_ok(), "{}", help.name);
        }
    }
    #[test]
    fn indentation_preserves_tokens_and_formula_meaning() {
        let text = "LET(pitch; 5 mm; width; 40 mm; FLOOR(width / pitch))";
        let formatted = indent_let(text);
        assert!(formatted.contains("\n    pitch; 5 mm;\n    width; 40 mm;\n"));
        assert_eq!(Expr::parse(text), Expr::parse(&formatted));
        assert_eq!(indent_let("VALUE(5 mm; \"mm\")"), "VALUE(5 mm; \"mm\")");
    }
    #[test]
    fn let_declarations_show_their_binding_values() {
        let text = "LET(a; 1; b; a + 1; a + b)";
        let expression = Expr::parse(text).unwrap();
        for (name, expected) in [("a", 1.0), ("b", 2.0)] {
            let start = text.find(name).unwrap();
            let (span, value) = inspect(
                &expression,
                Span::new(start, start + name.len()),
                &(),
                &EvalContext::default(),
            );
            assert_eq!(&text[span.start..span.end], name);
            assert_eq!(value.unwrap().value, expected);
        }
    }
}
