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
    let newer = SolidcraftFile::from_value(with(json!(["a-change-from-the-future"])));
    assert!(newer.unwrap_err().contains("newer"));
    let too_new = SolidcraftFile::from_json(
        r#"{"format": "solidcraft/51", "name": "A", "params": [], "features": []}"#,
    );
    assert!(too_new.unwrap_err().contains("up to format 50"));
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
