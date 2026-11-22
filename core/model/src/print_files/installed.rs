// SPDX-License-Identifier: MIT
//! The user presets of the installed Bambu-family slicers (mitcad#830):
//! where each keeps its data folder on each platform, the printer,
//! process and filament presets the user made there, and a preset with
//! the values it inherits from the slicer's system presets filled in.
//! Read only: the slicer's files are never changed.
//!
//! The layout, as the slicers' own sources make it (read 27.10.2026):
//!
//! - The data folder is named by the slicer's `SLIC3R_APP_KEY`
//!   (`version.inc` of each: `BambuStudio`, `OrcaSlicer`, `QIDIStudio`).
//!   `GUI_App::init_app_config` (`src/slic3r/GUI/GUI_App.cpp`) puts it in
//!   wxWidgets' user data folder, `%APPDATA%\<key>` on Windows and
//!   `~/Library/Application Support/<key>` on macOS, and on Linux in
//!   `$XDG_CONFIG_HOME/<key>`, else `~/.config/<key>`. A Flatpak's
//!   `XDG_CONFIG_HOME` is `~/.var/app/<app id>/config`: Flathub's
//!   `com.bambulab.BambuStudio` and `com.orcaslicer.OrcaSlicer`, and
//!   `io.github.softfever.OrcaSlicer`, the id OrcaSlicer's
//!   `migrate_flatpak_legacy_datadir` (same file) moves data from.
//! - User presets are in `<data>/user/<account>/` (`PRESET_USER_DIR`
//!   `"user"` in `src/libslic3r/Preset.hpp`; the account's id, or
//!   `DEFAULT_USER_FOLDER_NAME` `"default"` in `PresetBundle.hpp` when no
//!   one is signed in), one folder per kind: `machine`, `process`,
//!   `filament` (`PRESET_PRINTER_NAME`, `PRESET_PRINT_NAME`,
//!   `PRESET_FILAMENT_NAME`), each preset a `.json` file beside its
//!   `.info`.
//! - System presets are in `<data>/system/` (`PRESET_SYSTEM_DIR`), a
//!   folder per vendor; a preset names the one it inherits from by its
//!   `name` (`inherits`), which may inherit further. Vendors reuse the
//!   names of their bases (`fdm_machine_common`), so a base is looked for
//!   in its preset's vendor folder first.
//!
//! The slicer's own presets the user enabled (mitcad#871; read in the
//! sources and checked against installed slicers on 28.10.2026):
//!
//! - `<data>/system/` holds the vendors of the printers the user enabled,
//!   each whole; the slicer loads its presets from there
//!   (`load_system_presets_from_json`), the install folder's
//!   `resources/profiles/` only gives the setup wizard's printer models.
//! - A preset file says whether it is meant to be chosen: `instantiation`
//!   `"false"` on the bases (`fdm_process_common`, `… @base`), else it is
//!   (`is_visible = instantiation != "false"`); `type` `machine_model`
//!   files are printer models and files without `type` no presets.
//! - The app config `<data>/<key>.conf` (JSON, then a `# MD5 checksum`
//!   line) records what the user enabled: `models`, `[{vendor, model,
//!   nozzle_diameter: "0.2;0.4"}]`, the printers and nozzles, a system
//!   printer shown when its (inherited) `printer_model` and
//!   `printer_variant` are among them for its vendor folder
//!   (`Preset::set_visible_from_appconfig`, `AppConfig::get_variant`);
//!   `filaments`, the names of the filaments shown, to which
//!   `PresetBundle::load_installed_filaments` adds a printer model's
//!   `default_materials` when none listed suits one of its printers.
//!   Processes have no list: those for a shown printer
//!   (`compatible_printers`) are shown, and a filament only for the
//!   printers it is compatible with.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::source;
use crate::print_keys::{Dialect, PrintSet};

/// Which platform's folders the slicers use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    Linux,
}

/// The folders a slicer's data folder is found from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetEnv {
    pub platform: Platform,
    /// The user's home folder.
    pub home: Option<PathBuf>,
    /// Windows' `%APPDATA%`.
    pub app_data: Option<PathBuf>,
    /// Linux's `$XDG_CONFIG_HOME`.
    pub config_home: Option<PathBuf>,
}

impl PresetEnv {
    /// This computer's.
    pub fn current() -> PresetEnv {
        let platform = if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Linux
        };
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        PresetEnv {
            platform,
            home: var("HOME").or_else(|| var("USERPROFILE")),
            app_data: var("APPDATA"),
            config_home: var("XDG_CONFIG_HOME"),
        }
    }
}

/// The Bambu-family slicers: their dialect, data folder name and Flatpak
/// app ids (see the module documentation for the sources).
const SLICERS: [(Dialect, &str, &[&str]); 3] = [
    (
        Dialect::BambuStudio,
        "BambuStudio",
        &["com.bambulab.BambuStudio"],
    ),
    (
        Dialect::OrcaSlicer,
        "OrcaSlicer",
        &[
            "com.orcaslicer.OrcaSlicer",
            "io.github.softfever.OrcaSlicer",
        ],
    ),
    (Dialect::QidiStudio, "QIDIStudio", &[]),
];

/// The user presets' folder in a data folder.
const USER_DIR: &str = "user";
/// The system presets' folder in a data folder.
const SYSTEM_DIR: &str = "system";
/// The folders of each kind in an account's folder.
const KIND_DIRS: [(&str, PrintSet); 3] = [
    ("machine", PrintSet::Printer),
    ("process", PrintSet::Process),
    ("filament", PrintSet::Filament),
];

/// A slicer's data folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlicerFolder {
    pub dialect: Dialect,
    pub folder: PathBuf,
    /// The slicer's Flatpak's (Linux).
    pub flatpak: bool,
}

/// Where each Bambu-family slicer keeps its data folder in `env`'s layout,
/// whether it is there or not: one per slicer, on Linux also its Flatpak's.
pub fn candidate_folders(env: &PresetEnv) -> Vec<SlicerFolder> {
    let mut out = Vec::new();
    for (dialect, key, flatpaks) in SLICERS {
        let mut add = |folder: PathBuf, flatpak: bool| {
            out.push(SlicerFolder {
                dialect,
                folder,
                flatpak,
            });
        };
        match env.platform {
            Platform::Windows => {
                if let Some(app_data) = &env.app_data {
                    add(app_data.join(key), false);
                }
            }
            Platform::MacOs => {
                if let Some(home) = &env.home {
                    add(
                        home.join("Library").join("Application Support").join(key),
                        false,
                    );
                }
            }
            Platform::Linux => {
                let config = env
                    .config_home
                    .clone()
                    .or_else(|| env.home.as_ref().map(|h| h.join(".config")));
                if let Some(config) = config {
                    add(config.join(key), false);
                }
                if let Some(home) = &env.home {
                    for id in flatpaks {
                        let folder = home.join(".var").join("app").join(id);
                        add(folder.join("config").join(key), true);
                    }
                }
            }
        }
    }
    out
}

/// The data folders of the slicers installed in `env`: those that have a
/// user presets' folder, or system presets and an app config that enables
/// printers (mitcad#871: a slicer set up but with no preset of the user's
/// yet still has presets to import).
pub fn slicer_folders(env: &PresetEnv) -> Vec<SlicerFolder> {
    candidate_folders(env)
        .into_iter()
        .filter(|s| s.folder.join(USER_DIR).is_dir() || enables_printers(&s.folder, s.dialect))
        .collect()
}

/// Whether a data folder has system presets and its app config lists
/// printer models (`models`).
fn enables_printers(folder: &Path, dialect: Dialect) -> bool {
    folder.join(SYSTEM_DIR).is_dir()
        && app_config(folder, dialect).is_some_and(|config| {
            config
                .get("models")
                .and_then(Value::as_array)
                .is_some_and(|models| !models.is_empty())
        })
}

/// A preset found in a slicer's data folder: a user preset, or a system
/// preset the user enabled (`vendor`).
#[derive(Debug, Clone, PartialEq)]
pub struct FoundPreset {
    /// Printer (a `machine` preset), process or filament.
    pub set: PrintSet,
    /// Its `name`, else its file's name.
    pub name: String,
    pub path: PathBuf,
    /// The account folder it is in (`default` without one); empty for a
    /// system preset.
    pub account: String,
    /// The preset it inherits from, by name.
    pub inherits: Option<String>,
    /// A system preset's vendor folder (`BBL`); None for a user preset.
    pub vendor: Option<String>,
    /// The printer presets it names as compatible (`compatible_printers`;
    /// a user preset's own only), for a filament's variants (mitcad#1054).
    pub compatible: Vec<String>,
}

/// A preset's `compatible_printers`: a list of names, or `"a";"b"`.
fn compatible_of(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
            .collect(),
        Some(Value::String(text)) => text
            .split(';')
            .map(|t| t.trim().trim_matches('"').to_owned())
            .filter(|t| !t.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// The sorted entries of a folder.
fn entries(dir: &Path) -> Vec<std::fs::DirEntry> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<_> = read.filter_map(Result::ok).collect();
    out.sort_by_key(std::fs::DirEntry::file_name);
    out
}

/// The `.json` files under `dir`, down to `depth` folders below it.
fn json_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    for entry in entries(dir) {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if depth > 0 {
                json_files(&path, depth - 1, out);
            }
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        {
            out.push(path);
        }
    }
}

