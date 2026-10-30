// SPDX-License-Identifier: MIT
//! FreeCAD's expressions as its files keep them (the `ExpressionEngine`
//! bindings of objects' properties, spreadsheet cells): parsed into a tree
//! whose references (`Spreadsheet.Width`, `<<My sheet>>.B3`,
//! `.Constraints.height`, `Pad.Length`) are left for the document's reader
//! to resolve.
//!
//! The syntax, learnt from FreeCAD's documentation and the files it writes
//! (FreeCAD writes expressions back in a normal form: `30 mm`,
//! `hypot(3 mm; 4 mm)`, `a > 20 mm ? 5 mm : 7 mm`):
//!
//! - numbers (`30`, `2.5`, `1e-3`), with a unit right after them (`30 mm`,
//!   `30mm`, `2 mm^2`, `9.81 m/s^2`); a unit alone is one of it (`mm`);
//! - constants `pi` and `e`; strings `<<text>>`;
//! - references: an object by its name or label and a property path
//!   (`Pad.Length`, `<<My pad>>.Length`, `Sketch.Constraints.width`,
//!   `Pad.Placement.Base.z`, `Sketch.Constraints[2]`), a property of the
//!   expression's own object (`.Length`, or a bare name: `Length`, a
//!   spreadsheet's `B3` or alias in its own cells), another document's
//!   object (`doc#Pad.Length`), cell ranges in function arguments
//!   (`sum(A1:A3)`, `sum(Sheet.A1:A3)`);
//! - operators, from the loosest: `c ? a : b` (left-associative), the
//!   comparisons `== != < > <= >=`, `+ -`, `* / %`, `^`
//!   (left-associative), and the signs `-x`, `+x`, which bind tighter than
//!   `^` (`-2^2` is 4); a unit binds to its number;
//! - functions `name(a; b)` (`,` separates arguments too).
//!
//! The units read are lengths and angles (FreeCAD's other quantities are
//! names a reader cannot resolve).
//!
//! An expression nests at most [`MAX_DEPTH`] levels deep (parentheses,
//! signs, operators in a chain, conditionals, units in parentheses); a
//! deeper one is an error, so that a hostile file cannot overflow the
//! stack of the parser or of the code walking its tree.

use std::fmt;

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    /// An expression in a unit: `30 mm` (a number and its unit), or a unit
    /// alone (`Number(1)`).
    WithUnit(Box<Expr>, Vec<UnitFactor>),
    Constant(Constant),
    Reference(Reference),
    /// A cell range `A1:A3` (in function arguments): the reference names
    /// the first cell.
    Range(Reference, String),
    Text(String),
    Negate(Box<Expr>),
    /// `+x`.
    Plus(Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// `condition ? then : otherwise`.
    Conditional(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constant {
    Pi,
    E,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

impl BinaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Pow => "^",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::Le => "<=",
            BinaryOp::Ge => ">=",
        }
    }
}

/// A unit and its power: `mm^2` is `("mm", 2)`, `/s` `("s", -1)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitFactor {
    pub name: String,
    pub power: i32,
}

/// A part of a reference's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Component {
    Name(String),
    /// `[2]`.
    Index(i64),
}

/// A reference to a property of an object (or a spreadsheet's cell).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reference {
    /// Another document's name (`doc#…`).
    pub document: Option<String>,
    /// The object by its label (`<<label>>.…`); the path is its property.
    pub label: Option<String>,
    /// Written with a leading `.`: a property of the expression's own
    /// object.
    pub relative: bool,
    /// Without a label and not relative, the first name is an object's
    /// name, or a property of the expression's own object (a spreadsheet's
    /// cell or alias in its own cells).
    pub path: Vec<Component>,
}

impl Reference {
    /// The names of the path (indices left out).
    pub fn names(&self) -> Vec<&str> {
        self.path
            .iter()
            .filter_map(|c| match c {
                Component::Name(n) => Some(n.as_str()),
                Component::Index(_) => None,
            })
            .collect()
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(d) = &self.document {
            write!(f, "{d}#")?;
        }
        if let Some(l) = &self.label {
            write!(f, "<<{l}>>.")?;
        } else if self.relative {
            f.write_str(".")?;
        }
        for (i, c) in self.path.iter().enumerate() {
            match c {
                Component::Name(n) if i == 0 => f.write_str(n)?,
                Component::Name(n) => write!(f, ".{n}")?,
                Component::Index(k) => write!(f, "[{k}]")?,
            }
        }
        Ok(())
    }
}

