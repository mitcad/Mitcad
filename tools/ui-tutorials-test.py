# SPDX-License-Identifier: MIT
# check-all sources: app/tutorials app/webbrowser core/vcs/src/library tools/cli
# check-all sources: app/MainWindowStart.cpp app/WindowManager.cpp app/files/StartPage.cpp
"""In-app tutorials (mitcad#2012, docs/tutorials.md): a tutorials repository
of the test's own (a bare clone in a folder) as the only source; Help >
Tutorials... shows the Start page's tutorials (mitcad#2346), fetches the
source and shows its two-step tutorial's card, whose Start installs it and
opens it in the tutorial pane in the empty design under the page, through
the real CEF view of the web browser component the build made (the
tutorial's block made then, in its design). The page's bridge
(window.mitcad) starts the guide step; a step
naming a command MitCad does not have is refused; Escape with no command open
ends the step and the input is free, and the page shows it again; another
command is refused and logged, and the page hears of it; opening the intended command and
typing the intended value complete the steps and the page gets the events;
a page outside the tutorial's folder gets no bridge; Leave Tutorial ends the
step and closes the pane, and the input is free again.

Then the example tutorials repository (docs/examples/tutorials, the seed of
the public tutorials repository, mitcad#2078) as the only source, in a run of
its own: its sketch-extrude tutorial's card is shown by Help > Tutorials...,
its Start installs and opens it, and each of its steps is done as a user
does it (Create Sketch on the XY plane, a rectangle, Finish Sketch, Extrude,
20 mm, OK) until the page hears that the last one is done, the design has
the block and the tutorial's progress is done. A UI change that breaks the
example fails here.

With start-page (mitcad#2168, a test of its own): the example's tutorial
started while the Start page is shown, from its card that the page's own
Tutorials button shows (mitcad#2346), in the Floating and the Docked
layout, in three cases: the page at launch, the last design closed to the
page, and a modified design under the page. Each time the tutorial starts
in an empty design of its own (the page left for the empty design under
it, or New Design in a new window), its first step's Create Sketch button
is outlined, and the tutorial is done to its end. At launch and after the
last design closed, the window's Start page, shown again after the first
step, has the card In progress (step 1 of 5); over a modified design (the
tutorial's design another tab of the window) the user's design, shown again
at the end, keeps its block and its unsaved changes and its page's card
says Done.

With look (not a ctest): pictures of the Start page's tutorials, the
example's card not started and in progress, in both layouts, light and
dark, for a look.

Usage: ui-tutorials-test.py <app> <cli> <output folder> [<Qt's bin> [<the built web browser component> [<example>]]]
       ui-tutorials-test.py start-page <app> <cli> <output folder> [<Qt's bin> [<component> [<example>]]]
       ui-tutorials-test.py look <app> <output folder> [<Qt's bin> [<component> [<example>]]]
       ui-tutorials-test.py prepare <output folder> [<example>]   (the sources and the steps, for the macOS wrapper)
       ui-tutorials-test.py prepare-start-page <output folder> [<example>]   (the same for the Start page's runs)
       ui-tutorials-test.py verify <output folder>
<example> is the example tutorials repository's folder; by default
docs/examples/tutorials of this checkout.
"""
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile

# The example tutorials repository (docs/tutorials.md).
EXAMPLE = Path(__file__).resolve().parent.parent / "docs" / "examples" / "tutorials"
# Its tutorial the second run drives, and the folder of that run.
EXAMPLE_TUTORIAL = "sketch-extrude"
EXAMPLE_RUN = "example"
# The longest one run of the application may take, in seconds; the ctests'
# TIMEOUTs (cmake/SelectionUiTests.cmake) leave room for all their runs at
# this length (two runs, the Start page's six), so that a run that hangs is
# reported with its log rather than the whole test killed by CTest.
RUN_TIMEOUT = 300

STEPS = [
    {"id": "extrude", "text": "Open Extrude.", "target": {"command": "solid.extrude"},
     "done": {"started": "solid.extrude"}},
    {"id": "distance", "text": "Type 20 mm as the distance.", "target": {"input": "distance"},
     "done": {"input": "distance", "value": "20 mm"}},
    {"id": "finish", "text": "Click OK when the preview suits you.", "target": {"command": "solid.extrude"},
     "done": {"command": "solid.extrude"}},
]

