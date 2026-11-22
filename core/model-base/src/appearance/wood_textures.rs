// SPDX-License-Identifier: MIT
//! The texture sets of the built-in woods and panels (mitcad#2186): a base
//! colour, a normal map and a roughness map for each, 1024 × 1024 JPEG
//! images (`core/model-base/textures`, made by
//! `tools/wood-textures/generate.sh`: MitCad's own images under its
//! licence, no photographs), laid by the `grain` projection, so the grain
//! runs along each face's longest side.
//!
//! The images are files that MitCad ships once and reads when the library
//! is first used (mitcad#2218), not data built into each program: the
//! application and the command-line tool both link the model, and each
//! would carry its own copy of the 4.7 MB. [`folder`] says where they are
//! looked for.
//!
//! The appearances keep their ids, colours and parameters: the base colour
//! image's mean (in linear light) is the appearance's colour, which stays
//! the colour of exports and drawings and the look wherever textures are
//! not drawn (also when the files are not found). A design that uses one
//! of these ids gets the textures on open, without a change to its file (a
//! library appearance is never saved); a design's own copy of one keeps
//! what it has.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::{ImageData, Projection, Texture, TextureMap, TextureSource};

/// One look's id and the size of one repeat of its images (mm, square).
struct Set {
    id: &'static str,
    repeat: f64,
}

/// The sets, by appearance id. The woods repeat every 800 mm, the
/// plywoods' rotary-cut faces every 1000 mm, the panels by the size of
/// their chips and strands.
const SETS: [Set; 14] = [
    set("wood_birch", 800.0),
    set("wood_cedar", 800.0),
    set("wood_cherry", 800.0),
    set("wood_douglas_fir", 800.0),
    set("wood_eucalyptus", 800.0),
    set("wood_pine", 800.0),
    set("wood_teak", 800.0),
    set("panel_chipboard", 400.0),
    set("panel_mdf", 600.0),
    set("panel_osb", 600.0),
    set("plywood_birch", 1000.0),
    set("plywood_eucalyptus", 1000.0),
    set("plywood_pine", 1000.0),
    set("plywood_spruce", 1000.0),
];

const fn set(id: &'static str, repeat: f64) -> Set {
    Set { id, repeat }
}

/// The maps of a set, the suffixes of its file names
/// (`<id>-<map>.jpg`).
const MAPS: [&str; 3] = ["color", "normal", "roughness"];

/// The id of the source the sets' `source` names: MitCad's own images.
pub const SOURCE: &str = "mitcad";

/// The folder name the install rules give the images.
const FOLDER: &str = "textures";

/// The folders the images are looked for in, in order, relative to the
/// running program's folder: beside it (the Windows installer's
/// `bin/textures`), an installed tree's share folder (the AppImage's
/// `usr/share/mitcad/textures`, from `usr/bin`), a macOS bundle's
/// resources (`Contents/Resources/textures`, from `Contents/MacOS`).
const BESIDE_PROGRAM: [&str; 3] = [
    "textures",
    "../share/mitcad/textures",
    "../Resources/textures",
];

/// The folders [`folder`] tries, in order: those of [`BESIDE_PROGRAM`]
/// under `program_dir`, then the source tree's `core/model-base/textures`
/// (a build tree's programs and the tests, which have no installed copy).
fn candidates(program_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = program_dir
        .map(|dir| BESIDE_PROGRAM.iter().map(|f| dir.join(f)).collect())
        .unwrap_or_default();
    folders.push(Path::new(env!("CARGO_MANIFEST_DIR")).join(FOLDER));
    folders
}

/// Whether `folder` has the first set's base colour image, by which an
/// installed folder of the images is told from an unrelated one.
fn has_images(folder: &Path) -> bool {
    folder
        .join(format!("{}-{}.jpg", SETS[0].id, MAPS[0]))
        .is_file()
}

