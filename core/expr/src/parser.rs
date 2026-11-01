// SPDX-License-Identifier: MIT
//! Recursive descent parser for the grammar in the module documentation.

use std::fmt;

use super::Span;
use super::ast::{Arity, BinaryOp, ColumnPlace, Constant, Function, Node, NodeKind};
use super::lexer::{Tok, Token, lex, text_of};
use super::units::{Unit, base_unit, is_unit_name, unit_power};

/// Deepest accepted nesting of parentheses, unary operators and calls.
const MAX_NESTING: usize = 100;
/// Deepest accepted syntax tree (long operator chains nest to the left).
const MAX_DEPTH: usize = 256;

/// Syntax error with the byte range of the offending input.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

impl ParseError {
    pub fn new(kind: ParseErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseErrorKind {
    Empty,
    UnexpectedChar(char),
    /// A number too large to represent.
    InvalidNumber,
    /// `**`; the power operator is `^`.
    PowerOperator,
    /// `=`; comparisons use `==`.
    SingleEquals,
    Unexpected {
        found: String,
        expected: &'static str,
    },
    UnexpectedEnd {
        expected: &'static str,
    },
    UnknownFunction(String),
    /// A reserved name that Mitcad does not evaluate (`random`, `Gravity`).
    Unsupported(String),
    /// A function name without an argument list.
    MissingArguments(String),
    WrongArgumentCount {
        function: &'static str,
        expected: Arity,
        found: usize,
        /// Too few arguments where a decimal comma joined two: the call
        /// with them separated (`max(1,5)` is `max(1.5)`; `max(1; 5)`).
        suggestion: Option<String>,
    },
    UnknownUnit(String),
    /// A unit with no value before it, such as `mm * 2`.
    UnitWithoutValue(String),
    /// A unit power that is zero or too large.
    InvalidUnitPower,
    /// Two different length (or angle) units in one unit, such as `mm*in`.
    IncompatibleUnits(String),
    TooComplex,
    /// A `"` or `'` without the closing one.
    UnterminatedText,
    /// Text in quotes where it cannot be. Since mitcad#664 quoted text is a
    /// text value everywhere, so the parser no longer gives this error; it
    /// stays for callers that match on it.
    Text,
    /// `ifs` or `let` with an argument left over from its pairs.
    UnpairedArguments {
        function: &'static str,
        found: usize,
    },
    /// A `let` name that is a unit, function or constant.
    ReservedLocalName(String),
    /// A name given twice in one `let`.
    DuplicateLocalName(String),
    /// A column reference (`Screws.dk`, mitcad#667) where a value is
    /// expected; the text is the reference.
    ColumnOutsideLookup(String),
    /// A value where a lookup function needs a column (mitcad#667);
    /// `argument` counts from 1.
    ColumnExpected {
        function: &'static str,
        argument: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParseErrorKind::Empty => f.write_str("empty expression")?,
            ParseErrorKind::UnexpectedChar(c) => write!(f, "unexpected character “{c}”")?,
            ParseErrorKind::InvalidNumber => f.write_str("number out of range")?,
            ParseErrorKind::PowerOperator => f.write_str("use '^' for powers, not '**'")?,
            ParseErrorKind::SingleEquals => f.write_str("use '==' to compare, not '='")?,
            ParseErrorKind::Unexpected { found, expected } => {
                write!(f, "expected {expected}, found “{found}”")?;
            }
            ParseErrorKind::UnexpectedEnd { expected } => {
                write!(f, "expected {expected}, found the end of the expression")?;
            }
            ParseErrorKind::UnknownFunction(name) => write!(f, "unknown function {name}")?,
            ParseErrorKind::Unsupported(name) => write!(f, "{name} is not supported")?,
            ParseErrorKind::MissingArguments(name) => {
                write!(f, "function {name} needs arguments in parentheses")?;
            }
            ParseErrorKind::WrongArgumentCount {
                function,
                expected,
                found,
                suggestion,
            } => {
                write!(f, "{function} takes {expected} arguments, found {found}")?;
                if let Some(call) = suggestion {
                    write!(
                        f,
                        "; separate them with ';' or a space after the comma: “{call}”"
                    )?;
                }
            }
            ParseErrorKind::UnknownUnit(name) => write!(f, "unknown unit {name}")?,
            ParseErrorKind::UnitWithoutValue(name) => {
                write!(f, "unit {name} must follow a number or an expression")?;
            }
            ParseErrorKind::InvalidUnitPower => f.write_str("invalid unit power")?,
            ParseErrorKind::IncompatibleUnits(name) => {
                write!(
                    f,
                    "unit {name} cannot be combined with another unit of its kind"
                )?;
            }
            ParseErrorKind::TooComplex => f.write_str("expression is too complex")?,
            ParseErrorKind::UnterminatedText => f.write_str("text without its closing quote")?,
            ParseErrorKind::Text => {
                f.write_str(
                    "text in quotes can only be the unit of 'value', as in 'value(x; \"mm\")'",
                )?;
            }
            ParseErrorKind::UnpairedArguments { function, found } => {
                let pairs = if *function == "let" {
                    "names and values in pairs, then the result"
                } else {
                    "conditions and values in pairs"
                };
                write!(f, "{function} takes {pairs}, found {found} arguments")?;
            }
            ParseErrorKind::ReservedLocalName(name) => write!(
                f,
                "{name} is a unit, function or constant and cannot name a value in 'let'"
            )?,
            ParseErrorKind::DuplicateLocalName(name) => {
                write!(f, "{name} is named twice in 'let'")?;
            }
            ParseErrorKind::ColumnOutsideLookup(column) => write!(
                f,
                "{column} is a column: look a value up with XLOOKUP or INDEX"
            )?,
            ParseErrorKind::ColumnExpected { function, argument } => write!(
                f,
                "argument {argument} of {function} must be a column such as Table.Column"
            )?,
        }
        write!(f, " at byte {}", self.span.start)
    }
}

impl std::error::Error for ParseError {}

pub(crate) fn parse(text: &str) -> Result<Node, ParseError> {
    let tokens = lex(text)?;
    if tokens.is_empty() {
        return Err(ParseError::new(
            ParseErrorKind::Empty,
            Span::new(0, text.len()),
        ));
    }
    let mut parser = Parser {
        text,
        tokens,
        pos: 0,
        nesting: 0,
        locals: Vec::new(),
    };
    let node = parser.expression()?;
    if let Some(token) = parser.peek() {
        return Err(parser.unexpected(token, "an operator or the end of the expression"));
    }
    check_depth(&node)?;
    check_columns(&node)?;
    Ok(node)
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    nesting: usize,
    /// Names of the enclosing `let`s that are given so far, innermost
    /// last.
    locals: Vec<String>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.pos).copied()
    }

    fn peek_tok(&self) -> Option<Tok> {
        self.peek().map(|t| t.tok)
    }

    fn eat(&mut self, tok: Tok) -> Option<Token> {
        let token = self.peek().filter(|t| t.tok == tok)?;
        self.pos += 1;
        Some(token)
    }

    fn source(&self, token: Token) -> &str {
        &self.text[token.span.start..token.span.end]
    }

    fn unexpected(&self, token: Token, expected: &'static str) -> ParseError {
        ParseError::new(
            ParseErrorKind::Unexpected {
                found: self.source(token).to_owned(),
                expected,
            },
            token.span,
        )
    }

    /// Start of the next token (or the end of the text).
    fn start(&self) -> usize {
        self.peek().map_or(self.text.len(), |t| t.span.start)
    }

    /// Span from `start` to the end of the last consumed token.
    fn span_from(&self, start: usize) -> Span {
        let end = self
            .pos
            .checked_sub(1)
            .map_or(start, |i| self.tokens[i].span.end);
        Span::new(start, end)
    }

    fn end_error(&self, expected: &'static str) -> ParseError {
        let end = self.text.len();
        ParseError::new(
            ParseErrorKind::UnexpectedEnd { expected },
            Span::new(end, end),
        )
    }

    fn enter(&mut self, span: Span) -> Result<(), ParseError> {
        self.nesting += 1;
        if self.nesting > MAX_NESTING {
            return Err(ParseError::new(ParseErrorKind::TooComplex, span));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.nesting -= 1;
    }

    fn expression(&mut self) -> Result<Node, ParseError> {
        let start = self.start();
        let mut lhs = self.concatenation()?;
        while let Some(op) = self.peek_tok().and_then(comparison_op) {
            self.pos += 1;
            let rhs = self.concatenation()?;
            lhs = binary(op, lhs, rhs, self.span_from(start));
        }
        Ok(lhs)
    }

    /// Operands joined by `&` (mitcad#664).
    fn concatenation(&mut self) -> Result<Node, ParseError> {
        let start = self.start();
        let mut lhs = self.additive()?;
        while self.eat(Tok::Amp).is_some() {
            let rhs = self.additive()?;
            lhs = binary(BinaryOp::Concat, lhs, rhs, self.span_from(start));
        }
        Ok(lhs)
    }

    fn additive(&mut self) -> Result<Node, ParseError> {
        let start = self.start();
        let mut lhs = self.multiplicative()?;
        loop {
            let op = match self.peek_tok() {
                Some(Tok::Plus) => BinaryOp::Add,
                Some(Tok::Minus) => BinaryOp::Sub,
                _ => return Ok(lhs),
            };
            self.pos += 1;
            let rhs = self.multiplicative()?;
            lhs = binary(op, lhs, rhs, self.span_from(start));
        }
    }

    fn multiplicative(&mut self) -> Result<Node, ParseError> {
        let start = self.start();
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek_tok() {
                Some(Tok::Star) => BinaryOp::Mul,
                Some(Tok::Slash) => BinaryOp::Div,
                Some(Tok::Percent) => BinaryOp::Rem,
                _ => return Ok(lhs),
            };
            self.pos += 1;
            let rhs = self.unary()?;
            lhs = binary(op, lhs, rhs, self.span_from(start));
        }
    }

    fn unary(&mut self) -> Result<Node, ParseError> {
        let Some(token) = self.peek() else {
            return Err(self.end_error("a value"));
        };
        match token.tok {
            Tok::Minus | Tok::Plus => {
                self.pos += 1;
                self.enter(token.span)?;
                let operand = self.unary();
                self.leave();
                let operand = operand?;
                if token.tok == Tok::Plus {
                    return Ok(operand);
                }
                let span = self.span_from(token.span.start);
                Ok(Node::new(NodeKind::Neg(Box::new(operand)), span))
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Node, ParseError> {
        let start = self.start();
        let base = self.postfix()?;
        let Some(caret) = self.eat(Tok::Caret) else {
            return Ok(base);
        };
        self.enter(caret.span)?;
        let exponent = self.unary();
        self.leave();
        let exponent = exponent?;
        Ok(binary(BinaryOp::Pow, base, exponent, self.span_from(start)))
    }

    fn postfix(&mut self) -> Result<Node, ParseError> {
        let start = self.start();
        let value = self.primary()?;
        let Some(token) = self.peek().filter(|t| t.tok == Tok::Ident) else {
            return Ok(value);
        };
        let name = self.source(token);
        let Some(base) = base_unit(name) else {
            let reserved = Function::from_name(name).is_some_and(Function::is_reserved)
                || Constant::from_name(name).is_some();
            let kind = if reserved {
                ParseErrorKind::Unexpected {
                    found: name.to_owned(),
                    expected: "an operator",
                }
            } else {
                ParseErrorKind::UnknownUnit(name.to_owned())
            };
            return Err(ParseError::new(kind, token.span));
        };
        self.pos += 1;
        let mut unit = Unit::of_base(base);
        if let Some((power, used, span)) = unit_power(self.text, &self.tokens[self.pos..])? {
            unit = unit
                .powi(power)
                .ok_or_else(|| ParseError::new(ParseErrorKind::InvalidUnitPower, span))?;
            self.pos += used;
        }
        let span = self.span_from(start);
        Ok(Node::new(
            NodeKind::WithUnit {
                value: Box::new(value),
                unit,
            },
            span,
        ))
    }

    fn primary(&mut self) -> Result<Node, ParseError> {
        let Some(token) = self.peek() else {
            return Err(self.end_error("a value"));
        };
        self.pos += 1;
        match token.tok {
            Tok::Number(value) => Ok(Node::new(NodeKind::Number(value), token.span)),
            Tok::Ident => self.name(token),
            Tok::LParen => {
                self.enter(token.span)?;
                let inner = self.expression();
                self.leave();
                let inner = inner?;
                self.close_paren()?;
                // Parentheses add no node; enclosing nodes' spans cover them.
                Ok(inner)
            }
            Tok::Str => Ok(Node::new(
                NodeKind::Text(text_of(self.source(token))),
                token.span,
            )),
            _ => Err(self.unexpected(token, "a value")),
        }
    }

    fn close_paren(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(t) if t.tok == Tok::RParen => {
                self.pos += 1;
                Ok(())
            }
            Some(t) => Err(self.unexpected(t, "')'")),
            None => Err(self.end_error("')'")),
        }
    }

    fn name(&mut self, token: Token) -> Result<Node, ParseError> {
        let name = self.source(token).to_owned();
        if self.eat(Tok::Dot).is_some() {
            // `Table.Column` (mitcad#667); the lexer gives a dot only before
            // a name start, so a name follows. A column may have a reserved
            // name (`in`, `Max`).
            let column = match self.peek() {
                Some(t) if t.tok == Tok::Ident => t,
                Some(t) => return Err(self.unexpected(t, "a column name")),
                None => return Err(self.end_error("a column name")),
            };
            self.pos += 1;
            let kind = NodeKind::Column {
                table: name,
                column: self.source(column).to_owned(),
            };
            return Ok(Node::new(kind, self.span_from(token.span.start)));
        }
        if self.peek_tok() == Some(Tok::LParen) {
            // `PI()`: a constant with an empty argument list.
            if let Some(constant) = Constant::from_name(&name)
                && Function::from_name(&name).is_none()
                && self.tokens.get(self.pos + 1).map(|t| t.tok) == Some(Tok::RParen)
            {
                self.pos += 2;
                let span = self.span_from(token.span.start);
                return Ok(Node::new(NodeKind::Constant(constant), span));
            }
            return self.call(token, name);
        }
        if self.locals.contains(&name) {
            return Ok(Node::new(NodeKind::Local(name), token.span));
        }
        let error = |kind| Err(ParseError::new(kind, token.span));
        if let Some(constant) = Constant::from_name(&name) {
            Ok(Node::new(NodeKind::Constant(constant), token.span))
        } else if Function::from_name(&name).is_some_and(Function::is_reserved) {
            // The text functions' names are references here (mitcad#664).
            error(ParseErrorKind::MissingArguments(name))
        } else if is_unit_name(&name) {
            error(ParseErrorKind::UnitWithoutValue(name))
        } else if super::is_unsupported_name(&name) {
            error(ParseErrorKind::Unsupported(name))
        } else {
            Ok(Node::new(NodeKind::Reference(name), token.span))
        }
    }

    fn call(&mut self, name_token: Token, name: String) -> Result<Node, ParseError> {
        let Some(function) = Function::from_name(&name) else {
            let kind = if super::is_unsupported_name(&name) {
                ParseErrorKind::Unsupported(name)
            } else {
                ParseErrorKind::UnknownFunction(name)
            };
            return Err(ParseError::new(kind, name_token.span));
        };
        let open = self.eat(Tok::LParen).expect("caller checked '('");
        let first = self.pos;
        self.enter(open.span)?;
        let args = match function {
            Function::Let => self.let_arguments(),
            Function::Value => self.value_arguments(),
            _ => self.arguments(),
        };
        self.leave();
        let args = args?;
        let last = self.pos;
        self.close_paren()?;
        let span = self.span_from(name_token.span.start);
        let arity = function.arity();
        // `min(Screws.d)`: one column holds the values (mitcad#667).
        let one_column = args.len() == 1
            && function.column_place(0) == ColumnPlace::Allowed
            && matches!(args[0].kind, NodeKind::Column { .. });
        if !arity.accepts(args.len()) && !one_column {
            let suggestion = if args.len() < arity.min() {
                self.separate_arguments(&self.tokens[first..last], span)
            } else {
                None
            };
            return Err(ParseError::new(
                ParseErrorKind::WrongArgumentCount {
                    function: function.name(),
                    expected: arity,
                    found: args.len(),
                    suggestion,
                },
                span,
            ));
        }
        // `ifs` takes pairs; `let` pairs and then its result.
        let paired = args.len().is_multiple_of(2) != (function == Function::Let);
        if function.takes_pairs() && !paired {
            return Err(ParseError::new(
                ParseErrorKind::UnpairedArguments {
                    function: function.name(),
                    found: args.len(),
                },
                span,
            ));
        }
        Ok(Node::new(NodeKind::Call { function, args }, span))
    }

    /// The arguments of `let`: a name and its value in turn, then the
    /// result. A name can be used from the argument after its value on,
    /// and only inside the `let`.
    fn let_arguments(&mut self) -> Result<Vec<Node>, ParseError> {
        let outer = self.locals.len();
        let mut args = Vec::new();
        let result = self.let_pairs(outer, &mut args);
        self.locals.truncate(outer);
        result.map(|()| args)
    }

    fn let_pairs(&mut self, outer: usize, args: &mut Vec<Node>) -> Result<(), ParseError> {
        loop {
            let name_place = args.len().is_multiple_of(2);
            let named = name_place
                && self.peek_tok() == Some(Tok::Ident)
                && matches!(
                    self.tokens.get(self.pos + 1).map(|t| t.tok),
                    Some(Tok::Semicolon | Tok::Comma)
                );
            if named {
                let token = self.peek().expect("checked above");
                let name = self.source(token).to_owned();
                if super::is_reserved_name(&name) {
                    return Err(ParseError::new(
                        ParseErrorKind::ReservedLocalName(name),
                        token.span,
                    ));
                }
                if self.locals[outer..].contains(&name) {
                    return Err(ParseError::new(
                        ParseErrorKind::DuplicateLocalName(name),
                        token.span,
                    ));
                }
                // The name and its separator.
                self.pos += 2;
                args.push(Node::new(NodeKind::Local(name.clone()), token.span));
                args.push(self.expression()?);
                self.locals.push(name);
            } else {
                let arg = self.expression()?;
                let span = arg.span;
                args.push(arg);
                if name_place && matches!(self.peek_tok(), Some(Tok::Semicolon | Tok::Comma)) {
                    // A value where a name belongs.
                    return Err(ParseError::new(
                        ParseErrorKind::Unexpected {
                            found: self.text[span.start..span.end].to_owned(),
                            expected: "a name",
                        },
                        span,
                    ));
                }
            }
            if self.eat(Tok::Semicolon).is_none() && self.eat(Tok::Comma).is_none() {
                return Ok(());
            }
        }
    }

    /// The arguments of `value`: a value and its unit, in quotes (`"mm"`)
    /// or as a value (`1 mm`).
    fn value_arguments(&mut self) -> Result<Vec<Node>, ParseError> {
        let mut args = vec![self.expression()?];
        while self.eat(Tok::Semicolon).is_some() || self.eat(Tok::Comma).is_some() {
            match self.peek() {
                Some(token) if token.tok == Tok::Str && args.len() == 1 => {
                    self.pos += 1;
                    args.push(self.unit_text(token)?);
                }
                _ => args.push(self.expression()?),
            }
        }
        Ok(args)
    }

    /// The unit of a quoted text token, such as `"mm"` or `"in^2"`.
    fn unit_text(&self, token: Token) -> Result<Node, ParseError> {
        let start = token.span.start + 1;
        let text = &self.text[start..token.span.end - 1];
        let unit = Unit::parse(text).map_err(|e| {
            let span = Span::new(e.span.start + start, e.span.end + start);
            ParseError::new(e.kind, span)
        })?;
        if unit.is_unitless() {
            return Err(ParseError::new(
                ParseErrorKind::UnknownUnit(text.to_owned()),
                token.span,
            ));
        }
        Ok(Node::new(NodeKind::UnitText(unit), token.span))
    }

    /// The call at `span`, whose arguments are `args`, with `;` between
    /// them where a decimal comma or a comma stood (`max(1,5)` as
    /// `max(1; 5)`, `if(1,2,3)` as `if(1; 2; 3)`); None when no argument
    /// has a decimal comma. Nested calls stay as they are.
    fn separate_arguments(&self, args: &[Token], span: Span) -> Option<String> {
        let mut depth = 0usize;
        let mut commas = Vec::new();
        let mut decimal = false;
        for token in args {
            match token.tok {
                Tok::LParen => depth += 1,
                Tok::RParen => depth = depth.saturating_sub(1),
                Tok::Comma if depth == 0 => commas.push(token.span.start),
                Tok::Number(_) if depth == 0 => {
                    if let Some(at) = self.source(*token).find(',') {
                        commas.push(token.span.start + at);
                        decimal = true;
                    }
                }
                _ => {}
            }
        }
        if !decimal {
            return None;
        }
        let mut call = String::new();
        let mut copied = span.start;
        for comma in commas {
            call.push_str(&self.text[copied..comma]);
            call.push(';');
            copied = comma + 1;
            if !self.text[copied..].starts_with(char::is_whitespace) {
                call.push(' ');
            }
        }
        call.push_str(&self.text[copied..span.end]);
        Some(call)
    }

    fn arguments(&mut self) -> Result<Vec<Node>, ParseError> {
        let mut args = Vec::new();
        if self.peek_tok() == Some(Tok::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.expression()?);
            if self.eat(Tok::Semicolon).is_none() && self.eat(Tok::Comma).is_none() {
                return Ok(args);
            }
        }
    }
}

