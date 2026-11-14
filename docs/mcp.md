<!-- SPDX-License-Identifier: MIT -->
# MitCad Model Context Protocol server

## Connect from MitCad

To work with an agent inside MitCad instead, open the chat panel (**View ›
Chat**): it starts your own Claude Code or Codex with this process's live server as
its only tool source and asks you before file operations, or runs the tool
loop itself for an API endpoint or a local model, as a client of the same
live server; see [the chat panel](ai-chat.md).

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
  After an assistant disconnects in this run of MitCad, its line explains
  how to reconnect: **Reconnect MitCad in Claude Code (/mcp › mitcad ›
  Reconnect) or start a new session**, or **Restart ChatGPT or Codex, or
  start a new session**. The assistant indicator opens this dialog on the
  last chosen assistant even with no connection.
- **✓ Connected: *assistant*** with **Disconnect**: shown only after the
  MCP initialization handshake completes, and only for the chosen
  assistant's clients, recognised by the names they give themselves
  (`claude-ai` is Claude Desktop (chat), `local-agent-mode-*` Claude
  Desktop (Code tab), both under Claude Desktop, `claude-code` Claude
  Code, and a name with `codex`, `chatgpt` or `openai` is Codex; any other
  name counts as **Other** and is shown as given). Clients of other
  assistants are named quietly below the line (**Also connected: …**).
  Names are labels, not authentication identities. **Disconnect** ends the
  chosen assistant's connections only (**Disconnect *assistant*** while
  others stay connected, an **Other** client by its own name) and keeps the
  design and its unsaved edits and the assistant's setup. The connection
  ends, and it stays ended, also after a restart: the assistant is blocked,
  unchecked under **Incoming connections** in the assistant indicator's
  menu, as unchecking it there does. Until you choose **Allow** (or
  **Connect**, or check it again in the menu), MitCad refuses that
  assistant's new connections, also those its bridge makes by itself, with
  "Blocked in MitCad: check *assistant* under Incoming connections in
  MitCad's assistant menu to connect it again" (the line reads **Blocked in
  the assistant menu**). Once allowed, a bridge still running
  connects again by itself; otherwise reconnect in the assistant as the
  state line explains. A client that closes its connection gets the same
  reconnect advice.
- **Installed in Claude Desktop, not connected: turn the MitCad extension
  off and on in Claude Desktop, or start a new chat**, with **Open Claude
  Desktop**: MitCad's extension is installed in Claude Desktop (MitCad
  reads Claude Desktop's extensions folder), but Claude Desktop is not
  connected now. Nothing needs to be installed again.
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
- Each MitCad process has one live listener for all its document windows.
  With several processes open, the bridge connects to the process active last
  and names its active design on stderr. The connection keeps that process
  for the assistant session. **Copy Command for This Window** adds
  `--instance` to choose its process explicitly; `--list-instances` lists the
  live processes. A saved setup never records a temporary instance id.
  Within a process, `document_list` lists all designs; pass `document` to
  target one independently of window activation. The assistant indicator is
  shared by the process's windows. Chat connections belong to their own
  document and appear only in that document's indicator.

