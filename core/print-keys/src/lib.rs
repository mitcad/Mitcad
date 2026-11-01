// SPDX-License-Identifier: MIT
//! The print settings' key map (mitcad#640, part 1): Mitcad's keys of the
//! three layers of print settings (the printer, the process, the
//! filament), each with its type, unit, choices, a plain name for the
//! interface and, per slicer dialect, the slicer's key and how a value
//! converts. Export writes a dialect's keys from Mitcad's values, import
//! reads them back; `print_settings` and Property Sets show Mitcad's keys.
//!
//! # Dialects
//!
//! Five slicers, two file families: the Bambu Studio family (Bambu Studio,
//! OrcaSlicer, QIDI Studio: one JSON project settings file, per-filament
//! values as lists), PrusaSlicer (INI, per-extruder values as lists) and
//! UltiMaker Cura (stacks of INI files, per-extruder values in each
//! extruder's stack). [`Dialect`] names them.
//!
//! # Keys
//!
//! A Mitcad key is PrusaSlicer's key where PrusaSlicer has the setting
//! (`layer_height`, `perimeters`, `fill_density`), else the Bambu Studio
//! key (else OrcaSlicer's); a key that carries a vendor's name gets a
//! neutral one (the plate temperature `supertack_plate_temp` is
//! `adhesive_cool_plate_temp`). Keys are matched by meaning, not by name:
//! `perimeters` is `wall_loops` in the Bambu family and `wall_line_count`
//! in Cura; a name that matches but means something else is not mapped
//! (the Bambu family's `support_angle` turns the support pattern, Cura's
//! is the overhang threshold, which is `support_material_threshold`).
//!
//! Every setting of the three layers that has a counterpart in at least
//! two dialects is mapped; the printer layer carries only what Mitcad uses
//! (bed shape and excluded areas, printable height, nozzle diameter,
//! filament slots, the printer model and the preset name). Left out on
//! purpose, and kept as dialect text by the import: preset metadata
//! (`inherits`, compatibility conditions, notes), G-code templates and
//! post-processing scripts, per-nozzle-variant bookkeeping and device-side
//! values (drying settings of a filament feeder). A key whose value names
//! a dialect's own preset (`printer_settings_id`, `printer_model`) is
//! [`KeyDef::per_dialect`]: a profile keeps one value per dialect.
//!
//! A few keys are Mitcad's own ([`MITCAD_OWN`]), with no dialect key: a
//! material's purge volumes on filament changes (mitcad#705), which no
//! slicer keeps per filament. Export turns them into each slicer's purge
//! matrix (the model's `print_purge`).
//!
//! # Categories
//!
//! Every key has a place in the settings form (mitcad#655): a
//! [`Category`] of its set (a tab: the process' Quality, Strength, Speed,
//! Support and Others) and a [`Group`] in it (a headed section: Layer
//! height, Line width, Seam, ...), and is `advanced` or common: advanced
//! keys show only with Advanced on, or when an item sets them. The
//! enums' order is the display order, and the table's rows are in that
//! order too (common keys first in each group), so that it reads as the
//! form's outline. The structure follows the Bambu Studio family's
//! settings tabs (`src/slic3r/GUI/Tab.cpp`, read as a structure only),
//! adapted to Mitcad's keys; the titles are Mitcad's own. Choices have
//! plain names beside their values (`inner_outer`, "Inner, then outer").
//!
//! # Values and conversions
//!
//! Values ([`Value`]) are numbers in the key's unit (mm, mm/s, °C, ...),
//! percentages as numbers (15 is 15 %), integers, yes/no, text, choices
//! by Mitcad's names and points (a bed outline). A dialect's raw text
//! converts by the key's [`Form`]: the Bambu family and PrusaSlicer write a
//! percentage as `15%` and yes/no as `1`/`0`, Cura `15` and
//! `True`/`False`; a ratio may be a percentage in another dialect (a flow
//! of 0.95 is Cura's 95), a choice has other names (rectilinear infill is
//! `zig-zag` in Bambu Studio, `rectilinear` in OrcaSlicer and PrusaSlicer,
//! `lines` in Cura), and a length or speed given as a percentage of
//! another value has its own basis per dialect: PrusaSlicer's line widths
//! are a percentage of the layer height, the Bambu family's and Mitcad's
//! of the nozzle diameter, and Cura takes absolute values only. A
//! percentage is kept as one while the basis is the same; otherwise it
//! is resolved through the other values (a context of Mitcad values by
//! key) and written as an absolute value.
//!
//! A dialect key may carry the range of numbers that dialect takes
//! ([`DialectKey::range`]) where it is narrower than another's: Bambu
//! Studio's -1 for "automatic" (`raft_first_layer_expansion`,
//! `tree_support_wall_count`, `prime_tower_brim_width`) is out of
//! OrcaSlicer's or PrusaSlicer's range, and OrcaSlicer refuses a project
//! that has it. [`write`] refuses such a value, so that export leaves it out
//! of that dialect's file with a warning (the slicer keeps its default).
//!
//! Every numeric key has the range its slicers take ([`KeyDef::limits`],
//! mitcad#746): the widest of their definitions, so that the editors
//! refuse only what no slicer of the key takes (a fill density of 150 %,
//! a negative layer height) and keep what one does (a purge line spacing
//! of 150 %, Bambu Studio's -1); a dialect's narrower range is
//! [`KeyDef::dialect_limits`]. The model itself takes any finite number
//! ([`KeyDef::check`]): the key ranges are the editors' (the `print_keys`
//! query gives them), the dialect ranges also export's.
//!
//! # Sources
//!
//! Built from the slicers' option definitions and preset key lists, read
//! as interface facts (key names, types, units, enum values and what a
//! percentage is of); the names, the grouping and this table are Mitcad's
//! own, and no descriptions, labels or preset values were copied:
//!
//! - PrusaSlicer, tag `version_2.9.6`: `src/libslic3r/PrintConfig.cpp`,
//!   `src/libslic3r/Preset.cpp` (AGPL-3.0).
//! - Bambu Studio, commit `79df7ae3`: `src/libslic3r/PrintConfig.cpp`,
//!   `src/libslic3r/Preset.cpp`, `src/libslic3r/PresetBundle.cpp`,
//!   `src/libslic3r/Format/bbs_3mf.cpp`, `src/slic3r/GUI/Plater.cpp`
//!   (AGPL-3.0).
//! - OrcaSlicer, commit `0a00730d`: the same files (AGPL-3.0).
//! - QIDI Studio, commit `403b619d`: the same files, the 3MF code in
//!   `src/libslic3r/Format/qds_3mf.cpp` (AGPL-3.0).
//! - UltiMaker Cura, commit `5bea2f86`:
//!   `resources/definitions/fdmprinter.def.json` (LGPL-3.0).
//!
//! # What the Bambu family's loader needs (for export, part 5)
//!
//! From the 3MF loaders of the sources above (`bbs_3mf.cpp`,
//! `qds_3mf.cpp`, `Plater.cpp`, `PresetBundle.cpp`):
//!
//! - **Application metadata.** The model part's
//!   `<metadata name="Application">` decides whether the project settings
//!   are read at all. Bambu Studio reads `Metadata/project_settings.config`
//!   only when it is `BambuStudio-<version>` with a version it can parse
//!   (otherwise it loads geometry only); QIDI Studio when it is
//!   `QIDIStudio-<version>` or `BambuStudio-<version>` (its parser drops
//!   the first character of a `QIDIStudio-` version, so a two-digit major
//!   `02.04.00.00` keeps working); OrcaSlicer reads the settings of any 3MF
//!   but treats a file as its own family's only with `BambuStudio-` or
//!   `OrcaSlicer-` there (or its own version tag). A file version whose
//!   major is newer than the slicer's loads geometry only, a newer minor
//!   brings a warning, one before 1.5.9 is translated as an old project:
//!   write a modest version (e.g. `BambuStudio-02.00.00.00`) and
//!   `<metadata name="BambuStudio:3mfVersion">1</metadata>` beside it.
//! - **The settings file.** `Metadata/project_settings.config`, JSON, all
//!   values as strings, per-filament values as lists of one entry per
//!   filament. It must have a non-empty `filament_colour` (its length is
//!   the number of filaments; without it the load fails), and
//!   per-filament and per-extruder lists of matching lengths (a printer
//!   with several extruders also needs `extruder_type` with one entry per
//!   nozzle, else the settings are dropped unless the printer model is
//!   the vendor's own).
//! - **A partial printer block works when it names an installed preset.**
//!   The file's keys are first laid over the program's built-in defaults
//!   (`FullPrintConfig::defaults()`). Then each of process, filaments and
//!   printer is loaded as a preset: `inherits_group` (one entry each:
//!   process, the filaments, then the printer) names the system preset it
//!   derives from, and `different_settings_to_system` (the same entries,
//!   each the keys that differ, `;`-separated) says which of the file's
//!   values count. When the inherited preset is installed, every key not
//!   listed takes that preset's value (`update_non_diff_values_to_base_config`
//!   in `PresetCollection::load_external_preset`, the same in all three);
//!   a result equal to an installed preset selects that preset, one that
//!   differs becomes a project preset of the file, named by
//!   `printer_settings_id`, `print_settings_id` and `filament_settings_id`.
//!   Without an installed preset of that name the missing keys keep the
//!   built-in defaults, which are no printer's. So an export names the
//!   slicer's printer preset (the profile's preset name for the dialect)
//!   in `printer_settings_id` and as the printer's `inherits_group` entry,
//!   lists the keys it sets in `different_settings_to_system`, and writes
//!   only those; the import keeps a file's raw printer block for the
//!   same dialect (hidden, not editable) and writes it back whole, for a
//!   printer whose preset the receiving slicer may not have. The machine's
//!   own settings stay the slicer's.
//! - **Other files.** None are required for the settings:
//!   `Metadata/model_settings.config` (objects, parts, their per-object
//!   overrides and filament per part) and `Metadata/slice_info.config`
//!   are read when present; plates, thumbnails and the embedded preset
//!   files are optional.

