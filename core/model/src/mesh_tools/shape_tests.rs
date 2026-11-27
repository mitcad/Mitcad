// SPDX-License-Identifier: MIT
//! Smooth, Offset and Shell, Align, Plane Cut, merging and the checks of
//! a mesh (mitcad#2515) on synthetic meshes.

use mitcad_vec3::{norm, sub};

use super::align::{Rigid, align, from_pairs};
use super::bvh::Bvh;
use super::check::{Place, crossings, openings};
use super::offset::{Overreach, distance_range, offset, wall};
use super::plane_cut::plane_cut;
use super::smooth::{SmoothOptions, smooth};
use super::synthetic::{noisy_box, noisy_sphere, torus};
use super::{Point, TriangleMesh, centroid, corners, merged, volume};

fn closed(mesh: &TriangleMesh) -> bool {
    openings(mesh).closed()
}

/// The root mean square of the vertices' distances from the sphere.
fn sphere_rms(mesh: &TriangleMesh, centre: Point, radius: f64) -> f64 {
    let sum: f64 = mesh
        .vertices
        .iter()
        .map(|p| (norm(sub(*p, centre)) - radius).powi(2))
        .sum();
    (sum / mesh.vertices.len() as f64).sqrt()
}

/// The distance from a point near a box's surface to the nearest of its
/// face planes.
fn plane_distance(p: Point, size: Point) -> f64 {
    (0..3)
        .map(|i| p[i].abs().min((p[i] - size[i]).abs()))
        .fold(f64::INFINITY, f64::min)
}

fn box_rms(mesh: &TriangleMesh, size: Point) -> f64 {
    let sum: f64 = mesh
        .vertices
        .iter()
        .map(|p| plane_distance(*p, size).powi(2))
        .sum();
    (sum / mesh.vertices.len() as f64).sqrt()
}

const SMOOTH: SmoothOptions = SmoothOptions {
    amount: 0.5,
    passes: 8,
    sharp_angle: Some(std::f64::consts::PI / 6.0),
    keep_volume: true,
};

#[test]
fn smoothing_takes_the_noise_out_and_keeps_the_volume() {
    let centre = [1.0, 2.0, 3.0];
    let noisy = noisy_sphere(centre, 10.0, 48, 24, 0.15);
    assert!(closed(&noisy));
    let before = sphere_rms(&noisy, centre, 10.0);
    // No sharp edges: the thin triangles at the poles tilt with the noise.
    let done = smooth(
        &noisy,
        None,
        &SmoothOptions {
            sharp_angle: None,
            ..SMOOTH
        },
    )
    .unwrap();
    let after = sphere_rms(&done.mesh, centre, 10.0);
    assert!(after < 0.5 * before, "{before} -> {after}");
    // The volume is kept to a millionth (the stated tolerance).
    let (v0, v1) = done.volume.unwrap();
    assert!((v0 - volume(&noisy)).abs() < 1e-9 * v0);
    assert!((v1 - v0).abs() <= 1e-6 * v0, "{v0} -> {v1}");
    assert!((volume(&done.mesh) - v0).abs() <= 1e-6 * v0);
    assert!(done.moved > 0.0 && done.moved < 1.0, "{}", done.moved);
    assert_eq!(done.kept, 0);
    assert_eq!(done.mesh.triangles, noisy.triangles);
}

#[test]
fn smoothing_keeps_a_box_a_box() {
    let size = [20.0, 20.0, 20.0];
    let noisy = noisy_box(size, 10, 0.05);
    let before = box_rms(&noisy, size);
    let done = smooth(&noisy, None, &SMOOTH).unwrap();
    let after = box_rms(&done.mesh, size);
    assert!(after < 0.6 * before, "{before} -> {after}");
    // The eight corners stay, the edges' vertices move along them.
    assert_eq!(done.kept, 8);
    assert!(done.along_edges > 0);
    for (a, b) in noisy.vertices.iter().zip(&done.mesh.vertices) {
        assert!(plane_distance(*b, size) <= 0.1, "{a:?} -> {b:?}");
    }
    let corner = noisy.vertices.iter().position(|p| norm(*p) < 0.1).unwrap();
    assert_eq!(done.mesh.vertices[corner], noisy.vertices[corner]);
    // Without sharp edges the corners round off.
    let rounded = smooth(
        &noisy,
        None,
        &SmoothOptions {
            sharp_angle: None,
            ..SMOOTH
        },
    )
    .unwrap();
    assert_eq!(rounded.kept, 0);
    assert!(norm(sub(rounded.mesh.vertices[corner], noisy.vertices[corner])) > 0.1);
}

