// SPDX-License-Identifier: MIT
//! Mesh bodies: a mesh feature brings triangles into a part (a mesh read
//! from another file); the part keeps them in its graphics segment
//! (`PmGraphicsSegment`), not in the B-rep record. See `README.md` (*Mesh
//! features*).
//!
//! A mesh is a record of vertices (`02adf9de…`: f32 x, y, z in cm) and the
//! record of triangles after it (`03adf9de…`: u32 vertex indices, three a
//! triangle, counter-clockwise seen from outside); normals and their
//! indices follow (not read). Both are a six-byte head (u32, u16) and a
//! list (kind 2) whose second u32 is the item count again and the third an
//! item code (279 for points, 0 for indices).

use crate::{IptError, IptFile};

/// The record type of a mesh feature in the definitions segment, as
/// stored.
pub const MESH_FEATURE_TYPE: [u8; 16] = [
    0x66, 0x4C, 0x2D, 0xEC, 0xEA, 0x4A, 0x9C, 0xBA, 0x24, 0xC8, 0x42, 0x99, 0xA4, 0xCE, 0x8A, 0x9D,
];

/// The record type of a mesh's vertices in the graphics segment.
pub const VERTICES_TYPE: [u8; 16] = [
    0x02, 0xAD, 0xF9, 0xDE, 0xD4, 0x11, 0xB2, 0x94, 0x10, 0x00, 0xFB, 0x8D, 0xF8, 0xBB, 0x47, 0xB5,
];

/// The record type of a mesh's triangles (vertex indices).
pub const TRIANGLES_TYPE: [u8; 16] = [
    0x03, 0xAD, 0xF9, 0xDE, 0xD4, 0x11, 0xB2, 0x94, 0x10, 0x00, 0xFB, 0x8D, 0xF8, 0xBB, 0x47, 0xB5,
];

/// Bytes before a mesh list's items: the head (u32, u16) and the list's
/// kind, mark, count, count again and item code.
const LIST_HEAD: usize = 22;

/// A mesh body of the part.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    /// The graphics segment's record of its vertices.
    pub record: usize,
    /// cm, in the part's coordinates.
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    /// Where the mesh comes from, for reports: `PmGraphicsSegment#232`.
    pub fn place(&self) -> String {
        format!("PmGraphicsSegment#{}", self.record)
    }

    /// The vertices in millimetres, x, y, z after each other.
    pub fn vertices_mm(&self) -> Vec<f64> {
        self.vertices
            .iter()
            .flat_map(|v| v.map(|c| 10.0 * c))
            .collect()
    }

    /// The enclosed volume (mm³; for a closed mesh) and the area (mm²).
    pub fn volume_area(&self) -> (f64, f64) {
        let mut volume = 0.0;
        let mut area = 0.0;
        for t in &self.triangles {
            let [a, b, c] = t.map(|i| self.vertices[i as usize].map(|x| 10.0 * x));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                u[1] * w[2] - u[2] * w[1],
                u[2] * w[0] - u[0] * w[2],
                u[0] * w[1] - u[1] * w[0],
            ];
            area += 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            volume += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0;
        }
        (volume, area)
    }

    /// Every edge is shared by two triangles.
    pub fn is_closed(&self) -> bool {
        let mut edges: std::collections::HashMap<(u32, u32), usize> =
            std::collections::HashMap::new();
        for t in &self.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        !edges.is_empty() && edges.values().all(|&n| n == 2)
    }
}

/// The items of a mesh list: their count and the bytes after the head.
fn list(bytes: &[u8], item: usize) -> Result<(usize, &[u8]), String> {
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    if bytes.len() < LIST_HEAD {
        return Err(format!("{} bytes, no list", bytes.len()));
    }
    if u16_at(6) != 2 || u16_at(8) != 0x3000 {
        return Err("no list of kind 2".into());
    }
    let count = u32_at(10) as usize;
    if u32_at(14) as usize != count {
        return Err(format!("a list of {count} items, then {}", u32_at(14)));
    }
    let items = bytes[LIST_HEAD..]
        .get(..count.saturating_mul(item))
        .ok_or_else(|| format!("{count} items of {item} bytes in {} bytes", bytes.len()))?;
    Ok((count, items))
}

/// The vertices of a vertex record (cm).
pub fn vertices(bytes: &[u8]) -> Result<Vec<[f64; 3]>, String> {
    let (_, items) = list(bytes, 12)?;
    Ok(items
        .as_chunks::<12>()
        .0
        .iter()
        .map(|c| {
            let f = |k: usize| f64::from(f32::from_le_bytes([c[k], c[k + 1], c[k + 2], c[k + 3]]));
            [f(0), f(4), f(8)]
        })
        .collect())
}

/// The triangles of a triangle record, checked against the vertex count.
pub fn triangles(bytes: &[u8], vertices: usize) -> Result<Vec<[u32; 3]>, String> {
    let (count, items) = list(bytes, 4)?;
    if count % 3 != 0 {
        return Err(format!("{count} vertex indices, not triangles"));
    }
    let indices: Vec<u32> = items
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    if let Some(i) = indices.iter().find(|&&i| i as usize >= vertices) {
        return Err(format!("vertex index {i} of {vertices} vertices"));
    }
    Ok(indices.as_chunks::<3>().0.to_vec())
}

