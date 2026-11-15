// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): BUILD:
//! timber, wall bearings and splices, fasteners, profiled boards, the cut
//! list, bound lengths, the easy tools, library sources and members along
//! an edge.
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod bound_tests;
mod cut_list_tests;
mod easy_tests;
mod edge_tests;
mod fastener_tests;
mod profiled_tests;
mod roof_fastener_tests;
mod source_tests;
mod timber_tests;
mod wall_bearing_tests;
mod wall_splice_tests;
// Convert to Features: a placed part replaced by its source's features
// (mitcad#1815).
mod convert_tests;
