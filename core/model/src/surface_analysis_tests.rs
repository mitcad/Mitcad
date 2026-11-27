// SPDX-License-Identifier: MIT
//! The surface quality analyses' values (mitcad#2517) on samples made here:
//! the curvature kinds of a ball's, a cylinder's and a plane's principal
//! curvatures, a vertex without curvature taking its neighbours', the Zebra
//! stripes cut along their borders with the area kept, a plane in one
//! stripe, and placements.

use crate::surface_analysis::{
    CurvatureKind, automatic_limit, curvature_values, is_dark, placed, stripe_coordinate, zebra,
};
use crate::surface_quality::SurfaceSamples;
use crate::transform::Transform;

const TOL: f64 = 1e-12;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// A triangle in the XY plane with the normals and curvatures given.
fn triangle(normals: [[f64; 3]; 3], curvatures: [[f64; 2]; 3]) -> SurfaceSamples {
    SurfaceSamples {
        vertices: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
        normals: normals.to_vec(),
        curvatures: curvatures.to_vec(),
        triangles: vec![[0, 1, 2]],
        face_triangles: vec![1],
    }
}

fn area(triangles: &[[f64; 9]]) -> f64 {
    triangles
        .iter()
        .map(|t| {
            let u = [t[3] - t[0], t[4] - t[1], t[5] - t[2]];
            let v = [t[6] - t[0], t[7] - t[1], t[8] - t[2]];
            0.5 * mitcad_vec3::norm(mitcad_vec3::cross(u, v))
        })
        .sum()
}

#[test]
fn curvature_kinds_of_a_ball_a_cylinder_and_a_plane() {
    // A ball of radius 5 from outside: both principal curvatures 1/5.
    let ball = [0.2, 0.2];
    assert!(close(CurvatureKind::Gaussian.value(ball).unwrap(), 0.04));
    assert!(close(CurvatureKind::Mean.value(ball).unwrap(), 0.2));
    assert!(close(CurvatureKind::Maximum.value(ball).unwrap(), 0.2));
    assert!(close(CurvatureKind::Minimum.value(ball).unwrap(), 0.2));
    // A shaft of radius 4: 1/4 round it, none along it.
    let shaft = [0.25, 0.0];
    assert_eq!(CurvatureKind::Gaussian.value(shaft), Some(0.0));
    assert!(close(CurvatureKind::Mean.value(shaft).unwrap(), 0.125));
    assert!(close(CurvatureKind::Maximum.value(shaft).unwrap(), 0.25));
    assert_eq!(CurvatureKind::Minimum.value(shaft), Some(0.0));
    // A bore of radius 4 is hollow: negative.
    let bore = [0.0, -0.25];
    assert!(close(CurvatureKind::Mean.value(bore).unwrap(), -0.125));
    assert!(close(CurvatureKind::Minimum.value(bore).unwrap(), -0.25));
    // A plane: zero, never -0.
    for kind in [
        CurvatureKind::Gaussian,
        CurvatureKind::Mean,
        CurvatureKind::Maximum,
        CurvatureKind::Minimum,
    ] {
        let value = kind.value([0.0, -0.0]).unwrap();
        assert_eq!(value, 0.0);
        assert!(value.is_sign_positive(), "{kind:?}");
    }
    // None where the surface has no curvature.
    assert_eq!(CurvatureKind::Mean.value([f64::NAN, f64::NAN]), None);
    assert_eq!(CurvatureKind::Gaussian.unit(), "1/mm^2");
    assert_eq!(CurvatureKind::Maximum.unit(), "1/mm");
    assert_eq!(CurvatureKind::Minimum.name(), "minimum");
}

#[test]
fn a_vertex_without_curvature_takes_its_neighbours() {
    let up = [0.0, 0.0, 1.0];
    let samples = triangle([up; 3], [[0.1, 0.1], [0.3, 0.3], [f64::NAN, f64::NAN]]);
    let values = curvature_values(&samples, CurvatureKind::Mean);
    assert!(close(values.values[0].unwrap(), 0.1));
    assert!(close(values.values[2].unwrap(), 0.2));
    let (min, at) = values.min.unwrap();
    assert!(close(min, 0.1));
    assert_eq!(at, [0.0, 0.0, 0.0]);
    let (max, at) = values.max.unwrap();
    assert!(close(max, 0.3));
    assert_eq!(at, [10.0, 0.0, 0.0]);
    // The scale reaches the larger magnitude; a plane's a tiny one.
    assert!(close(automatic_limit(Some(-0.5), Some(0.3)), 0.5));
    assert!(automatic_limit(Some(0.0), Some(0.0)) > 0.0);
    assert!(automatic_limit(None, None) > 0.0);
}

