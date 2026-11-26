// SPDX-License-Identifier: MIT
//! The drawing book (mitcad#2448, `mitcad_model::DrawingBook`; see
//! `commands.md`, "The drawing book"): the command `set_drawing_book`, the
//! query `drawing_book` (the book checked against what the design and its
//! linked designs hold now, and what can go into it) and the query
//! `drawing_book_sheets` (every sheet of the book's drawings written as SVG
//! files in the book's order, for File › Drawing Book's Print…).
//!
//! The sheets are drawn as `export_drawing` draws them: the design's own
//! drawings on a copy of the design ([`Document::fork`]), a linked design's
//! on the design read from its file as it is now (read only, evaluated on a
//! kernel of its own), so that edits saved to a source drawing are in the
//! next print and no source design changes.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use super::dispatch::named_enum;
use super::{ApiError, DocumentApi};
use mitcad_model::drawing::def::DrawingUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::{BookDrawing, BookEntry, BookSource, Document};

/// A drawing of the book in a command.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BookDrawingInput {
    /// The linked design's file as its link saves it (the `drawing_book`
    /// query's sources' `source`); left out or null: the design's own.
    #[serde(default)]
    source: Option<String>,
    /// The drawing's id in that design (`D3`).
    drawing: String,
    /// Its name; the design's own drawing's is looked up when left out.
    #[serde(default)]
    name: Option<String>,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum DrawingBookCommand {
        // Each field given replaces the book's.
        "set_drawing_book" => Set {
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            cover: Option<bool>,
            #[serde(default)]
            drawings: Option<Vec<BookDrawingInput>>,
        },
    }
}

/// `drawing_book_sheets`: the folder the SVG files go into, the design
/// file's folder, and the title of the design's own drawings where they
/// have none (the design's file name, as their export gets it).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SheetsQuery {
    folder: String,
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

pub(crate) fn run<K: Kernel>(
    doc: &mut Document<K>,
    command: DrawingBookCommand,
) -> Result<Value, ApiError> {
    match command {
        DrawingBookCommand::Set {
            title,
            cover,
            drawings,
        } => {
            let mut book = doc.drawing_book().clone();
            if let Some(title) = title {
                book.title = title;
            }
            if let Some(cover) = cover {
                book.cover = cover;
            }
            if let Some(drawings) = drawings {
                book.drawings = drawings
                    .into_iter()
                    .map(|input| book_drawing(doc, input))
                    .collect::<Result<_, _>>()?;
            }
            let changed = doc.set_drawing_book(book)?;
            Ok(json!({"changed": changed, "drawings": doc.drawing_book().drawings.len()}))
        }
    }
}

fn book_drawing<K: Kernel>(
    doc: &Document<K>,
    input: BookDrawingInput,
) -> Result<BookDrawing, ApiError> {
    let drawing: DrawingUid = input.drawing.parse().map_err(ApiError)?;
    let source = input.source.filter(|s| !s.is_empty());
    let name = match input.name {
        Some(name) => name,
        None if source.is_none() => doc
            .drawings()
            .iter()
            .find(|d| d.uid == drawing)
            .map(|d| d.name.clone())
            .unwrap_or_default(),
        None => String::new(),
    };
    Ok(BookDrawing {
        source,
        drawing,
        name,
    })
}

/// The name a source goes by in the answers: its file's name, empty for
/// the design itself.
fn design_of(sources: &[BookSource], entry: &BookEntry) -> String {
    match entry.source {
        Some(at) => sources[at].label.clone(),
        None => entry
            .drawing
            .source
            .as_deref()
            .map(|s| {
                Path::new(s)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(s)
                    .to_owned()
            })
            .unwrap_or_default(),
    }
}

fn entry_json(sources: &[BookSource], entry: &BookEntry) -> Value {
    let mut value = json!({
        "source": entry.drawing.source,
        "design": design_of(sources, entry),
        "drawing": entry.drawing.drawing,
        "name": entry.name,
        "sheets": entry.sheets,
        // The source in `sources` that holds it; null when its design is
        // no longer linked.
        "source_index": entry.source,
    });
    if let Some(problem) = &entry.problem {
        value["problem"] = json!(problem);
    }
    value
}

fn source_json(source: &BookSource) -> Value {
    let drawings: Vec<Value> = source
        .drawings
        .iter()
        .map(|d| {
            json!({"drawing": d.uid, "name": d.name, "component_name": d.component,
                   "sheets": d.sheets})
        })
        .collect();
    let mut value = json!({
        "source": source.source,
        "design": source.label,
        "file": source.file.as_ref().map(|f| f.display().to_string()),
        "components": source.components,
        "drawings": drawings,
    });
    if let Some(problem) = &source.problem {
        value["problem"] = json!(problem);
    }
    value
}

/// The `drawing_book` query: the book as kept, its drawings checked
/// against what the design and its linked designs hold now, the problems
/// that keep it from printing, and the sources to choose from.
pub(crate) fn book_json<K: Kernel>(doc: &Document<K>, base: Option<&str>) -> Value {
    let base = base.map(PathBuf::from);
    let sources = doc.drawing_book_sources(base.as_deref());
    let entries = doc.drawing_book_entries(&sources);
    let book = doc.drawing_book();
    let problems: Vec<&String> = entries.iter().filter_map(|e| e.problem.as_ref()).collect();
    json!({
        "title": book.title,
        "cover": book.cover,
        "drawings": entries.iter().map(|e| entry_json(&sources, e)).collect::<Vec<_>>(),
        "problems": problems,
        "sources": sources.iter().map(source_json).collect::<Vec<_>>(),
    })
}

