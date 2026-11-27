// SPDX-License-Identifier: MIT
//! Offset and Shell of a mesh (mitcad#2515).
//!
//! Each vertex moves so that the planes of its triangles move by the
//! distance: the least-squares solution of `n · v = d` over its triangles'
//! normals (each weighted by the triangle's angle at the vertex), with the
//! directions its normals do not fix (along a flat or a gently curved
//! surface, along a sharp edge) left out. On a flat face a vertex moves
//! along the normal, at a box's edge and corner so that each face moves by
//! the distance (the corners stay sharp), on a smooth surface about along
//! its normal. A move longer than [`MAX_STRETCH`] times the distance (at a
//! needle-sharp corner) is cut back to that.
//!
//! - [`offset`]: the surface moved by a distance (outward positive), the
//!   triangles as they were;
//! - [`wall`]: the closed wall between two offsets of the surface (the
//!   inner and the outer side), with openings: triangles taken out first,
//!   the rims they and an open mesh's edges leave joined across the wall.
//!
//! Where the moved triangles turn over (an offset towards the inside of a
//! sharp edge farther than the triangles beside it are wide, or where the
//! surface curves tighter than the distance), a closed mesh's offset, or
//! the side of its wall that turned, is built again from the distance to
//! the mesh ([`super::offset_field`], mitcad#2564): the parts that turned
//! over are trimmed away. That offset is refused only where the mesh's
//! sides come closer than the distance allows (it would split apart, close
//! or open a hole, or leave nothing): counted ([`Offset::folded`]) with
//! the places, as the triangles that turn over on an open mesh, whose
//! offset is not trimmed.

use std::collections::HashMap;

use mitcad_vec3::{add, dot, norm, scale, sub};

use super::offset_field::{self, Shape};
use super::{Point, TriangleMesh, area_normal, corners, edge_key, edge_triangles, submesh_map};

/// The longest move, in distances, a vertex makes.
pub const MAX_STRETCH: f64 = 4.0;

/// Directions in which a vertex's normals spread less than this share of
/// the most (eigenvalues of their weighted scatter) are left free.
const FIXED_SHARE: f64 = 0.02;

/// Each vertex's offset direction: the move that offsets its triangles'
/// planes by one (outwards); zero for a vertex of no triangle.
pub(crate) fn offset_directions(mesh: &TriangleMesh) -> Vec<Point> {
    let n = mesh.vertices.len();
    let mut scatter = vec![[[0.0f64; 3]; 3]; n];
    let mut sums = vec![[0.0f64; 3]; n];
    for (t, tri) in mesh.triangles.iter().enumerate() {
        let p = corners(mesh, t);
        let Some(normal) = mitcad_vec3::unit(area_normal(&p)) else {
            continue;
        };
        for k in 0..3 {
            let a = sub(p[(k + 1) % 3], p[k]);
            let b = sub(p[(k + 2) % 3], p[k]);
            let (la, lb) = (norm(a), norm(b));
            if la <= 0.0 || lb <= 0.0 {
                continue;
            }
            let angle = (dot(a, b) / (la * lb)).clamp(-1.0, 1.0).acos();
            let v = tri[k] as usize;
            for i in 0..3 {
                for j in 0..3 {
                    scatter[v][i][j] += angle * normal[i] * normal[j];
                }
                sums[v][i] += angle * normal[i];
            }
        }
    }
    (0..n)
        .map(|v| {
            let (values, vectors) = super::symmetric_eigen(scatter[v]);
            let largest = values[2];
            if largest <= 0.0 {
                return [0.0; 3];
            }
            let mut move_by = [0.0; 3];
            for k in 0..3 {
                if values[k] > FIXED_SHARE * largest {
                    let along = dot(vectors[k], sums[v]) / values[k];
                    move_by = add(move_by, scale(vectors[k], along));
                }
            }
            let length = norm(move_by);
            if length > MAX_STRETCH {
                scale(move_by, MAX_STRETCH / length)
            } else {
                move_by
            }
        })
        .collect()
}

/// What the places of an offset that cannot be made are (mitcad#2564).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overreach {
    /// Triangles turned over by the move (an open mesh, whose offset is
    /// not trimmed).
    #[default]
    TurnedOver,
    /// Places where a closed mesh's sides come closer than the distance
    /// allows: its trimmed offset would split apart, close or open a hole,
    /// or leave nothing.
    TooThin,
}

/// An offset surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Offset {
    pub mesh: TriangleMesh,
    /// Where the offset cannot be made ([`Offset::overreach`] says how):
    /// a count, with a point of each (the first
    /// [`super::check::MAX_PLACES`]); 0 when it is made.
    pub folded: usize,
    pub folded_places: Vec<Point>,
    pub overreach: Overreach,
    /// Built from the distance to the mesh, the parts of the moved
    /// surface that turned over trimmed away (mitcad#2564).
    pub trimmed: bool,
}

