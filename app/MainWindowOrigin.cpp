// SPDX-License-Identifier: MIT
// How the design's origin and construction geometry are drawn (mitcad#1748,
// GitHub mitcad/Mitcad#273 and #279; app/COMMANDS.md "Origin planes and
// the size of construction geometry"): Preferences › Display › Origin
// planes (view/OriginPreference.hpp) given to the view, and the resize
// handles of a selected construction plane or axis (view/DatumHandles.hpp),
// whose drag sets the size the model keeps with the feature
// (set_datum_display_size).
#include "MainWindow.hpp"

#include <exception>
#include <optional>

#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QTimer>
#include <QtLogging>

#include <AIS_AnimationCamera.hxx>
#include <Bnd_Box.hxx>
#include <gp.hxx>
#include <gp_Trsf.hxx>

#include "OcctViewer.hpp"
#include "framework/Command.hpp"
#include "framework/ModelShapes.hpp"
#include "view/DatumHandles.hpp"
#include "view/OriginPreference.hpp"

namespace mitcad {

void MainWindow::applyOriginDisplay(bool redraw) {
  if (redraw && modelBusy()) {
    // The datums are read from the model: once its job is done.
    QTimer::singleShot(200, this, [this] { applyOriginDisplay(true); });
    return;
  }
  m_originDisplay = savedOriginDisplay();
  const bool onScreen = m_originDisplay.size == OriginSize::Screen;
  const bool corner = m_originDisplay.style == OriginStyle::Corner;
  m_viewer->setOriginLook(onScreen, corner);
  // What a fit frames with nothing else shown and a new design's first view
  // (mitcad#1000): the planes' box as they are drawn in millimetres; with
  // Size Screen they have no size in the model, and the 100 mm centred box
  // keeps a new design's scale as it was.
  Bnd_Box box;
  const double side = onScreen ? kDefaultDatumSize : m_originDisplay.millimetres;
  if (corner && !onScreen) {
    box.Update(0.0, 0.0, 0.0, side, side, side);
  } else {
    box.Update(-side / 2.0, -side / 2.0, -side / 2.0, side / 2.0, side / 2.0, side / 2.0);
  }
  m_viewer->setOriginBox(box);
  qDebug().noquote() << QStringLiteral("Origin planes: %1").arg(m_originDisplay.describe());
  if (!redraw) {
    return;
  }
  showDatums();
  if (m_mode == Mode::Idle) {
    updateIdleHighlights(); // a selected origin plane's highlight as it is drawn now
  }
}

double MainWindow::originDrawnSize() const {
  return m_originDisplay.size == OriginSize::Screen ? kOriginScreenPixels * m_viewer->devicePixelRatioF()
                                                    : m_originDisplay.millimetres;
}

Bnd_Box MainWindow::originPlanesBox() const {
  const double side = originModelSize();
  Bnd_Box box;
  if (m_originDisplay.style == OriginStyle::Corner) {
    box.Update(0.0, 0.0, 0.0, side, side, side);
  } else {
    box.Update(-side / 2.0, -side / 2.0, -side / 2.0, side / 2.0, side / 2.0, side / 2.0);
  }
  return box;
}

void MainWindow::keepOriginClearOfCards() {
  // Only the Floating layout's cards float over the view; Docked panels are
  // beside it.
  if (m_floating == nullptr || !m_originClearArmed || m_mode != Mode::PickPlane) {
    return;
  }
  // The planes' size on the screen where the view is going (a new design's
  // framing made first).
  m_viewer->settleCamera();
  if (m_viewer->panClearOfCards(originPlanesBox(), QStringLiteral("the origin planes"))) {
    // The animated move's end is no navigation of the user's; the datums'
    // places are logged again once it rests (cameraRested), the faces' too.
    m_originClearMoving = !m_viewer->ViewAnimation()->IsStopped();
    m_originClearLogPicks = true;
  }
}

double MainWindow::originModelSize() const {
  return m_originDisplay.size == OriginSize::Screen ? kOriginScreenPixels * m_viewer->modelPerPixel(gp::Origin())
                                                    : m_originDisplay.millimetres;
}

TopoDS_Shape MainWindow::originShape(const QJsonObject& datum) const {
  return originDatumShape(datum, m_originDisplay.style, originDrawnSize());
}

double MainWindow::datumDisplaySize(const QJsonObject& datum) {
  const double size = datum.value(QStringLiteral("display_size")).toDouble(0.0);
  return size > 0.0 ? size : kDefaultDatumSize;
}

void MainWindow::updateDatumHandles() {
  if (m_viewer == nullptr) {
    return;
  }
  std::optional<DatumHandles::Target> target;
  // One construction plane or axis selected, shown, while nothing else
  // runs and the design may be changed.
  const SelectionItem* item = m_selection.size() == 1 ? &m_selection.front() : nullptr;
  const DocumentSnapshot::Feature* feature = item != nullptr ? m_snapshot.feature(item->owner) : nullptr;
  if (m_mode == Mode::Idle && item != nullptr && feature != nullptr && !windowReadOnly() && !modelBusy() &&
      !m_constructionHidden && (item->kind == SelectKind::Plane || item->kind == SelectKind::Axis) &&
      (feature->type == QLatin1String("construction_plane") || feature->type == QLatin1String("construction_axis")) &&
      m_snapshot.featureShown.value(item->owner, true) && !m_snapshot.kindHidden(*feature) &&
      (item->occurrence.isEmpty() || m_snapshot.isOccurrenceShown(item->occurrence))) {
    try {
      const QJsonObject datum =
          queryObject({{QStringLiteral("query"), QStringLiteral("datum")}, {QStringLiteral("uid"), item->owner}});
      DatumHandles::Target shown;
      shown.uid = item->owner;
      shown.size = datumDisplaySize(datum);
      shown.height = shown.size;
      shown.stored = datum.value(QStringLiteral("display_size")).toDouble(0.0) > 0.0;
      const gp_Trsf placement =
          item->occurrence.isEmpty() ? gp_Trsf() : placementOf(item->occurrence).Transformation();
      // Where it is drawn (mitcad#1827): its rectangle or line, fitted to
      // what it is made from or at its own size.
      const bool plane = datum.value(QStringLiteral("type")).toString() == QLatin1String("plane");
      if (const std::optional<DrawnDatum> drawn = drawnDatum(datum); drawn && drawn->plane == plane) {
        shown.plane = plane;
        shown.centre = drawn->centre.Transformed(placement);
        shown.xAxis = gp_Dir(drawn->xAxis).Transformed(placement);
        shown.yAxis = plane ? gp_Dir(drawn->yAxis).Transformed(placement) : gp_Dir(0, 1, 0);
        shown.size = drawn->width;
        shown.height = plane ? drawn->height : drawn->width;
        target = shown;
      } else if (plane) {
        shown.plane = true;
        shown.centre = pointOf(datum.value(QStringLiteral("origin"))).Transformed(placement);
        shown.xAxis = gp_Dir(vectorOf(datum.value(QStringLiteral("x_axis")))).Transformed(placement);
        shown.yAxis = gp_Dir(vectorOf(datum.value(QStringLiteral("y_axis")))).Transformed(placement);
        target = shown;
      } else if (datum.value(QStringLiteral("type")).toString() == QLatin1String("axis")) {
        shown.plane = false;
        shown.centre = pointOf(datum.value(QStringLiteral("origin"))).Transformed(placement);
        shown.xAxis = gp_Dir(vectorOf(datum.value(QStringLiteral("direction")))).Transformed(placement);
        target = shown;
      }
    } catch (const std::exception&) {
      target.reset(); // gone from the model, or degenerate (Standard_Failure)
    }
  }
  if (!target && m_datumHandles == nullptr) {
    return;
  }
  if (m_datumHandles == nullptr) {
    m_datumHandles = new DatumHandles(*m_viewer);
    connect(m_datumHandles, &DatumHandles::resized, this, [this](const QString& uid, double size) {
      // One undo step ("Resize Plane1"); the view draws it anew from the
      // model's answer.
      runModelCommand({{QStringLiteral("cmd"), QStringLiteral("set_datum_display_size")},
                       {QStringLiteral("uid"), uid},
                       {QStringLiteral("size"), size}});
    });
    connect(m_datumHandles, &DatumHandles::resetRequested, this, [this](const QString& uid) {
      // Reset Size (mitcad#1827): drawn fitted to what it is made from
      // again, one undo step.
      runModelCommand({{QStringLiteral("cmd"), QStringLiteral("set_datum_display_size")},
                       {QStringLiteral("uid"), uid},
                       {QStringLiteral("size"), QJsonValue::Null}});
    });
  }
  m_datumHandles->setTarget(target);
}

void MainWindow::logDrawnDatum(const QString& uid, const QJsonObject& datum) {
  const std::optional<DrawnDatum> drawn = drawnDatum(datum);
  if (!drawn) {
    return;
  }
  const auto mm = [](double value) { return QString::number(value, 'g', 6); };
  const QString line = drawn->plane ? QStringLiteral("Datum %1 drawn %2 x %3 mm (%4)")
                                          .arg(uid, mm(drawn->width), mm(drawn->height), drawn->source)
                                    : QStringLiteral("Datum %1 drawn %2 mm long (%3)")
                                          .arg(uid, mm(drawn->width), drawn->source);
  if (m_loggedDrawn.value(uid) == line) {
    return;
  }
  m_loggedDrawn.insert(uid, line);
  qDebug().noquote() << line;
}

} // namespace mitcad
