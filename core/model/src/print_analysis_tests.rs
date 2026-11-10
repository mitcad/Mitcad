// SPDX-License-Identifier: MIT
//! Print Analysis on meshes built here (mitcad#620): overhangs of boxes, a
//! bridge, a T, a sphere and a lying cylinder, the walls of a plate and of
//! a shelled box, and the estimate's arithmetic. No kernel: the functions
//! take a `TriangleMesh` as `Kernel::triangle_mesh` gives one (closed,
//! counter-clockwise seen from outside).

use std::collections::BTreeMap;
use std::f64::consts::PI;

use crate::exchange::TriangleMesh;
use crate::print_analysis::{self, Bed, EstimateSettings};
use crate::transform::Transform;

/// The baseline tolerance of geometric comparisons, mm.
const TOL: f64 = 1e-6;
const DOWN: [f64; 3] = [0.0, 0.0, -1.0];

/// Adds the rectangle `origin`, `origin + u`, `origin + u + v`,
/// `origin + v` as two triangles facing `u × v`.
fn rect(mesh: &mut TriangleMesh, origin: [f64; 3], u: [f64; 3], v: [f64; 3]) {
    let at =
        |a: f64, b: f64| -> [f64; 3] { std::array::from_fn(|i| origin[i] + a * u[i] + b * v[i]) };
    let first = mesh.vertices.len() as u32;
    mesh.vertices
        .extend([at(0.0, 0.0), at(1.0, 0.0), at(1.0, 1.0), at(0.0, 1.0)]);
    mesh.triangles
        .extend([[first, first + 1, first + 2], [first, first + 2, first + 3]]);
}

/// The box from `min` to `max`, facing out.
fn boxed(mesh: &mut TriangleMesh, min: [f64; 3], max: [f64; 3]) {
    let [dx, dy, dz] = std::array::from_fn(|i| max[i] - min[i]);
    let x = [dx, 0.0, 0.0];
    let y = [0.0, dy, 0.0];
    let z = [0.0, 0.0, dz];
    rect(mesh, min, y, x); // bottom, -z
    rect(mesh, [min[0], min[1], max[2]], x, y); // top, +z
    rect(mesh, min, x, z); // -y
    rect(mesh, [min[0], max[1], min[2]], z, x); // +y
    rect(mesh, min, z, y); // -x
    rect(mesh, [max[0], min[1], min[2]], y, z); // +x
}

fn box_mesh(min: [f64; 3], max: [f64; 3]) -> TriangleMesh {
    let mut mesh = TriangleMesh::default();
    boxed(&mut mesh, min, max);
    welded(mesh)
}

/// The triangles turned inside out (a cavity's walls).
fn reversed(mut mesh: TriangleMesh) -> TriangleMesh {
    for t in &mut mesh.triangles {
        t.swap(1, 2);
    }
    mesh
}

fn joined(mut a: TriangleMesh, b: TriangleMesh) -> TriangleMesh {
    let first = a.vertices.len() as u32;
    a.vertices.extend(b.vertices);
    a.triangles
        .extend(b.triangles.iter().map(|t| t.map(|i| first + i)));
    welded(a)
}

/// Vertices at the same place merged, as the kernel's mesh shares them.
fn welded(mesh: TriangleMesh) -> TriangleMesh {
    let mut index: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    let mut vertices = Vec::new();
    let map: Vec<u32> = mesh
        .vertices
        .iter()
        .map(|v| {
            *index.entry(v.map(f64::to_bits)).or_insert_with(|| {
                vertices.push(*v);
                (vertices.len() - 1) as u32
            })
        })
        .collect();
    TriangleMesh {
        vertices,
        triangles: mesh
            .triangles
            .iter()
            .map(|t| t.map(|i| map[i as usize]))
            .collect(),
    }
}

fn normal(mesh: &TriangleMesh, t: usize) -> [f64; 3] {
    let [a, b, c] = mesh.triangles[t].map(|i| mesh.vertices[i as usize]);
    let u: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
    let v: [f64; 3] = std::array::from_fn(|i| c[i] - a[i]);
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    n.map(|c| c / length)
}

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

