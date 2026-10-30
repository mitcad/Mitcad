# SPDX-License-Identifier: MIT
"""The chat panel's group rows of tool calls (mitcad#983) on the light and
the dark theme: pictures for a look, not a recurring test. Run by
tools/ui-chat-groups-look.sh, with the scripted agent of the chat's UI test
(tools/ui-chat-test.py, whose helpers it uses) in place of Claude Code.

For each theme, one message makes three calls (the second fails), a text,
two calls and a text; the main window is saved twice:

  chat-groups-<theme>-closed.png  both groups closed: "3 calls (1 failed) ·
                                  document_list" and "2 calls · ui_state"
  chat-groups-<theme>-open.png    the first group opened to its three rows,
                                  the failed one opened to its Input and
                                  Result; the second still closed

The app's logs as look-<theme>.log in the output folder.

Usage: ui-chat-groups-look.py APP STUB OUTPUT
"""
import importlib.util
import os
from pathlib import Path
import sys
import tempfile


def chat_test():
    path = Path(__file__).resolve().parent / "ui-chat-test.py"
    spec = importlib.util.spec_from_file_location("ui_chat_test", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    app, stub, output = sys.argv[1:4]
    out = Path(output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    chat = chat_test()
    for theme in ("light", "dark"):
        os.environ["MITCAD_TEST_COLOR_SCHEME"] = theme
        with tempfile.TemporaryDirectory(prefix="chat-look-", dir=out) as temporary:
            work = Path(temporary)
            (work / "designs").mkdir()
            chat.install_stub(stub, work / "clients")
            (work / "claude-signed-in").write_text("", encoding="utf-8")
            closed = out / ("chat-groups-" + theme + "-closed.png")
            opened = out / ("chat-groups-" + theme + "-open.png")
            script = [
                "wait-idle",
                "command view.chat",
                "wait-log 'Chat panel: ready' 60",
                *chat.send("measure with several calls"),
                "button '^Continue$' chatContinue",
                "wait-log 'Chat tool group: 3 calls, 1 failed' 60",
                "wait-log 'Chat: turn ended \\(success\\)' 60",
                "sleep 1000",
                "screenshot '" + str(closed) + "'",
                "button '^3 calls \\(1 failed\\) . document_list$' chatToolGroupHead",
                "wait-log 'Chat tool group opened: 3 calls, 1 failed, 3 rows shown'",
                "button '^model_query . bodies . failed$' chatToolHead",
                "sleep 1000",
                "screenshot '" + str(opened) + "'",
            ]
            log = chat.run(app, "look-" + theme, work, out, "floating", script)
            if "Asking the platform for the " + theme + " colour scheme" not in log:
                raise RuntimeError("the " + theme + " colour scheme was not asked for; see "
                                   + str(out / ("look-" + theme + ".log")))
            print("ok   " + str(closed) + " " + str(opened))


if __name__ == "__main__":
    main()