PAGE = """<!doctype html>
<html><head><meta charset="utf-8"><title>Two steps</title></head><body>
<h1>Two steps</h1>
<p>An extrude, step by step.</p>
<p><a href="%(outside)s">Outside page</a></p>
<script>
function say(text) { console.log('mitcad:tutorial:' + text); }
if (!window.mitcad) {
  say('no bridge');
} else {
  var steps = %(steps)s;
  var next = 0;
  function start(index) {
    mitcad.startStep(steps[index]).then(function () { say('started ' + steps[index].id); },
                                       function (error) { say('step refused: ' + error.message); });
  }
  mitcad.on('step_done', function (event) {
    say('done ' + event.step + ' ' + JSON.stringify(event.done));
    next += 1;
    if (next < steps.length) { start(next); }
  });
  mitcad.on('refused', function (event) { say('refused ' + event.command); });
  mitcad.on('step_left', function (event) {
    say('left ' + event.step + ' ' + event.reason);
    if (event.reason === 'escape') {
      // Escape stops the step shown; the page offers it again.
      var again = document.createElement('a');
      again.href = '#again';
      again.textContent = 'Show this step again';
      again.onclick = function () { again.remove(); start(next); return false; };
      document.body.appendChild(again);
    }
  });
  mitcad.state().then(function (state) { say('state mode=' + state.mode); });
  // A step naming a command MitCad does not have is refused with the reason.
  mitcad.startStep({text: 'Nothing', target: {command: 'no.such'}, done: {command: 'no.such'}})
    .then(function () { say('unknown command started'); },
          function (error) { say('unknown command refused: ' + error.message); start(0); });
}
</script>
</body></html>
"""

OUTSIDE = """<!doctype html>
<html><head><meta charset="utf-8"><title>Outside</title></head><body>
<p>A page outside the tutorial's folder.</p>
<script>
console.log('mitcad:tutorial:outside bridge=' + typeof window.mitcad + ' query=' + typeof window.mitcadQuery);
</script>
</body></html>
"""


def git(folder, *args):
    subprocess.run(["git", "-c", "user.name=MitCad tests", "-c", "user.email=tests@example.invalid",
                    "-c", "core.autocrlf=false", *args], cwd=folder, check=True, stdout=subprocess.DEVNULL,
                   stderr=subprocess.STDOUT, timeout=60)


def remove(path):
    """A folder or file of an earlier run gone, git's read-only object files
    (Windows) too; an error if it cannot be removed."""
    path = Path(path)
    if path.is_dir() and not path.is_symlink():
        def writable(function, name, *_):
            os.chmod(name, stat.S_IWRITE)
            function(name)
        if sys.version_info >= (3, 12):
            shutil.rmtree(path, onexc=writable)
        else:
            shutil.rmtree(path, onerror=writable)
    elif path.exists() or path.is_symlink():
        os.chmod(path, stat.S_IWRITE)
        path.unlink()


def prepare(directory, example=None):
    """The tutorials repository (a bare clone, the source), the page outside
    it, and the test driver's steps (tutorials.mitcad-ui), after what an
    earlier run left is removed; and the example's run in a folder of its
    own (prepare_example)."""
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    for name in ("course", "source.git", "cache", "installed", "outside", "profile", "components",
                 "tutorials.mitcad-ui", "tutorials.log", EXAMPLE_RUN):
        remove(directory / name)
    for left in directory.glob("settings-*"):
        remove(left)
    course = directory / "course"
    tutorial = course / "tutorials" / "two-steps"
    tutorial.mkdir(parents=True)
    (course / "mitcad-tutorials.json").write_text(json.dumps(
        {"format": "mitcad-tutorials", "version": 1, "name": "Test tutorials"}), encoding="utf-8")
    (tutorial / "tutorial.json").write_text(json.dumps(
        {"id": "two-steps", "title": "Two steps", "language": "en", "version": "1.0", "min_mitcad": "0.1.0",
         "license": "CC0-1.0", "description": "An extrude, step by step.", "start": "index.html"}),
        encoding="utf-8")
    outside = directory / "outside" / "outside.html"
    outside.parent.mkdir(parents=True)
    outside.write_text(OUTSIDE, encoding="utf-8")
    (tutorial / "index.html").write_text(
        PAGE % {"outside": outside.as_uri(), "steps": json.dumps(STEPS)}, encoding="utf-8")
    git(course, "init", "-q")
    git(course, "add", "-A")
    git(course, "commit", "-q", "-m", "Two steps")
    source = directory / "source.git"
    git(directory, "clone", "-q", "--bare", str(course), str(source))
    (directory / "tutorials.mitcad-ui").write_text(workflow(source.as_posix()), encoding="utf-8")
    prepare_example(directory / EXAMPLE_RUN, example)
    return directory


