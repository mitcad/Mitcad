// SPDX-License-Identifier: MIT
//! 128-bit fingerprints (FNV-1a, 128 bits) of what feature evaluations are
//! given (P7d): a feature definition, the values and versions it read. The
//! recompute derives the version of every result from them, so the same
//! definition with the same inputs has the same version in every process,
//! and results stored on disk can be found again.
//!
//! Every value is written with its length or in a fixed size, so two
//! different sequences of values do not give the same bytes. A fingerprint
//! is not a cryptographic hash: it tells apart the inputs a document
//! produces, not inputs made to collide.

use std::io;

/// FNV-1a's 128-bit offset basis and prime.
const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

/// A fingerprint being written; [`io::Write`] takes serialized values
/// (`serde_json::to_writer`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fingerprint(u128);

impl Default for Fingerprint {
    fn default() -> Self {
        Self(OFFSET)
    }
}

impl Fingerprint {
    /// A fingerprint that starts with `tag`, which names what it is of.
    pub fn new(tag: &str) -> Self {
        let mut fingerprint = Self::default();
        fingerprint.str(tag);
        fingerprint
    }

    /// Bytes as they come; a caller that writes data of varying length
    /// writes its length first ([`Fingerprint::str`], [`Fingerprint::data`]).
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        let mut hash = self.0;
        for byte in bytes {
            hash ^= u128::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
        self.0 = hash;
        self
    }

    /// Data with its length.
    pub fn data(&mut self, bytes: &[u8]) -> &mut Self {
        self.u64(bytes.len() as u64).bytes(bytes)
    }

    pub fn str(&mut self, text: &str) -> &mut Self {
        self.data(text.as_bytes())
    }

    pub fn u8(&mut self, value: u8) -> &mut Self {
        self.bytes(&[value])
    }

    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    pub fn u64(&mut self, value: u64) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    pub fn u128(&mut self, value: u128) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    /// The bits of a number (so -0.0 and 0.0 differ, as in the cache).
    pub fn f64(&mut self, value: f64) -> &mut Self {
        self.u64(value.to_bits())
    }

    pub fn bool(&mut self, value: bool) -> &mut Self {
        self.u8(u8::from(value))
    }

    /// None and Some differ from every value.
    pub fn option<T>(&mut self, value: Option<T>, write: impl FnOnce(&mut Self, T)) -> &mut Self {
        match value {
            None => self.u8(0),
            Some(value) => {
                self.u8(1);
                write(self, value);
                self
            }
        }
    }

    pub fn finish(&self) -> u128 {
        self.0
    }
}

impl io::Write for Fingerprint {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The fingerprint of data, for keys of data of its own (B-rep data).
pub(crate) fn of_bytes(tag: &str, bytes: &[u8]) -> u128 {
    Fingerprint::new(tag).data(bytes).finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_128_of_known_input() {
        // The FNV-1a 128 of "a" and of "" (the offset basis).
        assert_eq!(Fingerprint::default().finish(), OFFSET);
        assert_eq!(
            Fingerprint::default().bytes(b"a").finish(),
            0xd228_cb69_6f1a_8caf_7891_2b70_4e4a_8964_u128
        );
    }

    #[test]
    fn lengths_keep_values_apart() {
        let ab_c = Fingerprint::new("t").str("ab").str("c").finish();
        let a_bc = Fingerprint::new("t").str("a").str("bc").finish();
        assert_ne!(ab_c, a_bc);
        let option = |value: Option<u8>| {
            Fingerprint::new("t")
                .option(value, |f, v| {
                    f.u8(v);
                })
                .finish()
        };
        assert_ne!(option(None), option(Some(0)));
        assert_ne!(
            Fingerprint::new("t").f64(0.0).finish(),
            Fingerprint::new("t").f64(-0.0).finish()
        );
    }
}