use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

use serde_json::json;

// The table's shorthand: a dialect key, and a Mitcad key with its dialect
// keys. Defined before `mod table`, which uses them. A trailing
// `range(min, max)` is the dialect's allowed range ([`DialectKey::range`]);
// its arms come before the `$form:expr` ones, which would take it as a form.
macro_rules! d {
    ($dialects:expr, $key:literal) => {
        DialectKey {
            dialects: $dialects,
            key: $key,
            list: false,
            form: Form::Same,
            range: None,
        }
    };
    ($dialects:expr, $key:literal, range($min:expr, $max:expr)) => {
        DialectKey {
            dialects: $dialects,
            key: $key,
            list: false,
            form: Form::Same,
            range: Some(($min, $max)),
        }
    };
    ($dialects:expr, $key:literal, list) => {
        DialectKey {
            dialects: $dialects,
            key: $key,
            list: true,
            form: Form::Same,
            range: None,
        }
    };
    ($dialects:expr, $key:literal, list, range($min:expr, $max:expr)) => {
        DialectKey {
            dialects: $dialects,
            key: $key,
            list: true,
            form: Form::Same,
            range: Some(($min, $max)),
        }
    };
    ($dialects:expr, $key:literal, list, $form:expr, range($min:expr, $max:expr)) => {
        DialectKey {
            dialects: $dialects,
            key: $key,
            list: true,
            form: $form,
            range: Some(($min, $max)),
        }
    };
    ($dialects:expr, $key:literal, list, $form:expr) => {
        DialectKey {
            dialects: $dialects,
            key: $key,
            list: true,
            form: $form,
            range: None,
        }
    };
    ($dialects:expr, $key:literal, $form:expr) => {
        DialectKey {
            dialects: $dialects,
            key: $key,
            list: false,
            form: $form,
            range: None,
        }
    };
}

// A key's level: `common` keys always show, `advanced` ones only with
// Advanced on or when an item sets them (mitcad#655).
macro_rules! level {
    (common) => {
        false
    };
    (advanced) => {
        true
    };
}

macro_rules! key {
    ($set:ident, $key:literal, $kind:expr, $name:literal, $group:ident, $level:ident, per_dialect, [$($d:expr),* $(,)?] $(,)?) => {
        KeyDef {
            key: $key,
            set: PrintSet::$set,
            kind: $kind,
            name: $name,
            group: Group::$group,
            advanced: level!($level),
            per_dialect: true,
            dialects: &[$($d),*],
        }
    };
    ($set:ident, $key:literal, $kind:expr, $name:literal, $group:ident, $level:ident, [$($d:expr),* $(,)?] $(,)?) => {
        KeyDef {
            key: $key,
            set: PrintSet::$set,
            kind: $kind,
            name: $name,
            group: Group::$group,
            advanced: level!($level),
            per_dialect: false,
            dialects: &[$($d),*],
        }
    };
}

mod limits;
mod table;
#[cfg(test)]
mod tests;

/// The dialects of a [`DialectKey`] as bits.
const B: u8 = 1;
const O: u8 = 2;
const Q: u8 = 4;
const P: u8 = 8;
const C: u8 = 16;

/// The three layers of print settings, each a property set (mitcad#496).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrintSet {
    Printer,
    Process,
    Filament,
}

impl PrintSet {
    pub const ALL: [PrintSet; 3] = [PrintSet::Printer, PrintSet::Process, PrintSet::Filament];

    /// The property set that holds the layer's values.
    pub fn property_set(self) -> &'static str {
        match self {
            PrintSet::Printer => "Print Printer",
            PrintSet::Process => "Print Process",
            PrintSet::Filament => "Print Filament",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            PrintSet::Printer => "printer",
            PrintSet::Process => "process",
            PrintSet::Filament => "filament",
        }
    }
}

/// A category of a set's keys (mitcad#655): a tab of the settings form.
/// The variants' order is the display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    // The printer's: one tab, Printer (mitcad#655), the nozzle with the
    // printable space.
    BasicInformation,
    // The process'.
    Quality,
    Strength,
    Speed,
    Support,
    Others,
    // The filament's (the material editor's 3D printing section).
    Filament,
    Cooling,
    SettingOverrides,
    Advanced,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::BasicInformation,
        Category::Quality,
        Category::Strength,
        Category::Speed,
        Category::Support,
        Category::Others,
        Category::Filament,
        Category::Cooling,
        Category::SettingOverrides,
        Category::Advanced,
    ];

    /// The set whose keys it holds.
    pub fn set(self) -> PrintSet {
        match self {
            Category::BasicInformation => PrintSet::Printer,
            Category::Quality
            | Category::Strength
            | Category::Speed
            | Category::Support
            | Category::Others => PrintSet::Process,
            Category::Filament
            | Category::Cooling
            | Category::SettingOverrides
            | Category::Advanced => PrintSet::Filament,
        }
    }

    /// The id in queries.
    pub fn id(self) -> &'static str {
        match self {
            Category::BasicInformation => "basic_information",
            Category::Quality => "quality",
            Category::Strength => "strength",
            Category::Speed => "speed",
            Category::Support => "support",
            Category::Others => "others",
            Category::Filament => "filament",
            Category::Cooling => "cooling",
            Category::SettingOverrides => "setting_overrides",
            Category::Advanced => "advanced",
        }
    }

    /// The tab's title.
    pub fn title(self) -> &'static str {
        match self {
            Category::BasicInformation => "Printer",
            Category::Quality => "Quality",
            Category::Strength => "Strength",
            Category::Speed => "Speed",
            Category::Support => "Support",
            Category::Others => "Others",
            Category::Filament => "Filament",
            Category::Cooling => "Cooling",
            // Named after what it holds (mitcad#655), not "Setting overrides".
            Category::SettingOverrides => "Retraction",
            Category::Advanced => "Advanced",
        }
    }

    /// The categories of a set, in display order.
    pub fn of_set(set: PrintSet) -> impl Iterator<Item = Category> {
        Category::ALL.into_iter().filter(move |c| c.set() == set)
    }

    /// Its groups, in display order.
    pub fn groups(self) -> impl Iterator<Item = Group> {
        Group::ALL.into_iter().filter(move |g| g.category() == self)
    }
}

