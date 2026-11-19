// SPDX-License-Identifier: MIT
//! The mesh tools (mitcad#1871) on synthetic scanned-like meshes: a noisy
//! box and a noisy cylinder written here (`synthetic.rs`).

use std::f64::consts::{FRAC_PI_2, PI};

use super::decimate::{DecimateOptions, decimate};
use super::fit::{Fitted, SurfaceKind, boundary, fit_auto, fit_surface};
use super::region::{Brush, Lasso, Pick, picked, remove};
use super::repair::{RepairOptions, repair};
use super::section::{Polyline, SectionCurve, fit_curves, fit_polyline, plane_section};
use super::segment::{RegionKind, segment};
use super::synthetic::{jitter, noisy_box, noisy_cylinder, unshared};
use super::{TriangleMesh, mesh_stats, signed_volume, submesh};
use crate::profile::SketchFrame;

const NOISE: f64 = 0.01;

fn volume(mesh: &TriangleMesh) -> f64 {
    signed_volume(mesh, &(0..mesh.triangles.len()).collect::<Vec<_>>())
}

fn cylinder() -> TriangleMesh {
    noisy_cylinder(10.0, 30.0, 48, 12, NOISE)
}

fn block() -> TriangleMesh {
    noisy_box([40.0, 20.0, 10.0], 8, NOISE)
}

#[test]
fn the_synthetic_meshes_are_closed_and_face_out() {
    for mesh in [cylinder(), block()] {
        let stats = mesh_stats(&mesh);
        assert_eq!(stats.open_edges, 0);
        assert_eq!(stats.non_manifold_edges, 0);
        assert_eq!(stats.holes, 0);
        assert!(volume(&mesh) > 0.0);
    }
    assert!((volume(&block()) - 8000.0).abs() < 20.0);
}

// Sections.

#[test]
fn a_section_across_a_noisy_cylinder_is_one_circle() {
    let lines = plane_section(&cylinder(), &SketchFrame::XY, 15.3);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].closed);
    let curves = fit_curves(&lines, 0.05);
    assert_eq!(curves.chains.len(), 1);
    match curves.chains[0][..] {
        [SectionCurve::Circle { center, radius }] => {
            assert!(center[0].hypot(center[1]) < 0.02, "{center:?}");
            assert!((radius - 10.0).abs() < 0.03, "{radius}");
        }
        ref other => panic!("not one circle: {other:?}"),
    }
}

#[test]
fn a_section_of_a_noisy_box_is_four_lines() {
    // A plane through the box's middle, square to Z.
    let lines = plane_section(&block(), &SketchFrame::XY, 5.2);
    assert_eq!(lines.len(), 1);
    let curves = fit_curves(&lines, 0.05);
    let chain = &curves.chains[0];
    assert_eq!(chain.len(), 4, "{chain:?}");
    assert!(chain.iter().all(|c| matches!(c, SectionCurve::Line { .. })));
    // End to end, round the 40 x 20 outline.
    let length: f64 = chain
        .iter()
        .map(|c| match c {
            SectionCurve::Line { start, end } => (end[0] - start[0]).hypot(end[1] - start[1]),
            _ => 0.0,
        })
        .sum();
    assert!((length - 120.0).abs() < 0.2, "{length}");
}

#[test]
fn a_section_along_a_cylinder_axis_is_its_outline() {
    // The XZ plane (x = model X, y = model -Z) cuts the side in two lines
    // and the caps across.
    let frame = SketchFrame {
        origin: [0.0, 0.0, 0.0],
        x_axis: [1.0, 0.0, 0.0],
        y_axis: [0.0, 0.0, -1.0],
    };
    let lines = plane_section(&cylinder(), &frame, 0.0);
    assert_eq!(lines.len(), 1);
    let curves = fit_curves(&lines, 0.05);
    assert_eq!(curves.count(), 4, "{:?}", curves.chains);
}