def prepare_example(directory, example=None):
    """The example tutorials repository as a source of its own (its files
    committed into a repository and cloned bare, as a contributor's clone
    is) and the test driver's steps for its tutorial (example.mitcad-ui).

    Only min_mitcad of the copy's tutorial.json is changed: a development
    build has the version of CMakeLists.txt, older than the release the
    example asks for."""
    example = Path(example).resolve() if example else EXAMPLE
    if not (example / "mitcad-tutorials.json").is_file():
        raise RuntimeError("no example tutorials repository in " + str(example))
    directory.mkdir(parents=True)
    course = directory / "course"
    shutil.copytree(example, course)
    manifest_path = course / "tutorials" / EXAMPLE_TUTORIAL / "tutorial.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    manifest["min_mitcad"] = "0.1.0"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    git(course, "init", "-q")
    git(course, "add", "-A")
    git(course, "commit", "-q", "-m", "The example tutorials")
    source = directory / "source.git"
    git(directory, "clone", "-q", "--bare", str(course), str(source))
    (directory / "example.mitcad-ui").write_text(example_workflow(source.as_posix(), manifest), encoding="utf-8")
    return directory


def workflow(source):
    """The in-process test driver's steps (MITCAD_TEST_INPUT)."""
    block = {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
                                           "corner": [0, 0], "length": 30, "width": 20, "height": 10,
                                           "operation": "new_body"}}
    lines = [
        "wait-idle",
        # The test's repository the only source.
        "set-setting tutorials/sources '%s'" % json.dumps([source]),
        # Help > Tutorials...: the Start page's tutorials (mitcad#2346), a
        # card for the test's tutorial; Start starts it in the empty design
        # under the page.
        "mark",
        "command help.tutorials",
        "expect-log '^Start page section: tutorials$' 20",
        "expect-log '^Start page shown$' 20",
        "expect-log '^Library fetched: tutorials Test tutorials \\(\\) from ' 60",
        "expect-log '^Tutorials: listed two-steps \\(Two steps\\) from .*: not started$' 30",
        "wait-idle",
        "mark",
        "button '^Start$' tutorialStart_two-steps",
        "expect-log '^Tutorials: chosen two-steps \\(start\\)$' 20",
        "expect-log '^Tutorial: installed two-steps 1\\.0 \\(2 files\\) into ' 30",
        "expect-log '^Tutorial: starts in the empty design under the Start page$' 30",
        "expect-log '^Tutorial: opened Two steps ' 30",
        "expect-log '^Start page hidden$' 10",
        "focus-main",
        # The page has the bridge: the application's state, a step of a
        # command MitCad does not have refused, the first step shown.
        "expect-log '^Tutorial page loaded: file:.*/two-steps/index\\.html \\(bridge: yes\\)$' 90",
        "expect-log '^Tutorial page says: state mode=idle$' 30",
        "expect-log '^Tutorial page says: unknown command refused: MitCad has no command no\\.such$' 30",
        "expect-log '^Guide: step extrude started: target command solid\\.extrude, navigation free$' 30",
        "expect-log '^Tutorial page says: started extrude$' 30",
        "expect-widget-property guideCallout visible true",
        "expect-widget-property guideCalloutText text 'Open Extrude.'",
        # A block, in the tutorial's own design: Extrude is offered only in
        # a design with a body, a sketch or a profile.
        "model-command '%s'" % json.dumps(block),
        "wait-idle",
        "expect-model-bodies 1 6000",
        # Escape with no command open ends the step; the input is free (a
        # command the step held back runs); the page shows the step again.
        "mark",
        "focus-main",
        "key Escape",
        "expect-log '^Guide: step extrude left \\(escape\\)$' 20",
        "expect-log '^Tutorial page says: left extrude escape$' 20",
        "expect-widget-property guideCallout visible false",
        "mark",
        "command solid.box",
        "expect-log '^Command Box started' 20",
        "expect-no-log '^Guide: command'",
        "key Escape",
        "expect-log '^Command Box cancelled' 20",
        "wait-idle",
        "mark",
        "invoke-widget tutorialPage clickLink 'Show this step again'",
        "expect-log '^Guide: step extrude started: target command solid\\.extrude, navigation free$' 30",
        "expect-log '^Tutorial page says: started extrude$' 30",
        "focus-main",
        # Another command: refused, logged and heard by the page.
        "mark",
        "command sketch.create",
        "expect-log '^Guide: command sketch\\.create refused during step extrude$' 20",
        "expect-log '^Tutorial page says: refused sketch\\.create$' 20",
        "expect-no-log '^Command Create Sketch started'",
        # The intended command: the step is done, the page gets it and shows
        # the next step.
        "mark",
        "command solid.extrude",
        "expect-log '^Command Extrude started' 20",
        "expect-log '^Guide: step extrude done: the panel of solid\\.extrude opened$' 20",
        "expect-log '^Tutorial page says: done extrude \\{\"started\":\"solid\\.extrude\"\\}$' 20",
        "expect-log '^Guide: step distance started: target input distance, navigation free$' 20",
        # The intended value.
        "mark",
        "type-in '^Panel Extrude input distance at ' '20 mm'",
        "expect-log '^Guide: step distance done: 20 mm typed into distance$' 20",
        "expect-log '^Tutorial page says: done distance \\{\"input\":\"distance\",\"value\":\"20 mm\"\\}$' 20",
        "expect-log '^Guide: step finish started: ' 20",
        # A page outside the tutorial's folder: no bridge.
        "mark",
        "invoke-widget tutorialPage clickLink 'Outside page'",
        "expect-log '^Tutorial page loaded: file:.*/outside/outside\\.html \\(bridge: no\\)$' 60",
        "expect-log '^Tutorial page says: outside bridge=undefined query=undefined$' 30",
        "expect-no-log '^Tutorial bridge: '",
        # Leave Tutorial: the step ends, the pane closes, the input is free.
        "mark",
        "command help.leave_tutorial",
        "expect-log '^Guide: step finish left \\(leave\\)$' 20",
        "expect-log '^Tutorial: pane closed$' 20",
        "expect-widget-property guideCallout visible false",
        "key Escape",
        "expect-log '^Command Extrude cancelled' 20",
        "wait-idle",
        "mark",
        "command sketch.create",
        "sleep 1",
        "expect-no-log '^Guide: command'",
        "key Escape",
        "wait-idle",
        "quit",
    ]
    return "\n".join(lines) + "\n"


