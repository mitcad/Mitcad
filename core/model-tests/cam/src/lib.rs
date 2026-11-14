// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api: CAM (mitcad#1013):
//! setups, operations and machines kept with the design, their tool
//! libraries, toolpaths computed from faces, sketch profiles and points,
//! out-of-date marks, the G-code of a setup in millimetres and inches,
//! the design file's `cam` section and the machine library's files, on
//! the mock kernel.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod cam_tests;

// The stock simulation (mitcad#1013, phase 3).
mod simulate_tests;
