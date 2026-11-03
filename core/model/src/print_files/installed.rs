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

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

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
/// nearer preset's value winning, for every key. The result is named as
/// the preset and says its kind (`type` and the slicer's id key of its
/// kind), and has no `inherits` when the whole chain was found; a base
/// that is missing leaves `inherits` naming it, so that the import
/// reports the values not read.
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
    mut vendor: Option<PathBuf>,
) -> Result<ResolvedPreset, String> {
    let mut chain = vec![own];
    let mut names = Vec::new();
    let mut bases: Option<Bases> = None;
    let mut missing = None;
    while let Some(base) = chain.last().and_then(|m| text_of(m, "inherits")) {
        if chain.len() > MAX_INHERITS {
            return Err(format!(
                "{name}: the presets it inherits from inherit in a loop"
            ));
        }
        let index = bases.get_or_insert_with(|| Bases::of(folder, account));
        let found = index.find(&base, vendor.as_deref()).map(Path::to_path_buf);
        match found.as_deref().and_then(read_preset_file) {
            Some(map) => {
                // A system preset's bases in its own vendor's folder first.
                if let Some(dir) = found.as_deref().and_then(|p| vendor_dir(folder, p)) {
                    vendor = Some(dir);
                }
                names.push(base);
                chain.push(map);
            }
            None => {
                missing = Some(base);
                break;
            }
        }
    }
    // The farthest first, each nearer one's values over it; but a nearer
    // preset's empty `compatible_printers` names no printer, so that the
    // printers of the preset it inherits from stay its own (mitcad#1216:
    // the printers a process or filament is for).
    let mut merged = Map::new();
    for map in chain.into_iter().rev() {
        for (key, value) in map {
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

/// A preset file with what it inherits filled in (mitcad#1053): one in a
/// slicer's data folder through that folder ([`resolve_preset`]), another
/// (a preset exported or copied elsewhere) through the system presets of
/// the data folders `folders` (the installed slicers'), those of
/// `dialect` first, the first that has the preset's base. None when the
/// file inherits nothing, or no folder has its base: then it is read as
/// it is and the import reports the values it inherits as not read.
pub fn resolve_preset_file(
    path: &Path,
    folders: &[SlicerFolder],
    dialect: Option<Dialect>,
) -> Result<Option<ResolvedPreset>, String> {
    let shown = path.display();
    let own = read_preset_file(path).ok_or_else(|| format!("{shown} is no preset file"))?;
    if text_of(&own, "inherits").is_none() {
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
        if !resolved.chain.is_empty() {
            return Ok(Some(resolved));
        }
    }
    Ok(None)
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
}
