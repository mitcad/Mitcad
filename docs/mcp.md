<!-- SPDX-License-Identifier: MIT -->
# Mitcad Model Context Protocol server

`mitcad-cli mcp` exposes Mitcad's native parametric model to an MCP client
without opening the desktop application. One process owns up to 64 open
documents, each identified by an opaque handle such as `D1`. Handles last
until the document closes or the process exits. The server uses the same
JSON commands, queries and OCCT geometry kernel as the application.

## Start and configure a client

Create a directory for the files the client may access, then start the
native executable with an absolute workspace path:

```bash
/absolute/path/to/mitcad-cli mcp --workspace /absolute/path/to/designs
```

On Windows use `mitcad-cli.exe`. `--workspace` is required and must name
an existing directory. Add `--read-only` to inspect existing designs;
this permits opening, listing, querying and closing documents, and
refuses creation, model commands, saving and exporting.

The executable is the `mitcad-cli` build target, usually under
`build/dev/tools/cli/`; use its actual absolute path for the chosen
platform and build configuration. It needs the same native libraries as
the other CLI commands. See [development.md](development.md) for builds
in the approved isolated environments.

For clients that accept an `mcpServers` configuration, the setup is:

```json
{
  "mcpServers": {
    "mitcad": {
      "command": "/absolute/path/to/mitcad-cli",
      "args": ["mcp", "--workspace", "/absolute/path/to/designs"]
    }
  }
}
```

On Windows, JSON accepts forward slashes, for example
`"command": "C:/Mitcad/mitcad-cli.exe"` and workspace `"C:/Designs"`.
Use the equivalent command-and-arguments fields if the client uses a
different configuration layout. The client launches the process and
owns its stdin and stdout pipes; no HTTP listener, desktop session or
plugin package is required. stdout carries only MCP messages and stderr
carries diagnostics. Closing stdin ends the session; unsaved documents
then leave memory without being written.

## Tools and results

`tools/list` supplies the full input schemas. All arguments are JSON
objects; fields below without a default are required.

| Tool | Arguments | Result |
|---|---|---|
| `document_create` | `{}` | `document`, null `path`, `state`, `timeline`, `bodies`, `applied` |
| `document_open` | `path` | New `document`, canonical `path`, recomputed `state`, `timeline`, `bodies`, `applied` |
| `document_list` | `{}` | `documents`: objects with `document`, `path`, `state` |
| `document_close` | `document`, `discard` (default false) | `document`, `closed`; refuses unsaved changes unless `discard` is true |
| `model_command` | `document`, `command` (native JSON object) | `document`, `applied`, native `result`, `state`, `timeline`, `bodies` |
| `model_query` | `document`, `query` (native JSON object) | `document`, native JSON in `result` |
| `document_save` | `document`, `path`, `overwrite` (default false) | `document`, canonical `path`, saved `state` |
| `model_export` | `document`, `path`, `format`, optional `bodies` (array of names or UIDs), optional `sketch` | `document`, canonical `path`, native export `result` |

Each tool result includes both `structuredContent` and a text content
block containing the same JSON. For example, `model_query` with
`{"document":"D1","query":{"query":"bodies","properties":true}}`
returns an object whose `result` is the native bodies array. The
`timeline` snapshot is the native timeline object, including its
`features` array; `state` is the native `document` query.

Export formats are `step`, `iges`, `brep`, `stl`, `obj`, `3mf` and `dxf`.
DXF requires a `sketch` name or UID and excludes `bodies`; other formats
accept `bodies` and exclude `sketch`. Export requires a new output path.
OBJ may also write the adjacent `.mtl` file, which must also be a new
path inside the workspace. STEP, IGES and BRep preserve exact geometry;
STL, OBJ and 3MF tessellate it. Document save always writes a single
self-contained `.mitcad` file, even in a project with a B-rep store.

The command reference is available through `resources/list` and
`resources/read` at `mitcad://reference/commands`, as well as in
[the native API reference](../core/model/src/api/commands.md). Use that
reference for feature definitions, sketch entities, parameters and
topological names. The server permits the modeling commands and queries
that operate on document data; file imports, external component refresh,
raw save/export commands, disk cache access and remote operations are
refused. Files are accessed through the dedicated document and export
tools. The allowlists are in
[core/ffi/src/mcp.rs](../core/ffi/src/mcp.rs).

## Create, edit, save and export a part

After initialization, call `document_create` with `{}` and retain the
returned handle. The following `tools/call` parameter objects assume
that handle is `D1`. They create a 30 × 20 × 10 mm box:

```json
{
  "name": "model_command",
  "arguments": {
    "document": "D1",
    "command": {
      "cmd": "add_feature",
      "def": {
        "type": "box",
        "plane": {"type": "origin", "plane": "xy"},
        "corner": [0, 0],
        "length": 30,
        "width": 20,
        "height": 10,
        "operation": "new_body"
      }
    }
  }
}
```

