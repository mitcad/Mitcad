#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Every fixed choice's label is marked for translation (mitcad#2362).

A command's fixed choices are lists of (value, label) pairs whose labels are
English source texts (`QStringLiteral("Along X")`). The panel translates a
label in the context `Choice` (`choiceLabel()`, app/framework/CommandPanel.cpp),
and lupdate puts a label into the catalogues only when some source marks it
`QT_TRANSLATE_NOOP("Choice", "...")`. This scan finds every label given as a
literal in a fixed choice list and reports each one that no source marks so.
A label its definition translates itself (`tr("...")`, `X::tr(...)`,
`QCoreApplication::translate(...)`) needs no mark; a label made at run time
(a variable, a function's result) is not a fixed text and is not read; a
label without a letter (`"0.5"`) has nothing to translate.

The fixed choice lists it reads:
- a variable of the type `Choices`: its initializer;
- a function or lambda returning `Choices`: what its body returns, streams
  in (`<<`) and appends (not the other pairs it uses, such as a query's);
- `choiceInput(id, label, choices, default)`: its third argument, and
  `withChoices(...)`: its argument (a lambda: as a body above);
- what is put into an input's `choices` (`.choices = ...`, `<<`, `append`);
- a variable or function of the general type `Choices` stands for
  (`QVector<QPair<QString, QString>>`, `QList<...>`), which other tables use
  too, only where one of the above uses it by name (`kHoleTypes`,
  `table.fits`, `unitChoices()`).
A pair is `{value, label}` or `qMakePair(value, label)`; a pair whose first
text is translated is a table of shown texts and values, not choices. The
test driver's sources (a file name with `Test`) are not read: their commands
are not the user's.

  check-choice-labels.py FOLDER... [--exclude PATH]...
      scans the C++ and Objective-C++ sources under the folders (marks are
      read from every scanned file); prints each unmarked label as
      `file:line: ...` and exits 1 when there is one.
  check-choice-labels.py FOLDER... --list
      lists every literal choice label found, marked or not.
  check-choice-labels.py --self-test
      runs the scan on planted sources: it must report exactly the planted
      unmarked labels.

Run with `python3 -I`; it needs no build (the ctest repo.choice-labels).
"""

import argparse
import os
import re
import sys

SUFFIXES = (".cpp", ".hpp", ".h", ".mm", ".cc", ".cxx")

# The type of a command's fixed choice list (`Choices`, as
# app/commands/CommandSupport.hpp names it), and the general type it stands
# for, which other tables use too: a list of the general type counts only
# where it is used as choices.
GENERAL = r"Q(?:Vector|List)\s*<\s*QPair\s*<\s*QString\s*,\s*QString\s*>\s*>"
LIST_TYPE = r"(?:\b(Choices)\b|(" + GENERAL + r"))"
# A declaration of a choice list variable, up to its initializer.
DECLARATION = re.compile(LIST_TYPE + r"(?:\s*&\s*|\s+)(\w+)\s*(=(?!=)|\{)")
# A function returning a choice list, up to its parameters.
FUNCTION = re.compile(LIST_TYPE + r"(?:\s*&\s*|\s+)(?:\w+::)*(\w+)\s*\(")
FUNCTION_BODY = re.compile(r"\s*(?:const\s*)?(?:noexcept\s*)?(?:override\s*)?\{")
# A lambda returning a choice list, up to its body.
LAMBDA = re.compile(r"->\s*" + LIST_TYPE + r"\s*\{")
# The calls that take a choice list.
CALL = re.compile(r"\b(choiceInput|withChoices)\s*\(")
# What is put into an input's choices.
ASSIGNED = re.compile(r"\.\s*choices\s*(?:(=)(?!=)|(<<)|\.\s*(?:append|push_back|prepend|insert|emplace_back)\s*\()")
# Within the body of what makes a choice list: what it returns, streams in
# and appends.
RETURN = re.compile(r"\breturn\b")
STREAM = re.compile(r"<<")
APPEND = re.compile(r"(?:\.|->)\s*(?:append|push_back|prepend|insert|emplace_back)\s*\(")
PAIR_CALL = re.compile(r"\b(?:qMakePair|std::make_pair|QPair\s*<\s*QString\s*,\s*QString\s*>)\s*\(")
# A list made in place: a brace list, a pair, or a list type's constructor.
LIST_HERE = re.compile(r"\s*(?:(\{)|" + PAIR_CALL.pattern + r"|(?:\w+::)*Choices\s*[{(]|" + GENERAL +
                       r"\s*[{(])")
# A simple expression that names a list (`kSides`, `table.fits`,
# `sizeChoices(context)`): its last name, after a member access or not.
NAMED = re.compile(r"^\s*((?:[\w:]+\s*(?:\([^;{}]*\))?\s*(?:\.|->)\s*)*)(?:\w+::)*(\w+)\s*(?:\([^;{}]*\))?\s*$")

LITERAL = r'"(?:[^"\\\n]|\\.)*"'
LITERALS = r"(" + LITERAL + r"(?:\s*" + LITERAL + r")*)"
# A label given as a literal: the literals' text (several are concatenated).
LABEL_FORMS = [
    re.compile(r"^" + LITERALS + r"$", re.S),
    re.compile(r"^(?:QStringLiteral|QString|QString::fromUtf8|QString::fromLatin1|QLatin1String|"
               r"QLatin1StringView|QStringView|QByteArrayLiteral)\s*\(\s*" + LITERALS + r"\s*\)$", re.S),
    re.compile(r"^u?" + LITERALS + r"_s$", re.S),
]
# A label its definition translates.
TRANSLATED = re.compile(r"^(?:(?:\w+::)*(?:tr|trUtf8|translate)|qtTrId|qApp\s*->\s*translate)\s*\(")
MARK = re.compile(r'QT_TRANSLATE_NOOP3?\s*\(\s*"Choice"\s*,\s*' + LITERALS)
STRING_PIECE = re.compile(LITERAL)
LETTER = re.compile(r"[^\W\d_]")
# What may stand before the R of a raw string: nothing of a name, or an
# encoding prefix (u8R"(...)").
RAW_PREFIX = re.compile(r"(?:^|[^\w])(?:u8|u|U|L)?$")

OPEN = "([{"
CLOSE = ")]}"


def masked(text):
    """(code, shape): the text with its comments blanked, and the same with
    the contents of its string and character literals blanked too. Both keep
    every offset and line break."""
    code = list(text)
    shape = list(text)
    i = 0
    n = len(text)

    def blank(chars, start, end):
        for k in range(start, end):
            if chars[k] != "\n":
                chars[k] = " "

    while i < n:
        c = text[i]
        if text.startswith("//", i):
            end = text.find("\n", i)
            end = n if end < 0 else end
            blank(code, i, end)
            blank(shape, i, end)
            i = end
        elif text.startswith("/*", i):
            end = text.find("*/", i + 2)
            end = n if end < 0 else end + 2
            blank(code, i, end)
            blank(shape, i, end)
            i = end
        elif c == "R" and text.startswith('"', i + 1) and RAW_PREFIX.search(text[max(0, i - 3):i]):
            # A raw string: R"delim( ... )delim".
            paren = text.find("(", i + 2)
            if paren < 0:
                i += 1
                continue
            delim = text[i + 2:paren]
            end = text.find(")" + delim + '"', paren)
            end = n if end < 0 else end + len(delim) + 2
            blank(shape, i + 2, end - 1)
            i = end
        elif c == "'" and i > 0 and text[i - 1].isdigit():
            # A digit separator (1'000).
            i += 1
        elif c in "\"'":
            k = i + 1
            while k < n and text[k] != c and text[k] != "\n":
                k += 2 if text[k] == "\\" else 1
            end = min(k + 1, n)
            blank(shape, i + 1, end - 1)
            i = end
        else:
            i += 1
    return "".join(code), "".join(shape)


def matching(shape, start):
    """The offset just after the bracket that closes the one at start."""
    depth = 0
    for k in range(start, len(shape)):
        c = shape[k]
        if c in OPEN:
            depth += 1
        elif c in CLOSE:
            depth -= 1
            if depth == 0:
                return k + 1
    return len(shape)


def statement_end(shape, start):
    """The offset of the `;` that ends the statement going on at start (or of
    the bracket that closes the one it is in)."""
    depth = 0
    for k in range(start, len(shape)):
        c = shape[k]
        if c in OPEN:
            depth += 1
        elif c in CLOSE:
            depth -= 1
            if depth < 0:
                return k
        elif c == ";" and depth == 0:
            return k
    return len(shape)


def top_level_parts(shape, start, end):
    """The (start, end) offsets of the comma-separated parts between start
    and end, at the top bracket level."""
    parts = []
    depth = 0
    begin = start
    for k in range(start, end):
        c = shape[k]
        if c in OPEN:
            depth += 1
        elif c in CLOSE:
            depth -= 1
        elif c == "," and depth == 0:
            parts.append((begin, k))
            begin = k + 1
    parts.append((begin, end))
    return parts


class Lists:
    """What one file says about its choice lists: the spans of the lists it
    makes in place, and the names of the lists it uses as choices. Each comes
    with the name of the general-typed list or function it belongs to (it
    counts only when that is used as choices), or None (it counts)."""

    def __init__(self, shape):
        self.shape = shape
        self.spans = []  # (start, end, owner)
        self.uses = []  # (name, member access, owner)
        self.declared = set()  # names of general-typed lists and functions

    def made_here(self, start, owner):
        """A list made in place at start: its span kept, or False."""
        m = LIST_HERE.match(self.shape, start)
        if not m:
            return False
        self.spans.append((m.start(), matching(self.shape, m.end() - 1), owner))
        return True

    def operand(self, start, end, owner):
        """A list given at start (up to end): made in place, or named."""
        if self.made_here(start, owner):
            return
        m = NAMED.match(self.shape[start:end])
        if m:
            self.uses.append((m.group(2), bool(m.group(1).strip()), owner))

    def body(self, start, end, owner):
        """The body of what makes a list: what it returns, streams in and
        appends (not the other pairs it uses, such as a query's)."""
        for m in RETURN.finditer(self.shape, start, end):
            self.operand(m.end(), statement_end(self.shape, m.end()), owner)
        for m in STREAM.finditer(self.shape, start, end):
            self.made_here(m.end(), owner)
        for m in APPEND.finditer(self.shape, start, end):
            paren = m.end() - 1
            for a, _ in top_level_parts(self.shape, paren + 1, matching(self.shape, paren) - 1):
                self.made_here(a, owner)

    def lambda_or_operand(self, start, end, owner):
        """A lambda's body, or a list given at start."""
        if self.shape[start:end].lstrip().startswith("["):
            brace = self.shape.find("{", start, end)
            if brace >= 0:
                self.body(brace, matching(self.shape, brace), owner)
            return
        self.operand(start, end, owner)


def regions(shape):
    """The choice lists of a file (Lists)."""
    lists = Lists(shape)
    for m in DECLARATION.finditer(shape):
        owner = None if m.group(1) else m.group(3)
        if not m.group(1):
            lists.declared.add(m.group(3))
        if m.group(4) == "{":
            lists.spans.append((m.end() - 1, matching(shape, m.end() - 1), owner))
        else:
            lists.lambda_or_operand(m.end(), statement_end(shape, m.end()), owner)
    for m in FUNCTION.finditer(shape):
        params_end = matching(shape, m.end() - 1)
        body = FUNCTION_BODY.match(shape, params_end)
        if body:
            owner = None if m.group(1) else m.group(3)
            if not m.group(1):
                lists.declared.add(m.group(3))
            lists.body(body.end() - 1, matching(shape, body.end() - 1), owner)
    for m in LAMBDA.finditer(shape):
        lists.body(m.end() - 1, matching(shape, m.end() - 1), None)
    for m in CALL.finditer(shape):
        paren = m.end() - 1
        parts = top_level_parts(shape, paren + 1, matching(shape, paren) - 1)
        if m.group(1) == "choiceInput":
            if len(parts) >= 3:
                lists.operand(parts[2][0], parts[2][1], None)
        else:
            lists.lambda_or_operand(parts[0][0], parts[-1][1], None)
    for m in ASSIGNED.finditer(shape):
        if m.group(1):
            lists.operand(m.end(), statement_end(shape, m.end()), None)
        elif m.group(2):
            lists.made_here(m.end(), None)
        else:
            paren = m.end() - 1
            for a, _ in top_level_parts(shape, paren + 1, matching(shape, paren) - 1):
                lists.made_here(a, None)
    return lists


def label_text(part):
    """The literal text of a label given as literals, or None."""
    part = part.strip()
    for form in LABEL_FORMS:
        m = form.match(part)
        if m:
            return "".join(p[1:-1] for p in STRING_PIECE.findall(m.group(1)))
    return None


def pairs(code, shape, start, end):
    """The (offset, label part) of the (value, label) pairs between start and
    end: brace pairs and pair calls."""
    found = []
    calls = {m.end() - 1 for m in PAIR_CALL.finditer(shape, start, end)}
    for k in range(start, end):
        if shape[k] != "{" and k not in calls:
            continue
        parts = top_level_parts(shape, k + 1, matching(shape, k) - 1)
        if len(parts) != 2:
            continue
        value, label = (code[a:b].strip() for a, b in parts)
        if not value or value.startswith("{") or not label or label.startswith("{"):
            continue
        # A translated first text: a (shown text, value) table, not choices.
        if TRANSLATED.match(value):
            continue
        raw = code[parts[1][0]:parts[1][1]]
        found.append((parts[1][0] + len(raw) - len(raw.lstrip()), label))
    return found


def marks(code):
    """The labels a file marks QT_TRANSLATE_NOOP("Choice", ...)."""
    return {"".join(p[1:-1] for p in STRING_PIECE.findall(m.group(1))) for m in MARK.finditer(code)}


def used_lists(parsed):
    """(file, name) of each general-typed list or function that is used as
    choices, through the uses that count (a use inside such a list counts
    when that list does). A plain name used in a file means the file's own
    list of that name when it has one, else the one other file's that
    declares it; a member (`table.fits`) means any file's."""
    declared = {}
    for name, lists in parsed:
        for d in lists.declared:
            declared.setdefault(d, set()).add(name)
    used = set()
    while True:
        before = len(used)
        for name, lists in parsed:
            for use, member, owner in lists.uses:
                if owner is not None and (name, owner) not in used:
                    continue
                files = declared.get(use, set())
                if member:
                    targets = files
                elif name in files:
                    targets = {name}
                else:
                    targets = files if len(files) == 1 else set()
                used |= {(f, use) for f in targets}
        if len(used) == before:
            return used


def labels(code, shape, lists, name, used):
    """The (offset, label) of every literal label of a fixed choice list in a
    file that its definition does not translate itself."""
    seen = set()
    found = []
    for start, end, owner in lists.spans:
        if owner is not None and (name, owner) not in used:
            continue
        for offset, part in pairs(code, shape, start, end):
            if offset in seen:
                continue
            seen.add(offset)
            if TRANSLATED.match(part):
                continue
            text = label_text(part)
            if text is None or not LETTER.search(text):
                continue
            found.append((offset, text))
    return found


def scan(files, every=False):
    """files: (name, text) pairs. The (name, line, label) of every literal
    choice label no file marks, in file and line order; every: of every
    literal choice label, with whether it is marked."""
    parsed = []
    marked = set()
    for name, text in files:
        code, shape = masked(text)
        marked |= marks(code)
        parsed.append((name, text, code, shape, regions(shape)))
    used = used_lists([(name, lists) for name, _, _, _, lists in parsed])
    found = []
    for name, text, code, shape, lists in parsed:
        for offset, label in labels(code, shape, lists, name, used):
            line = text.count("\n", 0, offset) + 1
            if every:
                found.append((name, line, label, label in marked))
            elif label not in marked:
                found.append((name, line, label))
    return sorted(found)


def sources(folders, excluded):
    """(path, text) of the sources under the folders, less the excluded
    folders and files."""
    excluded = {os.path.normcase(os.path.realpath(e)) for e in excluded}

    def kept(path):
        return os.path.normcase(os.path.realpath(path)) not in excluded

    for folder in folders:
        for current, dirs, names in os.walk(folder):
            dirs[:] = sorted(d for d in dirs if kept(os.path.join(current, d)))
            for name in sorted(names):
                path = os.path.join(current, name)
                # The test driver's sources (TestDriver*.cpp,
                # NativeTestInput.mm) define commands the user never sees.
                if name.endswith(SUFFIXES) and "Test" not in name and kept(path):
                    with open(path, encoding="utf-8", errors="replace") as f:
                        yield path, f.read()


# The planted sources of the self-test: each label named "Unmarked ..." must
# be reported, and no other.
PLANTED = {
    "marks.cpp": '''
// The catalogues' list.
const char* const kNames[] = {QT_TRANSLATE_NOOP("Choice", "Marked One"),
                              QT_TRANSLATE_NOOP("Choice", "Marked "
                                                          "Two")};
const char* const kOther[] = {QT_TRANSLATE_NOOP("Other", "Unmarked Other Context")};
''',
    "commands.cpp": '''
using Choices = QVector<QPair<QString, QString>>;
const Choices kKinds = {{QStringLiteral("one"), QStringLiteral("Marked One")},
                        {kTwo, QStringLiteral("Marked Two")},
                        // {QStringLiteral("old"), QStringLiteral("Commented Out")},
                        {QStringLiteral("three"), QStringLiteral("Unmarked Three")}};
const QVector<QPair<QString, QString>> kOther{{QStringLiteral("x"), tr("Translated Itself")},
                                              {QStringLiteral("y"), QObject::tr("Translated Too")},
                                              {QStringLiteral("z"), QStringLiteral("0.5")},
                                              {QStringLiteral("w"), "Unmarked Bare"}};
const QHash<QString, QString> kIcons = {{QStringLiteral("one"), QStringLiteral("not-a-label")}};
QJsonObject json() { return {{QStringLiteral("key"), QStringLiteral("Not A Choice")}}; }
Choices rulesOf(const Context& context) {
  Choices rules = {{QString(), QStringLiteral("Unmarked In Function")}};
  for (const auto& rule : context.query({{QStringLiteral("query"), QStringLiteral("not_a_label")}})) {
    rules.append({rule.id, rule.name});
  }
  rules << qMakePair(QStringLiteral("last"), QStringLiteral("Unmarked Made Pair"));
  return rules;
}
Choices declaredOnly(const Context& context);
void define(Command& def) {
  def.inputs << choiceInput(QStringLiteral("side"), tr("Side"),
                            {{QStringLiteral("a"), QStringLiteral("Unmarked Inline")},
                             {QStringLiteral("b"), QStringLiteral("Marked One")}},
                            QStringLiteral("a"))
                    .withTooltip(QStringLiteral("Not A Label {x, y}"));
  def.inputs << choiceInput(QStringLiteral("list"), tr("List"), {}, QString())
                    .withChoices([](const CommandState& state) -> Choices {
                      return {{QStringLiteral("p"), QStringLiteral("Unmarked In Lambda")},
                              {state.text(), state.text()}};
                    });
  InputDef input;
  input.choices = {{QStringLiteral("q"), QStringLiteral("Unmarked Assigned")}};
  input.choices.append({QStringLiteral("r"), u"Unmarked Appended"_s});
  const QString raw = QStringLiteral(R"(a {QStringLiteral("v"), QStringLiteral("Raw")} b)");
  def.inputs << choiceInput(QStringLiteral("other"), tr("Other"), kOther, QStringLiteral("x"));
  def.inputs << choiceInput(QStringLiteral("fit"), tr("Fit"), {}, QString())
                    .withChoices([&](const CommandState&) { return table(context).fits; });
}
// General pair lists that are no choices: shown texts with values, a map.
const QList<QPair<QString, QString>> kShown{{tr("Sheet metal"), QStringLiteral("Not A Choice")}};
const QList<QPair<QString, QString>> kSlots = {{QStringLiteral("a_inset"), QStringLiteral("inset")}};
QList<QPair<QString, QString>> unitsOf() { return {{QStringLiteral("mm"), QStringLiteral("mm")}}; }
''',
    "sizes.hpp": '''
struct Table {
  QVector<QPair<QString, QString>> fits{{QStringLiteral("fine"), QStringLiteral("Unmarked Member")},
                                        {QStringLiteral("normal"), QStringLiteral("Marked One")}};
};
''',
    "units.cpp": '''
QList<QPair<QString, QString>> unitsOf() { return {{QStringLiteral("cm"), QStringLiteral("Unmarked Unit")}}; }
void define(Command& def) {
  def.inputs << choiceInput(QStringLiteral("unit"), tr("Unit"), unitsOf(), QStringLiteral("cm"));
}
''',
}
PLANTED_EXPECTED = [
    ("commands.cpp", 6, "Unmarked Three"),
    ("commands.cpp", 10, "Unmarked Bare"),
    ("commands.cpp", 14, "Unmarked In Function"),
    ("commands.cpp", 18, "Unmarked Made Pair"),
    ("commands.cpp", 24, "Unmarked Inline"),
    ("commands.cpp", 30, "Unmarked In Lambda"),
    ("commands.cpp", 34, "Unmarked Assigned"),
    ("commands.cpp", 35, "Unmarked Appended"),
    ("sizes.hpp", 3, "Unmarked Member"),
    ("units.cpp", 2, "Unmarked Unit"),
]


def self_test():
    found = scan(sorted(PLANTED.items()))
    if found != PLANTED_EXPECTED:
        print("choice labels self-test failed")
        print("  expected:")
        for item in PLANTED_EXPECTED:
            print("    %s:%d: %s" % item)
        print("  found:")
        for item in found:
            print("    %s:%d: %s" % item)
        return 1
    # A label marked nowhere fails the scan; marking it passes it.
    lone = [("a.cpp", 'const Choices kA = {{QStringLiteral("a"), QStringLiteral("Lone")}};\n')]
    if scan(lone) != [("a.cpp", 1, "Lone")]:
        print("choice labels self-test failed: an unmarked label passed")
        return 1
    if scan(lone + [("b.cpp", 'QT_TRANSLATE_NOOP("Choice", "Lone")\n')]):
        print("choice labels self-test failed: a marked label was reported")
        return 1
    print("choice labels self-test passed: %d planted labels found" % len(found))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("folders", nargs="*")
    parser.add_argument("--exclude", action="append", default=[])
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.folders:
        parser.error("name the folders to scan")
    for folder in args.folders:
        if not os.path.isdir(folder):
            parser.error("not a folder: " + folder)
    files = list(sources(args.folders, args.exclude))
    if args.list:
        for name, line, label, marked in scan(files, every=True):
            print('%s:%d: %s "%s"' % (name, line, "marked" if marked else "UNMARKED", label))
        return 0
    unmarked = scan(files)
    for name, line, label in unmarked:
        print('%s:%d: the choice label "%s" is not marked QT_TRANSLATE_NOOP("Choice", "%s") '
              "(app/framework/CommandPanel.cpp, kChoiceNames) nor translated by its own tr()"
              % (name, line, label, label))
    if unmarked:
        print("%d unmarked choice label(s) in %d files (docs/translations.md, Fixed choices)"
              % (len(unmarked), len(files)))
        return 1
    print("choice labels: every literal label in %d files is marked" % len(files))
    return 0


if __name__ == "__main__":
    sys.exit(main())
