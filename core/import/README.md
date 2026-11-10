# mitcad-import: .f3d designs with their history

`core/import` replays a design's history as MitCad features, checks each
step against the bodies stored in the file, and brings in what it cannot
replay as the stored bodies. These imports are described here:

- **.f3d**: the design dump IR (`mitcad_dump::Dump`,
  [SCHEMA.md](SCHEMA.md)), checked against the file's ASM history. The
  replay is split into crates (mitcad#785, *The replay's crates* below);
  this one, `mitcad-import`, puts them together and holds the replay's
  tests.
- **.ipt** and **.iam**: read by `core/ipt` and replayed on the same dump
  IR as the `.f3d` import.
- **.sc** and **.solidcraft**: built again from the file's definitions;
  the file keeps no bodies to check against. A `.sc` file (SolidCraft's
  extension now, mitcad#1622, which other programs use too) is read only
  when its format key is SolidCraft's, else refused with "Not a
  SolidCraft design: …". A crate of its own,
  [mitcad-import-solidcraft](../import-solidcraft/README.md) (mitcad#785),
  re-exported as `mitcad_import::solidcraft`.

The other imports are crates of their own (mitcad#775), so that a change
to one does not rebuild the `.f3d` replay:

| Crate | What it holds |
|---|---|
| [mitcad-import-base](../import-base/README.md) | What the imports share: `Progress` and the import running on a thread, vector helpers, the bodies of one replay, planar faces, the candidate core, the notes' texts, the sketch helpers. `mitcad-import` re-exports what it exported before (`Progress`, `tick`, `current_progress`, `abandoned`, `Ticking`, `Helper`, `DEFAULT_GEOMETRIC_TOLERANCE`, `geom`). |
| [mitcad-import-fcstd](../import-fcstd/README.md) | FreeCAD `.FCStd` documents, checked against every feature's stored result. |
| [mitcad-import-ifc](../import-ifc/README.md) | IFC building models. |
| [mitcad-import-rvt](../import-rvt/README.md) | `.rvt` projects, built with the IFC import's builder. |
| [mitcad-import-replay](../import-replay/README.md) | The core of the `.f3d` dump replay (mitcad#785): `Importer`'s state, the timeline walk, references, fallbacks, the report. `mitcad-import` re-exports its public paths. |
| [mitcad-import-joints](../import-joints/README.md) | The replay's joints family: joints, joint origins, captured positions, ground items, assembly relationships (mitcad#785). |
| [mitcad-import-sheet](../import-sheet/README.md) | The replay's sheet-metal family: sheet items as native sheet features (mitcad#785). |
| [mitcad-import-primitives](../import-primitives/README.md) | The replay's primitives family: boxes, cylinders, spheres, tori and coils on a plane (mitcad#785). |
| [mitcad-import-appearances](../import-appearances/README.md) | The replay's appearances family: the file's appearances and physical materials on bodies, faces and occurrences (mitcad#791; re-exported as `appearances`). |
| [mitcad-import-solidcraft](../import-solidcraft/README.md) | `.solidcraft` designs with their history (mitcad#785; re-exported as `solidcraft`). |

The imports are generic over the [`Kernel`](../model-base/src/kernel.rs); the
tests run them on the model's mock kernel. The JSON commands and report
fields are in `core/model-api/src/commands.md` (*.f3d import with the
timeline*).

The crate sits above `mitcad-model`, not above the JSON api
`mitcad-model-api` (mitcad#778), so that an edit of the api does not
rebuild the replay. What the replay asks of the document goes to the
model's typed methods (`Document::sheet_state_json`, `sheet_rules_json`,
`sheet_reference_json`, `joint_frame`, `constraint_row`), which the api's
queries of the same names answer with. The one place that runs a JSON
command, a cut and paste moved by Move to Component
(`core/import-replay/src/pasting.rs`), runs it through
[`Options::commands`](../import-replay/src/lib.rs): a `Commands` the
caller makes for its document's kernel from the api's `command`
(`mitcad-ffi`'s `f3d_import::import_commands`, used by the `.f3d` and
`.ipt` imports), still without design policy checks. A caller that
wants such pastes moved must pass `commands`: `Options::default()` has
none, and then such a paste is not moved by a command (`MITCAD_IMPORT_TRACE`
says "no commands to run the move with") and takes the usual route, the
file's history state or its stored bodies, like an item without a
translation. Other crates' tests that run the import pass the api's
`command` the same way (`core/model-tests/imports/src/ipt_design_tests.rs`). Only this
crate's own unit tests, which keep the api as a dev-dependency, run its
commands when they give none (`import_design_with` under `cfg(test)`).

## The replay's crates

The replay is in crates of its own (mitcad#785), so that an edit in a
family above the core rechecks only its crate and this one:

- [mitcad-import-replay](../import-replay/README.md) (`core/import-replay`):
  the core, all of the replay that calls itself both ways (the timeline
  walk in `lib.rs`, `features`, `ops`, `sweeps`, `linked`, the retries,
  the deviations, the learning dump, the notes, ...) and the shared
  helpers (`refs`, `sketch`, `profiles`, `history`, `report`, ...). Its
  timeline walk reaches the families above it only through its
  `Families` trait; its `replay_design` takes them.
- The families above the core, as extension traits on its `Importer`:
  [mitcad-import-joints](../import-joints/README.md) (`Joints`,
  `Relationships`), [mitcad-import-sheet](../import-sheet/README.md)
  (`Sheets`; the core keeps `is_corner` for its edge guesses) and
  [mitcad-import-primitives](../import-primitives/README.md)
  (`Primitives`) and
  [mitcad-import-appearances](../import-appearances/README.md)
  (`Appearances`, mitcad#791; the core keeps the report's types in
  `appearance_report`, since `DesignReport` holds them).
- `mitcad-import` (this crate): `import_design` and `import_design_with`
  over `replay_design` with the families filled in, the core's public
  paths re-exported (`pub use mitcad_import_replay::*`, also the core's
  modules, which are public for the families and the tests but hidden
  from the docs), `.solidcraft` as `solidcraft`, and the replay's tests
  (`src/tests/`), which need the whole replay.

The map the cut was chosen by (which family calls which) is on mitcad#785.

## Documents

This README is the index. The notes are in `docs/`, one document per
format and, for the `.f3d` import, per topic:

| Document | What it holds |
|---|---|
| [docs/f3d.md](docs/f3d.md) | The `.f3d` import: entry points, modules, crashes, limits and the known failure classes. |
| [docs/f3d-history.md](docs/f3d-history.md) | The history as the reference, named inputs, sketch patterns, threads, sweeps, path patterns, what the history settles. |
| [docs/f3d-fallbacks.md](docs/f3d-fallbacks.md) | Fallbacks to the file's stored bodies, the outcomes on the timeline, items suppressed in the file, retries on the file's exact state, the geometric tolerance. |
| [docs/f3d-performance.md](docs/f3d-performance.md) | Time limit, stop and hangs, memory, definitions evaluated in parallel, work beside the replay. |
| [docs/f3d-components.md](docs/f3d-components.md) | Components, components of other documents, derived designs, joints, assembly relationships, items without a translation. |
| [docs/f3d-sheet-metal.md](docs/f3d-sheet-metal.md) | Sheet metal. |
| [docs/f3d-appearances.md](docs/f3d-appearances.md) | Appearances and materials, designs with many face appearances, light bulbs. |
| [docs/f3d-learning.md](docs/f3d-learning.md) | Learning the undecoded inputs: the learning dump and the input decoding audits. |
| [docs/f3d-undecoded.md](docs/f3d-undecoded.md) | What the file does not settle yet, the published format descriptions against the decoder and against the ASM B-rep reader (mitcad_private#198: unknown faces kept, exact helices, analytic blends and extrusions, offset surfaces; what the test files hold of them, compared before and after). |
| [docs/rounding-specs.md](docs/rounding-specs.md) | The rounding specs of the `.f3d` corpora (every fillet and chamfer the kernel fails on, with its input body): where they are kept, the generator, running them again. |
| [docs/f3d-rounds-1.md](docs/f3d-rounds-1.md), [-2](docs/f3d-rounds-2.md), [-3](docs/f3d-rounds-3.md), [-4](docs/f3d-rounds-4.md) | Notes on single rounds of work on the corpus: the censuses, model by model, and the fillet and chamfer rounds. |
| [../import-fcstd/docs/freecad.md](../import-fcstd/docs/freecad.md) | The FreeCAD import (in its crate, `mitcad-import-fcstd`). |
| [docs/ipt.md](docs/ipt.md) | The `.ipt` import. |
| [docs/iam.md](docs/iam.md) | The `.iam` import. |
| [../import-solidcraft/docs/solidcraft.md](../import-solidcraft/docs/solidcraft.md) | The `.solidcraft` import (in its crate, `mitcad-import-solidcraft`). |

The documents cite each other's sections by their titles in italics, with
a link where the section is in another document. [SCHEMA.md](SCHEMA.md)
is the dump format.

## Where new text goes

- A change to an import adds its notes to the document of its topic, as a
  section of its own at the end of the topic's part, so that parallel
  changes touch different places. Not here.
- A note on one round of work on the corpus goes into the newest
  `docs/f3d-rounds-*.md`; a new numbered one is started when that file
  nears 2 000 lines.
- A new topic gets a document of its own in `docs/` and a row in the table
  above. A document that grows past about 2 000 lines is split by topic.
- Cite a section by its document and title (`docs/f3d-history.md`, *Named
  inputs*); when a section moves, update the links to it
  (`grep -rn 'core/import/docs\|core/import/README' .`).
- Unit tests go into the topic module of `src/tests/`; see below.

## Tests and coverage runs

The crate's unit tests run on the model's mock kernel (`cargo test -p
mitcad-import`, the ctest `core.import`); the core's own unit tests,
beside its modules, run with `cargo test -p mitcad-import-replay`
(`core.import-replay`). Those of the `.f3d` import are
in `src/tests/`: `src/tests.rs` only declares the modules, `support.rs`
holds the test kernel and the builders several of them share, and every
other module holds the tests of one topic. The corpus tests of the other
formats are described with them: [FreeCAD](../import-fcstd/docs/freecad.md#tests),
[.ipt](docs/ipt.md#corpus-test), [.iam](docs/iam.md#iam-corpus-test).

### Corpus test

Real designs are kept outside the repository (`MITCAD_F3D_CORPUS`, default
`~/f3d-corpus`). `test_f3d_import` (`core/tests/test_f3d_import.cpp`):

```bash
test_f3d_import --corpus [dir] [--every N] [--max-items N] \
    [--time-limit S] [--hang-limit S] [--reports DIR] [--threads N] \
    [--jobs N] [--memory SIZE] [--file-timeout S]       # build/rel/core
mitcad-cli import-f3d part.f3d --time-limit 300 --report report.json
```

- `--corpus` imports every design of every `.f3d`/`.f3z` under the
  directory, requires each import to finish without error, and prints
  per feature type how items came in (parametric, partial, fallback,
  skipped) and how the final bodies agree with the stored ones. Exits 77
  (skipped) without a corpus. Files are named by position (f01, …), as in
  `core/f3d/CORPUS_REPORT.md`.
- `--reports DIR` keeps each JSON report; `--hang-limit` defaults to 600 s.
- `--threads N` imports with N threads (*Definitions evaluated in
  parallel*, *Work beside the replay*; default 1). Keep `--jobs` ×
  `--threads` within the cores; the reports are those of `--threads 1`
  except where the time decides.
- `--models [dir]` replays the reference models' external dumps
  (`MITCAD_F3D_MODELS`, default `~/f3d-models`) of lofts and sweeps,
  compares with the stored volumes, and imports each model from its own
  streams too.
- `mitcad-cli import-f3d --own-start-time-scale F` (the command's
  `own_start_time_scale`, `Options::own_start_scale`, mitcad#847) tries
  an item from its own start, the route tried first, F times as long as
  usual (half of `item_seconds`: 30 s). An item that gives its state that
  way hangs on that time (a path pattern of 372 copies, 18 s by itself in
  a debug build, took 31 s beside the corpus step), so a debug build,
  about three times slower, gives its slowdown: the ctest
  `f3d.corpus_designs` passes its `SLOWER` (3 in a debug build, 1
  otherwise; `tools/cli/f3d-designs.cmake`), which also scales the
  designs' own time limits, and its designs run at once beside the other
  corpus tests. The item's other routes keep their times: an item that
  falls back by design spends each of them in full, and scaled as well
  they made the test's longest design about 270 s slower (a sweep that
  passes over itself took 362 s instead of 95 s). The factor scales the
  other short time of a route tried first too, a thread's offset of the
  faces it sizes (2 s, mitcad#874: 0.9 s alone in a debug build). The
  times are on the import's clock, which leaves the machine's load out
  ([time limits](docs/f3d-performance.md#time-limit-stop-and-hangs)).
- The ctest `f3d.corpus_timeline` (`core/CMakeLists.txt`) runs `--every 4
  --max-items 30 --time-limit 60` to stay quick in debug builds. Run the
  full corpus with the release build, e.g. `--time-limit 300 --hang-limit
  300` per design.
- Each file (each model with `--models`) is imported in a child process of
  its own, `--jobs N` at a time (default: the cores / 4, bounded by the
  memory available / `--memory`, 4G per child by default), the largest
  first; a crash, a hang past `--file-timeout` (3600 s) or a file over its
  memory ends only that file, which counts as failed. The lines come in
  the files' order and the totals are those of `--jobs 1` (every file in
  the one process, as before); [docs/development.md](../../docs/development.md#f3d-corpus-tests)
  has the options.

#### Coverage runs in parallel

A full coverage run, for numbers before and after a change: build both
states with the release preset (keep the binaries of the first), then run
each with every file and compare. The lines are those of a run in one
process, so only the times differ:

```bash
B=build/rel/core   # the binaries of one state
$B/test_f3d_import --corpus --time-limit 300 --hang-limit 300 --jobs 8 \
    --reports before-reports > before.txt
$B/test_brep_import --corpus --jobs 8 > before-bodies.txt
$B/test_exchange --corpus --jobs 8 > before-import.txt
$B/test_f3d_import --models --jobs 8 > before-models.txt
# ... the same with the other state's binaries into after*.txt, then:
diff <(sed -E 's/[0-9.]+ m?s$//' before.txt) <(sed -E 's/[0-9.]+ m?s$//' after.txt)
```

The children's memory adds up: keep `--jobs` × `--memory` within what
the machine (or `systemd-run --user --scope -p MemoryMax=...`) allows. The
line on standard error at the end gives the largest peak memory of a
child and the longest child, e.g. to choose `--memory`. Under the test
runner (`tools/testrun`, mitcad#832) the corpus tests also read their
files' last times from `MITCAD_CORPUS_FILES` (the longest start first)
and append each child's time and peak to `MITCAD_CORPUS_RECORD`; neither
is set in a run by hand, which orders the files by size as before.

