// SPDX-License-Identifier: MIT
//! The library of Flex types (mitcad#1713; `commands.md`, "Flex types"):
//! the items a Flex feature is made of, kept apart from any design. There
//! are the built-in types ([`built_in`]: generic round wires, cables and
//! hoses of common sizes, flat cables and strips), the user's own and the
//! project's. The user's and the project's live as files beside the
//! materials of those libraries ([`crate::print_library::user_library`],
//! [`crate::print_library::project_library`]), one file for a type:
//! `flex-types/<id>.json`, the type with the file's format and the type's
//! version ([`FlexTypeFile`]).
//!
//! A design never refers to a type: a Flex item holds a copy
//! ([`LibraryFlexType::copy`]) with its origin, so a design opens the same
//! without the library. The commands that write the files, each an undo
//! step that writes the file back, are the document's
//! (`document/flex.rs`).

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::flex::{FlexSection, FlexType, FlexTypeOrigin};

/// The folder of Flex types in a user's or a project's library.
pub const FLEX_TYPES_DIR: &str = "flex-types";
/// The format a type's file names.
pub const FLEX_TYPE_FORMAT: &str = "mitcad-flex-type";
/// The largest type file read.
const MAX_FILE: u64 = 1 << 20;

/// The library of the built-in types, as an item's origin names it.
pub const BUILT_IN: &str = "built_in";
/// The user's library.
pub const USER: &str = "user";
/// The project's library.
pub const PROJECT: &str = "project";

/// A type's file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlexTypeFile {
    /// [`FLEX_TYPE_FORMAT`].
    pub format: String,
    /// 1 when the type is saved, one more for every change of it.
    pub version: u32,
    /// The type itself, without an origin.
    #[serde(rename = "type")]
    pub item: FlexType,
}

/// A type of a library.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryFlexType {
    pub id: String,
    /// [`BUILT_IN`], [`USER`] or [`PROJECT`].
    pub library: &'static str,
    pub version: u32,
    /// The type, without an origin.
    pub item: FlexType,
}

impl LibraryFlexType {
    pub fn is_built_in(&self) -> bool {
        self.library == BUILT_IN
    }

    /// The copy a design's item takes: the type with where it came from.
    pub fn copy(&self) -> FlexType {
        FlexType {
            origin: Some(FlexTypeOrigin {
                library: self.library.to_owned(),
                id: self.id.clone(),
                version: Some(self.version),
            }),
            ..self.item.clone()
        }
    }

    /// Whether the type's name or id holds every word of `text`, whatever
    /// the case; every type for a text without words.
    pub fn matches(&self, text: &str) -> bool {
        let name = self.item.name.to_lowercase();
        let id = self.id.to_lowercase();
        text.split_whitespace()
            .map(str::to_lowercase)
            .all(|word| name.contains(&word) || id.contains(&word))
    }
}

fn round(
    id: &str,
    name: &str,
    diameter: f64,
    wall: Option<f64>,
    bend_radius: f64,
    mass_per_length: f64,
) -> LibraryFlexType {
    LibraryFlexType {
        id: id.to_owned(),
        library: BUILT_IN,
        version: 1,
        item: FlexType {
            name: name.to_owned(),
            section: FlexSection::Round { diameter },
            wall,
            bend_radius: Some(bend_radius),
            bend_radius_edge: None,
            max_twist: None,
            appearance: None,
            material: None,
            mass_per_length: Some(mass_per_length),
            origin: None,
        },
    }
}

fn flat(
    id: &str,
    name: &str,
    [width, height, corner_radius]: [f64; 3],
    bend_radius: f64,
    max_twist: f64,
    mass_per_length: f64,
) -> LibraryFlexType {
    LibraryFlexType {
        id: id.to_owned(),
        library: BUILT_IN,
        version: 1,
        item: FlexType {
            name: name.to_owned(),
            section: FlexSection::Rectangular {
                width,
                height,
                corner_radius,
            },
            wall: None,
            // Over its flat side only: a flat item does not bend over its
            // edge.
            bend_radius: Some(bend_radius),
            bend_radius_edge: None,
            max_twist: Some(max_twist),
            appearance: None,
            material: None,
            mass_per_length: Some(mass_per_length),
            origin: None,
        },
    }
}

