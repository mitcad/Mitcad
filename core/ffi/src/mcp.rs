// SPDX-License-Identifier: MIT
//! Headless CAD tools and live GUI protocol state for local MCP transports.
//! Document IO stays in the chosen headless workspace; live tools have no IO.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use mitcad_mcp::{Backend, LiveSession, TimelineMode, ToolResult, Vocabulary};
use mitcad_model::{BlobStore, Sha256};
use mitcad_model_api::DrawingApi;
use mitcad_model_api::classes::{self, ApiClass};
use serde_json::{Map, Value, json};

use crate::Document;
use crate::kernel::OcctKernel;

const MAX_DOCUMENTS: usize = 64;
const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BLOB_BYTES: u64 = 256 * 1024 * 1024;

/// A pure edit plan: the UI retains consent, policy, backups and atomic writes.
pub fn plan_codex_mcp_config(current: &str, bridge: &str, replace_existing: bool) -> String {
    mitcad_mcp::assistant_setup::plan_codex_config(current, bridge, replace_existing).to_string()
}

pub fn verify_codex_mcp_config(before: &str, after: &str, bridge: &str) -> String {
    mitcad_mcp::assistant_setup::verify_codex_config(before, after, bridge).to_string()
}

/// Package only the application's installed native launcher, without executing it.
pub fn build_mcp_bundle(manifest: &str, launcher_path: &str) -> Result<Vec<u8>, crate::ApiError> {
    let launcher = fs::File::open(launcher_path).map_err(|error| {
        crate::ApiError(format!(
            "Cannot read the bundled assistant launcher: {error}"
        ))
    })?;
    if !launcher
        .metadata()
        .map_err(|error| crate::ApiError(error.to_string()))?
        .is_file()
    {
        return Err(crate::ApiError(
            "The bundled assistant launcher is not a regular file.".to_owned(),
        ));
    }
    let mut bytes = Vec::new();
    launcher
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            crate::ApiError(format!(
                "Cannot read the bundled assistant launcher: {error}"
            ))
        })?;
    mitcad_mcp::assistant_setup::mcp_bundle(manifest, &bytes).map_err(crate::ApiError)
}

/// Reads requests from stdin and writes only MCP messages to stdout.
pub fn run_mcp(workspace: &str, read_only: bool) -> Result<(), crate::ApiError> {
    let backend = CadBackend::new(workspace, read_only).map_err(crate::ApiError)?;
    mitcad_mcp::serve(io::stdin().lock(), io::stdout().lock(), backend)
        .map_err(|error| crate::ApiError(error.to_string()))
}

/// Protocol state only; the GUI retains its document and model worker.
pub struct LiveMcpSession(LiveSession);

pub fn new_live_mcp_session() -> Box<LiveMcpSession> {
    Box::new(LiveMcpSession(LiveSession::with_vocabulary(
        vocabulary().clone(),
    )))
}

/// The names MCP permits (mitcad#714), by their classes: what
/// `model_reference` lists and an unknown name's error names.
fn vocabulary() -> &'static Vocabulary {
    static NAMES: OnceLock<Vocabulary> = OnceLock::new();
    NAMES.get_or_init(|| {
        let owned = |names: &[&str]| -> Vec<String> {
            names.iter().map(|name| (*name).to_owned()).collect()
        };
        Vocabulary {
            queries: owned(&classes::external_queries()),
            commands: owned(&classes::external_commands()),
            features: owned(<mitcad_model::FeatureDef>::TYPE_NAMES),
        }
    })
}

impl LiveMcpSession {
    pub fn prepare_mcp(&mut self, message: &str) -> Result<String, crate::ApiError> {
        let prepared = self.0.prepare_text(message).map_err(crate::ApiError)?;
        Ok(prepared.into_plan().to_string())
    }

    pub fn finish_mcp(&mut self, value: &str, is_error: bool) -> Result<String, crate::ApiError> {
        // Parse before finishing, so a malformed application result cannot
        // silently lose the pending request ID.
        let value = serde_json::from_str(value)
            .map_err(|error| crate::ApiError(format!("invalid live MCP tool result: {error}")))?;
        self.0
            .finish(value, is_error)
            .map(|response| response.to_string())
            .map_err(crate::ApiError)
    }
}

/// Validate one model command before the GUI submits it to its worker.
pub fn validate_mcp_command(request: &str) -> Result<(), crate::ApiError> {
    let request = serde_json::from_str(request)
        .map_err(|error| crate::ApiError(format!("invalid MCP command JSON: {error}")))?;
    validate_command(&request).map_err(crate::ApiError)
}

/// Validate one model query before reading the GUI's idle document.
pub fn validate_mcp_query(request: &str) -> Result<(), crate::ApiError> {
    let request = serde_json::from_str(request)
        .map_err(|error| crate::ApiError(format!("invalid MCP query JSON: {error}")))?;
    validate_query(&request).map_err(crate::ApiError)
}

