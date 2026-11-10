// SPDX-License-Identifier: MIT
//! A decoder of raw LZMA (LZMA1) streams: no header, the properties given
//! by the caller, and the output size known in advance, so that a stream
//! with or without an end marker decodes the same (mitcad#1366: the
//! streams of the mesh containers, [`crate::paramesh`]).
//!
//! The algorithm is the published LZMA one (a range coder over adaptive
//! bit probabilities, literals coded with the byte before them, matches
//! with lengths and distances, four repeated distances and a twelve-state
//! machine), written from its public description.

/// Bits of a probability (`1 << 11` is certainty).
const PROB_BITS: u32 = 11;
/// How fast a probability adapts.
const MOVE_BITS: u32 = 5;
/// A probability's start: one half.
const PROB_INIT: u16 = 1 << (PROB_BITS - 1);
/// The range is normalised (a byte shifted in) below this.
const TOP: u32 = 1 << 24;
/// States of the state machine.
const STATES: usize = 12;
/// The states below this one follow a literal.
const LITERAL_STATES: usize = 7;
/// Position states at most (`pb` up to 4).
const POS_STATES_MAX: usize = 1 << 4;
/// Distance slots below this one code their low bits with probabilities.
const END_POS_MODEL: u32 = 14;
/// Distances below this one are coded without the align bits.
const FULL_DISTANCES: usize = 1 << (END_POS_MODEL >> 1);
/// Low distance bits coded with the align probabilities.
const ALIGN_BITS: u32 = 4;
/// The shortest match.
const MATCH_MIN: usize = 2;
/// The most output bytes reserved before decoding.
const RESERVE_MAX: usize = 16 << 20;

/// The range decoder over the compressed bytes.
struct Range<'a> {
    data: &'a [u8],
    pos: usize,
    range: u32,
    code: u32,
    /// Set when the stream is not a valid one (the decoder goes on, the
    /// caller reports it).
    corrupt: bool,
}

impl<'a> Range<'a> {
    fn new(data: &'a [u8]) -> Result<Self, String> {
        if data.len() < 5 {
            return Err("the LZMA stream is shorter than its range coder's start".to_owned());
        }
        if data[0] != 0 {
            return Err("the LZMA stream does not start with a zero byte".to_owned());
        }
        let code = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
        if code == u32::MAX {
            return Err("the LZMA stream's range coder starts at its end".to_owned());
        }
        Ok(Self {
            data,
            pos: 5,
            range: u32::MAX,
            code,
            corrupt: false,
        })
    }

    /// The next compressed byte; past the end a zero, and the stream is
    /// marked corrupt.
    fn byte(&mut self) -> u8 {
        let b = match self.data.get(self.pos) {
            Some(&b) => b,
            None => {
                self.corrupt = true;
                0
            }
        };
        self.pos += 1;
        b
    }

    fn normalize(&mut self) {
        if self.range < TOP {
            self.range <<= 8;
            self.code = (self.code << 8) | u32::from(self.byte());
        }
    }

    /// `count` bits of equal probability, the highest first.
    fn direct_bits(&mut self, count: u32) -> u32 {
        let mut out = 0u32;
        for _ in 0..count {
            self.range >>= 1;
            self.code = self.code.wrapping_sub(self.range);
            // All ones when the code went below zero (the bit is 0).
            let t = 0u32.wrapping_sub(self.code >> 31);
            self.code = self.code.wrapping_add(self.range & t);
            if self.code == self.range {
                self.corrupt = true;
            }
            self.normalize();
            out = (out << 1).wrapping_add(t.wrapping_add(1));
        }
        out
    }

    /// One bit with the probability `prob` of a zero, which adapts.
    fn bit(&mut self, prob: &mut u16) -> u32 {
        let p = u32::from(*prob);
        let bound = (self.range >> PROB_BITS) * p;
        let bit = if self.code < bound {
            *prob = (p + (((1 << PROB_BITS) - p) >> MOVE_BITS)) as u16;
            self.range = bound;
            0
        } else {
            *prob = (p - (p >> MOVE_BITS)) as u16;
            self.code -= bound;
            self.range -= bound;
            1
        };
        self.normalize();
        bit
    }

