// SPDX-License-Identifier: MIT
//! The `.sc` extension SolidCraft saves designs as now (mitcad#1622). Other
//! programs use `.sc` too, so a `.sc` file is read only when its format key
//! says it is a SolidCraft design; anything else is refused with "Not a
//! SolidCraft design: …". The designs are this crate's test designs
//! (`tests/designs`), copied to `.sc`.

use std::path::{Path, PathBuf};

use mitcad_solidcraft::{
    SolidcraftFile, check_design, check_file, has_design_extension, has_shared_extension,
};
use mitcad_zip::stored_zip;

const DESIGN: &str = include_str!("designs/box-fillet-hole.solidcraft.json");

fn designs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/designs")
}

/// A folder of this test's own under cargo's scratch folder for tests.
fn work(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("sc-extension")
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn same(a: &SolidcraftFile, b: &SolidcraftFile) -> bool {
    a.design == b.design
        && a.saved_format == b.saved_format
        && a.saved_pending == b.saved_pending
        && a.upgrades == b.upgrades
        && a.container == b.container
}

#[test]
fn the_extensions_are_known_in_any_case() {
    for name in [
        "a.sc",
        "a.SC",
        "a.Sc",
        "a.solidcraft",
        "a.SolidCraft",
        "a.solvecraft",
    ] {
        assert!(has_design_extension(Path::new(name)), "{name}");
    }
    for name in ["a.scad", "a.scx", "a.json", "sc", "a.mitcad"] {
        assert!(!has_design_extension(Path::new(name)), "{name}");
    }
    assert!(has_shared_extension(Path::new("a.SC")));
    assert!(!has_shared_extension(Path::new("a.solidcraft")));
}

/// Every test design copied to `.sc` (and `.SC`) reads as its original.
#[test]
fn every_test_design_reads_the_same_as_sc() {
    let dir = work("designs");
    let mut count = 0;
    for entry in std::fs::read_dir(designs_dir()).unwrap() {
        let path = entry.unwrap().path();
        let Some(stem) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".solidcraft.json"))
            .map(str::to_owned)
        else {
            continue;
        };
        let bytes = std::fs::read(&path).unwrap();
        let original = write(&dir, &format!("{stem}.solidcraft"), &bytes);
        let original =
            SolidcraftFile::open(&original).unwrap_or_else(|e| panic!("{stem}.solidcraft: {e}"));
        for extension in ["sc", "SC"] {
            let copy = write(&dir, &format!("{stem}.{extension}"), &bytes);
            assert_eq!(check_file(&copy), Ok(()), "{stem}.{extension}");
            let read =
                SolidcraftFile::open(&copy).unwrap_or_else(|e| panic!("{stem}.{extension}: {e}"));
            assert!(same(&original, &read), "{stem}.{extension}");
        }
        count += 1;
    }
    assert!(count >= 10, "only {count} test designs");
}

/// A design container saved as `.sc` reads as one saved as `.solidcraft`.
#[test]
fn a_container_reads_the_same_as_sc() {
    let dir = work("container");
    let bytes = stored_zip(&[
        (
            "container.json",
            br#"{"format":"solidcraft-container/1"}"#.as_slice(),
        ),
        ("document.json", DESIGN.as_bytes()),
    ]);
    let original = SolidcraftFile::open(&write(&dir, "box.solidcraft", &bytes)).unwrap();
    let copy = write(&dir, "box.sc", &bytes);
    assert_eq!(check_file(&copy), Ok(()));
    let read = SolidcraftFile::open(&copy).unwrap();
    assert!(read.container);
    assert!(same(&original, &read));
}

/// The format key is the top-level one, wherever it comes among the keys;
/// one inside a nested object does not count.
#[test]
fn the_format_key_is_the_top_level_one() {
    let later = DESIGN.replacen(
        "{",
        r#"{"note": {"format": "other/1", "list": ["a", "b\"c"]}, "tags": [1, {"format": 2}],"#,
        1,
    );
    assert_eq!(check_design(later.as_bytes()), Ok(()));
    let nested = r#"{"note": {"format": "solidcraft/27"}, "name": "A"}"#;
    let error = check_design(nested.as_bytes()).unwrap_err();
    assert!(error.contains("no `format` key"), "{error}");
    // The legacy format name too.
    assert_eq!(
        check_design(br#"{"format": "solvecraft/12", "features": []}"#),
        Ok(())
    );
}

/// Files of other programs saved as `.sc` are refused by their content,
/// with "Not a SolidCraft design: …", by `check_file` and by `open` alike.
#[test]
fn other_sc_files_are_refused() {
    let dir = work("others");
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "script.sc",
            b"// a script of another program\nx = 1;\n".to_vec(),
            "not a JSON object",
        ),
        (
            "other.sc",
            br#"{"format": "other/1", "name": "A"}"#.to_vec(),
            "format is `other/1`",
        ),
        (
            "no-format.sc",
            br#"{"name": "A", "params": [], "features": []}"#.to_vec(),
            "no `format` key",
        ),
        (
            "number.sc",
            br#"{"format": 27, "name": "A"}"#.to_vec(),
            "not a string",
        ),
        (
            "array.sc",
            br#"[{"format": "solidcraft/27"}]"#.to_vec(),
            "not a JSON object",
        ),
        (
            "drawing.sc",
            br#"{"format": "solidcraft-drawing/3", "sheets": []}"#.to_vec(),
            "SolidCraft drawing",
        ),
        (
            "archive.sc",
            stored_zip(&[("readme.txt", b"another archive".as_slice())]),
            "zip archive",
        ),
        (
            "other-container.sc",
            stored_zip(&[
                (
                    "container.json",
                    br#"{"format":"other-container/1"}"#.as_slice(),
                ),
                ("document.json", DESIGN.as_bytes()),
            ]),
            "zip archive",
        ),
        ("empty.sc", Vec::new(), "not a JSON object"),
        ("binary.sc", vec![0xff, 0xfe, 0, 0, 7], "not a JSON object"),
    ];
    for (name, bytes, reason) in cases {
        let path = write(&dir, name, &bytes);
        let refused = check_file(&path).expect_err(name);
        assert!(
            refused.starts_with("Not a SolidCraft design: ") && refused.contains(reason),
            "{name}: {refused}"
        );
        let opened = SolidcraftFile::open(&path).expect_err(name);
        assert_eq!(opened, refused, "{name}");
    }
    // The same content as `.solidcraft` is not checked by its key: the
    // reader says what is wrong with it (a design without a format key is
    // a format 1 design there).
    let legacy = write(
        &dir,
        "no-format.solidcraft",
        br#"{"name": "A", "params": [], "features": []}"#,
    );
    assert_eq!(check_file(&legacy), Ok(()));
    assert_eq!(SolidcraftFile::open(&legacy).unwrap().saved_format, 1);
}
