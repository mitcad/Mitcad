// SPDX-License-Identifier: MIT
//! Tokenizer for expressions and unit strings.

use super::parser::{ParseError, ParseErrorKind};
use super::{Span, is_name_char, is_name_start};

/// Longest accepted expression, in tokens. Keeps recursion in evaluation
/// and printing shallow even for hostile input.
pub(crate) const MAX_TOKENS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Tok {
    Number(f64),
    /// Name, unit, function or constant; the text is in the span.
    Ident,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    LParen,
    RParen,
    Semicolon,
    Comma,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Token {
    pub tok: Tok,
    pub span: Span,
}

pub(crate) fn lex(text: &str) -> Result<Vec<Token>, ParseError> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while let Some(c) = text[i..].chars().next() {
        let start = i;
        let next_byte = bytes.get(i + 1).copied();
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        let digit_next = next_byte.is_some_and(|b| b.is_ascii_digit());
        // `.5`, and `,5` where the comma cannot separate arguments: after
        // a value it does (`max(a,5)`, `max(1.5,5)`).
        let starts_number = c.is_ascii_digit()
            || (c == '.' && digit_next)
            || (c == ',' && digit_next && !tokens.last().is_some_and(ends_value));
        let tok = if starts_number {
            i = number_end(bytes, i);
            let source = &text[start..i];
            let value: Option<f64> = if source.contains(',') {
                source.replace(',', ".").parse().ok()
            } else {
                source.parse().ok()
            };
            let value = value.filter(|v| v.is_finite()).ok_or_else(|| {
                ParseError::new(ParseErrorKind::InvalidNumber, Span::new(start, i))
            })?;
            Tok::Number(value)
        } else if is_name_start(c) {
            i += text[i..]
                .char_indices()
                .find(|&(_, c)| !is_name_char(c))
                .map_or(text.len() - i, |(n, _)| n);
            Tok::Ident
        } else {
            i += c.len_utf8();
            let two = |second: u8, tok: Tok, i: &mut usize| {
                (next_byte == Some(second)).then(|| {
                    *i += 1;
                    tok
                })
            };
            match c {
                '°' => Tok::Ident,
                '+' => Tok::Plus,
                '-' => Tok::Minus,
                '*' if next_byte == Some(b'*') => {
                    return Err(ParseError::new(
                        ParseErrorKind::PowerOperator,
                        Span::new(start, start + 2),
                    ));
                }
                '*' => Tok::Star,
                '/' => Tok::Slash,
                '%' => Tok::Percent,
                '^' => Tok::Caret,
                '(' => Tok::LParen,
                ')' => Tok::RParen,
                ';' => Tok::Semicolon,
                ',' => Tok::Comma,
                '<' => two(b'=', Tok::Le, &mut i)
                    .or_else(|| two(b'>', Tok::Ne, &mut i))
                    .unwrap_or(Tok::Lt),
                '>' => two(b'=', Tok::Ge, &mut i).unwrap_or(Tok::Gt),
                '=' => two(b'=', Tok::Eq, &mut i).ok_or_else(|| {
                    ParseError::new(ParseErrorKind::SingleEquals, Span::new(start, start + 1))
                })?,
                '!' => two(b'=', Tok::Ne, &mut i).ok_or_else(|| {
                    ParseError::new(ParseErrorKind::UnexpectedChar('!'), Span::new(start, i))
                })?,
                _ => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedChar(c),
                        Span::new(start, i),
                    ));
                }
            }
        };
        tokens.push(Token {
            tok,
            span: Span::new(start, i),
        });
        if tokens.len() > MAX_TOKENS {
            return Err(ParseError::new(
                ParseErrorKind::TooComplex,
                Span::new(0, text.len()),
            ));
        }
    }
    Ok(tokens)
}

/// Whether a token ends a value, so that a comma after it separates
/// arguments.
fn ends_value(token: &Token) -> bool {
    matches!(token.tok, Tok::Number(_) | Tok::Ident | Tok::RParen)
}

