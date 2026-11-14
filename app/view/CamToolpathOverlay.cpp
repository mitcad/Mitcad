// SPDX-License-Identifier: MIT
// CAM toolpaths in the 3D view (mitcad#1013, app/COMMANDS.md "The CNC
// group"): the shown setups' toolpaths as lines over the bodies, in the
// looks of cam::ToolpathScene (rapids dashed, cuts solid, entries in their
// own colour, the current operation thicker in the selection's colour, an
// operation out of date dotted and muted), and Simulate's tool as a
// cylinder of lines. Nothing of it is picked.
#include <cmath>
#include <cstddef>
#include <utility>
#include <vector>

#include <QColor>
#include <QString>
#include <QtLogging>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <Aspect_TypeOfLine.hxx>
#include <Graphic3d_ArrayOfSegments.hxx>
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
#include "../cam/CamToolpaths.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

using Look = cam::ToolpathScene::Look;

Quantity_Color lineColor(const QColor& color) {
  return Quantity_Color(color.redF(), color.greenF(), color.blueF(), Quantity_TOC_sRGB);
}

// A look's colour role, line type and width. The roles are the view's
// own: rapids as hidden edges, cuts as an inspection's target, entries as
// the second input's, the current operation as a feature's input lines.
struct LineLook {
  ColorRole role;
  Aspect_TypeOfLine type;
  double width;
};

LineLook lookOf(Look look) {
  switch (look) {
  case Look::Rapid:
    return {ColorRole::HiddenEdge, Aspect_TOL_DASH, 1.0};
  case Look::Feed:
    return {ColorRole::PreviewTarget, Aspect_TOL_SOLID, 1.5};
  case Look::Entry:
    return {ColorRole::Input2, Aspect_TOL_SOLID, 1.5};
  case Look::Stale:
    return {ColorRole::HiddenEdge, Aspect_TOL_DOT, 1.0};
  case Look::Current:
    return {ColorRole::SelectLine, Aspect_TOL_SOLID, 2.5};
  case Look::CurrentRapid:
    return {ColorRole::SelectLine, Aspect_TOL_DASH, 1.5};
  }
  return {ColorRole::PreviewTarget, Aspect_TOL_SOLID, 1.5};
}

// The tool as lines: a circle at the tip and at the top of the length
// drawn, and four lines between them.
std::vector<float> toolLines(const cam::ToolpathScene::Tool& tool) {
  std::vector<float> out;
  const gp_Pnt tip(tool.tip[0], tool.tip[1], tool.tip[2]);
  gp_Vec axis(tool.axis[0], tool.axis[1], tool.axis[2]);
  if (axis.Magnitude() < 1e-12) {
    axis = gp_Vec(0.0, 0.0, 1.0);
  }
  axis.Normalize();
  // Two directions across the axis.
  gp_Vec across = std::abs(axis.Z()) < 0.9 ? axis.Crossed(gp_Vec(0.0, 0.0, 1.0)) : axis.Crossed(gp_Vec(1.0, 0.0, 0.0));
  across.Normalize();
  const gp_Vec third = axis.Crossed(across);
  const double radius = tool.diameter / 2.0;
  const auto at = [&](double angle, double up) {
    return tip.Translated(across * (radius * std::cos(angle)) + third * (radius * std::sin(angle)) + axis * up);
  };
  const auto add = [&out](const gp_Pnt& a, const gp_Pnt& b) {
    for (const gp_Pnt* p : {&a, &b}) {
      out.push_back(static_cast<float>(p->X()));
      out.push_back(static_cast<float>(p->Y()));
      out.push_back(static_cast<float>(p->Z()));
    }
  };
  constexpr int kSteps = 24;
  constexpr double kTurn = 6.28318530717958647692;
  for (int i = 0; i < kSteps; ++i) {
    const double a = kTurn * i / kSteps;
    const double b = kTurn * (i + 1) / kSteps;
    add(at(a, 0.0), at(b, 0.0));
    add(at(a, tool.length), at(b, tool.length));
    if (i % (kSteps / 4) == 0) {
      add(at(a, 0.0), at(a, tool.length));
    }
  }
  return out;
}

class CamToolpaths : public AIS_InteractiveObject {
public:
  CamToolpaths(cam::ToolpathScene scene, bool dark) : m_scene(std::move(scene)), m_dark(dark) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    if (mode != 0) {
      return;
    }
    for (const cam::ToolpathScene::Group& group : m_scene.groups) {
      const LineLook look = lookOf(group.look);
      addLines(presentation, group.segments, viewColor(look.role, m_dark), look.type, look.width);
    }
    if (m_scene.tool) {
      addLines(presentation, toolLines(*m_scene.tool), viewColor(ColorRole::Handle, m_dark), Aspect_TOL_SOLID, 2.0);
    }
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  static void addLines(const occ::handle<Prs3d_Presentation>& presentation, const std::vector<float>& segments,
                       const QColor& color, Aspect_TypeOfLine type, double width) {
    const std::size_t count = segments.size() / 6;
    if (count == 0) {
      return;
    }
    occ::handle<Graphic3d_ArrayOfSegments> lines = new Graphic3d_ArrayOfSegments(static_cast<int>(count * 2));
    for (std::size_t s = 0; s < count; ++s) {
      const float* v = segments.data() + s * 6;
      lines->AddVertex(gp_Pnt(v[0], v[1], v[2]));
      lines->AddVertex(gp_Pnt(v[3], v[4], v[5]));
    }
    occ::handle<Graphic3d_AspectLine3d> aspect = new Graphic3d_AspectLine3d(lineColor(color), type, width);
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(aspect);
    group->AddPrimitiveArray(lines);
  }

  cam::ToolpathScene m_scene;
  bool m_dark = false;
};

} // namespace

void OcctViewer::setCamToolpaths(const cam::ToolpathScene& scene) {
  if (!m_camToolpaths.IsNull()) {
    m_context->Remove(m_camToolpaths, false);
    m_camToolpaths.Nullify();
  }
  if (!scene.empty()) {
    // Over the bodies, as the filament change rings are: a toolpath runs
    // on the faces it cuts and would hide in them.
    m_camToolpaths = new CamToolpaths(scene, m_backgroundDark);
    m_context->Display(m_camToolpaths, 0, -1, false);
    m_context->SetZLayer(m_camToolpaths, Graphic3d_ZLayerId_Topmost);
  }
  const QString logged = scene.log();
  if (logged != m_loggedCamToolpaths) {
    m_loggedCamToolpaths = logged;
    qDebug().noquote() << logged;
  }
  sceneChanged();
}

} // namespace mitcad