/// A preset file's fields; None when it is no JSON object.
fn read_preset_file(path: &Path) -> Option<Map<String, Value>> {
    let data = std::fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

fn text_of(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

/// The user presets in a slicer's data folder: every account's printers,
/// processes and filaments (also those in a folder below a kind's, such
/// as `filament/base`), by account, kind and file name. Files that are no
/// preset are left out.
pub fn user_presets(folder: &Path) -> Vec<FoundPreset> {
    let mut out = Vec::new();
    for account in entries(&folder.join(USER_DIR)) {
        if !account.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let account_name = account.file_name().to_string_lossy().into_owned();
        for (dir, set) in KIND_DIRS {
            let mut files = Vec::new();
            json_files(&account.path().join(dir), 1, &mut files);
            for path in files {
                let Some(map) = read_preset_file(&path) else {
                    continue;
                };
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                out.push(FoundPreset {
                    set,
                    name: text_of(&map, "name").unwrap_or(stem),
                    path: path.clone(),
                    account: account_name.clone(),
                    inherits: text_of(&map, "inherits"),
                    vendor: None,
                    compatible: compatible_of(map.get("compatible_printers")),
                });
            }
        }
    }
    out
}

/// The folder name of a slicer's data folder and app config (`<key>.conf`).
fn app_key(dialect: Dialect) -> Option<&'static str> {
    SLICERS
        .iter()
        .find(|(d, _, _)| *d == dialect)
        .map(|(_, key, _)| *key)
}

/// A slicer's app config `<data>/<key>.conf`: JSON, its `#` lines (the
/// checksum after it) left out. None when it is missing or unreadable.
fn app_config(folder: &Path, dialect: Dialect) -> Option<Map<String, Value>> {
    let key = app_key(dialect)?;
    let text = std::fs::read_to_string(folder.join(format!("{key}.conf"))).ok()?;
    let json: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::from_str(&json).ok()
}

/// The fields of a system preset that say whether the user enabled it.
const SYSTEM_FIELDS: [&str; 6] = [
    "inherits",
    "printer_model",
    "printer_variant",
    "compatible_printers",
    "compatible_printers_condition",
    "default_materials",
];

/// A system preset file: its name, vendor folder, `type`, whether it is
/// meant to be chosen, and the [`SYSTEM_FIELDS`] it sets itself.
struct SystemPreset {
    name: String,
    path: PathBuf,
    vendor: String,
    kind: String,
    instantiable: bool,
    fields: Map<String, Value>,
}

/// The system presets of a data folder, by name.
struct SystemIndex {
    presets: Vec<SystemPreset>,
    by_name: HashMap<String, Vec<usize>>,
}

impl SystemIndex {
    fn of(folder: &Path) -> SystemIndex {
        let mut presets = Vec::new();
        for vendor in entries(&folder.join(SYSTEM_DIR)) {
            if !vendor.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let vendor_name = vendor.file_name().to_string_lossy().into_owned();
            let mut files = Vec::new();
            json_files(&vendor.path(), 3, &mut files);
            for path in files {
                let Some(map) = read_preset_file(&path) else {
                    continue;
                };
                let Some(name) = text_of(&map, "name") else {
                    continue;
                };
                // The slicer's rule: chosen unless `instantiation` is false.
                let instantiable = !matches!(map.get("instantiation"), Some(Value::Bool(false)))
                    && !map
                        .get("instantiation")
                        .and_then(Value::as_str)
                        .is_some_and(|t| t.trim().eq_ignore_ascii_case("false"));
                let fields = SYSTEM_FIELDS
                    .iter()
                    .filter_map(|key| map.get(*key).map(|v| ((*key).to_owned(), v.clone())))
                    .collect();
                presets.push(SystemPreset {
                    name,
                    path,
                    vendor: vendor_name.clone(),
                    kind: text_of(&map, "type").unwrap_or_default(),
                    instantiable,
                    fields,
                });
            }
        }
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, preset) in presets.iter().enumerate() {
            by_name.entry(preset.name.clone()).or_default().push(i);
        }
        SystemIndex { presets, by_name }
    }

    /// The preset of `name`, the one of `vendor` first.
    fn find(&self, name: &str, vendor: &str) -> Option<usize> {
        let found = self.by_name.get(name)?;
        found
            .iter()
            .copied()
            .find(|&i| self.presets[i].vendor == vendor)
            .or_else(|| found.first().copied())
    }

    /// A field of preset `i`, its own or the nearest it inherits.
    fn field(&self, mut i: usize, key: &str) -> Option<&Value> {
        for _ in 0..=MAX_INHERITS {
            let preset = &self.presets[i];
            if let Some(value) = preset.fields.get(key) {
                return Some(value);
            }
            let base = preset.fields.get("inherits").and_then(Value::as_str)?;
            i = self.find(base.trim(), &preset.vendor)?;
        }
        None
    }

    /// A text field of preset `i` (a list's first entry), when not empty.
    fn text(&self, i: usize, key: &str) -> Option<String> {
        let text = match self.field(i, key)? {
            Value::String(text) => Some(text.trim().to_owned()),
            Value::Array(items) => items
                .first()
                .and_then(Value::as_str)
                .map(|t| t.trim().to_owned()),
            _ => None,
        };
        text.filter(|t| !t.is_empty())
    }

    /// The printers a preset is for: none named is every printer.
    fn compatible(&self, i: usize) -> Vec<String> {
        compatible_of(self.field(i, "compatible_printers"))
    }

    /// Whether preset `i` is for one of `printers`.
    fn suits_any(&self, i: usize, printers: &BTreeSet<String>) -> bool {
        let compatible = self.compatible(i);
        if compatible.is_empty() {
            return !printers.is_empty();
        }
        compatible.iter().any(|p| printers.contains(p))
    }
}

/// The filament names an app config's `filaments` lists: an array of
/// names, or an object whose names have a value.
fn listed_filaments(config: &Map<String, Value>) -> Vec<String> {
    match config.get("filaments") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|t| t.trim().to_owned())
            .collect(),
        Some(Value::Object(map)) => map
            .iter()
            .filter(|(_, v)| match v {
                Value::String(t) => !t.trim().is_empty() && t.trim() != "false",
                Value::Bool(b) => *b,
                Value::Null => false,
                _ => true,
            })
            .map(|(name, _)| name.trim().to_owned())
            .collect(),
        _ => Vec::new(),
    }
}

/// The system presets the user enabled in the slicer `dialect` whose data
/// folder is `folder` (see the module documentation): the printers of the
/// app config's `models`, the processes for one of them, and the
/// filaments of its `filaments` (with the printer models' default
/// materials where none listed suits a printer) that suit one of them;
/// only presets meant to be chosen, never the bases. By kind, then name
/// in any case.
/// None without an app config.
pub fn system_presets(folder: &Path, dialect: Dialect) -> Vec<FoundPreset> {
    let Some(config) = app_config(folder, dialect) else {
        return Vec::new();
    };
    let index = SystemIndex::of(folder);
    let chosen = |kind: &str| -> Vec<usize> {
        (0..index.presets.len())
            .filter(|&i| index.presets[i].kind == kind && index.presets[i].instantiable)
            .collect()
    };
    // The printers: their model and nozzle among the vendor's models.
    let models: Vec<(String, String, Vec<String>)> = config
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let text = |key: &str| {
                m.get(key)
                    .and_then(Value::as_str)
                    .map(|t| t.trim().to_owned())
            };
            let nozzles = text("nozzle_diameter")
                .unwrap_or_default()
                .split(';')
                .map(|n| n.trim().to_owned())
                .filter(|n| !n.is_empty())
                .collect();
            Some((text("vendor")?, text("model")?, nozzles))
        })
        .collect();
    let printers: Vec<usize> = chosen("machine")
        .into_iter()
        .filter(|&i| {
            let (Some(model), Some(variant)) = (
                index.text(i, "printer_model"),
                index.text(i, "printer_variant"),
            ) else {
                return false;
            };
            models.iter().any(|(vendor, m, nozzles)| {
                *vendor == index.presets[i].vendor
                    && *m == model
                    && nozzles.iter().any(|n| same_nozzle(n, &variant))
            })
        })
        .collect();
    let printer_names: BTreeSet<String> = printers
        .iter()
        .map(|&i| index.presets[i].name.clone())
        .collect();
    let processes: Vec<usize> = chosen("process")
        .into_iter()
        .filter(|&i| index.suits_any(i, &printer_names))
        .collect();
    // The filaments listed, and a printer's default materials when none
    // listed suits it.
    let mut names: Vec<String> = listed_filaments(&config);
    for &p in &printers {
        let me = BTreeSet::from([index.presets[p].name.clone()]);
        let suited = names.iter().any(|n| {
            index.by_name.get(n).is_some_and(|found| {
                found
                    .iter()
                    .any(|&f| index.presets[f].kind == "filament" && index.suits_any(f, &me))
            })
        });
        if suited {
            continue;
        }
        let vendor = &index.presets[p].vendor;
        let model = index.text(p, "printer_model").unwrap_or_default();
        let defaults = index
            .by_name
            .get(&model)
            .into_iter()
            .flatten()
            .find(|&&m| {
                index.presets[m].kind == "machine_model" && index.presets[m].vendor == *vendor
            })
            .and_then(|&m| index.presets[m].fields.get("default_materials"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        for name in defaults.split(';').map(str::trim).filter(|n| !n.is_empty()) {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_owned());
            }
        }
    }
    let mut filaments: Vec<usize> = Vec::new();
    for name in &names {
        let found = index
            .by_name
            .get(name)
            .into_iter()
            .flatten()
            .copied()
            .find(|&i| {
                let p = &index.presets[i];
                p.kind == "filament" && p.instantiable && index.suits_any(i, &printer_names)
            });
        if let Some(i) = found
            && !filaments.contains(&i)
        {
            filaments.push(i);
        }
    }
    let mut out = Vec::new();
    for (set, mut list) in [
        (PrintSet::Printer, printers),
        (PrintSet::Process, processes),
        (PrintSet::Filament, filaments),
    ] {
        // By name in any case, then as written.
        list.sort_by_cached_key(|&i| {
            let name = &index.presets[i].name;
            (name.to_lowercase(), name.clone())
        });
        for i in list {
            let preset = &index.presets[i];
            out.push(FoundPreset {
                set,
                name: preset.name.clone(),
                path: preset.path.clone(),
                account: String::new(),
                inherits: preset
                    .fields
                    .get("inherits")
                    .and_then(Value::as_str)
                    .map(|t| t.trim().to_owned())
                    .filter(|t| !t.is_empty()),
                vendor: Some(preset.vendor.clone()),
                compatible: index.compatible(i),
            });
        }
    }
    out
}

