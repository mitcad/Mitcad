#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Keeps the user guide's table of contents in step with its headings.

docs/user-guide.md has a table of contents after its introduction: every
`##` heading and, indented under it, its `###` headings, each a link to the
heading's anchor on GitHub. The table sits between two marker comments,
`<!-- contents: ... -->` and `<!-- end of contents -->`; the script writes
it, so nobody edits it by hand.

    tools/check-guide-contents.py [--file GUIDE]
        checks the table: exit status 1 when it differs from the headings
        (or is missing), with the expected table printed
    tools/check-guide-contents.py --write [--file GUIDE]
        writes the table (in place of the old one, or before the first `##`
        heading when there is none)
    tools/check-guide-contents.py --self-test
        checks the anchors on headings with known GitHub anchors, then
        copies the guide to build/guide-contents-self-test.md (overwritten
        each run), renames a heading there and confirms that the check
        fails and passes again after the table is written

tools/check-all.sh runs the self-test and the check as its step `guide`
(docs/development.md, "The check script").

The anchors are GitHub's: the heading's text as rendered (code spans keep
their text, links their link text, emphasis loses its markers), lowercased;
every character that is not a letter, a mark, a decimal digit, a connector
(`_`), a hyphen or a space is removed; each space becomes a hyphen (so
"A › B" gives "a--b"); a repeated anchor gets "-1", "-2", ... in the order
of the headings, counting every heading level. Headings inside fenced code
blocks are not headings. Setext headings (underlined) are not read: the
guide has none.
"""

import argparse
import html
import os
import re
import sys
import unicodedata

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GUIDE = os.path.join(ROOT, "docs", "user-guide.md")
SCRIPT = "tools/check-guide-contents.py"
BEGIN = "<!-- contents: written by %s --write after a heading changes; do not edit by hand -->" % SCRIPT
END = "<!-- end of contents -->"
TITLE = "## Contents"

HEADING = re.compile(r"^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$")
FENCE = re.compile(r"^\s*(`{3,}|~{3,})")


def plain_text(markdown):
    """A heading's inline Markdown as GitHub renders it to text."""
    spans = []

    def keep(match):
        content = match.group(2)
        if len(content) > 1 and content.startswith(" ") and content.endswith(" ") and content.strip():
            content = content[1:-1]
        spans.append(content)
        return "\0%d\0" % (len(spans) - 1)

    text = re.sub(r"(`+)(.+?)(?<!`)\1(?!`)", keep, markdown)
    text = re.sub(r"!\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"\[([^\]]*)\]\[[^\]]*\]", r"\1", text)
    text = re.sub(r"<[^>\0]+>", "", text)
    escapes = []

    def escaped(match):
        escapes.append(match.group(1))
        return "\1%d\1" % (len(escapes) - 1)

    text = re.sub(r"\\([!-/:-@\[-`{-~])", escaped, text)
    # Emphasis: every * is a marker; an _ only where it does not join two
    # word characters (snake_case keeps its underscores).
    text = text.replace("*", "")
    text = re.sub(r"(?<![^\W_])_+|_+(?![^\W_])", "", text)
    text = re.sub("\1(\\d+)\1", lambda m: escapes[int(m.group(1))], text)
    text = re.sub("\0(\\d+)\0", lambda m: spans[int(m.group(1))], text)
    return html.unescape(text).strip()


def anchor(text):
    """GitHub's anchor of a heading's rendered text, before duplicates."""
    kept = []
    for ch in text.lower():
        category = unicodedata.category(ch)
        if ch in " -" or category[0] in "LM" or category in ("Nd", "Pc"):
            kept.append(ch)
    return "".join(kept).replace(" ", "-")


def headings(lines):
    """(line index, level, Markdown text, anchor) of every heading."""
    found = []
    seen = {}
    fence = None
    for index, line in enumerate(lines):
        opening = FENCE.match(line)
        if fence is not None:
            if opening and opening.group(1)[0] == fence[0] and len(opening.group(1)) >= len(fence) \
                    and not line.strip()[len(opening.group(1)):].strip():
                fence = None
            continue
        if opening:
            fence = opening.group(1)
            continue
        match = HEADING.match(line)
        if not match:
            continue
        markdown = (match.group(2) or "").strip()
        base = anchor(plain_text(markdown))
        count = seen.get(base, 0)
        seen[base] = count + 1
        name = base if count == 0 else "%s-%d" % (base, count)
        while count > 0 and name in seen:
            count += 1
            name = "%s-%d" % (base, count)
        if name != base:
            seen[name] = 1
        found.append((index, len(match.group(1)), markdown, name))
    return found


def region(lines):
    """(begin, end) line indices of the table's markers, or None."""
    begin = end = None
    for index, line in enumerate(lines):
        if line.strip() == BEGIN and begin is None:
            begin = index
        elif line.strip() == END and begin is not None:
            end = index
            break
    return None if begin is None or end is None else (begin, end)


def link_text(markdown):
    # A link inside a link is not one: such a heading is listed as text.
    if re.search(r"\]\(|\]\[", markdown):
        return plain_text(markdown).replace("[", r"\[").replace("]", r"\]")
    return markdown


def expected_table(lines):
    """The table's lines (markers included) for the headings outside it."""
    span = region(lines)
    entries = []
    for index, level, markdown, name in headings(lines):
        if span is not None and span[0] <= index <= span[1]:
            continue
        if level == 2:
            entries.append("- [%s](#%s)" % (link_text(markdown), name))
        elif level == 3:
            entries.append("  - [%s](#%s)" % (link_text(markdown), name))
    return [BEGIN, TITLE, ""] + entries + [END]


