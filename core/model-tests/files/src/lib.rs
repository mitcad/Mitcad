// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Files and
//! exchange: import and export, the design file, design comparison,
//! Reorient Design, import notes and hostile sizes in design files.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod diff_tests;
mod exchange_tests;
mod file_tests;
mod import_notes_tests;
mod limits_tests;
mod reorient_tests;
mod tables_tests;
// The design's summary and statistics (mitcad#1088).
mod design_properties_tests;
// Top-level fields a newer MitCad wrote (mitcad#1132).
mod unknown_fields_tests;
// Animated USDZ export (mitcad#1597).
mod usdz_animation_tests;
