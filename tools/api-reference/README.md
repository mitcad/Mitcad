<!-- SPDX-License-Identifier: MIT -->
# The generated API reference

A reference of MitCad's model API for users and integrators (scripts,
`mitcad-cli`, MCP clients), made from the sources (mitcad#2606):
`core/model-api/src/commands.md` stays the one source of truth, and the
reference is generated again from it for each release. It is published
with the website at <https://mitcad.org/docs/api/>.

```bash
tools/api-reference/build.sh [<folder>] [--offline]
```

writes the pages into `<folder>` (default `build/api-reference`) and the
report beside it (`<folder>.report.txt`, not published). It runs the
example `core/mcp/examples/api_reference.rs` with cargo; nothing is
fetched beyond what cargo needs to build (`--offline`: nothing at all).
The run fails, writing nothing, when a page is not clean (below), or
when `<folder>` holds files that are not pages of this reference (an
earlier run's pages that are gone, or anything else put there; it lists
them): the folder is published as it is, so give a new or an empty one.
A run into the folder of an earlier run with the same pages replaces
them.

## What it reads

- `commands.md` as MCP's agents see it (`model_reference`): without the
  sections marked `<!-- mcp: internal -->` under their heading and
  without the sections whose command and query tables name only what MCP
  does not permit.
- The commands' JSON Schemas (`core/mcp/src/schemas.rs` and
  `core/mcp/src/schemas/`): a command's purpose, its fields (type, unit,
  default, whether required, description), its restrictions and its
  examples.
- The names `model_reference` lists: the commands and queries an
  external client may run (`core/model-api/src/classes.rs`) and the
  feature types (`FeatureDef::TYPE_NAMES`).
- The queries' purposes (`core/mcp/src/query_purposes.rs`, mitcad#2636):
  a sentence or two per query on what it is for, as a command's schema
  gives its purpose; `model_reference` answers the same sentence as a
  query's `purpose`. A new permitted query appends its own.

## The pages

| Page | Holds |
|---|---|
| `index` | the commands and queries by their sections, the feature types, the topics, and a search (`search.js`, `search-index.js`, in the page; no server) |
| `overview` | how commands, queries, preview and undo work; values, units and expressions; identities and topological names; references to geometry; errors |
| `commands/<name>` | its purpose, its fields (from its schema, else from the reference's table), an example (the schema's, else the reference's JSON that names it), its result, its refusals and errors, the reference's own text of it and the other sections that describe it |
| `queries/<name>` | the same for a query, its purpose from the queries' purposes |
| `features/<type>` | its purpose, its fields (the list items of its section), an example `add_feature` with its definition, and its section |
| `topics/<section>` | each `##` section of the reference with its subsections |

Each page is written as `.html` (with `style.css`) and `.md`.

## Cleaning

The reference is the developers' document; the generated pages leave out
what a user does not need, a sentence at a time
(`core/mcp/src/api_reference/clean.rs`):

- Ticket references go: `(mitcad#714)`, `(#12, #14)`, `GitHub #557`;
  a parenthesis keeps the rest of its text (`(mitcad#689: …)` keeps `…`).
- File and test names in parentheses go
  (`` (`core/model/src/features/renamed.rs`) ``).
- A sentence is left out when it is history (`Before, …`, `since
  mitcad#…`, `until 0.x`, a date), is about tests (`Tests: …`), names a
  file or a test outside parentheses, or is about the code inside (OCCT,
  its port patches, the mock kernel, the corpora); `…; before, …` ends at
  the semicolon.
- A sentence that still names a ticket after that (`see mitcad#12`,
  `mitcad#557's design`) is left out as *could not be cleaned*.

Every sentence left out is listed in the report with its page and why.
Where a sentence is only history, leaving it out is right; where it says
something a user needs, rewrite it in `commands.md` without the ticket
or the history, or move a whole section that is about the code inside
under `<!-- mcp: internal -->`. The report also lists the names without
an example or a description (*Gaps*); an example in `commands.md` (a
code block or inline code `{"query": "<name>", …}`) gives the page one.

## The check

The tests of `core.mcp` (`core/mcp/src/api_reference_tests.rs`) make
the reference with the names `model_reference` lists and check that
every command, query and feature type has a page that the index and the
search list, that no page names a ticket, history, a file, a test or the
code inside, that every link finds its page and heading, and the pages
of `add_feature` (with an extrude), `sketch.add_line`, `bodies`,
`measure` and `extrude`: their fields, a JSON example and their result.
Every query page opens with its purpose (mitcad#2636), and every query
`model_reference` lists has one, a clean sentence the cleaning keeps
whole (the tests in `core/mcp/src/query_purposes.rs`). An output folder
with files that are not pages of the reference is told. The example
program itself is built by the same cargo test build.

## Publishing

At a release, from the release's tagged sources (their version is the
one the pages name), into a new folder:

```bash
tools/api-reference/build.sh build/api-reference-<version>
```

1. The folder `build/api-reference-<version>` goes to the website as
   `/docs/api/`, its files as they are, replacing the earlier release's
   (`index.html` is the start page; the links between the pages are
   relative, so the folder works under any path). The report
   (`build/api-reference-<version>.report.txt`) stays out.
2. The report is read: a sentence under *could not be cleaned*, or one a
   user needs under the other reasons, is rewritten in `commands.md`
   (without the ticket or the history) for the next release, and so is a
   name under *Gaps*.
3. After the upload, <https://mitcad.org/docs/api/> opens the index, and
   the pages of `add_feature`, `sketch.add_line`, `bodies` and `measure`
   read cleanly.
