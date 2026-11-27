// SPDX-License-Identifier: MIT
//! What the data object of a pattern or mirror stores besides its inputs
//! (mitcad#186): the compute option, the symmetry flags, the distance type
//! and the suppressed elements *(read from one-option variants of each:
//! the bytes that differ between the siblings; checked on the corpora's
//! patterns and mirrors, where the layout parses in all but a few)*.
//!
//! The item's first reference after its root part is the data object
//! ([`CIRCULAR_PATTERN_DATA`], [`RECTANGULAR_PATTERN_DATA`],
//! [`MIRROR_DATA`]). Its tail is, for all three,
//! `u32 compute | u32 kind | i32 -1`, with `compute` 0 optimized, 1
//! identical, 2 adjust and `kind` 0 faces, 1 features, 2 bodies (the
//! pattern's object type; the decoder's own object reading agrees in
//! every pattern and mirror of the corpora). Before it come `u32 n | n`
//! body records ([`BODY_RECORD`]) and 4 or 8 bytes the layouts differ in;
//! the oldest layout of the corpora has neither the gap nor the `-1`.
//!
//! - Circular pattern: after the root part a byte, 1 for a symmetric
//!   pattern; a parameter holder and a direction input; the tail; a byte
//!   (1: the total angle); `u32 m | m` pairs `u64 element number, u64
//!   id`; `u32 e | e` element references ([`PATTERN_ELEMENT`]); the
//!   quantity's holder, the item and its input list; `u32 s | s` `u32`
//!   suppressed element numbers. The numbers count the original as 1: the
//!   suppressed ids of the design's API are one less, and the model's
//!   element indices (the original 0) too. Only one design of the
//!   learning set suppresses an element, so that mapping rests on it.
//! - Rectangular pattern: the two distances' parameter holders, then one
//!   byte each for the symmetry in direction one and two, the quantity
//!   holder, the direction inputs (a null byte instead of the second
//!   one for a single direction), the body records, the tail, a byte (0
//!   spacing, 1 extent), the pairs, the elements, three references and
//!   the suppressed numbers. The element numbers of a rectangular
//!   pattern follow no order the variants explain (a one-direction
//!   pattern lists the second direction's defaults too), so only an empty
//!   list is read.
//! - Mirror: after the root part a byte, 1 when the bodies are joined with
//!   their images (`isCombine`; 0 for a mirror of features, whatever the
//!   bodies it makes), and the tail. The mirrors of the variants all
//!   adjust; the compute option of a mirror shares the patterns' layout
//!   and agrees with the original application's dump of 13 real mirrors
//!   (11 adjust, 2 identical).

use super::super::classes::*;
use super::super::stream::{Segment, f64s_at, u32_at};

/// The data object of a circular pattern.
pub const CIRCULAR_PATTERN_DATA: &str = "7493A99D-EDFC-4932-8B75-4120A8FA7875";
/// The data object of a rectangular pattern.
pub const RECTANGULAR_PATTERN_DATA: &str = "EA0E146E-3126-42F0-9280-0DE74AB17D78";
/// The data object of a mirror.
pub const MIRROR_DATA: &str = "C444E6AD-33DC-4877-B037-A3A5518856B1";
/// One element of a pattern.
pub const PATTERN_ELEMENT: &str = "AD719C1A-11AF-4537-BB41-030A9091D36B";
/// The data object of a path pattern (mitcad#468).
pub const PATH_PATTERN_DATA: &str = "BE1FB171-D98A-4A4D-99C2-3738627CF79C";

/// The settings that were read; a setting the layout does not give is
/// `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PatternSettings {
    /// A circular pattern's `isSymmetric`.
    pub symmetric: Option<bool>,
    /// A rectangular pattern's symmetry in directions one and two.
    pub symmetric_one: Option<bool>,
    pub symmetric_two: Option<bool>,
    /// `patternComputeOption`.
    pub compute: Option<&'static str>,
    /// Whether the distance is the pattern's extent (else the spacing).
    pub extent: Option<bool>,
    /// The suppressed elements as the design's API numbers them.
    pub suppressed: Option<Vec<i64>>,
    /// Whether the pattern has a second direction.
    pub two_directions: Option<bool>,
    /// A mirror's `isCombine`.
    pub combine: Option<bool>,
}