/// Whether two nozzle diameters (`0.4`, `0.40`) are the same.
fn same_nozzle(a: &str, b: &str) -> bool {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() < 1e-6,
        _ => a == b,
    }
}

/// The kind of preset a file under an account's or a vendor's folder is,
/// by its folder.
fn set_of(folder: &Path, path: &Path) -> Option<PrintSet> {
    let relative = path
        .strip_prefix(folder.join(USER_DIR))
        .or_else(|_| path.strip_prefix(folder.join(SYSTEM_DIR)))
        .ok()?;
    let kind = relative.components().nth(1)?.as_os_str().to_str()?;
    KIND_DIRS
        .iter()
        .find(|(dir, _)| *dir == kind)
        .map(|(_, set)| *set)
}

/// The vendor folder a system preset's file is in.
fn vendor_dir(folder: &Path, path: &Path) -> Option<PathBuf> {
    let system = folder.join(SYSTEM_DIR);
    let first = path.strip_prefix(&system).ok()?.components().next()?;
    Some(system.join(first.as_os_str()))
}

/// The presets a preset may inherit from, by name: the user's own of the
/// same account first, then the system presets.
struct Bases {
    presets: Vec<(String, PathBuf)>,
}

impl Bases {
    fn of(folder: &Path, account: Option<&Path>) -> Bases {
        let mut files = Vec::new();
        if let Some(account) = account {
            json_files(account, 2, &mut files);
        }
        json_files(&folder.join(SYSTEM_DIR), 4, &mut files);
        let presets = files
            .into_iter()
            .filter_map(|path| {
                let map = read_preset_file(&path)?;
                Some((text_of(&map, "name")?, path))
            })
            .collect();
        Bases { presets }
    }

    /// The preset of `name`, the one in the folder `prefer` first.
    fn find(&self, name: &str, prefer: Option<&Path>) -> Option<&Path> {
        let named = || self.presets.iter().filter(move |(n, _)| n == name);
        prefer
            .and_then(|dir| named().find(|(_, p)| p.starts_with(dir)))
            .or_else(|| named().next())
            .map(|(_, p)| p.as_path())
    }
}

/// The deepest chain of `inherits` followed.
const MAX_INHERITS: usize = 16;

/// A preset with the values it inherits filled in ([`resolve_preset`]),
/// and the chain of presets they came from (mitcad#1053).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPreset {
    /// The preset's keys with those of its chain (see [`resolved_preset`]).
    pub map: Map<String, Value>,
    /// The presets it inherits from that were found, the nearest first.
    pub chain: Vec<String>,
    /// The base the chain names last but that was not found.
    pub missing: Option<String>,
    /// The templates the chain includes (`include`, mitcad#2008) whose
    /// values were filled in, in the order applied.
    pub templates: Vec<String>,
    /// The templates the chain names but that were not found: their
    /// values are not in [`ResolvedPreset::map`], whose `include` names
    /// them.
    pub missing_templates: Vec<String>,
}

impl ResolvedPreset {
    /// The chain as the import's log line names it: `A > B`, a missing
    /// base last as `C (missing)`, `nothing` when the preset inherits
    /// nothing.
    pub fn chain_text(&self) -> String {
        chain_text(&self.chain, self.missing.as_deref())
    }
}

/// A chain of presets as the import's log line names it (see
/// [`ResolvedPreset::chain_text`]).
pub fn chain_text(chain: &[String], missing: Option<&str>) -> String {
    let mut names: Vec<String> = chain.to_vec();
    if let Some(base) = missing {
        names.push(format!("{base} (missing)"));
    }
    if names.is_empty() {
        "nothing".to_owned()
    } else {
        names.join(" > ")
    }
}

/// A preset's name: its `name`, else its file's.
fn preset_name(own: &Map<String, Value>, path: &Path) -> String {
    text_of(own, "name").unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    })
}

/// A user or system preset of a slicer's data folder `folder` with the
/// values it inherits filled in: from the preset it names in `inherits`
/// (the user's own of its account, else a system preset, one of the
/// vendor folder the chain is in first), and so on up the chain, the
/// nearer preset's value winning, for every key; each preset of the chain
/// with the templates it includes (`include`, mitcad#2008: found by name
/// as its bases are) under its own values, a later template's over an
/// earlier one's. The result is named as the preset and says its kind
/// (`type` and the slicer's id key of its kind), and has no `inherits`
/// when the whole chain was found and no `include` when every template
/// was; a base that is missing leaves `inherits` naming it, templates that
/// are missing leave `include` naming them, so that the import reports
/// the values not read. Templates that include each other in a loop, or
/// nest deeper than the resolver follows, are an error.
pub fn resolved_preset(folder: &Path, path: &Path) -> Result<Map<String, Value>, String> {
    resolve_preset(folder, path).map(|resolved| resolved.map)
}

/// [`resolved_preset`] with the chain it followed (mitcad#1053).
pub fn resolve_preset(folder: &Path, path: &Path) -> Result<ResolvedPreset, String> {
    let shown = path.display();
    let own = read_preset_file(path).ok_or_else(|| format!("{shown} is no preset file"))?;
    let account = path
        .strip_prefix(folder.join(USER_DIR))
        .ok()
        .and_then(|r| r.components().next())
        .map(|c| folder.join(USER_DIR).join(c.as_os_str()));
    let name = preset_name(&own, path);
    resolve_in(
        folder,
        own,
        name,
        set_of(folder, path),
        account.as_deref(),
        vendor_dir(folder, path),
    )
}

/// Fills in what `own` (named `name`, of kind `set` when known) inherits
/// from the presets of the data folder `folder`: the user's of `account`
/// first, then the system presets, those of `vendor` first.
fn resolve_in(
    folder: &Path,
    own: Map<String, Value>,
    name: String,
    set: Option<PrintSet>,
    account: Option<&Path>,
    vendor: Option<PathBuf>,
) -> Result<ResolvedPreset, String> {
    resolve_in_files(folder, own, name, set, account, vendor, None)
}

/// Resolves a preset using only an in-memory snapshot supplied by the caller.
/// No installed slicers, libraries or filesystem paths are consulted.
pub fn resolve_preset_from_files(
    folder: &Path,
    path: &Path,
    files: &BTreeMap<PathBuf, Map<String, Value>>,
) -> Result<ResolvedPreset, String> {
    let own = files
        .get(path)
        .cloned()
        .ok_or_else(|| format!("{} is no preset file", path.display()))?;
    let account = path
        .strip_prefix(folder.join(USER_DIR))
        .ok()
        .and_then(|r| r.components().next())
        .map(|c| folder.join(USER_DIR).join(c.as_os_str()));
    let name = preset_name(&own, path);
    resolve_in_files(
        folder,
        own,
        name,
        set_of(folder, path),
        account.as_deref(),
        vendor_dir(folder, path),
        Some(files),
    )
}

/// The presets of a data folder by name, read from the folder or from a
/// caller's snapshot of it ([`resolve_preset_from_files`]).
struct Lookup<'a> {
    folder: &'a Path,
    account: Option<&'a Path>,
    files: Option<&'a BTreeMap<PathBuf, Map<String, Value>>>,
    bases: Option<Bases>,
}

