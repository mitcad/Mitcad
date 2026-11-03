#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Real macOS file-open delivery to the already-running test application.

The macOS UI wrapper owns the Aqua lock. Screenshot/file gates synchronize
this companion with TestDriver; neither changes a document or its selection.
"""
import json
from pathlib import Path
import plistlib
import shlex
import subprocess
import sys
import time


def scenario(work):
    work = Path(work)
    path = lambda name: shlex.quote(str(work / name))
    return "\n".join([
        "wait-idle", "capture-document-window first", "expect-document-windows 1",
        "expect-document-window-active first", "expect-document-tab-selected first", "expect-model-bodies 1 6000", "expect-viewport",
        "screenshot " + path("first-ready.png"), "expect-file " + path("second-sent") + " 90",
        "expect-document-windows 2", "wait-idle", "expect-model-bodies 1 120",
        "capture-document-window second", "expect-document-window-active second", "expect-document-tab-selected second", "expect-viewport",
        "focus-document-window first", "expect-model-bodies 1 6000",
        "focus-document-window second", "expect-model-bodies 1 120",
        "screenshot " + path("second-ready.png"), "expect-file " + path("first-sent") + " 90",
        # Reopening an existing path must activate its original captured owner.
        "expect-document-window-active first", "expect-document-tab-selected first", "expect-document-windows 2", "wait-idle",
        "expect-model-bodies 1 6000", "focus-document-window second", "expect-model-bodies 1 120",
        "focus-document-window first", "expect-model-bodies 1 6000",
        "screenshot " + path("reopened-first.png"), "expect-file " + path("process-verified") + " 90",
        "quit", ""])


def deliver(app, work, log):
    app, work, log = Path(app).resolve(), Path(work).resolve(), Path(log)
    bundle = app.parents[2]
    with (bundle / "Contents/Info.plist").open("rb") as stream:
        about = plistlib.load(stream)
    assert bundle.suffix == ".app" and about["CFBundleIdentifier"], about
    assert (bundle / "Contents/MacOS" / about["CFBundleExecutable"]).resolve() == app, about

    # A different checkout may still hold the GUI lock: our deadline starts
    # only once ui_run_app launches this case and creates its new log.
    while not log.exists() or log.stat().st_size == 0:
        time.sleep(0.05)

    def wait_file(name):
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if (work / name).is_file():
                return
            if any(line.startswith("TestDriver: FAIL") for line in log.read_text(errors="replace").splitlines()):
                raise AssertionError("UI assertion failed before " + name)
            time.sleep(0.05)
        raise AssertionError("Timed out waiting for " + name)

    observations = []

    def sole_process(expected=None):
        listing = subprocess.run(["/bin/ps", "-axww", "-o", "pid=", "-o", "comm="],
                                 check=True, text=True, capture_output=True).stdout
        matches = []
        for line in listing.splitlines():
            fields = line.strip().split(maxsplit=1)
            if len(fields) == 2 and Path(fields[1]).is_absolute() and Path(fields[1]).resolve() == app:
                matches.append(int(fields[0]))
        assert len(matches) == 1, "Expected one process at " + str(app) + ": " + repr(matches)
        pid = matches[0]
        assert expected is None or pid == expected, "System open replaced the application process"
        observations.append({"pid": pid, "executable": str(app), "bundle": str(bundle),
                             "identifier": about["CFBundleIdentifier"]})
        return pid

    wait_file("first-ready.png")
    pid = sole_process()
    for ready, document, sent in [(None, "second.mitcad", "second-sent"),
                                   ("second-ready.png", "first.mitcad", "first-sent")]:
        if ready:
            wait_file(ready)
        sole_process(pid)
        # No -n: send the ordinary LaunchServices event to this exact bundle.
        subprocess.run(["/usr/bin/open", "-a", str(bundle), str(work / document)], check=True, timeout=20)
        sole_process(pid)
        (work / sent).touch()
    wait_file("reopened-first.png")
    sole_process(pid)
    (work / "system-open-processes.json").write_text(json.dumps(observations, indent=2) + "\n")
    (work / "process-verified").touch()
    print("PASS: real system file opens retained one exact application process", flush=True)


if __name__ == "__main__":
    if sys.argv[1] == "script":
        print(scenario(sys.argv[2]), end="")
    elif sys.argv[1] == "deliver":
        deliver(*sys.argv[2:5])
    else:
        raise SystemExit("expected script <work> or deliver <app> <work> <log>")