/// A sub-group of a category (mitcad#655): a headed section of a tab. The
/// variants' order is the display order; the keys of a group are in the
/// table's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    PrintableSpace,
    PrinterPresets,
    Nozzle,
    LayerHeight,
    LineWidth,
    Seam,
    Precision,
    Ironing,
    WallGenerator,
    OrderAndFlow,
    BridgesAndOverhangs,
    Travel,
    Walls,
    TopAndBottomShells,
    SparseInfill,
    InfillDetails,
    FirstLayer,
    OtherLayers,
    Overhangs,
    SlowDownByHeight,
    TravelSpeed,
    Acceleration,
    Jerk,
    Smoothing,
    Support,
    Raft,
    ContactAndInterface,
    SupportBody,
    TreeSupports,
    SupportFilament,
    SupportIroning,
    BedAdhesion,
    PrimeTower,
    Filaments,
    Interlocking,
    SpecialModes,
    FuzzySkin,
    GcodeOutput,
    ProcessPresets,
    FilamentBasics,
    Flow,
    NozzleTemperature,
    BedTemperature,
    Chamber,
    VolumetricSpeed,
    ScarfSeam,
    // The material's purge volumes on filament changes (mitcad#705).
    Purge,
    FilamentPresets,
    PartCoolingFan,
    SlowDownForCooling,
    OverhangsAndBridges,
    AuxiliaryFan,
    ExhaustFan,
    Retraction,
    ToolchangeRetraction,
    OverrideSpeed,
    FilamentChange,
    FilamentPrimeTower,
    LoadingAndRamming,
    CircleCompensation,
}

impl Group {
    pub const ALL: [Group; 60] = [
        Group::PrintableSpace,
        Group::PrinterPresets,
        Group::Nozzle,
        Group::LayerHeight,
        Group::LineWidth,
        Group::Seam,
        Group::Precision,
        Group::Ironing,
        Group::WallGenerator,
        Group::OrderAndFlow,
        Group::BridgesAndOverhangs,
        Group::Travel,
        Group::Walls,
        Group::TopAndBottomShells,
        Group::SparseInfill,
        Group::InfillDetails,
        Group::FirstLayer,
        Group::OtherLayers,
        Group::Overhangs,
        Group::SlowDownByHeight,
        Group::TravelSpeed,
        Group::Acceleration,
        Group::Jerk,
        Group::Smoothing,
        Group::Support,
        Group::Raft,
        Group::ContactAndInterface,
        Group::SupportBody,
        Group::TreeSupports,
        Group::SupportFilament,
        Group::SupportIroning,
        Group::BedAdhesion,
        Group::PrimeTower,
        Group::Filaments,
        Group::Interlocking,
        Group::SpecialModes,
        Group::FuzzySkin,
        Group::GcodeOutput,
        Group::ProcessPresets,
        Group::FilamentBasics,
        Group::Flow,
        Group::NozzleTemperature,
        Group::BedTemperature,
        Group::Chamber,
        Group::VolumetricSpeed,
        Group::ScarfSeam,
        Group::Purge,
        Group::FilamentPresets,
        Group::PartCoolingFan,
        Group::SlowDownForCooling,
        Group::OverhangsAndBridges,
        Group::AuxiliaryFan,
        Group::ExhaustFan,
        Group::Retraction,
        Group::ToolchangeRetraction,
        Group::OverrideSpeed,
        Group::FilamentChange,
        Group::FilamentPrimeTower,
        Group::LoadingAndRamming,
        Group::CircleCompensation,
    ];

    /// Its category, id and title.
    fn info(self) -> (Category, &'static str, &'static str) {
        match self {
            Group::PrintableSpace => (
                Category::BasicInformation,
                "printable_space",
                "Printable space",
            ),
            Group::PrinterPresets => (
                Category::BasicInformation,
                "printer_presets",
                "Slicer presets",
            ),
            Group::Nozzle => (Category::BasicInformation, "nozzle", "Nozzle"),
            Group::LayerHeight => (Category::Quality, "layer_height", "Layer height"),
            Group::LineWidth => (Category::Quality, "line_width", "Line width"),
            Group::Seam => (Category::Quality, "seam", "Seam"),
            Group::Precision => (Category::Quality, "precision", "Precision"),
            Group::Ironing => (Category::Quality, "ironing", "Ironing"),
            Group::WallGenerator => (Category::Quality, "wall_generator", "Wall generator"),
            Group::OrderAndFlow => (Category::Quality, "order_and_flow", "Order and flow"),
            Group::BridgesAndOverhangs => (
                Category::Quality,
                "bridges_and_overhangs",
                "Bridges and overhangs",
            ),
            Group::Travel => (Category::Quality, "travel", "Travel"),
            Group::Walls => (Category::Strength, "walls", "Walls"),
            Group::TopAndBottomShells => (
                Category::Strength,
                "top_and_bottom_shells",
                "Top and bottom shells",
            ),
            Group::SparseInfill => (Category::Strength, "sparse_infill", "Sparse infill"),
            Group::InfillDetails => (Category::Strength, "infill_details", "Infill details"),
            Group::FirstLayer => (Category::Speed, "first_layer", "First layer"),
            Group::OtherLayers => (Category::Speed, "other_layers", "Other layers"),
            Group::Overhangs => (Category::Speed, "overhangs", "Overhangs"),
            Group::SlowDownByHeight => (
                Category::Speed,
                "slow_down_by_height",
                "Slow down by height",
            ),
            Group::TravelSpeed => (Category::Speed, "travel_speed", "Travel"),
            Group::Acceleration => (Category::Speed, "acceleration", "Acceleration"),
            Group::Jerk => (Category::Speed, "jerk", "Jerk"),
            Group::Smoothing => (Category::Speed, "smoothing", "Smoothing"),
            Group::Support => (Category::Support, "support", "Support"),
            Group::Raft => (Category::Support, "raft", "Raft"),
            Group::ContactAndInterface => (
                Category::Support,
                "contact_and_interface",
                "Contact and interface",
            ),
            Group::SupportBody => (Category::Support, "support_body", "Support body"),
            Group::TreeSupports => (Category::Support, "tree_supports", "Tree supports"),
            Group::SupportFilament => (Category::Support, "support_filament", "Support filament"),
            Group::SupportIroning => (Category::Support, "support_ironing", "Support ironing"),
            Group::BedAdhesion => (Category::Others, "bed_adhesion", "Bed adhesion"),
            Group::PrimeTower => (Category::Others, "prime_tower", "Prime tower"),
            Group::Filaments => (Category::Others, "filaments", "Filaments"),
            Group::Interlocking => (Category::Others, "interlocking", "Interlocking"),
            Group::SpecialModes => (Category::Others, "special_modes", "Special modes"),
            Group::FuzzySkin => (Category::Others, "fuzzy_skin", "Fuzzy skin"),
            Group::GcodeOutput => (Category::Others, "gcode_output", "G-code output"),
            Group::ProcessPresets => (Category::Others, "process_presets", "Slicer presets"),
            Group::FilamentBasics => (Category::Filament, "filament_basics", "Basic information"),
            Group::Flow => (Category::Filament, "flow", "Flow"),
            Group::NozzleTemperature => (
                Category::Filament,
                "nozzle_temperature",
                "Nozzle temperature",
            ),
            Group::BedTemperature => (Category::Filament, "bed_temperature", "Bed temperature"),
            Group::Chamber => (Category::Filament, "chamber", "Chamber"),
            Group::VolumetricSpeed => (Category::Filament, "volumetric_speed", "Volumetric speed"),
            Group::ScarfSeam => (Category::Filament, "scarf_seam", "Scarf seam"),
            Group::Purge => (Category::Filament, "purge", "Purge volumes"),
            Group::FilamentPresets => (Category::Filament, "filament_presets", "Slicer presets"),
            Group::PartCoolingFan => (Category::Cooling, "part_cooling_fan", "Part cooling fan"),
            Group::SlowDownForCooling => (
                Category::Cooling,
                "slow_down_for_cooling",
                "Slow down for cooling",
            ),
            Group::OverhangsAndBridges => (
                Category::Cooling,
                "overhangs_and_bridges",
                "Overhangs and bridges",
            ),
            Group::AuxiliaryFan => (Category::Cooling, "auxiliary_fan", "Auxiliary fan"),
            Group::ExhaustFan => (Category::Cooling, "exhaust_fan", "Exhaust fan"),
            Group::Retraction => (Category::SettingOverrides, "retraction", "Retraction"),
            Group::ToolchangeRetraction => (
                Category::SettingOverrides,
                "toolchange_retraction",
                "Retraction on filament change",
            ),
            Group::OverrideSpeed => (Category::SettingOverrides, "override_speed", "Speed"),
            Group::FilamentChange => (Category::Advanced, "filament_change", "Filament change"),
            Group::FilamentPrimeTower => {
                (Category::Advanced, "filament_prime_tower", "Prime tower")
            }
            Group::LoadingAndRamming => (
                Category::Advanced,
                "loading_and_ramming",
                "Loading and ramming",
            ),
            Group::CircleCompensation => (
                Category::Advanced,
                "circle_compensation",
                "Circle compensation",
            ),
        }
    }