def example_workflow(source, manifest):
    """The test driver's steps for the example's tutorial in a new design:
    each step its page shows, done as the step's text asks."""
    lines = (["wait-idle"] + example_open(source, manifest) + example_first_step() + example_steps()
             + ["quit"])
    return "\n".join(lines) + "\n"


def example_listed(manifest, words):
    """The log's line of the example's card on the Start page with its
    progress (`words` a regular expression: TutorialProgress::words)."""
    return "'^Tutorials: listed %s \\(%s\\) from .*: %s$'" % (EXAMPLE_TUTORIAL, re_text(manifest["title"]), words)


def example_open(source, manifest, via="menu"):
    """The example's tutorial shown as a card on the Start page (mitcad#2346)
    by Help > Tutorials... (`via` "menu") or by the page's own Tutorials
    button ("button", the page shown), started from its card, installed and
    opened: the driver's steps up to its page loaded with the bridge."""
    title = manifest["title"]
    lines = [
        # The example's repository the only source.
        "set-setting tutorials/sources '%s'" % json.dumps([source]),
        "mark",
    ]
    if via == "menu":
        lines.append("command help.tutorials")
    else:
        # The Resources' Tutorials, under User Guide, clicked where it is.
        lines += [
            "expect-ever '^Start page resource Tutorials at ' 20",
            "click-logged '^Start page resource Tutorials at '",
            "expect-log '^Start page chosen: Tutorials$' 10",
        ]
    return lines + [
        "expect-log '^Start page section: tutorials$' 20",
        "expect-log '^Library fetched: tutorials Example tutorials \\(\\) from ' 60",
        # Its card, not started; Start is its one action.
        "expect-log %s 30" % example_listed(manifest, "not started"),
        "wait-idle",
        "button '^Start$' tutorialStart_%s" % EXAMPLE_TUTORIAL,
        "expect-log '^Tutorials: chosen %s \\(start\\)$' 20" % EXAMPLE_TUTORIAL,
        "expect-log '^Tutorial: installed %s %s \\([0-9]+ files\\) into ' 30"
        % (EXAMPLE_TUTORIAL, re_text(manifest["version"])),
        "expect-log '^Tutorial: opened %s ' 30" % re_text(title),
        "focus-main",
        "expect-log '^Tutorial page loaded: file:.*/%s/index\\.html \\(bridge: yes\\)$' 90" % EXAMPLE_TUTORIAL,
    ]


def example_first_step():
    """The example's first step shown, and its progress recorded (step 1 of
    its 5, mitcad#2346)."""
    return [
        # 1. Create Sketch, then the XY plane. Its button is outlined: shown
        # in the window, not under the Start page (mitcad#2168).
        "expect-log '^Guide: step sketch started: target command sketch\\.create, navigation free$' 30",
        "expect-log '^Guide: step sketch target at -?[0-9]+,-?[0-9]+ [1-9][0-9]*x[1-9][0-9]*$' 30",
        "expect-log '^Tutorial: progress %s: in progress \\(step 1 of 5\\)$' 30" % EXAMPLE_TUTORIAL,
    ]


