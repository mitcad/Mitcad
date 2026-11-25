// SPDX-License-Identifier: MIT
//! The tutorials' bridge `query` (mitcad#2371) for the application: what a
//! tutorial's page reads of the design of its window
//! (`mitcad_model_api::tutorial_query`).

use mitcad_model_api::ApiError;

use crate::Document;

pub fn tutorial_query(
    document: &Document,
    request: &str,
    open_sketch: &str,
    design: &str,
) -> Result<String, ApiError> {
    mitcad_model_api::tutorial_query(&document.0, request, open_sketch, design)
}