impl Lookup<'_> {
    fn read(&self, path: &Path) -> Option<Map<String, Value>> {
        match self.files {
            Some(files) => files.get(path).cloned(),
            None => read_preset_file(path),
        }
    }

    fn index(&mut self) -> &Bases {
        let (folder, account, files) = (self.folder, self.account, self.files);
        self.bases.get_or_insert_with(|| match files {
            None => Bases::of(folder, account),
            Some(files) => {
                let in_tree = |path: &Path, root: &Path, depth: usize| {
                    path.strip_prefix(root)
                        .is_ok_and(|p| p.components().count() <= depth + 1)
                };
                let eligible = |path: &Path| {
                    account.is_some_and(|a| in_tree(path, a, 2))
                        || in_tree(path, &folder.join(SYSTEM_DIR), 4)
                };
                // Match the native resolver's account-before-system order.
                let mut paths: Vec<_> = files.keys().filter(|p| eligible(p)).collect();
                paths.sort_by_key(|p| (!account.is_some_and(|a| p.starts_with(a)), *p));
                Bases {
                    presets: paths
                        .into_iter()
                        .filter_map(|p| Some((text_of(files.get(p)?, "name")?, p.clone())))
                        .collect(),
                }
            }
        })
    }

    /// The preset named `name` (one in the folder `vendor` first): its
    /// file and fields.
    fn find(&mut self, name: &str, vendor: Option<&Path>) -> Option<(PathBuf, Map<String, Value>)> {
        let path = self.index().find(name, vendor)?.to_path_buf();
        let map = self.read(&path)?;
        Some((path, map))
    }

    /// The vendor folder a found preset is in, else `vendor`.
    fn vendor_of(&self, path: &Path, vendor: Option<&Path>) -> Option<PathBuf> {
        vendor_dir(self.folder, path).or_else(|| vendor.map(Path::to_path_buf))
    }
}

/// The deepest nesting of templates followed (a template that includes or
/// inherits another, mitcad#2008).
const MAX_TEMPLATES: usize = 8;

/// What resolving a preset's templates found (mitcad#2008).
#[derive(Default)]
struct Templates {
    /// The templates whose values were filled in, in the order applied.
    found: Vec<String>,
    /// Those named but not found.
    missing: Vec<String>,
}

/// The values a preset's `include` templates give it (mitcad#2008): each
/// template in the order the preset names them, a later one's values over
/// an earlier one's, and each template with what it inherits and includes
/// itself under its own values; the templates' own bookkeeping (their
/// names, kinds, `instantiation`) left out. `stack` holds the presets and
/// templates being resolved: a template among them is a loop, an error.
fn include_layer(
    lookup: &mut Lookup<'_>,
    own: &Map<String, Value>,
    vendor: Option<&Path>,
    stack: &mut Vec<String>,
    templates: &mut Templates,
) -> Result<Map<String, Value>, String> {
    let mut layer = Map::new();
    for name in source::include_names(own.get("include")) {
        let found = template_layer(lookup, &name, vendor, stack, templates)?;
        if let Some(values) = found {
            templates.found.push(name);
            layer.extend(values);
        } else if !templates.missing.contains(&name) {
            templates.missing.push(name);
        }
    }
    Ok(layer)
}

/// A template's values by its name (mitcad#2008): what it inherits, then
/// what it includes, then its own, without its bookkeeping; None when no
/// preset has the name.
fn template_layer(
    lookup: &mut Lookup<'_>,
    name: &str,
    vendor: Option<&Path>,
    stack: &mut Vec<String>,
    templates: &mut Templates,
) -> Result<Option<Map<String, Value>>, String> {
    let first = stack.first().cloned().unwrap_or_default();
    if stack.iter().any(|s| s == name) {
        return Err(format!(
            "{first}: the templates it includes include or inherit {name} in a loop"
        ));
    }
    if stack.len() > MAX_TEMPLATES {
        return Err(format!(
            "{first}: its templates nest more than {MAX_TEMPLATES} deep"
        ));
    }
    let Some((path, template)) = lookup.find(name, vendor) else {
        return Ok(None);
    };
    let vendor = lookup.vendor_of(&path, vendor);
    stack.push(name.to_owned());
    let mut layer = Map::new();
    if let Some(base) = text_of(&template, "inherits") {
        match template_layer(lookup, &base, vendor.as_deref(), stack, templates)? {
            Some(values) => layer.extend(values),
            None => {
                if !templates.missing.contains(&base) {
                    templates.missing.push(base);
                }
            }
        }
    }
    layer.extend(include_layer(
        lookup,
        &template,
        vendor.as_deref(),
        stack,
        templates,
    )?);
    stack.pop();
    layer.extend(
        template
            .into_iter()
            .filter(|(key, _)| !source::PRESET_METADATA.contains(&key.as_str())),
    );
    Ok(Some(layer))
}

#[allow(clippy::too_many_arguments)]
fn resolve_in_files(
    folder: &Path,
    own: Map<String, Value>,
    name: String,
    set: Option<PrintSet>,
    account: Option<&Path>,
    vendor: Option<PathBuf>,
    files: Option<&BTreeMap<PathBuf, Map<String, Value>>>,
) -> Result<ResolvedPreset, String> {
    let mut lookup = Lookup {
        folder,
        account,
        files,
        bases: None,
    };
    // The chain, each preset with the vendor folder its bases are looked
    // for in first.
    let mut chain = vec![(own, vendor)];
    let mut names = Vec::new();
    let mut missing = None;
    while let Some(base) = chain.last().and_then(|(m, _)| text_of(m, "inherits")) {
        if chain.len() > MAX_INHERITS {
            return Err(format!(
                "{name}: the presets it inherits from inherit in a loop"
            ));
        }
        let vendor = chain.last().and_then(|(_, v)| v.clone());
        match lookup.find(&base, vendor.as_deref()) {
            Some((path, map)) => {
                // A system preset's bases in its own vendor's folder first.
                let vendor = lookup.vendor_of(&path, vendor.as_deref());
                names.push(base);
                chain.push((map, vendor));
            }
            None => {
                missing = Some(base);
                break;
            }
        }
    }
    // The farthest first, each nearer one's values over it, and each
    // preset's included templates under its own values (mitcad#2008: the
    // order the family's loader applies them: the base, the templates in
    // order, the preset's own keys); but a nearer preset's empty
    // `compatible_printers` names no printer, so that the printers of the
    // preset it inherits from stay its own (mitcad#1216: the printers a
    // process or filament is for).
    let mut templates = Templates::default();
    let mut merged = Map::new();
    for (map, vendor) in chain.into_iter().rev() {
        let mut stack = vec![name.clone()];
        let included = include_layer(
            &mut lookup,
            &map,
            vendor.as_deref(),
            &mut stack,
            &mut templates,
        )?;
        merged.extend(included);
        for (key, value) in map {
            if key == "include" {
                continue;
            }
            if key == "compatible_printers"
                && compatible_of(Some(&value)).is_empty()
                && !compatible_of(merged.get(&key)).is_empty()
            {
                continue;
            }
            merged.insert(key, value);
        }
    }
    merged.insert("name".to_owned(), json!(name));
    match &missing {
        Some(base) => {
            merged.insert("inherits".to_owned(), json!(base));
        }
        None => {
            merged.remove("inherits");
        }
    }
    // Templates not found stay named, so that the import reports the
    // values not read (mitcad#2008).
    merged.remove("include");
    if !templates.missing.is_empty() {
        merged.insert("include".to_owned(), json!(templates.missing));
    }
    if let Some(set) = set {
        let (kind, id_key, id) = match set {
            PrintSet::Printer => ("machine", "printer_settings_id", json!(name)),
            PrintSet::Process => ("process", "print_settings_id", json!(name)),
            PrintSet::Filament => ("filament", "filament_settings_id", json!([name])),
        };
        merged.insert("type".to_owned(), json!(kind));
        merged.insert(id_key.to_owned(), id);
    }
    Ok(ResolvedPreset {
        map: merged,
        chain: names,
        missing,
        templates: templates.found,
        missing_templates: templates.missing,
    })
}

/// The slicer data folder a preset file lies in: the folder whose
/// `user/<account>/<kind>/` or `system/<vendor>/<kind>/` holds it, when
/// that folder has system presets.
pub fn data_folder_of(path: &Path) -> Option<PathBuf> {
    for dir in path.ancestors().skip(1) {
        let Some(own) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if own != USER_DIR && own != SYSTEM_DIR {
            continue;
        }
        let below = path.strip_prefix(dir).ok()?.components().count();
        let Some(data) = dir.parent() else {
            continue;
        };
        if below >= 3 && data.join(SYSTEM_DIR).is_dir() {
            return Some(data.to_path_buf());
        }
    }
    None
}

/// Whether a preset's fields name a preset it inherits from (`inherits`)
/// or templates it includes (`include`, mitcad#2008).
pub fn builds_on_another(own: &Map<String, Value>) -> bool {
    text_of(own, "inherits").is_some() || !source::include_names(own.get("include")).is_empty()
}

