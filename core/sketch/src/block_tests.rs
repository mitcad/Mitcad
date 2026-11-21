// SPDX-License-Identifier: MIT
//! Sketch blocks (mitcad#2110): a block from a selection, instances placed
//! and turned by constraints, an edited block's instances following, a
//! lost constraint, explode, nesting and the refusals.

use std::f64::consts::FRAC_PI_6;

use super::block::{SketchBlock, holds, sync_nested};
use super::edit::{PointInput, SketchEdit};
use super::geometry::{Curve2, P2};
use super::modify::Item;
use super::tools::SlotMode;
use super::*;
use crate::def::SketchDef;
use crate::ids::FeatureUid;
use crate::parameters::Parameters;
use crate::values::ValueInput;

const TOL: f64 = 1e-6;

fn near(a: P2, b: P2) -> bool {
    (a[0] - b[0]).abs() < TOL && (a[1] - b[1]).abs() < TOL
}

fn turn(p: P2, angle: f64) -> P2 {
    let (s, c) = angle.sin_cos();
    [c * p[0] - s * p[1], s * p[0] + c * p[1]]
}

/// A sketch with a slot from (0, 0) to (20, 0), 6 wide, and its curves.
fn slot_sketch(params: &mut Parameters) -> (SketchDef, Vec<EntityUid>) {
    let mut edit = SketchEdit::new(SketchDef::default(), params, FeatureUid(1), "Sketch1");
    let curves = edit
        .slot(
            SlotMode::CenterToCenter {
                a: PointInput::At([0.0, 0.0]),
                b: PointInput::At([20.0, 0.0]),
            },
            6.0,
        )
        .unwrap();
    let (def, _) = edit.finish().unwrap();
    (def, curves)
}

/// Every point and curve of the sketch as a selection.
fn everything(def: &SketchDef) -> Vec<Item> {
    def.entities
        .iter()
        .map(|e| Item::Entity(e.as_ref()))
        .collect()
}

/// The slot of `slot_sketch` made a block, base point at its middle.
fn slot_block(params: &mut Parameters) -> (SketchDef, SketchBlock) {
    let (def, _) = slot_sketch(params);
    let items = everything(&def);
    let mut edit = SketchEdit::new(def, params, FeatureUid(1), "Sketch1");
    let (block, _) = edit
        .make_block(1, "Slot", &items, [10.0, 0.0], 0.0, true)
        .unwrap();
    let (def, _) = edit.finish().unwrap();
    (def, block)
}

/// The block's points placed at `at` turned by `angle`.
fn placed_points(block: &SketchBlock, at: P2, angle: f64) -> Vec<P2> {
    block
        .content
        .entities
        .iter()
        .filter_map(|e| match e.kind {
            EntityKind::Point { at: q } => {
                let t = turn(q, angle);
                Some([at[0] + t[0], at[1] + t[1]])
            }
            _ => None,
        })
        .collect()
}

/// The positions of an instance's block points, in the block's order.
fn instance_points(def: &SketchDef, id: ConstraintUid, block: &SketchBlock) -> Vec<P2> {
    let instance = def.blocks.iter().find(|b| b.id == id).unwrap();
    let index = EntityIndex::new(&def.entities);
    block
        .content
        .entities
        .iter()
        .filter(|e| e.is_point())
        .map(|e| index.point(instance.entities[&e.as_ref()].uid()).unwrap())
        .collect()
}

#[test]
fn a_selection_becomes_a_block_and_an_instance_in_place() {
    let mut params = Parameters::default();
    let (before, _) = slot_sketch(&mut params);
    let (def, block) = slot_block(&mut params);
    assert_eq!(block.name, "Slot");
    assert_eq!(def.blocks.len(), 1);
    let instance = &def.blocks[0];
    // The selection keeps its ids and places.
    for e in before.entities.iter() {
        assert_eq!(instance.entities[&e.as_ref()], e.as_ref());
        if let EntityKind::Point { at } = e.kind {
            let now = def.entity(e.id).unwrap();
            assert!(matches!(now.kind, EntityKind::Point { at: p } if near(p, at)));
        }
    }
    // Its constraints and dimensions are the block's now, in its frame.
    assert!(def.constraints.is_empty(), "{:?}", def.constraints);
    assert!(!block.content.constraints.is_empty());
    let local: Vec<P2> = placed_points(&block, [0.0, 0.0], 0.0);
    assert!(local.iter().any(|p| near(*p, [-10.0, 0.0])));
    assert!(local.iter().any(|p| near(*p, [10.0, 0.0])));
    // One rigid piece: the base point and the turn are free.
    let edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let solved = edit.solved().unwrap();
    assert_eq!(solved.status.dof, 3);
    assert!(
        solved.status.redundant.is_empty(),
        "{:?}",
        solved.status.redundant
    );
    // Its region is there as it was.
    let regions = edit.def.regions_of(&solved, &[]);
    assert_eq!(regions.len(), 1);
    // Sampled boundaries: within a per cent.
    let area = 20.0 * 6.0 + std::f64::consts::PI * 9.0;
    assert!(
        (regions[0].area - area).abs() < area * 0.01,
        "{}",
        regions[0].area
    );
}

