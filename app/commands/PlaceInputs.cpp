// SPDX-License-Identifier: MIT
// Placement by picked geometry (mitcad#656): the panel inputs every part
// placed by a pick shares (PlaceInputs.hpp).
#include "PlaceInputs.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <exception>
#include <limits>
#include <memory>
#include <set>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pln.hxx>
#include <gp_Vec.hxx>

#include <QObject>
#include <QStringList>
#include <QtLogging>

#include "../framework/HandleStep.hpp"
#include "../framework/ModelShapes.hpp"
#include "../framework/Theme.hpp"
#include "PlaceGeometry.hpp"

namespace mitcad::cmd {
namespace {

constexpr double kPi = 3.14159265358979323846;

// The inputs' keys.
const QString kAt = QStringLiteral("at");
const QString kEdge1 = QStringLiteral("at_edge1");
const QString kOffset1 = QStringLiteral("at_offset1");
const QString kEdge2 = QStringLiteral("at_edge2");
const QString kOffset2 = QStringLiteral("at_offset2");
const QString kOffset = QStringLiteral("at_offset");
const QString kFlip = QStringLiteral("at_flip");
const QString kAngle = QStringLiteral("at_angle");
const QString kAlign = QStringLiteral("at_align");
const QString kClearance = QStringLiteral("bore_clearance");
// Kept beside the inputs: the item placed on and its form, where it was
// picked (`near`), the reference lines' positive sides, the point on a
// face, a point's plane, where the feature's component is seen, values
// the panel does not show (an earlier definition's).
const QString kItem = QStringLiteral("at_item");
const QString kForm = QStringLiteral("at_form");
const QString kPick = QStringLiteral("at_pick");
const QString kSides = QStringLiteral("at_sides");
const QString kPoint = QStringLiteral("at_point");
const QString kPlane = QStringLiteral("at_plane");
const QString kContext = QStringLiteral("at_context");
const QString kExtra = QStringLiteral("at_extra");
// The More that folds Offset, Flip, Angle and Align away; whether Align
// is Reference 1's by default (the part's zero along a visible line,
// mitcad#656); whether the panel turns the part at all.
const QString kMore = QStringLiteral("at_more");
const QString kAlignAuto = QStringLiteral("at_align_auto");
// A face placement loaded without `align_side` (written before it): its
// line's direction kept as it is, so that it opens and saves unchanged;
// a new Place On or Align pick drops it (mitcad#656).
const QString kNoAlignSide = QStringLiteral("at_no_align_side");
// A face placement loaded without `align` (`align` is optional in `at`):
// saved without one, not with Reference 1's line, so that an unchanged OK
// leaves the part as it was; a new Place On, Reference or Align pick drops
// it (mitcad#761).
const QString kNoAlign = QStringLiteral("at_no_align");
// The references' colours whatever comes before them (mitcad#656).
const QColor kReference1Color = commandInputColor(2);
const QColor kReference2Color = commandInputColor(3);
const QString kTurnsPanel = QStringLiteral("at_turns");
// An earlier definition's own placement fields (`plane` and `center`,
// `belt_with`, `mesh_with` with its angle) and the `at` the inputs made of
// them on load: saved unchanged while the inputs still make that `at`.
const QString kLegacy = QStringLiteral("at_legacy");
// A vertex's or construction point's plane, in sight as a row of its own
// (mitcad#656): the origin plane facing the viewer at the pick, re-picked
// at will.
const QString kOnPlane = QStringLiteral("at_on_plane");

constexpr SelectFilter kPlaceKinds = SelectKind::Face | SelectKind::Edge | SelectKind::Plane | SelectKind::Axis |
                                     SelectKind::Point | SelectKind::Vertex | SelectKind::SketchPoint |
                                     SelectKind::SketchCurve;
constexpr SelectFilter kLineKinds = SelectKind::Edge | SelectKind::Axis | SelectKind::SketchCurve;

// --- Items and forms

QJsonArray json(const gp_Pnt& p) { return QJsonArray{p.X(), p.Y(), p.Z()}; }
QJsonArray json(const gp_Dir& d) { return QJsonArray{d.X(), d.Y(), d.Z()}; }
std::optional<gp_Dir> dirOf(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  if (a.size() != 3) {
    return std::nullopt;
  }
  const gp_Vec v(a[0].toDouble(), a[1].toDouble(), a[2].toDouble());
  if (v.Magnitude() < 1e-12) {
    return std::nullopt;
  }
  return gp_Dir(v);
}
std::optional<gp_Pnt> pointIn(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  if (a.size() != 3) {
    return std::nullopt;
  }
  return gp_Pnt(a[0].toDouble(), a[1].toDouble(), a[2].toDouble());
}

QJsonObject itemJson(const SelectionItem& item) {
  return {{QStringLiteral("kind"), static_cast<double>(static_cast<unsigned>(item.kind))},
          {QStringLiteral("owner"), item.owner},
          {QStringLiteral("name"), item.name},
          {QStringLiteral("geometry"), item.geometry},
          {QStringLiteral("occurrence"), item.occurrence}};
}
SelectionItem itemFromJson(const QJsonObject& o) {
  SelectionItem item;
  item.kind = static_cast<SelectKind>(static_cast<unsigned>(o.value(QStringLiteral("kind")).toDouble()));
  item.owner = o.value(QStringLiteral("owner")).toString();
  item.name = o.value(QStringLiteral("name")).toString();
  item.geometry = o.value(QStringLiteral("geometry")).toString();
  item.occurrence = o.value(QStringLiteral("occurrence")).toString();
  return item;
}

bool isLineItem(const SelectionItem& item) {
  return item.kind == SelectKind::Axis ||
         ((item.kind == SelectKind::Edge || item.kind == SelectKind::SketchCurve) && isLine(item));
}

bool isOriginPlane(const QString& owner) {
  return owner == QStringLiteral("xy") || owner == QStringLiteral("xz") || owner == QStringLiteral("yz");
}

// The form a pick places by; None for what it does not take (a straight
// edge of a body, a curved face other than a cylinder).
PlaceForm formOfItem(const SelectionItem& item) {
  switch (item.kind) {
  case SelectKind::Plane:
    return PlaceForm::Face;
  case SelectKind::Face:
    return item.geometry == QStringLiteral("plane")      ? PlaceForm::Face
           : item.geometry == QStringLiteral("cylinder") ? PlaceForm::Axis
                                                         : PlaceForm::None;
  case SelectKind::Edge:
    return isCircular(item) ? PlaceForm::Axis : PlaceForm::None;
  case SelectKind::Axis:
    return PlaceForm::Axis;
  case SelectKind::SketchCurve:
    return isLine(item) || isCircular(item) ? PlaceForm::Axis : PlaceForm::None;
  case SelectKind::Point:
  case SelectKind::Vertex:
  case SelectKind::SketchPoint:
    return PlaceForm::Point;
  default:
    return PlaceForm::None;
  }
}

QString formName(PlaceForm form) {
  return form == PlaceForm::Axis ? QStringLiteral("axis")
         : form == PlaceForm::Point ? QStringLiteral("point")
         : form == PlaceForm::Face  ? QStringLiteral("face")
                                    : QString();
}

PlaceForm storedForm(const CommandState& state) {
  const QString form = state.object(kForm).value(QStringLiteral("form")).toString();
  return form == QStringLiteral("axis")    ? PlaceForm::Axis
         : form == QStringLiteral("point") ? PlaceForm::Point
         : form == QStringLiteral("face")  ? PlaceForm::Face
                                           : PlaceForm::None;
}

bool offeredIn(const PlacePanel& panel, const CommandState& state) {
  return !panel.offered || panel.offered(state);
}

double roundTo(double value, double step) { return std::round(value / step) * step; }

// The step a dimension's drag rounds to at a pixel's size (mm): the
// overlay's own rule (framework/HandleStep.hpp) for an arrow without a
// scale whose value does not count as one (Manipulator::valueScale off on
// the dimensions), the zoom's step, so that a click and a drag round alike
// (mitcad#1431).
double dragStep(double pixel) {
  HandleStepInput input;
  input.zoom = 3.0 * pixel;
  return handleStep(input);
}

// --- Components (mitcad#578): a shaft of another
// component is a seat with its occurrence link.

QJsonArray occurrencesOf(const CommandContext& context) {
  try {
    return context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
        .value(QStringLiteral("occurrences"))
        .toArray();
  } catch (...) {
    return QJsonArray();
  }
}

// The first path to `component` in a depth-first walk of the occurrences.
QString pathTo(const QJsonArray& level, const QString& parent, const QString& component) {
  for (const QJsonValue& value : level) {
    const QJsonObject occurrence = value.toObject();
    const QString uid = occurrence.value(QStringLiteral("uid")).toString();
    const QString path = parent.isEmpty() ? uid : parent + QLatin1Char('/') + uid;
    if (occurrence.value(QStringLiteral("component")).toString() == component) {
      return path;
    }
    const QString child = pathTo(occurrence.value(QStringLiteral("children")).toArray(), path, component);
    if (!child.isEmpty()) {
      return child;
    }
  }
  return QString();
}

// The component at the end of an occurrence path ("C0" for the root).
QString componentAt(const QJsonArray& occurrences, const QString& path) {
  QString component = QStringLiteral("C0");
  QJsonArray level = occurrences;
  for (const QString& uid : path.split(QLatin1Char('/'), Qt::SkipEmptyParts)) {
    bool found = false;
    for (const QJsonValue& value : level) {
      const QJsonObject occurrence = value.toObject();
      if (occurrence.value(QStringLiteral("uid")).toString() == uid) {
        component = occurrence.value(QStringLiteral("component")).toString();
        level = occurrence.value(QStringLiteral("children")).toArray();
        found = true;
        break;
      }
    }
    if (!found) {
      return QString();
    }
  }
  return component;
}

QString setContext(CommandState& state, const CommandContext& context, const QString& component,
                   const QString& path) {
  QString seen;
  if (component != QStringLiteral("C0")) {
    seen = path.isEmpty() ? pathTo(occurrencesOf(context), QString(), component) : path;
  }
  state.setObject(kContext, {{QStringLiteral("component"), component}, {QStringLiteral("path"), seen}});
  return seen;
}

QString seenPath(const CommandState& state) {
  return state.object(kContext).value(QStringLiteral("path")).toString();
}

// The link of a shaft picked in another component than the feature's;
// empty for one of its own.
QJsonObject linkOf(const CommandState& state, const CommandContext& context, const SelectionItem& item) {
  if (item.kind != SelectKind::Face && item.kind != SelectKind::Edge) {
    return QJsonObject();
  }
  const QJsonObject where = state.object(kContext);
  const QString own = where.value(QStringLiteral("component")).toString(QStringLiteral("C0"));
  const QString component = componentAt(occurrencesOf(context), item.occurrence);
  if (component.isEmpty() || component == own) {
    return QJsonObject();
  }
  QJsonObject link{{QStringLiteral("source"), item.occurrence}};
  const QString target = where.value(QStringLiteral("path")).toString();
  if (!target.isEmpty()) {
    link.insert(QStringLiteral("target"), target);
  }
  return link;
}

// --- Geometry of faces and lines (the feature's component's coordinates)

// A face's plane as the model frames it: the origin projected onto it, x
// along +X projected (+Y when it faces X), the outward normal.
std::optional<PlaneFrame> framed(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face);
  if (surface.GetType() != GeomAbs_Plane) {
    return std::nullopt;
  }
  const gp_Pln plane = surface.Plane();
  gp_Dir normal = plane.Axis().Direction();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  PlaneFrame frame;
  const gp_Pnt on = plane.Location();
  frame.origin = gp_Pnt(normal.XYZ() * gp_Vec(gp_Pnt(0, 0, 0), on).Dot(gp_Vec(normal)));
  gp_Vec x = gp_Vec(1, 0, 0) - gp_Vec(normal) * normal.X();
  if (x.Magnitude() < 1e-9) {
    x = gp_Vec(0, 1, 0) - gp_Vec(normal) * normal.Y();
  }
  frame.normal = normal;
  frame.x = gp_Dir(x);
  frame.y = normal.Crossed(frame.x);
  return frame;
}

// The x axis a frame takes without one: +X along the plane, else +Y.
gp_Dir defaultX(const gp_Dir& normal) {
  gp_Vec x = gp_Vec(1, 0, 0) - gp_Vec(normal) * normal.X();
  if (x.Magnitude() < 1e-9) {
    x = gp_Vec(0, 1, 0) - gp_Vec(normal) * normal.Y();
  }
  return gp_Dir(x);
}

std::optional<TopoDS_Face> bodyFace(const SelectionItem& item, const CommandContext& context,
                                    std::shared_ptr<geometry::Shape>* body = nullptr) {
  const auto shape = context.bodyShape(item.owner);
  if (!shape) {
    return std::nullopt;
  }
  const auto found = shape->find_faces(item.name.toStdString());
  if (found.empty()) {
    return std::nullopt;
  }
  if (body != nullptr) {
    *body = shape;
  }
  return shape->face(found.front());
}

struct FaceInfo {
  PlaneFrame frame;
  // A body face's straight edges, with their references.
  std::vector<std::pair<QJsonValue, place::Line>> edges;
  // What a click snaps to: vertices, centres, edges' middles.
  std::vector<gp_Pnt> snaps;
  gp_Pnt middle;
  bool body = false;   // a body's face: sides toward its middle
  bool origin = false; // an origin plane: distances without edges
};

std::optional<FaceInfo> faceInfo(const SelectionItem& item, const CommandContext& context, bool full) {
  FaceInfo info;
  if (item.kind == SelectKind::Plane) {
    const auto frame = frameOf(item, context);
    if (!frame) {
      return std::nullopt;
    }
    info.frame = *frame;
    info.origin = isOriginPlane(item.owner);
    info.middle = frame->origin;
    info.snaps = {frame->origin};
    return info;
  }
  if (item.kind != SelectKind::Face) {
    return std::nullopt;
  }
  std::shared_ptr<geometry::Shape> body;
  const auto face = bodyFace(item, context, &body);
  if (!face) {
    return std::nullopt;
  }
  const auto frame = framed(*face);
  if (!frame) {
    return std::nullopt;
  }
  info.frame = *frame;
  info.body = true;
  Bnd_Box box;
  BRepBndLib::Add(*face, box);
  info.middle = box.IsVoid() ? frame->origin : gp_Pnt((box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0);
  if (!full) {
    return info;
  }
  std::set<int> seen;
  for (TopExp_Explorer edges(*face, TopAbs_EDGE); edges.More(); edges.Next()) {
    const int index = body->edge_index(edges.Current());
    if (index < 0 || !seen.insert(index).second) {
      continue;
    }
    const BRepAdaptor_Curve curve(TopoDS::Edge(edges.Current()));
    if (curve.GetType() == GeomAbs_Line) {
      const gp_Pnt a = curve.Value(curve.FirstParameter());
      const gp_Pnt b = curve.Value(curve.LastParameter());
      if (a.Distance(b) < 1e-9) {
        continue;
      }
      info.snaps.push_back(gp_Pnt((a.XYZ() + b.XYZ()) / 2.0));
      const std::string& name = body->edge_name(index);
      if (!name.empty()) {
        info.edges.emplace_back(QJsonObject{{QStringLiteral("body"), item.owner},
                                            {QStringLiteral("edge"), QString::fromStdString(name)}},
                                place::Line{a, gp_Dir(gp_Vec(a, b)), a.Distance(b)});
      }
    } else if (curve.GetType() == GeomAbs_Circle) {
      info.snaps.push_back(curve.Circle().Location());
    }
  }
  for (TopExp_Explorer vertices(*face, TopAbs_VERTEX); vertices.More(); vertices.Next()) {
    info.snaps.push_back(BRep_Tool::Pnt(TopoDS::Vertex(vertices.Current())));
  }
  return info;
}

// A line item as a line: an axis without ends, a straight edge or sketch
// line as its segment.
std::optional<place::Line> lineOf(const SelectionItem& item, const CommandContext& context) {
  if (item.kind == SelectKind::Axis) {
    const auto axis = axisOf(item, context);
    if (!axis) {
      return std::nullopt;
    }
    return place::Line{axis->first, axis->second, -1.0};
  }
  TopoDS_Shape shape;
  if (item.kind == SelectKind::Edge) {
    if (const auto body = context.bodyShape(item.owner)) {
      const auto found = body->find_edges(item.name.toStdString());
      if (!found.empty()) {
        shape = body->edge(found.front());
      }
    }
  }
  if (shape.IsNull()) {
    shape = context.itemShape(item);
  }
  TopExp_Explorer edges(shape, TopAbs_EDGE);
  if (!edges.More()) {
    return std::nullopt;
  }
  const BRepAdaptor_Curve curve(TopoDS::Edge(edges.Current()));
  if (curve.GetType() != GeomAbs_Line) {
    return std::nullopt;
  }
  const gp_Pnt a = curve.Value(curve.FirstParameter());
  const gp_Pnt b = curve.Value(curve.LastParameter());
  if (a.Distance(b) < 1e-9) {
    return std::nullopt;
  }
  return place::Line{a, gp_Dir(gp_Vec(a, b)), a.Distance(b)};
}

std::optional<place::Line> lineIn(const SelectionItem& item, const PlaneFrame& frame, const CommandContext& context) {
  const auto line = lineOf(item, context);
  if (!line) {
    return std::nullopt;
  }
  return place::inPlane(*line, frame.origin, frame.normal);
}

// The origin axis most nearly along a direction.
QString axisAlong(const gp_Dir& direction) {
  const double x = std::abs(direction.X());
  const double y = std::abs(direction.Y());
  const double z = std::abs(direction.Z());
  return x >= y && x >= z ? QStringLiteral("x") : y >= z ? QStringLiteral("y") : QStringLiteral("z");
}

// A definition's reference as an item: a fixed plane as an item of its own
// (mitcad#1137), seen in `occurrence`; anything else as itemOf gives it.
SelectionItem placeItemOf(const QJsonValue& ref, const CommandContext& context,
                          const QString& occurrence = QString()) {
  return isFixedPlaneRef(ref) ? fixedPlaneItem(ref.toObject(), occurrence) : itemOf(ref, context);
}

// What the placement row and the log say of a fixed plane (mitcad#1137):
// "Plane at Z 36 mm (fixed)" square to an origin axis, else the point it
// runs through and its normal.
QString fixedPlaneText(const SelectionItem& item, const CommandContext& context) {
  const QJsonObject ref = item.reference().toObject();
  const auto origin = pointIn(ref.value(QStringLiteral("origin")));
  const auto normal = dirOf(ref.value(QStringLiteral("normal")));
  if (!origin || !normal) {
    return QObject::tr("Fixed plane");
  }
  // Rounded, and never "-0 mm".
  const auto length = [&context](double value) { return lengthText(roundTo(value, 1e-6) + 0.0, context); };
  const QString axis = axisAlong(*normal);
  const bool alongX = axis == QStringLiteral("x");
  const bool alongY = axis == QStringLiteral("y");
  if (std::abs(alongX ? normal->X() : alongY ? normal->Y() : normal->Z()) >= 1.0 - 1e-9) {
    return QObject::tr("Plane at %1 %2 (fixed)")
        .arg(axis.toUpper(), length(alongX ? origin->X() : alongY ? origin->Y() : origin->Z()));
  }
  const auto component = [](double value) { return number(roundTo(value, 1e-4) + 0.0); };
  return QObject::tr("Plane through (%1, %2, %3), normal (%4, %5, %6) (fixed)")
      .arg(length(origin->X()), length(origin->Y()), length(origin->Z()), component(normal->X()),
           component(normal->Y()), component(normal->Z()));
}

// The two origin axes most nearly square to a normal, the one more so first.
std::pair<QString, QString> originAxesAcross(const gp_Dir& normal) {
  std::array<std::pair<double, QString>, 3> axes{
      std::make_pair(std::abs(normal.X()), QStringLiteral("x")),
      std::make_pair(std::abs(normal.Y()), QStringLiteral("y")),
      std::make_pair(std::abs(normal.Z()), QStringLiteral("z"))};
  std::sort(axes.begin(), axes.end(), [](const auto& a, const auto& b) { return a.first < b.first; });
  return {axes[0].second, axes[1].second};
}

// --- The point on a face

struct Sides {
  gp_Dir first;
  gp_Dir second;
  bool implicit = false; // an origin plane's own axes: distances without edges
};

std::optional<Sides> sidesOf(const CommandState& state) {
  const QJsonObject o = state.object(kSides);
  const auto a = dirOf(o.value(QStringLiteral("s1")));
  const auto b = dirOf(o.value(QStringLiteral("s2")));
  if (!a || !b) {
    return std::nullopt;
  }
  return Sides{*a, *b, o.value(QStringLiteral("implicit")).toBool()};
}

SelectionItem placedItem(const CommandState& state) {
  const Selection& items = state.items(kAt);
  return items.isEmpty() ? SelectionItem() : items.first();
}

// The point on the face from the references, their sides and the
// distances; the one kept last while a value does not evaluate.
std::optional<gp_Pnt> facePoint(const CommandState& state, const CommandContext& context) {
  const auto kept = pointIn(state.object(kPoint).value(QStringLiteral("p")));
  const auto info = faceInfo(placedItem(state), context, false);
  const auto sides = sidesOf(state);
  const Selection& first = state.items(kEdge1);
  const Selection& second = state.items(kEdge2);
  const double v1 = state.value(kOffset1);
  const double v2 = state.value(kOffset2);
  if (!info || !sides || first.isEmpty() || second.isEmpty() || !std::isfinite(v1) || !std::isfinite(v2)) {
    return kept;
  }
  const auto a = lineIn(first.first(), info->frame, context);
  const auto b = lineIn(second.first(), info->frame, context);
  if (!a || !b) {
    return kept;
  }
  const auto point = place::pointAt(*a, sides->first, v1, *b, sides->second, v2, info->frame.normal);
  return point ? point : kept;
}

void keepPoint(CommandState& state, const gp_Pnt& point) {
  state.setObject(kPoint, {{QStringLiteral("p"), json(point)}});
}

void setLength(CommandState& state, const QString& key, double millimetres, const CommandContext& context) {
  state.setText(key, lengthText(millimetres, context));
  state.setValue(key, std::numeric_limits<double>::quiet_NaN());
}

// The reference lines `first` and `second` of the face the inputs are on,
// with the point kept where it is: their sides (toward the face's middle
// on a body face, as the model's `place_frame` confirms; toward `near` on
// a plane) and the distances, rounded to `step`.
bool setReferences(CommandState& state, const CommandContext& context, const SelectionItem& face,
                   const SelectionItem& first, const SelectionItem& second, const gp_Pnt& point,
                   bool implicit, std::optional<gp_Pnt> nearPick, double step) {
  const auto info = faceInfo(face, context, false);
  if (!info) {
    return false;
  }
  const auto a = lineIn(first, info->frame, context);
  const auto b = lineIn(second, info->frame, context);
  if (!a || !b) {
    return false;
  }
  const gp_Dir& normal = info->frame.normal;
  gp_Dir sideA = place::across(*a, normal);
  gp_Dir sideB = place::across(*b, normal);
  QJsonObject pick = state.object(kPick);
  if (implicit) {
    sideA = info->frame.x;
    sideB = info->frame.y;
  } else if (info->body) {
    if (place::signedDistance(*a, sideA, info->middle) < 0.0) {
      sideA.Reverse();
    }
    if (place::signedDistance(*b, sideB, info->middle) < 0.0) {
      sideB.Reverse();
    }
    // The model's own side of each line: a point 1 mm from both.
    try {
      const QJsonObject probe{{QStringLiteral("face"), face.reference()},
                              {QStringLiteral("edge1"), first.reference()},
                              {QStringLiteral("offset1"), 1.0},
                              {QStringLiteral("edge2"), second.reference()},
                              {QStringLiteral("offset2"), 1.0}};
      const auto origin = pointIn(context.queryObject({{QStringLiteral("query"), QStringLiteral("place_frame")},
                                                       {QStringLiteral("at"), probe}})
                                      .value(QStringLiteral("origin")));
      if (origin) {
        if (place::signedDistance(*a, sideA, *origin) < 0.0) {
          sideA.Reverse();
        }
        if (place::signedDistance(*b, sideB, *origin) < 0.0) {
          sideB.Reverse();
        }
      }
    } catch (...) {
      // Toward the middle then.
    }
    pick.remove(QStringLiteral("near"));
  } else {
    // A plane: the side `near` lies on, off both lines.
    gp_Pnt where = nearPick.value_or(point);
    where.Translate(-gp_Vec(normal) * gp_Vec(info->frame.origin, where).Dot(gp_Vec(normal)));
    if (std::abs(place::signedDistance(*a, sideA, where)) < 1e-4) {
      where.Translate(gp_Vec(sideA) * 0.01);
    }
    if (std::abs(place::signedDistance(*b, sideB, where)) < 1e-4) {
      where.Translate(gp_Vec(sideB) * 0.01);
    }
    if (place::signedDistance(*a, sideA, where) < 0.0) {
      sideA.Reverse();
    }
    if (place::signedDistance(*b, sideB, where) < 0.0) {
      sideB.Reverse();
    }
    pick.insert(QStringLiteral("near"), json(where));
  }
  state.setObject(kPick, pick);
  const double v1 = roundTo(place::signedDistance(*a, sideA, point), step);
  const double v2 = roundTo(place::signedDistance(*b, sideB, point), step);
  state.setItems(kEdge1, {first});
  state.setItems(kEdge2, {second});
  setLength(state, kOffset1, v1, context);
  setLength(state, kOffset2, v2, context);
  state.setObject(kSides,
                  {{QStringLiteral("s1"), json(sideA)}, {QStringLiteral("s2"), json(sideB)},
                   {QStringLiteral("implicit"), implicit}});
  keepPoint(state, place::pointAt(*a, sideA, v1, *b, sideB, v2, normal).value_or(point));
  return true;
}

enum class Refs { Nearest, OriginAxes };

SelectionItem originAxis(const QString& name, const CommandContext& context) {
  return itemOf(QJsonValue(name), context);
}

// Places the inputs on a face or a plane at a point: snapped to the face's
// vertices, centres and edges' middles (`snapIt`), measured from the two
// nearest straight edges of a body's face, an origin plane's own axes, or
// the origin axes along it.
bool placeOnFace(CommandState& state, const CommandContext& context, const SelectionItem& face, gp_Pnt point,
                 bool snapIt, Refs refs, std::optional<gp_Pnt> nearPick) {
  const auto info = faceInfo(face, context, true);
  if (!info) {
    return false;
  }
  const gp_Vec n(info->frame.normal);
  point.Translate(-n * gp_Vec(info->frame.origin, point).Dot(n));
  // A click rounds its distances to the step a drag takes at this zoom
  // (mitcad#656: 40 mm, not 39.82); a snap keeps them exact.
  const double pixel = context.pixelSize(point);
  double step = !snapIt ? 1e-6 : std::isfinite(pixel) && pixel > 0.0 ? dragStep(pixel) : 0.01;
  if (snapIt) {
    const double tolerance = std::isfinite(pixel) && pixel > 0.0 ? 10.0 * pixel : 0.5;
    if (const auto snapped = place::snap(point, info->snaps, tolerance)) {
      point = *snapped;
      step = 1e-6;
    }
  }
  SelectionItem first;
  SelectionItem second;
  bool implicit = false;
  if (info->origin) {
    // An origin plane: from its own axes, distances along its x and y.
    first = originAxis(axisAlong(info->frame.y), context);
    second = originAxis(axisAlong(info->frame.x), context);
    implicit = true;
  } else if (refs == Refs::Nearest && info->edges.size() >= 2) {
    std::vector<place::Line> lines;
    for (const auto& edge : info->edges) {
      lines.push_back(place::inPlane(edge.second, info->frame.origin, info->frame.normal).value_or(edge.second));
    }
    if (const auto pair = place::nearestPair(lines, point, kPi / 180.0)) {
      first = itemOf(info->edges[static_cast<std::size_t>(pair->first)].first, context);
      second = itemOf(info->edges[static_cast<std::size_t>(pair->second)].first, context);
      first.occurrence = face.occurrence;
      second.occurrence = face.occurrence;
    }
  }
  if (!first.isValid() || !second.isValid()) {
    const auto axes = originAxesAcross(info->frame.normal);
    first = originAxis(axes.first, context);
    second = originAxis(axes.second, context);
  }
  return setReferences(state, context, face, first, second, point, implicit, nearPick ? nearPick : std::optional<gp_Pnt>(point), step);
}

// The plane a point is placed on when it has none of its own (a vertex, a
// construction point, the origin): the origin plane that faces the viewer
// most.
QString viewerPlane(const SelectionItem& item, const CommandContext& context) {
  gp_Pnt where(0, 0, 0);
  if (item.kind == SelectKind::Vertex) {
    if (const auto body = context.bodyShape(item.owner)) {
      const auto found = body->find_vertices(item.name.toStdString());
      if (!found.empty()) {
        where = BRep_Tool::Pnt(body->vertex(found.front()));
      }
    }
  } else if (item.kind == SelectKind::Point && item.owner != QStringLiteral("origin")) {
    try {
      where = pointOf(context.queryObject({{QStringLiteral("query"), QStringLiteral("datum")},
                                           {QStringLiteral("uid"), item.owner}})
                          .value(QStringLiteral("origin")));
    } catch (...) {
    }
  }
  const auto toward = context.towardViewer(where);
  if (!toward) {
    return QStringLiteral("xy");
  }
  const QString axis = axisAlong(*toward);
  return axis == QStringLiteral("x") ? QStringLiteral("yz") : axis == QStringLiteral("y") ? QStringLiteral("xz")
                                                                                          : QStringLiteral("xy");
}

void logPlace(const CommandState& state, const PlacePanel& panel, const CommandContext& context) {
  const PlaceForm form = storedForm(state);
  const SelectionItem item = placedItem(state);
  if (form == PlaceForm::None || !item.isValid()) {
    qDebug().noquote() << QStringLiteral("Place %1: none").arg(panel.command);
    return;
  }
  QString text = QStringLiteral("Place %1: %2 %3")
                     .arg(panel.command, formName(form),
                          isFixedPlane(item) ? fixedPlaneText(item, context) : item.describe());
  if (form == PlaceForm::Face && !state.items(kEdge1).isEmpty() && !state.items(kEdge2).isEmpty()) {
    text += QStringLiteral(", %1 from %2, %3 from %4")
                .arg(state.text(kOffset1), state.items(kEdge1).first().describe(), state.text(kOffset2),
                     state.items(kEdge2).first().describe());
    if (const auto point = facePoint(state, context)) {
      text += QStringLiteral(" (point %1, %2, %3)")
                  .arg(number(roundTo(point->X(), 1e-6)), number(roundTo(point->Y(), 1e-6)),
                       number(roundTo(point->Z(), 1e-6)));
    }
  }
  const QString plane = state.object(kPlane).value(QStringLiteral("ref")).toString();
  if (!plane.isEmpty()) {
    text += QStringLiteral(", on %1").arg(plane);
  }
  qDebug().noquote() << text;
}

// A new placement from a pick (`click` where it hit, when it has one).
void placeItem(CommandState& state, const CommandContext& context, const SelectionItem& item,
               std::optional<gp_Pnt> click) {
  const PlaceForm form = formOfItem(item);
  state.setObject(kItem, itemJson(item));
  state.setObject(kForm, {{QStringLiteral("form"), formName(form)}});
  state.setObject(kPick, QJsonObject());
  state.setObject(kNoAlignSide, QJsonObject()); // a new pick grows into its face
  state.setObject(kNoAlign, QJsonObject());
  state.setObject(kSides, QJsonObject());
  state.setObject(kPlane, QJsonObject());
  state.setObject(kPoint, QJsonObject());
  if (state.object(kAlignAuto).value(QStringLiteral("flat")).toBool()) {
    // A shaft's flat goes with the shaft.
    state.setItems(kAlign, {});
    state.setObject(kAlignAuto, QJsonObject());
  }
  state.setItems(kEdge1, {});
  state.setItems(kEdge2, {});
  state.setItems(kOnPlane, {});
  switch (form) {
  case PlaceForm::Face: {
    gp_Pnt point;
    if (click) {
      point = *click;
    } else if (const auto info = faceInfo(item, context, false)) {
      point = info->middle;
    }
    placeOnFace(state, context, item, point, click.has_value(), Refs::Nearest, click);
    break;
  }
  case PlaceForm::Axis:
    if (item.kind == SelectKind::Face && click) {
      // Which end of the shaft the offset is measured from.
      state.setObject(kPick, {{QStringLiteral("near"), json(*click)}});
    }
    break;
  case PlaceForm::Point:
    if (item.kind == SelectKind::Vertex || item.kind == SelectKind::Point) {
      const QString plane = viewerPlane(item, context);
      state.setObject(kPlane, {{QStringLiteral("ref"), plane}});
      state.setItems(kOnPlane, {itemOf(QJsonValue(plane), context)});
    }
    break;
  case PlaceForm::None:
    break;
  }
}

// A line picked while the part is on a face: the reference it runs with
// most nearly, the point kept. False when it is no line along the face.
bool rereference(CommandState& state, const CommandContext& context, const SelectionItem& line, int which) {
  const SelectionItem face = placedItem(state);
  const auto info = faceInfo(face, context, false);
  // Picked in a Reference row, the row holds the new line already: the
  // point is the one kept before it.
  const auto point = which < 0 ? facePoint(state, context) : pointIn(state.object(kPoint).value(QStringLiteral("p")));
  if (!info || !point || state.items(kEdge1).isEmpty() || state.items(kEdge2).isEmpty()) {
    return false;
  }
  const auto next = lineOf(line, context);
  if (!next) {
    return false;
  }
  // An axis or a sketch line across the face places the part on it.
  if (line.kind != SelectKind::Edge &&
      std::abs(gp_Vec(next->direction).Dot(gp_Vec(info->frame.normal))) > 1e-6) {
    return false;
  }
  const auto inFace = place::inPlane(*next, info->frame.origin, info->frame.normal);
  if (!inFace) {
    return false;
  }
  SelectionItem first = state.items(kEdge1).first();
  SelectionItem second = state.items(kEdge2).first();
  if (which < 0) {
    const auto a = lineIn(first, info->frame, context);
    const auto b = lineIn(second, info->frame, context);
    which = a && b ? place::replaced(*a, *b, *inFace) : 0;
  }
  (which == 0 ? first : second) = line;
  const auto kept = pointIn(state.object(kPick).value(QStringLiteral("near")));
  // Measured from an origin plane's own axes, the part turns from the
  // plane's x; picked lines are no longer its axes, so an empty Align
  // would turn it to Reference 1 (mitcad#656): the plane's X axis is
  // pinned as Align instead, and nothing turns.
  const auto was = sidesOf(state);
  const bool wasPlaneAxes = was && was->implicit;
  if (!setReferences(state, context, face, first, second, *point, false, kept ? kept : point, 1e-6)) {
    return false;
  }
  if (place::keepsPlaneX(wasPlaneAxes, false, state.items(kAlign).isEmpty()) &&
      state.object(kTurnsPanel).value(QStringLiteral("on")).toBool()) {
    state.setItems(kAlign, {originAxis(axisAlong(info->frame.x), context)});
  }
  return true;
}

// A shaft's flat (mitcad#656): a planar face of the picked shaft's body
// along its axis (the face's normal square to it) that cuts into the
// shaft (nearer the axis than its radius), for a set screw to turn to;
// none without one, or for an axis that is no body's shaft.
std::optional<SelectionItem> shaftFlat(const SelectionItem& shaft, const CommandContext& context) {
  if (shaft.kind != SelectKind::Face && shaft.kind != SelectKind::Edge) {
    return std::nullopt;
  }
  const auto axis = axisOf(shaft, context);
  const auto body = context.bodyShape(shaft.owner);
  const TopoDS_Shape picked = context.itemShape(shaft);
  if (!axis || !body || picked.IsNull()) {
    return std::nullopt;
  }
  double radius = 0.0;
  try {
    if (picked.ShapeType() == TopAbs_FACE) {
      const BRepAdaptor_Surface surface(TopoDS::Face(picked));
      if (surface.GetType() == GeomAbs_Cylinder) {
        radius = surface.Cylinder().Radius();
      }
    } else if (picked.ShapeType() == TopAbs_EDGE) {
      const BRepAdaptor_Curve curve(TopoDS::Edge(picked));
      if (curve.GetType() == GeomAbs_Circle) {
        radius = curve.Circle().Radius();
      }
    }
  } catch (const std::exception&) {
    return std::nullopt;
  }
  if (!(radius > 0.0)) {
    return std::nullopt;
  }
  const gp_Vec along(axis->second);
  for (int i = 0; i < body->face_count(); ++i) {
    if (body->face_names(i).empty()) {
      continue;
    }
    const BRepAdaptor_Surface surface(body->face(i));
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    const gp_Vec normal(plane.Axis().Direction());
    if (std::abs(normal.Dot(along)) > 1e-6) {
      continue; // across the axis: an end, not a flat
    }
    const double distance = std::abs(gp_Vec(plane.Location(), axis->first).Dot(normal));
    if (distance < radius - 1e-6) {
      return SelectionItem{SelectKind::Face, shaft.owner, QString::fromStdString(body->face_names(i).front()),
                           QStringLiteral("plane"), shaft.occurrence};
    }
  }
  return std::nullopt;
}

void picked(const SelectionItem& item, CommandState& state, const CommandContext& context, const PlacePanel& panel) {
  std::optional<gp_Pnt> click;
  if (item.at) {
    click = gp_Pnt((*item.at)[0], (*item.at)[1], (*item.at)[2]);
  }
  if (storedForm(state) == PlaceForm::Face && isLineItem(item)) {
    // A line along the face it is on: a new reference; Place On keeps the
    // face.
    const SelectionItem face = itemFromJson(state.object(kItem));
    state.setItems(kAt, {face});
    if (rereference(state, context, item, -1)) {
      state.setObject(kNoAlign, QJsonObject()); // a new reference: Align is Reference 1 again
      qDebug().noquote() << QStringLiteral("Place %1: %2 is a reference now").arg(panel.command, item.describe());
      logPlace(state, panel, context);
      return;
    }
    if (formOfItem(item) == PlaceForm::None) {
      return; // a straight edge of a body places nothing
    }
    state.setItems(kAt, {item});
  }
  if (formOfItem(item) == PlaceForm::None) {
    // Taken by the input but placing nothing: what was there stays.
    const QJsonObject before = state.object(kItem);
    state.setItems(kAt, before.isEmpty() ? Selection() : Selection{itemFromJson(before)});
    return;
  }
  placeItem(state, context, item, click);
  if (formOfItem(item) == PlaceForm::Axis && panel.turns && state.items(kAlign).isEmpty()) {
    // A shaft with a flat: aligned to it at once (mitcad#656), so that a
    // set screw turns without an edge drawn first.
    if (const auto flat = shaftFlat(item, context)) {
      state.setItems(kAlign, {*flat});
      state.setObject(kAlignAuto, {{QStringLiteral("flat"), true}}); // goes with the shaft
      qDebug().noquote() << QStringLiteral("Place %1: aligned to the flat %2").arg(panel.command, flat->describe());
    }
  }
  logPlace(state, panel, context);
}

// The point moved by a typed or dragged distance: kept for a reference
// picked next.
void pointMoved(CommandState& state, const CommandContext& context) {
  if (storedForm(state) != PlaceForm::Face) {
    return;
  }
  if (const auto point = facePoint(state, context)) {
    keepPoint(state, *point);
  }
}

// The frame the model gives the inputs' `at` without the fields named; for
// a shaft of another component its seat's (mitcad#754), the seat's own
// fields left out likewise and the x axis a plane's without one.
std::optional<QJsonObject> frameWithout(const CommandState& state, const CommandContext& context,
                                        const PlacePanel& panel, const QStringList& without) {
  if (placeForm(state, panel) == PlaceForm::Axis && !linkOf(state, context, placedItem(state)).isEmpty()) {
    QJsonObject def;
    Built ignored;
    if (!addPlace(def, state, context, panel, ignored) || !def.value(QStringLiteral("seat")).isObject()) {
      return std::nullopt;
    }
    QJsonObject seat = def.value(QStringLiteral("seat")).toObject();
    for (const QString& key : without) {
      seat.remove(key);
    }
    QJsonObject frame;
    try {
      frame = context.queryObject({{QStringLiteral("query"), QStringLiteral("seat_frame")}, {QStringLiteral("seat"), seat}});
    } catch (...) {
      return std::nullopt;
    }
    const auto axis = dirOf(frame.value(QStringLiteral("axis")));
    if (axis && !frame.contains(QStringLiteral("x_axis"))) {
      frame.insert(QStringLiteral("x_axis"), json(defaultX(*axis)));
    }
    return frame;
  }
  QJsonObject at = atOf(state, context, panel, true);
  if (at.isEmpty()) {
    return std::nullopt;
  }
  for (const QString& key : without) {
    at.remove(key);
  }
  try {
    return context.queryObject({{QStringLiteral("query"), QStringLiteral("place_frame")}, {QStringLiteral("at"), at}});
  } catch (...) {
    return std::nullopt;
  }
}

// A distance's dimension: from its reference line to the point.
std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)> dimension(int which) {
  return [which](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
    if (storedForm(state) != PlaceForm::Face) {
      return std::nullopt;
    }
    const auto sides = sidesOf(state);
    const auto point = facePoint(state, context);
    const double value = state.value(which == 0 ? kOffset1 : kOffset2);
    if (!sides || !point || !std::isfinite(value)) {
      return std::nullopt;
    }
    const gp_Dir& side = which == 0 ? sides->first : sides->second;
    Manipulator m = arrow(point->Translated(-gp_Vec(side) * value), side);
    m.valueBox = true;
    // Dragged by the zoom's step, as a click rounds it (dragStep,
    // mitcad#1431).
    m.valueScale = false;
    return m;
  };
}

} // namespace