/// Length and angle units of FreeCAD's expressions.
pub const UNITS: &[&str] = &[
    "nm", "µm", "um", "mm", "cm", "dm", "m", "km", "mil", "thou", "in", "\"", "ft", "'", "yd",
    "mi", "deg", "°", "rad", "gon", "′", "″",
];

/// Whether a name is a unit of [`UNITS`].
pub fn is_unit(name: &str) -> bool {
    UNITS.contains(&name)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Number(f64),
    Ident(String),
    /// `<<…>>`.
    Label(String),
    /// A unit written as a symbol (`°`, `"`, `'`, `′`, `″`).
    Symbol(String),
    /// `A1:B3` written without spaces.
    Range(String, String),
    Op(&'static str),
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    /// Where it starts (a byte offset).
    start: usize,
}

/// A parse error: what and where (a byte offset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub at: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at {})", self.message, self.at)
    }
}

impl std::error::Error for ParseError {}

fn error<T>(message: impl Into<String>, at: usize) -> Result<T, ParseError> {
    Err(ParseError {
        message: message.into(),
        at,
    })
}

/// A spreadsheet cell's address (`A1`, `AB12`): letters then digits.
pub fn is_cell_address(text: &str) -> bool {
    let letters = text.bytes().take_while(u8::is_ascii_uppercase).count();
    (1..=3).contains(&letters)
        && text.len() > letters
        && text[letters..].bytes().all(|b| b.is_ascii_digit())
        && !text[letters..].starts_with('0')
}

fn lex(text: &str) -> Result<Vec<Token>, ParseError> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let end_of = |k: usize| chars.get(k).map_or(text.len(), |(i, _)| *i);
    let mut k = 0;
    while k < chars.len() {
        let (start, c) = chars[k];
        if c.is_whitespace() {
            k += 1;
            continue;
        }
        let digit_at = |k: usize| chars.get(k).is_some_and(|(_, c)| c.is_ascii_digit());
        if c.is_ascii_digit() || (c == '.' && digit_at(k + 1)) {
            let mut j = k;
            while digit_at(j) {
                j += 1;
            }
            if chars.get(j).is_some_and(|(_, c)| *c == '.') {
                j += 1;
                while digit_at(j) {
                    j += 1;
                }
            }
            if chars.get(j).is_some_and(|(_, c)| matches!(c, 'e' | 'E')) {
                let sign = chars
                    .get(j + 1)
                    .is_some_and(|(_, c)| matches!(c, '+' | '-'));
                let first = if sign { j + 2 } else { j + 1 };
                if digit_at(first) {
                    j = first;
                    while digit_at(j) {
                        j += 1;
                    }
                }
            }
            let literal = &text[start..end_of(j)];
            let value: f64 = literal
                .parse()
                .or_else(|_| error(format!("'{literal}' is not a number"), start))?;
            out.push(Token {
                tok: Tok::Number(value),
                start,
            });
            k = j;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = k + 1;
            while chars
                .get(j)
                .is_some_and(|(_, c)| c.is_alphanumeric() || *c == '_')
            {
                j += 1;
            }
            let name = &text[start..end_of(j)];
            // A range written without spaces: `A1:B3`.
            if is_cell_address(name) && chars.get(j).is_some_and(|(_, c)| *c == ':') {
                let mut e = j + 1;
                while chars.get(e).is_some_and(|(_, c)| c.is_ascii_alphanumeric()) {
                    e += 1;
                }
                let to = &text[end_of(j + 1)..end_of(e)];
                if is_cell_address(to) {
                    out.push(Token {
                        tok: Tok::Range(name.to_owned(), to.to_owned()),
                        start,
                    });
                    k = e;
                    continue;
                }
            }
            out.push(Token {
                tok: Tok::Ident(name.to_owned()),
                start,
            });
            k = j;
            continue;
        }
        if c == '<' && chars.get(k + 1).is_some_and(|(_, c)| *c == '<') {
            let rest = &text[end_of(k + 2)..];
            let Some(close) = rest.find(">>") else {
                return error("a '<<' without '>>'", start);
            };
            let label = rest[..close].to_owned();
            let end = end_of(k + 2) + close + 2;
            out.push(Token {
                tok: Tok::Label(label),
                start,
            });
            while k < chars.len() && chars[k].0 < end {
                k += 1;
            }
            continue;
        }
        if matches!(c, '°' | '"' | '\'' | '′' | '″') {
            out.push(Token {
                tok: Tok::Symbol(c.to_string()),
                start,
            });
            k += 1;
            continue;
        }
        let two = chars.get(k + 1).map(|(_, d)| *d);
        let op: Option<(&'static str, usize)> = match (c, two) {
            ('=', Some('=')) => Some(("==", 2)),
            ('!', Some('=')) => Some(("!=", 2)),
            ('<', Some('=')) => Some(("<=", 2)),
            ('>', Some('=')) => Some((">=", 2)),
            ('<', Some('>')) => Some(("!=", 2)),
            ('+', _) => Some(("+", 1)),
            ('-', _) => Some(("-", 1)),
            ('*', _) => Some(("*", 1)),
            ('/', _) => Some(("/", 1)),
            ('%', _) => Some(("%", 1)),
            ('^', _) => Some(("^", 1)),
            ('(', _) => Some(("(", 1)),
            (')', _) => Some((")", 1)),
            ('[', _) => Some(("[", 1)),
            (']', _) => Some(("]", 1)),
            (';', _) => Some((";", 1)),
            (',', _) => Some((",", 1)),
            ('.', _) => Some((".", 1)),
            ('?', _) => Some(("?", 1)),
            (':', _) => Some((":", 1)),
            ('<', _) => Some(("<", 1)),
            ('>', _) => Some((">", 1)),
            ('#', _) => Some(("#", 1)),
            _ => None,
        };
        let Some((op, len)) = op else {
            return error(format!("unexpected '{c}'"), start);
        };
        out.push(Token {
            tok: Tok::Op(op),
            start,
        });
        k += len;
    }
    Ok(out)
}

