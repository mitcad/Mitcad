// SPDX-License-Identifier: MIT
//! ISO 10303-21 ("STEP physical file") text: the exchange structure IFC
//! files use. Written from the standard's clear-text encoding rules.
//!
//! - [`Records`] splits a byte stream into records (`...;`), skipping
//!   comments (`/* ... */`) and keeping strings whole, so that a file is
//!   read a record at a time and never held whole.
//! - [`parse_record`] parses one record: a header entity
//!   (`FILE_SCHEMA(('IFC4'));`), a section keyword (`DATA;`) or an entity
//!   instance (`#12=IFCWALL(...);`, or a complex instance
//!   `#12=(A(...)B(...));`).
//! - [`Value`] is a parameter: `$` (unset), `*` (derived), integers, reals,
//!   strings (with their `\X2\...\X0\` and other escapes decoded),
//!   enumerations (`.T.`), binaries, references (`#5`), lists and typed
//!   values (`IFCLABEL('x')`).

use std::fmt;
use std::io::BufRead;

/// A parameter of an entity instance.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `$`: not given.
    Null,
    /// `*`: derived from other attributes.
    Derived,
    Integer(i64),
    Real(f64),
    String(String),
    /// `.NAME.` without the dots (booleans and logicals are `T`, `F`, `U`).
    Enum(String),
    /// `"..."`: hexadecimal digits, the first the count of unused bits.
    Binary(String),
    /// `#id`.
    Ref(u64),
    List(Vec<Value>),
    /// A value of a defined type, `IFCLABEL('x')`: the type in upper case.
    Typed(String, Box<Value>),
}

impl Value {
    /// The referenced instance, also through a typed value.
    pub fn as_ref(&self) -> Option<u64> {
        match self {
            Value::Ref(id) => Some(*id),
            Value::Typed(_, v) => Value::as_ref(v),
            _ => None,
        }
    }

    /// A number (integers too), also through a typed value.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Real(v) => Some(*v),
            Value::Integer(v) => Some(*v as f64),
            Value::Typed(_, v) => v.as_f64(),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Integer(v) => Some(*v),
            Value::Real(v) if v.fract() == 0.0 && v.abs() < 9.0e15 => Some(*v as i64),
            Value::Typed(_, v) => v.as_i64(),
            _ => None,
        }
    }

    /// A string, also through a typed value.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            Value::Typed(_, v) => v.as_str(),
            _ => None,
        }
    }

    /// An enumeration's name (without dots), also through a typed value.
    pub fn as_enum(&self) -> Option<&str> {
        match self {
            Value::Enum(s) => Some(s),
            Value::Typed(_, v) => v.as_enum(),
            _ => None,
        }
    }

    /// A boolean or logical: `.T.` true, `.F.` false, `.U.` none.
    pub fn as_bool(&self) -> Option<bool> {
        match self.as_enum() {
            Some("T") => Some(true),
            Some("F") => Some(false),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(v) => Some(v),
            Value::Typed(_, v) => v.as_list(),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// The references of a list (other values left out).
    pub fn refs(&self) -> Vec<u64> {
        self.as_list()
            .map(|l| l.iter().filter_map(Value::as_ref).collect())
            .unwrap_or_default()
    }

    /// The numbers of a list (none when one is not a number).
    pub fn numbers(&self) -> Option<Vec<f64>> {
        self.as_list()?.iter().map(Value::as_f64).collect()
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => f.write_str("$"),
            Value::Derived => f.write_str("*"),
            Value::Integer(v) => write!(f, "{v}"),
            Value::Real(v) => write!(f, "{v:?}"),
            Value::String(s) => write!(f, "'{}'", s.replace('\'', "''")),
            Value::Enum(s) => write!(f, ".{s}."),
            Value::Binary(s) => write!(f, "\"{s}\""),
            Value::Ref(id) => write!(f, "#{id}"),
            Value::List(items) => {
                f.write_str("(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str(")")
            }
            Value::Typed(name, v) => write!(f, "{name}({v})"),
        }
    }
}

