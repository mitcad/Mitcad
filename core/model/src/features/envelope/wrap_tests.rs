// SPDX-License-Identifier: MIT
//! Envelope's wrap (mitcad#1271) on small synthetic meshes: a box on the
//! grid wrapped exactly, a slit narrower than the detail size closed and
//! a tunnel wider than it kept, a cavity filled, bodies apart kept apart,
//! bodies that touch only at an edge or a corner made a closed manifold
//! surface, and the refusals. The OCCT kernel's envelope is checked by
//! `cli.envelope` (`tools/cli/tests/envelope.json`).

use std::collections::HashMap;

use super::wrap::{MAX_CELLS, wrap};
use crate::exchange::TriangleMesh;
use crate::features::mesh_convert::{DEFAULT_ANGLE, faceted_solid};

/// The 12 triangles of the box from `min` to `max`, counter-clockwise
/// seen from outside (`inward`: seen from inside, a cavity's walls).
fn cuboid(min: [f64; 3], max: [f64; 3], inward: bool) -> TriangleMesh {
    let corner = |i: usize| -> [f64; 3] {
        std::array::from_fn(|axis| {
            if (i >> axis) & 1 == 1 {
                max[axis]
            } else {
                min[axis]
            }
        })
    };
    let quads: [[u32; 4]; 6] = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    TriangleMesh {
        vertices: (0..8).map(corner).collect(),
        triangles: quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .map(|t| if inward { [t[0], t[2], t[1]] } else { t })
            .collect(),
    }
}

/// Every edge is on two triangles that run along it in opposite
/// directions: the mesh is closed, manifold and oriented.
fn assert_closed(mesh: &TriangleMesh) {
    let mut directed: HashMap<(u32, u32), usize> = HashMap::new();
    for t in &mesh.triangles {
        for k in 0..3 {
            *directed.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    for (&(a, b), &count) in &directed {
        assert_eq!(count, 1, "edge {a} -> {b} runs along {count} triangles");
        assert_eq!(
            directed.get(&(b, a)),
            Some(&1),
            "edge {a} -> {b} has no triangle running back"
        );
    }
}

/// The volume the mesh encloses (positive when it faces outwards).
fn volume(mesh: &TriangleMesh) -> f64 {
    mesh.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|v| mesh.vertices[v as usize]);
            mitcad_vec3::dot(a, mitcad_vec3::cross(b, c)) / 6.0
        })
        .sum()
}