/// The surface moved by `distance` (outward positive), its triangles as
/// they were; `mesh` with its shared corners joined. Where they would
/// turn over, a closed mesh's offset is built from its distance instead
/// ([`super::offset_field`]).
pub fn offset(mesh: &TriangleMesh, distance: f64) -> Offset {
    let directions = offset_directions(mesh);
    let moved = offset_with(mesh, &directions, distance);
    if moved.folded == 0 || !offset_field::closed_solid(mesh) {
        return moved;
    }
    let built = offset_field::offset_surface(mesh, distance);
    trimmed(built, mesh, offset_field::shape(mesh), distance)
}

/// An offset built from the distance to `surface`, refused where it does
/// not keep the surface's shape (`expected`).
fn trimmed(built: TriangleMesh, surface: &TriangleMesh, expected: Shape, distance: f64) -> Offset {
    if !built.triangles.is_empty() && offset_field::shape(&built) == expected {
        return Offset {
            mesh: built,
            folded: 0,
            folded_places: Vec::new(),
            overreach: Overreach::default(),
            trimmed: true,
        };
    }
    let (count, places) = offset_field::thin_places(surface, distance);
    Offset {
        mesh: built,
        folded: count.max(1),
        folded_places: places,
        overreach: Overreach::TooThin,
        trimmed: true,
    }
}

fn offset_with(mesh: &TriangleMesh, directions: &[Point], distance: f64) -> Offset {
    let moved = TriangleMesh {
        vertices: mesh
            .vertices
            .iter()
            .zip(directions)
            .map(|(p, d)| add(*p, scale(*d, distance)))
            .collect(),
        triangles: mesh.triangles.clone(),
    };
    let mut folded = 0;
    let mut folded_places = Vec::new();
    for t in 0..mesh.triangles.len() {
        let before_corners = corners(mesh, t);
        let after_corners = corners(&moved, t);
        let before = area_normal(&before_corners);
        let after = area_normal(&after_corners);
        // Turned over: its normal against the original's, or an edge
        // pointing back (a box offset inward past its middle keeps its
        // faces' normals, every edge reversed).
        let reversed = (0..3).any(|k| {
            let was = sub(before_corners[(k + 1) % 3], before_corners[k]);
            let now = sub(after_corners[(k + 1) % 3], after_corners[k]);
            dot(was, now) < 0.0
        });
        if dot(before, after) < 0.0 || reversed {
            folded += 1;
            if folded_places.len() < super::check::MAX_PLACES {
                folded_places.push(super::centroid(&after_corners));
            }
        }
    }
    Offset {
        mesh: moved,
        folded,
        folded_places,
        overreach: Overreach::TurnedOver,
        trimmed: false,
    }
}

/// A wall of a surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Wall {
    pub mesh: TriangleMesh,
    /// Where either side cannot be made, with the places (as
    /// [`Offset::folded`]); 0 when the wall is made.
    pub folded: usize,
    pub folded_places: Vec<Point>,
    pub overreach: Overreach,
    /// A side built from the distance to the mesh, trimmed where its moved
    /// triangles turned over (mitcad#2564).
    pub trimmed: bool,
    /// Rims joined across the wall (the openings and an open mesh's
    /// edges).
    pub rims: usize,
}

/// The closed wall between the surface moved by `inner` and by `outer`
/// (outward positive, `inner` < `outer`): the outer side facing out, the
/// inner side facing in, and where the surface has rims (an open mesh's
/// edges, the triangles `openings` flags taken out) the two sides joined
/// across. `mesh` has its shared corners joined. Of a closed mesh, a side
/// whose moved triangles would turn over is built from the distance to
/// the surface kept instead ([`super::offset_field`]), ending on the
/// openings.
pub fn wall(
    mesh: &TriangleMesh,
    inner: f64,
    outer: f64,
    openings: Option<&[bool]>,
) -> Result<Wall, String> {
    if inner.partial_cmp(&outer) != Some(std::cmp::Ordering::Less) {
        return Err("the wall needs a thickness of more than 0".to_owned());
    }
    let (surface, _) = match openings {
        Some(open) => {
            let count = open.iter().filter(|o| **o).count();
            if count == 0 {
                return Err("the openings pick no triangle of the mesh".to_owned());
            }
            if count == mesh.triangles.len() {
                return Err("the openings pick every triangle of the mesh".to_owned());
            }
            submesh_map(mesh, |t| !open.get(t).copied().unwrap_or(false))
        }
        None => submesh_map(mesh, |_| true),
    };
    let edges = edge_triangles(&surface.triangles);
    if edges.values().any(|on| on.len() > 2) {
        return Err(
            "the mesh has an edge of more than two triangles; Repair Mesh makes it manifold"
                .to_owned(),
        );
    }
    let directions = offset_directions(&surface);
    let outside = offset_with(&surface, &directions, outer);
    let inside = offset_with(&surface, &directions, inner);
    if (outside.folded > 0 || inside.folded > 0) && offset_field::closed_solid(mesh) {
        let kept = openings.is_some().then_some(&surface);
        return Ok(trimmed_wall(
            mesh, &surface, kept, outside, inside, inner, outer,
        ));
    }
    let n = surface.vertices.len() as u32;
    let mut out = TriangleMesh {
        vertices: outside.mesh.vertices.clone(),
        triangles: surface.triangles.clone(),
    };
    out.vertices.extend_from_slice(&inside.mesh.vertices);
    for tri in &surface.triangles {
        out.triangles.push([tri[0] + n, tri[2] + n, tri[1] + n]);
    }
    // Each open edge a -> b of a triangle (outer side) is joined to its
    // twin b' -> a' of the inner side by two triangles across the wall.
    let mut rim_edges: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
    for tri in &surface.triangles {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            if edges.get(&edge_key(a, b)).is_some_and(|on| on.len() == 1) {
                rim_edges.insert(edge_key(a, b), (a, b));
            }
        }
    }
    let mut rim_list: Vec<(u32, u32)> = rim_edges.into_values().collect();
    rim_list.sort_unstable();
    for &(a, b) in &rim_list {
        out.triangles.push([b, a, a + n]);
        out.triangles.push([b, a + n, b + n]);
    }
    let rims = super::repair::boundary_loops(&surface.triangles).len();
    let mut folded_places = outside.folded_places;
    folded_places.extend(inside.folded_places);
    folded_places.truncate(super::check::MAX_PLACES);
    Ok(Wall {
        mesh: out,
        folded: outside.folded + inside.folded,
        folded_places,
        overreach: Overreach::TurnedOver,
        trimmed: false,
        rims,
    })
}

