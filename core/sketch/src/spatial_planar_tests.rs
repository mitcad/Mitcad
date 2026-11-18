// SPDX-License-Identifier: MIT
//! The sketch plane of a 3D sketch as a planar sketch (mitcad#1550): a
//! converted rectangle back to the same planar sketch, a point out of the
//! plane named, the regions of a closed loop in the plane (not of one out
//! of it), and a kept circular pattern binding its copies in the plane and
//! back in the planar sketch; a point attached to a clockwise arc's end
//! kept there, and a projected arc that bounds a profile kept as profile
//! geometry (mitcad#1833).

use std::collections::BTreeMap;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

use crate::def::SketchDef;
use crate::expr::Unit;
use crate::ids::{EntityUid, FeatureUid};
use crate::parameters::{ParamId, Parameters};
use crate::pattern::{PatternCopy, PatternKind, SketchPattern};
use crate::profile::SketchFrame;
use crate::solve::Solved;
use crate::spatial::*;
use crate::spatial_convert::spatial_from_planar;
use crate::spatial_planar::{planar_from_spatial, planar_view};
use crate::{
    Constraint, ConstraintKind, ConstraintUid, Dimension, DimensionKind, Entity, Projection,
    ProjectionSource, Ref, SketchEntityRef,
};

const TOL: f64 = 1e-6;

fn p(n: u32) -> EntityUid {
    EntityUid(n)
}

fn k(n: u32) -> ConstraintUid {
    ConstraintUid(n)
}

fn parameter(params: &mut Parameters, value: f64) -> ParamId {
    params
        .add_dimension(&value.to_string(), Unit::MM, "", Some(FeatureUid(3)))
        .unwrap()
}

/// The evaluation of `def` on the XY plane.
fn output(def: &SpatialSketchDef, params: &Parameters) -> SpatialSketchOutput {
    output_with(def, params, SpatialInputs::default())
}

/// The evaluation of `def` on the XY plane with its attachments' targets
/// and reference curves.
fn output_with(
    def: &SpatialSketchDef,
    params: &Parameters,
    inputs: SpatialInputs,
) -> SpatialSketchOutput {
    let mut value = |id| params.value(id).ok_or_else(|| "no value".to_owned());
    let solved = def
        .build_with(&mut value, &inputs)
        .and_then(|mut model| model.solve())
        .unwrap_or_else(|e| panic!("{e}"));
    SpatialSketchOutput {
        frame: Some(SketchFrame::XY),
        curves: def.curves.iter().map(|c| (c.id, c.clone())).collect(),
        targets: inputs.targets,
        references: inputs.curves,
        surfaces: inputs.surfaces,
        solved: Some(solved),
        failure: None,
    }
}

/// The planar solve of `def`.
fn solve_planar(def: &SketchDef, params: &Parameters) -> Solved {
    let mut value = |id| params.value(id).ok_or_else(|| "no value".to_owned());
    def.solve_with(&mut value, &|_| String::new())
        .unwrap_or_else(|e| panic!("{e:?}"))
}

/// The position of the point `id` among `def`'s entities.
fn point_at(def: &SketchDef, id: EntityUid) -> [f64; 2] {
    match def.entity(id).map(|e| &e.kind) {
        Some(crate::EntityKind::Point { at }) => *at,
        other => panic!("p{} is not a point: {other:?}", id.0),
    }
}

/// A circle of radius 10 about the origin in the sketch plane, from angle
/// `start` to `end` about `normal` (seen along the sketch's normal, a
/// normal along -z runs it clockwise).
fn arc_curve(normal: [f64; 3], start: f64, end: f64) -> SpatialReferenceCurve {
    SpatialReferenceCurve::Conic {
        center: [0.0, 0.0, 0.0],
        normal,
        x_axis: [1.0, 0.0, 0.0],
        major: 10.0,
        minor: 10.0,
        start,
        end,
        closed: false,
    }
}