fn validate_command(request: &Value) -> Result<(), String> {
    if !request.is_object() {
        return Err("MCP commands must be single JSON objects".to_owned());
    }
    // The form of a command when its name is missing (mitcad#714).
    let kind = string(request, "cmd").map_err(|_| mitcad_mcp::missing_name(request, "cmd"))?;
    // By the command's class (mitcad#265): the model's own commands only.
    match classes::command_class_with(request) {
        Some(ApiClass::Model) => {}
        Some(ApiClass::FileIo) if classes::file_path_read(request).is_some() => {
            let path = classes::file_path_read(request).unwrap_or_default();
            // The client's texts, cut (mitcad#935): refusals are logged.
            let (kind, path) = (mitcad_mcp::echoed(kind), mitcad_mcp::echoed(&path));
            return Err(format!(
                "command '{kind}' is not available through MCP: it would read the file '{path}'; use embedded image data, stored text outlines or an installed font instead"
            ));
        }
        Some(class) => {
            return Err(format!(
                "command '{}' is not available through MCP: it is a {} command; use document_save or model_export for output",
                mitcad_mcp::echoed(kind),
                class.name()
            ));
        }
        // With the commands MCP permits (mitcad#714).
        None => return Err(mitcad_mcp::unknown_command(kind, vocabulary())),
    }
    if matches!(kind, "add_feature" | "edit_feature") {
        let definition = object(request, "def")?;
        let feature = string(definition, "type").map_err(|_| {
            "'def' needs 'type', its feature type, for example {\"type\": \"box\", ...}; model_reference without arguments lists the feature types and with feature gives one's fields".to_owned()
        })?;
        if classes::feature_class_with(definition) != ApiClass::Model {
            return Err(format!(
                "feature type '{}' is not available through MCP",
                mitcad_mcp::echoed(feature)
            ));
        }
    }
    Ok(())
}

impl Document {
    /// Audit the actual parsed model before any MCP-open recompute.
    pub fn validate_mcp_features(&self) -> Result<(), crate::ApiError> {
        for feature in self.0.features() {
            if classes::feature_class_of(&feature.def) != ApiClass::Model {
                return Err(crate::ApiError(format!(
                    "feature type '{}' is not available through MCP: remove its font file metadata or use an installed font",
                    feature.def.type_name()
                )));
            }
        }
        Ok(())
    }

    /// The live document can already hold a text whose stored font file
    /// would be read again by an otherwise model-only edit.
    pub fn validate_mcp_document_command(&self, request: &str) -> Result<(), crate::ApiError> {
        let request: Value = serde_json::from_str(request)
            .map_err(|error| crate::ApiError(format!("invalid MCP command JSON: {error}")))?;
        validate_command(&request).map_err(crate::ApiError)?;
        if classes::command_class_in(&self.0, &request) != Some(ApiClass::Model) {
            return Err(crate::ApiError(
                "command is not available through MCP: it would read a stored font file; use an installed font or stored outlines".to_owned(),
            ));
        }
        Ok(())
    }
}

fn validate_query(request: &Value) -> Result<(), String> {
    if !request.is_object() {
        return Err("MCP queries must be single JSON objects".to_owned());
    }
    let kind = string(request, "query").map_err(|_| mitcad_mcp::missing_name(request, "query"))?;
    // A name the API lacks, with the queries MCP permits (mitcad#714).
    if classes::query_class(kind).is_none() {
        return Err(mitcad_mcp::unknown_query(kind, vocabulary()));
    }
    // By the query's class (mitcad#265); the result store's files are not
    // read either.
    if !classes::external_query_allowed(kind)
        || (kind == "cache" && request["disk"].as_bool() == Some(true))
    {
        return Err(format!(
            "query '{}' is not available through MCP",
            mitcad_mcp::echoed(kind)
        ));
    }
    Ok(())
}

struct OpenDocument {
    document: Box<Document>,
    path: Option<PathBuf>,
}

struct CadBackend {
    workspace: Workspace,
    read_only: bool,
    documents: BTreeMap<String, OpenDocument>,
    next_document: u64,
}

impl CadBackend {
    fn new(workspace: &str, read_only: bool) -> Result<Self, String> {
        Ok(Self {
            workspace: Workspace::new(workspace)?,
            read_only,
            documents: BTreeMap::new(),
            next_document: 1,
        })
    }

    fn require_writable(&self) -> Result<(), String> {
        if self.read_only {
            Err("this MCP server is read-only".to_owned())
        } else {
            Ok(())
        }
    }

    fn document(&self, id: &str) -> Result<&OpenDocument, String> {
        self.documents
            .get(id)
            .ok_or_else(|| format!("no open document '{id}'"))
    }

    fn document_mut(&mut self, id: &str) -> Result<&mut OpenDocument, String> {
        self.documents
            .get_mut(id)
            .ok_or_else(|| format!("no open document '{id}'"))
    }

