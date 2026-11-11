// SPDX-License-Identifier: MIT
//! A planar sketch turned into a 3D sketch in place (mitcad#1544), with the
//! mock kernel: a fully constrained rectangle keeps its feature, ids,
//! parameters and freedom (none), one Undo takes the conversion back and
//! Redo makes it again, saved and read again; a corner released from the
//! plane has its elevation free (W15). A sketch whose profile an Extrude
//! uses converts keeping the profile (mitcad#1550); a Pipe-path sketch
//! converts and the Pipe keeps its pieces through the rewritten path; a
//! later sketch's projection of a converted sketch's line keeps following,
//! and so does a converted sketch's own linked projection (W16). A 3D
//! sketch is not converted again. Back to 2D (mitcad#1550): the extruded
//! sketch with its ids and profile, a 3D sketch drawn in its plane with
//! its Pipe's path rewritten, refused for geometry out of the plane; a
//! pattern kept through both conversions; a route whose curves run against
//! its first keeping its direction in 2D and back (Pipe, Sweep, points
//! along it).

use serde_json::json;

use crate::features::construction::point::PointDefinition;
use crate::features::path::CurvePath;
use crate::features::spatial_path::SpatialPath;
use crate::features::spatial_sketch::{
    SpatialAxis, SpatialConstraintKind, SpatialDimensionKind, SpatialSense, SpatialSource,
};
use crate::features::{FeatureDef, SketchPlane};
use crate::ids::{EntityUid, FeatureUid};
use crate::sketch::edit::PointInput;
use crate::sketch::geometry::Curve2;
use crate::sketch::spatial_edit::SegmentRequest;
use crate::sketch::spatial_edit::SpatialPointInput::{At, Existing};
use crate::sketch::{EntityKind, Ref};
use crate::test_support::{block, def, num};
use crate::testing::MockKernel;
use crate::{Document, FeatureStatus};

const TOL: f64 = 1e-6;

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= TOL)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn type_of(doc: &Document<MockKernel>, uid: FeatureUid) -> &'static str {
    match &doc.feature(uid).expect("a feature").def {
        FeatureDef::Sketch(_) => "sketch",
        FeatureDef::SpatialSketch(_) => "spatial_sketch",
        _ => "other",
    }
}

/// The pieces of a Pipe's last spec in the mock kernel.
fn path_of(doc: &Document<MockKernel>, feature: FeatureUid) -> String {
    let pipe = format!("pipe {feature} ");
    let specs = doc.kernel().specs.borrow();
    let spec = specs
        .iter()
        .rev()
        .find(|s| s.starts_with(&pipe))
        .unwrap_or_else(|| panic!("no spec of {feature}: {specs:?}"));
    let after = spec.split(" path ").nth(1).expect("a path");
    after.split(' ').next().expect("its pieces").to_owned()
}

/// A solved point of a 3D sketch, in the component.
fn point(doc: &Document<MockKernel>, sketch: FeatureUid, id: EntityUid) -> [f64; 3] {
    doc.spatial_sketch(sketch)
        .and_then(|o| o.point(id))
        .unwrap_or_else(|| panic!("p{} is not solved", id.0))
}

/// The point of a planar sketch's last solve at `at`.
fn planar_point_at(doc: &Document<MockKernel>, sketch: FeatureUid, at: [f64; 2]) -> EntityUid {
    let output = doc.sketch_output(sketch).expect("evaluated");
    *output
        .solved
        .points
        .iter()
        .find(|(_, p)| (p[0] - at[0]).abs() <= TOL && (p[1] - at[1]).abs() <= TOL)
        .unwrap_or_else(|| panic!("no point at {at:?}"))
        .0
}