#[test]
fn smoothing_a_region_moves_its_vertices_only() {
    let noisy = noisy_sphere([0.0; 3], 10.0, 32, 16, 0.2);
    let region: Vec<bool> = (0..noisy.triangles.len())
        .map(|t| centroid(&corners(&noisy, t))[0] > 2.0)
        .collect();
    let done = smooth(&noisy, Some(&region), &SMOOTH).unwrap();
    let mut moved = 0;
    for (v, (a, b)) in noisy.vertices.iter().zip(&done.mesh.vertices).enumerate() {
        let outside = noisy
            .triangles
            .iter()
            .enumerate()
            .any(|(t, tri)| !region[t] && tri.contains(&(v as u32)));
        if outside {
            assert_eq!(a, b);
        } else if a != b {
            moved += 1;
        }
    }
    assert!(moved > 50, "{moved}");
}

#[test]
fn smoothing_refuses_what_it_cannot_do() {
    let noisy = noisy_sphere([0.0; 3], 10.0, 16, 8, 0.1);
    let error = smooth(
        &noisy,
        None,
        &SmoothOptions {
            amount: 0.0,
            ..SMOOTH
        },
    )
    .unwrap_err();
    assert!(error.contains("amount"), "{error}");
    let error = smooth(&noisy, Some(&vec![false; noisy.triangles.len()]), &SMOOTH).unwrap_err();
    assert!(error.contains("picks no triangle"), "{error}");
    // A lone triangle: every vertex on its rim.
    let lone = TriangleMesh {
        vertices: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        triangles: vec![[0, 1, 2]],
    };
    let error = smooth(&lone, None, &SMOOTH).unwrap_err();
    assert!(error.contains("no vertex of the mesh is free"), "{error}");
}

#[test]
fn an_offset_box_moves_each_face_by_the_distance() {
    let cube = noisy_box([20.0; 3], 4, 0.0);
    let out = offset(&cube, 2.0);
    assert_eq!(out.folded, 0);
    assert!((volume(&out.mesh) - 24.0f64.powi(3)).abs() < 1e-6);
    for p in &out.mesh.vertices {
        for x in p {
            assert!((-2.0 - 1e-9..=22.0 + 1e-9).contains(x), "{p:?}");
        }
    }
    // The distance measured: the faces 2 mm away, the sharp corners
    // 2 √3 mm.
    let (least, most) = distance_range(&cube, &out.mesh).unwrap();
    assert!((least - 2.0).abs() < 1e-9, "{least}");
    assert!((most - 2.0 * 3.0f64.sqrt()).abs() < 1e-9, "{most}");
    let inward = offset(&cube, -2.0);
    assert_eq!(inward.folded, 0);
    assert!((volume(&inward.mesh) - 16.0f64.powi(3)).abs() < 1e-6);
    let (least, _) = distance_range(&cube, &inward.mesh).unwrap();
    assert!((least - 2.0).abs() < 1e-9, "{least}");
}

#[test]
fn an_offset_sphere_lies_at_the_distance() {
    let centre = [0.0, 0.0, 5.0];
    let ball = noisy_sphere(centre, 10.0, 64, 32, 0.0);
    let out = offset(&ball, 1.0);
    assert_eq!(out.folded, 0);
    let (least, most) = distance_range(&ball, &out.mesh).unwrap();
    assert!(least > 0.98 && most < 1.02, "{least} {most}");
    for p in &out.mesh.vertices {
        let r = norm(sub(*p, centre));
        assert!((r - 11.0).abs() < 0.02, "{r}");
    }
}

