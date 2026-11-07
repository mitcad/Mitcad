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
