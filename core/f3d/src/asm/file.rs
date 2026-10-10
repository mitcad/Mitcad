// SPDX-License-Identifier: MIT
//! ASM binary file: header, records and the subtype object table.

use std::collections::BTreeMap;
use std::fmt;

use super::token::{Token, TokenError, Tokenizer};

/// File header.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    /// 4 for `ASM BinaryFile4`, 8 for `ASM BinaryFile8` (integer width).
    pub int_size: usize,
    /// Save version, e.g. 23100 for ASM 231.x.
    pub version: i64,
    /// Record count; 0 in all observed files.
    pub record_count: i64,
    /// Number of top-level entities including the `asmheader` record; they
    /// are the first records of the file.
    pub entity_count: i64,
    /// Bit 0 set when the file carries history data (`.smbh`).
    pub flags: i64,
    pub product: String,
    pub asm_version: String,
    pub date: String,
    /// Millimetres per model unit in `.smb` (10: centimetres). `.smbh`
    /// files hold other multiples of 10 here (see ASM_FORMAT.md).
    pub units: f64,
    pub resabs: f64,
    pub resnor: f64,
}

/// One entity record.
#[derive(Clone, Debug)]
pub struct Record {
    /// Full type name, prefixes joined with '-' (`plane-surface`).
    pub type_name: String,
    /// Token range of the fields (after the type name, before the end tag).
    pub fields: std::ops::Range<usize>,
    /// Byte offset of the record in the file.
    pub offset: usize,
    /// The record lies in the history section (after
    /// `Begin-of-ASM-History-Data`).
    pub in_history: bool,
}

impl Record {
    /// The last part of the type name: `surface` for `plane-surface`.
    pub fn base_type(&self) -> &str {
        self.type_name.rsplit('-').next().unwrap_or(&self.type_name)
    }

    /// The first part of the type name: `plane` for `plane-surface`.
    pub fn sub_type(&self) -> &str {
        self.type_name.split('-').next().unwrap_or(&self.type_name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AsmError {
    NotAsm,
    Token(TokenError),
    Header(String),
}

impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AsmError::NotAsm => f.write_str("not an ASM binary file"),
            AsmError::Token(e) => write!(f, "token error {e}"),
            AsmError::Header(e) => write!(f, "bad header: {e}"),
        }
    }
}

impl std::error::Error for AsmError {}

/// A parsed ASM file: all tokens, records and subtype objects.
pub struct AsmFile {
    pub header: Header,
    pub tokens: Vec<Token>,
    pub records: Vec<Record>,
    /// Token index of the `{` of every subtype object definition, numbered
    /// in file order (pre-order); `{ ref N }` refers to entry N.
    pub subtypes: Vec<usize>,
    /// Error that stopped tokenising before `End-of-ASM-data`, if any; the
    /// records before it are kept.
    pub truncated: Option<TokenError>,
    /// The file ended with `End-of-ASM-data`.
    pub complete: bool,
    /// Records of the history section (`Begin-of-ASM-History-Data` and the
    /// delta states, up to `End-of-ASM-History-Section`). They take no
    /// pointer numbers: the entity copies after them continue the numbering
    /// of the live entities (see [`AsmFile::record_of`]).
    pub history_section: Option<std::ops::Range<usize>>,
}

const MAGIC: &[u8] = b"ASM BinaryFile";

/// The magic of the same binary format as saved before ASM 218 (files of
/// 2012 seen, ASM 217): another four-letter prefix and no integer width
/// digit; integers are 4 bytes.
pub const OLDER_MAGIC: &[u8] = &[
    0x41, 0x43, 0x49, 0x53, b' ', b'B', b'i', b'n', b'a', b'r', b'y', b'F', b'i', b'l', b'e',
];

/// The markers of a file with the older magic name the format by its
/// prefix there (`End-of-<prefix>-data`, `Begin-of-<prefix>-History-Data`,
/// `End-of-<prefix>-History-Section`): they are read as the ASM markers.
fn marker_name(name: String) -> String {
    let prefix = &OLDER_MAGIC[..4];
    for start in ["End-of-", "Begin-of-"] {
        if let Some(rest) = name.strip_prefix(start)
            && rest.as_bytes().starts_with(prefix)
            && rest.as_bytes().get(prefix.len()) == Some(&b'-')
        {
            return format!("{start}ASM{}", &rest[prefix.len()..]);
        }
    }
    name
}

