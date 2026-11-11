// SPDX-License-Identifier: MIT
//! A planar sketch turned into a 3D sketch (mitcad#1544): ids, kinds and
//! relations kept, the plane groups, the freedom kept, projections kept
//! linked, and what 3D sketches have no own form for kept in the planar
//! records (mitcad#1550).

use std::collections::BTreeMap;

use crate::def::SketchDef;
use crate::expr::Unit;
use crate::ids::{EntityUid, FeatureUid};
use crate::parameters::{ParamId, Parameters};
use crate::spatial::*;
use crate::spatial_convert::{spatial_from_planar, swap_ends};
use crate::spatial_edit::SpatialEdit;
use crate::{
    Constraint, ConstraintKind, ConstraintUid, Dimension, DimensionKind, Entity, EntityKind,
    Projection, ProjectionSource, Ref, SketchEntityRef,
};

fn p(n: u32) -> EntityUid {
    EntityUid(n)
}

fn k(n: u32) -> ConstraintUid {
    ConstraintUid(n)
}

fn line(id: u32, start: u32, end: u32) -> Entity {
    Entity::new(
        p(id),
        EntityKind::Line {
            start: p(start),
            end: p(end),
            centerline: false,
        },
    )
}

fn solve(def: &SpatialSketchDef, params: &Parameters) -> SpatialSolved {
    let mut value = |id| params.value(id).ok_or_else(|| "no value".to_owned());
    def.build(&mut value, &BTreeMap::new())
        .and_then(|mut model| model.solve())
        .unwrap()
}

fn dimension(params: &mut Parameters, value: f64) -> ParamId {
    params
        .add_dimension(&value.to_string(), Unit::MM, "", Some(FeatureUid(3)))
        .unwrap()
}

/// A rectangle 40 x 30 with its first corner fixed, its sides horizontal
/// and vertical and two lengths: points p1-p4, lines c5-c8, k1-k6.
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
        def.entities.push(line(id, a, b));
    }
    for (id, kind) in [
        (1, ConstraintKind::Horizontal { line: p(5) }),
        (2, ConstraintKind::Horizontal { line: p(7) }),
        (3, ConstraintKind::Vertical { line: p(6) }),
        (4, ConstraintKind::Vertical { line: p(8) }),
    ] {
        def.constraints.push(Constraint::new(k(id), kind));
    }
    def.dimensions.push(Dimension::new(
        k(5),
        DimensionKind::Length { line: p(5) },
        Some(dimension(params, 40.0)),
    ));
    def.dimensions.push(Dimension::new(
        k(6),
        DimensionKind::Length { line: p(6) },
        Some(dimension(params, 30.0)),
    ));
    def
}

/// W15: a fully constrained rectangle keeps its ids, relations and
/// dimensions as in-plane ones, its freedom (none) and no redundancy; a
/// corner released from the plane frees its elevation only.
#[test]
fn a_fully_constrained_rectangle_stays_fully_constrained() {
    let mut params = Parameters::default();
    let planar = rectangle(&mut params);
    let converted = spatial_from_planar(&planar, None).unwrap();
    let def = &converted.def;
    let ids: Vec<EntityUid> = def.points.iter().map(|p| p.id).collect();
    assert_eq!(ids, [p(1), p(2), p(3), p(4)]);
    assert_eq!(def.points[2].at, [40.0, 30.0, 0.0]);
    let curves: Vec<(EntityUid, &str)> = def.curves.iter().map(|c| (c.id, c.type_name())).collect();
    assert_eq!(
        curves,
        [
            (p(5), "line"),
            (p(6), "line"),
            (p(7), "line"),
            (p(8), "line")
        ]
    );
    assert_eq!(converted.in_plane, [k(1), k(2), k(3), k(4), k(5), k(6)]);
    assert!(converted.coincident.is_empty());
    // The fixed corner's in-plane fix and the plane relation come after
    // the planar ids.
    assert_eq!(converted.fixes, [k(7)]);
    assert_eq!(
        converted.on_sketch_plane,
        Some((k(8), vec![p(1), p(2), p(3), p(4)]))
    );
    assert!(def.constraint(k(1)).unwrap().kind.is_in_plane());
    assert!(matches!(
        def.dimension(k(5)).unwrap().kind,
        SpatialDimensionKind::InPlane { .. }
    ));
    // The parameters stay the planar dimensions'.
    assert_eq!(
        def.dimension(k(5)).unwrap().value,
        planar.dimensions[0].value
    );
    assert_eq!(def.dimension(k(5)).unwrap().slot(), "dimensions[k5].length");
    let solved = solve(def, &params);
    assert_eq!(solved.status.dof, 0, "{:?}", solved.status);
    assert!(solved.status.redundant.is_empty(), "{:?}", solved.status);
    let mut e = SpatialEdit::new(def.clone(), &mut params, FeatureUid(3), "Sketch1");
    e.release_from_plane(&[p(3)]).unwrap();
    let (released, _, _) = e.finish().unwrap();
    let solved = solve(&released, &params);
    assert_eq!(solved.status.dof, 1);
}