#[test]
fn a_box_on_its_bottom_face_has_no_overhangs() {
    let mesh = box_mesh([0.0; 3], [20.0, 30.0, 10.0]);
    let found = print_analysis::overhangs(&mesh, DOWN, 45.0, Bed::Lowest, TOL);
    assert!(found.triangles.is_empty(), "{:?}", found.triangles);
    assert_eq!(found.overhang_area, 0.0);
    assert!(
        close(found.area, 2.0 * (600.0 + 300.0 + 200.0), TOL),
        "{}",
        found.area
    );
    assert_eq!(found.share(), 0.0);
    // The bed through the bottom, as a bed face gives it: the same.
    let found = print_analysis::overhangs(&mesh, DOWN, 45.0, Bed::Through([5.0, 5.0, 0.0]), TOL);
    assert!(found.triangles.is_empty());
    // Lifted off the bed, the bottom overhangs.
    let found = print_analysis::overhangs(&mesh, DOWN, 45.0, Bed::Through([0.0, 0.0, -1.0]), TOL);
    assert!(
        close(found.overhang_area, 600.0, TOL),
        "{}",
        found.overhang_area
    );
}

#[test]
fn a_bridge_under_a_box_is_flagged_exactly() {
    // A block on the bed and a 20 x 20 mm slab above it: the slab's
    // underside is the only overhang.
    let mesh = joined(
        box_mesh([0.0; 3], [40.0, 20.0, 10.0]),
        box_mesh([10.0, 0.0, 15.0], [30.0, 20.0, 20.0]),
    );
    let found = print_analysis::overhangs(&mesh, DOWN, 45.0, Bed::Lowest, TOL);
    assert!(
        close(found.overhang_area, 400.0, TOL),
        "{}",
        found.overhang_area
    );
    assert_eq!(found.triangles.len(), 2);
    for &t in &found.triangles {
        let [a, b, c] = mesh.triangles[t].map(|i| mesh.vertices[i as usize]);
        assert!(
            [a, b, c].iter().all(|p| close(p[2], 15.0, TOL)),
            "{a:?} {b:?} {c:?}"
        );
    }
    let flat = print_analysis::flat(&mesh, found.triangles[0]);
    assert_eq!(flat.len(), 9);
    assert!(close(flat[2], 15.0, TOL) && close(flat[5], 15.0, TOL) && close(flat[8], 15.0, TOL));
}

#[test]
fn a_t_shape_overhangs_under_its_arms() {
    // A 10 x 10 x 20 stem on the bed and a 40 x 10 x 5 bar on it: the arms
    // stick out 15 mm on both sides.
    let mut mesh = TriangleMesh::default();
    let (x, y, z) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    let s = |v: [f64; 3], k: f64| v.map(|c| c * k);
    // The stem: bottom and four sides.
    rect(&mut mesh, [0.0; 3], s(y, 10.0), s(x, 10.0));
    rect(&mut mesh, [0.0; 3], s(x, 10.0), s(z, 20.0));
    rect(&mut mesh, [0.0, 10.0, 0.0], s(z, 20.0), s(x, 10.0));
    rect(&mut mesh, [0.0; 3], s(z, 20.0), s(y, 10.0));
    rect(&mut mesh, [10.0, 0.0, 0.0], s(y, 10.0), s(z, 20.0));
    // The bar: the arms' undersides, top and sides.
    rect(&mut mesh, [-15.0, 0.0, 20.0], s(y, 10.0), s(x, 15.0));
    rect(&mut mesh, [10.0, 0.0, 20.0], s(y, 10.0), s(x, 15.0));
    rect(&mut mesh, [-15.0, 0.0, 25.0], s(x, 40.0), s(y, 10.0));
    rect(&mut mesh, [-15.0, 0.0, 20.0], s(x, 40.0), s(z, 5.0));
    rect(&mut mesh, [-15.0, 10.0, 20.0], s(z, 5.0), s(x, 40.0));
    rect(&mut mesh, [-15.0, 0.0, 20.0], s(z, 5.0), s(y, 10.0));
    rect(&mut mesh, [25.0, 0.0, 20.0], s(y, 10.0), s(z, 5.0));
    let mesh = welded(mesh);
    let found = print_analysis::overhangs(&mesh, DOWN, 45.0, Bed::Lowest, TOL);
    assert!(
        close(found.overhang_area, 300.0, TOL),
        "{}",
        found.overhang_area
    );
    assert_eq!(found.triangles.len(), 4);
    let area = 100.0 + 4.0 * 200.0 + 2.0 * 150.0 + 400.0 + 2.0 * 200.0 + 2.0 * 50.0;
    assert!(close(found.area, area, TOL), "{} / {area}", found.area);
    assert!(close(found.share(), 300.0 / area, TOL));
    // Printed upside down (on the bar's top), nothing overhangs: the
    // stem's end faces away from the bed.
    let found = print_analysis::overhangs(&mesh, [0.0, 0.0, 1.0], 45.0, Bed::Lowest, TOL);
    assert!(found.triangles.is_empty(), "{:?}", found.triangles);
}

