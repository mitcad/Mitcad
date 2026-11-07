// SPDX-License-Identifier: MIT
//! BUILD parts of component libraries (mitcad#375): the manifest's
//! supplier fields and `build` block kept and shown, `library check`'s
//! rules for them (roles, systems, lengths, frames that are joint origins
//! of the design, the profile layout, connector, panel and linear blocks,
//! references that resolve), `library_search`'s BUILD filters and
//! `library_build_candidates` for junctions and pickers. The designs are
//! made with the mock kernel; nothing goes over the network.

use serde_json::{Value, json};

use super::remote::{slashes, system_git};
use super::*;
use crate::library::manifest::Manifest;
use crate::library::{LibraryCache, api};

fn library_command(cache: &LibraryCache, git: Option<&GitCli>, command: Value) -> Value {
    serde_json::from_str(&api::command(&command.to_string(), cache, git, None).unwrap()).unwrap()
}

use crate::remote::GitCli;
use mitcad_model_api::DocumentApi;

const SQUARE: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn design(commands: &[Value]) -> String {
    let mut doc = Document::new(MockKernel::default());
    for command in commands {
        doc.command(&command.to_string())
            .unwrap_or_else(|e| panic!("{command}: {e}"));
    }
    doc.to_json()
}

/// A joint origin `mount:<name>` on a fixed plane.
fn frame(name: &str, origin: [f64; 3], normal: [f64; 3], x_axis: [f64; 3]) -> Value {
    json!({"cmd": "add_feature", "name": format!("mount:{name}"), "def": {
        "type": "joint_origin",
        "geometry": {"origin": origin, "normal": normal, "x_axis": x_axis}}})
}

/// A 30 x 30 profile in the layout: the section on XY at the origin
/// extruded by L along +Z, end A on XY turned over, end B on a plane at
/// L, a slot frame on the +X side and the core.
fn profile(slot_normal: [f64; 3]) -> String {
    design(&[
        json!({"cmd": "add_parameter", "name": "L", "expression": "100 mm"}),
        json!({"cmd": "sketch.create"}),
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-15, -15], "width": 30, "height": 30}),
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": SQUARE}],
            "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}}),
        json!({"cmd": "add_feature", "name": "End B plane", "def": {"type": "construction_plane",
            "definition": {"type": "offset", "plane": "xy", "distance": "L"}}}),
        json!({"cmd": "add_feature", "name": "mount:end_a", "def": {
            "type": "joint_origin", "geometry": "xy", "flip": true}}),
        json!({"cmd": "add_feature", "name": "mount:end_b", "def": {
            "type": "joint_origin", "geometry": "F3"}}),
        frame("slot_x+_1", [15.0, 0.0, 0.0], slot_normal, [0.0, 0.0, 1.0]),
        frame("core", [0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),
    ])
}

/// A 20 mm block with fixed frames of these names.
fn block(frames: &[&str]) -> String {
    let mut commands = vec![
        json!({"cmd": "sketch.create"}),
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20}),
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": SQUARE}],
            "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    ];
    for (i, name) in frames.iter().enumerate() {
        commands.push(frame(
            name,
            [i as f64, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
        ));
    }
    design(&commands)
}

/// A T-nut: a block with a table of two sizes.
fn nut() -> String {
    design(&[
        json!({"cmd": "add_parameter", "name": "d", "expression": "6 mm"}),
        json!({"cmd": "sketch.create"}),
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 16, "height": "d"}),
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": SQUARE}],
            "extent": {"type": "distance", "distance": 6}, "operation": "new_body"}}),
        frame("slot", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        json!({"cmd": "set_configurations", "configurations": {
            "selectors": ["Size"], "parameters": ["d"], "default": "M6",
            "rows": [{"name": "M5", "select": {"Size": "M5"}, "values": {"d": "5 mm"}},
                     {"name": "M6", "select": {"Size": "M6"}, "values": {"d": "6 mm"}}]}}),
    ])
}

fn b8() -> Value {
    json!({"type": "B", "slot": 8, "grid": 30})
}

/// The manifest entries of a small library of BUILD parts: a profile, two
/// brackets (B-type slot 8 and I-type slot 5) and a T-nut.
fn components() -> Value {
    json!([
        {"id": "p30-b8", "path": "profiles/p30-b8.mitcad", "name": "Profile 30x30 B8",
         "category": "profiles", "supplier": "Profile Shop", "part_number": "P30_N8_B",
         "supplier_url": "https://example.org/p30", "designation": "Profile 30x30 B8 L={L}",
         "build": {"role": "profile", "system": b8(),
            "length": {"parameter": "L", "min": "10 mm", "max": "6 m", "stock": ["6000 mm"]},
            "section": {"width": "30 mm", "height": "30 mm"},
            "slot": {"depth": "9 mm", "opening": "8.2 mm", "inner_width": "16.5 mm"},
            "mass_per_metre": "0.9 kg", "future_field": {"kept": true},
            "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                       {"name": "slot_x+_1", "kind": "slot", "accepts": ["connector", "panel"]},
                       {"name": "core", "kind": "bore", "diameter": "6.8 mm"}]}},
        {"id": "bracket-b8", "path": "connectors/bracket-b8.mitcad", "name": "Bracket 30 B8",
         "category": "connectors", "supplier": "Profile Shop", "part_number": "W30_B8",
         "build": {"role": "connector", "systems": [b8()],
            "frames": [{"name": "mount_a", "kind": "face"}, {"name": "mount_b", "kind": "face"}],
            "connector": {"junctions": ["L", "T"], "space": "inner_corner",
                "envelope": {"x": "28 mm", "y": "28 mm", "z": "20 mm"},
                "mates": [{"frame": "mount_a", "to": "slot"}, {"frame": "mount_b", "to": "slot"}],
                "fasteners": [{"frame": "mount_a",
                    "nut": {"library": "test-profiles", "component": "tnut-b8", "config": "M6"},
                    "screw": {"library": "mitcad-parts", "component": "iso4762", "config": "M6x12"}}]}}},
        {"id": "bracket-i5", "path": "connectors/bracket-i5.mitcad", "name": "Bracket 20 I5",
         "category": "connectors", "supplier": "Profile Shop", "part_number": "W20_I5",
         "build": {"role": "connector", "systems": [{"type": "I", "slot": 5, "grid": 20}],
            "frames": [{"name": "mount_a", "kind": "face"}, {"name": "mount_b", "kind": "face"}],
            "connector": {"junctions": ["L", "T"], "space": "inner_corner"}}},
        {"id": "tnut-b8", "path": "nuts/tnut-b8.mitcad", "name": "T-nut B8",
         "category": "nuts", "supplier": "Profile Shop", "part_number": "TN_B8",
         "build": {"role": "fastener", "systems": [{"type": "B", "slot": 8}],
            "frames": [{"name": "slot", "kind": "slot"}]}},
        {"id": "carriage-15", "path": "linear/carriage-15.mitcad", "name": "Carriage 15",
         "category": "linear", "supplier": "Profile Shop", "part_number": "C15",
         "build": {"role": "linear", "frames": [{"name": "rail", "kind": "rail"}],
            "linear": {"kind": "carriage", "size": "HGW15",
                       "mate": {"frame": "rail", "on": "rail", "joint": "slider", "axis": "x"}}}},
        {"id": "carriage-20", "path": "linear/carriage-20.mitcad", "name": "Carriage 20",
         "category": "linear", "supplier": "Profile Shop", "part_number": "C20",
         "build": {"role": "linear", "frames": [{"name": "rail", "kind": "rail"}],
            "linear": {"kind": "carriage", "size": "HGW20",
                       "mate": {"frame": "rail", "on": "rail", "joint": "slider", "axis": "x"}}}}
    ])
}

