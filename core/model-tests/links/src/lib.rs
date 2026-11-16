// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Geometry of
//! other components in sketches and component libraries.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod library_tests;
mod link_tests;
// Options of inserted components (mitcad#801).
mod component_options_tests;
// The materials of inserted components (mitcad#1921).
mod inserted_materials_tests;
