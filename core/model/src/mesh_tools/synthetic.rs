// SPDX-License-Identifier: MIT
//! Synthetic meshes for the mesh tools' tests (mitcad#1871): a box and a
//! cylinder as a scanner gives them, finely triangulated, closed, facing
//! out, every vertex moved by a little noise (deterministic, up to `noise`
//! from where it belongs).

use std::collections::HashMap;

use super::{Point, TriangleMesh};

/// A point moved by up to `amplitude` in a direction that depends only on
/// `seed`.
pub(crate) fn jitter(p: Point, seed: u64, amplitude: f64) -> Point {
    if amplitude == 0.0 {
        return p;
    }
    let mut state = seed
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(0x2545_f491_4f6c_dd1d);
    let mut next = || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    let d = [next(), next(), next()];
    let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1.0);
    let s = amplitude / length;
    [p[0] + d[0] * s, p[1] + d[1] * s, p[2] + d[2] * s]
}

/// A box from the origin to `size`, each face a grid of `divisions` by
/// `divisions` squares of two triangles, its vertices shared.
pub(crate) fn noisy_box(size: Point, divisions: usize, noise: f64) -> TriangleMesh {
    let n = divisions.max(1);
    let mut mesh = TriangleMesh::default();
    let mut index: HashMap<[i64; 3], u32> = HashMap::new();
    let mut vertex = |mesh: &mut TriangleMesh, g: [i64; 3]| -> u32 {
        *index.entry(g).or_insert_with(|| {
            let p = std::array::from_fn(|i| size[i] * g[i] as f64 / n as f64);
            let seed = (g[0] as u64) * 1_000_003 + (g[1] as u64) * 1009 + g[2] as u64;
            mesh.vertices.push(jitter(p, seed, noise));
            (mesh.vertices.len() - 1) as u32
        })
    };
    let n = n as i64;
    // Each face: its corner and two grid directions whose cross product
    // points out.
    let faces: [([i64; 3], [i64; 3], [i64; 3]); 6] = [
        ([0, 0, 0], [0, 1, 0], [1, 0, 0]),
        ([0, 0, n], [1, 0, 0], [0, 1, 0]),
        ([0, 0, 0], [1, 0, 0], [0, 0, 1]),
        ([0, n, 0], [0, 0, 1], [1, 0, 0]),
        ([0, 0, 0], [0, 0, 1], [0, 1, 0]),
        ([n, 0, 0], [0, 1, 0], [0, 0, 1]),
    ];
    for (origin, du, dv) in faces {
        for i in 0..n {
            for j in 0..n {
                let at = |a: i64, b: i64| -> [i64; 3] {
                    std::array::from_fn(|k| origin[k] + du[k] * a + dv[k] * b)
                };
                let p00 = vertex(&mut mesh, at(i, j));
                let p10 = vertex(&mut mesh, at(i + 1, j));
                let p11 = vertex(&mut mesh, at(i + 1, j + 1));
                let p01 = vertex(&mut mesh, at(i, j + 1));
                mesh.triangles.push([p00, p10, p11]);
                mesh.triangles.push([p00, p11, p01]);
            }
        }
    }
    mesh
}

/// A closed cylinder about the Z axis from z 0 to `height`: `segments`
/// around, `rings` rows of quads up its side, each cap a fan to its centre.
pub(crate) fn noisy_cylinder(
    radius: f64,
    height: f64,
    segments: usize,
    rings: usize,
    noise: f64,
) -> TriangleMesh {
    let mut mesh = TriangleMesh::default();
    let mut seed = 7u64;
    let mut push = |mesh: &mut TriangleMesh, p: Point| -> u32 {
        seed += 1;
        mesh.vertices.push(jitter(p, seed, noise));
        (mesh.vertices.len() - 1) as u32
    };
    let mut grid = vec![vec![0u32; segments]; rings + 1];
    for (j, row) in grid.iter_mut().enumerate() {
        let z = height * j as f64 / rings as f64;
        for (i, slot) in row.iter_mut().enumerate() {
            let a = std::f64::consts::TAU * i as f64 / segments as f64;
            *slot = push(&mut mesh, [radius * a.cos(), radius * a.sin(), z]);
        }
    }
    for j in 0..rings {
        for i in 0..segments {
            let next = (i + 1) % segments;
            let (a, b, c, d) = (grid[j][i], grid[j][next], grid[j + 1][next], grid[j + 1][i]);
            mesh.triangles.push([a, b, c]);
            mesh.triangles.push([a, c, d]);
        }
    }
    let bottom = push(&mut mesh, [0.0, 0.0, 0.0]);
    let top = push(&mut mesh, [0.0, 0.0, height]);
    for i in 0..segments {
        let next = (i + 1) % segments;
        mesh.triangles.push([bottom, grid[0][next], grid[0][i]]);
        mesh.triangles
            .push([top, grid[rings][i], grid[rings][next]]);
    }
    mesh
}

