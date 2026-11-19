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
// The Floating top bar of Windows and Linux has it left of the indicators
// (TitleBar::setBoostButton), the Docked toolbar at the right end of its
// tabs' row (Ribbon::addTabRowEndWidget).
// The alpha of the Support tint under the pointer and while pressed.
constexpr int kBoostHoverTint = 51;   // 20 %
constexpr int kBoostPressedTint = 82; // 32 %

class BoostButton : public QWidget {
  Q_OBJECT

public:
  BoostButton(int height, std::function<void(const QUrl&)> open, QWidget* parent = nullptr);

  QSize sizeHint() const override;
  // The rocket and the × alone: in a row short of room (the Docked
  // toolbar's tabs' row in a narrow window) the label is cut first, so
  // that the row never squeezes the tabs under the + after them.
  QSize minimumSizeHint() const override;

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
};

} // namespace mitcad
