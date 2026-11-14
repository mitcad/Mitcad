// SPDX-License-Identifier: MIT
#pragma once

// The in-process UI test driver. Created in main() and inert unless the
// environment variable MITCAD_TEST_INPUT names a script: then it runs the
// script's steps in the application itself, one per timer tick, by
// synthesising Qt events (QKeyEvent, QMouseEvent, QWheelEvent) for the
// widgets and by triggering actions. Nothing needs the operating system's
// input: no xdotool, no Accessibility or Screen Recording permission on
// macOS, no window messages on Windows, so the same scripts run over SSH on
// every platform. (Qt::Test is not linked into the application.)
//
// Environment (besides MITCAD_TEST_INPUT=<script>):
//   MITCAD_TEST_HEADLESS=1 Qt offscreen component workflows, without a
//                          viewport (10 ms ticks, 20 ms settle by default);
//                          requires QT_QPA_PLATFORM=offscreen and --ui=qt
//   MITCAD_TEST_STEP_MS    the tick, in ms (default 50): one step, or one
//                          event of a gesture, per tick
//   MITCAD_TEST_SETTLE_MS  the pause after an input before the next step
//                          (default 250)
//   MITCAD_TEST_DRAG_WAIT_PREVIEW=1  after a drag's final move, keep its
//                          button held until a new exact preview is installed
//                          while dragging (requires MITCAD_LOG_TIMING=1);
//                          poll without blocking, fail after 6 seconds
// The driver also reads variables for the script: ${NAME} in an argument is
// the environment variable NAME (see below).
//
// What it does about the application's state: it records every log line
// (qDebug, qWarning, ...; the messages still go to the handler that was
// installed before, so stderr is as without it) and takes "Computing X
// started" and "... done in / cancelled after" to know when the model is
// busy: steps that give input wait while a job runs (as ui_wait_idle does in
// tools/ui-test-lib.sh), and steps for a window wait while a modal dialog
// has the input. A step that opens a modal dialog does not return before the
// dialog closes (its nested event loop), but the driver goes on with the next
// steps from inside that loop, which is how the script closes the dialog.
// The driver's own log lines start with "TestDriver: " and are not part of
// what `expect-log` and the others match.
//
// Exit: "TestDriver: done (<n> steps)" and exit code 0 at the end of the
// script (not with `stay`); a failed step logs "TestDriver: FAIL line <n>:
// <reason>" and exits with 1. A script that cannot be read or has a syntax
// error fails the same way before it runs anything.
//
// Test script format
// ------------------
// One step per line: a name and arguments, separated by spaces. `#` starts a
// comment (at the start of a word); blank lines are ignored. Quoting is as
// in a shell: 'single quotes' keep everything as it is (best for regular
// expressions), "double quotes" interpret only \" \\ \$, a backslash outside
// quotes escapes the next character. `${NAME}` in an argument is replaced
// when the step runs, by a variable of the script (`set`, `capture`) or else
// the environment variable NAME (an undefined one fails the step);
// `${NAME:re}` is the value escaped for a regular expression (paths);
// `\${` is a literal `${`. Regular expressions are PCRE (QRegularExpression)
// and match anywhere in a log line. Percentages are of the 3D view widget;
// coordinates that the application logs ("at x,y") are in the main window's
// logical pixels. Buttons: left (default), right, middle. Modifiers: ctrl
// (the Command key on macOS), shift, alt, meta, joined with +.
//
// Input (waits until the app computes nothing; steps for the main window
// wait while a modal dialog is open):
//   key <key>...            a key press and release for the widget with the
//                           focus of the target window (see focus-dialog),
//                           or for the open popup menu. <key> is a portable
//                           key text: r, F6, Return, Escape, Tab, Down,
//                           ctrl+z, ctrl+shift+s, alt+r, shift (Shift
//                           alone), ... A key with a
//                           shortcut (a QAction, a QShortcut, a button's
//                           mnemonic) triggers it unless the focused widget
//                           takes the key as text.
//   key-down <key>          the key pressed and held: its shortcut offer and
//                           its press for the widget with the focus, no
//                           shortcut, no release (ctrl: Ctrl held alone,
//                           which hides a command's preview, mitcad#1421)
//   key-up <key>            its release
//   key-chord <key> <key2>  <key> pressed, then at once (in the same turn of
//   key-chord <key> wheel <x%> <y%> <notches>
//                           the event loop, before any timer) <key2> with
//                           <key>'s modifier and its shortcut, as `key`
//                           gives it, or the wheel over the view with that
//                           modifier; then <key> released (mitcad#1421:
//                           Ctrl+Z, Ctrl+S and a Ctrl+scroll while a
//                           command's preview shows)
//   type <text>             the characters as key events
//   expect-no-qt-popup     no Qt popup owns input or remains logically
//                           visible, including hidden native presenters
//   expect-widget-property <object-name> <property> <value>
//                           a Qt target widget's property as text (a
//                           size as WxH)
//   expect-widget-background <object-name> <#rrggbb> <tolerance>  wait until
//                           the colour covering most of the visible main
//                           window widget's area, in a grab of its window
//                           (its parents' backgrounds under it), is within
//                           the tolerance (the sum of the channels'
//                           differences); logs "TestDriver: widget
//                           background <name> #rrggbb (n of m pixels), ..."
//                           (mitcad#1245)
//   expect-widget-theme <object-name> dark|light  wait until that colour is
//                           a dark one (lightness below 128, as
//                           isDarkPalette decides) or a light one; logs
//                           "TestDriver: widget theme <name> #rrggbb (n of
//                           m pixels), lightness l: dark|light" (mitcad#1282)
//   expect-projects-layout <count>  actual Projects/Browser bounds and card
//                           slots, toolbar containment and loaded project count
//   expect-sketch-finish-layout  no title-row Finish Sketch or reserved gap;
//                           ribbon and palette retain the enabled action
//   expect-ribbon-labels <label>...  actual workspace labels, including
//                           accessibility, and no all-caps group captions
//                           (but the acronyms listed in testRibbonLabels,
//                           CNC, which are the group's name)
//   expect-design-chrome    the design in front shows its ribbon (Docked:
//                           its toolbar, the view's top below it; Floating:
//                           its group cards), its Browser and its
//                           navigation bar, each inside the window; logs
//                           "Design chrome: docked, ribbon x,y wxh, view
//                           x,y wxh, browser x,y wxh, navigation bar x,y
//                           wxh" in the window's coordinates (mitcad#1451)
//   expect-interface-size <ratio>  the window and the 3D view draw at that
//                           device pixel ratio (Preferences' Interface
//                           size, mitcad#1495), the window lies within the
//                           screen, the ribbon's captions are complete and
//                           its buttons as large as their icons, the
//                           Browser's first row as high as its icon, and a
//                           command panel shows with its OK whole and its
//                           fields as high as their text needs; logs
//                           "Interface size layout: docked, device pixel
//                           ratio 1.5, window wxh within the screen; ribbon
//                           x,y wxh (n captions, n buttons); browser x,y
//                           wxh (rows h high); command panel x,y wxh (n
//                           fields, OK whole)"; with `any` the device
//                           pixel ratio is not checked
//   expect-text-icon-size <text%> <icon%> <family|system>  Preferences'
//                           Text size, Icon size and Font as this run
//                           applied them (mitcad#1556): the application
//                           font in that family (system: the platform's)
//                           at the platform's point size times Text size,
//                           the Browser's icons at 18 and the ribbon's
//                           command buttons at 28 (Floating: 26) pixels
//                           times Icon size, the quick-access buttons in
//                           the tabs' row at 12 to 18 pixels times it,
//                           each button as large as its icon; logs "Text
//                           and icon size layout: font F, P pt; browser
//                           icons WxH; ribbon n buttons, icons WxH; quick
//                           access n buttons"
//   collapse-card <object-name> true|false  use the card's chevron button
//   expect-setting <key> <value>     the current saved QSettings value
//   set-setting <key> <value>        saves a QSettings value (text)
//   expect-native-configuration-layout complete visible configuration
//                           controls/captions and reachable slider track/knob
//   expect-dialog-gone <title-regex> no matching Qt tool window remains visible
//   hover-logged <regex>       move to a logged control without clicking
//   hover-logged-now <regex>   the same without waiting for the model: a
//                              Timeline item while a command's preview
//                              computes (mitcad#1600)
//   timeline-leave             leave the timeline and clear its preview
//   leave-widget <objectName>  the pointer leaves that widget (its viewport
//                              too): a notice's hover ends (mitcad#1327)
//   hover-row <objectName> <row>  the pointer over a row (from 0) of a list
//                              by its object name: a move to the row's middle
//                              sent to the list's viewport itself (a choice's
//                              rows that paint the hovered option, mitcad#1794)
//   click-row <objectName> <row>  hover-row, then the left button pressed and
//                              released there: chooses the row as a click does
//   expect-timeline-hover <uids|none>  actual preview feature identifiers
//   expect-timeline-tooltip <regex>  actual visible tooltip plain text
//   capture-view-image <path>  save only the OCCT framebuffer
//   expect-view-image <path> same|changed  compare the actual framebuffer
//   capture-view-pixel <var> <regex>  the OCCT framebuffer's colour
//                           (#rrggbb) where the newest "Pick places:" logs
//                           the place the regex matches, into ${var}
//                           (mitcad#1166)
//   expect-view-pixel <regex> <#rrggbb> <tolerance>  wait until that
//                           colour there is within the tolerance (the sum
//                           of the channels' differences)
//   expect-view-smooth <most %>  wait until, in the middle of a command's
//                           see-through preview on the screen, at most that
//                           share of the framebuffer's pixels have a step in
//                           brightness (no pattern of its triangles,
//                           mitcad#1575), counting only pixels away from the
//                           steps of the frame without the previews (lines
//                           behind them); logs "TestDriver: view smooth: ..."
//   expect-view-stepped <least %>  the same, at least that share stepped
//                           (the old double-sided look caught)
//   preview-back-faces cull|draw  see-through previews of closed solids
//                           without their back faces (the fix) or
//                           double-sided as before it
//   expect-navigation-release      synthetic modifier/button/scroll release ordering
//   expect-navigation-gesture-state keys changed during a press, a scroll
//                           while a button is held and Qt's macOS scroll
//                           phases, the window deactivated during a drag
//                           (mitcad#1571-#1573; synthetic)
//   view-navigation <scheme>  this process's view navigation, not saved:
//                           middle-pan|alt-buttons|middle-pan-f4|middle-orbit|
//                           right-orbit|trackpad|trackpad-shift
//   view-orbit-center model|cursor  this process's Orbit center, not saved
//                           (mitcad#1791)
//   capture-view-orbit-center <var> fixed|last  the fixed orbit center, or
//                           the point the last orbit or turn turned about,
//                           as x,y,z (waits for one)
//   expect-view-orbit-center none|<x,y,z>  no fixed center, or one there
//   capture-view-point <var> <x,y,z>  a model point's place in the view
//                           (logical pixels) as x,y
//   capture-view-place <var> <x%> <y%>  a place of the view as x,y
//   expect-view-point <x,y,z> <x,y> same|moved <px>  wait until the model
//                           point's place is at most (more than) that far
//                           from a captured place
//   click-view-point <x,y,z> [button] [mods]  a click at a model point's
//                           place
//   drag-view-point <x,y,z> <dx> <dy> [button] [mods]  a drag from it
//   expect-navigation-runtime       saved navigation bindings and camera gestures
//   expect-assistant-setup <mode>    isolated real setup controls and config results
//   agent-menu <row> on|off         the agent indicator's menu filled as it opens,
//                           then the checkmark of the row named so (Incoming
//                           or Outgoing connections) clicked when it differs
//                           (mitcad#1299)
//   expect-formula-builder          the open expression editor's real controls
//   open-formula-field <name>        open the editor attached to a real value field
//   expect-navigation-controls <mode>  default|configure|restored|reset
//   expect-selection-info <text-regex> [maximum-card-height] [with-create-row]
//                           idle Qt selection measurements and compact card;
//                           optionally add the visible Create row's size hint
//   click <x%> <y%> [button] [mods]
//   dblclick <x%> <y%> [button] [mods]
//   drag <x1%> <y1%> <x2%> <y2%> [button] [mods]     with moves between
//   wheel <x%> <y%> <notches> [mods]                 positive: away from you
//   wheel-logged <regex> <notches> [mods]  the wheel over the last place a
//                           log line matching regex has ("Panel <command>
//                           input <id> at ..."), to the widget there, as
//                           the one under the mouse (mitcad#1254); a wheel
//                           it leaves goes on to its parents, as the
//                           user's does (a button's to the card's scroll
//                           area, mitcad#1652), as for every wheel step
//   wheel-logged-now <regex> <notches> [mods]  the same without waiting for
//                           the model: while a panel's preview computes
//                           (mitcad#1407)
//   wheel-burst <x%> <y%> <notches> <count> [mods]   <count> wheel events of
//                           <notches>, one per tick (a quick burst)
//   trackpad-scroll <x%> <y%> <dx> <dy> <mods> [momentum]
//                           pixel scroll through the viewer's wheel handler
//   click-logged <regex> [button]       the last log line matching regex that
//                           has "at <x>,<y>" ("Datum xy at ...", "Ribbon ...
//                           at ...", "Panel <command> input <id> at ...")
//   dblclick-logged <regex>
//   dialog-click-logged <regex>
//   dialog-dblclick-logged <regex>  logged main-window coordinates delivered
//                           to the visible Qt dialog chosen by focus-dialog
//   context-menu-logged <regex> <entry|cancel> a real context-menu event at
//                           the visible live Qt widget under a logged point;
//                           choose its actual Qt/AppKit menu entry
//   test-body-highlights none|default|opaque|no-boundaries
//                           diagnostic overlays for exactly 15 shown bodies;
//                           fixes navigation in this test process only
//   click-pick <regex>      like click-logged, among the "Pick <kind> <name>
//                           at x,y" lines after the last "Pick places:"
//   native-viewport-click-pick <regex> the same newest pick-place batch,
//                           clicked through AppKit's actual hit-test/event
//                           path, rejecting native chrome covering the point
//   native-viewport-click-logged <regex> a dynamic viewport overlay point
//                           in MainWindow coordinates, through the same
//                           AppKit path without a pick-place batch constraint
//   expect-pick-offset <regex> <dx> <dy> <item-regex>
//                           probe fresh viewport picks plus logical pixels
//   click-pick-offset <regex> <dx> <dy> <item-regex>   probe, then click there
//   hold-pick <regex>       a left press held still where a click picks the
//                           item (probed as click-pick-offset, profiles too)
//                           until the view logs "Pick list at" (at most 3 s),
//                           then let go there (mitcad#1703)
//   click-edge-release <index> modifier-first|button-first|key-up-moved|f4-elsewhere
//                           Cmd/Ctrl-click a fresh visible edge, releasing the
//                           modifier before or after the button, or before a
//                           one-pixel move and the button (mitcad#1571); or
//                           a plain click after F4 was pressed on the view
//                           and released elsewhere (mitcad#1573) (synthetic)
//   click-visible-face      probe fresh visible OCCT face points, click one
//                           through the viewport input path, then verify the
//                           actual shared selection contains that exact face
//   click-visible-item <kind>   the same for another kind the view picks
//                           outside commands ("profile", "edge", "vertex":
//                           SelectionItem::kindName; mitcad#1704)
//   drag-logged <from regex> <to regex> [dx]    left drag between two logged
//                           places; dx pixels right of the end
//   drag-from <regex> <dx> <dy>         left drag from a logged place by
//                           dx, dy pixels (a manipulator's handle)
//   sketch-click <x> <y> [button] [mods]   a sketch point in mm, from the
//                           logged "Sketch view" (the origin, (100, 0) and
//                           (0, 100) in window coordinates)
//   sketch-dblclick <x> <y> [button] [mods]  double-click through the same
//                           viewport pointer path at a sketch coordinate
//   capture-sketch <uid> <path>   save an idle sketch's real JSON query,
//                           including solved points, curves and constraints
//   expect-sketch-dimension-clear <text regex>   visible text clear of all
//                           shown glyph bounds, with ordinary pick targets
//   expect-sketch-dimensions-apart <text regex>   at least two dimensions'
//                           values match, and none of their drawn value
//                           boxes overlaps another (mitcad#1673)
//   sketch-drag <x1> <y1> <x2> <y2>
//   sketch-pointer press|move|release <x> <y> [mods]  separate held left-drag
//                           events; mods (shift, alt, ...) held on that event
//                           only, e.g. Shift during a drag (mitcad#1538)
//   expect-profiles <count> inspect the actual displayed profile objects
//   type-in <regex> <text>  reveal/focus the identified panel field, select
//                           all and type through its actual frontend editor
//   qt-choice <id> <label> open a Qt combo (or a wrapper's child combo),
//                           then click its displayed enabled popup row
//   expect-input-text <id> <regex> check the actual Qt or AppKit field text
//   expect-model-parameter <name> <value|absent> [user|model|reference]
//                           check a parameter in mm; kind defaults to user
//   model-command <JSON object> send one synthetic model mutation through
//                           the shared command path; ordinary UI assertions
//                           still use pointer/key input for the action tested
//   command <id>            trigger the command's action (objectName
//                           "command_<id>": "sketch.create", "file.export")
//   capture-document-window <name> remember the active document's actual window
//   expect-document-frame-tabs <count> actual visible tab bar and document stack
//   expect-document-frame-order <capture>... actual stack order after tab input
//   document-tab-click <index> [left|middle] actual QTabBar press/release
//   document-tab-drag <from> <to> real held pointer events to reorder tabs
//   document-tab-context <index> real context event; follow with popup <entry>
//   document-tab-drop <capture> <destination capture> <index> real Qt drag/drop
//                           using the source document's registered MIME handle
//   expect-document-hidden-quiet <capture> <seconds> observe no actual GL
//                           frameSwapped events for a hidden retained document
//                           (indices above are zero-based)
//   observe-document-frame-close-all observe actual accepted closes of every
//                           document and log the empty registry before natural exit
//   focus-document-window <name> activate that retained window through its host
//   focus-document-path <path> activate the already open file's actual window
//   capture-model-number <var> <query JSON> <dot.path> read a numeric result
//   expect-document-cache-sharing <total bytes> actual per-document budgets
//   system-window-menu <entry> the owned macOS Window menu's actual action
//   expect-document-windows <count> the actual process window registry's count
//   expect-document-window-gone <name> its normal close removed that document
//   expect-document-tabs <count> actual macOS NSWindow tab group membership
//   open-file <path>        open through the external-file document host
//   open-file-now <path>    use that same host while its preview owns the model
//                           (Finder or drops), including save/lock questions
//   file-dialog save|directory <initial> [filters] [selected-filter]
//                           open a shared chooser with explicit initial state
//   menu <Top>/<Item>[/...] trigger an entry of the main window's menu bar
//                           ('&' and "..." in the names are ignored)
//   popup <entry>           choose an entry of the open popup menu (a
//                           context menu), by its text; "Sub/Item" opens
//                           the submenu Sub and chooses Item in it (also
//                           for context-menu-logged's entry); Item is
//                           looked for in the submenu Sub opened, not in
//                           whichever popup Qt reports active
//   expect-popup-entries <regex>  the open popup menu's shown entries,
//                           joined with " | " ("Name (disabled)" for a
//                           disabled one, no separators), match; logs
//                           "TestDriver: popup entries: ..." (mitcad#1455)
// The Docked layout's docks, by object name (commandDock, browserDock, ...;
// mitcad#1455, TestDriverDocks.cpp):
//   dock-title-context <dock>  a real context-menu event on the dock's
//                           title row, which opens the window's menu of
//                           its panels; follow with expect-popup-entries,
//                           popup <entry> or key Escape
//   dock-close <dock>       the pointer enters the dock's title row (its
//                           buttons show), then a click on its close button
//   expect-dock <dock> shown|hidden  wait until the dock is shown (visible,
//                           not empty, inside the window) or hidden, and
//                           the View menu's entry of its name (View ›
//                           Command for the Command dock) is checked or
//                           not; fails while the View menu has no entry
//                           for it; logs "TestDriver: dock <dock> shown,
//                           View/<name> checked"
// The interface walked (mitcad#1473, TestDriverWalk.cpp; with
// MITCAD_LOG_UNTRANSLATED=1 in another language, tools/ui-language-walk-test.sh):
//   walk-commands <regex> [exclude]  each command whose id matches the
//                           regex and not the exclusion, in the order of
//                           their ids: started as its button would ("Walk:
//                           command <id>", "(disabled)" when it cannot
//                           start), then closed again: its popups, dialogs
//                           and new windows closed, its panel cancelled
//                           with Esc, after its computation
//   walk-menus              every menu of the window (the menu bar's, the
//                           ribbon's and the panels' drop-downs and their
//                           submenus) filled as when it opens
//   walk-context-menus      the context menu of each row of the window's
//                           visible lists and trees, opened and closed
//   button <regex> [objectName] click a visible button whose text (without
//                           '&') matches; optional Qt objectName selects an
//                           exact control in the target window
//   native-action <id>      dispatch the native AppKit menu command
//   native-click <id>       click a native control by accessibilityIdentifier
//   native-focus <id>       focus a native control without invoking/selecting it;
//                           native.viewportHost: the embedded CAD view in it
//   native-hover <id> enter|leave send an AppKit tracking event through the
//                           control's registered hover owner
//   native-type <id> <text> replace an AppKit field editor's text
//   native-wheel <id> <notches> [mods] the mouse wheel over a native
//                           control's centre, to the AppKit view there
//                           (mitcad#1350: a value field with the keyboard
//                           steps); positive: away from you
//   native-choice <id> <title> choose a native popup item or workspace segment
//   expect-native-spanned-heading <table id> <caption regex>
//                           verify a visible full-row caption and selection
//                           policy against the shared tree, without scrolling
//   expect-native-first-row <table id> <text regex>  the first whole row in
//                           the native table's viewport, without scrolling
//   native-pointer-click <id> [<x fraction> <y fraction>]
//                           press a real, uncovered point within a control;
//                           the default point is its centre, on a checkbox
//                           or radio button the centre of its box (AppKit
//                           ignores a press on the rest of a stretched
//                           frame), and fractions outside the box and title
//                           glyphs of one fail the step
//   native-pointer-button <regex> the same at the default point of the
//                           enabled native button whose title matches
//   native-pointer-dblclick <id> two guarded native pointer clicks; the
//                           second carries AppKit's double-click count
//   native-pointer-popup <id> <choice> [capture path]
//                           open a real menu with guarded pointer events,
//                           optionally capture it while tracking, then choose
//                           its enabled action (or cancel)
//   native-pointer-choice <id> <expected-title> Up|Down... Return
//                           open the real popup by pointer, navigate its
//                           menu with key events, then verify the result
//   expect-native-color-field <id> <color|none> <words|-> placeholder|explicit [AX label]
//                           inspect actual swatch pixels and drawn placeholder
//                           text and optional accessibility label in the field
//   expect-native-duplicate-choices
//                           verify distinct values with duplicate native labels
//                           in command forms and secondary-dialog bindings
//   expect-native-widget-refresh
//                           verify production widget refresh, caption/icon
//                           modes and no property writes for unchanged values
//   expect-native-form-controls
//                           verify native icon artwork, values and disclosure;
//                           mount a synthetic disclosure command for layout checks
//   expect-native-disclosure-layout
//                           verify the mounted fixture's card height cap and
//                           scrolling to its last expanded input
//   native-toolbar-item <id> insert|remove|palette change the actual saved
//                           AppKit configuration or request a palette sample
//   expect-native-toolbar-item <id> true|false check actual toolbar membership
//   native-activate <id>    edit a native history icon or activate a model row
//   native-select <id> [shift|ctrl] select a native history icon/model row;
//                           modifiers exercise actual feature mouse input
//   native-expand <id>      expand a native outline hierarchy
//   native-click-now <id>   native click without waiting for model idleness
//   native-click-on-job <job> <id> arm one native button action on the next
//                           matching actual jobStarted while modelBusy is
//                           true, before progress-sheet/animation timing;
//                           fails if the armed job never starts
//   fail-next-recovery-command restore_command|discard_command
//                           fail one recovery job before it mutates the model;
//                           requires an idle native command session
//   expect-picking-on-job <job> probe a visible body during the next actual
//                           job: MoveTo must reject it without model reads;
//                           fails if the armed job never starts
//   native-type-now <id> <text> native edit without waiting for idleness;
//                           these exercise preview cancellation and races
//   native-key-now <key>... key input without waiting for model idleness
//   key-now <key>...       shared key input without the model-idle gate
//   qt-type-now <id> <text> replace a visible Qt editor through actual key
//                           events without waiting for model idleness
//   qt-cancel-progress <job> await the actual modal Qt ComputeProgress for
//                           the newest started job and click its real Cancel
//                           button, requiring modelBusy and cancelling state
//   native-system-key <chord> / native-system-type <text> WindowServer
//                           input for this app's active file panel only;
//                           requires macOS Accessibility/event-post access
//   native-system-pointer <native-id> actual WindowServer pointer movement
//                           over a visible, uncovered control of this active
//                           app; preflight existing access, never requests it
//   native-file-state [label] log the native panel, selection and public responder state
//   native-key-event <key>... await actual AppKit activation/key focus, then
//                            send key down/up through NSApp's event dispatch
//   expect-native-application-delegate check Qt activation signals, deactivate
//                           events, Dock reopen and secure state restoration
//   native-file-name <name> edit the real native save filename value
//   native-file-directory <path> navigate AppKit chooser to existing folder
//   native-file-cancel      cancel the focused AppKit open/save panel
//                           AppKit requires actual user acceptance of its
//                           separate-process chooser; no accept verb exists.
//   native-resize <w> <h>   resize the native document content in points
//   window-resize <w> <h>   resize this test's Qt or native main window
//   window-move <x> <y>     move only this test's Qt/native document relative
//                           to its screen's visible origin
//   window-state normal|minimized|maximized|fullscreen change the document state
//   expect-window-state normal|minimized|maximized|fullscreen await its actual
//                           state (maximized: Qt windows only, mitcad#1710)
//   expect-viewport         check the real framebuffer dimensions against
//                           the viewport's backing scale; log GL version,
//                           logical screen size and reported refresh rate
//                           MITCAD_UI_REQUIRE_RETINA=1 additionally requires 2x
//   capture-view-target <var> record actual OCCT camera target
//   expect-view-pan <before> changed|unchanged compare target in mm
//   space-mouse <tx> <ty> <tz> <rx> <ry> <rz> [ms] hold a 3D mouse's axes
//                           (the view's frame, about ±1 at full deflection)
//                           for ms (300), then release them (mitcad#951)
//   space-mouse-button <n>  press and release a 3D mouse's button n (1..32)
//   native-move <x> <y>     frame origin relative to the main screen's visible
//                           origin; native.window.origin and native.window.size
//                           expose the frame origin and content size as text
//   native-appearance light|dark set this app's AppKit appearance and Qt
//                           palette from real system colors, leaving OS
//                           preferences untouched
//   native-pinch <x%> <y%> <delta> dispatch magnify Began/Changed/Ended through
//                           the actual native document NSWindow
//   qt-pinch <x%> <y%> <delta> send the shared viewer a QNativeGestureEvent
//                           sequence, for comparison with AppKit routing
//   native-context-menu <id> cancel|item open a real AppKit browser/history
//                           context menu; cancel or select in its tracking loop
//   native-popup <id> cancel|item click a native popup button and choose its
//                           actual tracked AppKit menu leaf
//   view-context-menu <x%> <y%> cancel|item right click the actual viewport
//                           or orientation cube, including native menu tracking;
//                           "Sub/Item" chooses an item of a submenu
//   view-context-menu-logged <regex> cancel|item right click a logged position
//   view-pointer press|move|release <x%> <y%> none|left|right|middle [mods]
//                           separate events can simulate a release lost to menus
//   capture-view-orientation <var> record actual camera direction and up
//   expect-view-orientation <before> changed|unchanged compare that orientation
//   expect-view-buttons none|left|right|middle inspect OCCT's actual held buttons
//   expect-selection <total> <profiles> inspect the actual shared selection
//   capture-view-height <var> record the actual OCCT camera height in mm
//   expect-view-zoom <before> in|out|unchanged compare actual camera height
//                           [control-before control-after] also compares
//                           its ratio with a control gesture, detecting
//                           duplicated magnification as well as lost input
//   expect-native-text <id> <regex> assert native control text (6 s)
//   expect-native-accessibility <id> <regex> assert the actual visible
//                           AppKit control or model cell's VoiceOver label
//                           native.file.name/directory read the focused
//                           file panel's public filename/folder properties.
//                           native.window.title the document window's title;
//                           <popup>/items a popup's choices, "title · subtitle"
//                           joined by " | ", <popup>/item-tips their tooltips
//                           (mitcad#1358).
//   expect-native-enabled <id> true|false assert native control state (6 s)
//   expect-native-visible <id> true|false assert native document view state
//   expect-native-present <id> true|false assert document view membership, including hidden views
//   expect-native-attached <id> <anchor-id> above|below the visible control
//                           <id> of a popover lies directly above or below
//                           the document control <anchor-id> and across its
//                           centre, in screen coordinates (6 s)
//   expect-native-cube-arrows each shown orientation cube arrow's AppKit
//                           card is at its hidden Qt arrow's place, inset
//                           by 2 points: the view's place in the window
//                           plus the arrow's in the view (mitcad#1317, 6 s)
//   expect-native-qt-place <widget> above|top <native id> a visible Qt
//                           widget over the 3D view against a visible AppKit
//                           view, both in the 3D view's host: above, the
//                           widget's bottom at or above the view's top; top,
//                           both tops within a point (native.window.content:
//                           the window's content area; mitcad#1358, 6 s)
//   expect-native-style <id> <state,...> what a native table cell or label
//                           shows: error (red text, and a red fill in a
//                           cell), warning, normal, bold, regular, editable,
//                           readonly, editing, not-editing, single-line,
//                           truncated, in-view, readable-fill (opaque cell
//                           fill/text contrast >= 4.5); <table id>/<row>/<column>
//                           addresses a cell without an identifier (6 s)
//                           caption checks an NSButton's displayed single-line
//                           title mode and full native title bounds, including
//                           text-only and text-with-icon buttons (6 s)
//                           A Browser row (mitcad#1352): eye=<icon> its
//                           light bulb's artwork is that shared icon
//                           (eye, eye-half, eye-off); icon=<icon> the row's
//                           icon is it, badged-icon=<icon> it with a corner
//                           badge (a warning's); make-marker the Make tab's
//                           dot after the name, with a tooltip (logged
//                           "Native bulb <id>: <icon>", "Native row icon
//                           <id>: <icon>", "Native Make marker <id>: <tip>")
//   native-header-resize <id> <logical column> <delta points> drag the actual native header divider
//   expect-native-header-resize-retained <id> <logical column> compare the last dragged width
//   expect-native-split-retained <divider id> compare its live position after a real pointer drag
//   expect-native-columns <id> [fit|scroll|any] the native columns keep
//                           the shared header's sizes, hidden sections and
//                           stretching; content-sized headers fit their
//                           actual native title bounds; fit: no sideways scrolling (6 s)
//   native-dialog-size <w> <h> set the focused native dialog's content size
//   native-drag <row-id> <row-id> drag a native outline row onto another
//                           through the outline's own drag source and drop
//                           target, logging moved|refused|not draggable
//   expect-native-focus <id> [<regex>] the keyboard is in that native control
//                           (6 s); with a regex, in a text editor there whose
//                           text matches (a row being named)
//   expect-native-selection <outline-id> <row-id|-|*> the native outline's
//                           selected row, in sight, is its presenter's (6 s);
//                           - none, * whichever the presenter selected;
//                           <table-id> <count> checks both actual row counts,
//                           and for a plain list (formula completions) that
//                           the native selected row is the presenter's
//   expect-native-layout    full-window canvas, floating card geometry and
//                           real AppKit hit testing above/around the canvas
//   expect-native-history [uid...] compare horizontal native history icons,
//                           marker geometry and rollback appearance with the
//                           actual model; optional IDs check native and shared
//                           selection (a single '-' checks empty selection);
//                           a folded timeline group's features are hidden,
//                           its folder cell in their place (mitcad#1077)
//   expect-native-tooltip <id> <regex> the tooltip AppKit shows for a native
//                           control, also one its owner supplies when shown
//                           (the timeline's cells, mitcad#1076) (6 s)
//   native-history-drag begin|to <index>|end send actual marker mouse events;
//                           separate steps let the real model settle during
//                           a single drag before its undo steps are merged
//   native-history-drag-now begin|to <index>|end send the same actual event
//                           immediately, including while the model is busy
//   expect-model-busy true|false assert the worker's actual state immediately
//   hold-model-preview true|false hold asynchronous previews before model
//                           work until released or cancelled by real UI input
//   expect-native-history-wheel  mouse/trackpad history scrolling reaches
//                           both endpoints without leaving the history strip
//   test-manipulator-feedback check actual subpixel length/angle overlay
//                           events at two zooms, placements and cancellation
//   menu-now <menu path> invoke the actual menu action without waiting for
//                           an active worker, to test document transitions
//   cancel-model-job <command name> cancel its running preview through the
//                           worker control, to test its interrupted result;
//                           normal interactive previews have no modal dialog
//   capture-undo-depth <var> read the real document's undo depth
//   expect-undo-depth <base> <change> assert its change since capture
//   expect-model-bodies <count> <volume-mm3> query real OCCT body volumes
//   expect-shown-body <uid> <occurrence|-> <volume-mm3>
//                           [min-x min-y min-z max-x max-y max-z] compare
//                           the viewer's actual clipped shape and world bounds
//   capture-shown-body <prefix> <uid> <occurrence|-> save that actual shape's
//                           volume and world bounds as prefix_volume,
//                           prefix_min_x/y/z and prefix_max_x/y/z variables
//   expect-shown-body-meshes <uid> <occurrence|-> <faces> wait until the
//                           shape the view draws for the body has that many
//                           faces, each with a triangulation lying on it
//                           where the view places it (io::triangulation_on_face);
//                           logs "TestDriver: shown body <uid>[ in
//                           <occurrence>]: N of N faces meshed on their faces"
//   expect-body-edges-unmeshed <uid> wait until no edge of the shown body's
//                           own shape has a 3D polygon, which only the view's
//                           presentations meshing an edge by itself write
//                           (mitcad#1553); logs "TestDriver: body <uid>: 0 of
//                           N edges with a 3D polygon"
//   expect-view-own-shapes  wait until the view shows a sketch profile, a
//                           sketch entity and a datum plane, and none of the
//                           shapes they are drawn from (meshed by their
//                           presentations on the UI thread) has a face, edge
//                           or vertex of a shown body's shape (mitcad#1586);
//                           logs "TestDriver: view's own shapes: N profiles,
//                           N sketch entities and N datum planes shown, 0 of
//                           the bodies' N faces, edges and vertices shared"
//   expect-native-dialog-gone <regex> [seconds] assert native window closed
//                           These native steps require the macOS AppKit UI.
//                           key/type/button/menu/focus-dialog/screenshot
//                           also use AppKit when native controls have focus.
//
// Windows:
//   focus-dialog <title regex> [seconds]  wait for a visible window of the
//                           application with that title (a dialog); key,
//                           type and button then go to it. A message box has
//                           no title on macOS: the regex is matched with its
//                           text and informative text there (a script
//                           gives both: '^Restore Version$|^Restore v1 ')
//   focus-main              give them to the main window again (a dialog
//                           that closed is replaced by the main window too)
//   reveal-widget <objectName> scroll a named, visible Qt widget into view
//                           without changing focus or activating the widget;
//                           subsequent input still uses its logged place
//   focus-widget <objectName> focus a visible Qt widget in the target window
//                           or active popup; it becomes screenshot-dialog's target
//   dialog-resize <w> <h>   resize the focused Qt dialog in logical pixels
//   expect-dialog-appearance light|dark inspect its actual palette and scale;
//                           MITCAD_UI_REQUIRE_RETINA=1 additionally requires 2x
//   expect-command-search <filter> <current-id|*|-> [disabled|shortcuts]
//                           check real search rows, enabled selection and native
//                           shortcut text; * any selected, - none; optional flag
//                           requires disabled rows or the macOS Command glyph
//   expect-command-icon <id> <icon> [<other-id>] the command's action and
//                           its ribbon buttons render as app/icons/<icon>.svg
//                           (menus and the command search take the action's
//                           icon); with <other-id>, that command's icon differs
//   click-parameter <name> <column> click the real visible Qt table cell (0..5)
//   expect-parameter-cell <name> <column> <regex> visible|hidden|refused|absent
//                           check its actual text, filter visibility and error
//   expect-dialog-fits [<w> <h>] the focused Qt dialog's frame within its
//                           screen's available area (at most w x h), its
//                           dialog buttons visible outside any scroll area
//                           and its scroll areas reaching their ends
//   fit-main-frame <w> <h> [least] resize the Qt main window into an
//                           available frame using its actual titlebar/border
//                           margins; `least` raises <h> to the window's own
//                           minimum where it cannot be that short
//   expect-main-frame <w> <h> [least] its real frame fits that
//                           process-local area
//   expect-window-place <x> <y> <w> <h> the Qt main window (its document
//                           frame) with its frame's top left at x, y from its
//                           screen's available area's top left (as
//                           window-move) and its own area w x h (as
//                           window-resize), within a pixel (mitcad#1710)
//   expect-window-on-screen the Qt main window's whole frame within the
//                           primary screen's available area (mitcad#1710)
//   expect-widget-whole <object-name> the one visible Qt widget of that name
//                           as high as its text needs (else its minimum
//                           height) and cut by none of its parents (in view
//                           in a scroll area); in its own scroll area (the
//                           area's widget), that area's view whole, at
//                           least a line high and scrolling to its end
//                           (mitcad#1590); logs "TestDriver: widget whole: …"
//   native-card-width <card> <w> drag a native side card's width handle
//                           (native.projectsCard, native.sidebar or
//                           native.inspectorCard) with AppKit mouse events
//                           towards w points
//   expect-native-card-width <card> <min> [max] its actual width, its
//                           content following it and the free canvas
//   expect-native-name-fits <regex> true|false a visible native text is
//                           shown whole (true) or elided (false)
//   native-screen-limit <w> <h> native secondary windows fit a w x h visible
//                           screen frame (0 0: the real one), as after a
//                           display change
//   expect-native-window-fits <title regex> that native window within its
//                           visible frame, its buttons visible and its whole
//                           body reachable by scrolling
//
// The log (lines are numbered; `mark` remembers where the log is):
//   mark
//   wait-log <regex> [seconds]     a line since the mark matches (30 s)
//   expect-log <regex> [seconds]   the same, 6 s by default
//   expect-no-log <regex>          no line since the mark matches (now)
//   expect-ever <regex> [seconds]  a line anywhere in the log matches (also
//                           what was logged before the script started)
//   expect-never <regex>           no line in the whole log matches
//   capture <var> <regex> [seconds]   the last line since the mark that
//                           matches (10 s); group 1 (or all) into ${var}
//   set <var> <value>
//   expect-near <actual> <expected> [rel] [abs]   numbers: |a - e| <=
//                           max(rel * |e|, abs); rel 1e-6, abs 0.01 by default
//
// The system:
//   wait-idle [seconds]            no job computes (60 s)
//   sleep <ms>
//   color-scheme light|dark   ask the platform for that colour scheme, as
//                           when the desktop's setting changes while Mitcad
//                           runs (MITCAD_TEST_COLOR_SCHEME at a step of the
//                           script's, mitcad#1104; Diagnostics.hpp)
//   expect-file <path> [seconds]   the file or folder exists
//   expect-files <dir> <glob> <count> [seconds]   <count> files in <dir>
//                           match the glob (hidden ones included)
//   expect-clipboard <regex> [seconds]   the clipboard's whole text matches
//                           (6 s); its line ends as put there (\r\n),
//                           (?m) for a line's ^ and $ (mitcad#1641)
//   expect-clipboard-file <path> [seconds]   the file holds exactly the
//                           clipboard's text, in UTF-8: a list's Save as
//                           Text… against its Copy as Text (mitcad#1641)
//   screenshot <png>        the main window (the 3D view composed in)
//   screenshot-view <png>   the 3D view's framebuffer, as --screenshot
//   screenshot-screen <png> macOS: what the window server shows of the
//                           application's windows, the floating panels' child
//                           windows and their glass included (the others
//                           grab widgets; the 3D view of a session without a
//                           GPU may be blank in it)
//                           (headless tests: the visible Qt windows composed,
//                           without native chrome, glass or the 3D view)
//   screenshot-dialog <png> the window focus-dialog chose (the main window
//                           when none), as its widgets draw it
//   screenshot-widget <name> <png>  `screenshot` cut to the visible widget
//                           with that object name, such as timelineDock
//                           (the timeline, mitcad#961)
//   quit [code]             end at once, with exit code <code> (0)
//   kill                    end the process at once, as a killed app would
//                           (no cleanup, autosave files stay), exit code 0
//   stay                    at the end of the script: do not exit
//
// Example (see tools/ui-scripts/ for the workflow tests):
//   command sketch.create
//   expect-log 'Create Sketch: select a plane'
//   click-logged 'Datum xy'
//   key r
//   sketch-click 0 0
//   type 40
//   key Tab
//   type 25
//   key Return
//   expect-log 'Added rectangle 40 x 25 mm'

