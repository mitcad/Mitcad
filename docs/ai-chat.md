<!-- SPDX-License-Identifier: MIT -->
# The chat panel

The chat panel lets you ask an AI agent to change the open design in
words: "a 30 × 20 × 10 mm box with the top edges rounded 2 mm". The agent
works on the design through MitCad's own tools; the changes appear in the
view, the browser and the timeline as they happen, and the whole message
undoes with one **Undo** (Ctrl+Z).

The agent is your own: MitCad uses the Claude Code or Codex you have
installed and signed in to. It does not install, change or sign in to it,
and needs no other component (no npm, Node or adapter, no configuration
file to edit, no terminal).

Open the panel with **View › Chat**, its button beside the assistant
indicator (the Docked layout's toolbar), or Ctrl+Shift+L (⌘⇧L on macOS).
In the Floating layout it is a panel on the right edge of the window; in
the Docked layout a dock on the right.

## Setup

When the panel opens, MitCad looks for Claude Code (`claude` on `PATH`,
`~/.local/bin`, on Windows `%USERPROFILE%\.local\bin\claude.exe`) and
Codex (`codex` on `PATH`, the same folders, npm's folder on Windows, the
ChatGPT application on macOS), the same lookup as **Connect AI
Assistant**. It uses the agent you used last; when that one is not
installed and the other is, the other. It asks the agent whether it is
signed in (`claude auth status`, `codex login status`). The state line
says what is next, and one button does it (for Claude Code here; Codex's
are the same with its name, **Sign in to ChatGPT** and Codex's own
page):

| State line | Button | What happens |
|---|---|---|
| The chat uses Claude Code, which is not installed. | **Install Claude Code** | Opens Claude Code's setup page. Install it; the panel finds it by itself, without a restart of MitCad |
| The chat uses Claude Code; Claude Desktop connects through Connect AI Assistant. | **Install Claude Code**, **Connect AI Assistant…** | Only Claude Desktop is installed: the chat needs Claude Code; Claude Desktop works with MitCad through **Help › Connect AI Assistant…** |
| Claude Code *x* is older than *y*, which the chat needs. | **Update Claude Code** | Opens the setup page |
| Claude Code is not signed in. | **Sign in to Claude** | Starts Claude Code's own sign-in, which opens your browser. MitCad never sees your credentials |
| Finish signing in to Claude in your browser. | **Check again** | Once you have signed in, the panel is ready |
| Claude Code is ready. | **Send** | Write your message |

Only Claude Code's native installation is used on Windows (`claude.exe`);
a launcher script left by an npm installation is not started. For Codex
installed with npm on Windows, the native `codex.exe` its package carries
is started instead of the launcher script.

The oldest versions the chat starts are Claude Code 2.1.0 and Codex
0.80.0; an older one shows **Update Claude Code** or **Update Codex**.

### Choosing the agent

With both installed, the header shows a choice of the agents found
instead of the agent's name. Choosing the other one starts a new
conversation in it (the agents do not share what was said), and MitCad
uses it from then on, also after a restart. **Preferences › AI Assistants
› Chat › Agent** chooses it too.

### Choosing the model

**Model** below the header sets the model the agent uses, for each agent
separately: **Default model** (the agent's own choice), the models the
agent names (Claude Code's Opus, Sonnet and Haiku; the models Codex lists
once it has run), or a name you type. A new model applies from the next
message: the agent starts again with it and goes on with the same
conversation. Beside the choice, dimmed, is the model the agent reports
it uses.

The chat needs MitCad's listener for assistants (the one Connect AI
Assistant uses). When MitCad was started with `--no-mcp`, the panel says
so.

Before your first message to an agent, the panel says once what leaves
your computer: what you write, the selection and pictures of the view go
to the agent's service (Anthropic for Claude Code, OpenAI for Codex),
whose terms apply. The notice is shown once for each agent.
**Continue** sends the message.

## Messages

Write in the field at the bottom and press **Enter** (Shift+Enter for a
new line). The answer streams in as the agent writes it, formatted
(Markdown). Each tool call the agent makes is a row: its tool and what it
did (`model_command · add_feature box`); a click opens it to the call's
input and result. A failed call is marked.

- **Selection** (on by default): the selected items go with the message,
  as `ui_state` gives them to an agent, so "round these edges" knows which.
- **View**: a picture of the view goes with the message.
- While the agent works the button is **Stop** (also Esc in the field). A
  message you send meanwhile waits and goes when the agent is done.
- **New conversation** (the + in the header) starts over: the agent no
  longer sees what was said before.
- The assistant indicator says **Claude Code working** (**Codex working**)
  while the agent works on a message.

The agent runs in an empty folder of its own, with its built-in tools off
(no shell, no file changes, no web search; Codex in a read-only sandbox,
asking nothing of its own): it can only use MitCad's tools of this window,
and none of your other MCP servers.

## One undo step per message

When the agent is done with a message, its changes become one step in the
undo history, named after the message: **AI: a 30 × 20 × 10 mm box…**.
One **Undo** takes the whole message back, and the version history shows
it by that name.

If you change the design yourself while the agent works on a message,
nothing is merged: the agent's changes stay the separate steps of each of
its calls, labelled "… (agent)", between yours. A sketch being edited
keeps its own undo steps, as when you draw in it.

## Approvals

What the agent may do without asking is set in **Preferences › AI
Assistants › Chat**, under **Approvals**:

| Approvals | Without asking | Asks first |
|---|---|---|
| **Ask every time** | reading the design and the window | every change of the design, saving, opening, exporting, saving pictures, undoing what was there before the message |
| **Model edits allowed** (the default) | reading, changing the design | saving (**Save As** too), opening another design, exporting, saving a picture to a file, a File command, undoing what was there before the message |
| **Allow everything** | everything | nothing |

A call that asks shows a card in the conversation, and the agent waits
for your answer:

- **Allow once**: this call runs.
- **Always allow**: this call and every later one of its kind run without
  asking, for this agent. **Reset Always allow** in Preferences makes them
  ask again.
- **Deny**: the call is refused; the agent is told you did not allow it.

**Stop** refuses a card that is waiting. The approvals are enforced by
MitCad's listener, not left to the agent: they hold whatever the agent
tries.

Saving and opening go only to and from the folders for designs
(**Preferences › AI Assistants**, as for every assistant).

## History

The conversation of each design is kept in your own data folder, not in
the design file: one file per design, named by a digest of the design's
path (the `chat` folder of MitCad's application data; only you can read
it). When you open the design again, its conversation is shown, and the
agent goes on with it (Claude Code resumes its session, Codex its
thread); if the agent no longer has it, a new one starts beside the shown
history, and the panel says so.

An untitled design's conversation stays in memory until you save the
design (it is then kept with the design's path) or close it. A design
saved under another name (**Save As**) takes its conversation along.

- **Clear History of This Design**: the panel's **…** menu.
- **Clear all history…**: **Preferences › AI Assistants › Chat**.
- **Keep the chat history of each design**: off, nothing is written (and
  nothing is resumed after a restart).

## How it works

### Claude Code

The panel starts one Claude Code process per conversation, in its own
print mode, with its stream-json input and output:

```text
claude -p --input-format stream-json --output-format stream-json --verbose
  --include-partial-messages --session-id <uuid> --tools ""
  --strict-mcp-config --mcp-config <json> --allowedTools mcp__mitcad
```

`--mcp-config` is a JSON string naming the installed `mitcad-cli` bridge to
this window: `mitcad-cli mcp --connect --instance <id> --chat-session
<marker>`. The marker tells MitCad's listener that the connection is the
chat's, and the listener asks the panel before each of its calls
([Approvals](#approvals)). Nothing is written into Claude Code's own
configuration. MitCad chooses the session id and continues with
`--resume <uuid>`.

**Stop** sends Claude Code SIGINT on Linux and macOS, which ends its
running turn (if it is still running 5 seconds later, the process is
ended). Windows has no way to deliver a console Ctrl+C to a program that
was started without a console of its own, so there **Stop** ends the
process; the next message continues the same session with `--resume`.

The model chosen in the panel is given as `--model <name>`.

### Codex

For Codex the panel starts one `codex app-server` process per
conversation and speaks its JSON-RPC over stdin and stdout, one JSON
object per line: `initialize` and `initialized`, then `thread/start` (or
`thread/resume` with the thread id of the design's conversation), one
`turn/start` per message (its text, the selection as a second text and
the picture of the view as a `data:` URL), `turn/interrupt` for **Stop**,
and `model/list` for the model choice. The answer streams in from
`item/agentMessage/delta`; `mcpToolCall` items are the tool rows, and
`turn/completed` ends the message.

MitCad's tools are given with the thread, as its configuration overrides
(the `config` of `thread/start` and `thread/resume`), never written into
Codex's `config.toml`:

```json
{"mcp_servers": {"mitcad": {"command": "<mitcad-cli>",
   "args": ["mcp", "--connect", "--instance", "<id>", "--chat-session", "<marker>"],
   "startup_timeout_sec": 60, "tool_timeout_sec": 900}},
 "approval_policy": "never", "sandbox_mode": "read-only",
 "features.shell_tool": false, "features.unified_exec": false,
 "features.apply_patch_freeform": false, "include_apply_patch_tool": false,
 "tools.web_search": false, "web_search": "disabled", "tools.view_image": false}
```

`mcp_servers` replaces the whole table, so your other MCP servers are not
started. Codex asks nothing itself (`approvalPolicy` `never`): the
approvals are MitCad's, enforced by its listener for the chat's bridge
connection as for Claude Code. If Codex still asks the panel something,
a confirmation of one of MitCad's own tools is accepted (the listener
then decides and asks you), and every request to run a command, change a
file or widen its permissions is declined. A tool call may wait 15
minutes for your answer to a card.

**Stop** sends `turn/interrupt` on every platform; if the turn has not
ended 5 seconds later, the process is ended and the next message resumes
the thread. Codex names its threads itself: the conversation goes on
with the thread id that `thread/start` returns.

The protocol messages are written from the app server's documentation
(the JSON Schema the installed CLI generates, `codex app-server
generate-json-schema`, is the reference to check them against). All of
it is in `core/mcp/src/chat/codex.rs`, so that a change of the protocol
touches that file and its tests only.

### The parts

The chat's own logic is in Rust (`core/mcp/src/chat.rs`): the agents'
streams as panel events, the command lines and messages, Codex's whole
JSON-RPC exchange, the approval policy, the undo merge and the history
format. The panel, the processes and the files are the application's
(`app/agent/`). Each agent is a class behind one interface
(`app/agent/ChatAgent.hpp`: `ClaudeCodeAgent`, `CodexAgent`), so that API
endpoints and local models can plug in later without changing the panel.
