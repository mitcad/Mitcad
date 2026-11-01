// SPDX-License-Identifier: MIT
// Test steps for card widths and windows within the screen (mitcad#818):
// expect-dialog-fits for a Qt dialog, and the native macOS steps
// native-card-width, expect-native-card-width, expect-native-name-fits,
// native-screen-limit and expect-native-window-fits (TestDriver.hpp); the
// left column's native-column-split and expect-native-card-height
// (mitcad#1020).
#include "TestDriver.hpp"

#include <algorithm>

#include <QAbstractButton>
#include <QDialogButtonBox>
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
  if (name == QLatin1String("fit-main-frame") || name == QLatin1String("expect-main-frame")) {
    int width = 0;
    int height = 0;
    if (mac::test::active() || !m_main || !toInt(a.at(0), width) || !toInt(a.at(1), height) ||
        width < 300 || height < 200 || width > 10000 || height > 10000) {
      fail(line, QStringLiteral("%1 requires a Qt main window and a frame of at least 300x200").arg(name));
      return true;
    }
    const QSize available(width, height);
    if (name == QLatin1String("fit-main-frame")) {
      // A process-local available-frame scenario: use this window's real
      // titlebar/border size, without changing the display or user settings.
      const QSize frame = m_main->frameGeometry().size() - m_main->size();
      m_main->resize(available - frame);
      m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
    } else {
      whenReady([this, available] {
        const QSize frame = m_main->frameGeometry().size();
        m_waitDetail = QStringLiteral("actual frame %1x%2, client %3x%4, minimum %5x%6, available %7x%8")
            .arg(frame.width()).arg(frame.height()).arg(m_main->width()).arg(m_main->height())
            .arg(m_main->minimumSizeHint().width()).arg(m_main->minimumSizeHint().height())
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
