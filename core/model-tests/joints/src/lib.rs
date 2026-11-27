// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Joints, the
//! joint solver, motion studies, motion links and mount frames.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod flexible_nested_tests;
mod flexible_tests;
mod joint_poses_tests;
mod joint_solver_tests;
mod joint_tests;
mod loop_drag_tests;
mod motion_link_tests;
mod motion_tests;
mod mount_tests;
// Deleting a feature another occurrence is joined to (mitcad#1820).
mod delete_joined_feature_tests;
// An edit's preview with the joints after the edited one (mitcad#1860).
mod edit_preview_through_tests;

// The component a joint's picks belong in (mitcad#1856).
mod joint_holder_tests;
// Flexible subassemblies without joints of their own (mitcad#2016).
mod flexible_free_parts_tests;
// A new joint keeps what earlier joints hold where it is (mitcad#2199).
mod earlier_joint_kept_tests;
// Exploded views of assemblies (mitcad#2113).
mod explode_tests;
// Exploded views that follow the design, and Measure on them (mitcad#2328).
mod explode_follow_2328_tests;
// Explode views follow bodies moved into an existing component (mitcad#2572).
mod explode_destination_2572_tests;
