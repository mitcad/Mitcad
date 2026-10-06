# Commands in the Mitcad application

How to add or change UI commands, sketch tools and panels, the UI
framework's conventions, and the log lines UI tests read. A feature
command is a declarative definition plus a little glue between its inputs
and the model's feature definition (`core/model/src/api/commands.md`);
the framework does the rest. Where the code lives: [Files](#files).

## A feature command

```cpp
CommandDef chamfer(const CommandContext& context) {
  CommandDef def;
  def.id = "solid.chamfer";            // unique, also the settings key of its shortcut
  def.name = QObject::tr("Chamfer");
  def.icon = "chamfer";                // app/icons/chamfer.svg
  def.tooltip = QObject::tr("Bevels edges; a face bevels all its edges");
  def.shortcut = QKeySequence();       // none by default
  def.tab = "SOLID";                   // SOLID or SKETCH; empty: menus only
  def.group = "MODIFY";                // see Ribbon::groups()
  def.pinned = true;                   // a button besides the group menu entry
  def.keywords = {"bevel"};            // the command search finds it by these too
  def.featureType = "chamfer";         // the definition type it adds and edits
  def.inputs = {
      selectionInput("edges", tr("Edges"), SelectKind::Edge | SelectKind::Face, 1, 0),
      choiceInput("type", tr("Chamfer Type"), kChamferTypes, "equal_distance"),
      valueInput("distance", tr("Distance"), ValueKind::Length, "1 mm"),
      valueInput("angle", tr("Angle"), ValueKind::Angle, "45 deg")
          .withVisible([](const CommandState& s) { return s.choice("type") == "distance_angle"; }),
      flipInput("flip", tr("Flip")),
      checkInput("tangent_chain", tr("Tangent Chain"), true),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) -> Built { ... };
  def.load = [](const QJsonObject& def, CommandState& state, const CommandContext& context) { ... };
  def.describe = [](const CommandState& state, const CommandContext& context) { ... };
  return def;
}
```

Register it in the family's `register...Commands(CommandRegistry&, const
CommandContext&)` (called from `MainWindow::registerCommands`); the order
of registration is the order of the group menus and buttons. The toolbar,
menus, command search, shortcut dialog and context menu pick it up from
the registry. (The real Chamfer has sets of edges, see `listInput`.)

Checklist for a new command or feature type:

- An icon in `app/icons` (see [Icons](#icons)); a missing file logs
  `Command <id> has no icon '<name>'`.
- A new feature type also needs an icon in `featureIcon`
  (`browser/DocumentSnapshot.cpp`; the generic `feature` icon otherwise).
- A command that changes the model outside a panel goes through
  `DocumentHost::runModelCommand` (refresh, volume log) and checks
  `canChangeModel`.
- Picks may carry an occurrence: a command whose definition refers to
  bodies of another component must refuse them (the model fails such
  features).
- Every unit a dialog or panel offers starts at millimetres, the thread
  table at ISO metric (`tools/ui-units-test.sh`).

### Inputs

| Builder | Panel | State |
|---|---|---|
| `selectionInput(id, label, filter, min, max)` | a button "N selected" in the input's colour, and a clear button; `max` 0: any number | `state.items(id)`: `Selection` |
| `valueInput(id, label, kind, default)` | a text field taking expressions with units (`20`, `2 in`, `d1 * 2`) | `state.text(id)` as typed, `state.expression(id)` for the model, `state.value(id)` in mm or rad (NaN if invalid) |
| `choiceInput(id, label, {{value, label}}, default)` | a drop-down | `state.choice(id)` |
| `checkInput(id, label, default)` | a check box | `state.checked(id)` |
| `flipInput(id, label)` | a flip button | `state.checked(id)` |
| `textInput(id, label, default)` | a text field, not evaluated (a component's name, a thread's designation) | `state.text(id)` |
| `listInput(id, label, {children}, addLabel, rowLabel, minRows)` | rows of the children with a remove button each and an add button (fillet and chamfer sets); rows of values only are lines of a table (hole positions) | `state.rows(id)`, `state.row(id, i)` (the row's inputs by their own ids), `CommandState::rowKey(id, i, child)` |

Modifiers:

- `withVisible(test)`: the row is shown only while `test(state)` holds;
  hidden inputs are not checked and not highlighted. A list's child gets
  its row's state (`state.row(list, i)`).
- `withAccepts(test)`: narrows a selection beyond its kinds (only straight
  edges and sketch lines: `item.geometry == "line"`); the view then
  highlights and picks only what the input takes.
- `withColor(color)`: a selection's highlight colour; otherwise blue,
  orange, green, purple in input order.
- `withOrdered()`: the picks' order matters (loft sections); the panel
  lists them with up, down and remove buttons.
- `withChoices(options)`: a choice's options follow other inputs
  (`options(state)`, e.g. thread sizes follow the standard); the
  drop-down is refilled when they change, and a value they lack stays as
  an extra entry. `withOnChange(call)`: called after the user changed a
  choice, to set dependent inputs (a new size takes its coarse
  designation).
- `withConvert(form)`: the form a pick is kept in (Pattern's Features
  keeps the feature that made a face; Move/Copy's Components the
  occurrence that places a body).
- `withOnPick(call, keep)`: called after a pick, with `item.at` where the
  click hit, in its component's coordinates. With `keep`, picking an item
  already selected keeps it and still calls back (Hole adds a position
  per click; primitives and Coil put their centre at the click,
  `cmd::placeAt`).
- `withManipulator(handle)`: a value input with a handle in the view.
  `handle(state, context)` returns a `Manipulator` (or none): an arrow
  from `origin` along `direction`, knob at `origin + (value - base) *
  factor` (a diameter: factor 0.5), or a ring about `direction` with
  angle 0 towards `reference` (`cmd::arrow`, `cmd::ring`). Dragging sets
  the value rounded to a zoom-dependent step (whole degrees for rings) in
  the document's unit; a click without a drag picks what lies under it.
- `withToggles(made)`: a text input with dots in the view;
  `made(state, context, preview)` builds them from the preview's report
  (`Manipulator::Kind::Toggle` with `element` and `on`); a click on a dot
  adds or removes its element's number in the text ("2, 5"). Used by the
  patterns' Suppressed input.
- `showsOrigin = false`: the input takes construction geometry without
  showing the origin's planes and axes (Measure).

Values:

- Give the model `state.expression(id)`: the typed text with the
  document's unit added to a bare number (`"20"` → `"20 mm"`). A plain
  number would be millimetres or radians (90 as an angle is 90 rad). The
  panel evaluates values with the model's `evaluate` query while typing
  and shows `= 120 mm` or the model's error.
- Decimal commas: the model reads `12,5` as `12.5`; `state.expression`
  has points, and the field shows the expression with points once Enter
  or focus-out. The application's own checks for a bare number use
  `parsePlainNumber` (`framework/Numbers.hpp`); spin boxes are
  `DecimalSpinBox`. Tests: `tools/ui-decimal-test.sh`, ctest `app.unit`.
- A 0 that would only make a parameter (a taper) can be left out of the
  definition (`cmd::nonZero`, as Extrude does).
- Counts and factors in slots the model reads as lengths (pattern
  quantities, scale factors, a construction path's fraction) go as plain
  numbers (`cmd::countValue`) and come back with `cmd::loadCount`;
  fractions of sweeps and pipes and coil revolutions are unitless slots
  and take the expression.

A required selection that a choice reveals and that is still empty
becomes the active one (Extrude's To Object).

### Glue

- `build(state, context)` returns the definition (`Built{def}`) or
  `Built::failure(message, inputId)` for what the inputs cannot express
  (profiles of two sketches, edges of two bodies). It runs only when every
  shown selection has its `min` items and every shown value is valid.
  `Built::editing` makes the command open another feature's edit instead
  (`CommandHost::editInstead`: Press Pull on a fillet face opens the
  fillet for its radius).
- `load(def, state, context)` fills the inputs from the `feature` query's
  `def` (with `uid` added) for editing. Parameter slots hold names:
  `context.valueText(slot, def["uid"])` gives the feature's own dimension
  as its expression and a borrowed parameter by name, so OK changes the
  own parameter or keeps using the borrowed one.
- `canEdit(def)`: whether the inputs can hold all of a definition. A
  feature with more than the panel shows (the importer's fixed planes or
  axes, an asymmetric fillet size, a hole placed by offsets from edges or
  by sketch coordinates, a partial thread) is not offered for editing,
  so OK cannot drop the rest.
- `editsAlso`: other definition types the command edits (Move/Copy edits
  `move` and `move_occurrence`; `build` gives the type).
- `init(state, context)`: defaults that depend on the model, applied after
  the inputs' defaults and the pre-selection (Extrude and Revolve take the
  newest unused profile).
- `describe(state, context)`: a log line after OK (UI tests read it);
  with `describeBefore` it reads the model as before OK (the edges a
  fillet rounds), else after.
- `enabled()`: whether the command can start.

Two other kinds of panels:

- `modelCommand`: `build` gives `{"commands": [...], "label": "..."}`, run
  on OK as one undo step through `CommandHost::runCommands` (Physical
  Material, Appearance, New Component); no preview, outside the timeline.
- `inspect(state, context)` instead of `build`: an `Inspection` (rich
  text, a log line, a shape drawn over the view, red for a problem, a
  section plane) computed while inputs change; OK closes the panel
  (Measure, Interference, Section Analysis). With `keepsSection` the
  section stays after OK (`CommandHost::showSection`, kept in
  `MainWindow::m_section`) until Remove Section Analysis or a new
  document. Inspections draw with `setPreview`, so a command started
  during one ends it.

References (`commands/CommandSupport`):

- `SelectionItem::reference()` gives the JSON reference of an item
  (commands.md, "References to geometry"): `{"sketch", "region"}` for a
  profile, `{"body", "face"}`, `{"body", "edge"}`, `{"body", "vertex"}`,
  `{"body"}`, `{"sketch", "curve"}`, `{"sketch", "point"}`, or a datum id
  (`"xy"`, `"y"`, `"F5"`).
- `cmd::itemOf(ref, context)` is its inverse; invalid for fixed geometry
  (`isShowable`), which no input can show. `pathOf`/`pathItems` do the
  same for paths, `objectOf`/`loadObject` for extents' objects.

### What the framework does

- Pre-selection goes to the first input that takes each item. The first
  selection still needed is active (else the first); a click toggles an
  item in the active input, a one-item input takes the new one, and a
  full input passes on to the next. Window selection adds what the window
  holds of one kind. Clicking an input's button activates it. While an
  input takes construction geometry the origin planes and axes are shown.
- Items made by the previewed feature itself cannot be picked: picks are
  checked against the bodies before the feature.
- Previews run 150 ms after the last change and are cached by definition;
  the model caches results too, so OK does not compute again. A failed
  preview shows the model's message and the selections in red and OK is
  refused; missing selections show a hint ("Select edges.").
- OK (Enter, wherever the focus is) runs `add_feature`, or
  `edit_feature` when editing. Esc cancels.
- Editing rolls the timeline back to just before the feature, so picks
  name geometry there and the preview adds the edited definition at that
  point. OK undoes the roll-back and edits (one undo step); Cancel undoes
  it too. A feature type is edited by the command whose `featureType` it
  is (timeline double-click, the context menu's "Edit <feature>").
- The context menu offers the feature commands whose first selection
  input takes everything selected, "Edit <feature>" for an item a feature
  made, and Repeat; in sketch mode, model faces, edges and vertices get
  Project.
- Inputs that take `SelectKind::Feature` get the timeline's and browser's
  clicks on features (`DocumentHost::takesFeatures`); a face clicked in
  the view goes in as the feature that made it (`withConvert`).
- Handles (`ManipulatorOverlay`) are drawn for shown value inputs that
  have them, the active one's knob orange. The overlay follows the camera
  through `OcctViewer::viewChanged` and `toWidgetF`; a new display style
  or section view must keep these right.
- Long panels scroll (`m_panelScroll`); adding or removing a list row logs
  the panel's places again.

## Actions and tools

Commands without a panel set `kind` to `Action` (runs at once: Create
Sketch, Undo, Fit, Command Search) or `Tool` (an interactive tool in the
view: the sketch tools), a `mode` (`Model`: outside sketches, `Sketch`,
`Any`) and `run`. No command runs while a feature command is active,
except those with `duringCommands` (Fit, standard views, camera, visual
styles, grid, environment, Preferences, the shortcut overview). A `Model`
feature command whose first input takes profiles (Extrude, Revolve) can
also start in a sketch: the sketch is finished first and its selected
profiles go to the command. Measure is `Mode::Any`: in a sketch, the
sketch pauses as for sketch panels; sketch curves and points are measured
in the application on the shapes the view shows, the rest by the model's
`measure` query.

View and file commands are `Action`s with an empty tab (menus, search,
shortcuts).

**Shortcuts and macOS.** `def.shortcut` is the default key (Restore
Defaults and the saved settings go by it); `def.alternates` are further
keys while the shortcut is the default (Redo's Ctrl+Shift+Z, Fit's F6 and
Delete's Backspace on macOS). A platform's other default is an
`#ifdef Q_OS_MACOS` in the definition; nothing is remapped at run time. Qt
makes `Ctrl` Cmd and `Meta` Control on macOS. Fit is Cmd+0 there (F-keys
are media keys); Backspace as Delete loses to a focused text field (the
browser and timeline handle it in `keyPressEvent`); no default is Cmd+H,
Cmd+M, Cmd+Q, Cmd+Space or Cmd+Tab. Menu actions Qt's text heuristic could
move to the application menu get no role (`MainWindow::createMenus`);
Exit, Preferences (Cmd+,) and About have their roles, and macOS has a
Window menu.

