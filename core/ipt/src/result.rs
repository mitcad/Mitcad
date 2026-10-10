// SPDX-License-Identifier: MIT
//! The part's bodies as the result segment lists them (`PmResultSegment`):
//! for each body of the browser's body folders its node number, whether it
//! is shown, whether it is a solid, and its range box; groups of surfaces
//! (composite features) are one entry for all their bodies. Assemblies copy
//! this list into their document descriptors (`assembly`).
//!
//! The record (`24af8a12…`, the first of the segment): a u32, a u16, a byte
//! (majors 16 and 18) or a u32; then a list (kind 4: u16 4, u16 `0x3000`, u32
//! count and, when not empty, a u32 tag) of entries: the body's node number
//! (the header's node
//! of its body record in the definitions segment), a list of the features
//! that made its faces (their node numbers, as the faces' naming attributes
//! in the B-rep record name them), u32 shown (1) or hidden (0), u32 kind (2
//! a solid, 1 surfaces), a list of the bodies of a group (empty for one
//! body) and the range box (six f64, cm: min, max). Two more lists follow
//! that the import does not read. See `README.md`.

use crate::IptError;
use crate::dc::{self, DcError, Reader};

/// The result segment's name.
pub const RESULT_SEGMENT: &str = "PmResultSegment";

/// The record type of the body list, as stored.
pub const RESULT_BODIES: [u8; 16] = dc::type_id("24af8a125e4632dfe3f0509dc05b0cca");

/// A body of the result list.
#[derive(Clone, Debug, PartialEq)]
pub struct ResultBody {
    /// Its node number.
    pub node: u32,
    /// The node numbers of the features that made its faces.
    pub features: Vec<u32>,
    /// Shown in the part (the browser's visibility).
    pub visible: bool,
    /// A solid (kind 2); else surfaces (kind 1).
    pub solid: bool,
    /// The kind as stored (1 surfaces, 2 a solid).
    pub kind: u32,
    /// The node numbers of the bodies of a group (a composite feature's
    /// surfaces); empty for a single body.
    pub members: Vec<u32>,
    /// The range box (cm): min, max.
    pub range: [[f64; 3]; 2],
}

impl ResultBody {
    /// The number of bodies of the B-rep record it stands for.
    pub fn bodies(&self) -> usize {
        self.members.len().max(1)
    }

    /// Whether the body still exists: the list keeps entries of bodies a
    /// later feature consumed, with an empty range (min above max, or all
    /// zero).
    pub fn exists(&self) -> bool {
        let [min, max] = self.range;
        (0..3).all(|k| min[k].is_finite() && max[k].is_finite() && min[k] <= max[k])
            && min.iter().chain(&max).any(|v| *v != 0.0)
    }
}

/// A list of kind 4: its length (the tag after the count is skipped).
fn list_len(r: &mut Reader) -> dc::Result<usize> {
    let at = r.pos;
    let kind = r.u16()?;
    let tag = r.u16()?;
    if kind != 4 || tag != 0x3000 {
        return Err(DcError(format!(
            "no list of kind 4 at {at} ({kind:#x} {tag:#x})"
        )));
    }
    let n = r.u32()? as usize;
    if n == 0 {
        return Ok(0);
    }
    r.skip(4)?;
    if n > r.left() / 4 {
        return Err(DcError(format!(
            "a list of {n} items at {at} with {} bytes left",
            r.left()
        )));
    }
    Ok(n)
}

fn u32_list(r: &mut Reader) -> dc::Result<Vec<u32>> {
    let n = list_len(r)?;
    (0..n).map(|_| r.u32()).collect()
}

/// Reads the body list from the record's bytes.
pub fn parse(bytes: &[u8]) -> Result<Vec<ResultBody>, String> {
    let e = |e: DcError| e.to_string();
    // A u32 and a u16, then a byte (majors 16 and 18) or a u32.
    let list = [4, 0, 0, 0x30];
    let start = [7, 10]
        .into_iter()
        .find(|&at| bytes.get(at..at + 4) == Some(&list[..]))
        .unwrap_or(10);
    let mut r = Reader::at(bytes, start);
    let n = list_len(&mut r).map_err(e)?;
    let mut out = Vec::with_capacity(n.min(100_000));
    for _ in 0..n {
        let node = r.u32().map_err(e)?;
        let features = u32_list(&mut r).map_err(e)?;
        let shown = r.u32().map_err(e)?;
        let kind = r.u32().map_err(e)?;
        let members = u32_list(&mut r).map_err(e)?;
        let mut v = [0.0; 6];
        for x in &mut v {
            *x = r.f64().map_err(e)?;
        }
        if shown > 1 || !(1..=2).contains(&kind) {
            return Err(format!(
                "body {node}: shown {shown} and kind {kind} are not known"
            ));
        }
        out.push(ResultBody {
            node,
            features,
            visible: shown == 1,
            solid: kind == 2,
            kind,
            members,
            range: [[v[0], v[1], v[2]], [v[3], v[4], v[5]]],
        });
    }
    Ok(out)
}

