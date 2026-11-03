// SPDX-License-Identifier: MIT
//! Materials and print profiles where they live (mitcad#655), on top of
//! the libraries and the design's copies of #640
//! (`document/print_library.rs`):
//!
//! - **One list of choosable items** (`library_items`): the built-in
//!   materials, the nearest of each library item and the design's items
//!   that are in no library, an item and the design's copy of it as one
//!   row, showing the copy's fields (what the design uses).
//! - **Edited where it lives**: an item of the user's or the project's
//!   library is made, edited and deleted in its file, its version counting
//!   up with each write. An edit is applied key by key on top of the
//!   library's newest version, and the design's copy takes the same change.
//!   The file and the copy are one undo step: undo and redo write the file
//!   back ([`FileWrite`]).
//! - **Changed elsewhere**: a copy whose library item another design (or
//!   a hand) changed since is marked `changed`, until the design takes the
//!   change (`update_from_print_library`) or keeps its own values
//!   (`decline_print_library_update`, remembered per library version).
//! - **Chosen in one step**: choosing a library's item for a body, a
//!   component or the design's printer and process copies it in, or
//!   reuses the design's copy, in the same undo step as the choice.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{DocState, Document, ModelError, invalid};
use crate::appearance::{Appearance, library_appearance};
use crate::ids::ComponentUid;
use crate::kernel::Kernel;
use crate::material::{self, Material, MaterialChange, material_class};
use crate::print_library::{
    ItemData, ItemKind, Libraries, LibraryItem, LibraryKind, carried, material_item,
};
use crate::print_profile::{
    PROFILE_KEY, PrintProfile, ProfileChange, ProfileKind, copy_name, free_id, new_item_name, slug,
};
use crate::properties::{PropertyOwner, PropertyValue, same_name};

/// A library a command names: `base`, `user` or `project`.
pub fn library_kind(text: &str) -> Result<LibraryKind, ModelError> {
    LibraryKind::parse(text)
        .ok_or_else(|| invalid(format!("“{text}” is no library: base, user or project")))
}

/// A library a command writes in: the user's or the project's; the base
/// library is MitCad's and read-only.
pub fn writable_library(text: &str) -> Result<LibraryKind, ModelError> {
    match library_kind(text)? {
        LibraryKind::Base => Err(invalid(
            "the base library is MitCad's and read-only: duplicate its item into the user's or the project's library",
        )),
        library => Ok(library),
    }
}

/// An item of a library by kind and id.
pub(crate) fn library_item<'a>(
    libraries: &'a Libraries,
    library: LibraryKind,
    kind: ItemKind,
    id: &str,
) -> Result<&'a LibraryItem, ModelError> {
    libraries.find(Some(library), kind, id).ok_or_else(|| {
        invalid(format!(
            "the {} library has no {} {id}",
            library.id(),
            kind.id()
        ))
    })
}

/// The folder of the user's or the project's library.
pub(crate) fn library_folder(
    libraries: &Libraries,
    library: LibraryKind,
) -> Result<PathBuf, ModelError> {
    if library == LibraryKind::Base {
        return Err(invalid(
            "the base library is MitCad's component library: save to the user's or the project's",
        ));
    }
    libraries
        .folder(library)
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            invalid(match library {
                LibraryKind::Project => "the design is in no project",
                _ => "there is no user library folder",
            })
        })
}

/// A library file a command writes (mitcad#655): its content before the
/// command and after it, None where there is no file. Undo writes the
/// first back and redo the second, each only while the file still holds
/// what the other side left: a file another design saved since stays as
/// it is.
#[derive(Clone, Debug)]
pub(crate) struct FileWrite {
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
}

impl FileWrite {
    /// Writing `after` at `path` (None: removing the file), the file's
    /// content now being the one before.
    pub(super) fn new(path: PathBuf, after: Option<Vec<u8>>) -> Self {
        let before = std::fs::read(&path).ok();
        Self {
            path,
            before,
            after,
        }
    }

    /// Writing an item's file into a library folder, at the item's place.
    pub(super) fn item(folder: &Path, data: &ItemData) -> Self {
        let path = folder.join(crate::print_library::item_path(data.kind(), data.id()));
        Self::new(path, Some(data.text().into_bytes()))
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

/// Puts a file's content in place, or removes the file (None).
fn put_file(path: &Path, content: Option<&[u8]>) -> Result<(), ModelError> {
    match content {
        Some(bytes) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| invalid(format!("cannot make {}: {e}", parent.display())))?;
            }
            std::fs::write(path, bytes)
                .map_err(|e| invalid(format!("cannot write {}: {e}", path.display())))
        }
        None => match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(invalid(format!("cannot remove {}: {e}", path.display())))
            }
            _ => Ok(()),
        },
    }
}

/// A file's content on the side written from and the side written to:
/// undo (`back`) writes the content before, redo the content after.
fn sides(file: &FileWrite, back: bool) -> (&Option<Vec<u8>>, &Option<Vec<u8>>) {
    if back {
        (&file.after, &file.before)
    } else {
        (&file.before, &file.after)
    }
}

/// Writes library files for undo (`back`: their content before the
/// command, the last first) or for redo and the command itself (their
/// content after). A file that no longer holds the other side's content
/// is left as it is. On an error the files written go back.
pub(super) fn write_files(files: &[FileWrite], back: bool) -> Result<(), ModelError> {
    let order: Vec<&FileWrite> = if back {
        files.iter().rev().collect()
    } else {
        files.iter().collect()
    };
    let mut done: Vec<&FileWrite> = Vec::new();
    for file in order {
        let (from, to) = sides(file, back);
        if std::fs::read(&file.path).ok() != *from {
            continue;
        }
        if let Err(error) = put_file(&file.path, to.as_deref()) {
            for written in done.iter().rev() {
                let (from, _) = sides(written, back);
                let _ = put_file(&written.path, from.as_deref());
            }
            return Err(error);
        }
        done.push(file);
    }
    Ok(())
}

/// Whether a design copy's origin is a library's item.
fn copied_from(origin: Option<&material::MaterialOrigin>, library: LibraryKind, id: &str) -> bool {
    origin.is_some_and(|o| o.library == library.id() && o.id == id)
}

