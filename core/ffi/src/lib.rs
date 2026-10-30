// SPDX-License-Identifier: MIT
//! C++ bridges: the document for the Qt application and `mitcad-cli`, and
//! the model's [`mitcad_model::Kernel`] implemented with the OCCT geometry
//! library (`kernel`).
//!
//! The application talks to the document with JSON commands and queries
//! (`core/model-api/src/commands.md`) and gets geometry as shared `Shape`
//! handles, so a new command needs no change here.

pub mod kernel;

mod brep_import;
mod exchange;
// .f3d import with the timeline (T1).
mod f3d_import;
// The documents an .f3d design refers to, from local files (mitcad#196).
mod f3d_references;
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
// The .ipt import (mitcad#60).
mod ipt_history;
mod ipt_import;
// The .iam import (mitcad#60, stage 4).
mod iam_import;
// Component libraries (mitcad#64, mitcad#63).
mod libraries;
// A Rust panic's message for the application's crash reports (mitcad#62).
mod panic_note;
// The process's memory against its limits, for the import (mitcad#80).
pub mod memory;
// Local and Cloud projects (mitcad#89).
mod projects;
// Live updates (mitcad#89).
mod live;
// Local headless MCP tools (mitcad#124).
mod mcp;
// The IFC import (mitcad#273).
mod ifc_import;
use ifc_import::ifc_test_building;
// The .rvt and .rfa import, phase 1 (mitcad#274).
mod rvt_import;
use rvt_import::{rvt_write_building_test, rvt_write_test_file};
// The .solidcraft import (mitcad#450).
mod solidcraft_import;
// The .dwg import (mitcad#453).
mod dwg_import;
use dwg_import::{dwg_report_passes, dwg_report_text, dwg_write_test_file};
// The AI chat panel's logic (mitcad#947).
mod chat;
use chat::{ChatStream, agent_chat, new_chat_stream};

use cxx::SharedPtr;
use mitcad_model::{BodyUid, FeatureUid, RegionKey};
use mitcad_model_api::ApiError;
use mitcad_model_api::{AnalysisApi, DocumentApi, ReportApi, ScriptApi};

use jobs::{JobControl, new_job_control};
use kernel::{OcctKernel, Shape};
use result_store::{DisplayFeed, DisplayMeshWrite, result_store_clear, result_store_gc};
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
// Component libraries (mitcad#64).
use libraries::{
    configure_libraries, default_libraries_directory, describe_library, describe_parts,
    library_command,
};
// Import threads the hang watchdog gave up (mitcad#82).
use f3d_import::abandoned_imports;
// Local and Cloud projects (mitcad#89).
use projects::{projects_command, projects_command_text};
// Live updates (mitcad#89).
use live::{LiveHub, new_live_hub};
use mcp::{
    LiveMcpSession, build_mcp_bundle, new_live_mcp_session, plan_codex_mcp_config, run_mcp,
    validate_mcp_command, validate_mcp_query, verify_codex_mcp_config,
};
// Live MCP's compact command results and command arrays (mitcad#689).
use mcp::{mcp_command_document, mcp_entry_failed, mcp_recomputed, mcp_snapshot_queries};

