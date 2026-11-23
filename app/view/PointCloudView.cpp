// SPDX-License-Identifier: MIT
// Point clouds in the 3D view (mitcad#2240): each shown cloud as points,
// depth-tested with the bodies, in the file's colours or the neutral grey
// of the view's background (the points a shown Point Cloud Deviation did
// not measure dimmed, mitcad#2265); never picked. The points stay single
// precision from the cloud's origin and the presentation is placed by the
// cloud's transform, so a scan far from the design's origin keeps its
// detail on the screen.
#include <cstddef>
#include <cstdint>
#include <utility>
#include <vector>

#include <QColor>
#include <QtLogging>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <Graphic3d_ArrayOfPoints.hxx>
#include <Graphic3d_AspectMarker3d.hxx>
#include <Graphic3d_Group.hxx>
#include <Prs3d_Presentation.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_Selection.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "../OcctViewer.hpp"
#include "../framework/Theme.hpp"
#include "PointClouds.hpp"

namespace mitcad {
namespace {

Quantity_Color colorOf(const QColor& color) {
  return Quantity_Color(color.redF(), color.greenF(), color.blueF(), Quantity_TOC_sRGB);
}

// A cloud's points, from its origin; the transform places them.
class CloudPoints : public AIS_InteractiveObject {
public:
  CloudPoints(PointCloudDrawn cloud, const QColor& plain) : m_cloud(std::move(cloud)), m_plain(plain) {
    gp_Trsf place;
    const auto& m = m_cloud.linear;
    const auto& t = m_cloud.translation;
    place.SetValues(m[0][0], m[0][1], m[0][2], t[0], m[1][0], m[1][1], m[1][2], t[1], m[2][0], m[2][1], m[2][2],
                    t[2]);
    SetLocalTransformation(place);
  }

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    const std::size_t count = m_cloud.count();
    if (mode != 0 || count == 0) {
      return;
    }
    const bool colored = m_cloud.colors.size() == count * 3;
    occ::handle<Graphic3d_ArrayOfPoints> points = new Graphic3d_ArrayOfPoints(static_cast<int>(count), colored);
    for (std::size_t i = 0; i < count; ++i) {
      const float* p = m_cloud.points.data() + i * 3;
      if (colored) {
        const std::uint8_t* c = m_cloud.colors.data() + i * 3;
        points->AddVertex(gp_Pnt(p[0], p[1], p[2]), colorOf(QColor(c[0], c[1], c[2])));
      } else {
        points->AddVertex(gp_Pnt(p[0], p[1], p[2]));
      }
    }
    occ::handle<Graphic3d_AspectMarker3d> aspect =
        new Graphic3d_AspectMarker3d(Aspect_TOM_POINT, colorOf(m_plain), 2.0);
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(aspect);
    group->AddPrimitiveArray(points);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  PointCloudDrawn m_cloud;
  QColor m_plain;
};

} // namespace

void OcctViewer::setPointClouds(const std::vector<PointCloudDrawn>& clouds) {
  for (const occ::handle<AIS_InteractiveObject>& object : m_pointClouds) {
    m_context->Remove(object, false);
  }
  m_pointClouds.clear();
  // A cloud without colours in the neutral grey of the view's background;
  // a shown deviation's points it did not measure dimmed (mitcad#2265).
  const QColor plain = viewColor(ColorRole::DeviationNeutral, m_backgroundDark);
  const QColor dimmed = viewColor(ColorRole::DeviationUnmeasured, m_backgroundDark);
  for (const PointCloudDrawn& cloud : clouds) {
    if (cloud.count() == 0) {
      continue;
    }
    occ::handle<AIS_InteractiveObject> object = new CloudPoints(cloud, cloud.unmeasured ? dimmed : plain);
    m_context->Display(object, 0, -1, false);
    m_pointClouds.push_back(object);
  }
  // For UI tests: what is drawn, when it changes.
  const QString logged = pointCloudsLog(clouds);
  if (logged != m_loggedPointClouds) {
    m_loggedPointClouds = logged;
    qDebug().noquote() << logged;
  }
  sceneChanged();
}

} // namespace mitcad
