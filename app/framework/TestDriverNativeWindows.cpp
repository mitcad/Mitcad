// SPDX-License-Identifier: MIT
// Test steps for the design window's native windows (mitcad#2152; see
// TestDriver.hpp): expect-design-native, whether the 3D view or one of its
// parents below the top-level window has a native window of its own, and
// expect-screen-beside-cards, the screen as the system shows it just beside
// the Floating cards. A design window made native before it sat in its
// frame (the 3D mouse's driver asking for its winId(), mitcad#2059 on
// Linux, mitcad#2152 on Windows) painted the cards' translucent margins
// black: the screen beside a card was black, not the view's background.
#include "TestDriver.hpp"

#include <algorithm>
#include <array>
#include <cstdlib>

#include <QColor>
#include <QImage>
#include <QOpenGLWidget>
#include <QPixmap>
#include <QPoint>
#include <QRect>
#include <QScreen>
#include <QStringList>
#include <QWidget>
#include <QtLogging>

namespace mitcad {
namespace {

using Rgb = std::array<int, 3>;

// The mean colour of the 4 x 4 device pixels at `at` (device pixels) of
// the image; false when they are not all in it.
bool meanAt(const QImage& image, const QPoint& at, Rgb& mean) {
  if (!image.rect().contains(QRect(at, QSize(4, 4)))) {
    return false;
  }
  Rgb sum{};
  for (int y = at.y(); y < at.y() + 4; ++y) {
    for (int x = at.x(); x < at.x() + 4; ++x) {
      const QRgb pixel = image.pixel(x, y);
      sum[0] += qRed(pixel);
      sum[1] += qGreen(pixel);
      sum[2] += qBlue(pixel);
    }
  }
  for (int& channel : sum) {
    channel /= 16;
  }
  mean = sum;
  return true;
}

int difference(const Rgb& a, const Rgb& b) {
  return std::abs(a[0] - b[0]) + std::abs(a[1] - b[1]) + std::abs(a[2] - b[2]);
}

QString rgbText(const Rgb& color) {
  return QStringLiteral("%1 %2 %3").arg(color[0]).arg(color[1]).arg(color[2]);
}

QPoint toDevice(const QPoint& logical, qreal ratio) {
  return QPoint(qRound(logical.x() * ratio), qRound(logical.y() * ratio));
}

// Whether the image has one colour all over (a screen that shows no window).
bool uniform(const QImage& image) {
  if (image.isNull()) {
    return true;
  }
  const QRgb first = image.pixel(0, 0);
  const int stepX = std::max(1, image.width() / 64);
  const int stepY = std::max(1, image.height() / 64);
  for (int y = 0; y < image.height(); y += stepY) {
    for (int x = 0; x < image.width(); x += stepX) {
      if (image.pixel(x, y) != first) {
        return false;
      }
    }
  }
  return true;
}

} // namespace

bool TestDriver::runNativeWindowStep(const QString& name, const QStringList& a, int line) {
  if (name == QLatin1String("expect-design-native")) {
    // expect-design-native yes|no: the 3D view and its parents up to the
    // top-level window (the document frame), the top level not counted.
    // Logged "Design native windows: none" or the classes and names of
    // those that have one.
    const QString wanted = a.at(0);
    if (wanted != QLatin1String("yes") && wanted != QLatin1String("no")) {
      fail(line, QStringLiteral("expect-design-native yes|no, not '%1'").arg(wanted));
      return true;
    }
    QWidget* view = viewer();
    if (view == nullptr) {
      fail(line, QStringLiteral("expect-design-native needs the 3D view"));
      return true;
    }
    QStringList native;
    for (QWidget* widget = view; widget != nullptr && !widget->isWindow(); widget = widget->parentWidget()) {
      if (widget->internalWinId() != 0) {
        const QString object = widget->objectName();
        native << (object.isEmpty() ? QString::fromLatin1(widget->metaObject()->className())
                                    : QStringLiteral("%1 %2").arg(QString::fromLatin1(widget->metaObject()->className()),
                                                                  object));
      }
    }
    const QString found = native.isEmpty() ? QStringLiteral("none") : native.join(QStringLiteral(", "));
    qDebug().noquote() << QStringLiteral("Design native windows: %1").arg(found);
    if (native.isEmpty() == (wanted == QLatin1String("yes"))) {
      fail(line, wanted == QLatin1String("yes")
                     ? QStringLiteral("the design window has no native window of its own")
                     : QStringLiteral("the design window has native windows of its own: %1").arg(found));
    }
    return true;
  }
  if (name == QLatin1String("expect-screen-beside-cards")) {
    // expect-screen-beside-cards <card>...: the screen as the system shows
    // it (QScreen::grabWindow of the screen, not Qt's own picture of the
    // window), 6 px right of each named Floating card at its middle, has
    // the colour it has 60 px further right, over the view (the view's
    // background): no dark frame round the card (as
    // tools/ui-space-mouse-no-daemon-test.sh on Linux). Where the system
    // draws no window (SSH's session 0 on Windows: Qt never exposes the
    // window there), the screen is all one colour or does not show the
    // view's own frame: logged "Screen beside cards: not drawn here, not
    // checked", not a failure. A card that is not shown is noted. At least
    // two cards are checked when the screen is drawn.
    auto* view = qobject_cast<QOpenGLWidget*>(viewer());
    QWidget* top = view != nullptr ? view->window() : nullptr;
    QScreen* screen = top != nullptr ? top->screen() : nullptr;
    if (view == nullptr || screen == nullptr || !top->isVisible()) {
      fail(line, QStringLiteral("expect-screen-beside-cards needs a shown window with the 3D view"));
      return true;
    }
    const QPoint origin = top->mapToGlobal(QPoint(0, 0));
    const QPoint inScreen = origin - screen->geometry().topLeft();
    const QPixmap grabbed = screen->grabWindow(0, inScreen.x(), inScreen.y(), top->width(), top->height());
    const QImage shot = grabbed.toImage().convertToFormat(QImage::Format_RGB32);
    const qreal shotRatio = grabbed.devicePixelRatio();
    const QImage frame = view->grabFramebuffer().convertToFormat(QImage::Format_RGB32);
    const qreal frameRatio = frame.devicePixelRatio();
    bool drawn = !uniform(shot);
    int checked = 0;
    QStringList failures;
    for (const QString& cardName : a) {
      QWidget* card = nullptr;
      const QList<QWidget*> matches = top->findChildren<QWidget*>(cardName);
      for (QWidget* candidate : matches) {
        if (candidate->isVisible()) {
          card = candidate;
          break;
        }
      }
      if (card == nullptr) {
        qDebug().noquote() << QStringLiteral("Screen beside cards: %1 is not shown").arg(cardName);
        continue;
      }
      const QRect area(card->mapToGlobal(QPoint(0, 0)) - origin, card->size());
      const QPoint besideAt(area.right() + 1 + 6, area.center().y() - 2);
      const QPoint viewAt(besideAt.x() + 60, besideAt.y());
      Rgb beside{};
      Rgb background{};
      Rgb ownFrame{};
      if (!meanAt(shot, toDevice(besideAt, shotRatio), beside) ||
          !meanAt(shot, toDevice(viewAt, shotRatio), background) ||
          !meanAt(frame, toDevice(view->mapFrom(top, viewAt), frameRatio), ownFrame)) {
        qDebug().noquote() << QStringLiteral("Screen beside cards: no view beside %1").arg(cardName);
        continue;
      }
      // The screen shows the view's own frame there, or it draws nothing.
      if (difference(background, ownFrame) > 60) {
        drawn = false;
      }
      if (!drawn) {
        break;
      }
      const int off = difference(beside, background);
      const bool dark = *std::max_element(beside.begin(), beside.end()) < 40;
      if (off > 60 || dark) {
        failures << QStringLiteral("beside %1 the screen is %2, the view's background %3: a dark frame round the card")
                        .arg(cardName, rgbText(beside), rgbText(background));
      } else {
        qDebug().noquote() << QStringLiteral("Screen beside cards: beside %1 the view's background (%2, %3)")
                                  .arg(cardName, rgbText(beside), rgbText(background));
      }
      ++checked;
    }
    if (!drawn) {
      qDebug().noquote() << QStringLiteral("Screen beside cards: not drawn here, not checked");
      return true;
    }
    if (!failures.isEmpty()) {
      fail(line, failures.join(QStringLiteral("; ")));
    } else if (checked < 2) {
      fail(line, QStringLiteral("fewer than two Floating cards to check beside (%1)").arg(checked));
    }
    return true;
  }
  return false;
}

// This file's steps (mitcad#2173): their specs, run by its run function before the others.
namespace {

const TestDriver::StepSpec kStepSpecs[] = {
    // The design window's native windows and the screen beside the
    // Floating cards (mitcad#2152, TestDriverNativeWindows.cpp).
    {"expect-design-native", 1, 1, TestDriver::kIdle},
    {"expect-screen-beside-cards", 1, 3, TestDriver::kIdle},
};

} // namespace

const TestDriver::StepArea TestDriver::kNativeWindowSteps{
    kStepSpecs, std::size(kStepSpecs), nullptr};

} // namespace mitcad
