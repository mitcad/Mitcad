// SPDX-License-Identifier: MIT
//! The status of every row of an import's report and its one-line verdict
//! (mitcad#1893), for the application's Import Report and for agents alike.
//!
//! [`annotate`] runs on every import command's result (`mitcad-ffi`,
//! `import_command`): each row the report lists (a timeline item, a body
//! against the file's, an `.ipt` part's body, an assembly's file, an IFC
//! product, a SolidCraft feature, a level or a block) gets
//!
//! - `row_status`: `ok`, `warning` or `error`;
//! - `row_status_message`: what the status means for that row, in plain words
//!   (the symbol's tooltip in the report);
//!
//! and each design (an `.f3d` file's designs, every other import's report)
//! gets its `verdict`, `{"status", "message", "rows": {"ok", "warning",
//! "error"}, "warnings"}`, the worst of its rows and its warnings; the
//! result's own `verdict` sums its designs.
//!
//! What a status is:
//!
//! - **OK**: parametric; a valid body; a body that matches the file's
//!   within [`BODY_VOLUME_TOLERANCE`] of its volume and
//!   [`BODY_BOX_TOLERANCE`] of its bounding box (the difference in its
//!   message); an item left out as the file has it (suppressed, rolled
//!   back, without geometry); a file found and imported.
//! - **Warning**: partial; a fallback body; a parametric item whose result
//!   differs from the file's; a body that differs from the file's past the
//!   tolerances or in its number of faces; a body whose validity could not
//!   be checked; an assembly's part that came in as its stored bodies
//!   because its replay failed.
//! - **Error**: an item skipped for another reason, a body not built or
//!   not valid, a body of the file the import has none for, a file not
//!   found or not imported, a product or feature that could not be built.
//!
//! [`plain_words`] says the import's internal notes in words a person
//! reads (the report's `note_message`); the notes themselves stay the
//! agents' and the log's.

use serde_json::{Value, json};

/// How far (relative) a body's volume may be from the file's and still be
/// the same body (mitcad#1893: 0.01 %); its difference is in its message.
pub const BODY_VOLUME_TOLERANCE: f64 = 1e-4;

/// How far (mm) a body's bounding box may be from the file's and still be
/// the same body: the imports' geometric tolerance (mitcad#224).
pub const BODY_BOX_TOLERANCE: f64 = crate::DEFAULT_GEOMETRIC_TOLERANCE;

/// A row's status, the worst last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RowStatus {
    Ok,
    Warning,
    Error,
}

impl RowStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RowStatus::Ok => "ok",
            RowStatus::Warning => "warning",
            RowStatus::Error => "error",
        }
    }
}

/// A row's status and what it means, in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub status: RowStatus,
    pub message: String,
}

impl Status {
    fn new(status: RowStatus, message: impl Into<String>) -> Self {
        Status {
            status,
            message: message.into(),
        }
    }

    fn ok(message: impl Into<String>) -> Self {
        Status::new(RowStatus::Ok, message)
    }

    fn warning(message: impl Into<String>) -> Self {
        Status::new(RowStatus::Warning, message)
    }

    fn error(message: impl Into<String>) -> Self {
        Status::new(RowStatus::Error, message)
    }

    /// Writes `status` and `status_message` into the row.
    fn write(&self, row: &mut Value) {
        if let Some(object) = row.as_object_mut() {
            object.insert("row_status".to_owned(), json!(self.status.as_str()));
            object.insert("row_status_message".to_owned(), json!(self.message));
        }
    }
}

/// The notes of an item that the file itself leaves out: an item
/// suppressed or rolled back in the file, one without geometry of its own,
/// a mesh feature the file stores no mesh for, an object whose result is
/// the shape before it.
const LEFT_OUT_BY_THE_FILE: [&str; 6] = [
    "suppressed in the file",
    "suppressed in freecad",
    "rolled back in the file",
    "no geometry:",
    "no mesh body: the file stores none",
    "freecad's result is the shape before it",
];

/// Whether an item's note says the file itself leaves it out.
fn left_out_by_the_file(note: &str) -> bool {
    let note = note.trim().to_lowercase();
    LEFT_OUT_BY_THE_FILE.iter().any(|p| note.starts_with(p))
}

