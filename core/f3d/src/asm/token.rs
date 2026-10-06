// SPDX-License-Identifier: MIT
//! Tokeniser for the ASM binary format. See `ASM_FORMAT.md`.

use std::fmt;

/// One tagged value of the token stream.
#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    /// Tag 0x04: integer (4 bytes in BinaryFile4, 8 bytes in BinaryFile8).
    Int(i64),
    /// Tag 0x06: 8-byte double.
    Double(f64),
    /// Tags 0x07/0x08/0x09: string with a 1/2/4-byte length.
    Str(String),
    /// Tag 0x0a.
    True,
    /// Tag 0x0b.
    False,
    /// Tag 0x0c: record index, -1 for none (integer width as for `Int`).
    Ptr(i64),
    /// Tag 0x0d: identifier (record type, subtype and keyword names).
    Ident(String),
    /// Tag 0x0e: identifier prefix; joined with '-' to the next identifier
    /// (`plane` + `surface` = `plane-surface`).
    IdentPrefix(String),
    /// Tag 0x0f: start of a subtype object `{`.
    SubStart,
    /// Tag 0x10: end of a subtype object `}`.
    SubEnd,
    /// Tag 0x11: end of a record.
    End,
    /// Tag 0x12: string with a 4-byte length.
    Literal(String),
    /// Tag 0x13: position (3 doubles).
    Pos([f64; 3]),
    /// Tag 0x14: vector (3 doubles).
    Vector([f64; 3]),
    /// Tag 0x15: enumeration value (integer width as for `Int`).
    Enum(i64),
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Int(v) => write!(f, "{v}"),
            Token::Double(v) => write!(f, "{v}"),
            Token::Str(s) => write!(f, "{s:?}"),
            Token::True => f.write_str("T"),
            Token::False => f.write_str("F"),
            Token::Ptr(v) => write!(f, "${v}"),
            Token::Ident(s) => f.write_str(s),
            Token::IdentPrefix(s) => write!(f, "{s}-"),
            Token::SubStart => f.write_str("{"),
            Token::SubEnd => f.write_str("}"),
            Token::End => f.write_str("#"),
            Token::Literal(s) => write!(f, "@{s:?}"),
            Token::Pos(p) => write!(f, "P({} {} {})", p[0], p[1], p[2]),
            Token::Vector(p) => write!(f, "V({} {} {})", p[0], p[1], p[2]),
            Token::Enum(v) => write!(f, "E{v}"),
        }
    }
}

/// Tokenisation error: an unknown tag or data running past the end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at byte {}: {}", self.offset, self.message)
    }
}

/// Reads tokens from a byte buffer.
pub struct Tokenizer<'a> {
    data: &'a [u8],
    pos: usize,
    int_size: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(data: &'a [u8], pos: usize, int_size: usize) -> Self {
        Tokenizer {
            data,
            pos,
            int_size,
        }
    }

    pub fn offset(&self) -> usize {
        self.pos
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], TokenError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        match end {
            Some(end) => {
                let s = &self.data[self.pos..end];
                self.pos = end;
                Ok(s)
            }
            None => Err(TokenError {
                offset: self.pos,
                message: format!("{n} bytes needed past the end of data"),
            }),
        }
    }

    fn int(&mut self, size: usize) -> Result<i64, TokenError> {
        let b = self.take(size)?;
        Ok(match size {
            1 => i64::from(b[0] as i8),
            2 => i64::from(i16::from_le_bytes([b[0], b[1]])),
            4 => i64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            _ => {
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                i64::from_le_bytes(a)
            }
        })
    }

    fn uint(&mut self, size: usize) -> Result<usize, TokenError> {
        let b = self.take(size)?;
        let mut a = [0u8; 8];
        a[..size].copy_from_slice(b);
        Ok(u64::from_le_bytes(a) as usize)
    }

    fn double(&mut self) -> Result<f64, TokenError> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(f64::from_le_bytes(a))
    }

    fn string(&mut self, len_size: usize) -> Result<String, TokenError> {
        let n = self.uint(len_size)?;
        let b = self.take(n)?;
        // Names are ASCII; other bytes are kept as Latin-1.
        Ok(match std::str::from_utf8(b) {
            Ok(s) => s.to_string(),
            Err(_) => b.iter().map(|&c| c as char).collect(),
        })
    }

    fn triple(&mut self) -> Result<[f64; 3], TokenError> {
        Ok([self.double()?, self.double()?, self.double()?])
    }

    /// Reads the next token.
    pub fn next_token(&mut self) -> Result<Token, TokenError> {
        let start = self.pos;
        let tag = self.take(1)?[0];
        let isz = self.int_size;
        Ok(match tag {
            0x02 => Token::Int(self.int(1)?),
            0x03 => Token::Int(self.int(2)?),
            0x04 => Token::Int(self.int(isz)?),
            0x05 => {
                let b = self.take(4)?;
                Token::Double(f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])))
            }
            0x06 => Token::Double(self.double()?),
            0x07 => Token::Str(self.string(1)?),
            0x08 => Token::Str(self.string(2)?),
            0x09 => Token::Str(self.string(4)?),
            0x0a => Token::True,
            0x0b => Token::False,
            0x0c => Token::Ptr(self.int(isz)?),
            0x0d => Token::Ident(self.string(1)?),
            0x0e => Token::IdentPrefix(self.string(1)?),
            0x0f => Token::SubStart,
            0x10 => Token::SubEnd,
            0x11 => Token::End,
            0x12 => Token::Literal(self.string(4)?),
            0x13 => Token::Pos(self.triple()?),
            0x14 => Token::Vector(self.triple()?),
            0x15 => Token::Enum(self.int(isz)?),
            other => {
                return Err(TokenError {
                    offset: start,
                    message: format!("unknown tag 0x{other:02x}"),
                });
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tagged_values_with_4_byte_integers() {
        let mut data = vec![0x04, 7, 0, 0, 0, 0x0c, 0xff, 0xff, 0xff, 0xff, 0x06];
        data.extend_from_slice(&2.5f64.to_le_bytes());
        data.extend_from_slice(&[0x07, 3, b'a', b'b', b'c', 0x0e, 5]);
        data.extend_from_slice(b"plane");
        data.extend_from_slice(&[0x0d, 7]);
        data.extend_from_slice(b"surface");
        data.extend_from_slice(&[0x0a, 0x0b, 0x0f, 0x10, 0x11]);
        let mut t = Tokenizer::new(&data, 0, 4);
        let mut out = Vec::new();
        while !t.at_end() {
            out.push(t.next_token().unwrap());
        }
        assert_eq!(
            out,
            vec![
                Token::Int(7),
                Token::Ptr(-1),
                Token::Double(2.5),
                Token::Str("abc".into()),
                Token::IdentPrefix("plane".into()),
                Token::Ident("surface".into()),
                Token::True,
                Token::False,
                Token::SubStart,
                Token::SubEnd,
                Token::End,
            ]
        );
    }

    #[test]
    fn reads_8_byte_integers() {
        let mut data = vec![0x15];
        data.extend_from_slice(&(-2i64).to_le_bytes());
        let mut t = Tokenizer::new(&data, 0, 8);
        assert_eq!(t.next_token().unwrap(), Token::Enum(-2));
    }

    #[test]
    fn reports_unknown_tags_and_truncation() {
        assert!(Tokenizer::new(&[0x42], 0, 4).next_token().is_err());
        assert!(Tokenizer::new(&[0x06, 1, 2], 0, 4).next_token().is_err());
    }
}
