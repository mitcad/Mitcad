// SPDX-License-Identifier: MIT
//! A structure check of DXF R2000 text for tests (mitcad#2315).
//!
//! Its problems are diagnostics for tests, not messages a user sees; the
//! file's name keeps them out of the translated model messages.
//!
//! MitCad's reader takes nearly any group order, so a round trip through it
//! cannot tell whether a stricter reader takes a written file. This check
//! reads the text on its own, without the reader or the writer, and holds
//! it to the R2000 (`AC1015`) DXF reference:
//!
//! - the sections in their order (HEADER, CLASSES, TABLES, BLOCKS,
//!   ENTITIES, OBJECTS) and `EOF` last; the nine tables in their order,
//!   each with its required entries;
//! - each record's groups in the reference's order, subclass markers
//!   included, for every record kind MitCad writes (a kind the check does
//!   not know is refused, so a new kind in the writer comes with its rules
//!   here);
//! - group values of their type (16-bit and 32-bit integers, finite reals,
//!   hexadecimal handles), ASCII text, arc angles in [0, 360);
//! - unique handles below `$HANDSEED`, owners and pointers that name an
//!   existing record of the right kind, layers and line types that exist,
//!   dictionary entries owned by their dictionary, a class for each object
//!   of a class.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

/// The problems of `text` as DXF R2000, each with its line; empty when the
/// text passes.
pub fn check_r2000(text: &str) -> Vec<String> {
    let mut check = Check::default();
    if !text.is_ascii() {
        check.problem(1, "the text is not ASCII".into());
    }
    let pairs = match parse(text) {
        Ok(pairs) => pairs,
        Err(problem) => {
            check.problems.push(problem);
            return check.problems;
        }
    };
    for pair in &pairs {
        if let Some(problem) = value_problem(pair) {
            check.problem(pair.line, problem);
        }
    }
    let records = match split(pairs) {
        Ok(records) => records,
        Err(problem) => {
            check.problems.push(problem);
            return check.problems;
        }
    };
    check.document(&records);
    check.references();
    check.problems
}

#[derive(Debug, Clone)]
struct Pair {
    code: i32,
    value: String,
    /// Line of the group code, from 1.
    line: usize,
}

/// A record: the groups after its `0` group.
#[derive(Debug)]
struct Record {
    kind: String,
    line: usize,
    pairs: Vec<Pair>,
}

impl Record {
    fn first(&self, code: i32) -> Option<&Pair> {
        self.pairs.iter().find(|p| p.code == code)
    }

    fn value(&self, code: i32) -> Option<&str> {
        self.first(code).map(|p| p.value.as_str())
    }

    fn count(&self, code: i32) -> usize {
        self.pairs.iter().filter(|p| p.code == code).count()
    }

    /// The owner: the 330 group outside the `102` groups.
    fn owner(&self) -> Option<&Pair> {
        let mut inside = false;
        for pair in &self.pairs {
            match pair.code {
                102 => inside = pair.value.starts_with('{'),
                330 if !inside => return Some(pair),
                _ => {}
            }
        }
        None
    }

    /// The record's handle: group 105 in a dimension style, 5 elsewhere.
    fn handle(&self) -> Option<u64> {
        let code = if self.kind == "DIMSTYLE" { 105 } else { 5 };
        self.first(code).and_then(|p| hex(&p.value))
    }
}

fn hex(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(value, 16).ok()
}

fn parse(text: &str) -> Result<Vec<Pair>, String> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    if !lines.len().is_multiple_of(2) {
        return Err(format!(
            "line {}: a group code without its value",
            lines.len()
        ));
    }
    let mut pairs = Vec::with_capacity(lines.len() / 2);
    for (index, pair) in lines.chunks(2).enumerate() {
        let line = 2 * index + 1;
        let code = pair[0].trim_end_matches('\r').trim();
        let Ok(code) = code.parse::<i32>() else {
            return Err(format!("line {line}: group code {code:?} is not a number"));
        };
        pairs.push(Pair {
            code,
            value: pair[1].trim_end_matches('\r').to_owned(),
            line,
        });
    }
    Ok(pairs)
}

