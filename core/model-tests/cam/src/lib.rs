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

// Turning on a lathe (mitcad#1869).
mod turning_tests;

// Live tools on a lathe with a C axis (mitcad#2067).
mod live_tests;

// The mill-turn's Y axis, B head, sub-spindle, 3D stock and wrapped
// sketches (mitcad#2319).
mod axes_tests;

// Live tools on the sub-spindle and its machine data (mitcad#2375).
mod sub_live_tests;

// The sub-spindle's body and moving zones, a control that mirrors only Z
// there, the main spindle's operations after it (mitcad#2392).
mod sub_body_tests;

// The sub-spindle drawn in Simulate, the turret's body against it, its own
// profile (mitcad#2405).
mod sub_turret_tests;

// The fixed and moving zones and the turret drawn in Simulate, the
// turret's body against the main chuck and the fixed zones, a body per
// station (mitcad#2412).
mod turret_tests;

// Manual G-code in a lathe setup's program (mitcad#2506).
mod manual_tests;

// Turn Finishing into undercuts: the Plunge choice (mitcad#2503).
mod plunge_tests;

// A turning tool's and a boring bar's insert and holder (mitcad#2502).
mod insert_tests;

// Turn Roughing into undercuts: its Plunge choice, the G71 kept for the
// roughing without one, the finish after it (mitcad#2549).
mod rough_plunge_tests;
