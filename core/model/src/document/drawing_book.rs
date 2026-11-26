// SPDX-License-Identifier: MIT
//! The drawing book (mitcad#2448): a print set of drawings that already
//! exist, kept with a main design: drawings of the design itself and of the
//! designs it links (its linked components' files), in the book's order,
//! with a title and an optional cover page.
//!
//! The book keeps references, never copies: each drawing by the file of the
//! design that holds it (its link's path as this design saves it; none for
//! the design itself) and its id there (`D3`), with its name when it was
//! chosen so that a drawing that is gone can still be named. Two designs'
//! drawings of the same name are two entries. Setting the book is one undo
//! step that recomputes nothing.
//!
//! [`Document::drawing_book_sources`] lists what can go into the book: the
//! design's own drawings, then each linked design once (several components
//! or placements linked to one file are one source), read from its file as
//! it is now, read only. [`Document::drawing_book_entries`] checks the book
//! against them: a drawing that is gone, a design that is no longer linked
//! or whose file cannot be read is named with the reason, so that the
//! selection can be corrected before it is printed. Cut sheets are not
//! offered: they print at 1:1 on their own.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::components::{file_label, followed_link};
use super::{DocState, Document, ModelError, invalid};
use crate::drawing::def::{Drawing, DrawingUid};
use crate::kernel::Kernel;

fn is_false(value: &bool) -> bool {
    !*value
}

/// A drawing of the book: the design that holds it and its id there. Read
/// tolerantly from the design file: a field a newer MitCad adds is left
/// out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookDrawing {
    /// The linked design's file as its link saves it (relative to this
    /// design's folder); None: this design's own drawing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub drawing: DrawingUid,
    /// Its name when it was chosen: what a drawing that is gone is called.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

impl BookDrawing {
    /// Whether it is the same drawing of the same design as `other`.
    pub fn same(&self, other: &BookDrawing) -> bool {
        self.source == other.source && self.drawing == other.drawing
    }

    /// Its name as kept, else its id.
    fn label(&self) -> String {
        if self.name.is_empty() {
            self.drawing.to_string()
        } else {
            self.name.clone()
        }
    }
}

/// The drawing book of a design: its title, whether a cover page comes
/// first, and its drawings in the book's order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawingBook {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// A cover page with the title before the first sheet (off by default).
    #[serde(default, skip_serializing_if = "is_false")]
    pub cover: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawings: Vec<BookDrawing>,
}

impl DrawingBook {
    /// Nothing set: the design file leaves it out.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Refuses a drawing listed twice and a design's file left empty.
    pub fn check(&self) -> Result<(), String> {
        for (i, drawing) in self.drawings.iter().enumerate() {
            if drawing
                .source
                .as_deref()
                .is_some_and(|s| s.trim().is_empty())
            {
                return Err(format!(
                    "drawings[{i}]: the design of {} has no file",
                    drawing.label()
                ));
            }
            if self.drawings[..i].iter().any(|d| d.same(drawing)) {
                return Err(format!(
                    "drawings[{i}]: {} is in the drawing book more than once",
                    drawing.label()
                ));
            }
        }
        Ok(())
    }
}

/// A drawing a source offers to the book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDrawing {
    pub uid: DrawingUid,
    pub name: String,
    /// The component whose Drawings folder holds it (the root's name for
    /// the root).
    pub component: String,
    pub sheets: usize,
}

/// A design whose drawings can go into the book: the design itself, or a
/// linked design (each file once).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSource {
    /// The key the book keeps: None for the design itself, else the first
    /// link's path to the file.
    pub source: Option<String>,
    /// Every link path that leads to the file (one per linked component
    /// that has another path to it).
    pub paths: Vec<String>,
    /// The file's name (`part.mitcad`); empty for the design itself.
    pub label: String,
    /// The file the links are followed to.
    pub file: Option<PathBuf>,
    /// The linked components that hold it.
    pub components: Vec<String>,
    /// Why its file is not followed or cannot be read; its drawings are
    /// then unknown.
    pub problem: Option<String>,
    pub drawings: Vec<SourceDrawing>,
}