fn compute_name(code: u32) -> Option<&'static str> {
    match code {
        0 => Some("OptimizedPatternCompute"),
        1 => Some("IdenticalPatternCompute"),
        2 => Some("AdjustPatternCompute"),
        _ => None,
    }
}

/// A reference found in a data object: where it starts and ends and its
/// class.
struct Located<'a> {
    at: usize,
    end: usize,
    class: &'a str,
}

fn located<'a>(seg: &'a Segment, d: &[u8]) -> Vec<Located<'a>> {
    seg.refs_in(d, 0, d.len())
        .into_iter()
        .map(|(at, r)| Located {
            at,
            end: r.end,
            class: seg.guid_of(r.id).unwrap_or(""),
        })
        .collect()
}

/// The data object of an item: its first reference after the root part,
/// when it is of class `class`.
fn data_object<'a>(seg: &'a Segment, item: &[u8], class: &str) -> Option<&'a [u8]> {
    let p = seg.root_part(item)?.end;
    let r = seg.ref_at(item, p)?;
    (seg.guid_of(r.id) == Some(class)).then(|| seg.data_of(r.id))
}

/// The tail `u32 compute | u32 kind | i32 -1` from `from`, the end of the
/// body records, with 4 or 8 bytes between them: `(position of the -1,
/// compute code)`. The oldest layout has no gap and no `-1`: then the
/// position is where the `-1` would be (the kind's start), so that the
/// byte after the tail is at 4 bytes from it, and the caller must check
/// what follows (`legacy`).
fn tail(d: &[u8], from: usize) -> Option<(usize, u32, bool)> {
    let found = [from + 12, from + 16]
        .into_iter()
        .find(|&m| d.get(m..m + 4) == Some(&[0xff; 4][..]));
    let (m, legacy) = match found {
        Some(m) => (m, false),
        None => (from + 4, true),
    };
    let start = if legacy { from } else { m - 8 };
    let compute = u32_at(d, start).filter(|&c| c <= 2)?;
    u32_at(d, start + 4).filter(|&k| k <= 2)?;
    Some((m, compute, legacy))
}

/// Skips `n` body records ([`BODY_RECORD`]) that follow at `from`: the
/// index of the next reference, and the position after the records.
fn body_records(d: &[u8], refs: &[Located], mut i: usize, from: usize) -> Option<(usize, usize)> {
    let n = u32_at(d, from).filter(|&n| n <= 64)? as usize;
    let mut p = from + 4;
    for _ in 0..n {
        let r = refs
            .get(i)
            .filter(|r| r.at == p && r.class == BODY_RECORD)?;
        p = r.end;
        i += 1;
    }
    Some((i, p))
}

/// After the tail's `-1` at `m`: a byte, `u32 m | m` pairs, `u32 e | e`
/// elements, the three references that end the object's lists (the
/// quantity's holder, the item, its input list) and `u32 s | s` suppressed
/// element numbers; the byte and the numbers.
fn after_tail(d: &[u8], refs: &[Located], i: usize, m: usize) -> Option<(u8, Vec<u32>)> {
    let byte = *d.get(m + 4)?;
    let pairs = u32_at(d, m + 5).filter(|&n| n <= 100_000)? as usize;
    let mut p = m + 9 + 16 * pairs;
    let elements = u32_at(d, p).filter(|&n| n <= 100_000)? as usize;
    p += 4;
    let mut i = i;
    for _ in 0..elements {
        let r = refs
            .get(i)
            .filter(|r| r.at == p && r.class == PATTERN_ELEMENT)?;
        p = r.end;
        i += 1;
    }
    let rest = refs.get(i..)?;
    if rest.len() != 3
        || rest[0].at != p
        || rest[0].class != INT_HOLDER
        || rest[2].class != BODY_INPUT
    {
        return None;
    }
    p = rest[2].end;
    let n = u32_at(d, p).filter(|&n| n <= 10_000)? as usize;
    if p + 4 + 4 * n != d.len() {
        return None;
    }
    let numbers = (0..n)
        .map(|k| u32_at(d, p + 4 + 4 * k))
        .collect::<Option<_>>()?;
    Some((byte, numbers))
}