/// A rectangle 40 x 30, its first corner fixed: points p1-p4, lines
/// c5-c8, horizontal and vertical sides k1-k4, lengths k5 and k6.
fn rectangle(params: &mut Parameters) -> SketchDef {
    let mut def = SketchDef::<ParamId>::default();
    for (id, at) in [
        (1, [0.0, 0.0]),
        (2, [40.0, 0.0]),
        (3, [40.0, 30.0]),
        (4, [0.0, 30.0]),
    ] {
        def.entities.push(Entity::point(p(id), at));
    }
    def.entities[0].fixed = true;
    for (id, a, b) in [(5, 1, 2), (6, 2, 3), (7, 3, 4), (8, 4, 1)] {
        def.entities.push(Entity::new(
            p(id),
            crate::EntityKind::Line {
                start: p(a),
                end: p(b),
                centerline: false,
            },
        ));
    }
    for (id, kind) in [
        (1, ConstraintKind::Horizontal { line: p(5) }),
        (2, ConstraintKind::Horizontal { line: p(7) }),
        (3, ConstraintKind::Vertical { line: p(6) }),
        (4, ConstraintKind::Vertical { line: p(8) }),
    ] {
        def.constraints.push(Constraint::new(k(id), kind));
    }
    for (id, line, value) in [(5, 5, 40.0), (6, 6, 30.0)] {
        def.dimensions.push(Dimension::new(
            k(id),
            DimensionKind::Length { line: p(line) },
            Some(parameter(params, value)),
        ));
    }
    def
}

/// A converted rectangle converts back to the planar sketch it was: the
/// same entities, fixed corner, constraints and dimensions with their ids
/// and parameters; the in-plane fix and the On Sketch Plane relation are
/// what the plane holds by itself.
#[test]
fn a_converted_rectangle_converts_back_to_the_same_planar_sketch() {
    let mut params = Parameters::default();
    let planar = rectangle(&mut params);
    let converted = spatial_from_planar(&planar, None).unwrap();
    let def = converted.def;
    let flat = planar_from_spatial(&def, &output(&def, &params), &BTreeMap::new()).unwrap();
    assert!(flat.off_plane.is_empty());
    assert_eq!(flat.def.constraints, planar.constraints);
    assert_eq!(flat.def.dimensions, planar.dimensions);
    let mut dropped = converted.fixes.clone();
    dropped.extend(converted.on_sketch_plane.iter().map(|(id, _)| *id));
    assert_eq!(flat.dropped, dropped);
    assert_eq!(flat.def.entities.len(), planar.entities.len());
    for (back, was) in flat.def.entities.iter().zip(&planar.entities) {
        assert_eq!(
            (back.id, back.fixed, back.construction),
            (was.id, was.fixed, was.construction)
        );
        match (&back.kind, &was.kind) {
            (crate::EntityKind::Point { at: a }, crate::EntityKind::Point { at: b }) => {
                assert!((a[0] - b[0]).abs() <= TOL && (a[1] - b[1]).abs() <= TOL);
            }
            (a, b) => assert_eq!(a, b),
        }
    }
    assert_eq!(flat.def.plane, planar.plane);
}