    pub fn category(self) -> Category {
        self.info().0
    }

    /// The id in queries (unique over all sets).
    pub fn id(self) -> &'static str {
        self.info().1
    }

    /// The section's title.
    pub fn title(self) -> &'static str {
        self.info().2
    }
}

/// The categories of a set and their groups, in display order.
pub fn layout(set: PrintSet) -> Vec<(Category, Vec<Group>)> {
    Category::of_set(set)
        .map(|c| (c, c.groups().collect()))
        .collect()
}

/// A slicer whose settings Mitcad reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dialect {
    BambuStudio,
    OrcaSlicer,
    QidiStudio,
    PrusaSlicer,
    Cura,
}

impl Dialect {
    pub const ALL: [Dialect; 5] = [
        Dialect::BambuStudio,
        Dialect::OrcaSlicer,
        Dialect::QidiStudio,
        Dialect::PrusaSlicer,
        Dialect::Cura,
    ];

    /// The id in commands and files.
    pub fn id(self) -> &'static str {
        match self {
            Dialect::BambuStudio => "bambu_studio",
            Dialect::OrcaSlicer => "orcaslicer",
            Dialect::QidiStudio => "qidi_studio",
            Dialect::PrusaSlicer => "prusaslicer",
            Dialect::Cura => "cura",
        }
    }

    /// The slicer's name.
    pub fn name(self) -> &'static str {
        match self {
            Dialect::BambuStudio => "Bambu Studio",
            Dialect::OrcaSlicer => "OrcaSlicer",
            Dialect::QidiStudio => "QIDI Studio",
            Dialect::PrusaSlicer => "PrusaSlicer",
            Dialect::Cura => "UltiMaker Cura",
        }
    }

    pub fn from_id(id: &str) -> Option<Dialect> {
        Dialect::ALL.into_iter().find(|d| d.id() == id)
    }

    /// One of the Bambu Studio family, which share a project file format.
    pub fn bambu_family(self) -> bool {
        matches!(
            self,
            Dialect::BambuStudio | Dialect::OrcaSlicer | Dialect::QidiStudio
        )
    }

    fn bit(self) -> u8 {
        match self {
            Dialect::BambuStudio => B,
            Dialect::OrcaSlicer => O,
            Dialect::QidiStudio => Q,
            Dialect::PrusaSlicer => P,
            Dialect::Cura => C,
        }
    }

    /// How the dialect writes yes and no.
    fn bool_text(self, value: bool) -> &'static str {
        match (self, value) {
            (Dialect::Cura, true) => "True",
            (Dialect::Cura, false) => "False",
            (_, true) => "1",
            (_, false) => "0",
        }
    }

    /// How the dialect writes a percentage: `15%`, or Cura's `15`.
    fn percent_text(self, percent: f64) -> String {
        match self {
            Dialect::Cura => number_text(percent),
            _ => format!("{}%", number_text(percent)),
        }
    }
}

impl fmt::Display for Dialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The unit of a key's numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    None,
    Mm,
    Mm2,
    Mm3,
    MmPerS,
    MmPerS2,
    Mm3PerS,
    Mm3PerS2,
    Celsius,
    Degree,
    Second,
    Percent,
    GPerCm3,
    PerKg,
    Gram,
    Layers,
}

impl Unit {
    /// The unit as shown (empty for none).
    pub fn symbol(self) -> &'static str {
        match self {
            Unit::None => "",
            Unit::Mm => "mm",
            Unit::Mm2 => "mm²",
            Unit::Mm3 => "mm³",
            Unit::MmPerS => "mm/s",
            Unit::MmPerS2 => "mm/s²",
            Unit::Mm3PerS => "mm³/s",
            Unit::Mm3PerS2 => "mm³/s²",
            Unit::Celsius => "°C",
            Unit::Degree => "°",
            Unit::Second => "s",
            Unit::Percent => "%",
            Unit::GPerCm3 => "g/cm³",
            Unit::PerKg => "per kg",
            Unit::Gram => "g",
            Unit::Layers => "layers",
        }
    }
}

/// The type of a Mitcad key's values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Boolean,
    Integer(Unit),
    Number(Unit),
    /// A percentage as a number: 15 is 15 %.
    Percent,
    /// A number in the unit, or a percentage of the key named (empty: of
    /// nothing in particular, which only a dialect that takes percentages
    /// the same way can keep).
    OrPercent(Unit, &'static str),
    Text,
    /// `#RRGGBB` (or `#RRGGBBAA`).
    Color,
    /// One of the values, each `(value, name)`: the value is Mitcad's in
    /// commands and files, the name the plain one the interface shows
    /// (mitcad#655).
    Choice(&'static [(&'static str, &'static str)]),
    /// An outline in the bed's plane, mm.
    Points,
}

impl Kind {
    pub fn unit(self) -> Unit {
        match self {
            Kind::Integer(unit) | Kind::Number(unit) | Kind::OrPercent(unit, _) => unit,
            Kind::Percent => Unit::Percent,
            _ => Unit::None,
        }
    }

    /// A choice's values (none for other kinds).
    pub fn choice_values(self) -> impl Iterator<Item = &'static str> {
        let choices: &'static [(&'static str, &'static str)] = match self {
            Kind::Choice(choices) => choices,
            _ => &[],
        };
        choices.iter().map(|(value, _)| *value)
    }

    /// Whether a text is one of a choice's values.
    pub fn has_choice(self, text: &str) -> bool {
        self.choice_values().any(|value| value == text)
    }
}

