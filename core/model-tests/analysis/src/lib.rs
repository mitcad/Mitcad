// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Section
//! analyses, Measure, plane geometry, measurement objects and the
//! annotations grown from them.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod analysis_tests;
mod annotation_tests;
mod image_reference_tests;
mod measure_tests;
mod measurement_tests;
mod plane_geometry_tests;
// Material and process notes (mitcad#1049).
mod body_note_tests;
// Physical Properties' boxes, counts and component scopes (mitcad#1095).
mod physical_properties_tests;
// Comments pinned to the model for an AI agent (mitcad#1119).
mod comment_tests;
// Section Analysis through a pick: a round face, a circular or straight
// edge (mitcad#1769).
mod section_pick_tests;
// Draft Analysis kept in the document and its query (mitcad#2166).
mod draft_analysis_tests;
// Point clouds and Point Cloud Deviation (mitcad#2240).
mod point_cloud_tests;
// Decals: images put on faces, following them (mitcad#2244).
mod decal_tests;
// Point cloud tools: align, filter, reconstruct (mitcad#2516).
mod point_cloud_tools_tests;
// Minimum Radius kept in the document and its query (mitcad#2518).
mod min_radius_tests;
// Minimum Distance kept in the document and its query (mitcad#2519).
mod minimum_distance_tests;
// Surface quality analyses: Zebra, Curvature Map, Isocurve Analysis and
// Environment Map kept in the document and their query (mitcad#2517).
mod surface_analysis_tests;
// Curvature combs of sketch curves and edges (mitcad#2520).
mod curvature_comb_tests;
