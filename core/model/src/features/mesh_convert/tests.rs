// SPDX-License-Identifier: MIT
//! Convert Mesh (mitcad#462): the sections found in meshes built here (the
//! conversion adapted from SolidCraft) and the feature on the mock kernel.
//! The OCCT kernel's solids are checked by `cli.mesh_convert` and the
//! `.solidcraft` test design `mesh-convert` (`cli.import_solidcraft`).

use std::f64::consts::{PI, TAU};

use serde_json::json;

use super::prism::{certify_nesting, fit_loop};
use super::{SectionLoop, convert_prismatic_mesh, profile_regions};
use crate::exchange::{BaseInput, ImportBody, TriangleMesh};
use crate::kernel::BoundingBox;
use crate::test_support::{block, def};
use crate::testing::MockShape;
use crate::{Document, FeatureStatus};

/// The two triangles of a wall from the edge `a`-`b` of the low cap up to
/// the high one (`m` vertices on), facing out when the edge runs
/// counter-clockwise around the material seen from above.
fn wall(triangles: &mut Vec<[u32; 3]>, a: u32, b: u32, m: u32) {
    triangles.push([a, b, b + m]);
    triangles.push([a, b + m, a + m]);
}

/// A cap triangle (counter-clockwise seen from above) on the high cap and,
/// turned over, on the low one.
fn caps(triangles: &mut Vec<[u32; 3]>, t: [u32; 3], m: u32) {
    triangles.push([t[0] + m, t[1] + m, t[2] + m]);
    triangles.push([t[0], t[2], t[1]]);
}

/// The points of the section at z 0 and at `height`.
fn levels(section: &[[f64; 2]], height: f64) -> Vec<[f64; 3]> {
    let mut vertices: Vec<[f64; 3]> = section.iter().map(|p| [p[0], p[1], 0.0]).collect();
    vertices.extend(section.iter().map(|p| [p[0], p[1], height]));
    vertices
}

/// A convex polygon (counter-clockwise) extruded by `height`, its caps fans
/// from the polygon's centre.
fn extruded(polygon: &[[f64; 2]], height: f64) -> TriangleMesh {
    let n = polygon.len() as u32;
    let centre = [
        polygon.iter().map(|p| p[0]).sum::<f64>() / f64::from(n),
        polygon.iter().map(|p| p[1]).sum::<f64>() / f64::from(n),
    ];
    let mut section = polygon.to_vec();
    section.push(centre);
    let m = n + 1;
    let mut triangles = Vec::new();
    for k in 0..n {
        caps(&mut triangles, [n, k, (k + 1) % n], m);
        wall(&mut triangles, k, (k + 1) % n, m);
    }
    TriangleMesh {
        vertices: levels(&section, height),
        triangles,
    }
}

/// `sides` corners on a circle, counter-clockwise from angle `start`.
fn ring(center: [f64; 2], radius: f64, sides: usize, start: f64) -> Vec<[f64; 2]> {
    (0..sides)
        .map(|k| {
            let a = start + TAU * k as f64 / sides as f64;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        })
        .collect()
}

/// A plate `size` square and `height` thick with a hole of `sides` corners
/// (a multiple of four) on a circle of `radius` at its middle. Each cap is a
/// fan from each plate corner to a quarter of the hole and a strip between
/// two corners and the hole's corner between them.
fn plate(size: f64, radius: f64, sides: usize, height: f64) -> TriangleMesh {
    let c = size / 2.0;
    let mut section = vec![[size, size], [0.0, size], [0.0, 0.0], [size, 0.0]];
    section.extend(ring([c, c], radius, sides, 0.0));
    let n = sides as u32;
    let m = section.len() as u32;
    let hole = |k: u32| 4 + k % n;
    let mut triangles = Vec::new();
    for q in 0..4 {
        for k in q * n / 4..(q + 1) * n / 4 {
            caps(&mut triangles, [hole(k), q, hole(k + 1)], m);
        }
        caps(&mut triangles, [(q + 3) % 4, q, hole(q * n / 4)], m);
        wall(&mut triangles, (q + 3) % 4, q, m);
    }
    for k in 0..n {
        wall(&mut triangles, hole(k + 1), hole(k), m);
    }
    TriangleMesh {
        vertices: levels(&section, height),
        triangles,
    }
}

