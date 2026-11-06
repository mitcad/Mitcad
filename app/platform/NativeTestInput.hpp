// SPDX-License-Identifier: MIT
#pragma once

#include <QHash>
#include <QString>
#include <QStringList>
#include <QSet>
#include <QtGlobal>
#include <functional>

class QRegularExpression;
class QOpenGLWidget;
class QPointF;
class QJsonObject;
class QRect;
class QRectF;

namespace mitcad { class CommandHost; }

namespace mitcad::mac::test {

// In-process AppKit input for MITCAD_TEST_INPUT. There is no Accessibility
// API, system event injection, permission prompt or second application loop.
#if defined(MITCAD_MAC_UI) && defined(MITCAD_MACOS_NATIVE_UI)
bool active();
bool nativeFocused();
bool applicationDelegate(QString& error);
bool controlExists(const QString& identifier, QString* detail = nullptr);
bool click(const QString& identifier, QString& error);
// Move keyboard focus without invoking the control or changing selection.
bool focusControl(const QString& identifier, QString& error);
bool hover(const QString& identifier, bool enter, QString& error);
bool clickImmediately(const QString& identifier, QString& error);
bool replaceText(const QString& identifier, const QString& text, QString& error);
bool choose(const QString& identifier, const QString& title, QString& error);
bool duplicateChoices(CommandHost& host, QString& error);
bool widgetRefreshContract(QString& error);
// Real LibraryPicker fallback with a visible key window lacking content;
// restores the previous key window/responder and closes the Qt picker.
bool pickerFallback(QString& error);
bool formControls(CommandHost& host, QString& error);
bool disclosureLayout(QString& error);
bool triggerAction(const QString& identifier, QString& error);
bool triggerMenu(const QString& path, QString& error);
bool activate(const QString& identifier, QString& error);
bool select(const QString& identifier, QString& error, Qt::KeyboardModifiers modifiers = {});
bool expand(const QString& identifier, QString& error);
bool textOf(const QString& identifier, QString& text);
// Reads the actual AppKit accessibility label, including a model cell's
// semantic description, without substituting the shared widget's text.
bool accessibilityLabelOf(const QString& identifier, QString& label);
bool textMatches(const QString& identifier, const QRegularExpression& expression);
bool enabled(const QString& identifier, bool expected);
bool visible(const QString& identifier, bool expected);
bool present(const QString& identifier, bool expected);
// The visible control `identifier` of a popover directly above or below
// the visible document control `anchor` that opened it (mitcad#1021).
bool attached(const QString& identifier, const QString& anchor, bool above, QString& detail);
bool toolbarItemPresent(const QString& identifier, bool expected);
bool configureToolbarItem(const QString& identifier, const QString& operation, QString& error);
bool resizeDocument(int width, int height, QString& error);
bool moveDocument(int x, int y, QString& error);
bool setDocumentState(const QString& state, QString& error);
QString documentState();
bool setAppearance(const QString& name, QString& error);
bool floatingLayout(QString& error);
// `folded`: the features in folded timeline groups (mitcad#1077) and
// their groups' names, whose folder cell stands for them.
bool historyLayout(const QJsonObject& timeline, const QSet<QString>& filtered, const QStringList& selected,
                   bool checkSelection, QString& error, const QHash<QString, QString>& folded = {});
bool historyMarkerEvent(const QString& phase, int index, QString& error);
bool historyWheel(QString& error);
bool trackContextMenu(const QString& choice, const std::function<void()>& open, QString& error,
                      const std::function<void()>& onClosed = {}, const QString& screenshotPath = {});
bool contextMenu(const QString& identifier, const QString& choice, QString& error,
                 const std::function<void()>& onClosed = {});
bool pinch(QOpenGLWidget* viewport, const QPointF& position, double magnification, QString& error);
bool buttonExists(const QRegularExpression& expression);
bool clickButton(const QRegularExpression& expression, QString& error);
bool dialogExists(const QRegularExpression& expression);
bool focusDialog(const QRegularExpression& expression);
bool replaceFileName(const QString& name, QString& error);
bool setFileDirectory(const QString& directory, QString& error);
QString filePanelDiagnostic();
bool cancelFilePanel(QString& error);
void focusMain();
// Mouse input needs application activation, including in a transient
// popover whose owning window can remain the key window.
void requestPointerEventActivation();
bool pointerEventActivationReady();
void requestKeyEventFocus();
bool keyEventFocusReady();
QString keyEventFocusDiagnostic();
bool sendKey(int key, Qt::KeyboardModifiers modifiers, const QString& text);
bool sendKeyEvent(int key, Qt::KeyboardModifiers modifiers, const QString& text);
bool sendText(const QString& text);
// WindowServer events for this app's actual active file panel only. These
// require macOS event-posting permission; no other application is targeted.
bool sendSystemFileKey(int key, Qt::KeyboardModifiers modifiers, QString& error);
bool sendSystemFileText(const QString& text, QString& error);
// Actual cursor movement over one visible, uncovered control owned by this
// active test app. Uses existing event-posting permission, never requests it.
bool sendSystemPointer(const QString& identifier, QString& error);
bool screenshot(const QString& path, QOpenGLWidget* viewport = nullptr);
// Native tables (mitcad#814): what a cell or label shows (comma-separated
// states: error, warning, normal, bold, regular, editable, readonly,
// editing, not-editing, selectable, selected, single-line, truncated, in-view; "<table>/<row>/
// <column>" addresses a cell without an identifier; a Browser row's
// eye=<icon>, icon=<icon>, badged-icon=<icon> and make-marker, mitcad#1352),
// whether the columns
// keep the shared header's sizes and stretching (fit, scroll or any), and
// the focused native dialog's content size.
bool nativeStyle(const QString& identifier, const QStringList& states, QString& detail);
bool nativeColumns(const QString& identifier, const QString& mode, QString& detail);
bool resizeNativeDialog(int width, int height, QString& error);
// Card widths and windows within the screen (mitcad#818): a drag of a side
// card's width handle with AppKit mouse events; its width (and content and
// the free canvas); whether a visible text is shown whole; a smaller visible
// screen frame for secondary windows, followed as a display change; a
// secondary window within that frame with visible buttons and a reachable
// body.
bool dragCardWidth(const QString& card, int width, QString& error);
bool cardWidth(const QString& card, int minimum, int maximum, QString& detail);
bool nameFits(const QRegularExpression& expression, bool expected, QString& detail);
bool setScreenLimit(int width, int height, QString& error);
bool windowFits(const QRegularExpression& title, QString& detail);
// Native Projects rows (mitcad#816): whether outline rows are presented,
// found without scrolling; a row dragged onto another through the
// outline's own drag source and drop target (the outcome is logged); the
// keyboard in a control (with `editor`: a row text editor there whose text
// matches); the outline's one selected row ("-" none, "*" any), in sight and
// the same as its presenter's. `detail` says what was found instead.
bool rowsPresent(const QStringList& identifiers);
bool dragRow(const QString& from, const QString& to, QString& error);
bool focusedIn(const QString& identifier, const QRegularExpression* editor, QString& detail);
bool selectedRow(const QString& outline, const QString& row, QString& detail);
// mitcad#815: the selection card shows `expected` whole, reachable Create
// actions without scrolling; a command form keeps captions beside their
// controls in aligned columns, compact clear/flip controls, OK and Cancel
// whole under its inputs, and the `required` controls in view.
bool selectionCard(int expected, QString& error);
bool commandLayout(const QStringList& required, QString& error);
// Native tile grids, group lists and utility panels (mitcad#813, mitcad#819):
// tileAction drives them through their AppKit input paths ("click <grid>
// <tile>", "star", "group", "slider", "press", ...); tileCheck reads what
// they show ("count", "pictures", "captions", "layout", "focus", ...).
bool tileAction(const QStringList& args, QString& error);
bool tileCheck(const QStringList& args, QString& detail);
// Shared layout bounds, real pointer input and button order (mitcad#812, #817).
bool controlBounds(const QString& identifier, double minimumWidth, const QString& labelPattern, QString& error);
// controlBounds with the width the text field's own cell needs to show its
// whole placeholder in its actual font and insets (mitcad#953).
bool placeholderBounds(const QString& identifier, const QString& labelPattern, QString& error);
// The same with the width for the field's whole current text, such as a
// spin box value with its unit suffix (mitcad#975).
bool textBounds(const QString& identifier, const QString& labelPattern, QString& error);
// Simultaneous, read-only geometry: no scrolling individual controls into
// view. Entries are identifiers, optionally followed by = and a label.
bool formLayout(const QStringList& entries, QString& error);
// Settings pane geometry and repeatable Navigation tab positions (mitcad#992).
bool settingsLayout(const QString& page, QString& error);
// Read-only configuration-card geometry, complete captions and slider
// travel/knob exposure, plus one full reachable Browser row below it;
// never scrolls a clipped control or row into view. framesOnly checks the
// simultaneous card bounds and separation without asserting control sizes.
bool configurationLayout(QString& detail, bool framesOnly = false);
// Read the actual visible group row: full-width caption, no clipping, and
// the shared tree item's selection policy. Does not scroll or create rows.
bool spannedHeading(const QString& table, const QRegularExpression& caption, QString& detail);
// The first row wholly in a native table's viewport, by its first column's
// text (mitcad#972). Read only: nothing is scrolled.
bool firstVisibleRow(const QString& table, const QRegularExpression& text, QString& detail);
// The actual colour-field swatch pixels and drawn placeholder text.
bool colorField(const QString& identifier, const QString& color, const QString& words, bool placeholder,
                const QString& accessibilityLabel, QString& error);
bool buttonOrder(const QStringList& patterns, QString& error);
bool focused(const QString& identifier);
// A real press at the control's default point: the centre of its bounds,
// or of a checkbox's or radio button's box, since AppKit ignores a press on
// the rest of a stretched frame (mitcad#987). Requires a real, uncovered hit
// on that control or one of its descendants.
bool pointerClick(const QString& identifier, QString& error);
// The same at fractions of the control's bounds; a point outside a checkbox's
// or radio button's visible box and title glyphs is refused.
bool pointerClick(const QString& identifier, QString& error, double x, double y);
// A viewport-local logical point through the existing embedded Qt root and
// NSApp mouse routing. Refuses covering native cards; never creates a new
// native handle for the OpenGL child or invokes the model's picker.
bool pointerClickViewport(QOpenGLWidget* viewport, const QPointF& position, QString& error);
// Two real pointer clicks on the same guarded native control, with the
// second press carrying AppKit's double-click count.
bool pointerDoubleClick(const QString& identifier, QString& error);
// Open a real NSPopUpButton by pointer, then navigate/commit in its actual
// menu tracking loop. Keys are Up/Down followed by Return; never set a value.
bool pointerChoice(const QString& identifier, const QString& expected, const QStringList& keys, QString& error);
// A real press on the enabled button whose title matches, at its default
// point as for pointerClick.
bool pointerButton(const QRegularExpression& expression, QString& error);
// Drag the actual header divider, then compare its width after ordinary UI refreshes.
bool resizeHeader(const QString& identifier, int logicalColumn, double delta, QString& error);
bool headerResizeRetained(const QString& identifier, int logicalColumn, QString& error);
// Compare the live divider position with its last real, substantial pointer drag.
bool splitDragRetained(const QString& identifier, QString& error);
// Held sliders require their actual knob and guarded WindowServer input
// with already granted posting access. Other controls use fractional starts.
bool pointerDrag(const QString& identifier, double from, double to, QString& error, bool fromKnob = false);
bool resizeDialog(int width, int height, QString& error);
bool layoutContract(QString& error);
// The tooltip AppKit would show for a control: its toolTip, or the text
// its tooltip owner supplies when shown (the timeline's cells, mitcad#1076).
bool toolTipOf(const QString& identifier, QString& text);
// The left column (mitcad#1020): a drag of the boundary handle `handle`
// (native.columnSplit.<above>.<below>) with AppKit mouse events until the
// card above it is `height` points high, or a double-click on it (height
// -1); a card's height within minimum..maximum, or (minimum -1) reaching
// the bottom of the free column, above the timeline.
bool dragColumnSplit(const QString& handle, int height, QString& error);
bool cardHeight(const QString& card, int minimum, int maximum, QString& detail);
// A native view's frame in the 3D view's host (top-left origin, logical
// points), within a point of `expected` (mitcad#1317: the cube's arrows at
// the hidden Qt arrows' places). Read only.
bool viewportFrame(const QString& identifier, const QRect& expected, QString& detail);
// A visible native view's frame in the 3D view's host (top-left origin,
// logical points); native.window.content is the window's content area
// (mitcad#1358: the Start page's place). Read only.
bool viewportRect(const QString& identifier, QRectF& frame, QString& detail);
// The mouse wheel turned `notches` notches (positive away from you) with
// `modifiers` over a native control's centre, given to the AppKit view hit
// there, which passes it up its responder chain as the system's wheel does
// (mitcad#1350: a command form's or a sketch's value field steps while it
// has the keyboard, the panel scrolls otherwise). In-process: no system
// input.
bool wheel(const QString& identifier, int notches, Qt::KeyboardModifiers modifiers, QString& error);
#else
inline bool active() { return false; }
inline bool nativeFocused() { return false; }
inline bool applicationDelegate(QString&) { return false; }
inline bool controlExists(const QString&, QString* = nullptr) { return false; }
inline bool click(const QString&, QString&) { return false; }
inline bool focusControl(const QString&, QString&) { return false; }
inline bool hover(const QString&, bool, QString&) { return false; }
inline bool clickImmediately(const QString&, QString&) { return false; }
inline bool replaceText(const QString&, const QString&, QString&) { return false; }
inline bool choose(const QString&, const QString&, QString&) { return false; }
inline bool duplicateChoices(CommandHost&, QString&) { return false; }
inline bool widgetRefreshContract(QString&) { return false; }
inline bool pickerFallback(QString&) { return false; }
inline bool formControls(CommandHost&, QString&) { return false; }
inline bool disclosureLayout(QString&) { return false; }
inline bool triggerAction(const QString&, QString&) { return false; }
inline bool triggerMenu(const QString&, QString&) { return false; }
inline bool activate(const QString&, QString&) { return false; }
inline bool select(const QString&, QString&, Qt::KeyboardModifiers = {}) { return false; }
inline bool expand(const QString&, QString&) { return false; }
inline bool textOf(const QString&, QString&) { return false; }
inline bool accessibilityLabelOf(const QString&, QString&) { return false; }
inline bool textMatches(const QString&, const QRegularExpression&) { return false; }
inline bool enabled(const QString&, bool) { return false; }
inline bool visible(const QString&, bool) { return false; }
inline bool present(const QString&, bool) { return false; }
inline bool attached(const QString&, const QString&, bool, QString&) { return false; }
inline bool toolbarItemPresent(const QString&, bool) { return false; }
inline bool configureToolbarItem(const QString&, const QString&, QString&) { return false; }
inline bool resizeDocument(int, int, QString&) { return false; }
inline bool moveDocument(int, int, QString&) { return false; }
inline bool setDocumentState(const QString&, QString&) { return false; }
inline QString documentState() { return {}; }
inline bool setAppearance(const QString&, QString&) { return false; }
inline bool floatingLayout(QString&) { return false; }
inline bool historyLayout(const QJsonObject&, const QSet<QString>&, const QStringList&, bool, QString&,
                          const QHash<QString, QString>& = {}) { return false; }
inline bool historyWheel(QString&) { return false; }
inline bool historyMarkerEvent(const QString&, int, QString&) { return false; }
inline bool trackContextMenu(const QString&, const std::function<void()>&, QString&,
                             const std::function<void()>& = {}, const QString& = {}) { return false; }
inline bool contextMenu(const QString&, const QString&, QString&,
                        const std::function<void()>& = {}) { return false; }
inline bool pinch(QOpenGLWidget*, const QPointF&, double, QString&) { return false; }
inline bool buttonExists(const QRegularExpression&) { return false; }
inline bool clickButton(const QRegularExpression&, QString&) { return false; }
inline bool dialogExists(const QRegularExpression&) { return false; }
inline bool focusDialog(const QRegularExpression&) { return false; }
inline bool replaceFileName(const QString&, QString&) { return false; }
inline bool setFileDirectory(const QString&, QString&) { return false; }
inline QString filePanelDiagnostic() { return {}; }
inline bool cancelFilePanel(QString&) { return false; }
inline void focusMain() {}
inline void requestPointerEventActivation() {}
inline bool pointerEventActivationReady() { return false; }
inline void requestKeyEventFocus() {}
inline bool keyEventFocusReady() { return false; }
inline QString keyEventFocusDiagnostic() { return {}; }
inline bool sendKey(int, Qt::KeyboardModifiers, const QString&) { return false; }
inline bool sendKeyEvent(int, Qt::KeyboardModifiers, const QString&) { return false; }
inline bool sendText(const QString&) { return false; }
inline bool sendSystemFileKey(int, Qt::KeyboardModifiers, QString&) { return false; }
inline bool sendSystemFileText(const QString&, QString&) { return false; }
inline bool sendSystemPointer(const QString&, QString&) { return false; }
inline bool screenshot(const QString&, QOpenGLWidget* = nullptr) { return false; }
inline bool nativeStyle(const QString&, const QStringList&, QString&) { return false; }
inline bool nativeColumns(const QString&, const QString&, QString&) { return false; }
inline bool resizeNativeDialog(int, int, QString&) { return false; }
inline bool dragCardWidth(const QString&, int, QString&) { return false; }
inline bool cardWidth(const QString&, int, int, QString&) { return false; }
inline bool nameFits(const QRegularExpression&, bool, QString&) { return false; }
inline bool setScreenLimit(int, int, QString&) { return false; }
inline bool windowFits(const QRegularExpression&, QString&) { return false; }
inline bool rowsPresent(const QStringList&) { return false; }
inline bool dragRow(const QString&, const QString&, QString&) { return false; }
inline bool focusedIn(const QString&, const QRegularExpression*, QString&) { return false; }
inline bool selectedRow(const QString&, const QString&, QString&) { return false; }
inline bool selectionCard(int, QString&) { return false; }
inline bool commandLayout(const QStringList&, QString&) { return false; }
inline bool tileAction(const QStringList&, QString&) { return false; }
inline bool tileCheck(const QStringList&, QString&) { return false; }
inline bool controlBounds(const QString&, double, const QString&, QString&) { return false; }
inline bool placeholderBounds(const QString&, const QString&, QString&) { return false; }
inline bool textBounds(const QString&, const QString&, QString&) { return false; }
inline bool formLayout(const QStringList&, QString&) { return false; }
inline bool settingsLayout(const QString&, QString&) { return false; }
inline bool configurationLayout(QString&, bool = false) { return false; }
inline bool spannedHeading(const QString&, const QRegularExpression&, QString&) { return false; }
inline bool firstVisibleRow(const QString&, const QRegularExpression&, QString&) { return false; }
inline bool colorField(const QString&, const QString&, const QString&, bool, const QString&, QString&) { return false; }
inline bool buttonOrder(const QStringList&, QString&) { return false; }
inline bool focused(const QString&) { return false; }
inline bool pointerClick(const QString&, QString&) { return false; }
inline bool pointerClick(const QString&, QString&, double, double) { return false; }
inline bool pointerClickViewport(QOpenGLWidget*, const QPointF&, QString&) { return false; }
inline bool pointerDoubleClick(const QString&, QString&) { return false; }
inline bool pointerChoice(const QString&, const QString&, const QStringList&, QString&) { return false; }
inline bool pointerButton(const QRegularExpression&, QString&) { return false; }
inline bool resizeHeader(const QString&, int, double, QString&) { return false; }
inline bool headerResizeRetained(const QString&, int, QString&) { return false; }
inline bool splitDragRetained(const QString&, QString&) { return false; }
inline bool pointerDrag(const QString&, double, double, QString&, bool = false) { return false; }
inline bool resizeDialog(int, int, QString&) { return false; }
inline bool layoutContract(QString&) { return false; }
inline bool toolTipOf(const QString&, QString&) { return false; }
inline bool dragColumnSplit(const QString&, int, QString&) { return false; }
inline bool cardHeight(const QString&, int, int, QString&) { return false; }
inline bool viewportFrame(const QString&, const QRect&, QString&) { return false; }
inline bool viewportRect(const QString&, QRectF&, QString&) { return false; }
inline bool wheel(const QString&, int, Qt::KeyboardModifiers, QString&) { return false; }
#endif

} // namespace mitcad::mac::test
