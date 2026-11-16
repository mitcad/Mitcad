// SPDX-License-Identifier: MIT
// A scroll area as tall as its contents and a window sized to them within
// the screen (mitcad#631), on Qt's offscreen platform (no window).
#include "UnitTests.hpp"
#include "framework/FitToScreen.hpp"

#include <QDialog>
#include <QFontMetrics>
#include <QFrame>
#include <QGuiApplication>
#include <QLabel>
#include <QLayout>
#include <QMargins>
#include <QObject>
#include <QPoint>
#include <QRect>
#include <QScreen>
#include <QScrollArea>
#include <QSize>
#include <QString>
#include <QVBoxLayout>
#include <QWidget>

#include <algorithm>
#include <cstdio>
#include <memory>

namespace {

// A form of `lines` one-line labels in a scroll area of the dialog's kind.
mitcad::FittingScrollArea* makeArea(QWidget* parent, int lines, QWidget** contents = nullptr) {
  auto* area = new mitcad::FittingScrollArea;
  area->setWidgetResizable(true);
  area->setFrameShape(QFrame::NoFrame);
  auto* inner = new QWidget;
  auto* column = new QVBoxLayout(inner);
  for (int i = 0; i < lines; ++i) {
    column->addWidget(new QLabel(QStringLiteral("Line %1").arg(i + 1)));
  }
  area->setWidget(inner);
  if (parent != nullptr) {
    auto* outer = new QVBoxLayout(parent);
    outer->addWidget(area);
  }
  if (contents != nullptr) {
    *contents = inner;
  }
  return area;
}

} // namespace

