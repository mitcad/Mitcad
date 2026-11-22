// SPDX-License-Identifier: MIT
#pragma once

#include <QObject>
#include <QPointer>
#include <QRect>
#include <QRectF>
#include <QSize>
#include <QString>
#include <QUrl>
#include <QWidget>

class QAction;
class QEvent;
class QMouseEvent;
class QPaintEvent;

namespace mitcad {

// The address of MitCad's Discord server (mitcad#2123, app/COMMANDS.md
// "Discord"): the project's own redirect, so that an invitation can change
// without a new release.
QUrl discordUrl();

// Whether the Discord button shows (mitcad#2159, app/COMMANDS.md
// "Discord"): "appearance/discordButton", shown without a key. Its × hides
// it, Preferences › Display › Show the Discord button brings it back. Apart
// from the Boost development button's setting (BoostSettings): either ×
// hides only its own button. Plain Qt Core.
struct DiscordSettings {
  static QString visibleKey();
  static bool loadVisible();
  // Saves it at once and tells DiscordNotifier; false when it was already
  // so.
  static bool saveVisible(bool visible);
};

// Tells every window's button and an open Settings window that the button
// was shown or hidden (saveVisible).
class DiscordNotifier : public QObject {
  Q_OBJECT

public:
  static DiscordNotifier& instance();

signals:
  void visibilityChanged(bool visible);
};

// The Discord button (mitcad#2123, a pill since mitcad#2159): right of the
// Boost development button (BoostButton) in each place that has one: the
// top bar of Windows and Linux in both layouts (TitleBar::setDiscordButton,
// the row part "titleDiscord") and the macOS Docked toolbar's tab row
// (Ribbon::addTabRowEndWidget). A pill of the Boost pill's shape and
// height, filled with Discord's blurple (ColorRole::DiscordFill, the same
// on both themes), holding Discord's logo mark in white
// (ColorRole::DiscordMark; the mark :/brand/discord-mark.svg, its source
// and terms in third_party/brand-marks/README.md) and a small × at its
// right end; no label. A click on the pill triggers `action`
// (`help.discord`, the Help menu's "MitCad on Discord"), which opens
// discordUrl() and logs "Discord link opened: <url>"; the tooltip "Opens
// MitCad's Discord server". A click on the × hides it in every window
// (DiscordSettings::saveVisible(false)); Boost's × leaves it. Shown while
// the setting says so (applyVisible, logged "Discord button shown|hidden").
// `height` is the Boost button's beside it. Its places in the window's
// coordinates: "Discord button at x,y" (the mark) and "Discord button
// close at x,y" (the ×), logged when they change. Already as compact as
// the Boost pill's compact form: it has no compact form of its own.
// How much darker the blurple is under the pointer and while pressed
// (QColor::darker's factors); the white mark keeps its contrast on them.
constexpr int kDiscordHoverDarker = 112;
constexpr int kDiscordPressedDarker = 125;

class DiscordButton : public QWidget {
  Q_OBJECT

public:
  DiscordButton(QAction* action, int height, QWidget* parent = nullptr);

  QSize sizeHint() const override;
  QSize minimumSizeHint() const override { return sizeHint(); }

  // Shown or hidden as the setting says, once the button has its place in
  // a row; then again whenever the setting changes (DiscordNotifier).
  void applyVisible();

protected:
  bool event(QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;
  void mouseMoveEvent(QMouseEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;
  void mouseReleaseEvent(QMouseEvent* event) override;
  void leaveEvent(QEvent* event) override;
  void changeEvent(QEvent* event) override;

private:
  enum class Part { None, Pill, Close };
  // The pill's rectangle, the mark's and the ×'s, in the widget's
  // coordinates.
  QRectF pillRect() const;
  QRectF markRect() const;
  QRect closeRect() const;
  Part partAt(const QPoint& point) const;
  void logPlaces();

  QPointer<QAction> m_action;
  Part m_hovered = Part::None;
  Part m_pressed = Part::None;
  int m_logged = -1; // the visibility logged last: -1 none yet
  QString m_loggedPlaces;
};

} // namespace mitcad
