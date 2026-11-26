// SPDX-License-Identifier: MIT
//! The JSON Schema of the drawing book's command (mitcad#2448):
//! `set_drawing_book` of `core/model-api/src/drawing_book.rs`, written from
//! its fields as [`super`] writes the others. The drift test
//! `core/model-tests/api/src/mcp_schema_tests.rs` checks it against the
//! API.

use serde_json::{Value, json};

use super::{flag, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &["set_drawing_book"];

/// A drawing command's schema as [`super`] writes its own.
pub(super) fn spec(name: &str) -> Option<Value> {
    let spec = match name {
        "set_drawing_book" => json!({
            "description": "Sets the design's drawing book: existing drawings of the design and of the designs it links, in the book's order, printed as one job with an optional cover page. Each field given replaces the book's. The drawings are references, checked by the drawing_book query (a drawing that is gone, a design no longer linked). One undo step that recomputes nothing. Result: changed, drawings (the count).",
            "properties": {
                "title": text("The book's title, on its cover page"),
                "cover": flag(false, "A cover page with the title before the first sheet"),
                "drawings": {
                    "type": "array",
                    "description": "The book's drawings in its order; a drawing once",
                    "items": {
                        "type": "object",
                        "properties": {
                            "source": {"type": ["string", "null"], "description": "The linked design's file as its link saves it (the drawing_book query's sources' source); left out or null for the design's own drawing"},
                            "drawing": {"type": "string", "pattern": "^D[0-9]+$", "description": "The drawing's id in that design (D3)"},
                            "name": text("Its name, to name it if it is gone; the design's own drawing's when left out")
                        },
                        "required": ["drawing"],
                        "additionalProperties": false
                    }
                }
            },
            "required": [],
            "examples": [
                {"cmd": "set_drawing_book", "title": "Project", "cover": true,
                 "drawings": [{"drawing": "D1"}, {"source": "part.mitcad", "drawing": "D2", "name": "Drawing1"}]}
            ]
        }),
        _ => return None,
    };
    Some(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_here_has_its_spec() {
        for name in COMMANDS {
            assert!(spec(name).is_some(), "{name}");
        }
    }
}
