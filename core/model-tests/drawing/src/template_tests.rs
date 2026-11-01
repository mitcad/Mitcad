// SPDX-License-Identifier: MIT
//! Sheet templates and revisions (mitcad#707 phase 4) on the mock kernel: a
//! template's title block filled in from the design and following it (its
//! name, part number, material, mass, author, scale, sheets, revision and
//! any property); templates chosen per sheet, kept with the drawing,
//! copied with a component and saved to a project's library; the revision
//! table's commands, undo and the file; the export.

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

/// A fresh folder of a test's own beside the test program in the build
/// tree, not in the system's temporary folder (mitcad#722).
fn own_folder(name: &str) -> std::path::PathBuf {
    let exe = std::env::current_exe().expect("the test program's path");
    let dir = exe
        .parent()
        .expect("the test program's folder")
        .join("test-folders")
        .join(format!("{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn drawing(doc: &Document<MockKernel>, sheet: usize) -> Value {
    query(
        doc,
        json!({"query": "drawing", "drawing": "D1", "sheet": sheet, "title": "Bracket"}),
    )
}

/// A title block field's text on the sheet asked for, by its label.
fn field(doc: &Document<MockKernel>, sheet: usize, label: &str) -> String {
    let answer = drawing(doc, sheet);
    answer["sheets"][sheet]["title_fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["label"] == label)
        .unwrap_or_else(|| panic!("no field {label}: {answer}"))["text"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn texts(doc: &Document<MockKernel>, class: &str) -> Vec<String> {
    drawing(doc, 0)["sheets"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["class"] == class)
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter_map(|i| i["text"].as_str().map(str::to_owned))
        .collect()
}

/// A 40 x 30 x `height` block (F2.b0); a drawing with the ISO template and
/// a front view at 1:2.
fn templated() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "height", "value": "20 mm"},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "height"}, "operation": "new_body"}},
        ]),
    );
    command(&mut doc, json!({"cmd": "add_drawing", "template": "iso"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [150, 150], "scale": "1:2"}),
    );
    doc
}

#[test]
fn a_templates_title_block_follows_the_design() {
    let mut doc = templated();
    // The template's own sheet size, A3; its id and where it came from.
    let sheet = drawing(&doc, 0)["sheets"][0].clone();
    assert_eq!(sheet["size"], "A3");
    assert_eq!(sheet["template"]["choice"], "iso");
    assert_eq!(sheet["template"]["library"], "builtin");
    assert_eq!(field(&doc, 0, "TITLE"), "Bracket");
    assert_eq!(field(&doc, 0, "SCALE"), "1:2");
    assert_eq!(field(&doc, 0, "SHEET"), "1/1");
    assert_eq!(field(&doc, 0, "SIZE"), "A3");
    assert_eq!(field(&doc, 0, "UNITS"), "mm");
    assert_eq!(field(&doc, 0, "REV"), "-");
    assert_eq!(field(&doc, 0, "DRAWING NO."), "");
    // The template's frame and block are drawn, not the simple block.
    let block = texts(&doc, "title-block");
    for text in ["TITLE", "MATERIAL", "MASS", "Bracket", "1:2"] {
        assert!(block.iter().any(|t| t == text), "{text}: {block:?}");
    }
    assert!(!block.iter().any(|t| t == "MADE WITH"), "{block:?}");

    // The material: the body's; it follows a change of it.
    let before = field(&doc, 0, "MATERIAL");
    assert!(!before.is_empty());
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "pla"}),
    );
    let pla = query(&doc, json!({"query": "materials"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "pla")
        .unwrap()["name"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(field(&doc, 0, "MATERIAL"), pla);

    // The mass, worked out from the model: it follows the block's height.
    let mass = field(&doc, 0, "MASS");
    assert!(mass.ends_with("kg"), "{mass}");
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 35}),
    );
    let heavier = field(&doc, 0, "MASS");
    assert_ne!(heavier, mass);

    // The part number and the author: the design's properties, unless the
    // sheet gives its own.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Identity", "key": "Part number",
               "value": "P-100"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Identity", "key": "Author",
               "value": "A. Maker"}),
    );
    assert_eq!(field(&doc, 0, "PART NO."), "P-100");
    assert_eq!(field(&doc, 0, "DRAWING NO."), "P-100");
    assert_eq!(field(&doc, 0, "DRAWN BY"), "A. Maker");
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1",
               "title_block": {"number": "DWG-7", "author": "JD", "title": "Bracket, left"}}),
    );
    assert_eq!(field(&doc, 0, "DRAWING NO."), "DWG-7");
    assert_eq!(field(&doc, 0, "DRAWN BY"), "JD");
    assert_eq!(field(&doc, 0, "TITLE"), "Bracket, left");
    assert_eq!(field(&doc, 0, "PART NO."), "P-100");

    // A second sheet takes the first one's template: its sheet numbers.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_sheet", "drawing": "D1"}),
    );
    assert_eq!(field(&doc, 1, "SHEET"), "2/2");
    assert_eq!(field(&doc, 0, "SHEET"), "1/2");
}