Inspect the geometry with:

```json
{"name":"model_query","arguments":{"document":"D1","query":{"query":"bodies","properties":true}}}
```

The body volume is 6000 mm³. `model_query` with
`{"query":"timeline"}` identifies the feature (`F1` in this new
document). Edit its definition to make the height 15 mm, recompute
through the model command, and inspect the resulting volume, 9000 mm³:

```json
{
  "name": "model_command",
  "arguments": {
    "document": "D1",
    "command": {
      "cmd": "edit_feature",
      "uid": "F1",
      "def": {
        "type": "box",
        "plane": {"type": "origin", "plane": "xy"},
        "corner": [0, 0],
        "length": 30,
        "width": 20,
        "height": 15,
        "operation": "new_body"
      }
    }
  }
}
```

Save the editable model, export the solid, and close the saved document:

```json
{"name":"document_save","arguments":{"document":"D1","path":"box.mitcad"}}
{"name":"model_export","arguments":{"document":"D1","path":"box.step","format":"step"}}
{"name":"document_close","arguments":{"document":"D1"}}
```

These are three separate calls, each sent after the previous response.
Reopen with `document_open` and `{"path":"box.mitcad"}` and use its new
handle for queries. Existing save destinations require an explicit
`"overwrite":true`; export destinations must be new. Use actual IDs
returned by queries when extending an existing design. Native lengths
are in millimetres and angles in radians, with expressions such as
`"15 mm"` or `"30 deg"` supported by the model.

## Transport, errors and limits

The transport is newline-delimited UTF-8 JSON-RPC 2.0 over stdio, with
one object per line. The supported MCP version is `2025-11-25`. A client
first sends `initialize` with `protocolVersion`, `capabilities` and
`clientInfo`, reads the negotiated response, then sends
`notifications/initialized`. If it proposes an older or unsupported
version, the server responds with `2025-11-25`; the client must support
that version to continue. Discovery and tool calls require the completed
handshake. `ping` is available; resource templates are empty. Requests
use string or integer IDs and batches are unsupported.

This follows the [2025-11-25 lifecycle and version negotiation](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle).
Clients using the newer lifecycle may probe `server/discover`; this
server returns method-not-found (`-32601`), allowing them to fall back
to `initialize`, as described by the
[official SDK's protocol-version guide](https://ts.sdk.modelcontextprotocol.io/v2/protocol-versions).

Malformed JSON and invalid requests return JSON-RPC errors; unknown
methods and invalid tool parameters also return protocol errors. A
notification never invokes a tool or mutates a document and has no
response. Model, workspace and file errors return tool results with
`isError:true`, usually `structuredContent:{"error":"..."}`. A failed
command validation leaves the document unchanged.

A valid command can add or edit a feature whose geometry fails during
recompute. That result has `isError:true` **and** `applied:true`, with the
document's current state, timeline and bodies. Inspect the feature
errors, then edit or undo; do not treat this result as proof that nothing
changed. Opening a document with failed geometry similarly returns a
handle and an error snapshot. Export refuses documents with failed
features. File IO errors can leave partial output; inspect or remove it
before choosing a retry destination.

Paths are relative to the workspace or absolute inside it. Parent
traversal (`..`) and symlinks leading outside the workspace are refused;
output parent directories must already exist. Project discovery stops
at the workspace root. Version 3 documents can read their existing
B-rep store inside that root. Opening does not refresh linked
components, read per-user display state or configure the global library
resolver. Embedded bodies remain usable. Installed fonts may be used
when evaluating sketch text. Workspace path checks are intended for a
local client; they do not isolate the process from concurrent filesystem
changes or impose operating-system resource limits.

Requests run synchronously, one at a time. This version does not report
progress or interrupt a running geometry operation;
`notifications/cancelled` cannot cancel that operation. A client may
terminate the process, losing unsaved session documents. There is no
background autosave. Document and B-rep reads have size limits; large
geometry computations can still use substantial time and memory.

## Validation

The integration source [tools/cli/mcp-test.py](../tools/cli/mcp-test.py)
launches the actual CLI and covers initialization, discovery, two document
handles, box modeling and volume, save/open, STEP export, protocol errors,
notifications, workspace boundaries, read-only mode, version negotiation
and EOF cleanup, plus persisted geometry failures, symlink escapes and
OBJ companion-file conflicts. Protocol unit tests accompany the Rust
protocol crate; the native CLI integration tests exercise the FFI backend.
Builds, tests, formatters and dependency checks for this change
remain pending in the external validation environment. Run the focused
`cli.mcp` and `core.mcp` CTests there, followed by the repository's full
checks, as described in [development.md](development.md#mcp-validation).
