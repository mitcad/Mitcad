// SPDX-License-Identifier: MIT
// Places in the edited sketch for Move/Copy (mitcad#1563): the pick ids of
// places the sketch's snaps found where no point is (Point to Point's
// Origin and Target Point), and the middle of the objects, where Translate's
// handle sits and Rotate turns without a Pivot Point; the texts among the
// objects and where a text's anchor is (mitcad#1911). On the sketch model's
// data only: plain Qt Core.
#include "SketchModel.hpp"

#include <algorithm>
#include <cmath>

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

const TextData* textWithId(const SketchModel& model, const QString& id) {
  const auto found =
      std::find_if(model.texts.begin(), model.texts.end(), [&id](const TextData& t) { return t.id == id; });
  return found == model.texts.end() ? nullptr : &*found;
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
    } else if (const TextData* t = textWithId(model, id)) {
      // A text by its anchor, a text in a frame by the frame's corners
      // (mitcad#1911).
      if (!t->frame.isEmpty()) {
        for (const QString& point : t->frame) {
          if (const PointData* q = pointWithId(model, point)) {
            include(q->at);
          }
        }
      } else if (t->path.isEmpty()) {
        include(t->at);
      }
    }
  }
  if (!any) {
    return std::nullopt;
  }
  return V2{(low.x + high.x) / 2.0, (low.y + high.y) / 2.0};
}

MoveObjects moveObjects(const SketchModel& model, const QStringList& objects) {
  MoveObjects split;
  for (const QString& id : objects) {
    QStringList& list = textWithId(model, id) != nullptr ? split.texts : split.entities;
    if (!list.contains(id)) {
      list << id;
    }
  }
  return split;
}

QString textOnPath(const SketchModel& model, const QStringList& objects) {
  for (const QString& id : objects) {
    const TextData* t = textWithId(model, id);
    if (t != nullptr && !t->path.isEmpty()) {
      return id;
    }
  }
  return QString();
}

std::optional<V2> textAnchor(const SketchModel& model, const QString& id) {
  const TextData* t = textWithId(model, id);
  if (t == nullptr || !t->path.isEmpty()) {
    return std::nullopt;
  }
  if (t->frame.isEmpty()) {
    return t->at;
  }
  if (const PointData* corner = pointWithId(model, t->frame.first())) {
    return corner->at;
  }
  return std::nullopt;
}

std::optional<V2> freeTextPosition(const TextData& text) {
  if (!text.frame.isEmpty() || !text.path.isEmpty()) {
    return std::nullopt;
  }
  return text.at;
}

std::optional<V2> changedTextPosition(const TextData& text, double x, double y) {
  const auto at = freeTextPosition(text);
  if (!at || !std::isfinite(x) || !std::isfinite(y)) {
    return std::nullopt;
  }
  // The panel shows the place to 12 digits: unchanged within that.
  const auto same = [](double a, double b) { return std::abs(a - b) <= 1e-9 * std::max(1.0, std::abs(b)); };
  if (same(x, at->x) && same(y, at->y)) {
    return std::nullopt;
  }
  return V2{x, y};
}

} // namespace mitcad::sketch
