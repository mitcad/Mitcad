// SPDX-License-Identifier: MIT
//! Recursive descent parser for the grammar in the module documentation.

use std::fmt;

use super::Span;
use super::ast::{Arity, BinaryOp, Constant, Function, Node, NodeKind};
use super::lexer::{Tok, Token, lex};
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
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParseErrorKind::Empty => f.write_str("empty expression")?,
            ParseErrorKind::UnexpectedChar(c) => write!(f, "unexpected character '{c}'")?,
            ParseErrorKind::InvalidNumber => f.write_str("number out of range")?,
            ParseErrorKind::PowerOperator => f.write_str("use '^' for powers, not '**'")?,
            ParseErrorKind::SingleEquals => f.write_str("use '==' to compare, not '='")?,
            ParseErrorKind::Unexpected { found, expected } => {
                write!(f, "expected {expected}, found '{found}'")?;
            }
            ParseErrorKind::UnexpectedEnd { expected } => {
                write!(f, "expected {expected}, found the end of the expression")?;
            }
            ParseErrorKind::UnknownFunction(name) => write!(f, "unknown function '{name}'")?,
            ParseErrorKind::Unsupported(name) => write!(f, "'{name}' is not supported")?,
            ParseErrorKind::MissingArguments(name) => {
                write!(f, "function '{name}' needs arguments in parentheses")?;
            }
            ParseErrorKind::WrongArgumentCount {
                function,
                expected,
                found,
                suggestion,
            } => {
                write!(f, "'{function}' takes {expected} arguments, found {found}")?;
                if let Some(call) = suggestion {
                    write!(
                        f,
                        "; separate them with ';' or a space after the comma: '{call}'"
                    )?;
                }
            }
            ParseErrorKind::UnknownUnit(name) => write!(f, "unknown unit '{name}'")?,
            ParseErrorKind::UnitWithoutValue(name) => {
                write!(f, "unit '{name}' must follow a number or an expression")?;
            }
            ParseErrorKind::InvalidUnitPower => f.write_str("invalid unit power")?,
            ParseErrorKind::IncompatibleUnits(name) => {
                write!(
                    f,
                    "unit '{name}' cannot be combined with another unit of its kind"
                )?;
            }
            ParseErrorKind::TooComplex => f.write_str("expression is too complex")?,
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
    };
    let node = parser.expression()?;
    if let Some(token) = parser.peek() {
        return Err(parser.unexpected(token, "an operator or the end of the expression"));
    }
    check_depth(&node)?;
    Ok(node)
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    nesting: usize,
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
        let mut lhs = self.additive()?;
        while let Some(op) = self.peek_tok().and_then(comparison_op) {
            self.pos += 1;
            let rhs = self.additive()?;
            lhs = binary(op, lhs, rhs, self.span_from(start));
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
            let reserved =
                Function::from_name(name).is_some() || Constant::from_name(name).is_some();
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
        if self.peek_tok() == Some(Tok::LParen) {
            return self.call(token, name);
        }
        let error = |kind| Err(ParseError::new(kind, token.span));
        if let Some(constant) = Constant::from_name(&name) {
            Ok(Node::new(NodeKind::Constant(constant), token.span))
        } else if Function::from_name(&name).is_some() {
            error(ParseErrorKind::MissingArguments(name))
        } else if is_unit_name(&name) {
            error(ParseErrorKind::UnitWithoutValue(name))
        } else if super::UNSUPPORTED_NAMES.contains(&name.as_str()) {
            error(ParseErrorKind::Unsupported(name))
        } else {
            Ok(Node::new(NodeKind::Reference(name), token.span))
        }
    }

    fn call(&mut self, name_token: Token, name: String) -> Result<Node, ParseError> {
        let Some(function) = Function::from_name(&name) else {
            let kind = if super::UNSUPPORTED_NAMES.contains(&name.as_str()) {
                ParseErrorKind::Unsupported(name)
            } else {
                ParseErrorKind::UnknownFunction(name)
            };
            return Err(ParseError::new(kind, name_token.span));
        };
        let open = self.eat(Tok::LParen).expect("caller checked '('");
        let first = self.pos;
        self.enter(open.span)?;
        let args = self.arguments();
        self.leave();
        let args = args?;
        let last = self.pos;
        self.close_paren()?;
        let span = self.span_from(name_token.span.start);
        let arity = function.arity();
        if !arity.accepts(args.len()) {
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
        Ok(Node::new(NodeKind::Call { function, args }, span))
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
            NodeKind::Number(_) | NodeKind::Constant(_) | NodeKind::Reference(_) => {}
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
