# SPDX-License-Identifier: MIT
"""The version tags of the library generators (mitcad#1372).

With --commit, make-parts-library.py, make-fastener-library.py and
make-profile-library.py record the library in the git repository in --out
and tag it v<version>. A version that has been tagged is published (or about
to be): its tag is never moved to other contents. So each generator refuses
a --version whose tag the --out repository already has, before it makes
anything (2.10.0 was once tagged without the tubes meant for it, and a
generator run with the same default would have tagged it again).
"""

import os
import subprocess
import sys


def tag_of(version):
    """The tag of a library version: v<version>."""
    return f"v{version}"


def tag_exists(out, version):
    """Whether the git repository in `out` has the tag v<version>. False
    for a folder that is no repository of its own (or is not there yet), so
    that a folder inside another repository is not judged by that
    repository's tags. Raises RuntimeError when git cannot tell."""
    if not os.path.exists(os.path.join(out, ".git")):
        return False
    tag = tag_of(version)
    try:
        # The exact name: --list takes a pattern, the lines are compared.
        result = subprocess.run(["git", "-C", out, "tag", "--list", tag], capture_output=True, text=True,
                                encoding="utf-8", errors="replace")
    except OSError as e:
        raise RuntimeError(f"git could not be run to read the tags of {out} ({e})") from e
    if result.returncode != 0:
        raise RuntimeError(f"git could not read the tags of {out}: {result.stderr.strip()}")
    return tag in (line.strip() for line in result.stdout.splitlines())


def refuse_existing_tag(out, version, program):
    """Stops `program` with a message naming the tag when the repository in
    `out` already has the tag of `version` (or git cannot tell)."""
    try:
        exists = tag_exists(out, version)
    except RuntimeError as e:
        sys.exit(f"{program}: {e}")
    if exists:
        sys.exit(f"{program}: {out} already has the tag {tag_of(version)}: version {version} is already "
                 f"recorded and is never tagged again; give the next version with --version (nothing was made)")
