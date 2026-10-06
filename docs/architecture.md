# Mitcad architecture

Three layers: a Rust model that knows the document, timeline and features
but no geometry; a thin C++ facade over Open CASCADE Technology (OCCT)
that does the geometry; and a Qt application that talks to the model with
JSON commands.

## Layers

| Part | Language | Responsibility |
|---|---|---|
| `core/model` (`mitcad-model`) | Rust | Document, timeline, features, parameters and expressions, units, sketches, topological naming, cached recompute, undo, project file, design comparison, JSON command API. No OCCT. |
| `core/solver` (`mitcad-solver`) | Rust | 2D constraint solver behind a narrow API ([README](../core/solver/README.md)). |
| `core/f3d` (`mitcad-f3d`) | Rust | `.f3d` container, ASM B-rep, design streams decoded into the dump IR. |
| `core/import` (`mitcad-import`) | Rust | Replays `.f3d` dumps and FreeCAD documents as Mitcad commands, checked against the file's stored results ([README](../core/import/README.md)). |
| `core/freecad` (`mitcad-freecad`) | Rust | `.FCStd` reader: typed objects and properties, placements, links, sketches, spreadsheets, expressions. No geometry. |
| `core/zip` (`mitcad-zip`) | Rust | Zip container for `.f3d` and `.FCStd` (stored, deflate, zstd; a writer for tests). |
| `core/dxf` | Rust | DXF read and write. |
| `core/3mf` (`mitcad-3mf`) | Rust | 3MF writer (one object of parts) and a checking reader for tests. |
| `core/vcs` (`mitcad-vcs`) | Rust | Version history of projects in git (gitoxide, only here); remotes through the system's git. |
| `core/update` (`mitcad-update`) | Rust | Release manifest, signatures, version selection; the `mitcad-release` tool. No network ([updates.md](updates.md)). |
| `core/ffi`, `core/cpp` | Rust, C++ | CXX bridges: the `Kernel` on OCCT, the document API for C++. |
| `geometry` (`mitcad_geometry`) | C++ | OCCT facade: profiles, features, booleans, name transfer, queries, file formats, ASM bodies, analyses. No Qt. |
| `tools/cli` (`mitcad-cli`) | C++ | Headless open/import, recompute, properties, export; kernel and import tests run through it. |
| `app` | C++/Qt | The UI. |
| `tools/freecad-export` | Python, Bash | FreeCAD macros that make the FreeCAD import's reference models and dumps. |

Dependencies point one way: `app` → `core/ffi` → `core/model` (with
`solver`, `f3d`, `import`, `freecad`, `zip`, `3mf`, `vcs`; `update` stands
alone) and `core/ffi` → `geometry` → OCCT. The model reaches geometry only
through the `Kernel` trait, so model tests run on a mock kernel.

## Identifiers

- **Feature:** `FeatureUid(u64)`, stored as `F<n>`. Timeline order is a
  separate list; an index is not an id.
- **Sketch entity:** `EntityUid(u32)` within its sketch: `c<n>` curves,
  `p<n>` points, `t<n>` texts (one number space); constraints and
  dimensions `k<n>`.
- **Body:** `BodyUid` = creating feature + ordinal (`F7.b0`). The display
  name (`Body1`) is separate.
- **Component / occurrence:** `C<n>` (root `C0`) and `O<n>`; an
  occurrence is named `<component>:<n>`.
- **Parameter:** referenced by a persistent `ParamId`, so it can be
  renamed. A model parameter's `owner` is the feature whose dimension it
  is and is deleted with it.

## The model

- `Document`: parameters, timeline (`Vec<FeatureUid>` + features), the
  timeline marker (roll-back point), components and occurrences, settings.
- `FeatureEntry { uid, name, suppressed, component, def: FeatureDef }`.
  Each `FeatureDef` variant has its own module in
  `core/model/src/features/` giving dependencies, validation and
  evaluation.
- **Body state:** each feature maps the previous `BodyUid → Shape` state
  to a new one (New Body, Join, Cut, Intersect). A failed feature is
  skipped. State is per component; a feature sees only its component's
  bodies.
- **Recompute cache:** keyed by the feature's definition, the values of
  the parameters it uses and the versions of its inputs; reads of other
  features' results are recorded in the key (`Read::Output`,
  `Read::Datum`). A result's version is a 128-bit fingerprint of uid,
  definition and inputs (`core/model/src/fingerprint.rs`), stable across
  documents and processes.