#include <atomic>
#include <deque>
#include <functional>
#include <memory>
#include <vector>

#include <QElapsedTimer>
#include <QEvent>
#include <QHash>
#include <QMutex>
#include <QObject>
#include <QPoint>
#include <QPointer>
#include <QString>
#include <QStringList>

class QRegularExpression;
class QRegularExpressionMatch;
class QTimer;
class QWidget;

namespace mitcad {

class GlassCard;

class TestDriver : public QObject {
public:
  // Reads MITCAD_TEST_INPUT; without it nothing happens (no handler, no
  // timer). Create it once the QApplication exists and before the main
  // window, so that the log of the start-up is recorded; the driver finds the
  // window itself.
  TestDriver();
  ~TestDriver() override;
  TestDriver(const TestDriver&) = delete;
  TestDriver& operator=(const TestDriver&) = delete;

  bool active() const { return m_active; }

  // A log line (from the message handler, any thread).
  void record(const QString& message);

private:
  struct Step {
    int line = 0;
    QString text; // the script line, for the log
    QString name;
    QStringList args; // as written; ${...} is expanded when it runs
  };
  struct KeyPress {
    int key = 0;
    Qt::KeyboardModifiers modifiers;
    QString text;
  };

  void load(const QString& path);
  void tick();
  void run(const Step& step);
  void fail(int line, const QString& reason);
  bool verifyHeadlessCompletion();
  void finish();
  void leave(int code);

