#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""The C++ files a change touches, compiled with the build root's GCC.

The Linux release builds in an Ubuntu 24.04 build root
(tools/dev-env/build-root.sh, docs/development.md "Linux on a newer
host"), whose GCC is older than a newer host's. Code the host's compiler
accepts can fail there: 0.53.0-rc1 and 0.54.0-rc1 failed on an OCCT handle
of a derived class copy-initialized or returned as a handle of its base,
which GCC 15 accepts and GCC 13 refuses (mitcad#1551). check-all's
root-compile step runs this script, so that such code fails the scoped
check before a merge rather than the release.

Each selected file is compiled with -fsyntax-only by the root's compiler,
inside the root (build-root.sh run), with the flags of the dev build's
compile_commands.json: the same tree, its generated headers and the
build's own dependencies, only the compiler and its system headers are the
root's. Include folders outside /usr, /etc and /var (vcpkg's packages in
the build folder, a Qt installation, the render prefix) are bound into
the root at their real paths. An error or a warning fails the file: the
release's build counts warnings as failures too.

Which files:
- by default the change's: the C and C++ files that differ from the merge
  base with the nearest of origin/dev, origin/features and origin/main
  (the fewest commits from HEAD; --base REF instead), committed or not,
  and that have compile commands; for a changed header, every source file
  whose dependencies (ninja -t deps) name it;
- with --all every source file under geometry/, core/cpp/ and app/ that
  has compile commands.

It runs only where it can tell something: MITCAD_LINUX_BUILD_ROOT names the
root (--root), the build's compiler is GCC, and newer than the root's.
Otherwise it says why and exits with 77, which check-all shows as a
skipped row; so it skips on machines without a build root, on Windows and
macOS, and inside the root itself, where the build's compiler is the
root's (the release's own check-all).

  check-root-compile.py [--build DIR] [--root DIR] [--home DIR] [--all]
      [--base REF] [--jobs N] [--list]

Exit status: 0 every file compiled cleanly (or none was selected), 1 a
file failed, 2 a usage or setup error, 77 skipped.
"""

import argparse
import concurrent.futures
import glob
import json
import os
import re
import shlex
import shutil
import subprocess
import sys

SKIPPED = 77
SOURCE_SUFFIXES = (".c", ".cc", ".cpp", ".cxx")
HEADER_SUFFIXES = (".h", ".hh", ".hpp", ".hxx", ".inl", ".ipp", ".tpp")
# The folders --all compiles.
ALL_FOLDERS = ("geometry", "core/cpp", "app")
# The root's own folders: include paths there are the root's headers.
ROOT_FOLDERS = ("/usr/", "/etc/", "/var/")
# Flags that take a path, separately or attached.
PATH_FLAGS = ("-isystem", "-iquote", "-idirafter", "-include", "-imacros", "-I")
# A compiler's warning line, as check-all's build step counts them.
WARNING = re.compile(r"(^| )warning(\[[^]]*\])?: ")
CHECKOUT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUILD_ROOT_SH = os.path.join(CHECKOUT, "tools", "dev-env", "build-root.sh")


def skip(reason):
    print(f"skipped: {reason}")
    sys.exit(SKIPPED)


def fail_setup(message):
    print(f"error: {message}", file=sys.stderr)
    sys.exit(2)


def git(*args):
    result = subprocess.run(["git", "-C", CHECKOUT, *args], capture_output=True, text=True)
    return result.stdout if result.returncode == 0 else None


def version_tuple(text):
    return tuple(int(part) for part in re.findall(r"\d+", text or ""))


def host_compiler(build):
    """The build's C++ compiler as CMake found it: (id, version)."""
    files = glob.glob(os.path.join(build, "CMakeFiles", "*", "CMakeCXXCompiler.cmake"))
    if not files:
        fail_setup(f"no CMakeCXXCompiler.cmake in {build}/CMakeFiles (configure the build first)")
    text = open(max(files, key=os.path.getmtime), encoding="utf-8").read()

    def value(name):
        found = re.search(rf'set\({name} "([^"]*)"\)', text)
        return found.group(1) if found else ""

    return value("CMAKE_CXX_COMPILER_ID"), value("CMAKE_CXX_COMPILER_VERSION")


def resolve_in_root(root, path):
    """A path of the root's file system, its symbolic links followed within
    the root (c++ -> /etc/alternatives/c++ -> /usr/bin/g++ -> g++-13)."""
    for _ in range(40):
        host_path = root + path
        if not os.path.islink(host_path):
            return path
        target = os.readlink(host_path)
        path = target if target.startswith("/") else os.path.normpath(
            os.path.join(os.path.dirname(path), target))
    return path


def root_major(root):
    """The root's default GCC's major version from its c++ link's name
    (g++-13), without running it; None when the name does not tell."""
    found = re.search(r"-(\d+)$", os.path.basename(resolve_in_root(root, "/usr/bin/c++")))
    return int(found.group(1)) if found else None


