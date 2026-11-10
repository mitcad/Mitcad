// SPDX-License-Identifier: MIT
//! Every `.sc` or `.solidcraft` design under the folders of `MITCAD_SOLIDCRAFT_CORPUS`
//! (a path list, like PATH) reads and upgrades, and its sketches and the
//! kinds the import maps read typed. Skipped without the variable.

use std::path::{Path, PathBuf};

use mitcad_solidcraft::{SolidcraftFile, has_design_extension, kinds};

fn designs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            designs(&p, out);
        } else if has_design_extension(&p) {
            // `.sc` too (mitcad#1622), read only when its format key is
            // SolidCraft's.
            out.push(p);
        }
    }
}

#[test]
fn corpus_designs_read() {
    let Some(folders) = std::env::var_os("MITCAD_SOLIDCRAFT_CORPUS") else {
        eprintln!("SolidCraft corpus: skipped (set MITCAD_SOLIDCRAFT_CORPUS)");
        return;
    };
    let mut files = Vec::new();
    for dir in std::env::split_paths(&folders) {
        designs(&dir, &mut files);
    }
    files.sort();
    let mut failed = Vec::new();
    for f in &files {
        let result = SolidcraftFile::open(f).and_then(|file| {
            for feature in &file.design.features {
                if feature.kind == "sketch" {
                    feature.typed::<kinds::SketchFeature>()?;
                }
            }
            Ok(file)
        });
        match result {
            Ok(file) => eprintln!(
                "{}: format {}, {} features",
                f.display(),
                file.saved_format,
                file.design.features.len()
            ),
            Err(e) => failed.push(format!("{}: {e}", f.display())),
        }
    }
    assert!(
        failed.is_empty(),
        "{} of {} failed:\n{}",
        failed.len(),
        files.len(),
        failed.join("\n")
    );
}