  bool expand(const QString& raw, QString& out, QString& error) const;
  QWidget* inputWindow(const Step& step) const;
  QWidget* targetWindow() const;
  QWidget* topWindow() const;
  QWidget* viewer() const;
  bool findMain();
  static bool dialogMatches(const QWidget* window, const QRegularExpression& re);
  static bool isBlocked(const QWidget* window);
  bool computing() const;

  // Waiting: `ready` is polled every tick (without side effects), `then`
  // runs once it holds; `gate` (a window) must also not be blocked by a
  // modal dialog.
  void whenReady(std::function<bool()> ready, double seconds, const QString& timeoutMessage,
                 std::function<void()> then = {}, QWidget* gate = nullptr);
  void pollWait();

  // The log.
  int lineCount() const;
  int findLine(const QRegularExpression& re, int from, QString* line = nullptr) const;
  int findLastLine(const QRegularExpression& re, int from, QString* line = nullptr,
                   QRegularExpressionMatch* match = nullptr) const;
  bool loggedPoint(const QRegularExpression& re, int from, QPoint& out) const;
  bool sketchPoint(double x, double y, QPoint& out) const;
  bool viewPoint(double xPercent, double yPercent, QPoint& out) const;

  // Events. Gestures and keys are queued, one event per tick.
  void queue(std::function<void()> action);
  void sendKey(const KeyPress& press);
  void sendKeyHalf(const KeyPress& press, bool down);
  // The widget a key goes to (the target window activated by Qt when the
  // system did not activate it); `window` gets the target window.
  QWidget* keyReceiver(QWidget*& window);
  bool triggerShortcut(QWidget* window, const KeyPress& press);
  GlassCard* cardAt(const QPoint& windowPos) const;
  void sendText(const QString& text);
  void sendMouse(QEvent::Type type, const QPoint& windowPos, Qt::MouseButton button, Qt::MouseButtons buttons,
                 Qt::KeyboardModifiers modifiers);
  void queueClick(const QPoint& at, Qt::MouseButton button, Qt::KeyboardModifiers modifiers, int clicks);
  void queueDialogClick(const QPoint& at, int clicks);
  void queueDrag(const QPoint& from, const QPoint& to, Qt::MouseButton button, Qt::KeyboardModifiers modifiers);
  // hold-pick (mitcad#1703): the press held at `at` until the view logs its
  // pick list after log line `since`, at most `limitMs`, then let go there.
  void queueHoldRelease(const QPoint& at, int since, qint64 limitMs, std::shared_ptr<QElapsedTimer> held);
  void testManipulatorFeedback();
  void showDialogStyleFixture(const QString& kind);
  void testDialogControls(const QString& kind);
  void testMacToolButtons();
  void testSketchFinishLayout();
  void testRibbonLabels(const QStringList& expected);
  void testDesignChrome();
  void testInterfaceSize(double ratio);
  void testTextIconSize(int textPercent, int iconPercent, const QString& family);
  void testNavigationRuntime();
  void testNavigationRelease();
  void testNavigationGestureState();
  void setViewNavigation(const QString& scheme);
  void testNavigationControls(const QString& mode);
  void testAssistantSetup(const QString& mode);
  void agentMenuChoice(const QString& row, const QString& state);
  void testFormulaBuilder();
  void openFormulaField(const QString& name);
  void sendWheel(const QPoint& windowPos, int notches, Qt::KeyboardModifiers modifiers);
  // The main window (or the 3D view only), cut to `crop` when given.
  bool saveScreenshot(const QString& path, bool viewOnly, QWidget* crop = nullptr);
  // screenshot-screen in a headless test: the visible Qt windows composed.
  bool saveHeadlessScreen(const QString& path) const;
  // Card widths and windows within the screen (mitcad#818,
  // TestDriverWindowFit.cpp): runs the step if it is one of those.
  bool runWindowFitStep(const QString& name, const QStringList& a, int line);
  bool runDocumentFrameStep(const QString& name, const QStringList& args, int line);
  // The Docked layout's docks hidden and shown again (mitcad#1455,
  // TestDriverDocks.cpp): runs the step if it is one of those.
  bool runDockStep(const QString& name, const QStringList& args, int line);
  // The interface walked (mitcad#1473, TestDriverWalk.cpp): runs the step
  // if it is one of those.
  bool runWalkStep(const QString& name, const QStringList& args, int line);
  // The orbit's center (mitcad#1791, TestDriverOrbitCenter.cpp): runs the
  // step if it is one of those.
  bool runOrbitCenterStep(const QString& name, const QStringList& args, int line);
  void walkClose(qint64 notBefore, qint64 until);