/// How a dialect writes a key's value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Form {
    /// As Mitcad's kind (in the dialect's way of writing percentages and
    /// yes/no).
    Same,
    /// A whole number (a dialect integer for a Mitcad number).
    Int,
    /// An absolute number only: a percentage is resolved first.
    Absolute,
    /// Yes for no.
    Invert,
    /// dialect = a × Mitcad + b (a ratio as a percentage, a count from 0).
    Linear(f64, f64),
    /// A number, or a percentage of the Mitcad key named (empty: of no
    /// single value).
    OrPercent(&'static str),
    /// A percentage of the Mitcad key named only.
    PercentOf(&'static str),
    /// The choices by (Mitcad name, dialect text); for yes/no, `true` and
    /// `false`.
    Choices(&'static [(&'static str, &'static str)]),
    /// The value in two keys: `key` says yes or no; `off` is the Mitcad
    /// choice for no; otherwise the dialect key holds the choice
    /// (`choices` maps its names; none: Mitcad's).
    Switch {
        key: &'static str,
        off: &'static str,
        choices: &'static [(&'static str, &'static str)],
    },
    /// The number of entries of the dialect's list key (read only: the
    /// writer writes the lists).
    Count,
    /// Cura's bed: `machine_width`, `machine_depth`,
    /// `machine_center_is_zero` and `machine_shape`.
    CuraBed,
}

/// A Mitcad key's counterpart in some dialects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialectKey {
    /// The dialects, as bits.
    dialects: u8,
    /// The dialect's key.
    pub key: &'static str,
    /// The dialect keeps one value per filament or extruder (a list in
    /// the Bambu family and PrusaSlicer, each extruder's stack in Cura).
    pub list: bool,
    pub form: Form,
    /// The numbers the dialect takes (its own, after the form's
    /// conversion), when they are narrower than another dialect's: the
    /// Bambu Studio family shares most keys, but where Bambu Studio takes
    /// -1 for "automatic", OrcaSlicer refuses a project that has it.
    /// Export leaves a value out of the range out of that dialect's file
    /// (the slicer keeps its own default) with a warning.
    pub range: Option<(f64, f64)>,
}

impl DialectKey {
    pub fn has(&self, dialect: Dialect) -> bool {
        self.dialects & dialect.bit() != 0
    }

    /// Whether a dialect's text of the key is in its range: a number in it,
    /// or no plain number at all (a percentage, `nil`, text).
    fn in_range(&self, text: &str) -> bool {
        match (self.range, parse_number(text)) {
            (Some((min, max)), Ok(x)) => (min..=max).contains(&x),
            _ => true,
        }
    }
}

/// A Mitcad key of the print settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyDef {
    pub key: &'static str,
    pub set: PrintSet,
    pub kind: Kind,
    /// The plain name the interface shows (the key in the tooltip).
    pub name: &'static str,
    /// Its sub-group in the settings form (mitcad#655); the group knows its
    /// category.
    pub group: Group,
    /// Shown only with Advanced on, or when an item sets it.
    pub advanced: bool,
    /// The value names a dialect's own preset or model: a profile keeps
    /// one per dialect.
    pub per_dialect: bool,
    dialects: &'static [DialectKey],
}

impl KeyDef {
    /// The key's counterpart in a dialect.
    pub fn dialect(&self, dialect: Dialect) -> Option<&'static DialectKey> {
        self.dialects.iter().find(|d| d.has(dialect))
    }

    /// The dialects that have a counterpart.
    pub fn dialects(&self) -> impl Iterator<Item = Dialect> + '_ {
        Dialect::ALL
            .into_iter()
            .filter(|d| self.dialect(*d).is_some())
    }

    pub fn unit(&self) -> Unit {
        self.kind.unit()
    }

    /// Its tab in the settings form.
    pub fn category(&self) -> Category {
        self.group.category()
    }

    /// Whether the key is a filament's override of the printer's extruder
    /// settings ([`FILAMENT_OVERRIDES`]).
    pub fn overrides_printer(&self) -> bool {
        self.set == PrintSet::Filament && FILAMENT_OVERRIDES.contains(&self.key)
    }

    /// Whether a value is one of this key's: the right kind, finite, a
    /// known choice, a colour, an outline.
    pub fn check(&self, value: &Value) -> Result<(), ConvError> {
        let ok = match (self.kind, value) {
            (Kind::Boolean, Value::Bool(_)) => true,
            (Kind::Integer(_), Value::Integer(_)) => true,
            (Kind::Number(_) | Kind::Percent, Value::Number(x)) => x.is_finite(),
            (Kind::OrPercent(..), Value::Number(x) | Value::Percent(x)) => x.is_finite(),
            (Kind::Text, Value::Text(_)) => true,
            (Kind::Color, Value::Text(text)) => color(text).is_some(),
            (Kind::Choice(_), Value::Text(text)) => self.kind.has_choice(text),
            (Kind::Points, Value::Points(points)) => {
                points.len() >= 3 && points.iter().flatten().all(|v| v.is_finite())
            }
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(ConvError(format!(
                "{} is no value of {} ({})",
                value,
                self.key,
                self.kind_text()
            )))
        }
    }

    /// The kind in words, for messages.
    pub fn kind_text(&self) -> String {
        match self.kind {
            Kind::Boolean => "yes or no".to_owned(),
            Kind::Integer(_) => "a whole number".to_owned(),
            Kind::Number(_) => "a number".to_owned(),
            Kind::Percent => "a percentage".to_owned(),
            Kind::OrPercent(..) => "a number or a percentage".to_owned(),
            Kind::Text => "text".to_owned(),
            Kind::Color => "a color #RRGGBB".to_owned(),
            Kind::Choice(_) => format!(
                "one of {}",
                self.kind.choice_values().collect::<Vec<_>>().join(", ")
            ),
            Kind::Points => "an outline of three or more points".to_owned(),
        }
    }

    /// The numbers the key takes, `(min, max)` in its unit (mitcad#746):
    /// the widest range its slicers define, so that an editor refuses only
    /// what none of them takes; infinite where there is no bound. A
    /// number-or-percentage key's range is that of its numbers. `None` for
    /// keys that are not numbers.
    pub fn limits(&self) -> Option<(f64, f64)> {
        match self.kind {
            Kind::Integer(_) | Kind::Number(_) | Kind::Percent | Kind::OrPercent(..) => Some(
                limits::LIMITS
                    .iter()
                    .find(|(key, _, _)| *key == self.key)
                    .map_or((0.0, f64::INFINITY), |(_, min, max)| (*min, *max)),
            ),
            _ => None,
        }
    }

    /// The numbers a dialect takes where they are narrower than the key's
    /// [`limits`](Self::limits) (mitcad#746: Bambu Studio's -1 for
    /// "automatic" is not OrcaSlicer's), in Mitcad's unit: the dialect
    /// key's range when the dialect writes the value as it is.
    pub fn dialect_limits(&self, dialect: Dialect) -> Option<(f64, f64)> {
        let dk = self.dialect(dialect)?;
        match dk.form {
            Form::Same | Form::Int => dk.range,
            _ => None,
        }
    }
}

/// A value of a Mitcad key.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Integer(i64),
    /// A number in the key's unit (a percentage key's percentage).
    Number(f64),
    /// A percentage of the key's basis ([`Kind::OrPercent`]).
    Percent(f64),
    /// Text, a colour or a choice's name.
    Text(String),
    Points(Vec<[f64; 2]>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(true) => f.write_str("yes"),
            Value::Bool(false) => f.write_str("no"),
            Value::Integer(i) => write!(f, "{i}"),
            Value::Number(x) => f.write_str(&number_text(*x)),
            Value::Percent(p) => write!(f, "{}%", number_text(*p)),
            Value::Text(text) => write!(f, "'{text}'"),
            Value::Points(points) => f.write_str(&points_text(points)),
        }
    }
}

