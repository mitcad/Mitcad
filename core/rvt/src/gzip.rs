// SPDX-License-Identifier: MIT
//! The streams' compression: pages with trailers, and gzip members (RFC
//! 1952) with their CRC-32 and size checked.
//!
//! The large streams (the schema, every `Global/*` stream and every
//! partition) are stored in pages of [`PAGE`] bytes: [`PAGE_PAYLOAD`]
//! bytes of the stream followed by a trailer of [`PAGE_TRAILER`] bytes whose
//! contents are not read; the last page is shorter and has none. With the
//! trailers removed, a stream is a prefix and one or more complete gzip
//! members back to back.

use miniz_oxide::inflate::stream::{InflateState, inflate};
use miniz_oxide::{DataFormat, MZError, MZFlush, MZStatus};

/// A page of a paged stream: payload and trailer.
pub const PAGE: usize = 65_249;
/// The stream's bytes in a page.
pub const PAGE_PAYLOAD: usize = 64_896;
/// The trailer after each full page's payload.
pub const PAGE_TRAILER: usize = PAGE - PAGE_PAYLOAD;

/// The gzip magic and the DEFLATE method.
pub const GZIP_MAGIC: [u8; 3] = [0x1f, 0x8b, 0x08];

/// The largest member inflated (a damaged or hostile length cannot ask
/// for more).
pub const MAX_MEMBER: usize = 1 << 30;

/// The most a stream's members inflate to together (mitcad#924): 64
/// times the stream's stored size and 16 MiB more. Members back to back
/// are each limited by [`MAX_MEMBER`], but a stream of many members of
/// zeros (DEFLATE packs up to 1032 bytes in one) would otherwise make a
/// file of a few megabytes allocate tens of gigabytes. The files' element
/// data inflates to a few times its stored size, so 64 times leaves a
/// wide margin, and any stream of up to 16 MiB inflated reads whatever
/// its ratio.
pub fn stream_limit(stored: usize) -> usize {
    stored.saturating_mul(64).saturating_add(16 << 20)
}

/// Why a member did not inflate.
enum InflateError {
    /// It inflates to more than the bytes allowed.
    TooLarge,
    Bad(String),
}

/// The stream with the page trailers removed.
pub fn strip_pages(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    for page in raw.chunks(PAGE) {
        out.extend_from_slice(&page[..page.len().min(PAGE_PAYLOAD)]);
    }
    out
}

/// The stream with page trailers inserted (zeros), as the files store it:
/// the inverse of [`strip_pages`], for test files.
pub fn add_pages(stream: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(stream.len() + stream.len() / PAGE_PAYLOAD * PAGE_TRAILER);
    let mut chunks = stream.chunks(PAGE_PAYLOAD).peekable();
    while let Some(chunk) = chunks.next() {
        out.extend_from_slice(chunk);
        // Every page but the last has a trailer.
        if chunks.peek().is_some() {
            out.extend(std::iter::repeat_n(0u8, PAGE_TRAILER));
        }
    }
    out
}

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = crc_table();

/// The CRC-32 of gzip (ISO 3309, as RFC 1952 gives it).
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = CRC_TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

