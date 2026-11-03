#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Actual Qt/AppKit selection of another open document's saved design."""
import sys


def scenario(frontend="qt"):
    choose = "native-choice" if frontend == "native" else "qt-choice"
    picker = ["command insert.open_design", "focus-dialog '^Insert from an Open Design$'",
              "expect-widget-property openDesignChoice count 1",
              choose + " openDesignChoice second.mitcad", "button '^Insert$'"]
    copy = ["focus-dialog '^Insert second[.]mitcad linked,'", "button '^Copy$'", "focus-main"]
    save_question = "focus-dialog '^Save second[.]mitcad before inserting it[?]$'"
    return "\n".join([
        "wait-idle", "capture-document-window first", "expect-model-bodies 1 6000",
        "capture-undo-depth firstUndo", "open-file '${MITCAD_TEST_WORK}/second.mitcad'", "wait-idle",
        "capture-document-window second", "expect-document-windows 2", "expect-model-bodies 1 120",
        "capture-undo-depth secondUndo", "focus-document-window first",
        *picker[:-1], "screenshot '${MITCAD_TEST_WORK}/open-design-list.png'", picker[-1], *copy,
        "expect-document-window-active first", "expect-model-bodies 2 6120", "expect-undo-depth ${firstUndo} 1",
        "focus-document-window second", "expect-model-bodies 1 120", "expect-undo-depth ${secondUndo} 0",
        "focus-document-window first", "command edit.undo", "expect-model-bodies 1 6000",
        "expect-undo-depth ${firstUndo} 0",
        # A modified source must be saved explicitly. Cancel changes neither owner.
        "focus-document-window second",
        "model-command '{\"cmd\":\"add_feature\",\"def\":{\"type\":\"box\",\"plane\":{\"type\":\"origin\",\"plane\":\"xy\"},\"corner\":[20,0],\"length\":2,\"width\":2,\"height\":2,\"operation\":\"new_body\"}}'",
        "expect-model-bodies 2 128", "expect-undo-depth ${secondUndo} 1", "focus-document-window first",
        *picker, save_question, "screenshot '${MITCAD_TEST_WORK}/open-design-save-question.png'",
        "button '^Cancel$'", "focus-main", "expect-document-window-active first",
        "expect-model-bodies 1 6000", "expect-undo-depth ${firstUndo} 0",
        "focus-document-window second", "expect-model-bodies 2 128", "expect-undo-depth ${secondUndo} 1",
        "focus-document-window first", *picker, save_question, "button '^Save$'", *copy,
        "expect-document-window-active first", "expect-model-bodies 3 6128", "expect-undo-depth ${firstUndo} 1",
        "screenshot '${MITCAD_TEST_WORK}/open-design-inserted.png'",
        "focus-document-window second", "expect-model-bodies 2 128", "expect-undo-depth ${secondUndo} 1",
        "focus-document-window first", "command edit.undo", "expect-model-bodies 1 6000",
        "expect-undo-depth ${firstUndo} 0",
        # An unsaved source also asks first; cancellation keeps its independent design.
        "command window.new", "wait-idle", "capture-document-window unsaved",
        "model-command '{\"cmd\":\"add_feature\",\"def\":{\"type\":\"box\",\"plane\":{\"type\":\"origin\",\"plane\":\"xy\"},\"corner\":[0,0],\"length\":3,\"width\":3,\"height\":3,\"operation\":\"new_body\"}}'",
        "expect-model-bodies 1 27", "focus-document-window first", "command insert.open_design",
        "focus-dialog '^Insert from an Open Design$'", "expect-widget-property openDesignChoice count 2",
        choose + " openDesignChoice 'Untitled (window-3)'", "button '^Insert$'",
        "focus-dialog '^Save Untitled before inserting it[?]$'", "button '^Cancel$'", "focus-main",
        "expect-document-window-active first", "expect-model-bodies 1 6000", "expect-undo-depth ${firstUndo} 0",
        "focus-document-window unsaved", "expect-model-bodies 1 27", "command file.close",
        "focus-dialog '^Save changes to Untitled[?]$'", "button \"^Don't Save$\"", "focus-main",
        "expect-document-window-gone unsaved", "focus-document-window second", "command file.close",
        "expect-document-window-gone second", "expect-document-windows 1", "focus-document-window first",
        # With only this document left, self insertion has no selectable row.
        "command insert.open_design", "focus-dialog '^Insert from an Open Design$'",
        "expect-widget-property openDesignChoice count 0", "expect-widget-property openDesignInsert enabled false",
        "button '^Cancel$'", "focus-main", "expect-model-bodies 1 6000", "quit", "",
    ])


if __name__ == "__main__":
    print(scenario(*sys.argv[1:2]), end="")
