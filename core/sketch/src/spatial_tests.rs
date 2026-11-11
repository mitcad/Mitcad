// SPDX-License-Identifier: MIT
//! 3D sketches (mitcad#1521): the definition's file form, the adapter to
//! the spatial solver (ids, statuses and failures mapped back), the frame
//! and the edit operations, without the document; arcs, splines,
//! reference curves, plane membership and in-plane relations (the solver
//! study's fixtures 9 and 12-14).

use std::collections::BTreeMap;

use crate::def::SketchPlane;
use crate::ids::{EntityUid, FeatureUid};
use crate::parameters::Parameters;
use crate::profile::SketchFrame;
use crate::spatial::*;
use crate::spatial_edit::*;
use crate::values::ValueInput;
use crate::{ConstraintKind, ConstraintUid, DimensionKind, Ref};

use SpatialAxis::{X, Y, Z};
use SpatialPointInput::{At, Existing};
use SpatialSense::{Negative, Positive};

const TOL: f64 = 1e-6;

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= TOL)
}

/// Solves `def` with the parameters' values; attachments to the origin
/// only.
fn solve(def: &SpatialSketchDef, params: &Parameters) -> Result<SpatialSolved, SpatialFailure> {
    let mut value = |id| params.value(id).ok_or_else(|| "no value".to_owned());
    def.build(&mut value, &BTreeMap::new())?.solve()
}

fn edit<'a>(def: SpatialSketchDef, params: &'a mut Parameters) -> SpatialEdit<'a> {
    SpatialEdit::new(def, params, FeatureUid(1), "3DSketch1")
}

/// An edge of an earlier body, as a reference curve's source.
fn an_edge() -> SpatialSource {
    SpatialSource::Edge {
        body: crate::ids::BodyUid::new(FeatureUid(2), 0),
        edge: Box::new("E{F2:a|F2:b}".parse().unwrap()),
        at: None,
        link: None,
    }
}

/// The study's route: A at the origin, then 40 along +X, 30 along +Y and
/// 20 along +Z, each segment continuing from the last one's end point.
struct Route {
    points: [EntityUid; 4],
    lines: [EntityUid; 3],
    lengths: [ConstraintUid; 3],
}

fn segment(
    start: SpatialPointInput,
    toward: [f64; 3],
    axis: SpatialAxis,
    length: f64,
) -> SegmentRequest {
    SegmentRequest {
        axis: Some((axis, Positive)),
        length: Some(ValueInput::Number(length)),
        ..SegmentRequest::new(start, At(toward))
    }
}

fn route(edit: &mut SpatialEdit<'_>) -> Route {
    let first = edit
        .add_segment(&SegmentRequest {
            start_source: Some(SpatialSource::Origin),
            ..segment(At([0.0; 3]), [38.0, 1.0, 0.5], X, 40.0)
        })
        .unwrap();
    let second = edit
        .add_segment(&segment(Existing(first.end), [41.0, 28.0, 0.0], Y, 30.0))
        .unwrap();
    let third = edit
        .add_segment(&segment(Existing(second.end), [40.0, 31.0, 18.0], Z, 20.0))
        .unwrap();
    assert_eq!(
        second.start, first.end,
        "a continued route shares its point"
    );
    assert_eq!(third.start, second.end);
    Route {
        points: [first.start, first.end, second.end, third.end],
        lines: [first.line, second.line, third.line],
        lengths: [
            first.dimensions[0],
            second.dimensions[0],
            third.dimensions[0],
        ],
    }
}

#[test]
fn a_definition_is_written_flat_and_read_back() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::new(SketchPlane::Xz), &mut params);
    let r = route(&mut e);
    e.set_construction(r.lines[2], true).unwrap();
    let (def, _, _) = e.finish().unwrap();
    let names = params.clone();
    let written: SpatialSketchDef<String> = def
        .map_params(&mut |_: &str, id: &crate::parameters::ParamId| Ok::<_, ()>(names.name(*id)))
        .unwrap();
    let json = serde_json::to_value(&written).unwrap();
    assert_eq!(json["support"], "xz");
    assert_eq!(json["points"][0]["id"], "p1");
    assert_eq!(json["curves"][0]["id"], "c3");
    assert_eq!(json["curves"][0]["type"], "line");
    assert_eq!(json["curves"][0]["start"], "p1");
    assert_eq!(json["curves"][2]["construction"], true);
    assert_eq!(json["constraints"][0]["type"], "attach");
    assert_eq!(json["constraints"][0]["source"], "origin");
    assert_eq!(json["constraints"][1]["type"], "align");
    assert_eq!(json["constraints"][1]["sense"], "positive");
    assert_eq!(json["dimensions"][0]["type"], "length");
    assert_eq!(json["dimensions"][0]["line"], "c3");
    assert!(json["dimensions"][0]["value"].is_string(), "{json}");
    let read: SpatialSketchDef<String> = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(read, written);
    // A vertex source and a driven dimension read back too.
    let mut more = json;
    more["constraints"][0]["source"] =
        serde_json::json!({"body": "F2.b0", "vertex": "V{F2:a|F2:b|F2:c}"});
    more["dimensions"][1]
        .as_object_mut()
        .unwrap()
        .remove("value");
    let read: SpatialSketchDef<String> = serde_json::from_value(more).unwrap();
    assert!(matches!(
        read.constraints[0].kind,
        SpatialConstraintKind::Attach {
            source: SpatialSource::Vertex { .. },
            ..
        }
    ));
    assert!(read.dimensions[1].is_driven());
    // Unknown fields are refused.
    for bad in [
        serde_json::json!({"support": "xy", "regions": []}),
        serde_json::json!({"constraints": [{"id": "k1", "type": "fix", "point": "p1",
                                             "at": [0, 0, 0], "z": 1}]}),
        serde_json::json!({"constraints": [{"id": "k1", "type": "attach", "point": "p1",
                                             "source": "nowhere"}]}),
        serde_json::json!({"curves": [{"id": "c1", "type": "line", "start": "p1",
                                        "end": "p2", "radius": 3}]}),
        serde_json::json!({"curves": [{"id": "p1", "type": "line", "start": "p1",
                                        "end": "p2"}]}),
    ] {
        assert!(serde_json::from_value::<SpatialSketchDef<String>>(bad).is_err());
    }
}