/// A design to draw sheets of: the design itself (a copy) or a linked one
/// read from its file.
fn source_document<K: Kernel>(
    doc: &Document<K>,
    source: &BookSource,
) -> Result<Document<K>, ApiError> {
    let unforked = || {
        ApiError("this design's geometry kernel cannot draw the drawing book's sheets".to_owned())
    };
    let Some(file) = &source.file else {
        let mut copy = doc.fork().ok_or_else(unforked)?;
        copy.set_monitor(doc.monitor().cloned());
        return Ok(copy);
    };
    let text = std::fs::read_to_string(file)
        .map_err(|e| ApiError(format!("cannot read {} ({e})", source.label)))?;
    let kernel = doc.kernel().fork().ok_or_else(unforked)?;
    let mut linked = Document::from_json_at(&text, file, kernel)
        .map_err(|e| ApiError(format!("cannot read {}: {e}", source.label)))?;
    linked.set_monitor(doc.monitor().cloned());
    linked.try_recompute()?;
    Ok(linked)
}

/// The `drawing_book_sheets` query: every sheet of the book's drawings
/// written as an SVG file into `folder`, in the book's order (a drawing's
/// sheets in their order), each linked design read once. Refused while the
/// book is empty or names a drawing that cannot be printed, with the
/// reasons. Changes no design.
pub(crate) fn sheets_json<K: Kernel>(
    doc: &Document<K>,
    query: &SheetsQuery,
) -> Result<Value, ApiError> {
    let base = query.base.as_deref().map(PathBuf::from);
    let sources = doc.drawing_book_sources(base.as_deref());
    let entries = doc.drawing_book_entries(&sources);
    if entries.is_empty() {
        return Err(ApiError(
            "the drawing book has no drawing; choose its drawings first".to_owned(),
        ));
    }
    let problems: Vec<&str> = entries
        .iter()
        .filter_map(|e| e.problem.as_deref())
        .collect();
    if !problems.is_empty() {
        return Err(ApiError(format!(
            "the drawing book cannot be printed: {}",
            problems.join("; ")
        )));
    }
    let folder = Path::new(&query.folder);
    std::fs::create_dir_all(folder)
        .map_err(|e| ApiError(format!("cannot make the folder {}: {e}", query.folder)))?;
    let mut designs: HashMap<usize, Document<K>> = HashMap::new();
    let mut files = Vec::new();
    let mut drawings = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let at = entry
            .source
            .ok_or_else(|| ApiError(format!("{} has no design to draw it from", entry.name)))?;
        let source = &sources[at];
        let design = match designs.entry(at) {
            Entry::Occupied(read) => read.into_mut(),
            Entry::Vacant(unread) => unread.insert(source_document(doc, source)?),
        };
        // The design's file name as the title of a drawing without its own,
        // as its export gets it.
        let title = match &source.file {
            Some(file) => file.file_stem().and_then(|s| s.to_str()).map(str::to_owned),
            None => query.title.clone(),
        };
        let path = folder.join(format!("book-{}.svg", i + 1));
        let mut export = json!({"cmd": "export_drawing", "drawing": entry.drawing.drawing,
                                "path": path.to_string_lossy()});
        if let Some(title) = title.filter(|t| !t.is_empty()) {
            export["title"] = json!(title);
        }
        let answer = design
            .command(&export.to_string())
            .map_err(|e| ApiError(format!("cannot draw {}: {e}", entry.name)))?;
        let answer: Value = serde_json::from_str(&answer)
            .map_err(|e| ApiError(format!("cannot draw {}: {e}", entry.name)))?;
        let written: Vec<String> = answer["files"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .filter(|list: &Vec<String>| !list.is_empty())
            .unwrap_or_else(|| vec![path.to_string_lossy().into_owned()]);
        let design_name = design_of(&sources, entry);
        for (sheet, file) in written.iter().enumerate() {
            files.push(json!({
                "file": file,
                "source": entry.drawing.source,
                "design": design_name,
                "drawing": entry.drawing.drawing,
                "name": entry.name,
                "sheet": sheet + 1,
            }));
        }
        let mut drawn = entry_json(&sources, entry);
        drawn["sheets"] = json!(written.len());
        drawings.push(drawn);
    }
    let book = doc.drawing_book();
    Ok(json!({
        "title": book.title,
        "cover": book.cover,
        "drawings": drawings,
        "files": files,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_reads_its_fields_and_refuses_others() {
        let command: DrawingBookCommand = serde_json::from_value(json!({
            "cmd": "set_drawing_book", "title": "Project", "cover": true,
            "drawings": [{"source": "part.mitcad", "drawing": "D2", "name": "Drawing1"}, {"drawing": "D1"}]
        }))
        .unwrap();
        let DrawingBookCommand::Set {
            title,
            cover,
            drawings,
        } = command;
        assert_eq!(title.as_deref(), Some("Project"));
        assert_eq!(cover, Some(true));
        assert_eq!(drawings.map(|d| d.len()), Some(2));
        let error = serde_json::from_value::<DrawingBookCommand>(json!({
            "cmd": "set_drawing_book", "pages": 2
        }))
        .unwrap_err()
        .to_string();
        assert!(error.contains("unknown field `pages`"), "{error}");
        assert_eq!(DrawingBookCommand::NAMES.to_vec(), vec!["set_drawing_book"]);
    }
}
