// SPDX-License-Identifier: MIT
// The Center of Mass marker in the 3D view (mitcad#1094): a crossed circle
// (view/CenterOfMass.hpp) drawn as OCCT's marker of a vertex, pixel for
// pixel, so that it keeps its size on screen however far the camera is;
// in the top layer, as the center lies inside the bodies; never picked.
// Its tooltip when the pointer rests on it.
#include <cmath>
#include <cstddef>
#include <cstring>

#include <QImage>
#include <QPointF>
#include <QToolTip>
#include <QtLogging>

#include <AIS_InteractiveContext.hxx>
#include <AIS_Shape.hxx>
#include <Aspect_TypeOfMarker.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <Graphic3d_AspectMarker3d.hxx>
#include <Graphic3d_MarkerImage.hxx>
#include <Graphic3d_ZLayerId.hxx>
#include <Image_PixMap.hxx>
#include <Prs3d_Drawer.hxx>
#include <Prs3d_PointAspect.hxx>
#include <Quantity_Color.hxx>
#include <TopoDS_Vertex.hxx>

#include "../OcctViewer.hpp"
#include "CenterOfMass.hpp"

namespace mitcad {
namespace {

// The marker's picture as OCCT's marker image: straight-alpha RGBA, rows
// top down.
occ::handle<Graphic3d_MarkerImage> markerImage(const QImage& image) {
  const QImage rgba = image.convertToFormat(QImage::Format_RGBA8888);
  occ::handle<Image_PixMap> pixels = new Image_PixMap();
  if (rgba.isNull() ||
      !pixels->InitTrash(Image_Format_RGBA, static_cast<std::size_t>(rgba.width()),
                         static_cast<std::size_t>(rgba.height()))) {
    return occ::handle<Graphic3d_MarkerImage>();
  }
  pixels->SetTopDown(true);
  for (int row = 0; row < rgba.height(); ++row) {
    std::memcpy(pixels->ChangeRow(static_cast<std::size_t>(row)), rgba.constScanLine(row),
                static_cast<std::size_t>(rgba.width()) * 4);
  }
  return new Graphic3d_MarkerImage(pixels);
}

QString pointText(const gp_Pnt& at) {
  return QStringLiteral("%1, %2, %3").arg(at.X(), 0, 'f', 6).arg(at.Y(), 0, 'f', 6).arg(at.Z(), 0, 'f', 6);
}

} // namespace

void OcctViewer::setCenterOfMass(const std::optional<gp_Pnt>& at, const QString& tip) {
  m_centerOfMassTip = tip;
  if (!m_centerOfMassFollowsTone) {
    // Drawn again in the other tone when the background turns.
    m_centerOfMassFollowsTone = true;
    connect(this, &OcctViewer::backgroundToneChanged, this, [this] {
      if (m_centerOfMassAt) {
        paintCenterOfMass();
      }
    });
  }
  const bool same = at.has_value() == m_centerOfMassAt.has_value() &&
                    (!at || at->Distance(*m_centerOfMassAt) == 0.0);
  m_centerOfMassAt = at;
  if (same && (!at || !m_centerOfMass.IsNull())) {
    return; // as drawn
  }
  paintCenterOfMass();
}

void OcctViewer::paintCenterOfMass() {
  if (!m_centerOfMass.IsNull()) {
    m_context->Remove(m_centerOfMass, false);
    m_centerOfMass.Nullify();
  }
  QString logged = QStringLiteral("Center of Mass drawn: none");
  if (m_centerOfMassAt) {
    const occ::handle<Graphic3d_MarkerImage> image =
        markerImage(centerOfMassImage(m_backgroundDark, devicePixelRatioF()));
    if (!image.IsNull()) {
      const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(*m_centerOfMassAt).Vertex();
      m_centerOfMass = new AIS_Shape(vertex);
      // White as the object's color, which OCCT multiplies the image's by.
      const Quantity_Color white(Quantity_NOC_WHITE);
      m_centerOfMass->SetColor(white);
      occ::handle<Graphic3d_AspectMarker3d> aspect = new Graphic3d_AspectMarker3d(Aspect_TOM_USERDEFINED, white, 1.0);
      aspect->SetMarkerImage(image);
      m_centerOfMass->Attributes()->SetPointAspect(new Prs3d_PointAspect(aspect));
      // Shown, never picked (no selection mode), over the bodies.
      m_context->Display(m_centerOfMass, AIS_WireFrame, -1, false);
      m_context->SetZLayer(m_centerOfMass, Graphic3d_ZLayerId_Topmost);
      logged = QStringLiteral("Center of Mass drawn: %1 on a %2 background")
                   .arg(pointText(*m_centerOfMassAt),
                        m_backgroundDark ? QStringLiteral("dark") : QStringLiteral("light"));
    }
  }
  if (logged != m_loggedCenterOfMass) {
    m_loggedCenterOfMass = logged;
    qDebug().noquote() << logged;
  }
  sceneChanged();
}

bool OcctViewer::centerOfMassToolTip(const QPoint& at, const QPoint& global) {
  if (!m_centerOfMassAt || m_centerOfMass.IsNull() || m_centerOfMassTip.isEmpty()) {
    return false;
  }
  const QPointF marker = toWidgetF(*m_centerOfMassAt);
  const double reach = kCenterOfMassRadius + kCenterOfMassRing + kCenterOfMassHalo;
  if (std::hypot(marker.x() - at.x(), marker.y() - at.y()) > reach) {
    return false;
  }
  QToolTip::showText(global, m_centerOfMassTip, this);
  return true;
}

} // namespace mitcad