// ---------------------------------------------------------------------------

QVector<InputDef> placeInputs(const PlacePanel& panel) {
  const auto offered = [panel](const CommandState& state) { return offeredIn(panel, state); };
  const auto isPlaced = [panel](const CommandState& state) { return placeForm(state, panel) != PlaceForm::None; };
  const auto onFace = [panel](const CommandState& state) { return placeForm(state, panel) == PlaceForm::Face; };
  const auto turned = [panel, isPlaced](const CommandState& state) {
    return isPlaced(state) && panel.turns && panel.turns(state);
  };
  // Offset, Flip, Angle and Align are folded under More (mitcad#656: the
  // part's own sizes in sight) unless they are in use; their handles stay
  // in the view.
  const auto more = [](const CommandState& state) { return state.checked(kMore); };
  const auto alongShown = [isPlaced, more](const CommandState& state) {
    return isPlaced(state) && (more(state) || nonZero(state, kOffset) || state.checked(kFlip));
  };
  // The zero the angle is measured from is in sight (mitcad#656): Reference
  // 1 on a face, else an edge in Align to; a set screw or keyway then
  // lies at the angle from it. Never from the projected +X.
  const auto zeroShown = [onFace](const CommandState& state) {
    return onFace(state) || !state.items(kAlign).isEmpty();
  };
  const auto angleShown = [turned, more, zeroShown](const CommandState& state) {
    return turned(state) && zeroShown(state) && (more(state) || nonZero(state, kAngle));
  };
  const auto alignShown = [turned, more](const CommandState& state) {
    return turned(state) &&
           (more(state) || !state.items(kAlign).isEmpty());
  };
  // A reference row names what it is measured from: "Edge", "X axis".
  const auto referenceText = [](const Selection& items, const CommandState&, const CommandContext& context) {
    if (items.isEmpty()) {
      return QString();
    }
    const SelectionItem& item = items.first();
    if (item.kind == SelectKind::Axis) {
      const bool origin = item.owner == QStringLiteral("x") || item.owner == QStringLiteral("y") || item.owner == QStringLiteral("z");
      return origin ? QObject::tr("%1 axis").arg(item.owner.toUpper()) : context.featureName(item.owner);
    }
    if (item.kind == SelectKind::Face) {
      return QObject::tr("Face"); // Align's planar face, a shaft's flat
    }
    return item.kind == SelectKind::SketchCurve ? QObject::tr("Sketch line") : QObject::tr("Edge");
  };
  // A definition's fixed plane by where it lies (mitcad#1137), kept until
  // a face or a plane is picked in its place; other picks by their count.
  const auto fixedText = [](const Selection& items, const CommandState&, const CommandContext& context) {
    return items.size() == 1 && isFixedPlane(items.first()) ? fixedPlaneText(items.first(), context) : QString();
  };
  InputDef first = selectionInput(kEdge1, QObject::tr("Reference 1"), kLineKinds, 0, 1)
                       .withAccepts(isLineItem)
                       .withOnChange([panel](CommandState& state, const CommandContext& context) {
                         state.setObject(kNoAlign, QJsonObject()); // picked by the user
                         const Selection& line = state.items(kEdge1);
                         if (!line.isEmpty() && rereference(state, context, line.first(), 0)) {
                           logPlace(state, panel, context);
                         }
                       })
                       .withVisible(onFace);
  first.itemsText = referenceText;
  // The references in the same colours whatever comes before them (Check
  // Belt, the Plane row): green and purple (mitcad#656).
  first.color = kReference1Color;
  InputDef second = selectionInput(kEdge2, QObject::tr("Reference 2"), kLineKinds, 0, 1)
                        .withAccepts(isLineItem)
                        .withOnChange([panel](CommandState& state, const CommandContext& context) {
                          state.setObject(kNoAlign, QJsonObject()); // picked by the user
                          const Selection& line = state.items(kEdge2);
                          if (!line.isEmpty() && rereference(state, context, line.first(), 1)) {
                            logPlace(state, panel, context);
                          }
                        })
                        .withVisible(onFace);
  second.itemsText = referenceText;
  second.color = kReference2Color;
  QVector<InputDef> inputs = {
      selectionInput(kAt, QObject::tr("Place On"), kPlaceKinds, panel.required ? 1 : 0, 1)
          .withAccepts([](const SelectionItem& item) { return formOfItem(item) != PlaceForm::None || isLineItem(item); })
          .withPickRule([](const CommandState&) { return kPlaceKinds; },
                        [](const SelectionItem& item, const CommandState& state) {
                          // A body's straight edge is a reference of a face.
                          return formOfItem(item) != PlaceForm::None || storedForm(state) == PlaceForm::Face;
                        })
          .withPlaces()
          .withOnPick([panel](const SelectionItem& item, CommandState& state, const CommandContext& context) {
            picked(item, state, context, panel);
          })
          .withOnChange([panel](CommandState& state, const CommandContext& context) {
            if (state.items(kAt).isEmpty() && !state.object(kItem).isEmpty()) {
              clearPlace(state);
              logPlace(state, panel, context);
            }
          })
          .withItemsText(fixedText)
          .withVisible(offered),
      // A point's plane (mitcad#656), the guess shown and re-picked at will.
      selectionInput(kOnPlane, QObject::tr("Plane"), SelectKind::Plane | SelectKind::Face, 0, 1)
          .withAccepts([](const SelectionItem& item) {
            return item.kind == SelectKind::Plane ||
                   (item.kind == SelectKind::Face && item.geometry == QStringLiteral("plane"));
          })
          .withOnChange([panel](CommandState& state, const CommandContext& context) {
            const Selection& plane = state.items(kOnPlane);
            if (!plane.isEmpty()) {
              state.setObject(kPlane, {{QStringLiteral("ref"), plane.first().reference()}});
            } else {
              // Emptied: the guess again, the plane facing the viewer.
              const QString guess = viewerPlane(placedItem(state), context);
              state.setObject(kPlane, {{QStringLiteral("ref"), guess}});
              state.setItems(kOnPlane, {itemOf(QJsonValue(guess), context)});
            }
            logPlace(state, panel, context);
          })
          .withItemsText(fixedText)
          .withVisible([panel](const CommandState& state) {
            return placeForm(state, panel) == PlaceForm::Point && !state.object(kPlane).isEmpty();
          }),
      // Each reference with its distance on its line.
      first,
      valueInput(kOffset1, QObject::tr("Distance 1"), ValueKind::Length, QStringLiteral("0 mm"))
          .withManipulator(dimension(0))
          .withOnChange(pointMoved)
          .withBesidePrevious()
          .withVisible(onFace),
      second,
      valueInput(kOffset2, QObject::tr("Distance 2"), ValueKind::Length, QStringLiteral("0 mm"))
          .withManipulator(dimension(1))
          .withOnChange(pointMoved)
          .withBesidePrevious()
          .withVisible(onFace),
  };
  if (panel.along || panel.turns) {
    inputs.append(checkInput(kMore, QObject::tr("More"), false)
                      .withDisclosure()
                      .withVisible([isPlaced, alongShown, angleShown, alignShown, panel](const CommandState& state) {
                        // Only while something is folded away.
                        const bool along = panel.along && !alongShown(state);
                        const bool turns = panel.turns && panel.turns(state) && (!angleShown(state) || !alignShown(state));
                        return isPlaced(state) && (state.checked(kMore) || along || turns);
                      }));
  }
  if (panel.along) {
    inputs += {
        valueInput(kOffset, QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
            .withManipulator([panel](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
              // From where the part would start without it, along the axis.
              const auto frame = frameWithout(state, context, panel,
                                              {QStringLiteral("offset"), QStringLiteral("angle"), QStringLiteral("align")});
              if (!frame) {
                return std::nullopt;
              }
              const auto origin = pointIn(frame->value(QStringLiteral("origin")));
              const auto axis = dirOf(frame->value(QStringLiteral("axis")));
              if (!origin || !axis) {
                return std::nullopt;
              }
              Manipulator m = arrow(*origin, *axis);
              m.valueBox = true;
              m.boxAtZero = false;
              return m;
            })
            .withHandleWhenHidden()
            .withVisible(alongShown),
        flipInput(kFlip, QObject::tr("Flip")).withBesidePrevious().withVisible(alongShown),
    };
  }
  if (panel.turns) {
    // A line, or a planar face whose normal the x axis takes (a shaft's flat,
    // mitcad#656).
    InputDef align = selectionInput(kAlign, QObject::tr("Align to"), kLineKinds | SelectKind::Face, 0, 1)
                         .withAccepts([](const SelectionItem& item) {
                           return isLineItem(item) ||
                                  (item.kind == SelectKind::Face && item.geometry == QStringLiteral("plane"));
                         })
                         .withOnChange([](CommandState& state, const CommandContext&) {
                           // Picked or cleared by the user: no longer a shaft's flat,
                           // and a new line grows into the face.
                           state.setObject(kAlignAuto, QJsonObject());
                           state.setObject(kNoAlignSide, QJsonObject());
                           state.setObject(kNoAlign, QJsonObject());
                         })
                         .withVisible(alignShown);
    // Empty on a face it is Reference 1 (mitcad#656: never a hidden axis),
    // and its button says so in Reference 1's colour; an origin plane's
    // own x is the X axis, Reference 2. The user's edge or face in a
    // colour outside the panel's palette; cleared, Reference 1 again.
    align.itemsText = referenceText;
    align.emptyShows = [](const CommandState& state) -> std::pair<QString, QColor> {
      if (storedForm(state) != PlaceForm::Face || state.items(kAt).isEmpty()) {
        return {};
      }
      const auto sides = sidesOf(state);
      return sides && sides->implicit ? std::make_pair(QObject::tr("Reference 2"), kReference2Color)
                                      : std::make_pair(QObject::tr("Reference 1"), kReference1Color);
    };
    align.color = commandInputColor(1); // apart from the references' and the inactive pink (mitcad#1041)
    inputs += {
        valueInput(kAngle, QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0") + QChar(0x00B0))
            .withManipulator([panel, zeroShown](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
              // About the part's axis, from its zero along the reference;
              // only with a zero in sight.
              if (!panel.turns || !panel.turns(state) || !zeroShown(state)) {
                return std::nullopt;
              }
              const auto frame = frameWithout(state, context, panel, {QStringLiteral("angle")});
              if (!frame) {
                return std::nullopt;
              }
              const auto origin = pointIn(frame->value(QStringLiteral("origin")));
              const auto axis = dirOf(frame->value(QStringLiteral("axis")));
              const auto x = dirOf(frame->value(QStringLiteral("x_axis")));
              if (!origin || !axis || !x) {
                return std::nullopt;
              }
              Manipulator m = ring(*origin, *axis);
              m.reference = *x;
              m.valueBox = true;
              m.boxAtZero = false;
              return m;
            })
            .withHandleWhenHidden()
            .withVisible(angleShown),
        align,
    };
  }
  if (panel.clearance) {
    InputDef clearance =
        valueInput(kClearance, QObject::tr("Bore Clearance"), ValueKind::Length, QStringLiteral("0 mm"))
            .withVisible([panel](const CommandState& state) { return onShaft(state, panel); });
    if (!panel.printedFit.isEmpty()) {
      clearance.withPrintedFit(panel.printedFit);
    }
    inputs.append(clearance);
  }
  return inputs;
}

void initPlace(CommandState& state, const CommandContext& context, const PlacePanel& panel) {
  QString active = QStringLiteral("C0");
  try {
    active = context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
                 .value(QStringLiteral("active"))
                 .toString(QStringLiteral("C0"));
  } catch (...) {
  }
  state.setObject(kTurnsPanel, {{QStringLiteral("on"), static_cast<bool>(panel.turns)}});
  setContext(state, context, active, QString());
  const Selection& before = state.items(kAt);
  if (!before.isEmpty()) {
    // Selected before the command: placed as a click in its middle would.
    const SelectionItem item = before.first();
    if (formOfItem(item) == PlaceForm::None) {
      state.setItems(kAt, {});
    } else {
      placeItem(state, context, item, std::nullopt);
      logPlace(state, panel, context);
    }
  }
}

PlaceForm placeForm(const CommandState& state, const PlacePanel& panel) {
  if (!offeredIn(panel, state) || state.items(kAt).isEmpty()) {
    return PlaceForm::None;
  }
  return storedForm(state);
}

bool onShaft(const CommandState& state, const PlacePanel& panel) {
  if (placeForm(state, panel) != PlaceForm::Axis) {
    return false;
  }
  const SelectionItem item = placedItem(state);
  return item.kind == SelectKind::Face || item.kind == SelectKind::Edge;
}

void clearPlace(CommandState& state) {
  state.setItems(kAt, {});
  state.setItems(kEdge1, {});
  state.setItems(kOnPlane, {});
  state.setItems(kEdge2, {});
  for (const QString& key : {kItem, kForm, kPick, kSides, kPoint, kPlane, kLegacy, kNoAlignSide, kNoAlign}) {
    state.setObject(key, QJsonObject());
  }
}

QJsonObject atOf(const CommandState& state, const CommandContext& context, const PlacePanel& panel, bool numbers) {
  const PlaceForm form = placeForm(state, panel);
  if (form == PlaceForm::None) {
    return QJsonObject();
  }
  const auto valueOf = [&state, numbers](const QString& key) {
    return numbers ? QJsonValue(state.value(key)) : QJsonValue(state.expression(key));
  };
  const SelectionItem item = placedItem(state);
  // A shaft of another component is no `at` (mitcad#754): resolved in the
  // feature's component it would be in the wrong place. addPlace makes it
  // a seat with its link; placeFrame and shaftSeat ask `seat_frame`.
  if (form == PlaceForm::Axis && !linkOf(state, context, item).isEmpty()) {
    return QJsonObject();
  }
  QJsonObject at;
  const auto nearPick = state.object(kPick).value(QStringLiteral("near"));
  switch (form) {
  case PlaceForm::Axis:
    at.insert(QStringLiteral("axis"), item.reference());
    if (item.kind == SelectKind::Face && nearPick.isArray()) {
      at.insert(QStringLiteral("near"), nearPick);
    }
    break;
  case PlaceForm::Point: {
    at.insert(QStringLiteral("point"), item.reference());
    const QJsonValue plane = state.object(kPlane).value(QStringLiteral("ref"));
    if (!plane.isUndefined() && !plane.isNull()) {
      at.insert(QStringLiteral("plane"), plane);
    }
    break;
  }
  case PlaceForm::Face: {
    at.insert(QStringLiteral("face"), item.reference());
    const auto sides = sidesOf(state);
    if (!(sides && sides->implicit)) {
      if (!state.items(kEdge1).isEmpty()) {
        at.insert(QStringLiteral("edge1"), state.items(kEdge1).first().reference());
      }
      if (!state.items(kEdge2).isEmpty()) {
        at.insert(QStringLiteral("edge2"), state.items(kEdge2).first().reference());
      }
      if (item.kind != SelectKind::Face && nearPick.isArray()) {
        at.insert(QStringLiteral("near"), nearPick);
      }
    }
    at.insert(QStringLiteral("offset1"), valueOf(kOffset1));
    at.insert(QStringLiteral("offset2"), valueOf(kOffset2));
    break;
  }
  case PlaceForm::None:
    break;
  }
  if (nonZero(state, kOffset)) {
    at.insert(QStringLiteral("offset"), valueOf(kOffset));
  }
  if (state.checked(kFlip)) {
    at.insert(QStringLiteral("flip"), true);
  }
  // A turn given is kept, shown or not (a keyway taken away keeps its
  // pulley's teeth where they were).
  if (nonZero(state, kAngle)) {
    at.insert(QStringLiteral("angle"), valueOf(kAngle));
  }
  const auto sides = sidesOf(state);
  if (state.items(kAlign).isEmpty() && form == PlaceForm::Face && !state.items(kEdge1).isEmpty() &&
      !(sides && sides->implicit) && !state.object(kNoAlign).value(QStringLiteral("on")).toBool()) {
    // An empty Align on a body face is Reference 1 (mitcad#656); not on a
    // definition loaded without `align` (mitcad#761).
    at.insert(QStringLiteral("align"), state.items(kEdge1).first().reference());
  } else if (!state.items(kAlign).isEmpty()) {
    at.insert(QStringLiteral("align"), state.items(kAlign).first().reference());
  }
  // A line's own direction may run either way: the part's y axis toward
  // the face's middle, so that it grows into the face from a reference on
  // the far side too (mitcad#656; not for a definition written before).
  if (at.contains(QStringLiteral("align")) && form == PlaceForm::Face && sides && !sides->implicit &&
      !state.object(kNoAlignSide).value(QStringLiteral("on")).toBool()) {
    const SelectionItem line = state.items(kAlign).isEmpty() ? state.items(kEdge1).first() : state.items(kAlign).first();
    if (line.kind == SelectKind::Edge || line.kind == SelectKind::SketchCurve) {
      at.insert(QStringLiteral("align_side"), json(sides->first));
    }
  }
  // What an earlier definition had that the panel does not show.
  const QJsonObject extra = state.object(kExtra);
  for (auto it = extra.begin(); it != extra.end(); ++it) {
    if (!at.contains(it.key())) {
      at.insert(it.key(), it.value());
    }
  }
  return at;
}

void keepEarlierForm(CommandState& state, const CommandContext& context, const PlacePanel& panel,
                     const QJsonObject& fields) {
  state.setObject(kLegacy, {{QStringLiteral("fields"), fields}, {QStringLiteral("at"), atOf(state, context, panel)}});
}

QJsonObject keptEarlierForm(const CommandState& state, const CommandContext& context, const PlacePanel& panel) {
  const QJsonObject legacy = state.object(kLegacy);
  if (legacy.isEmpty() || placeForm(state, panel) == PlaceForm::None ||
      legacy.value(QStringLiteral("at")).toObject() != atOf(state, context, panel)) {
    return QJsonObject();
  }
  return legacy.value(QStringLiteral("fields")).toObject();
}

bool addPlace(QJsonObject& def, const CommandState& state, const CommandContext& context, const PlacePanel& panel,
              Built& error) {
  const PlaceForm form = placeForm(state, panel);
  if (form == PlaceForm::None) {
    if (panel.required && offeredIn(panel, state)) {
      error = Built::failure(QObject::tr("Pick where it goes: a shaft or a hole, an axis, a point or a face"), kAt);
      return false;
    }
    return true;
  }
  if (form == PlaceForm::Face && (state.items(kEdge1).isEmpty() || state.items(kEdge2).isEmpty())) {
    error = Built::failure(QObject::tr("Pick the line to measure from"),
                           state.items(kEdge1).isEmpty() ? kEdge1 : kEdge2);
    return false;
  }
  const SelectionItem item = placedItem(state);
  const QJsonObject link = form == PlaceForm::Axis ? linkOf(state, context, item) : QJsonObject();
  const QJsonObject earlier = keptEarlierForm(state, context, panel);
  if (!earlier.isEmpty()) {
    // The definition's own form, unchanged (mitcad#656).
    for (auto it = earlier.begin(); it != earlier.end(); ++it) {
      def.insert(it.key(), it.value());
    }
  } else if (!link.isEmpty()) {
    // Another component's shaft: a seat with its link (mitcad#578).
    QJsonObject seat{{QStringLiteral("shaft"), item.reference()}, {QStringLiteral("link"), link}};
    if (nonZero(state, kOffset)) {
      seat.insert(QStringLiteral("offset"), state.expression(kOffset));
    }
    if (state.checked(kFlip)) {
      seat.insert(QStringLiteral("flip"), true);
    }
    const QJsonValue nearPick = state.object(kPick).value(QStringLiteral("near"));
    if (item.kind == SelectKind::Face && nearPick.isArray()) {
      seat.insert(QStringLiteral("near"), nearPick);
    }
    def.insert(QStringLiteral("seat"), seat);
    if (nonZero(state, kAngle)) {
      def.insert(QStringLiteral("angle"), state.expression(kAngle));
    }
  } else {
    def.insert(QStringLiteral("at"), atOf(state, context, panel));
  }
  if (panel.clearance && onShaft(state, panel) && std::isfinite(state.value(kClearance))) {
    def.insert(QStringLiteral("bore_clearance"), state.expression(kClearance));
  }
  return true;
}

bool loadPlace(const QJsonObject& feature, CommandState& state, const CommandContext& context,
               const PlacePanel& panel, const QString& cornerKey) {
  QString component = QStringLiteral("C0");
  try {
    component = context.queryObject({{QStringLiteral("query"), QStringLiteral("feature")},
                                     {QStringLiteral("uid"), feature.value(QStringLiteral("uid"))}})
                    .value(QStringLiteral("component"))
                    .toString(QStringLiteral("C0"));
  } catch (...) {
  }
  const QJsonObject seat = feature.value(QStringLiteral("seat")).toObject();
  const QJsonValue link = seat.value(QStringLiteral("link"));
  const QString seen = setContext(state, context, component, link.toObject().value(QStringLiteral("target")).toString());
  clearPlace(state);
  state.setObject(kExtra, QJsonObject());
  // A definition keeps its own turn while it loads (no Reference 1 align).
  state.setObject(kTurnsPanel, QJsonObject());
  state.setObject(kAlignAuto, QJsonObject());
  state.setObject(kLegacy, QJsonObject());
  const auto ownItem = [&context, &seen](const QJsonValue& ref) {
    if (isFixedPlaneRef(ref)) {
      return placeItemOf(ref, context, seen); // a fixed `face` or `plane` (mitcad#1137)
    }
    SelectionItem item = itemOf(ref, context);
    if (item.kind == SelectKind::Face || item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex) {
      item.occurrence = seen;
    }
    return item;
  };
  const auto setPlaced = [&state](const SelectionItem& item, PlaceForm form) {
    state.setItems(kAt, {item});
    state.setObject(kItem, itemJson(item));
    state.setObject(kForm, {{QStringLiteral("form"), formName(form)}});
  };
  QJsonObject extra;
  const QJsonObject at = feature.value(QStringLiteral("at")).toObject();
  bool loaded = true;
  if (!at.isEmpty()) {
    const PlaceForm form = at.contains(QStringLiteral("axis"))    ? PlaceForm::Axis
                           : at.contains(QStringLiteral("point")) ? PlaceForm::Point
                                                                  : PlaceForm::Face;
    const SelectionItem item = ownItem(at.value(formName(form)));
    setPlaced(item, form);
    if (at.value(QStringLiteral("near")).isArray()) {
      state.setObject(kPick, {{QStringLiteral("near"), at.value(QStringLiteral("near"))}});
    }
    if (form == PlaceForm::Point && at.contains(QStringLiteral("plane"))) {
      state.setObject(kPlane, {{QStringLiteral("ref"), at.value(QStringLiteral("plane"))}});
      state.setItems(kOnPlane, {ownItem(at.value(QStringLiteral("plane")))});
    }
    if (form == PlaceForm::Face) {
      const bool implicit = !at.contains(QStringLiteral("edge1"));
      const auto info = faceInfo(item, context, false);
      SelectionItem first;
      SelectionItem second;
      if (implicit && info) {
        first = originAxis(axisAlong(info->frame.y), context);
        second = originAxis(axisAlong(info->frame.x), context);
      } else {
        first = ownItem(at.value(QStringLiteral("edge1")));
        second = ownItem(at.value(QStringLiteral("edge2")));
      }
      // The point where the model puts it, then the references' sides as
      // a pick would have them; the distances stay the definition's.
      QJsonObject flat = at;
      for (const char* key : {"offset", "flip", "angle", "align"}) {
        flat.remove(QLatin1String(key));
      }
      std::optional<gp_Pnt> point;
      try {
        point = pointIn(context.queryObject({{QStringLiteral("query"), QStringLiteral("place_frame")},
                                             {QStringLiteral("at"), flat}})
                            .value(QStringLiteral("origin")));
      } catch (...) {
      }
      if (point && first.isValid() && second.isValid()) {
        setReferences(state, context, item, first, second, *point, implicit,
                      pointIn(at.value(QStringLiteral("near"))), 1e-6);
      } else {
        state.setItems(kEdge1, first.isValid() ? Selection{first} : Selection());
        state.setItems(kEdge2, second.isValid() ? Selection{second} : Selection());
      }
      loadValue(state, kOffset1, at.value(QStringLiteral("offset1")), feature, context);
      loadValue(state, kOffset2, at.value(QStringLiteral("offset2")), feature, context);
      // Written before align_side: its line's direction as it is.
      if (!at.contains(QStringLiteral("align_side"))) {
        state.setObject(kNoAlignSide, {{QStringLiteral("on"), true}});
      }
      // On an origin plane from picked lines without an align: the part
      // turned from the plane's x, kept so as Align (mitcad#656).
      if (!implicit && info && info->origin && !at.contains(QStringLiteral("align")) && panel.turns) {
        state.setItems(kAlign, {originAxis(axisAlong(info->frame.x), context)});
      } else if (!implicit && !at.contains(QStringLiteral("align"))) {
        // Written without `align` (through the model's commands): saved
        // so again, not turned to Reference 1 (mitcad#761).
        state.setObject(kNoAlign, {{QStringLiteral("on"), true}});
      }
    }
    const auto keep = [&](const char* key, bool shown, const QString& input) {
      const QJsonValue slot = at.value(QLatin1String(key));
      if (slot.isUndefined() || slot.isNull()) {
        return;
      }
      if (shown) {
        loadValue(state, input, slot, feature, context);
      } else {
        extra.insert(QLatin1String(key), context.valueText(slot, feature.value(QStringLiteral("uid")).toString()));
      }
    };
    keep("offset", panel.along, kOffset);
    keep("angle", static_cast<bool>(panel.turns), kAngle);
    if (at.value(QStringLiteral("flip")).toBool()) {
      if (panel.along) {
        state.setChecked(kFlip, true);
      } else {
        extra.insert(QStringLiteral("flip"), true);
      }
    }
    if (at.contains(QStringLiteral("align"))) {
      if (panel.turns) {
        // Reference 1's line is an empty Align, kept following it.
        if (at.value(QStringLiteral("align")) != at.value(QStringLiteral("edge1"))) {
          state.setItems(kAlign, {ownItem(at.value(QStringLiteral("align")))});
        }
      } else {
        extra.insert(QStringLiteral("align"), at.value(QStringLiteral("align")));
      }
    }
  } else if (!seat.isEmpty()) {
    // A seat (mitcad#578): its shaft where it was picked, a linked one in
    // its occurrence; saved as `at` when it is of the feature's own
    // component.
    SelectionItem shaft = itemOf(seat.value(QStringLiteral("shaft")), context);
    shaft.occurrence = link.isObject() ? link.toObject().value(QStringLiteral("source")).toString() : seen;
    setPlaced(shaft, PlaceForm::Axis);
    if (seat.value(QStringLiteral("near")).isArray()) {
      state.setObject(kPick, {{QStringLiteral("near"), seat.value(QStringLiteral("near"))}});
    }
    loadValue(state, kOffset, seat.value(QStringLiteral("offset")), feature, context);
    state.setChecked(kFlip, seat.value(QStringLiteral("flip")).toBool());
  } else if (feature.contains(QStringLiteral("plane"))) {
    // A plane and a centre in its coordinates: the same point on the
    // plane, from its origin axes. A fixed plane (mitcad#1137) is shown as
    // it is and kept until a face or a plane is picked in its place.
    const SelectionItem plane = placeItemOf(feature.value(QStringLiteral("plane")), context, seen);
    const auto frame = frameOf(plane, context);
    const QJsonArray centre = feature.value(cornerKey).toArray();
    if (!plane.isValid() || !frame) {
      return false;
    }
    setPlaced(plane, PlaceForm::Face);
    // A coordinate is a number or, given as an expression, the feature's
    // parameter (mitcad#689): its value places the point.
    const QString uid = feature.value(QStringLiteral("uid")).toString();
    const auto coordinate = [&](const QJsonValue& slot) {
      const double value = evaluatedSlot(slot, uid, QStringLiteral("length"), context);
      return std::isfinite(value) ? value : 0.0;
    };
    const double x = coordinate(centre.at(0));
    const double y = coordinate(centre.at(1));
    const gp_Pnt point = frame->origin.Translated(gp_Vec(frame->x) * x + gp_Vec(frame->y) * y);
    placeOnFace(state, context, plane, point, false, Refs::OriginAxes, std::nullopt);
    // Moved on it, saved as `at` on the same plane without `align`: the
    // part keeps the plane's own x axis, as its centre had it. Reference 1
    // as Align would turn it onto that origin axis (mitcad#1140: by 180
    // degrees on a construction plane whose x runs along -X); a fixed
    // plane likewise (mitcad#1137).
    state.setObject(kNoAlign, {{QStringLiteral("on"), true}});
    // The distances show the coordinates' expressions where they are the
    // same distances (from the origin axes), negated across an axis.
    const std::pair<QString, double> distances[] = {{kOffset1, x}, {kOffset2, y}};
    for (int i = 0; i < 2; ++i) {
      const QJsonValue slot = centre.at(i);
      if (!slot.isString()) {
        continue;
      }
      const double shown = state.value(distances[i].first);
      if (std::abs(shown - distances[i].second) <= 1e-6) {
        loadValue(state, distances[i].first, slot, feature, context);
      } else if (std::abs(shown + distances[i].second) <= 1e-6) {
        state.setText(distances[i].first,
                      negated(context.valueText(slot, feature.value(QStringLiteral("uid")).toString())));
      }
    }
  } else {
    loaded = false;
  }
  // A turn of an earlier definition beside its placement.
  if (loaded && !feature.contains(QStringLiteral("at"))) {
    const QJsonValue angle = feature.value(QStringLiteral("angle"));
    if (!angle.isUndefined() && !angle.isNull()) {
      if (panel.turns) {
        loadValue(state, kAngle, angle, feature, context);
      } else {
        extra.insert(QStringLiteral("angle"), context.valueText(angle, feature.value(QStringLiteral("uid")).toString()));
      }
    }
  }
  state.setObject(kExtra, extra);
  loadValue(state, kClearance, feature.value(QStringLiteral("bore_clearance")), feature, context);
  // Picks from here on turn the part from Reference 1 by default.
  state.setObject(kTurnsPanel, {{QStringLiteral("on"), static_cast<bool>(panel.turns)}});
  if (loaded && !feature.contains(QStringLiteral("at"))) {
    // An earlier form is saved as it was until the placement changes.
    QJsonObject fields;
    if (!seat.isEmpty() && !link.isObject()) {
      fields.insert(QStringLiteral("seat"), seat);
    } else if (seat.isEmpty() && feature.contains(QStringLiteral("plane"))) {
      fields.insert(QStringLiteral("plane"), feature.value(QStringLiteral("plane")));
      fields.insert(cornerKey, feature.value(cornerKey));
    }
    if (!fields.isEmpty()) {
      if (feature.contains(QStringLiteral("angle"))) {
        fields.insert(QStringLiteral("angle"), feature.value(QStringLiteral("angle")));
      }
      keepEarlierForm(state, context, panel, fields);
    }
  }
  if (loaded) {
    logPlace(state, panel, context);
  }
  return loaded;
}

void logPlacement(const CommandState& state, const CommandContext& context, const PlacePanel& panel) {
  logPlace(state, panel, context);
}

bool placeOnPlaneAt(CommandState& state, const CommandContext& context, const QJsonValue& plane,
                    const gp_Pnt& point) {
  SelectionItem item = itemOf(plane, context);
  if (formOfItem(item) != PlaceForm::Face) {
    return false;
  }
  if (item.kind == SelectKind::Face) {
    item.occurrence = seenPath(state);
  }
  clearPlace(state);
  state.setItems(kAt, {item});
  state.setObject(kItem, itemJson(item));
  state.setObject(kForm, {{QStringLiteral("form"), formName(PlaceForm::Face)}});
  return placeOnFace(state, context, item, point, false, Refs::OriginAxes, std::nullopt);
}

bool placeShowable(const QJsonObject& feature, const CommandContext& context) {
  const auto showable = [&context](const QJsonValue& ref) {
    return ref.isUndefined() || ref.isNull() || itemOf(ref, context).isValid();
  };
  // A fixed plane (`{"origin", "normal"}`, as agents and importers write
  // it) is shown as an item of its own (mitcad#1137) where a plane is
  // placed on; a fixed axis or point is not shown.
  const auto planeShowable = [&showable](const QJsonValue& ref) { return isFixedPlaneRef(ref) || showable(ref); };
  const QJsonObject at = feature.value(QStringLiteral("at")).toObject();
  for (const char* key : {"axis", "point", "edge1", "edge2", "align"}) {
    if (!showable(at.value(QLatin1String(key)))) {
      return false;
    }
  }
  return planeShowable(at.value(QStringLiteral("face"))) && planeShowable(at.value(QStringLiteral("plane"))) &&
         showable(feature.value(QStringLiteral("seat")).toObject().value(QStringLiteral("shaft"))) &&
         planeShowable(feature.value(QStringLiteral("plane")));
}

std::optional<PlaneFrame> placeFrame(const CommandState& state, const CommandContext& context,
                                     const PlacePanel& panel) {
  const PlaceForm form = placeForm(state, panel);
  if (form == PlaceForm::None) {
    return std::nullopt;
  }
  QJsonObject answer;
  try {
    const SelectionItem item = placedItem(state);
    const QJsonObject link = form == PlaceForm::Axis ? linkOf(state, context, item) : QJsonObject();
    if (!link.isEmpty()) {
      QJsonObject def;
      Built ignored;
      addPlace(def, state, context, panel, ignored);
      answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("seat_frame")},
                                    {QStringLiteral("seat"), def.value(QStringLiteral("seat"))}});
    } else {
      answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("place_frame")},
                                    {QStringLiteral("at"), atOf(state, context, panel, true)}});
    }
  } catch (...) {
    return std::nullopt;
  }
  const auto origin = pointIn(answer.value(QStringLiteral("origin")));
  const auto axis = dirOf(answer.value(QStringLiteral("axis")));
  if (!origin || !axis) {
    return std::nullopt;
  }
  PlaneFrame frame;
  frame.origin = *origin;
  frame.normal = *axis;
  frame.x = dirOf(answer.value(QStringLiteral("x_axis"))).value_or(defaultX(*axis));
  frame.y = dirOf(answer.value(QStringLiteral("y_axis"))).value_or(axis->Crossed(frame.x));
  return frame;
}

