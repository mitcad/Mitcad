// SPDX-License-Identifier: MIT
//! Standard base64 (RFC 4648, with padding) for binary data in project
//! files, such as the B-rep of imported bodies.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn encode(data: &[u8]) -> String {
    let mut text = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let bits = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
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

fn value(byte: u8) -> Option<u32> {
    Some(u32::from(match byte {
        b'A'..=b'Z' => byte - b'A',
        b'a'..=b'z' => byte - b'a' + 26,
        b'0'..=b'9' => byte - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    }))
}

/// Decodes padded base64; whitespace is not allowed.
pub(crate) fn decode(text: &str) -> Result<Vec<u8>, String> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("base64 data must be a multiple of 4 characters long".to_owned());
    }
    let mut data = Vec::with_capacity(bytes.len() / 4 * 3);
    let quads = bytes.len() / 4;
    for (q, quad) in bytes.chunks(4).enumerate() {
        let padding = if q + 1 == quads {
            quad.iter().rev().take_while(|b| **b == b'=').count()
        } else {
            0
        };
        if padding > 2 {
            return Err("invalid base64 padding".to_owned());
        }
        let mut bits = 0u32;
        for (i, byte) in quad.iter().enumerate() {
            let v = if i >= 4 - padding {
                0
            } else {
                value(*byte).ok_or_else(|| {
                    format!(
                        "invalid base64 character '{}' at {}",
                        char::from(*byte).escape_default(),
                        q * 4 + i
                    )
                })?
            };
            bits = (bits << 6) | v;
        }
        let decoded = [(bits >> 16) as u8, (bits >> 8) as u8, bits as u8];
        data.extend_from_slice(&decoded[..3 - padding]);
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_4648_vectors() {
        for (data, text) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(data.as_bytes()), text);
            assert_eq!(decode(text).unwrap(), data.as_bytes());
        }
    }

    #[test]
    fn all_bytes_round_trip() {
        let data: Vec<u8> = (0..=255).chain((0..=255).rev()).collect();
        for len in [0, 1, 2, 3, 254, 255, 511, 512] {
            assert_eq!(decode(&encode(&data[..len])).unwrap(), &data[..len]);
        }
        assert_eq!(encode(&[0xfb, 0xff]), "+/8=");
    }

    #[test]
    fn malformed_text_is_rejected() {
        for (text, expected) in [
            ("Zm9", "multiple of 4"),
            ("Zm9v\n", "multiple of 4"),
            ("Zm9-", "invalid base64 character '-' at 3"),
            ("Z===", "padding"),
            ("Zg==Zm9v", "invalid base64 character '=' at 2"),
        ] {
            let error = decode(text).unwrap_err();
            assert!(error.contains(expected), "{text}: {error}");
        }
    }
}
