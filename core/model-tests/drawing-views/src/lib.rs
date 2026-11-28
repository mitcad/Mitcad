// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Drawing
//! views: features, following the model, picks and annotations on views.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod drawing_annotation_tests;
mod feature_tests;
mod follow_up_tests;
mod model_follow_tests;
mod pick_tests;
// Dimension spacing (mitcad#1173).
mod spacing_tests;
// Dimensions whose picks choose their type (mitcad#1272).
mod chosen_dimension_tests;
// The model's items in views, Model Items and kept ones (mitcad#1406).
mod model_items_tests;
// Widths and patterns of edges and bodies in a view (mitcad#1546).
mod line_style_tests;
// A center pattern's bolt circle and center lines as picks (mitcad#1693).
mod bolt_circle_tests;
// A radius on a fillet seen from the side (mitcad#1694).
mod fillet_radius_tests;
// Dimensions to a curved face's outline and its extreme points, and a
// section's label dragged (mitcad#1909).
mod silhouette_tests;
// A drawing's dimensioning standard, ISO or ASME (mitcad#2047).
mod standard_tests;
// A drawing's own hole callouts (mitcad#2054).
mod hole_callout_tests;
// A base view of a sheet-metal body's Flat Pattern (mitcad#2280).
mod flat_pattern_view_tests;
// Weld symbols by ISO 2553 and AWS A2.4 (mitcad#2350).
mod weld_symbol_tests;
// A section's extent: of the whole plane or limited to its line
// (mitcad#2479).
mod section_extent_tests;
// ASME's radius and diameter callouts on a shoulder and short vertical
// dimensions on an elbow (mitcad#2621).
mod asme_leader_tests;
