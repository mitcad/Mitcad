// SPDX-License-Identifier: MIT
//! A FreeCAD document's XML (`Document.xml`, `GuiDocument.xml`) as the
//! import reads it: the objects and their properties, the sketches'
//! geometry and constraints, the spreadsheets' cells and the expressions
//! of both, and the view data. Each is read or refused within the budget
//! and without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_freecad::expression::Expr;
use mitcad_freecad::spreadsheet::{Content, Sheet, is_sheet};
use mitcad_freecad::{Document, GuiDocument, Sketch, Value};
use mitcad_fuzz::within_budget;

fuzz_target!(|data: &[u8]| {
    within_budget("freecad_document", data, || {
        let text = String::from_utf8_lossy(data);
        if let Ok(document) = Document::parse(&text) {
            for object in &document.objects {
                let _ = Sketch::of(object);
                let _ = object.placement();
                if is_sheet(&object.type_name) {
                    for cell in &Sheet::of(object).cells {
                        if let Content::Expression(expression) = cell.content() {
                            let _ = Expr::parse(expression);
                        }
                    }
                }
                for property in &object.properties {
                    if let Value::Expressions(list) = &property.value {
                        for expression in list {
                            let _ = Expr::parse(&expression.expression);
                        }
                    }
                }
            }
        }
        let _ = GuiDocument::parse(&text);
    });
});
