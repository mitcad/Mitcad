// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Joints, the
//! joint solver, motion studies, motion links and mount frames.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod joint_poses_tests;
mod joint_solver_tests;
mod joint_tests;
mod loop_drag_tests;
mod motion_link_tests;
mod motion_tests;
mod mount_tests;
