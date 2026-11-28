// SPDX-License-Identifier: MIT
// Send Feedback on a Region (mitcad#2616): the region a drag picks, in
// either direction and clipped to the screens, a click without a drag that
// cancels, and the overlay's input: the mouse's release takes the region,
// the command's key held through the drag takes it on its release, Esc and
// another button cancel, and every key is the overlay's while it is up. On
// Qt's offscreen platform.
#include "UnitTests.hpp"
#include "report/FeedbackRegion.hpp"
#include "report/RegionPicker.hpp"

#include <QCoreApplication>
#include <QKeyEvent>
#include <QList>
#include <QMouseEvent>
#include <QPoint>
#include <QRect>
#include <QWidget>

#include <cstdio>

namespace {

struct Outcome {
  QRect picked;
  int pickedCount = 0;
  int canceledCount = 0;
};

void watch(mitcad::report::RegionPicker& picker, Outcome& outcome) {
  QObject::connect(&picker, &mitcad::report::RegionPicker::picked, [&outcome](const QRect& region) {
    outcome.picked = region;
    ++outcome.pickedCount;
  });
  QObject::connect(&picker, &mitcad::report::RegionPicker::canceled, [&outcome] { ++outcome.canceledCount; });
}

void sendMouse(QWidget* window, QEvent::Type type, const QPoint& global, Qt::MouseButton button,
               Qt::MouseButtons buttons) {
  QMouseEvent event(type, QPointF(window->mapFromGlobal(global)), QPointF(window->mapFromGlobal(global)),
                    QPointF(global), button, buttons, Qt::NoModifier);
  QCoreApplication::sendEvent(window, &event);
}

} // namespace