fn comparison_op(tok: Tok) -> Option<BinaryOp> {
    Some(match tok {
        Tok::Lt => BinaryOp::Lt,
        Tok::Le => BinaryOp::Le,
        Tok::Gt => BinaryOp::Gt,
        Tok::Ge => BinaryOp::Ge,
        Tok::Eq => BinaryOp::Eq,
        Tok::Ne => BinaryOp::Ne,
        _ => return None,
    })
}

fn binary(op: BinaryOp, lhs: Node, rhs: Node, span: Span) -> Node {
    Node::new(
        NodeKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        span,
    )
}

/// Rejects trees deeper than [`MAX_DEPTH`], without recursion.
fn check_depth(root: &Node) -> Result<(), ParseError> {
    let mut stack = vec![(root, 1usize)];
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err(ParseError::new(ParseErrorKind::TooComplex, node.span));
        }
        match &node.kind {
            NodeKind::Number(_)
            | NodeKind::Constant(_)
            | NodeKind::Reference(_)
            | NodeKind::Local(_)
            | NodeKind::UnitText(_)
            | NodeKind::Text(_)
            | NodeKind::Column { .. } => {}
            NodeKind::Neg(x) | NodeKind::WithUnit { value: x, .. } => stack.push((x, depth + 1)),
            NodeKind::Binary { lhs, rhs, .. } => {
                stack.push((lhs, depth + 1));
                stack.push((rhs, depth + 1));
            }
            NodeKind::Call { args, .. } => stack.extend(args.iter().map(|a| (a, depth + 1))),
        }
    }
    Ok(())
}

