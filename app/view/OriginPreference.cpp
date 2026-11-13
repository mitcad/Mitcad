// SPDX-License-Identifier: MIT
#include "view/OriginPreference.hpp"

#include <cmath>
#include <initializer_list>

#include <QJsonArray>
#include <QSettings>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <Standard_Failure.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

namespace mitcad {
namespace {

constexpr const char* kSizeKey = "view/originSize";
constexpr const char* kMillimetresKey = "view/originSizeMm";
constexpr const char* kStyleKey = "view/originStyle";

// Three numbers of the datums query; nullopt for anything else.
std::optional<gp_XYZ> xyzOf(const QJsonValue& value) {
  const QJsonArray array = value.toArray();
  if (array.size() != 3) {
    return std::nullopt;
  }
  const gp_XYZ xyz(array.at(0).toDouble(), array.at(1).toDouble(), array.at(2).toDouble());
  if (!std::isfinite(xyz.X()) || !std::isfinite(xyz.Y()) || !std::isfinite(xyz.Z())) {
    return std::nullopt;
  }
  return xyz;
}

} // namespace

QString originSizeId(OriginSize size) {
  return size == OriginSize::Model ? QStringLiteral("model") : QStringLiteral("screen");
}

std::optional<OriginSize> originSizeFromId(const QString& id) {
  if (id == QLatin1String("screen")) {
    return OriginSize::Screen;
  }
  if (id == QLatin1String("model")) {
    return OriginSize::Model;
  }
  return std::nullopt;
}

QString originStyleId(OriginStyle style) {
  return style == OriginStyle::Centred ? QStringLiteral("centred") : QStringLiteral("corner");
}

std::optional<OriginStyle> originStyleFromId(const QString& id) {
  if (id == QLatin1String("corner")) {
    return OriginStyle::Corner;
  }
  if (id == QLatin1String("centred")) {
    return OriginStyle::Centred;
  }
  return std::nullopt;
}

OriginDisplay OriginDisplay::load(const QSettings& settings) {
  OriginDisplay display;
  display.size = originSizeFromId(settings.value(QLatin1String(kSizeKey)).toString()).value_or(OriginSize::Screen);
  display.style =
      originStyleFromId(settings.value(QLatin1String(kStyleKey)).toString()).value_or(OriginStyle::Corner);
  bool number = false;
  const double millimetres = settings.value(QLatin1String(kMillimetresKey)).toDouble(&number);
  if (number && std::isfinite(millimetres) && millimetres >= kOriginMinMillimetres &&
      millimetres <= kOriginMaxMillimetres) {
    display.millimetres = millimetres;
  }
  return display;
}

void OriginDisplay::save(QSettings& settings) const {
  settings.setValue(QLatin1String(kSizeKey), originSizeId(size));
  settings.setValue(QLatin1String(kMillimetresKey), millimetres);
  settings.setValue(QLatin1String(kStyleKey), originStyleId(style));
}

QString OriginDisplay::describe() const {
  const QString sized = size == OriginSize::Model
                            ? QStringLiteral("model size %1 mm").arg(millimetres, 0, 'g', 6)
                            : QStringLiteral("screen size");
  return QStringLiteral("%1, %2 style").arg(sized, originStyleId(style));
}

OriginDisplay savedOriginDisplay() { return OriginDisplay::load(QSettings()); }

bool saveOriginDisplay(const OriginDisplay& display) {
  if (savedOriginDisplay() == display) {
    return false;
  }
  {
    QSettings settings;
    display.save(settings);
  }
  emit OriginDisplayNotifier::instance().changed();
  return true;
}

OriginDisplayNotifier& OriginDisplayNotifier::instance() {
  static OriginDisplayNotifier notifier;
  return notifier;
}

gp_Dir positiveWay(const gp_Dir& direction) {
  const gp_XYZ xyz = direction.XYZ();
  double largest = xyz.X();
  for (const double value : {xyz.Y(), xyz.Z()}) {
    if (std::abs(value) > std::abs(largest)) {
      largest = value;
    }
  }
  return largest < 0.0 ? direction.Reversed() : direction;
}

TopoDS_Shape originDatumShape(const QJsonObject& datum, OriginStyle style, double size) {
  if (!(size > 0.0) || !std::isfinite(size)) {
    return TopoDS_Shape();
  }
  try {
    const QString type = datum.value(QStringLiteral("type")).toString();
    if (type == QLatin1String("plane")) {
      const auto origin = xyzOf(datum.value(QStringLiteral("origin")));
      const auto normal = xyzOf(datum.value(QStringLiteral("normal")));
      const auto xAxis = xyzOf(datum.value(QStringLiteral("x_axis")));
      if (!origin || !normal || !xAxis) {
        return TopoDS_Shape();
      }
      const gp_Ax3 frame{gp_Pnt{*origin}, gp_Dir{*normal}, gp_Dir{*xAxis}};
      if (style == OriginStyle::Centred) {
        const double half = size / 2.0;
        return BRepBuilderAPI_MakeFace(gp_Pln(frame), -half, half, -half, half).Face();
      }
      // From the origin into the quadrant of the positive ways: a square
      // face over the frame's axes, each turned to its positive way.
      const gp_Vec u = gp_Vec(positiveWay(frame.XDirection())) * size;
      const gp_Vec v = gp_Vec(positiveWay(frame.YDirection())) * size;
      const gp_Pnt corner(*origin);
      BRepBuilderAPI_MakePolygon outline(corner, corner.Translated(u), corner.Translated(u + v),
                                         corner.Translated(v), true);
      return BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
    }
    if (type == QLatin1String("axis")) {
      const auto origin = xyzOf(datum.value(QStringLiteral("origin")));
      const auto direction = xyzOf(datum.value(QStringLiteral("direction")));
      if (!origin || !direction) {
        return TopoDS_Shape();
      }
      const gp_Pnt at(*origin);
      if (style == OriginStyle::Centred) {
        const gp_Vec half = gp_Vec(gp_Dir(*direction)) * (size / 2.0);
        return BRepBuilderAPI_MakeEdge(at.Translated(-half), at.Translated(half)).Edge();
      }
      const gp_Vec reach = gp_Vec(positiveWay(gp_Dir(*direction))) * (size * kCornerAxisReach);
      return BRepBuilderAPI_MakeEdge(at, at.Translated(reach)).Edge();
    }
    if (type == QLatin1String("point")) {
      const auto point = xyzOf(datum.value(QStringLiteral("point")));
      return point ? TopoDS_Shape(BRepBuilderAPI_MakeVertex(gp_Pnt(*point)).Vertex()) : TopoDS_Shape();
    }
  } catch (const Standard_Failure&) {
    // A degenerate datum (zero vectors) is not drawn.
  }
  return TopoDS_Shape();
}

} // namespace mitcad
