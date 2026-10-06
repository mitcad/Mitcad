// SPDX-License-Identifier: MIT
//! STL files of triangle meshes (mitcad#17): binary, or text with
//! `ascii`, in millimetres. The model writes them from the kernel's meshes
//! ([`crate::Kernel::triangle_mesh`]), placed where the design shows the
//! bodies, as it writes 3MF files.

use std::fmt::Write as _;

use super::TriangleMesh;

/// The bytes of an STL file of the mesh: one solid named `name`, each
/// triangle with its normal from its vertices' order (counter-clockwise
/// seen from outside).
pub(super) fn write(mesh: &TriangleMesh, name: &str, ascii: bool) -> Vec<u8> {
    let corners = |t: &[u32; 3]| t.map(|i| mesh.vertices[i as usize]);
    if ascii {
        // A name of one line, as readers take the rest of the first line.
        let name: String = name
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let mut text = String::new();
        let _ = writeln!(text, "solid {name}");
        for t in &mesh.triangles {
            let [a, b, c] = corners(t);
            let n = normal(a, b, c);
            let _ = writeln!(text, "  facet normal {:e} {:e} {:e}", n[0], n[1], n[2]);
            text.push_str("    outer loop\n");
            for p in [a, b, c] {
                let _ = writeln!(text, "      vertex {:e} {:e} {:e}", p[0], p[1], p[2]);
            }
            text.push_str("    endloop\n  endfacet\n");
        }
        let _ = writeln!(text, "endsolid {name}");
        return text.into_bytes();
    }
    // An 80-byte header that does not start with "solid" (text STL), the
    // count, then 50 bytes per triangle.
    let mut data = Vec::with_capacity(84 + 50 * mesh.triangles.len());
    let mut header = format!("Mitcad {} binary STL", env!("CARGO_PKG_VERSION")).into_bytes();
    header.resize(80, b' ');
    data.extend_from_slice(&header);
    let count = u32::try_from(mesh.triangles.len()).unwrap_or(u32::MAX);
    data.extend_from_slice(&count.to_le_bytes());
    for t in mesh.triangles.iter().take(count as usize) {
        let [a, b, c] = corners(t);
        for v in [normal(a, b, c), a, b, c] {
            for x in v {
                data.extend_from_slice(&(x as f32).to_le_bytes());
            }
        }
        data.extend_from_slice(&0u16.to_le_bytes());
    }
    data
}

/// The unit normal of a triangle; zero when it has no area.
fn normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if length > 0.0 {
        n.map(|x| x / length)
    } else {
        [0.0; 3]
    }
}

/// The triangles of an STL file (binary or text), for tests.
#[cfg(test)]
pub(super) fn read(data: &[u8]) -> Vec<[[f64; 3]; 3]> {
    if data.starts_with(b"solid ") {
        let text = std::str::from_utf8(data).unwrap();
        let points: Vec<[f64; 3]> = text
            .lines()
            .filter_map(|line| line.trim().strip_prefix("vertex "))
            .map(|p| {
                let v: Vec<f64> = p.split_whitespace().map(|x| x.parse().unwrap()).collect();
                [v[0], v[1], v[2]]
            })
            .collect();
        return points.chunks(3).map(|t| [t[0], t[1], t[2]]).collect();
    }
    let count = u32::from_le_bytes(data[80..84].try_into().unwrap()) as usize;
    assert_eq!(data.len(), 84 + 50 * count, "the size of a binary STL file");
    let float = |at: usize| f64::from(f32::from_le_bytes(data[at..at + 4].try_into().unwrap()));
    (0..count)
        .map(|i| {
            let at = 84 + 50 * i + 12; // after the normal
            std::array::from_fn(|v| std::array::from_fn(|x| float(at + 12 * v + 4 * x)))
        })
        .collect()
}