#[test]
fn a_box_tilted_on_an_edge_overhangs_with_its_bottom() {
    // Printed along a direction 26.6° from -Z: the bottom face is within
    // 45° of straight down and lies on the bed only along an edge; the
    // +X side is 63.4° from down and prints.
    let mesh = box_mesh([0.0; 3], [20.0, 20.0, 20.0]);
    let length = (0.25_f64 + 1.0).sqrt();
    let down = [0.5 / length, 0.0, -1.0 / length];
    let found = print_analysis::overhangs(&mesh, down, 45.0, Bed::Lowest, TOL);
    assert!(
        close(found.overhang_area, 400.0, TOL),
        "{}",
        found.overhang_area
    );
    assert!(close(found.share(), 1.0 / 6.0, TOL));
    for &t in &found.triangles {
        assert!(close(normal(&mesh, t)[2], -1.0, TOL));
    }
    // A steeper limit: the +X side too (63.4° from down, within 70°).
    let found = print_analysis::overhangs(&mesh, down, 20.0, Bed::Lowest, TOL);
    assert!(
        close(found.overhang_area, 800.0, TOL),
        "{}",
        found.overhang_area
    );
}

/// A sphere of `radius` centred at `center`, `rings` bands from pole to
/// pole and `segments` around.
fn sphere(center: [f64; 3], radius: f64, rings: usize, segments: usize) -> TriangleMesh {
    let mut mesh = TriangleMesh::default();
    // From the south pole (lowest) to the north pole.
    for i in 0..=rings {
        let polar = PI * i as f64 / rings as f64;
        for j in 0..segments {
            let around = 2.0 * PI * j as f64 / segments as f64;
            mesh.vertices.push([
                center[0] + radius * polar.sin() * around.cos(),
                center[1] + radius * polar.sin() * around.sin(),
                center[2] - radius * polar.cos(),
            ]);
        }
    }
    let at = |i: usize, j: usize| (i * segments + j % segments) as u32;
    for i in 0..rings {
        for j in 0..segments {
            let (a, b, c, d) = (at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j));
            // Counter-clockwise seen from outside; the poles' degenerate
            // triangles have no area and are left out.
            if i > 0 {
                mesh.triangles.push([a, b, d]);
            }
            if i + 1 < rings {
                mesh.triangles.push([b, c, d]);
            }
        }
    }
    mesh
}

#[test]
fn a_sphere_overhangs_by_the_analytic_share() {
    let mesh = sphere([0.0, 0.0, 10.0], 10.0, 64, 128);
    // The outward normals point away from the centre.
    for t in [0, mesh.triangles.len() / 2, mesh.triangles.len() - 1] {
        let n = normal(&mesh, t);
        let [a, ..] = mesh.triangles[t].map(|i| mesh.vertices[i as usize]);
        let out = [a[0], a[1], a[2] - 10.0];
        assert!(n[0] * out[0] + n[1] * out[1] + n[2] * out[2] > 0.0);
    }
    for angle in [30.0_f64, 45.0, 60.0] {
        let found = print_analysis::overhangs(&mesh, DOWN, angle, Bed::Lowest, TOL);
        // The cap within 90° − angle of the lowest point.
        let analytic = (1.0 - angle.to_radians().sin()) / 2.0;
        assert!(
            close(found.share(), analytic, 0.01),
            "{angle}: {} / {analytic}",
            found.share()
        );
    }
    assert!(close(
        print_analysis::overhangs(&mesh, DOWN, 45.0, Bed::Lowest, TOL).area,
        4.0 * PI * 100.0,
        4.0 * PI * 100.0 * 0.002
    ));
}

