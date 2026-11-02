// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Sketch
//! geometry from elsewhere and kept regular: regular polygons, equal
//! spacing, driven references, dimension names, projections and their
//! offsets, Convert to Sketch Geometry, assembly constraints, section
//! curves and selected profile areas.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod constraint_tests;
mod convert_projected_tests;
mod dimension_names_tests;
mod driven_references_tests;
mod equal_spacing_tests;
mod profile_selection_tests;
mod projected_offset_tests;
mod projection_tests;
mod regular_polygon_tests;
mod section_curve_tests;
// Project of another sketch's curves and points (mitcad#904).
mod face_bounded_profiles_tests;
mod sketch_projection_tests;