/// The status of a timeline item (`.f3d`, `.ipt` replay, FreeCAD object)
/// by its `outcome`, its note and whether its parametric result differs
/// from the file's (an accepted deviation).
pub fn item_status(outcome: &str, note: Option<&str>, deviates: bool) -> Status {
    match outcome {
        "parametric" if deviates => {
            Status::warning("came in parametric, but its result differs from the file's")
        }
        "parametric" => Status::ok("came in fully parametric"),
        "partial" => {
            Status::warning("came in partly parametric: the note says what was left out or fixed")
        }
        "fallback" => Status::warning("came in as a fixed body: its history could not be replayed"),
        "skipped" if note.is_some_and(left_out_by_the_file) => {
            Status::ok("left out as the file has it")
        }
        "skipped" => Status::error("could not be imported: the note says why"),
        // FreeCAD's objects that came in as what they are.
        "body" => Status::ok("came in as the shape the file stores"),
        "component" => Status::ok("came in as a component"),
        "occurrence" => Status::ok("came in as occurrences"),
        "included" => Status::ok("came in as part of another object's result"),
        _ => Status::warning("came in in a way the report does not know"),
    }
}

/// The relative volume difference as the messages show it.
fn percent(difference: f64) -> String {
    format!("{:+.4} %", difference * 100.0)
}

/// The status of a body of the file against the import's (the replay's
/// `bodies`: `file`, `mitcad`, `volume_difference`, `file_faces`,
/// `mitcad_faces`, `box_distance`).
pub fn body_status(body: &Value) -> Status {
    if body["mitcad"].as_str().is_none() {
        return Status::error("the file's body is not in the import");
    }
    let Some(difference) = body["volume_difference"].as_f64() else {
        return Status::ok("came in; not compared with the file's body");
    };
    let mut differences: Vec<String> = Vec::new();
    if difference.abs() > BODY_VOLUME_TOLERANCE {
        differences.push(format!("volume {}", percent(difference)));
    }
    if let (Some(mine), Some(file)) = (body["mitcad_faces"].as_u64(), body["file_faces"].as_u64())
        && mine != file
    {
        differences.push(format!("{mine} faces against the file's {file}"));
    }
    if let Some(off) = body["box_distance"]
        .as_f64()
        .filter(|d| *d > BODY_BOX_TOLERANCE)
    {
        differences.push(format!("its bounding box {off:.3} mm off"));
    }
    if !differences.is_empty() {
        let differences = differences.join(", ");
        return Status::warning(format!("differs from the file's body: {differences}"));
    }
    // The difference within the tolerance, said where it shows.
    if (difference * 100.0).abs() >= 0.00005 {
        let volume = percent(difference);
        Status::ok(format!("matches the file's body (volume {volume})"))
    } else {
        Status::ok("matches the file's body")
    }
}

/// The status of a body an `.ipt` part's import made (its `imported`:
/// `valid`, `solid`, `mesh`), with the replay's comparison of it with the
/// file's body where there is one.
pub fn stored_body_status(body: &Value, compared: Option<&Status>) -> Status {
    let own = match body["valid"].as_bool() {
        Some(false) => Status::error("came in, but the body is not valid"),
        None => Status::warning("came in; whether the body is valid could not be checked"),
        Some(true) if body["mesh"] == true => Status::ok("came in as a mesh"),
        Some(true) if body["solid"] == false => Status::ok("came in as a valid surface body"),
        Some(true) => Status::ok("came in as a valid body"),
    };
    match compared {
        Some(c) if c.status >= own.status => {
            let message = format!("{}; {}", own.message, c.message);
            Status::new(c.status, message)
        }
        _ => own,
    }
}

/// The status of an assembly's referenced file (an `.iam` import's
/// `files`: `status`, `found`, `kind`, `part`).
pub fn file_status(file: &Value) -> Status {
    let status = file["status"].as_str().unwrap_or_default();
    if file["found"] == "missing" || status == "missing" {
        return Status::error("the file was not found: its component is empty");
    }
    if status.starts_with("a cycle") {
        return Status::warning("the sub-assembly contains itself: it was left out");
    }
    if status != "imported" {
        return Status::error("the file could not be imported");
    }
    if file["kind"] == "assembly" {
        return Status::ok("found and placed");
    }
    let part = &file["part"];
    if part["outcome"] == "bodies"
        && part["reason"]
            .as_str()
            .is_some_and(|r| r.starts_with("the replay"))
    {
        return Status::warning("came in as its stored bodies: its history could not be replayed");
    }
    if part["valid"]
        .as_u64()
        .is_some_and(|valid| part["solids"].as_u64().is_some_and(|solids| valid < solids))
        || part["not_built"].as_u64().is_some_and(|n| n > 0)
    {
        return Status::error("found, but some of its bodies are missing or not valid");
    }
    let counts = &part["counts"];
    let count = |key: &str| counts[key].as_u64().unwrap_or(0);
    if count("partial") + count("fallback") + count("skipped") > 0 {
        return Status::warning("found; some of its features came in partly or as fixed bodies");
    }
    if part["outcome"] == "replayed" {
        Status::ok("found and replayed with its history")
    } else {
        Status::ok("found and imported")
    }
}

