// SPDX-License-Identifier: MIT
// A provisional drag presentation in the 3D view (mitcad#636,
// framework/DragProxy.hpp): an owned mesh drawn see-through amber with
// dashed edges, at each placement of the tool it stands for.
#include <array>
#include <cstddef>
#include <memory>
#include <utility>
#include <vector>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <Aspect_PolygonOffsetMode.hxx>
#include <Aspect_TypeOfLine.hxx>
#include <Graphic3d_ArrayFlags.hxx>
#include <Graphic3d_ArrayOfSegments.hxx>
#include <Graphic3d_ArrayOfTriangles.hxx>
#include <Graphic3d_AspectFillArea3d.hxx>
#include <Graphic3d_AspectLine3d.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_MaterialAspect.hxx>
#include <Prs3d_Presentation.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_Selection.hxx>
#include <gp_Vec.hxx>

#include "../OcctViewer.hpp"
#include "../framework/DragProxy.hpp"

namespace mitcad {
namespace {

// Amber, apart from the blue and red of an exact preview's tool: what is
// drawn is not computed yet.
const Quantity_Color kProvisionalColor = [] {
  const QColor color = viewColor(ColorRole::Provisional, false);
  return Quantity_Color(color.redF(), color.greenF(), color.blueF(), Quantity_TOC_sRGB);
}();
const Quantity_Color kProvisionalEdgeColor = [] {
  const QColor color = viewColor(ColorRole::ProvisionalEdge, false);
  return Quantity_Color(color.redF(), color.greenF(), color.blueF(), Quantity_TOC_sRGB);
}();
constexpr float kProvisionalTransparency = 0.55f;

class ProvisionalMesh : public AIS_InteractiveObject {
public:
  ProvisionalMesh(std::shared_ptr<const ProxyMesh> mesh, std::vector<gp_Trsf> placements)
      : m_mesh(std::move(mesh)), m_placements(std::move(placements)) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    if (mode != 0 || !m_mesh || m_mesh->triangles.empty() || m_placements.empty()) {
      return;
    }
    const ProxyMesh& mesh = *m_mesh;
    const int corners = static_cast<int>(mesh.triangles.size() * 3 * m_placements.size());
    occ::handle<Graphic3d_ArrayOfTriangles> triangles =
        new Graphic3d_ArrayOfTriangles(corners, 0, Graphic3d_ArrayFlags_VertexNormal);
    std::size_t segments = 0;
    for (const std::vector<int>& line : mesh.outlines) {
      segments += line.size() > 1 ? line.size() - 1 : 0;
    }
    occ::handle<Graphic3d_ArrayOfSegments> edges =
        segments > 0 ? new Graphic3d_ArrayOfSegments(static_cast<int>(segments * 2 * m_placements.size()))
                     : nullptr;
    for (const gp_Trsf& placement : m_placements) {
      for (const std::array<int, 3>& triangle : mesh.triangles) {
        const gp_Pnt a = mesh.nodes[static_cast<std::size_t>(triangle[0])].Transformed(placement);
        const gp_Pnt b = mesh.nodes[static_cast<std::size_t>(triangle[1])].Transformed(placement);
        const gp_Pnt c = mesh.nodes[static_cast<std::size_t>(triangle[2])].Transformed(placement);
        gp_Vec normal = gp_Vec(a, b).Crossed(gp_Vec(a, c));
        const double length = normal.Magnitude();
        normal = length > 1e-12 ? normal / length : gp_Vec(0.0, 0.0, 1.0);
        const gp_Dir direction(normal);
        triangles->AddVertex(a, direction);
        triangles->AddVertex(b, direction);
        triangles->AddVertex(c, direction);
      }
      if (!edges.IsNull()) {
        for (const std::vector<int>& line : mesh.outlines) {
          for (std::size_t i = 1; i < line.size(); ++i) {
            edges->AddVertex(mesh.nodes[static_cast<std::size_t>(line[i - 1])].Transformed(placement));
            edges->AddVertex(mesh.nodes[static_cast<std::size_t>(line[i])].Transformed(placement));
          }
        }
      }
    }
    Graphic3d_MaterialAspect material(Graphic3d_NameOfMaterial_Plastified);
    material.SetColor(kProvisionalColor);
    material.SetTransparency(kProvisionalTransparency);
    occ::handle<Graphic3d_AspectFillArea3d> fill = new Graphic3d_AspectFillArea3d();
    fill->SetInteriorStyle(Aspect_IS_SOLID);
    fill->SetInteriorColor(kProvisionalColor);
    fill->SetFrontMaterial(material);
    fill->SetBackMaterial(material);
    fill->SetDistinguishOff();
    fill->SetFaceCulling(Graphic3d_TypeOfBackfacingModel_DoubleSided);
    // As the exact tool: over the faces it shares with the bodies.
    fill->SetPolygonOffsets(Aspect_POM_Fill, -1.0f, -2.0f);
    const occ::handle<Graphic3d_Group> faces = presentation->NewGroup();
    faces->SetGroupPrimitivesAspect(fill);
    faces->AddPrimitiveArray(triangles);
    if (!edges.IsNull()) {
      const occ::handle<Graphic3d_Group> lines = presentation->NewGroup();
      lines->SetGroupPrimitivesAspect(new Graphic3d_AspectLine3d(kProvisionalEdgeColor, Aspect_TOL_DASH, 2.0));
      lines->AddPrimitiveArray(edges);
    }
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  std::shared_ptr<const ProxyMesh> m_mesh;
  std::vector<gp_Trsf> m_placements;
};

} // namespace

void OcctViewer::setProvisional(const std::shared_ptr<const ProxyMesh>& mesh, const std::vector<gp_Trsf>& placements) {
  const bool had = !m_provisional.IsNull();
  if (had) {
    m_context->Remove(m_provisional, false);
    m_provisional.Nullify();
  }
  if (mesh && !placements.empty()) {
    m_provisional = new ProvisionalMesh(mesh, placements);
    m_context->Display(m_provisional, 0, -1, false);
    if (!had && !m_preview.IsNull()) {
      m_context->Erase(m_preview, false); // the exact tool waits until it is cleared
    }
  } else if (had && !m_preview.IsNull()) {
    m_context->Display(m_preview, m_previewMode, -1, false);
  }
  sceneChanged();
}

} // namespace mitcad