def read(path):
    with open(path, encoding="utf-8") as stream:
        return stream.read().split("\n")


def write(path, lines):
    with open(path, "w", encoding="utf-8", newline="\n") as stream:
        stream.write("\n".join(lines))


def updated(lines):
    """The guide's lines with the table written into them."""
    span = region(lines)
    if span is not None:
        # Duplicate anchors count the table's own heading: written in place,
        # it is where it was.
        return lines[:span[0]] + expected_table(lines) + lines[span[1] + 1:]
    first = next((index for index, level, _, _ in headings(lines) if level == 2), len(lines))
    with_table = lines[:first] + [BEGIN, TITLE, "", END, ""] + lines[first:]
    return updated(with_table)


def check(path, quiet=False):
    lines = read(path)
    span = region(lines)
    expected = expected_table(lines) if span is not None else expected_table(updated(lines))
    if span is not None and lines[span[0]:span[1] + 1] == expected:
        if not quiet:
            count = sum(1 for line in expected if line.lstrip().startswith("- ["))
            print("%s: the table of contents matches its %d headings" % (path, count))
        return True
    if not quiet:
        why = "differs from the headings" if span is not None else "is missing (no %s marker)" % END
        print("%s: the table of contents %s. Run %s --write, or put in this table:" % (path, why, SCRIPT))
        print()
        print("\n".join(expected))
    return False


SAMPLES = [
    # (heading's Markdown, GitHub's anchor) in one document, in order.
    ("A project's viewer", "a-projects-viewer"),
    ("Help › User Guide", "help--user-guide"),
    ("Save As…", "save-as"),
    ("Connect AI Assistant...", "connect-ai-assistant"),
    ("`mitcad-cli` commands", "mitcad-cli-commands"),
    ("The `solid.extrude` command", "the-solidextrude-command"),
    ("Gear Pair — Belt Drive", "gear-pair--belt-drive"),
    ("Pages 1–3 – an en dash", "pages-13--an-en-dash"),
    ("snake_case and 2D/3D", "snake_case-and-2d3d"),
    ("*Emphasis* and **strong** and _under_", "emphasis-and-strong-and-under"),
    ("A [link](https://example.org) in it", "a-link-in-it"),
    (".f3d import", "f3d-import"),
    ("1. When you need configurations", "1-when-you-need-configurations"),
    ("Options", "options"),
    ("Options", "options-1"),
    ("Options", "options-2"),
    ("Größe & Maß", "größe--maß"),
    ("Closing hashes ##", "closing-hashes"),
]


def self_test():
    failures = []
    document = ["# Title", ""]
    for markdown, _ in SAMPLES:
        document += ["### " + markdown, "", "```", "## Not a heading in a fence", "```", ""]
    found = headings(document)[1:]
    if len(found) != len(SAMPLES):
        failures.append("%d headings read, %d expected" % (len(found), len(SAMPLES)))
    for (markdown, wanted), (_, _, _, name) in zip(SAMPLES, found):
        if name != wanted:
            failures.append("'%s': anchor '%s', GitHub's is '%s'" % (markdown, name, wanted))

    # The guide's copy: written, checked, a heading renamed, failed, written,
    # checked again; the renamed heading is in the new table.
    folder = os.path.join(ROOT, "build")
    os.makedirs(folder, exist_ok=True)
    copy = os.path.join(folder, "guide-contents-self-test.md")
    write(copy, updated(read(GUIDE)))
    if not check(copy, quiet=True):
        failures.append("the copy's table, just written, does not pass")
    lines = read(copy)
    span = region(lines)
    target = next((h for h in headings(lines) if h[1] == 3 and span is not None and h[0] > span[1]), None)
    if span is None or target is None:
        failures.append("the copy has no table or no ### heading after it")
    else:
        index, _, markdown, _ = target
        renamed = markdown + " renamed in the self-test"
        lines[index] = "### " + renamed
        write(copy, lines)
        if check(copy, quiet=True):
            failures.append("the check passes after '%s' was renamed" % markdown)
        write(copy, updated(read(copy)))
        if not check(copy, quiet=True):
            failures.append("the check fails after the table was written again")
        link = "[%s](#%s)" % (renamed, anchor(plain_text(renamed)))
        if not any(line.strip() == "- " + link for line in read(copy)):
            failures.append("the new table has no entry %s" % link)
    for failure in failures:
        print("self-test: FAIL: " + failure)
    if not failures:
        print("self-test: %d anchors as GitHub's; a renamed heading fails the check, the written table passes"
              % len(SAMPLES))
    return not failures


def main():
    parser = argparse.ArgumentParser(description="The user guide's table of contents.")
    parser.add_argument("--file", default=GUIDE, help="the guide (default docs/user-guide.md)")
    parser.add_argument("--write", action="store_true", help="write the table into the guide")
    parser.add_argument("--self-test", action="store_true", help="check the check itself")
    args = parser.parse_args()
    if args.self_test:
        return 0 if self_test() else 1
    if args.write:
        lines = read(args.file)
        new = updated(lines)
        if new != lines:
            write(args.file, new)
            print("%s: the table of contents written" % args.file)
        else:
            print("%s: the table of contents was up to date" % args.file)
        return 0
    return 0 if check(args.file) else 1


if __name__ == "__main__":
    sys.exit(main())
