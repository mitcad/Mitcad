// SPDX-License-Identifier: MIT
// Ctrl held alone hides a feature command's preview (mitcad#1421): when the
// held key hides it and shows it again, from synthetic events given to the
// filter as the application delivers them. Ctrl pressed alone hides it at
// once; a click, the wheel and Shift keep it hidden; another key (a
// shortcut) shows it again at once, as do Ctrl's release (also one lost,
// seen as a click without Ctrl) and the application's deactivation; Ctrl
// pressed again while it is down (repeated, auto-repeated, a forwarded copy
// with its modifier toggled) changes nothing; a Ctrl pressed with another
// modifier, during a drag or where it may not hide does nothing. And which
// widgets count as text fields.
#include "framework/PreviewHold.hpp"

#include <cstdio>

#include <QEvent>
#include <QKeyEvent>
#include <QLineEdit>
#include <QMouseEvent>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QStringList>
#include <QWheelEvent>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "preview_hold.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// What the hold did: "hide", "show: <why>".
struct Recorder {
  QStringList calls;
  bool allowed = true;

  mitcad::PreviewHold::Actions actions() {
    mitcad::PreviewHold::Actions result;
    result.mayHide = [this] { return allowed; };
    result.hide = [this] { calls << QStringLiteral("hide"); };
    result.show = [this](const QString& why) { calls << QStringLiteral("show: ") + why; };
    return result;
  }
};

const QString kHide = QStringLiteral("hide");
const QString kReleased = QStringLiteral("show: Ctrl released");
const QString kKey = QStringLiteral("show: a key pressed");

void key(mitcad::PreviewHold& hold, QEvent::Type type, int code, Qt::KeyboardModifiers modifiers,
         bool autoRepeat = false) {
  QKeyEvent event(type, code, modifiers, QString(), autoRepeat);
  hold.handle(&event);
}

// Ctrl pressed as a platform delivers it: its shortcut offer, then its press.
void pressCtrl(mitcad::PreviewHold& hold) {
  key(hold, QEvent::ShortcutOverride, Qt::Key_Control, Qt::ControlModifier);
  key(hold, QEvent::KeyPress, Qt::Key_Control, Qt::ControlModifier);
}

void releaseCtrl(mitcad::PreviewHold& hold) { key(hold, QEvent::KeyRelease, Qt::Key_Control, Qt::NoModifier); }

// A key with Ctrl as the platform delivers it: its offer, then its press.
void ctrlKey(mitcad::PreviewHold& hold, int code) {
  key(hold, QEvent::ShortcutOverride, code, Qt::ControlModifier);
  key(hold, QEvent::KeyPress, code, Qt::ControlModifier);
}

void wheel(mitcad::PreviewHold& hold, Qt::KeyboardModifiers modifiers) {
  QWheelEvent event(QPointF(10, 10), QPointF(10, 10), QPoint(), QPoint(0, 120), Qt::NoButton, modifiers,
                    Qt::NoScrollPhase, false);
  hold.handle(&event);
}

void mouse(mitcad::PreviewHold& hold, QEvent::Type type, Qt::MouseButton button, Qt::MouseButtons buttons,
           Qt::KeyboardModifiers modifiers) {
  QMouseEvent event(type, QPointF(10, 10), QPointF(10, 10), button, buttons, modifiers);
  hold.handle(&event);
}

// A click: the button pressed and released.
void click(mitcad::PreviewHold& hold, Qt::MouseButton button, Qt::KeyboardModifiers modifiers) {
  mouse(hold, QEvent::MouseButtonPress, button, button, modifiers);
  mouse(hold, QEvent::MouseButtonRelease, button, Qt::NoButton, modifiers);
}

void heldAloneTests() {
  // Pressed alone: hidden at once; released: shown.
  Recorder record;
  mitcad::PreviewHold hold(record.actions(), nullptr);
  pressCtrl(hold);
  CHECK(hold.hidden());
  CHECK(record.calls == QStringList{kHide});
  // A Ctrl+click, Ctrl+scroll, a Ctrl + middle button pan and Shift keep it
  // hidden.
  click(hold, Qt::LeftButton, Qt::ControlModifier);
  wheel(hold, Qt::ControlModifier);
  click(hold, Qt::MiddleButton, Qt::ControlModifier);
  key(hold, QEvent::KeyPress, Qt::Key_Shift, Qt::ControlModifier | Qt::ShiftModifier);
  CHECK(hold.hidden());
  CHECK(record.calls.size() == 1);
  releaseCtrl(hold);
  CHECK(!hold.hidden());
  CHECK(record.calls == (QStringList{kHide, kReleased}));
  // A release without a hold shows nothing.
  releaseCtrl(hold);
  CHECK(record.calls.size() == 2);
  // And the next press hides again.
  pressCtrl(hold);
  CHECK(hold.hidden());
  releaseCtrl(hold);
  CHECK(record.calls.size() == 4);
}