/// A record of the file.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    /// `ISO-10303-21`, `HEADER`, `DATA`, `ENDSEC`, `END-ISO-10303-21`.
    Keyword(String),
    /// A header entity: `FILE_NAME(...)`.
    Header(String, Vec<Value>),
    /// `#id=NAME(...)`.
    Instance(u64, String, Vec<Value>),
    /// `#id=(A(...)B(...))`: the partial entities in the file's order.
    Complex(u64, Vec<(String, Vec<Value>)>),
}

/// Why a record cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepError {
    /// The byte offset of the record in the file.
    pub offset: u64,
    pub message: String,
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for StepError {}

/// The records of a byte stream: each record's text without comments
/// (strings keep theirs), with its offset. Reads a buffer at a time.
pub struct Records<R> {
    reader: R,
    offset: u64,
    done: bool,
}

impl<R: BufRead> Records<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            offset: 0,
            done: false,
        }
    }

    /// The bytes read so far.
    pub fn offset(&self) -> u64 {
        self.offset
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lex {
    Code,
    /// After `/` in code: a comment may start.
    Slash,
    Comment,
    /// After `*` in a comment: it may end.
    CommentStar,
    String,
    /// After `'` in a string: the string ends unless another follows.
    Quote,
    Binary,
}

impl<R: BufRead> Iterator for Records<R> {
    type Item = Result<(u64, Vec<u8>), StepError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let mut record = Vec::new();
        let mut start: Option<u64> = None;
        let mut state = Lex::Code;
        loop {
            let buffer = match self.reader.fill_buf() {
                Ok(b) => b,
                Err(e) => {
                    self.done = true;
                    return Some(Err(StepError {
                        offset: self.offset,
                        message: e.to_string(),
                    }));
                }
            };
            if buffer.is_empty() {
                self.done = true;
                if state == Lex::Slash {
                    record.push(b'/');
                }
                return if record.iter().all(u8::is_ascii_whitespace) {
                    None
                } else {
                    Some(Err(StepError {
                        offset: start.unwrap_or(self.offset),
                        message: "the file ends inside a record".to_owned(),
                    }))
                };
            }
            let mut used = 0;
            let mut end = false;
            for &byte in buffer {
                used += 1;
                let here = self.offset + used as u64 - 1;
                match state {
                    Lex::Code => match byte {
                        b'/' => state = Lex::Slash,
                        b'\'' => {
                            record.push(byte);
                            state = Lex::String;
                        }
                        b'"' => {
                            record.push(byte);
                            state = Lex::Binary;
                        }
                        b';' => {
                            end = true;
                            break;
                        }
                        _ => {
                            if start.is_none() && !byte.is_ascii_whitespace() {
                                start = Some(here);
                            }
                            record.push(byte);
                        }
                    },
                    Lex::Slash => match byte {
                        b'*' => state = Lex::Comment,
                        _ => {
                            record.push(b'/');
                            if byte == b';' {
                                end = true;
                                break;
                            }
                            record.push(byte);
                            state = Lex::Code;
                        }
                    },
                    Lex::Comment => {
                        if byte == b'*' {
                            state = Lex::CommentStar;
                        }
                    }
                    Lex::CommentStar => match byte {
                        b'/' => {
                            // A comment separates like white space.
                            record.push(b' ');
                            state = Lex::Code;
                        }
                        b'*' => {}
                        _ => state = Lex::Comment,
                    },
                    Lex::String => {
                        record.push(byte);
                        if byte == b'\'' {
                            state = Lex::Quote;
                        }
                    }
                    Lex::Quote => {
                        if byte == b'\'' {
                            // A doubled apostrophe stays in the string.
                            record.push(byte);
                            state = Lex::String;
                        } else {
                            state = Lex::Code;
                            match byte {
                                b';' => {
                                    end = true;
                                    break;
                                }
                                b'/' => state = Lex::Slash,
                                _ => record.push(byte),
                            }
                        }
                    }
                    Lex::Binary => {
                        record.push(byte);
                        if byte == b'"' {
                            state = Lex::Code;
                        }
                    }
                }
            }
            self.reader.consume(used);
            self.offset += used as u64;
            if end {
                let at = start.unwrap_or(self.offset);
                return Some(Ok((at, record)));
            }
        }
    }
}

