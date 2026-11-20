// SPDX-License-Identifier: MIT
#pragma once

// The Docked layout's docks kept between starts (mitcad#2070, GitHub
// mitcad/Mitcad#451, app/COMMANDS.md "The window's size and place"): at
// quit, beside the window's place, the active window's arrangement of its
// docks (QMainWindow::saveState: each dock's area, size, docked or floating,
// shown or hidden) and each floating dock's own frame and screen; restored
// in the first window of the next start, where a floating dock whose kept
// frame is not wholly on its connected screen goes back into its dock
// (floatingFrameStays, WindowPlace.hpp). A new window takes the arrangement
// with its floating docks docked. Nothing of it in the Floating layout or
// the AppKit frontend, and not in Export Settings (it names this machine's
// screens).
//
// Kept in the application's settings, group "dockState":
//   version   kDockStateVersion when it was saved
//   state     QMainWindow::saveState(kDockStateVersion)
//   docks     the docks the state stands for (the Render workspace's are
//             left out: they open at their own areas)
//   shown     docks saved as hidden only because the Start page or the
//             Render workspace hid them: shown after the restore
//   floating/<dock>/frame     a floating dock's frame (title bar and
//                             borders included), global logical pixels
//   floating/<dock>/geometry  its own area
//   floating/<dock>/screen    the name of the screen it was on

#include <QByteArray>
#include <QHash>
#include <QRect>
#include <QString>
#include <QStringList>

#include <Qt>

class QDockWidget;
class QMainWindow;
class QSettings;

namespace mitcad {

// Raised whenever a dock is renamed or added at another default area, so
// that an older arrangement is dropped rather than misapplied.
constexpr int kDockStateVersion = 1;

struct KeptDockState {
  struct Floating {
    QRect frame;
    QRect geometry;
    QString screen;
  };
  int version = 0;
  QByteArray state;
  QStringList docks;
  QStringList shown;
  QHash<QString, Floating> floating;
  bool valid() const { return !state.isEmpty(); }
};

// The kept arrangement (invalid when none is kept), kept, and forgotten
// (Restore Default Layout).
KeptDockState savedDockState(const QSettings& settings);
void saveDockState(QSettings& settings, const KeptDockState& kept);
void forgetDockState(QSettings& settings);

// The arrangement of `window`'s docks (its direct children with an object
// name) to keep: `leftOut` are not among its docks, `shownOutside` are
// kept as shown although hidden now.
KeptDockState dockStateOf(QMainWindow& window, const QStringList& leftOut, const QStringList& shownOutside);

// What a restore did, for the log.
struct DockRestore {
  bool restored = false;
  QString ignored;          // why a kept arrangement was not used
  QStringList docked;       // "<dock> docked: screen gone" and the like
  QStringList misplaced;    // docks the state does not name, not at their areas
};

// Gives `window`'s docks the kept arrangement. A dock the arrangement does
// not name (`leftOut`, or one added in a later version) keeps its area and
// visibility; one that is not at its area in `defaultAreas` afterwards is
// listed in `misplaced`, for placeDocksAtTheirAreas() once the window has
// shown. With `keepFloating` (the first window of a start) a floating dock
// stays at its kept frame when floatingFrameStays() says so; otherwise
// (another window, or its screen gone) it goes back into its dock.
DockRestore restoreDockState(QMainWindow& window, const KeptDockState& kept,
                             const QHash<QString, Qt::DockWidgetArea>& defaultAreas, const QStringList& leftOut,
                             bool keepFloating);

// The docks `names` docked at their areas in `defaultAreas`, shown or
// hidden as they are. Not on a window that has not shown since its last
// restoreState (see restoreDockState).
void placeDocksAtTheirAreas(QMainWindow& window, const QStringList& names,
                            const QHash<QString, Qt::DockWidgetArea>& defaultAreas);

// "browserDock right, timelineDock floating 100,80 400x200 on <screen>,
// chatDock right hidden" for the logs: the docks among `names`.
QString dockArrangementDescription(const QMainWindow& window, const QStringList& names);

// "left", "right", "top", "bottom" or "floating" for a dock of `window`.
QString dockAreaName(const QMainWindow& window, const QDockWidget& dock);

} // namespace mitcad
