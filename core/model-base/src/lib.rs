// SPDX-License-Identifier: MIT
//! The base of the Mitcad model (mitcad#777): the [`Kernel`] trait with the
//! data it takes and gives, and the plain data and helpers of the model
//! that need nothing of its core: ids and topological names, profiles,
//! datums, transforms, sweep and surface specifications, parameters,
//! properties, configurations, appearances and render settings, the
//! recompute monitor, fingerprints, SHA-256 and Base64, the ISO 286 fits.
//!
//! `mitcad-model` re-exports the modules here at their old paths
//! (`mitcad_model::kernel`, `mitcad_model::topo`, …), so code above the
//! model names them as before; the helpers the model, the sketch and the
//! mock kernel share with the base are under [`internal`] (mitcad#811).
//! An edit in the model's core does not recompile this crate; the mock
//! kernel (`mitcad-model-testkit`) builds on it alone.

// Parameter expressions and the print settings' key map, at the paths the
// modules here name them by (`crate::expr`, `crate::print_keys`).
use mitcad_expr as expr;
use mitcad_print_keys as print_keys;

pub mod analysis;
// Appearances: physically based looks of bodies (mitcad#46).
pub mod appearance;
pub(crate) mod base64;
// Bodies from their boundaries and from triangles (the IFC import,
// mitcad#273).
pub mod boundary;
// Configuration tables (mitcad#64).
pub mod configurations;
// Points and ends of the kernel's 3D curves and of sweep paths.
pub mod curves;
pub mod datum;
// What the kernel works out for drawings: projected views and round holes
// (mitcad#471).
pub mod drawing_view;
// The data of data exchange the kernel reads and writes.
pub mod exchange;
// Deterministic result versions (P7d).
pub(crate) mod fingerprint;
pub mod ids;
// Items other Mitcad crates use; not part of this crate's API (mitcad#811).
pub mod internal;
// ISO 286 fit codes of tolerances (mitcad#702).
pub mod iso286;
pub mod kernel;
// Progress and cancellation of recomputes (P7).
pub mod monitor;
// Occurrence paths and links to other components' geometry (mitcad#780).
pub mod occurrence;
pub mod parameters;
pub mod profile;
// Typed property sets (mitcad#496).
pub mod properties;
// Render settings of the document (mitcad#47).
pub mod render_settings;
// Named profile segments (gears and pulleys, mitcad#472) and points along
// segments.
pub mod segments;
// Design files' content hashes (P12a).
pub(crate) mod sha256;
// The sheet metal kernel operations' material charts (mitcad#242).
pub mod sheet_charts;
// Surface modelling: surface bodies and their features (mitcad#455).
pub mod surface_modelling;
// Patch, Stitch and Thicken (mitcad#456).
pub mod surfaces;
// Sweeps, lofts, pipes, coils, ribs and webs (F3).
pub mod sweeps;
pub mod topo;
pub mod transform;
// Values in commands and the model's command error (mitcad#780).
pub mod values;
// The sizes of image files from their headers (image references,
// mitcad#782).
pub mod image_header;
// Readers of files the user chose, run in the application: a panic of
// theirs is their error (mitcad#922).
pub mod reader_guard;

pub use kernel::{Kernel, KernelError};
// Design files' content hashes (P12a).
pub use sha256::Sha256;