/// Geometry out of the sketch plane is named, and nothing is converted.
#[test]
fn a_point_off_the_plane_is_named() {
    let mut def = SpatialSketchDef::default();
    for (id, at) in [
        (1, [0.0, 0.0, 0.0]),
        (2, [10.0, 0.0, 0.0]),
        (3, [10.0, 0.0, 5.0]),
    ] {
        def.points.push(SpatialPoint { id: p(id), at });
    }
    def.curves.push(SpatialCurve::line(p(4), p(1), p(2), false));
    def.curves.push(SpatialCurve::line(p(5), p(2), p(3), false));
    let refused = planar_from_spatial(
        &def,
        &output(&def, &Parameters::default()),
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(
        refused.contains("p3 is 5 mm off the sketch plane"),
        "{refused}"
    );
}

/// W9 (mitcad#1550): a closed loop in the sketch plane bounds a region with
/// the keys a planar sketch's square has; a closed loop out of the plane
/// bounds none.
#[test]
fn a_loop_in_the_plane_bounds_a_region() {
    let mut def = SpatialSketchDef::default();
    let square = [
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [10.0, 10.0, 0.0],
        [0.0, 10.0, 0.0],
    ];
    let bent = [
        [20.0, 0.0, 0.0],
        [30.0, 0.0, 0.0],
        [30.0, 10.0, 5.0],
        [20.0, 10.0, 0.0],
    ];
    let mut next = 1;
    for corners in [square, bent] {
        let first = next;
        for at in corners {
            def.points.push(SpatialPoint { id: p(next), at });
            next += 1;
        }
        for i in 0..4 {
            def.curves.push(SpatialCurve::line(
                p(next),
                p(first + i),
                p(first + (i + 1) % 4),
                false,
            ));
            next += 1;
        }
    }
    let output = output(&def, &Parameters::default());
    let view = planar_view(&def, &output);
    let regions = view.def.regions_of(&view.solved, &[]);
    assert_eq!(regions.len(), 1, "{regions:?}");
    let key = regions[0].profile.key.to_string();
    for curve in ["c5", "c6", "c7", "c8"] {
        assert!(key.contains(curve), "{key}");
    }
    assert!(!key.contains("c13"), "{key}");
}

/// A circular pattern of a point (mitcad#1550): kept in the planar records,
/// it binds the copies about the centre in the plane, with the planar
/// sketch's freedom; back in the planar sketch as it was.
#[test]
fn a_kept_pattern_binds_its_copies_in_the_plane() {
    let mut params = Parameters::default();
    let mut planar = SketchDef::<ParamId>::default();
    planar.entities.push(Entity::point(p(1), [10.0, 0.0]));
    let mut center = Entity::point(p(2), [0.0, 0.0]);
    center.fixed = true;
    planar.entities.push(center);
    let mut copies = Vec::new();
    for (i, id) in [3, 4, 5].into_iter().enumerate() {
        planar.entities.push(Entity::point(p(id), [1.0, 1.0]));
        copies.push(PatternCopy {
            index: [i as u32 + 1, 0],
            entities: BTreeMap::from([(Ref::Point(p(1)), Ref::Point(p(id)))]),
        });
    }
    planar.patterns.push(SketchPattern {
        id: k(1),
        kind: PatternKind::Circular {
            center: p(2),
            count: 4,
            angle: parameter(&mut params, TAU),
        },
        entities: vec![Ref::Point(p(1))],
        copies,
        skipped: Vec::new(),
    });
    let converted = spatial_from_planar(&planar, None).unwrap();
    assert_eq!(converted.kept, ["k1 (pattern)"]);
    let def = converted.def;
    assert_eq!(def.planar.patterns, planar.patterns);
    let output = output(&def, &params);
    let solved = output.solved.as_ref().unwrap();
    // The original's two coordinates are free; its elevation and the
    // copies' are on the plane.
    assert_eq!(solved.status.dof, 2, "{:?}", solved.status);
    // The original is free, so the solve may move it as well as the copies
    // towards each other: each copy is the solved original turned a
    // quarter, a half and three quarters about the fixed centre.
    let o = solved.points[&p(1)];
    assert!(o[2].abs() <= TOL, "p1 at {o:?}");
    assert!(o[0].hypot(o[1]) > 1.0, "p1 at {o:?}");
    for (id, at) in [(3, [-o[1], o[0]]), (4, [-o[0], -o[1]]), (5, [o[1], -o[0]])] {
        let q = solved.points[&p(id)];
        assert!(
            (q[0] - at[0]).abs() <= TOL && (q[1] - at[1]).abs() <= TOL && q[2].abs() <= TOL,
            "p{id} at {q:?}"
        );
    }
    let flat = planar_from_spatial(&def, &output, &BTreeMap::new()).unwrap();
    assert_eq!(flat.def.patterns, planar.patterns);
}

/// mitcad#1833: a point attached to the start of a clockwise arc (seen
/// along the sketch's normal), and one to its end, stay where they were
/// after Convert to 2D Sketch: the projected arc runs counter-clockwise,
/// from the source's end to its start, so each point is tied to the
/// projected end nearest its target, not to the end of the same label.
#[test]
fn a_point_attached_to_a_clockwise_arcs_end_stays_there() {
    // About -z from (10, 0) to (0, -10): clockwise seen from +z.
    let arc = arc_curve([0.0, 0.0, -1.0], 0.0, FRAC_PI_2);
    let source = |at| SpatialSource::Sketch {
        entity: SketchEntityRef {
            sketch: FeatureUid(2),
            entity: Ref::Curve(p(20)),
        },
        at: Some(at),
        link: None,
    };
    let mut def = SpatialSketchDef::default();
    for (id, at) in [(1, [10.0, 0.0, 0.0]), (2, [0.0, -10.0, 0.0])] {
        def.points.push(SpatialPoint { id: p(id), at });
    }
    // Solved free, then attached (the evaluation's solve held them at
    // their targets).
    let mut output = output(&def, &Parameters::default());
    let mut attached = BTreeMap::new();
    for (id, point, at, target) in [
        (1, 1, SpatialCurvePoint::Start, [10.0, 0.0, 0.0]),
        (2, 2, SpatialCurvePoint::End, [0.0, -10.0, 0.0]),
    ] {
        def.constraints.push(SpatialConstraintDef {
            id: k(id),
            kind: SpatialConstraintKind::Attach {
                point: p(point),
                source: source(at),
                projected: true,
            },
        });
        output.targets.insert(k(id), target);
        attached.insert(k(id), arc.clone());
    }
    let flat = planar_from_spatial(&def, &output, &attached).unwrap();
    for (id, point, at) in [(1, 1, [10.0, 0.0]), (2, 2, [0.0, -10.0])] {
        let constraint = flat.def.constraints.iter().find(|c| c.id == k(id)).unwrap();
        let ConstraintKind::Coincident {
            point: held,
            entity: Ref::Point(end),
        } = constraint.kind
        else {
            panic!("k{id}: {:?}", constraint.kind);
        };
        assert_eq!(held, p(point));
        let end_at = point_at(&flat.def, end);
        assert!(
            (end_at[0] - at[0]).abs() <= TOL && (end_at[1] - at[1]).abs() <= TOL,
            "k{id} ties p{point} to {end_at:?}"
        );
    }
    // The 2D sketch solves with the points where they were.
    let solved = solve_planar(&flat.def, &Parameters::default());
    for (id, at) in [(1, [10.0, 0.0]), (2, [0.0, -10.0])] {
        let q = solved.points[&p(id)];
        assert!(
            (q[0] - at[0]).abs() <= TOL && (q[1] - at[1]).abs() <= TOL,
            "p{id} at {q:?}"
        );
    }
}

/// mitcad#1833: a linked projection of an arc, not construction, that
/// bounds a half disc with a line comes back from a 3D sketch as it was:
/// not construction (the 3D sketch stores it as a construction reference
/// curve, its own flag in the planar records), and the region is there.
#[test]
fn a_projected_arc_bounding_a_profile_converts_back_as_profile_geometry() {
    let mut planar = SketchDef::<ParamId>::default();
    for (id, at) in [(3, [0.0, 0.0]), (4, [10.0, 0.0]), (5, [-10.0, 0.0])] {
        let mut e = Entity::point(p(id), at);
        e.fixed = true;
        e.reference = true;
        planar.entities.push(e);
    }
    let mut arc = Entity::new(
        p(9),
        crate::EntityKind::Arc {
            center: p(3),
            start: p(4),
            end: p(5),
        },
    );
    arc.fixed = true;
    arc.reference = true;
    planar.entities.push(arc);
    planar.entities.push(Entity::new(
        p(8),
        crate::EntityKind::Line {
            start: p(5),
            end: p(4),
            centerline: false,
        },
    ));
    planar.projections.push(Projection {
        source: ProjectionSource::Sketch(SketchEntityRef {
            sketch: FeatureUid(2),
            entity: Ref::Curve(p(19)),
        }),
        body: None,
        entities: vec![
            Ref::Point(p(3)),
            Ref::Point(p(4)),
            Ref::Point(p(5)),
            Ref::Curve(p(9)),
        ],
        link: None,
        by_geometry: true,
        section: false,
        chain: Vec::new(),
        after: None,
        outline: false,
    });
    let params = Parameters::default();
    let before = planar.regions_of(&solve_planar(&planar, &params), &[]);
    assert_eq!(before.len(), 1, "{before:?}");

    let converted = spatial_from_planar(&planar, None).unwrap();
    assert_eq!(converted.references, [p(9)]);
    let def = converted.def;
    // Stored as a construction reference curve in the 3D sketch.
    assert!(def.curve(p(9)).unwrap().construction);
    // The arc's ends, which the line uses, are attached to the source's
    // ends: the evaluation gives each attachment its target, the end where
    // it was in the plane.
    let targets: BTreeMap<ConstraintUid, [f64; 3]> = converted
        .attachments
        .iter()
        .map(|id| {
            let Some(SpatialConstraintKind::Attach { point, .. }) = def
                .constraints
                .iter()
                .find(|c| c.id == *id)
                .map(|c| &c.kind)
            else {
                panic!("{id} is not an attachment");
            };
            let at = point_at(&planar, *point);
            (*id, [at[0], at[1], 0.0])
        })
        .collect();
    assert_eq!(targets.len(), 2, "{targets:?}");
    // The source curve, the same arc, for the reference curve and for each
    // attachment to its ends.
    let source = arc_curve([0.0, 0.0, 1.0], 0.0, PI);
    let attached: BTreeMap<ConstraintUid, SpatialReferenceCurve> =
        targets.keys().map(|id| (*id, source.clone())).collect();
    let output = output_with(
        &def,
        &params,
        SpatialInputs {
            curves: BTreeMap::from([(p(9), source)]),
            targets,
            ..SpatialInputs::default()
        },
    );
    let flat = planar_from_spatial(&def, &output, &attached).unwrap();
    let back = flat.def.entity(p(9)).unwrap();
    assert!(back.reference && !back.construction, "{back:?}");
    let regions = flat.def.regions_of(&solve_planar(&flat.def, &params), &[]);
    assert_eq!(regions.len(), 1, "{regions:?}");
    let key = regions[0].profile.key.to_string();
    for curve in ["c8", "c9"] {
        assert!(key.contains(curve), "{key}");
    }
}
