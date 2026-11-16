// SPDX-License-Identifier: MIT
// The busy mark beside a command panel's OK and Cancel (mitcad#1907, GitHub
// mitcad/Mitcad#361), laid out as CommandPanel lays out its button row:
// quick work never shows it; slow work shows it after its delay, with its
// text as its accessible name and tooltip, and neither the panel's size nor
// OK's place changes; work that follows at once keeps it shown; it hides
// after its grace once the work has ended. On Qt's offscreen platform.
#include "UnitTests.hpp"
#include "framework/BusyMark.hpp"

#include <QCoreApplication>
#include <QElapsedTimer>
#include <QEvent>
#include <QEventLoop>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QRect>
#include <QSize>
#include <QString>
#include <QVBoxLayout>
#include <QWidget>

#include <cstdio>
#include <functional>

namespace {

// Runs the event loop for `ms`, or until `done` holds; whether it held.
bool runUntil(int ms, const std::function<bool()>& done) {
  QElapsedTimer clock;
  clock.start();
  while (clock.elapsed() < ms) {
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
    if (done && done()) {
      return true;
    }
  }
  return done ? done() : true;
}

} // namespace

int busyMarkTests() {
  using namespace mitcad;
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "busy mark: %s\n", message);
      ++failures;
    }
  };
  const QString text = QStringLiteral("Computing preview…");

  // A panel's rows, then the mark just before OK and Cancel.
  QWidget panel;
  auto* column = new QVBoxLayout(&panel);
  column->addWidget(new QLabel(QStringLiteral("Distance")));
  column->addWidget(new QLineEdit(QStringLiteral("20 mm")));
  auto* row = new QWidget;
  auto* rowBox = new QHBoxLayout(row);
  rowBox->setContentsMargins(0, 0, 0, 0);
  auto* mark = new BusyMark(row);
  auto* ok = new QPushButton(QStringLiteral("OK"));
  rowBox->addStretch(1);
  rowBox->addWidget(mark);
  rowBox->addWidget(ok);
  rowBox->addWidget(new QPushButton(QStringLiteral("Cancel")));
  column->addWidget(row);
  panel.show();
  runUntil(50, {});
  const QSize hint = panel.sizeHint();
  const QSize size = panel.size();
  const QRect okAt = ok->geometry();
  check(!mark->isVisible() && !mark->busy() && mark->busyMs() < 0, "no work: the mark is hidden");
  check(mark->width() > 0 && mark->width() == mark->height(), "the mark keeps a square room");

  int shownCount = 0;
  int hiddenCount = 0;
  qint64 shownAfter = -1;
  QObject::connect(mark, &BusyMark::shownChanged, mark, [&](bool shown, qint64 ms) {
    if (shown) {
      ++shownCount;
      shownAfter = ms;
    } else {
      ++hiddenCount;
    }
  });

  // Quick work, ended before the delay: never shown.
  mark->setBusy(text);
  check(mark->busy() && mark->busyMs() >= 0 && !mark->isVisible(), "work just started: not shown yet");
  mark->setBusy(QString());
  runUntil(BusyMark::kDelayMs + 150, {});
  check(shownCount == 0 && !mark->isVisible() && !mark->busy(), "quick work does not show the mark");

  // Slow work: shown after the delay, nothing moved.
  mark->setBusy(text);
  check(!mark->isVisible(), "slow work: not shown at once");
  check(runUntil(3000, [mark] { return mark->isVisible(); }), "slow work shows the mark");
  check(shownCount == 1 && mark->shown(), "shown once");
  // (A coarse timer may fire up to 5 % early.)
  check(shownAfter >= BusyMark::kDelayMs * 9 / 10, "shown only after its delay");
  check(mark->accessibleName() == text, "the work's text is its accessible name");
  check(mark->toolTip() == text, "and its tooltip");
  runUntil(50, {});
  check(panel.sizeHint() == hint && panel.size() == size, "the shown mark changes no size");
  check(ok->geometry() == okAt, "nor OK's place");

  // The work ends and new work follows at once (a drag's next preview):
  // still shown, not shown again.
  mark->setBusy(QString());
  check(mark->isVisible(), "shown during its grace");
  mark->setBusy(text);
  runUntil(BusyMark::kGraceMs + 150, {});
  check(mark->isVisible() && hiddenCount == 0 && shownCount == 1, "work that follows at once keeps it shown");

  // The end: hidden after its grace, nothing moved.
  mark->setBusy(QString());
  check(!mark->busy() && mark->busyMs() < 0, "no work once ended");
  check(runUntil(3000, [mark] { return !mark->isVisible(); }), "hidden once the work has ended");
  check(hiddenCount == 1 && !mark->shown(), "hidden once");
  runUntil(50, {});
  check(panel.sizeHint() == hint && panel.size() == size && ok->geometry() == okAt,
        "the hidden mark leaves the size as it was");
  return failures;
}
MITCAD_UNIT_TEST(busyMarkTests);
