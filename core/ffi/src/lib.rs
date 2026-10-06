// SPDX-License-Identifier: MIT
//! C++ bridges: the document for the Qt application and `mitcad-cli`, and
//! the model's [`mitcad_model::Kernel`] implemented with the OCCT geometry
//! library (`kernel`).
//!
//! The application talks to the document with JSON commands and queries
//! (`core/model/src/api/commands.md`) and gets geometry as shared `Shape`
//! handles, so a new command needs no change here.

pub mod kernel;

mod brep_import;
mod exchange;
// .f3d import with the timeline (T1).
mod f3d_import;
// Background computation (P7).
mod jobs;
// The result store (P7d).
mod result_store;
// Version history (P12b).
mod vcs;
// Comparison of versions (P12c).
mod diff;
// Remote repositories (P12 remote).
mod remote;
// FreeCAD .FCStd import.
mod fcstd_import;
// Automatic updates (mitcad#9).
mod update;

use cxx::SharedPtr;
use mitcad_model::api::ApiError;
use mitcad_model::{BodyUid, FeatureUid, RegionKey};

use jobs::{JobControl, new_job_control};
use kernel::{OcctKernel, Shape};
use result_store::{result_store_clear, result_store_gc};
use vcs::{
    Project, create_project_repository, git_repository_root, init_project_history, load_version,
    open_project,
};
// Comparison of versions (P12c).
use diff::diff_documents;
// Remote repositories (P12 remote).
use remote::{
    SyncControl, clone_project, describe_remote, git_info, git_info_at, new_sync_control,
};