/// W1 at the adapter: four shared points, three lines, nothing free.
#[test]
fn the_reference_route_solves_fully_constrained() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let r = route(&mut e);
    let (def, report, goals) = e.finish().unwrap();
    assert!(goals.is_empty());
    assert_eq!(def.points.len(), 4);
    assert_eq!(def.curves.len(), 3);
    assert_eq!(report.parameters.len(), 3, "{:?}", report.parameters);
    let solved = solve(&def, &params).unwrap();
    let expected = [
        [0.0, 0.0, 0.0],
        [40.0, 0.0, 0.0],
        [40.0, 30.0, 0.0],
        [40.0, 30.0, 20.0],
    ];
    for (p, at) in r.points.iter().zip(expected) {
        assert!(near(solved.points[p], at), "{p:?}: {:?}", solved.points[p]);
    }
    assert_eq!(solved.status.dof, 0);
    assert!(solved.status.fully_constrained);
    assert_eq!(solved.status.determined_points, r.points.to_vec());
    assert_eq!(solved.status.determined_lines, r.lines.to_vec());
    assert!(solved.status.redundant.is_empty());
    assert!((solved.status.measured[&r.lengths[1]] - 30.0).abs() <= TOL);
}

/// W2 and W3 at the adapter: BC at 50 moves C and D; without its length
/// C and D share one free translation along Y.
#[test]
fn a_length_change_moves_the_route_and_its_removal_frees_one_motion() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let r = route(&mut e);
    let (def, _, _) = e.finish().unwrap();
    let bc = def.dimension(r.lengths[1]).unwrap().value.unwrap();
    params.set_value(bc, 50.0).unwrap();
    let solved = solve(&def, &params).unwrap();
    assert!(near(solved.points[&r.points[2]], [40.0, 50.0, 0.0]));
    assert!(near(solved.points[&r.points[3]], [40.0, 50.0, 20.0]));
    assert!(solved.status.fully_constrained);

    let mut e = edit(def, &mut params);
    e.remove_relation(r.lengths[1]).unwrap();
    let (def, report, _) = e.finish().unwrap();
    assert_eq!(report.removed, [r.lengths[1].to_string()]);
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.dof, 1);
    assert!(!solved.status.fully_constrained);
    assert_eq!(solved.status.determined_points, r.points[..2].to_vec());
    // The free motion keeps C and D on their lines.
    let c = solved.points[&r.points[2]];
    let d = solved.points[&r.points[3]];
    assert!((c[0] - 40.0).abs() <= TOL && c[2].abs() <= TOL);
    assert!(near(d, [40.0, c[1], 20.0]));
}

/// W5 at the adapter: a duplicate length is redundant and kept; a
/// different one contradicts, naming both; a segment without length is
/// refused before any solve.
#[test]
fn redundancy_conflicts_and_collapsed_lines_are_told_apart() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let r = route(&mut e);
    let twin = e
        .add_dimension(
            SpatialDimensionKind::Length { line: r.lines[0] },
            Some(&ValueInput::Number(40.0)),
            false,
        )
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.redundant, [twin]);
    assert_eq!(solved.status.dof, 0);

    let mut e = edit(def, &mut params);
    let other = e
        .add_dimension(
            SpatialDimensionKind::Length { line: r.lines[0] },
            Some(&ValueInput::Number(41.0)),
            false,
        )
        .unwrap();
    let (conflicting, _, _) = e.finish().unwrap();
    let failure = solve(&conflicting, &params).unwrap_err();
    assert_eq!(failure.kind, SpatialFailureKind::Conflict, "{failure}");
    assert!(failure.constraints.contains(&other), "{failure:?}");
    assert!(
        failure.constraints.contains(&r.lengths[0]) || failure.constraints.contains(&twin),
        "{failure:?}"
    );
    assert!(failure.message.contains(&other.to_string()), "{failure}");

    // Segments without length are refused, whether by their points or by
    // a zero length.
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let error = e
        .add_segment(&SegmentRequest::new(
            At([1.0, 2.0, 3.0]),
            At([1.0, 2.0, 3.0]),
        ))
        .unwrap_err();
    assert!(error.contains("no length"), "{error}");
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let error = e
        .add_segment(&segment(At([0.0; 3]), [5.0, 0.0, 0.0], X, 0.0))
        .unwrap_err();
    assert!(error.contains("no length"), "{error}");
    // A length set to zero or below is refused when the edit finishes.
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let made = e
        .add_segment(&segment(At([0.0; 3]), [5.0, 0.0, 0.0], X, 5.0))
        .unwrap();
    e.set_dimension_value(made.dimensions[0], &ValueInput::Number(-1.0))
        .unwrap();
    let error = e.finish().unwrap_err();
    assert!(error.contains("greater than"), "{error}");
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let error = e.add_point([f64::NAN, 0.0, 0.0]).unwrap_err();
    assert!(error.contains("not finite"), "{error}");
}

/// An aligned line held against its sense, and a free line pulled onto
/// one point, are invalid geometry, not conflicts.
#[test]
fn invalid_geometry_names_its_lines() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let ab = e
        .add_segment(&SegmentRequest {
            start_source: Some(SpatialSource::Origin),
            axis: Some((X, Negative)),
            ..SegmentRequest::new(At([0.0; 3]), At([-10.0, 0.0, 0.0]))
        })
        .unwrap();
    // The end held at +10 along X: against the negative sense.
    e.add_constraint(SpatialConstraintKind::Fix {
        point: ab.end,
        at: [10.0, 0.0, 0.0],
    })
    .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let failure = solve(&def, &params).unwrap_err();
    assert_eq!(
        failure.kind,
        SpatialFailureKind::InvalidGeometry,
        "{failure}"
    );
    assert!(failure.entities.contains(&ab.line), "{failure:?}");
}

/// Deleting a line keeps the points other lines and relations use; a
/// point in use is deleted only with what uses it.
#[test]
fn deleting_a_line_keeps_shared_points() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let r = route(&mut e);
    let (def, _, _) = e.finish().unwrap();

    // The middle line: B and C stay (AB and CD use them), its alignment
    // and length go.
    let mut e = edit(def.clone(), &mut params);
    e.delete_curve(r.lines[1]).unwrap();
    let (after, report, _) = e.finish().unwrap();
    assert!(after.curve(r.lines[1]).is_none());
    assert!(after.point(r.points[1]).is_some() && after.point(r.points[2]).is_some());
    assert!(after.dimension(r.lengths[1]).is_none());
    assert!(report.removed.contains(&format!("c{}", r.lines[1].0)));
    assert!(
        !report.removed.iter().any(|x| x.starts_with('p')),
        "{report:?}"
    );

    // The last line: D goes with it (nothing else uses it).
    let mut e = edit(def.clone(), &mut params);
    e.delete_curve(r.lines[2]).unwrap();
    let (after, _, _) = e.finish().unwrap();
    assert!(after.point(r.points[3]).is_none());
    assert!(after.point(r.points[2]).is_some());

    // B is used by two lines: deleting it alone is refused, naming them.
    let mut e = edit(def.clone(), &mut params);
    let error = e.delete_point(r.points[1], false).unwrap_err();
    assert!(error.contains(&format!("c{}", r.lines[0].0)), "{error}");
    assert!(error.contains(&format!("c{}", r.lines[1].0)), "{error}");
    let mut e = edit(def, &mut params);
    e.delete_point(r.points[1], true).unwrap();
    let (after, _, _) = e.finish().unwrap();
    assert!(after.point(r.points[1]).is_none());
    assert!(after.curves.iter().all(|c| c.id == r.lines[2]), "{after:?}");
    // A keeps its attachment to the origin; C and D their line.
    assert!(after.point(r.points[0]).is_some());
    assert!(after.point(r.points[2]).is_some() && after.point(r.points[3]).is_some());
}

