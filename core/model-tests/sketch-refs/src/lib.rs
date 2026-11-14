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
// Driven dimensions used by their own sketch's dimensions (mitcad#1116).
mod sketch_driven_uses_tests;
// Project of a curved face: its outline seen along the sketch's normal
// (mitcad#1265).
mod face_outline_tests;
// Project Again of a projection that stopped following its source
// (mitcad#1303).
mod project_again_tests;
// A dimension's value that uses its own parameter keeps that parameter as
// a user parameter (mitcad#1535).
mod own_parameter_tests;
// A 3D sketch dimension's value that uses its own parameter keeps that
// parameter as a user parameter, as a planar one does (mitcad#1535).
mod sketch3d_own_parameter_tests;
// Remove Name: a parameter's automatic name back (mitcad#1577).
mod remove_name_tests;
// A linked projection whose source edge was merged with, or split from,
// edges on the same curve (mitcad#1784).
mod projection_successor_tests;
