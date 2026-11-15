// SPDX-License-Identifier: MIT
//! The cages a Form starts from (mitcad#1757): a box, a cylinder, a sphere
//! and a plane about a centre, along the axes, and a polygon (a face of the
//! model) as a flat face or a slab. Ids run from 1 in the order the
//! vertices and faces are made; every face is counter-clockwise seen from
//! outside.

use std::collections::BTreeMap;

use mitcad_vec3::{Vec3, add, dot, scale, unit};

use crate::cage::{Cage, CageFace, CageVertex, polygon_normal};

/// The most segments along one side.
pub const MOST_SEGMENTS: u32 = 64;

fn empty() -> Cage {
    Cage {
        vertices: Vec::new(),
        faces: Vec::new(),
        creases: Vec::new(),
        corners: Vec::new(),
        symmetry: None,
        next_vertex: 0,
        next_face: 0,
    }
}

fn positive(what: &str, value: f64) -> Result<f64, String> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(format!("the {what} must be greater than zero"))
    }
}

fn count(what: &str, value: u32, least: u32) -> Result<u32, String> {
    if (least..=MOST_SEGMENTS).contains(&value) {
        Ok(value)
    } else {
        Err(format!(
            "the {what} must be from {least} to {MOST_SEGMENTS}"
        ))
    }
}

fn finite(point: Vec3) -> Result<Vec3, String> {
    if point.iter().all(|c| c.is_finite()) {
        Ok(point)
    } else {
        Err("the centre must be finite".to_owned())
    }
}

/// Vertices on a lattice, made once each.
struct Lattice {
    cage: Cage,
    ids: BTreeMap<[u32; 3], u32>,
}

impl Lattice {
    fn new() -> Self {
        Self {
            cage: empty(),
            ids: BTreeMap::new(),
        }
    }

    fn vertex(&mut self, at: [u32; 3], position: impl Fn([u32; 3]) -> Vec3) -> u32 {
        if let Some(id) = self.ids.get(&at) {
            return *id;
        }
        let id = self.cage.vertices.len() as u32 + 1;
        self.cage.vertices.push(CageVertex {
            id,
            at: position(at),
        });
        self.ids.insert(at, id);
        id
    }

    fn face(&mut self, vertices: Vec<u32>) {
        let id = self.cage.faces.len() as u32 + 1;
        self.cage.faces.push(CageFace { id, vertices });
    }
}

/// The quads of a box's surface, `n` segments along each axis, on the
/// lattice points `(i, j, k)`; `position` places them.
fn box_surface(n: [u32; 3], position: impl Fn([u32; 3]) -> Vec3 + Copy) -> Cage {
    let mut lattice = Lattice::new();
    let [nx, ny, nz] = n;
    let quad = |lattice: &mut Lattice, corners: [[u32; 3]; 4]| {
        let ids = corners.map(|c| lattice.vertex(c, position));
        lattice.face(ids.to_vec());
    };
    // -Z and +Z.
    for i in 0..nx {
        for j in 0..ny {
            quad(
                &mut lattice,
                [[i, j, 0], [i, j + 1, 0], [i + 1, j + 1, 0], [i + 1, j, 0]],
            );
        }
    }
    for i in 0..nx {
        for j in 0..ny {
            quad(
                &mut lattice,
                [
                    [i, j, nz],
                    [i + 1, j, nz],
                    [i + 1, j + 1, nz],
                    [i, j + 1, nz],
                ],
            );
        }
    }
    // -Y and +Y.
    for i in 0..nx {
        for k in 0..nz {
            quad(
                &mut lattice,
                [[i, 0, k], [i + 1, 0, k], [i + 1, 0, k + 1], [i, 0, k + 1]],
            );
        }
    }
    for i in 0..nx {
        for k in 0..nz {
            quad(
                &mut lattice,
                [
                    [i, ny, k],
                    [i, ny, k + 1],
                    [i + 1, ny, k + 1],
                    [i + 1, ny, k],
                ],
            );
        }
    }
    // -X and +X.
    for j in 0..ny {
        for k in 0..nz {
            quad(
                &mut lattice,
                [[0, j, k], [0, j, k + 1], [0, j + 1, k + 1], [0, j + 1, k]],
            );
        }
    }
    for j in 0..ny {
        for k in 0..nz {
            quad(
                &mut lattice,
                [
                    [nx, j, k],
                    [nx, j + 1, k],
                    [nx, j + 1, k + 1],
                    [nx, j, k + 1],
                ],
            );
        }
    }
    lattice.cage
}