impl Value {
    /// The value in commands and files: numbers, integers, yes/no as
    /// true/false, text, a percentage of a basis as `"15%"`, an outline as
    /// `[[x, y], ...]`.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Value::Bool(b) => json!(b),
            Value::Integer(i) => json!(i),
            Value::Number(x) => json!(x),
            Value::Percent(p) => json!(format!("{}%", number_text(*p))),
            Value::Text(text) => json!(text),
            Value::Points(points) => json!(points),
        }
    }

    /// A value of `def` from its JSON form ([`Value::to_json`]); numbers
    /// given as text are read too.
    pub fn from_json(def: &KeyDef, value: &serde_json::Value) -> Result<Value, ConvError> {
        let refuse = || {
            ConvError(format!(
                "{value} is no value of {} ({})",
                def.key,
                def.kind_text()
            ))
        };
        let number = |value: &serde_json::Value| match value {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => parse_number(s).ok(),
            _ => None,
        };
        let parsed = match def.kind {
            Kind::Boolean => match value {
                serde_json::Value::Bool(b) => Value::Bool(*b),
                serde_json::Value::String(s) => Value::Bool(parse_bool(s).map_err(|_| refuse())?),
                _ => return Err(refuse()),
            },
            Kind::Integer(_) => {
                let x = number(value).ok_or_else(refuse)?;
                if x.fract() != 0.0 {
                    return Err(refuse());
                }
                Value::Integer(x as i64)
            }
            Kind::Number(_) | Kind::Percent => {
                let x = match value {
                    serde_json::Value::String(s) => {
                        parse_number(s.trim().trim_end_matches('%')).map_err(|_| refuse())?
                    }
                    _ => number(value).ok_or_else(refuse)?,
                };
                Value::Number(x)
            }
            Kind::OrPercent(..) => match value {
                serde_json::Value::String(s) if s.trim().ends_with('%') => Value::Percent(
                    parse_number(s.trim().trim_end_matches('%')).map_err(|_| refuse())?,
                ),
                _ => Value::Number(number(value).ok_or_else(refuse)?),
            },
            Kind::Text | Kind::Color | Kind::Choice(_) => match value {
                serde_json::Value::String(s) => Value::Text(match def.kind {
                    Kind::Color => color(s).ok_or_else(refuse)?,
                    _ => s.clone(),
                }),
                _ => return Err(refuse()),
            },
            Kind::Points => {
                let points: Vec<[f64; 2]> =
                    serde_json::from_value(value.clone()).map_err(|_| refuse())?;
                Value::Points(points)
            }
        };
        def.check(&parsed)?;
        Ok(parsed)
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(x) => Some(*x),
            Value::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }
}

/// Why a value does not convert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvError(pub String);

impl fmt::Display for ConvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConvError {}

fn error(message: impl Into<String>) -> ConvError {
    ConvError(message.into())
}

/// Mitcad's values by key, for percentages of another value.
pub type Context<'a> = &'a dyn Fn(&str) -> Option<Value>;

/// No other values: a percentage of another value does not resolve.
pub fn no_context(_: &str) -> Option<Value> {
    None
}

/// Every key of the map.
pub fn keys() -> &'static [KeyDef] {
    table::KEYS
}

/// A key by its Mitcad name.
pub fn key(key: &str) -> Option<&'static KeyDef> {
    static INDEX: LazyLock<HashMap<&'static str, &'static KeyDef>> =
        LazyLock::new(|| table::KEYS.iter().map(|k| (k.key, k)).collect());
    INDEX.get(key).copied()
}

/// The filament keys that override the printer's extruder settings
/// (retraction, z hop, wipe) for one filament. The Bambu family and
/// PrusaSlicer keep them as lists that may hold `nil`: that filament takes
/// the printer's value. A list where only some filaments set the value is
/// written with `nil` for the others.
pub const FILAMENT_OVERRIDES: &[&str] = &[
    "filament_retract_length",
    "filament_retract_lift",
    "filament_retract_speed",
    "filament_deretract_speed",
    "filament_retract_restart_extra",
    "filament_retract_before_travel",
    "filament_retract_layer_change",
    "filament_wipe",
    "filament_retract_before_wipe",
    "filament_retract_lift_above",
    "filament_retract_lift_below",
    "filament_retract_length_toolchange",
    "filament_retract_restart_extra_toolchange",
    "filament_z_hop_types",
    "filament_retract_length_nc",
    "filament_wipe_distance",
    "filament_long_retractions_when_cut",
    "filament_retraction_distances_when_cut",
];

/// The keys that are Mitcad's own, with no dialect key (mitcad#705): a
/// filament's purge volumes into and out of it, which the export turns
/// into the slicers' purge matrices (the model's `print_purge`).
pub const MITCAD_OWN: &[&str] = &["purge_into", "purge_out"];

/// The keys of a layer, in the table's order.
pub fn keys_in(set: PrintSet) -> impl Iterator<Item = &'static KeyDef> {
    table::KEYS.iter().filter(move |k| k.set == set)
}

/// The Mitcad keys that read a dialect's key (directly, or as a
/// switch's yes/no key).
pub fn by_dialect_key(dialect: Dialect, dialect_key: &str) -> Vec<&'static KeyDef> {
    table::KEYS
        .iter()
        .filter(|k| {
            k.dialect(dialect).is_some_and(|d| {
                d.key == dialect_key
                    || matches!(d.form, Form::Switch { key, .. } if key == dialect_key)
                    || (d.form == Form::CuraBed && CURA_BED.contains(&dialect_key))
            })
        })
        .collect()
}

/// The number of keys per layer and of those each dialect has.
#[allow(clippy::type_complexity)]
pub fn counts() -> Vec<(PrintSet, usize, Vec<(Dialect, usize)>)> {
    PrintSet::ALL
        .into_iter()
        .map(|set| {
            let keys: Vec<&KeyDef> = keys_in(set).collect();
            let per_dialect = Dialect::ALL
                .into_iter()
                .map(|d| (d, keys.iter().filter(|k| k.dialect(d).is_some()).count()))
                .collect();
            (set, keys.len(), per_dialect)
        })
        .collect()
}

/// Cura's keys of the bed ([`Form::CuraBed`]).
pub const CURA_BED: [&str; 4] = [
    "machine_width",
    "machine_depth",
    "machine_center_is_zero",
    "machine_shape",
];

/// Reads a value of `def` from a dialect: `raw` gives the dialect's text
/// of a key (one entry of a list key: the caller splits lists by filament
/// or extruder; an outline is one value, `XxY,XxY,...`). None: the
/// dialect has no counterpart, the value is not there, or it is `nil`
/// (not set, the Bambu family's and PrusaSlicer's filament overrides).
pub fn read(
    def: &KeyDef,
    dialect: Dialect,
    raw: &dyn Fn(&str) -> Option<String>,
    context: Context<'_>,
) -> Result<Option<Value>, ConvError> {
    let Some(dk) = def.dialect(dialect) else {
        return Ok(None);
    };
    let value = match dk.form {
        Form::Count => {
            let Some(text) = raw(dk.key) else {
                return Ok(None);
            };
            Value::Integer(split_list(&text).len() as i64)
        }
        Form::CuraBed => {
            let (Some(width), Some(depth)) = (raw("machine_width"), raw("machine_depth")) else {
                return Ok(None);
            };
            let (width, depth) = (parse_number(&width)?, parse_number(&depth)?);
            let centred = match raw("machine_center_is_zero") {
                Some(text) => parse_bool(&text)?,
                None => false,
            };
            let (x0, y0) = if centred {
                (-width / 2.0, -depth / 2.0)
            } else {
                (0.0, 0.0)
            };
            Value::Points(vec![
                [x0, y0],
                [x0 + width, y0],
                [x0 + width, y0 + depth],
                [x0, y0 + depth],
            ])
        }
        Form::Switch { key, off, choices } => {
            let Some(on) = raw(key) else {
                return Ok(None);
            };
            if !parse_bool(&on)? {
                Value::Text(off.to_owned())
            } else {
                let Some(text) = raw(dk.key) else {
                    return Ok(None);
                };
                let text = text.trim();
                let name = if choices.is_empty() {
                    text
                } else {
                    choices
                        .iter()
                        .find(|(_, d)| *d == text)
                        .map(|(m, _)| *m)
                        .ok_or_else(|| unknown_choice(def, dialect, text))?
                };
                Value::Text(name.to_owned())
            }
        }
        _ => {
            let Some(text) = raw(dk.key) else {
                return Ok(None);
            };
            let text = text.trim();
            if text == "nil" || (text.is_empty() && !matches!(def.kind, Kind::Text)) {
                return Ok(None);
            }
            from_text(def, dk, dialect, text, context)?
        }
    };
    def.check(&value)?;
    Ok(Some(value))
}