/// Unfix removes a point's Fix only, Detach its attachment only; a
/// detached point stays where it was, free.
#[test]
fn unfix_and_detach_remove_only_their_own_relation() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let a = e.add_point([0.0; 3]).unwrap();
    let b = e.add_point([5.0, 6.0, 7.0]).unwrap();
    let attached = e.attach(a, SpatialSource::Origin).unwrap();
    let fixed = e.fix(b).unwrap();
    assert!(e.unfix(a).unwrap_err().contains("not fixed"));
    assert!(e.detach(b).unwrap_err().contains("not attached"));
    assert!(e.fix(b).unwrap_err().contains("fixed already"));
    assert!(e.attach(a, SpatialSource::Origin).is_err());
    let (def, _, _) = e.finish().unwrap();
    assert!(def.constraint(attached).is_some() && def.constraint(fixed).is_some());
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.dof, 0);

    let mut e = edit(def, &mut params);
    assert_eq!(e.unfix(b).unwrap(), [fixed]);
    assert_eq!(e.detach(a).unwrap(), [attached]);
    let (def, report, _) = e.finish().unwrap();
    assert!(def.constraints.is_empty());
    assert_eq!(report.removed.len(), 2);
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.dof, 6);
    assert!(near(solved.points[&b], [5.0, 6.0, 7.0]));
    assert!(near(solved.points[&a], [0.0; 3]));
}

/// The written forms of the linked sources (mitcad#1543) read back as they
/// were; a point source and a curve source are told apart, and the wrong
/// kind is refused for an attachment and for a reference curve.
#[test]
fn linked_sources_round_trip_and_keep_their_kind() {
    let forms = [
        (serde_json::json!("origin"), false),
        (
            serde_json::json!({"body": "F2.b0", "vertex": "V{F2:a|F2:b|F2:c}",
                               "link": {"source": "O1/O3", "target": "O2"}}),
            false,
        ),
        (
            serde_json::json!({"body": "F2.b0", "edge": "E{F2:a|F2:b}"}),
            true,
        ),
        (
            serde_json::json!({"body": "F2.b0", "edge": "E{F2:a|F2:b}", "at": "middle"}),
            false,
        ),
        (serde_json::json!({"sketch": "F3", "point": "p2"}), false),
        (serde_json::json!({"sketch": "F3", "curve": "c4"}), true),
        (
            serde_json::json!({"sketch": "F3", "curve": "c4", "at": "end",
                               "link": {"source": "O1"}}),
            false,
        ),
        (serde_json::json!({"feature": "F5"}), true),
        (serde_json::json!({"feature": "F5", "at": "start"}), false),
    ];
    for (form, curve) in forms {
        let source: SpatialSource = serde_json::from_value(form.clone()).unwrap();
        assert_eq!(serde_json::to_value(&source).unwrap(), form);
        assert_eq!(source.is_curve(), curve, "{form}");
    }
    for bad in [
        serde_json::json!({"body": "F2.b0", "vertex": "V{F2:a|F2:b|F2:c}", "at": "end"}),
        serde_json::json!({"sketch": "F3", "point": "p2", "at": "middle"}),
        serde_json::json!({"body": "F2.b0", "edge": "E{F2:a|F2:b}", "at": "side"}),
        serde_json::json!({"feature": "F5", "body": "F2.b0"}),
        serde_json::json!("nowhere"),
    ] {
        assert!(
            serde_json::from_value::<SpatialSource>(bad.clone()).is_err(),
            "{bad}"
        );
    }

    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let p = e.add_point([0.0; 3]).unwrap();
    let edge = an_edge();
    // A curve source puts the point on its reference curve, made once.
    let on = e.attach(p, edge.clone()).unwrap();
    let curve = e.include(edge.clone(), false).unwrap();
    assert_eq!(e.def.constraint(on).unwrap().kind.curves(), [curve]);
    assert!(e.attach(p, edge.clone()).is_err());
    assert!(e.include(SpatialSource::Origin, false).is_err());
    // A projected record of the same source is another record.
    assert_ne!(e.include(edge.clone(), true).unwrap(), curve);
    let (mut def, _, _) = e.finish().unwrap();
    let id = def.next_constraint();
    def.constraints.push(SpatialConstraintDef {
        id,
        kind: SpatialConstraintKind::Attach {
            point: p,
            source: edge,
            projected: false,
        },
    });
    assert!(def.check_definition().unwrap_err().contains("is a curve"));
}