- **Result store** (`core/model/src/store.rs`): costly results are also
  kept on disk under the same key, so reopening a design or an older
  version reuses them. Shapes are stored with their names
  (`geometry/include/mitcad/geometry/persist.hpp`); results of another
  build are ignored; sketches, construction and plain base features are
  not stored. The memory cache has a byte budget (the app gives a quarter
  of RAM) and evicts least recently used results not held by the current
  document. The `cache` query reports both caches.
- **Cancellation:** a recompute reports progress to a `RecomputeMonitor`
  and stops between features when cancelled. Commands, undo, redo and
  previews compute the new state before committing it, so a cancelled one
  changes nothing. Inside a feature, long OCCT operations (booleans,
  fillets, sweeps, lofts, offsets, splits, sewing) poll the monitor
  through a progress range (`Kernel::interruptible`,
  `geometry/include/mitcad/geometry/cancel.hpp`) and throw `Cancelled`.
  Without a monitor (`mitcad-cli`, the `.f3d` import) nothing stops early.
- **Undo:** the definition state (parameters, features, marker, body
  attributes, assembly, visibility, named views) is cheap to clone and
  pushed on a stack; several commands can merge into one step
  (`merge_undo`). Each state has a revision number, so `mark_saved` and
  the `document` query's `modified` work without comparing files.
- **Parameters:** name, expression (`d1 * 2 + 5 mm`), unit, comment,
  kind (model or user), evaluated in dependency order; cycles and unit
  mismatches are errors. Internal units are mm and radians. Input accepts
  a decimal comma (`core/model/src/expr/mod.rs`); stored expressions
  always use points.
- **Construction geometry:** planes, axes and points are features that
  make a datum, not bodies. Features refer to geometry through `GeomRef`
  (origin datum, construction feature, or a body face/edge/vertex) and to
  paths through `PathRef`.
- **Body attributes** (visibility, material, appearance; steel by
  default) live in the definition state.
- **Components** (`core/model/src/assembly.rs`): structure is in the
  definition state, not on the timeline, though features can create
  components. Occurrences belong to a component, so sub-assemblies are
  shared and world placements are paths from the root. One timeline;
  occurrence moves on it are placement changes. No cross-component
  references. A linked external component is read-only; its bodies are
  stored in a base feature refreshed on open.

### Units

The default is metric wherever a unit is chosen: document (mm), import and
export units, thread and hole tables (ISO first), materials and mass
properties, grid and settings. Inches only by choice. An imported file's
own units are kept.

## Geometry kernel

The `Kernel` trait (`core/model/src/kernel.rs`) is grouped by feature
family. New methods default to `Err(Unsupported)`, so the mock kernel
implements only what tests use. Bridges are split per family
(`core/ffi/src/kernel/<family>.rs`, `geometry/src/<family>.cpp`) and share
the `Shape` type.

- Inputs are plain structures: profile regions as loops of named segments
  (`Line`, `Arc`, `Circle`, `Ellipse`, `EllipseArc`, `BSpline`) plus a
  `SketchFrame`. Results are `Shape` handles whose faces carry names.
- Query results are kernel-independent descriptions: a planar face's
  origin is the projection of the model origin; a surface of revolution's
  axis points along its largest component.
- Volumes and areas are integrated per face: exact Gauss points on planes,
  quadrics, tori and polynomial B-splines, adaptive to 1e-9 relative
  elsewhere. `Shape` caches its measured properties and bounding box
  behind a lock; the underlying OCCT shape must not be read while another
  thread computes with it.
- Boolean results get coplanar faces and collinear edges merged, unless
  the merge breaks a result OCCT's checker accepted.
- **Operations must not modify their inputs.** Several OCCT algorithms
  write into input sub-shapes (p-curves, tolerances), which made a feature
  give a different B-rep depending on what ran before. Merged booleans,
  fillets, chamfers, shells, lofts from faces and shape healing therefore
  work on copies (`detail::InputCopy`, `detail::working_copy` in
  `geometry/src/history.hpp`). `MITCAD_CHECK_INPUTS=1` fingerprints inputs
  before and after every operation and reports changes on stderr
  (`geometry/include/mitcad/geometry/input_check.hpp`); geometry tests run
  with it.
