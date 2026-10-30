// SPDX-License-Identifier: MIT
//! Design file (`.mitcad`): the definition state as human-readable JSON.
//! Geometry is not stored; it is recomputed on open.
//!
//! Version 2 (written by this build, see `v2.rs`) saves the parameters, the
//! features in timeline order with their uids, the body names and the
//! timeline marker. A feature's fields are its definition's serde form (see
//! [`crate::features`]); references are uids (`"F3"`, `"F3.b0"`),
//! topological names (`"E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"`, see
//! [`crate::topo`]) and parameter names (`"d3"`). Version 1 files (V0) are
//! read and converted (`v1.rs`).
//!
//! Loading checks the format, the version, the schema, every reference and
//! the values, with the same checks as the commands, so a file can only
//! describe a timeline the commands could build. Errors say where, e.g.
//! `features[5] (Fillet1): radius: parameter 'd99' does not exist`.
//!
//! ```json
//! {
//!   "format": "mitcad",
//!   "version": 2,
//!   "units": { "length": "mm", "angle": "deg" },
//!   "parameters": [
//!     { "name": "d1", "expression": "60 mm", "unit": "mm", "value": 60.0,
//!       "comment": "Sketch1 width", "owner": "F1" },
//!     { "name": "d2", "expression": "40 mm", "unit": "mm", "value": 40.0,
//!       "comment": "Sketch1 height", "owner": "F1" },
//!     { "name": "d3", "expression": "d1 / 3", "unit": "mm", "value": 20.0,
//!       "comment": "Extrude1 distance", "owner": "F2" },
//!     { "name": "d4", "expression": "2 mm", "unit": "mm", "value": 2.0,
//!       "comment": "Fillet1 radius", "owner": "F3" }
//!   ],
//!   "features": [
//!     { "uid": "F1", "name": "Sketch1", "type": "sketch", "plane": "xy",
//!       "entities": [
//!         { "id": "c1", "type": "line", "start": "p5", "end": "p6" },
//!         { "id": "c2", "type": "line", "start": "p6", "end": "p7" },
//!         { "id": "c3", "type": "line", "start": "p7", "end": "p8" },
//!         { "id": "c4", "type": "line", "start": "p8", "end": "p5" },
//!         { "id": "p5", "type": "point", "at": [0.0, 0.0], "fixed": true },
//!         { "id": "p6", "type": "point", "at": [60.0, 0.0] },
//!         { "id": "p7", "type": "point", "at": [60.0, 40.0] },
//!         { "id": "p8", "type": "point", "at": [0.0, 40.0] } ],
//!       "constraints": [
//!         { "id": "k1", "type": "horizontal", "line": "c1" },
//!         { "id": "k2", "type": "vertical", "line": "c2" },
//!         { "id": "k3", "type": "horizontal", "line": "c3" },
//!         { "id": "k4", "type": "vertical", "line": "c4" } ],
//!       "dimensions": [
//!         { "id": "k5", "type": "length", "line": "c1", "value": "d1" },
//!         { "id": "k6", "type": "length", "line": "c2", "value": "d2" } ] },
//!     { "uid": "F2", "name": "Extrude1", "type": "extrude",
//!       "profiles": [ { "sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}" } ],
//!       "extent": { "type": "distance", "distance": "d3" }, "operation": "new_body" },
//!     { "uid": "F3", "name": "Fillet1", "type": "fillet", "body": "F2.b0",
//!       "edges": [ "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}" ], "radius": "d4" }
//!   ],
//!   "bodies": [ { "uid": "F2.b0", "name": "Body1" } ]
//! }
//! ```
//!
//! `units` are the document's default units: bare numbers in expressions
//! are read in them (a parameter's own unit comes first). A parameter is
//! its `expression` and `unit` (`""` for unitless); `value` is the value
//! when saved (millimetres or radians), for readers of the file, and is
//! not read back. Parameters may refer to parameters later in the list.
//! Files written before expressions have only a `value`: it becomes an
//! expression in millimetres (degrees for a parameter a feature uses as an
//! angle), and a name that is now a unit, function or constant (`m`, `E`,
//! `in`, `PI`) is renamed (`m_1`) with a warning
//! ([`crate::Document::load_warnings`]).
//!
//! A parameter copied from another document may also store
//! `expression_units` (`length`, `angle`): the source defaults for implicit
//! units inside its expression. These stay with the parameter through
//! edits, configuration changes and further copies; its declared `unit`
//! takes precedence for that unit's dimensions. Without this field,
//! expressions follow the document's defaults as before.
//!
//! A parameter with limits (mitcad#665) also stores `min`, `max` and
//! `step` (expression texts in its unit, each only when set) and
//! `"strict": true` (a result outside them is refused instead of clamped);
//! `value` is then the limited value.
//!
//! Optional fields: `units` (millimetres and degrees), a parameter's
//! `comment` and `owner` (the feature whose dimension it is), a feature's
//! `"suppressed": true` and `import_note` (what the importer that made it
//! said, mitcad#170), `"marker": n` (the number of features before the
//! timeline marker, when it is not at the end) and `bodies`, whose entries
//! may add `"visible": false`, a `"material"` and an `"appearance"` (F5).
//! Feature coordinates are millimetres and radians, in the coordinates of
//! the feature's component.
//!
//! Components and occurrences (F6, `crate::assembly`), when the design has
//! any: `components` (`uid`, `name`, the feature that made it as
//! `created_by`, a linked file as `link` {`path`, `digest`, `feature`}),
//! `occurrences` (`uid`, `component`, `parent`, `number`, `transform` as
//! rows of a rotation and a translation unless the identity, `grounded`,
//! `"visible": false`), `active_component`, `root_component` (the root's
//! name when renamed) and a feature's `component` when it is not the root
//! (`C0`). Files without them load with every feature in the root.
//!
//! A component from a library (mitcad#64) has `library` (`library`, `url`,
//! `rev`, `label`, `component`, `path`, `config`, `license`, `authors`,
//! `designation`; [`crate::library::LibraryRef`]); a design with a table of
//! sizes has `configurations` (`selectors`, `parameters`, `default`,
//! `rows`; [`crate::configurations`]). Both are left out otherwise, so
//! files without them are as before.
//!
//! A design that may be a hybrid assembly without asking (mitcad#171:
//! "don't ask again for this design") has `"hybrid_assembly": true`.
//!
//! A design whose importer set a join tolerance (mitcad#298,
//! `Document::set_join_tolerance`) has `"join_tolerance"` (mm).
//!
//! Design policy (mitcad#220, `crate::policy`): `root_policy` and a
//! component's `policy`, objects of the nine settings (`sketches`,
//! `features`, `components`, `attach_to_others`,
//! `attachments_from_others`, `first_sketch_attaches`,
//! `first_sketch_attachments_from_others`, `reference_others`,
//! `references_from_others`), left out while they are the general
//! defaults; a missing setting is its general default (the two
//! first-sketch settings, mitcad#293, read as on).
//!
//! Named views (U5), when there are any: `views` (`name`, `eye`, `target`,
//! `up`, `height`, `"perspective": true`).
//!
//! Analyses (mitcad#41), when there are any: `analyses` (`name`,
//! `"visible": false`, `type`: `section` with `plane`, `offset`, `flip`;
//! `print` with `bodies`, `bed_face`, `overhang`, `thin_wall`, `estimate`,
//! mitcad#620).
//!
//! Motion studies and contact sets (mitcad#454), when there are any:
//! `motion_studies` (`name`, `steps`, `keys`: `joint`, `motion`, `points`
//! `[[step, value], …]`) and `contact_sets` (`name`, `occurrences` as paths
//! from the root, `"suppressed": true`), see [`crate::motion`].
//!
//! Properties (mitcad#496), when there are any: `property_types` (`name`,
//! `base`, `values`, `unit`, `default`, `description`) and `properties`
//! (`owner`: `C2`, `O3` or `F7.b0`, `sets`: `name`, `properties`: `key`,
//! `type`, `value`, `source` unless `user`, `imported`, `unit`), see
//! [`crate::properties`]. Computed values are never written.
//!
//! Materials of the design (mitcad#640), when there are any: `materials`
//! (`id`, `name`, `density`, `appearance`, `filament`, `based_on`,
//! `version`, `origin`) and the components' default materials
//! `component_materials` (`[{"component", "material"}]`), see
//! [`crate::material`]; a body's `material` names a built-in material or
//! one of these.
//!
//! Printer and process profiles of the design (mitcad#640), when there are
//! any: `print_profiles` (`id`, `name`, `kind`, `values`, `slicer`, `raw`,
//! `based_on`, `version`, `origin`), see [`crate::print_profile`]; the
//! components', occurrences' and bodies' choices and overrides are
//! properties of the `Print Printer` and `Print Process` sets.
//!
//! Annotations (mitcad#702), when there are any: `annotations` (`name`,
//! `kind`, `color` `#rrggbb`, `"visible": false`, `group`, the picks `a`
//! and `b` as the `measure` query's selections: `body` with `face`, `edge`
//! or `vertex`, or `datum`, and `occurrence`; `plane`, `line_offset`,
//! `text_offset`, `format`, `tolerance`, a note's `text` and `at`), see
//! `document/annotations.rs`. Their values are not stored. While every one
//! has only the fields of the measurement objects of mitcad#649 (a
//! dimension's `name`, `kind`, `color`, `visible`, `a`, `b`), they are
//! written as `measurements`, which older versions read; both lists are
//! read as annotations. Since mitcad#894 an annotation's `color` may be
//! `auto` (the neutral annotation colour that follows the view's
//! background, the colour of new annotations); one stored with one of the
//! six colours new annotations took in turn before (`#d9480f`, `#1971c2`,
//! `#2f9e44`, `#c2255c`, `#7048e8`, `#e67700`: the program's choice, not
//! the user's) is read as `auto` and written so when saved again. The
//! height of the annotations' texts set for the design (mm, 0.1 to 1000)
//! is `annotation_text_height`, left out for the automatic one.
//!
//! Purge volumes set by hand (mitcad#705), when there are any:
//! `purge_volumes` (`[{"from", "to", "volume"}]`: material ids and mm³),
//! see [`crate::print_purge`]; a pair of a material the design no longer
//! has is kept.
//!
//! Drawings of the root and the components (mitcad#707), when there are
//! any: `drawings` (`uid` `D<n>`, `name`, `component` unless the root's,
//! `sheets`: `name`, `size`, `orientation`, `projection`, `title_block`,
//! `units`, `annotations`, `views`: `id`, `kind` {`type` `base`,
//! `projected`, `section` or `detail` with its fields}, `place`, `scale`,
//! `style`, `label`, `subject`, `breaks`, `aligned`), see
//! [`crate::drawing::def`]. Their lines are not stored.
//!
//! Appearances of the document (mitcad#46), when there are any:
//! `appearances` (`id`, `name`, `base_color`, `metalness`, `roughness`,
//! `specular`, `transmission`, `ior`, `coat`, `coat_roughness`, `emission`,
//! `emission_color`, `opacity`, and a `texture` {`path`, `size`,
//! `rotation`, `projection`, `data`, and the maps `normal`, `roughness`,
//! `metalness`, `occlusion` {`path`, `data`} and `source`, mitcad#313};
//! parameters left out are the default's), see [`crate::appearance`].
//! Embedded images are in their texture's `data` (base64) unless a map's
//! image is embedded or one image is embedded twice: then the file has
//! `images` (`[{"sha256", "data"}]`, each image once) and the textures and
//! maps refer to them by `"image": "<sha256>"` (mitcad#313).
//!
//! Render settings (mitcad#47), when they are not the defaults: `render`
//! with the sections `environment`, `background`, `ground`, `film` and
//! `output` (mitcad#48; fields left out are the defaults), see
//! [`crate::render_settings`].
//!
//! The display state (P9), when it is not the default: `display` with
//! `"origin": true` (the Origin folder shown) and `isolated` (`body` and
//! `occurrence`, or an `occurrence` path); a favourite parameter has
//! `"favorite": true`; timeline groups are `groups` (`name`, `features`).
//!
//! A sketch stores its entities with their last solved positions (the
//! start of the next solve), constraints, dimensions, linked projections
//! and texts (`core/model-api/src/commands.md`). Sketches of earlier files
//! (`shapes`: rectangles and circles, in version 1 and early version 2) are
//! converted to entities, constraints and dimensions with the same curve
//! ids and parameters, so region keys and face names keep resolving.
//!
//! Version 3 (P12a, a design in a project) is version 2 whose base features
//! refer to their B-rep data in the store of the project the file is in by
//! its SHA-256 (`{"format": "occt", "compression": "zlib", "size": n,
//! "sha256": "…"}`, see [`crate::features::Brep`] and [`project`]); a file
//! is version 3 only when it has such references. Designs in a project (a
//! folder with `.mitcad/project.json` above them) are written so, others as
//! version 2 (a single-file design) with the data inside ([`FileFormat`]). Opening resolves the references
//! from the store; data the store lacks leaves the base feature failing
//! with a message, and opening says so in a warning. In a project the
//! display state (`display`: the Origin folder, Isolate) is per user and
//! not versioned: it is kept apart in `.mitcad/local/display/<path in the
//! project>.json` (`{"format": "mitcad-local", "version": 1, "display":
//! {...}}`), which opening reads.