/// Parses one record's text (as [`Records`] gives it, without the `;`).
pub fn parse_record(offset: u64, text: &[u8]) -> Result<Record, StepError> {
    let mut p = Parser {
        text,
        at: 0,
        offset,
        depth: 0,
    };
    p.skip_space();
    let record = if p.peek() == Some(b'#') {
        p.at += 1;
        let id = p.digits()?;
        p.skip_space();
        p.expect(b'=')?;
        p.skip_space();
        if p.peek() == Some(b'(') {
            p.at += 1;
            let mut parts = Vec::new();
            loop {
                p.skip_space();
                if p.peek() == Some(b')') {
                    p.at += 1;
                    break;
                }
                let name = p.keyword()?;
                p.skip_space();
                let args = p.arguments()?;
                parts.push((name, args));
            }
            Record::Complex(id, parts)
        } else {
            let name = p.keyword()?;
            p.skip_space();
            let args = p.arguments()?;
            Record::Instance(id, name, args)
        }
    } else {
        let name = p.keyword()?;
        p.skip_space();
        if p.peek() == Some(b'(') {
            let args = p.arguments()?;
            if matches!(name.as_str(), "DATA") {
                Record::Keyword(name)
            } else {
                Record::Header(name, args)
            }
        } else {
            Record::Keyword(name)
        }
    };
    p.skip_space();
    if p.at < p.text.len() {
        return Err(p.error("text after the record's end"));
    }
    Ok(record)
}