int fitToScreenTests() {
  using namespace mitcad;
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "fit to screen: %s\n", message);
      ++failures;
    }
  };

  // The size of a window on a screen: the hint, at most what the screen
  // has left after the window frame.
  const QSize screen(1920, 1040);
  const QSize frame(16, 40);
  check(fittedWindowSize(QSize(700, 900), screen, frame) == QSize(700, 900), "a hint that fits is kept");
  check(fittedWindowSize(QSize(700, 1500), screen, frame) == QSize(700, 1000),
        "a tall hint is capped at the screen's height minus the frame");
  check(fittedWindowSize(QSize(3000, 100), screen, frame) == QSize(1904, 100),
        "a wide hint is capped at the screen's width minus the frame");
  check(fittedWindowSize(QSize(700, 900), QSize(10, 20), frame) == QSize(0, 0),
        "a frame larger than the screen leaves nothing, never a negative size");

  // The hint of a scroll area is its contents' size plus its frame, where
  // QScrollArea's own stops at 24 text lines.
  QWidget* contents = nullptr;
  std::unique_ptr<FittingScrollArea> tall(makeArea(nullptr, 80, &contents));
  const int framePixels = 2 * tall->frameWidth();
  const QSize wanted = contents->sizeHint();
  const QSize hint = tall->sizeHint();
  check(hint.height() >= wanted.height() + framePixels, "the hint is as tall as the contents and the frame");
  check(hint.width() >= wanted.width() + framePixels, "the hint is as wide as the contents and the frame");
  check(tall->QScrollArea::sizeHint().height() < hint.height(), "the plain QScrollArea's hint is the capped one");

  std::unique_ptr<FittingScrollArea> low(makeArea(nullptr, 3, &contents));
  check(low->sizeHint().height() == contents->sizeHint().height() + 2 * low->frameWidth(),
        "a short form's hint is exactly its contents");

  // Wrapped text fits at the width the area actually requests. Fonts/styles
  // may make its preferred width larger than the explicit minimum.
  auto* wrapped = new QLabel(QStringLiteral("A long note that wraps over several lines when it has only a narrow "
                                            "column to be laid out in, as the notes of a form do. A long note "
                                            "that wraps over several lines when it has only a narrow column."));
  wrapped->setWordWrap(true);
  std::unique_ptr<FittingScrollArea> notes(makeArea(nullptr, 2, &contents));
  contents->layout()->addWidget(wrapped);
  const int preferredWidth = notes->sizeHint().width();
  const int notesFrame = 2 * notes->frameWidth();
  const QMargins margins = contents->layout()->contentsMargins();
  // Exercise both the preferred-width and minimum-width branches without
  // depending on a platform's font metrics or layout margins.
  const int minimumWidths[]{preferredWidth / 2, preferredWidth + preferredWidth / 4 + 1};
  for (const int minimumWidth : minimumWidths) {
    notes->setMinimumWidth(minimumWidth);
    const QSize notesHint = notes->sizeHint().expandedTo(notes->minimumSize());
    check(notesHint.width() == std::max(preferredWidth, minimumWidth),
          "the effective hint respects the larger of the preferred and minimum widths");
    const int contentWidth = notesHint.width() - notesFrame;
    const int labelWidth = contentWidth - margins.left() - margins.right();
    check(labelWidth > 0 && wrapped->heightForWidth(labelWidth) > wrapped->fontMetrics().height(),
          "the note spans multiple lines at the requested width");
    check(notesHint.height() >= contents->layout()->totalHeightForWidth(contentWidth) + notesFrame,
          "the hint holds the wrapped text at the requested width");
  }

  // A window with a form taller than the screen fits the screen, a window
  // with a short one gets its hint.
  const QScreen* primary = QGuiApplication::primaryScreen();
  check(primary != nullptr, "there is a screen");
  if (primary != nullptr) {
    const QRect available = primary->availableGeometry();
    QDialog high;
    makeArea(&high, 400);
    fitWindowToContents(&high);
    check(high.height() <= available.height(), "a window taller than the screen is capped at its height");
    check(high.height() >= available.height() - 200, "a window taller than the screen takes what it has");
    check(high.width() <= available.width(), "a window never wider than the screen");

    QDialog small;
    makeArea(&small, 3);
    fitWindowToContents(&small);
    check(small.size() == small.sizeHint(), "a window that fits the screen gets its hint");
    // A window larger than its hint stays so with GrowOnly, and Fit takes
    // it back to the hint.
    small.resize(small.width() + 30, small.height() + 30);
    const QSize bigger = small.size();
    fitWindowToContents(&small, FitMode::GrowOnly);
    check(small.size() == bigger, "GrowOnly does not shrink");
    fitWindowToContents(&small);
    check(small.size() == small.sizeHint(), "Fit shrinks to the hint");

    // A preferred size opens a window roomier than its hint where the
    // screen has room, and never beyond the screen (mitcad#818).
    QDialog roomy;
    makeArea(&roomy, 3);
    const QSize preferred = roomy.sizeHint() + QSize(40, 40);
    fitWindowToContents(&roomy, FitMode::Fit, preferred);
    check(roomy.size() == preferred, "a preferred size larger than the hint is taken where it fits");
    fitWindowToContents(&roomy, FitMode::Fit, available.size() * 2);
    check(roomy.width() <= available.width() && roomy.height() <= available.height(),
          "a preferred size larger than the screen is capped at it");

    // A shown window larger than its screen is made to fit it and moved
    // inside its available area; one that fits keeps its size.
    QDialog shown;
    makeArea(&shown, 3);
    shown.show();
    shown.resize(available.size() + QSize(300, 300));
    shown.move(available.topLeft() + QPoint(200, 200));
    capWindowToScreen(&shown);
    check(shown.width() <= available.width() && shown.height() <= available.height(),
          "a shown window larger than the screen is capped at it");
    check(available.contains(shown.frameGeometry()), "a capped window is moved inside the available area");
    const QSize fitting = shown.sizeHint() + QSize(20, 20);
    shown.resize(fitting);
    capWindowToScreen(&shown);
    check(shown.size() == fitting, "a window that fits the screen keeps its size");
    keepWindowOnScreen(&shown);
    keepWindowOnScreen(&shown);
    check(shown.findChildren<QObject*>(QStringLiteral("mitcadKeptOnScreen"), Qt::FindDirectChildrenOnly).size() == 1,
          "a window is watched for screen changes once");
    shown.hide();
  }
  return failures;
}
MITCAD_UNIT_TEST(fitToScreenTests);
