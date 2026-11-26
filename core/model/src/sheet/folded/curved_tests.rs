// SPDX-License-Identifier: MIT
//! Curved panels' charts (mitcad#2435): a cone's development and its
//! inverse, a chart turned round its axis, the back surface of a plate and
//! the plane a curved panel develops into.

use super::*;

fn cone(inner_is_front: bool, turn: f64) -> ConeEmbedding {
    // Half angle asin 0.6 about +z, the apex at the origin.
    ConeEmbedding {
        apex: [0.0, 0.0, 0.0],
        axis: [0.0, 0.0, 1.0],
        radial_start: [1.0, 0.0, 0.0],
        angular_direction: [0.0, turn, 0.0],
        half_angle: 0.6_f64.asin(),
        neutral_depth: 0.6,
        angle_extent: 0.6 * std::f64::consts::PI,
        inner_is_front,
    }
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    norm(sub(a, b)) <= 1e-9
}

#[test]
fn cone_chart_points_lie_on_their_cones() {
    // The neutral cone's point 50 from the apex on the start generator:
    // radius 30, height 40; the front of an outer face is 1.4 (2 - 0.6)
    // further out along the normal (0.8, 0, -0.6).
    let outer = cone(false, 1.0);
    assert!(close(outer.point([50.0, 0.0], 1.4, 2.0), [30.0, 0.0, 40.0]));
    assert!(close(
        outer.point([50.0, 0.0], 0.0, 2.0),
        [30.0 + 1.4 * 0.8, 0.0, 40.0 - 1.4 * 0.6]
    ));
    // A developed angle of 0.6 pi / 2 is a quarter turn about the axis,
    // towards the angular direction.
    let quarter = 0.3 * std::f64::consts::PI;
    let p = [50.0 * quarter.cos(), 50.0 * quarter.sin()];
    assert!(close(outer.point(p, 1.4, 2.0), [0.0, 30.0, 40.0]));
    assert!(close(
        cone(false, -1.0).point(p, 1.4, 2.0),
        [0.0, -30.0, 40.0]
    ));
    // An inner face's front is 0.6 inside the neutral cone.
    assert!(close(
        cone(true, 1.0).point([50.0, 0.0], 0.0, 2.0),
        [30.0 - 0.6 * 0.8, 0.0, 40.0 + 0.6 * 0.6]
    ));
}

#[test]
fn cone_coordinates_invert_the_chart() {
    for inner in [false, true] {
        for turn in [1.0, -1.0] {
            let c = cone(inner, turn);
            for (p, depth) in [
                ([50.0, 0.0], 0.0),
                ([60.0, 30.0], 1.5),
                ([-20.0, 70.0], 2.0),
            ] {
                let at = c.point(p, depth, 2.0);
                let (back, d) = c.coordinates(at, 2.0).expect("on the chart");
                assert!((back[0] - p[0]).abs() <= 1e-9 && (back[1] - p[1]).abs() <= 1e-9);
                assert!((d - depth).abs() <= 1e-9);
            }
        }
    }
    // Past the chart's angle: none.
    let c = cone(false, 1.0);
    let beyond = 0.6 * std::f64::consts::PI + 0.2;
    let p = [50.0 * beyond.cos(), 50.0 * beyond.sin()];
    let wide = ConeEmbedding {
        angle_extent: 1.2 * std::f64::consts::PI - 0.01,
        ..c.clone()
    };
    assert!(c.coordinates(wide.point(p, 0.0, 2.0), 2.0).is_none());
}