/// The status of an IFC product by its `outcome`.
pub fn product_status(outcome: &str) -> Status {
    match outcome {
        "parametric" => Status::ok("came in as features"),
        "stored" => Status::ok("came in as the bodies the file stores"),
        "mesh" => Status::ok("came in as meshes"),
        "mixed" => Status::ok("came in partly as features, partly as the bodies the file stores"),
        "occurrence" => Status::ok("came in as occurrences"),
        "component" => Status::ok("came in as a component"),
        "axes" => Status::ok("came in as construction axes"),
        "cut" => Status::ok("came in as a cut from its element"),
        "partial" => Status::warning("came in partly: the notes say what was left out"),
        "not_imported" => Status::warning("not imported: the notes say why"),
        _ => Status::error("could not be built: the notes say why"),
    }
}

/// The status of a SolidCraft feature or joint by its `outcome`.
pub fn solidcraft_status(outcome: &str) -> Status {
    match outcome {
        "parametric" => Status::ok("came in fully parametric"),
        "skipped" => Status::ok("nothing to bring in"),
        "partial" => {
            Status::warning("came in partly parametric: the notes say what was left out or fixed")
        }
        "unmapped" => {
            Status::warning("MitCad has no counterpart: a suppressed placeholder holds its place")
        }
        _ => Status::error("could not be built: a suppressed placeholder holds its place"),
    }
}

/// Rows of one kind by their status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub ok: usize,
    pub warning: usize,
    pub error: usize,
}

impl Counts {
    fn count(&mut self, status: RowStatus) {
        match status {
            RowStatus::Ok => self.ok += 1,
            RowStatus::Warning => self.warning += 1,
            RowStatus::Error => self.error += 1,
        }
    }

    fn plus(&self, other: &Counts) -> Counts {
        Counts {
            ok: self.ok + other.ok,
            warning: self.warning + other.warning,
            error: self.error + other.error,
        }
    }

    fn total(&self) -> usize {
        self.ok + self.warning + self.error
    }
}

/// What one report's rows and warnings came to: the verdict's input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    /// Timeline items, FreeCAD objects, SolidCraft features and joints.
    pub features: Counts,
    /// The features that came in parametric.
    pub parametric: usize,
    /// Bodies.
    pub bodies: Counts,
    /// The bodies compared with the file's.
    pub compared: usize,
    /// An assembly's referenced files.
    pub files: Counts,
    /// IFC products.
    pub products: Counts,
    /// Other rows (levels, blocks).
    pub others: Counts,
    /// The report's warnings (its Warnings section).
    pub warnings: usize,
}

impl Tally {
    fn plus(&self, other: &Tally) -> Tally {
        Tally {
            features: self.features.plus(&other.features),
            parametric: self.parametric + other.parametric,
            bodies: self.bodies.plus(&other.bodies),
            compared: self.compared + other.compared,
            files: self.files.plus(&other.files),
            products: self.products.plus(&other.products),
            others: self.others.plus(&other.others),
            warnings: self.warnings + other.warnings,
        }
    }

    fn rows(&self) -> Counts {
        self.features
            .plus(&self.bodies)
            .plus(&self.files)
            .plus(&self.products)
            .plus(&self.others)
    }

    /// The status of the whole: an error row makes it an error, a warning
    /// row or a warning a warning.
    pub fn status(&self) -> RowStatus {
        let rows = self.rows();
        if rows.error > 0 {
            RowStatus::Error
        } else if rows.warning + self.warnings > 0 {
            RowStatus::Warning
        } else {
            RowStatus::Ok
        }
    }

