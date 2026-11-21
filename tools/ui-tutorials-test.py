# SPDX-License-Identifier: MIT
# check-all sources: app/tutorials app/webbrowser core/vcs/src/library tools/cli
"""In-app tutorials (mitcad#2012, docs/tutorials.md): a tutorials repository
of the test's own (a bare clone in a folder) as the only source; Help >
Tutorials... fetches and lists its two-step tutorial and opens it in the
tutorial pane, through the real CEF view of the web browser component the
build made. The page's bridge (window.mitcad) starts the guide step; a step
naming a command MitCad does not have is refused; Escape with no command open
ends the step and the input is free, and the page shows it again; another
command is refused and logged, and the page hears of it; opening the intended command and
typing the intended value complete the steps and the page gets the events;
a page outside the tutorial's folder gets no bridge; Leave Tutorial ends the
step and closes the pane, and the input is free again.

Then the example tutorials repository (docs/examples/tutorials, the seed of
the public tutorials repository, mitcad#2078) as the only source, in a run of
its own: its sketch-extrude tutorial is listed, installed and opened, and each
of its steps is done as a user does it (Create Sketch on the XY plane, a
rectangle, Finish Sketch, Extrude, 20 mm, OK) until the page hears that the
last one is done and the design has the block. A UI change that breaks the
example fails here.

Usage: ui-tutorials-test.py <app> <cli> <output folder> [<Qt's bin> [<the built web browser component> [<example>]]]
       ui-tutorials-test.py prepare <output folder> [<example>]   (the sources and the steps, for the macOS wrapper)
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
        # A block: Extrude is offered only in a design with a body, a
        # sketch or a profile.
        "model-command '%s'" % json.dumps(block),
        "wait-idle",
        "expect-model-bodies 1 6000",
        # The test's repository the only source.
        "set-setting tutorials/sources '%s'" % json.dumps([source]),
        "mark",
        "command help.tutorials",
        "focus-dialog '^Tutorials$' 30",
        "expect-log '^Library fetched: tutorials Test tutorials \\(\\) from ' 60",
        "expect-log '^Tutorials: listed two-steps \\(Two steps\\) from .*, installed no$' 30",
        "invoke-widget tutorialsDialog selectTutorial two-steps",
        "button '^Open$'",
        "expect-log '^Tutorial: installed two-steps 1\\.0 \\(2 files\\) into ' 30",
        "expect-log '^Tutorial: opened Two steps ' 30",
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
    title = manifest["title"]
    lines = [
        "wait-idle",
        # The example's repository the only source.
        "set-setting tutorials/sources '%s'" % json.dumps([source]),
        "mark",
        "command help.tutorials",
        "focus-dialog '^Tutorials$' 30",
        "expect-log '^Library fetched: tutorials Example tutorials \\(\\) from ' 60",
        "expect-log '^Tutorials: listed %s \\(%s\\) from .*, installed no$' 30" % (EXAMPLE_TUTORIAL, re_text(title)),
        "invoke-widget tutorialsDialog selectTutorial %s" % EXAMPLE_TUTORIAL,
        "button '^Open$'",
        "expect-log '^Tutorial: installed %s %s \\(2 files\\) into ' 30"
        % (EXAMPLE_TUTORIAL, re_text(manifest["version"])),
        "expect-log '^Tutorial: opened %s ' 30" % re_text(title),
        "focus-main",
        "expect-log '^Tutorial page loaded: file:.*/%s/index\\.html \\(bridge: yes\\)$' 90" % EXAMPLE_TUTORIAL,
        # 1. Create Sketch, then the XY plane.
        "expect-log '^Guide: step sketch started: target command sketch\\.create, navigation free$' 30",
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
        "expect-no-log '^Guide: command'",
        "expect-model-bodies 1 12000",
        # Leave Tutorial closes the pane.
        "mark",
        "command help.leave_tutorial",
        "expect-log '^Tutorial: pane closed$' 20",
        "wait-idle",
        "quit",
    ]
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


def run_app(app, directory, name, qt_bin=None, component=None):
    """The application with the driver's steps <name>.mitcad-ui, its
    sources and folders those of `directory`; an error when it fails."""
    script = directory / (name + ".mitcad-ui")
    with tempfile.TemporaryDirectory(prefix="settings-", dir=directory) as temporary:
        environment = environment_for(directory, temporary, qt_bin, component)
        environment["MITCAD_TEST_INPUT"] = str(script)
        environment["MITCAD_TEST_STEP_MS"] = "100"
        log_path = directory / (name + ".log")
        with log_path.open("wb") as log:
            result = subprocess.run([app, "--ui=qt", "--no-native-dialogs", "--no-recovery", "--no-mcp"],
                                    env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=300)
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


if __name__ == "__main__":
    if sys.argv[1] == "prepare":
        prepare(*sys.argv[2:4])
    elif sys.argv[1] == "verify":
        verify(sys.argv[2])
    else:
        run(*sys.argv[1:])