def example_steps():
    """Each step of the example's tutorial done as its text asks, to the
    page's end and Leave Tutorial (after example_first_step)."""
    return [
        "expect-widget-property guideCallout visible true",
        # The origin's planes keep their size on the screen; zooming out
        # about a point above the view's middle brings them up and left,
        # clear of the command card that Create Sketch shows at the right
        # (the test screen is only 1024 px wide).
        "wheel 50 25 -3",
        "wait-idle",
        "mark",
        "command sketch.create",
        "expect-log '^Create Sketch: select a plane' 20",
        "expect-log '^Datum xy at ' 20",
        "click-logged '^Datum xy at '",
        "expect-log '^Sketch started on xy' 20",
        "expect-log '^Guide: step sketch done: the mode is sketch$' 20",
        "expect-log '^Tutorial page says: done sketch$' 20",
        # 2. A rectangle with 2-Point Rectangle, then Finish Sketch. The
        # corners are on multiples of 10 mm: on the layout grid, whatever
        # its step. The rectangle goes left of and below the origin: on a
        # 1024 px screen the Floating layout moves the orientation cube left
        # of the command card on the right, and the cube's arrows (shown
        # while the sketch is face on) then sit above and right of the
        # origin, where a corner at (30, 20) clicked the down arrow and
        # turned the view.
        "expect-log '^Guide: step rectangle started: target command sketch\\.finish, navigation free$' 20",
        "mark",
        "command sketch.rectangle",
        "expect-log '^Sketch tool 2-Point Rectangle' 20",
        "wait-idle",
        "sketch-click 0 0",
        "sketch-click -30 -20",
        "expect-log '^Added rectangle (30 x 20|20 x 30) mm ' 20",
        "expect-no-log '^Orientation cube arrow [a-z]+$'",
        "focus-main",
        "key Escape",
        "expect-log '^Sketch tool ended: 2-Point Rectangle' 20",
        "wait-idle",
        "expect-no-log '^Guide: command'",
        "mark",
        "command sketch.finish",
        "expect-log '^Sketch finished' 20",
        "expect-log '^Guide: step rectangle done: the mode is idle$' 20",
        "expect-log '^Tutorial page says: done rectangle$' 20",
        # 3. Extrude, which starts with the sketch's profile.
        "expect-log '^Guide: step extrude started: target command solid\\.extrude, navigation free$' 20",
        "mark",
        "command solid.extrude",
        "expect-log '^Command Extrude started' 20",
        "expect-log '^Guide: step extrude done: the panel of solid\\.extrude opened$' 20",
        "expect-log '^Tutorial page says: done extrude$' 20",
        # 4. 20 mm as the distance.
        "expect-log '^Guide: step distance started: target input distance, navigation free$' 20",
        "mark",
        "type-in '^Panel Extrude input distance at ' '20 mm'",
        "expect-log '^Guide: step distance done: 20 mm typed into distance$' 20",
        "expect-log '^Tutorial page says: done distance$' 20",
        # 5. OK: the block, and the page's end.
        "expect-log '^Guide: step ok started: target command solid\\.extrude, navigation free$' 20",
        "mark",
        "button '^OK$'",
        "expect-log '^Guide: step ok done: command solid\\.extrude finished$' 30",
        "expect-log '^Tutorial page says: done ok$' 20",
        "expect-log '^Tutorial page says: finished$' 20",
        "expect-log '^Tutorial: progress %s: done$' 20" % EXAMPLE_TUTORIAL,
        "expect-no-log '^Guide: command'",
        "expect-model-bodies 1 12000",
        # Leave Tutorial closes the pane.
        "mark",
        "command help.leave_tutorial",
        "expect-log '^Tutorial: pane closed$' 20",
        "wait-idle",
    ]


# The Start page's cases (mitcad#2168): the example's tutorial opened while
# the Start page is shown, with MITCAD_START_PAGE for each (1: the page at
# launch), run in each layout.
START_PAGE_CASES = {"launch": "1", "closed": "0", "modified": "0"}
LAYOUTS = ("floating", "docked")