/// One gzip member inflated.
#[derive(Debug, Clone)]
pub struct Member {
    /// Where its header starts in the (page-stripped) stream.
    pub offset: usize,
    /// Where it ends (after the CRC-32 and the size).
    pub end: usize,
    pub data: Vec<u8>,
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

impl From<String> for InflateError {
    fn from(why: String) -> Self {
        InflateError::Bad(why)
    }
}

/// The gzip member whose header starts at `at`: inflated, its CRC-32 and
/// size checked.
pub fn member_at(data: &[u8], at: usize) -> Result<Member, String> {
    member_within(data, at, MAX_MEMBER).map_err(|e| match e {
        InflateError::TooLarge => {
            format!("gzip member at {at:#x}: inflates to more than {MAX_MEMBER} bytes")
        }
        InflateError::Bad(why) => why,
    })
}

/// The member at `at` of a stream whose members inflate to `limit` bytes
/// at most ([`stream_limit`]), `so_far` of them inflated before it.
fn member_in_stream(data: &[u8], at: usize, limit: usize, so_far: usize) -> Result<Member, String> {
    let left = limit.saturating_sub(so_far);
    member_within(data, at, left.min(MAX_MEMBER)).map_err(|e| match e {
        InflateError::TooLarge if left < MAX_MEMBER => {
            format!("the stream inflates to more than {limit} bytes")
        }
        InflateError::TooLarge => {
            format!("gzip member at {at:#x}: inflates to more than {MAX_MEMBER} bytes")
        }
        InflateError::Bad(why) => why,
    })
}

/// [`member_at`], the member inflating to `max` bytes at most.
fn member_within(data: &[u8], at: usize, max: usize) -> Result<Member, InflateError> {
    if data.get(at..at + 3) != Some(&GZIP_MAGIC[..]) {
        return Err(format!("no gzip member at {at:#x}").into());
    }
    let short = || format!("the gzip header at {at:#x} is cut");
    let flags = *data.get(at + 3).ok_or_else(short)?;
    let mut p = at + 10;
    if flags & 0x04 != 0 {
        p += 2 + usize::from(u16_at(data, p).ok_or_else(short)?);
    }
    for bit in [0x08, 0x10] {
        if flags & bit != 0 {
            let rest = data.get(p..).ok_or_else(short)?;
            p += rest.iter().position(|&b| b == 0).ok_or_else(short)? + 1;
        }
    }
    if flags & 0x02 != 0 {
        p += 2;
    }
    let input = data.get(p..).ok_or_else(short)?;
    let (out, consumed) = inflate_within(input, max).map_err(|e| match e {
        InflateError::TooLarge => InflateError::TooLarge,
        InflateError::Bad(why) => InflateError::Bad(format!("gzip member at {at:#x}: {why}")),
    })?;
    let end = p + consumed;
    let crc = u32_at(data, end).ok_or_else(|| format!("gzip member at {at:#x} has no CRC-32"))?;
    let size =
        u32_at(data, end + 4).ok_or_else(|| format!("gzip member at {at:#x} has no size"))?;
    if crc != crc32(&out) {
        return Err(InflateError::Bad(format!(
            "gzip member at {at:#x}: CRC-32 {crc:#010x} does not match the data"
        )));
    }
    if size != out.len() as u32 {
        return Err(InflateError::Bad(format!(
            "gzip member at {at:#x}: size {size} does not match the {} bytes inflated",
            out.len()
        )));
    }
    Ok(Member {
        offset: at,
        end: end + 8,
        data: out,
    })
}

/// Raw DEFLATE data inflated, and the input bytes it took.
pub fn inflate_raw(input: &[u8]) -> Result<(Vec<u8>, usize), String> {
    inflate_within(input, MAX_MEMBER).map_err(|e| match e {
        InflateError::TooLarge => format!("inflates to more than {MAX_MEMBER} bytes"),
        InflateError::Bad(why) => why,
    })
}

/// [`inflate_raw`] to `max` bytes at most.
fn inflate_within(input: &[u8], max: usize) -> Result<(Vec<u8>, usize), InflateError> {
    // Room for one byte more tells data of exactly `max` bytes from more.
    let room = max.saturating_add(1);
    let mut state = InflateState::new_boxed(DataFormat::Raw);
    let mut out = vec![0u8; input.len().saturating_mul(4).clamp(1024, 1 << 20).min(room)];
    let (mut read, mut written) = (0usize, 0usize);
    loop {
        if written == out.len() {
            if out.len() >= room {
                return Err(InflateError::TooLarge);
            }
            out.resize(out.len().saturating_mul(2).min(room), 0);
        }
        let result = inflate(
            &mut state,
            &input[read..],
            &mut out[written..],
            MZFlush::None,
        );
        read += result.bytes_consumed;
        written += result.bytes_written;
        match result.status {
            Ok(MZStatus::StreamEnd) => break,
            Ok(_) => {
                if result.bytes_consumed == 0 && result.bytes_written == 0 && written < out.len() {
                    return Err(InflateError::Bad("the DEFLATE data ends early".into()));
                }
            }
            Err(MZError::Buf) if written == out.len() => {}
            Err(e) => return Err(InflateError::Bad(format!("bad DEFLATE data ({e:?})"))),
        }
    }
    if written > max {
        return Err(InflateError::TooLarge);
    }
    out.truncate(written);
    Ok((out, read))
}

/// The gzip members from `start` on: each read where the previous one
/// ended, or at the next magic after it; together inflating to the
/// stream's [`stream_limit`] at most.
pub fn members_from(data: &[u8], start: usize) -> Result<Vec<Member>, String> {
    members_from_limited(data, start, stream_limit(data.len()))
}

/// [`members_from`], the members together inflating to `limit` bytes at
/// most.
pub fn members_from_limited(
    data: &[u8],
    start: usize,
    limit: usize,
) -> Result<Vec<Member>, String> {
    let mut out = Vec::new();
    let mut at = start;
    let mut inflated = 0usize;
    while let Some(next) = find_magic(data, at) {
        let member = member_in_stream(data, next, limit, inflated)?;
        inflated += member.data.len();
        at = member.end;
        out.push(member);
    }
    if out.is_empty() {
        return Err(format!("no gzip member after {start:#x}"));
    }
    Ok(out)
}

/// The next gzip magic at or after `from`.
pub fn find_magic(data: &[u8], from: usize) -> Option<usize> {
    data.get(from..)?
        .windows(3)
        .position(|w| w == GZIP_MAGIC)
        .map(|p| p + from)
}

/// Members inflated one at a time, on demand: a large partition is read
/// only as far as its records go.
pub struct Members<'a> {
    data: &'a [u8],
    at: usize,
    /// The inflated bytes so far.
    pub inflated: Vec<u8>,
    /// Members read.
    pub count: usize,
    /// The most the members inflate to together ([`stream_limit`]).
    limit: usize,
}