## Sketch mode

Create Sketch (`MainWindow::startSketch`) sketches on the selected plane or
planar face; with none, it shows the origin planes and waits for a pick
(`Mode::PickPlane`). Finish Sketch (Ctrl+Enter) leaves it. Editing a
sketch moves the timeline marker to just after it and back when it is
finished; everything in between, marker moves included, becomes one undo
step `Edit Sketch1` (`merge_undo`, commands.md, "Undo, revisions and the saved state").

`sketch::SketchController` is sketch mode: an event filter on the view's
mouse events; it keeps the edited sketch as the `sketch` query gives it
(`SketchModel`, re-read after every change by `refreshScene`), and owns
`SketchOverlay` (QPainter in view pixels: constraint glyphs, dimensions,
tool previews, the snap marker, DOF). Clicks on glyphs and dimension
values select them (`SelectKind::SketchConstraint`, `SketchDimension`; a
fixed entity's lock is `fix:<entity>`); double-click on a value edits it;
dragging a value moves it; dragging a sketch point or curve drags it with
the solver (`sketch.drag` per move, merged into one step).

### A sketch tool

A drawing or picking tool is a `sketch::SketchTool` (`sketch/SketchTool.hpp`)
made by a factory and started by a `Tool` command:

```cpp
add({"sketch.line", QObject::tr("Line"), "line", "CREATE",
     [&c] { c.startTool(sketch::lineTool(c)); }, QKeySequence(Qt::Key_L), true, tooltip});
```

- `move(snap)`, `press(snap)`, `release`, `doubleClick` get the cursor
  snapped (`SketchSnap`: points, origin, midpoints, centres, quadrants,
  intersections, onto curves, the grid; tolerances in pixels). Picking
  tools return `snaps() == false` and use `controller.entityAt(screen)`.
- The pointer is the precision cursor (`framework/Cursors.hpp`): a cross
  with the tool's icon as a badge (`controller.setToolIcon`, given by the
  registration), a square in its gap while snapped to a point.
  `OcctViewer::setSketchInput(true, icon)` sets it, `setSketchSnapping`
  follows the snap; built for the screen's device pixel ratio (again on
  `QEvent::DevicePixelRatioChange`) and the system's pointer size, cached.
- `paint(ToolPreview&)` gives the rubber band in sketch coordinates (with
  `glyph` for an inferred constraint, `dimension` for one being placed).
- `controller.setFields({{"length", tr("Length"), Kind::Length}, ...})`
  shows value boxes at the cursor (Tab moves, Enter calls `confirm()`);
  `setLive` shows the cursor's value while nothing is typed;
  `fieldValue`/`fieldExpression` give a typed one (evaluated by the
  model's `evaluate` query).
- `confirm()` (Enter) and `cancel()` (Esc: one step back; false at the
  start ends the tool).
- `SketchOp op(controller)` is one undo step of model commands:
  `op.run(cmd)` (refusals keep the model's reason for `refuse`),
  `op.pointInput(snap)` (an existing point where it snapped, the origin's
  fixed point made when needed, else `[x, y]`), `op.constrain(point,
  snap)` (coincident, midpoint, on-curve, intersection constraints for a
  new point), `op.addConstraint`, `op.addDimension(def, typed)` (refusals
  only logged), `op.commit()` (merges the steps, refreshes) or
  `op.rollback()`.
- Typed values become dimensions; snapped points get constraints; the
  line tool infers horizontal, vertical, perpendicular, parallel (to the
  line last passed over) and tangent within a few pixels.

### Sketch commands with panels

Offset, Mirror, the patterns, Project, sketch Fillet and Chamfer,
Move/Copy and Insert DXF into the edited sketch are feature commands with
`sketchCommand = true`: `build` returns a `sketch.*` command (or
`{"commands": [...]}`), the preview runs it on the document and undoes it
when an input changes, OK keeps it as one undo step, Cancel undoes it.
Their selection inputs take `SelectKind::SketchCurve`/`SketchPoint` of
the edited sketch (`ofSketch`, `linesOf` in `SketchCommands.cpp`; not
texts). While a panel is open, sketch mode's mouse handling pauses
(`SketchController::setPaused`).

Texts, patterns and offsets stay editable:

- **Text** (`TextTool`): a click places a text, a drag draws its frame (a
  construction rectangle, top aligned). A text is displayed as its glyph
  outlines (one `SketchEntityDisplay` per text, id `t<n>`, a compound of
  edges, also outside sketch mode); its letters are profiles.
- **Edit Text** (`sketch.edit_text`), **Edit Pattern**
  (`sketch.edit_pattern`) and **Edit Offset** (`sketch.edit_offset`) are
  panels on the picked text, pattern curve or offset curve; `init` fills
  them from `SketchModel::text`, `patternOf`, `offsetOf`. A double-click
  on such a selected curve opens its panel (`SketchHost::trigger`).
- A derived offset spline (of an ellipse or spline) shows only its ends
  as points: the query marks what the offset computes `derived`, and
  `SketchModel` drops the other control points (no snapping to them).
  Dragging or moving it carries its source along at the same distance.

## Browser, timeline and parameters

The browser (left) and timeline (bottom) are rebuilt from a
`DocumentSnapshot` after every model change (`MainWindow::refreshScene`,
only when the answers changed). They only show and emit;
`BrowserController` decides and runs model commands through
`DocumentHost::runModelCommand` (refreshes, logs volume changes) or
`runModelCommands` (one undo step). Model changes need the idle mode
(`canChangeModel`); light bulbs also work while sketching.

- **Rows:** a row's `SelectionItem` (`BrowserNode::item`) carries the
  occurrence path. Light bulbs: `set_body_visible`,
  `set_occurrence_visible`, `set_feature_visible`, `set_origin_visible`
  (commands.md, "Visibility and display"); a folder sets all of its own as
  one undo step. The model decides what is shown (the `timeline` query's
  `visible`, `DocumentSnapshot::featureShown`): a sketch whose bulb was
  not set is hidden while a feature uses it. The view shows only what is
  shown (`instances` leaves hidden bodies out). A fully constrained
  sketch's icon is `sketch-locked` (the `timeline` query's `dof`).
- **Paste / Paste New** place the copy where the original is and open
  Move/Copy on it (`DocumentHost::startCommand` with the occurrence
  selected); OK keeps the moved copy as its own placement
  (`set_occurrence_transform`, not a `move_occurrence` feature), paste and
  placement one undo step; Cancel takes the paste back.
- **Selection:** rows set the selection outside commands
  (`DocumentHost::pickItems`); in a command a row goes to the active input
  like a click in the view; while Create Sketch waits for a plane, a plane
  row starts the sketch.
- **Occurrences in the view:** a body placed by an occurrence is displayed
  with the occurrence's transformation on its AIS object
  (`BodyDisplay::placement`), so picks name the body's own faces, edges
  and vertices and carry `SelectionItem::occurrence` ("O1/O4"; empty in
  the root). Sketch curves, profiles and datums of a component are placed
  by its first occurrence (`MainWindow::placingOccurrence`).
  `MainWindow::itemShape` places highlights by the occurrence;
  `SelectKind::Component` and `SelectKind::Sketch` are highlighted as
  their bodies and curves.
- **Timeline:** status `warning` (message in `error`) is yellow, failed
  red. A feature that made no body of its own (fillet, hole, join) is
  selected as a `SelectKind::Feature` item placed by its component's first
  occurrence; `itemShape` gives the faces whose names start with `<uid>:`
  on the bodies at the marker. `pickItems` and `keepSelected` leave out a
  feature without such faces. Delete asks with the `dependents` query.
  Dragging the marker runs `set_marker` per crossed feature and merges the
  steps into one `Move Timeline Marker` (none when it ends where it
  began). Dragging a feature checks `can_reorder`. Groups are the model's
  (`group_features`); folding is the window's (`TimelineWidget`
  `m_collapsed`), not saved. Colours of the marker, buttons and strike
  line come from the palette and are repainted on a palette or style
  change (`TimelineWidget::changeEvent`). Test: `tools/ui-theme-test.sh`
  (the scheme switched with `MITCAD_TEST_COLOR_SCHEME`).