static BUILT_IN_TYPES: LazyLock<Vec<LibraryFlexType>> = LazyLock::new(|| {
    vec![
        // Single wires: the outer diameter with the insulation; the bend
        // radius four diameters.
        round("wire-1", "Wire 1 mm", 1.0, None, 4.0, 2.0),
        round("wire-1-5", "Wire 1.5 mm", 1.5, None, 6.0, 4.0),
        round("wire-2", "Wire 2 mm", 2.0, None, 8.0, 7.0),
        round("wire-3", "Wire 3 mm", 3.0, None, 12.0, 16.0),
        round("wire-4", "Wire 4 mm", 4.0, None, 16.0, 28.0),
        // Sheathed cables: six diameters.
        round("cable-5", "Cable 5 mm", 5.0, None, 30.0, 40.0),
        round("cable-6", "Cable 6 mm", 6.0, None, 36.0, 55.0),
        round("cable-8", "Cable 8 mm", 8.0, None, 48.0, 95.0),
        round("cable-10", "Cable 10 mm", 10.0, None, 60.0, 150.0),
        round("cable-12", "Cable 12 mm", 12.0, None, 72.0, 210.0),
        // Hoses: hollow, by their outer diameter.
        round("hose-4", "Hose 4 mm", 4.0, Some(0.75), 15.0, 9.0),
        round("hose-6", "Hose 6 mm", 6.0, Some(1.0), 25.0, 19.0),
        round("hose-8", "Hose 8 mm", 8.0, Some(1.25), 35.0, 32.0),
        round("hose-10", "Hose 10 mm", 10.0, Some(1.5), 45.0, 48.0),
        round("hose-12", "Hose 12 mm", 12.0, Some(2.0), 60.0, 75.0),
        round("hose-16", "Hose 16 mm", 16.0, Some(2.5), 90.0, 125.0),
        round("hose-19", "Hose 19 mm", 19.0, Some(3.0), 120.0, 180.0),
        round("hose-25", "Hose 25 mm", 25.0, Some(3.5), 160.0, 280.0),
        // Flat cables and strips: bent over the flat side.
        flat(
            "flat-cable-12",
            "Flat cable 12 mm",
            [12.0, 1.0, 0.0],
            6.0,
            0.05,
            25.0,
        ),
        flat(
            "flat-cable-25",
            "Flat cable 25 mm",
            [25.0, 1.0, 0.0],
            6.0,
            0.03,
            50.0,
        ),
        flat(
            "strip-10",
            "Strip 10 mm",
            [10.0, 2.0, 0.0],
            20.0,
            0.02,
            30.0,
        ),
        flat(
            "strip-10x5",
            "Strip 10 x 5 mm",
            [10.0, 5.0, 1.0],
            25.0,
            0.02,
            45.0,
        ),
    ]
});

/// The built-in types: generic round wires, cables and hoses of common
/// sizes, flat cables and strips.
pub fn built_in() -> &'static [LibraryFlexType] {
    &BUILT_IN_TYPES
}

/// A built-in type by its id.
pub fn built_in_type(id: &str) -> Option<&'static LibraryFlexType> {
    built_in().iter().find(|t| t.id == id)
}

/// A library a command names: [`BUILT_IN`], [`USER`] or [`PROJECT`].
pub fn library_name(text: &str) -> Result<&'static str, String> {
    match text.trim() {
        "built_in" => Ok(BUILT_IN),
        "user" => Ok(USER),
        "project" => Ok(PROJECT),
        other => Err(format!(
            "“{other}” is no library of flex types: built_in, user or project"
        )),
    }
}

/// The folder of a library's types: the user's ([`USER`]) or the
/// project's ([`PROJECT`], given the project's folder). The built-in types
/// have none.
pub fn folder(library: &str, project: Option<&Path>) -> Result<PathBuf, String> {
    match library_name(library)? {
        USER => crate::print_library::user_library()
            .map(|folder| folder.join(FLEX_TYPES_DIR))
            .ok_or_else(|| "there is no user library folder".to_owned()),
        PROJECT => project
            .map(|p| crate::print_library::project_library(p).join(FLEX_TYPES_DIR))
            .ok_or_else(|| "the design is in no project".to_owned()),
        _ => Err("the built-in types are MitCad's and cannot change: \
                  save a copy to the user's or the project's library"
            .to_owned()),
    }
}

