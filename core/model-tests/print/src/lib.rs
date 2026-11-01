// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): 3D
//! printing: slicers' print settings files, purge volumes, printed fits,
//! the print library and profiles, Print Analysis and the print bed face.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod filament_appearance_tests;
mod filament_variants_tests;
mod print_bed_tests;
mod print_files_corpus;
mod print_files_tests;
mod print_fits_tests;
mod print_library_tests;
mod print_profile_tests;
mod print_tests;
mod purge_tests;
mod slicer_colour_tests;
mod slicer_presets_tests;
// Print settings that reach the slicer (mitcad#1052).
mod print_send_tests;
// The automatic orientation on the print bed (mitcad#1109).
mod print_orient_tests;
