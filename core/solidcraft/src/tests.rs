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
        r#"{"format": "solidcraft/64", "name": "A", "params": [], "features": []}"#,
    )
    .unwrap();
    assert_eq!(past.newer.as_ref().map(|n| n.format), Some(64));
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

/// A point cloud's blob put back as the plain JSON's base64 `points` and
/// `colors` (mitcad#2240).
#[test]
fn a_container_puts_a_point_clouds_points_back() {
    let mut blob = b"SCPNTS01".to_vec();
    blob.extend(2u64.to_le_bytes());
    blob.extend(1u64.to_le_bytes());
    for x in [1.0f32, 2.0, 3.0, -1.0, 0.0, 0.5] {
        blob.extend(x.to_le_bytes());
    }
    blob.extend([255u8, 0, 0, 0, 0, 255]);
    let hash = "b".repeat(64);
    let doc = json!({"format": "solidcraft/45", "name": "A", "params": [], "features": [],
        "point_clouds": [{"id": 3, "name": "Scan", "origin": [0.0, 0.0, 0.0], "points": "", "colors": "",
                          "points_blob": format!("blobs/{hash}.points")}]});
    let bytes = mitcad_zip::stored_zip(&[
        ("container.json", br#"{"format":"solidcraft-container/1"}"#),
        ("document.json", doc.to_string().as_bytes()),
        (&format!("blobs/{hash}.points"), &blob),
    ]);
    let file = SolidcraftFile::from_bytes(&bytes).unwrap();
    let cloud = &file.design.point_clouds[0];
    assert!(cloud.get("points_blob").is_none());
    // 24 bytes of points and 6 of colours, as base64.
    assert_eq!(cloud["points"].as_str().unwrap().len(), 32);
    assert_eq!(cloud["colors"], "/wAAAAD/");
    let (points, colors) = container::decode_points(&blob).unwrap();
    assert_eq!((points.len(), colors.len()), (24, 6));
    // A blob whose counts do not match its bytes is refused.
    assert!(container::decode_points(&blob[..blob.len() - 1]).is_err());
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

/// The steps of formats 58 to 63 (SolidCraft 0.45.0, mitcad#2153), in the
/// order they run.
const STEPS_58_TO_63: [&str; 7] = [
    "extrude-from-slanted-face",
    "sketch-reference-placements",
    "surface-sweep-exact-spline-path",
    "sheet-edge-names",
    "sheet-face-names",
    "sheet-hem-gap",
    "sheet-surface-convert",
];

/// A design of format 57 (SolidCraft 0.43.0, mitcad#1994) reads with only
/// the steps of formats 58 to 63 run on it, its newer inputs typed.
#[test]
fn a_format_57_design_reads_its_inputs() {
    use crate::kinds::{AxisRef, BoxPrimitive, Extrude, Hole, Revolve, Thread};
    let file = design("format-57-holes-threads-revolve.solidcraft.json");
    assert_eq!(file.saved_format, 57);
    assert_eq!(file.upgrades, STEPS_58_TO_63, "{:?}", file.upgrades);
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

/// A design a newer SolidCraft saved (format 64) is read as far as this
/// reader reads it: a section it does not know and a parameter it cannot
/// read are left out and named, a feature of a kind it does not know keeps
/// its place (mitcad#1994).
#[test]
fn a_newer_design_is_read_around_what_it_cannot_read() {
    let file = design("format-64-newer.solidcraft.json");
    let newer = file.newer.as_ref().unwrap();
    assert_eq!((newer.format, newer.reads_up_to), (64, PINNED));
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
    let v = json!({"format": "solidcraft/64", "name": "A", "params": [], "features": [
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

/// A design SolidCraft 0.45.0 saves (format 63, mitcad#2153) reads as it
/// is: nothing to upgrade, no warning, its newer fields typed and its
/// newer inputs named.
#[test]
fn a_format_63_design_reads_as_it_is() {
    use crate::kinds::{BoundingSolid, Extrude, Fillet, SheetHem};
    let file = design("format-63-hem-slanted-fillet-bounding.solidcraft.json");
    assert_eq!(file.saved_format, 63);
    assert!(file.upgrades.is_empty(), "{:?}", file.upgrades);
    assert!(file.newer.is_none());
    let d = &file.design;
    let by_name = |n: &str| d.features.iter().find(|f| f.name == n).unwrap();
    // The hems' gaps as saved (no doubling), the rule's own `hem_gap` and
    // material.
    let hem: SheetHem = by_name("Hem1").typed().unwrap();
    assert_eq!(hem.gap.as_ref().map(|g| g.as_str()), Some("3 mm"));
    let hem: SheetHem = by_name("Hem2").typed().unwrap();
    assert!(hem.gap.is_none());
    let rule = d.sheet_rule(None).unwrap();
    assert_eq!(rule.name, "Plate");
    assert_eq!(
        rule.hem_gap.as_ref().map(|g| g.as_str()),
        Some("Thickness / 2")
    );
    assert_eq!(rule.material.as_deref(), Some("Aluminium 5052-H32"));
    // The flange's and hems' edge names are the model's already.
    assert_eq!(by_name("Hem1").edge_names, ["F2:side:l1|F2:top"]);
    // From Object on the slanted bevel starts on it: no `from_level`.
    let peg: Extrude = by_name("Peg").typed().unwrap();
    assert_eq!(peg.extent.from, Some([20.0, 135.0, 15.0]));
    assert!(!peg.extent.from_level);
    // Each edge at its own radius, an input each after the radius.
    let round: Fillet = by_name("Round").typed().unwrap();
    let radii: Vec<&str> = round.edge_radii.iter().map(|r| r.as_str()).collect();
    assert_eq!(radii, ["2", "3 mm"]);
    assert!(!round.tangent_chain);
    assert_eq!(by_name("Round").param_names.len(), 3);
    // Six side margins after the single one (`0` when the file has none).
    let envelope: BoundingSolid = by_name("Envelope").typed().unwrap();
    let margins: Vec<&str> = envelope
        .margins
        .as_ref()
        .unwrap()
        .iter()
        .map(|m| m.as_str())
        .collect();
    assert_eq!(margins, ["1", "1", "2", "2", "0", "5"]);
    assert!(!envelope.contents);
    let labels: Vec<&str> = crate::inputs::inputs(&by_name("Envelope").def)
        .into_iter()
        .map(|i| i.label)
        .collect();
    assert_eq!(
        labels,
        [
            "Margin",
            "-X margin",
            "+X margin",
            "-Y margin",
            "+Y margin",
            "-Z margin",
            "+Z margin"
        ]
    );
    assert_eq!(by_name("Envelope").param_names.len(), 7);
}

/// The same design as format 57: the steps of formats 58 to 63 run on it.
/// The hem's own gap and the rule's `hem_gap` read as SolidCraft 0.43.0
/// read them, the inside radius itself, so both are doubled; a rule
/// without `hem_gap` gets twice the old default; the From Object start
/// keeps the older level (mitcad#2153).
#[test]
fn a_format_57_design_gets_the_steps_of_58_to_63() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/designs/format-63-hem-slanted-fillet-bounding.solidcraft.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["format"] = json!("solidcraft/57");
    let file = SolidcraftFile::from_value(v.clone()).unwrap();
    assert_eq!(file.upgrades, STEPS_58_TO_63);
    let d = &file.design;
    let by_name = |n: &str| d.features.iter().find(|f| f.name == n).unwrap();
    assert_eq!(by_name("Hem1").def["gap"], "2 * (3 mm)");
    assert!(by_name("Hem2").def.get("gap").is_none());
    assert_eq!(d.sheet["rules"][0]["hem_gap"], "2 * (Thickness / 2)");
    assert_eq!(by_name("Peg").def["extent"]["from_level"], true);
    // A rule saved without `hem_gap` gets twice the old default 0.01 mm.
    v["sheet"]["rules"][0]
        .as_object_mut()
        .unwrap()
        .remove("hem_gap");
    let file = SolidcraftFile::from_value(v).unwrap();
    let rule = file.design.sheet_rule(None).unwrap();
    assert_eq!(rule.hem_gap.as_ref().map(|g| g.as_str()), Some("0.02 mm"));
}

/// The fields of formats 58 to 63 the import maps or notes read typed: a
/// sweep's path across sketches, a plane at an angle about an origin of
/// expressions, a construction plane's captured placement, a move turned
/// about an edge, Recognize Sheet Metal, a conversion's new fields and an
/// import kept as records (mitcad#2153).
#[test]
fn the_fields_of_formats_58_to_63_read() {
    use crate::kinds::{
        GeoRef, Import, Move, PathEntry, PlaneRef, SheetConvert, SheetRecognize, Sweep,
    };
    let sweep: Sweep = serde_json::from_value(json!({"sketch": 1, "path_sketch": 2,
        "path": ["l1", {"sketch": 3, "curve": "l2"}]}))
    .unwrap();
    assert_eq!(sweep.path[0].split(2), (2, "l1"));
    assert_eq!(sweep.path[1].split(2), (3, "l2"));
    assert_eq!(sweep.path[0], PathEntry::OfPathSketch("l1".to_owned()));
    let turned: PlaneRef = serde_json::from_value(json!({"type": "at_angle",
        "base": {"type": "origin", "name": "XY"}, "axis_origin": ["w / 2", 0, 5], "axis_dir": [1, 0, 0],
        "angle": "30 deg"}))
    .unwrap();
    let PlaneRef::AtAngle { axis_origin, .. } = turned else {
        panic!("{turned:?}")
    };
    assert_eq!(axis_origin[0].as_str(), "w / 2");
    assert_eq!(axis_origin[1].as_str(), "0");
    let construction: PlaneRef = serde_json::from_value(json!({"type": "construction",
        "feature": 4, "placement": crate::document::IDENTITY}))
    .unwrap();
    assert!(matches!(
        construction,
        PlaneRef::Construction {
            feature: 4,
            placement: Some(_)
        }
    ));
    let turn: Move = serde_json::from_value(json!({"bodies": ["B"], "translate": [0, 0, 0],
        "rotate_axis": [0, 0, 1], "angle": "90 deg",
        "rotate_about": {"type": "edge", "at": [1, 2, 3]}}))
    .unwrap();
    assert!(matches!(turn.rotate_about, Some(GeoRef::Edge { .. })));
    let recognized: SheetRecognize =
        serde_json::from_value(json!({"body": "Plate", "face": [1, 2, 3], "rule": "R1"})).unwrap();
    assert_eq!(recognized.face, Some([1.0, 2.0, 3.0]));
    assert!(recognized.skip.is_empty() && recognized.bends.is_empty());
    let convert: SheetConvert = serde_json::from_value(json!({"body": "Box", "face": [0, 0, 0],
        "faces": [[1, 0, 0]], "engrave": [[2, 0, 0]], "direction": "outside"}))
    .unwrap();
    assert_eq!(convert.faces.len(), 1);
    assert_eq!(convert.engrave.len(), 1);
    assert_eq!(convert.direction, "outside");
    let records: Import =
        serde_json::from_value(json!({"file": "part.rec", "step": "", "records": {"bodies": []}}))
            .unwrap();
    assert!(records.has_records());
    let step: Import = serde_json::from_value(json!({"file": "part.step", "step": "ISO"})).unwrap();
    assert!(!step.has_records());
}