fn bounds(mesh: &TriangleMesh) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in &mesh.vertices {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    (min, max)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

/// A box whose faces lie on cell boundaries is wrapped as it is: its
/// volume and box, six planar faces.
#[test]
fn a_box_on_the_grid_wraps_exactly() {
    let wrapped = wrap(&[cuboid([0.0; 3], [60.0, 40.0, 20.0], false)], 10.0).unwrap();
    assert_eq!(wrapped.cell, 5.0);
    assert_closed(&wrapped.mesh);
    assert!(near(wrapped.volume, 48_000.0), "{}", wrapped.volume);
    assert!(
        near(volume(&wrapped.mesh), 48_000.0),
        "{}",
        volume(&wrapped.mesh)
    );
    let (min, max) = bounds(&wrapped.mesh);
    assert!((0..3).all(|i| near(min[i], 0.0)), "{min:?}");
    assert!(
        near(max[0], 60.0) && near(max[1], 40.0) && near(max[2], 20.0),
        "{max:?}"
    );
    // Flat stretches take few triangles.
    assert!(
        wrapped.mesh.triangles.len() < 400,
        "{}",
        wrapped.mesh.triangles.len()
    );
    let faceted = faceted_solid(&wrapped.mesh, 1e-3, DEFAULT_ANGLE).unwrap();
    assert_eq!(faceted.boundary.faces.len(), 6);
    assert_eq!(faceted.shells, 1);
}

/// An inverted U (two legs and a top plate) with a 100 mm tunnel under
/// it and a 4 mm slit through the top plate, wrapped at a detail size of
/// 20 mm: the slit closes, the tunnel stays open.
#[test]
fn a_slit_closes_and_a_tunnel_stays_open() {
    let meshes = [
        cuboid([0.0, 0.0, 0.0], [50.0, 200.0, 150.0], false),
        cuboid([150.0, 0.0, 0.0], [200.0, 200.0, 150.0], false),
        cuboid([0.0, 0.0, 150.0], [98.0, 200.0, 200.0], false),
        cuboid([102.0, 0.0, 150.0], [200.0, 200.0, 200.0], false),
    ];
    let wrapped = wrap(&meshes, 20.0).unwrap();
    assert_closed(&wrapped.mesh);
    // The legs and the whole top plate; the tunnel's 100 × 200 × 150 not.
    let expected = 2.0 * 50.0 * 200.0 * 150.0 + 200.0 * 200.0 * 50.0;
    assert!(near(wrapped.volume, expected), "{}", wrapped.volume);
    assert!(
        near(volume(&wrapped.mesh), expected),
        "{}",
        volume(&wrapped.mesh)
    );
    let faceted = faceted_solid(&wrapped.mesh, 1e-2, DEFAULT_ANGLE).unwrap();
    // Top, bottoms of the legs, outer and inner sides, the tunnel's
    // ceiling, front and back.
    assert_eq!(faceted.boundary.faces.len(), 10);
    assert_eq!(faceted.shells, 1);
}

/// A hollow box (an outer and an inner shell): the cavity fills.
#[test]
fn a_cavity_fills() {
    let meshes = [
        cuboid([0.0; 3], [100.0; 3], false),
        cuboid([30.0; 3], [70.0; 3], true),
    ];
    let wrapped = wrap(&meshes, 20.0).unwrap();
    assert_closed(&wrapped.mesh);
    assert!(near(wrapped.volume, 1e6), "{}", wrapped.volume);
    let faceted = faceted_solid(&wrapped.mesh, 1e-2, DEFAULT_ANGLE).unwrap();
    assert_eq!(faceted.boundary.faces.len(), 6);
    assert_eq!(faceted.shells, 1);
}

/// Bodies further apart than the detail size stay apart: two shells.
#[test]
fn bodies_apart_stay_apart() {
    let meshes = [
        cuboid([0.0; 3], [40.0; 3], false),
        cuboid([100.0, 0.0, 0.0], [140.0, 40.0, 40.0], false),
    ];
    let wrapped = wrap(&meshes, 20.0).unwrap();
    assert_closed(&wrapped.mesh);
    assert!(near(wrapped.volume, 2.0 * 64_000.0), "{}", wrapped.volume);
    let faceted = faceted_solid(&wrapped.mesh, 1e-2, DEFAULT_ANGLE).unwrap();
    assert_eq!(faceted.shells, 2);
    assert_eq!(faceted.boundary.faces.len(), 12);
}

/// Boxes that touch only along an edge or at a corner: the closing keeps
/// cells that touch so, and the repair joins them, so the surface is
/// closed and manifold and Faceted solid takes it.
#[test]
fn boxes_touching_at_an_edge_or_a_corner_make_a_manifold_surface() {
    for other in [
        cuboid([10.0, 10.0, 0.0], [20.0, 20.0, 10.0], false),
        cuboid([10.0; 3], [20.0; 3], false),
    ] {
        let meshes = [cuboid([0.0; 3], [10.0; 3], false), other];
        let wrapped = wrap(&meshes, 2.0).unwrap();
        assert_closed(&wrapped.mesh);
        // Both boxes, and what joins them.
        assert!(wrapped.volume >= 2000.0, "{}", wrapped.volume);
        assert!(near(volume(&wrapped.mesh), wrapped.volume));
        let faceted = faceted_solid(&wrapped.mesh, 1e-3, DEFAULT_ANGLE).unwrap();
        assert_eq!(faceted.shells, 1);
    }
}

/// A slanted face (a box turned about z) wraps as stairs within a cell
/// of it: a closed surface around the box.
#[test]
fn a_turned_box_wraps_within_a_cell() {
    let mut mesh = cuboid([-30.0, -10.0, 0.0], [30.0, 10.0, 20.0], false);
    let (s, c) = (0.5f64, 0.75f64.sqrt());
    for p in &mut mesh.vertices {
        *p = [c * p[0] - s * p[1], s * p[0] + c * p[1], p[2]];
    }
    let wrapped = wrap(std::slice::from_ref(&mesh), 4.0).unwrap();
    assert_closed(&wrapped.mesh);
    let exact = 60.0 * 20.0 * 20.0;
    assert!(
        wrapped.volume >= exact && wrapped.volume < 2.0 * exact,
        "{}",
        wrapped.volume
    );
    let (min, max) = bounds(&wrapped.mesh);
    let (want_min, want_max) = bounds(&mesh);
    for i in 0..3 {
        assert!(
            min[i] <= want_min[i] + 1e-5 && min[i] >= want_min[i] - 4.0,
            "{min:?}"
        );
        assert!(
            max[i] >= want_max[i] - 1e-5 && max[i] <= want_max[i] + 4.0,
            "{max:?}"
        );
    }
    faceted_solid(&wrapped.mesh, 1e-3, DEFAULT_ANGLE).unwrap();
}

/// A detail size the grid cannot hold names the smallest that fits;
/// nothing to wrap and a size of zero are refused.
#[test]
fn refusals() {
    let big = [cuboid([0.0; 3], [1000.0; 3], false)];
    let error = wrap(&big, 0.1).unwrap_err();
    assert!(
        error.contains("at least") && error.contains(&MAX_CELLS.to_string()),
        "{error}"
    );
    // The size it names fits (without wrapping on so large a grid here).
    let at_least: f64 = error
        .rsplit("at least ")
        .next()
        .and_then(|rest| rest.split(' ').next())
        .and_then(|n| n.parse().ok())
        .unwrap();
    let cells: f64 = (0..3)
        .map(|_| (1000.0 / (at_least / 2.0)).floor() + 7.0)
        .product();
    assert!(cells <= MAX_CELLS as f64, "{at_least}: {cells}");
    let error = wrap(&[TriangleMesh::default()], 10.0).unwrap_err();
    assert!(error.contains("no triangles"), "{error}");
    let error = wrap(&big, 0.0).unwrap_err();
    assert!(error.contains("more than 0"), "{error}");
}