/// `name`, else `name 2`, `name 3`, ... the first that `taken` does not
/// hold.
pub(super) fn free_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_owned();
    }
    (2..)
        .map(|n| format!("{name} {n}"))
        .find(|candidate| !taken(candidate))
        .expect("a free name")
}

/// The appearances a library material carries for its list (mitcad#1067),
/// in the list's order: a library appearance needs none, one the item
/// carries stays, a design's appearance goes with it. None when its
/// default is no such appearance; another of its list that is none is
/// carried by none.
fn item_appearances(
    material: &Material,
    carried: &[Appearance],
    design: &[Appearance],
) -> Option<Vec<Appearance>> {
    let mut list = Vec::new();
    for id in material.appearance_list() {
        if library_appearance(&id).is_some() {
            continue;
        }
        let found = carried.iter().chain(design).find(|a| a.id == id).cloned();
        match found {
            Some(look) => list.push(look),
            None if material.appearance.as_deref() == Some(id.as_str()) => return None,
            None => {}
        }
    }
    Some(list)
}

impl DocState {
    /// The ids of the design's copies of a library's item.
    pub(crate) fn copies_of(&self, library: LibraryKind, kind: ItemKind, id: &str) -> Vec<String> {
        match kind {
            ItemKind::Material => self
                .materials
                .iter()
                .filter(|m| copied_from(m.origin.as_ref(), library, id))
                .map(|m| m.id.clone())
                .collect(),
            _ => self
                .print_profiles
                .iter()
                .filter(|p| {
                    ItemKind::of_profile(p.kind) == kind
                        && copied_from(p.origin.as_ref(), library, id)
                })
                .map(|p| p.id.clone())
                .collect(),
        }
    }

    /// The owners (components, occurrences, bodies) that chose a profile.
    pub(crate) fn profile_owners(&self, kind: ProfileKind, id: &str) -> Vec<PropertyOwner> {
        let chosen = PropertyValue::Text(id.to_owned());
        let set = kind.set().property_set();
        self.properties
            .owners
            .keys()
            .copied()
            .filter(|o| {
                self.properties
                    .get(*o, set, PROFILE_KEY)
                    .is_some_and(|p| p.value == chosen)
            })
            .collect()
    }

    /// The profile of a kind the design (the root) chose.
    pub(crate) fn design_profile(&self, kind: ProfileKind) -> Option<String> {
        let property = self.properties.get(
            PropertyOwner::Component(ComponentUid::ROOT),
            kind.set().property_set(),
            PROFILE_KEY,
        )?;
        match &property.value {
            PropertyValue::Text(id) => Some(id.trim().to_owned()),
            _ => None,
        }
    }
}

impl<K: Kernel> Document<K> {
    /// Runs `edit`, commands that recompute nothing, as one undo step named
    /// as its last step; on an error nothing it did stays.
    pub fn as_one_step<T>(
        &mut self,
        edit: impl FnOnce(&mut Self) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        let depth = self.undo.len();
        let redo = self.redo.clone();
        match edit(self) {
            Ok(value) => {
                if self.undo.len() > depth + 1 {
                    let label = self.undo_label().unwrap_or_default().to_owned();
                    self.merge_undo(depth, &label);
                }
                Ok(value)
            }
            Err(error) => {
                if self.undo.len() > depth {
                    // The library files written go back too (mitcad#655).
                    let files: Vec<FileWrite> = self.undo[depth..]
                        .iter_mut()
                        .flat_map(|step| std::mem::take(&mut step.files))
                        .collect();
                    let _ = write_files(&files, true);
                    self.undo.truncate(depth + 1);
                    let step = self.undo.pop().expect("the first step");
                    self.state = step.state;
                    self.revision = step.revision;
                    self.redo = redo;
                    self.preview = None;
                }
                Err(error)
            }
        }
    }

    /// Writes library files and makes `edit` one undo step named `label`
    /// (mitcad#655): undo writes the files back, redo writes them again.
    /// When the design refuses the edit, the files go back.
    pub(super) fn with_files<T>(
        &mut self,
        label: String,
        files: Vec<FileWrite>,
        edit: impl FnOnce(&mut DocState) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        write_files(&files, false)?;
        match self.apply_with(move |state| edit(state).map(|value| (label, value, false))) {
            Ok(value) => {
                if let Some(step) = self.undo.last_mut() {
                    step.files = files;
                }
                Ok(value)
            }
            Err(error) => {
                let _ = write_files(&files, true);
                Err(error)
            }
        }
    }

    /// The design's id of a library's item: its copy in the design, else a
    /// new copy (an undo step of its own; [`Document::as_one_step`] joins
    /// it with the choice).
    pub fn library_item_in_design(
        &mut self,
        libraries: &Libraries,
        library: LibraryKind,
        kind: ItemKind,
        id: &str,
    ) -> Result<String, ModelError> {
        let item = library_item(libraries, library, kind, id)?;
        if let Some(copy) = self.state.copies_of(library, kind, id).into_iter().next() {
            return Ok(copy);
        }
        self.copy_from_print_library(item)
    }

