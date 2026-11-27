// SPDX-License-Identifier: MIT
//! The import of a design saved as `.sc` (mitcad#1622), on the model's
//! mock kernel: a test design copied to `.sc` comes in as its `.solidcraft`
//! original does, and a `.sc` file of another program is refused with
//! "Not a SolidCraft design: …" before anything is imported.

use std::path::{Path, PathBuf};

use mitcad_import_solidcraft::{SolidcraftOptions, SolidcraftReport, import_solidcraft};
use mitcad_model::Document;
use mitcad_model::testing::MockKernel;

fn work() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("import-sc-extension");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn import(path: &Path) -> (Document<MockKernel>, Result<SolidcraftReport, String>) {
    let mut doc = Document::new(MockKernel::default());
    let report = import_solidcraft(&mut doc, path, &SolidcraftOptions::default());
    (doc, report)
}

#[test]
fn a_design_saved_as_sc_comes_in_as_the_original() {
    let dir = work();
    for design in ["box-fillet-hole", "format-1", "emboss"] {
        let bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../solidcraft/tests/designs")
                .join(format!("{design}.solidcraft.json")),
        )
        .unwrap();
        let original = dir.join(format!("{design}.solidcraft"));
        std::fs::write(&original, &bytes).unwrap();
        let (original_doc, original_report) = import(&original);
        let original_report = original_report.unwrap_or_else(|e| panic!("{design}: {e}"));
        for extension in ["sc", "SC"] {
            let copy = dir.join(format!("{design}.{extension}"));
            std::fs::write(&copy, &bytes).unwrap();
            let (doc, report) = import(&copy);
            let mut report = report.unwrap_or_else(|e| panic!("{design}.{extension}: {e}"));
            // Only the file's name differs.
            assert_eq!(report.file, format!("{design}.{extension}"));
            report.file = original_report.file.clone();
            assert_eq!(report, original_report, "{design}.{extension}");
            assert_eq!(
                doc.timeline().len(),
                original_doc.timeline().len(),
                "{design}.{extension}"
            );
        }
    }
}

#[test]
fn another_programs_sc_file_is_refused() {
    let dir = work();
    let path = dir.join("other.sc");
    std::fs::write(&path, br#"{"format": "other/1", "features": []}"#).unwrap();
    let (doc, report) = import(&path);
    let error = report.expect_err("another program's .sc file");
    assert!(error.starts_with("Not a SolidCraft design: "), "{error}");
    assert!(doc.timeline().is_empty());
}