/// Reads a value of `def` from the dialect's text of its own key.
pub fn read_text(
    def: &KeyDef,
    dialect: Dialect,
    text: &str,
    context: Context<'_>,
) -> Result<Option<Value>, ConvError> {
    let key = def.dialect(dialect).map(|d| d.key);
    read(
        def,
        dialect,
        &|k| (Some(k) == key).then(|| text.to_owned()),
        context,
    )
}

/// Writes a value of `def` in a dialect: the dialect's keys and texts
/// (none when the dialect has no counterpart; one entry of a list key).
pub fn write(
    def: &KeyDef,
    dialect: Dialect,
    value: &Value,
    context: Context<'_>,
) -> Result<Vec<(&'static str, String)>, ConvError> {
    def.check(value)?;
    let Some(dk) = def.dialect(dialect) else {
        return Ok(Vec::new());
    };
    match dk.form {
        Form::Count => Ok(Vec::new()),
        Form::CuraBed => {
            let Value::Points(points) = value else {
                unreachable!("checked")
            };
            let (min, max) = bounds(points);
            let centred = (min[0] + max[0]).abs() < 1e-6 && (min[1] + max[1]).abs() < 1e-6;
            if !centred && (min[0].abs() > 1e-6 || min[1].abs() > 1e-6) {
                return Err(error(format!(
                    "{dialect} takes a bed from 0, 0 or centered on it, not {}",
                    points_text(points)
                )));
            }
            Ok(vec![
                ("machine_width", number_text(max[0] - min[0])),
                ("machine_depth", number_text(max[1] - min[1])),
                (
                    "machine_center_is_zero",
                    dialect.bool_text(centred).to_owned(),
                ),
                ("machine_shape", "rectangular".to_owned()),
            ])
        }
        Form::Switch { key, off, choices } => {
            let Value::Text(name) = value else {
                unreachable!("checked")
            };
            if name == off {
                return Ok(vec![(key, dialect.bool_text(false).to_owned())]);
            }
            let text = if choices.is_empty() {
                name.as_str()
            } else {
                choices
                    .iter()
                    .find(|(m, _)| *m == name)
                    .map(|(_, d)| *d)
                    .ok_or_else(|| missing_choice(def, dialect, name))?
            };
            Ok(vec![
                (key, dialect.bool_text(true).to_owned()),
                (dk.key, text.to_owned()),
            ])
        }
        _ => {
            let text = to_text(def, dk, dialect, value, context)?;
            if !dk.in_range(&text) {
                let (min, max) = dk.range.unwrap_or_default();
                let range = if max.is_finite() {
                    format!("from {} to {}", number_text(min), number_text(max))
                } else {
                    format!("from {} up", number_text(min))
                };
                return Err(error(format!(
                    "{dialect} takes {} {range} only, not {text}: left to the slicer's default",
                    dk.key
                )));
            }
            Ok(vec![(dk.key, text)])
        }
    }
}

/// A dialect's text of a key's value as a Mitcad value.
fn from_text(
    def: &KeyDef,
    dk: &DialectKey,
    dialect: Dialect,
    text: &str,
    context: Context<'_>,
) -> Result<Value, ConvError> {
    if let Form::Choices(pairs) = dk.form {
        let name = pairs
            .iter()
            .find(|(_, d)| *d == text)
            .map(|(m, _)| *m)
            .ok_or_else(|| unknown_choice(def, dialect, text))?;
        return Ok(match def.kind {
            Kind::Boolean => Value::Bool(name == "true"),
            _ => Value::Text(name.to_owned()),
        });
    }
    let linear = |x: f64| match dk.form {
        Form::Linear(a, b) => (x - b) / a,
        _ => x,
    };
    Ok(match def.kind {
        Kind::Boolean => {
            let b = parse_bool(text)?;
            Value::Bool(if dk.form == Form::Invert { !b } else { b })
        }
        Kind::Integer(_) => {
            let x = linear(parse_number(text)?);
            if (x - x.round()).abs() > 1e-9 {
                return Err(error(format!(
                    "{dialect}: “{text}” of {} is no whole number",
                    dk.key
                )));
            }
            Value::Integer(x.round() as i64)
        }
        Kind::Number(_) => match (dk.form, percentage(text)) {
            (Form::OrPercent(of), Some(p)) => Value::Number(of_basis(p, of, context)?),
            _ => Value::Number(linear(parse_number(text)?)),
        },
        Kind::Percent => Value::Number(linear(parse_number(text.trim_end_matches('%'))?)),
        Kind::OrPercent(_, basis) => {
            let p = percentage(text);
            match (dk.form, p) {
                (Form::PercentOf(of), _) => {
                    let p = parse_number(text.trim_end_matches('%'))?;
                    percent_value(p, of, basis, context)?
                }
                (Form::OrPercent(of), Some(p)) => percent_value(p, of, basis, context)?,
                (Form::Same, Some(p)) => Value::Percent(p),
                (_, Some(_)) => {
                    return Err(error(format!(
                        "{dialect}: {} takes no percentage, got “{text}”",
                        dk.key
                    )));
                }
                (_, None) => Value::Number(linear(parse_number(text)?)),
            }
        }
        Kind::Text => Value::Text(text.to_owned()),
        Kind::Color => Value::Text(
            color(text)
                .ok_or_else(|| error(format!("{dialect}: “{text}” of {} is no color", dk.key)))?,
        ),
        Kind::Choice(_) => {
            if !def.kind.has_choice(text) {
                return Err(unknown_choice(def, dialect, text));
            }
            Value::Text(text.to_owned())
        }
        Kind::Points => Value::Points(parse_points(text)?),
    })
}

