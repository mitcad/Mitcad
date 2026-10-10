// SPDX-License-Identifier: MIT
//! The loops of the profiles an extrusion or revolution selected
//! (mitcad#96) *(read from the learning dump of the private corpora: the
//! grammar parses every extrusion and revolution that has the object to
//! its end)*.
//!
//! A profile source ([`PROFILE_SOURCE`]) refers to a loop list
//! ([`PROFILE_LOOPS`]), whose data, after its root part and the reference
//! back to the source, up to its sub-chunk, is
//!
//! ```text
//! u32 n | n × profile
//! profile = u32 m | m × loop | u32 k | k × child
//! child   = u32 m | m × loop            (no children of its own)
//! loop    = u32 c | c × record | u8 outer (1 the outer loop, 0 a hole)
//! record  = u32 tag (2 or 3) | u64 crv_primary_id | u64 crv_secondary_id
//!         | u32 reversed | u32 piece | u32 pieces | u64 0
//! ```
//!
//! A record names a sketch curve by the ids the curve carries as root-part
//! attributes (as the entity inputs of sweeps do, [`super::sweeps`]); a
//! curve split by others is listed piece by piece (`piece` of `pieces`).
//! A profile's children are the pieces curves inside it split it into:
//! the regions it selects are then the children's, not its own loop's.
//! Records can name curves the sketch no longer has (a profile older than
//! the sketch's last edit).

use serde_json::{Value, json};

use super::super::classes::*;
use super::super::stream::{u32_at, u64_at};
use super::{Builder, inputs_of, source_sketch};

/// The most records, loops and profiles read per list (the corpus' largest
/// are far below).
const MAX_RECORDS: u32 = 5000;
const MAX_LOOPS: u32 = 500;

/// One curve (or piece of one) of a loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LoopCurve {
    pub tag: u32,
    pub primary: u64,
    pub secondary: u64,
    pub reversed: bool,
    pub piece: u32,
    pub pieces: u32,
}

/// A closed loop of curves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileLoop {
    pub curves: Vec<LoopCurve>,
    /// The outer loop (else a hole).
    pub outer: bool,
}

/// A selected profile: its loops and the pieces curves inside it split it
/// into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Profile {
    pub loops: Vec<ProfileLoop>,
    pub children: Vec<Vec<ProfileLoop>>,
}

/// A cursor over the list's bytes.
struct Reader<'a> {
    d: &'a [u8],
    p: usize,
    end: usize,
}

impl Reader<'_> {
    fn u32(&mut self) -> Option<u32> {
        if self.p + 4 > self.end {
            return None;
        }
        let v = u32_at(self.d, self.p)?;
        self.p += 4;
        Some(v)
    }

    fn one_loop(&mut self) -> Option<ProfileLoop> {
        let n = self.u32().filter(|&n| n <= MAX_RECORDS)?;
        let mut curves = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let p = self.p;
            if p + 40 > self.end {
                return None;
            }
            let d = self.d;
            curves.push(LoopCurve {
                tag: u32_at(d, p)?,
                primary: u64_at(d, p + 4)?,
                secondary: u64_at(d, p + 12)?,
                reversed: u32_at(d, p + 20)? != 0,
                piece: u32_at(d, p + 24)?,
                pieces: u32_at(d, p + 28)?,
            });
            self.p += 40;
        }
        if self.p >= self.end {
            return None;
        }
        let outer = match self.d[self.p] {
            0 => false,
            1 => true,
            _ => return None,
        };
        self.p += 1;
        Some(ProfileLoop { curves, outer })
    }

    fn loops(&mut self) -> Option<Vec<ProfileLoop>> {
        let n = self.u32().filter(|&n| n <= MAX_LOOPS)?;
        (0..n).map(|_| self.one_loop()).collect()
    }
}

/// The profiles of a loop list's data `d[start..end]`; `None` unless the
/// grammar reads it to its end exactly.
pub(crate) fn parse_loop_list(d: &[u8], start: usize, end: usize) -> Option<Vec<Profile>> {
    let mut r = Reader {
        d,
        p: start,
        end: end.min(d.len()),
    };
    let n = r.u32().filter(|&n| n <= MAX_LOOPS)?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let loops = r.loops()?;
        let k = r.u32().filter(|&k| k <= MAX_LOOPS)?;
        let children = (0..k).map(|_| r.loops()).collect::<Option<Vec<_>>>()?;
        out.push(Profile { loops, children });
    }
    (r.p == r.end).then_some(out)
}