/// A preset file with what it inherits filled in (mitcad#1053): one in a
/// slicer's data folder through that folder ([`resolve_preset`]), another
/// (a preset exported or copied elsewhere) through the system presets of
/// the data folders `folders` (the installed slicers'), those of
/// `dialect` first, the first that has the preset's base (or one of its
/// templates). None when the file inherits and includes nothing, or no
/// folder has its base or templates: then it is read as it is and the
/// import reports the values it inherits as not read.
pub fn resolve_preset_file(
    path: &Path,
    folders: &[SlicerFolder],
    dialect: Option<Dialect>,
) -> Result<Option<ResolvedPreset>, String> {
    let shown = path.display();
    let own = read_preset_file(path).ok_or_else(|| format!("{shown} is no preset file"))?;
    if !builds_on_another(&own) {
        return Ok(None);
    }
    if let Some(folder) = data_folder_of(path) {
        return resolve_preset(&folder, path).map(Some);
    }
    let name = preset_name(&own, path);
    let mine = folders.iter().filter(|f| Some(f.dialect) == dialect);
    let others = folders.iter().filter(|f| Some(f.dialect) != dialect);
    for slicer in mine.chain(others) {
        let resolved = resolve_in(&slicer.folder, own.clone(), name.clone(), None, None, None)?;
        if !resolved.chain.is_empty() || !resolved.templates.is_empty() {
            return Ok(Some(resolved));
        }
    }
    Ok(None)
}

/// The extruders of a printer as the slicer `dialect` loads it
/// (mitcad#2207): the `nozzle_diameter` entries of its printer preset in
/// that slicer's data folders among `folders`, with what the preset
/// inherits and includes filled in. The preset is the first of `names`
/// that is one (the user's own of an account first, then the system
/// presets), else the first printer preset meant to be chosen whose
/// `printer_model` (its own or inherited) is `model`: every nozzle of a
/// printer model has the same extruders. Printer models' own files
/// (`type` `machine_model`) are no printer presets. None when no preset is
/// found or it names no nozzle. Read only.
pub fn printer_extruders(
    folders: &[SlicerFolder],
    dialect: Dialect,
    names: &[&str],
    model: Option<&str>,
) -> Option<usize> {
    folders
        .iter()
        .filter(|s| s.dialect == dialect)
        .find_map(|slicer| PrinterPresets::of(&slicer.folder).extruders(names, model))
}

/// The printer presets of a slicer's data folder (`machine`, the user's of
/// every account and the system presets), read once.
struct PrinterPresets<'a> {
    folder: &'a Path,
    files: BTreeMap<PathBuf, Map<String, Value>>,
    /// The printer presets' paths, the user's first.
    order: Vec<PathBuf>,
}