/// A face source (mitcad#1549) reads back as it was and takes no `at`;
/// Attach puts a point on it with an On Face relation (once, never
/// projected), which the build refuses as a reference failure without
/// the face's surface and solves with it (the point keeps two degrees of
/// freedom); an attachment to a face and an On Face relation with a point
/// source are refused definitions.
#[test]
fn a_point_lies_on_a_face_through_its_surface() {
    let form = serde_json::json!({"body": "F2.b0", "face": "F2:top", "link": {"source": "O1"}});
    let source: SpatialSource = serde_json::from_value(form.clone()).unwrap();
    assert_eq!(serde_json::to_value(&source).unwrap(), form);
    assert!(source.is_face() && !source.is_curve());
    assert_eq!(source.kind_name(), "face");
    let bad = serde_json::json!({"body": "F2.b0", "face": "F2:top", "at": "middle"});
    assert!(serde_json::from_value::<SpatialSource>(bad).is_err());

    let face: SpatialSource =
        serde_json::from_value(serde_json::json!({"body": "F2.b0", "face": "F2:top"})).unwrap();
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let p = e.add_point([3.0, 4.0, 7.0]).unwrap();
    let on = e.attach(p, face.clone()).unwrap();
    assert!(matches!(
        &e.def.constraint(on).unwrap().kind,
        SpatialConstraintKind::OnFace { point, .. } if *point == p
    ));
    assert!(e.attach(p, face.clone()).unwrap_err().contains("already"));
    assert!(e.attach_as(p, face.clone(), true).is_err());
    assert!(e.include(face.clone(), false).is_err());
    let (def, _, _) = e.finish().unwrap();
    assert_eq!(def.face_records().count(), 1);

    let mut value = |id| params.value(id).ok_or_else(|| "no value".to_owned());
    let failure = def
        .build_with(&mut value, &SpatialInputs::default())
        .unwrap_err();
    assert_eq!(failure.kind, SpatialFailureKind::Reference);
    assert_eq!(failure.constraints, [on]);
    let mut inputs = SpatialInputs::default();
    inputs.surfaces.insert(
        on,
        SpatialSurface::Plane {
            origin: [0.0, 0.0, 5.0],
            normal: [0.0, 0.0, 1.0],
        },
    );
    let solved = def
        .build_with(&mut value, &inputs)
        .unwrap()
        .solve()
        .unwrap();
    assert!(near(solved.points[&p], [3.0, 4.0, 5.0]));
    assert_eq!(solved.status.dof, 2);

    let mut wrong = def.clone();
    wrong.constraints[0].kind = SpatialConstraintKind::Attach {
        point: p,
        source: face,
        projected: false,
    };
    assert!(wrong.check_definition().unwrap_err().contains("is a face"));
    wrong.constraints[0].kind = SpatialConstraintKind::OnFace {
        point: p,
        source: SpatialSource::Origin,
    };
    assert!(wrong.check_definition().unwrap_err().contains("is a point"));
}

/// A drag pulls a free point to its target and a constrained one as far
/// as its relations allow, adding no relation.
#[test]
fn a_drag_pulls_points_with_soft_goals() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let free = e.add_point([1.0, 1.0, 1.0]).unwrap();
    let ab = e
        .add_segment(&SegmentRequest {
            start_source: Some(SpatialSource::Origin),
            axis: Some((Z, Positive)),
            ..SegmentRequest::new(At([0.0; 3]), At([0.0, 0.0, 10.0]))
        })
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let mut value = |id| params.value(id).ok_or_else(|| "no value".to_owned());
    let mut model = def.build(&mut value, &BTreeMap::new()).unwrap();
    let solved = model
        .drag(&[(free, [4.0, -2.0, 9.0]), (ab.end, [3.0, 3.0, 25.0])])
        .unwrap();
    assert!(near(solved.points[&free], [4.0, -2.0, 9.0]));
    // Along +Z through the origin only: it goes up to 25.
    assert!(near(solved.points[&ab.end], [0.0, 0.0, 25.0]));
    assert!(def.constraints.len() == 2 && def.dimensions.is_empty());
}

/// The frame: unit axes at right angles, local coordinates mapped once
/// each way; the study's rotated frame (fixture 8).
#[test]
fn the_frame_maps_local_coordinates_once() {
    let skewed = SketchFrame {
        origin: [1.0, 2.0, 3.0],
        x_axis: [2.0, 0.0, 0.0],
        y_axis: [1.0, 3.0, 0.0],
    };
    let frame = orthonormal_frame(&skewed).unwrap();
    assert!(near(frame.x_axis, [1.0, 0.0, 0.0]) && near(frame.y_axis, [0.0, 1.0, 0.0]));
    assert!(
        orthonormal_frame(&SketchFrame {
            origin: [0.0; 3],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [2.0, 0.0, 0.0],
        })
        .is_err()
    );
    let turned = SketchFrame {
        origin: [100.0, 200.0, 300.0],
        x_axis: [0.0, 1.0, 0.0],
        y_axis: [-1.0, 0.0, 0.0],
    };
    for (local, component) in [
        ([0.0, 0.0, 0.0], [100.0, 200.0, 300.0]),
        ([40.0, 0.0, 0.0], [100.0, 240.0, 300.0]),
        ([40.0, 30.0, 0.0], [70.0, 240.0, 300.0]),
        ([40.0, 30.0, 20.0], [70.0, 240.0, 320.0]),
    ] {
        assert!(near(to_component(&turned, local), component));
        assert!(near(to_local(&turned, component), local));
    }
}

/// Ids come from the definition: an edit that is thrown away leaves the
/// next edit the same ids.
#[test]
fn ids_follow_the_definition() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let r = route(&mut e);
    let (def, _, _) = e.finish().unwrap();
    assert_eq!(def.next_entity(), EntityUid(r.lines[2].0 + 1));
    let first = {
        let mut e = edit(def.clone(), &mut params);
        e.add_point([9.0, 9.0, 9.0]).unwrap()
    };
    let mut e = edit(def, &mut params);
    assert_eq!(e.add_point([1.0, 1.0, 1.0]).unwrap(), first);
}

// Arcs, circles, splines, reference curves, plane membership and in-plane
// relations (mitcad#1521's scope decision of 8.11.2026, on mitcad#1542's
// solver).

fn points_at(e: &mut SpatialEdit<'_>, at: &[[f64; 3]]) -> Vec<EntityUid> {
    at.iter().map(|p| e.add_point(*p).unwrap()).collect()
}