/// A box of `size` about `center`, its sides cut into `segments` along x,
/// y and z.
pub fn box_cage(center: Vec3, size: Vec3, segments: [u32; 3]) -> Result<Cage, String> {
    let center = finite(center)?;
    for (axis, s) in ["length", "width", "height"].iter().zip(size) {
        positive(axis, s)?;
    }
    let n = [
        count("segments along x", segments[0], 1)?,
        count("segments along y", segments[1], 1)?,
        count("segments along z", segments[2], 1)?,
    ];
    let position = |at: [u32; 3]| -> Vec3 {
        std::array::from_fn(|k| {
            center[k] - size[k] / 2.0 + size[k] * f64::from(at[k]) / f64::from(n[k])
        })
    };
    Ok(box_surface(n, position))
}

/// A sphere of `diameter` about `center`: a box cage of `segments` a side
/// with its vertices on the sphere.
pub fn sphere_cage(center: Vec3, diameter: f64, segments: u32) -> Result<Cage, String> {
    let center = finite(center)?;
    let radius = positive("diameter", diameter)? / 2.0;
    let s = count("segments", segments, 1)?;
    let position = |at: [u32; 3]| -> Vec3 {
        let cube: Vec3 = std::array::from_fn(|k| -1.0 + 2.0 * f64::from(at[k]) / f64::from(s));
        add(center, scale(unit(cube).unwrap_or([0.0, 0.0, 1.0]), radius))
    };
    Ok(box_surface([s, s, s], position))
}

/// A cylinder of `diameter` and `height` about `center`, its axis along z:
/// `around` vertices round each of `rows + 1` rings, and a cap of
/// `around` corners at each end.
pub fn cylinder_cage(
    center: Vec3,
    diameter: f64,
    height: f64,
    around: u32,
    rows: u32,
) -> Result<Cage, String> {
    let center = finite(center)?;
    let radius = positive("diameter", diameter)? / 2.0;
    let height = positive("height", height)?;
    let m = count("segments round", around, 3)?;
    let rows = count("rows", rows, 1)?;
    let mut cage = empty();
    let id = |ring: u32, k: u32| ring * m + k % m + 1;
    for ring in 0..=rows {
        let z = center[2] - height / 2.0 + height * f64::from(ring) / f64::from(rows);
        for k in 0..m {
            let a = std::f64::consts::TAU * f64::from(k) / f64::from(m);
            cage.vertices.push(CageVertex {
                id: id(ring, k),
                at: [
                    center[0] + radius * a.cos(),
                    center[1] + radius * a.sin(),
                    z,
                ],
            });
        }
    }
    let mut face = |vertices: Vec<u32>| {
        let id = cage.faces.len() as u32 + 1;
        cage.faces.push(CageFace { id, vertices });
    };
    face((0..m).rev().map(|k| id(0, k)).collect());
    face((0..m).map(|k| id(rows, k)).collect());
    for ring in 0..rows {
        for k in 0..m {
            face(vec![
                id(ring, k),
                id(ring, k + 1),
                id(ring + 1, k + 1),
                id(ring + 1, k),
            ]);
        }
    }
    Ok(cage)
}

/// A flat sheet of `size` (along x and y) about `center`, facing +z, cut
/// into `segments`.
pub fn plane_cage(center: Vec3, size: [f64; 2], segments: [u32; 2]) -> Result<Cage, String> {
    let center = finite(center)?;
    positive("length", size[0])?;
    positive("width", size[1])?;
    let nx = count("segments along x", segments[0], 1)?;
    let ny = count("segments along y", segments[1], 1)?;
    let mut cage = empty();
    let id = |i: u32, j: u32| i * (ny + 1) + j + 1;
    for i in 0..=nx {
        for j in 0..=ny {
            cage.vertices.push(CageVertex {
                id: id(i, j),
                at: [
                    center[0] - size[0] / 2.0 + size[0] * f64::from(i) / f64::from(nx),
                    center[1] - size[1] / 2.0 + size[1] * f64::from(j) / f64::from(ny),
                    center[2],
                ],
            });
        }
    }
    for i in 0..nx {
        for j in 0..ny {
            let face_id = cage.faces.len() as u32 + 1;
            cage.faces.push(CageFace {
                id: face_id,
                vertices: vec![id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)],
            });
        }
    }
    Ok(cage)
}

