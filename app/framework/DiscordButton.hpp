// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QUrl>

#include "Ribbon.hpp"

class QAction;

namespace mitcad {

// The address of MitCad's Discord server (mitcad#2123, app/COMMANDS.md
// "Discord"): the project's own redirect, so that an invitation can change
// without a new release.
QUrl discordUrl();

// The Discord button (mitcad#2123): an icon button right of the Boost
// development button (BoostButton), in each place that has one: the top bar
// of Windows and Linux in both layouts (TitleBar::setDiscordButton, the row
// part "titleDiscord") and the macOS Docked toolbar's tab row
// (Ribbon::addTabRowEndWidget). The icon only (`community`, MitCad's own
// speech bubble, tinted as the bar's other icons by RibbonButton), its
// action `help.discord` (the Help menu's "MitCad on Discord"), which opens
// discordUrl() and logs "Discord link opened: <url>"; the tooltip "Opens
// MitCad's Discord server". `height` is the Boost button's beside it. It
// has no × and no setting: it stays when Boost development is hidden. Its
// place in the window's coordinates, its middle, is logged "Discord button
// at x,y" when it changes.
class DiscordButton : public RibbonButton {
  Q_OBJECT

public:
  DiscordButton(QAction* action, int height, QWidget* parent = nullptr);

  QSize sizeHint() const override;
  QSize minimumSizeHint() const override { return sizeHint(); }

protected:
  bool event(QEvent* event) override;

private:
  void logPlace();

  int m_height;
  QString m_loggedPlace;
};

} // namespace mitcad