std::optional<std::pair<QJsonValue, QJsonArray>> planeCenterOf(const CommandState& state,
                                                              const CommandContext& context) {
  if (placeForm(state) != PlaceForm::Face) {
    return std::nullopt;
  }
  const SelectionItem item = placedItem(state);
  const auto sides = sidesOf(state);
  if (sides && sides->implicit && std::isfinite(state.value(kOffset1)) && std::isfinite(state.value(kOffset2))) {
    return std::make_pair(item.reference(), QJsonArray{state.value(kOffset1), state.value(kOffset2)});
  }
  const auto info = faceInfo(item, context, false);
  const auto point = facePoint(state, context);
  if (!info || !point) {
    return std::nullopt;
  }
  const gp_Vec offset(info->frame.origin, *point);
  return std::make_pair(item.reference(), QJsonArray{roundTo(offset.Dot(gp_Vec(info->frame.x)), 1e-6),
                                                     roundTo(offset.Dot(gp_Vec(info->frame.y)), 1e-6)});
}

double referenceAngle(const CommandState& state, const CommandContext& context) {
  const auto sides = sidesOf(state);
  if (placeForm(state) != PlaceForm::Face || !sides || sides->implicit) {
    return 0.0;
  }
  const auto info = faceInfo(placedItem(state), context, false);
  if (!info) {
    return 0.0;
  }
  // Along the line, its positive side (toward the point) to the left.
  const gp_Vec along = gp_Vec(sides->first).Crossed(gp_Vec(info->frame.normal));
  if (along.Magnitude() < 1e-9) {
    return 0.0;
  }
  return std::atan2(along.Dot(gp_Vec(info->frame.y)), along.Dot(gp_Vec(info->frame.x)));
}