#[test]
fn revisions_are_added_shown_edited_and_kept() {
    let mut doc = templated();
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_revision", "drawing": "D1", "description": "First release"}),
    );
    assert_eq!(added["revision"], "A");
    assert_eq!(undo_label(&doc), "Add revision A to Drawing1");
    let answer = drawing(&doc, 0);
    assert_eq!(answer["revisions"][0]["description"], "First release");
    assert!(answer["revisions"][0]["date"].is_string());
    assert_eq!(answer["next_revision"], "B");
    assert_eq!(field(&doc, 0, "REV"), "A");
    // The revision table above the title block.
    let table = texts(&doc, "revision-table");
    for text in ["REV", "DESCRIPTION", "A", "First release"] {
        assert!(table.iter().any(|t| t == text), "{text}: {table:?}");
    }
    command(
        &mut doc,
        json!({"cmd": "add_drawing_revision", "drawing": "D1", "description": "Hole moved",
               "author": "JD", "date": "2026-11-02"}),
    );
    assert_eq!(field(&doc, 0, "REV"), "B");
    assert!(
        texts(&doc, "revision-table")
            .iter()
            .any(|t| t == "2026-11-02")
    );
    // Edited and deleted, each an undo step.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_revision", "drawing": "D1", "revision": "B",
               "description": "Hole moved 5 mm"}),
    );
    assert_eq!(undo_label(&doc), "Edit revision B of Drawing1");
    assert_eq!(
        drawing(&doc, 0)["revisions"][1]["description"],
        "Hole moved 5 mm"
    );
    command(
        &mut doc,
        json!({"cmd": "delete_drawing_revision", "drawing": "D1", "revision": "B"}),
    );
    assert_eq!(undo_label(&doc), "Delete revision B of Drawing1");
    assert_eq!(field(&doc, 0, "REV"), "A");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(field(&doc, 0, "REV"), "B");
    // Refused: no description, a letter it has, one it has not.
    for (cmd, message) in [
        (
            json!({"cmd": "add_drawing_revision", "drawing": "D1"}),
            "needs a description",
        ),
        (
            json!({"cmd": "add_drawing_revision", "drawing": "D1", "letter": "A",
                   "description": "Again"}),
            "has a revision A already",
        ),
        (
            json!({"cmd": "edit_drawing_revision", "drawing": "D1", "revision": "Q",
                   "description": "x"}),
            "has no revision Q",
        ),
    ] {
        let error = refused(&mut doc, cmd);
        assert!(error.contains(message), "{error}");
    }
    // Saved and read again.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["drawings"][0]["revisions"][1]["letter"], "B");
    assert_eq!(file["drawings"][0]["sheets"][0]["template"]["id"], "iso");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
}