mod project;
// A project's settings, shared and of this computer (mitcad#89).
pub mod settings;
mod v1;
mod v2;
// The top id numbers a design file leaves free (mitcad#944).
mod id_reserve;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::Path;
use std::sync::Arc;

pub use self::project::{
    BREP_DIR, BlobStore, FsStore, GITATTRIBUTES, GITIGNORE, LOCAL_DIR, MemoryStore, PROJECT_MARKER,
    Project, brep_path, rename_retrying, write_atomically,
};
use crate::document::{DisplayState, DocState, Document};
use crate::expr::is_reserved_name;
use crate::features::{Brep, FeatureDef, FeatureEntry};
use crate::kernel::Kernel;
pub use crate::sha256::Sha256;

/// Value of the `"format"` field.
pub(crate) const FORMAT: &str = "mitcad";
/// The newest version this build reads; it writes it for files that refer
/// to a project's B-rep store.
pub(crate) const VERSION: u64 = 3;
/// The version of files with all their data inside.
pub(crate) const SINGLE_VERSION: u64 = 2;

/// How [`Document::save_file`] writes a design file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// Version 3 in a project, else a single-file design.
    Auto,
    /// A single-file design with the B-rep data inside (version 2): outside
    /// projects, for sharing one file, autosave and the import's process.
    Single,
    /// The B-rep data in the project's store (version 3); the file must be
    /// in a project.
    Project,
}