/// Whether `id` can name a type's file: lower-case letters, digits, `-`
/// and `_`, as the ids made from names are.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The path of the type `id` in a library's folder of types.
pub fn type_path(folder: &Path, id: &str) -> PathBuf {
    folder.join(format!("{id}.json"))
}

/// The text of a type's file.
pub fn file_text(item: &FlexType, version: u32) -> String {
    let file = FlexTypeFile {
        format: FLEX_TYPE_FORMAT.to_owned(),
        version,
        item: FlexType {
            origin: None,
            ..item.clone()
        },
    };
    let mut text = serde_json::to_string_pretty(&file).expect("a flex type writes as JSON");
    text.push('\n');
    text
}

/// A type file's data read, its id from its file's name.
pub fn parse_file(
    path: &Path,
    data: &[u8],
    library: &'static str,
) -> Result<LibraryFlexType, String> {
    let name = path.display();
    let file: FlexTypeFile =
        serde_json::from_slice(data).map_err(|e| format!("{name}: not a flex type: {e}"))?;
    if file.format != FLEX_TYPE_FORMAT {
        return Err(format!("{name}: its format is not {FLEX_TYPE_FORMAT}"));
    }
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|id| valid_id(id))
        .ok_or_else(|| format!("{name}: its name is not a flex type's id"))?;
    file.item.check().map_err(|e| format!("{name}: {e}"))?;
    Ok(LibraryFlexType {
        id: id.to_owned(),
        library,
        version: file.version,
        item: FlexType {
            origin: None,
            ..file.item
        },
    })
}

/// The types of a library's folder, sorted by name; what could not be
/// read goes to `problems`.
pub fn read_folder(
    folder: &Path,
    library: &'static str,
    problems: &mut Vec<String>,
) -> Vec<LibraryFlexType> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| e.metadata().is_ok_and(|m| m.len() <= MAX_FILE))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        match std::fs::read(&path)
            .map_err(|e| format!("{}: {e}", path.display()))
            .and_then(|data| parse_file(&path, &data, library))
        {
            Ok(found) => out.push(found),
            Err(problem) => problems.push(problem),
        }
    }
    out.sort_by(|a, b| a.item.name.cmp(&b.item.name).then(a.id.cmp(&b.id)));
    out
}

/// Every type a design can choose from: the built-in ones, then the
/// user's and the project's (its folder given); and what could not be
/// read.
pub fn list(project: Option<&Path>) -> (Vec<LibraryFlexType>, Vec<String>) {
    let user = folder(USER, None).ok();
    let project = folder(PROJECT, project).ok();
    list_in(user.as_deref(), project.as_deref())
}

/// [`list`] from these folders of the user's and the project's types.
pub fn list_in(user: Option<&Path>, project: Option<&Path>) -> (Vec<LibraryFlexType>, Vec<String>) {
    let mut problems = Vec::new();
    let mut out = built_in().to_vec();
    for (library, folder) in [(USER, user), (PROJECT, project)] {
        if let Some(folder) = folder {
            out.extend(read_folder(folder, library, &mut problems));
        }
    }
    (out, problems)
}