#[test]
fn templates_are_chosen_per_sheet_and_travel_with_the_drawing() {
    let mut doc = templated();
    // ANSI on the sheet; then none: the simple title block.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "template": "ansi"}),
    );
    let sheet = drawing(&doc, 0)["sheets"][0].clone();
    assert_eq!(sheet["template"]["choice"], "ansi");
    assert_eq!(sheet["size"], "A3", "a sheet keeps its size");
    assert_eq!(field(&doc, 0, "DWG NO."), "");
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "template": ""}),
    );
    assert!(drawing(&doc, 0)["sheets"][0]["template"].is_null());
    assert!(texts(&doc, "title-block").iter().any(|t| t == "MADE WITH"));
    // A template's data (a user's or a project's, as the query lists it),
    // with a field of any property.
    let mut ours = query(&doc, json!({"query": "drawing_templates"}))["templates"][0].clone();
    ours["id"] = json!("ours");
    ours["name"] = json!("Ours");
    ours["library"] = json!("user");
    ours.as_object_mut().unwrap().remove("choice");
    ours["title_block"]["fields"][0]["text"] = json!("{title} for {Identity/Customer}");
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Identity", "key": "Customer",
               "value": "ACME"}),
    );
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "template": ours}),
    );
    assert_eq!(
        drawing(&doc, 0)["sheets"][0]["template"]["choice"],
        "user:ours"
    );
    assert_eq!(field(&doc, 0, "TITLE"), "Bracket for ACME");
    // Refused: no such built-in one, a template that is not one.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "template": "din"}),
    );
    assert!(error.contains("no built-in template din"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing", "template": {"id": "x", "name": "X"}}),
    );
    assert!(error.contains("invalid template"), "{error}");

    // A component's drawing with a template and revisions: a copy of the
    // component brings both.
    let made = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bracket"}),
    );
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 10, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
        ]),
    );
    let own = command(
        &mut doc,
        json!({"cmd": "add_drawing", "component": "Bracket", "template": "ansi"}),
    )["drawing"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_revision", "drawing": own, "description": "First"}),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "paste_new", "occurrence": made["occurrence"]}),
    );
    let all = query(&doc, json!({"query": "drawings"}));
    let copy = all.as_array().unwrap().last().unwrap()["drawing"].clone();
    assert_ne!(copy, json!(own));
    let copied = query(&doc, json!({"query": "drawing", "drawing": copy}));
    assert_eq!(copied["sheets"][0]["template"]["choice"], "ansi");
    assert_eq!(copied["revisions"][0]["letter"], "A");
}

#[test]
fn a_sheets_template_is_saved_to_the_projects_library() {
    let mut doc = templated();
    let project = own_folder("project");
    let folder = project.to_string_lossy().into_owned();
    let depth = doc.undo_depth();
    let saved = command(
        &mut doc,
        json!({"cmd": "save_drawing_template", "drawing": "D1", "library": "project",
               "name": "Company A3", "project": folder}),
    );
    assert_eq!(saved["id"], "company-a3");
    assert_eq!(saved["choice"], "project:company-a3");
    assert!(
        std::path::Path::new(saved["path"].as_str().unwrap()).is_file(),
        "{saved}"
    );
    // The design did not change.
    assert_eq!(doc.undo_depth(), depth);
    // Listed with the built-in ones, and chosen for a new drawing.
    let listed = query(
        &doc,
        json!({"query": "drawing_templates", "project": folder}),
    );
    let choices: Vec<&str> = listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["choice"].as_str().unwrap())
        .collect();
    assert!(choices.contains(&"iso") && choices.contains(&"ansi"));
    assert!(choices.contains(&"project:company-a3"), "{listed}");
    let ours = listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["choice"] == "project:company-a3")
        .unwrap()
        .clone();
    let mut ours = ours;
    ours.as_object_mut().unwrap().remove("choice");
    let added = command(&mut doc, json!({"cmd": "add_drawing", "template": ours}));
    let second = query(
        &doc,
        json!({"query": "drawing", "drawing": added["drawing"]}),
    );
    assert_eq!(second["sheets"][0]["template"]["name"], "Company A3");
    assert_eq!(second["sheets"][0]["size"], "A3");
    // With its projection method, as saved from an ISO sheet (mitcad#725).
    assert_eq!(ours["projection"], "first_angle", "{ours}");
    assert_eq!(second["sheets"][0]["projection"], "first_angle");
    // A sheet with the simple block has no template to save; a library
    // other than the user's or the project's is refused.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "template": null}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "save_drawing_template", "drawing": "D1", "library": "project",
               "name": "None", "project": folder}),
    );
    assert!(error.contains("simple title block"), "{error}");
    std::fs::remove_dir_all(&project).ok();
}