#[test]
fn a_shell_with_an_opening_is_a_closed_wall() {
    let cube = noisy_box([20.0; 3], 4, 0.0);
    let top: Vec<bool> = (0..cube.triangles.len())
        .map(|t| centroid(&corners(&cube, t))[2] > 19.9)
        .collect();
    let made = wall(&cube, -2.0, 0.0, Some(&top)).unwrap();
    assert_eq!(made.folded, 0);
    assert_eq!(made.rims, 1);
    assert!(closed(&made.mesh));
    assert_eq!(crossings(&made.mesh).pairs, 0);
    // The box less its inside, 16 by 16, from 2 mm above the floor up to
    // the opening.
    let expected = 8000.0 - 16.0 * 16.0 * 18.0;
    assert!(
        (volume(&made.mesh) - expected).abs() < 1e-6,
        "{}",
        volume(&made.mesh)
    );
    // Without openings: a hollow, both sides closed.
    let hollow = wall(&cube, -2.0, 0.0, None).unwrap();
    assert_eq!(hollow.rims, 0);
    assert!(closed(&hollow.mesh));
    assert!((volume(&hollow.mesh) - (8000.0 - 4096.0)).abs() < 1e-6);
    // Both ways, 1 mm each.
    let both = wall(&cube, -1.0, 1.0, None).unwrap();
    assert!((volume(&both.mesh) - (22.0f64.powi(3) - 18.0f64.powi(3))).abs() < 1e-6);
}

#[test]
fn an_offset_larger_than_the_curvature_turns_triangles_over() {
    let ring = torus(10.0, 2.0, 48, 16);
    assert!(closed(&ring));
    let fine = offset(&ring, -1.0);
    assert_eq!(fine.folded, 0);
    // Thicker than the tube: the trimmed offset (mitcad#2564) leaves
    // nothing, refused where the tube's sides come too close.
    let over = offset(&ring, -3.0);
    assert!(over.folded > 0);
    assert_eq!(over.overreach, Overreach::TooThin);
    assert!(!over.folded_places.is_empty());
    let made = wall(&ring, -3.0, 0.0, None).unwrap();
    assert!(made.folded > 0);
    assert_eq!(made.overreach, Overreach::TooThin);
}

/// The distances from the vertices and the triangles' middles of `mesh`
/// to `to`.
fn distances(mesh: &TriangleMesh, to: &TriangleMesh) -> Vec<(Point, f64)> {
    let tree = Bvh::new(to);
    let middles = (0..mesh.triangles.len()).map(|t| centroid(&corners(mesh, t)));
    mesh.vertices
        .iter()
        .copied()
        .chain(middles)
        .map(|p| (p, tree.closest(p).unwrap().distance))
        .collect()
}

/// A 40 mm box of triangles 0.5 mm wide (mitcad#2564): offset 2 mm
/// inward, the triangles beside each edge would turn over; trimmed, the
/// offset is the 36 mm box inside, closed, 2 mm from the box everywhere.
#[test]
fn an_inward_offset_of_fine_triangles_is_trimmed_where_it_turns_over() {
    let cube = noisy_box([40.0; 3], 80, 0.0);
    let inward = offset(&cube, -2.0);
    assert_eq!(inward.folded, 0, "{:?}", inward.folded_places);
    assert!(inward.trimmed);
    assert!(closed(&inward.mesh));
    assert_eq!(crossings(&inward.mesh).pairs, 0);
    let exact = 36.0f64.powi(3);
    let made = volume(&inward.mesh);
    assert!((made - exact).abs() < 0.01 * exact, "{made}");
    let (least, most) = distance_range(&cube, &inward.mesh).unwrap();
    assert!(
        (least - 2.0).abs() < 0.01 && (most - 2.0).abs() < 0.01,
        "{least} {most}"
    );
    for p in &inward.mesh.vertices {
        for x in p {
            assert!((2.0 - 1e-6..=38.0 + 1e-6).contains(x), "{p:?}");
        }
    }
}

