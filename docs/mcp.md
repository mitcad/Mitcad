<!-- SPDX-License-Identifier: MIT -->
# MitCad Model Context Protocol server

## Connect from MitCad

To work with an agent inside MitCad instead, open the chat panel (**View ›
Chat**): it starts your own Claude Code or Codex with this window's live server as
its only tool source and asks you before file operations; see [the chat
panel](ai-chat.md).

For a design already open in MitCad, choose **Help › Connect AI Assistant…**.
The assistant indicator (a click with no assistant connected, or **Connect
AI Assistant…** in its menu) and **Preferences › AI Assistants** offer the same command; command search also finds `claude`, `codex`, `chatgpt` and `mcp`.
Opened from Preferences, the dialog opens over Preferences and takes
input while Preferences stays open; when Preferences closes, the dialog
stays open over the main window (mitcad#760).
The dialog has three parts: the assistant (**Claude Desktop**, **Claude
Code**, **ChatGPT / Codex** or **Other**; the dialog's lines say just
Codex), a state line with one button, and the folder line. Everything else is under **Advanced**.

The state line names the next step, and its button does it:

- ***Assistant* was not found on this computer** with **Get Claude
  Desktop** (the download page) or **How to install** (Claude Code's
  quickstart, OpenAI's Codex documentation). MitCad looks for Claude Code's and Codex's
  executables (on `PATH`, `~/.local/bin`, on Windows `%APPDATA%\npm` and
  the user's `Path` as stored, so an install made while MitCad runs counts,
  `~/.cargo/bin`, and the ChatGPT application on macOS; on Windows with
  `.exe`, `.cmd` or `.bat`), for Codex's settings folder, and for the Claude
  Desktop application or the settings folder it makes on its first start.
  The line changes by itself once the assistant is there, at once when you
  come back to the dialog, and on **Check again** below **How to install**.
  A client found outside `PATH` is named by its full path in the command
  under **Advanced**. Since this is a
  guess, Claude Desktop and ChatGPT / Codex also offer a small **Connect
  anyway**, which runs the same setup as **Connect**; Claude Code's setup
  runs its CLI, so it has none. An extension installer that opens shows
  that Claude Desktop is there. One that no application takes, with no
  Claude Desktop application found either (its settings folder alone,
  which stays after an uninstall, does not count), shows that it is not:
  **Install Claude Desktop first** in the error colour, with **Get Claude
  Desktop**. On
  Windows, which offers "Open with" for any file, MitCad asks first
  whether an application is associated with `.mcpb` files, and the line
  clears once one is.
- **Install the extension in Claude Desktop** with **Show in Explorer**
  (**Show in Finder** on macOS) and a small **Open Claude Desktop**: Claude
  Desktop is installed, but no application opens its extension files. This
  is the case on Windows with Claude Desktop from the Microsoft Store,
  which MitCad finds as the installed package `Claude_pzs8sxrjxfjjc` and
  which registers no `.mcpb` association. The line below says where:
  **In Claude Desktop: Settings › Extensions › Advanced settings ›
  Install Extension…, then choose** the saved extension (on Windows
  `%LOCALAPPDATA%\Mitcad\Mitcad\assistant-setup\mitcad-desktop.mcpb`);
  **Show in Explorer** shows that file selected. Do not double-click it:
  Windows has no application for it. **Open Claude Desktop** opens Claude
  Desktop through its `claude://` link; when Claude Desktop is no longer
  there, the line becomes **Install Claude Desktop first**. Approve the
  installation in Claude Desktop and enable the extension, then start a
  new chat. Once Claude Desktop connects, the line goes away.
- **Ready to connect *assistant*** with **Connect**: MitCad sets the
  connection up (below).
- The assistant's own next step: **Approve MitCad in Claude Desktop, then
  start a new chat** with **Open in Claude Desktop again** (which runs the
  setup again), or, with no button, **Start a new Claude Code session to
  use MitCad** or **Restart ChatGPT or start a new Codex session to use
  MitCad**. This means the settings exist, not that the assistant is
  running. Repeating a setup changes nothing and makes no new backup.
- **✓ Connected: *assistant*** with **Disconnect**: shown only after the
  MCP initialization handshake completes, and only for the chosen
  assistant's clients, recognised by the names they give themselves
  (`claude-ai` is Claude Desktop, `claude-code` Claude Code, and a name
  with `codex`, `chatgpt` or `openai` is Codex; any other name counts as
  **Other** and is shown as given). Clients of other assistants are named
  quietly below the line (**Also connected: …**). Names are labels, not
  authentication identities. **Disconnect** ends the chosen assistant's
  connections only (**Disconnect *assistant*** while others stay connected,
  an **Other** client by its own name) and keeps the design and its
  unsaved edits.
- A failure: one short line in the error colour saying what to do, with
  **Try again**. The details (a client's error output, for instance) are
  in the line's tooltip.

**Connect** does, per assistant:

- **Claude Desktop** (Windows and macOS): generates a MitCad extension
  (an MCP Bundle) and opens its installer in Claude Desktop. Approve the
  installation and enable the extension there, then start a new chat. If
  the tools are missing, fully quit and reopen Claude Desktop. When no
  application opens the bundle although Claude Desktop is installed (the
  Microsoft Store install on Windows), the bundle is installed from Claude
  Desktop's settings instead (above). The bundle
  contains a small native launcher that starts the installed MitCad CLI;
  Claude's configuration file is not edited. Whether the extension was
  installed is known only once Claude Desktop connects.
- **Claude Code:** runs the supported registration command with **user**
  scope, so the connection is available across projects. Registration runs
  against a private `CLAUDE_CONFIG_DIR` first. MitCad verifies the entry
  and the unchanged unrelated JSON before atomically applying it to
  `~/.claude.json`, or `.claude.json` inside an existing
  `CLAUDE_CONFIG_DIR`. A failed registration or closing the dialog leaves
  the previous connection intact. Start a new session and check `/mcp`;
  normal client trust approvals still apply. That file keeps Claude
  Code's projects and history and grows large over time: MitCad reads it
  up to 256 MB (other assistants' settings files up to 4 MB, mitcad#760).
- **ChatGPT / Codex:** uses the shared local Codex configuration. It
  respects `CODEX_HOME`; under **Advanced**, **Change…** selects the
  settings file actually used by a desktop client with another location.
  A Codex CLI is used when available, including the desktop client's
  bundled CLI. Otherwise MitCad updates the selected TOML file directly,
  without a separate CLI installation. Restart the desktop client or start
  a new CLI conversation after setup.
- **Other:** the button is **Copy configuration**: the generic
  `mcpServers` JSON with the installed executable path, to add through
  that client's supported setup interface.

Codex registration first runs against a private settings copy. MitCad checks
the result and atomically changes only its own entry in the actual file,
preserving unrelated settings and comments. Existing configuration is backed
up before replacement (the dialog names the backup); a matching entry needs
no write or new backup. A different MitCad entry requires explicit
replacement. Invalid TOML, a concurrent edit, a disabled entry or detected
managed policy stops setup without overwriting the configuration. Ask the
client's administrator to allow the entry when managed requirements apply.

The folder line, **Folder for designs: *path*** with **Change…**, shows the first
folder already allowed for AI assistants, or the projects folder. Pressing
**Connect** (or **Connect anyway**, **Open in Claude Desktop again**, or
**Copy configuration** for another assistant) with the line shown is the
consent: it allows that folder for Open and Save As, including subfolders,
and creates it if needed. **Change…** picks another folder first; it
allows nothing by itself. No access is granted merely by opening the
dialog. Without an allowed folder those operations are refused; saving an
open design to its existing file remains available. The allowed folders
are listed, and can be removed, in **Preferences › AI Assistants**.

**Advanced** holds the manual route and the rarer cases:

- Two read-only fields, **Configuration** and **Command** (**PowerShell
  command** on Windows), each with its
  own **Copy** button. Codex gets TOML, the others the `mcpServers` JSON;
  these formats are deliberately distinct. Paths use the platform's
  separators (escaped as JSON requires; a TOML literal string where it
  can be one). For Claude Code and Codex the command is the registration
  command, for the others the bridge's own launch; the Codex command
  includes the selected `CODEX_HOME`, and a settings file not named
  `config.toml` takes the configuration only (no command). Only the words
  that need it are quoted (PowerShell on Windows, a POSIX shell
  elsewhere). The copies are a fallback, not a prerequisite.
- **Codex settings file** with **Change…** (ChatGPT / Codex only).
- With several MitCad windows open, **Copy Command for This Window**. The
  bridge refuses to choose between several live MitCad windows: keep only
  the intended window open, or start the bridge with this command (it adds
  `--instance` for this window; `--list-instances` lists them). A saved
  setup never records a temporary instance id.

A WSL, remote or cloud client is not automatically on the native application's
host. Browser and mobile conversations cannot launch this stdio bridge;
setup does not expose a public listener or enable a tunnel.

Client references: [local MCP and shared configuration](https://learn.chatgpt.com/docs/extend/mcp),
[Codex configuration locations](https://learn.chatgpt.com/docs/config-file/config-advanced),
[Claude Code MCP and scopes](https://code.claude.com/docs/en/mcp),
[Claude Code configuration locations](https://code.claude.com/docs/en/mcp-quickstart#find-your-configuration-on-disk),
and the [MCP Bundle manifest](https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md).

## Manual setup and headless use

`mitcad-cli mcp` gives an MCP client access to the native parametric
model of MitCad. Use `--connect` to edit the document in a desktop
application that is already open. Use `--workspace DIR` to run without a
UI. The server uses the same JSON commands, queries and OCCT geometry
kernel as the application.

The headless process owns up to 64 open documents. An opaque handle such
as `D1` identifies each document. A handle stays valid until the
document closes or the process exits.

## Start a headless server

Make a directory for the files that the client can access. Then start
the native executable with an absolute workspace path:

```bash
/absolute/path/to/mitcad-cli mcp --workspace /absolute/path/to/designs
```

On Windows, use `mitcad-cli.exe`. The `--workspace` option is necessary.
It must name a directory that exists.

Add `--read-only` to examine existing designs. In read-only mode, the
server permits these operations:

- Open a document
- List the documents
- Query a document
- Close a document

In read-only mode, the server refuses these operations:

- Make a document
- Model commands
- Save
- Export

The executable is the `mitcad-cli` build target. It is usually under
`build/dev/tools/cli/`. Use its actual absolute path for your platform
and build configuration. The executable must have the same native
libraries as the other CLI commands. Refer to
[development.md](development.md) for builds in the approved isolated
environments.

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

On Windows, JSON accepts forward slashes. For example, write `"command":
"C:/Mitcad/mitcad-cli.exe"` and the workspace `"C:/Designs"`. If the
client uses a different configuration layout, use its equivalent fields
for the command and the arguments.

The client starts the process and owns its stdin and stdout pipes. No
HTTP listener, desktop session or plugin package is necessary. stdout
carries only MCP messages. stderr carries diagnostics. If the client
closes stdin, the headless session ends. Unsaved documents then leave
memory, and MitCad does not write them.

## Connect to the open desktop document

The desktop application starts a local MCP listener automatically. An
AI assistant can find the listener after the application is open. A restart
with special arguments is not necessary. To disable the listener, start
MitCad with `--no-mcp`.

On macOS, the installed bundle includes the CLI:

```bash
/Applications/Mitcad.app/Contents/MacOS/mitcad-cli mcp --list-instances
/Applications/Mitcad.app/Contents/MacOS/mitcad-cli mcp --connect
```

`--list-instances` prints `{"instances":[...]}`. The list gives the
`instance`, `pid`, `host` and `port` of each live instance. It never
prints authentication tokens. If several instances are open, `--connect`
refuses to select one. Then pass `--connect --instance ID` with an ID
from the list.

An instance is live only when the token of its record authenticates on
its port. A port that another program reuses is not sufficient. If the
`pid` of a record is no longer a process that runs, the bridge skips the
record and does not connect to its port. Such a record comes from an
application that crashed or that was killed. The bridge and the
application remove such records from the discovery directory of the
user. A process that the user cannot query counts as a process that
runs.

The bridge probes the other records in parallel. Thus instances that
do not answer cost one timeout in all. The timeout is 500 ms for
`--list-instances` and 5 s for `--connect`. `--connect --instance ID`
connects only to that record. It uses the connection that authenticates
the record. `--connect` without `--instance` probes every record to find
the single live one.

`--chat-session MARKER` belongs to the chat panel ([ai-chat.md](ai-chat.md)),
which starts its agent with it: the bridge sends the marker with the token
(`{"mitcadAuth": token, "mitcadChat": marker}`), and the window asks its
user before such a connection's file operations, as the panel's approvals
say. A connection without it is unchanged.

Configure an MCP client to start the stdio bridge:

```json
{
  "mcpServers": {
    "mitcad-desktop": {
      "command": "/Applications/Mitcad.app/Contents/MacOS/mitcad-cli",
      "args": ["mcp", "--connect"]
    }
  }
}
```

The bridge authenticates the local socket before it forwards MCP
messages. The listener binds only to `127.0.0.1`. MitCad selects the
port automatically. On Windows the listener holds its address and port
exclusively (`SO_EXCLUSIVEADDRUSE`): no other local process can bind
them while MitCad runs. The discovery record and the token are private to
the current user. They are under `GenericCacheLocation/Mitcad/mcp`:

| Platform | Discovery directory |
|---|---|
| macOS | `~/Library/Caches/Mitcad/mcp` |
| Windows | `%LOCALAPPDATA%\cache\Mitcad\mcp` |
| Linux | `~/.cache/Mitcad/mcp` |

`MITCAD_MCP_DISCOVERY_DIR` replaces that directory. Use it for isolated
tests or for a separate client environment. The application and the
bridge must use the same absolute directory. The bridge refuses public
or symlinked discovery records. This is a local stdio bridge. It is not
an HTTP endpoint for remote clients.

Other local accounts can see the port, so the listener protects itself
against a connection that does not authenticate:

- The token is compared in constant time: digests of both, compared byte
  by byte without an early exit.
- Only authenticated connections take one of the 8 client slots (agents
  and the bridge's probes). At most 4 connections can wait to
  authenticate; a new one closes the oldest of them. A waiting
  connection is also closed after 5 s and after 4 KiB without its
  authentication line. Thus idle connections cannot keep an agent out.
  When all 8 slots are taken, the answer to a correct token is
  `{"authenticated":false,"error":"too many clients"}`.

### Windows

The installer puts the CLI beside the application. If MitCad is
installed for all users, the CLI is in `C:\Program Files\Mitcad\bin`.
If MitCad is installed for the user only, the CLI is in
`%LOCALAPPDATA%\Programs\Mitcad\bin`. With MitCad open, run:

```powershell
& "C:\Program Files\Mitcad\bin\mitcad-cli.exe" mcp --list-instances
& "C:\Program Files\Mitcad\bin\mitcad-cli.exe" mcp --connect
```

The discovery directory is `%LOCALAPPDATA%\cache\Mitcad\mcp`. The user
owns it. Its access control list permits the user only.
`MITCAD_MCP_DISCOVERY_DIR` replaces it, as on the other platforms. The
configuration of an MCP client is JSON with forward slashes or doubled
backslashes:

```json
{
  "mcpServers": {
    "mitcad-desktop": {
      "command": "C:/Program Files/Mitcad/bin/mitcad-cli.exe",
      "args": ["mcp", "--connect"]
    }
  }
}
```

### Live tools

The live catalogue has these tools:

- `document_list`
- `model_query`
- `model_command`
- `model_export`
- `model_reference` (the command reference in parts, as in headless
  mode; [Look up the reference](#look-up-the-reference))
- `document_save`
- `document_open`
- The application's own tools `ui_state`, `ui_log`, `ui_screenshot`,
  `ui_select`, `ui_command` and `ui_camera`

The rule of thumb: what the user can see and do in the window, an AI assistant
can read and do through these tools. The assistant gets the same undo and
the same refusals.

`document_list` returns the opaque handle of the GUI document, with its
`path`, `name`, `state`, `busy` and `editable`. `editable` means that
model commands run now. The result also lists the folders for designs that AI assistants
can use. Commands change that actual document through its normal
computation and undo workflow. They update its viewport, browser and
history.

New or Open in the GUI invalidates the previous document handle.
`document_open` also invalidates it. List the documents again before
you edit the replacement. These tools act on the document and take the
handle: `model_query`, `model_command`, `model_export`, `document_save`,
`ui_select` and `ui_command`. These tools read or set the window and take no handle:
`ui_state`, `ui_log`, `ui_screenshot` and `ui_camera`. `document_open`
takes no handle, because it replaces the document.

**Folders for designs** are the live-mode counterpart of `--workspace` in
headless mode. `document_save` with a `path` (Save As) and
`document_open` work only on `.mitcad` files in folders that the user
permits. By default, the user permits no folder. The user lists the
folders in Preferences › AI Assistants (Settings › AI Assistants on
macOS; the setting `mcp/folders`; **Add…**
chooses a folder, **Remove** takes it off the list), or Connect in Connect
AI Assistant permits the folder it shows. Or the user permits one folder
for a single run: start the application with `--mcp-folder DIR`. The
option is repeatable. A refusal says why in words the assistant can relay
to the user: "No folder for designs is allowed: add one in Preferences ›
AI Assistants." (Settings › AI Assistants on macOS; the command-line
option is left to this guide) or "<path> is not in a folder for designs
(<folders>)".

A path is permitted when its folder is a permitted folder or below one.
MitCad resolves symbolic links before this check. `..` is refused. A
relative path is relative to the first folder. An existing file is
replaced only with `overwrite: true`, and never through a link.

`model_export` (mitcad#689) writes only into these folders too, by the
same rules, and only a new file: an existing one is refused. It writes
the formats of the headless `model_export` (3MF, STL, STEP, drawings,
…), with `bodies` choosing what goes in, for example without a hidden
helper body. The export does not change the document. It is refused
while a command panel is open, while a dialog is open and when the
document has failed features. It is also refused while a sketch that
has later features is edited (mitcad#750): editing it rolls those
features back until the sketch is finished, so the bodies of that moment
are not the design's. The answer is "Finish the sketch before
exporting: …". A new sketch at the end of the timeline does not stop
the export. `ui_screenshot` with a `path` writes its
image into these folders as well.

Without a `path`, `document_save` writes the document's own file as
before, in any folder. Thus an AI assistant that works in the untitled window
of the user can make the work crash-safe. Call `document_save` with a
path once. Then do plain saves.

**Queries** (`model_query`) are read-only and work at any time. They
work with a sketch or a command panel open, during drags and during
dialogs. They are refused only while the model computes, the view draws
or an import runs. That result is an error with `busy: true`,
`retry_after_ms` and the reason in `error`. Try again after that many
milliseconds. `ui_state`, `ui_log` and `ui_screenshot` are never refused
for a sketch or a command. Over MCP, `print_thickness` answers its
summary only (the thinnest wall, where it is, the regions and the
count of thin triangles or pieces of triangles, `thin_triangles`),
because the corners of every thin one can exceed a client's response
limit. Give
`"triangles": true` for them, with `max_triangles` to cap them
(mitcad#689).

**Model commands** (`model_command`) are refused, with the reason, in
these states:

- While busy, as queries are
- While a command panel is open
- While Create Sketch waits for its plane
- During drags, animations, imports and dialogs
- When the window is read-only

**Command arrays** (mitcad#689): `command` is one command object or an
array of them. The array runs in order, as the native API runs one, and
becomes one undo step labelled `… (agent)`. It stops at the first
refused entry: the entries before it are undone, so nothing of the
array stays, and the error result names the entry (`entry`, `undone`,
`applied: false`). It also stops after the first entry whose added or
edited feature fails; that entry is applied, and `stopped_at` names it.
`undo` and `redo` run alone, not in an array. The result of an array has
`ran`, `results` (each entry's native result) and `result` (the last
one's).

**Command results** (mitcad#689) hold the document's `state` and what
the command changed, in `changes`:

- `features`: the timeline entries of the features the command added,
  whose entries changed, that it recomputed, or that are left failed,
  each with `change` (`added`, `changed`, `recomputed`, `failed`)
- `bodies`: the bodies it added or changed (their shape or their
  attributes), each with `change` (`added`, `changed`) and its `bbox`
  {`min`, `max`} (mitcad#714), so that the side of each piece of a split
  and the place of a moved body show in the reply
- `removed_features`, `removed_bodies`: the uids of what it removed
- `marker`, `feature_count`, `body_count`

A command that recomputes nothing, such as `set_body_visible`, answers
with that one body. The native `result` of `add_feature` and
`edit_feature` has the feature's `warnings` when it has any, such as a
rotation, revolve, draft, taper or chamfer angle of more than one turn:
a JSON number there is radians, and `"angle": 90` warns
`Move1 angle: 90 is read as radians (5156.6°); write "90 deg" for
degrees` (mitcad#714, commands.md "Values"). `"timeline": "full"` gives the whole `timeline` and
`bodies` instead, as before; `"timeline": "none"` only `state`.

**A sketch that is open for edit** takes its own commands through the
sketch session, as if the user did them. These commands are a
`sketch.*` command whose `sketch` is the open sketch, `edit_feature` of
that sketch, and `undo` and `redo`. An array of such commands runs the
same way, as one undo step. `undo` and `redo` undo and redo the
steps of the sketch. If you undo past its first step, the sketch stays
as it does after the user's Undo.

The view, the Sketch Palette and the undo of the sketch follow. The
result is the usual result of the command, with `sketch`. Commands on
other features are refused with the reason until the sketch is finished
(`ui_command` with `sketch.finish`).

**The ui tools** work in every mode. `ui_state` and `ui_log` work at any
time. `ui_screenshot`, `ui_select` and `ui_command` work at any time
except while busy (as above). With a command panel open, the entities of
`ui_select` go to the active input of the panel, as clicks in the view
would.

MCP permits or refuses a command or a query by its class in the API of
the model, not by a list of names. Every command, query and feature type
carries a class. A unit test fails for one without a class. MCP permits
every **model** command, every feature type and every model query. Model
commands change the data of the document: features, sketches,
parameters, attributes, components, joints, views, analyses, undo and
redo.

MCP refuses these classes:

- **File IO**: imports, exports, other designs and links, images and
  drawings by path, the saved state
- **Remote**: the repositories of the library resolver
- **Internal**: the checkpoints of a command panel, the result caches

MCP also refuses a `cache` query that reads the files of the store
(`disk`). Thus a new model command works through MCP without another
edit.

Three commands are classified per call, not by name:
`create_appearance`, `edit_appearance` and `set_render_settings`. They
run through MCP when the call reads no file. A call reads no file when
it has only these items:

- Parameters
- `based_on`
- A null texture
- A texture whose images are embedded with `data` or kept with
  `"embedded": true`
- An environment that is not an image path

The call is refused when a texture image or `environment.image` would be
read from a file. The refusal names the path. Thus an AI assistant can make and
assign a custom or emissive appearance in a live session. The user does
not have to open the design again.

Each change that an AI assistant makes is labelled `… (agent)` in the undo text
and in the version history. Examples are `Add Line (agent)` and `Edit
Sketch2 (agent)`. The second occurs when an assistant changed the open
sketch. The user undoes such a change with Ctrl+Z, like their own
changes.

An MCP client is connected when it has initialized a session. The
assistant indicator, beside the project indicator, shows the connection:
"No assistant" while none is connected and the window accepts them,
"<assistant> connected" while one is connected ("2 assistants connected"
with several), and "Assistants unavailable" when the listener did not
start (or MitCad was started with `--no-mcp`). A client is named by the
assistant its `clientInfo` name matches ("Claude Code connected" for
`claude-code`), else by that name. It is in the status bar of the Docked
layout, in the title row of the Floating layout and in the toolbar of the
native macOS window; its tooltip adds the window's instance id and port.
With no assistant connected, a click on it opens Connect AI Assistant;
otherwise a click opens its menu. The menu says first why assistants are
unavailable, or lists each connected client (the assistant's name, its
`clientInfo` name and version, and when it connected) with Disconnect
<assistant> (Disconnect All with several), then Connect AI Assistant…,
Copy Command for This Window (only while several MitCad windows listen; for this
window, `mitcad-cli mcp --connect --instance <id>`) and Folders
for Designs… (Preferences › AI Assistants). Disconnect ends the connection.

If the stdin of the bridge closes, the connection ends after the bridge
forwards the requested replies. The GUI and its unsaved work stay open.
A new bridge can connect again to the same document. `--workspace` and
`--read-only` belong to headless mode. You cannot combine them with
`--connect` or `--list-instances`.

| Tool | Arguments | Result |
|---|---|---|
| `document_list` | `{}` | `documents`: the document of the window, with `document`, `path`, `name`, `state`, `busy` and `editable`. `folders`: the folders for designs, which AI assistants can save to and open from. An empty list means no folder. |
| `model_query` | `document`, `query` | `document`. Native JSON in `result`. |
| `model_command` | `document`, `command` (an object or an array). Optional `timeline` (`changed` default, `full`, `none`). | `document`, `applied`, native `result`, `failed_features`, `state`, `changes` (with `full` `timeline` and `bodies` instead). For an array also `ran`, `results`, `stopped_at`. In a sketch, also `sketch`. Refer to the command arrays above and [failed features](#transport-errors-and-limits). |
| `model_export` | `document`, `path` (a new file in a folder for designs), `format`. Optional `bodies`, `sketch`, `flat_pattern`, `drawing`, as in headless mode. | `document`, `path`, `exported`, native export `result`. |
| `model_reference` | As in headless mode: none, or one of `section`, `query`, `command`, `feature`, `search`; optional `offset`. | As in headless mode ([Look up the reference](#look-up-the-reference)). The protocol answers it, without the document. |
| `document_save` | `document`. Optional `path`. Optional `overwrite` (default false). | `document`, `path`, `saved`, `status`, `state`. `status` is the text of the status bar, with the recorded version. Refer to the text below the table. |
| `document_open` | `path`. Optional `discard` (default false). | `document` (the new handle), `path`, `name`, `state`, `busy`, `editable`, `opened`, `replaced` (the old handle). Refer to the text below the table. |
| `ui_state` | `{}` | `document`, `name`, `path`, `modified`, `read_only`, `read_only_reason`, `mode`, `busy`, `editable`, `active_component`, `undo`, `redo`, `undo_depth`, `selection`, `hint`, `hint_is_error`, `status_line`, `status_line_is_error`, `agents`, `sketch`, `command`, `app`. Refer to the text below the table. |
| `ui_log` | `since` (a sequence number). `limit` (default 200, at most 2000). | `lines`, `next`, `dropped`. Refer to the text below the table. |
| `ui_screenshot` | `window` (default false). `max_size` (default 1280, 64 to 4096). `format` (`png` default, `jpeg`), `quality` (JPEG, default 90). `width` and `height`, or `aspect`. `path`, `overwrite`. | An MCP image block, after the text: `format`, `mime_type`, `of`, `width`, `height`, `full_width`, `full_height`, `bytes`. With `path`, also `path` and no image. Refer to the text below the table. |
| `ui_select` | `document`. `items` (entities). `reveal` (`none`, `pan` default, `fit`). | `document`, `selected`, `selection`, `view` (`none`, `pan`, `zoom`), `camera`. With a command panel open, also `picked`, `into_command` and `command`. Refer to the text below the table. |
| `ui_command` | `document`. `id` or `edit`. `inputs`. `finish` (`ok` default, `cancel`, `keep_open`). | `document`, `command`, `panel`, `committed`, `open`, `mode`, `sketch`, `undo`, `previews`, `log`, `hint`. For a `view.*` command, also `camera`. Refer to the text below the table. |
| `ui_camera` | `eye`, `target`, `up`, `height`, `fov`, `projection`. `box` or `frame` (`none` default, `shown`, `selection`), optional `aspect` with them. | `camera`, `framed` (`none`, `box`, `shown`, `selection`), `changed`, `aspect` when given. Refer to the text below the table. |

**`document_save`** without `path` saves to the document's own path, as
File > Save does. In a project with versions, it uses the version and
the commit message of the project (`Save …: …`). With `path`, it saves
to that `.mitcad` file in a folder for designs, as Save As does.
Afterwards, the document is that file. An existing file must have
`overwrite: true`. The tool is refused in these cases:

- The document was never saved and the call has no `path`
- The path is outside the folders
- The document is read-only
- A command panel is open
- The save would ask the user something. Then MitCad closes the dialog
  of the question and names it.

**`document_open`** opens a `.mitcad` design from a folder for designs in
the window, as File > Open does. It replaces the open document. The tool
is refused in these cases:

- The path is outside the folders
- The open document has unsaved changes, unless `discard` is true
- A sketch or a command panel is open
- The open would ask the user something

If `discard` is true, MitCad discards the unsaved changes and their
recovery data.

**`ui_state`** reads the window. `read_only` comes with
`read_only_reason`. `mode` is `idle`, `sketch`, `command` or
`pick_plane`. `selection` lists the entities as `ui_select` takes them,
with `geometry`, `reference` and `text`.

`hint` and `hint_is_error` give the status line. While Create Sketch
waits for a plane, `hint` gives the text of its banner. Then
`status_line` and `status_line_is_error` give the status line when it
says something else.

`sketch` is null, or an object with `uid`, `name`, `dof`,
`fully_constrained`, `existing`, `tool` and `palette`. `palette` holds
`status`, `warnings` and `clean_up_offered`. Each warning is `{"text",
"entities"}`. The entities are the `warning_entities` of the `sketch`
query.

`command` is null, or the state of the panel: `id`, `name`, `editing`,
`in_sketch`, `inputs`, `ready` or `message`, `preview_ok`,
`preview_failed`, `preview_warnings`, `busy` and `panel`. `inputs` is by
key, with `label`, `type` and the values. `panel` holds `message`,
`result`, `ok_enabled` and `notes` by key. `app` holds `version`,
`build`, `qt`, `platform`, `ui` and `chrome`.

**`ui_log`** returns the log lines after `since`. `lines` holds `seq`,
`level`, `text` and `time` (ms since the epoch), the newest last. `next`
is the number of the next line. `dropped` counts the lines after `since`
that are no longer kept. The lines are the application's own log lines,
the same as on stderr: what commands did, `Selected: …`, `Preview …`,
refusals and `MCP refused: …`. MitCad keeps the newest 5000 lines, at
most 8 MiB of them. A line's text is cut to keep the formatted line under
4 KiB, with ` … [N more characters cut]` at its end; the debug log's file
gets the same cut line.

**`ui_screenshot`** returns the image that the user sees, with the
sketch overlay. By default, it shows the 3D view. With `window`, it
shows the whole window. In the native macOS window, this is the window
as the window server composes it. MitCad scales the image to fit
`max_size`. Then it scales the image further until the file is under
5 MiB, well in the 8 MiB frame.

The image comes as an MCP `image` content block after the text block,
not as text (mitcad#689); `format` `jpeg` with `quality` makes it
smaller. With `path`, the image is written to that `.png` or `.jpg`
file in a folder for designs instead (an existing file only with
`overwrite: true`), at its full size, and only the path, the size and
the bytes come back.

`width` and `height` (16 to 4096), or `aspect` (width / height, with
`max_size` on the longer side or with one of them), render the 3D view
offscreen at exactly that size, without the widgets over it, so the
picture does not take the shape of the window. A wide window otherwise
gives a wide strip. Frame the model for that shape first with
`ui_camera`'s `aspect`.

**`ui_select`** replaces the selection as clicks would. Then the user
can press Delete or run a command on it. Each item in `items` has
`kind`, `owner`, `name` and `occurrence`. `kind` is one of `face`,
`edge`, `vertex`, `body`, `profile`, `sketch_curve`, `sketch_point`,
`sketch_constraint`, `sketch_dimension`, `plane`, `axis`, `point`,
`component`, `sketch` and `feature`. Entities that the model does not
have are refused.

With `reveal` `pan`, the view moves to the entities when none is in
sight. It keeps the zoom, unless the entities do not fit. With `reveal`
`fit`, the view zooms to the entities. `camera` is as the camera of
`ui_camera`, unless `reveal` is `none`. When the reply comes, the view
is at its place and its animation has ended. Thus the next
`ui_screenshot` shows it.

With a command panel open, the entities go to the active input of the
panel instead. Each entity goes as one click. An entity that is already
in the input goes out again. Then the result holds `picked`,
`into_command` and `command` (the state of the panel).

**`ui_command`** runs a command of the registry
([app/COMMANDS.md](../app/COMMANDS.md)) by `id`, or opens a feature by
`edit` (the uid of the feature). `inputs` is by the input keys of the
panel. Each input takes a value of its type:

- A number or a text for values and texts
- The value of a choice
- True or false for checks
- An array of entities for selections

A command with a panel opens and takes the inputs as if the user typed,
selected and picked them. Then it previews. Then it does OK, Cancel or
stays open, as `finish` says.

If OK is refused, the panel stays open and the call returns an error
with the state of the panel. The OK of a
command that repeats (Place, Connect and Rotate on the Build tab) does
it once, closes the panel and keeps what the command made.

An action runs (Finish Sketch, Undo, Fit). `edit` opens a feature as a
double-click in the timeline does. A sketch opens in sketch mode.
Without `id` and `edit`, `finish` ends the open panel or the open
sketch. `ok` finishes it. `cancel` takes back every change made since
it was opened, and leaves it.

Commands that open a file dialog or another window are refused:
`file.*`, `insert.*`, `tools.*`, `help.*`, 3D Print, DXF import and
export, and others. `document_save` with a path and `document_open` do
Save As and Open. If a command opens any other modal dialog, MitCad
closes the dialog at once and refuses the call with the name of the
dialog.

In the result, `panel` is as the `command` of `ui_state`, after the
preview. `previews` holds the `Preview …` log lines: the validation, the
outcome of the preview and its warnings. `log` holds every line that the
command logged. For a `view.*` command, `camera` is as the camera of
`ui_camera`, with the animation of the view ended.

**`ui_camera`** without arguments reads the camera. `eye`, `target` and
`up` are each `[x, y, z]` in mm. `height` is the mm of the model shown
at the target, from top to bottom. `fov` is the vertical field of view
of the perspective, 1 to 170 degrees. `projection` is `orthographic` or
`perspective`. `box` is `{"min": [x, y, z], "max": [x, y, z]}`.

In the result, `camera` holds `eye`, `target`, `up`, `direction`,
`distance`, `height`, `width`, `projection`, `fov` and `view_size`.
`direction` is a unit vector from the eye to the target. `width` is the
mm at the target. `view_size` is the pixels of the view.

The values given replace the values of the camera at once, without
animation. If no `height` is given, the eye of a perspective camera
keeps its place. Then the height follows from its distance. With a
`height`, the eye moves along its line to show that much.

Then `box` frames that box. `frame: "shown"` frames the shown bodies.
When some bodies are isolated, it frames only the isolated ones, not the
origin, datums or sketches. `frame: "selection"` frames the selected
entities. The frame is along the view direction, with the margin of Fit,
in the part of the view that no panel covers. The next `ui_screenshot`
shows exactly that view. With `aspect` (mitcad#689), `box` or `frame`
fits the box into a picture of that width over height instead, as
`ui_screenshot` with the same `aspect` (or `width` and `height`)
renders it.

`ui_command` covers these UI-only flows:

- Clean Up Stale Projections: `{"id": "sketch.clean_up_stale_projections"}`
  in an open sketch
- Project: `sketch.project` with its `source` input
- Finish Sketch: `sketch.finish`
- Cancel the sketch: `{"finish": "cancel"}`
- Edit a sketch: `{"edit": "F4"}`
- Any feature panel

To examine the model in a screenshot, aim the camera with `ui_camera`.
Then take `ui_screenshot`. `{"box": {"min": [...], "max": [...]}}`
frames a region. `{"eye": ..., "target": ..., "up": ..., "height": ...}`
sets an exact view. `{"frame": "selection"}` frames what `ui_select`
selected. The `camera` of the reply says where the view is.

Fit (`view.fit`) frames the selection when something is selected outside
commands and sketches. It frames the isolated bodies when the design
isolates some. Otherwise it frames everything shown. Home (`view.home`)
takes the eye, target, up and height of a named view with the name
`Home`.

## Tools and results

`tools/list` supplies the full input schemas. The table lists the
headless catalogue. Live connections have their own catalogue (above).
All arguments are JSON objects. Fields below without a default are
required.

| Tool | Arguments | Result |
|---|---|---|
| `document_create` | `{}` | `document`, null `path`, `state`, `timeline`, `bodies`, `applied`, empty `failed_features` |
| `document_open` | `path` | New `document`, canonical `path`, recomputed `state`, `timeline`, `bodies`, `applied`, `failed_features`. For a `.solidcraft` design, refer to the text below the table. |
| `document_list` | `{}` | `documents`: objects with `document`, `path`, `state` |
| `document_close` | `document`, `discard` (default false) | `document`, `closed`. The tool refuses unsaved changes unless `discard` is true. |
| `model_command` | `document`, `command` (a native JSON object or an array of them), `timeline` (`changed` default, `full`, `none`) | `document`, `applied`, native `result`, `failed_features`, `state`, `changes` (with `full` `timeline` and `bodies` instead); for an array also `ran`, `results`, `stopped_at`. Refer to the command arrays and results under "Live tools" and [failed features](#transport-errors-and-limits). |
| `model_query` | `document`, `query` (a native JSON object) | `document`, native JSON in `result` |
| `document_save` | `document`, `path`, `overwrite` (default false) | `document`, canonical `path`, saved `state` |
| `model_export` | `document`, `path`, `format`. Optional `bodies` (an array of names or UIDs). Optional `sketch`. For DXF, `flat_pattern`. For a drawing, `drawing`. Refer to the text below the table. | `document`, canonical `path`, native export `result` |
| `model_reference` | None, or one of `section`, `query`, `command`, `feature`, `search`. Optional `offset`. Refer to [Look up the reference](#look-up-the-reference). | The contents and the permitted names, or one part of the reference, at most 64 kB of text |

`document_open` imports a `.solidcraft` design with its history into a
new document without a `path`. Save that document with `document_save`
and a path.

`model_export` with `flat_pattern` takes a sheet-metal body and exports
its Flat Pattern as DXF. With `format` `pdf` or `svg`, or `dxf` with
`drawing`, the tool writes a drawing: views, sizes, hole diameters, a
hole table and a title block. `drawing` holds the options of
`export_drawing` (commands.md "Drawings"). The title is the file name
of the document, unless `drawing` gives one.

Each tool result includes both `structuredContent` and a text content
block with the same JSON. For example, `model_query` with
`{"document":"D1","query":{"query":"bodies","properties":true}}`
returns an object whose `result` is the native bodies array. The
`timeline` snapshot (of `document_create`, `document_open` and
`model_command` with `"timeline": "full"`) is the native timeline
object, with its `features` array. `state` is the native `document`
query. A `model_command` answers with `changes` by default: what the
command changed, as "Live tools" above describes; arrays of commands
work as there, each refused entry undoing the ones before it.

The export formats are `step`, `iges`, `brep`, `stl`, `obj`, `3mf`,
`usdz` and `dxf`. DXF must have a `sketch` name or UID and excludes
`bodies`. The other formats accept `bodies` and exclude `sketch`. An
export must have a new output path. OBJ can also write the adjacent
`.mtl` file. That file must also be a new path in the workspace. Per-body
SVG and DXF drawings write numbered files such as `part-2.svg`; every
output is checked before the first file is written.

STEP, IGES and BRep keep exact geometry. STL, OBJ, 3MF and USDZ
tessellate it. USDZ includes the appearances. Headless USDZ export
requires embedded texture images, including the maps of a texture set;
it refuses appearances with external image paths before export.
A document save always writes a single self-contained
`.mitcad` file, also in a project with a B-rep store.

The command reference is
[the native API reference](../core/model-api/src/commands.md). Use it
for feature definitions, sketch entities, parameters and topological
names. The tool `model_reference` gives it in parts (below). The whole
reference is also available through `resources/list` and
`resources/read` at `mitcad://reference/commands`, for clients that
read resources; it is about 1 MB, more than an AI assistant's context. The
server permits the commands, feature types and
queries of the model class. These operate on the data of the document.
The server refuses the file IO, remote and internal ones by their class
(refer to "Live tools" above). These are file imports, external
component refresh, raw save and export commands, disk cache access and
remote operations.
Files are accessed through the dedicated document and export tools.

### Look up the reference

`model_reference` (mitcad#714) is in both servers. It is read-only and
needs no document; the protocol answers it from the reference built
into the server. Some clients forward tools only and cannot read a
resource; this tool works for them too. The instructions and the
descriptions of `model_query` and `model_command` point to it.

| Arguments | Result |
|---|---|
| none | `contents`: the `##` and `###` headings of the reference, one a line (`###` indented); `queries`, `commands`, `features`: every query, command and feature type the server permits; `usage` |
| `section` | One section with its subsections: `section` (its heading), `path` (the headings above it), `text`. The name is a heading (`"Values"`, `"Sketch commands"`, in any case), a name a heading lists (`"split_body"` finds *Face features: shell, …, split_body, split_face*) or a part of a heading, in that order; `other_sections` names the other matches |
| `query`, `command` | The reference rows of that query or command, each table with its header and the headings above it, in `text`; `sections`: the headings that list the name; `permitted`. A name without rows gives the lines that name it |
| `feature` | The section of that feature type (the heading that lists it, or the section of its first example definition), as `section` gives it, with `feature`, `example` (its first example definition) and `permitted` |
| `search` | `headings` that contain the text, and `hits`: `section` (the headings above the line) and `excerpt` (240 characters around the text) of each line that contains it, in any case; `total` |

Every answer holds at most 64 kB of text. A longer one has `truncated:
true`, `next_offset` and a `note`: call again with the same argument and
`offset` set to `next_offset` for the rest. A cut section also lists its
`subsections`, which are smaller to ask for. For `search`, `offset` is the
number of hits to skip; a page holds at most 200 hits. An unknown
section, query, command or feature type is an error (`isError`) that
lists the names there are. Give one argument at a time.

```json
{"name": "model_reference", "arguments": {"feature": "construction_plane"}}
```

answers with the section *construction_plane, construction_axis,
construction_point*, its table of definitions and the example
`{"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 10}}`.

## Keep an assistant-built timeline readable

Give features descriptive names and group each contiguous modeling stage as it
is completed. Use `model_command` with the ordinary model command
`{"cmd":"group_features","features":["F1","F2"],"name":"Mounting holes"}`.
The members must be adjacent in timeline order and must not already belong to
another group. Query `timeline` for their actual uids; do not assume the example
uids apply to the open design. Groups let the user fold a long timeline and
hover over a whole stage. Prefer names that explain the design's purpose,
such as **Housing**, **Mounting holes** and **Cable clearance**. Use
`rename_group` with `name` and `new_name` to improve an existing name, or
`ungroup` with `name` before regrouping a stage. These commands preserve the
features and their modeling history.

## Example: make, edit, save and export a part

After initialization, call `document_create` with `{}`. Keep the
returned handle. The `tools/call` parameter objects that follow assume
that the handle is `D1`. They make a 30 × 20 × 10 mm box:

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

Examine the geometry with:

```json
{"name":"model_query","arguments":{"document":"D1","query":{"query":"bodies","properties":true}}}
```

The volume of the body is 6000 mm³. `model_query` with
`{"query":"timeline"}` identifies the feature. In this new document, the
feature is `F1`. Edit its definition to make the height 15 mm. The model
command recomputes the model. Then examine the volume of the result,
9000 mm³:

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

These are three separate calls. Send each call after the response to
the previous one. Open the document again with `document_open` and
`{"path":"box.mitcad"}`. Use its new handle for queries.

Several commands go in one call as an array, one undo step. Positions of
primitives take expressions like their sizes, so a box centred on the
origin follows its parameters without a separate move:

```json
{
  "name": "model_command",
  "arguments": {
    "document": "D1",
    "command": [
      {"cmd": "add_parameter", "name": "L", "expression": "60 mm"},
      {"cmd": "add_parameter", "name": "W", "expression": "40 mm"},
      {"cmd": "add_feature", "def": {
        "type": "box", "plane": {"type": "origin", "plane": "xy"},
        "corner": ["-L/2", "-W/2"], "length": "L", "width": "W",
        "height": 10, "operation": "new_body"}}
    ]
  }
}
```

The result lists what the array changed: the new box and its body.

An existing save destination must have an explicit `"overwrite":true`.
An export destination must be new. When you extend an existing design,
use the actual IDs that queries returned. JSON numbers are millimetres
and radians; write `"90 deg"` for degrees. The instructions and the
description of `command` say so (mitcad#714). The model also accepts
expressions such as `"15 mm"` or `"30 deg"`. A string without a unit,
`"90"`, takes the document's unit, but a JSON number `90` in an angle
is 90 radians.

## Transport, errors and limits

The transport is newline-delimited UTF-8 JSON-RPC 2.0 over stdio, with
one object per line. The supported MCP version is `2025-11-25`. A client
first sends `initialize` with `protocolVersion`, `capabilities` and
`clientInfo`. It reads the negotiated response. Then it sends
`notifications/initialized`.

If the client proposes an older or unsupported version, the server
responds with `2025-11-25`. The client
must support that version to continue.

Discovery and tool calls must have the completed handshake. `ping` is
available. Resource templates are empty. Requests use string or integer
IDs. Batches are not supported.

This obeys the [2025-11-25 lifecycle and version
negotiation](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle).
Clients that use the newer lifecycle can probe `server/discover`. This
server returns method-not-found (`-32601`). Then those clients can use
`initialize` instead, as the [official SDK's protocol-version
guide](https://ts.sdk.modelcontextprotocol.io/v2/protocol-versions)
describes.

Malformed JSON and invalid requests return JSON-RPC errors. Unknown
methods and invalid tool parameters also return protocol errors. A
notification never invokes a tool, never changes a document and has no
response. Model, workspace and file errors return tool results with
`isError:true`, usually `structuredContent:{"error":"..."}`. If a
command validation fails, the document stays unchanged.

Errors give the choices (mitcad#714), in both servers:

- An unknown query says `unknown query 'bbox'` and lists the queries the
  server permits; an unknown command lists the permitted commands. A
  query or command that exists but that MCP refuses by its class keeps
  its own error (`query 'dxf_info' is not available through MCP`).
- A command or query object without its name shows the form:
  `{"command": "list"}` as `command` gives `a command is {"cmd":
  "<name>", ...its fields}, for example {"cmd": "add_feature", "def":
  {...}}; this one has no 'cmd' (its fields: command)`. A definition
  without `type` asks for it the same way.
- The model names only the first missing field of a definition. When a
  refused `model_command` misses a field, the result also has
  `reference`: the section of the feature type that its `def` names
  (`feature`, `section`, `example`, `text`, at most 12 kB, `truncated`),
  or the reference rows of its command (`command`, `text`) when the
  missing field is one of the command's own. The error then says so and
  names the `model_reference` call that gives the rest.
- The model's `unknown variant … expected one of …` and `unknown field …
  expected one of …` errors stay as they are.

The tools' strings have a maximum length in characters (`maxLength` in
their schemas): document handles, command ids (`id`) and feature uids
(`edit`) 256; owners, occurrences, body, sketch and cut sheet names and
`model_reference`'s arguments 1024; an entity's `name` (a topological
name, which nests the features it came through) and paths 4096. A longer
string is an invalid parameter. An error repeats at most 200 characters of
a name or other text that the client sent, then `…` and how much was left
out; a missing name lists at most 20 of the object's fields. The log line
of a refusal keeps its first 2000 characters.

Request and response frames are limited to 8 MiB. The limit includes
the JSON escaping and both the text and the structured tool content. An
oversized tool result returns `isError:true` with
`error_code:"response_too_large"` and `result_omitted:true`. The
connection stays usable.

If the operation already changed the document, the compact result keeps
`applied:true` and the document handle. Do not try that operation again.
`original_is_error` reports if the omitted result itself contained an
error. Large embedded images can exceed this response limit, although
the design file accepts them.

A failed feature does not reject a command. The command is applied and
the feature is skipped. Every applied `model_command` result has
`failed_features`. It lists every feature in error after the command, in
timeline order. It is empty when there is none.

Each entry has `uid`, `name`, `error`, `edited` and `new`. `edited` is
true when the command added the feature or edited it: its definition,
suppression, component or place. `edited` is also true when the feature
is the `uid` of `edit_feature`, `suppress_feature` or `reorder_feature`,
or the `sketch` of a sketch command. `new` is true when the feature did
not fail before this command.

`isError` is true only when the command was refused, or when a feature
that it added or edited fails. That result has `isError:true` **and**
`applied:true`, with the current state of the document and what the
command changed, the failed feature among them (or the whole timeline
and bodies with `"timeline": "full"`). Examine the error of the feature. Then edit or undo. Do not
treat the result as proof that nothing changed.

A failure elsewhere in the timeline is listed in `failed_features` but
leaves `isError` false.
Such a failure is an earlier failure, or one that this command caused in
a later feature (`new:true`), for example through a parameter. The
command did what it was asked.

If you open a document with failed geometry, the result is a handle, its
snapshot and the failed features, not an error. Export refuses documents
with failed features. File IO errors can leave partial output. Examine
or remove that output before you select a destination for a new try.

`add_feature` takes `after` (the uid of a feature) or `index` (a
timeline position, at most the marker). With one of them, it inserts a
feature in the middle of the timeline in one call. That is one undo step
and one recompute of the features after it. The marker stays after the
feature it was after. A refused insert changes nothing. Refer to the
command reference.

In headless mode, paths are relative to the workspace or absolute in it.
Parent traversal (`..`) and symlinks that lead outside the workspace are
refused. Output parent directories must already exist. Project discovery
stops at the workspace root. Version 3 documents can read their existing
B-rep store in that root.

An open does not refresh linked components, does not read per-user
display state and does not configure the global library resolver.
Embedded bodies stay usable. MitCad can use installed fonts when it
evaluates sketch text. MCP refuses sketch feature definitions and designs
opened through MCP with `font_file` metadata. In a live document, existing
file-backed text can keep using its stored outlines; an edit that would
read its font file again is refused. Recompute and history commands are
also checked for missing or stale stored outlines. Use an installed font
or text with stored outlines and no file path instead.
Live opens through MCP also retain the saved embedded bodies of linked
components; they do not refresh linked files or libraries. Ordinary
application opens continue to refresh them.
Workspace path checks are intended for a local
client. They do not isolate the process from concurrent filesystem
changes. They do not apply operating-system resource limits.

Requests run synchronously, one at a time. This version does not report
progress. It does not interrupt a geometry operation that runs.
`notifications/cancelled` cannot cancel that operation.

A client can stop the headless process. Then the unsaved session
documents of that process are lost. That mode has no background
autosave. Document and B-rep reads have size limits. Large geometry
computations can still use much time and memory.

## Validation

- [tools/cli/mcp-test.py](../tools/cli/mcp-test.py) starts the actual
  CLI and covers initialization, discovery (stale records, `--instance`,
  parallel probes), two document handles, box modelling and volume, save
  and open, STEP export, protocol errors, notifications, workspace
  boundaries, read-only mode, version negotiation, EOF cleanup,
  persisted geometry failures, symlink escapes and OBJ companion-file
  conflicts; command arrays (a box placed by parameter expressions, a
  refused entry undoing the ones before it), the compact command results
  and their size, `"timeline": "full"`, and the `print_thickness`
  summary (mitcad#689); `model_reference` (the contents and the
  permitted names, a section, a query row, a feature type's example, a
  search, a cut section continued) and the errors that give the choices:
  an unknown query and command, a refused query, a command without
  `cmd`, definitions that miss fields, an unknown variant (mitcad#714).
- Protocol unit tests are with the Rust protocol crate (`core.mcp`;
  `core/mcp/src/reference_tests.rs` for `model_reference` and its
  errors: the contents, a section, a query row, a feature type, a
  search, a long section cut and continued to its end, the unknown names
  and the form of a command, the reference of a definition or a command
  that misses fields, in both servers), and the native CLI integration
  tests exercise the FFI backend.
- [tools/cli/mcp-results-test.cmake](../tools/cli/mcp-results-test.cmake)
  (`cli.mcp_results`, no Python) checks `isError` and `failed_features`
  with a failed feature in the timeline, `changes` without the whole
  timeline, and `add_feature` with `after` (with `"timeline": "full"`).
- The macOS test `app.ui-macos-mcp`
  ([tools/ui-macos-mcp-test.py](../tools/ui-macos-mcp-test.py)) starts
  the actual GUI and the CLI bridge and checks live discovery, private
  records, authentication, model edits and undo, native history and
  viewport screenshots, reconnect and EOF retention, stale document
  handles, ambiguous instances, malformed and oversized input,
  full-width integer request IDs, and `--no-mcp`.
- [tools/ui-mcp-live.py](../tools/ui-mcp-live.py) then runs the live
  debugging flow on a design whose sketch holds a projection that no
  longer follows: `model_reference` and the queries an unknown query lists
  (mitcad#714), queries, `sketch.add_line` with the sketch open and
  undo, `ui_state` with the warning of the Sketch Palette and its
  entities, `ui_select`, `ui_screenshot`, `ui_command` Clean Up Stale
  Projections and Finish Sketch, `document_save` and `ui_log`.
- [tools/ui-mcp-test.sh](../tools/ui-mcp-test.sh) runs the same flow on
  Linux (Xvfb).
- The ctest `app.ui-windows-mcp`
  ([tools/ui-windows-mcp-test.ps1](../tools/ui-windows-mcp-test.ps1))
  runs the same flow on Windows, in session 0 with Mesa, with its own
  PowerShell client of the bridge.
- [tools/ui-mcp-files-camera-test.sh](../tools/ui-mcp-files-camera-test.sh)
  (Xvfb, a shell client of the bridge with jq) covers Save As, Open and
  the camera on Linux: an untitled design saved into the folder that
  `--mcp-folder` permits and refused elsewhere, `..`, other suffixes and
  an existing file without `overwrite`, `document_open` with and without
  unsaved changes, the new handle and the old one refused, `ui_camera`
  that reads and sets the camera and frames two equal blocks 500 mm
  apart with screenshots that match, the fit of `ui_select`, Fit with
  isolation, and Home with a named view `Home`. For mitcad#689 it runs a
  command array as one undo step labelled `(agent)`, a refused entry that
  undoes the ones before it, the compact results, `model_export` of a
  chosen body into the folder (an existing file and another folder
  refused, the document unchanged), `ui_camera`'s `aspect`, a JPEG
  screenshot of an asked size written into the folder, an image block
  and the `print_thickness` summary.
- The unit tests of the folder rules are in `app.unit`.
- The unit tests of the classification are in `core.model`, and those of
  the live catalogue are in `core.mcp`.

Run these CTests only in the approved isolated environments, as
[development.md](development.md#mcp-validation) describes.
