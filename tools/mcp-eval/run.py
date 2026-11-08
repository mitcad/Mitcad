# SPDX-License-Identifier: MIT
"""Runs the MCP evaluation tasks: one agent session per task through MitCad's
headless MCP server, then the report.

Usage: python3 -I run.py --cli <mitcad-cli> [--agent claude|codex] [--model M]
                         [--tasks a,b] [--out <run folder>] [--max-turns N]

Each task gets `<run>/<task>/`: `workspace/` (the server's workspace, where
the agent saves `design.mitcad`), `transcript.jsonl` (the agent's JSON
stream), `agent-stderr.txt` and `meta.json`. Then metrics.py writes
`<run>/report.md` and `<run>/report.json`.

The agent gets MitCad's tools only, as the application's chat panel starts
it (core/mcp/src/chat/claude.rs, codex.rs): Claude Code with every built-in
tool off and the MitCad server as its only MCP server, Codex with its shell,
patch, picture and web tools off in a read-only sandbox. The server here is
the headless one (`mitcad-cli mcp --workspace`), not a window's bridge.

Run it only in an isolated environment (the WSL distro, the Windows VM or
the macOS VM), where the agent's CLI is installed and signed in.
"""

import argparse
import datetime
import json
import os
import platform
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import metrics  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
TASKS = os.path.join(HERE, "tasks")
SERVER = "mitcad"

# What every task's prompt ends with: where the result goes, nothing about
# how to model it (that is what the evaluation measures).
CLOSING = (
    "\n\nWork in a new design (document_create). When the design is finished, "
    f"save it with document_save as `{metrics.DESIGN}`."
)


def task_names():
    return sorted(name[:-3] for name in os.listdir(TASKS) if name.endswith(".md"))


def task_prompt(name):
    with open(os.path.join(TASKS, name + ".md"), encoding="utf-8") as source:
        text = source.read()
    # The first line is the task's title for people; the prompt is the rest.
    return text.split("\n", 1)[1].strip() + CLOSING


def toml_literal(text):
    """A TOML literal string; it cannot hold a quote, which a path rarely does."""
    if "'" in text or "\n" in text:
        raise SystemExit(f"run.py: a path with a quote or line break cannot go to Codex: {text}")
    return "'" + text + "'"


def claude_command(executable, cli, workspace, config_path, model, max_turns):
    command = [executable, "-p", "--output-format", "stream-json", "--verbose",
               "--tools", "", "--strict-mcp-config", "--mcp-config", config_path,
               "--allowedTools", "mcp__" + SERVER, "--max-turns", str(max_turns)]
    if model:
        command += ["--model", model]
    with open(config_path, "w", encoding="utf-8") as out:
        json.dump({"mcpServers": {SERVER: {
            "command": cli, "args": ["mcp", "--workspace", workspace]}}}, out)
    return command


def codex_command(executable, cli, workspace, model):
    args = "[" + ", ".join(toml_literal(word) for word in ["mcp", "--workspace", workspace]) + "]"
    overrides = {
        f"mcp_servers.{SERVER}.command": toml_literal(cli),
        f"mcp_servers.{SERVER}.args": args,
        f"mcp_servers.{SERVER}.startup_timeout_sec": "60",
        f"mcp_servers.{SERVER}.tool_timeout_sec": "600",
        "approval_policy": "'never'",
        "features.shell_tool": "false",
        "features.unified_exec": "false",
        "features.apply_patch_freeform": "false",
        "features.view_image": "false",
        "features.web_search_request": "false",
        "web_search": "'disabled'",
    }
    command = [executable, "exec", "--json", "--skip-git-repo-check",
               "--sandbox", "read-only", "--cd", workspace]
    for key, value in overrides.items():
        command += ["-c", f"{key}={value}"]
    if model:
        command += ["--model", model]
    return command + ["-"]


def output_of(command):
    try:
        return subprocess.run(command, capture_output=True, text=True, timeout=60).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return None


