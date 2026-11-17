// SPDX-License-Identifier: MIT
//! A design modelled with Y up stands on Z up, and keeps its length unit
//! (mitcad#1937): the block design with the named views and the unit a
//! design file gives (`document.named_views`, `default_length_units`).

use super::*;
use mitcad_import_replay::upright::{front_view, upright_turn};
use mitcad_model::features::SketchPlane;
use mitcad_model::reorient::{Turn, TurnAxis};

/// The standard views of a design modelled with `up` (`y` or `z`), around a
/// target at (2, 1, 0.5) cm.
fn views(up: &str) -> Value {
    let target = [2.0, 1.0, 0.5];
    let at = |d: [f64; 3]| [target[0] + d[0], target[1] + d[1], target[2] + d[2]];
    let view = |name: &str, eye: [f64; 3], up: [f64; 3]| json!({"name": name, "eye": at(eye), "target": target, "up": up});
    match up {
        // Front looks along -Z with +Y up.
        "y" => json!([
            view("TOP", [0.0, 100.0, 0.0], [0.0, 0.0, -1.0]),
            view("FRONT", [0.0, 0.0, 100.0], [0.0, 1.0, 0.0]),
            view("RIGHT", [100.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            view("HOME", [60.0, 60.0, 60.0], [0.0, 1.0, 0.0]),
        ]),
        // Front looks along +Y with +Z up.
        _ => json!([
            view("TOP", [0.0, 0.0, 100.0], [0.0, 1.0, 0.0]),
            view("FRONT", [0.0, -100.0, 0.0], [0.0, 0.0, 1.0]),
            view("RIGHT", [100.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            view("HOME", [60.0, -60.0, 60.0], [0.0, 0.0, 1.0]),
        ]),
    }
}

/// The block design with its document's views and unit.
fn block_modelled(up: &str, unit: &str) -> Dump {
    let mut value: Value = serde_json::to_value(block_dump()).unwrap();
    value["document"] = json!({"default_length_units": unit, "named_views": views(up)});
    dump(value)
}

fn upright() -> Options {
    Options {
        upright: true,
        ..Options::default()
    }
}

fn sketch_plane(doc: &Document<MockKernel>) -> SketchPlane {
    doc.features()
        .find_map(|f| match &f.def {
            FeatureDef::Sketch(s) => Some(s.plane.clone()),
            _ => None,
        })
        .expect("a sketch")
}

fn length_unit(doc: &Document<MockKernel>) -> String {
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    saved["units"]["length"].as_str().unwrap_or("mm").to_owned()
}

#[test]
fn the_front_view_tells_the_up_direction() {
    let y = block_modelled("y", "mm");
    assert_eq!(front_view(&y), Some(([0.0, 0.0, -1.0], [0.0, 1.0, 0.0])));
    // Y up: a quarter turn about X takes -Z onto +Y and +Y onto +Z.
    let (turn, up) = upright_turn(&y).expect("a turn");
    assert_eq!(turn, Turn::about(TurnAxis::X, 1));
    assert_eq!(up, [0.0, 1.0, 0.0]);
    // Z up: MitCad's own; no turn.
    assert_eq!(upright_turn(&block_modelled("z", "mm")), None);
    // No views, or a Front view off the axes: as modelled.
    assert_eq!(upright_turn(&block_dump()), None);
    let mut skew: Value = serde_json::to_value(&y).unwrap();
    skew["document"]["named_views"][1]["eye"] = json!([3.0, 1.0, 100.5]);
    assert_eq!(upright_turn(&dump(skew)), None);
}

#[test]
fn a_design_modelled_with_y_up_stands_on_z_up_in_its_unit() {
    let mut doc = Document::new(MockKernel::default());
    let report = import_design(
        &mut doc,
        &block_modelled("y", "in"),
        &mut NoGeometry,
        &upright(),
    );
    assert_eq!(report.items[1].outcome, Outcome::Parametric);
    let turned = report.upright.as_ref().expect("turned");
    assert!(turned.turned, "{turned:?}");
    assert_eq!(turned.up, "+Y");
    assert_eq!(turned.matrix, Turn::about(TurnAxis::X, 1).matrix());
    // The sketch on XY now lies on XZ, so the block's front faces Front.
    assert_eq!(sketch_plane(&doc), SketchPlane::Xz);
    assert_eq!(length_unit(&doc), "in");
    // One undo step with the import.
    let mut options = upright();
    options.undo_label = Some("Import block.f3d".into());
    let mut doc = Document::new(MockKernel::default());
    import_design(
        &mut doc,
        &block_modelled("y", "in"),
        &mut NoGeometry,
        &options,
    );
    assert_eq!(doc.undo_depth(), 1);
    assert!(doc.features().count() > 0);
}

#[test]
fn a_design_modelled_with_z_up_is_not_turned() {
    let mut doc = Document::new(MockKernel::default());
    let report = import_design(
        &mut doc,
        &block_modelled("z", "mm"),
        &mut NoGeometry,
        &upright(),
    );
    assert!(report.upright.is_none());
    assert_eq!(sketch_plane(&doc), SketchPlane::Xy);
    assert_eq!(length_unit(&doc), "mm");
}

#[test]
fn a_referred_document_keeps_its_coordinates() {
    // Without `upright` (the documents a design refers to) the design
    // stays as modelled, in its unit.
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &block_modelled("y", "cm"));
    assert!(report.upright.is_none());
    assert_eq!(sketch_plane(&doc), SketchPlane::Xy);
    assert_eq!(length_unit(&doc), "cm");
}
