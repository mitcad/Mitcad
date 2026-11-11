// SPDX-License-Identifier: MIT
//! Tests of the Mitcad model through its JSON api (mitcad#777): Drawings:
//! the sheet, stored drawings, templates and revisions, tables, the .dwg
//! export and cut sheets (mitcad#853).
//!
//! A test crate of its own, so that a test added here recompiles only
//! this crate (core/model-tests/README.md).
#![cfg(test)]

mod drawing_tests;
mod dwg_tests;
mod stored_tests;
mod table_tests;
mod template_tests;
// Cut sheets (mitcad#853).
mod cut_sheet_tests;
// Export Face as DXF (mitcad#1049).
mod face_dxf_tests;
// The DXF exports' length unit (mitcad#1408).
mod dxf_units_tests;
// Custom sheet sizes, title block images and the title block editor
// (mitcad#1625).
mod title_block_tests;
