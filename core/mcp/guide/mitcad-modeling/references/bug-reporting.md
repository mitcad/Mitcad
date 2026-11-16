# Bug reporting

<!-- SPDX-License-Identifier: MIT -->

Read this when a result is unexpected: a refusal or a failed feature you
cannot explain, geometry that looks or measures wrong, a command that
succeeds but does the wrong thing, or reference text that misleads. It
says what evidence to collect, how to collect it with these tools, and
gives a report template. Collect what you can; mark what you cannot, and
why. Preparing a report does not authorize filing or uploading it.

## Ground rules

- Inspect and copy before you experiment. Do not overwrite the user's
  design or change the live document and its undo history just to gather
  evidence (see [Safe collection](#7-safe-collection-and-delivery)).
- Separate observations (what a tool returned, quoted exactly) from
  hypotheses (what you think the cause is) and from feature wishes (what
  you would like MitCad to do). A root cause is confirmed only by evidence,
  never by its plausibility.
- Never fabricate a log line, a screenshot, a response, a clean replay or
  a version. When evidence is missing, say which and why ("headless
  server: no screenshot of the window", "the client does not show the
  server version").
- A replay is verified only when you ran it and it reproduced the
  result; otherwise it is a proposed reproduction recipe. Say which.

## 1. Summary, intended result and impact

Write one sentence of what the user tried to make or do, then:

- **Expected:** the result wanted, and where the expectation comes from
  (the user's words, a drawing, the reference section, a dimension).
- **Actual:** what happened, quoted from the result.
- **Impact:** blocks the work, gives incorrect geometry or data, or only
  impairs usability (a confusing message, an extra step).

## 2. Provenance

Report only what you read; write "unknown" for the rest.

- **Server version and build:** in the live application `ui_state` gives
  the application's version and build. The serverInfo.version of the
  initialize result is the server's version in both servers; your client
  may show it. Never substitute the client's own version.
- **Mode:** live GUI (the ui_* tools exist) or headless (`document_create`
  exists); the client and its transport (the application's connection or
  stdio) when known.
- **OS and architecture:** from the environment you run in, when known.
- **The document:** its units and active component from the `document`
  query (`units.length`, `active_component`); in the live application the
  handle and active flag from `document_list`.
- **Relevant settings:** the definitions of the features involved
  (`feature`), the parameters (`parameters`), and for printing the
  settings a body gets (`print_settings`).

```json
{"name": "ui_state", "arguments": {}}
{"name": "document_list", "arguments": {}}
{"name": "model_query", "arguments": {"document": "<handle>", "query": {"query": "document"}}}
{"name": "model_query", "arguments": {"document": "<handle>", "query": {"query": "parameters"}}}
```

## 3. Reproduction

- **Starting state:** an empty design (`document_create` headless,
  `document_new` live), a saved file named in the manifest, or the user's
  open design as described by its timeline.
- **Steps:** the smallest reliable ordered sequence of the actual tool
  requests, each verbatim, with the complete relevant response and the
  state you expected after each decisive step. Prefer one
  `model_command` array that rebuilds the state from nothing.
- **The failing entry:** its command, its parameters and its index.
- **Keep the outcome fields** of every decisive response: isError,
  `applied`, `entry` (a refused array entry), stopped_at (an applied
  entry whose feature failed), `results`, `failed_features` with `edited`
  and `new`, `warnings`, `notices`, `places`, `detail` and the full
  `error`. Do not shorten identifiers in the copy you keep.
- **Refused or failed:** a refused command changes nothing (an array's
  earlier entries are undone too); a feature that fails after it is
  applied stays in the timeline with an error. Say which one happened.
- **Reproducibility:** how many runs, how many reproduced it, and whether
  the replay was run (verified) or only written (proposed).

A step as the report keeps it:

```text
Step 3 request:
{"name": "model_command", "arguments": {"document": "<handle>", "command": [
  {"cmd": "set_parameter", "name": "r", "value": 6},
  {"cmd": "edit_feature", "uid": "F3", "def": {"...": "the whole definition sent"}}]}}
Step 3 response (structuredContent, complete): {"isError": true, ...}
Expected after step 3: F3 green, one solid body.
```

## 4. State and geometry evidence

Read the state that bears on the failure, before and after the decisive
step:

```json
{"query": "timeline"}
{"query": "feature", "uid": "F3"}
{"query": "sketch", "uid": "F1"}
{"query": "profiles", "sketch": "F1"}
{"query": "bodies", "properties": true}
{"query": "design_review", "scope": "all", "print": false}
```

- **Timeline and features:** each involved feature's `status`, `error`,
  `warnings` and `notices`.
- **Sketches:** whether it solves, its `dof`, the entities and constraints
  involved, and the profiles a feature consumes.
- **References:** the exact face, edge and region names a feature uses,
  copied from its definition or a query, never retyped.
- **Bodies:** counts, kinds, `volume`, `area` and bounding boxes; measure
  the decisive sizes with `measure` (its selections:
  `model_reference {"query": "measure"}`), and for a parameter with limits
  run `parameter_range`.
- **Pictures:** a context view and a close-up with a reproducible camera.
  `model_render` draws the design without changing the view or the
  document; keep each result's `camera` (pass it back as view to draw the
  same picture) and add a section through the suspect place. In the live
  application `ui_screenshot` shows what the user sees.

```json
{"name": "model_render", "arguments": {"document": "<handle>", "view": "iso", "fit": "all"}}
{"name": "model_render", "arguments": {"document": "<handle>", "view": "front", "fit": ["F3.b0"], "highlight": ["F3.b0"], "section": {"origin": [0, 0, 10], "normal": [0, 1, 0]}}}
```

A green status, a changed face count or a picture alone proves neither
that geometry is right nor that it is wrong: compare measurements with the
expectation, and say what a picture shows and what it cannot.

## 5. Artifacts

- **The design:** the exact state that reproduces it, with the linked
  designs and inputs it needs, only when the user allows it. Headless:
  `document_save` to a new path (not overwrite: true). Live: do not save
  under a new name just to gather evidence (it changes the window's
  file); ask the user, or point them to Help › Send Feedback with Include
  the design, which packs the open design, the designs it links, a
  screenshot and the log into a zip without changing the design.
- **Replay:** the requests and responses of the steps, as a JSON lines
  file, one request and one response per line.
- **Images and logs:** the original files (`model_render` and
  `ui_screenshot` write them with path), and `ui_log` lines in the live
  application, unedited.
- **Manifest:** each file's relative name, size and SHA-256
  (`sha256sum <file>`, `Get-FileHash <file>`). Check that every file the
  report names exists, and that the saved design reopens with the reported
  parameter values (`document_open` of the copy, then the `parameters`
  query).
- **Reduction:** keep the original evidence first; then prefer a small
  synthetic design that still shows the fault, and say that it does.

## 6. Scope and limits

- Versions and modes actually tested; "not tested in the GUI" when you
  used the headless server only: an untested GUI claim is not a confirmed
  bug.
- A bounded neighbourhood where it helps: the nearest value that works and
  the one that fails (radius 2.4 mm succeeds, 2.5 mm fails).
- Workarounds found, related known issues when you know them, and the
  evidence that is untested or unavailable.
- No broad corpus or performance runs for an ordinary report.

## 7. Safe collection and delivery

- Read with `model_query`, `model_render`, `ui_state`, `ui_log` and
  `ui_screenshot`; they change nothing. Experiment on a copy (the headless
  server with a saved copy) rather than on the user's design.
- If an experiment changed the live document, take it back with `undo`
  (one per command or array) and check that the `document` query's
  `undo_depth` is back where it was.
- Keep raw evidence in storage the user authorized, and write a sanitized
  report to share: no credentials, account identities or machine paths
  (relative names only), nothing unrelated to the fault.
- The user's models go only to an authorized attachment or private test
  data, never into public source code.
- Filing or uploading is the user's decision or an explicitly authorized
  workflow's: say which files you prepared and where they are.

## Report template

```markdown
# <Short title: what fails, where>

## Summary
- Tried to: <what the user wanted to make or do>
- Expected: <result> (source: <user / drawing / reference section>)
- Actual: <quoted result>
- Impact: <blocks work | incorrect geometry or data | usability>

## Provenance
- Server: <version and build, or unknown>; mode: <live GUI | headless>
- Client and transport: <name, version, transport, or unknown>
- OS and architecture: <or unknown>
- Document: <units>, active component <name>; settings: <relevant>

## Reproduction (<verified by running it | proposed, not run>)
- Starting state: <empty design | file in the manifest | described>
- Steps: <each request verbatim, its complete relevant response,
  the expected state after it>
- Failing entry: <index, command, parameters>; outcome: <refused | applied
  and failed (stays in the timeline)>
- Reproducibility: <n of m runs>

## State and geometry evidence
- <queries and their results; measurements; pictures with cameras>

## Observations
- <facts read from results>

## Hypotheses
- <possible causes, each with the evidence for and against>

## Scope and limits
- Tested: <versions, modes>; nearby values: <works / fails>
- Workarounds: <...>; related issues: <...>
- Not available: <evidence and why>

## Artifacts
| File | Bytes | SHA-256 |
|---|---|---|
| <relative name> | <n> | <hash> |
```

## Two short examples

These show the form, not real results.

**A failed feature.** Observations: `model_command` with
`{"cmd": "edit_feature", "uid": "F3", ...}` raising a fillet's radius
from 2 to 6 answered isError true, `applied` true and `failed_features`
`[{"uid": "F3", "edited": true, ...}]` with its `error`; the `timeline`
query shows F3 with `status` error and the body unchanged since F2. A
radius of 2.4 mm works and 2.5 mm fails; the edge is 5 mm long.
Hypothesis: the radius exceeds what the neighbouring face allows. Missing:
no screenshot (headless server). Artifacts: the replay and the saved
design, each with its SHA-256.

**An unexpected success.** Observations: a cut answered isError false
and every feature is green, but the `bodies` query lists two solids
where one was expected, and the second has a `volume` of 3.1 mm³. A
section render through the cut shows a thin sliver; its camera is kept.
Hypothesis: the cut's depth leaves a thin skin. Not a confirmed bug until
the expected depth is checked against the reference; the user's drawing
asks for a through cut.