#[test]
fn a_cylinder_on_its_side_flags_the_lower_steep_strip_not_its_ends() {
    // Radius 10, 30 long along X, lying on the bed (its lowest line on
    // z = 0) in 72 facets, a vertex row at the bottom.
    let (radius, length, segments) = (10.0, 30.0, 72);
    let mut mesh = TriangleMesh::default();
    let point = |x: f64, k: usize| {
        let angle = 2.0 * PI * k as f64 / segments as f64;
        [x, radius * angle.sin(), radius - radius * angle.cos()]
    };
    for k in 0..segments {
        for x in [0.0, length] {
            mesh.vertices.push(point(x, k));
        }
    }
    let ring = |k: usize, end: usize| (2 * (k % segments) + end) as u32;
    for k in 0..segments {
        // Around the axis (+Y at the bottom), then along it: facing out.
        let (a, b, c, d) = (ring(k, 0), ring(k + 1, 0), ring(k + 1, 1), ring(k, 1));
        mesh.triangles.push([a, b, c]);
        mesh.triangles.push([a, c, d]);
    }
    // The end faces as fans: -X and +X.
    let start = mesh.vertices.len() as u32;
    mesh.vertices.push([0.0, 0.0, radius]);
    mesh.vertices.push([length, 0.0, radius]);
    for k in 0..segments {
        mesh.triangles.push([start, ring(k + 1, 0), ring(k, 0)]);
        mesh.triangles.push([start + 1, ring(k, 1), ring(k + 1, 1)]);
    }
    // The bed through the lowest line, as a bed face gives it.
    let found = print_analysis::overhangs(&mesh, DOWN, 45.0, Bed::Through([0.0; 3]), TOL);
    // Facets every 5°, their normals at 2.5°, 7.5°, ... from straight down:
    // nine on each side are within 45°.
    let chord = 2.0 * radius * (PI / segments as f64).sin();
    assert!(
        close(found.overhang_area, 18.0 * chord * length, TOL),
        "{}",
        found.overhang_area
    );
    for &t in &found.triangles {
        let n = normal(&mesh, t);
        assert!(close(n[0], 0.0, TOL), "an end face is flagged: {n:?}");
        assert!(n[2] < -(45.0_f64.to_radians().sin()), "{n:?}");
    }
}

#[test]
fn a_plate_is_as_thick_as_it_is() {
    let mesh = box_mesh([0.0; 3], [50.0, 40.0, 2.0]);
    let found = print_analysis::thickness(&mesh, 0.8, &|| false).unwrap();
    assert!(
        close(found.thinnest.unwrap(), 2.0, TOL),
        "{:?}",
        found.thinnest
    );
    assert!(found.thin.is_empty());
    assert_eq!(found.regions, 0);
    let at = found.at.unwrap();
    let to = found.to.unwrap();
    assert!(close((at[2] - to[2]).abs(), 2.0, TOL), "{at:?} {to:?}");
    // Thinner than 3 mm: the top and the bottom, one wall, one region.
    let found = print_analysis::thickness(&mesh, 3.0, &|| false).unwrap();
    assert_eq!(found.thin.len(), 4, "{:?}", found.thin);
    for &t in &found.thin {
        assert!(close(normal(&mesh, t)[2].abs(), 1.0, TOL));
    }
    assert_eq!(found.regions, 1);
    // Mirrored in the design, its triangles still face out.
    let mirror = Transform {
        linear: [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0; 3],
    };
    let mirrored = print_analysis::placed(&mesh, &mirror);
    let found = print_analysis::thickness(&mirrored, 0.8, &|| false).unwrap();
    assert!(close(found.thinnest.unwrap(), 2.0, TOL));
    assert!(
        print_analysis::overhangs(&mirrored, DOWN, 45.0, Bed::Lowest, TOL)
            .triangles
            .is_empty()
    );
}

#[test]
fn a_shelled_box_is_thinnest_at_its_floor() {
    // 22 x 22 x 12 outside, a closed cavity inside: 1.0 mm walls and
    // ceiling, a 0.5 mm floor.
    let mesh = joined(
        box_mesh([0.0; 3], [22.0, 22.0, 12.0]),
        reversed(box_mesh([1.0, 1.0, 0.5], [21.0, 21.0, 11.0])),
    );
    let found = print_analysis::thickness(&mesh, 0.8, &|| false).unwrap();
    assert!(
        close(found.thinnest.unwrap(), 0.5, TOL),
        "{:?}",
        found.thinnest
    );
    let at = found.at.unwrap();
    assert!(close(at[2], 0.0, TOL) || close(at[2], 0.5, TOL), "{at:?}");
    // Only the floor's two sides are thin; the walls are not under 0.8 mm.
    assert_eq!(found.thin.len(), 4, "{:?}", found.thin);
    for &t in &found.thin {
        let [a, b, c] = mesh.triangles[t].map(|i| mesh.vertices[i as usize]);
        assert!(
            [a, b, c].iter().all(|p| close(p[2], 0.0, TOL))
                || [a, b, c].iter().all(|p| close(p[2], 0.5, TOL)),
            "{a:?} {b:?} {c:?}"
        );
    }
    assert_eq!(found.regions, 1);
    // With a 1.2 mm minimum the walls and the ceiling are thin too.
    let found = print_analysis::thickness(&mesh, 1.2, &|| false).unwrap();
    assert!(found.thin.len() > 4);
}

