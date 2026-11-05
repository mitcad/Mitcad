<!-- SPDX-License-Identifier: MIT -->
# The chat panel

The chat panel lets you ask an AI agent to change the open design in
words: "a 30 × 20 × 10 mm box with the top edges rounded 2 mm". The agent
works on the design through MitCad's own tools; the changes appear in the
view, the browser and the timeline as they happen, and the whole message
undoes with one **Undo** (Ctrl+Z).

The agent is your own: MitCad uses the Claude Code or Codex you have
installed and signed in to, or a model's service with your API key or a
local model ([API endpoints and local models](#api-endpoints-and-local-models)), or a program of your own that speaks the
Agent Client Protocol ([A custom command](#a-custom-command)). It does
not install, change or sign in to it, and needs no other component (no
npm, Node or adapter, no configuration file to edit, no terminal).

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

With more than one agent available, the header shows a choice of them
instead of the agent's name: every Claude Code found (one on `PATH`, one
in `~/.local/bin`, npm's, each separately, with its folder beside its
name: **Claude Code (~/.local/bin)**), Codex, and every API endpoint or
local model and the custom command turned on in Preferences. The Claude
Code that Claude Desktop keeps for itself (in its settings folder's
`claude-code`) is listed as **Claude Code (Claude Desktop's)** when it is
there; whether it runs on its own and with which sign-in is what its own
check says beside it.

Each entry in the list shows its state, so that a wrong account is seen
before you send: **Claude Code · signed in as someone@example.com**,
**not signed in**, **needs an API key**, **not found**, **ready** (an
endpoint, a local model), and the account failure the chat met, if any
(**· your organization has disabled access**). MitCad checks every agent
found when the panel opens.

Choosing another one starts a new conversation in it (the agents do not
share what was said), and MitCad uses it from then on, also after a
restart. **Preferences › AI Assistants › Chat › Agent** chooses it too.

When the chosen agent cannot be used (it is not signed in, its account
fails, it is missing), its line says how to fix it: for Claude Code, sign
in here or run `claude` in a terminal and type `/login`; for Codex, sign in
here or run `codex logout`, then `codex login`. Another agent that is
ready is offered beside it (**Use Codex**).

Claude Desktop itself is not an agent of the chat: it has no program the
chat can start. Where it is the only assistant installed, the panel says
so and points to **Connect AI Assistant**, through which Claude Desktop
works with MitCad.

The chat's own agent is not one of the assistants connected to the window:
the assistant indicator says **Claude Code working** while it works on a
message, and its connection cannot be disconnected there. **Disconnect**
in Connect AI Assistant concerns the assistants connected from outside.

### Choosing the model

**Model** below the header sets the model the agent uses, for each agent
separately: **Default model** (the agent's own choice), the models the
agent names (Claude Code's Opus, Sonnet and Haiku; the models Codex lists
once it has run), or a name you type. A new model applies from the next
message: the agent starts again with it and goes on with the same
conversation. Beside the choice, dimmed, is the model the agent reports
it uses.

### API endpoints and local models

Instead of an agent program, the chat can use a model's service directly:
the **Anthropic API** with your own API key, a local model in **Ollama**
or **LM Studio**, or a **Custom endpoint** that speaks the
OpenAI-compatible chat completions API (a URL and a model). MitCad then
runs the agent's work itself: it gives the model MitCad's tools, makes
the calls the model asks for through its own listener, with the same
approvals as for Claude Code and Codex, and sends back the results until
the model is done. No agent program and no other component is needed.

Turn them on in **Preferences › AI Assistants › Chat › API endpoints**:
each one that is checked is offered in the panel's agent choice. Each has
its address, filled in for the presets:

| Agent | Address | Key |
|---|---|---|
| Anthropic API | `https://api.anthropic.com` | required |
| Ollama | `http://localhost:11434/v1` | none |
| LM Studio | `http://localhost:1234/v1` | none |
| Custom endpoint | yours, ending where `/chat/completions` and `/models` are (for example `https://example.com/v1`) | optional |

An address must be `https://`, or `http://` to this computer only
(`localhost`, `127.0.0.1`, `::1`), so that a key never travels unencrypted;
an address with a user name, a password or a query is not used.
**Request timeout** (300 s by default) is the longest MitCad waits for the
service's next data before it gives up on the message.

The key is kept in the system's keychain (Windows' Credential Manager,
the macOS Keychain, the Secret Service or KWallet on Linux), never in a
file of MitCad's or in its log. Without a keychain it is kept until MitCad
closes. Type it in **New API key** in Preferences or, when the panel says
**Anthropic API needs an API key.**, in its field and **Save Key**;
**Remove Key** in Preferences removes it. For the Anthropic API, MitCad
also uses `ANTHROPIC_API_KEY` from its environment when no key is kept.

When the panel opens or the agent is chosen, MitCad asks the service for
its models (`/models`). The answer makes the agent ready and fills
**Model** (the Anthropic API offers its current models even before);
a refused key asks for a new one; no answer says so:

| State line | Button | What happens |
|---|---|---|
| No answer from Ollama at http://localhost:11434/v1. Start it, or get it. | **Get Ollama** (**Get LM Studio**) | Opens its download page. Start the server; the panel goes on by itself once it answers |
| No answer from Custom endpoint at *address*: *why* | **Chat Preferences…** | Check the address |
| Custom endpoint has no address: set it in Preferences › AI Assistants › Chat. | **Chat Preferences…** | Opens Preferences |
| Anthropic API needs an API key. (*Anthropic API refused the API key. Enter a valid one.*) | **Save Key** beside the key field | Keeps the key and asks the service again |
| Ollama is ready. | **Send** | Write your message |

For a local server, **Default model** is the first model it lists; choose
another in **Model**. For the Anthropic API it is Claude Opus 5.5
(`claude-opus-5-5`).

A model that cannot call tools cannot change the design. When the service
says so (Ollama: "… does not support tools"), the message ends with
**This model cannot edit the design: *model* does not support tool calls.
Choose a model that does.**, and the message is not kept in the
conversation, so you can send it again with another model.

The conversation of an API agent is kept for each design as the others
are, and its full exchange with the model (without pictures) beside the
history, so that a reopened design goes on with it. **Clear History of
This Design** and **Clear all history…** remove it too.

### A custom command

An agent program that speaks the [Agent Client
Protocol](https://agentclientprotocol.com) itself can be the chat's agent
too. Set it in **Preferences › AI Assistants › Chat › Custom agent**:
check **Custom command**, and give its program (a name, looked for as a
command in a terminal is and in the same folders as Claude Code, or its
full path) and its arguments on one line (quote an argument with spaces).
Checked, it is offered in the panel's agent choice as **Custom command**.

MitCad starts the command as it is and talks to it over its standard
input and output; it ships and installs no adapter of any kind, and
starts no npm or Node launcher. The command must speak the protocol
itself. When the agent is chosen or the panel opens, MitCad starts the
command once for the protocol's handshake (`initialize`):

| State line | Button | What happens |
|---|---|---|
| No command is set for the custom agent: set it in Preferences. | **Chat Preferences…** | Opens Preferences |
| The custom command *name* was not found. | **Chat Preferences…** | Check the program's name or path |
| *program* did not answer the Agent Client Protocol handshake: *why*. | **Chat Preferences…** | The command is not an agent of this kind, or it failed: *why* says what it wrote, how it ended, or that no answer came in 20 s |
| Custom command is ready. | **Send** | Write your message |

A command that did not answer is not started again until it, its
arguments or its program file change (the panel then goes on by itself)
or the panel is opened again. On Windows a launcher script (`.cmd`,
`.bat`, `.ps1`) is not started: give the agent's program itself.

The agent works with the same approvals, one undo step per message and
history as Claude Code and Codex. When it has a model selector, **Model**
offers its models; a chosen one is set before the next message. Signing
in, an API key or anything else the agent needs is the agent's own:
set it up with its own tools before you use it here.

The chat needs MitCad's listener for assistants (the one Connect AI
Assistant uses). When MitCad was started with `--no-mcp`, the panel says
so.

Before your first message to an agent, the panel says once what leaves
your computer: what you write, the selection, the open comments and
pictures of the view go to the agent's service (Anthropic for Claude Code, OpenAI for Codex,
the endpoint's address for an API agent), whose terms apply; for a local
model, that they stay on this computer; for a custom command, that they
go to the command and wherever it sends them. The notice is shown once for each
agent.
**Continue** sends the message.

## Account and sign-in problems

An agent can be signed in while its account cannot use the service. An
organization may disable access, a sign-in may expire, an API key may be
missing or invalid, or a credit or rate limit may be reached. These failures
stay in the state line in the error colour; the panel stops saying **ready**
and shows the failure once in the conversation. A failed CAD tool does not
change the account state.

**How to fix** opens help inside the panel. For Claude Code, sign in again
with an allowed account (run `claude` in a terminal, then type `/logout` and `/login`),
set `ANTHROPIC_API_KEY` for MitCad and restart it, or ask your organization's
administrator to enable access. For Codex, use `codex logout`, then `codex
login`, or configure its API key. For an API agent, enter a valid key in
Preferences (**New API key**). Check billing or wait for a usage limit to
reset where applicable.

Choose **Check again** after correcting the account. It checks sign-in and
sends one short request in a fresh process, with no design, conversation,
selection or picture attached and no permission to call MitCad's tools.
The request may use the account's allowance. The error clears only after
the request succeeds; a failed or timed-out check keeps it visible. A check
that fails for another reason (no network, a timeout) keeps the account's
reason in the state line and says why in a notice. When the agent itself
is no longer found, the check ends at once and the panel shows the agent's
setup (**Install**) instead; **Check again** returns once it is found.
Messages written while the failing message ran are not sent; a notice says
how many.
**New conversation** (+) also ends the old process, so the next conversation
uses the latest sign-in. Automatic setup checks alone do not clear a known
account failure.

## Messages

Write in the field at the bottom and press **Enter** (Shift+Enter for a
new line). The answer streams in as the agent writes it, formatted
(Markdown). Each tool call the agent makes is a row: its tool and what it
did (`model_command · add_feature box`); a click opens it to the call's
input and result. A failed call is marked.

Calls that follow each other, with nothing else between them, are one
group row, so that twenty calls between two sentences of the agent take
one line: the number of calls (and of failed ones) and the latest call
with its state, `18 calls (14 failed) · model_query · measure — running`.
It changes as the calls come and finish, and takes the error colour only
while its latest call failed. A click opens it to the calls' own rows.
The agent's text ends a group, and so does an approval card, which shows
at once below it, never inside it; the next call starts a new group. A
single call is its own row. A design's stored history is shown grouped
the same way (the history keeps each call as before;
`core/mcp/src/chat/groups.rs` makes the groups).

- **Selection** (on by default): the selected items go with the message,
  as `ui_state` gives them to an agent, so "round these edges" knows which.
- **Comments** (on by default): the design's open comments go with the
  message, each with its id, text, target and author, so "do the
  comments" knows what to do. The agent addresses each one with a short
  note after its change, in the same undo step; you resolve them in the
  Comments panel ([MCP: Comments for agents](mcp.md#comments-for-agents)).
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
| **Ask every time** | reading the design and the window | every change of the design, a new design, saving, opening, exporting, saving pictures, undoing what was there before the message |
| **Model edits allowed** (the default) | reading, changing the design, a new design beside it | saving (**Save As** too), opening another design, exporting, saving a picture to a file, a File command, undoing what was there before the message |
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

A new design (the tool `document_new`, mitcad#1266) is an empty design
in a tab of its own beside the open ones, made as **File › New Design**
makes it: it replaces nothing and writes over no file, so it runs as a
change of the design does. The agent then works in the new design (and
can still read the one the conversation started in); the conversation
stays in the tab where it started.

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
`turn/start` per message (its text, the selection and the open comments
as a second text and
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
 "features.apply_patch_freeform": false, "features.view_image": false,
 "features.web_search_request": false, "web_search": "disabled"}
```

`mcp_servers` names MitCad's bridge alone, so that your other MCP
servers are not started. Codex has no setting that removes its
`apply_patch` tool; the read-only sandbox keeps it from changing files,
and a request to change one is declined. Codex asks nothing itself
(`approvalPolicy` `never`): the approvals are MitCad's, enforced by its
listener for the chat's bridge connection as for Claude Code. If Codex
still asks the panel something, a confirmation of one of MitCad's own
tools is accepted (the listener then decides and asks you), and every
request to run a command, change a file or widen its permissions is
declined. A tool call may wait 15
minutes for your answer to a card.

**Stop** sends `turn/interrupt` on every platform; if the turn has not
ended 5 seconds later, the process is ended and the next message resumes
the thread. Codex names its threads itself: the conversation goes on
with the thread id that `thread/start` returns.

The protocol messages were checked against the JSON Schema that `codex
app-server generate-json-schema` writes for codex-cli 0.157.1 (its
stable API, without `--experimental`), and the configuration keys
against the keys that version knows; the scripted test agent speaks the
same shapes. All of it is in `core/mcp/src/chat/codex.rs`, so that a
change of the protocol touches that file and its tests only.

What only a run with a real, signed-in Codex can show (the schema does
not say it):

- whether `mcp_servers` in the thread's `config` replaces the table of
  your `config.toml` or is merged with it (Codex layers the overrides
  over your configuration; another MCP server's start is logged as
  `other_mcp_server`, a key Codex ignores as `config_warning`);
- that the built-in tools are really gone from the thread (shell,
  `view_image`, web search) and which tools of newer features are on by
  default;
- whether Codex calls MitCad's tools without asking under `approvalPolicy`
  `never`, or declines them (then the bridge's
  `default_tools_approval_mode = "approve"`, which 0.157.1 knows, is the
  setting to add);
- the text of `codex login status`, and that the oldest version the chat
  starts, Codex 0.80.0, already has every message and key used here (only
  0.157.1's schema was at hand).

### API endpoints and local models

For an API agent there is no process. MitCad opens the chat's own
connection to its listener, with the listener's token and the chat's
marker as the bridge would, so the listener's approvals for the chat
apply exactly as they do for Claude Code and Codex. Over it MitCad sends
`initialize` and `tools/list`, and gives the model the tools and the
listener's instructions (after one paragraph of its own) as the system
prompt. Each message is a streamed request with the whole conversation:

- the Anthropic Messages API: `POST /v1/messages` with `x-api-key`,
  `anthropic-version: 2026-10-21`, `stream: true`, `max_tokens` 32000 and
  the tools as `input_schema`; the server-sent events give the text, the
  tool calls (their input streams as JSON pieces) and the model's
  thinking blocks, which go back unchanged with their signatures in the
  next request;
- an OpenAI-compatible endpoint: `POST <address>/chat/completions` with
  `Authorization: Bearer <key>` when a key is kept, `stream: true` and the
  tools as `function`s; the tool calls stream in pieces by their index.

Each call the model asks for goes to the listener as a `tools/call`; its
result goes back to the model (pictures too for the Anthropic API; text
only for an OpenAI-compatible endpoint, whose tool messages carry no
pictures), and the next request follows, until the model answers without
a call (at most 100 requests for one message). A tool result's text is
cut at 100 000 characters for the model. **Stop** ends the request in
flight; calls still running are answered as stopped, and the message
stays in the conversation for the next one to follow. Redirects are not
followed (they would carry the key elsewhere).

A refusal that names an account problem (an invalid key, a credit or rate
limit, HTTP 401 or 429) is an account error like Claude Code's, with
**How to fix** and **Check again**. The kept exchange is the chat folder's
`api-sessions/<session>.json`. All of the two services' protocols are in
`core/mcp/src/chat/api.rs` (and its tests, `api_tests.rs`); the
application's side, the connection, the requests and the keychain, in
`app/agent/ApiAgent.cpp`.

### The custom command's protocol

For a custom command the panel starts one process per conversation, in
the chat's own working folder, and speaks the Agent Client Protocol over
its standard input and output: JSON-RPC 2.0, one message per line, in the
shapes of the protocol's published schema (version 1). After
`initialize` (MitCad offers no file system and no terminal:
`clientCapabilities` `fs.readTextFile`, `fs.writeTextFile` and `terminal`
are `false`), `session/new` starts the conversation, or `session/load`
goes on with the design's session when the agent can load sessions
(`agentCapabilities.loadSession`; otherwise, or when the agent no longer
has it, a new session starts and the panel says so). The session's only
MCP server is MitCad's bridge:

```json
{"cwd": "<the chat's working folder>",
 "mcpServers": [{"name": "mitcad", "command": "<mitcad-cli>",
   "args": ["mcp", "--connect", "--instance", "<id>", "--chat-session", "<marker>"],
   "env": []}]}
```

Each message is a `session/prompt` (its text, the selection and the open
comments as a second text, and the picture of the view as an `image`
block when the agent takes pictures, `promptCapabilities.image`). The
`session/update` notifications stream the answer
(`agent_message_chunk`) and the tool rows (`tool_call`,
`tool_call_update`); the prompt's answer ends the message with its stop
reason (`end_turn`; `max_tokens`, `max_turn_requests` and `refusal` end
it with an error). The updates with which an agent replays a loaded
session are not shown: MitCad shows its own history. **Stop** sends
`session/cancel`; the prompt ends `cancelled`, and if it has not ended 5
seconds later, the process is ended and the next message loads the
session again.

The agent asks MitCad before some of its calls
(`session/request_permission`): a call of MitCad's own tools is allowed
once, as the listener then decides and asks you by the approvals above;
every other call (the agent's own edits and commands) is rejected. A
request for a file or a terminal is refused. The agent's model is its
model selector (a `configOptions` entry of the category `model`), set with
`session/set_config_option`. An `auth_required` refusal is an account
error with **How to fix** and **Check again**.

All of the protocol is in `core/mcp/src/chat/acp.rs` (and its tests,
`acp_tests.rs`); the process and the handshake check in
`app/agent/AcpAgent.cpp`.

### The parts

The chat's own logic is in Rust (`core/mcp/src/chat.rs`): the agents'
streams as panel events, the command lines and messages, Codex's whole
JSON-RPC exchange, the approval policy, the undo merge, the history
format and the tool calls' group rows (`tool_groups`). The panel, the processes and the files are the application's
(`app/agent/`). Each agent is a class behind one interface
(`app/agent/ChatAgent.hpp`: `ClaudeCodeAgent`, `CodexAgent`, `ApiAgent`
for the API endpoints and local models and `AcpAgent` for a custom
command), so that the panel does not change with an agent's protocol.
