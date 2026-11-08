// SPDX-License-Identifier: MIT
//! The document's parameters as CSV (mitcad#666): Mitcad's own table and
//! the parameter table CSV of the `.f3d` application, which Mitcad reads
//! but does not write.
//!
//! Mitcad's table has the header
//! `name,kind,unit,expression,value,comment,favorite,min,max,step,strict,values`:
//! one row per parameter in creation order, its kind (`user`, `model`,
//! `reference`), its unit (`""` for unitless, `Text` for a text parameter,
//! mitcad#664), its expression, its value as a number in its unit (a text
//! parameter's text as it is), its comment, whether it is a favourite
//! (`true`, `false`) and its limits (mitcad#665): `min`, `max` and `step`
//! as written (empty when not set) and `strict` (`true`, `false`), and its
//! allowed values (mitcad#798): `values`, the list as a JSON array of
//! expression texts (`["300 mm","500 mm"]`; empty when not set).
//!
//! The `.f3d` application's parameter table CSV has the header
//! `Name,Unit,Expression,Value,Comments,Favorite`: an empty unit for
//! unitless parameters, `°` for degrees and `Text` for text parameters
//! (whose expressions are quoted strings such as `'6qt'`).
//!
//! Both are UTF-8 (a byte order mark is skipped) with RFC 4180 quoting:
//! a field with a comma, a quote, a line break or leading or trailing
//! space is quoted, and a quote in it is doubled. Numbers have decimal
//! points. Rows end with CRLF when written; any line break is read. A
//! table whose header has no comma but semicolons is read with `;`
//! between fields. Columns are found by their names in the header (any
//! case, any order); `name` and `expression` are needed.
//!
//! What an import does with the rows is `document/parameters_csv.rs`.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use serde::Serialize;

use crate::ParamId;
use crate::expr::{is_reserved_name, is_valid_name, value_to_expression};
use crate::parameters::Parameters;

/// The header of Mitcad's own table.
pub const HEADER: [&str; 12] = [
    "name",
    "kind",
    "unit",
    "expression",
    "value",
    "comment",
    "favorite",
    "min",
    "max",
    "step",
    "strict",
    "values",
];

/// The header of the `.f3d` application's parameter table CSV.
pub const PARAMETER_TABLE_HEADER: [&str; 6] = [
    "Name",
    "Unit",
    "Expression",
    "Value",
    "Comments",
    "Favorite",
];

/// Which table a CSV is, by its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CsvFormat {
    /// Mitcad's own table ([`HEADER`], or any header with `name` and
    /// `expression` columns).
    Mitcad,
    /// The `.f3d` application's parameter table ([`PARAMETER_TABLE_HEADER`]).
    ParameterTable,
}

/// A row of a parameter CSV, with the cells of the columns the header has
/// (trimmed, except the comment).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CsvRow {
    /// The line of the file the row starts on (the header is line 1).
    pub line: usize,
    pub name: String,
    /// `user`, `model`, `reference` or `text` (Mitcad's table only).
    pub kind: Option<String>,
    pub unit: Option<String>,
    pub expression: String,
    pub value: Option<String>,
    pub comment: Option<String>,
    /// None when the column is missing or its cell is empty or not a
    /// truth value.
    pub favorite: Option<bool>,
    /// The limits (mitcad#665): the cells of `min`, `max` and `step`
    /// (trimmed; empty for no limit), None when the table has no such
    /// column (the parameter keeps what it has).
    pub min: Option<String>,
    pub max: Option<String>,
    pub step: Option<String>,
    /// The `strict` cell; None when the column is missing or the cell is
    /// empty or not a truth value (the parameter keeps what it has).
    pub strict: Option<bool>,
    /// The `values` cell (mitcad#798): a JSON array of expression texts,
    /// empty for none; None when the table has no such column.
    pub values: Option<String>,
}

impl CsvRow {
    /// A text parameter: the unit `Text` or the kind `text`.
    pub fn is_text(&self) -> bool {
        self.unit
            .as_deref()
            .is_some_and(|unit| unit.eq_ignore_ascii_case("text"))
            || self
                .kind
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("text"))
    }
}

/// A parameter CSV read: its format and its rows (empty lines left out).
#[derive(Debug, Clone, PartialEq)]
pub struct CsvTable {
    pub format: CsvFormat,
    pub rows: Vec<CsvRow>,
}

/// Why a text is not a parameter CSV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    /// Nothing but white space.
    Empty,
    /// A quoted field that is not closed, from this line on.
    OpenQuote { line: usize },
    /// The header lacks a column that is needed.
    MissingColumn(&'static str),
}

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("the CSV file is empty"),
            Self::OpenQuote { line } => {
                write!(f, "line {line}: a quoted field is not closed")
            }
            Self::MissingColumn(column) => write!(
                f,
                "not a parameter CSV: the header has no {column} column (Mitcad's table: {}; \
                 the .f3d application's parameter table: {})",
                HEADER.join(","),
                PARAMETER_TABLE_HEADER.join(",")
            ),
        }
    }
}

