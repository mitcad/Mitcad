#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Keeps the AppKit frontend from silently losing what the shared Qt UI
shows (docs/native-macos-ui.md, Coverage).

The AppKit frontend presents the shared Qt widgets through a generic bridge
(renderWidget in app/platform/macos/NativeDialogs.mm) that maps Qt's own
controls to AppKit ones. Two things escape it:

- a widget that paints itself (overrides paintEvent): the bridge knows only
  its children, so its picture is lost unless a native view presents it.
  Each such class of app/ has a line in the coverage list
  (app/platform/macos/native-coverage.txt) saying how the AppKit frontend
  shows it;
- a dialog, message box or menu that Qt shows itself (QDialog::exec,
  QMessageBox::warning, QFileDialog::getOpenFileName, QMenu::exec, ...)
  instead of the shared dispatch of app/framework/Dialogs.hpp (execDialog,
  showDialog, execMenu, sheetWarning, dialogOpenFileName, ...), which hands
  it to the AppKit presenter.

  check-native-coverage.py APP LIST    scans the .cpp, .hpp, .h and .mm files
                                       under APP, less platform/ and tests/,
                                       against the coverage list LIST; prints
                                       every problem as file:line and fails
                                       if any
  check-native-coverage.py --self-test checks the scan on planted sources

