// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Commands
//! and queries in general: the document query, the browser, named views and
//! DXF insert, small UI gaps and metric defaults, failed features,
//! inserting at a timeline position, autoselected profiles, faces and edges
//! with geometry, text parameters, parameter limits, the caches of results,
//! cancelling recomputes, the result store and preview placement.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod api_tests;
mod autoselect_tests;
mod browser_tests;
mod cache_tests;
mod face_lists_tests;
mod failed_features_tests;
mod insert_feature_tests;
mod metric_defaults_tests;
mod monitor_tests;
mod parameter_limits_tests;
mod preview_placement_tests;
mod store_tests;
mod tables_tests;
mod text_parameter_tests;
mod ui_gaps_tests;
mod view_file_tests;
// Configurations: options, rows and rules (mitcad#797).
mod configurations_tests;
// Tables in configurations (mitcad#806).
mod configuration_tables_tests;
// Filling a closed table: generated rows with parameter columns, the rows
// as CSV (mitcad#1044).
mod closed_table_fill_tests;
// The Center of Mass marker and its query (mitcad#1094).
mod center_of_mass_tests;
// Features that share a name: lookups by name refuse it with the uids
// (mitcad#1107).
mod shared_feature_names_tests;
// Allowed-value formulas in independent component copies (mitcad#1129).
mod copy_allowed_values_tests;
// Table references in bindings and rule expressions (mitcad#1131).
mod table_reference_rename_tests;
// The rolled-back features evaluated beside the document, adopted by it
// (mitcad#1267).
mod rolled_back_tests;
// View › Show of the whole design (mitcad#1287).
mod view_show_tests;
// Lengths entered in a design in inches (mitcad#1452).
mod inch_entry_tests;
// The command reference as MCP's agents see it, with the names MCP permits
// (mitcad#1509).
mod mcp_reference_tests;
// A feature value that uses the field's own parameter keeps it as a user
// parameter (mitcad#1536).
mod feature_own_parameter_tests;
// 3D sketches through their sketch3d.* commands and query (mitcad#1522).
mod sketch3d_tests;
// Read-only modeling guidance and bounded print analysis (mitcad#1513).
mod design_review_tests;
// Occurrence-specific print context in design review (mitcad#1513).
mod design_review_print_context_tests;
// What a person reads names no faces, edges, ids or points (mitcad#1612).
mod user_text_guard_tests;
// What a person reads of 3D sketches' refusals and failures names no ids
// (mitcad#1644).
mod sketch3d_user_text_tests;
// 3D relations and dimensions: parallel, perpendicular, angle, symmetry,
// concentric, curvature, On Plane, spline length, tangent magnitude and
// driven dimensions used by the sketch's own (mitcad#1661).
mod sketch3d_relations_tests;
// Copy, Cut and Paste of 3D sketch geometry, and planar geometry pasted into
// a 3D sketch (mitcad#1661).
mod sketch3d_clipboard_tests;
// The `bodies` query's last feature of each body (mitcad#1704).
mod body_last_features_tests;