#[test]
fn a_slot_outline_is_two_lines_and_two_arcs() {
    // A stadium 30 long, its ends half circles of radius 5, with noise.
    let mut points = Vec::new();
    let mut seed = 1;
    let mut push = |p: [f64; 2]| {
        seed += 1;
        let q = jitter([p[0], p[1], 0.0], seed, 0.005);
        points.push([q[0], q[1]]);
    };
    for i in 0..30 {
        push([i as f64, -5.0]);
    }
    for i in 0..24 {
        let a = -FRAC_PI_2 + PI * i as f64 / 24.0;
        push([30.0 + 5.0 * a.cos(), 5.0 * a.sin()]);
    }
    for i in 0..30 {
        push([30.0 - i as f64, 5.0]);
    }
    for i in 0..24 {
        let a = FRAC_PI_2 + PI * i as f64 / 24.0;
        push([5.0 * a.cos(), 5.0 * a.sin()]);
    }
    let curves = fit_polyline(
        &Polyline {
            points,
            closed: true,
        },
        0.05,
    );
    let lines = curves
        .iter()
        .filter(|c| matches!(c, SectionCurve::Line { .. }))
        .count();
    let arcs: Vec<_> = curves
        .iter()
        .filter_map(|c| match c {
            SectionCurve::Arc { center, start, .. } => {
                Some((start[0] - center[0]).hypot(start[1] - center[1]))
            }
            _ => None,
        })
        .collect();
    assert_eq!((lines, arcs.len()), (2, 2), "{curves:?}");
    assert!(arcs.iter().all(|r| (r - 5.0).abs() < 0.05), "{arcs:?}");
}

// Decimation.

#[test]
fn decimation_to_a_count_keeps_the_cylinder_closed_and_its_rims() {
    let mesh = cylinder();
    let before = mesh.triangles.len();
    let out = decimate(
        &mesh,
        &DecimateOptions {
            triangles: Some(before / 4),
            tolerance: None,
            sharp_angle: 30f64.to_radians(),
        },
    )
    .unwrap();
    let stats = mesh_stats(&out.mesh);
    assert!(stats.triangles <= before / 4 + 2, "{}", stats.triangles);
    assert_eq!(stats.open_edges, 0);
    assert_eq!(stats.non_manifold_edges, 0);
    // The rims are sharp edges: every rim vertex stays.
    let rim = |z: f64| {
        out.mesh
            .vertices
            .iter()
            .filter(|p| (p[2] - z).abs() < 0.05 && p[0].hypot(p[1]) > 9.9)
            .count()
    };
    assert_eq!(rim(0.0), 48);
    assert_eq!(rim(30.0), 48);
    assert!((volume(&out.mesh) - volume(&mesh)).abs() < 0.02 * volume(&mesh));
}

#[test]
fn decimation_within_a_tolerance_flattens_a_box_to_few_triangles() {
    let mesh = noisy_box([40.0, 20.0, 10.0], 8, 0.0);
    let out = decimate(
        &mesh,
        &DecimateOptions {
            triangles: None,
            tolerance: Some(0.001),
            sharp_angle: 30f64.to_radians(),
        },
    )
    .unwrap();
    let stats = mesh_stats(&out.mesh);
    assert!(
        stats.triangles < mesh.triangles.len() / 4,
        "{}",
        stats.triangles
    );
    assert_eq!(stats.open_edges, 0);
    assert!((volume(&out.mesh) - 8000.0).abs() < 1e-6);
    assert!(out.error <= 0.001);
}

#[test]
fn decimation_needs_a_limit() {
    let error = decimate(
        &block(),
        &DecimateOptions {
            triangles: None,
            tolerance: None,
            sharp_angle: 0.5,
        },
    )
    .unwrap_err();
    assert!(error.contains("a triangle count or a tolerance"), "{error}");
}

// Repair.

#[test]
fn repair_welds_orients_and_fills() {
    let mut mesh = unshared(&noisy_box([20.0, 20.0, 20.0], 4, 0.0));
    // One triangle turned over, one doubled, one taken out (a hole).
    mesh.triangles[3].swap(1, 2);
    mesh.triangles.push(mesh.triangles[10]);
    mesh.triangles.remove(20);
    let (fixed, report) = repair(
        &mesh,
        &RepairOptions {
            tolerance: 0.001,
            max_hole: Some(10.0),
        },
    )
    .unwrap();
    let stats = mesh_stats(&fixed);
    assert_eq!(stats.open_edges, 0);
    assert_eq!(stats.non_manifold_edges, 0);
    assert!((volume(&fixed) - 8000.0).abs() < 1e-6);
    assert!(report.welded > 0);
    assert_eq!(report.duplicate, 1);
    assert_eq!(report.flipped, 1);
    assert_eq!(report.holes_filled, 1);
    let text = report.describe();
    assert!(text.starts_with("Welded "), "{text}");
    assert!(
        text.contains("removed 1 triangle lying on another"),
        "{text}"
    );
    assert!(text.contains("turned 1 triangle to face"), "{text}");
    assert!(text.contains("filled 1 hole"), "{text}");
}

