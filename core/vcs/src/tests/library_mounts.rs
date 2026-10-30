// SPDX-License-Identifier: MIT
//! Mount frames of library parts that are no BUILD parts (mitcad#525):
//! `library_add` writes the design's frames into the manifest's `mounts`,
//! the block survives a round trip, `library check` holds it against its
//! rules and the design, `library_show` shows it, and a part of a fetched
//! library is placed by its frame through the resolver. The designs are
//! made with the mock kernel; nothing goes over the network; the fetch is
//! skipped without git.

use std::sync::Arc;

use serde_json::{Value, json};

use super::remote::system_git;
use super::*;
use crate::library::manifest::Manifest;
use crate::library::{LibraryCache, LibraryResolver, api};
use crate::remote::GitCli;
use mitcad_model::{InsertOptions, LibraryPart};
use mitcad_model_api::DocumentApi;

const SQUARE: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn library_command(cache: &LibraryCache, git: Option<&GitCli>, command: Value) -> Value {
    serde_json::from_str(&api::command(&command.to_string(), cache, git, None).unwrap()).unwrap()
}

/// A box `width` x `depth` x `height` from the origin with joint origins
/// on fixed planes, each `(origin, normal, mount)`; a mount of null names
/// the frame `mount:base` instead.
fn design(size: [f64; 3], frames: &[([f64; 3], [f64; 3], Value)]) -> String {
    let mut doc = Document::new(MockKernel::default());
    let mut commands = vec![
        json!({"cmd": "sketch.create"}),
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": size[0], "height": size[1]}),
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": SQUARE}],
            "extent": {"type": "distance", "distance": size[2]}, "operation": "new_body"}}),
    ];
    for (origin, normal, mount) in frames {
        let geometry = json!({"origin": origin, "normal": normal, "x_axis": [1, 0, 0]});
        commands.push(if mount.is_null() {
            json!({"cmd": "add_feature", "name": "mount:base",
                   "def": {"type": "joint_origin", "geometry": geometry}})
        } else {
            json!({"cmd": "add_feature",
                   "def": {"type": "joint_origin", "geometry": geometry, "mount": mount}})
        });
    }
    for command in commands {
        doc.command(&command.to_string())
            .unwrap_or_else(|e| panic!("{command}: {e}"));
    }
    doc.to_json()
}

fn bin(cells: u32) -> String {
    let width = 42.0 * f64::from(cells);
    design(
        [width, 42.0, 21.0],
        &[(
            [width / 2.0, 21.0, 0.0],
            [0.0, 0.0, -1.0],
            json!({"name": "foot", "kind": "foot", "footprint": [cells, 1], "pitch": [42, 42]}),
        )],
    )
}

fn baseplate() -> String {
    design(
        [126.0, 84.0, 5.0],
        &[(
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 1.0],
            json!({"name": "grid", "kind": "grid", "pitch": [42, 42], "cells": [3, 2]}),
        )],
    )
}

fn lamp() -> String {
    design(
        [10.0, 10.0, 5.0],
        &[(
            [5.0, 5.0, 0.0],
            [0.0, 0.0, -1.0],
            json!({"name": "base", "kind": "face", "joint": "planar"}),
        )],
    )
}