impl<'a> PrinterPresets<'a> {
    fn of(folder: &'a Path) -> PrinterPresets<'a> {
        let mut paths = Vec::new();
        for account in entries(&folder.join(USER_DIR)) {
            json_files(&account.path().join("machine"), 1, &mut paths);
        }
        for vendor in entries(&folder.join(SYSTEM_DIR)) {
            json_files(&vendor.path().join("machine"), 2, &mut paths);
        }
        let mut files = BTreeMap::new();
        let mut order = Vec::new();
        for path in paths {
            let Some(map) = read_preset_file(&path) else {
                continue;
            };
            if text_of(&map, "type").as_deref() != Some("machine_model") {
                order.push(path.clone());
            }
            files.insert(path, map);
        }
        PrinterPresets {
            folder,
            files,
            order,
        }
    }

    /// The preset of `path` with what it inherits and includes.
    fn resolved(&self, path: &Path) -> Option<Map<String, Value>> {
        resolve_preset_from_files(self.folder, path, &self.files)
            .ok()
            .map(|r| r.map)
    }

    /// See [`printer_extruders`].
    fn extruders(&self, names: &[&str], model: Option<&str>) -> Option<usize> {
        let named = |name: &str| {
            self.order.iter().find(|p| {
                self.files
                    .get(*p)
                    .and_then(|m| text_of(m, "name"))
                    .as_deref()
                    == Some(name)
            })
        };
        let by_name = names
            .iter()
            .map(|n| n.trim())
            .filter(|n| !n.is_empty())
            .find_map(named)
            .and_then(|path| self.resolved(path));
        let preset = by_name.or_else(|| {
            let model = model.map(str::trim).filter(|m| !m.is_empty())?;
            // Meant to be chosen unless `instantiation` is false (the
            // slicer's rule, as in `SystemIndex::of`).
            let chosen = |map: &Map<String, Value>| match map.get("instantiation") {
                Some(Value::Bool(false)) => false,
                Some(Value::String(t)) => !t.trim().eq_ignore_ascii_case("false"),
                _ => true,
            };
            self.order
                .iter()
                .filter(|p| self.files.get(*p).is_some_and(chosen))
                .filter_map(|p| self.resolved(p))
                .find(|m| text_of(m, "printer_model").as_deref() == Some(model))
        })?;
        match preset.get("nozzle_diameter")? {
            Value::Array(items) if !items.is_empty() => Some(items.len()),
            Value::String(text) => {
                let n = text
                    .split([',', ';'])
                    .filter(|t| !t.trim().is_empty())
                    .count();
                (n > 0).then_some(n)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(platform: Platform, home: &str) -> PresetEnv {
        PresetEnv {
            platform,
            home: Some(PathBuf::from(home)),
            app_data: None,
            config_home: None,
        }
    }

    fn folders(env: &PresetEnv) -> Vec<(Dialect, PathBuf)> {
        candidate_folders(env)
            .into_iter()
            .map(|s| (s.dialect, s.folder))
            .collect()
    }

    #[test]
    fn snapshot_resolution_uses_only_supplied_files() {
        let folder = Path::new("confined-presets");
        let path = folder.join("user/default/machine/child.json");
        let mut files = BTreeMap::new();
        let object = |value: Value| value.as_object().unwrap().clone();
        files.insert(
            path.clone(),
            object(json!({"name":"Child", "inherits":"Base", "height":"190"})),
        );
        files.insert(
            folder.join("system/vendor/machine/base.json"),
            object(json!({"name":"Base", "nozzle_diameter":["0.6"]})),
        );
        // A matching logical name outside the supplied data folder cannot
        // take part, even if a caller included it in the snapshot.
        files.insert(
            PathBuf::from("elsewhere/base.json"),
            object(json!({"name":"Base", "nozzle_diameter":["9"]})),
        );
        let resolved = resolve_preset_from_files(folder, &path, &files).unwrap();
        assert_eq!(resolved.chain, vec!["Base"]);
        assert_eq!(resolved.map["nozzle_diameter"], json!(["0.6"]));
        assert_eq!(resolved.map["height"], "190");
        assert_eq!(resolved.map["type"], "machine");
        files.remove(&folder.join("system/vendor/machine/base.json"));
        let missing = resolve_preset_from_files(folder, &path, &files).unwrap();
        assert_eq!(missing.missing.as_deref(), Some("Base"));
    }

    /// mitcad#2008: a printer that includes templates (as a two-extruder
    /// printer of the family names its machine G-code) gets their values
    /// over those of the presets it inherits from, its own over theirs, a
    /// later template's over an earlier one's; a template's own templates
    /// and base under its values; the templates' bookkeeping never. A
    /// template not found stays named in `include`, a loop is an error.
    #[test]
    fn included_templates_are_resolved_between_the_base_and_the_preset() {
        let folder = Path::new("synthetic-slicer");
        let vendor = folder.join("system/Acme/machine");
        let path = vendor.join("Acme Duo 0.4 nozzle.json");
        let object = |value: Value| value.as_object().unwrap().clone();
        let mut files = BTreeMap::new();
        files.insert(
            vendor.join("common.json"),
            object(json!({"name": "acme_common", "instantiation": "false",
                          "machine_start_gcode": "G28 ; generic start",
                          "machine_end_gcode": "M84 ; generic end",
                          "layer_change_gcode": "; generic layer",
                          "nozzle_diameter": ["0.4"]})),
        );
        files.insert(
            vendor.join("start.json"),
            object(
                json!({"name": "Acme Duo template start", "instantiation": "false",
                          "include": ["Acme Duo template header"],
                          "machine_start_gcode": ";===== machine: Duo\nG28"}),
            ),
        );
        files.insert(
            vendor.join("header.json"),
            object(
                json!({"name": "Acme Duo template header", "instantiation": "false",
                          "machine_start_gcode": "; header only",
                          "time_lapse_gcode": "; duo timelapse"}),
            ),
        );
        files.insert(
            vendor.join("end.json"),
            object(
                json!({"name": "Acme Duo template end", "instantiation": "false",
                          "type": "machine", "setting_id": "T1",
                          "machine_end_gcode": ";===== duo end",
                          "layer_change_gcode": "; template layer"}),
            ),
        );
        files.insert(
            path.clone(),
            object(json!({"type": "machine", "name": "Acme Duo 0.4 nozzle",
                          "inherits": "acme_common", "setting_id": "M1",
                          "include": ["Acme Duo template start", "Acme Duo template end"],
                          "nozzle_diameter": ["0.4", "0.4"],
                          "layer_change_gcode": "; own layer"})),
        );
        let resolved = resolve_preset_from_files(folder, &path, &files).unwrap();
        let map = &resolved.map;
        // The template's own over the one it includes, over the base.
        assert_eq!(map["machine_start_gcode"], ";===== machine: Duo\nG28");
        assert_eq!(map["time_lapse_gcode"], "; duo timelapse");
        assert_eq!(map["machine_end_gcode"], ";===== duo end");
        // The preset's own over its templates'.
        assert_eq!(map["layer_change_gcode"], "; own layer");
        assert_eq!(map["nozzle_diameter"], json!(["0.4", "0.4"]));
        // The preset's bookkeeping, not the templates'.
        assert_eq!(map["setting_id"], "M1");
        assert_eq!(map["name"], "Acme Duo 0.4 nozzle");
        assert_eq!(map["type"], "machine");
        assert!(!map.contains_key("include") && !map.contains_key("inherits"));
        assert_eq!(resolved.chain, vec!["acme_common"]);
        assert_eq!(
            resolved.templates,
            vec![
                "Acme Duo template header",
                "Acme Duo template start",
                "Acme Duo template end"
            ]
        );
        assert!(resolved.missing_templates.is_empty());

        // A template not found: reported, its values not made up.
        files.remove(&vendor.join("end.json"));
        let resolved = resolve_preset_from_files(folder, &path, &files).unwrap();
        assert_eq!(resolved.missing_templates, vec!["Acme Duo template end"]);
        assert_eq!(resolved.map["include"], json!(["Acme Duo template end"]));
        assert_eq!(resolved.map["machine_end_gcode"], "M84 ; generic end");
        assert_eq!(
            resolved.map["machine_start_gcode"],
            ";===== machine: Duo\nG28"
        );

        // Templates that include each other: an error, not a hang.
        files.insert(
            vendor.join("header.json"),
            object(json!({"name": "Acme Duo template header",
                          "include": "Acme Duo template start"})),
        );
        let error = resolve_preset_from_files(folder, &path, &files).unwrap_err();
        assert!(error.contains("loop"), "{error}");
        // A template that includes itself, too.
        files.insert(
            vendor.join("header.json"),
            object(json!({"name": "Acme Duo template header",
                          "include": ["Acme Duo template header"]})),
        );
        assert!(resolve_preset_from_files(folder, &path, &files).is_err());

        // A chain of templates deeper than followed: an error.
        let mut deep = files.clone();
        for i in 0..=MAX_TEMPLATES + 1 {
            deep.insert(
                vendor.join(format!("deep{i}.json")),
                object(json!({"name": format!("deep {i}"),
                              "include": [format!("deep {}", i + 1)]})),
            );
        }
        deep.insert(
            path.clone(),
            object(json!({"type": "machine", "name": "Acme Duo 0.4 nozzle",
                          "include": ["deep 0"]})),
        );
        let error = resolve_preset_from_files(folder, &path, &deep).unwrap_err();
        assert!(error.contains("deep"), "{error}");
    }

    /// mitcad#2008: a preset that includes templates but inherits nothing
    /// is resolved too.
    #[test]
    fn a_preset_of_templates_only_builds_on_another() {
        let own = json!({"name": "T", "include": ["a"]});
        assert!(builds_on_another(own.as_object().unwrap()));
        let own = json!({"name": "T", "include": "a;b"});
        assert!(builds_on_another(own.as_object().unwrap()));
        let own = json!({"name": "T", "include": []});
        assert!(!builds_on_another(own.as_object().unwrap()));
    }

    #[test]
    fn data_folders_follow_each_platform_s_layout() {
        let mut windows = env(Platform::Windows, "C:/Users/u");
        windows.app_data = Some(PathBuf::from("C:/Users/u/AppData/Roaming"));
        let roaming = Path::new("C:/Users/u/AppData/Roaming");
        assert_eq!(
            folders(&windows),
            vec![
                (Dialect::BambuStudio, roaming.join("BambuStudio")),
                (Dialect::OrcaSlicer, roaming.join("OrcaSlicer")),
                (Dialect::QidiStudio, roaming.join("QIDIStudio")),
            ]
        );
        // Without %APPDATA% none.
        assert!(folders(&env(Platform::Windows, "C:/Users/u")).is_empty());

        let support = Path::new("/Users/u/Library/Application Support");
        assert_eq!(
            folders(&env(Platform::MacOs, "/Users/u")),
            vec![
                (Dialect::BambuStudio, support.join("BambuStudio")),
                (Dialect::OrcaSlicer, support.join("OrcaSlicer")),
                (Dialect::QidiStudio, support.join("QIDIStudio")),
            ]
        );

        let home = Path::new("/home/u");
        let flatpak = |id: &str, key: &str| {
            home.join(".var")
                .join("app")
                .join(id)
                .join("config")
                .join(key)
        };
        assert_eq!(
            folders(&env(Platform::Linux, "/home/u")),
            vec![
                (
                    Dialect::BambuStudio,
                    home.join(".config").join("BambuStudio")
                ),
                (
                    Dialect::BambuStudio,
                    flatpak("com.bambulab.BambuStudio", "BambuStudio")
                ),
                (Dialect::OrcaSlicer, home.join(".config").join("OrcaSlicer")),
                (
                    Dialect::OrcaSlicer,
                    flatpak("com.orcaslicer.OrcaSlicer", "OrcaSlicer")
                ),
                (
                    Dialect::OrcaSlicer,
                    flatpak("io.github.softfever.OrcaSlicer", "OrcaSlicer")
                ),
                (Dialect::QidiStudio, home.join(".config").join("QIDIStudio")),
            ]
        );
        // $XDG_CONFIG_HOME in place of ~/.config.
        let mut xdg = env(Platform::Linux, "/home/u");
        xdg.config_home = Some(PathBuf::from("/cfg"));
        assert_eq!(
            folders(&xdg)[0],
            (Dialect::BambuStudio, PathBuf::from("/cfg/BambuStudio"))
        );
    }

    /// A fake home with the slicers' folders, in the test's own folder.
    fn scratch(name: &str) -> PathBuf {
        crate::testing::unique_temp_dir(&format!("installed-{name}"))
    }

    fn write(path: &Path, value: &Value) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value.to_string()).unwrap();
    }

    #[test]
    fn installed_slicers_are_those_with_user_presets_in_a_fake_home() {
        // Linux: OrcaSlicer in ~/.config, Bambu Studio as a Flatpak; QIDI
        // Studio's folder without user presets is not installed.
        let home = scratch("linux");
        std::fs::create_dir_all(home.join(".config/OrcaSlicer/user/default")).unwrap();
        std::fs::create_dir_all(
            home.join(".var/app/com.bambulab.BambuStudio/config/BambuStudio/user/123"),
        )
        .unwrap();
        std::fs::create_dir_all(home.join(".config/QIDIStudio/system")).unwrap();
        let found: Vec<(Dialect, bool)> =
            slicer_folders(&env(Platform::Linux, home.to_str().unwrap()))
                .into_iter()
                .map(|s| (s.dialect, s.flatpak))
                .collect();
        assert_eq!(
            found,
            vec![(Dialect::BambuStudio, true), (Dialect::OrcaSlicer, false)]
        );

        // Windows: %APPDATA%\QIDIStudio.
        let app_data = scratch("windows");
        std::fs::create_dir_all(app_data.join("QIDIStudio/user/default")).unwrap();
        let mut windows = env(Platform::Windows, "unused");
        windows.app_data = Some(app_data.clone());
        assert_eq!(
            slicer_folders(&windows),
            vec![SlicerFolder {
                dialect: Dialect::QidiStudio,
                folder: app_data.join("QIDIStudio"),
                flatpak: false,
            }]
        );

        // macOS: ~/Library/Application Support/BambuStudio.
        let mac = scratch("macos");
        std::fs::create_dir_all(mac.join("Library/Application Support/BambuStudio/user/default"))
            .unwrap();
        let found = slicer_folders(&env(Platform::MacOs, mac.to_str().unwrap()));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].dialect, Dialect::BambuStudio);
    }

    #[test]
    fn user_presets_are_listed_and_resolved_through_their_system_presets() {
        let folder = scratch("resolve").join("BambuStudio");
        write(
            &folder.join("system/Acme/filament/fdm_filament_common.json"),
            &json!({"type": "filament", "name": "fdm_filament_common", "instantiation": "false",
                    "filament_density": ["1.24"], "nozzle_temperature": ["200"]}),
        );
        write(
            &folder.join("system/Acme/filament/Acme PLA.json"),
            &json!({"type": "filament", "name": "Acme PLA", "inherits": "fdm_filament_common",
                    "filament_type": ["PLA"], "nozzle_temperature": ["210"]}),
        );
        write(
            &folder.join("user/default/filament/My PLA.json"),
            &json!({"name": "My PLA", "inherits": "Acme PLA", "from": "User",
                    "nozzle_temperature": ["215"]}),
        );
        std::fs::write(
            folder.join("user/default/filament/My PLA.info"),
            "sync_info = \n",
        )
        .unwrap();
        write(
            &folder.join("user/default/process/Lost.json"),
            &json!({"name": "Lost", "inherits": "No such preset", "layer_height": "0.3"}),
        );
        let found = user_presets(&folder);
        let names: Vec<(PrintSet, &str)> = found.iter().map(|p| (p.set, p.name.as_str())).collect();
        assert_eq!(
            names,
            vec![(PrintSet::Process, "Lost"), (PrintSet::Filament, "My PLA")]
        );
        assert_eq!(found[1].inherits.as_deref(), Some("Acme PLA"));
        assert_eq!(found[1].account, "default");

        let resolved = resolved_preset(&folder, &found[1].path).unwrap();
        assert_eq!(resolved["name"], "My PLA");
        assert_eq!(
            resolved["nozzle_temperature"],
            json!(["215"]),
            "its own value wins"
        );
        assert_eq!(
            resolved["filament_type"],
            json!(["PLA"]),
            "from its system preset"
        );
        assert_eq!(
            resolved["filament_density"],
            json!(["1.24"]),
            "from that one's base"
        );
        assert_eq!(resolved["filament_settings_id"], json!(["My PLA"]));
        assert!(!resolved.contains_key("inherits"));

        let lost = resolved_preset(&folder, &found[0].path).unwrap();
        assert_eq!(
            lost["inherits"], "No such preset",
            "a missing base stays named"
        );
        assert_eq!(lost["type"], "process");
    }