/// How deep an expression may nest. Every nesting level counts: each
/// parenthesis (three levels: the conditional, the sign and the primary
/// inside it), each sign, each operator of a chain (`1+1+1` is a tree two
/// deep), each conditional and each unit in parentheses. The expressions
/// FreeCAD writes nest a few levels (the deepest in the test corpora have
/// two parentheses and four operators); 256 keeps the parser's recursion
/// and every walk of the tree well within a thread's stack.
pub const MAX_DEPTH: usize = 256;

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    len: usize,
    /// The nesting levels open now (see [`MAX_DEPTH`]).
    depth: usize,
}

impl Parser {
    /// Opens a nesting level; an error past [`MAX_DEPTH`].
    fn enter(&mut self) -> Result<(), ParseError> {
        if self.depth >= MAX_DEPTH {
            return error("the expression nests too deep", self.offset());
        }
        self.depth += 1;
        Ok(())
    }

    /// Runs `f` one nesting level deeper.
    fn nested<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        self.enter()?;
        let out = f(self);
        self.depth -= 1;
        out
    }

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at).map(|t| &t.tok)
    }

    fn peek_at(&self, k: usize) -> Option<&Tok> {
        self.tokens.get(self.at + k).map(|t| &t.tok)
    }

    fn offset(&self) -> usize {
        self.tokens.get(self.at).map_or(self.len, |t| t.start)
    }

    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, op: &str) -> Result<(), ParseError> {
        if self.eat(op) {
            Ok(())
        } else {
            error(format!("'{op}' expected"), self.offset())
        }
    }

    fn conditional(&mut self) -> Result<Expr, ParseError> {
        self.nested(|p| {
            let mut e = p.comparison()?;
            // Each conditional of a chain nests the tree one level deeper.
            let mut chained = 0;
            while p.eat("?") {
                p.enter()?;
                chained += 1;
                let then = p.conditional()?;
                p.expect(":")?;
                let otherwise = p.comparison()?;
                e = Expr::Conditional(Box::new(e), Box::new(then), Box::new(otherwise));
            }
            p.depth -= chained;
            Ok(e)
        })
    }

    fn binary_level(
        &mut self,
        ops: &[(&str, BinaryOp)],
        next: fn(&mut Self) -> Result<Expr, ParseError>,
    ) -> Result<Expr, ParseError> {
        let mut e = next(self)?;
        // Each operator of a chain nests the tree one level deeper.
        let mut chained = 0;
        'outer: loop {
            for (symbol, op) in ops {
                if self.eat(symbol) {
                    self.enter()?;
                    chained += 1;
                    let right = next(self)?;
                    e = Expr::Binary(*op, Box::new(e), Box::new(right));
                    continue 'outer;
                }
            }
            self.depth -= chained;
            return Ok(e);
        }
    }

    fn comparison(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(
            &[
                ("==", BinaryOp::Eq),
                ("!=", BinaryOp::Ne),
                ("<=", BinaryOp::Le),
                (">=", BinaryOp::Ge),
                ("<", BinaryOp::Lt),
                (">", BinaryOp::Gt),
            ],
            Self::additive,
        )
    }

    fn additive(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(
            &[("+", BinaryOp::Add), ("-", BinaryOp::Sub)],
            Self::multiplicative,
        )
    }

    fn multiplicative(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(
            &[
                ("*", BinaryOp::Mul),
                ("/", BinaryOp::Div),
                ("%", BinaryOp::Mod),
            ],
            Self::power,
        )
    }

    fn power(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(&[("^", BinaryOp::Pow)], Self::unary)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        self.nested(|p| {
            if p.eat("-") {
                return Ok(Expr::Negate(Box::new(p.unary()?)));
            }
            if p.eat("+") {
                return Ok(Expr::Plus(Box::new(p.unary()?)));
            }
            let e = p.primary()?;
            // A unit after a number (or a parenthesized expression).
            if p.unit_ahead() {
                let unit = p.unit()?;
                return Ok(Expr::WithUnit(Box::new(e), unit));
            }
            Ok(e)
        })
    }

    /// Whether a unit comes next (a unit name not starting a reference or
    /// a call, or a unit in parentheses: FreeCAD writes `16 (mm ^ 2)`).
    fn unit_ahead(&self) -> bool {
        self.unit_at(0) || matches!(self.peek(), Some(Tok::Op("("))) && self.unit_at(1)
    }

    fn unit_at(&self, k: usize) -> bool {
        match self.peek_at(k) {
            Some(Tok::Symbol(_)) => true,
            Some(Tok::Ident(name)) => {
                is_unit(name)
                    && !matches!(self.peek_at(k + 1), Some(Tok::Op("." | "(" | "#" | "[")))
            }
            _ => false,
        }
    }

    /// An integer power after `^`, if one follows.
    fn power_suffix(&mut self) -> i32 {
        if !matches!(self.peek(), Some(Tok::Op("^"))) {
            return 1;
        }
        let negative = matches!(self.peek_at(1), Some(Tok::Op("-")));
        let number = self.peek_at(if negative { 2 } else { 1 }).cloned();
        match number {
            Some(Tok::Number(n)) if n.fract() == 0.0 && n.abs() < 100.0 => {
                self.at += if negative { 3 } else { 2 };
                if negative { -(n as i32) } else { n as i32 }
            }
            _ => 1,
        }
    }

    /// A unit: factors with powers, joined by `*` and `/`, in parentheses
    /// too.
    fn unit(&mut self) -> Result<Vec<UnitFactor>, ParseError> {
        let mut factors = Vec::new();
        let mut sign = 1;
        loop {
            if self.eat("(") {
                let inner = self.nested(Self::unit)?;
                self.expect(")")?;
                let power = sign * self.power_suffix();
                for f in inner {
                    // Powers of powers: `((mm^99)^99)^99` does not fit.
                    let Some(power) = power.checked_mul(f.power) else {
                        return error("a unit's power is out of range", self.offset());
                    };
                    factors.push(UnitFactor {
                        name: f.name,
                        power,
                    });
                }
            } else {
                let name = match self.peek() {
                    Some(Tok::Ident(n)) | Some(Tok::Symbol(n)) => n.clone(),
                    _ => return error("a unit expected", self.offset()),
                };
                self.at += 1;
                let power = self.power_suffix();
                factors.push(UnitFactor {
                    name,
                    power: sign * power,
                });
            }
            // `mm/s`, `N*m`: a further unit only.
            let joined = match self.peek() {
                Some(Tok::Op("*")) => 1,
                Some(Tok::Op("/")) => -1,
                _ => return Ok(factors),
            };
            let unit_next =
                self.unit_at(1) || matches!(self.peek_at(1), Some(Tok::Op("("))) && self.unit_at(2);
            if !unit_next {
                return Ok(factors);
            }
            self.at += 1;
            sign = joined;
        }
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        self.nested(Self::primary_inner)
    }

    fn primary_inner(&mut self) -> Result<Expr, ParseError> {
        let at = self.offset();
        let Some(tok) = self.peek().cloned() else {
            return error("an expression ended early", at);
        };
        match tok {
            Tok::Number(n) => {
                self.at += 1;
                Ok(Expr::Number(n))
            }
            Tok::Op("(") => {
                self.at += 1;
                let e = self.conditional()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::Symbol(_) => Ok(Expr::WithUnit(Box::new(Expr::Number(1.0)), self.unit()?)),
            Tok::Label(label) => {
                self.at += 1;
                if self.eat(".") {
                    let mut r = Reference {
                        label: Some(label),
                        ..Reference::default()
                    };
                    return self.path(&mut r, at);
                }
                Ok(Expr::Text(label))
            }
            Tok::Op(".") => {
                self.at += 1;
                let mut r = Reference {
                    relative: true,
                    ..Reference::default()
                };
                self.path(&mut r, at)
            }
            Tok::Range(from, to) => {
                self.at += 1;
                Ok(Expr::Range(
                    Reference {
                        path: vec![Component::Name(from)],
                        ..Reference::default()
                    },
                    to,
                ))
            }
            Tok::Ident(name) => {
                // A call.
                if matches!(self.peek_at(1), Some(Tok::Op("("))) {
                    self.at += 2;
                    let mut args = Vec::new();
                    if !self.eat(")") {
                        loop {
                            args.push(self.conditional()?);
                            if self.eat(")") {
                                break;
                            }
                            if !(self.eat(";") || self.eat(",")) {
                                return error("';' or ')' expected", self.offset());
                            }
                        }
                    }
                    return Ok(Expr::Call(name, args));
                }
                let follows = matches!(self.peek_at(1), Some(Tok::Op("." | "#" | "[")));
                if !follows {
                    match name.as_str() {
                        "pi" => {
                            self.at += 1;
                            return Ok(Expr::Constant(Constant::Pi));
                        }
                        "e" => {
                            self.at += 1;
                            return Ok(Expr::Constant(Constant::E));
                        }
                        n if is_unit(n) => {
                            return Ok(Expr::WithUnit(Box::new(Expr::Number(1.0)), self.unit()?));
                        }
                        _ => {}
                    }
                }
                let mut r = Reference::default();
                // Another document's object.
                if matches!(self.peek_at(1), Some(Tok::Op("#"))) {
                    self.at += 2;
                    r.document = Some(name);
                    if let Some(Tok::Label(label)) = self.peek().cloned() {
                        self.at += 1;
                        self.expect(".")?;
                        r.label = Some(label);
                    }
                }
                self.path(&mut r, at)
            }
            Tok::Op(op) => error(format!("unexpected '{op}'"), at),
        }
    }

    /// A reference's path: names joined by `.`, indices `[k]`, possibly
    /// ending in a range.
    fn path(&mut self, r: &mut Reference, at: usize) -> Result<Expr, ParseError> {
        loop {
            match self.peek().cloned() {
                Some(Tok::Ident(n)) => {
                    self.at += 1;
                    r.path.push(Component::Name(n));
                }
                Some(Tok::Range(from, to)) => {
                    self.at += 1;
                    r.path.push(Component::Name(from));
                    return Ok(Expr::Range(r.clone(), to));
                }
                _ => return error("a name expected", self.offset()),
            }
            while self.eat("[") {
                let negative = self.eat("-");
                match self.peek().cloned() {
                    Some(Tok::Number(n)) if n.fract() == 0.0 => {
                        self.at += 1;
                        let index = if negative { -n } else { n };
                        r.path.push(Component::Index(index as i64));
                    }
                    _ => return error("an index expected", self.offset()),
                }
                self.expect("]")?;
            }
            if !self.eat(".") {
                break;
            }
        }
        if r.path.is_empty() {
            return error("an empty reference", at);
        }
        Ok(Expr::Reference(r.clone()))
    }
}