- OCCT crashes become errors where possible (`catch_occt_crashes`; on
  Linux and macOS needs `OCC_CONVERT_SIGNALS`, defined in the root
  `CMakeLists.txt`; on macOS a fault is SIGSEGV or SIGBUS).
  Handlers are installed on the app's model worker thread, in
  `mitcad-cli`'s `main` and on the import's threads.
  `MITCAD_TEST_OCCT_CRASH=<operation>` forces a crash (`cli.occt_crash`).
- Where OCCT cannot impose what a feature asks, the geometry builds the
  surfaces itself (`geometry/src/skin.hpp`: interpolation with end
  derivatives, Gordon surfaces). Lofts with end conditions or rails
  (`loft_skin.cpp`), asymmetric and curvature-continuous fillets
  (`blend.cpp`) and setback/blend corners (`corner.cpp`) are built so;
  free lofts stay OCCT's through-sections.
- Export: 3MF and STL are written by the model from
  `Kernel::triangle_mesh` (one closed, outward-facing mesh per body,
  placed by its occurrences). STEP, IGES, OBJ and BRep go through
  `Kernel::write_file` with each body's placements (`ExportBody::placements`;
  STEP as an assembly, the others as moved copies).
- A mesh body (STL, OBJ) is a face with a triangulation and no surface.
  `geometry::transform_shape` moves a copy of the triangulation, and the
  validity check skips such faces.

## Topological naming

References to faces and edges are names derived from how faces came to
be; edges and vertices are named by their faces. Face names travel through
OCCT's history (`Modified`/`Generated`), and edge names are rebuilt from
neighbouring faces, so a reference survives a rebuild with new dimensions.
Grammar: `core/model/src/topo.rs`. C++ treats names as strings, carries
them through operations and derives edge names.

**Face name:** `<feature uid>:<role>[(<key>)]`, e.g. `F3:side(c1[c4,c2])`.

| Feature | Roles |
|---|---|
| Extrude, revolve, sweep, loft | `side(<segment>)`, `start(<region>)`, `end(<region>)` |
| Thin extrude | `outer(<segment>)`, `inner(<segment>)`, `mid(<region>)` |
| Fillet, chamfer | `fillet(<edge>)`, `chamfer(<edge>)`, `corner(<vertex>)` |
| Hole | `hole<i>.wall`, `.tip`, `.cbore_wall`, `.cbore_floor`, `.csink`, `.top`; thread `thread(<face>)` |
| Shell | `offset(<face>)`, `offset_cap(<removed face>)` |
| Replace face, split | `replace(<face>)`, `split`, `split(<tool face>)` |
| Pattern, mirror, copy | `inst<i>(<original face name>)`; a pattern of a pattern nests: `F6:inst1(F5:inst2(F4:side(c1)))` |
| Pipe, coil | `side<i>`, `inner<i>`, `start`, `end` |
| Primitives | `top`, `bottom`, `side<i>` |
| Imported body | `import(<i>)` |

- **Segment:** the part of a sketch curve in a profile,
  `<curve>[<start>,<end>]`, bounded by the curves it ends on in the
  curve's direction: `c3[c2,c4]`. Several curves at one point: `c2+c5`;
  free end: `-`; whole circle: `c5`.
- **Region:** sorted set of the outer loop's segments,
  `r{c1[c4,c2],c2[c1,c3],…}`. A key that no longer exists matches the
  region with the most curves in common and keeps the old key.
- A face may carry several names. When a boolean splits a face, pieces get
  `#k` in geometric order; a reference without `#k` means all pieces.
- **Edge:** `E{<face A>|<face B>}` (sorted), `#k` when the same faces meet
  more than once; a seam has the same face twice. **Vertex:** the set of
  faces meeting at it. Sorting is byte-wise in both languages.
- Splitting a body: the first piece in geometric order keeps the body id,
  the others get ids of the splitting feature.
- Imports resolve the source file's references geometrically against the
  replayed body and store Mitcad names.

## Sketch and solver

A sketch definition (`SketchDef`, `core/model/src/sketch/`) is stored; its
evaluation is not.