**The Linux AppImage** (mitcad#1170). The bridge, `mitcad-cli`, is inside
the AppImage, in a folder that the AppImage mounts anew on each start and
that goes away when MitCad closes. So the assistant's settings start the
AppImage file itself: `MitCad.AppImage cli <arguments>` runs the bundled
`mitcad-cli` with the arguments (anything else starts MitCad), and every
assistant registers `<AppImage> cli mcp --connect`, the AppImage's path
quoted where it needs it (the `APPIMAGE` path the AppImage runs from). The
chat panel and the assistant indicator's **Copy Command for This Window**
use the mounted `mitcad-cli`, which serves only while MitCad runs anyway.
After the AppImage file is moved or renamed, connect again. An AppImage
that updates itself keeps its path, so its setup holds. The assistant then
starts the AppImage like any program: without FUSE, give the assistant's
environment `APPIMAGE_EXTRACT_AND_RUN=1`.

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
- Draw a picture of a document (`model_render`, without `path`)
- Close a document

In read-only mode, the server refuses these operations:

- Make a document
- Model commands
- Save
- Export

Add `--memory-limit MiB` to stop model commands (and the recompute of a
design that the server opens) before the server's resident memory reaches
85 % of that many MiB (or grows so fast that it would within 0.1 s), when
that limit is lower than the process's own limits. Without it, the process's limits apply
([memory](#transport-errors-and-limits)).

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
MitCad with `--no-mcp`. An agent that starts MitCad itself (a test or a
debugging run, often with `--mcp-folder DIR`) adds `--no-recovery`: else
an earlier session that did not end normally makes MitCad ask about its
unsaved work at start-up, and edits wait for that dialog (mitcad#1725).

On macOS, the installed bundle includes the CLI:

```bash
/Applications/Mitcad.app/Contents/MacOS/mitcad-cli mcp --list-instances
/Applications/Mitcad.app/Contents/MacOS/mitcad-cli mcp --connect
```

`--list-instances` prints `{"instances":[...]}`. The list gives the
`instance`, `pid`, `host` and `port` of each live instance. It never
prints authentication tokens. With several instances open, `--connect`
selects the process active last. Each instance records its latest activation
as UTC milliseconds beside its private discovery record, along with its
design title (the name its title bar shows, without the modified mark) and
connected client names, without a token. The bridge names
its choice and the other designs on stderr; MCP stdout stays JSON-RPC only.
A connected bridge keeps its chosen process while that connection lasts.
If none of the live instances has an activation time (for example, older
applications), the bridge asks for `--instance` as before. For scripts,
`--connect --instance ID` always selects exactly that ID from the list.

Without `--instance`, the bridge outlives its window's connection
(mitcad#1288): MitCad not started yet, the window closing or restarting
its listener, or **Disconnect**. Until a window is there again it answers
the client itself: `initialize`, the tool and resource lists and
`model_reference` as a window would, and every other tool call with the
error **MitCad is not running: start MitCad (or reopen the design) and
try again** (a call the ending connection cut short: **The connection to
MitCad ended during this call: check the design in MitCad, then try
again**). It looks for a window every 2 s, and at once for a tool call;
the window it finds gets the client's `initialize`,
`notifications/initialized` and resource subscriptions first, under ids of
the bridge's own (`mitcad-bridge-N`), whose answers stay in the bridge.
So an assistant such as Claude Desktop never sees the server stop while
MitCad runs. A window that refuses the client at `initialize` (its
assistant was disconnected there, below) answers the calls with its reason
and is asked again every 10 s. End of input ends the bridge; with
`--instance` it ends with its window's connection, as before.

The bridge loads MitCad's libraries from the installation, so a running
bridge keeps an update from replacing them (mitcad#1454). On Windows the
updater (once MitCad has quit for the update) and the installer therefore
set the event `Local\MitcadBridgeQuit-<pid>` of each bridge that runs from
the installation, and the bridge then ends without an error. It does not
end on its own while it waits for MitCad to start. A bridge that does not
end so (one of a release before that change) the updater names in its
question before the install, and ends it on *End Them and Install*
(mitcad#1584).

It says so on stderr. Reconnect MitCad in the assistant after the update
(Claude Code: `/mcp`).

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
- `model_render` (a picture of the design, as in headless mode;
  [Preview images](#preview-images-model_render), mitcad#1487)
- `document_save`
- `document_open`
- The application's own tools `ui_state`, `ui_log`, `ui_screenshot`,
  `ui_select`, `ui_command` and `ui_camera`
- `document_new` (a new design beside the open ones, mitcad#1266)
- In an application started with `--mcp-folder` only: `ui_click`,
  `ui_type` and `ui_dialog` (mitcad#1735,
  [Clicks, keys and dialogs](#clicks-keys-and-dialogs))

The rule of thumb: what the user can see and do in the window, an AI assistant
can read and do through these tools. The assistant gets the same undo and
the same refusals.

`document_list` returns every open document's opaque handle, with its
`path`, `name`, `state`, `busy`, `editable` and `active` flag. `editable`
means that model commands run now. After connecting or creating a window,
wait for that document to become editable before sending model commands;
the process listener can be ready before native presentation finishes.
A blocked document includes `blocked_reason`. In AppKit builds (not built at
present, [macos-shared-qt.md](macos-shared-qt.md)),
`native_blockers`, when present, distinguishes `command_form`,
`closing_command`, `modal_dialog` and `window_modal`. These fields describe
the existing command and modal guards; they do not bypass them.
`native_modal_state` identifies the actual AppKit modal window, tracked
modal stack depth and top window, document/key windows and their sheets.
Each identity includes its class, title, identifier, visibility and whether
it or its sheet owner is the requested or another registered document window.
The result also lists the folders for
designs that AI assistants can use. Commands change their target's actual
document through its normal computation and undo workflow, updating that
document's viewport, browser and history.

Every live document or UI tool accepts an optional `document` handle.
Omitting it targets the active document at the start of the call. An
explicit handle keeps the target independent of focus. A request waiting
for approval retains its original target. Chat agents are bound to their
own document, including when another window becomes active; they cannot
use a different document's handle, except the handles of the designs they
made with `document_new`. After `document_new` an omitted handle goes to
the design made, for chat agents and other clients alike. `model_reference` is process-wide and
`document_list` always lists all documents. `ui_log` reads the process log.

Closing or replacing a document invalidates only that document's handle.
Opening or creating another window leaves the other handles valid.
`document_open` deliberately replaces the selected document, refusing
unsaved changes unless `discard` is true; UI File › Open follows the user's
window preference instead. List documents again before editing a
replacement. `ui_command` with `id: "window.new"` creates an empty document
window without a file dialog; use `document_list` to obtain its new handle.
The request's result still describes its original target. Existing file
and interactive-dialog commands remain refused through `ui_command`;
`file.new_design` is refused too, because New Design waits for the user
to name the design (in place in the Projects panel, or in a dialog
without it), and its refusal points to `document_new`, which makes the
design without either.
The headless server's explicit-handle contract is unchanged.

The `mitcad://comments` resource follows the last tool's selected document
for each connection, or the active document before a selection. Activating
another window resets that default for external connections. Chat
connections keep their own document's comments. The resource includes its
`document` handle, so clients can identify the design even when two designs
have identical comments.

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
same rules: an existing file is replaced only with `overwrite: true`
(mitcad#1569), and never through a link or outside the folders. It writes
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

**A dialog the user must answer** (mitcad#1725) is never closed for an
agent: the refusal of `model_command`, `ui_command`, `document_save`,
`document_open`, `document_new` and `model_export` names it by its title,
for example `The dialog "Recover Unsaved Work" is open in MitCad …`, and
says to ask the user to answer it. `ui_state` lists it as `dialog`
(`title`, and `recovery_at_start`), with `blocked_reason`;
`document_list` gives the same `blocked_reason`. Queries and the ui
tools that only read work meanwhile. The recovery question is asked at
start-up when an earlier session ended without closing its designs (P8c):
an agent that starts MitCad itself, for a test or a debugging run, starts
it with `--no-recovery`, and the refusal says so for that dialog. Asked by
the user (File › Recover Designs), the refusal only names it.

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
degrees` (mitcad#714, commands.md "Values"); an angle computed from
parameters (`"d1 + 30 deg"`) never warns (mitcad#1092). `"timeline": "full"` gives the whole `timeline` and
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

**Work steps.** Each `sketch.*` command that makes items records them as
one work step of the sketch, and its result names it (`"step": "s3"`).
The user sees one icon for each step in the timeline's place. An array
that runs as one undo step is also one work step. The kind of a step is
the command's name without `sketch.`: for example `rectangle`, `project`
or `add_constraint`. A `sketch.trim` or `sketch.extend` is a step of its
own, also when it only shortens or lengthens a curve. The curve that it
changes stays with the step that made it. The `sketch` query
(`model_query`) returns `steps` in the order they were made:
`{"id": "s3", "kind": "rectangle", "items": ["c1", "p5", "k1", …]}`. A
trim's or an extension's step also has `changed`, the curves that it
changed. An item that no step holds is shown as `{"items": ["p7"]}`.
This happens in designs saved before work steps were recorded, and in
imports. Refer to "The work steps of a sketch" in
[commands.md](../core/model-api/src/commands.md).

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
`claude-code`), else by that name; blocked assistants follow ("No
assistant · 1 blocked", "1 connected · 1 blocked"). It is in the status
bar of the Docked layout and in the title row of the Floating layout; its tooltip adds the window's
instance id and port. With no assistant connected, a click on it opens
Connect AI Assistant; otherwise a click opens its menu. The menu says
first why assistants are unavailable, then has two sections of checkable
rows. Incoming connections: one row per assistant (Claude Desktop (chat),
Claude Desktop (Code tab), Claude Code, Codex, another client by its
`clientInfo` name) set up with Connect AI Assistant, blocked, or connected
in this run, with its state (connected since, not connected, blocked) and
its clients' `clientInfo` names and versions in the tooltip; unchecking a
row disconnects its clients and refuses its new ones, checking it allows
them again, and the choice is kept across restarts. Outgoing connections:
the chat panel's agents with the panel's state; one unchecked is not
offered in the panel and never started. Then Connect AI Assistant…, Copy
Command for This Window (only while several MitCad windows listen; for
this window, `mitcad-cli mcp --connect --instance <id>`) and Folders
Assistants May Use… (Preferences › AI Assistants).

If the stdin of the bridge closes, the connection ends after the bridge
forwards the requested replies. The GUI and its unsaved work stay open.
A new bridge can connect again to the same document. `--workspace`,
`--read-only` and `--memory-limit` belong to headless mode. You cannot combine them with
`--connect` or `--list-instances`.

| Tool | Arguments | Result |
|---|---|---|
| `document_list` | `{}` | `documents`: the document of the window, with `document`, `path`, `name`, `state`, `busy` and `editable`. `folders`: the folders for designs, which AI assistants can save to and open from. An empty list means no folder. |
| `model_query` | `document`, `query` | `document`. Native JSON in `result`. |
| `model_command` | `document`, `command` (an object or an array). Optional `timeline` (`changed` default, `full`, `none`). | `document`, `applied`, native `result`, `failed_features`, `state`, `changes` (with `full` `timeline` and `bodies` instead). For an array also `ran`, `results`, `stopped_at`. In a sketch, also `sketch`. Refer to the command arrays above and [failed features](#transport-errors-and-limits). |
| `model_export` | `document`, `path` (a new file in a folder for designs, or an existing one with `overwrite`), `format`. Optional `bodies`, `sketch`, `flat_pattern`, `drawing`, `refinement`, `overwrite` (default false), and the print plates and print bed fields (`plates`, `plate`, `dialect`, `on_bed`, `center`, `drop`, `auto_orient`, `bed_face`, `spacing`, `occurrence`, `occurrences`, `whole`), as in headless mode; each file a `file_per_plate` writes is in a folder for designs, new unless `overwrite` is true. | `document`, `path`, `exported`, native export `result`. |
| `model_reference` | As in headless mode: none, or one of `section`, `query`, `command`, `feature`, `search`; optional `offset` and `extended`. | As in headless mode ([Look up the reference](#look-up-the-reference)). The protocol answers it, without the document. |
| `model_render` | As in headless mode, without `quality` and its fields (fast only); `document` optional; `path` a new `.png` file in a folder for designs. | As in headless mode ([Preview images](#preview-images-model_render)): the same picture as the headless server's for the same design and arguments. |
| `document_save` | `document`. Optional `path`. Optional `overwrite` (default false). | `document`, `path`, `saved`, `status`, `state`. `status` is the text of the status bar, with the recorded version. Refer to the text below the table. |
| `document_open` | `path`. Optional `discard` (default false). | `document` (the new handle), `path`, `name`, `state`, `busy`, `editable`, `opened`, `replaced` (the old handle). Refer to the text below the table. |
| `document_new` | Optional `name`, `folder`. | `document` (the new design's handle), `path` (null for an untitled design), `name`, `state`, `busy`, `editable`, `created`, `previous` (the calling design's handle, still valid), `recorded` (true when a project recorded it), `note` when the current project's folder could not be used. Refer to the text below the table. |
| `ui_state` | `{}` | `document`, `name`, `path`, `modified`, `read_only`, `read_only_reason`, `mode`, `busy`, `editable`, `active_component`, `undo`, `redo`, `undo_depth`, `selection`, `hint`, `hint_is_error`, `status_line`, `status_line_is_error`, `agents`, `sketch`, `command`, `comments` (`open`: the number of open comments), `app`, `dialog` (the open dialog's `title` and `recovery_at_start`, or null), `blocked_reason` (when not `editable`). Refer to the text below the table. |
| `ui_log` | `since` (a sequence number). `limit` (default 200, at most 2000). | `lines`, `next`, `dropped`. Refer to the text below the table. |
| `ui_screenshot` | `window` (default false). `max_size` (default 1280, 64 to 4096). `max_bytes` (the encoded image's bytes, 4096 to 5 MiB; default 5 MiB inline, none for a file). `format` (`png` default, `jpeg`), `quality` (JPEG, default 90). `width` and `height`, or `aspect`. `path`, `overwrite`. | An MCP image block, after the text: `format`, `mime_type`, `of`, `width`, `height`, `full_width`, `full_height`, `bytes`, `downscaled_for_bytes`, `max_bytes`; of the window, `windows`. With `path`, also `path` and no image. Refer to the text below the table. |
| `ui_select` | `document`. `items` (entities) or `comment` (a comment's id). `reveal` (`none`, `pan` default, `fit`). | `document`, `selected`, `selection`, `view` (`none`, `pan`, `zoom`), `camera`. With a command panel open, also `picked`, `into_command` and `command`. With `comment`, also `comment`. Refer to the text below the table. |
| `ui_command` | `document`. `id` or `edit`. `inputs`. `finish` (`ok` default, `cancel`, `keep_open`). Or `search` alone (words, `""` for all) to list the commands. | `document`, `command`, `panel`, `committed`, `open`, `mode`, `sketch`, `undo`, `previews`, `log`, `hint`. For a `view.*` command, also `camera`. A listing: `document`, `mode`, `search`, `commands` (`id`, `name`, `kind`, `agents`, `why`, `available`), `count`, `total`, and `note` when there was nothing to finish. Refer to the text below the table. |
| `ui_camera` | `eye`, `target`, `up`, `height`, `fov`, `projection`. `box`, `frame` (`none` default, `shown`, `selection`) or `comment` (a comment's id), optional `aspect` with them. | `camera`, `framed` (`none`, `box`, `shown`, `selection`, `comment`), `changed`, `aspect` when given, `comment` with it. Refer to the text below the table. |

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
- The save would write over the file of a design that a newer MitCad
  wrote, and drop its fields that this MitCad did not read (mitcad#1132).
  The error names the fields. Save the design in the application, which
  asks first, or give another `path`.
- The `path` is the file of a design that another window has open
  (mitcad#1323). The error names that window's document, also as
  `open_document`.

**`document_open`** opens a `.mitcad` design from a folder for designs in
the window, as File > Open does. It replaces the open document. The tool
is refused in these cases:

- The path is outside the folders
- The open document has unsaved changes, unless `discard` is true
- A sketch or a command panel is open
- The open would ask the user something
- Another window has the design open (mitcad#1323): the error names that
  window's document, also as `open_document`; work on that document, so
  that one file is never open twice

If `discard` is true, MitCad discards the unsaved changes and their
recovery data.

**`document_new`** (mitcad#1266) makes an empty design, as File › New
Design does, and opens it in a window (a tab) of its own beside the open
designs. It never replaces a design: every open design, with its unsaved
changes, stays open and keeps its handle. The new design becomes the
active one, and the caller's later calls without a `document` handle go
to it; a chat agent moves to it too and can still read the design it
came from by that handle (`previous`). Where it is made:

- With `folder`: in that folder, which must be a folder for designs or
  below one (a relative path is relative to the first folder; `..` is
  refused).
- Without `folder`: in the current project's top folder, the place New
  Design uses, when that folder is a folder for designs. Otherwise, with
  a `name`, in the first folder for designs; without a `name`, the design
  is untitled (save it later with `document_save` and a `path`), and
  `note` says why the project's folder was not used.

`name` is the file's name; `.mitcad` is added when it lacks it. Without
`name` the design is the first free `DesignN.mitcad` of the folder, as
New Design names it. A name that exists in the folder is refused, and the
refusal names the file; a name with a folder in it is refused. In a
project with versions the design is made and recorded as New Design
records it (the project's `new_design`, a version `Add DesignN.mitcad`);
outside projects the empty design is saved to the file. The design is in
the default units for new designs. The tool is refused while a dialog is
open, and when making the design would ask the user something (for
example the author of a project's versions): MitCad closes that dialog
and names it. The result waits until the new design has been computed;
`editable` in the result or in `document_list` says when commands run.

**`ui_state`** reads the window. `read_only` comes with
`read_only_reason`. `mode` is `idle`, `sketch`, `command` or
`pick_plane`. `selection` lists the entities as `ui_select` takes them,
with `geometry`, `reference` and `text`. The `measure` query takes such
an entity unchanged as its selection `a` or `b` (faces, edges, vertices,
bodies, planes, axes and points; `commands.md`, *Selections by names and
as entities*, mitcad#985).

`comments` holds `open`, the number of open comments in the design
([Comments for agents](#comments-for-agents)); it is left out while the
model computes.

`dialog` is the modal dialog open in the application, which only the
user can answer, or null: its `title`, and `recovery_at_start` true for
the recovery question MitCad asks at start-up (and a question over it).
While one is open, `editable` is false and `blocked_reason` names it
(mitcad#1725; [A dialog the user must answer](#live-tools)).

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
`preview_failed`, `preview_warnings`, `busy`, `activate` and `panel`.
`inputs` is by key, with `label`, `type` and the values. `activate` is
there when a feature that changes the picked body (Fillet, Chamfer,
Shell, …) has picks of a component other than the active one
(mitcad#1634): `component`, `name` and `drops` (the picks of other
components that activating it drops). An agent's pick never activates the
component by itself: cancel the command, activate the component
(`activate_component`) and start the feature again.
`panel` holds `message`, `result`, `ok_enabled`, `notes` by key and
`message_action`, the text of the button under the message while it
shows (`Activate Lid`). `app` holds `version`,
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
shows the whole window, with the application's other windows that are
open over it at their places on the screen: popups and open lists,
tooltips, dialogs (mitcad#1734). The picture grows to take in what lies
outside the main window, within the screens; what no window covers is
transparent. `windows` in the result gives each window's `kind`
(`main` first, then `window`, `dialog`, `popup` and `tooltip` in the
order they stack), its `title` when it has one, and its `x`, `y`,
`width` and `height` in the full picture's pixels (`full_width` by
`full_height`). A window manager's frame and title bar are not in the
picture. MitCad scales the image to fit
`max_size`. Then it scales the image further until the encoded image
(the PNG or JPEG file, the result's `bytes`) takes at most `max_bytes`
(mitcad#1188).

`max_bytes` counts the encoded image, not its base64 or the response
around it. The image block's base64 is 4 characters for every 3 bytes,
and the rest of the response (the text block, `structuredContent` and
the JSON-RPC envelope) adds less than 2 KiB. The default, 5 MiB, keeps
the response well within MitCad's own 8 MiB transport limit. That limit
is MitCad's; MitCad cannot know a client's own limit for a tool result
or an image. A client with a lower limit passes a smaller `max_bytes`
(4096 at least), for example 500000, and gets a known smaller reply.
The format asked for is kept: the budget makes the picture smaller,
never another format. `downscaled_for_bytes` tells when the budget made
it smaller than `max_size`, and `max_bytes` gives the budget used.
MitCad encodes at most 8 times and never goes below 64 pixels on the
longer side. When even that is over the budget, the call is refused
with `error_code` `screenshot_too_large`, `max_bytes`, and the smallest
attempt's `smallest_bytes`, `smallest_width` and `smallest_height`; the
connection stays usable. Then allow more bytes, ask for `format` `jpeg`
with a lower `quality`, or write the image to a file with `path`.

The image comes as an MCP `image` content block after the text block,
not as text (mitcad#689); `format` `jpeg` with `quality` makes it
smaller. With `path`, the image is written to that `.png` or `.jpg`
file in a folder for designs instead (an existing file only with
`overwrite: true`), and only the path, the size and the bytes come
back. A file has no byte budget: it takes the picture at its full size
for full quality, unless `max_bytes` is given, which then applies to
the file as it would to an image returned inline.

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

With `comment` instead of `items` (mitcad#1119), `ui_select` selects
that comment's target and reveals it the same way: a feature, a body, a
face, an edge, a vertex or a sketch entity. A comment on a free point or
a parameter, an orphaned comment and a target that is no longer in the
design select nothing; the view then moves to the comment's `place`
(none for a parameter). The result's `comment` holds its `id`, `label`,
`status`, `orphaned`, `place` and `selected` (whether its target was
selected). Give `items` or `comment`, not both.

With a command panel open, the entities go to the active input of the
panel instead. Each entity goes as one click. An entity that is already
in the input goes out again. Then the result holds `picked`,
`into_command` and `command` (the state of the panel).

**`ui_command`** runs a command of the registry
([app/COMMANDS.md](../app/COMMANDS.md); an agent lists it with `search`,
below) by `id`, or opens a feature by
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

`search` (mitcad#1269) lists the registry's commands instead of running
one, since an agent reads no file of the repository: each with its `id`,
`name`, `kind` (`feature`: a panel with inputs; `action`: runs at once;
`tool`: an interactive tool of the view), `agents` (false, with `why`,
for a command that `ui_command` refuses) and `available` (whether it can
run now, in this mode and window). Its words, all of them in any case,
filter by the id, the name and the command search's keywords; `""`
lists every command. It runs nothing, also while the model computes, and
takes no `id`, `edit` or `inputs`; the chat's approvals count it as a
read. A call without `id`, `edit` and `search` while nothing is open to
finish lists them too, with a `note`. An unknown `id` is refused with
the five nearest ids and their names (those that share the most words
with it, then the fewest edits), never with a path to a file.

Commands that open a file dialog or another window are refused:
`file.*`, `insert.*`, `tools.*`, `help.*`, 3D Print, DXF import and
export, and others. `document_save` with a path and `document_open` do
Save As and Open, and `document_new` does New Design. If a command opens any other modal dialog, MitCad
closes the dialog at once and refuses the call with the name of the
dialog. In an application started with `--mcp-folder`, `ui_dialog` shows
such a window for viewing, and the refusal says so
([Clicks, keys and dialogs](#clicks-keys-and-dialogs)).

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
`distance`, `height`, `width`, `projection`, `fov`, `focal_length` and
`view_size`. `direction` is a unit vector from the eye to the target.
`width` is the mm at the target. `focal_length` is `fov` as the focal
length in mm on a 35 mm frame, 12 / tan(fov / 2). `view_size` is the
pixels of the view.

The field of view in force is the perspective camera's or, while
Anaglyph 3D is on, its own (Preferences › Display › Focal length). A
`fov` given sets the one in force until a focal length changes in
Preferences. A new field of view keeps the height at the target: the
eye moves along its line.

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

`comment` (mitcad#1119) frames a comment's target instead of `box` or
`frame`, and selects it as `ui_select` does when no sketch, command
panel or plane pick is open: `framed` is `comment` and the result's
`comment` is as `ui_select`'s. A target without a shape of its own (a
free point, a feature without bodies, an orphaned comment) is centred at
its `place` at the current zoom. A comment on a parameter has no place
and is refused. `ui_camera` with `comment` waits for the model as
queries do (`busy`, `retry_after_ms`).

`ui_command` covers these UI-only flows:

- Clean Up Stale Projections: `{"id": "sketch.clean_up_stale_projections"}`
  in an open sketch
- Project Again: `{"id": "sketch.project_again"}` in an open sketch
  (mitcad#1303)
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

### Clicks, keys and dialogs

`ui_click`, `ui_type` and `ui_dialog` (mitcad#1735) let an agent use the
window as a person does, for screenshots of the real UI (a manual's
pictures, a field filled in, a dialog open). The live catalogue lists them
only when the application was started with `--mcp-folder`, and never to
the chat panel's own connection; the headless server has none of them.
In any other run they are not listed, and the application refuses them.

**`ui_click`** posts the move, the press and the release (twice with
`double`) of `button` (`left`, `right` or `middle`) to the window as Qt
events. Where:

- `input`: a command panel's input by its key, as `ui_state`'s `command`
  lists the inputs (`key=value` for one of a choice's icon buttons). The
  place is the one that `ui_command`'s `log` gives for it ("Panel Extrude
  input distance at 1471,595"): a value field's middle where the text is
  typed.
- `at`: `[x, y]` in the window's logical pixels from its top-left corner,
  as those log lines and a whole-window `ui_screenshot` at full size show
  them. A place outside the window is refused.

A floating panel in a window of its own over the main window takes a
click on it. A field clicked takes the keyboard. A click on an open list
goes to the list; a click outside it closes the list and goes no
further, as a real click does. The result has `at`, `posted` and
`target` (the widget's class, object name and text: for the agent).

**`ui_type`** posts key events to what has the keyboard: an open list,
else the focused field of the window or of one of its panels. `text`
comes first, any Unicode (`ä`, `ö`, `Ø`, `°`), one character after
another; then `keys`, key names separated by spaces (`Enter`, `Tab`,
`Shift+Tab`, `Escape`, `Backspace`, `Delete`, arrows, `Home`, `End`,
`F2`, `Ctrl+A`). The keys go to the field: the application's menu
shortcuts do not run from them (`ui_command` runs those commands). An
unknown key name is refused before anything is typed.

The events of both run right after the call has answered, so that a
dialog that a click opens can run its own event loop while the
connection goes on. Read the effect with `ui_state` (a panel's input
text) or `ui_screenshot`.

**`ui_dialog`** shows a window that `ui_command` refuses because it opens
a dialog or another window (Edit Parameters, Preferences, Grid Settings,
Send to Slicer's panel), for viewing. It takes one of:

- `open`: a command id. A dialog opens right after the result (`opening`);
  a command panel opens at once (`opened`, with the panel's inputs). A
  command that `ui_command` runs, one whose dialog reads or writes files
  (`file.*`, `insert.*`, imports, exports, a library's publishing) and one
  that opens another application (`help.*`) are refused; a file dialog
  that opens anyway is closed.
- `read: true`: the open window's `title`, `place` (`[x, y, width,
  height]` in the window's coordinates), `modal`, `fields` (each `kind`,
  `label`, `value`, `enabled`; tables and trees with their `columns` and
  `rows`, groups' rows after them) and `buttons` (each `text`, `enabled`,
  `default` and `closes`). For a panel, `panel` is as `ui_state`'s
  `command`, and its buttons are OK (or its own label, such as Send) and
  Cancel. While the window is still opening, the result is `busy` with
  `retry_after_ms`.
- `press`: a button whose `closes` is true (a dialog button box's Close or
  Cancel), or `Close` or `Cancel`, which close the window as its title
  bar's close button does (a panel: Cancel). Any other button is refused:
  nothing is changed or written through the window.

While such a window is open, `ui_click` and `ui_type` are refused, so the
window stays for viewing only. `ui_screenshot` with `window: true` takes
its picture.

### Comments for agents

The user pins **comments** to features, faces, edges, bodies, sketch
entities, parameters and points of the design, such as "make this hole
an M6 thread" (mitcad#1119; the commands and the `comments` query are in
[commands.md](../core/model-api/src/commands.md), section *Comments*).
They stay in the design between messages and sessions. The server's
instructions tell an AI assistant to read the open comments first and,
after the change a comment asks for, to put `address_comment` with its
`id` and a short `resolution` in the same `model_command` array as that
change. Both are then one undo step.

- `ui_state`'s `comments.open` counts the open comments.
- The resource `mitcad://comments` (`application/json`, listed by
  `resources/list` on a live connection) is the `comments` query's
  answer for every status: `comments` (each with `id`, `text`, `target`,
  `status`, `orphaned`, `author`, `resolution`, `replies`, `created`,
  `label`, `place`), `open` and `revision`. MitCad reads it again after
  every change of the document (commands, undo, redo, Open, the user's
  own edits) once the model is idle, and before it answers each request.
- `resources/subscribe` with `{"uri": "mitcad://comments"}` asks for
  `notifications/resources/updated` (`{"uri": "mitcad://comments"}`)
  whenever the comments change: one is added, edited, answered,
  addressed, resolved, reopened, deleted or orphaned. Read the resource
  again then. `resources/unsubscribe` stops them. The live server's
  `initialize` result says `"resources": {"subscribe": true}`; the
  headless server has neither the resource nor subscriptions (its
  documents' comments are read with `model_query`).
- `ui_select` and `ui_camera` take `comment` (above): the user sees the
  target, and the next `ui_screenshot` shows it.
- **The author.** MCP sets `author` on `add_comment`, `reply_comment`,
  `address_comment` and `reopen_comment` with a reply to the client's
  name from `initialize` (`clientInfo.name`, for example `claude-code`),
  whatever the call says: a client cannot write as the user. An empty
  name, or `user`, becomes `agent`. The headless server does the same.
- **Resolving is the user's.** `resolve_comment` through MCP is refused
  with "Resolving comments is left to the user (Preferences › AI
  Assistants › Agents may resolve comments)" unless the user turned on
  **Preferences › AI Assistants › Agents may resolve comments** (the
  setting `ai/agentsMayResolveComments`, off by default; MitCad reads it
  for each request). The refusal is the tool's error result with
  `applied: false`; in an array it names the `entry`, and nothing of the
  array runs. The headless server has no such preference and always
  refuses. Reopening and the other comment commands are allowed.

The chat panel sends the open comments with a message when its
**Comments** toggle is on ([AI chat](ai-chat.md)).

## Tools and results

`tools/list` supplies the full input schemas. The table lists the
headless catalogue. Live connections have their own catalogue (above).
All arguments are JSON objects. Fields below without a default are
required.

| Tool | Arguments | Result |
|---|---|---|
| `document_create` | `{}` | `document`, null `path`, `state`, `timeline`, `bodies`, `applied`, empty `failed_features` |
| `document_open` | `path` | New `document`, canonical `path`, recomputed `state`, `timeline`, `bodies`, `applied`, `failed_features`. For a SolidCraft design (`.sc`, `.solidcraft`), refer to the text below the table. |
| `document_list` | `{}` | `documents`: objects with `document`, `path`, `state` |
| `document_close` | `document`, `discard` (default false) | `document`, `closed`. The tool refuses unsaved changes unless `discard` is true. |
| `model_command` | `document`, `command` (a native JSON object or an array of them), `timeline` (`changed` default, `full`, `none`) | `document`, `applied`, native `result`, `failed_features`, `state`, `changes` (with `full` `timeline` and `bodies` instead); for an array also `ran`, `results`, `stopped_at`. Refer to the command arrays and results under "Live tools" and [failed features](#transport-errors-and-limits). |
| `model_query` | `document`, `query` (a native JSON object) | `document`, native JSON in `result` |
| `document_save` | `document`, `path`, `overwrite` (default false) | `document`, canonical `path`, saved `state` |
| `model_export` | `document`, `path`, `format`. Optional `overwrite` (default false: an existing file is refused). Optional `bodies` (an array of names or UIDs). Optional `sketch`. For DXF, `flat_pattern`. For a drawing, `drawing`. For STL, OBJ, 3MF and USDZ, `refinement`. For 3MF, `plates`, `plate`, `dialect`, `occurrences`, `whole`; on the print bed of 3MF, STL or OBJ, `on_bed`, `center`, `drop`, `auto_orient`, `bed_face`, `spacing`; for USDZ, `print_colors` (mitcad#1596); for the bodies' formats, `occurrence`. Refer to the text below the table. | `document`, canonical `path`, native export `result` |
| `model_reference` | None, or one of `section`, `query`, `command`, `feature`, `search`. Optional `offset` and `extended` (the whole section instead of the brief one). Refer to [Look up the reference](#look-up-the-reference). | The contents and the permitted names, or one part of the reference, at most 64 kB of text |
| `model_render` | `document`. Optional `view`, `projection`, `fit`, `bodies`, `hidden`, `size`, `edges`, `background`, `path`, `overwrite`, `max_bytes`, `colors`, `highlight`, `section`, `explode`, `print_colors`, `views`, `quality` (`"raytrace"` with `samples`, `time_limit_s`, `denoise`: [Ray-traced pictures](#ray-traced-pictures)). Refer to [Preview images](#preview-images-model_render). | An MCP image block (PNG) after the text: `camera`, `width`, `height`, `bytes`, `bodies`, `timing_ms`, `warnings`, ...; with `views` an image block per view and `views` |

`document_open` imports a SolidCraft design (`.sc`, `.solidcraft` or
`.solvecraft`) with its history into a new document without a `path`.
Save that document with `document_save` and a path. Other programs use
`.sc` too, so a `.sc` file is imported only when its JSON's top-level
`format` starts with `solidcraft/` (or it is SolidCraft's design
container); any other is refused with "Not a SolidCraft design: …"
(mitcad#1622).

Headless `model_command` also permits `import_print_profile` and
`import_slicer_presets` with `into` omitted or `"design"`. They change only
the design; `user`, `project` and `base` library targets and `project` paths
are refused. For example, copy the slicer's 3MF into the workspace, then:

```json
{"document":"D1","command":{"cmd":"import_print_profile",
 "path":"profiles/source.3mf","dialect":"orcaslicer","into":"design"}}
```

The normal result lists `printers`, `processes` and `materials`; the root
chooses the imported printer and process unless `choose:false`. Assign the
material and use these settings with `model_export`'s `plates` and `dialect`.
For separate presets, copy a slicer data folder into the workspace and use
`import_slicer_presets` with `folder` and `presets:[{"path":...}]`. Every
path is relative to the workspace or absolute inside it. The folder,
selected files and inherited bases are checked for traversal and symlink
escapes before any array entry runs. Missing bases are refused. Inherited
JSON passed to `import_print_profile` must lie under that folder's
`user/<account>/<kind>/` or `system/<vendor>/<kind>/`; for other layouts use
`import_slicer_presets` with an explicit folder. Installed slicer folders
and print libraries are never searched. The command memory guard also covers
preparation. A whole call shares limits of 4,096 file reads, 4,096 directory
entries and 64 MiB of source bytes plus expanded preset snapshots; repeated
sources in arrays count again. Archives are parsed in memory, without extracting entries. These two file
commands remain refused in live mode and under `--read-only`.

`model_export` with `flat_pattern` takes a sheet-metal body and exports
its Flat Pattern as DXF. With `format` `pdf` or `svg`, or `dxf` with
`drawing`, the tool writes a drawing: views, sizes, hole diameters, a
hole table and a title block. `drawing` holds the options of
`export_drawing` (commands.md "Drawings"). The title is the file name
of the document, unless `drawing` gives one.

`model_export` takes the `export` command's `refinement` for STL, OBJ,
3MF and USDZ (mitcad#1569, commands.md "Mesh refinement"): a preset,
`low`, `medium` (the default) or `high`, or the tolerances
`{"deviation": mm, "angle": radians}` with optional `face_triangles` and
`max_edge`. One of `deviation` and `angle` may be left out and is then
`medium`'s (0.03 mm, 15°; mitcad#1732); an object with neither is
refused. The `export` command checks the values; another format with
`refinement` is refused. A small printed part, a 7 mm part or a 15 mm
wheel, shows facets at the default and takes for example
`{"deviation": 0.03, "angle": 0.052}`.

`model_export` writes only new files unless `overwrite` is `true`, as
`document_save` (mitcad#1569): then an existing regular file in the
workspace (in a folder for designs, live) is replaced. The rule applies
to every file the export writes, `path`, an OBJ's `.mtl`, a drawing's
further sheets and the files of `file_per_plate`, and each is checked
before the first is written. A path outside the workspace or the
folders, or a link that leads out of them, is refused with `overwrite`
too.

```json
{"name":"model_export","arguments":{"document":"D1","path":"wheel.stl","format":"stl","refinement":{"deviation":0.03,"angle":0.052},"overwrite":true}}
```

`model_export` writes the design's print plates and a slicer's 3MF
(mitcad#1490) with the fields of the `export` command (commands.md
"Print plates", "Print bed faces", "Export by occurrences", "Spacing
beyond the brims"), passed on as given:

- `plates`: `one_file` writes every plate into `path` (Bambu Studio,
  OrcaSlicer, QIDI Studio, PrusaSlicer 2.9 and later); `file_per_plate`
  writes a file per plate beside `path`, `<name> - <plate name>.3mf`
  (UltiMaker Cura, older PrusaSlicer). `plate` (an id or a name) writes
  that plate alone into `path`. Plates take no `bodies`, `occurrence`,
  `occurrences`, `whole` or `bed_face`.
- `dialect`: the slicer whose print settings go with the bodies,
  `bambu_studio`, `orcaslicer`, `qidi_studio`, `prusaslicer` or `cura`.
- `on_bed`, `center`, `drop`, `auto_orient`, `bed_face` and `spacing`:
  the pieces on the print bed (3MF, STL and OBJ); `occurrences` and
  `whole` (3MF) pick placements, `occurrence` the placements of one
  occurrence.

`plates`, `plate`, `dialect`, `occurrences` and `whole` with another
format than `3mf` are refused with a message naming 3MF. Each file of
`file_per_plate` must be a new path in the workspace (in a folder for
designs, live) as `path` is, or an existing file there with `overwrite`:
every one is checked before the first is written. The result is the export command's, with `plates` (each
plate's `id`, `name`, `path`, `bodies`, `pieces`, `origin`, `bed` and
`warnings`) and `files`.

```json
{"name":"model_export","arguments":{"document":"D1","path":"kit.3mf","format":"3mf","plates":"one_file","dialect":"bambu_studio"}}
```

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
`bodies`; a name several features share gives its only sketch, else it
is refused with their UIDs (feature names are not unique). The other
formats accept `bodies` and exclude `sketch`. An export must have a new
output path. OBJ can also write the adjacent `.mtl` file. That file must
also be a new path in the workspace. Per-body SVG and DXF drawings write
numbered files such as `part-2.svg`; every output is checked before the
first file is written.

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
reference, as agents see it (below), is also available through
`resources/list` and `resources/read` at `mitcad://reference/commands`,
for clients that read resources; it is over 1 MB, more than an AI
assistant's context. The
server permits the commands, feature types and
queries of the model class. These operate on the data of the document.
The server refuses the file IO, remote and internal ones by their class
(refer to "Live tools" above). These are file imports, external
component refresh, raw save and export commands, disk cache access and
remote operations.
Files are accessed through the dedicated document and export tools.

### Selections by geometry

An agent may pick geometry by where it is and what it is instead of by
its topological name (mitcad#1587), in both servers' `model_command`. The
server turns each such selection into names on the document as the
entries before it left it, right before the entry runs; the stored
definition keeps the names, as the application's own commands write
them, so a later change of the design follows the names as it always
does. These forms are MCP's only: the application's commands and panels
never take them.

- A profile in a `profiles` array as `{"sketch": "F1", "point": [u, v]}`:
  the region of that sketch that holds the point (sketch coordinates)
  becomes `{"sketch": "F1", "region": "r{…}"}`. A point in no region, or
  on a boundary that two regions share, is refused. `model_query`'s
  `profiles` gives each region's `area` and `center` to agents
  (`measures`, which MCP sets).
- A fillet's or chamfer's `edges` or `faces` (in the short form and in
  each of its `sets`) as an object: the filter of the `edges` or `faces`
  query on the definition's `body`, e.g. `{"edges": {"between":
  ["cylinder", "sphere"]}}`, `{"edges": {"curve": "circle", "radius":
  2}}`, `{"faces": {"surface": "cylinder"}}`; it becomes the names the
  query finds (a face by its first name). A filter that finds nothing is
  refused.

```json
{"cmd": "add_feature", "def": {"type": "revolve",
 "profiles": [{"sketch": "F1", "point": [30, 20]}], "axis": {"sketch": "F1", "curve": "c6"},
 "extent": {"type": "full"}, "operation": "new_body"}}
{"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
 "sets": [{"edges": {"between": ["cylinder", "sphere"]}, "size": {"type": "constant", "radius": 2}}]}}
```

### Look up the reference

`model_reference` (mitcad#714) is in both servers. It is read-only and
needs no document; the protocol answers it from the reference built
into the server. Some clients forward tools only and cannot read a
resource; this tool works for them too. The instructions and the
descriptions of `model_query` and `model_command` point to it.

| Arguments | Result |
|---|---|
| none | `contents`: the `##` and `###` headings of the reference, one a line (`###` indented); `guides`: each guide topic's `id` and `read_when` (below); `queries`, `commands`, `features`: every query, command and feature type the server permits; `usage` |
| `section` | One section with its subsections: `section` (its heading), `path` (the headings above it), `text`. In brief by default (below); `"extended": true` gives the whole text. The name is a heading (`"Values"`, `"Sketch commands"`, in any case), a name a heading lists (`"split_body"` finds *Face features: shell, …, split_body, split_face*) or a part of a heading, in that order; `other_sections` names the other matches |
| `query`, `command` | The reference rows of that query or command, each table with its header and the headings above it, in `text`; `sections`: the headings that list the name; `permitted`. A name without rows gives the lines that name it |
| `feature` | The section of that feature type (the heading that lists it, or the section of its first example definition), as `section` gives it, with `feature`, `example` (its first example definition) and `permitted`; in brief by default like `section` |
| `search` | `headings` that contain the text, and `hits`: `section` (the headings above the line) and `excerpt` (240 characters around the text) of each line that contains it, in any case; `total` |
| `guide` | A guide on how to model well (below): `guide` (its id) and `text`. `"index"` gives the index |

Every answer holds at most 64 kB of text. A longer one has `truncated:
true`, `next_offset` and a `note`: call again with the same argument and
`offset` set to `next_offset` for the rest. A cut section also lists its
`subsections`, which are smaller to ask for. For `search`, `offset` is the
number of hits to skip; a page holds at most 200 hits. An unknown
section, query, command or feature type is an error (`isError`) that
lists the names there are. Give one argument at a time.

A `section` or `feature` comes in brief (mitcad#1587): the heading, the
first code block, the tables and each paragraph or top-level list item up
to 800 bytes, cut after a sentence, without nested list items, later
code blocks, the subsections' texts and parenthesized issue numbers. The
answer then has `brief: true`, `subsections` (their headings, each one to
ask for with `section`) and a `note` with the whole section's size; the
same call with `"extended": true` gives the whole text (a section that
loses nothing comes whole without them). A refused command whose error
names a missing or an unknown field gets its feature type's or command's
brief reference in `reference`; a definition's `name` says that a name
goes beside `def` or with `rename_feature`, and a parameter named like a
unit, a function or a constant (`Round`) says to choose another name.

The tool and the resource give the reference as agents see it
(mitcad#1509), without what an agent cannot use: the sections whose
command and query tables name only names the server does not permit
(files, version history, projects, edit locks, library commands, …;
asked for by name, such a command or query is "not available through
MCP"), the sections about how the code works inside (marked
`<!-- mcp: internal -->` under their heading), and the issue numbers of
headings (a section is still found by its heading with them). The
reference file itself stays whole for developers
([The reference in parts over MCP](../core/model-api/src/commands.md#the-reference-in-parts-over-mcp)).

```json
{"name": "model_reference", "arguments": {"feature": "construction_plane"}}
```

answers with the section *construction_plane, construction_axis,
construction_point*, its table of definitions and the example
`{"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 10}}`.

### Guides for agents

The reference says what exists; the guides (mitcad#1508) say how to
model well in MitCad: design intent, placement on references, fully
constrained sketches and, in later topics, choosing between overlapping
features and checking each step. They are short Markdown topics, read one
at a time when a task needs them, so that they cost an assistant nothing
until then.

They live in
[`core/mcp/guide/mitcad-modeling/`](../core/mcp/guide/mitcad-modeling/SKILL.md),
in the Agent Skills layout, and are compiled into the server like the
reference:

- `SKILL.md`: its frontmatter `name` and `description` say what the
  guide is and when to read it; its body is the index, one line per
  topic, `- [<id>](references/<id>.md): <when to read it>`;
- `references/<id>.md`: one topic each.

An assistant reaches them three ways, all with the same text:

| Channel | How |
|---|---|
| `model_reference` (every client: a tool is the one channel every client passes on) | `{"guide": "index"}` gives the index, `{"guide": "<id>"}` a topic; the answer without arguments lists the topics as `guides`. Answers keep the 64 kB limit and `offset`; an unknown id is an error that lists the ids |
| Resources | `mitcad://guide/index` and `mitcad://guide/<id>` in `resources/list` and `resources/read`, Markdown |
| A skill | The folder itself is an Agent Skill, for clients that load skills |

One sentence of the server instructions points at the index, and the
description of `model_reference` names `guide`.

To add a topic, write `references/<id>.md`, add its line to the index in
`SKILL.md` and its `include_str!` to `TOPICS` in `core/mcp/src/guide.rs`.
Unit tests in `mitcad-mcp` fail when a file in `references/` is missing
from the index or from `TOPICS`, when an index line has no file, or when
a backticked name in a topic is a command or query that MCP does not
permit, or a name neither MCP permits nor the reference has (a field is
in the reference). The `"cmd"` and `"query"` of JSON examples in a topic
must be permitted ones. These tests read the permitted names from the
model API, as the application gives them to `model_reference`.

### Preview images: model_render

`model_render` (mitcad#1486) draws a picture of a document on the
headless server (and on the live one, [below](#inspection-options-and-several-views)),
so that an agent sees what it builds: a roof slab past
a gable, a cut that undercuts a step. It is read-only, so `--read-only`
allows it (without `path`, which writes a file). It needs no display and
no GPU, and involves no UI state. The bodies are shaded with their
appearances (the body's, and those of single faces) and drawn with
their face boundaries and silhouettes, as the 3D view shows them, over
the view's default light gradient.

| Argument | Default | Meaning |
|---|---|---|
| `document` | | The handle |
| `view` | `"home"` | A name: `home` (the design's named view `Home`, else `iso`), `iso`, `front` (looking along +Y), `back`, `left`, `right` (looking along -X), `top` (looking down, +Y up), `bottom`, or a named view saved in the design. Or `{"azimuth", "elevation"}` in degrees (azimuth about Z from the front, 90 from the right; elevation above the XY plane), or a camera `{"eye", "target", "up"}` (points in mm; optional `height`, `fov`, `projection`). A `camera` the result gave is taken back as it is. |
| `projection` | the view's, else `"perspective"` | `"perspective"` or `"orthographic"` |
| `fit` | `"all"` for a named view or angles; none for a saved view or a camera | `"all"` (the bodies drawn), a list of body, component, occurrence or feature ids (`F2.b0`, `C1`, `O1/O4`, `F7`) or body names, or a box `{"min", "max"}` in mm to zoom on a detail. A camera with `fit` keeps its direction and up and is framed. |
| `bodies` | `"visible"` | `"visible"` (the bodies the design shows: its light bulbs and isolation), `"all"` (hidden ones too), or a list of ids as for `fit`: only these, shown or hidden |
| `hidden` | none | Ids as for `fit`, left out |
| `size` | `[1024, 768]` | Width and height in pixels, 16 to 4096 each |
| `edges` | `true` | Face boundaries and silhouettes |
| `background` | `"theme"` | `"theme"` (the view's light gradient), `"transparent"` (a PNG with alpha), `"white"`, `"black"` or `"#rrggbb"` |
| `path` | none | Also write the PNG, at full size, to this `.png` file in the workspace (refused under `--read-only`) |
| `overwrite` | `false` | Replace an existing file at `path` |
| `max_bytes` | 5 MiB | The returned PNG's largest size in bytes, as for `ui_screenshot` |
| `colors` | none | Per-body colors: `{"F2.b0": "#a52a2a", "C1": "#3060c0"}`, each key an id as for `bodies` ([Inspection options](#inspection-options-and-several-views)) |
| `highlight` | none | References drawn in the selection blue: bodies, faces and edges |
| `section` | none | A clip plane `{"origin", "normal"}` in mm: the side the normal points to is cut away, solids capped |
| `explode` | `0` | Moves components (or bodies) apart from the centre by this factor, 0 to 10 |
| `print_colors` | `false` | Each body in the colours of the filaments it prints with by height, as the filament changes of its plate say, cut at their layer boundaries, in place of its appearances (mitcad#1596; the commands reference's "Filament changes at print heights") |
| `views` | none | Up to 8 views answered as several pictures in one call |
| `overlay` | none | What an analysis finds, drawn over the bodies ([Overlays](#overlays)): `"overhang"`, `"thickness"`, `"draft"`, `"failed"`, or an object with `type` and its settings |

The result is the text block, then an MCP `image` block (`image/png`).
`structuredContent` holds:

- `camera`: the camera used, `{"eye", "target", "up", "projection",
  "height", "fov"}` (`height`: millimetres of the model shown at the
  target, top to bottom; `fov`: the vertical field of view in degrees,
  for a perspective only). Passed back as `view` (with `fit` left out) it
  gives the same picture, so a follow-up call can turn or zoom from it.
- `quality` (`"fast"`), `format` (`"png"`), `mime_type`, `width` and
  `height` of the image returned, `full_width` and `full_height` (the
  size asked for), `bytes`, `max_bytes`, `downscaled_for_bytes`.
- `bodies`: the ids of the bodies drawn; `triangles` and `edges`: how
  many were drawn.
- `timing_ms`: `mesh`, `raster`, `encode` and `total`.
- `warnings`: for example a body that could not be meshed, or nothing to
  frame (an empty design shows the origin).
- `path`, when the PNG was also written to a file.
- `overlay`, with an overlay: its summary ([Overlays](#overlays)), and
  `timing_ms.overlay`.

The byte budget is `ui_screenshot`'s: `max_bytes` counts the PNG, not
its base64. When the PNG is larger, the picture is drawn again smaller
with the same camera (at most 8 times, never below 64 pixels on the
longer side), and `downscaled_for_bytes` says so. When even that does
not fit, the call is refused with `error_code` `render_too_large`, with
`max_bytes` and the smallest attempt's `smallest_bytes`,
`smallest_width` and `smallest_height`. The file written to `path` is
always the size asked for; a file that cannot be written is refused with
`error_code` `render_write_failed`. Unknown ids in `bodies`, `hidden`
or `fit`, and an unknown view name, are refused with the choices.

The same document and arguments give the same bytes on one platform
(the tests compare them), so agents and tests can compare pictures by
their bytes. Another platform's floating point may differ in the last
bit, and a pixel with it.

**How it draws.** A software rasterizer in Rust (`core/preview`, with
the scene from the model, `core/model/src/preview.rs`), over the bodies'
display meshes (`Kernel::face_meshes`, each body meshed within a
thousandth of its box's diagonal, as the 3D view meshes it, 0.01 to
0.5 mm). It was chosen over an offscreen OCCT view (EGL or OSMesa) for
three reasons: it needs no GPU, display, EGL or Mesa library in a
container or CI; its output is exact integer and `f64` arithmetic in a
fixed order, so it is deterministic, which a GPU driver's rasterization
is not; and it is our own logic, which goes in Rust first. The camera
(`core/preview/src/camera.rs`) is a pure function of the view, the
framed box and the picture's shape, shared with every later quality, so
that a fast preview and a ray-traced picture of the same arguments frame
alike. Triangles and edges are clipped to the picture (with a small
margin) and to the camera's near plane before they are drawn, so a face
or edge that runs far past the picture, or close to the eye, is drawn
along its true line and without gaps. Edges are drawn against the depth
of the faces with a small tolerance; transparent bodies are drawn
faintly after the others. As in the 3D view (mitcad#1773), a seam (where
a round face closes on itself) and the line between pieces of one face
(faces whose names differ only in their `#k` piece numbers) are not drawn
where the surface runs on smoothly across them (normals within 0.01 rad);
a seam at a crease (a closed profile with a sharp corner, extruded as one
face) is. The model keeps these edges: `edges`, selections and features
that name them are unchanged.
Pictures larger than about two million pixels are drawn without the 2x2
supersampling that smooths the edges of smaller ones.

#### Inspection options and several views

`colors`, `highlight`, `section`, `explode` and `views` (mitcad#1487)
help an agent check a specific place: a multi-color print, a reference,
a fit or a cavity, an assembly or a kit. They are the fast mode's; a
ray-traced picture refuses them. A reference or id that names nothing
drawn, in `colors` or `highlight`, is a warning in `warnings`
(`colors: F9.b0 names no body drawn`), not a refusal; a value of the
wrong form (a color that is not `#rrggbb`, a zero normal) is refused.

- **`colors`**: an object of ids, as `bodies` takes them (a body,
  feature, component or occurrence id, or a body's name), and colors
  `"#rrggbb"`. Every face of the bodies named takes the color, their
  appearances and face appearances replaced for this picture. A body
  several keys name takes the color of the key naming the fewest bodies
  (the most specific), then of the key later in sorted order.
- **`highlight`**: a list of references, drawn in the selection blue
  (`#1f7aff`, the 3D view's selection color), to confirm that a reference
  points where the agent thinks:
  - an id or a body's name, as for `bodies`: every face of those bodies;
  - a face's topological name, as `model_query` `faces` lists it
    (`F2:side(c1[c4,c2])`): that face of every body drawn that has it
    (a face matches when one of its names matches);
  - an edge's name (`E{F2:side(c1[c4,c2])|F2:end(...)}`): a line two
    pixels wide along the edge, over everything, also where a face hides
    it;
  - `<body>/<face or edge name>` (`"Body1/F2:end(r{c5})"`), or
    `{"body", "face"}`, `{"body", "edge"}` (optional `"occurrence"`), or
    an entity as `ui_state` lists the selection
    (`{"kind": "face", "owner": "F2.b0", "name": ..., "occurrence": ...}`):
    only in that body (and occurrence).
- **`section`**: `{"origin": [x, y, z], "normal": [x, y, z]}` in mm.
  The side the normal points to is cut away, as a Section Analysis cuts.
  Edges and silhouettes are cut there too, and the cut's outline is drawn
  with the edges. A solid body (not a surface body, not a transparent
  one) is capped: its cut faces are drawn in the body's color (its
  `colors` color, if given), lit as facing the eye, with thin lines at
  45 degrees in a darker shade, so that the inside of a hollow body, a
  fit or a hole shows against the walls around it. An `overlay` is cut
  at the plane too. The camera frames the bodies whole, as without a
  section.
- **`explode`**: a factor, 0 to 10. The bodies are grouped by their
  top-level occurrence (a root body is a group of its own); in a design
  with one group only, each placed body is a group. Each group moves
  away from the centre of the drawn bodies' bounding box by `explode`
  times the distance of its own box's centre from it. The camera frames
  the moved bodies. `0` is the picture without `explode`.
- **`views`**: a list of up to 8 entries, each a view as `view` takes it
  (a name, angles or a camera) or an object with the picture's own
  `view`, `projection`, `fit`, `size` and `section`; the call's other
  arguments apply to every picture. The result has one image block per
  entry after the text, in order, and `views` in `structuredContent`:
  each picture's `camera`, `width`, `height`, `full_width`,
  `full_height`, `bytes`, `downscaled_for_bytes`, `bodies`, `triangles`,
  `edges`, `timing_ms` and `warnings`, as a single call gives them. The
  call's `max_bytes` is shared: each picture gets `max_bytes` divided by
  the number of entries (`share_bytes`), and `bytes` is their sum. Each
  picture is the one a single call with its entry's fields gives, byte
  for byte, when it fits that share. `views` returns pictures inline
  only: a call with `path` is refused.

**The live server.** The application's MCP connection answers
`model_render` too (mitcad#1487), with the same renderer and arguments
(without `quality` and its fields): the design at the timeline marker,
as the headless server draws the same design file. The picture does not
depend on the window's view, its camera, visual style or selection;
`ui_screenshot` is the tool for what the user sees. Like `model_query`,
it reads while a sketch or command is open and is refused only while the
model computes. `path` is a new `.png` file in a folder the user allows
agents to use (`document_list`'s `folders`), and the chat panel asks
before a chat agent writes one.

#### Overlays

`overlay` (mitcad#1488) draws what an analysis finds over the bodies in
one flat color, so that an agent sees where a print needs support or a
wall is too thin without slicing the design. The overhangs and the walls
come from the same Rust functions as Print Analysis
(`core/model/src/print_analysis.rs`, [commands.md](../core/model-api/src/commands.md),
Print analysis), on the mesh Print Analysis meshes a body with (the 3MF
export's default tolerance), and the triangles drawn are the ones its
numbers count: the picture and the numbers agree. A body is analysed
where Print Analysis prints it from (its first shown placement) for the
summary, and drawn where each of its placements puts it. The triangles
lie a little off the surface (0.05 mm, the walls' 0.08 mm, plus the
picture's mesh deviation of the body), unlit, as the 3D view draws Print
Analysis; edges are drawn over them.

| `overlay` | Settings | Color | Draws |
|---|---|---|---|
| `"overhang"` | `angle`: degrees from the vertical (default 45); `up`: the print direction's up `[x, y, z]` | `#7e3fd0` | The regions that need support, as `print_overhangs` finds them: printed with `up` upward when given, else on the body's print bed face ([Print bed faces](../core/model-api/src/commands.md#print-bed-faces)), else with +Z up; a face on the bed is none |
| `"thickness"` | `minimum`: the thinnest wall in mm (default 0.8) | `#c2188b` | Walls thinner than the minimum, as `print_thickness` finds them (the thin pieces of a triangle thin in part); the result is the one that query keeps, so asking both computes it once |
| `"draft"` | `pull`: the mold's pull direction (default `[0, 0, 1]`); `angle`: the least draft in degrees (default 1, less than 90) | `#d98a00` | Faces whose normal is within `angle` of square to the pull, either way (a two-part mold): they need more draft |
| `"failed"` | none | `#d32f2f` | The bodies the failed features before the marker read (else those of the features they name), whole: a failed feature changes nothing, so they show what it did not do |

Any of them takes `color` (`"#rrggbb"`). Unknown types and fields of
another type are refused. `overlay` is drawn in the fast mode only; with
`quality: "raytrace"` it is refused.

The result's `overlay` gives `type`, `color` and per type:

- `overhang`: `angle`, `up` (as given, or null), `area`,
  `overhang_area` and `share` of the bodies drawn, and `bodies`: per body
  `body`, `name`, `down` (towards the bed), `bed_face` (whether its print
  bed face orients it), `area`, `overhang_area`, `share` and `triangles`
  (their count): `print_overhangs`' numbers for that body and direction.
- `thickness`: `minimum`, `thinnest` and `at`, `regions`,
  `thin_triangles`, `thin_area` (mm²), and `bodies`: per body `thinnest`,
  `at`, `to`, `regions`, `thin_triangles` and `thin_area`, as
  `print_thickness` answers (with `triangles: false`).
- `draft`: `pull` (unit), `angle`, `area`, `under_area`, `share`, and
  `bodies`: per body `area`, `under_area`, `share` and `triangles`.
- `failed`: `features` (each failed feature's `feature`, `name`, `error`
  and `bodies`), `bodies` (those marked in the picture), `not_drawn`
  (marked bodies the picture does not draw, e.g. hidden ones; draw them
  with `bodies: "all"`) and `errors`.

A body that cannot be meshed for the analysis is left out with a
warning and an entry `{"body", "name", "error"}` in `bodies` (in
`errors` for `failed`).

```json
{"document": "D1", "view": "bottom", "overlay": {"type": "overhang", "angle": 50}}
→ {"overlay": {"type": "overhang", "color": "#7e3fd0", "angle": 50, "up": null,
     "overhang_area": 412.6, "area": 5180.2, "share": 0.0797,
     "bodies": [{"body": "F2.b0", "name": "Body1", "down": [0, 0, -1], "bed_face": false,
                 "area": 5180.2, "overhang_area": 412.6, "share": 0.0797, "triangles": 38}]}, …}
```

Tests: `render_overlay_tests` (`core/model-tests/print`: a
block with one 60-degree overhang whose pixels take the overlay's color
from below and not from the top, front or right, with `print_overhangs`'
area; the print bed face and `up`; thin walls from `print_thickness`'
result; faces under the draft; a failed fillet marking the block it read
and not the one beside it), `core/preview/src/overlay_tests.rs` (the
argument; flat over its face, hidden behind others),
`core/mcp/src/tests.rs` (the schema) and `tools/cli/mcp-test.py`
(`test_model_render_overlays`: the overhang and the walls of a box on
the real kernel, with `print_overhangs`' area).

#### Ray-traced pictures

With `quality: "raytrace"` (mitcad#1489), `model_render` ray traces the
picture with the Cycles render worker `mitcad-render`, as
`mitcad-cli render` does ([Final render](rendering.md#final-render)): the same
scene (the bodies as placed, with their appearances, their faces'
appearances and texture images) and the same job (the design's render
settings: environment, ground, lights, film). The camera, `fit`,
`bodies`, `hidden` and `size` are the fast mode's, so that a fast preview
and its ray-traced picture of the same arguments frame alike: the result's
`camera` is the one the fast mode gives for them.

| Argument | Default | Meaning |
|---|---|---|
| `quality` | `"fast"` | `"raytrace"` for the render worker |
| `samples` | `64` | Samples per pixel, 1 to 4096 |
| `time_limit_s` | `30` | The most seconds the render may take, 1 to 600; it stops with the samples it has by then |
| `denoise` | `true` | Denoise the picture |

`colors`, `highlight`, `section`, `explode` and `views` are the fast
mode's: a ray-traced call with any of them is refused.
`samples`, `time_limit_s`, `denoise`, `size` and `background` replace the
design's render settings for this picture only; the document does not
change. `background` `"theme"` is the render settings' background (their
colour, or the view's light gradient), `"transparent"` gives a PNG with
alpha. `edges` does not apply: a ray-traced picture shows no edge lines.
The render device is chosen as for `mitcad-cli render --device auto`.

The worker must be next to `mitcad-cli`, as `mitcad-cli render` finds
it, or `MITCAD_RENDER_WORKER` names it. The call is never answered with a
blank image:

| `error_code` | When |
|---|---|
| `render_worker_missing` | No render worker is installed (or `MITCAD_RENDER_WORKER` names no program) |
| `render_worker_failed` | The worker failed to start, does not match this MitCad, crashed, or did not finish within `time_limit_s` and a minute for its start |
| `render_too_large` | The picture does not fit `max_bytes` even scaled down (as in the fast mode) |
| `render_write_failed` | `path` could not be written |

Each refusal has `suggestion`: render with `quality: "fast"` instead.

The result is the fast mode's (an image block after the text) with
`quality: "raytrace"`, and also `samples` (the samples rendered), `denoise`,
`device` (the device it rendered on) and `timing_ms.render` (the worker's
render time). When the PNG exceeds `max_bytes`, the rendered picture is
scaled down smoothly until it fits (`downscaled_for_bytes`); the file
written to `path` is always the size asked for. Ray tracing is not
deterministic to the byte across devices and samples; compare pictures of
the fast mode instead. `--read-only` allows it, without `path`.

## Server instructions

The `instructions` of the `initialize` result are one fixed text per
server, live and headless (`core/mcp/src/lib.rs`), each at most 2,000
characters (mitcad#1506). Claude Code keeps only the first 2,048
characters of a server's instructions (4,096 since its v2.1.296) and
drops the rest without telling the user; the chat panel runs the user's
own `claude`, which may be older. The instructions hold only what applies
across tools, the most important first:

1. what the server is, and the document handles;
2. comments first (read the open ones, `address_comment` in the same
   array);
3. `model_reference` for commands, queries and fields, and
   `model_reference {"guide": "index"}` for how to model well (the guides
   above);
4. units: millimetres and radians, `"90 deg"`, the design's length unit;
5. place a part or a sketch on a reference, not on a fixed plane;
6. fully constrained and dimensioned sketches;
7. a `model_command` array is one undo step and stops at the first
   refusal.

A rule about one tool is in that tool's description, which an assistant
reads when it calls the tool; each description is at most 2,048
characters:

| Rule | In the description of |
|---|---|
| How a sketch's ids are given out, and that a point input `[x, y]` is always a new point | `model_command` (both servers); also the reference's *Sketch commands* |
| How the application's command ids are found (`search`) | `ui_command` |
| Busy and retry | `model_query` |
| The sketch session, the command panel and read-only documents | `model_command` |
| The `(agent)` undo label and Ctrl+Z | `model_command` |
| A failed feature stays in the timeline | `model_command` |
| What each `ui_*` tool shows or does | the `ui_*` tools |
| The folders of save, open, new and export | `document_save`, `document_open`, `document_new`, `model_export`, `ui_screenshot`, `model_render`, `document_list` |
| Handles that become invalid, no imports or closing, what Disconnect leaves | `document_list` |

A unit test in `mitcad-mcp` fails when either server's instructions, as
the client decodes them, are longer than 2,000 characters, when a tool
description is longer than 2,048, when the kept rules are out of order,
or when a moved rule's key phrase is missing from its tool's
description.

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

## Place parts on the model

Both servers' instructions ask an assistant to place a part or a sketch
on a reference to existing geometry where there is one: a face, a
construction plane, an axis or a point (a primitive's, pulley's or
gear's `at`, a sketch's `plane`). The part then follows when that
geometry moves. A fixed plane (`{"origin": [x, y, z], "normal": [x, y,
z]}`) is valid, but it stays where it is written and does not follow the
model (mitcad#1137). The application opens such a feature for editing:
the placement row shows the plane as **Plane at Z 36 mm (fixed)**, kept
unchanged until the user picks a face or a plane in its place
([app/COMMANDS.md](../app/COMMANDS.md)).

## Fully constrained sketches

Unless the user asks otherwise, both servers' instructions ask an
assistant to make every sketch fully constrained and dimensioned:
dimensions for the sizes that matter (named parameters where a value is
used again) and constraints for the rest, tied to the origin or to
existing geometry, until the sketch has no degrees of freedom left (the
`sketch` query's `dof` is 0). In a live session `ui_state` also shows
the Sketch Palette's warnings.

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

- An unknown query says `unknown query bbox` and lists the queries the
  server permits; an unknown command lists the permitted commands. A
  query or command that exists but that MCP refuses by its class keeps
  its own error (`query dxf_info is not available through MCP`).
- A command or query object without its name shows the form:
  `{"command": "list"}` as `command` gives `a command is {"cmd":
  "<name>", ...its fields}, for example {"cmd": "add_feature", "def":
  {...}}; this one has no cmd (its fields: command)`. A definition
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

This 8 MiB limit is MitCad's transport, not a promise about a client:
a client can reject smaller results of its own accord. For
`ui_screenshot`, `max_bytes` gives such a client a reply of a known size.

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

Such a result's `error` says this itself (mitcad#1481): `F57 (Fillet3)
was added but failed: fillet failed. It stays in the timeline; edit it, or
remove it with undo or delete_feature` (`was changed` for an edit). In an
array the entry is named first (`commands[1]: …`), and when the array
stopped there, `the entries after it did not run` follows.

A failed fillet or chamfer that the command added or edited (mitcad#1587)
is tried in previews with each of its edges (or faces) alone, up to 100
of them: its entry gets `alone`, `{"fail": [names], "succeed": n}`, and
its `error` ends with `tried one at a time, 1 of its 40 edges or faces
fail alone (E{…}) and the other 39 succeed`, or says that each succeeds
alone and they fail together. Only MCP results say this; the
application's own error text is unchanged.

A failure elsewhere in the timeline is listed in `failed_features` but
leaves `isError` false.
Such a failure is an earlier failure, or one that this command caused in
a later feature (`new:true`), for example through a parameter. The
command did what it was asked.

A command that would run the process out of memory is stopped (mitcad#1480),
in both servers: one boolean of many large, nearly flat spheres can take
gigabytes, and the system would otherwise end the process without a
reply. While a model command computes, MitCad measures its memory every
0.02 s against the tightest limit it runs under (the memory the system has
left, a cgroup's, `ulimit`'s or a job's limit; headless also
`--memory-limit`, below). At 85 % of it, when the memory grows so fast
that it would reach 85 % within 0.1 s (a join of a plate and 38 flattened
spheres grew by about 6 GiB a second, mitcad#1493), or when an allocation
inside the geometry kernel fails, the computation stops. The command is
refused with `isError:true` and an error that starts with `the operation
ran out of memory` and names the memory (`at 5.9 GiB of 6.9 GiB (memory
the system has left)`; `(growing too fast to go on)` or `(an allocation
in the geometry kernel failed)` before `at` when that was the reason);
the document stays as it was before the command (an array's
earlier entries are undone), and the server answers the next call. Do
the work in smaller steps: for example, join the bodies in smaller
groups. Imports keep their own memory guard; undo and redo are not
stopped.

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
  `cmd`, definitions that miss fields, an unknown variant (mitcad#714);
  the print plates of two boxes in one Bambu-family 3MF and a file per
  plate, refused while one plate's file exists and for STL (mitcad#1490).
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
- [tools/cli/mcp-memory-test.cmake](../tools/cli/mcp-memory-test.cmake)
  (`cli.mcp_memory`, no Python) runs the server with `--memory-limit 1`:
  a command and an array are refused with the memory error, the document
  stays as it was, and the server answers the next calls (mitcad#1480).
  [tools/cli/mcp-memory-boolean-test.cmake](../tools/cli/mcp-memory-boolean-test.cmake)
  (`cli.mcp_memory_boolean`) runs the boolean that grew fastest
  (mitcad#1493: a plate and 38 spheres flattened by a non-uniform scale,
  joined in one Combine) with `--memory-limit 300`: the join is refused
  within a second, the document stays as it was. The same script with
  `-DBUILT=ON` (`cli.mcp_boolean_flattened_spheres`, mitcad#1529) runs
  the join with `--memory-limit 4096`: it builds one valid body (about
  2 s and a peak of 1.8 GB on 32 threads).
  The guard itself, with readings of its own, is tested in
  `core/model-base/src/monitor/memory_guard.rs` (`core.model_base`).
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
- [tools/mcp-eval](../tools/mcp-eval/README.md) (mitcad#1507) measures
  what agents build through the headless server: six modelling tasks, a
  runner that gives each to Claude Code or Codex with MitCad's tools only,
  and metrics read from the saved designs (sketches not fully
  constrained, fixed planes, repeated literal lengths, user parameters,
  default names, groups, failed features, the design after each user
  parameter's +10 % change) and from the transcripts (tool calls, errors,
  tokens). Its extractor is tested by `cli.mcp-eval` on a small saved
  design with known values.
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
- [tools/ui-mcp-input-test.sh](../tools/ui-mcp-input-test.sh) (Xvfb, the
  same shell client, an application started with `--mcp-folder`) covers
  the input tools (mitcad#1735): the tools listed, `ui_click` on Extrude's
  distance field by its input key and at its place, `ui_type` with
  `Ctrl+A` and the Unicode text `pyörä_d` (a user parameter's name) that
  the panel's input then holds, a place outside the window, an input not
  shown and an unknown key refused; `ui_dialog` refusing a command
  without a dialog and one with files, Edit Parameters opened, read
  (title, buttons, the parameter in its table) and shown by a
  whole-window screenshot, `ui_click` and `ui_type` refused while it is
  open, OK refused and Close closing it; and Send to Slicer's panel
  opened for viewing, Send refused and Cancel closing it. The key names
  and the dialogs `ui_dialog` opens are unit-tested in `app.unit`
  (`app/tests/ui_keys.cpp`, `app/tests/ui_command_list.cpp`), the catalogue
  in `core.mcp`.
- [tools/ui-mcp-document-new-test.sh](../tools/ui-mcp-document-new-test.sh)
  (Xvfb, the same shell client) covers `document_new` (mitcad#1266):
  `ui_command` `file.new_design` refused with a pointer to it, a folder
  outside the folders, `..`, a name with a folder and an existing name
  refused, a new `Design1.mitcad` in the current project recorded as a
  version and opened beside the project's design (both listed, the old
  handle still valid), model commands with and without the new handle
  changing only the new design, and outside projects a named file, an
  untitled design and a name in the first folder. The chat UI test
  (`tools/ui-chat-test.py`, run `new-design`) runs a chat agent's "read
  the open design, `document_new`, build in it" without the user's hand.
- [tools/ui-mcp-recovery-dialog-test.sh](../tools/ui-mcp-recovery-dialog-test.sh)
  (Xvfb, the same shell client) covers a dialog that waits for the user
  (mitcad#1725): with a killed session's autosave, the recovery question
  at start-up is listed by `ui_state` (`dialog`, `blocked_reason`),
  `model_command` and `ui_command` are refused naming it and
  `--no-recovery`, `document_list` gives the reason, and queries work;
  after Later commands run again; File › Recover Designs' question is
  named without the start-up's hint.
- The unit tests of the folder rules are in `app.unit`.
- The unit tests of the classification are in `core.model`, and those of
  the live catalogue are in `core.mcp`.

Run these CTests only in the approved isolated environments, as
[development.md](development.md#mcp-validation) describes.

## Modeling feedback

Use `model_query` with `{"query":"design_review","print":false}` for a
read-only review of model health, sketch intent, references, parameters
and organisation. Findings include stable rule ids, severity, targets,
advice and guide links. Print analysis is optional; an unavailable check
is reported separately from a clean result. See the reference's
"Design review" section for scopes, filters and print contexts.

Both profiles return up to three modeling hints with `model_command`
about features that call added or edited. They share the review's rules
and guide links. Set `"hints": false` beside `command` to omit them and
avoid the review query, independently of the timeline response mode.