/// A closed sphere about `centre` (mitcad#2515): `rings` rows from pole to
/// pole, `segments` around, the poles single vertices; every vertex but
/// the poles moved by up to `noise`.
pub(crate) fn noisy_sphere(
    centre: Point,
    radius: f64,
    segments: usize,
    rings: usize,
    noise: f64,
) -> TriangleMesh {
    let mut mesh = TriangleMesh::default();
    mesh.vertices
        .push([centre[0], centre[1], centre[2] + radius]);
    let mut grid = vec![vec![0u32; segments]; rings - 1];
    for (j, row) in grid.iter_mut().enumerate() {
        let polar = std::f64::consts::PI * (j + 1) as f64 / rings as f64;
        for (i, slot) in row.iter_mut().enumerate() {
            let a = std::f64::consts::TAU * i as f64 / segments as f64;
            let p = [
                centre[0] + radius * polar.sin() * a.cos(),
                centre[1] + radius * polar.sin() * a.sin(),
                centre[2] + radius * polar.cos(),
            ];
            let seed = 31 + (j * segments + i) as u64;
            mesh.vertices.push(jitter(p, seed, noise));
            *slot = (mesh.vertices.len() - 1) as u32;
        }
    }
    mesh.vertices
        .push([centre[0], centre[1], centre[2] - radius]);
    let south = (mesh.vertices.len() - 1) as u32;
    for i in 0..segments {
        let next = (i + 1) % segments;
        mesh.triangles.push([0, grid[0][i], grid[0][next]]);
        for j in 0..rings - 2 {
            let (a, b, c, d) = (grid[j][i], grid[j + 1][i], grid[j + 1][next], grid[j][next]);
            mesh.triangles.push([a, b, c]);
            mesh.triangles.push([a, c, d]);
        }
        mesh.triangles
            .push([south, grid[rings - 2][next], grid[rings - 2][i]]);
    }
    mesh
}

/// A closed torus about the Z axis (mitcad#2515): the tube of radius
/// `minor` round a circle of radius `major` in XY, `segments` around the
/// axis, `sides` around the tube.
pub(crate) fn torus(major: f64, minor: f64, segments: usize, sides: usize) -> TriangleMesh {
    let mut mesh = TriangleMesh::default();
    for i in 0..segments {
        let phi = std::f64::consts::TAU * i as f64 / segments as f64;
        for j in 0..sides {
            let theta = std::f64::consts::TAU * j as f64 / sides as f64;
            let ring = major + minor * theta.cos();
            mesh.vertices
                .push([ring * phi.cos(), ring * phi.sin(), minor * theta.sin()]);
        }
    }
    let at = |i: usize, j: usize| ((i % segments) * sides + j % sides) as u32;
    for i in 0..segments {
        for j in 0..sides {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            mesh.triangles.push([a, b, c]);
            mesh.triangles.push([a, c, d]);
        }
    }
    mesh
}

/// The triangles of the mesh each with its own three vertices, as an STL
/// file gives them.
pub(crate) fn unshared(mesh: &TriangleMesh) -> TriangleMesh {
    let mut out = TriangleMesh::default();
    for tri in &mesh.triangles {
        let base = out.vertices.len() as u32;
        for &v in tri {
            out.vertices.push(mesh.vertices[v as usize]);
        }
        out.triangles.push([base, base + 1, base + 2]);
    }
    out
}