/// The mesh turned by `angle` about `axis` and moved by `offset`.
fn placed(mesh: &TriangleMesh, axis: [f64; 3], angle: f64, offset: [f64; 3]) -> TriangleMesh {
    let l = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let [x, y, z] = axis.map(|v| v / l);
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    let r = [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
    ];
    TriangleMesh {
        vertices: mesh
            .vertices
            .iter()
            .map(|p| {
                std::array::from_fn(|i| {
                    r[i][0] * p[0] + r[i][1] * p[1] + r[i][2] * p[2] + offset[i]
                })
            })
            .collect(),
        triangles: mesh.triangles.clone(),
    }
}

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// The plate's hole: a cylinder when its 16 chords lie within the
/// tolerance (each 22.5 degrees of radius 5 deviates 0.0961 from the arc),
/// else 16 planes; the plate's outline four planes.
#[test]
fn a_plate_with_a_round_hole_becomes_planes_and_a_cylinder() {
    let mesh = plate(20.0, 5.0, 16, 10.0);
    let prism = convert_prismatic_mesh(&mesh, 0.1).unwrap();
    assert!(near(prism.height, 10.0, 1e-9), "{}", prism.height);
    let normal = prism.frame.normal();
    assert!(near(normal[2].abs(), 1.0, 1e-12), "{normal:?}");
    assert_eq!(prism.regions.len(), 1);
    let region = &prism.regions[0];
    let SectionLoop::Polygon(outline) = &region.outer else {
        panic!("{:?}", region.outer)
    };
    assert_eq!(outline.len(), 4);
    assert_eq!(region.holes.len(), 1);
    let SectionLoop::Circle { center, radius } = region.holes[0] else {
        panic!("{:?}", region.holes[0])
    };
    assert!(near(radius, 5.0, 1e-9), "{radius}");
    assert!(
        near(center[0], 10.0, 1e-9) && near(center[1], 10.0, 1e-9),
        "{center:?}"
    );
    assert!(near(prism.volume(), (400.0 - 25.0 * PI) * 10.0, 1e-6));

    // Within 0.05 the chords stay planes: the mesh's own volume.
    let prism = convert_prismatic_mesh(&mesh, 0.05).unwrap();
    let SectionLoop::Polygon(hole) = &prism.regions[0].holes[0] else {
        panic!("{:?}", prism.regions[0].holes[0])
    };
    assert_eq!(hole.len(), 16);
    let polygon = 8.0 * 25.0 * (PI / 8.0).sin();
    assert!(near(prism.volume(), (400.0 - polygon) * 10.0, 1e-6));
}

/// The same plate turned and moved anywhere: the direction is found among
/// its triangles' normals.
#[test]
fn a_plate_in_any_orientation_converts() {
    let axis = [1.0, 2.0, 3.0];
    let mesh = placed(&plate(20.0, 5.0, 16, 10.0), axis, 0.73, [31.0, -17.0, 9.0]);
    let prism = convert_prismatic_mesh(&mesh, 0.1).unwrap();
    assert!(near(prism.height, 10.0, 1e-9), "{}", prism.height);
    let up = placed(
        &TriangleMesh {
            vertices: vec![[0.0, 0.0, 1.0]],
            triangles: Vec::new(),
        },
        axis,
        0.73,
        [0.0; 3],
    )
    .vertices[0];
    let normal = prism.frame.normal();
    let along: f64 = (0..3).map(|i| normal[i] * up[i]).sum();
    assert!(near(along.abs(), 1.0, 1e-9), "{normal:?} {up:?}");
    let region = &prism.regions[0];
    let SectionLoop::Circle { radius, .. } = region.holes[0] else {
        panic!("{:?}", region.holes[0])
    };
    assert!(near(radius, 5.0, 1e-9), "{radius}");
    assert!(near(prism.volume(), (400.0 - 25.0 * PI) * 10.0, 1e-6));
}