/// A Mitcad value as the dialect's text of its key.
fn to_text(
    def: &KeyDef,
    dk: &DialectKey,
    dialect: Dialect,
    value: &Value,
    context: Context<'_>,
) -> Result<String, ConvError> {
    if let Form::Choices(pairs) = dk.form {
        let name = match value {
            Value::Bool(true) => "true",
            Value::Bool(false) => "false",
            Value::Text(text) => text.as_str(),
            _ => unreachable!("checked"),
        };
        return pairs
            .iter()
            .find(|(m, _)| *m == name)
            .map(|(_, d)| (*d).to_owned())
            .ok_or_else(|| missing_choice(def, dialect, name));
    }
    let linear = |x: f64| match dk.form {
        Form::Linear(a, b) => a * x + b,
        Form::Int => x.round(),
        _ => x,
    };
    Ok(match (def.kind, value) {
        (Kind::Boolean, Value::Bool(b)) => {
            let b = if dk.form == Form::Invert { !b } else { *b };
            dialect.bool_text(b).to_owned()
        }
        (Kind::Integer(_), Value::Integer(i)) => number_text(linear(*i as f64)),
        (Kind::Number(_), Value::Number(x)) => number_text(linear(*x)),
        (Kind::Percent, Value::Number(x)) => match dk.form {
            Form::Linear(..) => number_text(linear(*x)),
            _ => dialect.percent_text(*x),
        },
        (Kind::OrPercent(..), Value::Number(x)) => match dk.form {
            Form::PercentOf(of) => {
                let base = absolute(of, context, 0).ok_or_else(|| no_basis(def, of))?;
                if base == 0.0 {
                    return Err(no_basis(def, of));
                }
                dialect.percent_text(x / base * 100.0)
            }
            _ => number_text(linear(*x)),
        },
        (Kind::OrPercent(_, basis), Value::Percent(p)) => match dk.form {
            Form::Same => dialect.percent_text(*p),
            Form::OrPercent(of) | Form::PercentOf(of) if of == basis => dialect.percent_text(*p),
            form => {
                if basis.is_empty() {
                    return Err(error(format!(
                        "{dialect} cannot take {} of {}: it is a percentage of no single value",
                        value, def.key
                    )));
                }
                let base = absolute(basis, context, 0).ok_or_else(|| no_basis(def, basis))?;
                let x = p / 100.0 * base;
                match form {
                    Form::PercentOf(of) => {
                        let other = absolute(of, context, 0).ok_or_else(|| no_basis(def, of))?;
                        if other == 0.0 {
                            return Err(no_basis(def, of));
                        }
                        dialect.percent_text(x / other * 100.0)
                    }
                    _ => number_text(linear(x)),
                }
            }
        },
        (Kind::Text | Kind::Color | Kind::Choice(_), Value::Text(text)) => text.clone(),
        (Kind::Points, Value::Points(points)) => points_text(points),
        _ => unreachable!("checked"),
    })
}

/// A percentage of a dialect's basis `of` as a value of a key whose
/// percentages are of `basis`: kept while the bases are the same, else
/// resolved to an absolute value.
fn percent_value(p: f64, of: &str, basis: &str, context: Context<'_>) -> Result<Value, ConvError> {
    if of == basis {
        Ok(Value::Percent(p))
    } else {
        Ok(Value::Number(of_basis(p, of, context)?))
    }
}

/// p % of the value of the Mitcad key `of`.
fn of_basis(p: f64, of: &str, context: Context<'_>) -> Result<f64, ConvError> {
    if of.is_empty() {
        return Err(error(format!(
            "{}% is a percentage of no single value",
            number_text(p)
        )));
    }
    let base = absolute(of, context, 0)
        .ok_or_else(|| error(format!("{}% of {of}: there is no {of}", number_text(p))))?;
    Ok(p / 100.0 * base)
}

/// The absolute value of a Mitcad key in the context, through its own
/// percentage basis when it is one.
fn absolute(key_name: &str, context: Context<'_>, depth: usize) -> Option<f64> {
    if depth > 8 {
        return None;
    }
    match context(key_name)? {
        Value::Percent(p) => match key(key_name)?.kind {
            Kind::OrPercent(_, basis) if !basis.is_empty() => {
                Some(p / 100.0 * absolute(basis, context, depth + 1)?)
            }
            _ => None,
        },
        value => value.as_f64(),
    }
}

fn no_basis(def: &KeyDef, of: &str) -> ConvError {
    error(format!(
        "{} needs {of} to convert a percentage, and there is none",
        def.key
    ))
}

fn unknown_choice(def: &KeyDef, dialect: Dialect, text: &str) -> ConvError {
    error(format!(
        "{dialect}: “{text}” is no known value of {}",
        def.dialect(dialect).map_or(def.key, |d| d.key)
    ))
}

fn missing_choice(def: &KeyDef, dialect: Dialect, name: &str) -> ConvError {
    error(format!("{dialect} has no {name} for {}", def.key))
}

/// A number as written: up to nine decimals, no exponent, no `-0`.
pub fn number_text(x: f64) -> String {
    let rounded = (x * 1e9).round() / 1e9;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    format!("{rounded}")
}

pub fn parse_number(text: &str) -> Result<f64, ConvError> {
    text.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
        .ok_or_else(|| error(format!("“{text}” is no number")))
}

/// The percentage of a `15%` text.
fn percentage(text: &str) -> Option<f64> {
    let number = text.trim().strip_suffix('%')?;
    parse_number(number).ok()
}

pub fn parse_bool(text: &str) -> Result<bool, ConvError> {
    match text.trim() {
        "1" | "true" | "True" | "TRUE" | "yes" => Ok(true),
        "0" | "false" | "False" | "FALSE" | "no" => Ok(false),
        _ => Err(error(format!("“{text}” is no yes or no"))),
    }
}

/// `#RRGGBB` or `#RRGGBBAA`, upper case.
pub fn color(text: &str) -> Option<String> {
    let hex = text.trim().strip_prefix('#')?;
    ((hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", hex.to_ascii_uppercase()))
}

/// An outline `XxY,XxY,...` (mm).
pub fn parse_points(text: &str) -> Result<Vec<[f64; 2]>, ConvError> {
    text.split(',')
        .map(|point| {
            let (x, y) = point
                .trim()
                .split_once('x')
                .ok_or_else(|| error(format!("“{point}” is no point XxY")))?;
            Ok([parse_number(x)?, parse_number(y)?])
        })
        .collect()
}

pub fn points_text(points: &[[f64; 2]]) -> String {
    points
        .iter()
        .map(|[x, y]| format!("{}x{}", number_text(*x), number_text(*y)))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn bounds(points: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    points.iter().fold(
        ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
        |(min, max), p| {
            (
                [min[0].min(p[0]), min[1].min(p[1])],
                [max[0].max(p[0]), max[1].max(p[1])],
            )
        },
    )
}

/// A PrusaSlicer INI list (`215,215`, or text `"PLA";"PETG"`, quoted only
/// where an entry needs it: `PLA;PETG`) as its entries; the Bambu family's
/// lists are JSON arrays already.
pub fn split_list(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    if text.starts_with('"') || text.contains(';') {
        let mut entries = Vec::new();
        let mut current = String::new();
        let mut quoted = false;
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => quoted = !quoted,
                '\\' if quoted => {
                    if let Some(next) = chars.next() {
                        current.push(match next {
                            'n' => '\n',
                            other => other,
                        });
                    }
                }
                ';' if !quoted => entries.push(std::mem::take(&mut current)),
                c if quoted => current.push(c),
                // Unquoted text between the separators.
                c if !c.is_whitespace() => current.push(c),
                _ => {}
            }
        }
        entries.push(current);
        entries
    } else {
        text.split(',').map(|s| s.trim().to_owned()).collect()
    }
}

/// Entries as a PrusaSlicer INI list: strings `;`-separated, each quoted
/// only where it needs it (as PrusaSlicer writes them: `PLA;PETG`,
/// `"Generic PLA";"Generic PETG"`), everything else (numbers, yes/no,
/// choices) `,`-separated.
pub fn join_list(entries: &[String], text: bool) -> String {
    if text {
        entries
            .iter()
            .map(|e| {
                // A separator, a blank, a quote or an escape needs quotes, and
                // so does a list's one empty entry. Commas too: the reader
                // takes an unquoted list without `;` as numbers.
                let quote = e.contains([' ', '\t', ';', ',', '\\', '"', '\r', '\n'])
                    || (e.is_empty() && entries.len() == 1);
                if quote {
                    format!(
                        "\"{}\"",
                        e.replace('\\', "\\\\")
                            .replace('"', "\\\"")
                            .replace('\n', "\\n")
                    )
                } else {
                    e.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(";")
    } else {
        entries.join(",")
    }
}
