// SPDX-License-Identifier: MIT
//! The compression of the R2004 format and its successors (the
//! specification's 4.7): an LZ77 variant of literal runs and copies from
//! the output written so far. A compressor of the same opcodes writes the
//! test files.

use crate::DwgError;

fn corrupt(msg: &str) -> DwgError {
    DwgError::Corrupt(format!("compressed data: {msg}"))
}

struct Input<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Input<'_> {
    fn byte(&mut self) -> Result<u8, DwgError> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or_else(|| corrupt("ends early"))?;
        self.pos += 1;
        Ok(b)
    }

    /// A literal length; `Err` of the byte when it is an opcode instead
    /// (any bit of its high nibble set: no literals).
    fn literal_length(&mut self) -> Result<Result<usize, u8>, DwgError> {
        let b = self.byte()?;
        if b & 0xf0 != 0 {
            return Ok(Err(b));
        }
        if b != 0 {
            return Ok(Ok(usize::from(b) + 3));
        }
        let mut total = 0x0f;
        loop {
            let b = self.byte()?;
            if b == 0 {
                total += 0xff;
            } else {
                total += usize::from(b);
                return Ok(Ok(total + 3));
            }
        }
    }

    fn long_count(&mut self) -> Result<usize, DwgError> {
        let b = self.byte()?;
        if b != 0 {
            return Ok(usize::from(b));
        }
        let mut total = 0xff;
        loop {
            let b = self.byte()?;
            if b == 0 {
                total += 0xff;
            } else {
                return Ok(total + usize::from(b));
            }
        }
    }

    /// The two-byte offset and its literal count.
    fn two_byte_offset(&mut self) -> Result<(usize, usize), DwgError> {
        let first = self.byte()?;
        let second = self.byte()?;
        Ok((
            usize::from(first >> 2) | usize::from(second) << 6,
            usize::from(first & 3),
        ))
    }
}

/// Decompresses `data` into at most `size` bytes (more output is an error).
pub fn decompress(data: &[u8], size: usize) -> Result<Vec<u8>, DwgError> {
    // Sized from the data, not from `size` (a field of the file, up to
    // 4 GiB): the output grows as the data gives it.
    let mut out: Vec<u8> = Vec::with_capacity(size.min(data.len().saturating_mul(4)));
    let mut input = Input { data, pos: 0 };
    let mut pending = None;
    let copy_literals = |out: &mut Vec<u8>, input: &mut Input, n: usize| -> Result<(), DwgError> {
        let end = input.pos + n;
        let bytes = input
            .data
            .get(input.pos..end)
            .ok_or_else(|| corrupt("literals run past the end"))?;
        if out.len() + n > size {
            return Err(corrupt("more output than the page holds"));
        }
        out.extend_from_slice(bytes);
        input.pos = end;
        Ok(())
    };
    match input.literal_length()? {
        Ok(n) => copy_literals(&mut out, &mut input, n)?,
        Err(op) => pending = Some(op),
    }
    loop {
        let op1 = match pending.take() {
            Some(op) => op,
            None => {
                if input.pos >= data.len() {
                    break;
                }
                input.byte()?
            }
        };
        let (count, offset, literals) = match op1 {
            0x11 => break,
            0x10 => {
                let count = input.long_count()? + 9;
                let (offset, lit) = input.two_byte_offset()?;
                (count, offset + 0x3fff, lit)
            }
            0x12..=0x1f => {
                let count = usize::from(op1 & 0x0f) + 2;
                let (offset, lit) = input.two_byte_offset()?;
                (count, offset + 0x3fff, lit)
            }
            0x20 => {
                let count = input.long_count()? + 0x21;
                let (offset, lit) = input.two_byte_offset()?;
                (count, offset, lit)
            }
            0x21..=0x3f => {
                let count = usize::from(op1) - 0x1e;
                let (offset, lit) = input.two_byte_offset()?;
                (count, offset, lit)
            }
            0x40..=0xff => {
                let count = usize::from(op1 >> 4) - 1;
                let op2 = input.byte()?;
                let offset = usize::from(op2) << 2 | usize::from(op1 & 0x0c) >> 2;
                (count, offset, usize::from(op1 & 3))
            }
            _ => return Err(corrupt(&format!("opcode {op1:#04x}"))),
        };
        // The offset counts back from the last byte written.
        let distance = offset + 1;
        if distance > out.len() {
            return Err(corrupt("a copy reaches before the start"));
        }
        if out.len() + count > size {
            return Err(corrupt("more output than the page holds"));
        }
        let start = out.len() - distance;
        for i in 0..count {
            let b = out[start + i];
            out.push(b);
        }
        let literals = if literals == 0 {
            match input.literal_length()? {
                Ok(n) => n,
                Err(op) => {
                    pending = Some(op);
                    0
                }
            }
        } else {
            literals
        };
        copy_literals(&mut out, &mut input, literals)?;
    }
    Ok(out)
}

fn push_literal_length(out: &mut Vec<u8>, n: usize) {
    debug_assert!(n >= 4);
    if n <= 0x12 {
        out.push((n - 3) as u8);
        return;
    }
    out.push(0);
    let mut r = n - 3 - 0x0f;
    while r > 0xff {
        out.push(0);
        r -= 0xff;
    }
    out.push(r as u8);
}

fn push_long_count(out: &mut Vec<u8>, v: usize) {
    debug_assert!(v >= 1);
    if v <= 0xff {
        out.push(v as u8);
        return;
    }
    out.push(0);
    let mut r = v - 0xff;
    while r > 0xff {
        out.push(0);
        r -= 0xff;
    }
    out.push(r as u8);
}

/// One copy of the compressed stream.
struct Copy {
    count: usize,
    /// Distance back from the last byte written, less one.
    offset: usize,
}