    /// The `library_items` query: one row per item of a kind the user can
    /// choose (see the module documentation).
    pub fn library_items_json(&self, libraries: &Libraries, kind: ItemKind) -> Value {
        let mut rows = Vec::new();
        if kind == ItemKind::Material {
            for m in material::built_in() {
                if self.state.materials.iter().any(|d| d.id == m.id) {
                    continue;
                }
                rows.push(self.item_row(
                    &material_item(m.clone()),
                    None,
                    Some(m.id.clone()),
                    false,
                    true,
                ));
            }
        }
        let mut shown: Vec<String> = Vec::new();
        let mut others = Vec::new();
        for item in libraries
            .items
            .iter()
            .filter(|i| i.data.kind() == kind && !i.shadowed)
        {
            let copy = self
                .state
                .copies_of(item.library, kind, item.data.id())
                .into_iter()
                .next();
            let changed = copy
                .as_deref()
                .is_some_and(|c| self.library_changed(c, item));
            let data = self.shown_data(item, copy.as_deref());
            if let Some(copy) = &copy {
                shown.push(copy.clone());
            }
            others.push(self.item_row(&data, Some(item.library), copy, changed, false));
        }
        // The design's items that no row shows: those of a library item
        // that is shadowed now, and those in no library.
        let design: Vec<(ItemData, Option<&material::MaterialOrigin>)> = match kind {
            ItemKind::Material => self
                .state
                .materials
                .iter()
                .map(|m| (material_item(m.clone()), m.origin.as_ref()))
                .collect(),
            _ => self
                .state
                .print_profiles
                .iter()
                .filter(|p| ItemKind::of_profile(p.kind) == kind)
                .map(|p| (ItemData::Profile(p.clone()), p.origin.as_ref()))
                .collect(),
        };
        for (data, origin) in design {
            if shown.iter().any(|s| s == data.id()) {
                continue;
            }
            let item = origin.and_then(|o| {
                let library = LibraryKind::parse(&o.library)?;
                libraries.find(Some(library), kind, &o.id)
            });
            let id = data.id().to_owned();
            others.push(match item {
                Some(item) => {
                    let changed = self.library_changed(&id, item);
                    let data = self.shown_data(item, Some(&id));
                    self.item_row(&data, Some(item.library), Some(id), changed, false)
                }
                None => self.item_row(&data, None, Some(id), false, false),
            });
        }
        others.sort_by_key(|row| row["name"].as_str().unwrap_or_default().to_lowercase());
        rows.extend(others);
        let mut answer = json!({"items": rows, "problems": libraries.problems});
        // The owners with values of the printer's or the process' set
        // whatever the design chose, also with Slicer default (no row
        // chosen), as the export writes them (mitcad#747).
        let profile = match kind {
            ItemKind::Printer => Some(ProfileKind::Printer),
            ItemKind::Process => Some(ProfileKind::Process),
            ItemKind::Material => None,
        };
        if let Some(profile) = profile {
            answer["print_overrides"] = json!(self.print_overrides(profile));
        }
        answer
    }

    /// The `library_items` query by place (`by_place`, mitcad#1135): the
    /// managers' one list model, each item where it lives and nowhere else.
    /// First every item of the kind the design has (place `design`, a copy
    /// of a library item too, with its `source` and `changed`), then each
    /// library item with its own fields, shadowed ones too (`shadowed`),
    /// the project's, the user's and MitCad's (the built-in materials the
    /// design does not replace first, in their order), each place by name.
    /// A library row names the design's copy of it (`copy`) and whether the
    /// design uses that copy (`copy_used`); its own `in_design`, `chosen`,
    /// `bodies` and the like are the copy row's. `places` lists the places
    /// in that order: `design`, `project` (with a project), `user`, `mitcad`.
    pub fn library_items_by_place_json(&self, libraries: &Libraries, kind: ItemKind) -> Value {
        let by_name = |rows: &mut Vec<Value>| {
            rows.sort_by_key(|row| row["name"].as_str().unwrap_or_default().to_lowercase());
        };
        // This design: every item of the kind it has.
        let design: Vec<(ItemData, Option<&material::MaterialOrigin>)> = match kind {
            ItemKind::Material => self
                .state
                .materials
                .iter()
                .map(|m| (material_item(m.clone()), m.origin.as_ref()))
                .collect(),
            _ => self
                .state
                .print_profiles
                .iter()
                .filter(|p| ItemKind::of_profile(p.kind) == kind)
                .map(|p| (ItemData::Profile(p.clone()), p.origin.as_ref()))
                .collect(),
        };
        let mut design_rows = Vec::new();
        for (data, origin) in design {
            let id = data.id().to_owned();
            let source = origin.and_then(|o| {
                let library = LibraryKind::parse(&o.library)?;
                libraries.find(Some(library), kind, &o.id)
            });
            let changed = source.is_some_and(|item| self.library_changed(&id, item));
            let mut row = self.item_row(&data, None, Some(id), changed, false);
            if let Some(item) = source {
                row["source"] = json!({
                    "library": item.library.id(),
                    "place": item.library.place(),
                    "id": item.data.id(),
                    "name": item.data.name(),
                });
            }
            design_rows.push(row);
        }
        by_name(&mut design_rows);
        // The libraries: each item with its own fields.
        let mut places: [Vec<Value>; 3] = Default::default();
        for item in libraries.items.iter().filter(|i| i.data.kind() == kind) {
            let copy = self
                .state
                .copies_of(item.library, kind, item.data.id())
                .into_iter()
                .next();
            let used = copy
                .as_deref()
                .is_some_and(|c| self.design_item_used(kind, c));
            let mut row = self.item_row(&item.data, Some(item.library), None, false, false);
            row["shadowed"] = json!(item.shadowed);
            row["copy"] = json!(copy);
            row["copy_used"] = json!(used);
            let at = match item.library {
                LibraryKind::Project => 0,
                LibraryKind::User => 1,
                LibraryKind::Base => 2,
            };
            places[at].push(row);
        }
        for rows in &mut places {
            by_name(rows);
        }
        let mut rows = design_rows;
        let [project, user, base] = places;
        rows.extend(project);
        rows.extend(user);
        if kind == ItemKind::Material {
            for m in material::built_in() {
                if self.state.materials.iter().any(|d| d.id == m.id) {
                    continue;
                }
                rows.push(self.item_row(
                    &material_item(m.clone()),
                    None,
                    Some(m.id.clone()),
                    false,
                    true,
                ));
            }
        }
        rows.extend(base);
        let mut shown = vec!["design"];
        if libraries.folder(LibraryKind::Project).is_some() {
            shown.push("project");
        }
        shown.extend(["user", "mitcad"]);
        let mut answer = json!({"items": rows, "places": shown, "problems": libraries.problems});
        let profile = match kind {
            ItemKind::Printer => Some(ProfileKind::Printer),
            ItemKind::Process => Some(ProfileKind::Process),
            ItemKind::Material => None,
        };
        if let Some(profile) = profile {
            answer["print_overrides"] = json!(self.print_overrides(profile));
        }
        answer
    }