/// A design file that could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesignError {
    /// Reading or writing a file failed.
    Io(String),
    /// The design file is rejected.
    File(FileError),
    /// A version 3 file was asked for outside a project.
    NotInProject(String),
}

impl fmt::Display for DesignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => f.write_str(message),
            Self::File(error) => error.fmt(f),
            Self::NotInProject(path) => write!(
                f,
                "{path} is not in a Mitcad project (a folder with {PROJECT_MARKER} \
                 in it or above it)"
            ),
        }
    }
}

impl std::error::Error for DesignError {}

impl From<FileError> for DesignError {
    fn from(error: FileError) -> Self {
        Self::File(error)
    }
}

/// Rejected design file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    /// The text is not valid JSON.
    Syntax(String),
    /// Valid JSON without the Mitcad format marker.
    NotADesign,
    /// Missing, malformed or unsupported file version.
    Version(String),
    /// The JSON does not match the schema of its version.
    Schema(String),
    /// A reference or a value is invalid.
    Invalid(String),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(message) => write!(f, "not valid JSON: {message}"),
            Self::NotADesign => write!(
                f,
                "not a Mitcad design file (\"format\": \"{FORMAT}\" is missing)"
            ),
            Self::Version(message) => f.write_str(message),
            Self::Schema(message) | Self::Invalid(message) => {
                write!(f, "invalid design file: {message}")
            }
        }
    }
}