fn split(pairs: Vec<Pair>) -> Result<Vec<Record>, String> {
    let mut records: Vec<Record> = Vec::new();
    for pair in pairs {
        if pair.code == 0 {
            records.push(Record {
                kind: pair.value.trim().to_owned(),
                line: pair.line,
                pairs: Vec::new(),
            });
        } else if let Some(record) = records.last_mut() {
            record.pairs.push(pair);
        } else {
            return Err(format!(
                "line {}: group {} before the first record",
                pair.line, pair.code
            ));
        }
    }
    Ok(records)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ValueType {
    Text,
    Real,
    Int16,
    Int32,
    Bool,
    Handle,
}

/// The value type of a group code, from the reference's group code ranges.
fn value_type(code: i32) -> ValueType {
    use ValueType::*;
    match code {
        5 | 105 | 320..=369 | 390..=399 | 480..=481 | 1005 => Handle,
        10..=59 | 110..=149 | 210..=239 | 460..=469 | 1010..=1059 => Real,
        60..=79 | 170..=179 | 270..=289 | 370..=389 | 400..=409 | 1060..=1070 => Int16,
        90..=99 | 420..=429 | 440..=449 | 1071 => Int32,
        290..=299 => Bool,
        _ => Text,
    }
}

fn value_problem(pair: &Pair) -> Option<String> {
    let value = pair.value.trim();
    let code = pair.code;
    let integer = |min: i64, max: i64| match value.parse::<i64>() {
        Ok(v) if (min..=max).contains(&v) => None,
        _ => Some(format!(
            "group {code} needs an integer in [{min}, {max}], not {value:?}"
        )),
    };
    match value_type(code) {
        ValueType::Text => None,
        ValueType::Real => match value.parse::<f64>() {
            Ok(v) if v.is_finite() => None,
            _ => Some(format!("group {code} needs a finite real, not {value:?}")),
        },
        ValueType::Int16 => integer(i64::from(i16::MIN), i64::from(i16::MAX)),
        ValueType::Int32 => integer(i64::from(i32::MIN), i64::from(i32::MAX)),
        ValueType::Bool => integer(0, 1),
        ValueType::Handle => match hex(value) {
            Some(_) => None,
            None => Some(format!(
                "group {code} needs a hexadecimal handle, not {value:?}"
            )),
        },
    }
}

/// One rule of a record's group sequence.
#[derive(Debug, Clone)]
enum G {
    /// A group that must come once.
    One(i32),
    /// A group that may come once.
    Opt(i32),
    /// A group with a fixed value (a subclass marker) that must come.
    Is(i32, &'static str),
    /// A sequence that may come any number of times; it starts with the
    /// group of its first rule.
    Many(Vec<G>),
    /// A sequence that may come once; it starts with the group of its
    /// first rule.
    Maybe(Vec<G>),
}

use G::{Is, Many, Maybe, One, Opt};

fn starts(rules: &[G], pair: &Pair) -> bool {
    match rules.first() {
        Some(One(code) | Opt(code)) => pair.code == *code,
        Some(Is(code, value)) => pair.code == *code && pair.value == *value,
        _ => false,
    }
}

/// Matches `pairs[*at..]` with `rules`; the first group out of sequence is
/// the error.
fn walk(rules: &[G], pairs: &[Pair], at: &mut usize) -> Result<(), String> {
    for rule in rules {
        let pair = pairs.get(*at);
        match rule {
            One(code) => match pair {
                Some(p) if p.code == *code => *at += 1,
                Some(p) => {
                    return Err(format!(
                        "expected group {code}, found group {} at line {}",
                        p.code, p.line
                    ));
                }
                None => return Err(format!("group {code} is missing")),
            },
            Opt(code) => {
                if pair.is_some_and(|p| p.code == *code) {
                    *at += 1;
                }
            }
            Is(code, value) => match pair {
                Some(p) if p.code == *code && p.value == *value => *at += 1,
                Some(p) => {
                    return Err(format!(
                        "expected group {code} {value:?}, found group {} {:?} at line {}",
                        p.code, p.value, p.line
                    ));
                }
                None => return Err(format!("group {code} {value:?} is missing")),
            },
            Many(rules) => {
                while pairs.get(*at).is_some_and(|p| starts(rules, p)) {
                    walk(rules, pairs, at)?;
                }
            }
            Maybe(rules) => {
                if pairs.get(*at).is_some_and(|p| starts(rules, p)) {
                    walk(rules, pairs, at)?;
                }
            }
        }
    }
    Ok(())
}

fn point(code: i32) -> Vec<G> {
    vec![One(code), One(code + 10), One(code + 20)]
}

fn normal() -> G {
    Maybe(vec![One(210), One(220), One(230)])
}

/// Handle, the optional reactor and extension dictionary groups, owner.
fn head(handle: i32) -> Vec<G> {
    vec![
        One(handle),
        Maybe(vec![
            Is(102, "{ACAD_REACTORS"),
            Many(vec![One(330)]),
            Is(102, "}"),
        ]),
        Maybe(vec![Is(102, "{ACAD_XDICTIONARY"), One(360), Is(102, "}")]),
        One(330),
    ]
}

fn table_record(handle: i32, subclass: &'static str, fields: Vec<G>) -> Vec<G> {
    let mut rules = head(handle);
    rules.push(Is(100, "AcDbSymbolTableRecord"));
    rules.push(Is(100, subclass));
    rules.extend(fields);
    rules
}

/// The common groups of an entity (`AcDbEntity`) before its own subclass.
fn entity_head() -> Vec<G> {
    let mut rules = head(5);
    rules.extend([
        Is(100, "AcDbEntity"),
        Opt(67),
        Opt(410),
        One(8),
        Opt(6),
        Opt(62),
        Opt(370),
        Opt(48),
        Opt(60),
    ]);
    rules
}

fn entity(subclass: &'static str, fields: Vec<G>) -> Vec<G> {
    let mut rules = entity_head();
    rules.push(Is(100, subclass));
    rules.extend(fields);
    rules
}

/// The R2000 rules of a table head.
fn table_rules(name: &str) -> Vec<G> {
    let mut rules = vec![
        One(2),
        One(5),
        Maybe(vec![Is(102, "{ACAD_XDICTIONARY"), One(360), Is(102, "}")]),
        One(330),
        Is(100, "AcDbSymbolTable"),
        One(70),
    ];
    if name == "DIMSTYLE" {
        rules.extend([Is(100, "AcDbDimStyleTable"), One(71), Many(vec![One(340)])]);
    }
    rules
}

/// The R2000 rules of a table entry.
fn entry_rules(kind: &str) -> Option<Vec<G>> {
    Some(match kind {
        "LTYPE" => table_record(
            5,
            "AcDbLinetypeTableRecord",
            vec![
                One(2),
                One(70),
                One(3),
                One(72),
                One(73),
                One(40),
                Many(vec![
                    One(49),
                    Opt(74),
                    Opt(75),
                    Opt(340),
                    Opt(46),
                    Opt(50),
                    Opt(44),
                    Opt(45),
                    Opt(9),
                ]),
            ],
        ),
        "LAYER" => table_record(
            5,
            "AcDbLayerTableRecord",
            vec![
                One(2),
                One(70),
                One(62),
                One(6),
                Opt(290),
                One(370),
                One(390),
            ],
        ),
        "STYLE" => table_record(
            5,
            "AcDbTextStyleTableRecord",
            vec![
                One(2),
                One(70),
                One(40),
                One(41),
                One(50),
                One(71),
                One(42),
                One(3),
                One(4),
            ],
        ),
        "APPID" => table_record(5, "AcDbRegAppTableRecord", vec![One(2), One(70)]),
        "DIMSTYLE" => {
            let mut fields = vec![One(2), One(70)];
            // The dimension variables that differ from their defaults, in
            // the reference's order; DIMTXSTY (340) names the text style.
            let variables = [3, 4]
                .into_iter()
                .chain(40..=48)
                .chain(140..=148)
                .chain(71..=79)
                .chain(170..=179)
                .chain(271..=279)
                .chain(280..=289);
            fields.extend(variables.map(Opt));
            fields.extend([One(340), Opt(341), Opt(342), Opt(343), Opt(344)]);
            fields.extend([Opt(371), Opt(372)]);
            table_record(105, "AcDbDimStyleTableRecord", fields)
        }
        "BLOCK_RECORD" => table_record(5, "AcDbBlockTableRecord", vec![One(2), Opt(340)]),
        _ => return None,
    })
}

/// The R2000 rules of an entity MitCad writes.
fn entity_rules(kind: &str) -> Option<Vec<G>> {
    let rules = match kind {
        "POINT" => {
            let mut fields = point(10);
            fields.extend([Opt(39), normal(), Opt(50)]);
            entity("AcDbPoint", fields)
        }
        "LINE" => {
            let mut fields = vec![Opt(39)];
            fields.extend(point(10));
            fields.extend(point(11));
            fields.push(normal());
            entity("AcDbLine", fields)
        }
        "CIRCLE" | "ARC" => {
            let mut fields = vec![Opt(39)];
            fields.extend(point(10));
            fields.extend([One(40), normal()]);
            if kind == "ARC" {
                fields.extend([Is(100, "AcDbArc"), One(50), One(51)]);
            }
            entity("AcDbCircle", fields)
        }
        "ELLIPSE" => {
            let mut fields = point(10);
            fields.extend(point(11));
            fields.extend([normal(), One(40), One(41), One(42)]);
            entity("AcDbEllipse", fields)
        }
        "SPLINE" => entity(
            "AcDbSpline",
            vec![
                normal(),
                One(70),
                One(71),
                One(72),
                One(73),
                One(74),
                Opt(42),
                Opt(43),
                Opt(44),
                Maybe(point(12)),
                Maybe(point(13)),
                Many(vec![One(40)]),
                Many(vec![One(41)]),
                Many(point(10)),
                Many(point(11)),
            ],
        ),
        "TEXT" => {
            let mut fields = vec![Opt(39)];
            fields.extend(point(10));
            fields.extend([
                One(40),
                One(1),
                Opt(50),
                Opt(41),
                Opt(51),
                Opt(7),
                Opt(71),
                Opt(72),
                Maybe(point(11)),
                normal(),
                Is(100, "AcDbText"),
                Opt(73),
            ]);
            entity("AcDbText", fields)
        }
        "BLOCK" => {
            let mut rules = entity_head();
            rules.extend([Is(100, "AcDbBlockBegin"), One(2), One(70)]);
            rules.extend(point(10));
            rules.extend([One(3), One(1)]);
            rules
        }
        "ENDBLK" => {
            let mut rules = entity_head();
            rules.push(Is(100, "AcDbBlockEnd"));
            rules
        }
        _ => return None,
    };
    Some(rules)
}

/// The R2000 rules of an object MitCad writes.
fn object_rules(kind: &str) -> Option<Vec<G>> {
    let entries = || {
        vec![
            Is(100, "AcDbDictionary"),
            Opt(280),
            Opt(281),
            Many(vec![One(3), One(350)]),
        ]
    };
    let mut rules = head(5);
    match kind {
        "DICTIONARY" => rules.extend(entries()),
        "ACDBDICTIONARYWDFLT" => {
            rules.extend(entries());
            rules.extend([Is(100, "AcDbDictionaryWithDefault"), One(340)]);
        }
        "ACDBPLACEHOLDER" => {}
        _ => return None,
    }
    Some(rules)
}

/// Objects that need a CLASS of their own in an R2000 file.
const CLASS_OBJECTS: [&str; 2] = ["ACDBDICTIONARYWDFLT", "ACDBPLACEHOLDER"];

const SECTIONS: [&str; 6] = [
    "HEADER", "CLASSES", "TABLES", "BLOCKS", "ENTITIES", "OBJECTS",
];

/// The tables in the reference's order with the entries each needs.
const TABLES: [(&str, &[&str]); 9] = [
    ("VPORT", &[]),
    ("LTYPE", &["ByBlock", "ByLayer", "Continuous"]),
    ("LAYER", &["0"]),
    ("STYLE", &["Standard"]),
    ("VIEW", &[]),
    ("UCS", &[]),
    ("APPID", &["ACAD"]),
    ("DIMSTYLE", &["Standard"]),
    ("BLOCK_RECORD", &["*Model_Space", "*Paper_Space"]),
];

/// The header variables MitCad writes, with the group of each one's value.
const HEADER: [(&str, i32); 5] = [
    ("$ACADVER", 1),
    ("$DWGCODEPAGE", 3),
    ("$HANDSEED", 5),
    ("$INSUNITS", 70),
    ("$MEASUREMENT", 70),
];

/// A record with a handle, for the reference checks at the end.
#[derive(Debug)]
struct Known {
    kind: String,
    line: usize,
    owner: Option<u64>,
}

/// A pointer to check once every record is known.
#[derive(Debug)]
struct Pointer {
    line: usize,
    /// The kind of the record it is in.
    kind: String,
    code: i32,
    target: u64,
    /// The kind its target must be, if any.
    wanted: Option<&'static str>,
}

#[derive(Debug, Default)]
struct Check {
    problems: Vec<String>,
    handseed: Option<u64>,
    known: HashMap<u64, Known>,
    pointers: Vec<Pointer>,
    /// Upper-case entry names of each table.
    names: HashMap<String, HashSet<String>>,
    /// Block records by upper-case name.
    block_records: HashMap<String, u64>,
    classes: HashSet<String>,
    /// Dictionary entries to check: line, dictionary, target.
    entries: Vec<(usize, u64, u64)>,
}

impl Check {
    fn problem(&mut self, line: usize, problem: String) {
        self.problems.push(format!("line {line}: {problem}"));
    }

    /// Checks `record` with `rules`: the group order, then the handle,
    /// owner and pointers.
    fn record(&mut self, record: &Record, rules: &[G]) -> bool {
        let mut at = 0;
        let order = walk(rules, &record.pairs, &mut at).and_then(|()| match record.pairs.get(at) {
            Some(p) => Err(format!(
                "group {} at line {} is out of the reference's sequence",
                p.code, p.line
            )),
            None => Ok(()),
        });
        if let Err(problem) = order {
            let kind = record.kind.clone();
            self.problem(record.line, format!("{kind}: {problem}"));
            return false;
        }
        if let Some(handle) = record.handle() {
            let owner = record.owner().and_then(|p| hex(&p.value));
            let problem = if handle == 0 {
                Some(format!("{}: handle 0", record.kind))
            } else if let Entry::Vacant(entry) = self.known.entry(handle) {
                entry.insert(Known {
                    kind: record.kind.clone(),
                    line: record.line,
                    owner,
                });
                None
            } else {
                Some(format!("{}: handle {handle:X} is used twice", record.kind))
            };
            if let Some(problem) = problem {
                self.problem(record.line, problem);
            }
        }
        for pair in &record.pairs {
            let pointer =
                matches!(value_type(pair.code), ValueType::Handle) && !matches!(pair.code, 5 | 105);
            if !pointer {
                continue;
            }
            let Some(target) = hex(&pair.value) else {
                continue;
            };
            // An owner of 0 is checked where it is allowed.
            if pair.code == 330 && target == 0 {
                continue;
            }
            let wanted = match (record.kind.as_str(), pair.code) {
                ("LAYER", 390) => Some("ACDBPLACEHOLDER"),
                ("DIMSTYLE", 340) => Some("STYLE"),
                ("BLOCK_RECORD", 340) => Some("LAYOUT"),
                _ => None,
            };
            self.pointers.push(Pointer {
                line: pair.line,
                kind: record.kind.clone(),
                code: pair.code,
                target,
                wanted,
            });
        }
        true
    }

    /// The owner group of `record` must be `owner` (0 for none).
    fn owned_by(&mut self, record: &Record, owner: u64, what: &str) {
        let found = record.owner().and_then(|p| hex(&p.value));
        if found != Some(owner) {
            let found = found.map_or("none".to_owned(), |h| format!("{h:X}"));
            self.problem(
                record.line,
                format!(
                    "{}: owner {found}, the reference needs {what} ({owner:X})",
                    record.kind
                ),
            );
        }
    }

    fn document(&mut self, records: &[Record]) {
        let mut i = 0;
        for name in SECTIONS {
            let Some(section) = records.get(i) else {
                self.problem(0, format!("section {name} is missing"));
                return;
            };
            let named = section
                .pairs
                .first()
                .is_some_and(|p| p.code == 2 && p.value == name);
            if section.kind != "SECTION" || !named {
                self.problem(
                    section.line,
                    format!("expected section {name}, found {}", section.kind),
                );
                return;
            }
            if name == "HEADER" {
                self.header(&section.pairs[1..], section.line);
            } else if let Some(p) = section.pairs.get(1) {
                self.problem(p.line, format!("group {} after the section's name", p.code));
            }
            i += 1;
            let start = i;
            while i < records.len() && records[i].kind != "ENDSEC" {
                i += 1;
            }
            if i == records.len() {
                self.problem(section.line, format!("section {name} has no ENDSEC"));
                return;
            }
            if let Some(p) = records[i].pairs.first() {
                self.problem(p.line, format!("group {} in ENDSEC", p.code));
            }
            let body = &records[start..i];
            i += 1;
            match name {
                "CLASSES" => self.classes(body),
                "TABLES" => self.tables(body),
                "BLOCKS" => self.blocks(body),
                "ENTITIES" => self.entities(body, None),
                "OBJECTS" => self.objects(body),
                _ => {}
            }
        }
        match records.get(i) {
            Some(r) if r.kind == "EOF" && r.pairs.is_empty() => {
                if let Some(extra) = records.get(i + 1) {
                    self.problem(extra.line, "a record after EOF".into());
                }
            }
            Some(r) => self.problem(r.line, format!("expected EOF, found {}", r.kind)),
            None => self.problem(0, "EOF is missing".into()),
        }
    }

    fn header(&mut self, pairs: &[Pair], line: usize) {
        let mut seen: HashSet<String> = HashSet::new();
        let mut i = 0;
        while i < pairs.len() {
            let name = &pairs[i];
            if name.code != 9 {
                self.problem(
                    name.line,
                    format!("group {} where a variable name belongs", name.code),
                );
                return;
            }
            if i == 0 && name.value != "$ACADVER" {
                self.problem(name.line, "the header does not start with $ACADVER".into());
            }
            if !seen.insert(name.value.clone()) {
                self.problem(name.line, format!("{} twice", name.value));
            }
            let Some(&(_, code)) = HEADER.iter().find(|(n, _)| *n == name.value) else {
                self.problem(
                    name.line,
                    format!("header variable {} is not known to the check", name.value),
                );
                return;
            };
            let Some(value) = pairs.get(i + 1).filter(|p| p.code == code) else {
                self.problem(name.line, format!("{} needs one group {code}", name.value));
                return;
            };
            match name.value.as_str() {
                "$ACADVER" if value.value != "AC1015" => {
                    self.problem(
                        value.line,
                        format!("$ACADVER {} is not R2000 (AC1015)", value.value),
                    );
                }
                "$HANDSEED" => self.handseed = hex(&value.value),
                _ => {}
            }
            i += 2;
        }
        for required in ["$ACADVER", "$HANDSEED"] {
            if !seen.contains(required) {
                self.problem(line, format!("the header has no {required}"));
            }
        }
    }

    fn classes(&mut self, body: &[Record]) {
        let rules = [One(1), One(2), One(3), One(90), One(280), One(281)];
        for record in body {
            if record.kind != "CLASS" {
                self.problem(
                    record.line,
                    format!("{} in the CLASSES section", record.kind),
                );
                continue;
            }
            if self.record(record, &rules) {
                let name = record.value(1).unwrap_or_default().to_owned();
                if !self.classes.insert(name.clone()) {
                    self.problem(record.line, format!("class {name} twice"));
                }
            }
        }
    }

    fn tables(&mut self, body: &[Record]) {
        let mut i = 0;
        for (name, required) in TABLES {
            let Some(head) = body.get(i) else {
                self.problem(0, format!("table {name} is missing"));
                return;
            };
            if head.kind != "TABLE" || head.value(2) != Some(name) {
                self.problem(
                    head.line,
                    format!(
                        "expected table {name}, found {} {}",
                        head.kind,
                        head.value(2).unwrap_or("")
                    ),
                );
                return;
            }
            self.record(head, &table_rules(name));
            self.owned_by(head, 0, "no owner");
            let table = head.handle().unwrap_or(0);
            i += 1;
            let mut count = 0;
            let mut names: HashSet<String> = HashSet::new();
            while i < body.len() && body[i].kind != "ENDTAB" {
                let record = &body[i];
                i += 1;
                count += 1;
                if record.kind != name {
                    self.problem(record.line, format!("{} in table {name}", record.kind));
                    continue;
                }
                let Some(rules) = entry_rules(name) else {
                    self.problem(
                        record.line,
                        format!("{name} entries are not known to the check"),
                    );
                    continue;
                };
                if !self.record(record, &rules) {
                    continue;
                }
                self.owned_by(record, table, "its table");
                let entry = record.value(2).unwrap_or_default().to_ascii_uppercase();
                if !names.insert(entry.clone()) {
                    self.problem(record.line, format!("{name} {entry} twice"));
                }
                if name == "BLOCK_RECORD" {
                    self.block_records
                        .insert(entry, record.handle().unwrap_or(0));
                }
                if name == "LTYPE" {
                    let elements = record.count(49);
                    let declared = record
                        .value(73)
                        .and_then(|v| v.trim().parse::<usize>().ok());
                    if declared != Some(elements) {
                        self.problem(
                            record.line,
                            format!("LTYPE: group 73 does not count its {elements} dashes"),
                        );
                    }
                }
            }
            if i == body.len() {
                self.problem(head.line, format!("table {name} has no ENDTAB"));
                return;
            }
            if let Some(p) = body[i].pairs.first() {
                self.problem(p.line, format!("group {} in ENDTAB", p.code));
            }
            i += 1;
            let declared = head.value(70).and_then(|v| v.trim().parse::<usize>().ok());
            if declared != Some(count) {
                self.problem(
                    head.line,
                    format!("table {name}: group 70 does not count its {count} entries"),
                );
            }
            for entry in required {
                if !names.contains(&entry.to_ascii_uppercase()) {
                    self.problem(head.line, format!("table {name} has no entry {entry}"));
                }
            }
            self.names.insert(name.to_owned(), names);
        }
        if let Some(extra) = body.get(i) {
            self.problem(extra.line, format!("{} after the nine tables", extra.kind));
        }
        // Layers name line types that exist.
        let linetypes = self.names.get("LTYPE").cloned().unwrap_or_default();
        for record in body.iter().filter(|r| r.kind == "LAYER") {
            if let Some(linetype) = record.value(6)
                && !linetypes.contains(&linetype.to_ascii_uppercase())
            {
                self.problem(
                    record.line,
                    format!("LAYER: line type {linetype} is not in the LTYPE table"),
                );
            }
        }
    }

    fn blocks(&mut self, body: &[Record]) {
        let mut i = 0;
        let mut defined: HashSet<String> = HashSet::new();
        while i < body.len() {
            let begin = &body[i];
            i += 1;
            if begin.kind != "BLOCK" {
                self.problem(begin.line, format!("{} where a BLOCK belongs", begin.kind));
                continue;
            }
            let rules = entity_rules("BLOCK").unwrap_or_default();
            self.record(begin, &rules);
            let name = begin.value(2).unwrap_or_default().to_ascii_uppercase();
            let record = self.block_records.get(&name).copied();
            match record {
                Some(handle) => self.owned_by(begin, handle, "its block record"),
                None => self.problem(begin.line, format!("BLOCK {name} has no block record")),
            }
            if begin.value(3).map(str::to_ascii_uppercase) != Some(name.clone()) {
                self.problem(begin.line, "BLOCK: group 3 is not the block's name".into());
            }
            defined.insert(name);
            let start = i;
            while i < body.len() && !matches!(body[i].kind.as_str(), "ENDBLK" | "BLOCK") {
                i += 1;
            }
            self.entities(&body[start..i], record);
            match body.get(i) {
                Some(end) if end.kind == "ENDBLK" => {
                    let rules = entity_rules("ENDBLK").unwrap_or_default();
                    self.record(end, &rules);
                    if let Some(handle) = record {
                        self.owned_by(end, handle, "its block record");
                    }
                    i += 1;
                }
                _ => self.problem(begin.line, "BLOCK without ENDBLK".into()),
            }
        }
        let missing: Vec<String> = self
            .block_records
            .keys()
            .filter(|name| !defined.contains(*name))
            .cloned()
            .collect();
        for name in missing {
            self.problem(0, format!("block record {name} has no BLOCK"));
        }
    }

    /// Entities of the ENTITIES section (`block` None) or of a block.
    fn entities(&mut self, body: &[Record], block: Option<u64>) {
        let layers = self.names.get("LAYER").cloned().unwrap_or_default();
        for record in body {
            let Some(rules) = entity_rules(&record.kind)
                .filter(|_| record.kind != "BLOCK" && record.kind != "ENDBLK")
            else {
                self.problem(
                    record.line,
                    format!("entity {} is not known to the check", record.kind),
                );
                continue;
            };
            if !self.record(record, &rules) {
                continue;
            }
            let owner = match block {
                Some(handle) => Some((handle, "its block record")),
                None => {
                    let paper = record.value(67).is_some_and(|v| v.trim() == "1");
                    let name = if paper {
                        "*PAPER_SPACE"
                    } else {
                        "*MODEL_SPACE"
                    };
                    self.block_records
                        .get(name)
                        .map(|&h| (h, "its layout's block record"))
                }
            };
            if let Some((handle, what)) = owner {
                self.owned_by(record, handle, what);
            }
            if let Some(layer) = record.value(8)
                && !layers.contains(&layer.to_ascii_uppercase())
            {
                self.problem(
                    record.line,
                    format!("{}: layer {layer} is not in the LAYER table", record.kind),
                );
            }
            self.entity_values(record);
        }
    }

    fn entity_values(&mut self, record: &Record) {
        let real = |code: i32| {
            record
                .value(code)
                .and_then(|v| v.trim().parse::<f64>().ok())
        };
        let count = |code: i32| {
            record
                .value(code)
                .and_then(|v| v.trim().parse::<usize>().ok())
        };
        match record.kind.as_str() {
            "CIRCLE" | "ARC" => {
                if real(40).is_none_or(|r| r <= 0.0) {
                    self.problem(
                        record.line,
                        format!("{}: the radius is not positive", record.kind),
                    );
                }
                if record.kind == "ARC" {
                    for code in [50, 51] {
                        if real(code).is_none_or(|a| !(0.0..360.0).contains(&a)) {
                            self.problem(
                                record.line,
                                format!("ARC: group {code} is not an angle in [0, 360)"),
                            );
                        }
                    }
                }
            }
            "SPLINE" => {
                let knots = record.count(40);
                let controls = record.count(10);
                let fits = record.count(11);
                let weights = record.count(41);
                if count(72) != Some(knots)
                    || count(73) != Some(controls)
                    || count(74) != Some(fits)
                {
                    self.problem(
                        record.line,
                        "SPLINE: groups 72 to 74 do not count its knots and points".into(),
                    );
                }
                if weights != 0 && weights != controls {
                    self.problem(
                        record.line,
                        "SPLINE: a weight for some control points only".into(),
                    );
                }
            }
            _ => {}
        }
    }

    fn objects(&mut self, body: &[Record]) {
        for (index, record) in body.iter().enumerate() {
            let Some(rules) = object_rules(&record.kind) else {
                self.problem(
                    record.line,
                    format!("object {} is not known to the check", record.kind),
                );
                continue;
            };
            if !self.record(record, &rules) {
                continue;
            }
            if CLASS_OBJECTS.contains(&record.kind.as_str()) && !self.classes.contains(&record.kind)
            {
                self.problem(record.line, format!("{} has no CLASS", record.kind));
            }
            if index == 0 {
                if record.kind != "DICTIONARY" {
                    self.problem(
                        record.line,
                        "the first object is not the root DICTIONARY".into(),
                    );
                }
                self.owned_by(record, 0, "no owner");
                if !record
                    .pairs
                    .iter()
                    .any(|p| p.code == 3 && p.value == "ACAD_GROUP")
                {
                    self.problem(record.line, "the root dictionary has no ACAD_GROUP".into());
                }
            } else if record
                .owner()
                .and_then(|p| hex(&p.value))
                .is_none_or(|h| h == 0)
            {
                self.problem(record.line, format!("{} has no owner", record.kind));
            }
            let handle = record.handle().unwrap_or(0);
            let mut names: HashSet<&str> = HashSet::new();
            let mut targets: Vec<u64> = Vec::new();
            for pair in record.pairs.windows(2) {
                if pair[0].code == 3 && pair[1].code == 350 {
                    if !names.insert(pair[0].value.as_str()) {
                        self.problem(
                            pair[0].line,
                            format!("dictionary entry {} twice", pair[0].value),
                        );
                    }
                    if let Some(target) = hex(&pair[1].value) {
                        targets.push(target);
                        self.entries.push((pair[1].line, handle, target));
                    }
                }
            }
            if record.kind == "ACDBDICTIONARYWDFLT" {
                let default = record
                    .pairs
                    .iter()
                    .rev()
                    .find(|p| p.code == 340)
                    .and_then(|p| hex(&p.value));
                if default.is_none_or(|d| !targets.contains(&d)) {
                    self.problem(
                        record.line,
                        "ACDBDICTIONARYWDFLT: the default is not one of its entries".into(),
                    );
                }
            }
        }
    }

    /// Handles below `$HANDSEED`, pointers that resolve, dictionary
    /// entries owned by their dictionary.
    fn references(&mut self) {
        if let Some(seed) = self.handseed {
            let mut over: Vec<(usize, u64)> = self
                .known
                .iter()
                .filter(|(h, _)| **h >= seed)
                .map(|(h, k)| (k.line, *h))
                .collect();
            over.sort_unstable();
            for (line, handle) in over {
                self.problem(
                    line,
                    format!("handle {handle:X} is not below $HANDSEED {seed:X}"),
                );
            }
        }
        let pointers = std::mem::take(&mut self.pointers);
        for pointer in pointers {
            let Pointer {
                line,
                kind,
                code,
                target,
                wanted,
            } = pointer;
            let problem = match self.known.get(&target) {
                None => Some(format!(
                    "{kind}: group {code} names handle {target:X}, which no record has"
                )),
                Some(found) => wanted.filter(|wanted| found.kind != *wanted).map(|wanted| {
                    format!(
                        "{kind}: group {code} names a {}, not a {wanted}",
                        found.kind
                    )
                }),
            };
            if let Some(problem) = problem {
                self.problem(line, problem);
            }
        }
        let entries = std::mem::take(&mut self.entries);
        for (line, dictionary, target) in entries {
            let problem = self
                .known
                .get(&target)
                .filter(|found| found.owner != Some(dictionary))
                .map(|found| {
                    format!(
                        "dictionary entry {target:X} ({}, line {}) is not owned by its dictionary",
                        found.kind, found.line
                    )
                });
            if let Some(problem) = problem {
                self.problem(line, problem);
            }
        }
    }
}
