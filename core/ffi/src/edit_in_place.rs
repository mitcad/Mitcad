// SPDX-License-Identifier: MIT
//! Edit in Place (mitcad#2321) for the application: the start and the end
//! of a linked design edited inside its holder are functions of both
//! documents (`mitcad_model_api::begin_edit_in_place` and the others).

use mitcad_model_api::ApiError;

use crate::Document;

pub fn begin_edit_in_place(
    linked: &mut Document,
    holder: &Document,
    request: &str,
) -> Result<String, ApiError> {
    mitcad_model_api::begin_edit_in_place(&mut linked.0, &holder.0, request)
}

pub fn finish_edit_in_place(
    holder: &mut Document,
    linked: &mut Document,
    request: &str,
) -> Result<String, ApiError> {
    mitcad_model_api::finish_edit_in_place(&mut holder.0, &mut linked.0, request)
}

pub fn linked_saved_in_place(holder: &mut Document, request: &str) -> Result<String, ApiError> {
    mitcad_model_api::linked_saved_in_place(&mut holder.0, request)
}