/// The library's folder: its manifest, designs and licence note.
fn write_library(dir: &Path, components: Value, designs: &[(&str, String)]) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("mitcad-library.json"),
        json!({"format": "mitcad-library", "version": 1, "id": "test-profiles",
               "name": "Test profiles", "license": "MIT", "library_version": "1.0.0",
               "categories": [{"id": "profiles", "name": "Profiles"},
                              {"id": "connectors", "name": "Connectors"},
                              {"id": "nuts", "name": "Nuts"},
                              {"id": "linear", "name": "Linear parts"}],
               "components": components})
        .to_string(),
    )
    .unwrap();
    fs::write(dir.join("LICENSE"), "MIT\n").unwrap();
    for (path, text) in designs {
        let file = dir.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
}

fn designs() -> Vec<(&'static str, String)> {
    vec![
        ("profiles/p30-b8.mitcad", profile([1.0, 0.0, 0.0])),
        (
            "connectors/bracket-b8.mitcad",
            block(&["mount_a", "mount_b"]),
        ),
        (
            "connectors/bracket-i5.mitcad",
            block(&["mount_a", "mount_b"]),
        ),
        ("nuts/tnut-b8.mitcad", nut()),
        ("linear/carriage-15.mitcad", block(&["rail"])),
        ("linear/carriage-20.mitcad", block(&["rail"])),
    ]
}

fn check(cache: &LibraryCache, dir: &Path) -> (Vec<String>, Vec<String>) {
    let answer = library_command(cache, None, json!({"cmd": "library_check", "dir": dir}));
    let list = |key: &str| -> Vec<String> {
        answer[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap().to_owned())
            .collect()
    };
    (list("errors"), list("warnings"))
}

#[test]
fn build_fields_are_kept_in_the_manifest() {
    let mut manifest: Manifest = serde_json::from_value(json!({
        "format": "mitcad-library", "version": 1, "id": "test-profiles", "name": "Test profiles",
        "license": "MIT", "components": components()}))
    .unwrap();
    let profile = manifest.component("p30-b8").unwrap().clone();
    assert_eq!(profile.supplier, "Profile Shop");
    assert_eq!(profile.part_number, "P30_N8_B");
    assert_eq!(profile.supplier_url, "https://example.org/p30");
    // Unknown fields of the block survive a round trip.
    let text = serde_json::to_string(&manifest).unwrap();
    let again: Manifest = serde_json::from_str(&text).unwrap();
    assert_eq!(again, manifest);
    assert_eq!(
        again.component("p30-b8").unwrap().build.as_ref().unwrap()["future_field"],
        json!({"kept": true})
    );
    // The manifest's own rules hold for the good parts.
    let problems = manifest.problems();
    assert_eq!(problems.errors, Vec::<String>::new());
    // A plain address is refused.
    manifest.components[0].supplier_url = "http://example.org/p30".to_owned();
    assert!(
        manifest.problems().errors[0].contains("no https:// address"),
        "{:?}",
        manifest.problems()
    );
}

