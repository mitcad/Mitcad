// SPDX-License-Identifier: MIT
//! The .solidcraft import on the model's mock kernel: the structure the
//! import makes (timeline, parameters, sketches, components, placeholders).
//! Geometry (volumes, boxes, references by point) is checked with OCCT by
//! `tools/cli/solidcraft-test.cmake`. Contact sets, motion studies and
//! motion links, read through the api's queries, are tested in
//! `core/model-tests/imports` (mitcad#831).

use std::path::{Path, PathBuf};

use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureDef};
use serde_json::json;

use super::*;

fn design(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../solidcraft/tests/designs")
        .join(name)
}

fn import(name: &str, options: &SolidcraftOptions) -> (Document<MockKernel>, SolidcraftReport) {
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft(&mut doc, &design(name), options)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    (doc, report)
}

fn outcome(report: &SolidcraftReport, name: &str) -> Outcome {
    report
        .features
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no {name} in {:?}", report.features))
        .outcome
}

/// The timeline, parameters and sketches of a part: user parameters keep
/// their names and expressions, a dimension's parameter is its own
/// (`d1 = width`), a feature's input its slot's (`d3 = height`).
#[test]
fn a_part_comes_in_with_its_parameters_and_sketches() {
    let (doc, report) = import(
        "box-fillet-hole.solidcraft.json",
        &SolidcraftOptions::default(),
    );
    assert_eq!(report.format, 27);
    let names: Vec<&str> = report.features.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["Sketch1", "Extrude1", "Fillet1", "Sketch2", "Hole cut"]
    );
    assert_eq!(outcome(&report, "Sketch1"), Outcome::Parametric);
    assert_eq!(outcome(&report, "Extrude1"), Outcome::Parametric);
    assert_eq!(outcome(&report, "Sketch2"), Outcome::Parametric);
    assert_eq!(outcome(&report, "Hole cut"), Outcome::Parametric);
    let p = doc.parameters();
    let expr = |n: &str| {
        let id = p.find(n).unwrap_or_else(|| panic!("no parameter {n}"));
        p.get(id).unwrap().expression().to_owned()
    };
    assert_eq!(expr("width"), "40 mm");
    assert_eq!(expr("height"), "20 mm");
    assert_eq!(expr("d1"), "width");
    assert_eq!(expr("d3"), "height");
    assert_eq!(expr("d5"), "10 mm");
    // Parameters the dimensions and features own.
    let owner = |n: &str| p.owner(p.find(n).unwrap());
    let extrude = doc.features().find(|f| f.name == "Extrude1").unwrap().uid;
    assert_eq!(owner("d3"), Some(extrude));
    assert_eq!(owner("width"), None);
    let s1 = report
        .sketches
        .iter()
        .find(|s| s.name == "Sketch1")
        .unwrap();
    assert_eq!((s1.curves, s1.constraints, s1.dimensions), (4, 5, 2));
    assert_eq!((s1.constraints_mapped, s1.dimensions_mapped), (5, 2));
    assert!(s1.dropped.is_empty(), "{:?}", s1.dropped);
    // The sketch's dimensions are MitCad's, with the file's parameters.
    let sketch = doc.features().find(|f| f.name == "Sketch1").unwrap();
    let FeatureDef::Sketch(def) = &sketch.def else {
        panic!("Sketch1 is not a sketch")
    };
    assert_eq!(def.entities.len(), 9);
    assert_eq!(def.constraints.len(), 5);
    assert_eq!(def.dimensions.len(), 2);
    // The body has the file's name.
    let body = doc.bodies().into_iter().find(|b| b.name == "Block");
    assert!(
        body.is_some(),
        "{:?}",
        doc.bodies()
            .iter()
            .map(|b| b.name.clone())
            .collect::<Vec<_>>()
    );
}

/// A changed parameter recomputes the features that use it.
#[test]
fn a_changed_parameter_rebuilds() {
    let options = SolidcraftOptions {
        set_parameters: vec![("height".to_owned(), "35 mm".to_owned())],
        ..SolidcraftOptions::default()
    };
    let (doc, report) = import("box-fillet-hole.solidcraft.json", &options);
    assert_eq!(report.changed, [("height".to_owned(), "35 mm".to_owned())]);
    let p = doc.parameters();
    assert_eq!(p.value(p.find("d3").unwrap()), Some(35.0));
    assert!(
        import_solidcraft(
            &mut Document::new(MockKernel::default()),
            &design("box-fillet-hole.solidcraft.json"),
            &SolidcraftOptions {
                set_parameters: vec![("nothing".to_owned(), "1".to_owned())],
                ..SolidcraftOptions::default()
            }
        )
        .unwrap_err()
        .contains("nothing")
    );
}