#[test]
fn a_plane_lies_in_one_stripe() {
    let up = [0.0, 0.0, 1.0];
    // Facing the direction: the first stripe's dark band.
    let samples = triangle([up; 3], [[0.0, 0.0]; 3]);
    let stripes = zebra(&samples, up, 12, 0.5);
    assert_eq!(stripes.dark.len(), 1);
    assert!(stripes.light.is_empty());
    assert!((stripes.dark_area - 50.0).abs() < TOL);
    // Square to it: 90° is 6 stripes of 15°, the start of a dark band.
    let side = zebra(&samples, [1.0, 0.0, 0.0], 12, 0.5);
    assert_eq!(side.dark.len(), 1);
    // 100° is 6.67 stripes: in the light band.
    let tilted = [
        0.0,
        (100.0_f64).to_radians().sin(),
        (100.0_f64).to_radians().cos(),
    ];
    let light = zebra(&triangle([tilted; 3], [[0.0, 0.0]; 3]), up, 12, 0.5);
    assert!(light.dark.is_empty());
    assert_eq!(light.light.len(), 1);
}

#[test]
fn stripes_are_cut_along_their_borders() {
    // Normals turning from along the direction (0°) to 60° across the
    // triangle: 4 stripes of 15° with 12 over the half turn, each half dark.
    let up = [0.0, 0.0, 1.0];
    let turned = |degrees: f64| {
        let r = degrees.to_radians();
        [r.sin(), 0.0, r.cos()]
    };
    let samples = triangle([up, turned(60.0), up], [[0.0, 0.0]; 3]);
    assert!(close(stripe_coordinate(turned(60.0), up, 12), 4.0));
    let stripes = zebra(&samples, up, 12, 0.5);
    // The whole triangle, split between the bands.
    assert!((stripes.dark_area + stripes.light_area - 50.0).abs() < 1e-9);
    assert!((area(&stripes.dark) - stripes.dark_area).abs() < 1e-9);
    assert!(stripes.dark.len() >= 4);
    assert!(stripes.light.len() >= 4);
    // The coordinate is linear in x here (0 to 4 over 10 mm): each band
    // is a strip between two lines x = const, its share of the area set
    // by where the lines are.
    let band = |from: f64, to: f64| {
        let x0 = from / 4.0 * 10.0;
        let x1 = to / 4.0 * 10.0;
        // The triangle's height at x is 10 - x.
        let h = |x: f64| 10.0 * x - 0.5 * x * x;
        h(x1) - h(x0)
    };
    let dark: f64 = (0..4).map(|k| band(k as f64, k as f64 + 0.5)).sum();
    assert!(
        (stripes.dark_area - dark).abs() < 1e-9,
        "{}",
        stripes.dark_area
    );
    // A narrower dark band takes less.
    let narrow = zebra(&samples, up, 12, 0.2);
    assert!(narrow.dark_area < stripes.dark_area);
    assert!(is_dark(3.1, 0.2) && !is_dark(3.3, 0.2));
}

#[test]
fn placements_move_points_and_turn_normals() {
    let up = [0.0, 0.0, 1.0];
    let samples = triangle([up; 3], [[0.5, 0.25]; 3]);
    let moved = placed(&samples, &Transform::translation([1.0, 2.0, 3.0]));
    assert_eq!(moved.vertices[1], [11.0, 2.0, 3.0]);
    assert_eq!(moved.normals[0], up);
    assert_eq!(moved.curvatures, samples.curvatures);
    // A mirror turns the triangles round and the normals with it.
    let mirror = Transform::mirror([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]).unwrap();
    let mirrored = placed(&samples, &mirror);
    assert_eq!(mirrored.triangles, vec![[0, 2, 1]]);
    assert!(close(mirrored.normals[0][2], -1.0));
}
