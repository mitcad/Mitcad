// SPDX-License-Identifier: MIT
// Simulate's stock in the 3D view (mitcad#1013, phase 3; app/COMMANDS.md
// "The CNC group"): the stock as the program has cut it so far, a lit
// surface over the bodies (the part is inside it) with its sides, grey,
// or at the end shaded by the material it leaves over the part: grey where
// nothing is left, teal to violet as more is left, pink where the part is
// cut into. Nothing of it is picked. Before Simulate and after it, the
// uncut stock of the setups shown (mitcad#2552): see-through in the
// stock's grey with thin edges, in the view's own layer so the part is seen
// inside it and the toolpaths over it.
#include <algorithm>
#include <cstddef>

#include <QColor>
#include <QString>
#include <QtLogging>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <Aspect_TypeOfLine.hxx>
#include <Graphic3d_ArrayOfSegments.hxx>
#include <Graphic3d_ArrayOfTriangles.hxx>
#include <Graphic3d_AspectFillArea3d.hxx>
#include <Graphic3d_AspectLine3d.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_MaterialAspect.hxx>
#include <Graphic3d_ZLayerId.hxx>
#include <Prs3d_Presentation.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_Selection.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "../OcctViewer.hpp"
#include "../cam/CamStock.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

Quantity_Color stockColor(const QColor& color) {
  return Quantity_Color(color.redF(), color.greenF(), color.blueF(), Quantity_TOC_sRGB);
}

// The colours of the shades (cam::StockMesh::shades) by the view's
// background.
struct StockPalette {
  QColor stock;
  QColor thin;
  QColor thick;
  QColor gouge;

  explicit StockPalette(bool dark)
      : stock(viewColor(ColorRole::CamStock, dark)), thin(viewColor(ColorRole::CamLeftThin, dark)),
        thick(viewColor(ColorRole::CamLeftThick, dark)), gouge(viewColor(ColorRole::CamGouge, dark)) {}

  Quantity_Color of(float shade) const {
    if (shade < 0.0f) {
      return stockColor(gouge);
    }
    if (shade <= 0.0f) {
      return stockColor(stock);
    }
    const double t = std::clamp(static_cast<double>(shade), 0.0, 1.0);
    const auto mix = [t](double a, double b) { return a + (b - a) * t; };
    QColor color;
    color.setRgbF(static_cast<float>(mix(thin.redF(), thick.redF())), static_cast<float>(mix(thin.greenF(), thick.greenF())),
                  static_cast<float>(mix(thin.blueF(), thick.blueF())));
    return stockColor(color);
  }
};

class CamStock : public AIS_InteractiveObject {
public:
  CamStock(const cam::StockMesh& mesh, bool dark) : m_mesh(mesh), m_dark(dark) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    if (mode != 0 || m_mesh.empty()) {
      return;
    }
    const StockPalette palette(m_dark);
    const int vertices = m_mesh.vertices();
    occ::handle<Graphic3d_ArrayOfTriangles> triangles = new Graphic3d_ArrayOfTriangles(
        vertices, static_cast<int>(m_mesh.triangles.size()),
        Graphic3d_ArrayFlags_VertexNormal | Graphic3d_ArrayFlags_VertexColor);
    for (int v = 0; v < vertices; ++v) {
      const std::size_t k = static_cast<std::size_t>(v) * 3;
      const gp_Pnt point(m_mesh.points[k], m_mesh.points[k + 1], m_mesh.points[k + 2]);
      const gp_Dir normal(m_mesh.normals[k], m_mesh.normals[k + 1], m_mesh.normals[k + 2]);
      const float shade = m_mesh.remaining ? m_mesh.shades[static_cast<std::size_t>(v)] : 0.0f;
      triangles->AddVertex(point, normal, palette.of(shade));
    }
    for (std::size_t t = 0; t + 2 < m_mesh.triangles.size(); t += 3) {
      // The array's vertices count from 1.
      triangles->AddEdges(m_mesh.triangles[t] + 1, m_mesh.triangles[t + 1] + 1, m_mesh.triangles[t + 2] + 1);
    }
    // The vertices carry the colour; the lit material is multiplied by
    // it, so the material is white. In the stock's colour it squared the
    // colour: the stock as cut drew nearly black (mitcad#2569).
    const Quantity_Color white(Quantity_NOC_WHITE);
    Graphic3d_MaterialAspect material(Graphic3d_NameOfMaterial_Plastified);
    material.SetColor(white);
    occ::handle<Graphic3d_AspectFillArea3d> fill = new Graphic3d_AspectFillArea3d();
    fill->SetInteriorStyle(Aspect_IS_SOLID);
    fill->SetInteriorColor(white);
    fill->SetFrontMaterial(material);
    fill->SetBackMaterial(material);
    fill->SetDistinguishOff();
    fill->SetFaceCulling(Graphic3d_TypeOfBackfacingModel_DoubleSided);
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(fill);
    group->AddPrimitiveArray(triangles);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  cam::StockMesh m_mesh;
  bool m_dark = false;
};

// The uncut stock's faces: mostly see-through, the part inside them in
// sight (a quarter of the stock's colour over it).
constexpr float kUncutTransparency = 0.75f;
constexpr double kUncutEdgeWidth = 1.0;

class CamUncutStock : public AIS_InteractiveObject {
public:
  CamUncutStock(const cam::UncutStock& stock, bool dark) : m_stock(stock), m_dark(dark) {}

