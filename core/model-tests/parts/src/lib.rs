// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Parts made
//! by features: Place At, plastic part features, printable bearings,
//! pulleys, gears, seats, point patterns and mortise and tenon joints.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod gear_tests;
mod place_at_tests;
mod plastic_tests;
mod point_pattern_tests;
mod printable_bearing_tests;
mod pulley_tests;
mod seat_tests;
// Mortise and Tenon (mitcad#1012).
mod mortise_tenon_tests;
