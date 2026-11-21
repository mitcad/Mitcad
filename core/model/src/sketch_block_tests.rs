// SPDX-License-Identifier: MIT
//! Sketch blocks in a design (mitcad#2110), with the mock kernel: a block
//! from a selection (the selection an instance in place), instances in two
//! sketches placed by a coincident base point and an angle, the block
//! edited in place and both instances following, a constraint on a
//! removed curve reported while the sketch still solves, an explode, a
//! nested block and the refused cycle, removal refused while instances
//! exist, an edit in a sketch of its own, the design saved and read back,
//! and an instance's region extruded.

use std::f64::consts::FRAC_PI_6;

use crate::features::{FeatureDef, SketchDef, SketchPlane};
use crate::ids::{EntityUid, FeatureUid};
use crate::sketch::edit::PointInput;
use crate::sketch::geometry::P2;
use crate::sketch::modify::Item;
use crate::sketch::tools::SlotMode;
use crate::sketch::{ConstraintKind, ConstraintUid, DimensionKind, EntityIndex, EntityKind, Ref};
use crate::test_support::{extrude, num};
use crate::testing::MockKernel;
use crate::{Document, FeatureStatus};

const TOL: f64 = 1e-6;

fn near(a: P2, b: P2) -> bool {
    (a[0] - b[0]).abs() < TOL && (a[1] - b[1]).abs() < TOL
}

fn sketch_def(doc: &Document<MockKernel>, sketch: FeatureUid) -> SketchDef {
    match &doc.feature(sketch).expect("a feature").def {
        FeatureDef::Sketch(def) => def.clone(),
        _ => panic!("not a sketch"),
    }
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

/// A sketch with a slot from (0, 0) to (20, 0), 6 wide.
fn slot_sketch(doc: &mut Document<MockKernel>) -> FeatureUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    doc.edit_sketch(sketch, "Slot", |edit| {
        edit.slot(
            SlotMode::CenterToCenter {
                a: PointInput::At([0.0, 0.0]),
                b: PointInput::At([20.0, 0.0]),
            },
            6.0,
        )
    })
    .unwrap();
    sketch
}

fn everything(doc: &Document<MockKernel>, sketch: FeatureUid) -> Vec<Item> {
    sketch_def(doc, sketch)
        .entities
        .iter()
        .map(|e| Item::Entity(e.as_ref()))
        .collect()
}

