// SPDX-License-Identifier: MIT
// The orbit's center (mitcad#1791, GitHub mitcad/Mitcad#300): what an
// orbit turns about. A fixed center set from the view's context menu
// comes first; else Preferences › Navigation's Orbit center: the middle of
// what is shown (Model), or the model point under the cursor where the
// orbit starts (Cursor). A marker shows the fixed center while it is set,
// and a Cursor orbit's point while it turns; drawn over the bodies and
// never picked.
#include <cmath>

#include <QtLogging>

#include <AIS_InteractiveContext.hxx>
#include <AIS_Shape.hxx>
#include <AIS_ViewCube.hxx>
#include <Aspect_TypeOfMarker.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <Graphic3d_ZLayerId.hxx>
#include <Precision.hxx>
#include <Prs3d_Drawer.hxx>
#include <Prs3d_PointAspect.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_EntityOwner.hxx>
#include <SelectMgr_SelectableObject.hxx>
#include <StdSelect_ViewerSelector3d.hxx>
#include <TopoDS_Vertex.hxx>

#include "../OcctViewer.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

QString pointText(const gp_Pnt& at) {
  return QStringLiteral("%1 %2 %3").arg(at.X(), 0, 'f', 6).arg(at.Y(), 0, 'f', 6).arg(at.Z(), 0, 'f', 6);
}

bool isFinitePoint(const gp_Pnt& at) {
  return std::isfinite(at.X()) && std::isfinite(at.Y()) && std::isfinite(at.Z()) &&
         !Precision::IsInfinite(at.X()) && !Precision::IsInfinite(at.Y()) && !Precision::IsInfinite(at.Z());
}

} // namespace

void OcctViewer::setOrbitCenter(const std::optional<gp_Pnt>& at) {
  if (at && !isFinitePoint(*at)) {
    return;
  }
  m_orbitCenter = at;
  if (at) {
    qDebug().noquote() << QStringLiteral("Orbit center set at %1").arg(pointText(*at));
  } else {
    qDebug().noquote() << QStringLiteral("Orbit center cleared");
  }
  showOrbitMarker(at);
}

std::optional<gp_Pnt> OcctViewer::contextMenuPoint(const QPoint& global) const {
  return global == m_contextPointGlobal ? m_contextPoint : std::nullopt;
}

gp_Pnt OcctViewer::GravityPoint(const occ::handle<AIS_InteractiveContext>& context,
                                const occ::handle<V3d_View>& view) {
  if (context != m_context || view != m_view) {
    return AIS_ViewController::GravityPoint(context, view);
  }
  // Once as an orbit starts; while it turns, the point it started with.
  const AIS_ViewInputBuffer& input = InputBuffer(AIS_ViewInputBufferType_GL);
  if (input.OrbitRotation.ToRotate && !input.OrbitRotation.ToStart && m_lastOrbitPivot) {
    return *m_lastOrbitPivot;
  }
  // The cube's drag starts on the cube: no model point under it.
  std::optional<NCollection_Vec2<int>> cursor;
  if (!m_cubePress) {
    cursor = NCollection_Vec2<int>(static_cast<int>(std::lround(input.OrbitRotation.PointStart.x())),
                                   static_cast<int>(std::lround(input.OrbitRotation.PointStart.y())));
  }
  return orbitPivot(cursor);
}

gp_Pnt OcctViewer::orbitPivot(const std::optional<NCollection_Vec2<int>>& cursor) {
  if (m_orbitCenter) {
    m_lastOrbitPivot = m_orbitCenter;
    qDebug().noquote() << QStringLiteral("Orbit about the fixed center at %1").arg(pointText(*m_orbitCenter));
    return *m_orbitCenter;
  }
  if (m_navigation.orbitCenter == OrbitCenter::Cursor && cursor) {
    if (const std::optional<gp_Pnt> under = modelPointAt(*cursor)) {
      m_lastOrbitPivot = under;
      qDebug().noquote() << QStringLiteral("Orbit about the point under the cursor at %1").arg(pointText(*under));
      showOrbitMarker(under);
      return *under;
    }
  }
  // The middle of what is shown: OCCT's own (the shown objects' box).
  const gp_Pnt middle = AIS_ViewController::GravityPoint(m_context, m_view);
  m_lastOrbitPivot = middle;
  if (m_navigation.orbitCenter == OrbitCenter::Cursor) {
    qDebug().noquote() << QStringLiteral("Orbit about the model's middle at %1: nothing under the cursor")
                              .arg(pointText(middle));
  }
  return middle;
}

std::optional<gp_Pnt> OcctViewer::modelPointAt(const NCollection_Vec2<int>& pixel) {
  if (m_view.IsNull() || m_view->Window().IsNull()) {
    return std::nullopt;
  }
  // The hover's pick there: the next hover picks again.
  ResetPreviousMoveTo();
  m_context->MainSelector()->Pick(pixel.x(), pixel.y(), m_view);
  return pickedModelPoint();
}

std::optional<gp_Pnt> OcctViewer::pickedModelPoint() const {
  const occ::handle<StdSelect_ViewerSelector3d>& selector = m_context->MainSelector();
  for (int rank = 1; rank <= selector->NbPicked(); ++rank) {
    const occ::handle<SelectMgr_EntityOwner>& owner = selector->Picked(rank);
    if (owner.IsNull() || !owner->HasSelectable()) {
      continue;
    }
    // The cube and whatever keeps its place or size on the screen: its
    // picked point is not where the model is.
    const occ::handle<SelectMgr_SelectableObject>& selectable = owner->Selectable();
    if (selectable == m_orientationCube || !selectable->TransformPersistence().IsNull()) {
      continue;
    }
    const gp_Pnt point = selector->PickedPoint(rank);
    if (isFinitePoint(point)) {
      return point;
    }
  }
  return std::nullopt;
}

void OcctViewer::showOrbitMarker(const std::optional<gp_Pnt>& at) {
  const bool same = at.has_value() == m_orbitMarkerAt.has_value() && (!at || at->Distance(*m_orbitMarkerAt) == 0.0);
  if (same && (!at || !m_orbitMarker.IsNull())) {
    return; // as drawn
  }
  if (!m_orbitMarker.IsNull()) {
    m_context->Remove(m_orbitMarker, false);
    m_orbitMarker.Nullify();
  }
  m_orbitMarkerAt = at;
  if (at) {
    const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(*at).Vertex();
    m_orbitMarker = new AIS_Shape(vertex);
    // A small ball in the handles' orange (ColorRole::Handle), apart from
    // the selection's and the hover's blues, the same size on the screen
    // at any zoom.
    const QColor handle = viewColor(ColorRole::Handle, m_backgroundDark);
    const Quantity_Color orange(handle.redF(), handle.greenF(), handle.blueF(), Quantity_TOC_sRGB);
    m_orbitMarker->SetColor(orange);
    m_orbitMarker->Attributes()->SetPointAspect(new Prs3d_PointAspect(Aspect_TOM_BALL, orange, 2.0));
    // Shown, never picked (no selection mode), over the bodies.
    m_context->Display(m_orbitMarker, AIS_WireFrame, -1, false);
    m_context->SetZLayer(m_orbitMarker, Graphic3d_ZLayerId_Topmost);
  }
  // Also as an orbit starts, inside a frame's handling: that frame draws it.
  m_view->Invalidate();
  requestFrame();
}

} // namespace mitcad