#[test]
fn a_sphere_is_as_thick_as_its_diameter_through_its_centre() {
    // The wall's ray follows the surface's normal: through the centre to
    // the facet opposite. Both ends lie on facets inside the sphere by at
    // most the mesh's sag, 10 × (1 − cos 2°) < 0.007 mm here, and the
    // interpolated normals miss the centre by little (a chord d beside it
    // is d² / 10 mm short).
    let center = [0.0, 0.0, 10.0];
    let mesh = welded(sphere(center, 10.0, 64, 128));
    let found = print_analysis::thickness(&mesh, 0.8, &|| false).unwrap();
    let wall = found.thinnest.unwrap();
    assert!(wall <= 20.0 + TOL && wall > 20.0 - 0.03, "{wall}");
    let (at, to) = (found.at.unwrap(), found.to.unwrap());
    let middle: [f64; 3] = std::array::from_fn(|i| (at[i] + to[i]) / 2.0 - center[i]);
    let off = (middle[0] * middle[0] + middle[1] * middle[1] + middle[2] * middle[2]).sqrt();
    assert!(off < 0.5, "the ray misses the center by {off} mm");
    assert!(found.thin.is_empty());
}

#[test]
fn walls_stop_when_cancelled() {
    let mesh = box_mesh([0.0; 3], [10.0, 10.0, 10.0]);
    assert!(print_analysis::thickness(&mesh, 0.8, &|| true).is_err());
    // An empty mesh has no walls.
    let found = print_analysis::thickness(&TriangleMesh::default(), 0.8, &|| false).unwrap();
    assert_eq!(found.thinnest, None);
    assert_eq!(found.regions, 0);
}

#[test]
fn the_estimate_of_a_20_mm_cube_is_worked_out_by_hand() {
    // 8000 mm³, 2400 mm², PLA (1.24 g/cm³), the defaults: the shell
    // 2400 × 2 × 0.4 = 1920 mm³, the infill (8000 − 1920) × 0.15 = 912 mm³,
    // the filament 2832 mm³ = 2.832 cm³ → 3.51168 g, and
    // 2832 / (π × 0.875²) mm of 1.75 mm filament.
    let settings = EstimateSettings::default();
    let estimate = print_analysis::estimate(8000.0, 2400.0, 1.24, &settings);
    assert!(close(estimate.shell_volume, 1920.0, 1e-9));
    assert!(close(estimate.infill_volume, 912.0, 1e-9));
    assert!(close(estimate.filament_volume, 2832.0, 1e-9));
    assert!(close(estimate.mass, 3.51168, 1e-9), "{}", estimate.mass);
    let length = 2832.0 / (PI * 0.875 * 0.875) / 1000.0;
    assert!(close(estimate.length, length, 1e-12), "{}", estimate.length);
    assert!(close(estimate.length, 1.1774, 1e-4));
    assert_eq!(estimate.cost, None);
    // 25 per kilogram.
    let priced = EstimateSettings {
        price_per_kg: Some(25.0),
        ..settings
    };
    let estimate = print_analysis::estimate(8000.0, 2400.0, 1.24, &priced);
    assert!(close(
        estimate.cost.unwrap(),
        25.0 * 3.51168 / 1000.0,
        1e-12
    ));
    // A part thinner than its walls is all shell.
    let estimate = print_analysis::estimate(100.0, 1000.0, 1.24, &settings);
    assert!(close(estimate.shell_volume, 100.0, 1e-12));
    assert_eq!(estimate.infill_volume, 0.0);
    assert!(close(estimate.filament_volume, 100.0, 1e-12));
}

#[test]
fn estimate_settings_out_of_range_are_refused() {
    let settings = EstimateSettings::default();
    assert!(settings.check().is_ok());
    for bad in [
        EstimateSettings {
            line_width: 0.0,
            ..settings
        },
        EstimateSettings {
            infill: 1.5,
            ..settings
        },
        EstimateSettings {
            filament_diameter: -1.75,
            ..settings
        },
        EstimateSettings {
            price_per_kg: Some(-1.0),
            ..settings
        },
        EstimateSettings {
            layer_height: f64::NAN,
            ..settings
        },
    ] {
        assert!(bad.check().is_err(), "{bad:?}");
    }
}