/// Every curve kind, the new relations, plane membership, in-plane
/// relations and dimensions and a kept text are written flat and read
/// back.
#[test]
fn curves_and_their_relations_are_written_flat_and_read_back() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let arc = e
        .add_arc(&ArcRequest {
            normal_axis: Some((Z, Positive)),
            radius: Some(ValueInput::Number(10.0)),
            ..ArcRequest::new(
                At([10.0, 0.0, 0.0]),
                At([-10.0, 0.0, 0.0]),
                ArcPlacement::ThreePoint {
                    through: [0.0, 10.0, 0.0],
                },
            )
        })
        .unwrap();
    let circle = e
        .add_circle(
            At([50.0, 0.0, 0.0]),
            4.0,
            [0.0, 0.0, 1.0],
            Some(SpatialDirection::SKETCH_PLANE),
            false,
        )
        .unwrap();
    let fitted = e
        .add_spline(&SplineRequest {
            ends: [SpatialSplineEnd::Free, SpatialSplineEnd::Natural],
            ..SplineRequest::fit(vec![
                At([0.0, 20.0, 0.0]),
                At([10.0, 25.0, 3.0]),
                At([20.0, 20.0, 6.0]),
            ])
        })
        .unwrap();
    let control = e
        .add_spline(&SplineRequest::control(
            vec![
                At([0.0, 40.0, 0.0]),
                At([5.0, 45.0, 0.0]),
                At([10.0, 40.0, 0.0]),
                At([15.0, 45.0, 0.0]),
            ],
            3,
        ))
        .unwrap();
    let line = e
        .add_line(At([0.0, 60.0, 0.0]), At([10.0, 60.0, 0.0]), false)
        .unwrap();
    let p = e.add_point([0.0, 0.0, 0.0]).unwrap();
    e.point_on(p, fitted.spline).unwrap();
    e.equal(arc.arc, circle).unwrap();
    e.add_constraint(SpatialConstraintKind::InPlane {
        relation: ConstraintKind::Horizontal { line },
        normal: SpatialAxis::Z,
    })
    .unwrap();
    let length = e
        .add_dimension(
            SpatialDimensionKind::InPlane {
                dimension: DimensionKind::Length { line },
                normal: SpatialAxis::Z,
            },
            Some(&ValueInput::Number(10.0)),
            false,
        )
        .unwrap();
    e.add_dimension(
        SpatialDimensionKind::SplineLength {
            curve: control.spline,
        },
        None,
        true,
    )
    .unwrap();
    let kept = e.keep_on_sketch_plane(&control.points).unwrap();
    let text = e.def.next_entity();
    e.def.texts.push(
        serde_json::from_value(serde_json::json!({
            "planar": {"id": format!("t{}", text.0), "text": "A", "at": [1, 2], "height": 5},
            "z": 2.5
        }))
        .unwrap(),
    );
    let (def, _, _) = e.finish().unwrap();
    let names = params.clone();
    let written: SpatialSketchDef<String> = def
        .map_params(&mut |_: &str, id: &crate::parameters::ParamId| Ok::<_, ()>(names.name(*id)))
        .unwrap();
    let json = serde_json::to_value(&written).unwrap();
    let curve = |id: EntityUid| {
        json["curves"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == format!("c{}", id.0))
            .unwrap()
            .clone()
    };
    assert_eq!(curve(arc.arc)["type"], "arc");
    assert_eq!(curve(arc.arc)["center"], format!("p{}", arc.center.0));
    assert!(curve(arc.arc)["normal"][2].as_f64().unwrap() > 0.99);
    assert_eq!(curve(circle)["plane"]["axis"], "z");
    assert_eq!(curve(fitted.spline)["type"], "fitted_spline");
    assert_eq!(curve(fitted.spline)["ends"][0], "free");
    assert_eq!(curve(control.spline)["degree"], 3);
    assert!(curve(control.spline).get("weights").is_none());
    let kinds: Vec<&str> = json["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["type"].as_str().unwrap())
        .collect();
    for kind in [
        "normal_align",
        "point_on",
        "equal",
        "in_plane",
        "on_sketch_plane",
    ] {
        assert!(kinds.contains(&kind), "{kind}: {kinds:?}");
    }
    let dimension = json["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == length.to_string())
        .unwrap()
        .clone();
    assert_eq!(dimension["dimension"]["type"], "length");
    assert_eq!(json["texts"][0]["z"], 2.5);
    let read: SpatialSketchDef<String> = serde_json::from_value(json).unwrap();
    assert_eq!(read, written);
    assert!(def.constraint(kept).is_some());
    // A spline's length drives too (mitcad#1661).
    let mut e = edit(def, &mut params);
    assert!(
        e.add_dimension(
            SpatialDimensionKind::SplineLength {
                curve: fitted.spline
            },
            Some(&ValueInput::Number(30.0)),
            false,
        )
        .is_ok()
    );
}

/// Fixture 9: a free arc has 8 degrees of freedom, 2 with fixed ends; a
/// plane-bound one 6.
#[test]
fn an_arc_has_the_freedom_of_its_points_and_normal() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let p = points_at(
        &mut e,
        &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
    );
    let arc = e
        .add_curve(
            SpatialCurveKind::Arc {
                center: p[0],
                start: p[1],
                end: p[2],
                normal: [0.0, 0.0, 1.0],
                plane: None,
            },
            false,
        )
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.dof, 8);
    match &solved.curves[&arc] {
        SolvedCurve::Round { normal, radius } => {
            assert!(near(*normal, [0.0, 0.0, 1.0]) && (radius - 10.0).abs() <= TOL);
        }
        other => panic!("{other:?}"),
    }
    let mut e = edit(def.clone(), &mut params);
    e.fix(p[1]).unwrap();
    e.fix(p[2]).unwrap();
    let (fixed, _, _) = e.finish().unwrap();
    assert_eq!(solve(&fixed, &params).unwrap().status.dof, 2);
    let mut bound = def;
    if let SpatialCurveKind::Arc { plane, .. } = &mut bound.curves[0].kind {
        *plane = Some(SpatialDirection::SKETCH_PLANE);
    }
    assert_eq!(solve(&bound, &params).unwrap().status.dof, 6);
}

/// The arc tools place the new points on the requested arc: Three-Point
/// through its third point (collinear points refused), Centre with its
/// end projected onto the circle.
#[test]
fn the_arc_tools_start_on_their_arc() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let made = e
        .add_arc(&ArcRequest::new(
            At([10.0, 0.0, 0.0]),
            At([-10.0, 0.0, 0.0]),
            ArcPlacement::ThreePoint {
                through: [0.0, 10.0, 0.0],
            },
        ))
        .unwrap();
    assert!(near(e.position(made.center).unwrap(), [0.0; 3]));
    let error = e
        .add_arc(&ArcRequest::new(
            At([0.0, 0.0, 5.0]),
            At([20.0, 0.0, 5.0]),
            ArcPlacement::ThreePoint {
                through: [10.0, 0.0, 5.0],
            },
        ))
        .unwrap_err();
    assert!(error.contains("one line"), "{error}");
    let centre = e
        .add_arc(&ArcRequest::new(
            At([35.0, 0.0, 0.0]),
            At([30.0, 7.0, 3.0]),
            ArcPlacement::Centre {
                center: At([30.0, 0.0, 0.0]),
                normal: [0.0, 0.0, 2.0],
            },
        ))
        .unwrap();
    assert!(near(e.position(centre.end).unwrap(), [30.0, 5.0, 0.0]));
    let (def, _, _) = e.finish().unwrap();
    match &def.curve(made.arc).unwrap().kind {
        SpatialCurveKind::Arc { normal, .. } => assert!(near(*normal, [0.0, 0.0, 1.0])),
        other => panic!("{other:?}"),
    }
    let solved = solve(&def, &params).unwrap();
    // A half circle from +X to -X about +Z: its middle is at +Y.
    assert!(near(solved.points[&made.end], [-10.0, 0.0, 0.0]));
    // An arc whose ends meet is refused.
    let mut e = edit(def, &mut params);
    let error = e
        .add_arc(&ArcRequest::new(
            Existing(made.start),
            Existing(made.start),
            ArcPlacement::Centre {
                center: Existing(made.center),
                normal: [0.0, 0.0, 1.0],
            },
        ))
        .unwrap_err();
    assert!(error.contains("three points"), "{error}");
}

