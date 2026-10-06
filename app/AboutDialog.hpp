// SPDX-License-Identifier: MIT
#pragma once

// Help > About Mitcad: the version, the licence and the third-party
// components with their licences (also the application menu's About on
// macOS).

class QWidget;

namespace mitcad {

// Shows the About box, modal to `parent`.
void showAboutDialog(QWidget* parent);

} // namespace mitcad
