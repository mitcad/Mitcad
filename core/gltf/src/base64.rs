// SPDX-License-Identifier: MIT
//! Base64 (RFC 4648) for the data URIs of glTF files' buffers and images:
//! padding optional, whitespace skipped.

fn value(byte: u8) -> Option<u32> {
    Some(u32::from(match byte {
        b'A'..=b'Z' => byte - b'A',
        b'a'..=b'z' => byte - b'a' + 26,
        b'0'..=b'9' => byte - b'0' + 52,
        b'+' | b'-' => 62,
        b'/' | b'_' => 63,
        _ => return None,
    }))
}

/// The bytes of base64 text (the standard or the URL alphabet).
pub fn decode(text: &str) -> Result<Vec<u8>, String> {
    let mut data = Vec::with_capacity(text.len() / 4 * 3);
    let mut bits = 0u32;
    let mut held = 0u32;
    let mut ended = false;
    for (at, byte) in text.bytes().enumerate() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            ended = true;
            continue;
        }
        if ended {
            return Err(format!("base64 data goes on after its padding (at {at})"));
        }
        let v = value(byte).ok_or_else(|| {
            format!(
                "a character that is not base64 ({}) at {at}",
                char::from(byte).escape_default()
            )
        })?;
        bits = (bits << 6) | v;
        held += 6;
        if held >= 8 {
            held -= 8;
            data.push((bits >> held) as u8);
            bits &= (1 << held) - 1;
        }
    }
    Ok(data)
}

/// Base64 text of bytes, padded (the test files' data URIs).
pub fn encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let bits = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                text.push(char::from(ALPHABET[((bits >> (18 - 6 * i)) & 63) as usize]));
            } else {
                text.push('=');
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_what_it_encodes_with_and_without_padding() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let text = encode(data);
            assert_eq!(decode(&text).unwrap(), data, "{text}");
            assert_eq!(decode(text.trim_end_matches('=')).unwrap(), data, "{text}");
        }
        assert_eq!(decode("Zm9v\nYmFy").unwrap(), b"foobar");
        assert!(decode("Zm9v!").is_err());
        assert!(decode("Zg==Zg==").is_err());
    }
}