/// A circular pattern's settings; `item` is the item's object.
pub(super) fn circular(seg: &Segment, item: &[u8]) -> PatternSettings {
    let mut out = PatternSettings::default();
    let Some(d) = data_object(seg, item, CIRCULAR_PATTERN_DATA) else {
        return out;
    };
    let Some(root) = seg.root_part(d).map(|r| r.end) else {
        return out;
    };
    let refs = located(seg, d);
    let Some(i) = refs.iter().position(|r| r.class == DIRECTION_INPUT) else {
        return out;
    };
    let Some((i, p)) = body_records(d, &refs, i + 1, refs[i].end) else {
        return out;
    };
    let Some((m, compute, legacy)) = tail(d, p) else {
        return out;
    };
    let rest = after_tail(d, &refs, i, m);
    if legacy && rest.is_none() {
        return out;
    }
    out.symmetric = d.get(root).copied().filter(|&b| b <= 1).map(|b| b == 1);
    out.compute = compute_name(compute);
    if let Some((_, numbers)) = rest {
        // The numbers count the original as 1; 0 names no element.
        out.suppressed = numbers
            .iter()
            .map(|&n| n.checked_sub(1).map(i64::from))
            .collect();
    }
    out
}

/// A rectangular pattern's settings; `item` is the item's object.
pub(super) fn rectangular(seg: &Segment, item: &[u8]) -> PatternSettings {
    let mut out = PatternSettings::default();
    let Some(d) = data_object(seg, item, RECTANGULAR_PATTERN_DATA) else {
        return out;
    };
    let refs = located(seg, d);
    // The two distances' holders, two flag bytes, the second quantity's
    // holder and the direction inputs.
    let Some(i) = refs.iter().position(|r| r.class == PARAMETER_HOLDER) else {
        return out;
    };
    let (Some(second), Some(quantity), Some(first)) =
        (refs.get(i + 1), refs.get(i + 2), refs.get(i + 3))
    else {
        return out;
    };
    let e = second.end;
    if second.class != PARAMETER_HOLDER
        || second.at != refs[i].end
        || quantity.at != e + 2
        || quantity.class != INT_HOLDER
        || first.class != DIRECTION_INPUT
    {
        return out;
    }
    let flag = |at: usize| d.get(at).copied().filter(|&b| b <= 1).map(|b| b == 1);
    let mut j = i + 4;
    let mut p = first.end;
    let two = refs
        .get(j)
        .is_some_and(|r| r.at == p && r.class == DIRECTION_INPUT);
    if two {
        p = refs[j].end;
        j += 1;
    } else if d.get(p) == Some(&0) {
        p += 1;
    } else {
        return out;
    }
    let Some((j, p)) = body_records(d, &refs, j, p) else {
        return out;
    };
    let Some((m, compute, _)) = tail(d, p) else {
        return out;
    };
    let Some((distance_type, numbers)) = after_tail(d, &refs, j, m) else {
        return out;
    };
    if distance_type > 1 {
        return out;
    }
    out.symmetric_one = flag(e);
    out.symmetric_two = flag(e + 1);
    out.compute = compute_name(compute);
    out.extent = Some(distance_type == 1);
    out.two_directions = Some(two);
    if numbers.is_empty() {
        out.suppressed = Some(Vec::new());
    }
    out
}

/// A rectangular pattern's second direction as a unit vector where it
/// names no entity: the data object stores three `f64` after a `-1` marker
/// and before the two distances' parameter holders (24 bytes, the vector
/// of the API's `directionTwo`; the all-zero value of a pattern of one
/// direction gives none). `item` is the item's object.
pub(super) fn rectangular_direction_two(seg: &Segment, item: &[u8]) -> Option<[f64; 3]> {
    let d = data_object(seg, item, RECTANGULAR_PATTERN_DATA)?;
    let refs = located(seg, d);
    let at = refs.iter().find(|r| r.class == PARAMETER_HOLDER)?.at;
    let start = at.checked_sub(24)?;
    if d.get(start.checked_sub(4)?..start)? != [0xff; 4] {
        return None;
    }
    let v = f64s_at(d, start, 3)?;
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (v.iter().all(|x| x.is_finite()) && (len - 1.0).abs() < 1e-6)
        .then(|| [v[0] / len, v[1] / len, v[2] / len])
}