#[test]
fn rebased_charts_keep_their_material_in_place() {
    // A cone's chart turned by 0.5 rad of developed angle: every material
    // point stays where it was in the model.
    let c = cone(false, -1.0);
    let region = polygon(
        FeatureUid(7),
        1,
        &[[50.0, 1.0], [90.0, 1.0], [90.0, 20.0], [50.0, 20.0]],
    )
    .unwrap();
    let patch = SheetPatch {
        domain: SheetDomainRef::Panel(PanelId {
            feature: FeatureUid(7),
            slot: 0,
        }),
        regions: vec![region],
        embedding: SheetEmbedding::Cone(c.clone()),
    };
    let moved = rebased(&patch, 0.5, 1.0);
    let SheetEmbedding::Cone(m) = &moved.embedding else {
        panic!("a cone");
    };
    let (sp, cp) = (-0.5_f64).sin_cos();
    let p = [70.0, 10.0];
    let q = [cp * p[0] - sp * p[1], sp * p[0] + cp * p[1]];
    assert!(close(c.point(p, 0.5, 2.0), m.point(q, 0.5, 2.0)));
    // A cylinder's moved along u.
    let cylinder = CylinderEmbedding {
        centre: [0.0, 0.0, 0.0],
        axis: [0.0, 0.0, 1.0],
        radial_start: [1.0, 0.0, 0.0],
        angular_direction: [0.0, -1.0, 0.0],
        inside_radius: 18.0,
        neutral_radius: 19.0,
        angle_extent: 3.0,
        inner_is_front: false,
    };
    let patch = SheetPatch {
        embedding: SheetEmbedding::Cylinder(cylinder.clone()),
        ..patch
    };
    let moved = rebased(&patch, 7.0, 40.0);
    let embedding = SheetEmbedding::Cylinder(cylinder);
    assert!(close(
        super::super::develop::chart_point(&embedding, [20.0, 5.0], 1.0, 2.0),
        super::super::develop::chart_point(&moved.embedding, [13.0, 5.0], 1.0, 2.0)
    ));
}

#[test]
fn the_back_surface_is_the_thickness_behind_the_face() {
    let cylinder = |radius: f64| SurfaceGeometry::Cylinder {
        origin: [1.0, 2.0, 0.0],
        axis: [0.0, 0.0, 1.0],
        radius,
    };
    assert!(on_back(&cylinder(20.0), &cylinder(18.0), -2.0));
    assert!(!on_back(&cylinder(20.0), &cylinder(18.0), 2.0));
    assert!(on_back(&cylinder(18.0), &cylinder(20.0), 2.0));
    // A cone of half angle asin 0.6: moved 2 along its normal, its radius
    // at the same height grows by 2 / 0.8; its axis may run either way.
    let cone = |origin: [f64; 3], axis: [f64; 3], radius: f64, half: f64| SurfaceGeometry::Cone {
        origin,
        axis,
        radius,
        half_angle: half,
    };
    let half = 0.6_f64.asin();
    let front = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 30.0, half);
    assert!(on_back(
        &front,
        &cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 27.5, half),
        -2.0
    ));
    assert!(on_back(
        &front,
        &cone([0.0, 0.0, 4.0], [0.0, 0.0, 1.0], 30.5, half),
        -2.0
    ));
    assert!(on_back(
        &front,
        &cone([0.0, 0.0, 4.0], [0.0, 0.0, -1.0], 30.5, -half),
        -2.0
    ));
    assert!(!on_back(
        &front,
        &cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 32.5, half),
        -2.0
    ));
}

#[test]
fn a_curved_panel_develops_into_its_tangent_plane() {
    let c = cone(false, 1.0);
    let embedding = SheetEmbedding::Cone(c.clone());
    let p = [60.0, 20.0];
    let [origin, x, y, f] = tangent_frame(&embedding, p, 2.0).unwrap();
    assert!(close(origin, c.point(p, 0.0, 2.0)));
    assert!((norm(x) - 1.0).abs() <= 1e-12 && (norm(y) - 1.0).abs() <= 1e-12);
    assert!(dot(x, y).abs() <= 1e-12 && dot(x, f).abs() <= 1e-12 && dot(y, f).abs() <= 1e-12);
    // Along the chart's x and y the front moves by the chart's lengths at
    // first order (the neutral cone's development is an isometry; the
    // front, 1.4 out, stretches round the axis by 1.4 cot(a) / rho).
    let h = 1e-5;
    let dx = scale(
        sub(c.point([p[0] + h, p[1]], 1.4, 2.0), c.point(p, 1.4, 2.0)),
        1.0 / h,
    );
    assert!(norm(sub(dx, x)) <= 1e-4);
    // Into the material from an outer front: towards the axis.
    let radial = [origin[0], origin[1], 0.0];
    assert!(dot(f, radial) < 0.0);
}
