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
// The middle of a free stretch of a side (mitcad#1401).
mod side_gap_tests;
// A length to a picked face plus an offset (mitcad#1630).
mod to_face_tests;
// Removing a profile others are placed on (mitcad#1811).
mod remove_placed_on_tests;
// Side snaps in line with the frame's other profiles (mitcad#1886).
mod side_lineup_tests;
// Moving what a STEP import brought into a frame (mitcad#1647).
mod move_imported_tests;
// Ends that meet and their shared miter (mitcad#1940).
mod end_meets_tests;
// Profiles along sketch lines (mitcad#1965).
mod lines_tests;
// A part started on another body's face along its normal (mitcad#2000).
mod face_normal_tests;
// A profile centred across a side with two slots (mitcad#2002).
mod side_middle_tests;
// A BUILD part's Kind (mitcad#1980) and BOM Structure (mitcad#1985).
mod bom_kind_tests;
// A free part of a flexible occurrence joined to a removed part (mitcad#2025).
mod flexible_free_part_remove_tests;
// A part's automatic name follows its designation; Press Pull on its free
// end edits its length (mitcad#2030).
mod auto_name_tests;
// Offset and Rotate at any angle on a plane or a face, edited with
// Placement (mitcad#2046).
mod ground_offset_tests;
// Drags leave placed parts at their Offset, a carriage's travel limits and
// joints named after the parts they join (mitcad#2088).
mod drag_travel_tests;
// Joint conditions of profiles along sketch lines (mitcad#2200).
mod joint_conditions_tests;
// Profiles in parallel, long faces in contact (mitcad#2201).
mod parallel_tests;
// A BUILD part of a source not for print services (mitcad#2214).
mod print_service_build_tests;
// Group identical parts in the Bill of Materials' views (mitcad#2215).
mod bom_group_view_tests;
// End caps of tubes and profiles (mitcad#2243).
mod end_cap_tests;
// End caps of closed sections of any shape (mitcad#2270).
mod end_cap_outline_tests;
// A part on a plane or a face follows it when it moves (mitcad#2286).
mod plane_follow_tests;
// Two carriages joined by a Rigid joint or a Rigid Group move together
// (mitcad#2274).
mod carriages_together_tests;
// Weld symbols proposed from BUILD's joints on a drawing (mitcad#2350).
mod weld_proposal_tests;