def clean_env():
    """The caller's environment less what build-root.sh would pass into the
    root (MITCAD_* and the build parallelism), which a compile does not
    need and which build-root.sh reports when it is outside the binds."""
    passed = ("CMAKE_BUILD_PARALLEL_LEVEL", "CARGO_BUILD_JOBS", "VCPKG_MAX_CONCURRENCY")
    return {name: value for name, value in os.environ.items()
            if not name.startswith("MITCAD_") and name not in passed}


def in_root(root, home, binds, command, cwd):
    args = ["bash", BUILD_ROOT_SH, "run"]
    for folder in binds:
        args += ["--bind", folder]
    args += [root, home, *command]
    return subprocess.run(args, cwd=cwd, capture_output=True, text=True, env=clean_env())


def change_base(base):
    """The merge base the change is measured from: --base, else that with
    the nearest of origin/dev, origin/features and origin/main."""
    if base:
        merge = git("merge-base", base, "HEAD")
        if merge is None:
            fail_setup(f"no merge base of {base} and HEAD")
        return merge.strip(), base
    best = None
    for ref in ("origin/dev", "origin/features", "origin/main"):
        merge = git("merge-base", ref, "HEAD")
        if merge is None:
            continue
        merge = merge.strip()
        count = int((git("rev-list", "--count", f"{merge}..HEAD") or "0").strip() or 0)
        if best is None or count < best[0]:
            best = (count, merge, ref)
    if best is None:
        return None, None
    return best[1], best[2]


def changed_paths(merge):
    """The absolute paths of the files the change touches: committed since
    the merge base, changed in the working tree, and new untracked ones."""
    names = set()
    for args in (("diff", "--name-only", merge, "HEAD"), ("diff", "--name-only", "HEAD"),
                 ("ls-files", "--others", "--exclude-standard")):
        names.update(line for line in (git(*args) or "").splitlines() if line)
    return {os.path.join(CHECKOUT, name) for name in names
            if os.path.exists(os.path.join(CHECKOUT, name))}


def entry_argv(entry):
    if "arguments" in entry:
        return list(entry["arguments"])
    return shlex.split(entry["command"])


def entry_output(entry):
    if "output" in entry:
        return entry["output"]
    argv = entry_argv(entry)
    for index, arg in enumerate(argv[:-1]):
        if arg == "-o":
            return argv[index + 1]
    return None


def header_users(build, headers, entries):
    """The compile entries whose objects depend on one of the headers, from
    ninja's dependency log (ninja -t deps); None when it cannot be read."""
    result = subprocess.run(["ninja", "-C", build, "-t", "deps"], capture_output=True, text=True)
    if result.returncode != 0:
        return None
    by_output = {}
    for entry in entries:
        output = entry_output(entry)
        if output:
            by_output[os.path.normpath(os.path.join(entry["directory"], output))] = entry
    users = []
    current = None
    for line in result.stdout.splitlines():
        if not line:
            current = None
        elif not line[0].isspace():
            current = by_output.get(os.path.normpath(os.path.join(build, line.split(":", 1)[0])))
        elif current is not None:
            dependency = os.path.normpath(os.path.join(build, line.strip()))
            if dependency in headers:
                users.append(current)
                current = None
    return users


def root_command(entry):
    """The entry's command for the root: its compiler (the root's cc or c++,
    without a compiler cache in front), -fsyntax-only instead of an
    object, and include paths outside the root's own folders at their real
    paths; with the folders to bind for them."""
    argv = entry_argv(entry)
    while argv and os.path.basename(argv[0]) in ("ccache", "sccache"):
        argv = argv[1:]
    name = os.path.basename(argv[0])
    compiler = "cc" if re.search(r"(^|-)(gcc|cc)(-\d+)?$", name) else "c++"
    command = [compiler, "-fsyntax-only"]
    folders = set()

    def outside(path):
        if not os.path.isabs(path) or path.startswith(ROOT_FOLDERS):
            return path
        real = os.path.realpath(path)
        folder = real if os.path.isdir(real) else os.path.dirname(real)
        if os.path.isdir(folder):
            folders.add(folder)
        return real

    args = argv[1:]
    index = 0
    while index < len(args):
        arg = args[index]
        index += 1
        if arg == "-o" and index < len(args):
            index += 1
            continue
        if arg in ("-MD", "-MMD"):
            continue
        if arg in ("-MF", "-MT", "-MQ") and index < len(args):
            index += 1
            continue
        if arg in PATH_FLAGS and index < len(args):
            command += [arg, outside(args[index])]
            index += 1
            continue
        flag = next((flag for flag in PATH_FLAGS if arg.startswith(flag) and len(arg) > len(flag)), None)
        if flag:
            command.append(flag + outside(arg[len(flag):]))
            continue
        command.append(arg)
    return command, folders