/// The triangles of a simple polygon given counter-clockwise, ears cut off
/// one by one.
fn ears(points: &[[f64; 2]]) -> Vec<[usize; 3]> {
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut left: Vec<usize> = (0..points.len()).collect();
    let mut out = Vec::new();
    while left.len() > 3 {
        let m = left.len();
        let corner = |k: usize| (left[(k + m - 1) % m], left[k], left[(k + 1) % m]);
        let ear = (0..m)
            .find(|&k| {
                let (a, b, c) = corner(k);
                let (pa, pb, pc) = (points[a], points[b], points[c]);
                cross(pa, pb, pc) > 1e-12
                    && left.iter().all(|&q| {
                        let p = points[q];
                        q == a
                            || q == b
                            || q == c
                            || cross(pa, pb, p) < 0.0
                            || cross(pb, pc, p) < 0.0
                            || cross(pc, pa, p) < 0.0
                    })
            })
            .expect("a simple polygon has an ear");
        let (a, b, c) = corner(ear);
        out.push([a, b, c]);
        left.remove(ear);
    }
    out.push([left[0], left[1], left[2]]);
    out
}

/// The prism of a profile (x, z; counter-clockwise with x to the right and
/// z up) from y = 0 to y = `depth`, its vertices shared, facing out.
fn prism(points: &[[f64; 2]], depth: f64) -> TriangleMesh {
    let n = points.len();
    let mut mesh = TriangleMesh::default();
    mesh.vertices
        .extend(points.iter().map(|p| [p[0], 0.0, p[1]]));
    mesh.vertices
        .extend(points.iter().map(|p| [p[0], depth, p[1]]));
    let low = |i: usize| (i % n) as u32;
    let high = |i: usize| (n + i % n) as u32;
    for i in 0..n {
        // y × the profile's edge: out.
        mesh.triangles.push([low(i), high(i), high(i + 1)]);
        mesh.triangles.push([low(i), high(i + 1), low(i + 1)]);
    }
    for [a, b, c] in ears(points) {
        // The end at y = 0 faces −y as the profile turns; the other end +y.
        mesh.triangles.push([low(a), low(b), low(c)]);
        mesh.triangles.push([high(a), high(c), high(b)]);
    }
    mesh
}

/// The points of the arc about `center` of `radius` from angle `start` to
/// `end` (radians) in `steps` steps, both ends included.
fn arc(center: [f64; 2], radius: f64, start: f64, end: f64, steps: usize) -> Vec<[f64; 2]> {
    (0..=steps)
        .map(|i| {
            let a = start + (end - start) * i as f64 / steps as f64;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        })
        .collect()
}

#[test]
fn a_shell_split_through_its_fillet_is_as_thick_as_its_wall() {
    // A 2 mm shell's corner rounded 6 mm outside and 4 mm inside (centre
    // x 6, z 14), split at z 17 through the fillets: the upper piece. The
    // split face meets the outer fillet at 60°; along its normal, a ray from
    // the split face leaves through the fillet at once next to that edge
    // (and one from the fillet through the split face), which is the
    // edge's shape, not a wall (mitcad#689).
    let inner_start = PI - 0.75_f64.asin(); // z 14 + 4 × 0.75 = 17
    let mut profile = arc([6.0, 14.0], 4.0, inner_start, PI / 2.0, 12);
    profile.extend([[20.0, 18.0], [20.0, 20.0]]);
    profile.extend(arc([6.0, 14.0], 6.0, PI / 2.0, 5.0 * PI / 6.0, 12));
    let mesh = prism(&profile, 10.0);
    let found = print_analysis::thickness(&mesh, 0.8, &|| false).unwrap();
    assert!(
        found.thin.is_empty(),
        "{:?} {:?}",
        found.thin,
        found.thinnest
    );
    assert_eq!(found.regions, 0);
    // Measured along the split face's normal, the curving wall is somewhat
    // less than 2 mm a sample distance away from the edge (2 mm × the tangent
    // of the fillet's slope there); nowhere near thin.
    let wall = found.thinnest.unwrap();
    assert!(wall > 1.2 && wall < 2.5, "{wall} at {:?}", found.at);
}

#[test]
fn a_hole_through_a_curved_wall_leaves_the_wall_as_thick_as_it_is() {
    // A 2 mm wall curved about the y axis (radius 18 to 20, from 150° down)
    // cut by the straight side x = 7 of a hole through it: the hole's side
    // meets the inner surface at 67°, so a ray from either leaves through
    // the other next to their edge (mitcad#689).
    let outer_start = (7.0_f64 / 20.0).acos();
    let inner_end = (7.0_f64 / 18.0).acos();
    let end = 5.0 * PI / 6.0;
    let mut profile = arc([0.0, 0.0], 20.0, outer_start, end, 16);
    profile.extend(arc([0.0, 0.0], 18.0, end, inner_end, 16));
    let mesh = prism(&profile, 10.0);
    let found = print_analysis::thickness(&mesh, 0.8, &|| false).unwrap();
    assert!(
        found.thin.is_empty(),
        "{:?} {:?}",
        found.thin,
        found.thinnest
    );
    // The wall between the facets of its two sides.
    let wall = found.thinnest.unwrap();
    assert!(wall > 1.9 && wall < 2.1, "{wall} at {:?}", found.at);
}