The scan reads the source text, not the compiled program, with comments and
literals left out. A class is a `class` or `struct` with a body; a painting
class declares `paintEvent(QPaintEvent...)` in its body, or its file defines
`Class::paintEvent(QPaintEvent...)`. A call's receiver has the type of its
nearest declaration before the call in the file (or, for a member, in the
file's header); a receiver of unknown type is not reported. Widgets painted
through an event filter or a delegate are not seen.

A line of the list is `Class status detail`:

  native SYMBOL     an AppKit view presents it; SYMBOL names that view's
                    code in the AppKit frontend (app/platform/macos,
                    app/MainWindowNative.cpp)
  replaced SYMBOL   the Qt frontend's own chrome; the AppKit frontend shows
                    SYMBOL (in the same code) in its place
  shared REASON     Qt draws it in both frontends (the OCCT view and what is
                    drawn over it)
  pending REASON    the AppKit frontend does not present it yet

The native UI tests (tools/ui-macos-lib.sh) fail when the bridge logs a
widget it could not present, unless the list marks that class pending.
"""

import os
import re
import sys
import tempfile

SOURCE_SUFFIXES = (".cpp", ".hpp", ".h", ".mm")
SKIPPED_DIRS = ("platform", "tests")
# The shared dispatch itself.
DISPATCH_FILES = ("framework/Dialogs.cpp",)
# The AppKit frontend's code outside app/platform/macos.
NATIVE_FILES = ("MainWindowNative.cpp",)
STATUSES = ("native", "replaced", "shared", "pending")
SYMBOL_STATUSES = ("native", "replaced")

# Qt's classes that open a window or popup of their own when executed.
QT_DIALOGS = {
    "QDialog", "QMessageBox", "QFileDialog", "QInputDialog", "QColorDialog",
    "QFontDialog", "QProgressDialog", "QErrorMessage", "QWizard", "QMenu",
}
STATIC_DIALOGS = re.compile(
    r"\b(QMessageBox\s*::\s*(?:warning|information|question|critical|about|aboutQt)"
    r"|QFileDialog\s*::\s*get\w+"
    r"|QInputDialog\s*::\s*get\w+"
    r"|QColorDialog\s*::\s*getColor"
    r"|QFontDialog\s*::\s*getFont)\s*\(")
DISPATCH_OF = {
    "QMessageBox": "sheetWarning, sheetInformation or sheetQuestion",
    "QFileDialog": "dialogOpenFileName, dialogSaveFileName or dialogDirectory",
    "QInputDialog": "dialogText or dialogItem",
    "QColorDialog": "dialogColor",
    "QFontDialog": "execDialog",
}
CLASS_HEAD = re.compile(
    r"\b(?<!enum\s)(?:class|struct)\s+(?:[A-Z][A-Z0-9_]*\s+)?(\w+)\s*(?:final\s*)?(:[^{;()]*)?\{")
PAINT_DECLARATION = re.compile(r"\bpaintEvent\s*\(\s*QPaintEvent\b")
PAINT_DEFINITION = re.compile(r"\b(\w+)\s*::\s*paintEvent\s*\(\s*QPaintEvent\b")
EXEC_CALL = re.compile(r"\b(\w+)\s*(?:\.|->)\s*exec\s*\(")
NOT_TYPES = {
    "return", "else", "case", "new", "delete", "throw", "co_return", "goto",
    "sizeof", "operator", "public", "private", "protected", "using", "typedef",
}


def strip(text, literals=False):
    """The text with comments blanked out, and string and character literals
    unless `literals`, lines kept."""
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            end = text.find("\n", i)
            end = n if end < 0 else end
            out.append(" " * (end - i))
            i = end
        elif text.startswith("/*", i):
            end = text.find("*/", i + 2)
            end = n if end < 0 else end + 2
            out.append(re.sub(r"[^\n]", " ", text[i:end]))
            i = end
        elif c == "R" and text.startswith('R"', i) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            open_paren = text.find("(", i + 2)
            delimiter = text[i + 2:open_paren]
            end = text.find(")" + delimiter + '"', open_paren)
            end = n if open_paren < 0 or end < 0 else end + len(delimiter) + 2
            out.append(re.sub(r"[^\n]", " ", text[i:end]))
            i = end
        elif c in "\"'":
            j = i + 1
            while j < n and text[j] != c and text[j] != "\n":
                j += 2 if text[j] == "\\" else 1
            j = min(j, n)
            closed = j < n and text[j] == c
            out.append(text[i:j] if literals else c + " " * (j - i - 1))
            out.append(c if closed else "")
            i = j + 1 if closed else j
        else:
            out.append(c)
            i += 1
    return "".join(out)


# Preprocessor names whose value in the AppKit frontend's build is known.
DEFINED_ON_MAC = {"Q_OS_MACOS", "Q_OS_MAC", "Q_OS_DARWIN", "__APPLE__", "MITCAD_MACOS_NATIVE_UI"}
UNDEFINED_ON_MAC = {"Q_OS_WIN", "Q_OS_WINDOWS", "Q_OS_WIN32", "Q_OS_WIN64", "_WIN32", "_MSC_VER",
                    "Q_OS_LINUX", "__linux__", "Q_OS_ANDROID"}
DIRECTIVE = re.compile(r"^\s*#\s*(if|ifdef|ifndef|elif|else|endif)\b(.*)$")


def on_mac(condition):
    """Whether the condition holds in the AppKit frontend's build: True,
    False, or None when the scan cannot tell."""
    for operator, all_needed in (("||", False), ("&&", True)):
        if operator in condition:
            values = [on_mac(part) for part in condition.split(operator)]
            if all_needed:
                return False if False in values else True if all(v is True for v in values) else None
            return True if True in values else False if all(v is False for v in values) else None
    m = re.fullmatch(r"\s*(!?)\s*(?:defined\s*\(?\s*(\w+)\s*\)?|(\w+))\s*", condition)
    if not m:
        return None
    name = m.group(2) or m.group(3)
    value = True if name in DEFINED_ON_MAC else False if name in UNDEFINED_ON_MAC else None
    return None if value is None else value != bool(m.group(1))


def mac_build(code):
    """The code with the lines blanked out that the AppKit frontend's build
    leaves out (#ifdef Q_OS_WIN, the #else of #ifdef Q_OS_MACOS, ...)."""
    lines = code.split("\n")
    frames = []  # [outer active, a branch holds, the current branch is active]
    active = True
    for i, line in enumerate(lines):
        m = DIRECTIVE.match(line)
        if m:
            kind, rest = m.group(1), m.group(2)
            if kind in ("if", "ifdef", "ifndef"):
                value = on_mac(rest if kind == "if" else ("!" if kind == "ifndef" else "") + rest)
                frames.append([active, value is True])
                active = active and value is not False
            elif frames and kind == "elif":
                outer, held = frames[-1]
                value = None if held else on_mac(rest)
                active = outer and not held and value is not False
                frames[-1][1] = held or value is True
            elif frames and kind == "else":
                outer, held = frames[-1]
                active = outer and not held
                frames[-1][1] = True
            elif frames and kind == "endif":
                active = frames.pop()[0]
            continue
        if not active:
            lines[i] = " " * len(line)
    return "\n".join(lines)


class Source:
    def __init__(self, rel, text):
        self.rel = rel
        self.code = mac_build(strip(text))
        self.starts = [0] + [m.end() for m in re.finditer("\n", self.code)]

    def line(self, position):
        lo, hi = 0, len(self.starts)
        while hi - lo > 1:
            mid = (lo + hi) // 2
            if self.starts[mid] <= position:
                lo = mid
            else:
                hi = mid
        return lo + 1


def matching_brace(code, open_at):
    depth = 0
    for i in range(open_at, len(code)):
        if code[i] == "{":
            depth += 1
        elif code[i] == "}":
            depth -= 1
            if depth == 0:
                return i
    return len(code)


def base_names(clause):
    names = []
    for part in (clause or "").lstrip(":").split(","):
        part = re.sub(r"<.*>", "", part)
        words = re.findall(r"\w+", part)
        if words:
            names.append(words[-1])
    return names


class Scan:
    def __init__(self):
        self.classes = {}      # name -> (rel, line, bases)
        self.painting = {}     # name -> (rel, line)
        self.problems = []     # (rel, line, text)

    def classes_of(self, source):
        found = []
        for m in CLASS_HEAD.finditer(source.code):
            name = m.group(1)
            open_at = m.end() - 1
            found.append((name, open_at, matching_brace(source.code, open_at), base_names(m.group(2))))
            self.classes.setdefault(name, (source.rel, source.line(m.start()), base_names(m.group(2))))
        return found

    def paint_declarations(self, source, scopes):
        for m in PAINT_DECLARATION.finditer(source.code):
            inner = [s for s in scopes if s[1] < m.start() < s[2]]
            if inner:
                name = max(inner, key=lambda s: s[1])[0]
                self.painting.setdefault(name, (source.rel, source.line(m.start())))

    def paint_definitions(self, source):
        for m in PAINT_DEFINITION.finditer(source.code):
            self.painting.setdefault(m.group(1), (source.rel, source.line(m.start())))

    def dialog_types(self):
        types = set(QT_DIALOGS)
        grown = True
        while grown:
            grown = False
            for name, (_, _, bases) in self.classes.items():
                if name not in types and any(b in types for b in bases):
                    types.add(name)
                    grown = True
        return types

    def receiver_type(self, source, header, name, before):
        """The type of the nearest declaration of name before the position."""
        declaration = re.compile(
            r"(?:\bauto[ \t]*[*&]?[ \t]*\b" + name +
            r"\s*=\s*(?:new\s+|(?:std\s*::\s*)?make_unique\s*<\s*)(?:\w+\s*::\s*)*(\w+))"
            r"|(?:\b(\w+)[ \t]*(?:<\s*(?:\w+\s*::\s*)*(\w+)\s*>)?[ \t]*[*&]?[ \t]*\b" + name +
            r"\s*(?=[({=;,)\[]))")

        def last(code, end):
            best = None
            for m in declaration.finditer(code, 0, end):
                if m.group(1) or m.group(2) not in NOT_TYPES:
                    best = m
            return best

        best = last(source.code, before)
        if best is None and header is not None:
            best = last(header.code, len(header.code))
        if best is None:
            return None
        # A smart pointer's or QPointer's pointee.
        return best.group(1) or best.group(3) or best.group(2)

    def dispatch(self, source, header, dialogs):
        if source.rel.replace(os.sep, "/") in DISPATCH_FILES:
            return
        for m in STATIC_DIALOGS.finditer(source.code):
            called = re.sub(r"\s+", "", m.group(1))
            use = DISPATCH_OF[called.split("::")[0]]
            self.problems.append((source.rel, source.line(m.start()),
                                  f"{called} shows a Qt window in the AppKit frontend; "
                                  f"use {use} (framework/Dialogs.hpp)"))
        for m in EXEC_CALL.finditer(source.code):
            kind = self.receiver_type(source, header, m.group(1), m.start())
            if kind in dialogs:
                use = "execMenu" if kind == "QMenu" else "execDialog"
                self.problems.append((source.rel, source.line(m.start()),
                                      f"{m.group(1)}: {kind}::exec shows a Qt window in the AppKit frontend; "
                                      f"use {use} (framework/Dialogs.hpp)"))


def read_sources(app):
    sources = []
    for current, dirs, names in os.walk(app):
        rel_dir = os.path.relpath(current, app)
        if rel_dir == ".":
            dirs[:] = sorted(d for d in dirs if d not in SKIPPED_DIRS)
        else:
            dirs.sort()
        for name in sorted(names):
            if name.endswith(SOURCE_SUFFIXES):
                path = os.path.join(current, name)
                with open(path, encoding="utf-8", errors="replace") as f:
                    sources.append(Source(os.path.normpath(os.path.relpath(path, app)), f.read()))
    return sources


def native_text(app):
    """The AppKit frontend's code, with its literals (command ids)."""
    paths = [os.path.join(app, name) for name in NATIVE_FILES]
    for current, _, names in os.walk(os.path.join(app, "platform", "macos")):
        paths += [os.path.join(current, name) for name in sorted(names) if name.endswith(SOURCE_SUFFIXES)]
    texts = []
    for path in paths:
        if os.path.isfile(path):
            with open(path, encoding="utf-8", errors="replace") as f:
                texts.append(strip(f.read(), literals=True))
    return "\n".join(texts)


def read_list(path):
    entries = []
    with open(path, encoding="utf-8") as f:
        for number, raw in enumerate(f, 1):
            text = raw.strip()
            if text and not text.startswith("#"):
                words = text.split(None, 2)
                entries.append((number, words))
    return entries


def check(app, list_path):
    sources = read_sources(app)
    scan = Scan()
    scopes = {s.rel: scan.classes_of(s) for s in sources}
    for s in sources:
        scan.paint_declarations(s, scopes[s.rel])
    for s in sources:
        scan.paint_definitions(s)
    # Only the app's own classes: Base::paintEvent(QPaintEvent*) of Qt's.
    scan.painting = {k: v for k, v in scan.painting.items() if k in scan.classes}
    by_rel = {s.rel: s for s in sources}
    dialogs = scan.dialog_types()
    for s in sources:
        stem, suffix = os.path.splitext(s.rel)
        header = by_rel.get(stem + ".hpp") if suffix != ".hpp" else None
        scan.dispatch(s, header, dialogs)

    list_rel = os.path.relpath(list_path, app)
    shown_list = os.path.join(os.path.basename(os.path.normpath(app)), list_rel)
    native = native_text(app)
    listed = {}
    for number, words in read_list(list_path):
        where = (list_rel, number)
        if len(words) < 3:
            scan.problems.append((*where, "a line is `Class status detail`"))
            continue
        name, status, detail = words
        if name in listed:
            scan.problems.append((*where, f"{name} is listed twice"))
        listed[name] = status
        if status not in STATUSES:
            scan.problems.append((*where, f"{name}: unknown status {status} (one of {', '.join(STATUSES)})"))
        if name not in scan.classes:
            scan.problems.append((*where, f"{name} is no class of the app; remove its line"))
        if status in SYMBOL_STATUSES:
            symbol = detail.split()[0].rstrip(":,;")
            if not re.search(r"\b" + re.escape(symbol) + r"\b", native):
                scan.problems.append((*where, f"{name}: {symbol} is not in the AppKit frontend's code "
                                              f"(platform/macos, {', '.join(NATIVE_FILES)})"))
    for name, (rel, line) in sorted(scan.painting.items()):
        if name not in listed:
            scan.problems.append((rel, line, f"{name} paints itself, which the AppKit frontend's generic bridge "
                                             f"cannot show; add it to {shown_list} (native, replaced, shared or "
                                             f"pending)"))
    pending = sorted(n for n, s in listed.items() if s == "pending")
    return sources, scan, pending


PLANTED = {
    "framework/Dialogs.cpp": """
int execDialog(QDialog* dialog) { return dialog->exec(); }
void warn() { QMessageBox::warning(nullptr, "a", "b"); }
""",
    "framework/Swatch.hpp": """
// class Commented : public QWidget { void paintEvent(QPaintEvent*) override; };
class Swatch : public QWidget {
  Q_OBJECT
public:
  void paintEvent(QPaintEvent* event) override;
  class Inner {
    int x = 0;
  };
};
enum class Mode { A, B };
class Plain : public QWidget {
  void resizeEvent(QResizeEvent*) override;
};
""",
    "framework/Swatch.cpp": """
void Swatch::paintEvent(QPaintEvent* event) { QWidget::paintEvent(event); }
namespace {
struct Local final : public QFrame {
  void paintEvent(QPaintEvent*) override {}
};
}
""",
    "framework/Listed.cpp": """
class Listed : public QWidget { void paintEvent(QPaintEvent*) override {} };
class Waiting : public QWidget { void paintEvent(QPaintEvent*) override {} };
""",
    "framework/Editor.hpp": """
class Editor : public QDialog {};
class Panel : public QWidget {
  Editor* m_editor = nullptr;
};
""",
    "framework/Editor.cpp": """
const char* text = "QMessageBox::warning(nullptr, a, b); dialog.exec()";
void a() {
  Editor dialog(nullptr);
  if (dialog.exec() != 1) {}
}
void b() {
  QEventLoop dialog;
  dialog.exec();
}
void c() {
  auto* box = new QMessageBox();
  box->exec();
  std::unique_ptr<QMenu> menu = makeMenu();
  menu->exec(QPoint());
  m_editor->exec();
  Wrapper wrapped(nullptr);
  wrapped.exec(QPoint());
}
void d() {
  QFileDialog::getOpenFileName(nullptr);
  QInputDialog :: getText(nullptr, "a", "b");
  QColorDialog::getColor();
  QMessageBox::question(nullptr, "a", "b");
}
#ifdef Q_OS_MACOS
void e() { showPanel(); }
#elif defined(Q_OS_WIN)
void e() { QMessageBox::about(nullptr, "a", "b"); }
#else
void e() { QMessageBox::about(nullptr, "a", "b"); }
#endif
#if !defined(Q_OS_WIN) && defined(MITCAD_MACOS_NATIVE_UI)
void f() { QMessageBox::critical(nullptr, "a", "b"); }
#endif
""",
}
PLANTED_LIST = """# SPDX-License-Identifier: MIT
# Listed native in a comment
Listed native MitcadNativeSwatch: the swatch
Waiting pending not yet
Gone shared over the view
Plain replaced MitcadMissing
Listed shared twice
"""
PLANTED_NATIVE = "@interface MitcadNativeSwatch : NSView\n@end\n"
EXPECTED = {
    ("framework/Swatch.hpp", 6): "Swatch paints itself",
    ("framework/Swatch.cpp", 5): "Local paints itself",
    ("framework/Editor.cpp", 5): "dialog: Editor::exec",
    ("framework/Editor.cpp", 13): "box: QMessageBox::exec",
    ("framework/Editor.cpp", 15): "menu: QMenu::exec",
    ("framework/Editor.cpp", 16): "m_editor: Editor::exec",
    ("framework/Editor.cpp", 21): "QFileDialog::getOpenFileName",
    ("framework/Editor.cpp", 22): "QInputDialog::getText",
    ("framework/Editor.cpp", 23): "QColorDialog::getColor",
    ("framework/Editor.cpp", 24): "QMessageBox::question",
    ("framework/Editor.cpp", 34): "QMessageBox::critical",
    ("native-coverage.txt", 5): "Gone is no class",
    ("native-coverage.txt", 6): "MitcadMissing is not in",
    ("native-coverage.txt", 7): "Listed is listed twice",
}


def self_test():
    with tempfile.TemporaryDirectory() as root:
        app = os.path.join(root, "app")
        for rel, text in PLANTED.items():
            path = os.path.join(app, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)
        os.makedirs(os.path.join(app, "platform", "macos"))
        with open(os.path.join(app, "platform", "macos", "NativeSwatch.mm"), "w", encoding="utf-8") as f:
            f.write(PLANTED_NATIVE)
        list_path = os.path.join(app, "native-coverage.txt")
        with open(list_path, "w", encoding="utf-8") as f:
            f.write(PLANTED_LIST)
        _, scan, pending = check(app, list_path)
    failures = 0
    found = {}
    for rel, line, text in scan.problems:
        found[(rel.replace(os.sep, "/"), line)] = text
    for where, expected in EXPECTED.items():
        if expected not in found.get(where, ""):
            print(f"self-test: not found at {where[0]}:{where[1]}: {expected}")
            failures += 1
    for where, text in sorted(found.items()):
        if where not in EXPECTED:
            print(f"self-test: found, not planted: {where[0]}:{where[1]}: {text}")
            failures += 1
    if pending != ["Waiting"]:
        print(f"self-test: pending {pending}, expected ['Waiting']")
        failures += 1
    print(f"self-test: {len(found)} problems found, {len(EXPECTED)} planted, {failures} failures")
    return 0 if failures == 0 else 1


def main(argv):
    if argv[1:] == ["--self-test"]:
        return self_test()
    if len(argv) != 3:
        print("usage: check-native-coverage.py APP LIST | --self-test", file=sys.stderr)
        return 2
    app, list_path = argv[1], argv[2]
    try:
        sources, scan, pending = check(app, list_path)
    except OSError as error:
        print(f"check-native-coverage: {error}", file=sys.stderr)
        return 2
    base = os.path.basename(os.path.normpath(app))
    for rel, line, text in scan.problems:
        print(f"{os.path.join(base, rel)}:{line}: {text}")
    print(f"{len(sources)} files, {len(scan.painting)} painting classes, {len(pending)} pending in the AppKit "
          f"frontend, {len(scan.problems)} problems (docs/native-macos-ui.md, Coverage)")
    return 0 if not scan.problems else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