QString shaftNote(const QJsonObject& values, const CommandContext& context) {
  if (!values.contains(QStringLiteral("shaft_diameter")) || !values.contains(QStringLiteral("bore_diameter")) ||
      !values.value(QStringLiteral("shaft_diameter")).isDouble()) {
    return QString();
  }
  return QObject::tr("bore %1 (shaft %2)")
      .arg(lengthText(values.value(QStringLiteral("bore_diameter")).toDouble(), context),
           lengthText(values.value(QStringLiteral("shaft_diameter")).toDouble(), context));
}

QJsonObject shaftSeat(const CommandState& state, const CommandContext& context, const PlacePanel& panel) {
  if (!onShaft(state, panel)) {
    return QJsonObject();
  }
  QJsonObject def;
  Built ignored;
  if (!addPlace(def, state, context, panel, ignored)) {
    return QJsonObject();
  }
  try {
    if (def.contains(QStringLiteral("seat"))) {
      return context.queryObject({{QStringLiteral("query"), QStringLiteral("seat_frame")},
                                  {QStringLiteral("seat"), def.value(QStringLiteral("seat"))}});
    }
    return context.queryObject({{QStringLiteral("query"), QStringLiteral("place_frame")},
                                {QStringLiteral("at"), atOf(state, context, panel, true)}})
        .value(QStringLiteral("seat"))
        .toObject();
  } catch (...) {
    return QJsonObject();
  }
}