impl Expr {
    /// Parses an expression (without a spreadsheet's leading `=`).
    pub fn parse(text: &str) -> Result<Expr, ParseError> {
        let tokens = lex(text)?;
        let mut parser = Parser {
            tokens,
            at: 0,
            len: text.len(),
            depth: 0,
        };
        let e = parser.conditional()?;
        if parser.at < parser.tokens.len() {
            return error("unexpected text after the expression", parser.offset());
        }
        Ok(e)
    }

    /// Every reference in it (ranges by their first cell).
    pub fn references(&self) -> Vec<&Reference> {
        let mut out = Vec::new();
        self.visit(&mut |e| match e {
            Expr::Reference(r) | Expr::Range(r, _) => out.push(r),
            _ => {}
        });
        out
    }

    fn visit<'a>(&'a self, f: &mut dyn FnMut(&'a Expr)) {
        f(self);
        match self {
            Expr::WithUnit(e, _) | Expr::Negate(e) | Expr::Plus(e) => e.visit(f),
            Expr::Binary(_, a, b) => {
                a.visit(f);
                b.visit(f);
            }
            Expr::Conditional(c, a, b) => {
                c.visit(f);
                a.visit(f);
                b.visit(f);
            }
            Expr::Call(_, args) => args.iter().for_each(|a| a.visit(f)),
            _ => {}
        }
    }
}

