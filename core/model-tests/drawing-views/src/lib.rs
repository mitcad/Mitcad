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