impl AsmFile {
    pub fn parse(data: &[u8]) -> Result<AsmFile, AsmError> {
        let older = data.starts_with(OLDER_MAGIC);
        let (int_size, mut pos) = if older {
            (4, OLDER_MAGIC.len())
        } else {
            if data.len() < MAGIC.len() + 1 || &data[..MAGIC.len()] != MAGIC {
                return Err(AsmError::NotAsm);
            }
            let int_size = match data[MAGIC.len()] {
                b'4' => 4,
                b'8' => 8,
                _ => return Err(AsmError::NotAsm),
            };
            (int_size, MAGIC.len() + 1)
        };
        let mut ints = [0i64; 4];
        for v in ints.iter_mut() {
            let b = data
                .get(pos..pos + int_size)
                .ok_or_else(|| AsmError::Header("truncated".into()))?;
            let mut a = [0u8; 8];
            a[..int_size].copy_from_slice(b);
            *v = if int_size == 4 {
                i64::from(i32::from_le_bytes([a[0], a[1], a[2], a[3]]))
            } else {
                i64::from_le_bytes(a)
            };
            pos += int_size;
        }
        let mut tz = Tokenizer::new(data, pos, int_size);
        let mut strs = Vec::new();
        let mut dbls = Vec::new();
        for _ in 0..3 {
            match tz.next_token().map_err(AsmError::Token)? {
                Token::Str(s) => strs.push(s),
                other => return Err(AsmError::Header(format!("expected a string, got {other}"))),
            }
        }
        for _ in 0..3 {
            match tz.next_token().map_err(AsmError::Token)? {
                Token::Double(d) => dbls.push(d),
                other => return Err(AsmError::Header(format!("expected a double, got {other}"))),
            }
        }
        let header = Header {
            int_size,
            version: ints[0],
            record_count: ints[1],
            entity_count: ints[2],
            flags: ints[3],
            product: strs[0].clone(),
            asm_version: strs[1].clone(),
            date: strs[2].clone(),
            units: dbls[0],
            resabs: dbls[1],
            resnor: dbls[2],
        };

        let mut tokens = Vec::new();
        let mut records = Vec::new();
        let mut subtypes = Vec::new();
        let mut truncated = None;
        let mut complete = false;
        let mut in_history = false;
        let mut history_begin = None;
        let mut history_section = None;
        'records: while !tz.at_end() {
            let offset = tz.offset();
            // Type name: prefixes then the identifier.
            let mut name = String::new();
            loop {
                match tz.next_token() {
                    Ok(Token::IdentPrefix(p)) => {
                        name.push_str(&p);
                        name.push('-');
                    }
                    Ok(Token::Ident(p)) => {
                        name.push_str(&p);
                        break;
                    }
                    Ok(other) => {
                        truncated = Some(TokenError {
                            offset,
                            message: format!("record starts with {other} instead of a type name"),
                        });
                        break 'records;
                    }
                    Err(e) => {
                        truncated = Some(e);
                        break 'records;
                    }
                }
            }
            if older {
                name = marker_name(name);
            }
            match name.as_str() {
                "End-of-ASM-data" => {
                    complete = true;
                    break;
                }
                // A marker without fields; it takes no record index.
                "End-of-ASM-History-Section" => {
                    if let Some(b) = history_begin {
                        history_section = Some(b..records.len());
                    }
                    continue;
                }
                "Begin-of-ASM-History-Data" => {
                    in_history = true;
                    history_begin = Some(records.len());
                }
                _ => {}
            }
            let start = tokens.len();
            loop {
                match tz.next_token() {
                    Ok(Token::End) => break,
                    Ok(tok) => {
                        if tok == Token::SubStart {
                            subtypes.push(tokens.len());
                        }
                        tokens.push(tok);
                    }
                    Err(e) => {
                        truncated = Some(e);
                        tokens.truncate(start);
                        break 'records;
                    }
                }
            }
            let end = tokens.len();
            records.push(Record {
                type_name: name,
                fields: start..end,
                offset,
                in_history,
            });
        }
        // `{ ref N }` is a reference, not a definition.
        subtypes.retain(|&i| !matches!(tokens.get(i + 1), Some(Token::Ident(n)) if n == "ref"));
        Ok(AsmFile {
            header,
            tokens,
            records,
            subtypes,
            truncated,
            complete,
            history_section,
        })
    }

    /// The record a pointer refers to, if any. Pointers past the live
    /// entities of an `.smbh` skip the history section *(verified: the
    /// bulletins of the delta states point at copies of the same entity
    /// type)*.
    pub fn record_of(&self, p: i64) -> Option<usize> {
        let mut i = usize::try_from(p).ok()?;
        if let Some(h) = &self.history_section
            && i >= h.start
        {
            i += h.len();
        }
        (i < self.records.len()).then_some(i)
    }

    pub fn fields(&self, record: usize) -> &[Token] {
        &self.tokens[self.records[record].fields.clone()]
    }

    /// The top-level entities saved explicitly (bodies, and in `.smb` files
    /// also edges and faces referred to by the design), in save order.
    pub fn top_level(&self) -> std::ops::Range<usize> {
        let n = usize::try_from(self.header.entity_count).unwrap_or(0);
        1.min(self.records.len())..n.min(self.records.len())
    }

    /// Record counts per type name.
    pub fn type_counts(&self) -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for r in &self.records {
            *m.entry(r.type_name.clone()).or_insert(0) += 1;
        }
        m
    }

    /// The text form of a record (for diagnostics), shortened to `max` chars.
    pub fn record_text(&self, record: usize, max: usize) -> String {
        let mut s = format!("{} {}", record, self.records[record].type_name);
        for t in self.fields(record) {
            if s.len() > max {
                s.push_str(" ...");
                break;
            }
            s.push(' ');
            s.push_str(&t.to_string());
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asm::writer::Writer;

    #[test]
    fn parses_header_records_and_subtypes() {
        let mut w = Writer::new(2, 2);
        w.record("asmheader")
            .ptr(-1)
            .int(-1)
            .str("231.6.3.65535")
            .end();
        w.record("intcurve-curve")
            .head()
            .bool(false)
            .sub_start()
            .ident("exact_int_cur")
            .int(23100);
        w.sub_end().bool(false).bool(false).end();
        w.record("spline-surface")
            .head()
            .bool(false)
            .sub_start()
            .ident("ref")
            .int(0)
            .sub_end();
        w.end();
        let data = w.finish();
        let f = AsmFile::parse(&data).unwrap();
        assert!(f.complete);
        assert_eq!(f.header.version, 23100);
        assert_eq!(f.header.units, 10.0);
        assert_eq!(f.header.asm_version, "ASM 231.6.3.65535 NT");
        assert_eq!(f.records.len(), 3);
        assert_eq!(f.records[1].type_name, "intcurve-curve");
        assert_eq!(f.records[1].base_type(), "curve");
        assert_eq!(f.records[1].sub_type(), "intcurve");
        assert_eq!(f.subtypes.len(), 1);
        assert_eq!(
            f.tokens[f.subtypes[0] + 1],
            Token::Ident("exact_int_cur".into())
        );
        assert_eq!(f.top_level(), 1..2);
    }

    #[test]
    fn parses_the_older_magic() {
        let mut w = Writer::new(1, 2);
        w.record("asmheader").ptr(-1).int(-1).str("217.0").end();
        // The same file with the older magic (no integer width digit) and
        // markers named by its prefix.
        let prefix = std::str::from_utf8(&OLDER_MAGIC[..4]).unwrap();
        let mut older = OLDER_MAGIC.to_vec();
        older.extend_from_slice(&w.data[MAGIC.len() + 1..]);
        let data = w.finish();
        let mut o = Writer { data: older };
        o.record(&format!("Begin-of-{prefix}-History-Data")).end();
        o.record(&format!("End-of-{prefix}-History-Section"));
        o.record(&format!("End-of-{prefix}-data"));
        o.data.extend_from_slice(&[0; 4]);
        let f = AsmFile::parse(&o.data).unwrap();
        assert!(f.complete, "{:?}", f.truncated);
        assert_eq!(f.header.int_size, 4);
        assert_eq!(
            f.header.version,
            AsmFile::parse(&data).unwrap().header.version
        );
        assert_eq!(f.records.len(), 2);
        assert_eq!(f.records[1].type_name, "Begin-of-ASM-History-Data");
        assert_eq!(f.history_section, Some(1..2));
        assert_eq!(marker_name("End-of-data".into()), "End-of-data");
        // Neither magic.
        o.data[0] = b'X';
        assert!(matches!(AsmFile::parse(&o.data), Err(AsmError::NotAsm)));
    }

    #[test]
    fn keeps_records_before_an_error() {
        let mut w = Writer::new(1, 2);
        w.record("asmheader").ptr(-1).int(-1).str("x").end();
        w.data.push(0x42);
        let f = AsmFile::parse(&w.data).unwrap();
        assert_eq!(f.records.len(), 1);
        assert!(f.truncated.is_some());
        assert!(!f.complete);
    }
}
