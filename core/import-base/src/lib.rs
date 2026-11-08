// SPDX-License-Identifier: MIT
//! What Mitcad's imports share (mitcad#775): the `.f3d` and `.ipt` replay
//! (`mitcad-import`), IFC (`mitcad-import-ifc`), `.rvt` (`mitcad-import-rvt`)
//! and FreeCAD documents (`mitcad-import-fcstd`).
//!
//! - [`progress`]: how far an import has come, for the watchdog and the
//!   memory guard of `mitcad-ffi`, and the import running on a thread
//!   ([`tick`], [`current_progress`], [`abandoned`], [`Ticking`]).
//! - [`geom`]: vector helpers and the dump's units.
//! - [`body_scope`]: the bodies that belong to one replay (a receiving
//!   document's own bodies are left alone).
//! - [`planes`]: the planar faces of the bodies at the marker, and the
//!   planar faces and edges kept by body version while an import runs.
//! - [`scaffold`]: the `$k` placeholders, the core of a candidate and the
//!   end of an import.
//! - [`names`], [`notes`], [`sketch`]: free parameter names, the notes'
//!   sentences, and the sketch definitions' references and inactive
//!   constraints and dimensions.
//! - [`free_memory`]: when the memory guard gives the allocator's free
//!   memory back to the system (mitcad#885).
//! - [`outcomes`]: how the `.ipt`, `.iam` and `.dwg` imports' items came
//!   in, and the levels of their notices (mitcad#1051).
//! - [`memory_guard`]: what the memory guard's measurements tell an
//!   import's progress, measured as often and looking ahead as the guard
//!   of model commands (mitcad#1505).

pub mod body_scope;
pub mod geom;
pub mod names;
pub mod notes;
pub mod planes;
pub mod progress;
pub mod scaffold;
pub mod sketch;

pub use progress::{Helper, Progress, Ticking, abandoned, current_progress, tick};

// When the memory guard gives the allocator's free memory back (mitcad#885).
pub mod free_memory;
pub use free_memory::FreeMemoryRelease;

// The outcomes of the .ipt, .iam and .dwg imports and their notices'
// levels (mitcad#1051).
pub mod outcomes;

// What the memory guard's measurements tell an import (mitcad#80), with the
// look-ahead of the guard of model commands (mitcad#1505).
pub mod memory_guard;
pub use memory_guard::ImportMemoryGuard;

/// The default geometric tolerance of the imports (mm, mitcad#224): how far
/// the replay's geometry may be from the file's where no exact result is
/// (`mitcad_import::Options::geometric_tolerance`).
pub const DEFAULT_GEOMETRIC_TOLERANCE: f64 = 0.01;