/// W15: a fully constrained rectangle stays fully constrained with its
/// ids, its parameters and nothing redundant, as one undo step.
#[test]
fn a_fully_constrained_rectangle_converts_in_place() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    doc.add_rectangle(sketch, [0.0, 0.0], &num(40.0), &num(30.0))
        .unwrap();
    let FeatureDef::Sketch(planar) = doc.feature(sketch).unwrap().def.clone() else {
        panic!("a planar sketch");
    };
    let converted = doc.convert_sketch_to_spatial(sketch).unwrap();
    assert_eq!(type_of(&doc, sketch), "spatial_sketch");
    assert_eq!(doc.feature(sketch).unwrap().name, "Sketch1");
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    assert_eq!(converted.status.dof, 0, "{:?}", converted.status);
    assert!(converted.status.redundant.is_empty());
    assert!(converted.status.fully_constrained);
    // Every planar id kept; the constraints and dimensions in plane.
    let def = doc.spatial_sketch_def(sketch).unwrap().clone();
    let planar_points: Vec<EntityUid> = planar
        .entities
        .iter()
        .filter(|e| e.is_point())
        .map(|e| e.id)
        .collect();
    let points: Vec<EntityUid> = def.points.iter().map(|p| p.id).collect();
    assert_eq!(points, planar_points);
    let planar_curves: Vec<EntityUid> = planar
        .entities
        .iter()
        .filter(|e| !e.is_point())
        .map(|e| e.id)
        .collect();
    let curves: Vec<EntityUid> = def.curves.iter().map(|c| c.id).collect();
    assert_eq!(curves, planar_curves);
    let mut planar_items: Vec<_> = planar
        .constraints
        .iter()
        .map(|c| c.id)
        .chain(planar.dimensions.iter().map(|d| d.id))
        .collect();
    planar_items.sort();
    let mut kept = converted.in_plane.clone();
    kept.extend(&converted.coincident);
    kept.sort();
    assert_eq!(kept, planar_items);
    for d in &planar.dimensions {
        let spatial = def.dimension(d.id).unwrap();
        assert_eq!(spatial.value, d.value);
        assert!(matches!(spatial.kind, SpatialDimensionKind::InPlane { .. }));
    }
    for p in &def.points {
        assert!(near(point(&doc, sketch, p.id), [p.at[0], p.at[1], 0.0]));
    }
    // A dimension edit still drives it.
    doc.set_parameter("d1", 50.0).unwrap();
    let wide = def
        .points
        .iter()
        .any(|p| near(point(&doc, sketch, p.id), [50.0, 30.0, 0.0]));
    assert!(wide, "{:?}", doc.spatial_sketch(sketch));
    doc.undo();

    // One Undo takes it back, Redo makes it again.
    assert_eq!(doc.undo().as_deref(), Some("Convert Sketch1 to 3D Sketch"));
    assert_eq!(doc.feature(sketch).unwrap().def, FeatureDef::Sketch(planar));
    doc.redo();
    assert_eq!(type_of(&doc, sketch), "spatial_sketch");

    // Saved and read again.
    let file = doc.to_json();
    let mut again = Document::from_json(&file, MockKernel::default()).unwrap();
    again.recompute();
    assert_eq!(again.spatial_sketch_def(sketch), Some(&def));
    assert_eq!(status(&again, sketch), FeatureStatus::Ok);

    // A corner released from the plane: its elevation is free.
    let corner = def.points[2].id;
    let (_, report) = doc
        .edit_spatial_sketch(sketch, "Release from Plane", |e| {
            e.release_from_plane(&[corner])
        })
        .unwrap();
    assert_eq!(report.status.dof, 1);

    // Not again.
    let error = doc.convert_sketch_to_spatial(sketch).unwrap_err();
    assert!(error.to_string().contains("3D sketch already"), "{error}");
}