impl BookSource {
    /// Whether a book's `source` names this design.
    pub fn holds(&self, source: Option<&str>) -> bool {
        match source {
            None => self.source.is_none(),
            Some(path) => self.source.is_some() && self.paths.iter().any(|p| p == path),
        }
    }
}

/// A drawing of the book checked against the sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookEntry {
    pub drawing: BookDrawing,
    /// The source that holds it (an index into the sources), when its
    /// design is still linked.
    pub source: Option<usize>,
    /// Its name now, or as kept when it is gone.
    pub name: String,
    /// Its sheets now (0 when it is gone).
    pub sheets: usize,
    /// Why it cannot be printed: it is gone, its design is no longer
    /// linked or cannot be read.
    pub problem: Option<String>,
}

/// The drawings of a design that can go into a book: not cut sheets, of
/// components it has.
fn offered(state: &DocState) -> Vec<SourceDrawing> {
    let assembly = &state.assembly;
    state
        .drawings
        .iter()
        .filter(|d| d.cut_sheet.is_none() && assembly.exists(d.component))
        .map(|d: &Drawing| SourceDrawing {
            uid: d.uid,
            name: d.name.clone(),
            component: assembly.name(d.component),
            sheets: d.sheets.len(),
        })
        .collect()
}

impl<K: Kernel> Document<K> {
    /// The design's drawing book (empty when it has none).
    pub fn drawing_book(&self) -> &DrawingBook {
        &self.state.drawing_book
    }

    /// Sets the drawing book as one undo step, `Edit Drawing Book`, that
    /// recomputes nothing; the title is trimmed. Whether it changed: a book
    /// as it was is no undo step. The drawings are not looked up here (a
    /// linked design's file may be unreadable for now); the entries
    /// ([`Document::drawing_book_entries`]) say which are gone.
    pub fn set_drawing_book(&mut self, mut book: DrawingBook) -> Result<bool, ModelError> {
        book.title = book.title.trim().to_owned();
        for drawing in &mut book.drawings {
            if let Some(source) = &mut drawing.source {
                *source = source.trim().to_owned();
            }
            drawing.name = drawing.name.trim().to_owned();
        }
        book.check().map_err(invalid)?;
        if book == self.state.drawing_book {
            return Ok(false);
        }
        self.apply_with(|state| {
            state.drawing_book = book;
            Ok(("Edit Drawing Book".to_owned(), (), false))
        })?;
        Ok(true)
    }

    /// What can go into the drawing book: the design's own drawings first,
    /// then each linked design's (not library parts) once, in the order of
    /// the components that link them, read from their files as they are
    /// now; several components or placements linked to one file are one
    /// source. A file that is not followed (outside the project, mitcad#929)
    /// or cannot be read is a source with its problem and no drawings.
    /// `base` is the design file's folder; left out, the folder the design
    /// was read from.
    pub fn drawing_book_sources(&self, base: Option<&Path>) -> Vec<BookSource> {
        let mut sources = vec![BookSource {
            source: None,
            paths: Vec::new(),
            label: String::new(),
            file: None,
            components: Vec::new(),
            problem: None,
            drawings: offered(&self.state),
        }];
        let base = base
            .map(Path::to_path_buf)
            .or_else(|| self.source_directory());
        for link in self.link_states(base.as_deref()) {
            let followed = followed_link(&link.path, base.as_deref()).ok();
            let known = sources.iter_mut().skip(1).find(|s| {
                s.paths.contains(&link.path) || (followed.is_some() && s.file == followed)
            });
            if let Some(source) = known {
                if !source.paths.contains(&link.path) {
                    source.paths.push(link.path.clone());
                }
                if !source.components.contains(&link.name) {
                    source.components.push(link.name.clone());
                }
                continue;
            }
            let label = file_label(&link.path);
            let mut source = BookSource {
                source: Some(link.path.clone()),
                paths: vec![link.path.clone()],
                label: label.clone(),
                file: followed.clone(),
                components: vec![link.name.clone()],
                problem: None,
                drawings: Vec::new(),
            };
            match (&followed, &link.problem) {
                (_, Some(problem)) => source.problem = Some(problem.clone()),
                (None, None) => source.problem = Some(format!("cannot follow the link to {label}")),
                (Some(file), None) => match std::fs::read_to_string(file)
                    .map_err(|e| format!("cannot read {label} ({e})"))
                    .and_then(|text| {
                        crate::file::load_state(&text)
                            .map_err(|e| format!("cannot read {label}: {e}"))
                    }) {
                    Ok((state, _)) => source.drawings = offered(&state),
                    Err(problem) => source.problem = Some(problem),
                },
            }
            sources.push(source);
        }
        sources
    }