    /// A synthetic data folder with two vendors, Acme and Able, each with a
    /// base of the same name (`fdm_machine_common`), and an app config.
    fn system_folder(name: &str, config: Option<&str>) -> PathBuf {
        let folder = scratch(name).join("QIDIStudio");
        let acme = folder.join("system/Acme");
        let able = folder.join("system/Able");
        // Vendor indexes beside the folders are no presets.
        write(
            &folder.join("system/Acme.json"),
            &json!({"name": "Acme", "machine_list": []}),
        );
        // Acme: a model with two nozzles, the 0.4 printer's variant
        // inherited from its base; a printer of another model.
        write(
            &acme.join("machine/Acme X.json"),
            &json!({"type": "machine_model", "name": "Acme X", "nozzle_diameter": "0.4;0.6",
                    "default_materials": "Acme PLA @X;Acme PETG @X"}),
        );
        write(
            &acme.join("machine/fdm_machine_common.json"),
            &json!({"type": "machine", "name": "fdm_machine_common", "instantiation": "false",
                    "printer_variant": "0.4", "printable_height": "250"}),
        );
        write(
            &acme.join("machine/Acme X 0.4 nozzle.json"),
            &json!({"type": "machine", "name": "Acme X 0.4 nozzle", "instantiation": "true",
                    "inherits": "fdm_machine_common", "printer_model": "Acme X"}),
        );
        write(
            &acme.join("machine/Acme X 0.6 nozzle.json"),
            &json!({"type": "machine", "name": "Acme X 0.6 nozzle", "instantiation": "true",
                    "inherits": "fdm_machine_common", "printer_model": "Acme X",
                    "printer_variant": "0.6"}),
        );
        write(
            &acme.join("machine/Acme Y 0.4 nozzle.json"),
            &json!({"type": "machine", "name": "Acme Y 0.4 nozzle", "instantiation": "true",
                    "inherits": "fdm_machine_common", "printer_model": "Acme Y"}),
        );
        // A template file without a type is no preset.
        write(
            &acme.join("machine/Acme X 0.4 nozzle template start_gcode.json"),
            &json!({"name": "Acme X 0.4 nozzle template start_gcode", "printer_model": "Acme X",
                    "printer_variant": "0.4"}),
        );
        // Processes: a base, one for X 0.4, one for Y only.
        write(
            &acme.join("process/fdm_process_common.json"),
            &json!({"type": "process", "name": "fdm_process_common", "instantiation": "false",
                    "layer_height": "0.2", "wall_loops": "2"}),
        );
        write(
            &acme.join("process/0.20mm Standard @Acme X.json"),
            &json!({"type": "process", "name": "0.20mm Standard @Acme X", "instantiation": "true",
                    "inherits": "fdm_process_common",
                    "compatible_printers": ["Acme X 0.4 nozzle"], "wall_loops": "3"}),
        );
        write(
            &acme.join("process/0.20mm Standard @Acme Y.json"),
            &json!({"type": "process", "name": "0.20mm Standard @Acme Y", "instantiation": "true",
                    "inherits": "fdm_process_common",
                    "compatible_printers": ["Acme Y 0.4 nozzle"]}),
        );
        // Filaments, in a subfolder too: a base, the defaults of X, one
        // listed for X and one listed only for Y.
        write(
            &acme.join("filament/base/Acme PLA @base.json"),
            &json!({"type": "filament", "name": "Acme PLA @base", "instantiation": "false",
                    "filament_type": ["PLA"]}),
        );
        for (name, printer) in [
            ("Acme PLA @X", "Acme X 0.4 nozzle"),
            ("Acme PETG @X", "Acme X 0.6 nozzle"),
            ("Acme TPU @X", "Acme X 0.4 nozzle"),
            ("Acme PLA @Y", "Acme Y 0.4 nozzle"),
        ] {
            write(
                &acme.join(format!("filament/{name}.json")),
                &json!({"type": "filament", "name": name, "instantiation": "true",
                        "inherits": "Acme PLA @base", "compatible_printers": [printer]}),
            );
        }
        // Able: a base of the same name as Acme's, found first by name
        // (its folder sorts first), and a printer of its own model.
        write(
            &able.join("machine/fdm_machine_common.json"),
            &json!({"type": "machine", "name": "fdm_machine_common", "instantiation": "false",
                    "printer_variant": "0.8", "printable_height": "999"}),
        );
        write(
            &able.join("machine/Able Z 0.8 nozzle.json"),
            &json!({"type": "machine", "name": "Able Z 0.8 nozzle", "instantiation": "true",
                    "inherits": "fdm_machine_common", "printer_model": "Able Z"}),
        );
        if let Some(config) = config {
            std::fs::write(folder.join("QIDIStudio.conf"), config).unwrap();
        }
        std::fs::create_dir_all(folder.join("user/default")).unwrap();
        folder
    }

    fn listed(found: &[FoundPreset]) -> Vec<(PrintSet, &str)> {
        found.iter().map(|p| (p.set, p.name.as_str())).collect()
    }

    #[test]
    fn the_enabled_system_presets_are_listed_and_the_bases_never() {
        // Acme X with both nozzles enabled; TPU listed, which suits the
        // 0.4 printer but not the 0.6 one, so X's default materials come
        // in too (all of them, as the slicer adds them); Y's PLA listed
        // but Y not enabled.
        let config = r#"{
            "models": [{"vendor": "Acme", "model": "Acme X", "nozzle_diameter": "0.4;0.6"}],
            "filaments": ["Acme TPU @X", "Acme PLA @Y", "No such filament"],
            "presets": {"machine": "Acme X 0.4 nozzle"}
        }
# MD5 checksum 0123456789ABCDEF
"#;
        let folder = system_folder("system", Some(config));
        let found = system_presets(&folder, Dialect::QidiStudio);
        assert_eq!(
            listed(&found),
            vec![
                (PrintSet::Printer, "Acme X 0.4 nozzle"),
                (PrintSet::Printer, "Acme X 0.6 nozzle"),
                (PrintSet::Process, "0.20mm Standard @Acme X"),
                (PrintSet::Filament, "Acme PETG @X"),
                (PrintSet::Filament, "Acme PLA @X"),
                (PrintSet::Filament, "Acme TPU @X"),
            ]
        );
        assert!(found.iter().all(|p| p.vendor.as_deref() == Some("Acme")));
        assert_eq!(found[2].inherits.as_deref(), Some("fdm_process_common"));
        // The user's presets are not among them, nor they among the user's.
        assert!(user_presets(&folder).is_empty());

        // A system process resolved: its own value over its base's.
        let process = resolved_preset(&folder, &found[2].path).unwrap();
        assert_eq!(process["name"], "0.20mm Standard @Acme X");
        assert_eq!(process["type"], "process");
        assert_eq!(process["print_settings_id"], "0.20mm Standard @Acme X");
        assert_eq!(process["wall_loops"], "3");
        assert_eq!(process["layer_height"], "0.2");
        assert!(!process.contains_key("inherits"));
        // A printer's base from its own vendor, though Able has one of the
        // same name (and Acme X 0.4 is listed only with Acme's variant).
        let printer = resolved_preset(&folder, &found[0].path).unwrap();
        assert_eq!(printer["printable_height"], "250");
        assert_eq!(printer["type"], "machine");

