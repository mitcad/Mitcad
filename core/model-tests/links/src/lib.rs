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
// Linked parts of a design (mitcad#2130).
mod link_parts_tests;
// A component's standing Part number and Description rows, and the parts
// list's part number of a library part with its own (mitcad#2138).
mod part_number_rows_tests;
// Components not for print services and the print_service_check query
// (mitcad#2214).
mod print_service_tests;
// A whole design linked with its components, joints and grounding
// (mitcad#2209).
mod link_tree_tests;
// A design of one component inserted as that component (mitcad#2429).
mod single_component_tests;
// Replace Component: a component swapped, its joints kept (mitcad#2640).
mod replace_component_tests;