/// Writes the copy's opcode with the literal count that follows it (in the
/// opcode when 1 to 3, else 0, with a literal length after it when more).
fn push_copy(out: &mut Vec<u8>, copy: &Copy, literals: usize) {
    let code = if (1..=3).contains(&literals) {
        literals as u8
    } else {
        0
    };
    let s = copy.offset;
    if (3..=14).contains(&copy.count) && s <= 0x3ff {
        out.push(((copy.count + 1) << 4) as u8 | ((s & 3) << 2) as u8 | code);
        out.push((s >> 2) as u8);
    } else if s <= 0x3fff {
        if copy.count <= 0x21 {
            out.push((copy.count + 0x1e) as u8);
        } else {
            out.push(0x20);
            push_long_count(out, copy.count - 0x21);
        }
        out.push(((s & 0x3f) << 2) as u8 | code);
        out.push((s >> 6) as u8);
    } else {
        let s = s - 0x3fff;
        if copy.count <= 17 {
            out.push((copy.count - 2) as u8 | 0x10);
        } else {
            out.push(0x10);
            push_long_count(out, copy.count - 9);
        }
        out.push(((s & 0x3f) << 2) as u8 | code);
        out.push((s >> 6) as u8);
    }
    if literals > 3 {
        push_literal_length(out, literals);
    }
}

/// Compresses `data` (greedy matches over a hash of three bytes).
pub fn compress(data: &[u8]) -> Vec<u8> {
    const MAX_DISTANCE: usize = 0x3fff + 0x3fff + 1;
    let mut out = Vec::new();
    let mut heads: std::collections::HashMap<[u8; 3], usize> = std::collections::HashMap::new();
    let mut i = 0;
    let mut literal_start = 0;
    let mut last_copy: Option<Copy> = None;
    let flush = |out: &mut Vec<u8>, last: Option<Copy>, lits: &[u8]| {
        match last {
            None => {
                if !lits.is_empty() {
                    // A run of 1 to 3 literals at the start is written as 4 or
                    // more by the caller (see below).
                    push_literal_length(out, lits.len());
                }
            }
            Some(c) => push_copy(out, &c, lits.len()),
        }
        out.extend_from_slice(lits);
    };
    while i + 3 <= data.len() {
        let key = [data[i], data[i + 1], data[i + 2]];
        let found = heads.insert(key, i);
        let copy = found.and_then(|j| {
            let distance = i - j;
            if distance == 0 || distance > MAX_DISTANCE {
                return None;
            }
            let mut n = 0;
            while i + n < data.len() && data[j + n] == data[i + n] && n < 0x1000 {
                n += 1;
            }
            let far = distance - 1 > 0x3fff;
            // A copy needs at least three bytes, four from far away; the
            // first literal run must be none or at least four bytes.
            let first = last_copy.is_none() && (1..4).contains(&(i - literal_start));
            (n >= 3 && !(far && n < 4) && !first).then_some(Copy {
                count: n,
                offset: distance - 1,
            })
        });
        match copy {
            Some(c) => {
                let count = c.count;
                flush(&mut out, last_copy.take(), &data[literal_start..i]);
                last_copy = Some(c);
                for k in i + 1..(i + count).min(data.len().saturating_sub(2)) {
                    heads.insert([data[k], data[k + 1], data[k + 2]], k);
                }
                i += count;
                literal_start = i;
            }
            None => i += 1,
        }
    }
    let rest = &data[literal_start..];
    if last_copy.is_none() && (1..4).contains(&rest.len()) {
        // Data of one to three bytes: a literal length counts at least
        // four, so the run is padded with zeros (the reader takes the
        // section's size).
        push_literal_length(&mut out, 4);
        out.extend_from_slice(rest);
        out.resize(out.len() + 4 - rest.len(), 0);
    } else {
        flush(&mut out, last_copy, rest);
    }
    out.push(0x11);
    out.push(0);
    out.push(0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_lengths_of_the_specification() {
        let mut i = Input {
            data: &[0x05, 0x00, 0x02, 0x40],
            pos: 0,
        };
        assert_eq!(i.literal_length().unwrap(), Ok(8));
        assert_eq!(i.literal_length().unwrap(), Ok(0x14));
        assert_eq!(i.literal_length().unwrap(), Err(0x40));
        let mut i = Input {
            data: &[0xdd, 0x00, 0x00, 0x34],
            pos: 0,
        };
        assert_eq!(i.long_count().unwrap(), 0xdd);
        assert_eq!(i.long_count().unwrap(), 0x232);
    }

    #[test]
    fn a_page_size_the_file_gives_allocates_nothing_up_front() {
        // The output was pre-sized from the size (up to 4 GiB from a
        // field; usize::MAX overflowed the capacity, mitcad#931).
        let packed = compress(b"abcdefgh");
        assert_eq!(decompress(&packed, usize::MAX).unwrap(), b"abcdefgh");
    }

    #[test]
    fn round_trips() {
        let mut long = Vec::new();
        for i in 0..40000u32 {
            long.extend_from_slice(&(i % 97).to_le_bytes());
            if i % 13 == 0 {
                long.extend_from_slice(b"repeated text repeated text");
            }
        }
        let mut far = vec![7u8; 10];
        far.extend((0..0x5000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8));
        far.extend_from_within(10..300);
        for data in [
            b"".to_vec(),
            b"ab".to_vec(),
            b"abcabcabcabcabcabc".to_vec(),
            vec![0u8; 1000],
            long,
            far,
        ] {
            let packed = compress(&data);
            let size = data.len().max(4);
            let mut unpacked = decompress(&packed, size).unwrap();
            unpacked.truncate(data.len());
            assert_eq!(unpacked, data);
        }
    }
}