/// What a path pattern's data object gives: its path's list input (a
/// [`BODY_INPUT`] of entity and edge inputs, as a sweep's path), the
/// settings read and the three flag bytes before the path input.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PathPatternData {
    pub path: u64,
    pub settings: PatternSettings,
    /// `isFlipDirection`, `isOrientationAlongPath` and `isSymmetric`, in
    /// the order of the bytes (mitcad#673).
    pub flip: bool,
    pub along_path: bool,
    pub symmetric: bool,
    /// The patterned objects' centre (cm), where the original sits on the
    /// path: the path's point nearest to it.
    pub center: [f64; 3],
}

/// A path pattern's path and settings (mitcad#468, mitcad#673); `item` is
/// the item's object. The data object (class version 4) holds, after its
/// root part, `u8 | u32 | ref quantity holder | u32 | ref distance holder |
/// 5 bytes | f64[3] | u8 1 | u8 flip | u8 along path | u8 symmetric | ref
/// path input | f64 | ref distance holder | ref key point | 8 bytes | u32 n
/// | n body records | 8 bytes`, then the tail and what follows it as a
/// rectangular pattern's: the compute option, the object kind, a byte (0
/// spacing, 1 extent) and the suppressed element numbers. (The oldest tail,
/// [`tail`]'s legacy one, has neither the 8 bytes after the records nor
/// the `-1`, mitcad#2493.) Only an empty
/// suppressed list is read, as for a rectangular pattern.
///
/// *(From two path patterns: the reference model's, four copies of a body
/// along one straight sketch line by spacing, none of the flags set, and a
/// design's, 372 copies of two features along a closed chain of lines and
/// arcs by extent, turned along it. The `f64[3]` is the patterned objects'
/// centre in cm (the reference model's cube's), not a point of the path;
/// the 8 bytes before the body-record count and after the records are zero
/// in both, so the count is read after them (the reference model alone,
/// without records, did not tell the two gaps apart). The design sets the
/// middle flag byte and turns its copies along the path, which its history
/// state confirms; the first and the last byte are zero in both, and are
/// read as the flip and the symmetry in the order the settings are listed
/// elsewhere, which neither file confirms.)*
pub(super) fn path(seg: &Segment, item: &[u8]) -> Option<PathPatternData> {
    let d = data_object(seg, item, PATH_PATTERN_DATA)?;
    let refs = located(seg, d);
    let i = refs.iter().position(|r| r.class == BODY_INPUT)?;
    let flags = d.get(refs[i].at.checked_sub(4)?..refs[i].at)?;
    if i < 2
        || refs[i - 2].class != INT_HOLDER
        || refs[i - 1].class != PARAMETER_HOLDER
        || refs[i - 1].end + 33 != refs[i].at
        || flags[0] != 1
        || flags[1..].iter().any(|&b| b > 1)
    {
        return None;
    }
    let center = f64s_at(d, refs[i].at - 28, 3)?;
    if center.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let path = seg.ref_at(d, refs[i].at)?.id;
    let (holder, key) = (refs.get(i + 1)?, refs.get(i + 2)?);
    if holder.class != PARAMETER_HOLDER
        || holder.at != refs[i].end + 8
        || key.class != KEY_POINT
        || key.at != holder.end
    {
        return None;
    }
    // 8 bytes before the body records' count, 8 after the records, then
    // the tail's compute option, object kind and -1; or the oldest tail,
    // the compute option and the object kind right after the records
    // (mitcad#2493: a design saved by build 2.0.12670, whose path pattern
    // then reads to the end of its data object as the others do).
    let (j, p) = body_records(d, &refs, i + 3, key.end + 8)?;
    let (m, compute, _) = tail(d, p)?;
    let compute = compute_name(compute)?;
    let (distance_type, numbers) = after_tail(d, &refs, j, m)?;
    if distance_type > 1 {
        return None;
    }
    let settings = PatternSettings {
        compute: Some(compute),
        extent: Some(distance_type == 1),
        suppressed: numbers.is_empty().then(Vec::new),
        ..PatternSettings::default()
    };
    Some(PathPatternData {
        path,
        settings,
        flip: flags[1] == 1,
        along_path: flags[2] == 1,
        symmetric: flags[3] == 1,
        center: [center[0], center[1], center[2]],
    })
}