def start_page_workflow(case, source, manifest):
    """The example's tutorial opened from Help > Tutorials... over the Start
    page (mitcad#2168), and its steps done to its end:
      launch: the page at launch over the window's empty untitled design;
        the page is left for that design, in the same window.
      closed: the last design closed to the page (File > Close); the page is
        left for the empty design that waits under it.
      modified: a design with a block and unsaved changes under the page;
        New Design opens a window of its own for the tutorial, and the
        user's design keeps its block and its changes."""
    block = {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
                                           "corner": [0, 0], "length": 30, "width": 20, "height": 10,
                                           "operation": "new_body"}}
    if case == "launch":
        before = [
            "expect-ever '^Start page at launch: shown$' 30",
            "expect-ever '^Start page shown$' 30",
            "wait-idle",
        ]
        opened = [
            "expect-ever '^Tutorial: starts in the empty design under the Start page$' 10",
            "expect-document-windows 1",
        ]
    elif case == "closed":
        before = [
            "wait-idle",
            "mark",
            "command file.close",
            "wait-log '^Last design closed: Start page$' 20",
            "wait-log '^Start page shown$' 10",
            "wait-idle",
            "expect-document-frame-design none",
        ]
        opened = [
            "expect-ever '^Tutorial: starts in the empty design under the Start page$' 10",
            "expect-ever '^Design shown again after the last design closed$' 10",
            "expect-document-frame-design open",
            "expect-document-windows 1",
        ]
    elif case == "modified":
        before = [
            "wait-idle",
            "model-command '%s'" % json.dumps(block),
            "wait-idle",
            "expect-model-bodies 1 6000",
            "expect-document-modified yes",
            "capture-document-window user",
            "mark",
            "click-logged '^Ribbon tab START at '",
            "wait-log '^Start page shown$' 10",
            "wait-idle",
        ]
        opened = [
            "expect-ever '^Tutorial: starts in a new design$' 10",
            "expect-no-log '^Tutorial: starts in the empty design under the Start page$'",
            "expect-document-windows 2",
            "capture-document-window tutorial",
            "expect-document-window-active tutorial",
            "expect-model-bodies 0 0",
        ]
    else:
        raise RuntimeError("no Start page case " + case)
    # Opened over the page: never by the page's controls, never held back.
    opened.append("expect-no-log '^Guide: input held back on '")
    after = []
    if case == "modified":
        # The user's design as it was: its block, its one feature, its
        # unsaved changes; the tutorial's block in the tutorial's window.
        # The user's design is shown again with its Start page's tutorials,
        # where the card now says the tutorial is done (mitcad#2346).
        after = [
            "expect-model-bodies 1 12000",
            "mark",
            "focus-document-window user",
            "focus-main",
            "wait-idle",
            "expect-log %s 20" % example_listed(manifest, "done"),
            "expect-model-bodies 1 6000",
            "capture-model-number features '{\"query\": \"document\"}' features",
            "expect-near ${features} 1",
            "expect-document-modified yes",
        ]
    # The Start page's own Tutorials button shows the cards (mitcad#2346).
    open_lines = example_open(source, manifest, via="button")
    # The window's design choice is logged once the tutorial is installed,
    # before its page is loaded.
    split = next(i for i, line in enumerate(open_lines) if line.startswith("expect-log '^Tutorial: opened "))
    # Once its first step is shown, its card says it is in progress.
    in_progress = "expect-log %s 20" % example_listed(manifest, "in progress \\(step 1 of 5\\)")
    if case == "modified":
        # The tutorial's design is another tab of the window: the user's
        # design, with its Start page's tutorials, is out of sight, and its
        # card is read again when it shows (at the end, Done).
        progress = []
    else:
        # The tutorial took this window's design: its Start page shows the
        # tutorials again, then the design is shown again.
        progress = [
            "mark",
            "click-logged '^Ribbon tab START at '",
            "wait-log '^Start page shown$' 10",
            in_progress,
            "wait-idle",
            "mark",
            "leave-start-page",
            "wait-log '^Start page hidden$' 10",
            "wait-idle",
        ]
    lines = (before + open_lines[:split + 1] + opened + open_lines[split + 1:] + example_first_step() + progress
             + example_steps() + after + ["quit"])
    return "\n".join(lines) + "\n"


def re_text(text):
    """`text` as a regular expression of its own (the test driver's quoted
    argument)."""
    if "'" in text:
        raise RuntimeError("a quote in a text the test driver matches: " + text)
    return "".join("\\" + c if c in ".^$*+?()[]{}|\\" else c for c in text)