/// The library `test-mounts` in `dir`: two bins, a baseplate, a lamp, a
/// block whose frame is named `mount:base`, and a part without frames.
fn make_library(cache: &LibraryCache, dir: &Path) {
    let answer = library_command(
        cache,
        None,
        json!({"cmd": "library_init", "dir": dir, "id": "test-mounts", "name": "Test mounts",
               "license": "CC0-1.0", "authors": ["Mount makers"]}),
    );
    assert_eq!(answer["id"], "test-mounts", "{answer}");
    let plain = design([10.0, 10.0, 10.0], &[]);
    let block = design(
        [10.0, 10.0, 5.0],
        &[([0.0; 3], [0.0, 0.0, -1.0], Value::Null)],
    );
    for (id, name, category, text) in [
        ("bin1x1", "Bin 1x1", "bins", bin(1)),
        ("bin2x1", "Bin 2x1", "bins", bin(2)),
        ("baseplate", "Baseplate", "baseplates", baseplate()),
        ("lamp", "Lamp", "lamps", lamp()),
        ("block", "Block", "blocks", block),
        ("plain", "Plain block", "blocks", plain),
    ] {
        let added = library_command(
            cache,
            None,
            json!({"cmd": "library_add", "dir": dir, "text": text, "id": id, "name": name,
                   "category": category}),
        );
        assert_eq!(added["errors"], json!([]), "{id}: {added}");
    }
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

fn manifest_of(dir: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(dir.join("mitcad-library.json")).unwrap()).unwrap()
}

fn component<'a>(manifest: &'a Value, id: &str) -> &'a Value {
    manifest["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {manifest}"))
}

#[test]
fn mount_frames_are_written_from_the_design_and_checked() {
    let scratch = Scratch::new("library-mounts");
    let cache = LibraryCache::new(scratch.join("cache"));
    let dir = scratch.join("mounts");
    make_library(&cache, &dir);
    let manifest = manifest_of(&dir);
    assert_eq!(
        component(&manifest, "bin1x1")["mounts"],
        json!([{"name": "foot", "kind": "foot", "joint": "rigid", "pitch": [42.0, 42.0],
                "footprint": [1, 1]}])
    );
    assert_eq!(
        component(&manifest, "baseplate")["mounts"],
        json!([{"name": "grid", "kind": "grid", "joint": "rigid", "pitch": [42.0, 42.0],
                "cells": [3, 2]}])
    );
    assert_eq!(
        component(&manifest, "lamp")["mounts"],
        json!([{"name": "base", "kind": "face", "joint": "planar"}])
    );
    // A frame named `mount:base` is a face frame, fixed.
    assert_eq!(
        component(&manifest, "block")["mounts"],
        json!([{"name": "base", "kind": "face", "joint": "rigid"}])
    );
    // A part without frames has no block.
    assert!(component(&manifest, "plain").get("mounts").is_none());
    let (errors, warnings) = check(&cache, &dir);
    assert_eq!(errors, Vec::<String>::new());
    assert!(
        !warnings.iter().any(|w| w.contains("mount")),
        "{warnings:?}"
    );
    // The block survives a round trip of the manifest.
    let read: Manifest = serde_json::from_value(manifest.clone()).unwrap();
    let again: Manifest = serde_json::from_str(&serde_json::to_string(&read).unwrap()).unwrap();
    assert_eq!(again, read);
    assert!(read.component("bin2x1").unwrap().mounts.is_some());

    // Broken blocks: each rule once.
    let mut broken = manifest.clone();
    for c in broken["components"].as_array_mut().unwrap() {
        match c["id"].as_str().unwrap() {
            "bin1x1" => {
                c["mounts"] = json!([{"name": "foot", "kind": "foot", "footprint": [2, 1],
                                      "pitch": [42, 42]}]);
            }
            "bin2x1" => {
                c.as_object_mut().unwrap().remove("mounts");
            }
            "baseplate" => {
                c["mounts"] = json!([{"name": "grid", "kind": "grid", "pitch": [42, 42]}]);
            }
            "lamp" => {
                c["mounts"] = json!([{"name": "base", "kind": "face", "joint": "planar"},
                                     {"name": "base", "kind": "face", "joint": "planar"},
                                     {"name": "wall", "kind": "face"}]);
            }
            "plain" => {
                c["mounts"] = json!([{"name": "x", "kind": "pin"}]);
            }
            "block" => {
                c["mounts"] = json!([{"name": "base", "kind": "face", "joint": "cylindrical"}]);
            }
            _ => {}
        }
    }
    fs::write(dir.join("mitcad-library.json"), broken.to_string()).unwrap();
    let (errors, warnings) = check(&cache, &dir);
    let has = |text: &str| errors.iter().any(|e| e.contains(text));
    for expected in [
        "bin1x1: bins/bin1x1.mitcad: the mount frame foot is foot (rigid), 2 x 1 footprint, pitch \
         42 x 42 mm in the manifest and foot (rigid), 1 x 1 footprint, pitch 42 x 42 mm in the \
         design",
        "baseplate: mount frame grid: a grid gives its cells",
        "lamp: mount frame base is listed twice",
        "lamp: lamps/lamp.mitcad: the mount frame wall is no joint origin of the design",
        "plain: mount frame x: kind 'pin' is none of face, hole, shaft, grid, foot",
        "block: mount frame base: a face frame's joint is rigid, planar, revolute, not \
         cylindrical",
    ] {
        assert!(has(expected), "{expected}\n{errors:#?}");
    }
    assert!(
        warnings.iter().any(|w| w.contains(
            "bin2x1: bins/bin2x1.mitcad: the mount frame foot of the design is not in mounts"
        )),
        "{warnings:#?}"
    );
    // library_add refuses a manifest that its block makes invalid.
    let refused = api::command(
        &json!({"cmd": "library_add", "dir": dir, "text": bin(1), "id": "bin3",
                "name": "Bin 3", "category": "bins", "mounts": [{"name": "foot", "kind": "box"}]})
        .to_string(),
        &cache,
        None,
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("kind 'box' is none of"), "{refused}");
}