impl std::error::Error for FileError {}

impl<K: Kernel> Document<K> {
    /// The design file of this document as a single file, pretty-printed
    /// (see [`crate::file`]): version 2 with the B-rep data inside, unless a
    /// base feature's data is missing (a reference that was not resolved),
    /// which keeps its reference and makes the file version 3.
    pub fn to_json(&self) -> String {
        v2::save(self.state())
    }

    /// The design file of this document as a project versions it (version
    /// 3): base features' B-rep data goes into `store` (what it has already
    /// is not written again) and the file refers to it; the display state
    /// (the Origin folder, Isolate) is left out, being per user
    /// ([`Document::save_file`] keeps it apart). Without base features and
    /// display state it is the same as [`Document::to_json`].
    pub fn to_project_json(&self, store: &dyn BlobStore) -> std::io::Result<String> {
        let state = self.state();
        if state.display.is_default()
            && !state
                .features
                .iter()
                .any(|f| matches!(f.def, FeatureDef::Base(_)))
            && state.library_sources.is_empty()
        {
            return Ok(v2::save(state));
        }
        let mut written = HashSet::new();
        let mut referring = state.clone();
        referring.display = DisplayState::default();
        referring.library_sources = state.library_sources.to_references(store)?;
        for entry in &mut referring.features {
            let FeatureDef::Base(base) = &entry.def else {
                continue;
            };
            let mut base = base.clone();
            for body in &mut base.bodies {
                let sha256 = body.brep.sha256();
                if !body.brep.is_reference() && !written.contains(&sha256) {
                    if !store.contains(&sha256)? {
                        let stored = body.brep.stored().expect("not a reference");
                        store.put(&sha256, &stored)?;
                    }
                    written.insert(sha256);
                }
                body.brep = body.brep.to_reference();
            }
            *entry = Arc::new(FeatureEntry {
                def: FeatureDef::Base(base),
                ..(**entry).clone()
            });
        }
        Ok(v2::save(&referring))
    }

