// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QObject>
#include <QString>

class QEvent;
class QWidget;

namespace mitcad {

// Ctrl (Cmd on macOS, Qt's ControlModifier and Key_Control there too) held
// alone hides a feature command's preview while its inputs are picked, and
// its release shows it again (mitcad#1421, app/COMMANDS.md "Holding Ctrl
// hides the preview"). The keys, the wheel and the mouse are read from the
// application's events (an event filter on it that never takes one):
//
// - Ctrl pressed alone (no other modifier, no mouse button held, `mayHide`
//   true: no dialog, the command's window active, not typing in a text
//   field) runs `hide` at once (owner's decision, 7.11.2026: no delay).
//   Ctrl pressed again while it is down (its shortcut offer and press,
//   each widget the press passes, a floating panel's copy sent on to the
//   main window, a platform's repeated press) changes nothing, whatever
//   modifiers it carries.
// - While Ctrl is down, a key that is no modifier (its shortcut offer, its
//   press or the shortcut it makes: Ctrl+Z, Ctrl+S) shows the preview again
//   at once (`show`, "a key pressed") and goes on to make its shortcut; it
//   stays shown until Ctrl is pressed again. Clicks, the wheel and Shift or
//   Alt leave it hidden: a click then is a Ctrl+click on what the preview
//   covered.
// - Ctrl's release (also a click, a wheel or a key that comes without Ctrl,
//   its release lost) and the application's deactivation show it again
//   ("Ctrl released", "the application deactivated"). Auto-repeat is
//   ignored.
class PreviewHold : public QObject {
public:
  struct Actions {
    std::function<bool()> mayHide;
    std::function<void()> hide;
    std::function<void(const QString& why)> show;
  };

  // Watches the application's events once install() is called; `parent`
  // owns it (the command's session), and its end removes the filter.
  PreviewHold(Actions actions, QObject* parent);

  void install();
  // The preview hidden by a held Ctrl.
  bool hidden() const { return m_state == State::Hidden; }

  // An event the application delivers (eventFilter); public for the unit
  // test.
  void handle(QEvent* event);

  // Whether the keyboard is in a field where Ctrl+A, C and V edit text.
  static bool editsText(const QWidget* widget);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  // Idle: Ctrl is up. Hidden: Ctrl is down and hides the preview. Shown:
  // Ctrl is down and hides nothing (refused, or a key showed it again).
  enum class State { Idle, Hidden, Shown };
  void ctrlPressed(Qt::KeyboardModifiers modifiers);
  // Ctrl pressed where it hides nothing; logged "Preview hold refused: ...".
  void refuse(const QString& why);
  // A key that is no modifier, or a shortcut, while Ctrl is down.
  void otherKey();
  void released(const QString& why);

  Actions m_actions;
  State m_state = State::Idle;
  // The mouse buttons held, from the mouse events the application
  // delivers (synthetic ones too).
  Qt::MouseButtons m_buttons = Qt::NoButton;
};

} // namespace mitcad
