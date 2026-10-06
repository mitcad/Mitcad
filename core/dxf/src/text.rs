// SPDX-License-Identifier: MIT
//! Text decoding: file encodings, `\U+XXXX` escapes, `%%` control codes and
//! MTEXT formatting.

/// Decodes one line of the file: UTF-8 (R2007 and newer), otherwise
/// Windows-1252, the usual `$DWGCODEPAGE` of older files.
pub(crate) fn decode_line(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes.iter().map(|&b| windows_1252(b)).collect(),
    }
}

fn windows_1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}',
        '\u{8F}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}',
        '\u{2014}', '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}',
        '\u{178}',
    ];
    match byte {
        0x80..=0x9F => HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

/// Replaces `\U+XXXX` escapes (R2000-R2004 files store non-ASCII text so).
fn unicode_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find("\\U+") {
        out.push_str(&rest[..index]);
        let hex = rest.get(index + 3..index + 7);
        match hex
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .and_then(char::from_u32)
        {
            Some(c) => {
                out.push(c);
                rest = &rest[index + 7..];
            }
            None => {
                out.push_str("\\U+");
                rest = &rest[index + 3..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `%%d` (degree), `%%p` (plus-minus), `%%c` (diameter), `%%%` and the
/// underline/overline toggles `%%u`, `%%o` of single-line text.
fn control_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' && chars.peek() == Some(&'%') {
            let mut ahead = chars.clone();
            ahead.next();
            let replacement = match ahead.next().map(|c| c.to_ascii_lowercase()) {
                Some('d') => Some(Some('\u{B0}')),
                Some('p') => Some(Some('\u{B1}')),
                Some('c') => Some(Some('\u{2300}')),
                Some('%') => Some(Some('%')),
                Some('u') | Some('o') | Some('k') => Some(None),
                _ => None,
            };
            if let Some(replacement) = replacement {
                chars = ahead;
                if let Some(r) = replacement {
                    out.push(r);
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Plain text of a `TEXT` entity value.
pub(crate) fn single_line(value: &str) -> String {
    control_codes(&unicode_escapes(value))
}

/// Plain text of an `MTEXT` value: paragraphs become `\n`, formatting codes
/// (`\fArial|b1;`, `\H2.5x;`, `\C1;`, `{...}` groups, ...) are removed and
/// stacked fractions (`\S1/2;`) are written inline.
pub(crate) fn multi_line(value: &str) -> String {
    let value = unicode_escapes(value);
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' | '}' => {}
            '\\' => match chars.next() {
                Some('P') | Some('X') | Some('N') => out.push('\n'),
                Some('~') => out.push('\u{A0}'),
                Some(c @ ('\\' | '{' | '}')) => out.push(c),
                // Toggles without arguments: underline, overline, strike-through.
                Some('L' | 'l' | 'O' | 'o' | 'K' | 'k') => {}
                Some('S') => {
                    // Stacked text up to ';': "1^2", "1/2" or "1#2".
                    for s in chars.by_ref() {
                        match s {
                            ';' => break,
                            '^' | '#' => out.push('/'),
                            _ => out.push(s),
                        }
                    }
                }
                // Codes with an argument up to ';' (font, height, colour, ...).
                Some(
                    'f' | 'F' | 'H' | 'h' | 'W' | 'w' | 'Q' | 'q' | 'T' | 't' | 'A' | 'a' | 'C'
                    | 'c' | 'p',
                ) => {
                    for s in chars.by_ref() {
                        if s == ';' {
                            break;
                        }
                    }
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            },
            _ => out.push(c),
        }
    }
    control_codes(&out)
}

/// Escapes text for writing: non-ASCII characters as `\U+XXXX` (readable by
/// every DXF version), line breaks as spaces in single-line text.
pub(crate) fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\n' | '\r' => out.push(' '),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c if c.is_ascii_control() => {}
            c => {
                let code = c as u32;
                if code <= 0xFFFF {
                    out.push_str(&format!("\\U+{code:04X}"));
                } else {
                    // Outside the basic plane: the escape has four digits only.
                    out.push('?');
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_codes() {
        assert_eq!(single_line("\\U+00E4l\\U+00F6"), "\u{E4}l\u{F6}");
        assert_eq!(
            single_line("45%%d %%p0.1 %%c10 100%%%"),
            "45\u{B0} \u{B1}0.1 \u{2300}10 100%"
        );
        assert_eq!(single_line("%%uunder%%u"), "under");
        assert_eq!(single_line("50% off"), "50% off");
    }

    #[test]
    fn mtext_formatting() {
        assert_eq!(
            multi_line("{\\fArial|b1|i0|c0|p34;Bold}\\Pnext"),
            "Bold\nnext"
        );
        assert_eq!(
            multi_line("\\H2.5x;\\C1;Red \\S1/2; in\\~x"),
            "Red 1/2 in\u{A0}x"
        );
        assert_eq!(multi_line("a\\\\b \\{c\\}"), "a\\b {c}");
        assert_eq!(multi_line("\\Lunder\\l"), "under");
    }

    #[test]
    fn encodings() {
        assert_eq!(decode_line(b"caf\xe9 \x80"), "caf\u{E9} \u{20AC}");
        assert_eq!(decode_line("café".as_bytes()), "café");
        assert_eq!(encode("é\nx"), "\\U+00E9 x");
    }
}