/// A cell address's column (from 0) and row (from 1): `B3` is (1, 3).
pub fn cell_position(address: &str) -> Option<(u32, u32)> {
    if !is_cell_address(address) {
        return None;
    }
    let letters = address.bytes().take_while(u8::is_ascii_uppercase).count();
    let column = address[..letters]
        .bytes()
        .fold(0u32, |c, b| c * 26 + u32::from(b - b'A') + 1)
        - 1;
    let row = address[letters..].parse().ok()?;
    Some((column, row))
}

/// A cell's address from its column (from 0) and row (from 1).
pub fn cell_address(column: u32, row: u32) -> String {
    let mut letters = Vec::new();
    let mut c = column + 1;
    while c > 0 {
        let r = (c - 1) % 26;
        letters.push(char::from(b'A' + r as u8));
        c = (c - 1) / 26;
    }
    letters.reverse();
    format!("{}{row}", letters.into_iter().collect::<String>())
}

/// The cells of a range, row by row (`A1:B2` is A1, B1, A2, B2).
pub fn range_cells(from: &str, to: &str) -> Option<Vec<String>> {
    let (c0, r0) = cell_position(from)?;
    let (c1, r1) = cell_position(to)?;
    let (c0, c1) = (c0.min(c1), c0.max(c1));
    let (r0, r1) = (r0.min(r1), r0.max(r1));
    if u64::from(c1 - c0 + 1) * u64::from(r1 - r0 + 1) > 10_000 {
        return None;
    }
    Some(
        (r0..=r1)
            .flat_map(|r| (c0..=c1).map(move |c| cell_address(c, r)))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(n: &str) -> Component {
        Component::Name(n.to_owned())
    }

    fn reference(path: &[&str]) -> Expr {
        Expr::Reference(Reference {
            path: path.iter().map(|n| name(n)).collect(),
            ..Reference::default()
        })
    }

    fn num(v: f64) -> Box<Expr> {
        Box::new(Expr::Number(v))
    }

    fn unit(n: &str, power: i32) -> UnitFactor {
        UnitFactor {
            name: n.to_owned(),
            power,
        }
    }

    #[test]
    fn numbers_with_units() {
        assert_eq!(
            Expr::parse("30 mm").unwrap(),
            Expr::WithUnit(num(30.0), vec![unit("mm", 1)])
        );
        assert_eq!(
            Expr::parse("30mm").unwrap(),
            Expr::WithUnit(num(30.0), vec![unit("mm", 1)])
        );
        assert_eq!(
            Expr::parse("2.5e-1 mm^2").unwrap(),
            Expr::WithUnit(num(0.25), vec![unit("mm", 2)])
        );
        assert_eq!(
            Expr::parse("9.81 m/mm^2").unwrap(),
            Expr::WithUnit(num(9.81), vec![unit("m", 1), unit("mm", -2)])
        );
        // Other quantities' units are names (references that do not
        // resolve).
        assert!(matches!(
            Expr::parse("9.81 m/s^2").unwrap(),
            Expr::Binary(BinaryOp::Div, _, _)
        ));
        assert_eq!(
            Expr::parse("30 °").unwrap(),
            Expr::WithUnit(num(30.0), vec![unit("°", 1)])
        );
        // FreeCAD writes a unit's power in parentheses.
        assert_eq!(
            Expr::parse("sqrt(16 (mm ^ 2))").unwrap(),
            Expr::Call(
                "sqrt".into(),
                vec![Expr::WithUnit(num(16.0), vec![unit("mm", 2)])]
            )
        );
        assert_eq!(
            Expr::parse("2 (m / in) ^ 2").unwrap(),
            Expr::WithUnit(num(2.0), vec![unit("m", 2), unit("in", -2)])
        );
        assert_eq!(
            Expr::parse("2\"").unwrap(),
            Expr::WithUnit(num(2.0), vec![unit("\"", 1)])
        );
        // A unit binds to its number.
        assert_eq!(
            Expr::parse("B3 * 2 mm").unwrap(),
            Expr::Binary(
                BinaryOp::Mul,
                Box::new(reference(&["B3"])),
                Box::new(Expr::WithUnit(num(2.0), vec![unit("mm", 1)]))
            )
        );
        assert_eq!(
            Expr::parse("10 mm / 2").unwrap(),
            Expr::Binary(
                BinaryOp::Div,
                Box::new(Expr::WithUnit(num(10.0), vec![unit("mm", 1)])),
                num(2.0)
            )
        );
    }

    #[test]
    fn references() {
        assert_eq!(
            Expr::parse("Spreadsheet.Width").unwrap(),
            reference(&["Spreadsheet", "Width"])
        );
        assert_eq!(
            Expr::parse("<<My sheet>>.B3").unwrap(),
            Expr::Reference(Reference {
                label: Some("My sheet".to_owned()),
                path: vec![name("B3")],
                ..Reference::default()
            })
        );
        assert_eq!(
            Expr::parse(".Constraints.width / 2").unwrap(),
            Expr::Binary(
                BinaryOp::Div,
                Box::new(Expr::Reference(Reference {
                    relative: true,
                    path: vec![name("Constraints"), name("width")],
                    ..Reference::default()
                })),
                num(2.0)
            )
        );
        assert_eq!(
            Expr::parse("Sketch.Constraints[2]").unwrap(),
            Expr::Reference(Reference {
                path: vec![name("Sketch"), name("Constraints"), Component::Index(2)],
                ..Reference::default()
            })
        );
        assert_eq!(
            Expr::parse("doc#Pad.Length").unwrap(),
            Expr::Reference(Reference {
                document: Some("doc".to_owned()),
                path: vec![name("Pad"), name("Length")],
                ..Reference::default()
            })
        );
        assert_eq!(
            Expr::parse(".Height.Value").unwrap(),
            Expr::Reference(Reference {
                relative: true,
                path: vec![name("Height"), name("Value")],
                ..Reference::default()
            })
        );
        assert_eq!(
            Expr::parse("doc#<<My pad>>.Length").unwrap(),
            Expr::Reference(Reference {
                document: Some("doc".to_owned()),
                label: Some("My pad".to_owned()),
                path: vec![name("Length")],
                ..Reference::default()
            })
        );
        assert_eq!(Expr::parse("<<text>>").unwrap(), Expr::Text("text".into()));
        assert_eq!(
            Expr::parse("pi * e").unwrap(),
            Expr::Binary(
                BinaryOp::Mul,
                Box::new(Expr::Constant(Constant::Pi)),
                Box::new(Expr::Constant(Constant::E))
            )
        );
    }

    #[test]
    fn calls_ranges_and_conditionals() {
        assert_eq!(
            Expr::parse("hypot(3 mm; 4 mm)").unwrap(),
            Expr::Call(
                "hypot".into(),
                vec![
                    Expr::WithUnit(num(3.0), vec![unit("mm", 1)]),
                    Expr::WithUnit(num(4.0), vec![unit("mm", 1)])
                ]
            )
        );
        let Expr::Call(_, args) = Expr::parse("sum(Sheet.A1:A3, B1:C1)").unwrap() else {
            panic!("a call");
        };
        assert_eq!(
            args[0],
            Expr::Range(
                Reference {
                    path: vec![name("Sheet"), name("A1")],
                    ..Reference::default()
                },
                "A3".into()
            )
        );
        assert!(matches!(&args[1], Expr::Range(_, to) if to == "C1"));
        let e = Expr::parse("w > 20 mm ? 5 mm : 7 mm").unwrap();
        assert!(
            matches!(e, Expr::Conditional(c, _, _) if matches!(*c, Expr::Binary(BinaryOp::Gt, _, _)))
        );
        // Nested in the middle; left-associative after it.
        let e = Expr::parse("a ? b ? 1 : 2 : 3").unwrap();
        assert!(matches!(e, Expr::Conditional(_, t, _) if matches!(*t, Expr::Conditional(..))));
        let e = Expr::parse("a ? 1 : b ? 2 : 3").unwrap();
        assert!(matches!(e, Expr::Conditional(c, _, _) if matches!(*c, Expr::Conditional(..))));
    }

    #[test]
    fn signs_bind_tighter_than_powers() {
        assert_eq!(
            Expr::parse("-2^2").unwrap(),
            Expr::Binary(BinaryOp::Pow, Box::new(Expr::Negate(num(2.0))), num(2.0))
        );
        // Left-associative powers.
        assert_eq!(
            Expr::parse("2^3^2").unwrap(),
            Expr::Binary(
                BinaryOp::Pow,
                Box::new(Expr::Binary(BinaryOp::Pow, num(2.0), num(3.0))),
                num(2.0)
            )
        );
    }

    #[test]
    fn errors() {
        assert!(Expr::parse("1 +").is_err());
        assert!(Expr::parse("(1").is_err());
        assert!(Expr::parse("1 2").is_err());
        assert!(Expr::parse("<<open").is_err());
    }

    /// The depth of a parsed tree (by a walk of its own, with a stack).
    fn tree_depth(e: &Expr) -> usize {
        let mut deepest = 0;
        let mut stack: Vec<(&Expr, usize)> = vec![(e, 1)];
        while let Some((e, depth)) = stack.pop() {
            deepest = deepest.max(depth);
            match e {
                Expr::WithUnit(a, _) | Expr::Negate(a) | Expr::Plus(a) => {
                    stack.push((a.as_ref(), depth + 1));
                }
                Expr::Binary(_, a, b) => {
                    stack.push((a.as_ref(), depth + 1));
                    stack.push((b.as_ref(), depth + 1));
                }
                Expr::Conditional(c, a, b) => {
                    stack.push((c.as_ref(), depth + 1));
                    stack.push((a.as_ref(), depth + 1));
                    stack.push((b.as_ref(), depth + 1));
                }
                Expr::Call(_, args) => stack.extend(args.iter().map(|a| (a, depth + 1))),
                _ => {}
            }
        }
        deepest
    }

    #[test]
    fn hostile_nesting_is_an_error_not_a_stack_overflow() {
        // Parentheses, signs, chains of operators and of conditionals,
        // units in parentheses, far past any expression FreeCAD writes.
        let open = Expr::parse(&"(".repeat(100_000)).unwrap_err();
        assert_eq!(open.message, "the expression nests too deep");
        let parens = format!("{}1{}", "(".repeat(50_000), ")".repeat(50_000));
        assert!(Expr::parse(&parens).is_err());
        assert!(Expr::parse(&format!("{}1", "-".repeat(400_000))).is_err());
        assert!(Expr::parse(&format!("{}1", "+ ".repeat(1_000))).is_err());
        assert!(Expr::parse(&format!("1{}", "+1".repeat(100_000))).is_err());
        assert!(Expr::parse(&format!("1{}", "^2".repeat(1_000))).is_err());
        assert!(Expr::parse(&format!("a{}", " ? 1 : 2".repeat(1_000))).is_err());
        assert!(Expr::parse(&format!("a{}", " ? b".repeat(1_000))).is_err());
        let calls = format!("{}1{}", "abs(".repeat(1_000), ")".repeat(1_000));
        assert!(Expr::parse(&calls).is_err());
        let units = format!("1 mm{}", "*(mm".repeat(1_000));
        assert!(Expr::parse(&units).is_err());
        // Powers of unit powers that do not fit an integer.
        let powers = format!("1 {}mm{}", "mm*(".repeat(8), ")^99".repeat(8));
        assert_eq!(
            Expr::parse(&powers).unwrap_err().message,
            "a unit's power is out of range"
        );
        // Ordinary nesting reads; the tree stays within the limit.
        let fine = format!("{}1 mm{}", "(".repeat(20), ")".repeat(20));
        assert!(Expr::parse(&fine).is_ok());
        let sum = Expr::parse(&format!("1{}", " + 1".repeat(100))).unwrap();
        assert_eq!(tree_depth(&sum), 101);
        // A chain as long as the limit is refused (the tree would be
        // deeper than it).
        assert!(Expr::parse(&format!("1{}", "+1".repeat(MAX_DEPTH))).is_err());
    }

    #[test]
    fn cells() {
        assert!(is_cell_address("B12"));
        assert!(is_cell_address("AB1"));
        assert!(!is_cell_address("B0"));
        assert!(!is_cell_address("Width"));
        assert_eq!(cell_position("AB12"), Some((27, 12)));
        assert_eq!(cell_address(27, 12), "AB12");
        assert_eq!(
            range_cells("A1", "B2").unwrap(),
            ["A1", "B1", "A2", "B2"].map(String::from)
        );
    }
}