#[test]
fn build_parts_are_checked_with_their_designs() {
    let scratch = Scratch::new("library-build-check");
    let cache = LibraryCache::new(scratch.join("cache"));
    let good = scratch.join("good");
    write_library(&good, components(), &designs());
    let (errors, warnings) = check(&cache, &good);
    assert_eq!(errors, Vec::<String>::new());
    // The screw is in a library not fetched here.
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("mitcad-parts is not fetched here")),
        "{warnings:?}"
    );

    // Broken parts: each rule once.
    let mut broken = components();
    let parts = broken.as_array_mut().unwrap();
    // The profile: no slot block, a slot frame named badly, a frame the
    // design lacks, a stock length outside its limits.
    parts[0]["build"]["frames"].as_array_mut().unwrap().extend([
        json!({"name": "side_1", "kind": "slot"}),
        json!({"name": "slot_y+_1", "kind": "slot"}),
    ]);
    parts[0]["build"]["length"]["stock"] = json!(["7 m"]);
    parts[0]["build"].as_object_mut().unwrap().remove("slot");
    // The bracket: an unknown junction, a mate on a frame it lacks, a nut
    // size the nut has not.
    parts[1]["build"]["connector"]["junctions"] = json!(["L", "Y"]);
    parts[1]["build"]["connector"]["mates"][1]["frame"] = json!("mount_c");
    parts[1]["build"]["connector"]["fasteners"][0]["nut"]["config"] = json!("M8");
    // The other bracket: no systems; a nut of a component that is not there.
    parts[2]["build"].as_object_mut().unwrap().remove("systems");
    parts[2]["build"]["connector"]["fasteners"] =
        json!([{"frame": "mount_a", "nut": {"library": "test-profiles", "component": "gone"}}]);
    // The nut: an unknown role.
    parts[3]["build"]["role"] = json!("washer");
    // A carriage: no count of teeth.
    parts[4]["build"]["linear"]["teeth"] = json!(0);
    let bad = scratch.join("bad");
    let mut files = designs();
    // The profile's slot frame points out of the wrong side.
    files[0].1 = profile([0.0, 1.0, 0.0]);
    // A size of the nut with a plain address for its own page.
    let mut nut: Value = serde_json::from_str(&files[3].1).unwrap();
    nut["configurations"]["rows"][0]["supplier_url"] = json!("http://example.org/m5");
    files[3].1 = nut.to_string();
    write_library(&bad, broken, &files);
    let (errors, warnings) = check(&cache, &bad);
    let has = |text: &str| errors.iter().any(|e| e.contains(text));
    for expected in [
        "p30-b8: slot frame side_1 is not named slot_<side>_<n>",
        "p30-b8: profiles/p30-b8.mitcad: the frame slot_y+_1 is no joint origin mount:slot_y+_1",
        "p30-b8: profiles/p30-b8.mitcad: the slot frame slot_x+_1 points out of the x+ side",
        "bracket-b8: build.connector.junctions: \"Y\" is none of",
        "bracket-b8: build.connector.mates[1] names the frame mount_c",
        "bracket-b8: build.connector.fasteners[0].nut: test-profiles/tnut-b8 has no configuration M8",
        "bracket-i5: a connector lists the slot systems it fits",
        "bracket-i5: build.connector.fasteners[0].nut: test-profiles has no component gone",
        "tnut-b8: build.role washer is none of",
        "tnut-b8: nuts/tnut-b8.mitcad: row M5: the supplier_url http://example.org/m5 is no https:// address",
        "carriage-15: build.linear.teeth is no count of teeth",
    ] {
        assert!(has(expected), "{expected}\n{errors:#?}");
    }
    let warned = |text: &str| warnings.iter().any(|w| w.contains(text));
    assert!(warned("p30-b8: no build.slot"), "{warnings:#?}");
    assert!(
        warned("p30-b8: build.length.stock[0] (7000 mm) is outside"),
        "{warnings:#?}"
    );

    // A profile out of the layout: extruded by a number, end B fixed.
    let mut moved = components();
    moved.as_array_mut().unwrap().truncate(1);
    let off = scratch.join("off-layout");
    let text = design(&[
        json!({"cmd": "add_parameter", "name": "L", "expression": "100 mm"}),
        json!({"cmd": "sketch.create"}),
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-15, -15], "width": 30, "height": 30}),
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": SQUARE}],
            "extent": {"type": "distance", "distance": 100}, "operation": "new_body"}}),
        json!({"cmd": "add_feature", "name": "mount:end_a", "def": {
            "type": "joint_origin", "geometry": "xy"}}),
        frame("end_b", [0.0, 0.0, 100.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        frame(
            "slot_x+_1",
            [15.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
        ),
        frame("core", [0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),
    ]);
    write_library(&off, moved, &[("profiles/p30-b8.mitcad", text)]);
    let (errors, _) = check(&cache, &off);
    let has = |text: &str| errors.iter().any(|e| e.contains(text));
    assert!(has("no extrusion by L makes the profile"), "{errors:#?}");
    assert!(
        has("end_a is at the origin pointing along -Z"),
        "{errors:#?}"
    );
    assert!(has("end_b is fixed; it follows the length"), "{errors:#?}");
}

#[test]
fn build_candidates_fit_junctions_and_pickers() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("library-build-candidates");
    let cache = LibraryCache::new(scratch.join("cache"));
    let dir = scratch.join("profiles");
    let mut files = designs();
    // The nut's rows carry their own part numbers.
    let mut nut: Value = serde_json::from_str(&files[3].1).unwrap();
    nut["configurations"]["rows"][0]["part_number"] = json!("TN_B8_M5");
    nut["configurations"]["rows"][1]["part_number"] = json!("TN_B8_M6");
    files[3].1 = nut.to_string();
    write_library(&dir, components(), &files);
    git(&dir, &["init", "-q"]).unwrap();
    git(&dir, &["add", "-A"]).unwrap();
    git(&dir, &["commit", "-q", "-m", "Profiles 1.0"]).unwrap();
    git(&dir, &["tag", "v1.0.0"]).unwrap();
    let fetched = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": slashes(&dir)}),
    );
    assert_eq!(fetched["error"], Value::Null, "{fetched}");

    // library_show carries the supplier, the block and the rows' numbers.
    let shown = library_command(
        &cache,
        None,
        json!({"cmd": "library_show", "id": "test-profiles"}),
    );
    let profile = &shown["components"][0];
    assert_eq!(profile["supplier"], "Profile Shop", "{shown}");
    assert_eq!(profile["part_number"], "P30_N8_B");
    assert_eq!(profile["supplier_url"], "https://example.org/p30");
    assert_eq!(profile["build"]["role"], "profile");
    let nut = &shown["components"][3];
    assert_eq!(
        nut["configurations"]["rows"][1],
        json!({"name": "M6", "select": {"Size": "M6"}, "part_number": "TN_B8_M6"}),
        "{nut}"
    );

    let ids = |answer: &Value, key: &str| -> Vec<String> {
        answer[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let run = |command: Value| library_command(&cache, None, command);

    // Search: by role and system, supplier and part numbers (a row's too).
    let found = run(json!({"cmd": "library_search", "role": "connector", "system": "B/8/30"}));
    assert_eq!(ids(&found, "components"), ["bracket-b8"], "{found}");
    let found = run(json!({"cmd": "library_search", "system": {"type": "I", "slot": 5}}));
    // Parts made for no system (the carriages) fit any.
    assert_eq!(
        ids(&found, "components"),
        ["bracket-i5", "carriage-15", "carriage-20"],
        "{found}"
    );
    let found = run(json!({"cmd": "library_search", "text": "p30_n8"}));
    assert_eq!(ids(&found, "components"), ["p30-b8"], "{found}");
    let found = run(json!({"cmd": "library_search", "part_number": "tn_b8_m5"}));
    assert_eq!(ids(&found, "components"), ["tnut-b8"], "{found}");
    let found = run(json!({"cmd": "library_search", "supplier": "elsewhere"}));
    assert_eq!(ids(&found, "components"), Vec::<String>::new());

    // A corner of two 30 B8 profiles: the B8 bracket fits, the I5 one is
    // greyed with its reason.
    let corner = json!({"id": "J1", "kind": "L",
        "a": {"component": "C1", "at": "end_a"}, "b": {"component": "C2", "at": "slot_x+_1"},
        "angle": 90, "systems": [b8(), b8()], "space": {"x": 40, "y": 40, "z": 30},
        "connectors": []});
    let answer = run(json!({"cmd": "library_build_candidates", "junction": corner}));
    assert_eq!(ids(&answer, "components"), ["bracket-b8"], "{answer}");
    assert_eq!(answer["components"][0]["supplier"], "Profile Shop");
    assert_eq!(answer["components"][0]["build"]["role"], "connector");
    assert_eq!(ids(&answer, "refused"), ["bracket-i5"], "{answer}");
    // A greyed row is the part and the reason, nothing more.
    assert_eq!(
        answer["refused"][0],
        json!({"id": "bracket-i5", "name": "Bracket 20 I5", "library": "test-profiles",
               "reason": "it is made for I-type slot 5 (20), not B-type slot 8 (30)"}),
        "{answer}"
    );
    assert!(
        answer["refused"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("made for I-type slot 5 (20), not B-type slot 8 (30)"),
        "{answer}"
    );
    // A cross: the bracket goes on L and T only.
    let mut cross = corner.clone();
    cross["kind"] = json!("cross");
    let answer = run(json!({"cmd": "library_build_candidates", "junction": cross}));
    assert_eq!(ids(&answer, "components"), Vec::<String>::new());
    assert!(
        answer["refused"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("goes on L, T junctions, not on cross"),
        "{answer}"
    );
    // A corner too tight for the bracket.
    let mut tight = corner.clone();
    tight["space"] = json!({"x": 20, "y": 40, "z": 30});
    let answer = run(json!({"cmd": "library_build_candidates", "junction": tight}));
    assert!(
        answer["refused"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("needs 28 x 28 x 20 mm in the inner corner"),
        "{answer}"
    );

    // Pickers: a role and a system.
    let answer =
        run(json!({"cmd": "library_build_candidates", "role": "profile", "system": "B/8"}));
    assert_eq!(ids(&answer, "components"), ["p30-b8"], "{answer}");
    let answer =
        run(json!({"cmd": "library_build_candidates", "role": "profile", "system": "I/5/20"}));
    assert_eq!(ids(&answer, "refused"), ["p30-b8"], "{answer}");
    let answer = run(json!({"cmd": "library_build_candidates", "role": "fastener"}));
    assert_eq!(ids(&answer, "components"), ["tnut-b8"], "{answer}");
    // A carriage for a rail of size HGW15: the other size is greyed.
    let answer = run(json!({"cmd": "library_build_candidates", "role": "linear",
                            "kind": "carriage", "size": "hgw15"}));
    assert_eq!(ids(&answer, "components"), ["carriage-15"], "{answer}");
    assert_eq!(ids(&answer, "refused"), ["carriage-20"], "{answer}");
    assert_eq!(answer["refused"][0]["reason"], "it is for HGW20, not hgw15");
    let answer = run(json!({"cmd": "library_build_candidates", "role": "linear", "kind": "rail"}));
    assert_eq!(ids(&answer, "components"), Vec::<String>::new());
    assert!(api::command(r#"{"cmd": "library_build_candidates"}"#, &cache, None, None).is_err());
}

// ---------------------------------------------------------------------
// Timber construction (mitcad#478).

/// A design with these user parameters, fixed frames of these names and a
/// configuration table of `rows` (name and parameter values): a member in
/// the profile layout when it has `L` (a 100 x 22 section on XY extruded by
/// L, its ends' frames), else a 20 mm block.
fn timber_design(
    parameters: &[(&str, &str)],
    frames: &[&str],
    rows: &[(&str, &[(&str, &str)])],
) -> String {
    let mut commands: Vec<Value> = parameters
        .iter()
        .map(|(name, value)| json!({"cmd": "add_parameter", "name": name, "expression": value}))
        .collect();
    let member = parameters.iter().any(|(name, _)| *name == "L");
    let (corner, width, height, distance) = if member {
        ([-50, -11], 100, 22, json!("L"))
    } else {
        ([0, 0], 20, 20, json!(20))
    };
    commands.extend([
        json!({"cmd": "sketch.create"}),
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": corner, "width": width,
               "height": height}),
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": SQUARE}],
            "extent": {"type": "distance", "distance": distance}, "operation": "new_body"}}),
    ]);
    if member {
        commands.extend([
            json!({"cmd": "add_feature", "name": "End B plane", "def": {"type": "construction_plane",
                "definition": {"type": "offset", "plane": "xy", "distance": "L"}}}),
            json!({"cmd": "add_feature", "name": "mount:end_a", "def": {
                "type": "joint_origin", "geometry": "xy", "flip": true}}),
            json!({"cmd": "add_feature", "name": "mount:end_b", "def": {
                "type": "joint_origin", "geometry": "F3"}}),
        ]);
    }
    for (i, name) in frames.iter().enumerate() {
        commands.push(frame(
            name,
            [i as f64, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
        ));
    }
    if !rows.is_empty() {
        let names: Vec<&str> = rows[0].1.iter().map(|(name, _)| *name).collect();
        let table: Vec<Value> = rows
            .iter()
            .map(|(row, values)| {
                let values: serde_json::Map<String, Value> = values
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), json!(v)))
                    .collect();
                json!({"name": row, "select": {"Size": row}, "values": values})
            })
            .collect();
        commands.push(json!({"cmd": "set_configurations", "configurations": {
            "selectors": ["Size"], "parameters": names, "rows": table}}));
    }
    design(&commands)
}