- **Plane:** origin plane, construction plane or planar face, with an
  optional frame relative to it (imports keep the source's exact frame).
- **Entities:** point, line, circle, arc (centre/start/end, CCW), ellipse,
  elliptical arc, spline (fit or control points, degree, weights, knots),
  text, projections (optionally linked). Flags `construction` and `fixed`.
  Points are entities and curves refer to them, so a shared point is a
  real connection.
- **Constraints:** coincident, horizontal, vertical, parallel,
  perpendicular, tangent, equal, fix, midpoint, concentric, collinear,
  symmetric, smooth. **Dimensions:** distance (aligned, horizontal,
  vertical), length, angle, radius, diameter, arc length. A driving
  dimension is bound to a parameter (`d<n>`); a driven one is display only.
- **Stored positions** are the last solution and the next solve's start,
  so the same definition always gives the same geometry.

Evaluation: resolve the frame; compute projections; solve from the stored
positions; report DOF, fully defined entities, conflicting and redundant
constraints (a conflict is an error, the last geometry is still shown);
cut profile regions from the planar arrangement of non-construction
curves (half-edge structure, leftmost turn; islands are separate regions).

Editing operations (trim, extend, offset, mirror, patterns, project,
move/copy, fillet, chamfer, slots, polygons) are model commands producing
entities and constraints, shared by the UI and the imports.

- **Text** (`sketch/text.rs`): glyph outlines from `Kernel::font_glyphs`
  (OCCT font manager; bundled Droid Sans as fallback), laid out in Rust;
  each outline is a region curve `t<n>.g<k>.c<j>`.
- **Patterns** (`sketch/pattern.rs`): copies are tied to originals by
  solver constraints (`Translated`, `Rotated`, `TurnedDirection`,
  `EqualSize`), so they add no DOF and keep ids when the count changes.
- **Offsets** (`sketch/offset.rs`): lines, arcs and circles are offset
  exactly with constraints. Chains with ellipses or splines are derived
  after each solve as a C1 cubic B-spline within 1e-4 mm; they are not
  solver unknowns, and dragging them moves the source chain.

The solver works on its own ids; the sketch model maps entity ids to them.

## Application

The app uses the model only through JSON commands and queries
([commands.md](../core/model/src/api/commands.md)); geometry comes back as
`SharedPtr<Shape>` handles, so new commands need no bridge changes.
A `preview` computes on a copy and leaves results in the cache, so OK does
not recompute.

- **Commands** ([app/COMMANDS.md](../app/COMMANDS.md)): a UI command is a
  declarative definition; the framework builds the panel, selections,
  preview and add/edit. Editing rolls back to before the feature,
  previews there, and on OK runs `edit_feature` as one undo step.
- **Sketch mode** (`app/sketch/`): the app keeps the sketch in the form of
  the `sketch` query and does snapping, inference and hit testing itself;
  every change is a `sketch.*` command, one undo step per user action.
  Tools use a custom cursor (`app/framework/Cursors`).
- **Browser, timeline, view** (`app/browser/`, `app/view/`): the model is
  read once per change; the view keeps AIS objects that did not change.
  Visibility is the model's: an unset sketch light bulb follows "hidden
  while a feature uses it" (`document/browser.rs`).
- **Background computation** (`app/framework/ModelWorker`): the model runs
  on one long-lived worker thread (256 MB stack, OCCT crash handlers) as
  jobs, one at a time; opening, every model command, undo/redo and every
  panel preview is a job. The document belongs to one thread at a time.
  The view keeps drawing what it built earlier but must not read or
  triangulate shapes while a job runs (OCCT writes flags and
  triangulations into shared sub-shapes). The caller stays synchronous:
  50 ms blocked, then input queued, after 400 ms a modal progress dialog
  whose Cancel goes to the job's `RecomputeMonitor` (`JobControl` in the
  bridge). Queries run on the UI thread between jobs; reads during a job
  are refused (`ModelBusy`), and deferred work waits for idle
  (`MainWindow::whenIdle`). `MITCAD_COMPUTE_INLINE=1` runs jobs on the UI
  thread for debugging.
- **Imports** run in a separate process (`mitcad --import-worker`) so a
  kernel crash or hang only kills that process. Stop keeps what was
  imported (the worker reads `stop` on stdin); Cancel kills the process.