/// A mirror's compute option; `item` is the item's object.
pub(super) fn mirror(seg: &Segment, item: &[u8]) -> PatternSettings {
    let mut out = PatternSettings::default();
    let Some(d) = data_object(seg, item, MIRROR_DATA) else {
        return out;
    };
    let Some(root) = seg.root_part(d).map(|r| r.end) else {
        return out;
    };
    let Some(m) = (root..d.len().saturating_sub(3)).find(|&m| d[m..m + 4] == [0xff; 4]) else {
        return out;
    };
    if m >= root + 8
        && let (Some(compute), Some(_)) = (
            u32_at(d, m - 8).and_then(compute_name),
            u32_at(d, m - 4).filter(|&k| k <= 2),
        )
    {
        out.compute = Some(compute);
        // The byte after the root part, as a circular pattern's symmetry:
        // whether the bodies are joined with their images (always 0 for
        // a mirror of features).
        out.combine = d.get(root).copied().filter(|&b| b <= 1).map(|b| b == 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::super::tests::Doc;
    use super::*;

    const ROOT: &str = "98542EB9-A4F2-4137-A808-DBB5B3CD6159";
    const ITEM: &str = "11F1A5CE-2B57-4476-8480-6994621493C9";

    fn r(id: u64) -> Vec<u8> {
        let mut v = vec![1];
        v.extend(id.to_le_bytes());
        v.extend([0, 0]);
        v
    }

    fn u(v: u32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }

    /// A design with one item of class `ITEM` whose data object (class
    /// `data`) has the body `data_body`; the objects 3 to 12 stand for
    /// what the data object refers to.
    fn doc(data: &str, data_body: &[u8]) -> Doc {
        let mut d = Doc::default();
        for (g, p, v) in [
            (ROOT, "", 0),
            (ITEM, ROOT, 0),
            (data, ROOT, 0),
            (PARAMETER_HOLDER, ROOT, 1),
            (DIRECTION_INPUT, ROOT, 1),
            (BODY_RECORD, ROOT, 1),
            (PATTERN_ELEMENT, ROOT, 1),
            (INT_HOLDER, ROOT, 1),
            (BODY_INPUT, ROOT, 1),
            (KEY_POINT, ROOT, 1),
        ] {
            d.class(g, p, v);
        }
        d.obj(1, ITEM, [&[0, 0][..], &r(2)].concat());
        d.obj(2, data, data_body.to_vec());
        for (id, class) in [
            (3, PARAMETER_HOLDER),
            (4, PARAMETER_HOLDER),
            (5, DIRECTION_INPUT),
            (6, DIRECTION_INPUT),
            (7, BODY_RECORD),
            (8, PATTERN_ELEMENT),
            (9, PATTERN_ELEMENT),
            (10, INT_HOLDER),
            (11, BODY_INPUT),
            (12, KEY_POINT),
        ] {
            d.obj(id, class, vec![0, 0]);
        }
        d
    }

    /// The tail and what follows it: `[gap] compute kind -1 byte | 1 pair
    /// | 2 elements | three references | suppressed numbers`.
    fn rest(gap: usize, compute: u32, kind: u32, byte: u8, suppressed: &[u32]) -> Vec<u8> {
        let mut v = vec![0; gap];
        v.extend([u(compute), u(kind), vec![0xff; 4], vec![byte]].concat());
        v.extend(
            [
                u(1),
                2u64.to_le_bytes().to_vec(),
                7u64.to_le_bytes().to_vec(),
            ]
            .concat(),
        );
        v.extend([u(2), r(8), r(9), r(10), r(1), r(11)].concat());
        v.extend(u(suppressed.len() as u32));
        for n in suppressed {
            v.extend(u(*n));
        }
        v
    }

    fn circular_body(
        flag: u8,
        bodies: u32,
        gap: usize,
        compute: u32,
        suppressed: &[u32],
    ) -> Vec<u8> {
        let mut v = vec![0, 0, flag];
        v.extend([r(3), r(5), u(bodies)].concat());
        for _ in 0..bodies {
            v.extend(r(7));
        }
        v.extend(rest(gap, compute, 1, 1, suppressed));
        v
    }

    fn settings(d: &Doc, read: fn(&Segment, &[u8]) -> PatternSettings) -> PatternSettings {
        let seg = d.segment();
        read(&seg, seg.data_of(1))
    }

    #[test]
    fn circular_pattern_settings() {
        // Symmetric, optimized, no body records, two elements suppressed.
        let d = doc(CIRCULAR_PATTERN_DATA, &circular_body(1, 0, 8, 0, &[3, 5]));
        let s = settings(&d, circular);
        assert_eq!(s.symmetric, Some(true));
        assert_eq!(s.compute, Some("OptimizedPatternCompute"));
        // The file counts the original as 1, the API as 0.
        assert_eq!(s.suppressed, Some(vec![2, 4]));

        // Not symmetric, identical, the shorter gap after one record.
        let d = doc(CIRCULAR_PATTERN_DATA, &circular_body(0, 1, 4, 1, &[]));
        let s = settings(&d, circular);
        assert_eq!(s.symmetric, Some(false));
        assert_eq!(s.compute, Some("IdenticalPatternCompute"));
        assert_eq!(s.suppressed, Some(vec![]));

        // The oldest layout: no gap and no -1.
        let mut body = vec![0, 0, 0];
        body.extend([r(3), r(5), u(0)].concat());
        body.extend([u(2), u(1), vec![1]].concat());
        body.extend(
            [
                u(1),
                2u64.to_le_bytes().to_vec(),
                7u64.to_le_bytes().to_vec(),
            ]
            .concat(),
        );
        body.extend([u(2), r(8), r(9), r(10), r(1), r(11), u(0)].concat());
        let s = settings(&doc(CIRCULAR_PATTERN_DATA, &body), circular);
        assert_eq!(s.compute, Some("AdjustPatternCompute"));
        assert_eq!(s.suppressed, Some(vec![]));
    }

    #[test]
    fn circular_pattern_with_unknown_layout_reads_nothing() {
        // An unknown compute code.
        let d = doc(CIRCULAR_PATTERN_DATA, &circular_body(1, 0, 8, 9, &[]));
        assert_eq!(settings(&d, circular), PatternSettings::default());
        // Bytes after the numbers.
        let mut body = circular_body(1, 0, 8, 2, &[]);
        body.push(0);
        let s = settings(&doc(CIRCULAR_PATTERN_DATA, &body), circular);
        assert_eq!(s.compute, Some("AdjustPatternCompute"));
        assert_eq!(s.suppressed, None);
        // A data object of another class.
        let d = doc(MIRROR_DATA, &circular_body(1, 0, 8, 2, &[]));
        assert_eq!(settings(&d, circular), PatternSettings::default());
    }

    fn rectangular_body(symmetric: [u8; 2], two: bool, extent: u8, suppressed: &[u32]) -> Vec<u8> {
        let mut v = vec![0, 0];
        v.extend([r(10), vec![0; 4], r(10), r(3), r(4)].concat());
        v.extend(symmetric);
        v.extend([r(10), r(5)].concat());
        if two {
            v.extend(r(6));
        } else {
            v.push(0);
        }
        v.extend(u(0));
        v.extend(rest(8, 2, 1, extent, suppressed));
        v
    }

    #[test]
    fn rectangular_pattern_settings() {
        let d = doc(
            RECTANGULAR_PATTERN_DATA,
            &rectangular_body([1, 0], false, 1, &[]),
        );
        let s = settings(&d, rectangular);
        assert_eq!(s.symmetric_one, Some(true));
        assert_eq!(s.symmetric_two, Some(false));
        assert_eq!(s.extent, Some(true));
        assert_eq!(s.two_directions, Some(false));
        assert_eq!(s.compute, Some("AdjustPatternCompute"));
        assert_eq!(s.suppressed, Some(vec![]));

        let d = doc(
            RECTANGULAR_PATTERN_DATA,
            &rectangular_body([0, 1], true, 0, &[4]),
        );
        let s = settings(&d, rectangular);
        assert_eq!(s.symmetric_two, Some(true));
        assert_eq!(s.extent, Some(false));
        assert_eq!(s.two_directions, Some(true));
        // The element numbers of a rectangular pattern are not read.
        assert_eq!(s.suppressed, None);
    }

    #[test]
    fn rectangular_pattern_second_direction() {
        let body = |v: [f64; 3]| {
            let mut b = vec![0, 0];
            b.extend([0xff; 4]);
            for x in v {
                b.extend(x.to_le_bytes());
            }
            b.extend([r(3), r(4)].concat());
            b
        };
        let read = |v: [f64; 3]| {
            let d = doc(RECTANGULAR_PATTERN_DATA, &body(v));
            let seg = d.segment();
            rectangular_direction_two(&seg, seg.data_of(1))
        };
        // A unit vector: the second direction (a vector that is not a unit
        // one is normalised only within the tolerance).
        assert_eq!(read([0.0, -1.0, 0.0]), Some([0.0, -1.0, 0.0]));
        assert_eq!(read([0.0, 0.0, 1.0]), Some([0.0, 0.0, 1.0]));
        // A pattern of one direction stores zeros: none.
        assert_eq!(read([0.0, 0.0, 0.0]), None);
        assert_eq!(read([0.0, 3.0, 0.0]), None);
        // Without the marker before the vector: none.
        let mut b = body([1.0, 0.0, 0.0]);
        b[2] = 0;
        let d = doc(RECTANGULAR_PATTERN_DATA, &b);
        let seg = d.segment();
        assert_eq!(rectangular_direction_two(&seg, seg.data_of(1)), None);
    }

    #[test]
    fn mirror_compute_option() {
        for joined in [0, 1] {
            let mut body = vec![0, 0, joined, 0];
            body.extend([r(3), r(5)].concat());
            body.extend([vec![0; 8], u(1), u(2), vec![0xff; 4], vec![1, 1]].concat());
            let s = settings(&doc(MIRROR_DATA, &body), mirror);
            assert_eq!(s.compute, Some("IdenticalPatternCompute"));
            assert_eq!(s.combine, Some(joined == 1));
            let s = settings(&doc(CIRCULAR_PATTERN_DATA, &body), mirror);
            assert_eq!(s, PatternSettings::default());
        }
    }

    /// A path pattern's data object (mitcad#468, mitcad#673): the objects'
    /// centre, the four bytes before the path input (1 and the flip,
    /// orientation and symmetry flags), `bodies` body records between their
    /// two 8-byte gaps, the tail's compute code and byte, and the suppressed
    /// numbers.
    fn path_body(
        flags: [u8; 4],
        bodies: u32,
        compute: u32,
        byte: u8,
        suppressed: &[u32],
    ) -> Vec<u8> {
        let mut v = vec![0, 0, 1];
        v.extend([u(2), r(10), u(0), r(3), vec![0; 5]].concat());
        for x in [0.65f64, 0.095, 6.09] {
            v.extend(x.to_le_bytes());
        }
        v.extend(
            [
                flags.to_vec(),
                r(11),
                1.5f64.to_le_bytes().to_vec(),
                r(3),
                r(12),
                vec![0; 8],
            ]
            .concat(),
        );
        v.extend(u(bodies));
        for _ in 0..bodies {
            v.extend(r(7));
        }
        v.extend(rest(8, compute, 2, byte, suppressed));
        v
    }

    fn read_path(body: &[u8]) -> Option<PathPatternData> {
        let d = doc(PATH_PATTERN_DATA, body);
        let seg = d.segment();
        path(&seg, seg.data_of(1))
    }

    #[test]
    fn path_pattern_data() {
        // The reference model's: no body records, no flags.
        let p = read_path(&path_body([1, 0, 0, 0], 0, 2, 0, &[])).expect("the layout");
        assert_eq!(p.path, 11);
        assert_eq!(p.settings.compute, Some("AdjustPatternCompute"));
        assert_eq!(p.settings.extent, Some(false));
        assert_eq!(p.settings.suppressed, Some(vec![]));
        assert!(!p.flip && !p.along_path && !p.symmetric);
        assert_eq!(p.center, [0.65, 0.095, 6.09]);

        // Body records, the identical option, an extent.
        let p = read_path(&path_body([1, 0, 0, 0], 2, 1, 1, &[])).expect("the layout");
        assert_eq!(p.settings.compute, Some("IdenticalPatternCompute"));
        assert_eq!(p.settings.extent, Some(true));

        // Suppressed numbers are not read (their numbering is not known).
        let p = read_path(&path_body([1, 0, 0, 0], 0, 2, 0, &[3])).expect("the layout");
        assert_eq!(p.settings.suppressed, None);

        // Another value before the flags or a flag byte that is no flag, an
        // unknown distance type byte, a gap of another size or another data
        // object: nothing.
        assert_eq!(read_path(&path_body([2, 0, 0, 0], 0, 2, 0, &[])), None);
        assert_eq!(read_path(&path_body([1, 0, 2, 0], 0, 2, 0, &[])), None);
        assert_eq!(read_path(&path_body([1, 0, 0, 0], 0, 2, 2, &[])), None);
        let mut body = path_body([1, 0, 0, 0], 0, 2, 0, &[]);
        body.remove(120);
        assert_eq!(read_path(&body), None);
        let d = doc(
            CIRCULAR_PATTERN_DATA,
            &path_body([1, 0, 0, 0], 0, 2, 0, &[]),
        );
        let seg = d.segment();
        assert_eq!(path(&seg, seg.data_of(1)), None);
    }

    #[test]
    fn path_pattern_body_records_after_their_gap() {
        // mitcad#673: a design's pattern of features along a closed chain,
        // one body record, identical, by extent. The count lies 8 bytes after
        // the key point (zeros there, as in the reference model, whose lack
        // of records hid the gap): read at the key point's end, it is 0 and
        // the record is taken for the gap before the tail.
        let body = path_body([1, 0, 1, 0], 1, 1, 1, &[]);
        let p = read_path(&body).expect("the layout");
        assert_eq!(p.settings.compute, Some("IdenticalPatternCompute"));
        assert_eq!(p.settings.extent, Some(true));
        assert_eq!(p.settings.suppressed, Some(vec![]));
        // The middle flag: the copies turn along the path.
        assert!(p.along_path);
        assert!(!p.flip && !p.symmetric);
        // The body records directly after the key point (the old reading)
        // do not parse.
        let mut old = body.clone();
        let key_end = 107;
        old.drain(key_end..key_end + 8);
        old.splice(key_end + 15..key_end + 15, [0; 8]);
        assert_eq!(read_path(&old), None);
        // The flip and the symmetry bytes.
        let p = read_path(&path_body([1, 1, 0, 1], 0, 2, 0, &[])).expect("the layout");
        assert!(p.flip && !p.along_path && p.symmetric);
    }

    #[test]
    fn path_pattern_with_the_oldest_tail() {
        // mitcad#2493: a design saved by an older build keeps the oldest
        // tail after the body records, without the 8 bytes and the -1 (as
        // a circular pattern's oldest layout); the rest reads as before.
        let body = path_body([1, 0, 0, 0], 1, 2, 1, &[]);
        // The key point's end, the gap, the count and one record.
        let records_end = 107 + 8 + 4 + 11;
        let mut oldest = body.clone();
        oldest.drain(records_end + 16..records_end + 20);
        oldest.drain(records_end..records_end + 8);
        let p = read_path(&oldest).expect("the oldest tail");
        assert_eq!(p.path, 11);
        assert_eq!(p.settings.compute, Some("AdjustPatternCompute"));
        assert_eq!(p.settings.extent, Some(true));
        assert_eq!(p.settings.suppressed, Some(vec![]));
        // The gap without the -1: nothing.
        let mut no_marker = body;
        no_marker.drain(records_end + 16..records_end + 20);
        assert_eq!(read_path(&no_marker), None);
    }

    #[test]
    fn compute_codes() {
        assert_eq!(compute_name(0), Some("OptimizedPatternCompute"));
        assert_eq!(compute_name(1), Some("IdenticalPatternCompute"));
        assert_eq!(compute_name(2), Some("AdjustPatternCompute"));
        assert_eq!(compute_name(3), None);
    }
}