/// A sawn member with a less common re-sawn row, a wood screw, a joist
/// hanger, an angle bracket and a sheet.
fn timber_components() -> Value {
    let screw = json!({"library": "test-profiles", "component": "wood-screw", "config": "5.0x40"});
    let outer = |kind: &str, fit: Value| {
        json!({"placement": "outer", "space": "outer", "junctions": ["L", "T"],
               "fit": {"kind": kind, "section": fit["section"], "width": fit["width"]},
               "mates": [{"frame": "mount", "to": "face"}],
               "fasteners": [{"on": "a", "frames": ["hole_a_1"], "count": 1, "screw": screw},
                             {"on": "b", "frames": ["hole_b_1"], "count": 1, "screw": screw}]})
    };
    let holes = json!([{"name": "mount", "kind": "face"}, {"name": "hole_a_1", "kind": "hole"},
                       {"name": "hole_b_1", "kind": "hole"}]);
    json!([
        {"id": "timber-sawn", "path": "timber/timber-sawn.mitcad", "name": "Sawn timber",
         "category": "profiles",
         "preview": "previews/timber-sawn.png",
         "row_previews": {"22x50": "previews/timber-sawn-22x50.png"},
         "build": {"role": "profile", "kind": "timber",
            "material": {"species": "spruce", "finish": "sawn", "grade": "", "density": 450,
                         "moisture": 20},
            "availability": "standard",
            "section": {"width": "100 mm", "height": "22 mm"},
            "nominal": {"width": "100 mm", "height": "22 mm"},
            "length": {"parameter": "L", "min": 100, "max": 6000, "stock": [2700, 3000]},
            "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                       {"name": "face_xp", "kind": "face"}, {"name": "face_yp", "kind": "face"}],
            "rows": {"22x50": {"section": {"width": "48 mm", "height": "22 mm"},
                               "nominal": {"width": "50 mm", "height": "22 mm"},
                               "availability": "less common",
                               "material": {"species": "pine", "density": 520}}}}},
        {"id": "wood-screw", "path": "wood/wood-screw.mitcad", "name": "Wood screw",
         "category": "connectors",
         "build": {"role": "fastener",
            "wood": {"kind": "screw", "head": "countersunk", "diameter": "5 mm", "length": "50 mm"},
            "frames": [{"name": "axis", "kind": "shaft"}],
            "rows": {"5.0x40": {"wood": {"length": "40 mm"}}}}},
        {"id": "hanger-48x148", "path": "wood/hanger-48x148.mitcad", "name": "Joist hanger 48x148",
         "category": "connectors",
         "build": {"role": "connector", "frames": holes,
                   "connector": outer("hanger", json!({"section": [48, 148]}))}},
        {"id": "bracket-60", "path": "wood/bracket-60.mitcad", "name": "Angle bracket 60",
         "category": "connectors",
         "build": {"role": "connector", "frames": holes,
                   "connector": outer("angle", json!({"width": [28, 150]}))}},
        {"id": "plywood", "path": "wood/plywood.mitcad", "name": "Spruce plywood", "category": "connectors",
         "build": {"role": "panel", "frames": [{"name": "face", "kind": "face"}],
            "material": {"product": "plywood", "density": 460},
            "panel": {"mount": "face", "thickness": "T", "width": "W", "height": "H",
                      "stock": [[1200, 2400]]}}}
    ])
}

fn timber_designs() -> Vec<(&'static str, String)> {
    let block = &["mount", "hole_a_1", "hole_b_1"];
    vec![
        (
            "timber/timber-sawn.mitcad",
            timber_design(
                &[("L", "3000 mm"), ("W", "100 mm"), ("T", "22 mm")],
                &["face_xp", "face_yp"],
                &[("22x100", &[("W", "100 mm")]), ("22x50", &[("W", "48 mm")])],
            ),
        ),
        (
            "wood/wood-screw.mitcad",
            timber_design(
                &[("D", "5 mm"), ("K", "50 mm")],
                &["axis"],
                &[("5.0x50", &[("K", "50 mm")]), ("5.0x40", &[("K", "40 mm")])],
            ),
        ),
        ("wood/hanger-48x148.mitcad", timber_design(&[], block, &[])),
        ("wood/bracket-60.mitcad", timber_design(&[], block, &[])),
        (
            "wood/plywood.mitcad",
            timber_design(
                &[("W", "1200 mm"), ("H", "2400 mm"), ("T", "12 mm")],
                &["face"],
                &[],
            ),
        ),
    ]
}

