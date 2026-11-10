// SPDX-License-Identifier: MIT
// Places in the edited sketch for Move/Copy (mitcad#1563): the pick ids of
// places the sketch's snaps found where no point is (Point to Point's
// Origin and Target Point), and the middle of the objects, where Translate's
// handle sits and Rotate turns without a Pivot Point. On the sketch model's
// data only: plain Qt Core.
#include "SketchModel.hpp"

#include <algorithm>

#include <QStringList>

namespace mitcad::sketch {
namespace {

const PointData* pointWithId(const SketchModel& model, const QString& id) {
  const auto found =
      std::find_if(model.points.begin(), model.points.end(), [&id](const PointData& p) { return p.id == id; });
  return found == model.points.end() ? nullptr : &*found;
}

const CurveData* curveWithId(const SketchModel& model, const QString& id) {
  const auto found =
      std::find_if(model.curves.begin(), model.curves.end(), [&id](const CurveData& c) { return c.id == id; });
  return found == model.curves.end() ? nullptr : &*found;
}

} // namespace

QString placePick(const V2& at) {
  return QStringLiteral("@%1,%2").arg(at.x, 0, 'g', 17).arg(at.y, 0, 'g', 17);
}

std::optional<V2> pickedPlace(const QString& id) {
  if (!id.startsWith(QLatin1Char('@'))) {
    return std::nullopt;
  }
  const QStringList parts = id.mid(1).split(QLatin1Char(','));
  if (parts.size() != 2) {
    return std::nullopt;
  }
  bool okX = false;
  bool okY = false;
  const double x = parts.at(0).toDouble(&okX);
  const double y = parts.at(1).toDouble(&okY);
  if (!okX || !okY) {
    return std::nullopt;
  }
  return V2{x, y};
}

std::optional<V2> pickPlace(const SketchModel& model, const QString& id) {
  if (const PointData* p = pointWithId(model, id)) {
    return p->at;
  }
  return pickedPlace(id);
}

std::optional<V2> middleOf(const SketchModel& model, const QStringList& entities) {
  bool any = false;
  V2 low{0.0, 0.0};
  V2 high{0.0, 0.0};
  const auto include = [&any, &low, &high](const V2& p) {
    low = any ? V2{std::min(low.x, p.x), std::min(low.y, p.y)} : p;
    high = any ? V2{std::max(high.x, p.x), std::max(high.y, p.y)} : p;
    any = true;
  };
  for (const QString& id : entities) {
    if (const PointData* p = pointWithId(model, id)) {
      include(p->at);
    } else if (const CurveData* c = curveWithId(model, id)) {
      if (c->isCircle()) {
        const V2 corner{c->curve.radius, c->curve.radius};
        include(c->curve.center - corner);
        include(c->curve.center + corner);
      }
      for (const QString& point : c->points) {
        if (const PointData* q = pointWithId(model, point)) {
          include(q->at);
        }
      }
    }
  }
  if (!any) {
    return std::nullopt;
  }
  return V2{(low.x + high.x) / 2.0, (low.y + high.y) / 2.0};
}

} // namespace mitcad::sketch
