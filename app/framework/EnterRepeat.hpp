// SPDX-License-Identifier: MIT
#pragma once

#include <QKeySequence>
#include <QObject>
#include <QPointer>

class QAction;
class QEvent;
class QWidget;

namespace mitcad {

// Enter repeats the last command (mitcad#1620, app/COMMANDS.md "Enter
// repeats the last command"): only while the widget with the keyboard
// focus does not take Enter itself. These do: a text or number field (a
// rename's editor too), a drop-down list, a list or tree (the Projects
// panel opens a design on Enter, the browser and the timeline), and a
// button. The 3D view, the window and no focus at all do not. Plain Qt
// Widgets.
bool focusTakesEnter(const QWidget* focus);

// The key a user gave the command Repeat in Keyboard Shortcuts (mitcad#2052;
// it has none by default). Installed on the application, it watches the
// keys offered as shortcuts (QEvent::ShortcutOverride) to the widget with
// the focus:
// - one of the action's keys, while the action is enabled, reaches a
//   widget that takes Enter (focusTakesEnter) as an ordinary key press, as
//   though the widget had asked for it: Space bound to Repeat still toggles
//   a focused check box, presses a button, types in a field. The action
//   stays enabled meanwhile, so that Command Search, whose own field has
//   the focus, lists Repeat as available.
// - elsewhere in `window` (the 3D view) the key is kept for takeKey(), so
//   that the run that follows can name it (`Repeat Extrude (Space)`).
class RepeatKeyFilter : public QObject {
public:
  RepeatKeyFilter(QAction* action, QWidget* window);
  ~RepeatKeyFilter() override;

  // The key that triggered the action, taken once; empty when it ran
  // otherwise (Command Search, a menu, MCP).
  QKeySequence takeKey();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  QPointer<QAction> m_action;
  QPointer<QWidget> m_window;
  QKeySequence m_key;
};

} // namespace mitcad
