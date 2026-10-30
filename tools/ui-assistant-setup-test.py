# SPDX-License-Identifier: MIT
# check-all sources: app/mcp app/framework/TestDriver app/MainWindowMcp core/mcp core/ffi tools/cli
"""Drive the real setup dialog using private homes and a compiled client stub."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zipfile


def main():
    app, cli, stub, output = sys.argv[1:5]
    out = Path(output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    for mode in ("file", "cli", "cli-fail", "cli-corrupt", "cli-concurrent"):
        for chrome in ("floating", "docked"):
            name = mode + "-" + chrome
            with tempfile.TemporaryDirectory(prefix=name + "-", dir=out) as temporary:
                work = Path(temporary)
                env = dict(os.environ)
                if len(sys.argv) > 5:
                    env["PATH"] = sys.argv[5] + os.pathsep + env.get("PATH", "")
                for key in ("MITCAD_TEST_ASSISTANT_FAIL", "MITCAD_TEST_ASSISTANT_CORRUPT", "MITCAD_TEST_ASSISTANT_CONCURRENT"):
                    env.pop(key, None)
                for variable in ("MITCAD_SETTINGS_DIR", "MITCAD_AUTOSAVE_DIR", "MITCAD_PROJECTS_DIR",
                                 "MITCAD_THUMBNAIL_DIR", "MITCAD_CRASH_DIR", "MITCAD_MCP_DISCOVERY_DIR"):
                    env[variable] = str(work / variable.lower())
                home = work / "codex user's settings"
                home.mkdir()
                original = "# Keep the user's comment\nmodel = 'unchanged'\n"
                # Bytes as written: text mode turns the line ends into CRLF on Windows,
                # and the driver compares the file with this LF text.
                (home / "config.toml").write_bytes(original.encode("utf-8"))
                original_claude = {"keep": "unchanged", "mcpServers": {"mitcad": {"command": "old-bridge", "args": []}}}
                (work / "claude.json").write_text(json.dumps(original_claude) + "\n", encoding="utf-8")
                managed = work / "managed"
                managed.mkdir()
                (managed / "config.toml").write_bytes(original.encode("utf-8"))
                (managed / "requirements.toml").write_text("# Managed configuration\n", encoding="utf-8")
                clients = work / "clients"
                clients.mkdir()
                if mode != "file":
                    executable = clients / ("codex.exe" if os.name == "nt" else "codex")
                    shutil.copy2(stub, executable)
                    executable.chmod(0o755)
                env.update(CODEX_HOME=str(home), MITCAD_ASSISTANT_TEST_WORK=str(work),
                    MITCAD_TEST_ASSISTANT_EXECUTABLES=str(clients), MITCAD_TEST_ASSISTANT_BUNDLE_OPEN="capture",
                    MITCAD_TEST_ASSISTANT_STUB=str(Path(stub).resolve()),
                    MITCAD_RESULT_STORE="off", MITCAD_NO_UPDATE_CHECK="1", QT_FORCE_STDERR_LOGGING="1")
                if mode == "cli-fail": env["MITCAD_TEST_ASSISTANT_FAIL"] = "1"
                if mode == "cli-corrupt": env["MITCAD_TEST_ASSISTANT_CORRUPT"] = "1"
                if mode == "cli-concurrent": env["MITCAD_TEST_ASSISTANT_CONCURRENT"] = "1"
                script = ["wait-idle", "expect-model-bodies 0 0", "command help.connect_assistant",
                    "focus-dialog '^Connect AI Assistant$'", "expect-assistant-setup codex",
                    "expect-assistant-setup other", "expect-assistant-setup disconnect-other",
                    "expect-assistant-setup folder",
                    "expect-assistant-setup missing-claude",
                    # Found in the isolated home's .local/bin, outside PATH (mitcad#956).
                    "expect-assistant-setup local-bin", "expect-assistant-setup claude"]
                if sys.platform == "win32":
                    # A Microsoft Store Claude Desktop without a .mcpb handler (mitcad#763).
                    script.append("expect-assistant-setup packaged-claude")
                if sys.platform in ("darwin", "win32"):
                    script.append("expect-assistant-setup bundle")
                script.extend(["expect-assistant-setup blocked",
                    "screenshot-dialog '" + str(out / (name + ".png")) + "'",
                    "button '^Close$'", "expect-dialog-gone '^Connect AI Assistant$'",
                    "focus-main", "expect-model-bodies 0 0"])
                source = work / "setup.mitcad-ui"
                source.write_text("\n".join(script) + "\n", encoding="utf-8")
                env["MITCAD_TEST_INPUT"] = str(source)
                log_path = out / (name + ".log")
                with log_path.open("wb") as log:
                    result = subprocess.run([app, "--ui=qt", "--chrome=" + chrome, "--no-recovery", "--no-mcp"],
                        env=env, stdout=log, stderr=subprocess.STDOUT, timeout=120)
                text = log_path.read_text(encoding="utf-8", errors="replace")
                if result.returncode or "TestDriver: done" not in text or any(line.startswith("TestDriver: FAIL") for line in text.splitlines()):
                    raise RuntimeError(name + " setup UI failed; see " + str(log_path))
                if mode != "file":
                    argv = json.loads((work / "registration-argv.json").read_text(encoding="utf-8"))
                    if argv[:4] != ["mcp", "add", "mitcad-desktop", "--"] or argv[-2:] != ["mcp", "--connect"]:
                        raise RuntimeError("client received incorrect structured arguments")
                if (managed / "config.toml").read_text(encoding="utf-8") != original:
                    raise RuntimeError("managed configuration was changed")
                claude_argv = json.loads((work / "claude-registration-argv.json").read_text(encoding="utf-8"))
                if claude_argv[:6] != ["mcp", "add", "--scope", "user", "mitcad", "--"] or claude_argv[-2:] != ["mcp", "--connect"]:
                    raise RuntimeError("Claude client received incorrect user-scope arguments")
                claude_config = json.loads((work / "claude.json").read_text(encoding="utf-8"))
                if claude_config.get("keep") != "unchanged":
                    raise RuntimeError("Claude registration changed unrelated configuration")
                if mode in ("cli-fail", "cli-corrupt", "cli-concurrent"):
                    expected = dict(original_claude)
                    if mode == "cli-concurrent": expected["concurrent"] = True
                    if claude_config != expected:
                        raise RuntimeError("rejected Claude registration changed the real configuration")
                elif "mitcad" not in claude_config.get("mcpServers", {}):
                    raise RuntimeError("Claude registration was not applied")
                if sys.platform in ("darwin", "win32"):
                    bundle_path = work / "assistant-setup" / "mitcad-desktop.mcpb"
                    shutil.copy2(bundle_path, out / (name + ".mcpb"))
                    with zipfile.ZipFile(bundle_path) as bundle:
                        manifest = json.loads(bundle.read("manifest.json"))
                        entry = manifest["server"]["entry_point"]
                        executable = bundle.getinfo(entry)
                        if manifest["manifest_version"] != "0.3" or (executable.external_attr >> 16) & 0o111 == 0:
                            raise RuntimeError("bundle launcher metadata is invalid")
                        if manifest["server"]["mcp_config"]["args"][-2:] != ["mcp", "--connect"]:
                            raise RuntimeError("bundle bridge arguments are invalid")
                        if len(bundle.read(entry)) < 100:
                            raise RuntimeError("bundle does not contain the actual compiled launcher")
    print("PASS: assistant setup, private file and CLI homes, rollback, folders and both layouts")


if __name__ == "__main__":
    main()
