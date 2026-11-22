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
// A fillet on a face joined from pieces on one surface (mitcad#1261).
mod same_surface_join_tests;
// Circular Pattern about two axes; copies that add nothing (mitcad#1588).
mod two_axis_pattern_tests;
// Flex items: nodes, routes, end ports and warnings (mitcad#1713).
mod flex_tests;
// The direction of gravity and a Flex item's slack hanging toward it
// (mitcad#1713, Part B).
mod gravity_tests;
// Coordinate System: frames at a point and exports in them (mitcad#1775).
mod coordinate_system_tests;
// Fill Cavity: a closed pocket filled up to a plane (mitcad#1818).
mod fill_cavity_tests;
// Press Pull continued Straight or Along the walls, tapered (mitcad#1850).
mod press_pull_tests;
// A Flex item let hang, pinned, dragged and locked (mitcad#1713).
mod flex_hang_tests;
// Panelize: a body's faces split off as panels, step by step (mitcad#2005).
mod panelize_tests;
// Cone: a cone or a frustum beside the other primitives (mitcad#2239).
mod cone_tests;
