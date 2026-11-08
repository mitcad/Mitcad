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
    /// Name, unit, function or constant; the text is in the span. Also an
    /// inch or foot mark after a number, `"`, `''` or `'` (mitcad#1531).
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
    /// Text in double or single quotes, `"mm"`, `'6qt'`; the span includes
    /// the quotes. A quote of the same kind inside is doubled: `'It''s'`
    /// (see [`text_of`]).
    Str,
    /// `&`, which joins texts (mitcad#664).
    Amp,
    /// The `.` of a column reference, `Screws.dk` (mitcad#667): only
    /// directly between a name and a name start, with no space around it.
    Dot,
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
    // Open parentheses: a single `=` inside them compares (mitcad#663).
    let mut depth = 0usize;
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
        } else if c == '.'
            && tokens
                .last()
                .is_some_and(|t| t.tok == Tok::Ident && t.span.end == i)
            && text[i + 1..].chars().next().is_some_and(is_name_start)
        {
            // `Screws.dk` (mitcad#667); before it, the text was an error.
            i += 1;
            Tok::Dot
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
                '(' => {
                    depth += 1;
                    Tok::LParen
                }
                ')' => {
                    depth = depth.saturating_sub(1);
                    Tok::RParen
                }
                ';' => Tok::Semicolon,
                ',' => Tok::Comma,
                '&' => Tok::Amp,
                // An inch or foot mark (mitcad#1531): a quote right after a
                // number, spaces allowed between, is its unit (`.875"`,
                // `3' 6"`, `2''`), never the start of a text. Nothing else
                // could stand there, as a value never follows a number.
                '"' | '\''
                    if tokens
                        .last()
                        .is_some_and(|t| matches!(t.tok, Tok::Number(_))) =>
                {
                    if c == '\'' && next_byte == Some(b'\'') {
                        // Two single quotes, `2''`, are the inch mark too.
                        i += 1;
                    }
                    Tok::Ident
                }
                '"' | '\'' => {
                    i = quoted_end(text, start, c).ok_or_else(|| {
                        ParseError::new(
                            ParseErrorKind::UnterminatedText,
                            Span::new(start, text.len()),
                        )
                    })?;
                    Tok::Str
                }
                '<' => two(b'=', Tok::Le, &mut i)
                    .or_else(|| two(b'>', Tok::Ne, &mut i))
                    .unwrap_or(Tok::Lt),
                '>' => two(b'=', Tok::Ge, &mut i).unwrap_or(Tok::Gt),
                // A single `=` compares inside parentheses; at the top level
                // `name = expression` names a slot's parameter, which the
                // caller splits off before parsing (mitcad#231).
                '=' if depth > 0 => two(b'=', Tok::Eq, &mut i).unwrap_or(Tok::Eq),
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

/// The end (after the closing quote) of the quoted text that starts with the
/// quote `quote` at `start`; a doubled quote inside is one quote character
/// (`'It''s'`). None when it is not closed.
fn quoted_end(text: &str, start: usize, quote: char) -> Option<usize> {
    let mut at = start + quote.len_utf8();
    loop {
        let close = at + text[at..].find(quote)?;
        let after = close + quote.len_utf8();
        if text[after..].starts_with(quote) {
            at = after + quote.len_utf8();
        } else {
            return Some(after);
        }
    }
}

/// The characters of a quoted text token (its span's source, quotes
/// included): without the quotes, a doubled quote as one.
pub(crate) fn text_of(source: &str) -> String {
    let mut chars = source.chars();
    let Some(quote) = chars.next() else {
        return String::new();
    };
    if source.len() < 2 * quote.len_utf8() {
        return String::new();
    }
    let inner = &source[quote.len_utf8()..source.len() - quote.len_utf8()];
    let doubled: String = [quote, quote].iter().collect();
    inner.replace(&doubled, &quote.to_string())
}

/// Whether a token ends a value, so that a comma after it separates
/// arguments.
fn ends_value(token: &Token) -> bool {
    matches!(
        token.tok,
        Tok::Number(_) | Tok::Ident | Tok::RParen | Tok::Str
    )
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
        Some(c) => ")*/%^<>=!;&".contains(c),
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

    #[test]
    fn single_equals_compares_inside_parentheses() {
        assert_eq!(
            kinds("(a = b)"),
            [Tok::LParen, Tok::Ident, Tok::Eq, Tok::Ident, Tok::RParen]
        );
        let tokens = lex("if(a = 1; 2; 3)").unwrap();
        assert_eq!(tokens[3].tok, Tok::Eq);
        assert_eq!(tokens[3].span, Span::new(5, 6));
        // After the parentheses close, the top level again.
        let e = lex("(a) = b").unwrap_err();
        assert_eq!(
            (e.kind, e.span),
            (ParseErrorKind::SingleEquals, Span::new(4, 5))
        );
    }

    #[test]
    fn quoted_text() {
        let tokens = lex("value(d1; \"mm^2\")").unwrap();
        assert_eq!(tokens[4].tok, Tok::Str);
        assert_eq!(tokens[4].span, Span::new(10, 16));
        assert_eq!(tokens[5].tok, Tok::RParen);
        // A comma after text separates arguments.
        assert_eq!(kinds("\"mm\",5"), [Tok::Str, Tok::Comma, Tok::Number(5.0)]);
        let e = lex("value(d1; \"mm)").unwrap_err();
        assert_eq!(
            (e.kind, e.span),
            (ParseErrorKind::UnterminatedText, Span::new(10, 14))
        );
    }

    #[test]
    fn texts_in_either_quotes_and_ampersands() {
        // mitcad#664: single quotes as `.f3d` expressions write them, a
        // doubled quote inside, `&` joining.
        let tokens = lex("'It''s' & \"say \"\"hi\"\"\"").unwrap();
        assert_eq!(
            tokens.iter().map(|t| t.tok).collect::<Vec<_>>(),
            [Tok::Str, Tok::Amp, Tok::Str]
        );
        let source = |t: &Token| text_of(&"'It''s' & \"say \"\"hi\"\"\""[t.span.start..t.span.end]);
        assert_eq!(source(&tokens[0]), "It's");
        assert_eq!(source(&tokens[2]), "say \"hi\"");
        // The other quote is an ordinary character inside.
        let tokens = lex("\"it's\"").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(text_of("\"it's\""), "it's");
        assert_eq!(text_of("''"), "");
        let e = lex("'open").unwrap_err();
        assert_eq!(
            (e.kind, e.span),
            (ParseErrorKind::UnterminatedText, Span::new(0, 5))
        );
        // A comma before `&` ends a number with a decimal comma.
        assert_eq!(kinds("5,&'a'"), [Tok::Number(5.0), Tok::Amp, Tok::Str]);
    }

    #[test]
    fn inch_and_foot_marks_after_numbers() {
        // mitcad#1531: a quote right after a number is its unit.
        assert_eq!(kinds(".875\""), [Tok::Number(0.875), Tok::Ident]);
        assert_eq!(kinds("2.5 \""), [Tok::Number(2.5), Tok::Ident]);
        let tokens = lex("3' 6''").unwrap();
        assert_eq!(
            tokens.iter().map(|t| t.tok).collect::<Vec<_>>(),
            [Tok::Number(3.0), Tok::Ident, Tok::Number(6.0), Tok::Ident]
        );
        assert_eq!(tokens[1].span, Span::new(1, 2));
        assert_eq!(tokens[3].span, Span::new(4, 6));
        // Elsewhere a quote starts a text.
        assert_eq!(kinds("5 & 'a'"), [Tok::Number(5.0), Tok::Amp, Tok::Str]);
        assert_eq!(kinds("a 'b'"), [Tok::Ident, Tok::Str]);
        assert_eq!(
            lex("d1\"").unwrap_err().kind,
            ParseErrorKind::UnterminatedText
        );
    }

    #[test]
    fn dots_between_names() {
        // mitcad#667: a column reference.
        assert_eq!(kinds("Screws.dk"), [Tok::Ident, Tok::Dot, Tok::Ident]);
        assert_eq!(lex("Screws.dk").unwrap()[1].span, Span::new(6, 7));
        assert_eq!(kinds("T.in"), [Tok::Ident, Tok::Dot, Tok::Ident]);
        // Numbers keep their points.
        assert_eq!(kinds("d1.5"), [Tok::Ident, Tok::Number(0.5)]);
        assert_eq!(kinds("5.e2"), [Tok::Number(500.0)]);
        // Elsewhere a dot is the error it was.
        for text in [
            "Screws .dk",
            "Screws. dk",
            ".dk",
            "Screws.",
            "2 .x",
            "a.'b'",
        ] {
            assert!(
                matches!(
                    lex(text).unwrap_err().kind,
                    ParseErrorKind::UnexpectedChar('.')
                ),
                "{text}"
            );
        }
    }
}