impl IptFile {
    /// Whether the definitions segment has a mesh feature.
    pub fn has_mesh_features(&self) -> Result<bool, IptError> {
        let Some(segment) = self.segments.iter().find(|s| s.name == "PmDCSegment") else {
            return Ok(false);
        };
        let content = self.segment_data(segment)?;
        Ok(match &content.records {
            Ok(records) => records
                .iter()
                .any(|r| content.tables.types[r.type_index()].id == MESH_FEATURE_TYPE),
            Err(_) => false,
        })
    }

    /// The mesh bodies of the part's mesh features: each vertex record of
    /// the graphics segment with the triangle record after it. None when
    /// the part has no mesh feature (the graphics segment's other records
    /// are the display of the B-rep bodies); meshes that cannot be read
    /// are errors of their own.
    pub fn meshes(&self) -> Result<Vec<Result<Mesh, String>>, IptError> {
        if !self.has_mesh_features()? {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for segment in self
            .segments
            .iter()
            .filter(|s| s.name == "PmGraphicsSegment")
        {
            let content = self.segment_data(segment)?;
            let records = content
                .records
                .map_err(|e| IptError::Segment(format!("{}: {e}", segment.name)))?;
            let type_of = |r: &crate::rse::Record| content.tables.types[r.type_index()].id;
            for (k, r) in records.iter().enumerate() {
                if type_of(r) != VERTICES_TYPE {
                    continue;
                }
                let place = format!("{}#{}", segment.name, r.index);
                let mesh = vertices(&content.data[r.range.clone()]).and_then(|vertices| {
                    let t = records[k + 1..]
                        .iter()
                        .take_while(|t| type_of(t) != VERTICES_TYPE)
                        .find(|t| type_of(t) == TRIANGLES_TYPE)
                        .ok_or("no triangles after the vertices")?;
                    let triangles = triangles(&content.data[t.range.clone()], vertices.len())?;
                    Ok(Mesh {
                        record: r.index,
                        vertices,
                        triangles,
                    })
                });
                out.push(mesh.map_err(|e| format!("{place}: {e}")));
            }
        }
        Ok(out)
    }
}

/// A vertex record of these vertices (cm), for tests.
pub fn write_vertices(vertices: &[[f64; 3]]) -> Vec<u8> {
    let mut b = list_head(vertices.len(), 279);
    for v in vertices {
        for c in v {
            b.extend_from_slice(&(*c as f32).to_le_bytes());
        }
    }
    b.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 6, 0, 0, 0x30, 0, 0, 0, 0]);
    b
}

/// A triangle record of these triangles, for tests.
pub fn write_triangles(triangles: &[[u32; 3]]) -> Vec<u8> {
    let mut b = list_head(3 * triangles.len(), 0);
    for i in triangles.iter().flatten() {
        b.extend_from_slice(&i.to_le_bytes());
    }
    b.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 6, 0, 0, 0x30, 0, 0, 0, 0]);
    b
}

fn list_head(count: usize, code: u32) -> Vec<u8> {
    let mut b = vec![0, 0, 0, 0, 0xCB, 0, 2, 0, 0, 0x30];
    b.extend_from_slice(&(count as u32).to_le_bytes());
    b.extend_from_slice(&(count as u32).to_le_bytes());
    b.extend_from_slice(&code.to_le_bytes());
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_written_mesh_records() {
        let v = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.5, 0.0],
            [0.0, 0.0, 2.0],
        ];
        let t = [[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]];
        assert_eq!(vertices(&write_vertices(&v)).unwrap(), v);
        assert_eq!(triangles(&write_triangles(&t), 4).unwrap(), t);
        assert!(triangles(&write_triangles(&t), 3).is_err());
        let mut short = write_vertices(&v);
        short.truncate(30);
        assert!(vertices(&short).is_err());
        assert!(vertices(&[0; 10]).is_err());
    }

    #[test]
    fn reads_the_meshes_of_a_mesh_part() {
        let f = IptFile::parse(crate::testdata::test_mesh_part()).unwrap();
        assert!(f.has_mesh_features().unwrap());
        let meshes = f.meshes().unwrap();
        assert_eq!(meshes.len(), 1);
        let mesh = meshes[0].as_ref().unwrap();
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.triangles.len(), 4);
        assert_eq!(mesh.place(), "PmGraphicsSegment#1");
        assert!(mesh.is_closed());
        // A tetrahedron of 10, 15 and 20 mm legs: 500 mm³.
        let (volume, area) = mesh.volume_area();
        assert!((volume - 500.0).abs() < 1e-9, "{volume}");
        let slant = 0.5 * (300.0f64.powi(2) + 200.0f64.powi(2) + 150.0f64.powi(2)).sqrt();
        assert!(
            (area - (75.0 + 100.0 + 150.0 + slant)).abs() < 1e-9,
            "{area}"
        );
        assert_eq!(&mesh.vertices_mm()[..6], &[0.0, 0.0, 0.0, 10.0, 0.0, 0.0]);
        // Its B-rep record has no bodies.
        let records = f.brep_records().unwrap();
        let blob = crate::read_bodies(&records).remove(0).unwrap();
        assert!(blob.bodies.is_empty());
        // A part without a mesh feature has no meshes.
        let f = IptFile::parse(crate::testdata::test_part()).unwrap();
        assert!(!f.has_mesh_features().unwrap());
        assert!(f.meshes().unwrap().is_empty());
    }
}