/// Fixture 15: an arc whose start is coincident with a line's end needs
/// one plane row for the arc's points and the line's end, one for the
/// line's other end; nothing is redundant. A centre line becomes a
/// construction line; the arc is bound to the sketch plane.
#[test]
fn an_arc_with_a_coincident_line_end_adds_no_redundancy() {
    let mut params = Parameters::default();
    let mut planar = SketchDef::<ParamId>::default();
    for (id, at) in [
        (1, [0.0, 0.0]),
        (2, [10.0, 0.0]),
        (3, [0.0, 10.0]),
        (4, [10.0, 0.0]),
        (5, [30.0, 0.0]),
    ] {
        planar.entities.push(Entity::point(p(id), at));
    }
    planar.entities.push(Entity::new(
        p(6),
        EntityKind::Arc {
            center: p(1),
            start: p(2),
            end: p(3),
        },
    ));
    let mut centre_line = line(7, 4, 5);
    centre_line.kind = EntityKind::Line {
        start: p(4),
        end: p(5),
        centerline: true,
    };
    planar.entities.push(centre_line);
    planar.constraints.push(Constraint::new(
        k(1),
        ConstraintKind::Coincident {
            point: p(2),
            entity: Ref::Point(p(4)),
        },
    ));
    planar.dimensions.push(Dimension::new(
        k(2),
        DimensionKind::Radius { curve: p(6) },
        Some(dimension(&mut params, 10.0)),
    ));
    let converted = spatial_from_planar(&planar, None).unwrap();
    let def = &converted.def;
    assert_eq!(converted.coincident, [k(1)]);
    assert!(matches!(
        def.constraint(k(1)).unwrap().kind,
        SpatialConstraintKind::Coincident { .. }
    ));
    let arc = def.curve(p(6)).unwrap();
    assert!(arc.is_plane_bound());
    assert!(def.curve(p(7)).unwrap().construction);
    assert_eq!(converted.on_sketch_plane, Some((k(3), vec![p(1), p(5)])));
    let solved = solve(def, &params);
    assert!(solved.status.redundant.is_empty(), "{:?}", solved.status);
    let planar_dof = 10 - 2 - 1 - 1; // points, coincidence, arc, radius
    assert_eq!(solved.status.dof, planar_dof, "{:?}", solved.status);
}

/// The definition's own frame, texts, construction flags and driven
/// dimensions are kept; the slot of an in-plane angle is an angle's.
#[test]
fn frames_texts_and_driven_dimensions_are_kept() {
    let mut planar = SketchDef::<ParamId> {
        frame: Some(Box::new(crate::def::FrameDef {
            origin: [0.0; 3],
            x_axis: [0.0, 1.0, 0.0],
            y_axis: [-1.0, 0.0, 0.0],
        })),
        ..Default::default()
    };
    for (id, at) in [(1, [0.0, 0.0]), (2, [10.0, 0.0]), (3, [0.0, 10.0])] {
        planar.entities.push(Entity::point(p(id), at));
    }
    planar.entities.push(line(4, 1, 2));
    let mut other = line(5, 1, 3);
    other.construction = true;
    planar.entities.push(other);
    planar
        .texts
        .push(crate::SketchText::new(p(6), "A", [1.0, 2.0], 5.0));
    planar.dimensions.push(Dimension::new(
        k(1),
        DimensionKind::Angle {
            a: p(4),
            b: p(5),
            reverse_a: None,
            reverse_b: None,
        },
        None,
    ));
    let converted = spatial_from_planar(&planar, None).unwrap();
    let def = &converted.def;
    assert_eq!(def.frame, planar.frame);
    assert!(def.curve(p(5)).unwrap().construction);
    assert_eq!(def.texts.len(), 1);
    assert_eq!(def.texts[0].planar.id, p(6));
    let angle = def.dimension(k(1)).unwrap();
    assert!(angle.is_driven());
    assert_eq!(angle.slot(), "dimensions[k1].angle");
}