/// End of the number starting at `i`: digits, an optional fraction and an
/// optional exponent. An `e` not followed by digits is not part of the
/// number (it may start a name). The decimal separator is `.` or `,` (see
/// [`decimal_comma`]).
fn number_end(bytes: &[u8], mut i: usize) -> usize {
    let digits = |mut i: usize| {
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        i
    };
    i = digits(i);
    match bytes.get(i) {
        Some(b'.') => i = digits(i + 1),
        Some(b',') if decimal_comma(bytes, i) => i = digits(i + 1),
        _ => {}
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(bytes.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if bytes.get(j).is_some_and(u8::is_ascii_digit) {
            i = digits(j);
        }
    }
    i
}

/// Whether the comma at `i`, inside a number (after its digits, or where
/// the lexer starts a number with it), is the number's decimal separator:
/// when a digit follows (`1,5`, `,5`), or when nothing that could be an
/// argument follows (`5,` at the end, before `)`, `;` or an operator other
/// than a sign). Any other comma separates arguments: `max(1, 5)`,
/// `max(5,a)`, `max(1,-5)`.
fn decimal_comma(bytes: &[u8], i: usize) -> bool {
    let rest = &bytes[i + 1..];
    if rest.first().is_some_and(u8::is_ascii_digit) {
        return true;
    }
    // After an ASCII comma the rest is whole characters.
    let rest = std::str::from_utf8(rest).unwrap_or_default();
    match rest.chars().find(|c| !c.is_whitespace()) {
        None => true,
        Some(c) => ")*/%^<>=!;".contains(c),
    }
}

/// Byte positions of the decimal commas in `text` (`1,5 mm`, `,5`), none
/// when it does not lex.
pub(crate) fn decimal_commas(text: &str) -> Vec<usize> {
    let Ok(tokens) = lex(text) else {
        return Vec::new();
    };
    tokens
        .iter()
        .filter(|t| matches!(t.tok, Tok::Number(_)))
        .flat_map(|t| (t.span.start..t.span.end).filter(|&i| text.as_bytes()[i] == b','))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<Tok> {
        lex(text).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn numbers() {
        assert_eq!(kinds("1e-3"), [Tok::Number(0.001)]);
        assert_eq!(kinds(".5"), [Tok::Number(0.5)]);
        assert_eq!(kinds("5."), [Tok::Number(5.0)]);
        assert_eq!(kinds("2.5E+2"), [Tok::Number(250.0)]);
        assert_eq!(kinds("1.e2"), [Tok::Number(100.0)]);
        // "e" without digits starts a name.
        assert_eq!(kinds("2em"), [Tok::Number(2.0), Tok::Ident]);
        assert_eq!(kinds("10mm"), [Tok::Number(10.0), Tok::Ident]);
        assert_eq!(
            lex("1e999").unwrap_err().kind,
            ParseErrorKind::InvalidNumber
        );
    }

    #[test]
    fn numbers_with_decimal_commas() {
        // The forms of a decimal point work with a comma.
        assert_eq!(kinds("1,5"), [Tok::Number(1.5)]);
        assert_eq!(kinds(",5"), [Tok::Number(0.5)]);
        assert_eq!(kinds("5,"), [Tok::Number(5.0)]);
        assert_eq!(kinds("2,5e3"), [Tok::Number(2500.0)]);
        assert_eq!(kinds("1,5mm"), [Tok::Number(1.5), Tok::Ident]);
        assert_eq!(kinds("0,25 mm"), [Tok::Number(0.25), Tok::Ident]);
        assert_eq!(
            kinds("2*,5"),
            [Tok::Number(2.0), Tok::Star, Tok::Number(0.5)]
        );
        assert_eq!(kinds("(5,)"), [Tok::LParen, Tok::Number(5.0), Tok::RParen]);
        assert_eq!(
            kinds("5, * 2"),
            [Tok::Number(5.0), Tok::Star, Tok::Number(2.0)]
        );
        // No thousands separators: one fraction.
        assert_eq!(kinds("1,000"), [Tok::Number(1.0)]);
        // Other commas separate arguments.
        let call = |args: &[Tok]| {
            let mut tokens = vec![Tok::Ident, Tok::LParen];
            tokens.extend_from_slice(args);
            tokens.push(Tok::RParen);
            tokens
        };
        assert_eq!(
            kinds("max(a,b)"),
            call(&[Tok::Ident, Tok::Comma, Tok::Ident])
        );
        assert_eq!(
            kinds("max(1, 5)"),
            call(&[Tok::Number(1.0), Tok::Comma, Tok::Number(5.0)])
        );
        assert_eq!(
            kinds("max(1;5)"),
            call(&[Tok::Number(1.0), Tok::Semicolon, Tok::Number(5.0)])
        );
        assert_eq!(
            kinds("max(a,5)"),
            call(&[Tok::Ident, Tok::Comma, Tok::Number(5.0)])
        );
        assert_eq!(
            kinds("max(1.5,5)"),
            call(&[Tok::Number(1.5), Tok::Comma, Tok::Number(5.0)])
        );
        assert_eq!(
            kinds("max(1,-5)"),
            call(&[Tok::Number(1.0), Tok::Comma, Tok::Minus, Tok::Number(5.0)])
        );
        assert_eq!(
            kinds("max(5,e3)"),
            call(&[Tok::Number(5.0), Tok::Comma, Tok::Ident])
        );
        // A comma between two digits is a decimal comma.
        assert_eq!(kinds("max(1,5)"), call(&[Tok::Number(1.5)]));
        assert_eq!(
            kinds("max(1,5,2)"),
            call(&[Tok::Number(1.5), Tok::Comma, Tok::Number(2.0)])
        );
        assert_eq!(decimal_commas("d1 * 1,5 + max(,5, 2,)"), [6, 15, 20]);
        assert!(decimal_commas("max(a, b)").is_empty());
    }

    #[test]
    fn operators_and_spans() {
        let tokens = lex("a <= b<>c != d == e >f").unwrap();
        let ops: Vec<Tok> = tokens
            .iter()
            .map(|t| t.tok)
            .filter(|t| *t != Tok::Ident)
            .collect();
        assert_eq!(ops, [Tok::Le, Tok::Ne, Tok::Ne, Tok::Eq, Tok::Gt]);
        assert_eq!(tokens[1].span, Span::new(2, 4));
        let unicode = lex("µm\u{a0}°").unwrap();
        assert_eq!(unicode[0].span, Span::new(0, 3));
        assert_eq!(unicode[1].span, Span::new(5, 7));
    }

    #[test]
    fn errors_have_positions() {
        let e = lex("2 ** 3").unwrap_err();
        assert_eq!(
            (e.kind, e.span),
            (ParseErrorKind::PowerOperator, Span::new(2, 4))
        );
        let e = lex("a = b").unwrap_err();
        assert_eq!(
            (e.kind, e.span),
            (ParseErrorKind::SingleEquals, Span::new(2, 3))
        );
        let e = lex("1 + $").unwrap_err();
        assert_eq!(
            (e.kind, e.span),
            (ParseErrorKind::UnexpectedChar('$'), Span::new(4, 5))
        );
        let e = lex("1 + €").unwrap_err();
        assert_eq!(e.span, Span::new(4, 7));
    }
}