QJsonObject occurrenceLinkOf(const CommandContext& context, const SelectionItem& item) {
  if (item.kind != SelectKind::Face && item.kind != SelectKind::Edge) {
    return QJsonObject();
  }
  QJsonObject components;
  try {
    components = context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
  } catch (...) {
    return QJsonObject();
  }
  // As linkOf for a panel seen where the active component is (initPlace).
  const QString active = components.value(QStringLiteral("active")).toString(QStringLiteral("C0"));
  const QJsonArray occurrences = components.value(QStringLiteral("occurrences")).toArray();
  const QString component = componentAt(occurrences, item.occurrence);
  if (component.isEmpty() || component == active) {
    return QJsonObject();
  }
  QJsonObject link{{QStringLiteral("source"), item.occurrence}};
  if (active != QStringLiteral("C0")) {
    const QString target = pathTo(occurrences, QString(), active);
    if (!target.isEmpty()) {
      link.insert(QStringLiteral("target"), target);
    }
  }
  return link;
}

// ---------------------------------------------------------------------------
// Where earlier definitions put their part

double evaluatedSlot(const QJsonValue& slot, const QString& owner, const QString& kind, const CommandContext& context) {
  if (slot.isDouble()) {
    return slot.toDouble();
  }
  if (slot.isUndefined() || slot.isNull()) {
    return std::numeric_limits<double>::quiet_NaN();
  }
  try {
    return context.queryObject({{QStringLiteral("query"), QStringLiteral("evaluate")},
                                {QStringLiteral("expression"), context.valueText(slot, owner)},
                                {QStringLiteral("kind"), kind}})
        .value(QStringLiteral("value"))
        .toDouble(std::numeric_limits<double>::quiet_NaN());
  } catch (...) {
    return std::numeric_limits<double>::quiet_NaN();
  }
}