#[cxx::bridge(namespace = "mitcad")]
mod ffi {
    unsafe extern "C++" {
        include!("mitcad/geometry/shape.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        include!("bridge/shape.hpp");
        #[namespace = "mitcad::bridge"]
        type ShapeList = crate::kernel::shape::ffi::ShapeList;
    }

    extern "Rust" {
        type Document;

        fn new_document() -> Box<Document>;
        /// A 60 x 40 x 20 mm block (Sketch1, Extrude1), used by tests.
        fn new_demo_document() -> Box<Document>;
        /// Reads a project file (.mitcad JSON, version 1, 2 or 3; the
        /// B-rep references of version 3 stay unresolved). Nothing is
        /// computed until a `recompute` command.
        fn load_document(json: &str) -> Result<Box<Document>>;
        /// The project file of the document as a single file (version 2,
        /// B-rep data inside).
        fn to_json(self: &Document) -> String;
        /// The project file with the timeline marker at `marker`: what an
        /// edit that rolled the marker back will restore (autosave, P8).
        fn to_json_with_marker(self: &Document, marker: usize) -> String;

        /// Runs a JSON command (or an array of them); returns its result.
        fn command(self: &mut Document, json: &str) -> Result<String>;
        /// Answers a JSON query.
        fn query(self: &Document, json: &str) -> Result<String>;
        /// Evaluates an add_feature or edit_feature command without
        /// committing it; see preview_body_shape and preview_tool_shape.
        fn preview(self: &mut Document, json: &str) -> Result<String>;
        fn clear_preview(self: &mut Document);
        /// Runs a script of commands and expectations (mitcad-cli run).
        fn run_script(self: &mut Document, json: &str) -> Result<String>;
        /// The timeline and the bodies with their measurements, as text or
        /// JSON.
        fn report(self: &Document, json: bool) -> Result<String>;

        /// Shape of a body at the timeline marker, or null.
        fn body_shape(self: &Document, uid: &str) -> SharedPtr<Shape>;
        /// The face of an evaluated profile region, or null.
        fn profile_shape(self: &Document, sketch: &str, region: &str) -> SharedPtr<Shape>;
        /// A body after the previewed command, or null.
        fn preview_body_shape(self: &Document, uid: &str) -> SharedPtr<Shape>;
        /// The tool body of the previewed feature, or null.
        fn preview_tool_shape(self: &Document) -> SharedPtr<Shape>;

        // Construction geometry and analysis (F5).
        /// Answers an analysis query (properties, measure, interference,
        /// section, compare_step, datums) as text or pretty JSON.
        fn analysis(self: &Document, query: &str, json: bool) -> Result<String>;
        /// A shape for display: a datum, a section, a clipped body or the
        /// overlap of two bodies (commands.md); null when there is none.
        fn analysis_shape(self: &Document, request: &str) -> Result<SharedPtr<Shape>>;
        // Base features from shapes and .f3d bodies (exchange.rs; files
        // are imported and exported with JSON commands).

        /// Adds a base feature holding the shapes as bodies; `json` may give
        /// its name, source, operation, participants and the bodies' names
        /// and colours (commands.md). Returns the feature and its bodies.
        fn add_base_feature(self: &mut Document, shapes: &ShapeList, json: &str) -> Result<String>;
        /// Imports the bodies of an .f3d or .f3z file without
        /// history, one base feature per body; returns a JSON report.
        fn import_f3d(self: &mut Document, path: &str, json: &str) -> Result<String>;
        /// Imports an .f3d design with its timeline replayed as Mitcad
        /// features (T1; options in commands.md, `import_f3d`); returns the
        /// import report as JSON, or text with `{"text": true}`.
        fn import_f3d_timeline(self: &mut Document, path: &str, json: &str) -> Result<String>;
        /// Imports a FreeCAD .FCStd document: its stored bodies in its
        /// structure (options in commands.md, `import_fcstd`); returns the
        /// import report as JSON, or text with `{"text": true}`.
        fn import_fcstd(self: &mut Document, path: &str, json: &str) -> Result<String>;
    }

    // Background computation (P7, jobs.rs): the application computes a
    // document on a worker thread and shows the progress of the job.

    /// A snapshot of a job's progress.
    struct JobProgress {
        /// The features before the one being computed (shown as
        /// `position + 1` of `total`); `total` when done.
        position: usize,
        /// The features before the timeline marker.
        total: usize,
        /// Evaluations kept so far in the job (cache hits are not counted).
        evaluated: usize,
        /// The feature being evaluated, or the last one; empty before any.
        feature: String,
        cancelled: bool,
    }

    extern "Rust" {
        /// A job's progress and cancel request; one per job. Both threads
        /// may use it at once.
        type JobControl;

        fn new_job_control() -> Box<JobControl>;
        /// Asks the job to stop before the next feature; what the job ran
        /// then fails with "the computation was cancelled" and changes
        /// nothing. A control stays cancelled.
        fn cancel(self: &JobControl);
        fn is_cancelled(self: &JobControl) -> bool;
        fn progress(self: &JobControl) -> JobProgress;
        /// For tests: every evaluation of the job takes `ms` longer.
        fn set_test_delay(self: &JobControl, ms: u32);
        /// The document's recomputes report to `control` (and stop when it
        /// is cancelled) until detach_job.
        fn attach_job(self: &mut Document, control: &JobControl);
        fn detach_job(self: &mut Document);
    }

    // The result store (P7d, result_store.rs): results of costly
    // evaluations kept on disk, found again by a new document of the
    // same design.

    extern "Rust" {
        /// Recomputes take results from the store in `dir`, stored by the
        /// build `build_id` (geometry::kernel_build_id()), and
        /// persist_results writes there for the document named `label`.
        /// An empty `dir`: no store.
        fn set_result_store(self: &mut Document, dir: &str, build_id: &str, label: &str);
        /// Writes the results of the last recompute that are not stored
        /// yet and whose evaluation took at least `min_ms`; returns JSON
        /// {"results", "shapes", "bytes", "failed", "errors", "ms"}.
        fn persist_results(self: &mut Document, min_ms: f64) -> String;
        /// When the store holds more than `budget_mb` megabytes, removes
        /// the files used longest ago down to 80 % of it; returns JSON
        /// {"files", "bytes", "removed", "removed_bytes"} (what is left and
        /// what went). Any thread may call it, also while a document uses
        /// the store.
        fn result_store_gc(dir: &str, budget_mb: u64) -> String;
        /// Removes every stored result; the same JSON.
        fn result_store_clear(dir: &str) -> String;
        /// The most memory the document's cached results may take,
        /// estimated; beyond it the results used longest ago go (they can
        /// still come from the store). 0: no limit.
        fn set_memory_cache_budget(self: &mut Document, megabytes: u64);
    }

    // Projects and project files of version 3 (P12a): in a folder with
    // .mitcad/project.json, base features' B-rep data is kept in the
    // project's store (.mitcad/brep) and the project file refers to it.

    extern "Rust" {
        /// Reads the project file at `path`, a version 3 file's B-rep data
        /// from the store of the project it is in. Nothing is computed.
        fn load_project(path: &str) -> Result<Box<Document>>;
        /// The same with the file's text read by the caller.
        fn load_document_at(json: &str, path: &str) -> Result<Box<Document>>;
        /// Writes the project file to `path`, whole or not at all, in
        /// `format`: "auto" (version 3 in a project, else a single file),
        /// "single" (version 2, B-rep data inside) or "project" (version 3;
        /// the path must be in a project). Returns the text written.
        fn save_project(self: &Document, path: &str, format: &str) -> Result<String>;
        /// What opening the file changed or could not find (missing B-rep
        /// data, renamed parameters).
        fn load_warnings(self: &Document) -> Vec<String>;
        /// Makes folder `dir` a project: the marker and the recommended
        /// .gitattributes and .gitignore lines. Returns JSON {"root",
        /// "written": [files]}.
        fn init_project(dir: &str) -> Result<String>;
        /// The folder of the project a file (which need not exist) is in:
        /// the nearest one upwards with the marker; "" when there is none.
        fn find_project(path: &str) -> String;
    }

    // Version history (P12b, core/vcs): a project whose folder is the root
    // of a git repository records its versions as commits.

    extern "Rust" {
        type Project;
        /// The project with version history that a file (which need not
        /// exist) or a project's folder is in; the error says why there is
        /// none.
        fn open_project(path: &str) -> Result<Box<Project>>;
        /// Makes folder `dir` a project with version history: the marker,
        /// .gitattributes, .gitignore, a git repository and a first version
        /// by `author` ("Name <email>", or "" for git's configured author).
        /// Returns JSON {"root", "branch", "commit", "written", ...}.
        fn init_project_history(dir: &str, author: &str) -> Result<String>;
        /// Runs a version history command (JSON, commands.md "Version
        /// history": commit, history, read_version, restore, changes,
        /// status, identity, resolve, info; "Comparison of versions":
        /// diff); returns its result.
        fn command(self: &Project, json: &str) -> Result<String>;
        /// The same, answered in text for people (mitcad-cli).
        fn command_text(self: &Project, json: &str) -> Result<String>;
        /// Reads the project file at `path` as it was in version `rev`, its
        /// B-rep data from that version. Nothing is computed, and linked
        /// components keep the bodies saved with the version.
        fn load_version(project: &Project, rev: &str, path: &str) -> Result<Box<Document>>;
        /// Saving versions in the application (P12d): folder `dir` made a
        /// project with a git repository but no version yet, so that the
        /// author can be shown before init_project_history records the
        /// first one.
        fn create_project_repository(dir: &str) -> Result<Box<Project>>;
        /// The work tree of the git repository a folder (which need not
        /// exist) is in, or "" when it is in none.
        fn git_repository_root(path: &str) -> String;
    }

    // Comparison of versions (P12c, diff.rs): what differs between two
    // designs in the model's terms (commands.md "Comparison of versions").

    extern "Rust" {
        /// What differs from document `from` to document `to` (their
        /// definitions; nothing is computed) as JSON; options (JSON, or
        /// ""): `text` (text for people instead), `geometry` (also the
        /// bodies' volumes and areas as last computed), `from` and `to`
        /// (names of the two for the answer).
        fn diff_documents(from: &Document, to: &Document, json: &str) -> Result<String>;
    }

    // Remote repositories (P12 remote, remote.rs): the version history
    // shared through a git server or a folder, with the system's git
    // program for the network (commands.md "Remote repositories").

    /// A snapshot of a remote operation's progress.
    struct SyncProgress {
        /// git's phase (`Receiving objects`); empty before git reports one.
        text: String,
        /// Its percentage, or -1 when git gives none.
        percent: i32,
        cancelled: bool,
    }

    extern "Rust" {
        /// The progress and cancel request of a remote operation; one per
        /// operation. Both threads may use it at once.
        type SyncControl;

        fn new_sync_control() -> Box<SyncControl>;
        /// Ends the git program the operation runs; the operation then
        /// answers with the error class "cancelled". A control stays
        /// cancelled.
        fn cancel(self: &SyncControl);
        fn is_cancelled(self: &SyncControl) -> bool;
        fn progress(self: &SyncControl) -> SyncProgress;
        /// Runs a version history command as command() does; a remote
        /// command (git_info, remote_info, remote_check, remote_set,
        /// remote_remove, connect, fetch, push, sync_plan, sync) reports
        /// its progress to `control` and stops when it is cancelled.
        fn command_with(self: &Project, json: &str, control: &SyncControl) -> Result<String>;
        /// Opens a project from the remote at `url` into folder `dir` (new,
        /// or empty) with git clone. JSON {"root", "url", "branch", "head",
        /// "files", "error", "log"}: a failure (no git, sign-in, network,
        /// not a project, cancelled) is in "error" and leaves no folder; in
        /// text (`text`) it is an error.
        fn clone_project(url: &str, dir: &str, control: &SyncControl, text: bool)
        -> Result<String>;
        /// The git program, its version and git-lfs's: JSON {"path",
        /// "version", "lfs", "supported", "minimum", "error"}, or text.
        fn git_info(text: bool) -> String;
        /// The JSON answer of remote command `command` (from command()) as
        /// text for people, a failure too: what a sync did before it
        /// stopped and why, with its conflicts and their versions.
        fn describe_remote(command: &str, answer: &str) -> Result<String>;
        /// The git program at `program` (a path, or "" for the one remote
        /// work finds), as git_info in JSON: what Preferences shows of a
        /// program before it is chosen.
        fn git_info_at(program: &str) -> String;
    }
}

/// The document of the application and `mitcad-cli`.
///
/// SAFETY (threads): C++ may hand a `Box<Document>` from one thread to
/// another, which CXX does not check (`Document` is not `Send`: its shapes
/// are `SharedPtr`s to C++ objects). The application computes on a worker
/// thread (P7, `jobs.rs`, `app/framework/ModelWorker.hpp`): a document
/// crosses threads only as a handover, used by one thread at a time and
/// passed through a mutex, which orders what one thread did before what the
/// other does next. The model keeps no thread-local state. The reference
/// counts of shapes (`shared_ptr`, OCCT handles) are atomic, so the other
/// thread may go on holding shapes of earlier results (shown bodies) and
/// drawing what it made of them before, but it does not read them while
/// the document computes: OCCT's algorithms set flags on the sub-shapes of
/// their inputs, which earlier results share, and OCCT keeps a face's
/// triangulation in the face (ThreadSanitizer finds the flag writes). The
/// caches of `geometry::Shape` are locked. This is the reasoning of
/// `f3d_import::Handover`.
pub struct Document(mitcad_model::Document<OcctKernel>);

fn new_document() -> Box<Document> {
    Box::new(Document(mitcad_model::Document::new(OcctKernel)))
}

fn new_demo_document() -> Box<Document> {
    let mut document = new_document();
    let region = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
    let commands = format!(
        r#"[{{"cmd": "sketch.create"}},
            {{"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
              "width": 60, "height": 40}},
            {{"cmd": "add_feature", "def": {{"type": "extrude",
              "profiles": [{{"sketch": "F1", "region": "{region}"}}],
              "extent": {{"type": "distance", "distance": 20}}, "operation": "new_body"}}}}]"#
    );
    document
        .0
        .command(&commands)
        .expect("the demo commands are valid");
    document
}