    /// Opens `document` under a new handle. `failed` is the opening
    /// recompute's `failed_features`: listed, not an error (mitcad#401).
    fn insert(
        &mut self,
        document: Box<Document>,
        path: Option<PathBuf>,
        failed: Value,
    ) -> Result<ToolResult, String> {
        if self.documents.len() >= MAX_DOCUMENTS {
            return Err(format!(
                "at most {MAX_DOCUMENTS} documents may be open; close one first"
            ));
        }
        let id = format!("D{}", self.next_document);
        self.next_document = self
            .next_document
            .checked_add(1)
            .ok_or_else(|| "document handle space is exhausted".to_owned())?;
        let mut value = snapshot(&document)?;
        value["document"] = json!(id);
        value["path"] = json!(path.as_ref().map(|p| p.to_string_lossy().into_owned()));
        value["applied"] = json!(true);
        value["failed_features"] = if failed.is_array() { failed } else { json!([]) };
        self.documents.insert(id, OpenDocument { document, path });
        success(value)
    }

    /// A SolidCraft design (mitcad#450) imported with its history into a
    /// new document, which has no path until it is saved.
    fn open_solidcraft(&mut self, path: &Path) -> Result<ToolResult, String> {
        // Within the limit before the import reads it.
        read_bounded(path, MAX_DOCUMENT_BYTES).map_err(|e| e.to_string())?;
        let mut document = crate::new_document();
        document
            .import_solidcraft_command(json!({"path": path.to_string_lossy()}))
            .map_err(|e| e.to_string())?;
        let recomputed = parse_result(
            &document
                .command(r#"{"cmd":"recompute"}"#)
                .map_err(|e| e.to_string())?,
        )?;
        self.insert(document, None, recomputed["failed_features"].clone())
    }

    fn open(&mut self, path: &str) -> Result<ToolResult, String> {
        if self.documents.len() >= MAX_DOCUMENTS {
            return Err(format!(
                "at most {MAX_DOCUMENTS} documents may be open; close one first"
            ));
        }
        let path = self.workspace.read_path(path)?;
        if path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("solidcraft"))
        {
            return self.open_solidcraft(&path);
        }
        require_extension(&path, "mitcad")?;
        let bytes = read_bounded(&path, MAX_DOCUMENT_BYTES).map_err(|e| e.to_string())?;
        let text =
            String::from_utf8(bytes).map_err(|e| format!("design file is not UTF-8: {e}"))?;
        // The model's ordinary load_design also reads per-user display
        // state and finds projects above the workspace. Use only this
        // explicit, read-only store, never refresh links on opening.
        let project = self.workspace.project_for(&path)?;
        let model = match project {
            Some(project) => {
                let store = ConfinedStore {
                    workspace: &self.workspace,
                    project,
                };
                // Preflight every reference: model resolution otherwise
                // turns an IO refusal into an ordinary load warning.
                for hash in mitcad_model::file::brep_references(&text).map_err(|e| e.to_string())? {
                    store.get(&hash).map_err(|e| e.to_string())?;
                }
                mitcad_model::Document::from_json_in(&text, &store, OcctKernel)
            }
            None => mitcad_model::Document::from_json(&text, OcctKernel),
        }
        .map_err(|e| e.to_string())?;
        // Apply the same evaluator audit to loaded files as to newly
        // added definitions. Existing links remain stored metadata and
        // their embedded base bodies; none are refreshed here.
        for feature in model.features() {
            if classes::feature_class_of(&feature.def) != ApiClass::Model {
                return Err(format!(
                    "feature type '{}' is not available through MCP",
                    feature.def.type_name()
                ));
            }
        }
        let mut document = Box::new(Document(model));
        let recomputed = parse_result(
            &document
                .command(r#"{"cmd":"recompute"}"#)
                .map_err(|e| e.to_string())?,
        )?;
        document
            .command(r#"{"cmd":"mark_saved"}"#)
            .map_err(|e| e.to_string())?;
        self.insert(document, Some(path), recomputed["failed_features"].clone())
    }
}

impl Backend for CadBackend {
    fn vocabulary(&self) -> Vocabulary {
        vocabulary().clone()
    }

    fn call(&mut self, name: &str, arguments: &Value) -> Result<ToolResult, String> {
        match name {
            "document_create" => {
                self.require_writable()?;
                // A new design as File > New makes it (mitcad#220).
                self.insert(crate::new_design_document(), None, json!([]))
            }
            "document_open" => self.open(string(arguments, "path")?),
            "document_list" => {
                let documents = self
                    .documents
                    .iter()
                    .map(|(id, opened)| {
                        Ok(json!({"document": id,
                            "path": opened.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                            "state": query(&opened.document, "document")?}))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                success(json!({"documents": documents}))
            }
            "document_close" => {
                let id = string(arguments, "document")?;
                let opened = self.document(id)?;
                let modified = query(&opened.document, "document")?["modified"]
                    .as_bool()
                    .unwrap_or(true);
                if modified && !boolean(arguments, "discard", false)? {
                    return Err(
                        "document has unsaved changes; save it or set discard to true".to_owned(),
                    );
                }
                self.documents.remove(id);
                success(json!({"document": id, "closed": true}))
            }
            "model_query" => {
                let id = string(arguments, "document")?;
                let request = object(arguments, "query")?;
                validate_query(request)?;
                let result = self
                    .document(id)?
                    .document
                    .query(&request.to_string())
                    .map_err(|e| e.to_string())?;
                success(json!({"document": id, "result": parse_result(&result)?}))
            }
            "model_command" => {
                self.require_writable()?;
                let id = string(arguments, "document")?;
                let mode = TimelineMode::of(arguments)?;
                let array = arguments["command"].is_array();
                let entries = command_entries(arguments)?;
                let opened = self.document_mut(id)?;
                // Every entry is checked before any runs (mitcad#689).
                for (i, entry) in entries.iter().enumerate() {
                    let checked = validate_command(entry).and_then(|()| {
                        opened
                            .document
                            .validate_mcp_document_command(&entry.to_string())
                            .map_err(|e| e.to_string())
                    });
                    match checked {
                        Ok(()) => {}
                        Err(error) if !array => return Err(error),
                        Err(error) => return Ok(refused_entry(id, i, &error, 0)),
                    }
                }
                run_commands(id, &mut opened.document, &entries, array, mode)
            }
            "document_save" => {
                self.require_writable()?;
                let id = string(arguments, "document")?;
                let overwrite = boolean(arguments, "overwrite", false)?;
                let path = self
                    .workspace
                    .write_path(string(arguments, "path")?, overwrite)?;
                require_extension(&path, "mitcad")?;
                let opened = self.document_mut(id)?;
                if !mitcad_model::file::brep_references(&opened.document.to_json())
                    .map_err(|e| e.to_string())?
                    .is_empty()
                {
                    return Err("document has unresolved B-rep references and cannot be saved as a single file".to_owned());
                }
                save_single(&path, &opened.document.to_json()).map_err(|e| e.to_string())?;
                opened
                    .document
                    .command(r#"{"cmd":"mark_saved"}"#)
                    .map_err(|e| e.to_string())?;
                opened.path = Some(path.clone());
                success(json!({"document": id, "path": path,
                    "state": query(&opened.document, "document")?}))
            }
            "model_export" => {
                self.require_writable()?;
                let id = string(arguments, "document")?;
                let path = self
                    .workspace
                    .write_path(string(arguments, "path")?, false)?;
                let plan = export_plan(arguments, &path)?;
                for extra in &plan.extra {
                    self.workspace.write_path(path_text(extra)?, false)?;
                }
                let opened = self.document(id)?;
                // A drawing's title block names the design by its file;
                // USDZ texture paths are relative to the design's folder.
                let title = opened
                    .path
                    .as_deref()
                    .and_then(|p| p.file_stem())
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_owned();
                let folder = match opened.path.as_deref().and_then(|p| p.parent()) {
                    Some(folder) => Some(path_text(folder)?.to_owned()),
                    None => None,
                };
                let mut command = plan.command;
                prepare_export(&opened.document, &mut command, &title, folder.as_deref())?;
                if command["cmd"] == "export_drawing" {
                    let result = opened
                        .document
                        .0
                        .export_drawing_checked(&command.to_string(), |output| {
                            self.workspace.write_path(output, false).map(|_| ())
                        })
                        .map_err(|e| e.to_string())?;
                    return success(json!({"document": id, "path": path, "result": result}));
                }
                let opened = self.document_mut(id)?;
                let result = opened
                    .document
                    .command(&command.to_string())
                    .map_err(|e| e.to_string())?;
                success(json!({"document": id, "path": path, "result": parse_result(&result)?}))
            }
            _ => Err(format!("unknown tool '{name}'")),
        }
    }
}

/// The commands of a `model_command` call: its object, or each of its
/// array (mitcad#689). Undo and redo run alone.
fn command_entries(arguments: &Value) -> Result<Vec<&Value>, String> {
    match &arguments["command"] {
        Value::Array(entries) => {
            if entries.is_empty() {
                return Err("'command' must not be an empty array".to_owned());
            }
            for (i, entry) in entries.iter().enumerate() {
                if !entry.is_object() {
                    return Err(format!("commands[{i}] must be a JSON object"));
                }
                if entries.len() > 1 && matches!(entry["cmd"].as_str(), Some("undo" | "redo")) {
                    return Err(format!(
                        "commands[{i}]: undo and redo run alone, not in a command array"
                    ));
                }
            }
            Ok(entries.iter().collect())
        }
        command if command.is_object() => Ok(vec![command]),
        _ => Err("'command' must be an object or an array of objects".to_owned()),
    }
}

/// The document as a `model_command` result compares it
/// (`mitcad_mcp::SNAPSHOT_QUERIES`).
fn command_snapshot(document: &Document) -> Result<Value, String> {
    let mut snapshot = Map::new();
    for (key, request) in mitcad_mcp::SNAPSHOT_QUERIES {
        let answer = document.query(request).map_err(|e| e.to_string())?;
        snapshot.insert(key.to_owned(), parse_result(&answer)?);
    }
    Ok(Value::Object(snapshot))
}

fn undo_depth(document: &Document) -> Result<u64, String> {
    Ok(query(document, "document")?["undo_depth"]
        .as_u64()
        .unwrap_or_default())
}

/// Undoes the steps after `depth` (a refused array entry, mitcad#689);
/// how many.
fn undo_to(document: &mut Document, depth: u64) -> Result<usize, String> {
    let mut undone = 0;
    loop {
        let now = undo_depth(document)?;
        if now <= depth {
            return Ok(undone);
        }
        document
            .command(r#"{"cmd":"undo"}"#)
            .map_err(|e| e.to_string())?;
        undone += 1;
        if undo_depth(document)? >= now {
            return Err("an entry of the array could not be undone".to_owned());
        }
    }
}

/// The error result of an array whose entry `i` was refused (mitcad#689):
/// nothing of it stays, `undone` steps of the entries before it undone.
fn refused_entry(id: &str, i: usize, error: &str, undone: usize) -> ToolResult {
    ToolResult {
        value: json!({
            "document": id,
            "applied": false,
            "error": format!("commands[{i}]: {error}; the {undone} step(s) of the entries before it were undone"),
            "entry": i,
            "undone": undone,
        }),
        is_error: true,
    }
}

/// Runs a `model_command`'s commands in order (mitcad#689): an array
/// stops at the first refused entry, whose earlier entries are undone, or
/// after the first entry whose added or edited feature fails, and its steps
/// become one undo step. The result holds the document by `mode`.
fn run_commands(
    id: &str,
    document: &mut Document,
    entries: &[&Value],
    array: bool,
    mode: TimelineMode,
) -> Result<ToolResult, String> {
    let before = if mode.needs_before() {
        Some(command_snapshot(document)?)
    } else {
        None
    };
    let depth = undo_depth(document)?;
    let mut results = Vec::with_capacity(entries.len());
    let mut times = Vec::new();
    let mut stopped = None;
    for (i, entry) in entries.iter().enumerate() {
        let text = match document.command(&entry.to_string()) {
            Ok(text) => text,
            Err(error) if !array => return Err(error.to_string()),
            Err(error) => {
                let undone = undo_to(document, depth)?;
                return Ok(refused_entry(id, i, &error.to_string(), undone));
            }
        };
        let result = parse_result(&text)?;
        if mitcad_mcp::recomputed(&result) {
            times.push(query(document, "recompute_times")?);
        }
        let failed = mitcad_mcp::entry_failed(&result);
        results.push(result);
        if failed && i + 1 < entries.len() {
            stopped = Some(i);
            break;
        }
    }
    // The array's steps as one, named as its last.
    let state = query(document, "document")?;
    if array && state["undo_depth"].as_u64().unwrap_or_default() > depth + 1 {
        let label = state["undo"].as_str().unwrap_or("Commands").to_owned();
        document
            .command(&json!({"cmd": "merge_undo", "depth": depth, "label": label}).to_string())
            .map_err(|e| e.to_string())?;
    }
    let after = command_snapshot(document)?;
    let evaluated = mitcad_mcp::evaluated_features(&times);
    let mut value = Value::Object(mitcad_mcp::command_document(
        mode,
        before.as_ref(),
        &after,
        &evaluated,
    ));
    value["document"] = json!(id);
    value["applied"] = json!(true);
    value["result"] = results.last().cloned().unwrap_or(Value::Null);
    if array {
        value["ran"] = json!(results.len());
        value["results"] = Value::Array(results);
        if let Some(i) = stopped {
            value["stopped_at"] = json!(i);
        }
    }
    // An error only when a feature the (last) command added or edited
    // fails; the others are listed (mitcad#401).
    let is_error = mitcad_mcp::command_outcome(&mut value);
    Ok(ToolResult { value, is_error })
}

/// A `model_export` call's model command writing to `path`, and the other
/// files it writes (an OBJ's .mtl beside it); the same in the headless and
/// the live server (mitcad#689). A drawing's further sheets are checked as
/// the drawing names them.
struct ExportPlan {
    command: Value,
    extra: Vec<PathBuf>,
}

fn export_plan(arguments: &Value, path: &Path) -> Result<ExportPlan, String> {
    let format = string(arguments, "format")?;
    if !matches!(
        format,
        "step" | "iges" | "brep" | "stl" | "obj" | "3mf" | "usdz" | "dxf" | "pdf" | "svg"
    ) {
        return Err(format!("unknown export format '{format}'"));
    }
    // A cut sheet kept in the design (mitcad#853): DXF or SVG of its parts'
    // outlines, their names on the layer LABEL with labels.
    if arguments.get("cut_sheet").is_some() {
        if !matches!(format, "dxf" | "svg") {
            return Err("a cut sheet is written as DXF or SVG".to_owned());
        }
        if let Some(key) = ["drawing", "sketch", "flat_pattern", "bodies"]
            .into_iter()
            .find(|k| arguments.get(*k).is_some())
        {
            return Err(format!("cut_sheet does not take '{key}'"));
        }
        let cut_sheet = string(arguments, "cut_sheet")?;
        let labels = match arguments.get("labels") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(labels)) => *labels,
            Some(_) => return Err("labels must be true or false".to_owned()),
        };
        require_extension(path, format)?;
        let text = path_text(path)?;
        return Ok(ExportPlan {
            command: json!({"cmd": "export_cut_sheet", "path": text, "drawing": cut_sheet,
                            "labels": labels}),
            extra: Vec::new(),
        });
    }
    if arguments.get("labels").is_some() {
        return Err("labels is only accepted with cut_sheet".to_owned());
    }
    // Drawings (mitcad#471): PDF and SVG, and DXF with drawing.
    let drawing = match arguments.get("drawing") {
        Some(options) => Some(
            options
                .as_object()
                .ok_or_else(|| "drawing must be an object".to_owned())?
                .clone(),
        ),
        None if matches!(format, "pdf" | "svg") => Some(Map::new()),
        None => None,
    };
    if let Some(options) = &drawing {
        if !matches!(format, "pdf" | "svg" | "dxf") {
            return Err("drawing is only accepted for PDF, SVG or DXF export".to_owned());
        }
        if arguments.get("sketch").is_some() || arguments.get("flat_pattern").is_some() {
            return Err("a drawing uses bodies, not sketch or flat_pattern".to_owned());
        }
        if let Some(key) = ["cmd", "path", "bodies", "format"]
            .into_iter()
            .find(|k| options.contains_key(*k))
        {
            return Err(format!("drawing must not hold '{key}'"));
        }
    }
    // DXF: a sketch, or a sheet body's Flat Pattern (mitcad#210).
    let (sketch, flat) = if format == "dxf" && drawing.is_none() {
        if arguments.get("bodies").is_some() {
            return Err("DXF export uses sketch or flat_pattern, not bodies".to_owned());
        }
        match (arguments.get("sketch"), arguments.get("flat_pattern")) {
            (Some(_), None) => (Some(string(arguments, "sketch")?), None),
            (None, Some(_)) => (None, Some(string(arguments, "flat_pattern")?)),
            _ => {
                return Err("DXF export needs either sketch or flat_pattern".to_owned());
            }
        }
    } else {
        if arguments.get("sketch").is_some() || arguments.get("flat_pattern").is_some() {
            return Err("sketch and flat_pattern are only accepted for DXF export".to_owned());
        }
        (None, None)
    };
    let bodies = arguments.get("bodies");
    if let Some(bodies) = bodies
        && !bodies
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string))
    {
        return Err("bodies must be an array of strings".to_owned());
    }
    let text = path_text(path)?;
    let mut extra = Vec::new();
    let mut command = if let Some(options) = drawing {
        require_extension(path, format)?;
        let mut command = Value::Object(options);
        command["cmd"] = json!("export_drawing");
        command["path"] = json!(text);
        command
    } else if let Some(sketch) = sketch {
        require_extension(path, "dxf")?;
        json!({"cmd": "export_sketch", "path": text, "sketch": sketch})
    } else if let Some(body) = flat {
        require_extension(path, "dxf")?;
        json!({"cmd": "export_flat_pattern_dxf", "path": text, "body": body})
    } else {
        if format == "obj" {
            // OCCT also writes <name>.mtl beside an OBJ.
            require_extension(path, "obj")?;
            extra.push(path.with_extension("mtl"));
        }
        json!({"cmd": "export", "path": text, "format": format})
    };
    if let Some(bodies) = bodies {
        command["bodies"] = bodies.clone();
    }
    Ok(ExportPlan { command, extra })
}

/// Completes a `model_export`'s command for `document` and refuses what
/// MCP must not export (mitcad#689: both servers): a drawing's title block
/// names the design (`title`); a USDZ's texture paths are relative to the
/// design's folder, and only textures embedded in the design are read; a
/// document with failed features is not exported.
fn prepare_export(
    document: &Document,
    command: &mut Value,
    title: &str,
    design_folder: Option<&str>,
) -> Result<(), String> {
    if command["cmd"] == "export_drawing" && command.get("title").is_none() {
        command["title"] = json!(title);
    }
    if command["cmd"] == "export" && command["format"] == "usdz" {
        if let Some(folder) = design_folder {
            command["design_folder"] = json!(folder);
        }
        for appearance in document.0.custom_appearances() {
            if let Some(texture) = &appearance.texture
                && texture.slots().any(|(_, _, data)| data.is_none())
            {
                return Err("USDZ export through MCP requires embedded texture images; external texture files are not read".to_owned());
            }
        }
    }
    if geometry_failed(&snapshot(document)?) {
        return Err("document has failed features; inspect and fix them before export".to_owned());
    }
    Ok(())
}

impl Document {
    /// A live `model_export` (mitcad#689): the model command that writes
    /// `arguments`' export to `path`, which the application found to be a
    /// new file in a folder for designs, with the other files it writes
    /// (`outputs`, to be checked the same way) and whether it is a drawing
    /// ([`Document::mcp_export_drawing`] writes those). `title` names the
    /// design in a drawing's title block; `design_folder` is the design's
    /// folder (empty for an unsaved design).
    pub fn mcp_export_plan(
        &self,
        arguments: &str,
        path: &str,
        title: &str,
        design_folder: &str,
    ) -> Result<String, crate::ApiError> {
        let arguments: Value = serde_json::from_str(arguments)
            .map_err(|error| crate::ApiError(format!("invalid MCP export JSON: {error}")))?;
        let plan = export_plan(&arguments, Path::new(path)).map_err(crate::ApiError)?;
        let mut command = plan.command;
        let folder = (!design_folder.is_empty()).then_some(design_folder);
        prepare_export(self, &mut command, title, folder).map_err(crate::ApiError)?;
        let outputs = plan
            .extra
            .iter()
            .map(|extra| path_text(extra).map(str::to_owned))
            .collect::<Result<Vec<_>, _>>()
            .map_err(crate::ApiError)?;
        let drawing = command["cmd"] == "export_drawing";
        Ok(json!({"command": command, "outputs": outputs, "drawing": drawing}).to_string())
    }

    /// Writes a live drawing export's files (mitcad#689): each, the further
    /// sheets too, must be a new file beside the first, whose folder the
    /// application checked; nothing is written before every one is.
    pub fn mcp_export_drawing(&self, command: &str) -> Result<String, crate::ApiError> {
        let request: Value = serde_json::from_str(command)
            .map_err(|error| crate::ApiError(format!("invalid MCP export JSON: {error}")))?;
        let first = request["path"]
            .as_str()
            .ok_or_else(|| crate::ApiError("the drawing has no path".to_owned()))?;
        let folder = Path::new(first).parent().map(Path::to_path_buf);
        let result = self
            .0
            .export_drawing_checked(command, |output| {
                let output = Path::new(output);
                if output.parent().map(Path::to_path_buf) != folder {
                    return Err(format!("{} is not beside {first}", output.display()));
                }
                match fs::symlink_metadata(output) {
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(format!("cannot inspect {}: {e}", output.display())),
                    Ok(_) => Err(format!(
                        "{} already exists; model_export writes new files only",
                        output.display()
                    )),
                }
            })
            .map_err(|e| crate::ApiError(e.to_string()))?;
        Ok(result.to_string())
    }
}

/// The queries of a live `model_command`'s snapshots, by the key each
/// answer goes under (mitcad#689, `mitcad_mcp::SNAPSHOT_QUERIES`).
pub fn mcp_snapshot_queries() -> String {
    let mut queries = Map::new();
    for (key, request) in mitcad_mcp::SNAPSHOT_QUERIES {
        let request: Value = serde_json::from_str(request).expect("a query of our own");
        queries.insert(key.to_owned(), request);
    }
    Value::Object(queries).to_string()
}

/// The document's part of a live `model_command`'s result (mitcad#689):
/// `mode` (`changed`, `full`, `none`), the snapshots `before` (empty when
/// the mode needs none) and `after`, and `times`, the `recompute_times`
/// answers of the commands that recomputed (a JSON array).
pub fn mcp_command_document(
    mode: &str,
    before: &str,
    after: &str,
    times: &str,
) -> Result<String, crate::ApiError> {
    let read = |text: &str| -> Result<Value, crate::ApiError> {
        serde_json::from_str(text)
            .map_err(|error| crate::ApiError(format!("invalid MCP snapshot JSON: {error}")))
    };
    let mode = TimelineMode::parse(mode).map_err(crate::ApiError)?;
    let before = if before.is_empty() {
        None
    } else {
        Some(read(before)?)
    };
    let after = read(after)?;
    let times = read(times)?;
    let evaluated = mitcad_mcp::evaluated_features(times.as_array().into_iter().flatten());
    Ok(Value::Object(mitcad_mcp::command_document(
        mode,
        before.as_ref(),
        &after,
        &evaluated,
    ))
    .to_string())
}

/// Whether a live command array stops after an entry whose model result
/// is `result` (mitcad#689): a feature it added or edited failed.
pub fn mcp_entry_failed(result: &str) -> bool {
    serde_json::from_str::<Value>(result).is_ok_and(|result| mitcad_mcp::entry_failed(&result))
}

/// Whether a model result says the command recomputed, so that the
/// `recompute_times` query tells what (mitcad#689).
pub fn mcp_recomputed(result: &str) -> bool {
    serde_json::from_str::<Value>(result).is_ok_and(|result| mitcad_mcp::recomputed(&result))
}

fn success(value: Value) -> Result<ToolResult, String> {
    Ok(ToolResult {
        value,
        is_error: false,
    })
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("'{field}' must be a nonempty string"))
}

fn object<'a>(value: &'a Value, field: &str) -> Result<&'a Value, String> {
    value
        .get(field)
        .filter(|v| v.is_object())
        .ok_or_else(|| format!("'{field}' must be an object"))
}

