// SPDX-License-Identifier: MIT
//! Reads every file of the local .f3d corpus (`MITCAD_F3D_CORPUS`, default
//! `~/f3d-corpus`). The corpus is not part of the repository; the test is
//! skipped when it is missing.

use std::path::{Path, PathBuf};

use mitcad_f3d::F3dFile;
use mitcad_f3d::convert::Options;

fn corpus_dir() -> Option<PathBuf> {
    let dir = match std::env::var_os("MITCAD_F3D_CORPUS") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME")?).join("f3d-corpus"),
    };
    dir.is_dir().then_some(dir)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect(&p, out);
        } else if matches!(p.extension().and_then(|e| e.to_str()), Some("f3d" | "f3z")) {
            out.push(p);
        }
    }
}

#[test]
fn corpus_files_read_and_convert() {
    let Some(dir) = corpus_dir() else {
        eprintln!("corpus not found; skipped");
        return;
    };
    let mut files = Vec::new();
    collect(&dir, &mut files);
    let (mut blobs, mut bodies, mut clean) = (0usize, 0usize, 0usize);
    for path in &files {
        let f = F3dFile::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut docs = vec![];
        for name in f.documents() {
            docs.push(
                f.open_document(&name)
                    .unwrap_or_else(|e| panic!("{}/{name}: {e}", path.display())),
            );
        }
        if docs.is_empty() {
            docs.push(f);
        }
        for doc in &docs {
            // Every entry decompresses and passes its CRC check.
            for e in doc.archive().entries() {
                doc.archive()
                    .read(e)
                    .unwrap_or_else(|err| panic!("{}: {}: {err}", path.display(), e.name));
            }
            for blob in mitcad_f3d::read_document_bodies(doc, &Options::default()) {
                let blob = blob.unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert!(
                    blob.truncated.is_none(),
                    "{}: {}: {:?}",
                    path.display(),
                    blob.entry,
                    blob.truncated
                );
                blobs += 1;
                for b in &blob.bodies {
                    bodies += 1;
                    if b.check.is_clean() && b.issues.is_empty() {
                        clean += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "{} files, {blobs} blobs, {bodies} bodies, {clean} clean",
        files.len()
    );
    // Regression guard: at the time of writing 3415 of 3424 bodies convert
    // without issues (thread surfaces included); the others have small
    // vertex gaps or edges the neutral check finds used inconsistently.
    assert!(
        clean * 1000 >= bodies * 995,
        "only {clean} of {bodies} bodies convert cleanly"
    );
}

/// The display meshes (`OGS.BlobFolder`) of every document that has them
/// parse without issues, and closed meshes have a positive volume.
#[test]
fn corpus_display_meshes() {
    let Some(dir) = corpus_dir() else {
        eprintln!("corpus not found; skipped");
        return;
    };
    let mut files = Vec::new();
    collect(&dir, &mut files);
    let (mut scenes, mut bodies, mut meshed, mut closed, mut positive) = (0, 0, 0, 0, 0);
    let mut issues = Vec::new();
    for path in &files {
        let f = F3dFile::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut docs = vec![];
        for name in f.documents() {
            docs.push(
                f.open_document(&name)
                    .unwrap_or_else(|e| panic!("{}/{name}: {e}", path.display())),
            );
        }
        if docs.is_empty() {
            docs.push(f);
        }
        for doc in &docs {
            let Some(scene) = mitcad_f3d::ogs::display_scene(doc)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
            else {
                continue;
            };
            scenes += 1;
            issues.extend(
                scene
                    .issues
                    .iter()
                    .map(|i| format!("{}: {i}", path.display())),
            );
            for b in &scene.bodies {
                bodies += 1;
                let s = b.stats();
                if s.triangles == 0 {
                    continue;
                }
                meshed += 1;
                if b.off_bbox_vertices > 0 {
                    issues.push(format!(
                        "{}: {} vertices outside their face box",
                        path.display(),
                        b.off_bbox_vertices
                    ));
                }
                if s.closed {
                    closed += 1;
                    if s.volume_mm3 > 0.0 {
                        positive += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "{scenes} display scenes, {bodies} bodies, {meshed} with a mesh, {closed} closed, {positive} closed with a positive volume"
    );
    assert!(issues.is_empty(), "{issues:#?}");
    // At the time of writing: 10 scenes, 16 bodies, 15 with a mesh, 12
    // closed, all with a positive volume.
    assert!(
        positive * 10 >= closed * 9,
        "only {positive} of {closed} closed meshes have a positive volume"
    );
}

/// The ASM history of every `.smbh` blob parses, and every bulletin pairs
/// an entity with a copy of the same record type (which checks the pointer
/// numbering of the copies, see `AsmFile::record_of`).
#[test]
fn corpus_histories() {
    use mitcad_f3d::asm::AsmFile;
    use mitcad_f3d::asm::history::History;
    let Some(dir) = corpus_dir() else {
        eprintln!("corpus not found; skipped");
        return;
    };
    let mut files = Vec::new();
    collect(&dir, &mut files);
    let (mut blobs, mut states, mut changes) = (0, 0, 0);
    let mut problems = Vec::new();
    for path in &files {
        let f = F3dFile::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut docs = vec![];
        for name in f.documents() {
            docs.push(
                f.open_document(&name)
                    .unwrap_or_else(|e| panic!("{}/{name}: {e}", path.display())),
            );
        }
        if docs.is_empty() {
            docs.push(f);
        }
        for doc in &docs {
            for blob in doc.body_blobs().into_iter().filter(|b| b.history) {
                let data = doc
                    .read(&blob.entry)
                    .unwrap_or_else(|e| panic!("{}: {e}", blob.entry));
                let file = AsmFile::parse(&data).unwrap_or_else(|e| panic!("{}: {e}", blob.entry));
                let history = match History::parse(&file) {
                    Ok(Some(h)) => h,
                    Ok(None) => {
                        problems.push(format!("{}: no history", blob.entry));
                        continue;
                    }
                    Err(e) => {
                        problems.push(format!("{}: {e}", blob.entry));
                        continue;
                    }
                };
                blobs += 1;
                states += history.states.len();
                for state in &history.states {
                    for &(before, after) in &state.bulletins {
                        if let (Some(c), Some(e)) = (before, after) {
                            changes += 1;
                            let (tc, te) = (&file.records[c].type_name, &file.records[e].type_name);
                            if tc != te {
                                problems.push(format!(
                                    "{}: copy {c} is {tc}, entity {e} {te}",
                                    blob.entry
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("{blobs} histories, {states} delta states, {changes} changed entities");
    assert!(problems.is_empty(), "{problems:#?}");
}