#[test]
fn an_inserted_instance_has_the_blocks_exact_geometry() {
    let mut params = Parameters::default();
    let (def, block) = slot_block(&mut params);
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let id = edit
        .insert_block(&block, [50.0, 30.0], std::f64::consts::FRAC_PI_2)
        .unwrap();
    let (def, report) = edit.finish().unwrap();
    assert_eq!(report.status.dof, 6);
    let got = instance_points(&def, id, &block);
    let want = placed_points(&block, [50.0, 30.0], std::f64::consts::FRAC_PI_2);
    for (g, w) in got.iter().zip(&want) {
        assert!(near(*g, *w), "{g:?} {w:?}");
    }
}

#[test]
fn instances_are_placed_and_turned_by_constraints_and_follow_an_edited_block() {
    let mut params = Parameters::default();
    let (def, block) = slot_block(&mut params);
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let id = edit.insert_block(&block, [40.0, 10.0], 0.2).unwrap();
    // A fixed corner and a fixed horizontal line to place it by.
    let corner = edit.add_point([100.0, 20.0]);
    let a = edit.add_point([0.0, -50.0]);
    let b = edit.add_point([10.0, -50.0]);
    let line = edit.add_curve(EntityKind::Line {
        start: a,
        end: b,
        centerline: false,
    });
    edit.set_fixed(&[Ref::Point(corner), Ref::Curve(line)], true)
        .unwrap();
    let instance = edit.def.blocks.iter().find(|b| b.id == id).unwrap().clone();
    edit.add_constraint(ConstraintKind::Coincident {
        point: instance.base,
        entity: Ref::Point(corner),
    });
    edit.add_dimension(
        DimensionKind::Angle {
            a: line,
            b: instance.direction,
            reverse_a: None,
            reverse_b: None,
        },
        Some(&ValueInput::Number(FRAC_PI_6)),
        false,
        None,
    )
    .unwrap();
    let (def, report) = edit.finish().unwrap();
    assert!(
        report.status.redundant.is_empty(),
        "{:?}",
        report.status.redundant
    );
    let got = instance_points(&def, id, &block);
    let want = placed_points(&block, [100.0, 20.0], FRAC_PI_6);
    for (g, w) in got.iter().zip(&want) {
        assert!(near(*g, *w), "{g:?} {w:?}");
    }
    // The block edited: the slot 10 longer to the right.
    let mut longer = block.clone();
    for e in &mut longer.content.entities {
        if let EntityKind::Point { at } = &mut e.kind
            && at[0] > 5.0
        {
            at[0] += 10.0;
        }
    }
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    assert_eq!(edit.sync_block(&longer).unwrap(), 0);
    let (def, report) = edit.finish().unwrap();
    assert!(report.status.conflicts.is_empty());
    let got = instance_points(&def, id, &longer);
    let want = placed_points(&longer, [100.0, 20.0], FRAC_PI_6);
    for (g, w) in got.iter().zip(&want) {
        assert!(near(*g, *w), "{g:?} {w:?}");
    }
}

#[test]
fn a_constraint_on_geometry_the_edit_removed_is_lost_and_the_sketch_solves() {
    let mut params = Parameters::default();
    let (def, block) = slot_block(&mut params);
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let id = edit.insert_block(&block, [0.0, 40.0], 0.0).unwrap();
    let instance = edit.def.blocks.iter().find(|b| b.id == id).unwrap().clone();
    // A point on the instance's first line.
    let line = block
        .content
        .entities
        .iter()
        .find(|e| matches!(e.kind, EntityKind::Line { .. }) && !e.construction)
        .unwrap()
        .clone();
    let on = instance.entities[&line.as_ref()].uid();
    let free = edit.add_point([3.0, 43.0]);
    edit.add_constraint(ConstraintKind::Coincident {
        point: free,
        entity: Ref::Curve(on),
    });
    let (def, _) = edit.finish().unwrap();
    // The block without that line (and what holds on it).
    let mut less = block.clone();
    less.content.entities.retain(|e| e.id != line.id);
    less.content
        .constraints
        .retain(|c| !c.kind.refs().contains(&line.as_ref()));
    less.content
        .dimensions
        .retain(|d| !d.kind.refs().contains(&line.as_ref()));
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    assert_eq!(edit.sync_block(&less).unwrap(), 1);
    let (def, report) = edit.finish().unwrap();
    assert!(report.status.conflicts.is_empty());
    let instance = def.blocks.iter().find(|b| b.id == id).unwrap();
    assert_eq!(instance.lost.len(), 1);
    assert_eq!(instance.lost[0].what, "coincident");
    assert!(def.entity(on).is_none());
    assert!(def.entity(free).is_some());
}