- **Updates** (`app/update/`): see [updates.md](updates.md).
- **3D view** (`app/OcctViewer`): an OCCT `V3d_View` in a
  `QOpenGLWidget`; OCCT renders into FBOs and needs a native drawable to
  make Qt's context current on. Linux: a GLX 4.5 compatibility context
  with a hidden helper window (a core profile makes OCCT read Qt's pbuffer
  and crash). Windows: WGL, the window of Qt's offscreen surface. macOS: a
  4.1 core context, the highest Apple offers; `macSurfaceFormat()` is the
  default `QSurfaceFormat` before `QApplication` exists, because contexts
  of different profiles cannot share there, and the drawable is the
  top-level window's `NSView`. The GL assumptions stay in the surface
  format and `occtDrawable()`, so that another backend stays local (OCCT
  has no Metal one). The orientation cube and pick widths scale with the
  device pixel ratio.
- **Theme and icons** (`app/framework/Theme`, `Icons`): dark mode is read
  from the application palette (`isDarkPalette`), and the error, warning
  and accent colours and the `set*StyleSheet` helpers follow palette
  changes. The SVG icons are rendered on demand at the asked size and
  device pixel ratio, with a light outline on a dark palette.

## Window layouts and the macOS layer

`chromeStyle()` (`app/framework/ChromeStyle`) picks the layout at start-up:
**Docked** (Windows and Linux: the ribbon in a toolbar, the panels in docks)
or **Floating** (the default on macOS; `--chrome=docked|floating` overrides
it). `MainWindow` builds either from the same panels, so object names and
the positions the UI tests click are the same in both.

Floating is a full-window 3D view with the panels over it:

- `TitleBar`: one toolbar-high row under the native title bar (the content
  extends under it; empty parts move the window): Undo and Redo, the
  ribbon's tabs as a segmented control, the document name, the command
  search and, in a sketch, Finish Sketch. Under it the ribbon in
  `Presentation::Mac` (capsule groups whose name opens a menu of all their
  commands), and the update and remote notices.
- `GlassCard`: the rounded Browser, Command and Timeline cards and the
  status pill. The Command card shows only while it has something to say.
  There is no status bar; messages go to the pill.
- `FloatingLayout`, the central widget, owns all card geometry: cards are
  anchored to the view's corners (Browser upper left, Command upper right
  under the orientation cube, Timeline along the bottom), sized by their
  content up to a share of the view, laid out in one queued pass. It gives
  the view the free margins (`OcctViewer::setOverlayInsets`) and covered
  rectangles (`setCoveredRects`): the cube and its buttons stay clear of
  cards, picks and logged click positions avoid them, and Fit, Home and the
  named views fit the model into the free area (`fitToFreeArea`). Cards
  take the mouse only inside their rounded rectangle.

**Why the cards are windows.** With `glassActive()` (macOS 26+, Floating,
no `--no-glass`) a card's material is the system's Liquid Glass
(`NSGlassEffectView`). Qt's OpenGL content covers any native view inside the
main window, so each card is then a frameless top-level window of its own
(`GlassCard::setWindowed`), attached as a child window of the main window
(`mac::attachGlassWindow`), and the window server composes the glass over the
real 3D view. Without glass the cards are child widgets that paint their own
material. `GlassCard` and `FloatingLayout` hide the difference: card places
are slots in parent coordinates mapped to the screen and kept there when the
main window moves, resizes, minimizes or goes full screen; card windows stay
out of the Window menu, the window cycle and other Spaces; a card window
without a focused widget forwards keys to the main window's focus widget
(`GlassCard::forwardKey`); logged positions are in host window coordinates
(`GlassCard::mapToHost`).

Dialogs modal to the main window are sheets on macOS (`app/framework/Dialogs`:
`prepareModal`, `sheetWarning`, `sheetInformation`, `sheetQuestion`). On
macOS Preferences is `SettingsWindow`, a pane per Preferences page whose
changes apply as they are made, and About is the standard About panel.

**The platform layer.** `app/platform/MacChrome.hpp` declares the
AppKit-only functions in `mitcad::mac`: SF Symbols as icons in the palette's
colours (`symbolIcon`), menu images hidden in the menu bar and shown in the
ribbon's menus, the hidden native title, the glass child windows, the About
panel and `captureOwnWindows` (the application's own windows as the window
server shows them, for tests). `app/platform/macos/MacChrome.mm` implements
them (Objective-C++, ARC); elsewhere they are inline stubs in the header, so
portable code calls them without `#ifdef`. `Q_OS_MACOS` branches in portable
files are for Qt-level behaviour only (the GL format, shortcuts).