    /// A symbol of `count` bits coded by the binary tree `probs` (its
    /// nodes from 1), the highest bit first.
    fn tree(&mut self, probs: &mut [u16], count: u32) -> u32 {
        let mut m = 1usize;
        for _ in 0..count {
            m = (m << 1) + self.bit(&mut probs[m]) as usize;
        }
        (m - (1 << count)) as u32
    }

    /// A symbol of `count` bits coded by the binary tree `probs`, the
    /// lowest bit first.
    fn tree_reverse(&mut self, probs: &mut [u16], count: u32) -> u32 {
        let mut m = 1usize;
        let mut symbol = 0u32;
        for i in 0..count {
            let bit = self.bit(&mut probs[m]);
            m = (m << 1) + bit as usize;
            symbol |= bit << i;
        }
        symbol
    }
}

/// The decoder of match lengths (one for matches, one for repeats).
struct Lengths {
    choice: u16,
    choice2: u16,
    low: [[u16; 8]; POS_STATES_MAX],
    mid: [[u16; 8]; POS_STATES_MAX],
    high: [u16; 256],
}

impl Lengths {
    fn new() -> Self {
        Self {
            choice: PROB_INIT,
            choice2: PROB_INIT,
            low: [[PROB_INIT; 8]; POS_STATES_MAX],
            mid: [[PROB_INIT; 8]; POS_STATES_MAX],
            high: [PROB_INIT; 256],
        }
    }

    /// A length less [`MATCH_MIN`] (0 to 271).
    fn decode(&mut self, rc: &mut Range<'_>, pos_state: usize) -> usize {
        if rc.bit(&mut self.choice) == 0 {
            return rc.tree(&mut self.low[pos_state], 3) as usize;
        }
        if rc.bit(&mut self.choice2) == 0 {
            return 8 + rc.tree(&mut self.mid[pos_state], 3) as usize;
        }
        16 + rc.tree(&mut self.high, 8) as usize
    }
}