    /// Writes the design file to `path`, whole or not at all (a temporary
    /// file renamed over it), in `format`; returns the text written. In a
    /// project the B-rep files are written before the design file, so an
    /// interruption leaves at most files nothing refers to, and the display
    /// state goes to the project's per-user state
    /// ([`Project::local_state_path`]; as far as it can be written: it is
    /// no part of the design).
    pub fn save_file(&self, path: &Path, format: FileFormat) -> Result<String, DesignError> {
        let absolute = std::path::absolute(path)
            .map_err(|e| DesignError::Io(format!("cannot resolve {}: {e}", path.display())))?;
        let project = match format {
            FileFormat::Single => None,
            FileFormat::Auto => Project::find(path),
            FileFormat::Project => Some(
                Project::find(path)
                    .ok_or_else(|| DesignError::NotInProject(path.display().to_string()))?,
            ),
        };
        let json = match &project {
            Some(project) => self.to_project_json(&project.store()).map_err(|e| {
                DesignError::Io(format!(
                    "cannot write the B-rep data of {} to {}: {e}",
                    path.display(),
                    project.store().dir().display()
                ))
            })?,
            None => self.to_json(),
        };
        write_atomically(path, json.as_bytes())
            .map_err(|e| DesignError::Io(format!("cannot write {}: {e}", path.display())))?;
        if let Some(local) = project.and_then(|p| p.local_state_path(path)) {
            let display = &self.state().display;
            let _ = if display.is_default() {
                std::fs::remove_file(&local)
            } else {
                let text = serde_json::json!({
                    "format": LOCAL_FORMAT, "version": 1, "display": display,
                });
                write_atomically(&local, format!("{text:#}\n").as_bytes())
            };
        }
        if let Some(directory) = absolute.parent() {
            self.set_source_directory(directory);
        }
        Ok(json)
    }

