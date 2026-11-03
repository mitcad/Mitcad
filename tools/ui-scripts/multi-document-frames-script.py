#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Generate the portable frame scenario for the platform's locked UI harness.

The harness supplies synthetic first/second/third.mitcad with volumes
6000/120/27 mm3, starts on first, and selects Floating or Docked chrome.
macOS enables only the guarded MITCAD_TEST_DOCUMENT_FRAMES test fixture.
"""
from pathlib import Path
import shlex
import sys


def scenario(work="${MITCAD_TEST_WORK}", mode="full"):
    work = Path(work)
    file = lambda name: shlex.quote(str(work / name))
    if mode == "close-all":
        # The harness expects natural process exit, not TestDriver's done
        # marker: only the actual final document close ends this scenario.
        return "\n".join([
            "wait-idle", "capture-document-window first", "expect-model-bodies 1 6000",
            "open-file " + file("second.mitcad"), "wait-idle", "capture-document-window second",
            "expect-model-bodies 1 120", "open-file " + file("third.mitcad"), "wait-idle",
            "capture-document-window third", "expect-model-bodies 1 27",
            "expect-document-windows 3", "expect-document-frame-tabs 3",
            "expect-document-frame-order first second third",
            "observe-document-frame-close-all", "document-tab-context 0", "popup 'Close All'", "stay", "",
        ])
    if mode != "full":
        raise ValueError("mode must be full or close-all")
    return "\n".join([
        "wait-idle", "window-resize 1280 800", "capture-document-window first",
        "expect-document-frame-tabs 1", "expect-model-bodies 1 6000",
        "capture-undo-depth firstUndo",
        "open-file " + file("second.mitcad"), "wait-idle",
        "capture-document-window second", "expect-model-bodies 1 120",
        "open-file " + file("third.mitcad"), "wait-idle",
        "capture-document-window third", "expect-model-bodies 1 27",
        "expect-document-windows 3", "expect-document-frame-tabs 3",
        "expect-document-frame-order first second third",
        "expect-document-hidden-quiet first 0.2", "expect-document-hidden-quiet second 0.2",
        "screenshot " + file("document-frames-three.png"),
        "document-tab-click 0", "expect-model-bodies 1 6000",
        "model-command '{\"cmd\":\"add_parameter\",\"name\":\"frameOnly\",\"unit\":\"mm\",\"expression\":\"1 mm\"}'",
        "expect-model-parameter frameOnly 1 user", "expect-undo-depth ${firstUndo} 1",
        "click-visible-face", "expect-selection 1 0",
        "document-tab-click 2", "expect-model-bodies 1 27",
        "expect-document-hidden-quiet first 0.5",
        "expect-model-parameter frameOnly absent",
        "document-tab-click 0", "expect-model-bodies 1 6000",
        "expect-selection 1 0", "expect-undo-depth ${firstUndo} 1",
        "document-tab-drag 0 2", "expect-document-frame-order second third first",
        "expect-model-bodies 1 6000",
        "screenshot " + file("document-frames-reordered.png"),
        "key ctrl+Tab", "expect-model-bodies 1 120",
        "key ctrl+shift+Tab", "expect-model-bodies 1 6000",
        "document-tab-context 2", "popup 'Move to New Window'",
        "expect-document-windows 3", "expect-document-frame-tabs 1",
        "expect-document-frame-order first", "expect-model-bodies 1 6000",
        "expect-viewport",
        "expect-selection 1 0", "expect-model-parameter frameOnly 1 user",
        "expect-undo-depth ${firstUndo} 1",
        "screenshot " + file("document-frames-detached.png"),
        # Drop uses the real tab bar's authenticated document MIME handler.
        "document-tab-drop first second 1", "focus-document-window first",
        "expect-document-frame-tabs 3", "expect-document-frame-order second first third",
        "expect-document-hidden-quiet third 0.2",
        "expect-model-bodies 1 6000", "expect-selection 1 0",
        "expect-viewport",
        "expect-model-parameter frameOnly 1 user", "expect-undo-depth ${firstUndo} 1",
        "screenshot " + file("document-frames-rejoined.png"),
        "document-tab-click 2 middle", "expect-document-window-gone third",
        "expect-document-windows 2", "expect-document-frame-order second first",
        "open-file " + file("third.mitcad"), "wait-idle",
        "capture-document-window thirdAgain", "expect-document-frame-tabs 3",
        "key ctrl+w", "expect-document-window-gone thirdAgain",
        "expect-document-frame-tabs 2", "expect-document-windows 2",
        # Close Others must stop at the changed first document's own question.
        "document-tab-context 0", "popup 'Close Others'",
        "focus-dialog '^Save changes to first[.]mitcad[?]$'", "button '^Cancel$'", "focus-main",
        "expect-document-windows 2", "expect-document-frame-order second first",
        "focus-document-window first", "expect-model-bodies 1 6000",
        "expect-model-parameter frameOnly 1 user", "expect-undo-depth ${firstUndo} 1",
        "screenshot " + file("document-frames-close-cancel.png"),
        "document-tab-context 0", "popup 'Close Others'",
        "focus-dialog '^Save changes to first[.]mitcad[?]$'", "button \"^Don't Save$\"", "focus-main",
        "expect-document-window-gone first", "expect-document-windows 1",
        "expect-document-frame-order second", "expect-model-bodies 1 120",
        "capture-undo-depth secondUndo",
        "model-command '{\"cmd\":\"add_parameter\",\"name\":\"secondOnly\",\"unit\":\"mm\",\"expression\":\"2 mm\"}'",
        "open-file " + file("third.mitcad"), "wait-idle",
        "capture-document-window thirdFinal", "expect-document-frame-tabs 2",
        "document-tab-context 1", "popup 'Close All'",
        "focus-dialog '^Save changes to second[.]mitcad[?]$'", "button '^Cancel$'", "focus-main",
        "expect-document-windows 2", "expect-document-frame-order second thirdFinal",
        "focus-document-window second", "expect-model-bodies 1 120",
        "expect-model-parameter secondOnly 2 user", "expect-undo-depth ${secondUndo} 1",
        "key ctrl+z", "expect-model-parameter secondOnly absent", "expect-undo-depth ${secondUndo} 0",
        "focus-document-window thirdFinal", "expect-model-bodies 1 27", "quit", "",
    ])


if __name__ == "__main__":
    print(scenario(*sys.argv[1:3]), end="")