#[test]
fn a_templated_sheet_is_exported_with_its_title_block_and_revisions() {
    let mut doc = templated();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_revision", "drawing": "D1", "description": "First release"}),
    );
    let dir = own_folder("template-export");
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["sheet.pdf", "sheet.svg", "sheet.dxf"] {
        let path = dir.join(name);
        command(
            &mut doc,
            json!({"cmd": "export_drawing", "drawing": "D1", "path": path.to_string_lossy(),
                   "title": "Bracket"}),
        );
        let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
        for expected in ["MATERIAL", "First release", "Bracket", "DESCRIPTION"] {
            assert!(text.contains(expected), "{name}: {expected}");
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_edit_that_clears_a_revisions_date_gives_it_todays() {
    // mitcad#722: the dialog's placeholder says "Today" and the guide
    // "today's when left empty", as for a new revision.
    let mut doc = templated();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_revision", "drawing": "D1", "description": "First release",
               "date": "2026-01-02", "author": "JD"}),
    );
    assert_eq!(drawing(&doc, 0)["revisions"][0]["date"], "2026-01-02");
    let before = mitcad_model_drawing::today();
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_revision", "drawing": "D1", "revision": "A",
               "date": " ", "author": ""}),
    );
    let after = mitcad_model_drawing::today();
    let revision = drawing(&doc, 0)["revisions"][0].clone();
    let date = revision["date"].as_str().unwrap_or_default().to_owned();
    assert!(date == before || date == after, "{revision}");
    // The author cleared stays cleared.
    assert!(
        revision.get("author").is_none_or(Value::is_null),
        "{revision}"
    );
    // A date given is kept.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_revision", "drawing": "D1", "revision": "A",
               "date": "2026-03-04"}),
    );
    assert_eq!(drawing(&doc, 0)["revisions"][0]["date"], "2026-03-04");
}

#[test]
fn a_sheets_template_must_fit_the_sheet() {
    // mitcad#722: margins and a title block larger than the sheet were
    // taken (a frame of negative width).
    let mut doc = templated();
    let mut wide = query(&doc, json!({"query": "drawing_templates"}))["templates"][0].clone();
    wide["id"] = json!("wide");
    wide["name"] = json!("Wide");
    wide["library"] = json!("user");
    wide.as_object_mut().unwrap().remove("choice");
    wide["margins"] = json!([250, 10, 250, 10]);
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing", "template": wide, "size": "A4"}),
    );
    assert!(error.contains("leave no frame"), "{error}");
    // ISO's block fits A4 upright; a block higher than the frame does not.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "size": "A4",
               "orientation": "portrait"}),
    );
    let mut high = query(&doc, json!({"query": "drawing_templates"}))["templates"][0].clone();
    high["id"] = json!("high");
    high["name"] = json!("High");
    high["library"] = json!("user");
    high.as_object_mut().unwrap().remove("choice");
    high["size"] = json!("A2");
    high["title_block"]["height"] = json!(200);
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "template": high,
               "orientation": "landscape"}),
    );
    assert!(error.contains("do not fit"), "{error}");
    assert_eq!(drawing(&doc, 0)["sheets"][0]["template"]["choice"], "iso");
}

#[test]
fn a_stored_mass_is_the_title_blocks_mass() {
    // mitcad#722: the title block's mass was always worked out from the
    // bodies, while its material honours a stored `Material`.
    let mut doc = templated();
    let worked_out = field(&doc, 0, "MASS");
    assert!(worked_out.ends_with("kg"), "{worked_out}");
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Physical", "key": "Mass",
               "value": "12.5 kg (weighed)"}),
    );
    assert_eq!(field(&doc, 0, "MASS"), "12.5 kg (weighed)");
    // It stays when the bodies change.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 35}),
    );
    assert_eq!(field(&doc, 0, "MASS"), "12.5 kg (weighed)");
    // Removed: worked out again.
    command(
        &mut doc,
        json!({"cmd": "remove_property", "owner": "C0", "set": "Physical", "key": "Mass"}),
    );
    let again = field(&doc, 0, "MASS");
    assert!(again.ends_with("kg") && again != worked_out, "{again}");
}

