// SPDX-License-Identifier: MIT
//! The decoding part of the design segment of `.f3d` files: the streams,
//! the object records and the sketches, below the feature building of
//! `mitcad-f3d-design`.
//!
//! Layers:
//! - [`stream`]: `MetaStream.dat` and the object framing of
//!   `BulkStream.dat` (headers, root parts, references, strings).
//! - [`classes`], [`decode`] and [`sketch`]: decoders for the parts that
//!   the timeline format study marks certain or probable.
//! - [`recipe`] and [`curve`]: the recipes and stored curves that feature
//!   inputs name their entities with.
//! - [`learn`]: raw item records for the import's learning dump
//!   (mitcad#96).
//! - [`writer`]: synthetic design streams for tests (mitcad#196).
//!
//! Until mitcad#779 these were modules of `mitcad-f3d-design`, which
//! re-exports them under their old paths (`mitcad_f3d_design::stream`,
//! `mitcad_f3d::design::stream`, ...).

pub mod classes;
// The curves edge inputs store beside their recipes (mitcad#306).
pub mod curve;
pub mod decode;
// The dump IR is the `mitcad-dump` crate (mitcad#776).
pub use mitcad_dump as ir;
// Raw item records for the import's learning dump (mitcad#96).
pub mod learn;
pub mod recipe;
pub mod sketch;
pub mod stream;
// Python's printable and alphanumeric character tables
// (`core/f3d/tools/gen_unicode_tables.py`).
pub mod unicode;
// Synthetic design streams for tests (mitcad#196).
pub mod writer;
// The design's mesh bodies and their containers (mitcad#1366).
pub mod mesh;