/// Refuses column references (mitcad#667) outside the arguments that take
/// them ([`Function::column_place`]), and values where a column must be,
/// without recursion. Errors come in source order.
fn check_columns(root: &Node) -> Result<(), ParseError> {
    let mut stack = vec![(root, false)];
    while let Some((node, allowed)) = stack.pop() {
        match &node.kind {
            NodeKind::Column { table, column } if !allowed => {
                return Err(ParseError::new(
                    ParseErrorKind::ColumnOutsideLookup(format!("{table}.{column}")),
                    node.span,
                ));
            }
            NodeKind::Number(_)
            | NodeKind::Constant(_)
            | NodeKind::Reference(_)
            | NodeKind::Local(_)
            | NodeKind::UnitText(_)
            | NodeKind::Text(_)
            | NodeKind::Column { .. } => {}
            NodeKind::Neg(x) | NodeKind::WithUnit { value: x, .. } => stack.push((x, false)),
            NodeKind::Binary { lhs, rhs, .. } => {
                stack.push((rhs, false));
                stack.push((lhs, false));
            }
            NodeKind::Call { function, args } => {
                for (i, arg) in args.iter().enumerate() {
                    let required = function.column_place(i) == ColumnPlace::Required;
                    if required && !matches!(arg.kind, NodeKind::Column { .. }) {
                        return Err(ParseError::new(
                            ParseErrorKind::ColumnExpected {
                                function: function.name(),
                                argument: i + 1,
                            },
                            arg.span,
                        ));
                    }
                }
                for (i, arg) in args.iter().enumerate().rev() {
                    stack.push((arg, function.column_place(i) != ColumnPlace::No));
                }
            }
        }
    }
    Ok(())
}