/// The length of an instance's slot centre line (the block's construction
/// line) as the sketch solved it.
fn centre_length(doc: &Document<MockKernel>, sketch: FeatureUid, instance: ConstraintUid) -> f64 {
    let def = sketch_def(doc, sketch);
    let record = def.blocks.iter().find(|b| b.id == instance).unwrap();
    let block = doc
        .sketch_blocks()
        .iter()
        .find(|b| b.id == record.block)
        .unwrap();
    let axis = block
        .content
        .entities
        .iter()
        .find(|e| e.construction && matches!(e.kind, EntityKind::Line { .. }))
        .expect("the slot's centre line");
    let line = record.entities[&axis.as_ref()].uid();
    let index = EntityIndex::new(&def.entities);
    let EntityKind::Line { start, end, .. } = index.get(line).unwrap().kind else {
        panic!("a line");
    };
    let (a, b) = (index.point(start).unwrap(), index.point(end).unwrap());
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

/// A design with the slot of Sketch1 made block "Slot" in place, and an
/// instance in Sketch2 placed by a fixed point and a fixed line: its base
/// point on (100, 20), its direction at 30° to the line.
struct Design {
    doc: Document<MockKernel>,
    first: FeatureUid,
    first_instance: ConstraintUid,
    second: FeatureUid,
    second_instance: ConstraintUid,
    line: EntityUid,
}

fn design() -> Design {
    let mut doc = Document::new(MockKernel::default());
    let first = slot_sketch(&mut doc);
    let items = everything(&doc, first);
    let made = doc
        .add_sketch_block(first, &items, Some("Slot"), [10.0, 0.0], 0.0)
        .unwrap();
    let first_instance = made.instance.unwrap();
    let second = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let inserted = doc
        .insert_sketch_block(second, "Slot", [40.0, 10.0], 0.2)
        .unwrap();
    let second_instance = inserted.instance.unwrap();
    let record = sketch_def(&doc, second).blocks[0].clone();
    let (line, _) = doc
        .edit_sketch(second, "Place", |edit| {
            let corner = edit.add_point([100.0, 20.0]);
            let a = edit.add_point([0.0, -50.0]);
            let b = edit.add_point([10.0, -50.0]);
            let line = edit.add_curve(EntityKind::Line {
                start: a,
                end: b,
                centerline: false,
            });
            edit.set_fixed(&[Ref::Point(corner), Ref::Curve(line)], true)?;
            edit.add_constraint(ConstraintKind::Coincident {
                point: record.base,
                entity: Ref::Point(corner),
            });
            edit.add_dimension(
                DimensionKind::Angle {
                    a: line,
                    b: record.direction,
                    reverse_a: None,
                    reverse_b: None,
                },
                Some(&num(FRAC_PI_6)),
                false,
                None,
            )?;
            Ok(line)
        })
        .unwrap();
    Design {
        doc,
        first,
        first_instance,
        second,
        second_instance,
        line,
    }
}

#[test]
fn a_block_from_a_selection_is_an_instance_in_place_and_others_are_placed_by_constraints() {
    let d = design();
    assert_eq!(d.doc.sketch_blocks().len(), 1);
    assert_eq!(d.doc.sketch_block_instances(1), 2);
    assert_eq!(status(&d.doc, d.first), FeatureStatus::Ok);
    assert_eq!(status(&d.doc, d.second), FeatureStatus::Ok);
    // The second instance: its base point on the corner, turned 30°.
    let def = sketch_def(&d.doc, d.second);
    let record = def
        .blocks
        .iter()
        .find(|b| b.id == d.second_instance)
        .unwrap();
    let index = EntityIndex::new(&def.entities);
    let (at, angle, _) = record.pose(&index).unwrap();
    assert!(near(at, [100.0, 20.0]), "{at:?}");
    assert!((angle - FRAC_PI_6).abs() < TOL, "{angle}");
    let block = &d.doc.sketch_blocks()[0];
    for e in block.content.entities.iter() {
        if let EntityKind::Point { at: q } = e.kind {
            let (s, c) = FRAC_PI_6.sin_cos();
            let want = [100.0 + c * q[0] - s * q[1], 20.0 + s * q[0] + c * q[1]];
            let got = index.point(record.entities[&e.as_ref()].uid()).unwrap();
            assert!(near(got, want), "{got:?} {want:?}");
        }
    }
    assert!(d.line.0 > 0);
    assert!((centre_length(&d.doc, d.first, d.first_instance) - 20.0).abs() < TOL);
}

#[test]
fn an_edited_block_moves_both_instances_and_a_lost_constraint_is_reported() {
    let mut d = design();
    // A point on the second instance's top line, held there.
    let block = d.doc.sketch_blocks()[0].clone();
    let top = block
        .content
        .entities
        .iter()
        .find(|e| matches!(e.kind, EntityKind::Line { .. }) && !e.construction)
        .unwrap()
        .as_ref();
    let record = sketch_def(&d.doc, d.second).blocks[0].clone();
    let on = record.entities[&top].uid();
    d.doc
        .edit_sketch(d.second, "Hold", |edit| {
            let p = edit.add_point([0.0, 0.0]);
            edit.add_constraint(ConstraintKind::Coincident {
                point: p,
                entity: Ref::Curve(on),
            });
            Ok(())
        })
        .unwrap();
    // Edit the block in place in Sketch1: its centre line 30 long.
    let opened = d
        .doc
        .edit_sketch_block("Slot", Some((d.first, d.first_instance)))
        .unwrap();
    assert_eq!(opened.sketch, Some(d.first));
    let first_record = sketch_def(&d.doc, d.first).blocks[0].clone();
    let axis = block
        .content
        .entities
        .iter()
        .find(|e| e.construction && matches!(e.kind, EntityKind::Line { .. }))
        .unwrap()
        .as_ref();
    let axis_line = first_record.entities[&axis].uid();
    d.doc
        .edit_sketch(d.first, "Longer", |edit| {
            edit.add_dimension(
                DimensionKind::Length { line: axis_line },
                Some(&num(30.0)),
                false,
                None,
            )
        })
        .unwrap();
    d.doc.finish_sketch_block(false).unwrap();
    assert!(d.doc.open_sketch_block().is_none());
    assert!((centre_length(&d.doc, d.first, d.first_instance) - 30.0).abs() < TOL);
    assert!((centre_length(&d.doc, d.second, d.second_instance) - 30.0).abs() < TOL);
    assert_eq!(status(&d.doc, d.second), FeatureStatus::Ok);
    // The block without its top line: the constraint on it is lost, and
    // the sketch still solves.
    d.doc
        .edit_sketch_block("Slot", Some((d.first, d.first_instance)))
        .unwrap();
    let top_now = sketch_def(&d.doc, d.first).blocks[0].entities[&top];
    d.doc
        .edit_sketch(d.first, "Delete", |edit| {
            edit.remove(&[Item::Entity(top_now)])
        })
        .unwrap();
    let finished = d.doc.finish_sketch_block(false).unwrap();
    assert_eq!(finished.warnings.len(), 1, "{:?}", finished.warnings);
    assert_eq!(status(&d.doc, d.second), FeatureStatus::Ok);
    let warnings = d.doc.warnings(d.second);
    assert!(
        warnings.iter().any(|w| w.contains("lost its geometry")),
        "{warnings:?}"
    );
    // No ids in what a person reads.
    let id_like = |t: &str| {
        t.len() > 1
            && (t.starts_with('c') || t.starts_with('k') || t.starts_with('p'))
            && t[1..].chars().all(|c| c.is_ascii_digit())
    };
    assert!(
        !warnings.iter().any(|w| w.split_whitespace().any(id_like)),
        "{warnings:?}"
    );
    let def = sketch_def(&d.doc, d.second);
    assert_eq!(def.blocks[0].lost.len(), 1);
    assert!(def.entity(on).is_none());
}

#[test]
fn explode_remove_and_cycles() {
    let mut d = design();
    // Removal is refused while instances exist.
    let error = d.doc.remove_sketch_block("Slot").unwrap_err().to_string();
    assert!(error.contains("explode or delete them first"), "{error}");
    // A block of the first instance and a circle: nested.
    let center = {
        let (c, _) = d
            .doc
            .edit_sketch(d.first, "Circle", |edit| {
                let center = edit.add_point([40.0, 0.0]);
                Ok(edit.add_curve(EntityKind::Circle {
                    center,
                    radius: 4.0,
                    start: 0.0,
                }))
            })
            .unwrap();
        c
    };
    assert!(center.0 > 0);
    let items = everything(&d.doc, d.first);
    let pair = d
        .doc
        .add_sketch_block(d.first, &items, Some("Pair"), [0.0, 0.0], 0.0)
        .unwrap();
    let blocks = d.doc.sketch_blocks();
    let pair_block = blocks.iter().find(|b| b.name == "Pair").unwrap();
    assert_eq!(pair_block.instances.len(), 1);
    assert_eq!(d.doc.sketch_block_instances(1), 2, "in Sketch2 and in Pair");
    // Editing Slot in place, an instance of Pair cannot go in: a cycle.
    d.doc
        .edit_sketch_block("Slot", Some((d.second, d.second_instance)))
        .unwrap();
    let error = d
        .doc
        .insert_sketch_block(d.second, "Pair", [0.0, 80.0], 0.0)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("cannot hold an instance of itself"),
        "{error}"
    );
    d.doc.finish_sketch_block(true).unwrap();
    // Explode the Pair instance: plain geometry, the Slot instance stays.
    d.doc
        .explode_sketch_block(d.first, pair.instance.unwrap())
        .unwrap();
    let def = sketch_def(&d.doc, d.first);
    assert_eq!(def.blocks.len(), 1);
    assert_eq!(def.blocks[0].block, 1);
    assert_eq!(status(&d.doc, d.first), FeatureStatus::Ok);
    // Pair has no instances now: it can go.
    d.doc.remove_sketch_block("Pair").unwrap();
    assert_eq!(d.doc.sketch_blocks().len(), 1);
    d.doc.rename_sketch_block("Slot", "Long slot").unwrap();
    assert_eq!(d.doc.sketch_blocks()[0].name, "Long slot");
}

