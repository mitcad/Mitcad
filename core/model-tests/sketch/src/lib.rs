// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): The general
//! sketch: entities, constraints, dimensions, patterns, offsets, text, the
//! sketch's axes, inactive constraints, one solve per edit, and copy and paste.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod sketch_axes_tests;
mod sketch_clipboard_tests;
mod sketch_closed_spline_783_tests;
mod sketch_inactive_tests;
mod sketch_p34_tests;
mod sketch_points_tests;
mod sketch_solve_once_tests;
mod sketch_tests;
mod sketch_text_473_tests;
// Redefine Sketch Plane (mitcad#787).
mod sketch_plane_tests;
// Insert Point on a spline (mitcad#836).
mod sketch_spline_point_836_tests;
// A selection dragged together (mitcad#837).
mod sketch_drag_selection_837_tests;
// Rounded rectangles and Sketch Fillet on several corners (mitcad#867).
mod sketch_rounded_rectangle_867_tests;
// Scaling a sketch, and the first dimension that scales it (mitcad#860).
mod sketch_scale_860_tests;
// Symmetry of circles and arcs whose radii are fixed (mitcad#865).
mod sketch_symmetry_865_tests;
// Mirror over a construction diagonal and about the sketch axes (mitcad#876).
mod sketch_mirror_876_tests;
// Offsets' tangent corners where their curves touch (mitcad#890).
mod sketch_offset_tangent_corners_890_tests;
// Constraints a drawing tool adds automatically, only when safe (mitcad#1024).
mod sketch_auto_1024_tests;
// Geometry tied to a linked projection keeps its side when the source moves
// (mitcad#1047).
mod sketch_projection_side_1047_tests;
// A new sketch on the XZ origin plane is seen from the front; existing
// sketches keep their frames (mitcad#1060).
mod sketch_xz_front_1060_tests;
// Sketch Fillet at corners of circular arcs (mitcad#1192).
mod sketch_fillet_arcs_1192_tests;
// Offsets leave out the curves that vanish at their distance and trim the
// overlaps on the source's side (mitcad#1234).
mod sketch_offset_vanishing_1234_tests;
// Offsets that fall into several outlines, lines that run on along each
// other across a closed notch, and offsets of offsets made again with
// them (mitcad#1244).
mod sketch_offset_outlines_1244_tests;
// Offsets of open chains that fall into parts, arcs on one circle across a
// closed notch, tapered waists whose walls' offsets cross, and lobes too
// small for the distance (mitcad#1246).
mod sketch_offset_open_parts_1246_tests;