/// The folder of the images: the first of [`candidates`] that has them,
/// found once. `None` when none has (the looks then draw their colours).
fn folder() -> Option<&'static Path> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let program = std::env::current_exe().ok();
            let program_dir = program.as_deref().and_then(Path::parent);
            candidates(program_dir).into_iter().find(|f| has_images(f))
        })
        .as_deref()
}

/// The built-in texture set of a library appearance, if it has one and
/// its three images are read from [`folder`].
pub(super) fn texture(id: &str) -> Option<Texture> {
    texture_from(folder()?, id)
}

/// The texture set of `id` from the images in `folder`; `None` when `id`
/// has no set or one of its images cannot be read.
fn texture_from(folder: &Path, id: &str) -> Option<Texture> {
    let set = SETS.iter().find(|s| s.id == id)?;
    let read = |map: &str| -> Option<(String, ImageData)> {
        let path = format!("{}-{map}.jpg", set.id);
        let bytes = std::fs::read(folder.join(&path)).ok()?;
        Some((path, ImageData::from_bytes(&bytes)))
    };
    let [color, normal, roughness] = MAPS.map(read);
    let ((path, data), normal, roughness) = (color?, normal?, roughness?);
    let map = |(path, data): (String, ImageData)| TextureMap {
        path,
        data: Some(data),
        ..TextureMap::default()
    };
    Some(Texture {
        path,
        size: [set.repeat, set.repeat],
        rotation: 0.0,
        projection: Projection::Grain,
        data: Some(data),
        normal: Some(map(normal)),
        roughness: Some(map(roughness)),
        source: Some(TextureSource {
            id: SOURCE.to_owned(),
            asset: set.id.to_owned(),
            resolution: Some("1k".to_owned()),
            license: Some("MIT".to_owned()),
            url: None,
        }),
        ..Texture::default()
    })
}

/// The ids of the appearances with a built-in texture set.
pub fn textured_ids() -> impl Iterator<Item = &'static str> {
    SETS.iter().map(|s| s.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file names of every built-in image (`<id>-<map>.jpg`).
    fn file_names() -> impl Iterator<Item = String> {
        SETS.iter()
            .flat_map(|s| MAPS.iter().map(move |m| format!("{}-{m}.jpg", s.id)))
    }

    fn source_tree() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(FOLDER)
    }

    #[test]
    fn the_folders_beside_the_program_come_before_the_source_tree() {
        let dir = Path::new("prefix").join("bin");
        let folders = candidates(Some(&dir));
        assert_eq!(
            folders,
            [
                dir.join("textures"),
                dir.join("../share/mitcad/textures"),
                dir.join("../Resources/textures"),
                source_tree(),
            ]
        );
        assert_eq!(candidates(None), [source_tree()]);
    }

    #[test]
    fn every_image_is_in_the_source_tree_and_nothing_else_is() {
        let names: Vec<String> = file_names().collect();
        assert_eq!(names.len(), 42);
        for name in &names {
            assert!(source_tree().join(name).is_file(), "{name} is missing");
        }
        let mut jpegs: Vec<String> = std::fs::read_dir(source_tree())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".jpg"))
            .collect();
        jpegs.sort();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(jpegs, sorted);
        assert!(has_images(&source_tree()));
    }

    #[test]
    fn a_folder_without_the_images_gives_no_texture() {
        let empty = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        assert!(!has_images(&empty));
        assert!(texture_from(&empty, "wood_birch").is_none());
        assert!(texture_from(&source_tree(), "paint_red").is_none());
        let t = texture_from(&source_tree(), "wood_birch").expect("a set");
        assert_eq!(t.path, "wood_birch-color.jpg");
        assert_eq!(t.normal.as_ref().unwrap().path, "wood_birch-normal.jpg");
        assert_eq!(
            t.roughness.as_ref().unwrap().path,
            "wood_birch-roughness.jpg"
        );
    }

    #[test]
    fn the_tests_read_the_images_from_the_source_tree() {
        // A test program has no installed folder beside it.
        assert_eq!(folder(), Some(source_tree().as_path()));
    }
}