#[test]
fn parts_of_a_fetched_library_are_placed_by_their_frames() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("library-mounts-fetch");
    let cache = LibraryCache::new(scratch.join("cache"));
    let dir = scratch.join("mounts");
    make_library(&cache, &dir);
    git(&dir, &["add", "-A"]).unwrap();
    git(&dir, &["commit", "-q", "-m", "Mounts 1.0"]).unwrap();
    git(&dir, &["tag", "v1.0.0"]).unwrap();
    let url = super::remote::slashes(&dir);
    let fetched = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": url}),
    );
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    let shown = library_command(
        &cache,
        None,
        json!({"cmd": "library_show", "id": "test-mounts"}),
    );
    let bin_shown = shown["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "bin1x1")
        .unwrap()
        .clone();
    assert_eq!(bin_shown["mounts"][0]["kind"], "foot", "{bin_shown}");
    let plain = shown["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "plain")
        .unwrap()
        .clone();
    assert_eq!(plain["mounts"], Value::Null, "{plain}");

    // Placed through the resolver: the baseplate dropped in, a bin on its
    // grid's cell (1, 0).
    let resolver = Arc::new(LibraryResolver::new(cache.clone()));
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(resolver));
    let part = |component: &str| LibraryPart {
        library: "test-mounts".to_owned(),
        url: shown["url"].as_str().unwrap().to_owned(),
        rev: "v1.0.0".to_owned(),
        component: component.to_owned(),
        config: None,
        standard: String::new(),
    };
    let plate = doc
        .insert_library(&part("baseplate"), &InsertOptions::default())
        .unwrap();
    assert_eq!(plate.frames.len(), 1);
    let grid = plate.frames[0].feature;
    let mount: mitcad_model::MountInput = serde_json::from_value(json!({
        "at": {"occurrence": doc.assembly().occurrence_name(plate.occurrence),
               "geometry": grid, "point": [68, 21, 5]}}))
    .unwrap();
    let options = InsertOptions {
        mount: Some(mount),
        ..InsertOptions::default()
    };
    let bin = doc.insert_library(&part("bin1x1"), &options).unwrap();
    let plan = bin.plan.unwrap();
    assert_eq!(plan.grid.unwrap().cell, [1, 0]);
    let placed = doc.placement(bin.occurrence).unwrap();
    assert!(
        (0..3).all(|i| (placed.translation[i] - [42.0, 0.0, 5.0][i]).abs() < 1e-6),
        "{placed:?}"
    );
    assert!(bin.joint.is_some());
}