## Project file

Single-file format, version 2:
`{"format": "mitcad", "version": 2, "units", "parameters", "features":
[{"uid": "F3", "type": "extrude", ...}], "bodies", "marker"}`, plus
`components`, `occurrences`, `active_component`, `root_component`, a
feature's `component` when present, and named `views`. Version 1 files
are converted on open. Parametric geometry is not stored. Imported bodies
live in base features as OCCT binary BRep, zlib + base64
(`{"format": "occt", "compression": "zlib", "size": n, "data": "…"}`,
`core/model/src/features/base.rs`); their faces are named
`<feature>:import(<j>)`.

**Projects** (version 3): a folder with the marker `.mitcad/project.json`.
Project files inside keep base-feature B-rep data in a content-addressed
store, `.mitcad/brep/<aa>/<sha256>.brep.zlib`, referenced as
`{"format": "occt", "compression": "zlib", "size": n, "sha256": "…"}`.
Store files are written once, before the project file, so the JSON stays
small and diffable. A file is version 3 only when it has such references.
Opening resolves them from the nearest enclosing project and checks size
and hash; missing data fails its base feature with a message. The
per-user display state (Origin folder, Isolate) is kept out of versioned
files in `.mitcad/local/display/<path>.json`. Autosave and the import
process always write single files (version 2).

Code: `Document::save_file` (atomic: temp file renamed over),
`load_project`, `from_json_at`, `from_json_in` with a `BlobStore`
(`core/model/src/file/project.rs`: `FsStore`, `MemoryStore`; `core/vcs`
adds `GitTreeStore`). `mitcad-cli project init` and
`convert --format v2|v3`. `project init` writes:

```
# .gitattributes
*.mitcad text eol=lf merge=binary
.mitcad/brep/** binary
# .gitignore
.mitcad/cache/
.mitcad/local/
.*.tmp
```

`eol=lf` protects files from `core.autocrlf`; `merge=binary` makes git's
own merge treat a project file changed on both sides as a whole-file
conflict instead of producing valid JSON of a broken design. Temp files
of an interrupted save are `.<name>.<process>.<n>.tmp`. On Windows the
rename is retried for up to 2 s while another program holds the file
(`file::rename_retrying`).

### Version history

`core/vcs`, gitoxide (`gix =0.88.0`, default features off: only `sha1`
and `index`; a default feature pulls in an MPL-2.0 crate). No gix types
in the crate's API. JSON commands: [commands.md](../core/model/src/api/commands.md)
("Version history").

- Mitcad commits only when the project folder is the root of a git
  repository, so a user's enclosing repository is never touched.
- A version is a commit built from HEAD's tree plus the saved paths (like
  `git commit --only`), adding the B-rep files they reference and removing
  unreferenced ones. `index.lock` is held for the round and only committed
  entries change, so the user's staged changes survive and `git status`
  stays clean. No commit when nothing changed, HEAD is detached, or a
  merge/rebase is in progress.
- Author: git's configured one, else Mitcad's settings. Messages end with
  trailers `Mitcad-Version` and `Mitcad-Format: 3`.
- A file's history is HEAD's first-parent chain, following content-
  preserving renames. Older versions are read from the commit tree with
  `GitTreeStore`, no checkout. Restore writes the old content back as a
  new version; history is never rewritten.
- **Comparison** (`core/model/src/diff/`): two document snapshots are
  compared in model terms without computing them (parameters by name,
  features by uid with moves, definition fields, sketch elements,
  components, bodies, views; optionally computed volumes and areas). The
  model has no git: the vcs `diff` command reads texts from trees, the
  bridge's `diff_documents` compares open documents.