def commit():
    """The MitCad commit of this checkout, when it is one."""
    git = shutil.which("git")
    if not git:
        return None
    return output_of([git, "-C", HERE, "rev-parse", "--short=12", "HEAD"])


def run_task(name, args, executable, run, context):
    folder = os.path.join(run, name)
    workspace = os.path.join(folder, "workspace")
    os.makedirs(workspace)
    if args.agent == "claude":
        command = claude_command(executable, context["cli"], workspace, os.path.join(folder, "mcp-config.json"),
                                 args.model, args.max_turns)
    else:
        command = codex_command(executable, context["cli"], workspace, args.model)
    started = datetime.datetime.now(datetime.timezone.utc)
    print(f"{name}: running {args.agent}", flush=True)
    with open(os.path.join(folder, metrics.TRANSCRIPT), "wb") as transcript, \
            open(os.path.join(folder, "agent-stderr.txt"), "wb") as stderr:
        try:
            completed = subprocess.run(command, input=task_prompt(name).encode("utf-8"),
                                       stdout=transcript, stderr=stderr, cwd=workspace,
                                       timeout=args.timeout)
            exit_code = completed.returncode
        except subprocess.TimeoutExpired:
            exit_code = "timeout"
    ended = datetime.datetime.now(datetime.timezone.utc)
    meta = {
        "task": name, "agent": args.agent, "agent_version": context["agent_version"],
        "model": args.model, "platform": context["platform"], "commit": context["commit"],
        "cli_version": context["cli_version"],
        "started": started.isoformat(timespec="seconds"),
        "seconds": round((ended - started).total_seconds()), "exit_code": exit_code,
        "max_turns": args.max_turns if args.agent == "claude" else None,
    }
    with open(os.path.join(folder, metrics.META), "w", encoding="utf-8") as out:
        json.dump(meta, out, indent=1)
    saved = os.path.isfile(os.path.join(workspace, metrics.DESIGN))
    print(f"{name}: exit {exit_code}, {meta['seconds']} s, design {'saved' if saved else 'not saved'}",
          flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--cli", required=True, help="mitcad-cli of the build under test")
    parser.add_argument("--agent", choices=("claude", "codex"), default="claude")
    parser.add_argument("--agent-command", help="the agent's executable (default: found on PATH)")
    parser.add_argument("--model", help="the agent's model (default: the agent's own)")
    parser.add_argument("--tasks", help="comma-separated task names (default: all): "
                        + ", ".join(task_names()))
    parser.add_argument("--out", help="the run folder (default: mcp-eval-<time> in the current folder)")
    parser.add_argument("--max-turns", type=int, default=80, help="Claude Code's turn limit per task")
    parser.add_argument("--timeout", type=int, default=3600, help="seconds per task")
    args = parser.parse_args()

    cli = os.path.abspath(args.cli)
    if not os.path.isfile(cli):
        raise SystemExit(f"run.py: no mitcad-cli at {cli}")
    executable = args.agent_command or shutil.which(args.agent)
    if not executable:
        raise SystemExit(f"run.py: {args.agent} is not on PATH (or give --agent-command)")
    names = args.tasks.split(",") if args.tasks else task_names()
    unknown = [name for name in names if name not in task_names()]
    if unknown:
        raise SystemExit(f"run.py: no such task: {', '.join(unknown)}")
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    run = os.path.abspath(args.out or f"mcp-eval-{stamp}")
    if os.path.exists(run) and os.listdir(run):
        raise SystemExit(f"run.py: the run folder is not empty: {run}")
    os.makedirs(run, exist_ok=True)

    context = {
        "cli": cli,
        "agent_version": output_of([executable, "--version"]),
        "platform": f"{platform.system()} {platform.release()} {platform.machine()}",
        "commit": commit(),
        "cli_version": output_of([cli, "--version"]),
    }
    for name in names:
        run_task(name, args, executable, run, context)
    metrics.evaluate_run(cli, run)
    print(f"report: {os.path.join(run, 'report.md')}")


if __name__ == "__main__":
    main()
