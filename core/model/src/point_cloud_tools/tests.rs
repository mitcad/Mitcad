// SPDX-License-Identifier: MIT
//! The point cloud tools on synthetic clouds (mitcad#2516): the k-d tree
//! against a plain search; a cloud moved and turned aligned back by picked
//! point pairs and the best fit (to another cloud and to a mesh's
//! triangles), and by its shape alone; outliers added to a sphere's points
//! taken out with every point of the sphere kept; noise smoothed; crops by
//! a box and a lasso; a sphere's and a box's points rebuilt as closed
//! meshes within a tolerance of the shapes; the refusals.

use super::filter::{ByRadius, CropBox, CropLasso, FilterOptions, Smooth, Statistical, filter};
use super::kd::KdTree;
use super::matching::{
    FitError, FitOptions, PointsTarget, Target, TrianglesTarget, best_fit, best_start,
    pairs_motion, rigid_fit, shape_starts,
};
use super::reconstruct::{
    ReconstructError, ReconstructOptions, is_closed, largest_vertex_distance, reconstruct,
};
use super::{Vec3, symmetric_eigen};
use crate::transform::Transform;

/// A repeatable sequence of numbers from 0 to 1.
struct Numbers(u64);

impl Numbers {
    fn next(&mut self) -> f64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let v = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (v >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn distance(a: Vec3, b: Vec3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Points spread unevenly through a lopsided block, 40 x 20 x 6 mm: no two
/// of its main axes alike and no symmetry, so only one motion fits it.
fn lopsided(count: usize) -> Vec<Vec3> {
    let mut n = Numbers(0x9e37_79b9_7f4a_7c15);
    (0..count)
        .map(|_| {
            let (u, v, w) = (n.next(), n.next(), n.next());
            [
                40.0 * u * u,
                20.0 * v.powf(1.5) + 3.0 * u,
                6.0 * w + 2.0 * u * v,
            ]
        })
        .collect()
}

/// The points of a sphere, evenly (a Fibonacci spiral).
fn sphere(count: usize, radius: f64) -> Vec<Vec3> {
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    (0..count)
        .map(|i| {
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / count as f64;
            let r = (1.0 - z * z).sqrt();
            let a = golden * i as f64;
            [radius * r * a.cos(), radius * r * a.sin(), radius * z]
        })
        .collect()
}

/// The points of a box's faces from the origin to `size`, `step` apart.
fn box_points(size: Vec3, step: f64) -> Vec<Vec3> {
    let mut points = Vec::new();
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let nu = (size[u] / step).round() as usize;
        let nv = (size[v] / step).round() as usize;
        for side in [0.0, size[axis]] {
            for i in 0..=nu {
                for j in 0..=nv {
                    let mut p = [0.0; 3];
                    p[axis] = side;
                    p[u] = size[u] * i as f64 / nu as f64;
                    p[v] = size[v] * j as f64 / nv as f64;
                    points.push(p);
                }
            }
        }
    }
    points
}

/// The 12 triangles of a box from the origin to `size`, facing outwards.
fn box_mesh(size: Vec3) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let vertices: Vec<Vec3> = (0..8)
        .map(|c| std::array::from_fn(|k| if (c >> k) & 1 == 1 { size[k] } else { 0.0 }))
        .collect();
    let triangles = vec![
        [0, 2, 3],
        [0, 3, 1],
        [4, 5, 7],
        [4, 7, 6],
        [0, 1, 5],
        [0, 5, 4],
        [2, 6, 7],
        [2, 7, 3],
        [0, 4, 6],
        [0, 6, 2],
        [1, 3, 7],
        [1, 7, 5],
    ];
    (vertices, triangles)
}

/// A point's distance from the surface of the box from the origin to `size`.
fn box_distance(p: Vec3, size: Vec3) -> f64 {
    let inside = (0..3).all(|k| p[k] >= 0.0 && p[k] <= size[k]);
    if inside {
        (0..3)
            .map(|k| p[k].min(size[k] - p[k]))
            .fold(f64::INFINITY, f64::min)
    } else {
        (0..3)
            .map(|k| (-p[k]).max(p[k] - size[k]).max(0.0).powi(2))
            .sum::<f64>()
            .sqrt()
    }
}

fn rigid_inverse(t: &Transform) -> Transform {
    let linear: [[f64; 3]; 3] = std::array::from_fn(|r| std::array::from_fn(|c| t.linear[c][r]));
    let back = Transform {
        linear,
        translation: [0.0; 3],
    };
    let moved = back.apply_vector(t.translation);
    Transform {
        linear,
        translation: moved.map(|v| -v),
    }
}

/// A turn of 25 degrees about a slanted axis, then a move.
fn motion() -> Transform {
    let turn = Transform::rotation([3.0, -2.0, 1.0], [1.0, 2.0, 3.0], 25f64.to_radians()).unwrap();
    Transform::translation([12.0, -7.0, 4.0]).after(&turn)
}

fn no_cancel(_: usize, _: usize) -> bool {
    true
}

#[test]
fn the_tree_finds_what_a_plain_search_finds() {
    let points = lopsided(500);
    let tree = KdTree::new(points.clone());
    let mut n = Numbers(7);
    for _ in 0..50 {
        let q = [
            50.0 * n.next() - 5.0,
            30.0 * n.next() - 5.0,
            10.0 * n.next() - 2.0,
        ];
        let mut plain: Vec<(usize, f64)> = points
            .iter()
            .enumerate()
            .map(|(i, p)| (i, distance(*p, q)))
            .collect();
        plain.sort_by(|a, b| a.1.total_cmp(&b.1));
        let found = tree.nearest_k(q, 7);
        assert_eq!(found.len(), 7);
        for (f, p) in found.iter().zip(&plain) {
            assert!((f.1 - p.1).abs() < 1e-12, "{f:?} {p:?}");
        }
        let near = tree.within(q, 4.0);
        let expected = plain.iter().filter(|(_, d)| *d <= 4.0).count();
        assert_eq!(near.len(), expected);
    }
}

#[test]
fn the_eigenvectors_of_a_symmetric_matrix() {
    let m = [[4.0, 1.0, 0.5], [1.0, 3.0, 0.2], [0.5, 0.2, 1.0]];
    let (values, vectors) = symmetric_eigen(m);
    assert!(values[0] <= values[1] && values[1] <= values[2]);
    for k in 0..3 {
        let v = [vectors[0][k], vectors[1][k], vectors[2][k]];
        for r in 0..3 {
            let mv: f64 = (0..3).map(|c| m[r][c] * v[c]).sum();
            assert!((mv - values[k] * v[r]).abs() < 1e-10);
        }
    }
}

#[test]
fn a_rigid_fit_finds_the_motion_of_exact_pairs() {
    let from = lopsided(30);
    let m = motion();
    let to: Vec<Vec3> = from.iter().map(|p| m.apply_point(*p)).collect();
    let fit = rigid_fit(&from, &to).unwrap();
    for (a, b) in from.iter().zip(&to) {
        assert!(distance(fit.apply_point(*a), *b) < 1e-9);
    }
    assert!(fit.is_rigid());
}

/// The ticket's test: a cloud moved and turned aligns back within a
/// tolerance, from point pairs picked roughly (0.2 mm off) refined by the
/// best fit to the cloud it was a copy of.
#[test]
fn a_moved_and_turned_cloud_aligns_back_to_its_original() {
    let target = lopsided(2000);
    let m = motion();
    let back = rigid_inverse(&m);
    // The scan lies moved away: each target point taken by the inverse.
    let scan: Vec<Vec3> = target.iter().map(|p| back.apply_point(*p)).collect();
    // Four points picked on the scan and on the target, roughly.
    let picks = [10, 500, 1200, 1900];
    let from: Vec<Vec3> = picks.iter().map(|&i| scan[i]).collect();
    let mut n = Numbers(99);
    let to: Vec<Vec3> = picks
        .iter()
        .map(|&i| target[i].map(|v| v + 0.4 * n.next() - 0.2))
        .collect();
    let start = pairs_motion(&from, &to).unwrap();
    let rough = scan
        .iter()
        .zip(&target)
        .map(|(s, t)| distance(start.apply_point(*s), *t))
        .fold(0.0, f64::max);
    assert!(rough > 1e-3 && rough < 2.0, "{rough}");
    let fitted = best_fit(
        &scan,
        &PointsTarget::new(target.clone()),
        start,
        &FitOptions::default(),
        &mut no_cancel,
    )
    .unwrap();
    assert!(fitted.converged, "{fitted:?}");
    assert_eq!(fitted.matched, scan.len());
    assert!(fitted.rms < 1e-6, "{}", fitted.rms);
    for (s, t) in scan.iter().zip(&target) {
        assert!(distance(fitted.motion.apply_point(*s), *t) < 1e-4);
    }
}

/// The same aligned to a mesh's triangles: a box's scan, moved, back on the
/// box's 12 triangles.
#[test]
fn a_moved_box_scan_aligns_back_to_the_box_mesh() {
    let size = [30.0, 20.0, 10.0];
    let (vertices, triangles) = box_mesh(size);
    let target = TrianglesTarget::new(vertices, triangles);
    let points = box_points(size, 2.0);
    let small = Transform::rotation([15.0, 10.0, 5.0], [0.3, -0.5, 1.0], 3f64.to_radians())
        .unwrap()
        .after(&Transform::translation([0.6, -0.4, 0.3]));
    let scan: Vec<Vec3> = points.iter().map(|p| small.apply_point(*p)).collect();
    let options = FitOptions {
        max_iterations: 300,
        ..FitOptions::default()
    };
    let fitted = best_fit(
        &scan,
        &target,
        Transform::IDENTITY,
        &options,
        &mut no_cancel,
    )
    .unwrap();
    assert!(fitted.rms < 1e-3, "{fitted:?}");
    for (s, p) in scan.iter().zip(&points) {
        assert!(distance(fitted.motion.apply_point(*s), *p) < 1e-2);
    }
}

/// No pairs: the rough start by the points' shape (middles and main axes)
/// finds a half turn the best fit alone could not.
#[test]
fn a_cloud_turned_far_aligns_back_by_its_shape() {
    let target = lopsided(1500);
    let turn = Transform::rotation([0.0; 3], [0.2, 1.0, -0.4], 150f64.to_radians())
        .unwrap()
        .after(&Transform::translation([-30.0, 5.0, 8.0]));
    let scan: Vec<Vec3> = target.iter().map(|p| turn.apply_point(*p)).collect();
    let points_target = PointsTarget::new(target.clone());
    let starts = shape_starts(&scan, &target);
    assert_eq!(starts.len(), 24);
    let start = best_start(&scan, &points_target, &starts, 20, &mut no_cancel).unwrap();
    let fitted = best_fit(
        &scan,
        &points_target,
        start,
        &FitOptions::default(),
        &mut no_cancel,
    )
    .unwrap();
    for (s, t) in scan.iter().zip(&target) {
        assert!(distance(fitted.motion.apply_point(*s), *t) < 1e-4);
    }
}

#[test]
fn alignments_refuse_too_few_points_and_no_overlap() {
    let target = PointsTarget::new(lopsided(100));
    let two = [[0.0; 3], [1.0, 0.0, 0.0]];
    assert_eq!(
        best_fit(
            &two,
            &target,
            Transform::IDENTITY,
            &FitOptions::default(),
            &mut no_cancel
        ),
        Err(FitError::TooFewPoints(2))
    );
    // A scan a metre away, matched within 1 mm only.
    let far: Vec<Vec3> = lopsided(50)
        .into_iter()
        .map(|p| [p[0] + 1000.0, p[1], p[2]])
        .collect();
    let options = FitOptions {
        max_distance: Some(1.0),
        ..FitOptions::default()
    };
    let error = best_fit(&far, &target, Transform::IDENTITY, &options, &mut no_cancel).unwrap_err();
    assert!(
        matches!(error, FitError::NoOverlap { matched: 0, .. }),
        "{error:?}"
    );
    assert!(error.to_string().contains("do not overlap"), "{error}");
    // Pairs: too few, and on one line.
    let line = [[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
    assert!(
        pairs_motion(&line[..2], &line[..2])
            .unwrap_err()
            .contains("three pairs")
    );
    assert!(pairs_motion(&line, &line).unwrap_err().contains("one line"));
    // A cancel stops the fit.
    let mut stop = |_: usize, _: usize| false;
    assert_eq!(
        best_fit(
            &lopsided(10),
            &target,
            Transform::IDENTITY,
            &FitOptions::default(),
            &mut stop
        ),
        Err(FitError::Cancelled)
    );
}

#[test]
fn the_closest_points_of_triangles_are_exact() {
    let (vertices, triangles) = box_mesh([10.0, 10.0, 10.0]);
    let target = TrianglesTarget::new(vertices, triangles);
    let found = target
        .nearest(&[[5.0, 5.0, 13.0], [12.0, 12.0, 5.0], [5.0, 4.5, 6.5]])
        .unwrap();
    let expect = [[5.0, 5.0, 10.0], [10.0, 10.0, 5.0], [5.0, 4.5, 10.0]];
    for (f, e) in found.iter().zip(expect) {
        assert!(distance(f.unwrap(), e) < 1e-12, "{f:?} {e:?}");
    }
}

/// Outliers spread round a sphere's points, each far from the others.
fn with_outliers() -> (Vec<Vec3>, usize) {
    let mut points = sphere(2000, 10.0);
    let count = points.len();
    let mut n = Numbers(1234);
    for i in 0..15 {
        let dir = sphere(15, 1.0)[i];
        let r = 25.0 + 15.0 * n.next();
        points.push(dir.map(|v| v * r));
    }
    (points, count)
}

/// The ticket's test: a cloud with added outliers loses them and keeps its
/// points (statistically, and by radius).
#[test]
fn outliers_go_and_the_sphere_s_points_stay() {
    let (points, count) = with_outliers();
    let statistical = filter(
        &points,
        &FilterOptions {
            statistical: Some(Statistical {
                neighbours: 8,
                ratio: 2.0,
            }),
            ..FilterOptions::default()
        },
    )
    .unwrap();
    assert_eq!(statistical.kept, (0..count).collect::<Vec<_>>());
    assert_eq!(statistical.statistical, 15);
    assert_eq!(statistical.points.len(), count);
    let by_radius = filter(
        &points,
        &FilterOptions {
            radius: Some(ByRadius {
                radius: 2.0,
                neighbours: 3,
            }),
            ..FilterOptions::default()
        },
    )
    .unwrap();
    assert_eq!(by_radius.kept, (0..count).collect::<Vec<_>>());
    assert_eq!(by_radius.radius, 15);
}

#[test]
fn smoothing_takes_noise_out_of_a_plane() {
    let mut n = Numbers(55);
    let mut points = Vec::new();
    for i in 0..40 {
        for j in 0..40 {
            points.push([i as f64 * 0.5, j as f64 * 0.5, 0.2 * n.next() - 0.1]);
        }
    }
    let rms = |ps: &[Vec3]| (ps.iter().map(|p| p[2] * p[2]).sum::<f64>() / ps.len() as f64).sqrt();
    let smoothed = filter(
        &points,
        &FilterOptions {
            smooth: Some(Smooth {
                neighbours: 20,
                passes: 2,
            }),
            ..FilterOptions::default()
        },
    )
    .unwrap();
    assert_eq!(smoothed.points.len(), points.len());
    assert!(
        rms(&smoothed.points) < 0.5 * rms(&points),
        "{}",
        rms(&smoothed.points)
    );
    assert!(smoothed.largest_move < 0.2);
}

#[test]
fn crops_by_a_box_and_a_lasso() {
    let points: Vec<Vec3> = (0..10)
        .flat_map(|i| (0..10).map(move |j| [f64::from(i), f64::from(j), 1.0]))
        .collect();
    let in_box = filter(
        &points,
        &FilterOptions {
            crop_box: Some(CropBox {
                min: [-0.5, -0.5, 0.0],
                max: [4.5, 2.5, 2.0],
                keep_inside: true,
            }),
            ..FilterOptions::default()
        },
    )
    .unwrap();
    assert_eq!(in_box.kept.len(), 15);
    assert_eq!(in_box.cropped, 85);
    // A lasso drawn from above round the points with x and y from 2 to 4,
    // on a plane 5 mm up: what lies inside goes.
    let lasso = CropLasso {
        normal: [0.0, 0.0, 1.0],
        points: vec![
            [1.5, 1.5, 5.0],
            [4.5, 1.5, 5.0],
            [4.5, 4.5, 5.0],
            [1.5, 4.5, 5.0],
        ],
        keep_inside: false,
    };
    let outside = filter(
        &points,
        &FilterOptions {
            lasso: Some(lasso),
            ..FilterOptions::default()
        },
    )
    .unwrap();
    assert_eq!(outside.kept.len(), 91);
    assert!(
        filter(&points, &FilterOptions::default())
            .unwrap_err()
            .contains("choose what the filter does")
    );
}

/// The ticket's test: a sphere's points rebuilt as a closed mesh within a
/// tolerance of the sphere.
#[test]
fn a_sphere_s_points_make_a_closed_mesh_of_the_sphere() {
    let radius = 10.0;
    let points = sphere(3000, radius);
    let made = reconstruct(
        &points,
        &ReconstructOptions {
            detail: 40,
            ..ReconstructOptions::default()
        },
        &mut no_cancel,
    )
    .unwrap();
    assert!(made.closed && is_closed(&made.mesh.triangles));
    assert_eq!((made.shells, made.dropped_shells), (1, 0));
    let off = largest_vertex_distance(&made.mesh, |p| (distance(p, [0.0; 3]) - radius).abs());
    assert!(off < 0.1, "{off}");
    let volume = 4.0 / 3.0 * std::f64::consts::PI * radius.powi(3);
    assert!(
        (made.volume - volume).abs() < 0.02 * volume,
        "{}",
        made.volume
    );
    assert!(made.max_deviation < 0.1, "{}", made.max_deviation);
}

/// The ticket's test: a box's points rebuilt as a closed mesh within a
/// tolerance of the box (its edges a little rounded).
#[test]
fn a_box_s_points_make_a_closed_mesh_of_the_box() {
    let size = [20.0, 12.0, 8.0];
    let points = box_points(size, 0.25);
    let made = reconstruct(
        &points,
        &ReconstructOptions {
            detail: 50,
            ..ReconstructOptions::default()
        },
        &mut no_cancel,
    )
    .unwrap();
    assert!(made.closed, "not closed");
    assert_eq!(
        (made.shells, made.dropped_shells),
        (1, 0),
        "{} bubbles left out",
        made.small_shells
    );
    let off = largest_vertex_distance(&made.mesh, |p| box_distance(p, size));
    assert!(off < 0.6, "{off}");
    let volume = size[0] * size[1] * size[2];
    assert!(
        (made.volume - volume).abs() < 0.05 * volume,
        "{}",
        made.volume
    );
}

#[test]
fn reconstructions_refuse_in_words() {
    let few = sphere(10, 5.0);
    let error = reconstruct(&few, &ReconstructOptions::default(), &mut no_cancel).unwrap_err();
    assert!(error.to_string().contains("at least 20 points"), "{error}");
    let bad = ReconstructOptions {
        detail: 2,
        ..ReconstructOptions::default()
    };
    assert!(
        reconstruct(&sphere(100, 5.0), &bad, &mut no_cancel)
            .unwrap_err()
            .to_string()
            .contains("the detail must be")
    );
    // A flat scan encloses nothing.
    let flat: Vec<Vec3> = (0..30)
        .flat_map(|i| (0..30).map(move |j| [f64::from(i), f64::from(j), 0.0]))
        .collect();
    let error = reconstruct(
        &flat,
        &ReconstructOptions {
            detail: 20,
            ..ReconstructOptions::default()
        },
        &mut no_cancel,
    )
    .unwrap_err();
    assert!(error.to_string().contains("enclose no volume"), "{error}");
    let mut stop = |_: usize, _: usize| false;
    assert_eq!(
        reconstruct(&sphere(100, 5.0), &ReconstructOptions::default(), &mut stop),
        Err(ReconstructError::Cancelled)
    );
}