/// A box is exact whichever of its directions is taken; a round bar is one
/// cylinder.
#[test]
fn boxes_are_exact_and_bars_round() {
    let mesh = extruded(&[[0.0, 0.0], [10.0, 0.0], [10.0, 20.0], [0.0, 20.0]], 30.0);
    let prism = convert_prismatic_mesh(&mesh, 0.01).unwrap();
    assert!(near(prism.volume(), 6000.0, 1e-9), "{}", prism.volume());
    assert_eq!(prism.regions.len(), 1);
    assert!(prism.regions[0].holes.is_empty());
    assert!(matches!(&prism.regions[0].outer, SectionLoop::Polygon(p) if p.len() == 4));

    let mesh = extruded(&ring([3.0, -2.0], 4.0, 24, 0.0), 12.0);
    let prism = convert_prismatic_mesh(&mesh, 0.05).unwrap();
    let SectionLoop::Circle { center, radius } = prism.regions[0].outer else {
        panic!("{:?}", prism.regions[0].outer)
    };
    assert!(near(radius, 4.0, 1e-9), "{radius}");
    assert!(
        near(center[0], 3.0, 1e-9) && near(center[1], -2.0, 1e-9),
        "{center:?}"
    );
    assert!(near(prism.volume(), PI * 16.0 * 12.0, 1e-6));
}

/// Meshes that are no straight extrusion of one profile, open or inside
/// out meshes, bad tolerances and hostile coordinates are refused.
#[test]
fn what_is_no_prism_is_refused() {
    let not_prism = "is not a supported constant-section prism";
    // A pyramid and a tapered block.
    let pyramid = TriangleMesh {
        vertices: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [5.0, 5.0, 8.0],
        ],
        triangles: vec![
            [0, 2, 1],
            [0, 3, 2],
            [0, 1, 4],
            [1, 2, 4],
            [2, 3, 4],
            [3, 0, 4],
        ],
    };
    let error = convert_prismatic_mesh(&pyramid, 0.05).unwrap_err();
    assert!(error.contains(not_prism), "{error}");
    let mut tapered = levels(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], 10.0);
    for p in &mut tapered[4..] {
        p[0] = 2.0 + p[0] * 0.6;
        p[1] = 2.0 + p[1] * 0.6;
    }
    let mut triangles = Vec::new();
    caps(&mut triangles, [0, 1, 2], 4);
    caps(&mut triangles, [0, 2, 3], 4);
    for k in 0..4 {
        wall(&mut triangles, k, (k + 1) % 4, 4);
    }
    let tapered = TriangleMesh {
        vertices: tapered,
        triangles,
    };
    let error = convert_prismatic_mesh(&tapered, 0.05).unwrap_err();
    assert!(error.contains(not_prism), "{error}");

    // Open, and inside out.
    let open = TriangleMesh {
        vertices: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        triangles: vec![[0, 1, 2]],
    };
    let error = convert_prismatic_mesh(&open, 0.05).unwrap_err();
    assert!(error.contains("closed"), "{error}");
    let mut inside_out = plate(20.0, 5.0, 16, 10.0);
    for t in &mut inside_out.triangles {
        t.swap(1, 2);
    }
    let error = convert_prismatic_mesh(&inside_out, 0.1).unwrap_err();
    assert!(error.contains("inwards"), "{error}");

    // Tolerances outside 0.000001..1 mm.
    let block = extruded(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], 10.0);
    for tolerance in [f64::NAN, f64::INFINITY, -1.0, 0.0, 2.0] {
        let error = convert_prismatic_mesh(&block, tolerance).unwrap_err();
        assert!(error.contains("tolerance"), "{tolerance}: {error}");
    }

    // Coordinates the conversion cannot weld.
    for coordinate in [f64::NAN, f64::INFINITY, f64::MAX, 1e30] {
        let mesh = TriangleMesh {
            vertices: vec![[coordinate, 0.0, 0.0]],
            triangles: Vec::new(),
        };
        let error = convert_prismatic_mesh(&mesh, 0.05).unwrap_err();
        assert!(error.contains("coordinates"), "{coordinate}: {error}");
    }
}