fn load_document(json: &str) -> Result<Box<Document>, mitcad_model::FileError> {
    mitcad_model::Document::from_json(json, OcctKernel).map(|doc| Box::new(Document(doc)))
}

fn load_project(path: &str) -> Result<Box<Document>, mitcad_model::ProjectError> {
    mitcad_model::Document::load_project(std::path::Path::new(path), OcctKernel)
        .map(|doc| Box::new(Document(doc)))
}

fn load_document_at(json: &str, path: &str) -> Result<Box<Document>, mitcad_model::FileError> {
    mitcad_model::Document::from_json_at(json, std::path::Path::new(path), OcctKernel)
        .map(|doc| Box::new(Document(doc)))
}

fn init_project(dir: &str) -> Result<String, ApiError> {
    let (project, written) = mitcad_model::Project::init(std::path::Path::new(dir))
        .map_err(|e| ApiError(format!("cannot make {dir} a project: {e}")))?;
    Ok(serde_json::json!({
        "root": project.root().to_string_lossy(),
        "written": written,
    })
    .to_string())
}

fn find_project(path: &str) -> String {
    mitcad_model::Project::find(std::path::Path::new(path))
        .map(|project| project.root().to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn shared(shape: Option<&SharedPtr<Shape>>) -> SharedPtr<Shape> {
    shape.cloned().unwrap_or_else(SharedPtr::null)
}

impl Document {
    fn to_json(&self) -> String {
        self.0.to_json()
    }

    fn to_json_with_marker(&self, marker: usize) -> String {
        self.0.to_json_with_marker(marker)
    }

    fn save_project(&self, path: &str, format: &str) -> Result<String, ApiError> {
        use mitcad_model::FileFormat;
        let format = match format {
            "auto" => FileFormat::Auto,
            "single" => FileFormat::Single,
            "project" => FileFormat::Project,
            other => return Err(ApiError(format!("unknown project file format '{other}'"))),
        };
        self.0
            .save_file(std::path::Path::new(path), format)
            .map_err(|e| ApiError(e.to_string()))
    }

    fn load_warnings(&self) -> Vec<String> {
        self.0.load_warnings().to_vec()
    }

    /// Model commands, and `import_f3d` and `import_fcstd`, which need the
    /// file's bodies built here (one command, or among an array).
    fn command(&mut self, json: &str) -> Result<String, ApiError> {
        let value: Option<serde_json::Value> = serde_json::from_str(json).ok();
        let is_import = |v: &serde_json::Value| {
            matches!(
                v.get("cmd").and_then(|c| c.as_str()),
                Some("import_f3d" | "import_fcstd")
            )
        };
        match value {
            Some(v) if is_import(&v) => {
                let mut result = self.import_command(v)?;
                result["recomputed"] = serde_json::json!(self.0.stats().evaluated.len());
                result["error"] = serde_json::json!(self.0.stats().error.map(|e| e.to_string()));
                Ok(result.to_string())
            }
            Some(serde_json::Value::Array(commands)) if commands.iter().any(is_import) => {
                let mut results = Vec::new();
                for (i, command) in commands.into_iter().enumerate() {
                    let result = if is_import(&command) {
                        self.import_command(command)
                    } else {
                        self.0
                            .command(&command.to_string())
                            .map(|r| serde_json::from_str(&r).expect("results are JSON"))
                    };
                    results.push(result.map_err(|e| ApiError(format!("commands[{i}]: {e}")))?);
                }
                Ok(serde_json::Value::Array(results).to_string())
            }
            _ => self.0.command(json),
        }
    }

    /// An `import_f3d` or `import_fcstd` command.
    fn import_command(
        &mut self,
        command: serde_json::Value,
    ) -> Result<serde_json::Value, ApiError> {
        if command.get("cmd").and_then(|c| c.as_str()) == Some("import_fcstd") {
            self.import_fcstd_command(command)
        } else {
            self.import_f3d_command(command)
        }
    }

    fn query(&self, json: &str) -> Result<String, ApiError> {
        let answer = self.0.query(json)?;
        // The caches' diagnostics (P7d) with the process's memory.
        Ok(result_store::with_process_memory(json, answer))
    }

    fn preview(&mut self, json: &str) -> Result<String, ApiError> {
        self.0.preview(json)
    }

    fn clear_preview(&mut self) {
        self.0.clear_preview();
    }

    fn run_script(&mut self, json: &str) -> Result<String, ApiError> {
        self.0.run_script(json)
    }

    fn report(&self, json: bool) -> Result<String, ApiError> {
        self.0.report(json)
    }

    fn body_shape(&self, uid: &str) -> SharedPtr<Shape> {
        let uid: Option<BodyUid> = uid.parse().ok();
        shared(uid.and_then(|uid| self.0.body_shape(uid)))
    }

    fn profile_shape(&self, sketch: &str, region: &str) -> SharedPtr<Shape> {
        use mitcad_model::Kernel;
        let (Ok(sketch), Ok(region)) = (sketch.parse::<FeatureUid>(), region.parse::<RegionKey>())
        else {
            return SharedPtr::null();
        };
        self.0
            .profile_region(sketch, &region)
            .and_then(|(frame, region)| {
                OcctKernel
                    .profile(&frame, std::slice::from_ref(region))
                    .ok()
            })
            .unwrap_or_else(SharedPtr::null)
    }

    fn preview_body_shape(&self, uid: &str) -> SharedPtr<Shape> {
        let uid: Option<BodyUid> = uid.parse().ok();
        shared(uid.and_then(|uid| self.0.preview_body(uid)))
    }

    fn preview_tool_shape(&self) -> SharedPtr<Shape> {
        shared(self.0.preview_tool())
    }

    fn analysis(&self, query: &str, json: bool) -> Result<String, ApiError> {
        self.0.analysis(query, json)
    }

    fn analysis_shape(&self, request: &str) -> Result<SharedPtr<Shape>, ApiError> {
        Ok(self
            .0
            .analysis_shape(request)?
            .unwrap_or_else(SharedPtr::null))
    }
}
