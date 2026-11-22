// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Sheet
//! metal: faces and flanges, rules, flat patterns and what the sheet-metal
//! UI asks.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod sheet_edges_tests;
mod sheet_flat_tests;
mod sheet_more_tests;
mod sheet_profile_tests;
mod sheet_tests;
mod sheet_ui_tests;
// A Bill of Materials' Kind and BOM Structure, the Sheet Metal list and
// Flat Pattern parts on cut sheets (mitcad#1980).
mod bom_kind_tests;
// Sheet process profiles and allowance methods (mitcad#1939).
mod sheet_process_tests;
// The Hem command's forms: flat, open and rolled hems (mitcad#2112).
mod sheet_bend_table_tests;
mod sheet_hem_tests;