impl std::error::Error for CsvError {}

/// The records of a CSV text, each with the line it starts on. `separator`
/// is `,` or `;`.
pub(crate) fn records(text: &str, separator: char) -> Result<Vec<(usize, Vec<String>)>, CsvError> {
    let mut out = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut line = 1;
    let mut start = 1;
    let mut chars = text.chars().peekable();
    // Whether the current record has anything yet (an empty line has not).
    let mut started = false;
    while let Some(c) = chars.next() {
        match c {
            '"' if field.trim().is_empty() => {
                // A quoted field (white space before the quote is dropped).
                field.clear();
                let opened = line;
                loop {
                    match chars.next() {
                        None => return Err(CsvError::OpenQuote { line: opened }),
                        Some('"') => {
                            if chars.peek() == Some(&'"') {
                                chars.next();
                                field.push('"');
                            } else {
                                break;
                            }
                        }
                        Some(c) => {
                            if c == '\n' {
                                line += 1;
                            }
                            field.push(c);
                        }
                    }
                }
                // Text after the closing quote is kept (leniently).
                started = true;
            }
            c if c == separator => {
                record.push(std::mem::take(&mut field));
                started = true;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                if started || !field.trim().is_empty() {
                    record.push(std::mem::take(&mut field));
                    out.push((start, std::mem::take(&mut record)));
                }
                field.clear();
                started = false;
                line += 1;
                start = line;
            }
            c => {
                if !started && field.is_empty() {
                    start = line;
                }
                field.push(c);
                if !c.is_whitespace() {
                    started = true;
                }
            }
        }
    }
    if started || !field.trim().is_empty() {
        record.push(field);
        out.push((start, record));
    }
    Ok(out)
}

/// The separator of a CSV text: `;` when its first line that is not empty
/// has semicolons and no comma, else `,`.
pub(crate) fn separator(text: &str) -> char {
    let first_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    if !first_line.contains(',') && first_line.contains(';') {
        ';'
    } else {
        ','
    }
}

/// A truth value of a favourite cell: `true`, `false`, `yes`, `no`, `1`,
/// `0` (any case); None for an empty or other text.
fn truth(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

/// Reads a parameter CSV: Mitcad's table or the `.f3d` application's
/// parameter table, told apart by the header.
pub fn parse(text: &str) -> Result<CsvTable, CsvError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records = records(text, separator(text))?.into_iter();
    let Some((_, header)) = records.next() else {
        return Err(CsvError::Empty);
    };
    let header: Vec<String> = header.iter().map(|h| h.trim().to_lowercase()).collect();
    let format = if header.len() == PARAMETER_TABLE_HEADER.len()
        && header
            .iter()
            .zip(PARAMETER_TABLE_HEADER)
            .all(|(h, p)| h.eq_ignore_ascii_case(p))
    {
        CsvFormat::ParameterTable
    } else {
        CsvFormat::Mitcad
    };
    let column = |names: &[&str]| header.iter().position(|h| names.contains(&h.as_str()));
    let name = column(&["name"]).ok_or(CsvError::MissingColumn("name"))?;
    let expression = column(&["expression"]).ok_or(CsvError::MissingColumn("expression"))?;
    let kind = column(&["kind"]);
    let unit = column(&["unit"]);
    let value = column(&["value"]);
    let comment = column(&["comment", "comments"]);
    let favorite = column(&["favorite", "favourite"]);
    let min = column(&["min"]);
    let max = column(&["max"]);
    let step = column(&["step"]);
    let strict = column(&["strict"]);
    let values = column(&["values"]);
    let mut rows = Vec::new();
    for (line, cells) in records {
        let cell = |i: Option<usize>| i.map(|i| cells.get(i).map_or("", |c| c.as_str()));
        let trimmed = |i: Option<usize>| cell(i).map(|c| c.trim().to_owned());
        let row = CsvRow {
            line,
            name: trimmed(Some(name)).unwrap_or_default(),
            kind: trimmed(kind).filter(|k| !k.is_empty()),
            unit: trimmed(unit),
            expression: trimmed(Some(expression)).unwrap_or_default(),
            value: trimmed(value).filter(|v| !v.is_empty()),
            comment: cell(comment).map(str::to_owned),
            favorite: cell(favorite).and_then(truth),
            min: trimmed(min),
            max: trimmed(max),
            step: trimmed(step),
            strict: cell(strict).and_then(truth),
            values: trimmed(values),
        };
        // A row of empty cells is an empty line.
        if cells.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        rows.push(row);
    }
    Ok(CsvTable { format, rows })
}