/// The same box shelled 2 mm inside, open at the top (mitcad#2564): the
/// inner side trimmed, ending on the opening, joined to the box's rim.
#[test]
fn a_shell_of_fine_triangles_inside_is_trimmed_and_open_at_the_top() {
    let cube = noisy_box([40.0; 3], 80, 0.0);
    let top: Vec<bool> = (0..cube.triangles.len())
        .map(|t| centroid(&corners(&cube, t))[2] > 39.9)
        .collect();
    let made = wall(&cube, -2.0, 0.0, Some(&top)).unwrap();
    assert_eq!(made.folded, 0, "{:?}", made.folded_places);
    assert!(made.trimmed);
    assert_eq!(made.rims, 1);
    assert!(closed(&made.mesh));
    assert_eq!(crossings(&made.mesh).pairs, 0);
    // The box less its inside, 36 by 36, from 2 mm above the floor up to
    // the opening.
    let exact = 40.0f64.powi(3) - 36.0 * 36.0 * 38.0;
    let v = volume(&made.mesh);
    assert!((v - exact).abs() < 0.01 * exact, "{v}");
    // The inner side 2 mm from the walls and the floor kept; the outer
    // side on them, the strip across the wall in the opening's plane.
    let kept = super::submesh(&cube, |t| !top[t]);
    let mut inner = 0;
    for (p, d) in distances(&made.mesh, &kept) {
        if d < 1e-6 || p[2] > 40.0 - 1e-6 {
            continue;
        }
        inner += 1;
        assert!((d - 2.0).abs() < 0.01, "{p:?} {d}");
    }
    assert!(inner > 1000, "{inner}");
    // Both ways, 1 mm each: the outer side moved (its corners sharp), the
    // inner one trimmed.
    let both = wall(&cube, -1.0, 1.0, None).unwrap();
    assert_eq!(both.folded, 0, "{:?}", both.folded_places);
    assert!(both.trimmed);
    assert!(closed(&both.mesh));
    let exact = 42.0f64.powi(3) - 38.0f64.powi(3);
    let v = volume(&both.mesh);
    assert!((v - exact).abs() < 0.01 * exact, "{v}");
}

/// An open mesh's offset is not trimmed: the triangles that turn over
/// are refused as before.
#[test]
fn an_open_mesh_offset_that_turns_over_is_refused() {
    let cube = noisy_box([40.0; 3], 80, 0.0);
    let open = super::submesh(&cube, |t| centroid(&corners(&cube, t))[2] < 39.9);
    let inward = offset(&open, -2.0);
    assert!(inward.folded > 0);
    assert!(!inward.trimmed);
    assert_eq!(inward.overreach, Overreach::TurnedOver);
}

#[test]
fn pairs_and_best_fit_bring_a_moved_copy_back() {
    let target = noisy_box([30.0, 20.0, 10.0], 6, 0.02);
    let axis = mitcad_vec3::unit([1.0, 2.0, 3.0]).unwrap();
    let away = Rigid::rotation_about(axis, 25f64.to_radians(), [5.0, 5.0, 5.0])
        .then(&Rigid::shift([40.0, -10.0, 5.0]));
    let moved: Vec<Point> = target.vertices.iter().map(|p| away.apply(*p)).collect();
    // Three corners picked roughly (0.3 mm off).
    let picks: Vec<(Point, Point)> = [0usize, 7, 40]
        .iter()
        .map(|&v| {
            let p = moved[v];
            ([p[0] + 0.3, p[1] - 0.2, p[2] + 0.1], target.vertices[v])
        })
        .collect();
    let rough = from_pairs(&picks).unwrap();
    let rough_error = moved
        .iter()
        .zip(&target.vertices)
        .map(|(m, t)| norm(sub(rough.apply(*m), *t)))
        .fold(0.0, f64::max);
    assert!(rough_error > 1e-3 && rough_error < 2.0, "{rough_error}");
    let aligned = align(&moved, &target, &picks, true).unwrap();
    assert!(aligned.converged);
    let error = moved
        .iter()
        .zip(&target.vertices)
        .map(|(m, t)| norm(sub(aligned.transform.apply(*m), *t)))
        .fold(0.0, f64::max);
    assert!(error < 1e-4, "{error}");
    assert!(aligned.max < 1e-4 && aligned.rms < 1e-5, "{aligned:?}");
    // Without the best fit the picks' error stays.
    let picked_only = align(&moved, &target, &picks, false).unwrap();
    assert_eq!(picked_only.iterations, 0);
    assert!(picked_only.max > 1e-3);
}