    /// Whether the design uses its item `id` of a kind: bodies or
    /// components name a material, the design or an owner chose a profile.
    fn design_item_used(&self, kind: ItemKind, id: &str) -> bool {
        match kind {
            ItemKind::Material => {
                !self.bodies_with_material(id).is_empty()
                    || !self.components_with_material(id).is_empty()
            }
            ItemKind::Printer | ItemKind::Process => {
                let profile = if kind == ItemKind::Printer {
                    ProfileKind::Printer
                } else {
                    ProfileKind::Process
                };
                self.state.design_profile(profile).as_deref() == Some(id)
                    || !self.state.profile_owners(profile, id).is_empty()
            }
        }
    }

    /// The fields a row shows for a library item: its design copy's when
    /// the design has one (what the design uses, mitcad#655), under the
    /// item's id; else the item's.
    fn shown_data(&self, item: &LibraryItem, copy: Option<&str>) -> ItemData {
        let id = item.data.id().to_owned();
        let mine = copy.and_then(|copy| match &item.data {
            ItemData::Material { .. } => self
                .state
                .materials
                .iter()
                .find(|m| m.id == copy)
                .cloned()
                .map(|mut material| {
                    material.id = id;
                    material_item(material)
                }),
            ItemData::Profile(_) => self.state.print_profile(copy).cloned().map(|mut profile| {
                profile.id = id;
                ItemData::Profile(profile)
            }),
        });
        mine.unwrap_or_else(|| item.data.clone())
    }

    /// Whether a design copy's library item was changed elsewhere (in
    /// another design, or its file by hand, mitcad#655): the library has a
    /// newer version than the copy's, one the design did not decline, and
    /// its fields differ from the copy's.
    pub(super) fn library_changed(&self, copy: &str, item: &LibraryItem) -> bool {
        let latest = item.data.version();
        let newer = |origin: Option<&material::MaterialOrigin>| {
            origin.is_some_and(|o| {
                o.version.unwrap_or(0) < latest && o.declined.is_none_or(|d| d < latest)
            })
        };
        match &item.data {
            ItemData::Material {
                material,
                appearances,
            } => {
                let Some(mine) = self.state.materials.iter().find(|m| m.id == copy) else {
                    return false;
                };
                if !newer(mine.origin.as_ref()) {
                    return false;
                }
                let mut same = mine.clone();
                same.id = material.id.clone();
                same.origin = material.origin.clone();
                same.version = material.version;
                // A carried appearance the design has under another id
                // (it had one of its id already) is the same (mitcad#1067).
                for look in appearances {
                    if same.offers(&look.id) {
                        continue;
                    }
                    let Some(here) = same
                        .appearance_list()
                        .into_iter()
                        .find(|id| self.same_look(Some(id.as_str()), Some(look)))
                    else {
                        continue;
                    };
                    same.rename_appearance(&here, &look.id);
                }
                // Of its list, the appearances a copy cannot have (neither
                // carried nor the library's nor the design's) do not count.
                let mut theirs = material.clone();
                theirs.appearances.retain(|id| {
                    carried(appearances, Some(id.as_str())).is_some()
                        || self.state.look(id).is_some()
                });
                same != theirs
            }
            ItemData::Profile(profile) => {
                let Some(mine) = self.state.print_profile(copy) else {
                    return false;
                };
                if !newer(mine.origin.as_ref()) {
                    return false;
                }
                let mut same = mine.clone();
                same.id = profile.id.clone();
                same.origin = profile.origin.clone();
                same.version = profile.version;
                same != *profile
            }
        }
    }

    /// Whether the design's appearance `mine` is the one a library material
    /// carries under another id (the design had one of its id already).
    fn same_look(&self, mine: Option<&str>, carried: Option<&Appearance>) -> bool {
        let look = mine.and_then(|id| self.state.appearances.iter().find(|a| a.id == id));
        match (look, carried) {
            (Some(look), Some(carried)) => {
                let mut look = look.clone();
                look.id = carried.id.clone();
                look == *carried
            }
            _ => false,
        }
    }

    /// What overrides the design's printer or process below the root
    /// (mitcad#655): each owner with values of the kind's set, by name, the
    /// keys it sets (`profile` when it chooses another one) and their values.
    fn print_overrides(&self, kind: ProfileKind) -> Vec<Value> {
        let set = kind.set().property_set();
        let root = PropertyOwner::Component(ComponentUid::ROOT);
        let mut overrides = Vec::new();
        for (owner, sets) in &self.state.properties.owners {
            if *owner == root {
                continue;
            }
            let Some(group) = sets.iter().find(|g| same_name(&g.name, set)) else {
                continue;
            };
            let keys: Vec<&str> = group.properties.iter().map(|p| p.key.as_str()).collect();
            let values: serde_json::Map<String, Value> = group
                .properties
                .iter()
                .map(|p| (p.key.clone(), p.value.to_json()))
                .collect();
            if !keys.is_empty() {
                overrides.push(json!({
                    "owner": owner,
                    "name": self.print_owner_name(*owner),
                    "keys": keys,
                    "values": values,
                }));
            }
        }
        overrides
    }