int feedbackRegionTests() {
  using namespace mitcad::report;
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "feedback region: %s\n", message);
      ++failures;
    }
  };

  // Two screens side by side, the right one shorter.
  const QList<QRect> screens = {QRect(0, 0, 1000, 800), QRect(1000, 0, 800, 600)};

  // A drag in either direction: the rectangle between its ends.
  check(regionFromDrag(QPoint(100, 120), QPoint(400, 320), screens) == QRect(100, 120, 300, 200),
        "a drag down and right");
  check(regionFromDrag(QPoint(400, 320), QPoint(100, 120), screens) == QRect(100, 120, 300, 200),
        "a drag up and left");
  check(regionFromDrag(QPoint(400, 120), QPoint(100, 320), screens) == QRect(100, 120, 300, 200),
        "a drag down and left");
  // Clipped to the screens: beyond the left edge, and below the shorter
  // screen across both.
  check(regionFromDrag(QPoint(-50, 100), QPoint(200, 300), screens) == QRect(0, 100, 200, 200),
        "clipped at the screens' left edge");
  check(regionFromDrag(QPoint(900, 500), QPoint(1100, 700), screens) == QRect(900, 500, 200, 200),
        "across two screens: the bounding box of the parts");
  check(regionFromDrag(QPoint(1500, 650), QPoint(1700, 750), screens).isNull(),
        "a drag below the shorter screen only, on no screen");
  // A click, or a drag of a few pixels, cancels.
  check(regionFromDrag(QPoint(300, 300), QPoint(300, 300), screens).isNull(), "a click picks nothing");
  check(regionFromDrag(QPoint(300, 300), QPoint(305, 400), screens).isNull(), "a narrow drag picks nothing");
  check(regionFromDrag(QPoint(-100, 300), QPoint(4, 400), screens).isNull(),
        "a drag that leaves less than the least side on the screens picks nothing");
  // The parts on each screen.
  const QList<QRect> parts = regionParts(QRect(900, 500, 200, 200), screens);
  check(parts.size() == 2 && parts.at(0) == QRect(900, 500, 100, 200) && parts.at(1) == QRect(1000, 500, 100, 100),
        "the region's part on each screen");
  check(regionParts(QRect(10, 10, 20, 20), screens).at(1).isNull(), "no part on a screen it does not touch");

  // The overlay's input. The area: a window over both screens.
  const QRect area(200, 100, 1200, 600);
  {
    RegionPicker picker(area, screens, Qt::Key_F9, nullptr);
    check(picker.parts() == QList<QRect>({QRect(200, 100, 800, 600), QRect(1000, 100, 400, 500)}),
          "one window per screen's part of the area");
    Outcome outcome;
    watch(picker, outcome);
    // Pressed once: the key released before the drag leaves it going.
    picker.keyRelease(Qt::Key_F9, false);
    picker.press(QPoint(900, 300), Qt::LeftButton);
    picker.move(QPoint(1100, 450));
    check(picker.dragging() && picker.dragged() == QRect(900, 300, 201, 151), "the dragged rectangle shows");
    picker.release(QPoint(1100, 450), Qt::LeftButton);
    check(outcome.pickedCount == 1 && outcome.picked == QRect(900, 300, 200, 150),
          "the mouse's release takes the region");
    check(picker.done() && picker.dragged().isNull(), "done after the region");
    picker.keyPress(Qt::Key_Escape);
    check(outcome.canceledCount == 0 && outcome.pickedCount == 1, "nothing more once done");
  }
  {
    // Held: the key's release during the drag takes the region; its
    // repeats do not.
    RegionPicker picker(area, screens, Qt::Key_F9, nullptr);
    Outcome outcome;
    watch(picker, outcome);
    picker.press(QPoint(600, 500), Qt::LeftButton);
    picker.move(QPoint(300, 200));
    picker.keyRelease(Qt::Key_F9, true);
    check(outcome.pickedCount == 0 && picker.dragging(), "a repeat of the held key does not take it");
    picker.keyRelease(Qt::Key_F10, false);
    check(outcome.pickedCount == 0, "another key's release does not take it");
    picker.keyRelease(Qt::Key_F9, false);
    check(outcome.pickedCount == 1 && outcome.picked == QRect(300, 200, 300, 300),
          "the held key's release takes the region dragged up and left");
    picker.release(QPoint(300, 200), Qt::LeftButton);
    check(outcome.pickedCount == 1 && outcome.canceledCount == 0, "the mouse's release after it does nothing");
  }
  {
    // A click without a drag cancels.
    RegionPicker picker(area, screens, Qt::Key_F9, nullptr);
    Outcome outcome;
    watch(picker, outcome);
    picker.press(QPoint(500, 400), Qt::LeftButton);
    picker.release(QPoint(502, 401), Qt::LeftButton);
    check(outcome.canceledCount == 1 && outcome.pickedCount == 0, "a click cancels");
  }
  {
    // Esc cancels, during a drag too.
    RegionPicker picker(area, screens, Qt::Key_F9, nullptr);
    Outcome outcome;
    watch(picker, outcome);
    picker.press(QPoint(500, 400), Qt::LeftButton);
    picker.move(QPoint(700, 600));
    picker.keyPress(Qt::Key_Escape);
    check(outcome.canceledCount == 1 && outcome.pickedCount == 0, "Esc cancels");
    picker.release(QPoint(700, 600), Qt::LeftButton);
    check(outcome.pickedCount == 0, "the release after Esc takes nothing");
  }
  {
    // Another button cancels.
    RegionPicker picker(area, screens, Qt::Key_F9, nullptr);
    Outcome outcome;
    watch(picker, outcome);
    picker.press(QPoint(500, 400), Qt::RightButton);
    check(outcome.canceledCount == 1, "the right button cancels");
  }
  {
    // Through its windows: they hold the mouse and the keys, take every
    // shortcut, and pass the drag on.
    QWidget owner;
    owner.setGeometry(area);
    RegionPicker picker(area, screens, Qt::Key_F9, &owner);
    Outcome outcome;
    watch(picker, outcome);
    check(picker.start(), "the overlay shows");
    const QList<QWidget*> windows = picker.windows();
    check(windows.size() == 2 && windows.at(0)->isVisible() && windows.at(1)->isVisible(), "both windows show");
    QWidget* grabber = QWidget::mouseGrabber();
    check(grabber != nullptr && windows.contains(grabber) && QWidget::keyboardGrabber() == grabber,
          "a window of the overlay holds the mouse and the keys");
    if (grabber != nullptr) {
      QKeyEvent offer(QEvent::ShortcutOverride, Qt::Key_F6, Qt::NoModifier);
      offer.ignore();
      QCoreApplication::sendEvent(grabber, &offer);
      check(offer.isAccepted(), "no shortcut under the overlay");
      sendMouse(grabber, QEvent::MouseButtonPress, QPoint(250, 150), Qt::LeftButton, Qt::LeftButton);
      sendMouse(grabber, QEvent::MouseMove, QPoint(450, 300), Qt::NoButton, Qt::LeftButton);
      sendMouse(grabber, QEvent::MouseButtonRelease, QPoint(450, 300), Qt::LeftButton, Qt::NoButton);
    }
    check(outcome.pickedCount == 1 && outcome.picked == QRect(250, 150, 200, 150), "the drag on the overlay");
    check(windows.size() == 2 && !windows.at(0)->isVisible() && !windows.at(1)->isVisible(),
          "the overlay hides once the region is taken");
    check(QWidget::mouseGrabber() == nullptr && QWidget::keyboardGrabber() == nullptr,
          "the mouse and the keys are let go");
  }
  {
    // An area on no screen shows nothing.
    RegionPicker picker(QRect(5000, 5000, 100, 100), screens, Qt::Key_F9, nullptr);
    check(!picker.start() && picker.windows().isEmpty(), "no overlay off the screens");
  }
  return failures;
}
MITCAD_UNIT_TEST(feedbackRegionTests);
