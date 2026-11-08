# SPDX-License-Identifier: MIT
"""The evaluation's metrics: read from a saved design and an agent's transcript.

Usage: python3 -I metrics.py --cli <mitcad-cli> <run folder>

Writes report.md and report.json into the run folder: one row per task
folder (`<run>/<task>/workspace/design.mitcad`, `<run>/<task>/transcript.jsonl`,
`<run>/<task>/meta.json`, as run.py leaves them).

The design is read through the headless MCP server's queries, never from
the file's own format:
- sketches with dof > 0: the `timeline` query's `dof` of each sketch;
- features placed on a fixed plane: a `{"origin", "normal"}` plane anywhere
  in a feature's definition (the `feature` query's `def`);
- literal lengths used three or more times: the model parameters (the
  values a definition's slots hold) whose expression is a literal length,
  grouped by value;
- user parameters: the `parameters` query's `kind` `user`;
- default feature names (`Extrude3`) and features outside every timeline
  group: the `timeline` query;
- failed features: the `timeline` query's `status` `error`;
- each user parameter changed by +10 %: `set_parameter`, the features that
  fail then and did not before, `undo`.
"""

import argparse
import json
import math
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from mcp_client import McpError, Server  # noqa: E402

DESIGN = "design.mitcad"
TRANSCRIPT = "transcript.jsonl"
META = "meta.json"

# Default names whose base is not the type's own name in CamelCase
# (`BASE_NAME` of core/model/src/features/*.rs; `extrude` is `Extrude`).
OTHER_BASE_NAMES = {
    "Plane", "Axis", "Point", "Sketch", "Rib", "Web", "Constraint", "Joint",
    "AsBuiltJoint", "JointOrigin", "RigidGroup", "MotionLink", "Placeholder",
}
DEFAULT_NAME = re.compile(r"^([A-Z][A-Za-z]*)([0-9]+)$")
LITERAL = re.compile(r"^\s*[-+]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][-+]?[0-9]+)?\s*([A-Za-z]*)\s*$")
NOT_LENGTH_UNITS = {"", "deg", "rad", "°"}


def camel(type_name):
    return "".join(word[:1].upper() + word[1:] for word in type_name.split("_"))


def default_name(name, type_name):
    """True when the name is the one the model gives: `<Base><n>`."""
    match = DEFAULT_NAME.match(name or "")
    return bool(match) and (match.group(1) == camel(type_name) or match.group(1) in OTHER_BASE_NAMES)


def fixed_plane(value):
    """True when a definition holds a fixed plane (`{"origin", "normal"}`)."""
    if isinstance(value, dict):
        if "origin" in value and "normal" in value:
            return True
        return any(fixed_plane(item) for item in value.values())
    if isinstance(value, list):
        return any(fixed_plane(item) for item in value)
    return False


def repeated_literals(parameters, at_least=3):
    """Literal lengths that three or more slots hold: {"30 mm": 4}."""
    counts = {}
    for parameter in parameters:
        if parameter.get("kind") != "model" or parameter.get("dependencies"):
            continue
        if parameter.get("unit", "") in NOT_LENGTH_UNITS:
            continue
        if not LITERAL.match(str(parameter.get("expression", ""))):
            continue
        value = parameter.get("value")
        if not isinstance(value, (int, float)):
            continue
        key = f"{round(value, 6):g} mm"
        counts[key] = counts.get(key, 0) + 1
    return {key: count for key, count in sorted(counts.items()) if count >= at_least}


def failed(timeline):
    return {item["uid"] for item in timeline["features"] if item.get("status") == "error"}


def perturbed(value, unit):
    """The value +10 %; a whole count stays whole and grows by at least one."""
    if unit == "" and float(value).is_integer():
        return float(max(value + 1, round(value * 1.1)))
    return value * 1.1


