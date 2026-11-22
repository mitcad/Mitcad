#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""What each Linux UI test uses of the test library and of the UI test driver.

The scoped check (tools/check-all.sh, `--ui auto`) reruns a UI test when a
file it depends on changed (docs/development.md, "UI tests by what
changed"). Two of those files are shared by hundreds of tests and change in
most bundles; this helper narrows them to what each test uses (mitcad#2173):

- The test library (tools/ui-test-lib.sh): a test depends on the library's
  top-level code and on the functions it calls, directly or through other
  functions. A function counts when its name appears as a word in the test's
  material (below), in the top-level code or in a function that counts, so a
  function passed by name (`ui_step "..." ui_key l`) counts too. The other
  library files (ui-image-stats.py, xwd2png.py, ui-visible-text-allow.txt)
  count when that text names them.
- The UI test driver: a test that declares app/framework/TestDriver.cpp
  depends on TestDriver.cpp (the parser, the input, the common steps),
  TestDriverSupport.hpp, and the area files (app/framework/TestDriver*.cpp)
  whose steps its material names. An area file's steps are the names of its
  spec table (`{"step-name", min, max, TestDriver::k...}`). A step name
  written with a variable (`expect-view-$kind`) counts every step it can
  stand for.

A test's material is its script and the files it names, followed through
the files they name: the scripts and fixtures under tools/ui-scripts/
(`ui-scripts/<name>`, a $variable in the name standing for any text and every
name taken as a prefix) and the files of tools/ named by their name
(`ui-timber-lib.sh`, `ui-chat-test.py`, other tests' scripts). The library
itself is not material: its functions count as above.

Usage:
  ui-test-deps.py plan [--root DIR]   one line per Linux UI test:
                                      name, library digest, driver files
  ui-test-deps.py --self-test [--work DIR] [--repo DIR]
                                      checks itself on a small tree it writes
                                      in DIR (default build/ui-test-deps-self-test;
                                      overwritten, never removed), and with
                                      --repo a checkout's driver files
                                      (repo_check; the ctest repo.ui-test-deps)
"""
import argparse
import glob
import hashlib
import os
import re
import sys

LIBRARY = 'tools/ui-test-lib.sh'
LIBRARY_FILES = ['tools/ui-image-stats.py', 'tools/xwd2png.py', 'tools/ui-visible-text-allow.txt']
DRIVER = 'app/framework/TestDriver.cpp'
DRIVER_CORE = ['app/framework/TestDriver.cpp', 'app/framework/TestDriverSupport.hpp']
DRIVER_AREAS = 'app/framework/TestDriver?*.cpp'

FUNCTION_START = re.compile(r'^([A-Za-z_][A-Za-z0-9_]*)\(\)\s*\{')
SPEC = re.compile(r'^\s*\{"([a-z0-9-]+)",\s*-?\d+,\s*-?\d+,\s*TestDriver::k', re.M)
WORD = re.compile(r'[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)*')
# A step name's start before a variable ("expect-view-$kind") and its end
# after one ("$kind-pixel").
PREFIX = re.compile(r'(?<![A-Za-z0-9_-])([a-z][a-z0-9]*(?:-[a-z0-9]+)*-)\$')
SUFFIX = re.compile(r'\$\{?[A-Za-z0-9_]+\}?(-[a-z0-9]+(?:-[a-z0-9]+)*)(?![A-Za-z0-9_-])')
SCRIPT_REF = re.compile(r'ui-scripts/([A-Za-z0-9_.${}*/-]+)')
# The folder itself named (a path made of it later): all of it.
FOLDER_REF = re.compile(r'ui-scripts(?!/?[A-Za-z0-9_.${*-])')
TOOL_REF = re.compile(r'([A-Za-z0-9_.${}-]+\.(?:sh|py))(?![A-Za-z0-9_])')
VARIABLE = re.compile(r'\$\{[^}]*\}|\$[A-Za-z0-9_]+')


def parse_library(text):
    """The library's top-level code and its functions: (top, {name: text}).
    The comment lines right above a function belong to it."""
    top = []
    functions = {}
    comments = []
    lines = text.split('\n')
    i = 0
    while i < len(lines):
        line = lines[i]
        match = FUNCTION_START.match(line)
        if match:
            body = [line]
            if line.rstrip().endswith('{'):
                i += 1
                while i < len(lines) and not lines[i].startswith('}'):
                    body.append(lines[i])
                    i += 1
                if i < len(lines):
                    body.append(lines[i])
            functions[match.group(1)] = '\n'.join(comments + body)
            comments = []
        elif line.startswith('#'):
            comments.append(line)
        else:
            top.extend(comments)
            comments = []
            top.append(line)
        i += 1
    top.extend(comments)
    return '\n'.join(top), functions


class Tree:
    """The files of a checkout, read once."""

    def __init__(self, root):
        self.root = root
        self.texts = {}
        self.words = {}
        self.refs = {}

    def path(self, rel):
        return os.path.join(self.root, rel)

    def text(self, rel):
        if rel not in self.texts:
            try:
                with open(self.path(rel), encoding='utf-8', errors='replace') as f:
                    self.texts[rel] = f.read()
            except OSError:
                self.texts[rel] = ''
        return self.texts[rel]

    def word_set(self, rel):
        if rel not in self.words:
            self.words[rel] = set(WORD.findall(self.text(rel)))
        return self.words[rel]

    def glob(self, pattern):
        """The files matching, as paths relative to the root with "/" on
        every system (the names the rest compares with, such as DRIVER)."""
        found = []
        for path in glob.glob(self.path(pattern)):
            if os.path.isfile(path):
                found.append(os.path.relpath(path, self.root).replace(os.sep, '/'))
        return found


def references(tree, rel):
    """The files of tools/ that the file `rel` names."""
    if rel in tree.refs:
        return tree.refs[rel]
    text = tree.text(rel)
    found = set()
    refs = SCRIPT_REF.findall(text)
    if FOLDER_REF.search(text):
        refs.append('*')
    for ref in refs:
        pattern = VARIABLE.sub('*', ref).rstrip('.')
        for path in tree.glob('tools/ui-scripts/' + pattern + '*'):
            if not path.endswith('.json'):
                found.add(path)
    for ref in TOOL_REF.findall(text):
        pattern = VARIABLE.sub('*', ref)
        if pattern.strip('*.') in ('sh', 'py', ''):
            continue
        found.update(tree.glob('tools/' + pattern))
    found.discard(LIBRARY)
    tree.refs[rel] = found
    return found


def material(tree, test):
    """The test's script and the files it names, followed through them."""
    start = 'tools/ui-%s-test.sh' % test
    seen = {start}
    todo = [start]
    while todo:
        rel = todo.pop()
        for path in sorted(references(tree, rel)):
            if path not in seen:
                seen.add(path)
                todo.append(path)
    return sorted(seen)


class Library:
    def __init__(self, tree):
        self.tree = tree
        self.top, self.functions = parse_library(tree.text(LIBRARY))
        self.top_words = set(WORD.findall(self.top))
        self.function_words = {name: set(WORD.findall(text)) for name, text in self.functions.items()}

    def reachable(self, words):
        """The functions the words name, and those they name in turn."""
        names = set(self.functions)
        found = (words | self.top_words) & names
        todo = list(found)
        while todo:
            name = todo.pop()
            for other in self.function_words[name] & names:
                if other not in found:
                    found.add(other)
                    todo.append(other)
        return found

    def digest(self, words, texts):
        functions = sorted(self.reachable(words))
        digest = hashlib.md5()
        digest.update(b'top\n' + self.top.encode())
        for name in functions:
            digest.update(('\nfunction %s\n' % name).encode() + self.functions[name].encode())
        named = '\n'.join([self.top] + [self.functions[name] for name in functions] + texts)
        for rel in LIBRARY_FILES:
            if os.path.basename(rel) in named and os.path.exists(self.tree.path(rel)):
                with open(self.tree.path(rel), 'rb') as f:
                    digest.update(('\nfile %s\n' % rel).encode() + f.read())
        return digest.hexdigest(), functions


class Driver:
    def __init__(self, tree):
        self.steps = {}  # step name -> area file
        for rel in sorted(tree.glob(DRIVER_AREAS)):
            if rel == DRIVER:
                continue
            for name in SPEC.findall(tree.text(rel)):
                self.steps.setdefault(name, rel)

    def files(self, words, text):
        used = set()
        for word in words:
            if word in self.steps:
                used.add(self.steps[word])
        for prefix in PREFIX.findall(text):
            used.update(rel for name, rel in self.steps.items() if name.startswith(prefix))
        for suffix in SUFFIX.findall(text):
            used.update(rel for name, rel in self.steps.items() if name.endswith(suffix))
        return DRIVER_CORE + sorted(used)


def plan(root, out):
    tree = Tree(root)
    library = Library(tree)
    driver = Driver(tree)
    for script in sorted(tree.glob('tools/ui-*-test.sh')):
        test = os.path.basename(script)[len('ui-'):-len('-test.sh')]
        if test.startswith('macos'):
            continue
        files = material(tree, test)
        words = set()
        for rel in files:
            words |= tree.word_set(rel)
        texts = [tree.text(rel) for rel in files]
        digest, functions = library.digest(words, texts)
        # The driver's steps in the material and in the library code it runs.
        lib_text = '\n'.join([library.top] + [library.functions[name] for name in functions])
        driver_words = words | set(WORD.findall(lib_text))
        driver_files = driver.files(driver_words, '\n'.join(texts + [lib_text]))
        out.write('%s\t%s\t%s\n' % (test, digest, ' '.join(driver_files)))


def write(root, rel, text):
    path = os.path.join(root, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w', encoding='utf-8') as f:
        f.write(text)


def self_test(root):
    root = os.path.abspath(root)
    for rel in ('tools/ui-test-lib.sh', 'tools/xwd2png.py', 'tools/ui-image-stats.py',
                'tools/ui-visible-text-allow.txt', 'tools/ui-helper-lib.sh', 'tools/ui-scripts/a.mitcad-ui',
                'tools/ui-scripts/b.mitcad-ui', 'tools/ui-scripts/b.json', 'tools/ui-one-test.sh',
                'tools/ui-two-test.sh', 'tools/ui-three-test.sh', 'tools/ui-macos-four-test.sh',
                'app/framework/TestDriver.cpp', 'app/framework/TestDriverSupport.hpp',
                'app/framework/TestDriverSketch.cpp', 'app/framework/TestDriverViewPixels.cpp',
                'app/framework/TestDriverModel.cpp'):
        write(root, rel, '')
    write(root, 'tools/ui-test-lib.sh', '\n'.join([
        '# The library.',
        'UI_LOG=x',
        'trap ui_cleanup EXIT',
        '# ui_cleanup: at the exit.',
        'ui_cleanup() {',
        '  :',
        '}',
        'ui_at() { echo 1; }',
        '# ui_key key: a key.',
        'ui_key() {',
        '  ui_at 1 2',
        '}',
        'ui_shot() {',
        '  python3 "$here/xwd2png.py"',
        '}',
        'ui_drive() {',
        '  echo "expect-model-busy no" > "$UI_DIR/s"',
        '}',
        '']))
    write(root, 'app/framework/TestDriver.cpp', '{"key", 1, -1, TestDriver::kInputOfTarget},\n')
    write(root, 'app/framework/TestDriverSketch.cpp',
          '    {"sketch-click", 2, 2, TestDriver::kInputOfMain},\n'
          '    {"expect-sketch-text", 1, 1, TestDriver::kIdle},\n'
          '      {"sideways", 0, 30, 0},\n')
    write(root, 'app/framework/TestDriverViewPixels.cpp',
          '    {"capture-view-pixel", 3, 3, TestDriver::kIdle},\n'
          '    {"expect-view-pixel", 3, 4, TestDriver::kIdle},\n')
    write(root, 'app/framework/TestDriverModel.cpp', '    {"expect-model-busy", 1, 1, TestDriver::kPlain},\n')
    write(root, 'tools/ui-scripts/a.mitcad-ui', 'sketch-click 0 0\nkey Return\n')
    write(root, 'tools/ui-scripts/b.mitcad-ui', '# sideways\nexpect-view-pixel 1 2 #fff\n')
    write(root, 'tools/ui-helper-lib.sh', 'helper() { ui_key a; }\n')
    write(root, 'tools/ui-one-test.sh',
          '# check-all sources: app/framework/TestDriver.cpp\n'
          'source "$(dirname "$0")/ui-test-lib.sh"\n'
          'export MITCAD_TEST_INPUT=$here/ui-scripts/a.mitcad-ui\n')
    write(root, 'tools/ui-two-test.sh',
          'source "$here/ui-test-lib.sh"\n'
          '. "$here/ui-helper-lib.sh"\n'
          'run() { MITCAD_TEST_INPUT=$here/ui-scripts/$1.mitcad-ui ui_shot; }\n')
    write(root, 'tools/ui-three-test.sh',
          'source "$here/ui-test-lib.sh"\n'
          'for what in pixel; do echo "capture-view-$what 1 2 x"; done\n'
          'ui_drive\n')
    import io
    out = io.StringIO()
    plan(root, out)
    rows = {}
    for line in out.getvalue().splitlines():
        name, digest, files = line.split('\t')
        rows[name] = (digest, files.split())
    failures = []

    def expect(what, condition):
        if not condition:
            failures.append(what)

    expect('the macOS tests are left out', 'macos-four' not in rows)
    expect('three Linux tests', sorted(rows) == ['one', 'three', 'two'])
    core = ['app/framework/TestDriver.cpp', 'app/framework/TestDriverSupport.hpp']
    expect('one: its script\'s sketch step\'s area (its key step is the core\'s)',
           rows['one'][1] == core + ['app/framework/TestDriverSketch.cpp'])
    expect('two: every script of ui-scripts/$1 (a and b), not the other table\'s "sideways"',
           rows['two'][1] == core + ['app/framework/TestDriverSketch.cpp', 'app/framework/TestDriverViewPixels.cpp'])
    expect('three: capture-view-$what stands for capture-view-pixel; the library function\'s step counts',
           rows['three'][1] == core + ['app/framework/TestDriverModel.cpp', 'app/framework/TestDriverViewPixels.cpp'])
    tree = Tree(root)
    library = Library(tree)
    expect('the library\'s functions', sorted(library.functions) == ['ui_at', 'ui_cleanup', 'ui_drive', 'ui_key',
                                                                      'ui_shot'])
    expect('a comment above a function is the function\'s', '# ui_key key' in library.functions['ui_key'])
    expect('the top-level code', 'UI_LOG=x' in library.top and 'ui_key()' not in library.top)
    one = set(WORD.findall(tree.text('tools/ui-one-test.sh')))
    expect('one reaches ui_cleanup through the trap only', library.reachable(one) == {'ui_cleanup'})
    two = set()
    for rel in material(tree, 'two'):
        two |= tree.word_set(rel)
    expect('two reaches ui_key and ui_at through its helper library, ui_shot directly',
           library.reachable(two) == {'ui_cleanup', 'ui_key', 'ui_at', 'ui_shot'})
    expect('the material of two', material(tree, 'two') == [
        'tools/ui-helper-lib.sh', 'tools/ui-scripts/a.mitcad-ui', 'tools/ui-scripts/b.mitcad-ui',
        'tools/ui-two-test.sh'])
    # A change in a function only the second test reaches changes only its digest.
    before = {name: row[0] for name, row in rows.items()}
    write(root, 'tools/ui-test-lib.sh', tree.text('tools/ui-test-lib.sh').replace('ui_at 1 2', 'ui_at 1 3'))
    write(root, 'tools/xwd2png.py', '# changed\n')
    out = io.StringIO()
    plan(root, out)
    after = {line.split('\t')[0]: line.split('\t')[1] for line in out.getvalue().splitlines()}
    expect('a change in ui_key and xwd2png.py changes two only',
           after['one'] == before['one'] and after['three'] == before['three'] and after['two'] != before['two'])
    write(root, 'tools/ui-test-lib.sh', Tree(root).text('tools/ui-test-lib.sh').replace('UI_LOG=x', 'UI_LOG=y'))
    out = io.StringIO()
    plan(root, out)
    changed = {line.split('\t')[0]: line.split('\t')[1] for line in out.getvalue().splitlines()}
    expect('a change in the top-level code changes all',
           all(changed[name] != after[name] for name in ('one', 'two', 'three')))
    for failure in failures:
        print('FAIL ' + failure)
    print('ui-test-deps self-test: %s' % ('FAILED' if failures else 'ok'))
    return 1 if failures else 0


def repo_check(root):
    """The checkout's driver files: each step in one file's spec table, every
    area file's step a name with a hyphen (a plain word such as `workspace`
    is in most scripts' text, so it would count that area for nearly every
    test: such a step stays in TestDriver.cpp), and a plan for every test."""
    tree = Tree(root)
    failures = []
    owner = {}
    for rel in sorted(tree.glob(DRIVER_AREAS)) + [DRIVER]:
        for name in SPEC.findall(tree.text(rel)):
            if name in owner and owner[name] != rel:
                failures.append('step %s is in %s and %s' % (name, owner[name], rel))
            owner.setdefault(name, rel)
            if rel != DRIVER and '-' not in name:
                failures.append('step %s of %s is a plain word: keep it in %s' % (name, rel, DRIVER))
    if not any(rel == DRIVER for rel in owner.values()):
        failures.append('no step spec found in %s' % DRIVER)
    import io
    out = io.StringIO()
    plan(root, out)
    rows = out.getvalue().splitlines()
    if not rows or any(len(row.split('\t')) != 3 for row in rows):
        failures.append('the plan of %s has no rows or a row without its three fields' % root)
    for failure in failures:
        print('FAIL ' + failure)
    print('ui-test-deps repo check: %s (%d steps, %d tests)' % ('FAILED' if failures else 'ok', len(owner), len(rows)))
    return 1 if failures else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('command', nargs='?', choices=['plan'])
    parser.add_argument('--root', default='.')
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('--work', default=os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'build',
                                                       'ui-test-deps-self-test'),
                        help='the folder the self-test writes its tree in (overwritten, never removed)')
    parser.add_argument('--repo', help='with --self-test: also check this checkout (repo_check)')
    args = parser.parse_args()
    if args.self_test:
        status = self_test(args.work)
        if args.repo:
            status |= repo_check(os.path.abspath(args.repo))
        return status
    if args.command != 'plan':
        parser.error('plan or --self-test')
    plan(os.path.abspath(args.root), sys.stdout)
    return 0


if __name__ == '__main__':
    sys.exit(main())