#[test]
fn a_thin_fin_next_to_an_edge_is_still_found() {
    // A 20 x 5 mm base with a 0.3 mm fin standing on it flush with its side
    // (x 0 to 0.3, z 5 to 10): the fin's sides are parallel, a real thin
    // wall next to the edge of the base's side.
    let mesh = prism(
        &[
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 5.0],
            [0.3, 5.0],
            [0.3, 10.0],
            [0.0, 10.0],
        ],
        10.0,
    );
    let found = print_analysis::thickness(&mesh, 0.8, &|| false).unwrap();
    assert!(
        close(found.thinnest.unwrap(), 0.3, TOL),
        "{:?}",
        found.thinnest
    );
    assert!(!found.thin.is_empty());
    assert!(found.regions >= 1);
    for &t in &found.thin {
        let corners = mesh.triangles[t].map(|i| mesh.vertices[i as usize]);
        assert!(
            corners.iter().all(|p| close(p[0], 0.0, TOL))
                || corners.iter().all(|p| close(p[0], 0.3, TOL)),
            "{corners:?}"
        );
    }
}

/// The area of a triangle given by its corners, mm².
fn triangle_area(p: &[[f64; 3]; 3]) -> f64 {
    let u: [f64; 3] = std::array::from_fn(|i| p[1][i] - p[0][i]);
    let v: [f64; 3] = std::array::from_fn(|i| p[2][i] - p[0][i]);
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() / 2.0
}

/// The thin pieces that lie in the plane z = `z`.
fn pieces_at(found: &print_analysis::Thickness, z: f64) -> Vec<[[f64; 3]; 3]> {
    found
        .pieces
        .iter()
        .filter(|p| p.iter().all(|c| close(c[2], z, TOL)))
        .copied()
        .collect()
}

#[test]
fn a_thin_lip_at_a_plate_edge_colours_a_band_not_the_plate() {
    // mitcad#767: a 40 x 40 mm plate, 5 mm thick, with a 0.5 mm lip along
    // its edge x = 40 (x 38 to 40). The plate's bottom is one face of two
    // triangles that span the plate; one of them reaches the lip with its
    // long edge, so some of its samples are thin and most are not. Only
    // the part of it under the lip is shown, not the triangle.
    let mesh = prism(
        &[
            [0.0, 0.0],
            [40.0, 0.0],
            [40.0, 0.5],
            [38.0, 0.5],
            [38.0, 5.0],
            [0.0, 5.0],
        ],
        40.0,
    );
    let found = print_analysis::thickness(&mesh, 0.8, &|| false).unwrap();
    // The thinnest wall is the lip's, as before.
    assert!(
        close(found.thinnest.unwrap(), 0.5, TOL),
        "{:?}",
        found.thinnest
    );
    let at = found.at.unwrap();
    assert!(close(at[2], 0.0, TOL) || close(at[2], 0.5, TOL), "{at:?}");
    // Thin pieces only on the bottom and on the lip's top.
    let bottom = pieces_at(&found, 0.0);
    let lip = pieces_at(&found, 0.5);
    assert_eq!(
        bottom.len() + lip.len(),
        found.pieces.len(),
        "{:?}",
        found.pieces
    );
    // The lip's top is thin all over: its two triangles whole.
    assert_eq!(lip.len(), 2, "{lip:?}");
    let lip_area: f64 = lip.iter().map(triangle_area).sum();
    assert!(close(lip_area, 80.0, TOL), "{lip_area}");
    // On the bottom, a band along the lip within about one sample distance
    // (2 mm) of it, not the triangle across the plate (800 mm²).
    assert!(!bottom.is_empty());
    for piece in &bottom {
        for corner in piece {
            assert!(corner[0] >= 36.0 - TOL, "a piece reaches x {corner:?}");
        }
    }
    let bottom_area: f64 = bottom.iter().map(triangle_area).sum();
    assert!(
        bottom_area > 70.0 && bottom_area < 120.0,
        "{bottom_area} mm² of the bottom shown thin"
    );
    // The bottom triangle under the lip is still listed as thin.
    assert!(found.thin.iter().any(|&t| {
        mesh.triangles[t]
            .iter()
            .all(|&v| close(mesh.vertices[v as usize][2], 0.0, TOL))
    }));
    // The lip's top and the band under it: one wall, one region.
    assert_eq!(found.regions, 1);
}