def environment_for(directory, temporary, qt_bin=None, component=None):
    environment = dict(os.environ)
    if qt_bin:
        environment["PATH"] = qt_bin + os.pathsep + environment.get("PATH", "")
    for variable in ("SETTINGS", "AUTOSAVE", "PROJECTS", "THUMBNAIL", "CRASH"):
        environment["MITCAD_" + variable + "_DIR"] = str(Path(temporary) / variable.lower())
    environment.update(MITCAD_RESULT_STORE="off", MITCAD_START_PAGE="0", MITCAD_NEW_DESIGN_VIEW="0",
                       MITCAD_LANGUAGE="en", MITCAD_NO_UPDATE_CHECK="1", QT_FORCE_STDERR_LOGGING="1",
                       MITCAD_REMEMBER_PANEL_VALUES="0", MITCAD_LOG_TYPED_TEXT="1",
                       # The places of the datums' clicks tried as a click lands
                       # (MainWindow::logDatumPlaces: clear of the cards, the
                       # command panel, the orientation cube and the guide's
                       # callout), as ui-test-lib.sh, ui-windows-lib.ps1 and
                       # the macOS wrappers' ui-macos-lib.sh have them; without
                       # it the XY plane's place is its first candidate, unchecked
                       # (mitcad#2311).
                       MITCAD_LOG_PICKS="1",
                       MITCAD_LIBRARIES_DIR=str(directory / "cache"),
                       MITCAD_TUTORIALS_DIR=str(directory / "installed"),
                       MITCAD_WEB_PROFILE_DIR=str(directory / "profile"),
                       # Never the user's installed components.
                       MITCAD_BROWSER_COMPONENTS_DIR=str(directory / "components"))
    # The web browser component the build made (no download).
    if component:
        environment["MITCAD_BROWSER_COMPONENT_DIR"] = str(component)
    return environment


def verify(directory):
    """The tutorials installed with their records of where they came from:
    the test's own, and the example's as the example has it."""
    installed = Path(directory) / "installed" / "two-steps"
    record = json.loads((installed / ".mitcad-tutorial-source.json").read_text(encoding="utf-8"))
    if record.get("version") != "1.0" or not (installed / "index.html").is_file():
        raise RuntimeError("the tutorial was not installed as expected: " + json.dumps(record))
    example = Path(directory) / EXAMPLE_RUN
    installed = example / "installed" / EXAMPLE_TUTORIAL
    record = json.loads((installed / ".mitcad-tutorial-source.json").read_text(encoding="utf-8"))
    given = example / "course" / "tutorials" / EXAMPLE_TUTORIAL
    manifest = json.loads((given / "tutorial.json").read_text(encoding="utf-8"))
    if (record.get("version") != manifest["version"]
            or (installed / "index.html").read_bytes() != (given / "index.html").read_bytes()):
        raise RuntimeError("the example's tutorial was not installed as expected: " + json.dumps(record))


def prepare_start_page(directory, example=None):
    """The Start page's runs (mitcad#2168): the example's repository as the
    source (source/), the driver's steps of each case (<case>.mitcad-ui) and
    a folder of its own for each case in each layout (<case>-<layout>: its
    cache, installed tutorials, profile and log), after what an earlier run
    left is removed."""
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    names = ["source"] + [case + ".mitcad-ui" for case in START_PAGE_CASES]
    names += [case + "-" + layout for case in START_PAGE_CASES for layout in LAYOUTS]
    for name in names:
        remove(directory / name)
    prepare_example(directory / "source", example)
    manifest = json.loads((directory / "source" / "course" / "tutorials" / EXAMPLE_TUTORIAL / "tutorial.json")
                          .read_text(encoding="utf-8"))
    source = (directory / "source" / "source.git").as_posix()
    for case in START_PAGE_CASES:
        (directory / (case + ".mitcad-ui")).write_text(start_page_workflow(case, source, manifest),
                                                       encoding="utf-8")
        for layout in LAYOUTS:
            (directory / (case + "-" + layout)).mkdir()
    return directory


def run_app(app, directory, name, qt_bin=None, component=None, script=None, arguments=(), start_page="0"):
    """The application with the driver's steps (<name>.mitcad-ui unless
    `script` is given), its sources and folders those of `directory`, the
    Start page at launch by `start_page` (MITCAD_START_PAGE); an error when
    it fails."""
    script = script or directory / (name + ".mitcad-ui")
    with tempfile.TemporaryDirectory(prefix="settings-", dir=directory) as temporary:
        environment = environment_for(directory, temporary, qt_bin, component)
        environment["MITCAD_TEST_INPUT"] = str(script)
        environment["MITCAD_TEST_STEP_MS"] = "100"
        environment["MITCAD_START_PAGE"] = start_page
        log_path = directory / (name + ".log")
        with log_path.open("wb") as log:
            try:
                result = subprocess.run([app, "--ui=qt", "--no-native-dialogs", "--no-recovery", "--no-mcp",
                                         *arguments],
                                        env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=RUN_TIMEOUT)
            except subprocess.TimeoutExpired:
                raise RuntimeError("the tutorials' UI workflow did not end in %d s: %s"
                                   % (RUN_TIMEOUT, log_path)) from None
        text = log_path.read_text(encoding="utf-8", errors="replace")
        if (result.returncode or any(line.startswith("TestDriver: FAIL") for line in text.splitlines())
                or "TestDriver: quit" not in text):
            raise RuntimeError("the tutorials' UI workflow failed: " + str(log_path))