def design_metrics(server, path):
    """The design's metrics; the design is opened, changed and closed unsaved."""
    document = server.tool("document_open", {"path": str(path)})["document"]
    try:
        timeline = server.query(document, {"query": "timeline"})
        features = timeline["features"]
        grouped = {uid for group in timeline.get("groups", []) for uid in group["features"]}
        definitions = {}
        for item in features:
            definitions[item["uid"]] = server.query(
                document, {"query": "feature", "uid": item["uid"]}).get("def", {})
        parameters = server.query(document, {"query": "parameters"})
        user = [p for p in parameters if p.get("kind") == "user"]
        before = failed(timeline)
        robust = []
        for parameter in user:
            value = parameter.get("value")
            entry = {"name": parameter["name"], "expression": parameter.get("expression")}
            if not isinstance(value, (int, float)) or not math.isfinite(value):
                entry["outcome"] = "skipped (no numeric value)"
                robust.append(entry)
                continue
            try:
                server.command(document, {"cmd": "set_parameter", "name": parameter["name"],
                                          "value": perturbed(value, parameter.get("unit", ""))})
            except McpError as error:
                entry["outcome"] = f"refused: {error}"
                robust.append(entry)
                continue
            after = server.query(document, {"query": "timeline"})
            names = {item["uid"]: item["name"] for item in after["features"]}
            broken = sorted(failed(after) - before)
            entry["outcome"] = "ok" if not broken else "failed"
            entry["failed"] = [names[uid] for uid in broken]
            robust.append(entry)
            server.command(document, {"cmd": "undo"})
        literals = repeated_literals(parameters)
        sketches = [item for item in features if item["type"] == "sketch"]
        return {
            "features": len(features),
            "sketches": len(sketches),
            "underdefined_sketches": [item["name"] for item in sketches if item.get("dof", 0) > 0],
            "fixed_plane_features": [item["name"] for item in features
                                     if fixed_plane(definitions[item["uid"]])],
            "repeated_literals": literals,
            "user_parameters": len(user),
            "default_names": [item["name"] for item in features
                              if default_name(item["name"], item["type"])],
            "groups": len(timeline.get("groups", [])),
            "ungrouped_features": len([item for item in features if item["uid"] not in grouped]),
            "failed_features": [item["name"] for item in features if item["uid"] in before],
            "parameter_changes": robust,
        }
    finally:
        server.tool("document_close", {"document": document, "discard": True})


def _claude_line(event, totals, seen):
    kind = event.get("type")
    message = event.get("message") or {}
    if kind == "assistant":
        for block in message.get("content") or []:
            if block.get("type") == "tool_use" and block.get("id") not in seen:
                seen.add(block.get("id"))
                totals["tool_calls"] += 1
                totals["tools"][block.get("name", "?")] = totals["tools"].get(block.get("name", "?"), 0) + 1
    elif kind == "user":
        content = message.get("content")
        for block in content if isinstance(content, list) else []:
            if block.get("type") == "tool_result" and block.get("is_error"):
                totals["tool_errors"] += 1
    elif kind == "result":
        usage = event.get("usage") or {}
        totals["input_tokens"] = usage.get("input_tokens", 0)
        totals["cached_input_tokens"] = (usage.get("cache_read_input_tokens", 0)
                                         + usage.get("cache_creation_input_tokens", 0))
        totals["output_tokens"] = usage.get("output_tokens", 0)
        totals["turns"] = event.get("num_turns")
        totals["cost_usd"] = event.get("total_cost_usd")
        totals["finished"] = not event.get("is_error", False)


def _codex_line(event, totals, seen):
    kind = event.get("type", "")
    item = event.get("item") or {}
    if kind == "item.completed" and item.get("type") == "mcp_tool_call" and item.get("id") not in seen:
        seen.add(item.get("id"))
        totals["tool_calls"] += 1
        name = f"{item.get('server', '?')}.{item.get('tool', '?')}"
        totals["tools"][name] = totals["tools"].get(name, 0) + 1
        result = item.get("result") or {}
        if (item.get("status") == "failed" or item.get("error")
                or (isinstance(result, dict) and (result.get("isError") or result.get("is_error")))):
            totals["tool_errors"] += 1
    elif kind == "item.completed" and item.get("type") in ("command_execution", "file_change", "web_search"):
        # A tool other than MitCad's: the run was not confined to them.
        totals["tools"][item["type"]] = totals["tools"].get(item["type"], 0) + 1
    elif kind == "turn.completed":
        usage = event.get("usage") or {}
        totals["input_tokens"] += usage.get("input_tokens", 0)
        totals["cached_input_tokens"] += usage.get("cached_input_tokens", 0)
        totals["output_tokens"] += usage.get("output_tokens", 0)
        totals["turns"] = (totals["turns"] or 0) + 1
        totals["finished"] = True
    elif kind in ("turn.failed", "error"):
        totals["finished"] = False


def transcript_metrics(path):
    """Tool calls, errors and tokens of a Claude Code stream-json or a
    `codex exec --json` transcript (one JSON object per line)."""
    totals = {"agent": None, "tool_calls": 0, "tool_errors": 0, "tools": {},
              "input_tokens": 0, "cached_input_tokens": 0, "output_tokens": 0,
              "turns": None, "cost_usd": None, "finished": False}
    seen = set()
    with open(path, encoding="utf-8") as lines:
        for line in lines:
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                event = json.loads(line)
            except ValueError:
                continue
            kind = event.get("type", "")
            if totals["agent"] is None:
                if kind in ("system", "assistant", "user", "result"):
                    totals["agent"] = "claude"
                elif "." in kind or kind in ("error",):
                    totals["agent"] = "codex"
            if totals["agent"] == "claude":
                _claude_line(event, totals, seen)
            elif totals["agent"] == "codex":
                _codex_line(event, totals, seen)
    return totals