/// Fixture 12: four free fit points with natural ends leave 12 degrees of
/// freedom; a tangency to a fixed line at its first point drops that
/// end's three curvature rows and adds two: 13, the extra one the end
/// tangent's magnitude. The tangency's sense is taken from the geometry.
#[test]
fn a_fit_point_spline_and_a_tangent_end() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let line = e
        .add_line(At([-20.0, 0.0, 0.0]), At([-10.0, 0.0, 0.0]), false)
        .unwrap();
    let [a, b] = e.def.line(line).unwrap();
    e.fix(a).unwrap();
    e.fix(b).unwrap();
    let spline = e
        .add_spline(&SplineRequest::fit(vec![
            At([0.0, 0.0, 0.0]),
            At([10.0, 5.0, 0.0]),
            At([20.0, 0.0, 5.0]),
            At([30.0, 5.0, 5.0]),
        ]))
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    assert_eq!(solve(&def, &params).unwrap().status.dof, 12);
    let mut e = edit(def, &mut params);
    let tangent = e.tangent(line, spline.spline, spline.points[0]).unwrap();
    let (mut def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.dof, 13);
    assert_eq!(solved.tangent_senses[&tangent], Positive);
    let SolvedCurve::Spline(net) = &solved.curves[&spline.spline] else {
        panic!("{:?}", solved.curves[&spline.spline]);
    };
    assert_eq!(
        net.control_points.len(),
        6,
        "fit points and the hidden ones"
    );
    let leg = mitcad_vec3::sub(net.control_points[1], net.control_points[0]);
    assert!(
        leg[0] > 0.0 && leg[1].abs() <= TOL && leg[2].abs() <= TOL,
        "{leg:?}"
    );
    def.store_solved(&solved);
    assert!(matches!(
        def.constraint(tangent).unwrap().kind,
        SpatialConstraintKind::Tangent {
            sense: Some(Positive),
            ..
        }
    ));
}

/// Fixture 13: a point on a constant reference edge and an edge's
/// midpoint follow the edge when it changes; a reference without its
/// geometry is a reference failure. A model curve becomes local geometry
/// through the frame.
#[test]
fn points_on_a_reference_curve_follow_it() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let p = points_at(
        &mut e,
        &[[0.0, 0.0, 0.0], [0.5, 0.0, 14.0], [0.0, 0.3, 19.0]],
    );
    let edge = e
        .add_curve(
            SpatialCurveKind::Reference {
                source: an_edge(),
                projected: false,
            },
            false,
        )
        .unwrap();
    e.fix(p[0]).unwrap();
    e.point_on(p[1], edge).unwrap();
    e.add_dimension(
        SpatialDimensionKind::Offset {
            a: p[0],
            b: p[1],
            axis: Z,
        },
        Some(&ValueInput::Number(15.0)),
        false,
    )
    .unwrap();
    e.midpoint(p[2], edge).unwrap();
    let (def, _, _) = e.finish().unwrap();
    let mut value = |id| params.value(id).ok_or_else(|| "no value".to_owned());
    let failure = def
        .build_with(&mut value, &SpatialInputs::default())
        .unwrap_err();
    assert_eq!(failure.kind, SpatialFailureKind::Reference, "{failure}");
    assert_eq!(failure.entities, [edge]);
    for (top, middle) in [(40.0, 20.0), (60.0, 30.0)] {
        let mut inputs = SpatialInputs::default();
        inputs.curves.insert(
            edge,
            SpatialReferenceCurve::Line {
                start: [0.0; 3],
                end: [0.0, 0.0, top],
            },
        );
        let solved = def
            .build_with(&mut value, &inputs)
            .unwrap()
            .solve()
            .unwrap();
        assert!(near(solved.points[&p[1]], [0.0, 0.0, 15.0]), "{top}");
        assert!(near(solved.points[&p[2]], [0.0, 0.0, middle]), "{top}");
        assert_eq!(solved.status.dof, 0);
    }
    let turned = SketchFrame {
        origin: [100.0, 200.0, 300.0],
        x_axis: [0.0, 1.0, 0.0],
        y_axis: [-1.0, 0.0, 0.0],
    };
    let local = local_reference_curve(
        &turned,
        &crate::kernel::Curve3::Line {
            start: [100.0, 200.0, 300.0],
            end: [70.0, 240.0, 320.0],
        },
    );
    match local {
        Some(SpatialReferenceCurve::Line { start, end }) => {
            assert!(
                near(start, [0.0; 3]) && near(end, [40.0, 30.0, 20.0]),
                "{end:?}"
            );
        }
        other => panic!("{other:?}"),
    }
}

/// Fixture 14: a converted rectangle (in-plane horizontals, verticals,
/// lengths and a fixed corner, its points kept on the sketch plane by one
/// row each) keeps the planar sketch's freedom, none; releasing one corner
/// from the plane frees its elevation only.
#[test]
fn a_converted_rectangle_keeps_its_freedom() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let p = points_at(
        &mut e,
        &[
            [0.0, 0.0, 0.0],
            [40.0, 0.0, 0.0],
            [40.0, 30.0, 0.0],
            [0.0, 30.0, 0.0],
        ],
    );
    let l: Vec<EntityUid> = (0..4)
        .map(|i| {
            e.add_line(Existing(p[i]), Existing(p[(i + 1) % 4]), false)
                .unwrap()
        })
        .collect();
    for relation in [
        ConstraintKind::Horizontal { line: l[0] },
        ConstraintKind::Horizontal { line: l[2] },
        ConstraintKind::Vertical { line: l[1] },
        ConstraintKind::Vertical { line: l[3] },
    ] {
        e.add_constraint(SpatialConstraintKind::InPlane {
            relation,
            normal: SpatialAxis::Z,
        })
        .unwrap();
    }
    e.add_constraint(SpatialConstraintKind::InPlaneFixed {
        entity: Ref::Point(p[0]),
    })
    .unwrap();
    let mut lengths = Vec::new();
    for (line, value) in [(l[0], 40.0), (l[1], 30.0)] {
        lengths.push(
            e.add_dimension(
                SpatialDimensionKind::InPlane {
                    dimension: DimensionKind::Length { line },
                    normal: SpatialAxis::Z,
                },
                Some(&ValueInput::Number(value)),
                false,
            )
            .unwrap(),
        );
    }
    e.keep_on_sketch_plane(&p).unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.dof, 0, "{:?}", solved.status);
    assert!(solved.status.redundant.is_empty(), "{:?}", solved.status);
    assert!((solved.status.measured[&lengths[0]] - 40.0).abs() <= TOL);
    let mut e = edit(def, &mut params);
    assert_eq!(e.release_from_plane(&[p[2]]).unwrap(), [p[2]]);
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.dof, 1);
    assert!(near(solved.points[&p[2]], [40.0, 30.0, 0.0]));
}