/// The highest y of a group's lines and outlines on the first sheet.
fn top_of(doc: &Document<MockKernel>, class: &str) -> f64 {
    drawing(doc, 0)["sheets"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["class"] == class)
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .flat_map(|i| {
            let points = i
                .get("line")
                .or_else(|| i.get("polyline"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            points.into_iter().filter_map(|p| p[1].as_f64())
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn views_keep_clear_of_the_revision_tables_room() {
    // mitcad#722: the views' area left room for the title block only, so
    // a view placed just above it was overlapped by the first revision.
    let mut doc = templated();
    let top = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "scale": "1:1"}),
    )["view"]
        .as_u64()
        .unwrap();
    let views = drawing(&doc, 0)["sheets"][0]["views"].clone();
    let view = views
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == top)
        .cloned()
        .unwrap();
    let b = &view["box"];
    let (low, high) = (b[0][1].as_f64().unwrap(), b[1][1].as_f64().unwrap());
    // ISO on A3: the frame from 10 to 287 high, the block 36 high and the
    // revision table's four rows and header of 5 mm above it: the views'
    // area from 75 to 283, its middle at 179.
    assert!(((low + high) / 2.0 - 179.0).abs() < 1e-6, "{view}");
    // Five revisions: the table, its four rows full, stays below the view.
    for description in ["First", "Second", "Third", "Fourth", "Fifth"] {
        command(
            &mut doc,
            json!({"cmd": "add_drawing_revision", "drawing": "D1", "description": description}),
        );
    }
    let table = top_of(&doc, "revision-table");
    assert!((table - 71.0).abs() < 1e-6, "{table}");
    assert!(table < low, "{table} {low}");
}

#[test]
fn a_template_brings_its_projection_method() {
    // mitcad#725: ISO starts a drawing in first angle, ANSI in third angle,
    // as each brings its sheet size; a projection given wins.
    let mut doc = templated();
    assert_eq!(drawing(&doc, 0)["sheets"][0]["projection"], "first_angle");
    let listed = query(&doc, json!({"query": "drawing_templates"}));
    let projections: Vec<(&str, &str)> = listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["choice"].as_str().unwrap(),
                t["projection"].as_str().unwrap_or(""),
            )
        })
        .collect();
    assert!(projections.contains(&("iso", "first_angle")), "{listed}");
    assert!(projections.contains(&("ansi", "third_angle")), "{listed}");
    let ansi = command(&mut doc, json!({"cmd": "add_drawing", "template": "ansi"}));
    let answer = query(
        &doc,
        json!({"query": "drawing", "drawing": ansi["drawing"]}),
    );
    assert_eq!(answer["sheets"][0]["projection"], "third_angle");
    let given = command(
        &mut doc,
        json!({"cmd": "add_drawing", "template": "iso", "projection": "third_angle"}),
    );
    let answer = query(
        &doc,
        json!({"query": "drawing", "drawing": given["drawing"]}),
    );
    assert_eq!(answer["sheets"][0]["projection"], "third_angle");
    // The simple title block brings none: the default, third angle.
    let simple = command(&mut doc, json!({"cmd": "add_drawing", "template": null}));
    let answer = query(
        &doc,
        json!({"query": "drawing", "drawing": simple["drawing"]}),
    );
    assert_eq!(answer["sheets"][0]["projection"], "third_angle");
}

