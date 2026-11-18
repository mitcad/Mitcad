// SPDX-License-Identifier: MIT
//! The reader: formats and their upgrades, the container, feature inputs.
//! The designs are written for these tests (`tests/designs/README.md`).

use serde_json::{Value, json};

use super::*;

fn design(path: &str) -> SolidcraftFile {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/designs")
        .join(path);
    SolidcraftFile::open(&file).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn a_current_design_reads_as_it_is() {
    let file = design("box-fillet-hole.solidcraft.json");
    assert_eq!(file.saved_format, 27);
    // Only the steps of the changes after format 27 (no hand-written one).
    assert!(
        file.upgrades.iter().all(|u| !u.contains(" to ")),
        "{:?}",
        file.upgrades
    );
    assert!(
        file.upgrades
            .iter()
            .any(|u| u == "two-sided-extrude-direction")
    );
    let d = &file.design;
    assert_eq!(d.params.len(), 5);
    let kinds: Vec<&str> = d.features.iter().map(|f| f.kind.as_str()).collect();
    assert_eq!(kinds, ["sketch", "extrude", "fillet", "sketch", "extrude"]);
    // Every input has its parameter name.
    assert_eq!(d.features[1].param_names, ["d3"]);
    assert_eq!(d.named_expression("d3").as_deref(), Some("height"));
}

/// Format 1: no format string at all is format 1; bare body colours become
/// appearances (2 → 3), joints' flips are inverted (3 → 4), features of a
/// design with components reach every body (4 → 5), the old XZ plane keeps
/// its axes (5 → 6), stored names get the legacy mark (13 → 14), and the
/// inputs are named.
#[test]
fn format_1_is_upgraded_step_by_step() {
    let file = design("format-1.solidcraft.json");
    assert_eq!(file.saved_format, 1);
    for step in [
        "2 to 3",
        "3 to 4",
        "4 to 5",
        "5 to 6",
        "13 to 14",
        "two-sided-extrude-direction",
    ] {
        assert!(
            file.upgrades.iter().any(|u| u == step),
            "{step}: {:?}",
            file.upgrades
        );
    }
    let d = &file.design;
    assert_eq!(
        d.appearances["bodies"]["Body1"]["color"],
        json!([1.0, 0.0, 0.0])
    );
    assert_eq!(d.assembly["joints"][0]["flip"], json!(true));
    let sketch = &d.features[0];
    assert_eq!(sketch.def["plane"]["type"], "custom");
    assert_eq!(sketch.def["plane"]["plane"]["y_dir"], json!([0, 0, 1]));
    let extrude = &d.features[1];
    assert_eq!(extrude.participants, ["*"]);
    assert!(extrude.shell_piece_order);
    let fillet = &d.features[2];
    assert_eq!(fillet.edge_names, ["?F2:top|F2:side:l5"]);
    assert_eq!(extrude.param_names, ["d1"]);
    assert_eq!(fillet.param_names, ["d2"]);
    assert_eq!(
        d.occurrences.len(),
        1,
        "a component without an occurrence gets one"
    );
}

#[test]
fn the_legacy_format_name_reads() {
    let file = SolidcraftFile::from_json(
        r#"{"format": "solvecraft/12", "name": "A", "params": [], "features": []}"#,
    )
    .unwrap();
    assert_eq!(file.saved_format, 12);
    assert!(file.upgrades.iter().any(|u| u == "13 to 14"));
}

/// A design saved while changes were pending: only the steps of the changes
/// it does not have run; one this import does not know refuses the file.
#[test]
fn pending_changes_run_once() {
    let two_sided = json!({"id": 2, "name": "E", "type": "extrude", "sketch": 1, "profiles": {"type": "all"},
        "extent": {"distance": "10", "direction": "negative", "distance2": "5"}});
    let with = |pending: Value| json!({"format": "solidcraft/28", "format_pending": pending, "name": "A", "params": [], "features": [two_sided.clone()]});
    let had = SolidcraftFile::from_value(with(json!(["two-sided-extrude-direction"]))).unwrap();
    assert_eq!(
        had.design.features[0].def["extent"]["direction"],
        "negative"
    );
    let missing = SolidcraftFile::from_value(with(json!(["component-metadata"]))).unwrap();
    assert_eq!(
        missing.design.features[0].def["extent"]["direction"],
        "positive"
    );
    let older = SolidcraftFile::from_value(
        json!({"format": "solidcraft/27", "name": "A", "params": [], "features": [two_sided]}),
    )
    .unwrap();
    assert_eq!(
        older.design.features[0].def["extent"]["direction"],
        "positive"
    );
    // A change nobody here knows: read, with the changes it does know run,
    // and said to be newer (mitcad#1994).
    let newer = SolidcraftFile::from_value(with(json!(["a-change-from-the-future"]))).unwrap();
    assert_eq!(
        newer.design.features[0].def["extent"]["direction"],
        "positive"
    );
    let said = newer.newer.unwrap();
    assert_eq!(said.unknown_changes, ["a-change-from-the-future"]);
    assert_eq!((said.format, said.reads_up_to), (28, PINNED));
    assert!(missing.newer.is_none());
    let past = SolidcraftFile::from_json(
        r#"{"format": "solidcraft/58", "name": "A", "params": [], "features": []}"#,
    )
    .unwrap();
    assert_eq!(past.newer.as_ref().map(|n| n.format), Some(58));
    assert!(past.upgrades.is_empty(), "{:?}", past.upgrades);
    assert!(SolidcraftFile::from_json(r#"{"format": "solidcraft/0"}"#).is_err());
    assert!(SolidcraftFile::from_json(r#"{"format": "other/3"}"#).is_err());
    assert!(SolidcraftFile::from_json("[1, 2]").is_err());
    // A drawing is a file of its own, refused as one.
    let drawing = SolidcraftFile::from_json(
        r#"{"format": "solidcraft-drawing/1", "sheets": [], "sources": []}"#,
    );
    assert!(drawing.unwrap_err().contains("drawing"));
}

/// A released format 28 (no `format_pending`) has all thirteen changes of
/// its format, the two-sided extrude's direction among them: the step of
/// `two-sided-extrude-direction` does not run on it again (mitcad#1331),
/// as SolidCraft's own upgrade runs a format's changes only for a design
/// that lists the ones it has.
#[test]
fn a_released_format_28_is_not_upgraded_to_28_again() {
    let file = design("format-28-released.solidcraft.json");
    assert_eq!(file.saved_format, 28);
    assert!(file.saved_pending.is_empty());
    assert!(
        !file
            .upgrades
            .iter()
            .any(|u| u == "two-sided-extrude-direction"),
        "{:?}",
        file.upgrades
    );
    assert_eq!(
        file.design.features[1].def["extent"]["direction"],
        "negative"
    );
    // Written while the changes were pending, without that one: it runs.
    let mut v: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/designs/format-28-released.solidcraft.json"),
        )
        .unwrap(),
    )
    .unwrap();
    v["format_pending"] = json!(["component-metadata", "joint-relations"]);
    let pending = SolidcraftFile::from_value(v).unwrap();
    assert!(
        pending
            .upgrades
            .iter()
            .any(|u| u == "two-sided-extrude-direction")
    );
    assert_eq!(
        pending.design.features[1].def["extent"]["direction"],
        "positive"
    );
    // The steps of the later formats run on it.
    assert!(file.upgrades.iter().any(|u| u == "face-pieces-by-place"));
    // The four changes format 28 got at its release are known too.
    for name in [
        "joint-origin-point-x",
        "line-circle-distance",
        "loft-guide-rails",
        "surface-blend-seam",
    ] {
        let v = json!({"format": "solidcraft/28", "format_pending": [name], "name": "A", "params": [], "features": []});
        assert!(SolidcraftFile::from_value(v).is_ok(), "{name}");
    }
}

/// Feature inputs are named in timeline order with the next `d<n>` not
/// taken by a parameter or another feature's name, as SolidCraft names
/// them.
#[test]
fn inputs_are_named_as_solidcraft_names_them() {
    let v = json!({"format": "solidcraft/27", "name": "A",
        "params": [{"name": "d1", "expr": "3", "unit": "mm", "model": true}],
        "features": [
            {"id": 1, "name": "Box1", "type": "box", "corner": [0, 0, 0], "length": "10", "width": "10", "height": "10"},
            {"id": 2, "name": "Box2", "type": "box", "corner": [0, 0, 0], "length": "1", "width": "2", "height": "3",
             "param_names": ["", "d2"]}]});
    let d = SolidcraftFile::from_value(v).unwrap().design;
    assert_eq!(d.features[0].param_names, ["d3", "d4", "d5"]);
    assert_eq!(d.features[1].param_names, ["d6", "d2", "d7"]);
}

/// The container: `document.json` with its STEP and mesh payloads in blobs.
#[test]
fn a_container_reads_with_its_blobs() {
    let mut mesh = b"SCMESH01".to_vec();
    mesh.extend(9u64.to_le_bytes());
    mesh.extend(3u64.to_le_bytes());
    for x in [0.0f64, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        mesh.extend(x.to_le_bytes());
    }
    for i in [0u32, 1, 2] {
        mesh.extend(i.to_le_bytes());
    }
    let hash = "a".repeat(64);
    let doc = json!({"format": "solidcraft/27", "name": "A", "params": [], "features": [
        {"id": 1, "name": "Import1", "type": "import", "file": "a.step", "step": "", "step_blob": format!("blobs/{hash}.step")},
        {"id": 2, "name": "Mesh1", "type": "mesh_import", "file": "a.stl",
         "meshes": [{"name": "M", "positions": [], "triangles": [], "mesh_blob": format!("blobs/{hash}.mesh")}]}]});
    let bytes = mitcad_zip::stored_zip(&[
        ("container.json", br#"{"format":"solidcraft-container/1"}"#),
        ("document.json", doc.to_string().as_bytes()),
        (&format!("blobs/{hash}.step"), b"ISO-10303-21;"),
        (&format!("blobs/{hash}.mesh"), &mesh),
    ]);
    assert!(looks_like_design(&bytes));
    let file = SolidcraftFile::from_bytes(&bytes).unwrap();
    assert!(file.container);
    let import: kinds::Import = file.design.features[0].typed().unwrap();
    assert_eq!(import.step, "ISO-10303-21;");
    let meshes: kinds::MeshImport = file.design.features[1].typed().unwrap();
    assert_eq!(meshes.meshes[0].triangles, [0, 1, 2]);
    assert_eq!(meshes.meshes[0].positions[3], 1.0);
    // A damaged mesh is refused, not read.
    assert!(container::decode_mesh(&mesh[..30]).is_err());
}

#[test]
fn sketches_read_typed() {
    let file = design("box-fillet-hole.solidcraft.json");
    let s: kinds::SketchFeature = file.design.features[0].typed().unwrap();
    assert_eq!(s.sketch.curves.len(), 4);
    assert_eq!(s.sketch.counts(), (5, 2));
    assert_eq!(s.sketch.constraints[5].param.as_deref(), Some("d1"));
}

/// The assembly's motion data (contact sets, motion studies, exploded
/// views, motion links) is read and kept, for the import to map later.
#[test]
fn motion_data_is_kept() {
    let v = json!({"format": "solidcraft/27", "name": "A", "params": [], "features": [],
        "assembly": {"joints": [{"id": 1, "name": "J", "kind": "revolute"}],
            "links": [{"a": 1, "b": 2, "ratio": 2.0}],
            "contact_sets": [{"name": "C", "occurrences": [3, 4]}], "contact_enabled": true,
            "studies": [{"name": "S", "steps": [{"joint": 1, "value": 1.0}]}],
            "exploded": [{"name": "E", "steps": []}]}});
    let d = SolidcraftFile::from_value(v).unwrap().design;
    assert_eq!(d.joints().len(), 1);
    assert_eq!(d.motion_links()[0]["ratio"], 2.0);
    assert_eq!(d.contact_sets()[0]["name"], "C");
    assert_eq!(d.contact(), (true, false));
    assert_eq!(d.motion_studies()[0]["steps"][0]["joint"], 1);
    assert_eq!(d.exploded_views()[0]["name"], "E");
}

/// A design of format 57 (SolidCraft 0.43.0, mitcad#1994) reads as it is:
/// nothing to upgrade, its newer inputs typed.
#[test]
fn a_format_57_design_reads_its_inputs() {
    use crate::kinds::{AxisRef, BoxPrimitive, Extrude, Hole, Revolve, Thread};
    let file = design("format-57-holes-threads-revolve.solidcraft.json");
    assert_eq!(file.saved_format, 57);
    assert!(file.upgrades.is_empty(), "{:?}", file.upgrades);
    assert!(file.newer.is_none());
    let d = &file.design;
    let by_name = |n: &str| d.features.iter().find(|f| f.name == n).unwrap();
    let slab: Extrude = by_name("Slab").typed().unwrap();
    assert!(!slab.extent.second_follows_first);
    assert_eq!(slab.extent.distance.as_str(), "-2 mm");
    let tapped: Hole = by_name("Tapped").typed().unwrap();
    assert_eq!(tapped.thread_class.as_deref(), Some("6H"));
    assert!(!tapped.old_surface_start);
    let threads: Thread = by_name("Pin threads").typed().unwrap();
    assert_eq!(threads.additional_faces, [[33.0, 15.0, 3.0]]);
    assert_eq!(threads.target_bodies, ["Slab", "Slab"]);
    assert!(threads.continuous_flanks);
    let ring: Revolve = by_name("Ring").typed().unwrap();
    let AxisRef::Line { origin, .. } = &ring.axis else {
        panic!("{:?}", ring.axis)
    };
    assert_eq!(origin[0].as_str(), "cx");
    assert_eq!(origin[1].as_str(), "0");
    let collar: Revolve = by_name("Collar").typed().unwrap();
    assert!(matches!(
        collar.axis,
        AxisRef::SketchLine {
            sketch: Some(10),
            ..
        }
    ));
    let cube: BoxPrimitive = by_name("Cube").typed().unwrap();
    assert!(cube.centred);
    // Every input has its parameter name.
    assert_eq!(by_name("Ring").param_names, ["d11"]);
}

/// The same design as format 50: the steps of formats 51 to 57 run on it,
/// so its two-sided extrude keeps the older reading and nothing else of it
/// changes.
#[test]
fn a_format_50_design_gets_the_steps_of_51_to_57() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/designs/format-57-holes-threads-revolve.solidcraft.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["format"] = json!("solidcraft/50");
    let file = SolidcraftFile::from_value(v).unwrap();
    for step in [
        "two-sided-extrude-fixed-sides",
        "configuration-row-ids",
        "hole-sketch-plane-start",
        "pattern-names-by-source",
        "extrude-to-object-keeps-picked-placements",
        "sweep-walls-named-along-the-path",
    ] {
        assert!(
            file.upgrades.iter().any(|u| u == step),
            "{step}: {:?}",
            file.upgrades
        );
    }
    let slab = file
        .design
        .features
        .iter()
        .find(|f| f.name == "Slab")
        .unwrap();
    assert_eq!(slab.def["extent"]["second_follows_first"], true);
    // A hole at a position is not one at sketch points.
    let tapped = file
        .design
        .features
        .iter()
        .find(|f| f.name == "Tapped")
        .unwrap();
    assert!(tapped.def.get("old_surface_start").is_none());
}

/// A design a newer SolidCraft saved (format 58) is read as far as this
/// reader reads it: a section it does not know and a parameter it cannot
/// read are left out and named, a feature of a kind it does not know keeps
/// its place (mitcad#1994).
#[test]
fn a_newer_design_is_read_around_what_it_cannot_read() {
    let file = design("format-58-newer.solidcraft.json");
    let newer = file.newer.as_ref().unwrap();
    assert_eq!((newer.format, newer.reads_up_to), (58, PINNED));
    assert!(file.upgrades.is_empty());
    assert_eq!(
        file.left_out,
        [
            "the section `lattice_library`, which this import does not know",
            "the parameter `spin`, which this import cannot read"
        ]
    );
    let kinds: Vec<&str> = file
        .design
        .features
        .iter()
        .map(|f| f.kind.as_str())
        .collect();
    assert_eq!(kinds, ["box", "gyroid_lattice", "box"]);
    let names: Vec<&str> = file.design.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["side"]);
    // A feature without an id or a type keeps its place with one of its
    // own; a section that does not read as this reader reads it is left
    // out.
    let v = json!({"format": "solidcraft/60", "name": "A", "params": [], "features": [
        {"id": 4, "name": "Known", "type": "box"},
        {"name": "Nameless kind"}],
        "occurrences": "not a list"});
    let file = SolidcraftFile::from_value(v).unwrap();
    let ids: Vec<(u64, &str)> = file
        .design
        .features
        .iter()
        .map(|f| (f.id, f.kind.as_str()))
        .collect();
    assert_eq!(ids, [(4, "box"), (5, "unknown")]);
    assert!(
        file.left_out
            .iter()
            .any(|l| l == "the section `occurrences`, which this import cannot read"),
        "{:?}",
        file.left_out
    );
    // The same damage in a design of a format this reader reads is refused.
    let v = json!({"format": "solidcraft/57", "name": "A", "params": [], "features": [],
        "occurrences": "not a list"});
    assert!(SolidcraftFile::from_value(v).is_err());
}

/// The references formats 51 to 57 add read: a component's origin datum
/// (format 52) by its key, a snap point (format 55), a plane at an angle
/// about a direction only (format 51).
#[test]
fn the_newer_references_read() {
    use crate::kinds::{GeoRef, PlaneRef, datum_key};
    assert_eq!(
        datum_key("o:2/3/origin/xy"),
        Some((vec![2, 3], "XY".to_owned()))
    );
    for bad in [
        "o:/origin/XY",
        "o:01/origin/X",
        "o:0/origin/X",
        "c0/origin",
        "o:2/origin/W",
    ] {
        assert!(datum_key(bad).is_none(), "{bad}");
    }
    let plane: PlaneRef =
        serde_json::from_value(json!({"type": "datum", "datum": "o:2/origin/XZ"})).unwrap();
    assert_eq!(
        plane,
        PlaneRef::Datum {
            datum: "o:2/origin/XZ".to_owned()
        }
    );
    let snap: GeoRef = serde_json::from_value(json!({"type": "snap", "kind": "centre",
        "of": {"type": "edge", "at": [1, 2, 3]}, "at": [1, 2, 3]}))
    .unwrap();
    assert!(matches!(snap, GeoRef::Snap { end: None, .. }));
    let turned: PlaneRef = serde_json::from_value(json!({"type": "at_angle",
        "base": {"type": "origin", "name": "XY"}, "axis_origin": [0, 0, 5], "axis_dir": [1, 0, 1],
        "angle": "30 deg", "direction_only": true, "pivot": {"type": "point", "p": [1, 0, 0]}}))
    .unwrap();
    assert!(matches!(
        turned,
        PlaneRef::AtAngle {
            direction_only: true,
            pivot: Some(_),
            ..
        }
    ));
}
