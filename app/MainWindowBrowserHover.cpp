// SPDX-License-Identifier: MIT
// The browser's hover (mitcad#1022; app/COMMANDS.md, "Browser"): the
// pointer over a body's, a component's or an occurrence's row highlights
// that body, or all the bodies of the occurrence and of those in it, in the
// view, over the scene, the parts other bodies hide shown through them. It
// identifies the parts of a large assembly from its rows. The selection,
// visibility and camera stay as they are; the highlight goes when the
// pointer leaves the row or the browser.
//
// A hover must stay cheap on a large assembly, so that moving the pointer
// down the rows quickly stays smooth: nothing is meshed and no presentation
// is computed. The view shows each body's presentation as it draws it now
// again through shadows of it (OcctViewer::setBrowserHover). Each change is
// logged, for the tests, with the time it took and the faces the view's
// presentations meshed meanwhile (none):
//   Browser hover highlight: Root/Component1:1 (12 bodies), 0.42 ms, 0 faces meshed
//   Browser hover highlight: none (0 bodies), 0.05 ms, 0 faces meshed
#include "MainWindow.hpp"

#include <cstddef>

#include <QElapsedTimer>
#include <QtLogging>

#include "mitcad/io/mesh.hpp"

namespace mitcad {

void MainWindow::previewBodies(const QString& row, const QString& body, const QString& occurrence) {
  if (row == m_browserHoverRow) {
    return;
  }
  m_browserHoverRow = row;
  QElapsedTimer clock;
  clock.start();
  const std::size_t meshedBefore = io::presentation_faces_meshed();
  const int bodies = m_viewer->setBrowserHover(body, occurrence, !row.isEmpty());
  const double milliseconds = static_cast<double>(clock.nsecsElapsed()) / 1.0e6;
  const std::size_t meshed = io::presentation_faces_meshed() - meshedBefore;
  qDebug().noquote() << QStringLiteral("Browser hover highlight: %1 (%2 bodies), %3 ms, %4 faces meshed")
                            .arg(row.isEmpty() ? QStringLiteral("none") : row)
                            .arg(bodies)
                            .arg(milliseconds, 0, 'f', 2)
                            .arg(static_cast<qulonglong>(meshed));
}

} // namespace mitcad
