// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Profile
//! features: extrudes and their scopes, extrudes from faces, revolves and
//! sweeps (F1 to F3).
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod extrude_scope_tests;
mod f3_tests;
mod face_extrude_tests;
mod hole_size_tests;
mod profile_feature_tests;
// A thread's fit to its face and the wall behind it (mitcad#1262).
mod thread_fit_tests;
// Extents up to a point and starts from a point (mitcad#1329).
mod extent_point_tests;
// Intersection Curve: a 3D curve from two sketches, a path (mitcad#1405).
mod intersection_curve_tests;
// A pipe's Corners, mitered or rounded (mitcad#1448).
mod pipe_corners_tests;
// A pipe whose path bends tighter than its section warns (mitcad#1497).
mod pipe_tight_bends_tests;