        // Without an app config nothing of the system's.
        let bare = system_folder("system-bare", None);
        assert!(system_presets(&bare, Dialect::QidiStudio).is_empty());
        // A model of another vendor's name is not enabled: Able Z needs
        // Able's model.
        let able = system_folder(
            "system-able",
            Some(
                r#"{"models": [{"vendor": "Acme", "model": "Able Z", "nozzle_diameter": "0.8"}]}"#,
            ),
        );
        assert!(system_presets(&able, Dialect::QidiStudio).is_empty());
        // Filaments as an object of names (the older form), Y enabled.
        let older = system_folder(
            "system-object",
            Some(
                r#"{"models": [{"vendor": "Acme", "model": "Acme Y", "nozzle_diameter": "0.4"}],
                    "filaments": {"Acme PLA @Y": "true", "Acme TPU @X": ""}}"#,
            ),
        );
        assert_eq!(
            listed(&system_presets(&older, Dialect::QidiStudio)),
            vec![
                (PrintSet::Printer, "Acme Y 0.4 nozzle"),
                (PrintSet::Process, "0.20mm Standard @Acme Y"),
                (PrintSet::Filament, "Acme PLA @Y"),
            ]
        );
    }

    #[test]
    fn a_slicer_with_enabled_system_presets_and_no_user_folder_is_installed() {
        // QIDI Studio set up for Acme Y but no preset of the user's yet: no
        // `user/`, still offered (mitcad#871 review). Y gets a second
        // process whose name sorts first in any case, last byte-wise.
        let config =
            r#"{"models": [{"vendor": "Acme", "model": "Acme Y", "nozzle_diameter": "0.4"}]}"#;
        let folder = system_folder("no-user", Some(config));
        std::fs::remove_dir_all(folder.join("user")).unwrap();
        write(
            &folder.join("system/Acme/process/0.20mm fine @Acme Y.json"),
            &json!({"type": "process", "name": "0.20mm fine @Acme Y", "instantiation": "true",
                    "inherits": "fdm_process_common",
                    "compatible_printers": ["Acme Y 0.4 nozzle"]}),
        );
        let mut linux = env(Platform::Linux, "/no-such-home");
        linux.config_home = folder.parent().map(Path::to_path_buf);
        assert_eq!(
            slicer_folders(&linux),
            vec![SlicerFolder {
                dialect: Dialect::QidiStudio,
                folder: folder.clone(),
                flatpak: false,
            }]
        );
        assert_eq!(
            listed(&system_presets(&folder, Dialect::QidiStudio)),
            vec![
                (PrintSet::Printer, "Acme Y 0.4 nozzle"),
                (PrintSet::Process, "0.20mm fine @Acme Y"),
                (PrintSet::Process, "0.20mm Standard @Acme Y"),
            ]
        );
        assert!(user_presets(&folder).is_empty());

        // With no printer enabled there is nothing to import: not offered.
        std::fs::write(folder.join("QIDIStudio.conf"), r#"{"models": []}"#).unwrap();
        assert!(slicer_folders(&linux).is_empty());
    }

    /// mitcad#1053: the chain a preset's values come from, several levels
    /// up, a missing base, and a preset file outside any data folder
    /// resolved through the installed slicers' system presets.
    #[test]
    fn the_chain_is_named_and_an_outside_preset_resolves_through_installed_slicers() {
        let root = scratch("chain");
        let folder = root.join("OrcaSlicer");
        write(
            &folder.join("system/Acme/filament/fdm_filament_common.json"),
            &json!({"type": "filament", "name": "fdm_filament_common", "instantiation": "false",
                    "filament_density": ["1.24"], "nozzle_temperature": ["200"]}),
        );
        write(
            &folder.join("system/Acme/filament/Acme PLA @base.json"),
            &json!({"type": "filament", "name": "Acme PLA @base", "instantiation": "false",
                    "inherits": "fdm_filament_common", "filament_colour": ["#FF8000"]}),
        );
        write(
            &folder.join("system/Acme/filament/Acme PLA.json"),
            &json!({"type": "filament", "name": "Acme PLA", "inherits": "Acme PLA @base",
                    "nozzle_temperature": ["220"]}),
        );
        let mine = folder.join("user/default/filament/My PLA.json");
        write(
            &mine,
            &json!({"name": "My PLA", "inherits": "Acme PLA", "from": "User",
                    "nozzle_temperature_initial_layer": ["215"]}),
        );
        let lost = folder.join("user/default/filament/Lost.json");
        write(
            &lost,
            &json!({"name": "Lost", "inherits": "Gone PLA", "nozzle_temperature": ["230"]}),
        );

        let resolved = resolve_preset(&folder, &mine).unwrap();
        assert_eq!(
            resolved.chain,
            vec!["Acme PLA", "Acme PLA @base", "fdm_filament_common"]
        );
        assert_eq!(resolved.missing, None);
        assert_eq!(
            resolved.chain_text(),
            "Acme PLA > Acme PLA @base > fdm_filament_common"
        );
        assert_eq!(resolved.map["filament_colour"], json!(["#FF8000"]));
        assert_eq!(resolved.map["nozzle_temperature"], json!(["220"]));
        assert_eq!(resolved.map["filament_density"], json!(["1.24"]));
        let gone = resolve_preset(&folder, &lost).unwrap();
        assert!(gone.chain.is_empty());
        assert_eq!(gone.missing.as_deref(), Some("Gone PLA"));
        assert_eq!(gone.chain_text(), "Gone PLA (missing)");
        assert_eq!(chain_text(&[], None), "nothing");

        // In a data folder: found from the file's place.
        assert_eq!(data_folder_of(&mine), Some(folder.clone()));
        let found = resolve_preset_file(&mine, &[], None).unwrap().unwrap();
        assert_eq!(found.chain.len(), 3);

        // A copy elsewhere: through the folders given, the dialect's first,
        // the first that has its base.
        let outside = root.join("Downloads").join("My PLA.json");
        write(
            &outside,
            &json!({"name": "My PLA", "inherits": "Acme PLA",
                    "nozzle_temperature_initial_layer": ["215"]}),
        );
        assert_eq!(data_folder_of(&outside), None);
        let empty = root.join("BambuStudio");
        std::fs::create_dir_all(empty.join("system")).unwrap();
        let folders = [
            SlicerFolder {
                dialect: Dialect::OrcaSlicer,
                folder: folder.clone(),
                flatpak: false,
            },
            SlicerFolder {
                dialect: Dialect::BambuStudio,
                folder: empty,
                flatpak: false,
            },
        ];
        let found = resolve_preset_file(&outside, &folders, Some(Dialect::BambuStudio))
            .unwrap()
            .unwrap();
        assert_eq!(
            found.chain,
            vec!["Acme PLA", "Acme PLA @base", "fdm_filament_common"]
        );
        assert_eq!(found.map["filament_colour"], json!(["#FF8000"]));
        assert_eq!(
            found.map["nozzle_temperature_initial_layer"],
            json!(["215"])
        );
        assert!(!found.map.contains_key("inherits"));
        // No folder has the base, or nothing to inherit: read as it is.
        assert_eq!(
            resolve_preset_file(&outside, &folders[1..], None).unwrap(),
            None
        );
        let plain = root.join("Downloads").join("Plain.json");
        write(
            &plain,
            &json!({"name": "Plain", "nozzle_temperature": ["200"]}),
        );
        assert_eq!(resolve_preset_file(&plain, &folders, None).unwrap(), None);
    }

    /// mitcad#2207: a printer's extruders as the slicer loads its preset,
    /// by the preset's name (a system one through its base and templates,
    /// a user one through the system preset it inherits from), else by its
    /// printer model; never from a printer model's own file, a base, or
    /// another slicer's folder.
    #[test]
    fn a_printers_extruders_are_those_of_its_preset() {
        let folder = scratch("extruders").join("BambuStudio");
        let machine = folder.join("system/Acme/machine");
        write(
            &machine.join("fdm_machine_common.json"),
            &json!({"type": "machine", "name": "fdm_machine_common", "instantiation": "false",
                    "nozzle_diameter": ["0.4"]}),
        );
        write(
            &machine.join("duo_template.json"),
            &json!({"type": "machine", "name": "Acme Duo template", "instantiation": "false",
                    "nozzle_diameter": ["0.4", "0.4"]}),
        );
        write(
            &machine.join("Acme Duo 0.4 nozzle.json"),
            &json!({"type": "machine", "name": "Acme Duo 0.4 nozzle",
                    "inherits": "fdm_machine_common", "include": ["Acme Duo template"],
                    "printer_model": "Acme Duo"}),
        );
        write(
            &machine.join("Acme Solo 0.4 nozzle.json"),
            &json!({"type": "machine", "name": "Acme Solo 0.4 nozzle",
                    "inherits": "fdm_machine_common", "printer_model": "Acme Solo"}),
        );
        // The printer model's own file, named as the model.
        write(
            &machine.join("Acme Duo.json"),
            &json!({"type": "machine_model", "name": "Acme Duo",
                    "nozzle_diameter": "0.4;0.6;0.8"}),
        );
        write(
            &folder.join("user/default/machine/My Duo.json"),
            &json!({"name": "My Duo", "inherits": "Acme Duo 0.4 nozzle", "from": "User"}),
        );
        let folders = vec![SlicerFolder {
            dialect: Dialect::BambuStudio,
            folder: folder.clone(),
            flatpak: false,
        }];
        let bambu = Dialect::BambuStudio;
        assert_eq!(
            printer_extruders(&folders, bambu, &["Acme Duo 0.4 nozzle"], None),
            Some(2)
        );
        assert_eq!(
            printer_extruders(&folders, bambu, &["Acme Solo 0.4 nozzle"], None),
            Some(1)
        );
        assert_eq!(
            printer_extruders(&folders, bambu, &["My Duo"], None),
            Some(2)
        );
        // A name not found: the first name that is a preset, else the
        // model's presets.
        assert_eq!(
            printer_extruders(&folders, bambu, &["No such", "Acme Duo 0.4 nozzle"], None),
            Some(2)
        );
        assert_eq!(
            printer_extruders(&folders, bambu, &["No such"], Some("Acme Duo")),
            Some(2)
        );
        // Not the printer model's own file, nor a base.
        assert_eq!(
            printer_extruders(&folders, bambu, &["Acme Duo"], None),
            None
        );
        assert_eq!(
            printer_extruders(&folders, bambu, &[], Some("No model")),
            None
        );
        // Only the slicer's own folders.
        assert_eq!(
            printer_extruders(
                &folders,
                Dialect::OrcaSlicer,
                &["Acme Duo 0.4 nozzle"],
                None
            ),
            None
        );
    }
}