#[test]
fn timber_parts_are_checked_and_picked() {
    let scratch = Scratch::new("library-build-timber");
    let cache = LibraryCache::new(scratch.join("cache"));
    let mut all = components();
    all.as_array_mut()
        .unwrap()
        .extend(timber_components().as_array().unwrap().iter().cloned());
    let mut files = designs();
    files.extend(timber_designs());
    // A picture of the part and one of its row 22x50 (mitcad#484).
    const PART_PNG: &[u8] = &[0x89, b'P', b'N', b'G', b'1'];
    const ROW_PNG: &[u8] = &[0x89, b'P', b'N', b'G', b'2'];
    let good = scratch.join("good");
    write_library(&good, all.clone(), &files);
    fs::create_dir_all(good.join("previews")).unwrap();
    fs::write(good.join("previews/timber-sawn.png"), PART_PNG).unwrap();
    fs::write(good.join("previews/timber-sawn-22x50.png"), ROW_PNG).unwrap();
    let (errors, warnings) = check(&cache, &good);
    assert_eq!(errors, Vec::<String>::new());
    // No slot block, mass per metre or slot frames asked of a member.
    // (The test designs have no preview images; that warning is anyone's.)
    assert!(
        !warnings
            .iter()
            .any(|w| w.contains("timber-sawn") && !w.ends_with("no preview image")),
        "{warnings:#?}"
    );

    // Broken timber parts: each rule once.
    let mut broken = all.clone();
    let parts = broken.as_array_mut().unwrap();
    let n = parts.len();
    parts[n - 5]["build"]["rows"]["22x75"] = json!({"availability": "rare"});
    parts[n - 5]["build"]["material"]["density"] = json!("heavy");
    parts[n - 5]["row_previews"] = json!({"22x75": "previews/none.png", "22x50": "../row.png"});
    parts[n - 4]["build"]["wood"]["kind"] = json!("bolt");
    parts[n - 3]["build"]["connector"]["fit"] = json!({"kind": "clip", "section": [48]});
    parts[n - 2]["build"]["connector"]["fasteners"][0]["count"] = json!(3);
    parts[n - 1]["build"]["panel"]["mount"] = json!("wall");
    let bad = scratch.join("bad");
    write_library(&bad, broken, &files);
    let (errors, _) = check(&cache, &bad);
    let has = |text: &str| errors.iter().any(|e| e.contains(text));
    for expected in [
        "timber-sawn: build.rows.22x75.availability \"rare\" is none of standard, less common",
        "timber-sawn: build.material.density: \"heavy\" is no density",
        "timber-sawn: timber/timber-sawn.mitcad: build.rows names 22x75, which is no configuration row",
        "timber-sawn: row_previews names 22x75, which is no configuration row of timber/timber-sawn.mitcad",
        "timber-sawn: previews/none.png is not in the folder",
        "timber-sawn: the preview of row 22x50, ../row.png, is no path of a PNG image in the library",
        "wood-screw: build.wood.kind \"bolt\" is neither screw nor nail",
        "hanger-48x148: build.connector.fit.kind clip is none of angle, hanger, post_base",
        "hanger-48x148: build.connector.fit.section is no pair of lengths above 0",
        "bracket-60: build.connector.fasteners[0].count is not the number of its frames (1)",
        "plywood: build.panel.mount wall is none of slot, surface, face",
    ] {
        assert!(has(expected), "{expected}\n{errors:#?}");
    }

    // The pickers: timber only where it is asked for, the slot profiles'
    // pickers as before.
    let Some(cli) = system_git() else {
        return;
    };
    git(&good, &["init", "-q"]).unwrap();
    git(&good, &["add", "-A"]).unwrap();
    git(&good, &["commit", "-q", "-m", "Parts 1.0"]).unwrap();
    git(&good, &["tag", "v1.0.0"]).unwrap();
    let fetched = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": slashes(&good)}),
    );
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    let run = |command: Value| library_command(&cache, None, command);
    let ids = |answer: &Value, key: &str| -> Vec<String> {
        answer[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let candidates = |mut picker: Value| {
        picker["cmd"] = json!("library_build_candidates");
        let answer = run(picker);
        (ids(&answer, "components"), ids(&answer, "refused"))
    };
    let only = |id: &str| (vec![id.to_owned()], Vec::<String>::new());
    let nothing = (Vec::<String>::new(), Vec::<String>::new());
    assert_eq!(candidates(json!({"role": "profile"})), only("p30-b8"));
    assert_eq!(
        candidates(json!({"role": "profile", "kind": "timber"})),
        only("timber-sawn")
    );
    assert_eq!(candidates(json!({"role": "fastener"})), only("tnut-b8"));
    assert_eq!(
        candidates(json!({"role": "fastener", "kind": "wood"})),
        only("wood-screw")
    );
    assert_eq!(candidates(json!({"role": "panel"})), nothing);
    assert_eq!(
        candidates(json!({"role": "panel", "mount": "face"})),
        only("plywood")
    );
    let found = run(json!({"cmd": "library_search", "role": "connector", "system": "B/8/30"}));
    assert_eq!(ids(&found, "components"), ["bracket-b8"], "{found}");

    // Every size its own entry (mitcad#484): the candidates carry their
    // tables, a part without one is as before.
    let timber =
        run(json!({"cmd": "library_build_candidates", "role": "profile", "kind": "timber"}));
    let table = &timber["components"][0]["configurations"];
    assert_eq!(table["default"], "22x100", "{timber}");
    assert_eq!(table["selectors"], json!(["Size"]), "{timber}");
    let rows: Vec<&str> = table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert_eq!(rows, ["22x100", "22x50"], "{timber}");
    assert_eq!(
        table["rows"][1]["select"],
        json!({"Size": "22x50"}),
        "{timber}"
    );
    assert_eq!(
        timber["components"][0]["build"]["rows"]["22x50"]["availability"],
        "less common"
    );
    let profiles = run(json!({"cmd": "library_build_candidates", "role": "profile"}));
    assert_eq!(profiles["components"][0]["id"], "p30-b8", "{profiles}");
    assert!(
        profiles["components"][0].get("configurations").is_none(),
        "{profiles}"
    );
    let sheets = run(json!({"cmd": "library_build_candidates", "role": "panel", "mount": "face"}));
    assert!(
        sheets["components"][0].get("configurations").is_none(),
        "{sheets}"
    );
    // A row with a picture of its own says so; library_preview gives it
    // with `config`, the part's for another row or none.
    assert_eq!(table["rows"][1]["preview"], true, "{timber}");
    assert!(table["rows"][0].get("preview").is_none(), "{timber}");
    let picture = |config: Value| {
        let answer = run(json!({"cmd": "library_preview", "id": "test-profiles",
                                "component": "timber-sawn", "config": config}));
        mitcad_model::library::decode_base64(answer["png"].as_str().unwrap()).unwrap()
    };
    assert_eq!(picture(json!("22x50")), ROW_PNG);
    assert_eq!(picture(json!("22x100")), PART_PNG);
    assert_eq!(picture(Value::Null), PART_PNG);
    // library_search answers without the tables, as before.
    let searched = run(json!({"cmd": "library_search", "role": "profile", "kind": "timber"}));
    assert_eq!(ids(&searched, "components"), ["timber-sawn"], "{searched}");
    assert!(
        searched["components"][0].get("configurations").is_none(),
        "{searched}"
    );

    // A corner of slot profiles takes no timber connectors.
    let corner = json!({"id": "J1", "kind": "L", "systems": [b8(), b8()],
                        "space": {"x": 40, "y": 40, "z": 30}});
    let answer = run(json!({"cmd": "library_build_candidates", "junction": corner}));
    assert_eq!(ids(&answer, "components"), ["bracket-b8"], "{answer}");
    assert_eq!(ids(&answer, "refused"), ["bracket-i5"], "{answer}");

    // A joist 48 x 148 on a beam: the wood screw, the hanger and the
    // bracket; a 48 x 123 joist: the hanger greyed with its reason.
    let joist = |a: [f64; 2]| {
        json!({"id": "J2", "kind": "T",
               "wood": {"through": "b", "into": "a", "thickness": 48, "max_diameter": 6.8,
                        "sections": {"a": a, "b": [148, 48]}}})
    };
    let answer = run(json!({"cmd": "library_build_candidates", "junction": joist([148.0, 48.0])}));
    let mut fits = ids(&answer, "components");
    fits.sort();
    assert_eq!(
        fits,
        ["bracket-60", "hanger-48x148", "wood-screw"],
        "{answer}"
    );
    let answer = run(json!({"cmd": "library_build_candidates", "junction": joist([123.0, 48.0])}));
    assert_eq!(ids(&answer, "refused"), ["hanger-48x148"], "{answer}");
    assert_eq!(
        answer["refused"][0]["reason"], "it fits 48 × 148, the member is 123 × 48",
        "{answer}"
    );
}

/// Profiled boards (mitcad#477): a tongue-and-groove family whose rows
/// give their cover width and area, its `tongue` and `groove` frames; the
/// `board` block's rules.
#[test]
fn profiled_boards_are_checked() {
    let scratch = Scratch::new("library-build-profiled");
    let cache = LibraryCache::new(scratch.join("cache"));
    let mut all = components();
    let mut board = timber_components()[0].clone();
    board["id"] = json!("floorboard");
    board["path"] = json!("timber/floorboard.mitcad");
    board["name"] = json!("Floorboard");
    board.as_object_mut().unwrap().remove("row_previews");
    board.as_object_mut().unwrap().remove("preview");
    board["build"]["board"] = json!({"profile": "tongue_and_groove", "cover": "92 mm",
                                     "area": 2120.5, "tongue": {"length": 8, "thickness": 6}});
    board["build"]["rows"]["22x50"]["board"] = json!({"cover": "40 mm", "area": 1020});
    for frame in ["groove", "tongue"] {
        board["build"]["frames"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": frame, "kind": frame}));
    }
    all.as_array_mut().unwrap().push(board);
    let mut files = designs();
    files.push((
        "timber/floorboard.mitcad",
        timber_design(
            &[("L", "3000 mm"), ("W", "100 mm"), ("T", "22 mm")],
            &["face_xp", "face_yp", "groove", "tongue"],
            &[("22x100", &[("W", "100 mm")]), ("22x50", &[("W", "48 mm")])],
        ),
    ));
    let good = scratch.join("good");
    write_library(&good, all.clone(), &files);
    let (errors, _) = check(&cache, &good);
    assert_eq!(errors, Vec::<String>::new());

    let mut broken = all.clone();
    let parts = broken.as_array_mut().unwrap();
    let n = parts.len();
    parts[n - 1]["build"]["board"]["profile"] = json!("beaded");
    parts[n - 1]["build"]["board"]["area"] = json!("large");
    parts[n - 1]["build"]["board"]["grooves"] = json!(2.5);
    parts[n - 1]["build"]["board"]["tongue"]["thickness"] = json!(-6);
    parts[n - 1]["build"]["rows"]["22x50"]["board"]["cover"] = json!("60 mm");
    let bad = scratch.join("bad");
    write_library(&bad, broken, &files);
    let (errors, _) = check(&cache, &bad);
    let has = |text: &str| errors.iter().any(|e| e.contains(text));
    for expected in [
        "floorboard: build.board.profile \"beaded\" is none of tongue_and_groove, shiplap, \
         feather_edge, deck",
        "floorboard: build.board.area \"large\" is no number of mm² above 0",
        "floorboard: build.board.grooves 2.5 is no count",
        "floorboard: build.board.tongue.thickness is -6 mm; it must be above 0",
        "floorboard: build.rows.22x50.board.cover is 60 mm, wider than the board's 48 mm",
    ] {
        assert!(has(expected), "{expected}\n{errors:#?}");
    }
}

// ---------------------------------------------------------------------
// The default framing fastener (mitcad#516).

/// A library's `wood.default` marks the wood fastener Timber's frames and
/// spacing sets start with: `library check` takes only true or false, and
/// the wood picker lists the marked one first, before a pan-head connector
/// screw whose name comes first.
#[test]
fn the_default_wood_fastener_comes_first() {
    let scratch = Scratch::new("library-build-wood-default");
    let cache = LibraryCache::new(scratch.join("cache"));
    // The wood screw of the timber parts, marked; the connector screw.
    let mut screw = timber_components()
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "wood-screw")
        .expect("the wood screw")
        .clone();
    screw["build"]["wood"]["seat"] = json!("head_top");
    screw["build"]["wood"]["default"] = json!(true);
    let mut all = json!([screw]);
    all.as_array_mut().unwrap().push(json!(
        {"id": "connector-screw", "path": "wood/connector-screw.mitcad", "name": "Connector screw",
         "category": "connectors",
         "build": {"role": "fastener",
            "wood": {"kind": "screw", "head": "pan", "seat": "head_underside",
                     "diameter": "5 mm", "length": "40 mm"},
            "frames": [{"name": "axis", "kind": "shaft"}]}}
    ));
    let mut files: Vec<(&str, String)> = timber_designs()
        .into_iter()
        .filter(|(path, _)| *path == "wood/wood-screw.mitcad")
        .collect();
    files.push((
        "wood/connector-screw.mitcad",
        timber_design(&[("D", "5 mm"), ("K", "40 mm")], &["axis"], &[]),
    ));
    let good = scratch.join("good");
    write_library(&good, all.clone(), &files);
    let (errors, _) = check(&cache, &good);
    assert_eq!(errors, Vec::<String>::new());

    let mut broken = all.clone();
    broken.as_array_mut().unwrap()[0]["build"]["wood"]["default"] = json!("yes");
    let bad = scratch.join("bad");
    write_library(&bad, broken, &files);
    let (errors, _) = check(&cache, &bad);
    assert!(
        errors
            .iter()
            .any(|e| e == "wood-screw: build.wood.default \"yes\" is neither true nor false"),
        "{errors:#?}"
    );

    let Some(cli) = system_git() else {
        return;
    };
    git(&good, &["init", "-q"]).unwrap();
    git(&good, &["add", "-A"]).unwrap();
    git(&good, &["commit", "-q", "-m", "Parts 1.0"]).unwrap();
    git(&good, &["tag", "v1.0.0"]).unwrap();
    let fetched = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": slashes(&good)}),
    );
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    let answer = library_command(
        &cache,
        None,
        json!({"cmd": "library_build_candidates", "role": "fastener", "kind": "wood"}),
    );
    let ids: Vec<&str> = answer["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["wood-screw", "connector-screw"], "{answer}");
    assert_eq!(
        answer["components"][0]["build"]["wood"]["default"], true,
        "{answer}"
    );
}

/// Mitcad's own wood fastener library (its group file, mitcad#516) marks
/// exactly one default: the countersunk wood screw, seated by its head's
/// top (flush, mitcad#512), not the pan-head connector screw.
#[test]
fn the_shipped_timber_fasteners_default_to_the_countersunk_wood_screw() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/libraries/profiles/timber-fasteners.json");
    let group: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let parts = group["parts"].as_array().unwrap();
    let defaults: Vec<&Value> = parts
        .iter()
        .filter(|p| p["build"]["wood"]["default"] == true)
        .collect();
    assert_eq!(defaults.len(), 1, "{defaults:#?}");
    let default = defaults[0];
    assert_eq!(default["id"], "wood-screw");
    assert_eq!(default["name"], "Wood screw, countersunk");
    assert_eq!(
        default["build"]["wood"],
        json!({"kind": "screw", "head": "countersunk", "seat": "head_top", "default": true})
    );
    let connector = parts
        .iter()
        .find(|p| p["id"] == "connector-screw")
        .expect("the connector screw");
    assert_eq!(connector["build"]["wood"]["seat"], "head_underside");
    assert!(connector["build"]["wood"].get("default").is_none());
}

