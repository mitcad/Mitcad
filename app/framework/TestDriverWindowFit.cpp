// SPDX-License-Identifier: MIT
// Test steps for card widths and windows within the screen (mitcad#818):
// expect-dialog-fits for a Qt dialog, and the native macOS steps
// native-card-width, expect-native-card-width, expect-native-name-fits,
// native-screen-limit and expect-native-window-fits (TestDriver.hpp); the
// left column's native-column-split and expect-native-card-height
// (mitcad#1020); expect-widget-whole for a Qt widget shown whole
// (mitcad#1590); expect-window-place and expect-window-on-screen for the
// design window's kept place (mitcad#1710).
#include "TestDriver.hpp"

#include <algorithm>
#include <cstdlib>

#include <QAbstractButton>
#include <QDialogButtonBox>
#include <QFontMetrics>
#include <QGuiApplication>
#include <QPoint>
#include <QRect>
#include <QRegularExpression>
#include <QScreen>
#include <QScrollArea>
#include <QScrollBar>
#include <QSize>
#include <QWidget>
#include <QtLogging>

#include "../platform/NativeTestInput.hpp"

namespace mitcad {
namespace {

bool toInt(const QString& text, int& value) {
  bool ok = false;
  value = text.trimmed().toInt(&ok);
  return ok;
}

// A shown Qt window within its screen's available area (frame included) and
// at most `most` (when valid); its dialog buttons visible and outside any
// scroll area; its scroll areas able to scroll to the end of their contents.
bool dialogFits(QWidget* window, const QSize& most, QString& detail) {
  if (window == nullptr || !window->isVisible() || window->screen() == nullptr) {
    detail = QStringLiteral("no visible dialog");
    return false;
  }
  const QRect available = window->screen()->availableGeometry();
  const QRect frame = window->frameGeometry();
  detail = QStringLiteral("frame %1,%2 %3x%4 in the available %5,%6 %7x%8")
               .arg(frame.x()).arg(frame.y()).arg(frame.width()).arg(frame.height())
               .arg(available.x()).arg(available.y()).arg(available.width()).arg(available.height());
  if (!available.adjusted(-1, -1, 1, 1).contains(frame)) {
    return false;
  }
  if (most.isValid() && (window->width() > most.width() || window->height() > most.height())) {
    detail += QStringLiteral(", larger than %1x%2").arg(most.width()).arg(most.height());
    return false;
  }
  int buttons = 0;
  for (QDialogButtonBox* box : window->findChildren<QDialogButtonBox*>()) {
    if (!box->isVisible()) {
      continue;
    }
    for (QWidget* parent = box->parentWidget(); parent != nullptr && parent != window; parent = parent->parentWidget()) {
      if (qobject_cast<QScrollArea*>(parent) != nullptr) {
        detail += QStringLiteral(", its buttons scroll with its body");
        return false;
      }
    }
    for (QAbstractButton* button : box->buttons()) {
      if (!button->isVisible()) {
        continue;
      }
      const QRect rect(button->mapTo(window, QPoint(0, 0)), button->size());
      if (!window->rect().contains(rect) || button->visibleRegion().isEmpty()) {
        detail += QStringLiteral(", the button '%1' is clipped").arg(button->text());
        return false;
      }
      ++buttons;
    }
  }
  if (buttons == 0) {
    detail += QStringLiteral(", no visible dialog buttons");
    return false;
  }
  for (QScrollArea* area : window->findChildren<QScrollArea*>()) {
    const QWidget* inner = area->widget();
    if (!area->isVisible() || inner == nullptr) {
      continue;
    }
    const int hidden = inner->height() - area->viewport()->height();
    if (hidden > 1 && area->verticalScrollBar()->maximum() < hidden - 1) {
      detail += QStringLiteral(", %1 of '%2' cannot be scrolled into view").arg(hidden).arg(area->objectName());
      return false;
    }
  }
  return true;
}

// The one visible widget named `name` in `window` shown whole (mitcad#1590):
// as high as its text needs at its width (heightForWidth; without wrapped
// text, its minimum height) and cut by none of its parents up to the
// window, scroll areas' views included. In its own scroll area (it is the
// area's widget) it counts as whole when that area's view is whole, at least
// a line of the widget's text high, and scrolls to the widget's end.
bool widgetWhole(QWidget* window, const QString& name, QString& detail) {
  if (window == nullptr) {
    detail = QStringLiteral("no window");
    return false;
  }
  QWidget* widget = nullptr;
  for (QWidget* candidate : window->findChildren<QWidget*>(name)) {
    if (!candidate->isVisible()) {
      continue;
    }
    if (widget != nullptr) {
      detail = QStringLiteral("several visible widgets '%1'").arg(name);
      return false;
    }
    widget = candidate;
  }
  if (widget == nullptr) {
    detail = QStringLiteral("no visible widget '%1'").arg(name);
    return false;
  }
  detail = QStringLiteral("'%1' %2x%3").arg(name).arg(widget->width()).arg(widget->height());
  if (widget->hasHeightForWidth()) {
    const int needed = widget->heightForWidth(widget->width());
    detail += QStringLiteral(", its text %1 high").arg(needed);
    if (needed > widget->height()) {
      detail += QStringLiteral(": cut");
      return false;
    }
  } else if (widget->height() < widget->minimumSizeHint().height()) {
    // A squeezed button cuts its text.
    detail += QStringLiteral(", below its minimum height %1").arg(widget->minimumSizeHint().height());
    return false;
  }
  const int line = widget->fontMetrics().lineSpacing();
  int scrolled = 0;
  QWidget* current = widget;
  QRect rect = widget->rect();
  while (current != window && !current->isWindow()) {
    QWidget* parent = current->parentWidget();
    if (parent == nullptr) {
      break;
    }
    rect.translate(current->pos());
    auto* area = qobject_cast<QScrollArea*>(parent->parentWidget());
    if (area != nullptr && area->viewport() == parent && area->widget() == widget) {
      // The widget's own scroll area (a long text's): clipped by the view,
      // but reachable by scrolling. In any other scroll area it must be in
      // view.
      const int hidden = area->widget()->height() - parent->height();
      if (parent->height() < line || (hidden > 1 && area->verticalScrollBar()->maximum() < hidden - 1)) {
        detail += QStringLiteral(", its scroll area '%1' %2 high, %3 of it out of reach")
                      .arg(area->objectName()).arg(parent->height()).arg(std::max(0, hidden));
        return false;
      }
      ++scrolled;
      rect = parent->rect();
    } else if (!parent->rect().contains(rect)) {
      detail += QStringLiteral(", cut by '%1' (%2)")
                    .arg(parent->objectName(), QString::fromLatin1(parent->metaObject()->className()));
      return false;
    }
    current = parent;
  }
  if (scrolled > 0) {
    detail += QStringLiteral(", in %1 scroll area(s)").arg(scrolled);
  }
  return true;
}

} // namespace

bool TestDriver::runWindowFitStep(const QString& name, const QStringList& a, int line) {
  const auto regex = [&](const QString& pattern, QRegularExpression& re) {
    re = QRegularExpression(pattern);
    if (!re.isValid()) {
      fail(line, QStringLiteral("bad regular expression '%1': %2").arg(pattern, re.errorString()));
      return false;
    }
    return true;
  };
  if (name == QLatin1String("expect-window-place")) {
    // expect-window-place <x> <y> <w> <h>: the kept place restored
    // (mitcad#1710), as window-move and window-resize give it.
    int x = 0;
    int y = 0;
    int width = 0;
    int height = 0;
    if (mac::test::active() || !toInt(a.at(0), x) || !toInt(a.at(1), y) || !toInt(a.at(2), width) ||
        !toInt(a.at(3), height) || width < 1 || height < 1) {
      fail(line, QStringLiteral("expect-window-place <x> <y> <w> <h> requires a Qt main window"));
      return true;
    }
    whenReady([this, x, y, width, height] {
      const QWidget* top = topWindow();
      if (top == nullptr || !top->isVisible() || top->screen() == nullptr) {
        m_waitDetail = QStringLiteral("no shown main window");
        return false;
      }
      const QRect available = top->screen()->availableGeometry();
      const QPoint at = top->frameGeometry().topLeft() - available.topLeft();
      m_waitDetail = QStringLiteral("frame at %1, %2 from the available area's top left, its own area %3x%4")
                         .arg(at.x()).arg(at.y()).arg(top->width()).arg(top->height());
      const bool placed = std::abs(at.x() - x) <= 1 && std::abs(at.y() - y) <= 1 && std::abs(top->width() - width) <= 1 &&
                          std::abs(top->height() - height) <= 1;
      if (placed) {
        qInfo().noquote() << QStringLiteral("TestDriver: window place: %1").arg(m_waitDetail);
      }
      return placed;
    }, 6, QStringLiteral("the main window is not at %1, %2 with the size %3x%4").arg(x).arg(y).arg(width).arg(height));
    return true;
  }
  if (name == QLatin1String("expect-window-on-screen")) {
    if (mac::test::active()) {
      fail(line, QStringLiteral("expect-window-on-screen requires a Qt main window"));
      return true;
    }
    whenReady([this] {
      const QWidget* top = topWindow();
      const QScreen* primary = QGuiApplication::primaryScreen();
      if (top == nullptr || !top->isVisible() || primary == nullptr) {
        m_waitDetail = QStringLiteral("no shown main window");
        return false;
      }
      const QRect available = primary->availableGeometry();
      const QRect frame = top->frameGeometry();
      m_waitDetail = QStringLiteral("frame %1,%2 %3x%4 in the primary screen's available %5,%6 %7x%8")
                         .arg(frame.x()).arg(frame.y()).arg(frame.width()).arg(frame.height())
                         .arg(available.x()).arg(available.y()).arg(available.width()).arg(available.height());
      const bool inside = available.contains(frame);
      if (inside) {
        qInfo().noquote() << QStringLiteral("TestDriver: window on screen: %1").arg(m_waitDetail);
      }
      return inside;
    }, 6, QStringLiteral("the main window is not wholly within the primary screen's available area"));
    return true;
  }
  if (name == QLatin1String("fit-main-frame") || name == QLatin1String("expect-main-frame")) {
    int width = 0;
    int height = 0;
    // `least`: the height raised to the window's own minimum where its
    // layout cannot be that short (mitcad#1590: a dock's minimum on top of
    // the platform's bars).
    const bool least = a.size() > 2 && a.at(2) == QLatin1String("least");
    if (mac::test::active() || !m_main || !toInt(a.at(0), width) || !toInt(a.at(1), height) ||
        width < 300 || height < 200 || width > 10000 || height > 10000 || (a.size() > 2 && !least)) {
      fail(line, QStringLiteral("%1 <w> <h> [least] requires a Qt main window and a frame of at least 300x200")
                     .arg(name));
      return true;
    }
    // The frame asked for, with the window's frame margins and minimum.
    const auto wanted = [width, height, least](const QWidget* top) {
      if (!least) {
        return QSize(width, height);
      }
      const int margins = top->frameGeometry().height() - top->height();
      const int lowest = top->minimumSizeHint().expandedTo(top->minimumSize()).height() + margins;
      return QSize(width, std::max(height, lowest));
    };
    if (name == QLatin1String("fit-main-frame")) {
      // A process-local available-frame scenario: use this window's real
      // titlebar/border size, without changing the display or user settings.
      QWidget* top = topWindow();
      const QSize frame = top->frameGeometry().size() - top->size();
      top->resize(wanted(top) - frame);
      m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
    } else {
      whenReady([this, wanted] {
        const QWidget* top = topWindow();
        if (top == nullptr) return false;
        const QSize available = wanted(top);
        const QSize frame = top->frameGeometry().size();
        m_waitDetail = QStringLiteral("actual frame %1x%2, client %3x%4, minimum %5x%6, available %7x%8")
            .arg(frame.width()).arg(frame.height()).arg(top->width()).arg(top->height())
            .arg(top->minimumSizeHint().width()).arg(top->minimumSizeHint().height())
            .arg(available.width()).arg(available.height());
        qInfo().noquote() << QStringLiteral("Compact main frame: %1").arg(m_waitDetail);
        return frame.width() <= available.width() && frame.height() <= available.height();
      }, 6, QStringLiteral("the main window does not fit the requested available frame"));
    }
    return true;
  }
  if (name == QLatin1String("expect-dialog-fits")) {
    QSize most;
    int width = 0;
    int height = 0;
    if (a.size() == 1 || (a.size() == 2 && (!toInt(a.at(0), width) || !toInt(a.at(1), height)))) {
      fail(line, QStringLiteral("expect-dialog-fits [<width> <height>]"));
      return true;
    }
    if (a.size() == 2) {
      most = QSize(width, height);
    }
    whenReady([this, most] {
      QString detail;
      const bool fits = dialogFits(targetWindow(), most, detail);
      m_waitDetail = detail;
      if (fits) {
        qDebug().noquote() << QStringLiteral("TestDriver: dialog fits: %1").arg(detail);
      }
      return fits;
    }, 6, QStringLiteral("the dialog is not within the screen with its buttons visible"));
    return true;
  }
  if (name == QLatin1String("expect-widget-whole")) {
    const QString widgetName = a.at(0);
    whenReady([this, widgetName] {
      QString detail;
      const bool whole = widgetWhole(targetWindow(), widgetName, detail);
      m_waitDetail = detail;
      if (whole) {
        qInfo().noquote() << QStringLiteral("TestDriver: widget whole: %1").arg(detail);
      }
      return whole;
    }, 6, QStringLiteral("the widget is not shown whole"));
    return true;
  }
  if (name == QLatin1String("native-card-width") || name == QLatin1String("expect-native-card-width") ||
      name == QLatin1String("expect-native-name-fits") || name == QLatin1String("native-screen-limit") ||
      name == QLatin1String("expect-native-window-fits") || name == QLatin1String("native-column-split") ||
      name == QLatin1String("expect-native-card-height")) {
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return true;
    }
  } else {
    return false;
  }
  if (name == QLatin1String("native-column-split")) {
    // native-column-split <handle> <height of the card above>|reset
    int height = -1;
    if (a.at(1) != QLatin1String("reset") && (!toInt(a.at(1), height) || height < 1 || height > 10000)) {
      fail(line, QStringLiteral("native-column-split <handle> <height 1..10000>|reset"));
      return true;
    }
    const QString handle = a.at(0);
    queue([this, handle, height, line] {
      QString error;
      if (!mac::test::dragColumnSplit(handle, height, error)) {
        fail(line, error);
      }
    });
    m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
  } else if (name == QLatin1String("expect-native-card-height")) {
    // expect-native-card-height <card> <minimum> [maximum] | <card> bottom
    int minimum = -1;
    int maximum = 0;
    if (a.at(1) != QLatin1String("bottom") &&
        (!toInt(a.at(1), minimum) || minimum < 0 || (a.size() > 2 && !toInt(a.at(2), maximum)))) {
      fail(line, QStringLiteral("expect-native-card-height <card> <minimum> [maximum] | <card> bottom"));
      return true;
    }
    if (a.size() < 3) {
      maximum = minimum;
    }
    const QString card = a.at(0);
    whenReady([this, card, minimum, maximum] {
      QString detail;
      const bool ok = mac::test::cardHeight(card, minimum, maximum, detail);
      m_waitDetail = detail;
      if (ok) {
        qInfo().noquote() << QStringLiteral("TestDriver: %1").arg(detail);
      }
      return ok;
    }, 6, QStringLiteral("native card '%1' is not %2").arg(card, a.mid(1).join(QLatin1Char(' '))));
  } else if (name == QLatin1String("native-card-width")) {
    int width = 0;
    if (!toInt(a.at(1), width) || width < 1 || width > 10000) {
      fail(line, QStringLiteral("native-card-width <card> <width 1..10000>"));
      return true;
    }
    const QString card = a.at(0);
    queue([this, card, width, line] {
      QString error;
      if (!mac::test::dragCardWidth(card, width, error)) {
        fail(line, error);
      }
    });
    m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
  } else if (name == QLatin1String("expect-native-card-width")) {
    int minimum = 0;
    int maximum = 0;
    if (!toInt(a.at(1), minimum) || (a.size() > 2 && !toInt(a.at(2), maximum))) {
      fail(line, QStringLiteral("expect-native-card-width <card> <minimum> [maximum]"));
      return true;
    }
    if (a.size() < 3) {
      maximum = minimum;
    }
    const QString card = a.at(0);
    whenReady([this, card, minimum, maximum] {
      QString detail;
      const bool ok = mac::test::cardWidth(card, minimum, maximum, detail);
      m_waitDetail = detail;
      return ok;
    }, 6, QStringLiteral("native card '%1' is not %2..%3 points wide").arg(card).arg(minimum).arg(maximum));
  } else if (name == QLatin1String("expect-native-name-fits")) {
    QRegularExpression re;
    if (!regex(a.at(0), re)) {
      return true;
    }
    if (a.at(1) != QLatin1String("true") && a.at(1) != QLatin1String("false")) {
      fail(line, QStringLiteral("expect-native-name-fits <regex> true|false"));
      return true;
    }
    const bool expected = a.at(1) == QLatin1String("true");
    whenReady([this, re, expected] {
      QString detail;
      const bool ok = mac::test::nameFits(re, expected, detail);
      m_waitDetail = detail;
      return ok;
    }, 6, QStringLiteral("native text '%1' is not %2").arg(a.at(0), expected ? QStringLiteral("shown whole")
                                                                                 : QStringLiteral("elided")));
  } else if (name == QLatin1String("native-screen-limit")) {
    int width = 0;
    int height = 0;
    if (!toInt(a.at(0), width) || !toInt(a.at(1), height)) {
      fail(line, QStringLiteral("native-screen-limit <width> <height> (0 0: none)"));
      return true;
    }
    QString error;
    if (!mac::test::setScreenLimit(width, height, error)) {
      fail(line, error);
    }
    m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
  } else {
    QRegularExpression re;
    if (!regex(a.at(0), re)) {
      return true;
    }
    whenReady([this, re] {
      QString detail;
      const bool ok = mac::test::windowFits(re, detail);
      m_waitDetail = detail;
      return ok;
    }, 6, QStringLiteral("native window '%1' is not within the screen with its buttons visible").arg(a.at(0)));
  }
  return true;
}

} // namespace mitcad