impl Builder<'_> {
    /// The loops of the profiles an item selected, in the dump's terms
    /// (`_f3d_profile_loops`): per profile its `loops` and `children` (lists
    /// of loops), each loop `outer` and its `curves`, each curve its ids
    /// and, where the sketch has it, its sketch-local `id` (`c5`).
    pub(super) fn profile_loops(&self, fid: u64) -> Option<Value> {
        let seg = self.seg;
        let (src, list) = inputs_of(seg, fid, PROFILE_SOURCE)
            .into_iter()
            .find_map(|s| Some((s, *inputs_of(seg, s, PROFILE_LOOPS).first()?)))?;
        let sketch = source_sketch(seg, src);
        let o = seg.object(list)?;
        let d = seg.data(o);
        // The reference back to the source follows the root part.
        let start = seg.ref_at(d, seg.root_part(d)?.end)?.end;
        let profiles = parse_loop_list(d, start, seg.main_end(o))?;
        let curve = |c: &LoopCurve| {
            let mut v = json!({"primary": c.primary, "secondary": c.secondary, "tag": c.tag,
                               "reversed": c.reversed, "piece": c.piece, "pieces": c.pieces});
            let local = sketch
                .and_then(|s| self.tags.curve(s, c.primary, c.secondary))
                .and_then(|e| self.local_ids.get(&e));
            if let Some((_, id)) = local {
                v["id"] = json!(id);
            }
            v
        };
        let loops = |ls: &[ProfileLoop]| -> Value {
            ls.iter()
                .map(|l| {
                    json!({"outer": l.outer,
                           "curves": l.curves.iter().map(curve).collect::<Vec<_>>()})
                })
                .collect()
        };
        Some(
            profiles
                .iter()
                .map(|p| {
                    json!({"loops": loops(&p.loops),
                           "children": p.children.iter().map(|c| loops(c)).collect::<Vec<_>>()})
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(primary: u64, piece: u32, pieces: u32) -> Vec<u8> {
        let mut v = 2u32.to_le_bytes().to_vec();
        v.extend(primary.to_le_bytes());
        v.extend(0u64.to_le_bytes());
        v.extend(1u32.to_le_bytes());
        v.extend(piece.to_le_bytes());
        v.extend(pieces.to_le_bytes());
        v.extend(0u64.to_le_bytes());
        v
    }

    fn one_loop(primaries: &[u64], outer: bool) -> Vec<u8> {
        let mut v = (primaries.len() as u32).to_le_bytes().to_vec();
        for &p in primaries {
            v.extend(record(p, 1, 1));
        }
        v.push(u8::from(outer));
        v
    }

    fn loops(ls: &[Vec<u8>]) -> Vec<u8> {
        let mut v = (ls.len() as u32).to_le_bytes().to_vec();
        for l in ls {
            v.extend(l);
        }
        v
    }

    #[test]
    fn profiles_with_holes_and_children() {
        // Two profiles: a square with a round hole, and a rectangle that a
        // line inside it splits into two children.
        let mut d = vec![9, 9, 9];
        d.extend(2u32.to_le_bytes());
        d.extend(loops(&[
            one_loop(&[10, 12, 14, 16], true),
            one_loop(&[20], false),
        ]));
        d.extend(0u32.to_le_bytes());
        d.extend(loops(&[one_loop(&[30, 32, 34, 36], true)]));
        d.extend(2u32.to_le_bytes());
        d.extend(loops(&[one_loop(&[30, 38, 36], true)]));
        d.extend(loops(&[one_loop(&[32, 34, 38], true)]));
        let end = d.len();
        d.extend([7, 7]);
        let p = parse_loop_list(&d, 3, end).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].loops.len(), 2);
        assert!(p[0].loops[0].outer && !p[0].loops[1].outer);
        assert_eq!(p[0].loops[1].curves[0].primary, 20);
        assert!(p[0].children.is_empty());
        assert_eq!(p[1].children.len(), 2);
        let c = p[1].children[1][0].curves[2];
        assert_eq!((c.tag, c.primary, c.secondary), (2, 38, 0));
        assert!(c.reversed);
        assert_eq!((c.piece, c.pieces), (1, 1));
        // Not to the end, past it, or with a bad outer byte: nothing.
        assert!(parse_loop_list(&d, 3, end + 1).is_none());
        assert!(parse_loop_list(&d, 3, end - 1).is_none());
        let mut bad = d.clone();
        bad[3 + 4 + 4 + 4 + 4 * 40] = 2;
        assert!(parse_loop_list(&bad, 3, end).is_none());
    }

    #[test]
    fn empty_and_damaged_lists() {
        assert_eq!(parse_loop_list(&0u32.to_le_bytes(), 0, 4), Some(vec![]));
        assert!(parse_loop_list(&[1, 0, 0], 0, 3).is_none());
        let mut huge = 1u32.to_le_bytes().to_vec();
        huge.extend(1u32.to_le_bytes());
        huge.extend(u32::MAX.to_le_bytes());
        assert!(parse_loop_list(&huge, 0, huge.len()).is_none());
    }
}
