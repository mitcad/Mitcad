# SPDX-License-Identifier: MIT
# check-all sources: app/tutorials app/webbrowser core/vcs/src/library tools/cli
# check-all sources: app/MainWindowStart.cpp app/WindowManager.cpp app/files/StartPage.cpp
# check-all sources: core/model-api/src/tutorial_bridge.rs
# check-all sources: app/framework/PaneMotion.cpp app/framework/FloatingLayout.cpp
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

Then, in a third run of its own, a tutorial whose page reads the design
through the bridge's query (mitcad#2371): with no sketch open the open
sketch is refused; in an open sketch a 30 x 30 mm square held by its two
dimensions is free to move, and with its corner fixed fully constrained;
the last extrude of a cube with a circle sketched under it is a cut
through, a blind pocket or a new body (with the bodies and their volume);
a command, another design and a query not on the list are refused in
words.

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
dark, for a look; each run must log the scheme it asked for as in effect
(mitcad#2404: under Xvfb the platform may keep its light palette, and
MitCad then sets its own dark one).

With quit (mitcad#2394, a test of its own): MitCad quits with the example
tutorial's page open, started from the Start page's card at launch, its
first step started (in the Docked layout the Start page shown again over
it). CEF ends at the shutdown before the window and its tutorial pane go;
the run must end with exit code 0 and no fatal check of the browser
component. Every run of every mode fails on such a line in its log.

With hold (mitcad#2368, a test of its own): a tutorial of the test's own
whose step with "hold" is done as the rectangle tool is picked in a sketch;
the page hears it once, the callout goes, clicks in the view with the tool
open add no geometry and a command is refused; the page's Continue starts
the next step, which releases the hold, and its rectangle is drawn; Finish
Sketch completes another step with hold, and Leave Tutorial during it frees
all input (a command and its pick in the view run again).

With placement (mitcad#2367, a test of its own): a tutorial of the test's
own whose page places its pane with window.mitcad.place, in the Floating
and the Docked layout and once more in the Floating layout with reduced
motion (MITCAD_REDUCED_MOTION=1): a place MitCad does not have is refused;
centred for its introduction (most of the window, the view held), at the
left, at the right with a step whose Browser row is clicked and picked
there (nothing held back), centred again for the review when the step is
done, then at the right for the next step. Each move animated (250 ms), or
at once with reduced motion; Leave Tutorial visible in every place; the
page never loaded again (its count of moves goes on); Leave Tutorial puts
the pane back at the left, hidden.

Usage: ui-tutorials-test.py <app> <cli> <output folder> [<Qt's bin> [<the built web browser component> [<example>]]]
       ui-tutorials-test.py hold <app> <cli> <output folder> [<Qt's bin> [<component>]]
       ui-tutorials-test.py start-page <app> <cli> <output folder> [<Qt's bin> [<component> [<example>]]]
       ui-tutorials-test.py quit <app> <cli> <output folder> [<Qt's bin> [<component> [<example>]]]
       ui-tutorials-test.py placement <app> <cli> <output folder> [<Qt's bin> [<component>]]
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
import re
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
# The tutorial whose page reads the design through the bridge's query
# (mitcad#2371), and the folder of its run.
READING_TUTORIAL = "reading"
READING_RUN = "reading"
# The longest one run of the application may take, in seconds; the ctests'
# TIMEOUTs (cmake/SelectionUiTests.cmake) leave room for all their runs at
# this length (three runs, the Start page's six), so that a run that hangs is
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


# The reading tutorial's page (mitcad#2371): each link reads the design
# through window.mitcad and says what a lesson would conclude.
READING_PAGE = """<!doctype html>
<html><head><meta charset="utf-8"><title>Reading the design</title></head><body>
<h1>Reading the design</h1>
<p><a href="#square" onclick="checkSquare(); return false;">Check the square</a></p>
<p><a href="#cut" onclick="checkCut(); return false;">Check the cut</a></p>
<p><a href="#refusals" onclick="checkRefusals(); return false;">Check the refusals</a></p>
<script>
function say(text) { console.log('mitcad:tutorial:' + text); }
function near(a, b) { return Math.abs(a - b) < 1e-6; }
// The open sketch: a 30 x 30 mm square, free to move or fully constrained.
function checkSquare() {
  mitcad.query({query: 'sketch'}).then(function (sketch) {
    var thirty = sketch.dimensions.filter(function (d) { return !d.driven && near(d.value, 30); }).length;
    say('square lines=' + (sketch.curves.line || 0) + ' thirty=' + thirty + ' dof=' + sketch.dof + ' '
        + (sketch.fully_constrained ? 'fully constrained' : 'free to move') + ' open=' + sketch.open);
  }, function (error) { say('square refused: ' + error.message); });
}
// The last extrude: a cut through the cube, a blind pocket or a new body.
function checkCut() {
  Promise.all([mitcad.query({query: 'feature', type: 'extrude'}), mitcad.query({query: 'bodies'})])
    .then(function (answers) {
      var cut = answers[0], bodies = answers[1];
      var verdict = cut.operation !== 'cut' ? 'new body'
          : cut.extent.type === 'through_all' ? 'through' : 'blind pocket';
      var volume = bodies.bodies.reduce(function (sum, body) { return sum + body.volume; }, 0);
      say('cut ' + verdict + ': bodies=' + bodies.count + ' solids=' + bodies.solids
          + ' volume=' + Math.round(volume));
    }, function (error) { say('cut refused: ' + error.message); });
}
// A command, another design and a query not on the list: each refused.
function checkRefusals() {
  mitcad.call('query', {cmd: 'delete_feature', uid: 'F1'})
    .then(function () { say('write answered'); }, function (error) { say('write refused: ' + error.message); })
    .then(function () { return mitcad.query({query: 'bodies', design: 'Another design'}); })
    .then(function () { say('other design answered'); },
          function (error) { say('other design refused: ' + error.message); })
    .then(function () { return mitcad.query({query: 'timeline'}); })
    .then(function () { say('timeline answered'); }, function (error) { say('timeline refused: ' + error.message); });
}
say('reading page ' + (window.mitcad && typeof window.mitcad.query === 'function' ? 'with query' : 'without query'));
</script>
</body></html>
"""


# The hints' tutorial (mitcad#2366): its page draws its own mouse and
# keycaps as SVG and plays them over the step's target with
# mitcad.demonstrate; the run's settings choose Alt + left to orbit
# (HINTS_SETTINGS).
HINTS_PAGE = r"""<!doctype html>
<html><head><meta charset="utf-8"><title>Hints</title></head><body>
<h1>Hints</h1>
<p><a href="#row" id="row">Start the Browser step</a></p>
<p><a href="#stop" id="stop">Stop the hint</a></p>
<p><a href="#show" id="show">Show the hint</a></p>
<script>
function say(text) { console.log('mitcad:tutorial:' + text); }
function svg(width, height, body) {
  return 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="' + width +
    '" height="' + height + '">' + body + '</svg>');
}
// The page's own mouse at x, with one button lit.
function mouseBody(x, lit) {
  var body = '<rect x="' + (x + 1) + '" y="1" width="30" height="44" rx="14" fill="#ffffff" stroke="#202020"' +
    ' stroke-width="2"/><line x1="' + (x + 16) + '" y1="1" x2="' + (x + 16) + '" y2="18" stroke="#202020"' +
    ' stroke-width="2"/><line x1="' + (x + 1) + '" y1="18" x2="' + (x + 31) + '" y2="18" stroke="#202020"' +
    ' stroke-width="2"/>';
  var lights = {left: [x + 3, 3, 12, 14], middle: [x + 13, 5, 6, 10], right: [x + 18, 3, 12, 14]};
  if (lit && lights[lit]) {
    var r = lights[lit];
    body += '<rect x="' + r[0] + '" y="' + r[1] + '" width="' + r[2] + '" height="' + r[3] +
      '" rx="3" fill="#2a9d5c"/>';
  }
  return body;
}
function mouse(lit) { return svg(32, 46, mouseBody(0, lit)); }
function capital(name) { return name.charAt(0).toUpperCase() + name.slice(1); }
// The keycaps of the keys held, then the mouse with its button lit.
function gesturePicture(gesture) {
  var x = 0;
  var body = '';
  gesture.modifiers.forEach(function (name) {
    var width = 16 + 9 * name.length;
    body += '<rect x="' + (x + 1) + '" y="9" width="' + width + '" height="26" rx="5" fill="#f0f0f0"' +
      ' stroke="#202020" stroke-width="2"/><text x="' + (x + 9) + '" y="27" font-family="sans-serif"' +
      ' font-size="14" fill="#202020">' + capital(name) + '</text>';
    x += width + 6;
  });
  return svg(x + 32, 46, body + mouseBody(x, gesture.input === 'scroll' ? 'middle' : gesture.input));
}
function gestureWords(gesture) {
  var how = gesture.input === 'scroll' ? 'scroll' : 'drag with the ' + gesture.input + ' button';
  return gesture.modifiers.length ? 'Hold ' + gesture.modifiers.map(capital).join(' + ') + ' and ' + how
                                  : capital(how);
}
function gestureText(gesture) {
  return gesture.input + (gesture.modifiers.length ? '+' + gesture.modifiers.join('+') : '');
}
// The row's hint: the mouse comes from below right, clicks with its left
// button there and lets go.
function rowHint(repeat) {
  return {label: 'Click Body1 with the left button', repeat: repeat, pause: 400, frames: [
    {image: mouse(''), offset: [70, 50], ms: 500},
    {image: mouse(''), move: true, ms: 700},
    {image: mouse('left'), ms: 500},
    {image: mouse(''), ms: 400}]};
}
function demonstrate(hint) {
  return mitcad.demonstrate(hint).then(function () { say('demonstrating'); },
                                       function (error) { say('demonstration refused: ' + error.message); });
}
var steps = {
  row: {id: 'row', text: 'Click Body1 in the Browser.', target: {browser: 'Body1'}, done: {pick: 'body'}},
  orbit: {id: 'orbit', text: 'Turn the view as the picture shows.', target: {view: true},
          done: {command: 'view.home'}}
};
if (!window.mitcad) {
  say('no bridge');
} else {
  mitcad.on('held', function (event) {
    say('held ' + event.place + ' ' + event.input + ' ' + (event.button || ''));
    // Held back: the hint at once, until the page stops it.
    if (event.step === 'row') { demonstrate(rowHint(0)); }
  });
  mitcad.on('demonstration_ended', function (event) {
    say('demonstration ended ' + event.step + ' ' + event.reason);
  });
  mitcad.on('step_done', function (event) {
    say('done ' + event.step + ' ' + JSON.stringify(event.done));
    if (event.step !== 'row') { return; }
    mitcad.startStep(steps.orbit).then(function () { return mitcad.navigation(); }).then(function (navigation) {
      var gesture = navigation.orbit[0];
      say('orbit gesture ' + gestureText(gesture));
      var picture = gesturePicture(gesture);
      return demonstrate({label: gestureWords(gesture), repeat: 0, frames: [
        {image: picture, at: [0.4, 0.5], anchor: [0.5, 0.5], ms: 300},
        {image: picture, at: [0.6, 0.5], anchor: [0.5, 0.5], move: true, ms: 800},
        {image: picture, at: [0.4, 0.5], anchor: [0.5, 0.5], move: true, ms: 800}]});
    });
  });
  document.getElementById('row').onclick = function () {
    mitcad.startStep(steps.row).then(function () {
      say('started row');
      // The page's own delay, then the hint once.
      setTimeout(function () { demonstrate(rowHint(1)); }, 300);
    });
    return false;
  };
  document.getElementById('stop').onclick = function () {
    mitcad.stopDemonstration().then(function (answer) { say('stopped ' + answer.stopped); });
    return false;
  };
  document.getElementById('show').onclick = function () { demonstrate(rowHint(0)); return false; };
  // The learner's keys and gestures; a demonstration with no step shown is
  // refused.
  mitcad.shortcut('view.front').then(function (answer) {
    say('shortcut view.front ' + answer.shortcuts.map(function (shortcut) { return shortcut.text; }).join(' '));
  });
  mitcad.shortcut('no.such').then(function () { say('shortcut of no.such'); },
                                  function (error) { say('shortcut refused: ' + error.message); });
  mitcad.navigation().then(function (navigation) { say('navigation orbit ' + gestureText(navigation.orbit[0])); });
  demonstrate(rowHint(1));
}
</script>
</body></html>
"""

# The hints' run's settings: Alt + left orbits (not the default scheme, and
# with a key held).
HINTS_SETTINGS = "[view]\nnavigationScheme=alt-buttons\n"


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
                 "tutorials.mitcad-ui", "tutorials.log", EXAMPLE_RUN, READING_RUN):
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
    prepare_reading(directory / READING_RUN)
    return directory


def prepare_reading(directory):
    """The reading tutorial (mitcad#2371) in a tutorials repository of its
    own (cloned bare, the run's only source) and the test driver's steps for
    it (reading.mitcad-ui), in a folder of its own."""
    directory.mkdir(parents=True)
    course = directory / "course"
    tutorial = course / "tutorials" / READING_TUTORIAL
    tutorial.mkdir(parents=True)
    (course / "mitcad-tutorials.json").write_text(json.dumps(
        {"format": "mitcad-tutorials", "version": 1, "name": "Reading tests"}), encoding="utf-8")
    (tutorial / "tutorial.json").write_text(json.dumps(
        {"id": READING_TUTORIAL, "title": "Reading the design", "language": "en", "version": "1.0",
         "min_mitcad": "0.1.0", "license": "CC0-1.0", "description": "A page that checks the design.",
         "start": "index.html"}), encoding="utf-8")
    (tutorial / "index.html").write_text(READING_PAGE, encoding="utf-8")
    git(course, "init", "-q")
    git(course, "add", "-A")
    git(course, "commit", "-q", "-m", "Reading the design")
    source = directory / "source.git"
    git(directory, "clone", "-q", "--bare", str(course), str(source))
    (directory / "reading.mitcad-ui").write_text(reading_workflow(source.as_posix()), encoding="utf-8")
    return directory


def reading_workflow(source):
    """The test driver's steps for the reading tutorial (mitcad#2371): its
    page reads the open sketch, a 30 x 30 mm square, free to move and then
    fully constrained; then the last extrude of a cube with a circle sketched
    under it, a cut through, a blind pocket and a new body; and is refused a
    command, another design and a query not on the list."""
    def command(entry):
        return "model-command '%s'" % json.dumps(entry)

    def extrude(extent, operation):
        definition = {"type": "extrude", "profiles": [{"sketch": "${circle}", "region": "${hole}"}],
                      "extent": extent, "operation": operation}
        if operation == "cut":
            definition["participants"] = ["${cube}"]
        return definition

    def check(link, said):
        return [
            "wait-idle",
            "mark",
            "invoke-widget tutorialPage clickLink '%s'" % link,
            "expect-log '^Tutorial page says: %s$' 30" % said,
        ]

    lines = [
        "wait-idle",
        "set-setting tutorials/sources '%s'" % json.dumps([source]),
        "mark",
        "command help.tutorials",
        "expect-log '^Start page section: tutorials$' 20",
        "expect-log '^Library fetched: tutorials Reading tests \\(\\) from ' 60",
        "expect-log '^Tutorials: listed %s \\(Reading the design\\) from .*: not started$' 30" % READING_TUTORIAL,
        "wait-idle",
        "mark",
        "button '^Start$' tutorialStart_%s" % READING_TUTORIAL,
        "expect-log '^Tutorial: installed %s 1\\.0 \\(2 files\\) into ' 30" % READING_TUTORIAL,
        "expect-log '^Tutorial: opened Reading the design ' 30",
        "focus-main",
        "expect-log '^Tutorial page loaded: file:.*/%s/index\\.html \\(bridge: yes\\)$' 90" % READING_TUTORIAL,
        "expect-log '^Tutorial page says: reading page with query$' 30",
    ]
    # No sketch open: the open sketch is refused in words.
    lines += check("Check the square", "square refused: No sketch is open; .*")
    # A sketch on the XY plane, open in sketch mode (the planes zoomed clear
    # of the command card, as the example's steps do), with a 30 x 30 mm
    # square: its first corner fixed, then freed, so that only its two
    # dimensions hold it.
    lines += [
        "wheel 50 25 -3",
        "wait-idle",
        "mark",
        "command sketch.create",
        "expect-log '^Create Sketch: select a plane' 20",
        "expect-log '^Datum xy at ' 20",
        "click-logged '^Datum xy at '",
        "expect-log '^Sketch started on xy' 20",
        "wait-idle",
        "capture-model-text sketch '{\"query\": \"timeline\"}' features.0.uid",
        command({"cmd": "sketch.add_rectangle", "sketch": "${sketch}", "corner": [0, 0], "width": 30, "height": 30}),
        "wait-idle",
        # The fixed first corner: the first line's start.
        "capture-model-text corner '{\"query\": \"sketch\", \"uid\": \"${sketch}\"}' entities.0.start",
        command({"cmd": "sketch.set_fixed", "sketch": "${sketch}", "entities": ["${corner}"], "fixed": False}),
    ]
    lines += check("Check the square", "square lines=4 thirty=2 dof=2 free to move open=true")
    lines += [command({"cmd": "sketch.set_fixed", "sketch": "${sketch}", "entities": ["${corner}"]})]
    lines += check("Check the square", "square lines=4 thirty=2 dof=0 fully constrained open=true")
    # The square extruded 30 mm into a cube, a 10 mm circle sketched in the
    # middle of its bottom face's plane, and an Extrude Cut through it.
    lines += [
        "mark",
        "command sketch.finish",
        "expect-log '^Sketch finished' 20",
        "wait-idle",
        "capture-model-text square '{\"query\": \"profiles\", \"sketch\": \"${sketch}\"}' 0.region",
        command({"cmd": "add_feature", "def": {"type": "extrude",
                                               "profiles": [{"sketch": "${sketch}", "region": "${square}"}],
                                               "extent": {"type": "distance", "distance": 30},
                                               "operation": "new_body"}}),
        "wait-idle",
        "capture-model-text cube '{\"query\": \"bodies\"}' 0.uid",
        command({"cmd": "sketch.create"}),
        "wait-idle",
        "capture-model-text circle '{\"query\": \"timeline\"}' features.2.uid",
        command({"cmd": "sketch.add_circle", "sketch": "${circle}", "center": [15, 15], "diameter": 10}),
        "wait-idle",
        "capture-model-text hole '{\"query\": \"profiles\", \"sketch\": \"${circle}\"}' 0.region",
        command({"cmd": "add_feature", "def": extrude({"type": "through_all"}, "cut")}),
        "wait-idle",
        "capture-model-text cut '{\"query\": \"timeline\"}' features.3.uid",
    ]
    # 27000 mm³ less a 10 mm hole 30 mm deep.
    lines += check("Check the cut", "cut through: bodies=1 solids=1 volume=24644")
    # A blind pocket 10 mm deep, then a new cylinder 40 mm tall beside the cube.
    lines += [command({"cmd": "edit_feature", "uid": "${cut}",
                       "def": extrude({"type": "distance", "distance": 10}, "cut")})]
    lines += check("Check the cut", "cut blind pocket: bodies=1 solids=1 volume=26215")
    lines += [command({"cmd": "edit_feature", "uid": "${cut}",
                       "def": extrude({"type": "distance", "distance": 40}, "new_body")})]
    lines += check("Check the cut", "cut new body: bodies=2 solids=2 volume=30142")
    # A command, another design and a query not on the list: refused in
    # words, and the design as it was.
    lines += [
        "wait-idle",
        "mark",
        "invoke-widget tutorialPage clickLink 'Check the refusals'",
        "expect-log '^Tutorial page says: write refused: A tutorial page only reads the design: it cannot run "
        "delete_feature; the user makes every change$' 30",
        "expect-log '^Tutorial page says: other design refused: A tutorial page reads only the design of its own "
        "window \\(untitled\\), not Another design$' 30",
        "expect-log '^Tutorial page says: timeline refused: A tutorial page cannot read the query timeline; it "
        "reads sketch, feature and bodies$' 30",
        "expect-no-log '^Tutorial page says: .* answered$'",
        "wait-idle",
        "capture-model-number bodies '{\"query\": \"document\"}' bodies",
        "expect-near ${bodies} 2",
        # Leave Tutorial closes the pane.
        "mark",
        "command help.leave_tutorial",
        "expect-log '^Tutorial: pane closed$' 20",
        "wait-idle",
        "quit",
    ]
    return "\n".join(lines) + "\n"


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


def run_app(app, directory, name, qt_bin=None, component=None, script=None, arguments=(), start_page="0",
            extra_environment=None, settings=None):
    """The application with the driver's steps (<name>.mitcad-ui unless
    `script` is given), its sources and folders those of `directory`, the
    Start page at launch by `start_page` (MITCAD_START_PAGE), the variables
    of `extra_environment` and its settings file `settings` (INI text) when
    given; an error when it fails."""
    script = script or directory / (name + ".mitcad-ui")
    with tempfile.TemporaryDirectory(prefix="settings-", dir=directory) as temporary:
        environment = environment_for(directory, temporary, qt_bin, component)
        if settings:
            # The stored names Mitcad/Mitcad (main.cpp useStoredSettings).
            ini = Path(environment["MITCAD_SETTINGS_DIR"]) / "Mitcad" / "Mitcad.ini"
            ini.parent.mkdir(parents=True, exist_ok=True)
            ini.write_text(settings, encoding="utf-8")
        environment["MITCAD_TEST_INPUT"] = str(script)
        environment["MITCAD_TEST_STEP_MS"] = "100"
        environment["MITCAD_START_PAGE"] = start_page
        environment.update(extra_environment or {})
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
        lines = text.splitlines()
        if (result.returncode or any(line.startswith("TestDriver: FAIL") for line in lines)
                or "TestDriver: quit" not in text):
            raise RuntimeError("the tutorials' UI workflow failed (exit code %d): %s"
                               % (result.returncode, log_path))
        # The browser component's fatal checks (mitcad#2394: its message
        # router released off CEF's UI thread after CEF ended), whatever the
        # exit code says.
        fatal = [line for line in lines if ":FATAL:" in line or "Check failed:" in line]
        if fatal:
            raise RuntimeError("the application logged a fatal check: %s (%s)" % (fatal[0], log_path))
        return text


def run(app, cli, directory, qt_bin=None, component=None, example=None):
    del cli  # the same arguments as the other UI tests; nothing to build here
    directory = prepare(directory, example)
    run_app(app, directory, "tutorials", qt_bin, component)
    run_app(app, directory / EXAMPLE_RUN, "example", qt_bin, component)
    run_app(app, directory / READING_RUN, "reading", qt_bin, component)
    verify(directory)
    print("PASS: Tutorials: listed and opened from Help > Tutorials..., the bridge's steps, a command refused, "
          "the steps completed with their events, no bridge outside the tutorial, Leave Tutorial frees the input; "
          "the example tutorial's steps done to its end; a page that reads the design tells a free square from "
          "a fully constrained one and a cut through from a blind pocket and a new body, and is refused a "
          "command and another design")


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
    theme on Linux, MITCAD_TEST_COLOR_SCHEME, or MitCad's own palette where
    that theme keeps its light one), as
    <directory>/tutorials-<layout>-<theme>.png and
    tutorials-<layout>-<theme>-in-progress.png; the scheme in effect checked
    in each run's log."""
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
            text = run_app(app, run_directory, name, qt_bin, component, script=script,
                           arguments=("--chrome=" + layout,), start_page="1")
            print("ok   %s (%s)" % (picture, scheme_in_effect(text, theme, run_directory / (name + ".log"))))


def scheme_in_effect(text, theme, log_path):
    """The app's line for the colour scheme in effect after the test asked
    for `theme` (mitcad#2404: under Xvfb the platform may keep its light
    palette, and MitCad then sets its own); an error when the scheme in
    effect is another one or the line is missing."""
    lines = [line for line in text.splitlines() if line.startswith("Colour scheme in effect: ")]
    if not lines:
        raise RuntimeError("no 'Colour scheme in effect' line for the %s pictures: %s" % (theme, log_path))
    if not lines[-1].startswith("Colour scheme in effect: %s " % theme):
        raise RuntimeError("the %s pictures were taken in another scheme (%s): %s" % (theme, lines[-1], log_path))
    return lines[-1][len("Colour scheme in effect: "):]


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


# The quit runs (mitcad#2394): the layout and whether the Start page is
# shown again over the tutorial before MitCad quits.
QUIT_CASES = {"floating": False, "docked": True}


def quit_workflow(source, manifest, start_page_again):
    """The example's tutorial started from the Start page's card at launch,
    its page loaded with the bridge and its first step started (the page's
    events listener open), the Start page shown again over it when
    `start_page_again`, then MitCad quits with the tutorial pane open."""
    lines = (["expect-ever '^Start page shown$' 30", "wait-idle"] + example_open(source, manifest, via="button")
             + example_first_step() + ["wait-idle"])
    if start_page_again:
        lines += [
            "mark",
            "click-logged '^Ribbon tab START at '",
            "wait-log '^Start page shown$' 10",
            "wait-idle",
        ]
    lines += [
        # The pane still open: the tutorial was not left.
        "expect-no-log '^Tutorial: pane closed$'",
        "quit",
    ]
    return "\n".join(lines) + "\n"


def run_quit(app, cli, directory, qt_bin=None, component=None, example=None):
    """MitCad quits with a tutorial's page shown (mitcad#2394), in the
    Floating layout with the pane in front and in the Docked layout with the
    Start page shown again over it: CEF ends at the shutdown before the
    window and its tutorial pane go, and the run must end with exit code 0
    and no fatal check of the browser component in its log (run_app)."""
    del cli  # the same arguments as the other UI tests; nothing to build here
    directory = prepare_start_page(directory, example)
    manifest = json.loads((directory / "source" / "course" / "tutorials" / EXAMPLE_TUTORIAL / "tutorial.json")
                          .read_text(encoding="utf-8"))
    source = (directory / "source" / "source.git").as_posix()
    for layout, start_page_again in QUIT_CASES.items():
        name = "quit-" + layout
        run_directory = directory / name
        remove(run_directory)
        run_directory.mkdir()
        script = run_directory / (name + ".mitcad-ui")
        script.write_text(quit_workflow(source, manifest, start_page_again), encoding="utf-8")
        text = run_app(app, run_directory, name, qt_bin, component, script=script,
                       arguments=("--chrome=" + layout,), start_page="1")
        # The order the crash needed: CEF ended first, the window (and its
        # tutorial pane) went after it.
        ended = text.find("Component browser: CEF ended")
        if ended < 0 or not re.search(r"^Shutdown: window manager: \S+ destroyed", text[ended:], re.MULTILINE):
            raise RuntimeError("the run did not end CEF before the window went: %s"
                               % (run_directory / (name + ".log")))
    print("PASS: Tutorials at quit: MitCad quits with the example tutorial's page open (Floating) and with the "
          "Start page shown over it (Docked); CEF ends before the window goes, exit code 0, no fatal check")


# The hold run (mitcad#2368): a sketch on the XY plane, the rectangle tool
# picked by a step with hold (the page shows its Continue meanwhile), then a
# rectangle drawn and the sketch finished by another step with hold, during
# which the tutorial is left.
HOLD_TUTORIAL = "hold-step"
HOLD_STEPS = [
    {"id": "sketch", "text": "Click Create Sketch, then the XY plane.", "target": {"command": "sketch.create"},
     "done": {"mode": "sketch"}},
    {"id": "tool", "text": "Pick the 2-Point Rectangle tool.", "target": {"command": "sketch.rectangle"},
     "done": {"command": "sketch.rectangle"}, "hold": True},
    {"id": "draw", "text": "Draw a rectangle, then click Finish Sketch.", "target": {"command": "sketch.finish"},
     "done": {"mode": "idle"}, "hold": True},
]

HOLD_PAGE = """<!doctype html>
<html><head><meta charset="utf-8"><title>Hold step</title></head><body>
<h1>Hold step</h1>
<p>A rectangle, with an explanation after each step.</p>
<script>
function say(text) { console.log('mitcad:tutorial:' + text); }
if (!window.mitcad) {
  say('no bridge');
} else {
  var steps = %(steps)s;
  var done = {};
  function start(index) {
    mitcad.startStep(steps[index]).then(function () { say('started ' + steps[index].id); },
                                       function (error) { say('step refused: ' + error.message); });
  }
  mitcad.on('step_done', function (event) {
    // Each step done is heard once.
    done[event.step] = (done[event.step] || 0) + 1;
    say('done ' + event.step + ' ' + done[event.step]);
    if (event.step === 'sketch') {
      start(1);
    } else if (event.step === 'tool') {
      // The explanation: the input is held until Continue starts the next step.
      mitcad.state().then(function (state) { say('holding ' + state.guide.holding); });
      var go = document.createElement('a');
      go.href = '#continue';
      go.textContent = 'Continue';
      go.onclick = function () { go.remove(); start(2); return false; };
      document.body.appendChild(go);
    }
  });
  mitcad.on('refused', function (event) { say('refused ' + event.command); });
  mitcad.on('step_left', function (event) { say('left ' + event.step + ' ' + event.reason); });
  start(0);
}
</script>
</body></html>
"""


def prepare_hold(directory):
    """The hold run's tutorials repository (a bare clone, the source) and
    the test driver's steps (hold.mitcad-ui), after what an earlier run left
    is removed."""
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    for name in ("course", "source.git", "cache", "installed", "profile", "components", "hold.mitcad-ui",
                 "hold.log"):
        remove(directory / name)
    for left in directory.glob("settings-*"):
        remove(left)
    course = directory / "course"
    tutorial = course / "tutorials" / HOLD_TUTORIAL
    tutorial.mkdir(parents=True)
    (course / "mitcad-tutorials.json").write_text(json.dumps(
        {"format": "mitcad-tutorials", "version": 1, "name": "Hold tutorials"}), encoding="utf-8")
    (tutorial / "tutorial.json").write_text(json.dumps(
        {"id": HOLD_TUTORIAL, "title": "Hold step", "language": "en", "version": "1.0", "min_mitcad": "0.1.0",
         "license": "CC0-1.0", "description": "A rectangle, with an explanation after each step.",
         "start": "index.html"}), encoding="utf-8")
    (tutorial / "index.html").write_text(HOLD_PAGE % {"steps": json.dumps(HOLD_STEPS)}, encoding="utf-8")
    git(course, "init", "-q")
    git(course, "add", "-A")
    git(course, "commit", "-q", "-m", "Hold step")
    source = directory / "source.git"
    git(directory, "clone", "-q", "--bare", str(course), str(source))
    (directory / "hold.mitcad-ui").write_text(hold_workflow(source.as_posix()), encoding="utf-8")
    return directory


def hold_workflow(source):
    """The test driver's steps of the hold run (mitcad#2368)."""
    lines = [
        "wait-idle",
        "set-setting tutorials/sources '%s'" % json.dumps([source]),
        "mark",
        "command help.tutorials",
        "expect-log '^Start page section: tutorials$' 20",
        "expect-log '^Library fetched: tutorials Hold tutorials \\(\\) from ' 60",
        "expect-log '^Tutorials: listed %s \\(Hold step\\) from .*: not started$' 30" % HOLD_TUTORIAL,
        "wait-idle",
        "mark",
        "button '^Start$' tutorialStart_%s" % HOLD_TUTORIAL,
        "expect-log '^Tutorials: chosen %s \\(start\\)$' 20" % HOLD_TUTORIAL,
        "expect-log '^Tutorial: opened Hold step ' 30",
        "focus-main",
        "expect-log '^Tutorial page loaded: file:.*/%s/index\\.html \\(bridge: yes\\)$' 90" % HOLD_TUTORIAL,
        # 1. A sketch on the XY plane: a step without hold, the input free
        # once it is done. The wheel brings the origin's planes clear of the
        # command card (example_steps).
        "expect-log '^Guide: step sketch started: target command sketch\\.create, navigation free$' 30",
        "wheel 50 25 -3",
        "wait-idle",
        "mark",
        "command sketch.create",
        "expect-log '^Create Sketch: select a plane' 20",
        "expect-log '^Datum xy at ' 20",
        "click-logged '^Datum xy at '",
        "expect-log '^Sketch started on xy' 20",
        "expect-log '^Guide: step sketch done: the mode is sketch$' 20",
        "expect-log '^Tutorial page says: done sketch 1$' 20",
        "expect-no-log '^Guide: holding input'",
        # 2. The rectangle tool picked: the step with hold is done, the page
        # hears it once and at once, the callout goes, the tool stays open.
        "expect-log '^Guide: step tool started: target command sketch\\.rectangle, navigation free$' 20",
        "mark",
        "command sketch.rectangle",
        "expect-log '^Sketch tool 2-Point Rectangle' 20",
        "expect-log '^Guide: step tool done: command sketch\\.rectangle finished$' 20",
        "expect-log '^Guide: holding input after step tool$' 20",
        "expect-log '^Tutorial page says: done tool 1$' 20",
        "expect-log '^Tutorial page says: holding true$' 20",
        "expect-widget-property guideCallout visible false",
        "focus-main",
        "wait-idle",
        # Clicks in the view with the tool open: held back, no geometry.
        "mark",
        "sketch-click 0 0",
        "sketch-click -30 -20",
        "expect-log '^Guide: input held back on the view during the hold after step tool$' 20",
        "sleep 1000",
        "wait-idle",
        "expect-no-log '^Added rectangle'",
        # A command: refused during the hold, heard by the page.
        "mark",
        "command sketch.line",
        "expect-log '^Guide: command sketch\\.line refused during the hold after step tool$' 20",
        "expect-log '^Tutorial page says: refused sketch\\.line$' 20",
        "expect-no-log '^Sketch tool Line'",
        "expect-no-log '^Tutorial page says: done tool 2$'",
        "expect-no-log '^Tutorial page says: left '",
        # The page's Continue starts the next step: the hold is released and
        # the step's action (the rectangle's clicks) runs.
        "mark",
        "invoke-widget tutorialPage clickLink 'Continue'",
        "expect-log '^Guide: hold after step tool released \\(next step\\)$' 30",
        "expect-log '^Guide: step draw started: target command sketch\\.finish, navigation free$' 30",
        "expect-log '^Tutorial page says: started draw$' 30",
        "focus-main",
        "wait-idle",
        "mark",
        "sketch-click 0 0",
        "sketch-click -30 -20",
        "expect-log '^Added rectangle (30 x 20|20 x 30) mm ' 20",
        "expect-no-log '^Guide: input held back'",
        "focus-main",
        "key Escape",
        "expect-log '^Sketch tool ended: 2-Point Rectangle' 20",
        "wait-idle",
        # 3. Finish Sketch: done with hold; a command is refused; Leave
        # Tutorial during the hold frees all input.
        "mark",
        "command sketch.finish",
        "expect-log '^Sketch finished' 20",
        "expect-log '^Guide: step draw done: the mode is idle$' 20",
        "expect-log '^Guide: holding input after step draw$' 20",
        "expect-log '^Tutorial page says: done draw 1$' 20",
        "wait-idle",
        "mark",
        "command sketch.create",
        "expect-log '^Guide: command sketch\\.create refused during the hold after step draw$' 20",
        "sleep 500",
        "expect-no-log '^Create Sketch: select a plane'",
        "mark",
        "command help.leave_tutorial",
        "expect-log '^Guide: hold after step draw released \\(leave\\)$' 20",
        "expect-log '^Tutorial: pane closed$' 20",
        "wait-idle",
        # Free again: a command runs and its pick in the view is taken.
        "mark",
        "command sketch.create",
        "expect-log '^Create Sketch: select a plane' 20",
        "expect-log '^Datum xy at ' 20",
        "click-logged '^Datum xy at '",
        "expect-log '^Sketch started on xy' 20",
        "expect-no-log '^Guide: '",
        "mark",
        "command sketch.finish",
        "expect-log '^Sketch finished' 20",
        "wait-idle",
        "quit",
    ]
    return "\n".join(lines) + "\n"


def run_hold(app, cli, directory, qt_bin=None, component=None, example=None):
    """A step with hold (mitcad#2368): done while the rectangle tool is
    open, a click in the view adds no geometry and a command is refused;
    the page's next step allows its action; Leave Tutorial during the hold
    frees all input."""
    del cli, example  # the same arguments as the other UI tests; nothing to build here
    directory = prepare_hold(directory)
    run_app(app, directory, "hold", qt_bin, component)
    print("PASS: Tutorials with hold: the step done once while the rectangle tool is open, clicks in the view "
          "and a command held back, the next step's rectangle drawn, Leave Tutorial during the hold frees the "
          "input")


# The placement runs (mitcad#2367): the run's name, its layout and whether
# the system asks for reduced motion (MITCAD_REDUCED_MOTION).
PLACEMENT_RUNS = {"place-floating": ("floating", False), "place-docked": ("docked", False),
                  "place-floating-reduced": ("floating", True)}

# The page of the placement runs' tutorial: centred for its introduction
# (after a place MitCad does not have is refused), its links move the pane,
# the step at the right asks for the body's Browser row, its end brings the
# review (centred). The count of moves would start again if the page were
# loaded again.
PLACE_PAGE = """<!doctype html>
<html><head><meta charset="utf-8"><title>Places</title></head><body>
<h1>Places</h1>
<p id="moves">No moves yet.</p>
<p><a href="#" id="left">Go to the left</a></p>
<p><a href="#" id="right">Go to the right</a></p>
<p><a href="#" id="next">Continue</a></p>
<script>
function say(text) { console.log('mitcad:tutorial:' + text); }
var moves = 0;
function place(side, then) {
  mitcad.place(side).then(function (answer) {
    moves += 1;
    document.getElementById('moves').textContent = moves + ' moves';
    say('placed ' + answer.place + ', move ' + moves);
    if (then) { then(); }
  }, function (error) { say('place refused: ' + error.message); });
}
say('page started');
mitcad.on('step_done', function (event) {
  say('done ' + event.step);
  // The step's review, centred.
  place('centre');
});
document.getElementById('left').onclick = function () { place('left'); return false; };
document.getElementById('right').onclick = function () {
  place('right', function () {
    mitcad.startStep({id: 'browser', text: 'Select the body in the Browser.', target: {browser: 'Body1'},
                      done: {pick: 'body'}})
      .then(function () { say('started browser'); }, function (error) { say('step refused: ' + error.message); });
  });
  return false;
};
// The next step's side.
document.getElementById('next').onclick = function () { place('right'); return false; };
// A place MitCad does not have, then the introduction, centred.
mitcad.place('middle').then(function () { say('middle placed'); },
                            function (error) { say('place refused: ' + error.message); place('centre'); });
</script>
</body></html>
"""


def prepare_placement(directory):
    """The placement runs' tutorials repository (a bare clone, the source)
    and each run's folder with its driver's steps (<run>/<run>.mitcad-ui),
    after what an earlier run left is removed."""
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    for name in ["course", "source.git"] + list(PLACEMENT_RUNS):
        remove(directory / name)
    course = directory / "course"
    tutorial = course / "tutorials" / "places"
    tutorial.mkdir(parents=True)
    (course / "mitcad-tutorials.json").write_text(json.dumps(
        {"format": "mitcad-tutorials", "version": 1, "name": "Test tutorials"}), encoding="utf-8")
    (tutorial / "tutorial.json").write_text(json.dumps(
        {"id": "places", "title": "Places", "language": "en", "version": "1.0", "min_mitcad": "0.1.0",
         "license": "CC0-1.0", "description": "The pane moved where each step needs it.", "start": "index.html"}),
        encoding="utf-8")
    (tutorial / "index.html").write_text(PLACE_PAGE, encoding="utf-8")
    git(course, "init", "-q")
    git(course, "add", "-A")
    git(course, "commit", "-q", "-m", "Places")
    source = directory / "source.git"
    git(directory, "clone", "-q", "--bare", str(course), str(source))
    for name, (_layout, reduced) in PLACEMENT_RUNS.items():
        (directory / name).mkdir()
        (directory / name / (name + ".mitcad-ui")).write_text(placement_workflow(source.as_posix(), reduced),
                                                               encoding="utf-8")
    return directory


def placed(place, move, how):
    """The driver's steps for the page's move number `move` to `place`: the
    page heard the answer, the pane ended there (`how` the move's words in
    the log) with Leave Tutorial in sight, its middle captured (${middle},
    ${wide}: percent of the window's width), and the page not loaded again."""
    return [
        "expect-log '^Tutorial: the page places the pane %s$' 30" % place,
        "expect-log '^Tutorial page says: placed %s, move %d$' 30" % (place, move),
        "expect-log '^Tutorial: pane placed %s \\(%s\\) at ' 30" % (place, how),
        "capture middle '^Tutorial: pane placed %s .*, its middle at ([0-9]+)%% of the window' 10" % place,
        "capture wide '^Tutorial: pane placed %s .*, ([0-9]+)%% of its width wide$' 10" % place,
        "expect-widget-property tutorialLeave visible true",
        "expect-no-log '^Tutorial page (loaded|says: page started)'",
    ]


def placement_workflow(source, reduced):
    """The driver's steps of a placement run (mitcad#2367): the test's
    tutorial started from its card, its page placing the pane centred, at
    the left, at the right (its step's Browser row clicked and picked
    there), centred for the review and at the right for the next step; then
    Leave Tutorial."""
    how = "at once" if reduced else "animated 250 ms"
    block = {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
                                           "corner": [0, 0], "length": 30, "width": 20, "height": 10,
                                           "operation": "new_body"}}
    lines = [
        "wait-idle",
        "set-setting tutorials/sources '%s'" % json.dumps([source]),
        "mark",
        "command help.tutorials",
        "expect-log '^Start page section: tutorials$' 20",
        "expect-log '^Library fetched: tutorials Test tutorials \\(\\) from ' 60",
        "expect-log '^Tutorials: listed places \\(Places\\) from .*: not started$' 30",
        "wait-idle",
        "mark",
        "button '^Start$' tutorialStart_places",
        "expect-log '^Tutorial: opened Places ' 30",
        "focus-main",
        "expect-log '^Tutorial page loaded: file:.*/places/index\\.html \\(bridge: yes\\)$' 90",
        "expect-log '^Tutorial page says: page started$' 30",
        # A place MitCad does not have is refused with the reason.
        "expect-log '^Tutorial page says: place refused: place takes \"left\", \"right\" or \"centre\"$' 30",
        # 1. The introduction, centred: most of the window, the view held.
        "expect-log '^Tutorial page says: placed centre, move 1$' 30",
        "expect-log '^Tutorial: pane placed centre \\(%s\\) at ' 30" % how,
        "capture middle '^Tutorial: pane placed centre .*, its middle at ([0-9]+)% of the window' 10",
        "capture wide '^Tutorial: pane placed centre .*, ([0-9]+)% of its width wide$' 10",
        "expect-near ${middle} 50 0 3",
        "expect-less 60 ${wide}",
        "expect-widget-property tutorialLeave visible true",
        "expect-log '^Guide: view held \\(the tutorial pane is centred\\)$' 10",
        "wait-idle",
        # A body for the step at the right, in the tutorial's own design.
        "model-command '%s'" % json.dumps(block),
        "wait-idle",
        "expect-model-bodies 1 6000",
        # 2. At the left, the view free again.
        "mark",
        "invoke-widget tutorialPage clickLink 'Go to the left'",
    ] + placed("left", 2, how) + [
        "expect-less ${middle} 50",
        "expect-log '^Guide: view free$' 10",
        "wait-idle",
        # 3. At the right with a step on the body's Browser row: the Browser
        # at the left stays in sight, its row is clicked and picked there,
        # nothing held back.
        "mark",
        "invoke-widget tutorialPage clickLink 'Go to the right'",
    ] + placed("right", 3, how) + [
        "expect-less 50 ${middle}",
        "expect-log '^Guide: step browser started: target Browser row Body1, navigation free$' 30",
        "expect-log '^Tutorial page says: started browser$' 20",
        "wait-idle",
        # The tutorial's new design has its body in its first component.
        "expect-ever '^Browser Root/(Component1:1/)?Bodies/Body1 at ' 20",
        "focus-main",
        "click-logged '^Browser Root/(Component1:1/)?Bodies/Body1 at '",
        "expect-log '^Guide: step browser done: a body picked$' 20",
        "expect-no-log '^Guide: input held back on '",
        # 4. The step done: the review, centred, the page still the same.
        "expect-log '^Tutorial page says: done browser$' 20",
    ] + placed("centre", 4, how) + [
        "expect-near ${middle} 50 0 3",
        "expect-less 60 ${wide}",
        "expect-log '^Guide: view held \\(the tutorial pane is centred\\)$' 10",
        "wait-idle",
        # 5. Continue: the next step's side.
        "mark",
        "invoke-widget tutorialPage clickLink 'Continue'",
    ] + placed("right", 5, how) + [
        "expect-less 50 ${middle}",
        "expect-log '^Guide: view free$' 10",
        "wait-idle",
        # Leave Tutorial: the pane closes, back at the left (hidden), where
        # the next tutorial opens.
        "mark",
        "command help.leave_tutorial",
        "expect-log '^Tutorial: pane placed left \\(hidden\\)$' 20",
        "expect-log '^Tutorial: pane closed$' 20",
        "expect-widget-property tutorialDock visible false",
        "wait-idle",
        "quit",
    ]
    return "\n".join(lines) + "\n"


def run_placement(app, cli, directory, qt_bin=None, component=None, example=None):
    """The tutorial pane's places (mitcad#2367): each placement run in a
    folder of its own, in its layout, with reduced motion or without it."""
    del cli, example  # the same arguments as the other UI tests
    directory = prepare_placement(directory)
    for name, (layout, reduced) in PLACEMENT_RUNS.items():
        run_app(app, directory / name, name, qt_bin, component, arguments=("--chrome=" + layout,),
                extra_environment={"MITCAD_REDUCED_MOTION": "1" if reduced else "0"})
    print("PASS: Tutorial pane places: a page's place refused for a name MitCad does not have; centred, at the "
          "left, at the right with a Browser row picked there, centred for the review and at the right for the "
          "next step, in the Floating and the Docked layout, animated and at once with reduced motion; Leave "
          "Tutorial in sight in every place, the page never loaded again, the pane back at the left when left")


def prepare_hints(directory):
    """The hints' run (mitcad#2366): a tutorials repository of the test's own
    with the hints' tutorial (HINTS_PAGE) as the source, and the driver's
    steps (hints.mitcad-ui), after what an earlier run left is removed."""
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    for name in ("course", "source.git", "cache", "installed", "profile", "components", "hints.mitcad-ui",
                 "hints.log"):
        remove(directory / name)
    for left in directory.glob("settings-*"):
        remove(left)
    course = directory / "course"
    tutorial = course / "tutorials" / "hints"
    tutorial.mkdir(parents=True)
    (course / "mitcad-tutorials.json").write_text(json.dumps(
        {"format": "mitcad-tutorials", "version": 1, "name": "Hint tutorials"}), encoding="utf-8")
    (tutorial / "tutorial.json").write_text(json.dumps(
        {"id": "hints", "title": "Hints", "language": "en", "version": "1.0", "min_mitcad": "0.1.0",
         "license": "CC0-1.0", "description": "Hints over the step's target.", "start": "index.html"}),
        encoding="utf-8")
    (tutorial / "index.html").write_text(HINTS_PAGE, encoding="utf-8")
    git(course, "init", "-q")
    git(course, "add", "-A")
    git(course, "commit", "-q", "-m", "Hints")
    source = directory / "source.git"
    git(directory, "clone", "-q", "--bare", str(course), str(source))
    (directory / "hints.mitcad-ui").write_text(hints_workflow(source.as_posix()), encoding="utf-8")
    return directory


def hints_workflow(source):
    """The driver's steps of the hints' run: the learner's keys and gestures
    read by the page; the Browser-row step's demonstration over the row
    after the page's delay, a click held back on the timeline heard by the
    page and its hint at once, stopped by the page, and the learner's own
    click through the playing hint completing the step; the orbit step's
    demonstration of the configured gesture with its key held, ended by
    Leave Tutorial."""
    block = {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
                                           "corner": [0, 0], "length": 30, "width": 20, "height": 10,
                                           "operation": "new_body"}}
    lines = [
        "wait-idle",
        "set-setting tutorials/sources '%s'" % json.dumps([source]),
        "mark",
        "command help.tutorials",
        "expect-log '^Start page section: tutorials$' 20",
        "expect-log '^Library fetched: tutorials Hint tutorials \\(\\) from ' 60",
        "expect-log '^Tutorials: listed hints \\(Hints\\) from .*: not started$' 30",
        "wait-idle",
        "mark",
        "button '^Start$' tutorialStart_hints",
        "expect-log '^Tutorial: opened Hints ' 30",
        "expect-log '^Start page hidden$' 10",
        "focus-main",
        "expect-log '^Tutorial page loaded: file:.*/hints/index\\.html \\(bridge: yes\\)$' 90",
        # The learner's keys and gestures: View Front's default key, an
        # unknown command refused, Alt + left orbits (the run's settings).
        "expect-log '^Tutorial page says: shortcut view\\.front 1$' 30",
        "expect-log '^Tutorial page says: shortcut refused: MitCad has no command no\\.such$' 30",
        "expect-log '^Tutorial page says: navigation orbit left\\+alt$' 30",
        # No step shown: no demonstration.
        "expect-log '^Tutorial page says: demonstration refused: No guide step is shown' 30",
        "expect-no-log '^Guide: demonstration on step '",
        # A body, whose Browser row the step names.
        "mark",
        "model-command '%s'" % json.dumps(block),
        "wait-idle",
        "expect-model-bodies 1 6000",
        "expect-log '^Browser Root/(?:[^ ]+/)?Bodies/Body1 at ' 20",
        "expect-ever '^Timeline Box1 at ' 20",
        # The Browser-row step; after the page's own delay its hint plays once
        # over the row: the mouse glides to the row's middle and clicks with
        # its left button there.
        "invoke-widget tutorialPage clickLink 'Start the Browser step'",
        "expect-log '^Guide: step row started: target Browser row Body1, navigation free$' 30",
        "expect-log '^Tutorial page says: started row$' 20",
        "expect-log '^Guide: demonstration on step row started: 4 frames, played once, "
        "\"Click Body1 with the left button\"$' 20",
        "expect-widget-property guideDemonstration visible true",
        "expect-log '^Guide: demonstration frame 1 at -?[0-9]+,-?[0-9]+ \\(outside the target\\)$' 20",
        "expect-log '^Guide: demonstration frame 3 at [0-9]+,[0-9]+ \\(in the target\\)$' 20",
        "capture hintY '^Guide: demonstration frame 3 at [0-9]+,([0-9]+) '",
        "capture rowY '^Browser Root/(?:[^ ]+/)?Bodies/Body1 at [0-9]+,([0-9]+)$'",
        "expect-near ${hintY} ${rowY} 0 3",
        "expect-log '^Guide: demonstration on step row ended \\(played\\)$' 20",
        "expect-log '^Tutorial page says: demonstration ended row played$' 20",
        "expect-widget-property guideDemonstration visible false",
        # The hint shows; it does not do the step.
        "expect-no-log '^Guide: step row done'",
        "expect-widget-property guideCallout visible true",
        # A click held back on the timeline: the page hears it and shows the
        # hint at once, until it stops it.
        "mark",
        "click-logged '^Timeline Box1 at '",
        "expect-log '^Guide: input held back on timelineDock during step row$' 20",
        "expect-log '^Tutorial page says: held timeline click left$' 20",
        "expect-log '^Guide: demonstration on step row started: 4 frames, until stopped, ' 20",
        "expect-log '^Guide: demonstration frame 1 at ' 20",
        "expect-no-log '^Guide: step row done'",
        "mark",
        "invoke-widget tutorialPage clickLink 'Stop the hint'",
        "expect-log '^Guide: demonstration on step row ended \\(stopped\\)$' 20",
        "expect-log '^Tutorial page says: stopped true$' 20",
        "expect-log '^Tutorial page says: demonstration ended row stopped$' 20",
        "expect-widget-property guideDemonstration visible false",
        # Shown again; the learner's own click on the row, through the
        # playing hint, does the step and ends the hint.
        "mark",
        "invoke-widget tutorialPage clickLink 'Show the hint'",
        "expect-log '^Guide: demonstration on step row started: 4 frames, until stopped, ' 20",
        "expect-log '^Guide: demonstration frame 1 at ' 20",
        "click-logged '^Browser Root/(?:[^ ]+/)?Bodies/Body1 at '",
        "expect-log '^Guide: demonstration on step row ended \\(step\\)$' 20",
        "expect-log '^Guide: step row done: a body picked$' 20",
        "expect-log '^Tutorial page says: done row \\{\"pick\":\"body\"\\}$' 20",
        # The orbit step: the configured gesture, Alt held with the left
        # button, played over the view until the step ends.
        "expect-log '^Guide: step orbit started: target the view, navigation free$' 20",
        "expect-log '^Tutorial page says: orbit gesture left\\+alt$' 20",
        "expect-log '^Guide: demonstration on step orbit started: 3 frames, until stopped, "
        "\"Hold Alt and drag with the left button\"$' 20",
        "expect-log '^Guide: demonstration frame 2 at [0-9]+,[0-9]+ \\(in the target\\)$' 20",
        "expect-log '^Guide: demonstration frame 3 at [0-9]+,[0-9]+ \\(in the target\\)$' 20",
        "expect-widget-property guideDemonstration visible true",
        "mark",
        "command help.leave_tutorial",
        "expect-log '^Guide: demonstration on step orbit ended \\(step\\)$' 20",
        "expect-log '^Guide: step orbit left \\(leave\\)$' 20",
        "expect-log '^Tutorial: pane closed$' 20",
        "expect-widget-property guideDemonstration visible false",
        "wait-idle",
        "quit",
    ]
    return "\n".join(lines) + "\n"


def run_hints(app, cli, directory, qt_bin=None, component=None):
    """The tutorials' hints (mitcad#2366) in one run, its settings choosing
    Alt + left to orbit."""
    del cli  # the same arguments as the other UI tests; nothing to build here
    directory = prepare_hints(directory)
    run_app(app, directory, "hints", qt_bin, component, settings=HINTS_SETTINGS)
    print("PASS: Tutorial hints: the learner's keys and gestures read; the Browser-row step's demonstration over "
          "the row, a held click heard by the page, the hint stopped, only the learner's click doing the step; "
          "the orbit step's demonstration of Alt + left until Leave Tutorial")


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
    elif sys.argv[1] == "quit":
        run_quit(*sys.argv[2:])
    elif sys.argv[1] == "hold":
        run_hold(*sys.argv[2:])
    elif sys.argv[1] == "placement":
        run_placement(*sys.argv[2:])
    elif sys.argv[1] == "hints":
        run_hints(*sys.argv[2:])
    else:
        run(*sys.argv[1:])