/// A profile without slots (a steel tube, mitcad#873) names no slot
/// system and has no slot block; a profile with a slot frame still needs
/// its system.
#[test]
fn a_profile_without_slots_needs_no_slot_system() {
    let scratch = Scratch::new("library-build-tube");
    let cache = LibraryCache::new(scratch.join("cache"));
    let mut all = components();
    let tube = json!({"id": "tube-40", "path": "profiles/tube-40.mitcad", "name": "Square tube 40x40x3",
        "category": "profiles", "designation": "Square tube 40x40x3 L={L}",
        "build": {"role": "profile",
            "length": {"parameter": "L", "min": "10 mm", "max": "6000 mm", "stock": ["6000 mm"]},
            "section": {"width": "40 mm", "height": "40 mm"},
            "mass_per_metre": "3.303 kg", "material": {"density": 7850},
            "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                       {"name": "face_xp", "kind": "face"}]}});
    all.as_array_mut().unwrap().push(tube);
    let mut files = designs();
    files.push((
        "profiles/tube-40.mitcad",
        timber_design(&[("L", "100 mm")], &["face_xp"], &[]),
    ));
    let good = scratch.join("good");
    write_library(&good, all.clone(), &files);
    let (errors, warnings) = check(&cache, &good);
    assert_eq!(errors, Vec::<String>::new());
    assert!(
        !warnings
            .iter()
            .any(|w| w.contains("tube-40") && !w.ends_with("no preview image")),
        "{warnings:#?}"
    );

    // With a slot frame it is a slot profile again: its system and slot
    // block are asked for.
    let mut slotted = all.clone();
    let parts = slotted.as_array_mut().unwrap();
    let n = parts.len();
    parts[n - 1]["build"]["frames"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "slot_x+_1", "kind": "slot"}));
    let mut files = designs();
    files.push((
        "profiles/tube-40.mitcad",
        timber_design(&[("L", "100 mm")], &["face_xp", "slot_x+_1"], &[]),
    ));
    let bad = scratch.join("bad");
    write_library(&bad, slotted, &files);
    let (errors, warnings) = check(&cache, &bad);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("tube-40: a profile names its slot system")),
        "{errors:#?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("tube-40: no build.slot")),
        "{warnings:#?}"
    );
}

