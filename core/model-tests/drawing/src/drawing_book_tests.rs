// SPDX-License-Identifier: MIT
//! The drawing book (mitcad#2448) on the mock kernel: a main design that
//! links two designs, one of them placed twice, offers its own drawings and
//! each linked design's once; same-named drawings of two designs are
//! chosen apart; the selection, its order, the title and the cover page are
//! one undo step and come back with the design file; the sheets are written
//! in the book's order (every sheet of each chosen drawing, the others
//! left out), drawn from the linked files as they are now, and no source
//! file changes; a drawing that is gone or a design that cannot be read is
//! named before printing and keeps the book from printing. Through the
//! application: `tools/ui-drawing-book-test.sh`.

use std::path::Path;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn base(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}

/// A design of one block with a drawing `name` of `sheets` sheets, a base
/// view on each, and, with `extra`, a second drawing Extra.
fn source(name: &str, sheets: usize, extra: bool) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    add_drawing(&mut doc, name, sheets);
    if extra {
        add_drawing(&mut doc, "Extra", 1);
    }
    doc
}

fn add_drawing(doc: &mut Document<MockKernel>, name: &str, sheets: usize) {
    let added = command(
        doc,
        json!({"cmd": "add_drawing", "name": name, "size": "A3"}),
    );
    let uid = added["drawing"].as_str().unwrap().to_owned();
    for sheet in 0..sheets {
        if sheet > 0 {
            command(doc, json!({"cmd": "add_drawing_sheet", "drawing": uid}));
        }
        command(
            doc,
            json!({"cmd": "add_drawing_view", "drawing": uid, "sheet": sheet, "type": "base",
                   "direction": "front", "at": [150, 120]}),
        );
    }
}

fn write(dir: &Path, file: &str, doc: &Document<MockKernel>) {
    std::fs::write(dir.join(file), doc.to_json()).unwrap();
}

/// The folder of the designs: `a.mitcad` (Drawing1 of two sheets and
/// Extra), `b.mitcad` (Drawing1 of one sheet), and the main design that
/// links a twice and b once and has a drawing Assembly of its own.
fn project(name: &str) -> (std::path::PathBuf, Document<MockKernel>) {
    let dir = mitcad_model::testing::unique_temp_dir(name);
    write(&dir, "a.mitcad", &source("Drawing1", 2, true));
    write(&dir, "b.mitcad", &source("Drawing1", 1, false));
    let mut main = source("Assembly", 1, false);
    for file in ["a.mitcad", "a.mitcad", "b.mitcad"] {
        command(
            &mut main,
            json!({"cmd": "insert_component", "path": file, "base": base(&dir)}),
        );
    }
    (dir, main)
}

/// The drawings of a source in the `drawing_book` query, by name.
fn offered(book: &Value, design: &str) -> Vec<String> {
    book["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["design"] == design)
        .unwrap_or_else(|| panic!("no source {design}: {book}"))["drawings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap().to_owned())
        .collect()
}

/// The book's drawings as `design/name` with their sheets.
fn chosen(book: &Value) -> Vec<String> {
    book["drawings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            format!(
                "{}/{}:{}",
                d["design"].as_str().unwrap(),
                d["name"].as_str().unwrap(),
                d["sheets"]
            )
        })
        .collect()
}

/// Drawing1 of b, Drawing1 of a, then the design's own Assembly, with a
/// cover page.
fn choose(main: &mut Document<MockKernel>) -> Value {
    command(
        main,
        json!({"cmd": "set_drawing_book", "title": "Project book", "cover": true,
               "drawings": [{"source": "b.mitcad", "drawing": "D1", "name": "Drawing1"},
                            {"source": "a.mitcad", "drawing": "D1", "name": "Drawing1"},
                            {"drawing": "D1"}]}),
    )
}

