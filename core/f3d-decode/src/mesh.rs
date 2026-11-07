// SPDX-License-Identifier: MIT
//! The design's mesh bodies (mitcad#1366): which container holds each one,
//! the timeline item and the component it belongs to, its placement and
//! its name. The containers themselves are `mitcad_f3d_file::paramesh`.
//!
//! Three records join a container to its body:
//!
//! - an entry-name record ([`MESH_ENTRY`]): a reference to the GUID record
//!   and the container's entry name, `ParaMeshGeometry.<uuid>.paramesh`
//!   (`str16`);
//! - a GUID record ([`MESH_GUID`]): the GUID (`u32 36` and ASCII) that the
//!   container's registry repeats, and a reference back to the entry-name
//!   record;
//! - the mesh body ([`MESH_BODY`], class version 7): after its prologue, at
//!   offset 41, its two placements, each a flag byte, `1` for none (the
//!   container's coordinates are the design's centimetres), `0` followed
//!   by a row-major 4×4 `f64` matrix from the container's coordinates to
//!   centimetres; then two presentation envelopes (a GUID, the physical
//!   material library's GUID, an empty display name `u32 0` and a preset
//!   name, as the appearances' names: the physical material and the
//!   appearance it brings, read by `mitcad_f3d_design::appearance`,
//!   mitcad#1376) with the body's name between them where it has one;
//!   and references to its
//!   timeline item ([`MESH_FEATURE`]), its body holder ([`BLOB_HOLDER`],
//!   whose component is the body's, as for B-rep blobs) and its GUID
//!   record. A record without its placements stored is 256 bytes shorter
//!   than one with both, the flags `1 1` standing for the two matrices: the
//!   corpus's records are such, the format description's with matrices.

use std::collections::HashMap;

use super::classes::{BLOB_HOLDER, COMPONENT, MESH_BODY, MESH_ENTRY, MESH_FEATURE, MESH_GUID};
use super::stream::{Segment, f64s_at, header_end, str16_at, text_at, u32_at};

/// The physical-material library the presentation envelopes name.
const MATERIAL_LIBRARY: &str = "C1EEA57C-3F56-45FC-B8CB-A9EC46A9994C";

/// Where a mesh body record's placements start.
const PLACEMENTS: usize = 41;

/// A row-major 4×4 matrix.
pub type Matrix = [[f64; 4]; 4];

/// One mesh body of the design.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshBodyEntry {
    /// The mesh body record.
    pub id: u64,
    /// The container's entry name (`ParaMeshGeometry.<uuid>.paramesh`).
    pub file: Option<String>,
    /// The GUID the container repeats (lower case).
    pub key: Option<String>,
    /// The timeline item that made it (its object id).
    pub feature: Option<u64>,
    /// Its body holder (`CD57BC48`).
    pub holder: Option<u64>,
    /// The component that holds it (its object id).
    pub component: Option<u64>,
    /// Its name, where it has one.
    pub name: Option<String>,
    /// From the container's coordinates to the design's centimetres,
    /// row-major; `None` the identity.
    pub transform: Option<[[f64; 4]; 4]>,
    /// Why its placement is not read (it is then not to be placed).
    pub error: Option<String>,
}