/// A linked projection of another sketch's line stays a construction line
/// whose ends are attached to the source line's start and end (projected),
/// and whose points take no plane row; a projection of another sketch's
/// arc becomes a reference curve, a point on it a point on the reference
/// curve, and the arc's unused points go.
#[test]
fn linked_projections_keep_following_their_sources() {
    let source = |entity: Ref| SketchEntityRef {
        sketch: FeatureUid(2),
        entity,
    };
    let mut planar = SketchDef::<ParamId>::default();
    for (id, at) in [
        (1, [0.0, 0.0]),
        (2, [20.0, 0.0]),
        (3, [50.0, 0.0]),
        (4, [60.0, 0.0]),
        (5, [50.0, 10.0]),
        (6, [40.0, 30.0]),
        (7, [0.0, 30.0]),
    ] {
        planar.entities.push(Entity::point(p(id), at));
    }
    let mut projected = line(8, 1, 2);
    projected.fixed = true;
    projected.reference = true;
    let mut arc = Entity::new(
        p(9),
        EntityKind::Arc {
            center: p(3),
            start: p(4),
            end: p(5),
        },
    );
    arc.fixed = true;
    arc.reference = true;
    for e in planar.entities.iter_mut().take(5) {
        e.fixed = true;
        e.reference = true;
    }
    planar.entities.push(projected);
    planar.entities.push(arc);
    planar.entities.push(line(10, 2, 6));
    planar.entities.push(line(11, 6, 7));
    planar.constraints.push(Constraint::new(
        k(1),
        ConstraintKind::Coincident {
            point: p(6),
            entity: Ref::Curve(p(9)),
        },
    ));
    planar.constraints.push(Constraint::new(
        k(2),
        ConstraintKind::Parallel { a: p(8), b: p(11) },
    ));
    for (curve, points) in [(8, vec![1, 2]), (9, vec![3, 4, 5])] {
        let mut entities: Vec<Ref> = points.into_iter().map(|n| Ref::Point(p(n))).collect();
        entities.push(Ref::Curve(p(curve)));
        planar.projections.push(Projection {
            source: ProjectionSource::Sketch(source(Ref::Curve(p(curve + 10)))),
            body: None,
            entities,
            link: None,
            by_geometry: true,
            section: false,
            chain: Vec::new(),
            after: None,
            outline: false,
        });
    }
    let converted = spatial_from_planar(&planar, None).unwrap();
    let def = &converted.def;
    // The projected line: a construction line with attached ends.
    let line8 = def.curve(p(8)).unwrap();
    assert_eq!(line8.type_name(), "line");
    assert!(line8.construction);
    assert_eq!(converted.attachments.len(), 2);
    assert_eq!(converted.ends.len(), 1);
    let ends: Vec<_> = def
        .attachment_records()
        .map(|(_, point, source, projected)| (point, source.clone(), projected))
        .collect();
    assert_eq!(
        ends,
        [
            (
                p(1),
                SpatialSource::Sketch {
                    entity: source(Ref::Curve(p(18))),
                    at: Some(SpatialCurvePoint::Start),
                    link: None,
                },
                true
            ),
            (
                p(2),
                SpatialSource::Sketch {
                    entity: source(Ref::Curve(p(18))),
                    at: Some(SpatialCurvePoint::End),
                    link: None,
                },
                true
            ),
        ]
    );
    // The parallel to it stays in plane.
    assert!(def.constraint(k(2)).unwrap().kind.is_in_plane());
    // The projected arc: a reference curve; its unused points go.
    assert_eq!(converted.references, [p(9)]);
    assert!(def.curve(p(9)).unwrap().is_reference());
    assert_eq!(converted.dropped, [p(3), p(4), p(5)]);
    assert!(def.point(p(3)).is_none());
    assert!(matches!(
        def.constraint(k(1)).unwrap().kind,
        SpatialConstraintKind::PointOn { .. }
    ));
    // No in-plane fix of what the attachments hold; plane rows for the
    // groups that nothing else holds: p7 alone (p6 is on the reference
    // curve, p1 and p2 attached).
    assert!(converted.fixes.is_empty(), "{:?}", converted.fixes);
    let (_, kept) = converted.on_sketch_plane.clone().unwrap();
    assert_eq!(kept, [p(7)]);
    // A projection that runs the other way along its source.
    let pair = converted.ends[0];
    let mut swapped = def.clone();
    swap_ends(&mut swapped, pair);
    let first = swapped.constraint(pair[0]).unwrap();
    assert!(matches!(
        &first.kind,
        SpatialConstraintKind::Attach {
            source: SpatialSource::Sketch {
                at: Some(SpatialCurvePoint::End),
                ..
            },
            ..
        }
    ));
}