#[test]
fn the_sources_are_the_design_and_each_linked_design_once() {
    let (dir, main) = project("book-sources");
    let book = query(&main, json!({"query": "drawing_book", "base": base(&dir)}));
    let designs: Vec<&str> = book["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["design"].as_str().unwrap())
        .collect();
    // The design itself first (no file name), then a once though it is
    // placed twice, then b.
    assert_eq!(designs, ["", "a.mitcad", "b.mitcad"], "{book}");
    assert_eq!(book["sources"][0]["source"], Value::Null);
    assert_eq!(offered(&book, ""), ["Assembly"]);
    assert_eq!(offered(&book, "a.mitcad"), ["Drawing1", "Extra"]);
    assert_eq!(offered(&book, "b.mitcad"), ["Drawing1"]);
    assert_eq!(book["sources"][1]["drawings"][0]["sheets"], 2);
    assert_eq!(book["sources"][1]["source"], "a.mitcad");
    // An empty book: off, untitled, nothing to print.
    assert_eq!(book["title"], "");
    assert_eq!(book["cover"], false);
    assert_eq!(book["drawings"], json!([]));
    assert_eq!(book["problems"], json!([]));
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn the_selection_order_and_cover_are_kept_with_the_design() {
    let (dir, mut main) = project("book-saved");
    let set = choose(&mut main);
    // The command's own fields (the API adds `error` and `recomputed`).
    assert_eq!(set["changed"], true, "{set}");
    assert_eq!(set["drawings"], 3, "{set}");
    assert_eq!(
        query(&main, json!({"query": "document"}))["undo"],
        "Edit Drawing Book"
    );
    // The same book again is no undo step.
    let depth = query(&main, json!({"query": "document"}))["undo_depth"].clone();
    assert_eq!(choose(&mut main)["changed"], false);
    assert_eq!(
        query(&main, json!({"query": "document"}))["undo_depth"],
        depth
    );
    let book = query(&main, json!({"query": "drawing_book", "base": base(&dir)}));
    // Same-named drawings of two designs are two entries.
    assert_eq!(
        chosen(&book),
        ["b.mitcad/Drawing1:1", "a.mitcad/Drawing1:2", "/Assembly:1"]
    );
    assert_eq!(book["title"], "Project book");
    assert_eq!(book["cover"], true);
    // Saved and read again.
    let path = dir.join("main.mitcad");
    std::fs::write(&path, main.to_json()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"drawing_book\""), "{text}");
    let read = Document::from_json_at(&text, &path, MockKernel::default()).unwrap();
    let again = query(&read, json!({"query": "drawing_book"}));
    assert_eq!(chosen(&again), chosen(&book));
    assert_eq!(again["title"], "Project book");
    assert_eq!(again["cover"], true);
    // Undo takes the book back.
    command(&mut main, json!({"cmd": "undo"}));
    let undone = query(&main, json!({"query": "drawing_book", "base": base(&dir)}));
    assert_eq!(undone["drawings"], json!([]));
    // A drawing twice is refused.
    let error = refused(
        &mut main,
        json!({"cmd": "set_drawing_book",
               "drawings": [{"source": "a.mitcad", "drawing": "D1"}, {"source": "a.mitcad", "drawing": "D1"}]}),
    );
    assert!(error.contains("more than once"), "{error}");
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn the_sheets_follow_the_book_and_the_source_files_as_they_are_now() {
    let (dir, mut main) = project("book-sheets");
    choose(&mut main);
    let before_a = std::fs::read_to_string(dir.join("a.mitcad")).unwrap();
    let before_b = std::fs::read_to_string(dir.join("b.mitcad")).unwrap();
    let out = dir.join("sheets");
    let sheets = query(
        &main,
        json!({"query": "drawing_book_sheets", "folder": base(&out), "base": base(&dir),
               "title": "main"}),
    );
    let files: Vec<String> = sheets["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            assert!(Path::new(f["file"].as_str().unwrap()).is_file(), "{f}");
            format!(
                "{}/{}:{}",
                f["design"].as_str().unwrap(),
                f["name"].as_str().unwrap(),
                f["sheet"]
            )
        })
        .collect();
    // Every sheet of each chosen drawing in the book's order; Extra is
    // left out.
    assert_eq!(
        files,
        [
            "b.mitcad/Drawing1:1",
            "a.mitcad/Drawing1:1",
            "a.mitcad/Drawing1:2",
            "/Assembly:1"
        ]
    );
    assert_eq!(sheets["title"], "Project book");
    assert_eq!(sheets["cover"], true);
    let first = std::fs::read_to_string(sheets["files"][0]["file"].as_str().unwrap()).unwrap();
    assert!(first.contains("<svg"), "{first}");
    // Printing changes no source design.
    assert_eq!(
        std::fs::read_to_string(dir.join("a.mitcad")).unwrap(),
        before_a
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("b.mitcad")).unwrap(),
        before_b
    );

    // A sheet added to a's drawing and saved is in the next print.
    let path = dir.join("a.mitcad");
    let mut a = Document::from_json_at(&before_a, &path, MockKernel::default()).unwrap();
    a.recompute();
    command(&mut a, json!({"cmd": "add_drawing_sheet", "drawing": "D1"}));
    command(
        &mut a,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "sheet": 2, "type": "base",
               "direction": "top", "at": [150, 120]}),
    );
    std::fs::write(&path, a.to_json()).unwrap();
    let book = query(&main, json!({"query": "drawing_book", "base": base(&dir)}));
    assert_eq!(chosen(&book)[1], "a.mitcad/Drawing1:3");
    let sheets = query(
        &main,
        json!({"query": "drawing_book_sheets", "folder": base(&out), "base": base(&dir)}),
    );
    assert_eq!(sheets["files"].as_array().unwrap().len(), 5, "{sheets}");
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_drawing_gone_or_a_design_unread_is_named_before_printing() {
    let (dir, mut main) = project("book-problems");
    choose(&mut main);
    // b's drawing deleted and saved.
    let mut b = source("Other", 1, false);
    command(&mut b, json!({"cmd": "delete_drawing", "drawing": "D1"}));
    write(&dir, "b.mitcad", &b);
    let book = query(&main, json!({"query": "drawing_book", "base": base(&dir)}));
    let problems = book["problems"].to_string();
    assert!(
        problems.contains("Drawing1 is no longer in b.mitcad"),
        "{book}"
    );
    assert_eq!(book["drawings"][0]["problem"], book["problems"][0]);
    assert!(book["drawings"][1].get("problem").is_none(), "{book}");
    let out = dir.join("sheets");
    let query_sheets =
        json!({"query": "drawing_book_sheets", "folder": base(&out), "base": base(&dir)});
    let error = main
        .query(&query_sheets.to_string())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the drawing book cannot be printed"),
        "{error}"
    );
    assert!(
        error.contains("Drawing1 is no longer in b.mitcad"),
        "{error}"
    );
    // a's file unreadable: its drawing is named with the reason.
    std::fs::write(dir.join("a.mitcad"), "not a design").unwrap();
    let book = query(&main, json!({"query": "drawing_book", "base": base(&dir)}));
    let problem = book["drawings"][1]["problem"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(problem.starts_with("Drawing1 of a.mitcad: "), "{book}");
    // The selection corrected: the book prints again.
    command(
        &mut main,
        json!({"cmd": "set_drawing_book", "drawings": [{"drawing": "D1"}]}),
    );
    let sheets = query(&main, query_sheets);
    assert_eq!(sheets["files"].as_array().unwrap().len(), 1, "{sheets}");
    // An empty book has nothing to print.
    command(
        &mut main,
        json!({"cmd": "set_drawing_book", "drawings": []}),
    );
    let error = main
        .query(&json!({"query": "drawing_book_sheets", "folder": base(&out)}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("has no drawing"), "{error}");
    mitcad_model::testing::remove_temp(&dir);
}