impl crate::IptFile {
    /// The part's bodies as its result segment lists them; None when the
    /// file has no such list.
    pub fn result_bodies(&self) -> Result<Option<Vec<ResultBody>>, IptError> {
        let Some(defs) = self.segment_records(RESULT_SEGMENT)? else {
            return Ok(None);
        };
        let Some(i) = defs.of_type(&RESULT_BODIES).next() else {
            return Ok(None);
        };
        parse(defs.bytes(i))
            .map(Some)
            .map_err(|e| IptError::Segment(format!("{RESULT_SEGMENT}#{i}: {e}")))
    }

    /// The range boxes (cm) of the existing bodies the file keeps hidden.
    pub fn hidden_ranges(&self) -> Result<Vec<[[f64; 3]; 2]>, IptError> {
        Ok(self
            .result_bodies()?
            .unwrap_or_default()
            .into_iter()
            .filter(|b| !b.visible && b.exists())
            .map(|b| b.range)
            .collect())
    }
}

/// The record of a body list, as the writer of the test files makes it.
pub fn write(bodies: &[ResultBody]) -> Vec<u8> {
    fn list(out: &mut Vec<u8>, items: &[u32], tag: u32) {
        out.extend_from_slice(&[4, 0, 0, 0x30]);
        out.extend_from_slice(&(items.len() as u32).to_le_bytes());
        if !items.is_empty() {
            out.extend_from_slice(&tag.to_le_bytes());
        }
        for i in items {
            out.extend_from_slice(&i.to_le_bytes());
        }
    }
    let mut out = vec![0, 0, 0, 0, 1, 0];
    out.extend_from_slice(&0x42u32.to_le_bytes());
    out.extend_from_slice(&[4, 0, 0, 0x30]);
    out.extend_from_slice(&(bodies.len() as u32).to_le_bytes());
    if !bodies.is_empty() {
        out.extend_from_slice(&0x101u32.to_le_bytes());
    }
    for b in bodies {
        out.extend_from_slice(&b.node.to_le_bytes());
        list(&mut out, &b.features, 0);
        out.extend_from_slice(&u32::from(b.visible).to_le_bytes());
        out.extend_from_slice(&b.kind.to_le_bytes());
        list(&mut out, &b.members, 0);
        for x in b.range.as_flattened() {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
    // The two lists the import does not read, empty.
    out.extend_from_slice(&[4, 0, 0, 0x30, 0, 0, 0, 0]);
    out.extend_from_slice(&[4, 0, 0, 0x30, 0, 0, 0, 0]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(node: u32, visible: bool, kind: u32, members: Vec<u32>) -> ResultBody {
        ResultBody {
            node,
            features: vec![node + 10, node + 11],
            visible,
            solid: kind == 2,
            kind,
            members,
            range: [[-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]],
        }
    }

    #[test]
    fn reads_the_written_list() {
        let bodies = vec![
            body(2, true, 2, vec![]),
            body(3500, false, 1, vec![]),
            body(85, true, 1, vec![83, 84]),
        ];
        let parsed = parse(&write(&bodies)).unwrap();
        assert_eq!(parsed, bodies);
        assert_eq!(parsed[2].bodies(), 2);
        assert!(parse(&write(&[])).unwrap().is_empty());
    }

    #[test]
    fn consumed_bodies_have_empty_ranges() {
        let mut b = body(2, true, 2, vec![]);
        assert!(b.exists());
        b.range = [[f64::MAX; 3], [-f64::MAX; 3]];
        assert!(!b.exists());
        b.range = [[0.0; 3], [0.0; 3]];
        assert!(!b.exists());
    }

    #[test]
    fn rejects_unknown_values() {
        let mut bad = body(2, true, 2, vec![]);
        bad.kind = 5;
        assert!(parse(&write(&[bad])).is_err());
        assert!(parse(&[0; 12]).is_err());
    }
}