    /// The verdict's one line.
    pub fn message(&self) -> String {
        let rows = self.rows();
        let warnings = rows.warning + self.warnings;
        if rows.error > 0 {
            let mut errors: Vec<String> = Vec::new();
            if self.features.error > 0 {
                errors.push(match self.features.error {
                    1 => "1 feature could not be imported".to_owned(),
                    n => format!("{n} features could not be imported"),
                });
            }
            if self.bodies.error > 0 {
                errors.push(match self.bodies.error {
                    1 => "1 body is missing or not valid".to_owned(),
                    n => format!("{n} bodies are missing or not valid"),
                });
            }
            if self.files.error > 0 {
                errors.push(match self.files.error {
                    1 => "1 file could not be imported".to_owned(),
                    n => format!("{n} files could not be imported"),
                });
            }
            if self.products.error > 0 {
                errors.push(match self.products.error {
                    1 => "1 product could not be built".to_owned(),
                    n => format!("{n} products could not be built"),
                });
            }
            if self.others.error > 0 {
                errors.push(match self.others.error {
                    1 => "1 item could not be imported".to_owned(),
                    n => format!("{n} items could not be imported"),
                });
            }
            let errors = errors.join("; ");
            return match warnings {
                0 => format!("{errors}."),
                1 => format!("{errors}; 1 warning."),
                n => format!("{errors}; {n} warnings."),
            };
        }
        match warnings {
            0 => {}
            1 => return "Came in with 1 warning.".to_owned(),
            n => return format!("Came in with {n} warnings."),
        }
        let mut parts: Vec<String> = Vec::new();
        let features = self.features.total();
        if features > 0 {
            let left_out = features - self.parametric;
            parts.push(match (features, left_out) {
                (1, 0) => "the feature parametric".to_owned(),
                (n, 0) => format!("all {n} features parametric"),
                (_, l) => {
                    let parametric = match self.parametric {
                        1 => "1 feature parametric".to_owned(),
                        p => format!("{p} features parametric"),
                    };
                    let left = match l {
                        1 => "1 left out as the file has it".to_owned(),
                        l => format!("{l} left out as the file has it"),
                    };
                    format!("{parametric}, {left}")
                }
            });
        }
        let bodies = self.bodies.total();
        if bodies > 0 {
            parts.push(match (bodies, self.compared > 0) {
                (1, true) => "the body matches the file".to_owned(),
                (n, true) => format!("all {n} bodies match the file"),
                (1, false) => "the body is valid".to_owned(),
                (n, false) => format!("all {n} bodies are valid"),
            });
        }
        match self.files.total() {
            0 => {}
            1 => parts.push("the file found and imported".to_owned()),
            n => parts.push(format!("all {n} files found and imported")),
        }
        match self.products.total() {
            0 => {}
            1 => parts.push("the product imported".to_owned()),
            n => parts.push(format!("all {n} products imported")),
        }
        if parts.is_empty() {
            "Everything came in.".to_owned()
        } else {
            let parts = parts.join(", ");
            format!("Everything came in: {parts}.")
        }
    }

    /// The verdict as JSON.
    pub fn verdict(&self) -> Value {
        let rows = self.rows();
        json!({
            "status": self.status().as_str(),
            "message": self.message(),
            "rows": {"ok": rows.ok, "warning": rows.warning, "error": rows.error},
            "warnings": self.warnings,
        })
    }
}

/// The rows of the array `key` of `value`, none when it has none (never
/// adding the key, as indexing would).
fn rows_mut<'a>(value: &'a mut Value, key: &str) -> impl Iterator<Item = &'a mut Value> {
    value
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
}

/// The note of a row (`note`, or its `notes` joined).
fn note_of(row: &Value) -> Option<String> {
    if let Some(note) = row["note"].as_str() {
        return Some(note.to_owned());
    }
    let notes: Vec<&str> = row["notes"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    (!notes.is_empty()).then(|| notes.join("; "))
}

/// The timeline items' rows of a design's `items`.
fn annotate_items(design: &mut Value, tally: &mut Tally) {
    for item in rows_mut(design, "items") {
        let Some(outcome) = item["outcome"].as_str().map(str::to_owned) else {
            continue;
        };
        let deviates = item["accepted_deviation"].is_object();
        let status = item_status(&outcome, note_of(item).as_deref(), deviates);
        tally.features.count(status.status);
        if outcome == "parametric" && status.status == RowStatus::Ok {
            tally.parametric += 1;
        }
        status.write(item);
    }
}

/// The rows of the bodies of the file against the import's, and those of
/// the import's bodies the file does not have (`extra_body_rows`: their
/// names, without their ids). Counted when `count`.
fn annotate_bodies(design: &mut Value, tally: &mut Tally, count: bool) {
    for body in rows_mut(design, "bodies") {
        if !body.is_object() || body.get("file_volume").is_none() {
            continue;
        }
        let status = body_status(body);
        // The body by its name: the import's (after its uid) or the file's.
        let name = body["mitcad"]
            .as_str()
            .map(|m| m.split_once(' ').map_or(m, |(_, n)| n).to_owned())
            .or_else(|| body["file"].as_str().map(str::to_owned))
            .unwrap_or_default();
        body["name"] = json!(name);
        if count {
            tally.bodies.count(status.status);
            if body["volume_difference"].is_number() {
                tally.compared += 1;
            }
        }
        status.write(body);
    }
    let extra: Vec<Value> = design["extra_bodies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|e| {
            let status = Status::warning("this body is not in the file");
            if count {
                tally.bodies.count(status.status);
            }
            json!({
                "name": e.split_once(' ').map_or(e, |(_, n)| n),
                "row_status": status.status.as_str(),
                "row_status_message": status.message,
            })
        })
        .collect();
    if !extra.is_empty() {
        design["extra_body_rows"] = Value::Array(extra);
    }
}

