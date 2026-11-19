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
// Occurrence placements kept rigid and straightened on opening (mitcad#1664).
mod occurrence_placement_tests;
// The preview a design file carries for file managers (mitcad#1539).
mod design_preview_tests;
// The design packed into a zip for a report (mitcad#1795).
mod design_package_tests;
// glTF files as mesh bodies and components (mitcad#1826).
mod gltf_tests;
// .serp files as bodies by layer, blocks as components (mitcad#1870).
mod serp_tests;
// A sketch's points in its DXF (mitcad#1963).
mod sketch_dxf_points_1963_tests;
// The reporter's arc whose Symmetry partly repeats the others: information,
// not a redundancy warning (mitcad#1992).
mod sketch_partly_redundant_reported_1992_tests;
// import_file stopped inside its read by the memory guard or the user's
// cancel, in the import's own words, the design unchanged (mitcad#2032).
mod import_stopped_2032_tests;