/// The wall of a closed mesh one of whose moved sides turned over: that
/// side built from the distance to the surface (`kept`: the surface less
/// the openings, None without), each rim of the outer side joined to the
/// inner side's across the wall.
fn trimmed_wall(
    mesh: &TriangleMesh,
    surface: &TriangleMesh,
    kept: Option<&TriangleMesh>,
    outside: Offset,
    inside: Offset,
    inner: f64,
    outer: f64,
) -> Wall {
    let expected = offset_field::shape(surface);
    let rims = expected.rims;
    let refused = |side: Offset| Wall {
        mesh: TriangleMesh::default(),
        folded: side.folded,
        folded_places: side.folded_places,
        overreach: side.overreach,
        trimmed: true,
        rims,
    };
    let reversed = |side: TriangleMesh| TriangleMesh {
        vertices: side.vertices,
        triangles: side.triangles.iter().map(|t| [t[0], t[2], t[1]]).collect(),
    };
    // The outer side facing out of the wall: as moved, or built (facing
    // the wall) turned round.
    let outer_side = if outside.folded == 0 {
        outside.mesh
    } else {
        let side = trimmed(
            offset_field::wall_side(mesh, kept, outer),
            surface,
            expected,
            outer,
        );
        if side.folded > 0 {
            return refused(side);
        }
        reversed(side.mesh)
    };
    // The inner side facing into the hollow: as moved or as built, both
    // facing the other way.
    let inner_side = if inside.folded == 0 {
        inside.mesh
    } else {
        let side = trimmed(
            offset_field::wall_side(mesh, kept, inner),
            surface,
            expected,
            inner,
        );
        if side.folded > 0 {
            return refused(side);
        }
        side.mesh
    };
    let inner_side = reversed(inner_side);
    let mut out = outer_side;
    let n = out.vertices.len() as u32;
    out.vertices.extend_from_slice(&inner_side.vertices);
    out.triangles
        .extend(inner_side.triangles.iter().map(|t| t.map(|v| v + n)));
    if rims > 0 && !offset_field::join_rims(&mut out, n) {
        let built = if inside.folded > 0 { inner } else { outer };
        let (count, places) = offset_field::thin_places(surface, built);
        return Wall {
            mesh: TriangleMesh::default(),
            folded: count.max(1),
            folded_places: places,
            overreach: Overreach::TooThin,
            trimmed: true,
            rims,
        };
    }
    Wall {
        mesh: out,
        folded: 0,
        folded_places: Vec::new(),
        overreach: Overreach::default(),
        trimmed: true,
        rims,
    }
}

/// The distances from the offset to the mesh, the least and the most,
/// measured at the offset's vertices and at its triangles' middles (a box's
/// mesh has vertices only at its corners, which lie farther out than its
/// faces).
pub fn distance_range(mesh: &TriangleMesh, offset: &TriangleMesh) -> Option<(f64, f64)> {
    let tree = super::bvh::Bvh::new(mesh);
    let mut least = f64::INFINITY;
    let mut most: f64 = 0.0;
    let mut used = vec![false; offset.vertices.len()];
    for tri in &offset.triangles {
        for v in tri {
            used[*v as usize] = true;
        }
    }
    let vertices = offset
        .vertices
        .iter()
        .zip(&used)
        .filter(|(_, u)| **u)
        .map(|(p, _)| *p);
    let middles = (0..offset.triangles.len()).map(|t| super::centroid(&corners(offset, t)));
    for p in vertices.chain(middles) {
        let closest = tree.closest(p)?;
        least = least.min(closest.distance);
        most = most.max(closest.distance);
    }
    least.is_finite().then_some((least, most))
}