/// Appends one record: each field quoted when it has to be (RFC 4180),
/// the record ended with CRLF.
pub fn write_record(out: &mut String, fields: &[&str]) {
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let quoted = field.contains([',', '"', '\r', '\n', ';'])
            || field.starts_with(char::is_whitespace)
            || field.ends_with(char::is_whitespace);
        if quoted {
            out.push('"');
            out.push_str(&field.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(field);
        }
    }
    out.push_str("\r\n");
}

/// The number of a canonical value in `unit`, without the unit: the
/// shortest decimal that gives the value back (`12.5`, `30`).
fn number_in_unit(value: f64, unit: crate::expr::Unit) -> String {
    let text = value_to_expression(value, unit);
    text.split(' ').next().unwrap_or(&text).to_owned()
}

/// Mitcad's table of the parameters, in creation order.
pub fn write(parameters: &Parameters, favorites: &BTreeSet<ParamId>) -> String {
    let mut out = String::new();
    write_record(&mut out, &HEADER);
    for p in parameters.iter() {
        let unit = p.unit().to_string();
        // A text parameter's value is its text (mitcad#664).
        let value = match p.text() {
            Some(text) => text.to_owned(),
            None => number_in_unit(p.value(), p.unit()),
        };
        let flag = |yes: bool| if yes { "true" } else { "false" };
        // The limits as written (mitcad#665, mitcad#798).
        let limits = p.limits();
        let values = limits
            .values
            .as_ref()
            .map(|values| serde_json::to_string(values).expect("texts serialize"))
            .unwrap_or_default();
        write_record(
            &mut out,
            &[
                p.name(),
                p.kind().as_str(),
                unit.as_str(),
                p.expression(),
                value.as_str(),
                p.comment(),
                flag(favorites.contains(&p.id())),
                limits.min.as_deref().unwrap_or(""),
                limits.max.as_deref().unwrap_or(""),
                limits.step.as_deref().unwrap_or(""),
                flag(limits.strict),
                values.as_str(),
            ],
        );
    }
    out
}