    /// Reads a design file of any version from `path`, its B-rep data
    /// from the store of the project it is in. Nothing is evaluated yet.
    pub fn load_design(path: &Path, kernel: K) -> Result<Self, DesignError> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| DesignError::Io(format!("cannot read {}: {e}", path.display())))?;
        Ok(Self::from_json_at(&json, path, kernel)?)
    }

    /// Reads the text of the design file at `path` (read by the caller),
    /// its B-rep data from the store of the project the path is in.
    pub fn from_json_at(json: &str, path: &Path, kernel: K) -> Result<Self, FileError> {
        let absolute = std::path::absolute(path)
            .map_err(|e| FileError::Invalid(format!("cannot resolve {}: {e}", path.display())))?;
        let (state, warnings) = load_state_at(json, path)?;
        let document = Document::from_state(kernel, state).with_warnings(warnings);
        if let Some(directory) = absolute.parent() {
            document.set_source_directory(directory);
        }
        Ok(document)
    }

    /// Reads a design file, its B-rep data from `store` (e.g. an older
    /// version's, P12b).
    pub fn from_json_in(json: &str, store: &dyn BlobStore, kernel: K) -> Result<Self, FileError> {
        let (mut state, mut warnings) = load_state(json)?;
        resolve(&mut state, store, &mut warnings);
        Ok(Document::from_state(kernel, state).with_warnings(warnings))
    }

    /// The design file of this document with the timeline marker at
    /// `marker` (clamped to the timeline). An edit rolls the marker back
    /// while it is open; the application's autosave writes the marker the
    /// edit restores (P8). The document does not change.
    pub fn to_json_with_marker(&self, marker: usize) -> String {
        let state = self.state();
        if marker == state.marker {
            return v2::save(state);
        }
        let mut moved = state.clone();
        moved.marker = marker.min(moved.features.len());
        v2::save(&moved)
    }

    /// Reads a design file of version 1, 2 or 3. Nothing is evaluated yet;
    /// call [`Document::recompute`] to build the geometry. Values the
    /// timeline accepts but recompute rejects, like a zero width, load and
    /// are then reported by recompute, as after a parameter change. The
    /// B-rep references of a version 3 file stay unresolved (their base
    /// features fail); [`Document::load_design`] and
    /// [`Document::from_json_at`] find the project's store.
    pub fn from_json(json: &str, kernel: K) -> Result<Self, FileError> {
        let (state, warnings) = load_state(json)?;
        Ok(Document::from_state(kernel, state).with_warnings(warnings))
    }
}

/// The definition state of a design file (version 1, 2 or 3) and what
/// loading changed. B-rep references stay unresolved.
#[doc(hidden)]
pub fn load_state(json: &str) -> Result<(DocState, Vec<String>), FileError> {
    // Some Windows editors start UTF-8 text with a byte order mark.
    let json = json.strip_prefix('\u{feff}').unwrap_or(json);
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| FileError::Syntax(e.to_string()))?;
    // The B-rep data inside the file decodes within one budget, so that
    // many bodies cannot ask for many times one body's limit (mitcad#928).
    let _budget = crate::features::base::FileBudget::start();
    let (mut state, warnings) = match check_header(&value)? {
        1 => v1::load(json),
        _ => v2::load(value),
    }?;
    // Ids in the top of their type would leave the session too few to
    // hand out (mitcad#944).
    id_reserve::check(&state)?;
    // Driven dimensions of older files get their reference parameters, and
    // stored ones their kind (mitcad#231).
    state
        .settle_references()
        .map_err(|e| FileError::Invalid(e.to_string()))?;
    Ok((state, warnings))
}

/// The definition state of the design file at `path` (its text read by
/// the caller), its B-rep references resolved from the store of the
/// project the path is in; inserted components read other files with it.
pub(crate) fn load_state_at(json: &str, path: &Path) -> Result<(DocState, Vec<String>), FileError> {
    let (mut state, mut warnings) = load_state(json)?;
    match Project::find(path) {
        Some(project) => {
            if has_references(&state) {
                resolve(&mut state, &project.store(), &mut warnings);
            }
            if let Some(display) = local_display(&project, path, &mut warnings) {
                state.display = display;
            }
        }
        None if has_references(&state) => warnings.push(format!(
            "{} is not in a Mitcad project (a folder with {PROJECT_MARKER} in it or above \
             it), so the B-rep data it refers to is not found",
            path.display()
        )),
        None => {}
    }
    Ok((state, warnings))
}