  bool m_active = false;
  QTimer* m_timer = nullptr;
  QElapsedTimer m_clock;
  qint64 m_notBefore = 0;
  int m_stepMs = 50;
  int m_settleMs = 250;

  std::vector<Step> m_steps;
  std::size_t m_next = 0;
  int m_line = 0; // the script line being run
  QString m_error;
  int m_errorLine = 0;
  bool m_stay = false;
  bool m_finished = false;
  QMetaObject::Connection m_nativeJobClickConnection;
  QString m_nativeJobClickLabel;
  int m_nativeJobClickLine = 0;
  QMetaObject::Connection m_nativeJobHistoryDragConnection;
  QString m_nativeJobHistoryDragLabel;
  int m_nativeJobHistoryDragLine = 0;
  QMetaObject::Connection m_jobPickConnection;
  QString m_jobPickLabel;
  int m_jobPickLine = 0;

  QPointer<QWidget> m_main;
  QHash<QString, QPointer<QWidget>> m_documentWindows;
  QPointer<QWidget> m_dialog; // the target of key, type and button; else the main window
  QPointer<QWidget> m_grab;   // the widget a pressed mouse button belongs to
  QPointer<QWidget> m_pointerTarget; // the last ungrabbed pointer receiver
  // The submenu a popup stage opened ("Sub/Item"), taken by the stage
  // that chooses in it; set only while that stage is queued.
  QPointer<QWidget> m_submenu;
  bool m_inSubmenu = false;
  QPointer<QObject> m_found;  // what a wait found
  QList<QPointer<QWidget>> m_walkKeep; // the windows a walk leaves open
  QString m_walkCommand;               // and the command it leaves open
  mutable QPointer<QWidget> m_viewer;

  std::deque<std::function<void()>> m_micro;
  bool m_waiting = false;
  std::function<bool()> m_ready;
  std::function<void()> m_then;
  QElapsedTimer m_waitClock;
  qint64 m_waitLimitMs = 0;
  QString m_waitMessage;
  QString m_waitDetail;
  int m_waitLine = 0;
  QPointer<QWidget> m_gate;
  qint64 m_blockedSince = -1;
  qint64 m_busySince = -1;

  QHash<QString, QString> m_vars;
  int m_mark = 0;

  mutable QMutex m_mutex;
  QStringList m_lines;
  std::atomic<int> m_started{0};
  std::atomic<int> m_ended{0};
  int m_ignoreStarted = -1;
};

} // namespace mitcad
