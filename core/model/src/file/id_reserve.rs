// SPDX-License-Identifier: MIT
//! The reserve of id numbers (mitcad#944): a design file uses no id number
//! in the top [`ID_RESERVE`] of its type, so the ids a session hands out by
//! counting on from the largest one read always exist and counting on never
//! overflows, in Debug and release builds alike.

use std::fmt::Display;

use super::FileError;
use crate::document::DocState;
use crate::features::FeatureDef;
use crate::ids::{ID_RESERVE, LARGEST_U32_ID, LARGEST_U64_ID, leaves_reserve};

/// Refuses a design one of whose counted ids is in the reserve: the ids of
/// components, occurrences and features, occurrences' numbers, sketch
/// entities and constraints, library sources, drawings and their views,
/// annotations, cut-sheet parts and export counts.
pub(super) fn check(state: &DocState) -> Result<(), FileError> {
    let assembly = &state.assembly;
    for (i, c) in assembly.components.iter().enumerate() {
        if !leaves_reserve(c.uid.0) {
            return Err(refused(
                &format!("components[{i}]"),
                "component id",
                c.uid,
                format!("C{LARGEST_U32_ID}"),
            ));
        }
    }
    for (i, o) in assembly.occurrences.iter().enumerate() {
        let place = format!("occurrences[{i}]");
        if !leaves_reserve(o.uid.0) {
            return Err(refused(
                &place,
                "occurrence id",
                o.uid,
                format!("O{LARGEST_U32_ID}"),
            ));
        }
        // Placing the component again numbers the new occurrence one past
        // its largest.
        if !leaves_reserve(o.number) {
            return Err(refused(
                &place,
                "occurrence number",
                o.number,
                LARGEST_U32_ID,
            ));
        }
    }
    for (i, f) in state.features.iter().enumerate() {
        let place = format!("features[{i}] ({})", f.name);
        if !leaves_reserve(f.uid.0) {
            return Err(refused(
                &place,
                "feature id",
                f.uid,
                format!("F{LARGEST_U64_ID}"),
            ));
        }
        if let FeatureDef::Sketch(sketch) = &f.def {
            // Points, curves and texts share the numbers; so do
            // constraints, dimensions, patterns and offsets.
            let entities = sketch
                .entities
                .iter()
                .map(|e| (e.id.0, if e.is_point() { "p" } else { "c" }))
                .chain(sketch.texts.iter().map(|t| (t.id.0, "t")));
            let constraints = sketch
                .constraints
                .iter()
                .map(|c| c.id.0)
                .chain(sketch.dimensions.iter().map(|d| d.id.0))
                .chain(sketch.patterns.iter().map(|p| p.id.0))
                .chain(sketch.offsets.iter().map(|o| o.id.0));
            for (id, prefix) in entities {
                if !leaves_reserve(id) {
                    return Err(refused(
                        &place,
                        "sketch entity id",
                        format!("{prefix}{id}"),
                        format!("{prefix}{LARGEST_U32_ID}"),
                    ));
                }
            }
            for id in constraints {
                if !leaves_reserve(id) {
                    return Err(refused(
                        &place,
                        "constraint id",
                        format!("k{id}"),
                        format!("k{LARGEST_U32_ID}"),
                    ));
                }
            }
        }
    }
    for (i, source) in state.library_sources.iter().enumerate() {
        if !leaves_reserve(source.id.0) {
            return Err(refused(
                &format!("library_sources[{i}]"),
                "library source id",
                source.id,
                format!("S{LARGEST_U32_ID}"),
            ));
        }
    }
    for (i, drawing) in state.drawings.iter().enumerate() {
        let place = format!("drawings[{i}] ({})", drawing.name);
        if !leaves_reserve(drawing.uid.0) {
            return Err(refused(
                &place,
                "drawing id",
                drawing.uid,
                format!("D{LARGEST_U32_ID}"),
            ));
        }
        let views = drawing.sheets.iter().flat_map(|s| &s.views);
        for id in views.map(|v| v.id) {
            if !leaves_reserve(id) {
                return Err(refused(&place, "view id", id, LARGEST_U32_ID));
            }
        }
        for id in drawing.annotation_ids() {
            if !leaves_reserve(id) {
                return Err(refused(&place, "annotation id", id, LARGEST_U32_ID));
            }
        }
        let parts = drawing.cut_sheet.iter().flat_map(|c| &c.parts);
        for id in parts.map(|p| p.id) {
            if !leaves_reserve(id) {
                return Err(refused(&place, "cut-sheet part id", id, LARGEST_U32_ID));
            }
        }
        if !leaves_reserve(drawing.exported) {
            return Err(refused(
                &place,
                "export count",
                drawing.exported,
                LARGEST_U32_ID,
            ));
        }
    }
    Ok(())
}

/// The refusal of an id in the reserve, in the words of a malformed id's
/// (`invalid component id 'C…'`, mitcad#929).
fn refused(place: &str, what: &str, id: impl Display, largest: impl Display) -> FileError {
    FileError::Invalid(format!(
        "{place}: invalid {what} '{id}' (at most {largest}: the top {ID_RESERVE} numbers \
         are kept for the ids a session hands out)"
    ))
}