/// mitcad#1550 (W16 as the owner decided): a sketch whose profile an
/// Extrude uses converts, and the Extrude keeps its region: the 3D sketch's
/// closed loop in its sketch plane bounds it with the same key. Back to 2D
/// the sketch has its ids, constraints and dimensions again and the
/// Extrude still has its profile; each conversion is one undo step.
#[test]
fn an_extruded_sketch_converts_both_ways_keeping_its_profile() {
    let mut b = block();
    let FeatureDef::Sketch(before) = b.doc.feature(b.sketch).unwrap().def.clone() else {
        panic!("a planar sketch");
    };
    let shape = b.history(b.body);
    b.doc.convert_sketch_to_spatial(b.sketch).unwrap();
    assert_eq!(type_of(&b.doc, b.sketch), "spatial_sketch");
    assert_eq!(status(&b.doc, b.extrude), FeatureStatus::Ok);
    let regions: Vec<_> = b
        .doc
        .sketch_output(b.sketch)
        .expect("the 3D sketch's planar output")
        .regions
        .iter()
        .map(|r| r.key.clone())
        .collect();
    assert_eq!(regions, [b.region.clone()]);
    assert_eq!(b.history(b.body), shape);
    assert!(
        b.doc
            .profiles()
            .iter()
            .any(|p| p.sketch == b.sketch && p.region == b.region && p.consumed)
    );

    let back = b.doc.convert_sketch_to_planar(b.sketch).unwrap();
    assert!(back.notes.is_empty(), "{back:?}");
    assert_eq!(type_of(&b.doc, b.sketch), "sketch");
    assert_eq!(status(&b.doc, b.sketch), FeatureStatus::Ok);
    assert_eq!(status(&b.doc, b.extrude), FeatureStatus::Ok);
    assert_eq!(b.history(b.body), shape);
    let FeatureDef::Sketch(after) = b.doc.feature(b.sketch).unwrap().def.clone() else {
        panic!("a planar sketch");
    };
    let ids = |def: &crate::features::SketchDef| {
        let mut entities: Vec<EntityUid> = def.entities.iter().map(|e| e.id).collect();
        entities.sort();
        (entities, def.constraints.clone(), def.dimensions.clone())
    };
    assert_eq!(ids(&after), ids(&before));
    assert_eq!(
        b.doc.sketch_output(b.sketch).unwrap().solved.status.dof,
        0,
        "fully constrained again"
    );
    assert_eq!(
        b.doc.undo().as_deref(),
        Some("Convert Sketch1 to 2D Sketch")
    );
    assert_eq!(type_of(&b.doc, b.sketch), "spatial_sketch");
    assert_eq!(
        b.doc.undo().as_deref(),
        Some("Convert Sketch1 to 3D Sketch")
    );
    assert_eq!(
        b.doc.feature(b.sketch).unwrap().def,
        FeatureDef::Sketch(before)
    );
}

/// mitcad#1550: a 3D sketch with geometry out of its sketch plane is not
/// turned into 2D: the refusal names the point, and nothing changes.
#[test]
fn a_3d_sketch_out_of_its_plane_stays_3d() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    doc.edit_spatial_sketch(sketch, "Add Lines", |e| {
        let a = e.add_point([0.0, 0.0, 0.0])?;
        let b = e.add_point([10.0, 0.0, 0.0])?;
        let c = e.add_point([10.0, 0.0, 7.0])?;
        e.add_line(Existing(a), Existing(b), false)?;
        e.add_line(Existing(b), Existing(c), false)?;
        e.fix(a)?;
        Ok(())
    })
    .unwrap();
    let before = doc.feature(sketch).unwrap().def.clone();
    let error = doc.convert_sketch_to_planar(sketch).unwrap_err();
    // A person reads words, agents the point's id (mitcad#1644).
    let read = crate::notice::FeatureError::from_text(&error.to_string());
    assert_eq!(
        read.message,
        "3DSketch1 cannot become a 2D sketch: it is not all in its sketch plane: a point is 7 mm \
         off the sketch plane",
        "{error}"
    );
    assert!(
        read.agent_text().contains(
            "3DSketch1 cannot become a 2D sketch: it is not all in its sketch plane: p3 is 7 mm off \
             the sketch plane"
        ),
        "{error}"
    );
    assert_eq!(doc.feature(sketch).unwrap().def, before);
}