/// The type `id` of `library`; of the nearest library that has it when
/// none is named: the project's, then the user's, then the built-in ones.
pub fn find(
    library: Option<&str>,
    id: &str,
    project: Option<&Path>,
) -> Result<LibraryFlexType, String> {
    let none = || {
        crate::notice::said(
            "there is no such flex type",
            format!("there is no flex type {id}"),
        )
    };
    let read = |library: &'static str| -> Option<LibraryFlexType> {
        if !valid_id(id) {
            return None;
        }
        let path = type_path(&folder(library, project).ok()?, id);
        let data = std::fs::read(&path).ok()?;
        parse_file(&path, &data, library).ok()
    };
    match library {
        Some(library) => match library_name(library)? {
            BUILT_IN => built_in_type(id).cloned(),
            library => read(library),
        },
        None => read(PROJECT)
            .or_else(|| read(USER))
            .or_else(|| built_in_type(id).cloned()),
    }
    .ok_or_else(none)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_types_can_be_built_and_have_ids_of_their_own() {
        let all = built_in();
        assert!(all.len() >= 10);
        for (i, t) in all.iter().enumerate() {
            assert!(t.item.check().is_ok(), "{}: {:?}", t.id, t.item.check());
            assert!(valid_id(&t.id), "{}", t.id);
            assert!(t.is_built_in());
            assert!(all[..i].iter().all(|other| other.id != t.id), "{}", t.id);
            assert!(
                all[..i].iter().all(|other| other.item.name != t.item.name),
                "{}",
                t.item.name
            );
            // Generic: a kind and a size, nothing else.
            assert!(t.item.origin.is_none());
        }
        // Wires and hoses, solid and hollow, a flat cable and a strip.
        let hose = built_in_type("hose-10").unwrap();
        assert_eq!(hose.item.section, FlexSection::Round { diameter: 10.0 });
        assert_eq!(hose.item.wall, Some(1.5));
        assert!(built_in_type("wire-2").unwrap().item.wall.is_none());
        let flat = built_in_type("flat-cable-12").unwrap();
        assert!(!flat.item.section.is_round());
        assert!(flat.item.bend_radius.is_some());
        assert!(flat.item.bend_radius_edge.is_none());
        assert!(built_in_type("strip-10").is_some());
        assert!(built_in_type("hose-11").is_none());
    }

    #[test]
    fn a_copy_names_where_it_came_from() {
        let copy = built_in_type("hose-10").unwrap().copy();
        assert_eq!(
            copy.origin,
            Some(FlexTypeOrigin {
                library: "built_in".to_owned(),
                id: "hose-10".to_owned(),
                version: Some(1),
            })
        );
        assert_eq!(copy.name, "Hose 10 mm");
    }

    #[test]
    fn a_text_finds_the_types_that_hold_its_every_word() {
        let found = |text: &str| -> Vec<&str> {
            built_in()
                .iter()
                .filter(|t| t.matches(text))
                .map(|t| t.id.as_str())
                .collect()
        };
        assert_eq!(found("hose 10"), ["hose-10"]);
        assert_eq!(found("FLAT"), ["flat-cable-12", "flat-cable-25"]);
        assert_eq!(found("  "), found(""));
        assert_eq!(found("").len(), built_in().len());
        assert!(found("hose wire").is_empty());
    }

    #[test]
    fn a_type_is_written_and_read_as_a_file_of_its_library() {
        let root = crate::testing::unique_temp_path("flex-types");
        let folder = root.join(FLEX_TYPES_DIR);
        std::fs::create_dir_all(&folder).unwrap();
        let hose = built_in_type("hose-10").unwrap();
        // A copy's origin is not written: the file is the type itself.
        let text = file_text(&hose.copy(), 3);
        assert!(!text.contains("origin"), "{text}");
        std::fs::write(type_path(&folder, "shop-hose"), &text).unwrap();
        std::fs::write(folder.join("broken.json"), "{").unwrap();
        std::fs::write(folder.join("notes.txt"), "not a type").unwrap();
        let (all, problems) = list_in(None, Some(&folder));
        assert_eq!(all.len(), built_in().len() + 1);
        let own = all.last().unwrap();
        assert_eq!(own.id, "shop-hose");
        assert_eq!(own.library, PROJECT);
        assert_eq!(own.version, 3);
        assert_eq!(own.item, hose.item);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("broken.json"), "{problems:?}");
        // The project's type by its id, and nearest when no library is named.
        let found = find(Some("project"), "shop-hose", Some(&root.join("no"))).unwrap_err();
        assert!(found.starts_with("there is no such flex type"), "{found}");
        assert!(find(Some("built_in"), "hose-10", None).is_ok());
        assert!(find(Some("shelf"), "hose-10", None).is_err());
        crate::testing::remove_temp(&root);
    }

    #[test]
    fn ids_are_file_names() {
        assert!(valid_id("hose-10"));
        assert!(valid_id("my_hose2"));
        for id in ["", "Hose", "a/b", "..", "a b", "a.json"] {
            assert!(!valid_id(id), "{id}");
        }
    }
}