/// The rows an `.ipt` part's import made: its bodies (`imported`), each
/// with the replay's comparison of it where there is one (`compared`, by
/// the body's uid), and the bodies it could not build (`skipped`).
fn annotate_ipt_bodies(design: &mut Value, compared: &[(String, Status)], tally: &mut Tally) {
    for body in rows_mut(design, "imported") {
        let uid = body["bodies"][0]["uid"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let comparison = compared
            .iter()
            .find(|(m, _)| !uid.is_empty() && (m == &uid || m.starts_with(&format!("{uid} "))))
            .map(|(_, s)| s);
        let status = stored_body_status(body, comparison);
        tally.bodies.count(status.status);
        if comparison.is_some() {
            tally.compared += 1;
        }
        status.write(body);
    }
    for body in rows_mut(design, "skipped") {
        if !body.is_object() {
            continue;
        }
        let status = Status::error("could not be built: the note says why");
        tally.bodies.count(status.status);
        status.write(body);
    }
}

/// The rows of one design (or report) and its verdict; `solidcraft` for a
/// SolidCraft design, whose `features` and `joints` are its rows.
fn annotate_design(design: &mut Value, solidcraft: bool) -> Tally {
    let mut tally = Tally::default();
    if !design.is_object() {
        return tally;
    }
    let ipt = design["format"] == "ipt";
    if solidcraft {
        for list in ["features", "joints"] {
            for row in rows_mut(design, list) {
                let outcome = row["outcome"].as_str().unwrap_or_default().to_owned();
                let status = solidcraft_status(&outcome);
                tally.features.count(status.status);
                if outcome == "parametric" {
                    tally.parametric += 1;
                }
                status.write(row);
            }
        }
    } else {
        annotate_items(design, &mut tally);
    }
    // An .ipt part replayed with its history: the replay's report is
    // nested; its items are the part's features, its comparison of the
    // bodies goes onto the part's body rows.
    let mut compared: Vec<(String, Status)> = Vec::new();
    if ipt && design["design"].is_object() {
        let replay = &mut design["design"];
        annotate_items(replay, &mut tally);
        annotate_bodies(replay, &mut tally, false);
        for body in replay["bodies"].as_array().into_iter().flatten() {
            if let Some(mitcad) = body["mitcad"].as_str() {
                compared.push((mitcad.to_owned(), body_status(body)));
            }
        }
        tally.warnings += replay["warnings"].as_array().map_or(0, Vec::len);
    }
    if ipt {
        annotate_ipt_bodies(design, &compared, &mut tally);
    } else {
        annotate_bodies(design, &mut tally, true);
    }
    for file in rows_mut(design, "files") {
        let status = file_status(file);
        tally.files.count(status.status);
        status.write(file);
    }
    if design["format"] == "ifc" || design["schema_name"].is_string() {
        for product in rows_mut(design, "products") {
            let outcome = product["outcome"].as_str().unwrap_or_default().to_owned();
            let status = product_status(&outcome);
            tally.products.count(status.status);
            status.write(product);
        }
    }
    // A building project's levels and a drawing's or a model's blocks:
    // what they hold came in.
    if let Some(import) = design.get_mut("import") {
        for level in rows_mut(import, "levels") {
            let status = Status::ok("came in as a component");
            tally.others.count(status.status);
            status.write(level);
        }
    }
    for component in rows_mut(design, "components") {
        let status = Status::ok("came in as a component");
        tally.others.count(status.status);
        status.write(component);
    }
    tally.warnings += design["warnings"].as_array().map_or(0, Vec::len);
    design["verdict"] = tally.verdict();
    tally
}

/// Every row of an import command's result with its status, each design
/// with its verdict, and the result with the verdict of them all.
pub fn annotate(result: &mut Value) {
    let solidcraft = result["format"] == "solidcraft";
    let Some(report) = result.get_mut("report") else {
        return;
    };
    let tally = if report["designs"].is_array() {
        let mut tally = Tally::default();
        for design in rows_mut(report, "designs") {
            tally = tally.plus(&annotate_design(design, false));
        }
        tally
    } else {
        annotate_design(report, solidcraft)
    };
    if let Some(object) = result.as_object_mut() {
        object.insert("verdict".to_owned(), tally.verdict());
    }
}

/// `text` with every `prefix`, a space, a number and `suffix` replaced by
/// `with` (the number given to it).
fn replace_numbered(
    text: &str,
    prefix: &str,
    suffix: &str,
    with: impl Fn(&str) -> String,
) -> String {
    let prefix = format!("{prefix} ");
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(&prefix) {
        let after = &rest[at + prefix.len()..];
        let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0 && after[digits..].starts_with(suffix) {
            out.push_str(&rest[..at]);
            out.push_str(&with(&after[..digits]));
            rest = &after[digits + suffix.len()..];
        } else {
            out.push_str(&rest[..at + prefix.len()]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// The tail of the note of curves put on the edges of their face.
const ON_EDGES: &str = " mm of the edges of the face it lies on put on them";

/// An import's note in the words a person reads (mitcad#1893, the
/// report's `note_message`): the history states of the file's B-rep and
/// the snapping tolerance stay in the note itself, which the log and
/// agents read.
pub fn plain_words(note: &str) -> String {
    let text = replace_numbered(
        note,
        "edges whose dressup ASM history state",
        " has",
        |_| "edges found from the file's fillet history".to_owned(),
    );
    let text = replace_numbered(&text, "edges found against ASM history state", "", |_| {
        "edges found against the file's history".to_owned()
    });
    let mut text = replace_numbered(&text, "ASM history state", "", |_| {
        "the file's history".to_owned()
    });
    // "N curves within T mm of the edges of the face it lies on put on
    // them": the count kept, the tolerance left to the log.
    while let Some(end) = text.find(ON_EDGES) {
        let head = &text[..end];
        let Some(within) = head.rfind(" curves within") else {
            break;
        };
        let count_start = head[..within]
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .len();
        let plain = match &head[count_start..within] {
            "" => break,
            "1" => "1 curve snapped onto the face's edges".to_owned(),
            n => format!("{n} curves snapped onto the face's edges"),
        };
        text = format!(
            "{}{plain}{}",
            &text[..count_start],
            &text[end + ON_EDGES.len()..]
        );
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(dv: Option<f64>, faces: (u64, u64), box_off: Option<f64>) -> Value {
        let mut b = json!({
            "file": "Body1", "file_volume": 1000.0, "mitcad": "F2.b0 Body1",
            "file_faces": faces.1, "mitcad_faces": faces.0,
        });
        if let Some(dv) = dv {
            b["volume_difference"] = json!(dv);
        }
        if let Some(d) = box_off {
            b["box_distance"] = json!(d);
        }
        b
    }

    #[test]
    fn items_get_their_status_from_the_outcome() {
        let s = |o: &str, note: Option<&str>| item_status(o, note, false).status;
        assert_eq!(s("parametric", None), RowStatus::Ok);
        assert_eq!(
            s("partial", Some("a constraint left out")),
            RowStatus::Warning
        );
        assert_eq!(s("fallback", None), RowStatus::Warning);
        assert_eq!(
            s("skipped", Some("the extrusion failed: no profile")),
            RowStatus::Error
        );
        assert_eq!(s("skipped", None), RowStatus::Error);
        // Left out as the file has it: no fault.
        assert_eq!(s("skipped", Some("suppressed in the file")), RowStatus::Ok);
        assert_eq!(
            s(
                "skipped",
                Some("no geometry: a timeline group (its items follow it)")
            ),
            RowStatus::Ok
        );
        assert_eq!(
            s(
                "skipped",
                Some("rolled back in the file (after its timeline marker)")
            ),
            RowStatus::Ok
        );
        // A parametric item whose result differs from the file's.
        assert_eq!(
            item_status("parametric", None, true).status,
            RowStatus::Warning
        );
        assert_eq!(
            item_status("parametric", None, false).message,
            "came in fully parametric"
        );
        assert_eq!(s("body", None), RowStatus::Ok);
        assert_eq!(s("included", None), RowStatus::Ok);
    }

    #[test]
    fn small_body_differences_are_ok_with_the_difference_said() {
        // +0.0002 %: the same body.
        let status = body_status(&body(Some(2e-6), (6, 6), Some(0.001)));
        assert_eq!(status.status, RowStatus::Ok);
        assert_eq!(status.message, "matches the file's body (volume +0.0002 %)");
        // No difference to speak of.
        let status = body_status(&body(Some(1e-9), (6, 6), None));
        assert_eq!(status, Status::ok("matches the file's body"));
        // Just within 0.01 %.
        assert_eq!(
            body_status(&body(Some(-9.9e-5), (6, 6), None)).status,
            RowStatus::Ok
        );
        // Past it: a warning naming the difference.
        let status = body_status(&body(Some(1.5e-4), (6, 6), None));
        assert_eq!(status.status, RowStatus::Warning);
        assert_eq!(
            status.message,
            "differs from the file's body: volume +0.0150 %"
        );
        // Other faces, or the box off past the tolerance: warnings too.
        let status = body_status(&body(Some(0.0), (40, 12), None));
        assert_eq!(
            status.message,
            "differs from the file's body: 40 faces against the file's 12"
        );
        let status = body_status(&body(Some(0.0), (6, 6), Some(0.05)));
        assert_eq!(status.status, RowStatus::Warning);
        assert_eq!(
            status.message,
            "differs from the file's body: its bounding box 0.050 mm off"
        );
        assert_eq!(
            body_status(&body(Some(0.0), (6, 6), Some(0.01))).status,
            RowStatus::Ok
        );
        // The file's body without one of the import's: an error.
        let missing = json!({"file": "Body3", "file_volume": 10.0});
        assert_eq!(body_status(&missing).status, RowStatus::Error);
    }

    #[test]
    fn stored_bodies_are_ok_when_valid_and_errors_when_not() {
        let valid = json!({"valid": true, "solid": true});
        assert_eq!(stored_body_status(&valid, None).status, RowStatus::Ok);
        let invalid = json!({"valid": false, "solid": true});
        assert_eq!(stored_body_status(&invalid, None).status, RowStatus::Error);
        let unchecked = json!({"valid": null, "solid": true});
        assert_eq!(
            stored_body_status(&unchecked, None).status,
            RowStatus::Warning
        );
        // With the replay's comparison: the worse of the two, both said.
        let differs = body_status(&body(Some(1e-3), (6, 6), None));
        let status = stored_body_status(&valid, Some(&differs));
        assert_eq!(status.status, RowStatus::Warning);
        assert_eq!(
            status.message,
            "came in as a valid body; differs from the file's body: volume +0.1000 %"
        );
        let matches = body_status(&body(Some(2e-6), (6, 6), None));
        assert_eq!(
            stored_body_status(&valid, Some(&matches)).message,
            "came in as a valid body; matches the file's body (volume +0.0002 %)"
        );
    }

    #[test]
    fn files_get_their_status() {
        let missing = json!({"found": "missing", "status": "missing", "kind": "part"});
        assert_eq!(file_status(&missing).status, RowStatus::Error);
        let failed = json!({"found": "by name", "status": "failed: no bodies", "kind": "part"});
        assert_eq!(file_status(&failed).status, RowStatus::Error);
        let replayed = json!({"found": "by name", "status": "imported", "kind": "part",
            "part": {"outcome": "replayed", "valid": 1, "solids": 1, "not_built": 0,
                     "counts": {"parametric": 2, "partial": 0, "fallback": 0, "skipped": 0}}});
        assert_eq!(file_status(&replayed).status, RowStatus::Ok);
        let fallback = json!({"found": "by name", "status": "imported", "kind": "part",
            "part": {"outcome": "bodies", "reason": "the replay failed: it hung"}});
        assert_eq!(file_status(&fallback).status, RowStatus::Warning);
        let sub =
            json!({"found": "relative to the assembly", "status": "imported", "kind": "assembly"});
        assert_eq!(file_status(&sub).status, RowStatus::Ok);
    }

    /// An `.ipt` part replayed with its history, as `import_ipt` reports it.
    fn ipt_result(outcomes: &[&str], dv: f64, warnings: &[&str]) -> Value {
        let items: Vec<Value> = outcomes
            .iter()
            .enumerate()
            .map(|(i, o)| json!({"index": i, "name": format!("Item{i}"), "type": "extrude", "outcome": o}))
            .collect();
        json!({"report": {
            "format": "ipt", "history": true,
            "imported": [{"valid": true, "solid": true, "bodies": [{"uid": "F3.b0", "name": "Body1"}]}],
            "skipped": [],
            "warnings": warnings,
            "design": {
                "items": items,
                "bodies": [{"file": "Body1", "file_volume": 1000.0, "mitcad": "F3.b0 Body1",
                            "volume_difference": dv, "file_faces": 6, "mitcad_faces": 6}],
                "extra_bodies": [],
                "warnings": [],
            },
        }})
    }

    #[test]
    fn the_verdict_says_everything_came_in() {
        let mut result = ipt_result(&["parametric"; 11], 2e-6, &[]);
        annotate(&mut result);
        assert_eq!(result["verdict"]["status"], "ok");
        assert_eq!(
            result["verdict"]["message"],
            "Everything came in: all 11 features parametric, the body matches the file."
        );
        assert_eq!(result["report"]["verdict"], result["verdict"]);
        let body = &result["report"]["imported"][0];
        assert_eq!(body["row_status"], "ok");
        assert_eq!(
            body["row_status_message"],
            "came in as a valid body; matches the file's body (volume +0.0002 %)"
        );
        assert_eq!(result["report"]["design"]["items"][0]["row_status"], "ok");
        assert_eq!(result["verdict"]["rows"]["ok"], 12);
    }

    #[test]
    fn the_verdict_counts_warnings_and_errors() {
        // Two warnings: a partial item and the report's warning.
        let mut result = ipt_result(
            &["parametric", "partial", "parametric"],
            0.0,
            &["a warning"],
        );
        annotate(&mut result);
        assert_eq!(result["verdict"]["status"], "warning");
        assert_eq!(result["verdict"]["message"], "Came in with 2 warnings.");
        assert_eq!(
            result["report"]["design"]["items"][1]["row_status"],
            "warning"
        );
        // A body past the tolerance: a warning on its row.
        let mut result = ipt_result(&["parametric"], 1.5e-4, &[]);
        annotate(&mut result);
        assert_eq!(result["report"]["imported"][0]["row_status"], "warning");
        assert_eq!(result["verdict"]["message"], "Came in with 1 warning.");
        // A skipped item: an error, the warnings counted after it.
        let mut result = ipt_result(&["parametric", "skipped", "fallback"], 0.0, &[]);
        result["report"]["design"]["items"][1]["note"] = json!("the hole failed: no circle");
        annotate(&mut result);
        assert_eq!(result["verdict"]["status"], "error");
        assert_eq!(
            result["verdict"]["message"],
            "1 feature could not be imported; 1 warning."
        );
        // A suppressed item is no error, and is said as left out.
        let mut result = ipt_result(&["parametric", "skipped"], 0.0, &[]);
        result["report"]["design"]["items"][1]["note"] = json!("suppressed in the file");
        annotate(&mut result);
        assert_eq!(
            result["verdict"]["message"],
            "Everything came in: 1 feature parametric, 1 left out as the file has it, the body matches the file."
        );
    }

    #[test]
    fn f3d_designs_get_body_rows_and_verdicts() {
        let mut result = json!({"report": {"designs": [{
            "items": [{"index": 0, "name": "Sketch1", "outcome": "parametric"}],
            "bodies": [
                {"file": "Body1", "file_volume": 1.0, "mitcad": "F2.b0 Body1", "volume_difference": 0.0},
                {"file": "Body2", "file_volume": 1.0},
            ],
            "extra_bodies": ["F9.b0 Body4"],
            "warnings": [],
        }]}});
        annotate(&mut result);
        let design = &result["report"]["designs"][0];
        assert_eq!(design["bodies"][0]["row_status"], "ok");
        assert_eq!(design["bodies"][0]["name"], "Body1");
        assert_eq!(design["bodies"][1]["row_status"], "error");
        assert_eq!(design["extra_body_rows"][0]["name"], "Body4");
        assert_eq!(design["extra_body_rows"][0]["row_status"], "warning");
        assert_eq!(
            design["verdict"]["message"],
            "1 body is missing or not valid; 1 warning."
        );
        assert_eq!(result["verdict"]["status"], "error");
    }

    #[test]
    fn an_assembly_with_a_missing_file_is_an_error() {
        let mut result = json!({"report": {"format": "iam", "files": [
            {"found": "relative to the assembly", "status": "imported", "kind": "part",
             "part": {"outcome": "bodies", "reason": "no design history", "valid": 1, "solids": 1}},
            {"found": "missing", "status": "missing", "kind": "part"},
        ], "warnings": []}});
        annotate(&mut result);
        assert_eq!(result["report"]["files"][0]["row_status"], "ok");
        assert_eq!(result["report"]["files"][1]["row_status"], "error");
        assert_eq!(
            result["verdict"]["message"],
            "1 file could not be imported."
        );
    }

    #[test]
    fn notes_are_said_in_plain_words() {
        assert_eq!(
            plain_words("edges whose dressup ASM history state 5 has"),
            "edges found from the file's fillet history"
        );
        assert_eq!(
            plain_words(
                "edges whose dressup ASM history state 12 has, one tangent chain at a time"
            ),
            "edges found from the file's fillet history, one tangent chain at a time"
        );
        assert_eq!(
            plain_words("edges found against ASM history state 3"),
            "edges found against the file's history"
        );
        assert_eq!(
            plain_words(
                "2 curves within 0.00001 mm of the edges of the face it lies on put on them"
            ),
            "2 curves snapped onto the face's edges"
        );
        assert_eq!(
            plain_words(
                "a constraint left out; 1 curves within 0.00001 mm of the edges of the face it lies on put on them"
            ),
            "a constraint left out; 1 curve snapped onto the face's edges"
        );
        // Other notes as they are.
        assert_eq!(
            plain_words("suppressed in the file"),
            "suppressed in the file"
        );
        assert_eq!(
            plain_words("state 5 of the ASM history"),
            "state 5 of the ASM history"
        );
    }
}