  // Recomputed by the context's Redisplay.
  void setDark(bool dark) { m_dark = dark; }
  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    const cam::StockMesh& mesh = m_stock.surface;
    if (mode != 0 || mesh.empty()) {
      return;
    }
    const Quantity_Color color = stockColor(viewColor(ColorRole::CamStock, m_dark));
    const int vertices = mesh.vertices();
    occ::handle<Graphic3d_ArrayOfTriangles> triangles = new Graphic3d_ArrayOfTriangles(
        vertices, static_cast<int>(mesh.triangles.size()), Graphic3d_ArrayFlags_VertexNormal);
    for (int v = 0; v < vertices; ++v) {
      const std::size_t k = static_cast<std::size_t>(v) * 3;
      triangles->AddVertex(gp_Pnt(mesh.points[k], mesh.points[k + 1], mesh.points[k + 2]),
                           gp_Dir(mesh.normals[k], mesh.normals[k + 1], mesh.normals[k + 2]));
    }
    for (std::size_t t = 0; t + 2 < mesh.triangles.size(); t += 3) {
      // The array's vertices count from 1.
      triangles->AddEdges(mesh.triangles[t] + 1, mesh.triangles[t + 1] + 1, mesh.triangles[t + 2] + 1);
    }
    Graphic3d_MaterialAspect material(Graphic3d_NameOfMaterial_Plastified);
    material.SetColor(color);
    material.SetTransparency(kUncutTransparency);
    occ::handle<Graphic3d_AspectFillArea3d> fill = new Graphic3d_AspectFillArea3d();
    fill->SetInteriorStyle(Aspect_IS_SOLID);
    fill->SetInteriorColor(color);
    fill->SetFrontMaterial(material);
    fill->SetBackMaterial(material);
    fill->SetDistinguishOff();
    // Both sides drawn: the far faces seen through the near ones.
    fill->SetFaceCulling(Graphic3d_TypeOfBackfacingModel_DoubleSided);
    const occ::handle<Graphic3d_Group> faces = presentation->NewGroup();
    faces->SetGroupPrimitivesAspect(fill);
    faces->AddPrimitiveArray(triangles);
    const std::vector<float>& edges = m_stock.edges;
    if (edges.size() >= 6) {
      occ::handle<Graphic3d_ArrayOfSegments> segments =
          new Graphic3d_ArrayOfSegments(static_cast<int>(edges.size() / 3));
      for (std::size_t k = 0; k + 2 < edges.size(); k += 3) {
        segments->AddVertex(gp_Pnt(edges[k], edges[k + 1], edges[k + 2]));
      }
      // In the stock's own grey by the background's tone, at least 3:1
      // on it (mitcad#2569: HiddenEdge's grey-blue was too faint on a
      // light background).
      const occ::handle<Graphic3d_Group> lines = presentation->NewGroup();
      lines->SetGroupPrimitivesAspect(new Graphic3d_AspectLine3d(
          stockColor(viewColor(ColorRole::CamStockEdge, m_dark)), Aspect_TOL_SOLID, kUncutEdgeWidth));
      lines->AddPrimitiveArray(segments);
    }
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  cam::UncutStock m_stock;
  bool m_dark = false;
};

} // namespace

void OcctViewer::setCamStock(const cam::StockMesh& mesh) {
  if (!m_camStock.IsNull()) {
    m_context->Remove(m_camStock, false);
    m_camStock.Nullify();
  }
  if (!mesh.empty()) {
    // Over the bodies, as the toolpaths are: the part is inside the stock,
    // and the stock as cut is what Simulate shows.
    m_camStock = new CamStock(mesh, m_backgroundDark);
    m_context->Display(m_camStock, 0, -1, false);
    m_context->SetZLayer(m_camStock, Graphic3d_ZLayerId_Topmost);
  }
  // The uncut stock steps aside while the stock as cut is shown and comes
  // back, as the setups are now, when that clears (mitcad#2552).
  if (!m_camUncutStock.IsNull()) {
    if (m_camStock.IsNull()) {
      m_context->Display(m_camUncutStock, 0, -1, false);
    } else {
      m_context->Erase(m_camUncutStock, false);
    }
  }
  const QString logged = mesh.log();
  if (logged != m_loggedCamStock) {
    m_loggedCamStock = logged;
    qDebug().noquote() << logged;
  }
  logCamUncutStock();
  sceneChanged();
}

void OcctViewer::setCamUncutStock(const cam::UncutStock& stock) {
  if (!m_camUncutStock.IsNull()) {
    m_context->Remove(m_camUncutStock, false);
    m_camUncutStock.Nullify();
  }
  if (!stock.empty()) {
    // In the view's own layer, not over the bodies: the part is seen
    // inside it, the toolpaths (topmost) over it. Never picked (no
    // selection mode), so the part is picked through it.
    // While Simulate's stock is shown it waits (setCamStock shows it).
    m_camUncutStock = new CamUncutStock(stock, m_backgroundDark);
    if (m_camStock.IsNull()) {
      m_context->Display(m_camUncutStock, 0, -1, false);
    }
  }
  m_camUncutStockText = stock.log();
  logCamUncutStock();
  sceneChanged();
}

void OcctViewer::recolorCamUncutStock() {
  if (auto* stock = dynamic_cast<CamUncutStock*>(m_camUncutStock.get())) {
    stock->setDark(m_backgroundDark);
    m_context->Redisplay(m_camUncutStock, false);
  }
}

void OcctViewer::logCamUncutStock() {
  const bool aside = !m_camUncutStock.IsNull() && !m_camStock.IsNull();
  const QString logged =
      aside ? m_camUncutStockText + QStringLiteral(", under the cut stock") : m_camUncutStockText;
  if (logged != m_loggedCamUncutStock) {
    m_loggedCamUncutStock = logged;
    qDebug().noquote() << logged;
  }
}

} // namespace mitcad