#[test]
fn thin_lips_at_opposite_edges_are_two_regions() {
    // mitcad#767: lips 0.5 mm thick along both edges x = 0 and x = 40 of a
    // 5 mm plate. The bottom's two triangles span the plate from lip to
    // lip and share corners, so as whole triangles the lips were one
    // region; their thin pieces do not touch: two regions.
    let mesh = prism(
        &[
            [0.0, 0.0],
            [40.0, 0.0],
            [40.0, 0.5],
            [38.0, 0.5],
            [38.0, 5.0],
            [2.0, 5.0],
            [2.0, 0.5],
            [0.0, 0.5],
        ],
        40.0,
    );
    let found = print_analysis::thickness(&mesh, 0.8, &|| false).unwrap();
    assert!(close(found.thinnest.unwrap(), 0.5, TOL));
    for piece in pieces_at(&found, 0.0) {
        for corner in piece {
            assert!(
                corner[0] <= 4.0 + TOL || corner[0] >= 36.0 - TOL,
                "a piece reaches x {corner:?}"
            );
        }
    }
    assert_eq!(found.regions, 2);
}

#[test]
fn a_uniformly_thin_plate_is_shown_whole() {
    // Thin all over: every triangle of the top and the bottom whole, as
    // before the pieces (mitcad#767), one region.
    let mesh = box_mesh([0.0; 3], [40.0, 40.0, 0.5]);
    let found = print_analysis::thickness(&mesh, 0.8, &|| false).unwrap();
    assert!(close(found.thinnest.unwrap(), 0.5, TOL));
    assert_eq!(found.thin.len(), 4, "{:?}", found.thin);
    let whole: Vec<[[f64; 3]; 3]> = found
        .thin
        .iter()
        .map(|&t| mesh.triangles[t].map(|v| mesh.vertices[v as usize]))
        .collect();
    assert_eq!(found.pieces, whole);
    let shown: f64 = found.pieces.iter().map(triangle_area).sum();
    assert!(close(shown, 3200.0, TOL), "{shown}");
    assert_eq!(found.regions, 1);
}

#[test]
fn gaps_are_touching_narrow_or_clear() {
    // mitcad#1519: 0.2 mm under a 0.3 mm gap is narrow, 0.5 mm clear;
    // touching or overlapping bodies are interference's.
    use print_analysis::GapKind;
    assert_eq!(print_analysis::gap_kind(0.2, false, 0.3), GapKind::Narrow);
    assert_eq!(print_analysis::gap_kind(0.5, false, 0.3), GapKind::Clear);
    assert_eq!(print_analysis::gap_kind(0.0, false, 0.3), GapKind::Touching);
    assert_eq!(
        print_analysis::gap_kind(1e-7, false, 0.3),
        GapKind::Touching
    );
    assert_eq!(print_analysis::gap_kind(0.0, true, 0.3), GapKind::Touching);
    // At the gap, within the baseline tolerance: not narrow.
    assert_eq!(
        print_analysis::gap_kind(0.3 - 1e-7, false, 0.3),
        GapKind::Clear
    );
    assert!(print_analysis::check_min_gap(0.3).is_ok());
    for bad in [0.0, -0.1, f64::NAN, f64::INFINITY] {
        assert!(print_analysis::check_min_gap(bad).is_err(), "{bad}");
    }
}

#[test]
fn only_bodies_whose_boxes_lie_within_the_gap_are_measured() {
    use crate::kernel::BoundingBox;
    let cube = |x: f64| {
        Some(BoundingBox {
            min: [x, 0.0, 0.0],
            max: [x + 20.0, 20.0, 20.0],
        })
    };
    // 0.2 mm, then 0.5 mm apart along X; an empty body; one far away.
    let boxes = [cube(0.0), cube(20.2), cube(40.7), None, cube(100.0)];
    assert_eq!(print_analysis::gap_candidates(&boxes, 0.3), [(0, 1)]);
    assert_eq!(
        print_analysis::gap_candidates(&boxes, 0.6),
        [(0, 1), (1, 2)]
    );
    // Boxes that overlap are candidates whatever the gap.
    let overlapping = [cube(0.0), cube(10.0)];
    assert_eq!(print_analysis::gap_candidates(&overlapping, 0.01), [(0, 1)]);
}