/// The mesh bodies of a design segment, in stream order.
pub fn mesh_bodies(seg: &Segment) -> Vec<MeshBodyEntry> {
    // Each GUID record's container entry name.
    let mut file_of: HashMap<u64, String> = HashMap::new();
    for o in seg.objects_of(MESH_ENTRY) {
        let d = seg.data(o);
        let start = header_end(d).unwrap_or(0);
        for (_, r) in seg.refs_in(d, start, seg.main_end(o)) {
            if seg.guid_of(r.id) == Some(MESH_GUID)
                && let Some((name, _)) = str16_at(d, r.end)
                && name.ends_with(".paramesh")
            {
                file_of.entry(r.id).or_insert(name);
                break;
            }
        }
    }
    // Each GUID record's GUID.
    let key_of: HashMap<u64, String> = seg
        .objects_of(MESH_GUID)
        .filter_map(|o| {
            let d = seg.data(o);
            let start = header_end(d).unwrap_or(0);
            let end = seg.main_end(o).min(d.len());
            (start..end.saturating_sub(40))
                .find(|&p| u32_at(d, p) == Some(36) && is_guid(&d[p + 4..p + 40]))
                .map(|p| {
                    (
                        o.id,
                        String::from_utf8_lossy(&d[p + 4..p + 40]).to_lowercase(),
                    )
                })
        })
        .collect();
    seg.objects_of(MESH_BODY)
        .map(|o| {
            let d = seg.data(o);
            let end = seg.main_end(o).min(d.len());
            let start = header_end(d).unwrap_or(0);
            let mut body = MeshBodyEntry {
                id: o.id,
                ..MeshBodyEntry::default()
            };
            let mut guid = None;
            for (_, r) in seg.refs_in(d, start, end) {
                match seg.guid_of(r.id) {
                    Some(MESH_FEATURE) => {
                        body.feature.get_or_insert(r.id);
                    }
                    Some(BLOB_HOLDER) => {
                        body.holder.get_or_insert(r.id);
                    }
                    Some(MESH_GUID) => {
                        guid.get_or_insert(r.id);
                    }
                    _ => {}
                }
            }
            if let Some(g) = guid {
                body.file = file_of.get(&g).cloned();
                body.key = key_of.get(&g).cloned();
            }
            // The holder's component, as a B-rep blob's (`decode::decode`).
            body.component = body.holder.and_then(|h| {
                seg.ref_ids(seg.data_of(h))
                    .into_iter()
                    .rev()
                    .find(|&r| seg.guid_of(r) == Some(COMPONENT))
            });
            match placement(d, seg.version(o)) {
                Ok((transform, after)) => {
                    body.transform = transform;
                    body.name = name_between_envelopes(d, after, end);
                }
                Err(e) => body.error = Some(e),
            }
            body
        })
        .collect()
}

/// The placement of a mesh body record and the offset after the
/// placements.
fn placement(d: &[u8], version: Option<u32>) -> Result<(Option<Matrix>, usize), String> {
    if version != Some(7) {
        return Err(format!(
            "the placement of a mesh body of class version {} is not read",
            version.map_or("?".to_owned(), |v| v.to_string())
        ));
    }
    let mut p = PLACEMENTS;
    let mut found: Vec<Option<Matrix>> = Vec::new();
    for _ in 0..2 {
        match d.get(p) {
            Some(1) => {
                found.push(None);
                p += 1;
            }
            Some(0) => {
                let v = f64s_at(d, p + 1, 16).ok_or("the mesh body ends in its placement")?;
                found.push(Some(
                    [0, 1, 2, 3].map(|r| [v[4 * r], v[4 * r + 1], v[4 * r + 2], v[4 * r + 3]]),
                ));
                p += 1 + 128;
            }
            Some(b) => return Err(format!("the mesh body's placement flag {b} is not known")),
            None => return Err("the mesh body ends before its placement".to_owned()),
        }
    }
    if found[0] != found[1] {
        return Err("the mesh body's two placements differ".to_owned());
    }
    if let Some(m) = found[0] {
        let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        if m.iter().flatten().any(|v| !v.is_finite())
            || m[3] != [0.0, 0.0, 0.0, 1.0]
            || det.abs() < 1e-12
        {
            return Err("the mesh body's placement is not an affine transform".to_owned());
        }
    }
    Ok((found[0], p))
}

/// The body's name: the one text between the presentation envelopes' preset
/// name and the second envelope's GUID, where there is one.
fn name_between_envelopes(d: &[u8], start: usize, end: usize) -> Option<String> {
    let mut texts: Vec<String> = Vec::new();
    let mut p = start;
    while p + 4 < end {
        match text_at(d, p) {
            Some((s, e)) if e <= end => {
                texts.push(s);
                p = e;
            }
            _ => p += 1,
        }
    }
    let libraries: Vec<usize> = texts
        .iter()
        .enumerate()
        .filter(|(_, t)| t.eq_ignore_ascii_case(MATERIAL_LIBRARY))
        .map(|(i, _)| i)
        .collect();
    let [first, second, ..] = libraries[..] else {
        return None;
    };
    // Between the first preset name (after the first library) and the
    // second envelope's GUID (before the second library).
    match texts.get(first + 2..second.checked_sub(1)?)? {
        [name] => Some(name.clone()),
        _ => None,
    }
}

/// `8-4-4-4-12` hex digits.
fn is_guid(b: &[u8]) -> bool {
    b.len() == 36
        && b.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}
