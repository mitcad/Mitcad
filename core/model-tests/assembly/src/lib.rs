// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Components
//! and occurrences, reparenting, Move Features into a Component, body
//! transfer and the design policy.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod body_transfer_tests;
mod component_tests;
mod move_features_tests;
mod policy_tests;
mod reparent_tests;
// Features of independent components evaluated in parallel (mitcad#842).
mod parallel_recompute_tests;
// Save Component as Design (mitcad#1057).
mod save_component_tests;
// Transparent bodies and occurrences (mitcad#1093).
mod transparency_tests;

// Read-only bill-of-materials grouping (mitcad#1176).
mod bom_tests;
// Find Repeated Parts (mitcad#1277).
mod repeated_parts_tests;

// A linked component shown by its envelope (mitcad#1271).
mod link_envelope_tests;

// An Envelope's source through copies and reparenting (mitcad#1276).
mod envelope_source_tests;

// Linked designs read again: on opening and while open (mitcad#1416).
mod link_update_tests;
// A design linked from outside the project, copied into it (mitcad#1719).
mod link_copy_tests;
// Folders of the Bodies folder (mitcad#1802).
mod body_folder_tests;
// New content under hidden parents shows them (mitcad#1995).
mod hidden_parents_tests;
// Folders of a component's occurrences, a Bodies folder's renamed
// (mitcad#2103).
mod occurrence_folder_tests;
// The order of occurrences among their siblings (mitcad#2104).
mod occurrence_order_tests;
// Assembly cuts: linked and multiply placed components machined in an
// assembly only (mitcad#2131).
mod assembly_cut_tests;
// Selection sets: named links to bodies, occurrences and features
// (mitcad#2192).
mod selection_set_tests;
// An extrude whose operation an edit changes takes the features on its
// geometry along (mitcad#2185).
mod operation_follow_tests;