#[test]
fn best_fit_alone_corrects_a_small_misplacement() {
    let target = noisy_sphere([0.0; 3], 10.0, 32, 16, 0.0);
    // A sphere turns into itself: a box's mesh has one way to fit.
    let target = merged(&[target, noisy_box([8.0, 4.0, 2.0], 4, 0.0)]);
    let away = Rigid::rotation_about([0.0, 0.0, 1.0], 3f64.to_radians(), [0.0; 3])
        .then(&Rigid::shift([0.5, 0.3, -0.4]));
    let moved: Vec<Point> = target.vertices.iter().map(|p| away.apply(*p)).collect();
    let aligned = align(&moved, &target, &[], true).unwrap();
    let error = moved
        .iter()
        .zip(&target.vertices)
        .map(|(m, t)| norm(sub(aligned.transform.apply(*m), *t)))
        .fold(0.0, f64::max);
    assert!(
        error < 1e-3,
        "{error} after {} iterations",
        aligned.iterations
    );
}

#[test]
fn aligning_by_points_needs_three_not_in_a_line() {
    let error =
        from_pairs(&[([0.0; 3], [1.0; 3]), ([1.0, 0.0, 0.0], [2.0, 1.0, 1.0])]).unwrap_err();
    assert!(error.contains("three point pairs or more"), "{error}");
    let error = from_pairs(&[
        ([0.0; 3], [0.0; 3]),
        ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        ([2.0, 0.0, 0.0], [2.0, 0.0, 0.0]),
    ])
    .unwrap_err();
    assert!(error.contains("on one line"), "{error}");
}

#[test]
fn plane_cut_pieces_sum_to_the_mesh() {
    let ball = noisy_sphere([0.0; 3], 10.0, 40, 20, 0.1);
    let whole = volume(&ball);
    let cut = plane_cut(&ball, [0.0, 0.0, 1.3], [0.2, 0.1, 1.0], true).unwrap();
    assert!(closed(&cut.above) && closed(&cut.below));
    assert_eq!(cut.capped, 2);
    assert_eq!(cut.open, 0);
    let sum = volume(&cut.above) + volume(&cut.below);
    assert!((sum - whole).abs() < 1e-9 * whole, "{sum} {whole}");
    assert!(volume(&cut.above) > 0.0 && volume(&cut.below) > 0.0);
    assert!(volume(&cut.above) < volume(&cut.below));
}

#[test]
fn a_torus_cut_through_its_vertices_caps_rings_and_disks() {
    let ring = torus(10.0, 3.0, 40, 16);
    let whole = volume(&ring);
    // Across the tube: two rings (an outline with a hole) at z 0, where
    // vertices lie in the plane.
    let across = plane_cut(&ring, [0.0; 3], [0.0, 0.0, 1.0], true).unwrap();
    assert!(closed(&across.above) && closed(&across.below));
    assert_eq!(across.capped, 4);
    let sum = volume(&across.above) + volume(&across.below);
    assert!((sum - whole).abs() < 1e-9 * whole, "{sum} {whole}");
    assert!((volume(&across.above) - volume(&across.below)).abs() < 1e-6 * whole);
    // Through the axis: two disks on each side.
    let through = plane_cut(&ring, [0.0, 0.0, 0.0], [1.0, 0.3, 0.0], true).unwrap();
    assert!(closed(&through.above) && closed(&through.below));
    assert_eq!(through.capped, 4);
    let sum = volume(&through.above) + volume(&through.below);
    assert!((sum - whole).abs() < 1e-9 * whole);
}