def run(app, cli, directory, qt_bin=None, component=None, example=None):
    del cli  # the same arguments as the other UI tests; nothing to build here
    directory = prepare(directory, example)
    run_app(app, directory, "tutorials", qt_bin, component)
    run_app(app, directory / EXAMPLE_RUN, "example", qt_bin, component)
    verify(directory)
    print("PASS: Tutorials: listed and opened from Help > Tutorials..., the bridge's steps, a command refused, "
          "the steps completed with their events, no bridge outside the tutorial, Leave Tutorial frees the input; "
          "the example tutorial's steps done to its end")


def look_workflow(source, manifest, picture):
    """The Start page's tutorials (mitcad#2346) shown by its Tutorials button
    with the example's card, saved as `picture`; then the example started
    and the page shown again, its card in progress, saved as
    <picture>-in-progress.png."""
    stem = picture[:-4] if picture.endswith(".png") else picture
    open_lines = example_open(source, manifest, via="button")
    # The cards shown, before the card's Start.
    at = next(i for i, line in enumerate(open_lines) if line.startswith("button "))
    lines = (["expect-ever '^Start page shown$' 30", "wait-idle"] + open_lines[:at]
             + ["sleep 1000", "screenshot '%s'" % picture] + open_lines[at:] + example_first_step())
    lines += [
        "mark",
        "click-logged '^Ribbon tab START at '",
        "wait-log '^Start page shown$' 10",
        "expect-log %s 20" % example_listed(manifest, "in progress \\(step 1 of 5\\)"),
        "wait-idle",
        "sleep 1000",
        "screenshot '%s-in-progress.png'" % stem,
        "quit",
    ]
    return "\n".join(lines) + "\n"


def run_look(app, directory, qt_bin=None, component=None, example=None):
    """Pictures of the Start page's tutorials for a look (not a ctest): in
    the Floating and the Docked layout, light and dark (Qt's GTK platform
    theme on Linux, MITCAD_TEST_COLOR_SCHEME), as
    <directory>/tutorials-<layout>-<theme>.png and
    tutorials-<layout>-<theme>-in-progress.png."""
    directory = prepare_start_page(directory, example)
    manifest = json.loads((directory / "source" / "course" / "tutorials" / EXAMPLE_TUTORIAL / "tutorial.json")
                          .read_text(encoding="utf-8"))
    source = (directory / "source" / "source.git").as_posix()
    for layout in LAYOUTS:
        for theme in ("light", "dark"):
            name = "look-%s-%s" % (layout, theme)
            run_directory = directory / name
            remove(run_directory)
            run_directory.mkdir()
            picture = (directory / ("tutorials-%s-%s.png" % (layout, theme))).as_posix()
            script = run_directory / (name + ".mitcad-ui")
            script.write_text(look_workflow(source, manifest, picture), encoding="utf-8")
            os.environ["MITCAD_TEST_COLOR_SCHEME"] = theme
            if sys.platform.startswith("linux"):
                os.environ["QT_QPA_PLATFORMTHEME"] = "gtk3"
            run_app(app, run_directory, name, qt_bin, component, script=script,
                    arguments=("--chrome=" + layout,), start_page="1")
            print("ok   " + picture)


def run_start_page(app, cli, directory, qt_bin=None, component=None, example=None):
    """The example's tutorial opened over the Start page (mitcad#2168) in
    each case and layout, each run in a folder of its own."""
    del cli  # the same arguments as the other UI tests; nothing to build here
    directory = prepare_start_page(directory, example)
    for case, start_page in START_PAGE_CASES.items():
        for layout in LAYOUTS:
            name = case + "-" + layout
            run_app(app, directory / name, name, qt_bin, component, script=directory / (case + ".mitcad-ui"),
                    arguments=("--chrome=" + layout,), start_page=start_page)
    print("PASS: Tutorials over the Start page: at launch, after the last design closed and over a modified "
          "design, in the Floating and the Docked layout, each in an empty design of its own with Create "
          "Sketch outlined and the example tutorial done to its end; the modified design kept as it was")


if __name__ == "__main__":
    if sys.argv[1] == "prepare":
        prepare(*sys.argv[2:4])
    elif sys.argv[1] == "verify":
        verify(sys.argv[2])
    elif sys.argv[1] == "start-page":
        run_start_page(*sys.argv[2:])
    elif sys.argv[1] == "prepare-start-page":
        prepare_start_page(*sys.argv[2:4])
    elif sys.argv[1] == "look":
        run_look(*sys.argv[2:])
    else:
        run(*sys.argv[1:])