/// mitcad#1550: a 3D sketch drawn in its sketch plane, attached to the
/// origin, lines along X and Y with their lengths, becomes a planar sketch
/// with the same ids: the alignments as horizontal and vertical, the
/// lengths as planar lengths, the origin's attachment a fixed point; a
/// Pipe along its route keeps its pieces through the rewritten path.
#[test]
fn a_3d_sketch_drawn_in_its_plane_becomes_2d() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let request = |start, toward: [f64; 3], axis, length: f64| SegmentRequest {
        axis: Some((axis, SpatialSense::Positive)),
        length: Some(num(length)),
        ..SegmentRequest::new(start, At(toward))
    };
    let (ab, _) = doc
        .edit_spatial_sketch(sketch, "Add Line", |e| {
            e.add_segment(&SegmentRequest {
                start_source: Some(SpatialSource::Origin),
                ..request(At([0.0; 3]), [38.0, 1.0, 0.0], SpatialAxis::X, 40.0)
            })
        })
        .unwrap();
    let (bc, _) = doc
        .edit_spatial_sketch(sketch, "Add Line", |e| {
            e.add_segment(&request(
                Existing(ab.end),
                [41.0, 28.0, 0.0],
                SpatialAxis::Y,
                30.0,
            ))
        })
        .unwrap();
    let tube = doc
        .add_feature(
            &def(json!({"type": "pipe", "size": 4, "operation": "new_body",
                        "path": {"spatial_sketch": sketch,
                                 "curves": [ab.line.curve_name(), bc.line.curve_name()]}})),
            None,
        )
        .unwrap()
        .uid;
    let pieces = path_of(&doc, tube);
    let back = doc.convert_sketch_to_planar(sketch).unwrap();
    assert_eq!(back.rewritten, [(tube, "Pipe1".to_owned())]);
    assert_eq!(type_of(&doc, sketch), "sketch");
    assert_eq!(status(&doc, tube), FeatureStatus::Ok);
    assert_eq!(path_of(&doc, tube), pieces);
    let FeatureDef::Sketch(planar) = doc.feature(sketch).unwrap().def.clone() else {
        panic!("a planar sketch");
    };
    assert!(planar.constraints.iter().any(|c| matches!(
        c.kind,
        crate::sketch::ConstraintKind::Horizontal { line } if line == ab.line
    )));
    assert!(planar.constraints.iter().any(|c| matches!(
        c.kind,
        crate::sketch::ConstraintKind::Vertical { line } if line == bc.line
    )));
    assert_eq!(
        planar
            .dimensions
            .iter()
            .filter(|d| matches!(d.kind, crate::sketch::DimensionKind::Length { .. }))
            .count(),
        2
    );
    assert!(planar.entity(ab.start).unwrap().fixed);
    let output = doc.sketch_output(sketch).unwrap();
    assert_eq!(output.solved.status.dof, 0);
    let c = output.solved.points[&bc.end];
    assert!((c[0] - 40.0).abs() <= TOL && (c[1] - 30.0).abs() <= TOL);
}

/// mitcad#1550: a circular pattern converts with its sketch (no refusal):
/// kept in the 3D sketch's planar records, its copies follow its angle;
/// back in 2D the pattern is the sketch's own again.
#[test]
fn a_pattern_converts_and_keeps_binding_its_copies() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let (pattern, _) = doc
        .edit_sketch(sketch, "Circular Pattern", |e| {
            let point = e.add_point([10.0, 0.0]);
            e.circular_pattern(
                &[Ref::Point(point)],
                PointInput::At([0.0, 0.0]),
                4,
                &num(std::f64::consts::TAU),
            )
        })
        .unwrap();
    let FeatureDef::Sketch(planar) = doc.feature(sketch).unwrap().def.clone() else {
        panic!("a planar sketch");
    };
    let record = planar.patterns[0].clone();
    assert_eq!(record.id, pattern);
    let crate::sketch::pattern::PatternKind::Circular { angle, .. } = record.kind else {
        panic!("a circular pattern");
    };
    let angle = doc.parameters().name(angle);
    let converted = doc.convert_sketch_to_spatial(sketch).unwrap();
    assert_eq!(converted.kept, [format!("{pattern} (pattern)")]);
    let copy = |doc: &Document<MockKernel>, index: u32| {
        let id = record
            .copies
            .iter()
            .find(|c| c.index == [index, 0])
            .and_then(|c| c.entities.values().next())
            .expect("a copy")
            .uid();
        point(doc, sketch, id)
    };
    assert!(near(copy(&doc, 2), [-10.0, 0.0, 0.0]));
    // Three copies over half a turn. The original point is free, so the
    // solve moves it as well as the copies (as the planar sketch's solve
    // does): the last copy is the original turned half a turn about the
    // fixed centre at the origin.
    doc.set_parameter(&angle, std::f64::consts::PI).unwrap();
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    let original = point(&doc, sketch, record.entities[0].uid());
    assert!(
        near(copy(&doc, 3), [-original[0], -original[1], 0.0]),
        "{:?} {original:?}",
        copy(&doc, 3)
    );
    doc.convert_sketch_to_planar(sketch).unwrap();
    let FeatureDef::Sketch(back) = &doc.feature(sketch).unwrap().def else {
        panic!("a planar sketch");
    };
    assert_eq!(back.patterns, planar.patterns);
}

