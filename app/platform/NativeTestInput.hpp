// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QStringList>
#include <QSet>
#include <QtGlobal>
#include <functional>

class QRegularExpression;
class QOpenGLWidget;
class QPointF;
class QJsonObject;

namespace mitcad { class CommandHost; }

namespace mitcad::mac::test {

// In-process AppKit input for MITCAD_TEST_INPUT. There is no Accessibility
// API, system event injection, permission prompt or second application loop.
#if defined(Q_OS_MACOS) && defined(MITCAD_MACOS_NATIVE_UI)
bool active();
bool nativeFocused();
bool applicationDelegate(QString& error);
bool controlExists(const QString& identifier, QString* detail = nullptr);
bool click(const QString& identifier, QString& error);
bool hover(const QString& identifier, bool enter, QString& error);
bool clickImmediately(const QString& identifier, QString& error);
bool replaceText(const QString& identifier, const QString& text, QString& error);
bool choose(const QString& identifier, const QString& title, QString& error);
bool duplicateChoices(CommandHost& host, QString& error);
bool widgetRefreshContract(QString& error);
bool formControls(CommandHost& host, QString& error);
bool disclosureLayout(QString& error);
bool triggerAction(const QString& identifier, QString& error);
bool triggerMenu(const QString& path, QString& error);
bool activate(const QString& identifier, QString& error);
bool select(const QString& identifier, QString& error, Qt::KeyboardModifiers modifiers = {});
bool expand(const QString& identifier, QString& error);
bool textOf(const QString& identifier, QString& text);
bool textMatches(const QString& identifier, const QRegularExpression& expression);
bool enabled(const QString& identifier, bool expected);
bool visible(const QString& identifier, bool expected);
bool present(const QString& identifier, bool expected);
bool toolbarItemPresent(const QString& identifier, bool expected);
bool configureToolbarItem(const QString& identifier, const QString& operation, QString& error);
bool resizeDocument(int width, int height, QString& error);
bool moveDocument(int x, int y, QString& error);
bool setDocumentState(const QString& state, QString& error);
QString documentState();
bool setAppearance(const QString& name, QString& error);
bool floatingLayout(QString& error);
bool historyLayout(const QJsonObject& timeline, const QSet<QString>& filtered, const QStringList& selected,
                   bool checkSelection, QString& error);
bool historyMarkerEvent(const QString& phase, int index, QString& error);
bool historyWheel(QString& error);
bool trackContextMenu(const QString& choice, const std::function<void()>& open, QString& error,
                      const std::function<void()>& onClosed = {}, const QString& screenshotPath = {});
bool contextMenu(const QString& identifier, const QString& choice, QString& error);
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
// <column>" addresses a cell without an identifier), whether the columns
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
// Simultaneous, read-only geometry: no scrolling individual controls into
// view. Entries are identifiers, optionally followed by = and a label.
bool formLayout(const QStringList& entries, QString& error);
// Read-only configuration-card geometry, complete captions and slider
// travel/knob exposure, plus one full reachable Browser row below it;
// never scrolls a clipped control or row into view.
bool configurationLayout(QString& detail);
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
// Optional normalized point within the control; still requires a real,
// uncovered hit on that control or one of its descendants.
bool pointerClick(const QString& identifier, QString& error, double x = 0.5, double y = 0.5);
// Two real pointer clicks on the same guarded native control, with the
// second press carrying AppKit's double-click count.
bool pointerDoubleClick(const QString& identifier, QString& error);
// Open a real NSPopUpButton by pointer, then navigate/commit in its actual
// menu tracking loop. Keys are Up/Down followed by Return; never set a value.
bool pointerChoice(const QString& identifier, const QString& expected, const QStringList& keys, QString& error);
bool pointerButton(const QRegularExpression& expression, QString& error);
// Held sliders require their actual knob and guarded WindowServer input
// with already granted posting access. Other controls use fractional starts.
bool pointerDrag(const QString& identifier, double from, double to, QString& error, bool fromKnob = false);
bool resizeDialog(int width, int height, QString& error);
bool layoutContract(QString& error);
#else
inline bool active() { return false; }
inline bool nativeFocused() { return false; }
inline bool applicationDelegate(QString&) { return false; }
inline bool controlExists(const QString&, QString* = nullptr) { return false; }
inline bool click(const QString&, QString&) { return false; }
inline bool hover(const QString&, bool, QString&) { return false; }
inline bool clickImmediately(const QString&, QString&) { return false; }
inline bool replaceText(const QString&, const QString&, QString&) { return false; }
inline bool choose(const QString&, const QString&, QString&) { return false; }
inline bool duplicateChoices(CommandHost&, QString&) { return false; }
inline bool widgetRefreshContract(QString&) { return false; }
inline bool formControls(CommandHost&, QString&) { return false; }
inline bool disclosureLayout(QString&) { return false; }
inline bool triggerAction(const QString&, QString&) { return false; }
inline bool triggerMenu(const QString&, QString&) { return false; }
inline bool activate(const QString&, QString&) { return false; }
inline bool select(const QString&, QString&, Qt::KeyboardModifiers = {}) { return false; }
inline bool expand(const QString&, QString&) { return false; }
inline bool textOf(const QString&, QString&) { return false; }
inline bool textMatches(const QString&, const QRegularExpression&) { return false; }
inline bool enabled(const QString&, bool) { return false; }
inline bool visible(const QString&, bool) { return false; }
inline bool present(const QString&, bool) { return false; }
inline bool toolbarItemPresent(const QString&, bool) { return false; }
inline bool configureToolbarItem(const QString&, const QString&, QString&) { return false; }
inline bool resizeDocument(int, int, QString&) { return false; }
inline bool moveDocument(int, int, QString&) { return false; }
inline bool setDocumentState(const QString&, QString&) { return false; }
inline QString documentState() { return {}; }
inline bool setAppearance(const QString&, QString&) { return false; }
inline bool floatingLayout(QString&) { return false; }
inline bool historyLayout(const QJsonObject&, const QSet<QString>&, const QStringList&, bool, QString&) { return false; }
inline bool historyWheel(QString&) { return false; }
inline bool historyMarkerEvent(const QString&, int, QString&) { return false; }
inline bool trackContextMenu(const QString&, const std::function<void()>&, QString&,
                             const std::function<void()>& = {}, const QString& = {}) { return false; }
inline bool contextMenu(const QString&, const QString&, QString&) { return false; }
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
inline bool formLayout(const QStringList&, QString&) { return false; }
inline bool configurationLayout(QString&) { return false; }
inline bool spannedHeading(const QString&, const QRegularExpression&, QString&) { return false; }
inline bool firstVisibleRow(const QString&, const QRegularExpression&, QString&) { return false; }
inline bool colorField(const QString&, const QString&, const QString&, bool, const QString&, QString&) { return false; }
inline bool buttonOrder(const QStringList&, QString&) { return false; }
inline bool focused(const QString&) { return false; }
inline bool pointerClick(const QString&, QString&, double = 0.5, double = 0.5) { return false; }
inline bool pointerDoubleClick(const QString&, QString&) { return false; }
inline bool pointerChoice(const QString&, const QString&, const QStringList&, QString&) { return false; }
inline bool pointerButton(const QRegularExpression&, QString&) { return false; }
inline bool pointerDrag(const QString&, double, double, QString&, bool = false) { return false; }
inline bool resizeDialog(int, int, QString&) { return false; }
inline bool layoutContract(QString&) { return false; }
#endif

} // namespace mitcad::mac::test
