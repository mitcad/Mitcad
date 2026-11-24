# In-app tutorials

A MitCad tutorial is a set of web pages shown beside the design. Its steps
point at what to use next. The tutorial pane outlines a button, an input
or a Browser row, and a callout says what to do. MitCad holds back other
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
| `mitcad.endStep()` | Ends the step shown. |
| `mitcad.state()` | `{"guide", "mode", "command", "picked", "design", "modified", "version", "progress"}`: the guide's state, the window's mode (`idle`, `sketch`, `command`, `pick_plane`), the open command's id, the kinds picked, the design's name, whether it has unsaved changes, MitCad's version, and the tutorial's progress ([Progress](#progress)). |
| `mitcad.setProgress(progress)` | Records where the user is: `{"step": 3, "steps": 8}` (`steps` may be left out) or `{"done": true}` ([Progress](#progress)). Resolves to the progress recorded. |
| `mitcad.openDesign(path)` | Opens a starting design of the tutorial (a `.mitcad` path in its folder) as a copy of its own in the local data folder's `tutorial-designs/<id>`. The user's unsaved changes are asked about first, as File › Open asks. |
| `mitcad.leave()` | Leaves the tutorial, as Leave Tutorial does. |
| `mitcad.on(event, listener)`, `mitcad.off(event, listener)` | Listens to the guide's events ([Events](#events)); `"*"` listens to all. |
| `mitcad.call(name, args)` | The calls above by name (`start_step`, `end_step`, `state`, `set_progress`, `open_design`, `leave`). |

The bridge gives no file access and makes no model edits. The user makes
every change in MitCad. A page cannot run a command; it shows a step, and
the user takes it.

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
| `target` | One of: `{"command": <id>}` (its button where one is shown; the ids are in [app/COMMANDS.md](../app/COMMANDS.md)), `{"input": <key>}` (an input of the open command's panel, by the key ui_state lists), `{"browser": <name>}` (a Browser row by its name), `{"view": true}` or `{"view": <kind or kinds>}` (the 3D view, picks of those kinds). |
| `done` | What completes the step, one of: `{"command": <id>}` (the command finished with OK, or an action ran), `{"started": <id>}` (its panel opened), `{"input": <key>, "value": <text>}` (the value typed into the open panel's input, and left for a moment; a length's "mm" may be left out), `{"pick": true}` or `{"pick": <kind or kinds>}` (a new pick of those kinds), `{"mode": <mode>}` (the window is in that mode: `idle`, `pick_plane`, `sketch`, `command`; a sketch opened is `sketch`, a sketch finished `idle`). |
| `allow` | More command ids the step allows. |
| `navigation` | View navigation: the wheel, the middle and right buttons and the camera commands (`view.home`, `view.fit`, the standard views, the projection). Default `true`. |

The kinds of picks are `face`, `edge`, `vertex`, `body`, `profile`,
`sketch_curve`, `sketch_point`, `plane`, `axis`, `point`, `component`,
`sketch`, `feature`.

While a step is shown:

- The step's commands (its target's, its done command's, `allow`) run.
  Other commands are refused, from a key, a button, a menu or the
  command search alike; a sketch's drawing tools too, unless the step
  allows them. The callout says so, and the log has
  `Guide: command <id> refused during step <id>`.
- Clicks in the Browser, the timeline, the Projects and Configuration
  panels are held back, and so are picks in the view, unless the step
  takes picks, a command's panel is open, a plane is being picked or a
  sketch is open. The callout says so briefly.
  The toolbars, the menus, the open panel, dialogs and the tutorial pane
  stay free.
- Escape with no command open ends the step (`step_left`, reason
  `escape`). The callout's Leave Tutorial, the pane's and Help › Leave
  Tutorial end it and close the tutorial (reason `leave`).

## Events

The events come to `mitcad.on` listeners and, for MCP, in the resource
`mitcad://ui-guide`'s `last_event`. Each has `event`, `step` (the step's
id) and `sequence` (one per step started).

| Event | Fields |
|---|---|
| `step_done` | `done`: `{"command": <id>, "committed": true}`, `{"started": <id>}`, `{"input": <key>, "value": <text typed>}`, `{"pick": <kind>}` or `{"mode": <mode>}`. |
| `step_left` | `reason`: `escape`, `leave`, `ended` (by the page or MCP), `replaced` (by another step), `closed` (the tutorial closed). |
| `refused` | `command`: the id of a command the step held back. |

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
