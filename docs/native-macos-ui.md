<!-- SPDX-License-Identifier: MIT -->
# Native macOS UI

The AppKit frontend uses the existing CAD model, commands, file formats,
undo history, imports and OCCT viewport. Its outer window and general
controls are AppKit. The shared session owns a single document; enabling
the frontend does not introduce multiple documents or a new renderer.

## Design contract

The native frontend preserves the default frontend's information structure:
menu and command locations, ordering, workspace groups, CAD artwork and
workflow capabilities. AppKit supplies the controls, typography, materials,
focus, accessibility and system conventions. Differences in those platform
conventions must preserve the task and its discoverability.

The generic presenter bridge is an implementation tool, not a design limit.
A tile picker still needs its pictures, groups and tile interaction; a tree
still needs its editing, keyboard and move actions; a parameter table still
needs its validation states. Use a dedicated AppKit presentation where the
generic bridge cannot preserve these capabilities. Fixed prototype dimensions,
missing interactions and historical descriptions of absent panels are not
requirements. Validate visible content and ordinary input against the default
frontend, beyond merely opening each native window.

Qt remains the default macOS frontend. macOS builds and releases carry
both frontends; the AppKit one is an opt-in preference (see
[Releases and the preference](#releases-and-the-preference)). Making AppKit
the default needs a maintainer decision of its own. The interaction
sections below describe implementation and validation progress, not an
assertion of complete parity.

## Coverage

New UI reaches the AppKit frontend without a native counterpart of its own
when it is built from declarative commands ([COMMANDS.md](../app/COMMANDS.md))
or from Qt's own controls: `createNativeCommandForm` presents the commands,
and the generic bridge (`renderWidget` in `NativeDialogs.mm`) maps the
controls of dialogs, cards and panels. Two things escape the bridge, and
two checks keep them from being lost without a word:

- A widget that paints itself (overrides `paintEvent`): the bridge shows
  only its children. Every such class of `app/` has a line in
  [native-coverage.txt](../app/platform/macos/native-coverage.txt) saying how
  the AppKit frontend shows it: `native` (an AppKit view presents it),
  `replaced` (Qt chrome the AppKit frontend replaces), `shared` (drawn by Qt
  over the 3D view in both frontends) or `pending` (not shown yet). Each
  `native` and `replaced` line names its AppKit code, which must exist.
  `pending` lines are the gaps to close before AppKit becomes the only macOS
  frontend. What such a widget paints can often be given to the bridge as
  data (`app/platform/NativeDialogs.hpp`): an item view's hint while it has
  no rows is its `NativeEmptyTextProperty`, a colour button's swatch its
  `NativeColorFieldProperty`.
- A dialog, message box or menu that Qt shows itself (`QDialog::exec`,
  `QMessageBox::warning`, `QFileDialog::getOpenFileName`, `QMenu::exec`):
  it goes through `app/framework/Dialogs.hpp` instead (`execDialog`,
  `sheetWarning`, `dialogOpenFileName`, `execMenu`, ...), which hands it to
  the AppKit presenter.

The ctest `repo.native-coverage` (`tools/native-coverage`) checks both from
the source text on every platform, so a change made on Linux or Windows
meets it in its own scoped check. It reads the build's macOS branches only
(`#ifdef Q_OS_WIN` and the `#else` of `#ifdef Q_OS_MACOS` are left out)
and does not see painting done in event filters or item delegates.

At run time the bridge logs `Native presenter: unpresented widget <class>`
for a widget it presents as nothing: one of a class other than plain
`QWidget` or `QFrame` with no control it maps, no layout and no children.
Every macOS UI test (`ui_run_app` in `tools/ui-macos-lib.sh`, so the parity
audit too) fails on that line unless the coverage list marks the class
`pending`.

## Building the experimental frontend

```bash
cmake --preset dev
cmake --build --preset dev
ctest --preset dev --output-on-failure
```

Run these commands in the isolated macOS VM. The macOS presets
(`macos-debug`, which the VM's `dev` preset inherits, and `macos-release`)
set `MITCAD_MACOS_NATIVE_UI=ON`; the option itself stays OFF by default, and
`-DMITCAD_MACOS_NATIVE_UI=OFF` builds the Qt frontend alone. The deployment
target stays 14.4; newer AppKit APIs need availability guards.

## Releases and the preference

Owner decision of 1.11.2026 (mitcad#1056): macOS releases are built with
`MITCAD_MACOS_NATIVE_UI=ON` and ship both frontends. Every build starts in
Qt unless something chooses AppKit:

- **Preference:** Settings › General › **Native macOS interface
  (experimental)**, off by default (the setting
  `appearance/nativeMacInterface`), with the note *Takes effect when MitCad
  starts again.* It exists only in builds with the AppKit frontend. A
  change is saved at once and applies at the next start; the running
  window keeps its frontend. Settings is the shared Qt dialog, which the
  AppKit frontend presents through its bridge, so the checkbox is there in
  both frontends.
- **Command line:** `--ui=native` or `--ui=qt` overrides the preference for
  one run. `--chrome=docked|floating` chooses a Qt layout, so without
  `--ui` it starts Qt; `--ui=native` with `--chrome` is an error.
- **Log:** the start-up names the effective frontend and what chose it:
  `Frontend: qt (default)`, `Frontend: native (preference)`,
  `Frontend: qt (--ui)`, `Frontend: native (--ui)` or
  `Frontend: qt (--chrome)` (a native preference overridden by `--chrome`).
  Use it to verify which frontend a run had.

Tests: every UI test passes `--ui=native` or `--ui=qt` explicitly, so the
default does not decide what a test covers. `app.ui-macos-native-frontend`
checks the default, the preference across a restart, `--ui` and `--chrome`.
In a native-enabled build `cmake/MacUiTests.cmake` labels the tests of the
AppKit frontend: `app.ui-macos-native-smoke` on the smoke subset
(`app.ui-macos-native`: a command with OK, the Browser and the timeline;
`app.ui-macos-native-frontend`: start by the preference with a design
open, quit) and `app.ui-macos-native-full` on every other one. The
release's macOS ctest leaves the full native suite out
(`-LE app.ui-macos-native-full`) and so runs the whole Qt suite and the
smoke subset; merge checks and native work run everything. A new native
test named `app.ui-macos-native-*` gets the full label without more work;
one with another name goes into `mitcad_native_other_tests` there.

## Interaction

- The native shell follows the default Qt layout: Undo/Redo and the
  Solid/Build/Make workspace selector on the left, the document title in the
  centre, and Project, AI Assistant with its Chat button (mitcad#1075) and
  Command Search on the right.
  Sketch adds and selects the same temporary workspace tab. The Make
  workspace (mitcad#1049) is read from the shared ribbon as Solid and
  Build are (`MainWindow::refreshNativeActions` lists `MAKE`): its
  Materials, Annotate, 3D Print and Laser groups, their pinned buttons
  and menus. The selected body's Make card and the Browser's Make markers
  are Qt's selection card and rows; the native inspector and outline do
  not show them yet (a Mac follow-up). The native
  menu bar keeps File, Edit, View, Tools, Window and Help in the shared
  order; CAD groups belong to the ribbon rather than additional menu-bar
  headings. Settings and About retain their standard application-menu roles.
- Ribbon snapshots come from the actual shared ribbon widgets and menus.
  Both frontends use the same pinned button order, artwork, subgroup
  defaults, section separators, selection filters and full/collapsed group
  widths. AppKit supplies the segmented workspace selector, buttons,
  popups, system typography and appearance; CAD command icons retain
  their familiar colours. The rightmost groups collapse as whole button
  rows, matching the default layout, while their menus stay accessible.
- Repeated action snapshots reuse native artwork and preserve menu objects
  while only availability or checked states change. Artwork is cached by its
  Qt icon key, symbol, palette, template mode and logical size; palette changes
  discard the cached images. Shortcut bindings are parsed with each changed
  action structure rather than on every key event. Overlay layout requests
  repaint the viewport only when its size or backing scale changes.
  Settings, view controls and project utilities keep their intended menu-bar
  locations; search-only settings pages stay available in command search.
- The OCCT viewport spans the full document window width below the ribbon.
  The Browser floats over its left edge and the inspector floats on
  the right, leaving the canvas width unchanged when either card is hidden.
  The cards use [NSGlassEffectView](https://developer.apple.com/documentation/appkit/nsglasseffectview)
  on macOS 26 and newer, with NSVisualEffectView on earlier supported systems.
  Rounded edges and shadows stay within the document window, with
  unobstructed canvas gaps around the cards.
- The orientation cube stays the shared OCCT one. Its arrows, which turn the
  view to the face beside the one in sight or roll it by 90 degrees while
  a face is in sight, are round AppKit buttons in the cards' material at
  the Qt arrows' places, with the same SF Symbols and tooltips, over the
  canvas and under the cards.
- Trackpad magnification, rotation and smart zoom enter through the native
  document window and reach the shared CAD viewer with explicit local
  coordinates. A gesture starting on a floating card stays outside the
  canvas; a gesture starting on the canvas keeps its target until it ends.
  The existing viewer supplies zoom sensitivity and navigation settings.
- The Browser shows selections, names and visibility. Its context
  menu uses the shared browser command semantics. The outline and its
  scroll view stay transparent over the rounded native card. A tracking
  area on the outline reports the row under the pointer, and none when the
  pointer leaves the rows or the card collapses or hides (mitcad#1072,
  `treeHovered`), to the shared panel's hover (`BrowserPanel::hoverNode`):
  a body's, a component's or an occurrence's row highlights its bodies in
  the view (mitcad#1022, `DocumentHost::previewBodies`), a joint's row
  previews its joint (mitcad#523), with the same log lines and without
  meshing as in Qt (`app.ui-macos-native`,
  `app.ui-macos-native-browser-rows`).
  Row names keep the shared rows' colours with AppKit's (mitcad#1079,
  `NativeMacTreeItem::tone`, also the row's accessibility value): a failed
  row red, a warned one the warning's colour, a suppressed, rolled-back or
  hidden one, or one under a hidden parent, grey. A light bulb that
  follows a Show When condition has the fx mark and says what a click
  does, a folder's of such rows names the bodies it leaves as they are
  (`eyeTip`, `eyeBound`, mitcad#863).
  Browser rows use the shared CAD artwork and initial expansion state,
  with visibility before the icon and component activation after the name.
  Its header retains Find in Design. Projects is an AppKit presentation of
  the existing Projects presenter, stacked above Browser and controlled
  by the same View > Projects action. Its native toolbar and outline bind
  the same commands, item model, selection and edits.
  Projects uses a bounded native outline below its toolbar. Its height follows
  actual AppKit row geometry, including source-list insets, and long lists
  scroll within that viewport. The Configuration card and Browser follow
  below it in the shared left column (mitcad#1020, "The Configuration card
  and the left column").
  The Projects disclosure arrow collapses only the content and leaves its
  header available to reopen it. View > Projects independently remembers
  whether the whole panel is shown across application restarts; before
  the user's first choice it starts hidden (mitcad#1020), as in Qt.
  Failure and warning counts appear in a native row below the Browser title,
  with the shared messages as tooltips and shared issue navigation on click.
  Their retained Qt presenters stay hidden so they cannot paint under the card.
- The inspector stays hidden until a command, sketch or selection needs it.
  It hosts one native command form, selection details or the sketch palette.
  Expressions, decimal commas, choices, repeated inputs, selection lists,
  errors and previews use the existing command definitions. Icon choices use
  native buttons with the shared CAD icons and option labels as tooltips.
  Disclosure rows reveal their dependent inputs while keeping the inspector's
  height cap and scrolling rules.
- View > Browser and View > Timeline control the shared floating cards.
  The inspector follows the active command or selection; its optional native
  toolbar shortcut remains available through toolbar customization.
  Feature history is a compact horizontal timeline of the shared
  coloured CAD icons in native buttons, with its marker between features
  and native step controls. It floats above the canvas and supports edit,
  selection, reordering and marker movement. Resizing keeps visible cards
  inside the document and preserves their input routing above the embedded
  viewport. Marker drags use the shared rollback and undo transaction.
  The timeline is visible initially, as in the default frontend, and keeps
  playback, filter and search controls to the left of the feature strip.
  Both frontends share 28-point cells, 22-point artwork and 3-point gaps.
  The native card is 42 points high. Neutral cells are borderless; errors
  have a red fill and outline, warnings a yellow fill and outline, and import
  notes a warning outline. Selection preserves the issue fill.
- Settings, parameters, appearances, version/library/remote dialogs,
  reports, progress and alerts bind existing workflow presenters to native
  controls. File open/save panels use NSOpenPanel/NSSavePanel, and colours
  use NSColorPanel. AppKit text responders handle normal editing shortcuts.
- Preview, inspection and commit jobs run on the model worker. A slow commit presents a
  cancellable progress sheet. Closing during a command waits for cancel
  compensation before asking to save and terminating.
- Checkpoints for commands in the native inspector restore the original
  definition, saved state and undo/redo history even when Cancel arrives
  just after a computation committed. Accepted commands keep the usual
  single undo step.
- Sketch precision fields are AppKit text fields anchored to the canvas.
  They share the sketch controller's expressions, validation and Tab order.

The sketch editor retains its existing shared marker and undo/merge
workflow. Inspector command checkpoints do not replace that whole sketch
edit transaction.

Keyboard Shortcuts validates commands in their available contexts and keeps
conflicting saved assignments visible for repair. Native menus and key
dispatch use the registry's effective primary and alternate bindings.
Recording, menu equivalents and event dispatch share the Qt/AppKit key
conversion; Command remains Qt's portable Ctrl and physical Control its
Meta. The recorder's Clear button removes a binding, so Backspace and
forward Delete can themselves be assigned.

With Browser or history focus, F2 renames and Backspace or forward Delete
deletes through the shared controllers. Return activates the selected Browser
row, opening its sketch or feature editor; Return in a row's text editor
finishes renaming.
Command+F opens native timeline search, sharing the query, component/status
filters, result navigation and selection restoration with the Qt timeline.
The timeline's magnifier, Command+F and Search Timeline all open it as a
popover rising from that magnifier, with its field at the bottom next to the
timeline, as the Qt field sits in the timeline bar, and Filters…, Previous,
Next, the match count and the matches above it. Command search hangs from
the Command Search toolbar item when opened there, and from the top centre
of the window when opened by its shortcut or a menu (mitcad#1021).
The wheel over the native timeline scrolls towards older or newer features,
using its larger axis for diagonal motion. Precise trackpad deltas retain
their point values; the strip consumes scrolling at both ends so it cannot
zoom the canvas.
Hovering a native timeline cell previews its feature as the Qt timeline's
hover does (mitcad#1076): the strip reports the cell to the shared
`TimelineWidget::hoverFeature`, which sets the same hover state, so the view
draws the feature through the solids and a Fillet's or Chamfer's picked
edges; leaving the cell, a press, a marker drag, hiding the timeline and a
rebuild end it. A cell's tooltip is the shared timeline's
(`TimelineWidget::featureToolTip`, from the feature details provider:
kind, main dimensions, component and state), asked for only when AppKit
shows it, since the details read the model; AppKit's own tooltip delay
applies, not the Qt strip's 350 ms.
Timeline groups (mitcad#1077) come from `TimelineWidget::shownGroups`, the
Qt strip's own groups and folded state, with a group the marker is inside
of shown open; the native strip keeps no state of its own. An open group
has its band over its cells, and a click on the band folds it into one
folder cell, in the worst state of its features; a double-click on that
cell opens it again, and a right click on the band or the cell opens the
shared group menu (Collapse or Expand Group, Rename Group, Ungroup).
Hovering a folded cell previews all its features, and its tooltip names
the group and its size. The native strip logs `Timeline group <name> at
x,y` as the Qt strip does, in the window's points;
`app.ui-macos-native-timeline` checks hover, tooltips and groups.
Command search result rows keep each command's shared CAD icon, at the shared
search's 18 points, beside its name and shortcut: the same artwork as the
ribbon and menus in the palette's light or dark rendering, dimmed with the
name when the command cannot run (mitcad#974).
Command+W in command or timeline search dismisses that search and keeps the
document open. Removing Project through toolbar customisation remains in
effect after saves and other project-status changes.
Text editors keep their normal editing keys; document shortcuts do not
intercept input in an attached modal sheet. In Save with Description,
Command+Return saves and plain Return inserts a newline.

### Drawing tabs and pinned commands

While a drawing is open (mitcad#1073, after mitcad#1008), the workspace
selector shows its Drawing tab alone, Cut Sheet for a cut sheet: the
retained ribbon's `Ribbon::setDrawingMode` changes its tabs and
`refreshNativeActions` passes the DRAWING and CUT SHEET groups (Close
Drawing first, Export last) to `setRibbon`. Close Drawing brings Solid,
Build and Make back with the tab the drawing was opened from selected.
The drawing uses the ribbon's tools (`DrawingWorkspace::setRibbonTools`),
so its own bar keeps only the drawing's name, the sheet tabs, **+**,
Sheet… and Add Revision…. `NativeMacWindow::setDrawingOpen` is the
counterpart of the Floating layout's `showOnlyCards`: the Browser stays,
Projects, the inspector, the timeline and the orientation cube's arrows
are hidden, and when the drawing closes each card is shown or hidden as it
was asked to be meanwhile. The log reads `Native workspaces: Drawing (shown
Drawing)` and `Native cards: Browser only (a drawing is open)`.

A pinned command (mitcad#1074, after mitcad#1001 and mitcad#1084) is a
button in its own group, after the group's own buttons, read from the
retained ribbon's group row (`Ribbon::addPinnedButtons`, a button's
`mitcadPinId`; one remade by a pin change is skipped once it has left
the row's layout) and marked `NativeMacRibbonButton::pinned`
(`native.ribbon.pinned.<id>`); it folds with its group's buttons, and its
right-click menu has Unpin from Toolbar and Change Keyboard Shortcut….
An NSMenu row has no ⋮ button and no right-click, so each group menu (and
a split button's submenu) ends, after a separator, with two submenus
listing its commands as its rows (a subgroup's in a submenu of its own):
Pin to Toolbar, a pinned command checked and unpinned when chosen,
disabled for a command that already is a button of its group
(`NativeMacAction::pinnable`), and Change Keyboard Shortcut, which opens
Keyboard Shortcuts at the command. Both act once AppKit's menu has
closed (in the default run loop mode), so Keyboard Shortcuts and the
remade ribbon never start inside the menu's tracking loop. They call the
shared pins (`Ribbon::pin`, `Ribbon::unpin`) and
`MainWindow::editShortcuts`, so the log lines and the saved
`toolbarPins/<TAB>` setting are those of the Qt ribbon. The tests' menu
comparison with the retained Qt menus leaves these entries out (their
tag). The Qt view's navigation bar (mitcad#1086) is not made in the
AppKit frontend, so a drawing has nothing more to hide.

`app.ui-macos-native-familiar` pins Sweep from the Create menu, starts it
from its button in the Create group, unpins it from that button's menu,
opens Keyboard
Shortcuts at Sweep, then opens a drawing from the Build tab: the Drawing
tab alone with its groups, the timeline and inspector hidden and the
Browser shown, the menus compared with the retained ones, and Close
Drawing back on Build with the cards. `app.ui-macos-native-cut-sheet-parity`
checks the Cut Sheet tab, then runs Add Components… and the exports from
the AppKit ribbon's button and Export menu.

### Part catalogue and Component Picker

Insert from Library, Community Library and the Component Picker present the
shared part catalogue (`PartGrid`) with a dedicated adapter
(`platform/macos/NativeCollection.mm`, mitcad#813) rather than generic
rows. The groups are a source list of readable width with the shared
folding, Favourites artwork and a star on each library group; the search,
licence filters, system, Star and the tile-size slider keep their rows; the
parts are an NSCollectionView of the shared pictures, names, sizes, stars,
use counts and greyed refusals. The slider sizes the native tiles as it
sizes Qt's. A click makes a tile current and is the Qt view's click (the
picker chooses the part); a second click or Return activates it (Insert
from Library inserts); the arrows, Home and End move between tiles, Up and
Down in the search too; Command+D and Command+Shift+D star the part and the
group. Resting on a tile shows the larger picture with its supplier, part
number, library and licence in a card that never takes the keyboard.

The adapter reads only properties of the shared widgets, so another picker
can reuse it without platform code: `nativeTileCatalogue` on the composite;
on an icon-mode list `nativeTileKeyRole`, `nativeTileImageRole`,
`nativeTilePreviewRole`, `nativeTileSubtitleRole`, `nativeTileStarRole`
(turned on and off with `setData`), `nativeTileBadgeRole` and
`nativeTileDisabledRole`, with the tooltip role as the hover details; on
the group tree `nativeGroupKeyRole`, `nativeStarRole` and
`nativeStarMethod`, an invokable of the composite. Any other icon-mode list
becomes the same grid with its decoration and display roles. A QSplitter's
panes keep its sizes and stretch factors, a draggable divider and each
pane's minimum, so the catalogue and the part details share the dialog as
in Qt.

The modeless Component Picker opens through the shared `showDialog` in a
floating AppKit utility panel (`nativeUtilityWindow`, mitcad#819). It does
not take the keyboard from the document: a click on a tile chooses the part
while keys still reach the running command; search and editable option
controls take the panel's keyboard focus. Pinning leaves that focus as it
was: select the document before using Escape to cancel its command.
A pinned picker stays when its command ends and the next
command takes the same panel over; reopening brings it forward without
activation. Close, Escape in the panel, Command+W and its close button hide
the presenter and return the keyboard to the document. The panel keeps its
frame under `partPicker/nativeFrame`; the hidden Qt presenter never appears
on screen. The BUILD commands' Component tile (`InputDef::choiceTile` with
`onButton`) is a native tile with the chosen part's picture and name that
opens the picker, as in the shared command panel.

`app.ui-macos-native-library` builds a local library of a pictured screw
and nut and checks, in light and dark appearance, Insert from Library and
Community Library: two pictured tiles with readable captions and contrast,
a readable group list, group selection, folding and stars, the library
search, a tile's star and Favourites, the hover card, three tile sizes
through the shared slider, usable bounds at normal and narrow sizes, Return
on the grid and a double click inserting parts. `app.ui-macos-native-part-picker`
builds two pictured slot profiles and starts BUILD Profile: its Component tile
opens the panel with pictures and groups and no Qt window; a click chooses
the 30 profile while the document keeps the keyboard; pinning survives the
command's Escape and the next Profile; Close and Command+W return the
keyboard to the document, where Escape still ends the command. The dialog
fixtures now open through `showDialog` as well, so their native captures
are AppKit windows, not a harness's Qt window.

### Converted layouts and dialog buttons

The Appearance and Materials windows no longer assign (mitcad#1059): the
Material and Appearance tools do, in the command form (mitcad#1066). Their
fx (`InputDef::fxToggle` with `besidePrevious`) is a compact push button
with the `fx` artwork at the end of the tile's line, pressed while the
value varies with an option, as the Qt panel's; the rows are captioned by
their value (`labelFor`) and their drop-downs keep each option's swatch
(`choiceImage` at `choiceImageSize`) and the headings, bold and disabled.
The picker button opens the tools' picker as an AppKit popover under the
tile (`showNativePicker`, `platform/macos/NativePicker.mm`): the shared
`LibraryPicker` stays the presenter, shown off the screen, and the popover
binds its search field and list. Typing goes to the presenter's search,
which filters; Up and Down move its current entry over the shown ones;
Return or a click on an entry chooses it; Escape or a click outside closes
it. The entries keep their swatches and headings, so the choosing, the
filtering and the log lines are the Qt picker's.
Send to Slicer's Printer and Process choices use the same popover
(mitcad#1122): the short label on the closed button, the full name as its
tooltip, vendor or printer headings, source tags at the right, the current
choice checked, New printer… or New process… below a separator, and
Edit printers and processes… last, which opens Printers and Processes on
the current choice; no button sits beside the field (mitcad#1134). The
shared command session receives the choice, preserving its undo step.
`app.ui-macos-native-print-picker` checks search, Return, Down, Escape,
pointer selection, new-profile creation, Edit printers and processes…
with no button beside the rows, and undo on synthetic profiles.
The Materials window's
appearances are one list with Add…, Remove and Make Default beside it
(mitcad#1083), all ordinary Qt controls: the bridge presents the list as
a native table with each row's swatch and name and the default's
**Default**, its row context menu, and the buttons as AppKit buttons
(rows `material.appearance.<id>`). Add… opens the same AppKit popover over
the shared `AppearancePopover` (mitcad#1068), under Add….
Configurations' fourth Bindings tab (mitcad#864) uses the
ordinary popup, model-table and button bindings. Small QLabel swatches and warning icons keep
their device-independent image height, including after an image changes.
`app.ui-macos-bindings-parity` and `app.ui-macos-native-bindings-parity`
exercise the tools: the Appearance tool's fx with Varies With and a row per
value, nothing written until OK, the Material tool's picker opened and
closed and its fx table, the binding shown again and the fx off keeping
the value shown, three bodies given one table, the Appearance tool's
picker choosing by its search, and the Bindings tab's filters, warnings,
the Material tool started on a binding, selection and remove/undo. They
retain the saved model and both content and own-window captures in each
theme. One additional light-mode run starts without options (the fx offers
None and its note), adds a numeric option with one value in
Configurations, then picks a visible OCCT face and binds that face with
the Appearance tool (one row, the option's only value); saved face keys
must match the exact face
selected by the viewport input. `app.ui-macos-appearance-edit-keys` (and
its native variant) assigns the appearance it edited with the Appearance
tool's picker, and `app.ui-macos-render-metal` with its native variant
assigns Glass - Clear through the tool and its picker.
`app.ui-macos-native-material-color-parity` checks the Materials window's
list of appearances and Add…'s popover, then Add… by the popover's search,
Make Default and Remove on the added row.

Dialogs, Settings and other secondary windows convert their shared Qt
layouts into one constraint-based AppKit container
(`app/platform/macos/NativeLayout.mm`); pages shown in the document's cards
(inspector, sketch palette) keep their stacks within the cards' own width
rules. Grids, forms and box layouts keep
the presenter's sizing contract: explicit minimum and maximum sizes; the
minimum and preferred widths of text fields, spin boxes, sliders, combo
boxes, labels and item views; size policies, stretch factors, spans,
alignment, spacing and margins. Extra space goes to stretched rows or
columns in their ratio, else to expanding ones, else to those that can
grow, as in Qt. A hidden widget leaves neither space nor spacing. Forms
keep the platform's field growth, label alignment and form alignment, so
labels share one column and fields start on one edge. Heights keep AppKit's
control metrics. The root fills its window, so stretched and expanding rows
take extra height. Preferred sizes stay below the window's own size
priority; minimum sizes hold against resizing, so a window cannot become
narrower than its fields' usable widths. Qt size hints of widgets are read
when a presenter is converted; a nested layout's natural size comes from its
native items, and a line edit keeps at least Qt's 17-character width. A
placeholder that fits its presenter's hint also stays whole: the line
edit's preferred width covers the AppKit cell's size for it, in the field's
actual font, bezel and padding. A longer placeholder keeps the shared width
instead of widening the form, as its Qt field does not show it whole either.
Non-wrapping form captions keep their complete single-line AppKit text
metrics, including trailing punctuation. Embedded table controls use cell
padding rather than the margins of an independent dialog.
Filtered categories retain their controls, but suspend the native layout
constraints of the entire hidden subtree in one batch. Hidden rows and
cells leave no guides in the active graph. Showing the category restores
its constraints even when its children's individual hidden flags did not
change; category switching, Search and Advanced keep every field available.

Settings chooses its sections in a native row of buttons above a separator,
not in text-only tabs (mitcad#971): each button has the shared section icon
above its complete name, in the shared header's order. The icons are
template images of the shared ones at 24 points with Retina pixels, so they
follow the appearance; the chosen section has the accent tint, as in the
shared header. A click selects the page through the same tab view and pane
switching as before, and the window is at least as wide as the complete
row. Other tabbed dialogs, without such icons, keep their tabs.

Converted button boxes use the macOS role order: help, reset, apply and
other helper actions on the left; destructive actions set apart; then
alternate and rejecting actions, with the primary accepting action
rightmost. Added, removed or hidden buttons and changed roles rearrange the
row. Return belongs to the presenter's default button and does nothing while
that button is unavailable; Escape belongs to the rejecting buttons, and the
window's cancel action rejects a dialog without one.

The Parameters dialog's Tables page keeps its column-name fields and unit
choices in the native table header. A presenter supplies those widgets
through `NativeColumnEditorsProperty` on its shared header; the adapter
converts their controls and follows column resizing, reordering and
horizontal scrolling. Renaming a column and choosing a unit use the shared
model commands, including number conversion and undo. Refused table cells
retain their typed text and show the native error colour and reason.
Return in a column-name or comment field commits that field and returns
focus to the table; it does not activate the dialog's OK. The paired
`ui-macos-table-parity-test.sh` workflow exercises the column editors,
unit conversion, refused and corrected numeric cells, the dependent
extrusion and one-step undo in both frontends and both appearances.

`app.ui-macos-native-dialog-layout` first checks these rules with synthetic
presenters: field widths, label and field columns, side-by-side slider
forms, a spanning row over stretched grid columns, hiding and showing,
unambiguous layout, and the role order with its gaps, Return, Escape, a
helper action and later role changes. Then, in light and dark appearance, it
opens Parameters, New Project, Settings > Navigation, Insert from Library,
Render Image and Library Components at the normal and a narrow document size and
at each dialog's narrowest size. In Settings every section button must show
its template icon and complete name within its bounds, and real clicks
switch to Display and back to Navigation. It asserts the visible, unclipped
and unobstructed bounds of the search field, the author's name and email
and the speed sliders beside their labels; left-to-right button bounds with the
primary action rightmost; real AppKit pointer clicks and slider drags; Tab
from name to email at both sizes; repeated pointer/search edits after the
narrow resize; all four narrow slider bounds; Return while Create is
unavailable; and Escape without creating, inserting or rendering anything.
Normal Zoom and narrow Trackpad sliders additionally require a real
Tab/Shift+Tab focus round trip, Home and Right/Left changes using the shared
single step, and restoration through their Reset buttons. The read-only
`expect-native-text` slider probe reads the actual AppKit integer value,
beside the independent shared-widget assertion. A real slider click takes
keyboard focus before standard AppKit mouse tracking; focus stays on it
during its own keyboard action. Other accepting actions still commit the
previous text field before dispatch. These acceptance interactions must be
run in both appearances before recording a pass.

The #187 keyboard changes were initially integrated without executing builds
or tests at the owner's request. On 2026-10-23, the physical M1 Max native
workflow passed whole AppKit Undo/Redo events, cancellation preserving the
redo history, search dismissal and the existing command/form scenarios.
The whole-event test bridge now waits for actual application activation and
its own key window before posting events; simulated delegate notifications
alone do not establish AppKit keyboard focus (mitcad#559). This scoped run
does not complete the full keyboard matrix. Outstanding manual scenarios
remain in [the macOS checklist](macos-trackpad-checklist.md).

### Tables and cell editing

Native dialog tables present the shared item views' semantic roles rather
than generic text rows (mitcad#814). A cell whose edit the model refused
(`NativeRefusedRole`, which the Qt delegate also paints) has the system red
text on a light red fill; `Qt::FontRole` keeps bold section rows such as
Parameters' groups; `Qt::ForegroundRole` and `Qt::BackgroundRole` apply,
with the theme's error, warning, disabled and placeholder colours mapped to
AppKit's dynamic colours. An emphasized selection shows the selection's own
text, as the Qt delegate leaves selected cells to the highlighted-text
colour. Labels styled with the theme's error or warning colour, such as a
refusal's reason below the table, carry that tone as the
`semanticTextTone` property and show it in AppKit. Cell text stays on one
line, elided as the view's text elide mode says; it never wraps into a
fixed-height row.

Tree and table columns follow the shared header: section sizes, hidden
sections, and stretching (`QHeaderView::Stretch` or a stretched last
section). Stretching columns take the width the others leave; below their
minimum the table scrolls sideways, as in the shared view. A tree's first
column keeps its shared width, which includes the tree's indentation, rather
than widening for deeper rows. Rows are flat, without AppKit's inset table
padding.

A shared tree's `setFirstColumnSpanned` rows use AppKit's full-width group
cell, rather than truncating the heading to the first column. Their shared
font, fill, alignment and text padding remain in effect; group rows scroll
normally and retain the item's enabled/selectable flags. This also keeps
selectable spanned roots distinct from nonselectable category headings.
The native print-history regression checks the actual visible full captions,
span width, padding, font and selection policy of both kinds without
scrolling individual rows into view. Padding is measured between the actual
AppKit alignment rectangles used by the anchors, while caption completeness
and clipping are checked against the visible view bounds.

A native table follows its shared view's own scroll requests: when the
shared view's vertical position moves, as after a presenter's `scrollTo`
(Settings > Navigation reveals its first active binding below the reserved
left drags, mitcad#972), and its new top row is not wholly in the native
viewport, that row becomes the native top row, below the header. Only a
move of the shared position counts, so the user's own native scrolling
stays; a shared move that only keeps its current row in sight, as after
native input chose a row the native table already shows, leaves the native
position too. All rows, reserved and disabled ones included, remain in the
table. The native dialog layout regression reads the first whole row in the
actual viewport (`expect-native-first-row`) when Navigation opens, after
scheme changes and at the narrow size.

When a shared `SelectItems` view restores a selected cell after rebuilding
its model, the native row containing that cell regains its highlight. This
native presentation update does not expand the Qt selection to the other
cells of the row. `SelectRows` keeps its existing whole-row policy. The
paired Tables regression checks the actual native row selection and the
shared single selected current cell after header, refusal and value edits.

In a cell editor, Return commits the value, refused or not, without
pressing the dialog's default button; Escape discards the edit without
closing the dialog; Tab and Shift+Tab commit and edit the next or previous
editable cell of the row. In a dialog the table keeps the keyboard focus
after an edit and after a row is selected, so arrow keys work and F2 edits
the current cell; a cell without a native editor passes F2 to the shared
presenter. An open edit keeps its cell: reloads, selection updates and
rebuilds of the native view wait until it ends, and when the model rebuilds
its rows (Parameters does after each change) the edited row is found again
by its stable identifier. Only a row that disappeared closes its editor
without committing. Ending an edit that changed nothing sets nothing.
Document cards keep leaving keys to the document window.

Property Sets' print cells mount the shared profile chooser and distinct
Edit/reset controls as native AppKit controls. The delegate's glyph painting
and mouse hit tests are not used by a native table: these controls call its
same actions directly. Printer/Process pencils open the selected profile,
Own settings opens that level's settings, and Material opens Materials;
reset removes only that level's profile choice in one undo step. Read-only
summary values remain labels. `app.ui-macos-native-print-history` checks
these controls with real pointer input in both appearances, alongside the
long timeline's wheel behavior, undo and unchanged model/viewport checks.

The bridge does not run item delegates, so what a delegate paints reaches
the native cells as data (mitcad#1078, mitcad#1080). A table that sets
`NativeRowHeaderProperty` (Purge Volumes) shows its vertical header as a
leading column of row labels with their icons; other tables leave their
row numbers out. Column headers show their icons before their titles;
a model column added after the native table was built, such as Purge
Volumes' slots once it opens, adds its native column. Purge Volumes' cells
are data without a painting delegate: the display role is the volume with
its unit ("140 mm³"), and a typed text, from either frontend, commits
through the dialog's one parse, so a text that is no volume is refused in
both. A read-only print colour in Property Sets shows its swatch beside its
name. Views name the columns whose delegates draw more than text:
`NativeDecimalColumnsProperty` puts Parameters' Expression and Value
numbers on one decimal axis per column in tabular digits, the text itself
unchanged, and `NativeSelectedIconTintProperty` gives a selected row's
icon the selection's text colour (Keyboard Shortcuts' commands, the
Projects status marks). `app.ui-macos-native-print-history` checks the
swatch, Purge Volumes' row labels and their swatches, the cells' unit, a
typed 120 committed and "abc" refused; the decimal axes and the tinted
marks are visual checks in its and the Tables test's captures.

`app.ui-macos-native-tables` exercises these in light and dark appearance
with the `--demo` block: an invalid `30 deg` length committed with Return
and `45 deg` with Tab both keep 20 mm and the 48,000 mm³ body, keep
Parameters open and show the refused cell and red reason; Escape, F2 and a
correction to 25 mm follow. It checks section rows, read-only cells and the
column sizes at the shared dialog size and a narrow size, and a local
synthetic library's long name and path in Libraries. Its probes read the
actual text colours, cell fill pixels, fonts and column geometry.

### Card widths and windows within the screen

Projects, Browser and the inspector are widened or narrowed by dragging
their inner edge: the right edge of Projects and Browser, the left edge of
the inspector, which show the resize cursor (mitcad#818). A card is at
least 230 points (Projects, for its toolbar's fixed buttons), 180 points
(Browser) or 300 points (the inspector, the width its forms are laid out
for), and at most two thirds of the canvas less its margins, as the shared
Floating card. With cards on both sides, at least 160 points of canvas stay
free between them: the inspector narrows first, then the left cards. A
compact window narrows the cards without changing the widths remembered
for a larger one. Releasing the edge saves the width: Projects in the
shared `projects/cardWidth`, so that the Floating Qt card opens as wide,
Browser in `browser/cardWidth` and the inspector in `inspector/cardWidth`.
The Qt Browser and command cards keep their fixed widths. The handles are
accessibility splitters named Projects width, Browser width and Command
panel width. Each layout gives the name column of the Projects and Browser
outlines the width the card leaves after the other columns (Projects'
compact marks keep theirs), also for an outline made inside a card that
opens at a remembered width.

Secondary windows scroll their whole body above the dialog's buttons,
which stay visible at the bottom: the presenter's trailing button box, or
the trailing row that holds it beside a stretch or other controls. A window
opens at the size its presenter requests, or taller when its content needs
it, always within the visible frame of its screen (without the menu bar and
the Dock), title bar included; a sheet fits below its host's title bar and
toolbar. Resizing keeps it at least as tall as its converted layout's
minimum with its buttons: each control at its own minimum height, a table
at its least rows and a tab view at its page's minimum (mitcad#812). Only
where the visible frame is lower do required Auto Layout sizes of the
controls enlarge the scrolled body instead of the window; a body wider than
the window scrolls horizontally. Display changes and moves to another
screen make a window smaller and move it inside the visible frame again,
never larger. The shared Qt Appearances dialog has the same arrangement:
its body is a `FittingScrollArea` within the screen's available area. That
scroll area is marked `nativeWindowBody`, so the AppKit conversion lays it
out inline in the window's own scrolling body rather than as a nested
scroll view.

The Configuration card (below) hosts the shared Configuration bar when the
design has options or rows. Its row presets, list choices, value fields,
range sliders, driven values and refusal notices use the same presenter as
Qt. A range slider commits on release, so a drag changes the model in one
undo step; ordinary Navigation sliders retain their continuous updates.

### The Configuration card and the left column (mitcad#1020)

The shared Qt frontend moved the Configuration bar out of the Browser into
a left panel of its own (`browser/MainWindowConfigurationPanel.cpp`):
Projects, Configuration and Browser from the top, the Configuration panel
shown while the bar is present (`ConfigurationBar::isPresent`,
`presenceChanged`), collapsible with its collapsed state in
`configuration/panelCollapsed`, its row combo captioned Row under the
panel's own Configuration header (`ConfigurationBar::setHeaderTitled`).
The left panels share the column's whole free height, from below the
toolbar to above the history bar, without fixed caps: a panel whose
content is shorter than its share gives the rest to the others, and they
scroll only when all of them are longer; the boundary between two open
panels is dragged (a resize cursor; at least the header and a row each),
the heights kept per layout, and a double-click on it returns to the
automatic split; a collapsed panel gives its height to the others. The
Floating layout implements this in `FloatingLayout` (`shareColumn`,
`floating/leftSplit`). The Projects panel starts hidden until the user
shows it (`projectsPanelShownAtStart`, shared with this frontend).

The AppKit frontend does the same (`NativeMacWindow.mm`,
`NativeColumnSplit.mm`, `MainWindowNative.cpp`):

- The Configuration card (`native.configurationCard`) sits between
  Projects and Browser, with its Configuration header and a disclosure
  (`native.configuration.disclosure`) that folds the content under the
  header, remembered in the shared `configuration/panelCollapsed`; it is
  hidden while the design has no options and rows
  (`setConfigurationView(nullptr)`). The bar's scroll view keeps its
  identifier `native.configuration`; the row combo is captioned Row
  (`ConfigurationBar::setHeaderTitled`). The card is as wide as Browser,
  and at least 230 points (Projects' least), the width its one-row options
  need, within the free canvas.
- The form has one row per option, its caption, field and slider side by
  side as in the Qt bar, the row combo beside its caption: the bar no
  longer opts in to the grid's wrapping (`nativeResponsiveLayout`), which
  put each option on three lines (about 80 points an option).
- The column: no fixed caps (the former `(floor - top) * 0.4` for
  Projects and `canvasHeight * 0.55` for Browser, which the owner lifted
  for mitcad#1020). Projects, Configuration and Browser share the free
  height by the Floating layout's rule (`shareColumn`,
  `framework/ColumnShare.hpp`); Projects shows whole rows and gives what
  that leaves to the next card. The gap between two open neighbours is an
  accessibility splitter (`native.columnSplit.<above>.<below>`, named like
  "Configuration and Browser height") with the row resize cursor: a drag
  moves height between the two, each keeping its header and a row and
  neither growing past its content's height (mitcad#1152: a weight past it
  moved the other boundaries instead); the heights then weigh the shares,
  kept in `native/leftSplit`
  ("configuration=80", ...; logged `Native split remembered:` and
  `Native split restored:`); a double-click returns to the automatic split
  (`Native split: automatic`). A collapsed card keeps its header and gives
  its height to the others.

`app.ui-macos-native-left-column` checks on a synthetic design with three
range options and 40 bodies: the card with the Row row and the three
options under 190 points, its controls whole and in view
(`expect-native-configuration-layout`), Browser reaching the bottom of the
free column (`expect-native-card-height native.sidebar bottom`), the
boundary dragged with AppKit mouse events (`native-column-split`) and its
split after a relaunch, the double-click, and the disclosure remembered
across a relaunch, in both appearances; with the Projects card shown, its
boundary dragged past its rows moves no card (mitcad#1152).
`app.ui-macos-native-configuration-parity`
keeps its configuration checks with the card moved.

`app.ui-macos-native-card-widths` drags each edge with AppKit mouse events,
checking pointer routing and the resize cursor, with a project and a body
whose synthetic names are elided at the default widths and shown whole
after widening. It checks the minimums, the free canvas, a compact
900 × 640 window and the widths after a relaunch in the native and the Qt
frontend. Appearances is checked within a 900 × 520 visible frame applied
as a test display change, with its buttons visible and its whole body
reachable, in light and dark appearance. `app.ui-macos-dialogs` checks
the Qt Appearances dialog at its opening size and compact. Physical
trackpad drags and real display changes remain manual checks.

### Projects keys, names and moves

The native Projects outline keeps the shared panel's tree contract
(mitcad#816). With the outline focused, Return opens the selected design
(a folder or project folds), F2 renames and Delete or Backspace deletes,
before default buttons and document shortcuts, exactly as the default
tree's keys do. Deletion keeps the shared confirmation, refusals and error
messages. Selecting a row or opening a design keeps the outline's keyboard
focus; a context menu no longer rebuilds the native controls.

New Design, New Folder and Rename name the row in the visible outline: its
own text field becomes editable on an opaque text background with the
whole name selected, scrolling a long name on one line. Return, Enter and
Tab take the name, Escape keeps the old one, and clicking elsewhere takes
it, as the default tree's editor does; focus then returns to the outline.
The shared panel validates the name and runs the same project commands. A
collapsed Projects card discloses its rows before naming.

A design or folder moves by dragging it onto a folder, the project or a
design (its folder) of the same project. The row under the pointer is the
target; the panel's own rules decide which targets accept the drop.
Refused drops get the shared feedback, including the message for a drop
on another project. Nothing leaves the outline. Selection after an edit,
move or deletion follows the shared panel, which selects no row whose name
changed. `app.ui-macos-native-projects-edit` exercises keyboard, pointer
and context-menu paths with nested synthetic projects and long names, in
both appearances and after reopening the application. Its row drag uses
the outline's own drag source and drop target; pointer feel in a real
drag session remains a manual check.

### Compact selection and command cards

The inspector card measures its content at the card's actual width, after
AppKit has wrapped its text, and adds a command form's action row; the
isolated `fittingSize` of the content alone was too low and made the
selection card scroll (mitcad#815). A later change of the content's
height, such as the selection card's text and Create row filled in after
the card is shown, lays the cards out again. The selection card's Create row, like
every converted tool button whose text is under its icon, is a row of
tiles: the shared artwork at the button's own icon size over its whole
name on up to two lines, the entire tile one pointer target with hover,
pressed and checked highlights.

A command form follows the shared panel's form: the command's CAD artwork
and name, then Edit Parameters, in its title row; captions beside their
controls in an `NSGridView` whose caption column wraps a caption longer
than 110 points; headings, lists, cut scopes and notes without a label
across both columns. Hidden inputs hide their grid rows, so no row's
layout depends on views that AppKit detaches. Selections show their
colour swatch and a compact clear control, Flip the shared flip artwork
as a toggle, a choice's `onButton` its small icon when it has one, note
rows their icon buttons, text buttons and links, a value field's mark
(`markFor`, mitcad#980: a Hole field that no longer follows the screw
size) an icon button beside the field with the shared mark's icon,
tooltip and action (Reset to size), and `labelFor`, `enabled` and
`autoText` follow the state. OK and Cancel (Send to Slicer's Send)
are a footer that the window keeps under the inputs, which scroll above
it, as the shared panel's page does.

Each value field has the shared `fx` button beside it. Its menu lists the
design's parameters by the shared suggestion rules, with those the field
cannot take disabled and the reason in their tooltip, then Name This
Value, Create User Parameter…, Edit Formula… and the functions. A chosen
name goes into the field as typed text. Name This Value, Create User
Parameter… and Edit Formula… run the shared `ParameterField` workflow and
its dialogs on a hidden Qt proxy of the field, in a parentless window that
is never shown and carries the main window's parameter source; what they
write goes into the AppKit field. A dialog chosen in the menu opens on the
event loop's next turn after the menu has closed, never inside its
tracking. F5 or Option-Esc completes a parameter name.

Multiline AppKit editors publish text and the UTF-16 selection together,
so shared formula assistance and function/template insertion use the
visible caret. The Formula Editor opts into `NativeTextEditorCommands`:
its existing key handler owns completion, Escape and Command+Return,
while AppKit retains ordinary text editing, composition, clipboard and
undo. A native popover presents the same completion items and current row;
its selected row follows the shared current row after every filter change
and Up/Down move, and is cleared with it (mitcad#984). Clicks call the
shared completion action. Other multiline editors do not
opt into these formula commands. When a presenter's widget tree changes
and the native dialog is rebuilt, a focused multiline editor gets the
keyboard back in its new view with the presenter's text and selection,
as a focused single-line field keeps its edit (mitcad#984). A scroll
area's own children (an item view's header, viewport and scroll bar
containers) count as a set in that widget tree: Qt reordering them as it
lays out the function list or the parameter table rebuilds nothing, while
a widget added or removed, or children reordered in any other container,
still does (mitcad#986). With `MITCAD_TEST_INPUT` set, each such rebuild
logs `Native presenter structure rebuild:` and its window's title.

`app.ui-macos-native-forms` opens a synthetic box in light and dark
appearance. At 1280 × 820 and 900 × 640 points it checks that a face's
selection card shows Extrude, Create Sketch and Hole whole, inside the
card, without scrolling and as hit-test targets, then starts Hole,
Extrude and Create Sketch through those tiles. For Extrude it checks
captions beside their controls in aligned columns, compact clear and flip
controls, the CAD artwork, and OK, Cancel, Local bodies, Flip, both `fx`
buttons and Edit Parameters in view, then creates, inserts, names and
edits a parameter through the `fx` menu and the shared dialogs. For 3D
Print it checks Send and Cancel whole under the scrolling inputs.

### The Chat card (mitcad#1075)

View › Chat (`view.chat`, last in the View menu as in Qt), ⌘⇧L and the
toolbar's Chat button beside the agent indicator (`native.chatButton`, the
command's CAD artwork, on while the card shows) show and hide the Chat
card `native.chatCard`, as the Floating `chatCard`: on the right edge from
below the toolbar's rows to above the timeline, 340 points wide in a wide
window (a third of the canvas in a narrow one, at least 280 points). The
inspector, its width handle and the orientation cube's arrows move left of
it; the left cards' widths and the free canvas between the side cards are
measured in the canvas left of it. The card's title is Chat; its content is
the shared `agent::ChatPanel` on the same `ChatController`, converted by
the presenter bridge in the Qt panel's order: the agent or the agent
choice, New Conversation and the panel's menu, the model, the
conversation with its tool-call group rows, approval cards and notices,
the state line, the privacy notice, the setup's action, and the message
field with Selection, View and Send (Stop while the agent works). The
controls keep the Qt panel's object names (`chatInput`, `chatSend`,
`chatAllowOnce`, ...) as identifiers. The conversation takes the height the
other rows leave.

The panel's Qt widget stays hidden: `ChatPanel::setPresented` stands for
its show and hide (the agent looked for when it first shows, a missing one
looked for again while it shows), and `ChatPanel::contentChanged` refreshes
the bridge's view on the next turn when rows, streamed answers or the
state change; a new row rebuilds the view and the message field keeps the
keyboard. The message field is a `NativeTextEditorCommands` editor: Return
sends, Shift+Return starts a new line and Escape stops a working agent in
the AppKit text view too. The card's host (`NativeChatCard.mm`) keeps the
conversation's end in sight while it showed its end, as the Qt panel's
`keepAtEnd`, also across the bridge's rebuilds; scrolled back, it stays
until the next rebuild. An agent's Markdown answer is shown as its plain
text; its links are not yet clickable in the card. Showing the card gives
the message field the keyboard when the agent is ready; hiding it while
the field has the keyboard returns it to the canvas.

`app.ui-macos-native-chat` (`tools/ui-macos-native-chat-test.sh`) installs
the chat's scripted test agent as a signed-in Claude Code, as the Qt chat
test does, and checks that View › Chat shows the 340-point card, a message
typed in the native field and sent with Return passes the privacy notice
and adds two boxes as one step that one ⌘Z with the canvas focused takes
back, an approval card's Allow once saves the design, and the toolbar
button, ⌘⇧L and View › Chat hide and show the card again. Its screenshots
(light, the approval card, dark) are left in the test's output folder.

### The quick-access group (mitcad#1113)

The toolbar's workspace item shows the quick-access group after the
segmented control, as the Floating title row shows it after the tabs
(mitcad#1090, `QuickAccessGroup`): in the same item, so that a toolbar
configuration AppKit saved before shows it too. `refreshNativeActions`
hands `setQuickAccess` the commands of the setting `quickAccess/commands`
by the Qt group's rules (`QuickAccessGroup::availableIds`, the File, Edit,
View and Tools menus' order), each button's tooltip from `quickAccessTooltip`
with `MainWindow::unavailableReason` while the native command is disabled,
and the menus' commands for its menu. A button (`native.quickAccess.<id>`)
is the command's CAD artwork and state and runs it as the menus do; after
them a separator and the ⋮ (`native.quickAccessMenu`, DotsButton's three
dots) open the group's menu: while folded the commands themselves, then
File, Edit, View and Tools with their commands as checks, and Reset to
Default. A check or the reset changes the shared setting through
`QuickAccessSettings` once the menu has closed. The group folds into its ⋮
by `TitleBar::updateQuickFold`'s rule: when the document's name, from the
unfolded group's right to the project, agent and search items, 16 points
off each, would have less room than its width (at most the title's 220
points).

`app.ui-macos-native-quick-access` (`tools/ui-macos-native-quick-access-test.sh`)
opens a saved plate and checks the default buttons, Save disabled with
"no unsaved changes" in its tooltip and enabled once the origin's light
bulb changed the display state, View › Look At checked in the ⋮ menu
adding a button before Preferences, Reset to Default, and the group folded
in a narrow window and unfolded in a wide one.

### The navigation bar (mitcad#1099)

The Qt bar of mitcad#1086 has an AppKit counterpart: the capsule card
`navigationBar` at the bottom centre of the view, just above the message
band's row (mitcad#1098) as in the Floating layout, with Look At, Fit, Visual Style, Layout Grid and Snap
to Grid in the Qt bar's order and CAD artwork (`navigationBar_<command>`,
the Qt buttons' names). `MainWindow::createNativeNavigationBar` gives the
commands; their checks, enabled states and tooltips (the registry's name
and shortcut) come from the native action snapshot, and a click runs the
command as its View menu item does, so a toggle in the bar, the View menu,
a shortcut or the Sketch Palette is one action. Changes made outside the
native window (the palette's Snap to grid) refresh the snapshot. Visual
Style pops up the View menu's style commands with the current one checked,
its tooltip naming the current style. Preferences › Display › Show the
navigation bar (`view/navigationBar`) shows and hides it; an open drawing
hides it with the other cards. The bar's place, tooltips, the buttons' and
View menu items' checks and the pop-up's entries are logged (`Native
navigation bar …`). `app.ui-macos-native-navigation-bar`
(`tools/ui-macos-native-test.sh navigation-bar`) checks the five buttons
and Fit's shortcut, Snap to Grid in step with View › Grid and Snaps both
ways, Wireframe from the pop-up, Look At in a sketch on XZ after Isometric
View, and the palette's Snap to grid.

### The message band (mitcad#1098)

The window's messages show in the card `native.messageBand` right above
the timeline, as the Floating message band (mitcad#1042,
[ui-conventions.md](ui-conventions.md#the-message-band)): as wide as the
timeline and at least as high, room for two lines of text, and the other
cards (the left column, the inspector, the Chat card) end above its row.
The band draws the window's shared `MessageLog` (`NativeMessageBand.mm`),
whose `MessageCenter` keeps the times for both frontends, so the levels,
lifetimes, stub and history are the Qt band's: the level's SF Symbol, the
text with its level's word (*Failed:*, *Warning:*) in the level's color on
the window background,
wrapped to two lines and then truncated (the whole text in the tooltip),
and × (`native.messageBand.close`); a hint, progress, information or a
success for 6 s, a warning for 10 s, an error until × or the next
command. Then the dimmed one-line stub of the latest message
(`native.messageBand.stub`) stays in the band's row, centred and as wide
as its text; a click opens the session's history in a popover above it,
newest first, each row with its time, the level's symbol and the text.
A row about a feature selects it (as the Qt history's row, without its
message said again); Esc or a click elsewhere closes the popover, whose
rows lie on an opaque window background in the levels' text colors. The
band logs `Native message band: full|stub <level> <text>` and its place
(`Native message band placed: full|stub x,y wxh, row <h>, timeline x,y
wxh`); the history
logs the Qt history's lines (`Message history: <n> entries`, its rows,
`Message history: selected <uid>`, `Message history: closed`).

`app.ui-macos-native-issues` checks that a failed feature's error stays
past a hint's 6 s until × closes it into the stub, a hint fades after
6 s into its stub, the stub opens the history with the hint as its first
row and the error's row selects the failed feature, a warning shows as a
warning, and editing a feature (the next command) closes an error into
the stub. Its screenshots of the band (an error, a warning, the stub and
the history, light and dark) are left in the test's output folder.

## Qt cards on macOS

Docked Projects and Browser trees can shrink to one complete row while
their existing scrollbars reach the remaining rows. Their stacked docks
keep the toolbar and title rows visible on a short workspace. The Projects
regression includes a process-local 394-point available frame, using the
actual titlebar margins, with 0, 1 and 24 projects at Qt scale factors 1
and 2. It checks the actual frame, dock containment, a complete tree row
and unclipped toolbar; it does not change the display configuration.

Every Qt tool button owned by the main window uses the shared
`MacToolButtonStyle`, in Floating and Docked layout, including controls
created later and buttons in owned menus and dialogs. Its resting surface
is transparent; hover and press use quiet rounded fills, checked controls
use the palette's accent, and keyboard focus keeps an accent ring. Native
label/icon rendering, disabled artwork and split-menu hit regions retain
the platform style's metrics. Custom-painted ribbon, capsule and view
navigation buttons keep their own painting. Other platforms are unchanged.
The `app.ui-macos-tool-buttons` check audits all owned buttons, including
hidden ones, and checks every ordinary button's actual effective style,
including stylesheet overrides, for resting pixels on the native platform at
Qt device pixel ratios 1 and 2 in both appearances. These scale checks do
not claim a change to the physical screen's backing scale.
The exact warning/error badge roles retain their semantic colored capsule
outlines, and the command panel's choice tiles retain their content outline;
these declared roles are the audit's only surface exceptions.

Qt remains the default macOS frontend. Its Floating cards are native tool
windows when system glass is available. A tool window can be taller than
its requested rectangle because Qt honours the content layout's minimum
size. `GlassCard::slot()` therefore reports the window's accepted size,
and `FloatingLayout` follows later card resizes. Projects and Browser stack
against those bounds, including after expanding project rows, folding a
card, changing the window size or showing Projects again. Layout constraints
are activated before assigning a slot so folding also releases the old
minimum height.

The Projects toolbar and tree icons use 16 logical pixels; the tree shares
Browser's 16-pixel indentation. Qt's macOS style supplies the row selection
and disclosure rendering, and both trees inherit the platform font and
palette. Project lock marks use the themed icon set; selected status marks
use the palette's highlighted-text colour to remain visible over the accent.
The Qt Mac ribbon uses
26-pixel icons in 36-pixel buttons, with separate group captions and menu
chevrons; the title bar keeps the compact Undo/Redo and Solid/Build/Make controls.
Card titles keep their system-font metrics inside a 30-pixel header with
20-pixel collapse controls.
The Floating Projects toolbar has a transparent background, so its native
buttons sit on the same glass material as the rows in both appearances.

`app.ui-macos-projects-cards` checks both requested slots and actual native
window bounds, plus toolbar/button containment, with zero, one and 24 local
synthetic projects. It covers project expansion, window resizes, card
folding and visibility, in Floating and Docked layout at Qt scales 1 and 2.
Scale overrides exercise Qt's high-density layout and artwork; they do not
establish physical Retina-display acceptance. Screenshots of each state
support the visual pass over the toolbar, rows, card titles and ribbon.

The selection card's Create row uses the shared macOS tool-button style:
borderless icon-over-label controls with hover and pressed highlights. A
change of selection reuses those controls, including their style and
accessible command names. `app.ui-macos-selection-create` covers a body
and a planar face, starts Rectangular Pattern and Extrude through their
actual buttons, and checks that the selection reaches each command. It
runs in Floating and Docked layout, light and dark appearance, at device
pixel ratios 1 and 2, with captures for visual review. Qt scale overrides
exercise dense layout and artwork without changing the physical display.

Browser, Projects and the other panel trees share `TreeItemDelegate` for
inline renaming. A string editor paints an opaque application Base behind
its text, without a native line-edit frame, and stays within the row. The
editor explicitly resolves the platform's Text and selection colour pair
for active, inactive and disabled groups, so a transparent tree selection
and the Docked window's accent-text colour cannot replace them. The Browser
places it after its custom bulb/icon decoration. While an editor
is open, the delegate omits the underlying name; the Browser also omits
the component radio button until editing ends. Non-string cells retain
Qt's normal editor factory. `app.ui-macos-tree-rename` exercises body,
component and measurement names, double-click/F2/slow-click entry and
Projects renaming in both chromes and appearances at absolute DPR 1 and 2,
including committing a typed name and discarding a later draft with Esc.
Its guard renders the actual delegate with two text colours and requires
identical pixels while editing, and checks the editor's opaque pixels,
selection, row containment, coverage of the actual old name's ink rectangle
and horizontal text alignment. These scale overrides cover layout and
artwork; physical Retina acceptance requires a display with that backing
scale.

Finish Sketch appears in the ribbon and the Sketch Palette while a sketch
is open; the Qt title row keeps its normal search control as its final
item. `app.ui-macos-sketch-finish-layout` checks the title layout and mask,
then finishes through the ribbon button, the palette button, the collapsed
ribbon group's menu and the standard shortcut. Floating and Docked layout,
light and dark appearance, and wide and narrow windows are covered. The
AppKit toolbar's default items already omit Finish Sketch; its native
ribbon retains the Finish Sketch group when other groups collapse.

## Viewport anti-aliasing measurements

The normal shaded viewport retains four MSAA samples on every backing
scale. `MITCAD_VIEWPORT_AA=msaa4|msaa2|off|ray-aa` is an opt-in diagnostic
override, shared by the Qt and native frontends. It changes neither the
rendered view's resolution nor the user's saved settings. Invalid values
warn and retain four samples. No scale-dependent or navigation-dependent
policy is selected without physical-display measurements (mitcad#552).

`MITCAD_LOG_TIMING=1` reports the GL renderer/version, screen refresh rate,
logical and backing dimensions, backing scale, `GL_MAX_SAMPLES` and the
configured OCCT sample count. Each `Timing frame` line also includes its
current backing dimensions and scale, including after a display move.
The configured sample count is a request: the driver or OCCT may reduce it;
`GL_MAX_SAMPLES` alone does not prove the internal framebuffer's allocation.

`Timing frame` measures CPU time inside `paintGL`, including any driver
waits. `Timing frame swap` is the interval between Qt's `frameSwapped`
signals, after composition; it includes input scheduling and is not a GPU
timestamp or a measurement of physical scanout. Optional
`MITCAD_VIEWPORT_SYNC=1` finishes this context's GL work before ending the
paint timer, adding GPU completion waits while excluding Qt composition
and presentation. That diagnostic fence changes pipelining; keep fenced
and unfenced results separate and retain the normal unfenced comparison.

Run the bounded synthetic dome and pointer navigation harness serially on
the physical Mac, using the application's companion CLI:

```sh
bash tools/measure-viewport-aa-macos.sh "$APP" "$CLI" "$OUT/aa-submit"
MITCAD_VIEWPORT_SYNC=1 bash tools/measure-viewport-aa-macos.sh "$APP" "$CLI" "$OUT/aa-sync"
```

The harness starts fresh isolated settings for each mode, fixes the window
at 1280 × 820 logical pixels, fits the same synthetic dome, warms up with
30 pointer moves and measures 60 further moves on the same circular orbit.
The in-process driver sends real viewer pointer events without external
input permissions. It checks that all 90 moves ran, that the camera changed
during the orbit and that the release cleared held buttons. Results include
raw logs, frame/swap samples, median and maximum milliseconds, resting
screenshots, and OS/display metadata. Review the images for line quality
and frame sample counts before interpreting a comparison. Keep identifying
display metadata outside published reports.

The fourth `ray-aa` mode explicitly tests the issue's suggested
`IsAntialiasingEnabled` flag with MSAA disabled. In the pinned OCCT 8.0.1
source that flag controls adaptive FSAA in `OpenGl_View_Raytrace.cxx`;
it does not enable FXAA for this raster shaded viewport. Its result must
be described as a ray-tracer flag control, not evidence for raster FXAA.
Implementing a separate raster postprocess is outside this measurement.

A genuine 1× comparison requires verifying the actual display's backing
scale using its display mode and runtime log. The harness rejects Qt scale
overrides: software scaling does not establish a physical 1× display.
When only 2× hardware is available, record that limitation and leave the
1× acceptance item open.

### Physical 2× result, 2026-10-23

The synthetic fixture was the fully revolved dome shell with 4.45 and
4.55 m radii from `tools/ui-fixtures/viewport-dome.json`, with the half-dome
feature suppressed. The controlled comparison ran on a physical Apple M1 Max
(32 GPU cores), macOS 27.0.1 build 26A434, with OpenGL reporting
`4.1 Metal - 91.7`. The built-in display reported 120 Hz and a 2× backing
scale. After the harness resize, the measured viewport was 2560 × 1380
backing pixels; its initial environment line reported 2560 × 1340 before
that resize. `GL_MAX_SAMPLES` was four. Each mode completed all 30 warmup
and 60 measured pointer moves, with 60 measured paint and swap samples.

Unfenced results, in milliseconds (median / maximum):

| Configured mode | Paint | Qt frame-swap interval |
| --- | ---: | ---: |
| MSAA 4 | 1.37 / 4.44 | 19.73 / 28.39 |
| MSAA 2 | 1.25 / 4.15 | 19.55 / 30.16 |
| MSAA off | 0.80 / 3.26 | 20.24 / 32.47 |
| Ray-tracer AA flag, MSAA off | 2.12 / 3.44 | 19.69 / 32.18 |

The input driver paced these swap intervals; they do not establish
sustained 120 Hz navigation. The ray-tracer flag control also varied from
the equivalent raster MSAA-off configuration, so differences between these
single runs must not be treated as a repeatability estimate.

The separate diagnostic run with `MITCAD_VIEWPORT_SYNC=1` also completed
all 60 measured paint and swap samples per mode. GPU-completion-inclusive
paint durations and Qt swap intervals, in milliseconds (median / maximum):

| Configured mode | Paint with GL completion wait | Qt frame-swap interval |
| --- | ---: | ---: |
| MSAA 4 | 4.43 / 6.42 | 19.66 / 30.87 |
| MSAA 2 | 3.05 / 6.52 | 19.52 / 30.98 |
| MSAA off | 2.87 / 7.32 | 19.32 / 32.92 |
| Ray-tracer AA flag, MSAA off | 2.59 / 4.44 | 19.16 / 31.23 |

Retain MSAA 4. MSAA 2 reduced median completion-inclusive paint duration
by 1.38 ms (31%) in this bounded workload, but its maximum did not improve
and the input-paced swap intervals showed no established cadence gain.
MSAA 4's largest completion-inclusive paint duration was below the
display's nominal 8.33 ms period in this run; that excludes Qt composition
and is not evidence of sustained 120 Hz output or a guarantee for other
designs. These results do not justify lowering resting line quality or
adding a navigation-dependent policy.

No actual 1× display/mode was available at this checkpoint; no Qt scale
override was used as a substitute. The owner subsequently selected a real
non-HiDPI mode, allowing the remaining measurement below. Raster FXAA is
unavailable through the suggested OCCT flag, as described above. Moving
between physical displays remains a separate hardware validation gap.

### Physical 1× result, 2026-10-24

The owner switched the built-in panel to its native 3456 × 2234 non-HiDPI
mode. AppKit reported a 3456 × 2234 logical screen and
`backingScaleFactor=1.0`; CoreGraphics independently reported both logical
and pixel dimensions as 3456 × 2234 at 120 Hz. The panel's marketing
"Retina" label in system metadata does not describe its current backing
scale. No Qt scale override was used.

The physical Apple M1 Max (32 GPU cores), macOS 27.0.1 build 26A434 and
OpenGL `4.1 Metal - 91.7` match the earlier hardware. This run used the
validated arm64 Qt RelWithDebInfo build with C++/Objective-C++ ThinLTO from
`ffe13896`; the earlier 2× checkpoint used a Debug profiling build.
Consequently these are separate within-build mode comparisons: differences
between their absolute times cannot be attributed solely to backing scale.
The harness and synthetic dome fixture are byte-identical to the 2× run.

Each of the four modes completed 30 warmup and 60 measured pointer moves
in both unfenced and fenced runs. Every measured paint recorded scale 1,
a 1280 × 690 backing viewport and the expected GPU-sync setting. Each mode
provided 60 paint and 60 frame-swap samples. The initial environment line
reported 1280 × 670 before the harness resize. The renderer was Apple M1 Max
and `GL_MAX_SAMPLES` was four; configured sample counts retain the
request-versus-allocation limitation described above.

Unfenced results, in milliseconds (median / maximum):

| Configured mode | Paint | Qt frame-swap interval |
| --- | ---: | ---: |
| MSAA 4 | 1.33 / 3.92 | 17.86 / 31.52 |
| MSAA 2 | 1.33 / 3.43 | 18.19 / 29.33 |
| MSAA off | 0.90 / 2.83 | 18.24 / 29.31 |
| Ray-tracer AA flag, MSAA off | 1.28 / 2.84 | 18.46 / 31.13 |

Separate `MITCAD_VIEWPORT_SYNC=1` results, in milliseconds (median / maximum):

| Configured mode | Paint with GL completion wait | Qt frame-swap interval |
| --- | ---: | ---: |
| MSAA 4 | 2.22 / 4.19 | 18.07 / 28.86 |
| MSAA 2 | 2.55 / 4.13 | 18.21 / 30.14 |
| MSAA off | 2.23 / 4.80 | 18.08 / 30.54 |
| Ray-tracer AA flag, MSAA off | 2.02 / 3.93 | 18.38 / 31.40 |

Retain MSAA 4 at 1× as well. MSAA 2 did not lower the unfenced paint median
and its completion-inclusive median was higher in this bounded run. The
small maximum differences and input-paced swap intervals do not justify
lowering line quality. Resting MSAA 4/off captures were visually reviewed;
the four-sample image retains the smooth dome outline and grid lines.
These single runs are not a repeatability estimate, physical scanout or
input-latency measurement, and the ray-tracer control is not raster FXAA.

The real 1× acceptance gap for #552 is now covered. Combined with the
earlier physical 2× result, no adaptive sample policy is justified: normal
MSAA 4 remains unchanged. No application code, user preference or display
mode was changed by the measurement harness. Live moves between different
physical displays remain untested; no new scale-dependent behaviour needs
that transition for this decision.

## Validation

`app.ui-macos-native` starts and cancels Offset Plane through its familiar
pinned CAD ribbon button, checks Extrude stays disabled without profiles,
then creates a 40 × 25 × 20 mm box through the native menu and AppKit controls,
rejects an invalid expression, edits its height to 10 mm through native
history, verifies undo/redo, cancels a preview and exercises command search.
Repeated native inputs retain their values after adding and removing rows,
and their controls unlock after each structural action is applied.
Native outline selection drives worker measurements, canvas Return and
Browser Escape close the inspector, and Physical Properties presents a
native report dialog. Command search checks its rows' shared CAD icons
(Libraries and Sphere selected, Extrude dimmed before it can run) and
Command+W dismissal without closing the document; command search opened
from its toolbar item lies directly under it, timeline search opened from
the magnifier directly above it. The native sketch
workflow checks Browser Return activation for sketches and features, row
renaming and command isolation.
The toolbar regression upgrades an older saved layout, checks live status
after a customization-palette request, removes Project, changes projects,
and verifies that removal persists in a second application process.
The secondary workflow checks Build direction buttons and the reusable
form contract for exclusive icon choices and expanding/collapsing rows.
Model-volume assertions check the persisted result. The native test bridge
uses AppKit target/action and the field editor without external
Accessibility permission. The retained Qt workflow tests still run with
`--ui=qt`.

The native workflow also dispatches activation and deactivation through the
installed application delegate, checking Qt's state signals and deactivate
events. Dock reopen must restore the active state without another activation
notification, and secure state restoration remains provided by Qt's delegate.

The same workflow checks that the viewport keeps the full content width
below the CAD ribbon when the model and inspector cards are shown or
hidden. It checks a compact 900 × 640 point window and the normal 1280 ×
820 point window, card bounds and AppKit hit testing: cards receive pointer
input above the canvas, and their outer gaps pass input to the canvas.
Command and sketch workflows also check the visible inspector layout and
start commands through the real native ribbon buttons. The sketch workflow
checks familiar line/rectangle buttons and finishes through the ribbon.
The layout checks cover ribbon button spacing, group caption bounds,
separators and the transparent browser background. In sketch mode they
compare the actual ribbon and palette constraint bitmaps with the shared
coloured CAD asset at each image's resolution, and reject template tinting.
An idle sketch screenshot shows both icon sets before drawing starts.
Hiding and showing an active inspector is checked with an invalid
expression: its text and disabled confirmation state remain intact.
The actual native ribbon popup menus are compared recursively with the
retained Qt CAD menus for the active workspace, including command order,
submenu structure and checked states. Settings must remain in the MitCad
application menu, and search-only settings pages must stay out of CAD menus.
The secondary-window and panel tests open the Cache settings page through
native command search.
Native screenshots preserve the captured AppKit cards when compositing the
real OCCT framebuffer beneath them.

`app.ui-macos-native-history` creates three independent bodies and checks
their actual counts and volumes at each marker position. It compares the
native feature bitmaps with the shared CAD assets in light and dark
appearance, verifies the compact transparent strip and in-row marker,
and checks Shift/Cmd selection across appearance and window-size changes.
Native double-click editing, cancellation, step controls and hide/show
use the real feature controls. Separate AppKit mouse-down, drag and
mouse-up events verify that a multi-position marker drag is one undo step,
that undo/redo restores the correct bodies, and that returning to the
starting position adds no undo entry. After clearing cached geometry, the
test sends a new drag target and releases the pointer while the real model
worker is still computing, then checks the final marker and single undo.
Feature and marker hit tests also verify pointer routing above the canvas.
Native feature drag/drop reordering
and physical pointer/trackpad feel remain hardware checklist checks.

`app.ui-macos-native-gestures` dispatches in-process magnification events
through the actual document NSWindow and compares the resulting OCCT
camera height with a direct Qt gesture control. It checks pinch in/out,
single delivery, suppression over the browser and preservation of sketch
precision input during zoom. The test uses a test-only NSEvent subclass
because AppKit exposes magnification as a read-only property. It verifies
event routing and camera behaviour; physical trackpad recognition and
gesture feel still require the [hardware checklist](macos-trackpad-checklist.md).

Native menu tracking, sheet presentation and document focus loss cancel
the viewport's outstanding pointer interaction. The shared viewer also
recovers from a consumed release when it next receives motion with no
button down. Cancellation drops pending pointer deltas and selections,
preserves explicit camera commands, and never turns the interrupted drag
into a click. Context-menu signals are emitted after the release reaches
the view controller.

Shared Qt context-menu definitions are presented by an owned AppKit popup
with explicit action targets, preserving icons, enabled and checked states,
submenus and shortcuts. The selected guarded QAction is triggered once
after tracking ends. The native document's responder chain cannot dispatch
Qt's implicit Cocoa menu-loader target.

`app.ui-macos-native-context-menus` reproduces a consumed right-button
release after an actual orbit and reads the OCCT camera direction and up
vector. It checks recovery and subsequent right dragging, tracks real
canvas, cube, Browser and history menus, and tests both cancellation and
selected actions. A stale viewport press must already be cleared when a
native menu closes, before hover recovery could hide the failure. Browser
visibility and feature suppression actions are verified against owner
logs and actual model volumes. Face on, real presses on the native cube
arrows turn and roll the camera as the Qt arrows do in
`tools/ui-view-test.sh`, and the arrows go once the view is off a face.

The native cancellation test exercises stale previews, Return during a
pending preview, cancellation during commit, preservation of a prior redo
step and subsequent command startup. Commit cancellation dispatches the
actual native Cancel button at the matching document job-start signal,
with the model busy, before a progress sheet blocks the command form.
The history drag test similarly submits its final target and mouse release
at job start, checking busy before and after each input. It then verifies
the final marker, body set and single undo step after the queued jobs finish.
The secondary-window test edits parameters and previews a local feedback
report through native controls. The sketch test checks invalid expressions,
decimal commas, Tab/Shift+Tab, Return/Escape and a 20,250 mm³ extrusion.

`app.ui-macos-sketch-new-main-parity` and its native counterpart cover five
bounded current-main workflows in both appearances. Real canvas picks and
typed precision fields create two-point and centre rounded rectangles;
a four-line selection fillets all four corners with one radius. Symmetry
takes two selected circles and an actual sketch-axis pick. A first 100 mm
dimension scales a drawn 10 × 5 mm rectangle and an unrelated circle about
the origin. Query snapshots check geometry, constraints and dimensions,
exact undo/redo and the actual GUI save; the unrelated body stays unchanged.
Measure picks body vertices over a shown middle-plane sketch, then the
sketch's points and a whole body through real filter choices. Its Keep
availability must follow those picks without changing the saved model or
undo history. Scene captures settle the camera before interaction; own-window
captures retain fields and results for separate visual review. These cases
require a runtime result and image review before acceptance. Stored operation
behaviour on edit is covered separately by `app.ui-macos-edit-auto-operation`.

The panel test configures and cancels native open/save panels and nested
report export, including the real format popup. Actual file acceptance is
a manual check: Apple's separate-process Save panels reject programmatic
`ok:` calls. The test bridge uses public panel properties and Cancel,
and never simulates an accepted file. See
[Apple's Save panel guidance](https://developer.apple.com/videos/play/wwdc2019/701/).

The initial native-frontend validation on 2026-10-09 used an arm64 macOS
27.0.1 VM, SDK 27 and Qt 6.12, with deployment target 14.4:

- Debug AppKit build and release builds with the native option both OFF
  and ON passed.
- The complete registered Mac test set ran in two groups: 124 passed,
  zero failed; seven third-party corpus tests skipped because their external
  datasets were unavailable. All five native UI tests passed.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and
  the dependency/advisory checks passed.
- The release bundle was signed ad hoc; signature and bundle checks passed
  with 58 Mach-O files and 738 library references checked.
- All five native workflows also passed against the packaged release
  application, including history and search under optimized ARC.
- Windows and Linux tests were not run on this machine, per user request.

The floating-layout follow-up on the same VM passed the five native UI
workflows and both retained Qt Mac UI tests. Debug and Release C++ and
Objective-C++ builds had no compiler warnings. The final Debug workflow
also passed the active inspector hide/show validation check. All five
native workflows passed against the ad hoc signed packaged Release app;
signature and bundle checks again passed with 58 Mach-O files and 738
library references. The full-width canvas, overlay input routing, compact
forms and familiar ribbon icons were also checked in captured images.
Windows and Linux tests were not run for this follow-up, per user request.

The ribbon-appearance follow-up on the same VM passed the Debug sketch
and floating-panel workflows, then all five native workflows against the
ad hoc signed packaged Release application. Debug and Release AppKit
builds passed; signature and bundle checks passed with 58 Mach-O files and
738 library references. The native ribbon and palette constraint bitmaps
matched the shared coloured asset at Retina resolution, including
antialiased alpha edges. Captured images confirmed the even icon spacing,
centred group captions and browser content blending into its floating
card. Windows and Linux tests were not run on this machine, per user request.

The trackpad follow-up on 2026-10-10 used the same macOS 27.0.1 VM.
Before the fix, the AppKit pinch regression left the camera unchanged while
its direct Qt control zoomed correctly. After the fix, the Debug gesture
and sketch tests passed. Debug and Release builds passed, and the gesture,
main workflow and panel tests passed against the ad hoc signed packaged
Release application. Signature and bundle checks passed with 58 Mach-O
files and 738 library references. The gesture test fixes the window at
1280 × 820 points so its browser hit stays deterministic. Windows and Linux
tests were not run on this machine, per user request.

The native-menu follow-up on 2026-10-10 reproduced 80 entries in the
original model Create popup versus 21 in the retained CAD menu. The fixed
Debug sketch test passed its recursive menu comparison, including shared
Select submenus. Debug and Release builds passed. Main, secondary-window
and panel workflows passed against the packaged Release app, checking the
model menus and command-search access to the Cache settings page. Ad hoc
signature and bundle checks passed with 58 Mach-O files and 738 library
references. Windows and Linux tests were not run on this machine, per user
request.

The native-history follow-up on 2026-10-10 used the same macOS 27.0.1 VM.
Debug and Release AppKit builds passed without C++ or Objective-C++
compiler warnings. The registered Mac checks ran in groups: 126 passed,
zero failed; seven external corpus cases skipped because their datasets
were unavailable. All seven native workflows passed against the signed
packaged Release app. The history regression verified real body rollback,
one undo per drag, release while computing, exact selection and shared
icon bitmaps in both appearances. Captured images confirmed the compact
horizontal strip and marker between feature icons. The cancellation test
now dispatches at the actual job-start boundary to avoid a progress-sheet
input race. Source license headers, signature, bundle and DMG checks
passed, with 58 Mach-O files and 738 library references checked.
Windows and Linux tests were not run on this machine, per user request.

The context-menu follow-up on 2026-10-10 used the same macOS 27.0.1 VM.
Disabling hover recovery reproduced camera rotation with no button down;
disabling native tracking cancellation independently left the right button
pressed after a Browser popup closed. The final registered Mac checks ran
in groups: 127 passed, zero failed, and seven external corpus cases skipped.
All eight native workflows and the retained Qt workflow passed. Debug and
Release C++/Objective-C++ builds had no compiler warnings; the Rust
updater's existing `rust-objcopy`/`libLLVM` stripping warning remains.
Source license headers, runner syntax, ad hoc signature, bundle and DMG
checks passed, with 58 Mach-O files and 738 library references checked.
The context-menu regression also passed against the signed Release app
included in the DMG.
Windows and Linux tests were not run on this machine, per user request.

### Physical Mac checklist (mitcad#558)

Qt remains the default macOS frontend; since mitcad#1056 the AppKit one
ships beside it as an opt-in preference. The physical acceptance
matrix below records the 2026-10-23 physical run separately from historical
VM results. At the time of that run the normal Qt build had
`MITCAD_MACOS_NATIVE_UI=OFF`, and the optional AppKit build enabled it in a
separate build directory.

The available configuration for the 2026-10-23 run is an Apple M1 Max
(32 GPU cores), 32 GiB memory, macOS 27.0.1 (26A434), with the built-in
Retina display (3456 × 2234 backing pixels, 1728 × 1117 logical points).
The runtime `expect-viewport` output records actual backing scale, screen
size, reported refresh rate and OpenGL version. Both frontends reported
scale 2, 120 Hz, `OpenGL renderer: Apple M1 Max` and OpenGL
`4.1 Metal - 91.7` while the built-in display was awake.
An older virtual-display configuration is not evidence for this panel.
`NSSupportsAutomaticGraphicsSwitching` has no GPU-switching effect on this
single-GPU Apple Silicon configuration.

| Checklist item | Qt evidence | AppKit evidence | Physical result |
| --- | --- | --- | --- |
| Render and face pick after resize, window move, two minimize/restore cycles, fullscreen and return | `app.ui-macos-physical` | `app.ui-macos-native-physical` | Passed in light/dark: twelve real framebuffer/face-selection checkpoints per frontend; actual selected face identity is read back |
| Actual 2x backing, framebuffer dimensions and input coordinates | Physical test with `MITCAD_UI_REQUIRE_RETINA=1`; `app.ui-macos-dialogs` | Native physical test and `app.ui-macos-native-context-menus` | Passed without scale overrides; Qt resized from 1280x690/2560x1380 to 1000x520/2000x1040, AppKit fullscreen reached 1728x968/3456x1936 |
| Synthetic pixel-scroll pan/orbit/zoom, pinch and selection | Physical test reads real camera target, orientation, height and selection | Same plus `app.ui-macos-native-gestures` for AppKit gesture routing, alternate scheme and sketch input | Passed; actual camera and released-button state checked; synthetic input does not establish sensor latency |
| Floating browser/command/history cards, compact ribbon, pointer hit testing and menus | Own-window captures and Qt workflow | Physical layout assertions, native history and context-menu tests | Passed; inspected actual light/dark own-window captures including glass; widget-only composites omit platform glass backgrounds |
| Light/dark appearance, native shortcut names, Tab/Return/Escape, editable fields | Physical test in both appearances, dialogs and Qt workflow | Physical test in both appearances, native main/sketch/secondary/gesture tests | Passed; parameter dialogs also passed 119 driver steps per appearance; requests are confined to the test process |
| File browsing, initial folder/name/filter, cancellation and frame restoration | Qt workflow uses shared file helpers and Qt choosers | Native panels test checks real AppKit panels from document and secondary windows | Passed programmatic configuration/cancellation and saved-frame restoration; native panel acceptance remains manual |
| Save/open, recovery, import/export, version history and local remote fixture | Qt workflow with fixture output outside Git | Native main, secondary and replacement workflows | Passed, including independent Git/history checks and recovery/import assertions; secondary/replacement fixtures also ran outside Git |
| Busy previews, latest edited input, cancel during commit and edit restoration on close | Qt command-cancel test | Native cancel test | Passed real progress/form cancellation, actual body volumes and unchanged undo depth; Qt checks actual window close and Cancel at its save question |
| Hardware Metal rendered-view navigation, resizing and worker shutdown | Dedicated render-metal test | Dedicated native-render-metal test | Covered separately by the [Metal acceptance matrix](rendering.md#physical-metal-acceptance-tools) (#185), outside #558 |
| ProMotion orbit pacing | Opt-in physical-test timing sample | Same optional sample | Both reported 120 Hz and about 121 swap callbacks/s in the unlocked session; see samples below; physical presentation/input latency remains unmeasured |
| Sleep/wake, physical trackpad/Magic Mouse input, Natural scrolling and momentum | Untested | Untested | Requires manual hardware interaction; no system sleep or input preference changes performed |
| Actual 1x non-HiDPI display mode | Four-mode physical AA comparison on 24 October 2026 | Native frontend not exercised in this mode | AppKit scale 1.0 and CoreGraphics logical/pixel size both 3456x2234; Qt measured backing viewport 1280x690; see the 1x result above |
| Live display moves, backing-scale transitions and multiple-display arrangements | Untested | Untested | One built-in panel; the owner changed its actual mode before the run, rather than forcing a Qt scale or moving a live window between displays |
| Increased contrast, reduced transparency/motion and VoiceOver navigation | Untested | Untested | Global accessibility preferences are not changed by the unattended harness |
| Native chooser acceptance, overwrite prompts, native colour acceptance/cancellation, Finder dispatch and Quit variants | Untested where not exercised by the workflows | Untested where not exercised by the workflows | Native save acceptance is in a separate system process; no permission-dependent input injection |
| Oldest supported macOS runtime | Untested | Untested | macOS 14.4 runtime unavailable; SDK/deployment target 14.4 does not provide that coverage |

The Qt command path waits synchronously for its worker while a nested
event loop presents `ComputeProgress`. Its busy-command test cancels
through that real modal dialog, then closes the idle command form with
Escape and checks body volumes and undo depth. AppKit uses asynchronous
command continuations; its existing native test can cancel the command
form at the actual job-start signal. Injecting a Qt form click synchronously
inside that signal would reenter a worker-start stack before work is posted,
which is not an ordinary user-input path. The Qt test's synthetic field
replacement and pending Return additionally stress retention of the latest
requested value; they do not measure physical typing responsiveness.
For a genuine slow Qt commit, the test first cancels the edited value's
preview and then accepts the unchanged value: a cancelled preview is not
cached, so OK computes the command itself. Accepting a successful small
Box preview instead adopts its cached result and does not demonstrate
busy commit cancellation.
Each physical run retains `windowed-ui-light.png` and
`windowed-ui-dark.png` as framebuffer/widget composites, and
`windowed-screen-light.png` and `windowed-screen-dark.png` from the
existing own-window capture API. The latter captures only this process's
on-screen windows and includes the platform glass; use it for inspecting
the floating cards, ribbon and Retina appearance rather than inferring
contrast from a widget grab without its native backdrop.

Use an unlocked console session. Initially the frontmost process was
`com.apple.loginwindow`, AppKit was inactive and its key window was nil.
Native whole-key tests, sketch Tab entry and fullscreen could not establish
their targets. After the owner unlocked the console, the same activation,
focus and fullscreen assertions passed without changing product behavior
or weakening guards. The suspected findings #573 and #574 were closed as
environment preconditions, with locked/unlocked diagnostics retained.
Hardware framebuffer/picking evidence alone does not establish active
AppKit keyboard routing or panel presentation.

The final warmed Alt+pixel-scroll observation ran with no concurrent build
or test load, after unlocking the console. Each frontend retained 180
measured callback intervals after 30 warm-up input events:

| Frontend | Median interval (ms) | Mean interval (ms) | Observed swap callbacks/s |
| --- | --- | --- | --- |
| Qt | 8.11 | 8.27 | 120.97 |
| AppKit | 8.11 | 8.26 | 121.06 |

These callbacks are consistent with the screen's reported 120 Hz pacing;
they are not scanout/presentation timestamps or physical trackpad latency.
An earlier locked-console sample was excluded from the physical pacing
result. The initial synthetic right-button sampler released at a different
point from its last move, opening a context menu; the corrected sample uses
the already-validated pixel-scroll orbit path and waits for its end timer.
The failed harness logs are retained; no product navigation or graphics
policy was changed to obtain the final result.

The physical tests are registered serial CTests and return 77 when no
physical Metal device is available. `MITCAD_UI_REQUIRE_HARDWARE=1` makes
that absence fail; `MITCAD_UI_REQUIRE_GUI=1` similarly requires a live
window server and GL context. Software or paravirtual viewport renderers
are rejected after startup. Settings and model fixtures are private to
each run, and the native test restores its own saved-frame preference.
No global display, appearance or accessibility settings are changed.
The no-device guard was also exercised with a synthetic empty worker
enumeration: both frontend invocations returned 77 with hardware optional
and 1 with hardware required, before launching any GUI. The real physical
runs required GUI, hardware and Retina and had no skips.

Run only the selected checks in the appropriate configured build:

```bash
MITCAD_UI_REQUIRE_GUI=1 MITCAD_UI_REQUIRE_HARDWARE=1 \
MITCAD_UI_REQUIRE_RETINA=1 \
ctest --test-dir <qt-build> -j1 --output-on-failure -R '^app\.ui-macos-physical$'
MITCAD_UI_REQUIRE_GUI=1 MITCAD_UI_REQUIRE_HARDWARE=1 \
MITCAD_UI_REQUIRE_RETINA=1 \
ctest --test-dir <native-build> -j1 --output-on-failure -R '^app\.ui-macos-native-physical$'
```

For the checklist's explicitly authorized frame-time observation, append
`MITCAD_PHYSICAL_MEASURE_FRAMES=1` to one physical test invocation per
frontend. It performs one warmed synthetic orbit with a 1 ms driver tick
(two ticks per Alt+pixel-scroll event), logs `MITCAD_LOG_TIMING=1` frame
swaps and retains raw samples plus `orbit-summary.tsv`. This avoids a
20 ms input pacing cap;
reported display refresh, actual observed swaps, hardware presentation and
physical trackpad latency remain different quantities. No 120 Hz success
is inferred from the panel specification or from input cadence alone.
Log failures and link their follow-up tickets alongside the result cells.

### Configuration rule blocks

The Configurations window's Rules page uses a dedicated AppKit adapter for
its palette and block canvas. The shared `RuleCanvas` supplies the rule's
scene artwork, geometry, editing, validation and undo operations. A native
scroll view presents that artwork at the window's backing scale; no Qt
widget is embedded or drawn. Native hit controls retain each piece's name,
and a native text field commits the shared inline editor with Return or Tab
and cancels it with Escape. The field follows the edited piece's scene
rectangle directly, including while the retained Qt viewport resizes or
scrolls. New hit controls stay below the field without disturbing its
active AppKit field editor. The palette has native buttons, arrow/Return
selection, Home/End, case-insensitive type-ahead within the shared keyboard
input interval, and drag sources. Selection scrolls into view; Return
inserts it, while Tab keeps ordinary focus traversal. The canvas accepts
palette drops, block moves,
context menus and Delete. Both panes scroll independently.
On a new design the native Options tree and Rows sheet show the shared
views' hints (`NativeEmptyTextProperty`) as placeholder-coloured labels in
their rows' top left corner, and hide them while there are rows
(`app.ui-macos-native-configuration-parity`).
The pointer regression selects a block's exposed word when its centre is
covered by a child field. The in-process driver's optional normalized click
point retains the same AppKit hit test and coverage checks as centre clicks.

The native Component Picker also presents an open configuration table's option
choosers below its pictured catalogue. Its list choices, number fields,
allowed ranges, cascade notices and changes to the command's current part
use `OptionChooser`, shared with Qt and Insert from Library. The production
native picker regression includes a list choice that narrows a numeric
range and then commits an ordinary numeric edit through real popup input
and Tab traversal. Its Grade combo has no explicit minimum width: the
populated Qt size hints and the form's effective field growth policy define
its width. `expect-native-bounds <identifier> shared [label regex]` reads
`QWidgetItem::minimumSize()` and `sizeHint()` from that presenter and honors
forms that keep nonexpanding fields at their hint. It logs the minimum,
hint, explicit minimum, size policy and form growth policy, then applies
the usual actual AppKit visibility, clipping, hit-test and label checks.
`expect-native-bounds <identifier> placeholder [label regex]` takes the
width the text field's own AppKit cell needs for its whole placeholder as
the minimum; New Project and Project Settings check their Author email
field with it (mitcad#953).
The number editor's inner native text field is checked separately against
its adapter's 80-point minimum; a compound editor's full Qt width also
includes its stepper. The field shows the presenter's own text with its
prefix and suffix, such as an option's ` mm`, on one scrolling line, and
widens past that minimum to its own cell's size for the whole text; it
does not narrow again while values change. A finished edit takes the
presenter's reformatted text even when the value did not change.
`expect-native-bounds <identifier> text [label regex]` takes the cell's
size for the field's whole current text as the minimum; the Component Picker
regression checks Offset's unit after its Tab commit and in the reopened
picker (mitcad#975). A line edit's field is the same single scrolling
line: a longer text scrolls with the caret, as in Qt, instead of wrapping
its last words onto an unseen second line. A text that fits the
presenter's Qt hint stays whole, so the field widens to its cell's size
for it, below the required priority and without narrowing again; the card
widths regression checks a committed `Key test appearance` in the
Appearances Name field at the ordinary and the lower screen frame, light
and dark (mitcad#982). The shared probe reads sizes without resizing or
focusing either frontend.

Noneditable combo popups take keyboard focus on a real mouse press before
AppKit tracks their menu. Their shared accepting action preserves that
focus, so Tab after a Grade choice reaches Offset. The regression checks
both the popup's retained focus and the next numeric editor. Held slider
tests post actual WindowServer pointer input. Tracking/common-mode timers
deliver six drags and a release even inside AppKit's nested tracking loop;
only the test driver's step timer is paused. Their exact endpoint, native
value and presenter value checks remain in force; the helper never sets
the value or first responder.
`native-pointer-drag <identifier> knob <endpoint fraction>` starts at the
centre of the actual `NSSliderCell` knob rectangle, read without changing
the value. Navigation sliders use this gesture to reach the same exact
maximum/minimum endpoints: clicking the geometric midpoint of a track is a
different AppKit action from grabbing its current knob. Test-only logs
record the knob, tracking begin/end, action values, mismatched refresh
values and follow-up event counts.
This held-slider path requires already granted event-posting access and
never requests it automatically. Before Down and throughout the gesture it
requires the explicit test driver, this app's frontmost PID, active key
window, live visible control and points inside that owned window. The press
must hit its actual knob. Available WindowServer metadata also checks for
covering windows and verifies the target's screen bounds. No preexisting
mouse press is replaced. A balancing Up is allocated and the tracking-mode
timers/watchdog are installed before Down; failure posts that release at
the last owned point. The six-second path records received input and actual
pressed-button state. It has no local-event fallback for held sliders;
ordinary controls retain their local AppKit event path.

A checkbox or radio button tracks a press only over its box (or circle)
and its title's glyphs; the rest of a frame stretched across a form is
inert, and a press there changes nothing. `native-pointer-click <id>`
without fractions and `native-pointer-button <title regex>` therefore aim
at the centre of the visible box of such a button (a button whose role is
checkbox or radio and whose image sits beside its title), not at the centre
of its frame; push buttons, pane buttons and other controls keep the
centre. Explicit fractions that put the point outside the visible box and
title glyphs fail the step instead of reporting a press that AppKit
ignores. The visibility, covering and hit-test guards and the real event
path are unchanged, and the test log records the measured box, title
glyphs and chosen point (`Native pointer active cell:`). The secondary
regression toggles the Review Report's wide Diagnostics section both ways
by identifier and by title, checks the shared presenter's log for each
transition, and still closes the report with a pointer press on Cancel
(mitcad#987).

Actual Tab/Shift+Tab in a noneditable popup follows its shared Qt focus
chain to the next mapped native control, skipping hidden, disabled and
non-Tab-focus fields. Focus proxies and a spin box's inner line editor map
to their owning field. Visibility is relative to the shared root and the
actual native view, so a hidden Qt host does not exclude its live AppKit
control. The route retains native forward/reverse key-view links and normal
field-editor commit; dedicated adapters keep their own key handling. The
Component Picker regression requires Grade to Offset, back to Grade and then
Offset again through real keys before committing its numeric edit.
An ordinary native text field's `insertTab:` and `insertBacktab:` delegate
commands use that same route, including the numeric field editor; responder
transfer first runs normal pending-edit validation and commit. A field's
focus proxy maps to its owner and cannot become a second stop on itself.
Slider diagnostics record the delivered event, AppKit current event and
actual pressed-button state alongside the unchanged endpoint assertions.


### Print settings and slicer imports

Standalone category bars such as the print settings form use a native popup
with the shared tab labels, icons, tooltips, order, enabled/visible state and
current category. Choosing one changes the same shared category and fields
as Qt. Converted popup buttons anchor their menus at the actual AppKit
control while the presenter action runs. Nested dialogs keep their own
anchors. The native popup regression driver resumes its script when menu
tracking ends, including when the chosen action enters a modal dialog.

An unset choice or yes/no setting keeps its shared combo at index -1 and
shows the combo's placeholder, Slicer default, as Qt does (mitcad#976). The
noneditable popup draws it in the theme's placeholder colour while no item
is selected, at AppKit's own title position, and reports it as its
accessibility placeholder; its menu holds only the real choices, so no item
can commit it and the indices stay the shared ones. Its preferred width
keeps the whole placeholder beside short choices. The native slicer parity
case checks the text before a choice, after it and after its undo.

`tools/ui-macos-slicer-parity-test.sh` exercises the New printer/process
choices, profile category and field edits, file import with Details, and
installed-preset imports into the design and My Library. Its local synthetic
folders are supplied through the existing `slicer_presets` query's `folders`
argument; `MITCAD_TEST_SLICER_FOLDERS` is honored only while an explicit
`MITCAD_TEST_INPUT` script is running. It never substitutes the user's home
folder. Run the harness serially in both frontends and appearances and
inspect the resulting captures before recording visual acceptance.

Import Presets opts its `NoSelection` tree into current-row navigation.
The AppKit outline frames the keyboard's current row without selecting it,
so same-name presets retain their warning colour. Arrows and Space reach
the shared model cursor and event filter, respectively. Arrow navigation
uses enabled visible native rows; it does not depend on the hidden Qt
tree's viewport layout. Tab reaches the actual row choice,
Shift+Tab returns to the list, and a further Tab reaches Import.
The shared Qt checklist also publishes its navigation cursor explicitly:
Cocoa's item-view style otherwise suppresses cursor movement when the
selection mode is `NoSelection`. It keeps Qt's `moveCursor` visibility and
expansion rules and leaves all selection flags unchanged. The paired Qt
regression asserts that the list retains focus before and after Space,
then exercises the same Down/Down, Fine-row Space and row-choice Tab path.
Native checkbox titles use the model's existing semantic colour and font roles.
The paired slicer regression checks the native frame's completed drawing,
checkbox state and focus, plus real Copy/Override choices, mixed All same
names, cancellation, imported item selection and filament classification
as a material. Each case clears only its own synthetic library before
launch, so reruns cannot inherit earlier imports. Captures remain subject
to visual review; these checks do not establish VoiceOver acceptance.

`app.ui-macos-move-proxy-parity` and its native counterpart exercise the
same public cylinder, block-component and 8×8 curved-peg fixture as the
existing Move/Copy UI test. Real browser selection, motion choices, datum
axis and circular-edge picks precede 64-event held arrow/ring gestures in
both appearances. Rigid Move/Copy deliberately uses
`MITCAD_TEST_DRAG_WAIT_PREVIEW=0`: its exact result follows release. The
tests require bounded meaningful proxy counters, unchanged volume,
unchanged display meshes, placement reuse and at most 0.000001 mm between
the last proxy and the exact result. Observed timings are evidence only,
without a throughput assertion.

Move Face rotates a block's top about its actual front edge, using exact
held previews because its neighbouring faces change shape. The read-only
`capture-shown-body` driver step saves actual displayed volume and world
bounds into numeric variables. Existing `expect-shown-body` compares these
with the committed result and redo, and the original with undo. The saved
motion must equal the actual dragged value; the tilted block's volume is
also checked analytically. Each case retains its preview image and numeric
evidence, without claiming visual acceptance from execution alone.

The focused Materials colour-field probe samples the composed pixels of this
application's own windows through the same API as `screenshot-screen`. It maps
the native field's points through its window to screen coordinates, then uses
the capture's logical bounds and actual pixel scale. An isolated button's
`cacheDisplay` replay can differ from its visible backing layer. The probe
retains its RGB tolerance, unset checker contrast, placeholder glyph and
readability requirements, clipping checks and accessibility-label assertions;
it does not accept semantic colour properties as proof of visible pixels.
These checks do not establish VoiceOver or maintainer acceptance.


### Navigation guide artwork

Settings > Navigation presents its mouse, trackpad and direction diagrams
through a dedicated native image view. Qt and AppKit use the same pure
painting function, supplied with the current bindings, constrained-orbit
setting, font, logical width and palette. The AppKit image is cached at the
window's actual backing scale and rebuilt when those inputs, appearance or
size change; no QWidget is captured, drawn or embedded. Its image
accessibility description names the current bindings. The normal and narrow
native layout regression checks the image's actual painted pixels and
backing size, then changes the scheme and reads the updated binding label.

## Binding cards

The native browser exports the shared row's binding count as an `fx N`
button. Features with a suppression expression have a corresponding
timeline badge. Their hover and click callbacks carry the exact shared
browser key or feature identifier into the existing controller: no native
code interprets binding expressions or chooses replacement targets.

The card presents the shared `BindingsCard` labels, cases, swatches,
warnings, Edit actions and All bindings link through `NativeWidgetView`
in an AppKit popover. Its Qt source is never displayed. Hover starts after
450 ms and follows the real pointer; clicking pins the card, and Escape or
an outside click closes it. Long content scrolls within the screen.
Both editor actions and the Configurations Bindings page retain the shared
controller's behavior.

`tools/ui-macos-binding-card-parity-test.sh` prepares only synthetic local
geometry and exercises both themes in Qt and AppKit. Its AppKit hover
path uses `native-system-pointer`, which requires already granted macOS
event-posting access and checks the actual active application, native hit
and WindowServer target. It never invokes `mouseEntered` directly. The
Qt path checks pinned card data and actual widget clicks; it does not
claim physical Qt hover from the driver's synthetic mouse events.
Saved model and undo assertions preserve the complete document while
opening and cancelling editors: an appearance's or a face's Edit… starts
the Appearance tool on its target (mitcad#1059, mitcad#1066), cancelled;
Shown's opens its condition dialog. This describes the test scope, not a
completed runtime or manual accessibility acceptance.