#[test]
fn an_exploded_instance_is_plain_geometry_with_the_blocks_constraints() {
    let mut params = Parameters::default();
    let (def, block) = slot_block(&mut params);
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let id = edit.insert_block(&block, [0.0, 40.0], 0.5).unwrap();
    let (def, _) = edit.finish().unwrap();
    let points = instance_points(&def, id, &block);
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let left_out = edit.explode_block(id, &block).unwrap();
    assert_eq!(left_out, 0);
    let (def, report) = edit.finish().unwrap();
    assert!(def.blocks.iter().all(|b| b.id != id));
    assert!(report.status.conflicts.is_empty());
    // The block's constraints are the sketch's, and the geometry stayed.
    assert!(def.constraints.len() >= block.content.constraints.len());
    let index = EntityIndex::new(&def.entities);
    for p in points {
        assert!(
            def.entities
                .iter()
                .any(|e| e.is_point() && index.point(e.id).is_some_and(|q| near(q, p)))
        );
    }
    // Now it can be trimmed and dimensioned like any geometry.
    let line = def
        .entities
        .iter()
        .find(|e| {
            matches!(e.kind, EntityKind::Line { .. })
                && !e.construction
                && def.instance_of(e.id).is_none()
        })
        .unwrap()
        .id;
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    edit.add_dimension(DimensionKind::Length { line }, None, true, None)
        .unwrap();
    edit.finish().unwrap();
}

#[test]
fn an_instance_refuses_inner_dimensions_and_trims_in_words() {
    let mut params = Parameters::default();
    let (def, block) = slot_block(&mut params);
    let instance = def.blocks[0].clone();
    let line = block
        .content
        .entities
        .iter()
        .find(|e| matches!(e.kind, EntityKind::Line { .. }) && !e.construction)
        .unwrap();
    let on = instance.entities[&line.as_ref()].uid();
    let mut edit = SketchEdit::new(def.clone(), &mut params, FeatureUid(1), "Sketch1");
    edit.add_dimension(DimensionKind::Length { line: on }, None, true, None)
        .unwrap();
    let error = edit.finish().unwrap_err();
    assert!(error.contains("explode it first"), "{error}");
    let mut edit = SketchEdit::new(def.clone(), &mut params, FeatureUid(1), "Sketch1");
    let error = edit.trim(on, [10.0, 3.0]).unwrap_err();
    assert!(error.contains("explode it first"), "{error}");
    // A horizontal constraint turns it: allowed.
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    edit.add_constraint(ConstraintKind::Horizontal {
        line: instance.direction,
    });
    edit.finish().unwrap();
}

#[test]
fn a_delete_takes_the_instance_whole() {
    let mut params = Parameters::default();
    let (def, block) = slot_block(&mut params);
    let instance = def.blocks[0].clone();
    let line = block
        .content
        .entities
        .iter()
        .find(|e| matches!(e.kind, EntityKind::Line { .. }) && !e.construction)
        .unwrap();
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    edit.remove(&[Item::Entity(instance.entities[&line.as_ref()])])
        .unwrap();
    let (def, _) = edit.finish().unwrap();
    assert!(def.blocks.is_empty());
    assert!(def.entities.is_empty(), "{:?}", def.entities);
}

#[test]
fn projected_geometry_cannot_go_into_a_block() {
    let mut params = Parameters::default();
    let (mut def, curves) = slot_sketch(&mut params);
    def.entity_mut(curves[0]).unwrap().reference = true;
    let items = everything(&def);
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let error = edit
        .make_block(1, "Slot", &items, [0.0, 0.0], 0.0, true)
        .unwrap_err();
    assert!(error.contains("projected"), "{error}");
}

