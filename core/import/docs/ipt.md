# .ipt import

Part of the [import documents](../README.md#documents).

mitcad#60. The code is not in this crate: [core/ipt](../../ipt/README.md)
reads the file (compound file, property sets, segment database, the B-rep
record, the definitions segment) and decodes the part's design into this
crate's dump IR (`mitcad_ipt::design`); `core/ffi/src/ipt_import.rs` runs
the import and `core/ffi/src/ipt_history.rs` gives the bodies of the ASM
history the B-rep record carries (`StoredGeometry`), state by state, as
the `.f3d` import's `F3dGeometry` does for the `.smbh` blobs. Entry
points: `mitcad-cli import-ipt part.ipt [--save part.mitcad] [--report
report.json] [--reference part.stp [--max-relative X] [--deviation]]
[--bodies-only] [--no-verify] [--no-fallback] [--no-compare] [--time-limit
S] [--hang-limit S] [--dump design.json] [--design design.json] [--json]`, the `import_ipt`
command ([commands.md](../../model-api/src/commands.md#ipt-import)) and File ›
Open or Import in the application (the import process, as for `.f3d` and
FreeCAD documents; Cancel only).

- **With the history** (stages 2 and 3, the default): `import_design`
  replays the dump as it replays an `.f3d` design: the part's parameters
  with their expressions (`params.rs`), sketches, work planes and
  features, each checked against the state of the ASM history its
  operation made (the definitions segment's state table names it), with
  the fallbacks, the guesses the history settles (profiles, fillet and
  chamfer edges, hole positions, mirror planes) and the final comparison
  of this crate. The items are those of the browser, in its order.
  With `Options::validate` a definition that gives an item's state but
  leaves a body OCCT's checker finds invalid is not taken: the item takes
  the file's bodies (a cut checked only near its change made a part's
  body invalid).
  An item whose state the history does not hold, between states it does
  (a feature suppressed in the file, `StoredGeometry::item_without_result`),
  is skipped as one the file keeps no result for, as an `.f3d` item with
  result number -1 is: before, its definitions were tried against the
  states up to the next item's, and it was skipped or taken into a later
  fallback, often with a reason that did not apply (*its result is not in
  the file's history*, *the extrusion does not cut into any participant
  body*, *its edges were not decoded …*).
  A stored body of several lumps (disjoint solids in one body, as a join
  of a piece that touches nothing makes it in the file) comes in as one
  body per lump in the history states and the stored design, as the
  replay makes them (MitCad keeps disjoint solids apart); before, such a
  join's replay had one body more than its state and fell back. With `hang_limit` (`--hang-limit`; the application 90 s,
  `ipt.corpus` 300 s) the replay runs on its own thread, watched as the
  `.f3d` import's (*Hangs* above): an item whose kernel call does not
  return (a fuse of two revolved regions that never ended) takes the
  file's bodies in the next try.
- **Bodies only** (`--bodies-only`, and files without a definitions
  segment): one base feature per body, solids and sheet bodies alike; a
  body that OCCT cannot build is reported (`skipped`) and left out.
- The document's length unit becomes the part's (the model settings'
  unit code); the material is set when MitCad's library has it.
- Appearances (mitcad#1367, `core/ipt/README.md`, *Appearances*): with
  the history, each stored body takes the part's default appearance, or
  the one style all its faces have, and its faces of other styles theirs,
  as the `.f3d` import's appearances (`mitcad-import-appearances`; the
  report's `appearances`). An appearance of the file's `Protein` package
  comes in with its parameters; a library preset the file does not hold
  as a generic appearance of the style's own diffuse colour and opacity.
  The physical material is recorded where the package holds it. The
  import of the bodies only sets the same on its bodies (mitcad#1375, the
  report's `appearances` and a line of the text report), and a stored
  body of several solids gets its appearance on each of them.
- The report lists the part number, material, units, the release that
  saved the file, each B-rep record (ASM version, bodies, the states of
  the ASM history it carries) and each body (solid or sheet, valid,
  volume, area, faces); with the history also the parameters (records,
  model, user, those not the part's), the expressions (translated,
  agreeing with the stored values), the features and the replay's report.
- Assemblies (`.iam`, stage 4) are an import of their own, which imports
  each part this way ([*.iam import*](iam.md#iam-import) below).
- Not yet: body names, files whose B-rep record is not an ASM
  file (older releases); what the
  definitions segment does not tell (core/ipt/README.md, *What the files
  do not tell*) comes in as the bodies of the history states.
- A hole whose decoded position does not give its state also tries the
  history's guess (each piece of material the state removes); that guess
  cuts the replay's body with the state's, and when the two are nearly
  coincident (a body a fallback took to the previous state) OCCT's boolean
  can take minutes (two minutes for one hole of the test files): the item
  then has no time left for its definitions and falls back.

## Corpus test

Real part files are kept outside the repository (`MITCAD_IPT_CORPUS`, a
path list of folders). The ctest `ipt.corpus`
([tools/cli/ipt-corpus.cmake](../../../tools/cli/ipt-corpus.cmake)) imports
every `.ipt` with `mitcad-cli import-ipt` (with the history), each in a
child process of its own, several at once (`mitcad_run_parallel`;
`MITCAD_CORPUS_JOBS`, `MITCAD_CORPUS_MEMORY`, `MITCAD_CORPUS_TIMEOUT`,
`MITCAD_CORPUS_JOBS=1` checks them in the script's process). Every body
must be built and valid, every expression of the part's parameters must
agree with its stored value (in the file and in MitCad), and the final
bodies must be the file's; the line per file gives the features' outcomes.
A file with a STEP file of the same part next to it (`<name>.stp` or
`.step`) is compared with it: as many solids, each one's volume and area
within 1e-6 relative, unless the folder's `references.tsv` gives the file
a limit of its own (`<name>.ipt`, a tab, the limit, a tab, why). Such a
limit is for a reference that was not exported from the file and models
the part a little differently: it is the difference measured, rounded up
by a few per cent, so that a change in what the import reads still fails.
`core/ipt/tests/corpus.rs` checks the same files without OCCT: the B-rep
records split out of their segments and every body converts cleanly, the
parameters and their expressions, the features' history states.

The test corpus: 16 public NIST MBE PMI test parts
(11 saved by a 2021 release, ASM 226 and one 227; 5 by a 2024 release, ASM
229), each with STEP files of the same test case that the institute
published separately, made from other CAD systems' models of the case
(sources, terms and checksums: the test-file repository's `ipt/README.md`).
Results (dev build, 2026-10-08):

- Every file reads; every B-rep record splits out of its segment; all 22
  bodies (16 solids, 6 one-face sheets) convert without issues and are
  valid for OCCT's checker.
- The STEP references: one part matches within 1e-6 (3.3e-7). The others
  differ by 5e-4 to 4e-2 in volume or area, because the references model
  the parts differently (different face counts: 270 against 255 faces,
  6 against 8; one plate 3.0 mm thick in the reference, 3.02 mm in the
  part); no published STEP file was exported from these part files (each
  part was compared with all of its case's STEP files, AP203 and AP242:
  none matched better). Their limits are in `references.tsv`.
- Three parts saved by both releases (two saves made independently, ASM
  226 and 229) give the same solids: volume and area within 5e-7 through
  a STEP round trip, 3e-9 between the build reports; two parts were
  changed between the saves.
- With the history (2026-10-09): 2,785 parameters with their expressions,
  all agreeing; 344 features with their history states; of 607 timeline
  items 450 parametric, 53 partial, 92 fallback and 12 skipped (per kind:
  [core/ipt/README.md](../../ipt/README.md#results-on-the-test-files)); the
  final bodies are the stored ones in every part.
- Import times: bodies only 0.12 to 0.83 s per file and 45 to 63 MB (dev
  build, which optimises dependencies); with the history 0.2 s to 3 min
  (most under 20 s; the slowest has the hole above). The corpus test
  takes about 3 minutes with 6 files at once.