    /// A row of `library_items`: the item's fields where it lives, its
    /// place (`library`: None for a built-in material or the design's
    /// own), the design's copy, whether the library item was changed
    /// elsewhere ([`Document::library_changed`]).
    fn item_row(
        &self,
        data: &ItemData,
        library: Option<LibraryKind>,
        in_design: Option<String>,
        changed: bool,
        built_in: bool,
    ) -> Value {
        let place = match (library, built_in) {
            (Some(library), _) => library.place(),
            (None, true) => "mitcad",
            (None, false) => "design",
        };
        let fields = match data {
            ItemData::Material { material, .. } => {
                serde_json::to_value(material).expect("materials serialize")
            }
            ItemData::Profile(profile) => {
                let mut value = serde_json::to_value(profile).expect("profiles serialize");
                if let Value::Object(fields) = &mut value {
                    fields.remove("raw");
                }
                value
            }
        };
        let mut row = json!({
            "kind": data.kind().id(),
            "id": data.id(),
            "name": data.name(),
            "place": place,
            "library": library.map(LibraryKind::id),
            "read_only": place == "mitcad",
            "in_design": in_design,
            "changed": changed,
            "chosen": false,
            "item": fields,
        });
        match data {
            ItemData::Material {
                material,
                appearances,
            } => {
                let class = material_class(material);
                row["group"] = json!(class.group());
                row["class"] = json!(class.id());
                // Print settings make it an FDM plastic (mitcad#1102).
                row["class_fixed"] = json!(material.has_print_settings());
                let color = carried(appearances, material.appearance.as_deref())
                    .map(|a| a.display_color())
                    .or_else(|| {
                        material
                            .appearance
                            .as_deref()
                            .and_then(|a| self.appearance(a))
                            .map(|a| a.display_color())
                    });
                if let Some(color) = color {
                    row["display_color"] = json!(color);
                }
                // Every appearance it offers, its default first
                // (mitcad#1055), and the names and colours of those its
                // library item carries, which the design may not have yet
                // (mitcad#1067).
                row["appearances"] = json!(material.appearance_list());
                row["carried_appearances"] = json!(
                    appearances
                        .iter()
                        .map(|a| json!({
                            "id": a.id,
                            "name": a.name,
                            "base_color": a.base_color,
                            "display_color": a.display_color(),
                            "library": false,
                        }))
                        .collect::<Vec<_>>()
                );
                let used = in_design.as_deref();
                row["bodies"] =
                    json!(used.map_or_else(Vec::new, |id| self.bodies_with_material(id)));
                row["components"] =
                    json!(used.map_or_else(Vec::new, |id| self.components_with_material(id)));
            }
            ItemData::Profile(profile) => {
                let chosen_by = in_design
                    .as_deref()
                    .map_or_else(Vec::new, |id| self.state.profile_owners(profile.kind, id));
                row["chosen"] = json!(
                    in_design.is_some()
                        && self.state.design_profile(profile.kind).as_deref()
                            == in_design.as_deref()
                );
                row["chosen_by"] = json!(chosen_by);
                if row["chosen"] == json!(true) {
                    row["overrides"] = json!(self.print_overrides(profile.kind));
                }
                // The printer and nozzle a material's variant names for it
                // (mitcad#1054).
                if profile.kind == ProfileKind::Printer {
                    row["variant_printer"] = json!(profile.variant_printer());
                    row["nozzle"] = json!(profile.nozzle());
                }
                // The parts of its name the 3D Print panel's picker shows
                // (mitcad#1108): a printer's name without its nozzle and the
                // nozzle its name says, a process's name before its `@` and
                // the printer after it.
                if profile.kind == ProfileKind::Printer {
                    let (bare, nozzle) = material::split_nozzle(&profile.name);
                    row["bare_name"] = json!(bare);
                    row["name_nozzle"] = json!(nozzle);
                } else {
                    let (bare, printer) = material::split_filament_name(&profile.name);
                    row["bare_name"] = json!(bare);
                    row["for_printer"] = json!(printer);
                }
                // The printers a process is for and the names a printer
                // goes by (mitcad#1216): the managers group the processes
                // by printer, the pickers list the chosen printer's first.
                if profile.kind == ProfileKind::Printer {
                    row["printer_names"] = json!(profile.printer_key().names);
                } else {
                    row["printers"] = json!(
                        profile
                            .process_printers()
                            .into_iter()
                            .map(|(printer, nozzle)| json!({"printer": printer, "nozzle": nozzle}))
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
        row
    }

    /// Makes a material in the user's or the project's library (version
    /// 1): from `based_on` (a material of the design or built in, or a
    /// library's item) or empty (1 g/cm³), named `<name> (copy)` after the
    /// one it is based on (mitcad#828), else `Material 1` (`… 2`), its id the name's when not
    /// given. Returns the id, the file written and the version. The design
    /// does not change: the file is an undo step of its own.
    pub fn create_library_material(
        &mut self,
        libraries: &Libraries,
        library: LibraryKind,
        change: &MaterialChange,
    ) -> Result<(String, PathBuf, u32), ModelError> {
        if change.version.is_some() || change.origin.is_some() {
            return Err(invalid("a library's item keeps its own version and origin"));
        }
        let folder = library_folder(libraries, library)?;
        // An entry of the bundled material dataset (mitcad#1213).
        let dataset = match &change.dataset {
            Some(entry) => Some(super::materials::dataset_material(change, entry)?),
            None => None,
        };
        let (mut new, carried) = match (&change.based_on, dataset) {
            (None, Some(base)) => (base, Vec::new()),
            (Some(crate::print_library::BasedOn::Design(id)), _) => {
                let base = self
                    .material(id)
                    .cloned()
                    .ok_or_else(|| invalid(format!("there is no material {id}")))?;
                (base, Vec::new())
            }
            (Some(crate::print_library::BasedOn::Library(item)), _) => {
                let found = library_item(
                    libraries,
                    library_kind(&item.library)?,
                    ItemKind::Material,
                    &item.id,
                )?;
                match &found.data {
                    ItemData::Material {
                        material,
                        appearances,
                    } => (material.clone(), appearances.clone()),
                    ItemData::Profile(_) => unreachable!("a material's folder"),
                }
            }
            (None, None) => (
                Material {
                    id: String::new(),
                    name: String::new(),
                    density: 1.0,
                    appearance: None,
                    filament: Default::default(),
                    slicer: Default::default(),
                    variants: Vec::new(),
                    appearances: Vec::new(),
                    based_on: None,
                    class: None,
                    version: None,
                    origin: None,
                    properties: Default::default(),
                    reference: None,
                },
                Vec::new(),
            ),
        };
        let names: Vec<String> = libraries
            .items
            .iter()
            .filter(|i| i.data.kind() == ItemKind::Material)
            .map(|i| i.data.name().to_owned())
            .chain(self.state.materials.iter().map(|m| m.name.clone()))
            .chain(material::built_in().iter().map(|m| m.name.clone()))
            .collect();
        let taken = |name: &str| names.iter().any(|n| n.eq_ignore_ascii_case(name));
        new.name = match &change.based_on {
            Some(_) => copy_name(&new.name, taken),
            // A dataset's material as its entry (mitcad#1213).
            None if change.dataset.is_some() => material::numbered_name(&new.name, taken),
            None => (1..)
                .map(|n| new_item_name("Material", n))
                .find(|name| !taken(name))
                .expect("a free name"),
        };
        new.based_on = change.based_on.as_ref().map(|b| b.id().to_owned());
        new.origin = None;
        // The class is stored (mitcad#655): the one chosen, else the source's
        // of a copy, else the one its fields give. Print settings make it an
        // FDM plastic whatever it stores (mitcad#1102).
        let source_class =
            (change.based_on.is_some() || change.dataset.is_some()).then(|| material_class(&new));
        change.apply_to(&mut new);
        let class = change
            .class
            .or(source_class)
            .unwrap_or_else(|| material_class(&new));
        // A filament keeps `plastic`, which older versions read (mitcad#1102).
        new.class = Some(material::stored_class(&new, class));
        new.id = match change.id.as_deref().map(str::trim) {
            Some(id) => id.to_owned(),
            None => free_id(&slug(&new.name), |id| {
                libraries
                    .items
                    .iter()
                    .any(|i| i.data.kind() == ItemKind::Material && i.data.id() == id)
                    || material::built_in_material(id).is_some()
            }),
        };
        if libraries
            .find(Some(library), ItemKind::Material, &new.id)
            .is_some()
        {
            return Err(invalid(format!(
                "the {} library has a material {}",
                library.id(),
                new.id
            )));
        }
        new.version = Some(1);
        let appearances =
            item_appearances(&new, &carried, &self.state.appearances).ok_or_else(|| {
                invalid(format!(
                    "material {}: there is no appearance {}",
                    new.id,
                    new.appearance.as_deref().unwrap_or_default()
                ))
            })?;
        new.check().map_err(invalid)?;
        let id = new.id.clone();
        let label = format!("Create Material {}", new.name);
        let data = ItemData::Material {
            material: new,
            appearances,
        };
        let file = FileWrite::item(&folder, &data);
        let path = file.path().to_path_buf();
        self.with_files(label, vec![file], |_| Ok(()))?;
        Ok((id, path, 1))
    }

    /// Edits a material of the user's or the project's library: the
    /// change is applied key by key on top of the library's newest version,
    /// which counts up; the design's copies take the same change in one
    /// undo step (a copy that had the library's version is then at the new
    /// one). Returns the file written and the version.
    pub fn edit_library_material(
        &mut self,
        libraries: &Libraries,
        library: LibraryKind,
        id: &str,
        change: &MaterialChange,
    ) -> Result<(PathBuf, u32), ModelError> {
        self.edit_library_material_carrying(libraries, library, id, change, None, &[])
    }

    /// [`Document::edit_library_material`] whose file carries `given`, the
    /// appearance the change names, in place of the one it carried
    /// (mitcad#1053: an imported filament's appearance overriding an item),
    /// and of `also` (other items' appearances, mitcad#1150: a merge's)
    /// those its list names that it does not carry. The design's copies of
    /// the item take the change too, so when it has any, `given` must be
    /// the design's appearance of that id.
    pub(crate) fn edit_library_material_carrying(
        &mut self,
        libraries: &Libraries,
        library: LibraryKind,
        id: &str,
        change: &MaterialChange,
        given: Option<&Appearance>,
        also: &[Appearance],
    ) -> Result<(PathBuf, u32), ModelError> {
        if change.based_on.is_some() {
            return Err(invalid("based_on is only for create_material"));
        }
        if change.version.is_some() || change.origin.is_some() {
            return Err(invalid("a library's item keeps its own version and origin"));
        }
        if change.id.as_deref().is_some_and(|new| new != id) {
            return Err(invalid("a material's id cannot change"));
        }
        let folder = library_folder(libraries, library)?;
        let item = library_item(libraries, library, ItemKind::Material, id)?;
        let ItemData::Material {
            material: original,
            appearances: carried,
        } = &item.data
        else {
            unreachable!("a material's folder")
        };
        let old = item.data.version();
        let mut edited = original.clone();
        change.apply_to(&mut edited);
        let path = folder.join(&item.path);
        let given = given.filter(|g| edited.appearance.as_deref() == Some(g.id.as_str()));
        // What the file carries (mitcad#1067), `given` in the place of its
        // appearance of that id (mitcad#1053).
        let mut carrying: Vec<Appearance> = carried
            .iter()
            .filter(|a| given.is_none_or(|g| g.id != a.id))
            .cloned()
            .collect();
        if let Some(g) = given {
            carrying.insert(0, g.clone());
        }
        for look in also {
            if !carrying.iter().any(|a| a.id == look.id) {
                carrying.push(look.clone());
            }
        }
        // The library has these values already (a newer version made in
        // another design): its file stays, and only the design's copies
        // that differ take the change (mitcad#758).
        let unchanged = edited == *original && given.is_none_or(|g| carried.iter().any(|a| a == g));
        let version = if unchanged { old } else { old + 1 };
        edited.version = Some(version);
        edited.origin = None;
        let appearances = item_appearances(&edited, &carrying, &self.state.appearances)
            .ok_or_else(|| {
                invalid(format!(
                    "material {id}: there is no appearance {}",
                    edited.appearance.as_deref().unwrap_or_default()
                ))
            })?;
        edited.check().map_err(invalid)?;
        // The design's copies: checked before the file is written.
        let mut copies = Vec::new();
        for (i, m) in self.state.materials.iter().enumerate() {
            if !copied_from(m.origin.as_ref(), library, id) {
                continue;
            }
            let mut copy = m.clone();
            change.apply_to(&mut copy);
            if !unchanged {
                if m.origin.as_ref().and_then(|o| o.version) == Some(old) {
                    copy.version = Some(version);
                    if let Some(origin) = &mut copy.origin {
                        origin.version = Some(version);
                    }
                }
                // A change the design declined stays declined (mitcad#655).
                if let Some(origin) = &mut copy.origin
                    && origin.declined == Some(old)
                {
                    origin.declined = Some(version);
                }
            }
            if copy != *m {
                self.check_material(&copy, Some(i))?;
                copies.push((i, copy));
            }
        }
        if unchanged && copies.is_empty() {
            return Ok((path, old));
        }
        // The file and the copies are one undo step (mitcad#655).
        let label = format!("Edit Material {}", edited.name);
        let files = if unchanged {
            Vec::new()
        } else {
            let data = ItemData::Material {
                material: edited,
                appearances,
            };
            vec![FileWrite::new(path.clone(), Some(data.text().into_bytes()))]
        };
        let make_default = change.appearance.is_none();
        self.with_files(label, files, move |state| {
            for (i, mut copy) in copies {
                // A filament colour set: an appearance of the copy's list
                // (mitcad#1055).
                state.adopt_filament_colour(&mut copy, make_default);
                state.materials[i] = copy;
            }
            Ok(())
        })?;
        Ok((path, version))
    }

    /// Deletes a material from the user's or the project's library (its
    /// file). The design's copies that bodies or components use stay as the
    /// design's own unsaved items (their origin goes, mitcad#655); the
    /// others go. One undo step, which brings the file back too. Returns
    /// the file removed, the copies kept and those removed.
    pub fn delete_library_material(
        &mut self,
        libraries: &Libraries,
        library: LibraryKind,
        id: &str,
    ) -> Result<(PathBuf, Vec<String>, Vec<String>), ModelError> {
        let folder = library_folder(libraries, library)?;
        let item = library_item(libraries, library, ItemKind::Material, id)?;
        let path = folder.join(&item.path);
        let (kept, removed): (Vec<String>, Vec<String>) = self
            .state
            .copies_of(library, ItemKind::Material, id)
            .into_iter()
            .partition(|copy| {
                !self.bodies_with_material(copy).is_empty()
                    || !self.components_with_material(copy).is_empty()
            });
        let label = format!("Delete Material {}", item.data.name());
        let (gone, own) = (removed.clone(), kept.clone());
        self.with_files(
            label,
            vec![FileWrite::new(path.clone(), None)],
            move |state| {
                state.materials.retain(|m| !gone.contains(&m.id));
                for m in state.materials.iter_mut().filter(|m| own.contains(&m.id)) {
                    m.origin = None;
                }
                // The removed copies' purge volumes set by hand (mitcad#758).
                for id in &gone {
                    state.drop_purge_overrides_of(id);
                }
                Ok(())
            },
        )?;
        Ok((path, kept, removed))
    }

    /// The kind of a library's profile by id: the one given, else the only
    /// kind the library has the id of.
    pub(super) fn library_profile_kind(
        libraries: &Libraries,
        library: LibraryKind,
        kind: Option<ProfileKind>,
        id: &str,
    ) -> Result<ProfileKind, ModelError> {
        if let Some(kind) = kind {
            return Ok(kind);
        }
        let kinds: Vec<ProfileKind> = ProfileKind::ALL
            .into_iter()
            .filter(|k| {
                libraries
                    .find(Some(library), ItemKind::of_profile(*k), id)
                    .is_some()
            })
            .collect();
        match kinds.as_slice() {
            [kind] => Ok(*kind),
            [] => Err(invalid(format!(
                "the {} library has no profile {id}",
                library.id()
            ))),
            _ => Err(invalid(format!(
                "the {} library has a printer and a process {id}: give the kind",
                library.id()
            ))),
        }
    }

    /// Makes a printer or process profile in the user's or the project's
    /// library (version 1): from `based_on` (a profile of the design, or a
    /// library's item) or empty of `kind`, named `<name> (copy)` after the
    /// one it is based on (mitcad#828), else `Printer 1` or `Process 1`
    /// (`… 2`), its id the
    /// name's when not given. Returns the id, the file written and the
    /// version. The design does not change: the file is an undo step of
    /// its own.
    pub fn create_library_profile(
        &mut self,
        libraries: &Libraries,
        library: LibraryKind,
        change: &ProfileChange,
    ) -> Result<(String, PathBuf, u32), ModelError> {
        if change.version.is_some() || change.origin.is_some() {
            return Err(invalid("a library's item keeps its own version and origin"));
        }
        let folder = library_folder(libraries, library)?;
        let mut new = match &change.based_on {
            Some(crate::print_library::BasedOn::Design(id)) => self
                .print_profile(id)
                .cloned()
                .ok_or_else(|| invalid(format!("there is no profile {id}")))?,
            Some(crate::print_library::BasedOn::Library(item)) => {
                let item_library = library_kind(&item.library)?;
                let kind =
                    Self::library_profile_kind(libraries, item_library, change.kind, &item.id)?;
                match &library_item(
                    libraries,
                    item_library,
                    ItemKind::of_profile(kind),
                    &item.id,
                )?
                .data
                {
                    ItemData::Profile(profile) => profile.clone(),
                    ItemData::Material { .. } => unreachable!("a profile's folder"),
                }
            }
            None => {
                let kind = change.kind.ok_or_else(|| {
                    invalid("create_print_profile needs a kind: printer or process")
                })?;
                PrintProfile::new("", "", kind)
            }
        };
        if let Some(kind) = change.kind
            && kind != new.kind
        {
            return Err(invalid(format!(
                "{} is a {} profile, not a {} one",
                new.name,
                new.kind.id(),
                kind.id()
            )));
        }
        let kind = new.kind;
        let item_kind = ItemKind::of_profile(kind);
        let names: Vec<String> = libraries
            .items
            .iter()
            .filter(|i| i.data.kind() == item_kind)
            .map(|i| i.data.name().to_owned())
            .chain(
                self.state
                    .print_profiles
                    .iter()
                    .filter(|p| p.kind == kind)
                    .map(|p| p.name.clone()),
            )
            .collect();
        let taken = |name: &str| names.iter().any(|n| n.eq_ignore_ascii_case(name));
        new.name = match &change.based_on {
            Some(_) => copy_name(&new.name, taken),
            None => {
                let title = match kind {
                    ProfileKind::Printer => "Printer",
                    ProfileKind::Process => "Process",
                };
                (1..)
                    .map(|n| new_item_name(title, n))
                    .find(|name| !taken(name))
                    .expect("a free name")
            }
        };
        new.based_on = change.based_on.as_ref().map(|b| b.id().to_owned());
        new.origin = None;
        change.apply_to(&mut new);
        new.normalize();
        new.id = match change.id.as_deref().map(str::trim) {
            Some(id) => id.to_owned(),
            None => free_id(&slug(&new.name), |id| {
                libraries
                    .items
                    .iter()
                    .any(|i| i.data.kind() == item_kind && i.data.id() == id)
            }),
        };
        if libraries.find(Some(library), item_kind, &new.id).is_some() {
            return Err(invalid(format!(
                "the {} library has a {} {}",
                library.id(),
                kind.id(),
                new.id
            )));
        }
        new.version = Some(1);
        new.check().map_err(invalid)?;
        let id = new.id.clone();
        let label = format!("Create {} {}", profile_title(kind), new.name);
        let file = FileWrite::item(&folder, &ItemData::Profile(new));
        let path = file.path().to_path_buf();
        self.with_files(label, vec![file], |_| Ok(()))?;
        Ok((id, path, 1))
    }

    /// Edits a printer or process profile of the user's or the project's
    /// library, as [`Document::edit_library_material`] a material: key by
    /// key on top of the newest version, the design's copies taking the
    /// same change in one undo step. Returns the file written and the
    /// version.
    pub fn edit_library_profile(
        &mut self,
        libraries: &Libraries,
        library: LibraryKind,
        id: &str,
        change: &ProfileChange,
    ) -> Result<(PathBuf, u32), ModelError> {
        if change.based_on.is_some() {
            return Err(invalid("based_on is only for create_print_profile"));
        }
        if change.version.is_some() || change.origin.is_some() {
            return Err(invalid("a library's item keeps its own version and origin"));
        }
        if change.id.as_deref().is_some_and(|new| new != id) {
            return Err(invalid("a profile's id cannot change"));
        }
        let folder = library_folder(libraries, library)?;
        let kind = Self::library_profile_kind(libraries, library, change.kind, id)?;
        let item = library_item(libraries, library, ItemKind::of_profile(kind), id)?;
        let ItemData::Profile(original) = &item.data else {
            unreachable!("a profile's folder")
        };
        let old = item.data.version();
        let mut edited = original.clone();
        change.apply_to(&mut edited);
        edited.normalize();
        let path = folder.join(&item.path);
        // As for a material: the library has these values already, so only
        // the design's copies that differ take the change (mitcad#758).
        let unchanged = edited == *original;
        let version = if unchanged { old } else { old + 1 };
        edited.version = Some(version);
        edited.origin = None;
        edited.check().map_err(invalid)?;
        let mut copies = Vec::new();
        for (i, p) in self.state.print_profiles.iter().enumerate() {
            if p.kind != kind || !copied_from(p.origin.as_ref(), library, id) {
                continue;
            }
            let mut copy = p.clone();
            change.apply_to(&mut copy);
            copy.normalize();
            if !unchanged {
                if p.origin.as_ref().and_then(|o| o.version) == Some(old) {
                    copy.version = Some(version);
                    if let Some(origin) = &mut copy.origin {
                        origin.version = Some(version);
                    }
                }
                // A change the design declined stays declined (mitcad#655).
                if let Some(origin) = &mut copy.origin
                    && origin.declined == Some(old)
                {
                    origin.declined = Some(version);
                }
            }
            if copy != *p {
                self.check_profile(&copy, Some(i))?;
                copies.push((i, copy));
            }
        }
        if unchanged && copies.is_empty() {
            return Ok((path, old));
        }
        // The file and the copies are one undo step (mitcad#655).
        let label = format!("Edit {} {}", profile_title(kind), edited.name);
        let files = if unchanged {
            Vec::new()
        } else {
            let text = ItemData::Profile(edited).text();
            vec![FileWrite::new(path.clone(), Some(text.into_bytes()))]
        };
        self.with_files(label, files, move |state| {
            for (i, copy) in copies {
                state.print_profiles[i] = copy;
            }
            state.sync_profile_types();
            Ok(())
        })?;
        Ok((path, version))
    }

    /// Deletes a printer or process profile from the user's or the
    /// project's library (its file). The design's copies that an owner
    /// chose stay as the design's own unsaved items (their origin goes,
    /// mitcad#655); the others go. One undo step, which brings the file
    /// back too. Returns the file removed, the copies kept and those
    /// removed.
    pub fn delete_library_profile(
        &mut self,
        libraries: &Libraries,
        library: LibraryKind,
        kind: Option<ProfileKind>,
        id: &str,
    ) -> Result<(PathBuf, Vec<String>, Vec<String>), ModelError> {
        let folder = library_folder(libraries, library)?;
        let kind = Self::library_profile_kind(libraries, library, kind, id)?;
        let item_kind = ItemKind::of_profile(kind);
        let item = library_item(libraries, library, item_kind, id)?;
        let path = folder.join(&item.path);
        let (kept, removed): (Vec<String>, Vec<String>) = self
            .state
            .copies_of(library, item_kind, id)
            .into_iter()
            .partition(|copy| !self.state.profile_owners(kind, copy).is_empty());
        let label = format!("Delete {} {}", profile_title(kind), item.data.name());
        let (gone, own) = (removed.clone(), kept.clone());
        self.with_files(
            label,
            vec![FileWrite::new(path.clone(), None)],
            move |state| {
                state.print_profiles.retain(|p| !gone.contains(&p.id));
                for p in state
                    .print_profiles
                    .iter_mut()
                    .filter(|p| own.contains(&p.id))
                {
                    p.origin = None;
                }
                state.sync_profile_types();
                Ok(())
            },
        )?;
        Ok((path, kept, removed))
    }
}

/// A profile kind's title in undo steps: `Printer`, `Process`.
pub(crate) fn profile_title(kind: ProfileKind) -> &'static str {
    match kind {
        ProfileKind::Printer => "Printer",
        ProfileKind::Process => "Process",
    }
}