#[test]
fn a_block_nests_an_instance_and_follows_its_edit() {
    let mut params = Parameters::default();
    let (def, inner) = slot_block(&mut params);
    // A circle beside the slot's instance, both made block 2.
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let center = edit.add_point([40.0, 0.0]);
    edit.add_curve(EntityKind::Circle {
        center,
        radius: 4.0,
        start: 0.0,
    });
    let items = everything(&edit.def);
    let (outer, _) = edit
        .make_block(2, "Pair", &items, [0.0, 0.0], 0.0, true)
        .unwrap();
    let (def, report) = edit.finish().unwrap();
    assert_eq!(report.status.dof, 3);
    assert_eq!(def.blocks.len(), 1);
    assert_eq!(outer.instances.len(), 1);
    assert_eq!(outer.instances[0].block, 1);
    let blocks = vec![inner.clone(), outer.clone()];
    assert!(holds(&blocks, 2, 1));
    assert!(holds(&blocks, 1, 1));
    assert!(
        !holds(&blocks, 1, 2),
        "a block cannot hold one that holds it"
    );
    // The inner block made longer: the outer's content follows.
    let mut longer = inner.clone();
    for e in &mut longer.content.entities {
        if let EntityKind::Point { at } = &mut e.kind
            && at[0] > 5.0
        {
            at[0] += 10.0;
        }
    }
    let mut outer2 = outer.clone();
    assert!(sync_nested(&mut outer2, &longer).unwrap());
    // The slot's right centre, at (20, 0) of the sketch, is at (30, 0).
    let points: Vec<P2> = outer2
        .content
        .entities
        .iter()
        .filter_map(|e| match e.kind {
            EntityKind::Point { at } => Some(at),
            _ => None,
        })
        .collect();
    assert!(points.iter().any(|p| near(*p, [30.0, 0.0])), "{points:?}");
    assert!(!points.iter().any(|p| near(*p, [20.0, 0.0])), "{points:?}");
    // An instance of the outer block solves as one rigid piece.
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    edit.insert_block(&outer2, [0.0, 100.0], 0.0).unwrap();
    let (_, report) = edit.finish().unwrap();
    assert_eq!(report.status.dof, 6);
}

#[test]
fn a_block_edited_in_place_takes_what_the_edit_made() {
    let mut params = Parameters::default();
    let (def, block) = slot_block(&mut params);
    let id = def.blocks[0].id;
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    edit.open_block(id, &block, false).unwrap();
    // A line drawn while it is open joins the block.
    let a = edit.add_point([10.0, -10.0]);
    let b = edit.add_point([10.0, 10.0]);
    edit.add_curve(EntityKind::Line {
        start: a,
        end: b,
        centerline: false,
    });
    let closed = edit.close_block(id).unwrap();
    let (def, report) = edit.finish().unwrap();
    assert_eq!(report.status.dof, 3);
    assert_eq!(
        closed.content.entities.len(),
        block.content.entities.len() + 3
    );
    // The new line in the block's frame: through its base point.
    let made: Vec<P2> = closed
        .content
        .entities
        .iter()
        .filter_map(|e| match e.kind {
            EntityKind::Point { at } => Some(at),
            _ => None,
        })
        .collect();
    assert!(made.iter().any(|p| near(*p, [0.0, -10.0])));
    assert!(made.iter().any(|p| near(*p, [0.0, 10.0])));
    assert_eq!(
        closed.content.constraints.len(),
        block.content.constraints.len()
    );
    assert!(def.blocks[0].open.is_none());
    assert!(def.constraints.is_empty(), "{:?}", def.constraints);
}

#[test]
fn a_copied_instance_pastes_as_an_instance() {
    let mut params = Parameters::default();
    let (def, _) = slot_block(&mut params);
    let items = everything(&def);
    let fragment = super::clipboard::copy(&def, &params, FeatureUid(1), &items).unwrap();
    assert_eq!(fragment.instances.len(), 1);
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    edit.paste(&fragment, [0.0, 50.0]).unwrap();
    let (def, report) = edit.finish().unwrap();
    assert_eq!(def.blocks.len(), 2);
    assert_eq!(report.status.dof, 6);
    // Each region is a slot.
    let edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let solved = edit.solved().unwrap();
    assert!(
        solved
            .curves
            .values()
            .all(|c| !matches!(c, Curve2::Line { a, b } if a == b))
    );
}

#[test]
fn a_coincident_between_two_instances_base_points_stays_a_constraint() {
    // Both base points end a direction line, so a Coincident between them
    // would merge two curve ends; an instance's points are its own, so it
    // stays a constraint and both instances stay.
    let mut params = Parameters::default();
    let (def, block) = slot_block(&mut params);
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let a = edit.insert_block(&block, [40.0, 0.0], 0.0).unwrap();
    let b = edit.insert_block(&block, [40.0, 30.0], 0.0).unwrap();
    let base =
        |edit: &SketchEdit<'_>, id| edit.def.blocks.iter().find(|i| i.id == id).unwrap().base;
    let kind = ConstraintKind::Coincident {
        point: base(&edit, b),
        entity: Ref::Point(base(&edit, a)),
    };
    assert_eq!(edit.coincident_merge(&kind), None);
    edit.add_constraint(kind);
    let (def, _) = edit.finish().unwrap();
    assert!(def.blocks.iter().any(|i| i.id == a));
    assert!(def.blocks.iter().any(|i| i.id == b));
}