/// The definition state of a design file whose B-rep data is in `store`
/// (a library component read from a commit, mitcad#64).
pub(crate) fn load_state_in(
    json: &str,
    store: &dyn BlobStore,
) -> Result<(DocState, Vec<String>), FileError> {
    let (mut state, mut warnings) = load_state(json)?;
    if has_references(&state) {
        resolve(&mut state, store, &mut warnings);
    }
    Ok((state, warnings))
}

/// The `"format"` of a project's per-user state files.
const LOCAL_FORMAT: &str = "mitcad-local";

/// The display state kept for the design file at `path` in the project's
/// per-user state, if there is one; one that cannot be read is left out
/// with a warning.
fn local_display(
    project: &Project,
    path: &Path,
    warnings: &mut Vec<String>,
) -> Option<DisplayState> {
    let local = project.local_state_path(path)?;
    let text = std::fs::read_to_string(&local).ok()?;
    #[derive(serde::Deserialize)]
    struct Local {
        format: String,
        #[serde(default)]
        display: DisplayState,
    }
    match serde_json::from_str::<Local>(&text) {
        Ok(local) if local.format == LOCAL_FORMAT => Some(local.display),
        Ok(_) => None,
        Err(e) => {
            warnings.push(format!(
                "the display state in {} is left out: {e}",
                local.display()
            ));
            None
        }
    }
}

/// The B-rep data that a design file's base features refer to by SHA-256
/// (version 3), read without loading the file: what a project's store must
/// keep for it (the version history's clean-up, P12b). Data inside the
/// file (version 2) is no reference. Text that is not a design file this
/// build reads is an error, so a caller does not take it for a file that
/// refers to nothing.
pub fn brep_references(json: &str) -> Result<BTreeSet<Sha256>, FileError> {
    #[derive(serde::Deserialize)]
    struct Head {
        format: Option<serde_json::Value>,
        version: Option<serde_json::Value>,
        #[serde(default)]
        features: Vec<serde_json::Value>,
        #[serde(default)]
        library_sources: Vec<serde_json::Value>,
    }
    let json = json.strip_prefix('\u{feff}').unwrap_or(json);
    let head: Head = serde_json::from_str(json).map_err(|e| FileError::Syntax(e.to_string()))?;
    let mut header = serde_json::Map::new();
    for (key, value) in [("format", head.format), ("version", head.version)] {
        if let Some(value) = value {
            header.insert(key.to_owned(), value);
        }
    }
    check_header(&serde_json::Value::Object(header))?;
    let mut out = BTreeSet::new();
    for (i, feature) in head.features.iter().enumerate() {
        if feature.get("type").and_then(serde_json::Value::as_str) != Some("base") {
            continue;
        }
        let bodies = feature.get("bodies").and_then(serde_json::Value::as_array);
        for (j, body) in bodies.into_iter().flatten().enumerate() {
            let Some(sha256) = body.pointer("/brep/sha256") else {
                continue;
            };
            let sha256 = sha256
                .as_str()
                .and_then(|text| text.parse().ok())
                .ok_or_else(|| {
                    FileError::Invalid(format!(
                        "features[{i}]: bodies[{j}]: brep: sha256 is not a SHA-256"
                    ))
                })?;
            out.insert(sha256);
        }
    }
    // Library sources kept in the store (mitcad#375).
    for (i, source) in head.library_sources.iter().enumerate() {
        if source.pointer("/design/text").is_some() {
            continue;
        }
        let Some(sha256) = source.pointer("/design/sha256") else {
            continue;
        };
        let sha256 = sha256
            .as_str()
            .and_then(|text| text.parse().ok())
            .ok_or_else(|| {
                FileError::Invalid(format!(
                    "library_sources[{i}]: design: sha256 is not a SHA-256"
                ))
            })?;
        out.insert(sha256);
    }
    Ok(out)
}

/// Whether a base feature refers to B-rep data elsewhere.
pub(crate) fn has_references(state: &DocState) -> bool {
    state.features.iter().any(|f| match &f.def {
        FeatureDef::Base(base) => base.bodies.iter().any(|b| b.brep.is_reference()),
        _ => false,
    }) || state.library_sources.has_references()
}

/// A definition state as a single-file design (version 2), its data
/// inside (a library source of BUILD, mitcad#375).
pub(crate) fn single_file(state: &DocState) -> String {
    v2::save(state)
}