/// Plane membership follows elevation groups: coincident points and a
/// plane-bound arc's points share one row.
#[test]
fn plane_membership_lists_one_point_per_elevation_group() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let p = points_at(
        &mut e,
        &[
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [10.0, 0.0, 0.0],
            [20.0, 0.0, 0.0],
        ],
    );
    e.add_curve(
        SpatialCurveKind::Arc {
            center: p[0],
            start: p[1],
            end: p[2],
            normal: [0.0, 0.0, 1.0],
            plane: Some(SpatialDirection::SKETCH_PLANE),
        },
        false,
    )
    .unwrap();
    e.add_line(Existing(p[3]), Existing(p[4]), false).unwrap();
    e.coincident(p[1], p[3]).unwrap();
    let kept = e.keep_on_sketch_plane(&p).unwrap();
    let (def, _, _) = e.finish().unwrap();
    match &def.constraint(kept).unwrap().kind {
        SpatialConstraintKind::OnSketchPlane { points } => assert_eq!(points, &[p[0], p[4]]),
        other => panic!("{other:?}"),
    }
    let solved = solve(&def, &params).unwrap();
    assert!(solved.status.redundant.is_empty(), "{:?}", solved.status);
}

/// mitcad#1550: a 2-point rectangle on the YZ plane through x = 5: four
/// lines, Horizontal and Vertical in that plane (along Y and along Z), its
/// corners kept in one plane square to X; 5 degrees of freedom, nothing
/// redundant; a typed width as a length dimension drives it.
#[test]
fn rectangle_on_the_yz_plane() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let made = e
        .add_rectangle(&RectangleRequest {
            corners: RectangleCorners::TwoPoint {
                a: At([5.0, 0.0, 0.0]),
                b: At([5.0, 40.0, 20.0]),
            },
            normal: X,
            construction: false,
        })
        .unwrap();
    assert_eq!((made.curves.len(), made.points.len()), (4, 4));
    // Four in-plane relations of the YZ plane and the Plane-Bound one.
    assert_eq!(made.constraints.len(), 5);
    let width = e
        .add_dimension(
            SpatialDimensionKind::Length {
                line: made.curves[0],
            },
            Some(&ValueInput::Number(50.0)),
            false,
        )
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    for k in &made.constraints[..4] {
        match &def.constraint(*k).unwrap().kind {
            SpatialConstraintKind::InPlane { normal, .. } => assert_eq!(*normal, X),
            other => panic!("{other:?}"),
        }
    }
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.dof, 4, "{:?}", solved.status);
    assert!(solved.status.redundant.is_empty());
    let at = |p: EntityUid| solved.points[&p];
    let [a, b, c, d] = [0, 1, 2, 3].map(|i| at(made.points[i]));
    for q in [a, b, c, d] {
        assert!((q[0] - 5.0).abs() <= TOL, "{q:?}");
    }
    assert!((b[1] - a[1] - 50.0).abs() <= TOL && (b[2] - a[2]).abs() <= TOL);
    assert!((c[1] - b[1]).abs() <= TOL && (d[2] - c[2]).abs() <= TOL);
    assert!((solved.status.measured[&width] - 50.0).abs() <= TOL);
}

/// mitcad#1550: a centre rectangle on the ZX plane keeps its centre at the
/// middle of its construction diagonal; a 3-point one is Perpendicular at
/// its corners.
#[test]
fn center_and_three_point_rectangles() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let centred = e
        .add_rectangle(&RectangleRequest {
            corners: RectangleCorners::Center {
                center: At([10.0, 2.0, 10.0]),
                corner: At([16.0, 2.0, 14.0]),
            },
            normal: Y,
            construction: false,
        })
        .unwrap();
    let centre = centred.center.unwrap();
    assert_eq!(centred.construction.len(), 1);
    let turned = e
        .add_rectangle(&RectangleRequest {
            corners: RectangleCorners::ThreePoint {
                a: At([0.0, 0.0, 30.0]),
                b: At([10.0, 10.0, 30.0]),
                c: [5.0, 15.0, 30.0],
            },
            normal: Z,
            construction: true,
        })
        .unwrap();
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert!(solved.status.redundant.is_empty(), "{:?}", solved.status);
    let at = |p: EntityUid| solved.points[&p];
    let (p0, p2) = (at(centred.points[0]), at(centred.points[2]));
    let mid = [
        (p0[0] + p2[0]) / 2.0,
        (p0[1] + p2[1]) / 2.0,
        (p0[2] + p2[2]) / 2.0,
    ];
    assert!(near(mid, at(centre)), "{mid:?}");
    for p in &centred.points {
        assert!((at(*p)[1] - 2.0).abs() <= TOL);
    }
    let q: Vec<[f64; 3]> = turned.points.iter().map(|p| at(*p)).collect();
    let (s1, s2) = (sub3(q[1], q[0]), sub3(q[2], q[1]));
    assert!(dot3(s1, s2).abs() <= 1e-6, "{q:?}");
    assert!(def.curve(turned.curves[0]).unwrap().construction);
}

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// mitcad#1550: an inscribed hexagon on the XY plane at z = 7 and a
/// circumscribed triangle: corners on a plane-bound construction circle,
/// sides Equal (or Tangent to the inner circle); 5 degrees of freedom each
/// (centre, size, turn), nothing redundant.
#[test]
fn polygons_on_a_plane() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let hexagon = e
        .add_polygon(&PolygonRequest {
            center: At([0.0, 0.0, 7.0]),
            vertex: At([10.0, 0.0, 7.0]),
            sides: 6,
            inscribed: true,
            normal: Z,
            construction: false,
        })
        .unwrap();
    assert_eq!((hexagon.curves.len(), hexagon.construction.len()), (6, 1));
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.dof, 5, "{:?}", solved.status);
    assert!(solved.status.redundant.is_empty());
    for p in &hexagon.points {
        let q = solved.points[p];
        assert!((q[2] - 7.0).abs() <= TOL);
        assert!(((q[0] * q[0] + q[1] * q[1]).sqrt() - 10.0).abs() <= TOL);
    }

    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let triangle = e
        .add_polygon(&PolygonRequest {
            center: At([0.0, 0.0, 0.0]),
            vertex: At([0.0, 5.0, 0.0]),
            sides: 3,
            inscribed: false,
            normal: X,
            construction: false,
        })
        .unwrap();
    assert_eq!(triangle.construction.len(), 2);
    let (def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    assert_eq!(solved.status.dof, 5, "{:?}", solved.status);
    assert!(solved.status.redundant.is_empty());
    // Its corners 10 mm from the centre (twice the apothem), on x = 0.
    for p in &triangle.points {
        let q = solved.points[p];
        assert!(q[0].abs() <= TOL);
        assert!(((q[1] * q[1] + q[2] * q[2]).sqrt() - 10.0).abs() <= 1e-5);
    }
    // A circumscribed polygon's vertex is a place, not a point.
    let mut params = Parameters::default();
    let mut e = edit(def, &mut params);
    let refused = e.add_polygon(&PolygonRequest {
        center: At([0.0, 0.0, 0.0]),
        vertex: Existing(triangle.points[0]),
        sides: 4,
        inscribed: false,
        normal: X,
        construction: false,
    });
    assert!(refused.is_err());
}