/// Identifiers of an expression (names, units, functions), with their byte
/// ranges; enough to rename parameters textually (as the `.f3d` import
/// does). Words in quoted texts (`'Width'`, mitcad#664) are none; a quote
/// right after a number is an inch or foot mark (`3' 6" + d1`, mitcad#1531),
/// as the expression lexer reads it.
pub(crate) fn identifiers(expression: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut chars = expression.char_indices().peekable();
    let mut after_number = false;
    while let Some((start, c)) = chars.next() {
        let number_before = after_number;
        after_number = c.is_whitespace() && number_before;
        if (c == '\'' || c == '"') && number_before {
            // `''` is the inch mark too.
            if c == '\'' && chars.peek().is_some_and(|&(_, after)| after == '\'') {
                chars.next();
            }
        } else if c == '\'' || c == '"' {
            // To the closing quote; a doubled one is a quote inside.
            while let Some((_, next)) = chars.next() {
                if next == c {
                    if chars.peek().is_some_and(|&(_, after)| after == c) {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
        } else if c.is_alphabetic() || c == '_' {
            let mut end = start + c.len_utf8();
            while let Some(&(i, c)) = chars.peek() {
                if c.is_alphanumeric() || c == '_' {
                    end = i + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            // A number's exponent (1e3) is not a name.
            let in_number = expression[..start]
                .chars()
                .next_back()
                .is_some_and(|p| p.is_ascii_digit() || p == '.');
            if !in_number {
                out.push((start, end));
            }
        } else if c.is_ascii_digit() || c == '.' {
            // Skip the rest of the number, including an exponent.
            let mut unit = false;
            while let Some(&(_, c)) = chars.peek() {
                if c.is_ascii_alphanumeric() || c == '.' {
                    unit = unit || (c.is_ascii_alphabetic() && !matches!(c, 'e' | 'E'));
                    chars.next();
                } else {
                    break;
                }
            }
            // A mark may follow the number, not a unit name (`5 mm'`).
            after_number = !unit;
        }
    }
    out
}

/// The expression with parameters renamed (whole names only).
pub(crate) fn rename_in(expression: &str, renamed: &HashMap<String, String>) -> String {
    if renamed.is_empty() {
        return expression.to_owned();
    }
    let mut out = String::new();
    let mut last = 0;
    for (start, end) in identifiers(expression) {
        if let Some(new) = renamed.get(&expression[start..end]) {
            out.push_str(&expression[last..start]);
            out.push_str(new);
            last = end;
        }
    }
    out.push_str(&expression[last..]);
    out
}

/// A free Mitcad name for a name Mitcad cannot use (a unit, a function, a
/// constant, other characters), as the `.f3d` import gives one: other
/// characters become `_`, a name that does not start with a letter gets
/// `p_` before it, and `_1`, `_2`, ... follow until the name is valid and
/// not in `taken`.
pub fn free_name(name: &str, taken: &BTreeSet<String>) -> String {
    let base: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let base = if base.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        base
    } else {
        format!("p_{base}")
    };
    if is_valid_name(&base) && !taken.contains(&base) {
        return base;
    }
    (1u32..)
        .map(|n| format!("{base}_{n}"))
        .find(|c| !taken.contains(c) && !is_reserved_name(c) && is_valid_name(c))
        .expect("a free name exists")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_follows_rfc_4180() {
        let mut out = String::new();
        write_record(
            &mut out,
            &["a", "b, c", "say \"hi\"", "two\nlines", " pad", ""],
        );
        assert_eq!(
            out,
            "a,\"b, c\",\"say \"\"hi\"\"\",\"two\nlines\",\" pad\",\r\n"
        );
        let read = records(&out, ',').unwrap();
        assert_eq!(
            read,
            vec![(
                1,
                vec![
                    "a".to_owned(),
                    "b, c".to_owned(),
                    "say \"hi\"".to_owned(),
                    "two\nlines".to_owned(),
                    " pad".to_owned(),
                    String::new()
                ]
            )]
        );
    }

    #[test]
    fn records_skip_empty_lines_and_count_lines() {
        let text = "h1,h2\n\n\"x\ny\",2\r\nlast,3";
        let read = records(text, ',').unwrap();
        assert_eq!(read.len(), 3);
        assert_eq!(read[1].0, 3);
        assert_eq!(read[1].1, vec!["x\ny".to_owned(), "2".to_owned()]);
        assert_eq!(read[2].0, 5);
        assert_eq!(
            records("a,\"open\n", ','),
            Err(CsvError::OpenQuote { line: 1 })
        );
    }

    #[test]
    fn the_header_tells_the_format() {
        let table = parse(
            "\u{feff}Name,Unit,Expression,Value,Comments,Favorite\r\n\
             Width,mm,30 mm,30.00,\"Outer width, without the flange\",true\r\n\
             Label,Text,'6qt','6qt',,false\r\n",
        )
        .unwrap();
        assert_eq!(table.format, CsvFormat::ParameterTable);
        assert_eq!(table.rows.len(), 2);
        let width = &table.rows[0];
        assert_eq!(width.line, 2);
        assert_eq!(width.name, "Width");
        assert_eq!(width.unit.as_deref(), Some("mm"));
        assert_eq!(width.value.as_deref(), Some("30.00"));
        assert_eq!(
            width.comment.as_deref(),
            Some("Outer width, without the flange")
        );
        assert_eq!(width.favorite, Some(true));
        assert!(!width.is_text());
        assert!(table.rows[1].is_text());

        let table = parse(
            "name,kind,unit,expression,value,comment,favorite,min,max,step\n\
                           a,user,,4,4,,false,1,,\n",
        )
        .unwrap();
        assert_eq!(table.format, CsvFormat::Mitcad);
        assert_eq!(table.rows[0].unit.as_deref(), Some(""));
        // Limits: an empty cell is no limit, a missing column keeps them.
        assert_eq!(table.rows[0].min.as_deref(), Some("1"));
        assert_eq!(table.rows[0].max.as_deref(), Some(""));
        assert_eq!(table.rows[0].strict, None);
        // Semicolons between fields.
        let table = parse("Name;Expression\nw;\"max(1, 2)\"\n").unwrap();
        assert_eq!(table.rows[0].expression, "max(1, 2)");
        assert_eq!(table.rows[0].unit, None);
        assert_eq!(
            parse("Name,Value\nw,3\n").unwrap_err(),
            CsvError::MissingColumn("expression")
        );
        assert_eq!(parse(" \n").unwrap_err(), CsvError::Empty);
    }

    #[test]
    fn free_names_and_renames_follow_the_f3d_import() {
        let renamed = HashMap::from([("mil".to_owned(), "mil_1".to_owned())]);
        assert_eq!(rename_in("mil * 2 + mils", &renamed), "mil_1 * 2 + mils");
        // Not inside texts (mitcad#664).
        assert_eq!(
            rename_in("'mil ''mil''' & mil & \"mil\"", &renamed),
            "'mil ''mil''' & mil_1 & \"mil\""
        );
        // Inch and foot marks after numbers start no text (mitcad#1531).
        assert_eq!(
            rename_in("3' 6\" + mil * 2'' + 1e1 \" & 'mil'", &renamed),
            "3' 6\" + mil_1 * 2'' + 1e1 \" & 'mil'"
        );
        assert_eq!(free_name("mil", &BTreeSet::new()), "mil_1");
        assert_eq!(free_name("a b", &BTreeSet::new()), "a_b");
        assert_eq!(free_name("2nd", &BTreeSet::new()), "p_2nd");
    }
}