In the app (`app/files/MainWindowVersions.cpp`): Save in a project with
history writes the file, then commits on the UI thread (it reads saved
files, not the document); a failed commit leaves the file saved. The
message is the undo labels since the saved state
(`changes_since_saved`), excluding display-only changes. Before
overwriting, Save compares the file's blob ids in the folder and HEAD
(`status`) with those at open/save and asks on a mismatch. A file renamed
outside Mitcad (`status`'s `renamed_from`) takes its display state along
(`follow_rename`), and the next save records the rename first. Creating
history is two steps (`create_project_repository`, then
`init_project_history`) so the author can be confirmed. Autosave never
records a version.

The Version History window (`app/files/VersionHistory.cpp`) reads history
on its own thread with its own `Project` (a project is single-threaded):
the list first, then per-version summaries. Geometry comparison runs as a
job on the model worker. Open loads a version as a new untitled design;
Restore saves or drops unsaved changes first. Each Save keeps a 256 px
preview in the user's local app data, `thumbnails/<blob id>.png`.

### Remote repositories

gix has no push or checkout, so network operations run the system's git
(`core/vcs/src/remote/cli.rs`; `MITCAD_GIT`, else `PATH`, on Windows also
Git for Windows' usual folders; 2.34+): `ls-remote`, `fetch`, `push`,
`clone`, `merge --ff-only`. TLS, SSH and credentials are git's (SSH
agent, credential helpers). git runs non-interactively (no stdin,
`GIT_TERMINAL_PROMPT=0`, `setsid` outside Windows), with `LC_ALL=C` so
failures can be classified (`errors.rs`), `GIT_ALLOW_PROTOCOL` limited to
ssh/https/http/file, and `-c core.autocrlf=false -c core.eol=lf`. Runs
are cancellable and time out after 5 minutes without output. Mitcad never
handles passwords: URLs with credentials and `ext::` helpers are refused,
and credentials are hidden in output.

- Ahead/behind counts come from local refs (`HEAD` vs.
  `refs/remotes/origin/<branch>`), so status needs no network.
- Connecting refuses an unrelated history, sets `origin` and upstream,
  commits `.gitattributes`, then pushes or fetches. Pushes are never
  forced; files over 100 MiB are held back. Clone refuses a repository
  without a project at its root.
- **Sync** (`core/vcs/src/remote/sync.rs`) fetches, then replays the
  project's unpublished versions on top of the remote's with gix, keeping
  history linear. Whole files are the unit: a path changed on both sides
  is a conflict resolved as keep mine, take theirs or save mine as a copy;
  file contents are never merged. Nothing changes until every conflict has
  a choice; the old history is kept in `refs/mitcad/sync-backup/<time>`
  (pruned after 30 days, newest 5 kept), then `git reset --keep` moves the
  branch and folder. `git gc --auto` runs afterwards since gix writes
  loose objects.
- Per-user remote state: `.mitcad/local/remote.json`.

In the app (`app/files/RemoteController.cpp`) remote work runs one task
at a time on its own thread (`RemoteTask` with its own `Project` and a
cancellable `SyncControl`), never on the UI thread or the model worker;
`remote_info` and `incoming` (no network) run on the UI thread. Saves push
immediately, retrying with exponential backoff from one minute; the remote
is fetched on open, every 10 minutes and at the first change. Save and
Restore wait for a running sync.

### Autosave

Autosave writes unsaved work as a version 2 project file with a metadata
file and a session lock to a recovery folder in the user's local app data,
never into the project (format: [app/COMMANDS.md](../app/COMMANDS.md),
Files). It writes only when the revision changed. On start the app offers
sessions whose lock is stale; a recovered document keeps its file path
but is modified and has no undo history.

## Imports with history

`.f3d` designs and FreeCAD documents are replayed as Mitcad features,
each step checked against what the file stores (the `.f3d` ASM history;
FreeCAD's per-feature shapes). What cannot be translated or does not match
falls back to a base feature of the stored bodies, and the replay
continues on it. Both run over the `Kernel` trait, so they are testable
on the mock kernel; the app runs them in the import process.

- `.f3d`: the design streams are decoded into the dump IR
  ([SCHEMA.md](../core/import/SCHEMA.md)), which external dumps (route A)
  also use; formats in [ASM_FORMAT.md](../core/f3d/ASM_FORMAT.md) and
  [OGS_FORMAT.md](../core/f3d/OGS_FORMAT.md).
- FreeCAD: `core/freecad` reads the archive and XML (`roxmltree`, DTDs
  refused); enumerations are indices, mapped per FreeCAD version through
  `core/freecad/data/enums.json`.

How both work, their limits and corpus tests:
[core/import/README.md](../core/import/README.md). Real files are kept
outside the repository (`MITCAD_F3D_CORPUS`, `MITCAD_F3D_MODELS`,
`MITCAD_FCSTD_CORPUS`; [development.md](development.md#freecad-reference-models-and-corpus)).