std::optional<BuiltFrame> builtFrame(const QJsonObject& def, const QString& uid, const CommandContext& context,
                                     int depth) {
  if (depth > 8) {
    return std::nullopt;
  }
  BuiltFrame frame;
  frame.plane = QJsonValue();
  const auto fromAnswer = [&frame](const QJsonObject& answer) {
    const auto origin = pointIn(answer.value(QStringLiteral("origin")));
    const auto axis = dirOf(answer.value(QStringLiteral("axis")));
    if (!origin || !axis) {
      return false;
    }
    frame.origin = *origin;
    frame.normal = *axis;
    frame.x = dirOf(answer.value(QStringLiteral("x_axis"))).value_or(defaultX(*axis));
    return true;
  };
  // Beside its mate, at a distance in a direction from the mate's x axis.
  const auto besideMate = [&](const QString& mate, double distance, double angle) -> std::optional<BuiltFrame> {
    QJsonObject mateDef;
    try {
      mateDef = context.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), mate}})
                    .value(QStringLiteral("def"))
                    .toObject();
    } catch (...) {
      return std::nullopt;
    }
    const auto mateFrame = builtFrame(mateDef, mate, context, depth + 1);
    if (!mateFrame || !std::isfinite(distance)) {
      return std::nullopt;
    }
    BuiltFrame placed = *mateFrame;
    const gp_Dir y = mateFrame->normal.Crossed(mateFrame->x);
    const double turn = std::isfinite(angle) ? angle : 0.0;
    placed.origin = mateFrame->origin.Translated(
        (gp_Vec(mateFrame->x) * std::cos(turn) + gp_Vec(y) * std::sin(turn)) * distance);
    return placed;
  };
  try {
    if (def.contains(QStringLiteral("at"))) {
      if (!fromAnswer(context.queryObject({{QStringLiteral("query"), QStringLiteral("place_frame")},
                                           {QStringLiteral("at"), def.value(QStringLiteral("at"))}}))) {
        return std::nullopt;
      }
      return frame;
    }
    if (def.contains(QStringLiteral("seat"))) {
      if (!fromAnswer(context.queryObject({{QStringLiteral("query"), QStringLiteral("seat_frame")},
                                           {QStringLiteral("seat"), def.value(QStringLiteral("seat"))}}))) {
        return std::nullopt;
      }
      return frame;
    }
  } catch (...) {
    return std::nullopt;
  }
  if (def.contains(QStringLiteral("plane"))) {
    const auto plane = frameOf(placeItemOf(def.value(QStringLiteral("plane")), context), context);
    if (!plane) {
      return std::nullopt;
    }
    // Numbers, or expressions of the feature's parameters (mitcad#689).
    const QJsonArray centre = def.value(QStringLiteral("center")).toArray();
    const double u = centre.at(0).isUndefined() ? 0.0 : evaluatedSlot(centre.at(0), uid, QStringLiteral("length"), context);
    const double v = centre.at(1).isUndefined() ? 0.0 : evaluatedSlot(centre.at(1), uid, QStringLiteral("length"), context);
    if (!std::isfinite(u) || !std::isfinite(v)) {
      return std::nullopt;
    }
    frame.origin = plane->origin.Translated(gp_Vec(plane->x) * u + gp_Vec(plane->y) * v);
    frame.normal = plane->normal;
    frame.x = plane->x;
    frame.plane = def.value(QStringLiteral("plane"));
    return frame;
  }
  const QJsonObject belt = def.value(QStringLiteral("belt_with")).toObject();
  if (!belt.isEmpty()) {
    return besideMate(belt.value(QStringLiteral("pulley")).toString(),
                      evaluatedSlot(belt.value(QStringLiteral("center_distance")), uid, QStringLiteral("length"), context),
                      evaluatedSlot(belt.value(QStringLiteral("angle")), uid, QStringLiteral("angle"), context));
  }
  const QJsonObject mesh = def.value(QStringLiteral("mesh_with")).toObject();
  if (!mesh.isEmpty() && !mesh.contains(QStringLiteral("position")) && !mesh.contains(QStringLiteral("toward"))) {
    double distance = std::numeric_limits<double>::quiet_NaN();
    try {
      distance = context.queryObject({{QStringLiteral("query"), QStringLiteral("gear_values")}, {QStringLiteral("uid"), uid}})
                     .value(QStringLiteral("centre_distance"))
                     .toDouble(std::numeric_limits<double>::quiet_NaN());
    } catch (...) {
    }
    return besideMate(mesh.value(QStringLiteral("gear")).toString(), distance,
                      evaluatedSlot(mesh.value(QStringLiteral("angle")), uid, QStringLiteral("angle"), context));
  }
  return std::nullopt;
}

