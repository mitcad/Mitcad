// SPDX-License-Identifier: MIT
//! A triangle mesh of the limit surface for drawing a Form while its cage
//! is edited (mitcad#1757), made without the geometry kernel: the cage
//! subdivided a few times, its vertices at their limit positions, each
//! quad two triangles.

use mitcad_vec3::{Vec3, add, cross, sub, unit};

use crate::cage::{Cage, Full};
use crate::mesh::{limit_points, subdivide};

/// The most steps the preview takes (each makes four times the triangles).
pub const MOST_LEVELS: usize = 5;

/// Triangles on the limit surface, each with the cage face it lies on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LimitMesh {
    pub positions: Vec<Vec3>,
    /// Unit normals at the positions (zero where there is none).
    pub normals: Vec<Vec3>,
    /// Counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
    /// The cage face of each triangle, by its id in the whole cage.
    pub faces: Vec<i64>,
}

/// The limit mesh after `levels` steps (1 to [`MOST_LEVELS`]); Err when
/// the cage is not valid.
pub fn limit_mesh(cage: &Cage, levels: usize) -> Result<LimitMesh, String> {
    cage.check()?;
    let Full {
        mut mesh, face_ids, ..
    } = cage.full();
    let mut origin = face_ids;
    for _ in 0..levels.clamp(1, MOST_LEVELS) {
        let (finer, step) = subdivide(&mesh);
        let mut next = vec![0; finer.faces.len()];
        for (f, face) in mesh.faces.iter().enumerate() {
            for k in 0..face.len() {
                next[step.child_start[f] + k] = origin[f];
            }
        }
        origin = next;
        mesh = finer;
    }
    let positions = limit_points(&mesh);
    let mut triangles = Vec::with_capacity(2 * mesh.faces.len());
    let mut faces = Vec::with_capacity(2 * mesh.faces.len());
    let mut sums = vec![[0.0; 3]; positions.len()];
    for (f, quad) in mesh.faces.iter().enumerate() {
        let q: Vec<u32> = quad.iter().map(|v| *v as u32).collect();
        for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            let [a, b, c] = t.map(|v| positions[v as usize]);
            let n = cross(sub(b, a), sub(c, a));
            for v in t {
                sums[v as usize] = add(sums[v as usize], n);
            }
            triangles.push(t);
            faces.push(origin[f]);
        }
    }
    let normals = sums
        .into_iter()
        .map(|n| unit(n).unwrap_or([0.0; 3]))
        .collect();
    Ok(LimitMesh {
        positions,
        normals,
        triangles,
        faces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::box_cage;
    use mitcad_vec3::{dot, norm};

    #[test]
    fn a_box_cage_previews_as_a_closed_round_mesh() {
        let cage = box_cage([0.0; 3], [40.0, 40.0, 40.0], [1, 1, 1]).unwrap();
        let mesh = limit_mesh(&cage, 3).unwrap();
        // 6 faces, 4 quads each a step, 2 triangles a quad.
        assert_eq!(mesh.triangles.len(), 6 * 64 * 2);
        assert_eq!(mesh.faces.len(), mesh.triangles.len());
        assert!(mesh.faces.iter().all(|f| (1..=6).contains(f)));
        // Round: every point well inside the cage, normals pointing out.
        for (p, n) in mesh.positions.iter().zip(&mesh.normals) {
            assert!(norm(*p) < 20.0 * 3f64.sqrt());
            assert!(dot(*p, *n) > 0.0);
        }
    }
}
