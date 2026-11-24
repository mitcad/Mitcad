// SPDX-License-Identifier: MIT
//! The reporter's design of GitHub mitcad/Mitcad#588 (mitcad#2299;
//! `MITCAD_MITCAD_REPORTED`/`github-588`, when it is there): a body of one
//! component split by a face of a body of another component, as the Split
//! Body panel now writes it. With the face's `tool_links` (where the other
//! component and the split's are seen) the split builds in the component of
//! the body it splits and leaves the tool's body where it is; without them
//! it is refused in words, without the body's id and without advising an
//! activation that cannot help.
//!
//! The design is rebuilt with the mock kernel, which does not rebuild every
//! component: the reporter's torso (C20) starts from a sketch that projects
//! edges of other components' bodies, whose mock edges are not where the
//! real ones are, so the plane its outline is drawn on and its body do not
//! come. The test takes the reporter's pair (the torso split by a face of
//! the head, C21) when both have bodies, else the first two components
//! placed in the root that do: what it checks is the links between the
//! design's occurrences, which the mock evaluates as the real kernel does.
//! The reporter's own split, built by the real kernel, is the ctest
//! `cli.split_body_other_component_reported`
//! (`tools/cli/split-body-other-component-reported-test.py`, mitcad#2318).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::testing::{MockKernel, remove_temp, unique_temp_dir};
use mitcad_model_api::DocumentApi;

/// The reporter's design unpacked from its report's zip into `dir`, loaded
/// with the mock kernel, and the design file as it is stored; None, and a
/// note, when the private test files are not here.
fn reported(dir: &Path) -> Option<(Document<MockKernel>, Value)> {
    let root = std::env::var("MITCAD_MITCAD_REPORTED").ok()?;
    let zip = PathBuf::from(root)
        .join("github-588")
        .join("split_body_bug.zip");
    if !zip.is_file() {
        eprintln!("skipped: {} is not here", zip.display());
        return None;
    }
    let archive = mitcad_zip::Archive::open(&zip).unwrap();
    let entry = archive
        .entries()
        .iter()
        .find(|e| e.name.ends_with(".mitcad"))
        .unwrap_or_else(|| panic!("no design in {}", zip.display()));
    let bytes = archive.read(entry).unwrap();
    let stored: Value = serde_json::from_slice(&bytes).unwrap();
    let design = dir.join("design.mitcad");
    std::fs::write(&design, &bytes).unwrap();
    let mut doc = Document::load_design(&design, MockKernel::default())
        .unwrap_or_else(|e| panic!("{}: {e}", design.display()));
    doc.recompute();
    Some((doc, stored))
}

/// The components placed in the root, each with its occurrence there, in
/// the order the design file lists them.
fn root_occurrences(stored: &Value) -> Vec<(ComponentUid, String)> {
    stored["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["parent"] == "C0")
        .map(|o| {
            (
                o["component"].as_str().unwrap().parse().unwrap(),
                o["uid"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// A body of `component` with a face, and that face's name.
fn body_with_face(
    doc: &Document<MockKernel>,
    component: ComponentUid,
) -> Option<(BodyUid, String)> {
    let mut bodies = doc.component_bodies(component);
    bodies.sort();
    bodies.into_iter().find_map(|body| {
        let face = doc.body_shape(body)?.faces.first()?.to_string();
        Some((body, face))
    })
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
}

fn added(doc: &mut Document<MockKernel>, def: Value) -> FeatureUid {
    command(doc, json!({"cmd": "add_feature", "def": def}))["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn a_body_split_by_a_face_of_another_component_builds_with_its_link() {
    let dir = unique_temp_dir("2299-reported");
    let Some((mut doc, stored)) = reported(&dir) else {
        remove_temp(&dir);
        return;
    };
    // The components the mock rebuilt bodies for, with their occurrences.
    let built: Vec<(ComponentUid, String, BodyUid, String)> = root_occurrences(&stored)
        .into_iter()
        .filter_map(|(component, occurrence)| {
            let (body, face) = body_with_face(&doc, component)?;
            Some((component, occurrence, body, face))
        })
        .collect();
    let find = |c: u32| built.iter().find(|b| b.0 == ComponentUid(c));
    let (own, here, target, other, there, tool, face) = match (find(20), find(21)) {
        // The reporter's: the torso split by a face of the head.
        (Some(t), Some(h)) => (t.0, t.1.clone(), t.2, h.0, h.1.clone(), h.2, h.3.clone()),
        _ => {
            assert!(
                built.len() >= 2,
                "the mock rebuilt bodies with faces in fewer than two components placed in \
                 the root: {:?}",
                built.iter().map(|b| b.0).collect::<Vec<_>>()
            );
            let (t, h) = (&built[0], &built[1]);
            (t.0, t.1.clone(), t.2, h.0, h.1.clone(), h.2, h.3.clone())
        }
    };
    assert_ne!(own, other);
    // The split goes into the component of the body it splits.
    doc.activate_component(own).unwrap();
    let split = |links: Value| {
        json!({"type": "split_body", "bodies": [target],
               "tool": {"body": tool, "face": face}, "tool_links": links})
    };

    // Without the link, as the panel wrote it before: refused in words.
    let unlinked = added(&mut doc, split(json!({})));
    let status = doc.status(unlinked).unwrap().clone();
    let error = status.error().unwrap_or_default().to_owned();
    assert!(
        error.contains("the splitting tool is in ")
            && error.contains("another component")
            && !error.contains("activate")
            && !error.contains(&tool.to_string()),
        "{status:?}"
    );
    doc.undo();

    // With the link: built in the body's component, the tool's body left
    // in its own.
    let before = doc.component_bodies(own).len();
    let links = json!({tool.to_string(): {"source": there, "target": here}});
    let uid = added(&mut doc, split(links.clone()));
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.feature(uid).unwrap().component, own);
    assert_eq!(doc.body_component(target), Some(own));
    assert_eq!(doc.body_component(tool), Some(other));
    assert_eq!(doc.component_bodies(own).len(), before + 1);
    let def: Value = serde_json::from_str(
        &doc.query(&json!({"query": "feature", "uid": uid.to_string()}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(def["def"]["tool_links"], links, "{def}");
    remove_temp(&dir);
}