def collapse(folders):
    """The folders less those inside another one of them."""
    kept = []
    for folder in sorted(folders):
        if not any(folder == other or folder.startswith(other + "/") for other in kept):
            kept.append(folder)
    return kept


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--build", default=os.path.join(CHECKOUT, "build", "dev"))
    parser.add_argument("--root", default=os.environ.get("MITCAD_LINUX_BUILD_ROOT", ""))
    parser.add_argument("--home", help="the root's home folder (default: <build>/root-compile/home)")
    parser.add_argument("--all", action="store_true", help="every source file of " + ", ".join(ALL_FOLDERS))
    parser.add_argument("--base", help="measure the change from the merge base with this ref")
    parser.add_argument("--jobs", type=int, default=int(
        os.environ.get("CMAKE_BUILD_PARALLEL_LEVEL") or max(1, (os.cpu_count() or 2) // 2)))
    parser.add_argument("--list", action="store_true", help="list the files and compile nothing")
    options = parser.parse_args()
    build = os.path.realpath(options.build)

    # Where it can tell something.
    if not options.root:
        skip("no build root (MITCAD_LINUX_BUILD_ROOT is not set; docs/development.md, The check script)")
    root = os.path.realpath(options.root)
    if not os.access(os.path.join(root, "usr", "bin", "apt-get"), os.X_OK):
        skip(f"no build root at {options.root} (MITCAD_LINUX_BUILD_ROOT)")
    compile_commands = os.path.join(build, "compile_commands.json")
    if not os.path.isfile(compile_commands):
        fail_setup(f"no {compile_commands} (configure and build first)")
    compiler_id, host_version = host_compiler(build)
    if compiler_id != "GNU":
        skip(f"the build's compiler is {compiler_id or 'unknown'} {host_version}, not GCC")
    major = root_major(root)
    if major is not None and version_tuple(host_version)[:1] <= (major,):
        skip(f"the build's compiler, GCC {host_version}, is not newer than the build root's GCC {major}")
    if shutil.which("bwrap") is None:
        skip("bwrap (bubblewrap), which enters the build root, is not installed")
    home = os.path.realpath(options.home or os.path.join(build, "root-compile", "home"))
    os.makedirs(home, exist_ok=True)
    asked = in_root(root, home, [], ["c++", "-dumpfullversion"], home)
    if asked.returncode != 0:
        print(f"the build root {root} does not run its compiler:", asked.stderr.strip(), sep="\n")
        return 1
    root_version = asked.stdout.strip()
    if version_tuple(host_version) <= version_tuple(root_version):
        skip(f"the build's compiler, GCC {host_version}, is not newer than the build root's GCC {root_version}")

    # The files.
    entries = json.load(open(compile_commands, encoding="utf-8"))
    for entry in entries:
        entry["file"] = os.path.normpath(os.path.join(entry["directory"], entry["file"]))
    if options.all:
        folders = tuple(os.path.join(CHECKOUT, folder) + "/" for folder in ALL_FOLDERS)
        selected = [entry for entry in entries if entry["file"].startswith(folders)]
        what = "every source file of " + ", ".join(f"{folder}/" for folder in ALL_FOLDERS)
    else:
        merge, ref = change_base(options.base)
        if merge is None:
            skip("no merge base with origin/dev, origin/features or origin/main to measure the change from")
        changed = changed_paths(merge)
        sources = {path for path in changed if path.endswith(SOURCE_SUFFIXES)}
        headers = {path for path in changed if path.endswith(HEADER_SUFFIXES)}
        selected = [entry for entry in entries if entry["file"] in sources]
        what = f"the change's C and C++ files since the merge base with {ref} ({merge[:10]})"
        if headers:
            users = header_users(build, headers, entries)
            if users is None:
                print(f"note: ninja -t deps could not be read in {build}: "
                      f"the sources that include the {len(headers)} changed headers are not compiled")
            else:
                selected += users
                what += f", with the sources that include its {len(headers)} changed headers"
    unique = {}
    for entry in selected:
        unique.setdefault(entry["file"], entry)
    selected = [unique[path] for path in sorted(unique)]
    print(f"compiling {len(selected)} files with the build root's GCC {root_version} "
          f"(the build's is GCC {host_version}), -fsyntax-only, {options.jobs} at once: {what}")
    if options.list:
        for entry in selected:
            print("  " + os.path.relpath(entry["file"], CHECKOUT))
        return 0
    if not selected:
        print("nothing to compile")
        return 0

    # The compiles, each in the root with its folders bound.
    def compile_one(entry):
        command, folders = root_command(entry)
        folders.update({os.path.realpath(CHECKOUT), build, os.path.realpath(entry["directory"])})
        result = in_root(root, home, collapse(folders), command, entry["directory"])
        output = (result.stdout + result.stderr).strip()
        warns = any(WARNING.search(line) for line in output.splitlines())
        return entry, result.returncode, warns, output

    failed = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, options.jobs)) as pool:
        for entry, status, warns, output in pool.map(compile_one, selected):
            name = os.path.relpath(entry["file"], CHECKOUT)
            if status != 0 or warns:
                failed.append(name)
                verdict = "errors" if status != 0 else "warnings"
                lines = output.splitlines()
                print(f"---- {name}: {verdict} with GCC {root_version}")
                print("\n".join(lines[:60]))
                if len(lines) > 60:
                    print(f"... ({len(lines) - 60} more lines)")
    if failed:
        print(f"FAILED: {len(failed)} of {len(selected)} files with the build root's GCC {root_version}: "
              + " ".join(failed))
        return 1
    print(f"ok: {len(selected)} files compile cleanly with the build root's GCC {root_version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
