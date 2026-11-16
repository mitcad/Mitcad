# .iam import

Part of the [import documents](../README.md#documents).

mitcad#60, stage 4. [core/ipt](../../ipt/README.md#assemblies) reads the
assembly (referenced files, occurrences, placements, the range boxes and
display transforms the file stores); `core/ffi/src/iam_import.rs` makes
MitCad's components and occurrences of them and imports each part with
the `.ipt` import above, in a document of its own: its bodies become a
base feature of a component, or with `--history` its replayed design is
copied into one (`Document::add_component_copy`). Entry points: `mitcad-cli
import-iam assembly.iam [--save out.mitcad] [--report report.json]
[--search folder]... [--history [--no-verify] [--no-fallback]
[--no-compare] [--time-limit S]] [--json]` (`import-ipt` takes an `.iam`
too), the `import_iam` command
([commands.md](../../model-api/src/commands.md#iam-import)) and File › Open
or Import in the application (the import process, as for parts).

- Each part file comes in once, its stored bodies as base features (with
  `--history` its features as the `.ipt` import replays them), and each
  occurrence is an occurrence of its component;
  sub-assemblies are components with their own occurrences.
- Referenced files are found as saved, relative to the assembly as it was
  saved, by the saved path's tail, or by name in the search folders; a
  part found so is checked to be the referenced document by the version
  id both files store. Files that are not found are empty components,
  placed, and reported.
- A part without bodies says why (mitcad#1913): its file holds none, or
  its result lists bodies the import did not find. An assembly converted
  from another format (a building model) keeps the conversion's
  translation report; what it says of such a part (that no bodies were
  read from the source file) is on the part's line of the report and in
  a warning.
- Checks against the file: each placement against the transform the file
  displays the occurrence with, each part's bodies within the range box
  the file stores for it (parts saved again after the assembly only
  reported).
- Not yet: constraints and joints (the stored placements are taken as
  they are), design views and positional representations, the model state
  an occurrence uses (the part comes in in its active state).

## .iam corpus test

`iam.corpus` ([tools/cli/iam-corpus.cmake](../../../tools/cli/iam-corpus.cmake))
imports every `.iam` under `MITCAD_IPT_CORPUS` with `mitcad-cli
import-iam`, the corpus folders searched by name, each in a child process
of its own, several at once (as `ipt.corpus`). Every occurrence that is
not suppressed must be placed, every part file found must import, every
placement must agree with the displayed one and every part's bodies must
lie within their stored range box, and every part whose result lists
bodies must bring them (a part without geometry is one whose file holds
none; the import says why, with what a converted assembly's translation
report says of it, mitcad#1913); files that are not in the corpus
(library parts, files of other projects) are reported, not failed.

Results (dev build, 2026-10-09) on a private project tree of 166
assemblies: see [core/ipt/README.md](../../ipt/README.md#results-on-the-assemblies).