/// The design file of a new, empty design (what
/// `Document::new_design(..).to_json()` writes: the root and an empty
/// active Component1, both with the general defaults, mitcad#220,
/// mitcad#421), without a geometry kernel: the projects' `new_design`
/// (mitcad#144).
pub fn empty_design() -> String {
    v2::save(&DocState::new_design())
}

/// Gives the base features' B-rep references their data from `store`.
/// Data the store lacks, or a file that does not hold the data it is named
/// after, leaves the reference (its base feature fails) and adds a warning.
/// Bodies with the same data share it.
fn resolve(state: &mut DocState, store: &dyn BlobStore, warnings: &mut Vec<String>) {
    // Library sources' designs (mitcad#375).
    state.library_sources.resolve(store, warnings);
    let mut found: HashMap<Sha256, Option<Brep>> = HashMap::new();
    // The data read from the store counts against one budget, each piece
    // once however many bodies share it (mitcad#928).
    let mut left = crate::features::base::MAX_FILE_SIZE;
    for (i, entry) in state.features.iter_mut().enumerate() {
        let FeatureDef::Base(base) = &entry.def else {
            continue;
        };
        if !base.bodies.iter().any(|b| b.brep.is_reference()) {
            continue;
        }
        let mut base = base.clone();
        for (j, body) in base.bodies.iter_mut().enumerate() {
            if !body.brep.is_reference() {
                continue;
            }
            let sha256 = body.brep.sha256();
            let brep = found.entry(sha256).or_insert_with(|| {
                let size = body.brep.size();
                let problem = if size > left {
                    format!(
                        "its B-rep data {sha256} is not read: the design's B-rep data exceeds \
                         {} bytes in all",
                        crate::features::base::MAX_FILE_SIZE
                    )
                } else {
                    match store.get(&sha256) {
                        Ok(Some(stored)) => match body.brep.resolve(&stored) {
                            Ok(brep) => {
                                left -= size;
                                return Some(brep);
                            }
                            Err(e) => format!("the file {} is damaged: {e}", brep_path(&sha256)),
                        },
                        Ok(None) => format!(
                            "its B-rep data {sha256} is missing from the project store \
                             ({BREP_DIR})"
                        ),
                        Err(e) => format!("cannot read {}: {e}", brep_path(&sha256)),
                    }
                };
                warnings.push(format!(
                    "features[{i}] ({}): bodies[{j}]: {problem}",
                    entry.name
                ));
                None
            });
            if let Some(brep) = brep {
                body.brep = brep.clone();
            }
        }
        *entry = Arc::new(FeatureEntry {
            def: FeatureDef::Base(base),
            ..(**entry).clone()
        });
    }
}

/// A free name for a parameter of an older file whose name is now a unit,
/// function or constant in expressions (`m`, `E`, `in`, `PI`, `sin`), or
/// None when the name can stay. `taken` holds the names in use.
pub(crate) fn legacy_name(name: &str, taken: &HashSet<String>) -> Option<String> {
    if !is_reserved_name(name) {
        return None;
    }
    (1u32..)
        .map(|n| format!("{name}_{n}"))
        .find(|candidate| !taken.contains(candidate) && !is_reserved_name(candidate))
}

/// Checks the format marker and the version before the schema, so a file
/// from another program or a newer Mitcad gets a clear message.
fn check_header(value: &serde_json::Value) -> Result<u64, FileError> {
    if value.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(FileError::NotADesign);
    }
    match value.get("version") {
        Some(version) if matches!(version.as_u64(), Some(1..=VERSION)) => {
            Ok(version.as_u64().expect("checked"))
        }
        Some(version) if version.as_u64().is_some_and(|v| v > VERSION) => {
            Err(FileError::Version(format!(
                "the file is version {version}, newer than this Mitcad can read \
                 (version {VERSION}); update Mitcad to open it"
            )))
        }
        Some(version) => Err(FileError::Version(format!(
            "unsupported design file version {version}"
        ))),
        None => Err(FileError::Version(
            "the design file has no \"version\"".to_owned(),
        )),
    }
}

#[cfg(test)]
mod project_tests;