fn boolean(value: &Value, field: &str, default: bool) -> Result<bool, String> {
    match value.get(field) {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("'{field}' must be a boolean")),
    }
}

fn parse_result(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|e| format!("model returned invalid JSON: {e}"))
}

fn query(document: &Document, kind: &str) -> Result<Value, String> {
    parse_result(
        &document
            .query(&json!({"query": kind}).to_string())
            .map_err(|e| e.to_string())?,
    )
}

fn snapshot(document: &Document) -> Result<Value, String> {
    Ok(json!({"state": query(document, "document")?,
        "timeline": query(document, "timeline")?, "bodies": query(document, "bodies")?}))
}

fn geometry_failed(value: &Value) -> bool {
    value["timeline"]["features"]
        .as_array()
        .is_some_and(|features| {
            features
                .iter()
                .any(|feature| feature["status"].as_str() == Some("error"))
        })
}

struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn new(path: &str) -> Result<Self, String> {
        let root = fs::canonicalize(path).map_err(|e| format!("cannot open workspace: {e}"))?;
        path_text(&root)?;
        if !root.is_dir() {
            return Err("workspace must be an existing directory".to_owned());
        }
        Ok(Self { root })
    }

    fn candidate(&self, path: &str) -> Result<PathBuf, String> {
        let path = Path::new(path);
        if path.as_os_str().is_empty()
            || path.components().any(|c| matches!(c, Component::ParentDir))
        {
            return Err("paths must be nonempty and cannot contain '..'".to_owned());
        }
        Ok(if path.is_absolute() {
            path.to_owned()
        } else {
            self.root.join(path)
        })
    }

    fn confined(&self, path: PathBuf) -> Result<PathBuf, String> {
        path_text(&path)?;
        if path.starts_with(&self.root) {
            Ok(path)
        } else {
            Err("path leaves the MCP workspace".to_owned())
        }
    }

    fn read_path(&self, path: &str) -> Result<PathBuf, String> {
        let path = self.candidate(path)?;
        let path = self.confined(
            fs::canonicalize(&path).map_err(|e| format!("cannot open {}: {e}", path.display()))?,
        )?;
        if !path.is_file() {
            return Err("input must be a regular file".to_owned());
        }
        Ok(path)
    }

    fn write_path(&self, path: &str, overwrite: bool) -> Result<PathBuf, String> {
        let candidate = self.candidate(path)?;
        let name = candidate
            .file_name()
            .ok_or_else(|| "output must have a file name".to_owned())?;
        let parent = candidate
            .parent()
            .ok_or_else(|| "output must have a parent directory".to_owned())?;
        let parent = self.confined(
            fs::canonicalize(parent).map_err(|e| format!("output directory must exist: {e}"))?,
        )?;
        if !parent.is_dir() {
            return Err("output parent must be a directory".to_owned());
        }
        let path = parent.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                // Canonicalize even if overwrite is refused, so dangling
                // symlinks cannot be mistaken for absent output files.
                let resolved = self.confined(
                    fs::canonicalize(&path).map_err(|e| format!("cannot resolve output: {e}"))?,
                )?;
                if !resolved.is_file() {
                    return Err("output must be a regular file".to_owned());
                }
                if !overwrite {
                    return Err(
                        "output already exists; document_save requires overwrite: true".to_owned(),
                    );
                }
                Ok(resolved)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(path),
            Err(e) => Err(format!("cannot inspect output: {e}")),
        }
    }

    fn project_for(&self, file: &Path) -> Result<Option<PathBuf>, String> {
        let mut parent = file.parent();
        while let Some(directory) = parent {
            if !directory.starts_with(&self.root) {
                break;
            }
            let marker = directory.join(mitcad_model::file::PROJECT_MARKER);
            match fs::symlink_metadata(&marker) {
                Ok(_) => {
                    self.read_path(path_text(&marker)?)?;
                    return Ok(Some(directory.to_owned()));
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("cannot inspect project marker: {e}")),
            }
            if directory == self.root {
                break;
            }
            parent = directory.parent();
        }
        Ok(None)
    }
}