/// A polygon (a face of the model, its outline's points in order) facing
/// `normal`: one face, or with a `thickness` a slab standing on it, its
/// top `thickness` along the normal and its sides one quad an edge.
pub fn polygon_cage(points: &[Vec3], normal: Vec3, thickness: Option<f64>) -> Result<Cage, String> {
    if points.len() < 3 {
        return Err("the outline has fewer than three points".to_owned());
    }
    if !points.iter().all(|p| p.iter().all(|c| c.is_finite())) {
        return Err("the outline's points must be finite".to_owned());
    }
    let normal = unit(normal).ok_or("the face has no normal")?;
    let (own, _) = polygon_normal(points);
    let mut outline = points.to_vec();
    if dot(own, normal) < 0.0 {
        outline.reverse();
    }
    let n = outline.len() as u32;
    let mut cage = empty();
    for (k, p) in outline.iter().enumerate() {
        cage.vertices.push(CageVertex {
            id: k as u32 + 1,
            at: *p,
        });
    }
    let Some(thickness) = thickness else {
        cage.faces.push(CageFace {
            id: 1,
            vertices: (1..=n).collect(),
        });
        return Ok(cage);
    };
    let thickness = positive("thickness", thickness)?;
    for (k, p) in outline.iter().enumerate() {
        cage.vertices.push(CageVertex {
            id: n + k as u32 + 1,
            at: add(*p, scale(normal, thickness)),
        });
    }
    // The bottom faces into the face it stands on, the top away.
    cage.faces.push(CageFace {
        id: 1,
        vertices: (1..=n).rev().collect(),
    });
    cage.faces.push(CageFace {
        id: 2,
        vertices: (n + 1..=2 * n).collect(),
    });
    for k in 0..n {
        let next = (k + 1) % n;
        cage.faces.push(CageFace {
            id: 3 + k,
            vertices: vec![k + 1, next + 1, n + next + 1, n + k + 1],
        });
    }
    Ok(cage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitcad_vec3::sub;

    /// The centre of the box around a set of points.
    fn middle(points: &[Vec3]) -> Vec3 {
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for p in points {
            low = std::array::from_fn(|k| low[k].min(p[k]));
            high = std::array::from_fn(|k| high[k].max(p[k]));
        }
        scale(add(low, high), 0.5)
    }

    #[test]
    fn primitives_are_valid_cages() {
        let cages = [
            box_cage([1.0, 2.0, 3.0], [30.0, 20.0, 10.0], [3, 2, 1]).unwrap(),
            sphere_cage([0.0; 3], 50.0, 2).unwrap(),
            cylinder_cage([0.0; 3], 20.0, 40.0, 8, 3).unwrap(),
            plane_cage([0.0; 3], [30.0, 20.0], [3, 2]).unwrap(),
            polygon_cage(
                &[
                    [0.0, 0.0, 0.0],
                    [10.0, 0.0, 0.0],
                    [10.0, 5.0, 0.0],
                    [0.0, 5.0, 0.0],
                ],
                [0.0, 0.0, 1.0],
                Some(2.0),
            )
            .unwrap(),
            polygon_cage(
                &[[0.0, 0.0, 0.0], [0.0, 5.0, 0.0], [10.0, 5.0, 0.0]],
                [0.0, 0.0, -1.0],
                None,
            )
            .unwrap(),
        ];
        for cage in &cages {
            cage.check().unwrap();
        }
        // Closed but the plane and the polygon without a thickness.
        let closed: Vec<bool> = cages.iter().map(|c| c.full().closed()).collect();
        assert_eq!(closed, [true, true, true, false, true, false]);
        assert_eq!(cages[0].faces.len(), 2 * (6 + 3 + 2));
        assert_eq!(cages[2].faces.len(), 2 + 8 * 3);
        // Faces point out of the solid ones.
        for cage in &cages[..3] {
            let center = middle(&cage.vertices.iter().map(|v| v.at).collect::<Vec<_>>());
            for face in &cage.faces {
                let (normal, at) = cage.face_normal(face);
                assert!(dot(normal, sub(at, center)) > 0.0, "{face:?}");
            }
        }
        // The slab's top is 2 above its outline, the bare polygon turned to
        // face down.
        assert!(cages[4].vertices.iter().any(|v| v.at == [10.0, 5.0, 2.0]));
        let (normal, _) = cages[5].face_normal(&cages[5].faces[0]);
        assert!(normal[2] < 0.0);
    }

    #[test]
    fn sizes_and_segments_are_checked() {
        assert!(box_cage([0.0; 3], [0.0, 1.0, 1.0], [1, 1, 1]).is_err());
        assert!(box_cage([0.0; 3], [1.0, 1.0, 1.0], [0, 1, 1]).is_err());
        assert!(cylinder_cage([0.0; 3], 10.0, 10.0, 2, 1).is_err());
        assert!(sphere_cage([0.0; 3], -1.0, 2).is_err());
        assert!(polygon_cage(&[[0.0; 3], [1.0, 0.0, 0.0]], [0.0, 0.0, 1.0], None).is_err());
    }
}
