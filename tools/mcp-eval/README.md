# MCP evaluation of agents' designs

A fixed set of modelling tasks that an agent completes through MitCad's
headless MCP server, and metrics read from the designs it saves
(mitcad#1507). The set measures whether a change of what the server tells
agents (its instructions, the reference, guide topics) changes what they
build: run it before the change and after it, on the same platform and
agent, and compare the two reports.

| File | What it is |
|---|---|
| `tasks/*.md` | The six tasks, a title line and the prompt: a plate bracket with holes, a turned shaft with grooves, a shelled housing with screw bosses, a 3D-printed clip (horizontal hole, counterbore), a swept handle and a two-component assembly. Text only: no models of others. |
| `run.py` | The runner: one agent session per task, then the report. |
| `metrics.py` | The metric extractor and the report; also runs on its own on a run folder. |
| `mcp_client.py` | The small MCP client the extractor reads designs with. |
| `test_metrics.py` | The extractor's test, the ctest `cli.mcp-eval`. |

## Running it

Run it in an isolated environment only (the WSL distro, the Windows VM or
the macOS VM), never on the host. It needs a built `mitcad-cli`, Python 3
and an agent's CLI, installed and signed in there: Claude Code (`claude`)
or Codex (`codex`). The scripts are plain Python without POSIX-only paths,
so the same commands work on every platform (`py -3 -I` in place of
`python3 -I` on Windows).

```bash
python3 -I tools/mcp-eval/run.py --cli build/dev/tools/cli/mitcad-cli --agent claude --out ~/mcp-eval/baseline
```

- `--agent claude|codex` (default `claude`), `--agent-command` for an
  executable that is not on `PATH`, `--model` for another model than the
  agent's default.
- `--tasks 1-plate-bracket,6-two-component-assembly` runs some tasks only.
- `--max-turns` caps a Claude Code session (default 80), `--timeout` any
  session (default 3600 s).
- `--out` is the run folder; it must be empty or new (default
  `mcp-eval-<time>` in the current folder). Keep it out of the source
  tree.

Each task's session gets the prompt of its file and one closing sentence:
work in a new design and save it as `design.mitcad`. Nothing in the prompt
says how to model: that is what the metrics measure.

The agent gets MitCad's tools only, with the flags the application's chat
panel gives (`core/mcp/src/chat/claude.rs`, `codex.rs`), but the server is
the headless one, `mitcad-cli mcp --workspace <run>/<task>/workspace`:

- Claude Code: `-p` with stream-json output, `--tools ""` (no built-in
  tool), `--strict-mcp-config` with the MitCad server as the only MCP
  server, `--allowedTools mcp__mitcad`.
- Codex: `codex exec --json` in a read-only sandbox, with the MitCad
  server added and the shell, patch, picture and web search tools off.
  Codex layers this over the user's own `config.toml`, so MCP servers of
  that file may still start; the report's tool counts show any call to
  them.

Each task leaves `<run>/<task>/`: `workspace/design.mitcad`,
`transcript.jsonl` (the agent's JSON stream), `agent-stderr.txt`,
`meta.json` (agent and version, model, platform, MitCad commit and
version, time, exit code). After the last task the runner writes
`<run>/report.md` (the table) and `<run>/report.json` (every value and
the lists behind the counts: which sketches, which features, which
parameter changes failed).

To read a run folder again, for example after a change of the extractor:

```bash
python3 -I tools/mcp-eval/metrics.py --cli build/dev/tools/cli/mitcad-cli ~/mcp-eval/baseline
```

## Token budget

A run is six agent sessions, one per task. The report records each
session's tool calls and its input (uncached and cached) and output
tokens, so the budget comes from the baseline's report: estimate a run
before the baseline as six times one session of a few dozen tool calls,
in the order of a million mostly cached input tokens and tens of
thousands of output tokens per session. `--max-turns` bounds a Claude
Code session; run a single task with `--tasks` first to see a session's
use on that platform.

## Metrics

Read from the saved design through the headless server's queries; the
design is opened, changed for the parameter check and closed unsaved:

| Column | Read from |
|---|---|
| saved | whether `design.mitcad` exists and opens |
| features | `timeline` |
| dof > 0 | the sketches whose `dof` in `timeline` is above 0, of all sketches |
| fixed plane | the features whose definition (`feature` query, `def`) holds a fixed plane, `{"origin", "normal"}` |
| literals x3+ | the definitions' slots (model parameters) that hold a literal length that three or more slots hold; a parameter's name or an expression in a slot is no literal |
| user params | `parameters` of kind `user` |
| default names | the features still named as the model names them (`Extrude3`), of all features |
| ungrouped | the features in no timeline group, of all features |
| failed | the features whose `timeline` status is `error` |
| +10 % failing | the user parameters whose change by +10 % (`set_parameter`, then `undo`) fails a feature that did not fail before, or is refused, of all user parameters |
| tool calls, tool errors | the transcript: tool uses, and results marked as errors |
| tokens | the transcript: input without cache, cached input, output |

A whole count (a parameter without a unit) grows by at least one in the
+10 % check.

The report names the platform and the MitCad commit it ran on. One
platform is enough for a comparison: the metrics come from the saved
design, and the agent's choices do not depend on the platform.
