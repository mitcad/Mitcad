// SPDX-License-Identifier: MIT
#include "DiscordButton.hpp"

#include <algorithm>

#include <QAction>
#include <QEvent>
#include <QtLogging>

#include "PlaceWatcher.hpp"
#include "UiSizes.hpp"

namespace mitcad {

QUrl discordUrl() { return QUrl(QStringLiteral("https://mitcad.org/discord")); }

DiscordButton::DiscordButton(QAction* action, int height, QWidget* parent)
    // The icon with 5 px around it (docs/ui-conventions.md 11), within the
    // top bar's and the rows' icon sizes.
    : RibbonButton(action, std::clamp(height - 10, uiIconSize(12), uiIconSize(18)), parent), m_height(height) {
  setObjectName(QStringLiteral("discordButton"));
  setFixedSize(sizeHint());
  setCursor(Qt::PointingHandCursor);
  new PlaceWatcher(this, [this] { logPlace(); });
}

QSize DiscordButton::sizeHint() const {
  // As high as the Boost development button beside it, square.
  return QSize(m_height, m_height);
}

bool DiscordButton::event(QEvent* event) {
  if (event->type() == QEvent::Hide) {
    // Its place is logged again when it shows.
    m_loggedPlace.clear();
  }
  return RibbonButton::event(event);
}

void DiscordButton::logPlace() {
  if (!isVisible() || window() == nullptr) {
    return;
  }
  const QPoint at = mapTo(window(), rect().center());
  const QString place = QStringLiteral("Discord button at %1,%2").arg(at.x()).arg(at.y());
  if (place == m_loggedPlace) {
    return;
  }
  m_loggedPlace = place;
  qDebug().noquote() << place;
}

} // namespace mitcad