    /// The book's drawings in its order, each checked against `sources`
    /// ([`Document::drawing_book_sources`]): its name and sheets now, or
    /// why it cannot be printed (its drawing is gone, its design is no
    /// longer linked or its file cannot be read).
    pub fn drawing_book_entries(&self, sources: &[BookSource]) -> Vec<BookEntry> {
        self.state
            .drawing_book
            .drawings
            .iter()
            .map(|drawing| {
                let mut entry = BookEntry {
                    drawing: drawing.clone(),
                    source: None,
                    name: drawing.label(),
                    sheets: 0,
                    problem: None,
                };
                let Some(at) = sources
                    .iter()
                    .position(|s| s.holds(drawing.source.as_deref()))
                else {
                    let label = file_label(drawing.source.as_deref().unwrap_or_default());
                    entry.problem = Some(format!(
                        "{} of {label}: {label} is no longer linked in this design",
                        entry.name
                    ));
                    return entry;
                };
                entry.source = Some(at);
                let source = &sources[at];
                let design = if source.source.is_none() {
                    "this design".to_owned()
                } else {
                    source.label.clone()
                };
                if let Some(problem) = &source.problem {
                    entry.problem = Some(format!("{} of {design}: {problem}", entry.name));
                    return entry;
                }
                match source.drawings.iter().find(|d| d.uid == drawing.drawing) {
                    Some(found) => {
                        entry.name = found.name.clone();
                        entry.sheets = found.sheets;
                    }
                    None => {
                        entry.problem = Some(format!("{} is no longer in {design}", entry.name));
                    }
                }
                entry
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawing(source: Option<&str>, uid: u32, name: &str) -> BookDrawing {
        BookDrawing {
            source: source.map(str::to_owned),
            drawing: DrawingUid(uid),
            name: name.to_owned(),
        }
    }

    #[test]
    fn a_drawing_is_in_a_book_once_and_same_names_of_two_designs_are_two() {
        let mut book = DrawingBook {
            title: "Project".to_owned(),
            cover: false,
            drawings: vec![
                drawing(Some("a.mitcad"), 1, "Drawing1"),
                drawing(Some("b.mitcad"), 1, "Drawing1"),
                drawing(None, 1, "Drawing1"),
            ],
        };
        assert_eq!(book.check(), Ok(()));
        book.drawings.push(drawing(Some("a.mitcad"), 1, "Other"));
        let error = book.check().unwrap_err();
        assert!(error.contains("more than once"), "{error}");
        book.drawings.pop();
        book.drawings.push(drawing(Some(" "), 2, "Drawing2"));
        assert!(book.check().unwrap_err().contains("has no file"));
    }

    #[test]
    fn the_file_form_leaves_out_what_is_not_set() {
        let book = DrawingBook {
            title: String::new(),
            cover: false,
            drawings: vec![drawing(None, 3, "")],
        };
        let text = serde_json::to_value(&book).unwrap();
        assert_eq!(text, serde_json::json!({"drawings": [{"drawing": "D3"}]}));
        let back: DrawingBook = serde_json::from_value(text).unwrap();
        assert_eq!(back, book);
        assert!(DrawingBook::default().is_empty());
        // A field of a newer MitCad is left out.
        let newer: DrawingBook =
            serde_json::from_value(serde_json::json!({"title": "A", "pages": 1})).unwrap();
        assert_eq!(newer.title, "A");
    }
}