#[test]
fn plane_cut_open_and_its_refusal() {
    let cube = noisy_box([20.0; 3], 4, 0.0);
    // Through a row of vertices, not capped: each piece open at the cut.
    let cut = plane_cut(&cube, [0.0, 0.0, 10.0], [0.0, 0.0, 1.0], false).unwrap();
    assert_eq!(cut.capped, 0);
    assert_eq!(openings(&cut.above).holes, 1);
    assert_eq!(openings(&cut.below).holes, 1);
    assert_eq!(
        cut.above.triangles.len() + cut.below.triangles.len(),
        cube.triangles.len()
    );
    let capped = plane_cut(&cube, [0.0, 0.0, 10.0], [0.0, 0.0, 1.0], true).unwrap();
    assert!((volume(&capped.above) - 4000.0).abs() < 1e-6);
    assert!((volume(&capped.below) - 4000.0).abs() < 1e-6);
    // A plane on the top face cuts nothing off.
    let error = plane_cut(&cube, [0.0, 0.0, 20.0], [0.0, 0.0, 1.0], true).unwrap_err();
    assert!(error.contains("lies below it"), "{error}");
    let error = plane_cut(&cube, [0.0, 0.0, 30.0], [0.0, 0.0, 0.0], true).unwrap_err();
    assert!(error.contains("no normal"), "{error}");
}

#[test]
fn merged_meshes_add_their_volumes_and_may_cross() {
    let a = noisy_box([10.0; 3], 2, 0.0);
    // Moved off a's grid of 5 mm: each crossing is an edge through the
    // inside of a triangle. (Moved by 5 every vertex of either box would lie
    // in a face plane of the other, which no edge then crosses.)
    let shift = [4.3, 3.7, 5.9];
    let mut b = noisy_box([10.0; 3], 2, 0.0);
    for p in &mut b.vertices {
        for i in 0..3 {
            p[i] += shift[i];
        }
    }
    let both = merged(&[a.clone(), b]);
    assert_eq!(both.triangles.len(), 2 * a.triangles.len());
    assert!((volume(&both) - 2000.0).abs() < 1e-9);
    assert_eq!(crossings(&a).pairs, 0);
    let crossed = crossings(&both);
    assert!(crossed.pairs > 0);
    assert!(!crossed.places.is_empty());
    // Each place lies where the boxes overlap.
    for p in &crossed.places {
        assert!(
            (0..3).all(|i| (shift[i] - 1e-9..=10.0 + 1e-9).contains(&p[i])),
            "{p:?}"
        );
    }
}

#[test]
fn boxes_crossing_through_each_others_edges_cross() {
    // Two boxes of two triangles a face, one moved by 5 along each axis:
    // a's edges pass through b's faces on their diagonals, and b's through
    // a's (on an edge of a triangle, not inside one).
    let a = noisy_box([10.0; 3], 1, 0.0);
    let mut b = noisy_box([10.0; 3], 1, 0.0);
    for p in &mut b.vertices {
        for x in p.iter_mut() {
            *x += 5.0;
        }
    }
    let crossed = crossings(&merged(&[a, b]));
    assert!(crossed.pairs > 0);
    assert!(
        crossed
            .places
            .iter()
            .all(|p| p.iter().all(|x| (5.0 - 1e-9..=10.0 + 1e-9).contains(x)))
    );
}

#[test]
fn openings_count_the_holes_with_their_places() {
    let cube = noisy_box([20.0; 3], 4, 0.0);
    assert!(openings(&cube).closed());
    let open = TriangleMesh {
        vertices: cube.vertices.clone(),
        triangles: (0..cube.triangles.len())
            .filter(|&t| centroid(&corners(&cube, t))[2] < 19.9)
            .map(|t| cube.triangles[t])
            .collect(),
    };
    let found = openings(&open);
    assert!(!found.closed());
    assert_eq!(found.holes, 1);
    assert_eq!(found.open_edges, 16);
    assert_eq!(found.describe(), "a hole");
    match found.places.as_slice() {
        [Place::Box { min, max }] => {
            assert_eq!(min[2], 20.0);
            assert_eq!(max[2], 20.0);
            assert_eq!((min[0], max[0]), (0.0, 20.0));
        }
        other => panic!("{other:?}"),
    }
}