COLUMNS = [
    ("saved", "saved"),
    ("features", "features"),
    ("dof > 0", "underdefined"),
    ("fixed plane", "fixed_plane"),
    ("literals x3+", "repeated_literals"),
    ("user params", "user_parameters"),
    ("default names", "default_names"),
    ("ungrouped", "ungrouped"),
    ("failed", "failed"),
    ("+10 % failing", "parameter_failures"),
    ("tool calls", "tool_calls"),
    ("tool errors", "tool_errors"),
    ("tokens in / cached / out", "tokens"),
]


def task_row(task, design, transcript):
    """One report row: short cells for the table."""
    row = {"task": task}
    if design is None:
        row.update({"saved": "no"})
    elif "error" in design:
        row.update({"saved": "unreadable"})
    else:
        changes = design["parameter_changes"]
        row.update({
            "saved": "yes",
            "features": design["features"],
            "underdefined": f"{len(design['underdefined_sketches'])}/{design['sketches']}",
            "fixed_plane": len(design["fixed_plane_features"]),
            "repeated_literals": sum(design["repeated_literals"].values()),
            "user_parameters": design["user_parameters"],
            "default_names": f"{len(design['default_names'])}/{design['features']}",
            "ungrouped": f"{design['ungrouped_features']}/{design['features']}",
            "failed": len(design["failed_features"]),
            "parameter_failures": f"{sum(1 for c in changes if c.get('outcome') != 'ok')}/{len(changes)}",
        })
    if transcript is not None:
        row.update({
            "tool_calls": transcript["tool_calls"],
            "tool_errors": transcript["tool_errors"],
            "tokens": f"{transcript['input_tokens']} / {transcript['cached_input_tokens']}"
                      f" / {transcript['output_tokens']}",
        })
    return row


def markdown(rows, header):
    lines = [header, "", "| task | " + " | ".join(title for title, _ in COLUMNS) + " |",
             "|---|" + "---|" * len(COLUMNS)]
    for row in rows:
        lines.append("| " + row["task"] + " | "
                     + " | ".join(str(row.get(key, "")) for _, key in COLUMNS) + " |")
    lines += [
        "",
        "dof > 0: sketches not fully constrained / sketches. literals x3+: slots holding a "
        "literal length that three or more slots hold. default names, ungrouped: features / "
        "features. +10 % failing: user parameters whose +10 % change fails a feature or is "
        "refused / user parameters. tokens: input (uncached) / cached input / output.",
        "",
    ]
    return "\n".join(lines)


def read_json(path):
    try:
        with open(path, encoding="utf-8") as source:
            return json.load(source)
    except (OSError, ValueError):
        return None


def evaluate_run(cli, run):
    """Reads every task folder of a run; writes report.json and report.md."""
    tasks = sorted(entry for entry in os.listdir(run)
                   if os.path.isdir(os.path.join(run, entry, "workspace")))
    results, rows, metas = {}, [], []
    for task in tasks:
        folder = os.path.join(run, task)
        workspace = os.path.join(folder, "workspace")
        design = None
        if os.path.isfile(os.path.join(workspace, DESIGN)):
            try:
                with Server(cli, workspace) as server:
                    design = design_metrics(server, DESIGN)
            except (McpError, KeyError, ValueError) as error:
                design = {"error": str(error)}
        transcript_path = os.path.join(folder, TRANSCRIPT)
        transcript = transcript_metrics(transcript_path) if os.path.isfile(transcript_path) else None
        meta = read_json(os.path.join(folder, META))
        if meta:
            metas.append(meta)
        results[task] = {"design": design, "transcript": transcript, "meta": meta}
        rows.append(task_row(task, design, transcript))
    first = metas[0] if metas else {}
    header = "# MCP evaluation run\n\n" + "\n".join(
        f"- {label}: {first.get(key, 'unknown')}" for label, key in (
            ("Platform", "platform"), ("MitCad commit", "commit"), ("MitCad version", "cli_version"),
            ("Agent", "agent"), ("Agent version", "agent_version"), ("Model", "model"),
            ("Started", "started")))
    with open(os.path.join(run, "report.json"), "w", encoding="utf-8") as out:
        json.dump(results, out, indent=1, sort_keys=True)
    with open(os.path.join(run, "report.md"), "w", encoding="utf-8") as out:
        out.write(markdown(rows, header))
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--cli", required=True, help="mitcad-cli of the build under test")
    parser.add_argument("run", help="the run folder run.py made")
    args = parser.parse_args()
    evaluate_run(os.path.abspath(args.cli), os.path.abspath(args.run))
    with open(os.path.join(args.run, "report.md"), encoding="utf-8") as report:
        sys.stdout.write(report.read())


if __name__ == "__main__":
    main()
