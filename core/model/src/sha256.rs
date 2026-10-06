// SPDX-License-Identifier: MIT
//! SHA-256 (FIPS 180-4), which names B-rep files in a project's store by
//! their content (P12a, `crate::file::store`). Own code, like `base64.rs`,
//! to keep the model's dependencies few; checked against the standard's
//! test vectors.

use std::fmt;
use std::str::FromStr;

/// A SHA-256 digest; written as 64 lowercase hexadecimal digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sha256(pub [u8; 32]);

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// One 64-byte block into the state.
fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
        w[i] = u32::from_be_bytes(*word);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choice = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(choice)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

impl Sha256 {
    /// The digest of `data`.
    pub fn of(data: &[u8]) -> Self {
        let mut state = INITIAL;
        let (blocks, rest) = data.as_chunks::<64>();
        for block in blocks {
            compress(&mut state, block);
        }
        // The rest, a 1 bit, zeros and the length in bits: one or two blocks.
        let mut tail = [0u8; 128];
        tail[..rest.len()].copy_from_slice(rest);
        tail[rest.len()] = 0x80;
        let tail_len = if rest.len() < 56 { 64 } else { 128 };
        let bits = (data.len() as u64).wrapping_mul(8);
        tail[tail_len - 8..tail_len].copy_from_slice(&bits.to_be_bytes());
        for block in tail[..tail_len].as_chunks::<64>().0 {
            compress(&mut state, block);
        }
        let mut digest = [0u8; 32];
        for (bytes, word) in digest.as_chunks_mut::<4>().0.iter_mut().zip(state) {
            *bytes = word.to_be_bytes();
        }
        Self(digest)
    }
}

impl fmt::Display for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha256({self})")
    }
}

impl FromStr for Sha256 {
    type Err = String;

    /// 64 lowercase hexadecimal digits, as they are written.
    fn from_str(text: &str) -> Result<Self, String> {
        let bad = || format!("'{text}' is not a SHA-256 (64 lowercase hexadecimal digits)");
        if text.len() != 64 {
            return Err(bad());
        }
        let digit = |c: u8| match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        };
        let mut digest = [0u8; 32];
        for (byte, [high, low]) in digest.iter_mut().zip(text.as_bytes().as_chunks::<2>().0) {
            *byte = digit(*high)
                .zip(digit(*low))
                .map(|(h, l)| h << 4 | l)
                .ok_or_else(bad)?;
        }
        Ok(Self(digest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(data: &[u8]) -> String {
        Sha256::of(data).to_string()
    }

    #[test]
    fn standard_test_vectors() {
        // FIPS 180-4 examples and the NIST test vectors.
        assert_eq!(
            hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            hex(b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"),
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
        );
        assert_eq!(
            hex(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn padding_at_block_edges() {
        // Lengths around the 56-byte boundary need one or two final blocks;
        // a digest of the bytes one at a time must not depend on the split.
        let data: Vec<u8> = (0..200u8).collect();
        let digests: Vec<Sha256> = (54..=66).map(|n| Sha256::of(&data[..n])).collect();
        for (i, a) in digests.iter().enumerate() {
            for b in &digests[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(
            hex(&[b'a'; 55]),
            "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"
        );
        assert_eq!(
            hex(&[b'a'; 56]),
            "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"
        );
        assert_eq!(
            hex(&[b'a'; 64]),
            "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb"
        );
    }

    #[test]
    fn hex_round_trip() {
        let digest = Sha256::of(b"abc");
        let text = digest.to_string();
        assert_eq!(text.parse::<Sha256>(), Ok(digest));
        assert!(text.to_uppercase().parse::<Sha256>().is_err());
        assert!(text[..62].parse::<Sha256>().is_err());
        assert!(format!("{}zz", &text[..62]).parse::<Sha256>().is_err());
        assert_eq!(format!("{digest:?}"), format!("Sha256({text})"));
    }
}
