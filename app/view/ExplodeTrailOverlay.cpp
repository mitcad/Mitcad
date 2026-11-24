// SPDX-License-Identifier: MIT
// An explode view's trail lines in the 3D view (mitcad#2113): while an
// explode view is shown, a thin dashed line from where each moved part was
// to where each step took it (the model's `explode_view` query's
// `design_trails`), over the bodies in the edges' color; never picked.
#include <cstddef>
#include <utility>
#include <vector>

#include <QColor>
#include <QString>
#include <QtLogging>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <Graphic3d_ArrayOfSegments.hxx>
#include <Graphic3d_AspectLine3d.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_ZLayerId.hxx>
#include <Prs3d_Presentation.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_Selection.hxx>
#include <gp_Pnt.hxx>

#include "../OcctViewer.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

class ExplodeTrails : public AIS_InteractiveObject {
public:
  ExplodeTrails(std::vector<float> points, const QColor& color) : m_points(std::move(points)), m_color(color) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    const std::size_t count = m_points.size() / 6;
    if (mode != 0 || count == 0) {
      return;
    }
    occ::handle<Graphic3d_ArrayOfSegments> lines = new Graphic3d_ArrayOfSegments(static_cast<int>(count * 2));
    for (std::size_t s = 0; s < count; ++s) {
      const float* v = m_points.data() + s * 6;
      lines->AddVertex(gp_Pnt(v[0], v[1], v[2]));
      lines->AddVertex(gp_Pnt(v[3], v[4], v[5]));
    }
    const Quantity_Color color(m_color.redF(), m_color.greenF(), m_color.blueF(), Quantity_TOC_sRGB);
    occ::handle<Graphic3d_AspectLine3d> aspect = new Graphic3d_AspectLine3d(color, Aspect_TOL_DASH, 1.0);
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(aspect);
    group->AddPrimitiveArray(lines);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  std::vector<float> m_points;
  QColor m_color;
};

} // namespace

void OcctViewer::setExplodeTrails(const std::vector<float>& lines) {
  if (!m_explodeTrails.IsNull()) {
    m_context->Remove(m_explodeTrails, false);
    m_explodeTrails.Nullify();
  }
  const std::size_t count = lines.size() / 6;
  if (count > 0) {
    m_explodeTrails = new ExplodeTrails(lines, viewColor(ColorRole::Edge, m_backgroundDark));
    m_context->Display(m_explodeTrails, 0, -1, false);
    m_context->SetZLayer(m_explodeTrails, Graphic3d_ZLayerId_Topmost);
  }
  // For UI tests: how many lines, when it changes.
  const QString logged = count == 0 ? QStringLiteral("Explode trails: none")
                                    : QStringLiteral("Explode trails: %1 line(s)").arg(static_cast<qulonglong>(count));
  if (logged != m_loggedExplodeTrails) {
    m_loggedExplodeTrails = logged;
    qDebug().noquote() << logged;
  }
  sceneChanged();
}

} // namespace mitcad