/// Decodes a raw LZMA stream of the literal context bits `lc`, literal
/// position bits `lp` and position bits `pb` to exactly `size` bytes. The
/// dictionary size only bounds the distances (the whole output is the
/// dictionary here). An end marker before `size` bytes is an error;
/// whatever follows the `size` bytes is not read. `size` is not trusted
/// for the allocation: the caller caps it, and at most `RESERVE_MAX`
/// bytes are reserved before the stream proves to hold them.
pub fn decompress(
    data: &[u8],
    lc: u32,
    lp: u32,
    pb: u32,
    dictionary: u32,
    size: usize,
) -> Result<Vec<u8>, String> {
    if lc > 8 || lp > 4 || pb > 4 {
        return Err(format!(
            "LZMA properties out of range: lc {lc}, lp {lp}, pb {pb}"
        ));
    }
    if size == 0 {
        return Ok(Vec::new());
    }
    // The range coder's start is checked before the output is allocated,
    // and `size` (the caller's, from the file) reserves at most
    // `RESERVE_MAX` up front: the output grows as it is decoded
    // (mitcad#1604).
    let mut rc = Range::new(data)?;
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(size.min(RESERVE_MAX))
        .map_err(|_| format!("no memory for an LZMA stream of {size} bytes"))?;
    let mut literals = vec![PROB_INIT; 0x300 << (lc + lp)];
    let mut is_match = [PROB_INIT; STATES * POS_STATES_MAX];
    let mut is_rep = [PROB_INIT; STATES];
    let mut is_rep_g0 = [PROB_INIT; STATES];
    let mut is_rep_g1 = [PROB_INIT; STATES];
    let mut is_rep_g2 = [PROB_INIT; STATES];
    let mut is_rep0_long = [PROB_INIT; STATES * POS_STATES_MAX];
    let mut pos_slots = [[PROB_INIT; 64]; 4];
    let mut pos_probs = [PROB_INIT; 1 + FULL_DISTANCES - END_POS_MODEL as usize];
    let mut align = [PROB_INIT; 1 << ALIGN_BITS];
    let mut lengths = Lengths::new();
    let mut rep_lengths = Lengths::new();
    let (pb_mask, lp_mask) = ((1usize << pb) - 1, (1usize << lp) - 1);
    let mut state = 0usize;
    // The four last distances (each one less than the distance back).
    let mut reps = [0usize; 4];
    let window = (dictionary as usize).max(1 << 12);
    while out.len() < size {
        if rc.corrupt {
            return Err(format!(
                "the LZMA stream is corrupt after {} of {size} bytes",
                out.len()
            ));
        }
        let pos_state = out.len() & pb_mask;
        if rc.bit(&mut is_match[(state << 4) + pos_state]) == 0 {
            // A literal, coded with the byte before it (and the byte at the
            // last distance after a match).
            let prev = out.last().copied().unwrap_or(0);
            let context = ((out.len() & lp_mask) << lc) + (usize::from(prev) >> (8 - lc));
            let probs = &mut literals[0x300 * context..0x300 * (context + 1)];
            let mut symbol = 1usize;
            if state >= LITERAL_STATES {
                let mut matched = usize::from(out[out.len() - reps[0] - 1]);
                while symbol < 0x100 {
                    let match_bit = (matched >> 7) & 1;
                    matched <<= 1;
                    let bit = rc.bit(&mut probs[((1 + match_bit) << 8) + symbol]) as usize;
                    symbol = (symbol << 1) | bit;
                    if match_bit != bit {
                        break;
                    }
                }
            }
            while symbol < 0x100 {
                symbol = (symbol << 1) | rc.bit(&mut probs[symbol]) as usize;
            }
            out.push((symbol - 0x100) as u8);
            state = match state {
                0..=3 => 0,
                4..=9 => state - 3,
                _ => state - 6,
            };
            continue;
        }
        let len;
        if rc.bit(&mut is_rep[state]) != 0 {
            if out.is_empty() {
                return Err("the LZMA stream repeats a match before any byte".to_owned());
            }
            if rc.bit(&mut is_rep_g0[state]) == 0 {
                if rc.bit(&mut is_rep0_long[(state << 4) + pos_state]) == 0 {
                    // One byte at the last distance.
                    state = if state < LITERAL_STATES { 9 } else { 11 };
                    out.push(out[out.len() - reps[0] - 1]);
                    continue;
                }
            } else {
                // The second, third or fourth distance moves to the front.
                let distance = if rc.bit(&mut is_rep_g1[state]) == 0 {
                    reps[1]
                } else {
                    let distance = if rc.bit(&mut is_rep_g2[state]) == 0 {
                        reps[2]
                    } else {
                        let fourth = reps[3];
                        reps[3] = reps[2];
                        fourth
                    };
                    reps[2] = reps[1];
                    distance
                };
                reps[1] = reps[0];
                reps[0] = distance;
            }
            len = rep_lengths.decode(&mut rc, pos_state);
            state = if state < LITERAL_STATES { 8 } else { 11 };
        } else {
            reps[3] = reps[2];
            reps[2] = reps[1];
            reps[1] = reps[0];
            len = lengths.decode(&mut rc, pos_state);
            state = if state < LITERAL_STATES { 7 } else { 10 };
            // The distance: its slot by the length, then its low bits.
            let slot = rc.tree(&mut pos_slots[len.min(3)], 6);
            let distance = if slot < 4 {
                slot
            } else {
                let direct = (slot >> 1) - 1;
                let base = (2 | (slot & 1)) << direct;
                if slot < END_POS_MODEL {
                    let at = (base - slot) as usize;
                    base + rc.tree_reverse(&mut pos_probs[at..], direct)
                } else {
                    let high = rc.direct_bits(direct - ALIGN_BITS) << ALIGN_BITS;
                    base.wrapping_add(high)
                        .wrapping_add(rc.tree_reverse(&mut align, ALIGN_BITS))
                }
            };
            if distance == u32::MAX {
                return Err(format!(
                    "the LZMA stream ends after {} of {size} bytes",
                    out.len()
                ));
            }
            reps[0] = distance as usize;
            if reps[0] >= out.len() || reps[0] >= window {
                return Err(format!(
                    "the LZMA stream refers back {} bytes after {} bytes",
                    reps[0] + 1,
                    out.len()
                ));
            }
        }
        // The match: copied byte by byte (it may overlap itself), cut at
        // the size.
        let count = (len + MATCH_MIN).min(size - out.len());
        for _ in 0..count {
            out.push(out[out.len() - reps[0] - 1]);
        }
    }
    if rc.corrupt {
        return Err("the LZMA stream is corrupt at its end".to_owned());
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The bytes of the test streams: a fixed pseudo-random mix of
    /// literals and repeats (the same generator made the compressed bytes
    /// below with a raw LZMA1 encoder, lc 3, lp 0, pb 2, 1 MiB dictionary).
    pub(crate) fn sample() -> Vec<u8> {
        let mut out = Vec::new();
        let mut x: u32 = 12345;
        let mut next = || {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (x >> 16) & 0x7fff
        };
        for i in 0..40u32 {
            out.extend_from_slice(b"mesh corner ");
            out.extend_from_slice(i.to_string().as_bytes());
            for _ in 0..(next() % 7) {
                out.push((next() & 0xff) as u8);
            }
            if i % 5 == 0 {
                let back = out.len().min(1 + (next() as usize % 30));
                let start = out.len() - back;
                for k in 0..(3 + next() as usize % 20) {
                    let b = out[start + k % back];
                    out.push(b);
                }
            }
        }
        out
    }

    /// `sample()` compressed with an end marker.
    const SAMPLE_LZMA: &str = "\
        0036994acd28ea54efd5cca9ed02963ae3d122414750d32fba20827563f7340c\
        a1c75ff270eb7c01d36dfecd1ad61f5eddb60ca064d86d030748574e69aeb7f4\
        7afb9936a83d982352ebbdab82f025f1ff32ed7e4b8c0eaf105ca42f050f80ba\
        88602e497968a66795c05b6bcda87cd08e30c25f5325f843cf48ade554da5ee0\
        17c7a48a34e86d1e712c6d1d0b3bbf83c9842d17ebb4dd651774a4dd77fc945f\
        30ff2d4d8e90f7f63e9a9eaa0883b4df00087c080ec45b87f387b0035115b35e\
        5f18ce72e7935e11398e0b29697b7e6175534eaf0460eef5bdd36ba9c0c2255d\
        b7feaa85d690d396bb02db66223b73d4326f62348dcdaaec88fb98eda60eebbf\
        fff65d3ea0";

    /// [`sample`] compressed.
    pub(crate) fn sample_packed() -> Vec<u8> {
        let digits: Vec<u8> = SAMPLE_LZMA.bytes().filter(u8::is_ascii_hexdigit).collect();
        digits
            .chunks(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn decodes_a_raw_stream_to_its_size() {
        let expected = sample();
        let packed = sample_packed();
        let out = decompress(&packed, 3, 0, 2, 1 << 20, expected.len()).unwrap();
        assert_eq!(out, expected);
        // Its start alone (the stream is read no further than needed).
        let out = decompress(&packed, 3, 0, 2, 1 << 20, 100).unwrap();
        assert_eq!(out, expected[..100]);
    }

    #[test]
    fn refuses_what_is_not_a_stream() {
        let expected = sample();
        let mut packed = sample_packed();
        // More bytes than the stream holds: its end marker comes first.
        assert!(decompress(&packed, 3, 0, 2, 1 << 20, expected.len() + 10).is_err());
        // Cut short.
        packed.truncate(packed.len() / 2);
        assert!(decompress(&packed, 3, 0, 2, 1 << 20, expected.len()).is_err());
        assert!(decompress(&[1, 0, 0, 0, 0], 3, 0, 2, 1 << 20, 4).is_err());
        assert!(decompress(&[0, 0], 3, 0, 2, 1 << 20, 4).is_err());
        assert!(decompress(&[0; 8], 9, 0, 2, 1 << 20, 4).is_err());
        assert_eq!(
            decompress(&[], 3, 0, 2, 1 << 20, 0).unwrap(),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn does_not_allocate_a_declared_size_up_front() {
        // mitcad#1604: a huge size over a payload that is no stream, or a
        // bare range coder start, fails without reserving the size.
        assert!(decompress(&[1, 0, 0, 0, 0], 3, 0, 2, 1 << 20, usize::MAX).is_err());
        assert!(decompress(&[0, 0], 3, 0, 2, 1 << 20, usize::MAX).is_err());
        let error = decompress(&[0, 0, 0, 0, 0], 3, 0, 2, 1 << 20, u32::MAX as usize).unwrap_err();
        assert!(error.contains("corrupt"), "{error}");
    }
}
