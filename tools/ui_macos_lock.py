# SPDX-License-Identifier: MIT
"""Keep Python-launched macOS applications on the shared Aqua lock."""
from contextlib import contextmanager
import os
from pathlib import Path
import subprocess
import sys


@contextmanager
def gui_lock():
    if sys.platform != "darwin":
        yield {}
        return
    import fcntl

    directory = Path.home() / ".mitcad"
    directory.mkdir(exist_ok=True)
    descriptor = os.open(directory / "macos-ui.lock", os.O_RDWR | os.O_CREAT, 0o600)
    try:
        # Wait before the caller starts the application timeout. The app
        # inherits the same open description, retaining ownership even if
        # its Python harness is killed. Never unlink or explicitly unlock:
        # close releases ownership only when the last inheritor has ended.
        fcntl.flock(descriptor, fcntl.LOCK_EX)
        yield {"pass_fds": (descriptor,)}
    finally:
        os.close(descriptor)


def run_gui(command, **kwargs):
    with gui_lock() as options:
        if "pass_fds" in options:
            options["pass_fds"] = (*kwargs.pop("pass_fds", ()), *options["pass_fds"])
        return subprocess.run(command, **kwargs, **options)