struct ConfinedStore<'a> {
    workspace: &'a Workspace,
    project: PathBuf,
}

impl BlobStore for ConfinedStore<'_> {
    fn get(&self, hash: &Sha256) -> io::Result<Option<Vec<u8>>> {
        let candidate = self.project.join(mitcad_model::file::brep_path(hash));
        match fs::symlink_metadata(&candidate) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
            Ok(_) => {
                let path = self
                    .workspace
                    .read_path(path_text(&candidate).map_err(io::Error::other)?)
                    .map_err(io::Error::other)?;
                read_bounded(&path, MAX_BLOB_BYTES).map(Some)
            }
        }
    }

    fn contains(&self, hash: &Sha256) -> io::Result<bool> {
        self.get(hash).map(|value| value.is_some())
    }

    fn put(&self, _hash: &Sha256, _content: &[u8]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the MCP project store is read-only",
        ))
    }
}

fn read_bounded(path: &Path, maximum: u64) -> io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > maximum {
        return Err(io::Error::other(format!(
            "{} exceeds the MCP file size limit",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(io::Error::other("file exceeds the MCP file size limit"));
    }
    Ok(bytes)
}

fn save_single(path: &Path, text: &str) -> io::Result<()> {
    // Unlike the shared model save helper, exclusive creation cannot
    // follow a pre-existing symlink at the temporary file's name.
    static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing output parent"))?;
    let mut opened = None;
    for _ in 0..32 {
        let temporary = parent.join(format!(
            ".mitcad-mcp.{}.{}.tmp",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => {
                opened = Some((temporary, file));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let (temporary, mut file) =
        opened.ok_or_else(|| io::Error::other("cannot create a unique save file"))?;
    let written = (|| {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        mitcad_model::file::rename_retrying(&temporary, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(temporary);
    }
    written
}

fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| "MCP paths must be valid UTF-8".to_owned())
}

fn require_extension(path: &Path, expected: &str) -> Result<(), String> {
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case(expected))
    {
        Ok(())
    } else {
        Err(format!("output/input must have the .{expected} extension"))
    }
}