#[cxx::bridge(namespace = "mitcad")]
mod ffi {
    unsafe extern "C++" {
        include!("mitcad/geometry/shape.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        include!("bridge/shape.hpp");
        #[namespace = "mitcad::bridge"]
        type ShapeList = crate::kernel::shape::ffi::ShapeList;
    }

    extern "Rust" {
        type Document;

        /// Serves local MCP tools on stdin/stdout within the workspace.
        fn run_mcp(workspace: &str, read_only: bool) -> Result<()>;
        /// Plan a preserving local assistant configuration edit; no filesystem IO.
        fn plan_codex_mcp_config(current: &str, bridge: &str, replace_existing: bool) -> String;
        fn verify_codex_mcp_config(before: &str, after: &str, bridge: &str) -> String;
        /// Build an MCP bundle containing the installed native launcher.
        fn build_mcp_bundle(manifest: &str, launcher_path: &str) -> Result<Vec<u8>>;

        /// Live MCP protocol only; the application owns and dispatches tools.
        type LiveMcpSession;
        fn new_live_mcp_session() -> Box<LiveMcpSession>;
        /// Serialized protocol response/null, or a tool and its arguments.
        fn prepare_mcp(self: &mut LiveMcpSession, message: &str) -> Result<String>;
        /// Complete the pending tool without changing protocol ownership.
        fn finish_mcp(self: &mut LiveMcpSession, value: &str, is_error: bool) -> Result<String>;
        fn validate_mcp_command(request: &str) -> Result<()>;
        fn validate_mcp_features(self: &Document) -> Result<()>;
        fn validate_mcp_document_command(self: &Document, request: &str) -> Result<()>;
        fn validate_mcp_query(request: &str) -> Result<()>;
        /// Live MCP's command results, arrays and export (mitcad#689).
        fn mcp_snapshot_queries() -> String;
        fn mcp_command_document(
            mode: &str,
            before: &str,
            after: &str,
            times: &str,
        ) -> Result<String>;
        fn mcp_entry_failed(result: &str) -> bool;
        fn mcp_recomputed(result: &str) -> bool;
        fn mcp_export_plan(
            self: &Document,
            arguments: &str,
            path: &str,
            title: &str,
            design_folder: &str,
        ) -> Result<String>;
        fn mcp_export_drawing(self: &Document, command: &str) -> Result<String>;

        fn new_document() -> Box<Document>;
        /// A new, empty design: Root and an empty component Component1,
        /// active, both with the general defaults (mitcad#220, mitcad#421).
        fn new_design_document() -> Box<Document>;
        /// A 60 x 40 x 20 mm block (Sketch1, Extrude1), used by tests.
        fn new_demo_document() -> Box<Document>;
        /// Reads a design file (.mitcad JSON, version 1, 2 or 3; the
        /// B-rep references of version 3 stay unresolved). Nothing is
        /// computed until a `recompute` command.
        fn load_document(json: &str) -> Result<Box<Document>>;
        /// The design file of the document as a single file (version 2,
        /// B-rep data inside).
        fn to_json(self: &Document) -> String;
        /// The design file with the timeline marker at `marker`: what an
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
        /// Runs a script as run_script does; returns how long each step
        /// took, a line "Timing steps[i] <name>: <ms> ms" per step
        /// (mitcad-cli run --timings).
        fn run_script_timings(self: &mut Document, json: &str) -> Result<String>;
        /// The timeline and the bodies with their measurements, as text or
        /// JSON.
        fn report(self: &Document, json: bool) -> Result<String>;

        /// Shape of a body at the timeline marker, or null.
        fn body_shape(self: &Document, uid: &str) -> SharedPtr<Shape>;
        /// The face of an evaluated profile region, or null: the region a
        /// reference with the key means (the best match when the key changed
        /// with the sketch, as features find it).
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
        /// Several bodies clipped at one plane, as the `clip` shape clips
        /// each, worked out together on all cores (commands.md,
        /// `analysis_clips`; mitcad#438).
        fn analysis_clips(self: &Document, request: &str) -> Result<Box<ClippedBodies>>;
        /// `analysis_clips`' results, one per body in the request's order.
        type ClippedBodies;
        fn count(self: &ClippedBodies) -> usize;
        /// The clipped body (the body's own shape when the plane leaves it
        /// whole); null when its clip failed.
        fn shape(self: &ClippedBodies, index: usize) -> SharedPtr<Shape>;
        /// Why its clip failed; empty when it did not.
        fn error(self: &ClippedBodies, index: usize) -> String;
        // Section Analysis off the UI thread (mitcad#438): prepared from
        // the document while it is idle, worked out without it on another
        // thread. What they hold of the bodies is detached from them: a
        // copy sharing no sub-shape with a body the plane may reach, the
        // body itself (only its kept box read) for one it passes. The
        // geometry stops when the running thread's operations are told to
        // (geometry/cancel.hpp).
        /// `analysis_clips` prepared (its request).
        fn prepare_clips(self: &Document, request: &str) -> Result<Box<PreparedClips>>;
        type PreparedClips;
        /// The clips: what is left of each detached copy (the copy when
        /// the clip leaves it whole), the body itself when the plane
        /// passes it and keeps it.
        fn run(self: &PreparedClips) -> Box<ClippedBodies>;
        /// The shape body `index`'s clip reads; null for a body with an
        /// error.
        fn input(self: &PreparedClips, index: usize) -> SharedPtr<Shape>;
        /// Whether that shape is a detached copy (else the body itself).
        fn detached(self: &PreparedClips, index: usize) -> bool;
        /// A query prepared (`{"query": "section", ...}`, the only one so
        /// far): `run` gives the answer `query` gives now.
        fn prepare_query(self: &Document, request: &str) -> Result<Box<PreparedQuery>>;
        type PreparedQuery;
        fn run(self: &PreparedQuery) -> Result<String>;
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
        /// Imports the bodies stored in an .ipt part file as base features
        /// (options in commands.md, `import_ipt`); returns the import
        /// report as JSON, or text with `{"text": true}`.
        fn import_ipt(self: &mut Document, path: &str, json: &str) -> Result<String>;
        /// Imports an .iam assembly: its occurrences as components and
        /// occurrences, each part with the .ipt import (options in
        /// commands.md, `import_iam`); returns the import report as JSON,
        /// or text with `{"text": true}`.
        fn import_iam(self: &mut Document, path: &str, json: &str) -> Result<String>;
        /// Imports an IFC file (.ifc, .ifczip): its spatial structure as
        /// components and its elements' shapes as features and bodies
        /// (options in commands.md, `import_ifc`); returns the import report
        /// as JSON, or text with `{"text": true}`.
        fn import_ifc(self: &mut Document, path: &str, json: &str) -> Result<String>;
        /// The IFC reader's test building (`mitcad_ifc::testdata`), for
        /// `mitcad-cli import-ifc --write-test`.
        fn ifc_test_building() -> String;
        /// Imports an .rvt project's elements, one component per level,
        /// or reads an .rfa family's metadata (options in commands.md,
        /// `import_rvt`); returns the import report as JSON, or text with
        /// `{"text": true}`.
        fn import_rvt(self: &mut Document, path: &str, json: &str) -> Result<String>;
        /// Writes the .rvt reader's small test project (or family) file
        /// (tests).
        fn rvt_write_test_file(path: &str, family: bool) -> Result<()>;
        /// Writes the .rvt reader's building test project (walls, a floor, a
        /// grid, a link) to `dir` as building.rvt with its linked.rvt
        /// (mitcad#275).
        fn rvt_write_building_test(dir: &str) -> Result<()>;
        /// Imports a .solidcraft design with its history (mitcad#450; options in
        /// commands.md, `import_solidcraft`); returns the import report as
        /// JSON, or text with `{"text": true}`.
        fn import_solidcraft(self: &mut Document, path: &str, json: &str) -> Result<String>;
        /// Imports a .dwg drawing's 3D content: its solids and meshes as
        /// bodies, its blocks as components with their occurrences
        /// (options in commands.md, `import_dwg`); returns the import
        /// report as JSON, or text with `{"text": true}`.
        fn import_dwg(self: &mut Document, path: &str, json: &str) -> Result<String>;
        /// Writes the .dwg reader's test drawing in a version (`R2000`,
        /// `R2018`, ...), with a test case added (`empty-solid`; empty
        /// for none) (tests).
        fn dwg_write_test_file(path: &str, version: &str, test_case: &str) -> Result<()>;
        /// The text form of an `import_dwg` report given as JSON.
        fn dwg_report_text(json: &str) -> String;
        /// Whether an `import_dwg` report given as JSON passes its checks.
        fn dwg_report_passes(json: &str) -> bool;
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
        /// The job's question to the user waiting for an answer (one
        /// line), empty when none waits: the .f3d import asks what to do
        /// with the bodies it imported (mitcad#283).
        fn question(self: &JobControl) -> String;
        /// Answers the question waiting ("keep" or "replace").
        fn answer(self: &JobControl, answer: &str);
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
        /// Writes every result of the last recompute that a store takes
        /// to the store in `dir` (build `build_id`, document `label`),
        /// however quick, leaving the document's own store as it is: the
        /// import process hands its evaluation over to the application
        /// (mitcad#413). The same JSON as persist_results.
        fn hand_off_results(self: &Document, dir: &str, build_id: &str, label: &str) -> String;
        /// When the store holds more than `budget_mb` megabytes, removes
        /// the files used longest ago down to 80 % of it; returns JSON
        /// {"files", "bytes", "removed", "removed_bytes"} (what is left and
        /// what went). Any thread may call it, also while a document uses
        /// the store.
        fn result_store_gc(dir: &str, budget_mb: u64) -> String;
        /// Removes every stored result; the same JSON.
        fn result_store_clear(dir: &str) -> String;
        /// Display meshes kept in the store (mitcad#858). `request`:
        /// {"bodies": ["F2.b0", ...], "options": "<the meshing options>"}.
        /// Gives each body at the marker whose shape has display
        /// triangulations stored for its version and those options its
        /// triangulations back (the shape then counts as meshed for
        /// display); returns JSON {"restored": [uids], "faces", "bytes",
        /// "ms"}. Without a store, nothing.
        fn restore_display_meshes(self: &Document, request: &str) -> String;
        /// Writes the display triangulations of the bodies of `request`
        /// (the same form), each meshed for display to the end, under
        /// their versions and those options; returns JSON {"stored",
        /// "bytes", "failed", "errors", "ms"}.
        fn persist_display_meshes(self: &Document, request: &str) -> String;
        /// Display meshes written after an open (mitcad#880): the store,
        /// the options and the versions of the bodies of `request` (the
        /// same form) as the document has them now, for the application's
        /// writer, which stores the triangulations it took meanwhile on
        /// its own thread; the document may change or go meanwhile.
        fn display_mesh_write(self: &Document, request: &str) -> Box<DisplayMeshWrite>;
        type DisplayMeshWrite;
        /// Whether the body (a uid of the request) can be written: the
        /// document had a store and the body a version.
        fn has_body(self: &DisplayMeshWrite, uid: &str) -> bool;
        /// Writes `mesh`, the body's triangulations as
        /// io::triangulation_bytes gives them, under its version and the
        /// options (whole or not at all); the bytes of the file. Any
        /// thread.
        fn write_body(self: &DisplayMeshWrite, uid: &str, mesh: &[u8]) -> Result<u64>;
        /// Display meshes made during an open's recompute (mitcad#881):
        /// the next recompute through the whole timeline (the open's) puts
        /// each shown body into the feed once no later feature can change
        /// it (mitcad_model::FinalBodies), and closes it when it ends.
        /// `options` are the display meshes' in the store (as in a
        /// request): a body whose version has meshes stored with them is
        /// left to the store.
        fn display_feed(self: &mut Document, options: &str) -> Box<DisplayFeed>;
        type DisplayFeed;
        /// The bodies put in since the last take, waiting up to `wait_ms`
        /// for the first: their uids, and their shapes pushed to `shapes`
        /// in the same order (shared with the document's body states, to
        /// be read only); none once the recompute ended and all were
        /// taken (`is_done`). Any thread.
        fn take(self: &DisplayFeed, wait_ms: u32, shapes: Pin<&mut ShapeList>) -> Vec<String>;
        /// The recompute ended and every body was taken.
        fn is_done(self: &DisplayFeed) -> bool;
        /// Bodies put in, those of them left to the store's meshes, and
        /// those a later feature changed all the same (0 unless the
        /// classification missed one).
        fn given(self: &DisplayFeed) -> usize;
        fn left_to_store(self: &DisplayFeed) -> usize;
        fn changed_later(self: &DisplayFeed) -> usize;
        /// The most memory the document's cached results may take,
        /// estimated; beyond it the results used longest ago go (they can
        /// still come from the store). 0: no limit.
        fn set_memory_cache_budget(self: &mut Document, megabytes: u64);
    }

    // Projects and designs in them, design files of version 3 (P12a): in a
    // folder with .mitcad/project.json, base features' B-rep data is kept in
    // the project's store (.mitcad/brep) and the design file refers to it.

    extern "Rust" {
        /// Reads the design file at `path`, a version 3 file's B-rep data
        /// from the store of the project it is in. Nothing is computed.
        fn load_design(path: &str) -> Result<Box<Document>>;
        /// The same with the file's text read by the caller.
        fn load_document_at(json: &str, path: &str) -> Result<Box<Document>>;
        /// Writes the design file to `path`, whole or not at all, in
        /// `format`: "auto" (version 3 in a project, else a single-file
        /// design), "single" (version 2, B-rep data inside) or "project"
        /// (version 3; the path must be in a project). Returns the text
        /// written.
        fn save_design(self: &Document, path: &str, format: &str) -> Result<String>;
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
        /// Reads the design file at `path` as it was in version `rev`, its
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

    // Component libraries (mitcad#64, mitcad#63, libraries.rs): libraries
    // and community indexes in git repositories, fetched into a cache;
    // documents read library parts from it (commands.md "Component
    // libraries").

    extern "Rust" {
        /// The cache directory shared by the application and CLI, including
        /// MITCAD_LIBRARIES_DIR when set.
        fn default_libraries_directory() -> String;
        /// Sets where fetched libraries are kept (JSON {"root", "extra"};
        /// "" for the default: MITCAD_LIBRARIES_DIR, else the user's data
        /// folder) and lets documents read library parts from there.
        /// Returns {"root", "extra"}.
        fn configure_libraries(json: &str) -> Result<String>;
        /// Runs a library command (library_fetch, library_list,
        /// library_show, library_search, library_preview, library_diff,
        /// library_check, library_init, library_add, index_entry,
        /// licenses); a fetch reports its progress to `control`.
        fn library_command(json: &str, control: &SyncControl) -> Result<String>;
        /// A library command's JSON answer as text for people.
        fn describe_library(command: &str, answer: &str) -> Result<String>;
        /// The parts_list query's answer as text (mitcad-cli parts).
        fn describe_parts(answer: &str) -> Result<String>;
    }

    // Import threads the hang watchdog gave up (mitcad#82, f3d_import.rs).

    extern "Rust" {
        /// How many import threads the hang watchdog gave up still run
        /// (their geometry kernel call has not returned), with the
        /// workers that evaluated definitions ahead and still run
        /// (mitcad#95). A process that
        /// ends while one runs ends with std::_Exit once its output is
        /// written, without its static destructors, which would tear down
        /// OCCT's state under the thread.
        fn abandoned_imports() -> usize;
    }

    // Local and Cloud projects (mitcad#89, projects.rs): what a folder is,
    // what a remote holds, projects made and opened from a remote, shared
    // folders and SSH host keys (commands.md "Projects"). A project's own
    // commands (project_settings, set_project_settings, set_identity,
    // remember_design) are Project commands.

    extern "Rust" {
        /// Runs a command without a project (inspect_folder, check_remote,
        /// create_project, clone_project, init_bare, host_keys,
        /// trust_host_key, ssh_public_key, git_info); one that uses git or
        /// the network reports its progress to `control` and stops when it
        /// is cancelled. JSON with "error" (null on success) and "log".
        fn projects_command(json: &str, control: &SyncControl) -> Result<String>;
        /// The same answered in text for people (mitcad-cli); a failure in
        /// the answer's "error" is an error.
        fn projects_command_text(json: &str, control: &SyncControl) -> Result<String>;
    }

    // Live updates (mitcad#89, live.rs): the MQTT client of Cloud
    // projects, all in Rust (core/vcs/src/remote/mqtt); the application
    // gets checked events as JSON (commands.md "Live updates").

    extern "Rust" {
        /// The application's live updates: one connection per broker, user
        /// and prefix, shared by the projects on it. Both threads may use
        /// it at once: the commands on the UI thread, the events on a
        /// worker thread.
        type LiveHub;

        fn new_live_hub() -> Box<LiveHub>;
        /// Runs a live command (connect, subscribe, unsubscribe, watch,
        /// publish, disconnect, status, test, close); returns its JSON
        /// answer. `test` blocks up to its timeout: run it on a worker
        /// thread.
        fn command(self: &LiveHub, json: &str) -> Result<String>;
        /// The checked events that waited, after waiting up to
        /// `timeout_ms` for one: JSON {"events": [...], "lost", "closed"};
        /// at once when the hub is closed.
        fn events(self: &LiveHub, timeout_ms: u32) -> String;
    }

    // The AI chat panel (mitcad#947, chat.rs; core/mcp/src/chat.rs): the
    // agents' command lines and messages, approvals, the undo merge and the
    // history format as JSON requests, and the agents' output as panel
    // events.

    extern "Rust" {
        /// One request ({"op": ...}); JSON with "error" when it fails.
        fn agent_chat(request: &str) -> String;
        type ChatStream;
        /// The output stream of an agent ("claude", "codex").
        fn new_chat_stream(agent: &str) -> Result<Box<ChatStream>>;
        /// The panel events of one output line, a JSON array.
        fn feed_line(self: &mut ChatStream, line: &str) -> String;
        /// A control of the stream (Codex: start, message, interrupt): its
        /// panel events, a JSON array, or an object with "error".
        fn control(self: &mut ChatStream, request: &str) -> String;
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

/// A new, empty design as File > New makes it (mitcad#220): the root and an
/// empty component Component1, active, both with the general defaults
/// (mitcad#421).
fn new_design_document() -> Box<Document> {
    Box::new(Document(mitcad_model::Document::new_design(OcctKernel)))
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

fn load_design(path: &str) -> Result<Box<Document>, mitcad_model::DesignError> {
    mitcad_model::Document::load_design(std::path::Path::new(path), OcctKernel)
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

    fn save_design(&self, path: &str, format: &str) -> Result<String, ApiError> {
        use mitcad_model::FileFormat;
        let format = match format {
            "auto" => FileFormat::Auto,
            "single" => FileFormat::Single,
            "project" => FileFormat::Project,
            other => return Err(ApiError(format!("unknown design file format '{other}'"))),
        };
        self.0
            .save_file(std::path::Path::new(path), format)
            .map_err(|e| ApiError(e.to_string()))
    }

    fn load_warnings(&self) -> Vec<String> {
        self.0.load_warnings().to_vec()
    }

    /// Model commands, and `import_f3d`, `import_fcstd` and `import_ipt`,
    /// which need the file's bodies built here (one command, or among an
    /// array).
    fn command(&mut self, json: &str) -> Result<String, ApiError> {
        let value: Option<serde_json::Value> = serde_json::from_str(json).ok();
        let is_import = |v: &serde_json::Value| {
            matches!(
                v.get("cmd").and_then(|c| c.as_str()),
                Some(
                    "import_f3d"
                        | "import_fcstd"
                        | "import_ipt"
                        | "import_iam"
                        | "import_ifc"
                        | "import_rvt"
                        | "import_solidcraft"
                        | "import_dwg"
                )
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

    /// An `import_f3d`, `import_fcstd` or `import_ipt` command.
    fn import_command(
        &mut self,
        command: serde_json::Value,
    ) -> Result<serde_json::Value, ApiError> {
        match command.get("cmd").and_then(|c| c.as_str()) {
            Some("import_fcstd") => self.import_fcstd_command(command),
            Some("import_ipt") => self.import_ipt_command(command),
            Some("import_iam") => self.import_iam_command(command),
            Some("import_ifc") => self.import_ifc_command(command),
            Some("import_rvt") => self.import_rvt_command(command),
            Some("import_solidcraft") => self.import_solidcraft_command(command),
            Some("import_dwg") => self.import_dwg_command(command),
            _ => self.import_f3d_command(command),
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

    fn run_script_timings(&mut self, json: &str) -> Result<String, ApiError> {
        let mut times = String::new();
        self.0.run_script_timed(json, Some(&mut times))?;
        Ok(times)
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
        // Every piece of a region a sketch change split (mitcad#407).
        self.0
            .profile_pieces(sketch, &region)
            .and_then(|(frame, regions, _)| {
                let regions: Vec<_> = regions.into_iter().cloned().collect();
                OcctKernel.profile(&frame, &regions).ok()
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

    fn analysis_clips(&self, request: &str) -> Result<Box<ClippedBodies>, ApiError> {
        Ok(Box::new(ClippedBodies(self.0.analysis_clips(request)?)))
    }

    fn prepare_clips(&self, request: &str) -> Result<Box<PreparedClips>, ApiError> {
        Ok(Box::new(PreparedClips(self.0.prepare_clips(request)?)))
    }

    fn prepare_query(&self, request: &str) -> Result<Box<PreparedQuery>, ApiError> {
        Ok(Box::new(PreparedQuery(self.0.prepare_query(request)?)))
    }
}

/// `Document::prepare_clips` (mitcad#438).
pub struct PreparedClips(mitcad_model_api::PreparedClips<OcctKernel>);

impl PreparedClips {
    fn run(&self) -> Box<ClippedBodies> {
        Box::new(ClippedBodies(self.0.run(&OcctKernel)))
    }

    fn input(&self, index: usize) -> SharedPtr<Shape> {
        self.0
            .input(index)
            .map_or_else(SharedPtr::null, |(shape, _)| shape.clone())
    }

    fn detached(&self, index: usize) -> bool {
        self.0.input(index).is_some_and(|(_, detached)| detached)
    }
}

/// `Document::prepare_query` (mitcad#438).
pub struct PreparedQuery(mitcad_model_api::PreparedQuery<OcctKernel>);

impl PreparedQuery {
    fn run(&self) -> Result<String, ApiError> {
        Ok(self.0.run(&OcctKernel)?.to_string())
    }
}

/// `Document::analysis_clips`' results (mitcad#438).
pub struct ClippedBodies(Vec<Result<SharedPtr<Shape>, ApiError>>);

impl ClippedBodies {
    fn count(&self) -> usize {
        self.0.len()
    }

    fn shape(&self, index: usize) -> SharedPtr<Shape> {
        match self.0.get(index) {
            Some(Ok(shape)) => shape.clone(),
            _ => SharedPtr::null(),
        }
    }

    fn error(&self, index: usize) -> String {
        match self.0.get(index) {
            Some(Err(error)) => error.to_string(),
            _ => String::new(),
        }
    }
}