/// A title block field's text on a drawing's first sheet, by its label.
fn field_of(doc: &Document<MockKernel>, drawing: &str, label: &str) -> String {
    let answer = query(doc, json!({"query": "drawing", "drawing": drawing}));
    answer["sheets"][0]["title_fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["label"] == label)
        .unwrap_or_else(|| panic!("no field {label}: {answer}"))["text"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn material_name(doc: &Document<MockKernel>, id: &str) -> String {
    query(doc, json!({"query": "materials"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .unwrap_or_else(|| panic!("no material {id}"))["name"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_components_drawing_takes_the_default_material_of_the_components_between() {
    // mitcad#758 item 1: a component's drawing looked for the material of
    // a body in a subcomponent only in the body's own component and the
    // root, skipping the drawing's component and those between.
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    // Frame holds Bracket:1, whose block has no material of its own.
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Frame"}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bracket"}),
    );
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    // The root's default PLA, Frame's steel (denser by far).
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "C0", "material": "pla"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Frame", "material": "steel"}),
    );
    let root = command(&mut doc, json!({"cmd": "add_drawing", "template": "iso"}))["drawing"]
        .as_str()
        .unwrap()
        .to_owned();
    let frame = command(
        &mut doc,
        json!({"cmd": "add_drawing", "component": "Frame", "template": "iso"}),
    )["drawing"]
        .as_str()
        .unwrap()
        .to_owned();
    let steel = material_name(&doc, "steel");
    // The root's drawing finds Frame's default up the path Frame:1/Bracket:1,
    // and so does Frame's own drawing: steel, not the root's PLA.
    assert_eq!(field_of(&doc, &root, "MATERIAL"), steel);
    assert_eq!(field_of(&doc, &frame, "MATERIAL"), steel);
    // The mass with steel's density, as the root's drawing works it out.
    let mass = field_of(&doc, &frame, "MASS");
    assert!(mass.ends_with("kg"), "{mass}");
    assert_eq!(mass, field_of(&doc, &root, "MASS"));
    // Without Frame's default both drawings come down to the root's PLA,
    // and the mass drops with the density.
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Frame", "material": null}),
    );
    let pla = material_name(&doc, "pla");
    assert_eq!(field_of(&doc, &frame, "MATERIAL"), pla);
    assert_eq!(field_of(&doc, &root, "MATERIAL"), pla);
    let lighter = field_of(&doc, &frame, "MASS");
    assert_ne!(lighter, mass);
    assert_eq!(lighter, field_of(&doc, &root, "MASS"));
    // Bracket's own default wins over Frame's on Frame's drawing.
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Frame", "material": "steel"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Bracket", "material": "nylon"}),
    );
    assert_eq!(
        field_of(&doc, &frame, "MATERIAL"),
        material_name(&doc, "nylon")
    );
}

#[test]
fn a_title_block_falls_back_to_the_design_summary() {
    // mitcad#1088: File › Design Properties…'s summary (the root's set
    // Summary) fills the title, the author and the description where the
    // sheet and the component give none.
    let mut doc = templated();
    command(
        &mut doc,
        json!({"cmd": "set_design_summary", "title": "Gearbox housing", "author": "A. Maker",
               "comments": "Cast, then machined"}),
    );
    assert_eq!(field(&doc, 0, "TITLE"), "Gearbox housing");
    assert_eq!(field(&doc, 0, "DRAWN BY"), "A. Maker");
    // The description: a template field of its own.
    let mut ours = query(&doc, json!({"query": "drawing_templates"}))["templates"][0].clone();
    ours["id"] = json!("described");
    ours["name"] = json!("Described");
    ours["library"] = json!("user");
    ours.as_object_mut().unwrap().remove("choice");
    for f in ours["title_block"]["fields"].as_array_mut().unwrap() {
        if f["label"] == "PART NO." {
            f["text"] = json!("{description}");
        }
    }
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "template": ours}),
    );
    assert_eq!(field(&doc, 0, "PART NO."), "Cast, then machined");
    // A property Description of the component comes first.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Identity", "key": "Description",
               "value": "Housing"}),
    );
    assert_eq!(field(&doc, 0, "PART NO."), "Housing");
    // The sheet's own title and author come first.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1",
               "title_block": {"author": "JD", "title": "Housing, left"}}),
    );
    assert_eq!(field(&doc, 0, "TITLE"), "Housing, left");
    assert_eq!(field(&doc, 0, "DRAWN BY"), "JD");

    // A component's drawing: titled by its name (the summary names the
    // design), its author the design's.
    command(&mut doc, json!({"cmd": "create_component", "name": "Lid"}));
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    let lid = command(
        &mut doc,
        json!({"cmd": "add_drawing", "component": "Lid", "template": "iso"}),
    )["drawing"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(field_of(&doc, &lid, "TITLE"), "Lid");
    assert_eq!(field_of(&doc, &lid, "DRAWN BY"), "A. Maker");
}
