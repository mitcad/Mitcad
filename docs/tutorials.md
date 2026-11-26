# In-app tutorials

A MitCad tutorial is a set of web pages shown beside the design. Its steps
point at what to use next. The tutorial pane outlines a button, an input,
a Browser row or a place of the toolbar, and a callout says what to do. MitCad holds back other
input until the user does the step. Tutorials are packages in git
repositories, as [component libraries](libraries.md) are. Users choose
them on the Start page: its Tutorials button (under User Guide) and Help ›
Tutorials… show a card of each in the page's center. Nothing ships in the
installers.

This page gives the formats, so that you can write a tutorial and publish
it. The user's side is in the [user guide](user-guide.md#tutorials). The
same guide steps are open to AI assistants through live MCP's `ui_guide`
([mcp.md](mcp.md#guided-tutorials)).

## A tutorials repository

```
my-course/                         a git repository
  mitcad-tutorials.json            the repository's manifest (required)
  LICENSE
  README.md
  tutorials/
    sketch-extrude/                one folder per tutorial, named by its id
      tutorial.json                the tutorial's manifest (required)
      index.html                   its first page
      step2.html, style.css, …     its other pages and their assets
      images/, audio/, video/      pictures, sound and video, as files
      designs/start.mitcad         its starting designs
```

`mitcad-tutorials.json`:

```json
{
  "format": "mitcad-tutorials",
  "version": 1,
  "name": "MitCad tutorials",
  "description": "Short lessons on sketches, features and assemblies"
}
```

`tutorials/<id>/tutorial.json`:

```json
{
  "id": "sketch-extrude",
  "title": "Your first part: a sketch and an extrude",
  "language": "en",
  "version": "1.0",
  "min_mitcad": "0.68.0",
  "license": "CC-BY-4.0",
  "description": "Draw a rectangle and extrude it into a block.",
  "start": "index.html",
  "designs": ["designs/start.mitcad"],
  "duration": 10,
  "level": "beginner",
  "picture": "images/result.png",
  "order": 1
}
```

| Field | Meaning |
|---|---|
| `id` | Lower case letters, digits and hyphens, at most 64 characters. The same as the tutorial's folder name. |
| `title` | Shown on its card on the Start page and above the pane. |
| `language` | The language of its pages (`en`, `fi`, `pt-BR`, …). The Start page shows the tutorials in MitCad's language first, and offers a choice of language when the tutorials are in several. |
| `version` | Its own version, in the card's details. |
| `min_mitcad` | The oldest MitCad it works with. An older MitCad refuses to start it. |
| `license` | An SPDX license id for its text and media, in the card's details. |
| `description` | Optional. What it teaches, in one line on its card (the whole text in the line's tooltip). |
| `start` | The page shown first, a path in its folder. Default `index.html`. |
| `designs` | Its starting designs, paths in its folder. A page opens them with `window.mitcad.openDesign`. |
| `duration` | Optional. How long it takes, in whole minutes; the card shows "10 min". |
| `level` | Optional. `beginner`, `intermediate` or `advanced`; the card shows it. |
| `picture` | Optional. A picture of its result, a PNG or JPEG file in its folder of at most 4 MB; the card shows it at 168 × 112 (a picture of 336 × 224 stays sharp on HiDPI screens). |
| `order` | Optional. Its place in the recommended order of its repository, from 1 (the first part first). The cards are in this order, those without one after them, each by title; the sources in the order they were added. |

The card shows only the optional fields a tutorial gives.

A tutorial with a wrong manifest is left out of the list, and the log
says why. The paths stay inside the tutorial's folder. Symbolic links and
submodules are refused. One tutorial has at most 2000 files and 100 MB.
A tutorials repository is fetched as a library is, at most 200 MB.

## Pages

The pages are plain HTML, CSS and JavaScript, with pictures, audio and
video as files of the folder. They are shown by MitCad's web browser
component (the Chromium Embedded Framework). The pages refer to each
other and to their assets by relative paths. A page may link to the web.
A web page, and any page outside the tutorial's folder, gets no bridge.

MitCad writes what a page logs as `console.log("mitcad:tutorial:<text>")`
into its own log as `Tutorial page says: <text>`. Use it for your own
checks and for the tests.

## The bridge

The tutorial's own pages get `window.mitcad`. Its calls return promises.

| Call | What it does |
|---|---|
| `mitcad.startStep(step)` | Shows a guide step ([Guide steps](#guide-steps)), replacing one shown. Resolves to the guide's state; rejects with the reason when the step is not valid or names a command MitCad does not have. |
| `mitcad.endStep()` | Ends the step shown, or releases the hold after a step done with `hold`. |
| `mitcad.releaseHold()` | Releases the hold after a step done with `hold` ([After a step with hold](#after-a-step-with-hold)); nothing happens when none holds. Resolves to the guide's state. |
| `mitcad.state()` | `{"guide", "mode", "command", "picked", "design", "modified", "version", "progress", "place"}`: the guide's state, the window's mode (`idle`, `sketch`, `command`, `pick_plane`), the open command's id, the kinds picked, the design's name, whether it has unsaved changes, MitCad's version, the tutorial's progress ([Progress](#progress)) and where the pane is (`left`, `right`, `centre`; [Placement](#placement)). |
| `mitcad.query(request)` | Reads the design: `{"query": "sketch"}`, `{"query": "feature", "name": "Extrude1"}` or `{"query": "bodies"}` ([Reading the design](#reading-the-design)). Resolves to the answer; rejects with the reason in words. |
| `mitcad.setProgress(progress)` | Records where the user is: `{"step": 3, "steps": 8}` (`steps` may be left out) or `{"done": true}` ([Progress](#progress)). Resolves to the progress recorded. |
| `mitcad.place(side)` | Moves the pane: `"left"`, `"right"` or `"centre"` (`"center"` too) ([Placement](#placement)). Resolves to `{"place": <side>}` as the move starts; rejects any other side. |
| `mitcad.openDesign(path)` | Opens a starting design of the tutorial (a `.mitcad` path in its folder) as a copy of its own in the local data folder's `tutorial-designs/<id>`. The user's unsaved changes are asked about first, as File › Open asks. |
| `mitcad.leave()` | Leaves the tutorial, as Leave Tutorial does. |
| `mitcad.demonstrate(demonstration)` | Plays the page's input demonstration over the step's target ([Demonstrations](#demonstrations)), replacing one playing. Resolves to the guide's state; rejects with the reason when no step is shown or the demonstration is not valid. |
| `mitcad.stopDemonstration()` | Stops the demonstration playing. Resolves to `{"stopped": true}`, or `false` when none played. |
| `mitcad.shortcut(command)` | A command's keys as the user has them now ([The user's keys and gestures](#the-users-keys-and-gestures)). Rejects when MitCad has no such command. |
| `mitcad.navigation()` | The view's orbit, pan and zoom gestures of the user's navigation scheme, for the mouse and the trackpad each on its own ([The user's keys and gestures](#the-users-keys-and-gestures)). |
| `mitcad.on(event, listener)`, `mitcad.off(event, listener)` | Listens to the guide's events ([Events](#events)); `"*"` listens to all. |
| `mitcad.call(name, args)` | The calls above by name (`start_step`, `end_step`, `release_hold`, `state`, `query`, `set_progress`, `place` with `{"side": <side>}`, `open_design`, `leave`, `demonstrate`, `stop_demonstration`, `shortcut` with `{"command": <id>}`, `navigation`). |

The bridge gives no file access and makes no model edits. The user makes
every change in MitCad. A page cannot run a command; it shows a step, and
the user takes it. It reads only the design of the tutorial's window.

```html
<script>
const steps = [
  {id: "sketch", text: "Start a sketch: click Create Sketch.",
   target: {command: "sketch.create"}, done: {started: "sketch.create"}},
  {id: "extrude", text: "Extrude the profile 20 mm.",
   target: {input: "distance"}, done: {input: "distance", value: "20 mm"}}
];
let next = 0;
mitcad.on("step_done", () => { if (++next < steps.length) mitcad.startStep(steps[next]); });
mitcad.on("step_left", event => console.log("mitcad:tutorial:left " + event.reason));
mitcad.openDesign("designs/start.mitcad").then(() => mitcad.startStep(steps[0]));
</script>
```

## Reading the design

A step's `done` says that the user did something: opened a command,
typed a value, finished a sketch. To check what the user made, a page
reads the design with `mitcad.query`: the sketch before the user
finishes it, the feature after the user commits it, the bodies. It reads
the design of the tutorial's window only, and only these three queries:

| Request | Answer |
|---|---|
| `{"query": "sketch"}`, `{"query": "sketch", "name": "Sketch1"}` | The open sketch, or the one named (its name in the Browser): `uid`, `name`, `open` (whether it is the open one), `plane` (`"xy"`, `"xz"`, `"yz"`, a construction plane's uid, or `{"face", "body"}` for a sketch on a face), `origin` and `normal` (where its plane is in the design, in mm; `normal` `[0, 0, 1]` for a sketch facing up), `solved`, `error`, `dof` (the degrees of freedom left), `fully_constrained` (true when it solved with none left), `dimensions` (each one's `id`, `type`: `length`, `distance`, `diameter`, `angle`, …, `value` in mm or radians, `driven`, `parameter`: its name such as `d1`), `curves` (the curves that make profiles, counted by type: `{"line": 4}`), `construction_curves` (how many), `profiles` (each closed region's `area` in mm², `centroid` and `perimeter` in mm). |
| `{"query": "feature", "name": "Extrude1"}`, `{"query": "feature", "type": "extrude"}`, `{"query": "feature"}` | The feature named (in the timeline), or the last one before the timeline marker, of the `type` when given (`extrude`, `revolve`, `hole`, `fillet`, `sketch`, …): `uid`, `name`, `type`, `suppressed`, `status` (`ok`, `warning`, `error`, `suppressed`, `rolled_back`), `error`, `rolled_back` (after the marker), `operation` (`new_body`, `join`, `cut`, `intersect`, …) and `extent` (an extrude's `{"type": "distance", "distance": 10}`, `{"type": "through_all"}`, …) where the feature has them, and `def`: its whole definition ([commands.md](../core/model-api/src/commands.md#feature-definitions)) with the values in mm and radians. |
| `{"query": "bodies"}` | The bodies at the timeline marker: `count`, `solids` (how many of them are solids), and `bodies`, each one's `uid`, `name`, `kind` (`solid`, `sheet`, `mesh` or `empty`), `solid`, `volume` (mm³), `visible`, and `component` for a component's body. |

A request may say which design it means, `"design": "<name>"`; it must
be the window's (the `design` that `mitcad.state()` gives). A command, a
query other than these, a field a query does not take, another design, a
sketch or feature the design does not have, and `{"query": "sketch"}`
with no sketch open are refused: the promise rejects with the reason in
words. A query while the design computes is refused too; ask again after
a moment or at the next event.

An example: a 30 × 30 mm square that is fully constrained, checked while
the sketch is still open; then a cut through the block, told apart from
a blind pocket and from a new body.

```js
const near = (a, b) => Math.abs(a - b) < 1e-6;

async function checkSquare() {
  const sketch = await mitcad.query({query: "sketch"});
  const lengths = sketch.dimensions.filter(d => !d.driven && near(d.value, 30));
  if (sketch.curves.line !== 4 || lengths.length < 2) {
    return "Draw a square and give two sides 30 mm.";
  }
  if (!sketch.fully_constrained) {
    return "The square can still move (" + sketch.dof + " degrees of freedom): fix a corner at the origin.";
  }
  return null;  // ready: show the step that finishes the sketch
}

async function checkCut() {
  const cut = await mitcad.query({query: "feature", type: "extrude"});
  const bodies = await mitcad.query({query: "bodies"});
  if (cut.operation !== "cut" || bodies.count !== 1) {
    return "That made a new body. Undo, and choose Cut as the operation.";
  }
  if (cut.extent.type !== "through_all") {
    return "That is a blind pocket. Edit the extrude and choose Through All.";
  }
  return null;
}

mitcad.on("step_done", async event => {
  if (event.step === "square") {
    const problem = await checkSquare();
    // … show the problem, or the next step
  }
});
```

`mitcad.query` came after the Start page's tutorials; `mitcad.call("query",
…)` does the same with a web browser component installed before it, and a
MitCad without it rejects the call.

## Progress

A tutorial's card on the Start page says *Not started*, *In progress (step
3 of 8)* or *Done*, as the tutorial's pages report it with
`mitcad.setProgress`. MitCad keeps it per tutorial id until the user
starts the tutorial from the beginning again. The card's action is
*Start*, *Continue* while the tutorial is in progress and *Start Again*
once it is done; Start Again, and *Start Over* in the card's context menu,
forget the progress first. Every start opens the tutorial's first page
(`start`) in an empty design of its own.

The page then finds the progress in `mitcad.state().progress`:
`{"state": "not_started"}`, `{"state": "in_progress", "step": 3,
"steps": 8}` or `{"state": "done"}`. A page that continues uses it: the
example below marks the steps before it as done and offers to go on there
or to start over. A tutorial whose steps need what the user built earlier
can open a starting design of that point with `mitcad.openDesign`.

```js
mitcad.state().then(state => {
  const saved = state.progress;
  if (saved && saved.state === "in_progress") {
    offerToContinue(saved.step);   // the page's own
  } else {
    show(0);
  }
});
// Each step shown, and the end:
mitcad.setProgress({step: index + 1, steps: steps.length});
mitcad.setProgress({done: true});
```

`mitcad.setProgress` came with MitCad's Start page tutorials;
`mitcad.call("set_progress", …)` does the same, and a MitCad without it
rejects the call, so a page that catches the rejection works with both.

## Placement

The pane opens at the left. A page moves it where its step needs it with
`mitcad.place(side)`:

| Side | Floating layout | Docked layout |
|---|---|---|
| `"centre"` | A large card centered over the view, most of the room between the toolbar and the timeline, over the other cards. | A floating pane centered over the window, most of it. |
| `"left"` | The left column, under the Browser. | A dock at the left, beside the Browser. |
| `"right"` | The right column, from the toolbar to the timeline; the Browser at the left stays in sight and takes clicks. | A dock at the right. |

Centre suits an introduction, an illustration or a step's review; left a
sketch or a modeling step worked in the view; right a step in the
Browser. While the pane is centered, clicks in the view are held back as a
step holds them (the view's navigation stays free). A step started there
holds what it holds anyway.

The pane moves to its new place in about 250 ms, or at once when the
system's reduced-motion setting is on (Windows: Animation effects off;
macOS: Reduce motion; Linux: KDE's animation speed at Instant, else
GNOME's animations off). The page is not loaded again: its scripts, its
audio and its video go on across the move. Leave Tutorial is in the
pane's header in every place. Leaving the tutorial puts the pane back at
the left, hidden, and the window as it was; the next tutorial opens there.

```js
mitcad.place("centre");                       // the introduction
start.onclick = () => mitcad.place("left").then(() => mitcad.startStep(sketchStep));
mitcad.on("step_done", () => mitcad.place("centre"));   // the review
```

`mitcad.place` came with the pane's placement; with an older web browser
component `mitcad.call("place", {side: "left"})` does the same, and an
older MitCad rejects the call, so a page that catches the rejection works
with every version. MitCad's UI test `app.ui-*-tutorials-placement`
(`tools/ui-tutorials-test.py placement`) moves a pane through these
places in both layouts, with and without reduced motion.

## Guide steps

A step names its target as the UI names it, never by pixels. MitCad
finds the target and outlines it, and places the callout beside it.

```json
{
  "id": "extrude",
  "text": "Extrude the profile 20 mm.",
  "target": {"command": "solid.extrude"},
  "done": {"command": "solid.extrude"},
  "allow": ["view.fit"],
  "navigation": true
}
```

| Field | Meaning |
|---|---|
| `id` | The step's name, echoed in its events. Default `step`. |
| `text` | The callout's text, at most 400 characters. Required. |
| `target` | One of: `{"command": <id>}` (its button where one is shown; the ids are in [app/COMMANDS.md](../app/COMMANDS.md)), `{"input": <key>}` (an input of the open command's panel, by the key ui_state lists), `{"browser": <name>}` (a Browser row by its name), `{"view": true}` or `{"view": <kind or kinds>}` (the 3D view, picks of those kinds), `{"workspace_selector": true}` (the toolbar's workspace selector), `{"workspace": <name>}` (a workspace by its name: its entry in the selector's list while the list is open, else the selector), `{"tab": <name>}` (a toolbar tab by its name; another workspace's tab: the selector), `{"timeline": true}` (the timeline, a place to point at), `{"plane": <plane>}` (the view, where the plane to sketch on is picked; only that plane takes a sketch during the step, [Sketch input steps](#sketch-input-steps)), `{"field": <key>}` (a drawing tool's value box, such as a rectangle's `width`, while the tool shows it). |
| `done` | What completes the step, one of: `{"command": <id>}` (the command finished with OK, or an action ran), `{"started": <id>}` (its panel opened), `{"input": <key>, "value": <text>}` (the value typed into the open panel's input, and left for a moment; a length's "mm" may be left out), `{"pick": true}` or `{"pick": <kind or kinds>}` (a new pick of those kinds), `{"mode": <mode>}` (the window is in that mode: `idle`, `pick_plane`, `sketch`, `command`; a sketch opened is `sketch`, a sketch finished `idle`), `{"navigate": <action>}` (the user orbited, panned or zoomed the view: `orbit`, `pan`, `zoom`; see [Navigation steps](#navigation-steps)), `{"opened": "workspace_selector"}` (the selector's list opened), `{"chosen": <workspace>}` (the workspace shown, with the selector's list closed: at once when it is shown already as the step starts, else once the user chose it), `{"activated": <tab>}` (the tab shown: at once when it is shown already), `{"plane": <plane>}` (a sketch made on that plane), `{"point": <place or places, or [x, y]>}` (the next point of a sketch's drawing tool placed there), `{"field": <key>, "value": <text>}` (the value typed into the drawing tool's value box); the last three are in [Sketch input steps](#sketch-input-steps). Optional: a step without `done` stays until the page ends it (`mitcad.endStep()`, a page's Continue), Escape or Leave Tutorial. |
| `allow` | More command ids the step allows. |
| `navigation` | View navigation: the wheel, the mouse buttons and keys the navigation scheme binds, the orientation cube, and the camera commands (`view.home`, `view.fit`, the standard views, the projection, the arrow keys' turns `view.turn_left`, `view.turn_right`, `view.turn_up`, `view.turn_down`). Default `true`; a `navigate` step needs it. |
| `hold` | `true`: once the step is done, the input stays held while the page shows what comes next ([After a step with hold](#after-a-step-with-hold)). Default `false`. |

The kinds of picks are `face`, `edge`, `vertex`, `body`, `profile`,
`sketch_curve`, `sketch_point`, `plane`, `axis`, `point`, `component`,
`sketch`, `feature`.

### The toolbar's places

A step can introduce the toolbar to a beginner (mitcad#2370): the
workspace selector, a workspace of its list, a tab and the timeline.
MitCad finds each in the layout shown (Floating, Docked, and the Docked
toolbar of macOS) and outlines it; a workspace's entry is outlined in the
selector's list itself, which lies over the window. The workspaces are
`Design`, `Make`, `Render` and `Drawing`; the tabs `Start`, `Solid`,
`Surface`, `Sheet Metal`, `Build`, `3D Print`, `Laser`, `CNC`, `Manage`
and the modes' `Sketch`, `Drawing`, `Cut Sheet` and `Render`. A name is
written as the toolbar shows it in English or in MitCad's language, or as
the toolbar's key (`DESIGN`, `SHEET METAL`, `MAKE_PRINT`); case does not
matter. A name MitCad does not have is refused with the list. The step's
state and its events carry the toolbar's key (`{"chosen": "DESIGN"}`). A
layout without a workspace selector (the title bar's tabs of the macOS
Floating layout) refuses a selector or workspace step with the reason.

```js
const steps = [
  {id: "selector", text: "This is the workspace selector. Click it.",
   target: {workspace_selector: true}, done: {opened: "workspace_selector"}},
  {id: "design", text: "Choose Design, the workspace for modeling.",
   target: {workspace: "Design"}, done: {chosen: "Design"}},
  {id: "surface", text: "Open the Surface tab.",
   target: {tab: "Surface"}, done: {activated: "Surface"}},
  {id: "timeline", text: "The timeline lists your features in order. Click Continue.",
   target: {timeline: true}}   // no done: the page's Continue calls mitcad.endStep()
];
```

The selector and the tab row are free during every step: opening the list,
choosing a workspace and clicking a tab are no commands, so a step never
refuses them. Escape in the selector's list (or in a menu) closes the list
and leaves the step shown. A timeline step only points at the timeline:
its clicks stay held back, as in every step.

While a step is shown:

- The step's commands (its target's, its done command's, `allow`) run.
  Other commands are refused, from a key, a button, a menu or the
  command search alike; a sketch's drawing tools too, unless the step
  allows them. The callout says so, and the log has
  `Guide: command <id> refused during step <id>`.
- Clicks in the Browser, the timeline, the Projects and Configuration
  panels are held back, and so are picks in the view, unless the step
  takes picks, a command's panel is open, a plane is being picked or a
  sketch is open. A left press that navigates (Alt + left where the
  navigation scheme orbits so, the orientation cube's drag) is navigation,
  free while the step allows it. The callout says so briefly, and the page
  hears it (`held`, [Events](#events)).
  The toolbars, the menus, the open panel, dialogs and the tutorial pane
  stay free, in each of the pane's places.
- Escape with no command open ends the step (`step_left`, reason
  `escape`); Escape in a popup (a menu, the workspace selector's list)
  closes the popup. The callout's Leave Tutorial, the pane's and Help › Leave
  Tutorial end it and close the tutorial (reason `leave`).

### After a step with hold

A step is done and the input is free again at once: a click in the view
with a drawing tool still open starts another shape while the page plays
its explanation. A step with `"hold": true` keeps the input held after it
is done:

- `step_done` comes at once, once, as for any step.
- The callout and the outline go. No command runs but Leave Tutorial and,
  while the step allows navigation, the camera's: a key, a button, a menu
  or the command search alike (the log has `Guide: command <id> refused
  during the hold after step <id>`, the page a `refused` event). Picks in
  the view are held back, a drawing tool's points too, and so are clicks
  in the Browser, the timeline, the Projects and Configuration panels.
- The tutorial pane, Leave Tutorial, view navigation, the toolbars, the
  menus, an open command's panel and dialogs stay free; Escape is the
  window's own (it ends a drawing tool, it does not release the hold).
- The hold ends when the page starts the next step (`mitcad.startStep`,
  whose own commands and picks then run), calls `mitcad.releaseHold()` or
  `mitcad.endStep()`, or when the tutorial is left or closed. Always:
  Leave Tutorial frees all input.

`mitcad.state().guide.holding` says whether a hold is on.

```js
const steps = [
  {id: "tool", text: "Pick the 2-Point Rectangle tool.", target: {command: "sketch.rectangle"},
   done: {command: "sketch.rectangle"}, hold: true},
  {id: "draw", text: "Draw a rectangle, then click Finish Sketch.", target: {command: "sketch.finish"},
   done: {mode: "idle"}}
];
mitcad.on("step_done", event => {
  if (event.step === "tool") {
    explain().then(() => mitcad.startStep(steps[1]));   // the page's own
  }
});
```

A MitCad without `hold` refuses a step with it ("A step has no field
\"hold\""), and one without `releaseHold` has no such function on
`window.mitcad`: a page that catches the refusal shows the step again
without it and works with both.

### Navigation steps

A step with `"done": {"navigate": "orbit"}` (or `"pan"`, `"zoom"`) is done
when the user moves the view with that action, as the navigation scheme
chosen in Settings binds it, so the same tutorial works whichever scheme
the learner uses:

```json
{"id": "orbit", "text": "Orbit the view: drag with the orbit gesture of your navigation scheme.",
 "target": {"view": true}, "done": {"navigate": "orbit"}}
```

- What counts: a mouse drag bound to the action, the wheel and a pinch
  (zoom), a trackpad scroll bound to it (pan, orbit or zoom), the
  orientation cube's drag (orbit), and the arrow keys, whose turns are
  orbits.
- It is done once, when the camera has moved: a press of the gesture
  without a move leaves the step open, and so does another action (a
  zoom during an orbit step; the log has
  `Guide: step <id> view zoomed, not its orbit` once a step). The camera
  commands (Home, Fit, the standard views, a click on the orientation
  cube's face) and the 3D mouse are none of the three.
- The event is `step_done` with `done` `{"navigate": "orbit"}`; the log has
  `Guide: step <id> done: the view orbited` (`panned`, `zoomed`).

### Sketch input steps

A step can check the learner's sketch input one interaction at a time
(mitcad#2440): the plane a sketch goes on, where a drawing tool's next
point lands, and a value typed into a drawing tool's value box. Input that
misses is held back: nothing is made, the callout says *Not this plane.*,
*Not this place.* or *Not this value.*, the page hears a `mismatch` event
([Events](#events)) and the learner can try again.

```js
const steps = [
  {id: "plane", text: "Click Create Sketch, then the XY plane.",
   target: {plane: "XY"}, done: {plane: "XY"}, allow: ["sketch.create"]},
  {id: "corner", text: "Pick 2-Point Rectangle, then click the origin for the first corner.",
   target: {view: true}, done: {point: "origin"}, allow: ["sketch.rectangle"]},
  {id: "width", text: "Type 40 for the width.",
   target: {field: "width"}, done: {field: "width", value: "40 mm"}, allow: ["sketch.rectangle"]}
];
mitcad.on("mismatch", event => showHint(event.got));   // the page's own
```

- **A plane**: `{"plane": <plane>}` as the target, the done or both (the
  same plane). The plane is `XY`, `XZ` or `YZ` (the origin's planes),
  `face` (any face), a construction plane's name as the Browser shows it
  (`Plane1`), or a face by its reference, as `mitcad.query({query:
  "sketch"})` gives it in `plane.face` (`F2:top`); case does not matter.
  A step names a plane the design has; another one is refused with the
  reason. During the step a sketch goes only on that plane, however it is
  started (a pick in the view or the Browser while Create Sketch waits for
  a plane, Create Sketch with a plane selected, a Browser row's Create
  Sketch); the done is met when the sketch is made on it, 2D or 3D. A
  plane target outlines the view and takes its picks. The log has `Guide:
  step <id> plane xz, not its XY` for a plane held back and `Guide: step
  <id> done: a sketch on XY`.
- **A point**: `{"point": <place>}`, a list of places (any of them) or the
  coordinates `[x, y]` in the sketch, in mm (within 0.001 mm). It is the
  next point of a 2D sketch's drawing tool (Line, 2-Point Rectangle,
  Circle, …) placed with a click: a click where the point does not snap to
  the place is held back and the tool keeps what it has; the click at the
  place completes the step and goes on to the tool. Enter, which places
  no point where the pointer snapped, is left to the tool. The places:
  `origin`, `x_axis`, `y_axis` (on the sketch's axis; the origin is on
  both), `point` (a point of the sketch, the origin's too), `center` (a
  circle's or an arc's), `midpoint`, `quadrant`, `intersection`, `curve`
  (along a curve of the sketch), `extension` (on a line's extension),
  `body_vertex`, `body_midpoint`, `body_edge`, `body_center`,
  `body_section` (where the sketch plane cuts a body), `grid` and `free`
  (no snap). The log has `Guide: step <id> point at grid (10, 5), not its
  origin` and `Guide: step <id> done: a point at origin (0, 0)`.
- **A value**: `{"field": <key>, "value": <text>}`, the value typed into a
  drawing tool's value box, by its key: `width` and `height` (the
  rectangles), `radius` (a rounded rectangle's corners, an arc's),
  `diameter` (a circle's), `length` and `angle` (a line's), `sides` (a
  polygon's). It is matched as an input's value is (`"40"` is `"40 mm"`).
  The step is done once the value typed stays a moment, or at the click or
  Enter that uses the box while it holds that value. While the tool shows
  the box, a click or Enter that would use another value (another typed
  one, or the value under the pointer when nothing is typed) is held back.
  The log has `Guide: step <id> value 25 in width, not its 40` (`not
  typed`) and `Guide: step <id> done: 40 typed into width`.

The events: `step_done` with `done` `{"plane": "XY"}`, `{"point":
"origin", "at": [0, 0]}` (the place the step waited for, or for
coordinates the one it snapped to) or `{"field": "width", "value": "40"}`;
`mismatch` with `got` the same shapes for the input held back (`"value":
""` when nothing was typed). MitCad's UI test `app.ui-*-guide-sketch-input`
(`tools/ui-scripts/guide-sketch-input.mitcad-ui`) takes each of them.

A MitCad without these refuses such a step ("A step's target takes exactly
one of …"): a page that catches the refusal can show the step with a
`mode` done instead.

## Events

The events come to `mitcad.on` listeners and, for MCP, in the resource
`mitcad://ui-guide`'s `last_event`. Each has `event`, `step` (the step's
id) and `sequence` (one per step started).

| Event | Fields |
|---|---|
| `step_done` | `done`: `{"command": <id>, "committed": true}`, `{"started": <id>}`, `{"input": <key>, "value": <text typed>}`, `{"pick": <kind>}`, `{"mode": <mode>}`, `{"navigate": <action>}` (`orbit`, `pan`, `zoom`), `{"opened": "workspace_selector"}`, `{"chosen": <workspace>}`, `{"activated": <tab>}`, `{"plane": <plane>}`, `{"point": <place>, "at": [x, y]}` or `{"field": <key>, "value": <text typed>}`. |
| `mismatch` | `got`: what the user did that a sketch input step held back ([Sketch input steps](#sketch-input-steps)): `{"plane": <its name>}`, `{"point": <where it snapped>, "at": [x, y]}`, `{"field": <key>, "value": <text typed, or "">}`. |
| `step_left` | `reason`: `escape`, `leave`, `ended` (by the page or MCP), `replaced` (by another step), `closed` (the tutorial closed). Not sent when a hold after a step done ends. |
| `refused` | `command`: the id of a command the step, or the hold after it, held back. |
| `held` | A click or the wheel the step held back: `place` (`view`, `browser`, `timeline`, `projects`, `configuration`), `input` (`click`, `double_click`, `wheel`), `button` (`left`, `middle`, `right`, `other`; not for the wheel) and `modifiers` (the keys held: `shift`, `ctrl`, `alt`, `meta`). A held drag is one `held`, at its press; a held wheel at most one a second. |
| `demonstration_ended` | `reason`: `played` (its plays ended), `stopped` (by the page), `replaced` (by another demonstration), `step` (the step was done or left; before its `step_done` or `step_left`). |

`refused`, `held` and `mismatch` are when a user tried something else: a
page may show its help at once then, and after a delay of its own while
the step is not done ([Demonstrations](#demonstrations)).

## Demonstrations

A page can show the user how to do a step over MitCad's own control: a
mouse that moves to the Browser row and lights its left button, a keycap
held while the mouse drags. The page makes the pictures and their timing;
MitCad shows them over the step's target, on a layer that takes no input,
and follows the target when the window is resized or its panes move.
MitCad never moves the real pointer, and the user's own input goes
through the pictures to the control under them: only the user does the
step. The demonstration ends when its plays end, when the page stops it
or starts another, and when the step is done or left.

```js
mitcad.demonstrate({
  label: "Click Body1 with the left button",
  repeat: 1,
  frames: [
    {image: "images/mouse.svg", offset: [70, 50], ms: 500},
    {image: "images/mouse.svg", move: true, ms: 700},
    {image: "images/mouse-left.svg", ms: 500},
    {image: "images/mouse.svg", ms: 400}
  ]
});
```

| Field | Meaning |
|---|---|
| `frames` | Its frames, 1 to 64, shown one after the other. |
| `repeat` | How many times it plays: 1 by default, `0` until it is stopped or the step ends; at most 100. |
| `pause` | Milliseconds with nothing shown between two plays; default 0. |
| `label` | What it shows, in words, at most 200 characters: the layer's accessible name, and MitCad's log. |

A frame:

| Field | Meaning |
|---|---|
| `image` | Its picture: an SVG, PNG or JPEG file of the tutorial's folder by its relative path, or a `data:` address (`data:image/svg+xml,…` or base64). At most 2 MB. A page can draw an SVG of its own, such as the user's keys ([The user's keys and gestures](#the-users-keys-and-gestures)). |
| `size` | `[width, height]` in pixels (MitCad's logical pixels, drawn sharp on HiDPI screens); default the picture's own size (an SVG's `width` and `height`). At most 1024 a side. |
| `at` | `[x, y]`: where it is in the target's rectangle, as parts of its width and height; default `[0.5, 0.5]`, the middle. |
| `offset` | `[x, y]` pixels added to that place; default `[0, 0]`. |
| `anchor` | `[x, y]`: the picture's point put at that place, as parts of its size; default `[0, 0]`, its top left (a pointer's tip). `[0.5, 0.5]` centres it. |
| `ms` | How long it is shown, 1 to 20000 milliseconds; default 500. |
| `move` | `true`: it glides from the previous frame's place to its own meanwhile; default `false`. |

A target that is not shown (a Browser row scrolled away, a panel closed)
hides the demonstration until it is shown again. The guide's state
(`mitcad.state().guide`) has the target's rectangle, `target` (`x`, `y`,
`width`, `height` in pixels of the design's window), for a page that
sizes its pictures to it, and `demonstrating`.

When to show it is the page's choice: after a delay of its own while the
step is not done, or at once on `held` or `refused`. So is a still picture
for users who prefer less motion (one frame, or CSS's
`prefers-reduced-motion` in the page):

```js
let timer;
function hint() { mitcad.demonstrate(clickRow); }
mitcad.on("held", hint);
mitcad.on("refused", hint);
mitcad.on("step_done", () => clearTimeout(timer));
mitcad.startStep(step).then(() => { timer = setTimeout(hint, 8000); });
```

## The user's keys and gestures

A user may have changed a command's key or chosen another way to turn the
view. `mitcad.shortcut(command)` and `mitcad.navigation()` give the ones
that work on this user's MitCad now, so that a page says and draws those.
Both are read at each call: a key changed in Keyboard Shortcuts or a
navigation scheme or gesture changed in Preferences is in the next answer,
with no restart. Words in the page's language are the page's own: the
answers give the structure (device, button or gesture, keys held) for a
sentence and for a picture or an animation alike.

`mitcad.shortcut("view.front")`:

```json
{"command": "view.front", "name": "Front View",
 "shortcuts": [{"keys": [{"key": "1", "modifiers": []}], "text": "1"}],
 "platform": "linux"}
```

Each of `shortcuts` is one key sequence: its `keys` (a key and the
modifiers held with it: `shift`, `ctrl`, `alt`, `meta`; on macOS `ctrl` is
the Command key, `alt` the Option key and `meta` the Control key) and
`text`, as the platform writes it. `platform` is `macos`, `windows` or
`linux`, so that a page names the keys as that keyboard has them. A
command with no key has `"shortcuts": []`: name its menu command or point
at its button instead.

`mitcad.navigation()` with Alt + left to orbit (in Preferences: Alt + left
orbits, Alt + middle pans, Alt + right zooms):

```json
{"mouse": {
   "orbit": [{"device": "mouse", "input": "left", "modifiers": ["alt"]}],
   "pan": [{"device": "mouse", "input": "middle", "modifiers": []}],
   "zoom": [{"device": "mouse", "input": "right", "modifiers": ["alt"]},
            {"device": "mouse", "input": "wheel", "modifiers": []}]},
 "trackpad": {
   "orbit": [{"device": "trackpad", "input": "drag", "fingers": 1, "modifiers": ["alt"]}],
   "pan": [],
   "zoom": [{"device": "trackpad", "input": "slide", "fingers": 2, "modifiers": []},
            {"device": "trackpad", "input": "pinch", "fingers": 2, "modifiers": []}]},
 "orbit": [{"input": "left", "modifiers": ["alt"]}],
 "pan": [{"input": "middle", "modifiers": []}],
 "zoom": [{"input": "right", "modifiers": ["alt"]}, {"input": "scroll", "modifiers": []}],
 "reverse_zoom": false, "custom": false, "platform": "linux"}
```

`mouse` and `trackpad` each list what that device does for `orbit`, `pan`
and `zoom`; an empty list says the device has no gesture for it (with the
scheme above a trackpad does not pan: the page can point at the view's
other controls instead). Each gesture names its `device`, its `input` and
the keys held (`modifiers`, as above), and `"key": "F4"` for the one with
F4 held:

- The mouse: a drag with the `left`, `middle` or `right` button, then the
  `wheel`, which zooms in every scheme whatever keys are held.
- The trackpad (a touchpad too, and a mouse's touch surface): `slide`,
  two fingers moved on the pad (`"fingers": 2`), as the scheme has it: in
  the Trackpad schemes it pans, orbits with one key and zooms with
  another (with the macOS default, Option and a two-finger slide orbits:
  `{"input": "slide", "modifiers": ["alt"]}`), with Custom as the user set
  it, in the other schemes every slide zooms; `pinch`, two fingers moved apart or together, which zooms
  in every scheme whatever keys are held; `drag`, the pad (or its button)
  pressed and moved (`"fingers": 1`), which is the mouse's left button.
  They are in that order.

A gesture that needs more keys than another one of the same input for the
same action is left out (when the plain slide zooms, so does Shift +
slide). `reverse_zoom` says the wheel and the slide zoom the other way
(the pinch follows the fingers); `custom` that the user set the gestures
one by one.

`orbit`, `pan` and `zoom` at the top are the answer's first form, kept for
the pages that read it: the same gestures as the scheme's table has them,
`input` `left`, `middle`, `right` or `scroll` (the wheel and the
two-finger slide alike), in the order left, middle, right, scroll, then
F4. A new page reads `mouse` and `trackpad`: with a trackpad scheme the
first form's `scroll` is the slide, while the wheel still zooms.

`mitcad.demonstrate`, `mitcad.stopDemonstration`, `mitcad.shortcut` and
`mitcad.navigation` came with MitCad's tutorial hints; `mitcad.call` with
their names does the same, and a MitCad without them rejects the call.
`mouse`, `trackpad` and `platform` came with a later MitCad; a page that
finds them missing reads the first form.

## Sources and download

The Start page's Tutorials (and Help › Tutorials…) shows the tutorials of
the configured sources as cards in the page's center; the breadcrumb's
Recent designs leads back to the designs. The public repository
`https://github.com/mitcad/tutorials.git` is the default source; Sources ›
Add Source… adds a repository of one's own by its address or folder (a
private course). The sources are kept in the setting `tutorials/sources`.

A source is fetched as a library is: a bare clone in the libraries'
folder, made the first time the tutorials show a source never fetched,
and updated with Sources › Update
([Where libraries are kept](libraries.md#where-libraries-are-kept),
[Fetching without git](libraries.md#fetching-without-git)). A card's
picture is read from that clone. A card's Start (Continue, Start Again)
writes the tutorial's folder, at the source's version fetched last, into
the user's data folder (`tutorials/<id>` in the local data folder,
`MITCAD_TUTORIALS_DIR` for tests), with `.mitcad-tutorial-source.json`
naming where it came from, and opens its pane in an empty design of its
own. There is no step of installing apart from starting. The pane needs
the web browser component; MitCad offers its download when it is missing
([Component from Web](architecture.md)).

The model's commands `tutorial_list`, `tutorial_install` and
`tutorial_picture` do the listing, the installing and the cards' pictures
([commands.md](../core/model-api/src/commands.md#in-app-tutorials)).

An example, a sketch and an extrude, is in
[docs/examples/tutorials](examples/tutorials/): a tutorials repository's
layout to start from, and the seed of the public repository, with its
`README.md`, `LICENSE` and a CI check (`tools/check-tutorials.py`, run by
`.github/workflows/check.yml`) that checks every `tutorial.json` against
the format above. MitCad reads only the manifests and `tutorials/`, so
these files are left alone. MitCad's UI tests `app.ui-*-tutorials` and
`app.ui-*-tutorials-start-page` (`tools/ui-tutorials-test.py`) add the
example as a source, start its tutorial from its card on the Start page
and take each step of it to its end, so a UI change that breaks the
example fails a MitCad test.

## Making and publishing a tutorial

1. Make a repository with `mitcad-tutorials.json` and a folder in
   `tutorials/` per tutorial.
2. Write its pages and its steps. Name targets by command ids and input
   keys: `ui_state` over live MCP lists an open panel's input keys, and
   [app/COMMANDS.md](../app/COMMANDS.md) the command ids.
3. Add your repository's folder with the Start page's Tutorials › Sources
   › Add Source…, and start the tutorial from its card to try it.
4. Push it to a git host. For the public repository, open a pull request;
   its CI runs `tools/check-tutorials.py`, which you can run first.
