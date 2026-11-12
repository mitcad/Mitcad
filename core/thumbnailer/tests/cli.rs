// SPDX-License-Identifier: MIT
//! The program as a file manager runs it (mitcad#1539): a design saved
//! with its preview, as the application's Save writes it, gives its
//! thumbnail at the size asked for; a design saved without one (as before
//! MitCad kept them) gives none and no error.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

use mitcad_model::file::{DesignPreview, FileFormat, SaveStamp};
use mitcad_model::test_support::block;
use mitcad_model::testing::{remove_temp, unique_temp_dir};

fn thumbnailer(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mitcad-thumbnailer"))
        .args(args)
        .output()
        .expect("mitcad-thumbnailer runs")
}

/// A 3D view's picture: an opaque gradient.
fn view(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[
                (x % 256) as u8,
                (y % 256) as u8,
                ((x + y) % 256) as u8,
                255,
            ]);
        }
    }
    mitcad_preview::png::encode(width, height, 4, &pixels)
}

fn size_of(path: &Path) -> Option<(u32, u32)> {
    mitcad_thumbnailer::png_size(&std::fs::read(path).ok()?)
}

#[test]
fn a_saved_design_gives_its_preview_at_the_size_asked_for() {
    let dir = unique_temp_dir("thumbnailer-saved");
    let design = dir.join("block.mitcad");
    let picture = view(256, 192);
    let b = block();
    b.doc
        .save_file_previewed(
            &design,
            FileFormat::Single,
            &SaveStamp::now(),
            Some(DesignPreview::from_png(&picture).unwrap()),
        )
        .unwrap();

    // A large thumbnail (256) and the default: the preview as it was saved.
    let large = dir.join("large.png");
    let run = thumbnailer(&[design.as_os_str(), large.as_os_str(), OsStr::new("256")]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(std::fs::read(large).unwrap(), picture);
    let default = dir.join("default.png");
    let run = thumbnailer(&[design.as_os_str(), default.as_os_str()]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(size_of(&default), Some((256, 192)));
    // Larger than the preview: never scaled up.
    let huge = dir.join("huge.png");
    let run = thumbnailer(&[design.as_os_str(), huge.as_os_str(), OsStr::new("1024")]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(size_of(&huge), Some((256, 192)));
    // A normal thumbnail (128): scaled down, the aspect ratio kept.
    let normal = dir.join("normal.png");
    let run = thumbnailer(&[design.as_os_str(), normal.as_os_str(), OsStr::new("128")]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_eq!(size_of(&normal), Some((128, 96)));
    remove_temp(&dir);
}

#[test]
fn a_design_saved_without_a_preview_gives_none_and_no_error() {
    let dir = unique_temp_dir("thumbnailer-old");
    let design = dir.join("old.mitcad");
    // As files were written before MitCad kept previews.
    block().doc.save_file(&design, FileFormat::Single).unwrap();
    assert!(
        !std::fs::read_to_string(&design)
            .unwrap()
            .contains("\"preview\"")
    );
    let out = dir.join("old.png");
    let run = thumbnailer(&[design.as_os_str(), out.as_os_str(), OsStr::new("128")]);
    assert_eq!(run.status.code(), Some(1), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert!(!out.exists());
    remove_temp(&dir);
}

#[test]
fn what_cannot_be_done_is_an_error_that_says_why() {
    let dir = unique_temp_dir("thumbnailer-errors");
    let out = dir.join("out.png");
    let missing = dir.join("missing.mitcad");
    let run = thumbnailer(&[missing.as_os_str(), out.as_os_str()]);
    assert_eq!(run.status.code(), Some(2), "{run:?}");
    assert!(String::from_utf8_lossy(&run.stderr).contains("cannot read"));

    let other = dir.join("other.mitcad");
    std::fs::write(&other, "{\"format\": \"other\"}").unwrap();
    let run = thumbnailer(&[other.as_os_str(), out.as_os_str()]);
    assert_eq!(run.status.code(), Some(2), "{run:?}");
    assert!(String::from_utf8_lossy(&run.stderr).contains("not a MitCad design"));

    let design = dir.join("block.mitcad");
    block().doc.save_file(&design, FileFormat::Single).unwrap();
    for size in ["0", "large"] {
        let run = thumbnailer(&[design.as_os_str(), out.as_os_str(), OsStr::new(size)]);
        assert_eq!(run.status.code(), Some(2), "{size}: {run:?}");
    }
    let run = thumbnailer(&[design.as_os_str()]);
    assert_eq!(run.status.code(), Some(2), "{run:?}");
    assert!(String::from_utf8_lossy(&run.stderr).contains("usage"));
    assert!(!out.exists());
    remove_temp(&dir);
}