/// W16: a sketch used as a Pipe's path converts, and the Pipe keeps its
/// pieces in their order through the rewritten path (the same curves,
/// chained, running the same way).
#[test]
fn a_pipe_path_sketch_converts_and_the_pipe_keeps_its_path() {
    let mut doc = Document::new(MockKernel::default());
    let planar = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let ((first, second), _) = doc
        .edit_sketch(planar, "Add Line", |e| {
            let first = e.add_line(
                PointInput::At([0.0, 0.0]),
                PointInput::At([40.0, 0.0]),
                false,
                false,
            )?;
            let end = match e.def.entity(first).map(|x| &x.kind) {
                Some(EntityKind::Line { end, .. }) => *end,
                _ => return Err("a line".to_owned()),
            };
            let second = e.add_line(
                PointInput::Existing(end),
                PointInput::At([40.0, 30.0]),
                false,
                false,
            )?;
            Ok((first, second))
        })
        .unwrap();
    let tube = doc
        .add_feature(
            &def(json!({"type": "pipe", "size": 4, "operation": "new_body",
                        "path": {"sketch": planar, "curves": [second.curve_name()],
                                 "chain": true}})),
            None,
        )
        .unwrap()
        .uid;
    let before = path_of(&doc, tube);
    let converted = doc.convert_sketch_to_spatial(planar).unwrap();
    assert_eq!(converted.rewritten, [(tube, "Pipe1".to_owned())]);
    let FeatureDef::Pipe(pipe) = &doc.feature(tube).unwrap().def else {
        panic!("a pipe");
    };
    assert_eq!(
        pipe.path,
        CurvePath::Spatial(SpatialPath {
            sketch: planar,
            curves: vec![second],
            reversed: true,
            chain: true,
        })
    );
    assert_eq!(status(&doc, tube), FeatureStatus::Ok);
    assert_eq!(path_of(&doc, tube), before);
    assert!(before.contains(&first.curve_name()), "{before}");
    // Undo puts the planar path back.
    doc.undo();
    let FeatureDef::Pipe(pipe) = &doc.feature(tube).unwrap().def else {
        panic!("a pipe");
    };
    assert!(matches!(pipe.path, CurvePath::Sketch(_)), "{:?}", pipe.path);
}

/// W16: a later sketch's linked projection of a converted sketch's line
/// keeps following its dimension; converted too, the later sketch keeps
/// its projection linked (a construction line whose ends are attached to
/// the source line's), which still follows.
#[test]
fn projections_keep_following_across_conversions() {
    let mut doc = Document::new(MockKernel::default());
    let source = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let rectangle = doc
        .add_rectangle(source, [0.0, 0.0], &num(60.0), &num(40.0))
        .unwrap();
    // The bottom line: the rectangle's line through (0, 0) and (60, 0).
    let on = |p: [f64; 2], x: f64| (p[0] - x).abs() <= TOL && p[1].abs() <= TOL;
    let bottom = *rectangle
        .curves
        .iter()
        .find(|c| {
            matches!(
                doc.sketch_output(source).unwrap().solved.curves.get(*c),
                Some(Curve2::Line { a, b })
                    if (on(*a, 0.0) && on(*b, 60.0)) || (on(*a, 60.0) && on(*b, 0.0))
            )
        })
        .expect("the bottom line");
    let later = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    doc.project_sketch_geometry(
        later,
        source,
        Some(&[Ref::Curve(bottom)]),
        true,
        None,
        None,
        false,
    )
    .unwrap();
    let end = planar_point_at(&doc, later, [60.0, 0.0]);
    // The later sketch's own line from the projected end.
    doc.edit_sketch(later, "Add Line", |e| {
        e.add_line(
            PointInput::Existing(end),
            PointInput::At([60.0, 20.0]),
            false,
            false,
        )
    })
    .unwrap();

    // The source converted: the projection follows its width.
    doc.convert_sketch_to_spatial(source).unwrap();
    doc.set_parameter("d1", 80.0).unwrap();
    assert_eq!(status(&doc, later), FeatureStatus::Ok);
    let moved = planar_point_at(&doc, later, [80.0, 0.0]);
    assert_eq!(moved, end);

    // The later sketch converted: its projected line is attached to the
    // source line's ends, and still follows.
    let converted = doc.convert_sketch_to_spatial(later).unwrap();
    assert_eq!(converted.attachments.len(), 2, "{converted:?}");
    assert!(converted.references.is_empty());
    assert!(converted.status.redundant.is_empty(), "{converted:?}");
    let def = doc.spatial_sketch_def(later).unwrap().clone();
    let attached: Vec<EntityUid> = def
        .attachment_records()
        .map(|(_, point, _, projected)| {
            assert!(projected);
            point
        })
        .collect();
    assert!(attached.contains(&end), "{attached:?}");
    assert!(near(point(&doc, later, end), [80.0, 0.0, 0.0]));
    doc.set_parameter("d1", 70.0).unwrap();
    assert_eq!(status(&doc, later), FeatureStatus::Ok);
    assert!(near(point(&doc, later, end), [70.0, 0.0, 0.0]));
    // Its own line is kept in plane; its free end is on the sketch plane.
    let kept = def
        .constraints
        .iter()
        .find_map(|c| match &c.kind {
            SpatialConstraintKind::OnSketchPlane { points } => Some(points.clone()),
            _ => None,
        })
        .expect("an On Sketch Plane relation");
    assert!(!kept.contains(&end), "{kept:?}");
}

