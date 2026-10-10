// SPDX-License-Identifier: MIT
//! Headless CAD tools for the local MCP transport. Document IO stays in
//! the chosen workspace; imports and external-link refresh are not tools.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mitcad_mcp::{Backend, ToolResult};
use mitcad_model::{BlobStore, Sha256};
use serde_json::{Value, json};

use crate::Document;
use crate::kernel::OcctKernel;

const MAX_DOCUMENTS: usize = 64;
const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BLOB_BYTES: u64 = 256 * 1024 * 1024;

/// Reads requests from stdin and writes only MCP messages to stdout.
pub fn run_mcp(workspace: &str, read_only: bool) -> Result<(), crate::ApiError> {
    let backend = CadBackend::new(workspace, read_only).map_err(crate::ApiError)?;
    mitcad_mcp::serve(io::stdin().lock(), io::stdout().lock(), backend)
        .map_err(|error| crate::ApiError(error.to_string()))
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

    fn insert(
        &mut self,
        document: Box<Document>,
        path: Option<PathBuf>,
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
        let is_error = geometry_failed(&value);
        self.documents.insert(id, OpenDocument { document, path });
        Ok(ToolResult { value, is_error })
    }

    fn open(&mut self, path: &str) -> Result<ToolResult, String> {
        if self.documents.len() >= MAX_DOCUMENTS {
            return Err(format!(
                "at most {MAX_DOCUMENTS} documents may be open; close one first"
            ));
        }
        let path = self.workspace.read_path(path)?;
        require_extension(&path, "mitcad")?;
        let bytes = read_bounded(&path, MAX_DOCUMENT_BYTES).map_err(|e| e.to_string())?;
        let text =
            String::from_utf8(bytes).map_err(|e| format!("project file is not UTF-8: {e}"))?;
        // The model's ordinary load_project also reads per-user display
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
            if !safe_feature(feature.def.type_name()) {
                return Err(format!(
                    "feature type '{}' is not available through MCP",
                    feature.def.type_name()
                ));
            }
        }
        let mut document = Box::new(Document(model));
        document
            .command(r#"{"cmd":"recompute"}"#)
            .map_err(|e| e.to_string())?;
        document
            .command(r#"{"cmd":"mark_saved"}"#)
            .map_err(|e| e.to_string())?;
        self.insert(document, Some(path))
    }
}

impl Backend for CadBackend {
    fn call(&mut self, name: &str, arguments: &Value) -> Result<ToolResult, String> {
        match name {
            "document_create" => {
                self.require_writable()?;
                self.insert(crate::new_document(), None)
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
                let kind = string(request, "query")?;
                if !safe_query(kind) || (kind == "cache" && request["disk"].as_bool() == Some(true))
                {
                    return Err(format!("query '{kind}' is not available through MCP"));
                }
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
                let request = object(arguments, "command")?;
                let kind = string(request, "cmd")?;
                if !safe_command(kind) {
                    return Err(format!(
                        "command '{kind}' is not available through MCP; use document_save or model_export for output"
                    ));
                }
                if matches!(kind, "add_feature" | "edit_feature") {
                    let definition = object(request, "def")?;
                    let feature = string(definition, "type")?;
                    if !safe_feature(feature) {
                        return Err(format!(
                            "feature type '{feature}' is not available through MCP"
                        ));
                    }
                }
                let opened = self.document_mut(id)?;
                let result = opened
                    .document
                    .command(&request.to_string())
                    .map_err(|e| e.to_string())?;
                let result = parse_result(&result)?;
                let mut value = snapshot(&opened.document)?;
                value["document"] = json!(id);
                value["applied"] = json!(true);
                let is_error =
                    geometry_failed(&value) || result.get("error").is_some_and(|e| !e.is_null());
                value["result"] = result;
                Ok(ToolResult { value, is_error })
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
                let format = string(arguments, "format")?;
                if !matches!(
                    format,
                    "step" | "iges" | "brep" | "stl" | "obj" | "3mf" | "dxf"
                ) {
                    return Err(format!("unknown export format '{format}'"));
                }
                let sketch = if format == "dxf" {
                    if arguments.get("bodies").is_some() {
                        return Err("DXF export uses sketch, not bodies".to_owned());
                    }
                    Some(string(arguments, "sketch")?)
                } else {
                    if arguments.get("sketch").is_some() {
                        return Err("sketch is only accepted for DXF export".to_owned());
                    }
                    None
                };
                let bodies = arguments.get("bodies");
                if let Some(bodies) = bodies
                    && !bodies
                        .as_array()
                        .is_some_and(|items| items.iter().all(Value::is_string))
                {
                    return Err("bodies must be an array of strings".to_owned());
                }
                let path = self
                    .workspace
                    .write_path(string(arguments, "path")?, false)?;
                let mut command = if let Some(sketch) = sketch {
                    require_extension(&path, "dxf")?;
                    json!({"cmd": "export_sketch", "path": path, "sketch": sketch})
                } else {
                    if format == "obj" {
                        // OCCT also writes <name>.mtl beside an OBJ.
                        require_extension(&path, "obj")?;
                        self.workspace
                            .write_path(path_text(&path.with_extension("mtl"))?, false)?;
                    }
                    json!({"cmd": "export", "path": path, "format": format})
                };
                if let Some(bodies) = bodies {
                    command["bodies"] = bodies.clone();
                }
                let opened = self.document_mut(id)?;
                if geometry_failed(&snapshot(&opened.document)?) {
                    return Err(
                        "document has failed features; inspect and fix them before export"
                            .to_owned(),
                    );
                }
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

// Audited against model/api/{mod,sketch,components,joints,libraries}.rs.
// Never extend these by prefix: imports, links and output need separate
// path validation, including the auxiliary paths of foreign importers.
fn safe_command(command: &str) -> bool {
    matches!(
        command,
        "add_feature"
            | "edit_feature"
            | "delete_feature"
            | "suppress_feature"
            | "reorder_feature"
            | "rename_feature"
            | "rename_body"
            | "set_marker"
            | "add_parameter"
            | "set_parameter"
            | "set_units"
            | "rename_parameter"
            | "delete_parameter"
            | "undo"
            | "redo"
            | "recompute"
            | "merge_undo"
            | "set_body_visible"
            | "set_body_material"
            | "set_body_appearance"
            | "set_face_appearance"
            | "clear_face_appearances"
            | "set_feature_visible"
            | "add_named_view"
            | "delete_named_view"
            | "rename_named_view"
            | "set_origin_visible"
            | "set_isolation"
            | "group_features"
            | "ungroup"
            | "rename_group"
            | "add_analysis"
            | "edit_analysis"
            | "set_analysis_visible"
            | "rename_analysis"
            | "delete_analysis"
            | "create_component"
            | "components_from_bodies"
            | "activate_component"
            | "rename_component"
            | "ground_occurrence"
            | "set_occurrence_visible"
            | "set_occurrence_transform"
            | "copy_occurrence"
            | "paste_new"
            | "delete_occurrence"
            | "add_joint"
            | "add_as_built_joint"
            | "add_rigid_group"
            | "drive_joint"
            | "drag_occurrence"
            | "set_configurations"
            | "apply_configuration"
            | "sketch.create"
            | "sketch.add_rectangle"
            | "sketch.add_circle"
            | "sketch.add_point"
            | "sketch.add_line"
            | "sketch.rectangle"
            | "sketch.circle"
            | "sketch.arc"
            | "sketch.polygon"
            | "sketch.slot"
            | "sketch.ellipse"
            | "sketch.spline"
            | "sketch.add_text"
            | "sketch.edit_text"
            | "sketch.add_constraint"
            | "sketch.add_dimension"
            | "sketch.set_dimension"
            | "sketch.set_driven"
            | "sketch.remove"
            | "sketch.set_construction"
            | "sketch.set_centerline"
            | "sketch.set_fixed"
            | "sketch.drag"
            | "sketch.move"
            | "sketch.trim"
            | "sketch.extend"
            | "sketch.fillet"
            | "sketch.chamfer"
            | "sketch.offset"
            | "sketch.mirror"
            | "sketch.circular_pattern"
            | "sketch.rectangular_pattern"
            | "sketch.edit_pattern"
            | "sketch.edit_offset"
            | "sketch.project"
            | "sketch.set_dimension_text"
    )
}

fn safe_feature(feature: &str) -> bool {
    // All current feature evaluators operate on document geometry or
    // embedded B-rep bytes. Text resolves installed font families; a
    // feature has no external import or component-refresh operation.
    matches!(
        feature,
        "sketch"
            | "extrude"
            | "fillet"
            | "chamfer"
            | "shell"
            | "draft"
            | "offset_face"
            | "delete_face"
            | "replace_face"
            | "split_body"
            | "split_face"
            | "construction_plane"
            | "construction_axis"
            | "construction_point"
            | "base"
            | "revolve"
            | "hole"
            | "thread"
            | "rectangular_pattern"
            | "circular_pattern"
            | "path_pattern"
            | "mirror"
            | "combine"
            | "move"
            | "align"
            | "scale"
            | "box"
            | "cylinder"
            | "sphere"
            | "torus"
            | "sweep"
            | "loft"
            | "pipe"
            | "coil"
            | "rib"
            | "web"
            | "component_from_bodies"
            | "move_occurrence"
            | "capture_position"
            | "helix"
            | "joint"
            | "as_built_joint"
            | "joint_origin"
            | "rigid_group"
    )
}

fn safe_query(query: &str) -> bool {
    matches!(
        query,
        "document"
            | "timeline"
            | "evaluate"
            | "parameters"
            | "bodies"
            | "profiles"
            | "feature"
            | "faces"
            | "edges"
            | "report"
            | "datums"
            | "datum"
            | "properties"
            | "measure"
            | "interference"
            | "section"
            | "threads"
            | "sketch"
            | "components"
            | "instances"
            | "dependents"
            | "can_reorder"
            | "named_views"
            | "recompute_times"
            | "thread_sizes"
            | "cache"
            | "changes_since_saved"
            | "analyses"
            | "appearances"
            | "appearance_image"
            | "render_settings"
            | "joints"
            | "joint_dof"
            | "joint_drag"
            | "joint_frame"
            | "configurations"
            | "library_parts"
            | "parts_list"
    )
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