/// A mesh over the budget says how big it is, the limit, and that only
/// constant-section parts convert.
#[test]
fn a_mesh_over_the_budget_says_how_big_it_is() {
    let n = super::prism::MAX_TRIANGLES + 1;
    let vertices: Vec<[f64; 3]> = (0..n + 2)
        .map(|k| [(k / 2) as f64, (k % 2) as f64, 0.0])
        .collect();
    let triangles: Vec<[u32; 3]> = (0..n as u32).map(|i| [i, i + 1, i + 2]).collect();
    let error = convert_prismatic_mesh(
        &TriangleMesh {
            vertices,
            triangles,
        },
        0.05,
    )
    .unwrap_err();
    assert!(
        error.contains("50001 triangles") && error.contains("50000"),
        "{error}"
    );
    assert!(
        error.contains("constant-section") && error.contains("freeform"),
        "{error}"
    );
}

/// Corners on a circle are a circle only when there are 12 or more and
/// every chord lies within the tolerance; corners in a line go.
#[test]
fn loops_are_fitted_within_the_tolerance() {
    let hexagon = ring([0.0, 0.0], 10.0, 6, 0.0);
    assert!(matches!(fit_loop(&hexagon, 1.0, 1e-6), SectionLoop::Polygon(p) if p.len() == 6));
    let sixteen = ring([0.0, 0.0], 10.0, 16, 0.0);
    assert!(matches!(fit_loop(&sixteen, 0.001, 1e-6), SectionLoop::Polygon(p) if p.len() == 16));
    assert!(matches!(
        fit_loop(&sixteen, 0.2, 1e-6),
        SectionLoop::Circle { .. }
    ));
    let square = [
        [0.0, 0.0],
        [5.0, 0.0],
        [10.0, 0.0],
        [10.0, 5.0],
        [10.0, 10.0],
        [5.0, 10.0],
        [0.0, 10.0],
        [0.0, 5.0],
    ];
    assert!(matches!(fit_loop(&square, 0.05, 1e-6), SectionLoop::Polygon(p) if p.len() == 4));
}

/// A circle fitted to a hole may not reach across the wall next to it, and
/// a section of more than 50 loops is too complex.
#[test]
fn fitting_cannot_open_a_hole_through_a_nearby_wall() {
    let outer = vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    let inner = ring([0.0, 0.0], 1.02, 12, PI / 12.0);
    let fitted = fit_loop(&inner, 0.05, 1e-6);
    assert!(matches!(fitted, SectionLoop::Circle { .. }), "{fitted:?}");
    let error = certify_nesting(
        &[outer.clone(), inner],
        &[SectionLoop::Polygon(outer), fitted],
        1e-6,
    )
    .unwrap_err();
    assert!(error.contains("cross"), "{error}");

    let triangles: Vec<Vec<[f64; 2]>> = (0..51)
        .map(|i| {
            let x = 10.0 * f64::from(i);
            vec![[x, 0.0], [x + 1.0, 0.0], [x, 1.0]]
        })
        .collect();
    let fitted: Vec<SectionLoop> = triangles
        .iter()
        .map(|t| SectionLoop::Polygon(t.clone()))
        .collect();
    let error = certify_nesting(&triangles, &fitted, 1e-6).unwrap_err();
    assert!(error.contains("too complex"), "{error}");
}

/// The section's edges are curves c1, c2, ... loop by loop, as a sketch's
/// rectangle and circle would be.
#[test]
fn the_section_is_named_as_a_sketch_profile() {
    let prism = convert_prismatic_mesh(&plate(20.0, 5.0, 16, 10.0), 0.1).unwrap();
    let regions = profile_regions(&prism);
    assert_eq!(regions.len(), 1);
    assert_eq!(
        regions[0].key.to_string(),
        "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"
    );
    assert_eq!(regions[0].loops.len(), 2);
    assert_eq!(regions[0].loops[1].segments.len(), 1);
    assert_eq!(regions[0].loops[1].segments[0].key.to_string(), "c5");
    // The outline runs counter-clockwise from its lowest corner.
    let starts: Vec<[f64; 2]> = regions[0].loops[0]
        .segments
        .iter()
        .map(|s| match s.geometry {
            crate::profile::SegmentGeometry::Line { start, .. } => start,
            ref other => panic!("{other:?}"),
        })
        .collect();
    let expected = [[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]];
    for (start, corner) in starts.iter().zip(expected) {
        assert!(
            near(start[0], corner[0], 1e-9) && near(start[1], corner[1], 1e-9),
            "{starts:?}"
        );
    }
}