namespace {

// The origin plane square to a normal, when one is.
QString originPlaneAlong(const gp_Dir& normal) {
  const QString axis = axisAlong(normal);
  const double along = axis == QStringLiteral("x")   ? std::abs(normal.X())
                       : axis == QStringLiteral("y") ? std::abs(normal.Y())
                                                     : std::abs(normal.Z());
  if (along < 1.0 - 1e-9) {
    return QString();
  }
  return axis == QStringLiteral("x") ? QStringLiteral("yz") : axis == QStringLiteral("y") ? QStringLiteral("xz")
                                                                                          : QStringLiteral("xy");
}

} // namespace

bool placeAtFrame(CommandState& state, const CommandContext& context, const BuiltFrame& frame) {
  SelectionItem plane = frame.plane.isUndefined() || frame.plane.isNull()
                            ? SelectionItem()
                            : placeItemOf(frame.plane, context, seenPath(state));
  double along = 0.0;
  bool flip = false;
  if (!plane.isValid()) {
    const QString name = originPlaneAlong(frame.normal);
    if (name.isEmpty()) {
      return false;
    }
    plane = itemOf(QJsonValue(name), context);
    const auto planeFrame = frameOf(plane, context);
    if (!planeFrame) {
      return false;
    }
    const double distance = gp_Vec(planeFrame->origin, frame.origin).Dot(gp_Vec(planeFrame->normal));
    flip = gp_Vec(frame.normal).Dot(gp_Vec(planeFrame->normal)) < 0.0;
    along = flip ? -distance : distance;
  }
  clearPlace(state);
  state.setItems(kAt, {plane});
  state.setObject(kItem, itemJson(plane));
  state.setObject(kForm, {{QStringLiteral("form"), formName(PlaceForm::Face)}});
  if (!placeOnFace(state, context, plane, frame.origin, false, Refs::OriginAxes, std::nullopt)) {
    return false;
  }
  setLength(state, kOffset, roundTo(along, 1e-6), context);
  state.setChecked(kFlip, flip);
  return true;
}

