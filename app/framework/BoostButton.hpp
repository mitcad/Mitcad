// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QObject>
#include <QRect>
#include <QRectF>
#include <QSize>
#include <QString>
#include <QUrl>
#include <QWidget>

class QEvent;
class QMouseEvent;
class QPaintEvent;

namespace mitcad {

// Whether the Boost development button shows (mitcad#1990, app/COMMANDS.md
// "Boost development"): "appearance/boostButton", shown without a key. Its
// × hides it, Preferences › Display › Show the Boost development button
// brings it back. Plain Qt Core.
struct BoostSettings {
  static QString visibleKey();
  static bool loadVisible();
  // Saves it at once and tells BoostNotifier; false when it was already so.
  static bool saveVisible(bool visible);
  // The page the button opens: the project's Ko-fi page.
  static QUrl url();
};

// Tells every window's button and an open Settings window that the button
// was shown or hidden (saveVisible).
class BoostNotifier : public QObject {
  Q_OBJECT

public:
  static BoostNotifier& instance();

signals:
  void visibilityChanged(bool visible);
};

// The Boost development button (mitcad#1990): a quiet outlined pill, its
// outline and rocket in the warm Support role and "Boost development" in
// the window's text, filled with a translucent tint of Support only under
// the pointer (kBoostHoverTint, kBoostPressedTint), and a small × at its
// right end. A click on the pill opens BoostSettings::url()
// through `open` (logged "Boost link opened: <url>"; the window passes
// openExternalUrl, which the UI tests make only log the address); a click
// on the × hides it (BoostSettings::saveVisible(false)). Shown while the
// setting says so (applyVisible, logged "Boost button shown|hidden"). Its
// places in the window's coordinates: "Boost button at x,y" (the pill's
// label) and "Boost button close at x,y" (the ×), logged when they change.
// The top bar of Windows and Linux has it left of the indicators
// (TitleBar::setBoostButton) in both layouts (mitcad#2082); the macOS
// Docked layout, where the system shows the menu bar, in the toolbar's
// tabs' row after the quick-access group (Ribbon::addTabRowEndWidget;
// logged "Boost button row: ...").
// The alpha of the Support tint under the pointer and while pressed.
constexpr int kBoostHoverTint = 51;   // 20 %
constexpr int kBoostPressedTint = 82; // 32 %

class BoostButton : public QWidget {
  Q_OBJECT

public:
  BoostButton(int height, std::function<void(const QUrl&)> open, QWidget* parent = nullptr);

  QSize sizeHint() const override;
  // The rocket and the × alone: in a row short of room (the Floating top
  // bar) the label is cut first. The compact form is as wide as its hint.
  QSize minimumSizeHint() const override;

  // The compact form (mitcad#2079, docs/ui-conventions.md 13, Docked tabs
  // in a narrow window): the rocket and the × close together in its
  // outlined pill, its name and where it leads in the tooltip. The Docked
  // tabs' row takes it before any tab folds, so that its name is never cut
  // there. Logged "Boost button compact" or "Boost button wide" when it
  // changes; its places as the whole pill's ("Boost button at x,y" the
  // rocket, "Boost button close at x,y" the ×).
  void setCompact(bool compact);
  bool isCompact() const { return m_compact; }
  // The whole pill's width (the rocket, the name and the ×) and the
  // compact one's.
  int wideWidth() const;
  int compactWidth() const;

  // Shown or hidden as the setting says, once the button has its place in
  // a row; then again whenever the setting changes (BoostNotifier).
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
  // The pill's rectangle and its ×'s, in the widget's coordinates.
  QRectF pillRect() const;
  QRect closeRect() const;
  Part partAt(const QPoint& point) const;
  void logPlaces();

  std::function<void(const QUrl&)> m_open;
  Part m_hovered = Part::None;
  Part m_pressed = Part::None;
  int m_logged = -1; // the visibility logged last: -1 none yet
  QString m_loggedPlaces;
  bool m_compact = false;
};

} // namespace mitcad
