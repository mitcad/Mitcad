// SPDX-License-Identifier: MIT
#pragma once

// The Docked layout's Command panel's width (mitcad#1825, GitHub
// mitcad/Mitcad#336). The panel was a fixed 300 px, so that the view kept
// its size when a command's panel opened; with larger Text size and Icon
// size the Sketch Palette no longer fitted it and showed scrolled sideways,
// its labels cut at the left. Now:
//   - The page stack (CommandPages) is at least commandPanelMinimumWidth()
//     wide (300 px times the larger of Text size and Icon size,
//     UiSizes.hpp), and its size hint is the panel's width whatever its
//     pages ask for, so that a page that opens or closes never moves the
//     dock's edge.
//   - The user drags the dock's edge (the window's separator left of it)
//     to make it wider; the width is remembered once the drag rests
//     (`commandPanel/dockWidth`) and restored at the next start, never below
//     the minimum at that run's sizes (CommandDockWidth).
// The Floating layout's Command card takes the same minimum width
// (MainWindow, mitcad#1846) but is not dragged.

#include <QObject>
#include <QPointer>
#include <QSize>
#include <QStackedWidget>
#include <QString>

#include <functional>

class QDockWidget;
class QEvent;
class QMainWindow;
class QScrollArea;
class QTimer;

namespace mitcad {

// A panel's scroll area (the Sketch Palette's, the 3D Sketch Palette's, a
// command's panel's) scrolls up and down only, never sideways, in both
// layouts. Its horizontal bar is hidden, but a palette wider than the panel
// still had a range there, and bringing a section into view
// (ensureWidgetVisible) scrolled it to the right, its labels cut at the
// left; the Docked panel is as wide as its contents need instead.
void keepUnscrolledSideways(QScrollArea* scroll);

// The Docked Command dock's pages: at least the panel's minimum width, and
// a size hint as wide as the panel opens (the remembered width), not the
// widest page's.
class CommandPages : public QStackedWidget {
public:
  explicit CommandPages(QWidget* parent = nullptr);
  QSize sizeHint() const override;
  QSize minimumSizeHint() const override;

private:
  int m_hintWidth;
};

// The remembered width (0: none dragged yet) and its key.
int rememberedCommandDockWidth();
void rememberCommandDockWidth(int width);

// Watches the Command dock in its main window: a resize of the dock while
// the left button is held on the window (a drag of the separator) is
// remembered once it rests; restore() gives the dock its remembered width.
// For UI tests it logs `Command panel width <w> (at least <m>)` and
// `Command edge at x,y` (where the dock's edge is dragged) when they
// change, `Command panel: dock width <w> remembered` and `Command panel:
// dock width <w> restored`.
class CommandDockWidth : public QObject {
public:
  CommandDockWidth(QMainWindow& window, QDockWidget& dock);

  // Gives the dock its remembered width (none dragged: nothing to do).
  void restore();

  // Called once the dock's width changed and its layout was logged (the
  // main window logs the Sketch Palette's rows again).
  std::function<void()> resized;

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void scheduleLog();
  void logLayout();
  void rest();

  QMainWindow& m_window;
  QPointer<QDockWidget> m_dock;
  QTimer* m_rest = nullptr; // a drag of the edge rests
  bool m_pressed = false;   // the left button is held on the window
  bool m_dragged = false;   // the dock resized while it was held
  int m_loggedWidth = -1;
  QString m_logged;         // the lines last logged
};

} // namespace mitcad