/// Update brings a library's newest tag to the BUILD pickers (mitcad#1423,
/// GitHub mitcad/Mitcad#172): a library in a local bare repository, tagged
/// v2.9.0 and fetched; then v2.10.0 with a new profile pushed to it and
/// fetched again into the same clone. v2.10.0 is the newest version (by
/// its numbers, not as a text before v2.9.0), and the picker's candidates,
/// the search and library_show read it at once.
#[test]
fn an_update_brings_the_newest_tag_to_the_pickers() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("library-build-update");
    let cache = LibraryCache::new(scratch.join("cache"));
    let work = scratch.join("work");
    write_library(&work, components(), &designs());
    git(&work, &["init", "-q", "-b", "main"]).unwrap();
    git(&work, &["add", "-A"]).unwrap();
    git(&work, &["commit", "-q", "-m", "Profiles 2.9"]).unwrap();
    git(&work, &["tag", "-a", "v2.9.0", "-m", "Profiles 2.9.0"]).unwrap();
    let remote = scratch.join("remote.git");
    let url = slashes(&remote);
    git(
        &scratch.0,
        &["clone", "-q", "--bare", &slashes(&work), &url],
    )
    .unwrap();
    let run = |command: Value| library_command(&cache, Some(&cli), command);
    // The profiles a picker lists, "<id> <version>", by id.
    let profiles = |answer: &Value| -> Vec<String> {
        let mut found: Vec<String> = answer["components"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let version = c["version"].as_str().unwrap();
                let label = version.split(' ').next().unwrap_or("");
                format!("{} {label}", c["id"].as_str().unwrap())
            })
            .collect();
        found.sort();
        found
    };

    let fetched = run(json!({"cmd": "library_fetch", "url": url}));
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    assert_eq!(fetched["cloned"], true, "{fetched}");
    let picker = json!({"cmd": "library_build_candidates", "role": "profile"});
    assert_eq!(
        profiles(&run(picker.clone())),
        ["p30-b8 v2.9.0"],
        "the first fetch"
    );

    // v2.10.0: a 40 x 40 profile more, pushed with its tag.
    let mut newer = components();
    let mut p40 = newer[0].clone();
    p40["id"] = json!("p40-b8");
    p40["path"] = json!("profiles/p40-b8.mitcad");
    p40["name"] = json!("Profile 40x40 B8");
    p40["part_number"] = json!("P40_N8_B");
    p40["designation"] = json!("Profile 40x40 B8 L={L}");
    p40["build"]["section"] = json!({"width": "40 mm", "height": "40 mm"});
    newer.as_array_mut().unwrap().push(p40);
    let mut files = designs();
    files.push(("profiles/p40-b8.mitcad", profile([1.0, 0.0, 0.0])));
    write_library(&work, newer, &files);
    git(&work, &["add", "-A"]).unwrap();
    git(&work, &["commit", "-q", "-m", "Profiles 2.10"]).unwrap();
    git(&work, &["tag", "-a", "v2.10.0", "-m", "Profiles 2.10.0"]).unwrap();
    git(&work, &["push", "-q", &url, "main", "v2.10.0"]).unwrap();

    // Update: the same clone, both tags, the newest first.
    let updated = run(json!({"cmd": "library_fetch", "url": url}));
    assert_eq!(updated["error"], Value::Null, "{updated}");
    assert_eq!(updated["cloned"], false, "{updated}");
    assert_eq!(updated["dir"], fetched["dir"], "{updated}");
    let labels: Vec<&str> = updated["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["v2.10.0", "v2.9.0"], "{updated}");

    // The picker, the search and the library read v2.10.0.
    assert_eq!(
        profiles(&run(picker)),
        ["p30-b8 v2.10.0", "p40-b8 v2.10.0"],
        "after Update"
    );
    let found = run(json!({"cmd": "library_search", "text": "40x40"}));
    assert_eq!(profiles(&found), ["p40-b8 v2.10.0"], "{found}");
    let shown = run(json!({"cmd": "library_show", "id": "test-profiles"}));
    assert_eq!(shown["labels"], json!(["v2.10.0"]), "{shown}");
}

