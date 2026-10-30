# Commands in the MitCad application

How to add or change UI commands, sketch tools and panels, the UI
framework's conventions, and the log lines UI tests read. A feature
command is a declarative definition plus a little glue between its inputs
and the model's feature definition (`core/model-api/src/commands.md`);
the framework does the rest. Where the code lives: [Files](#files).

## A feature command

```cpp
CommandDef chamfer(const CommandContext& context) {
  CommandDef def;
  def.id = "solid.chamfer";            // unique, also the settings key of its shortcut
  def.name = QObject::tr("Chamfer");
  def.icon = "chamfer";                // app/icons/chamfer.svg
  def.tooltip = QObject::tr("Bevels edges"); // one short line, at most 80 characters
  def.shortcut = QKeySequence();       // none by default
  def.tab = "SOLID";                   // SOLID, BUILD or SKETCH; empty: menus only
  def.group = "MODIFY";                // see Ribbon::groups()
  def.pinned = true;                   // a button besides the group menu entry
  def.subgroup = "";                   // a submenu of the group's menu, see below
  def.section = "";                    // a section of the group's menu, see below
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

**Subgroups** (mitcad#222): commands of a group with the same `subgroup`
are one submenu of the group's menu, where the first of them is, in
registration order (`Ribbon::createGroup`). The subgroup's pinned command
is the default of a split button (`QToolButton::MenuButtonPopup`; in the
Mac presentation `RibbonButton` draws the arrow part): the button runs it,
its arrow opens the submenu. A subgroup without a pinned command has no
button; a subgroup of one command is a plain entry. The sketch commands
give it with the group, `"CREATE/Circles"` (`sketchCommand` in
`SketchCommands.cpp`). The native macOS frontend mirrors the submenus as
popup entries of its group menus; the default keeps its plain button there.

**Sections** (mitcad#579): `section` is a short id of a part of the group's
menu (`"profiles"`). `Ribbon::createGroup` puts a separator between two
consecutive entries of the menu whose sections differ; a submenu is an
entry of its first command's section. A separator is never at the top or
the bottom of the menu, and never two are in a row. In a group whose
commands have no section, the menu has no separators between them (the
separator before a group's extras, `addGroupExtras`, stays). The order is
still the registration order: register a section's commands together. The
native macOS frontend's group menus have the same separators
(`NativeMacAction::section`). The command search and the view's context
menu do not use sections or subgroups.

The SOLID CREATE menu's sections, in this order (`CreateCommands.cpp`,
then Gear, Pulley, Bearing, Gear Pair and Belt Drive from
`MainWindow::registerCommands`):

| Section | Commands |
|---|---|
| `sketch` | Create Sketch |
| `profiles` | Extrude, Revolve, Sweep, Loft, Rib, Web, Emboss |
| `faces` | Hole, Thread, Plastic Feature |
| `primitives` | Box, Cylinder, Sphere, Torus, Coil, Helix, Pipe |
| `patterns` | the Pattern submenu (`subgroup` `"Pattern"`: Rectangular Pattern, Circular Pattern, Pattern on Path, Pattern at Points), then Mirror |
| `machine` | the Machine Elements submenu (`subgroup` `"Machine Elements"`: Gear, Pulley, Bearing, Gear Pair, Belt Drive) |

A new machine element sets `def.group = "CREATE"`, `def.section =
"machine"` and `def.subgroup = "Machine Elements"`, and is registered
after Belt Drive. None of these commands is pinned, so the submenus have no
split buttons: the group's buttons are Create Sketch, Extrude, Revolve and
Hole (`tools/ui-sketch-create-menu-test.sh`,
`tools/ui-windows-pattern-options-test.ps1`).

Checklist for a new command or feature type:

- An icon in `app/icons` (see [Icons](#icons)); a missing file logs
  `Command <id> has no icon '<name>'`.
- A new feature type also needs an icon in `featureIcon`
  (`browser/DocumentSnapshot.cpp`; the generic `feature` icon otherwise).
- A command that changes the model outside a panel goes through
  `DocumentHost::runModelCommand` (refresh, volume log) and checks
  `canChangeModel`.
- Picks may carry an occurrence: a command whose definition refers to
  bodies of another component must link them or refuse them (the model
  fails such features without a link). Combine sends its tools in their
  occurrences; Extrude (To Object on each side, Start: Object) and Revolve
  (To Object) send an object picked in another occurrence than the
  profiles with its body's link (`object_links`, `cmd::addObjectLink`,
  mitcad#762), and their `load` puts the picks back in the occurrences the
  links name (`cmd::linkObject`) and keeps the links' target, where they
  saw the profiles (`cmd::keepLinksTarget`): the session puts a profile in
  the first occurrence of its sketch's component (canonical picks), so
  `build` takes the kept target while it places that component
  (`cmd::linksOccurrence`), and an edit of a feature whose profiles were
  picked in a second copy keeps its link. An object picked in two
  occurrences of one body is refused ("Pick both objects in the same
  occurrence of frame."). Hole refuses another component's object in its
  panel. A construction plane of another component than the feature's
  (its profiles' sketch's, Hole's own) is refused in the panel by all
  three (`cmd::foreignPlane`, `cmd::planeComponent`, mitcad#772): the
  links carry bodies only, and the model reads a construction plane only
  in its own component ("Plane1 is in frame: pick a plane of crank, or a
  face of frame.").
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
- `withEnabled(test)` (mitcad#655): the row is shown but disabled while
  `test(state)` does not hold (3D Print's Printer, Process and Materials
  with STL), logged `Panel <command> input <key> enabled|disabled` when
  it changes; frontends without it show the row enabled.
- `withLabel(text)`: the row's label follows the state (`labelIn(state)`,
  `label` without it; mitcad#545: a pattern's length row says Spacing or
  Extent by its Distance Type). The Qt panel changes it as the state
  changes and logs it (`Panel Rectangular Pattern label distance1:
  Extent`), the value lines use it; the AppKit inspector changes its
  caption too (mitcad#815).
- `withHeading(text)`: a bold heading row above the input, shown and
  hidden with it: the first input of a group (Thread's and Hole's Extra
  clearance: Radial clearance and Flank clearance, shown for modelled
  threads only).
- `withAccepts(test)`: narrows a selection beyond its kinds (only straight
  edges and sketch lines: `item.geometry == "line"`); the view then
  highlights and picks only what the input takes.
- `withoutMeshes()` (mitcad#399): a selection that hands bodies or faces to
  a surface operation takes no mesh body and no face of one (triangles
  without surfaces; `item.geometry == "mesh"`: the view sets it on mesh
  bodies and their faces, the browser on bodies the `bodies` query marks
  `mesh`). The view neither highlights nor picks them; one picked in the
  browser is not taken, logged `<command> <label>: body F3.b0 not taken: a
  mesh body` with a hint in the status bar. Combine's Target Body and Tool
  Bodies, the Objects of the operation inputs, an extent's Object,
  Hole's Objects to Cut, Shell's Faces/Body, Draft's, Offset Face's,
  Replace Face's source, Split Face's and Delete Face's Faces, Split
  Body's Bodies and splitting tool, Pattern's Faces and Sweep's Guide
  Surface use it; the model refuses mesh bodies in those features anyway
  (commands.md, "Mesh bodies in surface operations").
- `withColor(color)`: a selection's highlight colour; otherwise blue,
  orange, green, purple in input order.
- `withOrdered()`: the picks' order matters (loft sections); the panel
  lists them with up, down and remove buttons.
- `withChoices(options)`: a choice's options follow other inputs
  (`options(state)`, e.g. thread sizes follow the standard); the
  drop-down is refilled when they change, and a value they lack stays as
  an extra entry. `withOnChange(call)`: called after the user changed a
  choice, to set dependent inputs (a new size takes its coarse
  designation); shown values it leaves NaN (`state.setValue(id, NaN)`)
  are evaluated again (Drive Joint's value of another motion). A value's
  or a text's `onChange` runs when it is typed (a value's also when its
  handle drags it): BUILD's typed length detaches the auto length, its
  search narrows the picker.
- `withTabCycle(values)`: a choice that Tab steps through these values
  while the main window has the keyboard and the choice is shown (BUILD's
  auto length: face, centre, through); the session filters the
  application's key presses while such a command runs (Shift+Tab still
  moves between the fields), logged `<command>: Tab steps <label>`.
- `withTabPicks(items)` (mitcad#857): a selection of one item that Tab,
  while it is the active input and the main window has the keyboard,
  sets to the next of `items(state)` (after the one picked; the first when
  none is), drawn in the input's colour, so that Enter takes it (3D
  Print's print bed faces to choose from, each shown in turn); logged
  `<command>: Tab steps <label>: <item>`.
- `withRefusedHint(hint)` (mitcad#857): a click on an item of the
  selection's kinds that it does not take (the view's pick refused it:
  `OcctViewer::takeRefusedClick`) shows `hint(item, state)` in the status
  bar, logged `<command> <label>: <item> not taken: <text>` (an empty
  text: nothing). With `MITCAD_LOG_PICKS` the input's activation logs
  where such clicks hit, after the host's pick places: `Pick refused face
  F1.b0/F1:side0 in O1 at x,y`.
- `withItemsText(text)`: the picks by name instead of their count
  (`InputDef::itemsText`; 3D Print's Bed face: `Tall: print bed face`),
  logged `<command> <label> listed: <text>`.
- `withChoiceImages(image)`: a choice whose options (from `withChoices`)
  each have a picture beside the label, `image(value)` (a null image for
  none; 48 px in the drop-down). Logged `Panel <command> option pictures
  <key>: <n> of <m>` with the options. BUILD's connectors show the
  library's preview images.
- `withCursorBox()`: a value input also as a box at the cursor in the
  view, as a sketch tool's value boxes are (mitcad#375: BUILD's length).
  It follows the mouse over the view once it has moved there, takes no
  mouse input (a move or a click on it reaches the view, mitcad#740), and is
  shown while the input is; its text is the input's, or the shown
  inspection's `report.cursor.<key>` (the auto length) while that gives
  one. Typing in the box, or a number typed while the view has the
  keyboard (it goes into the box), is the input's text as typing in the
  panel's field is, and the field shows it; Enter confirms, Esc closes.
  With several such inputs (BUILD's length and tilt, mitcad#444) the box
  is the first shown one's, or the one whose handle was dragged last.
  Not in the AppKit frontend. Logged `Cursor box <command> <key>: <text>
  at x,y` when its text changes. the form a pick is kept in (Pattern's Features
  keeps the feature that made a face; Move/Copy's Components the
  occurrence that places a body).
- `withPickRule(kinds, test)` (mitcad#394): what a selection takes
  follows the other inputs: `kinds(state)` (within its filter) and
  `test(item, state)`. The view's hover and clicks use them, the origin's
  planes show only while the kinds have planes, and the active input
  takes them again after each change of the inputs (logged `<command>
  <label> takes now: faces`). BUILD's placement: planes and faces in a
  ground placement, a profile's faces otherwise. `withHiddenConstruction()`:
  while the input is active and takes no construction geometry, the view
  hides the construction planes, axes and points (not their visibility in
  the design; `CommandHost::setConstructionHidden`, logged `Construction
  geometry hidden for the command` and `... shown again`).
- `withChoiceIcons({value: icon})`: a choice shown as a row of icon
  buttons, the chosen one pressed (BUILD's X / Y / Z), logged `Panel
  <command> input <key>=<value> at x,y` for each. AppKit uses native toggle
  buttons with the same artwork, option tooltips and selected value.
  `withDisclosure()`: a check shown as a small
  `▸ More` button that opens the inputs whose `withVisible` follows it
  (a native disclosure triangle and label in the AppKit frontend). Hidden
  rows leave the native stack layout; the inspector keeps its height cap
  and scrolls when the expanded form exceeds the available space.
- `withButton(text, tooltip, call)` (mitcad#418): a choice with a button
  beside its drop-down; a click calls `call(state, context)` once the
  model is idle (the presenter's `buttonClicked`, logged `<command>:
  <label> button`; the button's place `Panel <command> button_<key> at
  x,y`). It changes no input itself: what it opens chooses later through
  `DocumentHost::chooseInCommand(command, key, value)` (the main window's
  `CommandSession::choose`, as the drop-down chooses: `onChange` runs and
  the preview follows; false when that command does not run). BUILD's
  Part and Connector open the part picker so. The AppKit inspector shows
  the button beside its drop-down (mitcad#815).
- `withChoiceTile(tile)` (mitcad#427): a choice with a button
  (`withButton`) shown as a tile instead: `tile(state)` gives a
  `ChoiceTile` (the chosen option's picture, its name and a smaller line
  under it, BUILD: the supplier and part number), logged `Panel <command>
  tile <key>: <name> (<line>)` when it changes; the row's place is `Panel
  <command> input <key> at x,y`. A click on it, or Enter or Space while it
  has the keyboard (it takes it by Tab only, so that Enter after a click
  still confirms the command), calls the button's `call`. Frontends
  without tiles (AppKit, the web) show the drop-down and its button.
- `withChoiceColumns(headings)` (mitcad#656): a choice shown as rows of a
  list under these column headings instead of a drop-down (Gear Pair's
  and Belt Drive's candidates), the chosen row selected; each option's
  label holds its columns separated by tabs. A click on a row chooses it
  (logged `<command>: <label> = <the row's text>`). The options are
  logged `Panel <command> options <key>: <row> | <row> ...` (columns
  joined by commas) when they change, each row shown `Panel <command>
  input <key>=<value> at x,y`. Frontends without such a list (AppKit)
  show the drop-down. The list shows six rows and its headings, its
  columns as wide as their cells and itself as wide as its columns
  (nothing clipped, so the cells are kept short); a column empty in
  every row is left out (Gear Pair's and Belt Drive's Ratio when every
  row meets the ratio), and a cell led by a warning sign (U+26A0) shows the warning
  icon before its text (an undercut gear's teeth).
- `withChoiceIcon(icon)` (mitcad#655): a drop-down whose options each have
  a small icon, `icon(value)` gives its name in `app/icons` and the
  option's tooltip (`ChoiceIcon`). Such a drop-down keeps the panel's width and elides
  a long name in the middle only when it must (`MiddleElideComboBox` in
  `framework/CommandPanel.cpp`; its list shows the full names).
- `noteInput(id, note, action, label)` (mitcad#655, `framework/NoteRow.*`): a
  note row, no value: `note(state)` gives an `InputNote`, a short text
  (wrapped, never clipped) and its buttons under it by their texts; an
  action with an icon in `InputNote::icons` is a small icon button on the
  text's line instead, its text the tooltip (3D Print's Materials row and
  its pencil); a link (`linkActionIcon()`) has a row of its own under
  the buttons, the note's width, wrapped at its words, never cut;
  `InputNote::oneLine` keeps the text on one line, elided in the middle,
  its whole text the tooltip; a row with neither text nor actions is
  hidden. Without a label it takes the panel's whole
  width. A click calls `action(index, state, context)` once the model is
  idle (the presenter's `noteAction`, logged `<command>: <key> action
  <index>`), then the panel shows the inputs again after
  `CommandDef::modelChanged` (the inputs that show the model follow it;
  `CommandSession::refreshInputs`, which a window beside the panel also
  calls through `DocumentHost::commandInputsChanged`). Logged `Panel
  <command> note <key>: <text> [<button>] ...` when it changes, the
  buttons' places `Panel <command> action <key>.<index> at x,y`. 3D
  Print's Materials and Own settings rows and Import from slicer… with
  its line under it; the AppKit form shows the text with its icon buttons
  and links, and its text buttons under it (mitcad#815).
- `withButtonIcon(icon)` (mitcad#655): the button of `withButton` as a small
  icon (`app/icons`) with its tooltip, the text its accessible name: 3D
  Print's Edit.
- `withButtonEnabled(test)` (mitcad#655): the button of `withButton` enabled
  only while `test(state)` holds (3D Print's Edit, disabled on Slicer
  default).
- `withFieldColumn()` (mitcad#655): a note row without a label in the field
  column, under the fields above it, rather than across the panel (3D
  Print's Import from slicer…).
- A choice whose value starts with `choiceHeadingPrefix()` (`__heading/`)
  is a heading of the options after it (mitcad#655): the Qt panel shows it
  small, bold and not choosable; a frontend without headings lists it,
  and choosing it changes nothing (none uses it since 3D Print's boxes
  show no places).
- A choice whose value starts with `choiceSeparatorPrefix()`
  (`__separator/`, `isChoiceSeparator`) is a separator line between the
  options (mitcad#828), its label empty: the Qt panel inserts a disabled
  separator (not choosable, skipped by the arrow keys), the AppKit form a
  separator item, the log lists it as `---`, and a script that chooses it
  is refused. 3D Print's Printer and Process boxes set New printer… /
  New process… apart from the profiles with one.
- `Inspection::viewHint`: a line over the top of the view while the
  command waits for something there (BUILD's first part: `Click a plane
  or a face, or press Enter`), logged `View hint <command>: <text>`.
- `withOnPick(call, keep)`: called after a pick, with `item.at` where the
  click hit, in its component's coordinates. With `keep`, picking an item
  already selected keeps it and still calls back (Place On places again
  where the same face is clicked again, [Placement by picked
  geometry](#notes-on-particular-commands)).
- `withPlaces()` (mitcad#656): a selection input whose picks only say
  where a creation command's part goes (Place On, Gear Pair's and Belt
  Drive's axes): the view's context menu offers the command under More
  for them, as for a face or a plane ([The view's context
  menu](#the-views-context-menu)).
- `withManipulator(handle)`: a value input with a handle in the view.
  `handle(state, context)` returns a `Manipulator` (or none): an arrow
  from `origin` along `direction`, knob at `origin + (value - base) *
  factor` (a diameter: factor 0.5), or a ring about `direction` with
  angle 0 towards `reference` (`cmd::arrow`, `cmd::ring`). Dragging sets
  the value rounded to a zoom-dependent step (whole degrees for rings) in
  the document's unit; a click without a drag picks what lies under it.
  An arrow's `minimum` is the smallest value a drag gives: the handle
  stops there (mitcad#693). Extrude's Two Sides arrows have none: a drag
  across the start gives a negative distance, which ends on the other
  side's side (mitcad#765; the panel sends the extent with `signed`,
  but keeps an edited feature saved without it, whose negative
  distance counts by its size, as it is). With `valueBox` (mitcad#656) the handle shows its value in a box: an
  arrow is drawn as a dimension, a solid line from its origin (the
  reference it is measured from, a short bar across it there) with the
  box at its middle (a short dimension's text on its line beyond the
  reference, away from the point), a ring with its zero line along
  `reference`, an arc to the knob and the box beyond the knob. A layout
  pass then pushes the boxes off each other and out of the panels
  floating over the view (`CommandHost::coveredRects`; the boxes count as
  covered for the view's picks); a box the pass moved gets a thin leader
  line back to where it belongs. With `boxAtTip` an arrow's box sits
  beyond its tip, as a size reads (a box's length, width and height, a
  coil's height, a hole's depth), not on its line as a distance from a
  reference. With `boxAtZero = false` a box of a
  value that is 0 shows only while its handle or the box is under the
  mouse or dragged (a placement's Offset and Angle). A click on the box
  opens a field over the view at it (`CommandSession::openValueBox`, not
  in the AppKit frontend), styled as the boxes are, that edits the value
  as the panel's field does: what is typed is the input's text (a bare
  number gets the document's unit, `15` becomes `15 mm` in the panel, an
  angle `30°`; a ring's drag writes its angle so too, and the
  placement's and the gears' angles start so, `0°`, `20°`),
  Enter or a click elsewhere keeps it, Esc puts the value back, Tab and
  Shift+Tab keep it and open the next or previous box; none of them
  reaches the window's OK or Cancel. Logged `Value box <command> <key> at
  x,y: <text>` with the handles' places (a hidden box is not logged),
  `Value box <command> <key> clicked`, `... opened: <text> at x,y` and
  `... closed: <text>`.
- `withHandleWhenHidden()` (mitcad#656): a value input whose handle stays
  in the view while its row is hidden (a placement's Offset and Angle
  folded under More).
- `withBesidePrevious()` (mitcad#656): the input on the line of the one
  before it, after its field and without a label of its own (a
  reference's distance beside the edge it is measured from, Flip beside
  Offset, the hole's Flip beside Placement), hidden and shown on its own
  but inside the row before it. Frontends without such lines (AppKit)
  show it as a row of its own.
- `withRowNote(note)` (mitcad#656): grey text under the row,
  `note(state, context)` read after each preview that succeeds, one line
  per `\n`, or rich text (Pulley's Check Belt: its verdict in bold, belt
  length, nearest belt, travel); logged `Panel <command> <key> note:
  <lines joined by "; ", without markup>` when it changes. Shown through
  the presenter's `setValueHint`.
- `withAuto(text)` (mitcad#656, `InputDef::autoText`): a value that may be
  left empty, which the definition then leaves out and the model works
  out ("0 means automatic" made explicit). The empty field's placeholder
  is `Auto`, then `Auto (<text(state, context)>)` after each preview that
  succeeds, the value the model worked out (the presenter's
  `setPlaceholder`, logged `Panel <command> placeholder <key>: Auto
  (24.4 mm)`); evaluated it is NaN with no error, logged `<command>
  <label>:  = auto`. A definition's slot that is missing or 0 loads as
  empty (`zeroOrNone`).
- A bare number typed in a value field gets its unit when the field is
  left or Enter is pressed (`15` becomes `15 mm`, mitcad#656), as a value
  box does.
  A field the keyboard is not in shows a long expression from its start
  (`shaft_bore_clearance`, not its end): when its text is set, and again
  when it loses the keyboard. A name picked from the view still goes in
  where the cursor was as the field lost the keyboard. Not when the
  field's own context menu or a drop-down's popup takes the keyboard
  (`Qt::PopupFocusReason`, `leaveValueField`, mitcad#756): the caret and
  the selection stay for the menu's Copy, Cut and Insert Parameter….
- `InputDef::emptyShows` (mitcad#656): what an empty selection row's
  button says instead of `None`, with its swatch's colour then (Align on
  a face: `Reference 1` in Reference 1's green).
- The first pick of a placement input (`withPlaces`) whose preview is
  drawn moves the view sideways, as little as it takes, so that the part
  is clear of the panels floating over the view
  (`OcctViewer::bringClearOfPanels`, logged `View moved dx,dy px: the
  part clear of the panels`); the pick places are logged again once the
  view rests.
- `CommandDef::viewLines` (mitcad#656): polylines in model coordinates the
  manipulator overlay draws while the command runs (Pulley's belt pitch
  line round it and the Check Belt pulley); logged `Lines <command>: n`
  when their number changes.
- `CommandDef::extendedToolPreview` (mitcad#712: Split Face, Split Body,
  Trim): the preview's tool (`preview_tool_shape`) is the splitting or
  trimming tool as the feature extends it, which the model makes for
  previews only (never in a recompute or an import) while
  Extend Splitting Tool (Extend Trimming Tool) is checked and the tool is
  a plane or a face: the faces the split cuts with, clipped to the bodies'
  box grown by 5 % of its diagonal. The view draws it see-through orange
  with its outline (`OcctViewer::PreviewStyle::ExtendedTool`, the tool
  input's colour) over the pieces; with the option off there is none and
  only the picked tool's highlight shows. Logged `Preview extended tool:
  <box>` (or `none`).
- `withToggles(made)`: a text input with dots in the view;
  `made(state, context, preview)` builds them from the preview's report
  (`Manipulator::Kind::Toggle` with `element` and `on`); a click on a dot
  adds or removes its element's number in the text ("2, 5"). Used by the
  patterns' Suppressed input.
- `showsOrigin = false`: the input takes construction geometry without
  showing the origin's planes and axes (Measure).
- `withGhosts(named)`: a selection whose picks let later picks through
  (mitcad#147). `named(item, state, context)` gives the occurrences (paths
  from the root, `O1/O4`) to draw as ghosts for an item of the input:
  their bodies, and those of the occurrences inside them, see-through in
  their own colour and not pickable (`BodyDisplay::Style::Ghost`,
  `OcctViewer::setGhosts`), and the highlights on them let clicks through
  too. The session asks every shown input's items whenever the selections
  or the active input change, and leaves out the active input's own (its
  next pick may lie on them: re-picking shows them normally); deselecting
  the item, OK and Esc take the ghosts away. A list row's input gets its
  row's state. Joint and As-Built Joint ghost side A's occurrence until B
  is picked (`ghostUntilPicked`); the same hook serves other commands that
  pick one component and then another behind it (Rigid Group, assembly
  constraints, mitcad#148). The ghosts of components that are not active
  (mitcad#420, "Inactive components as ghosts" under The view) look the
  same but stay pickable; a body ghosted by both is the command's ghost.
- `withToolOverlays()`: a selection of bodies the command consumes
  (Combine's Tool Bodies, mitcad#561). A consumed body leaves its
  component, so the preview has it in none of the component's
  occurrences; while the input is active, the session draws each
  occurrence of a picked body that the preview removes, where it is not
  picked itself, as a tool overlay (`BodyDisplay::Style::Tool`, `CutTool`
  for a cut: see-through like the tool preview, blue or red, and
  pickable), so that a click picks the body there too. The session finds
  them with `removedShownBodies` (`framework/PreviewBodies.hpp`) and
  draws the shown preview again when the input is activated or left;
  `Preview bodies:` lists the overlays with ` tool`
  (`F2.b0 in O2 at 30,0,0 tool`).
- `withHoverChain(kinds, check)`: while the check input `check` (a
  top-level one) is on, the hover over an edge or face of a body of
  `kinds` shows its whole tangent chain, what the operation takes with
  the pick (mitcad#290): Fillet's and Chamfer's edges, the faces of
  Shell, Draft and Replace Face, all with `tangent_chain`. Picks stay the
  item clicked. The session asks the model's `tangent_chains` query once
  per body and kind in a command run (logged as `Tangent chains of F2.b0
  (edges): 2 in 3 ms`, `(slow)` over a second; a failure is a warning and
  leaves the hover on the item) and looks each hover up; the view draws
  the rest of the chain in the hover's colour (`OcctViewer::setHoverChain`,
  `framework/CommandSessionHover.cpp`). The log line `Hover chain: 3
  edge(s) of F2.b0` (`face(s)`) gives how many the hover shows
  (`tools/ui-hover-chain-test.sh`).

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
- Counts, factors and fractions (pattern quantities, scale factors, a
  construction path's fraction, teeth, ribs) are unitless slots in the
  model (mitcad#662): `ValueKind::Unitless` inputs that take the
  expression (`state.expression`) and come back with `cmd::loadCount`.

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
  by sketch coordinates, a partial thread, a standard size's thread with
  starts) is not offered for editing, so OK cannot drop the rest.
- Fillet's and Chamfer's All Edges (mitcad#793): a check in each set that
  hides the set's Edges and shows a Body input in their place; the set is
  saved as `{"all_edges": true}` (every edge of the body when the feature
  is computed, no edge list), its own set only (a second set is refused),
  and the fillet keeps the general `sets` form. `load` turns All Edges on
  with the feature's body for such a set, so editing a fillet of all
  edges (the FreeCAD import's "use all edges") keeps it whole-body
  instead of an empty or frozen edge list; `describe` says `Added fillet
  on all edges` (`tools/ui-modify-test.sh`).
- `editsAlso`: other definition types the command edits (Move/Copy edits
  `move` and `move_occurrence`; `build` gives the type).
- `init(state, context)`: defaults that depend on the model, applied after
  the inputs' defaults and the pre-selection (Extrude and Revolve take the
  newest unused profile).
- `ended(committed)`: called once when the command ends (OK, Cancel,
  Close, discarded), before the main window's `finished`: what the
  command opened beside its panel goes with it (BUILD's part picker,
  mitcad#418).
- `preselect(selection, state, context)`: fills the inputs from the
  selection made before the command itself, instead of each item going to
  the first input that takes it (3D Print: a face stands for its body, an
  occurrence for the bodies shown in it, nothing selected for every shown
  body); `init` follows.
- `preselected(selection, state, context)`: called with the selection
  made before the command once its items went to the inputs that take
  them (without `preselect`), before `init` (Extrude: a selected face that
  is not planar becomes the profile all the same, so that the panel
  refuses it and no unused profile stands in for it, mitcad#535; Thread:
  the first cylindrical face suggests the size and class, as a face
  picked in the open panel does through `onPick`, mitcad#691). A command
  whose `onPick` sets other inputs from the first pick needs it: items
  selected before the command reach the inputs without `onPick`.
- `rolledBack(state, context)`: editing, called once the timeline is
  rolled back to the edited feature (at once, or after `startAsync`'s
  deferred rollback). `load` runs before that, on the model after the
  feature, so items that depend on the model are resolved again here
  (Extrude: its profile face, which its own extrusion joined away after
  it, gets its kind where the feature finds it, mitcad#535).
- `describe(state, context)`: a log line after OK (UI tests read it);
  with `describeBefore` it reads the model as before OK (the edges a
  fillet rounds), else after.
- `marks(state)` (mitcad#857): items drawn in a colour of their own while
  the panel is open, besides the inputs' picks (3D Print's print bed
  faces to choose from, in the bed face's green), asked whenever the
  highlights are drawn again; a click on one picks it as on the item.
  Logged when they change: `3D Print marks: face F1.b0/F1:top in O1,
  face F2.b0/F2:side0 in O1` (`none` once gone). While there are any,
  only the active input's picks are drawn beside them, so that they stand
  out (mitcad#857 review: green over the bodies' blue came out teal);
  logged `3D Print picks not drawn beside the marks: Bodies` (`none`
  again after).
- `escape(state, context)` (mitcad#857): Esc while the panel is open
  (`MainWindow::cancel` asks `CommandSession::escape` first); true when
  the command took it, and the inputs then follow as after a change (3D
  Print's bed face choice: Esc ends the question, its rows hidden, the
  marks gone, nothing sent, the panel open), else Esc cancels the
  command as always.
- `CommandSession::ask(key, message)` (mitcad#857): an application
  command asks for a pick before it goes on, from its `accept`: the panel
  shows its inputs again, the selection input `key` (shown now) is
  emptied and becomes the active one, the status bar says `message`
  (logged `Panel 3D Print asks: …`; the panel's grey message line is
  cleared, the command's own rows say what is asked), OK is disabled
  until the pick (a required input shown empty: Enter says `Select …`
  and logs `3D Print: OK refused`) and the highlights with the marks are
  drawn again.
- `enabled()`: whether the command can start.

Three other kinds of panels:

- `modelCommand`: `build` gives `{"commands": [...], "label": "..."}`, run
  on OK as one undo step through `CommandHost::runCommands` (Appearance,
  New Component); no preview, outside the timeline.
- `accept(state, context)` (mitcad#234): an application command with a
  panel (3D Print). The inputs are picked and checked as a feature's are,
  and the view highlights the selections; `build` gives an empty
  definition or a `Built::failure` the panel shows; nothing is previewed.
  OK calls `accept`, which does the work outside the model's history
  (files, other programs) and returns false to keep the panel open; then
  `Done <command>` is logged. `okLabel` names the OK button (Send).
  In every panel OK and Cancel stay in view under the inputs, which
  scroll above them (mitcad#655: `CommandPanel::takeButtons`, laid out
  by the main window's command page; a long 3D Print panel in the
  Floating card).- `inspect(state, context)` instead of `build`: an `Inspection` (rich
  text, a log line, a shape drawn over the view, red for a problem, a
  section plane) computed while inputs change; OK closes the panel
  (Measure, Interference, Section Analysis). With `keepsSection` the
  section stays after OK; with `build` too, OK keeps it in the document
  as an analysis (mitcad#41): `add_analysis` with `build`'s definition,
  or `edit_analysis` (shown) when the panel edits one
  (`CommandSession::editAnalysis`, its inputs filled by `load` from the
  `analyses` query's entry). While an inspection's panel is open it shows
  its own section (`CommandHost::showSection`); otherwise
  `MainWindow::m_section` follows the model's shown analysis
  (`MainWindow::followAnalyses` in `refreshScene`). Print Analysis
  (mitcad#620) keeps its analysis the same way; its `Inspection::printOverlay`
  goes to `CommandHost::showPrintAnalysis` (every `keepsSection` panel's
  inspection: an empty one draws nothing), and a long query of an
  inspection goes through `CommandHost::jobQuery`, a model job with the
  progress and its Cancel. A link `point:x,y,z` in an inspection's result
  frames the view 20 mm around that point (`resultLinkActivated`, logged
  `<command>: shows point:…`). Inspections draw with
  `setPreview`, so a command started during one ends it. An inspection's
  `highlights` are drawn orange besides the inputs' (a Component item: the
  bodies of its occurrence), and its `report` goes to the definition's
  `previewMarkers` for labelled points over the view (`previewMarkersIn`
  gets the state too). `hover(item, state, context)` is called when the
  mouse moves onto another item of the view (an invalid item: off the
  bodies); true when it changed the state redraws the markers, nothing
  is computed (BUILD's Connector: the junctions of the profile under the
  mouse; 3D Print's bed face choice: the face's body in the status bar,
  mitcad#857). `hoverPreview(item, state, context)` (mitcad#417) is called as
  the point under the mouse moves (`OcctViewer::hoverMoved`: the item with
  its `at`; an invalid item off the bodies or when the mouse leaves the
  view), not while the model computes or the view is navigated; true
  when it changed the state computes the preview again (no command does
  any more: BUILD's Profile, Linear Part and Timber only log the face a
  click picks and return false, mitcad#508, a preview that followed the
  mouse lagged).
  `CommandContext::pixelSize(point)` is the size of a view pixel in mm
  at a point of the design (NaN on the model's worker), for snap
  distances that keep their size on the screen (BUILD's `at.snap`).
- `apply(state, context)` with `inspect` (mitcad#375, BUILD): a panel with
  model commands of its own. The inspection is the preview; OK (its
  `okLabel`, Place) is refused while the inspection failed, else calls
  `apply`, which runs its commands through the main window
  (`DocumentHost::runModelCommand`, each its own undo step, or
  `runModelCommands` for one) and may change the inputs; false (the model
  refused: its reason is shown) keeps the panel as it is. Without
  `repeats` the panel then closes (`Done <command>`); with `repeats` it
  stays open for the next one (`Applied <command> (<n>)`), its first
  needed selection active again, and its Cancel is **Close**: what was
  applied stays (`Closed <command> after <n>`, also in the AppKit
  frontend, whose checkpoint is then kept). A repeat previews nothing
  until the next pick (mitcad#513): BUILD's `apply` records it in the
  state (`placed_one`), and a place that would fall back to a default
  without a pick (a ground placement at the origin, a frame from the
  origin) is then a waiting inspection instead (`Inspect <command>:
  failed: Click ...`, OK refused), not a ghost on top of what was just
  placed.

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

- The idle selection card (Floating) and info panel (Docked) show each
  selected face, edge, vertex or body with its **Component:** occurrence
  name, including the full nested path separated by `›`, and **Body:**
  display name above its measurements. Root bodies omit the Component
  line. Every displayed item's block names its own owners, including
  selections spanning several bodies or occurrences.
- With exactly two faces selected, the same card/panel also shows
  **Between faces** automatically. Parallel planar faces show their
  perpendicular **Distance**; when their bounds do not overlap across
  that normal, **Minimum distance** also shows the longer actual gap.
  Other pairs show the actual bounded **Minimum distance** and
  **Maximum distance**. The maximum means the largest distance from a
  point of either face to its nearest point on the other face (the
  symmetric Hausdorff distance), rather than the longest point-to-point
  diagonal. This exact maximum is available for bounded convex planar
  faces with straight edges and no holes. Curved, concave or holed faces
  show `Maximum distance: unavailable (requires convex planar polygons)`
  while keeping their exact minimum; no mesh approximation is substituted.
  The measurements use each occurrence's design placement, the document's
  length units and the usual dimension display precision. Selection,
  geometry, placement, undo and unit changes invalidate the cached values;
  a failed query displays `Measurements unavailable`, and selecting any
  other combination removes the pair block. Both Floating and Docked
  layouts keep the existing owner and area details beside these values.
- In Solid mode, `AutoselectPanel` offers the last unused sketch's profile
  groups from the model's `autoselect_profiles` query. Group clicks apply
  the viewport's per-item Ctrl-toggle semantics to the current selection;
  they preserve unrelated items and pass every selected profile to the
  next command as normal preselection. Hover is a separate highlight list.
  Scene refresh clears that preview, replaces the groups and removes idle
  profile selections whose exact keys no longer occur in `profiles`.
- The test driver's `expect-selection-info <text-regex> [maximum-card-height]
  [with-create-row]` checks measurements and compact card sizing. The optional
  `with-create-row` adds only the visible Create row's size hint and layout
  gap to the existing height budget; hidden rows add nothing. This lets the
  MCP UI test retain its measurements budget after the row added by #619.
- The card's Create row (mitcad#619, `framework/SelectionCreate.cpp`,
  `MainWindowSelectionCreate.cpp`) is under the selection's text and
  above Autoselect. It shows up to four buttons, each with the command's
  icon and name. The rule is in `selectionCreateCandidates`: one list per
  kind of selection, Extrude first in each. Every item must be of the
  kind; a mixed selection has no list.

  | Selection | Buttons, in this order |
  |---|---|
  | profiles | Extrude, Revolve, Sweep, Loft (2 or more profiles only) |
  | planar faces | Extrude, Create Sketch, Hole, Emboss |
  | cylindrical faces, circular edges | Extrude, Thread, Pulley, Gear, Bearing |
  | sketch points | Extrude, Hole, Pattern at Points |
  | sketch curves, edges | Extrude, Sweep, Pipe, Pattern on Path |
  | bodies | Extrude, Rectangular Pattern, Circular Pattern, Mirror, Combine |
  | construction or origin planes | Extrude, Create Sketch |

  A command is shown only when its action is enabled and it can use the
  whole selection (`takesWholeSelection`): its first selection input is
  shown in the command's start state, takes every item in that state (as
  preselection does: Extrude takes sketch curves only as a thin
  extrusion, which is not its default), has room for all of them (`max`),
  and `offeredFor` holds. Create Sketch takes one plane or planar face.
  The first four that apply are shown. Some listed commands do not show
  now, because their first selection input takes other items: Emboss
  (profiles) for a face; Hole (a face) and Pattern at Points (bodies) for
  sketch points; Sweep (profiles) and Pattern on Path (bodies) for a path;
  Thread, Pulley, Gear and Bearing (faces) for a circular edge. When none applies, the row is hidden.
  A click starts the command as the ribbon does (`trigger`): the
  selection goes to its inputs as preselection. The buttons are made once
  and changed in place when the selection changes. The Docked panel gives
  the row a section title, as Autoselect. The native macOS inspector
  mirrors the idle page's widgets, the row too. Logs:
  `Selection card create: Extrude | Revolve | Sweep` (or `none`) when the
  row changes, `Selection card create Extrude at x,y` for each button, and
  `Selection card create chose Extrude` on a click. Tests:
  `app/tests/selection_create.cpp` (`app.unit`),
  `tools/ui-selection-create-test.sh` (Docked and Floating),
  `tools/ui-windows-selection-create-test.ps1` (ctest
  `app.ui-windows-selection-create`).
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
- While the panel is open, the sketches the command uses are shown as if
  their light bulbs were on (mitcad#546; `view/CommandSketches.hpp`,
  `MainWindowVisibility.cpp`): an edit's are the edited feature's (the
  `timeline` query's `sketches`: profiles, paths, rails, sections, hole
  points, guide curves), a new command's are those of the profiles, sketch
  curves and points its selection inputs start with (selected before it,
  or its default, such as Extrude's newest free profile, which may lie in a
  hidden sketch). Their curves and points are drawn and pickable, all
  their regions too (also those a feature consumed, as for a sketch the
  user showed), and their dimensions are offered as parameter labels. A
  sketch shown only for the command (its light bulb off) is drawn a little
  lighter. When the panel closes (OK, Cancel, Esc, another command) each
  goes back to its light bulb: hidden again unless the user turned it on
  meanwhile (that light bulb is a visibility change of its own, kept as
  any light bulb during a command is). It is a view state of the window,
  never `set_feature_visible`: nothing is saved and no undo step is made.
  Picks during a new command show nothing more: the view picks from shown
  sketches only, and a face picked as Extrude's profile stays a face.
  Logs: `Sketch Sketch1 shown for the edit of Extrude1` (`shown for
  Extrude` for a new command), `Sketch Sketch1 hidden again after the edit
  of Extrude1`, `Sketch Sketch1 stays shown after the edit of Extrude1 (its
  light bulb)`, and `Sketches shown: …`. Tests:
  `tools/ui-edit-sketches-test.sh`, `tools/ui-windows-edit-sketches-test.ps1`
  (ctest `app.ui-windows-edit-sketches`), `app/tests/command_sketches.cpp`
  (`app.unit`).
- The context menu offers the feature commands whose first selection
  input takes everything selected, "Edit <feature>" for an item a feature
  made, and Repeat; in sketch mode, model faces, edges and vertices get
  Project. Outside sketches the commands are in submenus by kind, those
  the selection completes first (see [The view's context
  menu](#the-views-context-menu)).
- Inputs that take `SelectKind::Feature` get the timeline's and browser's
  clicks on features (`DocumentHost::takesFeatures`); a face clicked in
  the view goes in as the feature that made it (`withConvert`).
- `withAlsoTakes(kinds)` (mitcad#522): kinds an input takes from what was
  selected before the command, and the context menu offers it for, beyond
  the kinds the view picks for it, kept in `withConvert`'s form: a BUILD
  member's edge selected before names the member as its body, while clicks
  in the view still pick its faces and bodies only.
- Handles (`ManipulatorOverlay`) are drawn for shown value inputs that
  have them, the active one's knob orange. The overlay follows the camera
  through `OcctViewer::viewChanged` and `toWidgetF`; a new display style
  or section view must keep these right.
- Long panels scroll (`m_panelScroll`); adding or removing a list row logs
  the panel's places again, and so does scrolling it.
- The panel is never wider than its page, which does not scroll sideways
  (mitcad#198): its message (`CommandMessage`) breaks between words and
  inside a word longer than the line (a profile's or face's name), shows
  at most six lines and scrolls beyond them, can be selected and copied
  whole, and is hidden when empty; the notes under the fields break long
  words too (`breakableText`). A row that still asks for more width is
  squeezed rather than pushing the fields and OK and Cancel out of sight.

### The view's context menu

Outside sketches and commands (`MainWindowContextMenu.cpp`, mitcad#494;
sketch mode's menu and a command's OK / Cancel are as they were):

```
Repeat <last command>
Solid      >  the commands the selection completes ... | More > the others
BUILD      >  (only on BUILD parts)                    | More > ...
Construct  >  ...                                      | More > ...
Create Sketch, Edit <feature>, Move/Copy (bodies or components), Edit Sketch, Redefine Sketch Plane, Save As DXF
Measure (and the other INSPECT commands that take the selection)
Appearance...
Material... (the Materials window: the bodies' material; for a component alone its default material)
Property Sets... (a body's, or a component's, with its occurrence and definition)
Delete     >  Member, Spacing Set, Body, Component, Feature
Hide       >  Body, Component
Find in Browser
Save As Mesh... (bodies and components; disabled with the reason without a solid or mesh body)
Clear Selection, Undo, Fit, Command Search
```

- **Kinds.** A command of the BUILD tab goes into BUILD, one of the
  CONSTRUCT group into Construct, one of INSPECT into the menu itself,
  every other feature command into Solid; a submenu with nothing is left
  out. BUILD is there only when every item is on a BUILD part
  (`build::partOf`); a face of an ordinary body has no BUILD.
- **First, or under More** (`CommandDef::completedBy`, computed from the
  definition): first when the selection alone completes the command: its
  first selection input takes every item and as many as it needs (its
  `min` to `max`), every other selection input shown at the start (the
  inputs' defaults, a list's first rows) may stay empty, and it is no
  creation command (group CREATE) given a face or a plane, which would
  only be where it goes (Box, Cylinder, Hole, Coil, Gear, Pulley, Bearing, Plastic
  Feature), or given anything a placement input takes (`withPlaces`,
  mitcad#656: a shaft's edge, an axis or a point for Place On, the axes
  of Gear Pair and Belt Drive).
  Otherwise, needing further picks, it is under **More**, the submenu's
  last entry. In each, the commands keep the registry's (the ribbon's)
  order. A definition says what its inputs cannot:
  - `completes`, when set, replaces the creation rule: Thread (the faces
    are what it threads), Profile and Linear Part (not on a timber
    member: under More there), Timber Frame (only a frame's member
    completes it), Floor / Roof (members of two wall frames complete its
    roof, mitcad#520).
  - `offeredFor`, when set, must hold too: Spacing Set only on members of
    a spacing set; Swap Fastener (mitcad#497) only when every item is on a
    fastener, which then completes it (BUILD's first entries); Floor /
    Roof only on members of wall frames (mitcad#520; Timber and Wall on
    any member's face).
  - `surfaceBodiesOnly`: offered only when every item is on a surface body
    (no solid): Stitch, Thicken, Extend, Trim, Untrim, Unstitch, Reverse
    Normal, Match Surface, Fair Surface.
- **At the top** (mitcad#735): a feature command whose `offeredAtTop`
  holds for the selection is an entry of the menu itself, after Edit
  <feature>, and not in its kind's submenu. Move/Copy sets it with
  `takenWholeByOneOf` of its two Objects inputs: a selection of one kind
  that one input takes whole, bodies or placed components (an occurrence;
  not the root), never a mix, which would go to both inputs. The entry is
  the command's own action: it starts with the selection. The browser's
  rows use the same rule (`DocumentHost::offersCommand`, below).
- **What the right click selects:** a right click on something not
  selected selects it, except a face, an edge or a vertex of a body or an
  occurrence selected whole (a body's or a component's row, or the
  Components filter): the selection stays, so that the menu is the body's
  or the component's (mitcad#735), with Move/Copy at its top.
- **Delete** holds whole things, each when the selection has it, each one
  undo step: **Delete Member** (`build_remove` of each BUILD part, with
  the joints, fasteners and holes it added), **Delete Spacing Set**
  (`build_remove` with `set: true`, when a member is in a set), **Delete
  Body** (a `remove_body` feature in each body's component,
  `BrowserController::deleteBodies`), **Delete Component** (the
  occurrences, `delete_occurrence`), **Delete Feature** (the one feature
  that made the items, as the browser's Delete: the features that use it
  too when the user says so; not for BUILD parts, which go as members).
  Counts as Hide's: "Delete 2 Bodies". Delete Face stays in Solid.
- The log names each submenu's entries: `Context menu: Repeat Box | Solid
  | Construct | Create Sketch | ...`, `Context menu Solid: Press Pull |
  Fillet | ... | More`, `Context menu Solid More: Draft | Hole | ...`,
  `Context menu Delete: Delete Body | Delete Feature`, `Context menu Hide:
  Hide Body`; `tools/ui-context-menu-test.sh`; Move/Copy at the top:
  `tools/ui-move-copy-menu-test.sh`, `tools/ui-windows-move-copy-menu-test.ps1`
  (ctest `app.ui-windows-move-copy-menu`), `app/tests/context_menu.cpp`
  (`app.unit`).
- **Save As Mesh…** (mitcad#734, `files/MainWindowSaveAsMesh.cpp`,
  `files/SaveAsMesh.hpp`) when every item is on a body or is a component;
  also in the browser's menus of body and component rows, last before
  Collapse All (for the rows selected with it; the root's row is the whole
  design). Disabled, with the reason as its tooltip (logged `Save As Mesh disabled: <reason>`),
  when the items have no solid or mesh body. Its dialog (Format, Unit,
  Structure, Refinement with Custom's three numbers, On its print bed face)
  logs `Save As Mesh dialog opened: bodies …; print bed face available` and
  `Save As Mesh dialog: 3mf, mm, one file, medium`; each file written
  `Saved as mesh <path>: …, <n> body(ies)`, and on the bed `Save As Mesh
  bed <file>: …` as 3D Print's. The model's
  [`export`](../core/model-api/src/commands.md#mesh-files-in-a-unit-with-a-longest-edge)
  writes the files; `tools/ui-save-as-mesh-test.sh`,
  `tools/ui-windows-save-as-mesh-test.ps1`.

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
`measure` query. Each pick goes to the query with its occurrence
(`SelectionItem::occurrence`, mitcad#646), as do the selection card's
measurements, so that the model measures bodies and datums of components
where the view shows them (in the design's coordinates). For two parallel
planar faces or planes the panel shows `Offset: <d> mm`, their distance
along the normal, under the closest distance and the angle.

Measure's picks prefer the body (mitcad#869). Its Selection Filter is
Faces, Edges and Vertices (the default), Bodies, or Sketch Curves and
Points. With Faces, Edges and Vertices the inputs put sketches behind the
bodies (`InputDef::sketchesBehind`, `OcctViewer::setSketchesBehind`,
`PickContext.hpp`'s `setPickYield`): a shown sketch's curves and points,
which the view draws over the bodies, come after the bodies' faces, edges
and vertices under the pointer, so a sketch point on a body's corner or
a sketch line on its edge or face gives the body's (a sketch in the
middle plane of a symmetric extrusion), and the hover highlights what the
click takes. A sketch's curve or point is picked where no body is under
the pointer; those of the sketch that sketch mode edits keep their place
(`CommandHost::editedSketch`). Sketch Curves and Points takes only sketch
curves and points (its own inputs `sketch_a` and `sketch_b`, as Bodies has
`body_a` and `body_b`), also where they lie on a body.

Axes in Measure (mitcad#647): a pick with an axis (a cylindrical,
conical or toroidal face, a circle or an arc, a construction axis) shows
`Axis: through (x, y, z), direction (dx, dy, dz)` (its point, the centre
of a round face or a circle), and the view draws the axis as a line
through the pick while the panel is open (with the closest distance's
line and the axis distance's). Every pair with an axis, a straight edge
or a normal has an `Angle` (axes and edges 0 to 90°, against a plane
from the plane). With an axis on one side the panel adds `Axis distance:
<d> mm` and `Axis from (…) to (…)`: parallel axes their distance, skew
ones the common perpendicular, meeting ones 0 and the point, an axis and
a vertex or a construction point the perpendicular distance, an axis and
a plane parallel to it the offset; two circles or arcs also `Centre
distance: <d> mm`. The closest distance stays above them. The log line
(`Inspect Measure: …`) adds `axis (x, y, z) along (dx, dy, dz)` per pick,
`axis distance <d>`, `center distance <d>` and `axis lines <n>` (the axes
drawn).

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

**Keys per context** (mitcad#720). A command's shortcut contexts
(`CommandRegistry::shortcutContexts`, `ShortcutContext`) come from its
`mode`: Model, Sketch or both (`Any`), Sketch too for a profile feature
(Extrude, Revolve), Command Panel (a command's panel is open) for
`duringCommands`. Two commands may share a key unless a context of the
one is a context of the other; a conflict's message names where they meet
("S is used by both Command Search and Fit Point Spline in a sketch.
Change one of them."; the others "in the model" and "while a command
panel is open"; a reserved key: "Ctrl+S is reserved for Save. Choose
another key for …"). A command of several contexts has a key in each, the same one
by default: `shortcuts/<id>` in the settings is its key in every context
(as before), `shortcuts.model/<id>`, `shortcuts.sketch/<id>` and
`shortcuts.commands/<id>` a context's own key, written only where the keys
differ. `MainWindow::updateActions` sets the context by the mode
(`setShortcutContext`: Sketch, a command's panel, else Model) before it
enables the actions, and the actions of several contexts then carry that
context's key; `effectiveShortcuts`, `shortcutHint` and the native menus
follow it. Keys that differ by context are logged at the start and after
Keyboard Shortcuts, one line per command: `Shortcut keys of tools.search by
context: Model S, Sketch F3, Command Panel S` (`(none)` for a cleared
key).

Keyboard Shortcuts (`framework/ShortcutDialog`) is a tree: Command,
Context (a fixed width, the longest text's) and Shortcut. A command of one
context is one row; a command of several is a row with its contexts
(All for the three, else their names: "Model, Sketch" for Extrude) and
the key of every one, or Varies when they differ, and a child row per
context (Model, Sketch, Command Panel), collapsed unless its keys differ.
With the context filter (All Contexts, Model, Sketch, Command Panel) on
one context, every command of that context is one flat row of that
context's key alone, and the Context column is hidden (mitcad#730). The
row the user selected last is selected again when the rows are made
again (a filter change, Restore Defaults) if its command is shown: a
context's row becomes the command's flat row, and back to that context's
row (its command's rows opened) when all contexts are shown. The row
picked that way is not remembered in place of the user's, so All, Model,
All comes back to the Sketch row the user chose. The reserved keys
(`fixedShortcutBindings`, by `reservedShortcuts`: one row per name with
all its keys, such as Ctrl+F for Search Timeline and Return, Enter for
OK) are muted, read-only rows of their contexts whose key reads `Ctrl+F
(reserved)`: under the command of the same id when it is listed (Search
Timeline, its rows then open), else after the commands. Search Timeline,
Rename and Delete have the tooltip "Works while the Browser or the
timeline has the keyboard focus", the others "Reserved; this key cannot
be changed". They cannot be recorded, cleared or reset: Enter, F2, a
double-click or a click on the key shows "Ctrl+F is reserved for Search
Timeline." as a tooltip at the row. The search finds them by name or key
(Ctrl+F finds Search Timeline). Their names are those the UI shows
(Search Timeline, Rename, Delete, OK, Cancel), as conflict messages name
them. A click on a Shortcut cell, Enter or F2
records the next key on that row (Esc or Enter again keeps the old one);
a key changes the row's contexts only: a command's row all of them, a
context's row (or a flat row) its own, so the command's keys come apart
and join again as they are given. A selected row's icon is drawn in the
selection's text colour. Delete or the row's menu (Clear Shortcut) clears a key, Reset to
Default restores one; Down goes from the search to the first row found.
OK with a conflict shows its message and selects the row to change (the
command changed last, in the context where the keys meet). Its log lines,
for the UI tests: `Keyboard Shortcuts opened`, `Keyboard Shortcuts row:
Command Search | Sketch | S` (the current row: command, context, key in
portable text, `Varies` or `(none)`; a reserved row `Search Timeline |
All | Ctrl+F (reserved)`), `Keyboard Shortcuts expanded:
Command Search`, `Keyboard Shortcuts recording: Command Search | Sketch`,
`Keyboard Shortcuts key: Command Search | Sketch = F3` with `Keyboard
Shortcuts keys of Command Search: Model S, Sketch F3, Command Panel S`,
`Keyboard Shortcuts recording cancelled`, `Keyboard Shortcuts context
filter: Sketch` with `Keyboard Shortcuts columns: Command, Shortcut` (the
columns shown) and `Keyboard Shortcuts selection kept: Fit Point Spline |
Sketch`, `Keyboard Shortcuts reserved: Search Timeline | Ctrl+F is
reserved for Search Timeline.` (a reserved row's key asked for), `Keyboard Shortcuts conflict at: Fit Point Spline |
Sketch`, `Keyboard Shortcuts conflict: <message>` and `Keyboard Shortcuts
saved`. `tools/ui-command-test.sh` and
`tools/ui-windows-shortcut-contexts-test.ps1` drive it by its keys: Command
Search expanded, its Sketch row F3, Fit Point Spline S, OK; then F3 for
the spline too, which OK refuses, and, filtered to Sketch, F5 on Command
Search's one row, which leaves its other keys alone (no Context column,
the spline's row selected again after the filter passed Model), All
Contexts again with Command Search's Sketch row selected (Model, then
All Contexts once more, still the Sketch row), and Search Timeline's
reserved Ctrl+F row under it, where Enter records nothing and says why.
S then
searches in the model and draws the spline in a sketch, where F3
searches.

### Measure's history

Each finished measurement of the Measure panel (one or two picks with a
result) is a row of its history (mitcad#648; `commands/MeasureHistory`,
`commands/MeasureHistoryView`, `MainWindowMeasureHistory.cpp`). The
inspection gives the row (`Inspection::record`: what was picked, its main
values, the inputs that hold the picks) and the session hands it to the
host when the panel shows it (`CommandHost::inspectionRecorded`); the
window keeps the rows and the panel shows them above its result
(`CommandPanel::addSection`), in the Floating and the Docked layout.

- **Rows.** Newest first, each with its number, what was picked (`Face of
  Body1 ↔ Edge of Body2`; a body's name, a construction feature's name,
  `Plane XY` for the origin's) and its main values (`Distance 40 mm`,
  `Angle 90°`, `Offset`, `Axis distance`, `Centre distance`, the circles'
  `Radius` and `Diameter`; of a single pick also its `Length`, `Area`,
  `Volume` or `Position`). The first pick's row becomes the pair's when the
  second pick follows in the same panel; a pick that replaces the second
  makes a new row; the same picks again (a row shown again) update their
  row in place. The list scrolls past four rows, so the panel does not
  grow past the screen.
- **Show again.** A click on a row puts its picks back into the panel's
  inputs (`CommandSession::restoreInputs`, with the Selection Filter they
  were made with): the panel measures them again, highlights them and
  draws its lines. A row whose picks the view no longer has (a body or a
  face gone) is grey and says `The geometry is not there any more.`; a
  click on it measures nothing. The rows are checked when a Measure panel
  opens and when one is clicked (not while the model computes).
- **Copy, Copy All, Clear.** Copy puts the row clicked last on the
  clipboard as tab-separated text (number, picks, each value), Copy All
  every row, one per line, newest first; Clear empties the history and
  its numbers start from 1 again.
- **Lifetime.** The history lasts while the design is open, also across
  closing and opening the panel; opening or creating another design
  empties it. It is not saved with the design. The macOS AppKit form
  (`MITCAD_MACOS_NATIVE_UI`) has no history.
- **Log.** `Measure history: <n> rows` and `Measure history row <n>:
  <picks>: <values>` (` (gone)`) whenever the rows change; their places
  `Measure history row <n> at x,y` (`… out of sight` scrolled away) and
  `Measure history Copy|Copy All|Clear at x,y` (` (disabled)`) with the
  panel's places; `Measure history showed row <n>`, `Measure history row
  <n>: gone`, `Measure history copied row <n>: <text>` and `Measure
  history copied <n> rows: <text>` (tabs as ` | `, lines as ` / `),
  `Measure history cleared`.

Tests: `app/tests/measure_history.cpp` (the rows' rules),
`tools/ui-measure-test.sh` and `tools/ui-windows-measure-test.ps1` (three
measurements, three rows, the first shown again, copied, cleared; one row
in the Floating layout). `tools/ui-measure-test.sh` also checks the
picks' preference (mitcad#869): with a shown sketch in the middle plane
of a symmetric extrusion, seen from the top, two corners give two body
vertices and Keep is enabled; with Sketch Curves and Points two sketch
points, and Keep greyed out with its reason.

### Measurement objects

A measurement kept in the design (mitcad#649; since mitcad#702 an
annotation, *Annotations* below; the model's *Measurement objects* and
*Annotations* in `core/model-api/src/commands.md`;
`MainWindowMeasurements.cpp`, `view/MeasurementDrawing`,
`view/MeasurementOverlay`, `view/Snapshot.cpp`, `view/SnapshotDialog`,
`view/ViewSnapshot.cpp`).

- **Keep.** The Measure panel's history section has **Keep** at its top:
  the measurement the panel shows becomes an annotation (`add_annotation`
  with its picks, their occurrences and the view — its direction, up and
  a pixel's size at its centre, `MainWindow::annotationView` — so that a
  length lies in the plane square to the view, its dimension line on the
  view's upper side; the model chooses the kind — a round pick's
  diameter, an arc's radius, of two picks their offset, axis distance,
  centre distance, distance or angle — the name `Distance1`, `Angle1`, …
  and the next of six colours). **Keep Row** beside it keeps the history
  row clicked last (its picks as the row has them). Keep is enabled once
  the panel shows a result of the model's picks: sketch curves and points,
  which the panel measures itself, cannot be kept (a hint says so). Greyed
  out for them, Keep's tooltip gives the reason instead of what it does
  (mitcad#869: "Sketch geometry cannot be kept as an annotation: pick the
  body's corners or edges"; `MeasureHistoryView::setKeepable`).
  **Annotations…** beside them opens the Annotations window. Under them
  **Note…**, **Datum Label…** and **Frame…** (enabled with one face, edge
  or vertex of a body or one datum plane, axis or point picked) put a
  leader on that pick, pointing where it was clicked (`SelectionItem::at`,
  in the pick's component's coordinates, as the model keeps it;
  `MainWindow::keepLeader`): Note… asks for a note's text (the **Note**
  dialog, several lines), Datum Label… for a datum label's **Letter** (the
  **Datum Label** dialog, the model's `next_datum_letter` offered: A to Z
  without I, O and Q, then AA …; kept, no `letter` is sent and the model
  chooses it), Frame… for a feature control frame (the **Feature Control
  Frame** dialog, *Annotations* below). **Hole Callout** beside them
  (mitcad#902; enabled as Datum Label…) puts a hole callout on the hole
  picked — its round face (a wall, a counterbore's or countersink's face)
  or a rim — without a dialog: the model writes its text from the hole's
  feature (`4x Ø6.6 THRU`, `⌴ Ø11 ↧ 6.8`, `M8x1.25 - 6H ↧ 15`; the
  model's *Hole callouts and the reference look*) and points its arrow at
  the hole's edge where it was clicked; a pick that is no hole is refused
  with the reason (`Leader hole_callout not added: …`). A datum label, a
  frame and a hole callout stand on a feature: on a vertex or a datum
  point a hint says so and no dialog opens (the model refuses them too). The change runs outside the panel's
  undo range as a light bulb's does (`CommandSession::runVisibility`): it stays when the panel
  is cancelled and is an undo step of its own. The macOS AppKit form
  (`MITCAD_MACOS_NATIVE_UI`), which has no history, has no Keep either;
  the window, the view and the snapshot work there.
- **Browser.** The root's **Annotations** folder is one row, shown while
  there are any; its tooltip counts them and the broken ones (the row in
  the failure colour with the warning icon then). A double-click and its
  menu's **Annotations…** open the Annotations window; the menu also has
  Measure.
- **Export Snapshot** (View menu, command search; `view.export_snapshot`,
  also in read-only designs): a dialog with the file (PNG or JPEG,
  Browse…), **Size** (2 × or 4 × the view, or **Width** in pixels with the
  view's aspect; at most 16384 pixels a side), **Transparent background**
  (PNG) and **Annotations** (All shown, None, or `Group <name> (<n>
  shown)` for each group with shown annotations: that group's only),
  remembered in the settings group `snapshot` (all or none). Once the
  model is idle the view is rendered offscreen at that size
  (`OcctViewer::snapshot`: OCCT's `V3d_View::ToPixMap`, without the
  orientation cube; a transparent background from a render over black and
  one over white) and the annotations are drawn over it at the picture's
  scale.
- **Render Image** draws the shown annotations over the saved PNG or JPEG
  image (its **Annotations** check, on by default; not over OpenEXR),
  through the render's camera (`renderViewProjection` of the job's view).
- **Log.** `Annotations: <name> <kind>[ (hidden)][ in <group>] | <name>
  broken: <why>` (`none`) when they change; `Annotation kept: <name>`;
  `Measure Keep enabled|disabled[: <reason>]`, `Measure Note enabled|disabled` (Datum
  Label…, Frame… and Hole Callout with it), `Measure Keep at x,y` (` (disabled)`),
  `Measure Note at x,y`, `Measure Datum Label at x,y`, `Measure Frame at
  x,y`, `Measure Hole Callout at x,y`, `Measure Annotations at x,y` and `Measure history Keep at x,y`
  with the panel's places; `Measure Keep clicked`, `Measure Note
  clicked`, `Measure Datum Label clicked`, `Measure Frame clicked`,
  `Measure Annotations clicked`,
  `Measure history Keep clicked: row <n>`, `Measurement not kept (<what>):
  <why>`, `Leader note|datum|frame not added: <why>`; `Note dialog opened:
  <pick>`, `Note dialog: <lines joined by " / ">`, `Datum Label dialog
  opened: <pick> <letter>`, `Datum Label dialog: <letter>`, `Feature
  Control Frame dialog opened: <pick or dimension> <frame as JSON>`,
  `Feature Control Frame dialog: <frame as JSON>`;
  `Snapshot dialog opened: size <2x|4x|width>, transparent <on|off>,
  annotations <all|none|group <name>> (<n> shown)`, `Snapshot dialog
  <field> at x,y` (the dialog's own coordinates; `annotations` the
  combo box), `Snapshot dialog: size = …`, `… transparent = …`, `…
  annotations = …`, `Snapshot dialog: <size> <w> x <h> to <path>`,
  `Snapshot saved <path>: <w> x <h>, transparent|opaque, <n>
  annotation(s)[ of group <name>]`; `Render image measurements on|off`,
  `Render image saved <path> with <n> measurement(s)`.

Tests: `tools/ui-measurement-objects-test.sh` and
`tools/ui-windows-measurement-objects-test.ps1`
(`app.ui-windows-measurement-objects`: two round faces kept from the
panel, a rim's row kept from the history, the browser's folder, a 4 ×
snapshot with a transparent background written, saved and opened again).

### Annotations

Dimensions, leader notes, datum labels and feature control frames kept
in the design (mitcad#702; the model's *Annotations* and *Datums and
feature control frames* in `core/model-api/src/commands.md`;
`MainWindowAnnotations.cpp`, `view/AnnotationsWindow`,
`view/AnnotationDialogs`, `view/MeasurementDrawing`,
`view/MeasurementOverlay`). The application draws what the model's
`annotations` query lays out and changes them only through
`add_annotation`, `edit_annotation` and `delete_annotation`, each one
undo step outside a panel's undo range as a light bulb's (`runVisibilityCommands`). Showing
and hiding works in every mode, as a light bulb does; other changes too
while a command's panel is open (the Measure panel's Annotations… opens
the window there, mitcad#764), and are refused with the reason (`Annotation
command refused: <why>`) only in Create Sketch's plane pick and a
read-only window.

- **In the view** (`OcctViewer::setMeasurements`, `MeasurementOverlay`,
  painted with QPainter as the camera moves). Every shown annotation is
  drawn in its plane: the extension lines from a little off the measured
  points to a little past the dimension line, the dimension line, an
  angle's arc, the arrows (outside, pointing in, where a length is too
  short on the screen for them), a circle's centre mark, a leader with
  its arrow (a datum label's with a filled triangle on its feature); and
  its text set in the plane — foreshortened as the plane turns away,
  turned round where it would read mirrored or right to left (upright
  text reads upwards), faded out as the plane turns edge on (from about
  17° to the line of sight it is whole, under about 5° it is gone). A dimension's text stands above its line: the value with its
  prefix and own text, deviations or limits stacked after it in a smaller
  size (a fit's in brackets), the text after them; a note's lines start
  at its leader's end; a datum label's letter stands in a square there,
  and a feature control frame's cells in a row of boxes (ISO 1101: the
  characteristic's symbol, the tolerance with Ø and Ⓜ or Ⓛ, each datum;
  the fourteen symbols and the circled M or L drawn in lines, not taken
  from a font that may lack them: `paintCharacteristic`),
  or, under a dimension, centred just below that dimension's line where
  its text is (not dragged on its own).
  **The look of a technical drawing** (mitcad#894, mitcad#902: the
  reference PMI look; ISO 129-1, ISO 3098, ISO 128, ISO 1101;
  `paintMeasurements`), in both background tones: the text in the
  bundled technical lettering (an ISO 3098 type B style font,
  `third_party/fonts/iso-lettering`, a file beside the program loaded at run
  time, `measurementFontFamily`; the UI's font when it is missing), regular
  weight with ISO 3098's tight spacing, with no card or outline under it;
  it is the model's `text_height` (mm) high in its plane, so it grows and
  shrinks with the model like the lines, but its capitals as seen on
  screen (foreshortened as the plane turns away; measured on an H's ink,
  not the font's own cap height) never under 9 pixels
  (zoomed out) nor over 16 (zoomed in), times a picture's scale
  (`measurementTextFloor`, `measurementTextCeiling`); the depth symbol ↧
  is drawn in strokes (an arrow onto a bar) in the glyph's own advance,
  as fonts blur it at that size; a dimension's value
  centred above its line a quarter of that height off. Text reads from the
  bottom or from the right (ISO 129-1): never mirrored, left to right, and
  a text that would run down within 30 degrees of straight down runs up
  instead, so vertical and near-vertical texts read bottom to top from
  every side. Lines are crisp and thin (1 pixel times the picture's scale,
  ISO 128, on the pixel grid: `measurementSnapped`) with no halo;
  arrowheads closed and filled, 30 degrees, a text height long (at
  least 5 pixels), laid in the annotation's plane as its text is (so they
  foreshorten with it, but keep at least 0.6 of their width seen
  obliquely), a datum label's triangle as high, a leader on a
  face ending in a dot (the model's `terminator`), a circle's centre mark
  a cross in its plane; extension lines start half a text height off
  the point the model gives (the picked edge, vertex, or the picked face's
  own outline), a visible gap, and run 0.8 text heights past the
  dimension line; a
  frame's cells two text heights high and a datum label's square outlined
  thinly, not filled, the stem meeting the middle of the side it comes
  to. A note's, a hole callout's and a diameter on a leader's lines are
  1.6 text heights apart, the first level with the landing and the others
  below it, set flush left, and start at the leader's end, or end there
  where the model's `align` says so (the landing then runs on to the
  first line's end); a frame under a hole callout stands below its lines
  flush with their left edge (`measurementDisplays` gives each the
  other's words: `overLines`, `framesUnder`). No pixel sizes are frozen when an annotation is
  added: its default places are in text heights (the model's). An
  annotation of the colour **Auto** (the model's `auto`, a new one's) is
  drawn in the neutral annotation colour of the background tone
  (`measurementAutoColor`: near-black on the light background, a light grey
  on the dark one, not chalk-white).
  Over the model, Auto's ink follows what lies behind it: the overlay
  grabs the view's frame once the camera rests or the scene changes
  (`MeasurementBackdrop`, `Annotation backdrop: W x H` is logged): each
  piece of a line (5 pixels: extension and dimension lines, leaders) and
  each terminator (arrowhead, dot, datum triangle) takes the dark ink over
  a light backdrop and the light grey over a dark one, so that a leader
  stays readable over a body (its tone changes where it leaves the
  body); an annotation's text and frames take one ink, by what lies
  behind most of them (`measurementAutoInk`); no casing or halo. Until a
  frame is grabbed, the background tone's colour.
  Snapshots and Render Image take the tone of the picture's own
  background (`measurementPictureTone`; a transparent snapshot the
  view's). A selected one is drawn in the selection's blue, a hovered one
  in the hover's, with the same thin lines (at most 1.5 pixels) and the
  same text: no band, no box (`Annotation hovered: <name>|none` is
  logged). Its picks are not filled in the selection's colour
  (mitcad#902: only the annotation recolours;
  `MainWindow::addAnnotationHighlights` logs `Annotation picks
  highlighted` and draws nothing). A selection made in the view (or
  another document) takes the highlight away and deselects the window's
  rows
  (`MainWindow::dropAnnotationHighlight`).
- **The mouse.** The overlay lets the mouse through, but watches the
  view's: on an annotation's text or a length's dimension line (an
  angle's arc), seen at least half face on, outside commands and sketches
  (there the view takes picks: `AnnotationActions::active`), it takes the
  press and its release (also the release after a double-click or a right
  click; clicks elsewhere reach the model; the cursor is a hand there,
  and the view's own again, `OcctViewer::restoreCursor`, away from it). A
  click selects
  it (its row in the window, its highlight); a drag of its text or its
  line shows it moving and, released, moves it: the drag in pixels into
  millimetres along the plane's axes (`measurementDragMillimetres`), added
  to the `text_offset` or the `line_offset` in use; a double-click opens
  its editor; a right click its menu (Edit…, Hide, Delete, Annotations…).
- **The editor** (**Edit Annotation**, `editAnnotation`): **Name**,
  **Kind** (the values its picks have, when more than one), **Prefix**
  (the kind's, Ø or R, when not changed; emptied for none),
  **Decimals** (Automatic, 0 to 6), **Text before**, **Text after**,
  **Replace value**, **Tolerance** (None, ± symmetric, Deviations, Limits,
  Fit (ISO 286); **Value**, **Upper deviation** and **Lower deviation**,
  or for Limits **Upper limit** and **Lower limit**, the sizes as they are
  to be written (kept as deviations from the value), in mm or degrees;
  **Fit** an editable list of ISO 286-2's common codes, **Show
  deviations**), **Colour** (Auto first, then Black and Dark Grey,
  mitcad#902: a technical drawing's colours, the bright presets gone; an
  annotation's own colour, an older preset's too, kept and offered as it
  is; Custom… with the colour dialog), **Group** (editable, the groups
  there are), **Shown**; a hole callout shows its text (the hole's,
  **Callout**, not edited); a note
  has **Text** (several lines) instead of the value's fields, a datum
  label its **Letter**, a feature control frame the frame's fields
  (below) and **Placed** (On its leader, or Under each dimension: `on`).
  Every one shows its picks — **First pick** and **Second pick** (a
  leader's **Points at**) and **Plane** (the face or datum plane picked,
  the view's when it was kept, or Automatic) — each with a button:
  **Pick Again…** and **Pick…** close the editor (its other changes run
  first) and wait for a click in the view; **Automatic** gives the plane
  back to the model (mitcad#894: a length's dimension line too, `plane`
  and `line_offset` null: placed outside its bodies again). A radius, a diameter and an angle have no Plane row
  (they lie in their circle's or their sides' plane, and the model takes
  no plane for them); where Kind offers one of them beside a kind that
  takes a plane, choosing it disables the row (`The plane of its
  circle|sides`) and choosing the other enables it again (mitcad#713). A
  frame under a dimension shows neither its pick nor its plane: it stands
  in the dimension's place, as Placed says. Placed set to On its leader
  shows Points at with Pick Again…; the pick then sends `on: null` with
  the new `a` and `at` in one `edit_annotation`. A frame whose pick under
  the dimension is a vertex, a datum point or a whole body
  (`AnnotationEditContext::leaderNeedsPick`: the model refuses a leader on
  them) is asked for that pick on OK (`(pick a with {"on":null})`) instead
  of sending an edit the model refuses; its plane row shows once it
  stands on its leader (the editor opens again after the pick). A broken one says why at
  its top. OK runs `edit_annotation` with the fields that changed — the
  tolerance (a frame's too) only when its fields were changed, limits
  typed as sizes read back as deviations to the fields' four decimals.
  Another kind chosen relabels the tolerance fields (degrees and no Fit
  for an angle) with its value, and drops the tolerance between an angle
  and a length.
- **A pick again** (`MainWindow::startAnnotationPick`): outside commands
  and sketches, the selection cleared, the view's banner says what to pick
  (`Edit <name>: pick a face, edge, vertex or datum again. Esc cancels.`;
  a datum label's or a frame's: a face, edge, datum plane or datum axis,
  no vertex or point; a planar face or a datum plane for the plane) and
  the view picks only that; the click runs `edit_annotation` with `a` or
  `b` (a leader's `at` where the click hit) or `plane: {"pick": …}` and
  opens the editor again;
  a click on nothing or a pick the model refuses keeps waiting
  (`Annotation pick still waited for: <name> <field>`); Esc cancels and
  opens the editor again. Undo, Redo, a command, a sketch, Create Sketch,
  another document, an editor opened, or the annotation gone after a
  refresh ends the pick without opening the editor
  (`MainWindow::endAnnotationPick`, `followAnnotationPick`). While a
  command or a sketch is open the editor's Pick Again… and Pick… are
  disabled.
- **Feature control frame** (the **Feature Control Frame** dialog and the
  editor; `FrameFields`): **Characteristic** (each one's ISO 1101 symbol
  and name, from the model's `geometric_characteristics` query),
  **Tolerance** (the zone's width, mm), **Cylindrical zone (Ø)** and
  **Material condition** (None, Ⓜ Maximum material, Ⓛ Least material)
  only where the characteristic takes them, **Datums** (three editable
  lists of the design's datum letters, in order; none for a form
  tolerance, at least one where it needs one).
- **The Annotations window** (`AnnotationsWindow`; View menu and command
  search `view.annotations`, the Measure panel's Annotations…, the
  browser's folder): a search field, **Kind** and **Group** filters (All
  groups, No group, each group) with **Show Group** and **Hide Group**
  (the chosen group's annotations, one step), a sortable table — Name
  (a swatch of its colour, or the warning icon with the reason in its
  tooltip for a broken one), Kind, Value (the label; `Broken` in red with
  the reason; a hidden one's too: while the window is open it and the
  editor ask the model to measure the hidden ones, which a refresh
  otherwise leaves unmeasured, mitcad#716), Tolerance, Text (own text
  before or after the value, a
  text in its place, a note's text), Colour, Group, Shown (a check box:
  the light bulb) — and **Edit…**, **Colour…**, **Group…**,
  **Decimals…**, **Tolerance…**, **Show**, **Hide**, **Delete** for the
  selected rows (Decimals and Tolerance for the dimensions among them,
  Tolerance not for angles and lengths together;
  each one `edit_annotation` with `names`, one undo step), and **Add
  Frame…** for one dimension or hole callout that is not broken (a feature control frame
  under it: `add_annotation` with its first pick, `on`, its colour and
  group), and **Text Height…** (Alt+X, mitcad#894; the design's, not the
  rows'): the **Annotation Text Height** dialog, an editable list of Auto
  (with the height it gives, `Auto (2.5 mm)`) and the ISO 3098 series
  2.5, 3.5, 5, 7, 10 mm, a typed height taken from 0.1 to 1000 mm; OK runs
  `set_annotation_text_height` (`null` for Auto), one undo step. The
  Colour column shows `Auto` for an annotation in Auto. Kinds are Distance … Diameter, Note, Datum label, Feature
  control frame and Hole callout (mitcad#902; its Value its lines); a frame's Value is its cells (`⟂ | Ø0.05 Ⓜ | A`, a
  warning in its tooltip for a datum no label has), its Tolerance the
  tolerance cell, its Text `Under <dimension>`. The table has
  the keyboard when it opens (arrows, type-ahead, Ctrl+A); every button
  has its Alt key. A row selected highlights its annotation and its picks in the view
  (`MainWindow::highlightAnnotations`); a double-click opens its
  editor. The window follows the model after every refresh and keeps the
  whole selection (selected at once, `selectRows`); closing it takes the
  highlight away.
- **Several at once:** **Annotation Colour** (Auto, Black, Dark Grey, the
  first's own colour when it is another, and Custom…), **Annotation Group** (editable; empty for none), **Annotation
  Decimals** (Automatic, 0 to 6), **Annotation Tolerance** (the editor's
  tolerance fields; Limits as deviations, the values being several).
- **Log.** `Annotations drawn: <name> = <label> (#rrggbb|auto) | …` (`none`)
  when what is drawn changes; `Annotation label <name> at x,y` and
  `Annotation line <name> at x,y` (a length's line, an angle's arc; the
  label's place is its text's middle, or the middle of the part of the
  text in the view when the middle is past the view's edge) when
  their places change (also when the view's size or the camera moves
  them, `OcctViewer::logAnnotationPlaces`); `Annotations highlighted: <names>` (`none`),
  `Annotation picks highlighted: <name>: <pick>; … | …` (`none`);
  `Annotation pick started: <name> a|b|plane[ with <fields>]`, `Annotation picked:
  <name> <field>: <pick>`, `Annotation pick cancelled: <name>`,
  `Annotation pick ended: <name>`, `Edit Annotation pick
  a|b|plane|automatic plane at x,y[ (disabled)]` (the editor's buttons
  shown, once it shows, again while it settles and when its kind or
  Placed changes, when they change; watched by the test sync), `Edit Annotation dialog: <change>[ (pick
  <field>[ with <fields>])]`; `Annotations Add Frame clicked: <name>`, `Frame added under
  <name>`;
  `Annotation clicked: <name>`, `Annotation double-clicked: <name>`,
  `Annotation <name> dragged: text|line by <x> x <y> mm`, `Annotation
  <name> moved: <command>`, `Annotation command: <command>`, `Annotation
  command refused: <why>`, `Context menu: Edit… | Hide | Delete |
  Annotations…`; `Annotations window opened: <n> annotation(s)`,
  `Annotations row <name>: <kind>, <label>, <tolerance>, <group>, <#colour|auto>,
  shown|hidden|broken: <why>` (`-` for none) when the rows change,
  `Annotations rows: none`, `Annotations shown rows: <names>` (those the
  search and the filters leave, `none`), `Annotations selected: <names>`
  (`none`),
  `Annotations group filter: <group>`, `Annotations show|hide group
  <group>: <names>`, `Annotations Delete clicked: <names>`, and the
  places in the main window's coordinates: `Annotations row <name> at
  x,y`, `Annotations shown <name> at x,y`, `Annotations <search|kind
  filter|group filter|Show Group|Hide Group|Edit|Colour|Group|Decimals|
  Tolerance|Add Frame|Show|Hide|Delete|Text Height> at x,y` (` (disabled)`),
  `Annotations Text Height clicked`; the dialogs'
  `<title> dialog opened: …` and `<title> dialog: <change as JSON>`
  (`Edit Annotation dialog opened: <name> <kind>`, `Annotation Colour
  dialog opened: <what> <#rrggbb|auto> (<names>)`, `Annotation Text
  Height dialog opened: <height|null> (<shown>)`, `Annotation Text Height
  dialog: <height|null>`, `Annotation Group dialog
  opened: <what> '<group>' (<groups>)`, `Annotation Decimals dialog
  opened: <what> <decimals|null>`, `Annotation Tolerance dialog opened:
  <what> <tolerance|null>`).

Tests: `app/tests/measurement_drawing.cpp` (the query's entries to what is
drawn, values in words, the screen-facing layouts and their scale, the
layouts in a plane — the text's frame face on, mirrored and from behind,
faded edge on —, clicks on the text and the line, a drag into
millimetres, the dragged shape, a note, a datum label's box and
triangle, a frame's cells on a leader and under a dimension, not dragged
there, painting, a render camera's projection); `tools/ui-annotations-test.sh` and
`tools/ui-windows-annotations-test.ps1` (`app.ui-windows-annotations`:
two kept, a ± tolerance from the editor opened by a double-click in the
view, the text and then the dimension line dragged, a broken one marked
in the window, an H7 fit from the window, the kind filter and the
search, both rows recoloured — kept selected through the refresh —,
given two decimals and a group together, one put in another group, a
group hidden and shown, a snapshot of one group, saved and opened again,
limits typed as sizes read back exactly, a colour changed alone; a datum
label and a frame from the Measure panel, listed and filtered, a row
highlighting its pick, a frame added under a dimension and edited, a
datum label's pick and a frame's plane picked again in the view (a
click on nothing waiting on, Esc cancelling), saved; a hole callout on a
plate's counterbored holes from the Measure panel's Hole Callout,
written from the hole's feature in the bundled lettering, saved
(mitcad#902); the Windows test without the broken one, the filters, the
line drag and the picks again).
`tools/ui-annotation-colour-test.sh` (mitcad#764): Edit Annotation's
Colour, a named one and Custom… (Auto, Black, Dark Grey and Custom…
offered, a stored colour kept, mitcad#902), for an annotation whose picks are in an
occurrence and a root one, with the Measure panel closed and open (the
window from its Annotations…): the view and the window show it at once,
it stays as the panel closes, Undo and Redo, saved; both stored in the
old default orange and drawn as Auto, and the window's Text Height…
setting 5 mm, saved (mitcad#894). `app/tests/measurement_drawing.cpp`
also checks the look of mitcad#894: the text's direction horizontal,
vertical and half a degree off vertical from the front, from below and
from behind (bottom to top or left to right, never mirrored), no box
painted behind a value, the arrowhead's 30 degrees and its length of 0.8
text heights, the model's text height on screen and its floor, Auto's
colour on a light and a dark background; and the reference look of
mitcad#902: no band and no box under a selected or hovered one, lines on
whole-pixel centres, the text between its floor and its ceiling at a
large and a small zoom, arrowheads and a centre mark in the plane, the
extension lines from the model's points, frames two text heights high,
the bundled lettering found, a leader's dot, a hole callout's lines and a
frame under it (the first line level with the landing, flush left with
the frame), a picture's own background tone, Auto's ink piece by piece
along a line over both halves of a picture and one ink for a text over
them. `tools/ui-annotations-look.sh <dir>`
(not a recurring test) models a part like the reference drawing and
saves the screen with nothing and with a dimension selected on the white
and the dark background, each with the 3D view cut out of the same
picture (`tools/xwd2png.py` with a rectangle), for a look against it.

## Sketch mode

Create Sketch (`MainWindow::startSketch`) sketches on the selected plane or
planar face; with none, it waits for a pick (`Mode::PickPlane`). It shows
the origin planes for the pick in every design, whatever the Origin
folder's light bulb says (mitcad#844, replacing mitcad#294's rule that
left them out of designs with components): a view state for the pick only
(`inputActivated` with construction geometry), the light bulb untouched,
so after the pick the origin is as the bulb says. The planes come behind
the model's faces for picking (`OcctViewer::setPlanesBehind` and
`setPlanesByDepth`, as a placement input's do, mitcad#656, mitcad#762):
a click where a face lies in front of a plane, or where a plane runs
through a body, picks the face, also another component's while a
component is active; a click on a plane where no face is picks the
plane. A click on a face the pick refuses (not planar, or left out by the
design policy, which the view's filter does not take) is refused with its
reason also where a plane lies behind it, never picked through it
(`OcctViewer::untestedClickItem`). `updateModeBanner` puts the pick order
back when the pick ends.
The browser's XY, XZ and YZ rows pick the plane too, greyed or not.
Light bulbs work during the pick (mitcad#295): hiding a body lets the
faces behind it be picked, and the pick places are logged again. Logs:
`Create Sketch: origin planes shown`. A face or plane of a component
other than the active one asks first whether to activate that component
(mitcad#960, [Activating the picked component](#activating-the-picked-component)).

The new sketch opens upright (mitcad#692): `MainWindow::createSketch`
sends `sketch.create` with `upright`, and the model stores the own frame
whose y axis is the design's +Z projected onto the plane (+Y on a
horizontal plane), keeping the plane's normal (commands.md, "An upright
sketch frame"). Entering the sketch looks along its frame's normal with
its y axis up (`OcctViewer::lookAtSketchPlane`), so the screen's
horizontal is the sketch's x and Horizontal, Vertical, the grid and the
dimensions stay level. Editing a sketch again, Look At on it and the
axes indicator use the stored frame, so existing and imported sketches
keep theirs. `tools/ui-sketch-view-test.sh` checks XZ, YZ and a face
facing -Y (`Sketch frame in the design`, `Sketch view`).

The sketch draws its own origin and its x and y axes (mitcad#254,
`SketchOverlay::paintAxes`), whatever the Origin folder's light bulb
says (mitcad#739): they are the sketch's reference, not the design's
origin datums. The design's origin keeps its own visibility (hidden in a
new design, `set_origin_visible`): Create Sketch shows the origin planes
only for the pick, and entering the sketch hides them again unless the
light bulb is on; the light bulb is never switched, so nothing is
restored after the sketch. The sketch's origin is snapped to
(`Snap::Kind::Origin`, the fixed origin point made at the first snap),
its axes are snapped onto (mitcad#948, "Snapping onto the axes" below),
and its origin and axes are picks of Sketch Dimension and the constraint
tools (mitcad#500); they are drawn by the overlay, not shown in the 3D
view, so no body pick can take them. Logs: `Sketch axes drawn: …` and
`Design origin hidden in the view; the Origin folder's light bulb off`;
`tools/ui-sketch-origin-test.sh` and `app.ui-windows-sketch-origin`
check XY, XZ and YZ in a new design, an edited sketch and the snap.

The turn to the sketch plane centres the view (mitcad#738). A fit leaves
the camera's centre off what the middle of the free area between the
floating cards shows (`fitToFreeArea`), so a turn about that centre took
the origin off the middle, in a direction of each plane's own.
`lookAtSketchPlane` turns about the point to show in the middle instead:
- Entering a sketch (Create Sketch, Edit Sketch) shows the sketch's
  origin there while the view shows nothing (no bodies, no sketches), or
  shows only sketches and the origin is inside the free area before the
  turn.
- Otherwise, and always for Look At, the view keeps its middle: what the
  middle of the free area showed, projected onto the sketch plane. A pan
  to a far part of a sketch stays, and a sketch on a face keeps that face
  in view, also when the sketch's origin is a corner of the face.
- Entering a sketch on a face seen at a steep angle (mitcad#772): the
  middle of the free area, at the camera's depth, projected along the
  normal can lie far from the face in its plane. When that point is on
  none of the faces the sketch lies on (`MainWindow::sketchFace`, placed
  where the plane's link or the sketch's occurrence shows it), the middle
  of their box goes to the middle of the free area instead. Look At keeps
  the view's middle as above.

The point goes to the middle of the free area without a zoom. Entering a
sketch turns the view after the cards are where sketch mode puts them
(`MainWindow::enterSketch` lays out the Floating layout's cards at once,
`FloatingLayout::layoutNow`, the command card showing the palette), so
the view does not move again once the sketch has started. Logged
`Sketch plane centred on the origin: origin at x,y; free area centre at
x,y` (`on the view's middle`, `on the sketch's face`), in window
coordinates; `tools/ui-sketch-view-test.sh` and
`tools/ui-windows-sketch-view-test.ps1` check the origin of sketches on
XY, XZ and YZ after New Design, and a sketch on a block's top face edited
from the front view, the face edge-on, its middle then in the middle of
the free area.

While it waits for the plane (mitcad#474), a banner at the top of the 3D
view says so, `Create Sketch: pick a plane or a planar face. Esc
cancels.` (`view/ModeBanner`, a child of the view itself, so the Docked
and the Floating layout and the native macOS window show it alike, below
the cards floating over the view's top); hints of the status line do not
replace it, and the MCP's `ui_state` reports it as its `hint` (the status
line beside it as `status_line`). A click on a face that cannot take the
sketch, which the pick's filter neither highlights nor picks, says why: a
face that is not planar (`Create Sketch: <face> is not planar (a
cylindrical surface); pick a planar face or a plane, or make one with
Construct.`: cylindrical, conical, spherical, toroidal, a mesh's triangles
or a free-form surface, such as a thread's) or one the design policy
leaves out (its reason); the mode goes on (`OcctViewer::takeRefusedClick`:
what the click hit of the faces and planes when it picked nothing). A
click where an accepted plane lies behind such a face picks that plane, as
its hover shows. Actions blocked by the pick (the browser's, the
appearance and render dialogs', imports) say `Create Sketch is waiting
for a plane: pick one or press Esc.` instead of `Finish the command or the
sketch first.` (`DocumentHost::changeBlockedReason`). Logs: `View banner:
<text>`, `View banner at x,y WxH`, `View banner hidden`, `Create Sketch:
refused pick <face>: <reason>`; with `MITCAD_LOG_PICKS` the faces a click
hits but cannot sketch on, `Pick refused face <body>/<face> [in
<occurrence>] at x,y`. Test: `tools/ui-sketch-plane-pick-test.sh`.

Finish Sketch
(Ctrl+Enter) leaves it. Editing a
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
the solver (`sketch.drag` per move, merged into one step). A press on one
of several selected curves and points drags the whole selection together
(mitcad#837): each move is one `sketch.drag` with `entities` (the selected
points and curves of the sketch) and `by`, the offset that brings the
grabbed point to the cursor; a press on an unselected one drags only it,
and the press does not change the selection
(`SketchController::selectionDraggedWith`).

### A sketch tool

A drawing or picking tool is a `sketch::SketchTool` (`sketch/SketchTool.hpp`)
made by a factory and started by a `Tool` command:

```cpp
add({"sketch.line", QObject::tr("Line"), "line", "CREATE/Lines",
     [&c] { c.startTool(sketch::lineTool(c)); }, QKeySequence(Qt::Key_L), true, tooltip});
```

The sketch CREATE group's subgroups (mitcad#222), in this order: Lines
(Line), Rectangles (default 2-Point Rectangle; also the rounded ones, below), Circles (default Center
Diameter Circle), Arcs (default 3-Point Arc), Other Geometry (Ellipse, the
polygons, the slots, the splines, Point; no default), Text, Patterns
(Rectangular Pattern, Circular Pattern, Mirror; no default), References
(Project, Intersect, Clean Up Projections, Clean Up Stale Projections);
Sketch Dimension follows on its own. Offset, Edit Pattern, Edit
Offset and Edit Text are in MODIFY. The toolbar row has Line, the three
split buttons and Sketch Dimension (`tools/ui-sketch-create-menu-test.sh`).

Closed splines and tangents at an end (mitcad#783): Fit Point Spline
(`SplineTool` in `DrawTools.cpp`) closes on a click on its first point
after three or more (within 6 pixels, or the same existing point it
snapped to): `sketch.spline` with `closed`, previewed by
`closedFitSplinePreview` (the periodic chord-length cubic the model
builds) with a ring on the first point while the pointer is there, logged
`Fit Point Spline: a click here closes it`. `SketchModel` reads a
`fitted_spline`'s `closed` into `CurveData::closed` and gives a closed one
no `start` or `end`; Trim's preview wraps a closed spline over its
parameter range. Tangent and Curvature (`ConstraintTool` in
`EditTools.cpp`) keep where each curve was clicked and send `at`: the end
the two curves share nearest the clicks, else a spline's end nearest its
click (`tangentPoint`); while picking, the constraint's glyph is drawn at that end
(`ToolPreview::glyphAt`; with only a spline picked, its end nearest the
click), logged `Tangent would apply at p2` when it changes; a spline whose two ends are one point may be
picked twice, `at` that point. A constraint from a selection sends `at`
only for such a spline. `SketchOverlay` draws the glyph of a tangent or
curvature with `at` beside that point. `tools/ui-sketch-spline-test.sh` and
`tools/ui-windows-sketch-spline-test.ps1` (ctest
`app.ui-windows-sketch-spline`) draw both.

Insert Point on a spline (mitcad#836; commands.md, "Insert Point on a
spline"): the sketch's context menu offers **Insert Point** when the one
item selected is a spline (fitted or by control points, not projected,
not an offset's) and the right click was on it
(`SketchController::splineAt`); a double-click on a spline that the first
click selected does the same where it was double-clicked (after the
edit-panel double-clicks of texts, patterns and offsets).
`SketchController::insertSplinePoint` sends `sketch.insert_spline_point`
with the place on the curve nearest the click, one `SketchOp`: one undo
step, `Insert Spline Point in <sketch>`. A fitted spline gets a fit point
between its neighbours; a spline by control points a control point by
knot insertion, its shape unchanged until the new point is dragged. A
refusal shows `Insert Point: <reason>`. Logged `Inserted point p12 into
c3 at 20.000,5.000` and, for the double-click, `Double-click on c3:
Insert Point`. No UI test script of its own; the model tests are
`core/model-tests/sketch/src/sketch_spline_point_836_tests.rs`.

Rounded rectangles (mitcad#867): the Rectangles submenu ends with
**2-Point Rounded Rectangle** (`sketch.rectangle_rounded`, icon
`rectangle-rounded`) and **Center Rounded Rectangle**
(`sketch.rectangle_rounded_center`, `rectangle-rounded-center`), no
shortcuts: `roundedRectangleTool` (`RectangleTool` in `DrawTools.cpp`
with `rounded`). After the first click the fields are Width, Height and
Corner Radius (Tab in that order); the radius field is a radius dimension
at the second corner's arc (`fieldRadius`, the placement of the other
drawing values). Until one is typed the radius is the last one this tool
used, at first 2 mm (Sketch Fillet's default), shown as its live value.
The preview draws the four arcs, or the sharp rectangle while the radius
is half the shorter side or more. The click sends `sketch.rectangle` with
`radius` (the typed expression, else the number): four lines and four
tangent arcs, Equal on the arcs and one radius dimension; the model
refuses a radius of half the shorter side or more with the reason
(`2-Point Rounded Rectangle not added: the corner radius 12 mm must be
less than half the shorter side (10 mm)`). The corners stay as points on
the lines, so typed Width and Height become distances between them
(`pointNear` on the corners), and snapped corners are constrained as for
a rectangle. One `SketchOp`, logged `Added rounded rectangle 40 x 20 mm,
corner radius 5 mm at (0, 0) [c1, …]`. No UI test script of its own
(the menu entries are in `tools/ui-sketch-create-menu-test.sh`); the
model tests are
`core/model-tests/sketch/src/sketch_rounded_rectangle_867_tests.rs`.

- `move(snap)`, `press(snap)`, `release`, `doubleClick` get the cursor
  snapped (`SketchSnap`: points, origin, midpoints, centres, quadrants,
  intersections, onto curves, onto the sketch's x and y axes, the grid;
  with `bodySnaps()` also body vertices and edge midpoints in the plane,
  below; tolerances in pixels).
  Picking tools return `snaps() == false` and use
  `controller.entityAt(screen)`.
- The pointer is the precision cursor (`framework/Cursors.hpp`): a cross
  with the tool's icon as a badge (`controller.setToolIcon`, given by the
  registration), a square in its gap while snapped to a point.
  `OcctViewer::setSketchInput(true, icon)` sets it, `setSketchSnapping`
  follows the snap; built for the screen's device pixel ratio (again on
  `QEvent::DevicePixelRatioChange`) and the system's pointer size, cached.
- `paint(ToolPreview&)` gives the rubber band in sketch coordinates (with
  `glyph` for an inferred constraint, `dimension` for one being placed).
- `controller.setFields({{"length", tr("Length"), Kind::Length}, ...})`
  shows value boxes beside the geometry they measure (Tab moves, Enter calls `confirm()`);
  `setLive` shows the cursor's value while nothing is typed;
  `fieldValue`/`fieldExpression` give a typed one (evaluated by the
  model's `evaluate` query).
  Drawing tools provide `ToolPreview::fieldDimensions`, each with its
  field id, dimension definition and temporary preview geometry.
  `SketchController::placeFields` puts the editor in the dimension line,
  and `SketchOverlay` draws its extension lines, arrows and angle arc
  through the same `dimensionShape` renderer as placed dimensions.
  Rectangle width is above its horizontal side and height beside its
  vertical side; other lengths follow their side or axis, radius and
  diameter values follow their circle, and angular values sit on their
  sector (including reflex angles). Sides and Text retain visible labels.
  Degenerate geometry also falls back to a labelled editor until there
  is a dimension to draw. Fields stay inside the view and avoid one
  another, including while typed values lock the preview geometry.
  `tools/ui-macos-sketch-drawing-dimensions-test.sh` checks actual preview
  measurements, graphics, editor separation, labels, locking and Tab in
  Floating and Docked layouts; its screenshots support visual review.
  The tool's next click goes where the pointer is (mitcad#671): a field
  whose place would cover the pointer, with 12 px to spare, moves to the
  nearest free place, preferring the side away from the pointer (the
  dimension graphics follow the field). A field the user has not typed
  into nor reached with Tab passes the left button and the pointer's moves
  over it (also on its fx button) to the view (`fieldMouse`), so a field
  placed for the pointer's last place cannot take a click meant for the
  view; the right button keeps its menu. Section 6 of
  `tools/ui-sketch-tools-test.sh` clicks where the fields are.
- `confirm()` (Enter) and `cancel()` (Esc: one step back; false at the
  start ends the tool). Esc in the value editor of the Dimension tool also
  steps the tool back; in an editor opened from the context menu (Edit
  Dimension) it only closes the editor and a running tool keeps its state
  (mitcad#354). `name = value` in the Dimension tool's editor adds the
  dimension with that parameter name and closes the editor (mitcad#352);
  when editing an existing dimension the editor stays open if the model
  refuses the name.
- `SketchOp op(controller)` is one undo step of model commands:
  `op.run(cmd)` (refusals keep the model's reason for `refuse`),
  `op.pointInput(snap)` (an existing point where it snapped, the origin's
  fixed point made when needed, else `[x, y]`), `op.constrain(point,
  snap)` (coincident, midpoint, on-curve, intersection constraints for a
  new point), `op.addConstraint`, `op.addDimension(def, typed)` (refusals
  only logged), `op.commit()` (merges the steps, refreshes) or
  `op.rollback()`.
- Typed values become dimensions; snapped points get constraints (every
  drawing tool: a point that snapped onto geometry is held there, mitcad#353;
  the Point tool on the origin makes the sketch's one fixed origin point and
  no constraint, mitcad#355); an Ellipse whose rim click is farther than its
  major end turns its axes, and a typed major radius then dimensions the
  minor one (mitcad#356); the
  line tool infers horizontal, vertical, perpendicular, parallel (to the
  line last passed over) and tangent within a few pixels.
- **Snapping onto the axes** (mitcad#948): every tool that snaps gives the
  snapper the sketch's x and y axes (`SnapSettings::axes`) as lines
  across the view, offered as `Snap::Kind::OnCurve` with `Snap::axis`
  ("x", "y") and the curves' tolerance (6 px), 1 px further than they
  are, so a sketch curve along an axis, points, the origin, midpoints,
  quadrants and intersections win ties; the snap marker is the on-curve
  circle. Along the axis the point keeps to the grid while grid snapping
  is on. `Snap::curve` is the axis's line where the model made it, else
  its pick id (`x_axis`); hovering makes nothing. A point placed there is
  held on the axis: `op.constrain(point, snap)` makes the axis's entity
  with `op.axisInput` (`sketch.axis`, reused within the action; logged
  `Made the sketch y axis for a pick: c5`) and adds Coincident (point on
  the axis line, `axisHold`), logged `Coincident p3 on the sketch y axis
  c5`, all in the action's undo step. The line tool's horizontal and
  vertical inference is the parallel and perpendicular to the axes (the
  directions are the same, so an axis needs no reference of its own);
  for a line along an axis both its ends lie on (`axesAt`: the origin,
  an axis snap, a point held on an axis by Coincident,
  `SketchModel::pointAxes`) it adds only one of the two equivalent
  constraints: the inferred direction, with an end that snapped onto the
  axis left unheld, or no direction at a point already on the axis
  (logged `Line c9 lies on the sketch y axis: no vertical constraint`).
  Drags, profiles, Delete and Copy leave the axes out as before. Unit
  test: `axisSnapTests` in `app/tests/sketch_snap.cpp`; section 8 of
  `tools/ui-sketch-axes-test.sh` and of
  `tools/ui-windows-sketch-axes-test.ps1` draws a circle with its centre
  on the x axis.
- The first dimension scales the sketch (mitcad#860; commands.md,
  "Scaling a sketch"): while **Preferences › Sketch › Scale the sketch to
  its first dimension** is on (the default; `sketch/SketchPreferences.hpp`,
  setting `sketch/scaleToFirstDimension`), `SketchOp::run` sends every
  `sketch.add_dimension` with `"scale_sketch": true`, the Sketch
  Dimension tool's and a drawing tool's typed value's alike. The model
  applies the rule: the first driving length, distance, radius or
  diameter of a sketch without other dimensions, offsets, rectangular
  patterns, projected geometry or fixed points other than the origin
  scales the whole sketch about its origin so that the dimension holds,
  in the same undo step; a drawing tool's typed value, drawn at that
  size, scales nothing. When the answer has `scaled`, the action logs
  `Scaled the sketch by 10 to its first dimension` and its closing notice
  says `The sketch was scaled by 10 to its first dimension`. The UI test
  libraries set `MITCAD_SKETCH_FIRST_DIMENSION_SCALE=0`, which makes the
  unsaved preference off, so that their drawn sketches keep their sizes.

Body edges and vertices in the sketch plane (mitcad#256,
`sketch/BodyGeometry.hpp`): `controller.bodyGeometry()` reads the model's
`plane_geometry` query (commands.md, "Body geometry in a sketch's plane")
once per tool session and again after the document changed (its
revision), logging `Sketch plane geometry: 4 edge(s), 4 vertex(es)`.
`bodyGeometry(true)`, the Dimension tool's (mitcad#866), adds every other
edge and vertex of the visible bodies before the sketch, as the curve or
point it projects to along the plane's normal (an edge along the normal as
its point, type `point`), with `inPlane` false, its `height` and its
`path` (where it is, above or below the plane): the same query with
`out_of_plane`, asked once while the document stays as it is (not per tool
session, as the model then lists every edge of the bodies) and logged
`Sketch body geometry out of the plane: 12 item(s)`; the counts above stay
those in the plane. The Point tool snaps to the items in the plane only.
The
items have ids of their own that never clash with the sketch's: `body:3`
an edge or a vertex, `body:3:start`, `body:3:end` and `body:3:center` a
curve's points. Nothing is projected while a tool picks; the action that
makes something from an item projects it then with
`op.projectBody(item)` (`sketch.project`, `linked: true`, the item's
`source`, `body` and `occurrence`), or takes the entities of an existing
linked projection of the same source, body and occurrence that is not
stale and still lies where the item is (`existingProjection`; logged
`Projected the body edge E{…} of F2.b0: c9` or `Reused the projection of
the body edge E{…} of F2.b0: c9`); the projection is part of the action's
one undo step, named after what the action makes.

- **Sketch Dimension** (`DimensionTool`) takes any body edge or vertex,
  in the sketch plane or out of it (mitcad#866, GitHub #50): one out of
  the plane is projected onto the plane along its normal and linked when
  the dimension is added, as one in the plane is, and is dimensioned as
  what it projects to: a circle edge as its projected circle, a line as
  its line, a vertex as its point, and an edge along the normal (seen end
  on: a cylinder's seam, a vertical edge) as its projected point. The tool
  picks with
  the current selection first (mitcad#660): one line, circle or arc, or
  two supported sketch entities, go straight to placing the value after
  **D**, without picking again. Its usual cursor-dependent choice of a
  length, distance or angle and its value editor apply unchanged. An
  unsupported selection is cleared with the existing hint; no selection
  starts picking as usual, and three or more spacing anchors still open
  Equal Spacing Dimension. Pending viewport picks are delivered before
  reading the selection. Interactive picking uses
  `entityAt(screen, PickAll | PickBody)`: body vertices and the centres of
  body circles and arcs as points, body edges as curves, with the same
  pixel tolerances as sketch entities, which win ties (a body item counts
  one pixel further than it is). An item out of the plane is picked where
  the view shows it (its `path` in the view), not where it projects to, and
  only where it is in sight: of those near enough to win, nearest first,
  the first whose point under the pointer `OcctViewer::inSight` finds in
  front of the shown bodies' faces (the test a click on a body edge makes;
  at most four such tests per move), so that an edge behind a face is not
  taken. It counts one and a half pixels further than it is, so that what
  lies in the plane at the same place wins (seen from above, an edge below
  a top edge). Their places in the view are made once and again only when
  the items or the camera change (`SketchController::bodyScreens`); a move
  then costs a bounding-box test per item, a distance to the path of the
  few near the pointer, and those sight tests. Hover and the picks are highlighted as
  sketch entities are (`SketchOverlay::paintEntityHighlight` looks body
  ids up in `bodyGeometryShown()`): what the pick projects to, the curve
  or point in the sketch plane, and for an item out of the plane also the
  item itself, thin and dashed. `decide()` and the preview work on the
  tool's view of the sketch, the sketch with the items added as the fixed
  entities they project to (`BodyGeometry::addTo`; `ToolPreview::model`),
  so the dimension types follow as for sketch geometry. A body pick is
  logged `Sketch Dimension picked the body edge E{…} of F2.b0 (body:3)`,
  one out of the plane with `, out of the plane: projected as a line`.
  Selected body edges and vertices, in the plane or out of it, are matched
  against the same list by body, topological source and occurrence link
  before the tool starts placement, and used; an unmatched one (a hidden
  body, a body after the sketch, an edge without a name) is refused with
  `Sketch Dimension: the body edge cannot be projected onto this sketch:
  it is not on a visible body before the sketch, where the sketch is
  edited.`
  Own-component body selections must also belong to the occurrence being
  edited, rather than another placement of that same component definition.
  When the value is entered, one `SketchOp` projects each body pick
  (once each) and adds the dimension on the projected entities (a line's
  end by where the edge's is; where the projection joined a curve or point
  the sketch has projected before from another source, an edge right below
  a projected one, that one: `projectedLike`); a dimension between projected (fixed)
  entities only is added driven, as any over-constraining one. Esc,
  another tool or leaving the sketch before that changes nothing. The
  geometric constraint tools take no body edges or vertices, in the plane
  or out of it (`ConstraintTool::entityAt` picks the sketch's entities and
  axes; `constrainSelection` takes the sketch's): a body edge is projected
  first (Project, **P**) and its projection constrained, so the rules of
  the two kinds of tools do not differ.
- An angle between two lines (mitcad#296) measures the sector of the four
  the lines' extensions make that the cursor is in: `decide()` writes
  `reverse_a`/`reverse_b` from `cursor - corner = s·da + t·db` (a ray
  reversed where its factor is negative), so the preview's arc, arrows and
  value follow the cursor and switch to the supplement across a line;
  while the value is typed, `decide()` keeps the placing click's sector.
  The overlay draws the arc, the extension lines and the default value
  place between the rays `angleRays()` gives (`SketchOverlay.hpp`); a
  dimension stored without the fields gets the sector whose angle is its
  value, of the two the one nearer its text. Moving the value afterwards
  only moves the text. After adding, the tool logs `Angle dimension k5:
  60.00 deg, the arc from 0.0 to 60.0 deg about 0.00,0.00` (the rays'
  directions in degrees, 0 to 360) for `tools/ui-sketch-angle-test.sh`.
- Signed horizontal and vertical distances (mitcad#409; commands.md,
  "Signed point dimensions"): with the palette's **Signed distances**
  (`paletteSignedDistances`, `SketchController::signedDistances()`, not
  saved) checked, `decide()` adds `"signed": true` to a
  `horizontal_distance` or `vertical_distance`, so the preview and the
  value editor show the offset `b - a` from the first pick (`measure()`
  keeps the sign of a signed one). Without it, a value the model refuses
  as negative for a new horizontal or vertical distance is added again
  signed in the same `SketchOp` (the point goes to that side). The
  overlay's text of a signed one has its sign, `+` too (`+25.98`,
  `fx: -15.00`). The sketch's context menu on a selected horizontal or
  vertical distance offers **Make Distance Signed** or **Make Distance
  Unsigned** (`SketchController::setDimensionSigned`, `sketch.set_signed`:
  the point stays, a value on the negative side is negated). Logged:
  `Added dimension k5 horizontal_distance signed d3 = r * cos(a)`,
  `Dimension k5 shows fx: +25.98`, `Signed distances on`, `Dimension k6
  unsigned: fx: 25.98 (-(r * sin(a)))`, and `Dimension texts: …` as a
  sketch with dimensions opens; `tools/ui-sketch-signed-test.sh` steps `a`
  through the four quadrants.
- **Inactive constraints and dimensions** (mitcad#721; commands.md,
  "Inactive constraints and dimensions"): the overlay draws an inactive
  constraint's symbol pink with a dashed frame (a symbol of its own beside
  an active one of the same type on the same entity) and an inactive
  dimension pink, dashed, its value on pink (`kInactiveColor`, apart from
  the conflict red and the driven grey). Its tooltip starts with `Not
  applied: <inactive_reason>` (`SketchOverlay::inactiveTooltip`; a
  constraint's tooltip is only that). The sketch's context menu offers
  **Activate** for the selected inactive constraints and dimensions, else
  **Deactivate** for the selected active ones
  (`SketchController::setItemsActive`, `sketch.set_active`); a refused
  activation shows the model's reason and changes nothing; **Delete**
  removes them as any other. Logged: `Sketch inactive drawn: k1
  constraint, k4 dimension` (`none`) when it changes, `Constraint k1 glyph
  at x,y` for each inactive constraint with the dimensions' places (with
  `MITCAD_LOG_PICKS`, which the UI tests set, for every constraint, to
  click an active one too; mitcad#773),
  `Constraint tooltip k1: Not applied: …`, `Sketch items activated: k1`,
  `Sketch items not activated: k1 cannot be activated: …`;
  `tools/ui-sketch-inactive-test.sh` and
  `tools/ui-windows-sketch-inactive-test.ps1`.
- The picks are geometry the model gives, not the view's shapes: Hide
  Above Sketch, which cuts what the view draws, leaves the edges in the
  plane pickable.
- **The sketch's axes and origin as picks** (mitcad#500;
  `sketch/SketchAxes.cpp`; commands.md, "The sketch's axes and origin as
  entities"): Sketch Dimension and the constraint tools (Fix/Unfix
  excepted) pick with `PickAxes` too. `entityAt` then offers the origin
  within a point's tolerance and the x and y axes within a curve's, as
  the infinite lines drawn across the view, each counted two pixels
  further than it is, so the sketch's and the bodies' own entities win.
  It gives the entity the model made for it (the fixed origin point, an
  axis's line flagged `axis`) or, before that, a pick id: `origin`,
  `x_axis`, `y_axis`. The tools decide on a view of the sketch with
  those added as the fixed point and construction lines the model would
  make (`SketchModel::addAxisPicks`), so an axis is a line like any
  other: Sketch Dimension gives a point's `point_line_distance` from it,
  a line's `angle` to it (`line_distance` when parallel), and the origin
  with a point the horizontal, vertical or aligned distance; an axis
  alone, or two of the axes and the origin together, give nothing. The
  constraint tools take at most one of them per constraint: an axis in
  Coincident (a point on it), Parallel, Perpendicular, Collinear, Tangent
  and as Symmetric's line, the origin wherever a point goes (Coincident,
  Horizontal/Vertical with another point, Midpoint, Symmetric). Hover and
  picks are highlighted as the axis across the view or the origin's dot.
  The action's `SketchOp` makes a pick's entity first with
  `op.axisInput(pick)` (`sketch.axis`, one each, reused within the
  action), logged `Made the sketch x axis for a pick: c3` (`… origin …`),
  so undo takes it back with the dimension or constraint. The axis lines
  and their far ends are never drawn as geometry (`sketchEntities`
  leaves entities with `axis` out) or dragged, and are snapped onto only
  as the axes across the view (mitcad#948, "Snapping onto the axes"
  above); they are
  picked by the tools above, by a command panel's input that takes them
  (Mirror's Mirror Line, mitcad#876; "Sketch commands with panels"
  below) and selected with no tool running (below); a
  dimension or a constraint's glyph on one is drawn at the axis near the
  origin, and its extension lines take the axis as infinite.
  `tools/ui-sketch-axes-test.sh` and
  `tools/ui-windows-sketch-axes-test.ps1` dimension a point 30 mm from
  the x axis, give a line 25° to it and put a point on the y axis in a
  new sketch, mirror a circle about the y axis (mitcad#876), and select a
  point and the y axis first, then Coincident, and the point and the x
  axis, then Sketch Dimension (mitcad#899).
- **Selecting the axes and origin first** (mitcad#899): with no tool
  running, the x axis, the y axis and the origin hover and select like
  construction geometry (`SketchController::axisPickEvent` with
  `selecting`; the view does not show them, so sketch mode takes the
  press): `axisSelectionAt` asks `entityAt` with `PickAll | PickAxes`,
  so a dimension's or constraint's symbol and the sketch's points and
  curves win ties, and then leaves the click to the view wherever the
  view picks a precise target itself (`OcctViewer::claimsClick`: a text,
  a body edge or vertex in sight, or one behind faces, which the
  selection filter's rules and Shift decide on; a face or a profile under
  the axis does not count). Otherwise it gives the pick id
  (`SketchModel::axisSelectionPick`: `x_axis`, `y_axis`, also where the
  model made the line; `origin` before a step made its point, after which
  the origin is a point of the sketch the view picks). Hover and the
  selection are drawn as the tools draw them: the axis across the view,
  the origin's dot. The click logs `Sketch axis selected: y_axis (y axis)`
  or `Sketch origin selected: origin` and goes to the selection as a
  sketch curve of geometry `line` (an axis) or a sketch point (the
  origin) with the pick id (`axisSelectionItem`); Ctrl adds and removes
  as with other picks, the selection filter counts them as construction
  geometry, and a drag from one is the view's (a window), never a move.
  Selecting makes nothing and changes no undo history: the pick id is
  turned into the model's entity (`SketchModel::axisEntity`, the entity
  there is) or made with `sketch.axis` in the step that uses it, as for
  the tools' picks. The constraint tools (Fix/Unfix excepted) take such a
  selection through `takeSelection` and `constrainSelection`, decided on
  the sketch with the picks added (`addAxisPicks`) and with the "one of
  the origin and axes per constraint" rule; Sketch Dimension's
  preselection takes it (a point and an axis: the point's distance; a
  line and an axis: the angle; the origin and a point: the distances);
  Mirror's Mirror Line takes a selected axis (the command's preselection
  gives an axis pick only to an input with `sketchAxes`, so Objects and
  other commands' inputs leave it out, `isAxisSelection`). Delete,
  Construction, Convert to Sketch Geometry, Fix/Unfix, Copy and Cut leave
  the axes and origin out.
- **Snapping to bodies** (mitcad#258): a tool whose `bodySnaps()` is true
  gives the snapper the body geometry (`SnapSettings::body`), which then
  offers body vertices as `Snap::Kind::BodyVertex` (with the points' rank
  and tolerance) and the middles of body lines and arcs as
  `Snap::Kind::BodyMidpoint` (the midpoints'), the item in
  `Snap::bodyItem`; the sketch's own point or midpoint at the same place
  wins (a body target counts one pixel further). The marker is a point's
  square or a midpoint's triangle, the cursor shows the snap. Only the
  **Point** tool turns it on now; the other drawing tools can opt in
  without changes of their own, as `op.pointInput(snap)` gives a body
  vertex's projected point and `op.constrain(point, snap)` holds a point
  on the projected vertex (coincident) or at the projected edge's middle
  (midpoint), projecting the vertex or edge first (`projectBody`).
- **Point** (`PointTool`): on a body target its hint says what a click
  does; the click is one `SketchOp`: the vertex or the edge projected
  (linked, or an existing projection taken), `sketch.add_point`, and the
  coincident or midpoint constraint. Logged `Point p9 coincident with p8,
  the projected corner V{…}` or `Point p9 at the midpoint of c7, the
  projected edge E{…}` after `Added point p9 at (x, y)`. A point already
  coincident with the corner's existing projection makes the click do
  nothing, as at a point of the sketch.

### Sketch commands with panels

Offset, Mirror, the patterns, Project, sketch Fillet and Chamfer,
Move/Copy and Insert DXF into the edited sketch are feature commands with
`sketchCommand = true`: `build` returns a `sketch.*` command (or
`{"commands": [...]}`), the preview runs it on the document and undoes it
when an input changes, OK keeps it as one undo step, Cancel undoes it.
Their selection inputs take `SelectKind::SketchCurve`/`SketchPoint` of
the edited sketch (`ofSketch`, `linesOf` in `SketchCommands.cpp`; not
texts). While a panel is open, sketch mode's mouse handling pauses
(`SketchController::setPaused`), but for the sketch's axes while the
active input takes them (`InputDef::sketchAxes`, Mirror below); the
window is in `Mode::Command`, but the sketch is still what the view
shows (`MainWindow::sketchShown`), so Hide Above Sketch keeps the bodies
cut through the panel, its previews, OK and Cancel (mitcad#223). Drive
Joint (outside sketches) runs its `drive_joint` the same way. A sketch
command's `warnings` are the panel's message (not red) and the preview's
log line (`Preview <name>: ok, warning: ...`).

**Mirror** has Objects (curves and points of the sketch, any number) and
Mirror Line (one line). `build` leaves the line out of Objects and
refuses only when nothing else is left (`The mirror line cannot mirror
itself.`); the model does the same (mitcad#876). Mirror Line also takes
the sketch's x and y axes (mitcad#876; `InputDef::sketchAxes`): while it
is the active input, `SketchController::axisPickEvent` hovers and
picks an axis where `entityAt` with `PickAxes` gives one (the sketch's
own entities win ties), always as its pick id `x_axis` or `y_axis`, also
when the model made the line (a preview may have made it, and takes it
back before it runs again), logged `Sketch axis picked for the command:
y_axis (y axis)`. A drag that starts on an axis is the view's (a window).
`build` sends the axis by its name (`"axis": "y"`): the model makes its
line in the mirror's own undo step, or takes the one there is. The
overlay draws the picked axis as selected across the view
(`SketchHost::sketchAxisPicks`), as the view cannot. Other inputs never
get the axes. `tools/ui-sketch-axes-test.sh` and
`tools/ui-windows-sketch-axes-test.ps1` mirror a circle about the y
axis.

Offset's Chain Selection (`chainOf` in `SketchCommands.cpp`) grows the
picked curves through points where exactly two joinable curves of the
same construction kind meet. A picked curve that joins nothing while a
curve it lies on does (`chainStart`, mitcad#741: an edge an earlier
version projected again after its face, with points of its own) is
replaced by that curve, so the whole outline is offset. The model counts
the same curve listed twice once and says which it took, refuses a
selection that is no chain with the reason (a branch, two ends at one
place that are two points), and warns where an open chain stops at
another curve's end that is a point of its own (`sketch.offset`,
[commands.md](../core/model-api/src/commands.md#projected-outlines-and-their-offsets)).

**Sketch Fillet** (mitcad#867) has one selection, Lines or Corners
(`corners`: lines of the sketch, and points where exactly two lines of
one kind end, `cornerOfTwoLines`; one or more), and Radius (default 2).
`build` sends `sketch.fillet` with the picked lines as `curves` and the
points as `points`: two lines round the corner where they meet (as
before), more lines every corner where two of them end together (a whole
rectangle, also picked with a window: a Center Rectangle's construction
diagonal and its centre are no corners), a point its corner; one arc
each, one radius dimension and the other arcs Equal to the first, one
undo step. One line alone fails in the panel (`Pick two lines, a chain
of lines or corner points.`); the model's refusals (a radius that does
not fit names the line and the largest radius that does) are the panel's
error. `describe`: `Sketch fillet of c1 and c2, radius 5`, or `Sketch
fillet of c1, c2, c3, c4, radius 5`. Keywords: round corners, rounded
rectangle.

**Project** (`sketch.project`, CREATE's References, **P**, icon
`project`, keywords include, reference) is a panel of Geometry (one or
more) and Projection Link (checked: the projection follows its source).
Geometry takes edges, faces (their edges) and vertices of the bodies, and
(mitcad#904) curves and points of other sketches (lines, arcs, circles,
ellipses, splines, points, construction and projected ones too), also of
other components where an occurrence shows them, and a whole sketch
picked in the browser (all its curves); the edited sketch's own geometry
and the sketch itself are not taken (`withAccepts`). While it runs its
selection filter context has a row for other sketches' curves and points
(a cross by default: a click and a window take them, the hover shows them
as its candidates), the only place where they are selectable. `build`
sends one `sketch.project` per pick: a body item's name as `source` with
its `body`; a sketch item's reference as `source`
(`{"sketch": "F1", "curve": "c3"}`, `{"sketch": "F1", "point": "p2"}`,
`{"sketch": "F1"}`), no body; `linked` and the pick's `occurrence` for
both. A later sketch's geometry fails the preview with the model's reason
in the panel (it comes after the sketch in the timeline), OK off.
Described `Projected <n> item(s)`, with the other sketches' items named
(`Projected 1 item(s): sketch curve c3 of F1`). Tests: the selection
scope's (`app/tests/selection_scope.cpp`), the Project step of
`tools/ui-sketch-intersect-test.sh` and of
`tools/ui-windows-projections-test.ps1`.

**Intersect** (`sketch.intersect`, CREATE's References after Project,
icon `sketch-intersect`, keywords section, cut, reference, project; no
shortcut: I is Measure; mitcad#736) is such a panel: Geometry (bodies,
faces and edges, one or more) and Projection Link (checked). `build`
sends one `sketch.project` with `"section": true` per pick, its `body`,
`occurrence` and `linked` as Project's; a face or an edge has its
`source`, a body none (the model cuts every face of it, a linked section
per face, commands.md "Section curves in sketches"). An edge gives the
points where the plane crosses it. A pick the plane does not cut fails
the preview with the model's reason in the panel (red), OK off.
Described `Intersected <n> item(s)`. The sketch context menu of picked
faces and edges offers it after Project (`MainWindow.cpp`). Test:
`tools/ui-sketch-intersect-test.sh`, the last part of
`tools/ui-windows-projections-test.ps1`.

**Clean Up Projections** (`sketch.clean_up_projections`, CREATE's
References, mitcad#232) is such a panel with one input, Merge Overlapping
Lines (checked): its preview runs the command, whose warnings list what it
removes and merges, the links merged curves lose and the regions later
features use that change their names; OK keeps it as one undo step, Cancel
takes it back, and with nothing to clean up the panel says so and OK stays
off. The Sketch Palette lists the sketch's `warnings` (linked projections
that could not follow their sources) under the degrees of freedom
(`sketch::PaletteWarnings`), one label each. A warning the `sketch` query
gives entities for (`warning_entities`, mitcad#264: a projection that keeps
its geometry or lost its source, all its curves and points; else its stale
curves, else those that did not follow) looks like a link, underlined with
a pointing hand and the tooltip "Click to select the curves this is about";
a click selects them with the normal sketch selection
(`SketchController::selectEntities`, in place of the selection, so a second
click or a click on another warning replaces it, and Delete and the other
selection commands take it), and when none of them is in sight the view
pans to them, keeping its zoom unless they do not fit
(`view/ViewReveal.hpp`). Warnings without entities stay plain text.

**Clean Up Stale Projections** (`sketch.clean_up_stale_projections`,
CREATE's References, mitcad#252) is such a panel without inputs: its
preview runs the command and shows the sketch tidied (stale projected
curves that nothing uses gone, stale lines trimmed to the regions later
features use), and its warnings list the curves it removes, trims and keeps
(with why), the projections it unlinks and the regions later features use
that change their names; OK keeps it as one undo step, Cancel takes it
back, and with nothing stale the panel says so and OK stays off. While
the preview lasts the view highlights what it does: the removed curves as
they were and the pieces trimmed off, dashed in the remove colour, and
the trimmed lines in the pick colour (the answer's `stale.preview`, given
to `SketchController::setStalePreview` through the definition's
`sketchPreviewShown`, which a sketch preview calls with its answer and
with nothing when it is taken back; the panel's end clears it too). The
log says `Preview highlights: removed c8; trimmed c3, c7`, and
`Preview highlights: none` once they are gone. The Sketch Palette offers
the command under its warnings while a linked projection does not follow
(`kept`, `lost`, `unfollowed` or `stale` in the `sketch` query): a
Clean Up Stale Projections button (`paletteCleanUpStale`, a capsule in the
Floating layout), whose click starts it as the menu does.

**Convert to Sketch Geometry** (`sketch.convert_projected`, MODIFY after
Centerline, mitcad#257) is an `Action` on the selection, like
Construction: it runs `sketch.convert_projected` with the selected curves
and points of the edited sketch, one undo step, and logs `Converted to
sketch geometry: c9, p7` (and `; kept projected: p8` for shared points
that stay projected); the model's warnings go to the hint line, a refusal
to the error message. It is enabled only while the selection holds
projected geometry (`SketchController::selectionHasProjected`, its
`enabled`; `SketchController::updateConvertAction` refreshes the action on
selection and model changes, since the window refreshes the commands only
on model changes), and the sketch's context menu offers it on projected
geometry.

Points that become one (mitcad#259, mitcad#260): the Coincident tool's
picks of two curve ends merge them in the model; `applyEach` reads the
result's `merged` (`{"kept", "removed"}`), logs `Merged points p5 into p3
(coincident on p3, p5)`, uses the kept point for the remaining pairs of a
multiple selection and replaces the removed point in the selection.
Extend logs the point an end joined (`Extended c1 at (9, 0) (joined
p5)`).

A constraint that replaces one it makes redundant (mitcad#287): when
`sketch.add_constraint` answers with `implied_removed`, `SketchOp::run`
logs `Removed constraint horizontal k12: implied by the new coincident
k17` and the action ends with the hint `Horizontal removed: implied by
the new Coincident.` A constraint or dimension a snap, an inference or a
typed value adds through `SketchOp::attempt` is left out when the model
refuses it, as before, but now logs `Constraint coincident on c27, p40
not added: <reason>` and the action ends with the hint `Coincident not
added: <reason>.` These hints go through `SketchController::notice`
(logged as `Sketch notice: …`), which keeps them in front of the tool's
next prompt hint once.

The constraint tools and the selection (mitcad#865; `ConstraintTool` in
`sketch/EditTools.cpp`):

- A selection that is everything the constraint takes is applied when
  the tool is chosen (`constrainSelection`; a pairwise constraint such as
  Equal or Parallel applies the first entity with each of the others).
  A selection of fewer entities than the constraint takes becomes the
  tool's first picks, in the order selected, and the tool waits for the
  rest: Symmetry started with two circles selected takes them and asks
  for the line, so one click on a line or on the sketch's x or y axis
  adds the constraint; Symmetry with one entity selected, or Equal,
  Parallel, Tangent and the other two-pick constraints with one, likewise.
  The selection is cleared once taken and logged `Symmetry takes the
  selection as its picks: c1, c3`. A selection that does not fit (a
  point and a circle for Symmetry) is left as it is, and the tool starts
  with no picks after the notice `Symmetry does not take the selection:
  pick a second point.` A first pick from the selection has no click, so
  Tangent and Curvature with one curve selected apply where the curves
  meet, as a whole selection of two does.
- No silent refusals: a click on geometry the tool does not take shows
  why in front of the prompt (`Symmetry does not take that: the line to
  mirror about is picked last: first pick the two points or curves to
  mirror.`; logged `Symmetry pick c7 refused: …`), and the picks so far
  stay. A constraint the model refuses (over-constraining, contradicting)
  shows the error `Symmetry not added: <reason>` on the status line, for
  a single constraint as for a selection of several; the tool then
  leaves it there instead of its next prompt.
- Symmetry of two circles or arcs whose radii are fixed already (both
  dimensioned, or Equal) mirrors their positions only (the model's
  `radii: false`, commands.md "Symmetry of circles with fixed radii"),
  and an older constraint on the mirrored geometry that it makes
  redundant goes in its favour (the hint `Coincident removed: implied by
  the new Symmetry.`). Tests: the `sketch_symmetry_865_tests` model tests
  and step 5 of `tools/ui-sketch-axes-test.sh`.

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
- Edit Text's **Text Parameter** (mitcad#664): a text parameter's name or
  a text expression (`'Box ' & GridX`) the text follows, sent as `value`
  (`TextData::value`, from the `sketch` query's text `value`: the name, or
  the expression of the sketch's own parameter); clearing it sends `""`,
  and the text then keeps its string. With a parameter the **Text** field
  may be empty. Change Parameters lists text parameters with the unit
  `Text` and their text in Value (Add User Parameter offers the unit
  **Text**); their Min, Max, Step and Strict cells and slider are off, a
  bare number in their expression gets no unit, and parameter fields
  treat them as a kind of their own (`ParameterKind::Text`: a text field
  takes only texts, a number field no text).
- Edit Text's font (mitcad#473, `sketch/FontFiles.*`): the choice lists
  Droid Sans, the installed families (as the system has them before any
  font file is loaded into the application) and the font files of the
  folders of Preferences › Fonts (the setting `fonts/folders`, found
  again each time the panel starts), the files as the value
  `file:<path>` labelled `<family> (<file name>)`; each option has a
  preview ("Ag" in the font, `choiceImage`). **File...** beside it
  (`onButton`) opens a file dialog and chooses the file it gives
  (`DocumentHost::chooseInCommand`), listed then too. A file choice
  sends `font_file` instead of `font`. A text in a font file starts with
  the file where it was read, or one of the same name in the font
  folders. Right-to-left and joined scripts need nothing of the app: the
  model shapes them and orders the line (commands.md, the sketch's
  `texts`), and the text shows the outlines it gives
  (`tools/ui-text-fonts-test.sh` checks a Hebrew word's order).
- **QR Code** (`sketch.add_code`, CREATE › Text, mitcad#473) and **Edit
  QR Code** (`sketch.edit_code`) are panels like Edit Text: content,
  module size, error correction (`L`, `M`, `Q`, `H`), and for a new code
  its lower left corner. A code is a text with `code` (`TextData::code`,
  `ecc`), displayed by its outlines; a double-click on it (and its
  context menu) opens Edit QR Code instead of Edit Text, which does not
  take codes. `newestProfile` (Extrude, Emboss, …): when the newest unused
  profile is a region of a code, the command starts with all of the
  code's material regions (`letter` in the sketch query's regions), so a
  code extrudes whole. Logged `Added QR code "https://mitcad.org", module
  1, level M` and `Edited QR code t2: "…", module 0.5, level M`. Test:
  `tools/ui-qr-code-test.sh`.
- A derived offset spline (of an ellipse or spline) shows only its ends
  as points: the query marks what the offset computes `derived`, and
  `SketchModel` drops the other control points (no snapping to them).
  Dragging or moving it carries its source along at the same distance.

Equal spacing (mitcad#221, `commands/EqualSpacingCommands.cpp`;
commands.md, "Equal spacing in sketches") is a constraint with a panel:

- **Equal Spacing** (`sketch.equal_spacing`, in CONSTRAINTS and the
  palette's constraint grid) and **Equal Spacing Dimension**
  (`sketch.equal_spacing_dimension`, the same panel with its Common
  Spacing Dimension checked) build `sketch.add_equal_spacing`. Sketch
  Dimension (D) with three or more points, circles, arcs or lines selected
  (only those) opens Equal Spacing Dimension instead of its tool
  (`dimensionEqualSpacing`): the spacing field shows the mean interval
  and has the keyboard, OK adds the constraint and its one dimension as
  one undo step.
- Inputs: Objects (an ordered list, `withOrdered`: its order is the
  chain's; points, lines, circles and arcs of the sketch), Measure
  (Horizontal, Vertical, Along a Line, Point to Point; the start chooses
  Horizontal or Vertical by the objects' spread), Direction (a line, for
  Along a Line), Keep Listed Order (directional modes: otherwise the model
  sorts along the direction and refuses ties, naming them), Chain in
  Listed Order (Point to Point: needed unless every object was clicked
  into the list in the panel, one by one, as the list's `onPick` records;
  a selection made before the command or a window has no order of its
  own, so the chain's order is confirmed explicitly), Common Spacing
  Dimension and Spacing.
- The preview runs the command, so the model's refusal (too few or
  unsupported objects, a tie, a pattern's copies, redundant or
  conflicting) is the panel's message before OK. `build` gives the
  objects to `SketchController::setSpacingPreview`; the overlay draws the
  previewed chain dashed, numbered 1, 2, 3 in the order the model keeps
  (`SketchOverlay::paintChain`), and the same for a selected or hovered
  equal spacing. Each anchor has the constraint's glyph (three ticks on a
  line); the `spacing` dimension is drawn on the first interval.
- **Edit Equal Spacing** (`sketch.edit_equal_spacing`) takes the
  constraint's glyph or one of its objects (also by a double-click on the
  selected glyph, and in the sketch's context menu when the glyph is
  selected or right-clicked: a right click on a glyph or a dimension
  selects it, as on geometry), fills the panel from the stored order (Keep Listed
  Order checked), and builds `sketch.edit_equal_spacing` with
  `sketch.add_dimension`, `sketch.set_dimension` or `sketch.remove` for
  the common spacing, one undo step.

### Copy, Cut and Paste

Sketch geometry goes from one sketch to another (mitcad#788,
`sketch/SketchClipboard.cpp`; commands.md, "Copy and paste of sketch
geometry"):

- **Copy** (`sketch.copy`, Ctrl+C), **Cut** (`sketch.cut`, Ctrl+X) and
  **Paste** (`sketch.paste`, Ctrl+V) are `Action`s of the MODIFY group
  after Delete, with the icons `copy`, `cut` and `paste`; the sketch's
  context menu offers Cut and Copy on selected points and curves, and
  Paste while the clipboard holds sketch geometry
  (`SketchController::canPaste`). Their keys are the reserved Cut Text,
  Copy Text and Paste Text (`fixedShortcutBindings`), which are a text
  field's in the model and in command panels and the sketch's own in a
  sketch: a focused text field (a value box, the browser's search) still
  takes them first.
- Copy sends the selected points, curves and texts of the edited sketch,
  in the order they were selected, to the model's `sketch_fragment`
  query and puts the answer (the fragment, its base point, the directions
  it is held in, its outline for the preview and the counts) on the
  clipboard as the type `application/x-mitcad-sketch-fragment+json`, with
  nothing else, so other applications take nothing from it. A clipboard
  that cannot be written (a session without a desktop, such as the
  Windows UI tests' SSH session 0) still pastes: the application keeps
  the last fragment and uses it while the clipboard holds nothing at
  all. Cut is Copy and then `sketch.remove` of the same items, one undo
  step `Cut in Sketch1` (`SketchOp::setLabel`, which makes `commit` run
  `merge_undo` also for a single step, to name it). The notice says how
  many items and constraints and dimensions went (`%n` plurals) and how
  many were left out.
- Paste is a tool (`PasteTool`, cursor icon `paste`): the fragment's
  curves and texts are drawn at the pointer by its base point (the first
  selected item's first point, marked by a dot), snapping as the drawing
  tools do. A click places it, or the X and Y value boxes (the first has
  the keyboard) with Enter; Esc cancels. In a direction the copy's
  constraints and dimensions hold it to the origin or an axis (the
  query's `held`), the preview and the place stay where the copy was and
  that value box is left out, and the hint says the copy is dimensioned
  to the origin; held in both, Paste places it where it was at once.
  Placing runs `sketch.paste` with the place as `at`, one undo step
  `Paste into Sketch2`, ends the tool with the button's release and
  selects the pasted points and curves; the model's warnings (what the
  paste itself dropped, dimensions that took their values) are a
  notice. The fragment is plain JSON, so a design open in another MitCad
  window takes it too.
- Logs: `Copied c2, t5 from Sketch1: 3 constraint(s) and dimension(s)
  kept, 1 left out, base point at (10, 10)` (`Cut …`, then `Deleted c2,
  t5`), `Paste: geometry of Sketch1 from the clipboard, base point at
  (10, 10), held in y` (`from the application (the clipboard is
  empty)`; `held in x and y`, none when nothing is held), `Sketch tool
  Paste`, `Pasted p2, p3, c4 into Sketch2 at (30, 10): 3 constraint(s)
  and dimension(s) kept, 1 left out`, `Paste cancelled`, `Paste: the
  clipboard holds no sketch geometry`, `Undo: Cut in Sketch1`. Tests:
  `tools/ui-sketch-clipboard-test.sh` and
  `tools/ui-windows-sketch-clipboard-test.ps1` (`app.ui-windows-sketch-clipboard`).

### Redefine Sketch Plane

**Redefine Sketch Plane** (`sketch.redefine_plane`, mitcad#787,
`commands/SketchPlaneCommands.cpp`; commands.md "Redefine Sketch Plane")
moves a sketch to another plane or planar face and places it there. It
is a sketch command (`sketchCommand`: the preview runs the model command
and takes it back, so the view shows the sketch and the features built
on it where OK puts them; OK keeps it as one undo step, Cancel takes it
back) with `Mode::Any` and no tab: it is not on the ribbon, and the
view's generic submenus and the selection card never offer it
(`offeredFor` false).

- **Where it starts.** Outside sketches: the browser's sketch row and the
  timeline's sketch menu (`BrowserController`), and the view's context
  menu after Edit Sketch for a sketch's curve or profile
  (`MainWindowContextMenu.cpp`), each through
  `DocumentHost::startCommand` with the sketch as a `SelectKind::Sketch`
  item. In the sketch being edited: the sketch's context menu (after the
  tools). It is not on the sketch palette: a rarely used feature edit
  does not belong among the palette's view options. Every entry is in
  the sketch's own menus, with the icon `offset-plane` (Edit Sketch above
  it has `sketch`).
- **Inputs.** Sketch (`SelectKind::Sketch`, a browser or timeline pick;
  a new pick loads it again; hidden in the sketch being edited, where
  `init` always sets the edited sketch, so no pick can move another
  one), Plane (a plane or a planar face, `cmd::isPlanar`), Offset with
  Flip beside it (`withBesidePrevious`: both act on the normal), Move X,
  Move Y (lengths) and Angle, the labels of the other placement panels.
  Plane starts empty, its button saying **Same Plane** (`emptyShows`,
  `min` 0): the sketch keeps its plane until one is picked. The sketch's
  plane is not put in the input: a picked origin plane is drawn as a
  highlight whose square, in front of the bodies' faces, would take the
  clicks meant for them (no face of a body over the XY square had a pick
  place). `init` fills the values with the placement's expressions
  (`CommandContext::valueText`: the sketch's own parameters show their
  expressions, a borrowed parameter its name), `0 mm` and `0 deg`
  without one.
- **Build.** `sketch.redefine_plane` with every value as typed (a plain
  zero takes it away) and `flip`; `plane`, its `occurrence` (but for an
  origin plane, as Create Sketch sends it) and `upright` only when a plane
  was picked (the model keeps the own frame and link when that is the
  sketch's plane). A plane after the sketch fails the preview with the
  model's reason (`Sketch1 cannot lie on a plane or face of Plane1:
  Plane1 comes after Sketch1 in the timeline, …`), OK off.
- **After OK.** Logged `Redefined the plane of Sketch1: plane xz, offset
  5 mm, x 0 mm, y 0 mm, angle 0 deg; frame origin (0, 5, 0), x (-1, 0,
  0), y (0, 0, 1)` (and `, flipped`). In the sketch being edited the
  view turns to the moved sketch (`SketchController::lookAt`, as Look
  At); the sketch edit's undo step takes it in.
- Tests: `tools/ui-sketch-redefine-plane-test.sh` (the browser's menu,
  the panel's offset, OK, undo, and the sketch's context menu in sketch
  mode) and
  `tools/ui-windows-sketch-redefine-plane-test.ps1` (ctest
  `app.ui-windows-sketch-redefine-plane`).

## Browser, timeline and parameters

The browser (left) and timeline (bottom) are rebuilt from a
`DocumentSnapshot` after every model change (`MainWindow::refreshScene`,
only when the answers changed). They only show and emit;
`BrowserController` decides and runs model commands through
`DocumentHost::runModelCommand` (refreshes, logs volume changes) or
`runModelCommands` (one undo step). Model changes need the idle mode
(`canChangeModel`); other browser actions in another mode say "Finish the
command or the sketch first.", or while Create Sketch waits for a plane
"Create Sketch is waiting for a plane: pick one or press Esc." (mitcad#474,
`DocumentHost::changeBlockedReason`).

The light bulbs of bodies, the Bodies, Sketches and Construction folders,
sketches, construction geometry, components and the Origin folder work in
every mode (mitcad#295, `BrowserController::visibilityInEveryMode`): while
Create Sketch asks for a plane, while sketching and during commands; a
Flat Pattern's and an analysis's only where the model can change. They go
through `DocumentHost::runVisibilityCommands`; during a command
`CommandSession::runVisibility` (`framework/CommandSessionVisibility.cpp`)
keeps them out of the command's undo range: an applied sketch preview is
taken back before and applied again after, and what takes the range back
later (Cancel; OK of an edit, whose roll-back is undone; the asynchronous
checkpoint restored) runs the recorded changes again, so a cancelled
command leaves them as the user set them and a committed one is one undo
step of its own after them. The inputs stay selected (one on a body hidden
meanwhile is not highlighted), the preview is drawn again over the bodies
shown now from its cached result (computed again only when a body shown
now was changed by the command and was hidden when it was computed), and
the active input's pick places are logged again. An asynchronous command
refuses a change while it computes. Logs: `Visibility <path>: hidden
(during <command>)` (the command's name, `Create Sketch` while it asks for
a plane, the sketch's name while sketching), `Command <name>: <n>
visibility change(s) kept` (they were run again after its roll-back or
checkpoint was taken back), `Visibility during <command> refused: it is
computing`. Tests: `tools/ui-visibility-test.sh`,
`tools/ui-windows-visibility-test.ps1` (ctest `app.ui-windows-visibility`).

Light bulbs and Show When (mitcad#863): a body or an occurrence that
follows a Show When condition keeps it unless the user changes that very
item by hand; the condition never goes silently. A bulk change leaves
conditioned items to their conditions, logs `<label>: kept <n> following
Show When: <names>` and says `<label> left Show When in place for
<names>` in the status bar (`BrowserController::keptByConditions`); a
change of one item removes its condition and says so
(`conditionRemoved`).

- **The Bodies folder.** Its light bulb and its **Show All** / **Hide
  All** change only the bodies without a condition; those with one follow
  it (`BrowserController::conditionedBodies`). The folder's bulb is on
  while one of those bodies is shown (the row is greyed only when no body
  is shown). The folder has no condition of its own, so its bulb is the
  plain one; its tooltip says what a click does and names the bodies left
  as they are (`Clicking the light bulb hides the other bodies. Body2
  follows its Show When and is left as it is.`, logged `Browser bulb
  Root/Bodies on: <tooltip>`), and so does the tooltip of Show All / Hide
  All (logged `Hide All tooltip: Body2 follows its Show When and is left
  as it is.`). Logged: `Hide Bodies Root/Bodies: kept 1 following Show
  When: Body2`. A folder that holds only conditioned bodies has nothing to
  change: Show All / Hide All is disabled with the reason as its tooltip
  (logged `Hide All disabled: <reason>`), and its bulb changes nothing and
  says why in the status bar. A component's row has
  no Show All of its own; its occurrence's bulb is the one below.
- **A conditioned row's light bulb** (a body's or an occurrence's) is the
  fx bulb: the whole plain bulb, open eye or closed eye as the condition
  gives it, with a small `fx` corner badge in its lower right corner,
  outlined in the accent colour as the row's fx badge, on the view's base
  colour with darker text (`EyeIconEngine::setBadge`, `kEyeBoundRole`),
  with the tooltip `Shown when Holes == 'Yes'. Hidden now (Holes is 'No').
  Clicking the light bulb shows Body2 and removes its condition.` (the
  values of the parameters the condition names: `DocumentSnapshot::
  conditionValues`, read from the `parameters` query only while something
  has a condition). Logged at each rebuild: `Browser fx bulb
  Root/Bodies/Body2 off: <tooltip>`.
- **Clicking it** (or Hide / Show in the row's menu) still sets the
  visibility by hand, which removes the condition in the same undo step
  (the model's rule), and the status bar says `Show When of Body2 removed
  – Undo brings it back` (logged `Status: …`).
- **The view's Hide submenu** (`hideBodies`, `hideOccurrences`): Hide
  Body or Hide Component of one item is a change of that item (its
  condition goes, with the status message); Hide 2 Bodies or Hide 2
  Components is a bulk change (`Hide Bodies: kept 1 following Show When:
  Leg`, `Hide Components: …`).
- **Find in Design's Show and Hide** are bulk changes of the found items,
  whatever their number (`Hide Found Items: kept 1 following Show When:
  Body2`; the `Find in Design: hid …` line counts only what changed).
- **Isolate** writes no visibility: `set_isolation` keeps its own list in
  the design's display state (`core/model/src/document/display.rs`), so a
  condition stays and goes on deciding the item's visibility.
- A body hidden by its condition keeps its row, greyed as hidden rows
  are, with its fx badge, so Show When… stays in its menu.
- Test: `tools/ui-show-when-test.sh`.

Change Parameters works while a sketch or a feature is created or edited
(mitcad#564; `MainWindowParameterEdits.cpp`,
`framework/CommandSessionParameters.cpp`, `framework/ParameterEdits.*`).
SKETCH > MODIFY has its own entry (`sketch.parameters`, sketch mode only);
`solid.parameters` (SOLID > MODIFY, Edit menu) is `duringCommands`, so
it, the command search (now also open during commands, listing what runs
beside the panel) and a button at the end of the title row of every panel
with a value field (`CommandPanel::addHeaderAction`, logged as `Panel
<command> parameters at x,y`) open the table while the panel stays open.
At most one of the two entries is enabled at a time. The table runs its
commands through `DocumentHost::runParameterCommand`, and shows what is
open in a line above its rows (`ParameterEditScope::note`); closing it
gives the keys back to the window.

- **Sketch mode:** a change is a model command and an undo step like any
  sketch action. The sketch is read again, so its dimensions and geometry
  follow at once; Undo and Redo in the sketch take it back and again. A
  new sketch keeps it as its own step; the edit of an existing sketch
  merges it into its one `Edit <sketch>` step at Finish Sketch (as it
  merges everything done in the sketch). A tool's half-drawn input stays.
- **A panel open:** `CommandSession::runParameter` runs the change as the
  light bulbs' changes are run (the same recorded list, in order): an
  applied sketch preview is taken back first, an edit's roll-back and the
  asynchronous checkpoint run it again where they are taken back. So
  **Cancel keeps the change**, and OK adds or edits the feature as one
  undo step after the change's own (`Change <name>`, `Rename <a> to <b>`,
  `Add Parameter <name>`). Undo and Redo stay unavailable while the panel
  is open. After a change of values (an expression, unit, value, name, a
  parameter added or deleted; not a comment or the star) every value input
  is evaluated again (the values and the hints under the fields), a rename
  first goes into the panel's expressions that use the old name as a whole
  name (`renameParameterIn`), the selections are resolved again, the
  cached previews are dropped and the preview is computed again; what the
  user typed, chose and picked stays. An asynchronous command refuses a
  change while it computes.
- **The edited feature's own parameters** (owner = the feature edited:
  an extrusion's distance) are the panel's, which OK writes: the table
  does not edit their names and expressions (`ParameterEditScope::holds`;
  tooltip and message "<name> is a value of <feature>, whose panel is open:
  change it there."), and the window refuses such a command
  (`ParameterEditScope::refuses`). Their comments and stars stay editable.
- **Refusals** stay in the cell in red with the model's reason, as outside
  edits; the model, the panel and the sketch keep the last valid value.

Logs: `Parameters during sketch Sketch1` (`command Extrude`, `command
Extrude editing Extrude1`, `command Sketch Fillet in sketch Sketch1`, `no
edit`), `Parameters closed: back to Extrude`, `Sketch Sketch1 dimensions
after the parameter change: width 50.000, d2 40.000`, `Extrude: parameter
depth changed; the preview follows`, `Extrude distance: depth becomes dp
(depth renamed)`, `Parameter d3 is held by the open panel of Extrude1`,
`Parameter change refused during <scope>: <reason>`, `Command Extrude: 1
parameter change(s) kept`, `Parameter change during Extrude refused: it is
computing`. Tests: `tools/ui-parameters-edit-test.sh`, the unit test
`app/tests/parameter_edits.cpp`.

- **Rows:** a row's `SelectionItem` (`BrowserNode::item`) carries the
  occurrence path. Light bulbs: `set_body_visible`,
  `set_occurrence_visible`, `set_feature_visible`, `set_origin_visible`
  (commands.md, "Visibility and display"); a folder sets all of its own as
  one undo step. The model decides what is shown (the `timeline` query's
  `visible`, `DocumentSnapshot::featureShown`): a sketch whose bulb was
  not set is hidden while a feature uses it. The view shows only what is
  shown (`instances` leaves hidden bodies out), and the sketches a
  running command uses, for its time (mitcad#546, see [What the framework
  does](#what-the-framework-does); the browser's light bulb stays the
  model's). A fully constrained
  sketch's icon is `sketch-locked` (the `timeline` query's `dof`).
- **Features under sketches** (mitcad#177): each feature that uses a
  sketch (the `timeline` query's `sketches`) has a row
  (`BrowserNode::Type::Feature`, keyed by feature and sketch) under each
  sketch row it uses, in timeline order; sketches, construction geometry
  and joints keep their own rows only. Sketch rows with such rows start
  collapsed. Feature rows have no light bulb (hiding a sketch hides only
  it), are grey when suppressed or after the marker and red when failed,
  and a feature whose bodies are in another component than its sketch (a
  new component) carries a component badge (`LinkedIconEngine`) with that
  component in its tooltip. Their menu is the timeline's
  (`BrowserController::featureMenu`, shared), with Rename in place and
  Find in Timeline; a double-click edits, Delete deletes the feature.
  Test: `tools/ui-browser-features-test.sh`.
- **Collapse All** (mitcad#312): the last entry of every browser row's
  context menu (`BrowserController::browserMenu`, `featureMenu` for feature
  rows; not the timeline's) calls `BrowserPanel::collapseAll`, whichever row
  was clicked. It folds every row below the top row, those under an already
  folded row too, and remembers them as folded (`foldBelowTop`: `m_expanded`
  is cleared, then filled with `false` for each row), so a rebuild does not
  reopen anything, also not the folders that open by default. The top row
  (the root) is opened and remembered open (mitcad#426): its children stay
  in sight, folded; its own arrow still folds it. An arrow click then opens
  only that row (the tree never opens recursively); rows below stay folded
  until their own arrows are clicked. A presentation action: it runs no
  model command and changes no visibility. Logs: `Browser collapsed all`;
  `logLayout` also logs each row with children's arrow as `Browser arrow
  <path> at x,y`. Test: `tools/ui-browser-collapse-test.sh`.
- **A design as it opens** (mitcad#426): every design that comes to the
  window from a file (`MainWindow::installDocument`: Open, a version, a
  copy, a recovered document, an import of any format) opens with the root
  active, whatever active component its file stores: right after reading,
  on the worker, `activate_component` with `"opening": true` (no undo step,
  no new revision, so a design opened unmodified stays unmodified; the
  file stores what is active when it is saved, as before). Logged when the
  file had another one active: `Opened with the root active; C1 was
  active in the file (unmodified)`. Every new design starts with its
  Component1 active instead, so its first sketch and feature go into
  Component1, whichever way it is made (`m_openingNewDesign`; each logged
  as `Opened the new design with Component1 active`): the untitled design
  at the start (`DocumentSession`, logged after the first recompute; not
  the `--demo` design), New and Close (`newDocument`, `closeDocument`), an
  empty project opened and the open design deleted in the Projects panel
  (all four through `installEmptyDocument`, mitcad#898), and New Design in
  a project (`createDesignInProject`, mitcad#844: the design it made, with
  the component its file stores as active). Then
  `BrowserController::designOpened`
  makes the browser rebuild with the rows' default states
  (`BrowserPanel::designOpened` forgets what the design before left open)
  and, once the panel shows with its size (a design opened at the start
  is read before the window shows),
  counts the rows in sight against `BrowserPanel::availableHeight` (the
  rows' viewport; the Floating layout gives the card's largest height,
  `FloatingLayout::maximumContentHeight`, with `setAvailableHeight`; the
  viewport again while that is not known yet). More rows than fit: the top
  row stays open and every row under it is folded and remembered so
  (`foldBelowTop`, as Collapse All), the root's children in sight with their
  arrows closed; otherwise the rows stay as they opened. Only at the
  opening: later rebuilds (a new component, an undo, a recompute) keep the
  rows as the user left them. Logs: `Browser opened folded: <n> rows would
  not fit (<visible> visible)`, `Browser opened expanded: <n> rows
  (<visible> visible)`. Tests: `tools/ui-browser-collapse-test.sh`,
  `tools/ui-windows-browser-open-test.ps1`.
- **Background in the Floating layout** (mitcad#419,
  `browser/BrowserBackground.*`): Preferences › Display › Browser
  background, the setting `appearance/browserBackground`, shown only in
  the Floating layout and applied at once (OK, or as it changes in the
  macOS Settings window). **Card** (`card`, the default): the Browser on
  its card, the rows as a sidebar's (`SidebarDelegate`). **Rows** (`rows`):
  the card is bare (`GlassCard::setBare`): no material and no shadow; its
  header is a small box of the card's material as wide as its chevron,
  title and buttons, which follow the title; each row in sight is on a box
  of its own (`BrowserTree::rowBox`, painted in `drawRow` under the row's
  arrow), from its arrow's place to the end of its name or radio button and
  2 px short of the row at the top and bottom, tinted with the accent colour
  when selected and lightly under the pointer. The card's mask
  (`GlassCard::updateBareMask`) is the header's box and
  `BrowserPanel::rowsRegion` (the rows' boxes, the scroll bars shown, a
  rename editor), kept up to date as the rows scroll, open, fold, change,
  are selected or the card resizes or folds: a click, drag or wheel
  anywhere else in the card's rectangle reaches the 3D view (select,
  orbit, zoom), as beside a card. The view's covered area for fitting and
  pick places stays the card's rectangle. With native glass the card is a
  widget over the view while bare (`GlassCard::setWindowed(false)`), a
  window again with Card. The timeline and the command panel keep their
  cards. Logs: `Browser background: rows` (`card`; at start and on each
  change), `Preferences: browser background rows`, `Preferences browser
  background at x,y`, and with Rows `Browser view beside the rows at x,y`
  (right of every row's box at the middle row in sight; `… out of sight`
  when it goes, as Card comes back). Tests: `tools/ui-browser-rows-test.sh`,
  `tools/ui-windows-browser-rows-test.ps1` (ctest
  `app.ui-windows-browser-rows`).
  With Rows the vertical scroll bar is at the card's left edge, beside the
  rows' start (mitcad#425, `BrowserPanel::setScrollBarOnLeft`, called by
  the Rows switch): a bar of the panel's own that mirrors the tree's (the
  tree's own is off; the wheel and the keys still scroll it), shown while
  the rows overflow, the tree right of it, and in `rowsRegion`; with Card
  the tree's bar stays at the right. Logs: `Browser scroll range 240` (the
  tree's vertical range, 0 while every row fits) and `Browser scroll bar
  at x,y` (the vertical bar in sight, either one; `… out of sight`).
- **The Browser card's height** (mitcad#425, the Floating layout on every
  platform): the card is as high as its rows in sight
  (`BrowserPanel::contentHeight`) and grows down as rows open, up to the
  card below it (the timeline, or the view's bottom margin when the
  timeline is hidden) less the gap, and only then scrolls; it shrinks as
  rows fold, and is placed again when the window resizes (the Rows mask
  follows). `FloatingLayout::maximumContentHeight(content)` gives the
  height the content of a card may take at its place before it scrolls
  (here `m_floating->maximumContentHeight(m_controller->browser())`; -1
  before the first layout). Its height is in the `Floating cards:` line
  (`browserDock`).
- **Rename in place:** F2, the menu's Rename, or a slow second click on
  the name of the selected, current row (`BrowserTree`: after the
  double-click interval, not in a double-click or a drag; the timer is
  watched by `TestSync`). Bodies, components, sketches, datums, joints,
  analyses and named views rename this way too.
- **Paste / Paste New** place the copy where the original is and open
  Move/Copy on it (`DocumentHost::startCommand` with the occurrence
  selected); OK keeps the moved copy as its own placement
  (`set_occurrence_transform`, not a `move_occurrence` feature), paste and
  placement one undo step; Cancel takes the paste back.
- **Hybrid assemblies** (mitcad#171, `framework/HybridAssembly.cpp`):
  before a component is made in the root while the root has modelled
  bodies, `askHybridAssembly` asks (title `Hybrid Assembly`): Move into a
  New Component (default; `move_to_component` with everything), Continue
  as Is, Cancel, and "Don't ask again for this design"
  (`set_hybrid_assembly`). It is called by the browser's New Component
  and Create Components from Bodies (`BrowserController`), and on OK of a
  `modelCommand` whose commands make a component (`create_component`,
  `components_from_bodies`) or of a new feature with the `new_component`
  operation (`CommandSession::askBeforeComponent`, both the synchronous
  and the asynchronous commit). Cancel leaves the panel open. For Create
  Components from Bodies, Move moves each of the root's selected bodies
  with its features into its own component (`groups`) and the command is
  done; a new feature whose references moved (`feature_references`
  query) is added to the new component (`add_feature`'s `component`).
  Move to Component (a body's row, a sketch's row (mitcad#218), a
  timeline feature: `bodies` or `features`) and Move Features into a
  Component (a component's row) run `moveToComponent`. Its dialog always asks for an existing destination or
  Create New, with a name and parent, before running a command. Component
  paths and occurrence identifiers distinguish repeated names; browser
  rows also supply `source_occurrence`. The plan lists expanded sketches,
  features and bodies, shows target problems, and offers Keep Them Together
  (`together`) for conflicts. Cancel makes no model change; a failed move
  leaves the dialog open. From a body's row the dialog also offers **Move
  Only the Bodies (a Body Transfer on the Timeline)** (mitcad#465,
  `moveBodiesOnly`): an existing destination only, its rows' reasons from
  the `transfer_bodies` query; Move runs the `transfer_bodies` command (a
  `body_transfer` feature in the destination at the marker, one undo step
  `Move Body1 into Bracket`), the features that made the bodies staying
  where they are. Log line: `Transferred Body1 into Bracket
  (BodyTransfer1)`. Test: `tools/ui-body-transfer-test.sh`, `app.unit`. These explicit moves send `destination`; Hybrid
  Assembly keeps its automatic new-component behavior. Move the Features
  That Made It into the Component uses `move_history_into_component` for a
  `component_from_bodies` feature. Log lines: `Hybrid assembly question: Root
  has Body1`, `Hybrid assembly: move|continue as is[, don't ask again]`,
  `Moved Sketch1, Extrude1 into Component1`. Test:
  `tools/ui-hybrid-test.sh`; explicit destination dialog checks: `app.unit`.
- **Move/Copy** on a body's row and on an occurrence's row (not the root;
  mitcad#735): `BrowserController::menuMoveItems` gives the selected rows
  of the row's kind when the row is one of them, else the row alone; the
  entry is there when `DocumentHost::offersCommand("solid.move", items)`
  holds (the window is idle, the command is available, its
  `offeredAtTop` takes the items; left out otherwise, as in a read-only
  window) and starts the command with them (`startCommand`). On a body's
  row it follows Edit <feature>; on an occurrence's row it opens the
  section with Ground, apart from the structure moves above. Tests:
  `tools/ui-move-copy-menu-test.sh`, `tools/ui-windows-move-copy-menu-test.ps1`.
- **Move to Component…** (Move to… before mitcad#735, renamed so that it
  is not taken for Move/Copy: the same name as the body's structure move)
  on a component occurrence's row (not the root; mitcad#219,
  `framework/MoveOccurrenceDialog.cpp`, `BrowserController::moveOccurrence`):
  another parent for the occurrence with everything in it, where it stays
  in the design. The dialog (title `Move Component`) reads the
  `reparent_occurrence` query: the root and every placed component by full
  path, the current parent marked, unavailable parents grey with their
  reason (selectable, so that the reason shows); what is in the occurrence
  and, for the chosen parent, the references that follow it or the
  blockers, and the features that place it and go with it into the new
  parent (mitcad#301: `Joint1 moves from Box to Root`, or `Part:1's part
  of Move1 moves to Move2, a new feature of Root` when a feature is split).
  Move runs the `reparent_occurrence` command (one undo step) and
  selects the occurrence's row at its new path; a refused command leaves
  the dialog open; Cancel changes nothing. Log lines: `Move Component
  parents of Part:1: Root | Root/Assembly:1 (unavailable) | …`, `Move
  Component features: Joint1 -> Root` (`Move1 -> Move2 in Root`), `Move
  Component parent: Root, ok` (or `…, unavailable: <reason>`), `Moved
  Part:1 to Root`, `Move of Holder:1 cancelled`. Tests: `app.unit`
  (`tests/move_occurrence.cpp`), `tools/ui-move-test.sh` (also the sketch
  row's Move to Component, and a component a joint places).
- **Selection:** rows set the selection outside commands
  (`DocumentHost::pickItems`); in a command a row goes to the active input
  like a click in the view; while Create Sketch waits for a plane, a plane
  row starts the sketch. Browser and timeline follow each other
  (mitcad#177): the features of the selected rows (sketches, datums,
  joints, feature rows) are selected in the timeline through
  `TimelineWidget::setSelectedFeatures`, which emits nothing; a feature
  row selects what its timeline cell does (`featureItems`); a click in the
  timeline selects the feature's rows (`BrowserPanel::showSelection` with
  the features, also silent; `BrowserController::pick`). A click in the
  view leaves the timeline as it is.
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
  red (mitcad#961, `browser/FeatureStatus.*`): the cell's fill, a status
  ring on its edge and a status badge at its top right corner, a red
  circle with a white "!" when failed, an amber triangle with a dark "!"
  for a warning (shape as well as colour), in a halo of the strip's
  background. The badge is drawn in every state, selected, hovered,
  found by the search or rolled back (it is never dimmed), and on a
  folded group's cell when one of its features failed. A selected or
  found cell gets the selection's (or the match's) ring on its edge and
  keeps the status ring (2 px, brighter on a dark theme) just inside it;
  a failed or warned cell keeps its fill when selected or hovered, the
  pointer on it a ring in the text colour (80 %) outside the status ring. The selection card
  of a failed feature ends with `Failed: <error>` in the error colour
  (`errorColor`: #b00020, on a dark theme #ff9c9c, at least 4.5:1 also on
  the Floating card; the light theme's amber ring #a87d00, 3:1) (a
  warning's with `Warning: <message>`); a failed feature has no faces to
  keep in the selection, so the timeline's selection alone shows its card
  (`MainWindow::updateIdleView`), and the failures' and warnings' buttons
  select the feature they go to as a click on its cell does
  (`BrowserController::reveal`, not during a command). Esc, Clear
  Selection and the autoselect panel's clear clear the timeline's
  selection too (`MainWindow::clearSelection`). Those buttons are badges
  of their own colours on every theme (`issueBadgeStyle`): a red capsule
  with white text and the red circle in a white halo, an amber one with
  dark text and the triangle, each at least 4.5:1; in the Floating
  browser's header the warnings' one shows only its count
  (`setCompactBadges`), its tooltip and accessible name the rest
  (`1 warning`, `2 warnings`). A text of `setErrorStyleSheet` /
  `setWarningStyleSheet` takes the colours of what it sits on (the first
  ancestor with an opaque background) and is checked again when it paints:
  Qt passes a window's palette change on to no child with a sheet of its
  own. Test: `app.unit` (`tests/feature_status.cpp`, the painted pixels of
  a failed and a warned cell, selected and not, the buttons' contrast as
  painted on a dark, a grey and a light window, their icons, a themed text
  after the window turns dark). A feature that made no body of its own (fillet, hole, join) is
  selected as a `SelectKind::Feature` item placed by its component's first
  occurrence; `itemShape` gives the faces whose names start with `<uid>:`
  on the bodies at the marker. `pickItems` and `keepSelected` leave out a
  feature without such faces. Delete asks with the `dependents` query.
  Dragging the marker runs `set_marker` per crossed feature and merges the
  steps into one `Move Timeline Marker` (none when it ends where it
  began). Dragging a feature checks `can_reorder`. Groups are the model's
  (`group_features`); folding is the window's (`TimelineWidget`
  `m_collapsed`), not saved. So is the filter (mitcad#98,
  `TimelineWidget::setFilter`, the filter button and the browser's Show
  in Timeline Only): hidden features take no room (`Item::filtered`), the
  marker and drops land on `shownBoundary` places, and the playback
  buttons step between them; it logs `Timeline filter: <label>: n of m
  features`. Test: `tools/ui-component-sketch-test.sh`. Colours of the marker, buttons and strike
  line come from the palette and are repainted on a palette or style
  change (`TimelineWidget::changeEvent`). Test: `tools/ui-theme-test.sh`
  (the scheme switched with `MITCAD_TEST_COLOR_SCHEME`).
- **The import's outcomes** (mitcad#170): what an importer said about a
  feature is the `timeline` query's `import` (also first in `error`,
  `DocumentSnapshot::Feature::importNote`). A fallback's base feature and
  an item that came in partly have the status `warning`, so they are
  yellow, the note in the tooltip ("Stands in for Extrude5 (its result is
  not in the file's history)", "Partly imported: left out: ..."). A
  placeholder (type `placeholder`, the `feature` icon) of an item the file
  had suppressed or failed is drawn suppressed with a yellow outline, its
  tooltip "Suppressed: <the file's state>"; so is an `.f3d` item
  suppressed in the file that came in as its own feature (mitcad#180). The failures badge counts only
  features that failed; beside it a yellow badge (`BrowserController::
  warnings`, in the status bar in the docked layout and the browser card's
  header in the floating one) counts the features of
  `DocumentSnapshot::warnings` (status `warning` or an import note, not
  failed) and goes to them like the failures badge: to the one, or
  through a menu of them. An edit of the feature that builds clears its
  note (the model's). Test: `tools/ui-import-status-test.sh`.
- **Timeline search** (mitcad#179): the timeline's search button, Ctrl+F
  (`QKeySequence::Find`, Cmd+F on macOS) in `keyPressEvent` of the
  timeline's strip and the browser's tree (`BrowserPanel::searchRequested`;
  no window shortcut, so it acts only where the timeline or the browser
  has the keys), and `view.search_timeline` (Edit › Search Timeline, the
  command search; disabled in the native macOS UI, whose history is its
  own) call `TimelineWidget::openSearch`: a field after the filter button
  (`m_searchBar`: field, "n of m", previous and next), in both layouts
  (the dock and the floating card hold the same widget). The text is read
  by `TimelineQuery` (`browser/TimelineSearch.*`, plain Qt Core, unit
  tested in `app.unit`): words the name or the type must contain, `in:`
  (`component:`) and `is:` (`status:`) filters with abbreviations, an
  unknown status matching nothing. It searches the features the
  component filter shows (`Item::filtered`); `is:warning` counts as the
  warnings badge does (`DocumentSnapshot::warnings`), `is:rolled-back` is
  after the marker. Matches are outlined (`TimelineStrip::setFound`,
  `kFoundBorder`) and listed with their component's colour and name in
  `m_searchList`, a `QTreeWidget` child of the host window
  (`GlassCard::hostWindow`) placed over the field, without focus. The
  field takes Enter, Esc, Up and Down before the window's OK and Cancel
  (ShortcutOverride): Enter and Down step forward, Shift+Enter and Up
  back (wrapping), Esc closes; a step is `selectFeature` (scroll, a folded
  group opened) plus `featuresClicked`, as a click. When the text becomes
  non-empty the scroll position and the timeline's selection are kept and
  `searchStarted` lets the controller keep the window's selection; when
  it becomes empty after a step and the step's match is still the
  selection, both come back and `searchRestored` makes the controller
  pick its kept selection again (outside commands). Test:
  `tools/ui-timeline-search-test.sh` (both layouts).
- **Find in Design** (mitcad#350): `view.find_in_design` (Edit › Find in
  Design..., the command search) and the magnifier button
  (`BrowserController::findButton`) on the browser's title row (in the
  floating layout the first widget of the card's header; in the docked
  one a title bar of the dock's own, `MainWindow::createPanels`) call
  `BrowserController::openFind`: `FindDialog` (`browser/FindDialog.*`), a
  non-modal window over the main window. The search is
  `findInDesign` (`browser/FindInDesign.*`, plain Qt Core) on the
  `DocumentSnapshot` (occurrences, components, bodies and their names), not
  on the browser's rows: the root's bodies, then each occurrence with its
  bodies and children; a body of a component placed twice is a result per
  place (`FindResult::key`). `NameMatcher`: a case-insensitive substring,
  `*` and `?` as wildcards (still unanchored). Text, kinds and visibility
  filter the results after a 150 ms pause (`TestSync::watch`'d timer); the
  window keeps the selected results through a re-search
  (`BrowserController::refresh` calls `setSnapshot`, which ignores an equal
  snapshot key). The buttons act on the selected rows, or on all:
  Show and Hide run `set_body_visible` per body uid and
  `set_occurrence_visible` per occurrence path, each once, through
  `DocumentHost::runVisibilityCommands` as one undo step ("Show Found
  Items" / "Hide Found Items"); Appearance... calls `openAppearances` with
  the bodies (a component's: its definition's bodies); Select and
  selecting rows call `DocumentHost::pickItems` with body items (uid and
  occurrence) and component items (the component and its occurrence; not
  during a command); a double-click calls `findInBrowser`. All of them
  wait for a running job (`whenIdle`). Test:
  `tools/ui-find-in-design-test.sh`.
- **Analysis folder** (mitcad#41): the analyses kept in the document (the
  `analyses` query, `DocumentSnapshot::analyses`), after the root's other
  rows. Light bulb: `set_analysis_visible`; showing one hides the others,
  as one section cuts the bodies at a time. Double-click and Edit Section
  Analysis open its panel with its plane, offset, flip, axis and angle
  (`DocumentHost::editAnalysis`); Rename, Delete (`rename_analysis`,
  `delete_analysis`); the folder's menu starts a new Section Analysis or
  a new Print Analysis. An
  analysis whose plane cannot be found at the marker is red with the
  reason in its tooltip, and cuts nothing. Remove Section Analysis hides
  the shown section (it stays in the folder); Flip Section Analysis edits
  it with `flip` toggled; neither is offered while a Print Analysis is
  the one shown. A Print Analysis (mitcad#620) is a row `Print1` with
  its own icon (`print-analysis`), edited with Edit Print Analysis (the
  row's `featureType` is the analysis' `type`); one whose bodies or bed
  face are gone is red with the model's reason and draws nothing.
- **Section coordinates** (mitcad#204): preview, the manipulator and saved
  analyses share the model's `section_plane` resolver. A selected component
  face or construction plane keeps its occurrence path, including when an
  analysis is reopened. Distance is applied along the world normal before
  Flip reverses it. Each displayed body converts the same world plane to
  its local coordinates before clipping and keeps its usual placement;
  caps use those clipped faces. Section area and outline length count
  visible occurrences. Regression: `tools/ui-macos-section-test.sh` checks
  the actual retained volumes and world bounds, plus screenshots.
- **Section angle** (mitcad#823): after Distance, Section Analysis has
  Axis (0 or 1 axial items, `axisOf`: a straight edge, a sketch line, a
  construction axis or an origin axis) and Angle (`0 deg`, a ring
  manipulator about the axis at its point nearest the picked plane's
  centre, its 0 along the plane's normal across the axis, as Plane at
  Angle draws its ring). `sectionPlane`, `build` and `load` pass and
  restore `angle` (radians), `axis` and `axis_occurrence` (the picked
  axis' occurrence path); the model turns the plane about the axis
  before Distance moves it along the turned normal and Flip reverses the
  normal, so the arrow keeps moving along the shown plane's normal. OK
  with an Angle other than 0 and no axis fails on Axis ("Select an axis
  for the angle."); an axis along the plane's normal fails in the model
  (the preview says the plane cannot be found). Drawing section views of
  the analysis follow the angle. Test: `tools/ui-section-caps-test.sh`
  (a 10 mm cube cut through its middle at 45 degrees about Z: area
  141.42 mm², kept, edited with its Axis and Angle, saved).
- **Edit Appearances** (`solid.appearances`, mitcad#46; also Appearance...
  in a body's browser menu and in the view's menu of bodies, faces, edges
  and vertices): the `appearances` query's library and the design's own
  appearances with swatches, the selected one's preview and parameters
  (`AppearanceDialog`, modeless, refreshed with the browser). The
  library's are read-only; New is `create_appearance` with `based_on` the
  selected one (named "<name> Copy"), each field change one
  `edit_appearance` undo step (colours as `#rrggbb` or from a colour
  dialog), Delete `delete_appearance`, Assign `set_body_appearance` for the
  bodies it was opened for (the selection's) as one undo step, and
  `set_face_appearance` for its faces (mitcad#53: the dialog opened on
  selected faces assigns to them; on edges and vertices, to their
  bodies). Assign to N bodies (mitcad#346), shown beside Assign while the
  selection holds faces: `set_body_appearance` for each body that owns a
  selected face or is selected, once, as one undo step; the faces' own
  appearances are untouched. A double-click on an appearance assigns it as
  Assign does (mitcad#861). The assignment panel at the bottom (mitcad#861,
  `AssignmentPanel`; [Bindings](#bindings-varies-with-suppress-when-show-when))
  shows what the targets have and **Varies with**: with an option, Assign,
  a double-click, New and a texture's Use fill its rows instead (Assign
  reads `Assign to row Red`; Assign to N bodies is hidden), each fill one
  binding written to every target; with None the window is as before. The
  window grows by the panel's height when the panel shows or gets taller
  (`growForPanel`, once per height, within the screen), so the body keeps
  its height where there is room; the face list keeps at least two rows
  (the body scrolls instead). The list logs only the rows in sight in the
  body too, which scrolls while the panel's table takes room. The texture (mitcad#53): Image (typed, or Choose... with a file
  dialog; made relative to the design's folder when it is saved), Size,
  Rotation (degrees), Projection (Box, Planar), Embed the image in the
  design (reads the file into `data`; off again needs the file at its
  path), Remove; each one `edit_appearance` with the texture as the
  `appearances` query lists it, so an embedded image stays; a line says
  when the image is missing. "Faces with their own appearance" lists the
  face appearances of the bodies it was opened for (`bodies` query's
  `face_appearances`; a face not found is marked), and Clear gives the
  selected ones their body's again (`set_face_appearance` with null, one
  undo step). The Appearance panel (`solid.appearance`) lists the same
  appearances (`withChoices`) and has a Faces input besides Bodies
  (Default clears a face's own). The view colours each body with the
  appearance's `display_color` and faces with theirs
  (`MainWindow::modelBodies`, `BodyDisplay::faceLooks`, an
  `AIS_ColoredShape` in the shaded styles); `BodyDisplay::appearance`
  carries the parameters to the rendered view, and a change of them alone
  emits `bodiesChanged` without redrawing the shaded body.
  Texture sources (mitcad#315, `app/materials/`): a selector above the
  list chooses the appearances (the list; the search field and the
  category, All, Library or This Design, hide what does not match) or a
  registered `TextureSource` (`textureSourceRegistry()`; now Local
  Folders, `FolderTextureSource` over the folders of Preferences ›
  Textures, the setting `textures/folders`). A source's search fills
  `TextureBrowser`'s grid (thumbnails asked of the source as they come
  into sight and kept in the texture cache); its categories fill the
  filter; the chosen asset's resolutions (`textures/resolution`
  remembers the choice, 2k at first) show the bytes they take; its
  attribution line, when it has one, is under the grid. **Use** fetches
  the asset's maps in the resolution and sends one `create_appearance`
  (`textureUseCommand`): the asset's name (made unique), the base colour
  image's average as `base_color`, and a texture with each map's file
  name and `data`, or `{"path", "embedded": true, "image_sha256"}` for an
  image the design embeds already (`embeddedImageDigests` of the
  `appearances` query), `size` the asset's or 100 mm wide in the image's
  proportions, and `source` (`id`, the asset's recorded id, `resolution`,
  `license`, `url` when known); the new appearance is selected for
  Assign. The model never sees a source or the cache. A source that uses
  the network is asked only once the user allows it (asked on its first
  selection; No goes back to the previous source and asks nothing;
  `textures/allowNetwork`, Preferences › Textures). Choosing an asset
  asks the source for its details once (`TextureSource::details`: an
  online source's real resolutions and their bytes).
  The texture's own lines list the set's maps and its source.
  **Poly Haven** (mitcad#313, `PolyHavenTextureSource`): the CC0 textures
  of https://polyhaven.com through its public API
  (`https://api.polyhaven.com`): `/assets?t=textures` gives the list
  (name, tags, categories, `dimensions` in mm as the texture's size,
  `max_resolution`, the thumbnail's address, `files_hash`; kept in the
  cache as `polyhaven/textures.json` for a day, and used however old when
  the API cannot answer), `/files/<id>` the chosen asset's maps per
  resolution (`Diffuse`, `nor_gl`, `Rough`, `Metal`, `AO`; JPEG where
  there is one, else PNG; kept as `polyhaven/<id>/files-<files_hash>.json`),
  the thumbnails come as PNG in the size shown (`thumbnail-96.png`, a
  month), and Use downloads only the chosen resolution's maps into
  `polyhaven/<id>/<resolution>/`, checked by size and MD5 (kept for a
  later Use). `source` records `polyhaven`, the id, the resolution,
  `CC0-1.0` and `https://polyhaven.com/a/<id>`. The requests go through a
  `TextureFetcher` (`NetworkTextureFetcher`: Qt Network, HTTPS only,
  User-Agent `Mitcad/<version>`, no cookies, 30 s without data ends a
  request, nothing while the network is not allowed); failures (no
  network, a timeout, an HTTP error, an unexpected answer) are sentences
  in the browser and leave the other sources working.
  `MITCAD_TEXTURE_FIXTURES` answers the requests from files
  (`<folder>/<host>/<path>`) for UI tests.
  The dialog stays within the screen's available area, title bar
  included (mitcad#818): its body (the lists, the preview and fields and
  the faces) is a `FittingScrollArea` that scrolls on a lower screen, with
  the dialog's buttons below it; it opens at 820 x 780 where the screen has
  room, follows changes of the screen (`keepWindowOnScreen`) and can be
  made compact by the user. Regressions: `tools/ui-appearance-fit-test.sh`
  (a 600-pixel Xvfb screen, Floating and Docked) and
  `app.ui-macos-dialogs`.
- **Change Parameters** (`solid.parameters`; `sketch.parameters` in
  SKETCH > MODIFY): each cell change is one
  model command and undo step; a bare number gets the parameter's unit;
  refusals stay in the cell in red with the reason. While a sketch or a
  feature is edited, see the paragraph after the light bulbs above
  (mitcad#564). Favourites are
  `set_parameter` `favorite`. `--set name=value` sets a value the same way.
- **Change Parameters, aligned numbers** (mitcad#566): the Expression and
  Value columns paint a cell whose whole text is one number with an
  optional unit (`3 mm`, `-2.75 mm`, `0.5 in`, `30 deg`, `2,5`) with
  `DecimalAlignDelegate`: the integer part ends at one decimal axis per
  column (an integer has its implied separator there), the separator and
  fraction follow, and the units start on a line of their own after the
  longest fraction, in tabular digits (`tnum`). The layout is measured over
  all rows, hidden ones too, so that the axis stays when the filter
  changes. The cell's text is not changed: selection, the focus frame, the
  tool tip and the in-place editor are the standard ones, and the editor
  shows the whole text. Formulas, names and refused entries are painted as
  text at the block's left edge; a column narrower than its block is
  painted as standard text, with an ellipsis. `DecimalAlign` (the split of a
  text, the column layout) is plain Qt Core; the window logs
  `Parameters <name> value|expression decimal at x,y` and `... unit at x,y`
  for the UI test.
- **Materials and print settings** (mitcad#640, mitcad#655;
  `app/print/`): **Materials** and **Printers and Processes** are library
  windows of one layout (the list on the left, no search or filter of its
  own; the selected item's editor on the right, whose settings search is
  the window's one search; New, Duplicate and Delete under
  the list, Close on the right); 3D Print's Printer, Process and
  Materials rows choose and open them; **Print settings** is in Property
  Sets. The user never handles a copy: choosing an item copies it into
  the design, editing an item edits it where it lives. Every list reads
  the model's `library_items` query (`print/PrintLibrary.*`,
  `LibraryItem`): one row per item, and the user sees two states only
  (the owner's option A): built-in (the built-in materials and the base
  library, read only) or the user's own, saved in My library or the
  project's library or not yet; the lists and the boxes of 3D Print and
  Property Sets show no places, and the editor's header says `Built-in`
  for a built-in item only. The design's printer and
  process, and the materials its bodies use, are bold with a small
  filled dot (`markInDesign`, `usedByDesign`): one mark, one meaning,
  "used in this design", said in words too, the same in both windows:
  the header shows `● Used in this design` next to the name, and Use in
  this design shows only for an item the design does not use. A name
  is never shown twice: the model gives a clashing new or saved item a
  counter (`Fine 2`).
  No ids, no versions and no raw slicer keys are shown. Commands name a
  library's item with its `library` and the project (`nameItem`); the
  design's and the built-ins by id alone. The lists' selected row is at
  full strength, with the keyboard or without (`styleItemList`).
  - **The settings form** (`print/SettingsForm.*`, `print/SettingField.*`,
    `print/PrintKeys.*`): one widget wherever print settings are edited.
    Search settings (the plain names, advanced ones too) and Advanced
    (remembered, `print/advancedSettings`) on top; the categories of the
    set as tabs, their sub-groups as headed sections in the key table's
    order (`print_keys`' `category`, `group` and `layout`; without a
    layout the keys' own); a row per key with its plain name. Advanced off
    hides the advanced keys unless the item sets them; a sub-group or tab
    with nothing to show is hidden. A field per kind of value, the same
    everywhere and of the same widths, set or not: a Yes/No drop-down, a
    number field with its unit (`%` for a percentage; a [mm | %] box wide
    enough for its unit only for a key that takes a number or a
    percentage, and only where the design's slicer takes the percentage:
    `print_keys`' `percent`, `PrintKey::takesPercent`, `designDialectOf`;
    elsewhere the unit as a label, `%` only for a percentage already set), a drop-down of the choices' plain names, a colour as
    a field drawn as the others (their frame and white fill) with a swatch
    of its height at the left and words after it, which opens the colour
    picker, never a code to type (the swatch solid with a neutral border,
    a checker only under transparency; unset: the swatch of the colour
    used instead and its words in the placeholder's grey, Materials' the
    appearance's name; without one a checker with `Slicer default`), a
    text field, an outline as `256 × 256 mm` in a read-only field of the
    standard width (unset: empty with `Slicer default`) with Edit… (width, depth and origin for a rectangle, else a table of
    points); a value per slicer (a printer preset's name) is not shown.
    Values are in the key's own unit, not the
    design's, and its range (`printRange`, mitcad#746: the `print_keys`
    query's `min` and `max`, what the key's slicers take, within the
    design's slicer's own range where that slicer takes less: a fill
    density 0 to 100 %, a purge line spacing 100 to 300 %, Tree support
    walls -1 ("automatic") with a Bambu Studio printer but 0 to 2 with an
    OrcaSlicer one, a compensation or a direction either way). An unset value is the same empty field with the grey
    placeholder `Slicer default` (a drop-down with no choice made), or what
    stands for it (`setPlaceholderValues`: Materials' filament colour shows
    the appearance's colour, which is what is exported, with the
    appearance's name in the placeholder's grey); a click
    leaves it empty, nothing is guessed, and Enter on an empty field sets
    nothing. A field is saved at Enter or when it is left after a change
    (a choice, a colour at once), as one model command and undo step;
    emptying a set field clears it, and ↺ (tooltip Reset, in one column
    after the fields) clears a set value; a plain unit is a label set where
    a [mm | %] box shows its text, and an unset field shows its unit as
    text until a value is typed; typed text that is not a value of the key, and the
    model's refusal of it, show under the field as an instruction (`Use a
    value of 0 or more`, `Use a value from 0 to 100`, `Use a colour`;
    logged `Setting <key> refused: <reason>`). The own values that
    bodies, occurrences or components set of a key show as a grey note
    after it (`4 on Body1, 3 on Arm`, `setOwnValues`: the process editor on the
    design's process). A window's
    own fields can be a first tab beside the settings' (`setGeneralPage`:
    Materials' General), and `setPrintShown(false)` shows that page alone;
    Search settings stays on every tab and finds the page's fields too
    (`setGeneralNames`), Advanced wherever there are print settings. Drop-downs take the mouse wheel only while they
    have the keyboard: the wheel scrolls the form. The selected tab is
    bold and underlined in the highlight colour. A slicer's own keys are
    not shown. Wherever a key or a model's message would show a raw slicer
    key, the key table's plain name stands for it (`PrintKeyTable::
    plainName`, `plainText`); keys with no plain name are counted, never
    listed (`3 other settings`). The windows have no OK and no default
    button: Enter saves the field it is in. The model's refusals reach the
    message line in plain words with a next step (`plainModelError`, the
    model's known messages matched whole: a write failure `Could not save
    to My library. Check that its folder can be written.`, a removed item,
    a built-in one (`Not changed: the item is built in.`, no rule), no
    project, an item from no library (`Shop PETG is up to date.`); any other in its own words by plain names, paths left
    out, when short, else `Could not change Shop PETG. Check its values
    and try again.`). Every line field has one white fill, set or
    empty (only the placeholder is grey); a disabled field's grey fill is
    for read only. New items are named alike: `Printer 1`, `Process
    1`, `Material 1`, then `… 2` (never `New …`, which read as 3D
    Print's New process…, mitcad#828); a copy `<name> (copy)`, then
    `(copy 2)`.
  - **Two states** (mitcad#655, the owner's option A): an item is
    built-in (MitCad's, read-only) or the user's own; the lists and the 3D
    Print and Property Sets boxes show no places, no place icons or
    headings. Both windows have one header above the tabs (`itemHeader`):
    the item's name to edit (bold), then its states as quiet grey text
    next to it: `● Used in this design`, `In My library` or `In this
    project's library` for a saved one (`showSavedState`), `Built-in` with
    a lock and a **Duplicate** button for a built-in item (no tooltip);
    else Save to. The design's copy of a library item is never
    shown as such: the editors show the values the design prints with
    (`designFields`). Read only disables the fields, never hides them.
    The editor side is never blank: Default shows its name and swatch,
    read only; nothing selected a quiet line (`No material selected`, `No
    printer or process selected`).
  - **New and Duplicate** make the item in this design and make it the
    design's at once, in one undo step: a new or duplicated printer or
    process becomes the design's (`create_print_profile` with `choose`), a
    material is selected in Materials (Assign gives it to the bodies). A
    copy is named after its source (`PLA Plastic (copy)`). Nothing is written
    to a library until Save to; Duplicate of a built-in item is the way to
    change one.
  - **Save to** shows only for the user's own item that is in no library
    yet (`LibraryItem::unsaved`): `Save to My library` without a project,
    in a project `Save to ▾`, a menu of My library and This project's
    library that starts on the place chosen last (`print/creationPlace`),
    logged `<window> menu: My library | This project's library`
    (`choosePlace`). A saved item shows its state instead. Its edits change the library's file and the design's copy as
    one undo step: Undo restores the file too.
  - **Delete** is always asked, the same in both windows
    (`confirmDelete`), box titled Delete, one verb with the button, and
    says who it concerns and what follows, never the root as a component
    and never a rule. An item saved in a library is removed from there:
    `Delete Shop PETG from My library? This design keeps using it.` (the
    second sentence while the design uses it); the design keeps using its
    copy as its own unsaved item (Save to shows again). The design's
    printer or process: `Delete Fine from this design? This design then
    uses Slicer default.`; its material: `Delete Shop PETG? Body1 and
    Body2 then use Default (Steel).` (`3 bodies` for more; the bodies
    without a material of their own when it is the root's default).
    Logged `<window> delete:
    <question>` and `<window> delete cancelled`.
  - **Changed in another design**: a saved item whose library item
    another design changed (the row's `changed`) is asked about once,
    when the design uses it (`usedByDesign`: a window shows it, 3D Print
    has it as the design's printer or process) or it is edited, never on
    a mere selection (`askChangedElsewhere`): `Shop PETG was changed in
    another design. Use the changes or keep this design's settings?` in a
    box titled Changed in Another Design, **Use the changes**
    (`update_from_print_library`) or **Keep this design's settings** (the default, also
    Escape; `decline_print_library_update`, which the design
    remembers until the library changes again), one undo step either way;
    not asked again in the session. Logged `<window> changed elsewhere:
    <name>` and `<window> changed elsewhere: use|keep`.
- **Edit Materials** (`solid.materials`, SOLID › MODIFY next to Edit
  Appearances; the selected bodies are its targets,
  `MainWindow::openMaterialsForSelection`; Physical Material's old id
  `solid.material` is an alias of it, mitcad#655: one command for the
  window) and **Material...**
  (the browser's menus of a body after Appearance... and of a component
  after Design Policy...; the view's menu after Appearance...: the bodies
  of the selected bodies, faces, edges and vertices, a component selected
  alone its default material; a body row's, like its Appearance..., every
  selected body when the row is one of them, else the row's body alone,
  `BrowserController::menuBodies`, mitcad#703; a component row's, that
  component alone) call `BrowserController::openMaterial`:
  `MaterialsDialog` (`print/MaterialsDialog.*`, a non-modal window titled
  Materials that follows the model). On the left the materials grouped
  by their class in Metals, Plastics (the filaments too) and Other (the
  rows' `group`; print settings never move one), the built-in ones
  first, then by name, each with its appearance's swatch; the list is
  narrow. Opened for targets the top row is
  Default with what it gives and its swatch (`Default (Steel)`, or the
  root's default material for a part of a component: no material of its
  own, the same word for bodies and components), a line under it, the
  targets' material selected; bodies of different materials select
  nothing and show `2 materials` beside Assign, which waits for a choice
  (never Default, which would give them all the same; logged `Materials
  for bodies …: mixed, 2 materials`). **Assign** reads `Assign to 2
  bodies`, `Assign to Body1` or `Assign to Arm` (a double-click on a row
  assigns as Assign does, mitcad#861): for bodies it asks, when
  the material's appearance would replace one set by hand (the `bodies`
  query's `appearance_set_by_hand`), in a box titled Replace Appearance
  (Replace, Keep, Cancel), then `set_body_material` per body with
  `appearance` and the item's library (one undo step `Material`; a single
  body `Set Material of Body1`), for a component
  `set_component_material`; Assign closes the window (not when opened from
  3D Print's Assign…). Opened for bodies, the assignment panel under the
  list (mitcad#861, `AssignmentPanel`;
  [Bindings](#bindings-varies-with-suppress-when-show-when)) shows their
  material and **Varies with**: with an option, Assign, a double-click,
  New and Duplicate fill its rows (`set_body_material` with `expression`,
  a library's material copied in by the same undo step), the window stays,
  and `2 materials` gives way to the panel's Mixed notice; with None the
  window is as before. The window opens 680 pixels high, so that the list
  keeps its height under the panel's header line; Default alone shows its
  name at the top of the right pane. On the right the
  header (the name to edit, F2 on a row puts the keyboard there;
  Built-in; Save to), then the tabs: **General** (Class, Metal, Plastic or Other,
  `edit_material`'s `class`; Density in g/cm³, a value that is not above
  0 refused under the field; Appearance, the appearances' swatches with
  a search, None first, Edit Appearances… last) and, for a plastic only, the
  settings form of the filament set (Filament, Cooling, ...), without
  `filament_density` (Density stands for it). A metal or an other shows
  General alone: no print settings. Each change is
  `edit_material` with the item's library (the library's file and the
  design's copy, one undo step). Built-in ones are read only: their
  fields and Delete are disabled, Duplicate is the way on. **New ▾** asks
  the class (`Materials new menu: Metal | Plastic | Other`):
  `create_material` in this design with `class` and its class's typical
  density (7.85 g/cm³ for a metal, 1.24 for a plastic; another material
  has none typical, so Density takes the keyboard); **Duplicate**
  `create_material` in this design with `based_on` (the copy keeps the
  source's class; the design's copy's id when it has one, else
  `{library, id}` of a library's item, else its id; the model names the
  copy `PLA Plastic 2`); the new row is selected and its Name takes the
  keyboard. Delete is `delete_material` with the library. Logs `Materials
  opened` (`for bodies F1.b0`, `for component C0`), `Materials for bodies
  F1.b0: <material id>` (`default`), `Materials assign label: Assign to
  Body1`, `Materials: <n> items` with `Materials row: <group>/<id> <name>
  (<place>)[ changed]`, `Material <name> value <key> = <text>`, `Materials
  item <name> at x,y` (`default` for Default), `Materials <what> at x,y`
  (`name`, `save_to`, `header_duplicate` beside Built-in; `class`,
  `density`, `appearance` on General;
  `new`, `duplicate`, `delete`, `assign`, `my_library`, `close`;
  **My Library…** beside Close opens My Library (below), logged
  `Materials: My Library…`),
  the form's `Materials settings field <key> at x,y` (`clear <key>`, `tab
  <category>` with `tab general`, `search`, `advanced`), `Material selected: <name>
  (<place>)`, `Material <name> (<place>) <field> = <value>`, `Material
  created: <id> in design`, `Material duplicated: <name> as <id> in
  design`, `Material deleted: <name> (<place>)`, `Material saved to
  <library>: <name>`, `Materials changed elsewhere: <name>` (`use`,
  `keep`) with `Material <name>: <cmd>`, `Material: replace the
  appearance of <names>?`, `Material <id> set for <n> body(ies),
  appearance replace|keep`, `Default material of <component>: <id>`, and
  refusals `Materials: <cmd> refused: <reason>`
  (`tools/ui-material-test.sh`, `tools/ui-print-settings-test.sh`,
  `tools/ui-modify-test.sh`).
- **Printers and Processes** (`make.printers`, Edit Printers and
  Processes… in MAKE next to 3D Print, `MainWindow::printersCommand`:
  the design's process selected, else its printer; Edit of 3D Print's
  and Property Sets' Printer and Process rows,
  `BrowserController::openProfiles`; `print/ProfilesDialog.*`): a
  non-modal window laid out as Materials that follows the model. On the
  left the items under Printers and Processes by name, the design's
  choice marked (no filter); on the right, with the values the design
  prints with, the header as Materials' and the settings
  form of the printer set (one category, Printer: printable space and
  nozzle, without a tab bar: a form shows its tabs only when there are
  two or more) or the process set (Quality, Strength, Speed, Support,
  Others). Each field is
  `edit_print_profile` with the item's library and kind (undo step `Edit
  Process 0.20 mm Standard`; a saved one's file and the design's copy
  together). Under the list: **New ▾** (Printer or Process:
  `create_print_profile` in this design with `choose`, the design's
  choice in the same undo step), **Duplicate** (`create_print_profile`
  with `based_on` and `choose`: the user's own copy, the design's
  choice), **Delete** (`delete_print_profile`, asked); **Use in this design** on the
  right makes the selected one the design's printer or process
  (`set_property` of the root's `profile`, one undo step; shown only
  for an item the design does not use; logged `Profile used in this design: <kind>
  <name>`, its place `Profiles use at x,y`). Edit on Slicer default opens
  the window with nothing selected, the same from 3D Print and Property
  Sets. **Import from Slicer…** at the top right (mitcad#829, where My
  Library has it) is 3D Print's Import from slicer…, the one
  implementation the three windows share (`print/SlicerImport.*`,
  `SlicerImportBar`): the same file dialog, `import_print_profile` with
  `into` `design` (the root chooses the printer and process read, one
  undo step `Import Print Settings plate.3mf`), its line (`plate.3mf
  imported`) and **Details…** (Import Details) before it; the process
  read (else the printer) is then selected in the list, the design's,
  "Used in this design"; Save to puts it into a library. It works while
  3D Print's panel is open too (`canChangePrintSettings`). Logged as 3D
  Print's: `Imported print settings from <file>: printers: …`,
  `Profiles import: <line>` (refused: `Profiles import refused:
  <reason>`, the reason on the line), places `Profiles import at x,y`,
  `Profiles details at x,y`. Logs `Printers and processes
  opened`, `Profiles: <n> items` with `Profiles row: <kind>/<id> <name>
  (<place>)[, the design's]`, `Profile selected: <kind> <name> (<place>)`,
  `Profile shown: <kind> <name> (<place>)[, read only]` with `Profile
  <name> value <key> = <text>`, `Profiles item <name> at x,y`, `Profiles
  <what> at x,y` (`name`, `save_to`, `header_duplicate`, `new`, `duplicate`, `delete`,
  `use`, `my_library`, `close`; My Library… logged `Profiles: My
  Library…`), the form's
  `Profiles settings field <key> at x,y` (as Materials'), `Profiles new
  menu: Printer | Process`, `Profile <name> (<place>) <key> = <text>`,
  `Profile created: <kind> <id> in design`, `Profile duplicated: <name>
  as <id> in design`, `Profile saved to <library>: <name>`,
  `Profile deleted: <name> (<place>)`, `Profiles changed elsewhere:
  <name>` (`use`, `keep`) with `Profile <name>: <cmd>` and
  refusals `Profiles: <cmd> refused: <reason>`
  (`tools/ui-print-settings-test.sh`).
- **My Library** (`tools.my_library`, Tools › My Library… after Keyboard
  Shortcuts, also in the command search and while a panel is open; My
  Library… of Materials and Printers and Processes;
  `BrowserController::openMyLibrary`, `print/MyLibraryDialog.*`,
  mitcad#704): the user's own materials, printers and processes without a
  design, in a non-modal window titled My Library. Tabs **Materials**,
  **Printers** and **Processes**, each the panel of the design's window
  (`MaterialsPanel`, `ProfilesPanel` with one kind) on the library: its
  items (Materials by class), then the built-in ones under **Built-in**
  (read only, the way to Duplicate); no Default, Assign, Save to, Use in
  this design or saved state. In a project a box at the top chooses **My
  library** or **This project's library** (`user`, `project`; My library
  never names the project). The panels work through a model of the
  window's own, an empty design never shown or saved
  (`LibraryItemsHost`): **New** (Materials: New ▾ with the class;
  Printers and Processes: New, of the tab's kind) is `create_material` or
  `create_print_profile` with `library`; **Duplicate**, also of a built-in
  one, the same with `based_on` (a built-in material by its id, a
  library's item as `{library, id}`; the copy is named `PLA Plastic 2`);
  the name (F2 on a row), the class, density, appearance and the settings
  are `edit_material` or `edit_print_profile` with `library`; **Delete**
  (asked, `Delete Bench PLA from My library?`) `delete_material` or
  `delete_print_profile` with `library`. A material's appearance that is
  not MitCad's, which the library's item carries, shows as an entry of
  its own with the item's colour. **Import from Slicer…** at the top
  right reads a slicer's 3MF or preset into the library shown
  (`import_print_profile` with `into` `user` or `project`; 3D Print's
  import, `SlicerImportBar`, mitcad#829). With an installed Bambu-family
  slicer found (mitcad#830, `slicer_presets`: Bambu Studio, OrcaSlicer,
  QIDI Studio; the Flatpak's too; one with presets of the user's or with
  printers enabled in its setup) the button is **Import from Slicer ▾**,
  a menu of **From a file…** and **Presets of Bambu Studio…** for each
  slicer found (`OrcaSlicer (Flatpak)`; logged `My Library import menu:
  From a file… | Presets of Bambu Studio…`), looked for again at each
  click; Printers and Processes has the same. Presets of… opens **Import
  Presets** (`print/SlicerPresetsDialog.*`, modal): the slicer at the top
  (a box when several were found), under it **Filter presets** (a field:
  the rows whose name contains its text in any case stay, a group with
  none hides and those with some open while it filters, and clearing it
  brings back which groups were open; a hidden row keeps its tick, which
  still counts and imports, and a group's box then ticks only the rows
  shown; Down or Enter goes to the list, Shift+Tab from the list to it),
  the user's presets under **My printers**, **My processes** and **My
  filaments**, then the slicer's own presets the user enabled there
  (mitcad#871: the printers chosen in its setup, their processes and the
  filaments chosen there, each kind by name in any case; `system` in
  `slicer_presets`) under **Bambu Studio printers**, **Bambu Studio
  processes** and **Bambu Studio filaments**, collapsed at first (Right
  or the arrow opens one) unless a row in it has a same name, and
  unticked, a box each, each group's box ticking all or none of it. A
  preset whose name the target (the library shown; the design in
  Printers and Processes) has already is in the warning colour with the
  tooltip "My library has a process of this name", unticked, with
  **Same name:** and its own **Copy** (the default: `Fine 2`) or
  **Override** (the existing item's next version) beside it (accessible
  name `Fine: same name`; the row's and the box's accessible description
  say what it meets and what Copy and Override do); choosing in it ticks
  the row. **All same names:** above the list sets every such row's
  choice (the ticks stay), shows their choice when they agree and no
  value when they differ, and shows only with such rows. New user
  presets are ticked. The name column is as wide as the names and the
  group titles, each choice beside its name. Keyboard: the list has it
  at first on its first user preset (else its first group), the current
  row framed; the arrows move it, Space ticks or unticks it (a group:
  all of it), Tab goes to the row's choice (Shift+Tab back) and on to
  Import. **Import** says how many are ticked (`Import 1 Preset`,
  `Import 3 Presets`; plain `Import`, disabled, with nothing ticked;
  Alt+I) and runs `import_slicer_presets` with
  the ticked ones (their inherited values filled in from the slicer's
  system presets; into the design one undo step `Import Presets of Bambu
  Studio`), the line saying `2 presets imported into My library`, for
  one preset its name (`Fine imported into My library`, after a Copy
  `Fine imported into My library as Fine 2`; into the design `Fine
  imported as Fine 2`), and Details… the report; My Library then shows
  the tab of the first item that came in with that item selected
  (`My Library tab: processes`, `Profile selected: process Fine (user)`,
  `Material selected: …`). Cancel imports nothing (`Slicer presets
  cancelled`). Logs `Slicer presets opened: <slicer>, <n> presets`,
  `Slicer presets row: <kind> <name>, ticked|unticked[, system][, same
  name: <tooltip>, copy|override]`, `Slicer presets ticked: <n>, <Import's
  text>`, `Slicer presets tick: <name> ticked|unticked`, `Slicer presets
  choice: <name> copy|override`, `Slicer presets same name:
  copy|override`, `Slicer presets all same names: copy|override|none`,
  `Slicer presets filter: <text>, <n> shown` (`Slicer presets filter
  cleared, <n> shown`), `Slicer presets current: <name>`, `Slicer
  presets focus: <name> choice` and the places `Slicer
  presets <what> at x,y` (`slicer`, `same_all`, `import`, `cancel`,
  `item <name>`: its box) (`tools/ui-my-library-test.sh`). Its line (`plate.3mf
  imported into My library`) and **Details…** (Import Details) beside
  it. The open design does not change: when the window closes after a
  change, the open Materials and Printers and Processes windows read the
  libraries again, and a changed item the design uses is asked about
  there as another design's change (`changed elsewhere`); the window
  reads the libraries again whenever the design changes. Logs `My
  Library opened` (` in a project`), `My Library place: user|project`,
  `My Library tab: materials|printers|processes`, `My Library tab <tab>
  at x,y`, `My Library <what> at x,y` (`place`, `import`, `details`),
  `My Library import: <line>` with `My Library imported into <library>:
  <kind> <name>, …` (refused: `My Library import refused: <reason>`),
  `My Library closed` (` after changes`); each tab's panel logs as the
  design's windows, headed `My Library materials`, `My Library printers`,
  `My Library processes` (`My Library materials: <n> items`, `… row:
  <group>/<id> <name> (<place>)`, `… item <name> at x,y`, `… <what> at
  x,y` with `close`, `… settings field <key> at x,y`, `… new menu`, `…
  delete: <question>`, `…: <cmd> refused: <reason>`), and the actions as
  there with the library as the place: `Material created: <id> in user`,
  `Material duplicated: PLA Plastic as pla-plastic-2 in user`, `Material
  <name> (user) <field> = <value>`, `Material deleted: <name> (user)`,
  `Profile created: printer <id> in user`, `Profile duplicated: <name> as
  <id> in user`, `Profile <name> (user) <key> = <text>`, `Profile
  deleted: <name> (user)` (`tools/ui-my-library-test.sh`,
  `tools/ui-windows-my-library-test.ps1`, ctest
  `app.ui-windows-my-library`).
- **Property Sets' print settings** (mitcad#640, mitcad#655): under the
  levels, **Print settings** sums up what the most specific level prints
  with (`print_settings` of the body in its occurrence, the occurrence or
  the component), read only: the values are edited in the settings form.
  Process shows the profile chosen: a click opens a drop-down of the
  choosable items by name, no places (choosing sets that
  level's `profile`, with the item's library), **Edit…** after it opens
  the process in Printers and Processes, ↺ takes a choice of that level
  away (Edit… and ↺ take the left button only, mitcad#761: a right click
  on them does nothing). Below the root the process lists its own values only, and a row
  **Own settings** of its own names what bodies and components set of
  their own (`Walls`, `3 settings`, `None`; logged `print Print
  Process/own_settings = Walls`), with those values as rows under it,
  each owner as its Source; its pencil opens the level's own values in
  the settings form (`openOwnSettings`, below); the root has none, its
  values are the process. Printer at the root likewise, its Edit… opening Printers and
  Processes; below the root, a plate having one printer, it is one line
  without values (`Bench X`).
  Material shows the body's material (not again in Source): a click or
  Edit opens Materials for the body or the component. Below Printer (at
  the root), Process and Material the values under one heading line per
  category › sub-group (small and bold on a band of the row's width, so
  it does not read as a value), in the key table's order, in the key's own
  unit (a colour as a bordered swatch with its appearance's name, the
  appearance's when unset; Yes and No as in the editors; a value per
  slicer and the filament's density not shown), no Type, and the source
  once: the group's on its row, a row's own where an owner sets it (the
  level shown too: `Body1`), in words with its kind's icon; the name column as
  wide as the names. The level's own values are bold on a tinted row;
  unset ones are the placeholder's grey in italics; computed ones (the
  levels') grey upright; the computed values do not repeat the material.
  A body's level is headed by its name alone (`Body1`). The per-level
  `Print Printer`, `Print Process`, `Print Filament` and `Print Slicer
  (…)` sets are not listed again among the levels. The row names are
  `print <set>/<key>` in the log (`Property sets row: print Print
  Process/layer_height = 0.2 mm (profile Standard (Root))`); the places
  `Property sets print <set>/<key> value at x,y`, `… edit at x,y`, `…
  clear at x,y` (`print Print Process/profile`, `print Print
  Filament/material`).
- **Own settings** (mitcad#655; `print/OwnSettingsDialog.*`,
  `BrowserController::openOwnSettings`): a body's, an occurrence's or a
  component's own process values in the same tabbed settings form as a
  process, as a slicer's per-object settings, in a small non-modal window
  titled `Body1 · Own settings` that follows the model. Every process key can
  be set from its tab; the owner's own values show set, the others show
  what it prints with as their placeholders. A change is `set_property`
  of the owner's `Print Process`, emptying a field or ↺ `remove_property`,
  one undo step each. Opened from 3D Print's own-settings links and
  Property Sets' Own settings. Logs `Own settings opened: <title>`,
  `Own setting <owner> <key> = <text>` (`cleared`), refusals `Own
  settings: <key> refused: <reason>`, the form's `Own settings field <key>
  at x,y` (`clear <key>`, `tab <category>`, `search`, `advanced`) and `Own
  settings close at x,y` (`tools/ui-print-settings-test.sh`).
- **Change Parameters, limits** (mitcad#665; the model's side:
  [Parameter limits](../core/model-api/src/commands.md#parameter-limits)):
  the columns Min, Max, Step, Strict and Slider come after the star (the
  first six keep their numbers, which UI scripts use). Min, Max and Step
  are expressions edited in place (`set_parameter` `min`/`max`/`step`; an
  emptied cell removes the limit); Strict is a check box (`strict`). A
  parameter without a limit of its own shows the limits of the feature
  that uses it (the query's `intrinsic`, such as a pattern's quantity: 1
  to 10000, whole) in grey, with the feature in the tooltip; leaving such
  a cell unchanged sets nothing. A limited value is marked in its Value
  cell (`372 mm → 300 mm ▲max`, `▼min`, `◆step`) with the model's
  `limit_message`, the limit's expression and the parameters it uses in
  the tooltip, and the message line says it after the change. While an
  expression of a parameter with limits is typed, the `evaluate` query
  with `parameter` says at once that it will be limited, or refused by a
  strict parameter. A parameter with a minimum and a maximum of its own
  gets a `QSlider` (its steps when it has a step and at most 10000 of
  them, else 1000 positions); a released handle or a key sets the
  expression to the number in the parameter's unit, a click on the handle
  alone sets nothing. "Only limited" shows only the rows a limit changed.
  A feature panel's value field whose own parameter (when editing) has
  limits shows the same mark as its hint (`→ 300 mm ▲max`) and says it in
  its tooltip (`CommandSession::limitHint`), and a limited sketch
  dimension's tooltip ends with the mark and the model's message (the
  `sketch` query's `limited` and `limit_message`). Test:
  `tools/ui-parameter-limits-test.sh`.
- **Configurations** (mitcad#807; the design note
  [docs/configurations.md](../docs/configurations.md), the model's side:
  [Configurations](../core/model-api/src/commands.md#configurations-options-rows-and-rules)):
  `solid.configurations` (Configurations…, beside Change Parameters in
  SOLID > MODIFY and in the Edit menu) opens `ConfigurationsDialog`, a
  window of its own (not modal, as Change Parameters; the same in the
  Floating and Docked layouts) with the tabs Options, Rows, Rules and
  Bindings (mitcad#862). Every
  edit is one model command and one undo step; a refused one stays in its
  cell in red with the reason, and the message line says it.
  - **Options**: the options in choice order. Rows are dragged into their
    order (`InternalMove`; items take no drops, so an option never lands
    under another) or moved with Move Up and Move Down; either is
    `set_configurations` with the new `options`. Add Option… offers the
    user parameters that are no option yet and New Parameter (name, unit,
    value): `add_parameter` (for a new one) and `set_configurations` as one
    undo step `Add Option <name>`. Remove Option takes it out of `options`
    and out of the rows (the parameter stays). Domain (double-click or Edit
    Domain…: List, one value per line, the parameter's `values` limit;
    Range, its `min`, `max` and `step`; Table Column (Alt+T, mitcad#849),
    a column of the design's tables as the `values` limit `Table.Column`,
    offered from `formula_help`'s `tables` by the option's kind (a text
    option text columns, a number option the others); Free; a text option
    is a list, a column or free) is one `set_parameter`, sending only the
    limits that change. A column domain shows as the column and its values
    (`Screws.Size: M3, M4, M5`; `configurations::valuesColumn`).
    Default is the option's own value (its parameter's constant), edited
    through `configure`, so the cascade runs; Comment is `set_parameter`
    `comment`; Driven marks an option whose expression is a formula (the
    `configure` query's `driven`): its Default is not edited.
  - **Rows**: a `QTableWidget` with Name, a column per option (texts
    without quotes, numbers as their expressions), a closed table's other
    `parameters`, Designation, Part Number and Supplier URL; Closed; Add
    Row (the next free `Row<n>`, with the options' expressions as they are
    now), Generate Rows from Table… (mitcad#849: the window `Generate Rows
    from Table` chooses a table of the design, its key column and an option
    or a user parameter, offering first the column an option's values come
    from and that option (`configurations::keyOption`); Generate sends
    `generate_configuration_rows`, one undo step, and the message line says
    which rows it added or updated, with any moves; disabled while the
    design has no tables), Delete Row, Set as Default (the default row's
    name is bold) and Apply (`apply_configuration`; the moves in the message
    line and the status bar). A cell edit is `set_configurations` with the table; a typed
    text gets quotes for a text option and the unit for a bare number
    (`configurations::rowExpression`). Before an option's value is written,
    the row's combination is tried with the `configure` query's `values`
    (nothing changes): a `refused` answer refuses the cell with the rule's
    name and message, and nothing is sent (decision 4: rules hold
    everywhere). Each row is tried so whenever the window reads the model:
    a row that breaks a rule is marked in the cells of the options the
    refusal names (else in its name) with the rule in the tooltip. The line
    above the sheet names the row the options match (`row` of the query).
  - **Rules**: `makeConfigurationRulesPage` (`browser/ConfigurationRulesPage.cpp`,
    a `ConfigurationPage` the window refreshes with itself): the rule block
    editor of mitcad#808, [The rule block editor](#the-rule-block-editor).
  - **The Configuration bar** (`ConfigurationBar`): the top of the browser
    (`BrowserPanel::setTopBar`: counted in `contentHeight`, so the
    Floating card grows with it, and a box of its own in the Rows
    background's `rowsRegion`), hidden while the design has no options and
    no rows. A row combo (`apply_configuration`; `No row` while the options
    match none), then a chooser per option in choice order from the
    `configure` query: a combo of the `allowed` list, a field and a
    `QSlider` for an `allowed` range with both bounds (positions by its
    step, else 100; a released handle or a key sends the number in mm or
    rad), a field alone for a range without both bounds or a free option,
    the value alone for a driven one. A value the rules do not allow now is
    shown but not offered. A choice is `configure` with that option's value
    (a text option's text, a number option's expression with the unit for a
    bare number: `configurations::chooserValue`), one undo step. The
    answer's `moves` show as a notice under the choosers and in the status
    bar (`Length 1500 mm → 1000 mm: Black is short`); a `refused` answer or
    a refusal shows in red. The widgets are made again only when the
    choices change; refreshed with the browser.
  - **Change Parameters** marks an option with the configurations icon on
    its name (the tooltip: an option, driven); an option the rules narrow
    now (`restricted_by`) shows the narrowed range's bounds in grey in
    Min, Max and Step where the parameter has no limit of its own (as a
    feature's fixed limits), else in their tooltips, and a narrowed list in
    the Value tooltip.
  - **The JSON** is read and made in one place,
    `browser/ConfigurationsData.*` (plain Qt Core, `app.unit`:
    `app/tests/configurations_data.cpp`): the table (what the window does
    not edit, `rules` and unknown fields, goes back unchanged), the option
    reports, moves and refusals, and the commands.
  - **Bindings** (mitcad#862, the fourth tab; `ConfigurationBindingsPage`,
    a `ConfigurationPage` the window refreshes first, [Bindings at a
    glance](#bindings-follow-option-suppress-when-show-when)): every
    binding of the design as a table, Item (with the browser's icon of a
    body, a feature's type or an occurrence), Property (Appearance,
    Material, Shown, `Face 1 (side)` with the face's topological name in
    its tooltip, Suppressed), Follows (`Color: Black → Paint - Black, …`,
    `when Holes = Yes`, or the expression, as the card reads them; the
    column that stretches) and Now (what it gives now, with its swatch).
    **Driven by** (All options, or one of the parameters the bindings use)
    and an item search filter it. A binding whose results name no
    appearance or material (`no appearance "paint_purple"`), or that the
    model warns about, is a warning row (yellow, the warning icon in Now);
    the problem is in the row's tooltip and, while the row is selected, in
    the status line (`⚠ no appearance "paint_purple" — 6 bindings, 1
    warning`). The first row is selected when the tab fills with none
    selected, so that the buttons work at once. **Edit…** (or a
    double-click) opens the fx card's editor (`BindingActions::edit`:
    `BrowserController::editBinding`); **Select in Browser** selects the
    item and shows its row (a feature in the timeline too); **Remove
    Binding** sends the property's command with an empty `expression`
    (the value it gave stays, one undo step: `Clear Appearance Expression
    of Body2`). The status line counts them (`6 bindings, 1 warning`, `2
    of 6 bindings`). On **Options**, **Used By** says per option `used by
    N bindings` (the column as wide as its text; the users in its
    tooltip; a double-click shows the Bindings tab driven by it). Logged:
    `Configurations binding Body1 Appearance: <follows>; now <now>[;
    warning <w>]`, `Configurations bindings status: 6 bindings, 1
    warning`, `Configurations bindings selected warning: no appearance
    "paint_purple"`, `Configurations bindings
    driven by Holes, search '': 2 of 6 bindings`, `Configurations option
    Color used by 4 bindings`, `Configurations binding edit|select: Body1
    Appearance`, `Configurations binding removed: Body2 Appearance` (`…
    refused: <reason>`), and the places `Configurations binding Body2
    Appearance at x,y`, `Configurations bindings driven by|search|edit|
    select|remove at x,y`. Test: `tools/ui-bindings-glance-test.sh`.
  Tests: `tools/ui-configurations-test.sh` (Linux: two options and a row
  made and applied, a choice in the bar, a rule's move as a notice, a row
  value refused with the rule's name), `tools/ui-configuration-tables-test.sh`
  (Linux, mitcad#849: a table column as an option's domain, rows generated
  from the table, a combination in table rule built and saved, and a
  generated row applied with the rule's move), `app.ui-windows-configurations` and
  `app.ui-macos-configurations` (the driver script
  `tools/ui-scripts/configurations.mitcad-ui`: the window opens, a choice
  in the bar changes the body).
- **Property Sets** (mitcad#496): `view.property_sets` (the command
  search: the selected body or component, else the root), the browser's
  menus of a body (after Appearance...) and of a component (after Design
  Policy...) and the view's menu (after Appearance...) call
  `BrowserController::openPropertySets` with a `PropertyTarget` (a body,
  an occurrence path, a component): `PropertySetsDialog`
  (`browser/PropertySetsDialog.*`), a non-modal window titled Property
  Sets, refreshed with the model. It shows the `property_sets` query's
  levels (body, occurrence, component) as top rows, their sets foldable
  below them (folded sets stay folded), each property's key, value (in
  the shown unit), type (an own type's tooltip names its base) and source
  (`ifc, edited` with the source's value as the tooltip; `inherited` for
  an occurrence's component values; `overrides`; computed values greyed,
  read-only). Computed values (check box, on) asks `computed`. A
  double-click or F2 edits a value in place: text typed as the model reads
  it (`1.2 m`, `500 g`, `yes`); an own type with allowed values, and a
  yes/no, as a drop-down over the cell whose change sets it (its arrow
  keys and wheel work without opening its list). Editing an inherited
  value sets the occurrence's own. Typing a key's first letters goes to
  its row (the tree's keyboard search). Add... (level, set, key, type,
  value), Remove (an own stored value; an occurrence's component value
  shows again) and Types... (`Property Types`: the own types with their
  values; Define with name, base type, comma-separated values, unit,
  description; Remove) run one model command each; refusals stay in the
  window's message line.
- **Find in Design by property** (mitcad#496): a search text with a
  comparison (`=`, `<`, `>`, `~`: `FireRating = EI60`,
  `Pset_WallCommon/FireRating = EI60`, `Mass > 10 kg`, `Name ~ wall`)
  asks the model's `property_search` (`FindDialog::setPropertySearch`, set
  by the controller) and lists the bodies and occurrences it found
  (`FindOptions::propertyOwners`): an occurrence by its own or inherited
  value, a body in each of its places; the component and visibility
  filters still apply. A search the model cannot read finds nothing and is
  logged.

### The rule block editor

The Rules tab of the Configurations window (mitcad#808; design:
docs/configurations.md, "Block editor", decision 6) is native Qt, with no
dependency of its own: `configurations/RulesPage` (the tab), on
`configurations/RuleCanvas` (the palette and a `QGraphicsView` canvas)
and `configurations/RuleBlocks` (plain Qt Core: a rule as blocks, read
from and written to the model's JSON). `browser/ConfigurationRulesPage.cpp`
gives the window `makeConfigurationRulesPage`, a `RulesSource` on its
`DocumentHost`: the `configurations` query (the table and its
`problems`), the `configure` query (which options are numbers), the
`check_rules` query, `formula_help`'s `tables` (the design's tables and
their columns, for combination in table), and `set_configurations` with the table as the model
has it (through the configurations' JSON adapter, so the options and rows
go back unchanged) and the page's `rules`; Undo and Redo are the model's
`undo` and `redo`.

- **The list**: a rule per row, its name (edited in place: double-click,
  F2; Add opens the new rule's name at once; an empty or taken name is
  refused), the enabled check and a warning mark whose tooltip says why:
  `Not saved: fill its empty sockets`, `Not saved: its else has no action`,
  `Not saved: <the model's refusal>`, or the model's `problems` that name
  the rule. Add, Duplicate (`<name> 2`), Delete; Insert and Delete in the
  list.
- **Blocks** (`RuleBlocks`): a rule's `when`, `require` and the values of
  its actions are expression text in the model. Each is read into blocks:
  compare (`==`, `=`, `<>`, `!=`, `<`, `<=`, `>`, `>=`, one at the top),
  `and`/`or`/`not` (any case), is one of (`or(x == a; x == b; …)`: one name
  compared with values), names, numbers with a unit (`1000 mm`, `-5 mm`,
  `10 mm^2`), texts, table columns (`Colors.Name`), parentheses around any
  of them. Everything else is a formula block holding its text. A block
  keeps the exact text it was read from and prints it while neither it
  nor a block in it changes, so a rule that is opened and saved keeps its
  text character for character; an edited block prints canonically
  (`Color == 'Black'`, `and(a; b)`, a comparison in a comparison in
  parentheses). An action object of the JSON becomes a block per key
  (`{"option": "Depth", "min": …, "max": …}`: min and max) and is written
  back as one object; an action or field the editor does not know is kept
  as it is (`OtherAction`, the rule's other fields, `comment`).
- **The canvas**: the rule's frame with `when` (or `always`), `then` and
  `else`; value blocks with a plug on their left, action blocks with a
  notch and a tab, empty sockets and the end of a list (`+`) dashed, a slot
  before each action and `+ action` after the last. A value or logic block
  fits only a value socket (an empty one, or a value it replaces, or the
  `+` of and, or, is one of, allow only, exclude); an action fits only a
  then or else slot; the Rule group's entries (when … then, then (always),
  else) apply to the whole rule. A palette entry dragged over the canvas
  highlights where it would go (by the mouse's own press, move and
  release: no platform drag and drop); a block is moved the same way, and
  deleted when dropped on the palette. A new name, number, text, column or
  formula opens its editor at once (a name with the first option, an
  action with the first option, min, max and step with the first number
  option); fields (an action's option, a comparison's operator as a menu,
  a message, a table, its columns `Color=Color, Length=Length`) open on a
  click, blocks on a double-click; a formula opens in the Formula Editor
  (`FormulaEditor`, any kind) and is read into blocks again.
- **Combination in table** (`in_table`, mitcad#806, #849): a new block
  opens its table at once, completed from the design's tables as it is
  typed. A table of the design maps each option to its column of the same
  name (else of the same name in another case), keeping the mappings the
  table still has (`rules::inTableColumns`), so the block and its table are
  one complete edit that the model takes. Its columns field opens `Table
  Columns`, a column of the table (or none) per option; for a table the
  design does not have, the columns are typed (`Color=Color,
  Length=Length`).
- **Keyboard**: the palette takes Enter on an entry (put at the canvas's
  focus: its socket or slot, else after the focused action, else the
  then's end or the first empty socket); the canvas takes Left and Right
  (the blocks in order), Up and Down (the nearest place in the row above
  or below), Enter or F2 (edit; on a socket or a slot: the palette),
  Delete or Backspace (the block; on `when` or `else`: that section), the
  menu key or Shift+F10 (Edit, Edit as Formula…, Move Up, Move Down,
  Delete; Add When, Apply Always, Add Else, Remove Else). The labels
  `&Rules`, `&Palette` and `&Blocks` and the buttons `&Add`, `D&uplicate`,
  `&Delete`, `View as &Text` and `Chec&k Rules` take Alt.
- **Saving**: each edit is `set_configurations` at once, one undo step
  (`Edit Configurations`), when every rule is complete (a name, no empty
  socket, an else with an action: the model keeps no empty else). A new
  block whose editor opens at once and the value typed in it are one
  edit, said when the editor closes (as the block came when Esc cancels
  or nothing changed): an empty text is never a step of its own. An edit
  that leaves an empty socket, or that the model refuses, stays on the
  page (marked in the list, a line under the canvas says why) until an
  edit completes it; Ctrl+Z in the page takes such edits back one by one,
  then runs the design's Undo; the platform's Redo keys its Redo. A refresh
  of the window keeps them.
- **View as Text**: the selected rule in the expression language, a line
  per part (`when Color == 'Black'`, `then Length max 1000 mm`, `else …`,
  `always` without a when). **Check Rules**: the `check_rules` report's
  `lines` (else its parts in words), under the canvas; with changes not
  saved yet it says that the saved rules were checked.
- The editor follows the theme's palette (its block colours are the same
  in both themes; sockets and fields use the palette's base and text) and
  is the same in the Floating and Docked layouts.

Tests: `app.unit` (`app/tests/rule_blocks.cpp`: the round trip of every
rule of the design note and of commands.md, and of rules with formula
blocks; canonical text; the edits; the page on a design of the test's
own; a combination in table whose columns follow its table) and
`tools/ui-configuration-rules-test.sh` (Linux: "Black is short"
built from the palette with the mouse in Docked, the Configuration bar's
Length limited by it, View as Text; a rule built with the keyboard only in
Floating; Check Rules lists a value that is never allowed).

## Joints

The ASSEMBLE group of the SOLID tab (mitcad#55, `commands/JointCommands.cpp`;
the model's side in commands.md, "Joints"). A joint goes into the active
component; an edit keeps the joint's own (a hidden `component` input). Its
origins are geometry of occurrences inside that component: picks carry
paths from the root (`O1/O4`), which `build` makes paths from the
component, and `load` makes them paths from the root again through the
component's first occurrence. A pick outside the component fails the
build with a message.

- **Joint** (`assemble.joint`, J): Origin A and Origin B (faces with a
  plane, axis or centre; straight and circular edges and sketch curves;
  points, axes and planes: `givesFrame`), Type, Slide Along (sliders),
  Offset, Angle, Flip, and per free motion of the type a Limits check
  (Minimum, Maximum) and a Rest check (Rest Value). The origin inputs
  leave the origin's planes hidden (`showsOrigin = false`), as they would
  cover the components. Offset's arrow and Angle's ring stand on the frame
  the model resolves origin B to (else A): the `joint_frame` query, so a
  pick shows where its origin snaps. A driven `position` is kept through
  an edit (hidden input) for the motions the type still has; limits with
  a minimum but no maximum cannot be edited (`canEdit`).
- **As-Built Joint** (`assemble.as_built_joint`): Component A and B (in the
  browser, or bodies they place), Type, Motion Origin (optional), limits.
  `relative` is recorded from the `components` query's placements (B's
  inverse times A's); an edit keeps the recorded one while A and B stay.
- **Joint Origin** (`assemble.joint_origin`): geometry of the active
  component itself, Offset, Angle, Flip; a `frame_override` is kept.
  **Mount Frame** (mitcad#525; commands.md, "Mount frames") makes it a
  mount frame of the part, for placing it from a library: Name, Mounts
  (On a face, Into a hole (its shaft), Onto a shaft (its hole), Grid of
  cells, Foot on a grid), Moves (the joint a placement makes: Fixed, Free
  on the face, Turns, Turns and slides, Slides, as the kind takes them),
  and for a grid Pitch X and Y and Cells X and Y, for a foot Pitch X and Y
  and Footprint X and Y; the feature's `mount`. Logged as `Added joint
  origin JointOrigin1: origin (0, 0, 0), normal (0, 0, 1), mount head
  (shaft, cylindrical)`.
- **Rigid Group** (`assemble.rigid_group`): two or more components; a pick
  inside a sub-assembly stands for the occurrence placed in the component.
- **Drive Joint** (`assemble.drive_joint`): a joint (a feature: the
  timeline, the Joints folder, or the newest drivable one), its Motion and
  the value (Angle or Distance, with a ring or an arrow on the joint's
  frame B). It runs `drive_joint` while the inputs change, as sketch
  commands do (`sketchCommand`), and OK keeps it: one undo step `Drive
  Joint1`. Choosing another motion or joint takes its value where it is
  (a choice's `onChange` may set values; those it leaves NaN are evaluated
  again).
- **Constraint** (`assemble.constraint`, mitcad#148; the model's side in
  commands.md, "Assembly constraints"): a `constraint_set` in the active
  component, its rows a `listInput` (`rows`, Add Constraint): Geometry 1
  and Geometry 2 (as Joint's origins: `givesFrame`), Type, and per type
  Second Face (center), Offset (align, insert), Angle (angle), Flip,
  Lock Rotation (inserts), Limits with Minimum, Maximum and Rest (align:
  lengths, angle: angles). Type's first option is Automatic, labelled
  with the type the picks fit best (`Automatic (Insert)`), from the
  model's `constraint_row` query (remembered per pair of picks), which
  `build` writes; the other options set it. Geometry 1's occurrence is a
  ghost until the row's Geometry 2 is picked (`ghostUntilPicked`, the
  row's state). A locked insert records the turn where the picks are (the
  query's `twist`); an edit keeps the recorded one (hidden
  `lock_angle`). The preview solves the rows with the joints and
  constraints before them and shows the moved components; OK logs
  `Added constraint Constraint1: insert placed, moved O2; DOF 1`. In the
  browser a set is a row of the Joints folder (icon `constraint`) with
  its rows under it (`Insert (Pin:1, Plate:1)`, red when a row does not
  hold at the marker); the folder's menu has New Constraint.
  `DocumentSnapshot::joint` gives a set as a joint of the kind
  `constraint`, so drags name it. Test: `tools/ui-constraint-test.sh`.
- **Animate Joint** (`assemble.animate_joint`, `MainWindowJoints.cpp`): the
  joint selected in the timeline, else the newest with free motions, its
  first motion through its limits (from where it is to one end, the other
  and back) or once round without limits (a slide ±25 mm): 36 frames, each
  the `preview` of `drive_joint` to the frame's value from the frame
  before (mitcad#454), shown through the preview's `placements`; the
  preview is cleared at the end and nothing changes. Starting a command
  stops it. A contact of a contact set ends it there: the status bar says
  where, logged `Animation of Joint1 stopped by contact at tx 10.01 mm:
  Body2 (Pin:1) and Body3 (Wall:1) in Contact1`.
- **Contact Set** (`assemble.contact_set`, `JointCommands.cpp`, mitcad#454;
  the model's side in commands.md, "Motion studies and contact sets"): a
  model command (`modelCommand`): Components (two or more; a body picks
  the occurrence that places it, as paths from the root) and Suppressed;
  OK runs `add_contact_set`, or `edit_contact_set` of the set the browser
  opened (its entry goes through `startFeatureCommand`'s analysis argument
  to `load`, which keeps its name in the hidden input `set`). Logs `Added
  contact set Contact1: Pin:1, Wall:1` and `Edited contact set Contact1`.
- **Motion Link** (`assemble.motion_link`, `JointCommands.cpp`, mitcad#466;
  the model's side in commands.md, "Motion links"): a `motion_link`
  feature. Type (Gears, Rack and Pinion, Screw, Ratio, Cam), Driver and
  Follower (joints with free motions, `SelectKind::Feature`, from the
  browser or the timeline) with Driver Motion and Follower Motion (the
  joint's motions), the type's sizes (Driver Gear and Follower Gear with
  Internal Gear, Pinion Radius, Lead, Ratio; each size's "From" input
  takes a round face, circular edge or circle whose radius it becomes
  instead of the value), a cam's Cam Profile (a flat face or a sketch
  curve), Follower (Knife Edge, Roller with Roller Radius, Flat Face) and
  Follower Tip, Reverse, and Keep Where They Are (on, but for a cam: the
  build asks the `motion_link_offset` query for the offset that keeps the
  joints where they are) else Offset (an angle or a length, as the
  follower's motion). An edit loads the link with its offset. Logs `Added
  motion link MotionLink1: rack_pinion, Joint1 rz to Joint2 tx,
  satisfied`. Test: `tools/ui-motion-link-test.sh`.
- **Motion Study** (`assemble.motion_study`, `MainWindowMotion.cpp`,
  `motion/MotionStudyDialog`, mitcad#454): a panel beside the window, not a
  command (the window stays usable; a command or a sketch pauses it): the
  study (the first, or a new `Study1` when there is none; the browser's
  Play opens another), Steps, the step slider, Play/Pause (Alt+P), Loop,
  Speed (0.25x to 4x of one step per 40 ms frame), the keys per joint
  motion with their points and the value at the step, Add Key (Alt+K: the
  joint motion chosen at the current step, with the typed Value (Alt+V)
  or where the joint is), Remove Key (the selected row's point at the
  step) and Keep Position (Alt+T: `play_study` to the shown step, one undo
  step, then it closes). Each frame and each slider move is the `preview`
  of `play_study` from the step shown before, shown through its
  `placements` on the bodies as shown when the panel opened; a contact
  pauses it at the step where the bodies touch, and the panel says where.
  Closing it (Close, Esc) clears the preview. Logs: `Motion study Open
  opened: 20 steps, 2 keys`, `Playing Open from step 0`, `Played Open to
  step 20: Joint1 tx 15 mm, Joint2 rz 90 deg`, `Paused Open at step 7
  (paused): …`, `Playback of Push stopped at step 5.005: stopped at
  contact: …`, `Key added to Open: Joint1 tx at step 0: 5 mm`, `Kept Open
  at step 20: …`, `Motion study Open closed`. Tests:
  `tools/ui-motion-study-test.sh`, `tools/ui-contact-test.sh`.
- **Contacts in Drive Joint and drags** (mitcad#454): Drive Joint's
  `drive_joint` answer's warning shows in its panel (`Preview Drive
  Joint: ok, warning: stopped at contact: …`); a drag stops where the
  `joint_drag` query's `contact` says (`Drag of Pin:1 stopped at contact:
  …` once per contact, the status bar too) and its release logs `Dragged
  Pin:1 to where it touches: …`.
- **Ghost of side A** (mitcad#147): once Joint's origin A or As-Built
  Joint's component A is picked, and while B is not, the occurrence placed
  directly in the joint's component that A lies in is drawn as a ghost
  (`withGhosts`), so that B can be picked behind it; A's highlight stays
  shown but takes no clicks (A is deselected by activating its input, which
  shows A normally again, or with the input's clear button). Once B is
  picked the preview shows A where the joint puts it, normally. Logged as
  `View ghosts: O2 (1 body)` and `View ghosts: none`.
- **Previews of placements:** a feature command's preview shows the
  bodies of the occurrences it places elsewhere where the preview's
  `placements` put them (joints, Move/Copy of components; `Preview <name>
  moves O2`).
- **Dragging components** (`OcctViewer::setOccurrenceDrag`,
  `MainWindowJoints.cpp`): in idle mode, a left drag without keys that
  starts on a body moves the innermost occurrence of its path whose unit
  in its parent component has joints, is not grounded and can move (the
  `joints` query's unit: `dof` above 0, or `mobile`, as a link of a
  four-bar linkage); a click on such a body still picks. The pointer's
  point is on the plane through the grabbed point facing the viewer; per
  move the `joint_drag` query gives the occurrences' placements, which
  move the shown bodies; the release runs `drag_occurrence` (one undo
  step `Drag Pin:1`, after the mouse event: `whenIdle`). A dialog that
  takes the mouse cancels it.
  One function decides what a drag on an item does (`dragAnswer`,
  mitcad#150), so the hint and the refusal agree:
  - **Hint:** the pointer over a body that would drag shows the move
    cursor (`OcctViewer::setDragCursor`) and the status bar hint `Drag to
    move Pin:1 (Joint1: rz)` (its unit's joints and their free motions;
    log `Drag hint: …`, `Drag hint: none` when it goes).
  - **Refusal:** in a design with components, a left drag without keys
    that starts on a body that does not drag is neither a drag nor a
    window selection: the status bar says why (log `Drag refused: …`).
    The reasons, the innermost occurrence's first: `Base:1 is grounded`;
    `Clip:1 is in a rigid group with Base:1, which is grounded`; held
    fixed by its joints (`dof` 0 and not `mobile`): `Fixed:1 cannot move:
    Joint2 is rigid` (several: `Joint2 and Joint3 are rigid`, other kinds
    `Joint2 (revolute) and Joint3 (slider) hold it`); no joints of its own
    in its parent component (components do not drag freely): `Plate:1 has
    no joints in its parent component`, for a body of a component with
    several bodies `Body3 is a body of Plate: bodies move only with their
    component, and Plate:1 has no joints in its parent component; Create
    Components from Bodies makes one component per body`; a body of the
    root: `Bodies of the root component do not move`. A body that would
    drag but not now: the read-only window's reason, `Finish the sketch to
    drag components`, `Close the command to drag components`, `Wait for
    the computation to finish to drag components`. In a command or a
    sketch, a drag on a body that would not drag anyway selects with a
    window as before; so does a drag with keys (Shift), and every drag in
    a design without components. The test: `tools/ui-drag-test.sh`.
- **Browser:** each component's **Joints** folder lists its joints,
  as-built joints, rigid groups and joint origins before the marker
  (joint origins are datum rows with light bulbs; their datum is a plane).
  A joint's tooltip has its kind, state and values, the folder's the
  component's degrees of freedom (the `joints` query's `dof`), an
  occurrence's what its joints leave it. A joint's menu: Edit, Drive,
  Animate, Suppress, Rename, Delete, Find in Timeline; the folder's: New
  Joint, New As-Built Joint, New Rigid Group. A joint row stands for its
  feature (`SelectKind::Feature`), which Drive Joint takes.
  The root's **Motion Studies** and **Contact Sets** folders (mitcad#454,
  after the Analysis folder) list the studies (tooltip: steps and keyed
  joint motions; menu Play, Rename, Delete; a double-click plays) and the
  contact sets (tooltip: their components; menu Edit, Suppress or
  Unsuppress, Rename, Delete; grey while suppressed); the folders' menus
  have New Motion Study and New Contact Set.
  A component's **Motion Links** folder (mitcad#466, after its Joints)
  lists its motion links before the marker as joint rows (tooltip:
  `MotionLink1: gear, Joint1 rz to Joint3 rz, placed`; menu Edit,
  Suppress, Rename, Delete, Find in Timeline); the folder's menu has New
  Motion Link. The browser's joints log line lists them too
  (`MotionLink1 rack_pinion satisfied`).
- **Joint highlight** (mitcad#523, `MainWindowJointHighlight.cpp`,
  `framework/JointHighlight`; the model's side in commands.md, "What each
  side is on"): a joint, as-built joint or constraint set selected outside
  commands (its Joints row, which stands for its feature placed by the
  folder's occurrence, or its timeline cell, placed by its component's
  first occurrence; `keepSelected` keeps it while the `joints` query has
  it) shows, from the `joints` query alone and in whatever component the
  joint lives:
  - each side's geometry (`itemOf` of its `geometry`, placed by the path
    from the root through the placing occurrence): side a orange, side b
    green, an as-built joint's motion origin yellow, pickable as the
    selection's highlights are (a click selects that face instead);
  - each side's frame at the marker (`frames`, placed by the placing
    occurrence) as a triad that takes no clicks: x red, y green, z in the
    side's colour, its length 0.12 of the joined occurrences' box (2 to
    250 mm);
  - the joined occurrences (the sides' own paths) in a faint tint of
    their side's colour (`HighlightDisplay::light`: under the other
    highlights, no clicks);
  - the frame only for a side `on` a fixed reference or with an override
    that sets `origin`, `z_axis` or `x_axis` (a `shift` alone keeps the
    geometry); the frame and the part (the occurrence, drawn whole) for a
    `mount` side; the occurrence for an as-built joint's side.

  The selection card (Floating) and info panel (Docked) say `Joint1:
  revolute, a = Pin:1, face F4:start(…) of Body2, b = Plate:1, face
  F2:end(…) of Body1` (long names shortened in the middle, ` (its frame)`
  after a frame-only side). Selecting another joint replaces the
  highlight; clearing the selection clears it. The pointer over a joint's
  row in the browser (`BrowserPanel::jointHovered`, the viewport followed
  without a button) previews it in lighter colours, nothing of it
  pickable and no tint. Logs, whenever what is highlighted changes:
  `Joint highlight a: face F4:start(…) of F4.b0 in Pin:1 (O2) [Joint1]`,
  `Joint highlight b: edge E{…} of F5.b0 in Shaft:1 (O3), frame only
  [Joint2]` (`, frame and part` for a mount frame; `row 1 a` for a
  constraint row; `… of the joint's component` for its own geometry),
  `Joint highlight frame a at 20.0,20.0,10.0 [Joint1]` (in the design),
  `Joint highlight occurrences: Pin:1 (O2), Plate:1 (O1) [Joint1]`,
  `Joint highlight: none`; `Joint card: Joint1: revolute, a = …` (the
  card's text) and `Joint hover preview: Joint2` / `none`. Tests:
  `tools/ui-joint-highlight-test.sh`, `tools/ui-windows-joint-highlight-test.ps1`
  (ctest `app.ui-windows-joint-highlight`), `app/tests/joint_highlight.cpp`
  (`app.unit`).

## Sheet metal

The SHEET METAL group of the SOLID tab (mitcad#210,
`commands/SheetCommands.cpp`, `MainWindowSheet.cpp`; the model's side in
commands.md, "Sheet rules and base Face" to "Sheet-metal command UI and Flat
Pattern DXF"). Picks on a sheet body are faces and edges; `build` asks the
model's `sheet_reference` (and for two panels `sheet_seam`) what they stand
for, and the definitions keep the semantic references (panels, bends,
boundaries, corners); `load` makes the faces and edges that carry those names
again (a wall `F3:sheet_side0(F2:sheet_curve(c2[c1,c3]))`, a panel's
`sheet_front(<slot>)`, a bend's `sheet_bend_front(<slot>)`, a corner's edge
between two walls), so an edit highlights what the feature uses.

- **Face** (`sheet.face`): Profiles (the newest unused one), Rules: New Rules
  (the default while the design has none: Thickness, Bend Radius, K-Factor,
  Sketch Side; reliefs at round 1 x 2 mm) or one of the design's rules
  features (the newest is preselected once there are some). New Rules is an
  inline definition the model adds as a `sheet_rules` feature just before the
  Face, one undo step; an edit lists only existing rules.
- **Flange** (`sheet.flange`): a straight edge of a flat face or its side face,
  Angle with Flip (the angle's sign), Length and where it is Measured
  (tangent, inner, outer), Position (bend line, inside, outside), Offsets from
  the Ends, Own Bend Radius, Own Bend Relief.
- **Bend** (`sheet.bend`): Fixed and Moving: two flat faces of one sheet (their
  common seam, `virtual_sharp`; where the kernel joined their caps into one
  face, a click on each side of the seam) or an edge of each at the bend
  (placed sheets, `tangent_ports`; another body becomes `moving_body`), Angle,
  Flip, overrides. Bend Type Double Bend (mitcad#288): an edge each of two
  sheets in parallel planes joined by a jog (`port_mode: "double"`), Whole
  Face (`fixed`: the Fixed or the Moving edge's face keeps all its material),
  the angle of each of the two bends without Flip; editing a double bend
  loads it as one.
- **Fold** (`sheet.fold`): the flat face, a sketch line on it, Fixed Side,
  Angle, Flip, Position, overrides.
- **Corner Round**, **Corner Chamfer** (`sheet.corner_round`,
  `sheet.corner_chamfer`): edges through the thickness at corners (between two
  walls), Radius or Distance 1 and 2.
- **Sheet Cut** (`sheet.cut`): Profiles (of one or several sketches on the
  face), the flat Face they lie on, Across Bends (bends opened for the cut).
- **Corner Relief** (`sheet.corner_relief`, mitcad#209): faces of the two
  bends, the Face they leave when they leave several (for bends on different
  faces, the face held, one next to either bend; mitcad#125), Own Corner
  Relief (shape, size; a tear has none).
- **Unfold** (`sheet.unfold`): the Fixed Face and faces of the bends to open.
  **Refold** (`sheet.refold`): the Unfold (a choice of the design's, the newest
  at the start) and the bends to close, none for all.
- **Flat Pattern** (`sheet.flat_pattern`, a model command): the fixed face (or
  the body: keep its fixed face), Seen From (front, back), Show Flat;
  `set_flat_pattern` on OK. Opened on a body with a pattern, it starts with
  the pattern's settings.
- **Sheet Rules** (`sheet.rules`): a rules feature of its own (Thickness, Bend
  Radius, K-Factor, Sketch Side, bend and corner reliefs); its edit is
  **Edit Sheet Rules** (`sheet.edit_rules`, an action: the rules of the
  selected sheet, or of the only one), which every sheet using them follows.
- **Show Flat Pattern** (`sheet.show_flat`): `set_flat_pattern` with `shown`
  toggled for the selected sheet (or the first with a pattern). The view shows
  the developed solid (`analysis_shape` `flat_pattern`) in place of the body,
  without a section's clip (`MainWindow::modelBodies`).
- **Export Flat Pattern** (`sheet.export_dxf`): a save dialog ("Export Flat
  Pattern", the body's name `.dxf`), then `export_flat_pattern_dxf` in the
  document's unit when it is inches, else millimetres; a refusal shows the
  model's reasons. Allowed in a read-only window.

In the browser a sheet-metal body's row has two rows under it (expanded by
default; `BrowserNode::Type::SheetRules`, `FlatPattern`): its rules
(`SheetRules1 (2 mm, R 3 mm, K 0.3)`; double-click and Edit Sheet Rules edit
the feature, Find in Timeline) and, once set, its Flat Pattern (light bulb:
Show Flat; double-click and Edit Flat Pattern open Flat Pattern on it; Show
Flat/Folded, Export DXF..., Delete). A Flat Pattern that is not valid is red
with the reasons in its tooltip. The body's menu adds Edit Sheet Rules and
Create Flat Pattern or Export Flat Pattern. The rows come from the
`sheet_bodies` query (`DocumentSnapshot::sheets`). Test:
`tools/ui-windows-sheet-test.ps1` (ctest `app.ui-windows-sheet`: Face with its
own rules, Flange, rule edits of thickness, K and radius, Cancel, Flat
Pattern, Show Flat and Undo, the DXF read back, an edit from the timeline, the
Floating layout) and `tools/ui-windows-sheet-ops-test.ps1` (ctest
`app.ui-windows-sheet-ops`: Fold with Cancel, Bend at a seam, Corner Round and
Chamfer in the Floating layout, Corner Relief (also of flanges on different
panels), Sheet Cut across a bend, Unfold and Refold, each with its picks,
preview, analytic volumes or flat areas, an edit from the timeline and Undo).

## Surface modelling

The SURFACE group of the SOLID tab (mitcad#455,
`commands/SurfaceCommands.cpp`; the model's side in commands.md, "Surface
modelling" and "Patch, Stitch and Thicken"), after SHEET METAL in
`Ribbon::groups` and the native macOS ribbon's group list.

- **Surface Extrude**, **Surface Revolve**, **Surface Sweep** and
  **Surface Loft** add and edit `extrude`, `revolve`, `sweep` and `loft`
  definitions with `"surface": true` and the operation `new_body`. Their
  Profiles input takes profiles and sketch curves of one sketch; the
  profiles go into `profiles`, the curves into `curves` (with Chain
  Selection's `chain`). Surface Loft's Sections are ordered: a sketch curve
  is a `curves` section of that curve with `chain` set, a profile a
  `profile` section; not both kinds.
- Editing: the solid Extrude, Revolve, Sweep and Loft refuse a definition
  with `surface` set (`canEdit`), so the surface commands, registered
  after them, edit those; each surface command takes only what its panel
  shows (Surface Extrude: distances from the profile plane without taper;
  Surface Revolve: angles or a full turn about a shown axis; Surface Sweep:
  a plain path without the solid Sweep's options; Surface Loft: sections of
  one chained curve or profiles). Other surface definitions (from imports
  or scripts) open in no panel.
- **Patch** takes profiles of one sketch and edges of one body (a loop,
  named edges) with one Continuity; **Stitch** bodies and a Tolerance;
  **Thicken** surface bodies, Thickness, Symmetric and the operation
  inputs. A patch of an opening, with points or with edges of their own
  continuity is not edited in the panel.
- **Offset Surface** takes faces of one body, Distance and Tangent Chain
  (`chain`); a definition of every face (no `faces`) is not edited.
  **Ruled Surface** and **Extend** take Edges (named edges) and Open Edges
  Of: faces of a surface body whose free edges are meant (free edges have
  no names, so the view cannot pick them): one `openings` entry
  `{"faces": [...]}` without an `opening`. Ruled Surface adds Face Beside
  (`face`), Distance, Direction (normal, tangent, angle; the fixed
  direction mode has no input and is not edited), Angle, Flip and Corners;
  Extend Distance and Extension (`mode`). A definition with an `opening`
  of its own (from an import) is not edited.
- **Trim** takes a Surface (`body`), the Trimming Tool, Extend Trimming
  Tool, Offset and the sketch curves' direction of Split Body (the helpers
  `toolInput`, `addTool`, `loadTool` in `CommandSupport`), and Keep: faces
  of the surface clicked where a piece is kept; the click points
  (`SelectionItem::at`) are the `keep` points. Edited, the panel keeps the
  definition's points until pieces are clicked again. **Untrim** takes a
  Face, Close Holes (`holes`), Outer Boundary (`outer`) and In Place;
  **Unstitch** and **Reverse Normal** Faces/Body: faces of one body, or a
  body for every face (no `faces`).
- **Blend Surface** takes for each end (`from_…`, `to_…`) Edges, Open
  Edges Of, Face Beside, Continuity and Bulge (a number, not a
  parameter), and Stitch. **Boundary Surface** has a Type: Fill a Loop
  (Boundary Curves of one sketch with Chain Selection and Tangent to
  Sketch Plane, one `boundary` entry; Open Edges Of faces of one body with
  their Edge Continuity, another) or Network (Curves One Way and Crossing
  Curves, each sketch curve a curve of its own). **Match Surface** takes
  the Face, Target Edges, Target Open Edges Of, Target Face Beside and
  Continuity; **Fair Surface** Faces/Body, Tolerance, Strength,
  Iterations and Keep Tangents at Edges (`keep_edges`). Definitions the
  panels cannot show (several sketch chains or points in a loop, chains in
  a network, a match without a face, openings of their own) open in no
  panel.
- Log lines: `Added surface extrude of <n> profiles and <m> curves` (and
  revolve, sweep), `Added surface loft through <n> sections`, `Added patch
  of <n> items`, `Added stitch of <n> bodies`, `Added thicken (<operation>)
  of <n> bodies`, `Added offset surface of <n> faces`, `Added ruled surface
  off <n> edges and the open edges of <m> faces`, `Added extend of <n>
  edges and the open edges of <m> faces`, `Added trim keeping <n> pieces`,
  `Added untrim (holes closed|kept, outer restored|kept)`, `Added unstitch
  of <n> faces` (or `of a body`), `Added reverse normal of <n> faces`,
  `Added blend surface (<continuity>, <continuity>)`, `Added boundary
  surface filling <n> curves and the open edges of <m> faces` (or
  `through <u> by <v> curves`), `Added match surface (<continuity>)`,
  `Added fair surface of <n> faces` (or `of a body`). Tests:
  `tools/ui-surface-test.sh`, `tools/ui-surface-fill-test.sh`.

## BUILD

The BUILD tab (mitcad#375, `commands/BuildCommands.cpp`, `build/`; the
model's side in commands.md, "BUILD: profile assemblies"; the parts'
`build` blocks in [docs/libraries.md](../docs/libraries.md#build-parts))
builds frames of slot profiles with their connectors, panels and linear
parts, and timber construction ([Timber](#timber), mitcad#478). It is a tab of its own beside SOLID (`Ribbon::groups("BUILD")`:
PROFILE, TIMBER (mitcad#520), CONNECT, MODIFY, OUTPUT), in both chrome styles; SKETCH comes
after it while a sketch is edited, and Finish Sketch goes back to the tab
the sketch started from. The AppKit frontend has no workspace for it: its
commands are in a Build menu there. Everything BUILD makes is ordinary
design content (components, occurrences, features, joints), so SOLID's
commands edit it as any other.

The commands are panels of the framework with their own model commands
(`CommandDef::apply`, `inspect` as their preview, [Glue](#glue)): the app
calls only the model's `build_place`, `build_edit`, `build_connect`,
`build_panel`, `build_remove` and the queries `build_preview`,
`build_parts`, `build_junctions`, `build_cut_list`, `library_parts`,
`components`, and the library commands `library_build_candidates`,
`library_search`, `library_show`. Live MCP fills them as any panel
(`ui_command` with `inputs`); an agent's OK of a command that repeats
places one and closes it.

| Command | Panel | Model |
|---|---|---|
| `build.profile` Profile | grows as the placement needs it (mitcad#394). At first: Part, a tile with the chosen part's picture, name, supplier and part number (`withChoiceTile`, mitcad#427); a click on it, or Enter or Space, opens the part picker with the role's parts, filtered to the system chosen last (mitcad#418); Search and System are hidden inputs (for live MCP); Place On (none needed for the first part); Length (the part's stock length, else 500 mm; also a box at the cursor, `withCursorBox`, and a handle at the free end while the length is a number); for a ground placement Direction, three icon buttons X, Y, Z (`withChoiceIcons`, default Z: standing; `at.direction`). Once a part is placed, Length's mode as four icon buttons (mitcad#427, `withChoiceIcons`: Auto: to the face it meets, Auto: to its centre, Auto: through it, Typed length; their labels the tooltips) and, by the pick: an **end pick** adds Direction (mitcad#417: Auto, the click's zone of the end face, `Auto: by the click (+X)`; Straight; the four turns square to the part it meets, `end.turns`, named by the host axis they run along, `+X`, else `Side 1`..`Side 4`), on a corner Offset (the new part's end from the outer side, positive towards the turn, default 0) and Mitre (Auto, `Auto (none)`: the panel's turns are square, and a square corner lies on the end face, mitcad#427; `Auto (<cut>)` when the click's preview says another; None; Shared, not offered at a corner offset other than 0; Cut the new part; Cut the existing part) with Mitre Angle; a **side pick** adds Offset and From (End A, End B, Middle), which the click fills; either adds Rotate (0, 90, 180, 270 deg) and Flip, and a side pick of a profile Tilt below Rotate (mitcad#444: 0 deg, the lean along the part it stands on, positive towards its end B, up to 80 deg; an arc handle at the foot, a ring about the slot's cross axis, drags it, the angle in the box at the cursor while it is dragged; the foot is cut to lie on the side, an auto length ends flush on the face it meets; Offset and From then refer to the foot's long point; sent as `tilt` when not 0). Placement, last, once the design has a part to go on: four icon buttons (mitcad#427, no More; Automatic: where the click is, the default; On a profile's side; On a profile's end; On a plane (as the first part): a ground placement again; their labels the tooltips). The result names where the part goes (`Adds to: Root`, the `build_host` query). **Place** (Enter) places it, also without a pick in a ground placement (at the origin), and the panel stays for the next one; **Shift + click** (mitcad#451, [Selection inputs](#selection-inputs)) on a place Place On takes picks it and places the part at once with the panel's values, then the next one follows as after Place (a plain click still only previews; one that cannot be placed makes nothing and the panel says why); the view's hint says `Shift+click places at once`; **Close** (Esc) keeps what was placed | `build_host`, `build_preview` while the inputs change, `build_place` on Place |
| `build.timber` Timber | [Timber](#timber), TIMBER group (mitcad#478, mitcad#520): Part (the picker of timber only, `timber`), Mode (three icon buttons: On a member, Spacing, Two points), then Mode's inputs; On a member with Profile's Length (Auto: face, centre, through, Tab; typed, the box at the cursor, the handle); Place (Enter) repeats, Close keeps what was placed | `build_preview`, `build_place` with `to`, `lie`, `length`, `spacing` |
| `build.wall` Wall | [Timber](#timber), TIMBER group (mitcad#479, mitcad#520): Part, Start, End, Length (Auto or Typed, on another wall) and Length, Direction, From and Offset (on another wall), Height, Spacing, Top Plates, Corner, Openings (a tile that opens the Openings window, mitcad#519), Fasteners with Fastener, Size and Count; the members previewed as boxes; Place (Enter) repeats | `build_preview` with `frame`, `build_frame` (`kind: wall`) |
| `build.floor_roof` Floor / Roof | [Timber](#timber), TIMBER group (mitcad#479, mitcad#520): Part, Kind (two icon buttons: Floor or ceiling, Roof), then only that kind's inputs: a floor's Start, End, Length, Width, Direction, Spacing, Openings (the Openings window), Blocking; a roof's Walls, Roof, Pitch From Walls, Pitch, Overhang; Fasteners with Fastener, Size and Count; Place (Enter) repeats | `build_preview` with `frame`, `build_frame` (`kind: floor`, `roof`) |
| `build.panel` Panel | Part, the tile that opens the part picker (panels; mitcad#427; on a timber member's face sheets, mitcad#478); Frame Face (an inner face of a frame puts it in the slots, an outer face on the surface with clamps, a timber member's face a sheet on it); Thickness when the panel's thickness is a parameter. Place repeats; Shift + click on a face places at once (mitcad#451) | `build_preview`, `build_panel`; `fits` false is shown with its reason, not refused |
| `build.linear` Linear Part | as Profile (its part picker too) without the mitre (rails are cut to length; a carriage on its rail gets the joint its `mate` says) | `build_preview`, `build_place` |
| `build.connector` Connector | Near (a click on a profile chooses the nearest junction), Junction (every junction, `L junction of Part1 and Part2`, `, connected`), Connector (the parts that fit, best first, each with the library's picture of it (`library_preview`, `withChoiceImages`), its supplier and part number; shown as a tile, `withChoiceTile`, mitcad#427, that opens the part picker with the fitting connectors first and, greyed with the reason, those that nearly fit; a new junction gives it new ones), With Nuts and Screws, With End Machining, Show All Junctions. The junctions of the profile under the mouse are marked in the view at their `at_frame` (`L`, `T (connected)`; `CommandDef::hover`, `previewMarkersIn`), the chosen one always (`> L`), every one with Show All Junctions; the panel lists the fitting connectors and, greyed, those that nearly fit with the reason; the two profiles are highlighted. The chosen connector is previewed where it goes (`build_preview` with `build_connect`'s fields; its shape `analysis_shape` `{"shape": "build_preview", "junction", "part", ...}`), its fasteners as markers, the machining it needs in the panel; choosing another in the list previews that one. **Connect** repeats and moves on to the nearest junction not connected; Shift + click near a junction connects it at once (mitcad#451; the view's hint says so while Near is empty) | `build_junctions`, `library_build_candidates` with the junction, `build_preview`, `build_connect` |
| `build.fasteners` Fasteners | Connector (a connector, or one of its nuts or screws), With Nuts and Screws, Fastener (the kinds it has), Size (the rows of its library component, `library_show`) | `build_edit` with `config`; off: `build_remove` of each; on for a connector without them: `build_remove` and `build_connect` again with them; one undo step `Fasteners of <name>` |
| `build.swap_fastener` Swap Fastener | [Timber](#timber), Swap Fastener (mitcad#497): Fasteners (the fasteners selected; the view's context menu, BUILD, for fasteners only), Fastener (the compatible ones, a tile that opens the part picker), Size (a wood fastener's: Auto or a row) | `build_swap_fasteners`, one undo step |
| `build.spacing` Spacing Set | Member (a member of a spacing set, mitcad#478; its set's values fill the panel), Spacing (Centre distance, Gap), Distance, Last Board (gap: cut, spread), Count (0: from the distance), Size (the rows of the set's source); enabled while the design has a set | `build_edit` of the set's lead with `spacing` (and `config`): the members laid out again in one undo step |
| `build.frame` Timber Frame | Member (a member of a timber frame, mitcad#479; its frame's values fill the panel), by its kind the inputs Wall or Floor / Roof shows for it (mitcad#520): a wall's Length, Height, Spacing, Top Plates, Corner, Openings (the Openings window, mitcad#519); a floor's Length, Width, Spacing, Openings, Blocking; a roof's Spacing, Pitch, Overhang; Size (the members' row); the members previewed as boxes | `build_preview` with `frame` and `component`, `build_edit` with `frame` (one undo step) |
| `build.length` Length | Part, Length (Detached: this length, Bound: to the face, to the centre, through), Value, Bound To (none: the face it is bound to now); the bound target highlighted | `build_edit` with `length` (a value detaches, a `bind` binds) |
| `build.ends` Ends | Part (the end nearer the click; an end face by its name), End, Angle, About (x, -x, y, -y), Also the Part It Meets (a shared mitre: the end that meets this one in an `L` or `inline` junction); End Machining (As it is, A bore: Diameter, Depth, Thread; None takes the part's end machining away) | `build_edit` with `ends` and `machining`, the part's whole list (`[{end, hole: {diameter, depth, thread}}]`): `build_parts`' list with this end's entries replaced by the bore, or left out for None; one undo step for both parts |
| `build.flip` Flip, `build.rotate` Rotate 90 deg | Parts (any number); Rotate repeats (Enter turns them again) | `build_edit` with `flip` / `rotate`, one undo step |
| `build.cut_list` Cut List, `build.parts_list` Parts List | an action: `BuildListsDialog` (`build/BuildLists.cpp`) with the tabs Cut List (Kerf; the cut parts grouped as the model groups them: quantity, part, length, end A and B, machining, own features, supplier, part number, mass each; the stock: pieces in cutting order, stock pieces, offcut; the other parts) and Parts List (`parts_list`: quantity, part, supplier, part number, supplier part numbers (the row's suppliers' articles, `supplier_parts_text`, mitcad#949), library, version, licence); **Copy as CSV** (Alt+C) copies the shown tab's tables, **Save as PDF...** (Alt+P) writes them with `QPdfWriter` (Qt Gui; A4 landscape) under the list's title and the design's name with the date. A design with timber (the cut list's `timber`, mitcad#478) adds columns after these: the cut parts' material, grade, finish, nominal and actual section (thickness first, `48x148`), running metres and volume (m³), and a row of the timber's totals; the stock's size and each stock piece's length; the parts list's material and grade. A design with profiled boards (the cut list's `profiled`, mitcad#477) adds three more: the profile, the cover width and the area covered (m²), its total in the totals row. A design with timber frames (a cut row's `frame`, mitcad#522) lists each frame's members apart, by frame, with a last column Frame. A design without timber lists exactly as before. The Parts List tab's Property columns field (Alt+L, mitcad#496) takes property keys separated by commas (`Mark, Pset_WallCommon/FireRating, Mass`): a column each after the others with each part's value (its component's own, its library's or computed), in the CSV and the PDF too; the keys stay for the session | `build_cut_list` (`kerf`), `parts_list` (`properties`) |

- **The picker** (`build/BuildCatalog`): the parts of the role from
  `library_build_candidates` (`library_search` with the `role` when the
  former is refused), each configuration row an entry, and the design's
  sources (`library_parts` entries with `source`), so that a part placed
  once is placed again from its source (`part.source`) without its
  library. Read when the command starts and after each placement. Every
  part of the role is a choice of Part (live MCP's `part`). Search and
  System are hidden inputs (mitcad#427; the part picker has both), kept
  for live MCP: Search words must all be in the name, designation, ids,
  supplier, part number, system (`B8 30`), size (`30x30`) or slots (`4
  slots`), and a part chosen that they do not match gives way to the
  first they match; System likewise to the first part of that system.
  The system of the part chosen last is the next start's System (kept
  across designs). **The part a command starts with** (mitcad#452,
  `startPicker`): a part the panel has already (live MCP) stays; else, in
  a design with parts of the role, the part chosen or placed last in this
  design, or, when none was chosen in it since it was opened, the part of
  the role the design placed last (`build_parts` lists them in the order
  they were made; its source in the catalog); in a design without parts
  of the role (a new one) the picker's first part within the System it
  opens with: the first favourite (`pickerOrder`: the starred parts first,
  in the catalog's order), else its first part (`pickerFirst`; a System
  none of the parts fits leaves them all). A part of another design does
  not carry over: the parts chosen last belong to the design they were
  chosen in (`DocumentHost::designId`, new with every design opened or
  started). The part picker shows the part chosen, scrolled into sight
  and highlighted (`PartGrid::reveal`: All parts, without the search,
  when the group or the search leaves it out). Log: `BUILD profile starts
  with <library>/<component>: the first favourite|the picker's first
  part|chosen last in this design|placed last in this design`.
- **The part picker** (mitcad#418, [Part picker](#part-picker)): the Part
  tile (Connector's Connector; `withChoiceTile`, mitcad#427) opens the main
  window's `PartPicker` for the command: Profile, Panel and Linear Part
  ask with their catalog's parts (`pickerParts(QVector<build::Part>)`,
  the drop-down's keys) and the panel's System as the picker's system;
  Connector with the junction's fitting connectors (the catalog's
  `forJunction`) and, greyed with the reason, the nearly fitting ones
  (`buildCandidates` with the junction's `refused`). A tile clicked is
  chosen in the command (`DocumentHost::chooseInCommand` with the key)
  and previewed at once; the open picker follows the panel (the part,
  another junction; live MCP's Search and System). The part chosen sets
  the hidden System to its own system (unless it fits several); the
  picker leaves its own filter as it is until it is opened again. The picker closes with the command
  (`CommandDef::ended`, `commandEnded`) unless pinned; an open, pinned
  picker follows the next BUILD command that chooses parts (its `init`).
  In this design reads `usedLibraryParts`; the main window's
  `refreshScene` calls `designChanged`. Not in the AppKit frontend (its
  inspector has no button).
- **Where parts go** (mitcad#394): no component needs to be activated.
  The model's `build_host` query names the BUILD host, the active
  component when it takes components, else the nearest one up that does
  (in a new design Component1 itself, which takes components, mitcad#421;
  the root when Component1 is set to Part); the panel's result says
  `Adds to: <name>`, and no policy refusal comes from where the parts go.
- **Where a pick is:** `withOnPick` keeps the click in the design (`at`
  is in the item's component; `build::recordPick`); an item given
  otherwise (live MCP) stands for its middle. `at.occurrence` is the
  item's path from the host (`build::pathFromHost`), `at.point` in the
  host's coordinates (`build::inHost`), as joints resolve their picks
  from their component: the root's origin planes, construction planes and
  faces are picked while Component1 is active.
- **Only usable targets** (`InputDef::withPickRule`): a **ground
  placement** (no profile in the design yet, or Placement On a plane)
  takes planes, points and faces that are no BUILD part's, the origin's
  planes shown; with nothing picked the view shows `Click a plane or a
  face, or press Enter; Shift+click places at once`
  (`Inspection::viewHint`) and Enter places the
  part at the host's origin along the chosen axis (no `at.geometry`).
  After Place (mitcad#513) a ground placement with nothing picked (Placement
  On a plane, which stays chosen) previews nothing at the origin on top of
  the part placed there: the view says `Click a plane or a face for the
  next one; Shift+click places at once`, the panel `Click a plane or a
  face where the next one goes.` (`Inspect Profile: failed: ...`, Enter
  refused) until a click picks a place; the next part previews there.
  Otherwise only faces of the parts it can go on highlight (a profile's
  sides and ends; for Linear Part also a rail's), the view says `Click a
  profile's side or end; Shift+click places at once`, and the origin's and construction planes are
  hidden (`withHiddenConstruction`, the host's `setConstructionHidden`)
  until the placement is a ground one again or the command ends; their
  visibility in the design is not changed.
- **Snap distance** (mitcad#417): every placement with a click sends
  `at.snap`, 12 pixels of the view at the click in mm
  (`build::kSnapPixels`, `CommandContext::pixelSize`, kept with the
  click by `build::recordPick`); the model snaps within the larger of it
  and its own distances, so a zoomed-out view still snaps.
- **Side snaps:** a click on a side goes flush with an end within one
  section width of it, or to the middle or a grid point (the model's
  rules, commands.md, *Side placements*); the result says where (`0.0
  mm from end B (flush B)`), and the markers show the ends and the middle
  (`flush A`, `flush B`, `middle`) and the snap taken (`> flush B`).
- **Side offsets** (mitcad#417): Offset and From are always there on a
  side. A click fills them from the preview's `side.from` and
  `side.offset` (flush with an end: that end and 0; the middle: Middle
  and 0; else the nearer end), and the placement is left to the click
  (`at.point`), which the model binds to that reference. Typing an
  offset sends `at.from` and `at.offset` from then on: the part moves
  from its reference and never snaps; only a new click replaces both.
  Changing From keeps the part where it is: the offset from the new
  reference comes from the side shown last (`side.offset`, `side.width`,
  the target's length from end B's snap) and is sent as typed.
- **Ends** (mitcad#417): a click on an end face continues the part
  straight in the face's inner half and turns it towards an edge in the
  band along it; the click fills Direction's choices from the preview's
  `end` (its `turns`; Auto again). Straight sends the end's axis
  (`at.direction`, the turns' cross product), a turn its vector; on a
  corner the Offset goes as `at.offset`. Mitre Auto sends no `mitre`
  (the model's default, mitcad#427: none on a square corner, the panel's
  turns; shared at offset 0 only on a corner at another angle), None sends
  `{"cut": "none"}`. The markers show the end face's zones (`straight`
  in its middle, `turn +X` in the middle of each band; the one taken
  `> turn +X`). The preview never follows the mouse (mitcad#508:
  running the placement on every move lagged): with the mouse on a side
  or an end face of a part it can go on (`hoverPreview`; a side while
  Placement is Auto or On the side, an end while Auto or On the end)
  only the face a click picks is shown (the view's hover highlight, and
  the log names it), before the first click too; the preview and the
  inspection stay where the click and the panel's inputs put it, and
  only a click (on another face or place) or an input previews again.
- **Auto length:** the preview without a length names the face the ray
  meets (`length.target`); the panel then binds to it (`{"bind": target,
  "mode", "add": 0}`) and previews again. Nothing in the way (the first
  part): the typed length. **Tab** steps face, centre, through wherever
  the main window has the keyboard (`InputDef::tabCycle`); typing a length
  or dragging its handle switches to Typed length (`onChange`), and
  switching to it starts from the auto length shown.
- **The preview:** the new part's shape is `analysis_shape` `{"shape":
  "build_preview", ...}` (`build_place`'s fields); without one, the
  section along `frame`'s z for `length` (`build::boxAlong`). The target
  face is highlighted orange (`Inspection::highlights`); the length is a
  label at the free end and the slot's `snaps` are markers
  (`Inspection::report`, `previewMarkers`). `frame` is `{"origin",
  "x_axis", "y_axis", "z_axis"}` in the design, the new part's own axes at end A
  with z from end A to end B (the junctions' frames too). The replies'
  lengths are millimetres and their angles radians.
- **Enabled:** Profile, Panel, Linear Part and Parts List always (an empty
  picker says to fetch the parts library); the others while
  `build_parts` lists parts.

Logs: `BUILD parts <role>: <n> (<m> in the design)`, `BUILD profile adds
to Root (a ground placement)`, `BUILD connectors for
junction <id> (<kind>): <n> fit, <m> nearly`, `Inspect Profile: place
side, length 742.50 mm (bound, auto: to the face to F8:part(...) of
Part5), square ends; adds to Root; snap end_b at 0.0 mm from b; least
snap 1.30 mm; body 742.50 mm; part Profile 30x30` (the part previewed, last, mitcad#418; the side's snap, `none` between snaps, its offset from
its reference `a`, `b` or `middle`; `at.snap`; how far the model's
preview body reaches along the part's axis, mitcad#431), `Inspect Profile: place
end, ...; end edge towards +X, mitre none, offset 0.0 mm; least snap
1.30 mm` (the end's zone `straight`, `edge` or `given`,
the direction, the mitre the placement takes, a corner's offset), `BUILD side click: From End B, Offset 0 mm (end_b)`,
`BUILD side offset typed: from end B`, `BUILD From End A keeps the part
in place: Offset 350 mm`, `BUILD end click: edge towards +X, mitre
none; turns +X, +Y, -X, -Y`, `BUILD corner offset: the shared mitre
goes (it needs offset 0)`, `BUILD hover pick: face <face> of <body> in
<occurrence>, no preview` (`BUILD hover pick: off the parts' faces`; the
face a click picks, mitcad#508), `BUILD click not placed: <reason>`, `Preview marker Profile
turn +X world x,y,z at x,y` (an end face's zone; `straight`; the taken
one with `> `), `Inspect
Profile: waiting: Click a profile's side or end.; adds to Root; part <part>`, `View
hint Profile: Click a plane or a face, or press Enter`, `Profile Place On
takes now: faces`, `Construction geometry hidden for the command`,
`Construction geometry shown again`, `Panel Profile input axis=x at x,y`
(an icon button of a choice), `Placed <part> as <name> (<occurrence>), length
<how>`, `Applied Profile (2)`, `Closed Profile after 2`, `Profile: Tab
steps Length`, `BUILD length typed: detached from the auto length`,
`Cursor box Profile length: 742.5 mm at x,y` (the box's text, when it
changes while shown), `Inspect Connector: junction J3 (L): 1 fit, 0
nearly, preview <connector> with 2 fastener(s), machining D6.8 x 15 M8
at end a`,
`Preview marker Connector > L world x,y,z at x,y` (a junction's marker,
the chosen one with `> `; `Preview marker Profile 742.5 mm ...` the
length at the free end), `Placed panel <part> as <name> (<occurrence>), fits; 0
clamp(s)` (`, does not fit: <reason>`), `Connected <part> at junction <id>
(<kind>) as <component>: <n> added`, `Fasteners of <name>: <n>
command(s)`, `Length of <name>: <value>` (`bound, face`), `Ends of <name>:
end a 45 deg about x, shared, machining D6.8 mm x 15 mm M8 at end a`, `Flip <names>`, `Rotate <names> 90 deg`,
`Cut list row: 2 x <part>, 500.0 mm, A 45 deg (x), B square`, `Cut list
stock: <part>, 6000.0 mm stock x 1, pieces 500.0, ..., offcut <mm>`, `Cut
list: <n> row(s), <n> stock, <n> other part(s), kerf 3 mm`, `Build parts
list: <n> row(s)`, `Build list CSV: <line>`, `Build list copied as CSV:
<n> line(s)`, `Build list PDF written: <path> (<bytes> bytes; Cut list of
<design>, <date>)`, `Ribbon tab BUILD at x,y`, `Panel Profile input part at x,y` (the Part tile), `Panel Profile tile part: Profile 30 test (Test supplier P30-T)` (what it shows), `Profile: Part button` (clicked), `BUILD picker: <part> not chosen, <command> does not run` (a pinned picker's tile after its command ended), and the picker's own ([Part picker](#part-picker)). Tests:
`tools/ui-build-test.sh`, `tools/ui-windows-build-test.ps1` (ctest
`app.ui-windows-build`).

### Timber

Timber construction (mitcad#478; the model's side in commands.md, "BUILD:
timber construction"; the parts in [docs/libraries.md](../docs/libraries.md#timber-components))
has a group of its own on the BUILD tab, **TIMBER** (mitcad#520, between
PROFILE and CONNECT), with three tools split by what they build:
**Timber** (`build.timber`: loose members, on a member, at a spacing or by
two points), **Wall** (`build.wall`: wall frames) and **Floor / Roof**
(`build.floor_roof`: floors, ceilings and roofs). Before mitcad#520 one
Timber tool had a fourth mode, Frame, with a Frame choice of wall, floor
or roof; that mode and its `how=frame` input are gone, with no
compatibility layer (scripts and live MCP start Wall or Floor / Roof);
the model's commands and saved designs are unchanged. A slot profile is
placed by clicking a slot, a timber member is mostly drawn from point to
point and repeated at a spacing. Both drive the same model commands;
Profile's steps, dialogs and defaults are unchanged, and its picker lists
no timber (the catalog's `profile` asks `library_build_candidates` without
`kind`, and the design's timber sources are left out of it), as the
TIMBER group's list no slot profile (`timber`: `role: profile`, `kind:
timber`). The part picker's choice goes into the tool that asked for it
(`PickerRequest::command`: `build.timber`, `build.wall`,
`build.floor_roof`).

- **Part**: the tile that opens the part picker with timber only;
  favourites and the design's own parts first ([Part picker](#part-picker),
  mitcad#418, #452), then the standard sizes before the less common ones,
  which are marked (`less common` in the label and the tile's sizes). A
  timber row is named thickness first (`48x148`); its data are the row's
  own block (`build.rows`) merged over the part's (`build::Part::build`).
  Every size is a tile of its own (mitcad#484: `library_build_candidates`
  answers with each part's `configurations`, and the catalog makes an
  entry of each row): the tile shows the size, a search by it (`48x148`)
  finds it, and the member placed from it is that row (`config`) with its
  section. The families are groups of their own (the library's
  categories `sawn-timber`, `planed-timber`, `dimensioned-timber`;
  subgroups of Wood in MitCad's library, mitcad#527). A
  row with a picture of its own (`row_previews`, docs/libraries.md; the
  timber's show the section from its end) shows it in its tile. Profiled
  boards (mitcad#477: tongue and groove, cladding and deck boards, each
  family a group: `tongue-and-groove`, `cladding`, `decking`) are timber
  too; a board that joins the next adds its cover width to its label
  (`cover 112`).
- **Mode** (Timber's), three icon buttons in this order (mitcad#485,
  mitcad#520): On a member (the default), Spacing and Two points; the one
  used last is the next start's.
  - **On a member**: Profile's placement inputs and rules (a ground
    placement on a plane, Direction X/Y/Z; a click on a side near an end or
    on an end face; Offset and From, but no slot grid: the model places on
    a member's `face` frames with a free offset; the auto length, Tab, the
    mitres, Rotate, Flip, Tilt; the hover shows only the face a click
    picks, the preview stays the click's, mitcad#508). Its Length starts as
    **Typed length**; Auto (to the face, the centre, through) stays a
    choice and binds as Profile's (`bindAutoLength`). A ground placement
    also has **Lie** (Flat or On edge, Tab steps it; `build_place`'s `lie`
    with `at.direction`). A member on another member's end lies as that
    one does (the model's rule, commands.md, *Frames of members*):
    two boards of one size meet with ends of the same width and flush
    faces; Rotate and Flip still turn it. The last member of a frame, its
    length bound to the first one it meets end to end, closes that corner
    as its start was made: a shared mitre cutting both after a mitred
    start, a square butt after a square one (commands.md, *Frames of
    members*); the panel says so (`Closes the corner at end A of <part>: a
    shared mitre`). A typed length that ends on that member's outer side
    (or its face) closes the corner the same way (mitcad#492). A member
    lying on a plane (a ground placement along X or Y on XY, or on a
    picked plane or face) rests on it, on the plane's normal side, not
    centred on it; one standing along the normal stands on its end. A
    member standing on another member's side lies as that one does: its
    width along the other's width, same-size members flush (Rotate turns
    it further). The inspection logs where end A goes (`; end A at x, y,
    z`). Commands.md, *Resting, lying as the host, typed corners*.
  - **Spacing**: Spacing On: **Between two members** (on a member's side as
    On a member, the auto length bound to the member it meets: studs,
    joists, rafters), or **Over the supporting faces** (Supporting Faces:
    the faces the row lies on; the first member by two points across all
    of them at their start, `spacing.along` and `spacing.length` the run
    of the faces along the members that carry them: deck boards, battens;
    Lie as above). Spacing: Centre distance (default 400 mm) or Gap (a
    board row, default 5 mm; Last Board: cut to what is left, or the gaps
    spread). The panel counts the members as the model will, and the
    preview shows every member of the set, the last one ripped when it is
    cut (`build_preview` with `spacing`, mitcad#487), after the click: the
    hover between two members shows only the face a click picks (the
    view's hover highlight; no set laid out and no inspection while the
    mouse moves, mitcad#489: laying the set out on every move lagged; On
    a member likewise, mitcad#508).
    Offset and From typed after the click move the set (mitcad#492): from
    End A the first member's near face, from End B the last one's far
    face; the count is the model's (`spacing.count` of the preview). One
    `build_place` with `spacing`: one undo step for the set. Profiled
    boards that join (tongue and groove, shiplap, feather-edge, mitcad#477;
    `build::Part::joinedCover`) lying flat are laid edge to edge in a Gap
    row: the panel counts them by their cover width and says `edge to
    edge, cover 112 mm` instead of the gap, which the model does not keep
    (its warning says so); deck boards keep the gap.
  - **Two points** (mitcad#485: two clicks; nothing follows the mouse):
    **Start** (a plane, a face, a member or a point) and **End** (a second
    click; Shift + click places at once) give the member and its length,
    their distance (no `length` sent; no Length inputs in this mode).
    **Lie** (Flat or On edge, **Tab** steps it). Start and End snap to the
    members of the design (timber and profiles): to an end's middle within
    the snap distance (`at.snap`, 12 pixels), else to the nearest centre
    line or side's centre line (`snapToMembers`, from each member's
    placement, length and its source's section). The member runs between
    the two points in 3D (mitcad#509): when End is on Start's plane (End
    clicked on the same plane or a face in it, within 0.01 mm, or on no
    plane at a point in it) it lies on that plane, End moved onto it as
    snapped; otherwise it is **free** between the points (no `geometry`
    sent: grounded through them), turned by Lie about its axis with "up"
    along the host's +Z. Before mitcad#509 End was always moved onto
    Start's plane: a click on another member's side gave a member a few
    millimetres long beside the start. Where a point is in another member
    (a centre line snapped to) or, free, on its far side, the model cuts
    that end back to the member's face (commands.md, *Members by two
    points*): a member clicked between two studs fits between them. The
    panel says what each point is on and whether its end was cut (`Start:
    on <member>, its side; cut back to its face`), and the plane (`Lies on
    the start's plane: the end is on it too`) or `Free between the points:
    turned by Lie about its axis, up along +Z`. `build_place` with `at`
    (`geometry` the plane or face picked when the member lies on it,
    `point`), `to.point` and `lie`; the placed length logged is the
    preview's (the points' distance less the cuts).
- **What the tools remember** (mitcad#485, mitcad#520, while the
  application runs): shared by the three tools, the part chosen last (in
  any design) and the fastener with its size and count (below); each
  tool's own: Timber's Mode, the length typed last, the ground placement's
  Direction and Lie and whether a set is fastened; Wall's Fasteners switch
  and Corner; Floor / Roof's Kind and Fasteners switch. The first time: the part's default
  length (the library's `length.default`, 3000 mm for timber), Direction X,
  Flat, and a dimensioned 48x98 (`material.finish: dimensioned`, a 48 x 98
  section), else the picker's first part. Logged as `BUILD timber starts:
  length 3000 mm (the part's default), typed; along x, flat` and `BUILD
  timber starts with <library>/<component>: the first time's dimensioned
  48x98` (`chosen last in the tool` later).
- **Spacing Set** (`build.spacing`, MODIFY) edits a set as one afterwards:
  its distance, mode, count and members' size.
- **Wall** and **Floor / Roof** (mitcad#479, tools of their own since
  mitcad#520; the model's side in commands.md, "BUILD: timber frames"): a
  whole frame of the chosen timber in one `build_frame`, one undo step.
  Wall shows only a wall's inputs (no Mode or Frame choice); Floor / Roof
  has **Kind** at the top, two icon buttons (`frame_kind`: **Floor or
  ceiling**, the default the first time, and **Roof**; the one used last
  is the next start's; the spacing follows it while it is the other
  kind's default, 400 mm and 600 mm), and only that kind's inputs. Each
  frame is a component of its own in the BUILD host (mitcad#522), named
  after it: the browser shows `Wall 1`, `Wall 2`, `Roof 1` under the
  host, each with its members and a Joints folder of only its own
  fasteners' joints; the panel says where it goes (`Adds to: Wall 2 in
  Component1`). Hide, move (the frame moves as one; laid out again where it
  is), delete and Timber Frame work on the component; a click on any of
  its members is a click on the frame.
  - Wall (`build.wall`): Start (a click; none: the host's origin) and End (a click,
    level with the start; none: Length along Direction, +X by default),
    Height (2400 mm), Spacing (600 mm; a floor's 400 mm), Top Plates (one
    or two), **Openings** (a tile that opens the Openings window, below,
    mitcad#519; the command's data is a text, each opening `offset
    widthxheight [sill]` in mm separated by `;`, no sill: a door, `600
    900x1200 900`, the hidden input `frame_openings`). A Start
    or End clicked on a member of another wall frame joins it (`joins`:
    a corner or a T-junction) at that wall's base height. **Corner**
    (mitcad#510; `corner`, sent with every wall): three icon buttons with
    the joints' symbols (mitcad#524, `withChoiceIcons`; `build-corner-butt`,
    `-lapped`, `-straight`): **Butt** (the default), **Lapped plates** and
    **Straight** (on an end face the new wall continues the one clicked in
    line, its end against the end face), remembered for the next start
    with the tool's other choices; Lapped plates is not offered while the
    Start is on another body's face (`withChoiceOffered`: nothing to lap).
    **A Start on any other body's face** (case B, mitcad#524: an imported
    wall, a solid, a loose member; a face that is no Timber wall frame's,
    which is case A and recognised first) sends `square`, `face` (the
    body, the face, its occurrence) and the click: the wall starts square
    from the face on its outward side, on the floor under the click (else
    the face's bottom edge), at the click along the face with From and
    Offset as on a wall's side (the click fills them); a thin end face
    makes a corner (Butt: its side against it; Straight: in line beyond
    it); a sloped or horizontal face is refused with the reason; the body
    is not changed. Length's Auto ends the run at the first wall or any
    body's face it meets (`to a face of Body2` in the view's hint). The
    panel and the log say which case and where: `Wall 1: from a face of
    Body2, at the middle, base on Body1's top (z 200 mm)`, `Wall 1: from
    the end face of Body2 (butt corner), base at the face's bottom edge
    (z 0 mm)`, `Wall 1: in line beyond the end face of Body2 (straight),
    ...`, `Wall 2: in line beyond Wall 1's end B (straight)`. A Start
    clicked on another wall's side or end face
    (mitcad#493; `square`) starts the wall square to it, towards the side
    clicked, as far as the End clicked is along that direction, else
    Length (Direction is hidden). With Butt, a click on the side starts it
    there, also right next to the end (never hanging past it: its outer
    face at most flush with the end face): its end stud and plates butt
    square against that side; the other wall is neither run on, lapped
    nor notched, only given a backing stud each side of the new one where
    they fit, so both walls' sheathing has a stud behind it. A click on an
    end face starts it from that end, its side against the end face (a
    corner, still joined: the other wall unchanged, the new one with a
    backing stud where the other meets it, laid out again when the other
    is edited). With Lapped
    plates (mitcad#493's corner): an L corner at that wall's end when the
    click is near it (within half its stud spacing), else a T at the
    click; at the L the existing wall continues, the new one butts against
    it, the corner's plates lap. **From** (End A, End B, Middle) and
    **Offset** (mitcad#514; `from`, `offset`, shown while Start is on
    another wall) say where along that wall it starts, as Profile's side
    pick (mitcad#417): the click fills them from where the model put it,
    snapped as a side placement is (flush with an end when the new wall's
    face is within its depth of it, a clean corner: its outer face in the
    plane of that end face; centred near the middle; else the click's
    place from the nearer end; the click's snap distance sent as `snap`).
    Typed, they are sent and move the preview once (no mouse-follow,
    mitcad#508); a new Start click leaves them to the click again. With
    Lapped plates, flush with an end is the L corner there. Without an End
    clicked, **Length** (mitcad#515; `frame_length_mode`, `auto_end`)
    offers **Auto: to the wall it meets** (the default; again after each
    wall placed): a wall whose run meets another wall frame's side ends
    there, butting it (`joins.end` that wall, the view's hint `Click where
    the wall ends, or press Enter: to Wall 1's side`), else the typed
    Length; and **Typed length**: typing a Length switches to it. An End
    clicked wins. The panel says where the wall starts, the wall its end
    butts against and its length between the faces, and the corner
    (`Wall 2: from Wall 1's side, flush with end A (butt)`, `Wall 2: from
    Wall 1's side, 1200 mm from end B (butt)`, `Wall 4: from Wall 2's
    side, at the middle, to Wall 1's side (butt), 2902 mm`, `Wall 2: from
    Wall 1's end A (butt corner)`, `Wall 2: an L corner at Wall 1's end B
    (lapped plates)`, `Wall 2: a T on Wall 1, 1551 mm from end B`) and the
    length the model works out. With Auto (mitcad#536) a line says what
    the run ran to and the wall's length (`Length Auto: the run met a face
    of Body2, the wall 7819 mm`, `Length Auto: the run met Wall 3's side,
    the wall 3453 mm`, `Length Auto: the run met no wall or face, Length
    3600 mm`), also when the preview is refused: the panel still names the
    wall, its length as worked out and what its start and run met, the
    reason in red (`Adds to: Wall 1 in Component1`, `Wall 1, wall 7819 x 7000
    mm: 0 member(s)`). The run meets the body it starts on too beyond its
    own face (a room modelled as one body: its far wall), along lines a
    quarter, half and three quarters up and just under the wall's top, at
    its centre line and its sides (commands.md, *The auto end*).
    **Spliced plates** (mitcad#536): a plate longer than the timber's
    longest is made of pieces butted end to end, each joint over a stud,
    a second top plate's joints a stud spacing from the first's; the panel
    lists them (`Spliced over studs, pieces up to 5400 mm: top plate at
    5400 mm; top plate at 4800 mm; bottom plate at 5400 mm`), the preview
    and the cut list show the pieces.
  - Floor / Roof's floor (`build.floor_roof`, Kind Floor or ceiling):
    Start and End two opposite corners (axis-aligned in the host),
    or Length and Width from the start (Width to the left of Direction);
    Spacing (400 mm); Blocking (a row halfway); Openings `a b
    widthxlength` (from the corner, across and along the joists).
  - Floor / Roof's roof (Kind Roof): Walls (a member of each of two wall
    frames; the first selection input, so the view's context menu hands
    it the wall members selected), Roof (Gable or Mono-pitch), Spacing
    (600 mm), Pitch (30 deg; a mono-pitch roof's from the walls' heights
    while Pitch From Walls is checked), Overhang (300 mm).
  - Fasteners (checked, each tool's own switch): under it while it is on,
    **Fastener**, **Size** and **Count** (mitcad#497; below, *Fasteners of
    frames and sets*): two of the default wood fastener at each junction
    (mitcad#516: the countersunk wood screw, flush), sized by the model,
    until another is chosen.
  The preview (`build_preview` with `frame`) draws the members as boxes
  and counts them by role, with the layout's warnings. Enter places it;
  the panel stays for the next one with its values (Part, Kind, Length,
  Height, Spacing, Corner, Fasteners, ...), Start and End empty.
  Then nothing is previewed (mitcad#513: the next wall previewed at the
  origin, the default place, lay on top of the one placed, one wall seen
  twice) until a click picks a start: the view says `Click where the next
  wall starts` (`floor`), the panel `Click where the next wall starts.`
  (`Inspect Wall: failed: ...`; Enter is refused, places no second wall
  at the origin). A click on a start (a plane, a face, another wall's
  member) previews the next wall there. Only the first frame of a panel
  is previewed from the origin before a click.
  - **Openings** (mitcad#519; Wall's, Floor / Roof's floor's and Timber
    Frame's): a tile with the openings' summary (`Add openings...`, `2
    openings: 1 window, 1 door`; a wall's kind under it) whose click (or
    Enter) opens the **Openings window** (`build/OpeningsWindow`), a
    non-modal window of the main window beside the panel; it closes with
    the command. A wall's openings are rows of **Kind** (Window, Door),
    **From** (End A: the offset from the wall's start to the opening's near
    side; End B: from its end to the far side, turned into the offset from
    the start with the wall's length when written), **Offset**, **Width**,
    **Height** and **Sill** (a window's; a door has none); a floor's of
    **Offset Along**, **Offset Across** (from the outline's corner, across
    and along the joists), **Width** and **Length**. **Add Window**, **Add
    Door** (a floor's **Add Opening**) add a row 600 mm after the last
    opening (900 x 1200, sill 900; a door 900 x 2100; a floor's 900 x 2400
    at 600, 600), **Remove** takes the selected row away, **Close** hides
    the window. The window takes the keyboard when it opens (Enter in a
    field commits the field, Escape closes the window); once it is hidden
    (Close, Escape, the command ending) the main window is made active
    again (mitcad#532: `OpeningsWindow::hideEvent`, logged `Openings window
    hidden: the main window active again` when it still had the
    activation as it hid),
    so Enter places and Escape cancels the command without a click on the
    main window first; a click on the panel's buttons works while the
    window is open. Each change (a field committed with Enter or Tab, a choice,
    a button) writes the rows as the command's text
    (`DocumentHost::chooseInCommand` with the text input `frame_openings`,
    `CommandSession::choose` taking text inputs too), so the preview
    follows once; the text stays the command's data (live MCP, scripts,
    `build_frame`'s `openings`; `build/TimberOpenings` reads and writes
    it). The selected row (`frame_opening_selected`) is marked in the view:
    each opening's marker `opening 2` at the middle of its framing's
    members, `> opening 2` for the selected one. The rows are checked as
    they are typed, each row's problem shown on it: no width or height, a
    window without its sill, outside the wall, too near its end studs or
    overlapping another one with its framing, the header above the top
    plate, the sill below the bottom plate (a floor's: outside the floor,
    a negative offset), against the sizes the preview laid the frame out
    with. A wall's **Load-bearing** choice at the top (`frame_bearing`,
    `load_bearing` of `build_frame` and `build_edit`; commands.md, *BUILD:
    timber frames*): **Non-load-bearing (partition)**, the default for a
    new wall, frames each opening with its king studs as its sides and one
    flat header between them, no trimmers; **Load-bearing** with trimmer
    studs and a header of plies on edge, as before; Timber Frame shows a
    wall saved without it as load-bearing. The inspection says so (`2
    openings, non-load-bearing: 1 flat header each, no trimmers`). Logs:
    `Openings window opened for build.wall: 0 row(s), non-load-bearing`,
    `Openings place add_window at x,y; in the window x,y` (`add_door`,
    `add_opening`, `remove`, `close`, `bearing`, each row's `kind.1`,
    `from.1`, `offset.1`, `across.1`, `width.1`, `height.1`, `sill.1`;
    logged again when they move), `Openings added a window as row 1`,
    `Openings rows: 600 900x1200 900` (written), `Openings row 2: outside
    the wall (3600 mm long)`, `Openings rows ok`, `Openings bearing:
    load-bearing`, `Openings selected row 2`, `Openings window closed with
    build.wall`, the inspection's `; openings 2, non-load-bearing, opening
    2 marked`, `Preview marker Wall > opening 2 world ...`.
  - The view's context menu (mitcad#494, mitcad#520) offers them under
    **BUILD** by what is picked: on a member's face Timber (On a member)
    and Wall (its Start: a wall started on that member, From and Offset
    filled as by a click); on members of wall frames Floor / Roof
    (`offeredFor`), first when they are members of two walls (`completes`):
    it starts as a roof with them in Walls (`BUILD floor_roof kind roof, 2
    item(s) selected before`); a selection it cannot take as walls goes to
    the floor's Start.
- **Fasteners of frames and sets** (mitcad#497; the model's side in
  commands.md, "BUILD: choosing and swapping fasteners"): Wall's and Floor
  / Roof's Fasteners (on the first time) and Timber Spacing's
  **Fasteners** (off the first time: each member of the set fastened to
  the members it stands on, reaches or crosses) show, while on, three
  inputs, their choices shared by the three tools:
  - **Fastener**: a tile with the fastener's picture that opens the part
    picker with the libraries' wood fasteners (the catalog's `wood`:
    `library_build_candidates` with `role: fastener`, `kind: wood`; each
    once, its rows are Size's), the choice going into Fastener
    (`PickerRequest::input`, `fastener`); the first time the **default
    wood fastener** (mitcad#516, `defaultWoodFastener`): the one a
    library marks `wood.default` (MitCad's library: Wood screw,
    countersunk, its head driven flush, mitcad#512), else the first screw
    driven flush (`wood.seat` `head_top`, or a countersunk or bugle head
    without a seat), else the first screw, else the first wood fastener,
    in the catalog's order. Not the catalog's order alone: by name the
    pan-head Connector screw (`head_underside`, for metal connectors) came
    first, and every default frame's screws stood proud of the wood. A
    choice the user made (in this run) still wins; the Connector at a
    timber junction starts with the same default;
  - **Size**: **Auto** (sized from the members at each junction, the
    model's rule) or one of the fastener's rows (`3.1x90`);
  - **Count**: how many at each junction (2).

  A frame sends `fastener` (its row unless Auto) and `fastener_count` with
  `build_frame`; a set `spacing.fastener` and `spacing.fastener_count` with
  `build_place`. The panel says it (`3 x Ring nail 3.1x90 at each
  junction`, `2 x Wood screw at each junction, sized from the members`;
  a set's `Fastened to its hosts: ...`, `No fasteners`). The fastener,
  its size and count are remembered for the next start of any of the
  three tools, each tool's Fasteners switch for its own (with its other
  choices, mitcad#485, mitcad#520).
- **Swap Fastener** (`build.swap_fastener`, CONNECT; mitcad#497): the
  fasteners selected (Fasteners: wood screws and nails, or a connector's
  nuts and screws; picked in the view, a pick rule takes fasteners only)
  made from another one where they are. **Fastener** offers the compatible
  ones (a tile that opens the part picker): wood fasteners for wood ones,
  with **Size** (Auto: sized from the members each joins, or a row); the
  other fasteners of the first one's slot system (`library_build_candidates`
  with `role: fastener`, without a `wood` block) for a connector's, each
  row a choice. It starts with the fasteners' own part (only Size to
  change), Auto. **Swap** (Enter) runs `build_swap_fasteners`: one undo
  step; the model's warnings for the new size (embedment, edges, ends) in
  the status bar. The view's context menu offers it under **BUILD** when
  everything selected is a fastener (`offeredFor`), the selection its
  Fasteners (`preselect`).
- **Timber Frame** (`build.frame`, MODIFY) lays a frame out again: a
  member picked (a click on its face or body, also inside a frame's own
  component; an edge of it selected before names it too, `withAlsoTakes`,
  so the view's context menu offers BUILD for a member's edge as for its
  face; Floor / Roof's Walls and Spacing Set's Member alike, mitcad#522),
  its frame's values in the panel by its kind, the inputs
  Wall or Floor / Roof shows for that kind (mitcad#520: a wall's Length, Height,
  Spacing, Top Plates, Corner (mitcad#510; Lapped plates for a wall saved
  without one), Openings; a floor's Length and Width when its
  outline is a rectangle, Spacing, Blocking, Openings; a roof's Spacing,
  Pitch, Overhang; Size, the members' row), previewed, `build_edit` with
  `frame` (and `config`).
- **Connector** at a junction of two timber members (`build_junctions`'s
  `wood`): the catalog's `forJunction` lists each wood fastener once
  without a row (`(sized from the members)`: the model sizes it) and the
  rows of outer connectors (brackets, hangers) that fit member a's
  section, then the joinery the junction takes (`joint:housing` where an
  end meets a side, `joint:half_lap` where members cross, `joint:notch`;
  none in line). It starts with the default wood fastener (mitcad#516: the countersunk wood screw), two of them: one click (or
  Shift + click near the junction) finishes a joint. Count and Size (Auto,
  or a row) for a wood fastener; Own Depth and Depth for a joint (else the
  joint's default); With Nuts and Screws and With End Machining only at a
  slot junction, whose list is as before. The preview shows the
  fasteners, their size and the model's warnings (embedment, edge and end
  distances); a joint's preview its cuts.
- **Panel** on a timber member's face lists sheets (`sheet`: `role:
  panel`, `mount: face`) and lays one on the face; on a profile's face
  slot panels as before; without a pick, sheets only in a design of timber
  without slot profiles.

Logs: `BUILD parts timber: <n> (<m> in the design)`, `BUILD timber starts
with <library>/<component>: ...`, `BUILD timber adds to <host>, member`,
`BUILD timber starts: length 3000 mm (the part's default), typed; along x,
flat`, `Inspect Timber: points, length 742.50 mm, flat; start snap end, end
snap none; length by two points; start on <member>, end on <member> cut;
free; adds to Component1; part <part>` (`start on none`, `; on the start's
plane`, mitcad#509), `Inspect
Timber: place grounded, length 3000.00 mm (typed), ...; along x, flat`,
`...; closes the corner at end A of <part>, shared mitre` (`square butt`),
`Timber: Tab steps Lie`, `BUILD
length typed: detached from the auto length`, `Cursor box Timber length:
2451.0 mm at x,y`, `Inspect Timber: row over 3 face(s), span 2400.0 mm, 18
member(s), gap 5 mm, the last board cut; ...`, Profile's `Inspect
Timber: place side, ...` with `; set preview 4 member(s), 4 body(ies)`
(the members laid out and the bodies drawn, mitcad#487; a row's too) and
`; set of 4, centre distance 400 mm` for a
set, `BUILD hover pick: face <face> of <body> in <occurrence>, no preview`
(`BUILD hover pick: off the parts' faces`; the hover On a member and
Spacing's between two members, mitcad#489, mitcad#508), `Placed <part> as <name> (<occurrence>), length by two points, 742.5
mm` (`length typed`, `length bound, auto: through`; `; a set of <n>`), `Spacing set of <name>: <n> member(s), centre
distance 600 mm`, `Connected half_lap at junction <id> (cross): 2 cut(s)`,
`BUILD panel picker: sheets on a face`, `Cut list timber: <m> m, <v> m3,
<kg> kg; columns ...`, `Cut list timber row: <part>: spruce C24
dimensioned, nominal 48x148, actual 48x148, 3.000 m, 0.0213 m3`, `Cut
list timber stock: <part> <size>: <lengths>`, `Cut list profiled row: <part>:
tongue and groove, cover 112.0 mm, 0.917 m2`, `Cut list profiled boards:
2.048 m2`, `Cut list: no timber
columns`, `Ribbon BUILD/TIMBER at x,y` (the group, mitcad#520), `Ribbon
button build.timber at x,y` (`build.wall`, `build.floor_roof`: the shown
page's command buttons). Tests: `tools/ui-timber-test.sh` (Timber's modes
on a wall's members: no Frame mode, Two points between studs, On a
member's preview staying the click's), `tools/ui-windows-timber-test.ps1`
(ctest `app.ui-windows-timber`; each section with its own tool) and
`tools/ui-windows-timber-frames-test.ps1` (`app.ui-windows-timber-frames`:
mitred frames of members and profiles, a member on a board). The Windows
TIMBER tests (six ctests `app.ui-windows-timber*`, run side by side,
mitcad#834) share `tools/ui-windows-timber-lib.ps1`.

Frames' logs (mitcad#479, mitcad#520): `BUILD parts wood: <n> (<m> in the
design)`, `BUILD wall adds to Component1; fasteners on; 2 x Wood screw at each
junction, sized from the members` (`BUILD floor_roof adds to ...`; a
start of Wall or Floor / Roof), `BUILD floor_roof kind roof, 2 item(s)
selected before` (`kind floor`),
`Inspect Wall: frame wall Wall 1, wall 3600 x 2400 mm, 16 member(s) (1
bottom plate, 2 cripple stud, ...); fasteners yes; adds to Component1; part
<part>` (`Inspect Floor / Roof: frame floor Floor 1, floor 4000 x 3000
mm, ...`, `... frame roof Roof 1, gable roof, ...`; `frame <kind> waiting:
<problem>; ...` while it cannot be laid
out; `...; Wall 2: from Wall 1's side, flush with end A (butt)` for a
wall started on another wall's side or end; a wall's `; corner butt` or
`; corner lapped`, mitcad#510; `; length auto` or `; length typed` for
one started on another wall without an End, mitcad#515; then `; Length
Auto: the run met a face of Body2, the wall 7819 mm` and `; spliced top
plate 5400, top plate 4800, bottom plate 5400` (a plate's joints along
the wall, mm), mitcad#536), `Frame preview refused: frame wall Wall 1,
wall 7819 x 7000 mm, 0 member(s) (); ...; reason: <the reason>` (a
refused preview's inspection, whose `Inspect Wall:` line has only the
reason, mitcad#536), `Frame spliced: top plate 4200, bottom plate 4200`
(after Place), `BUILD frame
start click: From End A, Offset 0 mm (side)`, `BUILD frame offset typed:
from End A`, `BUILD frame From Middle, Offset 0 mm: typed`, `BUILD frame
length typed: Typed length` (mitcad#514, mitcad#515), `Placed frame Wall
1 (wall, lead C3): 16 member(s), 31 junction(s) fastened`, `Frame
component Wall 1 (C2) in Component1` and `Joints by component: Component1 0, Wall 1
62` (the host first; mitcad#522, also after Timber Frame), `Cut list row:
7 x <part>, 2304 mm, A square, B square, frame Wall 1` (a frame's members
by frame, mitcad#522), `Frame
junction: Wall 2: a T on Wall 1, 1800 mm from end A`, `Frame warning:
<warning>`, `Inspect Timber Frame: frame Wall 1 laid out again: 20
member(s) (...)`, `Timber frame Wall 1: 20 member(s), 36 junction(s)
fastened`. Tests: `tools/ui-timber-wall-test.sh` (Wall: a wall with a window,
the cut list; after Place no wall previewed from the origin and Enter
refused until a click, mitcad#513; butting walls from its side near end
A, flush with it, an Offset typed moving it once, and from its end B's
face; a wall started on a wall's side whose run meets the wall opposite
ends there, no End clicked; then Lapped plates' L corner and the choice
remembered; a start on another body's face; a wall across a long room
modelled as one body, Auto to its far wall, the plates spliced, a refused
preview still naming the wall, mitcad#536), `tools/ui-timber-floor-roof-test.sh` (Floor / Roof: a floor
with blocking, Timber Frame showing its values; a roof on two walls from
the view's context menu, the kind remembered), `tools/ui-windows-timber-wall-test.ps1` (a wall, after Place no
ghost and Enter refused, mitcad#513; butting walls from its side near
end A, flush with it, an Offset typed moving it once, and from its end
B's face; with Lapped plates an L wall from near its end and a T wall
from mid-side, the choice remembered; the run meeting the wall
opposite), `tools/ui-windows-timber-openings-test.ps1` (the Openings
window; a start on a solid wall's faces; a wall across a long room, Auto,
its plates spliced, mitcad#536), `tools/ui-windows-timber-floor-roof-test.ps1`
(a floor with Floor / Roof; frames as components, a gable roof's
nails). The three Linux tests share
`tools/ui-timber-lib.sh` (the synthetic library and the log helpers).

Fasteners' logs (mitcad#497): `BUILD timber fasteners: set off; 2 x Wood
screw at each junction, sized from the members` (a Timber start in
Spacing; a wall's in its `BUILD wall adds to ...` line), `Inspect Wall:
frame ...; fasteners yes; ...;
fastener 3 x Ring nail 3.1x90 at each junction`, a set's `...; fasteners
2 x Wood screw ...` (`; fasteners no`), `Placed <part> as <name> (<o>),
length ...; a set of 6, 12 junction(s) fastened`, `BUILD swap fastener
starts with 2 fastener(s), <key>`, `Inspect Swap Fastener: swap 2
fastener(s) (wood, 2 x Wood screw 5x80) for Ring nail 3.1x90`, `Swapped 2
fastener(s) for Ring nail 3.1x90 (3.1x90); <warnings>`. Tests:
`tools/ui-timber-fasteners-test.sh`, `tools/ui-windows-timber-fasteners-test.ps1`
(both libraries have a pan-head connector screw whose name
comes first: the first time's fastener is still the countersunk wood
screw, mitcad#516, the Linux one marked `wood.default`, the Windows one
by the head-top rule).

## The view

`ViewController` (`view/`) registers the view commands and keeps display
settings in the user's settings (`ViewSettings`, group `view`);
`OcctViewer` draws them.

- **Orientation cube** (OCCT's `AIS_ViewCube`, drawn by `GlassViewCube` in
  `view/ViewCube.cpp`, mitcad#430): AIS_ViewCube's picking, hover, turns
  and logs, in glass of its own. Over a dark background dark smoked glass
  with light names, over a light one frosted light glass with dark names
  (`setViewCubeBackdrop`, from `setBackground`); the faces about 85 %
  opaque (OCCT blends in linear light, so the glass follows the
  background rather than the theme), their names painted with Qt into
  their textures (only the faces in front are drawn, so no names show
  mirrored from behind), the edges and corners AIS_ViewCube's flat bevels
  a shade apart (they are parts to click), the axes X red, Y green, Z
  blue, the hover in the accent. The Home button above it
  (`ViewCubeHomeButton`, `viewHome`) is a capsule of the same glass, its
  house in the names' colour, the accent on hover. A drag that starts on
  the cube orbits (passed to OCCT with its own key flag `kCubeDrag`);
  sketch mode leaves presses on the cube to the view
  (`OcctViewer::cubeInteraction`).
  Create Sketch's plane pick points keep away from the cube
  (`cubeArea`, `planePickPoint`), and the datums' logged pick places are
  where no other datum lies. The arrows shown when looking straight at a
  face: `OcctViewer::turnCube`.
  Its projection (mitcad#774, Preferences › Display › **Orientation
  cube**, `view/cubeProjection`: `follow-view`, `orthographic`,
  `perspective`; `OcctViewer::setCubeProjection`, applied at once): **Follow
  the View**, the default (owner 27.10.2026), is the triedron transform
  persistence alone, so a perspective view draws the cube in its
  perspective, the sides towards the view's middle in sight; **Orthographic**
  adds `Graphic3d_TMF_OrthoPers` (mitcad#766), drawn and picked the same in
  either view projection; **Perspective** is a perspective of the cube's
  own in either view projection. OCCT's transform persistence has no
  perspective counterpart of `OrthoPers`, so `GlassViewCube` makes its
  geometry in that perspective (`setViewCubePerspective`, a projective map
  of its points for the direction `setViewCubeEye` gives:
  `OcctViewer::updateCubeEye`, before and after each frame's redraw and
  before the cube is picked) and is drawn with `OrthoPers`; its faces (a
  grid of cells, so that the names stay straight), bevels, axes, hover and
  picking (its own `ComputeSelection` of AIS_ViewCube's parts) use the same
  points, so a click hits what is drawn. The eye is on the axis through the
  cube's middle, as far as a 45° field of view takes to show twice its
  box, and face on the front face is as large as the orthographic cube's:
  the size and the place in the corner are the same in all three. Tests:
  `tools/ui-view-test.sh`, `tools/ui-windows-test.ps1` (ctest
  `app.ui-windows`).
- **Camera:** `turnTo(direction, up)` fits and animates
  (`AIS_AnimationCamera`, 0.4 s; Preferences can turn it off);
  `camera()`/`setCamera()` take a `CameraState` (eye, target, up,
  projection, height), which named views store; the height is what the
  view shows from top to bottom at the target, also in a view taller than
  wide, where OCCT's scale is the width (`view/CameraHeight.hpp`,
  mitcad#959); `lookAlong` and
  `lookHome` serve Look At and named views. Fit (F6) and sketch mode's
  look at the plane are instant. Fit (`MainWindow::fitView`, mitcad#405)
  frames the selection when something with a shape is selected outside
  commands and sketches ("Fit: the selection"), the shown bodies when the
  design isolates some ("Fit: the isolated bodies", not the origin, datums
  or sketches; `OcctViewer::frameBox`, `bodiesBox`), and otherwise
  everything shown. The camera is logged once it has kept
  still for 250 ms. A design's first bodies coming into an empty view are
  fitted on their own (a design opened, a new design's first body), not
  the new bodies a command previews (mitcad#467), and only once per design
  (`view/FirstBodiesFit.hpp`, armed by `installDocument` through
  `OcctViewer::expectFirstBodies`): bodies shown again after the user hid
  every one (the light bulbs, an isolation left) stay where the view has
  them. Before, that fit framed the one body shown first and left the
  bodies shown after it out of sight (found in the bundle check of
  mitcad#529/#530; `tools/ui-browser-test.sh` checks the places).
- **Camera changes never end the application** (mitcad#773): a click on a
  cube arrow or Home in a sketch with a constraint selected ended it on
  Windows with a C++ exception out of the button's slot. The view's own
  camera math (`turnCube`, `turnTo`, `setCamera`, the fit to the free
  area) now keeps its directions square without OCCT's `gp_Dir::Crossed`
  and constructors, which throw `Standard_ConstructionError` inline in
  the application (OCCT's libraries are built without those checks): the
  up direction is made square to the view's direction, and a camera whose
  centre or distance is no number or zero turns about the origin from a
  distance of its own. A fit leaves out a box that is no number ("View
  fit left out: the shown objects' box is not finite"). `turnTo`,
  `setCamera`, `lookAtSketchPlane` and the slots of the arrows and Home
  run through
  `OcctViewer::runViewOperation`: an exception (OCCT's or a standard one)
  is logged, `View operation failed: <operation> (<step>): <type>:
  <message>` (operations `turn`, `camera`, `home`, `cube arrow up`, …;
  the step the last one reached, such as `the fit to the free area`), and
  the camera put back as it was. A click on the cube's faces turns the
  view in the frame's event handling, which logs `OCCT view event failed:`
  for a standard exception too. `tools/ui-sketch-cube-test.sh` and
  `tools/ui-windows-sketch-cube-test.ps1` (`app.ui-windows-sketch-cube`)
  select an active, an inactive and a drawn line's inferred constraint in
  turn, click every arrow and Home, and fail on a crash, a crash report or
  either line.
- **Visual styles:** hidden edges use two z-layers after the default one:
  the bodies' edges dashed without a depth test, then the visible edges
  against the faces' depth. Wireframe with Hidden Edges draws the faces
  unlit in the background's middle colour, because OCCT draws see-through
  faces after every layer sharing the depth buffer. Bodies keep
  `BodyDisplay::color`; previews, highlights and overlays are untouched.
- **Component Colors** (`view.component_colors`, View menu, mitcad#321):
  draws every body in a stable colour of its owning component
  (`BodyDisplay::component`; hues of consecutive component ids a golden
  angle apart, other ids hashed) and leaves out face appearances, in the
  shaded and wireframe styles; new bodies of a preview keep their style.
  A view setting (`componentColors`), not part of the document. Turning
  it on leaves the rendered view; entering the rendered view turns it off.
  Logs `Component colors on` / `Component colors off`; checked in
  `tools/ui-windows-test.ps1` (`app.ui-windows`).
- **Show Build Volume** (`view.build_volume`, View menu, mitcad#640;
  `view/BuildVolume.cpp`): draws the design printer's bed (the
  `build_volume` query: the root's effective printer, its bed moved so
  that its middle is on the origin, its surface at Z = 0), its excluded
  areas and its printable height as lines (`OcctViewer::setBuildVolume`,
  not pickable), and the boxes of the shown bodies that do not fit as
  dashed lines in the warning colour; drawn again after each change of
  the model (`MainWindow::showBodies`) while on. A view setting of the
  window's session, not part of the document. Logs `Show build volume
  on|off` and, when what is drawn changes, `Build volume: <printer> 256 x
  256 x 250 mm, outside: Body2, Pin (Arm:1)` (`none`), `Build volume: the
  design's printer has no bed` or `Build volume: hidden`
  (`tools/ui-print-settings-test.sh`).
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
  zooms, Alt + left orbits), and `trackpad-shift` (Shift + scroll orbits,
  Alt or Ctrl + scroll zooms; the same pan, pinch and mouse buttons).
  Pinch, rotation and two-finger double tap
  work in every scheme (`OcctViewer::event`).
  **Custom** keeps a table of button and modifier bindings alongside these
  presets. **Start from** copies the chosen preset; editing any preset row
  switches to Custom while keeping its other bindings. Left drags reserved
  for selection and component movement cannot be assigned; the orientation
  cube retains its gestures. The table includes two-finger scroll with
  modifiers and F4 + left drag. `view/navigationBindings` stores the table;
  old preset ids and the legacy index continue to load.
  Navigation's mouse and trackpad drawing shows the primary gestures; its
  table lists all modifier combinations. Orbit turns around the model,
  Pan slides the view and Zoom moves closer or farther. Constrained orbit
  keeps Z upright. **Orbit speed**, **Pan speed**, **Zoom speed** and
  **Trackpad scroll speed** range from 0.10× to 4.00×, each with **Reset**
  to 1.00× (the original behavior). They are stored in `view/orbitSpeed`,
  `view/panSpeed`, `view/zoomSpeed` and `view/trackpadSpeed` and preview
  immediately without restarting. On macOS Settings saves navigation
  changes immediately; elsewhere Preferences saves with OK and Cancel
  restores the previous navigation settings.
- **3D mouse** (mitcad#951, `view/SpaceMouseController.*`,
  `view/SpaceMouseNavigation.hpp`, the backends behind
  `platform/SpaceMouseDevice.hpp`): a 6-DOF device navigates the 3D view
  with orbit, pan and zoom together, the model following the cap (pushed
  right it moves right, lifted it moves up, pulled toward the user it
  comes closer, turned it turns the same way), smooth at the view's frame
  rate (`OcctViewer::setSpaceMouseRates`: each frame moves the camera by
  the time since the last one). It turns about the same centre as the
  mouse's orbit, with the constrained orbit's Z lock (no roll then, and a
  degree short of looking straight along Z). In sketch mode it pans and
  zooms only. Preferences › Navigation, its **3D Mouse** tab (the mouse
  and the trackpad on the **Mouse and Trackpad** tab): **Use a 3D mouse**
  (`view/spaceMouse`, on by default), **3D mouse speed** (0.10× to 4.00×,
  `view/spaceMouseSpeed`), per axis **On** and **Reverse**
  (`view/spaceMouseAxesOff`, `view/spaceMouseAxesReversed`, the axes
  `tx ty tz rx ry rz`), **Lock rotation**, **Lock translation**,
  **Dominant axis only** (`view/spaceMouseLockRotation`,
  `view/spaceMouseLockTranslation`, `view/spaceMouseDominantAxis`), and a
  command per button (`view/spaceMouseButtons`, a JSON array of command
  ids, `""` for none; by default Fit and Home), chosen from the view
  commands (Fit, Home, the standard views, Look At, Orthographic,
  Perspective). A button runs its command through its action, as a
  shortcut does; a command that is disabled now does nothing. The axes
  have a dead zone of 4 % and a gentle curve near rest. Backends, without
  a vendor SDK: Windows Raw Input (HID usage page 1, usage 8; report 1
  translation, 2 rotation, 1 with 12 bytes both, 3 the buttons), Linux
  and the BSDs spacenavd's socket (`/var/run/spnav.sock` or
  `SPNAV_SOCKET`, its original protocol, tried again every 5 s), macOS
  IOKit's HID Manager. Input counts only while Mitcad is the active
  application. The test driver's `space-mouse` and `space-mouse-button`
  steps give the controller the input a backend gives
  (`tools/ui-scripts/space-mouse.mitcad-ui`, run by
  `tools/ui-space-mouse-test.sh` and the ctest `app.ui-windows-space-mouse`).
- **Preferences** (`tools.preferences`): one page per group (General,
  with **Ask to activate the component of a picked face**, mitcad#960,
  [Activating the picked component](#activating-the-picked-component),
  Navigation, Display with the window's Layout, Cache, Cloud, Updates, 3D
  Print, Diagnostics, Textures: `TexturePreferencesBox`, the texture
  folders, mitcad#315, AI Assistants (`agents`), Fonts: `sketch::FontPreferencesBox`, the
  folders of font files Edit Text offers, mitcad#473, Sketch: **Scale the
  sketch to its first dimension**, `sketch/SketchPreferences.*`, logged
  `Preferences: scale the sketch to its first dimension on|off`,
  mitcad#860) so the window fits 1366 × 768; each page is also a command
  `tools.preferences.<page>` that the search finds by its settings. OK
  saves every page. The window opens at its largest page's size, never
  larger than the screen's available area, with its frame on the screen;
  only the page shown sets its minimum size (the hidden pages are
  `Ignored`), so it can be made smaller (mitcad#690). The page list is
  measured once the dialog is polished, with room for a scroll bar. Texts
  that come from the system do not size the window: the Cloud page's git
  status wraps at about 60 characters (git's not-installed message is some
  330), and the render device and slicer drop-downs are 32 characters
  wide, not as wide as their longest entry. On macOS the pages are the panes of `SettingsWindow`
  (Settings, Cmd+,), whose changes apply as they are made. Display starts
  with the **Camera** (Orthographic or Perspective) and the **Orientation
  cube** (Follow the View, Orthographic or Perspective, mitcad#774,
  [above](#the-view)). In builds with
  the render worker, Display also has the **render device** (mitcad#50,
  the setting `render/device`: Automatic, the CPU or a GPU the worker
  lists with `mitcad-render --list-devices`; a device of an earlier choice
  that is not there stays as "Not found"): a change starts a running
  rendered view's worker again on it, and Render Image renders on it. In
  the Floating layout Display also has the **Browser background** (Card
  or Rows, mitcad#419, [above](#browser-timeline-and-parameters)), which
  applies without a restart.
  Display ends with **Ghost inactive components** (`view/ghostInactive`)
  and **Pick the active component first** (`view/activePickPriority`),
  both on by default (mitcad#420, below).
- **Layout** (mitcad#419; Preferences › Display › Layout, after the render
  device; not in the native macOS frontend: `layoutChoosable()`):
  Docked or Floating, kept as `appearance/layout` (`docked`, `floating`;
  absent: the platform's default, `defaultChromeStyle()`: Floating on
  macOS, Docked elsewhere). The ticket's "Appearance" page is this Display
  page. `main()` reads it before the window is made (`layoutPreference()`,
  `framework/ChromeStyle`); `--chrome=docked|floating` overrides it for
  that run, and the start-up's `Chrome:` line says which decided. A change
  to a layout other than the window's asks *Restart MitCad* (Restart Now,
  Later) once the dialog is gone (on macOS's Settings window, as it is
  made); Later keeps it for the next start. Restart Now
  (`MainWindow::restartApplication`, once no model job runs) closes the
  window as quitting does (`maybeSave`: Cancel keeps MitCad open and
  nothing restarts) and starts MitCad again after this process has quit
  (`framework/Restart`: a post routine of `QCoreApplication`, as an
  installed update starts; the AppImage itself when it runs from one) with
  this run's settings options (`--ui`, `--no-native-dialogs`,
  `--no-recovery`, `--no-glass`, `--no-mcp`, `--mcp-folder`, a plain
  `--debug-log`), without `--chrome` and the one-off start actions, and
  with `--open <file>` when the design has a file. `MITCAD_TEST_NO_RESTART=1`
  logs the command instead of starting it (UI tests).
- **The Floating layout's top bar** (mitcad#419, mitcad#425): no card
  starts above the bar at the top. On macOS (`floatingTopCards()` false)
  that bar is `FloatingLayout`'s header: the title bar row with the
  workspace switch, the ribbon's groups, the notices; the upper cards' top
  is at least the header's bottom plus the margin, and the header's size
  and place are followed (resized, moved, a notice shown), so a narrow
  window whose groups collapse, or a taller bar, places them again. On
  Windows and Linux only the menu bar is a bar: the 3D view starts right
  below it, and the title row and the toolbar's groups float over the
  view's top as cards (`FloatingLayout::addTopRow`): the title bar row
  (Undo and Redo, the Solid/Build switch, the document's title with the
  project indicator, the command search; left, centre and right as before,
  the title elided when narrow) is one card (`titleCard`, a `GlassCard`
  with the `TitleBar` as its content), as wide as the view less the
  margins; below it a row of the ribbon's groups, each a card of its own
  (`Ribbon::setGroupCards`: the `CapsuleFrame` capsules in the cards'
  material, named `ribbonGroup_<TAB>/<GROUP>`), with the same buttons,
  split buttons and drop-downs, collapsing from the right in a narrow
  window as before; below them the update, remote, live and edit-lock
  notices in a card (`noticeCard`) shown while one of them is. The ribbon
  takes the mouse only on its group cards (a mask of the shown page's
  cards), each card only inside its own rectangle: a click, drag or wheel
  between and beside them reaches the view (select, orbit, pan, zoom), and
  the view picks there (`OcctViewer::setTopCards`: picks are refused only
  inside the cards, not in a margin around them). The Projects, Browser
  and Command cards start below the last row (the gap), the orientation
  cube and its buttons too, and Fit, Home and the named views fit the
  model below the rows. Docked is unchanged everywhere.
- **Inactive components as ghosts** (mitcad#420): while a component other
  than the root is active (Browser › Activate, its radio button; also its
  undo and redo; a design opens with the root active, mitcad#426),
  `MainWindow::refreshScene` gives the view the active component's
  occurrences (`OcctViewer::setActiveScope`, from the `DocumentSnapshot`:
  every occurrence of the active component, its sub-components with them),
  the sketch being edited and the origin's datums (the active component's
  own). Everything else is an inactive ghost: bodies are drawn as
  `BodyDisplay::Style::Ghost` (see-through, their own colour) but stay
  pickable, with the hover and highlights of any item; sketch curves and
  points, profiles and datums are pale (faded towards the background's
  middle, `inactiveColor`; profiles and planes more see-through). The root
  active, or the setting off (`OcctViewer::setGhostInactive`): the view as
  before. A command's ghosts (`withGhosts`, Joint's side A) and previews
  keep working on top: a body ghosted by both is drawn once, as the
  command's ghost (not pickable). Inactive ghosts hide nothing for the
  edge sight test (`occluded`). The rendered view, Render Image and
  `shownBodies()` ignore the ghosting (every body renders normally);
  Section Analysis cuts the ghosts like any body (their caps as usual);
  screenshots and the view's captures show what the viewport shows.
  **Pick priority:** while the scope is on and the setting
  `view/activePickPriority` is on (`OcctViewer::setActivePickPriority`,
  independent of the ghosting), a click, a command's pick and the hover take
  the nearest pickable item of the active component under the cursor before
  nearer items of other components (`setPickPriority` in `PickContext.hpp`:
  the selector orders its point picks with the preferred owners first, after
  its edges-first order; what is not the model's, the orientation cube,
  keeps its place). The origin's planes count as the active component's,
  except while an extent's object takes picks (`InputDef::planesByDepth`,
  `OcctViewer::setPlanesByDepth`, mitcad#762): then planes and the other
  components' items go by depth, so a face of another component in front
  of an origin plane is picked, and a plane in front of a face still is.
  Where nothing of the active component is under the cursor, the ghosts
  are picked as usual; there is no layered pick through to the next item
  under the cursor, so a ghost behind the active component is picked where
  nothing of the active component covers it, or with the setting off. Logs: `View inactive ghosts: 2 bodies (active
  Bracket:1)` (the bodies drawn as inactive ghosts, a command's ghosts
  among them; the active component's occurrence names) and `View inactive
  ghosts: none`, when that changes; `Pick priority: active Bracket:1` when
  a click took the active component's item before a nearer one of another
  component; `Preferences: ghost inactive components on|off`, `Preferences:
  active component first in picks on|off`. Tests:
  `tools/ui-inactive-ghost-test.sh`, `tools/ui-windows-inactive-ghost-test.ps1`
  (ctest `app.ui-windows-inactive-ghost`).
- **Named views** are the document's (`add_named_view` and friends); a
  view named `Home` is the document's home.
- **Section Analysis** and **Hide Above Sketch:** the cut faces are drawn
  as caps (`MainWindow::showSectionCaps`: the clipped bodies' faces in the
  plane, without those the whole body has there, as under a sketch on a
  body's face), one group per component, in the component's timeline colour
  (mitcad#382: `componentColor` / `componentColorIndex` in
  `browser/ComponentColors.hpp`, shared with the timeline; the root's bodies
  keep the default orange) in every visual style, however the bodies
  themselves are coloured. `OcctViewer::setSectionCaps` takes the groups
  (`OcctViewer::SectionCap`) and hatches each over its fill at 45 degrees
  to the plane's x axis (the sketch's in Hide Above Sketch), the next group
  at 135, in a darker shade of its own colour (`capHatchColor`), 1.5 px
  wide, with a spacing per cap face (about eight lines across a small
  face, at most 40 across all the caps of a group, at least 400th of
  them); the lines lie off the caps toward the removed side, in the layer of
  the edges in sight, so that they are drawn over the fill. The clip is the
  model's `analysis_clips` (the `clip` shape of every body shown at once,
  mitcad#438), worked out on all cores (OCCT's thread pool); a body the
  plane does not reach needs no boolean: one behind it is shown as its own
  shape, so the view keeps its presentation and meshes (no new
  presentation, no meshing), one wholly in front shows nothing.
  Section Analysis' plane, distance and Flip clip **off the UI thread**
  (mitcad#438, `framework/SectionWorker.hpp`, `MainWindowSection.cpp`):
  the model prepares the clips while the document is idle
  (`prepare_clips`), detaching them on the UI thread then: a copy that
  shares no face, edge or vertex with each body the plane may reach, and
  of the others only the box they keep (`Kernel::detach_all`). The section
  worker's thread clips the copies and meshes the new shapes together with
  the view's options (`io::mesh_all_in_place`, `viewMeshOptions`); it
  never reads a sub-shape of a body the view draws, so the view's meshes
  and a model job's may be made meanwhile. The window stays responsive and
  the view keeps what it showed, and shows the clipped bodies when they
  are ready. A newer plane, distance or Flip supersedes a round that runs
  (its result is never shown; at most one round runs and one of each kind
  waits), the panel's Cancel and OK while one runs drop it, and every
  model job tells the rounds to stop (without waiting: their result may be
  stale after it) and they are done again once the window is idle. The
  clips are kept for the plane last clipped (by body, shape and
  placement), so showing the bodies again at that plane (OK, a visibility
  change) clips nothing; a clip the view needs that is not kept (Hide
  Above Sketch, a body shown since, the refresh after a model command or
  after OK pressed while a round runs) is made on the UI thread as before
  and meshed there (faces the cut left as they were keep their meshes). `MITCAD_LOG_TIMING` logs `Timing
  section clips: … ms, <n> bodies, <m> new` on whichever thread clips. A
  body whose clip fails is shown whole, also when OCCT crashes in the
  boolean (OCCT's handlers turn the crash into that body's error,
  mitcad#392). The panel's area and length come from the `section` query
  with `world`, which sections the bodies on all cores too and skips those
  the plane passes; it is measured after the view has its clipped bodies,
  on the worker too (`prepare_query`, `Inspection::laterQuery`,
  `CommandHost::measureLater`), and the panel shows `Section area: …`
  until it arrives. A crash in the section the panel measures is likewise
  the panel's error, while the view shows the bodies clipped
  (`tools/ui-section-crash-test.sh`). Log lines of the rounds:
  `Computing section view started` and `Computing section view done in
  <n> ms` (`cancelled after <n> ms`), the same for `measure` (the UI tests'
  idle wait counts them with the model's jobs); `Section clips at
  {"plane":…} off the UI thread (section worker): <n> bodies, <m> new, <k>
  meshed in <t> ms` and `Section measured off the UI thread (section
  worker)` from the worker's thread; `Section view ready at {"plane":…}:
  <n> bodies from the section worker` before the view shows them;
  `Section round view superseded` (a running round told to stop by a newer
  one) and `Section round view replaced` (a waiting one dropped); the
  panel's `Inspect Section Analysis: measuring`, then `Inspect Section
  Analysis: area …, length …` (or `failed: …`) after the view's `Section
  caps: …`. `MITCAD_TEST_SECTION_DELAY_MS=<ms>` makes each view round wait
  that long before it clips (watching its stop), for tests that change the
  plane or cancel while one runs. What is left of
  the bodies keeps their names, so its faces, edges and vertices are picked
  as the bodies' own (Project while Hide Above Sketch is on, also the face
  the sketch lies on, which stays whole); the caps cannot be picked
  (mitcad#233). The viewer logs `Section caps: 3 colour(s) [#e0892a hatch
  #4b2d0b at 135 deg, 12 lines; ...] in <style>, bodies in <component|appearance> colours`
  (again when the style or Component Colors change). Tests:
  `tools/ui-section-caps-test.sh` (the colours per component and the
  hatch from the log; the rounds off the UI thread: the panel's numbers
  after the view's caps, a quick change of the distance superseding the
  round that runs, Cancel while one runs), `tools/ui-windows-section-pick-test.ps1` (ctest
  `app.ui-windows-section-pick`; the same rounds), `app.unit`
  (`tests/section_worker.cpp`: superseded, waiting, stopped rounds).
- **Section Analysis past every body** (mitcad#824): when the `section`
  query's answer cuts no body (`cut` 0, a zero area and outline), the
  panel shows a warning in the warning colour instead of the zero totals
  (`sectionMissWarning` in `commands/InspectCommands.cpp`): all bodies
  removed, `The plane is beyond the bodies: it cuts them all away. Change
  Distance or turn on Flip.`; all kept whole, `The plane is beyond the
  bodies: it cuts nothing.`; some of each, `The plane passes between the
  bodies: it cuts none of them.` The log line is `Inspect Section
  Analysis: <the warning>`. A saved analysis' Browser row does not say so
  (it would need the clip). Test: `tools/ui-section-beyond-test.sh`.

- **Rendered** (`view.rendered`, in View > Visual Style; only when the
  render worker `mitcad-render`, which every build has, is next to the
  application, [docs/rendering.md](../docs/rendering.md)): a
  toggle beside the visual styles, an ordinary entry, not a developer
  setting. `RenderMode` (`render/`) starts the render worker, sends it the shown bodies as a scene update whenever
  `OcctViewer::bodiesChanged` (once the model is idle: every body with
  its mesh's content hash, placement and material, and only the meshes
  the worker does not hold) and the camera and size whenever a frame was
  drawn with another one, and hands its frames to
  `OcctViewer::setRenderedImage`: the frame covers the view in a layer
  over the bodies (which stay drawn, for their depth, and pickable) and
  under the overlays, so sketches and other overlays behind a body are
  hidden as in the shaded view (`OcctViewer::setRenderedMode`). A
  render's first frame is denoised. Until the first frame of a new camera
  comes, the bodies are drawn shaded. If the
  worker dies, a message over the view says so and the mode turns off; a
  worker of another protocol version is stopped with a message too.
- **Render Environment...** (`view.render_environment`, in View > Visual
  Style after Rendered and in View > Environment; with the render worker
  only; mitcad#47): a non-modal dialog (`render/RenderEnvironmentDialog`)
  beside the main window, with the document's render settings (the
  model's `render_settings`, [commands.md](../core/model-api/src/commands.md#render-settings)):
  light (Studio, White Studio, Dark Studio, Outdoor, HDR Image with its
  path and Browse...), strength, rotation, the sun's elevation and
  direction (degrees), background (View Background, Colour, Environment)
  and its colour, ground shadows, reflections, under the lowest body or at
  a height, exposure and view transform on the Environment page, a
  Rendered view check box, Reset and Close. Fields that the light or the
  ground does not use are disabled. The Lights page (mitcad#54) lists the
  document's lights (the chosen one's fields below): Add adds a light of
  the kind beside it (Point, Spot, Area, Sun) at the camera, shining where
  it looks (`add_render_light`); Delete deletes the chosen one
  (`delete_render_light`); its name, kind, On, Follows the camera (the
  numbers change so that the light stays where it is), position and
  direction (x, y, z), colour, power (W, a sun's W/m²), size, an area
  light's height and shape, a spot's cone and blend and a sun's size each
  change it (`edit_render_light`, an undo step); Aim at Face takes the
  next click on a body's face in the view (`render::surfaceAt`: the
  bodies' display triangles under the mouse) and puts the light the
  distance beside it out along the face's normal, shining at the point
  (a sun only turns; Esc cancels); At Camera puts it at the camera,
  shining where it looks. While the dialog is open the lights are drawn
  as glyphs over the view (`render::RenderLightsOverlay`, a child widget
  of the view that takes no input: a point light a dot with rays, a spot
  its cone, an area light its outline, a sun an arrow toward the view's
  middle; the chosen one orange, those off grey). Each change is one `set_render_settings` (Reset:
  `reset_render_settings`), an undo step; not while a command panel or a
  sketch is open. `MainWindow::refreshScene` hands the settings to
  `ViewController::setRenderSettings` after every change (also undo, redo
  and opening), which passes them to `RenderMode` and refreshes the
  dialog. `RenderMode` sends the worker an `environment` command when the
  environment, the background's visibility or the ground changed, with a
  new view, so the render starts again; the exposure, the view transform
  and a background colour re-display the last frame, as a change of the
  view's background does.
- **Render Image...** (`file.render_image`, a button of the ribbon's MAKE
  group before 3D Print (mitcad#175) and in View > Visual Style after
  Render Environment; with the render worker only; mitcad#48): a
  non-modal dialog (`render/RenderImageDialog`) beside the main window
  with the render settings' `output` section (Camera: Current View or a named view; Size
  presets, which fix the aspect; Width, Height, Aspect From the View or
  Fixed; Samples, Time limit, Denoise; Format PNG, PNG 16 bits, JPEG,
  OpenEXR; JPEG quality; Transparent background), a preview, a progress
  bar and Render, Cancel, Save... and Close. Settings change as in Render
  Environment (one `set_render_settings` each, an undo step; Height only
  with a fixed aspect, JPEG quality only for JPEG, transparency not for
  JPEG). Render waits until the model is idle (`DocumentHost::whenIdle`),
  writes the shown bodies as a scene and the job (`render/RenderBatch`,
  shared with `mitcad-cli render`) into a temporary folder and runs
  `mitcad-render --batch <job> --control` (`render::FinalRender`); the
  window stays usable meanwhile. Cancel (and Close or Escape during a
  render) writes `stop` to the worker and ends it after two seconds;
  Save... copies the finished image through a file dialog filtered to
  its format, named after the design. With a fixed aspect and the
  current view, the view shows the image's frame (a child widget that
  dims the rest and takes no input) while the dialog is shown.
  `ViewController::setRenderSettings` refreshes the dialog after every
  change of the model, and the main window gives it the design's folder
  and name.

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

**Curve display quality.** The interactive context's default drawer uses
`view/ViewTessellation.hpp`: absolute chordal deviation `0.01 mm` and a
5-degree angular limit, inherited by body, wireframe and highlight drawers.
The absolute limit prevents a long body's bounds from inflating OCCT's
minimum mesh size; the angle limit preserves small circular details.
`app.view_tessellation` checks shaded boundaries and wire curves after
synthetic STEP round trips at several scales, including a long thin body.

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
synchronous; a job under 50 ms behaves as inline. A feature panel's
preview is the exception (mitcad#634): it computes in the background,
without a wait cursor or progress dialog, while the window takes input;
Esc or Cancel stops it with the command, a newer choice or selection
replaces it, a number typed meanwhile waits for it and is previewed
next, and OK waits for it. Rules:

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
  `busyChanged` suspends viewport picking before the worker starts: OCCT's
  `MoveTo` can run during a paint in the nested wait loop, and command
  selection predicates read the document. Finishing, failing or cancelling
  the job restores picking and invalidates cached hover detection.
- A cancel takes back what started it as a whole: the command is rejected
  with `the computation was cancelled` (`ComputationCancelled`;
  `lastCancelled()`), `runModelCommands` undoes the commands before it,
  Change Parameters keeps the old value, OK leaves its panel open, Finish
  Sketch stays in the sketch, Cancel of an edit panel whose roll-forward
  is cancelled closes with the timeline rolled back. A preview stopped
  while its panel stays open (the worker control of the test driver's
  `cancel-model-job`; the window has no control for that alone) is not
  cached; OK then computes the command itself. A cancel inside a
  single kernel call ends when it returns ("Cancelling...").
- A cancelled open keeps the previous document; closing the window during
  a job cancels it and closes when it is back.
- The wait cursor (`framework/WaitCursor.hpp`, mitcad#268) is a guard of
  the wait: set once when the wait starts (after 50 ms), restored once,
  when the progress dialog shows or when the wait ends (also early, and
  for nested waits), each logged (`Wait cursor on: <job> (depth N)`,
  `Wait cursor off: …`). When the window is idle (no job, no mouse button
  held, no modal dialog or popup) an override cursor left on is cleared
  (right after a job and every second), with a warning: a stuck busy
  cursor fixes itself and the debug log says after which job.
- Test switches (`framework/Diagnostics.hpp`):
  `MITCAD_TEST_RECOMPUTE_DELAY_MS` (`400,undo=800`: per job name),
  `MITCAD_TEST_JOB_SLEEP_MS` (`redo=1500`: the job of that name that much
  longer whatever it computes; `tools/ui-drawings-test.sh`),
  `MITCAD_COMPUTE_INLINE=1` (jobs on the UI thread);
  `MITCAD_TEST_WAIT_CURSOR=1` (the commands Nested Wait (Test), a wait
  waited for again inside it, and Leave Wait Cursor On (Test));
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
| `framework/SelectionScope.*`, `framework/SelectionFilterPanel.*`, `MainWindowSelection.cpp` | sketch selection filters (mitcad#217): the three-state rows, the sketch's and Project's contexts, the Selection Filter control, sketch mode's picks through them |
| `framework/CommandSession.*` | a running feature command: state, panel, selections in the view, previews, OK and Cancel, editing |
| `framework/CommandPanel.*` | the panel generated from the inputs |
| `framework/CommandRegistry.*` | every command with its `QAction`; shortcuts saved in the settings, per context where they differ (mitcad#720), the actions' keys of the current context (`setShortcutContext`); `alias(oldId, id)` keeps a renamed command's saved shortcut and `find`, `action`, `trigger` working for its old id (`file.move_to_project`, mitcad#503) |
| `framework/Ribbon.*` | the toolbar: tabs, groups, group menus |
| `framework/CommandSearch.*` | the command search (S) |
| `framework/ShortcutDialog.*` | Tools > Keyboard Shortcuts: a tree of commands with their context and key, a row per context for a command of several (mitcad#720), a context filter, the reserved keys as read-only rows (mitcad#730) |
| `AboutDialog.*` | Help > About MitCad: version, licence, third-party credits (OCCT and Qt versions) |
| `framework/ModelShapes.*` | display shapes of datums and sketch geometry from JSON |
| `framework/ManipulatorOverlay.*` | handles a command draws over the view: distance arrows, angle rings |
| `framework/Appearances.*` | appearances as the model's `appearances` query lists them (`Appearance`: the physically based parameters and the display colour), a preview swatch (`appearanceSwatch`), and the physical materials |
| `materials/TextureSource.*`, `materials/FolderTextureSource.*`, `materials/TextureBrowser.*`, `materials/TextureUse.*`, `materials/TexturePreferences.*` | texture sources (mitcad#315): the `TextureSource` interface, its registry, `TextureSettings` and the texture cache's folders; local texture folders (map detection in file names, the scan, thumbnails); Edit Appearances' grid; Use's `create_appearance`; Preferences' Textures group |
| `sketch/FontFiles.*` | font files for sketch text (mitcad#473): the font folders' setting and their files, the families and previews of Edit Text's font chooser, Preferences' Fonts group |
| `sketch/SketchPreferences.*` | Preferences › Sketch (mitcad#860): the setting of the first dimension that scales the sketch, and `scale_sketch` on a sketch action's `sketch.add_dimension` |
| `materials/PolyHavenTextureSource.*`, `materials/TextureFetcher.*` | Poly Haven as an online texture source (mitcad#313): reading the API's lists, the cached list, files and thumbnails, the downloads; the requests of online sources (Qt Network, or fixture files for UI tests) |
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
| `commands/PlaceInputs.*`, `commands/PlaceGeometry.*` | placement by picked geometry (mitcad#656): Place On and the inputs it brings for every part placed by a pick, the `at` they make, earlier placements read into them; snapping, the nearest edges and the point at two distances |
| `commands/DriveCommands.cpp` | Gear Pair and Belt Drive (mitcad#656): the candidates of a ratio between two axes, both parts made |
| `commands/PatternCommands.cpp` | Rectangular, Circular and Path Pattern, Mirror |
| `commands/PointPatternCommands.cpp` | Pattern at Points: sources and referenced targets, optional anchor, model-reported preview marker |
| `commands/ModifyCommands.cpp` | `registerModifyCommands`: the MODIFY group in menu order |
| `commands/FilletCommands.cpp` | Press Pull, Fillet and Chamfer |
| `commands/FaceCommands.cpp` | Shell, Draft, Offset Face, Replace Face, Split Face, Split Body, Delete Face |
| `commands/MoveCommands.cpp` | Scale, Combine, Move/Copy, Move Face (its summary in `commands/MoveFaceText`), Align, Appearance |
| `commands/ConstructCommands.cpp` | `registerConstructCommands`: every construction plane, axis and point, from a table; a pick of another component's geometry asks whether to activate it (mitcad#960, `MainWindow::pickComing`, [Activating the picked component](#activating-the-picked-component)) |
| `commands/InspectCommands.cpp` | `registerInspectCommands`: Measure, Interference, Section Analysis, Print Analysis, Physical Properties |
| `commands/PrintAnalysis.cpp` | Print Analysis (mitcad#620): the settings group `print_analysis`, the `print` definition, its three queries, the answers in words and in the log, the overlay's data |
| `MainWindowPrintAnalysis.cpp`, `view/PrintOverlay.cpp` | the shown Print Analysis followed and computed as model jobs; its triangles, marker and legend in the view |
| `commands/JointCommands.cpp` | `registerAssembleCommands`: the ASSEMBLE group: Joint, As-Built Joint, Joint Origin, Rigid Group, Drive Joint (mitcad#55), Constraint (mitcad#148), Contact Set (mitcad#454), Motion Link (mitcad#466); New Component (`PrimitiveCommands.cpp`) leads the group, Animate Joint, Insert Component and Insert from Library (the main window) end it |
| `MainWindowJoints.cpp` | dragging components the joints move, Animate Joint |
| `commands/SheetCommands.cpp` | `registerSheetMetalCommands`: the SHEET METAL group (mitcad#210): Face, Flange, Bend, Fold, Corner Round, Corner Chamfer, Sheet Cut, Corner Relief, Unfold, Refold, Flat Pattern, Sheet Rules |
| `MainWindowSheet.cpp` | Edit Sheet Rules, Show Flat Pattern, Export Flat Pattern and its dialog |
| `commands/SurfaceCommands.cpp` | `registerSurfaceCommands`: the SURFACE group (mitcad#455, [Surface modelling](#surface-modelling)): Surface Extrude, Surface Revolve, Surface Sweep, Surface Loft, Patch, Stitch, Thicken and the surface features |
| `commands/BuildCommands.cpp` | `registerBuildCommands`: the BUILD tab (mitcad#375, [BUILD](#build)): Profile, Panel, Linear Part, Connector, Fasteners, Swap Fastener, Length, Ends, Flip, Rotate 90 deg, Cut List, Parts List |
| `build/BuildCatalog.*`, `build/BuildModel.*`, `build/BuildLists.*` | BUILD's parts for the pickers (library commands, the design's sources); what it reads of the model (paths, placements, picks as `at`, BUILD parts, junctions, preview frames); the Cut List and Parts List dialog with CSV and PDF |
| `library/PartPicker.*`, `library/PartFavourites.*`, `library/PartGroups.*`, `library/UsedParts.*` | the part picker (mitcad#418, [Part picker](#part-picker)): the grid of the library's parts (`PartGrid`), the floating window of the part-choosing commands (`PartPicker`), Insert from Library's results; the favourites and their order; the groups' tree (mitcad#527); the design's library parts |
| `commands/SketchCommands.cpp` | the SKETCH tab: drawing tools, Sketch Dimension, constraints, Offset, Mirror, patterns, Project, Intersect, sketch Fillet and Chamfer, Move/Copy, Trim, Extend, Construction, Convert to Sketch Geometry, Delete |
| `sketch/SketchController.*` | sketch mode: mouse and keys, typed values, drags, `SketchOp` |
| `sketch/SketchTool.hpp`, `DrawTools.cpp`, `EditTools.cpp` | the tools (`SketchTool`) and their factories |
| `sketch/SketchOverlay.*` | drawn over the view: constraint glyphs, dimensions, previews, snaps, DOF |
| `sketch/SketchSnap.*`, `SketchGeometry.*`, `SketchModel.*` | snapping; plane curves; the `sketch` query parsed |
| `sketch/SketchPalette.*` | the sketch palette (options, constraints, DOF, the drawing tool's auto constraints) in the command panel's place |
| `sketch/AutoConstraint.*` | auto constraints while drawing (mitcad#189): the types a tool offers, their definitions, the geometry that satisfies them, the tool session's first and last entities |
| `browser/DocumentSnapshot.*` | the document's structure read once per change; feature icons; body volume logs |
| `browser/ActivateQuestion.*` | whether a pick of another component's geometry asks to activate it, the question's names and message box, the preference (mitcad#960) |
| `browser/BrowserPanel.*` | the browser (model tree) |
| `browser/TimelineWidget.*` | the timeline with the marker and playback buttons |
| `browser/TimelineSearch.*` | the timeline search's words and filters (mitcad#179) |
| `browser/FeatureStatus.*` | a feature's build state on the timeline and the selection card (mitcad#961): the cell's fill, status ring and badge, the card's `Failed: <error>` line |
| `browser/ParametersDialog.*` | Change Parameters |
| `browser/ConfigurationsDialog.*`, `browser/ConfigurationBar.*`, `browser/ConfigurationRulesPage.cpp`, `browser/ConfigurationsData.*` | Configurations (mitcad#807): the window, the browser's Configuration bar, the Rules page's place (mitcad#808), the configurations' JSON |
| `browser/BindingList.*`, `browser/BindingsCard.*`, `browser/ConfigurationBindingsPage.*` | Bindings at a glance (mitcad#862): the design's bindings in words, the fx card, the Configurations window's Bindings tab |
| `configurations/RuleBlocks.*`, `configurations/RuleCanvas.*`, `configurations/RulesPage.*` | the rule block editor (mitcad#808): a rule's blocks and their JSON, the palette and the canvas, the Rules tab ([The rule block editor](#the-rule-block-editor)) |
| `browser/TablesPage.*` | Change Parameters' Tables tab (mitcad#667): the design's tables, the grid with its header boxes, Import Table's preview |
| `browser/DecimalAlign.*`, `browser/DecimalAlignDelegate.*` | numbers on a shared decimal axis in the parameter table (mitcad#566): the split of a number text, the column layout, the table delegate |
| `browser/PropertySetsDialog.*` | Property Sets (mitcad#496): a body's, an occurrence's and a component's property sets, in-place editing, Add, Remove, Property Types |
| `browser/AppearanceDialog.*` | Edit Appearances |
| `browser/BrowserController.*` | what the browser, timeline, parameters dialog and failed-features summary do |
| `browser/DocumentHost.hpp` | what they ask of the main window |
| `view/ViewController.*` | the View menu: standard views, Look At, camera, visual styles, grid and snaps, environment, named views, the cube's menu, Preferences, the keyboard and mouse overview; `ViewSettings` |
| `render/` | [docs/rendering.md](../docs/rendering.md): View > Rendered (`RenderMode`, `RenderClient`, `FrameMemory`), Render Environment, Render Image (`RenderImageDialog`, `RenderBatch`: the final render's scene, camera, job and worker process); the worker `mitcad-render` (`RenderWorker`, `CyclesRenderer`, `RenderOutput`) |
| `LayoutGrid.*` | the layout grid |
| `files/MainWindowFiles.cpp` | the File menu (mitcad#175; groups: New Design, Open, Open from Project (mitcad#530), Open Recent, Open Read-Only, Close; Save, Save As, Save to Project (mitcad#503), Save with Description, Version History; the Project submenu with New Project, Open Project, Open from Cloud, Sync, Check for Newer Versions, Open in Browser, Project Settings; Import, Export; Recover Designs; Exit. 3D Print, Insert Component and Render Image are on the ribbon only): open, recent files, close, import, export, Insert Component, drops |
| `files/FileFormats.*`, `files/FileDialogs.*` | file kinds by extension and dialog filters; Export, Insert DXF, Insert Mesh, linked or copy, the import report |
| `framework/DialogFolders.*` | the file dialogs' folders per group, remembered across restarts ([below](#file-dialogs-folders), mitcad#498) |
| `files/F3dImport.*` | `.f3d`, FreeCAD, `.ipt` and `.iam` import in a worker process behind a progress dialog |
| `files/Autosave.*`, `files/Recovery.*`, `files/MainWindowAutosave.cpp` | autosave and recovery |
| `files/MainWindowVersions.cpp`, `files/VersionDialogs.*` | saving versions: Save and Save with Description in a versioned project, the author, changes and renames made outside MitCad, the indicator's version |
| `files/VersionHistory.*` | the Version History window |
| `files/MainWindowProjects.cpp`, `files/Projects.*` | Local and Cloud projects (mitcad#89): the window's current project (`ProjectState`), New Project, Open Project, Open from Cloud, Open Read-Only, Project Settings, Save to Project |
| `files/ProjectDialogs.*`, `files/ProjectSettings.*`, `files/CloudSection.*`, `files/CloudAddress.*` | their dialogs: New Project, Open from Cloud, Change Address, Project Settings; the shared Cloud section (service, address, the check, Trust This Server); addresses composed and read back (Qt Core, `app.unit`) |
| `files/ProjectIndicator.*` | the project indicator and its menu |
| `projects/` | the Projects panel (mitcad#145, [below](#projects-panel)): `ProjectsPanel` (the tree, names typed in place, drags), `ProjectsTree` (the projects read with `project_tree`, their names), `ProjectRowText` (the rows' marks and the middle elision, mitcad#529), `MainWindowProjectsPanel.cpp` (its dock or card, View › Projects, File › New Design, what the panel asks for); the [project picker](#project-picker) (`ProjectPicker`) and File › Open from Project's [project browser](#project-browser) (`ProjectBrowser`, `ProjectBrowserData`, mitcad#530) |
| `files/Remote*.*` | the current Cloud project's remote: sync, check for newer versions, open in browser, sending versions, Resolve Sync Conflicts, Preferences' Cloud page; `RemoteTask` runs one operation (a project's or a projects command) on a thread of its own |
| `files/Print3d.*`, `files/MainWindowPrint.cpp` | 3D Print: slicers found, settings, file names, the start bodies and the Slicer row's choices, a slicer's dialect of print settings, the Preferences group; the command's panel (`print3dCommand`) and its send (`sendToSlicer`); the Printer and Process rows (a changed library item asked about once) and Import from slicer… with its line (mitcad#655) |
| `print/PrintKeys.*`, `print/PrintLibrary.*`, `print/SettingField.*`, `print/SettingsForm.*` | the key table's categories and values, the choosable items and their places, the settings form and its fields (mitcad#655) |
| `print/MaterialsDialog.*`, `print/ProfilesDialog.*` | Materials (Edit Materials, Material...) and Printers and Processes (Edit Printers and Processes, Edit of Printer and Process) (mitcad#655): each window a panel (`MaterialsPanel`, `ProfilesPanel`) on the design, which My Library shares (mitcad#704) |
| `print/MyLibraryDialog.*`, `print/PrintItemsHost.*` | My Library (Tools › My Library…, mitcad#704) and what the panels work through: the open design's model (`DesignItemsHost`) or a model of My Library's own (`LibraryItemsHost`) |
| `print/PrintImportSummary.*`, `print/ImportDetailsDialog.*` | an import's line under Import from slicer… and Details…, each distinct problem once with the number of profiles that had it (mitcad#654, mitcad#655) |
| `print/SlicerImport.*` | Import from Slicer… (mitcad#829): the one import of 3D Print, Printers and Processes and My Library (file dialog, `import_print_profile`, log) and the windows' row with its line and Details… (`SlicerImportBar`), with the menu of an installed slicer's presets (mitcad#830) |
| `print/SlicerPresetsDialog.*` | Import Presets (mitcad#830): an installed Bambu-family slicer's user presets, and its own presets the user enabled there (mitcad#871), to tick (a name filter, the count on Import), same names marked with Copy or Override |
| `framework/NoteRow.*`, `view/BuildVolume.cpp` | the panel's note row (mitcad#655); Show Build Volume (mitcad#640) |
| `update/` | automatic updates ([docs/updates.md](../docs/updates.md)); the download in a private, random-named temp folder on Windows and macOS; on Windows, `mitcad-updater.exe` hashes the installer again through a handle without write or delete sharing and runs it only when it matches `--sha256` from the verified manifest (mitcad#603, `UpdaterChecks.hpp`); on macOS, verified disk image, read-only mount, signature-preserving copy, atomic bundle exchange after quit, rollback and Launch Services relaunch |
| `report/` | feedback and error reports ([below](#feedback-and-error-reports)): `ReportCenter` (Help › Send Feedback, the offers, the delivery), `ReportDialogs` (the form, the screenshot's crop, the preview), `ReportText` (masking, crash files, duplicate keys, the issue form's address; Qt Core, `app.unit`), `CrashHandler` (the signal and exception handlers, plain C++, also in `mitcad-render`) |
| `MainWindow.*` | the host: document, modes (idle, picking a sketch plane, sketch, command), selection, context menu, actions |
| `OcctViewer.*` | the 3D view: bodies in the visual style, previews, highlights, picking, camera, orientation cube, navigation |
| `view/ViewCube.*` | the orientation cube's glass and its Home capsule (mitcad#430) |
| `view/SpaceMouse*`, `platform/SpaceMouseDevice.hpp`, `platform/*/SpaceMouse*.cpp` | 3D mice (mitcad#951): the mapping to the camera and the settings, the controller, the camera's motion in `OcctViewer` (`SpaceMouseView.cpp`), and the backends (Raw Input, spacenavd's socket, IOKit HID) |
| `icons/*.svg` | own icons (see [Icons](#icons)) |
| `web/` | the web viewer's pages (mitcad#423, [Web page](#web-page)): `WebPage` (`writePage` of the open design, `writePageOfFile` of a design file without a window, `pageHtml`: the viewer's template with its import map, module and data inlined), `WebGlb` (the bodies as the viewer's glb: a node per placed body, appearances and textures; the component tree); the viewer itself in `web/viewer/` |
| `web/MainWindowWeb.cpp`, `web/ProjectWeb.*` | a project's web viewer (mitcad#423): Web Viewer, Update Web Viewer, the Save that writes a design's page; the `web/` folder's slugs, `designs.json` and `index.html` (Qt Core, `app.unit`) |

### Import

By extension (`files/FileFormats`); `--open` and dropped files take the
same ways without questions (mm, the XY plane, linked).

| Kind | How |
|---|---|
| `.f3d`, `.f3z` | a new document with the design's history (worker process, below) |
| `.FCStd` | a new document with the bodies and PartDesign/Part history (`import_fcstd`, same worker; commands.md, "FreeCAD import (.FCStd)"); no Stop |
| `.ipt` | a new document with the part's stored bodies, one base feature each (`import_ipt`, same worker, `hang_limit` 90 s; commands.md, ".ipt import"); no Stop; the report lists the bodies, part number, material and units |
| `.iam` | a new document with the assembly's components and occurrences, each part imported once (`import_ipt`, which imports an assembly with `import_iam`; same worker; commands.md, ".iam import"); no Stop; the report lists the occurrences, how each file was found, missing files and the checks against the file. **Import Assembly** (`askIamImport`, object name `importAssembly`, mitcad#507), asked by File › Open and Import (not `--open` or MCP, which take the choice made last): **Parts with their design history** (`iamPartsHistory`, on by default, remembered as `import/iamPartsHistory`; `files/IamImport`) sends `history` true, so each part's design is replayed into its component (parameters, sketches, features), else its stored bodies come in as one base feature; **Import** (default) or Cancel (the import does not start). The worker's trace `import: [t] part <n> of <m>: <file>` (and `…: <file>: replayed in <s> s`, `bodies`) shows in the progress dialog as `Part 3 of 7: <file>`, with `, timeline item <k>: <name>` while the part's items are replayed, the bar counting parts. The report says how the parts came in (`Parts with their design history: 6 replayed, 1 as their stored bodies.`) and, per part, `imported, replayed, 12 items (10 parametric, …), 9 verified, 3 parameters` or `imported, stored bodies: <reason>` (a replay that failed or hung, a part without a design history) |
| `.rvt`, `.rte`, `.rfa`, `.rft` | a new document with a project's building elements, one component per level (`import_rvt`, same worker, mitcad#274; commands.md, ".rvt import"); a family file: its metadata only; no Stop; the report lists the bodies per level, what was left out and why, and the components are logged (`Imported <file>: components <name> (<n> bodies), …`) |
| `.solidcraft` | a new document with the design's history (`import_solidcraft`, same worker, mitcad#450; commands.md, ".solidcraft import"); no Stop; the report lists every feature and joint with how it came in (parametric, partial, unmapped, failed) and the kinds without a MitCad counterpart, and is logged (`Import report: <file>: <counts>`) |
| STEP, IGES, BRep | a base feature (`import_file`, one undo step `Import part.step`) |
| STL, OBJ | mesh bodies, after asking the unit (Insert Mesh) |
| DXF | a new sketch on a plane (`sketch.create` + `sketch.import_dxf`, one undo step `Insert drawing.dxf`), or into the edited sketch; the dialog uses the `dxf_info` query (origin placement, layers) |
| SVG | as DXF (`importDrawing`; Insert SVG, `insert.svg` and in sketch mode `sketch.insert_svg`, mitcad#473): a new sketch on a plane (`sketch.create` + `sketch.import_svg`, one undo step `Insert logo.svg`) or into the edited sketch; the dialog (`askSvgInsert`) shows the `svg_info` query's size and contents and asks the scale, where the lower left corner goes and whether strokes become bands; in a new sketch the `filled` regions the result lists are selected as profiles, so Extrude takes them (`tools/ui-svg-test.sh`) |
| `.mitcad` | Insert Component, linked or as a copy (`insert_component`); in a project with versions the design is chosen in the [project picker](#project-picker) (Another File... the file dialog; mitcad#176) |

**The `.f3d` worker** (`files/F3dImport`): the application runs itself as
`mitcad --import-worker <file> --output <design> --result <json>` (no
window; `main.cpp`), which runs `import_f3d` with `hang_limit` 90 s and
no time limit on a new document under a job (`attach_job`) and writes the
design file and result; it also gets `--result-store` and
`--persist-min-ms` and stores the design's results. It hands the
import's evaluation over to the window (mitcad#413): with
`--hand-off-store <folder>` (in the import's temporary folder, whether
the user's store is on or not) it writes every result a store takes,
however quick (`hand_off_results`), and prints `import-handed-over <n>`;
the window reads the design with that store (`installDocument`'s
`handedOver`, logged `Results handed over for <file>: restored <n>,
evaluated <m>`), so it restores those results instead of evaluating
every feature again, then gives the document the user's store. A kernel crash or
hang ends only the worker. While import threads the hang watchdog gave
up still run, the worker ends with `std::_Exit` after writing its result
(`abandoned_imports()`), without the static destructors that would crash
under them (mitcad#82). Progress: the worker runs with
`MITCAD_IMPORT_TRACE`, and its per-item lines (`import: [t] item <n>:
<name>` when an item starts, `import: [t] <item>: <outcome> in <s> s`
when done; core/import) are counted against the timeline's length
(`import_f3d`'s `list`); a change of that format only loses the count.
**Stop and Keep What Is Imported** writes `stop` to the worker's stdin;
the worker cancels its job, which `import_f3d` takes as a stop request
(`core/import/docs/f3d-performance.md`, *Stop*). **Cancel Import** kills the worker;
nothing of it is kept. The window then loads the design unsaved (`*`)
and shows the import report. For tests,
`MITCAD_TEST_RECOMPUTE_DELAY_MS=import_f3d=<ms>` slows every definition
the worker tries, `import_dwg=<ms>` every item of a drawing it reads
(`tools/ui-dwg-import-test.sh`, mitcad#589: Cancel while the drawing is
read, and while the window reads the imported design).

**Keep the imported bodies?** (mitcad#283) The worker runs `import_f3d`
with `retry_bodies` `ask`: the first time an item is to be tried again on
the file's stored bodies before it, and that would replace replayed
bodies that already match them within the geometric tolerance, the
import asks through the job (`JobControl::question`). A thread of the
worker prints it as `import-ask <question>`; the window shows it in a
question dialog over the progress, **Keep the Imported Bodies?**
(`importRetryBodies`), whose buttons are **Keep Imported Bodies** (the
default, also Esc) and **Replace with Stored Bodies**, and writes
`answer keep` or `answer replace` to the worker's stdin, which passes it
to the job (`JobControl::answer`). The answer holds for the rest of the
import. The import report's **Retries on the file's stored bodies**
section names the answer and, per item retried, the bodies kept and
replaced. Log lines: `Import of <file> asks: <question>`, `Import of
<file> answer: keep` (`replace`), `Import report retry: <file>: <line>`.
For tests, `MITCAD_TEST_IMPORT_QUESTION=1` makes the worker ask at the
start of the import (`tools/ui-import-retry-test.sh`).

**Keep Deviating Results?** (mitcad#379; `files/ImportDeviations`) An item
of an `.f3d` design whose decoded definition builds a result that deviates
from the bodies stored in the file comes in as the file's stored geometry,
and the import report lists it with its numbers (`deviation`,
core/model-api/src/commands.md). Once the import report is closed, with
the design in view, the window asks about those items in one dialog
(`importDeviations`), a row each (table `importDeviationsTable`): **Keep
Parametric** (a check box, Space toggles the selected row; unchecked, *Use
stored*, is the default), the component, the item's name (*without n
edges* for a rounding offered without the edges the kernel fails on,
named in its tooltip) and type, the
volume it adds or removes against the file's (mm³), the difference (mm³
and per cent), the largest distance of its new faces from the file's
(mm), the share of their area beyond the geometric tolerance, and the
volume of the bodies it changed against the stored ones. Selecting a row
selects the faces of the design where the item deviates (the report's
`faces`: of the stored geometry standing in for it), so the view shows
where. Buttons: **Keep All Parametric** (Alt+K), **Use Stored for All**
(Alt+U), **Apply** (the default, Enter: the rows checked) and **Cancel**
(Esc: the design stays as imported). Items kept make the worker run the
import again on the same file and options with `accept_deviations` (the
items kept before and now), which replaces the document: those items are
parametric with their results (the report's `accepted_deviation`, their
features' timeline note *Deviation from the file accepted at import: …*)
and the items after them are replayed on those results. Then the dialog
comes again for the items still deviating that were not decided, and for
those after the first item kept in that run (replayed on its result, their
numbers can change); items left as the stored geometry before are not
asked again otherwise. A stopped import does not ask. Log lines: `Import
deviations: <file>: <n> items`, `Import deviation: <file>: <index> <name>
(<type>): volume change … mm3 against the file's …`, `Import deviation
shown: <file>: <name>, <n> faces`, `Import deviation choice: <file>:
<name> keep|stored`, `Import deviations answer: <file>: keep <name>
(<index>), …` (or `stored for all`), `Import of <file> runs again keeping
<n> items parametric`. UI test: `tools/ui-import-deviations-test.sh`.

**Documents the design refers to** (mitcad#196): before the import starts,
the worker lists the documents an `.f3d` design's components come from
(`mitcad --import-worker <file> --list-references --result <json>
[--import-options <json>]`, which runs `import_f3d_timeline` with
`list_references` and the files chosen so far, writes the listing and
prints `import-listed`; mitcad#922: no reader of the file runs in the
application's process). The progress dialog shows `Reading the documents
<file> refers to` meanwhile, its Cancel ends the listing; a listing with
no answer in 300 s is ended and the import goes on without its dialogs.
A listing that fails with its own message (`import-failed`) is logged
(`Import references of <file> not listed: <why>`) and the import goes on
(it says what is wrong with the file); a listing process that stops (a
crash, a reader's panic) fails the import with `the import process
stopped (it crashed): <its last line>`, the session untouched. When the
listing returns, for each document whose
file is not found (no reference folder next to the design, or no file
there for it), opens a file dialog titled with the document's name (where
a reference folder gives one) and version, `Find Part, Version 2`, whose
buttons are **Use** and **Skip**. A chosen file is used for that document
and its folder is searched for the others (by the GUIDs their files
list), so the worker lists them again after each (a skip changes
nothing, so the next one comes from the same listing); a skipped one
leaves its components empty, the report saying `skipped`. A building
project's linked models are listed the same way, once
(`--list-links`, `import_rvt` with `list_links`; `Reading the linked
models of <file>`), and asked as `Find the Linked Model <name>`. Logged:
`Import references of <file>: listed in the import process` (`Import
links of …`) at each listing. The worker gets the chosen
files and the skipped documents as `reference_files` and
`skipped_references` (`--import-options <json>`); the import report lists
the documents referred to, where each came from (core/model-api/src/commands.md,
*Components of other documents*). Tests: `tools/ui-windows-references-test.ps1`
(`app.ui-windows-references`), `tools/ui-import-listing-test.sh` (the
listings in the import process, and a listing's panic on a corrupt design
stream, `MITCAD_TEST_PARSER_PANIC`, ending only that process).

### Export and 3D Print

**Export:** STEP AP214/AP242, IGES, STL, OBJ, BRep, 3MF (appearance
colours as `colors`), USDZ for all or the selected bodies (faces, edges and
vertices stand for their bodies), and a sketch to DXF (`export_sketch`).
STEP and IGES are written in millimetres unless the Unit row says
otherwise. Bodies are where the design shows them (the model's default;
a STEP of moved bodies is an assembly); with occurrences, Coordinates can
give `coordinates: component`. USDZ (mitcad#123) takes the Refinement
row; the model writes the bodies' appearances itself, and a saved design
passes its folder as `design_folder` for relative texture paths; the log
line adds the materials (`…, 2 material(s)`) and each texture written in
its base colour is a warning (`Export: Wood: the texture image …`). A web
page (`.html`): [Web page](#web-page). A drawing with views,
sizes and a hole table (PDF, SVG, DXF, DWG): File › Export Drawing
([Drawing](#drawing)).

**3D Print** (`make.print3d`; `files/MainWindowPrint.cpp`,
`files/Print3d`) is a panel (mitcad#234; an `accept` command, see
[Glue](#glue)):

- **Bodies** (`SelectKind::Body`, any number): starts with what the
  selection stands for (`printStartBodies`: a face, edge or vertex its
  body, an occurrence the bodies shown in it), or with nothing of that
  selected the bodies with a shown instance (`instances`); a body item per
  placement, with its occurrence. A click on a body in the view adds or
  removes it (Ctrl and Shift as in other inputs, a window adds several);
  the bodies in it are highlighted whole in the input's colour. Its
  `withAccepts` takes solids and mesh bodies only (`geometry::body_kind`):
  sheet and empty bodies cannot be picked, and those of the start
  selection are left out with a warning.
- **Format** (STL, 3MF), **Refinement** (Low, Medium, High, Custom with
  **Deviation** and **Angle**, checked in `build`), **Open in** (mitcad#655:
  Folder only, the files' folder; the settings' slicer, the ones found, Other program... with
  a file dialog in its `withOnChange`; a choice is the slicer as JSON,
  `slicerChoice`). They start with the settings, group `print`: `format`,
  `refinement`, `deviation`, `angle`, `slicerName`, `slicerProgram`,
  `slicerArguments`, `slicerArgumentsAfter`; Send (Enter) saves them, the
  format also as soon as it is chosen (3MF the first time, mitcad#655).
- Send writes the files and starts the slicer (`sendToSlicer`): each body
  once; of a body shown more than once only the placements picked (all
  when none is; `printPieces`), STL as a file each, 3MF in one `export`
  with them as `occurrences` (mitcad#856: one occurrence of a nut placed
  many times selected sends that nut alone). In the 3MF file, when two or
  more of the placements sent are of bodies with print bed faces, each is
  an object of its own turned onto its face, on the bed and set side by
  side (a nut placed many times, each turned differently, all on their
  faces); a component selected when the panel started is sent whole
  (`whole`, the occurrences that still hold a body picked): one object as
  assembled. The last line adds how many placements lie on their print
  bed faces (`on_faces` of the `bed` results): `Sent 3 bodies to Bambu
  Studio. 3 bodies on their print bed faces.` (`Sent N bodies` counts the
  placements sent); problems as before, in the warning colour. Left room
  for filament assignment (mitcad#21): further rows of the panel.
- **The print bed face of a piece with several** (mitcad#857): when a 3MF
  piece is kept as assembled (a component sent whole, or bodies kept
  together) and two or more print bed faces apply to it, counted per
  placement, Send asks which one before any file is written. It first
  runs the `export` with `plan` (nothing written) and reads each piece's
  `choices` (`printBedAsks`, Print3d.hpp); for a piece not chosen for yet
  (by its key, the piece and its faces, in `PrintPanel::bedFaces`) it
  logs `3D Print bed face to choose for the file: 2 faces: Tall F1:top in
  Pair:1; Low F2:side0 in Pair:1` (`for Nuts:1` for an object of its
  own of several), shows the panel's question row (`bed_question`, in the
  panel's normal text: *Not sent yet: Pair:1 has 2 print bed faces.
  Click the green one to put on the bed.*, `printBedQuestion`: the
  object's name, else the occurrence its faces share, else "the piece";
  logged `Panel 3D Print note bed_question: …`), the **Bed face** row (a
  face selection, required while shown, so it says *Select* and Send is
  disabled, that takes only the faces asked about:
  `MainWindow::isPrintBedFace` with the body and the occurrence) and
  **Send as assembled** beside its field (`withBesidePrevious`), and
  calls `CommandSession::ask` with the same question for the status bar
  (`Panel 3D Print asks: …`); the faces are the command's `marks`, in the
  bed face's green (`printBedColor`) on every occurrence, and Bodies'
  highlight is left out meanwhile; `accept` returns false and the panel
  stays. A click on one of the faces chooses it (`3D Print bed face of
  the file: Tall F1:top in Pair:1`) and sends; a click on another face
  picks nothing and says *Not one of the print bed faces: click a green
  face.* in the status bar (`withRefusedHint`); the mouse on a green face
  shows its body there (`hover`: *Tall: print bed face*, logged `3D Print
  hint: …`). Tab picks the green faces in turn (`withTabPicks`: the one
  picked in the Bed face's blue, named in its field, `3D Print Bed face
  listed: Tall: print bed face`, the others green) and Enter sends on
  the one picked. Send as assembled sends the piece as assembled (`3D
  Print bed face of the file: kept as assembled`). Esc (`escape`) only
  ends the question (`3D Print bed face of the file: not asked any more
  (Esc)`): its rows hidden, the marks gone, nothing sent or remembered,
  the panel open; the next Esc cancels the panel as always. Either
  answer is remembered for the piece until the panel closes (`init` and
  `ended` forget them), and Send runs again at once: the `export` names
  the faces chosen in `bed_face` (`{body, occurrence}`), each turning its
  piece onto it; with two pieces to ask about, each is asked in turn, its
  question naming it. The last line names a face chosen (`printBedLine`:
  `Sent 2 bodies to Bambu Studio. Pair:1 on Tall's print bed face.`,
  then the other placements on their faces; logged `3D Print last line:
  …`). Changing Bodies or the format asks no more (the rows hidden, the
  marks gone). One bed face, or none: nothing is asked.
- **Format** is `STL` or `3MF`; the panel's tooltips and choices are
  plain values (Open in: `Folder only`, the slicers, `Other program…`).
- **Printer, Process, Materials** (mitcad#640, mitcad#655), enabled with
  3MF only (disabled, not hidden, with STL: `withEnabled`): Printer and
  Process each a box of `Slicer default` (always first; choosing it takes
  the design's choice away, `remove_property`) and the choosable printers
  or processes (`library_items`) by name in a steady order, no places, so a choice never moves them, the full name shown
  (elided in the middle only when it must), and last, below a separator
  line (`choiceSeparatorPrefix()`, mitcad#828), **New printer…** /
  **New process…** (`MainWindow::newDesignProfile`: a new profile in this
  design named by the model, `Process 1`, `Process 2`, ...,
  `create_print_profile` with `choose`: its choice in the same
  undo step, nothing written to a library (Save to puts it in one), and
  opened in Printers and Processes; logged `3D Print <kind> new: <name> (<key>)`). A
  choice changes the design at once, one undo step (`set_property` of the
  root's `Print Printer`/`Print Process` `profile` with the item's
  library: `Use Printer <name>`); Cancel does not take it back (the
  owner's decision). **Edit** (the same small pencil, tooltip Edit, on
  the Printer, Process and Materials rows) beside each opens Printers and
  Processes on the chosen item; on Slicer default it is disabled
  (`withButtonEnabled`). Under Process the row **Own settings**, always
  shown, as the other rows: its value on the label's line, what bodies,
  occurrences or components set of their own in the one wording,
  `Body1: Walls; Arm: 3 settings` (`None` without any; the chosen row's
  `overrides`, `MainWindow::processOwnSettings`), and the pencil at the
  right edge (`Panel 3D Print action process_overrides.0`): with one body
  printed its own settings in the settings form (Own settings), with
  more a menu of the printed bodies' names (logged `3D Print own
  settings menu: Body1 | Body2`); the process editor shows their values
  beside each field (`4 on Body1`).
  **Materials** sums up the bodies' materials, wrapped only between
  names (`Shop PETG, PLA Plastic ×2`: no-break spaces in each name; a
  count only above one; a body without a material of its own `Default
  (Steel)`; the
  `body_material` query of each body); its pencil (`Panel 3D Print action materials.0`)
  opens Materials for those bodies; after an import that added materials
  a plain action **Assign…** (`materials.1`) opens it
  for the panel's bodies and stays open after each Assign
  (`openMaterial`'s `keepOpen`), so several materials go to different
  bodies. **Purge volumes** (mitcad#705), under Materials, sums up the
  table of the bodies' filament slots (the model's `purge_volumes` query
  with the panel's bodies): `Computed`, or how many pairs are set by hand
  (`1 set by hand`); `None` with one filament (nothing to change), without
  the pencil. Its pencil (`Panel 3D Print action purge.0`,
  `BrowserController::openPurgeVolumes`) opens **Purge Volumes**
  (`print/PurgeVolumesDialog.*`), a small window beside the panel: a row
  per filament changed from (`From White PLA`) and a column per filament
  changed to (`To Black PETG`), each header with the material's colour,
  each cell its volume in mm³ (`—` from a filament to itself). A cell
  shows the computed volume until it is typed (Enter or leaving the cell
  saves it; a number, the unit optional): then it is the design's own for
  that pair of materials, bold on a tinted ground, its tooltip the
  computed volume; **Reset** gives the selected cells set by hand their
  computed volumes again. Each edit and each Reset is one
  `set_purge_volumes` and undo step; the window follows the model (an
  undo, a material's purge values or colour, the process' multiplier).
  Logged `Purge volumes opened: <materials>`, each pair `Purge volume
  <from> → <to> = <n> mm³` with ` (set by hand)` when it is, `Purge volume
  set: <from> → <to> = <n> mm³`, `Purge volumes reset: <n>`, refusals
  `Purge volumes refused: …`, and the places `Purge volumes cell
  <row>.<column> at x,y` (from 0), `Purge volumes reset at x,y`, `Purge
  volumes close at x,y` (`tools/ui-purge-volumes-test.sh`,
  `tools/ui-windows-purge-volumes-test.ps1`). A material's
  own purge values are two fields of the material editor's Filament tab
  (Purge into this filament, Purge out of this filament, under Purge
  volumes: keys of the key table), their placeholders the colour's values
  (the `purge_defaults` query with the filament colour, else the
  appearance's). The design's printer or process saved in a library that
  another design changed is asked about once when the panel opens or it
  is chosen (`askChangedElsewhere`, logged `3D Print changed elsewhere:
  <name>`). **Import from slicer…**, the first print row (it fills Printer,
  Process and Materials), the one button in the field column, never clipped (`withFieldColumn`; a file dialog
  titled Import from Slicer: `.3mf`, `.json`, `.ini`, `.bbscfg`,
  `.bbsflmt`, `.zip`) runs `import_print_profile` into the design (the
  root chooses what it read; one undo step); under it, on a line of its
  own as wide as the panel, one short line
  names the file and counts only what failed (`mount.3mf imported`,
  `mount.3mf imported. 1 setting not read`; `print/PrintImportSummary.*`),
  itself the link to Details (`import.1`); the line goes when the panel
  closes. Details shows the whole report in a small read-only view titled Import
  Details (Copy, Close), each section only when it has something:
  what came in by name under the title, without a heading of its own (a
  heading in the singular for one, and the same count of what failed),
  Left out, Not read (by plain names,
  each distinct problem once with `(4 times)`, the settings with no
  plain name counted as `3 other settings`). The rows
  follow the design when Printers and Processes, an import or Update changes
  it (`CommandDef::modelChanged`). The rows are note rows and choices of
  the panel (`noteInput`, `withChoiceIcon`, `withButton`); while 3D Print
  is open the print windows may change the design
  (`DocumentHost::canChangePrintSettings`). 3D Print has no print
  settings of its own: the printer, the process and the filaments are the
  design's. A 3MF to a slicer whose
  dialect is known (`slicerDialect`: Bambu Studio, OrcaSlicer, QIDI
  Studio, PrusaSlicer, UltiMaker Cura by the slicer's name or program)
  goes with `export`'s `dialect`: each body's effective settings in the
  slicer's own files ([commands.md](../core/model-api/src/commands.md#print-profiles));
  with Slicer None (open the folder) a 3MF still carries the printer, the
  process and the materials, in the dialect of the design's printer (the
  slicer named in its `printer_settings_id`, else the first of its
  `raw_slicers`), else Bambu Studio's (`MainWindow::designDialect`):
  nothing is dropped silently;
  the log adds `3D Print: print settings for <dialect>, filaments 1,2`
  and the warnings (more materials than the printer's slots), which the
  status line shows in the warning colour, one per line, with the last
  line of the send, by plain names and the slicers' titles, as the
  import's report (`PrintKeyTable::plainText`; logged `3D Print warnings:
  <text>`). STL carries no settings (its choice says so).
  The panel logs `3D Print printer: <name>` (`Slicer default`; the root's
  effective printer, `designPrinterName`), `3D Print printer: <place>/<id>`
  (`process`; `slicer default`) when a row follows the design, `Imported
  print settings from <file>: printers: …; processes: …; materials: …;
  mapped <n>, kept <n>, left out: …[; not read: …]`, `3D Print import:
  <the line>`, `Import details opened: <n> lines` with `Import details:
  <line>` and `Import details closed`, `3D Print printer update not now:
  <name>`, `3D Print printer updated: <name>`
  (`tools/ui-print-settings-test.sh`,
  `tools/ui-windows-print-settings-test.ps1`). QIDI Studio is found as the
  others (`kKnownSlicers`: `QIDIStudio\qidi-studio.exe`, `qidi-studio`
  on PATH, `QIDIStudio.app`; no Flatpak).

- Files go to `<QDir::tempPath()>/mitcad-print/<design>`
  (`preparePrintFolder`: emptied before each send; `mitcad-print` is made
  for the user alone and refused when it is a link or another user's,
  since /tmp is shared on Linux), through the model's `export`: 3MF one
  file named after the design with `colors`, `occurrences` (each body's
  placements picked, `[]` for a body shown once) and `whole` (mitcad#856);
  STL one per body
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
- Tests: `tools/ui-print-test.sh` (a fake slicer script; its part 8 the
  bed face choice of mitcad#857: a component of two bodies with bed faces
  sent whole, the two faces highlighted, Bodies' highlight left out, the
  question in the row and the status bar, Send as assembled beside the
  field; Enter refused while asking, the hover's and a refused click's
  status bar texts; a click on one sends a 3MF on that face and the last
  line names it; Tab and Enter choose a face; Esc ends the question,
  nothing sent, and the next Esc cancels; Send as assembled sends it as
  assembled; the format STL chosen while asking ends the question), the
  Windows
  workflow test's parts 10 and 15 (the format chosen with the wheel,
  `Ui-WheelLogged`: drop-down lists do not open in session 0), `app.unit`
  (`printBedAsks`, the choice's texts and `printBedLine`).

### Web page

**Export › Web page** (`.html`, mitcad#423; `web/WebPage`, `web/WebGlb`)
writes one self-contained page: the web viewer and the design, opened
from the disk in any browser without a server or the internet (browsers
block module scripts and `fetch()` from `file://`, so nothing is a file
of its own). A project's `web/` folder holds one such page per design
(`writePageOfFile`).

- **Dialog:** the type *Web page (\*.html)*, also by a `.html` or `.htm`
  name; the Refinement row as for STL and OBJ (Low, Medium, High: the
  deviation and angle of `io::MeshOptions::from`, a doubly curved face
  about 20 000 triangles at most); no Bodies or Coordinates row: the page
  shows every visible body where the design places it, as the view does.
- **Writing** (`exportFile`, on the UI thread under a wait cursor, the
  model idle): `web::writePage(model(), path, options)` with the title of
  the design's name and its folder for relative texture paths. The log
  says `Exported <path>: html, <n> body(ies), <bytes> bytes`, the status
  bar the size as Qt formats it; a failure is a sheet (`Could not
  export:`) and `Export of <path> failed: <reason>` in the log.
- **Bodies** (`instances`, the visible ones): each body meshed once on a
  copy of its shape (`io::triangulate`, then `render::meshOf`: smooth
  normals per face, faces in the shape's order), placed by each of its
  occurrences; faces with appearances of their own by their names
  (`face_appearances`, `find_faces`). A body that cannot be meshed is left
  out with a warning.
- **glb** (`pageGlb`): one node per placed body named `n1`, `n2`, … (its
  extras `body`, `body_name`, `occurrence`: not `name`, where a glTF loader keeps the node's name), with its placement as the node's
  matrix; bodies of the same mesh and look share a glTF mesh, a body's
  faces with their own look are primitives of their own. Materials are
  metallic-roughness ones from the appearances (`render::materialOf`:
  linear colours, metalness, roughness, emission; see-through as opacity
  × (1 − 0.9 × transmission), `BLEND`; double sided). Textures go into
  the glb (PNG or JPEG as they are, others and images over 2048 pixels as
  PNG), with texture coordinates by the projection the rendered view uses
  (planar along Z, else box by each triangle's facing, turned and
  scaled), the normal and occlusion maps as glTF's, the roughness and
  metalness maps combined into its one image; an image that cannot be
  read is left out with a warning. Positions are in the design's unit
  (`mm`, `cm`, `m`, `in`, `ft`; other units: mm).
- **Data** (`#mitcad-data`, format 1; `web/WebPage.hpp`): `title`,
  `units`, `unit_scale` (metres per unit), `up` (`[0, 0, 1]`), `glb`
  (base64), `tree` (one root item `{"id": "root", "name", "nodes",
  "children"}`, the occurrences of `components` below it by their uid
  paths, each with the nodes of its own bodies; occurrences without
  visible bodies left out), `views` (`named_views`: `name`, `eye`,
  `target`, `up`, `orthographic`, `height`), `home` (the named view Home,
  else from the front, right and above), `created`, `generator`.
- **Page** (`pageHtml`): the viewer's `template.html` with
  `<!--MITCAD:IMPORTMAP-->` replaced by an import map of `modules.txt`'s
  modules as `data:text/javascript;base64,…` URLs, `<!--MITCAD:VIEWER-->`
  by `viewer.js` as a module script, `<!--MITCAD:DATA-->` by the data
  (`<` written `<`), `{{TITLE}}` by the title, HTML-escaped. The
  viewer's files are resources under `:/web-viewer/<repository path>`
  (`app/web/viewer/*`, `third_party/three/**`, a glob in
  `app/CMakeLists.txt`); a missing one, or a template without a
  placeholder, fails the export with its name.
- **`writePageOfFile`:** the design file on disk (its newest version) read
  with `load_design`, its links followed, computed with the result store,
  as opening it computes it, without a window; any thread.
- Tests: `app.unit` (`tests/web_page.cpp`: `pageHtml` from fixture files,
  the glb's nodes against the tree), `tools/ui-import-test.sh` and the
  Windows workflow test's part 7 (the log line, the file's size, its
  `mitcad-data`).

### Drawing

**File › Export Drawing** (`file.export_drawing`, mitcad#471;
`files/MainWindowDrawing.cpp`, `files/DrawingDialog`,
`files/DrawingSettings`) writes the bodies as a drawing through the
model's `export_drawing` (core/model-api/src/commands.md, *Drawings*):
views with hidden lines removed, their sizes, hole diameters, a hole
table and a title block, as PDF, SVG, DXF or DWG (R2000, mitcad#728). An action of the File menu
after Export (keywords drawing, pdf, svg, dxf, dwg, print, blueprint), any
mode.

- **Dialog** (`askDrawing`, *Export Drawing*): File (Browse...: *Drawings
  (\*.pdf \*.svg \*.dxf \*.dwg)*), Format (PDF, SVG, DXF, DWG; the file's extension
  follows the format and the format the extension), Bodies (*All shown
  bodies*, or *Selected bodies (n)* first when the selection stands for
  bodies: bodies, and the bodies of faces, edges and vertices), Sheet (A4
  … A0, ANSI A (Letter) … ANSI E), Orientation, Scale (an editable
  drop-down: *Fit the Sheet*, 1:1, 1:2, 1:5, 1:10, 1:20, 1:50, 2:1, 5:1,
  10:1, or a typed ratio), Projection (Third Angle, First Angle), Views
  (Front, Top, Right, Isometric), Section view (None, or the document's
  Section Analyses by name), Lines (Hidden Edges, Tangent Edges),
  Annotations (Sizes and hole diameters, Hole table, Sketch dimensions),
  One sheet per body; Drawing, under Format (mitcad#725: the root's
  drawings by name, the one laid out last first as *Drawing1 (laid out
  last)*, the `drawings` query's highest `exported`, then *New Drawing*,
  which is first when no drawing was laid out; logged `Drawing dialog:
  drawing = D1` and at the end of `Drawing dialog opened: format pdf,
  sheet A4, scale fit, drawing D1` (`new`); its place `Drawing dialog
  drawing at`). With a drawing of the design chosen, the layout's inputs
  (Bodies to One sheet per body) are off and a line says that it is
  exported as it is and changed in its own workspace; OK checks no scale
  or views then.
  OK refuses an empty file, a scale that is neither *Fit the Sheet* nor
  a ratio, no view; an existing file asks to be replaced.
- **Settings:** group `drawing` (`format`, `sheet`, `scale`, `portrait`,
  `firstAngle`, `front`, `top`, `right`, `iso`, `hiddenLines`,
  `tangentEdges`, `dimensions`, `holeTable`, `sketchDimensions`,
  `perBody`): the dialog starts with them (an unknown value as the
  default: PDF, A4 landscape, fit, third angle, front, top and right,
  sizes and the hole table) and OK saves them. The Section view and the
  bodies are chosen each time. `DrawingSettings::command` makes the
  command: the title is the design file's name (the document's name when
  unsaved).
- **Kept** (mitcad#707): OK first lays the drawing out in the root's
  Drawings folder (`add_drawing` with the command's fields as its
  `layout`, an undo step `Add Drawing1`; logged `Drawing kept: D1
  Drawing1`; the layout's warnings logged as the export's), unless a
  drawing of the design is chosen under Drawing: that one is written as
  it is, with no change of the model (logged `Drawing exported as it is:
  D1`; mitcad#725), so repeated exports do not pile up drawings and lose
  nothing made by hand; then writes
  that drawing (`export_drawing` with `drawing` and the title), so the
  file is the one the dialog always made and the drawing can be opened,
  changed and written again (*Drawings*). A file that could not be
  written takes the drawing back to a checkpoint taken before it
  (`begin_command`, `restore_command`, then `discard_command`: no redo
  step is left), or by Undo when the model refused the checkpoint
  (`undo`, which leaves a redo step); logged `Drawing not kept: <path> was
  not written` only when the drawing went, else `Drawing D1 kept although
  <path> was not written: <why>`. A cancelled export shows no warning.
  The checkpoint's own commands run without a refresh of the scene
  (`runCommand`): they change nothing it shows. A design that cannot change now
  (read-only, a panel open) gets the file only: `export_drawing` with the
  dialog's fields lays the same drawing out without keeping it (logged
  `Drawing not kept: <why>`); a read-only window allows
  `file.export_drawing` and `export_drawing` for that
  (`MainWindowLocks.cpp`).
- **Writing** runs the command under a wait cursor (`Export drawing`). The
  log says `Exported <path>: drawing, 3 views, 2 bodies, A3, 1:1` (the
  views of a sheet, the bodies, the sheet and the first sheet's scale),
  each of the model's warnings as `Export drawing: <warning>`, the status
  bar the file with the views, bodies, sheet, scale and the sheets when
  more than one; a failure is a sheet (`Could not export:`) and `Export
  of <path> failed: <reason>` in the log.
- Tests: `app.unit` (`tests/drawing_settings.cpp`: the defaults, the
  settings read back, unknown values, typed scales, the command),
  `app.ui-windows-drawing` (`tools/ui-windows-drawing-test.ps1`: the
  dialog with its defaults, a typed SVG file, A3 by the sheet drop-down's
  wheel, OK at its logged place, the log line, the file's views, size,
  callout and table, the choices kept).

### File dialogs' folders

`framework/DialogFolders` (mitcad#498). The file dialogs come in groups,
and each group remembers the folder of the file last accepted in one of
its dialogs (not a cancelled one), across restarts (settings
`fileDialogs/<group>`, written at once). A dialog starts in its group's
folder while that folder exists, else where it started before the groups
(`MainWindow::fileDialogDirectory`: the design's folder, the project, the
Projects panel's folder, Documents; for the dialogs of other windows
their own earlier start):

| Group | Dialogs |
|---|---|
| `open` | Open, Open Read-Only, Save As (also the first save of an untitled design, and Another Folder... of the project picker) |
| `export` | Export (the dialog's suggested file), Export Drawing, Export Flat Pattern, Save As DXF, the Cut List's and Parts List's Save as PDF, Help › Diagnostics' Export Report, Render's Save Rendered Image: the last export folder, also while a design is open |
| `import` | Import, Insert SVG, Insert Mesh, Insert DXF, Insert Component (Another File... in a project) |
| `texture` | Edit Appearances' texture image |
| `program` | the 3D Print slicer program (the panel's and Preferences'), Preferences' git program |

Version History's Save Copy As starts in the design's own folder and
remembers nothing (`DialogGroup::SaveCopy`). Other file dialogs (a font
file, Render's environment image, folders, the `.f3d` import's
documents) keep their own start. `MainWindow::dialogFolder(group)` gives the start with the
window's fallback; `dialogStartFolder` and `rememberDialogFolder` the
rest. Tests: `app.unit` (`tests/dialog_folders.cpp`),
`tools/ui-dialog-folders-test.sh` and `app.ui-windows-dialog-folders`
(Import, Export and Open each in their own folder, also after a restart).

A path typed into Qt's own file dialog is what Enter or Open takes, also
when the dialog starts in a folder on that path (`prepareFileDialog` in
`framework/Dialogs.cpp` clears list selections other than the typed name;
`tools/ui-import-test.sh` opens a part again from its own folder).

### Unsaved changes

The title's `*` and the question on New Design, Open, Close and exit
follow the model's `modified` (`document` query), which compares the
revision with the one marked saved (`mark_saved` after Open and Save; a
new document starts saved, an imported `.f3d` design does not). Undo back
to the saved state clears it (`tools/ui-file-test.sh`).

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
- `<id>.<generation UUID>.mitcad`: an immutable design snapshot.
- `<id>.json`, atomically published last: `"format": "mitcad-autosave"`, `version` 2,
  `session`, `pid`, `application`, `application_version`, `document`,
  `path` and `base_digest` of the file it was opened from or saved to,
  `saved_at`, `revision`, `project` (the snapshot's name; the key keeps
  its old name), and the design file's `size` and `digest`. Digests are
  FNV-1a 64 in 16 hex digits.

The design file is made on the UI thread, never while a job has the
model (`Autosave skipped: busy`, retried in 5 s) or while a sketch
command's preview is applied; an edit that rolled the timeline back is
written with the marker it restores (`to_json_with_marker`). Both files
are written on a thread of their own with `QSaveFile`. The metadata points
to a complete generation; earlier bytes stay until its replacement is
published, so interrupted updates retain the last recoverable snapshot.
Successful publication cleans obsolete snapshots and interrupted writes.
Version 1 metadata with `<id>.mitcad` is still recovered and cleaned up.
Save, Save As, a
new document (New, Open, Close, an import) and undo back to the saved
state remove the session's files; a normal end removes them and the lock;
a crash leaves them. An imported `.f3d` design is written at once.

### Recovery

`files/Recovery`, `files/MainWindowAutosave.cpp`. When the window shows
(`main.cpp`; not with `--no-recovery` or `--screenshot`; after an `.f3d`
given with `--open` has come in) and by File › Recover Designs
(`file.recover`), sessions whose lock `QLockFile::tryLock(0)` takes are
found and held until the user decides (a running instance's sessions are
never offered). A lock without files is removed. A session whose metadata
is missing, unreadable or of another version, or whose design file is
missing or does not match `size` and `digest`, is damaged and can only be
discarded.

Restore reads the design file, follows links relative to it and
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
100 ms (`MITCAD_RESULT_STORE_MIN_MS`), and the quicker ones whose shapes
are small for their time (mitcad#882, commands.md "Result store"), in
the same job; Save writes them as
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
- Author (`versionAuthor`): the project's (`identity`: the repository's
  own `user.name` and `user.email`, which New Project and Project Settings
  write, else git's configured user), else Preferences' default author
  (`versions/name`, `versions/email`); asked (`askVersionAuthor`) only when
  none has one, and kept as the default.
- Before Save overwrites its file, `askVersionConflict` compares the
  `status` command's blob ids with those of the open or last save
  (`m_versionBase`, by path). After anything that writes the file
  (restore), call `rememberVersionBase` (or reopen) and
  `updateVersionStatus`.
- Opening runs `follow_rename` first: a file renamed or moved outside
  MitCad within its project takes its display state along, and Save
  records the rename first.
- Projects are made in [Projects](#projects) below; an untitled design of
  a project remembers its folder for Save As (`m_projectDir`, else the
  Projects panel's selection, else the current project's root). In a
  project with versions Save As chooses the project, folder and name in
  the [project picker](#project-picker) (`saveAsProjectPlace`; Another
  Folder... the file dialog; mitcad#176); a design in no such project keeps
  the file dialog.
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
`loadDesign`. Save Copy As: the place chosen in the project picker
(the version's project and folder; Another Folder... the file dialog;
mitcad#176), then `load_version` and `save_design` `auto`, plus a version
in a versioned project. Previews: after `writeFile`,
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

`files/RemoteController.cpp`: File › Project › Sync, Check for Newer
Versions, Open in Browser, the automatic push and the remote's state in
the project indicator, for the window's current project
(`Host::projectRoot`). The model's side: commands.md, "Remote
repositories".

| Model command / bridge function | Used by |
|---|---|
| `push` | after each version Save records (Project Settings, else Preferences; on by default); a network failure, `not_found` or `other` retries after 60 s, doubling up to 15 minutes |
| `fetch` | on opening a design in a Cloud project, every 10 minutes (Project Settings, else Preferences; 0: never), at the first change after opening when the last fetch is over 2 minutes old, after a refused push, and Check for Newer Versions; then `push` if versions wait and the remote has none newer |
| `incoming` | the status bar tooltip: who saved the remote's newest version, and when |
| `incoming` with `path` | after a fetch and after Save: the notice of a newer version of the open file (`file_versions`, `differs`, `changed_here`) |
| `sync` (`author`; after a conflict `resolutions` and `fetch` false) | Sync and the notice's Sync Now; `changed_paths` reopens the design |
| `diff` (`from`: the conflict's `theirs.version.id`, `to`: `mine.version.id`) | Compare in Resolve Sync Conflicts |
| `commit` (`Save <paths> before sync`) | a sync refused for `local_changes`: records the files listed in `uncommitted` first |
| `remote_info` | the indicator's state, Project Settings' Status row |
| `git_info_at(program)` | Preferences › Cloud: the chosen git program (a path, or "" for the one found) and its version; the app sets `MITCAD_GIT` from it |

Tests: `tools/ui-sync-test.sh` and the Windows workflow test.

### Projects

Local and Cloud projects (mitcad#89; `files/MainWindowProjects.cpp`,
`files/ProjectDialogs`, `files/ProjectSettings`, `files/CloudSection`,
`files/ProjectIndicator`; commands.md, "Projects"). The window keeps the
current project (`ProjectState`: kind `None`, `Local`, `Cloud` or
`NoHistory`, root, remote) as state of its own; the indicator, the
remote and Project Settings use its root. The commands below, with Sync,
Check for Newer Versions and Open in Browser, make the File menu's
Project submenu (mitcad#175); Open Read-Only and Open from Project
(mitcad#530) are in the File menu itself.

| Command | What |
|---|---|
| `file.new_project` New Project... | the New Project dialog: folder, Local or Cloud, author, the Cloud section; Create (an empty project, or one of the designs its folder has, shown in the Projects panel), Open It, Open It Instead, Create as Local for Now |
| `file.open_project` Open Project... | a project chosen in the [project picker](#project-picker) (the Projects panel's projects, also those without versions; mitcad#176), or a folder (Another Folder...: Qt's folder dialog) recognised with `inspect_folder`: a project shows in the Projects panel and opens its design opened last (`last_design`), else the newest, none when it has none; an older one gets its history (`init_project_history`), anything else goes to New Project with the folder fixed |
| `file.open_read_only` Open Read-Only... | a design opened without an edit lock (`m_openingReadOnly` for the lock controller) |
| `file.open_from_project` Open from Project... | File menu, right after Open, before Open Recent (mitcad#530; also in the command search and in a read-only window): a design chosen in the [project browser](#project-browser), opened as Open Recent opens one (unsaved changes asked then; the open design itself only told it is open), or with Read-only checked as Open Read-Only does; Another File... is Open's (or Open Read-Only's) system dialog |
| `file.open_remote` Open from Cloud... | the Open from Cloud dialog: clone, Create Project Here, Make It a Project, Open It |
| `file.project_settings` Project Settings... | the current project's settings (a loose design: Save to Project offered); a **Display name** first, the name the Projects panel shows on this computer (`set_project_settings`' `local.name`; empty: the default; never renames the folder or the repository) |
| `file.save_to_project` Save to Project... | File menu, right after Save As (mitcad#503; File › Project › Move to a Project before, whose id `file.move_to_project` stays an alias, `CommandRegistry::alias`: a shortcut saved under it moves to the new id, scripts and `trigger` accept it): the design saved as Save As saves it (the window then shows that file, the file it came from stays, an untitled design's first save the same) in a project chosen in the [project picker](#project-picker) ("Save to Project": the design's own project and folder, else the Projects panel's selection; a project with a folder in it and the design's name, a clash shown there; its own file no clash unless the window is read-only); **New Project...** in the picker makes a project first (the New Project dialog, the design's own folder by default when it is no project's or repository's, else a folder named after it; the design recorded in it, or saved under its name there), **Another Folder...** asks a project not listed with the folder dialog (it must keep versions; the project is then a known one; the design is saved in its top under its name, the picker asked again for an untitled design's name or a name clash); the saved design shown selected in the Projects panel (mitcad#174); also in the indicator's menu of a loose design and in Save with Description's, Version History's and Project Settings' questions; allowed in a read-only window (a copy, as Save As) |
| `file.web_viewer` Web Viewer..., `file.update_web_viewer` Update Web Viewer | the current project's web viewer (mitcad#423, [Project web viewer](#project-web-viewer)): every design's page and the index written into `web/` and recorded as a version; Web Viewer turns it on, Update writes it again; only in a project with versions |

| Model command | Used by |
|---|---|
| `inspect_folder` | the current project of a design opened or saved (`projectOfFile`); Open Project; the folder checks of New Project and Open from Cloud (a thread of their own, after a pause while typing) |
| `check_remote` | the Cloud section of New Project and Open from Cloud (a thread of its own, after a pause, and when the dialog gets the focus back) |
| `remote_check` (a project's) | the Cloud section of Project Settings' Share and of Change Address (`related`) |
| `create_project` (no `design`: the project starts empty, mitcad#145; `url` for Cloud; `shared` with New Project's live updates) | New Project's Create, Create as Local for Now, Open from Cloud's Create Project Here; with `init_bare` first for a new shared folder |
| `clone_project` (`adopt` for Make It a Project) | Open from Cloud |
| `connect` with `onto_files` | Project Settings' Share (conflicts in Resolve Sync Conflicts) |
| `remote_set`, `remote_remove` | Change Address, Stop Syncing |
| `project_settings`, `set_project_settings`, `set_identity`, `identity` | Project Settings (a change for everyone is a version, sent as a saved one is); the sending and checking of the remote |
| `remember_design` | each design opened in a project (Open Project opens it next time) |
| `project_settings`' `authors` | Share lists the authors of the versions to publish |
| `git_info` (`credential_helper`, `author`) | HTTPS or SSH by default; the author New Project offers; Cloud off without git |
| `ssh_public_key`, `host_keys`, `trust_host_key` | Copy Public Key; Trust This Server |

Tests: `tools/ui-project-test.sh`, `tools/ui-cloud-test.sh`,
`tools/ui-cloud-address-test.sh` (a pasted git address), `tools/ui-sync-test.sh`,
`tools/ui-version-test.sh`, the Windows and macOS workflow tests.

### Projects panel

The projects, their folders and designs in one panel (mitcad#145;
`projects/ProjectsPanel`, `projects/ProjectsTree`,
`projects/MainWindowProjectsPanel.cpp`): a dock above the browser's in the
Docked layout, a card above the browser's in the Floating one (none in the
AppKit frontend). The tree is project → folders → designs (mitcad#173):
a project row is the repository's root, nothing above it, below it only
the folders and designs it holds; it has Local or Cloud (its icon and
tooltip too) and its sync state as the indicator shows it, a design row
its version, Changed and its edit lock, the open design in bold. The
projects are the known ones (`ProjectsSettings`: made, opened or cloned
here, newest first; `projects/known`) and those of the projects
folder (`projectsDirectory()`) that Remove from List did not take out
(`projects/hidden`). Whether the panel shows is `projects/panel`, the
user's last show or hide (View › Projects, the dock's close button, New
Project and Open Project showing a project); before the first one it is
never written, and the panel starts shown when a project is configured (a
known project whose folder is there, or one of the projects folder's that
was not removed from the list; `anyProjectConfigured`, folder entries
only), hidden otherwise (mitcad#151), in both layouts. The
trees are read on a thread (`project_tree` per project, a `RemoteTask`),
again after a save, a sync, versions sent, the window's focus back and
each change made in the panel; while a name is typed or a row dragged the
rows wait.

A project row shows its name (mitcad#152, #173), and nothing else: the
display name set in Project Settings (`project_tree`'s `display_name`,
`local.name` of the project's settings, kept in `.mitcad/local/name.json`
on this computer), else a Cloud project's repository address without the
scheme (`repositoryAddressOf`: `github.com/owner/repository`, GitLab with
its subgroups, Forgejo, another server's host and path without `.git`, a
shared folder's path), else the folder's name. The projects are sorted
by it, case-insensitive. The rows' keys (the logs, the expanded rows, the
selection) start with the folder's name (`Shop`, `Shop/parts/`; the
whole path when two listed projects' folders have the same name), which a
display name does not change.

The rows give the names the width (mitcad#529; `projects/ProjectRowText`):
the state column holds compact marks, a short text and at most one icon,
with the state in full words in the tooltip (`RowMarks`). A design shows
its version (`v2`; `v2*` changed since then; `new` with none) and its edit
lock as an icon (`lock`: Editing elsewhere or Locked: Alex; `edit`:
Editing here). A project shows its sync state as an icon (`cloud-synced`
in sync; `cloud` with `↑2 ↓1` to send and newer, or not synced yet and `?`
for no remote chosen; `cloud-offline` with `↑2` waiting; `warning` for a
conflict, sign-in needed, missing or unreadable; `–` without versions); a
Local project none, its row's icon tells. A name that does not fit is
elided in the middle, its start and end kept (`ProjectsRowDelegate` paints
it, `elideMiddle`; a rename's editor gets the whole name); every row's
tooltip starts with the full name, then the path (a design's and a
folder's) or the project's state, kind, folder and remote. Very deep trees
get neither a horizontal scroll bar nor a second line: the levels indent
little (mitcad#393), the names elide in the middle, and the panel is
widened instead. Its width is dragged by its edge and remembered: in the
Floating layout the card's grip along its right edge
(`GlassCard::setWidthAdjustable`, `FloatingLayout::setCardWidth`: at
least 180 px, at most two thirds of the view; `projects/cardWidth`), in
the Docked one the dock's own edge (a resize while the left button is
held, `ProjectsPanel::dockWidthChosen`, restored with `resizeDocks` at the
start and when the panel is shown; `projects/dockWidth`). The AppKit
frontend's Projects card shares `projects/cardWidth` (mitcad#818,
[docs/native-macos-ui.md](../docs/native-macos-ui.md)).

| Command | What |
|---|---|
| `view.projects` Projects | shows or hides the panel (View menu, checkable; the dock's close button too) |
| `file.new_design` New Design... (Ctrl+N) | File menu and the indicator's menu: a design named in place in the panel's selected folder of the current project (its top when another project or none is selected), recorded (`new_design`) and opened (the open design's versions sent and its edit lock released first: a refused release makes nothing); without a current project (also one without versions) an untitled design, as `newDocument` (mitcad#175: New and New Design are one command) |

In the panel (toolbar, context menu, keys): **Open** (double-click, Enter;
unsaved changes asked as with Open), **New Design** and **New Folder**
(named in place), **Rename** (F2 and the context menu of a design or a
folder; no toolbar button), **Delete** (Del; confirmed, with the
designs it holds, changes no version holds and whether it is open), a
**drag** onto a folder, a project or a design (its folder) of the same
project (`move`), **Version History...** (the design opened first),
**Show in Folder**, **Project Settings...** (mitcad#152, #153: the
toolbar's button for the selected project, folder or design's project;
a double-click on a Cloud project; the current project's as
File › Project Settings, another's without opening it, through a
`ProjectSettingsHost` that refreshes the panel, sends a recorded version
in the background and has no lock probe), **Sync Now** (another
project opened first, as Open Project), **Remove from List** (nothing is
deleted), **New Project...**, **Open from Cloud...**, **Add Existing
Project Folder...** (a folder that is no project: Make It a Project). An
open design that is moved or renamed follows its new path (its version
base and edit lock: `openDesignMoved`); a deleted open design closes. Save
of an untitled design starts in the panel's selected folder (the project
picker's preselection, `saveAsProjectPlace`; after Another Folder... the
file dialog starts in the Open group's [folder](#file-dialogs-folders),
else in `fileDialogDirectory`).

| Model command | Used by |
|---|---|
| `project_tree` (with this run's `session`) | the panel's rows, on a thread of their own |
| `new_design`, `create_folder`, `move`, `delete` (`author`, `session`, `push: false`) | New Design, New Folder, Rename and drags, Delete (`force` only after a second question when the core answers `local_changes`); a refusal (`exists`, `locked`, `invalid_path`, `not_found`, `unsupported`) is shown and changes nothing. Each is a version, sent as Save's are: the current project's by `RemoteController::versionRecorded`, another Cloud project's by a `push` task |

Tests: `tools/ui-projects-test.sh`, `tools/ui-projects-settings-test.sh`
(the default shown or hidden, the remembered choice, the flat tree, the
repository's address, the double-click, the display name, the settings
button; mitcad#151–#153, #173),
`tools/ui-projects-rows-test.sh` and the Windows
`app.ui-windows-projects-rows` (a 55-character name elided in the middle
with both ends, its lock as a mark, the full name in the tooltip, the card
and the dock widened and their widths after a restart; mitcad#529), the
`app.unit` tests of the marks and the elision (`tests/project_rows.cpp`)
and the project tests above.

### Project picker

`projects/ProjectPicker` (mitcad#174): instead of a file or folder dialog,
a place, a design or a project chosen among the projects the Projects
panel lists (`readProjects` on a thread of its own: the known ones and
those of the projects folder), each by the name the panel shows
(`ProjectRow::name`), with its Local or Cloud icon and its kind, remote
address and folder in the tooltip. `pickInProjects(parent, options)`:

- **Place** (`Mode::Place`): a project with versions and one of its real
  folders (`project_tree`'s `folders`), the preselected project first and
  expanded (`root`, `folder`), and the design's name typed in the dialog
  (`.mitcad` added; `ownFile` is no clash); a clash with a file there, or
  a name no file can have, is shown under the name and the accept button
  waits. Save to Project ("Save to Project", mitcad#503: the design's
  project and folder, else the Projects panel's selection; `newProjectText`
  adds a New Project... button, `newProject` in the result, for a project
  that does not exist yet), and its second question, the name of an
  untitled design or a clash of the name in a project's top ("Save in a
  Project").
  Save As in a project ("Save in a Project": the design's project and
  folder, else the panel's selection; its own file no clash unless the
  window is read-only) and Version History's Save Copy As ("Save a Copy in
  a Project"), mitcad#176.
- **Design** (`Mode::Design`): a design of a project with versions, the
  given project first and expanded (`ownFile` not offered): Insert
  Component in a project ("Insert from a Project"; the linked or copy
  question as before), mitcad#176.
- **Project** (`Mode::Project`): a project, also one without versions:
  Open Project ("Open a Project"), mitcad#176.

The secondary button (`otherText`: Another Folder..., Another File...)
returns `other`: the caller asks with its file or folder dialog, whose
title differs from the picker's. Missing and unreadable projects are never
offered. `ProjectsPanel::revealDesign` selects a design once a read lists
it (`MainWindow::showDesignInPanel`).

Tests: `tools/ui-project-test.sh` (Save to Project, Open Project, Save As
with Another Folder..., Insert Component), `tools/ui-version-test.sh` (an
untitled design saved into a new project, Save Copy As, Save As of an
opened version, Open Project's Another Folder...),
`tools/ui-projects-test.sh` (Save in the selected folder, Open Project),
`tools/ui-locks-test.sh` (Save As of a read-only window).

Insert Component's "Insert from a Project" stays the picker's Design mode
(its tree, its tests and its Another File...); a design to open is chosen
in the project browser below.

### Project browser

`projects/ProjectBrowser` (mitcad#530): File › Open from Project's window,
like a file dialog on top of the project system. It is built on the
picker's data (`readProjects` with this run's edit-lock session on a
thread, `project_tree`; the listing and the search are
`projects/ProjectBrowserData`, plain Qt Core and unit-tested), never on
the file system's raw folders: `.mitcad/` internals and files that are no
designs are not in `project_tree`'s answer and are left out, not greyed
(Another File... reaches any file). `browseProjects(parent, options)`:

- **Left**, the projects with versions (Local and Cloud, by the Projects
  panel's names, with its icons and the kind, remote and folder in the
  tooltip) and all their folders (those `project_tree` lists and those a
  design's path names), unfolded. A click shows a place.
- **Right**, the place's contents: its sub-folders first, then its
  designs, each by name (case ignored), with the version (`v2`, `v2*`
  changed, `new`), the edit lock (its icon and Editing, Editing elsewhere
  or Locked: Alex), the last change (the latest version's time) and who
  made it. A double-click or Enter goes into a folder or opens a design;
  Backspace (Alt+Up) goes up. Open does the same for the selected row.
- **The path bar**: the project's name, then each folder down to the
  place, each a button that shows that place.
- **Search** (Alt+S): as the text is typed, the designs whose name (the
  file's without `.mitcad`) holds it, case ignored, in all projects or in
  the place's project (**In**, Alt+I), with their project and folder in a
  column of their own; a click in the folders' tree or the path bar ends
  the search. Enter in the field opens the first result.
- **Read-only** (Alt+R): the chosen design opens as Open Read-Only does.
  **Cancel**; **Another File...** gives the choice to the system's
  dialog.
- It starts in the place shown last (`projects/browseRoot`,
  `projects/browseFolder`: the chosen design's folder, or the place shown
  when it was cancelled), else in the Projects panel's selection or the
  current project, else in the first project.

Tests: `tools/ui-open-from-project-test.sh` and the Windows
`app.ui-windows-open-from-project` (two projects, a folder with a
sub-folder, the search across projects, Open, Open Read-Only, the place
remembered after a restart), `app.unit` (`tests/project_browser.cpp`: the
listing and the search over a fixture project tree).

### Edit locks

`files/LockController.cpp`, `files/MainWindowLocks.cpp` (mitcad#89; the
user's side in [docs/user-guide.md](../docs/user-guide.md#edit-locks),
the core's in commands.md, "Edit locks"): the open design's edit lock in a
Cloud project with edit locks on, and the window's read-only mode. The
lock commands run as `RemoteTask`s, one at a time, with a project of
their own; `MITCAD_LOCK_TIME_SCALE` speeds up their timers as it does the
core's clock (UI tests). What others should learn at once (locks,
requests, receipts, answers, withdrawals, who has the design open) goes to
the live controller through `LiveLink`, and its events come back through
`LockEvents` (`files/MainWindowLive.cpp` joins them): a lock, request or
version event polls at once, open events list who else has the design
open, a holder's offline will asks whether to take the lock.

| Model command | Used by |
|---|---|
| `lock_take` (`poll_seconds`, `mqtt`; `take_over` with the lock's commit), then `lock_status` (`network` false: `newer`) | a design of a Cloud project opened (not by Open Read-Only), Edit in the banner, Take Over, a request whose holder did not answer, the remote out of reach (tried again every poll) |
| `lock_poll` (`mqtt`) | every `poll_seconds` (2 minutes while live updates are connected) while the design has a lock or is read-only; at once on a live lock, request or version event |
| `lock_refresh` (`active_at`, `state`, `idle_minutes`, `poll_seconds`, `mqtt`) | every half idle time, when the lock turns idle or active, after Save |
| `lock_request` (`message`), `lock_answer` (`declined`, `keep` for 15 minutes) | Request Edit Access; the holder's Decline and Keep 15 More Minutes |
| `lock_hand_over` (`to`) | the holder's Release, a request while the lock is idle, a request not answered within the idle time |
| `lock_release` (`all`) | closing the design or quitting, the idle time, Release Edit Lock, edit locks turned off, Stop Syncing |
| `lock_probe` | Project Settings of a Cloud project, once per remote: a remote that refuses lock refs disables the Edit locks row |
| `sync_plan` (`fetch` false, `locks` `last`, `session`) | before Sync sends versions: files someone else holds (Send Anyway) |
| `sync` (`RemoteController::syncNow`) | the versions sent before a release or a hand-over |
| `load_version` (bridge) | a read-only window shows the editor's version after a fetch (`incoming`) |

Read-only windows: `MainWindow::isAvailable` disables all but the view's
commands, inspections and `file.export`, `file.version_history`,
`file.sync`, the project and opening commands, `make.print3d`, the
libraries' browsing; `MainWindow::command` refuses every model command
but `set_isolation`, `set_origin_visible`, the analyses' commands,
`export`, `export_sketch` and the reading ones (`ReadOnlyError`); Save
offers Save as Copy and Save as New Version for unsaved changes kept from
editing; sketch mode, command panels and Restore are refused.

A project whose repository's own git configuration names programs
(mitcad#920; the error class `untrusted_config`, no git run): `lock_take`'s
refusal leaves the design editable without a lock (as a remote that
refuses lock refs does) with the message in the status bar;
`RemoteController` takes the class from `remote_info`'s `untrusted_config`
over the last attempt's, and the project indicator shows `Not synced:
untrusted git settings` with the message as its tooltip.

Tests: `tools/ui-locks-test.sh` (two and three instances; its remote
that refuses lock refs hides them by its configuration, and a hook in
that folder's repository never runs, mitcad#940), the Windows
workflow test (Take Over after a killed session),
`tools/ui-untrusted-git-config-test.sh` (a received project with
`remote.origin.uploadpack` set: nothing runs, the indicator says why).

### Live updates

Live updates through an MQTT broker (mitcad#89; `files/LiveController`,
`files/LiveDialogs`, `files/LiveBrokers`, `files/Keychain`,
`files/MainWindowLive.cpp`; commands.md, "Live updates"). One `LiveHub`
for the application: its commands on the UI thread, its events read on a
thread of the controller's own and handled on the UI thread. The window's
current Cloud project subscribes when it opens with live updates set
(Project Settings, for everyone or on this computer) and Preferences ›
Cloud allows them, and unsubscribes when another project opens or it stops
syncing; quitting closes the hub with `wait_ms` (a clean leave).

| Hub command / model command | Used by |
|---|---|
| `project_settings` (`live_updates`), `project_id`, `identity` | the broker and prefix this computer uses, the project's id on the broker (its first commit), the name others see |
| `subscribe` (`broker`, `prefix`, `session`: autosave's, `user` and `password` of the keychain over TLS only) | a project opens (after the trust question and the keychain), its live settings change, a sign-in; a new connection for another user unsubscribes the old one |
| `unsubscribe` | another project opens, the live settings change, Stop Syncing, Preferences' Forget and Sign Out, live updates turned off |
| `publish` | the lock controller's `LiveLink` calls; `version` after a push or a sync that pushed (`RemoteController::versionsSent`); `open` and `closed` for the open design while no lock controller does it |
| `connect` with a password over `mqtt://` | the sign-in dialog: the core's refusal (`insecure`) shown in it |
| `test` (on a thread of its own) | Project Settings' Test: its steps in a dialog |
| `close` (`wait_ms` 2000) | quitting |
| events: `state`, `subscribed` | the indicator's `Live` / `Live offline (polling)`; `auth_failed` shows the sign-in notice; a subscription after a lost connection checks the remote |
| events: `version` (not this session's) | the remote checked at once (`RemoteController::versionAnnounced`): the indicator's newer versions and the notice of a newer version of the open file |
| events: all of a project's | `LockEvents::liveEvent` of the lock controller, and `LockEvents::liveStateChanged` when the connection comes or goes |

While the project's connection is up the remote is not checked on the
timer (`RemoteController::setLive`). The trust answers are the settings
`live/brokers` (address, trusted); credentials are QtKeychain entries in
the service `Mitcad live updates`, keyed by host and port, holding the user
name and the password; without a keychain they are kept for the session.

Tests: `tools/ui-live-test.sh` (against a mosquitto it starts; skipped
without one).

### Feedback and error reports

`report/` (mitcad#61, mitcad#62; the user's side in
[docs/user-guide.md](../docs/user-guide.md#feedback-and-error-reports),
the design in [docs/architecture.md](../docs/architecture.md)).

- **Help › Send Feedback** (`help.send_feedback`, an action in the Help
  menu and the command search): `FeedbackDialog` with the window grabbed
  before it shows; Preview builds a `report::Report` (sections
  description, contact, the recent actions for a bug, diagnostics) and
  shows `ReportPreviewDialog` over the form, which closes once Send is
  done.
- **Every report** goes through `ReportCenter::previewAndSend`: the title
  and the sections marked `masked` are masked (`report::mask`; the contact
  address is not), the preview edits them, and `deliver` builds the
  Markdown (`report::reportBody`) and the address
  (`report::issueLink`, at most 8000 characters: longer, the body is cut
  at a line with a note and the whole goes to the clipboard), saves a
  screenshot under `reports` in the local app data and opens the address
  (`openExternalUrl`). Nothing goes over the network from MitCad.
- **The tracker:** `MITCAD_ISSUE_URL` (CMake cache variable, default
  `https://github.com/mitcad/Mitcad/issues` from README.md), overridden
  by the setting `reports/issueUrl`: an issue list's address (`/new` and
  `title`, `body` added) or a template with `{title}` and `{body}`.
- **Crashes:** `crash::install` at the start of `main` (the application
  and the import worker; `mitcad-render`'s `main` too) and
  `ReportCenter::prepareCrashReports` once the settings are known: the
  folder (`crashes` in the local app data, `MITCAD_CRASH_DIR`), the
  workers' environment (`MITCAD_CRASH_DIR`, `MITCAD_CRASH_PARENT`) and the
  Rust panic hook (`mitcad_set_panic_sink`, `core/ffi/src/panic_note.rs`).
  `ReportCenter::startUp` watches the folder: a report of this process's
  worker is offered at once, the others' at the next start (the newest,
  counting the rest); offered ones become `*.crash.offered` (20 kept).
- **Internal errors:** `MainWindow::showError` passes every message to
  `ReportCenter::errorShown`, which offers the kernel's converted crashes
  (`report::isKernelCrashMessage`); `runCommand` offers an exception that
  is not the model's (`internalError`). Once per message and run.
- **Recent actions:** `MainWindow::trigger` (`command <id>`) and
  `runCommand` (`model <cmd>`), the last 32, also in the crash handler's
  buffer; never arguments or names from the design.
- Offers wait for a free window (`offerNext`: no modal dialog or popup,
  no job, no import); `reports/offerErrors` false turns them off.

Test switches: `MITCAD_TEST_LOG_URLS=1` (set by `tools/ui-test-lib.sh`)
logs an address instead of opening it; `MITCAD_TEST_CRASH=app`,
`model-worker` (once the window shows), `import-worker`, `render-worker`
(at the worker's start) crash there; `MITCAD_TEST_CRASH=app-exception`
throws a `std::range_error` out of a slot once the window shows, which
nothing catches: the report's `message` names the exception's type and
what it says (`uncaught exception std::range_error: …`; on Windows the
signal `CXX_EXCEPTION` and `unhandled C++ exception std::range_error: …`;
mitcad#773); `MITCAD_TEST_OCCT_CRASH=<operation>`
makes a kernel crash for an internal error. `tools/ui-report-test.sh`.

### Component libraries

`files/MainWindowLibraries.cpp`, `files/LibraryDialogs.cpp`,
`files/Libraries.cpp` (mitcad#64, mitcad#63; the formats in
[docs/libraries.md](../docs/libraries.md), the model's side in
commands.md, "Component libraries"). The commands are in Tools ›
Libraries and the command search; Insert from Library is also in the
SOLID tab's ASSEMBLE group.

| Command | What it does | Model command / bridge |
|---|---|---|
| `insert.library_component` Insert from Library | `LibraryBrowser`: search (words, a licence filter, items without a licence only when asked), the results as the part picker's grid (`PartGrid`, [Part picker](#part-picker): groups with Favourites and In this design, the libraries' groups as a tree (mitcad#527), stars, the tiles' size, the hover's larger picture; an index's libraries not fetched in "Libraries to get"; a double click or Return inserts), details with the licence and the attribution, the version (newest first) and the size (a closed table's cascaded combo boxes per option, `SizeChooser`; an open table's chooser per option filtered by the rules, mitcad#810, [Options of library parts](#options-of-library-parts)), linked or a copy; **Update** fetches every enabled source (as Fetch All, one warning for the failures) and searches again, keeping the selected item; while no enabled source is fetched the empty list says to press Update (mitcad#317); a part whose manifest lists mount frames goes on to Place Component (below, mitcad#525), any other is dropped in at the origin as before | `library_search`, `library_show`, `library_preview`, `library_list`, `library_fetch`, `library_configure`; then `insert_component` with `library` (and `parameters`) |
| `insert.mount_part` Place Component (mitcad#525, `commands/MountCommands.cpp`) | Insert from Library starts it for a part whose manifest lists mount frames (`mounts`, [docs/libraries.md](../docs/libraries.md#mount-frames)); not in the ribbon. Place On (a flat face or a plane for a face or grid frame; for a foot frame any face of the part with the grid, the cell under the click, so that the planes of the frames of the parts already placed, which lie in the cells' faces, take no clicks; a round face, a circular edge or an axis for a shaft or hole frame; none: a drop-in), Mount Frame (the part's frames, the first chosen), Rotate (about the frame's z; on a grid quarter turns), Offset, Flip (a plane's other side, a hole's other end). A click snaps the frame to the pick face to face, its way towards the viewer kept with it (`towardViewer`: a hole's end); the preview shows the part there (`analysis_shape` `mount_preview`) with a marker on its frame, the panel the frame, where it goes (a grid's cells) and how it may move, or why it cannot (a taken cell). **Place** (Enter) places it and the panel stays for the next one; **Shift + click** places at once; Enter without a pick drops it in at the origin as before, to be joined later with Joint, whose origins take its frames (its component's joint origins, the browser's Joints rows); **Close** (Esc) keeps what was placed | `mount_preview` while the inputs change; `insert_component` with `mount` (without: as before) |
| `tools.library_parts` Library Parts... | `LibraryPartsDialog`: each part's recorded version and size (with the Standard and Supplier filters, mitcad#949), the row's supplier part numbers, another version or size, Show Changes, Update; Check for Newer Versions and Get Missing Libraries fetch (the latter after showing the addresses, which come from the design); the Parts List tab (Copy as CSV; the column Supplier part numbers, `parts_list`'s `supplier_parts_text`) | `library_parts`, `parts_list`, `library_show`, `library_diff`, `library_fetch`; then `update_library_parts` |
| `tools.libraries` Libraries... | `LibrariesDialog`: the sources (settings `libraries/sources`: URL or folder, on or off, and Auto-update, mitcad#576; the defaults are MitCad's fastener library and the community index, both with Auto-update off), the columns On, Auto-update (tooltip "Fetch this library when MitCad starts"; greyed out, its state kept, while the source is off), Source, Library, Versions, Licence; Add URL, Add Folder, Remove, Fetch Selected, Fetch All | `library_list`, `library_fetch` |
| `tools.community_library` Community Library... | the browser with the indexes' libraries first; Get Library adds a library an index lists to the sources and fetches it after a question with its address and licence | as Insert from Library |
| `tools.publish_library` Publish to Library... | `PublishLibraryDialog`: the open design (a single file) added to a library folder of one's own (made when new), an image of the view as its preview, recorded as a version of the folder's project, pushed (`connect` with a remote URL, else `push`), and the index entry (copied) | `library_init`, `library_add`, the project's `commit`, `connect`/`push`, `index_entry` |

- **An index's library a source has** (mitcad#949): the browser's
  `library_search` sends the enabled sources (`sources`); a library an
  index offers, or a fetched clone of one, whose repository an enabled
  source has under another id, or whose id a fetched library replaces, is
  not listed (`left_out`; `browserLibraries` in
  `files/LibrarySettings.cpp` also leaves out an entry whose URL is an
  enabled source's while its id is not fetched), logged once in a
  session: `Library index: mitcad-fasteners left out: same repository as
  https://github.com/mitcad/parts.git`. Test: `tests/library_settings.cpp`
  (test_app).
- Where fetched libraries are kept: `MITCAD_LIBRARIES_DIR`, else
  `libraries` in the local app data (`Mitcad/Mitcad` in the platform's
  data folder). `librariesDirectory()` asks the bridge for its shared
  default; `configureLibraries()` uses the same default configuration as
  the CLI (`configure_libraries`), including read access to earlier CLI
  caches. Explicit overrides are isolated.
- Fetches run as `RemoteTask::library` on a thread of their own with a
  progress dialog whose Cancel ends git; when the user asks, and at
  start-up for the sources with Auto-update (below). The other library
  commands are local and run at once.
- **Auto-update at start-up** (mitcad#576, `files/LibraryAutoUpdate.*`,
  `files/LibrarySettings.cpp`): `LibrarySource::autoUpdate` (default off;
  settings without the key read as off) is saved with `url` and
  `enabled`. About one second after the main window shows,
  `MainWindow::updateLibrariesAtStart` starts `LibraryAutoUpdate`: it
  fetches the sources that are on and have Auto-update, one after
  another, each as a `RemoteTask::library` without a progress dialog.
  Nothing blocks the window. A failure (offline, server down) is only
  logged with `qWarning` and counted. When the run ends, the status bar
  says `Libraries updated: mitcad-parts v2.7.0` for the sources with a
  version that was not there before (a source fetched the first time
  counts), and `... could not be fetched` for the failures; it says nothing when
  nothing is new. The signal `updated()` makes the open library browser
  (search again, selection kept), the Libraries dialog (table again) and
  a BUILD part picker that is open (`PickerRequest::role`, the catalog
  read again, `PartPicker::refreshParts`) list the new versions. Nothing
  in a design changes: its parts keep their chosen version. A fetch the
  user asks for (`fetchLibraries`) ends a run that is still going on
  first. There is no start-up fetch in the CLI, the MCP server, a run of
  a UI test script (`MITCAD_TEST_INPUT`) or a screenshot run.

Logs: `Library fetched: <kind> <name> (<id>) from <url>; versions ...`,
`Library fetch failed: <url>: ...`, `Library fetch of <n> source(s): <m>
failed` (Fetch All, Update), `Library auto-update: fetching <n>
source(s) in the background` (only with a source on Auto-update),
`Library auto-update: <n> source(s) fetched, <m> failed; new: ...`,
`Library auto-update: Libraries updated: ...`, `Library auto-update ended
before it was done`, `Library source <url>: auto-update on|off` (the
check box), `Library browser: libraries updated in the background`,
`Part picker refreshed: <n> parts for <command>`, `Library browser update: <n> enabled
source(s)`, `Library browser empty: nothing fetched yet, press Update` (or
`nothing found`), `Library browser kept the selection <key>`, `Library source <url> (on|off):
<what>; versions ...; auto-update on|off`, `Library source added|removed: <url>`, `Library
search '<text>': <n> components, <m> libraries, <k> hidden by the
licence`, `Library browser selected <library>/<component>: version ...,
size ..., licence ...` (or `selected library <id> (<url>), licence ...,
fetched|not fetched`), `Library size <row>`, `Inserted component <name>
(linked|copy) from <library> <version>, licence <spdx|none>` (from Place
Part with `, dropped in; bodies <uids>; mount frames <name> (<kind>), ...`;
mitcad#525), `Insert from Library: <library>/<component> has mount frames
<name> (<kind>, <how it moves>): Place Part`, `Place Part <name> from
<library> <version>: frames ...`, `Inspect Place Component: place part <name>:
frame <frame> (<kind>), <joint> joint, on <target>[, cell (1, 0)|, cells
(0, 0) and (0, 1)]` (`Inspect Place Component: failed: <why>`, `place part
<name>: no pick, Enter drops it in`), `Placed part <name> (linked|copy)
from <library> <version> as <component> (<occurrence>) by its frame
<frame> on <target>[, cells ...]: <joint> joint <Joint3> at (x, y, z);
bodies <uids>` (where the frame went, in the design), `Library
part <name>: <library> <version>, size <row>, <status>`, `Library changes:
...`, `Updated library parts: ...`, `Parts list row: <qty> x <part>
(...)`, `Publish: ...`. Tests: `tools/ui-library-test.sh`,
`tools/ui-windows-library-test.ps1` (ctest `app.ui-windows-library`); the
sources' settings with and without the Auto-update key in
`tests/library_settings.cpp` (test_app), and `ui-library` checks that no
start-up fetch runs with the defaults;
Place Component and Joint Origin's Mount Frame `tools/ui-mount-test.sh` and
`tools/ui-windows-mount-test.ps1` (ctest `app.ui-windows-mount`: synthetic
Gridfinity bins on a baseplate's cells, a taken cell refused, a lamp free
on a face, an own screw's frame made, published and placed into a hole, a
drop-in joined by its frame; with `MITCAD_GRIDFINITY_LIBRARY`, a library
of a ready-made model from the test files, its parts too), the frames'
texts in `tests/mount_frames.cpp` (test_app).

### Part picker

`library/PartPicker.*`, `library/PartFavourites.*`, `library/PartGroups.*`,
`library/UsedParts.*` (mitcad#418, mitcad#527). The library's parts as pictures, for Insert from Library
(the browser's results) and the commands that choose parts (BUILD's
Profile, Panel, Linear Part and Connector).

- **`PartGrid`** (a widget): the groups on the left (Alt+O): Favourites
  first, In this design (only while the design uses one of the parts
  listed), All parts, then the libraries' groups as a tree, Other (no
  category) and Libraries to get (an index's libraries not fetched, in
  the browser).
- **The groups' tree** (mitcad#527, `library/PartGroups.*`,
  [Groups](../docs/libraries.md#groups)): a `QTreeWidget` of the groups
  `library_search` gives each part (`groups`: a category's names from the
  top, from its `parent` and path names). A group is known by its key,
  its names in lower case joined by `/` (`groupKey`: `wood/dimensioned
  timber`), so the groups of several libraries with the same names are
  one; a name like an id is shown with its words (`linear-parts` as
  "Linear parts", `groupLabel`). A part is listed in each of its groups
  (a component's `category` may be a list) and stays one tile: one star,
  one count, once in Favourites and In this design. A click on a group
  shows its parts and its subgroups' (`inGroup`); search and the system
  filter work inside it. Each level is in the libraries' order
  (`order`: the place of the first category naming the group), then by
  name, the starred groups first (`PartGroupTree`). Groups with subgroups
  fold and unfold by their arrow, a double click, or Left and Right; the
  folded ones are kept in the settings (`partPicker/foldedGroups`, by
  key, shared by every picker) and unfolded by default; the folds above
  the group shown open to show it, the setting kept. Shipped libraries'
  main groups: Wood, Slot profile, Motion, General fasteners, Hardware,
  Workshop ([docs/libraries.md](../docs/libraries.md#mitcads-parts-library)).
- Above the grid: Search (Alt+S; the words `library_search`
  matches: name, id, standard, description, categories, keywords,
  supplier, part number, library), the slot System (Alt+Y; only when a
  part has one), the Supplier (Alt+R, mitcad#949, `library/PartSuppliers.*`:
  All suppliers, then each supplier of the parts listed, by name: the
  suppliers with articles on a part's rows, `library_search`'s
  `suppliers`, and a part's own `supplier`; a part none of whose rows the
  supplier sells is hidden, a tile of one row by that row's articles; the
  choice is kept in the setting `partPicker/supplierFilter` and is the
  size's Supplier filter to start with; only when a part has a supplier),
  Star and the tiles' size (Tiles, Alt+T, 48 to 192
  pixels, the setting `partPicker/tileSize`). A tile: the part's preview
  (`library_preview`, read a few at a time while the application is
  idle, only for tiles a view paints, kept for the session), its name,
  its main sizes (a BUILD part's section and row, else its standard),
  `×N` when the design uses it (mitcad#518: a tile of one configuration
  row counts the uses of that row, so every size of a timber family
  has its own count and the sizes the design does not use have none;
  the default row's count includes the uses placed without a row; a
  part without rows counts its component's uses; `UsedCounts`), a star in
  the corner. Refused parts
  (`library_build_candidates`' `refused`) are greyed and cannot be
  chosen. Hovering a tile shows a larger picture with the sizes, the
  supplier, the part number, the library and its version, the licence
  and the reason a refused part does not fit. Up and Down in the search
  move through the tiles; a double click or Return activates a tile.
- **Favourites**: a click on a tile's star (or Star, Ctrl+D) stars the
  part; a click on a group's star (Ctrl+Shift+D) stars the group and its
  subgroups (`setGroupsStarred`, by key; turned off, also a star kept
  before mitcad#527 by the id of a category that is the group). Stars
  are the user's (`PartFavourites`, settings group `partPicker`, keys
  `starredParts` and `starredGroups`), keyed by the library's id and the
  component's id and, for a tile of one configuration row, the row
  (`library|component|row`, mitcad#491: every size is a tile of its own,
  so starring 48x148 stars that size only), so they survive library
  updates; a part without rows keeps its key `library|component`. A star
  kept without a row on a component that has rows (starred before
  mitcad#491, or on a tile without rows) marks only its default row
  (`configurations.default`, else the first), and turning that tile's
  star off removes it. A starred part that is not listed is left out of
  Favourites. Order (`pickerOrder`,
  `groupOrder`): starred first, then the rest, each in the given order;
  starred groups first after Favourites, In this design and All parts,
  and on every level of the tree; in Favourites the parts of starred
  groups first (any of a part's groups). Search and the system filter
  work inside every group.
- **When the order applies** (mitcad#517): a click on a star (a tile's,
  Star, Ctrl+D; a group's, Ctrl+Shift+D) changes only the star. The
  tiles and the groups stay where they are while the picker shows that
  list, so the tile clicked does not move from under the mouse and
  several sizes can be starred in a row; the keyboard focus and the
  selected tile stay on the tile whose star changed. The starred-first
  order (and the groups') is applied the next time the list is made
  again: when the picker opens (`ask`, `setParts`), the group, the search
  or the system filter changes, or the design's parts are read again. In
  Favourites a star turned off leaves the tile in place with its star
  off until then, and a part starred elsewhere appears there the next
  time Favourites is shown (`PartGrid::refreshStars`, no refill).
- **In this design**: `usedLibraryParts` (the model's `library_usage`
  query): the BUILD sources and inserted library components with how
  many times the design uses each, in total (`count`) and per
  configuration row (`rows`: `{config, count}` of each row the design
  uses, mitcad#518); a part placed without a row is the component's
  default row's. The group lists only the tiles the design uses: for a
  part with rows, the sizes it uses.
- **`PartPicker`** (a non-modal dialog of the main window,
  `PartPicker::of(window)`, shown without taking the focus from the
  view; Docked and Floating chrome alike): `ask(PickerRequest)` with the
  command's id, a title ("Part Picker - Profile"), the parts
  (`buildCandidates(fields)` runs `library_build_candidates`;
  `pickerParts(QVector<build::Part>)` turns a BUILD catalog's parts into
  tiles with their categories and licences from one `library_search`),
  the current key and the system to start with. A click (or Return) on a
  tile emits `chosen(part, command)`; `commandEnded(command)` hides it
  unless **Pin** (Alt+P) is on; a pinned, open picker follows the next
  `ask`. `setDesignParts(reader)` and `designChanged()` keep In this
  design current. Its place and size: the setting `partPicker/geometry`.

Logs: `Part picker opened: <n> parts for <command or dialog>`, `Part
picker follows <command>: <n> parts`, `Part picker group <group>: <n>
parts, first <library>/<component>` (every time the tiles change: a
group, a search, the system, the parts; not a star, mitcad#517; a
subgroup by its path, `General fasteners/Screws`), `Part
picker tiles: <tile>[ x<uses>], ...` (the same moments: the first 24
tiles in order with their use counts, mitcad#518; a tile of a row as
`<library>/<component> (<row>)`), `Part picker groups: <group>, ...`
(the groups in the tree's order, whenever they are made: subgroups by
their paths, a folded group as `<group> (folded)` without its
subgroups), `Part picker group <group> folded|unfolded` (by the user,
kept), `Part picker search
'<text>'` (searched by the grid), `Part picker system <system|all>`,
`Part picker in this design: <n> parts: <library>/<component> x<count>[
[<row> x<count>, ...]], ...`, `Part picker star <component>[ (<row>)]
on|off` and `Part picker group star <key> on|off` (the group's key,
`slot profile/profiles`; each followed by
`Part picker tiles kept: <tiles as above>; current <tile|none>` and
`Part picker groups kept: <groups>`: the star changed, the order did
not), `Part picker chose <library>/<component>[ (<row>)]`, `Part
picker refused <library>/<component>: <reason>`, `Part picker shows the
part chosen: <library>/<component>` (`ask` scrolled to it and highlighted
it, mitcad#452; before it, when needed, `Part picker group <group> has
not the part chosen: All parts`, `Part picker search cleared: it left out
the part chosen`), `Part picker pinned on|off`, `Part picker closed with <command>`, `Part picker stays pinned
after <command>`. The window's places, once laid out after it shows,
moves, resizes or its tiles change (for UI tests): `Part picker place
search|star|pin at x,y; in the picker x,y`, `Part picker group row
<group> at ...` (the middle of the row, clear of its star), `Part
picker place group star <group> at ...` (the star of a library group's
row), `Part picker place group fold <group> at ...` (the arrow of a group
with subgroups, left of its row; logged again after a fold)
and `Part picker tile <library>/<component> at ...` (the tiles in sight, up to
24): in the main window's coordinates as the panel's places, then in
the picker's own (the Windows tests post their clicks to its window); a
row inside a folded group has no place.
Tests: the data in `tests/part_picker_data.cpp` (test_app; the groups'
tree of two libraries, mitcad#527), Insert from
Library in `tools/ui-library-test.sh` (part 7; the tree in part 8) and
`tools/ui-windows-library-test.ps1` (part 4; the tree in part 5: one
Wood of two libraries, a part in two groups and once in Favourites, a
fold kept), BUILD in
`tools/ui-build-test.sh` (part 8; part 9, the part a command starts with,
mitcad#452) and `tools/ui-windows-build-test.ps1`
(part 8; both with the tree for Profile's role: Slot profile with its
Profiles, listing them, folded and unfolded by the arrow); the
manifest's side in `core/vcs/src/tests/library_groups.rs`. The stars that leave the tiles and the groups in place
(mitcad#517) are in the BUILD tests' parts 8 and 9 (`Part picker tiles
kept`, `Part picker groups kept`, then the order after switching the
group) and the Windows timber test's part 7; the per-size counts
(mitcad#518) in the Windows timber test's part 7 (the tiles' `x<uses>`,
In this design with the sizes the design uses) and the Linux timber
fasteners test (`In this design` with the row), the data in
`tests/part_picker_data.cpp` (`UsedCounts`).

### Options of library parts

`library/OptionChooser.*`, `library/PartOptions.*`,
`library/EditOptionsDialog.*` (mitcad#810; the design in
[docs/configurations.md](../docs/configurations.md), "Part pickers and
BUILD" and "Inserted components"). A library part, a BUILD part or a
linked design whose configuration table is **open** (`options`, not
`closed`) is chosen by its options rather than only by its rows.

- **The choosers** (`OptionChooser`): a chooser per option, in the
  options' order: a combo box for a list (its allowed values), a number
  field for a range or a free number (its range and step, in the option's
  unit; the value is taken on Enter or Tab), a text field for a free
  text; a driven option (a formula, or bound to the parent) shows its
  value, read-only, its expression in the tooltip. The tooltip names what
  is allowed and the rules that narrow it. Each change asks the
  `configure` answer with the options chosen up to it; the later options
  keep their values where the rules still allow them (asked again one by
  one, in order) and move where they do not, and the notice under the
  choosers lists the moves with their rules (`Length 1200 mm → 1000 mm:
  Black is short`). A combination the rules refuse is not taken: the
  choosers go back and the notice, in the error colour, says why. A
  **closed** table (and one of the old form, `selectors`) looks as it
  does today: a combo box per option of the values its rows have, or the
  rows by name (`librarySize<n>`). An open table whose options cannot be
  read (a library or model without the answer) falls back to its rows.
- **Insert from Library**: the browser's size is the choosers
  (`SizeChooser` is `OptionChooser`), asked of the library's component at
  the version shown (`library_configure`); Tab goes from Version through
  the choosers to Linked. Insert sends `insert_component` with
  `parameters`, the options as constants (`{"Color": "'Black'",
  "Length": "800 mm"}`), and `library.config` the row the combination
  matches, none when it matches no row (so a combination without a row
  counts under the component in In this design and the tiles' `×N`).
  Place Component (mitcad#525) sends the same `parameters`.
- **BUILD's part picker**: the rows of an open table stay tiles (presets
  of the options; favourites and use counts per row as before,
  mitcad#491, mitcad#518), and the options of the tile chosen show under
  the grid. A click chooses the part with them, and a change of an option
  chooses it again: the command's Part takes an **options key**, the
  tile's key with the matching row and the options (`lib|comp|row?{json}`,
  `build::optionsKey`); `Catalog::find` gives the part with them,
  `build_place`'s `part` carries `parameters`, the Part drop-down lists
  it in place of its tile with the option values.
- **The Standard and Supplier filters** (mitcad#949, `OptionChooser`,
  `library/PartOptions.*`; the rows' tags in
  [docs/libraries.md](../docs/libraries.md#standards-and-suppliers-of-rows)):
  above the size choosers of Insert from Library, Library Parts, Edit
  Options… and BUILD's part picker, when the rows name standards or
  suppliers' articles: Standard (Alt+D: All, then the standards the
  component's rows name in its order, Non-standard last) and Supplier
  (Alt+E: All, then the suppliers with articles on its rows), default
  All. A filter narrows the choices to the rows that match: a closed
  table's rows (`filteredTable`: the row chosen kept when it matches,
  else the default or the first that matches), an open table's list
  values (`narrowAnswer`: the values of the matching rows with the
  options before); an open table on a row that does not match moves to
  the first that does. It changes no part and is not kept in the design;
  the browser's Supplier filter starts as the grid's. The designation
  follows the Standard filter: Insert from Library shows the row's
  `Designation:` (`library_configure` with `standard`: `DIN 912 M6x8`;
  All: the row's first standard; `M6x6 (non-standard)`) and its
  `Supplier part numbers:` (`Würth 0084 6 8 (8.8 zinc-plated); …`) under
  the size, and Insert sends `insert_component`'s `library.standard`, so
  the part and the parts list keep it. Library Parts and Edit Options…
  show the chosen row's supplier part numbers under the choosers; Edit
  Options… reads the library's rows of the part (`library_show` at its
  version) for its filters. Tab goes from Version through the filters to
  the size choosers in the browser; in Library Parts the size choosers
  stay right after Version.
- **Edit Options…** (a library part's or a linked design's row in the
  browser, after Property Sets…): `EditOptionsDialog`, titled `Edit
  Options - <name>`: the choosers of the inserted component (the
  `configure` query with `component`, its own expressions first), and an
  expression field per option with the parameter suggestions and the
  Formula Editor (fx). A constant there chooses the option; a formula of
  the design's parameters (`Shelf_W - 2 * Wall`) binds it, and its
  chooser then shows the value it gets. OK keeps every option's
  expression in one undo step (`set_component_parameters`); the options
  belong to the component, so all of its occurrences follow.

| Model entry | Used for |
|---|---|
| library command `library_configure` (`url`, `id`, `rev`, `component`, `values`) | Insert from Library, BUILD's picker: the `configure` answer on the library component's design |
| query `configure` with `component` (uid), `values`, `parameters` (the bound expressions; left out the first time: the component's own) | Edit Options… |
| `set_component_parameters` (`component`, `parameters`) | Edit Options…' OK |
| `insert_component`'s and `build_place`'s `parameters` | the options chosen |

Logs (the prefix is `Library` in Insert from Library, `Part picker` in
BUILD's picker, `Edit Options` in the dialog): `<prefix> options: Color =
Black, Length = 800 mm (no row)` (or `(row Standard)`; a driven option
`(driven)`), `<prefix> option <name> allowed <300 mm … 1000 mm, step 10
mm | Black, White | any>[ (<rules>)]`, `<prefix> option <name> chosen:
<value>`, `<prefix> option moves: <move>; ...`, `<prefix> option refused:
<why>`, `<prefix> options not read (<why>): the rows instead`, `<prefix>
size <row>` (a closed table); `Library browser options of
<library>/<component>: <options>`, `Inserted component <name> with
options Color = 'Black', Length = 800 mm (no row|row <row>)`, `Part
picker options of <tile>: <options>`, `Part picker chose
<library>/<component> with options <options>`, `Edit Options opened for
<name>`, `Edit Options <option> expression at x,y` and `Edit Options OK at x,y` (in the main window,
as the panels' places), `Edit Options bound <option> = <expression>`,
`Edit Options unbound <option>`, `Edit Options accepted: <option> =
<expression>, ...`, `Edit Options of <name>: ...` (the command run),
`Edit Options of <name>: no options (<why>)`. The filters (mitcad#949):
`<prefix> filter: Standard DIN 912, Supplier All: 7 of 20 rows`, while a
filter is on `<prefix> <option> choices (Standard DIN 912, Supplier All):
8, 10, 12`, `Library browser row M6x8: designation DIN 912 M6x8; supplier
part numbers <text|none>`, `Library part <name> supplier part numbers:
<text>`, `Edit Options supplier part numbers: <text>`, `Part picker
supplier <id|all>`. Tests:
`tools/ui-part-options-test.sh` (an open table's options in Insert from
Library, one narrowed by a rule; Length bound to the design's parameter
in Edit Options and following it; a closed table as before), the answers
and requests in `tests/part_options.cpp` (test_app; the filters'
narrowing and the designation's standard, mitcad#949),
`tests/part_picker_data.cpp` (the grid's Supplier filter),
`tools/ui-library-test.sh` part 9 and `tools/ui-windows-library-test.ps1`
part 6 (Standard = DIN 912 on the socket head cap screw).

### Design policy

`browser/PolicyDialog.cpp`, `BrowserController::openPolicy`,
`MainWindow::startSketch` (mitcad#220; the model's side in commands.md,
"Design policy").

| Command | What it does | Model command / query |
|---|---|---|
| `assembly.policy` Design Policy... (command search; the browser's component menu, last entry, opens it for that component) | `PolicyDialog`: the nine settings of the component or the root, each with what it allows in words (incoming and outgoing links told apart; the first-sketch pair, Alt+I and Alt+K, next to the sketch attachment pair, mitcad#293), the presets (Alt+H Hybrid, everything allowed, the defaults; Alt+P Part; Alt+A Assembly; mitcad#421), Alt+T for the incoming sketch attachment setting, and next to a blocked setting the number of objects that break it, a button that lists them; counts follow the edited settings; OK sets them as one undo step, Cancel changes nothing | `policy` (with the proposed `policy`), then `set_policy` |

- A new design (File > New, the window's first design) has the root and
  an empty Component1, active, both Hybrid (mitcad#421): Component1 takes
  sketches, features and components, and other components' sketches may
  lie on its faces. It is named as any new component is (mitcad#711); a
  design made before keeps its Part1. Opened and imported designs are not
  rearranged; a stored policy keeps its values, an owner without one is
  Hybrid.
- New Component (the command and the browser's) makes a component with
  the defaults, Hybrid. Made in a component that takes no components (one
  set to Part), or by a feature's New Component operation there, the
  model places it
  beside the part, in the nearest owner above that takes components
  (mitcad#292): the reply's `placement_warning` is shown as a hint, also
  when the command's panel closes (instead of "Done.").
- Create Sketch asks the model (`sketch_supports`) first: an active
  component that takes no sketches refuses with the reason and the user
  activates another one or changes the policy (nothing is redirected).
  While a plane is picked, the faces and planes of owners the policy
  leaves out (either side refuses the attachment) are not picked or
  highlighted, the hint names them, and a browser row, a selected face or
  Create Sketch on a plane of theirs is refused with the reason. The
  active component's origin planes are always its own. Linked projections
  are not filtered: the model checks the reference settings and refuses
  with the reason (a projection without Projection Link is always allowed).
- Commands that the policy refuses (features in an Assembly, components in
  a Part with no owner above that takes them, a move into a component that refuses what moves) show the
  model's reason in their panel's preview or when OK is pressed. Under the
  defaults nothing is refused and no hint names left-out owners.

Logs: `Placement warning: <text>`, `Create Sketch refused: <reason>`, `Create Sketch: policy leaves out
<owners>`, `Sketch <name> is in <component>`, `Design Policy dialog opened
for <name>`, `Policy dialog <name>: <preset|custom>; <setting>
allowed|blocked (<count>), ...`, `Policy dialog preset <name>`, `Policy
violations of <name> <setting>: <objects>`, `Policy of <name> set:
<preset|custom>`, `Design Policy dialog closed without a change`; with
`MITCAD_LOG_PICKS`, Create Sketch's `Pick places:` lists the faces it can
pick (`Pick face <body>/<face> in <occurrence> at x,y`). Test:
`tools/ui-windows-policy-test.ps1` (ctest `app.ui-windows-policy`).

### Activating the picked component

`browser/ActivateQuestion.*`, `MainWindowActivate.cpp`,
`MainWindow::createSketchOnPick`, `CommandHost::pickComing` (mitcad#960).

A sketch or a construction feature goes into the active component, also
when it lies on another component's face. Create Sketch and the Construct
commands (every construction plane, axis and point, `construct.*`) ask
right after the pick, when the picked face, plane, edge or point belongs
to a body or datum of a component other than the active one, the active
component's child occurrences included:

> The face belongs to M6 rod bracket:1, but Corner brace:1 is active.

(`plane`, `edge`, … for other picks), in a message box **Activate
Component** with **Activate M6 rod bracket:1** (the default, Enter or
Alt+A), **Keep in Corner brace:1** (Alt+K, also Esc) and a check box
**Don't ask again** (Alt+D). The picked component is named by the
occurrence picked; the active one by its occurrence above the pick when
there is one, else by its first (the root by its name).

- Nothing is asked for the active component's own geometry (at any of
  its occurrences), its origin planes, axes and point, a pick the design
  policy refuses (refused as before, [Design policy](#design-policy)), a
  component that takes no sketches (Create Sketch: the picked
  component's `sketch_supports` gives a reason; the sketch goes as
  before), an edit of an existing construction feature, a window
  selection, or while an agent's request picks (MCP `ui_select`,
  `ui_command`). Other features on another component's faces (Extrude,
  Fillet, …) are not asked about.
- **Activate** runs `activate_component` as one undo step of its own, as
  the browser's radio button does, and the command goes on with the same
  pick: Create Sketch makes the sketch in that component (a plane of its
  own: no link), edited and shown where the pick was, so of a component
  placed several times the occurrence picked; a Construct command's
  activation is run outside its panel's undo range
  (`CommandSession::runActivation`, as a light bulb's change: kept on
  Cancel), its cached previews dropped, and the preview computed in the
  new component. **Keep** goes on as before (the sketch of the active
  component linked to the face).
- **Preferences › General › Ask to activate the component of a picked
  face** (settings `general/askActivateComponent`, on by default) turns
  the question off; Don't ask again clears it, the preference turns it
  back on. With no saved value, `MITCAD_ASK_ACTIVATE_COMPONENT=0` turns it
  off: `tools/ui-test-lib.sh`, `tools/ui-windows-lib.ps1` and
  `tools/ui-macos-lib.sh` set it, so that the tests' sketches on other
  components' faces go as before; a test of the question sets it to 1.

Logs: `Activate component asked: <picked> (active <active>)`, `Activate
component answer: activate <picked>|keep in <active>[, don't ask
again]`, `Activated <component>`, `Activate component not asked: <picked>
takes no sketches (<reason>)`, `Preferences: ask to activate the
component of a picked face on|off`. Tests: the decision and the message
box in `tests/activate_question.cpp` (test_app);
`tools/ui-occurrence-face-sketch-test.sh` and
`tools/ui-windows-occurrence-face-sketch-test.ps1` (a sketch on the
root's child occurrence's face asks, Keep in Root and Activate Box:1);
`tools/ui-activate-component-look.sh <dir>` saves a picture of the
question (a one-off visual check).

### Reorient Design

`browser/ReorientDialog.cpp`, `BrowserController::openReorient`
(mitcad#502; the model's side in commands.md, "Reorient Design").

| Command | What it does | Model command / query |
|---|---|---|
| `assembly.reorient_design` Reorient Design... (command search; the browser's menu of the root, last entry) | `ReorientDialog`: turn about an axis (Alt+T) with the axis X, Y or Z (Alt+X, Alt+Y, Alt+Z) and 90, 180 or 270 degrees (Alt+9, Alt+1, Alt+7), or an origin plane (Alt+P: XZ or YZ) becomes XY (Alt+O); the design shows turned at once and with each choice; what changes (features rewritten, Moves added after imported bodies, components placed anew, named views, sections and lights) or the model's reason for refusing the turn, which disables OK; OK keeps it as one undo step, Cancel (or closing the dialog) takes it back with the undo history as it was | `begin_command`, then for each choice `restore_command` and `reorient_design`; OK `discard_command`, Cancel `restore_command` and `discard_command`; afterwards the `instances` query with `properties` |

- The view's camera stays where it is, so the design shows turned against
  the origin and the grid; named views (`Home` among them) turn with the
  design.

Logs: `Reorient Design dialog opened`, `Reorient Design preview: <turn>:
<n> features, <n> moves, <n> components`, `Reorient Design preview refused:
<turn>: <reason>`, `Reorient Design cancelled`, and after OK `Reorient
Design: <turn>` (`90 degrees about X`, `XZ becomes XY`) and for each placed
body `Reorient Design body <name>: box (x, y, z) - (x, y, z)` (in the
design's coordinates, to 1e-6 mm). Tests: `tools/ui-reorient-design-test.sh`,
`tools/ui-windows-reorient-design-test.ps1` (ctest
`app.ui-windows-reorient-design`; the root's menu cannot open in session 0,
so the command comes from a shortcut there).

### Project web viewer

`web/MainWindowWeb.cpp`, `web/ProjectWeb` (mitcad#423); the pages
themselves are `web/WebPage` (`writePage`, `writePageOfFile`), the model's
side is commands.md, "The web viewer". A project's `web/` folder:

| File | What |
|---|---|
| `web/index.html` | plain HTML and CSS, no scripts (light and dark by `prefers-color-scheme`): the project's folder name, a card per design (its thumbnail or "No preview", name, path, "Saved by <name>, <UTC time>") linking its page |
| `web/<slug>.html` | the page of the design's newest version (one self-contained file) |
| `web/<slug>.png` | its thumbnail: the version's preview (`versionThumbnailPath`, 256 px), when there is one; else one written before stays |
| `web/designs.json` | `[{"name", "file", "page", "thumbnail", "saved_by", "saved_at"}]`, sorted by `file` ignoring case; null where unknown |

`<slug>` is the design's path in the project with `/` as `--`, other
characters outside `[A-Za-z0-9._-]` as `-`, without `.mitcad`
(`designSlug`); a clash ignoring case, and `index`, get `-2`, `-3`, ... in
the order of the paths (`designSlugs`).

| Command / moment | What it does | Model commands |
|---|---|---|
| `file.web_viewer` Web Viewer... (File › Project) | every design `inspect_folder` lists: its newest version's author, time and blob (`history`); its page with `writePageOfFile` in a job (`runJob`, the progress dialog per design, Cancel between designs: nothing recorded); the thumbnails (the open design's preview saved first when it is unchanged); `writeIndex`, which also removes the pages and thumbnails the previous `designs.json` listed for designs gone (only plain file names of the folder). Off before: one version with the setting, `Change project settings: web viewer on`; on: as Update | `inspect_folder`, `history`, `set_project_settings` with `shared.web_viewer` and `paths` |
| `file.update_web_viewer` Update Web Viewer (File › Project) | the same when the viewer is on (a hint when it is not), recorded as `Update the web viewer` | `commit` |
| Save (`writeFile`, before `recordVersion`) | when the viewer is on and updated on each Save (`webViewerOnSave`): unless the file is as its latest version (no version follows), the open design's page (`writePage`, the design file's folder for textures), its thumbnail (`saveVersionThumbnail` of the file's blob, copied), and the index with the other designs as `designs.json` had them (while there with the same slug); these files and those removed go into the design's version | `info`, `status`, `inspect_folder`, then Save's `commit` |
| Project Settings › Web viewer | **Update the web viewer on each Save** (Alt+U; off and disabled until Web Viewer turned it on): for everyone, a version `Change project settings: web viewer (not) updated on each save` | `set_project_settings` with `shared.web_viewer.update_on_save` |

Both commands are enabled in a project with versions; a page that could
not be written is left out of the index and named in a warning. Logs:
`Web viewer: page of <path>: <slug>.html (<n> bytes[, thumbnail])`,
`Web viewer: wrote <n> page(s) and the index in <folder>[, removed
<files>]`, `Web viewer: recorded as version <id>` (or `no new version`,
`not recorded: <why>`), `Web viewer: not turned on in <root>`, `Web
viewer: wrote the page of <path> and the index in <folder> (<n> bytes)`,
`Web viewer: not updated on Save of <path> (off in Project Settings)`,
`Web viewer: <path> unchanged, its page kept`, `Project Settings web
viewer: not turned on | on, updated on each Save | on, not updated on
Save`, `Project Settings: web viewer updated on each Save on|off,
recorded: <id>`. Tests: `tests/project_web.cpp` (test_app),
`tools/ui-web-viewer-test.sh`, `tools/ui-windows-web-viewer-test.ps1`
(ctest `app.ui-windows-web-viewer`).

Limitations: a design that was never saved, opened or restored here has
no preview, so its card shows "No preview" until a Save; two people who
save different designs both change `index.html` and `designs.json`, which
a sync then asks about (take either; Update Web Viewer writes them again);
the pages are whole files in the history (each version of a page is a new
blob).

## Selection

`OcctViewer` names picks through the bodies' `geometry::Shape`
(`names_of_face`, `name_of_edge`, `name_of_vertex`) and reports them as
`SelectionItem`s; it keeps no selection of its own. Its pick filter
(`setPickFilter(kinds, test)`) also limits the hover highlight; a
selection scope (`setPickScope`, sketch mode and Project, see [Sketch
selection filters](#sketch-selection-filters)) also tells its test how an
item is picked. Selection outside commands is the main window's
(`m_selection`). Outside sketch mode a window selection keeps one kind
(profiles, bodies, edges, faces, vertices, sketch curves, points, datums,
in that order of preference), and Shift with a click adds or removes as
Ctrl does. Faces win over bodies, so the bodies filter is off by default.

The highlights look alike (mitcad#149): the hover (`setHoverColor`,
OCCT's dynamic highlight styles, set up by `setHighlightStyle`) and the
selections (`setHighlights`) cover a face half see-through (the hover;
selections 35 %, whole bodies 55 %) in a colour that light does not
change (`flatMaterial`: emission only, no specular), so the face's own
shading shows through and the highlight does not wash out to white
where the light falls on the face head-on. The face's boundary, edges and
vertices are drawn opaque in the colour, edges 3 px wide. The highlights
are in the top layer, which keeps the scene's depth: nearer geometry
hides them (OCCT's own default put the hover of a face, edge or vertex in
the topmost layer, which clears the depth), and a polygon offset brings a
face's highlight just in front of the face. Test:
`tools/ui-hover-test.sh`.

A sketch's points are drawn after the highlights of its curves
(mitcad#897): the plain point markers, a selected point's ball and the
hover of a point are in a layer of their own just after the top layer
(`OcctViewer` "sketch points", immediate as the top layer is, so OCCT
draws it after it), against the scene's depth as before, so that a
selected or hovered line, arc, circle or spline shows its end, centre,
fit and control points on top of its wider highlight line, and a body in
front still hides them. In that layer the ball is drawn after the plain
markers (a higher display priority), so it covers its point's marker.
Logged as `Sketch layers:` and `Sketch highlights:`; tests:
`tools/ui-sketch-spline-test.sh`, `app.ui-windows-sketch-spline`.

### Selection inputs

A command's selection input keeps each model item once (mitcad#199,
`sameEntry` in `framework/Selection.hpp`). A profile is a region of a
sketch: the occurrence that places the sketch in the view is not part of
it, and a definition keeps the key it stored for a region after the
sketch changed while the view names the region by its current key; the
model's `profile` query names the region a key means now
(`CommandSession::resolver`, asked once per item and pick). So:

- A click toggles: a click on a selected region takes out every entry for
  it (`toggleEntry`), whatever key or occurrence it was put there with;
  clicking a region twice gives back the selection before, in any order.
  The model composes the selected regions odd/even ([commands.md, Profile
  selections](../core/model-api/src/commands.md#profile-selections)):
  clicks on nested squares' rings and centre leave exactly the regions
  clicked an odd number of times.
- Selections filled from elsewhere than clicks (the newest profile of
  `init`, the selection made before the command, a definition loaded for
  editing) are made canonical when the command starts
  (`canonicalizeSelections`): profiles placed by the occurrence the view
  shows the sketch in (`CommandHost::profileOccurrence`), so that their
  highlights lie on the shown profiles and a click on one is on its entry,
  and each region once (`uniqueProfiles`). Before, a click on the newest
  profile of a placed component's sketch added the region a second time,
  and the feature failed with "profile … is listed more than once".
  Faces, edges, vertices and bodies without an occurrence (a loaded
  definition's references have none) get the first occurrence of their
  body's component (`CommandHost::bodyOccurrence`): before, editing a
  thread on a placed component's face highlighted the face unplaced, at
  the component's own coordinates, on another part of the design.
  Construction planes, axes and points without an occurrence get the one
  the view shows them in (`CommandHost::datumOccurrence`: the first
  occurrence of their feature's component; none for the origin's, which
  the view draws unplaced). In both cases a click on the shown face or
  datum is on the loaded entry and takes it out; before, it added the face
  a second time, or replaced a datum with itself (mitcad#432,
  `tools/ui-placed-edit-test.sh`).
- A region a sketch edit split (mitcad#407) is all its pieces: the
  `profile` query's `regions` lists them, the main one (which keeps the
  old key in the model) first, and the resolver returns that list. Every
  piece is part of the entry (`containsEntry`): the highlight shows them
  all (`profile_shape` draws every piece), and a window over a piece adds
  nothing. A click on one piece takes out just that piece (mitcad#410,
  `removeEntry`): the entry is replaced, in its place, by the other
  pieces under their own current keys, the main one first. The pieces are
  separate regions in the view, so a click works on the region clicked,
  as everywhere else: taking the whole reference out would also unselect
  pieces that were not clicked, and a second click on the same piece
  would then add back only that piece instead of giving back the
  selection before. The old key cannot stay for the rest: it
  would still mean every piece. Before, toggling compared only the main
  region, so a click on another piece added it instead (the model
  deduplicated it), and the piece could not be clicked off. A definition
  that lists a piece after its split reference keeps the reference only
  (`uniqueProfiles`).
- An empty selection is the usual missing-selection hint ("Select
  profiles.") with OK refused.
- **Shift + click places at once** (mitcad#451,
  `InputDef::withShiftClickApplies`): on an input of one item (max 1)
  that has it, a Shift + click the input takes is the pick and then the
  command's OK, as Enter would press it (`CommandSession::picked` calls
  `commit`; the asynchronous frontend's `commitAsync` waits for the
  inspection as for Enter). An item picked already stays picked (no
  toggle). When OK is refused (an input missing, the inspection's error,
  `apply` false) nothing is made, the panel shows the reason as after a
  refused OK, and the pick stays as a plain click leaves it. Inputs
  without it, and inputs of several items, keep Shift's meaning (adding
  to the selection); Ctrl + Shift and a window selection are unchanged.
  BUILD's Place On (Profile, Linear Part), Frame Face (Panel) and Near
  (Connector) have it. Log: `<command> <input>: Shift+click places at
  once`. Tests: `tools/ui-build-test.sh` (parts 2b, Floating, and 10).

Tests: `app.unit` (`selectionEntryTests`), `tools/ui-profile-toggle-test.sh`,
`tools/ui-placed-edit-test.sh` (faces and datums of a placed component),
`tools/ui-profile-split-test.sh` (split pieces, Docked and Floating),
part 14 of `tools/ui-windows-workflow-test.ps1`.

### Sketch selection filters

While a sketch is edited, picks go through the sketch's three-state
filter (mitcad#217, `framework/SelectionScope.hpp`, `MainWindowSelection.cpp`)
instead of the SOLID tab's filters (`m_filterActions`), which stay as they
are and decide again once sketch mode ends. Each row cycles empty (cannot
be selected) -> square (a click) -> cross (a click and a window, either
direction: inside, or crossing). The rows and their defaults, which every
sketch edit starts with (`SelectionScope::sketch`):

| Row | Default |
| --- | --- |
| Sketch geometry (the edited sketch's ordinary curves and points; centre lines too, they bound profiles) | cross |
| Construction geometry | cross |
| Projected geometry (reference entities) | cross |
| Dimensions and constraints (sketch mode's overlay) | square |
| Profiles | square |
| Visible body edges | square |
| Occluded body edges | empty |
| Body faces, body vertices, bodies, components, planes/axes/points | empty |

- Another sketch's curves and points have a row of their own, Other
  sketches' curves and points (`other-sketch-geometry`, mitcad#904), which
  only Project's context has: in the sketch's context they are never
  selectable, Shift included (`filterRowOf`); another sketch's profiles,
  dimensions and constraints have no row at all. A window collects every kind the filter lets a window
  take, not one preferred kind; dimensions and constraints come from the
  overlay (`SketchController::annotationsIn`) when their row is a cross.
- Visible or occluded: for a body edge, the view looks from the picked
  point of the edge (a click: the edge's point nearest where it hit; a
  window: its samples inside the window) towards the eye, along the view
  in an orthographic camera, and asks whether a face of a shown body lies
  in between (`OcctViewer::occluded`, `IntCurvesFace_ShapeIntersector` per
  body, made once; 0.01 mm tolerance). An edge partly hidden is one item:
  a window takes it when the part it holds is allowed. Bodies that are
  hidden are not shown, see-through ones (previews, ghosts) hide nothing.
  The camera of the moment decides, so an orbit changes the answer.
- Shift: a click with Shift also picks what empty rows hold (the view's
  kinds widen while Shift is down, `setPickScope`'s `expanded`); faces win
  over whole bodies as outside sketch mode. Shift pressed or released
  picks again under the pointer, so the hover shows what the click would
  take (`OcctViewer::eventFilter`, an application event filter for the
  key). Ctrl adds and removes; Ctrl+Shift does both; Shift with a window
  adds it as before without widening it. Other buttons and the wheel keep
  Shift's navigation. A face, a plane or a body is not picked where an
  edge the filter offers is within the edge's pick width, also one behind
  it (with Shift the sketch's own plane lies in front of the bodies). A
  right click opens the context menu for what a Shift+click would take,
  so a body's face in sketch mode still offers Project.
- The control: the SKETCH tab's SELECT group has a Selection Filter
  button whose menu holds `SelectionFilterPanel`: the symbols' legend, a
  button per row (its action `command_select.filter.<row>`, the symbol as
  its icon, the state in its tooltip and accessible description) and
  Reset to Defaults (`command_select.filter.reset`).
- Project (`commandSelectionScope("sketch.project")`, `CommandHost::commandScope`)
  has its own context while it runs: visible edges cross, occluded edges
  empty, faces and vertices square, other sketches' curves and points
  cross (mitcad#904; the edited sketch is `CommandHost::editedSketch`, its
  own geometry is in none of Project's rows); the control shows and
  changes it, and Shift widens a click to occluded edges. The input's own
  kinds (edges, faces, vertices, sketch curves and points, sketches)
  always hold. On OK and on Cancel the sketch's rows are
  as they were before. Other commands' inputs decide with their own
  filters as before.

Logs: `Selection filter sketch defaults: <row>=<state>, ...` (entering a
sketch), `Selection filter <scope> <row> <state>: ...` (a row clicked),
`... reset: ...`, `Selection filter project defaults: ...`, `Selection
filter sketch restored: ...`, `Selection filter widened (Shift)` and
`Selection filter as set`; with `MITCAD_LOG_PICKS` also `Sketch hover:
<item>|nothing`, and with `MITCAD_LOG_EDGE_PLACES` once the camera rests `Sketch pick places:` then
`Sketch pick edge F1.b0/<edge> visible|occluded at x,y`. Tests: `app.unit`
(`selectionScopeTests`), `tools/ui-sketch-select-test.sh`.

## Icons

24 x 24 SVG, drawn for MitCad (MIT, SPDX line first): dark outline
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
  opens as Revolution and Pitch with the expression as is. Growth goes in
  as `growth` unless it is a plain zero; a new helix is of MitCad's
  construction, and an edited one keeps its `construction` (FreeCAD's for
  the FreeCAD import's growing helices, mitcad#83). A fixed axis (the
  importer's) is not editable.
- **Sweep** (`solid.sweep`, `SweepCommands.cpp`): Type is Single Path,
  Path + Guide Rail (`guide_rail`, Profile Scaling) or Path + Guide
  Surface (mitcad#157): faces picked in Guide Surface go into
  `guide_surfaces`, the path must lie on them, and Orientation, Taper
  and Twist are hidden. A definition with guide surfaces and a twist or
  taper (which the model ignores) is not editable. Single Path has End
  Scale (`end_scale`, left out at 1) and Path + Guide Rail an optional
  Second Rail (`guide_rail2`), mitcad#464. Path + Guide Rail's Rail Point
  (mitcad#469) is Square to Path (`rail_point` `plane`, the default of a
  new sweep) or Fraction of Length (left out); a definition without
  `rail_point` loads as Fraction of Length. The path's extent has no
  Full / Partial choice (mitcad#795): Distance 1 (`fraction`, the part
  beyond the profile) is always shown, Distance 2 (`fraction2`, the part
  before it) when the preview's `sweep_path` puts the profile past the
  start of an open path (`profile_at` `inside` or `end`; `takeSweepPath`
  keeps it in the state and previews again when Distance 2 shows or
  hides) or when the edited definition has a `fraction2`. Both default
  to 1; a plain 1 / 1 (or 1 with Distance 2 hidden) is written as `{"type":
  "full"}`, anything else as `partial` with `fraction` and, when shown,
  `fraction2`; `full` loads as 1 / 1. In every Type alike. Logged
  `Sweep: the profile lies inside an open path; Distance 2 shown` (`at
  the start of`, `at the end of`; `a closed`; `hidden`) when the place
  changes. **Pipe** shows
  its Distance 1 (`fraction` from the path's start) the same way, 1
  written as `full`. Test: `tools/ui-sweep-test.sh`.
- **Fillet**'s Spline radius type (mitcad#464): Radius is the first
  control radius, End Radius the last, Knots the knot vector (empty:
  evenly spaced, degree 3 at most) and the Control Radii list the ones
  between, each with the spline set it is of; `load` splits the
  definition's `radii` the same way. **Pattern on Path** takes several
  sketch curves of one sketch as a chain (`"path": {"sketch": …,
  "curves": […]}`). Test: `tools/ui-feature-options-test.sh`.
- **Hole**: Counterdrill is `kind` `counterdrill` with `cd_diameter`,
  `cd_depth`, `cd_angle` (the cone's full angle); Taper Angle is `taper`
  (the wall's angle to the axis), left out when 0 and hidden for tapped
  holes, which the model refuses to taper. Thread and Hole take sizes
  from the model's `thread_sizes`.
- **Thread and Hole: Custom threads** (mitcad#893): Thread Type Custom
  (`thread_sizes`' last standard, without sizes or classes) hides Size,
  Designation and Class and shows Major Diameter (`thread_major`, a
  length, 10 mm; a shaft picked sets it to its diameter) and Pitch
  (`thread_pitch`, 1.5 mm), saved as `"custom": {"major", "pitch"}`
  expressions (the thread feature's top level, a tapped hole's `thread`),
  `"thread": {"standard": "custom"}` without a designation or class.
  Modeled shows Starts (`starts`, unitless, 1), saved as `starts`; the
  end chamfer's Custom size asks `thread_chamfer` with the major diameter
  and pitch. A tapped hole has Direction (Right Hand, Left Hand: its
  thread's `right_handed`) like Thread, so a left-handed tapped hole opens
  for editing. A thread or tapped hole of a standard size with `starts`
  (made through the API) is not offered for editing (its Starts is
  Custom's only). The descriptions name the size `custom 10x1.5, 2
  starts`. Tests: `tools/ui-create-test.sh` (Custom thread),
  `tools/ui-windows-selection-create-test.ps1` (5.).
  By Pick (mitcad#656) is one hole where Place On puts it ([Placement by
  picked geometry](#notes-on-particular-commands)): into a face at
  distances from two of its edges, at a point, or along a shaft's or a
  hole's axis into it (`placement` `{"type": "at", "at": {...}}`); a
  face hole of one position written before opens on its face from the
  origin axes and saves `at`, one of several positions keeps them as
  Placement Earlier Positions until By Pick is chosen. Flip is on the
  Placement line.
- **Hole: Clearance holes** (mitcad#903): Hole Tap Type has a third
  option, Clearance (`clearance`, after Simple and Tapped), which shows
  Screw Size (`clearance_size`, the model's `hole_sizes` sizes M1.6 to
  M24, M6 first) and Fit (`clearance_fit`: Fine, Normal, Coarse, Normal
  first; ISO 273's fine, medium and coarse series) after Drill Point. The
  size fills Diameter (the fit's clearance hole), Counterbore Diameter
  and Depth (DIN 974-1 for the ISO 4762 head, M6: 11 and 6.4),
  Countersink Diameter and Angle (90°; the larger of ISO 10642's head
  and ISO 15065's countersink, M6: 13.44; none for M24) and Counterdrill
  Diameter, Depth (the counterbore's) and Angle (90°), all of them while
  the Hole Type is any. A tapped hole's Countersink Diameter and Angle
  follow its thread: just over the major diameter (`hole_sizes` with
  `major`, M6x1: 6.6), at 90°. A field the size fills follows the size,
  the fit, the Hole Type and the thread size until a value is typed in it
  (or dragged): that value stays (logged `Hole cb_depth typed: kept when
  the size changes`), and so does a field that holds an expression.
  Choosing a Hole Tap Type fills every field again but expressions. Each
  fill is logged `Hole sizes for M6 normal: diameter 6.6 mm, cb_diameter
  11 mm, …; kept cb_depth`. The definition stores the values as plain
  values, as before, and `"clearance": {"size": "M6", "fit": "normal"}`;
  `load` sets Clearance, the size and the fit from it, and the stored
  values that the size does not give (or expressions) count as typed
  (logged `Hole values kept from the size: …`), so an edited hole keeps
  them when its size changes. A tapped hole loaded the same way keeps a
  countersink other than its thread's. Holes saved before have no
  `clearance` and open as they were. The description adds `(clearance
  M6 normal)` to the diameter. Tests: `tools/ui-create-test.sh` (Hole,
  Clearance), `tools/ui-windows-workflow-test.ps1` (12.).
- **Press Pull** on one face of a fillet opens the fillet for the radius
  of the set that rounds its edge (`Built::editing`).
- **Fillet and Chamfer handles** (`withManipulator`, mitcad#534): each
  set's handle stands on the set's first edge that can take one (a face
  picked in Edges gives none), at the edge's middle as the view shows the
  body, so a placed component's edge carries it too, and dragging it
  changes that set's value only. The edge's middle, direction, faces'
  normals and convexity come from the body's solid, in its own
  coordinates before the placement (`edgeSides` in
  `commands/ChamferHandles.hpp`, called by `edgeAt` in
  `FilletCommands.cpp`): convex where a point just off the edge along
  the first face's normal minus the second's lies outside the solid,
  concave where it lies inside (mitcad#571: a probe along the normals'
  bisector is outside at both kinds, took every edge for convex and
  turned an inside corner's arrows into the body, so its sizes grew when
  pushed). Where the arrows stand is plain math there too (ctest
  `app.unit`, also on a real L bracket's edges). Every arrow points the
  way its value grows: pulled along it the value grows, pushed back it
  shrinks. Fillet's constant Radius: an arrow along the bisector of the
  faces' normals (into the material at a convex edge, out of it into the
  open corner at a concave one), its knob at the middle of the rounding. Chamfer's **Distance**: for Equal Distance the
  same arrow, its knob at the middle of the bevel's face (distance ×
  sin(half the angle between the normals)); for Two Distances and
  Distance and Angle an arrow along the face the model measures the first
  distance on (the Reference Face, else the edge's first face; Flip the
  other: `measuredSide`, as the kernel's `measured_face`), square to the
  edge, its knob where the bevel meets that face. Chamfer's **Distance 2**
  (Two Distances): an arrow along the other face. A face's arrow points
  away from the edge along the face, at a concave edge away from the
  other face too. The **Angle** has no handle and is typed. Tests:
  `tools/ui-chamfer-handle-test.sh` (convex edges; Floating and Docked: a
  drag, the panel's value, the preview, OK, an edit from the timeline
  cancelled and accepted, Undo; two sets; Flip; a turned and moved
  component) and `tools/ui-fillet-handle-test.sh` (an L bracket's inside
  corner: in Floating and Docked Fillet's Radius and Equal Distance
  pulled into the open corner and pushed back, made and edited from the
  timeline; Two Distances and Distance and Angle with the floor as the
  Reference Face, with Flip; a convex edge's Radius; each arrow's way
  checked against the camera, the bracket's volume against the dragged
  sizes).
- **Move/Copy's points and axes in occurrences** (`solid.move` and
  `solid.move_face`, `MoveCommands.cpp`, mitcad#699; the model's side in
  commands.md, "move, align, scale"): Origin Point, Target Point, Point
  to Position's Point and Rotate's (Translate Along's) Axis picked in an
  occurrence of another component than the move's (a vertex of another
  part, or of either copy of a part placed twice) go into the
  definition with `from_occurrence`, `to_occurrence`, `point_occurrence`
  or `axis_occurrence`, the item's occurrence path (`addPickedOccurrence`);
  geometry of the move's own component, or of no occurrence, goes as
  before, without one. The move's component is a hidden text input,
  `component`: the active one when the command starts (`init`), the
  feature's own when an edit loads it (`loadTransform`, the `feature`
  query). An edit gives each point its occurrence back
  (`pickedItemOf`), so the panel shows it on the copy it was picked on
  (`Move/Copy Origin Point: 1 vertex [vertex … of F2.b0 in O2]`). No pick
  rule keeps these inputs to the active component: other components'
  bodies (ghosts too) take the picks. Tested by the last part of
  `tools/ui-component-combine-test.sh` and
  `tools/ui-windows-component-combine-test.ps1`.
- **Move Face** (`solid.move_face`, MODIFY, `MoveCommands.cpp`,
  mitcad#531; the model's side in commands.md, "Move face"): Faces (of one
  body, no meshes: `oneBody`) and the motion with Move/Copy's own inputs
  (`motionInputs`: Move Type Free Move with its arrows and rings about the
  faces' middle, Translate, Rotate, Point to Point, Point to Position with
  an arrow along each axis to the position (mitcad#838), and
  Translate Along, which Move/Copy has not: Direction, an axis, edge or
  face with an axis, and Distance with its arrow); Move/Copy keeps no
  choice from one run to the next, and neither does Move Face. Rotate's
  ring runs about the axis the preview reports the faces turn about (its
  `motion`, mitcad#854), as Move/Copy's does (`previewReported` =
  `keepRotateBasis`, `rotateBasisToKeep`, `rotateRingAxis` in
  `commands/MoveDragMotion`): about a circular edge whose circle's own
  axis points against the model's canonical one, a drag of the ring gives
  the angle that turns the faces the way the ring shows. Move Face has no
  provisional drag motion (its body is not moved rigidly: the faces
  around extend or shorten), so a drag shows the exact previews. It writes
  `move_face` with the faces' names and the move's `transform`
  (`translate_along` for Translate Along). The preview shows the moved
  body; a motion the faces around cannot follow fails the preview with
  the model's reason ("the neighbouring faces cannot follow the moved
  faces", "the faces' motion moves nothing") on the panel in red, and OK
  is refused. The note under the inputs (`CommandDef::note`) and the log
  after OK say what moves: `Panel Move Face note: Move Face: 1 face of
  Body1, translate (5, 0, 0) mm`, `Moved faces: Move Face: …`
  (`commands/MoveFaceText`: `rotate 5 deg about an edge`, `translate 3 mm
  along x`, a free move's matrix as `turn 90 deg, translate (1, 2, 3)
  mm`). Editing (the timeline's double-click, also of the `.f3d` import's
  moved faces) loads the faces, then the motion, a free move's pivot the
  faces' middle; every motion type is editable while its references show.
  The timeline cell of a Move Face selects the faces it moved
  (`MainWindow::movedFacesShape`: they keep their names), and the
  selection card names it: `MoveFace1: 2 faces of Body1, translate
  (-1.856, 0, -0.361) mm` (parameters by their values; logged `Move Face
  card: …`). A panel's red message is logged `Panel <command> error:
  <text>` (`CommandPanel::setMessage`). Tests: `tools/ui-move-face-test.sh`,
  `tools/ui-windows-move-face-test.ps1` (ctest `app.ui-windows-move-face`),
  `app/tests/move_face_text.cpp` (`app.unit`), `app/tests/drag_proxy.cpp`
  (`testMoveFaceRing`, `app.drag_proxy`: the ring's axis).
- **Combine** (`solid.combine`, `MoveCommands.cpp`): a tool body picked
  in another occurrence than the target (a body of another component, in
  an assembly context) goes in as a body in that occurrence,
  `{"body": "F2.b0", "occurrence": "O1"}` (`context` the target's
  occurrence path when it has one; commands.md, *combine*, mitcad#104,
  mitcad#105, mitcad#544): the tool is read where it was picked and
  follows when either occurrence moves. Tool Bodies keeps a pick per
  occurrence, so the same body picked where two occurrences show it (two
  screws of one part) is two tools, each cutting, joining or intersecting
  where it is; a tool in the target's own occurrence goes in as its id.
  Once a tool is picked in another occurrence, the panel lists the tools
  by name instead of their count: such a tool by its occurrence, `Part:1,
  Part:2` (`Body1 (Part:1)` when the occurrence gives two), the others by
  their bodies (`InputDef::itemsText`; logged `Combine Tool Bodies listed:
  Part:1, Part:2`). `load` gives the picks back their occurrences, also
  from the `tool_links` of earlier versions. Without Keep Tools a
  consumed tool leaves its own component (mitcad#104), so the preview has
  it in no occurrence of that component: while Tool Bodies is active its
  other occurrences are drawn as tool overlays (`withToolOverlays`,
  see-through, red for a cut), and a click there adds the same body as
  another tool (mitcad#561); OK removes the body once, as before. The
  panel's note says what else it takes away, when a consumed tool lies in
  another component that has occurrences where it is not a tool:
  `Body1 leaves Part: it is also gone from Part:2.` (logged `Panel
  Combine note: …`; none with Keep Tools), also while an existing Combine
  is edited. Tests:
  `tools/ui-component-combine-test.sh`,
  `tools/ui-windows-component-combine-test.ps1` (ctest
  `app.ui-windows-component-combine`).
- **Patterns and Mirror** of features take fillets and chamfers among the
  features (the timeline or a face they made picks them, as other
  features); the model repeats them on the copies' edges (commands.md,
  *Fillets and chamfers among the features*, mitcad#105).
- **Pattern options** (mitcad#545, `PatternCommands.cpp`,
  `commands/PatternOptions`; commands.md, *Patterns*): Rectangular
  Pattern has a Distance Type (Spacing or Extent: `distance_type`, one
  for both directions as the model keeps it) above the directions, and
  each direction a Direction Type (One Direction or Symmetric: its
  `symmetric`, written only when Symmetric). The length rows' labels
  follow the Distance Type ("Spacing", "Spacing 2" or "Extent", "Extent
  2"). A symmetric direction hides its Flip and builds its distance as
  typed: the copies lie on both sides, and only an even quantity's extra
  copy follows the distance's sign, which an edited pattern shows as
  stored. Pattern on Path has the same Distance Type and Direction Type
  rows; its Flip stays, as the definition keeps `flip` (with Symmetric it
  puts an even quantity's extra copy at the other side). Circular
  Pattern's Symmetric check centres the angle on the originals (a full
  turn is the same either way). The defaults stay one direction and
  spacing; `load` shows an imported or saved pattern's stored values.
  The added line carries the options (`Added rectangular pattern of 1
  bodies: direction 1 symmetric, quantity 3, extent 40 mm; direction 2
  one direction, quantity 3, extent 100 mm`, `Added circular pattern of 1
  bodies: symmetric, quantity 3, angle 90 deg`), and a panel logs its
  check boxes when it starts (`Panel Circular Pattern checks:
  symmetric=on`). The sketch's Rectangular Pattern has a Direction Type
  per direction too (`symmetric` [b1, b2] of `sketch.rectangular_pattern`,
  an odd quantity; Flip hidden while symmetric). Tests:
  `tools/ui-pattern-options-test.sh`, `tools/ui-windows-pattern-options-test.ps1`
  (ctest `app.ui-windows-pattern-options`), `app/tests/pattern_options.cpp`
  (`app.unit`).
- **Extrude** chooses the operation of a new extrusion until the user
  picks one (mitcad#289, `CommandDef::autoOperation`,
  `framework/CommandSessionAuto.cpp`, `framework/AutoOperationPolicy.hpp`):
  each preview asks the model for it (`auto_operation`, commands.md,
  *Extrude's automatic operation*), which tries the extrusion's tool
  against the visible bodies of its component: Join with the bodies it
  touches, Cut with those it goes into (a negative distance or Flip from
  a profile on a face), New Body when it touches none. The answer sets
  Operation and Objects (logged `Extrude: automatic Operation = Cut`) and
  their own preview follows, so the view never shows the old operation on
  the new inputs. A change of Operation, Objects or Affected components by
  the user (the panel, a pick, live MCP) turns automatic mode off for the
  rest of the command (logged `Extrude: operation chosen by hand`).
  Automatic transitions into Cut use the same conversion-time visibility
  as a deliberate first Cut; previously chosen scopes and exclusions stay
  stored through automatic Join/Cut changes. **An edit keeps the stored
  operation** (the owner, mitcad#868, 28.10.2026; it replaces mitcad#575's
  edits, which followed the geometry when the stored operation matched the
  automatic choice and so turned an edited boss's Join into a Cut when
  Flip pointed it into its arm): an edited Join stays Join, and a stored
  Cut or New Body stays, whatever Flip, Start or a distance does to the
  tool; only the user changes Operation, Objects or Affected components.
  An edit asks the model for no automatic operation and logs `Extrude:
  editing keeps the stored operation`. Tests: `app.unit`
  (auto_operation_policy), `tools/ui-auto-operation-test.sh`,
  `app.ui-windows-auto-operation`, `tools/ui-macos-edit-auto-operation-test.sh`,
  `tools/ui-wrap-test.sh`.
- **Extrude, Affected components** (an assembly cut's `cut_scope`,
  mitcad#200; `framework/CutTargetsPanel`): for a new cut, the targets the
  view hides start unchecked (mitcad#475). `def.init` takes a snapshot of
  what is hidden when the panel opens (`cutTargetVisibility`: the
  occurrences whose light bulb, or one above them, is off, the hidden
  bodies, and what Isolate leaves out); each preview's report then goes
  through `CommandDef::previewReported` (`excludeHiddenCutTargets`): a row
  seen for the first time while the scope is automatic is put in
  `excluded` when it is hidden (Isolate showing none of its bodies counts),
  or only its hidden bodies in `excluded_bodies` (the root's own bodies by
  their light bulbs); the preview is computed again with them. They are
  persistent exclusions of the stored scope: the model's automatic
  discovery never depends on visibility, so light bulbs toggled later
  change no cut. A row seen before keeps what the user chose (it can be
  checked as any other), and a hidden target joining later (a new
  intersection after an extent change) starts unchecked too. Editing a cut
  keeps its stored scope as it is. Hidden rows and bodies say `(hidden)`
  after their names. Logs: `Extrude: hidden targets start unchecked:
  excluded [...], excluded bodies [...]`, `Cut target C2@O2: eligible,
  unchecked, intersects, hidden`, `Cut target row C2@O2 at x,y` (a row's
  place once laid out). Test: `tools/ui-cut-hidden-targets-test.sh`.
  The extrusion's own component (the active one) is cut in its definition
  however many occurrences place it (mitcad#769): one row, at the
  profile's occurrence, eligible, whose text adds the model's `note`
  ("Component being edited; the cut changes all 2 occurrences."); another
  shared component stays disabled with its reason. Log: `Cut target
  C1@O1: eligible, checked, intersects, own definition` (the suffix only
  with a note). Test: `tools/ui-cut-own-shared-component-test.sh`.
- **Extrude operation conversion** (mitcad#570): editing a Join or New Body
  without a saved scope initializes Affected components when the user changes
  Operation to Cut. Visibility is sampled at that conversion, not when the
  original boss was created or when its editor opened. A modern boss stores
  a dormant scope with `initialize_on_cut: true` until that first choice;
  this distinguishes automatic Join participants from explicit local picks.
  An automatic Cut preview before Join is chosen does not count as a saved
  target choice. Changing the target checklist itself does.
  Explicit local body
  participants and existing legacy Cuts keep their local behavior. A scoped
  Cut changed to another solid operation retains its choices as dormant
  `cut_scope` data, including through save/reopen; changing back to Cut
  restores them without sampling visibility again. Merely reopening any Cut
  never initializes or resamples scope. An older unscoped non-Cut uses the
  same deliberate conversion policy; no file migration is needed. A legacy
  Cut toggled to Join and back within one editor session stays local; if
  committed as Join and later converted in another session it follows the
  non-Cut policy. Test: `app.ui-macos-cut-conversion` (primary Qt).
- **Emboss** (`solid.emboss`, `EmbossCommands.cpp`, mitcad#458): Profiles
  (the newest unused one at start; a text's letters are profiles), Face
  (one planar, cylindrical or conical face, `withAccepts`; none: the sketch
  plane), Depth, Raise or Sink (`mode`, left out for Raise) and Objects
  (`participants`; none: the face's body). The model wraps the profiles
  onto a cylindrical face, and onto a conical one by its development
  (mitcad#473; commands.md, *emboss*). Logged `Added emboss
  (Raise) of 1 profile(s) on a cylindrical face, wrapped, depth 1` (`a
  conical face, wrapped`). Test: `tools/ui-create-test.sh` (Emboss).
- **Extrude's Wrap Onto** (`wrap`, mitcad#473): a face input (0 or 1, a
  cylindrical or conical face, `withAccepts`), shown for a solid
  extrusion to one side from the profile plane; with a face the
  definition gets `wrap` (commands.md, *extrude*: wrapped as an emboss,
  out of the material by the distance, into it flipped), refuses an
  extent other than a distance or a taper ("A wrapped extrusion goes a
  distance, without a taper."), and sends no assembly cut scope. Logged
  `Extrude Wrap Onto: 1 face` and `Added extrude (Join), distance 1,
  wrapped onto a cylindrical face`. Test: `tools/ui-wrap-test.sh`.
- **Extrude of a body face** (`profile_face`, mitcad#535): Profiles also
  take one planar face of a body (solid Extrude only: the pick rule offers
  faces beside profiles, Thin Extrude sketch curves instead; `withAccepts`
  takes planar faces, `withoutMeshes`), so a body's face extrudes with no
  sketch, also in a design of an imported body alone (the command is
  enabled when the design has profiles, sketches or bodies). The face
  goes in by a pick in the command, by the selection made before it (a
  planar face selected goes to Profiles, before the newest unused
  profile, which then does not stand in), and from the view's context
  menu (Solid › Extrude, among the commands the face alone completes:
  `completes`; `offeredFor` one face, or profiles and curves). The
  definition gets `profile_face` instead of `profiles` (commands.md,
  *extrude*): Direction (one side, two sides, symmetric) with distances,
  Flip, Start at the face or an offset, Operation and Objects; the arrow
  runs along the face's outward normal (`frameOf`). A click that would
  put a face beside profiles or curves, a second face, or a profile beside
  the face is not taken (mitcad#868, `commands/ProfilePicks`: the pick
  rule's test, so the hover does not offer it either); the status bar
  says what to click instead (`refusedHint`, logged `Extrude Profiles:
  face … not taken: Profiles holds sketch profiles, …`): while Start is
  Object, click Start Object, then the face (an edited boss's start face
  clicked while Profiles was still the active input once made the panel's
  error below); otherwise set Start to Object, or clear Profiles to
  extrude the face. The face held is taken again, so that its click
  removes it. Refused on the panel: a face beside profiles from a
  selection made before the command or an edited definition ("Select one
  planar face, or profiles of one sketch, not both."), Thin Extrude of a
  face, an extent to an object or through all ("An extrusion of a face
  goes a distance."), a start object. Each side's Taper Angle goes with the face (mitcad#547; the
  model tapers a face's extrusion since mitcad#541). Wrap Onto and Affected components
  are hidden; a face's Cut works on the bodies of its own component
  (Objects). A face selected before the command that is not planar (a
  cylinder or cone goes to Wrap Onto) or a mesh body's goes to Profiles
  all the same (`preselected`), and the panel says why it is refused
  ("Extrude takes a planar face; the selected face is not planar.").
  Logged `Extrude Profiles: 1 face [face F2:end(…) of F2.b0]`, `Extrude:
  face … is no planar face, refused as the profile` and `Added extrude
  (Join), distance 5, of a face`. Tests: `tools/ui-extrude-face-test.sh`,
  `app.ui-windows-extrude-face`.
- **Extrude in a turned occurrence** (mitcad#958): the `sketch` query's
  frame is in the sketch's component's coordinates, so the arrows of
  Extrude's Distance and Start Offset and Press Pull's Distance on
  profiles (`profileNormal`) take the sketch's normal through the
  `world` placement of the occurrence the profiles are seen at
  (`commands/SketchNormal`: the `components` query's tree, nested and
  oblique occurrences too); the arrow and its drag follow the profiles as
  the view shows them. A Join whose extrusion touches none of its
  component's bodies (Objects empty: all of them) but a body of a
  component placed inside it fails with the model's named error
  (commands.md, *Joins that touch only another component's body*), which
  the panel shows: a feature joins only its own component's bodies, so
  Objects does not offer those bodies, and the message suggests New Body
  or the feature in that component. Logged `Arrow direction Extrude
  distance: 0.000,-1.000,0.000`. Tests: `app.unit` (sketch_normal),
  `tools/ui-extrude-face-test.sh`, `app.ui-windows-extrude-face`.
- **Revolve of a body face** (`profile_face`, mitcad#543): Revolve's
  Profiles also take one planar face of a body (`withAccepts` planar
  faces, `withoutMeshes`), picked or selected before the command, and the
  command is enabled in a design with bodies; the definition gets
  `profile_face` instead of `profiles` (commands.md, *revolve*). A click
  that would mix a face and profiles, or add a second face, is not taken,
  with a hint in the status bar, as Extrude's (mitcad#868). Refused on
  the panel: a face beside profiles, a mesh body's or a non-planar
  face, To Object ("A revolution of a face turns by angles or a full
  turn."), Project Axis. Editing loads the face, resolved again where the
  feature finds it (`rolledBack`, as Extrude's). Logged `Revolve
  Profiles: 1 face [face F2:side(…) of F2.b0]` and `Added revolve (New
  Body), angle -90 deg, of a face`. Test: `tools/ui-revolve-face-test.sh`.
- **Options the panels do not show, kept on edit** (mitcad#543): editing
  an extrusion keeps its own `direction` (commands.md, *extrude*: the
  imports' extrusions along a direction off the profile's normal) and a
  sweep its `frame` and fixed `binormal` (while it stays perpendicular):
  `load` stores them in the state (`kept`), `build` puts them back.
  Logged `Extrude: keeps its direction [0,3,4]` and `Sweep: keeps
  {"frame":…}`. A sweep of a body face (`profile_face`) opens no panel
  (`canEdit`). Test: `tools/ui-kept-options-test.sh`.
- **Plastic Feature** (`solid.plastic`, `PlasticCommands.cpp`,
  mitcad#459): one panel for the model's `boss`, `lip`, `groove`,
  `snap_fit`, `rest` and `plastic_rule` (commands.md, *Plastic part
  features*), chosen by Type (`featureType` `boss`, the others in
  `editsAlso`; a change of Type sets that type's starting sizes). A click
  on the face puts the point (Point X, Y, Z) where it hit (`withOnPick`);
  a lip or groove takes the rim face only. Sizes left at 0 are left out
  (`nonZero`): the model's defaults, the rule's draft and clearance. The
  Plastic Rule choice lists the timeline's `plastic_rule` features not
  rolled back (`withChoices`), the newest chosen at the start (`init`).
  Snap Fit's Catch Toward and Rest's Length Along are axis directions; a
  definition with its own `direction`, or a hook or `along` off the axes,
  is not editable. Test: `tools/ui-plastic-test.sh`.
- **Gear** (`solid.gear`, `GearCommands.cpp`, mitcad#472; registered by
  `registerGearCommands` with the main window as its `DocumentHost`, at
  the end of CREATE): Place On and the inputs it brings ([Placement by
  picked geometry](#notes-on-particular-commands) below; nothing placed
  before a pick, mitcad#656, Angle the gear's phase while it meshes with
  nothing, Bore Clearance on a shaft), Type (`kind`, a row of icon buttons
  `Panel Gear input kind=rack`), Module, Teeth, Pressure Angle (20, 14.5
  or 25 degrees), Face Width, then Mesh With (below the gear's own sizes,
  mitcad#656; a gear: a face it made or its
  feature, `withConvert`; `onPick` reads the gear's kind; it only sets
  the phase: the gear Place On places is turned by the mesh to engage,
  `mesh_with` without `angle`, and a new gear needs that pick, `Pick
  where the gear goes`; a rack and its pinion are placed by the mesh
  alone, along a rack by Position; Toward, `mesh_with.toward`, shows only
  for a gear of an earlier version written beside its mate toward a point
  or an axis, `toward_kept`), and as the kind takes them Helical Teeth
  (rings and racks), Helix Angle, Left-Handed, Gap, Rim Thickness,
  Height, Length (`withAuto`: empty, the pitch line), Shaft Angle, Mate
  Teeth, Make the Pair, then Backlash, Profile Shift, Bore (None, Plain,
  Thread with Thread Size), Keyway, Hub with its diameter (`withAuto`:
  the model's `hub_diameter`) and length, and the operation (commands.md,
  *Gears*). Off a face its Angle is offered only once Align to has an
  edge or a flat face, keyway or not. No tooltips that state rules. Values left at 0 are left out
  (`nonZero`). `CommandDef::note` (the framework's line under the inputs
  after a successful preview, before the warnings; logged `Panel Gear
  note: ...`) shows the model's `gear_values` of the definition: `Pitch
  diameter 40 mm, outside 44 mm, root 35 mm`, a rack's pitch line, the
  centre distance of a mesh, `undercut`. After OK (`describe`, which
  reads the model after it) a gear that meshes with another is linked to
  it when the model's `gear_link` gives an `add_motion_link` (both gears'
  components on joints, not linked yet), run when the model is idle;
  logged `Gear F4 linked to its mate's joint` or `Gear F4 not linked:
  <reason>`. A definition whose placement the panel cannot show, or with
  a thread of another standard, is not editable. Logged `Added gear
  (Spur), 20 teeth, module 2`. On a shaft's round face or edge Bore
  Diameter hides (a Plain bore, or None, is the shaft's) and the note
  adds `bore 8.2 mm (shaft 8 mm)`. A rack and a bevel pair keep their
  `plane` and `center` (the model takes no `at` for them): Place On on a
  face or a plane gives them (`planeCenterOf`); a rack runs along
  Reference 1, its `angle` Reference 1's direction on the face
  (`referenceAngle`) plus Angle. A gear written with
  `mesh_with.angle` opens placed where it is (`builtFrame`,
  `placeAtFrame`: on its mate's plane, or the origin plane square to its
  axis, measured from the origin axes) and meshing its mate; it is saved
  as it was (`keepEarlierForm`) until its placement or its mate changes,
  then with `at` (a rack beside its pinion keeps its angle until Toward is
  picked). Tests:
  `tools/ui-gear-test.sh` and `app.ui-windows-gear`
  (`tools/ui-windows-gear-test.ps1`: a spur gear, then a rack meshing it,
  then a spur gear on a shaft's top edge with an offset; the Linux test
  also a Gear Pair), `tools/ui-place-test.sh`.
- **Pulley** (`solid.pulley`, `PulleyCommands.cpp`, mitcad#562; registered
  by `registerPulleyCommands` after Gear, at the end of CREATE): Place On
  and the inputs it brings ([Placement by picked
  geometry](#notes-on-particular-commands) below; nothing placed before
  a pick, mitcad#656, Angle and Align with a keyway or set screws, which
  lie in its direction, Bore Clearance on a shaft), Check Belt right under
  Place On (mitcad#656: another pulley, a face it made or its feature,
  `withConvert`; kept with the pulley as `belt_check: {"pulley": "F3"}`,
  which places nothing) with Belts (`catalogue`: Library, the parts
  library's closed belts, or Whole Teeth, any whole number of teeth; not
  stored), Type (`kind`, a row of icon
  buttons `Panel Pulley input kind=idler`: Toothed, Idler, Toothed
  Idler), then Belt Profile (GT2 2 mm, GT2 3 mm, HTD 3M,
  HTD 5M, T5, XL) and Teeth for the toothed kinds or Diameter for an
  idler, Belt Width, Belt Clearance (`clearance`, always given), Pitch Correction, Flanges
  (None, One, Both; None leaves `flanges` out) with Flange Diameter
  (`withAuto`: the model's `flange_diameter`) and Thickness, Bore (None,
  Plain, Thread with Thread Size), Keyway, and for a toothed pulley only
  Hub with its diameter (`withAuto`: the model's `hub_diameter`) and
  length and Set Screws with their count, size (a thread designation)
  and position (`withAuto`: the middle of the hub), and the operation
  (commands.md, *Pulleys*). No tooltips that state rules. Values left at 0 are left out (`nonZero`);
  every value is an expression the model keeps as the feature's
  parameter. `CommandDef::note` (logged `Panel Pulley note: ...`) shows
  the model's `pulley_values` of the definition: `Pitch diameter 12.7324
  mm, outside 12.2244 mm`, an idler's diameter, `bore 8.2 mm (shaft 8
  mm)` on a shaft, or the model's `error`. Check Belt's rows under it
  (`withRowNote`) give the belt round both pulleys as they are now, one
  per line, values to 0.1 mm: the verdict in a word, bold, from the
  model's `fit` (`Fits`, `Needs a tensioner`, `Belt too short`), `Belt
  length 200 mm (100 teeth), 10 teeth in mesh`, `Nearest belt GT2 200
  mm, 6 mm wide` (`beltText`: the profile, the length and the library
  belt's width, else its teeth), `Travel 1 mm apart` (`6 mm closer`:
  the axes must move so far for that belt), then the model's `warning`
  when the pulleys are not in one plane (the
  model's `belt_drive` for the two placements, pinned to the two tooth
  counts with any teeth in mesh, the other pulley's placement as
  `atOfFeature` gives it; a pulley on a shaft in another component's
  occurrence by its `seat` with the link, mitcad#754, where `atOf` gives
  no `at`; for an edited pulley where that does not apply,
  `belt_check` of the two as built; Whole Teeth asks with
  `whole_teeth`); logged `Pulley belt check with F1: ...` (the verdict
  last, in lower case) and `Panel Pulley
  check_belt note: ...`. The view draws the belt's pitch line round both
  pitch circles (`viewLines`, logged `Lines Pulley: 1`). A definition with a kind, belt profile or flange side the
  panel does not list, a placement it cannot show, a thread of another
  standard, or a hub or set screws on an idler (or set screws without a
  hub) is not editable. Logged `Added pulley (Toothed), GT2 2 mm, 20
  teeth` or `Added pulley (Idler), diameter 20 mm`. On a shaft's round
  face or edge Bore Diameter hides (a Plain bore, or None, is the shaft's;
  a Keyway goes into it). A pulley written with `belt_with` opens placed
  where it is (`builtFrame`, `placeAtFrame`) with the other pulley as its
  Check Belt, and is saved as it was until its placement or Check Belt
  changes. Tests: `tools/ui-pulley-test.sh` and
  `app.ui-windows-pulley` (`tools/ui-windows-pulley-test.ps1`): a GT2
  pulley on XY, its teeth and flanges changed in the preview, edited,
  then a second pulley 80 mm along X typed in Distance 1's value box in
  the view with the belt round both, its pitch line and the nearest
  catalogue belt, the check kept after OK, then a pulley
  on a shaft's top edge with an offset and a bore clearance (the Linux
  test also a Belt Drive), then Check Belt with a pulley on a shaft in a
  translated occurrence (its Offset handle on that shaft, the belt for
  the real distance); `tools/ui-place-test.sh`.
- **Bearing** (`solid.bearing`, `BearingCommands.cpp`, mitcad#577;
  registered by `registerBearingCommands` after Pulley, in Machine
  Elements): a printable bearing (commands.md, *Printable bearings*).
  Place On and the inputs it brings ([Placement by picked
  geometry](#notes-on-particular-commands) below; nothing placed before
  a pick, mitcad#656; not with Groove Into), Element (Dumbbell, Roller, Ball,
  Airsoft), Groove Into (a round face of a body, a cylinder: the model's
  `groove`, with Offset; the bearing then takes no placement and is made
  as new bodies), Fit Clearance (on a shaft or in a hole), Size (the
  model's `bearing_sizes`, read once, `608 (8 x 22 x 7)`, then Custom
  with Bore, Outside Diameter and Width), Flange with its diameter and
  thickness, then Printing: Radial Clearance, Axial Clearance, Chamfer,
  Smallest Wall and Flank Angle (always given), Element Gap, Elements and
  Element Diameter (`withAuto`: empty, the model's default, the most that
  fit, the largest that leaves the walls, shown from its
  `bearing_values`), and Operation (New Component, the
  default, or New Body). Every value is an expression the model keeps as
  the feature's parameter. `CommandDef::note` (logged `Panel Bearing
  note: ...`) shows the model's `bearing_values` of the definition:
  `Pitch diameter 15 mm, 10 elements of 3.8 mm, walls 1.2 mm and 1.2 mm,
  least gap 0.2828 mm` (airsoft balls also `buy 8 x Airsoft ball 6 mm`),
  or the model's `error` (a printability check), capitalized. A
  definition with a size the panel does not list or a placement or face
  it cannot show is not editable. Logged `Added bearing (Dumbbell), 608`,
  `Added bearing (Roller), 10 mm x 30 mm x 9 mm`, `Added bearing
  (Dumbbell), 608, groove into a face`, `Added bearing (Dumbbell), 608, on
  a shaft or in a hole`. On a shaft or in a hole (mitcad#578, Place On a
  round face or edge): Fit Clearance (0.5 mm); the pick also sets Size to
  the one that fits it (`shaftSeat`: the model's `place_frame` seat, or
  `seat_frame` for another component's shaft, then `bearing_sizes` by the
  bore on a shaft, by the outside diameter in a hole, the nearest), which
  the user may change; the definition always has the size, so the
  component is named after it. The note adds `on a 8 mm shaft` or `in a
  22 mm hole`, and `no standard size fits it` when none does. Tests:
  `tools/ui-bearing-test.sh` and `app.ui-windows-bearing`
  (`tools/ui-windows-bearing-test.ps1`): on XY with too many elements and
  an edit, Groove Into a shaft, on a cylinder's top edge with an offset,
  in a hole's top rim.
- **Placement by picked geometry** (`PlaceInputs.hpp`, the plane geometry
  in `PlaceGeometry.hpp`, mitcad#656; commands.md, *Placement by picked
  geometry*): the inputs every part placed by a pick shares (Pulley,
  Gear, Bearing, Coil, Box, Cylinder, Sphere, Torus, and Hole's On Face),
  `placeInputs(PlacePanel)`. No typed coordinates: where the part goes
  comes from what is picked, distances are dimensions to lines in sight.
  - **Place On** (`at`, `withPlaces`, keeps a face picked again): a
    shaft's cylindrical face or circular edge, a hole's wall or rim, a
    construction or origin axis, a sketch line, circle or arc give
    `at.axis` (a click on a face keeps where it hit, `near`: the end the
    offset is measured from); a sketch point, a vertex, a construction
    point give `at.point` (a vertex or construction point on the origin
    plane facing the viewer most, `at.plane`, shown in a **Plane** row,
    `at_on_plane`, where another plane or planar face can be picked); a
    click on a planar face
    or a plane gives `at.face`: the point snaps to the face's vertices,
    circle centres and edge middles within 10 pixels, and is measured
    from the two nearest straight edges of a body's face (crossing at a
    degree or more; the origin axes along the face when it has no two),
    from an origin plane's own axes (`offset1` along its x, `offset2`
    along its y, no edges), or from the origin axes along a construction
    plane (with `near`). Each reference's positive side is the face's
    middle's on a body face (as the model's `place_frame` confirms with a
    point 1 mm from both) and the click's on a plane. A straight edge, a
    sketch line or an axis along the face clicked while Place On takes
    picks becomes the reference it runs with most nearly, the point kept
    where it is (`Place <command>: edge ... is a reference now`); Place
    On keeps the face.
  - **Reference 1**, **Distance 1**, **Reference 2**, **Distance 2**
    (`at_edge1`, `at_offset1`, `at_edge2`, `at_offset2`; on a face): the
    lines and the point's distances from them, rounded for a click to
    the step a drag takes at that zoom (1, 2 or 5 times a power of ten,
    about three pixels; exact for a snap); each distance a dimension in the view with
    its value box (`Manipulator::valueBox`), dragged or edited there or
    typed in the panel. Picking another line in a Reference row makes it
    that reference. Each reference and its distance are one line of the
    panel (`withBesidePrevious`): the colour swatch, the line picked
    (`Edge`, `X axis`, `Sketch line`) and the distance.
  - **More** (`at_more`, a disclosure): Offset, Flip, Angle and Align to
    Edge folded away unless they are in use (not 0, Flip on, Align picked
    by the user), so that the part's own sizes are in sight; their handles
    stay in the view (`withHandleWhenHidden`), their value boxes only when
    not 0 or under the mouse (`boxAtZero = false`).
  - **Offset** and **Flip** (`at_offset`, `at_flip` beside it;
    `PlacePanel::along`, not Hole's, which has its own Flip on the
    Placement line): along the axis or the normal, an arrow with its value
    box from where the part starts without it.
  - **Angle** and **Align to** (`at_angle`, `at_align`; an edge or a
    face;
    while `PlacePanel::turns` says the turn matters: a box, a gear's
    phase, a pulley's keyway and set screws): a ring about the part's
    axis with its value box, its zero line drawn along the part's x axis
    and an arc to the angle; Align is the line (or a planar face's
    normal) the x axis runs along. On a face an empty Align is always
    Reference 1 (`atOf` writes Reference 1 as `align`; there is no
    hidden axis and nothing to clear: clearing a picked edge returns to
    Reference 1), and its empty button says `Reference 1` in Reference
    1's green (`InputDef::emptyShows`); on an origin plane, whose
    distances are from the origin axes, it is the plane's own x, the X
    axis, Reference 2, and the button says so. Off a face (a shaft, an
    axis, a point) Angle and its ring are offered only once Align has an
    edge or a face; a pulley's keyway or first set screw, a gear's
    keyway then lie at the angle from it. No angle is measured from the
    projected +X. Reference 1 and 2 have fixed colours (green, purple)
    and an edge picked in Align one outside the panel's palette (pink),
    so that none looks like another (`InputDef::color`). Picking
    another line for a reference never turns the part: on an origin
    plane, where the empty Align is the plane's X axis, the X axis is
    pinned in Align as soon as the distances are measured from picked
    lines (`place::keepsPlaneX`), and a definition loaded so without an
    `align` gets it the same way. A line's direction is taken into the
    face: `atOf` writes `align_side` (Reference 1's side toward the
    point), so that a part placed from the face's far edge grows into
    the face (`place::intoFace`); a definition written without it keeps
    its line's direction (`at_no_align_side`) until Place On or Align is
    picked again. A face placement on a body written without `align`
    (optional in `at`, as the model's commands may write it) is saved
    without one, not with Reference 1 (`at_no_align`), until Place On, a
    Reference or Align is picked again, so that an edit that changes
    only a size leaves the part as it was (mitcad#761;
    `ui-place-test.sh` part 5). Tests: `place_geometry` in app.unit. Align
    takes any straight edge, an axis, a sketch line or a planar face
    (the x axis along its normal, put square to the part's axis). A
    shaft picked in Place On whose body has a flat (a planar face along
    the axis, nearer it than the shaft's radius, `shaftFlat`) is aligned
    to it at once (logged `Place Pulley: aligned to the flat face ...`),
    so that a set screw turns without an edge drawn first; that align
    goes with the shaft when another is picked.
  - **Bore Clearance** (`PlacePanel::clearance`; on a shaft or in a hole).
  `initPlace` in `init`: where the feature's component is seen, a pick
  made before the command placed as a click in its middle; nothing is
  placed without a pick (mitcad#656: no part starts on XY at the origin,
  where Enter would make a part floating there). `addPlace` writes `at` (a
  shaft of another component, a BUILD shaft or a library part, `seat`
  with its `link` `{"source": <the pick's occurrence>, "target": <where
  the feature's component is seen>}`) and `bore_clearance`; `loadPlace`
  fills the inputs from `at`, `seat`, or an earlier `plane` and `center`
  (a box's `corner`): that plane with distances from the origin axes, an
  angle beside it as Angle (a coordinate given as an expression, a
  parameter of the primitive since mitcad#689, places the point by its
  value, and its distance shows the expression, negated when the
  distance runs the other way); what the panel does not show of an `at` (a
  bearing's angle, a hole's offset) is kept as it was (`at_extra`). An
  earlier form (`plane` and `center`, a `seat` of the feature's own
  component, a gear's `mesh_with.angle`, a pulley's `belt_with`, a hole's
  one position on a face) is saved as it was while the inputs still make
  the `at` they made of it on load (`keepEarlierForm`, `keptEarlierForm`;
  `at_legacy`): a relation is not frozen into numbers unless the
  placement is changed.
  `placeShowable` in `canEdit`; `placeFrame` (the model's `place_frame`)
  for the parts' own handles; `shaftNote` the note's `bore 8.2 mm (shaft
  8 mm)`; `builtFrame` and `placeAtFrame` where an earlier `belt_with` or
  `mesh_with.angle` put a part; `atOfFeature` an `at` for a part made
  before (Check Belt). Logged `Place <command>: face plane xy, 100 mm
  from axis y, 0 mm from axis x (point 100, 0, 0)`, `Place <command>:
  axis edge E{...} of F1.b0`, `Place <command>: point ...`, `Place
  <command>: none`.
- **Gear Pair** and **Belt Drive** (`solid.gear_pair`,
  `solid.belt_drive`, `DriveCommands.cpp`, mitcad#656; registered beside
  Gear and Pulley, `Gear | Gear Pair | Pulley | Belt Drive | Bearing` in
  Machine Elements): First Axis and Second Axis (a shaft's round face or
  edge, a hole's, an axis, a sketch line, circle or arc; `withPlaces`),
  Ratio (typed `1 : 2`, the first part's teeth to the second's, or the
  second number alone), Belt Drive's Belt Profile, the list of what fits
  right after them, then More (`drive_more`): each axis's Offset
  (`first_offset`, `second_offset`: an arrow with its value box along
  the axis, shown once the axis is picked, so that both parts lie in one
  plane; folded unless not 0, its handle in the view all the same) and
  Gear Pair's Type (Spur or Helical, Spur, Helical), Pressure Angle and
  Modules (All Standard, or From To with the two modules), folded unless
  one of them is changed; each of them asks the model again (`gear_pair`,
  `belt_drive`), logged `Gear Pair candidates: 12, centre distance 60 mm`
  or `... none: <why>`; the row chosen before stays chosen while the new
  list has the same pair (the same teeth, module, fit and kind: an Offset
  changes none of them), logged `Gear Pair candidate kept: row 2`, else
  the first row is (mitcad#761). The candidates are rows under columns
  (`withChoiceColumns`, the chosen row selected and the list taking the
  arrow keys): Module, Teeth (`20:40`, the warning icon when a gear is
  undercut), Ratio (how far the pair is off the ratio asked, only where
  it is; the column left out when no row has one), Centre distance (how
  the pair meets it: `exact`, `exact, corrected` for a pair the model
  shifted only against undercut, `shifted`, `helix 12°`); the list takes
  the panel's whole width under its name, Centre distance's heading
  with the axes' distance in it (`Centre distance 60 mm`,
  `withChoiceColumns(headings, now)`, `InputDef::choiceColumnsFor`);
  Teeth, Ratio (the model's `ratio_deviation`, only where a row has one),
  Belt (`GT2 200 mm`), Travel (`0.4 mm apart`, `0.3 mm closer`, to 0.1
  mm), with the chosen
  row's verdict, belt and travel under the list as Check Belt gives them
  (`withRowNote`) and its belt's pitch line drawn (`viewLines`); the best first, chosen at the start. Gear Pair's note adds the
  chosen pair's `Profile shift +0.10 / -0.10` and `A gear is undercut`
  (the centre distance is in the list's heading only); Belt Drive's says
  in Check Belt's words `Belt length 200 mm (100 teeth), 10 teeth in
  mesh` (its centre distance in the Travel heading, `Travel from 60 mm`;
  the verdict and the belt's width under the list, the belt and its
  travel in the row, logged `Belt Drive belt: GT2 200 mm, 6 mm wide,
  travel 0 mm, fits`);
  both the model's `warning` when the two
  placements are not in one plane (an axial offset of 0.0005 mm or
  more). The row
  chosen is previewed (`sketchCommand` with `previewAsNew`: `add_gear_pair`
  / `add_belt_drive` applied, the bodies they make kept and taken back at
  once, then drawn as a preview's new bodies, see-through blue and not in
  the browser, logged `Preview Gear Pair: 2 new bodies shown as a
  preview`; the first preview framed in the view's free area, zoomed out
  when it does not fit, `View fitted: the parts clear of the panels`) and
  OK applies it again as one undo step; each part is on its axis (`at.axis`) with the candidate's
  fields, Face Width or Belt Width, Bore Clearance on a shaft, and the
  operation (New Component or New Body). A shaft picked in another
  component's occurrence (mitcad#754) is a `seat` with its occurrence
  link (`occurrenceLinkOf`, as Gear's and Pulley's Place On write it), in
  the queries (`{"seat": ...}`), the parts, the Offset arrows and the
  belt's pitch line, so the distance is the one the view shows. Gear
  Pair's second gear meshes with the first and goes on a shaft of the
  active component, as Gear has it: a second axis in another component's
  occurrence lists nothing and says why (logged `Gear Pair candidates:
  none: The second gear meshes with the first, so it goes on a shaft of
  the active component: pick one there, or this shaft as the First
  Axis`). The model's warning when the second axis points the opposite
  way from the first (mitcad#749) shows under the list as the axial one
  does. Belt Drive's note: `Belt
  length 200 mm (100 teeth), 10 teeth in mesh`. Gears made as components have their joints linked after OK
  (`linkLater`). Logged `Added gear pair, module 2, 20 and 40 teeth
  (exact)`, `Added belt drive (gt2_2mm), 20 and 40 teeth`. Tests:
  `tools/ui-gear-test.sh`, `tools/ui-pulley-test.sh` and
  `app.ui-windows-drives` (`tools/ui-windows-drives-test.ps1`).
- **Expression fields' parameters** (mitcad#567, mitcad#568;
  `framework/ParameterField.*` for the list, the fx button and Create
  User Parameter, `framework/ParameterSuggest.*` for the text rules). One
  implementation serves every field that takes expressions: a panel's
  value inputs (`CommandPanel::createRow`, its own parameter set again in
  `refreshRow`), the sketch dimension editor and the sketch tools' value
  fields (`SketchController::openValueEditor`, `setFields`) and the
  parameter table's expression editor (`ParametersDialog::edit`; ordinary
  text inputs have none). `ParameterField::attach(edit, options)` gives the
  field an fx button (a trailing action), a list and a right-click menu;
  options say the field's kind (length, angle, number), its own parameter,
  its sketch, its log label, whether it takes `name = …` and whether it
  offers Create User Parameter. The window gives the parameters and the
  command (`setParameterSource`, MainWindow's constructor: the
  `parameters` query, `DocumentHost::runParameterCommand`, the document's
  length unit); a field asks the source of its window (or of the window
  its dialog belongs to) each time the list opens, so the list follows
  additions, renames and the document shown.
  - **Typing:** the identifier at the caret opens the list
    (`nameFragmentAt`): not a number, not a unit (a word right after a
    number, a name or `)`), not a function (before `(`), not the name of a
    leading `name =`. Matches are case-insensitive: names that start with
    the text, then names that contain it; user parameters, then model
    parameters (with their reference parameters, "driven"), each with its
    value in its unit and its comment, in natural order. The field's own
    parameter is left out. A choice that uses it (a cycle), a reference of
    the field's own sketch, or one of another kind where it would be the
    whole expression (an angle as a length; a unitless parameter goes
    anywhere) is shown dimmed and says why instead of being put in. A
    choice replaces the whole identifier the caret is in, never the rest
    of the expression, with the name (not its value), with the field's
    undo kept, and the field reacts as to typing (`textEdited`). Exactly
    the one name typed, or a unit or function name, opens no list.
  - **Keys:** while the list is open, Up, Down, Page Up and Page Down move
    in it, Enter (or a click) takes the highlighted row and Esc closes the
    list; the field accepts their shortcut override, so the window's Enter
    (OK, the sketch's accept) and Esc (Cancel) do not fire and a table
    cell's editor stays open. Tab closes the list and moves on. With the
    list closed the keys are the field's. The list is a child of the
    field's window and takes no focus: the field keeps the keyboard (a
    panel's field is not committed, a cell's editor stays open).
  - **fx:** the browser lists every parameter; what is typed then
    searches names and comments without changing the field; a choice goes
    in where the caret was when it opened (in place of a selection, so a
    sketch value not edited yet is replaced). fx again closes it.
  - **`name = …` (mitcad#568):** in panels and the dimension editor the
    list offers `Name this value: width = …` for an unknown name typed
    first (the field then reads `width = `) and `Name this value` in the
    browser (the own parameter's name before the value, selected to type
    over). This is the existing rename (mitcad#231, commands.md *Values*),
    not a user parameter.
  - **Create User Parameter... (mitcad#568):** offered for an unknown,
    valid name typed (and always in the browser and the right-click
    menu), not in the table's cell editor (a dialog would end the cell's
    editing). The editor proposes the name, the field's kind's unit (the
    document's length unit, deg, none) and says what the field is. It
    refuses an empty, invalid or reserved name, a taken one (with a "Use
    <name>" button: never overwritten), a unit of another kind when the
    name would be the field's whole expression, and what the model refuses
    (a cycle, an unknown name, a unit mismatch), all in the editor, which
    stays open; Cancel changes nothing. Created, the reference goes in
    where the name was typed and the field has the keyboard again. The
    command runs as Change Parameters runs it (mitcad#564): in a sketch an
    undo step of the sketch (merged into an existing sketch's `Edit
    <sketch>` at Finish Sketch), while a panel is open outside its undo
    range (Cancel keeps the parameter; OK's step comes after `Add
    Parameter <name>`).
  Tests: `tools/ui-parameter-suggest-test.sh`,
  `tools/ui-parameter-create-test.sh`, the unit test
  `app/tests/parameter_suggest.cpp`.
- **Print Analysis** (`inspect.print_analysis`, INSPECT after Section
  Analysis, `InspectCommands.cpp` and `PrintAnalysis.cpp`, mitcad#620;
  commands.md, *Print analysis*): where a part prints badly and what it
  takes, before it goes to a slicer. It changes no body, feature,
  timeline or export; 3D Print stays as it is. An inspection with
  `keepsSection` and `build`, as Section Analysis:
  - **Bodies** (`SelectKind::Body`, any number; solids and mesh bodies
    only, as 3D Print's `withAccepts`): what the selection stands for (a
    face, edge or vertex its body, an occurrence the bodies shown in it,
    `printStartBodies`), else every shown body; each body once.
  - **Bed face** (optional, one planar face of a chosen body): the face
    that lies on the bed, its outward normal pointing down. Without one
    the design's -Z is down (`down: [0, 0, -1]`) and the bed is at each
    body's lowest point. The bodies do not move.
  - **Overhangs** (on) with **Angle** (45 deg, from the vertical);
    **Thin walls** (on) with **Minimum wall** (0.8 mm); **Estimate** (on)
    with **Material** (the bodies' own, else PLA; or a physical
    material), **Line width** 0.4 mm, **Walls** 2, **Layer height**
    0.2 mm, **Top/bottom layers** 4, **Infill %** 15, **Filament
    diameter** 1.75 mm, **Price €/kg** (a text: empty, no cost). The
    rows of a part switched off are hidden and not checked.
  - The result area: per body (and in total for several) the overhang
    area in mm² with its share of the area, the thinnest wall as a link
    that frames the view around it, the thin regions; then the estimate
    `41.5 g · 13.9 m · 1.04 €` with the material, its density and the
    filament's volume, labelled as an estimate. Overhangs and walls are
    `CommandHost::jobQuery` jobs on the model's worker (the progress
    dialog's Cancel stops them; the panel then shows the cancel as its
    error), the estimate a plain query; in the native macOS window the
    whole inspection already runs on the worker.
  - A new panel starts with the settings group `print_analysis`
    (`PrintAnalysisSettings`); OK keeps the analysis (`add_analysis`, the
    model names it `Print1`, `Print2`, …; `edit_analysis` for one edited)
    and saves its numbers in the group. Each analysis keeps its own in
    its `def` (`load` fills the panel from the `analyses` entry).
  - **In the view** while the panel is open or the analysis is shown
    (`OcctViewer::setPrintOverlay`, `view/PrintOverlay.cpp`): the
    overhang triangles in the theme's warning colour, the thin-wall
    triangles in its error colour, flat and over the bodies' own faces
    (raised 0.05 and 0.08 mm along their normals, with a polygon offset,
    as the analysis' mesh and the view's differ on curved faces), not
    pickable; a marker and a label `Thinnest 0.50 mm` at the thinnest
    point, over everything; a legend in the lower left corner with the
    analysis' name, the colours, the angle and the minimum wall
    (`PrintOverlayLabels`: cards painted once into images at the
    screen's pixel ratio and drawn by the view itself in its overlay
    layers, the marker's where its point lies, facing the screen at its
    size, the legend's kept clear of the cards over the view; no widget
    over the view, which Qt would paint again at every frame of an
    orbit, mitcad#768). One analysis is shown at a time; a shown Print
    Analysis is computed again (`MainWindow::followPrintAnalysis`, after
    the refresh) when it, the design or its bodies' shapes change, never
    for a camera move; the model keeps the results by the bodies'
    content and the settings, so showing it again costs only the answer.
  Tests: `tools/ui-print-analysis-test.sh` (panel, an orbit with the
  panel open and one with Print1 shown that compute nothing again,
  set no overlay and paint no widget over the view, Print1 kept, light
  bulb, edit with a bed face and a minimum wall, save with the STL
  export unchanged, delete and undo), the unit test
  `app/tests/print_analysis.cpp`; the model's tests in
  `core/model` (commands.md).

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

## Live MCP

An agent connected through `mitcad-cli mcp --connect` ([docs/mcp.md](../docs/mcp.md),
mitcad#265) runs these commands by their ids with `ui_command`
(`MainWindowMcpUi.cpp`): a command with a panel opens as from the menu, its
inputs are given by their keys through the panel's own actions
(`CommandSession::scriptInputs`: what typing, choosing and picking do), its
preview runs, then OK, Cancel or the panel stays open for the user; an
action runs as its shortcut would. `edit` opens a feature as a
double-click in the timeline does. The availability rules are the user's
(`isAvailable`, read-only windows); commands that open a file dialog or
another window (`file.*`, `insert.*`, `tools.*`, `help.*`, 3D Print, DXF
import and export, Parameters, the render environment and grid settings)
are refused, and any other modal dialog is closed as cancelled
(`mcp::ModalGuard`). Their steps are labelled "(agent)" in the undo text,
and the agent indicator beside the project indicator (`mcp/AgentIndicator`,
mitcad#710) names the connected assistants ("Claude Code connected" by
`mcp::assistantDisplayName`, else the client's own label; "2 assistants
connected"), says "No assistant" while the window accepts them and
"Assistants unavailable" when its listener did not start; the instance id
and port are in its tooltip. With none connected a click opens Connect AI
Assistant (`help.connect_assistant`), and its arrow or a right click the
menu; otherwise a click opens the menu (InstantPopup): the reason first
when unavailable, else each client (assistant name, own label and
version, connected since) and Disconnect *assistant* (Disconnect All,
`disconnectAgent`), then Connect AI Assistant…, Copy Command for This
Window (`copyConnectCommand`, only while several windows listen) and Folders
for Designs… (`tools.preferences.agents`). The log keeps the clients' own
labels ("Agent indicator: connected: <label>").

Save As and Open have tools of their own instead (mitcad#402):
`document_save` with a `path` and `document_open`, into and from the
folders for designs only (`mcp/AgentFolders`: Preferences ›
AI Assistants, the setting `mcp/folders`, or `--mcp-folder DIR` for a run; none by
default), without dialogs; Open replaces the document as File › Open does
and refuses unsaved changes unless the agent discards them. `ui_camera`
sets the view's camera or frames a box, the shown bodies or the selection
at once (`OcctViewer::frameBox`, mitcad#405). A `view.*` command's reply,
`ui_select`'s reveal and `ui_camera` end the view's animation and a pending
fit before they answer (`OcctViewer::settleCamera`): an animation of
0.4 s otherwise left the camera where it started in the reply and in the
next screenshot.

## Logs that UI tests read

| Line | When |
|---|---|
| `View area x y w h` | the 3D view's place in the window (`ui_view_click`) |
| `Ribbon SOLID/CREATE at x,y`, `Ribbon tab BUILD at x,y` | group menus and the workspace tabs (wherever the tab bar is: the ribbon's, the title bar's), when the ribbon or its tab bar is shown, moves or is resized (`PlaceWatcher`, when the places changed), and when another tab is shown (`ui_click_logged`) |
| `Window layout: window 1280x800, minimum 484x315; menu 6,0 1268x22; central 252,126 716x593; QStatusBar 6,771 1268x29; browserDock 6,126 240x593; commandDock 974,126 300x593; ribbonToolbar 6,22 1268x104; timelineDock 6,725 1268x46` | the main window's size and minimum size and the geometry of its menu bar, central widget (the 3D view) and visible docks, toolbars and status bar (by object name, else class), once the window shows and again whenever its size changes after that (mitcad#560: the docks' content never makes the window grow; `tools/ui-docked-height-test.sh`, `app.ui-windows-docked-height`; not with the native UI); `tools/ui-layout.sh` prints it and `tools/check-all.sh` reruns every UI test when it changes (mitcad#412) |
| `Ribbon split SKETCH/CREATE/Circles at x,y`, `Ribbon split SKETCH/CREATE/Circles arrow at x,y` | a subgroup's split button: the part that starts its default, the arrow that opens the submenu (mitcad#222; logged with the group menus) |
| `Ribbon menu SKETCH/CREATE: Line \| Rectangles > [2-Point Rectangle \| …] \| … \| Sketch Dimension`, `Ribbon menu SKETCH/CREATE/Circles: Center Diameter Circle \| …` | a group menu or a subgroup's submenu as it opens (`-` a separator, a submenu as `Name > [its entries]`; the SOLID CREATE menu: `Create Sketch \| - \| Extrude \| … \| - \| Pattern > [Rectangular Pattern \| …] \| Mirror \| - \| Machine Elements > [Gear | Pulley | Bearing]`, mitcad#579)\| Pulley]`, mitcad#579) |
| `Panel <command> input <id> at x,y [(hidden)]`, `Panel <command> OK at x,y` | a panel opened; again whenever its places move (mitcad#528: the panel, a row's field or a widget it is in moved or was resized, a tile's picture read, the dock laid out again: 50 ms after it settles, a timer the sync waits for, only when a place differs from those logged last), so the last line logged is where the input is; the part picker's places likewise when its window moves or is resized; the in-process test driver's `click-logged` (macOS) waits for these timers (`TestSync::settling`, up to 5 s) before it reads a place |
| `Panel <command> view y 160 to 668, scrolled 0 of 297, bar at 1015,414` | with a panel's places: the part of the window its page shows, how far it is scrolled, and its scroll bar while it scrolls (a long panel in a low window); the places are logged again when it scrolls (`Ui-PanelReveal` in `tools/ui-windows-lib.ps1`) |
| `Datum xy at x,y`, `Sketch entity F1/c4 at x,y` | an input that takes them became active; the datum places are tried again with each input (a placement input puts the planes behind the model and the axes, mitcad#656), and an origin plane covered along its diagonals by an axis gets a place off them |
| `Datum F4 at x,y`, `Datum F4 in O1 at x,y` | with `MITCAD_LOG_PICKS`: where a click picks a shown construction feature, a component's where its occurrence places it (mitcad#432) |
| `View highlight places: face F2.b0/F2:side(c1) in O1 at 60.0,0.0,10.0; plane F4 in O1 at …` (`none`) | with `MITCAD_LOG_PICKS`: the highlighted faces, edges, vertices and datums changed: each with its occurrence and the middle of its bounding box in the design (mitcad#432: a loaded face of a placed component drawn where the view shows it) |
| `Command <name> started`, `Editing F2 with <name>`, `Editing analysis Section1 with <name>`, `Command <name> cancelled` | |
| `<command> <input>: 2 edges [edge E{…} of F2.b0; …]` | a selection input changed |
| `<command> <input> listed: Part:1, Part:2` | the picks the panel lists by name instead of their count changed (`InputDef::itemsText`: Combine's tools in occurrences, mitcad#544) |
| `<command> <input>: d1*2 = 120 mm`, `… is invalid: <reason>` | a value was evaluated |
| `Extrude: automatic Operation = Join` (with `Extrude Local bodies: 1 body [body F2.b0]`), `Extrude: operation chosen by hand` | a new Extrude's operation and objects the model chose from its preview, and the user's own choice of either, which stays (mitcad#289) |
| `Extrude: editing keeps the stored operation` | an edited Extrude follows no automatic choice: its stored operation stays until the user changes it (mitcad#868) |
| `Sweep: the profile lies inside an open path; Distance 2 shown` (`at the start of`, `at the end of`; `a closed`; `hidden`) | a Sweep preview found the profile somewhere else on its path than the last one (mitcad#795) |
| `<command>: <input> = <choice>` / `= on` | a choice or check changed |
| `Preview <command>: ok`, `Preview <command>: failed: <reason>`, `Preview <command>: not built: <reason>` (the inputs make no definition), `<command>: OK refused` | |
| `Added <name> as F5`, `Edited F2`, the command's `describe` line | OK |
| `Press Pull opened Fillet1 for its radius` | a command's inputs asked for another feature's edit, which opens next |
| `Body Body1 (F2.b0): volume a -> b mm3`, `New body …`, `Removed body …` | the bodies OK, Undo or Redo changed, measured by the model (with `MITCAD_LOG_VOLUMES=1`, which the tests set: the first change after opening a large design would measure every body) |
| `Cosmetic threads shown: 1` | the cosmetic threads drawn as rings changed |
| `Sketches shown: F1, …` | the sketches the view shows |
| `Sketch Sketch1 shown for the edit of Extrude1` (`for Extrude`), `Sketch Sketch1 hidden again after the edit of Extrude1`, `Sketch Sketch1 stays shown after … (its light bulb)` | a command started with a sketch it uses whose light bulb is off, and ended: the sketch shown for its time, then back to its light bulb (mitcad#546) |
| `Timing refresh: 36.5 ms`, `Timing display: 1093 ms, 166 bodies, 166 new` (`, 1 placed`: bodies only placed elsewhere, their drawn objects moved, mitcad#838), `Timing hover: …`, `Timing frame: …`, `Timing open: …` | with `MITCAD_LOG_TIMING=1` (`framework/Diagnostics.hpp`): how long the steps take, for `tools/perf-measure.sh` |
| `Selected: 1 face [face F2:end(…) of F2.b0]`, `Context menu: A \| B` | outside commands |
| `Context menu Solid: Press Pull \| Fillet \| More`, `Context menu Solid More: Draft \| Hole` (`BUILD`, `Construct`), `Context menu Delete: Delete Member \| Delete Body \| Delete Component`, `Deleted member <name>` (`spacing set of <name>`), `Deleted body Body1`, `Deleted component Component1:1` | the view's context menu's submenus (mitcad#494; [The view's context menu](#the-views-context-menu)) and what Delete did, one undo step each; `tools/ui-context-menu-test.sh` |
| `Eye click Root/Bodies/Body1 (press)` (`(double-click)`) | a click on a light bulb in the browser, before its `Visibility …` line; the press, its moves and its release belong to the light bulb and never reach the tree's selection (mitcad#344; `tools/ui-browser-test.sh`, `tools/ui-windows-browser-test.ps1`) |
| `Context menu Hide: Hide Body \| Hide Component` (`Hide 2 Bodies`, `Hide 2 Components`; the submenu of the view's context menu, mitcad#347), `Hid body Body2`, `Hid component Component1:1` | Hide > Hide Body / Hide Component: `set_body_visible` for the bodies of the selected faces, edges, vertices and bodies, `set_occurrence_visible` for their occurrences (none for the root's bodies, so no Hide Component), each once, as the browser's light bulbs do (`BrowserController::hideBodies`, `hideOccurrences`), one undo step; `tools/ui-hide-menu-test.sh` |
| `Find in Design opened`, `Find in Design found: 5 bodies, 0 components`, `Find in Design: hid 3 bodies` (`showed`, `1 component`; `selected …`, `appearance for N bodies`), `Find in Design button at x,y`, `Find in Design field\|components\|bodies\|visibility\|show\|hide\|appearance\|select\|all\|none\|close at x,y`, `Find in Design row N at x,y` | the Find in Design window (mitcad#350): the results after each search, what each button did (bodies by uid and components by occurrence, each once), and where the controls and the rows in sight are; the field is also logged as `Dialog field Find in Design: <text>`; `tools/ui-find-in-design-test.sh` |
| `Create Sketch: select a plane`, `Create Sketch: origin planes shown`, `Create Sketch picked …`, `Sketch started on xy, origin (0, 0, 0)`, `Editing sketch F1 (Sketch1)`, `Sketch finished` | sketch mode |
| `Sketch view x0,y0 x1,y1 x2,y2` | where sketch points (0, 0), (100, 0), (0, 100) are in the window, when it changed (`ui_sketch_click x y`) |
| `Sketch plane centred on the origin: origin at 812.00,410.00; free area centre at 812.00,410.00` (`on the view's middle`, `on the sketch's face`) | the view turned to a sketch plane (a sketch entered, Look At): the point it centred, where the sketch's origin and the middle of the free area are in the window (mitcad#738; the middle of the sketch's face, mitcad#772) |
| `Sketch axes drawn: origin at 812,456 in the view; x axis 300,456 to 1400,456; y axis 812,900 to 812,60` (`outside the view`; an axis `none`) | in sketch mode, the sketch's own origin and its x and y axes as the overlay draws them (window pixels; each axis across the view from its negative to its positive end, `none` when the plane is seen edge on or the axis misses the view), whatever the Origin folder's light bulb says (mitcad#739, `SketchOverlay::logAxes`); when it changed, logged when the overlay paints and with `Sketch view` (a Windows session without a desktop never paints), again at every sketch edit (`tools/ui-sketch-origin-test.sh`, `app.ui-windows-sketch-origin`) |
| `Design origin hidden in the view; the Origin folder's light bulb off` (`shown`, `on`) | whether the view shows the design's origin datums (an input that takes construction geometry, such as Create Sketch's plane pick, or the Origin folder's light bulb) and the light bulb, when either changed (mitcad#739, `MainWindow::showDatums`) |
| `Sketch frame in the design (at O1): origin (100.000000, -10.000000, 50.000000), normal (0.000000, -1.000000, 0.000000), x axis (…)`, `Sketch view along (0.000000, 1.000000, 0.000000)`, `Line c1 in the design from (x, y, z) to (x, y, z)` | a sketch edit started: its frame where the view shows it (a component's sketch placed by the occurrence named, mitcad#505) and the direction Look At turned the view to; a line the Line tool added, its ends in the design; `tools/ui-occurrence-face-sketch-test.sh` |
| `Sketch origin at 0.00, 0.00, 20.00 mm in the design; the design origin at x 0.00, y 0.00 mm in the sketch` | a sketch edit started: where its origin lies in the design and where the design origin lies on its plane, in the document's length unit (mitcad#254) |
| `Sketch pointer x 30.00, y -12.50 mm` | in sketch mode, the pointer's place in sketch coordinates that the view shows above the status badge (where a tool snaps it), once the pointer rests at a new place (mitcad#254) |
| `Sketch field length at x,y` | a drawing tool's value field: the centre of its box in the main window's coordinates, for each field shown, once the pointer rests and the fields have moved since (mitcad#671) |
| `Selection filter sketch defaults: sketch-geometry=cross, …`, `Selection filter sketch visible-edges cross: …`, `… reset: …`, `Selection filter project defaults: …`, `Selection filter sketch restored: …`, `Selection filter button at x,y`, `Selection filter row visible-edges at x,y`, `Selection filter reset at x,y`, `Selection filter widened (Shift)`, `Selection filter as set` | sketch selection filters (mitcad#217): the rows when a sketch edit or Project starts, a row clicked, the sketch's rows back after Project, where the control and its rows are (the rows when its menu opens), Shift down and up |
| `Sketch hover: edge E{…} of F1.b0` / `nothing`, `Sketch pick places:` then `Sketch pick edge F1.b0/E{…} visible at x,y` (`occluded`) | with `MITCAD_LOG_PICKS`: what a click would take under the pointer in sketch mode and Project; with `MITCAD_LOG_EDGE_PLACES` (`tools/ui-sketch-select-test.sh`; every edge is tried) where a click hits each body edge in sight and behind faces once the camera rests in sketch mode |
| `Sketch Sketch1: 2 DOF`, `… 0 DOF, fully constrained`, `…, conflicts k3+k5` | the sketch's status changed |
| `Sketch entities recoloured for a dark background: 8 fully constrained, 1 centre lines` (`light`) | the view's background turned dark or light while sketch entities are shown: fully constrained geometry and centre lines take the background's colours (mitcad#253), their halos too (mitcad#302) |
| `Sketch halos: 5 dark` (`light`) | the halos shown changed in number or tone: fully constrained curves and centre lines are drawn over a wider line in the background's tone (`OcctViewer::styleSketchHalo`), dark on a dark background and light on a light one, so that they stay readable over a body's face; points, construction and projected geometry and unconstrained curves have none (mitcad#302) |
| `Sketch layers: halos < curves and highlights < points and point highlights; points keep the scene's depth; point highlights over the markers` | the order OCCT draws the sketch layers in, read from the viewer when sketch entities are shown and it changed (`OcctViewer::logSketchLayers`: the layers that are not immediate first, then the immediate ones, each in its place in the list); whether the points' layer still tests the depth drawn before it (`lose` if it cleared it), and whether a selected point's ball is drawn after the plain markers in it (mitcad#897; `tools/ui-sketch-spline-test.sh`, `app.ui-windows-sketch-spline`) |
| `Sketch highlights: 1 curve, 0 points` | the selection's highlights of sketch curves (in the top layer) and sketch points (in the points' layer) changed in number (mitcad#897) |
| `Sketch dimension halos: 3 dark` (`light`) | the dimensions shown in sketch mode changed in number or the halo's tone: every dimension (normal, driven, hovered, selected, and the tool preview's) is painted over a halo under its dimension and extension lines, angle arcs and arrowheads (`SketchOverlay::paintDimensionHalo`: 3 px strokes under the 1 px lines, a 2 px outline round the arrowheads, all halos before any line; the values' boxes unchanged), in the same tone as the geometry's halo (`OcctViewer::sketchHaloColor`, the background's tone, the opposite of the lines'), so that the lines stay readable over a body's face of their own tone (mitcad#633). The lines and arrow tips are moved by less than a device pixel onto whole device pixels, so that a level or upright line is one sharp row in its own tone between whole rows of its halo instead of two half-tone rows blurred into it; `tools/ui-sketch-view-test.sh` also reads the pixels |
| `Sketch colours: F1 8 fully constrained, 0 unconstrained; F3 in O1 4 fully constrained, 3 unconstrained` (`none`) | the shown sketches changed how they are coloured, in sketch mode and outside it: per sketch (with the occurrence that places a component's sketch) the entities drawn in the fully constrained colour of the background and those in the unconstrained blue; construction, centre lines and projected geometry are not counted (mitcad#286) |
| `Sketch tool Line`, `Added line c2 from (0, 0) to (40, 0) (horizontal)`, `Line chain closed`, `Added rectangle 40 x 25 mm at (0, 0) [c1, …]`, `Added rounded rectangle 40 x 20 mm, corner radius 5 mm at (0, 0) [c1, …]` (mitcad#867), `Added circle, diameter 20 mm at (x, y) [c5]`, `Added arc …`, `Added polygon (6 sides) …`, `Added slot, width 12 mm …`, `Added ellipse …`, `Added spline c9 through 3 points` (`Added closed spline c9 through 4 points`, mitcad#783), `Added point p4 at …`, `Added text t7 "…" at …` (or `in a frame from (x, y) to (x, y)`), `Trimmed c3 at (x, y)`, `Extended c1 at (x, y)` (with ` (joined p5)` when the end joined an existing point, mitcad#259) | the tools |
| `Sketch cursor: bitmap 32x32, hot spot 10,10, ratio 1, icon line`, `Sketch cursor: arrow` | the view's pointer when a sketch tool starts or ends, or the screen's ratio changes (logical pixels) |
| `Sketch value length: 40 = 40 mm`, `Sketch value editor at x,y with 40.00` (`with d1 = 40` for a dimension's, its name first), `Sketch value editor tooltip: d1 = 40.00 mm (Sketch1 width)`, `Sketch value editor: inserted d2, now d2 * 2`, `Sketch value editor kept open with d2 = 30` | typed values, a dimension's value editor, a dimension clicked while it is open, a refused name (mitcad#231) |
| `Sketch plane geometry: 4 edge(s), 4 vertex(es)` (`Sketch plane geometry not read: <reason>`), `Sketch Dimension picked the body edge E{…} of F2.b0 (body:3)`, `Projected the body vertex V{…} of F2.b0: p8`, `Reused the projection of the body edge E{…} of F2.b0: c9`, `Point p9 coincident with p8, the projected corner V{…}`, `Point p9 at the midpoint of c7, the projected edge E{…}`, `Sketch Sketch2: 1 linked projection(s)` | body edges and vertices in the sketch plane (mitcad#256, mitcad#258): what the tools can pick and snap to, read for a tool session or after a change; a body pick; what a dimension or a point projected or reused; the sketch's linked projections, when their number changes |
| `Made the sketch x axis for a pick: c3` (`y axis`, `origin`) | Sketch Dimension or a constraint tool made the entity of an axis or origin pick for its dimension or constraint (mitcad#500), in the same undo step |
| `Added dimension k5 length d1 = 40 mm`, `Added driven dimension k7 length`, `Editing dimension k5 (40 mm)`, `Dimension k5 = 50 mm (d1)`, `Moved dimension k5 to (x, y)`, `Dimension k5 text at x,y` (with each `Sketch view`), `Dimension tooltip k5: d1 = 40.00 mm (Sketch1 width)`, `Dimension text name`, `Dimension texts: k5 d1, k6 d2`, `Palette paletteDimensionText at x,y`, `Panel Extrude label: Distance (d3)` | dimensions; their tooltip, the palette's Dimension text option and what each shows, a feature panel's field with its parameter's name (mitcad#231) |
| `Signed distances on` (`off`), `Added dimension k5 horizontal_distance signed d3 = r * cos(a)`, `Dimension k5 shows fx: +25.98`, `Dimension k6 unsigned: fx: 25.98 (-(r * sin(a)))` (`signed: …`), `Dimension texts: k5 fx: +25.98, k6 fx: +15.00` (as a sketch with dimensions opens), `Palette paletteSignedDistances at x,y` | signed horizontal and vertical distances (mitcad#409): the palette option, a signed one added and the text it shows, a switch from the context menu with the text and expression after it |
| `Panel Extrude tooltip distance: d5 = 2*d1 → 80 mm (Extrude2 distance)`, `Parameter labels for Extrude: Sketch1 k5 d1, Sketch1 k7 d3` (or `none`), `Parameter label Sketch1 k5 d1 at x,y`, `Parameter label picked: d1 (Sketch1 k5)`, `Panel Extrude input distance: inserted d1, now 2*d1`, `Parameter labels for Extrude hidden`, `Native inline editor sketchValueEditor name: d3 =` (macOS) | a feature panel's value field whose parameter the feature owns: its tooltip; while a value field has the keyboard (when the keyboard went there by itself as the panel opened, only once the field is clicked or typed in, mitcad#587; after a pick when the selection takes more items, only once something is typed in it, mitcad#467), the labels of the shown sketches' dimensions, only those (mitcad#587 took out the earlier features' values; `framework/ParameterPicker.hpp`), where each is, the one clicked and the name it put into the field, the labels gone a moment after the field lost the keyboard; the dimension's name beside the native inline editor (mitcad#250) |
| `Added constraint perpendicular k6 on c1, c2` (a tangent's point after it: `Added constraint tangent k7 on c1, c5 at p2`, mitcad#783), `Constraint perpendicular refused: <reason>`, `Symmetry takes the selection as its picks: c1, c3`, `Symmetry does not take the selection: <reason>`, `Symmetry pick c7 refused: <reason>` (mitcad#865), `Fixed …`, `Construction on: [...]`, `Merged points p5 into p3 (coincident on p3, p5)` (two curve ends made one, mitcad#260), `Converted to sketch geometry: c9, p7; kept projected: p8` (mitcad#257), `Deleted c4, k2` | constraints and modify commands |
| `MCP live instance: <id> 127.0.0.1: <port>`, `Agent connected`, `Agent disconnected`, `Agent disconnected by the user`, `MCP: an agent ran sketch.add_line in sketch F4`, `MCP: an agent ran the UI command sketch.finish`, `MCP: an agent selected 2 sketch curves`, `MCP: an agent saved <path>`, `MCP: screenshot of the view, 640 x 480, 12345 bytes`, `Agent change: Add Line (agent)`, `MCP refused: <reason>`, `MCP: closed the dialog <title> (an agent cannot answer it)` | live MCP (mitcad#265): the listener, connected agents, what an agent ran, refusals |
| `Agent indicator: no agent` (`connected: claude-code`, `2 agents connected: claude-code, codex-mcp-client`, `unavailable`: the clients' own labels), `Agent indicator at x,y`, `Agent indicator chose help.connect_assistant` (a click with no assistant connected), `Agent indicator menu: Connect AI Assistant… \| Folders for Designs…` (with Copy Command for This Window between them while several windows listen), `Agent indicator menu chose disconnectAgent` (`help.connect_assistant`, `copyConnectCommand`, `tools.preferences.agents`), `Agent indicator: copied <command>` | the agent indicator (mitcad#710): its state when it changes (the agents by their `clientInfo` names), its place, a click, its menu (entries as shown) and what was chosen |
| `Palette warning 0 at x,y` (each warning shown as a link), `Palette: warning 0 selects 8 entities: p1, p2, c3, …`, `Sketch view to the selection: pan` (or `zoom`; nothing when they were in sight) | the Sketch Palette's warnings (mitcad#264): where to click them, what a click selects (the `Selected: …` line follows), how the view moved |
| `Palette Auto constraint: Link to first ( ), Link to last (x); Equal (x), Horizontal/Vertical alignment ( ), Concentric ( )` (or `hidden`), `Palette paletteAutoLast at x,y` (`paletteAutoFirst`, `paletteAuto_equal`, …, the section `paletteAutoConstraint`), `Auto constraint link: first`, `Auto constraint equal: on (checked: equal)`, `Auto constraint target c2`, `Auto constraint reference highlighted: c2` / `highlight off`, `Auto constraint equal k7 on c2, c4` (the reference first; the alignment as `horizontal_points`/`vertical_points` of the curves' centres), `Auto constraint equal on c2, c4 refused: <reason>`, `Auto constraint dialog: Equal: <reason>`, `Auto constraint accepted: c4 kept without them` / `cancelled: c4 taken back`, `Auto constraint entities: c2, c4` | the palette's Auto constraint section (mitcad#189): its state when it changes, its places (in the docked layout the palette scrolls, and the section is scrolled into view when it shows; every `Palette … at x,y` line is logged again when the palette scrolls, mitcad#412), the choices, the reference the preview targets, the constraints each new entity gets, a refused one's dialog and answer, the tool session's entities |
| `Palette row paletteGrid at x,y size 276x22, hint 93x22` (each option row, `paletteDimensionText`, `paletteFinish`), `Palette size 270x512, hint 242x512, minimum hint 242x512`, `Palette scrolled to 137 of 137 (view 375 high, palette 512)` | the palette's rows against their size hints (mitcad#470): in the floating layout the palette is in a scroll area in the command card, so a card shorter than the rows scrolls them instead of squeezing them below their hints; logged with the places, again when the palette scrolls |
| `Dragged p5 to (x, y)`, `Dragged c7 to (x, y)`, `Drag of p5 did not move it` | a drag ended: where the point is, or the curve's point nearest the cursor |
| `Double-click on t26: sketch.edit_text`, `Edited text t26: "MIT", Droid Sans, center`, `Edited pattern k8: 4 in all`, `Edited offset k13: 3 mm` | the edit panels of texts, patterns and offsets |
| `Edited text t1: "MIT", Liberation Sans (LiberationSans-Regular.ttf), left`, `Edit Text font file <path>`, `Preferences font folder add at x,y` (Add…, answered in Qt's own folder dialog **Font Folder**), `Preferences: font folders <folders>` (`none`) | a text in a font file, the file chosen with File..., Preferences › Fonts (mitcad#473, `tools/ui-text-fonts-test.sh`) |
| `Equal spacing k4 horizontal: p1, c2, c5 (spacing k6 = 12.00)` (`… along c9`; without the spacing when it is free), `Equal spacing k4 removed`, `Equal spacing preview: p1, c5, c2` (the panel's objects in its order), `Sketch Dimension on 3 objects: Equal Spacing Dimension`, `Double-click on k4: sketch.edit_equal_spacing`, `Equal spacing of 3 object(s), horizontal, spacing 12 mm`, `Edited equal spacing k4 of 4 object(s), vertical` | equal spacings (mitcad#221): each chain in the order the model keeps, when it changes (also during a preview), the panels |
| `Timeline Sketch1 at x,y`, `Timeline marker at x,y`, `Timeline button back at x,y` (start, back, forward, end) | the timeline's items, when they changed, also when the timeline moves in the window (a banner or another panel makes room) |
| `Timeline search opened` / `closed`, `Timeline search field at x,y`, `Timeline button search at x,y` (`search-previous`, `search-next`), `Timeline search "fillet in:cover": 2 matches: Fillet2 (Cover), Fillet7 (Cover)` (`no matches`, `unknown filter is:x`), `Timeline search result Fillet2 (Cover) at x,y`, `Timeline search results hidden`, `Timeline search match 1 of 2: Fillet2 (Cover)`, `Timeline search cleared` / `cleared: selected Plane1 again, scrolled to 0`, `Selection restored after the timeline search: 1 item(s)` | the timeline search (mitcad#179): its field, the matches (when they change), the listed ones in sight, a step, the end and what it restored |
| `Timeline colours: dark, marker #c8d0da, buttons #f0f0f0, window #1f2024, handle #5aa2ff` (`light, marker #2b3a4a, buttons the style's, …`), `Asking the platform for the dark colour scheme` | the timeline's palette colours at start and when the theme changes (the marker's handle: the Docked window's accent colour, orange in the Floating chrome); the switch of `MITCAD_TEST_COLOR_SCHEME` |
| `Window style: dark, window #141518, panels #1f2024, accent #5aa2ff, font Segoe UI Variable Text` (`light, …`; `font the platform's`), `Window frame: Windows 11 caption colour, Mica, rounded corners` (`skipped (Windows build 19045)`, `not applied (DWM error 0x…)`) | the Docked window's style at start and when the theme changes (mitcad#251, `framework/WindowStyle`); on Windows what the window frame got, when the window shows and at each change of theme |
| `Timeline hover highlighted: F3 1, F4 2` / `none` | timeline hover preview, without changing selection; folded groups include every active member |
| `Feature faces highlighted: F3 1, F4 2` / `none` | the faces of the features selected on the timeline (uid and number of faces), when they change |
| `Feature inputs highlighted: Fillet1 edges 4, Chamfer1 edges 2` / `none` | the picked edges of the Fillets and Chamfers selected, drawn where they were (mitcad#957; name and number of edges), when they change |
| `Selection card create: Extrude \| Revolve \| Sweep` / `none`, `Selection card create Extrude at x,y`, `Selection card create chose Extrude` | the selection card's Create row changed, where each of its buttons is (300 ms after the change), and a click on one (mitcad#619) |
| `Timeline group Group1 at x,y` (its band, or its folded cell), `Timeline group Group1 collapsed` / `expanded`, `Timeline selected: F1, F2, F4` (a Shift+click run, or what the browser's rows selected; `nothing`), `Grouped Sketch1, Extrude1 as Group1`, `Renamed group Group1 to Body`, `Ungrouped Body` | timeline groups |
| `Timeline cell Fillet2: failed, selected, status ring, outer ring, badge` (the state: `failed`, `warning` or `import note`; then `selected`, `found`, `hovered`, `rolled back`; then what was painted: `status ring`, `outer ring` (the selection's or a match's), `badge`; a folded group's cell by the group's name) | the painted state of a timeline cell with a status, when it changes (mitcad#961) |
| `Timeline tooltip: Fillet2: <error>`, `Marker at 2 of 4`, `Moved Plane1 to 0 in the timeline`, `Move of Fillet1 refused: <reason>`, `Suppressed Fillet1`, `Renamed Plane1 to Datum`, `Delete Sketch1: also Extrude1, Fillet1?`, `Deleted Sketch1, Extrude1, Fillet1`, `Delete Sketch1 cancelled` | timeline actions |
| `Browser width 240` (`, scrolls sideways` with a horizontal scroll bar), `Browser Root/Bodies/Body1 at x,y` (its name), `Browser eye <path> at x,y`, `Browser radio <path> at x,y`, `Browser linked Root/Sketches/Sketch1/Extrude2 to Component1` (a feature row whose result is in another component) | the browser's width and the rows in sight (names from the root; a row whose middle is in the view), when they changed, also when the tree scrolls or the panel moves in the window; a place logged before whose row has left sight (scrolled away, folded, gone) is logged once more as `Browser <path> out of sight` (also `Browser eye <path>`, `Browser radio <path>`), so that a test never clicks a place that is not valid; `tools/ui-test-lib.sh` scrolls such a row back into sight before it clicks it (`ui_browser_reveal`) |
| `Browser background: rows` (`card`), `Preferences: browser background rows`, `Preferences browser background at x,y`, `Browser view beside the rows at x,y` (`… out of sight`) | the Browser's background in the Floating layout (mitcad#419): at start and on each change, the choice saved in Preferences, its drop-down in the dialog, and with Rows a place right of every row's box where a click reaches the 3D view (`tools/ui-browser-rows-test.sh`) |
| `Browser scroll range 240`, `Browser scroll bar at x,y` (`… out of sight`) | the rows' vertical scroll range (0 while every row fits; mitcad#425: the Floating Browser card grows to the timeline before it scrolls) and the vertical scroll bar in sight, the tree's or, with Rows, the one at the card's left (`BrowserPanel::setScrollBarOnLeft`), with the rows' places (`tools/ui-browser-rows-test.sh`, `app.ui-windows-browser-rows`) |
| `Browser selected: <paths>`, `Visibility <path>: hidden`, `Visibility <path>: hidden (during Extrude)`, `Command Extrude: 1 visibility change(s) kept`, `Renaming <path>`, `Renamed Body1 to Plate`, `Renamed body F3.b0 with its component to Bracket` (mitcad#291), `Activated Bracket`, `Isolated …`, `Copied …`, `Pasted Bracket:1 as Bracket:2`, `Placed Bracket:2 at (90, 0, 0)`, `Paste of Bracket:3 cancelled`, `Ground Bracket:2`, `Deleted Bracket:2`, `New component in Root`, `Placement warning: <text>` (mitcad#292), `Look at plane F4`, `View top`, `Units: in`, `Found … in the browser` | browser actions |
| `Activate component asked: Box:1 (active Root)`, `Activate component answer: activate Box:1`, `Activate component answer: keep in Root, don't ask again`, `Activated Box` | Create Sketch or a Construct command picked another component's geometry and asked whether to activate it, and the answer (mitcad#960, [Activating the picked component](#activating-the-picked-component)) |
| `Bodies shown: F2.b0, F3.b0 in O1`, `Body F3.b0 in O1 at x,y` | the shown bodies (with their occurrences) and their middles in the window |
| `Failed features: Fillet2: <error>` / `none`, `Failures button at x,y`, `Revealed Fillet2` | the status bar's summary; a reveal selects the feature (mitcad#961) |
| `Feature warnings: Fillet1: <warning>` / `none`, `Preview Fillet: ok, warning: <warning>` | features that succeeded with warnings, when they change; a preview's warning |
| `Warnings badge: Gear: <message>; Helix: <message>` / `none`, `Warnings button at x,y` | the yellow badge's features (with warnings or an import note, mitcad#170), when they change |
| `Browser sketch DOF: Sketch1 0, Sketch2 4` | the sketches' degrees of freedom the browser shows, when they change |
| `Browser joints: Joint1 placed, AsBuiltJoint1 satisfied, DOF Root 1` | the joints' states and each component's degrees of freedom (mitcad#55), when they change |
| `Added joint Joint1 (revolute): placed, rz 30 deg, moved O2; DOF 7` (a failure: `failed: <error>`), `Added as-built joint …`, `Added joint origin JointOrigin1: origin (5, 5, 10), normal (0, 0, 1)`, `Added rigid group RigidGroup1: Pin:1, Cap:1`, `Drove joint Joint1 (revolute): placed, rz 45 deg, …`, `Added constraint Constraint1: insert placed, moved O2; DOF 1` (each row's type, `(does not hold)` after one that does not), `Preview Joint moves O2` | the ASSEMBLE group's commands (mitcad#55, mitcad#148; the joint as the `joints` query has it after OK) and a preview that places occurrences elsewhere |
| `Drag of Pin:1 started at (x, y, z)`, `Dragged Pin:1 to (x, y, z): Joint1 rz 63.2 deg`, `Drag of Pin:1 cancelled`, `Animating Joint1 rz: 36 frames from 45 deg to 45 deg`, `Animated Joint1: 36 frames`, `Animation of Joint1 stopped: <why>` | dragging a component the joints move (points in its parent's coordinates) and Animate Joint |
| `Parameters favorites: width` / `none`, `Parameters width favorite at x,y`, `Parameter width favorite: on = 30 mm` | favourites |
| `Parameters dialog opened`, `Parameters: d1 = width + 5 mm (35 mm); …`, `Parameters d1 expression at x,y` (name, unit, expression, comment), `Parameters add/delete/OK at x,y`, `Added parameter width = 30 mm`, `Parameter d1 expression: width + 5 mm = 35 mm`, `Parameter width expression: d2 / 2 refused: <reason>`, `Delete parameter w refused: <reason>` | Change Parameters (places in the main window's coordinates) |
| `Parameters d1 min\|max\|step\|strict at x,y`, `Parameters d1 slider at x,y`, `Parameters d1 slider handle at x,y`, `Parameters only limited at x,y`, `Parameter d1 max: 50 = 50 mm`, `Parameter d1 limited: <the model's limit_message>`, `Parameters: d1 = 60 mm (50 mm ▲max); …`, `Parameters limited: d1 = 60 mm → 50 mm ▲max (<the Value cell's tooltip, lines joined with "; ">)` / `none`, `Parameter d1 typed 70 mm: d1: 70 mm will be limited to 50 mm (▲max)` (`within its limits`, `will be refused: <reason>`), `Parameter d1 slider: 40 mm = 40 mm`, `Parameters only limited: on: d1` | Change Parameters' limits (mitcad#665); `tools/ui-parameter-limits-test.sh` |
| `Parameters during command Extrude editing Extrude1`, `Parameters closed: back to Extrude`, `Panel Extrude parameters at x,y`, `Sketch Sketch1 dimensions after the parameter change: width 50.000, d2 40.000`, `Extrude: parameter depth changed; the preview follows`, `Command Extrude: 1 parameter change(s) kept`, `Parameter d3 is held by the open panel of Extrude1` | Change Parameters while a sketch or a feature is edited (mitcad#564, [Browser, timeline and parameters](#browser-timeline-and-parameters)) |
| `Configurations window opened`, `Configurations page Rows`, `Configurations options: Color (Black, White) = White; Length (300 mm … 2000 mm, step 10 mm) = 1500 mm` / `none`, `Configurations rows: open, default none, current Row1: Row1: Color = White, Length = 1500 mm`, `Configurations row issues: Row2: <the refusal>` / `none`, `Configurations tab Rows at x,y`, `Configurations add option\|remove option\|edit domain\|move up\|move down\|closed\|add row\|delete row\|set as default\|apply at x,y`, `Configurations option Length domain\|default\|comment at x,y`, `Configurations row Row1 Length at x,y` (a cell: the row and the column's header), `Add Option dialog opened: Color, Length, New Parameter`, `Configurations added option Color`, `Configurations added row: Row1`, `Configurations row Row1 Length: 800`, `Configurations row Row2 Length: 1500 refused: <reason>`, `Configurations option Length default: 1200`, `Configurations applied Row1; moves: <notice>`, `Option Length domain: range 300 mm … 2000 mm, step 10 mm` (`column Screws.Size`), `Option Domain dialog opened for Size`, `Configurations generate rows at x,y`, `Generate Rows dialog opened: Screws.Size for Size`, `Configurations generated rows from Screws.Size for Size: 3 rows from Screws: added M3, M4, M5` (`; updated M4`, `all up to date`), `Configurations generate rows from Screws.Size for Size refused: <reason>` | the Configurations window (mitcad#807, #849; places in the main window's coordinates) |
| `Configuration bar: Color = White (Black \| White); Length = 1200 mm (300 mm … 2000 mm, step 10 mm); row none` / `none`, `Configuration bar row at x,y`, `Configuration bar Length at x,y`, `Configuration bar Length slider at x,y`, `Configuration chosen: Color = Black`, `Configuration row chosen: Row1`, `Configuration moves: Length 1200 mm → 1000 mm: Black is short`, `Configuration refused: <why>`, `Parameters options: Color; Length (narrowed by Black is short: 300 mm … 1000 mm, step 10 mm)` / `none` | the Configuration bar in the browser and the options in Change Parameters (mitcad#807) |
| `Rules: 2`, `Rules row Black is short: enabled, ok` (`disabled`; `Not saved: fill its empty sockets`, a problem), `Rules add\|duplicate\|delete\|text\|check at x,y`, `Rules row Black is short at x,y`, `Rules naming Rule 1`, `Rules saved: <edit>`, `Rules not saved yet: <edit>`, `Rules refused: <edit>: <reason>`, `Rules rename refused: <reason>`, `Rules undo: the last change not saved` (`the design's last step`), `Rule palette compare at x,y` (each entry by its id: `when`, `always`, `else`, `and`, `or`, `not`, `compare`, `one_of`, `option`, `parameter`, `number`, `text`, `column`, `formula`, `allow_only`, `exclude`, `min`, `max`, `step`, `require`, `forbid`, `in_table`), `Rule palette drag max`, `Rule palette chooses for then.1 slot`, `Rule canvas when.0 socket at x,y` (each place: `rule frame`, `when label`, `then label`, `when block`, `when socket`, `when.2 add`, `then.1 slot`, `then.0 option`, `when op`, `then.1 message`, `then.0 table`, `then.0 columns`; `… out of sight` scrolled away), `Rule canvas target when socket`, `Rule canvas drag then.0 block`, `Rule canvas focus then.0.0 socket`, `Rule canvas editing then.0.0 block: 1000 mm` (`… cancelled`), `Rule canvas formula when: <text>`, `Rule edited Black is short: Put max in then.0` (`Rule when`, `Set when.1 to 'Black'`, `Move …`, `Delete …`), `Rule as text Black is short: when Color == 'Black' / then Length max 1000 mm`, `Rule canvas columns of Screws: Length=Length, Size=Size` (`none`), `Rule canvas columns dialog for then.0 of Screws`, `Check Rules: <line>`, `Check Rules never allowed: Length 2500 mm`, `Context menu: Edit \| Edit as Formula… \| Delete` | the Rules tab and its block editor (mitcad#808, [The rule block editor](#the-rule-block-editor); places in the main window's coordinates); `tools/ui-configuration-rules-test.sh` |
| `Property Sets opened for F2.b0, O1`, `Property sets of Body Panel, Occurrence Wall:1, Component Wall: N properties`, `Property sets row: component Identity/Mark = W-12 (user)` (`(user, inherited)`, `(ifc, edited)`, `(computed)`), `Property sets <level> <set>/<key> value at x,y`, `Property sets add\|remove\|types\|computed\|close at x,y`, `Editing property <level> <set>/<key>` (`as a choice: EI30 \| EI60 \| EI90`), `Property sets choice at x,y`, `Property <level> <set>/<key> set: W-20`, `… chosen: EI60`, `…: <text> refused: <reason>`, `Added property <set>/<key> (<type>)`, `Removed property …`, `Property type defined: <name> (<base>)`, `Property type removed: <name>`, `Find in Design property search: <reason>` | Property Sets and its property search (mitcad#496; places in the main window's coordinates); `tools/ui-property-sets-test.sh`, `tools/ui-windows-property-sets-test.ps1` |
| `Build parts list columns: Mark, FireRating`, `Build parts list row: Wall: W-20 \| EI60`, `Parts list columns at x,y` | the Parts List's property columns (mitcad#496): the keys and each row's values; the field's place |
| `Panel Fillet input sets.1.radius at x,y`, `Panel Fillet input add sets at x,y`, `… input remove sets.1 at x,y`, `… input up_sections.2 at x,y`, `… input down_sections.0 at x,y` | a list's rows and buttons, an ordered selection's buttons |
| `Panel Profile button_part at x,y` (a frontend without tiles), `Panel Profile tile part: <name>`; `Part picker place search at x,y; in the picker x,y` (also `star`, `pin`), `Part picker group row Profiles at …`, `Part picker tile <library>/<component> at …` | the button beside a choice (`withButton`, mitcad#418); the part picker's places, in the main window and in the picker ([Part picker](#part-picker)) |
| `Panel Extrude width 284 of 284` (after its places), `Panel Extrude message 284x36, panel width 284 of 284` (`message hidden`; `284x96 scrolling`) | the panel's width and its page's (mitcad#198): with its places, and once laid out after its message or a note under a field changed (`tools/ui-panel-message-test.sh`) |
| `Pick places:` then `Pick face F2.b0/F2:end(…) at x,y`, `Pick edge …`, `Pick vertex …`, `Pick profile F1/r{…} at x,y` | an input became active (with `MITCAD_LOG_PICKS=1`, which the tests set): where a click picks each item it takes (`ui_click_pick`); again after a pick when the input's test depends on the picks (Place On takes a body's straight edges once it has a face, mitcad#697). Each pixel is picked once while the places are tried |
| `View ghosts: O2 (1 body)`, `View ghosts: none` | the occurrences a command draws as ghosts changed (`withGhosts`, mitcad#147); logged before the pick places of the input that picks through them |
| `View inactive ghosts: 2 bodies (active Bracket:1)`, `View inactive ghosts: none`, `Pick priority: active Bracket:1` | the bodies drawn as ghosts outside the active component changed (mitcad#420; `none`: the root active or the setting off); a click took the active component's item before a nearer one of another component |
| `View body highlights: F1.b0, F2.b0 in O1`, `View body highlights: none` | the bodies highlighted whole changed (a selection input of bodies, such as 3D Print's, mitcad#234) |
| `Hover chain: 3 edge(s) of F2.b0` (`face(s)`), `Tangent chains of F2.b0 (edges): 2 in 3 ms` | the hover of an input with `withHoverChain` over an edge or face changed how many it shows (the tangent chain with its check on, else 1); a body's chains asked the first time (mitcad#290) |
| `Manipulator <command> <input> at x,y`, `Manipulator <command> <input> dragged to <value>` | a handle's knob, a drag ended |
| `Arrow direction <command> <input>: x,y,z` (three decimals, beside each arrow's `Manipulator … at`) | which way an arrow points in the design (mitcad#958: Extrude's arrow of a sketch in a turned occurrence, `tools/ui-extrude-face-test.sh`) |
| `Value box <command> <input> at x,y: <text>`, `Value box <command> <input> opened: <text> at x,y`, `... closed: <text>` | a handle's value box (mitcad#656) and the field a click on it opens over the view |
| `Place <command>: face plane xy, 80 mm from axis y, 0 mm from axis x (point 80, 0, 0)`, `Place <command>: axis edge E{...} of F1.b0`, `Place <command>: edge ... is a reference now`, `Place <command>: none` | where Place On puts the part (mitcad#656), after a pick, a reference picked or a placement loaded |
| `Gear Pair candidates: 12, centre distance 60 mm`, `Belt Drive candidates: ...`, `Pulley belt check with F1: ...` | the candidates the model found for two axes, a pulley's belt check (mitcad#656) |
| `Gear Pair candidate kept: row 2`, `Belt Drive candidate kept: ...` | the row chosen before is still chosen after the candidates were asked for again (mitcad#761) |
| `Manipulator Circular Pattern suppressed.2 at x,y` (`… (off) at` when suppressed), `Manipulator Circular Pattern suppressed.2 toggled`, `Circular Pattern Suppressed: 2` | a pattern's instance dots |
| `Inspect <command>: <log>`, `Closed <command>` | an inspection's result (Measure: `area 2400, distance 45, angle 90 deg`, of a circle or arc `length 31.416, radius 5, diameter 10, axis (0, 0, 20) along (0, 0, 1)`, with axes `axis distance 50, center distance 50, axis lines 2` (mitcad#647); Interference: `Body1 x Body2: <volume>` or `none`; Section Analysis: `measuring`, then `area …, length …` once measured after the view's update, mitcad#438) |
| `Measure history: 3 rows`, `Measure history row 3: <picks>: <values>`, `Measure history row 3 at x,y`, `Measure history Copy at x,y`, `Measure history showed row 1`, `Measure history copied row 1: 1 \| …`, `Measure history cleared` | Measure's history (mitcad#648, *Measure's history*) |
| `Computing section view started`, `Computing section view done in 12 ms` (`cancelled after`; `measure`), `Section clips at {"plane":…} off the UI thread (section worker): 3 bodies, 3 new, 3 meshed in 40 ms`, `Section measured off the UI thread (section worker)`, `Section view ready at {"plane":…}: 3 bodies from the section worker`, `Section round view superseded`, `Section round view replaced` | Section Analysis' rounds off the UI thread (mitcad#438, `framework/SectionWorker.hpp`): started and ended, what the worker's thread clipped and meshed, the view about to show them, a running round stopped by a newer one, a waiting one dropped |
| `Kept Section Analysis as Section1`, `Edited analysis Section1`, `Deleted analysis Section1`, `Section analysis at {"plane":{"normal":[1,0,0],"origin":[10,0,0]}}`, `Section analysis off` | Section Analysis kept in the document (mitcad#41); the plane the bodies are cut at changed |
| `Inspect Print Analysis: Body1 overhang 412.5 mm^2 (6.2 %), thinnest 0.50 mm, 3 thin region(s), estimate 41.5 g 13.9 m` (`, cost 1.04` with a price; bodies separated by `; `), `Kept Print Analysis as Print1`, `Print analysis Print1: <the same words>` (`failed: <reason>`, `cancelled`) | Print Analysis (mitcad#620): the panel's result, and the shown analysis computed again (after OK, a light bulb, an edit, a change of its bodies) |
| `Print overlay: 12 overhang triangle(s) #rrggbb, 4 thin triangle(s) #rrggbb (150.0 mm^2), marker Thinnest 0.50 mm at (x, y, z); legend Print1: #rrggbb Overhang, angle 45° \| #rrggbb Thin wall, under 0.8 mm`, `Print overlay: none` | what the view draws of a Print Analysis, when it changes (`no marker` without thin walls; in brackets the area drawn thin, only where the walls are thin, mitcad#767) |
| `Physical properties: Body1 volume … mass … center (…)` | the Properties dialog opened |
| `Done <command>` | a model command panel's OK |
| `Camera direction -0.577 0.577 -0.577 up … (orthographic)` | the camera came to rest after it moved (direction from the eye to the model) |
| `View navigation: 24 frame(s), 0 repaint(s) of widgets over the view, Print Analysis overlay set 0 time(s)` (the widgets' object names in brackets after their count when there are any, `(parameterPicker, viewHome)`; the overlay's count only while a Print Analysis is drawn) | a navigation of the view ended (`OcctViewer::navigationFinished`: a drag released, a wheel burst or an animation over): the frames drawn meanwhile, how often a widget over the view was painted again (each repaint makes Qt compose the window's whole area under the view again at that frame, which costs milliseconds per million pixels of the view: mitcad#768), and how often the Print Analysis overlay was set (a camera move computes nothing again) |
| `3D mouse: Raw Input connected (SpaceMouse Compact (HID 256f:c635))` (`spacenavd connected (/var/run/spnav.sock)`, `IOKit HID connected (…)`; also when a device comes later), `3D mouse: none` (no device or daemon at start; the backend keeps listening), `3D mouse: off` (Preferences has it off), `3D mouse: Raw Input disconnected`, `3D mouse: button 1 runs view.fit` (debug; `3D mouse: button 3 has no command`, `3D mouse: view.look_at is not available now`), `3D mouse motion: 42 frame(s), turned 12.3 degrees, panned 0.41 view heights, zoom 1.25x` (debug; when the cap comes to rest: what the motion did, a zoom above 1 closer) | a 3D mouse (mitcad#951, `view/SpaceMouseController.*`, `view/SpaceMouseView.cpp`); the test driver's `space-mouse` and `space-mouse-button` steps give the same input |
| `Layout grid 5 mm, major 25 mm, 18.2 px apart` | the layout grid's spacing changed, logged when the camera rests |
| `Orientation cube places:` then `Orientation cube front at x,y`, `Orientation cube front-top at …`, `Orientation cube front-right-top at …` | with `MITCAD_LOG_ORIENTATION_CUBE=1`, before each camera line: where a click picks each face, edge and corner of the cube in sight |
| `Orientation cube outline x0,y0 to x1,y1, picks x0,y0 to x1,y1, looking 0 1 0 (perspective), cube orthographic` | with `MITCAD_LOG_ORIENTATION_CUBE=1`, after the cube's places: the cube's box as drawn (through its own perspective, if any) and the extent of the places that pick it, in window pixels, with the camera's direction and projection and the cube's projection (`follow-view`, `orthographic`, `perspective`, mitcad#774); an orthographic or perspective cube is drawn and picked the same in either view projection (mitcad#766), so both stay the same when only the view's projection changes |
| `Orientation cube projection perspective`, `Preferences: orientation cube perspective` | the cube's projection changed (the view; Preferences when a change is saved); the cube's places and outline are logged again |
| `Orientation cube pressed on front at x,y`, `View home at x,y`, `View home`, `View top`, `Look at …` | the cube, the house, standard views |
| `Context menu: Home \| Camera \| Set Current View as Home \| Reset Home \| Fit`, `Context menu Camera: Orthographic \| Perspective` | the cube's right-click menu; the camera's projection in its Camera submenu, as in View › Camera, apart from Preferences' Orientation cube projection (mitcad#774) |
| `Orientation cube arrow up at x,y` (up, down, left, right, cw, ccw), `Orientation cube arrow cw`, `Orientation cube arrows hidden` | the cube's arrows: where they are when the view comes to rest face on, a click |
| `View operation failed: cube arrow up (the fit to the free area): Standard_ConstructionError: …` (warning), `View fit left out: the shown objects' box is not finite` | a camera change (an arrow, Home, a standard or named view, Look At) threw and the camera was put back as it was, instead of the exception ending the application; a turn did not fit a box that is no number (mitcad#773; `tools/ui-sketch-cube-test.sh`, `app.ui-windows-sketch-cube` fail on the first) |
| `Visual style Shaded with Hidden Edges`, `Camera perspective`, `Background #525761 #1f2126`, `Layout grid off`, `Snap to grid on`, `Grid spacing 10 mm` | display settings |
| `Saved named view NamedView1`, `View NamedView1`, `Deleted named view …` | named views |
| `Rendered view on`/`off`, `No render worker (mitcad-render) next to the application: no rendered view`, `Render worker started (pid <n>): <path of mitcad-render>`, `Render frame memory 1: 910 x 735` (debug; the frame memory's id and capacity), `Render worker ready: Cycles 5.3.0 on <CPU>`, `Render scene 1: 2 bodies, 1 meshes sent (F4.b0), 1 meshed, 0 released`, `Render scene 1 applied in 2.1 ms: received F4.b0; added -; changed F4.b0; moved -; removed -; kept 1; meshes 3; instanced 0`, `Render view 3: orthographic 728 x 588` (debug; a view sent to the worker, once a navigation of the view ends, mitcad#214), `Render view 3: first frame (145 x 117) after 26 ms`, `Render view 3: first denoised frame (145 x 117, 1 samples) after 60 ms`, `Render frame view 3 728x588 samples 4 denoised covers x0,y0 x1,y1` (debug; what the frame's alpha covers, in view pixels; `denoised` for a denoised frame), `Render image dropped: camera moved` (debug; `size changed`, `animation`: the view draws the bodies shaded from this frame, once per navigation, mitcad#216), `Render view 3: 64 samples in 2.91 s`, `Render mesh F2.b0: 12 of 12 triangles turned to face outwards` (debug; a triangulation wound inwards, mitcad#319), `Render material swap F2.b0: default (transmission 0, opacity 1) -> glass_clear (transmission 1, opacity 1); mesh reused (the worker holds it)` (debug; `meshed again`, `sent again`), `Render brightness view 3 samples 16: 0.412 over 5321 pixels` (debug; the mean linear brightness of what the frame covers, `tools/ui-render-glass-test.sh`), `Render worker: render-ground off: the camera looks at the ground from below` (debug; `render-ground on: the camera is above the ground again`: the ground dropped and built again as the camera crosses its plane, mitcad#320, `tools/ui-render-ground-test.sh`), `Render worker stopped: <reason>`, `View message: <text>`, `Render worker ended` | View > Rendered (`tools/ui-render-test.sh`) |
| `Preferences opened: General, 640 x 330` (the page shown and the size the window asks for), `Preferences page: General, minimum 520 x 300` (a page chosen and the minimum size it sets), `Preferences frame 700 x 420 at 330,170 on a screen of 1600 x 768 at 0,0` (once shown: the frame and the screen's available area), `Preferences minimum 520 x 300, resizable` (or `fixed`), `Preferences page list 140 wide, its entries 118` (the list's room and what its entries need; `tools/ui-preferences-test.sh`, mitcad#690), `Preferences: navigation middle-orbit, zoom to cursor on, …, autosave every 5 min` (or `…, autosave off`), `Shortcut overview: 23 command shortcuts` | the dialogs |
| `Section caps: 2 face(s) at {"normal":[1,0,0],"origin":[30,…]}`, `Section caps: none` | the caps of Section Analysis or Hide Above Sketch drawn (when they change), and gone again (the bodies whole) |
| `Section: F2.b0 shown whole: section: clip: SIGSEGV 'segmentation violation' detected. …` (warning) | a body whose clip failed, shown uncut (mitcad#392) |
| `Section caps: 3 colour(s) [#e0892a hatch #4b2d0b at 135 deg, 12 lines; …] in Shaded, bodies in appearance colours` | the viewer's caps (mitcad#382): per colour group the cap colour (the component's timeline colour, the default orange for the root), the hatch's colour, direction and line count; again when the visual style or Component Colors change |
| `Hide Above Sketch on`/`off`, `In front of the sketch plane: F2.b0 5.0, F5.b0 10.0` | the sketch palette's option; in sketch mode (also while a sketch command's panel is open), how far each shown body reaches in front of the sketch plane (mm, when it changes; 0 or less once cut, a body cut away entirely is left out) |
| `Exported <path>: dxf flat pattern of Body1, 5 entities (CUT_OUTER 4, BEND_DOWN 1), mm`, `Export of the flat pattern of Body1 refused: <reasons>`, `Flat pattern of Body1 shown` (`folded`), `Flat pattern of Body1: valid, area 1086.39 mm2, 1 bend(s)`, `Deleted the flat pattern of Body1`, `Added sheet face of 1 profile(s), new rules, thickness 2`, `Added flange, length 15 (tangent), angle 90 deg`, `Sheet rules: thickness …` | sheet metal (mitcad#210): the Flat Pattern's export, display and item, the commands' describe lines |
| `Export dialog: STEP AP214 (*.step) to <path>`, `Exported <path>: step, 1 body(ies), mm` (with occurrences `…, mm, design coordinates` or `component coordinates`), `Exported <path>: usdz, 1 body(ies), 1 material(s)`, `Exported <path>: dxf, 4 entities` | export |
| `3D Print panel: 3 visible: Plate, Cube, Pin; format stl, refinement high, slicer Fake slicer (/path <files>); slicers None \| …` (`1 selected: Cube`), `3D Print Bodies: 2 bodies [body F1.b0; body F2.b0]` (the input, as every selection input logs), `3D Print: Format = 3MF`, `3D Print: left out Surface1 (a surface body)`, `3D Print: no solid or mesh bodies`, `3D Print: 3 bodies as stl (high) to <folder>: Plate.stl, Cube.stl, Pin.stl` (a body shown twice: `Pin (Pin_1).stl, Pin (Pin_2).stl`; 3MF: `parts.3mf`… per body `Plate 12 triangles, 1000.000 mm3`), `3D Print: started <program> <arguments>`, `3D Print: <slicer> did not start`, `3D Print: opened <folder>`, `Done 3D Print`, `Command 3D Print cancelled`, `Preferences: slicer <slicer>` (or `automatic`) | 3D Print (mitcad#234: the panel replaced the dialog, whose `3D Print dialog: …` and `3D Print cancelled` lines are gone) |
| `Export dialog opened: unit mm (mm \| cm \| m \| in \| ft)` (with occurrences also `, coordinates design (design \| component)`), `Insert Mesh dialog opened: unit …`, `Insert DXF dialog opened: unit …`, `Units dialog opened: mm (…)`, `Add User Parameter dialog opened: unit mm`, `Grid settings dialog opened: automatic, fixed spacing 10 mm` | a dialog that offers units opened, with its default first |
| `Panel Hole choices: placement=face, …, standard=iso_metric, size=6, designation=M6x1, class=6H`, `Panel Hole options size: 1 \| 1.1 \| …`, `Panel Circular Pattern checks: symmetric=on`, `Panel Rectangular Pattern label distance1: Extent` | a panel's choices and check boxes when it starts; the options of a `withChoices` input when they change; a `withLabel` input's label when it changes (mitcad#545) |
| `Imported block.step as Base1: 1 body(ies)`, `Inserted drawing.dxf into Sketch2: 4 curves, 4 points, 0 texts, 1 profiles, 0 warnings`, `Inserted component block (linked) from block.mitcad` | import |
| `Insert SVG dialog opened: 100 x 60 mm, 2 filled, 1 stroked, 1 texts`, `Insert SVG scale at x,y` (`x`, `y`, `bands`, `OK`), `Insert SVG: scale 1, lower left at (0, 0), bands on`, `Inserted logo.svg into Sketch1: 16 curves, 17 points, 1 texts, 8 profiles, 0 warnings, 3 filled, 1 bands` | the Insert SVG dialog (places in the main window's coordinates), what it chose and what came in (mitcad#473) |
| `Insert DXF drawing: unit mm, layers Outline (4) \| Holes (1) off`, `Insert DXF place lower_left at x,y`, `Insert DXF x at …`, `Insert DXF layer Holes at …`, `Insert DXF OK at …`, `Insert DXF: lower left corner at (10, 5), layers Outline` | the Insert DXF dialog (places in the main window's coordinates) and what it chose |
| `Import of part.f3d started`, `Import item 3: Extrude2 parametric`, `Import of part.f3d: stop requested after 12 s`, `Import of part.f3d stopping: the remaining items take the file's bodies`, `Import of part.f3d cancelled`, `Import of part.f3d failed: …`, `Imported part.f3d: 2 bodies; items …[; stopped at Extrude3 (item 5)]`, `Import report: part.f3d: 28 items (25 parametric, …), 2 bodies, 2 of the file's 2 solids match[, stopped at Extrude3 (item 5)]`, `Import report summary: <the report's text above its list>`, `Import report kept: Pad.Length = Sheet.w * 2: <why>` | an `.f3d` or FreeCAD import (`stopped after the last item` when only the final comparison was left; `kept`: a FreeCAD expression kept as its value) |
| `References dialog: Part version 2 of Assembly v1.f3d` (`?` without a name), `References dialog: chose Part v2.f3d`, `References dialog: skipped`, `Import references of Assembly v1.f3d: 2 found, 0 skipped`, `Imported Assembly v1.f3d: documents referred to: Part linked, Holder skipped` | an `.f3d` design's documents referred to (mitcad#196): a file dialog for one not found, the choice, the documents found before the import, how each came in |
| `Import Assembly dialog opened: top.iam, parts with their design history on`, `Import Assembly: parts with their design history off` (the box toggled), `Import Assembly: top.iam, parts with their design history on` (accepted), `Import Assembly cancelled`, `Import of top.iam: parts with their design history on`, `Import of top.iam: part 2 of 3: cube.ipt`, `Import of top.iam: part 2 of 3: cube.ipt: replayed in 1.25 s` (`bodies`), `Import report part: cube.ipt: replayed, 2 items (2 parametric, 0 partial, 0 fallback, 0 skipped), 2 verified, 1 parameters` (`stored bodies: <reason>`), `Imported top.iam: component cube: 2 features, 1 bodies, volume 1000.000000 mm3` (the volume with `MITCAD_LOG_VOLUMES`) | an `.iam` assembly (mitcad#507): the Import Assembly dialog, the parts' progress, each part's outcome in the report and each component after the import; `tools/ui-iam-history-test.sh`, `tools/ui-windows-iam-history-test.ps1` |
| `Recent files: a.mitcad \| b.step`, `Dropped <paths>`, `Document closed`, `New document` (each after `Opened the new design with Component1 active`, mitcad#898), `File menu: New Design \| Open \| Open Recent > \| … \| - \| Project > [New Project \| … \| Project Settings] \| … \| Exit` (debug, as it opens: `-` a separator, names without underlines and dots, mitcad#175) | the File menu |
| `Remote status: synced` (`ahead N`, `behind N`, `push`, `fetch`, `sync`, `conflict`, `offline`, `sign-in needed`, `remote not found`), `Remote task <command> started\|done\|failed (<class>): <message>`, `Remote push: sent N version(s) to origin/main`, `Remote check: ahead N, behind M`, `Remote notice: <text> [Sync Now, Dismiss]`, `Sync: <case>[ (<class>)], N replayed, sent\|nothing sent`, `Sync conflict: <path> (<kind>): <choice>`, `Sync done: <text>` | the current Cloud project's remote: its state (the indicator shows it), a remote task, a push, a check, the notice, a sync and its conflicts |
| `Computing part.mitcad started`, `Computing part.mitcad: 57 of 196, Extrude6` (when the feature changes, at most 5 a second, after the first 50 ms), `Progress dialog shown: part.mitcad`, `Computing part.mitcad: cancel requested after <ms> ms`, `Computing part.mitcad cancelled after <ms> ms`, `Computing part.mitcad done in <ms> ms (<n> evaluated)`, `Opening part.mitcad cancelled` | a job on the model's worker thread (opening a design, an imported design, New, Close) |
| `Computing set_parameter started` … `Computing set_parameter done in <ms> ms (<n> evaluated)` (a model command's job is named by its `cmd` and logged only when it takes over 50 ms; `Computing preview …` for a panel's preview, which has no progress dialog (mitcad#634) and logs `Computing preview: cancel requested after <ms> ms` when Esc, Cancel or a newer input stops it), `Command set_parameter cancelled`, `Command undo refused: the model is busy computing`, `Preview Extrude: cancelled`, `Extrude: OK cancelled`, `Extrude: OK not done`, `Parameter d1 expression: 70 cancelled`, `Finish Sketch cancelled: Sketch1 stays open`, `Command Extrude: the timeline stays rolled back`, `Drag of p3 ended by a dialog`, `Manipulator Extrude distance dragged to 25 (ended by a dialog)`, `Context menu dropped: the model is computing` | commands and previews as jobs: a cancel, a call that came while a job ran (refused, or waiting until it is done), a drag the progress dialog ended |
| `Display meshes of part.mitcad: 2 bodies in <ms> ms on the model's thread` (also `of <cmd>`, `of preview`, `of preview refinement`; only when it meshed any; a preview's end in ` (interactive)`) | the view's meshes of the shown, previewed and tool bodies made at the end of the job (`framework/DisplayMeshes`, mitcad#398; the job's stage reads "Meshing the bodies for display"). A preview's are the interactive tier's (mitcad#635): stand-ins, copies of its bodies meshed with planes through a faster triangulation (the same nodes and deviation), the bodies themselves left for the final tier |
| `Drag proxy Extrude: shown for distance 12 mm (the exact preview at 5 mm) at 60.000,50.000,26.000`, `Drag proxy Extrude: hidden: <why>` (`the exact preview shows 12 mm`, `a value was typed`, `an input changed the command's structure`, `no exact preview is shown`, `the command was committed`, `the command was cancelled`), `Drag proxy Extrude: not shown: <why>` (`not for these inputs`: a taper, an extent to an object or through all, a wrap, a start at an object, another input changed; `the exact preview's tool is not the prism of its values`), `Drag proxy Extrude distance: 44 inputs, 44 proxy updates (every 64.0 ms median, 81.0 ms p95), 5 exact previews (every 560.0 ms median, 610.0 ms p95), 120 frames in 3120 ms; proxy update 0.150 ms median, 0.400 ms max` (at the release), `Drag proxy Extrude: the exact preview of 27.5 mm replaced it 540 ms after the release`; with `MITCAD_LOG_TIMING=1` each update: `Drag proxy Extrude distance: 12 mm over the exact 5 mm at 60.000,50.000,26.000 in 0.120 ms` | Extrude's provisional tool while a handle is dragged (mitcad#636, `framework/CommandSessionProxy.cpp`): the exact preview's tool changed to the dragged value at once, amber and see-through with dashed edges, where its component's occurrences put it, until the exact preview of that value replaces it. A drag's line counts what was delivered (snapped inputs), the proxy updates drawn, the exact previews shown while the button was held and the frames submitted (`OcctViewer::framesDrawn`, not fenced) apart; `tools/ui-drag-proxy-test.sh` |
| `Drag proxy Move/Copy: shown for tx 12 mm (the exact preview at 0 mm) at 12.000,0.000,10.000` (`rz 30 deg`), `Drag proxy Move/Copy: hidden: <why>` (`the exact preview shows 12 mm`, …, as Extrude's), `Drag proxy Move/Copy: not shown: <why>` (`no motion of these inputs`: an exact preview whose report has no `motion` for Rotate or Point to Position; `not for these inputs`: a value that is not a number; `no exact preview to start from`), `Drag proxy Move/Copy tx: 44 inputs, 44 proxy updates (…), 1 exact previews (…), 120 frames in 3120 ms; …` (at the release), `Drag proxy Move/Copy: the exact preview of 27.5 mm replaced it 120 ms after the release, 0.000000 mm from the proxy` (`37 deg`; the distance only when the proxy was drawn at that value: between where the proxy last drew the first moved body's middle and where the exact preview draws it) | Move/Copy's provisional motion while a handle of Free Move, Translate, Rotate (its ring) or Point to Position (its X, Y and Z arrows) is dragged (mitcad#838, `CommandDef::dragMotion`, `framework/CommandSessionProxy.cpp`, `commands/MoveDragMotion.cpp`): the bodies or components the exact preview moved are drawn where the dragged value puts them, their meshes only placed elsewhere (no model preview, no meshing), until the exact preview of that value replaces them; while the button is held the exact preview waits until the pointer rests for 250 ms. Rotate turns them about the axis and Point to Position moves them from the point the model resolved for the exact preview (its report's `motion`, commands.md; `DragMotion::resolve`, `basisMotion`), so the proxy and the result agree; Rotate's ring itself runs about that reported axis once a preview has reported one (`previewReported` keeps it in the state with the Axis pick it is of, `keptRotateAxis`; before that, or after another axis is picked, about the picked geometry's own axis, `axisOf`), so its angle grows the way the bodies turn even where a circle's own axis points against the model's (Move Face's ring alike, mitcad#854, with no proxy: its body is not moved rigidly); Point to Point has no value to drag. `at`: where the middle of the first moved body is drawn. `tools/ui-move-drag-test.sh` (Free Move's arrow and ring, a component's arrow, Rotate's ring and Point to Position's arrow on a component of 64 pegs) |
| `Preview tool at 60.000,50.000,22.500` (`none`) | where the preview draws its tool, the middle of its box in the design: at each placement of its component's occurrences (the report's `tool_instances`, mitcad#636) |
| `Preview extended tool: 10.0,-3.7,-3.7 to 10.0,43.7,23.7` (`none`) | a split's or trim's tool as the feature extends it (`CommandDef::extendedToolPreview`, mitcad#712): its box in the design, logged when it changes; `none` while Extend is off or the tool is a body or sketch curves |
| `Preview Extrude: refining the display meshes of 2 bodies`, `Preview Extrude: display meshes refined` | a preview shown in the interactive tier, once a drag has ended and no input changed for 250 ms: its bodies meshed in the final tier in a job of their own (`Display meshes of preview refinement: …`), which changes nothing in the model, and drawn instead of their stand-ins (mitcad#635). OK meshes them in its own job when this has not happened yet |
| `Mesh trace preview interactive: 0 bodies ready, 2 meshed, 88 faces reused; faces=99 kept=88 meshed=11 failed=0 build=… edges=… heal=… check=… triangulate=… write=… total=… ms; meshed plane 7, …; slowest plane wires=85 points=11960 31.4 ms, …` (`final` for a command's, an open's or a refinement's), on standard error `mesh-trace: presentation faces=… kept=… meshed=…` | with `MITCAD_MESH_TRACE=1` (mitcad#635): each display meshing of a job (bodies that had meshes or stand-ins, bodies meshed, faces taken from the bodies as shown, then BRepMesh's faces kept and meshed, its phases and the slowest faces; "; planes redone N" when the final tier's check made N planes again by the default insertion after Delabella, mitcad#644), and each meshing a presentation asks for on the UI thread (none is expected for a body a job meshed); `tools/ui-preview-meshing-test.sh`, `app.preview_meshing` |
| `Display meshes of 2.dwg: 4 bodies with faces that could not be meshed (not drawn)` (a warning) | bodies with faces OCCT's mesher failed on when the job meshed them for display: those faces are not drawn, and neither the job nor the view meshes those bodies again (meshing such a face again took without end, on the UI thread, mitcad#589; `viewMeshes` in `view/ViewTessellation.hpp`) |
| `Autosave every 300 s to <folder>`, `Autosave off`, `Autosaved block.mitcad: 2865 bytes to <folder>`, `Autosave removed for block.mitcad`, `Autosave skipped: busy` (or `previewing`), `Autosave of block.mitcad failed: <reason>` | autosave: the timer set at start and by Preferences, a write, the session's files removed, a write put off (`MITCAD_TEST_AUTOSAVE_SECONDS` for tests; `tools/ui-autosave-test.sh`) |
| `Recovery: none`, `Recovery: 2 document(s) found`, `Recoverable block.mitcad: /path/block.mitcad, autosaved 2026-10-05T03:12:00Z, 2865 bytes` (`never saved`; `, the file has changed since`, `, the file is gone`, `, damaged: <why>`), `Recovery: removed the lock of session <id>`, `Recovery: later, 1 document(s) kept`, `Discarded recovery of block.mitcad`, `Recovered block.mitcad from autosave of 2026-10-05T03:12:00Z`, `Recovered session <id> removed`, `Recovering block.mitcad cancelled`, `Recovery of block.mitcad failed: <reason>`, `Save conflict: <path> changed since it was opened`, `Save conflict: overwrite` (`save as`, `cancelled`) | recovery: what was found, each session, the dialog's choices, the earlier session's files removed once this session wrote the document, and Save over a recovered document's changed file |
| `Result store: restored 4, evaluated 2; stored 0 (0 bytes) in 0.1 ms; unpacked 1, skipped 3` (restored results whose shapes were unpacked, and those left packed, mitcad#858), `Display meshes of part.mitcad: 2 bodies (180 faces) from the result store in 12 ms; 0 bodies (0 faces) meshed`, `Display meshes of part.mitcad: stored 2 bodies (81234 bytes) in 9 ms`, `Display meshes of part.mitcad: 2 bodies shown, 51234 triangles` (an open, mitcad#858), `Result store: <error>`, `Result store: removed 12 old results (310.5 MB), 4800.0 MB kept`, `Result store cleared: 4 files, 0.2 MB`, `Import stored 17 results`, `Preferences: results on disk on, at most 5120 MB; in memory at most 7726 MB` | the result store: an open's job (restored from the store, evaluated, written) and a Save's, a write that failed, the clean-up beyond the size, Preferences' Clear, the `.f3d` import's worker, the Cache settings saved |
| `Open part.mitcad: 2221 ms: read 48 ms, recompute 1653 ms (44 restored, 106 evaluated), results stored 0 ms, display meshes 148 ms (4 bodies from the result store, 0 meshed), view 362 ms`, `Open part.mitcad: first frame 2604 ms after it began` | where an open's time went (`MainWindow::installDocument`, mitcad#878), after every open, new design and import into a new document: reading the file (with its links), the recompute (results restored from the store, features evaluated), writing results to the store, the display meshes (`meshOpenedForDisplay`), refreshing the window (presentations, browser, timeline), and once the view drew its next frame, the time from the open's start (`OcctViewer::logFirstFrame`). The total runs from the open's start to the window refreshed; what the parts leave of it is the job's start, the root's activation and the edit lock |
| `Diagnostics: memory 307830 of 8101298176 bytes (11 results), disk 11238 bytes (4 files), process 355545088 bytes; last recompute 5 evaluated, 0 from the store, 1 cached`, `Diagnostics: cleared memory: 7 results, 120000 bytes`, `Diagnostics: cleared disk: 4 files, 11238 bytes`, `Diagnostics report written to <path>` | Help › Diagnostics: shown or refreshed, Clear Memory, Clear Disk, Export Report |
| `Version recorded: part.mitcad abc1234 v3: <summary>`, `Version unchanged: part.mitcad (v3)`, `Version not recorded: <why>`, `Version failed: <error>`, `Version warning: <warning>`, `Version status: bracket, main, v3` (`…, no version`; `none` outside projects with history; when it changes), `Version author: Name <email> (git)` (or `(settings)`), `Version author dialog: git has none` (an older project without an author), `Version author dialog cancelled`, `Save with Description dialog: <automatic message's summary>`, `Save with Description cancelled`, `Save with Description: no version history for part.mitcad`, `Save conflict: <path> changed outside Mitcad (the file)` (or `(a newer version)`), `Save conflict: compare: <the diff's lines joined by " \| ">`, `Save conflict: new version` (`save as`, `cancelled`), `Renamed from a.mitcad: its display state moved along` | saving versions: a commit and what it did, the indicator's version (`Version status`), the author, Save with Description, a change outside MitCad before Save, a rename followed at opening |
| `Version History dialog: part.mitcad, reading its versions`, `Version History: 3 versions of part.mitcad: v3 abc1234 <summary> \| v2 … (renamed from a.mitcad)` (at most 30), `Version History changes: v3 <changes> \| v2 … \| v1 first version`, `Version History selected v2 (abc1234): preview` (or `no preview`), `Version History compare v2 with v1: <heading and text joined by " \| ">` (`with the open design`), `Version History geometry v2 with v1: computing`, `…: <text>`, `…: cancelled`, `Version History open v2 (abc1234)`, `Version History restore v2 (abc1234)`, `Version History save copy v2 (abc1234)`, `Version History closed`, `Version History failed: <error>`, `Version History: no version history for part.mitcad`, `Restore dialog: v2 (abc1234) of part.mitcad` (` (unsaved changes)`), `Restore cancelled`, `Restore: the unsaved changes dropped`, `Version restored: part.mitcad v2 (abc1234) as v4 (def5678)`, `Opened version v2 (abc1234) of part.mitcad as part v2`, `Save Copy As dialog: part v2.mitcad`, `Save Copy As cancelled`, `Saved a copy of part.mitcad v2 (abc1234) as <path>`, `Version preview saved: <blob's 7 digits> (256 x 160)` | Version History: the list, the changes, the selection and its comparison, the geometry, the choice; Restore, Open and Save Copy As; a version's preview |
| `Appearances dialog opened`, `Appearances for bodies: F1.b0 (custom1)` (the bodies' shared appearance; `none` without bodies), `Appearances item chrome at x,y` (the list's rows in sight), `Appearances field roughness at x,y` (also `name`, `base_color`, `emission_color`: their text fields), `Appearances new at x,y` (`delete`, `assign`, `close`), `Appearance selected: chrome`, `Created appearance custom1 (Chrome Copy) from chrome`, `Appearance custom1 roughness = 0.35` (`base_color = #d02020`; `… refused: <why>`), `Assigned appearance custom1 to F1.b0` (faces: `F1.b0 F1:top`), `Appearances assign_bodies at x,y` and `Appearances assign_bodies label: Assign to 2 bodies` (the button next to Assign, only while the selection holds faces; `Assign to 1 body`), `Appearance Red Copy assigned to 2 bodies (from faces)` (mitcad#346), `Deleted appearance custom1`; mitcad#53: `Appearances for faces: F1.b0 F1:top (paint_red)`, `Appearances field texture_path at x,y` (also `texture_width`, `texture_height`, `texture_rotation`, `texture_projection`, `texture_embed`, `texture_remove`), `Appearances face F1.b0 F1:top paint_red at x,y` (the listed face appearances), `Appearances clear_faces at x,y`, `Cleared face appearances of F1.b0 F1:top`, `Appearance checker texture embedded` (`not embedded`, `removed`, `path <path>`, `size [...]`), `Appearance lost texture missing: <why>` | Edit Appearances (`tools/ui-appearance-test.sh`, `tools/ui-appearance-bodies-test.sh`, `tools/ui-render-faces-test.sh`); places in the main window's coordinates |
| `Render body F1.b0: appearance plastic_red` (`default`; with faces of their own `…, 1 faces paint_red`), `Render textures not drawn: <appearance>: <why>, its base colour is shown` (a map of its set, mitcad#314: `<appearance>: the normal map is left out, <why>`) | the rendered view's scene: each body's appearance and its faces' (`tools/ui-render-materials-test.sh`, `tools/ui-render-faces-test.sh`) |
| `Appearances texture sources: folder, polyhaven (1 folders)`, `Appearances source at x,y` (also `search`, `category`; with a texture source `resolution`, `use`), `Appearances source: folder` (`appearances`), `Appearances filtered "brick" (All Categories): 2 shown`, `Texture search folder "brick" (all categories): 1 found` (`…: <error>`), `Appearances texture Red bricks at x,y` (the grid's tiles in sight), `Texture thumbnail Red bricks shown`, `Texture selected: Red bricks (2k)`, `Texture fetching folder red_bricks 2k`, `Texture used: folder red_bricks 2k as custom1 (3 maps; 3 images embedded, 0 shared)`, `Texture not used: <why>`, `Appearance custom1 texture: embedded, maps normal roughness, source folder red_bricks 2k`, `Appearances: texture sources on the internet allowed`, `Preferences texture folder add at x,y` (Add…, answered in Qt's own folder dialog **Texture Folder**), `Preferences: texture folders <folders>` (`none`) | texture sources in Edit Appearances and Preferences › Textures (mitcad#315, `tools/ui-appearance-textures-test.sh`) |
| `Texture source polyhaven attribution: Powered by …`, `Texture request https://api.polyhaven.com/assets?t=textures` (every request of an online source; `Texture request refused: <why>`, `Texture request <host> failed: <why> (HTTP <status>)`), `Texture details: Red Tiles 01: 1k 292 bytes, 2k 1.2 kB`, `Poly Haven: 2 textures` (`… from the cache`; `the cached list of textures, as <error>`), `Poly Haven: red_tiles_01 2k in the cache (3 maps)` | Poly Haven as a texture source (mitcad#313, `tools/ui-appearance-polyhaven-test.sh`) |
| `Render image dialog opened`, `Render image field output.width at x,y` (every output field; also `camera`, `size`, `render`, `cancel`, `save`, `close`), `Render setting output.width = 320` (`output.size = 3840 x 2160` for a preset), `Render image camera Front` (`current view`), `Render image frame x y w h` (the image's frame in the view, main window coordinates; `Render image frame hidden`), `Render image started: 320 x 240, 8 samples, png, current view, 1 bodies (pid <n>)`, `Render image progress 4/8` (debug), `Render image preview (4 samples)` (debug), `Render image done: 320 x 240, 8 samples in 0.21 s`, `Render image cancelled`, `Render image failed: <reason>`, `Render image saved <path>` | Render Image (`tools/ui-render-image-test.sh`) |
| `Render environment studio_dark, background view, ground shadows` (`environment`; `none`, `shadows and reflections`), `Render worker: environment studio_dark, 1 lights` (debug), `Render environment dialog opened`, `Render settings field film.exposure at x,y` (every field by its section and name, also `environment.image`, `background.color`, `ground.lowest`; a check box at its box), `Render settings rendered at x,y` (`reset`, `close`, `page environment`, `page lights`), `Render settings page lights` (the page shown; the places are logged again), `Render setting environment.preset = studio_dark` (`… refused: <why>`), `Render settings reset`, `Render light field power at x,y` (`list`, `new`, `add`, `delete`, `name`, `type`, `enabled`, `camera`, `position.0`…`direction.2`, `color`, `size`, `size_y`, `shape`, `spot_angle`, `spot_blend`, `angle`, `distance`, `aim`, `from_camera`), `Render light light1 added (point)`, `Render light light1 power = 25` (`… refused: <why>`; `space = camera`, `placed at the camera`, `aimed at a face`), `Render light light1: click a face to aim at`, `Render light light1 aimed at x, y, z (normal x, y, z)`, `Render light aim: no face there`, `Render light light1 deleted`, `Render light glyph light1 at x,y` (`tools/ui-render-lights-test.sh`), the environment's ending `, 1 lights` (`, 2 lights (1 off)`), `View message: Rendering: cannot read the environment image <path>: <why>; the studio lights the scene instead.` | the render settings: an environment sent to the worker, Render Environment's places (main window coordinates) and changes, an image the worker cannot read (`tools/ui-render-environment-test.sh`) |
| `Send Feedback: the form shows`, `Feedback screenshot 1280x800`, `Feedback screenshot image at x,y`, `Feedback screenshot image size w h`, `Report screenshot cropped to 568x402`, `Feedback form: kind bug, summary '<s>', description 42 characters, contact given, diagnostics on, screenshot 568x402`, `Send Feedback cancelled`, `Report preview: <title>`, `Report section <id>: <text, lines joined by " \| ">`, `Report section <id> at x,y` (its check box), `Report text <id> at x,y` (its text), `Report key: mitcad-<key>`, `Report section <id> included` (`left out`), `Report not sent: the preview was cancelled`, `Report sent: '<title>', sections description, contact; link 812 characters`, `Report on the clipboard: <n> characters (too long for the link)`, `Report screenshot saved: <path> 568x402`, `Open URL (test, not opened): <url>`; `Internal error (<context>): <masked message>; key mitcad-<key>`, `Crash report of this application's import-worker: SIGSEGV <file>`, `Crash reports of earlier runs: 1, the newest SIGSEGV of the app`, `Error report offered (crash): <message>; key mitcad-<key>` (`worker-crash`, `error`), `Error report no more offers at x,y`, `Error reports: no more offers`, `Error report declined`, `Error report not offered (turned off): <message>`, `Crashing for a test (MITCAD_TEST_CRASH=app)`; on standard error of a crashed process `Mitcad crashed: SIGSEGV; crash report: <file>` | feedback and error reports (`tools/ui-report-test.sh`); places in the main window's coordinates |
| `Sync 12` | with `MITCAD_TEST_SYNC=1` (the Linux and Windows UI tests): the answer to the Pause key (`ui_sync`, `Ui-Sync`), once the input before it is handled, at least 20 ms after it and when no watched timer runs (`framework/TestSync.hpp`) |
| `Project: local Robot arm at <root>` (`cloud … at <root> (origin <url>)`, `no versions … at <root> (inside <repository>)`, `local … at <root> (remotes first, second, none followed)`, `none`), `Project indicator: Robot arm, local, v3` (`…, cloud, synced` / `ahead 2` / `behind 1` / `offline` / `remote not found` / `sign-in needed` / `conflict` / `not synced` / a running task `fetch`, `push`, `sync`; `…, cloud, no remote chosen`: several remotes and none followed; `not in a project`; `…, no versions (inside <repository>)`), `Project indicator at x,y`, `Project indicator menu: Sync Now \| Check for Newer Versions \| …`, `Project indicator menu chose file.sync` | the current project (mitcad#89): when it changes; the indicator's text, its place and its menu (entries as shown) |
| `New Project dialog: <folder offered>`, `New Project dialog: folder <folder>: <kind>[ (inside <repository>)]` (the kinds of `inspect_folder`), `New Project dialog: storage cloud` (`local`), `New Project: creating <folder> (local)` (`(cloud <url>)`), `New Project: created <root>: version abc1234 on main[, sent \| , not sent (<class>)]`, `New Project failed (<class>): <message>`, `New Project: Open It <folder>`, `New Project: Open It Instead <url> into <folder>`, `New Project: Create as Local for Now`, `New Project cancelled`, `New project <root>` | New Project (also with the folder fixed by Open Project, and as Save to Project's new project) |
| `Cloud check started: <url>`, `Cloud check <url>: <what was found>` (`<class>: <message>` for a failure), `Cloud check again: <url>`, `Cloud section: open the new repository page <url>` (`SSH keys page`), `Copied the public key <path>`, `Trust This Server dialog: host: SHA256:… (verified \| mismatch \| not published)`, `Trust This Server cancelled`, `Trusted the keys of <host> in <known_hosts>`, `Trust This Server: <host> refused: <why>` | the Cloud section of New Project, Open from Cloud, Project Settings and Change Address |
| `Open from Cloud dialog: <location>`, `Open from Cloud dialog: clone, enabled` (`create`, `adopt`, `open`, `none`; `disabled: <why>`), `Open from Cloud: <action> <url> into <folder>`, `Open from Cloud: <action> done into <root>[, not sent (<class>)]`, `Open from Cloud failed (<class>): <message>`, `Open from Cloud cancelled`, `Open from Cloud: opened <root> (<action>): <designs>` | Open from Cloud |
| `Open Project: <folder>: <kind>`, `Open Project cancelled`, `Open Project: no designs in <root>`, `Open Project: parts/a.mitcad (opened last)` (`(newest)`), `Version history started in <root>: version abc1234 on main`, `Open Read-Only: <path>` | Open Project and the design it opens, Open Read-Only |
| `Projects panel: shown` (`hidden`), `Projects panel: hidden at the next start` (`shown`; the user's choice written), `Projects panel at the start: shown (no choice yet, projects configured)` (`hidden (no choice yet, no projects)`, `shown (as chosen)`, `hidden (as chosen)`), `Projects panel: 2 projects: gripper \| Shop` (when it changed), `Projects top rows: gripper \| Shop` (the keys of the rows at the top of the tree, which are the projects, in their order; when they changed), `Projects names: gripper = github.com/acme/gripper \| Shop = Shop` (the names shown, `(set)` for a display name; when they changed), `Projects panel: settings of <root> (<key> selected)` (the settings button; `no project`), `Projects panel: settings of <root> (double-click on <key>)`, `Projects panel: Project Settings of the current project <root>` (`of <root> (not the current project)`, `no Project Settings for <root>`), `Project Settings: display name <name>` (`display name cleared: <default>`, `display name refused: <why>`), `Projects tree: Shop (local, current): parts/ \| parts/plate.mitcad v3, changed, open \| base.mitcad v1, locked by Alex` (per project when it changed; states `local`, `cloud, synced` / `ahead 2` / `behind 1` / `offline` / `not synced` / `conflict` / `sign-in needed`, `cloud, no remote chosen`, `no versions`, `missing`, `unreadable`; designs `vN` or `not recorded`, `changed`, `editing`, `editing elsewhere`, `locked by <name>`, `open`; `no designs`), `Projects row <key> at x,y` (rows in sight: `Shop`, `Shop/parts/`, `Shop/parts/plate.mitcad`), `Projects button <name> at x,y`, `Projects width <w>` and `Projects edge at x,y` (the card's or the dock's width and where its edge is dragged; with the rows' places), `Projects text Shop/parts/plate.mitcad: "plate" v3* (lock); tooltip: plate / <path> / v3 · Changed · Editing elsewhere / …` (per row not in a folded folder, when it changed: the name as shown, `…` where it is elided, its marks with the icon in brackets, `none` without, and its tooltip's lines joined by ` / `; mitcad#529), `Projects panel: card width <w> remembered` (`dock width <w> remembered`; a drag of the edge), `Projects panel: card width <w> (remembered)` (at the start), `Projects panel: dock width <w> restored`, `Projects panel selected <key>`, `Projects panel: naming a new design in <key>` (`folder`; logged once the editor is open), `Projects panel: renaming <key>`, `Projects panel: new design parts/a.mitcad in <root>` (`new folder`), `Projects panel: rename <from> to <to>`, `Projects panel: <what> cancelled`, `Projects panel: <what> refused: the name "<name>"`, `Projects panel: dragging <key>`, `Projects panel: drag <from> to <to>`, `Projects panel: drop of <key> refused[ on <key>]`, `Projects panel: open <key>`, `Projects panel: delete <key>`, `Delete dialog: <path> (<n> designs[, open][, changes no version holds])`, `Delete cancelled`, `Delete: changes no version holds: deleted all the same` (`cancelled`), `Projects panel: <command> recorded as abc1234` (`(no new version)`), `Projects panel: <command> refused (<class>): <message>`, `Projects panel: the open design follows: <from> -> <to>`, `Projects panel: closed the deleted design <path>`, `Projects panel: sent the versions of <root>` (`the versions of <root> wait (<class>): <message>`), `Projects panel: show in folder <folder>`, `Projects panel: added <root>`, `Projects panel: removed <root> from the list`, `New Design: in <root>[, folder <folder>]`, `New Design: no project, an untitled design` (then `New document`), `New Design: made <path>`, `New Design cancelled` | the Projects panel (mitcad#145, `tools/ui-projects-test.sh`); places in the main window's coordinates |
| `Project Settings dialog: <name>, local; edit locks on, idle 10 min, poll 10 s; live updates none; author Name <email>; send at once on; check <choice>; live here project` (`cloud <url>`), `Project Settings: storage cloud (share)`, `Project Settings: authors to publish: …`, `Project Settings: share to <url>`, `Project Settings: shared <name> to <url>: versions sent` (`nothing sent`; ` (<class>)`), `Project Settings: share failed (<class>): <message>`, `Project Settings: stop syncing asked` (`cancelled`), `Project Settings: stopped syncing <root> (was <url>)`, `Change Address dialog: <url>`, `Change Address refused: <why>`, `Change Address cancelled`, `Project Settings: address changed to <url>`, `Project Settings: several remotes, none followed: first, second`, `Project Settings: following <name> (<url>), <name>/main` (`follow <name> failed: <message>`), `Project Settings: shared settings recorded: abc1234` (`no change`; `… refused: <why>`), `Project Settings: here: send at once on, check <choice>, live project`, `Project Settings: author Name <email>`, `Project Settings: Sync Now`, `Project Settings closed` | Project Settings |
| `Edit lock: taking part.mitcad[ over]`, `Edit lock taken: part.mitcad[ from Alex (unchanged \| no_receipt \| unanswered \| take_over)]`, `Edit lock granted: part.mitcad by Alex`, `Edit lock held: part.mitcad by Alex[, another session] (since <time>)`, `Edit lock not confirmed (<class>): part.mitcad: <message>`, `Edit locks: part.mitcad: <message>` (a remote that refuses lock refs), `Edit lock: none for part.mitcad (<why>)`, `Edit lock: part.mitcad opened read-only, without an edit lock`, `Edit lock state: editing, part.mitcad` (`editing, idle`, `read-only`, `not confirmed`, `taking`, `none`), `Read-only: on (<reason>)`, `Read-only: off`, `Read-only: refused <command or what>`, `Read-only: unsaved changes of part.mitcad asked about`, `Lock banner: <text> [Request Edit Access..., Edit, Save as Copy..., Save as New Version]`, `Lock banner <button> at x,y`, `Lock banner: <button>` (clicked), `Lock banner: hidden` | the open design's edit lock (mitcad#89, `tools/ui-locks-test.sh`): taken, held by someone else, not confirmed, the window's read-only mode and its banner |
| `Edit lock dialog: <text> [Continue Read-Only, Request Edit Access...]` (`[Take Over, Open Read-Only]`, `[Take the Edit Lock, Not Now]`), `Edit lock dialog: <button>`, `Request Edit Access dialog: part.mitcad, Alex`, `Request Edit Access cancelled`, `Edit lock request sent: part.mitcad to Alex[: "<message>"]`, `Edit lock request seen by Alex`, `Edit lock request declined by Alex[: <message>]`, `Edit lock request: Alex keeps the edit lock until <time>`, `Edit lock request: Alex did not answer (no_receipt): taking the lock`, `Edit lock of part.mitcad is free: taking it`, `Edit lock of part.mitcad released by its holder`, `Edit lock request refreshed: part.mitcad` (a poll kept the waiting request alive) | opening a design someone else holds, Take Over, a holder's offline will (live updates), the requester's side of a request |
| `Edit lock receipt: part.mitcad for Sam`, `Edit lock request: Sam asks to edit part.mitcad[: "<message>"] [Release, Keep 15 More Minutes, Decline...]` (`Release (Save First), Release Without Saving` with unsaved changes), `Edit lock request: also waiting Kim`, `Edit lock request: Release[ Without Saving]` (`Keep 15 More Minutes`, `Decline`), `Decline dialog: Sam`, `Edit lock request of Sam declined[: <message>]`, `Edit lock request of Sam: kept until <time>`, `Edit lock request withdrawn: part.mitcad`, `Edit lock: handing part.mitcad over to Sam (released \| idle \| no answer)`, `Edit lock handed over: part.mitcad to Sam`, `Edit lock handed over: the unsaved changes of part.mitcad stay`, `Edit lock: Sam asks while part.mitcad is idle: handing it over`, `Edit lock request of Sam not answered within 10 min: handing over` | the holder's side of a request |
| `Edit lock idle: part.mitcad after 10 min without activity (no unsaved changes \| unsaved changes)`, `Edit lock idle: saving part.mitcad`, `Edit lock idle: part.mitcad kept, marked idle (autosave off \| waits)`, `Edit lock idle: part.mitcad kept, marked idle (sync: <class>)`, `Edit lock: part.mitcad active again`, `Edit lock refreshed: part.mitcad (idle \| active)`, `Edit lock released: part.mitcad`, `Edit lock request withdrawn: <path>`, `Edit lock release failed (<class>): <message>`, `Edit lock stays: part.mitcad (<class>)`, `Edit lock: the sync of part.mitcad needs a choice before the release: asked`, `Edit lock: Resolve Sync` (`Release Without Sending`, `Keep Lock`), `Edit lock lost: part.mitcad to Alex (<reason> \| removed)`, `Edit lock lost: the unsaved changes of part.mitcad stay`, `Edit locks off: part.mitcad`, `Edit locks off: released <paths>`, `Edit lock: the project stops syncing`, `Edit lock: part.mitcad saved as copy.mitcad`, `Save as New Version: <path> (read-only)` | holding, idle time, releasing, losing a lock |
| `Edit lock poll failed (<class>): <message>`, `Edit locks: the remote answers again`, `Edit lock: a stale request removed: part.mitcad by Sam` (a request its MitCad no longer refreshed, removed by this poll), `Edit lock: a newer version on the remote: checking part.mitcad`, `Updated to Alex's version saved at 14:05.`, `Edit lock: bringing part.mitcad up to date (sync)`, `Edit lock: the remote has newer versions: checking part.mitcad`, `Edit lock: also open: <entries>`, `Edit locks: live updates connected: the remote is read every 120 s`, `Edit locks probe: the remote accepts Mitcad's lock references` (`<message> (<reason>)`), `Project Settings: edit locks accepted by the remote`, `Project Settings: edit locks: <message>`, `Edit lock details: <lines joined by " \| ">`, `Project indicator menu chose Release Edit Lock` (`Edit Lock Details`), `Sync: files someone else holds the edit lock of: part.mitcad (Alex): asked`, `Sync: Send Anyway`, `Sync: not sent (locked files)` | polls, a read-only window following the editor's versions, the lock just taken brought up to date, live updates, the remote's probe, the details, Sync's question |
| `Save to Project cancelled`, `Save to Project: <path> into a new project`, `Save to Project: <path> saved in <root>`, `Save to Project: <folder> is no project with versions`, `Recorded <path> in the project <root>` (the design was in the new project's folder already), `Preferences: cloud: git 2.47.0 at <path>…`, `Preferences: versions by git's user, else Name <email>` (or `nobody`), `Preferences: git …, remote checks every 10 min, each version sent on, live updates on, default broker none` | Save to Project; Preferences' Cloud page |
| `Project picker <title>: a place, reading the projects` (`a design`, `a project`), `Project picker project <key>: local, <root>` (`cloud`, `no versions`; one per project offered; the key is its name, or its folder when another listed project has the same name), `Project picker <title>: 2 projects: Shop (local) \| acme/gripper (cloud)` (once read), `Project picker row <key> at x,y` (rows in sight: `<key>`, `<key>/<folder>/`, `<key>/<design path>`), `Project picker name at x,y`, `Project picker button Another Folder at x,y` (each button), `Project picker selected <key>`, `Project picker target <key>/parts/bolt.mitcad: free` (`exists`, `invalid`; when it changes), `Project picker <title>: chose <path>` (`another folder`, `another file`, `new project`, `cancelled`), `Project picker button New Project at x,y` | the project picker (mitcad#174; places in the main window's coordinates) |
| `Project browser <title>: reading the projects`, `Project browser <title>: 2 projects: Shop (local) \| Team (cloud)` (once read, by name), `Project browser starts at Shop/parts/ (remembered)` (`(current project)`, `(first project)`), `Project browser shows Shop/parts/: sub/ \| gear.mitcad v2, changed, editing elsewhere` (`empty`; each place shown: its sub-folders, then its designs with the words of `Projects tree:`), `Project browser search "hex" in all projects: 1 found: Team/bolts/hex-bolt.mitcad` (`in <project key>`, `0 found`; as typed), `Project browser search scope: all projects` (`this project`), `Project browser read-only: on` (`off`), `Project browser selected <key>`, the places (debug, when they moved): `Project browser folder <key> at x,y` (the folders' tree, rows in sight: `Shop`, `Shop/parts/`), `Project browser item <key> at x,y` (the contents' rows in sight: `Shop/parts/sub/`, `Shop/parts/gear.mitcad`, a search's results too), `Project browser path <place key> at x,y` (the path bar), `Project browser search at x,y`, `Project browser scope at x,y`, `Project browser button <Another File\|Read-only\|Open\|Cancel> at x,y`; `Project browser <title>: chose <path>` (` (read-only)`; `another file`, `cancelled`) | File › Open from Project's project browser (mitcad#530; keys: a project's folder name, its whole path when two have the same; places in the main window's coordinates) |
| `Updates: Mitcad 0.0.0 for linux-x64, the AppImage <path>` (`installed in <folder>`, `announce only: <why>`), `Automatic update checks are off`, `Update checks are turned off by the administrator`, `Update check: the last was at <time>`, `Update check: manifest at <url>` (`releases at <url>`), `Update check: TLS OpenSSL 3.6.5 29 Sep 2026 (openssl backend)` (the TLS library of the first request; `Update check: TLS unavailable (Qt's TLS backends: <list>; Qt built with <version>)`), `Update request redirected to https://<host>`, `Update manifest: Mitcad 0.1.0 of 2026-10-06 (available), 214 bytes for linux-x64`, `Update check: Mitcad 0.0.1 is up to date`, `Update notice (offer): <text> [Release Notes, Install, Skip This Version, Later]` (`(progress)`, `(message)`, `(error)`), `Update notice closed`, `Update: skipping Mitcad 0.1.0`, `Update download: <url> (<n> bytes) to <path>`, `Update verified: <path>`, `Update rejected: <reason> (deleted <path>)`, `Update staged: 0.0.0 -> 0.1.0, <program> when Mitcad has quit`, `Update: replaced <AppImage>`, `Update: started <program> (process <pid>)`, `Update installed: 0.0.0 -> 0.1.0` (`Update not installed: …`), `Update failed: <reason>`, `Preferences: update checks on, channel stable` | automatic updates ([docs/updates.md](../docs/updates.md); `tools/ui-update-test.sh`) |
| `Live: <project>: off (no broker)` (`not allowed in Preferences`, `no versions`), `Live trust question: <project> through <address> [Connect, Not Now]`, `Live trust: <address> connect` (`not now`), `Live: <project>: <address> not trusted (Not Now): not connected`, `Keychain: read <host:port>: found, user <user>` (`not found`, `unavailable (<why>)`), `Keychain: saved <host:port>` (`save … failed (<why>)`), `Keychain: removed <host:port>`, `Live: <project> subscribes through <address>, prefix <prefix>, as <user>` (`anonymous`; `(<connection>, <state>)`), `Live: <project>: cannot connect to <address>: <why>`, `Live: connection c1 (<address>) connected` (`connecting`, `offline, again in <ms> ms`, `failed (auth_failed: <why>)`, `closed`), `Live: <project> subscribed`, `Live: <project> unsubscribed from c1[, which closed]`, `Live: <project>: the remote is checked (version abc1234)` (`connected again`), `Remote check: announced by live updates`, `Remote check: no timer while live updates are connected` (`the timer again (every 10 min)`), `Live: <project> published version abc1234` (`open <path>`, `closed <path>`, `lock <path>`), `Live event: <project>: open <file id> by <name> (editing) in session <id>` (`open … closed by session <id>[ (cleared: offline)]`, `version abc1234 on main by <name>`, `lock <file id> held by <name> (active)`, `lock … released`, `request <kind> <file id> from <name>`, `session <id> online` / `offline` / `left`, `dropped <reason> (<n> so far)`; ` (retained)`; ids are their first 8 characters), `Live notice: Live updates need you to sign in to <host> [Sign In, Not Now]`, `Live notice Sign In at x,y` (`Not Now`), `Live notice: Sign In` (`not now`), `Live sign-in dialog: <address>, user <user>, keychain on` (`off`), `Live sign-in refused: <why>`, `Live sign-in: <address> as <user>[, remembered]` (`cancelled`), `Live: signed out of <host:port>`, `Live: <address> forgotten: <project> disconnected`, `Live test dialog: <address>, prefix <prefix>[, as <user>]`, `Live test: <address>: ok; steps connect ok, sign_in ok, subscribe ok, publish ok, receive ok` (`failed (<class>: <why>)`), `Live: closed[, every connection left cleanly]`, `Preferences: known brokers: <address> (trusted, user <user>), …` (`none`), `Preferences: sign out of <host:port>`, `Preferences: forget <address>` | live updates (mitcad#89, `tools/ui-live-test.sh`); places in the main window's coordinates |
| `Debug log on: <file> (--debug-log)` (`MITCAD_DEBUG_LOG`, `Preferences`), `Debug log off: <file>`, `Debug log not started: <why>`, `Debug log turned off from the project indicator`, `Preferences: debug log on` (`off`), `Project indicator: …, debug log on`, `Open Log Folder: <folder>`, `Report section log: 150 lines`, `Report section logfile: <file>`, `Report section logfile included` (`left out`), `Report debug log: <file>` | the debug log (mitcad#269): started and stopped (the file's own first and last lines too), Preferences' switch, the indicator, Help > Open Log Folder (with `MITCAD_TEST_LOG_URLS` the folder's address is logged, not opened), a report's recent log lines and log file |
| `Wait cursor on: set_parameter (depth 1)`, `Wait cursor off: set_parameter (depth 1)`, `Wait cursor at idle: none set`, `Wait cursor left on after <job>: cleared` (a warning; ` (2 override cursors)`), `Wait cursor left on for a test: test hook` | the wait cursor of a job the window waits for (mitcad#268): set, and restored once by the progress dialog or at the wait's end (N counts the waits holding one, nested waits too); the first idle check after a wait; the idle recovery of one left on; the test hook (`MITCAD_TEST_WAIT_CURSOR=1`) |
| `Style: <style> Chrome: floating (the preference)` (`(--chrome)`, `(--chrome; the preference floating)`, `(the platform's default)`), `Layout preference: floating (restart asked)` (`(in use)`: the window's layout already), `Layout restart: now` (`later (floating at the next start)`), `Restart: closing the window`, `Restart cancelled: Mitcad stayed open`, `Restart (test, not started): <program> <arguments>` (`MITCAD_TEST_NO_RESTART=1`), `Restart: <program> <arguments>`, `Restart: started <program> (process <pid>)` | the layout preference and the restart (mitcad#419; `tools/ui-chrome-layout-test.sh`, `tools/ui-windows-chrome-layout-test.ps1`) |
| `Floating cards: window 1028x749, top bar bottom 21, view top 21, floating rows bottom 172; titleCard 12,33 1004x52; ribbonGroup_SOLID/CREATE 12,93 180x67; ribbonGroup_SOLID/MODIFY 200,93 120x67; projectsDock 12,180 260x200; browserDock 12,388 260x300; timelineDock 12,688 1004x48; commandDock 716,370 300x200`, `Floating view between the cards at 196,126` | the Floating layout's visible cards (main window coordinates, also for cards that are windows), the rows' cards first (mitcad#425: the title card, the group cards with spaces in their names as `_`, the notice card), the bottom of the bar above the view (the menu bar, the header on macOS), the view's top and the lowest row's bottom (the bar's bottom without rows), when it changed (mitcad#419: no card's top above the bar's bottom; mitcad#425: on Windows and Linux the view's top is the menu bar's bottom, and no other card's top is above the rows' bottom); and the middle of the gap between the first two cards of a row, where a click reaches the view (`tools/ui-chrome-layout-test.sh`, `app.ui-windows-chrome-layout`) |
| `Export dialog: Web page (*.html) to <path>`, `Exported <path>: html, 1 body(ies), 123456 bytes` (the page's size; each texture image left out a warning, `Export: …`), `Export of <path> failed: <reason>` | Export › Web page (mitcad#423, [Web page](#web-page)) |
| `Drawing dialog opened: format pdf, sheet A4, scale fit`, `Drawing dialog file\|format\|sheet\|scale\|OK at x,y` (in the dialog's own window), `Drawing dialog: format = svg`, `Drawing dialog: sheet = A3`, `Drawing dialog: svg A3 to <path>`, `Exported <path>: drawing, 3 views, 2 bodies, A3, 1:1`, `Export drawing: <warning>` | File › Export Drawing (mitcad#471, [Drawing](#drawing)) |
| `Dialog folder import: <folder>`, `Dialog folder import remembered: <folder>` (`open`, `export`, `import`, `texture`, `program`) | where a file dialog of the group starts, and the folder it remembers when accepted ([File dialogs' folders](#file-dialogs-folders), mitcad#498) |
| `Typed name kept in Open: deselected <folder>` | Qt's own file dialog had other entries selected than the typed name (a folder the typed path passed through); they are deselected so that Enter or Open takes the typed name (`framework/Dialogs.cpp`) |
| `Parameter suggestions for Extrude Distance "thi": thickness, thickness2 (A length: …); actions: name, create` (`none` when nothing matches), `Parameter suggestion thickness at x,y`, `Parameter suggestion action create at x,y` (`action name`), `Parameter suggestion highlighted: thickness2`, `Parameter suggestion inserted thickness into Extrude Distance: width / 2 + thickness` (`browser`, `created`, `existing` instead of `suggestion`), `Parameter suggestion tilt refused: <why>`, `Parameter suggestions closed for Extrude Distance (Esc)`, `Parameter fx Extrude Distance at x,y`, `Parameter browser opened for Extrude Distance`, `Parameter naming for dimension d1: width = `, `Create User Parameter opened for dimension d2: name gap, unit mm`, `Create User Parameter gap refused: <why>`, `Created user parameter gap = 12 mm (mm) for dimension d2`, `Create User Parameter cancelled for dimension d2: <the field's text>` | expression fields' suggestions, fx browser, `name = …` and Create User Parameter (mitcad#567, mitcad#568, [Notes](#notes-on-particular-commands)); the field is named `<command> <input label>`, `dimension <its parameter>`, `sketch dimension` (a new one), `sketch <tool value>` or `<parameter> expression` (the table); places in the main window's coordinates |

Test logs are switched on by environment variables
(`framework/Diagnostics.hpp`): `MITCAD_LOG_PICKS`, `MITCAD_LOG_VOLUMES`
(the UI test libraries set both), `MITCAD_LOG_TIMING`,
`MITCAD_LOG_ORIENTATION_CUBE`; `MITCAD_TEST_SYNC` answers the sync key;
`MITCAD_LOG_TYPED_TEXT` logs typed-in text fields (`File dialog Save As:
/path/part.mitcad`, `File dialog Save As: accepted`, `Dialog field
Parameters: 25 mm`, `Panel field sets.0.radius: 2`;
`framework/TypedTextLog.hpp`); `MITCAD_LOG_KEYS` logs the key presses the
application takes, with the modifiers it saw (`Key log on`, `Key press: F9
with shift (native 120)`; the Windows UI tests hold the modifiers until
then, mitcad#271).
`MITCAD_SETTINGS_DIR` keeps the settings in an INI file of a test's own
(on Windows they are otherwise in the registry).

### The debug log

Every message (`qDebug`, `qInfo`, `qWarning`, `qCritical`) goes through
one handler, the log ring (`mcp/LogRing.hpp`), which `main()` installs
for the whole run after the UI test driver's and which passes every
message on to it (and so to stderr). It keeps the newest 5000 lines for
live MCP's `ui_log` and the problem reports, and passes each line to the
debug log (`framework/DebugLog.hpp`, mitcad#269) while that writes a
file. Preferences' Diagnostics page switches the debug log on and off at
once (saved as `diagnostics/debugLog`, so also at the next start);
`--debug-log [FILE]` (`--debug-log=FILE`, or a following argument ending
in `.log`) and `MITCAD_DEBUG_LOG=1|<file>` switch it on for one run. A
line of the file: `2026-10-14 09:30:12.345 debug    [model worker]
Computing set_parameter started` (local time in ms, the level, the thread
when it is not the GUI thread, the text; further lines of a message
indented). Each file begins with a header (`Mitcad debug log`,
`Version:`, `Build:`, `System:`, `Graphics:`, `Command line:`, `Design:`,
`Switches:`, `Started: …, process <pid>, file <n>`), the first file of a
start then with the ring's earlier lines between `--- The newest <n>
lines from before the debug log started:` and `--- The debug log
starts`. A log switched on before the 3D view exists (`--debug-log`,
`MITCAD_DEBUG_LOG`, the setting at start-up) makes its file at once and
writes it when the view logs its renderer (the fact `Graphics`, set with
`OpenGL renderer: …`), so its header has `Graphics: <renderer>, OpenGL
<version>`; after 5 s without it, at the first warning or critical
message, or when stopped, the file is written with `Graphics: not known
yet (…)`, padded with spaces that the renderer overwrites if it comes
later. The files are `mitcad-YYYYMMDD-HHMMSS-<pid>.log` in
`AppLocalDataLocation/logs` (`MITCAD_DEBUG_LOG_DIR` moves the folder,
for tests), 20 MB each (`--- Continued in <next>`), at most 10 files and
200 MB in the folder, the oldest removed first; written at least once a
second and at every warning. A log may contain file paths, design names
and entity names; typed text and pick places only with
`MITCAD_LOG_TYPED_TEXT` and `MITCAD_LOG_PICKS`. The test variables keep
their environment switches (no check boxes in Preferences).

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
  manager). After a dialog that works on a thread before it closes (New
  Project's Create, Open from Cloud), call `ui_focus_main` once its result
  is logged: the main window focused while the modal dialog is still open
  is left without Qt's activation when the dialog closes, and its
  shortcuts (S) do not work.
- `ui_focus_replace "Save As"`: Qt's own file dialog asks before saving
  over an existing file in a message box with the file dialog's own
  title (not "Confirm Save As", a native dialog's), so
  `ui_focus_dialog` cannot tell the two apart; this focuses the newer of
  the two windows (Alt+Y replaces).
- `ui_mark` and `ui_expect_new "text" [seconds]` (only lines logged since
  the mark).
- `ui_click_pick "face F2.b0/F2:end"` (clicks where the last `Pick
  places:` puts the first item whose line starts so), `ui_pick_at`
  (prints the place).
- `ui_drag_from "Manipulator Extrude distance" dx dy`.
- `ui_type_in "Panel Hole input diameter" "8"` (selects the field's text
  and types; `--` lets "-25" through).
- `ui_choose "Panel Extrude input operation" 2` (the entry two below the
  current one; -1 the one above).
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
Open Recent) and imports a corpus design when
`MITCAD_F3D_CORPUS` is set.

**Windows** (the build VM): `tools/ui-windows-lib.ps1` has the same
helpers in PowerShell (`Ui-Key "ctrl+z"`, `Ui-Type`, `Ui-ViewClick`,
`Ui-SketchClick`, `Ui-ClickLogged`, `Ui-ClickPick`, `Ui-TypeIn`,
`Ui-FocusDialog`, `Ui-ExpectNew`, `Ui-ExpectVolume`, `Ui-WheelLogged`,
...). They post mouse and keyboard messages to the window, read the same
log lines, and give commands without a default shortcut one in the
test's settings (`Ui-Init ... @{'sketch.create' = 'F7'}`), since popups
close at once in session 0; a panel's drop-down changes with the wheel
(`Ui-WheelLogged "Panel 3D Print input format" -1`: the next entry). `tools/ui-windows-workflow-test.ps1` (ctest
`app.ui-windows-workflow`, with `MITCAD_MESA_DIR`) is the workflow test.

## Printed fit fields

New Thread, Gear, Pulley and Bearing panels pre-fill their fit fields with
the ordinary user-parameter names in the model's
[`Printed fit parameters`](../core/model-api/src/commands.md#printed-fit-parameters)
table. Thread offers radial and flank clearance, Gear backlash, a seated
Gear or Pulley bore clearance, and Bearing radial clearance, axial
clearance, element gap and the fit clearance used when seated.

The expression field shows the value even before the parameter exists.
Typing a number or another expression replaces the suggestion; clearing
or changing a field keeps that text across refreshes. Editing an existing
feature loads its stored values and expressions. OK creates missing
referenced fit parameters with their comments in the feature's undo step;
Cancel creates none. Change Parameters lists these normal user parameters
and their printer-related comments. A parameter already present is used
without resetting it. The plain Hole panel keeps its existing fields.
In the Bearing panel a typed number 0 in Element Gap or Element Diameter
still leaves the model's choice (the field is left out of the
definition); an expression naming a parameter stays in it even when the
parameter is zero.

## Parameters as CSV

Change Parameters has **Import...** and **Export...** at the left of its
bottom row (`ParametersDialog::importParameters`, `exportParameters`;
mitcad#666). The model reads and writes the CSV
([commands.md, "Parameters as CSV"](../core/model-api/src/commands.md#parameters-as-csv));
the dialog only reads and writes the file (UTF-8) in Qt's or the
platform's file dialog of the Import or Export group
(`framework/DialogFolders`).

- **Export...** asks for a file (`Export Parameters to CSV`, suggesting
  `parameters.csv`) and writes the `parameters_csv` query's table:
  MitCad's own (`name,kind,unit,expression,value,comment,favorite,min,max,step,strict`,
  with the limits of mitcad#665). The status bar says how many parameters
  it wrote.
- **Import...** asks for a file (`Import Parameters from CSV`), MitCad's
  table or the parameter table CSV of the `.f3d` application
  (`Name,Unit,Expression,Value,Comments,Favorite`), and shows the
  `parameters_csv_preview` query's answer in the **Import Parameters**
  dialog (`parametersImportPreview`): the file and its format, the counts
  (`5 to add, 1 to change, 1 renamed, 1 refused, 1 skipped`) and a row per
  row of the file: its name (`mil → mil_1` when renamed), the action
  (Add, Change: expression, limits, comment, Unchanged, Refused, Skipped),
  the expression and the value after the import (`35 mm (limited by its
  max)`), the limits after it (`min 10, max count * 20 mm, step 0.5 mm,
  strict`), and a note (the reason, `From
  its value: <why the expression was not used>`, what a model parameter
  kept); refused and skipped rows in red. Nothing changes until **Import**
  (the default button; disabled when nothing would change) runs
  `import_parameters_csv`, one undo step `Import Parameters`; Cancel
  leaves the design as it is. A table that is not a parameter CSV shows
  the model's reason under the table, and nothing opens.
- Import is disabled while a sketch or a command's panel is open
  (mitcad#564: they hold parameters); Export works then too.

Logs: `Parameters import at x,y`, `Parameters export at x,y`, `Exported
parameters to <path>: 3 parameter(s)`, `Parameters import preview of
table.csv: <counts>`, `Parameters import row d1: change (expression) Width
+ 5 mm = 35 mm`, `Parameters import row mil as mil_1: add 2 mm = 2 mm`,
`Parameters import row Label: skip: <reason>`, `Parameters import row
Broken: add 12 mm = 12 mm, from its value: <reason>`, `Parameters import
row d2: change (limits) 40 mm = 35 mm, limits: max 35 mm, limited by max`,
`Import of parameters
from table.csv cancelled`, `Imported parameters from table.csv: <counts>`,
`Import of parameters from table.csv refused: <reason>`. Test:
`tools/ui-parameters-csv-test.sh`.

### Timeline feature inspection (mitcad#695)

`TimelineWidget::featuresHovered` previews the active feature or every active
member of a folded group. Leaving, rebuilding, dragging or blocking the strip
clears the preview. Failed, suppressed and rolled-back entries never reuse
cached geometry. The strip has a 350 ms tooltip timer of its own.

The tooltip and idle selection card share `FeaturePresentation`: the command
registry supplies the feature kind and its loader supplies the main visible
dimensions, with their editor labels. The component is named from the snapshot.
Timeline selection retains its established body/sketch/datum selection semantics;
the card adds the selected timeline feature names rather than exposing uids.
Debug selection and highlight logs retain uids for diagnosis.

`HighlightDisplay::throughSolid` adds a subdued, dashed, non-pickable boundary
in OCCT's Topmost layer, whose independent depth buffer makes enclosed or rear
features visible. The ordinary depth-tested shaded highlight is retained. This
inspection overlay does not alter body visibility or ordinary viewport picks.
The portable `timeline-inspection.mitcad-ui` fixture checks real tooltip/card
widgets and compares the actual OCCT framebuffer for two boxes wholly enclosed
by an outer body, in both Docked and Floating layouts.

**A Fillet's or Chamfer's picked edges (mitcad#957).** With a Fillet or a
Chamfer selected on the timeline (or in the browser), and while the pointer
is over its timeline cell, the edges it picked are drawn as selection lines
where they were before the feature, although the rounding or the bevel took
them away: in the selection colour, the parts behind or inside the solid
dashed through it (as `throughSolid`, the lines' copy in the Topmost layer).
The feature's own faces stay highlighted, in amber (lighter on the hover)
so that they stand apart from the lines; other features' faces keep the
selection colour. The lines are display only: they take no clicks and the
selection does not change. The model is not rolled back and nothing is
rebuilt: the feature's build kept the edges as polylines at the view's
display tolerance, and the `feature_inputs` query only reads them
(`core/model-api/src/commands.md`, "Feature inputs";
`MainWindowFeatureInputs.cpp`, once per refresh of the scene). A feature
whose build kept none (one not built since the document was read without
building) shows no lines until its next build; selecting it starts
nothing. The card names the count first among the sizes it shows
(`Fillet: Fillet8`, `Component: Root`, then `12 edges`; `1 edge`); the
tooltip too. Logged `Feature inputs
highlighted: Fillet1 edges 4` (the selected features' names and edge
counts; `none`) when it changes. Tests: `tools/ui-timeline-test.sh` (a
Fillet of four edges, a Fillet and a Chamfer of one edge each),
`timeline-inspection.mitcad-ui` (`app.ui-linux-timeline-inspection`,
`app.ui-windows-timeline-inspection`: a Fillet of four edges selected, its
card's count, the view changed); `tools/ui-feature-inputs-look.sh` takes
the one-off pictures (a filleted and a chamfered block, front and back,
light and dark background).

Agents should name features and use `group_features` for contiguous modeling
stages, as described in [the MCP guide](../docs/mcp.md#keep-an-assistant-built-timeline-readable).

## Tables (mitcad#667)

Change Parameters has two tabs, **Parameters** (Alt+P: the table above)
and **Tables** (Alt+T; `browser/TablesPage.*`): the design's tables of
data that formulas look up with XLOOKUP, MATCH and INDEX
([docs/table-lookups.md](../docs/table-lookups.md); the model's side:
[commands.md, "Tables" and "Tables as CSV"](../core/model-api/src/commands.md)).
The tab works while a sketch or a panel is open, as the Parameters tab
does (its commands go through `DocumentHost::runParameterCommand` then;
a panel's expressions follow a renamed table). The note about what is
being edited stands above both tabs.

- **The list** on the left (`tablesList`), from the `tables` query; New
  (Alt+N, the add icon), Delete (Alt+D, the delete icon) and Rename (Alt+R)
  above it, as the Parameters tab has User Parameter and Delete (+ and −
  on macOS, `mac::styleListButtons`), Import CSV… (Alt+I) and Export CSV…
  (Alt+X) below it. **New** asks for a name (`New Table`, `Table1` by
  default; a table's name is taken from the parameters' namespace, so the
  model refuses a parameter's) and makes the table with a `Key` column of
  texts, a `Value` column in the document's length unit and one empty row
  (`set_table`). **Rename** (`Rename Table`) runs `rename_table`: the
  formulas follow. **Delete** runs `delete_table`; the model refuses it
  while a parameter uses the table, naming them, and the reason shows
  under the grid. With no table the right side says so, with an example
  formula.
- **The selected table** on the right: its comment (Alt+O), Add Row
  (Alt+W, after the current row), Remove Row (Alt+V), ▲ ▼, Add Column
  (Alt+L, after the current column, in the length unit), Remove Column
  (Alt+M), ◀ ▶, and the grid (`tablesGrid`). Each header cell holds the
  column's name (`tablesColumnName`) and a unit box (`tablesColumnUnit`:
  mm, cm, m, in, ft, deg, rad, no unit, Text, and a unit read from a file
  such as `mm^2`). Enter in a column's name or in the comment applies it
  (`rename_table_column`, `set_table`, as the field loses the keyboard)
  and goes on in the grid: it is never the dialog's OK. The unit changes
  only by a choice in the box's open list (Alt+Down, F4 or Space opens
  it): the wheel, the arrow keys and letters do nothing while it is
  closed. A new unit converts the column's numbers so that the values stay
  (25.4 mm becomes 1 in; rounded to 12 significant digits, so mm → in → mm
  gives the numbers back), and the status bar says so (`dk: mm → in, the
  numbers converted (25.4 → 1). Undo brings the old numbers back.`);
  between no unit and a unit, or between lengths and angles, the numbers
  stay; into Text a number becomes its text, out of it a text that is a
  number becomes the number (others are refused by the model). Tab goes
  from the comment through the names and units to the grid.
- **Cells**: typing over a cell (or F2, a double-click) edits it; Enter
  commits, Enter again goes to the cell below (never the dialog's OK);
  Delete empties it. A Text column's cell is its text; a number column's
  cell is a plain number in the column's unit (`8,5` too), or a number
  with a unit of the column's kind, converted (`0.25 in` in an mm column
  is 6.35, as the CSV import reads it); an empty one is null; anything
  else goes to the model as typed, which refuses it. Every commit, and
  every row, column and unit change, is one `set_table` of the whole table
  (one undo step, `Edit Table Screws`); a commit that changes nothing runs
  nothing. A refused cell or column name keeps what was typed in red with
  the model's reason as its tooltip and under the grid (`Screws, row 2,
  column dk: <reason>`), as a refused parameter cell does, until it is
  typed again; the document is unchanged.
- **Used by** names the parameters that use the table (`used_by`), and
  the query's `problems` (first cells that repeat) show in red below it.
- **Import CSV…** (`Import Table from CSV`) reads the file and shows
  **Import Table** (`tableImportPreview`) from `table_csv_preview`: the
  table's name (the file's name made a valid name, editable; the preview
  follows a new one), whether it makes a new table or replaces one (`Makes
  a new table Screws.`, `Replaces the table Screws (2 row(s)).`), the
  summary, the columns with their units (a renamed
  column as `<the file's name> in the file`, an inferred unit as `unit
  read from the cells`), the rows, the refused cells in red, the
  parameters whose values change (`d3: 10 mm → 9 mm`) and those that would
  fail. Refused cells and failing parameters refuse the import: Import is
  then disabled, as it is for a file that changes nothing. **Import** runs
  `import_table_csv` (one undo step, `Import Table Screws`); Cancel leaves
  the design as it is. The configuration options an import or a table edit
  moved off removed values (`moves` of the answer, mitcad#806, #849) show
  in the status bar and are logged as `Configuration moves: <notice>`.
- **Export CSV…** (`Export Table to CSV`, suggesting `<name>.csv`)
  writes the `table_csv` query's text.
- **Completion.** The parameter suggestions of expression fields
  (`ParameterField`) list the tables matching the name typed after the
  parameters (`Screws`, its columns as the value); choosing one puts
  `Screws.` in and the list goes on with its columns; after `Screws.` the
  list offers the columns matching what is typed (`columnFragmentAt`,
  `matchColumns` in `ParameterSuggest`). The Formula Editor completes as
  it is typed (`formulaCompletions`, a list under the caret without the
  keyboard): a name offers the parameters, the tables (`Screws.`) and the
  functions (`XLOOKUP(`) starting with it, `Screws.` the table's columns;
  Up and Down choose, Enter or Tab puts the choice in, Esc closes the list
  only. The list opens only on typed text (Backspace updates a list that
  is open); a caret moved by the arrows or a click closes it, so Up, Down,
  Enter and Tab stay the text's (a new line in a long LET). A `(` or `.`
  that already follows the name is not put in twice (`MAX|(` gives
  `MAX(`, `Scr|.dk` gives `Screws.dk`; the fields' suggestions do the
  same for a table). The column lists show units in the unit box's words
  (`mm`, `no unit`, `Text`). Ctrl+Enter is the Formula Editor's OK (Enter
  is the text's). The tables come from `formula_help`'s `tables`; XLOOKUP,
  MATCH and INDEX are reserved names and in its function catalogue.

Logs: `Parameters tab Tables at x,y`, `Parameters tab shown: Tables`,
`Parameters formula at x,y`, `Tables: Screws (2 columns, 3 rows)` /
`none`, `Table Screws columns: Size [Text], dk [mm]`, `Table Screws rows:
[["M4",7],["M5",8.5]]` (the cells as JSON), `Table Screws used by: d3` /
`none`, `Table Screws refused cells: row 2 dk: abc (<reason>)` / `none`,
`Tables new|rename|delete|import|export|add row|remove row|row up|row
down|add column|remove column|column left|column right|comment at x,y`,
`Tables list Screws at x,y`, `Tables column 1 name|unit at x,y`, `Tables
cell 2 1 at x,y` (row, column, from 1; cells in sight), `New Table dialog
opened: Table1`, `Table Screws created`, `New table Screws refused:
<reason>`, `Table Screws row 2 dk: 8.5`, `Table Screws row 2 dk: abc
refused: <reason>`, `Table Screws changed parameters: d3` / `none`,
`Table Screws column 1 renamed: Key -> Size`, `Table Screws column 1
name: x y refused: <reason>`, `Table Screws column dk unit: mm -> in`, `Table Screws: dk: mm → in, the
numbers converted (25.4 → 1). Undo brings the old numbers back.`,
`Table Screws row added|row removed|row moved|column added|column
removed|column moved: …`, `Table Screws renamed to Bolts`, `Deleted table
Screws`, `Delete table Screws refused: <reason>`, `Table import preview
of Screws.csv as Screws: <summary>`, `Table import column dk [mm]` (`
(Head diameter in the file, unit read from the cells)`), `Table import
rows: [[…]]`, `Table import refused line 3 dk: <reason>`, `Table import
changes d3: 10 mm -> 9 mm`, `Table import fails d3: <error>`, `Imported
table Screws from Screws.csv: <summary>`, `Import of a table from
Screws.csv cancelled|refused: <reason>`, `Exported table Screws to
<path>: 3 row(s)`, `Parameter suggestions for d3 expression
"Screws.d": d, dk`, `Parameter suggestions for … "Scr": …; tables:
Screws`, `Parameter table inserted Screws. into …`, `Parameter column
inserted dk into …`, `Formula completions for "Screws.": Size, dk`,
`Formula completion inserted dk: <text>`, `Formula completions closed
(Esc)`, `Formula editor OK|Cancel at x,y`. Tests:
`tools/ui-tables-test.sh`, `app.ui-windows-tables`
(`tools/ui-windows-tables-test.ps1`, by the keyboard), `parameter_suggest`
in `test_app` (`tableTests`).

## Formula builder (mitcad#668)

`ParameterSource::query` lets expression fields use the read-only `formula_help`
query. The Rust tokenizer finds the active call even while the formula is
incomplete; the AST supplies sub-expression ranges, units and errors, retaining
enclosing `LET` bindings. The function catalog comes from the same set of
functions the parser accepts. Qt converts UTF-16 cursor positions to UTF-8 byte
ranges at this boundary.

`FormulaEditor` is shared by the fx list, a field's **Edit Formula…** context
action and Change Parameters' **Edit Formula…** button. It contains searchable
functions, plain-text templates, multiline text, **Indent LET**, a live signature,
selected/hovered sub-expression values, error ranges and actual parameter limits.
It returns formula text and, when limits change, a `set_parameter` command;
the existing parameter-command route retains the normal undo and edit ownership.
A field's `name =` stays before the edited formula: the prefix ends where the
`name =` match ends (`expressionStart`), not where the expression's text first
occurs (`t2 = 2` stays `t2 = …`, mitcad#755); the field's signature help
measures its cursor from there too. A field's tooltip shows the value of the
part under the pointer where `formula_help` gives one, else the field's own
tooltip (the parameter's name, expression and comment, mitcad#761).
No builder-specific state is stored in a design.

The portable `formula-builder.mitcad-ui` fixture runs through both entry points,
checks a partial signature, the real model value of a selected ratio, a template
and saved limits, and leaves an edited command field for normal confirmation.
The same fixture is registered on macOS, Windows and Linux with the existing
platform UI infrastructure. Root runs the implementation platform's checks.

## Connect AI Assistant (mitcad#682)

`help.connect_assistant` is an Any-mode action, visible in Help, command
search, the agent indicator (mitcad#710) and Preferences › AI Assistants
(`AgentPreferencesBox`: Connect AI Assistant… and the list **Folders for
designs** (`agentFolders`, the same accessible name, filling the page's
height) with **Add…** (`agentFolderAdd`, a folder chooser) and **Remove**,
no paragraph of help). Textures and Fonts have the same folder row
(`textureFolderAdd`, `fontFolderAdd`; mitcad#727): no typed field; and,
as AI Assistants (mitcad#732), a heading for the list, **Texture folders**
(`textureFolders`) and **Font folders** (`fontFolders`), the same
accessible names, the list filling the page's height, no paragraph of
help. The shared dialog offers Claude Desktop,
Claude Code, ChatGPT / Codex (local MCP), and other clients; its choice is
labelled ChatGPT / Codex (`assistantName`), its sentences and the connected
clients say Codex (`assistantSentenceName`, mitcad#724). It resolves the installed native CLI rather than depending on a
terminal's working directory or a development checkout.

Layout (mitcad#706, `app/mcp/ConnectAssistantDialog.cpp`): a row of four
checkable tool buttons with icons (`assistant-desktop`,
`assistant-terminal`, `assistant-chat`, `assistant-other`; the last choice
is remembered in `assistant/client`), one bold state line
(`assistantConnectionState`) beside one primary button (`assistantSetup`,
the dialog's default, styled as the primary button), **Connect anyway**
(`assistantConnectAnyway`, Claude Desktop and Codex when they were not
found: the same setup), an optional result line below them
(`assistantSetupResult`: a backup's name; an error is shown in the state
line itself in the error colour, its detail in the tooltip), **Also
connected: …** (`assistantAlsoConnected`, clients of other assistants), the
folder line **Folder for designs: *path*** (`assistantFolder`, elided to
its width, the whole path and its purpose in the tooltip; **Change…**
`assistantChangeFolder`) and the
**Advanced** disclosure (`assistantAdvancedToggle`, `assistantAdvanced`):
**Codex settings file** (`assistantConfigTitle`, `assistantConfigPath`,
Codex only, native separators; **Change…** `assistantChooseConfig`, its
chooser titled **Codex Settings File**), two
labelled read-only fields with a **Copy** button each (each label without a
colon above its field, its button at the right of the label's row, the
settings file's too, mitcad#727), **Configuration** (`assistantConfiguration`, fixed font, its lines' height
up to 200 px; `assistantCopyConfiguration`, hidden for Other, whose
primary button copies it) and **Command**, **PowerShell command** on
Windows, whose commands are PowerShell's (`assistantCommandBox`, `assistantCommand`;
`assistantCopyCommand`; hidden for a Codex file not named `config.toml`),
and, with several live MitCad windows, **Copy Command for This Window**
(`assistantCopyWindowCommand`, `--instance` of this process's endpoint).
Both show the bridge with native separators (JSON escapes, a TOML literal
string) and quote only the words that need it (`assistantShellWord`).
Every layout change fits the window's height to its contents, both ways;
its width only grows (`refit`). No paragraphs of help: the explanations
are in `docs/mcp.md`.

Connected clients are named by `assistantDisplayName`: the assistant
whose label `assistantLabelMatches` (`claude-ai` is Claude Desktop), in
its sentence form (Codex), else the client's own label. Disconnect ends
the chosen assistant's clients only (`Host::disconnect` with their
`ConnectedAgent::id`s; named "Disconnect *assistant*" while others stay
connected, by the names Connected shows: for Other the clients' own
labels). The settings files
(`~/.claude.json`, the Codex TOML) are read again only when their size or
modification time changed.

The state line's `assistantState` property and the button's
`assistantAction` property, refreshed every second:

| `assistantState` | Line | `assistantAction` (text) |
|---|---|---|
| `failed` | the error, saying what to do | `retry` (Try again); none when the installed CLI is missing; `install` (Get Claude Desktop) for Install Claude Desktop first, when no application takes the extension (on Windows: no `.mcpb` association, `AssocQueryStringW`, checked before opening and every second after) and Claude Desktop was not found either |
| `install-extension` | Install the extension in Claude Desktop (mitcad#763): Claude Desktop was found but no application takes the extension, as with a Microsoft Store install, which registers only `claude://`; the result line below says In Claude Desktop: Settings › Extensions › Advanced settings › Install Extension…, then choose *path* (cleared once Claude Desktop connects) | `reveal` (Show in Explorer, Show in Finder on macOS: the saved `mitcad-desktop.mcpb` selected, its path in the tooltip), with the quiet **Open Claude Desktop** (`assistantOpenDesktop`, opens `claude://`; without Claude Desktop's application, or when the link does not open, the state becomes Install Claude Desktop first) |
| `busy` | Connecting *assistant*… | `connect`, disabled |
| `connected` | ✓ Connected: *the chosen assistant's name* (Other: the clients' labels) | `disconnect` (Disconnect) |
| `manual` | Copy the configuration into your assistant's settings (Other) | `copy` (Copy configuration) |
| `unavailable` | Claude Desktop is not available for this system (Linux) | none |
| `not-installed` | *Assistant* was not found on this computer | `install` (Get Claude Desktop: the download page; How to install: Claude Code's and Codex's pages), with Connect anyway; after How to install the quiet **Check again** (`assistantCheckAgain`: looks again at once, "Not found yet" for a moment while still missing) |
| `waiting` | the assistant's next step (approve, new session) | `reopen` (Open in Claude Desktop again) for Claude Desktop; none for the others |
| `ready` | Ready to connect *assistant* | `connect` (Connect) |

Installed means: Claude Code's or Codex's executable
(`assistantExecutable`, the one lookup the chat panel of mitcad#947 uses
too), or Codex's settings folder; Claude Desktop's
application, its Microsoft Store package (the family
`Claude_pzs8sxrjxfjjc`, `kClaudeDesktopPackageFamily`, installed for the
user by `GetPackagesByPackageFamily`) or its settings folder
(`claudeDesktopInWindowsFolders` on Windows; in a test run only a
`claude-desktop` file in `MITCAD_TEST_ASSISTANT_EXECUTABLES` or, on
Windows, the folders in the work folder's `local-app-data` and
`roaming-app-data`, where the package's folder under `Packages`,
`claudeDesktopPackaged`, stands for the package), or the bundle opened.
A saved bundle that no application opened is Install the extension in
Claude Desktop when Claude Desktop's application is found
(`claudeDesktopApplicationFound`: the application or the package, never
the settings folder an uninstall leaves behind;
`claudeDesktopApplicationInWindowsFolders` in a test run), else Install
Claude Desktop first.

The executable's lookup order (mitcad#956, `app/mcp/AssistantSetup.cpp`):

1. `PATH` (`QStandardPaths::findExecutable`, PATHEXT's suffixes on Windows).
2. The home's `.local/bin` (Claude Code's own installer, which may leave
   `PATH` without it).
3. On Windows `%APPDATA%\npm` (npm's global installs).
4. The home's `.cargo/bin`.
5. On Windows the user's `Path` as stored in `HKCU\Environment`
   (`storedUserPath`: `QSettings` on the registry, `%NAME%` expanded): an
   install made while MitCad runs is there before MitCad's environment has
   it.
6. On macOS the ChatGPT or Codex application's `codex`.

Steps 2-5 (`assistantFallbackFolders`, `findAssistantExecutable`) try the
platform's suffixes in each folder (`assistantExecutableSuffixes`: on
Windows `.exe`, `.cmd`, `.bat` in PATHEXT's order; none elsewhere). The
state is read every second, and at once when the dialog is activated again
(back from an installer) or on Check again. A client found outside `PATH`
is named by its full path in the Advanced command (`assistantProgram`);
setup always starts the path found. A test run looks only in
`MITCAD_TEST_ASSISTANT_EXECUTABLES` (`none`: nothing), then in the folders
of the home `MITCAD_TEST_ASSISTANT_HOME` (no `PATH`, no registry).

Waiting means a verified entry (Claude Code, Codex) or the bundle
opened from this dialog (Claude Desktop). Connect, Copy configuration and
Try again first allow the folder shown (`grantFolder`, the consent; the
dialog's `folder()`/`setFolder()` are what Change… uses and the test
driver calls). Log lines: `Assistant setup: updated|error` per result,
`Assistant setup: folder allowed`, `Assistant setup: extension shown`,
`Assistant setup: Claude Desktop opened` (or `… could not be opened`), and the driver's `Assistant setup UI:
<mode>`; for the Linux UI tests (mitcad#760) `Connect AI Assistant: <choice>,
<assistantState>: <line>` when the choice or its state changes,
`Connect AI Assistant choice <choice> at x,y` (each choice, in the main
window's coordinates, again when the dialog moves or is laid out anew),
`Connect AI Assistant opened over Preferences` and, on the AI Assistants
page, `Preferences connect assistant at x,y`.

Opened while an application-modal dialog runs (Preferences outside macOS,
`execDialog`), the dialog is that dialog's child, since Qt gives input
only to the modal dialog and its own windows, and it moves back to the
main window, where it was, when that dialog finishes
(`MainWindow::showConnectAssistant`, mitcad#760). Settings files are read
up to 4 MB (`kAssistantConfigurationLimit`), Claude Code's `.claude.json`,
which keeps its projects' history, up to 256 MB
(`kClaudeConfigurationLimit`).

Claude Desktop receives a runtime `.mcpb` with manifest 0.3 and a standalone
native `mitcad-mcp-launcher`; the launcher starts the installed CLI without a
shell, retaining its own runtime library paths. Opening installation does
not establish installed or connected state. Claude Code registration uses
`claude mcp add --scope user mitcad -- <bridge> mcp --connect`.

Codex registration uses `codex mcp add mitcad-desktop -- <bridge> mcp --connect`
against a private staged `CODEX_HOME` and verifies the result before an
atomic, backed-up update of the actual selected file. With no CLI, the same
Rust TOML planner supplies the update directly. Settings and comments outside
the MitCad entry are preserved; conflicts need explicit replacement, and
managed/disabled configurations remain blocked. Repeated matching setup is a
no-op. The setup never stores an instance id or changes model/undo state.

LiveServer retains bounded, plain-text `clientInfo.name`
labels after `notifications/initialized`, each with an id of its
connection; the agent indicator and dialog show all initialized clients.
`ui_state.agent_clients` exposes those labels
beside the existing `agents` count; disconnect preserves the design.

Tests: `app.unit` helper checks, the portable `app.ui-*-assistant-setup`
fixtures with compiled stub clients and private configuration homes, and the
existing MCP integration suites. They cover direct and CLI setup, malformed
and managed configuration, idempotence, backups, failed/corrupt clients,
concurrent edits, bundle metadata, explicit folders and connection state.
The driver (`expect-assistant-setup <mode>`, `TestDriverAssistant.cpp`)
picks assistants by clicking their buttons and checks `assistantState` and
`assistantAction`: `install` for a missing Claude Code and Claude Desktop,
Connect once they are there, `waiting` after a verified setup, `failed`
with Try again after a rejected one, no folder allowed before Connect and
the folder shown allowed after it. Its `codex` mode also checks the words
of Codex's choice and Advanced and that the dialog keeps no empty height
once Codex's shorter configuration replaces Claude Code's with Advanced
open, and once Advanced closes; `disconnect-other` builds a dialog of its
own whose host lists a Claude Code client and `example-client`: Disconnect
names Claude Code with it chosen, and with Other chosen reads "Disconnect
example-client" and ends only that client (mitcad#724). Its
`packaged-claude` mode (Windows, mitcad#763) sets
`MITCAD_TEST_ASSISTANT_BUNDLE_OPEN=none` (no application for the
extension, without the registry; the other modes' `capture` records the
installer instead): with no Claude Desktop, Connect anyway gives Install
Claude Desktop first; with the package's folder in the isolated
`local-app-data`, Connect gives `install-extension` with `reveal` and Open
Claude Desktop, which in an isolated run record the file
(`assistantShownPath`) and the link (`assistantOpenedDesktop`) instead of
opening them. Its `local-bin` mode (every platform, mitcad#956) sets
`MITCAD_TEST_ASSISTANT_HOME` to the work folder's `user home`: a missing
Claude Code offers Check again (which says Not found yet); the stub copied
to that home's `.local/bin` (`claude.exe` on Windows) is found when the
dialog gets an activation change, and the command names its full path;
removed, Check again shows it missing, and copied back, found.

## Drawings

Drawings kept in the design (mitcad#707, phases 1 to 4; the model's side in
`core/model-api/src/commands.md`, *Drawings in the design*, *Annotations
on drawing views*, *Tables on drawing views* and *Drawing templates and
revisions*). The app
paints what the model's `drawing` query gives and sends the model's
drawing commands; it computes no geometry of its own.

- **Browser** (`browser/BrowserPanel.cpp`, `BrowserController.cpp`): each
  component's rows (the root's too) get a **Drawings** folder after
  Construction when it has drawings (`DocumentSnapshot::drawings`, the
  `drawings` query), with a row per drawing (`BrowserNode::Type::Drawing`,
  uid `D1`, icon `drawing`, the tooltip its sheets). A double-click opens
  it; its menu: Open, Rename (F2 too: `edit_drawing`), Delete
  (`delete_drawing`). The folder's menu and a component row's menu (after
  New Component) have **Create Drawing**. Logged as other rows
  (`Browser Root/Drawings/Drawing1 at x,y`, `Deleted drawing Drawing1`).
- **Create Drawing** (`file.create_drawing`, the File menu's *Create
  Drawing…* after Export,
  keywords drawing, sheet, blueprint, 2d, views; model mode) and the menus'
  Create Drawing ask in a dialog (**Create Drawing**: Template, ISO first,
  then ANSI, the user's and the project's templates and Simple Title
  Block, as the model's `drawing_templates` lists them with the design's
  project (`drawings::availableTemplates`, `DrawingTemplates`), a
  template chosen bringing its sheet size and its projection method
  (`TemplateChoice::projection`), the first one too (ISO: A3, first
  angle, mitcad#722, mitcad#725); Size, A4 first in the list; Orientation;
  Projection; logged `Create Drawing dialog opened` and `Create Drawing
  dialog: template iso, A3, first_angle`), then run `add_drawing` with them (a
  built-in template by its id, a library's as its data) (and `component` for a component's folder),
  an undo step, and open the new drawing (`DocumentHost::createDrawing`;
  logged `Drawing created: D1 Drawing1 in C0`) with Base View on
  (mitcad#725).
- **The drawing's view** (`drawings/DrawingWorkspace`): a child of the 3D
  view inside the cards over it (`OcctViewer::overlayInsets`, as the mode
  banner), so the Docked, Floating and native layouts show it alike;
  beside the cards at the view's sides whose rectangles reach its bar's row
  (`coveredRects`, followed through `OcctViewer::coveredChanged`: the
  Floating layout's browser opened or folded), so that Close Drawing and
  the tools are never under a card (mitcad#725). Where the bar is then
  narrower than its row, the views' tools show their icons only, their
  names in their tooltips (`fitTools`, logged `Drawing tools: icons`,
  `names`), and the places are logged again. It
  covers the 3D view until **Close Drawing** (also the command
  `drawing.close`) or a command of the ribbon chosen there or in the
  command search (`MainWindow::trigger`: a feature command or one with a
  tab leads back; the menus' commands such as Undo, Save and Export do
  not). Esc never closes it (the owner, mitcad#725).
  It cannot open while a command or a sketch is open
  (`changeBlockedReason`). Its bar has two rows: Close Drawing, the drawing's name
  (`Component › Drawing1` in a component), the sheets as tabs (a tab's
  menu: Rename…, Delete Sheet, **Save as Template…**: a name and the User
  or Project library, `save_drawing_template`, for a sheet with a
  template; logged `Drawing template saved: user:company-a3 <path>`) and
  **+** (`add_drawing_sheet`), **Sheet…** (a dialog: Template, which
  brings its size and projection method as in Create Drawing (the
  sections and details drawn on projected views follow a changed
  method, mitcad#729), Size,
  Orientation, Projection, Units (The Design's, mm, cm, m, in, ft, um,
  mil: the sheet's `units`, mitcad#725), Title, Drawing Number, Drawn By,
  Date; `edit_drawing_sheet`, with `template` only when another one is
  chosen), **Add Revision…** (also the command `drawing.add_revision`: a
  dialog, **Add Revision**: Description, needed, Date, today's when empty,
  By; `add_drawing_revision`); under them the tools in groups, each over
  its caption and after a line (`toolGroup`, `toolGroupName`, mitcad#725):
  Create (the views'), Geometry (Center Mark, Centerline, Line, Arc,
  Center Pattern, Edge Extension; mitcad#729), Dimensions
  (Dimension, Ordinate, Baseline), Text (Text, Note), Symbols (Datum
  Label, Feature Control Frame, Surface Finish), Tables (Hole Table,
  Parts List, Balloon) and Export: **Export…** (PDF…, SVG…, DXF…, DWG…: a save
  dialog in the Export folder, then `export_drawing` with `drawing` and
  the design's name as `title`), so the tabs keep their room. The tab
  bar is at most as wide as its tabs and at least as its first three
  (`Maximum` policy and a minimum set in `showSheet`): many sheets scroll
  under its buttons instead of pushing the bar past the view, a few never
  hide under them. A press on a tab
  shows its sheet through the workspace's event filter on the tab bar
  (`switchSheet`; logged `Drawing sheet tab pressed: 1`, then `Drawing
  sheet 1 shown`), not only through the bar's `currentChanged`, which
  comes only when the bar's own current tab changes: on Windows a click
  on a tab did not switch sheets (mitcad#725). The keyboard and the
  wheel on the tabs still switch through `currentChanged`. Another sheet
  shown (a tab, +) drops the active tool's picks so far, a half-drawn
  line or circle and a placing click awaited; the tool stays
  (`dropPicks`, mitcad#759). Delete Sheet keeps the sheet shown shown:
  one before it deleted, it moves up a place (logged `Drawing sheet 2
  shown`); the one shown deleted, the next shows, the one before when it
  was the last (`deleteSheet`). Beside the sheet a
  panel shows the active tool's options, else the selected view's
  properties, else the sheet's size, scale, views and template and the
  drawing's revisions, a row each (a click opens it in the revision
  dialog, with Delete: `edit_drawing_revision`, `delete_drawing_revision`)
  and **Add Revision…**; several items selected: their names and Delete
  (`drawingSelectionDelete`). A view's
  properties, each change one `edit_drawing_view`: why it cannot be drawn
  (red); a base view's Orientation (the tool's list; a given direction
  shows as Given; Current View takes the 3D view's again) and what it
  **Shows** (`drawingViewShows`: the tool's list, `subjectChoices`; a
  subject of several bodies as Chosen Bodies); a projected view's Side; a
  section's and a detail's Letter; a section's Hatching and Look the
  Other Way (`flip`); a detail's Radius (mm on the sheet, a point or a
  comma as `parsePlainNumber` takes them; sent only when its text
  changed) and Center X and Center Y (its circle's centre on the sheet,
  mm, the same way; mitcad#715); the Scale (a
  base view's Fit the Sheet: `"fit"`, its own scale shown, 1:1 for one
  without; another's first entry follows its
  parent, labelled with the scale that gives: `Its Parent's (1:2)`,
  `Twice Its Parent's (2:1)` for a detail); Hidden Edges, Tangent Edges,
  Shaded, Label, In Line with Its Parent, Remove Breaks, Delete View;
  every view's but a detail's **Rotation** (`drawingViewRotation`,
  degrees counter-clockwise, a point or a comma; `"rotation"`,
  mitcad#729). A view turned (its rotation, direction, side, a section's
  flip, the sheet's projection method) takes the sections, details and
  annotations on it along (the model's `follow_turns`, mitcad#729; a
  view with breaks turns by quarter turns only, and the status bar says
  so). With nothing selected the panel also has the drawing's
  **Decimals** (`drawingDefaultDecimals`: Automatic, 0 to 6) and **Text
  Height (mm)** (`drawingDefaultTextHeight`, empty: 3.5), each an
  `edit_drawing` (`drawingDefaultsEdit`, mitcad#729). The active tool's hint shows over the top of the
  sheet (logged `Drawing hint: <text>`); results and refusals go to the
  window's status bar, as do the views the model could not draw
  (`View 2: the body F9.b0 is gone`) when they change. The view follows
  the model (`MainWindow::refreshScene` → `followDrawing`: the `drawing`
  query again; a drawing that is gone, or another design, closes it).
- **Keys.** While a drawing is open, Delete (and Backspace) and Esc,
  without modifiers, are its own wherever the focus is in it
  (`DrawingWorkspace::event` takes their ShortcutOverride; a text field
  takes them first): Delete deletes the selected views and annotations;
  Esc clears a tool's picks (the tool's hint again), then ends the tool,
  then clears the selection (logged `Drawing selection cleared`), and
  then does nothing (logged `Drawing Esc: nothing to end`): only Close
  Drawing closes it (the owner, mitcad#725). The shortcuts of the model's
  commands (a feature, or a command with a ribbon tab: E, H, …) do
  nothing while the focus is in the drawing (mitcad#725): the workspace
  takes their ShortcutOverride (`modelShortcut`, given by the main window
  from the command registry's effective shortcuts) and logs `Drawing
  shortcut ignored: E`, also while the 3D view under the sheet has the
  keyboard (its event filter on the view, as `escapeDrawing` counts
  that focus as the drawing's), so a stray key does not close the
  drawing; the
  menus' shortcuts (Undo, Redo, Save) and the command search's S stay.
  This is the drawing's own rule, not a shortcut context of the
  framework. The panel, built again after each change, gives the keyboard
  back to the field of the same name (else to the sheet), so the keys
  keep working after a panel edit. The window's Esc (`MainWindow::cancel`)
  is the drawing's when the focus is in it, on the 3D view or nowhere
  (`escapeDrawing`). The browser's and the timeline's Delete key deletes
  no model item while a drawing is open (`DocumentHost::isDrawingOpen`;
  the status bar says why); their menus still do. Close Drawing
  (`drawing.close`) works in a read-only window too.
- **The sheet** (`drawings/DrawingSheetView`): the paper on grey, the
  query's groups painted with ISO 128's widths and dashes at the zoom
  (lines at least a pixel), texts upright in Helvetica at their height on
  the paper (none under 2 pixels), shading under the lines. The wheel
  zooms about the pointer, the middle button pans, Home fits the sheet
  (also on a resize until zoomed). A zoom or a pan stays through the
  model's next answers (a command, an undo, another sheet of the same
  size); a drawing opened, or a sheet of another size, shows the whole
  sheet (mitcad#751). The paper and the groups are painted into a picture
  kept until the sheet, the zoom, the pan or the size changes; the
  selection, the hover and the previews are painted over it, and a move
  of the pointer paints again only when what is under it or what follows
  it changed (mitcad#759). The view under the pointer is outlined
  light blue, the selected one blue, a projected view's parent dashed.
  With no tool a click selects a view and a drag moves it (`edit_drawing_view`
  with `move`, the drop logged `Drawing view 1 dragged by 10.0,5.0`);
  Delete deletes it with the views from it. Ctrl+click adds a view or an
  annotation to the selection or takes it out (mitcad#725; logged
  `Drawing selection: view 3, annotation 1`): a drag of one of them moves
  them all (one `runModelCommands` step; the views from a moved view and
  the annotations on it placed from their picks are not moved again, its
  hole tables and parts lists, which stand on the sheet, are, mitcad#752;
  logged `Drawing selection
  dragged by 5.0,0.0`), Delete deletes them (annotations, then views, the
  later ones first). The right button opens the sheet's menu (logged
  `Context menu: …`): a view's Projected View, Section View, Detail View,
  Break, Rotate 90° Left and Rotate 90° Right (not a detail's;
  mitcad#729), its panel's Hidden Edges, Tangent Edges, Shaded, Center Marks (a
  section's Hatching) and Delete View; an annotation's Edit Text… (a note
  or a text), Tolerance… (a dimension or a frame: its panel's button) and
  Delete; several items' Delete; the sheet's Sheet…, Add Revision…, Add
  Sheet. A double-click on a note or a text opens its text in the note
  dialog (`editNoteText`; logged `Drawing annotation 4 double-clicked`). A selected detail's circle is
  drawn blue on its parent; a drag that starts inside it moves the circle
  (`edit_drawing_view` with `center`; logged `Drawing detail 4 circle
  dragged by 4.0,4.0`). A view the model could not
  draw keeps a red dashed box at its place with why, so that it can be
  selected and deleted (the query's `broken` and `box`).
- **The tools** are declarative (`drawings/DrawingTools`): each a
  `DrawingToolDef` with its id, name, icon, tooltip, what the sheet waits
  for (`SheetPick`: a point, a drag out of a view, a line on a view, a
  circle on a view, picks on a view, a click on a view and then a place:
  `ViewThenPlace`, logged `Drawing picked view 5`; two or three clicks
  anywhere: `Segment` and `ArcPoints`, each logged `Drawing point 1 at
  x,y`, the view under the first one given, mitcad#729), the hint, its options (`ToolOption`: choice, check,
  text), `command`, which makes the model's command from the picks
  (sheet millimetres, the view picked) and the options (the model turns
  sheet points into the view's coordinates), and `place` with its hint:
  one more click, after the line or the circle, where the new view goes
  (the sheet draws the line or circle and a dotted line to the pointer;
  logged `Drawing placing view`). While a section or a detail is placed,
  and beside the view a projected view comes from, a dashed box about
  the new view's size follows the pointer (its parent's box, a detail's
  circle at twice its scale; logged `Drawing preview: view box`).

  | Tool | Picks | Options | Command |
  |---|---|---|---|
  | Base View (`drawing.base_view`) | a click: the view's middle | Shows (mitcad#725: the drawing's component by its name, `design`; each occurrence in it by its path of names, `occurrence:O1/O4`; each of its own bodies, `body:F2.b0`; from the `components` and `bodies` queries, `subjectChoices`), Orientation (Front, Top, Right, Back, Bottom, Left, Isometric, Isometric Top Left, Isometric Bottom Right, Isometric Bottom Left, Current View: the 3D view's camera as `{"toward_eye", "up"}`), Scale (Fit the Sheet, as in the view panel, or a scale), Hidden Edges, Tangent Edges, Shaded | `add_drawing_view` `base` (with `subject` unless the whole component; `direction` the camera for Current View); then Projected View starts from it |
  | Projected View (`drawing.projected_view`) | a drag out of a view, or a click beside the view picked (the selected one, or one clicked) | | `add_drawing_view` `projected` with `at`; the side follows from where it is |
  | Section View (`drawing.section_view`) | two clicks on a view: the line; a third: where it goes | Letter (the next free), Hatching, Look the Other Way | `add_drawing_view` `section` with `line`, `flip`, `at` |
  | Detail View (`drawing.detail_view`) | a click on a view (the middle), a second (the radius), a third: where it goes | Letter, Scale (Twice Its Parent's, as in the view panel, or a scale) | `add_drawing_view` `detail` with `at` |
  | Break (`drawing.break`) | two clicks on a view: the band | Direction (Horizontal, Vertical) | `edit_drawing_view` `add_break` |
  | Dimension (`drawing.dimension`) | two picks (a radius or diameter: one), then a click where it goes | Type (Linear, Aligned, Angle, Radius, Diameter) | `add_drawing_annotation` `linear` (Aligned: `"along": "aligned"`), `angle`, `radius`, `diameter` with `picks` and `at` |
  | Ordinate (`drawing.ordinate`) | the origin, the point, then a click where the value goes | Direction (Horizontal, Vertical) | `ordinate` with `along` |
  | Baseline (`drawing.baseline`) | picks until a click that picks nothing, which places them (two at least) | Type (Baseline, Chain), Direction | `linear` with `series` |
  | Center Mark (`drawing.center_mark`) | a circle, or a round face seen from the side | | `center_mark` |
  | Centerline (`drawing.centerline`) | two circles, or two parallel edges | | `centerline` |
  | Line (`drawing.line`, mitcad#729) | two clicks anywhere: its ends | Drawn (In the View Clicked: the view under the first click, if any; On the Sheet), Line (Thin, Thick, Dashed, Center Line: `penChoices`) | `"type": "line"` with `points`, `pen`, and `view` or `sheet`; the tool stays |
  | Arc (`drawing.arc`, mitcad#729) | three clicks anywhere: its centre, start and end (a dashed line from the centre and the circle through the start follow the pointer) | as Line's | `"type": "arc"` with `points`, `pen`, `view` or `sheet` |
  | Center Pattern (`drawing.center_pattern`, mitcad#729) | one circle seen end on | | `center_pattern`: the model finds the pattern's other holes |
  | Edge Extension (`drawing.edge_extension`, mitcad#729) | two straight edges, or one | Edges (Two, to Their Corner; One, Both Ways) | `edge_extension` |
  | Text (`drawing.text`) | a click on the sheet | Text (empty: the annotations' Note dialog asks), Drawn (On the Sheet, In the View Clicked; mitcad#729) | `"type": "text"` with `sheet` (or `view`), `at`, `text` |
  | Note (`drawing.note`) | a pick, then a click where the text goes | Text (as Text's) | `note` |
  | Datum Label (`drawing.datum`) | a pick, then a click where the label goes | Letter (the next free of the model's and the drawings' letters) | `datum` |
  | Feature Control Frame (`drawing.frame`) | a pick, then a click where the frame goes | (the annotations' frame dialog asks for its tolerance; its datums: the `drawing` query's `datum_letters`, the model's and the drawings') | `frame` |
  | Surface Finish (`drawing.finish`) | a pick | Surface (Any Process, Material Removed, No Material Removed), Requirement (`Ra 1.6`) | `finish` |
  | Hole Table (`drawing.hole_table`) | a click on a view, then where the table goes (its upper left corner) | Origin (Lower Left Corner, Component Origin), Tags (By Size, Numbers) | `add_drawing_annotation` `hole_table` with `at` and `table` |
  | Parts List (`drawing.parts_list`) | a click on a view, then where the list goes | Rows (First Level, Parts Only), Balloons (on: `"balloons": true`, a balloon on each row's bodies; mitcad#725) | `parts_list` with `at` and `table` (`levels`) |
  | Balloon (`drawing.balloon`) | a pick on a component's body, then where the balloon goes | | `balloon` |

  Each tool's options are kept, by tool, while the window lives. Esc
  clears the picks so far, then ends the tool; a tool's button again ends
  it. A table ends its tool and is selected, its panel shown; a balloon's
  tool stays for the next one.
- **Annotations** (phase 2; the model's side in `commands.md`,
  *Annotations on drawing views*). The annotations' tools are on the bar
  after a gap, as their icons (their names and tooltips in their
  tooltips), and are commands of the window like the views' tools. Their
  clicks pick model topology, not sheet points: each click on a view
  (`SheetPick::Picks`, the sheet's `pickRequested`) is sent to the model's
  `drawing_pick` with the tool's annotation type (`annotationType`), and
  what it picks is drawn in orange over the sheet (logged `Drawing picked
  line (1): {"body":"F2.b0","edge":"E{…}"}`; before the click, what
  it would pick under the pointer is drawn light orange: the pointer's
  moves ask `drawing_pick` at most every 60 ms, logged `Drawing hover:
  line` (`nothing`) when that changes, mitcad#725; while a job computes
  the model the question waits for it, `whenIdle`, mitcad#753; a click that picks nothing
  says so in the status bar, `Drawing pick: nothing at x,y`). Once the
  tool has its picks (`pickCount`) it waits for its placing click (logged
  `Drawing placing view`) or is made at once; the tool stays on for the
  next one. While it is placed, a linear dimension's extension and
  dimension lines follow the pointer, any other annotation's leader and
  a box (logged `Drawing preview: dimension`, `leader`; the picks' points
  as `drawing_pick` gave them, mitcad#725). The command gets the model's
  picks (`ToolPicks::picks`), so the annotation follows the model. A click picks an annotation on what it
  draws (mitcad#717): near one of its lines or arcs (4 pixels, 0.8 mm of
  paper at least) or on one of its texts or arrowheads (each text's area
  as Helvetica writes it), the nearest; one that draws nothing (a broken
  one) by its box; so an annotation's box no longer covers the view under
  it. A selected annotation (annotations come before the views under them) is outlined blue, a drag
  moves it (`edit_drawing_annotation` with `move`: a dimension's line
  across, its text along; a leader's text; logged `Drawing annotation 1
  dragged by 10.0,0.0`), Delete deletes it. Its panel: what it Shows (its
  words), a linear dimension's or an ordinate's Direction, a dimension's
  Tolerance (the annotations' tolerance dialog, `askAnnotationTolerance`)
  and Decimals (`askAnnotationDecimals`), Text Before and Text After; a
  note's Text… (`askNoteText`, opened with the note's text), a datum's
  Letter, a frame's Tolerance… (`askFeatureControlFrame`, its datums the
  model's and the drawings'), a finish's Surface and Requirement; the
  Text Height of what writes (`drawingAnnotationHeight`, empty: the
  drawing's), a line's or an arc's Line (`drawingAnnotationPen`), an
  edge extension of one edge's Length (`drawingAnnotationLength`)
  (mitcad#729); Delete.
  What a pick shows in orange includes an arc where it is
  (`{"arc": …}`), not its whole circle.
  An annotation whose picks are gone keeps a red dashed box beside its
  view with its id, to be selected (its panel says why) and deleted. A
  view's panel has Center Marks (automatic centre marks and centrelines,
  on for a new view) and Sketch Dimensions (the sketch dimensions it sees
  square on).
- **Tables** (phase 3; the model's side in `commands.md`, *Tables on
  drawing views*). Hole Table and Parts List take a click on a view and
  then one where the table goes; the table is selected, and its panel
  (`DrawingWorkspace::fillTablePanel`) has, each change one
  `edit_drawing_annotation` with `table` (`annotationEdit`'s `columns`,
  `sort`, `descending`, `origin`, `tags`, `levels`, `title`): Columns (a
  check box per column the table can show, `available` in the query; one
  stays on: the status bar says so), a parts list's More Columns
  (properties of the components, names separated by commas), Sort By,
  Descending, a hole table's Origin (Lower Left Corner, Component Origin,
  and A Point of the Model when it has an origin pick; choosing one of
  its own drops the pick) and Tags, a parts list's Rows (First Level,
  Parts Only), Title (empty: its own), Delete. Its "Shows" row has its
  rows in words. A table is dragged as an annotation is (by its title,
  cells or tags; `move`), and Delete deletes it. A balloon's panel shows
  its number; a balloon whose component is gone, or that no parts list
  of the sheet lists, keeps a red box with why.
- **The tools as commands.** Each tool is also a command of the window
  (`MainWindow::registerDrawingCommands`: `CommandDef::Kind::Action`, its
  id, name, icon and tooltip, no ribbon tab, `run` starts it, `enabled`
  while a drawing is open, the command search's keywords drawing and
  sheet and the tool's own, `toolKeywords`: Leader, Chain, Datum
  Identifier, Surface Texture, Break View, BOM; mitcad#725; Sketch, Bolt
  Circle, Virtual Sharp, mitcad#729), as is Close Drawing (`drawing.close`), so the
  command search and the shortcuts find them. The drawing's Parts List is
  **Parts List (Drawing)** in the command search, apart from the Build
  tab's Parts List (`build.parts_list`); on the drawing's bar it is Parts
  List (mitcad#718). Add Revision… (`drawing.add_revision`) is a
  command of the window too, enabled while a drawing is open. The `DrawingToolDef` stays
  for what a `CommandDef` does not say: what the sheet waits for (its
  picks are sheet points, not selections of the 3D view the framework's
  inputs take) and the builder of the model's command from them.
- **Why the drawing's panel is not the framework's** (decided for
  mitcad#715): the workspace's panel shows a tool's options and a view's
  or an annotation's properties with plain widgets (`ToolOption`, and
  the rows `fillViewPanel` builds). The framework's panel
  (`CommandPanel`) is the face of a `CommandSession`: it holds the model
  at a checkpoint while the command is open, previews the definition its
  inputs build, and changes the model once, on OK. A view's properties
  are the opposite: each field is its own undo step at once (a change of
  Scale, Hatching or Letter is an `edit_drawing_view` when made), there
  is no OK, Cancel or preview, and the panel is rebuilt from the model's
  answer after every change (also after Undo). A non-feature mode of
  `CommandPanel` would have to take its state from a query instead of a
  session, send one command per input instead of one definition, and
  drop its checkpoint, preview, OK and Cancel: most of what it is. The
  drawing's panel keeps the framework's look (a title, a form of labelled
  rows) and its log lines (`Drawing panel <field> at x,y`), and its
  option kinds (choice, check, text) are those of `choiceInput`,
  `checkInput` and `textInput`.
- **Logs** for UI tests: `Drawing opened: D1 Drawing1, sheet 1 of 2`,
  `Drawing closed`, `Drawing tool: Section View` (`selection`), `Drawing
  selection cleared`, `Drawing hint: <text>`, `Drawing placing view`,
  `Drawing tool <name> at x,y`, `Drawing Close Drawing at x,y`, `Drawing
  Add Sheet at`, `Drawing Sheet at`, `Drawing Export at`, `Drawing Add
  Revision at`, `Drawing title block (iso): TITLE=block | MATERIAL=Steel
  | … | REV=A` (`simple` for the simple block; the sheet's fields as drawn,
  when they change), `Drawing revisions: A First release; B Hole moved`
  (`none`), `Drawing revision dialog opened: Add Revision`, `Drawing Sheet
  tab <n> at`, `Drawing sheet tab pressed: <n>` (`none`), `Drawing group
  <caption> at`, `Drawing sheet menu: sheet <n>`, `Drawing sheet <n> shown`,
  `Drawing panel <field> at` (the options' fields: `drawingOption_<id>`;
  the view's `drawingViewOrientation`, `drawingViewShows`,
  `drawingViewSide`, `drawingViewLetter`, `drawingViewHatch`,
  `drawingViewFlip`, `drawingViewRadius`, `drawingViewCenterX`,
  `drawingViewCenterY`, `drawingViewScale`, `drawingViewRotation`,
  `drawingViewLabel`, `drawingViewCenterMarks`,
  `drawingViewRetrievedDimensions`, `drawingViewRemoveBreaks`,
  `drawingViewDelete`; an annotation's `drawingAnnotationValue`,
  `drawingAnnotationAlong`, `drawingAnnotationTolerance`,
  `drawingAnnotationDecimals`, `drawingAnnotationBefore`,
  `drawingAnnotationAfter`, `drawingAnnotationText`,
  `drawingAnnotationLetter`, `drawingAnnotationFrame`,
  `drawingAnnotationProcess`, `drawingAnnotationRequirement`,
  `drawingAnnotationPen`, `drawingAnnotationLength`,
  `drawingAnnotationHeight`, `drawingAnnotationDelete`; a table's
  `drawingTableColumn_<column>`,
  `drawingTableMoreColumns`, `drawingTableSort`, `drawingTableDescending`,
  `drawingTableOrigin`, `drawingTableTags`, `drawingTableLevels`,
  `drawingTableTitle`; the sheet's `drawingDefaultDecimals`,
  `drawingDefaultTextHeight`, `drawingRevision_<letter>`,
  `drawingPanelAddRevision`; several items' `drawingSelectionDelete`),
  `Drawing point <n> at x,y` (a line's or an arc's clicks),
  `Drawing picked view 5` (a table's view),
  `Drawing sheet view x0,y0 x1,y1 x2,y2` (where the sheet points (0, 0),
  (100, 0) and (0, 100) are in the window), `Drawing view <id> at x,y`
  (its box's middle), `Drawing detail 4 circle at x,y` (its circle's
  centre on its parent), `Drawing annotation <id> at x,y` (a point that
  picks it: its first text's middle, else its first line's, else its
  box's middle), `Drawing view 2 selected`, `Drawing annotation 1 selected`,
  `Drawing sheet drawn: 4 views, 312 items, 2 center marks`, `Drawing
  annotations drawn: 1 = 20 ±0.1 | 2 = Ø10 | 3 = center_mark | 4 broken`
  (each annotation's words, its type when it has none, when they change),
  `Drawing picked <seen> (<n>): <pick>`, `Drawing pick: nothing at x,y`,
  `Drawing projected views from view 1`, `Drawing hover: <seen>`,
  `Drawing preview: <what>`, `Drawing selection: <items>`, `Drawing
  selection dragged by x,y`, `Drawing shortcut ignored: <key>`, `Drawing
  Esc: nothing to end`, `Drawing annotation <id> double-clicked`, `Drawing
  option letter: K` (a text option on each change), `Drawing command: <json>`, `Drawing command refused:
  <why>`, `Create Drawing dialog opened`, `Drawing sheet dialog opened:
  A3`, `Drawing export: <path>` and `Exported <path>: drawing Drawing1, 2
  sheets`.
- Tests: `app.unit` (`tests/drawing_tools.cpp`: the tools, their picks and
  options, the commands and the view and annotation properties' edits),
  `tools/ui-drawings-test.sh` (a drawing created through its dialog, a
  base view, a projected view, a section and a detail each placed by a
  click, a detail's circle dragged, a break, a panel edit, Delete and Undo
  while the drawing is open, a sheet added, renamed and deleted, the
  Sheet… dialog; a dimension, a diameter, a centre mark and a note picked
  on views, a ± tolerance from the annotations' dialog, a dimension
  dragged; the model changed under it and the dimension following, a
  suppressed hole's diameter marked broken; saved and opened again, a PDF
  exported with the annotations; a top view through Base View's
  Orientation and Scale and a hole table of it, which empties when the
  hole is suppressed; on an assembly a parts list and a balloon, an SVG
  with the list, a placement deleted from the browser: the list counts
  one, the balloon is broken, Undo brings them back; mitcad#725: Base View
  on a new drawing, the ISO template's first angle, the captioned groups,
  model shortcuts ignored, sheets switched by tab clicks, the Sheet…
  dialog's Units, the hover's pick and the dimension's preview, two items
  selected with Ctrl+click, dragged, deleted and undone, Esc leaving the
  drawing open, the note's menu and double-click, a view's menu, a parts
  list with its balloons, the assembly in the Floating layout, File ›
  Export Drawing writing the same drawing again as it is),
  `tools/ui-windows-drawings-test.ps1` (ctest `app.ui-windows-drawings`:
  most of the same in the Windows VM, each tool's log line awaited before
  its clicks; a dimension that follows the model; a parts list and a
  balloon on an assembly; mitcad#725: Base View on a new drawing, the
  sheet tab's press logged, Close Drawing instead of Esc, the parts
  list's Balloons off), `tools/ui-drawing-features-test.sh` (mitcad#729:
  the drawing's decimals and text height in the sheet's panel and a
  dimension's own; a line and an arc drawn in a view and on the sheet,
  a text in a view, the view moved with them; a centre pattern from one
  hole; an edge extension; a view rotated in its panel and from its
  menu, its dimension following, undone; a base view's orientation
  changed under its section and detail).

- **Cut sheets** (mitcad#853; the model's side in
  `core/model-api/src/commands.md`, *Cut sheets*): Create Drawing's dialog
  has a **Kind** first (Drawing, Cut Sheet; `drawingKind`); Cut Sheet
  shows **Width** and **Height** (the material, 600 × 300 mm,
  `drawingCutWidth`, `drawingCutHeight`) instead of the template and the
  sheet, logs `Create Drawing dialog: cut sheet 600 x 300` and runs
  `add_cut_sheet` with the bodies selected in the model
  (`DocumentHost::selectedModelBodies`: a body's uid, `{"body", "occurrence"}`
  for one an occurrence places), then opens it without a tool
  (`Drawing created: D1 CutSheet1 in C0`). A cut sheet opens in the
  drawing's view as a drawing does (the `drawing` query's `kind`
  `cut_sheet`): its parts are the sheet's views (`type` `part`), dragged
  to move them (`edit_drawing_view` `move`), deleted with Delete; the bar
  shows **Material…** (the material's width and height, `edit_cut_sheet`)
  instead of Sheet…, no sheet tabs, + or Add Revision…, and instead of the
  tools **Add Parts…** (logged `Drawing Add Parts at x,y`: a dialog of the
  bodies of the cut sheet's component, those selected in the model chosen
  first, Ctrl+click or Ctrl+A for several, and a **Count**;
  `add_cut_sheet_parts`; logged `Cut sheet Add Parts dialog opened: 2
  bodies`) and **Export…** (DXF…, SVG… and the check **Part Names on the
  LABEL Layer**: `export_cut_sheet` with `labels`; the file dialog *Export
  Cut Sheet*; logged `Exported <path>: cut sheet CutSheet1, 2 parts`).
  The drawing tools' commands do nothing on a cut sheet (a status says
  so). The panel shows the cut sheet (Material, Scale 1:1, Parts, its
  warnings, Add Parts…, Material…) or the selected part: Body, **Face**
  (Largest Planar Face or one of the body's planar faces with its area,
  `cutPartFace`), **Rotation (°)** (`cutPartRotation`), Rotate 90° Left
  and Right, **Add Copy** (another part of the same body and face), and
  whether it leaves the sheet or overlaps others. A part's menu: Rotate
  90° Left, Rotate 90° Right (`turn`), Add Copy, Delete Part; the sheet's:
  Add Parts…, Material…. A part whose body is gone keeps a red box at its
  place (the view's `broken`, as a drawing's). The model's warnings
  (overlaps, parts off the sheet, missing bodies) go to the status bar
  and the log when they change (`Cut sheet warnings: …`, `none`). Tested
  by `tools/ui-drawings-test.sh` (Kind Cut Sheet, two bodies added, DXF
  written).

## Print bed face

A body's print bed face (mitcad#731; the model's `set_print_bed_face`, the
`bodies` query's `print_bed_face` and `export`'s `on_bed`, see
`core/model-api/src/commands.md`, "Print bed faces"):

- **The view's context menu** (`MainWindowContextMenu.cpp`), with one face
  of a body that is not a surface body selected (a right click selects
  it), in a window that can change the design: **Set as Print Bed Face**
  on a planar face (`geometry` `plane`), **Clear Print Bed Face** on the
  body's print bed face instead (the face one of those the stored name
  finds at the marker, `print_bed_face.faces`, by name or by the faces of
  the shape, `Shape::find_faces`). A stored face that is gone or not
  planar (`print_bed_face.error`) is cleared with **Clear Print Bed Face**
  on any face of the body, beside Set on a planar one. A surface body
  that keeps a face (it became a surface body, or a file stored one;
  `print_bed_face.error` `a surface body is not printed`) offers Clear
  Print Bed Face only, on any of its faces, and shows no bed face
  (mitcad#737). Each runs
  `set_print_bed_face` (one
  undo step, `Set Print Bed Face of Body1` / `Clear Print Bed Face of
  Body1`) and logs `Print bed face of Body1: F2:end(…)` / `Print bed face
  of Body1: none`. No new command definitions: the entries are the menu's
  own, as Appearance… and Material… are.
- **Seeing it:** while the context menu of another face of a body with a
  print bed face is open, that face is highlighted in green (not
  pickable), in the occurrence of the face clicked
  (`MainWindow::m_printBedShown`, drawn by `updateIdleView`, cleared when
  the menu closes), logged `Print bed face shown: F2.b0/F2:end(…)`. The
  body's browser row's tooltip adds a line `Print bed face: end face of
  Extrude1` (the role and the feature of the face's name: end, start,
  side, top and bottom; `a face of Base1` for other roles), or `Print bed
  face: gone` / `not planar` / `not used, a surface body is not printed`
  for the model's three reasons, else the model's reason as it is (a
  renamed face whose plane is not found, mitcad#737)
  (`DocumentSnapshot::Body::printBedFace`, `printBedFaces`,
  `printBedError`; `printBedText` in `browser/BrowserPanel.cpp`).
- **Print Analysis** (`inspect.print_analysis`): a new analysis's
  **Bed face** starts with the print bed face of its bodies by the
  export's rule: only when exactly one of them has a usable one
  (`storedBedFace` in `commands/InspectCommands.cpp`, in the occurrence
  the body was taken in), logged `Print Analysis bed
  face: F2.b0/F2:end(…)`; another face picked or the face taken out
  overrides it. Editing a kept analysis keeps its own `bed_face`.
- **3D Print** (`make.print3d`, `files/MainWindowPrint.cpp`) exports with
  `"on_bed": true`, 3MF and every STL file alike: one bed face among the
  bodies turns them onto it and centres them on the plate, several keep
  them as assembled. Logged per 3MF file `3D Print bed: Body1 on F2:end(…),
  plate 128.000,128.000, transform 1.000 0.000 0.000 118.000 | 0.000
  -1.000 0.000 138.000 | 0.000 0.000 -1.000 20.000` (the rows of what moved
  the bodies after the design's placements, three decimals), `3D Print
  bed: kept as assembled: F2.b0, F3.b0` or `3D Print bed: as in the
  design`, each bed face that cannot be used added (`; Body1: the print
  bed face is gone`); per STL file `3D Print bed Body1.stl: …`. A face
  that cannot be used is a warning of the status line, as the export
  says it (`Body1: the print bed face is gone; printed as in the
  design`), with the print settings' warnings.
- Tests: `tools/ui-print-bed-test.sh` and
  `tools/ui-windows-print-bed-test.ps1` (ctest `app.ui-windows-print-bed`):
  Set as Print Bed Face from the context menu on a box's top (the file
  keeps it), a side's menu showing it and offering Set, Clear on it and
  Undo; 3D Print to a 3MF file turned over onto the top and centred on the
  plate (the logged transform); Print Analysis starting from it; a design
  whose stored face is gone reported in the log and the status line, then
  cleared from a side's menu, after which 3D Print warns no more; a box
  made a surface body by Unstitch that keeps its face, whose menus offer
  Clear only and clear it (mitcad#737).

## Image references

Images placed in the design to model from (mitcad#782; the model's
`insert_image_reference`, `edit_image_reference`,
`calibrate_image_reference`, `set_image_reference_visible`,
`rename_image_reference`, `delete_image_reference` and the
`image_references`, `image_reference_image`, `image_reference_outline`
and `image_info` queries, see `core/model-api/src/commands.md`, "Image
references"). The code: `MainWindowImageReferences.cpp` (the commands,
the refresh, calibration), `view/ImageReferences.*` (the query's entries
as drawn, points on an image and its plane, the log line; plain Qt Core),
`view/ImageReferenceView.cpp` (`OcctViewer::setImageReferences`) and
`view/ImageCalibration.*` (the two clicks).

- **Image Reference** (`insert.image_reference`, SOLID INSERT, icon
  `image-reference`): a panel of a model command (`modelCommand`). Inputs:
  Image (the file's path, typed) with **Choose Image…** under it (a note
  row whose button opens the Import group's file dialog, its filter PNG,
  JPEG, BMP, WebP and, in builds with the HEIF decoder, `.heic` and
  `.heif`; the path goes into Image when the panel shows its inputs
  again, `modelChanged`), Plane
  (a datum plane or planar face, `kPlaneKinds` and `isPlanar`, 0 to 1 so
  that an edit may keep a plane the panel cannot show), X and Y (the
  centre in the plane's frame; a plane picked, or selected before the
  command, sets them to its middle, `centreOnPlane` with
  `image_reference_outline`'s `center`), Angle, Flip, Width (empty: from
  the file, or unchanged when editing; `withAuto`), Opacity % (0 to 100,
  default 50) and Through Bodies. OK runs `insert_image_reference` with
  the file's absolute `path`, `center`, `angle` in degrees, `flip`,
  `width` when typed, `opacity` / 100 and `through` (no label of the
  panel's: the undo step is the model's `Insert Image1`), and logs
  `Inserted image reference from plan.png`. A HEIF file (mitcad#850) is
  decoded first (`heifProblem`, files of at most 32 MB), since MitCad's
  own decoder draws it: one it cannot decode, or any HEIF file in a build
  without the decoder, is refused with its reason (`The image cannot be
  read: the HEIF image cannot be decoded: …`). While the panel is open the
  image's outline is drawn (`viewLines`, from `image_reference_outline`; a
  new image's pixels and pixel size from `image_info`).
- **Editing** (a double-click on the image's browser row, or Edit Image
  Reference in its menu; `MainWindow::editImageReference`): the same
  panel, titled `Image Reference (edit)` and logged `Editing image reference
  Image1 with Image Reference` (`CommandDef::editedKind`), filled from the `image_references`
  entry (`load`, kept as the state's object `image`; the width from the
  entry's `size`; Plane left empty when the entry has an `error`, so that
  another plane is picked). OK runs `edit_image_reference` with every
  value (the plane only while one is picked) and logs `Edited image
  reference Image1`.
- **Calibrate Image Reference** (`insert.calibrate_image_reference`, an
  action, icon `image-calibrate`; also in an image row's menu, for that
  image, refused with `Show Image1 first.` while it is hidden): from the
  ribbon on the shown images, the first click taking the one it lands on
  (the nearest along the ray, within 2 % of its edges, `imageFractions`;
  one image alone takes any point of its plane). It logs `Calibrate Image1
  started` (with several, `Calibrate started on Image1, Plan`) and the
  places a tenth of each image in from its edges through its middle,
  `Calibrate Image1 left at x,y` (right, top, bottom; window coordinates,
  again when the camera comes to rest). The two left clicks in the view
  (the view's own left button off, `OcctViewer::setSketchInput`) are
  points where the clicks' rays meet the image's plane, logged `Calibrate
  Image1 first point (x, y, z)` and `Calibrate Image1 second point (x, y,
  z), d mm from the first`; between them a cross marks the first point and
  a line runs to the pointer (a `ManipulatorOverlay`'s lines). A dialog
  `Calibrate Image1` then asks for the distance, prefilled with the
  measured one in the design's unit (a bare number in that unit, else an
  expression the model evaluates), and `calibrate_image_reference` with
  the two `points` logs `Calibrated Image1: d mm is D mm, s mm per pixel`.
  Esc, a command, or one of its images gone or hidden ends it (`Calibrate
  Image1 cancelled`). The middle and right buttons still navigate.
- **The view** (`MainWindow::followImageReferences`, after each refresh):
  the shown entries with their corners, each picture fetched once by its
  digest (`image_reference_image`, decoded by `decodeImageFile`,
  `view/ImageFile.*`: HEIF through libheif and its HEVC decoder libde265
  in builds with `MITCAD_HEIF`, upright as the file's `irot`, `imir`
  and `clap` say, 8 bits per channel, with its alpha plane; the rest with
  `QImage::fromData`, a WebP file needing Qt's image format plugin),
  logged `Image reference Image1 decoded: 200 x 100 pixels` or `Image
  reference Image1 cannot be drawn: <why>`. Each is a textured quad (at
  most 4096 pixels on a side) with the opacity in its alpha, unlit, both
  sides, in front of a face it lies on (polygon offset); Through Bodies in
  the view's image layer, without depth, just before the sketch halos'
  layer: over the bodies and their edges, under the sketches, previews and
  highlights (logged once `Image references through the bodies: in the
  image layer, before the sketch halos`). Never picked. Logged when it changes: `Image
  references drawn: Image1 50.0 x 25.0 mm 40% behind, Plan 100.0 x 50.0
  mm 70% through` (`imageReferencesLog`), or `… none`.
- **The browser**: the root's **Images** folder (`ImageReferences`, its
  menu Insert Image Reference) and a row per image (`ImageReference`, uid
  its name, icon `image-reference`): its light bulb runs
  `set_image_reference_visible`, F2 `rename_image_reference`, Delete
  `delete_image_reference`; the tooltip gives the file's name and the
  size, or why its plane is not there (the row red).
- Tests: `tests/image_references.cpp` (`app.unit`),
  `tools/ui-image-references-test.sh` and
  `tools/ui-windows-image-references-test.ps1` (ctest
  `app.ui-windows-image-references`): inserted on a plate's top from a
  BMP file, calibrated by two clicks and 80 typed, edited (opacity,
  Through Bodies), hidden, saved, opened again and shown; in the Linux
  test also a HEIF file turned a quarter by its `irot`, decoded upright
  and drawn, and read back from the design after the file is gone.
  `tests/heif_image.cpp` (`app.unit`): HEIF files told from the other
  formats, and files the test makes around `tools/heif-test-fixture.py`'s
  HEVC data (64 x 32 pixels, four coloured quarters, 8 and 10 bits per
  channel) decoded with their quarters where a turn of 0, 90 or 270
  degrees puts them; a file cut short refused with the decoder's reason.

## Bindings: Varies with, Suppress When, Show When

Values that follow expressions of parameters (mitcad#809, mitcad#861; the
model's mitcad#800 and its `expression` fields of `set_body_appearance`,
`set_face_appearance`, `set_body_material`, `suppress_feature`,
`set_body_visible` and `set_occurrence_visible`, see
`core/model-api/src/commands.md`, "Bindings: values from expressions").
The code: `browser/Bindings.*` (the condition dialog, the switch() form
`OptionMapping`, the fx badge), `browser/AssignmentPanel.*` (Varies with
in the Appearances and Materials windows), `BrowserController::{suppressWhen,
showBodyWhen, showOccurrenceWhen}` and the menus.

- **The menus.** A body's browser row: **Show When…** after Hide/Show; an
  occurrence's row: **Show When…** after Hide/Show; a feature's menu (the
  timeline's, and its rows in the browser): **Suppress When…** after
  Suppress Features. The view's menu: **Show When…** for one body. There
  is one place for a body's appearance and one for its material
  (mitcad#861): **Appearance…** and **Material…** open the Appearances and
  Materials windows, whose assignment panel binds them to an option (the
  submenu Follow Option of mitcad#809 is gone). While a target follows an
  expression (a body's appearance or material, a picked face's own
  appearance), its entry carries an fx mark: `boundIcon` on its icon and
  `fx` after its text (`Appearance…  fx`, the text's shortcut column);
  logged `Context menu fx: Appearance… | Material…` beside the menu's own
  line, which names the entries without the mark (`ui_menu_choose
  "Appearance..."` takes them either way).
- **Suppress When… and Show When…** (`ConditionDialog`, titled `Suppress
  Extrude2 When`, `Show Body1 When`, `Show Board:1 When`): an expression
  field (`bindingCondition`, the parameters' suggestions) with **Formula
  Editor…**, **Remove Condition** (when there is one), OK and Cancel. OK
  runs the command with `expression` (one undo step: `Suppress Extrude2
  When`; an empty field removes the condition and keeps the value), and
  logs `Suppress Extrude2 when Holes = 'No'`, `Show Body1 when …`, `…
  removed`, or `… refused: <reason>`. Logged: `Condition dialog <title>:
  <condition or none>`, `Condition field|formula|remove|OK at x,y`.
- **Varies with** (`AssignmentPanel`, mitcad#861; one widget above the
  button row of the Appearances and the Materials windows, shown while the
  window has targets: bodies and faces in Appearances, bodies in
  Materials, none for a component's default material or in My Library).
  Its header line: the title (`Assign to Body1`, `Assign to 3 Bodies`,
  `Assign to 2 Faces`, `Assign to 3 Items`), `Now:` with what the first
  target has (its own value, else what it shows, the `body_material`
  query's material or its appearance; `Default` when none; only with
  None), **Varies with** (`assignmentVaries`: None, the configuration's
  options from the `configure` query, then the user parameters with a
  `values` limit; with neither, **Add Option…**, which opens the
  Configurations window) and an **fx** mark while the targets are bound.
  The button row keeps New, Delete, Assign and Close.
  - **None**: the window's Assign and a double-click in its list (new in
    both windows) assign as before: the value set by hand, which ends a
    binding (the model's rule).
  - **An option**: a table (`assignmentRows`) of a row per value (its
    `domain` list, or the parameter's `values`) and its result's swatch and
    name, the last column always stretched; the design's value now (the
    option's `expression`/`text`) reads `Black (now)`, and the selected row
    is the one the next pick fills (no mark of its own). **Formula Editor…**
    sits at the table's right edge under it. An **Other** row (the `switch()`'s
    default) only for an option whose domain is no list, with a row for the
    value now when no row has it. A chosen option is prefilled with what
    the first target has, so nothing is written until a row changes; an
    option the targets follow fills the rows from their `switch()`
    (`OptionMapping::parse`; values it names that the option no longer
    lists keep their rows; a closed list's default is kept as it was).
  - **Filling**: click a row (Shift and Ctrl select several), then
    double-click an item of the list or press Assign, whose text names the
    row (`Assign to row Red`, `Assign to 2 Rows`; with several targets
    `Assign to row Red (3 bodies)`); New, Duplicate (Materials)
    and a texture's Use (Appearances) fill the active row with what they
    made. Each fill writes the whole expression at once
    (`switch(Color; 'Black'; 'paint_black'; 'Red'; 'paint_red')`, rows
    without a result left out, an Other row's result as the default):
    `set_body_appearance`, `set_face_appearance` (the faces by body) or
    `set_body_material` with `expression` for every target, one undo step
    (one target: the command's own, `Set Appearance of Body1`; several:
    `Appearance of Body1, Body2 and Body3`, `Material of …`). The next row
    becomes active; after the last row none is, so the next pick cannot
    overwrite it unasked: Assign reads `Assign`, disabled with the tooltip
    `Click a row to fill it`, and a double-click says so under the table,
    until a row is clicked. A material of a library that is not
    in the design is first copied in by `set_body_material` with its
    `library` in the same step, and named by its name; Default (Materials)
    fills the root's default material, else `steel`. No OK: Close only
    closes.
  - **None again** after a binding: the result of the value now (else what
    the first target has) set by hand for every target, one undo step.
  - **Custom**: an expression of another form shows `Custom:
    <expression>` with Formula Editor… and **Replace with a table** (the
    first option, prefilled, written at the first fill); it is never
    rewritten by itself. The Formula Editor's expression is written for
    every target.
  - **Several targets**: all in the same state are shown and written
    together. Different states (bound to other expressions, other values)
    show a tinted notice with a warning sign (`assignmentMixed`) above the
    table: `Mixed: Body1 and Body2 follow Color, Body3 has Steel - Satin.
    Filling a row gives all 3 bodies this table.` (`both bodies` for two;
    with None `Assign gives all 3 bodies the selected appearance.`). The
    table shows the first target's state; nothing is written until the
    first change, which writes the shown table to all targets.
  - Logged (the window's name first, `Appearances` or `Materials`):
    `<window> panel: Assign to Body1; varies with None (Steel - Satin)`
    (`Color (bound)`, `Color (not written)`, `custom: <expression>`),
    `<window> varies with: None | Color`, `<window> mixed: Mixed: …`,
    `<window> value Black: Paint - Black (now) (active)` for each row, `<window>
    varies at x,y`, `<window> value Black at x,y` (the rows in sight; `Other`),
    `<window> formula at x,y`, `<window> replace_table at x,y`, `<window>
    varies with Color: 2 rows, not written yet`, `<window> active row:
    Red` (`none` after the last row), `<window> assign label: Assign to row
    Red`, `<window>: no row to fill`, `<window> filled Black
    with 'paint_black'`, `<window> binding of Body1: <expression>` (`…
    refused: <reason>`), `<window> back to None: paint_red for Body1`,
    `<window> from the Formula Editor: <expression>`, `<window>
    double-click: <id or key>`, `<window>: Add Option…`.
- **The marks.** A bound body's, occurrence's or feature's browser row has
  the fx badge after its name with the count of its bound properties (`fx
  3`, mitcad#862: the card below), logged `Browser fx Root/Bodies/Body1:
  Appearance = switch(…)` at each rebuild (the expressions, and a body's
  warnings); a feature suppressed by a
  condition has the badge in the timeline and `Suppressed when …` in its
  tooltip. `DocumentSnapshot` reads `suppressed_when`, `visible_when`,
  `appearance_expr`, `material_expr`, `face_appearance_exprs` and a body's
  `warnings`.
- **By hand.** Appearance…, Material…, the light bulbs, Hide and Suppress
  Features send the value, which removes the expression in the same undo
  step (the model's rule).
- **Light bulbs and Show When** (mitcad#863): a conditioned body's or
  occurrence's light bulb is an fx bulb (the plain bulb with a small fx
  corner badge; a folder's bulb stays plain) whose tooltip gives the condition,
  what it gives now and that a click removes it; the click says so in the
  status bar (`Show When of Body2 removed – Undo brings it back`). The
  Bodies folder's light bulb and Show All / Hide All, the view's Hide of
  several items and Find in Design's Show and Hide leave conditioned items
  to their conditions. See the light bulbs in "Browser, timeline and
  parameters" above; test `tools/ui-show-when-test.sh`.
- Tests: `tools/ui-bindings-test.sh`: no menu has Follow Option; Body1's
  appearance varies with the text parameter Color in the Appearances
  window (its allowed values, nothing written when chosen, the rows filled
  by double-clicks, the next row active), Color = 'Red' in Change
  Parameters turns it red and the menu marks Appearance… with fx; Body2's
  material varies with Color in the Materials window; None keeps the
  shown appearance and undo brings the binding back; three bodies of
  different assignments show the Mixed notice and the first fill writes the
  table to all three in one undo step; Box2 suppressed by `Holes = 'No'`
  and back with Holes = 'Yes'.

**Bindings at a glance** (mitcad#862): what an item follows, in words.
The code: `browser/BindingList.*` (the design's bindings from the
`bodies`, `timeline` and `components` queries, the parameters' values and
the appearances' and materials' names and colours, and the words:
`readBinding`), `browser/BindingsCard.*` (the card),
`browser/ConfigurationBindingsPage.*` (the Configurations window's
Bindings tab) and `BrowserController::{browserBindings, timelineBindings,
editBinding, selectBinding, showAllBindings}`. No query field was added.
The UI says "follows" and "when" for bindings, never "rule" (rules are
the configuration's).

- **The fx badge.** A bound browser row draws a solid pill `fx N` after
  its name (and after a component's radio button; dark blue semibold text
  on a light blue fill, light text on a dark blue fill in a dark theme, so
  that it stays legible on a selected row's band), N its bound properties: a
  body's appearance, material, Show When and each face with an appearance
  of its own; 1 for a feature's Suppress When and an occurrence's Show
  When (`BrowserDelegate::Parts::fx`, the role `kBindingsRole`; the row's
  box with the Rows background reaches its end). The pointer resting on it
  (450 ms, `BrowserPanel::hoverBindings`) shows the card until the pointer
  leaves the badge and the card; a click pins it (a popup: Esc or a click
  outside closes it). The timeline's badge on the icon of a feature with
  Suppress When does the same (`TimelineStrip::badgeRect`; the strip's
  hover pause). A native presentation's rows keep the fx mark on the icon
  (`presentationRows`, `boundIcon`).
- **The card** (`BindingsCard`): `Body2 follows` with the options it uses
  and their values now (`now: Color = Black, Holes = No`), then a line per
  property: a `switch()` over one option (`OptionMapping`), or an `if()`
  whose conditions compare one option with values (`if(Color = 'Black';
  'paint_white'; 'paint_yellow')`, nested ifs too), as a case per value
  (`Black → Paint - White`) with the result's swatch (a filled disc of its
  `display_color` with a darker border, `bindingSwatch`), the current case
  marked ●, a result that names no appearance or material in the warning
  colour. The default case is `otherwise → …` for an option without a list
  of values; for one with a list (its `values` limit) it is named by the
  values it covers (`Red → Paint - Yellow`), or left out when the cases
  list them all. A face is `Face 1 (side)` (its number among the body's
  bound faces and the kind its topological name gives; the name itself in
  the tooltip). A condition reads `when Holes = Yes · hidden now`
  (literals without quotes, `==` as `=`, `<>` and `!=` as `≠`); another
  expression its text so, ids of appearances and materials as their names,
  and what it gives now (`· Paint - White now`), elided with the
  expression as written in the tooltip. A body's appearance and material
  that follow nothing are plain grey lines (`Steel, set by hand`, `Steel,
  the default`, `Paint - Red, from its material`); the model's warnings
  stand under their line. The card is at most 460 px wide, its texts
  elided (whole in their tooltips). **Edit…** per line, a link (the link
  colour, underlined, darker under the pointer): the Appearance window for
  the body or the face (`openAppearances`), the Materials window for the
  body (`openMaterial`), Show When… or Suppress When… (the occurrence's by
  its path). **All bindings of the design**, the same kind of link, opens
  Configurations on the Bindings tab. The card reads the model again after
  every change while it shows.
- Logged: `Browser fx badge Root/Bodies/Body1: fx 3`, `Browser fx badge
  Root/Bodies/Body1 at x,y`, `Browser fx click Root/Bodies/Body1`,
  `Timeline fx Box2 at x,y`, `Timeline fx click Box2`, `Bindings card shown
  for Body1 (hover|pinned)`, `Bindings card Body1 follows; now: Color =
  Black, Holes = No`, `Bindings card line Appearance: Color: Black →
  Paint - Black ●, Red → Paint - Red`, `Bindings card line Face 1 (side):
  Color: Black → Paint - White ●, Red → Paint - Yellow`,
  `Bindings card line Shown: when Holes = No · shown now`, `Bindings card warning
  Appearance: no appearance "x"`, `Bindings card Edit Appearance at x,y`,
  `Bindings card all bindings at x,y` (the main window's coordinates),
  `Bindings card Edit Appearance of Body1`, `Bindings card all bindings`,
  `Bindings card closed`.
- Test: `tools/ui-bindings-glance-test.sh [screenshot folder]` (the card
  on hover and pinned, Edit… opening the Appearance window, an
  occurrence's card, a feature's card from the timeline, and the Bindings
  tab below; with a folder, `bindings-card.png` and `bindings-tab.png`).

## Chat (mitcad#947)

The chat panel ([docs/ai-chat.md](../docs/ai-chat.md), `app/agent/`): the
user's own agent (Claude Code or Codex) changes the open design through
the window's live MCP server.

| Command | Does |
|---|---|
| `view.chat` Chat | shows or hides the panel (View menu, checkable; its button `titleChat` beside the agent indicator in the title row, the Docked toolbar; Ctrl+Shift+L, ⌘⇧L on macOS; the dock's close button too). Any mode, also while a panel is open |

- Place: Floating, the card `chatCard` on the right edge
  (`FloatingLayout::Anchor::Right`: from below the top rows to above the
  timeline, 340 wide; the upper right cards and the orientation cube move
  left of it); Docked, the dock `chatDock` on the right. The panel itself
  is `chatPanel` in both. The native AppKit window has no panel yet.
- The panel (`agent/ChatPanel`): a header (the agent `chatAgent`, or the
  choice `chatAgentChoice` once more than one agent is found: the agents
  found on this computer and the chosen one; another one starts a new
  conversation in it and is saved as `chat/agent`, the last used one; New
  conversation `chatNew`, the menu `chatMore`: New Conversation, Clear
  History of This Design, Chat Preferences… (`tools.preferences.agents`)),
  the line Model: `chatModelChoice` (editable: Default model, the agent's
  models (Claude Code's Opus, Sonnet, Haiku; Codex's `model/list`, kept as
  `chat/models/codex`), or a typed name; saved per agent as
  `chat/model/<agent>`, given as `ChatAgent::Start::model`; an idle agent
  restarts with it on the next message, resuming its session; disabled
  while the agent works) and the reported model `chatModel` (dimmed),
  the conversation `chatConversation` (`chatUser`, `chatAnswer` as
  Markdown, `chatTool` rows with `chatToolHead`/`chatToolDetails` and the
  property `toolState` running, done, failed; approval cards
  `chatApproval` with `chatAllowOnce`, `chatAlwaysAllow`, `chatDeny` and
  the property `cardState` open, allowed, always, denied, gone;
  `chatNotice`), the state line `chatState`, the privacy notice
  `chatPrivacy` (`chatContinue`, `chatCancel`), the setup's button
  `chatSetup` (Install Claude Code, Update Claude Code, Sign in to Claude,
  Install Codex, Update Codex, Sign in to ChatGPT, Check again) with
  `chatConnectAssistant` (Claude Code chosen, Claude Desktop only), the
  field `chatInput` (Enter sends, Shift+Enter a new line, Esc stops),
  `chatAttachSelection` (on), `chatAttachView` (off) and `chatSend` (Send;
  Stop while the agent works, property `chatAction` send or stop). The
  panel's property `chatState`: unavailable, checking, missing, outdated,
  signed-out, signing-in, notice, ready, starting, working, stopping.
- Preferences › AI Assistants › Chat (`agent/ChatPreferences`, below the
  folders): Agent `chatAgentSetting` (Claude Code, Codex; a change switches
  an open panel as its choice does), Approvals `chatApprovals` (Ask every
  time, Model edits allowed, Allow everything; the settings
  `chat/agent`, `chat/approvals` ask, model, all, `chat/history`), Keep the
  chat history of each design `chatHistorySetting`, Clear all history…
  `chatClearAll` (asks), Reset Always allow `chatResetAlways` (the
  settings `chat/always/<agent>`).
- The approvals are enforced by the live server for the chat's own bridge
  connection (`mcp/ChatGate.hpp`), by the policy of
  `core/mcp/src/chat/policy.rs`.
- Logs: `Chat panel: <state>` (the property's value when it changes),
  `Chat state: <missing|checking|outdated|signed out|signing in|ready>` (the
  agent's), `Chat activity: <idle|starting|working|stopping>`, `Chat
  shown`, `Chat hidden`, `Chat: Claude Code appeared`, `Chat: Claude Code
  2.5.0 found, signed in`, `Chat: Claude Code's sign-in started`
  (`ended`), `Chat: install page of Claude Code`, `Chat: privacy notice
  accepted for Claude Code` (`cancelled`), `Chat: message written (21
  characters, 2 selected, view)`, `Chat: message queued (1 waiting)`,
  `Chat: message 1 sent (undo depth 3)`, `Chat: Claude Code started (new
  session <uuid>)` (`resuming session <uuid>`), `Chat: Claude Code started
  its session <uuid>, model <m>, MitCad's tools connected`, `Chat: tool
  model_command add_feature box`, `Chat: tool model_command done`
  (`failed`), `Chat: approval asked for document_save <path>`, `Chat:
  Allow Once for document_save` (`Always Allow`, `Deny`), `Chat: turn
  ended (success)`, `Chat: the message's 2 step(s) merged into 'AI: …'`,
  `Chat: the message's steps stay separate (user_edited)` (`sketch`,
  `panel`, `no_change`, `undone`), `Chat: the design changed between the
  agent's calls (before model_command); …`, `Chat: Stop`, `Chat: stopping
  Claude Code (SIGINT)` (`(ending the process)` on Windows), `Chat: Claude
  Code ended (exit 0, stopped)`, `Chat notice: <text>`, `Chat: history of
  12 entries shown, session <uuid> to resume`, `Chat: the conversation
  follows the design to <path>`, `Chat: new conversation (session
  <uuid>)`, `Chat: history of this design cleared`, `Chat panel: approval
  card '<text>'`, `Agent indicator: chat working: Claude Code`. The agent
  switcher and the model: `Chat: agents found: claude, codex`, `Chat:
  agent Codex (found: claude, codex)` (at the start), `Chat: agent Codex`
  (chosen), `Chat: model test-codex-large for Codex` (`default`). Codex:
  `Chat: Codex 0.95.0 found, signed in`, `Chat: Codex's sign-in started`
  (`ended`), `Chat: Codex started (new thread)` (`resuming thread <id>`),
  `Chat: Codex started its session <thread>, model <m>, MitCad's tools
  pending`, `Chat: MitCad's tools connected in Codex` (`failed`), `Chat:
  Codex offers 2 model(s)`, `Chat: stopping Codex (turn/interrupt)`,
  `Chat: Codex did not stop its turn in 5000 ms; ending it`, `Chat: Codex's
  conversation cannot go on: <reason>`, `Chat: Codex ended (exit 0)`,
  `Chat: Codex's conversation closed`, `Chat: turn ended (completed)`
  (`interrupted`, `failed`, `error`).
- Tests: `tools/ui-chat-test.sh` (Linux, Xvfb) and `app.ui-windows-chat`,
  `app.ui-macos-chat` run `tools/ui-chat-test.py` with the scripted agent
  `test_chat_agent` (`app/tests/chat_test_agent.cpp`, copied as `claude`
  and as `codex`, whose app-server mode speaks Codex's JSON-RPC) through
  the real bridge, also the runs `codex` (the switcher, Codex, its model,
  Save As, Stop) and `codex-again` (the last used agent and model);
  `core/mcp/src/chat/tests.rs` and `core/mcp/src/chat/codex_tests.rs`
  (cargo) and `app/tests/chat_history.cpp` (`app.unit`).