/// An older format is upgraded first; components get their occurrences.
#[test]
fn an_old_format_comes_in_upgraded() {
    let (doc, report) = import("format-1.solidcraft.json", &SolidcraftOptions::default());
    assert_eq!(report.format, 1);
    assert!(report.upgrades.iter().any(|u| u == "5 to 6"));
    assert_eq!(report.components, 1);
    assert_eq!(report.occurrences, 1);
    // The old XZ plane is a plane of its own with the axes it had.
    let sketch = report
        .features
        .iter()
        .find(|f| f.name == "Sketch1")
        .unwrap();
    assert_eq!(sketch.outcome, Outcome::Parametric, "{:?}", sketch.notes);
    assert!(
        doc.features()
            .any(|f| f.def.type_name() == "construction_plane")
    );
}
/// Features without a MitCad counterpart (here options MitCad has not) are
/// named in the report and hold their place as suppressed placeholders;
/// the timeline goes on.
#[test]
fn unmapped_features_are_placeholders() {
    let v = json!({"format": "solidcraft/27", "name": "U", "params": [], "features": [
        {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "10", "width": "10", "height": "10",
         "body_names": ["B"]},
        {"id": 2, "name": "SurfaceTrim1", "type": "surface_trim", "body": "B", "with": "B", "keep": [5, 5, 5],
         "plane": {"type": "offset", "base": {"type": "origin", "name": "XY"}, "distance": "2"}},
        {"id": 3, "name": "SurfaceUntrim1", "type": "surface_untrim", "face": {"body": "B", "point": [5, 5, 10]},
         "holes": [[5, 5, 10]]},
        {"id": 4, "name": "Box2", "type": "box", "corner": [20, 0, 0], "length": "5", "width": "5", "height": "5"}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "u.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome(&report, "SurfaceTrim1"), Outcome::Unmapped);
    assert_eq!(outcome(&report, "SurfaceUntrim1"), Outcome::Unmapped);
    assert_eq!(outcome(&report, "Box2"), Outcome::Parametric);
    assert_eq!(
        report.unmapped,
        [
            ("SurfaceTrim".to_owned(), 1),
            ("SurfaceUntrim".to_owned(), 1)
        ]
    );
    let placeholder = doc.features().find(|f| f.name == "SurfaceTrim1").unwrap();
    assert!(placeholder.suppressed);
    assert!(
        doc.import_note(placeholder.uid)
            .unwrap()
            .contains("no MitCad counterpart")
    );
    // Every input is a parameter, the unmapped ones' too.
    assert!(
        report
            .parameters
            .iter()
            .any(|p| p.name == "d4" && p.error.is_none())
    );
}

/// A design of the newest format opens (mitcad#1331): the kinds of the
/// later formats MitCad has no feature for (a path flange, Repair Part;
/// a cone comes in as a Cone since mitcad#2239) and the options it has not (a hole placed by edge distances, a
/// coil of a sketch's profile) are placeholders with their reasons, and
/// the timeline goes on; a pattern names its seed by id. A frame member
/// is one Pipe or Sweep per line (mitcad#2157): this one's lines' sketch
/// is not in the file, so it fails with that reason.
#[test]
fn a_format_50_design_opens_with_placeholders_for_what_has_no_counterpart() {
    let v = json!({"format": "solidcraft/50", "name": "N50", "params": [], "features": [
        {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "40", "width": "40", "height": "10",
         "body_names": ["B"]},
        {"id": 2, "name": "Cone1", "type": "cone", "base": [60, 0, 0], "radius": "5", "top_radius": "0", "height": "10"},
        {"id": 3, "name": "Frame1", "type": "frame_member", "path_sketch": 9, "lines": ["l1"],
         "profile": {"source": "library", "profile": {"kind": "rect_tube", "width": 40, "height": 40, "wall": 3}}},
        {"id": 4, "name": "Repair1", "type": "repair", "body": "B", "gap": "0.1"},
        {"id": 5, "name": "Path1", "type": "sheet_path", "edge": [0, 0, 0], "sketch": 9, "curves": ["l1"]},
        {"id": 6, "name": "Hole1", "type": "hole", "position": [10, 10, 10], "direction": [0, 0, -1],
         "diameter": "3", "references": [{"edge": [0, 5, 10], "distance": "10"}]},
        {"id": 7, "name": "Coil1", "type": "coil", "base": [0, 0, 0], "sketch": 9, "pitch": "2", "turns": "3"},
        {"id": 8, "name": "Copies", "type": "pattern", "features": [1],
         "pattern": {"type": "rectangular", "dir1": [1, 0, 0], "count1": "2", "spacing1": "50"}}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    assert_eq!(file.saved_format, 50);
    // Only the steps of formats 51 to 57 (mitcad#1994), none of its own.
    assert!(
        !file.upgrades.iter().any(|u| u == "thread-nominal-size"),
        "{:?}",
        file.upgrades
    );
    assert!(
        file.upgrades
            .iter()
            .any(|u| u == "two-sided-extrude-fixed-sides"),
        "{:?}",
        file.upgrades
    );
    assert!(file.newer.is_none());
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "n.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    for name in ["Repair1", "Path1", "Hole1", "Coil1"] {
        assert_eq!(outcome(&report, name), Outcome::Unmapped, "{name}");
    }
    assert_eq!(outcome(&report, "Frame1"), Outcome::Failed);
    let frame = doc.features().find(|f| f.name == "Frame1").unwrap();
    assert!(
        doc.import_note(frame.uid)
            .unwrap()
            .contains("the lines' sketch did not come in"),
        "{:?}",
        doc.import_note(frame.uid)
    );
    assert_eq!(outcome(&report, "Box1"), Outcome::Parametric);
    assert_eq!(outcome(&report, "Copies"), Outcome::Parametric);
    // A cone is MitCad's Cone since mitcad#2239.
    assert_eq!(outcome(&report, "Cone1"), Outcome::Parametric);
}

/// A SolidCraft drawing is a file of its own: refused as a drawing.
#[test]
fn a_drawing_is_refused_as_one() {
    let e = mitcad_solidcraft::SolidcraftFile::from_value(
        json!({"format": "solidcraft-drawing/1", "sheets": [], "sources": []}),
    )
    .unwrap_err();
    assert!(e.contains("drawing"), "{e}");
}

/// A design whose expected values disagree fails the check.
#[test]
fn the_check_compares_with_expected_values() {
    let options = SolidcraftOptions {
        expected: Some(json!({"timeline": [{"name": "Sketch1"}], "body_count": 7})),
        ..SolidcraftOptions::default()
    };
    let (_, report) = import("box-fillet-hole.solidcraft.json", &options);
    let check = report.check.unwrap();
    assert!(!check.passed);
    assert_eq!(check.failures.len(), 2, "{:?}", check.failures);
}

/// `"valid": true` checks every body with the kernel's checker
/// (mitcad#2175): only then is validity measured; a body the kernel finds
/// not valid, or cannot check (the mock kernel), fails the check.
#[test]
fn the_check_asks_for_valid_bodies() {
    let (_, plain) = import(
        "box-fillet-hole.solidcraft.json",
        &SolidcraftOptions::default(),
    );
    assert!(!plain.bodies.is_empty());
    assert!(plain.bodies.iter().all(|b| b.valid.is_none()));
    let options = SolidcraftOptions {
        expected: Some(json!({"valid": true})),
        ..SolidcraftOptions::default()
    };
    let (_, report) = import("box-fillet-hole.solidcraft.json", &options);
    let asked = report.check.clone().unwrap();
    assert!(!asked.passed);
    assert!(
        asked
            .failures
            .iter()
            .all(|f| f.contains("validity was not checked")),
        "{:?}",
        asked.failures
    );
    // Measured by a kernel that checks: one body not valid fails, all
    // valid pass.
    let mut measured = report.clone();
    for b in &mut measured.bodies {
        b.valid = Some(true);
    }
    assert!(check(&measured, &json!({"valid": true})).passed);
    measured.bodies[0].valid = Some(false);
    let failed = check(&measured, &json!({"valid": true}));
    assert!(!failed.passed);
    assert_eq!(
        failed.failures,
        [format!("{}: not valid", measured.bodies[0].name)]
    );
    assert!(measured.text().contains(", not valid"));
}

/// Expressions keep their meaning in MitCad's language.
#[test]
fn expressions_are_translated() {
    let v = json!({"format": "solidcraft/27", "name": "E",
        "params": [{"name": "a", "expr": "2,5 mm", "unit": "mm"},
                   {"name": "E", "expr": "hypot(3, 4) mm", "unit": "mm"},
                   {"name": "n", "expr": "3", "unit": ""}],
        "features": []});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "e.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    let p = doc.parameters();
    assert_eq!(p.value(p.find("a").unwrap()), Some(2.5));
    // `E` is MitCad's constant: the parameter is `E_`.
    assert_eq!(p.value(p.find("E_").unwrap()), Some(5.0));
    assert_eq!(p.value(p.find("n").unwrap()), Some(3.0));
    let e = report.parameters.iter().find(|r| r.name == "E").unwrap();
    assert_eq!(e.renamed.as_deref(), Some("E_"));
}

/// Embosses come in as MitCad's (mitcad#458): a deboss sinks, one without
/// a face starts from the sketch plane on its targets, one on a wall names
/// the face the import finds.
#[test]
fn embosses_come_in() {
    let (doc, report) = import("emboss.solidcraft.json", &SolidcraftOptions::default());
    assert!(report.unmapped.is_empty(), "{:?}", report.unmapped);
    use mitcad_model::features::{EmbossDef, EmbossMode};
    let def = |name: &str| -> EmbossDef {
        let f = doc.features().find(|f| f.name == name).unwrap();
        match &f.def {
            FeatureDef::Emboss(e) => e.clone(),
            other => panic!("{name}: {other:?}"),
        }
    };
    assert_eq!(outcome(&report, "Emboss3"), Outcome::Parametric);
    let top = def("Emboss3");
    assert!(top.face.is_none());
    assert_eq!(top.mode, EmbossMode::Raise);
    assert_eq!(top.participants.len(), 1);
    for (name, mode) in [
        ("Emboss1", EmbossMode::Raise),
        ("Emboss2", EmbossMode::Sink),
    ] {
        let f = report.features.iter().find(|f| f.name == name).unwrap();
        assert_ne!(f.outcome, Outcome::Unmapped, "{name}: {:?}", f.notes);
        if f.outcome != Outcome::Failed {
            assert_eq!(def(name).mode, mode);
        }
    }
}

/// Bounding solids come in as MitCad's `bounding_solid` features
/// (mitcad#460): the bodies the file names (none: every body), the margin
/// its input's parameter (`gap`, or 0 when the file has none), and the
/// box named as SolidCraft names it (the stored name, else `Bounding`).
#[test]
fn bounding_solids_come_in() {
    let (doc, report) = import(
        "bounding-solid.solidcraft.json",
        &SolidcraftOptions::default(),
    );
    for name in ["Stock box", "Lug box", "Envelope"] {
        assert_eq!(outcome(&report, name), Outcome::Parametric, "{name}");
    }
    assert!(report.unmapped.is_empty(), "{:?}", report.unmapped);
    let def = |name: &str| {
        let entry = doc.features().find(|f| f.name == name).unwrap();
        let FeatureDef::BoundingSolid(def) = &entry.def else {
            panic!("{name} is not a bounding solid")
        };
        def.clone()
    };
    let p = doc.parameters();
    let expr = |id: mitcad_model::ParamId| p.get(id).unwrap().expression().to_owned();
    let stock = def("Stock box");
    assert_eq!(stock.bodies.len(), 2);
    assert_eq!(expr(stock.margin), "gap");
    let lug = def("Lug box");
    assert_eq!(lug.bodies.len(), 1);
    assert_eq!(p.value(lug.margin), Some(0.0));
    let envelope = def("Envelope");
    assert!(envelope.bodies.is_empty());
    assert_eq!(p.value(envelope.margin), Some(1.0));
    let mut names: Vec<String> = doc.bodies().iter().map(|b| b.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Block", "Bounding", "Bounding (2)", "Lug", "Stock"]);
}

/// Boundary fills come in as `boundary_fill` features (mitcad#457): their
/// new bodies named as the file names them, the tools they remove gone,
/// a cut's participants the feature's.
#[test]
fn boundary_fills_come_in() {
    let (doc, report) = import(
        "boundary-fill.solidcraft.json",
        &SolidcraftOptions::default(),
    );
    for name in ["CoreFill", "SidesFill", "SlotsFill"] {
        assert_eq!(outcome(&report, name), Outcome::Parametric, "{name}");
    }
    assert!(report.unmapped.is_empty(), "{:?}", report.unmapped);
    let mut names: Vec<String> = doc.bodies().iter().map(|b| b.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Core", "Left", "Plate", "Right"]);
    let fill = |name: &str| {
        let entry = doc.features().find(|f| f.name == name).unwrap();
        let FeatureDef::BoundaryFill(def) = &entry.def else {
            panic!("{name} is no boundary fill: {:?}", entry.def);
        };
        def.clone()
    };
    let sides = fill("SidesFill");
    assert_eq!(sides.cells, [[5.0, 5.0, 5.0], [25.0, 25.0, 25.0]]);
    assert!(sides.remove_tools);
    assert!(sides.participants.is_empty());
    let slots = fill("SlotsFill");
    let plate = doc.bodies().iter().find(|b| b.name == "Plate").unwrap().uid;
    assert_eq!(slots.participants, [plate]);
    assert!(!fill("CoreFill").remove_tools);
}

/// Convert Mesh (mitcad#462): a mesh import's body converted by MitCad's
/// `mesh_convert`, its tolerance the input's parameter (`d1 = tol`); a
/// conversion of a body that is no mesh body fails and is a placeholder.
#[test]
fn a_mesh_conversion_comes_in_as_convert_mesh() {
    let (doc, report) = import(
        "mesh-convert.solidcraft.json",
        &SolidcraftOptions::default(),
    );
    assert_eq!(outcome(&report, "Mesh1"), Outcome::Parametric);
    assert_eq!(outcome(&report, "ConvertMesh1"), Outcome::Parametric);
    assert!(report.unmapped.is_empty(), "{:?}", report.unmapped);
    let convert = doc
        .features()
        .find(|f| f.name == "ConvertMesh1")
        .expect("ConvertMesh1");
    let FeatureDef::MeshConvert(def) = &convert.def else {
        panic!("{:?}", convert.def)
    };
    assert!(doc.status(convert.uid).is_some_and(|s| s.is_ok()));
    let p = doc.parameters();
    assert_eq!(p.name(def.tolerance), "d1");
    assert_eq!(p.get(def.tolerance).unwrap().expression(), "tol");
    let value = p.value(def.tolerance).unwrap();
    assert!((value - 0.1).abs() < 1e-12, "{value}");
    // The body keeps the file's name.
    assert!(
        doc.bodies()
            .iter()
            .any(|b| b.name == "Plate" && b.uid == def.body)
    );

    let v = json!({"format": "solidcraft/27", "name": "C", "params": [], "features": [
        {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "10", "width": "10",
         "height": "10", "body_names": ["B"]},
        {"id": 2, "name": "ConvertMesh1", "type": "mesh_convert", "body": "B", "tolerance": "0.05"}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "c.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome(&report, "Box1"), Outcome::Parametric);
    assert_eq!(outcome(&report, "ConvertMesh1"), Outcome::Failed);
}

/// Patch, Stitch and Thicken come in as MitCad's (mitcad#456): a patch of
/// sketch regions makes a surface body of the file's name, a thicken new
/// bodies (the surface stays), a stitch keeps the first body's name; a
/// crown is noted as left out.
#[test]
fn surface_features_come_in() {
    let rectangle = |x: f64| {
        json!({"points": [
                {"id": "origin", "pos": [0.0, 0.0], "fixed": true},
                {"id": "p1", "pos": [x, 0.0], "fixed": true},
                {"id": "p2", "pos": [x + 20.0, 0.0], "fixed": true},
                {"id": "p3", "pos": [x + 20.0, 10.0], "fixed": true},
                {"id": "p4", "pos": [x, 10.0], "fixed": true}],
            "curves": [
                {"id": "l1", "type": "line", "a": 1, "b": 2},
                {"id": "l2", "type": "line", "a": 2, "b": 3},
                {"id": "l3", "type": "line", "a": 3, "b": 4},
                {"id": "l4", "type": "line", "a": 4, "b": 1}],
            "constraints": []})
    };
    let v = json!({"format": "solidcraft/27", "name": "S", "params": [], "features": [
        {"id": 1, "name": "Sketch1", "type": "sketch", "plane": {"type": "origin", "name": "XY"},
         "sketch": rectangle(0.0)},
        {"id": 2, "name": "Patch1", "type": "patch", "body_names": ["Sheet"], "sketch": 1,
         "profiles": {"type": "all"}},
        {"id": 3, "name": "Sketch2", "type": "sketch", "plane": {"type": "origin", "name": "XY"},
         "sketch": rectangle(40.0)},
        {"id": 4, "name": "Patch2", "type": "patch", "body_names": ["Other"], "sketch": 3,
         "profiles": {"type": "all"}, "crown": 0.5},
        {"id": 5, "name": "Thicken1", "type": "thicken", "body_names": ["Plate"], "bodies": ["Sheet"],
         "thickness": "2 mm", "symmetric": true, "operation": "new_body"},
        {"id": 6, "name": "Stitch1", "type": "stitch", "bodies": ["Sheet", "Other"], "tolerance": "0.01 mm"}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "s.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    for name in ["Patch1", "Thicken1", "Stitch1"] {
        assert_eq!(outcome(&report, name), Outcome::Parametric, "{name}");
    }
    assert_eq!(outcome(&report, "Patch2"), Outcome::Partial);
    let patch2 = report.features.iter().find(|f| f.name == "Patch2").unwrap();
    assert!(patch2.notes[0].contains("crown"), "{:?}", patch2.notes);
    assert!(report.unmapped.is_empty(), "{:?}", report.unmapped);
    let types: Vec<&str> = doc.features().map(|f| f.def.type_name()).collect();
    for t in ["patch", "thicken", "stitch"] {
        assert!(types.contains(&t), "{types:?}");
    }
    // The stitch consumed Other into Sheet; the thickened Plate is new.
    let mut names: Vec<String> = doc.bodies().iter().map(|b| b.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Plate", "Sheet"]);
    let thicken = doc.features().find(|f| f.name == "Thicken1").unwrap();
    let FeatureDef::Thicken(def) = &thicken.def else {
        panic!("Thicken1 is not a thicken")
    };
    assert!(def.symmetric);
    // Its thickness is the file's input, a parameter.
    assert_eq!(doc.parameters().value(def.thickness), Some(2.0));
}

/// A sweep's end scale and second rail and a pattern along a chain of
/// curves come in as MitCad's options (mitcad#464); a spline fillet
/// radius is no placeholder any more (its edge is found by its pick, which
/// the mock kernel does not place; OCCT's run is `cli.import_solidcraft`).
#[test]
fn spline_radii_end_scales_two_rails_and_chains_come_in() {
    let (doc, report) = import(
        "fillet-sweep-pattern-options.solidcraft.json",
        &SolidcraftOptions::default(),
    );
    assert_ne!(outcome(&report, "Fillet1"), Outcome::Unmapped);
    for name in ["Sweep1", "Sweep2", "Pattern1"] {
        assert_eq!(outcome(&report, name), Outcome::Parametric, "{name}");
    }
    assert!(report.unmapped.is_empty(), "{:?}", report.unmapped);
    let def = |name: &str| {
        doc.features()
            .find(|f| f.name == name)
            .map(|f| f.def.clone())
            .unwrap()
    };
    assert!(matches!(def("Sweep1"), FeatureDef::Sweep(s) if s.end_scale.is_some()));
    assert!(matches!(def("Sweep2"), FeatureDef::Sweep(s) if s.guide_rail2.is_some()));
    // The rails' points where they cross the plane square to the path
    // (mitcad#469; the volume of a rail where it matters is checked in
    // `cli.import_solidcraft`, design sweep-rail-plane).
    assert!(matches!(def("Sweep2"),
        FeatureDef::Sweep(s) if s.rail_point == mitcad_model::RailPoint::Plane));
    assert!(matches!(def("Sweep1"),
        FeatureDef::Sweep(s) if s.rail_point == mitcad_model::RailPoint::Fraction));
    let FeatureDef::PathPattern(pattern) = def("Pattern1") else {
        panic!("Pattern1 is not a path pattern");
    };
    let path = format!("{:?}", pattern.path);
    assert!(path.starts_with("SketchCurves"), "{path}");
    let p = doc.parameters();
    let scale = p.find("taper").unwrap();
    assert_eq!(p.value(scale), Some(0.5));
}

/// Surface extrude, revolve, sweep and loft come in as the solid features'
/// surface mode (mitcad#455): every one parametric, a surface body of each
/// (two of the extrude: its profile's walls and its curves'), named as the
/// file names them.
#[test]
fn surface_sweeps_come_in() {
    let (doc, report) = import(
        "surface-sweeps.solidcraft.json",
        &SolidcraftOptions::default(),
    );
    for name in ["Walls", "Drum", "Swept", "Skin"] {
        assert_eq!(outcome(&report, name), Outcome::Parametric, "{name}");
    }
    assert!(report.unmapped.is_empty(), "{:?}", report.unmapped);
    let mut names: Vec<String> = doc.bodies().iter().map(|b| b.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Drum", "Fence", "Skin", "Swept", "Tube"]);
    let surface = |name: &str| match &doc.features().find(|f| f.name == name).unwrap().def {
        FeatureDef::Extrude(d) => d.surface,
        FeatureDef::Revolve(d) => d.surface,
        FeatureDef::Sweep(d) => d.surface,
        FeatureDef::Loft(d) => d.surface,
        other => panic!("{name} is a {}", other.type_name()),
    };
    for name in ["Walls", "Drum", "Swept", "Skin"] {
        assert!(surface(name), "{name}");
    }
}

/// Offset Surface comes in as `offset_surface` (mitcad#455): every face of
/// the body without picks; an extend up to an object is not mapped, with
/// the reason.
#[test]
fn surface_offsets_come_in() {
    let v = json!({"format": "solidcraft/27", "name": "O", "params": [], "features": [
        {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "10", "width": "10", "height": "10",
         "body_names": ["B"]},
        {"id": 2, "name": "Offset1", "type": "surface_offset", "body": "B", "distance": "1 mm",
         "body_names": ["Skin"], "chain": true},
        {"id": 3, "name": "Extend1", "type": "surface_extend", "body": "Skin", "edges": [[0, 0, 11]],
         "distance": "1", "up_to": "bounds"}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "o.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome(&report, "Offset1"), Outcome::Parametric);
    assert_eq!(outcome(&report, "Extend1"), Outcome::Unmapped);
    let offset = doc.features().find(|f| f.name == "Offset1").unwrap();
    let FeatureDef::OffsetSurface(def) = &offset.def else {
        panic!("Offset1 is not an offset surface")
    };
    assert!(def.chain && def.faces.is_empty());
    assert!(doc.bodies().iter().any(|b| b.name == "Skin"));
}

/// Trim and Reverse Normal come in as `trim_surface` and `reverse_normal`
/// (mitcad#455), with the trim's keep point; a trim's "trim with" option and an untrim of some of a face's holes are
/// not mapped, with the reason.
#[test]
fn surface_trims_come_in() {
    let v = json!({"format": "solidcraft/27", "name": "T", "params": [], "features": [
        {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "10", "width": "10", "height": "10",
         "body_names": ["B"]},
        {"id": 2, "name": "Offset1", "type": "surface_offset", "body": "B", "distance": "1 mm",
         "body_names": ["Skin"], "chain": true},
        {"id": 3, "name": "Trim1", "type": "surface_trim", "body": "Skin",
         "plane": {"type": "offset", "base": {"type": "origin", "name": "XY"}, "distance": "4 mm"},
         "keep": [5, 5, 10]},
        {"id": 4, "name": "Trim2", "type": "surface_trim", "body": "Skin",
         "plane": {"type": "origin", "name": "YZ"}, "keep": [5, 5, 10], "with": "mutual"},
        {"id": 5, "name": "Flip1", "type": "reverse_normal", "body": "Skin"},
        {"id": 6, "name": "Untrim1", "type": "surface_untrim", "face": {"body": "Skin", "point": [5, 5, 11]},
         "holes": [[5, 5, 11]]}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "t.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome(&report, "Trim1"), Outcome::Parametric);
    assert_eq!(outcome(&report, "Flip1"), Outcome::Parametric);
    assert_eq!(outcome(&report, "Trim2"), Outcome::Unmapped);
    assert_eq!(outcome(&report, "Untrim1"), Outcome::Unmapped);
    let trim = doc.features().find(|f| f.name == "Trim1").unwrap();
    let FeatureDef::TrimSurface(def) = &trim.def else {
        panic!("Trim1 is not a trim")
    };
    assert_eq!(def.keep, [[5.0, 5.0, 10.0]]);
    let flip = doc.features().find(|f| f.name == "Flip1").unwrap();
    assert!(matches!(&flip.def, FeatureDef::ReverseNormal(d) if d.faces.is_empty()));
}

/// Boundary patches, networks and fairings come in as `boundary_surface`
/// and `fair_surface` (mitcad#455): four curves a loop, two a loft of
/// them, a network of curves each way; a crowned patch is not mapped.
#[test]
fn boundary_surfaces_and_fairings_come_in() {
    let sketch = json!({
        "points": [
            {"id": "origin", "pos": [0.0, 0.0], "fixed": true},
            {"id": "p1", "pos": [0.0, 0.0], "fixed": true}, {"id": "p2", "pos": [10.0, 0.0], "fixed": true},
            {"id": "p3", "pos": [10.0, 10.0], "fixed": true}, {"id": "p4", "pos": [0.0, 10.0], "fixed": true}],
        "curves": [
            {"id": "l1", "type": "line", "a": 1, "b": 2}, {"id": "l2", "type": "line", "a": 2, "b": 3},
            {"id": "l3", "type": "line", "a": 3, "b": 4}, {"id": "l4", "type": "line", "a": 4, "b": 1}],
        "constraints": []});
    let curve = |c: &str| json!({"sketch": 1, "curves": [c]});
    let v = json!({"format": "solidcraft/27", "name": "N", "params": [], "features": [
        {"id": 1, "name": "Square", "type": "sketch", "plane": {"type": "origin", "name": "XY"}, "sketch": sketch},
        {"id": 2, "name": "Lid", "type": "surface_boundary", "sketch": 1, "curves": ["l1", "l2", "l3", "l4"],
         "body_names": ["Lid"]},
        {"id": 3, "name": "Bridge", "type": "surface_boundary", "sketch": 1, "curves": ["l1", "l3"],
         "orders": ["g1", "g2"], "body_names": ["Bridge"]},
        {"id": 4, "name": "Dome", "type": "surface_boundary", "sketch": 1, "curves": ["l1", "l3"], "crown": 0.5},
        {"id": 5, "name": "Net", "type": "surface_network", "u": [curve("l1"), curve("l3")],
         "v": [curve("l2"), curve("l4")], "body_names": ["Net"]},
        {"id": 6, "name": "Smooth", "type": "surface_fair", "body": "Net", "tolerance": "0.2 mm",
         "iterations": 3, "keep_edges": false}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "n.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    for name in ["Lid", "Bridge", "Net", "Smooth"] {
        assert_eq!(outcome(&report, name), Outcome::Parametric, "{name}");
    }
    assert_eq!(outcome(&report, "Dome"), Outcome::Unmapped);
    let def = |name: &str| {
        doc.features()
            .find(|f| f.name == name)
            .map(|f| f.def.clone())
            .unwrap()
    };
    assert!(matches!(def("Lid"), FeatureDef::BoundarySurface(d) if d.boundary.len() == 4));
    assert!(
        matches!(def("Bridge"), FeatureDef::BoundarySurface(d) if d.u.len() == 2 && d.v.is_empty())
    );
    assert!(
        matches!(def("Net"), FeatureDef::BoundarySurface(d) if d.u.len() == 2 && d.v.len() == 2)
    );
    let FeatureDef::FairSurface(fair) = def("Smooth") else {
        panic!("Smooth is not a fairing")
    };
    assert!(fair.iterations == 3 && !fair.keep_edges && fair.faces.is_empty());
    assert_eq!(doc.parameters().value(fair.tolerance), Some(0.2));
}

/// A Join To Object of a design before format 40 keeps the body it runs to
/// apart (mitcad#1603): when that body is its only target, the one body of
/// its component or the one it names, the tool joins nothing and comes in
/// as a new body of its own, with a note, so the design has two bodies.
#[test]
fn a_join_keeping_its_only_target_apart_is_a_new_body() {
    use mitcad_model::features::{Extent, ExtentObject, Operation as ModelOperation};
    for targets in [json!([]), json!(["Body1"])] {
        let sketch = json!({
            "points": [
                {"id": "origin", "pos": [0.0, 0.0], "fixed": true},
                {"id": "p1", "pos": [2.0, 2.0], "fixed": true},
                {"id": "p2", "pos": [4.0, 2.0], "fixed": true},
                {"id": "p3", "pos": [4.0, 4.0], "fixed": true},
                {"id": "p4", "pos": [2.0, 4.0], "fixed": true}],
            "curves": [
                {"id": "l1", "type": "line", "a": 1, "b": 2},
                {"id": "l2", "type": "line", "a": 2, "b": 3},
                {"id": "l3", "type": "line", "a": 3, "b": 4},
                {"id": "l4", "type": "line", "a": 4, "b": 1}],
            "constraints": []});
        // A box from z 0 to 10; a square sketched at z 20 extruded down to
        // the box's top face.
        let v = json!({"format": "solidcraft/39", "name": "J", "params": [], "features": [
            {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "10", "width": "10",
             "height": "10", "body_names": ["Body1"]},
            {"id": 2, "name": "Sketch1", "type": "sketch",
             "plane": {"type": "offset", "base": {"type": "origin", "name": "XY"}, "distance": "20"},
             "sketch": sketch},
            {"id": 3, "name": "Extrude1", "type": "extrude", "sketch": 2, "profiles": {"type": "all"},
             "extent": {"distance": "10", "to": [3, 3, 10]}, "operation": "join", "targets": targets.clone()}]});
        let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
        let mut doc = Document::new(MockKernel::default());
        // The box's faces on its bounds, so that the extent's point finds
        // the top face (and the body it belongs to).
        doc.kernel().box_faces.set(true);
        let report = import_solidcraft_file(
            &mut doc,
            file,
            "j.solidcraft",
            &SolidcraftOptions::default(),
        )
        .unwrap();
        assert_eq!(outcome(&report, "Box1"), Outcome::Parametric, "{targets}");
        let extrude = report
            .features
            .iter()
            .find(|f| f.name == "Extrude1")
            .unwrap();
        assert_eq!(
            extrude.outcome,
            Outcome::Partial,
            "{targets}: {:?}",
            extrude.notes
        );
        assert!(
            extrude.notes.iter().any(|n| n.contains("a new body")),
            "{targets}: {:?}",
            extrude.notes
        );
        let entry = doc.features().find(|f| f.name == "Extrude1").unwrap();
        let FeatureDef::Extrude(def) = &entry.def else {
            panic!("Extrude1 is not an extrude: {:?}", entry.def)
        };
        assert!(
            matches!(&def.extent, Extent::ToObject { object, .. }
                if matches!(**object, ExtentObject::Face { .. })),
            "{targets}: {:?}",
            def.extent
        );
        assert_eq!(def.operation, ModelOperation::NewBody, "{targets}");
        assert!(def.participants.is_empty(), "{targets}");
        assert!(
            doc.status(entry.uid).is_some_and(|s| s.is_ok()),
            "{targets}"
        );
        let names: Vec<String> = doc.bodies().iter().map(|b| b.name.clone()).collect();
        assert_eq!(names.len(), 2, "{targets}: {names:?}");
        assert!(names.iter().any(|n| n == "Body1"), "{targets}: {names:?}");
    }
}

/// A modelled thread runs its grooves out (mitcad#1302): SolidCraft runs
/// its groove a pitch past an end of the face, a cone there too, and
/// leaves no flat lip. A cosmetic thread has no ends to cut.
#[test]
fn a_modelled_thread_runs_out() {
    let modelled = features::thread_def("F2.b0", "F2:side0", "M8", true, "high_end");
    assert_eq!(modelled["type"], "thread", "{modelled}");
    assert_eq!(modelled["modeled"], true, "{modelled}");
    assert_eq!(modelled["ends"], "run_out", "{modelled}");
    assert_eq!(
        modelled["faces"],
        json!([{"body": "F2.b0", "face": "F2:side0"}]),
        "{modelled}"
    );
    assert_eq!(
        modelled["thread"],
        json!({"designation": "M8"}),
        "{modelled}"
    );
    assert_eq!(modelled["location"], "high_end", "{modelled}");
    let cosmetic = features::thread_def("F2.b0", "F2:side0", "M8", false, "low_end");
    assert_eq!(cosmetic["modeled"], false, "{cosmetic}");
    assert!(cosmetic.get("ends").is_none(), "{cosmetic}");
}

/// The definition a feature of the import became, as its debug text.
fn def_of(doc: &Document<MockKernel>, name: &str) -> String {
    let entry = doc
        .features()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no feature {name}"));
    format!("{:?}", entry.def)
}

/// A design SolidCraft 0.43.0 saves (format 57, mitcad#1994) opens with
/// only the steps of formats 58 to 63 to run (none of which changes it)
/// and no warning: a two-sided extrude of
/// entered distances is MitCad's signed one (Direction fixes the sides, a
/// sign turns its own side over), a revolve's fixed axis whose origin is a
/// parameter is fixed where it puts it now, a revolve about another
/// sketch's line takes that line, a box placed by its center is MitCad's
/// box from its corner, its height both ways.
#[test]
fn a_format_57_design_opens_as_it_is() {
    use mitcad_model::features::Extent;
    let (doc, report) = import(
        "format-57-holes-threads-revolve.solidcraft.json",
        &SolidcraftOptions::default(),
    );
    assert_eq!(report.format, 57);
    assert!(report.newer.is_none());
    assert_eq!(
        report.upgrades,
        [
            "extrude-from-slanted-face",
            "sketch-reference-placements",
            "surface-sweep-exact-spline-path",
            "sheet-edge-names",
            "sheet-face-names",
            "sheet-hem-gap",
            "sheet-surface-convert"
        ],
        "{:?}",
        report.upgrades
    );
    assert_eq!(outcome(&report, "Slab"), Outcome::Parametric);
    let slab = doc.features().find(|f| f.name == "Slab").unwrap();
    let FeatureDef::Extrude(extrude) = &slab.def else {
        panic!("Slab is not an extrude: {:?}", slab.def)
    };
    assert!(
        matches!(extrude.extent, Extent::TwoSides { signed: true, .. }),
        "{:?}",
        extrude.extent
    );
    assert!(!extrude.flip);
    let ring = report.features.iter().find(|f| f.name == "Ring").unwrap();
    assert_eq!(ring.outcome, Outcome::Partial, "{:?}", ring.notes);
    assert!(
        ring.notes
            .iter()
            .any(|n| n.contains("axis is fixed where its expressions put it now")),
        "{:?}",
        ring.notes
    );
    let ring_def = def_of(&doc, "Ring");
    assert!(ring_def.contains("[60.0, 0.0, 0.0]"), "{ring_def}");
    assert_eq!(outcome(&report, "Collar"), Outcome::Parametric);
    let cube = def_of(&doc, "Cube");
    assert!(cube.contains("symmetric: true"), "{cube}");
    assert!(cube.contains("95.0") && cube.contains("-5.0"), "{cube}");
    // A tapped hole's tolerance class is a drawing's: nothing of it here.
    let tapped = report.features.iter().find(|f| f.name == "Tapped").unwrap();
    assert!(
        !tapped.notes.iter().any(|n| n.contains("6H")),
        "{:?}",
        tapped.notes
    );
}

/// The same two-sided extrude in a design before format 51 keeps its older
/// reading (`second_follows_first`): the first side goes where its sign
/// sends it and the second to the other side of it, MitCad's unsigned
/// sides with the extrude flipped.
#[test]
fn an_older_two_sided_extrude_keeps_its_sides() {
    use mitcad_model::features::Extent;
    let mut v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(design("format-57-holes-threads-revolve.solidcraft.json"))
            .unwrap(),
    )
    .unwrap();
    v["format"] = json!("solidcraft/50");
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    assert!(
        file.upgrades
            .iter()
            .any(|u| u == "two-sided-extrude-fixed-sides")
    );
    let mut doc = Document::new(MockKernel::default());
    import_solidcraft_file(
        &mut doc,
        file,
        "old.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    let slab = doc.features().find(|f| f.name == "Slab").unwrap();
    let FeatureDef::Extrude(extrude) = &slab.def else {
        panic!("Slab is not an extrude: {:?}", slab.def)
    };
    assert!(
        matches!(extrude.extent, Extent::TwoSides { signed: false, .. }),
        "{:?}",
        extrude.extent
    );
    assert!(
        extrude.flip,
        "a negative first side goes against the normal"
    );
}

/// A design a newer SolidCraft saved (format 64, past 0.45.0's 63) opens
/// with a warning that names its format and says features may build
/// differently; a feature of a kind this import does not know keeps its
/// place as a placeholder with the reason, and the section and the
/// parameter it cannot read are left out and named (mitcad#1994,
/// mitcad#2153).
#[test]
fn a_newer_design_opens_with_a_warning() {
    let (doc, report) = import(
        "format-64-newer.solidcraft.json",
        &SolidcraftOptions::default(),
    );
    assert_eq!(report.format, 64);
    let newer = report.newer.as_ref().expect("a newer design");
    assert_eq!((newer.format, newer.reads_up_to), (64, 63));
    assert!(
        newer
            .warning
            .starts_with("Saved by a newer SolidCraft (format 64;"),
        "{}",
        newer.warning
    );
    assert!(
        newer.warning.contains("may build differently"),
        "{}",
        newer.warning
    );
    assert!(
        newer
            .left_out
            .iter()
            .any(|l| l.contains("`lattice_library`")),
        "{:?}",
        newer.left_out
    );
    assert!(
        newer.left_out.iter().any(|l| l.contains("`spin`")),
        "{:?}",
        newer.left_out
    );
    assert_eq!(outcome(&report, "Block"), Outcome::Parametric);
    assert_eq!(outcome(&report, "Lattice"), Outcome::Unmapped);
    assert_eq!(outcome(&report, "Lid"), Outcome::Parametric);
    let lattice = doc.features().find(|f| f.name == "Lattice").unwrap();
    assert!(lattice.suppressed);
    assert!(
        doc.import_note(lattice.uid)
            .unwrap()
            .contains("saved by a newer SolidCraft"),
    );
    // The warning is the result's first notice that is not an error.
    let notices = report.notices();
    assert_eq!(notices[0].message, newer.warning);
    assert!(
        report
            .text()
            .contains("warning: Saved by a newer SolidCraft")
    );
}

/// A design of the pinned format that lists a change this import does not
/// know opens too, the warning naming the change.
#[test]
fn an_unknown_change_opens_with_a_warning_naming_it() {
    let v = json!({"format": "solidcraft/57", "format_pending": ["a-later-change"], "name": "U",
        "params": [], "features": [
        {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "10", "width": "10",
         "height": "10", "body_names": ["B"]}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "u.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    let newer = report.newer.as_ref().expect("a newer design");
    assert_eq!(newer.unknown_changes, ["a-later-change"]);
    assert!(
        newer.warning.contains("`a-later-change`"),
        "{}",
        newer.warning
    );
    assert_eq!(outcome(&report, "Box1"), Outcome::Parametric);
}

/// The kinds and options of formats 51 to 57 without a MitCad counterpart
/// are placeholders with their reasons, and the timeline goes on: a cell
/// grid, a frame member's end cap, a patch of 3D sketch loops, a flange
/// whose edge is split into stretches, a move whose copies take their
/// sources' placements in another component (mitcad#1994). Notes anchored
/// in the model are named in the report.
#[test]
fn format_57_kinds_without_a_counterpart_are_placeholders() {
    let frame = json!([[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [5, 0, 0, 1]]);
    let v = json!({"format": "solidcraft/57", "name": "K", "params": [], "features": [
        {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "40", "width": "40", "height": "10",
         "body_names": ["B"]},
        {"id": 2, "name": "Grid", "type": "cell_grid", "sketch": 9, "profiles": {"type": "all"},
         "size": "5", "web": "1", "depth": "2", "targets": ["B"]},
        {"id": 3, "name": "Cap", "type": "frame_end_cap", "ends": [], "thickness": "3", "chamfer": "1"},
        {"id": 4, "name": "Patch3d", "type": "patch", "sketch": 9, "profiles_3d": [0]},
        {"id": 5, "name": "Flange", "type": "sheet_flange", "height": "10", "angle": "90 deg",
         "edges": [{"point": [0, 0, 0], "segments": [{"from": 0.0, "to": 0.5}]}]},
        {"id": 6, "name": "Copy", "type": "move", "bodies": ["B"], "translate": [0, 0, 0], "copy": true,
         "copy_frames": [frame]},
        {"id": 7, "name": "Box2", "type": "box", "corner": [60, 0, 0], "length": "5", "width": "5", "height": "5"}],
        "notes": [{"id": 8, "anchor": [0, 0, 10], "text": "Check this"}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    assert!(file.newer.is_none());
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "k.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    for name in ["Grid", "Cap", "Patch3d", "Flange", "Copy"] {
        assert_eq!(outcome(&report, name), Outcome::Unmapped, "{name}");
    }
    assert_eq!(outcome(&report, "Box2"), Outcome::Parametric);
    let grid = doc.features().find(|f| f.name == "Grid").unwrap();
    assert!(
        doc.import_note(grid.uid)
            .unwrap()
            .contains("no cell grid feature")
    );
    assert!(
        report.notes.iter().any(|n| n.starts_with("a note (text")),
        "{:?}",
        report.notes
    );
}

/// The options of formats 51 to 57 MitCad has come in: a bridged
/// counterbore as MitCad's stepped print bridging, a move about a point of
/// its own given by a parameter (mitcad#1994).
#[test]
fn format_57_options_come_in() {
    let v = json!({"format": "solidcraft/57", "name": "O", "params": [{"name": "px", "expr": "5 mm", "unit": "mm"}], "features": [
        {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "40", "width": "40", "height": "10",
         "body_names": ["B"]},
        {"id": 2, "name": "Bridged", "type": "hole", "position": [20, 20, 10], "direction": [0, 0, -1],
         "diameter": "3", "hole": {"type": "counterbore", "cb_diameter": "6", "cb_depth": "2", "bridge": "0.2 mm"}},
        {"id": 3, "name": "Turn", "type": "move", "bodies": ["B"], "translate": [0, 0, 0],
         "rotate_axis": [0, 0, 1], "origin": ["px", 0, 0], "angle": "90 deg"}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "o.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    let bridged = report
        .features
        .iter()
        .find(|f| f.name == "Bridged")
        .unwrap();
    assert_eq!(bridged.outcome, Outcome::Partial, "{:?}", bridged.notes);
    assert!(
        bridged.notes.iter().any(|n| n.contains("print bridging")),
        "{:?}",
        bridged.notes
    );
    let hole = def_of(&doc, "Bridged");
    assert!(hole.contains("print_bridging: Stepped"), "{hole}");
    let turn = report.features.iter().find(|f| f.name == "Turn").unwrap();
    assert_eq!(turn.outcome, Outcome::Partial, "{:?}", turn.notes);
    let rotate = def_of(&doc, "Turn (turn)");
    assert!(rotate.contains("[5.0, 0.0, 0.0]"), "{rotate}");
}

/// A design SolidCraft 0.45.0 saves (format 63, mitcad#2153) opens without
/// the newer-format warning and with nothing to upgrade. A bounding solid
/// whose six side margins are one expression is MitCad's of that margin;
/// one with a margin of its own on each side is a placeholder with the
/// reason.
#[test]
fn a_format_63_design_opens_without_a_warning() {
    let (doc, report) = import(
        "format-63-hem-slanted-fillet-bounding.solidcraft.json",
        &SolidcraftOptions::default(),
    );
    assert_eq!(report.format, 63);
    assert!(report.newer.is_none());
    assert!(report.upgrades.is_empty(), "{:?}", report.upgrades);
    assert!(
        !report.notices().iter().any(|n| n.message.contains("newer")),
        "{:?}",
        report.notices()
    );
    let names: Vec<&str> = report.features.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Outline",
            "Base",
            "Hem1",
            "Hem2",
            "Block",
            "Bevel",
            "PegSketch",
            "Peg",
            "Cube",
            "Round",
            "Envelope",
            "Even"
        ]
    );
    assert_eq!(outcome(&report, "Envelope"), Outcome::Unmapped);
    let envelope = doc.features().find(|f| f.name == "Envelope").unwrap();
    assert!(envelope.suppressed);
    assert!(
        doc.import_note(envelope.uid)
            .unwrap()
            .contains("a margin of its own on each side")
    );
    assert_eq!(outcome(&report, "Even"), Outcome::Parametric);
    let even = doc.features().find(|f| f.name == "Even").unwrap();
    let FeatureDef::BoundingSolid(def) = &even.def else {
        panic!("Even is not a bounding solid: {:?}", even.def)
    };
    assert_eq!(doc.parameters().value(def.margin), Some(1.0));
}

/// The kinds and options of formats 58 to 63 without a MitCad counterpart
/// are placeholders with their reasons, and the timeline goes on: a
/// conversion of a whole body from its faces, a recognition from the
/// body's largest face, a conversion of several faces or with engravings,
/// an import kept as B-rep records, a pattern of bodies where a placement
/// shows them, a component's
/// envelope that has subcomponents (mitcad#2153). Drawings and rigid
/// groups are named in the report.
#[test]
fn format_63_kinds_without_a_counterpart_are_placeholders() {
    let v = json!({"format": "solidcraft/63", "name": "K", "params": [],
        "components": [{"id": 20, "name": "Sub"}],
        "assembly": {"rigid_groups": [{"id": 41, "name": "Board", "component": 20, "members": [21]}]},
        "drawings": [{"format": "solidcraft-drawing/1", "sheets": []}],
        "features": [
        {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "40", "width": "40", "height": "10",
         "body_names": ["B"]},
        {"id": 2, "name": "Whole", "type": "sheet_surface_convert", "body": "B"},
        {"id": 3, "name": "Largest", "type": "sheet_recognize", "body": "B"},
        {"id": 4, "name": "Bent", "type": "sheet_recognize", "body": "B", "face": [20, 20, 10],
         "bends": [[0, 20, 10]]},
        {"id": 5, "name": "Several", "type": "sheet_convert", "body": "B", "face": [20, 20, 10],
         "faces": [[20, 0, 5]], "bends": [[20, 0, 10]]},
        {"id": 6, "name": "Engraved", "type": "sheet_convert", "body": "B", "face": [20, 20, 10],
         "engrave": [[20, 20, 9]], "bends": [[20, 0, 10]]},
        {"id": 7, "name": "Outside", "type": "sheet_convert", "body": "B", "face": [20, 20, 10],
         "direction": "outside", "bends": [[20, 0, 10]]},
        {"id": 8, "name": "Records", "type": "import", "file": "part.rec", "step": "",
         "records": {"bodies": []}},
        {"id": 9, "name": "Placed", "type": "pattern", "features": [], "bodies": ["B"],
         "placements": [[21]], "pattern": {"type": "rectangular", "dir1": [1, 0, 0], "count1": "2", "spacing1": "50"}},
        {"id": 12, "name": "Contents", "type": "bounding_solid", "bodies": [], "contents": true},
        {"id": 13, "name": "Box2", "type": "box", "corner": [60, 0, 0], "length": "5", "width": "5", "height": "5"}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    assert!(file.newer.is_none());
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "k.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    for (name, why) in [
        ("Whole", "whole body to sheet metal"),
        ("Largest", "largest face"),
        ("Bent", "bent walls"),
        ("Several", "several picked faces"),
        ("Engraved", "engravings"),
        ("Outside", "`outside`"),
        ("Records", "B-rep records"),
        ("Placed", "where a placement of their component shows them"),
        ("Contents", "subcomponents"),
    ] {
        assert_eq!(outcome(&report, name), Outcome::Unmapped, "{name}");
        let feature = doc.features().find(|f| f.name == name).unwrap();
        let note = doc.import_note(feature.uid).unwrap();
        assert!(note.contains(why), "{name}: {note}");
    }
    assert_eq!(outcome(&report, "Box2"), Outcome::Parametric);
    assert!(
        report.notes.iter().any(|n| n == "a drawing: not mapped"),
        "{:?}",
        report.notes
    );
    assert!(
        report.notes.iter().any(|n| n.starts_with("a rigid group")),
        "{:?}",
        report.notes
    );
}

/// A fillet that rounds each picked edge at its own radius (format 60) is
/// one MitCad fillet per body, a set per edge with its radius's slot; one
/// whose other options SolidCraft refuses edge radii with fails as it does
/// there (mitcad#2153).
#[test]
fn a_fillet_of_edge_radii_comes_in_as_a_set_per_edge() {
    let v = json!({"format": "solidcraft/63", "name": "R", "params": [{"name": "r2", "expr": "3 mm", "unit": "mm"}],
        "features": [
        {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "20", "width": "20", "height": "20",
         "body_names": ["B"]},
        {"id": 2, "name": "Round", "type": "fillet", "edges": [[10, 0, 20], [10, 20, 20]],
         "edge_names": ["F1:+z|F1:-y", "F1:+y|F1:+z"], "radius": "2", "edge_radii": ["2", "r2"],
         "tangent_chain": false},
        {"id": 3, "name": "Chained", "type": "fillet", "edges": [[0, 10, 20]],
         "radius": "1", "edge_radii": ["1"]}]});
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    let report = import_solidcraft_file(
        &mut doc,
        file,
        "r.solidcraft",
        &SolidcraftOptions::default(),
    )
    .unwrap();
    // The mock kernel finds a box's edges only as far as its faces go: where
    // it finds both, the fillet is one feature of a set per edge.
    let round = report.features.iter().find(|f| f.name == "Round").unwrap();
    if round.outcome == Outcome::Parametric {
        let def = def_of(&doc, "Round");
        assert_eq!(def.matches("Constant").count(), 2, "{def}");
    } else {
        assert_eq!(round.outcome, Outcome::Failed, "{:?}", round.notes);
    }
    // Tangent chains (the default) with edge radii: SolidCraft refuses it.
    let chained = report
        .features
        .iter()
        .find(|f| f.name == "Chained")
        .unwrap();
    assert_eq!(chained.outcome, Outcome::Failed);
    assert!(
        chained
            .notes
            .iter()
            .any(|n| n.contains("SolidCraft refuses it too")),
        "{:?}",
        chained.notes
    );
}

/// Cones and frustums (format 40) are MitCad's Cone (mitcad#2239), each on
/// the plane across its axis through its base point: a full cone ending in
/// an apex (no top face), a frustum along X sized by a parameter and
/// standing against its axis (a negative height), a frustum cut into a
/// block from its top face down; each parametric, its inputs the file's.
#[test]
fn cones_come_in_as_cones() {
    let (doc, report) = import(
        "format-50-cone.solidcraft.json",
        &SolidcraftOptions::default(),
    );
    for name in ["Point", "Frustum", "Block", "Countersink"] {
        assert_eq!(outcome(&report, name), Outcome::Parametric, "{name}");
    }
    let p = doc.parameters();
    let cone = |name: &str| {
        let feature = doc.features().find(|f| f.name == name).unwrap();
        let FeatureDef::Cone(def) = &feature.def else {
            panic!("{name} is not a cone: {:?}", feature.def)
        };
        assert_eq!(
            doc.status(feature.uid),
            Some(&mitcad_model::FeatureStatus::Ok),
            "{name}"
        );
        (feature.uid, def.clone())
    };
    let values = |def: &mitcad_model::features::ConeDef| {
        (
            p.value(def.radius),
            def.top_radius.and_then(|t| p.value(t)),
            p.value(def.height),
        )
    };
    // The body's faces of the cone itself: the mock kernel's cut adds the
    // countersink's faces to every body, which a real cut does not.
    let faces = |body: &str, of: mitcad_model::FeatureUid| -> Vec<String> {
        let view = doc.bodies().into_iter().find(|b| b.name == body).unwrap();
        let own = format!("{of}:");
        view.shape
            .faces
            .iter()
            .map(ToString::to_string)
            .filter(|face| face.starts_with(&own))
            .collect()
    };

    // The full cone on XY: its base and its side, no top.
    let (point, def) = cone("Point");
    assert!(matches!(def.plane, Some(mitcad_model::GeomRef::Origin(_))));
    assert_eq!(values(&def), (Some(5.0), Some(0.0), Some(10.0)));
    assert_eq!(
        faces("Point", point),
        [format!("{point}:bottom"), format!("{point}:side0")]
    );

    // The frustum on YZ by the parameter r, against the X axis.
    let (frustum, def) = cone("Frustum");
    assert!(matches!(def.plane, Some(mitcad_model::GeomRef::Origin(_))));
    assert_eq!(values(&def), (Some(8.0), Some(4.0), Some(-12.0)));
    let radius = p.get(def.radius).unwrap();
    assert_eq!(radius.expression(), "r");
    assert_eq!(
        faces("Frustum", frustum),
        [
            format!("{frustum}:bottom"),
            format!("{frustum}:top"),
            format!("{frustum}:side0")
        ]
    );

    // The countersink cuts the block: no body of its own.
    let (_, def) = cone("Countersink");
    assert_eq!(def.operation, mitcad_model::features::Operation::Cut);
    assert_eq!(values(&def), (Some(4.0), Some(1.0), Some(3.0)));
    assert_eq!(doc.bodies().len(), 3);
}