void repeatedPressTests() {
  // Ctrl held as the platforms and the test driver deliver it: its offer
  // and press, the press again at each widget it passes, a floating
  // panel's copy sent on to the main window (QKeyEvent gives a modifier
  // key's own modifier toggled, so the copy says Ctrl where the original
  // did not), and a held key's repeated presses, auto-repeat or not. Each
  // is the same press: hidden once, shown once on the release.
  Recorder record;
  mitcad::PreviewHold hold(record.actions(), nullptr);
  pressCtrl(hold);
  key(hold, QEvent::KeyPress, Qt::Key_Control, Qt::NoModifier);
  key(hold, QEvent::KeyPress, Qt::Key_Control, Qt::ControlModifier);
  key(hold, QEvent::KeyPress, Qt::Key_Control, Qt::ControlModifier, true);
  key(hold, QEvent::ShortcutOverride, Qt::Key_Control, Qt::ControlModifier, true);
  key(hold, QEvent::KeyPress, Qt::Key_Control, Qt::NoModifier, true);
  key(hold, QEvent::KeyRelease, Qt::Key_Control, Qt::ControlModifier, true);
  CHECK(hold.hidden());
  CHECK(record.calls == QStringList{kHide});
  releaseCtrl(hold);
  CHECK(record.calls == (QStringList{kHide, kReleased}));
  // A press reported with its own modifier only (no Shift or Alt): alone.
  key(hold, QEvent::KeyPress, Qt::Key_Control, Qt::NoModifier);
  CHECK(hold.hidden());
  releaseCtrl(hold);
  // Pressed while a mouse button is held (a drag): not alone; after the
  // button's release it is.
  record.calls.clear();
  mouse(hold, QEvent::MouseButtonPress, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
  pressCtrl(hold);
  CHECK(!hold.hidden());
  releaseCtrl(hold);
  mouse(hold, QEvent::MouseButtonRelease, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
  pressCtrl(hold);
  CHECK(hold.hidden());
  releaseCtrl(hold);
  CHECK(record.calls == (QStringList{kHide, kReleased}));
}

void shortcutTests() {
  // Ctrl+Z (its offer, as the platform sends it before it runs the
  // shortcut, and its press): hidden by Ctrl, shown again at once by Z; it
  // stays shown while Ctrl is down, and the release shows nothing more.
  Recorder record;
  mitcad::PreviewHold hold(record.actions(), nullptr);
  pressCtrl(hold);
  ctrlKey(hold, Qt::Key_Z);
  CHECK(!hold.hidden());
  CHECK(record.calls == (QStringList{kHide, kKey}));
  ctrlKey(hold, Qt::Key_S);
  click(hold, Qt::LeftButton, Qt::ControlModifier);
  CHECK(!hold.hidden());
  releaseCtrl(hold);
  CHECK(record.calls.size() == 2);
  // A shortcut that reaches its action without an offer (QEvent::Shortcut).
  pressCtrl(hold);
  QEvent shortcut(QEvent::Shortcut);
  hold.handle(&shortcut);
  CHECK(!hold.hidden());
  releaseCtrl(hold);
  CHECK(record.calls == (QStringList{kHide, kKey, kHide, kKey}));
}

void refusedTests() {
  // Pressed with Shift held, or where it may not hide (a text field, a
  // dialog): nothing, also on its release.
  Recorder record;
  mitcad::PreviewHold hold(record.actions(), nullptr);
  key(hold, QEvent::KeyPress, Qt::Key_Control, Qt::ControlModifier | Qt::ShiftModifier);
  CHECK(!hold.hidden());
  releaseCtrl(hold);
  record.allowed = false;
  pressCtrl(hold);
  CHECK(!hold.hidden());
  releaseCtrl(hold);
  CHECK(record.calls.isEmpty());
}

void shownAgainTests() {
  // The application deactivated: shown.
  Recorder record;
  mitcad::PreviewHold hold(record.actions(), nullptr);
  pressCtrl(hold);
  QEvent deactivated(QEvent::ApplicationDeactivate);
  hold.handle(&deactivated);
  CHECK(!hold.hidden());
  CHECK(record.calls == (QStringList{kHide, QStringLiteral("show: the application deactivated")}));
  releaseCtrl(hold);
  // Ctrl's release lost (it went to another window): a click or a key
  // without Ctrl shows it.
  record.calls.clear();
  pressCtrl(hold);
  click(hold, Qt::LeftButton, Qt::NoModifier);
  CHECK(!hold.hidden());
  pressCtrl(hold);
  key(hold, QEvent::KeyPress, Qt::Key_A, Qt::NoModifier);
  CHECK(!hold.hidden());
  CHECK(record.calls == (QStringList{kHide, kReleased, kHide, kReleased}));
}

void textFieldTests() {
  QLineEdit line;
  CHECK(mitcad::PreviewHold::editsText(&line));
  line.setReadOnly(true);
  CHECK(!mitcad::PreviewHold::editsText(&line));
  QPlainTextEdit plain;
  CHECK(mitcad::PreviewHold::editsText(&plain));
  QPushButton button;
  CHECK(!mitcad::PreviewHold::editsText(&button));
  CHECK(!mitcad::PreviewHold::editsText(nullptr));
}

} // namespace

int previewHoldTests() {
  heldAloneTests();
  repeatedPressTests();
  shortcutTests();
  refusedTests();
  shownAgainTests();
  textFieldTests();
  return failures;
}