- **Change Parameters** (`solid.parameters`): each cell change is one
  model command and undo step; a bare number gets the parameter's unit;
  refusals stay in the cell in red with the reason. Favourites are
  `set_parameter` `favorite`. `--set name=value` sets a value the same way.

## The view

`ViewController` (`view/`) registers the view commands and keeps display
settings in the user's settings (`ViewSettings`, group `view`);
`OcctViewer` draws them.

- **Orientation cube** (OCCT's `AIS_ViewCube`): a drag that starts on it
  orbits (passed to OCCT with its own key flag `kCubeDrag`); sketch mode
  leaves presses on the cube to the view (`OcctViewer::cubeInteraction`).
  Create Sketch's plane pick points keep away from the cube
  (`cubeArea`, `planePickPoint`), and the datums' logged pick places are
  where no other datum lies. The arrows shown when looking straight at a
  face: `OcctViewer::turnCube`.
- **Camera:** `turnTo(direction, up)` fits and animates
  (`AIS_AnimationCamera`, 0.4 s; Preferences can turn it off);
  `camera()`/`setCamera()` take a `CameraState` (eye, target, up,
  projection, height), which named views store; `lookAlong` and
  `lookHome` serve Look At and named views. Fit (F6) and sketch mode's
  look at the plane are instant. The camera is logged once it has kept
  still for 250 ms.
- **Visual styles:** hidden edges use two z-layers after the default one:
  the bodies' edges dashed without a depth test, then the visible edges
  against the faces' depth. Wireframe with Hidden Edges draws the faces
  unlit in the background's middle colour, because OCCT draws see-through
  faces after every layer sharing the depth buffer. Bodies keep
  `BodyDisplay::color`; previews, highlights and overlays are untouched.
- **Layout grid** (`LayoutGrid`): on XY, or the sketch plane in sketch
  mode, in a layer under everything, left out of Fit. Automatic spacing:
  the largest 1 or 5 × 10ⁿ mm whose lines are at most 35 px apart
  (`layoutGridStep`), major line every fifth. Snap to Grid snaps to the
  shown minor lines (`OcctViewer::gridStep`).
- **Navigation schemes** (`NavigationScheme`, saved by id as
  `view/navigationScheme`; the old index `view/navigation` is read and
  replaced): `middle-pan` (default), `alt-buttons`, `middle-pan-f4`,
  `middle-orbit`, `right-orbit`, `trackpad` (the default on macOS:
  two-finger scroll pans, Alt + scroll orbits, Shift or Ctrl + scroll
  zooms, Alt + left orbits). Pinch, rotation and two-finger double tap
  work in every scheme (`OcctViewer::event`).
- **Preferences** (`tools.preferences`): one page per group (General,
  Navigation, Display, Cache, Version Control, Updates, 3D Print) so the
  window fits 1366 × 768; each page is also a command
  `tools.preferences.<page>` that the search finds by its settings. OK
  saves every page. On macOS the pages are the panes of `SettingsWindow`
  (Settings, Cmd+,), whose changes apply as they are made.
- **Named views** are the document's (`add_named_view` and friends); a
  view named `Home` is the document's home.
- **Section Analysis:** the cut faces are drawn as caps
  (`MainWindow::showSectionCaps`: the clipped bodies' faces in the plane);
  the clip is the model's `analysis_shape {"shape": "clip"}`.

**What the view keeps between refreshes.** `refreshScene` runs after every
model change; the view keeps what did not change: bodies with the same
shape, placement (compared by value, `sameTransformation`: a new
`TopLoc_Location` is never `IsEqual` to an old one), style and colour;
profiles whose face the main window passes again (`m_profileFaces`, kept
while the `profiles` query's region `hash` and the placement stay);
sketch entities with the same `signature`. A new display path must keep
to this: give the view the same shape object for the same geometry. The
commands' availability checks share query answers while actions are
updated (`m_queryMemo`).

**Windows session 0** (the VM's tests over SSH): the window is never
exposed, so `update()` paints nothing; `OcctViewer::requestFrame` (used
instead of `update()`) then draws the frame directly. Popups close at
once there, so the Windows tests start commands by shortcuts.

## Background computation

Opening, every model command (also undo, redo, `recompute`, the browser's,
timeline's and Change Parameters' commands) and every preview
(`CommandHost::modelPreview`, sketch mode's `CommandHost::modelCommand`)
run as jobs on the model's worker thread (`MainWindow::runJob`,
`installDocument`, `command()`; design in
[docs/architecture.md](../docs/architecture.md)). Callers stay
synchronous; a job under 50 ms behaves as inline. Rules:

- The caller waits in a nested event loop where timers and queued
  signals still run. Everything that reads the document goes through
  `MainWindow::idleDocument()`, which asserts (debug) and throws
  `ModelBusy` during a job. What can come from the nested loop waits or
  is refused: preview and drag timers restart, picks, the command search
  and the browser's and timeline's queued calls wait (`whenIdle`;
  `BrowserController::follow`), context menus are dropped, `runCommand`
  refuses, autosave skips.
- Drags end when the progress dialog blocks the window
  (`QEvent::WindowBlocked`: the view, sketch drags, manipulators, the
  timeline marker).
- While a job computes the window's document, the UI thread must not read
  or triangulate its shapes, only draw what the view already made of
  them.
- A cancel takes back what started it as a whole: the command is rejected
  with `the computation was cancelled` (`ComputationCancelled`;
  `lastCancelled()`), `runModelCommands` undoes the commands before it,
  Change Parameters keeps the old value, OK leaves its panel open, Finish
  Sketch stays in the sketch, Cancel of an edit panel whose roll-forward
  is cancelled closes with the timeline rolled back. A cancelled preview
  is not cached; OK then computes the command itself. A cancel inside a
  single kernel call ends when it returns ("Cancelling...").
- A cancelled open keeps the previous document; closing the window during
  a job cancels it and closes when it is back.
- Test switches (`framework/Diagnostics.hpp`):
  `MITCAD_TEST_RECOMPUTE_DELAY_MS` (`400,undo=800`: per job name),
  `MITCAD_COMPUTE_INLINE=1` (jobs on the UI thread);
  `tools/ui-compute-test.sh`.

Limitation: queries and analyses (Measure, Interference, Physical
Properties), Save's commit and the status bar's history still run on the
UI thread.

## Files

### Source files

| File | What |
|---|---|
| `framework/Command.hpp` | `CommandDef`, `InputDef`, `CommandState`, `Built`, `CommandContext` |
| `framework/Selection.hpp` | `SelectKind`, `SelectFilter`, `SelectionItem` (a picked entity by its model name) |
| `framework/CommandSession.*` | a running feature command: state, panel, selections in the view, previews, OK and Cancel, editing |
| `framework/CommandPanel.*` | the panel generated from the inputs |
| `framework/CommandRegistry.*` | every command with its `QAction`; shortcuts saved in the settings |
| `framework/Ribbon.*` | the toolbar: tabs, groups, group menus |
| `framework/CommandSearch.*` | the command search (S) |
| `framework/ShortcutDialog.*` | Tools > Keyboard Shortcuts |
| `AboutDialog.*` | Help > About Mitcad: version, licence, third-party credits (OCCT and Qt versions) |
| `framework/ModelShapes.*` | display shapes of datums and sketch geometry from JSON |
| `framework/ManipulatorOverlay.*` | handles a command draws over the view: distance arrows, angle rings |
| `framework/Appearances.*` | the appearance library (colours of `set_body_appearance` ids, `appearanceColor`) and the physical materials |
| `framework/Cursors.*`, `framework/Icons.*`, `framework/Numbers.*` | the sketch tools' pointer; icons from `app/icons`; numbers with decimal commas |
| `framework/AppSettings.*` | general settings (autosave) |
| `framework/DesktopEntry.*` | the Linux desktop file and icons an AppImage installs for itself |
| `framework/ModelWorker.*`, `framework/ComputeProgress.*`, `MainWindowJobs.cpp` | background computation: the worker thread, a job's control and stage (`ModelJob`, `AttachedJob`), the progress dialog, `MainWindow::runJob` |
| `framework/ResultCache.*`, `framework/CachePreferences.*`, `framework/CacheDiagnostics.*` | the result store's folder, build id and settings, its clean-up; Preferences' Cache group; Help › Diagnostics |
| `framework/Diagnostics.hpp`, `framework/TestSync.*` | environment switches for test logs and delays; the UI tests' sync |
| `commands/CommandSupport.*` | shared by the families: references to and from items, paths, operations, extents' objects, values, plane frames and axes for manipulators |
| `commands/CommandFactories.hpp` | the SOLID tab's feature commands, made in the family files below |
| `commands/CreateCommands.cpp` | `registerCreateCommands`: the CREATE group in menu order |
| `commands/ExtrudeCommands.cpp` | Extrude and Revolve |
| `commands/SweepCommands.cpp` | Sweep, Loft, Rib, Web, Coil, Pipe, Helix |
| `commands/HoleCommands.cpp` | Hole, Thread |
| `commands/PrimitiveCommands.cpp` | New Component, Box, Cylinder, Sphere, Torus |
| `commands/PatternCommands.cpp` | Rectangular, Circular and Path Pattern, Mirror |
| `commands/ModifyCommands.cpp` | `registerModifyCommands`: the MODIFY group in menu order |
| `commands/FilletCommands.cpp` | Press Pull, Fillet and Chamfer |
| `commands/FaceCommands.cpp` | Shell, Draft, Offset Face, Replace Face, Split Face, Split Body, Delete Face |
| `commands/MoveCommands.cpp` | Scale, Combine, Move/Copy, Align, Physical Material, Appearance |
| `commands/ConstructCommands.cpp` | `registerConstructCommands`: every construction plane, axis and point, from a table |
| `commands/InspectCommands.cpp` | `registerInspectCommands`: Measure, Interference, Section Analysis, Physical Properties |
| `commands/SketchCommands.cpp` | the SKETCH tab: drawing tools, Sketch Dimension, constraints, Offset, Mirror, patterns, Project, sketch Fillet and Chamfer, Move/Copy, Trim, Extend, Construction, Delete |
| `sketch/SketchController.*` | sketch mode: mouse and keys, typed values, drags, `SketchOp` |
| `sketch/SketchTool.hpp`, `DrawTools.cpp`, `EditTools.cpp` | the tools (`SketchTool`) and their factories |
| `sketch/SketchOverlay.*` | drawn over the view: constraint glyphs, dimensions, previews, snaps, DOF |
| `sketch/SketchSnap.*`, `SketchGeometry.*`, `SketchModel.*` | snapping; plane curves; the `sketch` query parsed |
| `sketch/SketchPalette.*` | the sketch palette (options, constraints, DOF) in the command panel's place |
| `browser/DocumentSnapshot.*` | the document's structure read once per change; feature icons; body volume logs |
| `browser/BrowserPanel.*` | the browser (model tree) |
| `browser/TimelineWidget.*` | the timeline with the marker and playback buttons |
| `browser/ParametersDialog.*` | Change Parameters |
| `browser/BrowserController.*` | what the browser, timeline, parameters dialog and failed-features summary do |
| `browser/DocumentHost.hpp` | what they ask of the main window |
| `view/ViewController.*` | the View menu: standard views, Look At, camera, visual styles, grid and snaps, environment, named views, the cube's menu, Preferences, the keyboard and mouse overview; `ViewSettings` |
| `LayoutGrid.*` | the layout grid |
| `files/MainWindowFiles.cpp` | the File menu: open, recent files, close, import, export, Insert Component, drops |
| `files/FileFormats.*`, `files/FileDialogs.*` | file kinds by extension and dialog filters; Export, Insert DXF, Insert Mesh, linked or copy, the import report |
| `files/F3dImport.*` | `.f3d` and FreeCAD import in a worker process behind a progress dialog |
| `files/Autosave.*`, `files/Recovery.*`, `files/MainWindowAutosave.cpp` | autosave and recovery |
| `files/MainWindowVersions.cpp`, `files/VersionDialogs.*` | saving versions: Save and Save Version in a versioned project, New Project, Start Version History, the author, changes and renames made outside Mitcad, the status bar's version |
| `files/VersionHistory.*` | the Version History window |
| `files/Remote*.*` | remote repositories: connect, open from remote, sync, check for newer versions, settings; `RemoteTask` runs one operation on a thread of its own |
| `files/Print3d.*`, `files/MainWindowPrint.cpp` | 3D Print: slicers found, settings, file names, the dialog and Preferences group; the command |
| `update/` | automatic updates ([docs/updates.md](../docs/updates.md)) |
| `MainWindow.*` | the host: document, modes (idle, picking a sketch plane, sketch, command), selection, context menu, actions |
| `OcctViewer.*` | the 3D view: bodies in the visual style, previews, highlights, picking, camera, orientation cube, navigation |
| `icons/*.svg` | own icons (see [Icons](#icons)) |

### Import

By extension (`files/FileFormats`); `--open` and dropped files take the
same ways without questions (mm, the XY plane, linked).

| Kind | How |
|---|---|
| `.f3d`, `.f3z` | a new document with the design's history (worker process, below) |
| `.FCStd` | a new document with the bodies and PartDesign/Part history (`import_fcstd`, same worker; commands.md, "FreeCAD import (.FCStd)"); no Stop |
| STEP, IGES, BRep | a base feature (`import_file`, one undo step `Import part.step`) |
| STL, OBJ | mesh bodies, after asking the unit (Insert Mesh) |
| DXF | a new sketch on a plane (`sketch.create` + `sketch.import_dxf`, one undo step `Insert drawing.dxf`), or into the edited sketch; the dialog uses the `dxf_info` query (origin placement, layers) |
| `.mitcad` | Insert Component, linked or as a copy (`insert_component`) |

**The `.f3d` worker** (`files/F3dImport`): the application runs itself as
`mitcad --import-worker <file> --output <project> --result <json>` (no
window; `main.cpp`), which runs `import_f3d` with `hang_limit` 90 s and
no time limit on a new document under a job (`attach_job`) and writes the
project file and result; it also gets `--result-store` and
`--persist-min-ms` and stores the design's results. A kernel crash or
hang ends only the worker. Progress: the worker runs with
`MITCAD_IMPORT_TRACE`, and its per-item lines (`import: [t] item <n>:
<name>` when an item starts, `import: [t] <item>: <outcome> in <s> s`
when done; core/import) are counted against the timeline's length
(`import_f3d`'s `list`); a change of that format only loses the count.
**Stop and Keep What Is Imported** writes `stop` to the worker's stdin;
the worker cancels its job, which `import_f3d` takes as a stop request
(`core/import/README.md`, *Stop*). **Cancel Import** kills the worker;
nothing of it is kept. The window then loads the project unsaved (`*`)
and shows the import report. For tests,
`MITCAD_TEST_RECOMPUTE_DELAY_MS=import_f3d=<ms>` slows every definition
the worker tries.

### Export and 3D Print

**Export:** STEP AP214/AP242, IGES, STL, OBJ, BRep, 3MF (appearance
colours as `colors`) for all or the selected bodies (faces, edges and
vertices stand for their bodies), and a sketch to DXF (`export_sketch`).
STEP and IGES are written in millimetres unless the Unit row says
otherwise. Bodies are where the design shows them (the model's default;
a STEP of moved bodies is an assembly); with occurrences, Coordinates can
give `coordinates: component`.

**3D Print** (`make.print3d`; `files/MainWindowPrint.cpp`,
`files/Print3d`): the selected bodies (an occurrence: the bodies shown in
it), or with nothing selected those with a shown instance (`instances`);
sheet and empty bodies (`geometry::body_kind`) are left out with a
warning. Settings group `print`: `format`, `refinement`, `deviation`,
`angle`, `slicerName`, `slicerProgram`, `slicerArguments`,
`slicerArgumentsAfter`.

- Files go to `<QDir::tempPath()>/mitcad-print/<design>`
  (`preparePrintFolder`: emptied before each send; `mitcad-print` is made
  for the user alone and refused when it is a link or another user's,
  since /tmp is shared on Linux), through the model's `export`: 3MF one
  file named after the design with `colors`; STL one per body
  (`printFileNames`: safe and unique, `Body1.stl`, `Body1_2.stl`) in
  design coordinates, and one per occurrence of a body shown more than
  once (`printPieces` from `instances`, `export` with `occurrence`:
  `Pin (Arm_2).stl`).
- The slicer starts once: `QProcess::startDetached(program, arguments +
  files + argumentsAfter, folder)`; without one, or when it fails, the
  folder opens (`QDesktopServices`).
- `findSlicers`: on Windows Program Files and the user's Programs (`Bambu
  Studio\bambu-studio.exe`, `OrcaSlicer\orca-slicer.exe`,
  `Prusa3D\PrusaSlicer\prusa-slicer.exe`, the newest `UltiMaker Cura
  *\UltiMaker-Cura.exe`) and the registry's Uninstall lists (read only);
  on Linux `PATH`, Flatpaks (`flatpak run --file-forwarding <id> @@
  <files> @@`: the sandbox has its own /tmp) and desktop files
  (AppImages). `SlicerPreferencesBox` is the Preferences group.
- Tests: `tools/ui-print-test.sh` (a fake slicer script), the Windows
  workflow test's part 10, `app.unit`.

### Unsaved changes

The title's `*` and the question on New, Open, Close and exit follow the
model's `modified` (`document` query), which compares the revision with
the one marked saved (`mark_saved` after Open and Save; a new document
starts saved, an imported `.f3d` design does not). Undo back to the saved
state clears it (`tools/ui-file-test.sh`).

### Autosave

`files/Autosave`. Every 5 minutes by default (settings `autosave/enabled`,
`autosave/minutes`, 1–60), a document the model calls modified whose
revision was not written yet goes to the recovery folder `autosave` in the
user's local application data (`~/.local/share/Mitcad/Mitcad/autosave`,
`%LOCALAPPDATA%\Mitcad\Mitcad\autosave`; `MITCAD_AUTOSAVE_DIR` moves it),
never a project's folder and never a version.

Each run is a session with a UUID. Its files:

- `<id>.lock`: a `QLockFile` from the first write to the end; stale only
  when the process is gone.
- `<id>.mitcad`: the project file.
- `<id>.json`, written last: `"format": "mitcad-autosave"`, `version` 1,
  `session`, `pid`, `application`, `application_version`, `document`,
  `path` and `base_digest` of the file it was opened from or saved to,
  `saved_at`, `revision`, `project`, and the project file's `size` and
  `digest`. Digests are FNV-1a 64 in 16 hex digits.

The project file is made on the UI thread, never while a job has the
model (`Autosave skipped: busy`, retried in 5 s) or while a sketch
command's preview is applied; an edit that rolled the timeline back is
written with the marker it restores (`to_json_with_marker`). Both files
are written on a thread of their own with `QSaveFile`. Save, Save As, a
new document (New, Open, Close, an import) and undo back to the saved
state remove the session's files; a normal end removes them and the lock;
a crash leaves them. An imported `.f3d` design is written at once.

### Recovery

`files/Recovery`, `files/MainWindowAutosave.cpp`. When the window shows
(`main.cpp`; not with `--no-recovery` or `--screenshot`; after an `.f3d`
given with `--open` has come in) and by File › Recover Documents
(`file.recover`), sessions whose lock `QLockFile::tryLock(0)` takes are
found and held until the user decides (a running instance's sessions are
never offered). A lock without files is removed. A session whose metadata
is missing, unreadable or of another version, or whose project file is
missing or does not match `size` and `digest`, is damaged and can only be
discarded.

Restore reads the project file, follows links relative to it and
computes it as a job (`installDocument`), not marked saved (modified, no
undo history), with the file's path or its untitled name. Autosave writes
it into this session at once, and only then removes the earlier
session's files (`AutosaveManager::recovered`). Later keeps the sessions,
unlocked. Save of a recovered document compares its file with
`base_digest` first (`askOverwriteChanged`) and asks before overwriting a
changed file.

Limitation: only a recovered document's Save checks the file on disk; a
document opened normally does not notice a file changed by another
program (one in a versioned project does at Save); there is no
`QFileSystemWatcher`.

### Result store

`framework/ResultCache`, `framework/CachePreferences`; the model's side in
commands.md, "Result store". Every document the window gets
(`installDocument`: open, recovery, import, New, Close, start-up) uses
`resultStoreDirectory()` (`QStandardPaths::CacheLocation` + `/results`;
`MITCAD_RESULT_STORE` for tests, `off` for none; empty when Preferences
turns it off), with the build id `geometry::kernel_build_id()` and the
document's name. The open job writes the results that took at least
100 ms (`MITCAD_RESULT_STORE_MIN_MS`) in the same job; Save writes them as
a job after the file (`MainWindow::afterSaved` → `persistResults`), under
the saved name; autosave and recovery do not. At start-up and after
writes, a pool thread removes the least recently used files beyond the
size (`result_store_gc`). Settings: `cache/memoryMegabytes` (default a
quarter of the machine's memory, `set_memory_cache_budget`),
`cache/disk`, `cache/diskMegabytes` (5 GB); Clear is `result_store_clear`.

**Help › Diagnostics** (`help.diagnostics`, `framework/CacheDiagnostics`):
the model's `cache` query with the store's files and the process's
memory; Clear Memory is `clear_cache` as a job; Export Report writes the
query's JSON, or text for a `.txt` name.

### Versions

`files/MainWindowVersions.cpp`, `files/VersionDialogs`; the model's side
in commands.md, "Version history".

- `writeFile` asks `open_project` whether the file is in a project with
  version history (`versionedProject`; open a `Project` per use, it is
  not shared between threads); there Save writes the file and then
  records it (`commit` of its path, the author always given) on the UI
  thread. `MainWindow::afterSaved` is where a save's follow-ups go.
- The message is `automaticVersionMessage` of the `changes_since_saved`
  query, read before saving marks the state: `Save <file>: <steps>`, the
  steps' labels joined while the line stays within 72 characters, then
  `and N more` with the whole list in the body.
- Author (`versionAuthor`): git's (`identity`) while `versions/useGit` is
  on (default) and git has one, else `versions/name`, `versions/email`;
  confirmed once (`versions/confirmed`).
- Before Save overwrites its file, `askVersionConflict` compares the
  `status` command's blob ids with those of the open or last save
  (`m_versionBase`, by path). After anything that writes the file
  (restore), call `rememberVersionBase` (or reopen) and
  `updateVersionStatus`.
- Opening runs `follow_rename` first: a file renamed or moved outside
  Mitcad within its project takes its display state along, and Save
  records the rename first.
- New Project: `projectsDirectory()` (Documents/Mitcad, or
  `MITCAD_PROJECTS_DIR`), `create_project_repository`,
  `init_project_history`; cancelled at the author, what it added is
  removed again. The new design remembers the folder (`m_projectDir`).
- Versions are per file: `v3` is the file's history length (`history`),
  not the project's commits.
- Tests: `tools/ui-version-test.sh`; the Windows workflow test's part 8.

**Version History** (`file.version_history`; `files/VersionHistory`;
commands.md, "Version history"): reads the history on its own
thread (`QThread::create` with its own `open_project`; the window waits
for it when it closes): `history`, then `history` with `summaries`.
Compare with the previous version uses `diff` on the UI thread; with the
open design `load_version` and `diff_documents`; Compare Geometry is a job
`compare` (`compareVersionGeometry`). Open reads the version as an
untitled document `part v3` (`openVersion`, marked saved). Restore records
unsaved changes or a change in the folder first, then `restore` and
`loadProject`. Save Copy As: `load_version` and `save_project` `auto`,
plus a version in a versioned project. Previews: after `writeFile`,
`openVersion` and `restoreVersion`, the view is grabbed
(`QOpenGLWidget::grabFramebuffer`) and scaled to 256 px when the blob has
none (`saveVersionThumbnail`), stored as `thumbnails/<blob>.png` in
`AppLocalDataLocation` (`MITCAD_THUMBNAIL_DIR` for the Windows tests).

Limitations: previews exist only for versions saved, opened or restored
here, and are never removed; linked components' relative paths are not
rewritten when a design is saved or copied to another folder or moved
into a new project; a recovered document in a project is saved without
the external-change check; closing the window with a very long history
waits until the summaries are read.

### Remote repositories

`files/RemoteController.cpp`: File › Connect Project to Remote, Open
Project from Remote, Sync, Check for Newer Versions, Remote Settings, the
automatic push and the status bar. The model's side: commands.md,
"Remote repositories".

| Model command / bridge function | Used by |
|---|---|
| `connect` (`author` given) | Connect, and Change Address in Remote Settings, with a progress dialog; `remote_info` with `links` first, warning about `external_links` |
| `clone_project(url, dir, control, false)` | Open Project from Remote; then opens the project file of `files` (the only one, or the one chosen) |
| `push` | after each version Save records (Preferences, on by default); a network failure, `not_found` or `other` retries after 60 s, doubling up to 15 minutes |
| `fetch` | on opening a design in a project with a remote, every 10 minutes (Preferences; 0: never), at the first change after opening when the last fetch is over 2 minutes old, after a refused push, and Check for Newer Versions; then `push` if versions wait and the remote has none newer |
| `incoming` | the status bar tooltip: who saved the remote's newest version, and when |
| `incoming` with `path` | after a fetch and after Save: the notice of a newer version of the open file (`file_versions`, `differs`, `changed_here`) |
| `sync` (`author`; after a conflict `resolutions` and `fetch` false) | Sync and the notice's Sync Now; `changed_paths` reopens the design |
| `diff` (`from`: the conflict's `theirs.version.id`, `to`: `mine.version.id`) | Compare in Resolve Sync Conflicts |
| `commit` (`Save <paths> before sync`) | a sync refused for `local_changes`: records the files listed in `uncommitted` first |
| `remote_info`, `remote_remove` | the status bar's state and Remote Settings; Disconnect |
| `git_info_at(program)` | Preferences › Version Control: the chosen git program (a path, or "" for the one found) and its version; the app sets `MITCAD_GIT` from it |

Tests: `tools/ui-sync-test.sh` and the Windows workflow test.

## Selection

`OcctViewer` names picks through the bodies' `geometry::Shape`
(`names_of_face`, `name_of_edge`, `name_of_vertex`) and reports them as
`SelectionItem`s; it keeps no selection of its own. Its pick filter
(`setPickFilter(kinds, test)`) also limits the hover highlight. Selection
outside commands is the main window's (`m_selection`). A window selection
keeps one kind (profiles, bodies, edges, faces, vertices, sketch curves,
points, datums, in that order of preference). Faces win over bodies, so
the bodies filter is off by default.

## Icons

24 x 24 SVG, drawn for Mitcad (MIT, SPDX line first): dark outline
`#2b3a4a` 1.3 px with round joins, solids light blue `#9cc0ec` (tops
`#c9ddf6`, sides `#7aa6dc`), the action in orange `#e07b1a`, sketch
geometry blue `#1f5fc0`, construction planes `#f3c48a`. Rendered with Qt
SVG at several sizes (`themeIcon`).

## Notes on particular commands

- **Helix** (`solid.helix`, `SweepCommands.cpp`): the model's `helix`
  holds only `pitch` and `revolutions`, so a height goes in as a ratio:
  `revolutions` "<height> / <pitch>" or `pitch` "<height> /
  <revolutions>", each operand in parentheses unless it is a name, a
  number with its unit or a call (as the FreeCAD import writes it).
  `load` reads the type back from that form; a ratio in another form
  opens as Revolution and Pitch with the expression as is. A fixed axis
  (the importer's) is not editable.
- **Hole**: Counterdrill is `kind` `counterdrill` with `cd_diameter`,
  `cd_depth`, `cd_angle` (the cone's full angle); Taper Angle is `taper`
  (the wall's angle to the axis), left out when 0 and hidden for tapped
  holes, which the model refuses to taper. Thread and Hole take sizes
  from the model's `thread_sizes`.
- **Press Pull** on one face of a fillet opens the fillet for the radius
  of the set that rounds its edge (`Built::editing`).

## Known limitations

- Sketch mode works in the component's own coordinates, so a placed
  component's sketch is edited where the component's origin is; the
  browser's Create Sketch refuses planes of placed components. A
  component placed twice shows its sketches and datums once.
- A sketch's Show Profile/Points/Dimensions/Constraints options are the
  palette's (`SketchController::setShow…`), not saved.
- Offset Face after Delete Face on the same body fails in the model ("the
  offset produced no solid").
- Bodies the section cuts away cannot be picked while Section Analysis is
  open (the clipped shape is drawn instead).
- Named views keep no visual style; Insert Mesh does not convert meshes
  to solids.
- File drops are not UI-tested (xdotool cannot drag files).

## Logs that UI tests read

| Line | When |
|---|---|
| `View area x y w h` | the 3D view's place in the window (`ui_view_click`) |
| `Ribbon SOLID/CREATE at x,y` | group menus (`ui_click_logged`) |
| `Panel <command> input <id> at x,y [(hidden)]`, `Panel <command> OK at x,y` | a panel opened |
| `Datum xy at x,y`, `Sketch entity F1/c4 at x,y` | an input that takes them became active |
| `Command <name> started`, `Editing F2 with <name>`, `Command <name> cancelled` | |
| `<command> <input>: 2 edges [edge E{…} of F2.b0; …]` | a selection input changed |
| `<command> <input>: d1*2 = 120 mm`, `… is invalid: <reason>` | a value was evaluated |
| `<command>: <input> = <choice>` / `= on` | a choice or check changed |
| `Preview <command>: ok`, `Preview <command>: failed: <reason>`, `<command>: OK refused` | |
| `Added <name> as F5`, `Edited F2`, the command's `describe` line | OK |
| `Press Pull opened Fillet1 for its radius` | a command's inputs asked for another feature's edit, which opens next |
| `Body Body1 (F2.b0): volume a -> b mm3`, `New body …`, `Removed body …` | the bodies OK changed, measured by the model (with `MITCAD_LOG_VOLUMES=1`, which the tests set: the first change after opening a large design would measure every body) |
| `Cosmetic threads shown: 1` | the cosmetic threads drawn as rings changed |
| `Sketches shown: F1, …` | the sketches the view shows |
| `Timing refresh: 36.5 ms`, `Timing display: 1093 ms, 166 bodies, 166 new`, `Timing hover: …`, `Timing frame: …`, `Timing open: …` | with `MITCAD_LOG_TIMING=1` (`framework/Diagnostics.hpp`): how long the steps take, for `tools/perf-measure.sh` |
| `Selected: 1 face [face F2:end(…) of F2.b0]`, `Context menu: A \| B` | outside commands |
| `Create Sketch: select a plane`, `Create Sketch picked …`, `Sketch started on xy, origin (0, 0, 0)`, `Editing sketch F1 (Sketch1)`, `Sketch finished` | sketch mode |
| `Sketch view x0,y0 x1,y1 x2,y2` | where sketch points (0, 0), (100, 0), (0, 100) are in the window, when it changed (`ui_sketch_click x y`) |
| `Sketch Sketch1: 2 DOF`, `… 0 DOF, fully constrained`, `…, conflicts k3+k5` | the sketch's status changed |
| `Sketch tool Line`, `Added line c2 from (0, 0) to (40, 0) (horizontal)`, `Line chain closed`, `Added rectangle 40 x 25 mm at (0, 0) [c1, …]`, `Added circle, diameter 20 mm at (x, y) [c5]`, `Added arc …`, `Added polygon (6 sides) …`, `Added slot, width 12 mm …`, `Added ellipse …`, `Added spline c9 through 3 points`, `Added point p4 at …`, `Added text t7 "…" at …` (or `in a frame from (x, y) to (x, y)`), `Trimmed c3 at (x, y)`, `Extended …` | the tools |
| `Sketch cursor: bitmap 32x32, hot spot 10,10, ratio 1, icon line`, `Sketch cursor: arrow` | the view's pointer when a sketch tool starts or ends, or the screen's ratio changes (logical pixels) |
| `Sketch value length: 40 = 40 mm`, `Sketch value editor at x,y with 40.00` | typed values, a dimension's value editor |
| `Added dimension k5 length d1 = 40 mm`, `Added driven dimension k7 length`, `Editing dimension k5 (40 mm)`, `Dimension k5 = 50 mm (d1)`, `Moved dimension k5 to (x, y)` | dimensions |
| `Added constraint perpendicular k6 on c1, c2`, `Constraint perpendicular refused: <reason>`, `Fixed …`, `Construction on: [...]`, `Deleted c4, k2` | constraints and modify commands |
| `Dragged p5 to (x, y)`, `Dragged c7 to (x, y)`, `Drag of p5 did not move it` | a drag ended: where the point is, or the curve's point nearest the cursor |
| `Double-click on t26: sketch.edit_text`, `Edited text t26: "MIT", Droid Sans, center`, `Edited pattern k8: 4 in all`, `Edited offset k13: 3 mm` | the edit panels of texts, patterns and offsets |
| `Timeline Sketch1 at x,y`, `Timeline marker at x,y`, `Timeline button back at x,y` (start, back, forward, end) | the timeline's items, when they changed |
| `Timeline colours: dark, marker #c8d0da, buttons #f0f0f0, window #323232` (`light, marker #2b3a4a, buttons the style's, …`), `Asking the platform for the dark colour scheme` | the timeline's palette colours at start and when the theme changes; the switch of `MITCAD_TEST_COLOR_SCHEME` |
| `Feature faces highlighted: F3 1, F4 2` / `none` | the faces of the features selected on the timeline (uid and number of faces), when they change |
| `Timeline group Group1 at x,y` (its band, or its folded cell), `Timeline group Group1 collapsed` / `expanded`, `Timeline selected: F1, F2, F4`, `Grouped Sketch1, Extrude1 as Group1`, `Renamed group Group1 to Body`, `Ungrouped Body` | timeline groups |
| `Timeline tooltip: Fillet2: <error>`, `Marker at 2 of 4`, `Moved Plane1 to 0 in the timeline`, `Move of Fillet1 refused: <reason>`, `Suppressed Fillet1`, `Renamed Plane1 to Datum`, `Delete Sketch1: also Extrude1, Fillet1?`, `Deleted Sketch1, Extrude1, Fillet1`, `Delete Sketch1 cancelled` | timeline actions |
| `Browser width 240` (`, scrolls sideways` with a horizontal scroll bar), `Browser Root/Bodies/Body1 at x,y` (its name), `Browser eye <path> at x,y`, `Browser radio <path> at x,y` | the browser's width and visible rows (names from the root), when they changed |
| `Browser selected: <paths>`, `Visibility <path>: hidden`, `Renaming <path>`, `Renamed Body1 to Plate`, `Activated Bracket`, `Isolated …`, `Copied …`, `Pasted Bracket:1 as Bracket:2`, `Placed Bracket:2 at (90, 0, 0)`, `Paste of Bracket:3 cancelled`, `Ground Bracket:2`, `Deleted Bracket:2`, `New component in Root`, `Look at plane F4`, `View top`, `Units: in`, `Found … in the browser` | browser actions |
| `Bodies shown: F2.b0, F3.b0 in O1`, `Body F3.b0 in O1 at x,y` | the shown bodies (with their occurrences) and their middles in the window |
| `Failed features: Fillet2: <error>` / `none`, `Failures button at x,y`, `Revealed Fillet2` | the status bar's summary |
| `Feature warnings: Fillet1: <warning>` / `none`, `Preview Fillet: ok, warning: <warning>` | features that succeeded with warnings, when they change; a preview's warning |
| `Browser sketch DOF: Sketch1 0, Sketch2 4` | the sketches' degrees of freedom the browser shows, when they change |
| `Parameters favorites: width` / `none`, `Parameters width favorite at x,y`, `Parameter width favorite: on = 30 mm` | favourites |
| `Parameters dialog opened`, `Parameters: d1 = width + 5 mm (35 mm); …`, `Parameters d1 expression at x,y` (name, unit, expression, comment), `Parameters add/delete/OK at x,y`, `Added parameter width = 30 mm`, `Parameter d1 expression: width + 5 mm = 35 mm`, `Parameter width expression: d2 / 2 refused: <reason>`, `Delete parameter w refused: <reason>` | Change Parameters (places in the main window's coordinates) |
| `Panel Fillet input sets.1.radius at x,y`, `Panel Fillet input add sets at x,y`, `… input remove sets.1 at x,y`, `… input up_sections.2 at x,y`, `… input down_sections.0 at x,y` | a list's rows and buttons, an ordered selection's buttons |
| `Pick places:` then `Pick face F2.b0/F2:end(…) at x,y`, `Pick edge …`, `Pick vertex …`, `Pick profile F1/r{…} at x,y` | an input became active (with `MITCAD_LOG_PICKS=1`, which the tests set): where a click picks each item it takes (`ui_click_pick`) |
| `Manipulator <command> <input> at x,y`, `Manipulator <command> <input> dragged to <value>` | a handle's knob, a drag ended |
| `Manipulator Circular Pattern suppressed.2 at x,y` (`… (off) at` when suppressed), `Manipulator Circular Pattern suppressed.2 toggled`, `Circular Pattern Suppressed: 2` | a pattern's instance dots |
| `Inspect <command>: <log>`, `Closed <command>` | an inspection's result (Measure: `area 2400, distance 45, angle 90 deg`; Interference: `Body1 x Body2: <volume>` or `none`; Section Analysis: `area …, length …`) |
| `Physical properties: Body1 volume … mass … center (…)` | the Properties dialog opened |
| `Done <command>` | a model command panel's OK |
| `Camera direction -0.577 0.577 -0.577 up … (orthographic)` | the camera came to rest after it moved (direction from the eye to the model) |
| `Layout grid 5 mm, major 25 mm, 18.2 px apart` | the layout grid's spacing changed, logged when the camera rests |
| `Orientation cube places:` then `Orientation cube front at x,y`, `Orientation cube front-top at …`, `Orientation cube front-right-top at …` | with `MITCAD_LOG_ORIENTATION_CUBE=1`, before each camera line: where a click picks each face, edge and corner of the cube in sight |
| `Orientation cube pressed on front at x,y`, `View home at x,y`, `View home`, `View top`, `Look at …` | the cube, the house, standard views |
| `Orientation cube arrow up at x,y` (up, down, left, right, cw, ccw), `Orientation cube arrow cw`, `Orientation cube arrows hidden` | the cube's arrows: where they are when the view comes to rest face on, a click |
| `Visual style Shaded with Hidden Edges`, `Camera perspective`, `Background #525761 #1f2126`, `Layout grid off`, `Snap to grid on`, `Grid spacing 10 mm` | display settings |
| `Saved named view NamedView1`, `View NamedView1`, `Deleted named view …` | named views |
| `Preferences opened: General, 640 x 330` (the page shown and the size the window asks for), `Preferences: navigation middle-orbit, zoom to cursor on, …, autosave every 5 min` (or `…, autosave off`), `Shortcut overview: 23 command shortcuts` | the dialogs |
| `Section caps: 2 face(s) at {"normal":[1,0,0],"origin":[30,…]}` | the section's caps drawn (when they change) |
| `Hide Above Sketch on`/`off`, `In front of the sketch plane: F2.b0 5.0, F5.b0 10.0` | the sketch palette's option; in sketch mode, how far each shown body reaches in front of the sketch plane (mm, when it changes; 0 or less once cut, a body cut away entirely is left out) |
| `Export dialog: STEP AP214 (*.step) to <path>`, `Exported <path>: step, 1 body(ies), mm` (with occurrences `…, mm, design coordinates` or `component coordinates`), `Exported <path>: dxf, 4 entities` | export |
| `3D Print dialog: 3 visible: Plate, Cube, Pin; format stl, refinement high, slicer Fake slicer (/path <files>); slicers None: … \| …`, `3D Print: left out Surface1 (a surface body)`, `3D Print: 3 bodies as stl (high) to <folder>: Plate.stl, Cube.stl, Pin.stl` (a body shown twice: `Pin (Pin_1).stl, Pin (Pin_2).stl`; 3MF: `parts.3mf`… per body `Plate 12 triangles, 1000.000 mm3`), `3D Print: started <program> <arguments>`, `3D Print: <slicer> did not start`, `3D Print: opened <folder>`, `3D Print cancelled`, `Preferences: slicer <slicer>` (or `automatic`) | 3D Print |
| `Export dialog opened: unit mm (mm \| cm \| m \| in \| ft)` (with occurrences also `, coordinates design (design \| component)`), `Insert Mesh dialog opened: unit …`, `Insert DXF dialog opened: unit …`, `Units dialog opened: mm (…)`, `Add User Parameter dialog opened: unit mm`, `Grid settings dialog opened: automatic, fixed spacing 10 mm` | a dialog that offers units opened, with its default first |
| `Panel Hole choices: placement=face, …, standard=iso_metric, size=6, designation=M6x1, class=6H`, `Panel Hole options size: 1 \| 1.1 \| …` | a panel's choices when it starts; the options of a `withChoices` input when they change |
| `Imported block.step as Base1: 1 body(ies)`, `Inserted drawing.dxf into Sketch2: 4 curves, 4 points, 0 texts, 1 profiles, 0 warnings`, `Inserted component block (linked) from block.mitcad` | import |
| `Insert DXF drawing: unit mm, layers Outline (4) \| Holes (1) off`, `Insert DXF place lower_left at x,y`, `Insert DXF x at …`, `Insert DXF layer Holes at …`, `Insert DXF OK at …`, `Insert DXF: lower left corner at (10, 5), layers Outline` | the Insert DXF dialog (places in the main window's coordinates) and what it chose |
| `Import of part.f3d started`, `Import item 3: Extrude2 parametric`, `Import of part.f3d: stop requested after 12 s`, `Import of part.f3d stopping: the remaining items take the file's bodies`, `Import of part.f3d cancelled`, `Import of part.f3d failed: …`, `Imported part.f3d: 2 bodies; items …[; stopped at Extrude3 (item 5)]`, `Import report: part.f3d: 28 items (25 parametric, …), 2 bodies, 2 of the file's 2 solids match[, stopped at Extrude3 (item 5)]`, `Import report summary: <the report's text above its list>`, `Import report kept: Pad.Length = Sheet.w * 2: <why>` | an `.f3d` or FreeCAD import (`stopped after the last item` when only the final comparison was left; `kept`: a FreeCAD expression kept as its value) |
| `Recent files: a.mitcad \| b.step`, `Dropped <paths>`, `Document closed` | the File menu |
| `Remote status: synced` (`ahead N`, `behind N`, `push`, `fetch`, `sync`, `conflict`, `offline`, `sign-in needed`, `remote not found`), `Remote task <command> started\|done\|failed (<class>): <message>`, `Remote push: sent N version(s) to origin/main`, `Remote check: ahead N, behind M`, `Remote notice: <text> [Sync Now, Dismiss]`, `Sync: <case>[ (<class>)], N replayed, sent\|nothing sent`, `Sync conflict: <path> (<kind>): <choice>`, `Open from Remote: <url> into <folder>: <files>` | remote repositories: the status bar's state, a remote task, a push, a check, the notice, a sync and its conflicts, a clone |
| `Computing part.mitcad started`, `Computing part.mitcad: 57 of 196, Extrude6` (when the feature changes, at most 5 a second, after the first 50 ms), `Progress dialog shown: part.mitcad`, `Computing part.mitcad: cancel requested after <ms> ms`, `Computing part.mitcad cancelled after <ms> ms`, `Computing part.mitcad done in <ms> ms (<n> evaluated)`, `Opening part.mitcad cancelled` | a job on the model's worker thread (opening a project, an imported design, New, Close) |
| `Computing set_parameter started` … `Computing set_parameter done in <ms> ms (<n> evaluated)` (a model command's job is named by its `cmd` and logged only when it takes over 50 ms; `Computing preview …` for a panel's preview), `Command set_parameter cancelled`, `Command undo refused: the model is busy computing`, `Preview Extrude: cancelled`, `Extrude: OK cancelled`, `Extrude: OK not done`, `Parameter d1 expression: 70 cancelled`, `Finish Sketch cancelled: Sketch1 stays open`, `Command Extrude: the timeline stays rolled back`, `Drag of p3 ended by a dialog`, `Manipulator Extrude distance dragged to 25 (ended by a dialog)`, `Context menu dropped: the model is computing` | commands and previews as jobs: a cancel, a call that came while a job ran (refused, or waiting until it is done), a drag the progress dialog ended |
| `Autosave every 300 s to <folder>`, `Autosave off`, `Autosaved block.mitcad: 2865 bytes to <folder>/<id>.mitcad`, `Autosave removed for block.mitcad`, `Autosave skipped: busy` (or `previewing`), `Autosave of block.mitcad failed: <reason>` | autosave: the timer set at start and by Preferences, a write, the session's files removed, a write put off (`MITCAD_TEST_AUTOSAVE_SECONDS` for tests; `tools/ui-autosave-test.sh`) |
| `Recovery: none`, `Recovery: 2 document(s) found`, `Recoverable block.mitcad: /path/block.mitcad, autosaved 2026-10-05T03:12:00Z, 2865 bytes` (`never saved`; `, the file has changed since`, `, the file is gone`, `, damaged: <why>`), `Recovery: removed the lock of session <id>`, `Recovery: later, 1 document(s) kept`, `Discarded recovery of block.mitcad`, `Recovered block.mitcad from autosave of 2026-10-05T03:12:00Z`, `Recovered session <id> removed`, `Recovering block.mitcad cancelled`, `Recovery of block.mitcad failed: <reason>`, `Save conflict: <path> changed since it was opened`, `Save conflict: overwrite` (`save as`, `cancelled`) | recovery: what was found, each session, the dialog's choices, the earlier session's files removed once this session wrote the document, and Save over a recovered document's changed file |
| `Result store: restored 4, evaluated 2; stored 0 (0 bytes) in 0.1 ms`, `Result store: <error>`, `Result store: removed 12 old results (310.5 MB), 4800.0 MB kept`, `Result store cleared: 4 files, 0.2 MB`, `Import stored 17 results`, `Preferences: results on disk on, at most 5120 MB; in memory at most 7726 MB` | the result store: an open's job (restored from the store, evaluated, written) and a Save's, a write that failed, the clean-up beyond the size, Preferences' Clear, the `.f3d` import's worker, the Cache settings saved |
| `Diagnostics: memory 307830 of 8101298176 bytes (11 results), disk 11238 bytes (4 files), process 355545088 bytes; last recompute 5 evaluated, 0 from the store, 1 cached`, `Diagnostics: cleared memory: 7 results, 120000 bytes`, `Diagnostics: cleared disk: 4 files, 11238 bytes`, `Diagnostics report written to <path>` | Help › Diagnostics: shown or refreshed, Clear Memory, Clear Disk, Export Report |
| `Version recorded: part.mitcad abc1234 v3: <summary>`, `Version unchanged: part.mitcad (v3)`, `Version not recorded: <why>`, `Version failed: <error>`, `Version warning: <warning>`, `Version status: bracket, main, v3` (`…, no version`; `none` outside projects with history; when it changes), `Version author: Name <email> (git)` (or `(settings)`), `Version author dialog: git's Name <email>` (`the settings' (git has …)`, `git has none`), `Version author dialog cancelled`, `Save Version dialog: <automatic message's summary>`, `Save Version cancelled`, `Save Version: no version history for part.mitcad`, `Save conflict: <path> changed outside Mitcad (the file)` (or `(a newer version)`), `Save conflict: compare: <the diff's lines joined by " \| ">`, `Save conflict: new version` (`save as`, `cancelled`), `Renamed from a.mitcad: its display state moved along` | saving versions: a commit and what it did, the status bar's label, the author, Save Version, a change outside Mitcad before Save, a rename followed at opening |
| `Version History dialog: part.mitcad, reading its versions`, `Version History: 3 versions of part.mitcad: v3 abc1234 <summary> \| v2 … (renamed from a.mitcad)` (at most 30), `Version History changes: v3 <changes> \| v2 … \| v1 first version`, `Version History selected v2 (abc1234): preview` (or `no preview`), `Version History compare v2 with v1: <heading and text joined by " \| ">` (`with the open design`), `Version History geometry v2 with v1: computing`, `…: <text>`, `…: cancelled`, `Version History open v2 (abc1234)`, `Version History restore v2 (abc1234)`, `Version History save copy v2 (abc1234)`, `Version History closed`, `Version History failed: <error>`, `Version History: no version history for part.mitcad`, `Restore dialog: v2 (abc1234) of part.mitcad` (` (unsaved changes)`), `Restore cancelled`, `Restore: the unsaved changes dropped`, `Version restored: part.mitcad v2 (abc1234) as v4 (def5678)`, `Opened version v2 (abc1234) of part.mitcad as part v2`, `Save Copy As dialog: part v2.mitcad`, `Save Copy As cancelled`, `Saved a copy of part.mitcad v2 (abc1234) as <path>`, `Version preview saved: <blob's 7 digits> (256 x 160)` | Version History: the list, the changes, the selection and its comparison, the geometry, the choice; Restore, Open and Save Copy As; a version's preview |
| `Sync 12` | with `MITCAD_TEST_SYNC=1` (the Linux UI tests): the answer to the Pause key (`ui_sync`), once the input before it is handled, at least 20 ms after it and when no watched timer runs (`framework/TestSync.hpp`) |
| `New Project dialog: <folder offered>`, `New Project cancelled`, `New project inside the git repository <root>`, `Version history started in <root>: version abc1234 on main`, `New project <folder>`, `Start Version History dialog: <folder>` (` (a repository)`, ` (inside <root>)`; `: the project <root>`), `Start Version History: part.mitcad has version history already`, `Start Version History cancelled`, `Moved <path> to <path>`, `Preferences: versions by git's user, else Name <email>` (or `Name <email>`, `nobody`) | New Project, Start Version History, Preferences' author |
| `Updates: Mitcad 0.0.0 for linux-x64, the AppImage <path>` (`installed in <folder>`, `announce only: <why>`), `Automatic update checks are off`, `Update checks are turned off by the administrator`, `Update check: the last was at <time>`, `Update check: manifest at <url>` (`releases at <url>`), `Update request redirected to https://<host>`, `Update manifest: Mitcad 0.1.0 of 2026-10-06 (available), 214 bytes for linux-x64`, `Update check: Mitcad 0.0.1 is up to date`, `Update notice (offer): <text> [Release Notes, Install, Skip This Version, Later]` (`(progress)`, `(message)`, `(error)`), `Update notice closed`, `Update: skipping Mitcad 0.1.0`, `Update download: <url> (<n> bytes) to <path>`, `Update verified: <path>`, `Update rejected: <reason> (deleted <path>)`, `Update staged: 0.0.0 -> 0.1.0, <program> when Mitcad has quit`, `Update: replaced <AppImage>`, `Update: started <program> (process <pid>)`, `Update installed: 0.0.0 -> 0.1.0` (`Update not installed: …`), `Update failed: <reason>`, `Preferences: update checks on, channel stable` | automatic updates ([docs/updates.md](../docs/updates.md); `tools/ui-update-test.sh`) |

Test logs are switched on by environment variables
(`framework/Diagnostics.hpp`): `MITCAD_LOG_PICKS`, `MITCAD_LOG_VOLUMES`
(the UI test libraries set both), `MITCAD_LOG_TIMING`,
`MITCAD_LOG_ORIENTATION_CUBE`; `MITCAD_TEST_SYNC` answers the sync key.
`MITCAD_SETTINGS_DIR` keeps the settings in an INI file of a test's own
(on Windows they are otherwise in the registry).

**A new delayed log line** that tests read must come from a watched timer:
`TestSync::watch(timer)`, or `TestSync::singleShot(ms, context, function)`
instead of `QTimer::singleShot`, so that `Sync <n>` waits for it (how the
sync works: [docs/development.md](../docs/development.md)).

### Test helpers

`tools/ui-test-lib.sh`:

- `ui_command "Name"` runs a command through the search.
- `ui_view_click x% y%`, `ui_view_drag`.
- `ui_click_logged "Panel Extrude input operation"`,
  `ui_double_click_logged`, `ui_drag_logged`.
- `ui_menu_choose "Entry"` (in the context menu just opened).
- `ui_create_sketch xy` (Create Sketch and the plane);
  `ui_sketch_click x y`, `ui_sketch_drag x1 y1 x2 y2`, `ui_sketch_at x y`
  (sketch millimetres).
- `ui_focus_dialog "title"`, `ui_focus_main` (Xvfb has no window
  manager).
- `ui_mark` and `ui_expect_new "text" [seconds]` (only lines logged since
  the mark).
- `ui_click_pick "face F2.b0/F2:end"` (clicks where the last `Pick
  places:` puts the first item whose line starts so), `ui_pick_at`
  (prints the place).
- `ui_drag_from "Manipulator Extrude distance" dx dy`.
- `ui_type_in "Panel Hole input diameter" "8"` (selects the field's text
  and types; `--` lets "-25" through).
- `ui_choose "Panel Extrude input operation" 2` (the entry two below the
  current one).
- `ui_expect_volume "sed pattern with a group" volume "description"
  [tolerance]` (waits for the line, relative tolerance).
- `ui_step`, `ui_sync [window]`, `ui_wait_idle`: wait until the app has
  handled the input sent so far, or until no job runs.
- `ui_start_app` (Windows `Ui-StartApp`) starts with `--no-recovery`;
  `UI_RECOVERY=1` (`-Recovery`) lets it ask.

Tests run with their own settings (`XDG_CONFIG_HOME`), data
(`XDG_DATA_HOME`, so the recovery folder is theirs), and git
configuration (`GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM`), so no version
is recorded with the user's identity; a test that records versions sets
its author (`tools/ui-version-test.sh` also sets `HOME`). On Windows:
`MITCAD_SETTINGS_DIR`, `MITCAD_AUTOSAVE_DIR`.

Other tools: test models can be made with `mitcad-cli run script.json
--save model.mitcad` (`tools/ui-browser-test.sh`);
`tools/ui-image-stats.py` counts pixels of a region of an `xwd` dump
(objects, dark, grey lines, red, the section caps' yellow, changes against
another dump) for `tools/ui-view-test.sh`; `tools/ui-import-test.sh`
drives the File menu by accelerators (Alt+F, then E Export, I Import, R
Open Recent, M Insert Component) and imports a corpus design when
`MITCAD_F3D_CORPUS` is set.

**Windows** (the build VM): `tools/ui-windows-lib.ps1` has the same
helpers in PowerShell (`Ui-Key "ctrl+z"`, `Ui-Type`, `Ui-ViewClick`,
`Ui-SketchClick`, `Ui-ClickLogged`, `Ui-ClickPick`, `Ui-TypeIn`,
`Ui-FocusDialog`, `Ui-ExpectNew`, `Ui-ExpectVolume`, ...). They post mouse
and keyboard messages to the window, read the same log lines, and give
commands without a default shortcut one in the test's settings
(`Ui-Init ... @{'sketch.create' = 'F7'}`), since popups close at once in
session 0. `tools/ui-windows-workflow-test.ps1` (ctest
`app.ui-windows-workflow`, with `MITCAD_MESA_DIR`) is the workflow test.