#[test]
fn repair_leaves_a_hole_larger_than_the_size_open() {
    let mut mesh = noisy_box([20.0, 20.0, 20.0], 1, 0.0);
    // The top face's two triangles out: a hole 20 x 20.
    mesh.triangles.drain(2..4);
    let (fixed, report) = repair(
        &mesh,
        &RepairOptions {
            tolerance: 0.001,
            max_hole: Some(5.0),
        },
    )
    .unwrap();
    assert_eq!(report.holes_left, 1);
    assert_eq!(mesh_stats(&fixed).holes, 1);
    assert!(
        report
            .describe()
            .contains("1 hole larger than the size to fill left open")
    );
    let clean = repair(
        &noisy_box([20.0, 20.0, 20.0], 1, 0.0),
        &RepairOptions {
            tolerance: 0.001,
            max_hole: None,
        },
    )
    .unwrap()
    .1;
    assert_eq!(clean.describe(), "the mesh needed no repair");
}

#[test]
fn repair_splits_two_cones_touching_at_a_vertex() {
    // Two tetrahedra sharing one vertex only.
    let mesh = TriangleMesh {
        vertices: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, -1.0],
            [0.0, 1.0, -1.0],
            [-1.0, -1.0, -1.0],
            [1.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [-1.0, -1.0, 1.0],
        ],
        triangles: vec![
            [0, 2, 1],
            [0, 3, 2],
            [0, 1, 3],
            [1, 2, 3],
            [0, 4, 5],
            [0, 5, 6],
            [0, 6, 4],
            [4, 6, 5],
        ],
    };
    let (_, report) = repair(
        &mesh,
        &RepairOptions {
            tolerance: 1e-6,
            max_hole: None,
        },
    )
    .unwrap();
    assert_eq!(report.split, 1);
}

// Regions.

#[test]
fn a_lasso_from_above_removes_the_top_cap_and_fill_closes_it() {
    let mesh = cylinder();
    // A square round the cap, seen from above.
    let lasso = Lasso {
        normal: [0.0, 0.0, 1.0],
        points: vec![
            [-12.0, -12.0, 40.0],
            [12.0, -12.0, 40.0],
            [12.0, 12.0, 40.0],
            [-12.0, 12.0, 40.0],
        ],
    };
    let pick = Pick {
        lassos: vec![lasso.clone()],
        ..Pick::default()
    };
    let chosen = picked(&mesh, &pick);
    // The top cap's 48 triangles face the viewer; the side's face away or
    // across.
    assert_eq!(chosen.iter().filter(|c| **c).count(), 48);
    let (open, removed) = remove(&mesh, &pick, false).unwrap();
    assert_eq!(removed.triangles, 48);
    assert_eq!(mesh_stats(&open).holes, 1);
    let (closed, removed) = remove(&mesh, &pick, true).unwrap();
    assert_eq!(removed.holes_filled, 1);
    assert_eq!(mesh_stats(&closed).open_edges, 0);
    // Through: the bottom cap too, and the side's triangles inside it.
    let through = Pick {
        lassos: vec![lasso],
        through: true,
        ..Pick::default()
    };
    assert_eq!(
        picked(&mesh, &through).iter().filter(|c| **c).count(),
        mesh.triangles.len()
    );
    assert!(
        remove(&mesh, &through, false)
            .unwrap_err()
            .contains("every triangle")
    );
}

#[test]
fn a_brush_removes_the_triangles_about_its_points() {
    let mesh = block();
    let pick = Pick {
        brush: Some(Brush {
            radius: 3.0,
            points: vec![[20.0, 10.0, 10.0]],
        }),
        ..Pick::default()
    };
    let (out, removed) = remove(&mesh, &pick, false).unwrap();
    assert!(
        removed.triangles > 0 && removed.triangles < 20,
        "{}",
        removed.triangles
    );
    // The triangles whose centres lie within the radius of a grid corner
    // meet the rest at corners too: the rim may pass a corner twice and
    // count as two loops.
    assert!((1..=2).contains(&mesh_stats(&out).holes));
    assert!(mesh_stats(&out).open_edges > 0);
    let none = Pick {
        brush: Some(Brush {
            radius: 1.0,
            points: vec![[100.0, 100.0, 100.0]],
        }),
        ..Pick::default()
    };
    assert!(
        remove(&mesh, &none, false)
            .unwrap_err()
            .contains("picks no triangle")
    );
}

