// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777):
//! Appearances, texture maps, render settings, mesh export, typed
//! properties and materials, and bindings of appearances, materials,
//! suppression and visibility to expressions (mitcad#800).
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod appearance_tests;
mod binding_tests;
mod copy_binding_tests;
mod material_properties_tests;
mod material_tests;
mod mesh_export_tests;
mod properties_tests;
mod render_settings_tests;
mod texture_map_tests;
