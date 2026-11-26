// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Files and
//! exchange: import and export, the design file, design comparison,
//! Reorient Design, import notes and hostile sizes in design files.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod diff_tests;
mod exchange_tests;
mod file_tests;
mod import_notes_tests;
mod limits_tests;
mod reorient_tests;
mod tables_tests;
// The design's summary and statistics (mitcad#1088).
mod design_properties_tests;
// Top-level fields a newer MitCad wrote (mitcad#1132).
mod unknown_fields_tests;
// Animated USDZ export (mitcad#1597).
mod usdz_animation_tests;
// Occurrence placements kept rigid and straightened on opening (mitcad#1664).
mod occurrence_placement_tests;
// The preview a design file carries for file managers (mitcad#1539).
mod design_preview_tests;
// The design packed into a zip for a report (mitcad#1795).
mod design_package_tests;
// glTF files as mesh bodies and components (mitcad#1826).
mod gltf_tests;
// .serp files as bodies by layer, blocks as components (mitcad#1870).
mod serp_tests;
// A sketch's points in its DXF (mitcad#1963).
mod sketch_dxf_points_1963_tests;
// The reporter's arc whose Symmetry partly repeats the others: information,
// not a redundancy warning (mitcad#1992).
mod sketch_partly_redundant_reported_1992_tests;
// import_file stopped inside its read by the memory guard or the user's
// cancel, in the import's own words, the design unchanged (mitcad#2032).
mod import_stopped_2032_tests;
// The reporter's design of GitHub mitcad/Mitcad#470 and #471: its emptied
// sketch patterns go with the next edit of the sketch (mitcad#2058).
mod sketch_pattern_last_copy_reported_2058_tests;
// 3MF files as mesh bodies, a part placed twice as a component (mitcad#2156).
mod threemf_import_2156_tests;
// The reporter's designs of GitHub mitcad/Mitcad#570: an offset arc's free
// ends stay where they are dragged (mitcad#2277).
mod sketch_offset_free_ends_reported_2277_tests;
// The reporter's design of GitHub mitcad/Mitcad#588: a body split by a face of
// another component, linked where both are seen (mitcad#2299).
mod split_body_other_component_reported_2299_tests;
// URDF robot descriptions for ROS 2 (mitcad#2100).
mod urdf_tests;
// Faces coloured on their own in STEP and IGES files as face appearances,
// and face appearances written as the faces' colours (mitcad#2313).
mod face_colors_2313_tests;
// An explode view of a sub-component and the root's own bodies an explode
// moves, as an animated USDZ (mitcad#2328).
mod usdz_explode_2328_tests;
// Faces coloured on their own written on their triangles in 3MF files
// (mitcad#2385).
mod threemf_face_colors_2385_tests;
// Bodies coloured as a whole in STEP files as the bodies' appearances, in the
// colouring pattern of the reporter's circuit board of GitHub
// mitcad/Mitcad#677 (mitcad#2423).
mod body_colors_2423_tests;