// Segmentation.

#[test]
fn a_noisy_box_is_six_planes() {
    let regions = segment(&block(), 20f64.to_radians(), 0.05).unwrap();
    assert_eq!(regions.len(), 6);
    assert!(regions.iter().all(|r| r.kind == RegionKind::Plane));
    // Largest first: the two 40 x 20 faces.
    assert!((regions[0].area - 800.0).abs() < 2.0, "{}", regions[0].area);
}

#[test]
fn a_noisy_cylinder_is_a_cylinder_and_two_planes() {
    let regions = segment(&cylinder(), 20f64.to_radians(), 0.05).unwrap();
    let kinds: Vec<RegionKind> = regions.iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds,
        [RegionKind::Cylinder, RegionKind::Plane, RegionKind::Plane]
    );
    assert_eq!(regions[0].triangles.len(), 48 * 12 * 2);
}

// Surface fitting.

fn side_of(mesh: &TriangleMesh) -> TriangleMesh {
    let regions = segment(mesh, 20f64.to_radians(), 0.05).unwrap();
    let mut keep = vec![false; mesh.triangles.len()];
    for &t in &regions[0].triangles {
        keep[t] = true;
    }
    submesh(mesh, |t| keep[t])
}

#[test]
fn a_cylinder_fitted_to_the_noisy_side() {
    let side = side_of(&cylinder());
    let fitted = fit_surface(&side, SurfaceKind::Cylinder).unwrap();
    assert!(fitted.deviation < 2.0 * NOISE, "{}", fitted.deviation);
    let Fitted::Cone {
        radius,
        half_angle,
        axis,
        angles,
        heights,
        ..
    } = fitted.surface
    else {
        panic!("not a cylinder: {fitted:?}");
    };
    assert!((radius - 10.0).abs() < NOISE, "{radius}");
    assert_eq!(half_angle, 0.0);
    assert!(axis[2].abs() > 0.9999, "{axis:?}");
    assert!(angles.full());
    assert!((heights[1] - heights[0] - 30.0).abs() < 2.0 * NOISE);
    // Auto takes it too; the boundary is two half faces sharing edges.
    let auto = fit_auto(&side, 0.05).unwrap();
    assert_eq!(auto.kind, SurfaceKind::Cylinder);
    let body = boundary(&auto).unwrap();
    assert_eq!(body.faces.len(), 2);
    assert_eq!(body.shells, vec![vec![0, 1]]);
    assert!(
        fitted.describe().starts_with("a cylinder of radius "),
        "{}",
        fitted.describe()
    );
}

#[test]
fn a_plane_fitted_to_a_box_face() {
    let mesh = block();
    let regions = segment(&mesh, 20f64.to_radians(), 0.05).unwrap();
    let mut keep = vec![false; mesh.triangles.len()];
    for &t in &regions[0].triangles {
        keep[t] = true;
    }
    let face = submesh(&mesh, |t| keep[t]);
    let fitted = fit_auto(&face, 0.05).unwrap();
    assert_eq!(fitted.kind, SurfaceKind::Plane);
    let body = boundary(&fitted).unwrap();
    assert_eq!((body.faces.len(), body.edges.len()), (1, 4));
}

/// A half of a sphere of radius 8 about (1, 2, 3), as triangles.
fn hemisphere(noise: f64) -> TriangleMesh {
    let mut mesh = TriangleMesh::default();
    let (rings, segments) = (10, 32);
    let mut seed = 0;
    for j in 0..=rings {
        let lat = FRAC_PI_2 * j as f64 / rings as f64 * 0.95;
        for i in 0..segments {
            let lon = 2.0 * PI * i as f64 / segments as f64;
            seed += 1;
            mesh.vertices.push(jitter(
                [
                    1.0 + 8.0 * lat.cos() * lon.cos(),
                    2.0 + 8.0 * lat.cos() * lon.sin(),
                    3.0 + 8.0 * lat.sin(),
                ],
                seed,
                noise,
            ));
        }
    }
    for j in 0..rings {
        for i in 0..segments {
            let next = (i + 1) % segments;
            let at = |j: usize, i: usize| (j * segments + i) as u32;
            mesh.triangles
                .push([at(j, i), at(j, next), at(j + 1, next)]);
            mesh.triangles
                .push([at(j, i), at(j + 1, next), at(j + 1, i)]);
        }
    }
    mesh
}

