#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Check the real shell GUI lock using child processes that open no GUI."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


FAKE_APP = '''#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
import json
import os
from pathlib import Path
import sys
import time
root = Path(sys.argv[3])
if os.environ.get("MITCAD_TEST_HEADLESS") == "1":
    assert os.environ["MITCAD_TEST_STEP_MS"] == os.environ.get("EXPECTED_STEP", "10")
    assert os.environ["MITCAD_TEST_SETTLE_MS"] == os.environ.get("EXPECTED_SETTLE", "20")
    try:
        os.fstat(9)
    except OSError:
        pass
    else:
        raise AssertionError("headless app inherited the desktop lock")
else:
    os.fstat(9)
(root / "started.json").write_text(json.dumps({"pid": os.getpid(), "parent": os.getppid(),
                                              "started_s": time.monotonic()}))
if sys.argv[5] == "driver-exit-fail":
    print("Failed to create an OpenGL context", flush=True)
    print("TestDriver: FAIL line 3: synthetic immediate assertion", flush=True)
    os._exit(1)
elif sys.argv[5] == "driver-fail":
    print("TestDriver: FAIL line 3: synthetic held modal loop", flush=True)
elif sys.argv[5] == "trace-only":
    print("TestDriver: line 3: expect-never 'TestDriver: FAIL'", flush=True)
while not (root / "release").exists():
    time.sleep(0.02)
os._exit(int(sys.argv[4]))
'''

SHELL = '''
set -u
. "$1"
ui_init "$2" "$3"
# A second harness may prepare its private files before it owns Aqua.
printf '%s\\n' "$UI_ROOT" > "$3/prepared-root.txt"
UI_TIMEOUT=$4
ui_run_app fake "$3" "$5" "$6" --ui=qt
if [ "$6" = post-hold ]; then
  : > "$3/verifying"
  while [ ! -f "$3/verified" ]; do sleep 0.02; done
fi
printf '{"exit":%s,"timeout":%s,"assertion":%s,"failures":%s,"signal":"%s","pid":"%s"}\\n' \\
    "$UI_EXIT" "$UI_TIMED_OUT" "$UI_ASSERTION_FAILED" "$UI_FAILURES" "$UI_SIGNAL" "$UI_PID" > "$3/status.json"
'''


def await_condition(predicate, description, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.02)
    raise AssertionError(description)