/// mitcad#1550: an ellipse on the ZX plane: its major point goes into the
/// plane through the centre, its minor radius is solved: a major and a
/// minor radius dimension drive it and the solve writes the minor radius
/// back; a point put on it stays in its plane.
#[test]
fn ellipse_on_the_zx_plane_solves_its_minor_radius() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let ellipse = e
        .add_ellipse(At([0.0, 3.0, 0.0]), At([0.0, 9.0, 20.0]), 5.0, Y, false)
        .unwrap();
    let major = def_point(&e, ellipse, true);
    assert!((e.position(major).unwrap()[1] - 3.0).abs() <= TOL);
    e.fix(def_point(&e, ellipse, false)).unwrap();
    let minor = e
        .add_dimension(
            SpatialDimensionKind::MinorRadius { curve: ellipse },
            Some(&ValueInput::Number(8.0)),
            false,
        )
        .unwrap();
    e.add_dimension(
        SpatialDimensionKind::MajorRadius { curve: ellipse },
        Some(&ValueInput::Number(20.0)),
        false,
    )
    .unwrap();
    let on = e.add_point([6.0, 1.0, 1.0]).unwrap();
    e.point_on(on, ellipse).unwrap();
    let (mut def, _, _) = e.finish().unwrap();
    let solved = solve(&def, &params).unwrap();
    match &solved.curves[&ellipse] {
        SolvedCurve::Ellipse { major, minor } => {
            assert!((major - 20.0).abs() <= TOL && (minor - 8.0).abs() <= TOL);
        }
        other => panic!("{other:?}"),
    }
    assert!((solved.status.measured[&minor] - 8.0).abs() <= TOL);
    assert!((solved.points[&on][1] - 3.0).abs() <= TOL);
    def.store_solved(&solved);
    assert_eq!(
        def.stored_minor(ellipse),
        Some(solved_minor(&solved, ellipse))
    );
    // A radius dimension is not an ellipse's.
    let mut bad = def.clone();
    bad.dimensions.push(SpatialDimension::new(
        ConstraintUid(99),
        SpatialDimensionKind::Radius { curve: ellipse },
        None,
    ));
    assert!(bad.check_definition().is_err());
}

/// The centre (`major` false) or major point of an ellipse being edited.
fn def_point(e: &SpatialEdit<'_>, ellipse: EntityUid, major: bool) -> EntityUid {
    match e.def.curve(ellipse).unwrap().kind {
        SpatialCurveKind::Ellipse {
            center, major: m, ..
        } => {
            if major {
                m
            } else {
                center
            }
        }
        ref other => panic!("{other:?}"),
    }
}

fn solved_minor(solved: &SpatialSolved, ellipse: EntityUid) -> f64 {
    match &solved.curves[&ellipse] {
        SolvedCurve::Ellipse { minor, .. } => *minor,
        other => panic!("{other:?}"),
    }
}

/// mitcad#1550: a text on the YZ plane at its anchor, and one in a frame:
/// the frame is a construction rectangle kept in the plane, its corner, x
/// corner and y corner the text's frame; deleting the rectangle's lines
/// leaves no point behind that only the Plane-Bound relation held.
#[test]
fn texts_on_a_plane() {
    let mut params = Parameters::default();
    let mut e = edit(SpatialSketchDef::default(), &mut params);
    let plain = e
        .add_text(&TextRequest {
            text: crate::SketchText::new(EntityUid(0), "AB", [0.0, 0.0], 5.0),
            at: [4.0, 10.0, 20.0],
            frame: None,
            normal: X,
        })
        .unwrap();
    let framed = e
        .add_text(&TextRequest {
            text: crate::SketchText::new(EntityUid(0), "C", [0.0, 0.0], 5.0),
            at: [0.0, 0.0, 0.0],
            frame: Some([30.0, 20.0, 0.0]),
            normal: Z,
        })
        .unwrap();
    assert_eq!(e.report.texts, vec![plain, framed]);
    let (def, _, _) = e.finish().unwrap();
    let text = def.texts.iter().find(|t| t.planar.id == plain).unwrap();
    assert_eq!(
        (text.normal, text.z, text.planar.at),
        (X, 4.0, [10.0, 20.0])
    );
    let boxed = def.texts.iter().find(|t| t.planar.id == framed).unwrap();
    assert_eq!(boxed.planar.frame.len(), 3);
    let corner = def.position(boxed.planar.frame[0]).unwrap();
    let x_corner = def.position(boxed.planar.frame[1]).unwrap();
    let y_corner = def.position(boxed.planar.frame[2]).unwrap();
    assert!(near(corner, [0.0, 0.0, 0.0]));
    assert!(near(x_corner, [30.0, 0.0, 0.0]) && near(y_corner, [0.0, 20.0, 0.0]));
    solve(&def, &params).unwrap();
    // The text goes alone; its frame stays.
    let mut e = edit(def, &mut params);
    e.delete_text(framed).unwrap();
    assert!(e.delete_text(framed).is_err());
    let lines: Vec<EntityUid> = e
        .def
        .curves
        .iter()
        .filter(|c| c.as_line().is_some())
        .map(|c| c.id)
        .collect();
    for l in lines {
        e.delete_curve(l).unwrap();
    }
    let (def, _, _) = e.finish().unwrap();
    assert!(def.points.is_empty(), "{:?}", def.points);
    assert!(
        !def.constraints
            .iter()
            .any(|c| matches!(c.kind, SpatialConstraintKind::PlaneBound { .. }))
    );
}
