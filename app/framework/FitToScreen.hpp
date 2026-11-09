// SPDX-License-Identifier: MIT
#pragma once

#include <QScrollArea>
#include <QSize>

class QEvent;
class QObject;
class QWidget;

namespace mitcad {

// A QScrollArea whose size hint is as tall (and wide) as its contents need.
// QScrollArea's own hint stops at 36 x 24 text lines, so a dialog that holds
// its form in a scroll area opens at that size however much the form needs
// and scrolls although the screen has room. With this one the dialog's
// hint is the whole form; the scroll bar remains for a screen too low for
// it (see fitWindowToContents). Needs widgetResizable for the contents'
// size hint to count; the minimum size set on it is respected.
class FittingScrollArea : public QScrollArea {
public:
  explicit FittingScrollArea(QWidget* parent = nullptr) : QScrollArea(parent) {}
  QSize sizeHint() const override;

protected:
  // A layout change in the contents (a field shown or hidden) makes the
  // area's hint, which the dialog's layout caches, stale.
  bool eventFilter(QObject* watched, QEvent* event) override;
};

// The size of a window whose contents ask for `hint`, on a screen whose
// available area is `available` (the screen without the task bar and the
// like), for a window frame (title bar and borders) of `frame` around the
// window's own area: the hint, at most what is left of the screen.
QSize fittedWindowSize(const QSize& hint, const QSize& available, const QSize& frame);

enum class FitMode {
  Fit,      // the window gets exactly its fitted size (also smaller)
  GrowOnly  // the window only grows to it: a change while the user is typing
};

// Sizes a top-level window (a dialog) to its size hint, capped by the
// screen's available size minus the window frame (QWidget::adjustSize caps
// at two thirds of the screen instead), and moves it up or left when it
// would stick out of the screen. Before the window is shown its frame is
// not known: room is kept for a typical one, and the next call, from
// showEvent, takes the real one. A valid `preferred` size is a roomier one
// the window opens at where the screen has room for it (the hint expanded
// to it), still within the screen.
void fitWindowToContents(QWidget* window, FitMode mode = FitMode::Fit, const QSize& preferred = QSize());

// Makes a shown top-level window no larger than its screen's available
// area minus the window frame, and moves it inside that area (mitcad#818);
// a window that fits keeps its size and place. Never makes it larger.
void capWindowToScreen(QWidget* window);

// A design window just shown for the first time: capWindowToScreen when it
// is in its normal state (not maximized, full screen or minimized). Its
// frame is known only once it is shown: a start size as large as the
// screen's available area put the title bar's height below the bottom of a
// 1024 x 768 screen on Windows (mitcad#1556).
void capNewWindowToScreen(QWidget* window);

// Keeps a shown top-level window on its screen: when the screen's
// available area changes (another resolution, the Dock shown) or the
// window moves to another screen, capWindowToScreen. Call it once the
// window has its native window (from showEvent); further calls do nothing.
void keepWindowOnScreen(QWidget* window);

} // namespace mitcad