#[test]
fn a_sphere_fitted_to_a_noisy_dome() {
    let dome = hemisphere(NOISE);
    let fitted = fit_auto(&dome, 0.05).unwrap();
    assert_eq!(fitted.kind, SurfaceKind::Sphere);
    let Fitted::Sphere { center, radius, .. } = fitted.surface else {
        panic!("{fitted:?}");
    };
    assert!((radius - 8.0).abs() < 0.02, "{radius}");
    assert!((center[0] - 1.0).abs() + (center[1] - 2.0).abs() + (center[2] - 3.0).abs() < 0.05);
    assert!(!boundary(&fitted).unwrap().faces.is_empty());
}

#[test]
fn a_cone_fitted_to_a_frustum_side() {
    // A frustum's side: radius 10 at z 0 to 5 at z 10 (half angle 26.57).
    let mut mesh = TriangleMesh::default();
    let (rings, segments) = (10, 40);
    for j in 0..=rings {
        let z = 10.0 * j as f64 / rings as f64;
        let r = 10.0 - 0.5 * z;
        for i in 0..segments {
            let a = 2.0 * PI * i as f64 / segments as f64;
            mesh.vertices.push(jitter(
                [r * a.cos(), r * a.sin(), z],
                (j * segments + i) as u64,
                NOISE,
            ));
        }
    }
    for j in 0..rings {
        for i in 0..segments {
            let next = (i + 1) % segments;
            let at = |j: usize, i: usize| (j * segments + i) as u32;
            mesh.triangles
                .push([at(j, i), at(j, next), at(j + 1, next)]);
            mesh.triangles
                .push([at(j, i), at(j + 1, next), at(j + 1, i)]);
        }
    }
    let fitted = fit_auto(&mesh, 0.05).unwrap();
    assert_eq!(fitted.kind, SurfaceKind::Cone);
    let Fitted::Cone { half_angle, .. } = fitted.surface else {
        panic!("{fitted:?}");
    };
    assert!(
        (half_angle.to_degrees() - 26.565).abs() < 0.2,
        "{}",
        half_angle.to_degrees()
    );
    assert_eq!(boundary(&fitted).unwrap().faces.len(), 2);
}

#[test]
fn a_b_spline_surface_fitted_to_a_wavy_patch() {
    // z = sin(x / 5) cos(y / 7) over 30 x 20, gently.
    let (nx, ny) = (30, 20);
    let mut mesh = TriangleMesh::default();
    for j in 0..=ny {
        for i in 0..=nx {
            let (x, y) = (i as f64, j as f64);
            mesh.vertices
                .push([x, y, (x / 5.0).sin() * (y / 7.0).cos()]);
        }
    }
    for j in 0..ny {
        for i in 0..nx {
            let at = |i: usize, j: usize| (j * (nx + 1) + i) as u32;
            mesh.triangles
                .push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
            mesh.triangles
                .push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
        }
    }
    let fitted = fit_auto(&mesh, 0.05).unwrap();
    assert_eq!(fitted.kind, SurfaceKind::BSpline, "{fitted:?}");
    let body = boundary(&fitted).unwrap();
    assert_eq!(body.faces.len(), 1);
    assert!(body.faces[0].loops.is_empty());
    // A closed box folds over: refused as a B-spline.
    let error = fit_surface(&block(), SurfaceKind::BSpline).unwrap_err();
    assert!(error.contains("folds over"), "{error}");
}

#[test]
fn auto_names_the_closest_when_nothing_fits() {
    let error = fit_auto(&block(), 0.0001).unwrap_err();
    assert!(
        error.starts_with("no surface lies within the tolerance"),
        "{error}"
    );
}

#[test]
fn corners_stored_apart_are_joined() {
    let soup = unshared(&cylinder());
    assert_eq!(mesh_stats(&soup).open_edges, soup.triangles.len() * 3);
    let joined = super::joined(&soup);
    let stats = mesh_stats(&joined);
    assert_eq!(stats.open_edges, 0);
    assert_eq!(stats.vertices, cylinder().vertices.len());
    assert_eq!(stats.triangles, soup.triangles.len());
}