/// mitcad#1550: nothing is refused. An inactive constraint is kept in the
/// plane, holding nothing (the rectangle's bottom side may turn: one degree
/// of freedom); a relation to a projected curve other than a line and a
/// face's edges keep their projections as planar records, whose entities
/// stay geometry held in the plane.
#[test]
fn what_a_3d_sketch_has_no_form_for_is_kept() {
    let mut params = Parameters::default();
    let mut planar = rectangle(&mut params);
    planar.constraints[0].active = false;
    planar.constraints[0].inactive_reason = Some("not met".to_owned());
    let converted = spatial_from_planar(&planar, None).unwrap();
    let def = &converted.def;
    assert_eq!(
        def.planar.inactive,
        [crate::spatial_planar::InactiveRelation {
            id: k(1),
            reason: Some("not met".to_owned()),
        }]
    );
    assert!(matches!(
        def.constraint(k(1)).unwrap().kind,
        SpatialConstraintKind::InPlane { .. }
    ));
    assert_eq!(converted.kept, ["k1 (inactive)"]);
    assert_eq!(solve(def, &params).status.dof, 1);

    // A relation other than a point on it, to a projected arc.
    let mut planar = SketchDef::<ParamId>::default();
    for (id, at) in [(1, [0.0, 0.0]), (2, [10.0, 0.0]), (3, [0.0, 10.0])] {
        let mut point = Entity::point(p(id), at);
        point.fixed = true;
        point.reference = true;
        planar.entities.push(point);
    }
    let mut projected = Entity::new(
        p(4),
        EntityKind::Circle {
            center: p(1),
            radius: 10.0,
            start: 0.0,
        },
    );
    projected.fixed = true;
    projected.reference = true;
    planar.entities.push(projected);
    planar.entities.push(Entity::point(p(5), [30.0, 0.0]));
    planar.entities.push(Entity::new(
        p(6),
        EntityKind::Circle {
            center: p(5),
            radius: 5.0,
            start: 0.0,
        },
    ));
    planar.constraints.push(Constraint::new(
        k(1),
        ConstraintKind::Equal { a: p(4), b: p(6) },
    ));
    planar.projections.push(Projection {
        source: ProjectionSource::Sketch(SketchEntityRef {
            sketch: FeatureUid(2),
            entity: Ref::Curve(p(14)),
        }),
        body: None,
        entities: vec![Ref::Point(p(1)), Ref::Curve(p(4))],
        link: None,
        by_geometry: true,
        section: false,
        chain: Vec::new(),
        after: None,
        outline: false,
    });
    let converted = spatial_from_planar(&planar, None).unwrap();
    let def = &converted.def;
    assert_eq!(def.planar.projections, planar.projections);
    assert!(converted.references.is_empty());
    assert_eq!(def.curve(p(4)).unwrap().type_name(), "circle");
    assert!(def.constraints.iter().any(|c| matches!(
        c.kind,
        SpatialConstraintKind::InPlaneFixed {
            entity: Ref::Curve(id)
        } if id == p(4)
    )));
    assert_eq!(
        def.planar.references,
        [
            Ref::Point(p(1)),
            Ref::Point(p(2)),
            Ref::Point(p(3)),
            Ref::Curve(p(4))
        ]
    );
    // The circle equal to the projected one takes its radius.
    let solved = solve(def, &params);
    match solved.curves.get(&p(6)) {
        Some(SolvedCurve::Round { radius, .. }) => assert!((radius - 10.0).abs() <= 1e-6),
        other => panic!("{other:?}"),
    }

    // A face's edges: the projection kept as it is.
    planar.constraints.clear();
    planar.projections[0].source = ProjectionSource::Topo(crate::topo::TopoName::Face(
        "F2:end(r{c1})".parse().unwrap(),
    ));
    let converted = spatial_from_planar(&planar, None).unwrap();
    assert_eq!(converted.def.planar.projections, planar.projections);
    assert_eq!(converted.kept.len(), 1, "{:?}", converted.kept);
}
