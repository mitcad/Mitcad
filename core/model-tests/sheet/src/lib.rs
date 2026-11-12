// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Sheet
//! metal: faces and flanges, rules, flat patterns and what the sheet-metal
//! UI asks.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod sheet_flat_tests;
mod sheet_more_tests;
mod sheet_profile_tests;
mod sheet_tests;
mod sheet_ui_tests;
