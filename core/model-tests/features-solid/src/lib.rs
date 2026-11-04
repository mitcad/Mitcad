// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Patterns
//! and dressups (F4), construction geometry, renamed references, automatic
//! operations and surface modelling.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod auto_operation_tests;
mod construction_tests;
mod f4_tests;
mod renamed_tests;
mod surface_modelling_tests;
// A Fillet's and a Chamfer's picked edges kept with the result (mitcad#957).
mod feature_inputs_tests;
// Dogbone: relief cuts at a body's inside corners (mitcad#1011).
mod dogbone_tests;
// Auto Fix of references a warning found again (mitcad#1223).
mod auto_fix_tests;
// The end pieces of a modelled thread keep their names (mitcad#1231).
mod thread_end_tests;