/// Block (F2.b0) and a mesh body (F3.b0 from the base feature "scan"; the
/// mock kernel's mesh of it is its box, 10 mm each way).
fn block_and_mesh() -> (Document<crate::testing::MockKernel>, crate::ids::BodyUid) {
    let mut b = block();
    let imported = b
        .doc
        .add_base_feature(BaseInput {
            name: Some("scan".to_owned()),
            ..BaseInput::new(vec![ImportBody {
                name: Some("Scan".to_owned()),
                color: None,
                shape: MockShape::imported("mesh", 1),
                face_colors: Vec::new(),
            }])
        })
        .unwrap();
    let mesh = imported.bodies[0].uid;
    assert_eq!(mesh.to_string(), "F3.b0");
    (b.doc, mesh)
}

/// The feature replaces the mesh body by the kernel's extrusion of its
/// section, faces named as an extrusion's after the feature; suppressed,
/// the mesh body is back as it was.
#[test]
fn convert_mesh_rebuilds_the_body_in_place() {
    let (mut doc, mesh) = block_and_mesh();
    let before = doc.body_shape(mesh).unwrap().clone();
    let uid = doc
        .add_feature(
            &def(json!({"type": "mesh_convert", "body": "F3.b0", "tolerance": 0.05})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(uid.to_string(), "F4");
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.feature(uid).unwrap().name, "ConvertMesh1");
    let shape = doc.body_shape(mesh).unwrap();
    assert!(
        shape.history.starts_with("prism(F4:0..10)"),
        "{}",
        shape.history
    );
    let names: Vec<String> = shape.faces.iter().map(ToString::to_string).collect();
    assert_eq!(names.len(), 6, "{names:?}");
    for name in [
        "F4:side(c1[c4,c2])",
        "F4:start(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})",
        "F4:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})",
    ] {
        assert!(names.iter().any(|n| n == name), "{name} not in {names:?}");
    }
    assert_eq!(
        shape.bounds,
        Some(BoundingBox {
            min: [0.0; 3],
            max: [10.0; 3]
        })
    );
    assert!(doc.kernel().count("triangle_mesh") >= 1);
    // The body kept its name.
    assert!(
        doc.bodies()
            .iter()
            .any(|b| b.uid == mesh && b.name == "Scan")
    );

    doc.set_suppressed(uid, true).unwrap();
    assert_eq!(doc.body_shape(mesh), Some(&before));
}

/// A body that is no mesh body, and a tolerance outside its range, are
/// refused.
#[test]
fn convert_mesh_refuses_solids_and_bad_tolerances() {
    let (mut doc, _) = block_and_mesh();
    let uid = doc
        .add_feature(
            &def(json!({"type": "mesh_convert", "body": "F2.b0", "tolerance": 0.05})),
            None,
        )
        .unwrap()
        .uid;
    match doc.status(uid).cloned() {
        Some(FeatureStatus::Failed(message)) => assert!(
            message.starts_with("body F2.b0 (from Extrude1) is not a mesh body"),
            "{message}"
        ),
        other => panic!("{other:?}"),
    }
    doc.delete_feature(uid, false).unwrap();
    for tolerance in [0.0, 2.0] {
        let error = doc
            .add_feature(
                &def(json!({"type": "mesh_convert", "body": "F3.b0", "tolerance": tolerance})),
                None,
            )
            .unwrap_err();
        assert!(error.to_string().contains("tolerance"), "{error}");
    }
}