/// The URL forms that name one repository have one key (mitcad#1436,
/// GitHub mitcad/Mitcad#172): with or without `www.`, `.git` or a
/// trailing slash, in any letter case of the host, over https, http or
/// SSH; a folder by its path, as written or as a file:// URL. Other
/// repositories keep keys of their own.
#[test]
fn url_forms_of_one_repository_have_one_key() {
    use crate::library::repository_key;
    let parts = repository_key("https://github.com/mitcad/parts.git");
    assert_eq!(parts, "github.com/mitcad/parts");
    for form in [
        "https://www.github.com/mitcad/parts.git",
        "https://github.com/mitcad/parts",
        "https://github.com/mitcad/parts/",
        "https://github.com/mitcad/parts.git/",
        " HTTPS://WWW.GitHub.com/mitcad/parts.git ",
        "https://GITHUB.COM/mitcad/parts",
        "http://github.com/mitcad/parts.git",
        "https://user@github.com/mitcad/parts.git",
        "ssh://git@github.com/mitcad/parts.git",
        "git@github.com:mitcad/parts.git",
    ] {
        assert_eq!(repository_key(form), parts, "{form}");
    }
    for other in [
        "https://github.com/mitcad/part.git",
        "https://www.github.com/mitcad/community-index.git",
        "https://gitlab.com/mitcad/parts.git",
        "https://github.com/someone/parts.git",
        "https://wwwgithub.com/mitcad/parts.git",
    ] {
        assert_ne!(repository_key(other), parts, "{other}");
    }
    // Folders: the path, its separators, file:// and a drive.
    assert_eq!(
        repository_key("/srv/libraries/parts.git"),
        repository_key("file:///srv/libraries/parts")
    );
    assert_eq!(
        repository_key("/srv/libraries/parts/"),
        "/srv/libraries/parts"
    );
    assert_eq!(
        repository_key("C:\\Libraries\\parts.git"),
        repository_key("file:///C:/Libraries/parts")
    );
    assert_eq!(repository_key("C:\\Libraries\\parts"), "c:/libraries/parts");
    assert_ne!(
        repository_key("/srv/libraries/parts"),
        repository_key("/srv/other/parts")
    );
}

/// A source removed in Library Sources and added again (mitcad#1436,
/// GitHub mitcad/Mitcad#172), from a local bare repository: removed, its
/// components leave the BUILD picker and the search while designs still
/// read the library; added again in another URL form and fetched, the same
/// clone is updated and each component is listed once. A second clone of
/// the repository that an older MitCad made for another URL form is left
/// out, so each component, and so each favourite (the stars are kept by
/// library and component), comes once.
#[test]
fn a_source_removed_and_added_again_lists_each_component_once() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("library-sources-again");
    let cache = LibraryCache::new(scratch.join("cache"));
    let work = scratch.join("work");
    write_library(&work, components(), &designs());
    git(&work, &["init", "-q", "-b", "main"]).unwrap();
    git(&work, &["add", "-A"]).unwrap();
    git(&work, &["commit", "-q", "-m", "Profiles 1.0"]).unwrap();
    git(&work, &["tag", "-a", "v1.0.0", "-m", "Profiles 1.0.0"]).unwrap();
    let remote = scratch.join("remote.git");
    let url = slashes(&remote);
    git(
        &scratch.0,
        &["clone", "-q", "--bare", &slashes(&work), &url],
    )
    .unwrap();
    // The same repository as a file:// URL (file:///C:/… on Windows).
    let other_form = if url.starts_with('/') {
        format!("file://{url}")
    } else {
        format!("file:///{url}")
    };
    let run = |command: Value| library_command(&cache, Some(&cli), command);
    // "<library>/<component>" of each component an answer lists, sorted.
    let ids = |answer: &Value| -> Vec<String> {
        let mut found: Vec<String> = answer["components"]
            .as_array()
            .unwrap_or_else(|| panic!("{answer}"))
            .iter()
            .map(|c| {
                format!(
                    "{}/{}",
                    c["library"].as_str().unwrap(),
                    c["id"].as_str().unwrap()
                )
            })
            .collect();
        found.sort();
        found
    };
    let picker = json!({"cmd": "library_build_candidates", "role": "profile"});
    let search = json!({"cmd": "library_search", "sources": [other_form]});
    let all = [
        "test-profiles/bracket-b8",
        "test-profiles/bracket-i5",
        "test-profiles/carriage-15",
        "test-profiles/carriage-20",
        "test-profiles/p30-b8",
        "test-profiles/tnut-b8",
    ];

    let fetched = run(json!({"cmd": "library_fetch", "url": url}));
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    assert_eq!(fetched["cloned"], true, "{fetched}");
    assert_eq!(ids(&run(picker.clone())), ["test-profiles/p30-b8"]);
    assert_eq!(ids(&run(json!({"cmd": "library_search"}))), all);

    // Removed: the clone is marked and left out of the picker and the
    // search; the libraries list says so; designs still read it.
    let removed = run(json!({"cmd": "library_remove", "url": url, "sources": []}));
    assert_eq!(removed["changed"], json!([fetched["dir"]]), "{removed}");
    assert!(
        ids(&run(picker.clone())).is_empty(),
        "the picker after Remove"
    );
    let found = run(json!({"cmd": "library_search"}));
    assert!(ids(&found).is_empty(), "{found}");
    let text = found["clones"]["left_out"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(
        text.ends_with("left out: its source was removed"),
        "{found}"
    );
    let listed = run(json!({"cmd": "library_list"}));
    assert_eq!(listed["libraries"][0]["removed"], true, "{listed}");
    let shown = run(json!({"cmd": "library_show", "id": "test-profiles"}));
    assert_eq!(shown["manifest"]["id"], "test-profiles", "{shown}");

    // Added again in another form: the mark is taken off at once, and the
    // fetch updates the same clone instead of making a second one.
    let added = run(json!({"cmd": "library_remove", "url": other_form, "removed": false}));
    assert_eq!(added["changed"], json!([fetched["dir"]]), "{added}");
    assert_eq!(ids(&run(picker.clone())), ["test-profiles/p30-b8"]);
    let again = run(json!({"cmd": "library_fetch", "url": other_form}));
    assert_eq!(again["error"], Value::Null, "{again}");
    assert_eq!(again["cloned"], false, "{again}");
    assert_eq!(again["dir"], fetched["dir"], "{again}");
    assert_eq!(cache.repos().len(), 1, "one clone of the repository");
    assert_eq!(ids(&run(picker.clone())), ["test-profiles/p30-b8"]);
    assert_eq!(ids(&run(search.clone())), all);

    // A second clone an older MitCad made for another form of the URL
    // (`.git` left out): read once, the other left out and logged.
    let old = scratch.join("cache").join("remote-00000000.git");
    let old_dir = slashes(&old);
    git(&scratch.0, &["clone", "-q", "--bare", &url, &old_dir]).unwrap();
    let without_git = url.trim_end_matches(".git").to_owned();
    git(&old, &["config", "remote.origin.url", &without_git]).unwrap();
    assert_eq!(cache.repos().len(), 2);
    let candidates = run(picker.clone());
    assert_eq!(ids(&candidates), ["test-profiles/p30-b8"], "{candidates}");
    assert_eq!(
        candidates["clones"]["read"].as_array().map(Vec::len),
        Some(1)
    );
    let found = run(search.clone());
    assert_eq!(ids(&found), all, "{found}");
    let text = found["clones"]["left_out"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(text.contains("left out: same repository as "), "{found}");
    assert_eq!(ids(&run(json!({"cmd": "library_search"}))), all);

    // Removing one form while another listed source names the repository
    // marks nothing; removing the last one marks both clones.
    let kept = run(json!({"cmd": "library_remove", "url": url, "sources": [other_form]}));
    assert_eq!(kept["changed"], json!([]), "{kept}");
    assert_eq!(ids(&run(picker.clone())), ["test-profiles/p30-b8"]);
    let gone = run(json!({"cmd": "library_remove", "url": other_form, "sources": []}));
    assert_eq!(gone["changed"].as_array().map(Vec::len), Some(2), "{gone}");
    assert!(ids(&run(picker)).is_empty(), "both clones left out");
}