/// How deeply lists and typed values may nest in a record. IFC's deepest
/// values (lists of coordinate lists in a typed value) need a few levels;
/// the limit keeps a hostile record (thousands of `(`) from overflowing
/// the stack of the recursive parser.
pub const MAX_NESTING: usize = 64;

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
    offset: u64,
    /// The `(` levels open now ([`MAX_NESTING`] at most).
    depth: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> StepError {
        StepError {
            offset: self.offset,
            message: format!("{message} (character {} of the record)", self.at),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), StepError> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.error(&format!("'{}' expected", byte as char)))
        }
    }

    fn digits(&mut self) -> Result<u64, StepError> {
        let start = self.at;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.at += 1;
        }
        std::str::from_utf8(&self.text[start..self.at])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| self.error("an instance number expected"))
    }

    /// A keyword: letters, digits, `_` and `-` (`END-ISO-10303-21`), upper
    /// case.
    fn keyword(&mut self) -> Result<String, StepError> {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'!')
        {
            self.at += 1;
        }
        if self.at == start {
            return Err(self.error("a keyword expected"));
        }
        Ok(String::from_utf8_lossy(&self.text[start..self.at]).to_ascii_uppercase())
    }

    /// `( value, value, ... )`, nested at most [`MAX_NESTING`] deep.
    fn arguments(&mut self) -> Result<Vec<Value>, StepError> {
        if self.depth >= MAX_NESTING {
            return Err(self.error("values nested too deeply"));
        }
        self.depth += 1;
        let values = self.argument_list();
        self.depth -= 1;
        values
    }

    fn argument_list(&mut self) -> Result<Vec<Value>, StepError> {
        self.expect(b'(')?;
        let mut values = Vec::new();
        self.skip_space();
        if self.peek() == Some(b')') {
            self.at += 1;
            return Ok(values);
        }
        loop {
            self.skip_space();
            values.push(self.value()?);
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b')') => {
                    self.at += 1;
                    return Ok(values);
                }
                _ => return Err(self.error("',' or ')' expected")),
            }
        }
    }

    fn value(&mut self) -> Result<Value, StepError> {
        match self.peek() {
            Some(b'$') => {
                self.at += 1;
                Ok(Value::Null)
            }
            Some(b'*') => {
                self.at += 1;
                Ok(Value::Derived)
            }
            Some(b'#') => {
                self.at += 1;
                Ok(Value::Ref(self.digits()?))
            }
            Some(b'\'') => self.string(),
            Some(b'"') => {
                self.at += 1;
                let start = self.at;
                while self.peek().is_some_and(|b| b != b'"') {
                    self.at += 1;
                }
                let text = String::from_utf8_lossy(&self.text[start..self.at]).into_owned();
                self.expect(b'"')?;
                Ok(Value::Binary(text))
            }
            Some(b'.') => {
                self.at += 1;
                let start = self.at;
                while self.peek().is_some_and(|b| b != b'.') {
                    self.at += 1;
                }
                let name = String::from_utf8_lossy(&self.text[start..self.at])
                    .trim()
                    .to_ascii_uppercase();
                self.expect(b'.')?;
                Ok(Value::Enum(name))
            }
            Some(b'(') => Ok(Value::List(self.arguments()?)),
            Some(b) if b == b'-' || b == b'+' || b.is_ascii_digit() => self.number(),
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => {
                let name = self.keyword()?;
                self.skip_space();
                let mut args = self.arguments()?;
                let inner = if args.len() == 1 {
                    args.remove(0)
                } else {
                    Value::List(args)
                };
                Ok(Value::Typed(name, Box::new(inner)))
            }
            _ => Err(self.error("a value expected")),
        }
    }

    fn number(&mut self) -> Result<Value, StepError> {
        let start = self.at;
        let mut real = false;
        if matches!(self.peek(), Some(b'-' | b'+')) {
            self.at += 1;
        }
        while let Some(b) = self.peek() {
            match b {
                b'0'..=b'9' => {}
                b'.' => real = true,
                b'E' | b'e' => {
                    real = true;
                    if matches!(self.text.get(self.at + 1), Some(b'-' | b'+')) {
                        self.at += 1;
                    }
                }
                _ => break,
            }
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.text[start..self.at]).unwrap_or_default();
        if !real && let Ok(v) = text.parse::<i64>() {
            return Ok(Value::Integer(v));
        }
        // `1.E-5` and `1.` are reals; Rust reads `1.E-5` as well.
        text.parse::<f64>()
            .map(Value::Real)
            .map_err(|_| self.error(&format!("'{text}' is no number")))
    }

    /// A string with its escapes decoded (ISO 10303-21, 6.4.3).
    fn string(&mut self) -> Result<Value, StepError> {
        self.expect(b'\'')?;
        let mut bytes: Vec<u8> = Vec::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(self.error("the string does not end"));
            };
            self.at += 1;
            match b {
                b'\'' => {
                    if self.peek() == Some(b'\'') {
                        self.at += 1;
                        bytes.push(b'\'');
                    } else {
                        break;
                    }
                }
                b'\\' => self.escape(&mut bytes),
                _ => bytes.push(b),
            }
        }
        Ok(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    }

    /// One escape after its backslash; what cannot be decoded stays as it
    /// is.
    fn escape(&mut self, out: &mut Vec<u8>) {
        let rest = &self.text[self.at..];
        let push = |out: &mut Vec<u8>, c: char| {
            let mut buf = [0; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        };
        let hex = |s: &[u8]| {
            std::str::from_utf8(s)
                .ok()
                .and_then(|s| u32::from_str_radix(s, 16).ok())
        };
        if rest.first() == Some(&b'\\') {
            self.at += 1;
            out.push(b'\\');
        } else if rest.starts_with(b"X2\\") || rest.starts_with(b"X4\\") {
            let width = if rest[1] == b'2' { 4 } else { 8 };
            let Some(end) = find(&rest[3..], b"\\X0\\") else {
                out.push(b'\\');
                return;
            };
            let digits = &rest[3..3 + end];
            self.at += 3 + end + 4;
            if width == 4 {
                let units: Vec<u16> = digits
                    .chunks(4)
                    .filter_map(|c| hex(c).map(|v| v as u16))
                    .collect();
                for c in char::decode_utf16(units) {
                    push(out, c.unwrap_or(char::REPLACEMENT_CHARACTER));
                }
            } else {
                for chunk in digits.chunks(8) {
                    let c = hex(chunk)
                        .and_then(char::from_u32)
                        .unwrap_or(char::REPLACEMENT_CHARACTER);
                    push(out, c);
                }
            }
        } else if rest.starts_with(b"X\\") && rest.len() >= 4 {
            match hex(&rest[2..4]).and_then(char::from_u32) {
                Some(c) => {
                    self.at += 4;
                    push(out, c);
                }
                None => out.push(b'\\'),
            }
        } else if rest.starts_with(b"S\\") && rest.len() >= 3 {
            // A character of the upper half of the code page (ISO 8859-1
            // by default).
            self.at += 3;
            push(out, char::from(rest[2].wrapping_add(128)));
        } else if rest.len() >= 3 && rest[0] == b'P' && rest[2] == b'\\' {
            // The code page for \S\: only ISO 8859-1 is read.
            self.at += 3;
        } else if rest.starts_with(b"N\\") {
            self.at += 2;
            out.push(b'\n');
        } else {
            out.push(b'\\');
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records(text: &str) -> Vec<Record> {
        Records::new(text.as_bytes())
            .map(|r| {
                let (offset, bytes) = r.expect("a record");
                parse_record(offset, &bytes).expect("parses")
            })
            .collect()
    }

    #[test]
    fn records_skip_comments_and_keep_strings() {
        let text = "ISO-10303-21;\nHEADER;/* a ; comment */\nFILE_SCHEMA(('IFC4'));\n\
                    ENDSEC;DATA;\n#1=IFCLABELLED('a;b /* not a comment */', 'it''s');\n\
                    #2 = IFCX(#1,$,*,.T.,(1,2.5,-3.E-2),IFCLABEL('x'),\"0FF\");ENDSEC;END-ISO-10303-21;";
        let r = records(text);
        assert_eq!(r[0], Record::Keyword("ISO-10303-21".into()));
        assert_eq!(r[1], Record::Keyword("HEADER".into()));
        assert_eq!(
            r[2],
            Record::Header(
                "FILE_SCHEMA".into(),
                vec![Value::List(vec![Value::String("IFC4".into())])]
            )
        );
        assert_eq!(
            r[5],
            Record::Instance(
                1,
                "IFCLABELLED".into(),
                vec![
                    Value::String("a;b /* not a comment */".into()),
                    Value::String("it's".into())
                ]
            )
        );
        let Record::Instance(2, name, args) = &r[6] else {
            panic!("an instance: {:?}", r[6]);
        };
        assert_eq!(name, "IFCX");
        assert_eq!(args[0], Value::Ref(1));
        assert_eq!(args[1], Value::Null);
        assert_eq!(args[2], Value::Derived);
        assert_eq!(args[3].as_bool(), Some(true));
        assert_eq!(args[4].numbers(), Some(vec![1.0, 2.5, -0.03]));
        assert_eq!(
            args[4].as_list().map(|l| l[0].clone()),
            Some(Value::Integer(1))
        );
        assert_eq!(args[5].as_str(), Some("x"));
        assert_eq!(args[6], Value::Binary("0FF".into()));
        assert_eq!(r.len(), 9);
    }

    #[test]
    fn escapes_decode() {
        let r = records(
            "#1=A('\\X2\\00C400E4\\X0\\ \\X\\E9 \\S\\D \\\\ \\X4\\0001F600\\X0\\ \\PA\\x');",
        );
        let Record::Instance(_, _, args) = &r[0] else {
            panic!()
        };
        assert_eq!(args[0].as_str(), Some("Ää é Ä \\ \u{1F600} x"));
        // UTF-8 written as it is (common in exports).
        let r = records("#1=A('V\u{e4}ggar');");
        let Record::Instance(_, _, args) = &r[0] else {
            panic!()
        };
        assert_eq!(args[0].as_str(), Some("V\u{e4}ggar"));
    }

    #[test]
    fn complex_instances_and_errors() {
        let r = records("#7=(IFCA(1)IFCB('x',$));");
        assert_eq!(
            r[0],
            Record::Complex(
                7,
                vec![
                    ("IFCA".into(), vec![Value::Integer(1)]),
                    ("IFCB".into(), vec![Value::String("x".into()), Value::Null])
                ]
            )
        );
        let bad: Vec<_> = Records::new("#1=A(1,;".as_bytes()).collect();
        let (offset, bytes) = bad[0].clone().expect("a record");
        assert!(parse_record(offset, &bytes).is_err());
        let open: Vec<_> = Records::new("#1=A('x".as_bytes()).collect();
        assert!(open[0].is_err());
    }

    #[test]
    fn deep_nesting_fails_the_record_not_the_stack() {
        // mitcad#930: 100 000 open lists would overflow the stack of the
        // recursive parser; the record fails instead.
        let mut text = String::from("#1=IFCWALL(");
        text.push_str(&"(".repeat(100_000));
        text.push_str("))");
        let error = parse_record(0, text.as_bytes()).expect_err("too deep");
        assert!(
            error.message.contains("nested too deeply"),
            "{}",
            error.message
        );
        // Nesting up to the limit still reads (the record's own list is
        // the first level).
        let inner = MAX_NESTING - 1;
        let text = format!("#2=IFCA({}1{})", "(".repeat(inner), ")".repeat(inner));
        let Record::Instance(2, _, args) = parse_record(0, text.as_bytes()).expect("reads") else {
            panic!("an instance");
        };
        let mut value = &args[0];
        for _ in 1..inner {
            value = &value.as_list().expect("a list")[0];
        }
        assert_eq!(value.as_list(), Some(&[Value::Integer(1)][..]));
        let text = format!(
            "#3=IFCA({}1{})",
            "(".repeat(inner + 1),
            ")".repeat(inner + 1)
        );
        assert!(parse_record(0, text.as_bytes()).is_err());
        // Typed values count as levels too.
        let text = format!(
            "#4=IFCA({}1{})",
            "IFCLABEL(".repeat(MAX_NESTING),
            ")".repeat(MAX_NESTING)
        );
        assert!(parse_record(0, text.as_bytes()).is_err());
    }

    #[test]
    fn small_buffers_split_nothing() {
        let text = "#1=IFCA('a''b',/* c */#2);\n#2=IFCB((0.,1.,2.));";
        let reader = std::io::BufReader::with_capacity(1, text.as_bytes());
        let r: Vec<Record> = Records::new(reader)
            .map(|r| {
                let (o, b) = r.expect("record");
                parse_record(o, &b).expect("parses")
            })
            .collect();
        assert_eq!(
            r[0],
            Record::Instance(
                1,
                "IFCA".into(),
                vec![Value::String("a'b".into()), Value::Ref(2)]
            )
        );
        assert_eq!(
            r[1],
            Record::Instance(
                2,
                "IFCB".into(),
                vec![Value::List(vec![
                    Value::Real(0.0),
                    Value::Real(1.0),
                    Value::Real(2.0)
                ])]
            )
        );
    }
}