def load_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bash", required=True)
    parser.add_argument("--lib", type=Path, required=True)
    args = parser.parse_args()
    launchers = []
    orphan_pids = set()
    with tempfile.TemporaryDirectory(prefix="mitcad-ui-lock-test-") as temporary:
        root = Path(temporary)
        app = root / "fake-app"
        app.write_text(FAKE_APP)
        app.chmod(0o755)

        def launch(name, timeout=10, exit_code=0, mode="normal", headless=False, pacing=None):
            work = root / name
            work.mkdir()
            output = work / "harness.log"
            # HOME and the production lock path stay unchanged. Separate
            # checkout-like shells must contend on the real per-user lock.
            env = dict(os.environ)
            if headless:
                env.update(MITCAD_TEST_HEADLESS="1", QT_QPA_PLATFORM="offscreen", MITCAD_TEST_INPUT="fixture")
            if pacing:
                env.update(MITCAD_TEST_STEP_MS=str(pacing[0]), MITCAD_TEST_SETTLE_MS=str(pacing[1]),
                           EXPECTED_STEP=str(pacing[0]), EXPECTED_SETTLE=str(pacing[1]))
            with output.open("wb") as stream:
                process = subprocess.Popen(
                    [args.bash, "-c", SHELL, "lock-test", str(args.lib.resolve()), str(app), str(work),
                     str(timeout), str(exit_code), mode], env=env, stdout=stream, stderr=subprocess.STDOUT)
            launchers.append(process)
            return process, work

        def started(run):
            process, work = run
            await_condition(lambda: load_json(work / "started.json") or process.poll() is not None,
                            f"{work.name}: child did not start", timeout=540)
            record = load_json(work / "started.json")
            assert record is not None, (work / "harness.log").read_text()
            assert record["parent"] == process.pid, "UI_PID must remain a direct child, without a lock wrapper"
            return record["pid"]

        def queued(run):
            process, work = run
            await_condition(lambda: "Waiting for the shared macOS UI lock" in (work / "harness.log").read_text(),
                            f"{work.name}: harness did not reach the lock")
            assert process.poll() is None, (work / "harness.log").read_text()

        def release(run, expected_exit=0, expected_timeout=0):
            process, work = run
            (work / "release").touch()
            process.wait(timeout=20)
            assert process.returncode == 0, (work / "harness.log").read_text()
            record = load_json(work / "status.json")
            assert record is not None, (work / "harness.log").read_text()
            assert record["exit"] == expected_exit, record
            assert record["timeout"] == expected_timeout, record
            assert record["assertion"] == 0, record
            assert record["signal"] == "", record
            assert record["failures"] == (1 if expected_exit or expected_timeout else 0), record
            assert record["pid"] == "", "ui_run_app did not reap its real child"

        def remains_queued(run, duration):
            _, work = run
            until = time.monotonic() + duration
            while time.monotonic() < until:
                assert not (work / "started.json").exists(), "two GUI applications overlapped"
                time.sleep(0.02)

        try:
            # Lock waiting exceeds the next app's entire timeout budget;
            # only the actual application lifetime counts against that cap.
            first = launch("owner")
            started(first)
            # Headless Qt applications share neither the Aqua lock nor
            # each other's isolated settings, even beside an active GUI.
            headless_a = launch("headless-a", headless=True)
            headless_b = launch("headless-b", headless=True, pacing=(37, 61))
            started(headless_a)
            started(headless_b)
            assert first[0].poll() is None
            release(headless_a)
            release(headless_b)
            second = launch("queued", timeout=1)
            queued(second)
            first_root = (first[1] / "prepared-root.txt").read_text().strip()
            second_root = (second[1] / "prepared-root.txt").read_text().strip()
            assert first_root != second_root, "overlapping harnesses shared their settings/data root"
            assert Path(first_root).is_dir() and Path(second_root).is_dir()
            remains_queued(second, 1.4)
            release(first)
            started(second)
            release(second)

            # Post-app verification is headless: another harness may own
            # Aqua while it is still checking the first run's files.
            post = launch("post-work", mode="post-hold")
            started(post)
            (post[1] / "release").touch()
            await_condition(lambda: (post[1] / "verifying").exists(), "post-work did not start")
            next_app = launch("beside-post-work")
            started(next_app)
            assert post[0].poll() is None, "the verification fixture did not overlap"
            release(next_app)
            (post[1] / "verified").touch()
            post[0].wait(timeout=20)
            assert post[0].returncode == 0

            # A nonzero app exit releases ownership without hiding its
            # failure. The next run starts through the same global lock.
            failed = launch("failed", exit_code=7)
            started(failed)
            release(failed, expected_exit=7)
            normal = launch("after-failure")
            started(normal)
            release(normal)

            immediate = launch("immediate-driver-failure", mode="driver-exit-fail")
            started(immediate)
            release(immediate, expected_exit=1)

            # A real assertion can leave a native modal loop running after
            # Qt's exit request. Reap only our app, preserve its raw exit
            # status and classify it as an assertion, not a crash/timeout.
            lingering = launch("driver-failure", timeout=20, mode="driver-fail")
            started(lingering)
            detected = load_json(lingering[1] / "started.json")["started_s"]
            lingering[0].wait(timeout=10)
            assert time.monotonic() - detected >= 0.8, "assertion cleanup skipped its shutdown grace"
            record = load_json(lingering[1] / "status.json")
            assert record is not None, (lingering[1] / "harness.log").read_text()
            assert record == {"exit": 128 + signal.SIGTERM, "timeout": 0, "assertion": 1,
                              "failures": 1, "signal": "", "pid": ""}, record
            log = (lingering[1] / "harness.log").read_text()
            assert "TestDriver assertion failed; stopped its lingering app" in log, log
            assert "ended with signal" not in log and "did not exit within" not in log, log
            assert "TestDriver: FAIL line 3:" in (lingering[1] / "fake.log").read_text()
            after_assertion = launch("after-assertion")
            started(after_assertion)
            release(after_assertion)

            # A script trace quoting the failure text is not a failure.
            # Keep it alive beyond the grace period, then release normally.
            traced = launch("trace-only", mode="trace-only")
            started(traced)
            time.sleep(1.4)
            assert traced[0].poll() is None, (traced[1] / "harness.log").read_text()
            assert "TestDriver: line 3: expect-never 'TestDriver: FAIL'" in (traced[1] / "fake.log").read_text()
            release(traced)
            after_trace = launch("after-trace")
            started(after_trace)
            release(after_trace)

            # TERM while awaiting another app interrupts the helper's wait,
            # removes that helper and leaves the existing owner untouched.
            owner = launch("term-owner")
            started(owner)
            cancelled = launch("term-waiter", timeout=1)
            queued(cancelled)
            remains_queued(cancelled, 0.2)
            cancelled[0].terminate()
            cancelled[0].wait(timeout=3)
            assert cancelled[0].returncode == 143, (cancelled[1] / "harness.log").read_text()
            after_cancel = launch("after-term")
            queued(after_cancel)
            remains_queued(after_cancel, 0.3)
            release(owner)
            started(after_cancel)
            release(after_cancel)

            # SIGKILL cannot run a shell trap. The actual child inherits
            # FD 9 and keeps the lock until that child, too, has exited.
            killed = launch("killed-shell")
            app_pid = started(killed)
            killed[0].kill()
            killed[0].wait(timeout=3)
            os.kill(app_pid, 0)
            orphan_pids.add(app_pid)
            after_kill = launch("after-kill", timeout=1)
            queued(after_kill)
            remains_queued(after_kill, 1.4)
            os.kill(app_pid, signal.SIGTERM)
            started(after_kill)
            orphan_pids.discard(app_pid)
            release(after_kill)

            # The existing timeout path must also reap its actual child
            # and release FD 9 before the next independent run starts.
            timed = launch("timed", timeout=1)
            started(timed)
            timed[0].wait(timeout=10)
            record = load_json(timed[1] / "status.json")
            assert record is not None and record["timeout"] == 1 and record["failures"] == 1, record
            assert record["pid"] == "", record
            final = launch("after-timeout")
            started(final)
            release(final)

            # The setup dialog's Python launcher must contend with the
            # shell protocol on the same existing file. Its timeout starts
            # after acquisition and the actual app inherits ownership.
            python_app = root / "python-app"
            python_app.write_text(FAKE_APP.replace("os.fstat(9)\n", ""))

            def launch_python(name, timeout=10, exit_code=0):
                work = root / name
                work.mkdir()
                env = dict(os.environ, PYTHONPATH=str(args.lib.resolve().parent))
                code = ("import sys; from ui_macos_lock import run_gui; "
                        "sys.exit(run_gui(sys.argv[2:], timeout=float(sys.argv[1])).returncode)")
                with (work / "harness.log").open("wb") as stream:
                    process = subprocess.Popen(
                        [sys.executable, "-c", code, str(timeout), sys.executable, str(python_app),
                         "unused", "unused", str(work), str(exit_code), "normal"],
                        env=env, stdout=stream, stderr=subprocess.STDOUT)
                launchers.append(process)
                return process, work

            owner = launch("shell-before-python")
            started(owner)
            python_run = launch_python("python-waiter", timeout=1)
            remains_queued(python_run, 1.4)
            release(owner)
            started(python_run)
            (python_run[1] / "release").touch()
            assert python_run[0].wait(timeout=10) == 0

            python_run = launch_python("python-failed", exit_code=7)
            started(python_run)
            shell_run = launch("shell-after-python-failure")
            queued(shell_run)
            remains_queued(shell_run, 0.3)
            (python_run[1] / "release").touch()
            assert python_run[0].wait(timeout=10) == 7
            started(shell_run)
            release(shell_run)

            python_run = launch_python("python-timeout", timeout=1)
            started(python_run)
            assert python_run[0].wait(timeout=10) != 0
            assert "TimeoutExpired" in (python_run[1] / "harness.log").read_text()
            shell_run = launch("shell-after-python-timeout")
            started(shell_run)
            release(shell_run)

            python_run = launch_python("python-killed")
            app_pid = started(python_run)
            python_run[0].kill()
            python_run[0].wait(timeout=3)
            os.kill(app_pid, 0)
            orphan_pids.add(app_pid)
            shell_run = launch("shell-after-python-kill")
            queued(shell_run)
            remains_queued(shell_run, 1.4)
            os.kill(app_pid, signal.SIGTERM)
            started(shell_run)
            orphan_pids.discard(app_pid)
            release(shell_run)
        finally:
            for process in launchers:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=3)
            for pid in orphan_pids:
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
    print("PASS: global GUI exclusion, direct child PID, queued timeout, failure/assertion/trace classification, TERM, SIGKILL and timeout cleanup, Python/shell exclusion and inherited ownership")


if __name__ == "__main__":
    main()