QJsonObject atOfFeature(const QJsonObject& def, const QString& uid, const CommandContext& context) {
  if (def.value(QStringLiteral("at")).isObject()) {
    return def.value(QStringLiteral("at")).toObject();
  }
  const QJsonObject seat = def.value(QStringLiteral("seat")).toObject();
  if (!seat.isEmpty() && !seat.contains(QStringLiteral("link"))) {
    QJsonObject at{{QStringLiteral("axis"), seat.value(QStringLiteral("shaft"))}};
    for (const char* key : {"offset", "flip", "near"}) {
      if (seat.contains(QLatin1String(key))) {
        at.insert(QLatin1String(key), seat.value(QLatin1String(key)));
      }
    }
    return at;
  }
  const auto frame = builtFrame(def, uid, context);
  if (!frame) {
    return QJsonObject();
  }
  // On the origin plane square to its axis, at the distances along the
  // plane's x and y.
  const QString name = originPlaneAlong(frame->normal);
  if (name.isEmpty()) {
    return QJsonObject();
  }
  const auto plane = frameOf(itemOf(QJsonValue(name), context), context);
  if (!plane) {
    return QJsonObject();
  }
  const gp_Vec offset(plane->origin, frame->origin);
  const double distance = offset.Dot(gp_Vec(plane->normal));
  const bool flip = gp_Vec(frame->normal).Dot(gp_Vec(plane->normal)) < 0.0;
  QJsonObject at{{QStringLiteral("face"), name},
                 {QStringLiteral("offset1"), roundTo(offset.Dot(gp_Vec(plane->x)), 1e-9)},
                 {QStringLiteral("offset2"), roundTo(offset.Dot(gp_Vec(plane->y)), 1e-9)}};
  if (std::abs(distance) > 1e-9) {
    at.insert(QStringLiteral("offset"), flip ? -distance : distance);
  }
  if (flip) {
    at.insert(QStringLiteral("flip"), true);
  }
  return at;
}

} // namespace mitcad::cmd
