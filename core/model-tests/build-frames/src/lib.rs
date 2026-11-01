// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): BUILD:
//! frames, framed components, placement, closing, sides and ends, offsets,
//! two-point and tilted members, face starts and junctions.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod close_tests;
mod face_start_tests;
mod frame_component_tests;
mod frame_tests;
mod junction_tests;
mod place_tests;
mod side_end_tests;
mod side_offset_tests;
mod tilt_tests;
mod two_points_tests;
// A BUILD part's options (mitcad#801).
mod options_tests;
// Corner connectors of a frame and their nuts (mitcad#1115).
mod connector_corner_tests;