#[test]
fn a_block_edited_in_a_sketch_of_its_own_and_the_design_saved_and_read() {
    let mut d = design();
    let features = d.doc.features().count();
    let opened = d.doc.edit_sketch_block("Slot", None).unwrap();
    let own = opened.sketch.unwrap();
    assert_eq!(d.doc.features().count(), features + 1);
    let record = sketch_def(&d.doc, own).blocks[0].clone();
    let index_def = sketch_def(&d.doc, own);
    let index = EntityIndex::new(&index_def.entities);
    assert!(near(record.pose(&index).unwrap().0, [0.0, 0.0]));
    // A circle at the base point joins the block.
    d.doc
        .edit_sketch(own, "Circle", |edit| {
            let center = edit.add_point([0.0, 0.0]);
            edit.add_curve(EntityKind::Circle {
                center,
                radius: 1.0,
                start: 0.0,
            });
            Ok(())
        })
        .unwrap();
    let finished = d.doc.finish_sketch_block(false).unwrap();
    assert_eq!(finished.sketch, None);
    assert_eq!(d.doc.features().count(), features, "its sketch went");
    assert!(d.doc.sketch_blocks()[0].content.entities.iter().any(
        |e| matches!(e.kind, EntityKind::Circle { radius, .. } if (radius - 1.0).abs() < TOL)
    ));
    // Both instances have the circle.
    for sketch in [d.first, d.second] {
        let def = sketch_def(&d.doc, sketch);
        assert!(
            def.entities
                .iter()
                .any(|e| matches!(e.kind, EntityKind::Circle { .. }))
        );
        assert_eq!(status(&d.doc, sketch), FeatureStatus::Ok);
    }
    // Saved and read back with the blocks and the instances.
    let json = d.doc.to_json();
    let mut read = Document::from_json(&json, MockKernel::default()).unwrap();
    // Reading evaluates nothing; the instances solve on the recompute.
    read.recompute();
    assert_eq!(read.sketch_blocks(), d.doc.sketch_blocks());
    assert_eq!(
        sketch_def(&read, d.second).blocks,
        sketch_def(&d.doc, d.second).blocks
    );
    assert_eq!(status(&read, d.second), FeatureStatus::Ok);
}

#[test]
fn an_instances_region_is_extruded() {
    let d = design();
    let mut doc = d.doc;
    let output = doc.sketch_output(d.second).expect("evaluated");
    let region = output
        .regions
        .iter()
        .map(|r| r.key.clone())
        .next()
        .expect("the slot's region");
    let added = doc
        .add_feature(&extrude(d.second, &[&region], 5.0, "new_body", &[]), None)
        .unwrap();
    assert_eq!(status(&doc, added.uid), FeatureStatus::Ok);
}
