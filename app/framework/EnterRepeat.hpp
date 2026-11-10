// SPDX-License-Identifier: MIT
#pragma once

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

} // namespace mitcad