impl<'a> Members<'a> {
    pub fn new(data: &'a [u8], start: usize) -> Self {
        Members {
            data,
            at: start,
            inflated: Vec::new(),
            count: 0,
            limit: stream_limit(data.len()),
        }
    }

    /// The members inflating to `limit` bytes at most together.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Inflates members until at least `len` bytes are there; false when
    /// the stream ends first; an error when the members inflate to more
    /// than the limit.
    pub fn ensure(&mut self, len: usize) -> Result<bool, String> {
        while self.inflated.len() < len {
            let Some(next) = find_magic(self.data, self.at) else {
                return Ok(false);
            };
            let member = member_in_stream(self.data, next, self.limit, self.inflated.len())?;
            self.at = member.end;
            self.inflated.extend_from_slice(&member.data);
            self.count += 1;
        }
        Ok(true)
    }

    /// Every remaining member inflated.
    pub fn all(&mut self) -> Result<(), String> {
        self.ensure(usize::MAX).map(|_| ())
    }

    /// Whether every member was read.
    pub fn finished(&self) -> bool {
        find_magic(self.data, self.at).is_none()
    }
}

/// A gzip member of `data` (no file name, the header's OS byte 0x0b as the
/// files have it), for test files.
pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 0x08, 0, 0, 0, 0, 0, 0, 0x0b];
    out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec(data, 6));
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes that do not compress (xorshift).
    fn noise(n: usize) -> Vec<u8> {
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 32) as u8
            })
            .collect()
    }

    #[test]
    fn crc_of_the_standard_check_string() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn members_round_trip_through_pages() {
        let a = noise(200_000);
        let b = b"second member".to_vec();
        let mut stream = vec![9u8, 0, 0, 0, 0, 0, 0, 0];
        stream.extend(gzip(&a));
        stream.extend(gzip(&b));
        let paged = add_pages(&stream);
        assert!(paged.len() > stream.len());
        assert_eq!(strip_pages(&paged), stream);
        let members = members_from(&strip_pages(&paged), 8).unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].data, a);
        assert_eq!(members[1].data, b);
        let mut lazy = Members::new(&stream, 8);
        assert!(lazy.ensure(10).unwrap());
        assert_eq!(lazy.count, 1);
        lazy.all().unwrap();
        assert_eq!(lazy.count, 2);
        assert!(lazy.finished());
        assert!(!lazy.ensure(usize::MAX).unwrap());
    }

    #[test]
    fn a_stream_inflates_to_its_limit_at_most() {
        // mitcad#924: a paged stream of four members of 64 MiB of zeros
        // under a total limit of 128 MiB.
        const MIB: usize = 1 << 20;
        let member = gzip(&vec![0u8; 64 * MIB]);
        let mut stream = vec![9u8, 0, 0, 0, 0, 0, 0, 0];
        for _ in 0..4 {
            stream.extend(&member);
        }
        let stream = strip_pages(&add_pages(&stream));
        let e = members_from_limited(&stream, 8, 128 * MIB).unwrap_err();
        assert!(e.contains(&(128 * MIB).to_string()), "{e}");
        let mut lazy = Members::new(&stream, 8).with_limit(128 * MIB);
        assert!(lazy.ensure(128 * MIB).unwrap());
        let e = lazy.all().unwrap_err();
        assert!(e.contains(&(128 * MIB).to_string()), "{e}");
        drop(lazy);
        // Within the limit (exactly at it) the members read.
        let members = members_from_limited(&stream[..8 + 2 * member.len()], 8, 128 * MIB);
        assert_eq!(members.unwrap().len(), 2);
        // The limit by default: 64 times the stored bytes and 16 MiB.
        assert_eq!(stream_limit(1 << 20), 80 * MIB);
    }

    #[test]
    fn damage_is_an_error() {
        let mut member = gzip(b"payload of a member");
        let crc = member.len() - 8;
        member[crc] ^= 1;
        assert!(member_at(&member, 0).unwrap_err().contains("CRC-32"));
        let good = gzip(b"payload of a member");
        assert!(member_at(&good[..good.len() - 6], 0).is_err());
        assert!(member_at(b"not gzip", 0).is_err());
        // The page trailers left in: the member does not inflate or check.
        let long = noise(300_000);
        let paged = add_pages(&gzip(&long));
        assert!(member_at(&paged, 0).is_err());
    }
}
