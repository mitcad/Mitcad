// SPDX-License-Identifier: MIT
// Filament changes in the 3D view (mitcad#1643): while Send to Slicer's
// Filament changes section is open, the bodies drawn in the colours of the
// filaments they print with by height (the model's
// `filament_change_preview` query, as `model_render`'s print colours), and
// a thin ring round them at each change's height. Neither is pickable;
// closing the panel clears them and the bodies look as before.
#include <cstddef>
#include <utility>
#include <vector>

#include <QColor>
#include <QString>
#include <QStringList>
#include <QtLogging>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <Aspect_PolygonOffsetMode.hxx>
#include <Graphic3d_ArrayOfSegments.hxx>
#include <Graphic3d_ArrayOfTriangles.hxx>
#include <Graphic3d_AspectFillArea3d.hxx>
#include <Graphic3d_AspectLine3d.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_ZLayerId.hxx>
#include <Prs3d_Presentation.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_Selection.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "../OcctViewer.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

// How far the colour bands lie off the bodies' faces along their normals
// (mm), over the polygon offset: the preview's mesh and the view's differ
// by their deviations.
constexpr double kBandLift = 0.04;

Quantity_Color bandColor(const QColor& color) {
  return Quantity_Color(color.redF(), color.greenF(), color.blueF(), Quantity_TOC_sRGB);
}

// The bands: a group of flat triangles per colour, both sides, in front of
// the bodies' own faces.
class FilamentBands : public AIS_InteractiveObject {
public:
  explicit FilamentBands(std::vector<std::pair<QColor, std::vector<float>>> bands) : m_bands(std::move(bands)) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    if (mode != 0) {
      return;
    }
    for (const auto& [color, xyz] : m_bands) {
      const std::size_t count = xyz.size() / 9;
      if (count == 0) {
        continue;
      }
      occ::handle<Graphic3d_ArrayOfTriangles> triangles =
          new Graphic3d_ArrayOfTriangles(static_cast<int>(count * 3));
      for (std::size_t t = 0; t < count; ++t) {
        const float* v = xyz.data() + t * 9;
        const gp_Pnt a(v[0], v[1], v[2]);
        const gp_Pnt b(v[3], v[4], v[5]);
        const gp_Pnt c(v[6], v[7], v[8]);
        const gp_Vec normal = gp_Vec(a, b).Crossed(gp_Vec(a, c));
        const double length = normal.Magnitude();
        const gp_Vec shift = length > 1e-12 ? normal * (kBandLift / length) : gp_Vec(0.0, 0.0, 0.0);
        triangles->AddVertex(a.Translated(shift));
        triangles->AddVertex(b.Translated(shift));
        triangles->AddVertex(c.Translated(shift));
      }
      occ::handle<Graphic3d_AspectFillArea3d> aspect = new Graphic3d_AspectFillArea3d();
      aspect->SetInteriorStyle(Aspect_IS_SOLID);
      aspect->SetShadingModel(Graphic3d_TypeOfShadingModel_Unlit);
      aspect->SetInteriorColor(bandColor(color));
      aspect->SetFaceCulling(Graphic3d_TypeOfBackfacingModel_DoubleSided);
      aspect->SetPolygonOffsets(Aspect_POM_Fill, -1.0f, -1.0f);
      const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
      group->SetGroupPrimitivesAspect(aspect);
      group->AddPrimitiveArray(triangles);
    }
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  std::vector<std::pair<QColor, std::vector<float>>> m_bands;
};

// The rings: segments over the bodies, in the bodies' edge colour.
class FilamentRings : public AIS_InteractiveObject {
public:
  FilamentRings(std::vector<float> points, const QColor& color) : m_points(std::move(points)), m_color(color) {}

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
    occ::handle<Graphic3d_AspectLine3d> aspect = new Graphic3d_AspectLine3d(bandColor(m_color), Aspect_TOL_SOLID, 2.0);
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

void OcctViewer::setFilamentChangeOverlay(const std::vector<std::pair<QColor, std::vector<float>>>& bands,
                                          const std::vector<float>& rings) {
  if (!m_filamentBands.IsNull()) {
    m_context->Remove(m_filamentBands, false);
    m_filamentBands.Nullify();
  }
  if (!m_filamentRings.IsNull()) {
    m_context->Remove(m_filamentRings, false);
    m_filamentRings.Nullify();
  }
  std::size_t triangles = 0;
  QStringList colors;
  for (const auto& [color, xyz] : bands) {
    triangles += xyz.size() / 9;
    if (!xyz.empty()) {
      colors << color.name().toUpper();
    }
  }
  if (triangles > 0) {
    m_filamentBands = new FilamentBands(bands);
    m_context->Display(m_filamentBands, 0, -1, false);
  }
  if (rings.size() >= 6) {
    // Drawn over the bodies as their edges are, in the edges' colour.
    const QColor ring = viewColor(ColorRole::Edge, m_backgroundDark);
    m_filamentRings = new FilamentRings(rings, ring);
    m_context->Display(m_filamentRings, 0, -1, false);
    m_context->SetZLayer(m_filamentRings, Graphic3d_ZLayerId_Topmost);
  }
  // For UI tests: the colours drawn and the rings' segments, when they
  // change.
  const QString logged = triangles == 0 && rings.size() < 6
                             ? QStringLiteral("Filament change overlay: none")
                             : QStringLiteral("Filament change overlay: %1 triangle(s) in %2; %3 ring segment(s)")
                                   .arg(static_cast<qulonglong>(triangles))
                                   .arg(colors.join(QStringLiteral(", ")))
                                   .arg(static_cast<qulonglong>(rings.size() / 6));
  if (logged != m_loggedFilamentOverlay) {
    m_loggedFilamentOverlay = logged;
    qDebug().noquote() << logged;
  }
  sceneChanged();
}

} // namespace mitcad