/// The pieces of the last pipe or sweep the mock kernel made for
/// `feature`, in order: `c3,c2,c1`.
fn swept_path(doc: &Document<MockKernel>, feature: FeatureUid) -> String {
    let (pipe, sweep) = (format!("pipe {feature} "), format!("sweep {feature} "));
    let specs = doc.kernel().specs.borrow();
    let spec = specs
        .iter()
        .rev()
        .find(|s| s.starts_with(&pipe) || s.starts_with(&sweep))
        .unwrap_or_else(|| panic!("no spec of {feature}: {specs:?}"));
    let after = spec.split(" path ").nth(1).expect("a path");
    after.split(' ').next().expect("its pieces").to_owned()
}

/// A construction point's place.
fn datum_point(doc: &Document<MockKernel>, uid: FeatureUid) -> [f64; 3] {
    match doc.datum(uid) {
        Some(crate::datum::Datum::Point(p)) => p.point,
        other => panic!("{uid}: {other:?}"),
    }
}

/// mitcad#1550: a 3D path keeps its direction through 3D→2D. The route
/// A-B-C-D in the sketch plane: AB drawn from A to B, its second and
/// third curves drawn the other way (C to B, D to C). A Pipe against AB
/// (D to A), a Sweep against BC by chain selection (A to D) and points
/// along the route and along single lines run the same way as 2D
/// sketch's paths: the planar path is `reversed` where the chain of its
/// curves runs the other way, and back in 3D the paths are as they were.
#[test]
fn a_3d_path_keeps_its_direction_back_in_2d() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let ((ab, bc, cd), _) = doc
        .edit_spatial_sketch(sketch, "Add Lines", |e| {
            let a = e.add_point([0.0, 0.0, 0.0])?;
            let b = e.add_point([40.0, 0.0, 0.0])?;
            let c = e.add_point([40.0, 30.0, 0.0])?;
            let d = e.add_point([0.0, 30.0, 0.0])?;
            let ab = e.add_line(Existing(a), Existing(b), false)?;
            let bc = e.add_line(Existing(c), Existing(b), false)?;
            let cd = e.add_line(Existing(d), Existing(c), false)?;
            for p in [a, b, c, d] {
                e.fix(p)?;
            }
            Ok((ab, bc, cd))
        })
        .unwrap();
    let forward = [ab, bc, cd].map(|c| c.curve_name()).join(",");
    let backward = [cd, bc, ab].map(|c| c.curve_name()).join(",");
    fn curves(ids: &[EntityUid]) -> Vec<String> {
        ids.iter().map(|c| c.curve_name()).collect()
    }

    let against_ab = json!({"spatial_sketch": sketch, "curves": curves(&[ab, bc, cd]),
                            "reversed": true});
    let tube = doc
        .add_feature(
            &def(json!({"type": "pipe", "size": 4, "operation": "new_body",
                        "path": against_ab.clone()})),
            None,
        )
        .unwrap()
        .uid;
    let profile = doc.add_sketch(SketchPlane::Yz).unwrap().uid;
    let region = doc
        .add_rectangle(profile, [-2.0, -2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let against_bc = json!({"spatial_sketch": sketch, "curves": curves(&[bc]),
                            "chain": true, "reversed": true});
    let sweep = doc
        .add_feature(
            &def(
                json!({"type": "sweep", "profiles": [{"sketch": profile, "region": region}],
                        "path": against_bc, "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    let along = |doc: &mut Document<MockKernel>, path: serde_json::Value, distance: f64| {
        doc.add_feature(
            &def(json!({"type": "construction_point",
                        "definition": {"type": "along_path", "path": path,
                                       "distance": distance}})),
            None,
        )
        .unwrap()
        .uid
    };
    let start = along(&mut doc, against_ab, 0.0);
    let back_on_ab = along(
        &mut doc,
        json!({"spatial_sketch": sketch, "curves": curves(&[ab]), "reversed": true}),
        0.25,
    );
    let on_bc = along(
        &mut doc,
        json!({"spatial_sketch": sketch, "curves": curves(&[bc])}),
        0.0,
    );
    for uid in [tube, sweep, start, back_on_ab, on_bc] {
        assert_eq!(status(&doc, uid), FeatureStatus::Ok, "{uid}");
    }
    assert_eq!(swept_path(&doc, tube), backward);
    assert_eq!(swept_path(&doc, sweep), forward);
    let places = |doc: &Document<MockKernel>| {
        assert!(near(datum_point(doc, start), [0.0, 30.0, 0.0]));
        assert!(near(datum_point(doc, back_on_ab), [30.0, 0.0, 0.0]));
        assert!(near(datum_point(doc, on_bc), [40.0, 30.0, 0.0]));
    };
    places(&doc);
    let stored = |doc: &Document<MockKernel>| {
        let pipe = match &doc.feature(tube).unwrap().def {
            FeatureDef::Pipe(p) => p.path.clone(),
            other => panic!("{other:?}"),
        };
        let swept = match &doc.feature(sweep).unwrap().def {
            FeatureDef::Sweep(s) => s.path.clone(),
            other => panic!("{other:?}"),
        };
        (pipe, swept)
    };
    let in_3d = stored(&doc);

    let back = doc.convert_sketch_to_planar(sketch).unwrap();
    assert!(back.notes.is_empty(), "{back:?}");
    assert_eq!(type_of(&doc, sketch), "sketch");
    for uid in [tube, sweep, start, back_on_ab, on_bc] {
        assert_eq!(status(&doc, uid), FeatureStatus::Ok, "{uid}");
    }
    // The same pieces in the same order, and the same places.
    assert_eq!(swept_path(&doc, tube), backward);
    assert_eq!(swept_path(&doc, sweep), forward);
    places(&doc);
    // The planar chain of AB, BC, CD runs from A: reversed. So does the
    // chain through BC (along BC from C): reversed. BC alone runs its own
    // way.
    let (pipe_path, sweep_path) = stored(&doc);
    assert_eq!(
        serde_json::to_value(&pipe_path).unwrap(),
        json!({"sketch": sketch, "curves": curves(&[ab, bc, cd]), "reversed": true})
    );
    assert_eq!(
        serde_json::to_value(&sweep_path).unwrap(),
        json!({"sketch": sketch, "curves": curves(&[bc]), "chain": true, "reversed": true})
    );
    let point_path = |uid: FeatureUid| match &doc.feature(uid).unwrap().def {
        FeatureDef::ConstructionPoint(p) => match &p.definition {
            PointDefinition::AlongPath { path, .. } => serde_json::to_value(path).unwrap(),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    assert_eq!(point_path(back_on_ab)["reversed"], json!(true));
    assert_eq!(point_path(on_bc).get("reversed"), None);

    // Saved and read again, the planar paths keep running that way.
    let file = doc.to_json();
    let mut again = Document::from_json(&file, MockKernel::default()).unwrap();
    again.recompute();
    assert_eq!(swept_path(&again, tube), backward);
    assert_eq!(swept_path(&again, sweep), forward);
    places(&again);

    // Back in 3D, the paths are the ones the 3D sketch had.
    doc.convert_sketch_to_spatial(sketch).unwrap();
    assert_eq!(stored(&doc), in_3d);
    assert_eq!(swept_path(&doc, tube), backward);
    assert_eq!(swept_path(&doc, sweep), forward);
    places(&doc);
}
